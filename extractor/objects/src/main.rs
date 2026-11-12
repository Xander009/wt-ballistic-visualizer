//! Exports the placed objects, trees and ground colours of War Thunder levels for the visualizer.
//!
//! `wtv-objects <game dir> <jobs file>`. Each line of the jobs file is
//! `<level> <out base> <ox> <oz> <cell> <width> <height>`: the level name, where to write, and the heightmap
//! grid the browser uses. Writes `<out base>.obj` and, when the level has land-class data, `<out base>.ground`.
//!
//! `.obj`, little endian, every block 4-byte aligned:
//! ```text
//! "WTVO" u32 version=2
//! u32 w, u32 h
//! f32 base, f32 scale, u16 roof[w*h] (+ pad)   top of shot-blocking objects per grid point, q = 0: none
//! f32 base, f32 scale, u16 veg[w*h]  (+ pad)   top of tree and bush crowns per grid point, q = 0: none
//!     height = base + q * scale
//! u32 models; per model: u32 name_len, name (+ pad), u32 verts, u32 indices,
//!     f32 pos[verts*3], i8 normal[verts*4], u8 rgba[verts*4], u32 idx[indices]      (object space, Dagor axes)
//! u32 instances; per instance: u32 model, f32 tm[12] (rows: x axis, y axis, z axis, position)
//! u32 tree models; per model: f32 height, f32 radius, f32 crown_base, u8 rgba[4]  (unscaled)
//! u32 trees; per tree: u32 model, f32 x, y, z, scale
//! ```
//! `.ground`: u32 w, u32 h, rgba[w*h], row 0 at the smallest z: the land classes' colour maps blended by
//! their weights, over the heightmap's extent. Then u8 detail[w*h] (which detail texture, 255 none, see
//! bake_details) and u32 n; per detail texture: f32 tile metres, u32 side, u8 luminance[side*side] (mean 128).
//! Tree models' rgba[3] is the crown shape: 0 broadleaf, 1 conifer, 2 palm, 3 bush.
//!
//! Shot collision (`roof`): objects whose bullet-traceable collision is at least 2.5 m tall and 6 m² in plan,
//! flattened to the highest collision surface per grid point, so they act as solid columns.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use wt_formats::grp::GrpPack;
use wt_unpack::blk::{Block, Value};
use wt_unpack::collision::{Collision, CollisionNode, NodeKind, COLLISION};
use wt_unpack::landclass::{decode_colormaps, GroundBaker, GroundMaterial, GroundWeights, LandClassKind, LandClassMaterial};
use wt_unpack::texlib::Rgba;
use wt_unpack::level::Level;
use wt_unpack::rendinst::{is_facing_leaves, model_descs, ResourceIndex, RiModel, LAND_CLASS, REND_INST};
use wt_unpack::texlib::TextureLibrary;
use wt_unpack::vehicle;

const MIN_HEIGHT: f32 = 2.5;
const MIN_AREA: f32 = 6.0;
/// Objects smaller than this in every direction (pebbles, litter) are not drawn
const MIN_DRAWN: f32 = 0.5;
const GRID_SCALE: f32 = 0.05;
/// Render LOD: the one the game uses at this distance, coarser while over the triangle budget
const LOD_DISTANCE: f32 = 60.0;
const MAX_TRIS: usize = 6000;
/// Ground colour image: at most this many pixels per side, at most this fine
const GROUND_PX: f32 = 4096.0;
const GROUND_MIN_TEXEL: f32 = 0.5;

type Tri = [[f32; 3]; 3];
type Tm = [[f32; 3]; 4];

struct Mesh {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    rgba: Vec<[u8; 4]>,
    idx: Vec<u32>,
}

/// What an object model is, decided once per name
enum Kind {
    /// Drawn with `mesh`; `shot` = bullet-traceable collision triangles (object space)
    Object { mesh: Option<usize>, shot: Vec<Tri>, drawn: bool },
    /// A tree or bush: its index in the tree model table
    Tree(usize),
    Skip,
}

const SHAPE_BROADLEAF: u8 = 0;
const SHAPE_CONIFER: u8 = 1;
const SHAPE_PALM: u8 = 2;
const SHAPE_BUSH: u8 = 3;

/// Crown shape of a tree model, from its name (the game's own names: liveoak, picea, pine, palm, bush_...)
fn tree_shape(name: &str, height: f32) -> u8 {
    let n = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    if has(&["palm", "cocos", "phoenix", "washingtonia"]) {
        SHAPE_PALM
    } else if has(&["pine", "pinus", "fir", "abies", "spruce", "picea", "cedar", "cedrus", "larch", "larix", "juniper", "cypress", "cupress", "thuja", "taxus", "sequoia", "conifer", "tsuga"]) {
        SHAPE_CONIFER
    } else if height < 4.0 || has(&["bush", "shrub", "hedge"]) {
        SHAPE_BUSH
    } else {
        SHAPE_BROADLEAF
    }
}

struct TreeModel {
    height: f32,
    radius: f32,
    crown_base: f32,
    rgba: [u8; 4],
}

struct Ctx {
    index: ResourceIndex,
    textures: TextureLibrary,
    descs: HashMap<String, Block>,
    tex_colour: HashMap<String, Option<[u8; 4]>>,
    kinds: HashMap<String, Kind>,
    meshes: Vec<Mesh>,
    trees: Vec<TreeModel>,
}

fn resource(index: &ResourceIndex, class: u32, name: &str) -> Result<Option<Vec<u8>>> {
    let Some(path) = index.find(class, name) else { return Ok(None) };
    let file = vehicle::map(path)?;
    let pack = GrpPack::parse(&file).map_err(|e| anyhow::anyhow!("{}: {e:?}", path.display()))?;
    Ok(pack.find(name, class).map(|r| pack.data(r).to_vec()))
}

fn box_tris(n: &CollisionNode) -> Vec<Tri> {
    let [lo, hi] = n.bbox;
    let c = |i: usize| [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    quads.iter().flat_map(|q| [[c(q[0]), c(q[1]), c(q[2])], [c(q[0]), c(q[2]), c(q[3])]]).collect()
}

/// Collision triangles in object space: (bullet-traceable, all)
fn collision_tris(index: &ResourceIndex, name: &str) -> Result<(Vec<Tri>, Vec<Tri>)> {
    let resource_name = format!("{name}_collision");
    let Some(data) = resource(index, COLLISION, &resource_name)? else { return Ok((Vec::new(), Vec::new())) };
    let c = Collision::parse(&resource_name, &data).with_context(|| format!("parsing {resource_name}"))?;
    let (mut shot, mut all) = (Vec::new(), Vec::new());
    for n in &c.nodes {
        let local: Vec<Tri> = match n.kind {
            NodeKind::Box => box_tris(n),
            NodeKind::Mesh | NodeKind::Convex => n.indices.chunks_exact(3).map(|t| [0, 1, 2].map(|k| n.positions[t[k] as usize])).collect(),
            _ => continue,
        };
        let obj: Vec<Tri> = local.into_iter().map(|t| t.map(|p| n.to_object(p))).collect();
        if n.traceable() {
            shot.extend_from_slice(&obj);
        }
        all.extend(obj);
    }
    Ok((shot, all))
}

fn str_param<'a>(b: &'a Block, key: &str) -> Option<&'a str> {
    b.params.iter().find(|(n, _)| n == key).and_then(|(_, v)| if let Value::Str(s) = v { Some(s.as_str()) } else { None })
}

/// Material class and colour texture of each material of a model (riDesc.bin), as scene.rs reads them
fn materials(desc: Option<&Block>) -> Vec<(String, Option<String>)> {
    let Some(desc) = desc else { return Vec::new() };
    let tex: Vec<&str> = desc
        .blocks
        .iter()
        .find(|b| b.name == "tex")
        .map(|b| b.params.iter().filter_map(|(_, v)| if let Value::Str(s) = v { Some(s.as_str()) } else { None }).collect())
        .unwrap_or_default();
    let Some(mats) = desc.blocks.iter().find(|b| b.name == "mat") else { return Vec::new() };
    mats.blocks
        .iter()
        .map(|m| {
            let class = str_param(m, "cls").unwrap_or("").to_string();
            let colour = m.params.iter().find(|(n, _)| n == "t0").and_then(|(_, v)| if let Value::Int(i) = v { tex.get(*i as usize) } else { None }).map(|s| s.to_string());
            (class, colour)
        })
        .collect()
}

impl Ctx {
    /// Average colour of a texture (sRGB). `foliage`: a leaf atlas, which also holds bark and branches: when at
    /// least a quarter of its opaque pixels look like leaves (green at least red), only those count.
    fn colour(&mut self, name: &str, foliage: bool) -> Option<[u8; 4]> {
        let key = format!("{name}|{foliage}");
        if let Some(c) = self.tex_colour.get(&key) {
            return *c;
        }
        // 64 px and only clearly opaque pixels: a smaller shrink mixes the dark colour under a leaf texture's
        // transparent parts into the leaves (Ardennes crowns came out near black)
        let c = self.textures.decode(name, 64).ok().flatten().and_then(|img| {
            let (mut acc, mut wsum) = ([0f64; 3], 0f64);
            let opaque = img.rgba.chunks_exact(4).any(|p| p[3] > 160);
            let solid = |p: &[u8]| !opaque || p[3] > 160;
            let leafy = |p: &[u8]| p[1] >= p[0];
            let n_solid = img.rgba.chunks_exact(4).filter(|p| solid(p)).count();
            let only_leaves = foliage && img.rgba.chunks_exact(4).filter(|p| solid(p) && leafy(p)).count() * 4 >= n_solid;
            for p in img.rgba.chunks_exact(4) {
                let w = f64::from(u8::from(solid(p) && (!only_leaves || leafy(p))));
                for k in 0..3 {
                    acc[k] += f64::from(p[k]) * w;
                }
                wsum += w;
            }
            (wsum > 0.0).then(|| [(acc[0] / wsum) as u8, (acc[1] / wsum) as u8, (acc[2] / wsum) as u8, 255])
        });
        self.tex_colour.insert(key, c);
        c
    }

    fn classify(&mut self, name: &str) -> Kind {
        let model = resource(&self.index, REND_INST, name).ok().flatten().and_then(|d| RiModel::parse(&d).ok());
        let mats = materials(self.descs.get(name));
        let leafy = model.as_ref().is_some_and(|m| m.has_impostor) || mats.iter().any(|(c, _)| is_facing_leaves(c));
        if leafy {
            let Some(m) = model.as_ref() else { return Kind::Skip };
            if std::env::var_os("WTV_DEBUG_TREES").is_some() {
                if let Some(d) = self.descs.get(name) {
                    for mb in d.blocks.iter().filter(|b| b.name == "mat").flat_map(|b| &b.blocks) {
                        eprintln!("  tree {name} mat {:?}", mb.params.iter().map(|(k, v)| format!("{k}={v:?}")).collect::<Vec<_>>().join(" ").chars().take(400).collect::<String>());
                    }
                }
            }
            let (height, radius) = match &m.impostor_data {
                Some(i) if i.max_y > 0.0 && i.cyl_rad > 0.0 => (i.max_y, i.cyl_rad),
                _ => (m.bbox[1][1], 0.25 * ((m.bbox[1][0] - m.bbox[0][0]) + (m.bbox[1][2] - m.bbox[0][2]))),
            };
            if height < 0.3 {
                return Kind::Skip; // grass tufts
            }
            // the leaves' material: facing leaf cards, else the colored/leaves tree class (rendinst_tree_colored);
            // plain rendinst_tree is the trunk and branches (bark)
            let rank = |c: &str| if is_facing_leaves(c) { 0 } else if c.contains("colored") || c.contains("leaves") { 1 } else if c.contains("tree") { 2 } else { 3 };
            let leaf_tex = mats.iter().filter(|(c, t)| t.is_some() && rank(c) < 3).min_by_key(|(c, _)| rank(c));
            let mut rgba = leaf_tex.and_then(|(_, t)| t.clone()).and_then(|t| self.colour(&t, true)).unwrap_or([70, 95, 50, 255]);
            // The game tints rendinst_tree_colored leaves at run time (the tint is not in the files), so their
            // textures are dark: lift to a foliage brightness keeping the hue (approximation, at most 3x)
            let peak = rgba[0].max(rgba[1]).max(rgba[2]).max(1);
            if peak < 110 {
                let k = (110.0 / f32::from(peak)).min(3.0);
                for c in &mut rgba[..3] {
                    *c = (f32::from(*c) * k).min(255.0) as u8;
                }
            }
            rgba[3] = tree_shape(name, height); // the alpha byte carries the crown shape
            let crown_base = match rgba[3] {
                SHAPE_BUSH => 0.0,
                SHAPE_PALM => 0.8 * height,
                SHAPE_CONIFER => 0.15 * height,
                _ => 0.35 * height,
            };
            self.trees.push(TreeModel { height, radius, crown_base, rgba });
            return Kind::Tree(self.trees.len() - 1);
        }
        let (shot, all) = collision_tris(&self.index, name).unwrap_or_else(|e| {
            eprintln!("  {name}: {e:#}");
            (Vec::new(), Vec::new())
        });
        let mut mesh = model.as_ref().and_then(|m| self.render_mesh(m, &mats));
        if mesh.is_none() && !all.is_empty() {
            mesh = Some(Mesh {
                pos: all.iter().flatten().copied().collect(),
                nrm: all.iter().flat_map(|t| { let n = face_normal(t); [n, n, n] }).collect(),
                rgba: vec![[150, 145, 135, 255]; all.len() * 3],
                idx: (0..all.len() as u32 * 3).collect(),
            });
        }
        if mesh.is_none() && shot.is_empty() {
            return Kind::Skip;
        }
        let ext = mesh.as_ref().map_or(0.0, |m| {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in &m.pos {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            (0..3).map(|k| hi[k] - lo[k]).fold(0.0, f32::max)
        });
        let id = mesh.map(|m| {
            self.meshes.push(m);
            self.meshes.len() - 1
        });
        Kind::Object { mesh: id, shot, drawn: ext >= MIN_DRAWN }
    }

    fn render_mesh(&mut self, model: &RiModel, mats: &[(String, Option<String>)]) -> Option<Mesh> {
        let last = if model.has_impostor { model.lods.len().checked_sub(2)? } else { model.lods.len().checked_sub(1)? };
        let mut lod = model.lod_for_distance(LOD_DISTANCE).min(last);
        while lod < last && model.triangles(lod) > MAX_TRIS {
            lod += 1;
        }
        let mut m = Mesh { pos: Vec::new(), nrm: Vec::new(), rgba: Vec::new(), idx: Vec::new() };
        let mut remap: HashMap<(usize, usize, u32), u32> = HashMap::new();
        for e in &model.lods.get(lod)?.elems {
            let (class, tex) = mats.get(e.material).cloned().unwrap_or_default();
            if class.contains("decal") || class.contains("baked_impostor") || is_facing_leaves(&class) {
                continue;
            }
            let rgba = tex.and_then(|t| self.colour(&t, false)).unwrap_or([150, 145, 135, 255]);
            let vd = &model.vdata[e.vdata];
            let Some(range) = vd.indices.get(e.first_index..e.first_index + e.triangles * 3) else { continue };
            let mut tri = Vec::with_capacity(3);
            for &raw in range {
                let v = raw as usize + e.base_vertex;
                let key = (e.material, e.vdata, v as u32);
                let i = match remap.get(&key) {
                    Some(&i) => i,
                    None => {
                        let Ok(vx) = model.vertex(vd, v) else { break };
                        m.pos.push(vx.pos);
                        m.nrm.push(vx.normal);
                        m.rgba.push(rgba);
                        let i = (m.pos.len() - 1) as u32;
                        remap.insert(key, i);
                        i
                    }
                };
                tri.push(i);
                if tri.len() == 3 {
                    m.idx.extend_from_slice(&tri);
                    tri.clear();
                }
            }
        }
        (!m.idx.is_empty()).then_some(m)
    }
}

fn face_normal(t: &Tri) -> [f32; 3] {
    let (a, b) = ([0, 1, 2].map(|k| t[1][k] - t[0][k]), [0, 1, 2].map(|k| t[2][k] - t[0][k]));
    let n = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
    n.map(|x| x / l)
}

fn apply(tm: &Tm, p: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|k| p[0] * tm[0][k] + p[1] * tm[1][k] + p[2] * tm[2][k] + tm[3][k])
}

struct Grid {
    ox: f32,
    oz: f32,
    cell: f32,
    w: usize,
    h: usize,
}

impl Grid {
    fn at(&self, x: f32, z: f32) -> Option<usize> {
        let (gx, gz) = (((x - self.ox) / self.cell).round(), ((z - self.oz) / self.cell).round());
        (gx >= 0.0 && gz >= 0.0 && gx < self.w as f32 && gz < self.h as f32).then(|| gz as usize * self.w + gx as usize)
    }
}

/// Highest point of every grid cell (centred on its grid point) the triangle covers, sampled at half a cell
fn rasterize(g: &Grid, t: &Tri, top: &mut [f32]) {
    let [a, b, c] = *t;
    let len = |p: [f32; 3], q: [f32; 3]| ((p[0] - q[0]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
    let span = len(a, b).max(len(b, c)).max(len(a, c)).max((a[1] - b[1]).abs()).max((a[1] - c[1]).abs());
    let n = ((span / (g.cell * 0.5)).ceil() as usize).clamp(1, 4000);
    for i in 0..=n {
        for j in 0..=n - i {
            let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
            let p = [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * u + (c[k] - a[k]) * v);
            if let Some(k) = g.at(p[0], p[2]) {
                top[k] = top[k].max(p[1]);
            }
        }
    }
}

/// A tree crown: a disc of `r` around (x, z) up to `y`
fn rasterize_disc(g: &Grid, x: f32, z: f32, r: f32, y: f32, top: &mut [f32]) {
    let n = (r / g.cell).ceil() as i32;
    let (cx, cz) = (((x - g.ox) / g.cell).round() as i32, ((z - g.oz) / g.cell).round() as i32);
    for dz in -n..=n {
        for dx in -n..=n {
            let (gx, gz) = (cx + dx, cz + dz);
            if gx < 0 || gz < 0 || gx >= g.w as i32 || gz >= g.h as i32 {
                continue;
            }
            let (wx, wz) = (g.ox + gx as f32 * g.cell - x, g.oz + gz as f32 * g.cell - z);
            if wx * wx + wz * wz <= r * r + g.cell * g.cell * 0.25 {
                let k = gz as usize * g.w + gx as usize;
                top[k] = top[k].max(y);
            }
        }
    }
}

const MAX_DETAILS: usize = 32;
const DETAIL_PX: u32 = 256;

/// Which tiled detail texture the game paints at each ground texel (255: none), and those textures as
/// luminance normalised to a mean of 128, with their tile size in metres. Per texel: the land class with
/// the largest weight; in a mega class its splat map picks the Red/Green/Blue/Black layer, other classes
/// use their first layer. Texels as in GroundBaker::bake (row 0 at the smallest z).
#[allow(clippy::too_many_arguments)]
fn bake_details(ctx: &mut Ctx, classes: &[Option<LandClassMaterial>], weights: &GroundWeights, corner: [f32; 2], rect: [f32; 4], texel: f32, w: u32, h: u32, level: &str) -> (Vec<u8>, Vec<(f32, u32, Vec<u8>)>) {
    // mega classes: splat map (R, G, B, rest = Black layer); indexed classes: index map (R = which `detail`
    // entry of the class's `details` block, whose tex_a names the texture). Both placed like the colour map.
    let splats: Vec<Option<Rgba>> = classes
        .iter()
        .map(|c| {
            let c = c.as_ref()?;
            let name = match c.kind {
                LandClassKind::Mega => c.splat_map.as_deref()?,
                LandClassKind::Indexed => c.extra_textures.iter().find(|(k, _)| k == "indices").map(|(_, t)| t.as_str())?,
                _ => return None,
            };
            ctx.textures.decode(name, 4096).ok().flatten()
        })
        .collect();
    // indexed classes: index value -> texture (detail block) name
    let index_tex: Vec<Vec<Option<String>>> = classes
        .iter()
        .map(|c| {
            let Some(c) = c.as_ref().filter(|c| c.kind == LandClassKind::Indexed) else { return Vec::new() };
            let Some(details) = c.raw.blocks.iter().find(|b| b.name == "details") else { return Vec::new() };
            details
                .blocks
                .iter()
                .filter(|b| b.name == "detail")
                .map(|b| b.params.iter().find(|(k, _)| k == "tex_a").and_then(|(_, v)| if let Value::Str(s) = v { Some(s.clone()) } else { None }))
                .collect()
        })
        .collect();
    if std::env::var_os("WTV_DEBUG").is_some() {
        for c in classes.iter().flatten() {
            eprintln!("  class {} {:?} splat {:?} extra {:?}", c.name, c.kind, c.splat_map, c.extra_textures);
            for l in &c.layers {
                eprintln!("    layer {} albedo {:?} tile {:?}", l.slot, l.albedo, l.tile_m);
            }
            if let Some((_, t)) = c.extra_textures.iter().find(|(k, _)| k == "indices") {
                if let Ok(Some(img)) = ctx.textures.decode(t, 4096) {
                    let mut hist = [[0usize; 256]; 4];
                    for p in img.rgba.chunks_exact(4) {
                        for k in 0..4 {
                            hist[k][usize::from(p[k])] += 1;
                        }
                    }
                    for (k, h) in hist.iter().enumerate() {
                        let used: Vec<String> = h.iter().enumerate().filter(|(_, &n)| n > 0).take(24).map(|(v, n)| format!("{v}:{n}")).collect();
                        eprintln!("    indices {}x{} channel {k}: {}", img.width, img.height, used.join(" "));
                    }
                }
            }
        }
    }
    let mut ids: HashMap<String, u8> = HashMap::new();
    let mut details: Vec<(f32, u32, Vec<u8>)> = Vec::new();
    // per class: per layer slot name -> detail id
    let mut layer_ids: Vec<Vec<(String, u8)>> = Vec::new();
    for c in classes {
        let mut v = Vec::new();
        if let Some(c) = c {
            for l in &c.layers {
                let Some(albedo) = &l.albedo else { continue };
                let id = match ids.get(albedo) {
                    Some(&id) => id,
                    None if details.len() < MAX_DETAILS => {
                        let Some(img) = ctx.textures.decode(albedo, DETAIL_PX).ok().flatten() else { continue };
                        let lum: Vec<f32> = img.rgba.chunks_exact(4).map(|p| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2])).collect();
                        let mean = (lum.iter().sum::<f32>() / lum.len().max(1) as f32).max(1.0);
                        let mut px = vec![128u8; (DETAIL_PX * DETAIL_PX) as usize];
                        // nearest-resample to DETAIL_PX square (decode keeps the aspect; details are square in practice)
                        for y in 0..DETAIL_PX {
                            for x in 0..DETAIL_PX {
                                let sx = (x * img.width / DETAIL_PX) as usize;
                                let sy = (y * img.height / DETAIL_PX) as usize;
                                px[(y * DETAIL_PX + x) as usize] = (lum[sy * img.width as usize + sx] / mean * 128.0).clamp(0.0, 255.0) as u8;
                            }
                        }
                        details.push((l.tile_m.unwrap_or(4.0).max(0.5), DETAIL_PX, px));
                        let id = (details.len() - 1) as u8;
                        ids.insert(albedo.clone(), id);
                        id
                    }
                    None => continue,
                };
                v.push((l.slot.clone(), id));
            }
        }
        layer_ids.push(v);
    }
    if details.len() == MAX_DETAILS {
        eprintln!("  {level}: more than {MAX_DETAILS} ground detail textures, the rest left out");
    }
    let mut idx = vec![255u8; (w * h) as usize];
    let mut ws = Vec::with_capacity(8);
    for py in 0..h {
        let z = rect[1] + (py as f32 + 0.5) * texel;
        for px in 0..w {
            let x = rect[0] + (px as f32 + 0.5) * texel;
            weights.at(x, z, &mut ws);
            let Some(&(class, _)) = ws.iter().max_by(|a, b| a.1.total_cmp(&b.1)) else { continue };
            let class = usize::from(class);
            let (Some(Some(c)), Some(layers)) = (classes.get(class), layer_ids.get(class)) else { continue };
            if layers.is_empty() {
                continue;
            }
            let pick = match splats.get(class).and_then(|s| s.as_ref()) {
                Some(img) => {
                    // placed like the colour map: tiles every `size` from corner + offset, top row at larger z
                    let u = (x - corner[0] - c.offset[0]) / c.size[0];
                    let v = -(z - corner[1] - c.offset[1]) / c.size[1];
                    let sx = (((u - u.floor()) * img.width as f32) as usize).min(img.width as usize - 1);
                    let sy = (((v - v.floor()) * img.height as f32) as usize).min(img.height as usize - 1);
                    let p = &img.rgba[(sy * img.width as usize + sx) * 4..][..3];
                    if c.kind == LandClassKind::Indexed {
                        let tex = index_tex[class].get(usize::from(p[0])).cloned().flatten();
                        let id = tex.and_then(|t| layers.iter().find(|(s, _)| *s == t).map(|l| l.1));
                        if let Some(id) = id.or_else(|| layers.first().map(|l| l.1)) {
                            idx[(py * w + px) as usize] = id;
                        }
                        continue;
                    }
                    let black = 255i32 - i32::from(p[0]) - i32::from(p[1]) - i32::from(p[2]);
                    let strength = |slot: &str| match slot {
                        "Red" => i32::from(p[0]),
                        "Green" => i32::from(p[1]),
                        "Blue" => i32::from(p[2]),
                        _ => black,
                    };
                    layers.iter().max_by_key(|(s, _)| strength(s)).map(|l| l.1)
                }
                None => layers.first().map(|l| l.1),
            };
            if let Some(id) = pick {
                idx[(py * w + px) as usize] = id;
            }
        }
    }
    (idx, details)
}

fn put_grid(b: &mut Vec<u8>, top: &[f32]) {
    let base = top.iter().copied().filter(|v| v.is_finite()).fold(f32::INFINITY, f32::min) - GRID_SCALE;
    let base = if base.is_finite() { base } else { 0.0 };
    b.extend_from_slice(&base.to_le_bytes());
    b.extend_from_slice(&GRID_SCALE.to_le_bytes());
    for v in top {
        let q: u16 = if v.is_finite() { ((v - base) / GRID_SCALE).round().clamp(1.0, 65535.0) as u16 } else { 0 };
        b.extend_from_slice(&q.to_le_bytes());
    }
    pad(b);
}

fn pad(b: &mut Vec<u8>) {
    while b.len() % 4 != 0 {
        b.push(0);
    }
}

#[derive(Default)]
struct Stats {
    shot: usize,
    drawn: usize,
    trees: usize,
    ground: bool,
}

fn export(ctx: &mut Ctx, game: &Path, level: &str, out: &Path, g: Grid) -> Result<Stats> {
    let file = vehicle::map(&game.join("levels").join(format!("{level}.bin")))?;
    let lv = Level::parse(&file).with_context(|| format!("parsing {level}"))?;
    let (x1, z1) = (g.ox + g.cell * (g.w - 1) as f32, g.oz + g.cell * (g.h - 1) as f32);
    let mut roof = vec![f32::NEG_INFINITY; g.w * g.h];
    let mut veg = vec![f32::NEG_INFINITY; g.w * g.h];
    let mut used: Vec<usize> = Vec::new(); // mesh ids, in file order
    let mut file_id: HashMap<usize, u32> = HashMap::new();
    let mut tree_used: Vec<usize> = Vec::new();
    let mut tree_id: HashMap<usize, u32> = HashMap::new();
    let mut insts: Vec<(u32, Tm)> = Vec::new();
    let mut trees: Vec<(u32, [f32; 4])> = Vec::new();
    let mut st = Stats::default();
    for layer in lv.rendinst_layers()? {
        for ri in layer.instances()? {
            let pool = &layer.pools[ri.pool];
            let p = ri.tm[3];
            if p[0] < g.ox - 100.0 || p[0] > x1 + 100.0 || p[2] < g.oz - 100.0 || p[2] > z1 + 100.0 {
                continue;
            }
            if !ctx.kinds.contains_key(&pool.name) {
                let k = ctx.classify(&pool.name);
                ctx.kinds.insert(pool.name.clone(), k);
            }
            match &ctx.kinds[&pool.name] {
                Kind::Skip => {}
                Kind::Tree(t) => {
                    let t = *t;
                    let s = (ri.tm[0][0].powi(2) + ri.tm[0][1].powi(2) + ri.tm[0][2].powi(2)).sqrt().max(1e-3);
                    let m = &ctx.trees[t];
                    rasterize_disc(&g, p[0], p[2], m.radius * s, p[1] + m.height * s, &mut veg);
                    let id = *tree_id.entry(t).or_insert_with(|| {
                        tree_used.push(t);
                        (tree_used.len() - 1) as u32
                    });
                    trees.push((id, [p[0], p[1], p[2], s]));
                    st.trees += 1;
                }
                Kind::Object { mesh, shot, drawn } => {
                    if !shot.is_empty() {
                        let world: Vec<Tri> = shot.iter().map(|t| t.map(|q| apply(&ri.tm, q))).collect();
                        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                        for q in world.iter().flatten() {
                            for k in 0..3 {
                                lo[k] = lo[k].min(q[k]);
                                hi[k] = hi[k].max(q[k]);
                            }
                        }
                        if hi[1] - lo[1] >= MIN_HEIGHT && (hi[0] - lo[0]) * (hi[2] - lo[2]) >= MIN_AREA {
                            st.shot += 1;
                            for t in &world {
                                rasterize(&g, t, &mut roof);
                            }
                        }
                    }
                    if let (Some(mesh), true) = (mesh, drawn) {
                        let id = *file_id.entry(*mesh).or_insert_with(|| {
                            used.push(*mesh);
                            (used.len() - 1) as u32
                        });
                        insts.push((id, ri.tm));
                        st.drawn += 1;
                    }
                }
            }
        }
    }

    let mut b: Vec<u8> = Vec::new();
    b.extend_from_slice(b"WTVO");
    b.extend_from_slice(&2u32.to_le_bytes());
    b.extend_from_slice(&(g.w as u32).to_le_bytes());
    b.extend_from_slice(&(g.h as u32).to_le_bytes());
    put_grid(&mut b, &roof);
    put_grid(&mut b, &veg);
    b.extend_from_slice(&(used.len() as u32).to_le_bytes());
    for &mi in &used {
        let name = ctx.kinds.iter().find_map(|(n, k)| matches!(k, Kind::Object { mesh: Some(m), .. } if *m == mi).then_some(n.as_str())).unwrap_or("");
        b.extend_from_slice(&(name.len() as u32).to_le_bytes());
        b.extend_from_slice(name.as_bytes());
        pad(&mut b);
        let m = &ctx.meshes[mi];
        b.extend_from_slice(&(m.pos.len() as u32).to_le_bytes());
        b.extend_from_slice(&(m.idx.len() as u32).to_le_bytes());
        for v in m.pos.iter().flatten() {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for n in &m.nrm {
            b.extend(n.map(|x| (x.clamp(-1.0, 1.0) * 127.0).round() as i8 as u8));
            b.push(0);
        }
        for c in &m.rgba {
            b.extend_from_slice(c);
        }
        for i in &m.idx {
            b.extend_from_slice(&i.to_le_bytes());
        }
    }
    b.extend_from_slice(&(insts.len() as u32).to_le_bytes());
    for (m, tm) in &insts {
        b.extend_from_slice(&m.to_le_bytes());
        for v in tm.iter().flatten() {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    b.extend_from_slice(&(tree_used.len() as u32).to_le_bytes());
    for &t in &tree_used {
        let m = &ctx.trees[t];
        for v in [m.height, m.radius, m.crown_base] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&m.rgba);
    }
    b.extend_from_slice(&(trees.len() as u32).to_le_bytes());
    for (m, v) in &trees {
        b.extend_from_slice(&m.to_le_bytes());
        for x in v {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }
    fs::write(out.with_extension("obj"), b).with_context(|| format!("writing {}", out.display()))?;

    // Ground colours over the heightmap's extent (cells centred on the grid points, as the browser draws them)
    if let Some(gm) = GroundMaterial::load(&lv, &ctx.index)? {
        if let Some(weights) = gm.weights {
            let corner = weights.origin;
            let maps = decode_colormaps(&gm.classes, &ctx.textures, 1024, &mut |w| eprintln!("  {level}: {w}"));
            let baker = GroundBaker::new(weights, &gm.classes, maps, corner);
            let rect = [g.ox - g.cell / 2.0, g.oz - g.cell / 2.0, g.ox + (g.w as f32 - 0.5) * g.cell, g.oz + (g.h as f32 - 0.5) * g.cell];
            let texel = ((rect[2] - rect[0]).max(rect[3] - rect[1]) / GROUND_PX).max(GROUND_MIN_TEXEL);
            let img = baker.bake(rect, texel);
            let (idx, details) = bake_details(ctx, &gm.classes, baker.weights(), corner, rect, texel, img.width, img.height, level);
            let mut gb = Vec::with_capacity(8 + img.rgba.len() * 5 / 4);
            gb.extend_from_slice(&img.width.to_le_bytes());
            gb.extend_from_slice(&img.height.to_le_bytes());
            gb.extend_from_slice(&img.rgba);
            gb.extend_from_slice(&idx);
            gb.extend_from_slice(&(details.len() as u32).to_le_bytes());
            for (tile, side, lum) in &details {
                gb.extend_from_slice(&tile.to_le_bytes());
                gb.extend_from_slice(&side.to_le_bytes());
                gb.extend_from_slice(lum);
            }
            fs::write(out.with_extension("ground"), gb)?;
            st.ground = true;
        }
    }
    Ok(st)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        bail!("usage: wtv-objects <game dir> <jobs file>");
    }
    let game = PathBuf::from(&args[1]);
    let jobs = fs::read_to_string(&args[2])?;
    let mut ctx = Ctx {
        index: ResourceIndex::build(&game, &[COLLISION, REND_INST, LAND_CLASS])?,
        textures: TextureLibrary::open(&game)?,
        descs: model_descs(&game)?,
        tex_colour: HashMap::new(),
        kinds: HashMap::new(),
        meshes: Vec::new(),
        trees: Vec::new(),
    };
    eprintln!("resources: {}, model descriptions: {}", ctx.index.len(), ctx.descs.len());
    let mut failed = 0;
    for line in jobs.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let parsed = (|| -> Result<Grid> { Ok(Grid { ox: f[2].parse()?, oz: f[3].parse()?, cell: f[4].parse()?, w: f[5].parse()?, h: f[6].parse()? }) })();
        match parsed.and_then(|g| export(&mut ctx, &game, f[0], Path::new(f[1]), g)) {
            Ok(s) => println!("{} block shots {} drawn {} trees {} ground {}", f[0], s.shot, s.drawn, s.trees, if s.ground { "yes" } else { "no" }),
            Err(e) => {
                failed += 1;
                println!("{} FAILED {e:#}", f[0]);
            }
        }
    }
    if failed > 0 {
        bail!("{failed} levels failed");
    }
    Ok(())
}
