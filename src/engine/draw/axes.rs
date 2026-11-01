//! Coordinate axes, grid lines, tick marks and numbers, data points.
use super::plane::{dot_op, nodes};
use super::{Ctx, Frame, push_text};
use crate::engine::field::Field;
use crate::engine::raster::measure_text;
use crate::engine::scene::{Anchor, FontRef, Op, Rgb};
use std::f64::consts::TAU;

const MAX_TICKS: usize = 500;
const MAX_POINTS: usize = 50_000;
const MAX_EDGES: usize = 300_000;

/// Number in general format with trailing zeros removed (1.2E+05 for large or tiny values).
pub fn fmt_num(v: f64) -> String {
    if !v.is_finite() {
        return "?".into();
    }
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    if !(1e-4..1e5).contains(&a) {
        let e = format!("{:.9e}", v);
        let (m, ex) = e.split_once('e').unwrap_or((&e, "0"));
        let m = m.trim_end_matches('0').trim_end_matches('.');
        let ex: i32 = ex.parse().unwrap_or(0);
        return format!("{}E{}{:02}", m, if ex < 0 { '-' } else { '+' }, ex.abs());
    }
    let s = format!("{:.10}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// Standard interval: a 1, 2 or 5 times a power of ten giving about ten divisions at most.
pub fn standard_step(range: f64) -> f64 {
    if !(range > 0.0) || !range.is_finite() {
        return 1.0;
    }
    let raw = range / 10.0;
    let mag = 10f64.powf(raw.log10().floor());
    let m = raw / mag;
    mag * if m <= 1.0 + 1e-9 { 1.0 } else if m <= 2.0 { 2.0 } else if m <= 5.0 { 5.0 } else { 10.0 }
}

/// Next larger 1-2-5 value.
fn coarser(step: f64) -> f64 {
    let mag = 10f64.powf(step.log10().floor());
    let m = (step / mag).round();
    mag * if m < 1.5 { 2.0 } else if m < 3.5 { 5.0 } else { 10.0 }
}

/// Multiples of `step` within lo..=hi.
pub fn ticks(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    if !(step > 0.0) || !(hi >= lo) || !lo.is_finite() || !hi.is_finite() {
        return vec![];
    }
    let k0 = (lo / step - 1e-9).ceil() as i64;
    let k1 = (hi / step + 1e-9).floor() as i64;
    (k0..=k1.min(k0 + MAX_TICKS as i64 * 2)).map(|k| if k == 0 { 0.0 } else { k as f64 * step }).collect()
}

/// Interval from the CBmemory setting: 0 picks a spacing where the labels do not overlap.
/// `extent` is the label size along the axis in pixels and `span_px` the axis length.
pub fn pick_step(lo: f64, hi: f64, memory: u8, span_px: f64, extent: &dyn Fn(&str) -> f32) -> f64 {
    let range = hi - lo;
    if !(range > 0.0) {
        return 1.0;
    }
    let std = standard_step(range);
    let mut step = match memory {
        0 => std,
        1 => std * 10.0,
        2 => std * 5.0,
        3 => std * 2.0,
        5 => std * 0.5,
        6 => std * 0.2,
        7 => std * 0.1,
        _ => std,
    };
    if memory == 0 {
        for _ in 0..40 {
            let t = ticks(lo, hi, step);
            let need = t.iter().map(|&v| extent(&fmt_num(v))).fold(0.0, f32::max) as f64 + 6.0;
            if t.len() <= 2 || step * span_px / range >= need {
                break;
            }
            step = coarser(step);
        }
    }
    while (range / step) > MAX_TICKS as f64 {
        step = coarser(step);
    }
    step
}

fn extents(font: &FontRef) -> (impl Fn(&str) -> f32 + '_, impl Fn(&str) -> f32 + '_) {
    (move |t: &str| measure_text(t, font).0, move |t: &str| measure_text(t, font).1)
}

fn line(ops: &mut Vec<Op>, a: (f32, f32), b: (f32, f32), color: Rgb, width: f32) {
    ops.push(Op::Polyline { pts: vec![a, b], color, width });
}

pub fn grid(ctx: &Ctx, frame: &Frame, bounds: (u32, u32), ops: &mut Vec<Op>) {
    let s = ctx.s;
    if !s.grid {
        return;
    }
    let (col, lw) = (s.fg, s.grid_line_width.max(1) as f32);
    let font = s.axis_scale_font();
    let (wext, hext) = extents(font);
    let len = s.grid_line_length as f32;
    let out = (-len).max(0.0) + 3.0;
    // a triangle mesh shows its cell outlines instead of lines at the ticks
    let edges = s.grid_mesh && matches!(ctx.field, Field::Mesh(_));
    if let (true, Field::Mesh(m)) = (edges, ctx.field) {
        let mut seen = std::collections::HashSet::new();
        for t in &m.tris {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                if seen.len() < MAX_EDGES && seen.insert((a.min(b), a.max(b))) {
                    let (pa, pb) = (frame.to_px(m.nodes[a].0, m.nodes[a].1), frame.to_px(m.nodes[b].0, m.nodes[b].1));
                    line(ops, pa, pb, col, lw);
                }
            }
        }
    }
    let mesh = s.grid_mesh && !edges;
    if let Some(c) = &frame.cyl {
        let (cx, cy) = frame.centre();
        let (cx, cy) = (cx as f32, cy as f32);
        let radius = (frame.w / 2.0) as f32;
        let k = frame.cyl_scale();
        let rt = ticks(c.r_min, c.r_max, pick_step(c.r_min, c.r_max, s.memory_x, frame.w / 2.0, &wext));
        let tt = ticks(c.th_min, c.th_max, pick_step(c.th_min, c.th_max, s.memory_y, std::f64::consts::PI * frame.w / 2.0, &wext));
        let span = c.th_max - c.th_min;
        let ang = |th: f64| (th - c.th_min) / span * TAU;
        let tts: Vec<f64> = tt.into_iter().filter(|&t| span.abs() > 0.0 && (t - c.th_max).abs() > span.abs() * 1e-9).collect();
        if mesh {
            for &r in rt.iter().filter(|&&r| r > c.r_min) {
                let rr = ((r - c.r_min) * k) as f32;
                ops.push(Op::Ellipse { cx, cy, rx: rr, ry: rr, fill: None, stroke: Some(col) });
            }
            for &t in &tts {
                let a = ang(t);
                line(ops, (cx, cy), (cx + radius * a.cos() as f32, cy - radius * a.sin() as f32), col, lw);
            }
        }
        if s.grid_box {
            ops.push(Op::Ellipse { cx, cy, rx: radius, ry: radius, fill: None, stroke: Some(col) });
        }
        if s.grid_scale && len != 0.0 {
            for &r in &rt {
                let x = cx + ((r - c.r_min) * k) as f32;
                line(ops, (x, cy), (x, cy - len), col, lw);
            }
            for &t in &tts {
                let a = ang(t);
                let (ca, sa) = (a.cos() as f32, a.sin() as f32);
                line(ops, (cx + radius * ca, cy - radius * sa), (cx + (radius - len) * ca, cy - (radius - len) * sa), col, lw);
            }
        }
        if s.axis_scale {
            for &r in &rt {
                push_text(ops, bounds, cx + ((r - c.r_min) * k) as f32, cy + out, &fmt_num(r), font, col, Anchor::TopCenter);
            }
            for &t in &tts {
                let a = ang(t);
                let d = radius + out + hext("0") / 2.0 + 2.0;
                push_text(ops, bounds, cx + d * a.cos() as f32, cy - d * a.sin() as f32, &fmt_num(t), font, col, Anchor::Center);
            }
        }
        return;
    }
    let (left, top) = (frame.ox as f32, frame.oy as f32);
    let (right, bottom) = ((frame.ox + frame.w) as f32, (frame.oy + frame.h) as f32);
    let xt = ticks(frame.x0, frame.x1, pick_step(frame.x0, frame.x1, s.memory_x, frame.w, &wext));
    let yt = ticks(frame.y0, frame.y1, pick_step(frame.y0, frame.y1, s.memory_y, frame.h, &hext));
    let xp = |x: f64| frame.to_px(x, frame.y0).0.round();
    let yp = |y: f64| frame.to_px(frame.x0, y).1.round();
    if mesh {
        for &x in &xt {
            line(ops, (xp(x), top), (xp(x), bottom), col, lw);
        }
        for &y in &yt {
            line(ops, (left, yp(y)), (right, yp(y)), col, lw);
        }
    }
    if s.grid_box {
        ops.push(Op::Polyline { pts: vec![(left, top), (right, top), (right, bottom), (left, bottom), (left, top)], color: col, width: lw });
    }
    if s.grid_scale && len != 0.0 {
        for &x in &xt {
            line(ops, (xp(x), bottom), (xp(x), bottom - len), col, lw);
        }
        for &y in &yt {
            line(ops, (left, yp(y)), (left + len, yp(y)), col, lw);
        }
    }
    if s.axis_scale {
        for &x in &xt {
            push_text(ops, bounds, xp(x), bottom + out, &fmt_num(x), font, col, Anchor::TopCenter);
        }
        for &y in &yt {
            push_text(ops, bounds, left - out, yp(y), &fmt_num(y), font, col, Anchor::MiddleRight);
        }
    }
}

/// Filled arrow head with its tip at `to`.
pub(crate) fn arrow_head(ops: &mut Vec<Op>, from: (f32, f32), to: (f32, f32), size: f32, color: Rgb) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let n = dx.hypot(dy);
    if size <= 0.0 || n < 1e-3 {
        return;
    }
    let (ux, uy) = (dx / n, dy / n);
    let (bx, by) = (to.0 - ux * size, to.1 - uy * size);
    let w = size * 0.35;
    ops.push(Op::Polygon { pts: vec![to, (bx - uy * w, by + ux * w), (bx + uy * w, by - ux * w)], fill: Some(color), stroke: Some(color) });
}

pub fn axes(ctx: &Ctx, frame: &Frame, bounds: (u32, u32), ops: &mut Vec<Op>) {
    let s = ctx.s;
    if !s.axis {
        return;
    }
    let (col, lw, head) = (s.fg, s.axis_line_width.max(1) as f32, s.axis_head_size as f32);
    // (start, end) of each axis in pixels, end = positive direction
    let (x_axis, y_axis) = if frame.cyl.is_some() {
        let (cx, cy) = frame.centre();
        let r = frame.w / 2.0;
        let (cx, cy, r) = (cx as f32, cy as f32, r as f32);
        let (xs, ys) = if s.axis_cross { (cx - r, cy + r) } else { (cx, cy) };
        (((xs, cy), (cx + r, cy)), ((cx, ys), (cx, cy - r)))
    } else {
        if !(frame.x0 <= 0.0 && 0.0 <= frame.x1 && frame.y0 <= 0.0 && 0.0 <= frame.y1) {
            return;
        }
        let o = frame.to_px(0.0, 0.0);
        let (xneg, xpos) = (frame.to_px(frame.x0, 0.0), frame.to_px(frame.x1, 0.0));
        let (yneg, ypos) = (frame.to_px(0.0, frame.y0), frame.to_px(0.0, frame.y1));
        (((if s.axis_cross { xneg.0 } else { o.0 }, o.1), xpos), ((o.0, if s.axis_cross { yneg.1 } else { o.1 }), ypos))
    };
    for (a, b) in [x_axis, y_axis] {
        // stop the line at the arrow base so that a thick line does not poke through the tip
        let n = (b.0 - a.0).hypot(b.1 - a.1).max(1e-3);
        let back = head.min(n);
        let end = (b.0 - (b.0 - a.0) / n * back * 0.8, b.1 - (b.1 - a.1) / n * back * 0.8);
        line(ops, a, if head > 0.0 { end } else { b }, col, lw);
        arrow_head(ops, a, b, head, col);
    }
    if s.axis_title {
        let font = s.axis_title_font();
        let (xe, ye) = (x_axis.1, y_axis.1);
        push_text(ops, bounds, xe.0 + 4.0, xe.1, &s.title_x, font, col, Anchor::MiddleLeft);
        let (anchor, dy) = if ye.1 < y_axis.0.1 { (Anchor::BottomCenter, -4.0) } else { (Anchor::TopCenter, 4.0) };
        push_text(ops, bounds, ye.0, ye.1 + dy, &s.title_y, font, col, anchor);
    }
}

/// Small marks at the data points (cell centres for panels).
pub fn points(ctx: &Ctx, frame: &Frame, ops: &mut Vec<Op>) {
    if !ctx.s.point {
        return;
    }
    let mark = |ops: &mut Vec<Op>, x: f64, y: f64| {
        let (px, py) = frame.to_px(x, y);
        ops.push(dot_op(0, px, py, 3.0, ctx.s.fg));
    };
    if let Field::Panel(p) = ctx.field {
        let stride = (p.rows * p.cols).div_ceil(MAX_POINTS).max(1);
        for k in (0..p.rows * p.cols).step_by(stride) {
            mark(ops, (k % p.cols) as f64 + 0.5, (k / p.cols) as f64 + 0.5);
        }
        return;
    }
    let stride = nodes(ctx.field).count().div_ceil(MAX_POINTS).max(1);
    for n in nodes(ctx.field).step_by(stride) {
        mark(ops, n.x, n.y);
    }
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::super::*;
    use super::{coarser, fmt_num, pick_step, standard_step, ticks};

    #[test]
    fn number_format() {
        assert_eq!(fmt_num(0.5), "0.5");
        assert_eq!(fmt_num(10.0), "10");
        assert_eq!(fmt_num(120000.0), "1.2E+05");
        assert_eq!(fmt_num(0.1 + 0.2), "0.3");
        assert_eq!(fmt_num(-0.0), "0");
        assert_eq!(fmt_num(-2.5), "-2.5");
        assert_eq!(fmt_num(0.00002), "2E-05");
        assert_eq!(fmt_num(99999.0), "99999");
    }

    #[test]
    fn nice_numbers() {
        assert_eq!(standard_step(39.0), 5.0);
        assert_eq!(standard_step(10.0), 1.0);
        assert_eq!(standard_step(100.0), 10.0);
        assert!((standard_step(0.0123) - 0.002).abs() < 1e-12);
        assert_eq!(standard_step(0.0), 1.0);
        assert_eq!(coarser(1.0), 2.0);
        assert_eq!(coarser(2.0), 5.0);
        assert_eq!(coarser(5.0), 10.0);
        assert_eq!(ticks(0.0, 39.0, 5.0), vec![0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0]);
        assert_eq!(ticks(-1.0, 1.0, 0.5).len(), 5);
    }

    #[test]
    fn memory_ratios_and_auto_spacing() {
        let narrow = |_: &str| 10.0f32;
        assert_eq!(pick_step(0.0, 100.0, 4, 500.0, &narrow), 10.0);
        assert_eq!(pick_step(0.0, 100.0, 1, 500.0, &narrow), 100.0);
        assert_eq!(pick_step(0.0, 100.0, 2, 500.0, &narrow), 50.0);
        assert_eq!(pick_step(0.0, 100.0, 3, 500.0, &narrow), 20.0);
        assert_eq!(pick_step(0.0, 100.0, 5, 500.0, &narrow), 5.0);
        assert_eq!(pick_step(0.0, 100.0, 7, 500.0, &narrow), 1.0);
        assert_eq!(pick_step(0.0, 100.0, 0, 500.0, &narrow), 10.0);
        // wide labels on a short axis force a coarser spacing
        let wide = |_: &str| 40.0f32;
        assert!(pick_step(0.0, 100.0, 0, 100.0, &wide) > 10.0);
    }

    #[test]
    fn axes_and_grid_emit_ops() {
        let f = grid(5, 5, |x, y| Some(x + y));
        let mut s = settings();
        s.axis = true;
        s.axis_cross = true;
        s.axis_head_size = 8;
        s.grid = true;
        s.grid_mesh = true;
        s.grid_box = true;
        s.grid_scale = false;
        s.axis_scale = false;
        s.memory_x = 2;
        s.memory_y = 2;
        let b = build_scene(&f, &s, &[]).unwrap();
        // ticks at 0 and 2.5 on each axis + box + 2 axes
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polyline { .. })), 2 + 2 + 1 + 2);
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polygon { .. })), 2);
        // axes are skipped when the origin is out of range
        let g = {
            use crate::engine::data::{DataFile, Point};
            let mut df = DataFile::default();
            for j in 1..4 {
                for i in 1..4 {
                    df.points.push(Point { x: Some(i as f64), y: Some(j as f64), z: Some(1.0), u: None, v: None });
                }
            }
            crate::engine::field::build(&df, &s).unwrap().0
        };
        s.grid = false;
        let b = build_scene(&g, &s, &[]).unwrap();
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polyline { .. })), 0);
    }

    #[test]
    fn cylindrical_grid_has_circles_and_spokes() {
        let f = grid(9, 5, |x, y| Some(x + y));
        let mut s = settings();
        s.cylindrical = true;
        s.grid = true;
        s.grid_mesh = true;
        s.memory_x = 4;
        s.memory_y = 4;
        let b = build_scene(&f, &s, &[]).unwrap();
        assert!(count_ops(&b.scene.ops, |o| matches!(o, Op::Ellipse { .. })) >= 2);
        assert!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polyline { .. })) >= 4);
    }

    #[test]
    fn points_mark_nodes() {
        let f = grid(3, 4, |x, y| Some(x + y));
        let mut s = settings();
        s.point = true;
        let b = build_scene(&f, &s, &[]).unwrap();
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Polygon { .. })), 12);
    }
}
