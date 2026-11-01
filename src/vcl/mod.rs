//! VCL-style (Windows classic) drawing primitives.
use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, Vec2};

pub mod form;
pub mod tooltip;

pub const FACE: Color32 = Color32::from_rgb(240, 240, 240);
pub const SHADOW: Color32 = Color32::from_rgb(160, 160, 160);
pub const DKSHADOW: Color32 = Color32::from_rgb(105, 105, 105);
pub const HILITE: Color32 = Color32::WHITE;
pub const LIGHT: Color32 = Color32::from_rgb(227, 227, 227);
pub const BLACK: Color32 = Color32::BLACK;
pub const SELECT: Color32 = Color32::from_rgb(0, 120, 212);
pub const GRAYTEXT: Color32 = Color32::from_rgb(109, 109, 109);

/// TColor (BGR int) to Color32.
pub fn col(c: u32) -> Color32 {
    Color32::from_rgb((c & 0xff) as u8, ((c >> 8) & 0xff) as u8, ((c >> 16) & 0xff) as u8)
}

pub fn from_col(c: Color32) -> u32 {
    c.r() as u32 | (c.g() as u32) << 8 | (c.b() as u32) << 16
}

pub fn px(r: Rect) -> Rect {
    Rect::from_min_max(r.min.round(), r.max.round())
}

pub fn fill(p: &Painter, r: Rect, c: Color32) {
    p.rect_filled(r, egui::CornerRadius::ZERO, c);
}

/// 1px line as a filled rect (crisp with feathering off).
pub fn hline(p: &Painter, x0: f32, x1: f32, y: f32, c: Color32) {
    fill(p, Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, y + 1.0)), c);
}
pub fn vline(p: &Painter, x: f32, y0: f32, y1: f32, c: Color32) {
    fill(p, Rect::from_min_max(Pos2::new(x, y0), Pos2::new(x + 1.0, y1)), c);
}

/// One-pixel frame: top/left `tl`, bottom/right `br`.
pub fn frame1(p: &Painter, r: Rect, tl: Color32, br: Color32) {
    hline(p, r.min.x, r.max.x, r.min.y, tl);
    vline(p, r.min.x, r.min.y, r.max.y, tl);
    hline(p, r.min.x, r.max.x, r.max.y - 1.0, br);
    vline(p, r.max.x - 1.0, r.min.y, r.max.y, br);
}

pub fn rect_outline(p: &Painter, r: Rect, c: Color32) {
    frame1(p, r, c, c);
}

/// Two-pixel frame (outer then inner).
pub fn frame2(p: &Painter, r: Rect, otl: Color32, obr: Color32, itl: Color32, ibr: Color32) {
    frame1(p, r, otl, obr);
    frame1(p, r.shrink(1.0), itl, ibr);
}

pub fn raised(p: &Painter, r: Rect) {
    frame2(p, r, LIGHT, DKSHADOW, HILITE, SHADOW);
}
pub fn sunken(p: &Painter, r: Rect) {
    frame2(p, r, SHADOW, HILITE, DKSHADOW, LIGHT);
}
pub fn raised1(p: &Painter, r: Rect) {
    frame1(p, r, HILITE, SHADOW);
}
pub fn sunken1(p: &Painter, r: Rect) {
    frame1(p, r, SHADOW, HILITE);
}
/// Etched frame (group box): shadow line with a highlight line offset by one.
pub fn etched(p: &Painter, r: Rect) {
    let s = Rect::from_min_max(r.min, r.max - Vec2::new(1.0, 1.0));
    rect_outline(p, s.translate(Vec2::new(1.0, 1.0)), HILITE);
    rect_outline(p, s, SHADOW);
}

/// Dotted focus rectangle.
pub fn focus_rect(p: &Painter, r: Rect) {
    // GDI dotted rectangle: every other pixel, phase tied to the screen position ((x + y) even)
    let dot = |x: f32, y: f32| {
        if (x as i32 + y as i32) % 2 == 0 {
            fill(p, Rect::from_min_size(Pos2::new(x, y), Vec2::new(1.0, 1.0)), BLACK);
        }
    };
    let mut x = r.min.x;
    while x < r.max.x {
        dot(x, r.min.y);
        dot(x, r.max.y - 1.0);
        x += 1.0;
    }
    let mut y = r.min.y + 1.0;
    while y < r.max.y - 1.0 {
        dot(r.min.x, y);
        dot(r.max.x - 1.0, y);
        y += 1.0;
    }
}

/// Split `&x` accelerator markup: (display text, char index of accelerator, accelerator char).
pub fn parse_caption(s: &str) -> (String, Option<usize>, Option<char>) {
    let mut out = String::new();
    let mut idx = None;
    let mut ch = None;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '&' {
            match it.peek() {
                Some('&') => {
                    out.push('&');
                    it.next();
                }
                Some(&n) => {
                    if idx.is_none() {
                        idx = Some(out.chars().count());
                        ch = Some(n);
                    }
                }
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    (out, idx, ch)
}

#[derive(Clone, Copy, PartialEq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

/// Draw caption text inside `r`; `underline` shows the accelerator underline.
pub fn caption(p: &Painter, r: Rect, text: &str, font: FontId, color: Color32, h: HAlign, vcenter: bool, underline: bool) -> Rect {
    let (s, idx, _) = parse_caption(text);
    let galley = p.layout_no_wrap(s.clone(), font.clone(), color);
    let size = galley.size();
    let x = match h {
        HAlign::Left => r.min.x,
        HAlign::Center => r.min.x + ((r.width() - size.x) / 2.0).floor(),
        HAlign::Right => r.max.x - size.x,
    };
    let y = if vcenter { r.min.y + ((r.height() - size.y) / 2.0).floor() } else { r.min.y };
    let pos = Pos2::new(x, y);
    p.galley(pos, galley, color);
    if let (true, Some(i)) = (underline, idx) {
        let pre: String = s.chars().take(i).collect();
        let one: String = s.chars().skip(i).take(1).collect();
        let x0 = x + p.layout_no_wrap(pre, font.clone(), color).size().x;
        let w = p.layout_no_wrap(one, font, color).size().x;
        hline(p, x0, x0 + w, y + size.y - 1.0, color);
    }
    Rect::from_min_size(pos, size)
}

pub fn text_size(p: &Painter, s: &str, font: &FontId) -> Vec2 {
    p.layout_no_wrap(parse_caption(s).0, font.clone(), BLACK).size()
}

pub fn arrow(p: &Painter, center: Pos2, dir: char, c: Color32) {
    // 7x4 style triangles built from rows of 1px rects
    let (rows, horiz) = match dir {
        'u' | 'd' => (4, false),
        _ => (4, true),
    };
    for i in 0..rows {
        let half = match dir {
            'u' => i,
            'd' => rows - 1 - i,
            'l' => i,
            _ => rows - 1 - i,
        };
        if !horiz {
            let y = center.y - 2.0 + i as f32;
            let x0 = center.x - half as f32;
            fill(p, Rect::from_min_size(Pos2::new(x0, y), Vec2::new(half as f32 * 2.0 + 1.0, 1.0)), c);
        } else {
            let x = center.x - 2.0 + i as f32;
            let y0 = center.y - half as f32;
            fill(p, Rect::from_min_size(Pos2::new(x, y0), Vec2::new(1.0, half as f32 * 2.0 + 1.0)), c);
        }
    }
}

pub fn stroke(c: Color32) -> Stroke {
    Stroke::new(1.0, c)
}
