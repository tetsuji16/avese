//! Builds the `Scene` for a field: image size, margins, drawing order.
pub mod axes;
pub mod land;
pub mod legend;
pub mod plane;

use crate::engine::data::Label;
use crate::engine::field::{Cylinder, Field};
use crate::engine::gradient::{Gradient, GradientKind};
use crate::engine::raster::{anchor_origin, measure_text};
use crate::engine::scene::{Anchor, FontRef, Op, Rgb, Scene};
use crate::engine::settings::Settings;

const MAX_SIDE: f64 = 16384.0;
const MAX_PIXELS: f64 = 2.0e8;

/// Mapping between data coordinates and image pixels for flat (non-3D) views.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// top-left corner and size of the plot area in pixels
    pub ox: f64,
    pub oy: f64,
    pub w: f64,
    pub h: f64,
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    /// panel data: y grows downwards
    pub flip_y: bool,
    pub cyl: Option<Cylinder>,
}

impl Frame {
    fn span(a: f64, b: f64) -> f64 {
        if (b - a).abs() < 1e-300 { 1.0 } else { b - a }
    }

    /// Pixels per data unit of r in cylindrical views.
    pub fn cyl_scale(&self) -> f64 {
        match &self.cyl {
            Some(c) => self.w / (2.0 * Self::span(c.r_min, c.r_max).abs()),
            None => 1.0,
        }
    }

    pub fn centre(&self) -> (f64, f64) {
        (self.ox + self.w / 2.0, self.oy + self.h / 2.0)
    }

    /// Data (x, y) or cylindrical (theta, r) to image pixels.
    pub fn to_px(&self, x: f64, y: f64) -> (f32, f32) {
        if let Some(c) = &self.cyl {
            let k = self.cyl_scale();
            let (dx, dy) = c.to_plane(x, y);
            let (cx, cy) = self.centre();
            return ((cx + dx * k) as f32, (cy - dy * k) as f32);
        }
        let u = (x - self.x0) / Self::span(self.x0, self.x1);
        let v = (y - self.y0) / Self::span(self.y0, self.y1);
        let v = if self.flip_y { v } else { 1.0 - v };
        ((self.ox + u * self.w) as f32, (self.oy + v * self.h) as f32)
    }

    /// Image pixels to data coordinates, without a range check.
    pub fn from_px_raw(&self, px: f64, py: f64) -> Option<(f64, f64)> {
        if let Some(c) = &self.cyl {
            let k = self.cyl_scale();
            let (cx, cy) = self.centre();
            let (dx, dy) = ((px - cx) / k, (cy - py) / k);
            if dx.hypot(dy) > Self::span(c.r_min, c.r_max).abs() {
                return None;
            }
            return Some(c.to_data(dx, dy));
        }
        let u = (px - self.ox) / self.w;
        let v = (py - self.oy) / self.h;
        let v = if self.flip_y { v } else { 1.0 - v };
        Some((self.x0 + u * Self::span(self.x0, self.x1), self.y0 + v * Self::span(self.y0, self.y1)))
    }

    /// Image pixels to data coordinates; None outside the plot area.
    pub fn from_px(&self, px: f64, py: f64) -> Option<(f64, f64)> {
        if px < self.ox || py < self.oy || px >= self.ox + self.w || py >= self.oy + self.h {
            return None;
        }
        self.from_px_raw(px, py)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProbeHit {
    /// data x (theta for cylindrical views)
    pub x: f64,
    /// data y (r for cylindrical views)
    pub y: f64,
    pub z: Option<f64>,
}

/// Maps image pixels back to data coordinates; empty for 3D views.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Probe {
    frame: Option<Frame>,
}

impl Probe {
    pub fn is_flat(&self) -> bool {
        self.frame.is_some()
    }

    pub fn frame(&self) -> Option<&Frame> {
        self.frame.as_ref()
    }

    pub fn at(&self, px: f64, py: f64) -> Option<ProbeHit> {
        let (x, y) = self.frame.as_ref()?.from_px(px, py)?;
        Some(ProbeHit { x, y, z: None })
    }

    /// Like `at`, also looking up the value in `field`.
    pub fn at_field(&self, px: f64, py: f64, field: &Field) -> Option<ProbeHit> {
        let mut hit = self.at(px, py)?;
        hit.z = field.sample(hit.x, hit.y);
        Some(hit)
    }
}

pub struct BuiltScene {
    pub scene: Scene,
    pub probe: Probe,
}

pub struct Ctx<'a> {
    pub field: &'a Field,
    pub s: &'a Settings,
    pub grad: Gradient,
    pub zr: Option<(f64, f64)>,
}

impl<'a> Ctx<'a> {
    pub fn new(field: &'a Field, s: &'a Settings) -> Ctx<'a> {
        Ctx { field, s, grad: Gradient::new(GradientKind::from_index(s.gradient_type), s.n_colors, s.reverse, s.fold), zr: field.z_range(s) }
    }

    pub fn t_of(&self, z: f64) -> f64 {
        let (lo, hi) = self.zr.unwrap_or((0.0, 1.0));
        (z - lo) / (hi - lo)
    }

    pub fn color(&self, z: f64) -> Rgb {
        self.grad.color(self.t_of(z))
    }
}

pub fn use_cylinder(field: &Field, s: &Settings) -> Option<Cylinder> {
    if s.cylindrical { field.cylinder() } else { None }
}

pub(crate) struct Layout {
    pub w: u32,
    pub h: u32,
    pub px: f64,
    pub py: f64,
    pub pw: f64,
    pub ph: f64,
    pub legend_y: f64,
    pub proj: Option<land::Proj>,
}

/// Plot area size before margins, ignoring the 3D projection.
fn flat_dims(field: &Field, s: &Settings) -> (f64, f64) {
    let size = s.size.max(1) as f64;
    if use_cylinder(field, s).is_some() {
        return (size, size);
    }
    let (x0, x1, y0, y1) = field.extent().unwrap_or((0.0, 1.0, 0.0, 1.0));
    let (xr, yr) = ((x1 - x0).abs().max(1e-300), (y1 - y0).abs().max(1e-300));
    if s.size_by_height {
        (size * xr / yr * s.scale_h, size * s.scale_v)
    } else {
        (size * s.scale_h, size * yr / xr * s.scale_v)
    }
}

pub(crate) fn legend_extra(s: &Settings) -> f64 {
    if !s.legend {
        return 0.0;
    }
    let (_, th) = measure_text("0", s.legend_font());
    4.0 + s.legend_h as f64 + th.ceil() as f64 + 6.0
}

impl Layout {
    pub fn new(field: &Field, s: &Settings) -> Result<Layout, String> {
        let m = s.margin as f64;
        let (pw, ph, proj) = if s.land {
            let (wd, hd) = flat_dims(field, s);
            let p = land::Proj::new(s.eta, s.phi, wd, hd, s.land_height.max(1) as f64);
            (p.w.ceil().max(1.0), p.h.ceil().max(1.0), Some(p))
        } else {
            let (a, b) = flat_dims(field, s);
            (a.round().max(1.0), b.round().max(1.0), None)
        };
        let extra = legend_extra(s);
        let w = (pw + 2.0 * m).max(if s.legend { s.legend_w as f64 + 8.0 } else { 0.0 });
        let h = ph + 2.0 * m + extra;
        if !(w <= MAX_SIDE && h <= MAX_SIDE) || w * h > MAX_PIXELS {
            return Err(format!("画像が大きすぎます ({} x {})", w.min(1e9) as u64, h.min(1e9) as u64));
        }
        let px = ((w - pw) / 2.0).floor();
        Ok(Layout { w: w as u32, h: h as u32, px, py: m, pw, ph, legend_y: m + ph + m + 4.0, proj })
    }

    pub fn frame(&self, field: &Field, s: &Settings) -> Option<Frame> {
        let (x0, x1, y0, y1) = field.extent()?;
        Some(Frame {
            ox: self.px,
            oy: self.py,
            w: self.pw,
            h: self.ph,
            x0,
            x1,
            y0,
            y1,
            flip_y: matches!(field, Field::Panel(_)),
            cyl: use_cylinder(field, s),
        })
    }
}

/// Image size in pixels for the field and settings.
pub fn image_size(field: &Field, s: &Settings) -> Result<(u32, u32), String> {
    let l = Layout::new(field, s)?;
    Ok((l.w, l.h))
}

/// Adds a text op, shifted when needed so that it stays inside the image.
pub(crate) fn push_text(ops: &mut Vec<Op>, bounds: (u32, u32), x: f32, y: f32, text: &str, font: &FontRef, color: Rgb, anchor: Anchor) {
    if text.is_empty() {
        return;
    }
    let (w, h) = measure_text(text, font);
    let (mut l, mut t) = anchor_origin(anchor, x, y, w, h);
    let (bw, bh) = (bounds.0 as f32, bounds.1 as f32);
    if w <= bw {
        l = l.clamp(0.0, bw - w);
    }
    if h <= bh {
        t = t.clamp(0.0, bh - h);
    }
    ops.push(Op::Text { x: l, y: t, text: text.to_string(), font: font.clone(), color, anchor: Anchor::TopLeft });
}

pub fn build_scene(field: &Field, s: &Settings, labels: &[Label]) -> Result<BuiltScene, String> {
    build_scene_with_progress(field, s, labels, |_| true)
}

/// Build a scene while reporting coarse phase progress. Returning false cancels the build.
pub fn build_scene_with_progress(
    field: &Field,
    s: &Settings,
    labels: &[Label],
    mut progress: impl FnMut(f32) -> bool,
) -> Result<BuiltScene, String> {
    const CANCELLED: &str = "__AVese_RENDER_CANCELLED__";
    let lay = Layout::new(field, s)?;
    if !progress(0.0) { return Err(CANCELLED.into()); }
    let ctx = Ctx::new(field, s);
    let mut ops = Vec::new();
    let mut probe = Probe::default();
    if s.land {
        if !land::draw_with_progress(&ctx, &lay, &mut ops, |p| progress(p * 0.8)) { return Err(CANCELLED.into()); }
    } else if let Some(frame) = lay.frame(field, s) {
        let is_panel = matches!(field, Field::Panel(_));
        let scatter = s.scatter && !is_panel;
        if scatter {
            plane::scatter(&ctx, &frame, &mut ops);
        } else {
            if !plane::image_with_progress(&ctx, &frame, &mut ops, |p| progress(p * 0.35)) { return Err(CANCELLED.into()); }
            if !is_panel && s.toko {
                if !plane::contours_with_progress(&ctx, &frame, &mut ops, |p| progress(0.35 + p * 0.25)) { return Err(CANCELLED.into()); }
            }
        }
        if !progress(0.6) { return Err(CANCELLED.into()); }
        let bounds = (lay.w, lay.h);
        axes::grid(&ctx, &frame, bounds, &mut ops);
        if !is_panel && s.vector {
            if !plane::vectors_with_progress(&ctx, &frame, &mut ops, |p| progress(0.6 + p * 0.2)) { return Err(CANCELLED.into()); }
        }
        axes::axes(&ctx, &frame, bounds, &mut ops);
        axes::points(&ctx, &frame, &mut ops);
        probe = Probe { frame: Some(frame) };
    }
    if !progress(0.85) { return Err(CANCELLED.into()); }
    legend::draw(&ctx, &lay, &mut ops);
    if s.label {
        for l in labels {
            if l.x.is_finite() && l.y.is_finite() {
                ops.push(Op::Text { x: l.x as f32, y: l.y as f32, text: l.text.clone(), font: s.label_font().clone(), color: s.fg, anchor: Anchor::TopLeft });
            }
        }
    }
    if !progress(1.0) { return Err(CANCELLED.into()); }
    Ok(BuiltScene { scene: Scene { width: lay.w, height: lay.h, bg: s.bg, ops }, probe })
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use crate::engine::data::{DataFile, Point};
    use crate::engine::field;

    pub fn settings() -> Settings {
        let mut s = Settings::default();
        s.size = 100;
        s.size_by_height = false;
        s.margin = 10;
        s.legend = false;
        s.grid = false;
        s.axis = false;
        s.toko = false;
        s.vector = false;
        s.point = false;
        s.land = false;
        s.scatter = false;
        s.panel = false;
        s.cylindrical = false;
        s.cartesian = true;
        s.label = false;
        s.scale_h = 1.0;
        s.scale_v = 1.0;
        s
    }

    pub fn grid(nx: usize, ny: usize, f: impl Fn(f64, f64) -> Option<f64>) -> Field {
        let mut df = DataFile::default();
        for j in 0..ny {
            for i in 0..nx {
                let (x, y) = (i as f64, j as f64);
                df.points.push(Point { x: Some(x), y: Some(y), z: f(x, y), u: None, v: None });
            }
        }
        field::build(&df, &settings()).unwrap().0
    }

    pub fn grid_uv(nx: usize, ny: usize, f: impl Fn(f64, f64) -> (f64, f64)) -> Field {
        let mut df = DataFile::default();
        for j in 0..ny {
            for i in 0..nx {
                let (x, y) = (i as f64, j as f64);
                let (u, v) = f(x, y);
                df.points.push(Point { x: Some(x), y: Some(y), z: Some(x + y), u: Some(u), v: Some(v) });
            }
        }
        field::build(&df, &settings()).unwrap().0
    }

    pub fn count_ops(ops: &[Op], f: impl Fn(&Op) -> bool) -> usize {
        ops.iter().filter(|o| f(o)).count()
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    #[test]
    fn size_follows_aspect_and_margin() {
        let f = grid(11, 6, |x, y| Some(x + y)); // 10 x 5 units
        let s = settings();
        assert_eq!(image_size(&f, &s).unwrap(), (120, 70));
        let mut s2 = settings();
        s2.size_by_height = true;
        assert_eq!(image_size(&f, &s2).unwrap(), (220, 120));
        let mut s3 = settings();
        s3.scale_h = 2.0;
        assert_eq!(image_size(&f, &s3).unwrap(), (220, 70));
    }

    #[test]
    fn cylindrical_is_square() {
        let f = grid(9, 5, |x, y| Some(x + y));
        let mut s = settings();
        s.cylindrical = true;
        s.scale_h = 3.0;
        let (w, h) = image_size(&f, &s).unwrap();
        assert_eq!(w, h);
        assert_eq!(w, 120);
    }

    #[test]
    fn legend_adds_height() {
        let f = grid(5, 5, |x, y| Some(x + y));
        let a = image_size(&f, &settings()).unwrap();
        let mut s = settings();
        s.legend = true;
        let b = image_size(&f, &s).unwrap();
        assert_eq!(a.0, b.0.min(a.0));
        assert!(b.1 > a.1 + s.legend_h);
    }

    #[test]
    fn huge_size_is_an_error() {
        let f = grid(3, 3, |x, y| Some(x + y));
        let mut s = settings();
        s.size = 100_000;
        assert!(build_scene(&f, &s, &[]).is_err());
    }

    #[test]
    fn probe_round_trip() {
        let f = grid(11, 6, |x, y| Some(x + y));
        let b = build_scene(&f, &settings(), &[]).unwrap();
        let hit = b.probe.at(10.5, 10.5).unwrap(); // top-left of the plot: x min, y max
        assert!(hit.x < 0.1 && hit.y > 4.9);
        assert!(b.probe.at(2.0, 2.0).is_none());
        assert!(b.probe.at_field(60.0, 35.0, &f).unwrap().z.is_some());
    }

    #[test]
    fn cylindrical_centre_is_r_min() {
        let f = grid(9, 5, |x, y| Some(x + y)); // r from 0 to 4
        let mut s = settings();
        s.cylindrical = true;
        let b = build_scene(&f, &s, &[]).unwrap();
        let c = b.scene.width as f64 / 2.0;
        let hit = b.probe.at(c, c).unwrap();
        assert!(hit.y < 0.1);
        let edge = b.probe.at(c + 49.0, c).unwrap();
        assert!(edge.y > 3.8);
    }
}
