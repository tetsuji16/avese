//! Windows metafile writer with an Aldus placeable header (96 dpi). Text is Shift-JIS.
use super::{Table, coord, font_px, info_header, opaque_blocks, push_u16, push_u32};
use crate::engine::raster::{anchor_origin, measure_text};
use crate::engine::scene::{FontRef, Op, Rgb, Scene};

const SETBKMODE: u16 = 0x0102;
const SETMAPMODE: u16 = 0x0103;
const SETTEXTCOLOR: u16 = 0x0209;
const SETTEXTALIGN: u16 = 0x012E;
const SETWINDOWORG: u16 = 0x020B;
const SETWINDOWEXT: u16 = 0x020C;
const CREATEPENINDIRECT: u16 = 0x02FA;
const CREATEFONTINDIRECT: u16 = 0x02FB;
const CREATEBRUSHINDIRECT: u16 = 0x02FC;
const SELECTOBJECT: u16 = 0x012D;
const DELETEOBJECT: u16 = 0x01F0;
const POLYGON: u16 = 0x0324;
const POLYLINE: u16 = 0x0325;
const ELLIPSE: u16 = 0x0418;
const RECTANGLE: u16 = 0x041B;
const TEXTOUT: u16 = 0x0521;
const STRETCHDIB: u16 = 0x0F43;
const EOF: u16 = 0x0000;

/// points per POLYLINE record
const CHUNK: usize = 16000;

type PenKey = Option<(Rgb, i32)>;
type FontKey = (String, i32, u32, bool);

struct Writer {
    buf: Vec<u8>,
    max_words: u32,
    handles: Table,
    pen: Option<(PenKey, u16)>,
    brush: Option<(Option<Rgb>, u16)>,
    font: Option<(FontKey, u16)>,
    text_color: Option<Rgb>,
}

fn c16(v: i32) -> i16 {
    v.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

fn push_i16(v: &mut Vec<u8>, x: i32) {
    v.extend_from_slice(&c16(x).to_le_bytes());
}

fn colorref(c: Rgb) -> u32 {
    c.0 as u32 | (c.1 as u32) << 8 | (c.2 as u32) << 16
}

fn sjis(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for ch in text.chars() {
        let mut b = [0u8; 4];
        let (bytes, _, bad) = encoding_rs::SHIFT_JIS.encode(ch.encode_utf8(&mut b));
        if bad {
            out.push(b'?');
        } else {
            out.extend_from_slice(&bytes);
        }
    }
    out
}

impl Writer {
    fn rec(&mut self, func: u16, f: impl FnOnce(&mut Vec<u8>)) {
        let start = self.buf.len();
        push_u32(&mut self.buf, 0);
        push_u16(&mut self.buf, func);
        f(&mut self.buf);
        if (self.buf.len() - start) % 2 != 0 {
            self.buf.push(0);
        }
        let words = ((self.buf.len() - start) / 2) as u32;
        self.buf[start..start + 4].copy_from_slice(&words.to_le_bytes());
        self.max_words = self.max_words.max(words);
    }

    /// Creates and selects an object, then deletes the one it replaces.
    fn install(&mut self, func: u16, body: Vec<u8>, old: Option<u16>) -> u16 {
        let h = self.handles.alloc() as u16;
        self.rec(func, |v| v.extend(body));
        self.rec(SELECTOBJECT, |v| push_u16(v, h));
        if let Some(o) = old {
            self.rec(DELETEOBJECT, |v| push_u16(v, o));
            self.handles.free(o as usize);
        }
        h
    }

    fn select_pen(&mut self, key: PenKey) {
        if matches!(&self.pen, Some((k, _)) if *k == key) {
            return;
        }
        let old = self.pen.take().map(|p| p.1);
        let mut b = Vec::new();
        match key {
            Some((c, w)) => {
                push_u16(&mut b, 0);
                push_i16(&mut b, w);
                push_i16(&mut b, 0);
                push_u32(&mut b, colorref(c));
            }
            None => {
                // PS_NULL
                push_u16(&mut b, 5);
                push_i16(&mut b, 0);
                push_i16(&mut b, 0);
                push_u32(&mut b, 0);
            }
        }
        let h = self.install(CREATEPENINDIRECT, b, old);
        self.pen = Some((key, h));
    }

    fn select_brush(&mut self, key: Option<Rgb>) {
        if matches!(&self.brush, Some((k, _)) if *k == key) {
            return;
        }
        let old = self.brush.take().map(|p| p.1);
        let mut b = Vec::new();
        // BS_SOLID, or BS_NULL
        push_u16(&mut b, if key.is_some() { 0 } else { 1 });
        push_u32(&mut b, key.map_or(0, colorref));
        push_u16(&mut b, 0);
        let h = self.install(CREATEBRUSHINDIRECT, b, old);
        self.brush = Some((key, h));
    }

    fn select_font(&mut self, font: &FontRef, jp: bool) {
        let key: FontKey = (font.family.clone(), font_px(font), font.style, jp);
        if matches!(&self.font, Some((k, _)) if *k == key) {
            return;
        }
        let old = self.font.take().map(|f| f.1);
        let mut b = Vec::with_capacity(50);
        push_i16(&mut b, -key.1);
        for _ in 0..3 {
            push_i16(&mut b, 0);
        }
        push_i16(&mut b, if font.style & 1 != 0 { 700 } else { 400 });
        b.extend_from_slice(&[
            (font.style & 2 != 0) as u8,
            (font.style & 4 != 0) as u8,
            (font.style & 8 != 0) as u8,
            if jp { 128 } else { 1 },
            0,
            0,
            0,
            0,
        ]);
        let mut name = Vec::new();
        for ch in font.family.chars() {
            let e = sjis(&ch.to_string());
            if name.len() + e.len() > 31 {
                break;
            }
            name.extend(e);
        }
        name.resize(32, 0);
        b.extend(name);
        let h = self.install(CREATEFONTINDIRECT, b, old);
        self.font = Some((key, h));
    }

    fn poly(&mut self, pts: &[(i32, i32)], closed: bool) -> Result<(), String> {
        if closed && pts.len() > u16::MAX as usize {
            return Err("WMF に書き出せない数の頂点を持つ多角形があります".into());
        }
        let mut chunks: Vec<&[(i32, i32)]> = Vec::new();
        if closed || pts.len() <= CHUNK {
            chunks.push(pts);
        } else {
            let mut s = 0;
            while s + 1 < pts.len() {
                let e = (s + CHUNK).min(pts.len());
                chunks.push(&pts[s..e]);
                s = e - 1;
            }
        }
        for c in chunks {
            self.rec(if closed { POLYGON } else { POLYLINE }, |v| {
                push_u16(v, c.len() as u16);
                for &(x, y) in c {
                    push_i16(v, x);
                    push_i16(v, y);
                }
            });
        }
        Ok(())
    }

    fn text(&mut self, op: &Op) {
        let Op::Text { x, y, text, font, color, anchor } = op else { return };
        if text.is_empty() || !x.is_finite() || !y.is_finite() {
            return;
        }
        let (tw, th) = measure_text(text, font);
        let (ox, oy) = anchor_origin(*anchor, *x, *y, tw, th);
        let (Some(ox), Some(oy)) = (coord(ox), coord(oy)) else { return };
        let mut bytes = sjis(text);
        bytes.truncate(i16::MAX as usize - 1);
        self.select_font(font, !text.is_ascii());
        if self.text_color != Some(*color) {
            self.rec(SETTEXTCOLOR, |v| push_u32(v, colorref(*color)));
            self.text_color = Some(*color);
        }
        self.rec(TEXTOUT, |v| {
            push_i16(v, bytes.len() as i32);
            v.extend_from_slice(&bytes);
            if bytes.len() % 2 != 0 {
                v.push(0);
            }
            push_i16(v, oy);
            push_i16(v, ox);
        });
    }

    fn image(&mut self, op: &Op, canvas: (u32, u32)) {
        let Op::Image { x, y, w, h, rgba } = op else { return };
        for b in opaque_blocks(*x, *y, *w, *h, rgba, canvas) {
            self.rec(STRETCHDIB, |v| {
                push_u32(v, 0x00CC_0020);
                push_u16(v, 0);
                for n in [b.h as i32, b.w as i32, 0, 0, b.h as i32, b.w as i32, b.y, b.x] {
                    push_i16(v, n);
                }
                v.extend(info_header(b.w, b.h, 24, 0, b.bits.len() as u32, 0));
                v.extend_from_slice(&b.bits);
            });
        }
    }
}

pub fn encode_wmf(scene: &Scene) -> Result<Vec<u8>, String> {
    let (w, h) = (scene.width.max(1), scene.height.max(1));
    if w > i16::MAX as u32 - 1 || h > i16::MAX as u32 - 1 {
        return Err("画像が大きすぎて WMF に書き出せません（上限 32766 ピクセル）".into());
    }
    let mut e = Writer { buf: Vec::new(), max_words: 0, handles: Table::new(0), pen: None, brush: None, font: None, text_color: None };
    e.rec(SETMAPMODE, |v| push_u16(v, 8));
    e.rec(SETWINDOWORG, |v| {
        push_i16(v, 0);
        push_i16(v, 0);
    });
    e.rec(SETWINDOWEXT, |v| {
        push_i16(v, h as i32);
        push_i16(v, w as i32);
    });
    e.rec(SETBKMODE, |v| push_u16(v, 1));
    e.rec(SETTEXTALIGN, |v| push_u16(v, 0));
    e.select_pen(None);
    e.select_brush(Some(scene.bg));
    e.rec(RECTANGLE, |v| {
        for n in [h as i32 + 1, w as i32 + 1, 0, 0] {
            push_i16(v, n);
        }
    });

    for op in &scene.ops {
        match op {
            Op::Image { .. } => e.image(op, (w, h)),
            Op::Polyline { pts, color, width } => {
                let p: Option<Vec<(i32, i32)>> = pts.iter().map(|&(x, y)| Some((coord(x)?, coord(y)?))).collect();
                let Some(p) = p.filter(|p| p.len() >= 2) else { continue };
                let pw = if width.is_finite() { width.round().clamp(1.0, 1000.0) as i32 } else { 1 };
                e.select_pen(Some((*color, pw)));
                e.poly(&p, false)?;
            }
            Op::Polygon { pts, fill, stroke } => {
                if fill.is_none() && stroke.is_none() {
                    continue;
                }
                let p: Option<Vec<(i32, i32)>> = pts.iter().map(|&(x, y)| Some((coord(x)?, coord(y)?))).collect();
                let Some(p) = p.filter(|p| p.len() >= 2) else { continue };
                e.select_pen(stroke.map(|c| (c, 1)));
                e.select_brush(*fill);
                e.poly(&p, true)?;
            }
            Op::Ellipse { cx, cy, rx, ry, fill, stroke } => {
                if fill.is_none() && stroke.is_none() {
                    continue;
                }
                let r = [cx - rx, cy - ry, cx + rx, cy + ry].map(coord);
                let [Some(l), Some(t), Some(rt), Some(b)] = r else { continue };
                e.select_pen(stroke.map(|c| (c, 1)));
                e.select_brush(*fill);
                e.rec(ELLIPSE, |v| {
                    for n in [b, rt, t, l] {
                        push_i16(v, n);
                    }
                });
            }
            Op::Text { .. } => e.text(op),
        }
    }
    // the objects still selected stay in the file; players free them at the end
    e.rec(EOF, |_| {});

    let body_words = e.buf.len() / 2;
    let mut out = Vec::with_capacity(22 + 18 + e.buf.len());
    push_u32(&mut out, 0x9AC6_CDD7);
    push_u16(&mut out, 0);
    for n in [0, 0, w as i32, h as i32] {
        push_i16(&mut out, n);
    }
    push_u16(&mut out, 96);
    push_u32(&mut out, 0);
    let sum = out.chunks_exact(2).fold(0u16, |a, c| a ^ u16::from_le_bytes([c[0], c[1]]));
    push_u16(&mut out, sum);
    // METAHEADER
    push_u16(&mut out, 1);
    push_u16(&mut out, 9);
    push_u16(&mut out, 0x0300);
    push_u32(&mut out, (9 + body_words) as u32);
    push_u16(&mut out, e.handles.count as u16);
    push_u32(&mut out, e.max_words);
    push_u16(&mut out, 0);
    out.extend(e.buf);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::export::tests::test_scene;

    fn u16_at(b: &[u8], o: usize) -> u16 {
        u16::from_le_bytes([b[o], b[o + 1]])
    }
    fn u32_at(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
    }

    #[test]
    fn placeable_header_and_records() {
        let b = encode_wmf(&test_scene()).unwrap();
        assert_eq!(u32_at(&b, 0), 0x9AC6_CDD7);
        let x = (0..10).fold(0u16, |a, i| a ^ u16_at(&b, i * 2));
        assert_eq!(x, u16_at(&b, 20), "checksum");
        assert_eq!(u16_at(&b, 10), 300);
        assert_eq!(u16_at(&b, 12), 200);
        assert_eq!(u16_at(&b, 14), 96);
        let m = &b[22..];
        assert_eq!(u16_at(m, 0), 1);
        assert_eq!(u16_at(m, 2), 9);
        assert_eq!(u32_at(m, 6) as usize * 2, m.len(), "mtSize");
        let n_obj = u16_at(m, 10);
        let max_rec = u32_at(m, 12);
        let (mut o, mut sum, mut seen_max) = (18, 9usize, 0u32);
        let mut table: Vec<bool> = vec![false; n_obj as usize];
        let mut funcs = Vec::new();
        while o < m.len() {
            let words = u32_at(m, o);
            let f = u16_at(m, o + 4);
            assert!(words >= 3);
            seen_max = seen_max.max(words);
            sum += words as usize;
            if matches!(f, CREATEPENINDIRECT | CREATEBRUSHINDIRECT | CREATEFONTINDIRECT) {
                // the new object takes the lowest free slot
                let slot = table.iter().position(|u| !u).expect("object table full");
                table[slot] = true;
                // the select record that follows names that slot
                let next = o + words as usize * 2;
                assert_eq!(u16_at(m, next + 4), SELECTOBJECT);
                assert_eq!(u16_at(m, next + 6) as usize, slot);
            }
            if f == DELETEOBJECT {
                table[u16_at(m, o + 6) as usize] = false;
            }
            funcs.push(f);
            o += words as usize * 2;
        }
        assert_eq!(o, m.len());
        assert_eq!(sum * 2, m.len(), "record sizes add up to mtSize");
        assert_eq!(seen_max, max_rec);
        assert_eq!(*funcs.last().unwrap(), EOF);
        assert!(funcs.contains(&TEXTOUT) && funcs.contains(&STRETCHDIB) && funcs.contains(&ELLIPSE) && funcs.contains(&POLYGON));
        assert!(funcs.iter().filter(|&&f| f == POLYLINE).count() >= 3, "long polyline is chunked");
    }

    #[test]
    fn text_is_shift_jis() {
        let b = encode_wmf(&test_scene()).unwrap();
        let want = sjis("日本語 ABC");
        assert_eq!(want[..2], [0x93, 0xFA]);
        assert!(b.windows(want.len()).any(|w| w == want.as_slice()));
    }

    #[test]
    fn oversized_scene_is_an_error() {
        let s = Scene { width: 40000, height: 10, bg: Rgb::WHITE, ops: vec![] };
        assert!(encode_wmf(&s).is_err());
    }
}
