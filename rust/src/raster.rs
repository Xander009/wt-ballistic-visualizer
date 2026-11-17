//! CPU images shared by the 2D map and the 3D terrain: relief shading, ground colours, water and zones.
//! All are straight-alpha RGBA with +Z drawn upward (row 0 = the map's top edge), like the page's canvases.

use image::RgbaImage;
use tiny_skia::{BlendMode, Color, FillRule, Paint, PathBuilder, Pattern, Pixmap, Rect, SpreadMode, Stroke, Transform};

use crate::core::Terrain;
use crate::data::LoadedMap;
use crate::scene::{Shape, Zones};

const LIGHT: [f32; 3] = [-0.55, 0.75, 0.37];

/// Relief shading, one pixel per cell. grey: a multiplier for the ground colours (flat ground = 1), else a height tint.
pub fn shade(t: &Terrain, grey: bool) -> RgbaImage {
    let (w, h, hv, cell) = (t.w, t.hh, &t.h, t.cell as f32);
    let span = ((t.hi - t.lo) as f32).max(1.0);
    let mut img = RgbaImage::new(w as u32, h as u32);
    let px: &mut [u8] = &mut img;
    for j in 0..h {
        let r = (h - 1 - j) * w * 4;
        for i in 0..w {
            let c = hv[j * w + i];
            let gx = (hv[j * w + (i + 1).min(w - 1)] - hv[j * w + i.saturating_sub(1)]) / (2.0 * cell);
            let gz = (hv[(j + 1).min(h - 1) * w + i] - hv[j.saturating_sub(1) * w + i]) / (2.0 * cell);
            let nl = (gx * gx + 1.0 + gz * gz).sqrt();
            let lit = ((-gx * LIGHT[0] + LIGHT[1] - gz * LIGHT[2]) / nl).max(0.0);
            let o = r + i * 4;
            px[o + 3] = 255;
            if grey {
                let v = (255.0 * (0.3 + 0.7 * lit / 0.75).min(1.0)) as u8;
                px[o..o + 3].fill(v);
                continue;
            }
            let tt = (c - t.lo as f32) / span;
            let k = 0.35 + 0.75 * lit;
            let (rr, gg, bb) = if c < 0.0 {
                (40.0, 70.0, 100.0)
            } else if tt < 0.5 {
                (70.0 + 120.0 * tt, 95.0 + 70.0 * tt, 60.0 + 50.0 * tt)
            } else {
                (130.0 + 110.0 * (tt - 0.5), 130.0 + 100.0 * (tt - 0.5), 85.0 + 120.0 * (tt - 0.5))
            };
            px[o] = (rr * k).min(255.0) as u8;
            px[o + 1] = (gg * k).min(255.0) as u8;
            px[o + 2] = (bb * k).min(255.0) as u8;
        }
    }
    img
}

/// The map image: the game's ground colours multiplied by the relief of ground + roofs, else a height tint.
pub fn compose(map: &LoadedMap) -> RgbaImage {
    let Some(g) = &map.ground else { return shade(&map.tr, false) };
    let s = image::imageops::resize(&shade(&map.tr, true), g.color.width(), g.color.height(), image::imageops::FilterType::Triangle);
    let mut out = g.color.clone();
    for (o, m) in out.pixels_mut().zip(s.pixels()) {
        for c in 0..3 {
            o.0[c] = (o.0[c] as u16 * m.0[c] as u16 / 255) as u8;
        }
        o.0[3] = 255;
    }
    out
}

/// Water over the bare ground below the level's water plane; deeper is darker and more opaque.
pub fn water(t: &Terrain, level: f64) -> RgbaImage {
    let (w, h) = (t.w, t.hh);
    let mut img = RgbaImage::new(w as u32, h as u32);
    let px: &mut [u8] = &mut img;
    for j in 0..h {
        let r = (h - 1 - j) * w;
        for i in 0..w {
            let d = level as f32 - t.h[j * w + i];
            if d <= 0.0 {
                continue;
            }
            let k = (d / 12.0).min(1.0);
            let o = (r + i) * 4;
            px[o] = (72.0 - 50.0 * k) as u8;
            px[o + 1] = (142.0 - 78.0 * k) as u8;
            px[o + 2] = (178.0 - 60.0 * k) as u8;
            px[o + 3] = (255.0 * (0.55 + 0.37 * (d / 4.0).min(1.0))) as u8;
        }
    }
    img
}

/// Zone layer over the square map extent `size` m from (T.ox - cell/2, T.z1 + cell/2) down-right.
pub struct ZoneLayer {
    pub img: RgbaImage,
    pub size: f64,
}

/// Map border: stripes inside the map bounds but outside every battle area, red border at the battle area's edge
/// (none without a battle area). Red zones: the kill areas, solid dark red.
pub fn zones(map: &LoadedMap, z: &Zones, border: bool, red: bool) -> ZoneLayer {
    let t = &map.t;
    let c = t.cell;
    let (x0, z1) = (t.ox - c / 2.0, t.z1 + c / 2.0);
    let ext = t.w.max(t.hh) as f64 * c;
    let n = 2048.min(t.w.max(t.hh)) as u32;
    let k = n as f64 / ext;
    let mut pm = Pixmap::new(n, n).expect("zone layer size");
    let tf = |x: f64, zz: f64| (((x - x0) * k) as f32, ((z1 - zz) * k) as f32);
    let path = |sh: &Shape| {
        let mut pb = PathBuilder::new();
        match sh {
            Shape::Poly(p) => {
                for (i, q) in p.iter().enumerate() {
                    let (a, b) = tf(q[0], q[1]);
                    if i == 0 { pb.move_to(a, b) } else { pb.line_to(a, b) }
                }
                pb.close();
            }
            Shape::Circle { c, r } => {
                let (a, b) = tf(c[0], c[1]);
                pb.push_circle(a, b, ((r * k) as f32).max(0.01));
            }
        }
        pb.finish()
    };
    if border && !z.battle.is_empty() {
        let [bx0, bz0, bx1, bz1] = map.meta.bounds.unwrap_or([x0, z1 - ext, x0 + ext, z1]);
        let mut tile = Pixmap::new(16, 16).unwrap();
        tile.fill(Color::from_rgba8(230, 20, 20, 56));
        let mut sp = Paint::default();
        sp.set_color_rgba8(255, 25, 25, 217);
        sp.anti_alias = true;
        let mut pb = PathBuilder::new();
        for (a, b, cc, d) in [(-4.0, 20.0, 20.0, -4.0), (-4.0, 4.0, 4.0, -4.0), (12.0, 20.0, 20.0, 12.0)] {
            pb.move_to(a, b);
            pb.line_to(cc, d);
        }
        tile.stroke_path(&pb.finish().unwrap(), &sp, &Stroke { width: 3.0, ..Default::default() }, Transform::identity(), None);
        let mut paint = Paint::default();
        paint.shader = Pattern::new(tile.as_ref(), SpreadMode::Repeat, tiny_skia::FilterQuality::Nearest, 1.0, Transform::identity());
        let (a, b) = tf(bx0, bz1);
        if let Some(r) = Rect::from_xywh(a, b, ((bx1 - bx0) * k) as f32, ((bz1 - bz0) * k) as f32) {
            pm.fill_rect(r, &paint, Transform::identity(), None);
        }
        let mut erase = Paint { blend_mode: BlendMode::DestinationOut, ..Default::default() };
        erase.set_color_rgba8(0, 0, 0, 255);
        erase.anti_alias = true;
        for s in &z.battle {
            if let Some(p) = path(s) {
                pm.fill_path(&p, &erase, FillRule::Winding, Transform::identity(), None);
            }
        }
        let mut line = Paint { anti_alias: true, ..Default::default() };
        line.set_color_rgba8(255, 30, 30, 255);
        let st = Stroke { width: (4.0 * n as f32 / 2048.0).max(2.0), ..Default::default() };
        for s in &z.battle {
            if let Some(p) = path(s) {
                pm.stroke_path(&p, &line, &st, Transform::identity(), None);
            }
        }
    }
    if red && !z.kill.is_empty() {
        // one light tint over the union of all red zones (overlaps would stack to near-opaque red and hide the map),
        // then every zone's own edge: a dark halo under a bright line, readable on any ground
        let mut union = Pixmap::new(n, n).expect("zone layer size");
        let mut fill = Paint { anti_alias: true, ..Default::default() };
        fill.set_color_rgba8(170, 10, 10, 255);
        let paths: Vec<_> = z.kill.iter().filter_map(path).collect();
        for p in &paths {
            union.fill_path(p, &fill, FillRule::Winding, Transform::identity(), None);
        }
        pm.draw_pixmap(0, 0, union.as_ref(), &tiny_skia::PixmapPaint { opacity: 0.3, ..Default::default() }, Transform::identity(), None);
        let w = (3.0 * n as f32 / 2048.0).max(2.0);
        for (rgba, width) in [([40, 0, 0, 190], w + 2.5), ([255, 70, 60, 255], w)] {
            let mut line = Paint { anti_alias: true, ..Default::default() };
            line.set_color_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]);
            for p in &paths {
                pm.stroke_path(p, &line, &Stroke { width, ..Default::default() }, Transform::identity(), None);
            }
        }
    }
    ZoneLayer { img: demultiply(pm), size: ext }
}

fn demultiply(pm: Pixmap) -> RgbaImage {
    let (w, h) = (pm.width(), pm.height());
    let data = pm.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    });
    RgbaImage::from_raw(w, h, data.collect()).expect("pixmap size")
}

/// Metres over which a ground patch's own landmesh fades into the whole level's coarser ground at its edge
const PATCH_FADE: f64 = 48.0;
/// The finest and the most grid points along a patch's side (0.5 m on a heli pad, about 2 m on a 3 km airfield)
const PATCH_CELL: f64 = 0.5;
const PATCH_PX: f64 = 2048.0;
/// Calls f(k, l1, l2, l3) for each point k = j * w + i of a w x h grid inside the triangle a, b, d (grid units),
/// with its barycentric weights; `area` is the triangle's signed doubled area (non-zero).
fn each_point(a: (f32, f32), b: (f32, f32), d: (f32, f32), area: f32, w: usize, h: usize, mut f: impl FnMut(usize, f32, f32, f32)) {
    let i0 = a.0.min(b.0).min(d.0).ceil().max(0.0) as isize;
    let i1 = (a.0.max(b.0).max(d.0).floor() as isize).min(w as isize - 1);
    let j0 = a.1.min(b.1).min(d.1).ceil().max(0.0) as isize;
    let j1 = (a.1.max(b.1).max(d.1).floor() as isize).min(h as isize - 1);
    for j in j0..=j1 {
        for i in i0..=i1 {
            let (x, z) = (i as f32, j as f32);
            let l1 = ((b.0 - x) * (d.1 - z) - (d.0 - x) * (b.1 - z)) / area;
            let l2 = ((d.0 - x) * (a.1 - z) - (a.0 - x) * (d.1 - z)) / area;
            let l3 = 1.0 - l1 - l2;
            if l1 >= -1e-4 && l2 >= -1e-4 && l3 >= -1e-4 {
                f(j as usize * w + i as usize, l1, l2, l3);
            }
        }
    }
}

/// The triangles of a slab part placed by `pl` that face up (seen from above), on a grid c m apart from ox, oz: per
/// triangle its vertex indices into `part.v`, their grid positions and the signed doubled area. An airfield's edge
/// decal is a closed hull: its bottom faces down and carries the texture mirrored.
fn up_tris<'a>(part: &'a crate::data::SlabPart, pl: &crate::data::SlabPlace, ox: f64, oz: f64, c: f64) -> impl Iterator<Item = ([usize; 3], [(f32, f32); 3], f32)> + 'a {
    let m = pl.tm;
    let g: Vec<(f32, f32)> = part
        .v
        .iter()
        .map(|v| {
            let (x, z) = (v[0] * m[0] + v[1] * m[3] + v[2] * m[6] + m[9], v[0] * m[2] + v[1] * m[5] + v[2] * m[8] + m[11]);
            (((x as f64 - ox) / c) as f32, ((z as f64 - oz) / c) as f32)
        })
        .collect();
    // up-facing triangles wind clockwise in the model's x, z (Dagor's left-handed axes); the placement can mirror
    let flip = m[0] * m[8] - m[2] * m[6];
    part.idx.chunks_exact(3).filter_map(move |tri| {
        let t = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
        let [pa, pb, pd] = t.map(|i| g[i]);
        let area = (pb.0 - pa.0) * (pd.1 - pa.1) - (pd.0 - pa.0) * (pb.1 - pa.1);
        (area * flip < -1e-6).then_some((t, [pa, pb, pd], area))
    })
}

/// Paints the slabs' parts over the ground colours `col` (a w x h grid c m apart from ox, oz), as the game draws
/// its decals: bigger slabs first (a heli pad goes over an airfield), and within one the opaque parts, then the
/// alpha-blended ones, each group from low to high.
fn paint_slabs(slabs: &[(&crate::data::Slab, &crate::data::SlabPlace)], col: &mut [[f32; 3]], w: usize, h: usize, ox: f64, oz: f64, c: f64) {
    let span = |s: &crate::data::Slab| s.parts.iter().flat_map(|p| &p.v).fold(0f32, |a, v| a.max(v[0].abs()).max(v[2].abs()));
    let mut order: Vec<_> = slabs.iter().copied().collect();
    order.sort_by(|a, b| span(b.0).total_cmp(&span(a.0)));
    for (slab, pl) in order {
        let median_y = |p: &crate::data::SlabPart| { let mut y: Vec<f32> = p.v.iter().map(|v| v[1]).collect(); y.sort_by(f32::total_cmp); y[y.len() / 2] };
        let mut parts: Vec<_> = slab.parts.iter().filter(|p| !p.v.is_empty()).collect();
        parts.sort_by(|a, b| b.blend.cmp(&a.blend).then(median_y(a).total_cmp(&median_y(b))));
        for part in parts {
            let chain = &slab.tex[part.tex];
            let (tw, th) = (chain[0].width() as f32, chain[0].height() as f32);
            for ([a, b, d], [pa, pb, pd], area) in up_tris(part, pl, ox, oz, c) {
                // the mip level with about one texel per patch cell
                let (va, vb, vd) = (&part.v[a], &part.v[b], &part.v[d]);
                let uv_area = ((vb[3] - va[3]) * (vd[4] - va[4]) - (vd[3] - va[3]) * (vb[4] - va[4])).abs() * tw * th;
                let level = ((uv_area / area.abs()).sqrt().max(1.0).log2().floor() as usize).min(chain.len() - 1);
                let tex = &chain[level];
                let (lw, lh) = (tex.width() as f32, tex.height() as f32);
                each_point(pa, pb, pd, area, w, h, |k, l1, l2, l3| {
                    let (u, v) = (l1 * va[3] + l2 * vb[3] + l3 * vd[3], l1 * va[4] + l2 * vb[4] + l3 * vd[4]);
                    let p = tex.get_pixel(((u.rem_euclid(1.0) * lw) as u32).min(tex.width() - 1), ((v.rem_euclid(1.0) * lh) as u32).min(tex.height() - 1)).0;
                    let (q, al) = (&mut col[k], p[3] as f32 / 255.0);
                    let al = if part.blend == 1 { 1.0 } else { al };
                    (0..3).for_each(|k| q[k] += (p[k] as f32 - q[k]) * al);
                });
            }
        }
    }
}

/// Bilinear sample of `img` (row 0 = +Z edge) spread over x0..x0 + wm, z1 - hm..z1, clamped at its edge.
fn sample(img: &RgbaImage, x0: f64, z1: f64, wm: f64, hm: f64, x: f64, z: f64) -> [f32; 3] {
    let (w, h) = (img.width() as f64, img.height() as f64);
    let fx = ((x - x0) / wm * w - 0.5).clamp(0.0, w - 1.0);
    let fy = ((z1 - z) / hm * h - 0.5).clamp(0.0, h - 1.0);
    let (i, j) = (fx as u32, fy as u32);
    let (i1, j1) = ((i + 1).min(w as u32 - 1), (j + 1).min(h as u32 - 1));
    let (tx, ty) = ((fx - i as f64) as f32, (fy - j as f64) as f32);
    let p = |a: u32, b: u32| img.get_pixel(a, b).0;
    let (a, b, c, d) = (p(i, j), p(i1, j), p(i, j1), p(i1, j1));
    std::array::from_fn(|k| (a[k] as f32 * (1.0 - tx) + b[k] as f32 * tx) * (1.0 - ty) + (c[k] as f32 * (1.0 - tx) + d[k] as f32 * tx) * ty)
}

/// The smallest objects that block shots, as in the level's own collision grid (extractor/objects MIN_HEIGHT, MIN_AREA)
const ROOF_MIN_H: f32 = 2.5;
const ROOF_MIN_AREA: f32 = 6.0;

/// The patch `p`'s surface with the objects of its placement standing on it (hangars, towers, barracks: the
/// instances of `obj` tagged `p.tag` at least ROOF_MIN_H tall and ROOF_MIN_AREA in plan), each a solid block up to
/// its highest surface per grid point, which src marks 1 ("an object").
// shortcut: from the drawn meshes, not the game's collision meshes; a lattice tower blocks only where its
// triangles face up or down. Export the collision like the level's roof grid if that shows.
pub fn roofed(p: &crate::data::Patch, obj: &crate::data::Objects) -> Terrain {
    let t = &p.t;
    let (w, h) = (t.w, t.hh);
    let mut hv = t.h.clone();
    let mut src = vec![0u8; w * h];
    let tags = obj.tags.as_ref().map(|x| &x.0);
    for (k, (mi, m)) in obj.inst.iter().enumerate() {
        if tags.is_some_and(|tg| tg[k] != p.tag) || !t.inside(f64::from(m[9]), f64::from(m[11])) {
            continue;
        }
        let Some(model) = obj.models.get(*mi as usize) else { continue };
        let world: Vec<[f32; 3]> = model.pos.chunks_exact(3).map(|v| std::array::from_fn(|a| v[0] * m[a] + v[1] * m[3 + a] + v[2] * m[6 + a] + m[9 + a])).collect();
        let (lo, hi) = world.iter().fold(([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]), |(lo, hi), v| (std::array::from_fn(|a| lo[a].min(v[a])), std::array::from_fn(|a| hi[a].max(v[a]))));
        if hi[1] - lo[1] < ROOF_MIN_H || (hi[0] - lo[0]) * (hi[2] - lo[2]) < ROOF_MIN_AREA {
            continue;
        }
        let g: Vec<(f32, f32)> = world.iter().map(|v| (((f64::from(v[0]) - t.ox) / t.cell) as f32, ((f64::from(v[2]) - t.oz) / t.cell) as f32)).collect();
        for tri in model.idx.chunks_exact(3) {
            let [a, b, d] = [tri[0], tri[1], tri[2]].map(|i| i as usize);
            let (pa, pb, pd) = (g[a], g[b], g[d]);
            let area = (pb.0 - pa.0) * (pd.1 - pa.1) - (pd.0 - pa.0) * (pb.1 - pa.1);
            if area.abs() < 1e-6 {
                continue; // a wall seen from above: the roofs and floors give the block
            }
            each_point(pa, pb, pd, area, w, h, |k, l1, l2, l3| {
                let y = l1 * world[a][1] + l2 * world[b][1] + l3 * world[d][1];
                if y > hv[k] {
                    hv[k] = y;
                    src[k] = 1;
                }
            });
        }
    }
    Terrain::grid(hv, w, h, t.cell, t.ox, t.oz, Some(src))
}

/// Metres a slab's lowest vertices lie below its top for it to be a platform with a skirt (see level_under)
const SKIRT: f32 = 10.0;

/// Airfields (and some heli pads) stand on level ground, where the landmesh under them rises and falls by 10 to
/// 250 m: their slab is a flat top with a skirt hanging down tens to hundreds of metres to meet the ground. The
/// game puts the group's origin on the ground (`ground` there), so the top is a platform that high over it, flat
/// across hills, and the skirt an embankment down to the ground around it (ground above the skirt stays).
// shortcut: the platform height is the ground at the slab's origin; the 3 km airfields' groups also carry a
// "ground_level" node 200 m off it, which the game may use instead. Export it if a runway sits visibly off.
#[allow(clippy::too_many_arguments)]
fn level_under(slab: &crate::data::Slab, pl: &crate::data::SlabPlace, heights: &mut [f32], w: usize, h: usize, ox: f64, oz: f64, c: f64, ground: impl Fn(f64, f64) -> f64) {
    let (lo, hi) = slab.parts.iter().flat_map(|p| &p.v).fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), v| (a.min(v[1]), b.max(v[1])));
    // trench slabs are dug-in bunkers, not platforms
    if hi - lo < SKIRT || pl.model.to_lowercase().contains("trench") {
        return;
    }
    let y0 = ground(f64::from(pl.tm[9]), f64::from(pl.tm[11])) as f32;
    let mut top = vec![f32::NEG_INFINITY; w * h];
    for part in &slab.parts {
        for ([a, b, d], [pa, pb, pd], area) in up_tris(part, pl, ox, oz, c) {
            let y = [part.v[a][1], part.v[b][1], part.v[d][1]];
            each_point(pa, pb, pd, area, w, h, |k, l1, l2, l3| top[k] = top[k].max(l1 * y[0] + l2 * y[1] + l3 * y[2]));
        }
    }
    for (g, &t) in heights.iter_mut().zip(&top) {
        if t >= hi - 0.25 {
            *g = y0 + hi;
        } else if t.is_finite() {
            *g = g.max(y0 + t);
        }
    }
}

/// The ground patch of one airfield or heli pad slab placement `pl`: the landmesh around it from its tile (fading
/// into `far`'s ground at the edge), coloured by the slabs' textures over the level's ground colours by their
/// masks, with the patch's own relief. The slabs are decals: the game lays them on the ground (their placements'
/// heights are often tens of metres off it), so they colour the ground; where two overlap the higher part in its own
/// frame wins. An airfield's slab also levels the ground (see level_under). Every slab of `slabs` that reaches into the patch is in it, so patches that
/// overlap (a heli pad by an airfield) agree.
// shortcut: slabs of missions that do not place `pl` are baked in too where they overlap; all missions of a level
// seen so far place the same slabs. Bake per mission if one ever differs.
pub fn patch(slabs: &[(&crate::data::Slab, &crate::data::SlabPlace)], pl: &crate::data::SlabPlace, tile: &[f32], far: &LoadedMap) -> crate::data::Patch {
    use rayon::prelude::*;
    let tl = &pl.tile;
    let (wm, hm) = ((tl.w - 1) as f64 * tl.cell, (tl.h - 1) as f64 * tl.cell);
    let c = (wm.max(hm) / (PATCH_PX - 1.0)).max(PATCH_CELL);
    let (w, h) = ((wm / c) as usize + 1, (hm / c) as usize + 1);
    let (ox, oz) = (tl.x0, tl.z0);
    let tile_t = Terrain::grid(tile.to_vec(), tl.w, tl.h, tl.cell, tl.x0, tl.z0, None);
    // ground: the tile fading into the whole level's ground at the edge
    let ft = &far.t;
    let heights: Vec<f32> = (0..w * h)
        .into_par_iter()
        .map(|k| {
            let (x, z) = (ox + (k % w) as f64 * c, oz + (k / w) as f64 * c);
            let edge = (x - ox).min(ox + wm - x).min(z - oz).min(oz + hm - z);
            let f = (edge / PATCH_FADE).clamp(0.0, 1.0);
            (ft.height_at(x, z) * (1.0 - f) + tile_t.height_at(x, z) * f) as f32
        })
        .collect();
    let mut heights = heights;
    for &(slab, sp) in slabs {
        level_under(slab, sp, &mut heights, w, h, ox, oz, c, |x, z| if tile_t.inside(x, z) { tile_t.height_at(x, z) } else { ft.height_at(x, z) });
    }
    let t = Terrain::grid(heights, w, h, c, ox, oz, None);
    // colours (row 0 = +Z edge) times the patch's relief
    let base = far.ground.as_ref().map(|g| &g.color);
    let far_img;
    let (img_src, fx0, fz1, fwm, fhm) = match base {
        Some(b) => (b, ft.ox - ft.cell / 2.0, ft.z1 + ft.cell / 2.0, ft.w as f64 * ft.cell, ft.hh as f64 * ft.cell),
        None => {
            far_img = shade(ft, false);
            (&far_img, ft.ox - ft.cell / 2.0, ft.z1 + ft.cell / 2.0, ft.w as f64 * ft.cell, ft.hh as f64 * ft.cell)
        }
    };
    let relief = shade(&t, true);
    // the level's ground colours, then the slabs' layers painted over them in the game's order (see paint_slabs)
    let mut col: Vec<[f32; 3]> = (0..w * h).into_par_iter().map(|k| sample(img_src, fx0, fz1, fwm, fhm, ox + (k % w) as f64 * c, oz + (k / w) as f64 * c)).collect();
    paint_slabs(slabs, &mut col, w, h, ox, oz, c);
    let mut img = RgbaImage::new(w as u32, h as u32);
    img.par_chunks_mut(w * 4).enumerate().for_each(|(r, row)| {
        let j = h - 1 - r;
        for i in 0..w {
            let (rgb, s) = (col[j * w + i], relief.get_pixel(i as u32, r as u32).0[0] as f32 / 255.0);
            row[i * 4..i * 4 + 4].copy_from_slice(&[(rgb[0] * s).min(255.0) as u8, (rgb[1] * s).min(255.0) as u8, (rgb[2] * s).min(255.0) as u8, 255]);
        }
    });
    crate::data::Patch { tag: pl.tag, t: std::sync::Arc::new(t), img }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{MapMeta, Slab, SlabPart, SlabPlace};

    #[test]
    fn patch_takes_the_tile_inside_the_level_at_its_edge_and_the_slab_colours() {
        // the whole level: flat at 0 over -1000..1000; the tile: flat at 3 over -100..100
        let meta = MapMeta { name: "far".into(), bounds: None, width: 3, height: 3, cell: 1000.0, ox: -1000.0, oz: -1000.0, hmin: 0.0, hmax: 10.0, missions: vec![], water: None };
        let far = LoadedMap::new(meta, vec![0; 9]);
        // a red slab 10 m square at (0..10, 0..10), opaque, its placement 40 m up (a decal: it does not lift the ground)
        let mut tex = RgbaImage::new(2, 2);
        tex.pixels_mut().for_each(|p| p.0 = [250, 0, 0, 255]);
        let slab = Slab { parts: vec![SlabPart { tex: 0, blend: 0, v: vec![[0.0, 0.0, 0.0, 0.0, 0.0], [10.0, 0.0, 0.0, 1.0, 0.0], [10.0, 0.0, 10.0, 1.0, 1.0], [0.0, 0.0, 10.0, 0.0, 1.0]], idx: vec![0, 2, 1, 0, 3, 2] }], tex: vec![vec![tex]] };
        let pl: SlabPlace = serde_json::from_value(serde_json::json!({"tag": 7, "model": "s", "tm": [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 40, 0],
            "tile": {"x0": -100, "z0": -100, "cell": 8, "w": 26, "h": 26, "lo": 0, "hi": 1, "q": ""}})).unwrap();
        let p = patch(&[(&slab, &pl)], &pl, &vec![3.0; 26 * 26], &far);
        assert_eq!(p.tag, 7);
        let t = &p.t;
        assert!((t.height_at(5.0, 5.0) - 3.0).abs() < 1e-4, "the slab does not shape the ground");
        assert!(t.height_at(t.ox, 0.0).abs() < 1e-4, "the edge meets the whole level");
        let px = |x: f64, z: f64| p.img.get_pixel(((x - t.ox) / t.cell).round() as u32, ((t.z1 - z) / t.cell).round() as u32).0;
        assert!(px(5.0, 5.0)[0] > 200 && px(5.0, 5.0)[1] < 30, "slab colour: {:?}", px(5.0, 5.0));
        assert!(px(-50.0, -50.0)[0] < 200, "ground colour: {:?}", px(-50.0, -50.0));
    }

    #[test]
    fn airfield_slab_levels_the_ground_and_its_bottom_does_not_paint() {
        let meta = MapMeta { name: "far".into(), bounds: None, width: 3, height: 3, cell: 1000.0, ox: -1000.0, oz: -1000.0, hmin: 0.0, hmax: 10.0, missions: vec![], water: None };
        let far = LoadedMap::new(meta, vec![0; 9]);
        let solid = |c: [u8; 4]| { let mut t = RgbaImage::new(2, 2); t.pixels_mut().for_each(|p| p.0 = c); vec![t] };
        // x0..x1 by z0..z1 at height y, facing up (the data's winding) or down
        let quad = |tex: usize, x0: f32, x1: f32, z0: f32, z1: f32, y: f32, up: bool| SlabPart {
            tex, blend: 1, v: vec![[x0, y, z0, 0.0, 0.0], [x1, y, z0, 1.0, 0.0], [x1, y, z1, 1.0, 1.0], [x0, y, z1, 0.0, 1.0]],
            idx: if up { vec![0, 2, 1, 0, 3, 2] } else { vec![0, 1, 2, 0, 2, 3] },
        };
        // a platform 4 m up over -20..20, its skirt 50 m down out to 60 m, and a red bottom over 30..50 facing down
        let slab = Slab { parts: vec![quad(0, -20.0, 20.0, -20.0, 20.0, 4.0, true), quad(0, -60.0, 60.0, -60.0, 60.0, -46.0, true), quad(1, 30.0, 50.0, -10.0, 10.0, 4.0, false)], tex: vec![solid([90, 90, 90, 255]), solid([250, 0, 0, 255])] };
        let pl: SlabPlace = serde_json::from_value(serde_json::json!({"tag": 1, "model": "af", "tm": [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 40, 0],
            "tile": {"x0": -100, "z0": -100, "cell": 8, "w": 26, "h": 26, "lo": 0, "hi": 1, "q": ""}})).unwrap();
        // the ground rises 1 m every 8 m eastward: 12.5 m at the origin
        let tile: Vec<f32> = (0..26 * 26).map(|k| (k % 26) as f32).collect();
        let p = patch(&[(&slab, &pl)], &pl, &tile, &far);
        let t = &p.t;
        for x in [-15.0, 0.0, 15.0] {
            assert!((t.height_at(x, 5.0) - 16.5).abs() < 1e-3, "platform at {x}: {}", t.height_at(x, 5.0));
        }
        assert!((t.height_at(40.0, 0.0) - 17.5).abs() < 0.2, "the ground beyond the platform stays: {}", t.height_at(40.0, 0.0));
        let px = p.img.get_pixel(((40.0 - t.ox) / t.cell).round() as u32, ((t.z1 - 0.0) / t.cell).round() as u32).0;
        assert!(px[1] > 40, "the downward face is not painted: {px:?}");
    }

    #[test]
    fn placement_buildings_stand_on_their_patch() {
        use crate::data::{Model, Objects, Patch};
        let grid = crate::core::Grid { base: 0.0, scale: 1.0, q: vec![] };
        // a 10 x 10 m roof 6 m up, and a 1 m crate (too small to block); placed at (20, 20) and (40, 40)
        let quad = |s: f32, y: f32| Model { pos: vec![0.0, y, 0.0, s, y, 0.0, s, y, s, 0.0, y, s, 0.0, 0.0, 0.0], nrm: vec![0; 20], rgba: vec![0; 20], idx: vec![0, 1, 2, 0, 2, 3, 4, 0, 1] };
        let at = |m: u32, x: f32| (m, [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, x, 2.0, x]);
        let obj = Objects { roof: grid.clone(), veg: grid, models: vec![quad(10.0, 6.0), quad(1.0, 1.0)], inst: vec![at(0, 20.0), at(1, 40.0), at(0, 60.0)], tree_models: vec![], trees: vec![], tags: Some((vec![3, 3, 4], vec![])) };
        let p = Patch { tag: 3, t: std::sync::Arc::new(Terrain::grid(vec![2.0; 81 * 81], 81, 81, 1.0, 0.0, 0.0, None)), img: RgbaImage::new(1, 1) };
        let t = roofed(&p, &obj);
        assert!((t.height_at(25.0, 25.0) - 8.0).abs() < 1e-4 && t.src_at(25.0, 25.0) == 1, "the building blocks");
        assert!((t.height_at(40.5, 40.5) - 2.0).abs() < 1e-4, "the crate does not");
        assert!((t.height_at(65.0, 65.0) - 2.0).abs() < 1e-4 && t.src_at(65.0, 65.0) == 0, "another placement's building is not on this patch");
    }

    /// Review aid: every level's airfield and heli pad patches as PNGs in $WTV_PATCH_DIR (cargo test --release
    /// -- --ignored patches_of_every_level).
    #[test]
    #[ignore]
    fn patches_of_every_level() {
        let dir = std::path::PathBuf::from(std::env::var("WTV_PATCH_DIR").expect("WTV_PATCH_DIR"));
        let store = crate::data::Store::open();
        for m in crate::data::load_index(&store).unwrap().maps {
            let Some(f) = m.far.as_ref().filter(|_| std::env::var("WTV_PATCH_LEVEL").is_err_and(|_| true) || std::env::var("WTV_PATCH_LEVEL").is_ok_and(|l| l == m.name)) else { continue };
            let (fm, Some(g)) = crate::data::load_far(&store, &m.name, f).unwrap() else { continue };
            let models: std::collections::HashMap<&str, Slab> = g.slabs.iter().map(|p| (p.model.as_str(), crate::data::load_slab(&store, &p.model).unwrap())).collect();
            let slabs: Vec<_> = g.slabs.iter().map(|p| (&models[p.model.as_str()], p)).collect();
            for &(_, pl) in &slabs {
                let p = patch(&slabs, pl, &pl.tile.heights(&store).unwrap(), &fm);
                let img = if std::env::var("WTV_PATCH_FULL").is_ok() { p.img.clone() } else { image::imageops::thumbnail(&p.img, 600, 600 * p.img.height() / p.img.width()) };
                img.save(dir.join(format!("{}_{}_{}.png", m.name, pl.tag, pl.model))).unwrap();
            }
        }
    }
}
