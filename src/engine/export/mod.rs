//! Output formats: BMP (8 bit, optionally RLE), PNG, EMF, WMF and AVI.
pub mod avi;
pub mod bmp;
pub mod emf;
pub mod wmf;

use super::pipeline::Rendered;
use super::scene::FontRef;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Bmp,
    RleBmp,
    Png,
    Emf,
    Wmf,
}

impl ImageFormat {
    /// `.bmp` gives the plain BMP; the RLE variant has to be asked for explicitly.
    pub fn from_extension(path: &Path) -> Option<ImageFormat> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "bmp" => Some(ImageFormat::Bmp),
            "png" => Some(ImageFormat::Png),
            "emf" => Some(ImageFormat::Emf),
            "wmf" => Some(ImageFormat::Wmf),
            _ => None,
        }
    }
}

pub fn save_image(path: &Path, r: &Rendered, fmt: ImageFormat) -> Result<(), String> {
    let (w, h) = (r.pixmap.width(), r.pixmap.height());
    let bytes = match fmt {
        ImageFormat::Bmp => bmp::encode_bmp8(&bmp::quantize(r.pixmap.data(), w, h)),
        ImageFormat::RleBmp => bmp::encode_rle8(&bmp::quantize(r.pixmap.data(), w, h)),
        ImageFormat::Png => encode_png(r.pixmap.data(), w, h)?,
        ImageFormat::Emf => emf::encode_emf(&r.scene),
        ImageFormat::Wmf => wmf::encode_wmf(&r.scene)?,
    };
    write_atomic(path, &bytes)
}

fn encode_png(rgba: &[u8], w: u32, h: u32) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

pub(crate) fn temp_sibling(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

/// Writes to a temporary sibling and renames it over `path`, so a failure keeps the old file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = temp_sibling(path);
    let r = std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, path));
    r.map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", path.display())
    })
}

pub(crate) fn push_u16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
pub(crate) fn push_u32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}
pub(crate) fn push_i32(v: &mut Vec<u8>, x: i32) {
    v.extend_from_slice(&x.to_le_bytes());
}

/// BITMAPINFOHEADER for a bottom-up bitmap.
pub(crate) fn info_header(w: u32, h: u32, bit_count: u16, compression: u32, size_image: u32, clr_used: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity(40);
    push_u32(&mut v, 40);
    push_i32(&mut v, w as i32);
    push_i32(&mut v, h as i32);
    push_u16(&mut v, 1);
    push_u16(&mut v, bit_count);
    push_u32(&mut v, compression);
    push_u32(&mut v, size_image);
    push_i32(&mut v, 3780);
    push_i32(&mut v, 3780);
    push_u32(&mut v, clr_used);
    push_u32(&mut v, 0);
    v
}

/// Round to an integer coordinate; None when the value cannot be placed.
pub(crate) fn coord(v: f32) -> Option<i32> {
    (v.is_finite() && v.abs() < 1e8).then(|| v.round() as i32)
}

/// Font height in pixels at 96 dpi, as the rasterizer sizes it.
pub(crate) fn font_px(f: &FontRef) -> i32 {
    (f.size_pt.clamp(1.0, 500.0) * 96.0 / 72.0).round().max(1.0) as i32
}

/// Lowest-free-slot handle table, as GDI hands out object indices.
pub(crate) struct Table {
    used: Vec<bool>,
    /// highest index ever used + 1
    pub count: usize,
}

impl Table {
    /// `first` is the lowest index that may be handed out (EMF reserves 0).
    pub fn new(first: usize) -> Table {
        Table { used: (0..first).map(|_| true).collect(), count: first }
    }
    pub fn alloc(&mut self) -> usize {
        let i = self.used.iter().position(|u| !u).unwrap_or(self.used.len());
        if i == self.used.len() {
            self.used.push(true);
        } else {
            self.used[i] = true;
        }
        self.count = self.count.max(i + 1);
        i
    }
    pub fn free(&mut self, i: usize) {
        if let Some(u) = self.used.get_mut(i) {
            *u = false;
        }
    }
}

/// A rectangle of an image whose pixels are all drawn, as 24 bit bottom-up DIB bits.
pub(crate) struct Block {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub bits: Vec<u8>,
}

/// Splits an RGBA image into opaque blocks (alpha 0 is a hole), clipped to the canvas.
/// Row runs with the same extent in consecutive rows are merged into one block.
pub(crate) fn opaque_blocks(x: i32, y: i32, w: u32, h: u32, rgba: &[u8], canvas: (u32, u32)) -> Vec<Block> {
    let mut out = Vec::new();
    if w == 0 || h == 0 || rgba.len() < w as usize * h as usize * 4 {
        return out;
    }
    let (w, h) = (w as i64, h as i64);
    let (x, y) = (x as i64, y as i64);
    let cx0 = (-x).max(0);
    let cx1 = w.min(canvas.0 as i64 - x);
    let cy0 = (-y).max(0);
    let cy1 = h.min(canvas.1 as i64 - y);
    // (x0, x1, start row) of runs continuing from the previous row
    let mut active: Vec<(i64, i64, i64)> = Vec::new();
    let flush = |out: &mut Vec<Block>, r: (i64, i64, i64), end: i64| {
        let (bw, bh) = ((r.1 - r.0) as usize, (end - r.2) as usize);
        let stride = (bw * 3 + 3) & !3;
        let mut bits = vec![0u8; stride * bh];
        for j in 0..bh {
            let src_row = r.2 as usize + j;
            let dst_row = bh - 1 - j;
            for i in 0..bw {
                let s = (src_row * w as usize + r.0 as usize + i) * 4;
                let d = dst_row * stride + i * 3;
                bits[d] = rgba[s + 2];
                bits[d + 1] = rgba[s + 1];
                bits[d + 2] = rgba[s];
            }
        }
        out.push(Block { x: (x + r.0) as i32, y: (y + r.2) as i32, w: bw as u32, h: bh as u32, bits });
    };
    for j in cy0..cy1.max(cy0) {
        let mut runs: Vec<(i64, i64)> = Vec::new();
        let mut i = cx0;
        while i < cx1 {
            if rgba[((j * w + i) * 4 + 3) as usize] == 0 {
                i += 1;
                continue;
            }
            let s = i;
            while i < cx1 && rgba[((j * w + i) * 4 + 3) as usize] != 0 {
                i += 1;
            }
            runs.push((s, i));
        }
        let mut next = Vec::with_capacity(runs.len());
        for &(a, b) in &runs {
            match active.iter().position(|r| r.0 == a && r.1 == b) {
                Some(k) => next.push(active.swap_remove(k)),
                None => next.push((a, b, j)),
            }
        }
        for r in active.drain(..) {
            flush(&mut out, r, j);
        }
        active = next;
    }
    for r in active {
        flush(&mut out, r, cy1.max(cy0));
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::engine::scene::{Anchor, Op, Rgb, Scene};

    /// Every op kind, Japanese text, an image with holes, a long polyline and huge coordinates.
    pub fn test_scene() -> Scene {
        let font = FontRef { family: "MS ゴシック".into(), size_pt: 10.0, style: 1 | 4 };
        let mut rgba = vec![0u8; 12 * 8 * 4];
        for j in 0..8 {
            for i in 0..12 {
                if (i + j) % 5 != 0 && !(3..6).contains(&j) {
                    let s = (j * 12 + i) * 4;
                    rgba[s..s + 4].copy_from_slice(&[200, (i * 20) as u8, 40, 255]);
                }
            }
        }
        let long: Vec<(f32, f32)> = (0..20000).map(|i| ((i % 300) as f32, (i / 70) as f32)).collect();
        Scene {
            width: 300,
            height: 200,
            bg: Rgb(250, 250, 240),
            ops: vec![
                Op::Image { x: 10, y: 10, w: 12, h: 8, rgba },
                Op::Polyline { pts: long, color: Rgb(0, 0, 255), width: 1.0 },
                Op::Polyline { pts: vec![(0.0, 0.0), (100000.0, 50.0), (20.0, 90.0)], color: Rgb(9, 9, 9), width: 3.0 },
                Op::Polygon { pts: vec![(50.0, 50.0), (90.0, 50.0), (70.0, 90.0)], fill: Some(Rgb(255, 200, 0)), stroke: Some(Rgb::BLACK) },
                Op::Polygon { pts: vec![(120.0, 50.0), (150.0, 50.0), (130.0, 90.0)], fill: None, stroke: Some(Rgb(1, 2, 3)) },
                Op::Ellipse { cx: 200.0, cy: 100.0, rx: 30.0, ry: 20.0, fill: Some(Rgb(0, 200, 0)), stroke: None },
                Op::Text { x: 150.0, y: 150.0, text: "日本語 ABC".into(), font: font.clone(), color: Rgb(255, 0, 0), anchor: Anchor::Center },
                Op::Text { x: 5.0, y: 180.0, text: "plain".into(), font: FontRef { style: 0, ..font }, color: Rgb::BLACK, anchor: Anchor::TopLeft },
            ],
        }
    }

    #[test]
    fn extension_mapping() {
        assert_eq!(ImageFormat::from_extension(Path::new("a.BMP")), Some(ImageFormat::Bmp));
        assert_eq!(ImageFormat::from_extension(Path::new("a.png")), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::from_extension(Path::new("a.emf")), Some(ImageFormat::Emf));
        assert_eq!(ImageFormat::from_extension(Path::new("a.wmf")), Some(ImageFormat::Wmf));
        assert_eq!(ImageFormat::from_extension(Path::new("a.gif")), None);
        assert_eq!(ImageFormat::from_extension(Path::new("a")), None);
    }

    #[test]
    fn handle_table_reuses_lowest() {
        let mut t = Table::new(0);
        assert_eq!((t.alloc(), t.alloc(), t.alloc()), (0, 1, 2));
        t.free(1);
        assert_eq!(t.alloc(), 1);
        assert_eq!(t.count, 3);
    }

    #[test]
    fn blocks_skip_holes_and_merge_rows() {
        // 4x3 image, holes at (1,1) and (1,2), and the whole of column 3 transparent
        let mut rgba = vec![0u8; 4 * 3 * 4];
        for j in 0..3 {
            for i in 0..3 {
                if i != 1 || j == 0 {
                    let s = (j * 4 + i) * 4;
                    rgba[s..s + 4].copy_from_slice(&[10, 20, 30, 255]);
                }
            }
        }
        let b = opaque_blocks(0, 0, 4, 3, &rgba, (4, 3));
        let area: u32 = b.iter().map(|b| b.w * b.h).sum();
        assert_eq!(area, 7);
        assert!(b.iter().any(|b| b.h == 2 && b.w == 1), "columns 0 and 2 merge vertically");
        assert_eq!(&b[0].bits[..3], &[30, 20, 10]);
    }
}
