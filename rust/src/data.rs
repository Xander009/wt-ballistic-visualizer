//! Reads the files extractor/extract.py and extractor/tank_models.py write into ../data. Each is a JS call
//! `NAME(arg, arg, ...);` whose arguments are JSON, so the argument list is parsed as one JSON array.

use anyhow::{Context as _, Result, bail, ensure};
use base64::Engine as _;
use serde::Deserialize;
use serde_json::Value;

use crate::core::{Grid, Shell, Terrain, TerrainMeta};
pub use crate::store::Store;

fn call_args(store: &Store, rel: &str, name: &str) -> Result<Vec<Value>> {
    let s = store.read(rel)?;
    let s = s.trim();
    let body = s.strip_prefix(name).and_then(|r| r.strip_prefix('(')).with_context(|| format!("{rel} is not a {name}(...) file"))?;
    let body = body.trim_end_matches(';').trim_end().strip_suffix(')').context("unterminated call")?;
    Ok(serde_json::from_str(&format!("[{body}]"))?)
}

fn b64(s: &str) -> Result<Vec<u8>> {
    Ok(base64::engine::general_purpose::STANDARD.decode(s)?)
}
fn inflate(bytes: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut out = vec![];
    flate2::read::ZlibDecoder::new(bytes).read_to_end(&mut out)?;
    Ok(out)
}
/// A zlib + base64 payload, or "@<entry>": the raw bytes kept as their own entry (lite bundle).
fn payload(store: &Store, s: &str) -> Result<Vec<u8>> {
    match s.strip_prefix('@') {
        Some(rel) => store.bytes(rel),
        None => inflate(&b64(s)?),
    }
}
/// A data URL, or "@<entry>": the encoded image as its own entry (lite bundle).
fn image_of(store: &Store, s: &str) -> Result<image::RgbaImage> {
    match s.strip_prefix('@') {
        Some(rel) => Ok(image::load_from_memory(&store.bytes(rel)?)?.to_rgba8()),
        None => data_url_image(s),
    }
}
/// `data:<mime>;base64,<...>` -> decoded image
fn data_url_image(url: &str) -> Result<image::RgbaImage> {
    let (_, data) = url.split_once(";base64,").context("not a base64 data URL")?;
    Ok(image::load_from_memory(&b64(data)?)?.to_rgba8())
}

// ------------------------------------------------------------------ index

#[derive(Deserialize, Clone)]
pub struct Gun {
    pub weapon: String,
    pub pitch: [f64; 2],
    pub shells: Vec<Shell>,
    /// the largest angle a shot leaves the bore line by, horizontal and vertical, degrees (modules researched)
    #[serde(default)]
    pub disp: Option<[f64; 2]>,
}
#[derive(Deserialize, Clone)]
pub struct Tank {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub model: Option<String>,
    pub guns: Vec<Gun>,
    /// how much wider the spread is, horizontal and vertical, while its fire-control modules are not researched
    /// (Adjustment of Fire, Elevation Mechanism)
    #[serde(default)]
    pub stock: Option<[f64; 2]>,
}
#[derive(Deserialize, Clone)]
pub struct MapEntry {
    pub name: String,
    pub width: usize,
    pub cell: f64,
    #[serde(default)]
    pub objects: bool,
    #[serde(default)]
    pub ground: bool,
    /// The whole level around the tank map (Max build only): maps/<level>.far.js and its objects and ground,
    /// plus maps/<level>.groups.obj.js, the missions' object groups (airfields, heli pads, props)
    #[serde(default)]
    pub far: Option<FarEntry>,
}
#[derive(Deserialize, Clone)]
pub struct FarEntry {
    pub name: String,
    #[serde(default)]
    pub objects: bool,
    #[serde(default)]
    pub ground: bool,
    #[serde(default)]
    pub groups: bool,
}
#[derive(Deserialize)]
pub struct Index {
    pub maps: Vec<MapEntry>,
    pub tanks: Vec<Tank>,
}

pub fn load_index(store: &Store) -> Result<Index> {
    let mut a = call_args(store, "index.js", "WTV_INDEX")?;
    ensure!(!a.is_empty(), "empty index.js");
    Ok(serde_json::from_value(a.remove(0))?)
}

// ------------------------------------------------------------------ map

#[derive(Deserialize, Clone, Debug)]
pub struct Area {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub team: Option<i64>,
    #[serde(rename = "type")]
    pub shape: String,
    pub tm: [[f64; 3]; 4],
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    /// the battle ranks it is in (an import with a rankRange: low or high rank airfields, heli pads); None = all
    #[serde(default)]
    pub rank: Option<[u32; 2]>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct Mission {
    pub name: String,
    pub areas: Vec<Area>,
    /// anti-aircraft guns (airfield and heli pad defences): name, unit class, x, z, lowest and highest rank
    #[serde(default)]
    pub aa: Vec<(String, String, f64, f64, u32, u32)>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct MapMeta {
    pub name: String,
    #[serde(default)]
    pub bounds: Option<[f64; 4]>,
    pub width: usize,
    pub height: usize,
    pub cell: f64,
    pub ox: f64,
    pub oz: f64,
    pub hmin: f64,
    pub hmax: f64,
    pub missions: Vec<Mission>,
    #[serde(default)]
    pub water: Option<f64>,
}
impl MapMeta {
    pub fn terrain_meta(&self) -> TerrainMeta {
        TerrainMeta { width: self.width, height: self.height, cell: self.cell, ox: self.ox, oz: self.oz, hmin: self.hmin, hmax: self.hmax }
    }
}

pub fn load_map(store: &Store, name: &str) -> Result<(MapMeta, Vec<u16>)> {
    let mut a = call_args(store, &format!("maps/{name}.js"), "WTV_MAP")?;
    let mut obj = a.pop().context("empty map file")?;
    let h = obj.as_object_mut().and_then(|o| o.remove("h")).and_then(|v| v.as_str().map(str::to_owned)).context("map has no heightmap")?;
    let meta: MapMeta = serde_json::from_value(obj)?;
    let mut raw = payload(store, &h)?;
    if h.ends_with(crate::store::PREDICTED) {
        crate::store::unpredict(&mut raw, meta.width);
    }
    ensure!(raw.len() == meta.width * meta.height * 2, "heightmap size mismatch");
    let h16 = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    Ok((meta, h16))
}

// ------------------------------------------------------------------ objects (WTVO v2, see extractor/objects main.rs)

#[derive(Clone)]
pub struct Model {
    pub pos: Vec<f32>,
    pub nrm: Vec<i8>,  // 4 per vertex
    pub rgba: Vec<u8>, // 4 per vertex
    pub idx: Vec<u32>,
}
#[derive(Clone)]
pub struct TreeModel {
    pub height: f32,
    pub radius: f32,
    pub crown: f32,
    pub rgba: [u8; 4],
}
pub struct Objects {
    pub roof: Grid,
    pub veg: Grid,
    pub models: Vec<Model>,
    /// per instance: model index, then 12 floats (rows of a Dagor tm: x axis, y axis, z axis, position)
    pub inst: Vec<(u32, [f32; 12])>,
    pub tree_models: Vec<TreeModel>,
    /// per tree: tree model index, x, y, z, scale
    pub trees: Vec<(u32, [f32; 4])>,
    /// object groups only: the placement each instance and each tree belongs to
    pub tags: Option<(Vec<u32>, Vec<u32>)>,
}

impl Objects {
    /// The instances and trees whose tag `keep` accepts (all when untagged); models are shared by index.
    pub fn subset(&self, keep: impl Fn(u32) -> bool) -> Objects {
        let ok = |tags: Option<&Vec<u32>>, i: usize| tags.is_none_or(|t| keep(t[i]));
        Objects {
            roof: self.roof.clone(),
            veg: self.veg.clone(),
            models: self.models.clone(),
            inst: self.inst.iter().enumerate().filter(|(i, _)| ok(self.tags.as_ref().map(|t| &t.0), *i)).map(|(_, x)| *x).collect(),
            tree_models: self.tree_models.clone(),
            trees: self.trees.iter().enumerate().filter(|(i, _)| ok(self.tags.as_ref().map(|t| &t.1), *i)).map(|(_, x)| *x).collect(),
            tags: None,
        }
    }
}

struct Rd<'a> {
    b: &'a [u8],
    o: usize,
}
impl Rd<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        ensure!(self.o + n <= self.b.len(), "objects file truncated");
        let s = &self.b[self.o..self.o + n];
        self.o += n;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn align4(&mut self) {
        self.o = (self.o + 3) & !3;
    }
    fn f32s(&mut self, n: usize) -> Result<Vec<f32>> {
        Ok(self.take(n * 4)?.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect())
    }
}

pub fn parse_objects(buf: &[u8]) -> Result<Objects> {
    let mut r = Rd { b: buf, o: 0 };
    let (magic, version) = (r.u32()?, r.u32()?);
    ensure!(magic == 0x4f56_5457 && (version == 2 || version == 3), "not a WTVO v2/v3 file: re-run extractor/extract.py");
    let (w, h) = (r.u32()? as usize, r.u32()? as usize);
    let grid = |r: &mut Rd| -> Result<Grid> {
        let (base, scale) = (r.f32()?, r.f32()?);
        let q = r.take(w * h * 2)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        r.align4();
        Ok(Grid { base, scale, q })
    };
    let roof = grid(&mut r)?;
    let veg = grid(&mut r)?;
    let mut models = vec![];
    for _ in 0..r.u32()? {
        let len = r.u32()? as usize;
        r.take(len)?;
        r.align4();
        let (nv, ni) = (r.u32()? as usize, r.u32()? as usize);
        let pos = if version == 3 {
            // v3 (lite bundle): u16 positions over the model's bounding box, see store::quantize_objects
            let (lo, size) = (r.f32s(3)?, r.f32s(3)?);
            let q = r.take(nv * 6)?.chunks_exact(2).enumerate().map(|(i, c)| lo[i % 3] + u16::from_le_bytes([c[0], c[1]]) as f32 / 65535.0 * size[i % 3]).collect();
            r.align4();
            q
        } else {
            r.f32s(nv * 3)?
        };
        let nrm = r.take(nv * 4)?.iter().map(|&b| b as i8).collect();
        let rgba = r.take(nv * 4)?.to_vec();
        let idx = r.take(ni * 4)?.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        models.push(Model { pos, nrm, rgba, idx });
    }
    let ni = r.u32()? as usize;
    let raw = r.take(ni * 52)?;
    let inst: Vec<(u32, [f32; 12])> = raw
        .chunks_exact(52)
        .map(|c| {
            let m = u32::from_le_bytes(c[0..4].try_into().unwrap());
            let mut f = [0f32; 12];
            for (k, v) in f.iter_mut().enumerate() {
                *v = f32::from_le_bytes(c[4 + k * 4..8 + k * 4].try_into().unwrap());
            }
            (m, f)
        })
        .collect();
    let mut tree_models = vec![];
    for _ in 0..r.u32()? {
        let (height, radius, crown) = (r.f32()?, r.f32()?, r.f32()?);
        let rgba = r.take(4)?.try_into()?;
        tree_models.push(TreeModel { height, radius, crown, rgba });
    }
    let nt = r.u32()? as usize;
    let trees: Vec<(u32, [f32; 4])> = r
        .take(nt * 20)?
        .chunks_exact(20)
        .map(|c| {
            let f = |k: usize| f32::from_le_bytes(c[k * 4..k * 4 + 4].try_into().unwrap());
            (u32::from_le_bytes(c[0..4].try_into().unwrap()), [f(1), f(2), f(3), f(4)])
        })
        .collect();
    // object groups: u32 n, tag[n] per instance, then per tree (see extractor/objects main.rs)
    let tags = if r.o < buf.len() {
        let mut list = || -> Result<Vec<u32>> { let n = r.u32()? as usize; Ok(r.take(n * 4)?.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()) };
        let (ti, tt) = (list()?, list()?);
        ensure!(ti.len() == inst.len() && tt.len() == trees.len(), "object group tags do not match");
        Some((ti, tt))
    } else {
        None
    };
    Ok(Objects { roof, veg, models, inst, tree_models, trees, tags })
}

pub fn load_objects(store: &Store, name: &str) -> Result<Objects> {
    let a = call_args(store, &format!("maps/{name}.obj.js"), "WTV_OBJ")?;
    let b = a.get(1).and_then(Value::as_str).context("objects file without data")?;
    let o = parse_objects(&payload(store, b)?)?;
    Ok(o)
}

/// Max build: what a level's missions place (maps/<level>.groups.obj.js, see extract.py group_placements).
pub struct Groups {
    /// the object groups' models and trees, tagged by placement
    pub obj: Objects,
    /// per mission name, the placements it uses: placement, lowest and highest rank of the battles it is in
    pub missions: std::collections::HashMap<String, Vec<(u32, u32, u32)>>,
    /// the ground slabs of airfields and heli pads
    pub slabs: Vec<SlabPlace>,
    /// spots aircraft ("plane") and helicopters ("heli") spawn on: placement, kind, x, y, z
    pub spawns: Vec<(u32, String, f64, f64, f64)>,
    /// per placement, the height of its group's origin in the mission (often far off the ground)
    pub origins: Vec<f32>,
}
impl Groups {
    /// Sets every placement on the ground `t`, as the game does: each object, tree and spawn spot moves by the
    /// ground under it less its group's origin height, keeping its height over the group.
    pub fn settle(&mut self, t: &Terrain) {
        let o = &self.origins;
        let dy = |tag: u32, x: f32, z: f32| o.get(tag as usize).map_or(0.0, |&y0| t.height_at(x as f64, z as f64) as f32 - y0);
        if let Some((ti, tt)) = &self.obj.tags {
            for ((_, m), &tag) in self.obj.inst.iter_mut().zip(ti) {
                m[10] += dy(tag, m[9], m[11]);
            }
            for ((_, f), &tag) in self.obj.trees.iter_mut().zip(tt) {
                f[1] += dy(tag, f[0], f[2]);
            }
        }
        for s in &mut self.spawns {
            s.3 += f64::from(dy(s.0, s.2 as f32, s.4 as f32));
        }
    }
}
#[derive(Deserialize)]
pub struct SlabPlace {
    pub tag: u32,
    pub model: String,
    /// Dagor tm, rows: x, y, z axis, position
    pub tm: [f32; 12],
    /// the landmesh around the slab, finer than the whole level's grid
    pub tile: Tile,
}
#[derive(Deserialize)]
pub struct Tile {
    pub x0: f64,
    pub z0: f64,
    pub cell: f64,
    pub w: usize,
    pub h: usize,
    pub lo: f64,
    pub hi: f64,
    q: String,
}
impl Tile {
    /// Heights, row j = z index j.
    pub fn heights(&self, store: &Store) -> Result<Vec<f32>> {
        let raw = payload(store, &self.q)?;
        ensure!(raw.len() == self.w * self.h * 2, "tile size does not match");
        let sc = (self.hi - self.lo) / 65535.0;
        Ok(raw.chunks_exact(2).map(|c| (self.lo + f64::from(u16::from_le_bytes([c[0], c[1]])) * sc) as f32).collect())
    }
}

/// The missions' object groups of a level.
pub fn load_groups(store: &Store, name: &str) -> Result<Groups> {
    let a = call_args(store, &format!("maps/{name}.obj.js"), "WTV_OBJ")?;
    let b = a.get(1).and_then(Value::as_str).context("objects file without data")?;
    let obj = parse_objects(&payload(store, b)?)?;
    ensure!(obj.tags.is_some(), "{name}: not an object group file");
    let extra = a.get(2).context("object groups without missions")?;
    let field = |k: &str| extra.get(k).cloned().unwrap_or(Value::Array(vec![]));
    Ok(Groups {
        obj,
        missions: serde_json::from_value(extra.get("missions").cloned().context("object groups without missions")?)?,
        slabs: serde_json::from_value(field("slabs"))?,
        spawns: serde_json::from_value(field("spawns"))?,
        origins: serde_json::from_value(field("origins"))?,
    })
}

/// A ground slab model (slabs/<model>.js, see extractor/objects main.rs): textured triangles in its own frame.
pub struct Slab {
    pub parts: Vec<SlabPart>,
    /// per texture its mip chain (each level half the last): tiled concrete repeats every few hundred metres and
    /// is drawn at 0.5 to 2 m, where single texels would only be noise
    pub tex: Vec<Vec<image::RgbaImage>>,
}
pub struct SlabPart {
    pub tex: usize,
    /// how it paints the ground: 0 by the texture's alpha, 1 opaque
    pub blend: u32,
    /// x, y, z, u, v
    pub v: Vec<[f32; 5]>,
    pub idx: Vec<u32>,
}

pub fn load_slab(store: &Store, model: &str) -> Result<Slab> {
    let a = call_args(store, &format!("slabs/{model}.js"), "WTV_SLAB")?;
    let d = a.get(1).context("slab without data")?;
    let raw = payload(store, d.get("mesh").and_then(Value::as_str).context("slab without mesh")?)?;
    let images = |k: &str| -> Result<Vec<image::RgbaImage>> { d.get(k).and_then(Value::as_array).with_context(|| format!("slab without {k}"))?.iter().map(|t| image_of(store, t.as_str().unwrap_or(""))).collect() };
    let mut tex = images("tex")?;
    // the decal mask, kept as a grey image of its own: into each texture's alpha
    for (t, a) in tex.iter_mut().zip(images("alpha")?) {
        ensure!(a.dimensions() == t.dimensions(), "slab mask size does not match");
        t.pixels_mut().zip(a.pixels()).for_each(|(p, q)| p.0[3] = q.0[0]);
    }
    let tex: Vec<Vec<image::RgbaImage>> = tex
        .into_iter()
        .map(|t| {
            let mut chain = vec![t];
            while let Some(l) = chain.last().filter(|l| l.width() > 1 && l.height() > 1) {
                let next = image::imageops::resize(l, l.width() / 2, l.height() / 2, image::imageops::FilterType::Triangle);
                chain.push(next);
            }
            chain
        })
        .collect();
    let mut r = Rd { b: &raw, o: 0 };
    let mut parts = vec![];
    for _ in 0..r.u32()? {
        let t = r.u32()? as usize;
        ensure!(t < tex.len(), "slab part without its texture");
        let blend = r.u32()?;
        let nv = r.u32()? as usize;
        let v = r.take(nv * 20)?.chunks_exact(20).map(|c| std::array::from_fn(|k| f32::from_le_bytes(c[k * 4..k * 4 + 4].try_into().unwrap()))).collect();
        let ni = r.u32()? as usize;
        let idx: Vec<u32> = r.take(ni * 4)?.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        ensure!(idx.iter().all(|&i| (i as usize) < nv), "slab index out of range");
        parts.push(SlabPart { tex: t, blend, v, idx });
    }
    Ok(Slab { parts, tex })
}

/// Max build: the whole level around tank map `level` (its far entry) and the missions' object groups.
pub fn load_far(store: &Store, level: &str, f: &FarEntry) -> Result<(LoadedMap, Option<Groups>)> {
    let (meta, h16) = load_map(store, &f.name)?;
    let mut m = LoadedMap::new(meta, h16);
    if f.objects {
        m.set_objects(load_objects(store, &f.name)?)?;
    }
    if f.ground {
        m.ground = Some(std::sync::Arc::new(load_ground(store, &f.name)?));
    }
    let groups = if f.groups { Some(load_groups(store, &format!("{level}.groups"))?) } else { None };
    Ok((m, groups))
}

// ------------------------------------------------------------------ ground colours

/// idx: which detail texture each ground texel uses (red channel, 255 none); strip: the detail textures side by side.
pub struct Detail {
    pub idx: image::RgbaImage,
    pub strip: image::RgbaImage,
    pub tiles: Vec<f32>,
}
pub struct Ground {
    pub color: image::RgbaImage,
    pub detail: Option<Detail>,
}

pub fn load_ground(store: &Store, name: &str) -> Result<Ground> {
    let a = call_args(store, &format!("maps/{name}.tex.js"), "WTV_TEX")?;
    let color = image_of(store, a.get(1).and_then(Value::as_str).context("ground file without image")?)?;
    let detail = match a.get(2) {
        Some(Value::Object(d)) => Some(Detail {
            idx: image_of(store, d.get("idx").and_then(Value::as_str).context("detail idx")?)?,
            strip: image_of(store, d.get("strip").and_then(Value::as_str).context("detail strip")?)?,
            tiles: serde_json::from_value(d.get("tiles").cloned().context("detail tiles")?)?,
        }),
        _ => None,
    };
    Ok(Ground { color, detail })
}

// ------------------------------------------------------------------ tank models (WTVK v1, see extractor/tank_models.py)

/// Flat-shaded triangles: 3 floats position and 3 floats normal per vertex, +X front, +Y up, lowest point y = 0.
pub struct TankMesh {
    pub pos: Vec<f32>,
    pub nrm: Vec<f32>,
}

pub fn load_tank(store: &Store, model: &str) -> Result<TankMesh> {
    let a = call_args(store, &format!("tanks/{model}.js"), "WTV_TANK")?;
    let buf = payload(store, a.get(1).and_then(Value::as_str).context("tank file without data")?)?;
    let mut r = Rd { b: &buf, o: 0 };
    if r.u32()? != 0x4b56_5457 || r.u32()? != 1 {
        bail!("not a WTVK v1 file");
    }
    let (nv, nt) = (r.u32()? as usize, r.u32()? as usize);
    let q: Vec<f32> = r.take(nv * 6)?.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 1000.0).collect();
    let idx: Vec<usize> = r.take(nt * 6)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as usize).collect();
    let (mut pos, mut nrm) = (Vec::with_capacity(nt * 9), Vec::with_capacity(nt * 9));
    for t in idx.chunks_exact(3) {
        ensure!(t.iter().all(|&i| i < nv), "tank index out of range");
        let p: Vec<[f32; 3]> = t.iter().map(|&i| [q[i * 3], q[i * 3 + 1], q[i * 3 + 2]]).collect();
        let u = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let w = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
        let n = [u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0]];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
        for v in &p {
            pos.extend_from_slice(v);
            nrm.extend(n.iter().map(|c| c / l));
        }
    }
    Ok(TankMesh { pos, nrm })
}

/// A loaded map with the surfaces the page builds: t = bare ground (positions of A, B), tr = ground + roofs,
/// tv = ground + roofs + crowns. tc / ts (shots / sight) pick tr or tv by the two "block" switches.
#[derive(Clone)]
pub struct LoadedMap {
    pub meta: MapMeta,
    pub h16: Vec<u16>,
    pub t: Terrain,
    pub tr: Terrain,
    pub tv: Terrain,
    pub obj: Option<std::sync::Arc<Objects>>,
    pub ground: Option<std::sync::Arc<Ground>>,
    /// Max build: the whole level around this map, view only (its own objects and ground)
    pub far: Option<std::sync::Arc<LoadedMap>>,
    /// Max build: what the missions place on the level
    pub groups: Option<std::sync::Arc<Groups>>,
    /// Max build: the ground of the airfields and heli pads every mission places, finer than `far`
    pub patches: Vec<std::sync::Arc<Patch>>,
}

/// The ground under and around an airfield or heli pad slab: its own grid, and its colours (row 0 = +Z edge).
pub struct Patch {
    /// the slab's placement (Groups::missions)
    pub tag: u32,
    pub t: std::sync::Arc<Terrain>,
    pub img: image::RgbaImage,
}

impl LoadedMap {
    pub fn new(meta: MapMeta, h16: Vec<u16>) -> Self {
        let t = Terrain::new(&meta.terrain_meta(), &h16, &[]);
        Self { tr: t.clone(), tv: t.clone(), t, meta, h16, obj: None, ground: None, far: None, groups: None, patches: vec![] }
    }
    /// Where a tank at (x, z) stands: the ground, or the top of a rock or other solid object there.
    pub fn stand(&self, x: f64, z: f64) -> f64 {
        self.tr.height_at(x, z)
    }
    pub fn set_objects(&mut self, o: Objects) -> Result<()> {
        ensure!(o.roof.q.len() == self.meta.width * self.meta.height, "object grid does not match the heightmap");
        let tm = self.meta.terrain_meta();
        self.tr = Terrain::new(&tm, &self.h16, &[&o.roof]);
        self.tv = Terrain::new(&tm, &self.h16, &[&o.roof, &o.veg]);
        self.obj = Some(std::sync::Arc::new(o));
        Ok(())
    }
    /// Max build: the whole level and the airfield and heli pad patches around this map. Its surfaces then
    /// reach over their bare ground (A and B can stand there; the level's roofs and crowns are on a 32 m grid, far
    /// too coarse to block with); after set_objects, which builds the surfaces anew.
    pub fn set_far(&mut self, far: LoadedMap, patches: Vec<Patch>) {
        let patches: Vec<_> = patches.into_iter().map(std::sync::Arc::new).collect();
        let outer = Some(std::sync::Arc::new(crate::core::Outer { patches: patches.iter().map(|p| p.t.clone()).collect(), far: std::sync::Arc::new(far.t.clone()) }));
        (self.t.outer, self.tr.outer, self.tv.outer) = (outer.clone(), outer.clone(), outer);
        self.far = Some(std::sync::Arc::new(far));
        self.patches = patches;
    }
    /// Max build, once the groups have settled: the airfields' and heli pads' buildings as solid blocks on their
    /// patches, in the surfaces shots and sight meet and A and B stand on (tr, tv; t stays bare ground).
    // shortcut: every mission's placements block, not only the selected one's; they lie apart (an airfield per
    // rank span), so only a shot across an airfield the mission leaves out meets them. Per mission if that matters.
    pub fn set_group_roofs(&mut self) {
        let (Some(g), Some(o)) = (&self.groups, &self.t.outer) else { return };
        let patches = self.patches.iter().map(|p| std::sync::Arc::new(crate::raster::roofed(p, &g.obj))).collect();
        let outer = Some(std::sync::Arc::new(crate::core::Outer { patches, far: o.far.clone() }));
        (self.tr.outer, self.tv.outer) = (outer.clone(), outer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_real_data() {
        let dir = Store::open();
        let Ok(idx) = load_index(&dir) else { return }; // no extracted data in this checkout
        assert!(idx.maps.len() > 10 && idx.tanks.len() > 1000);
        let m = &idx.maps[0];
        let (meta, h16) = load_map(&dir, &m.name).unwrap();
        assert_eq!(h16.len(), meta.width * meta.height);
        if m.objects {
            let o = load_objects(&dir, &m.name).unwrap();
            assert!(!o.models.is_empty() && !o.inst.is_empty());
        }
        if m.ground {
            assert!(load_ground(&dir, &m.name).unwrap().color.width() > 0);
        }
        let t = load_tank(&dir, "t_72a").unwrap();
        assert!(t.pos.len() > 1000);
    }

    #[test]
    fn quantized_objects_match() {
        let store = Store::open();
        let Ok(a) = call_args(&store, "maps/avg_normandy.obj.js", "WTV_OBJ") else { return };
        let v2 = inflate(&b64(a[1].as_str().unwrap()).unwrap()).unwrap();
        let (o2, o3) = (parse_objects(&v2).unwrap(), parse_objects(&crate::store::quantize_objects(&v2).unwrap()).unwrap());
        assert_eq!((o2.models.len(), o2.inst.len(), o2.trees.len()), (o3.models.len(), o3.inst.len(), o3.trees.len()));
        assert_eq!(o2.roof.q, o3.roof.q);
        for (m2, m3) in o2.models.iter().zip(&o3.models) {
            assert_eq!((m2.idx.len(), &m2.nrm, &m2.rgba), (m3.idx.len(), &m3.nrm, &m3.rgba));
            for (p2, p3) in m2.pos.iter().zip(&m3.pos) {
                assert!((p2 - p3).abs() < 0.01, "{p2} vs {p3}");
            }
        }
    }

    #[test]
    fn groups_settle_on_the_ground() {
        let none = Grid { base: 0.0, scale: 1.0, q: vec![] };
        let mut tm = [0.0f32; 12];
        tm[9..].copy_from_slice(&[50.0, 61.0, 50.0]); // 1 m over its group, whose origin is 60 m up
        let obj = Objects { roof: none.clone(), veg: none, models: vec![], inst: vec![(0, tm)], tree_models: vec![], trees: vec![(0, [50.0, 60.0, 50.0, 1.0])], tags: Some((vec![0], vec![0])) };
        let mut g = Groups { obj, missions: Default::default(), slabs: vec![], spawns: vec![(0, "heli".into(), 50.0, 60.0, 50.0)], origins: vec![60.0] };
        g.settle(&Terrain::grid(vec![10.0; 4], 2, 2, 100.0, 0.0, 0.0, None));
        assert_eq!((g.obj.inst[0].1[10], g.obj.trees[0].1[1], g.spawns[0].3), (11.0, 10.0, 10.0));
    }
}
