//! Colour bar under the plot with the minimum and maximum values.
use super::axes::fmt_num;
use super::{Ctx, Layout, push_text};
use crate::engine::scene::{Anchor, Op};

pub(crate) fn draw(ctx: &Ctx, lay: &Layout, ops: &mut Vec<Op>) {
    let s = ctx.s;
    let Some((lo, hi)) = ctx.zr.filter(|_| s.legend) else { return };
    let (w, h) = (s.legend_w as usize, s.legend_h as usize);
    let x = ((lay.w as f64 - w as f64) / 2.0).floor();
    let y = lay.legend_y;
    let pal = ctx.grad.palette();
    let mut rgba = Vec::with_capacity(w * h * 4);
    for _ in 0..h {
        for i in 0..w {
            let c = pal[(i * pal.len() / w).min(pal.len() - 1)];
            rgba.extend_from_slice(&[c.0, c.1, c.2, 255]);
        }
    }
    ops.push(Op::Image { x: x as i32, y: y as i32, w: w as u32, h: h as u32, rgba });
    let (x0, y0, x1, y1) = (x as f32 - 1.0, y as f32 - 1.0, x as f32 + w as f32, y as f32 + h as f32);
    ops.push(Op::Polygon { pts: vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)], fill: None, stroke: Some(s.fg) });
    let bounds = (lay.w, lay.h);
    let ty = y1 + 3.0;
    push_text(ops, bounds, x0, ty, &fmt_num(lo), s.legend_font(), s.fg, Anchor::TopLeft);
    push_text(ops, bounds, x1, ty, &fmt_num(hi), s.legend_font(), s.fg, Anchor::TopRight);
}

#[cfg(test)]
mod tests {
    use super::super::testutil::*;
    use super::super::*;

    #[test]
    fn legend_bar_has_one_step_per_colour() {
        let f = grid(5, 5, |x, y| Some(x + y));
        let mut s = settings();
        s.legend = true;
        s.n_colors = 4;
        s.legend_w = 40;
        s.legend_h = 5;
        let b = build_scene(&f, &s, &[]).unwrap();
        let bar = b.scene.ops.iter().find_map(|o| match o {
            Op::Image { w: 40, h: 5, rgba, .. } => Some(rgba.clone()),
            _ => None,
        });
        let rgba = bar.expect("legend image");
        let mut cols: Vec<_> = rgba[..40 * 4].chunks(4).map(|p| [p[0], p[1], p[2]]).collect();
        cols.dedup();
        assert_eq!(cols.len(), 4);
        assert!(b.scene.height > 100 + 20);
    }

    #[test]
    fn labels_are_drawn_when_enabled() {
        let f = grid(3, 3, |x, y| Some(x + y));
        let labels = [crate::engine::data::Label { x: 5.0, y: 6.0, text: "A".into() }];
        let mut s = settings();
        s.label = true;
        let b = build_scene(&f, &s, &labels).unwrap();
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Text { text, .. } if text == "A")), 1);
        s.label = false;
        let b = build_scene(&f, &s, &labels).unwrap();
        assert_eq!(count_ops(&b.scene.ops, |o| matches!(o, Op::Text { .. })), 0);
    }
}
