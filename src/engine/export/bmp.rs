//! 8 bit palette bitmaps: quantizer, BMP / RLE8 encoders and the clipboard DIB.
use super::{info_header, push_u32};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Indexed {
    pub w: u32,
    pub h: u32,
    pub palette: Vec<[u8; 3]>,
    /// top-down, `w * h` palette indices
    pub pixels: Vec<u8>,
}

fn pack(c: [u8; 3]) -> u32 {
    (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32
}

fn unpack(k: u32) -> [u8; 3] {
    [(k >> 16) as u8, (k >> 8) as u8, k as u8]
}

/// Palette with at most `max` (<= 256) entries from distinct colours with their pixel counts:
/// exact when they fit, otherwise median cut.
pub fn palette_from_histogram(hist: &[([u8; 3], u64)], max: usize) -> Vec<[u8; 3]> {
    let max = max.clamp(1, 256);
    if hist.len() <= max {
        return hist.iter().map(|h| h.0).collect();
    }
    let mut boxes: Vec<Vec<([u8; 3], u64)>> = vec![hist.to_vec()];
    while boxes.len() < max {
        let pick = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .max_by_key(|(_, b)| (widest(b).1, b.iter().map(|e| e.1).sum::<u64>()))
            .map(|(i, _)| i);
        let Some(i) = pick else { break };
        let mut b = boxes.swap_remove(i);
        let (ch, _) = widest(&b);
        b.sort_by_key(|e| e.0[ch]);
        let total: u64 = b.iter().map(|e| e.1).sum();
        let (mut acc, mut cut) = (0, 1);
        for (k, e) in b.iter().enumerate() {
            acc += e.1;
            if acc * 2 >= total {
                cut = (k + 1).clamp(1, b.len() - 1);
                break;
            }
        }
        let hi = b.split_off(cut);
        boxes.push(b);
        boxes.push(hi);
    }
    let mut pal: Vec<[u8; 3]> = boxes
        .iter()
        .map(|b| {
            let n: u64 = b.iter().map(|e| e.1).sum::<u64>().max(1);
            let m = |c: usize| ((b.iter().map(|e| e.0[c] as u64 * e.1).sum::<u64>() + n / 2) / n) as u8;
            [m(0), m(1), m(2)]
        })
        .collect();
    pal.sort_by_key(|c| pack(*c));
    pal.dedup();
    pal
}

/// Channel with the largest range and that range.
fn widest(b: &[([u8; 3], u64)]) -> (usize, u32) {
    let mut best = (0, 0);
    for c in 0..3 {
        let lo = b.iter().map(|e| e.0[c]).min().unwrap_or(0);
        let hi = b.iter().map(|e| e.0[c]).max().unwrap_or(0);
        let r = (hi - lo) as u32;
        if r > best.1 {
            best = (c, r);
        }
    }
    best
}

/// Nearest palette entry, remembered per colour.
pub struct Mapper<'a> {
    palette: &'a [[u8; 3]],
    cache: HashMap<u32, u8>,
}

impl<'a> Mapper<'a> {
    pub fn new(palette: &'a [[u8; 3]]) -> Mapper<'a> {
        Mapper { palette, cache: HashMap::new() }
    }

    pub fn index(&mut self, c: [u8; 3]) -> u8 {
        let pal = self.palette;
        *self.cache.entry(pack(c)).or_insert_with(|| {
            let d = |p: &[u8; 3]| (0..3).map(|i| (p[i] as i32 - c[i] as i32).pow(2)).sum::<i32>();
            pal.iter().enumerate().min_by_key(|(_, p)| d(p)).map(|(i, _)| i as u8).unwrap_or(0)
        })
    }
}

pub fn quantize(rgba: &[u8], w: u32, h: u32) -> Indexed {
    let n = w as usize * h as usize;
    let mut hist: HashMap<u32, u64> = HashMap::new();
    for p in rgba.chunks_exact(4).take(n) {
        *hist.entry(pack([p[0], p[1], p[2]])).or_default() += 1;
    }
    let mut entries: Vec<([u8; 3], u64)> = hist.into_iter().map(|(k, c)| (unpack(k), c)).collect();
    entries.sort_by_key(|e| pack(e.0));
    let mut palette = palette_from_histogram(&entries, 256);
    if palette.is_empty() {
        palette.push([0, 0, 0]);
    }
    let mut pixels = vec![0u8; n];
    let mut m = Mapper::new(&palette);
    for (o, p) in pixels.iter_mut().zip(rgba.chunks_exact(4)) {
        *o = m.index([p[0], p[1], p[2]]);
    }
    Indexed { w, h, palette, pixels }
}

fn palette_bytes(img: &Indexed) -> Vec<u8> {
    img.palette.iter().flat_map(|c| [c[2], c[1], c[0], 0]).collect()
}

fn file_wrap(img: &Indexed, compression: u32, data: &[u8]) -> Vec<u8> {
    let pal = palette_bytes(img);
    let off = 14 + 40 + pal.len();
    let mut v = Vec::with_capacity(off + data.len());
    v.extend_from_slice(b"BM");
    push_u32(&mut v, (off + data.len()) as u32);
    push_u32(&mut v, 0);
    push_u32(&mut v, off as u32);
    v.extend(info_header(img.w, img.h, 8, compression, data.len() as u32, img.palette.len() as u32));
    v.extend(pal);
    v.extend_from_slice(data);
    v
}

fn raw_rows(img: &Indexed) -> Vec<u8> {
    let w = img.w as usize;
    let stride = (w + 3) & !3;
    let mut out = vec![0u8; stride * img.h as usize];
    for (j, row) in img.pixels.chunks(w.max(1)).take(img.h as usize).enumerate() {
        let d = (img.h as usize - 1 - j) * stride;
        out[d..d + row.len()].copy_from_slice(row);
    }
    out
}

pub fn encode_bmp8(img: &Indexed) -> Vec<u8> {
    file_wrap(img, 0, &raw_rows(img))
}

/// Packed DIB (info header, palette, bits) without a file header.
pub fn dib_bytes(img: &Indexed) -> Vec<u8> {
    let data = raw_rows(img);
    let mut v = info_header(img.w, img.h, 8, 0, data.len() as u32, img.palette.len() as u32);
    v.extend(palette_bytes(img));
    v.extend(data);
    v
}

fn rle_row(row: &[u8], out: &mut Vec<u8>) {
    let n = row.len();
    let run3 = |j: usize| j + 2 < n && row[j] == row[j + 1] && row[j] == row[j + 2];
    let mut i = 0;
    while i < n {
        if run3(i) {
            let mut r = 1;
            while i + r < n && r < 255 && row[i + r] == row[i] {
                r += 1;
            }
            out.extend_from_slice(&[r as u8, row[i]]);
            i += r;
            continue;
        }
        let mut j = i + 1;
        while j < n && j - i < 255 && !run3(j) {
            j += 1;
        }
        let lit = &row[i..j];
        if lit.len() >= 3 {
            out.extend_from_slice(&[0, lit.len() as u8]);
            out.extend_from_slice(lit);
            if lit.len() % 2 == 1 {
                out.push(0);
            }
        } else {
            for &p in lit {
                out.extend_from_slice(&[1, p]);
            }
        }
        i = j;
    }
}

pub fn encode_rle8(img: &Indexed) -> Vec<u8> {
    let w = img.w as usize;
    let mut data = Vec::new();
    if w > 0 {
        for row in img.pixels.chunks(w).take(img.h as usize).collect::<Vec<_>>().into_iter().rev() {
            rle_row(row, &mut data);
            data.extend_from_slice(&[0, 0]);
        }
    }
    data.extend_from_slice(&[0, 1]);
    file_wrap(img, 1, &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg(seed: &mut u32) -> u32 {
        *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        *seed >> 8
    }

    fn gradient(w: u32, h: u32) -> Vec<u8> {
        let mut v = Vec::new();
        for j in 0..h {
            for i in 0..w {
                v.extend_from_slice(&[(i * 255 / w) as u8, (j * 255 / h) as u8, ((i + j) % 256) as u8, 255]);
            }
        }
        v
    }

    fn indexed(w: u32, h: u32, f: impl Fn(u32, u32) -> u8, ncol: usize) -> Indexed {
        let palette = (0..ncol).map(|i| [i as u8, 255 - i as u8, (i * 7) as u8]).collect();
        let pixels = (0..h).flat_map(|j| (0..w).map(move |i| (i, j))).map(|(i, j)| f(i, j)).collect();
        Indexed { w, h, palette, pixels }
    }

    /// Minimal BI_RLE8 decoder; asserts that no row runs past the width.
    fn decode_rle(bmp: &[u8], w: usize, h: usize) -> Vec<u8> {
        let off = u32::from_le_bytes(bmp[10..14].try_into().unwrap()) as usize;
        let d = &bmp[off..];
        let mut out = vec![0u8; w * h];
        let (mut x, mut y, mut i) = (0usize, 0usize, 0usize);
        let mut ended = false;
        while i < d.len() {
            let (a, b) = (d[i] as usize, d[i + 1]);
            i += 2;
            if a > 0 {
                assert!(x + a <= w, "run past the row end");
                for k in 0..a {
                    out[(h - 1 - y) * w + x + k] = b;
                }
                x += a;
            } else {
                match b {
                    0 => {
                        x = 0;
                        y += 1;
                    }
                    1 => {
                        ended = true;
                        break;
                    }
                    2 => panic!("delta not expected"),
                    n => {
                        let n = n as usize;
                        assert!(n >= 3, "absolute mode needs >= 3");
                        assert!(x + n <= w, "absolute run past the row end");
                        for k in 0..n {
                            out[(h - 1 - y) * w + x + k] = d[i + k];
                        }
                        x += n;
                        i += n + (n & 1);
                    }
                }
            }
        }
        assert!(ended);
        assert_eq!(y, h);
        out
    }

    #[test]
    fn exact_palette_when_few_colours() {
        let rgba: Vec<u8> = (0..100u32).flat_map(|i| [(i % 5) as u8 * 50, 7, 9, 255]).collect();
        let q = quantize(&rgba, 10, 10);
        assert_eq!(q.palette.len(), 5);
        for (k, p) in q.pixels.iter().enumerate() {
            assert_eq!(q.palette[*p as usize], [(k % 5) as u8 * 50, 7, 9]);
        }
    }

    #[test]
    fn median_cut_stays_within_256() {
        let q = quantize(&gradient(200, 150), 200, 150);
        assert!(q.palette.len() <= 256 && q.palette.len() > 100, "{}", q.palette.len());
        assert_eq!(q.pixels.len(), 200 * 150);
        assert!(q.pixels.iter().all(|&p| (p as usize) < q.palette.len()));
        // the error stays small on a smooth gradient
        let rgba = gradient(200, 150);
        let worst = rgba
            .chunks_exact(4)
            .zip(&q.pixels)
            .map(|(p, &i)| (0..3).map(|c| (p[c] as i32 - q.palette[i as usize][c] as i32).abs()).max().unwrap())
            .max()
            .unwrap();
        assert!(worst < 64, "{worst}");
    }

    #[test]
    fn empty_image() {
        let q = quantize(&[], 0, 0);
        assert_eq!(q.palette.len(), 1);
        let b = encode_bmp8(&q);
        assert!(b.len() >= 54);
        encode_rle8(&q);
    }

    #[test]
    fn bmp8_roundtrip_with_padding() {
        for w in [1u32, 2, 3, 4, 5, 13] {
            let img = indexed(w, 7, |i, j| ((i * 3 + j) % 9) as u8, 9);
            let bytes = encode_bmp8(&img);
            let dec = image::load_from_memory_with_format(&bytes, image::ImageFormat::Bmp).unwrap().to_rgb8();
            assert_eq!(dec.dimensions(), (w, 7));
            for j in 0..7 {
                for i in 0..w {
                    let want = img.palette[img.pixels[(j * w + i) as usize] as usize];
                    assert_eq!(dec.get_pixel(i, j).0, want, "w={w} ({i},{j})");
                }
            }
            assert_eq!(u32::from_le_bytes(bytes[46..50].try_into().unwrap()), 9, "biClrUsed");
        }
    }

    #[test]
    fn rle_flat_noisy_and_mixed() {
        let mut seed = 7;
        let noisy: Vec<u8> = (0..60 * 20).map(|_| (lcg(&mut seed) % 200) as u8).collect();
        let cases = [
            indexed(300, 4, |_, _| 5, 8),
            indexed(60, 20, |i, j| noisy[(j * 60 + i) as usize], 200),
            indexed(33, 9, |i, j| if (i / 5 + j) % 3 == 0 { 1 } else { (i % 7) as u8 }, 8),
            indexed(1, 5, |_, j| j as u8, 5),
            indexed(2, 3, |i, _| i as u8, 2),
            indexed(7, 2, |i, _| (i / 2) as u8, 4),
        ];
        for img in cases {
            let bytes = encode_rle8(&img);
            assert_eq!(decode_rle(&bytes, img.w as usize, img.h as usize), img.pixels);
            // the image crate agrees
            let dec = image::load_from_memory_with_format(&bytes, image::ImageFormat::Bmp).unwrap().to_rgb8();
            for (k, p) in dec.pixels().enumerate() {
                assert_eq!(p.0, img.palette[img.pixels[k] as usize]);
            }
        }
        // flat rows compress
        let flat = indexed(300, 4, |_, _| 5, 8);
        assert!(encode_rle8(&flat).len() < encode_bmp8(&flat).len());
    }

    #[test]
    fn dib_layout() {
        let img = indexed(3, 2, |i, _| i as u8, 3);
        let d = dib_bytes(&img);
        assert_eq!(u32::from_le_bytes(d[0..4].try_into().unwrap()), 40);
        assert_eq!(d.len(), 40 + 3 * 4 + 4 * 2);
    }
}
