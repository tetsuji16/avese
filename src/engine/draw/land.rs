//! Bird's-eye view: orthographic projection with a painter's algorithm.
use super::axes::{arrow_head, fmt_num, pick_step, ticks};
use super::plane::{dot_op, nodes};
use super::{Ctx, Layout, push_text, use_cylinder};
use crate::engine::field::{Cylinder, Field, GridField, MeshField};
use crate::engine::raster::measure_text;
use crate::engine::scene::{Anchor, Op, Rgb};
use std::f64::consts::TAU;

const MAX_POLYS: usize = 60_000;

/// Rotation by `eta` around the vertical axis, then tilt by the zenith angle `phi`.
#[derive(Clone, Copy, Debug)]
pub struct Proj {
    cos_e: f64,
    sin_e: f64,
    cos_p: f64,
    sin_p: f64,
    wd: f64,
    hd: f64,
    min_x: f64,
    max_up: f64,
    /// size of the projected bounding box in pixels
    pub w: f64,
    pub h: f64,
    ox: f64,
    oy: f64,
}

impl Proj {
    pub fn new(eta: i32, phi: i32, wd: f64, hd: f64, hz: f64) -> Proj {
        let (e, p) = ((eta as f64).to_radians(), (phi as f64).to_radians());
        let mut pr = Proj { cos_e: e.cos(), sin_e: e.sin(), cos_p: p.cos(), sin_p: p.sin(), wd, hd, min_x: 0.0, max_up: 0.0, w: 1.0, h: 1.0, ox: 0.0, oy: 0.0 };
        let (mut lx, mut hx, mut lu, mut hu) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for &x in &[0.0, wd] {
            for &y in &[0.0, hd] {
                for &z in &[0.0, hz] {
                    let (sx, up, _) = pr.raw(x, y, z);
                    lx = lx.min(sx);
                    hx = hx.max(sx);
                    lu = lu.min(up);
                    hu = hu.max(up);
                }
            }
        }
        pr.min_x = lx;
        pr.max_up = hu;
        pr.w = (hx - lx).max(1.0);
        pr.h = (hu - lu).max(1.0);
        pr
    }

    fn raw(&self, x: f64, y: f64, z: f64) -> (f64, f64, f64) {
        let (xc, yc) = (x - self.wd / 2.0, y - self.hd / 2.0);
        let a = xc * self.cos_e - yc * self.sin_e;
        let b = xc * self.sin_e + yc * self.cos_e;
        (a, b * self.cos_p + z * self.sin_p, b * self.sin_p - z * self.cos_p)
    }

    pub fn placed(mut self, ox: f64, oy: f64) -> Proj {
        self.ox = ox;
        self.oy = oy;
        self
    }

    /// Image position and depth (larger = farther away).
    pub fn at(&self, x: f64, y: f64, z: f64) -> ((f32, f32), f64) {
        let (sx, up, d) = self.raw(x, y, z);
        (((self.ox + sx - self.min_x) as f32, (self.oy + self.max_up - up) as f32), d)
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
    let (Some(proj), Some((zlo, zhi)), Some((x0, x1, y0, y1))) = (lay.proj, ctx.zr, ctx.field.extent()) else { return };
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
    if s.grid {
        floor_grid(&v, ops);
    }
    let mut items: Items = Vec::new();
    let is_panel = matches!(ctx.field, Field::Panel(_));
    if s.scatter && !is_panel {
        scatter(&v, &mut items);
    } else {
        match ctx.field {
            Field::Grid(g) => grid_surface(&v, g, &mut items),
            Field::Mesh(m) => mesh_surface(&v, m, &mut items),
            Field::Panel(_) => panel_surface(&v, &mut items),
        }
    }
    items.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    ops.extend(items.into_iter().map(|i| i.1));
    if s.axis {
        axes(&v, ops);
    }
    if s.point {
        let total = nodes(ctx.field).count();
        for n in nodes(ctx.field).step_by(total.div_ceil(50_000).max(1)) {
            if let Some(z) = n.z {
                ops.push(dot_op(0, v.pt(n.x, n.y, z).0.0, v.pt(n.x, n.y, z).0.1, 3.0, s.fg));
            }
        }
    }
}

fn scatter(v: &View, items: &mut Items) {
    let s = v.ctx.s;
    let total = nodes(v.ctx.field).count();
    for n in nodes(v.ctx.field).step_by(total.div_ceil(MAX_POLYS * 2).max(1)) {
        let Some(z) = n.z else { continue };
        let ((px, py), d) = v.pt(n.x, n.y, z);
        items.push((d, dot_op(s.scatter_mode, px, py, s.dot_size.max(1) as f32, v.ctx.color(z))));
    }
}

fn poly(pts: Vec<(f32, f32)>, c: Rgb, wire: bool) -> Op {
    if wire { Op::Polygon { pts, fill: None, stroke: Some(c) } } else { Op::Polygon { pts, fill: Some(c), stroke: Some(c) } }
}

fn grid_surface(v: &View, g: &GridField, items: &mut Items) {
    let s = v.ctx.s;
    let (nx, ny) = (g.xs.len(), g.ys.len());
    let full = (nx - 1) * (ny - 1);
    let stride = ((full as f64 / MAX_POLYS as f64).sqrt().ceil() as usize).max(1);
    let idx = |n: usize| -> Vec<usize> {
        let mut v: Vec<usize> = (0..n).step_by(stride).collect();
        if v.last() != Some(&(n - 1)) {
            v.push(n - 1);
        }
        v
    };
    let (is, js) = (idx(nx), idx(ny));
    let k = if stride == 1 { ((MAX_POLYS as f64 / full as f64).sqrt().floor() as usize).clamp(1, 4) } else { 1 };
    for jw in js.windows(2) {
        for iw in is.windows(2) {
            let (ia, ib, ja, jb) = (iw[0], iw[1], jw[0], jw[1]);
            let (Some(z00), Some(z10), Some(z01), Some(z11)) = (g.at(ia, ja), g.at(ib, ja), g.at(ia, jb), g.at(ib, jb)) else { continue };
            if !s.surface {
                let mut seg = |a: (usize, usize, f64), b: (usize, usize, f64)| {
                    let (pa, da) = v.pt(g.xs[a.0], g.ys[a.1], a.2);
                    let (pb, db) = v.pt(g.xs[b.0], g.ys[b.1], b.2);
                    items.push(((da + db) / 2.0, Op::Polyline { pts: vec![pa, pb], color: v.ctx.color((a.2 + b.2) / 2.0), width: 1.0 }));
                };
                seg((ia, ja, z00), (ib, ja, z10));
                seg((ia, ja, z00), (ia, jb, z01));
                if ib == nx - 1 {
                    seg((ib, ja, z10), (ib, jb, z11));
                }
                if jb == ny - 1 {
                    seg((ia, jb, z01), (ib, jb, z11));
                }
                continue;
            }
            let at = |tx: f64, ty: f64| {
                let x = g.xs[ia] + (g.xs[ib] - g.xs[ia]) * tx;
                let y = g.ys[ja] + (g.ys[jb] - g.ys[ja]) * ty;
                let z = z00 * (1.0 - tx) * (1.0 - ty) + z10 * tx * (1.0 - ty) + z01 * (1.0 - tx) * ty + z11 * tx * ty;
                (x, y, z)
            };
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
}

fn mesh_surface(v: &View, m: &MeshField, items: &mut Items) {
    let stride = m.tris.len().div_ceil(MAX_POLYS).max(1);
    for t in m.tris.iter().step_by(stride) {
        let (Some(a), Some(b), Some(c)) = (m.z[t[0]], m.z[t[1]], m.z[t[2]]) else { continue };
        let p: Vec<((f32, f32), f64)> = [(t[0], a), (t[1], b), (t[2], c)].iter().map(|&(k, z)| v.pt(m.nodes[k].0, m.nodes[k].1, z)).collect();
        let depth = p.iter().map(|q| q.1).sum::<f64>() / 3.0;
        items.push((depth, poly(p.iter().map(|q| q.0).collect(), v.ctx.color((a + b + c) / 3.0), !v.ctx.s.surface)));
    }
}

fn panel_surface(v: &View, items: &mut Items) {
    let Field::Panel(p) = v.ctx.field else { return };
    let cells = p.rows * p.cols;
    let target = if cells > MAX_POLYS { MAX_POLYS / 5 } else { MAX_POLYS };
    let stride = (cells as f64 / target as f64).sqrt().ceil().max(1.0) as usize;
    for r in (0..p.rows).step_by(stride) {
        for c in (0..p.cols).step_by(stride) {
            let Some(z) = p.z[r * p.cols + c] else { continue };
            let (x, y, e) = (c as f64, r as f64, stride as f64);
            let q: Vec<((f32, f32), f64)> = [(x, y), (x + e, y), (x + e, y + e), (x, y + e)].iter().map(|&(a, b)| v.pt(a, b, z)).collect();
            let depth = q.iter().map(|q| q.1).sum::<f64>() / 4.0;
            items.push((depth, poly(q.iter().map(|q| q.0).collect(), v.ctx.color(z), !v.ctx.s.surface)));
            // Draw a riser where the adjacent tile is lower or absent. The
            // higher tile owns shared walls, so each height transition is emitted once.
            let faces = [
                (r > 0, c, r.saturating_sub(stride), [(x,y),(x+e,y)]),
                (c + stride < p.cols, c + stride, r, [(x+e,y),(x+e,y+e)]),
                (r + stride < p.rows, c, r + stride, [(x+e,y+e),(x,y+e)]),
                (c > 0, c.saturating_sub(stride), r, [(x,y+e),(x,y)]),
            ];
            for (has_neighbor, nc, nr, edge) in faces {
                let neighbor = if has_neighbor { p.z.get(nr * p.cols + nc).copied().flatten() } else { None };
                if neighbor.is_some_and(|nz| nz >= z) { continue; }
                let low = neighbor.unwrap_or(v.map.zlo).min(z);
                let (a,b) = (edge[0], edge[1]);
                let pts = [(a.0,a.1,z),(b.0,b.1,z),(b.0,b.1,low),(a.0,a.1,low)];
                let pr: Vec<_> = pts.iter().map(|&(xx,yy,zz)| v.pt(xx,yy,zz)).collect();
                let depth = pr.iter().map(|q| q.1).sum::<f64>() / 4.0;
                items.push((depth, poly(pr.iter().map(|q| q.0).collect(), v.ctx.color((z+low)*0.5), !v.ctx.s.surface)));
            }
        }
    }
}

/// Floor grid, bounding box and scale of the bird's-eye view.
fn floor_grid(v: &View, ops: &mut Vec<Op>) {
    let s = v.ctx.s;
    let (col, lw) = (s.fg, s.grid_line_width.max(1) as f32);
    let m = &v.map;
    let (wd, hd, hz) = (m.wd, m.hd, m.hz);
    let font = s.axis_scale_font();
    let wext = |t: &str| measure_text(t, font).0;
    let hext = |t: &str| measure_text(t, font).1;
    let (x1, y1) = (m.x0 + m.xr, m.y0 + m.yr);
    if let Some((c, k)) = &m.cyl {
        let rt = ticks(c.r_min, c.r_max, pick_step(c.r_min, c.r_max, s.memory_x, wd / 2.0, &wext));
        let tt = ticks(c.th_min, c.th_max, pick_step(c.th_min, c.th_max, s.memory_y, wd * 1.5, &wext));
        if s.grid_mesh {
            for &r in rt.iter().filter(|&&r| r > c.r_min) {
                let pts = (0..=72).map(|i| {
                    let a = TAU * i as f64 / 72.0;
                    let rr = (r - c.r_min) * k;
                    v.proj.at(wd / 2.0 + rr * a.cos(), hd / 2.0 + rr * a.sin(), 0.0).0
                });
                ops.push(Op::Polyline { pts: pts.collect(), color: col, width: lw });
            }
            for &t in tt.iter().filter(|&&t| (t - c.th_max).abs() > (c.th_max - c.th_min).abs() * 1e-9) {
                let a = (t - c.th_min) / (c.th_max - c.th_min) * TAU;
                let r = wd / 2.0;
                v.line(ops, (wd / 2.0, hd / 2.0, 0.0), (wd / 2.0 + r * a.cos(), hd / 2.0 + r * a.sin(), 0.0), col, lw);
            }
        }
        if s.grid_box {
            for z in [0.0, hz] {
                let pts = (0..=72).map(|i| {
                    let a = TAU * i as f64 / 72.0;
                    v.proj.at(wd / 2.0 + wd / 2.0 * a.cos(), hd / 2.0 + hd / 2.0 * a.sin(), z).0
                });
                ops.push(Op::Polyline { pts: pts.collect(), color: col, width: lw });
            }
        }
        return;
    }
    let xt = ticks(m.x0, x1, pick_step(m.x0, x1, s.memory_x, wd, &wext));
    let yt = ticks(m.y0, y1, pick_step(m.y0, y1, s.memory_y, hd, &hext));
    if s.grid_mesh {
        for &x in &xt {
            let wx = m.xy(x, m.y0).0;
            v.line(ops, (wx, 0.0, 0.0), (wx, hd, 0.0), col, lw);
        }
        for &y in &yt {
            let wy = m.xy(m.x0, y).1;
            v.line(ops, (0.0, wy, 0.0), (wd, wy, 0.0), col, lw);
        }
    }
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
    let len = s.grid_line_length as f64;
    let out = (-len).max(0.0) + 4.0;
    let (sx, sy) = (if near.0 == 0.0 { -1.0 } else { 1.0 }, if near.1 == 0.0 { -1.0 } else { 1.0 });
    let label = |ops: &mut Vec<Op>, p: (f64, f64, f64), text: String, anchor: Anchor| {
        let (q, _) = v.proj.at(p.0, p.1, p.2);
        push_text(ops, v.bounds, q.0, q.1, &text, font, col, anchor);
    };
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
    let zs = ticks(m.zlo, m.zlo + m.zr, pick_step(m.zlo, m.zlo + m.zr, s.memory_z, hz, &hext));
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

fn axes(v: &View, ops: &mut Vec<Op>) {
    let s = v.ctx.s;
    let m = &v.map;
    let (col, lw, head) = (s.fg, s.axis_line_width.max(1) as f32, s.axis_head_size as f32);
    let o = if m.cyl.is_some() {
        (m.wd / 2.0, m.hd / 2.0)
    } else {
        m.xy(0f64.clamp(m.x0, m.x0 + m.xr), 0f64.clamp(m.y0, m.y0 + m.yr))
    };
    let (xs, ys) = if s.axis_cross { ((0.0, o.1), (o.0, 0.0)) } else { (o, o) };
    let ends = [
        ((xs.0, xs.1, 0.0), (m.wd, o.1, 0.0), &s.title_x),
        ((ys.0, ys.1, 0.0), (o.0, m.hd, 0.0), &s.title_y),
        ((o.0, o.1, 0.0), (o.0, o.1, m.hz), &s.title_z),
    ];
    for (a, b, title) in ends {
        let (pa, pb) = (v.proj.at(a.0, a.1, a.2).0, v.proj.at(b.0, b.1, b.2).0);
        ops.push(Op::Polyline { pts: vec![pa, pb], color: col, width: lw });
        arrow_head(ops, pa, pb, head, col);
        if s.axis_title {
            let (dx, dy) = (pb.0 - pa.0, pb.1 - pa.1);
            let n = dx.hypot(dy).max(1e-3);
            push_text(ops, v.bounds, pb.0 + dx / n * 8.0, pb.1 + dy / n * 8.0, title, s.axis_title_font(), col, Anchor::Center);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::super::*;

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
    fn surface_makes_polygons_and_wire_makes_lines() {
        let f = grid(6, 6, |x, y| Some(x + y));
        let b = build_scene(&f, &land(), &[]).unwrap();
        let polys = count_ops(&b.scene.ops, |o| matches!(o, Op::Polygon { .. }));
        assert!(polys >= 25);
        let mut s = land();
        s.surface = false;
        let b = build_scene(&f, &s, &[]).unwrap();
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polygon { .. })), 0);
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polyline { .. })), 60); // 2 * 6 * 5
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
}
