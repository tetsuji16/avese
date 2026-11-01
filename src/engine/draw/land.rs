//! Bird's-eye view: oblique projection with a painter's algorithm.
use super::axes::{arrow_head, fmt_num, pick_step, ticks};
use super::plane::{dot_op, nodes};
use super::{Ctx, Layout, push_text, use_cylinder};
use crate::engine::field::{Cylinder, Field, GridField, MeshField};
use crate::engine::raster::measure_text;
use crate::engine::scene::{Anchor, Op, Rgb};
use std::f64::consts::TAU;

const MAX_POLYS: usize = 60_000;
/// Height bands per full z range at which a gradient looks continuous.
const COLOR_STEPS: f64 = 85.0;
/// Upper limit of mesh lines per direction; more would only blacken the surface.
const MESH_LINES: usize = 120;

/// Oblique projection measured from the original: `eta` shears the plate sideways by
/// `tan(min(|eta|, 45))` of its depth, `phi` shortens the depth by `cos(phi)`, and heights keep
/// their full length at every angle.
#[derive(Clone, Copy, Debug)]
pub struct Proj {
    /// sideways shift per unit of depth, positive when the far edge moves right
    shear: f64,
    cos_p: f64,
    wd: f64,
    hd: f64,
    /// size of the projected bounding box in pixels
    pub w: f64,
    pub h: f64,
    ox: f64,
    oy: f64,
}

impl Proj {
    pub fn new(eta: i32, phi: i32, wd: f64, hd: f64, hz: f64) -> Proj {
        let shear = (eta.abs().min(45) as f64).to_radians().tan() * (eta.signum() as f64);
        let cos_p = (phi as f64).to_radians().cos();
        Proj { shear, cos_p, wd, hd, w: (wd + hd * shear.abs()).max(1.0), h: (hd * cos_p + hz).max(1.0), ox: 0.0, oy: 0.0 }
    }

    pub fn placed(mut self, ox: f64, oy: f64) -> Proj {
        self.ox = ox;
        self.oy = oy;
        self
    }

    /// Image position and depth (larger = farther away).
    pub fn at(&self, x: f64, y: f64, z: f64) -> ((f32, f32), f64) {
        let shift = if self.shear >= 0.0 { self.shear * y } else { -self.shear * (self.hd - y) };
        let top = self.h - self.hd * self.cos_p;
        let sx = self.ox + x + shift;
        let sy = self.oy + top + (self.hd - y) * self.cos_p - z;
        ((sx as f32, sy as f32), y - self.shear * x - self.cos_p * z)
    }
}

/// Data to world coordinates (pixels before projection).
struct Map {
    x0: f64,
    y0: f64,
    xr: f64,
    yr: f64,
    wd: f64,
    hd: f64,
    cyl: Option<(Cylinder, f64)>,
    flip: bool,
    zlo: f64,
    zr: f64,
    hz: f64,
}

impl Map {
    fn xy(&self, x: f64, y: f64) -> (f64, f64) {
        if let Some((c, k)) = &self.cyl {
            let (dx, dy) = c.to_plane(x, y);
            return (self.wd / 2.0 + dx * k, self.hd / 2.0 + dy * k);
        }
        let v = (y - self.y0) / self.yr;
        (((x - self.x0) / self.xr) * self.wd, (if self.flip { 1.0 - v } else { v }) * self.hd)
    }

    fn z(&self, z: f64) -> f64 {
        ((z - self.zlo) / self.zr).clamp(0.0, 1.0) * self.hz
    }
}

type Items = Vec<(f64, Op)>;

struct View<'a> {
    ctx: &'a Ctx<'a>,
    map: Map,
    proj: Proj,
    bounds: (u32, u32),
}

impl View<'_> {
    fn pt(&self, x: f64, y: f64, z: f64) -> ((f32, f32), f64) {
        let (wx, wy) = self.map.xy(x, y);
        self.proj.at(wx, wy, self.map.z(z))
    }

    fn line(&self, ops: &mut Vec<Op>, a: (f64, f64, f64), b: (f64, f64, f64), color: Rgb, width: f32) {
        ops.push(Op::Polyline { pts: vec![self.proj.at(a.0, a.1, a.2).0, self.proj.at(b.0, b.1, b.2).0], color, width });
    }
}

pub(crate) fn draw(ctx: &Ctx, lay: &Layout, ops: &mut Vec<Op>) {
    let _ = draw_with_progress(ctx, lay, ops, |_| true);
}

pub(crate) fn draw_with_progress(ctx: &Ctx, lay: &Layout, ops: &mut Vec<Op>, mut progress: impl FnMut(f32) -> bool) -> bool {
    if !progress(0.0) { return false; }
    let (Some(proj), Some((zlo, zhi)), Some((x0, x1, y0, y1))) = (lay.proj, ctx.zr, ctx.field.extent()) else { return true };
    let proj = proj.placed(lay.px, lay.py);
    let s = ctx.s;
    let cyl = use_cylinder(ctx.field, s).map(|c| (c, proj.wd / (2.0 * (c.r_max - c.r_min).abs().max(1e-300))));
    let map = Map {
        x0,
        y0,
        xr: (x1 - x0).abs().max(1e-300),
        yr: (y1 - y0).abs().max(1e-300),
        wd: proj.wd,
        hd: proj.hd,
        cyl,
        flip: matches!(ctx.field, Field::Panel(_)),
        zlo,
        zr: zhi - zlo,
        hz: s.land_height.max(1) as f64,
    };
    let v = View { ctx, map, proj, bounds: (lay.w, lay.h) };
    if !progress(0.08) { return false; }
    let mut items: Items = Vec::new();
    let is_panel = matches!(ctx.field, Field::Panel(_));
    if s.scatter && !is_panel {
        if !scatter(&v, &mut items, &mut progress) { return false; }
    } else {
        match ctx.field {
            Field::Grid(g) => if !grid_surface(&v, g, &mut items, &mut progress) { return false; },
            Field::Mesh(m) => if !mesh_surface(&v, m, &mut items, &mut progress) { return false; },
            Field::Panel(_) => if !panel_surface(&v, &mut items, &mut progress) { return false; },
        }
        if !s.surface {
            let done = match ctx.field {
                Field::Grid(g) => grid_skirts(&v, g, &mut items, &mut progress),
                Field::Mesh(m) => mesh_skirts(&v, m, &mut items, &mut progress),
                Field::Panel(_) => true,
            };
            if !done { return false; }
        }
    }
    if s.grid && s.grid_mesh && !surface_mesh(&v, &mut items, &mut progress) {
        return false;
    }
    if !progress(0.82) { return false; }
    items.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    ops.extend(items.into_iter().map(|i| i.1));
    if s.grid {
        frame(&v, ops);
    }
    if s.axis {
        axes(&v, ops);
    }
    if s.point {
        let total = nodes(ctx.field).count();
        let stride = total.div_ceil(50_000).max(1);
        for (i, n) in nodes(ctx.field).step_by(stride).enumerate() {
            if i % 256 == 0 && !progress(0.82 + 0.17 * i as f32 / (total.div_ceil(stride).max(1) as f32)) { return false; }
            if let Some(z) = n.z {
                ops.push(dot_op(0, v.pt(n.x, n.y, z).0.0, v.pt(n.x, n.y, z).0.1, 3.0, s.fg));
            }
        }
    }
    progress(1.0)
}

fn scatter(v: &View, items: &mut Items, progress: &mut impl FnMut(f32) -> bool) -> bool {
    let s = v.ctx.s;
    let total = nodes(v.ctx.field).count();
    let stride = total.div_ceil(MAX_POLYS * 2).max(1);
    for (i, n) in nodes(v.ctx.field).step_by(stride).enumerate() {
        if i % 256 == 0 && !progress(0.08 + 0.72 * i as f32 / total.div_ceil(stride).max(1) as f32) { return false; }
        let Some(z) = n.z else { continue };
        let ((px, py), d) = v.pt(n.x, n.y, z);
        items.push((d, dot_op(s.scatter_mode, px, py, s.dot_size.max(1) as f32, v.ctx.color(z))));
    }
    true
}

fn poly(pts: Vec<(f32, f32)>, c: Rgb, wire: bool) -> Op {
    if wire { Op::Polygon { pts, fill: None, stroke: Some(c) } } else { Op::Polygon { pts, fill: Some(c), stroke: Some(c) } }
}

fn unit_grid_cells(v: &View, g: &GridField, i0: usize, i1: usize, j0: usize, j1: usize, items: &mut Items, progress: &mut impl FnMut(f32) -> bool, from: f32, to: f32) -> bool {
    let total = (i1 - i0) * (j1 - j0);
    let mut done = 0usize;
    for j in j0..j1 {
        for i in i0..i1 {
            done += 1;
            if done % 128 == 0 && !progress(from + (to-from) * done as f32 / total.max(1) as f32) { return false; }
            let [Some(z0),Some(z1),Some(z2),Some(z3)] = [g.at(i,j),g.at(i+1,j),g.at(i+1,j+1),g.at(i,j+1)] else { continue };
            let corners = [(i,j,z0),(i+1,j,z1),(i+1,j+1,z2),(i,j+1,z3)];
            let p: Vec<_> = corners.iter().map(|&(x,y,z)| v.pt(g.xs[x],g.ys[y],z)).collect();
            let depth = p.iter().map(|q| q.1).sum::<f64>() / 4.0;
            items.push((depth, poly(p.iter().map(|q| q.0).collect(), v.ctx.color((z0+z1+z2+z3)/4.0), false)));
        }
    }
    progress(to)
}

fn grid_surface(v: &View, g: &GridField, items: &mut Items, progress: &mut impl FnMut(f32) -> bool) -> bool {
    let base_items = items.len();
    let (nx, ny) = (g.xs.len(), g.ys.len());
    let full = (nx - 1) * (ny - 1);
    if full > MAX_POLYS && !g.z.iter().all(Option::is_some) {
        return reduced_sparse_grid_surface(v, g, items, progress);
    }
    // For dense, oversized grids, keep a sparse triangulated sample of the
    // full surface. Each block contributes its actual extrema in addition to
    // the coarse lattice vertices; one global triangulation keeps adjacent
    // blocks connected and avoids the gaps created by dropping whole cells.
    if full > MAX_POLYS && g.z.iter().all(Option::is_some) {
        if let Some(completed) = reduced_grid_surface(v, g, items, progress) {
            return completed;
        }
        items.truncate(base_items);
        return unit_grid_cells(v, g, 0, nx - 1, 0, ny - 1, items, progress, 0.57, 0.82);
    }
    let stride = ((full as f64 / MAX_POLYS as f64).sqrt().ceil() as usize).max(1);
    let idx = |n: usize| -> Vec<usize> {
        let mut v: Vec<usize> = (0..n).step_by(stride).collect();
        if v.last() != Some(&(n - 1)) {
            v.push(n - 1);
        }
        v
    };
    let (is, js) = (idx(nx), idx(ny));
    let kmax = if stride == 1 { ((MAX_POLYS as f64 / full as f64).sqrt().floor() as usize).clamp(1, 32) } else { 1 };
    let total = js.len().saturating_sub(1) * is.len().saturating_sub(1);
    let mut done = 0usize;
    for jw in js.windows(2) {
        for iw in is.windows(2) {
            done += 1;
            if done % 128 == 0 && !progress(0.08 + 0.72 * done as f32 / total.max(1) as f32) { return false; }
            let (ia, ib, ja, jb) = (iw[0], iw[1], jw[0], jw[1]);
            let (Some(z00), Some(z10), Some(z01), Some(z11)) = (g.at(ia, ja), g.at(ib, ja), g.at(ia, jb), g.at(ib, jb)) else { continue };
            let at = |tx: f64, ty: f64| {
                let x = g.xs[ia] + (g.xs[ib] - g.xs[ia]) * tx;
                let y = g.ys[ja] + (g.ys[jb] - g.ys[ja]) * ty;
                let z = z00 * (1.0 - tx) * (1.0 - ty) + z10 * tx * (1.0 - ty) + z01 * (1.0 - tx) * ty + z11 * tx * ty;
                (x, y, z)
            };
            // Fine enough that the colour steps vanish, without going below a pixel.
            let corners = [(ia, ja, z00), (ib, ja, z10), (ia, jb, z01), (ib, jb, z11)].map(|(i, j, z)| v.pt(g.xs[i], g.ys[j], z).0);
            let extent = corners.iter().flat_map(|p| corners.iter().map(move |q| (p.0 - q.0).abs().max((p.1 - q.1).abs()))).fold(0.0f32, f32::max);
            let span = z00.max(z10).max(z01).max(z11) - z00.min(z10).min(z01).min(z11);
            let k = ((span / v.map.zr.max(1e-300) * COLOR_STEPS).ceil() as usize).max(4).min(extent.ceil() as usize).max(1).min(kmax);
            for b in 0..k {
                for a in 0..k {
                    let (t0, t1, u0, u1) = (a as f64 / k as f64, (a + 1) as f64 / k as f64, b as f64 / k as f64, (b + 1) as f64 / k as f64);
                    let corners = [at(t0, u0), at(t1, u0), at(t1, u1), at(t0, u1)];
                    let proj: Vec<((f32, f32), f64)> = corners.iter().map(|&(x, y, z)| v.pt(x, y, z)).collect();
                    let depth = proj.iter().map(|p| p.1).sum::<f64>() / 4.0;
                    let c = v.ctx.color(at((t0 + t1) / 2.0, (u0 + u1) / 2.0).2);
                    items.push((depth, poly(proj.iter().map(|p| p.0).collect(), c, false)));
                }
            }
        }
    }
    true
}

fn reduced_grid_surface(v: &View, g: &GridField, items: &mut Items, progress: &mut impl FnMut(f32) -> bool) -> Option<bool> {
    let (nx, ny) = (g.xs.len(), g.ys.len());
    let full = (nx - 1) * (ny - 1);
    // A block contributes one lattice vertex and at most two extrema, so about
    // six triangles. Keep a little in reserve for very narrow grids where only
    // one axis can be decimated.
    let target_blocks = (MAX_POLYS / 7).max(1);
    let mut stride = ((full as f64 / target_blocks as f64).sqrt().ceil() as usize).max(1);
    while (nx - 1).div_ceil(stride) * (ny - 1).div_ceil(stride) > target_blocks {
        stride += 1;
    }
    let mut is: Vec<usize> = (0..nx).step_by(stride).collect();
    let mut js: Vec<usize> = (0..ny).step_by(stride).collect();
    if is.last() != Some(&(nx - 1)) { is.push(nx - 1); }
    if js.last() != Some(&(ny - 1)) { js.push(ny - 1); }

    let block_count = (is.len() - 1) * (js.len() - 1);
    let mut retained = Vec::<(usize, usize)>::with_capacity(is.len() * js.len() + block_count * 2);
    for &j in &js {
        for &i in &is { retained.push((i, j)); }
    }
    for (bi, jw) in js.windows(2).enumerate() {
        for (ai, iw) in is.windows(2).enumerate() {
            let index = bi * (is.len() - 1) + ai;
            if index % 128 == 0 && !progress(0.08 + 0.48 * index as f32 / block_count.max(1) as f32) { return Some(false); }
            let block_progress = 0.08 + 0.48 * index as f32 / block_count.max(1) as f32;
            let mut lo = (iw[0], jw[0], g.at(iw[0], jw[0]).unwrap());
            let mut hi = lo;
            let mut scanned = 0usize;
            for j in jw[0]..=jw[1] {
                for i in iw[0]..=iw[1] {
                    scanned += 1;
                    if scanned % 8192 == 0 && !progress(block_progress) { return Some(false); }
                    let z = g.at(i, j).unwrap();
                    if z < lo.2 { lo = (i, j, z); }
                    if z > hi.2 { hi = (i, j, z); }
                }
            }
            retained.push((lo.0, lo.1));
            retained.push((hi.0, hi.1));
        }
    }
    retained.sort_unstable();
    retained.dedup();
    let points: Vec<(f64, f64)> = retained.iter().map(|&(i, j)| (g.xs[i], g.ys[j])).collect();
    if !progress(0.57) { return Some(false); }
    let triangles = match crate::engine::mesh::triangulate_points(&points, |_| {}) {
        Ok(t) => t,
        Err(_) => return None,
    };
    if triangles.len() > MAX_POLYS { return None; }
    let total = triangles.len();
    for (n, tri) in triangles.iter().enumerate() {
        if n % 128 == 0 && !progress(0.58 + 0.23 * n as f32 / total.max(1) as f32) { return Some(false); }
        let a = retained[tri[0]];
        let b = retained[tri[1]];
        let c = retained[tri[2]];
        let (za, zb, zc) = (g.at(a.0, a.1).unwrap(), g.at(b.0, b.1).unwrap(), g.at(c.0, c.1).unwrap());
        let pa = v.pt(g.xs[a.0], g.ys[a.1], za);
        let pb = v.pt(g.xs[b.0], g.ys[b.1], zb);
        let pc = v.pt(g.xs[c.0], g.ys[c.1], zc);
        let pts = vec![pa.0, pb.0, pc.0];
        let depth = (pa.1 + pb.1 + pc.1) / 3.0;
        items.push((depth, poly(pts, v.ctx.color((za + zb + zc) / 3.0), false)));
    }
    Some(progress(0.82))
}

fn reduced_sparse_grid_surface(v: &View, g: &GridField, items: &mut Items, progress: &mut impl FnMut(f32) -> bool) -> bool {
    let (nx, ny) = (g.xs.len(), g.ys.len());
    let base_items = items.len();
    let full = (nx - 1) * (ny - 1);
    let target_blocks = (MAX_POLYS / 32).max(1);
    let mut stride = ((full as f64 / target_blocks as f64).sqrt().ceil() as usize).max(1);
    while (nx - 1).div_ceil(stride) * (ny - 1).div_ceil(stride) > target_blocks { stride += 1; }
    let mut is: Vec<usize> = (0..nx).step_by(stride).collect();
    let mut js: Vec<usize> = (0..ny).step_by(stride).collect();
    if is.last() != Some(&(nx - 1)) { is.push(nx - 1); }
    if js.last() != Some(&(ny - 1)) { js.push(ny - 1); }
    let (bw, bh) = (is.len() - 1, js.len() - 1);
    let total = bw * bh;
    let mut complete = vec![true; total];
    let mut detailed_cells = 0usize;
    let mut scanned = 0usize;
    let mut last_progress = 0.08f32;
    for by in 0..bh {
        for bx in 0..bw {
            let idx = by * bw + bx;
            'scan: for j in js[by]..=js[by + 1] {
                for i in is[bx]..=is[bx + 1] {
                    scanned += 1;
                    if scanned % 8192 == 0 && !progress(0.08) { return false; }
                    if g.at(i, j).is_none() { complete[idx] = false; break 'scan; }
                }
            }
            if !complete[idx] {
                for j in js[by]..js[by + 1] {
                    for i in is[bx]..is[bx + 1] {
                        if [g.at(i,j),g.at(i+1,j),g.at(i,j+1),g.at(i+1,j+1)].iter().all(Option::is_some) { detailed_cells += 1; }
                    }
                }
            }
        }
    }
    // Detailed cells around holes are safe but expensive. If they exceed the
    // reduced mesh budget, fall back to every original valid unit cell.
    let detail = detailed_cells <= MAX_POLYS / 4;
    if !detail {
        return unit_grid_cells(v, g, 0, nx - 1, 0, ny - 1, items, progress, 0.08, 0.82);
    }
    let mut done = 0usize;
    for by in 0..bh {
        for bx in 0..bw {
            let idx = by * bw + bx;
            done += 1;
            let tile_progress = 0.08 + 0.72 * done as f32 / total.max(1) as f32;
            if done % 64 == 0 {
                if !progress(tile_progress) { return false; }
                last_progress = tile_progress;
            }
            let (i0, i1, j0, j1) = (is[bx], is[bx+1], js[by], js[by+1]);
            if !complete[idx] {
                if !unit_grid_cells(v,g,i0,i1,j0,j1,items,progress,last_progress,tile_progress) { return false; }
                last_progress = tile_progress;
                if items.len() > MAX_POLYS {
                    items.truncate(base_items);
                    return unit_grid_cells(v,g,0,nx-1,0,ny-1,items,progress,tile_progress,0.82);
                }
                continue;
            }

            let mut vertices = Vec::<(usize,usize)>::new();
            let mut push_extrema = |iter: &mut dyn Iterator<Item=(usize,usize)>| {
                let mut lo: Option<(usize,usize,f64)> = None;
                let mut hi: Option<(usize,usize,f64)> = None;
                for (i,j) in iter {
                    let z = g.at(i,j).unwrap();
                    if lo.is_none_or(|p| z < p.2) { lo = Some((i,j,z)); }
                    if hi.is_none_or(|p| z > p.2) { hi = Some((i,j,z)); }
                }
                if let Some(p) = lo { vertices.push((p.0,p.1)); }
                if let Some(p) = hi { vertices.push((p.0,p.1)); }
            };
            for j in [j0,j1] { push_extrema(&mut (i0..=i1).map(|i| (i,j))); }
            for i in [i0,i1] { push_extrema(&mut (j0..=j1).map(|j| (i,j))); }
            if i0+1 < i1 && j0+1 < j1 {
                push_extrema(&mut (j0+1..j1).flat_map(|j| (i0+1..i1).map(move |i| (i,j))));
            }
            vertices.extend([(i0,j0),(i1,j0),(i1,j1),(i0,j1)]);
            // Refined neighbors expose every sample along their shared edge;
            // retain those same edge vertices here to keep the surfaces joined.
            if detail {
                let neighbors = [
                    (bx.checked_sub(1).map(|x| by*bw+x), (i0,j0,i0,j1)),
                    ((bx+1<bw).then_some(by*bw+bx+1), (i1,j0,i1,j1)),
                    (by.checked_sub(1).map(|y| y*bw+bx), (i0,j0,i1,j0)),
                    ((by+1<bh).then_some((by+1)*bw+bx), (i0,j1,i1,j1)),
                ];
                for (neighbor, (xa,ya,xb,yb)) in neighbors {
                    if neighbor.is_some_and(|n| !complete[n]) {
                        if xa == xb { vertices.extend((ya.min(yb)..=ya.max(yb)).map(|y| (xa,y))); }
                        else { vertices.extend((xa.min(xb)..=xa.max(xb)).map(|x| (x,ya))); }
                    }
                }
            }
            vertices.sort_unstable();
            vertices.dedup();
            let points: Vec<_> = vertices.iter().map(|&(i,j)| (g.xs[i],g.ys[j])).collect();
            // Geometry failure is not cancellation: fall back to original valid
            // cells in this tile without reducing the source data.
            let Ok(tris) = crate::engine::mesh::triangulate_points(&points, |_| {}) else {
                if !unit_grid_cells(v,g,i0,i1,j0,j1,items,progress,last_progress,tile_progress) { return false; }
                last_progress = tile_progress;
                if items.len() > MAX_POLYS {
                    items.truncate(base_items);
                    return unit_grid_cells(v,g,0,nx-1,0,ny-1,items,progress,tile_progress,0.82);
                }
                continue
            };
            if tris.is_empty() {
                if !unit_grid_cells(v,g,i0,i1,j0,j1,items,progress,last_progress,tile_progress) { return false; }
                last_progress = tile_progress;
                if items.len() > MAX_POLYS {
                    items.truncate(base_items);
                    return unit_grid_cells(v,g,0,nx-1,0,ny-1,items,progress,tile_progress,0.82);
                }
                continue
            }
            if items.len() + tris.len() > MAX_POLYS {
                if !progress(tile_progress) { return false; }
                items.truncate(base_items);
                return unit_grid_cells(v,g,0,nx-1,0,ny-1,items,progress,tile_progress,0.82);
            }
            for tri in tris {
                let a=vertices[tri[0]]; let b=vertices[tri[1]]; let c=vertices[tri[2]];
                let (za,zb,zc)=(g.at(a.0,a.1).unwrap(),g.at(b.0,b.1).unwrap(),g.at(c.0,c.1).unwrap());
                let p=[v.pt(g.xs[a.0],g.ys[a.1],za),v.pt(g.xs[b.0],g.ys[b.1],zb),v.pt(g.xs[c.0],g.ys[c.1],zc)];
                items.push(((p[0].1+p[1].1+p[2].1)/3.0, poly(p.iter().map(|q| q.0).collect(), v.ctx.color((za+zb+zc)/3.0), false)));
            }
            if done % 64 != 0 {
                if !progress(tile_progress) { return false; }
                last_progress = tile_progress;
            }
        }
    }
    progress(0.82)
}

fn mesh_surface(v: &View, m: &MeshField, items: &mut Items, progress: &mut impl FnMut(f32) -> bool) -> bool {
    let reduced;
    // Spatial reduction can bridge missing-data holes. Use it only when every
    // source vertex has a value; incomplete meshes retain all original triangles.
    let (nodes, values, tris) = if m.tris.len() > MAX_POLYS && m.z.iter().all(Option::is_some) {
        let Some(r) = reduce_mesh(m, &mut |p| progress(0.08 + 0.05 * p)) else { return false };
        reduced = r;
        (&reduced.0[..], &reduced.1[..], &reduced.2[..])
    } else {
        (&m.nodes[..], &m.z[..], &m.tris[..])
    };
    let draw_start = if m.tris.len() > MAX_POLYS { 0.13 } else { 0.08 };
    for (i, t) in tris.iter().enumerate() {
        if i % 256 == 0 && !progress(draw_start + 0.67 * i as f32 / tris.len().max(1) as f32) { return false; }
        let (Some(a), Some(b), Some(c)) = (values[t[0]], values[t[1]], values[t[2]]) else { continue };
        let p: Vec<((f32, f32), f64)> = [(t[0], a), (t[1], b), (t[2], c)].iter().map(|&(k, z)| v.pt(nodes[k].0, nodes[k].1, z)).collect();
        let depth = p.iter().map(|q| q.1).sum::<f64>() / 3.0;
        items.push((depth, poly(p.iter().map(|q| q.0).collect(), v.ctx.color((a + b + c) / 3.0), false)));
    }
    true
}

/// Reduce a large Delaunay mesh by spatially sampling its source vertices, then
/// retriangulating them. Keeping source hull vertices preserves the full boundary;
/// retriangulation covers the reduced domain without dropping scattered triangles.
fn reduce_mesh(m: &MeshField, progress: &mut impl FnMut(f32) -> bool) -> Option<(Vec<(f64, f64)>, Vec<Option<f64>>, Vec<[usize; 3]>)> {
    use std::collections::HashMap;
    use delaunator::{Point, triangulate};

    const NODE_BUDGET: usize = MAX_POLYS / 3;
    if m.nodes.len() <= NODE_BUDGET { return Some((m.nodes.clone(), m.z.clone(), m.tris.clone())); }
    let mut edge_counts = HashMap::<(usize, usize), u8>::new();
    for (i, tri) in m.tris.iter().enumerate() {
        if i % 8192 == 0 && !progress(0.4 * i as f32 / m.tris.len() as f32) { return None; }
        for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let key = if a < b { (a, b) } else { (b, a) };
            *edge_counts.entry(key).or_default() += 1;
        }
    }
    let mut boundary = edge_counts.into_iter().flat_map(|((a,b),n)| if n == 1 { vec![a,b] } else { Vec::new() }).collect::<Vec<_>>();
    boundary.sort_unstable();
    boundary.dedup();
    // Dropping hull vertices cuts away real portions of the surface. When the
    // boundary alone cannot fit safely, retain the original mesh instead.
    if boundary.len() > MAX_POLYS / 2 {
        return Some((m.nodes.clone(), m.z.clone(), m.tris.clone()));
    }
    let interior_budget = NODE_BUDGET.saturating_sub(boundary.len());

    let (x0,x1,y0,y1) = m.nodes.iter().fold((f64::INFINITY,f64::NEG_INFINITY,f64::INFINITY,f64::NEG_INFINITY), |a,p| (a.0.min(p.0),a.1.max(p.0),a.2.min(p.1),a.3.max(p.1)));
    let side = (interior_budget as f64).sqrt().floor() as usize;
    let mut selected = boundary;
    if side > 0 && x1 > x0 && y1 > y0 {
        let sx = x1-x0;
        let sy = y1-y0;
        let mut best = vec![None::<(f64,usize)>; side*side];
        // Two more nodes per coarser block keep peaks and valleys, within MAX_POLYS.
        let peak_side = (side / 2).max(1);
        let mut peaks = vec![None::<(f64,usize,f64,usize)>; peak_side*peak_side];
        for (i,&(x,y)) in m.nodes.iter().enumerate() {
            if i % 8192 == 0 && !progress(0.4 + 0.5 * i as f32 / m.nodes.len() as f32) { return None; }
            let cx = (((x-x0)/sx*side as f64).floor() as usize).min(side-1);
            let cy = (((y-y0)/sy*side as f64).floor() as usize).min(side-1);
            let center = (x0+(cx as f64+0.5)*sx/side as f64,y0+(cy as f64+0.5)*sy/side as f64);
            let d = (x-center.0).powi(2)+(y-center.1).powi(2);
            let slot = &mut best[cy*side+cx];
            if slot.is_none_or(|v| d<v.0) { *slot=Some((d,i)); }
            if let Some(z) = m.z[i] {
                let block = &mut peaks[cy*peak_side/side*peak_side+cx*peak_side/side];
                let e = block.get_or_insert((z,i,z,i));
                if z<e.0 { (e.0,e.1)=(z,i); }
                if z>e.2 { (e.2,e.3)=(z,i); }
            }
        }
        selected.extend(best.into_iter().flatten().map(|(_,i)| i));
        selected.extend(peaks.into_iter().flatten().flat_map(|(_,lo,_,hi)| [lo,hi]));
    }
    selected.sort_unstable();
    selected.dedup();
    let nodes = selected.iter().map(|&i| m.nodes[i]).collect::<Vec<_>>();
    let z = selected.iter().map(|&i| m.z[i]).collect::<Vec<_>>();
    let points = nodes.iter().map(|&(x,y)| Point{x,y}).collect::<Vec<_>>();
    if !progress(0.93) { return None; }
    let triangulation = triangulate(&points);
    let tris = triangulation.triangles.chunks_exact(3).filter_map(|t| {
        let tri = [t[0],t[1],t[2]];
        let p = tri.map(|i| nodes[i]);
        let samples = [
            p[0], p[1], p[2],
            ((p[0].0+p[1].0)*0.5,(p[0].1+p[1].1)*0.5),
            ((p[1].0+p[2].0)*0.5,(p[1].1+p[2].1)*0.5),
            ((p[2].0+p[0].0)*0.5,(p[2].1+p[0].1)*0.5),
            ((p[0].0+p[1].0+p[2].0)/3.0,(p[0].1+p[1].1+p[2].1)/3.0),
        ];
        samples.iter().all(|&(x,y)| m.sample(x,y).is_some()).then_some(tri)
    }).collect::<Vec<_>>();
    debug_assert!(tris.len() <= MAX_POLYS);
    if tris.is_empty() { return Some((nodes,z,tris)); }
    Some((nodes,z,tris))
}

fn panel_surface(v: &View, items: &mut Items, progress: &mut impl FnMut(f32) -> bool) -> bool {
    let Field::Panel(p) = v.ctx.field else { return true };
    let cells = p.rows * p.cols;
    let target = if cells > MAX_POLYS { MAX_POLYS / 5 } else { MAX_POLYS };
    let stride = (cells as f64 / target as f64).sqrt().ceil().max(1.0) as usize;
    let total = p.rows.div_ceil(stride) * p.cols.div_ceil(stride);
    let mut done = 0usize;
    for r in (0..p.rows).step_by(stride) {
        for c in (0..p.cols).step_by(stride) {
            done += 1;
            if done % 128 == 0 && !progress(0.08 + 0.72 * done as f32 / total.max(1) as f32) { return false; }
            let Some(z) = p.z[r * p.cols + c] else { continue };
            let (x, y) = (c as f64, r as f64);
            let (x1, y1) = ((c + stride).min(p.cols) as f64, (r + stride).min(p.rows) as f64);
            let q: Vec<((f32, f32), f64)> = [(x, y), (x1, y), (x1, y1), (x, y1)].iter().map(|&(a, b)| v.pt(a, b, z)).collect();
            let depth = q.iter().map(|q| q.1).sum::<f64>() / 4.0;
            items.push((depth, poly(q.iter().map(|q| q.0).collect(), v.ctx.color(z), !v.ctx.s.surface)));
            // Draw a riser where the adjacent tile is lower or absent. The
            // higher tile owns shared walls, so each height transition is emitted once.
            let faces = [
                (r > 0, c, r.saturating_sub(stride), [(x,y),(x1,y)]),
                (c + stride < p.cols, c + stride, r, [(x1,y),(x1,y1)]),
                (r + stride < p.rows, c, r + stride, [(x1,y1),(x,y1)]),
                (c > 0, c.saturating_sub(stride), r, [(x,y1),(x,y)]),
            ];
            for (has_neighbor, nc, nr, edge) in faces {
                let neighbor = if has_neighbor { p.z.get(nr * p.cols + nc).copied().flatten() } else { None };
                if neighbor.is_some_and(|nz| nz >= z) { continue; }
                let low = neighbor.unwrap_or(v.map.zlo).min(z);
                if low == z { continue; }
                let (a,b) = (edge[0], edge[1]);
                let pts = [(a.0,a.1,z),(b.0,b.1,z),(b.0,b.1,low),(a.0,a.1,low)];
                let pr: Vec<_> = pts.iter().map(|&(xx,yy,zz)| v.pt(xx,yy,zz)).collect();
                let depth = pr.iter().map(|q| q.1).sum::<f64>() / 4.0;
                items.push((depth, poly(pr.iter().map(|q| q.0).collect(), v.ctx.color((z+low)*0.5), !v.ctx.s.surface)));
            }
        }
    }
    true
}

/// Keep the part of a convex polygon in `(t, height)` between two heights.
fn clip_band(pts: &[(f64, f64)], lo: f64, hi: f64) -> Vec<(f64, f64)> {
    let clip = |pts: &[(f64, f64)], limit: f64, keep_above: bool| {
        let inside = |p: (f64, f64)| if keep_above { p.1 >= limit } else { p.1 <= limit };
        let mut out = Vec::with_capacity(pts.len() + 2);
        for (i, &a) in pts.iter().enumerate() {
            let b = pts[(i + 1) % pts.len()];
            if inside(a) { out.push(a); }
            if inside(a) != inside(b) {
                let u = (limit - a.1) / (b.1 - a.1);
                out.push((a.0 + (b.0 - a.0) * u, limit));
            }
        }
        out
    };
    clip(&clip(pts, lo, true), hi, false)
}

/// One stretch of the skirt below a surface edge given in world coordinates, in `bands` height
/// steps coloured by height.
fn wall(v: &View, a: (f64, f64, f64), b: (f64, f64, f64), bands: usize, items: &mut Items) {
    let hz = v.map.hz;
    let top = a.2.max(b.2);
    let quad = [(0.0, 0.0), (1.0, 0.0), (1.0, b.2), (0.0, a.2)];
    for n in 0..bands {
        let (lo, hi) = (hz * n as f64 / bands as f64, hz * (n + 1) as f64 / bands as f64);
        if lo >= top { break; }
        let pts = clip_band(&quad, lo, hi);
        if pts.len() < 3 { continue; }
        let p: Vec<_> = pts.iter().map(|&(t, h)| v.proj.at(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, h)).collect();
        let depth = p.iter().map(|q| q.1).sum::<f64>() / p.len() as f64;
        let color = v.ctx.color(v.map.zlo + v.map.zr * (lo + hi) / 2.0 / hz);
        items.push((depth, poly(p.iter().map(|q| q.0).collect(), color, false)));
    }
}

/// Whether the edge a-b, with the surface on the `inner` side, turns towards the viewer.
fn faces_viewer(v: &View, a: (f64, f64), b: (f64, f64), inner: (f64, f64)) -> bool {
    let (ex, ey) = (b.0 - a.0, b.1 - a.1);
    if ex.hypot(ey) < 1e-9 { return false; }
    let (mut nx, mut ny) = (ey, -ex);
    if nx * (inner.0 - a.0) + ny * (inner.1 - a.1) > 0.0 { (nx, ny) = (-nx, -ny); }
    ny - v.proj.shear * nx < -1e-9
}

/// Draw the skirts for edges given in world coordinates, sharing the polygon budget between them.
/// Only heavily fragmented data has more walls than the budget; an even share of them is kept.
fn push_walls(v: &View, walls: &[((f64, f64, f64), (f64, f64, f64))], items: &mut Items) {
    let budget = MAX_POLYS / 4;
    let bands = ((v.map.hz / 1.5).ceil() as usize).min(COLOR_STEPS as usize).min(budget / walls.len().max(1)).max(1);
    for &(a, b) in walls.iter().step_by(walls.len().div_ceil(budget).max(1)) {
        wall(v, a, b, bands, items);
    }
}

/// The cell column beside a column of cells, if any. Theta (x) wraps around in cylindrical
/// coordinates: the first and last columns meet.
fn neighbour_column(n: Option<usize>, nx: usize, cyl: bool) -> Option<usize> {
    match n {
        Some(n) if n + 1 < nx => Some(n),
        Some(_) => cyl.then_some(0),
        None => cyl.then(|| nx - 2),
    }
}

/// With `CBsurface` off the original closes the surface with solid walls down to the floor along
/// the edges that face the viewer, including the edges of missing data.
fn grid_skirts(v: &View, g: &GridField, items: &mut Items, progress: &mut impl FnMut(f32) -> bool) -> bool {
    let (nx, ny) = (g.xs.len(), g.ys.len());
    let cyl = v.map.cyl.is_some();
    let ok = |i: usize, j: usize| i + 1 < nx && j + 1 < ny && g.at(i, j).is_some() && g.at(i + 1, j).is_some() && g.at(i, j + 1).is_some() && g.at(i + 1, j + 1).is_some();
    let mut walls = Vec::new();
    for j in 0..ny.saturating_sub(1) {
        if j % 16 == 0 && !progress(0.82) { return false; }
        for i in 0..nx.saturating_sub(1) {
            if !ok(i, j) { continue; }
            let inner = v.map.xy((g.xs[i] + g.xs[i + 1]) / 2.0, (g.ys[j] + g.ys[j + 1]) / 2.0);
            let open_i = |n: Option<usize>| neighbour_column(n, nx, cyl).is_none_or(|n| !ok(n, j));
            let edges = [
                (j == 0 || !ok(i, j - 1), (i, j), (i + 1, j)),
                (!ok(i, j + 1), (i, j + 1), (i + 1, j + 1)),
                (open_i(i.checked_sub(1)), (i, j), (i, j + 1)),
                (open_i(Some(i + 1)), (i + 1, j), (i + 1, j + 1)),
            ];
            for (open, a, b) in edges {
                if !open { continue; }
                let (Some(za), Some(zb)) = (g.at(a.0, a.1), g.at(b.0, b.1)) else { continue };
                let (da, db) = ((g.xs[a.0], g.ys[a.1]), (g.xs[b.0], g.ys[b.1]));
                // an edge of constant radius is an arc: follow it in short pieces
                let (pa, pb) = (v.map.xy(da.0, da.1), v.map.xy(db.0, db.1));
                let pieces = if cyl { (((pb.0 - pa.0).hypot(pb.1 - pa.1) / 4.0).ceil() as usize).clamp(1, 64) } else { 1 };
                let at = |n: usize| {
                    let t = n as f64 / pieces as f64;
                    let p = v.map.xy(da.0 + (db.0 - da.0) * t, da.1 + (db.1 - da.1) * t);
                    (p.0, p.1, v.map.z(za + (zb - za) * t))
                };
                for n in 0..pieces {
                    let (p, q) = (at(n), at(n + 1));
                    if faces_viewer(v, (p.0, p.1), (q.0, q.1), inner) { walls.push((p, q)); }
                }
            }
        }
    }
    push_walls(v, &walls, items);
    true
}

fn mesh_skirts(v: &View, m: &MeshField, items: &mut Items, progress: &mut impl FnMut(f32) -> bool) -> bool {
    use std::collections::HashMap;
    let mut edges = HashMap::<(usize, usize), (u32, usize)>::new();
    for (n, t) in m.tris.iter().enumerate() {
        if n % 8192 == 0 && !progress(0.82) { return false; }
        if t.iter().any(|&k| m.z[k].is_none()) { continue; }
        for (a, b, c) in [(t[0], t[1], t[2]), (t[1], t[2], t[0]), (t[2], t[0], t[1])] {
            edges.entry((a.min(b), a.max(b))).or_insert((0, c)).0 += 1;
        }
    }
    let world = |k: usize| v.map.xy(m.nodes[k].0, m.nodes[k].1);
    let mut open: Vec<_> = edges.into_iter().filter(|e| e.1.0 == 1 && faces_viewer(v, world(e.0.0), world(e.0.1), world(e.1.1))).collect();
    // Equal depths keep this order through the stable sort, so it must not depend on the hash.
    open.sort_unstable_by_key(|e| e.0);
    let walls: Vec<_> = open.into_iter().filter_map(|((a, b), _)| {
        let (pa, pb) = (world(a), world(b));
        Some(((pa.0, pa.1, v.map.z(m.z[a]?)), (pb.0, pb.1, v.map.z(m.z[b]?))))
    }).collect();
    push_walls(v, &walls, items);
    true
}

/// Mesh lines of the bird's-eye view follow the data cells over the surface, so that nearer parts
/// of it hide them.
fn surface_mesh(v: &View, items: &mut Items, progress: &mut impl FnMut(f32) -> bool) -> bool {
    let s = v.ctx.s;
    let m = &v.map;
    // the original draws no mesh lines in cylindrical coordinates
    if m.cyl.is_some() { return true; }
    let (col, lw) = (s.fg, s.grid_line_width.max(1) as f32);
    // enough to win over the polygon the line lies on, not enough to show through a ridge
    let bias = 3.0 * (m.wd * m.hd / items.len().max(1) as f64).sqrt().clamp(1.0, 10.0);
    const STEP_PX: f64 = 3.0;
    const MAX_SEGMENTS: f64 = (MAX_POLYS / 2) as f64;
    let thin = |t: &[f64]| -> Vec<f64> {
        let stride = t.len().div_ceil(MESH_LINES).max(1);
        let mut out: Vec<f64> = t.iter().copied().step_by(stride).collect();
        if out.last() != t.last() { out.extend(t.last()); }
        out
    };
    let lines: Vec<((f64, f64), (f64, f64))> = match v.ctx.field {
        Field::Grid(g) => {
            let (xs, ys) = (thin(&g.xs), thin(&g.ys));
            xs.iter().map(|&x| ((x, m.y0), (x, m.y0 + m.yr))).chain(ys.iter().map(|&y| ((m.x0, y), (m.x0 + m.xr, y)))).collect()
        }
        Field::Mesh(mesh) => {
            let mut edges: Vec<(usize, usize)> = mesh.tris.iter().flat_map(|t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]).map(|(a, b)| (a.min(b), a.max(b))).collect();
            edges.sort_unstable();
            edges.dedup();
            let stride = edges.len().div_ceil(MESH_LINES * MESH_LINES).max(1);
            edges.into_iter().step_by(stride).map(|(a, b)| (mesh.nodes[a], mesh.nodes[b])).collect()
        }
        Field::Panel(p) => {
            let cols = (0..=p.cols).map(|c| { let x = (c as f64).min(p.cols as f64 - 1e-9); ((x, 0.0), (x, p.rows as f64 - 1e-9)) });
            let rows = (0..=p.rows).map(|r| { let y = (r as f64).min(p.rows as f64 - 1e-9); ((0.0, y), (p.cols as f64 - 1e-9, y)) });
            cols.chain(rows).collect()
        }
    };
    let length = |&(a, b): &((f64, f64), (f64, f64))| {
        let (wa, wb) = (m.xy(a.0, a.1), m.xy(b.0, b.1));
        (wb.0 - wa.0).hypot(wb.1 - wa.1)
    };
    // A large image gets longer segments rather than more of them.
    let step = STEP_PX.max(lines.iter().map(length).sum::<f64>() / MAX_SEGMENTS);
    for (i, line) in lines.iter().enumerate() {
        if i % 16 == 0 && !progress(0.82) { return false; }
        let (a, b) = *line;
        let n = ((length(line) / step).ceil() as usize).max(1);
        let at = |k: usize| {
            let t = k as f64 / n as f64;
            let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            // The surface is bilinear even with spline interpolation on, and the line lies on it.
            let z = match v.ctx.field {
                Field::Grid(g) => g.sample_linear(x, y),
                field => field.sample(x, y),
            };
            z.map(|z| v.pt(x, y, z))
        };
        let mut last = at(0);
        for k in 1..=n {
            let next = at(k);
            if let (Some(p), Some(q)) = (last, next) {
                items.push(((p.1 + q.1) / 2.0 - bias, Op::Polyline { pts: vec![p.0, q.0], color: col, width: lw }));
            }
            last = next;
        }
    }
    true
}

/// Bounding box and scale of the bird's-eye view, drawn over the surface.
fn frame(v: &View, ops: &mut Vec<Op>) {
    let s = v.ctx.s;
    let (col, lw) = (s.fg, s.grid_line_width.max(1) as f32);
    let m = &v.map;
    let (wd, hd, hz) = (m.wd, m.hd, m.hz);
    let font = s.axis_scale_font();
    let wext = |t: &str| measure_text(t, font).0;
    let hext = |t: &str| measure_text(t, font).1;
    let (x1, y1) = (m.x0 + m.xr, m.y0 + m.yr);
    let len = s.grid_line_length as f64;
    let out = (-len).max(0.0) + 4.0;
    let label = |ops: &mut Vec<Op>, p: (f64, f64, f64), text: String, anchor: Anchor| {
        let (q, _) = v.proj.at(p.0, p.1, p.2);
        push_text(ops, v.bounds, q.0, q.1, &text, font, col, anchor);
    };
    let zs = ticks(m.zlo, m.zlo + m.zr, pick_step(m.zlo, m.zlo + m.zr, s.memory_z, hz, &hext));
    if let Some((c, k)) = &m.cyl {
        let (cx, cy, r) = (wd / 2.0, hd / 2.0, wd / 2.0);
        if s.grid_box {
            for z in [0.0, hz] {
                let pts = (0..=72).map(|i| {
                    let a = TAU * i as f64 / 72.0;
                    v.proj.at(cx + r * a.cos(), cy + r * a.sin(), z).0
                });
                ops.push(Op::Polyline { pts: pts.collect(), color: col, width: lw });
            }
            for x in [cx - r, cx + r] {
                v.line(ops, (x, cy, 0.0), (x, cy, hz), col, lw);
            }
        }
        if !(s.grid_scale || s.axis_scale) { return; }
        let rt = ticks(c.r_min, c.r_max, pick_step(c.r_min, c.r_max, s.memory_x, r, &wext));
        let tt = ticks(c.th_min, c.th_max, pick_step(c.th_min, c.th_max, s.memory_y, wd * 1.5, &wext));
        let span = c.th_max - c.th_min;
        for &t in tt.iter().filter(|&&t| span != 0.0 && (t - c.th_max).abs() > span.abs() * 1e-9) {
            let a = (t - c.th_min) / span * TAU;
            let (ca, sa) = (a.cos(), a.sin());
            if s.grid_scale {
                v.line(ops, (cx + r * ca, cy + r * sa, 0.0), (cx + (r - len) * ca, cy + (r - len) * sa, 0.0), col, lw);
            }
            if s.axis_scale {
                let d = r + out + hext("0") as f64 / 2.0 + 2.0;
                label(ops, (cx + d * ca, cy + d * sa, 0.0), fmt_num(t), Anchor::Center);
            }
        }
        for &rv in &rt {
            let x = cx + (rv - c.r_min) * k;
            if s.grid_scale {
                v.line(ops, (x, cy, 0.0), (x, cy + len, 0.0), col, lw);
            }
            if s.axis_scale {
                label(ops, (x, cy - out - 4.0, 0.0), fmt_num(rv), Anchor::Center);
            }
        }
        for &z in &zs {
            let wz = m.z(z);
            if s.grid_scale {
                let e = v.proj.at(cx, cy, wz).0;
                ops.push(Op::Polyline { pts: vec![e, (e.0 + len as f32, e.1)], color: col, width: lw });
            }
            if s.axis_scale {
                label(ops, (cx, cy, wz), fmt_num(z), Anchor::MiddleRight);
            }
        }
        return;
    }
    let xt = ticks(m.x0, x1, pick_step(m.x0, x1, s.memory_x, wd, &wext));
    let yt = ticks(m.y0, y1, pick_step(m.y0, y1, s.memory_y, hd, &hext));
    if s.grid_box {
        let c = [(0.0, 0.0), (wd, 0.0), (wd, hd), (0.0, hd)];
        for i in 0..4 {
            let (a, b) = (c[i], c[(i + 1) % 4]);
            v.line(ops, (a.0, a.1, 0.0), (b.0, b.1, 0.0), col, lw);
            v.line(ops, (a.0, a.1, hz), (b.0, b.1, hz), col, lw);
            v.line(ops, (a.0, a.1, 0.0), (a.0, a.1, hz), col, lw);
        }
    }
    if !(s.grid_scale || s.axis_scale) {
        return;
    }
    // scale along the two floor edges nearest to the viewer and the leftmost vertical edge
    let corners = [(0.0, 0.0), (wd, 0.0), (wd, hd), (0.0, hd)];
    let near = corners.iter().copied().min_by(|a, b| v.proj.at(a.0, a.1, 0.0).1.total_cmp(&v.proj.at(b.0, b.1, 0.0).1)).unwrap_or((0.0, 0.0));
    let left = corners.iter().copied().min_by(|a, b| v.proj.at(a.0, a.1, 0.0).0.0.total_cmp(&v.proj.at(b.0, b.1, 0.0).0.0)).unwrap_or((0.0, 0.0));
    let (sx, sy) = (if near.0 == 0.0 { -1.0 } else { 1.0 }, if near.1 == 0.0 { -1.0 } else { 1.0 });
    for &x in &xt {
        let wx = m.xy(x, m.y0).0;
        if s.grid_scale {
            v.line(ops, (wx, near.1, 0.0), (wx, near.1 - sy * len, 0.0), col, lw);
        }
        if s.axis_scale {
            label(ops, (wx, near.1 + sy * (out + 4.0), 0.0), fmt_num(x), Anchor::Center);
        }
    }
    for &y in &yt {
        let wy = m.xy(m.x0, y).1;
        if s.grid_scale {
            v.line(ops, (near.0, wy, 0.0), (near.0 - sx * len, wy, 0.0), col, lw);
        }
        if s.axis_scale {
            label(ops, (near.0 + sx * (out + 4.0), wy, 0.0), fmt_num(y), Anchor::Center);
        }
    }
    for &z in &zs {
        let wz = m.z(z);
        if s.axis_scale {
            label(ops, (left.0, left.1, wz), fmt_num(z), Anchor::MiddleRight);
        }
        if s.grid_scale {
            let e = v.proj.at(left.0, left.1, wz).0;
            ops.push(Op::Polyline { pts: vec![e, (e.0 + len as f32, e.1)], color: col, width: lw });
        }
    }
}

/// How far the axis arrows reach beyond the plate, in pixels on the image, measured from the original.
const AXIS_OVERSHOOT: f32 = 30.0;
const Z_AXIS_OVERSHOOT: f32 = 19.0;

fn axes(v: &View, ops: &mut Vec<Op>) {
    let s = v.ctx.s;
    let m = &v.map;
    let (col, lw, head) = (s.fg, s.axis_line_width.max(1) as f32, s.axis_head_size as f32);
    let cyl = m.cyl.is_some();
    let o = if cyl {
        (m.wd / 2.0, m.hd / 2.0)
    } else {
        m.xy(0f64.clamp(m.x0, m.x0 + m.xr), 0f64.clamp(m.y0, m.y0 + m.yr))
    };
    let (xs, ys) = if s.axis_cross && !cyl { ((0.0, o.1), (o.0, 0.0)) } else { (o, o) };
    let ends = [
        Some(((xs.0, xs.1, 0.0), (m.wd, o.1, 0.0), &s.title_x, AXIS_OVERSHOOT)),
        // the original has a single in-plane axis in cylindrical coordinates
        (!cyl).then_some(((ys.0, ys.1, 0.0), (o.0, m.hd, 0.0), &s.title_y, AXIS_OVERSHOOT)),
        Some(((o.0, o.1, 0.0), (o.0, o.1, m.hz), &s.title_z, Z_AXIS_OVERSHOOT)),
    ];
    for (a, b, title, over) in ends.into_iter().flatten() {
        let (pa, plate) = (v.proj.at(a.0, a.1, a.2).0, v.proj.at(b.0, b.1, b.2).0);
        let (dx, dy) = (plate.0 - pa.0, plate.1 - pa.1);
        let n = dx.hypot(dy).max(1e-3);
        let pb = (plate.0 + dx / n * over, plate.1 + dy / n * over);
        ops.push(Op::Polyline { pts: vec![pa, pb], color: col, width: lw });
        arrow_head(ops, pa, pb, head, col);
        if s.axis_title {
            push_text(ops, v.bounds, pb.0 + dx / n * 8.0, pb.1 + dy / n * 8.0, title, s.axis_title_font(), col, Anchor::Center);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::super::*;
    use super::{faces_viewer, grid_skirts, mesh_skirts, neighbour_column, reduce_mesh, reduced_grid_surface, reduced_sparse_grid_surface, surface_mesh, Items, Map, View, MAX_POLYS};

    fn land() -> Settings {
        let mut s = settings();
        s.land = true;
        s.land_height = 40;
        s.eta = 30;
        s.phi = 60;
        s.surface = true;
        s
    }

    #[test]
    fn projection_fits_inside_the_image() {
        let f = grid(6, 6, |x, y| Some(x * y));
        let s = land();
        let b = build_scene(&f, &s, &[]).unwrap();
        assert!(!b.probe.is_flat());
        let (w, h) = (b.scene.width as f32, b.scene.height as f32);
        for op in &b.scene.ops {
            if let Op::Polygon { pts, .. } = op {
                for p in pts {
                    assert!(p.0 >= -1.0 && p.0 <= w + 1.0 && p.1 >= -1.0 && p.1 <= h + 1.0, "{p:?} outside {w}x{h}");
                }
            }
        }
    }

    #[test]
    fn dense_land_surface_checks_cancellation_during_cell_generation() {
        let f = grid(100, 100, |x, y| Some(x + y));
        let s = land();
        let ctx = Ctx::new(&f, &s);
        let lay = Layout::new(&f, &s).unwrap();
        let mut ops = Vec::new();
        let mut checks = 0;
        let completed = super::draw_with_progress(&ctx, &lay, &mut ops, |p| {
            checks += 1;
            !(p > 0.2)
        });
        assert!(!completed);
        assert!(checks > 3, "cancellation should happen inside the surface loop");
    }

    #[test]
    fn surface_off_adds_solid_walls_instead_of_lines() {
        let f = grid(6, 6, |x, y| Some(x + y));
        let on = build_scene(&f, &land(), &[]).unwrap();
        let polys_on = count_ops(&on.scene.ops, |o| matches!(o, Op::Polygon { .. }));
        assert!(polys_on >= 25);
        let mut s = land();
        s.surface = false;
        let off = build_scene(&f, &s, &[]).unwrap();
        assert!(count_ops(&off.scene.ops, |o| matches!(o, Op::Polygon { .. })) > polys_on);
        assert_eq!(count_ops(&off.scene.ops, |o| matches!(o, Op::Polyline { .. })), 0);
    }

    /// A triangulated strip of two rows of nodes, `n` cells long.
    fn strip_mesh(n: usize) -> Field {
        use crate::engine::data::{DataFile, Point};
        let mut df = DataFile::default();
        for j in 0..2 {
            for i in 0..=n {
                df.points.push(Point { x: Some(i as f64), y: Some(j as f64), z: Some(0.5), u: None, v: None });
            }
        }
        df.mesh_auto = true;
        let (f, _) = crate::engine::field::build(&df, &land()).unwrap();
        assert!(matches!(f, Field::Mesh(_)));
        f
    }

    /// Run `draw` on a view of `f` spanning `size` world pixels with x and y in 0..=`range`.
    fn items_of(f: &Field, range: f64, size: f64, draw: impl FnOnce(&View, &mut Items) -> bool) -> Items {
        let s = land();
        let ctx = Ctx::new(f, &s);
        let layout = Layout::new(f, &s).unwrap();
        let proj = layout.proj.unwrap();
        let v = View { ctx: &ctx, map: Map { x0: 0.0, y0: 0.0, xr: range, yr: range, wd: size, hd: size, cyl: None, flip: false, zlo: 0.0, zr: 1.0, hz: 40.0 }, proj, bounds: (layout.w, layout.h) };
        let mut items = Vec::new();
        assert!(draw(&v, &mut items));
        items
    }

    #[test]
    fn fragmented_walls_stay_within_their_budget_and_can_be_cancelled() {
        let f = grid(400, 400, |x, y| (x as usize % 3 != 0 || y as usize % 3 != 0).then_some(0.5));
        let Field::Grid(g) = &f else { unreachable!() };
        let items = items_of(&f, 399.0, 800.0, |v, items| grid_skirts(v, g, items, &mut |_| true));
        assert!(!items.is_empty());
        assert!(items.len() <= MAX_POLYS / 4, "{} walls", items.len());
        items_of(&f, 399.0, 800.0, |v, items| !grid_skirts(v, g, items, &mut |_| false));
    }

    #[test]
    fn theta_seam_looks_at_the_column_on_the_other_side() {
        assert_eq!(neighbour_column(None, 9, true), Some(7));
        assert_eq!(neighbour_column(Some(8), 9, true), Some(0));
        assert_eq!(neighbour_column(None, 9, false), None);
        assert_eq!(neighbour_column(Some(8), 9, false), None);
        assert_eq!(neighbour_column(Some(3), 9, true), Some(3));
    }

    #[test]
    fn mesh_walls_come_in_the_same_order_every_time() {
        let f = strip_mesh(50);
        let Field::Mesh(m) = &f else { unreachable!() };
        let depths = || items_of(&f, 50.0, 400.0, |v, items| mesh_skirts(v, m, items, &mut |_| true)).iter().map(|i| i.0).collect::<Vec<_>>();
        let first = depths();
        assert!(!first.is_empty());
        assert_eq!(first, depths());
    }

    #[test]
    fn mesh_lines_stay_within_the_limit_on_a_large_image() {
        let f = grid(200, 200, |x, y| Some((x + y) / 398.0));
        let items = items_of(&f, 199.0, 6000.0, |v, items| surface_mesh(v, items, &mut |_| true));
        assert!(!items.is_empty());
        assert!(items.len() <= MAX_POLYS, "{} segments", items.len());
    }

    #[test]
    fn thinned_mesh_lines_cover_the_whole_mesh() {
        let f = strip_mesh(10_000);
        let items = items_of(&f, 10_000.0, 1000.0, |v, items| surface_mesh(v, items, &mut |_| true));
        assert!(items.len() <= MAX_POLYS, "{} segments", items.len());
        let xs: Vec<f32> = items.iter().filter_map(|i| match &i.1 { Op::Polyline { pts, .. } => Some(pts[0].0), _ => None }).collect();
        let (lo, hi) = xs.iter().fold((f32::MAX, f32::MIN), |(l, h), &x| (l.min(x), h.max(x)));
        let span = hi - lo;
        for part in 0..10 {
            let (a, b) = (lo + span * part as f32 / 10.0, lo + span * (part + 1) as f32 / 10.0);
            assert!(xs.iter().any(|&x| x >= a && x <= b), "no mesh line in part {part} of the mesh");
        }
    }

    #[test]
    fn walls_stand_only_on_edges_that_face_the_viewer() {
        let f = grid(3, 3, |_, _| Some(1.0));
        for (eta, near_x_wall) in [(30, Some(1.0)), (-30, Some(0.0)), (0, None)] {
            let mut s = land();
            s.eta = eta;
            let ctx = Ctx::new(&f, &s);
            let layout = Layout::new(&f, &s).unwrap();
            let proj = layout.proj.unwrap();
            let v = View { ctx: &ctx, map: Map { x0: 0.0, y0: 0.0, xr: 2.0, yr: 2.0, wd: proj.wd, hd: proj.hd, cyl: None, flip: false, zlo: 0.0, zr: 1.0, hz: 100.0 }, proj, bounds: (layout.w, layout.h) };
            let (w, h) = (proj.wd, proj.hd);
            let inner = (w / 2.0, h / 2.0);
            assert!(faces_viewer(&v, (0.0, 0.0), (w, 0.0), inner), "front edge eta {eta}");
            assert!(!faces_viewer(&v, (0.0, h), (w, h), inner), "back edge eta {eta}");
            assert_eq!(faces_viewer(&v, (w, 0.0), (w, h), inner), near_x_wall == Some(1.0), "right edge eta {eta}");
            assert_eq!(faces_viewer(&v, (0.0, 0.0), (0.0, h), inner), near_x_wall == Some(0.0), "left edge eta {eta}");
        }
    }

    #[test]
    fn subdivision_follows_the_height_of_each_cell() {
        let flat = build_scene(&grid(3, 3, |_, _| Some(1.0)), &land(), &[]).unwrap();
        let steep = build_scene(&grid(3, 3, |x, _| Some(if x > 1.0 { 100.0 } else { 0.0 })), &land(), &[]).unwrap();
        let polys = |b: &crate::engine::draw::BuiltScene| count_ops(&b.scene.ops, |o| matches!(o, Op::Polygon { .. }));
        assert!(polys(&steep) > polys(&flat));
    }

    #[test]
    fn mesh_lines_run_over_the_surface_and_box_comes_last() {
        let f = grid(4, 4, |x, _| Some(x));
        let mut s = land();
        s.grid = true;
        s.grid_mesh = true;
        s.grid_box = true;
        let b = build_scene(&f, &s, &[]).unwrap();
        let ops = &b.scene.ops;
        let last_poly = ops.iter().rposition(|o| matches!(o, Op::Polygon { .. })).unwrap();
        let first_line = ops.iter().position(|o| matches!(o, Op::Polyline { .. })).unwrap();
        assert!(first_line < last_poly, "mesh lines are sorted among the polygons");
        assert!(matches!(ops.last(), Some(Op::Polyline { .. })), "the box is drawn over the surface");
    }

    #[test]
    fn cylinder_ignores_eta() {
        let f = grid(9, 5, |x, y| Some(x + y));
        let mut s = land();
        s.cylindrical = true;
        s.eta = 60;
        let shear_free = Layout::new(&f, &s).unwrap().proj.unwrap().w;
        assert_eq!(shear_free, s.size as f64);
    }

    #[test]
    fn painter_draws_far_cells_first() {
        let f = grid(3, 3, |_, _| Some(1.0));
        let mut s = land();
        s.eta = 0;
        s.phi = 45;
        let b = build_scene(&f, &s, &[]).unwrap();
        let ys: Vec<f32> = b
            .scene
            .ops
            .iter()
            .filter_map(|o| if let Op::Polygon { pts, .. } = o { Some(pts.iter().map(|p| p.1).sum::<f32>() / pts.len() as f32) } else { None })
            .collect();
        // looking from the south: far (north) cells are higher on screen, i.e. smaller y, and come first
        assert!(ys.first().unwrap() < ys.last().unwrap());
    }

    #[test]
    fn taller_data_makes_taller_image() {
        let f = grid(4, 4, |x, y| Some(x + y));
        let a = image_size(&f, &land()).unwrap();
        let mut s = land();
        s.land_height = 200;
        let b = image_size(&f, &s).unwrap();
        assert!(b.1 > a.1);
    }

    #[test]
    fn land_with_axes_scatter_and_cylinder() {
        let f = grid(6, 6, |x, y| Some(x + y));
        let mut s = land();
        s.axis = true;
        s.axis_title = true;
        s.grid = true;
        s.grid_mesh = true;
        s.grid_box = true;
        s.grid_scale = true;
        s.axis_scale = true;
        s.legend = true;
        assert!(build_scene(&f, &s, &[]).is_ok());
        s.scatter = true;
        s.scatter_mode = 1;
        let b = build_scene(&f, &s, &[]).unwrap();
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Ellipse { .. })), 36);
        s.scatter = false;
        s.cylindrical = true;
        let b = build_scene(&f, &s, &[]).unwrap();
        assert!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polygon { .. })) >= 25);
    }

    #[test]
    fn land_panel_and_mesh() {
        use crate::engine::data::{DataFile, PanelData, Point};
        let s = land();
        let p = Field::Panel(PanelData { rows: 2, cols: 3, z: vec![Some(1.0), Some(2.0), None, Some(4.0), Some(5.0), Some(6.0)] });
        let b = build_scene(&p, &s, &[]).unwrap();
        assert!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polygon { .. })) > 5);
        let mut wire = s.clone();
        wire.surface = false;
        let b = build_scene(&p, &wire, &[]).unwrap();
        assert!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polygon { .. })) > 5);
        let mut df = DataFile::default();
        for (x, y) in [(0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (10.0, 10.0)] {
            df.points.push(Point { x: Some(x), y: Some(y), z: Some(x + y), u: None, v: None });
        }
        df.mesh_auto = true;
        let (m, _) = crate::engine::field::build(&df, &s).unwrap();
        let b = build_scene(&m, &s, &[]).unwrap();
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polygon { .. })), 2);
    }

    #[test]
    fn reduced_panel_last_tile_stops_at_data_boundary() {
        use crate::engine::data::PanelData;
        let (rows, cols) = (251, 251);
        let f = Field::Panel(PanelData { rows, cols, z: vec![Some(1.0); rows * cols] });
        let s = land();
        let ctx = Ctx::new(&f, &s);
        let layout = Layout::new(&f, &s).unwrap();
        let proj = layout.proj.unwrap().placed(layout.px, layout.py);
        let view = View { ctx: &ctx, map: Map { x0:0.0, y0:0.0, xr:cols as f64, yr:rows as f64, wd:proj.wd, hd:proj.hd, cyl:None, flip:true, zlo:1.0, zr:1.0, hz:s.land_height as f64 }, proj, bounds:(layout.w,layout.h) };
        let mut items = Vec::new();
        assert!(super::panel_surface(&view, &mut items, &mut |_| true));
        let expected: Vec<_> = [(249.0,249.0),(251.0,249.0),(251.0,251.0),(249.0,251.0)].into_iter().map(|(x,y)| view.pt(x,y,1.0).0).collect();
        let Op::Polygon { pts, .. } = &items.last().unwrap().1 else { panic!("expected last tile") };
        assert_eq!(pts, &expected, "the final tile must not extend to 252");
        assert_eq!(items.len(), 84 * 84, "flat boundary walls have zero height and should not be emitted");
    }

    #[test]
    fn oversized_mesh_is_retriangulated_without_triangle_skips() {
        use crate::engine::data::{DataFile, Point};
        let mut df = DataFile::default();
        let n = 350usize;
        for j in 0..n {
            for i in 0..n {
                df.points.push(Point { x:Some(i as f64), y:Some(j as f64), z:Some((i+j) as f64), u:None, v:None });
            }
        }
        df.mesh_auto = true;
        let (Field::Mesh(m), _) = crate::engine::field::build(&df, &land()).unwrap() else { panic!("expected mesh") };
        let mut incomplete_source = m.clone();
        assert!(m.tris.len() > MAX_POLYS);
        let mut progress = |_| true;
        let reduced = reduce_mesh(&m, &mut progress).expect("reduction completes");
        assert!(reduced.2.len() <= MAX_POLYS);
        for corner in [(0.0,0.0),(349.0,0.0),(0.0,349.0),(349.0,349.0)] {
            assert!(reduced.0.contains(&corner), "source hull corner {corner:?} was discarded");
        }
        let covered = |p:(f64,f64)| reduced.2.iter().any(|t| {
            let (a,b,c)=(reduced.0[t[0]],reduced.0[t[1]],reduced.0[t[2]]);
            let cross=|u:(f64,f64),v:(f64,f64),w:(f64,f64)| (v.0-u.0)*(w.1-u.1)-(v.1-u.1)*(w.0-u.0);
            let x=cross(a,b,p); let y=cross(b,c,p); let z=cross(c,a,p);
            (x>=-1e-8 && y>=-1e-8 && z>=-1e-8) || (x<=1e-8 && y<=1e-8 && z<=1e-8)
        });
        for p in [(0.01,0.01),(348.99,0.01),(0.01,348.99),(348.99,348.99),(174.5,174.5),(100.2,201.7)] {
            assert!(covered(p), "retriangulated mesh leaves a hole at {p:?}");
        }
        let mut progress_values=Vec::new();
        let started=std::time::Instant::now();
        let b=super::super::build_scene_with_progress(&Field::Mesh(m),&land(),&[],|p| { progress_values.push(p); true }).unwrap();
        let large_draw_elapsed=started.elapsed();
        let poly_count=count_ops(&b.scene.ops,|o|matches!(o,Op::Polygon{..}));
        assert!(poly_count <= MAX_POLYS);
        assert!(progress_values.windows(2).all(|w| w[0] <= w[1]),"progress must be monotonic: {progress_values:?}");
        let emf_started=std::time::Instant::now();
        let large_emf=crate::engine::export::emf::encode_emf(&b.scene);
        let large_emf_elapsed=emf_started.elapsed();

        let mut incomplete = incomplete_source;
        let missing = incomplete.z.len()/2;
        incomplete.z[missing]=None;
        let expected_valid = incomplete.tris.iter().filter(|t| t.iter().all(|&i| incomplete.z[i].is_some())).count();
        assert!(expected_valid>MAX_POLYS);
        let b=build_scene(&Field::Mesh(incomplete),&land(),&[]).unwrap();
        assert_eq!(count_ops(&b.scene.ops,|o|matches!(o,Op::Polygon{..})),expected_valid,
            "incomplete large meshes keep every valid source triangle rather than bridging holes or reducing");

        let mut triangle = DataFile::default();
        let side = 310usize;
        for j in 0..side {
            for i in 0..(side-j) {
                triangle.points.push(Point { x:Some(i as f64), y:Some(j as f64), z:Some(1.0), u:None, v:None });
            }
        }
        triangle.mesh_auto = true;
        let (Field::Mesh(source),_) = crate::engine::field::build(&triangle,&land()).unwrap() else { panic!("expected triangular mesh") };
        assert!(source.tris.len()>MAX_POLYS);
        let mut progress=|_| true;
        let reduced=reduce_mesh(&source,&mut progress).unwrap();
        assert!(reduced.2.len()<=MAX_POLYS);
        assert!(reduced.0.contains(&(0.0,0.0)) && reduced.0.contains(&((side-1) as f64,0.0)) && reduced.0.contains(&(0.0,(side-1) as f64)));
        assert!(reduced.0.iter().all(|&(x,y)| x+y <= (side-1) as f64 + 1e-8), "reduction must not extend beyond the source hull");
        for p in [(0.1,0.1),(100.2,100.3),(200.1,50.1),(0.1,308.7)] {
            let covered = reduced.2.iter().any(|t| {
                let (a,b,c)=(reduced.0[t[0]],reduced.0[t[1]],reduced.0[t[2]]);
                let cross=|u:(f64,f64),v:(f64,f64),w:(f64,f64)| (v.0-u.0)*(w.1-u.1)-(v.1-u.1)*(w.0-u.0);
                let (x,y,z)=(cross(a,b,p),cross(b,c,p),cross(c,a,p));
                (x>=-1e-8 && y>=-1e-8 && z>=-1e-8) || (x<=1e-8 && y<=1e-8 && z<=1e-8)
            });
            assert!(covered,"triangular hull has a gap at {p:?}");
        }

        let mut under=DataFile::default();
        let side=140usize;
        for j in 0..side { for i in 0..side {
            under.points.push(Point { x:Some(i as f64),y:Some(j as f64),z:Some((i+j) as f64),u:None,v:None });
        }}
        under.mesh_auto=true;
        let (field,_) = crate::engine::field::build(&under,&land()).unwrap();
        let start=std::time::Instant::now();
        let under_scene=build_scene(&field,&land(),&[]).unwrap();
        let under_draw_elapsed=start.elapsed();
        let start=std::time::Instant::now();
        let under_emf=crate::engine::export::emf::encode_emf(&under_scene.scene);
        let under_emf_elapsed=start.elapsed();
        eprintln!("oversized mesh benchmark: {} polygons, draw {:?}, EMF {} bytes in {:?}; below-limit {} polygons, draw {:?}, EMF {} bytes in {:?}",
            poly_count,large_draw_elapsed,large_emf.len(),large_emf_elapsed,
            count_ops(&under_scene.scene.ops,|o|matches!(o,Op::Polygon{..})),under_draw_elapsed,under_emf.len(),under_emf_elapsed);
    }

    #[test]
    fn reduction_preserves_large_curved_mesh_boundary() {
        use crate::engine::data::{DataFile, Point};
        use std::collections::HashSet;
        let mut df = DataFile::default();
        let count = MAX_POLYS / 3 + 1;
        for i in 0..count {
            let a = std::f64::consts::TAU * i as f64 / count as f64;
            df.points.push(Point { x:Some(a.cos()), y:Some(a.sin()), z:Some(1.0), u:None, v:None });
        }
        for y in 0..150 { for x in 0..150 {
            df.points.push(Point { x:Some(-0.6 + x as f64 * 1.2 / 149.0), y:Some(-0.6 + y as f64 * 1.2 / 149.0), z:Some(1.0), u:None, v:None });
        }}
        df.mesh_auto = true;
        let (Field::Mesh(mesh), _) = crate::engine::field::build(&df, &land()).unwrap() else { panic!("expected mesh") };
        assert!(mesh.tris.len() > MAX_POLYS);
        let (nodes, _, tris) = reduce_mesh(&mesh, &mut |_| true).unwrap();
        let retained: HashSet<_> = nodes.iter().map(|p| (p.0.to_bits(), p.1.to_bits())).collect();
        for &(x,y) in &mesh.nodes[..count] {
            assert!(retained.contains(&(x.to_bits(), y.to_bits())), "a hull vertex was removed");
        }
        assert_eq!(tris.len(), count - 2, "the convex boundary must be fully triangulated without holes");
    }

    #[test]
    fn extremely_large_boundary_keeps_original_mesh() {
        use crate::engine::data::{DataFile, Point};
        let mut df = DataFile::default();
        let count = MAX_POLYS / 2 + 1;
        for i in 0..count {
            let a = std::f64::consts::TAU * i as f64 / count as f64;
            df.points.push(Point { x:Some(a.cos()), y:Some(a.sin()), z:Some(1.0), u:None, v:None });
        }
        for y in 0..150 { for x in 0..150 {
            df.points.push(Point { x:Some(-0.6 + x as f64 * 1.2 / 149.0), y:Some(-0.6 + y as f64 * 1.2 / 149.0), z:Some(1.0), u:None, v:None });
        }}
        df.mesh_auto = true;
        let (Field::Mesh(mesh), _) = crate::engine::field::build(&df, &land()).unwrap() else { panic!("expected mesh") };
        assert!(mesh.tris.len() > MAX_POLYS);
        let (nodes, values, tris) = reduce_mesh(&mesh, &mut |_| true).unwrap();
        assert_eq!(nodes, mesh.nodes);
        assert_eq!(values, mesh.z);
        assert_eq!(tris, mesh.tris);
    }

    #[test]
    fn reduced_mesh_keeps_single_node_peaks_within_the_polygon_budget() {
        use crate::engine::data::{DataFile, Point};
        let mut df = DataFile::default();
        let (peak, valley) = ((173usize, 211usize), (321usize, 67usize));
        for j in 0..400usize { for i in 0..400usize {
            // Noise makes every block contribute two distinct extrema, the worst case for the budget.
            let noise = ((i * 7919 + j * 104_729) % 1009) as f64 / 1009.0;
            let z = if (i, j) == peak { 1000.0 } else if (i, j) == valley { -1000.0 } else { noise };
            let wobble = ((i * 31 + j * 17) % 13) as f64 * 0.01;
            df.points.push(Point { x:Some(i as f64 + wobble), y:Some(j as f64 + 0.5 * wobble), z:Some(z), u:None, v:None });
        }}
        df.mesh_auto = true;
        let (Field::Mesh(mesh), _) = crate::engine::field::build(&df, &land()).unwrap() else { panic!("expected mesh") };
        assert!(mesh.tris.len() > MAX_POLYS);
        let (_, z, tris) = reduce_mesh(&mesh, &mut |_| true).unwrap();
        assert!(tris.len() <= MAX_POLYS, "{} triangles", tris.len());
        assert!(z.contains(&Some(1000.0)) && z.contains(&Some(-1000.0)), "a peak was lost");
    }

    #[test]
    fn oversized_dense_grid_keeps_extrema_in_a_contiguous_triangular_surface() {
        use std::time::Instant;
        let peaks = [(173usize, 211usize, 1000.0), (321, 367, -1000.0)];
        let f = grid(450, 450, |x, y| {
            let (i, j) = (x as usize, y as usize);
            Some(peaks.iter().find(|&&(px, py, _)| (px, py) == (i, j)).map_or(0.0, |p| p.2))
        });
        let s = land();
        let ctx = Ctx::new(&f, &s);
        let layout = Layout::new(&f, &s).unwrap();
        let proj = layout.proj.unwrap().placed(layout.px, layout.py);
        let (x0, x1, y0, y1) = f.extent().unwrap();
        let zr = ctx.zr.unwrap();
        let view = View {
            ctx: &ctx,
            map: Map { x0, y0, xr: x1 - x0, yr: y1 - y0, wd: proj.wd, hd: proj.hd, cyl: None, flip: false, zlo: zr.0, zr: (zr.1 - zr.0).max(1e-300), hz: s.land_height.max(1) as f64 },
            proj,
            bounds: (layout.w, layout.h),
        };
        let start = Instant::now();
        let mut items = Vec::new();
        let mut progress = |_| true;
        assert!(reduced_grid_surface(&view, match &f { Field::Grid(g) => g, _ => unreachable!() }, &mut items, &mut progress).unwrap());
        let elapsed = start.elapsed();
        assert!(items.len() <= MAX_POLYS, "{} ops exceeds {MAX_POLYS}", items.len());
        for &(i, j, z) in &peaks {
            let target = view.pt(i as f64, j as f64, z).0;
            assert!(items.iter().any(|(_, op)| matches!(op, Op::Polygon { pts, .. } if pts.contains(&target))), "extremum at ({i}, {j}) was not retained");
        }

        let below = grid(200, 200, |_, _| Some(0.0));
        let below_scene = build_scene(&below, &s, &[]).unwrap();
        let below_emf = crate::engine::export::emf::encode_emf(&below_scene.scene).len();
        let scene = build_scene(&f, &s, &[]).unwrap();
        let emf = crate::engine::export::emf::encode_emf(&scene.scene).len();
        eprintln!("land grid benchmark: 200x200 points, {} scene ops, {below_emf} B EMF; 450x450 points, {} scene ops, {emf} B EMF, surface generation {elapsed:?}", below_scene.scene.ops.len(), scene.scene.ops.len());
    }

    #[test]
    fn oversized_sparse_grid_keeps_extrema_without_filling_missing_cells() {
        let peaks = [(300usize, 300usize, 1000.0), (400, 400, -1000.0)];
        let f = grid(450, 450, |x, y| {
            let (i, j) = (x as usize, y as usize);
            if (i,j) == (20,20) { None }
            else { Some(peaks.iter().find(|&&(px,py,_)| (px,py)==(i,j)).map_or(0.0, |p| p.2)) }
        });
        let s = land();
        let ctx = Ctx::new(&f, &s);
        let layout = Layout::new(&f, &s).unwrap();
        let proj = layout.proj.unwrap().placed(layout.px, layout.py);
        let (x0,x1,y0,y1)=f.extent().unwrap();
        let zr=ctx.zr.unwrap();
        let view=View { ctx:&ctx, map:Map{x0,y0,xr:x1-x0,yr:y1-y0,wd:proj.wd,hd:proj.hd,cyl:None,flip:false,zlo:zr.0,zr:(zr.1-zr.0).max(1e-300),hz:s.land_height.max(1) as f64}, proj, bounds:(layout.w,layout.h) };
        let mut items=Vec::new();
        let mut progress=|_| true;
        assert!(reduced_sparse_grid_surface(&view, match &f { Field::Grid(g)=>g,_=>unreachable!() }, &mut items, &mut progress));
        assert!(items.len() <= MAX_POLYS);
        for &(i,j,z) in &peaks {
            let target=view.pt(i as f64,j as f64,z).0;
            assert!(items.iter().any(|(_,op)| matches!(op,Op::Polygon{pts,..} if pts.contains(&target))), "lost extremum ({i},{j})");
        }
        let hole=view.pt(20.5,20.5,0.0).0;
        let contains=|poly:&[(f32,f32)]| {
            let mut inside=false;
            for k in 0..poly.len() {
                let (a,b)=(poly[k],poly[(k+1)%poly.len()]);
                if (a.1>hole.1)!=(b.1>hole.1) && hole.0 < (b.0-a.0)*(hole.1-a.1)/(b.1-a.1)+a.0 { inside=!inside; }
            }
            inside
        };
        assert!(!items.iter().any(|(_,op)| matches!(op,Op::Polygon{pts,..} if contains(pts))), "missing cell was covered by a coarse triangle");
    }

    #[test]
    fn oversized_sparse_grid_can_cancel_during_pre_scan() {
        let f = grid(450, 450, |x, y| {
            if x == 20.0 && y == 20.0 { None } else { Some(0.0) }
        });
        let s = land();
        let ctx = Ctx::new(&f, &s);
        let layout = Layout::new(&f, &s).unwrap();
        let proj = layout.proj.unwrap().placed(layout.px, layout.py);
        let (x0,x1,y0,y1) = f.extent().unwrap();
        let zr = ctx.zr.unwrap();
        let view = View { ctx: &ctx, map: Map { x0,y0,xr:x1-x0,yr:y1-y0,wd:proj.wd,hd:proj.hd,cyl:None,flip:false,zlo:zr.0,zr:(zr.1-zr.0).max(1e-300),hz:s.land_height.max(1) as f64 }, proj, bounds:(layout.w,layout.h) };
        let Field::Grid(g) = &f else { unreachable!() };
        let mut calls = 0;
        let mut progress = |_| { calls += 1; false };
        let mut items = Vec::new();
        assert!(!reduced_sparse_grid_surface(&view, g, &mut items, &mut progress));
        assert_eq!(calls, 1, "cancellation should be observed at the first periodic pre-scan check");
        assert!(items.is_empty());
    }

    #[test]
    fn very_sparse_grid_falls_back_to_every_valid_unit_cell() {
        let f = grid(240, 240, |x, y| {
            if (x as usize) % 8 == 0 && (y as usize) % 8 == 0 { None } else { Some(0.0) }
        });
        let s=land();
        let ctx=Ctx::new(&f,&s);
        let layout=Layout::new(&f,&s).unwrap();
        let proj=layout.proj.unwrap().placed(layout.px,layout.py);
        let (x0,x1,y0,y1)=f.extent().unwrap();
        let view=View{ctx:&ctx,map:Map{x0,y0,xr:x1-x0,yr:y1-y0,wd:proj.wd,hd:proj.hd,cyl:None,flip:false,zlo:0.0,zr:1.0,hz:s.land_height.max(1) as f64},proj,bounds:(layout.w,layout.h)};
        let Field::Grid(g)=&f else { unreachable!() };
        let sentinel=Op::Polyline{pts:vec![(0.0,0.0),(1.0,1.0)],color:Rgb::BLACK,width:1.0};
        let mut items=vec![(0.0,sentinel.clone())];
        let mut progress_values=Vec::new();
        let mut progress=|p| { progress_values.push(p); true };
        assert!(reduced_sparse_grid_surface(&view,g,&mut items,&mut progress));
        let expected=(0..239).flat_map(|j|(0..239).map(move |i|(i,j)))
            .filter(|&(i,j)|[g.at(i,j),g.at(i+1,j),g.at(i+1,j+1),g.at(i,j+1)].iter().all(Option::is_some)).count();
        assert!(expected > MAX_POLYS/4, "test must exercise the unit-cell fallback");
        assert_eq!(items.len()-1,expected,"all complete original cells must be retained");
        assert!(matches!(items[0].1,Op::Polyline{..}), "fallback must preserve items owned by the caller");
        assert!(progress_values.windows(2).all(|w| w[0] <= w[1]), "progress must be monotonic during fallback");
    }

}
