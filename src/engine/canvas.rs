use ab_glyph::{Font, FontRef, Glyph, Point, ScaleFont};
use tiny_skia::{Color, Pixmap, PremultipliedColorU8};

#[derive(Clone, Debug)]
pub struct FontSpec {
    pub name: String,
    pub size: f32,
}

pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub pixmap: Pixmap,
}

impl Canvas {
    pub fn new(w: u32, h: u32, bg_rgb: u32) -> Self {
        let mut pixmap = Pixmap::new(w, h).expect("pixmap");
        let (r, g, b) = split_rgb(bg_rgb);
        pixmap.fill(Color::from_rgba8(r, g, b, 255));
        Self { width: w, height: h, pixmap }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32) {
        let (r, g, b) = split_rgb(rgb);
        for yy in y..y + h {
            for xx in x..x + w {
                self.set_pixel(xx, yy, rgb, 255);
            }
        }
        let _ = (r, g, b);
    }

    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, rgb: u32, width: f32) {
        // Bresenham, no AA
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx - dy;
        let mut x = x0;
        let mut y = y0;
        loop {
            self.set_pixel(x, y, rgb, 255);
            if x == x1 && y == y1 { break; }
            let e2 = 2 * err;
            if e2 > -dy { err -= dy; x += sx; }
            if e2 < dx { err += dx; y += sy; }
        }
        let _ = width;
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, rgb: u32, a: u8) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height { return; }
        let (r, g, b) = split_rgb(rgb);
        let p = PremultipliedColorU8::from_rgba(r, g, b, a).unwrap_or(PremultipliedColorU8::TRANSPARENT);
        // tiny_skia Pixmap pixels are premul; use set_pixel for direct
        self.pixmap.pixels_mut()[(y as u32 * self.width + x as u32) as usize] = p;
    }

    pub fn get_pixel(&self, x: i32, y: i32) -> Option<(u8, u8, u8, u8)> {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height { return None; }
        let p = self.pixmap.pixels()[(y as u32 * self.width + x as u32) as usize];
        Some((p.red(), p.green(), p.blue(), p.alpha()))
    }

    pub fn to_rgba_bytes(&self) -> Vec<u8> {
        self.pixmap.data().to_vec()
    }

    pub fn text(&mut self, x: i32, y: i32, s: &str, spec: &FontSpec, rgb: u32) {
        let (r, g, b) = split_rgb(rgb);
        // find font file
        let candidates: Vec<&str> = match spec.name.as_str() {
            "MS Gothic" | "MS UI Gothic" | "MS PGothic" => vec!["C:\\Windows\\Fonts\\msgothic.ttc"],
            "MS Mincho" | "MS PMincho" => vec!["C:\\Windows\\Fonts\\msmincho.ttc"],
            _ => vec!["C:\\Windows\\Fonts\\msgothic.ttc", "C:\\Windows\\Fonts\\msmincho.ttc"],
        };
        let mut font_bytes: Option<Vec<u8>> = None;
        let mut index: u32 = 0;
        for c in &candidates {
            if let Ok(data) = std::fs::read(c) {
                font_bytes = Some(data);
                // index selection for msgothic: 0 Gothic,1 UI Gothic,2 PGothic
                if spec.name == "MS UI Gothic" { index = 1; }
                if spec.name == "MS PGothic" { index = 2; }
                break;
            }
        }
        // fallbacks
        if font_bytes.is_none() {
            for c in ["C:\\Windows\\Fonts\\meiryo.ttc", "C:\\Windows\\Fonts\\meiryob.ttc"] {
                if let Ok(data) = std::fs::read(c) { font_bytes = Some(data); break; }
            }
        }
        let Some(data) = font_bytes else { return; };
        let Ok(font) = FontRef::try_from_slice_and_index(&data, index) else { return; };
        let scale = ab_glyph::PxScale::from(spec.size);
        let font = font as FontRef;
        let scaled = font.as_scaled(scale);
        let mut cursor_x = x as f32;
        let baseline = y as f32 + spec.size;
        for ch in s.chars() {
            let glyph_id = scaled.glyph_id(ch);
            let mut glyph = glyph_id.with_scale(scale);
            glyph.position = Point { x: cursor_x, y: baseline };
            if let Some(outlined) = font.outline_glyph(glyph.clone()) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, cov| {
                    let px = (bounds.min.x as i32 + gx as i32);
                    let py = (bounds.min.y as i32 + gy as i32);
                    if px < 0 || py < 0 || px as u32 >= self.width || py as u32 >= self.height { return; }
                    let alpha = (cov * 255.0) as u8;
                    // alpha blend over existing
                    let idx = (py as u32 * self.width + px as u32) as usize;
                    let dst = self.pixmap.pixels()[idx];
                    let a = alpha as f32 / 255.0;
                    let nr = (r as f32 * a + dst.red() as f32 * (1.0 - a)) as u8;
                    let ng = (g as f32 * a + dst.green() as f32 * (1.0 - a)) as u8;
                    let nb = (b as f32 * a + dst.blue() as f32 * (1.0 - a)) as u8;
                    let na = (dst.alpha() as f32 + (255 - dst.alpha() as i32) as f32 * a) as u8;
                    let premul = PremultipliedColorU8::from_rgba(nr, ng, nb, na).unwrap_or(PremultipliedColorU8::TRANSPARENT);
                    self.pixmap.pixels_mut()[idx] = premul;
                });
            }
            cursor_x += scaled.h_advance(glyph_id);
        }
    }
}

fn split_rgb(v: u32) -> (u8, u8, u8) {
    (((v >> 16) & 0xff) as u8, ((v >> 8) & 0xff) as u8, (v & 0xff) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bg() {
        let c = Canvas::new(4, 4, 0x123456);
        let p = c.get_pixel(0, 0).unwrap();
        assert_eq!((p.0, p.1, p.2), (0x12, 0x34, 0x56));
    }
    #[test]
    fn fill_rect_pixel() {
        let mut c = Canvas::new(8, 8, 0xffffff);
        c.fill_rect(1, 1, 2, 2, 0xff0000);
        assert_eq!(c.get_pixel(1, 1).unwrap().0, 0xff);
        assert_eq!(c.get_pixel(0, 0).unwrap().0, 0xff); // white bg red channel
    }
    #[test]
    fn horiz_vert_line() {
        let mut c = Canvas::new(8, 8, 0xffffff);
        c.line(0, 2, 7, 2, 0x0000ff, 1.0);
        c.line(3, 0, 3, 7, 0x0000ff, 1.0);
        assert_eq!(c.get_pixel(3, 2).unwrap().2, 0xff);
        assert_eq!(c.get_pixel(0, 2).unwrap().2, 0xff);
    }
    #[test]
    fn out_of_range_no_panic() {
        let mut c = Canvas::new(4, 4, 0x000000);
        c.fill_rect(-10, -10, 100, 100, 0xff0000);
        c.line(-5, -5, 100, 100, 0x00ff00, 1.0);
        c.set_pixel(100, 100, 0xffffff, 255);
        let _ = c.get_pixel(100, 100);
        c.text(-5, -5, "abc", &FontSpec { name: "MS Gothic".into(), size: 12.0 }, 0x000000);
    }
    #[test]
    fn text_no_font_no_panic() {
        let mut c = Canvas::new(16, 16, 0xffffff);
        c.text(0, 0, "test", &FontSpec { name: "Nonexistent".into(), size: 12.0 }, 0x000000);
    }
}
