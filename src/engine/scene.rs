//! Intermediate drawing representation shared by the rasterizer and the metafile writers.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const WHITE: Rgb = Rgb(255, 255, 255);
    pub const BLACK: Rgb = Rgb(0, 0, 0);

    /// Delphi TColor (0x00BBGGRR).
    pub fn from_tcolor(c: u32) -> Rgb {
        Rgb((c & 0xff) as u8, ((c >> 8) & 0xff) as u8, ((c >> 16) & 0xff) as u8)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FontRef {
    pub family: String,
    pub size_pt: f32,
    /// Delphi TFontStyles bit set: 1 bold, 2 italic, 4 underline, 8 strikeout.
    pub style: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    Center,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Transparent pixels are not drawn.
    Image { x: i32, y: i32, w: u32, h: u32, rgba: Vec<u8> },
    Polyline { pts: Vec<(f32, f32)>, color: Rgb, width: f32 },
    Polygon { pts: Vec<(f32, f32)>, fill: Option<Rgb>, stroke: Option<Rgb> },
    Ellipse { cx: f32, cy: f32, rx: f32, ry: f32, fill: Option<Rgb>, stroke: Option<Rgb> },
    Text { x: f32, y: f32, text: String, font: FontRef, color: Rgb, anchor: Anchor },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub width: u32,
    pub height: u32,
    pub bg: Rgb,
    pub ops: Vec<Op>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tcolor_is_bgr() {
        assert_eq!(Rgb::from_tcolor(0x0033_2211), Rgb(0x11, 0x22, 0x33));
        assert_eq!(Rgb::from_tcolor(0xff_ffff), Rgb::WHITE);
    }
}
