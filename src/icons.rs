//! Toolbar/menu icons: Phosphor for common actions, generated custom art for plot modes.
use crate::vcl::form::Assets;
use crate::vcl::*;
use egui::{Color32, Painter, Pos2, Rect, TextureHandle, Vec2};

/// Phosphor regular icons used for common file and view actions.
/// Phosphor is MIT licensed. Font/codepoint source: https://github.com/amPerl/egui-phosphor
/// and https://github.com/phosphor-icons/core. See the package license in Cargo.toml.
fn standard_icon(idx: usize) -> Option<&'static str> {
    use egui_phosphor::regular as ph;
    Some(match idx {
        0 => ph::FOLDER_OPEN,
        1 => ph::FLOPPY_DISK,
        2 => ph::COPY,
        3 => ph::CHART_LINE_UP,
        14 => ph::FILE_TEXT,
        4 => ph::GEAR,
        5 => ph::QUESTION,
        _ => return None,
    })
}

fn draw_standard(p: &Painter, idx: usize, r: Rect, enabled: bool) {
    let Some(glyph) = standard_icon(idx) else { return };
    let color = if !enabled { SHADOW } else { match idx {
        0 => Color32::from_rgb(225, 174, 24), // warm yellow folder
        1 => Color32::from_rgb(48, 104, 190), // blue save disk
        2 => Color32::from_rgb(164, 117, 70), // paper/clipboard brown
        3 => Color32::from_rgb(35, 110, 190), // redraw the graph
        14 => Color32::from_rgb(28, 130, 95), // reload the data file
        4 => Color32::from_rgb(89, 114, 146),
        5 => Color32::from_rgb(45, 126, 205), // blue help mark
        _ => BLACK,
    }};
    let font = egui::FontId::new(17.0, egui::FontFamily::Name("phosphor".into()));
    if matches!(idx, 3 | 14) {
        // Keep the target visible while the small circular arrow signals repetition.
        p.text(r.center() - Vec2::new(2.0, 2.0), egui::Align2::CENTER_CENTER, glyph, font, color);
        let badge = r.center() + Vec2::new(5.0, 5.0);
        p.circle_filled(badge, 5.5, Color32::from_rgb(248, 248, 248));
        p.text(badge, egui::Align2::CENTER_CENTER, egui_phosphor::regular::ARROW_CLOCKWISE,
            egui::FontId::new(11.0, egui::FontFamily::Name("phosphor".into())), color);
    } else {
        p.text(r.center(), egui::Align2::CENTER_CENTER, glyph, font, color);
    }
}

/// Fallback glyphs per ImageList index: (text, background).
const GLYPHS: [(&str, (u8, u8, u8)); 20] = [
    ("L", (230, 190, 60)), ("S", (60, 90, 200)), ("C", (190, 160, 110)), ("R", (60, 160, 60)), ("*", (120, 120, 120)),
    ("?", (40, 140, 220)), ("D", (220, 80, 60)), ("P", (200, 120, 200)), ("T", (60, 170, 170)), ("S", (220, 140, 40)),
    ("V", (90, 90, 220)), ("A", (40, 40, 40)), ("G", (150, 150, 150)), ("-", (200, 200, 200)), ("N", (60, 160, 60)),
    (".", (220, 60, 60)), ("F", (120, 190, 120)), ("B", (150, 120, 90)), ("X", (80, 120, 200)), ("O", (200, 120, 80)),
];

pub fn draw(p: &Painter, sheet: Option<&TextureHandle>, idx: usize, r: Rect, enabled: bool) {
    if standard_icon(idx).is_some() {
        draw_standard(p, idx, r, enabled);
        return;
    }
    let tint = if enabled { Color32::WHITE } else { Color32::from_gray(150) };
    if let Some(t) = sheet {
        let (cx, cy) = ((idx % 4) as f32, (idx / 4) as f32);
        let uv = Rect::from_min_max(Pos2::new(cx * 20.0 / 80.0, cy * 20.0 / 120.0), Pos2::new((cx + 1.0) * 20.0 / 80.0, (cy + 1.0) * 20.0 / 120.0));
        p.image(t.id(), r, uv, tint);
        return;
    }
    let (s, (rr, g, b)) = GLYPHS[idx.min(19)];
    let bg = if enabled { Color32::from_rgb(rr, g, b) } else { SHADOW };
    let ir = Rect::from_center_size(r.center(), Vec2::splat(16.0));
    fill(p, ir, bg);
    rect_outline(p, ir, DKSHADOW);
    let fg = if enabled { Color32::WHITE } else { HILITE };
    p.text(ir.center(), egui::Align2::CENTER_CENTER, s, crate::fonts::font("MS Gothic", 12.0), fg);
}

fn cell_uv(idx: usize) -> Rect {
    let (cx, cy) = ((idx % 4) as f32, (idx / 4) as f32);
    Rect::from_min_max(Pos2::new(cx * 20.0 / 80.0, cy * 20.0 / 120.0), Pos2::new((cx + 1.0) * 20.0 / 80.0, (cy + 1.0) * 20.0 / 120.0))
}

/// Build the "checked" (two checker phases) and "disabled" variants of the 20x20-cell icon sheet.
/// A checked button shows the icon with every other pixel replaced by the highlight blue.
pub fn bake_variants(img: &image::RgbaImage) -> ([egui::ColorImage; 2], egui::ColorImage) {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let get = |x: usize, y: usize| img.get_pixel(x as u32, y as u32).0;
    let is_white = |c: [u8; 4]| c[3] > 0 && c[0] == 255 && c[1] == 255 && c[2] == 255;
    let is_ink = |c: [u8; 4]| c[3] > 0 && !is_white(c);
    let blue = Color32::from_rgb(0, 120, 212);
    let mut chk = [vec![Color32::TRANSPARENT; w * h], vec![Color32::TRANSPARENT; w * h]];
    let mut dis = vec![Color32::TRANSPARENT; w * h];
    for y in 0..h {
        for x in 0..w {
            let c = get(x, y);
            let plain = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
            for ph in 0..2 {
                chk[ph][y * w + x] = if c[3] > 0 && (x % 20 + y % 20 + ph) % 2 == 0 { blue } else { plain };
            }
            if is_ink(c) {
                dis[y * w + x] = SHADOW;
            }
        }
    }
    // white emboss shadow one pixel down-right, inside the same cell, under the gray glyph
    for y in 0..h {
        for x in 0..w {
            if is_ink(get(x, y)) && lx_ok(x) && lx_ok(y) {
                let i = (y + 1) * w + x + 1;
                if dis[i] == Color32::TRANSPARENT {
                    dis[i] = HILITE;
                }
            }
        }
    }
    let [c0, c1] = chk;
    ([egui::ColorImage::new([w, h], c0), egui::ColorImage::new([w, h], c1)], egui::ColorImage::new([w, h], dis))
}

fn lx_ok(v: usize) -> bool {
    v % 20 != 19
}

/// Simple coordinate symbols stay legible at toolbar size.
fn draw_coordinate_icon(p: &Painter, idx: usize, r: Rect, enabled: bool) -> bool {
    if !matches!(idx, 11 | 12 | 18 | 19) { return false; }
    let ink = if enabled { Color32::from_rgb(30, 45, 65) } else { SHADOW };
    let blue = if enabled { Color32::from_rgb(32, 96, 190) } else { SHADOW };
    let pt = |x: f32, y: f32| Pos2::new(r.min.x + x * r.width() / 20.0, r.min.y + y * r.height() / 20.0);
    let line = |a: (f32, f32), b: (f32, f32), color| { p.line_segment([pt(a.0, a.1), pt(b.0, b.1)], egui::Stroke::new(1.5, color)); };
    match idx {
        11 => {
            line((4.0, 16.0), (16.0, 16.0), ink);
            line((4.0, 16.0), (4.0, 4.0), ink);
            line((16.0, 16.0), (13.0, 13.0), ink);
            line((4.0, 4.0), (7.0, 7.0), ink);
        }
        12 => {
            for t in [4.0, 8.0, 12.0, 16.0] {
                line((4.0, t), (16.0, t), ink);
                line((t, 4.0), (t, 16.0), ink);
            }
        }
        18 => {
            p.text(r.center(), egui::Align2::CENTER_CENTER, "XY", egui::FontId::proportional(13.0), blue);
        }
        19 => {
            // Cylinder outline and central axis distinguish it from Cartesian XY.
            line((5.0, 6.0), (5.0, 14.0), blue);
            line((15.0, 6.0), (15.0, 14.0), blue);
            for (a, b) in [((5.0, 6.0), (8.0, 4.0)), ((8.0, 4.0), (12.0, 4.0)), ((12.0, 4.0), (15.0, 6.0)), ((5.0, 6.0), (8.0, 8.0)), ((8.0, 8.0), (12.0, 8.0)), ((12.0, 8.0), (15.0, 6.0)), ((5.0, 14.0), (8.0, 16.0)), ((8.0, 16.0), (12.0, 16.0)), ((12.0, 16.0), (15.0, 14.0))] { line(a, b, blue); }
            line((10.0, 17.0), (10.0, 3.0), ink);
        }
        _ => unreachable!(),
    }
    true
}

/// Toolbar glyph in its button state (normal, pressed/checked, disabled).
pub fn draw_tb(p: &Painter, a: &Assets, idx: usize, r: Rect, enabled: bool, checked: bool) {
    if draw_coordinate_icon(p, idx, r, enabled) { return; }
    if standard_icon(idx).is_some() {
        draw_standard(p, idx, r, enabled);
        return;
    }
    if draw_custom(p, a.plot_icons.as_ref(), idx, r, enabled) { return; }
    // the pattern is anchored to the VCL client origin, which sits one pixel row (odd) above ours
    let odd = ((r.min.x + r.min.y).round() as i64).rem_euclid(2) == 0;
    let variant = if !enabled { a.imagelist_dis.as_ref() } else if checked { if odd { a.imagelist_chk1.as_ref() } else { a.imagelist_chk.as_ref() } } else { a.imagelist.as_ref() };
    match (variant, a.imagelist.as_ref()) {
        (Some(t), Some(_)) => {
            p.image(t.id(), r, cell_uv(idx), Color32::WHITE);
        }
        _ => draw(p, None, idx, r, enabled),
    }
}

/// Draw a generated custom icon from the 4x3, 20px-cell sheet.
/// Indices follow the application's ImageList positions. Returns true when mapped.
pub fn draw_custom(p: &Painter, sheet: Option<&TextureHandle>, idx: usize, r: Rect, enabled: bool) -> bool {
    let cell = match idx {
        6 => 0, 7 => 1, 8 => 2, 9 => 3, 10 => 4, 11 => 5, 12 => 6,
        15 => 7, 16 => 8, 17 => 9, 19 => 11,
        _ => return false,
    };
    let Some(t) = sheet else { return false };
    let uv = Rect::from_min_max(
        Pos2::new((cell % 4) as f32 / 4.0, (cell / 4) as f32 / 3.0),
        Pos2::new((cell % 4 + 1) as f32 / 4.0, (cell / 4 + 1) as f32 / 3.0),
    );
    let tint = if enabled { Color32::WHITE } else { Color32::from_gray(150) };
    p.image(t.id(), r, uv, tint);
    true
}

pub fn draw_fallback_logo(p: &Painter, r: Rect) {
    fill(p, r, Color32::from_rgb(40, 90, 160));
    rect_outline(p, r, BLACK);
    p.text(r.center(), egui::Align2::CENTER_CENTER, "AV", crate::fonts::font("MS Gothic", 24.0), Color32::WHITE);
}
