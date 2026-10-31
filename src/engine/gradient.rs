// 要確認: 原作の色（Smooth2/3, Spectral1/3 は近似）
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientKind {
    Smooth1, Smooth2, Smooth3,
    Gray,
    Spectral1, Spectral2, Spectral3,
}

#[derive(Clone, Debug)]
pub struct Gradient {
    kind: GradientKind,
    n: usize,
    reverse: bool,
    fold: bool,
    palette: Vec<u32>,
}

impl Gradient {
    pub fn new(kind: GradientKind, n_colors: usize, reverse: bool, fold: bool) -> Self {
        let mut n = n_colors.clamp(2, 200);
        // base palette size n
        let palette = build_palette(kind, n);
        let mut g = Gradient { kind, n, reverse, fold, palette };
        if reverse {
            g.palette.reverse();
        }
        g
    }
    pub fn color(&self, t: f64) -> u32 {
        let mut t = t.clamp(0.0, 1.0);
        if self.fold {
            t = 1.0 - (2.0 * t - 1.0).abs();
        }
        let idx = if self.n <= 1 { 0 } else {
            (t * (self.n - 1) as f64).round() as usize
        };
        let idx = idx.min(self.n - 1);
        self.palette[idx]
    }
    pub fn palette(&self) -> &[u32] { &self.palette }
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> u32 {
    let c = v * s;
    let hh = h / 60.0;
    let x = c * (1.0 - (hh % 2.0 - 1.0).abs());
    let (r1,g1,b1) = match hh as i32 {
        0 => (c,x,0.0),
        1 => (x,c,0.0),
        2 => (0.0,c,x),
        3 => (0.0,x,c),
        4 => (x,0.0,c),
        _ => (c,0.0,x),
    };
    let m = v - c;
    let r = ((r1+m)*255.0).round() as u32;
    let g = ((g1+m)*255.0).round() as u32;
    let b = ((b1+m)*255.0).round() as u32;
    (r<<16)|(g<<8)|b
}

fn build_palette(kind: GradientKind, n: usize) -> Vec<u32> {
    (0..n).map(|i| {
        let t = i as f64 / (n-1) as f64;
        match kind {
            GradientKind::Smooth1 => {
                // hue 240 -> 0
                let h = 240.0 * (1.0 - t);
                hsv_to_rgb(h, 1.0, 1.0)
            }
            GradientKind::Smooth2 => {
                let h = 240.0 * (1.0 - t) + 30.0 * t; // 近似
                hsv_to_rgb(h, 1.0, 1.0)
            }
            GradientKind::Smooth3 => {
                let h = 240.0 * (1.0 - t) * 0.8; // 近似
                hsv_to_rgb(h, 1.0, 1.0)
            }
            GradientKind::Gray => {
                let v = (t*255.0).round() as u32;
                (v<<16)|(v<<8)|v
            }
            GradientKind::Spectral1 => {
                // 要確認: 原作の色
                let h = 300.0 - 280.0*t; // 近似
                hsv_to_rgb(h.clamp(0.0,360.0), 1.0, 1.0)
            }
            GradientKind::Spectral2 => {
                let h = 240.0*(1.0-t) + 60.0*t; // 近似
                hsv_to_rgb(h, 0.9, 1.0)
            }
            GradientKind::Spectral3 => {
                let h = 360.0*(1.0-t); // 近似
                hsv_to_rgb(h, 0.85, 0.95)
            }
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoints_smooth1() {
        let g = Gradient::new(GradientKind::Smooth1, 5, false, false);
        // hue 240 -> blue-ish, 0 -> red
        assert_eq!(g.color(0.0) & 0x0000ff, 0xff); // blue channel high at t=0 (240 hue)
        assert_eq!(g.color(1.0) & 0xff0000, 0xff0000);
    }
    #[test]
    fn gray_mid() {
        let g = Gradient::new(GradientKind::Gray, 5, false, false);
        assert_eq!(g.color(0.5), 0x808080); // approx
    }
    #[test]
    fn reverse() {
        let a = Gradient::new(GradientKind::Gray, 4, false, false);
        let b = Gradient::new(GradientKind::Gray, 4, true, false);
        assert_eq!(a.color(0.0), b.color(1.0));
    }
    #[test]
    fn fold() {
        let g = Gradient::new(GradientKind::Gray, 5, false, true);
        assert_eq!(g.color(0.25), g.color(0.75));
    }
    #[test]
    fn clamp_n() {
        let g = Gradient::new(GradientKind::Gray, 1, false, false);
        assert_eq!(g.palette().len(), 2);
        let g2 = Gradient::new(GradientKind::Gray, 1000, false, false);
        assert_eq!(g2.palette().len(), 200);
    }
    #[test]
    fn palette_len() {
        let g = Gradient::new(GradientKind::Gray, 7, false, false);
        assert_eq!(g.palette().len(), 7);
    }
}