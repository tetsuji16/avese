//! The nine colour scales of the CBtype combo.
use crate::engine::scene::Rgb;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientKind {
    Smooth1,
    Smooth2,
    Smooth3,
    Gray1,
    Gray2,
    Gray3,
    Spectral1,
    Spectral2,
    Spectral3,
}

impl GradientKind {
    /// CBtype item index; out-of-range values fall back to Smooth1.
    pub fn from_index(i: u8) -> GradientKind {
        use GradientKind::*;
        [Smooth1, Smooth2, Smooth3, Gray1, Gray2, Gray3, Spectral1, Spectral2, Spectral3]
            .get(i as usize)
            .copied()
            .unwrap_or(Smooth1)
    }
}

#[derive(Clone, Debug)]
pub struct Gradient {
    palette: Vec<Rgb>,
}

impl Gradient {
    /// `n_colors` is clamped to 2..=200. Reverse and fold are baked into the palette.
    pub fn new(kind: GradientKind, n_colors: usize, reverse: bool, fold: bool) -> Gradient {
        let n = n_colors.clamp(2, 200);
        let mut base: Vec<Rgb> = (0..n).map(|i| base_color(kind, i as f64 / (n - 1) as f64)).collect();
        if reverse {
            base.reverse();
        }
        let palette = (0..n)
            .map(|i| {
                let mut t = i as f64 / (n - 1) as f64;
                if fold {
                    t = 1.0 - (2.0 * t - 1.0).abs();
                }
                base[((t * (n - 1) as f64).round() as usize).min(n - 1)]
            })
            .collect();
        Gradient { palette }
    }

    /// Colour for t in 0..=1 (clamped; NaN is treated as 0).
    pub fn color(&self, t: f64) -> Rgb {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        let n = self.palette.len();
        self.palette[((t * (n - 1) as f64).round() as usize).min(n - 1)]
    }

    pub fn palette(&self) -> &[Rgb] {
        &self.palette
    }

    pub fn len(&self) -> usize {
        self.palette.len()
    }
}

fn hsv(h: f64, s: f64, v: f64) -> Rgb {
    let hh = (h.rem_euclid(360.0)) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (hh % 2.0 - 1.0).abs());
    let (r, g, b) = match hh as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let q = |f: f64| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb(q(r), q(g), q(b))
}

/// Google's published polynomial approximation of Turbo.
/// Copyright 2019 Google LLC, Apache-2.0. Colormap by Anton Mikhailov, approximation by Ruofei Du.
fn turbo(t: f64, sat: f64, val: f64) -> Rgb {
    const R: [f64; 6] = [0.13572138, 4.61539260, -42.66032258, 132.13108234, -152.94239396, 59.28637943];
    const G: [f64; 6] = [0.09140261, 2.19418839, 4.84296658, -14.18503333, 4.27729857, 2.82956604];
    const B: [f64; 6] = [0.10667330, 12.64194608, -60.58204836, 110.36276771, -89.90310912, 27.34824973];
    let t = t.clamp(0.0, 1.0);
    let q = |c: &[f64; 6]| {
        let v = c.iter().rev().fold(0.0, |acc, k| acc * t + k).clamp(0.0, 1.0);
        ((1.0 - sat * (1.0 - v)) * val * 255.0).round() as u8
    };
    Rgb(q(&R), q(&G), q(&B))
}

/// Variant 1 is fully saturated, 2 is darkened, 3 is pale.
fn tone(variant: u8) -> (f64, f64) {
    match variant {
        1 => (1.0, 1.0),
        2 => (1.0, 0.6),
        _ => (0.4, 1.0),
    }
}

fn base_color(kind: GradientKind, t: f64) -> Rgb {
    use GradientKind::*;
    let (family, variant) = match kind {
        Smooth1 => (0, 1),
        Smooth2 => (0, 2),
        Smooth3 => (0, 3),
        Gray1 => (1, 1),
        Gray2 => (1, 2),
        Gray3 => (1, 3),
        Spectral1 => (2, 1),
        Spectral2 => (2, 2),
        Spectral3 => (2, 3),
    };
    let (sat, val) = tone(variant);
    match family {
        0 => turbo(t, sat, val),
        // black to white; the dark variant tops out at mid grey, the pale one starts at mid grey
        1 => {
            let (lo, hi) = match variant {
                1 => (0.0, 1.0),
                2 => (0.0, 0.6),
                _ => (0.4, 1.0),
            };
            let g = ((lo + (hi - lo) * t) * 255.0).round() as u8;
            Rgb(g, g, g)
        }
        // violet through the whole rainbow to red
        _ => hsv(300.0 * (1.0 - t), sat, val),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smooth1_is_turbo() {
        let g = Gradient::new(GradientKind::Smooth1, 200, false, false);
        assert_eq!(g.color(0.0), Rgb(35, 23, 27));
        assert_eq!(g.color(1.0), Rgb(144, 13, 0));
        let mid = g.color(0.5);
        assert!(mid.1 > mid.0 && mid.1 > mid.2 && mid.1 > 200);
        let (blue, red) = (g.color(0.15), g.color(0.85));
        assert!(blue.2 > blue.0 && blue.2 > 200);
        assert!(red.0 > red.2 && red.0 > 200);
    }

    #[test]
    fn variants_differ_in_brightness() {
        let sum = |k| {
            let c = Gradient::new(k, 11, false, false).color(0.5);
            c.0 as u32 + c.1 as u32 + c.2 as u32
        };
        for (a, b, c) in [
            (GradientKind::Smooth1, GradientKind::Smooth2, GradientKind::Smooth3),
            (GradientKind::Spectral1, GradientKind::Spectral2, GradientKind::Spectral3),
        ] {
            assert!(sum(b) < sum(a));
            assert!(sum(c) > sum(b));
        }
        let g = |k| Gradient::new(k, 11, false, false).color(1.0).0;
        assert!(g(GradientKind::Gray2) < g(GradientKind::Gray1));
        assert_eq!(Gradient::new(GradientKind::Gray3, 11, false, false).color(0.0).0, 102);
    }

    #[test]
    fn reverse_and_fold() {
        let a = Gradient::new(GradientKind::Gray1, 4, false, false);
        let b = Gradient::new(GradientKind::Gray1, 4, true, false);
        assert_eq!(a.color(0.0), b.color(1.0));
        let f = Gradient::new(GradientKind::Spectral1, 21, false, true);
        assert_eq!(f.color(0.0), f.color(1.0));
        assert_eq!(f.color(0.25), f.color(0.75));
    }

    #[test]
    fn clamps_and_bad_input() {
        assert_eq!(Gradient::new(GradientKind::Gray1, 1, false, false).palette().len(), 2);
        assert_eq!(Gradient::new(GradientKind::Gray1, 1000, false, false).palette().len(), 200);
        let g = Gradient::new(GradientKind::Gray1, 7, false, false);
        assert_eq!(g.color(f64::NAN), g.color(0.0));
        assert_eq!(g.color(9.0), g.color(1.0));
        assert_eq!(GradientKind::from_index(200), GradientKind::Smooth1);
        assert_eq!(GradientKind::from_index(5), GradientKind::Gray3);
    }
}
