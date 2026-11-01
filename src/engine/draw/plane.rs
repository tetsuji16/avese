//! Flat views: distribution, scatter, panel, contour lines and vectors (Cartesian and cylindrical).
use super::{Ctx, Frame};
use crate::engine::field::{Field, GridField, MeshField};
use crate::engine::scene::{Op, Rgb};
use contour::ContourBuilder;
use geo_types::Coord;

const MAX_DOTS: usize = 200_000;
const MAX_LEVELS: usize = 1000;
/// sub-cells per grid cell when contouring spline data
const SPLINE_REFINE: usize = 4;
const MAX_MESH_SAMPLES: usize = 2048;
const EDGE_TOL: f64 = 1e-9;

pub(crate) struct Node {
    pub x: f64,
    pub y: f64,
    pub z: Option<f64>,
    pub u: Option<f64>,
    pub v: Option<f64>,
    /// grid indices; for meshes `i` is the node number
    pub i: usize,
    pub j: usize,
}

pub(crate) fn nodes(field: &Field) -> Box<dyn Iterator<Item = Node> + '_> {
    match field {
        Field::Grid(g) => {
            let nx = g.xs.len();
            Box::new((0..g.ys.len()).flat_map(move |j| {
                (0..nx).map(move |i| {
                    let k = j * nx + i;
                    Node { x: g.xs[i], y: g.ys[j], z: g.z[k], u: g.u.get(k).copied().flatten(), v: g.v.get(k).copied().flatten(), i, j }
                })
            }))
        }
        Field::Mesh(m) => Box::new(m.nodes.iter().enumerate().map(|(k, p)| Node {
            x: p.0,
            y: p.1,
            z: m.z[k],
            u: m.u.get(k).copied().flatten(),
            v: m.v.get(k).copied().flatten(),
            i: k,
            j: 0,
        })),
        Field::Panel(_) => Box::new(std::iter::empty()),
    }
}

pub(crate) fn dot_op(mode: u8, x: f32, y: f32, d: f32, c: Rgb) -> Op {
    let (x, y) = (x.round(), y.round());
    let r = (d / 2.0).max(0.5);
    let (fill, stroke) = if mode >= 2 { (None, Some(c)) } else { (Some(c), None) };
    if mode % 2 == 0 {
        Op::Polygon { pts: vec![(x - r, y - r), (x + r, y - r), (x + r, y + r), (x - r, y + r)], fill, stroke }
    } else {
        Op::Ellipse { cx: x, cy: y, rx: r, ry: r, fill, stroke }
    }
}

/// Distribution (grid, mesh) or panel as one image; pixels without a value stay transparent.
pub fn image(ctx: &Ctx, frame: &Frame, ops: &mut Vec<Op>) {
    let _ = image_with_progress(ctx, frame, ops, |_| true);
}

/// Build the plot image while checking for cancellation between scanlines.
pub fn image_with_progress(ctx: &Ctx, frame: &Frame, ops: &mut Vec<Op>, mut progress: impl FnMut(f32) -> bool) -> bool {
    if ctx.zr.is_none() {
        return true;
    }
    let (w, h) = (frame.w as usize, frame.h as usize);
    let mut rgba = vec![0u8; w * h * 4];
    match ctx.field {
        Field::Mesh(m) => if !mesh_image_with_progress(ctx, frame, m, &mut rgba, &mut progress) { return false; },
        _ => {
            use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
            if !progress(0.0) { return false; }
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(16).min(h.max(1));
            let rows_per = h.div_ceil(threads).max(1);
            let completed = AtomicUsize::new(0);
            let cancelled = AtomicBool::new(false);
            std::thread::scope(|scope| {
                let mut workers = Vec::new();
                for (chunk_index, chunk) in rgba.chunks_mut(rows_per * w * 4).enumerate() {
                    let completed = &completed;
                    let cancelled = &cancelled;
                    let frame = *frame;
                    workers.push(scope.spawn(move || {
                        for (row_index, row) in chunk.chunks_mut(w * 4).enumerate() {
                            if cancelled.load(Ordering::Relaxed) { break; }
                            let j = chunk_index * rows_per + row_index;
                            for i in 0..w {
                                if i % 4096 == 0 && cancelled.load(Ordering::Relaxed) { return; }
                                let p = frame.from_px_raw(frame.ox + i as f64 + 0.5, frame.oy + j as f64 + 0.5);
                                if let Some(z) = p.and_then(|(x, y)| ctx.field.sample(x, y)) {
                                    let c = ctx.color(z);
                                    row[i * 4..i * 4 + 4].copy_from_slice(&[c.0, c.1, c.2, 255]);
                                }
                            }
                            completed.fetch_add(1, Ordering::Relaxed);
                        }
                    }));
                }
                let mut reported = 0usize;
                let mut last_check = std::time::Instant::now();
                loop {
                    let now = completed.load(Ordering::Relaxed);
                    // A panicked worker cannot finish its rows; leave the loop so
                    // thread::scope can propagate the panic to the job handler.
                    if now >= h || cancelled.load(Ordering::Relaxed) || workers.iter().all(|w| w.is_finished()) { break; }
                    if now.saturating_sub(reported) >= 8 || last_check.elapsed() >= std::time::Duration::from_millis(10) {
                        reported = now;
                        last_check = std::time::Instant::now();
                        if !progress(now as f32 / h.max(1) as f32) {
                            cancelled.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            });
            if cancelled.load(Ordering::Relaxed) { return false; }
        }
    }
    if !progress(1.0) { return false; }
    ops.push(Op::Image { x: frame.ox as i32, y: frame.oy as i32, w: w as u32, h: h as u32, rgba });
    true
}

fn mesh_image_with_progress(ctx: &Ctx, frame: &Frame, m: &MeshField, rgba: &mut [u8], progress: &mut impl FnMut(f32) -> bool) -> bool {
    let (w, h) = (frame.w as i64, frame.h as i64);
    let mut rows_checked = 0usize;
    for (ti, tri) in m.tris.iter().enumerate() {
        if ti % 32 == 0 && !progress(ti as f32 / m.tris.len().max(1) as f32) { return false; }
        let (Some(z0), Some(z1), Some(z2)) = (m.z[tri[0]], m.z[tri[1]], m.z[tri[2]]) else { continue };
        let p: Vec<(f64, f64)> = tri
            .iter()
            .map(|&k| {
                let (a, b) = frame.to_px(m.nodes[k].0, m.nodes[k].1);
                (a as f64 - frame.ox, b as f64 - frame.oy)
            })
            .collect();
        let det = (p[1].1 - p[2].1) * (p[0].0 - p[2].0) + (p[2].0 - p[1].0) * (p[0].1 - p[2].1);
        if det.abs() < 1e-12 {
            continue;
        }
        let lo = |f: fn(&(f64, f64)) -> f64| p.iter().map(f).fold(f64::INFINITY, f64::min);
        let hi = |f: fn(&(f64, f64)) -> f64| p.iter().map(f).fold(f64::NEG_INFINITY, f64::max);
        let (x0, x1) = ((lo(|q| q.0) - 0.5).floor().max(0.0) as i64, (hi(|q| q.0) + 0.5).ceil().min(w as f64) as i64);
        let (y0, y1) = ((lo(|q| q.1) - 0.5).floor().max(0.0) as i64, (hi(|q| q.1) + 0.5).ceil().min(h as f64) as i64);
        for j in y0..y1 {
            rows_checked += 1;
            if rows_checked % 8 == 0 && !progress(ti as f32 / m.tris.len().max(1) as f32) { return false; }
            for i in x0..x1 {
                let (x, y) = (i as f64 + 0.5, j as f64 + 0.5);
                let w0 = ((p[1].1 - p[2].1) * (x - p[2].0) + (p[2].0 - p[1].0) * (y - p[2].1)) / det;
                let w1 = ((p[2].1 - p[0].1) * (x - p[2].0) + (p[0].0 - p[2].0) * (y - p[2].1)) / det;
                let w2 = 1.0 - w0 - w1;
                if w0 >= -1e-9 && w1 >= -1e-9 && w2 >= -1e-9 {
                    let c = ctx.color(w0 * z0 + w1 * z1 + w2 * z2);
                    let o = (j * w + i) as usize * 4;
                    rgba[o..o + 4].copy_from_slice(&[c.0, c.1, c.2, 255]);
                }
            }
        }
    }
    progress(1.0)
}

pub fn scatter(ctx: &Ctx, frame: &Frame, ops: &mut Vec<Op>) {
    let s = ctx.s;
    let total = nodes(ctx.field).count();
    let stride = total.div_ceil(MAX_DOTS).max(1);
    for n in nodes(ctx.field).step_by(stride) {
        let Some(z) = n.z else { continue };
        let (px, py) = frame.to_px(n.x, n.y);
        ops.push(dot_op(s.scatter_mode, px, py, s.dot_size.max(1) as f32, ctx.color(z)));
    }
}

// ---- contour lines ----

type Pt = (f64, f64);

/// Contour levels in increasing order.
pub fn contour_levels(ctx: &Ctx) -> Vec<f64> {
    let s = ctx.s;
    let Some((dlo, dhi)) = ctx.field.data_z_range() else { return vec![] };
    let lo = if s.toko_min_on { s.toko_min } else { dlo };
    let hi = if s.toko_max_on { s.toko_max } else { dhi };
    if !(hi > lo) {
        return vec![];
    }
    if s.toko_by_step && !s.toko_by_count {
        if !(s.toko_dt > 0.0) {
            return vec![];
        }
        let k0 = (lo / s.toko_dt).ceil();
        (0..MAX_LEVELS).map(|n| (k0 + n as f64) * s.toko_dt).take_while(|&v| v <= hi).collect()
    } else {
        let n = (s.toko_count as usize).min(MAX_LEVELS);
        (1..=n).map(|k| lo + (hi - lo) * k as f64 / (n + 1) as f64).collect()
    }
}

/// Node values of a grid; spline data is refined so that the lines follow the interpolated surface.
fn grid_samples(g: &GridField) -> (Vec<f64>, Vec<f64>, Vec<Option<f64>>) {
    grid_samples_with_progress(g, |_| true).expect("infallible grid sampling")
}

fn grid_samples_with_progress(g: &GridField, mut progress: impl FnMut(f32) -> bool) -> Option<(Vec<f64>, Vec<f64>, Vec<Option<f64>>)> {
    if !g.has_spline() {
        let mut z = Vec::with_capacity(g.z.len());
        for (i, chunk) in g.z.chunks(4096).enumerate() {
            if i % 16 == 0 && !progress(i as f32 * 4096.0 / g.z.len().max(1) as f32) { return None; }
            z.extend_from_slice(chunk);
        }
        return Some((g.xs.clone(), g.ys.clone(), z));
    }
    let refine = |axis: &[f64]| -> Vec<f64> {
        let inner = axis.windows(2).flat_map(|w| (0..SPLINE_REFINE).map(move |i| w[0] + (w[1] - w[0]) * i as f64 / SPLINE_REFINE as f64));
        inner.chain(axis.last().copied()).collect()
    };
    let (xs, ys) = (refine(&g.xs), refine(&g.ys));
    let mut z = Vec::with_capacity(xs.len() * ys.len());
    for (j, &y) in ys.iter().enumerate() {
        if j % 8 == 0 && !progress(j as f32 / ys.len().max(1) as f32) { return None; }
        z.extend(xs.iter().map(|&x| g.sample(x, y)));
    }
    if !progress(1.0) { return None; }
    Some((xs, ys, z))
}

/// A mesh resampled on a regular grid of about one sample per pixel.
fn mesh_samples(m: &MeshField, field: &Field, frame: &Frame) -> Option<(Vec<f64>, Vec<f64>, Vec<Option<f64>>)> {
    mesh_samples_with_progress(m, field, frame, |_| true)
}

fn mesh_samples_with_progress(m: &MeshField, field: &Field, frame: &Frame, mut progress: impl FnMut(f32) -> bool) -> Option<(Vec<f64>, Vec<f64>, Vec<Option<f64>>)> {
    let (x0, x1, y0, y1) = field.extent()?;
    let axis = |a: f64, b: f64, px: f64| -> Vec<f64> {
        let n = (px.round() as usize).clamp(2, MAX_MESH_SAMPLES);
        (0..n).map(|i| a + (b - a) * i as f64 / (n - 1) as f64).collect()
    };
    let (xs, ys) = (axis(x0, x1, frame.w), axis(y0, y1, frame.h));
    let mut z = Vec::with_capacity(xs.len() * ys.len());
    for (j, &y) in ys.iter().enumerate() {
        if j % 8 == 0 && !progress(j as f32 / ys.len().max(1) as f32) { return None; }
        z.extend(xs.iter().map(|&x| m.sample(x, y)));
    }
    if !progress(1.0) { return None; }
    Some((xs, ys, z))
}

/// Traces the levels with the `contour` crate and keeps the parts inside cells whose four corners have data.
/// The crate closes its rings around the grid and treats a hole as a low value; those parts are not contour lines.
fn isolines(xs: &[f64], ys: &[f64], z: &[Option<f64>], levels: &[f64]) -> Vec<Vec<Pt>> {
    isolines_with_progress(xs, ys, z, levels, |_| true).unwrap_or_default()
}

fn isolines_with_progress(xs: &[f64], ys: &[f64], z: &[Option<f64>], levels: &[f64], mut progress: impl FnMut(f32) -> bool) -> Option<Vec<Vec<Pt>>> {
    let (nx, ny) = (xs.len(), ys.len());
    if nx < 2 || ny < 2 {
        return Some(vec![]);
    }
    let known = || z.iter().flatten().chain(levels).copied();
    let (lo, hi) = (known().fold(f64::INFINITY, f64::min), known().fold(f64::NEG_INFINITY, f64::max));
    let below = lo - (hi - lo).max(1.0);
    let values: Vec<f64> = z.iter().map(|v| v.unwrap_or(below)).collect();
    isolines_prepared(xs, ys, z, &values, levels, progress)
}

fn isolines_prepared(
    xs: &[f64],
    ys: &[f64],
    z: &[Option<f64>],
    values: &[f64],
    levels: &[f64],
    mut progress: impl FnMut(f32) -> bool,
) -> Option<Vec<Vec<Pt>>> {
    let (nx, ny) = (xs.len(), ys.len());
    let lines = match ContourBuilder::new(nx, ny, true).lines_cancellable(values, levels, || progress(0.0)) {
        Ok(lines) => lines,
        Err(e) if matches!(e.kind(), contour::ErrorKind::Cancelled) => return None,
        Err(_) => return Some(vec![]),
    };
    let inside = |a: Coord<f64>, b: Coord<f64>| {
        // the crate places node i at i + 0.5
        let (u, v) = ((a.x + b.x) / 2.0 - 0.5, (a.y + b.y) / 2.0 - 0.5);
        if u < -EDGE_TOL || v < -EDGE_TOL || u > (nx - 1) as f64 + EDGE_TOL || v > (ny - 1) as f64 + EDGE_TOL {
            return false;
        }
        let k = (v.max(0.0) as usize).min(ny - 2) * nx + (u.max(0.0) as usize).min(nx - 2);
        [k, k + 1, k + nx, k + nx + 1].iter().all(|&c| z[c].is_some())
    };
    let at = |axis: &[f64], u: f64| {
        let u = (u - 0.5).clamp(0.0, (axis.len() - 1) as f64);
        let i = (u as usize).min(axis.len() - 2);
        axis[i] + (axis[i + 1] - axis[i]) * (u - i as f64)
    };
    let mut out = Vec::new();
    let total = lines.iter().map(|line| line.geometry().0.iter().map(|r| r.0.len().saturating_sub(1)).sum::<usize>()).sum::<usize>();
    let mut done = 0usize;
    for line in &lines {
        for ring in &line.geometry().0 {
            let mut parts: Vec<Vec<Pt>> = Vec::new();
            let mut open = false;
            for w in ring.0.windows(2) {
                done += 1;
                if done % 1024 == 0 && !progress(done as f32 / total.max(1) as f32) { return None; }
                let keep = inside(w[0], w[1]);
                if keep {
                    let p = (at(xs, w[1].x), at(ys, w[1].y));
                    if open {
                        parts.last_mut().unwrap().push(p);
                    } else {
                        parts.push(vec![(at(xs, w[0].x), at(ys, w[0].y)), p]);
                    }
                }
                open = keep;
            }
            // the ring may start in the middle of a line
            let joined = ring.0.len() >= 2 && ring.0.first() == ring.0.last() && inside(ring.0[0], ring.0[1]) && open && parts.len() >= 2;
            if joined {
                let head = parts.remove(0);
                parts.last_mut().unwrap().extend(&head[1..]);
            }
            out.append(&mut parts);
        }
    }
    if !progress(1.0) { return None; }
    Some(out)
}

fn contour_values(z: &[Option<f64>], levels: &[f64], mut progress: impl FnMut(f32) -> bool) -> Option<Vec<f64>> {
    let (mut lo, mut hi) = levels.iter().copied().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)));
    for (i, chunk) in z.chunks(4096).enumerate() {
        if i % 16 == 0 && !progress(0.5 * (i * 4096) as f32 / z.len().max(1) as f32) { return None; }
        for v in chunk.iter().flatten() { lo = lo.min(*v); hi = hi.max(*v); }
    }
    let below = lo - (hi - lo).max(1.0);
    let mut values = Vec::with_capacity(z.len());
    for (i, chunk) in z.chunks(4096).enumerate() {
        if i % 16 == 0 && !progress(0.5 + 0.5 * (i * 4096) as f32 / z.len().max(1) as f32) { return None; }
        values.extend(chunk.iter().map(|v| v.unwrap_or(below)));
    }
    if !progress(1.0) { return None; }
    Some(values)
}

pub fn contours_with_progress(ctx: &Ctx, frame: &Frame, ops: &mut Vec<Op>, mut progress: impl FnMut(f32) -> bool) -> bool {
    let levels = contour_levels(ctx);
    if levels.is_empty() {
        return true;
    }
    let samples = match ctx.field {
        Field::Grid(g) => grid_samples_with_progress(g, |p| progress(p * 0.3)),
        Field::Mesh(m) => mesh_samples_with_progress(m, ctx.field, frame, |p| progress(p * 0.3)),
        Field::Panel(_) => return true,
    };
    let Some((xs, ys, z)) = samples else { return false };
    let Some(values) = contour_values(&z, &levels, |p| progress(0.3 + p * 0.1)) else { return false };
    for (i, level) in levels.iter().enumerate() {
        if !progress(0.4 + 0.6 * i as f32 / levels.len() as f32) { return false; }
        let Some(lines) = isolines_prepared(&xs, &ys, &z, &values, &[*level], |p| progress(0.4 + 0.6 * (i as f32 + p) / levels.len() as f32)) else { return false };
        for line in lines {
        let pts: Vec<(f32, f32)> = line.iter().map(|&(x, y)| frame.to_px(x, y)).collect();
        ops.push(Op::Polyline { pts, color: ctx.s.toko_color, width: 1.0 });
        }
    }
    progress(1.0)
}

// ---- vectors ----

pub fn vectors_with_progress(ctx: &Ctx, frame: &Frame, ops: &mut Vec<Op>, mut progress: impl FnMut(f32) -> bool) -> bool {
    let s = ctx.s;
    let actual_max = ctx.field.max_vector();
    let Some(max) = (if s.vmax_on { Some(s.vmax) } else { actual_max }).filter(|m| *m > 0.0) else { return true };
    let base = s.size as f64 / 20.0 * s.vector_len / 100.0;
    let fixed_head_base = base * actual_max.unwrap_or(0.0) / max;
    let keep = |idx: usize, on: bool, step: u32, off: u32| !on || (idx >= off as usize && (idx - off as usize) % step.max(1) as usize == 0);
    let is_grid = matches!(ctx.field, Field::Grid(_));
    let total = nodes(ctx.field).count();
    for (i, n) in nodes(ctx.field).enumerate() {
        if i % 256 == 0 && !progress(i as f32 / total.max(1) as f32) { return false; }
        if n.u.is_none() && n.v.is_none() {
            continue;
        }
        if is_grid && !(keep(n.i, s.skip_x, s.step_x, s.offset_x) && keep(n.j, s.skip_y, s.step_y, s.offset_y)) {
            continue;
        }
        if !is_grid && !keep(n.i, s.skip_x, s.step_x, s.offset_x) {
            continue;
        }
        let (u, v) = (n.u.unwrap_or(0.0), n.v.unwrap_or(0.0));
        let m = u.hypot(v);
        if m <= 0.0 || !m.is_finite() {
            continue;
        }
        // direction in the plane from the raw components, so EDh / EDv never bend it
        let (dx, dy) = match &frame.cyl {
            Some(c) => {
                let span = c.th_max - c.th_min;
                let a = if span.abs() < f64::EPSILON { 0.0 } else { (n.x - c.th_min) / span * std::f64::consts::TAU };
                (-u * a.sin() + v * a.cos(), u * a.cos() + v * a.sin())
            }
            None => (u, v),
        };
        let (ux, uy) = (dx / m, -dy / m);
        let len = base * m / max;
        let (px, py) = frame.to_px(n.x, n.y);
        let (tx, ty) = (px as f64 + ux * len, py as f64 + uy * len);
        let col = if s.color_vector { ctx.grad.color(m / max) } else { s.vector_color };
        ops.push(Op::Polyline { pts: vec![(px, py), (tx as f32, ty as f32)], color: col, width: 1.0 });
        if s.vector_head {
            let hl = s.head_len / 100.0 * if s.head_fix { fixed_head_base } else { len };
            let ang = (s.head_angle as f64).to_radians();
            let back = |sgn: f64| {
                let (sn, cs) = (sgn * ang).sin_cos();
                // reverse direction rotated by +-angle
                let (bx, by) = (-ux, -uy);
                ((tx + hl * (bx * cs - by * sn)) as f32, (ty + hl * (bx * sn + by * cs)) as f32)
            };
            let (h1, h2) = (back(1.0), back(-1.0));
            let tip = (tx as f32, ty as f32);
            if s.head_paint {
                ops.push(Op::Polygon { pts: vec![tip, h1, h2], fill: Some(s.vector_head_color), stroke: Some(col) });
            } else {
                ops.push(Op::Polyline { pts: vec![h1, tip, h2], color: col, width: 1.0 });
            }
        }
    }
    progress(1.0)
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::super::*;
    use super::{SPLINE_REFINE, Pt, grid_samples, isolines};
    use crate::engine::data::{DataFile, Point};
    use crate::engine::field;
    use crate::engine::scene::Op;

    fn spline_grid(nx: usize, ny: usize, f: impl Fn(f64, f64) -> f64) -> Field {
        let mut df = DataFile::default();
        for j in 0..ny {
            for i in 0..nx {
                let (x, y) = (i as f64, j as f64);
                df.points.push(Point { x: Some(x), y: Some(y), z: Some(f(x, y)), u: None, v: None });
            }
        }
        let mut s = settings();
        s.spline = true;
        field::build(&df, &s).unwrap().0
    }

    #[test]
    fn holes_are_transparent() {
        let f = grid(5, 5, |x, y| if x == 2.0 && y == 2.0 { None } else { Some(x + y) });
        let b = build_scene(&f, &settings(), &[]).unwrap();
        let Op::Image { w, h, rgba, .. } = &b.scene.ops[0] else { panic!("no image") };
        assert_eq!((*w, *h), (100, 100));
        let alpha = |x: usize, y: usize| rgba[(y * 100 + x) * 4 + 3];
        assert_eq!(alpha(50, 50), 0); // centre of the hole
        assert_eq!(alpha(5, 5), 255);
        assert_eq!(alpha(95, 95), 255);
    }

    #[test]
    fn image_scanline_generation_can_be_cancelled_without_emitting_partial_image() {
        let f = grid(3, 3, |x, y| Some(x + y));
        let mut s = settings();
        s.size = 2048;
        let ctx = Ctx::new(&f, &s);
        let frame = Layout::new(&f, &s).unwrap().frame(&f, &s).unwrap();
        let mut ops = Vec::new();
        let mut checks = 0;
        let completed = plane::image_with_progress(&ctx, &frame, &mut ops, |_| {
            checks += 1;
            checks < 2
        });
        assert!(!completed);
        assert!(ops.is_empty());
        assert_eq!(checks, 2, "the second progress update should cancel during scanline work");
    }

    #[test]
    fn vector_generation_checks_cancellation_during_node_loop() {
        let f = grid_uv(30, 30, |x, y| (x + 1.0, y + 1.0));
        let mut s = settings();
        s.vector = true;
        let ctx = Ctx::new(&f, &s);
        let frame = Layout::new(&f, &s).unwrap().frame(&f, &s).unwrap();
        let mut ops = Vec::new();
        let mut checks = 0;
        let completed = super::vectors_with_progress(&ctx, &frame, &mut ops, |_| {
            checks += 1;
            checks < 2
        });
        assert!(!completed);
        assert!(checks >= 2);
        assert!(!ops.is_empty());
        assert!(ops.len() < 900); // the loop stopped before processing all 900 nodes
    }

    #[test]
    fn contour_segment_traversal_checks_cancellation() {
        let (xs, ys) = ((0..2).map(|i| i as f64).collect::<Vec<_>>(), (0..2048).map(|i| i as f64).collect::<Vec<_>>());
        let z = ys.iter().flat_map(|_| [Some(0.0), Some(1.0)]).collect::<Vec<_>>();
        let mut checks = 0;
        let out = super::isolines_with_progress(&xs, &ys, &z, &[0.5], |_| {
            checks += 1;
            false
        });
        assert!(out.is_none());
        assert_eq!(checks, 1);
    }

    #[test]
    fn contour_range_scan_checks_cancellation_before_value_allocation() {
        let z = vec![Some(1.0); 200_000];
        let mut calls = 0;
        let values = super::contour_values(&z, &[0.5], |p| { calls += 1; p < 0.1 });
        assert!(values.is_none());
        assert!(calls >= 2);
    }

    #[test]
    fn contour_tracing_can_cancel_inside_external_algorithm() {
        let values = vec![0.0; 512 * 512];
        let mut calls = 0;
        let result = contour::ContourBuilder::new(512, 512, true).lines_cancellable(&values, &[0.5], || {
            calls += 1;
            calls < 5
        });
        assert!(matches!(result, Err(e) if matches!(e.kind(), contour::ErrorKind::Cancelled)));
        assert_eq!(calls, 5);
    }

    #[test]
    fn cancellable_contours_match_upstream_geometry() {
        let values = (0..64 * 48).map(|i| ((i % 64) as f64 * 0.2).sin() + ((i / 64) as f64 * 0.3).cos()).collect::<Vec<_>>();
        let thresholds = [-1.0, -0.25, 0.0, 0.75, 1.5];
        let expected = contour_reference::ContourBuilder::new(64, 48, true).lines(&values, &thresholds).unwrap();
        let actual = contour::ContourBuilder::new(64, 48, true).lines_cancellable(&values, &thresholds, || true).unwrap();
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.iter().zip(&expected) { assert_eq!(a.geometry(), b.geometry()); }
    }

    #[test]
    fn contour_and_vector_progress_is_monotonic() {
        let f = grid_uv(12, 12, |x, y| (1.0, x - y));
        let mut s = settings();
        s.toko = true;
        s.toko_by_count = true;
        s.toko_count = 4;
        s.vector = true;
        let mut last = 0.0;
        build_scene_with_progress(&f, &s, &[], |p| {
            assert!(p >= last, "progress regressed from {last} to {p}");
            last = p;
            true
        }).unwrap();
        assert_eq!(last, 1.0);
    }

    #[test]
    fn y_increases_upward() {
        let f = grid(5, 5, |_, y| Some(y));
        let b = build_scene(&f, &settings(), &[]).unwrap();
        let Op::Image { rgba, .. } = &b.scene.ops[0] else { panic!() };
        let px = |y: usize| rgba[(y * 100 + 50) * 4..(y * 100 + 50) * 4 + 3].to_vec();
        let c = |z: f64| Ctx::new(&f, &settings()).color(z);
        let (top, bottom) = (c(3.98), c(0.02));
        assert_eq!(px(0), vec![top.0, top.1, top.2]);
        assert_eq!(px(99), vec![bottom.0, bottom.1, bottom.2]);
    }

    #[test]
    fn contour_count_and_levels() {
        let f = grid(11, 3, |x, _| Some(x)); // z from 0 to 10
        let mut s = settings();
        s.toko = true;
        s.toko_by_count = true;
        s.toko_by_step = false;
        s.toko_count = 4;
        let b = build_scene(&f, &s, &[]).unwrap();
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polyline { .. })), 4);
        let ctx = Ctx::new(&f, &s);
        assert_eq!(plane::contour_levels(&ctx), vec![2.0, 4.0, 6.0, 8.0]);
        s.toko_by_count = false;
        s.toko_by_step = true;
        s.toko_dt = 2.5;
        let ctx = Ctx::new(&f, &s);
        assert_eq!(plane::contour_levels(&ctx), vec![0.0, 2.5, 5.0, 7.5, 10.0]);
        s.toko_min_on = true;
        s.toko_min = 3.0;
        let ctx = Ctx::new(&f, &s);
        assert_eq!(plane::contour_levels(&ctx), vec![5.0, 7.5, 10.0]);
    }

    #[test]
    fn closed_contour_is_one_polyline() {
        let f = grid(9, 9, |x, y| Some(((x - 4.0).powi(2) + (y - 4.0).powi(2)).sqrt()));
        let mut s = settings();
        s.toko = true;
        s.toko_by_count = true;
        s.toko_count = 1;
        let b = build_scene(&f, &s, &[]).unwrap();
        let lines: Vec<_> = b.scene.ops.iter().filter_map(|o| if let Op::Polyline { pts, .. } = o { Some(pts) } else { None }).collect();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].first(), lines[0].last());
    }

    #[test]
    fn contour_skips_missing_cells() {
        let f = grid(5, 3, |x, y| if x == 2.0 && y == 1.0 { None } else { Some(x) });
        let mut s = settings();
        s.toko = true;
        s.toko_count = 1; // level 2 passes through the missing node
        let b = build_scene(&f, &s, &[]).unwrap();
        // the level passes only through cells that touch the hole, and those are skipped
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polyline { .. })), 0);
    }

    #[test]
    fn contour_line_lies_on_its_level() {
        let f = grid(11, 3, |x, _| Some(x));
        let mut s = settings();
        s.toko = true;
        s.toko_by_count = true;
        s.toko_count = 1; // level 5
        let ctx = Ctx::new(&f, &s);
        let Field::Grid(g) = &f else { unreachable!() };
        let lines = isolines(&g.xs, &g.ys, &g.z, &plane::contour_levels(&ctx));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].len(), 3);
        assert!(lines[0].iter().all(|p| (p.0 - 5.0).abs() < 1e-9));
    }

    #[test]
    fn contour_follows_uneven_axes() {
        let (xs, ys) = (vec![0.0, 1.0, 10.0], vec![0.0, 1.0]);
        let z: Vec<_> = [0.0, 1.0, 2.0, 0.0, 1.0, 2.0].iter().map(|&v| Some(v)).collect();
        let lines = isolines(&xs, &ys, &z, &[1.5]);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].iter().all(|p| (p.0 - 5.5).abs() < 1e-9));
    }

    #[test]
    fn contour_stops_at_a_hole_and_keeps_the_rest() {
        let f = grid(5, 7, |x, y| if x == 2.0 && y == 3.0 { None } else { Some(x) });
        let Field::Grid(g) = &f else { unreachable!() };
        let lines = isolines(&g.xs, &g.ys, &g.z, &[1.5]);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().flatten().all(|p| (p.0 - 1.5).abs() < 1e-9 && (p.1 <= 2.0 || p.1 >= 4.0)));
    }

    #[test]
    fn spline_contour_stays_on_the_interpolated_surface() {
        let f = spline_grid(7, 7, |x, y| (x - 3.0).powi(2) + (y - 3.0).powi(2));
        let Field::Grid(g) = &f else { unreachable!() };
        assert!(g.has_spline());
        let (xs, ys, z) = grid_samples(g);
        assert_eq!(xs.len(), 6 * SPLINE_REFINE + 1);
        let lines = isolines(&xs, &ys, &z, &[4.2]);
        assert_eq!(lines.len(), 1);
        let linear = isolines(&g.xs, &g.ys, &g.z, &[4.2]);
        let err = |l: &[Pt]| l.iter().map(|&(x, y)| (g.sample(x, y).unwrap() - 4.2).abs()).fold(0.0, f64::max);
        assert!(err(&lines[0]) < 0.05, "{}", err(&lines[0]));
        assert!(err(&lines[0]) < err(&linear[0]));
    }

    #[test]
    fn mesh_contour_is_drawn_inside_the_mesh() {
        let mut df = DataFile::default();
        for (x, y) in [(0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (10.0, 10.0), (5.0, 5.0)] {
            df.points.push(Point { x: Some(x), y: Some(y), z: Some(x), u: None, v: None });
        }
        df.mesh_auto = true;
        let f = field::build(&df, &settings()).unwrap().0;
        let mut s = settings();
        s.toko = true;
        s.toko_by_count = true;
        s.toko_count = 1;
        let b = build_scene(&f, &s, &[]).unwrap();
        assert!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polyline { .. })) >= 1);
    }

    #[test]
    fn vector_head_keeps_direction_under_scaling() {
        let f = grid_uv(2, 2, |x, y| if x == 0.0 && y == 0.0 { (1.0, 1.0) } else { (0.0, 0.0) });
        let mut s = settings();
        s.vector = true;
        s.vector_head = true;
        s.head_paint = false;
        s.head_fix = false;
        s.head_angle = 30;
        s.head_len = 50.0;
        s.vector_len = 100.0;
        s.scale_v = 3.0;
        let b = build_scene(&f, &s, &[]).unwrap();
        let lines: Vec<_> = b.scene.ops.iter().filter_map(|o| if let Op::Polyline { pts, .. } = o { Some(pts.clone()) } else { None }).collect();
        assert_eq!(lines.len(), 2);
        let (a, t) = (lines[0][0], lines[0][1]);
        // u = v = 1 gives 45 degrees on screen regardless of the 3x vertical scale
        assert!(((t.0 - a.0) + (t.1 - a.1)).abs() < 1e-3);
        let len = ((t.0 - a.0).powi(2) + (t.1 - a.1).powi(2)).sqrt();
        assert!((len - 5.0).abs() < 1e-3); // size 100 / 20
        let head = &lines[1];
        let hlen = ((head[0].0 - t.0).powi(2) + (head[0].1 - t.1).powi(2)).sqrt();
        assert!((hlen - len * 0.5).abs() < 1e-3);
        let shaft = (a.1 - t.1).atan2(a.0 - t.0);
        let wing = (head[0].1 - t.1).atan2(head[0].0 - t.0);
        assert!(((shaft - wing).abs() - 30f32.to_radians()).abs() < 1e-3);
    }

    #[test]
    fn vector_skip_and_color() {
        let f = grid_uv(4, 4, |_, _| (1.0, 0.0));
        let mut s = settings();
        s.vector = true;
        s.vector_head = false;
        s.skip_x = true;
        s.step_x = 2;
        s.offset_x = 1;
        let b = build_scene(&f, &s, &[]).unwrap();
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polyline { .. })), 8); // x = 1, 3 on 4 rows
    }

    #[test]
    fn fixed_heads_follow_largest_data_vector_with_a_separate_reference() {
        let f = grid_uv(2, 2, |x, _| if x == 0.0 { (1.0, 0.0) } else { (2.0, 0.0) });
        let mut s = settings();
        s.vector = true;
        s.vector_head = true;
        s.head_fix = true;
        s.head_paint = false;
        s.head_len = 50.0;
        s.vmax_on = true;
        s.vmax = 4.0;
        let b = build_scene(&f, &s, &[]).unwrap();
        let lines: Vec<_> = b.scene.ops.iter().filter_map(|o| if let Op::Polyline { pts, .. } = o { Some(pts) } else { None }).collect();
        assert_eq!(lines.len(), 8);
        let largest_shaft = lines.chunks_exact(2).map(|pair| {
            let a = pair[0][0]; let b = pair[0][1];
            (b.0 - a.0).hypot(b.1 - a.1)
        }).fold(0.0_f32, f32::max);
        for pair in lines.chunks_exact(2) {
            let tip = pair[1][1]; let wing = pair[1][0];
            let head = (tip.0 - wing.0).hypot(tip.1 - wing.1);
            assert!((head - 0.5 * largest_shaft).abs() < 1e-3, "{head} vs largest shaft {largest_shaft}");
        }
    }

    #[test]
    fn scatter_draws_dots() {
        let f = grid(3, 3, |x, y| Some(x + y));
        let mut s = settings();
        s.scatter = true;
        s.scatter_mode = 1;
        let b = build_scene(&f, &s, &[]).unwrap();
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Ellipse { .. })), 9);
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Image { .. })), 0);
    }

    #[test]
    fn mesh_is_rasterised_per_triangle() {
        let mut df = DataFile::default();
        for (x, y) in [(0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (10.0, 10.0), (5.0, 5.0)] {
            df.points.push(Point { x: Some(x), y: Some(y), z: Some(x + y), u: None, v: None });
        }
        df.mesh_auto = true;
        let (f, _) = crate::engine::field::build(&df, &settings()).unwrap();
        let b = build_scene(&f, &settings(), &[]).unwrap();
        let Op::Image { rgba, .. } = &b.scene.ops[0] else { panic!() };
        assert!(rgba.chunks(4).all(|p| p[3] == 255));
    }
}
