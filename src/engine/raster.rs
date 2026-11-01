//! Draws a `Scene` into a tiny-skia pixmap. Lines and shapes are not anti-aliased; text uses ab_glyph.
use crate::engine::scene::{Anchor, FontRef, Op, Rgb, Scene};
use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tiny_skia::{Color, FillRule, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform};

type FontCache = Mutex<HashMap<(PathBuf, u32), Option<Arc<FontVec>>>>;

/// Horizontal lean of a synthesised italic, about 12 degrees (tan 12° ≈ 0.21).
const ITALIC_SHEAR: f32 = 0.21;

fn cache() -> &'static FontCache {
    static C: OnceLock<FontCache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn windows_font_dir() -> PathBuf {
    let root = std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    root.join("Fonts")
}

fn discover_system_fonts() -> Vec<PathBuf> {
    #[cfg(windows)]
    let roots = vec![windows_font_dir()];
    #[cfg(target_os = "macos")]
    let roots = vec![
        PathBuf::from("/System/Library/Fonts"), PathBuf::from("/Library/Fonts"),
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join("Library/Fonts"),
    ];
    #[cfg(target_os = "linux")]
    let roots = vec![
        PathBuf::from("/usr/share/fonts"), PathBuf::from("/usr/local/share/fonts"),
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".local/share/fonts"),
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(".fonts"),
    ];
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    let roots: Vec<PathBuf> = Vec::new();

    let mut dirs = Vec::new();
    let mut pending = roots;
    let mut visited = 0usize;
    while let Some(dir) = pending.pop() {
        visited += 1;
        if visited > 20_000 { break; }
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                let path = entry.path();
                let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("");
                if ["ttf", "ttc", "otf"].iter().any(|x| ext.eq_ignore_ascii_case(x)) {
                    dirs.push(path);
                }
            }
        }
    }
    dirs.sort();
    dirs.dedup();
    dirs
}

fn is_japanese_font_file(name: &str, ext: &str) -> bool {
    ["ttf", "ttc", "otf"].iter().any(|x| ext.eq_ignore_ascii_case(x))
        && ["notosanscjk", "notoserifcjk", "notosansjp", "notoserifjp", "ipaex", "ipag", "ipamp", "ipamincho", "hiragino", "ヒラギノ", "yugoth", "yumin", "msgothic", "msmincho"]
            .iter().any(|x| name.contains(x))
}

fn system_font_paths() -> &'static [PathBuf] {
    static PATHS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    PATHS.get_or_init(discover_system_fonts)
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

fn matches_system_family(family: &str, path: &std::path::Path) -> bool {
    let name: String = path.file_stem().and_then(|s| s.to_str()).unwrap_or("")
        .chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
    let family = normalize(family);
    let family: String = family.chars().filter(|c| c.is_alphanumeric()).collect();
    let key = match family.as_str() {
        "ipaexゴシック" | "ipaexgothic" => "ipaexg",
        "ipaex明朝" | "ipaexmincho" => "ipaexm",
        "游ゴシック" | "yugothic" => "yugoth",
        "游明朝" | "yumincho" => "yumin",
        "hiraginosans" => "hiragino",
        "notosanscjkjp" => "notosanscjk",
        "notoserifcjkjp" => "notoserifcjk",
        _ => family.as_str(),
    };
    !key.is_empty() && (name.contains(key) || (key == "hiragino" && name.contains("ヒラギノ")))
}

fn candidate_paths(family: &str) -> Vec<(PathBuf, u32)> {
    let mut out = Vec::new();
    // On other platforms, prefer the selected installed face before generic
    // fallbacks. Otherwise changing the family in the picker has no effect.
    #[cfg(not(windows))]
    out.extend(system_font_paths().iter().filter(|p| matches_system_family(family, p)).map(|p| (p.clone(), 0)));
    out.extend(candidates(family).into_iter().map(|(f, i)| (windows_font_dir().join(f), i)));
    out.extend(system_font_paths().iter().filter(|p| {
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
        is_japanese_font_file(&name, ext)
    }).map(|p| (p.clone(), 0)));
    out
}

fn load_face(path: &PathBuf, index: u32) -> Option<Arc<FontVec>> {
    let mut c = cache().lock().ok()?;
    c.entry((path.clone(), index))
        .or_insert_with(|| {
            let bytes = std::fs::read(path).ok()?;
            FontVec::try_from_vec_and_index(bytes, index).ok().map(Arc::new)
        })
        .clone()
}

fn find_face(family: &str) -> Option<Arc<FontVec>> {
    candidate_paths(family).iter().find_map(|(p, i)| load_face(p, *i))
}

fn px_per_pt(size_pt: f32) -> f32 {
    size_pt.clamp(1.0, 500.0) * 96.0 / 72.0
}

fn scale_of(face: &FontVec, size_pt: f32) -> PxScale {
    face.pt_to_px_scale(size_pt.clamp(1.0, 500.0)).unwrap_or(PxScale::from(px_per_pt(size_pt)))
}

struct PositionedGlyph {
    face: Arc<FontVec>,
    id: ab_glyph::GlyphId,
    scale: PxScale,
    x: f32,
}

struct TextLayout {
    glyphs: Vec<PositionedGlyph>,
    width: f32,
    ascent: f32,
    descent: f32,
}

/// Pick a fallback face per missing glyph, sharing the same layout for measuring and drawing.
fn layout_text(text: &str, font: &FontRef) -> Option<TextLayout> {
    let primary = find_face(&font.family)?;
    let paths = candidate_paths(&font.family);
    let mut paths = paths.iter();
    let mut faces = vec![primary];
    let mut glyphs = Vec::with_capacity(text.chars().count());
    let mut width = 0.0;
    let primary_scaled = faces[0].as_scaled(scale_of(&faces[0], font.size_pt));
    let (mut ascent, mut descent) = (primary_scaled.ascent(), primary_scaled.descent());
    let mut prev: Option<(usize, ab_glyph::GlyphId)> = None;
    for ch in text.chars() {
        let mut index = faces.iter().position(|face| face.glyph_id(ch).0 != 0);
        while index.is_none() {
            let Some((path, face_index)) = paths.next() else { break };
            if let Some(face) = load_face(path, *face_index) {
                if faces.iter().any(|f| Arc::ptr_eq(f, &face)) { continue; }
                let covers = face.glyph_id(ch).0 != 0;
                faces.push(face);
                if covers { index = Some(faces.len() - 1); }
            }
        }
        let index = index.unwrap_or(0);
        let face = &faces[index];
        let scale = scale_of(face, font.size_pt);
        let scaled = face.as_scaled(scale);
        let id = face.glyph_id(ch);
        if let Some((previous_face, previous)) = prev {
            if previous_face == index { width += scaled.kern(previous, id); }
        }
        ascent = ascent.max(scaled.ascent());
        descent = descent.min(scaled.descent());
        glyphs.push(PositionedGlyph { face: Arc::clone(face), id, scale, x: width });
        width += scaled.h_advance(id);
        prev = Some((index, id));
    }
    if font.style & 1 != 0 { width += 1.0; }
    Some(TextLayout { glyphs, width, ascent, descent })
}

/// Width and height in pixels. Without an installed font, an estimate is returned so layout still works.
pub fn measure_text(text: &str, font: &FontRef) -> (f32, f32) {
    match layout_text(text, font) {
        Some(layout) => (layout.width, layout.ascent - layout.descent),
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

fn put_image(pm: &mut Pixmap, x: i32, y: i32, w: u32, h: u32, rgba: &[u8], mut progress: impl FnMut(f32) -> bool) -> bool {
    let (pw, ph) = (pm.width() as i64, pm.height() as i64);
    if rgba.len() < w as usize * h as usize * 4 {
        return true;
    }
    let data = pm.data_mut();
    for j in 0..h as i64 {
        if j % 16 == 0 && !progress(j as f32 / h.max(1) as f32) { return false; }
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
    true
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
    let Some(layout) = layout_text(text, font) else { return };
    let (w, h) = (layout.width, layout.ascent - layout.descent);
    let (left, top) = anchor_origin(anchor, x, y, w, h);
    let base = top + layout.ascent;
    let strikes = if font.style & 1 != 0 { 2 } else { 1 };
    // fsItalic: lean the glyph pixels around the baseline, the way GDI synthesises an
    // italic face. The shift is split over the two columns it falls between so the
    // stroke keeps its weight instead of stair-stepping.
    let shear = if font.style & 2 != 0 { ITALIC_SHEAR } else { 0.0 };
    for glyph in &layout.glyphs {
        if let Some(o) = glyph.face.outline_glyph(glyph.id.with_scale_and_position(glyph.scale, point(left + glyph.x, base))) {
            let b = o.px_bounds();
            for s in 0..strikes {
                o.draw(|gx, gy, cov| {
                    let px = b.min.x as i32 + gx as i32 + s;
                    let py = b.min.y as i32 + gy as i32;
                    if shear == 0.0 {
                        blend(pm, px, py, color, cov);
                    } else {
                        let dx = (base - py as f32) * shear;
                        let whole = dx.floor();
                        let f = dx - whole;
                        blend(pm, px + whole as i32, py, color, cov * (1.0 - f));
                        blend(pm, px + whole as i32 + 1, py, color, cov * f);
                    }
                });
            }
        }
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
        hline(base - layout.ascent * 0.35);
    }
}

pub fn rasterize(scene: &Scene) -> Pixmap {
    rasterize_with_progress(scene, |_| true).expect("infallible rasterization")
}

pub fn rasterize_with_progress(scene: &Scene, mut progress: impl FnMut(f32) -> bool) -> Option<Pixmap> {
    let mut pm = Pixmap::new(scene.width.max(1), scene.height.max(1)).unwrap_or_else(|| Pixmap::new(1, 1).expect("1x1 pixmap"));
    pm.fill(Color::from_rgba8(scene.bg.0, scene.bg.1, scene.bg.2, 255));
    let id = Transform::identity();
    for (index, op) in scene.ops.iter().enumerate() {
        let base = index as f32 / scene.ops.len().max(1) as f32;
        if index % 64 == 0 && !progress(base) { return None; }
        match op {
            Op::Image { x, y, w, h, rgba } => {
                if !put_image(&mut pm, *x, *y, *w, *h, rgba, |p| progress(base + p / scene.ops.len().max(1) as f32)) { return None; }
            }
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
    if progress(1.0) { Some(pm) } else { None }
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
    fn rasterization_can_be_cancelled_without_returning_a_partial_image() {
        let s = scene((0..256).map(|i| Op::Ellipse {
            cx: i as f32, cy: 2.0, rx: 1.0, ry: 1.0,
            fill: Some(Rgb::BLACK), stroke: None,
        }).collect());
        let mut seen = Vec::new();
        let result = rasterize_with_progress(&s, |p| { seen.push(p); false });
        assert!(result.is_none());
        assert_eq!(seen, [0.0]);
    }

    #[test]
    fn large_image_copy_checks_cancellation_between_rows() {
        let rgba = vec![255; 20 * 100 * 4];
        let s = Scene { width: 20, height: 100, bg: Rgb::BLACK, ops: vec![Op::Image { x: 0, y: 0, w: 20, h: 100, rgba }] };
        let mut checks = 0;
        let result = rasterize_with_progress(&s, |_| { checks += 1; checks < 3 });
        assert!(result.is_none());
        assert_eq!(checks, 3);
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
    fn installed_font_family_matches_filenames() {
        use std::path::Path;
        assert!(matches_system_family("Noto Sans CJK JP", Path::new("NotoSansCJK-Regular.ttc")));
        assert!(matches_system_family("IPAexゴシック", Path::new("ipaexg.ttf")));
        assert!(matches_system_family("Arial", Path::new("Arial-Bold.ttf")));
        assert!(!matches_system_family("Arial", Path::new("NotoSansCJK-Regular.ttc")));
    }

    #[test]
    fn font_names_normalize() {
        assert_eq!(normalize("ＭＳ Ｐゴシック"), "mspゴシック");
        assert_eq!(normalize("MS PGothic"), "mspgothic");
        assert_eq!(candidates("ＭＳ Ｐ明朝")[0], ("msmincho.ttc", 1));
        assert_eq!(candidates("nonexistent")[0].0, "msgothic.ttc");
    }

    #[test]
    fn japanese_font_discovery_uses_specific_family_names() {
        assert!(is_japanese_font_file("notosanscjk-regular.ttc", "ttc"));
        assert!(is_japanese_font_file("ipaexgothic.ttf", "ttf"));
        assert!(is_japanese_font_file("yugothr.ttc", "ttc"));
        assert!(!is_japanese_font_file("noto-sans.ttf", "ttf"));
        assert!(!is_japanese_font_file("liberation-gothic.ttf", "ttf"));
        assert!(!is_japanese_font_file("YuGothR.ttc", "txt"));
    }

    #[test]
    fn installed_japanese_font_renders_glyphs() {
        let Some((path, index)) = candidate_paths("ＭＳ Ｐゴシック").into_iter().find(|(p, i)| load_face(p, *i).is_some()) else {
            // Minimal containers may have no Japanese font installed.
            return;
        };
        assert!(load_face(&path, index).is_some());
        let f = FontRef { family: "ＭＳ Ｐゴシック".into(), size_pt: 10.0, style: 0 };
        let pm = rasterize(&scene(vec![Op::Text { x: 10.0, y: 10.0, text: "日".into(), font: f, color: Rgb::BLACK, anchor: Anchor::Center }]));
        assert!(pm.data().chunks_exact(4).any(|px| px[..3] != [10, 20, 30]));
    }

    #[cfg(windows)]
    #[test]
    fn mixed_text_uses_japanese_fallback_for_missing_glyphs() {
        let arial = windows_font_dir().join("arial.ttf");
        let gothic = windows_font_dir().join("msgothic.ttc");
        let (Some(primary), Some(fallback)) = (load_face(&arial, 0), load_face(&gothic, 0)) else { return };
        assert_eq!(primary.glyph_id('日').0, 0);
        assert_ne!(fallback.glyph_id('日').0, 0);
        let font = FontRef { family: "Arial".into(), size_pt: 12.0, style: 0 };
        let layout = layout_text("A日B", &font).unwrap();
        assert!(Arc::ptr_eq(&layout.glyphs[0].face, &primary));
        assert!(Arc::ptr_eq(&layout.glyphs[1].face, &fallback));
        assert!(Arc::ptr_eq(&layout.glyphs[2].face, &primary));
        assert_ne!(layout.glyphs[1].id.0, 0);
        assert_eq!(measure_text("A日B", &font), (layout.width, layout.ascent - layout.descent));
    }

    /// fsItalic leans the glyph pixels to the right around the baseline (#18).
    #[test]
    fn italic_text_leans_right_above_the_baseline() {
        let upright = FontRef { family: "ＭＳ Ｐゴシック".into(), size_pt: 24.0, style: 0 };
        if layout_text("H", &upright).is_none() {
            return; // no Japanese font on this machine
        }
        let italic = FontRef { style: 2, ..upright.clone() };
        let draw = |font: FontRef| {
            rasterize(&Scene {
                width: 80,
                height: 80,
                bg: Rgb(255, 255, 255),
                ops: vec![Op::Text { x: 10.0, y: 10.0, text: "H".into(), font, color: Rgb::BLACK, anchor: Anchor::TopLeft }],
            })
        };
        let (a, b) = (draw(upright), draw(italic));
        let ink_rows = |pm: &Pixmap| (0..pm.height()).filter(|y| (0..pm.width()).any(|x| px(pm, x, *y).0 < 200)).collect::<Vec<_>>();
        let left_of = |pm: &Pixmap, y: u32| (0..pm.width()).find(|x| px(pm, *x, y).0 < 200).unwrap();
        let (ra, rb) = (ink_rows(&a), ink_rows(&b));
        assert!(!ra.is_empty() && !rb.is_empty(), "both faces must draw ink");
        assert_eq!(ra.len(), rb.len(), "the shear must not change the height");
        // the top of the italic stem sits further right, the baseline barely moves
        assert!(left_of(&b, rb[0]) > left_of(&a, ra[0]), "italic must lean right at the top");
        let (top, bottom) = (left_of(&b, rb[0]) as i32 - left_of(&a, ra[0]) as i32, left_of(&b, *rb.last().unwrap()) as i32 - left_of(&a, *ra.last().unwrap()) as i32);
        assert!(top > bottom, "the lean must shrink towards the baseline: top={top} bottom={bottom}");
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
