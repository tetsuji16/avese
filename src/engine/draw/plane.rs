//! Flat views: distribution, scatter, panel, contour lines and vectors (Cartesian and cylindrical).
use super::{Ctx, Frame};
use crate::engine::field::{Field, GridField, MeshField};
use crate::engine::scene::{Op, Rgb};
use std::collections::{HashMap, VecDeque};

const MAX_DOTS: usize = 200_000;
const MAX_LEVELS: usize = 1000;

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
    if ctx.zr.is_none() {
        return;
    }
    let (w, h) = (frame.w as usize, frame.h as usize);
    let mut rgba = vec![0u8; w * h * 4];
    match ctx.field {
        Field::Mesh(m) => mesh_image(ctx, frame, m, &mut rgba),
        _ => {
            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(16);
            let rows_per = h.div_ceil(threads).max(1);
            std::thread::scope(|sc| {
                for (ci, chunk) in rgba.chunks_mut(rows_per * w * 4).enumerate() {
                    let frame = *frame;
                    sc.spawn(move || {
                        for (r, row) in chunk.chunks_mut(w * 4).enumerate() {
                            let j = ci * rows_per + r;
                            for i in 0..w {
                                let p = frame.from_px_raw(frame.ox + i as f64 + 0.5, frame.oy + j as f64 + 0.5);
                                if let Some(z) = p.and_then(|(x, y)| ctx.field.sample(x, y)) {
                                    let c = ctx.color(z);
                                    row[i * 4..i * 4 + 4].copy_from_slice(&[c.0, c.1, c.2, 255]);
                                }
                            }
                        }
                    });
                }
            });
        }
    }
    ops.push(Op::Image { x: frame.ox as i32, y: frame.oy as i32, w: w as u32, h: h as u32, rgba });
}

fn mesh_image(ctx: &Ctx, frame: &Frame, m: &MeshField, rgba: &mut [u8]) {
    let (w, h) = (frame.w as i64, frame.h as i64);
    for tri in &m.tris {
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

struct Seg {
    ka: u64,
    a: Pt,
    kb: u64,
    b: Pt,
}

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

fn cross(l: f64, pa: Pt, va: f64, pb: Pt, vb: f64) -> Pt {
    let t = if vb == va { 0.5 } else { ((l - va) / (vb - va)).clamp(0.0, 1.0) };
    (pa.0 + t * (pb.0 - pa.0), pa.1 + t * (pb.1 - pa.1))
}

fn grid_segments(g: &GridField, levels: &[f64]) -> Vec<Vec<Seg>> {
    let (nx, ny) = (g.xs.len(), g.ys.len());
    let mut out: Vec<Vec<Seg>> = (0..levels.len()).map(|_| Vec::new()).collect();
    for j in 0..ny.saturating_sub(1) {
        for i in 0..nx.saturating_sub(1) {
            let k = j * nx + i;
            let (Some(v00), Some(v10), Some(v01), Some(v11)) = (g.z[k], g.z[k + 1], g.z[k + nx], g.z[k + nx + 1]) else { continue };
            let lo = v00.min(v10).min(v01).min(v11);
            let hi = v00.max(v10).max(v01).max(v11);
            let (a, b) = (levels.partition_point(|&l| l <= lo), levels.partition_point(|&l| l <= hi));
            if a >= b {
                continue;
            }
            let p = [(g.xs[i], g.ys[j]), (g.xs[i + 1], g.ys[j]), (g.xs[i + 1], g.ys[j + 1]), (g.xs[i], g.ys[j + 1])];
            let v = [v00, v10, v11, v01];
            let c = ((k * 2) as u64, (2 * (k + 1) + 1) as u64, (2 * (k + nx)) as u64, (k * 2 + 1) as u64);
            // bottom, right, top, left as corner pairs
            let edges = [(0, 1, c.0), (1, 2, c.1), (3, 2, c.2), (0, 3, c.3)];
            for li in a..b {
                polygon_segments(levels[li], &p, &v, &edges, &mut out[li]);
            }
        }
    }
    out
}

/// Segments of one level inside a convex cell with corners `p`/`v` in order and its edges as (corner, corner, key).
fn polygon_segments(l: f64, p: &[Pt], v: &[f64], edges: &[(usize, usize, u64)], out: &mut Vec<Seg>) {
    let mut hits: Vec<(usize, u64, Pt)> = Vec::with_capacity(4);
    for (e, &(a, b, key)) in edges.iter().enumerate() {
        if (v[a] >= l) != (v[b] >= l) {
            hits.push((e, key, cross(l, p[a], v[a], p[b], v[b])));
        }
    }
    match hits.len() {
        2 => out.push(Seg { ka: hits[0].1, a: hits[0].2, kb: hits[1].1, b: hits[1].2 }),
        4 => {
            // saddle: the centre value decides which diagonal pair stays connected
            let centre_high = v.iter().sum::<f64>() / 4.0 >= l;
            for c in 0..4 {
                if (v[c] >= l) != centre_high {
                    let (e1, e2) = ((c + 3) % 4, c);
                    out.push(Seg { ka: hits[e1].1, a: hits[e1].2, kb: hits[e2].1, b: hits[e2].2 });
                }
            }
        }
        _ => {}
    }
}

fn mesh_segments(m: &MeshField, levels: &[f64]) -> Vec<Vec<Seg>> {
    let n = m.nodes.len() as u64;
    let mut out: Vec<Vec<Seg>> = (0..levels.len()).map(|_| Vec::new()).collect();
    for t in &m.tris {
        let (Some(z0), Some(z1), Some(z2)) = (m.z[t[0]], m.z[t[1]], m.z[t[2]]) else { continue };
        let v = [z0, z1, z2];
        let (lo, hi) = (z0.min(z1).min(z2), z0.max(z1).max(z2));
        let (a, b) = (levels.partition_point(|&l| l <= lo), levels.partition_point(|&l| l <= hi));
        let p = [m.nodes[t[0]], m.nodes[t[1]], m.nodes[t[2]]];
        let key = |a: usize, b: usize| t[a].min(t[b]) as u64 * n + t[a].max(t[b]) as u64;
        let edges = [(0, 1, key(0, 1)), (1, 2, key(1, 2)), (2, 0, key(2, 0))];
        for li in a..b {
            polygon_segments(levels[li], &p, &v, &edges, &mut out[li]);
        }
    }
    out
}

/// Joins segments that share an edge key into chains of points.
fn chain(segs: &[Seg]) -> Vec<Vec<Pt>> {
    let mut at: HashMap<u64, [u32; 2]> = HashMap::with_capacity(segs.len() * 2);
    for (i, s) in segs.iter().enumerate() {
        for k in [s.ka, s.kb] {
            let e = at.entry(k).or_insert([u32::MAX; 2]);
            if e[0] == u32::MAX {
                e[0] = i as u32;
            } else {
                e[1] = i as u32;
            }
        }
    }
    let next = |key: u64, from: usize, done: &[bool]| -> Option<usize> {
        at.get(&key)?.iter().map(|&i| i as usize).find(|&i| i != u32::MAX as usize && i != from && !done[i])
    };
    let mut done = vec![false; segs.len()];
    let mut out = Vec::new();
    for s0 in 0..segs.len() {
        if done[s0] {
            continue;
        }
        done[s0] = true;
        let mut pts: VecDeque<Pt> = VecDeque::from([segs[s0].a, segs[s0].b]);
        let mut tail = (s0, segs[s0].kb);
        while let Some(n) = next(tail.1, tail.0, &done) {
            done[n] = true;
            let s = &segs[n];
            let (p, k) = if s.ka == tail.1 { (s.b, s.kb) } else { (s.a, s.ka) };
            pts.push_back(p);
            tail = (n, k);
        }
        let mut head = (s0, segs[s0].ka);
        while let Some(n) = next(head.1, head.0, &done) {
            done[n] = true;
            let s = &segs[n];
            let (p, k) = if s.ka == head.1 { (s.b, s.kb) } else { (s.a, s.ka) };
            pts.push_front(p);
            head = (n, k);
        }
        out.push(pts.into());
    }
    out
}

pub fn contours(ctx: &Ctx, frame: &Frame, ops: &mut Vec<Op>) {
    let levels = contour_levels(ctx);
    if levels.is_empty() {
        return;
    }
    let per_level = match ctx.field {
        Field::Grid(g) => grid_segments(g, &levels),
        Field::Mesh(m) => mesh_segments(m, &levels),
        Field::Panel(_) => return,
    };
    for segs in per_level {
        for pts in chain(&segs) {
            let pts: Vec<(f32, f32)> = pts.iter().map(|&(x, y)| frame.to_px(x, y)).collect();
            ops.push(Op::Polyline { pts, color: ctx.s.toko_color, width: 1.0 });
        }
    }
}

// ---- vectors ----

pub fn vectors(ctx: &Ctx, frame: &Frame, ops: &mut Vec<Op>) {
    let s = ctx.s;
    let Some(max) = (if s.vmax_on { Some(s.vmax) } else { ctx.field.max_vector() }).filter(|m| *m > 0.0) else { return };
    let base = s.size as f64 / 20.0 * s.vector_len / 100.0;
    let keep = |idx: usize, on: bool, step: u32, off: u32| !on || (idx >= off as usize && (idx - off as usize) % step.max(1) as usize == 0);
    let is_grid = matches!(ctx.field, Field::Grid(_));
    for n in nodes(ctx.field) {
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
            let hl = s.head_len / 100.0 * if s.head_fix { base } else { len };
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
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::super::*;
    use crate::engine::scene::Op;

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
        use crate::engine::data::{DataFile, Point};
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
