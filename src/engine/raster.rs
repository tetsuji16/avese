//! Draws a `Scene` into a tiny-skia pixmap. Lines and shapes are not anti-aliased; text uses ab_glyph.
use crate::engine::scene::{Anchor, FontRef, Op, Rgb, Scene};
use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tiny_skia::{Color, FillRule, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform};

type FontCache = Mutex<HashMap<(String, u32), Option<Arc<FontVec>>>>;

fn cache() -> &'static FontCache {
    static C: OnceLock<FontCache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn font_dir() -> PathBuf {
    let root = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    root.join("Fonts")
}

/// Lower case, fullwidth ASCII folded, spaces removed.
fn normalize(name: &str) -> String {
    name.chars()
        .filter_map(|c| match c {
            ' ' | '\u{3000}' => None,
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).map(|c| c.to_ascii_lowercase()),
            c => Some(c.to_ascii_lowercase()),
        })
        .collect()
}

/// Font files (and collection indices) to try for a family name, best first.
fn candidates(family: &str) -> Vec<(&'static str, u32)> {
    let mut v: Vec<(&'static str, u32)> = match normalize(family).as_str() {
        "msゴシック" | "msgothic" | "terminal" | "system" | "fixedsys" => vec![("msgothic.ttc", 0)],
        "mspゴシック" | "mspgothic" => vec![("msgothic.ttc", 1)],
        "msuiゴシック" | "msuigothic" => vec![("msgothic.ttc", 2)],
        "ms明朝" | "msmincho" => vec![("msmincho.ttc", 0)],
        "msp明朝" | "mspmincho" => vec![("msmincho.ttc", 1)],
        "arial" | "helvetica" | "sansserif" => vec![("arial.ttf", 0)],
        "timesnewroman" | "times" | "serif" => vec![("times.ttf", 0)],
        "couriernew" | "courier" => vec![("cour.ttf", 0)],
        "tahoma" => vec![("tahoma.ttf", 0)],
        "verdana" => vec![("verdana.ttf", 0)],
        "segoeui" => vec![("segoeui.ttf", 0)],
        "meiryo" | "メイリオ" => vec![("meiryo.ttc", 0)],
        "yugothic" | "游ゴシック" => vec![("YuGothR.ttc", 0)],
        _ => vec![],
    };
    v.extend([("msgothic.ttc", 0), ("meiryo.ttc", 0), ("YuGothR.ttc", 0), ("arial.ttf", 0)]);
    v
}

fn load_face(file: &str, index: u32) -> Option<Arc<FontVec>> {
    let mut c = cache().lock().ok()?;
    c.entry((file.to_string(), index))
        .or_insert_with(|| {
            let bytes = std::fs::read(font_dir().join(file)).ok()?;
            FontVec::try_from_vec_and_index(bytes, index).ok().map(Arc::new)
        })
        .clone()
}

fn find_face(family: &str) -> Option<Arc<FontVec>> {
    candidates(family).into_iter().find_map(|(f, i)| load_face(f, i))
}

fn px_per_pt(size_pt: f32) -> f32 {
    size_pt.clamp(1.0, 500.0) * 96.0 / 72.0
}

fn scale_of(face: &FontVec, size_pt: f32) -> PxScale {
    face.pt_to_px_scale(size_pt.clamp(1.0, 500.0)).unwrap_or(PxScale::from(px_per_pt(size_pt)))
}

/// Width and height in pixels. Without an installed font, an estimate is returned so layout still works.
pub fn measure_text(text: &str, font: &FontRef) -> (f32, f32) {
    match find_face(&font.family) {
        Some(face) => {
            let sf = face.as_scaled(scale_of(&face, font.size_pt));
            let mut w = 0.0;
            let mut prev = None;
            for ch in text.chars() {
                let g = face.glyph_id(ch);
                if let Some(p) = prev {
                    w += sf.kern(p, g);
                }
                w += sf.h_advance(g);
                prev = Some(g);
            }
            (w + if font.style & 1 != 0 { 1.0 } else { 0.0 }, sf.ascent() - sf.descent())
        }
        None => {
            let px = px_per_pt(font.size_pt);
            let w: f32 = text.chars().map(|c| if c.is_ascii() { 0.55 } else { 1.0 }).sum();
            (w * px, px * 1.2)
        }
    }
}

/// Top-left corner of a box of size (w, h) placed at (x, y) with the given anchor.
pub fn anchor_origin(a: Anchor, x: f32, y: f32, w: f32, h: f32) -> (f32, f32) {
    use Anchor::*;
    let dx = match a {
        TopLeft | MiddleLeft | BottomLeft => 0.0,
        TopCenter | Center | BottomCenter => w / 2.0,
        TopRight | MiddleRight | BottomRight => w,
    };
    let dy = match a {
        TopLeft | TopCenter | TopRight => 0.0,
        MiddleLeft | Center | MiddleRight => h / 2.0,
        BottomLeft | BottomCenter | BottomRight => h,
    };
    (x - dx, y - dy)
}

fn paint(c: Rgb) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(Color::from_rgba8(c.0, c.1, c.2, 255));
    p.anti_alias = false;
    p
}

fn finite(pts: &[(f32, f32)]) -> bool {
    pts.iter().all(|p| p.0.is_finite() && p.1.is_finite() && p.0.abs() < 1e7 && p.1.abs() < 1e7)
}

fn poly_path(pts: &[(f32, f32)], close: bool, off: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(pts.first()?.0 + off, pts[0].1 + off);
    for p in &pts[1..] {
        pb.line_to(p.0 + off, p.1 + off);
    }
    if close {
        pb.close();
    }
    pb.finish()
}

fn put_image(pm: &mut Pixmap, x: i32, y: i32, w: u32, h: u32, rgba: &[u8]) {
    let (pw, ph) = (pm.width() as i64, pm.height() as i64);
    if rgba.len() < w as usize * h as usize * 4 {
        return;
    }
    let data = pm.data_mut();
    for j in 0..h as i64 {
        let ty = y as i64 + j;
        if ty < 0 || ty >= ph {
            continue;
        }
        for i in 0..w as i64 {
            let tx = x as i64 + i;
            if tx < 0 || tx >= pw {
                continue;
            }
            let s = ((j * w as i64 + i) * 4) as usize;
            if rgba[s + 3] == 0 {
                continue;
            }
            let d = ((ty * pw + tx) * 4) as usize;
            data[d..d + 3].copy_from_slice(&rgba[s..s + 3]);
            data[d + 3] = 255;
        }
    }
}

fn blend(pm: &mut Pixmap, x: i32, y: i32, c: Rgb, cov: f32) {
    if x < 0 || y < 0 || x >= pm.width() as i32 || y >= pm.height() as i32 || cov <= 0.0 {
        return;
    }
    let i = (y as usize * pm.width() as usize + x as usize) * 4;
    let d = pm.data_mut();
    for (k, v) in [c.0, c.1, c.2].into_iter().enumerate() {
        d[i + k] = (d[i + k] as f32 + (v as f32 - d[i + k] as f32) * cov.min(1.0)).round() as u8;
    }
}

fn draw_text(pm: &mut Pixmap, x: f32, y: f32, text: &str, font: &FontRef, color: Rgb, anchor: Anchor) {
    let Some(face) = find_face(&font.family) else { return };
    let scale = scale_of(&face, font.size_pt);
    let sf = face.as_scaled(scale);
    let (w, h) = measure_text(text, font);
    let (left, top) = anchor_origin(anchor, x, y, w, h);
    let base = top + sf.ascent();
    let strikes = if font.style & 1 != 0 { 2 } else { 1 };
    let mut cx = left;
    let mut prev = None;
    for ch in text.chars() {
        let id = face.glyph_id(ch);
        if let Some(p) = prev {
            cx += sf.kern(p, id);
        }
        if let Some(o) = face.outline_glyph(id.with_scale_and_position(scale, point(cx, base))) {
            let b = o.px_bounds();
            for s in 0..strikes {
                o.draw(|gx, gy, cov| blend(pm, b.min.x as i32 + gx as i32 + s, b.min.y as i32 + gy as i32, color, cov));
            }
        }
        cx += sf.h_advance(id);
        prev = Some(id);
    }
    let mut hline = |yy: f32| {
        for xx in left.round() as i32..(left + w).round() as i32 {
            blend(pm, xx, yy.round() as i32, color, 1.0);
        }
    };
    if font.style & 4 != 0 {
        hline(base + 1.5);
    }
    if font.style & 8 != 0 {
        hline(base - sf.ascent() * 0.35);
    }
}

pub fn rasterize(scene: &Scene) -> Pixmap {
    let mut pm = Pixmap::new(scene.width.max(1), scene.height.max(1)).unwrap_or_else(|| Pixmap::new(1, 1).expect("1x1 pixmap"));
    pm.fill(Color::from_rgba8(scene.bg.0, scene.bg.1, scene.bg.2, 255));
    let id = Transform::identity();
    for op in &scene.ops {
        match op {
            Op::Image { x, y, w, h, rgba } => put_image(&mut pm, *x, *y, *w, *h, rgba),
            Op::Polyline { pts, color, width } => {
                if pts.len() < 2 || !finite(pts) {
                    continue;
                }
                let off = if *width <= 1.5 { 0.5 } else { 0.0 };
                if let Some(path) = poly_path(pts, false, off) {
                    let stroke = Stroke { width: width.max(1.0), ..Stroke::default() };
                    pm.stroke_path(&path, &paint(*color), &stroke, id, None);
                }
            }
            Op::Polygon { pts, fill, stroke } => {
                if pts.len() < 2 || !finite(pts) {
                    continue;
                }
                if let Some(path) = poly_path(pts, true, 0.0) {
                    if let Some(c) = fill {
                        pm.fill_path(&path, &paint(*c), FillRule::Winding, id, None);
                    }
                    if let Some(c) = stroke {
                        pm.stroke_path(&path, &paint(*c), &Stroke::default(), id, None);
                    }
                }
            }
            Op::Ellipse { cx, cy, rx, ry, fill, stroke } => {
                if !finite(&[(*cx, *cy), (*rx, *ry)]) {
                    continue;
                }
                let r = Rect::from_ltrb(cx - rx.max(0.25), cy - ry.max(0.25), cx + rx.max(0.25), cy + ry.max(0.25));
                if let Some(path) = r.and_then(PathBuilder::from_oval) {
                    if let Some(c) = fill {
                        pm.fill_path(&path, &paint(*c), FillRule::Winding, id, None);
                    }
                    if let Some(c) = stroke {
                        pm.stroke_path(&path, &paint(*c), &Stroke::default(), id, None);
                    }
                }
            }
            Op::Text { x, y, text, font, color, anchor } => {
                if x.is_finite() && y.is_finite() {
                    draw_text(&mut pm, *x, *y, text, font, *color, *anchor);
                }
            }
        }
    }
    pm
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(ops: Vec<Op>) -> Scene {
        Scene { width: 20, height: 20, bg: Rgb(10, 20, 30), ops }
    }

    fn px(pm: &Pixmap, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let p = pm.pixel(x, y).unwrap();
        (p.red(), p.green(), p.blue(), p.alpha())
    }

    #[test]
    fn background_is_opaque() {
        let pm = rasterize(&scene(vec![]));
        assert_eq!(px(&pm, 3, 3), (10, 20, 30, 255));
    }

    #[test]
    fn image_skips_transparent_pixels() {
        let rgba = vec![255, 0, 0, 255, 0, 255, 0, 0];
        let pm = rasterize(&scene(vec![Op::Image { x: 2, y: 2, w: 2, h: 1, rgba }]));
        assert_eq!(px(&pm, 2, 2), (255, 0, 0, 255));
        assert_eq!(px(&pm, 3, 2), (10, 20, 30, 255));
    }

    #[test]
    fn polygon_fill_and_line() {
        let ops = vec![
            Op::Polygon { pts: vec![(2.0, 2.0), (10.0, 2.0), (10.0, 10.0), (2.0, 10.0)], fill: Some(Rgb(1, 2, 3)), stroke: None },
            Op::Polyline { pts: vec![(0.0, 15.0), (19.0, 15.0)], color: Rgb(200, 0, 0), width: 1.0 },
        ];
        let pm = rasterize(&scene(ops));
        assert_eq!(px(&pm, 5, 5), (1, 2, 3, 255));
        assert_eq!(px(&pm, 5, 15), (200, 0, 0, 255));
        assert_eq!(px(&pm, 15, 5), (10, 20, 30, 255));
    }

    #[test]
    fn ellipse_fills_centre() {
        let pm = rasterize(&scene(vec![Op::Ellipse { cx: 10.0, cy: 10.0, rx: 5.0, ry: 5.0, fill: Some(Rgb(9, 9, 9)), stroke: None }]));
        assert_eq!(px(&pm, 10, 10), (9, 9, 9, 255));
        assert_eq!(px(&pm, 1, 1), (10, 20, 30, 255));
    }

    #[test]
    fn font_names_normalize() {
        assert_eq!(normalize("ＭＳ Ｐゴシック"), "mspゴシック");
        assert_eq!(normalize("MS PGothic"), "mspgothic");
        assert_eq!(candidates("ＭＳ Ｐ明朝")[0], ("msmincho.ttc", 1));
        assert_eq!(candidates("nonexistent")[0].0, "msgothic.ttc");
    }

    #[test]
    fn missing_font_does_not_panic() {
        let f = FontRef { family: "zzz".into(), size_pt: 10.0, style: 1 };
        let (w, h) = measure_text("abc", &f);
        assert!(w > 0.0 && h > 0.0);
        let s = scene(vec![Op::Text { x: 1.0, y: 1.0, text: "abc".into(), font: f, color: Rgb::BLACK, anchor: Anchor::Center }]);
        let _ = rasterize(&s);
    }
}
