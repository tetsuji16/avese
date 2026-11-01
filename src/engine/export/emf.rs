//! Enhanced metafile writer (96 dpi, MM_TEXT-like device units).
use super::{Table, coord, font_px, info_header, opaque_blocks, push_i32, push_u16, push_u32};
use crate::engine::raster::{anchor_origin, measure_text};
use crate::engine::scene::{FontRef, Op, Rgb, Scene};

const EMR_HEADER: u32 = 1;
const EMR_POLYGON: u32 = 3;
const EMR_POLYLINE: u32 = 4;
const EMR_EOF: u32 = 14;
const EMR_SETBKMODE: u32 = 18;
const EMR_SETTEXTALIGN: u32 = 22;
const EMR_SETTEXTCOLOR: u32 = 24;
const EMR_SELECTOBJECT: u32 = 37;
const EMR_CREATEPEN: u32 = 38;
const EMR_CREATEBRUSHINDIRECT: u32 = 39;
const EMR_DELETEOBJECT: u32 = 40;
const EMR_ELLIPSE: u32 = 42;
const EMR_RECTANGLE: u32 = 43;
const EMR_STRETCHDIBITS: u32 = 81;
const EMR_EXTCREATEFONTINDIRECTW: u32 = 82;
const EMR_EXTTEXTOUTW: u32 = 84;
const EMR_POLYGON16: u32 = 86;
const EMR_POLYLINE16: u32 = 87;

const STOCK_NULL_BRUSH: u32 = 0x8000_0005;
const STOCK_NULL_PEN: u32 = 0x8000_0008;
const STOCK_DEVICE_DEFAULT_FONT: u32 = 0x8000_000E;
/// points per POLYLINE16 record
const CHUNK: usize = 8000;

type PenKey = Option<(Rgb, i32)>;
type FontKey = (String, i32, u32, bool);

struct Writer {
    buf: Vec<u8>,
    n_rec: u32,
    handles: Table,
    pen: Option<(PenKey, Option<u32>)>,
    brush: Option<(Option<Rgb>, Option<u32>)>,
    font: Option<(FontKey, u32)>,
    text_color: Option<Rgb>,
}

fn colorref(c: Rgb) -> u32 {
    c.0 as u32 | (c.1 as u32) << 8 | (c.2 as u32) << 16
}

fn push_rect(v: &mut Vec<u8>, r: [i32; 4]) {
    r.iter().for_each(|&x| push_i32(v, x));
}

/// Inclusive bounds of points, grown by `pad`.
fn bounds(pts: &[(i32, i32)], pad: i32) -> [i32; 4] {
    let mut r = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
    for &(x, y) in pts {
        r = [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)];
    }
    [r[0] - pad, r[1] - pad, r[2] + pad, r[3] + pad]
}

impl Writer {
    fn rec(&mut self, ty: u32, f: impl FnOnce(&mut Vec<u8>)) {
        let start = self.buf.len();
        push_u32(&mut self.buf, ty);
        push_u32(&mut self.buf, 0);
        f(&mut self.buf);
        while (self.buf.len() - start) % 4 != 0 {
            self.buf.push(0);
        }
        let size = (self.buf.len() - start) as u32;
        self.buf[start + 4..start + 8].copy_from_slice(&size.to_le_bytes());
        self.n_rec += 1;
    }

    /// Creates the object (if `body` is given) and selects it, or selects the stock object.
    /// The previously selected object is deleted afterwards.
    fn install(&mut self, ty: u32, body: Option<Vec<u8>>, stock: u32, old: Option<u32>) -> Option<u32> {
        let h = body.map(|b| {
            let h = self.handles.alloc() as u32;
            self.rec(ty, |v| {
                push_u32(v, h);
                v.extend(b);
            });
            h
        });
        self.rec(EMR_SELECTOBJECT, |v| push_u32(v, h.unwrap_or(stock)));
        if let Some(o) = old {
            self.rec(EMR_DELETEOBJECT, |v| push_u32(v, o));
            self.handles.free(o as usize);
        }
        h
    }

    fn select_pen(&mut self, key: PenKey) {
        if matches!(&self.pen, Some((k, _)) if *k == key) {
            return;
        }
        let old = self.pen.take().and_then(|p| p.1);
        let body = key.map(|(c, w)| {
            let mut b = Vec::new();
            push_u32(&mut b, 0);
            push_i32(&mut b, w);
            push_i32(&mut b, 0);
            push_u32(&mut b, colorref(c));
            b
        });
        let h = self.install(EMR_CREATEPEN, body, STOCK_NULL_PEN, old);
        self.pen = Some((key, h));
    }

    fn select_brush(&mut self, key: Option<Rgb>) {
        if matches!(&self.brush, Some((k, _)) if *k == key) {
            return;
        }
        let old = self.brush.take().and_then(|p| p.1);
        let body = key.map(|c| {
            let mut b = Vec::new();
            push_u32(&mut b, 0);
            push_u32(&mut b, colorref(c));
            push_u32(&mut b, 0);
            b
        });
        let h = self.install(EMR_CREATEBRUSHINDIRECT, body, STOCK_NULL_BRUSH, old);
        self.brush = Some((key, h));
    }

    fn select_font(&mut self, font: &FontRef, sjis: bool) {
        let key: FontKey = (font.family.clone(), font_px(font), font.style, sjis);
        if matches!(&self.font, Some((k, _)) if *k == key) {
            return;
        }
        let old = self.font.take().map(|f| f.1);
        let mut b = Vec::with_capacity(92);
        push_i32(&mut b, -key.1);
        push_i32(&mut b, 0);
        push_i32(&mut b, 0);
        push_i32(&mut b, 0);
        push_i32(&mut b, if font.style & 1 != 0 { 700 } else { 400 });
        b.extend_from_slice(&[
            (font.style & 2 != 0) as u8,
            (font.style & 4 != 0) as u8,
            (font.style & 8 != 0) as u8,
            if sjis { 128 } else { 1 },
            0,
            0,
            0,
            0,
        ]);
        let mut name: Vec<u16> = font.family.encode_utf16().take(31).collect();
        name.resize(32, 0);
        name.iter().for_each(|&u| push_u16(&mut b, u));
        let h = self.install(EMR_EXTCREATEFONTINDIRECTW, Some(b), 0, old);
        self.font = h.map(|h| (key, h));
    }

    fn polyline(&mut self, pts: &[(i32, i32)], pad: i32, closed: bool) {
        let small = pts.iter().all(|&(x, y)| i16::try_from(x).is_ok() && i16::try_from(y).is_ok());
        let chunks: Vec<&[(i32, i32)]> = if closed || pts.len() <= CHUNK {
            vec![pts]
        } else {
            let mut v = Vec::new();
            let mut s = 0;
            while s + 1 < pts.len() {
                let e = (s + CHUNK).min(pts.len());
                v.push(&pts[s..e]);
                s = e - 1;
            }
            v
        };
        for c in chunks {
            let ty = match (closed, small) {
                (false, true) => EMR_POLYLINE16,
                (false, false) => EMR_POLYLINE,
                (true, true) => EMR_POLYGON16,
                (true, false) => EMR_POLYGON,
            };
            self.rec(ty, |v| {
                push_rect(v, bounds(c, pad));
                push_u32(v, c.len() as u32);
                for &(x, y) in c {
                    if small {
                        push_u16(v, x as i16 as u16);
                        push_u16(v, y as i16 as u16);
                    } else {
                        push_i32(v, x);
                        push_i32(v, y);
                    }
                }
            });
        }
    }

    fn text(&mut self, op: &Op) {
        let Op::Text { x, y, text, font, color, anchor } = op else { return };
        if text.is_empty() || !x.is_finite() || !y.is_finite() {
            return;
        }
        let (tw, th) = measure_text(text, font);
        let (ox, oy) = anchor_origin(*anchor, *x, *y, tw, th);
        let (Some(ox), Some(oy)) = (coord(ox), coord(oy)) else { return };
        let box_ = [ox, oy, ox + tw.ceil() as i32, oy + th.ceil() as i32];
        let units: Vec<u16> = text.encode_utf16().collect();
        // advance of every character, rounded so the running position does not drift
        let mut dx = Vec::with_capacity(units.len());
        let (mut cum, mut done) = (0.0f32, 0i32);
        for ch in text.chars() {
            cum += measure_text(&ch.to_string(), font).0;
            let at = cum.round() as i32;
            for _ in 0..ch.len_utf16() {
                dx.push(0);
            }
            let n = dx.len();
            dx[n - 1] = at - done;
            done = at;
        }
        self.select_font(font, !text.is_ascii());
        if self.text_color != Some(*color) {
            self.rec(EMR_SETTEXTCOLOR, |v| push_u32(v, colorref(*color)));
            self.text_color = Some(*color);
        }
        self.rec(EMR_EXTTEXTOUTW, |v| {
            push_rect(v, box_);
            push_u32(v, 1);
            v.extend_from_slice(&1.0f32.to_le_bytes());
            v.extend_from_slice(&1.0f32.to_le_bytes());
            push_i32(v, ox);
            push_i32(v, oy);
            push_u32(v, units.len() as u32);
            let off_string = 76u32;
            push_u32(v, off_string);
            push_u32(v, 0);
            push_rect(v, [0, 0, 0, 0]);
            let str_bytes = ((units.len() * 2 + 3) & !3) as u32;
            push_u32(v, off_string + str_bytes);
            units.iter().for_each(|&u| push_u16(v, u));
            while v.len() % 4 != 0 {
                v.push(0);
            }
            dx.iter().for_each(|&d| push_i32(v, d));
        });
    }

    fn image(&mut self, op: &Op, canvas: (u32, u32)) {
        let Op::Image { x, y, w, h, rgba } = op else { return };
        for b in opaque_blocks(*x, *y, *w, *h, rgba, canvas) {
            self.rec(EMR_STRETCHDIBITS, |v| {
                push_rect(v, [b.x, b.y, b.x + b.w as i32 - 1, b.y + b.h as i32 - 1]);
                push_i32(v, b.x);
                push_i32(v, b.y);
                push_i32(v, 0);
                push_i32(v, 0);
                push_i32(v, b.w as i32);
                push_i32(v, b.h as i32);
                push_u32(v, 80);
                push_u32(v, 40);
                push_u32(v, 120);
                push_u32(v, b.bits.len() as u32);
                push_u32(v, 0);
                push_u32(v, 0x00CC_0020);
                push_i32(v, b.w as i32);
                push_i32(v, b.h as i32);
                v.extend(info_header(b.w, b.h, 24, 0, b.bits.len() as u32, 0));
                v.extend_from_slice(&b.bits);
            });
        }
    }
}

pub fn encode_emf(scene: &Scene) -> Vec<u8> {
    let (w, h) = (scene.width.max(1), scene.height.max(1));
    let mut e = Writer {
        buf: Vec::new(),
        n_rec: 0,
        handles: Table::new(1),
        pen: None,
        brush: None,
        font: None,
        text_color: None,
    };
    // the header is written last-patched: reserve its 100 bytes
    e.buf.resize(100, 0);
    e.n_rec = 1;
    e.rec(EMR_SETBKMODE, |v| push_u32(v, 1));
    e.rec(EMR_SETTEXTALIGN, |v| push_u32(v, 0));
    e.select_pen(None);
    e.select_brush(Some(scene.bg));
    e.rec(EMR_RECTANGLE, |v| push_rect(v, [0, 0, w as i32 + 1, h as i32 + 1]));

    for op in &scene.ops {
        match op {
            Op::Image { .. } => e.image(op, (w, h)),
            Op::Polyline { pts, color, width } => {
                let p: Option<Vec<(i32, i32)>> = pts.iter().map(|&(x, y)| Some((coord(x)?, coord(y)?))).collect();
                let Some(p) = p.filter(|p| p.len() >= 2) else { continue };
                let pw = if width.is_finite() { width.round().clamp(1.0, 1000.0) as i32 } else { 1 };
                e.select_pen(Some((*color, pw)));
                e.polyline(&p, pw / 2 + 1, false);
            }
            Op::Polygon { pts, fill, stroke } => {
                if fill.is_none() && stroke.is_none() {
                    continue;
                }
                let p: Option<Vec<(i32, i32)>> = pts.iter().map(|&(x, y)| Some((coord(x)?, coord(y)?))).collect();
                let Some(p) = p.filter(|p| p.len() >= 2) else { continue };
                e.select_pen(stroke.map(|c| (c, 1)));
                e.select_brush(*fill);
                e.polyline(&p, 1, true);
            }
            Op::Ellipse { cx, cy, rx, ry, fill, stroke } => {
                if fill.is_none() && stroke.is_none() {
                    continue;
                }
                let r = [cx - rx, cy - ry, cx + rx, cy + ry].map(coord);
                let [Some(a), Some(b), Some(c), Some(d)] = r else { continue };
                e.select_pen(stroke.map(|c| (c, 1)));
                e.select_brush(*fill);
                e.rec(EMR_ELLIPSE, |v| push_rect(v, [a, b, c, d]));
            }
            Op::Text { .. } => e.text(op),
        }
    }
    // release what is still selected so the handle table ends empty
    e.select_pen(None);
    e.select_brush(None);
    if let Some((_, h)) = e.font.take() {
        e.rec(EMR_SELECTOBJECT, |v| push_u32(v, STOCK_DEVICE_DEFAULT_FONT));
        e.rec(EMR_DELETEOBJECT, |v| push_u32(v, h));
    }
    e.rec(EMR_EOF, |v| {
        push_u32(v, 0);
        push_u32(v, 16);
        push_u32(v, 20);
    });

    // header
    let n_bytes = e.buf.len() as u32;
    let mut hd = Vec::with_capacity(100);
    push_u32(&mut hd, EMR_HEADER);
    push_u32(&mut hd, 100);
    push_rect(&mut hd, [0, 0, w as i32 - 1, h as i32 - 1]);
    let frame = |px: u32| (px as u64 * 2540 / 96) as i32;
    push_rect(&mut hd, [0, 0, frame(w), frame(h)]);
    push_u32(&mut hd, 0x464D_4520);
    push_u32(&mut hd, 0x0001_0000);
    push_u32(&mut hd, n_bytes);
    push_u32(&mut hd, e.n_rec);
    push_u16(&mut hd, e.handles.count as u16);
    push_u16(&mut hd, 0);
    push_u32(&mut hd, 0);
    push_u32(&mut hd, 0);
    push_u32(&mut hd, 0);
    // reference device: 1920x1080 pixels, 508x286 mm (96 dpi)
    push_i32(&mut hd, 1920);
    push_i32(&mut hd, 1080);
    push_i32(&mut hd, 508);
    push_i32(&mut hd, 286);
    push_u32(&mut hd, 0);
    push_u32(&mut hd, 0);
    push_u32(&mut hd, 0);
    e.buf[..100].copy_from_slice(&hd);
    e.buf
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::engine::export::tests::test_scene;

    fn u32_at(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
    }

    /// (type, offset, size) of every record; asserts the framing.
    pub fn walk(b: &[u8]) -> Vec<(u32, usize, usize)> {
        let mut v = Vec::new();
        let mut o = 0;
        while o < b.len() {
            let (ty, size) = (u32_at(b, o), u32_at(b, o + 4) as usize);
            assert!(size >= 8 && size % 4 == 0, "record {} at {o} size {size}", v.len());
            assert!(o + size <= b.len());
            v.push((ty, o, size));
            o += size;
        }
        assert_eq!(o, b.len());
        v
    }

    #[test]
    fn records_are_consistent() {
        let b = encode_emf(&test_scene());
        let recs = walk(&b);
        assert_eq!(recs[0].0, EMR_HEADER);
        assert_eq!(u32_at(&b, 40), 0x464D_4520);
        assert_eq!(u32_at(&b, 48) as usize, b.len(), "nBytes");
        assert_eq!(u32_at(&b, 52) as usize, recs.len(), "nRecords");
        let n_handles = u16::from_le_bytes([b[56], b[57]]) as u32;
        assert!(n_handles >= 2 && n_handles <= 8, "{n_handles}");
        let last = recs.last().unwrap();
        assert_eq!(last.0, EMR_EOF);
        assert_eq!(u32_at(&b, last.1 + 16), 20);
        for &(ty, o, _) in &recs {
            match ty {
                EMR_CREATEPEN | EMR_CREATEBRUSHINDIRECT | EMR_EXTCREATEFONTINDIRECTW | EMR_DELETEOBJECT => {
                    let h = u32_at(&b, o + 8);
                    assert!(h > 0 && h < n_handles);
                }
                EMR_SELECTOBJECT => {
                    let h = u32_at(&b, o + 8);
                    assert!(h & 0x8000_0000 != 0 || (h > 0 && h < n_handles));
                }
                _ => {}
            }
        }
        // the long polyline was split, the huge coordinates took the 32 bit record
        assert!(recs.iter().filter(|r| r.0 == EMR_POLYLINE16).count() >= 3);
        assert!(recs.iter().any(|r| r.0 == EMR_POLYLINE));
        assert!(recs.iter().any(|r| r.0 == EMR_EXTTEXTOUTW));
        assert!(recs.iter().filter(|r| r.0 == EMR_STRETCHDIBITS).count() >= 2, "holes split the image");
        // every created object is deleted again
        let created = recs.iter().filter(|r| matches!(r.0, EMR_CREATEPEN | EMR_CREATEBRUSHINDIRECT | EMR_EXTCREATEFONTINDIRECTW)).count();
        assert_eq!(created, recs.iter().filter(|r| r.0 == EMR_DELETEOBJECT).count());
    }

    #[test]
    fn handles_stay_few_for_many_ops() {
        let mut s = test_scene();
        s.ops = (0..500)
            .map(|i| Op::Polyline { pts: vec![(0.0, i as f32), (10.0, i as f32)], color: Rgb(i as u8, 0, 0), width: 1.0 })
            .collect();
        let b = encode_emf(&s);
        assert!(u16::from_le_bytes([b[56], b[57]]) <= 4);
    }

    #[cfg(windows)]
    #[test]
    fn gdi_accepts_the_metafile() {
        use windows_sys::Win32::Graphics::Gdi::{DeleteEnhMetaFile, GetEnhMetaFileHeader, SetEnhMetaFileBits};
        let s = Scene { width: 40, height: 30, bg: Rgb::WHITE, ops: vec![
            Op::Polyline { pts: vec![(1.0, 1.0), (30.0, 20.0)], color: Rgb(255, 0, 0), width: 2.0 },
            Op::Ellipse { cx: 20.0, cy: 15.0, rx: 8.0, ry: 5.0, fill: Some(Rgb(0, 255, 0)), stroke: Some(Rgb::BLACK) },
        ] };
        let b = encode_emf(&s);
        unsafe {
            let h = SetEnhMetaFileBits(b.len() as u32, b.as_ptr());
            assert!(!h.is_null());
            let mut hd: windows_sys::Win32::Graphics::Gdi::ENHMETAHEADER = std::mem::zeroed();
            let n = GetEnhMetaFileHeader(h, std::mem::size_of_val(&hd) as u32, &mut hd);
            assert!(n > 0);
            assert_eq!(hd.nRecords as usize, walk(&b).len());
            DeleteEnhMetaFile(h);
        }
    }
}
