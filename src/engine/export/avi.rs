//! Uncompressed 8 bit AVI with one palette shared by all frames.
use super::bmp::{Mapper, palette_from_histogram};
use super::{push_u16, push_u32, temp_sibling};
use std::collections::HashSet;
use std::io::{BufWriter, Write};
use std::path::Path;

const LIMIT: u64 = 0x7FFF_FFFF - (4 << 20);
const AVIF_HASINDEX: u32 = 0x10;
const AVIIF_KEYFRAME: u32 = 0x10;
const STRF_SIZE: u64 = 40 + 256 * 4;

/// Frame data as RGBA at exactly (w, h).
fn fit(f: (Vec<u8>, u32, u32), w: u32, h: u32) -> Result<Vec<u8>, String> {
    let (rgba, fw, fh) = f;
    if fw == 0 || fh == 0 || rgba.len() != fw as usize * fh as usize * 4 {
        return Err("フレームの画像データが不正です".into());
    }
    if (fw, fh) == (w, h) {
        return Ok(rgba);
    }
    let img = image::RgbaImage::from_raw(fw, fh, rgba).ok_or("フレームの画像データが不正です")?;
    Ok(image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle).into_raw())
}

struct Gather {
    /// all colours seen, until there are more than 256
    exact: Option<HashSet<[u8; 3]>>,
    /// 5 bits per channel: count and channel sums
    bins: Vec<(u64, u64, u64, u64)>,
}

impl Gather {
    fn new() -> Gather {
        Gather { exact: Some(HashSet::new()), bins: vec![(0, 0, 0, 0); 1 << 15] }
    }

    fn add(&mut self, rgba: &[u8]) {
        for p in rgba.chunks_exact(4) {
            let c = [p[0], p[1], p[2]];
            if let Some(set) = &mut self.exact {
                set.insert(c);
                if set.len() > 256 {
                    self.exact = None;
                }
            }
            let k = (c[0] as usize >> 3) << 10 | (c[1] as usize >> 3) << 5 | (c[2] as usize >> 3);
            let b = &mut self.bins[k];
            *b = (b.0 + 1, b.1 + c[0] as u64, b.2 + c[1] as u64, b.3 + c[2] as u64);
        }
    }

    fn palette(self) -> Vec<[u8; 3]> {
        if let Some(set) = self.exact {
            let mut v: Vec<[u8; 3]> = set.into_iter().collect();
            v.sort();
            return v;
        }
        let hist: Vec<([u8; 3], u64)> = self
            .bins
            .iter()
            .filter(|b| b.0 > 0)
            .map(|b| ([((b.1 + b.0 / 2) / b.0) as u8, ((b.2 + b.0 / 2) / b.0) as u8, ((b.3 + b.0 / 2) / b.0) as u8], b.0))
            .collect();
        palette_from_histogram(&hist, 256)
    }
}

fn rate_pair(fps: f64) -> Result<(u32, u32), String> {
    if !fps.is_finite() || fps <= 0.0 || fps > 1000.0 {
        return Err("フレームレートが範囲外です（0 より大きく 1000 以下）".into());
    }
    if (fps - fps.round()).abs() < 1e-9 {
        return Ok((1, fps.round() as u32));
    }
    let rate = (fps * 1000.0).round() as u64;
    if rate == 0 {
        return Err("フレームレートが小さすぎます".into());
    }
    let g = {
        let (mut a, mut b) = (rate, 1000u64);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        a
    };
    Ok(((1000 / g) as u32, (rate / g) as u32))
}

pub fn write_avi(
    path: &Path,
    n_frames: usize,
    w: u32,
    h: u32,
    fps: f64,
    mut frame: impl FnMut(usize) -> Result<(Vec<u8>, u32, u32), String>,
    mut progress: impl FnMut(f32),
) -> Result<(), String> {
    if n_frames == 0 || w == 0 || h == 0 {
        return Err("AVI にするフレームがありません".into());
    }
    let (scale, rate) = rate_pair(fps)?;
    let stride = (w as u64 + 3) & !3;
    let fsz = stride * h as u64;
    let chunk = 8 + fsz;
    let movi_list = 4 + n_frames as u64 * chunk;
    let hdrl_list = 4 + (8 + 56) + (8 + 4 + (8 + 56) + (8 + STRF_SIZE));
    let riff = 4 + (8 + hdrl_list) + (8 + movi_list) + (8 + 16 * n_frames as u64);
    if riff + 8 > LIMIT {
        return Err("AVI のサイズが 2GB を超えます。フレーム数か画像サイズを減らしてください".into());
    }

    let mut g = Gather::new();
    for i in 0..n_frames {
        g.add(&fit(frame(i)?, w, h)?);
        progress(i as f32 / n_frames as f32 * 0.5);
    }
    let mut palette = g.palette();
    if palette.is_empty() {
        palette.push([0, 0, 0]);
    }

    let tmp = temp_sibling(path);
    let r = (|| -> Result<(), String> {
        let file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        let mut o = BufWriter::new(file);
        let mut head = Vec::with_capacity(1300);
        head.extend_from_slice(b"RIFF");
        push_u32(&mut head, riff as u32);
        head.extend_from_slice(b"AVI LIST");
        push_u32(&mut head, hdrl_list as u32);
        head.extend_from_slice(b"hdrlavih");
        push_u32(&mut head, 56);
        push_u32(&mut head, (1e6 / fps).round().clamp(1.0, u32::MAX as f64) as u32);
        push_u32(&mut head, (fsz as f64 * fps).min(u32::MAX as f64) as u32);
        push_u32(&mut head, 0);
        push_u32(&mut head, AVIF_HASINDEX);
        push_u32(&mut head, n_frames as u32);
        push_u32(&mut head, 0);
        push_u32(&mut head, 1);
        push_u32(&mut head, fsz as u32);
        push_u32(&mut head, w);
        push_u32(&mut head, h);
        for _ in 0..4 {
            push_u32(&mut head, 0);
        }
        head.extend_from_slice(b"LIST");
        push_u32(&mut head, (4 + (8 + 56) + (8 + STRF_SIZE)) as u32);
        head.extend_from_slice(b"strlstrh");
        push_u32(&mut head, 56);
        head.extend_from_slice(b"vidsDIB ");
        push_u32(&mut head, 0);
        push_u16(&mut head, 0);
        push_u16(&mut head, 0);
        push_u32(&mut head, 0);
        push_u32(&mut head, scale);
        push_u32(&mut head, rate);
        push_u32(&mut head, 0);
        push_u32(&mut head, n_frames as u32);
        push_u32(&mut head, fsz as u32);
        push_u32(&mut head, 0xFFFF_FFFF);
        push_u32(&mut head, 0);
        for n in [0, 0, w as u16, h as u16] {
            push_u16(&mut head, n);
        }
        head.extend_from_slice(b"strf");
        push_u32(&mut head, STRF_SIZE as u32);
        head.extend(super::info_header(w, h, 8, 0, fsz as u32, 256));
        for i in 0..256 {
            let c = palette.get(i).copied().unwrap_or([0, 0, 0]);
            head.extend_from_slice(&[c[2], c[1], c[0], 0]);
        }
        head.extend_from_slice(b"LIST");
        push_u32(&mut head, movi_list as u32);
        head.extend_from_slice(b"movi");
        o.write_all(&head).map_err(|e| e.to_string())?;

        let mut mapper = Mapper::new(&palette);
        let mut row = vec![0u8; stride as usize];
        for i in 0..n_frames {
            let rgba = fit(frame(i)?, w, h)?;
            o.write_all(b"00db").map_err(|e| e.to_string())?;
            o.write_all(&(fsz as u32).to_le_bytes()).map_err(|e| e.to_string())?;
            for j in (0..h as usize).rev() {
                let src = &rgba[j * w as usize * 4..(j + 1) * w as usize * 4];
                for (d, p) in row.iter_mut().zip(src.chunks_exact(4)) {
                    *d = mapper.index([p[0], p[1], p[2]]);
                }
                o.write_all(&row).map_err(|e| e.to_string())?;
            }
            progress(0.5 + (i + 1) as f32 / n_frames as f32 * 0.5);
        }

        let mut idx = Vec::with_capacity(8 + 16 * n_frames);
        idx.extend_from_slice(b"idx1");
        push_u32(&mut idx, 16 * n_frames as u32);
        for i in 0..n_frames as u64 {
            idx.extend_from_slice(b"00db");
            push_u32(&mut idx, AVIIF_KEYFRAME);
            push_u32(&mut idx, (4 + i * chunk) as u32);
            push_u32(&mut idx, fsz as u32);
        }
        o.write_all(&idx).map_err(|e| e.to_string())?;
        o.flush().map_err(|e| e.to_string())?;
        Ok(())
    })();
    let r = r.and_then(|_| std::fs::rename(&tmp, path).map_err(|e| e.to_string()));
    if let Err(e) = r {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{}: {e}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_at(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
    }

    fn solid(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> (Vec<u8>, u32, u32) {
        let mut v = Vec::new();
        for j in 0..h {
            for i in 0..w {
                let c = f(i, j);
                v.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        (v, w, h)
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("avese_{}_{name}", std::process::id()))
    }

    #[test]
    fn chunk_walk_and_pixels() {
        let path = tmp("walk.avi");
        let mut prog = Vec::new();
        // width 5 forces row padding; the second frame has another size and is resized
        let colour = |k: usize| [(k * 80) as u8, 10, 200];
        write_avi(&path, 3, 5, 4, 2.0, |k| Ok(if k == 1 { solid(10, 8, |_, _| colour(1)) } else { solid(5, 4, |_, _| colour(k)) }), |p| prog.push(p)).unwrap();
        let b = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(!temp_sibling(&path).exists());
        assert!(prog.windows(2).all(|w| w[0] <= w[1]) && *prog.last().unwrap() == 1.0);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(u32_at(&b, 4) as usize + 8, b.len());
        assert_eq!(&b[8..12], b"AVI ");
        assert_eq!(&b[12..16], b"LIST");
        let hdrl = u32_at(&b, 16) as usize;
        assert_eq!(&b[20..24], b"hdrl");
        assert_eq!(&b[24..28], b"avih");
        assert_eq!(u32_at(&b, 32), 500_000, "dwMicroSecPerFrame");
        assert_eq!(u32_at(&b, 44) & AVIF_HASINDEX, AVIF_HASINDEX);
        assert_eq!(u32_at(&b, 48), 3, "dwTotalFrames");
        assert_eq!((u32_at(&b, 64), u32_at(&b, 68)), (5, 4), "dwWidth, dwHeight");
        // strh: scale / rate / length
        let strh = 20 + 4 + 64 + 12;
        assert_eq!(&b[strh..strh + 4], b"strh");
        assert_eq!((u32_at(&b, strh + 28), u32_at(&b, strh + 32), u32_at(&b, strh + 40)), (1, 2, 3));
        let lp = 20 + hdrl;
        assert_eq!(&b[lp..lp + 4], b"LIST");
        assert_eq!(&b[lp + 8..lp + 12], b"movi");
        let movi = lp + 8;
        let movi_end = movi + u32_at(&b, lp + 4) as usize;
        let mut o = movi + 4;
        let mut starts = Vec::new();
        while o < movi_end {
            assert_eq!(&b[o..o + 4], b"00db");
            let sz = u32_at(&b, o + 4) as usize;
            assert_eq!(sz, 8 * 4);
            assert_eq!(sz % 2, 0);
            starts.push(o);
            o += 8 + sz;
        }
        assert_eq!(o, movi_end);
        assert_eq!(&b[o..o + 4], b"idx1");
        assert_eq!(u32_at(&b, o + 4), 48);
        for (i, s) in starts.iter().enumerate() {
            let e = o + 8 + i * 16;
            assert_eq!(&b[e..e + 4], b"00db");
            assert_eq!(u32_at(&b, e + 4), AVIIF_KEYFRAME);
            assert_eq!(movi + u32_at(&b, e + 8) as usize, *s);
            assert_eq!(u32_at(&b, e + 12), 32);
        }
        assert_eq!(o + 8 + 48, b.len());
        // palette is exact: frame 2 pixel maps back to its colour
        let pal = movi_palette(&b);
        let px = b[starts[2] + 8] as usize;
        assert_eq!(pal[px], colour(2));
    }

    fn movi_palette(b: &[u8]) -> Vec<[u8; 3]> {
        let strf = 20 + 4 + 64 + 12 + 64 + 8;
        (0..256).map(|i| [b[strf + 40 + i * 4 + 2], b[strf + 40 + i * 4 + 1], b[strf + 40 + i * 4]]).collect()
    }

    #[test]
    fn many_colours_share_a_quantized_palette() {
        let path = tmp("many.avi");
        write_avi(&path, 2, 64, 64, 10.0, |k| Ok(solid(64, 64, |i, j| [(i * 4) as u8, (j * 4) as u8, (k * 100) as u8])), |_| {}).unwrap();
        let b = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(u32_at(&b, 48), 2);
        assert_eq!(u32_at(&b, 4) as usize + 8, b.len());
    }

    #[test]
    fn errors() {
        let p = tmp("never.avi");
        let f = |_| Ok(solid(2, 2, |_, _| [0, 0, 0]));
        assert!(write_avi(&p, 0, 2, 2, 5.0, f, |_| {}).is_err());
        assert!(write_avi(&p, 2, 2, 2, 0.0, f, |_| {}).is_err());
        let e = write_avi(&p, 5000, 4000, 4000, 5.0, |_| Err("called".into()), |_| {}).unwrap_err();
        assert!(e.contains("2GB"), "{e}");
        let e = write_avi(&p, 1, 2, 2, 5.0, |_| Err("render failed".into()), |_| {}).unwrap_err();
        assert_eq!(e, "render failed");
        assert!(!p.exists() && !temp_sibling(&p).exists());
    }

    #[test]
    fn rate_pairs() {
        assert_eq!(rate_pair(30.0).unwrap(), (1, 30));
        assert_eq!(rate_pair(29.97).unwrap(), (100, 2997));
        assert_eq!(rate_pair(0.5).unwrap(), (2, 1));
    }
}
