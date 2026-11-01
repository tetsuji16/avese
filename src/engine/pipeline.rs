//! Entry points shared by the GUI and the command line: `load` and `render`.
use crate::engine::data::{self, DataFile, DataOptions};
use crate::engine::draw::{self, Probe};
use crate::engine::field::{self, Field};
use crate::engine::raster;
use crate::engine::scene::Scene;
use crate::engine::settings::Settings;
use crate::state::Store;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tiny_skia::Pixmap;

pub struct Loaded {
    pub data: DataFile,
    pub field: Arc<Field>,
    pub warnings: Vec<String>,
    pub path: Option<PathBuf>,
    /// settings the field was built from; see `is_stale`
    pub opts: DataOptions,
    pub spline: bool,
}

pub struct Rendered {
    pub scene: Scene,
    pub pixmap: Pixmap,
    pub probe: Probe,
    /// The field used for this image, even if a later render is cancelled.
    pub field: Arc<Field>,
}

pub const RENDER_CANCELLED: &str = "__AVese_RENDER_CANCELLED__";

impl Rendered {
    /// RGBA bytes for egui (`ColorImage::from_rgba_premultiplied`); the pixmap is opaque, so
    /// premultiplied and straight alpha agree.
    pub fn rgba(&self) -> Vec<u8> {
        self.pixmap.data().to_vec()
    }

    pub fn size(&self) -> [usize; 2] {
        [self.pixmap.width() as usize, self.pixmap.height() as usize]
    }
}

impl Loaded {
    /// True when `s` changes how the data are read or interpolated, so the bytes must be parsed again.
    pub fn is_stale(&self, s: &Settings) -> bool {
        self.opts != DataOptions::from_settings(s) || self.spline != s.spline
    }
}

/// Parse and build the field with fixed settings (no embedded settings are applied).
pub fn reparse(bytes: &[u8], s: &Settings, progress: impl FnMut(f32)) -> Result<Loaded, String> {
    reparse_with_mesh(bytes, s, false, progress)
}

/// Parse held bytes and optionally build a mesh even when the source file did not
/// contain the `Mesh AutoCreate` directive.
pub fn reparse_with_mesh(bytes: &[u8], s: &Settings, mesh_auto: bool, progress: impl FnMut(f32)) -> Result<Loaded, String> {
    let opts = DataOptions::from_settings(s);
    let mut df = data::parse(bytes, &opts);
    df.mesh_auto |= mesh_auto;
    build(df, s, opts, progress)
}

fn build(df: DataFile, s: &Settings, opts: DataOptions, progress: impl FnMut(f32)) -> Result<Loaded, String> {
    let (field, warnings) = field::build_with_progress(&df, s, progress)?;
    Ok(Loaded { data: df, field: Arc::new(field), warnings, path: None, opts, spline: s.spline })
}

/// Read a data file: parse, apply embedded settings and IncludeINI to `store`, build the field.
/// The data are parsed again when those settings change how they are read.
pub fn load(bytes: &[u8], store: &mut Store, base_dir: Option<&Path>, progress: impl FnMut(f32)) -> Result<Loaded, String> {
    let s0 = Settings::from_store(store);
    let opts0 = DataOptions::from_settings(&s0);
    let mut df = data::parse(bytes, &opts0);
    let mut warnings = data::apply_embedded(store, &df, base_dir);
    let s = Settings::from_store(store);
    let opts = DataOptions::from_settings(&s);
    if opts != opts0 {
        df = data::parse(bytes, &opts);
    }
    let mut l = build(df, &s, opts, progress)?;
    warnings.append(&mut l.warnings);
    l.warnings = warnings;
    Ok(l)
}

/// Size of the image in bytes (width x height), for the SEmaxbmp warning; 0 if the size is invalid.
pub fn required_bytes(loaded: &Loaded, s: &Settings) -> u64 {
    draw::image_size(&loaded.field, s).map(|(w, h)| w as u64 * h as u64).unwrap_or(0)
}

pub fn render(loaded: &Loaded, s: &Settings) -> Result<Rendered, String> {
    render_with_progress(loaded, s, |_| true)
}

/// Render while reporting normalized progress. Returning false aborts and returns the
/// distinguished cancellation error used by the job layer.
pub fn render_with_progress(loaded: &Loaded, s: &Settings, mut progress: impl FnMut(f32) -> bool) -> Result<Rendered, String> {
    let b = draw::build_scene_with_progress(&loaded.field, s, &loaded.data.labels, |p| progress(p * 0.5))
        .map_err(|e| if e == RENDER_CANCELLED { RENDER_CANCELLED.to_owned() } else { e })?;
    let pixmap = raster::rasterize_with_progress(&b.scene, |p| progress(0.5 + p * 0.5)).ok_or(RENDER_CANCELLED)?;
    Ok(Rendered { scene: b.scene, pixmap, probe: b.probe, field: Arc::clone(&loaded.field) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Settings::default_store()
    }

    fn text(n: usize) -> String {
        let mut t = String::new();
        for y in 0..n {
            for x in 0..n {
                t += &format!("{} {} {}\n", x, y, x + y * n);
            }
        }
        t
    }

    fn small(st: &Store) -> Settings {
        let mut s = Settings::from_store(st);
        s.size = 20;
        s.size_by_height = false;
        s.legend = false;
        s.margin = 2;
        s.scale_h = 1.0;
        s.scale_v = 1.0;
        s
    }

    #[test]
    fn load_and_render() {
        let mut st = store();
        let l = load(text(3).as_bytes(), &mut st, None, |_| {}).unwrap();
        let s = small(&st);
        let r = render(&l, &s).unwrap();
        assert_eq!(r.size(), [24, 24]);
        assert_eq!(r.rgba().len(), 24 * 24 * 4);
        assert_eq!(required_bytes(&l, &s), 24 * 24);
        assert!(r.probe.at(12.0, 12.0).is_some());
        assert!(r.pixmap.data().chunks(4).all(|p| p[3] == 255));
    }

    #[test]
    fn cancelled_new_render_keeps_original_probe_data() {
        let mut st = store();
        let old = load(text(3).as_bytes(), &mut st, None, |_| {}).unwrap();
        let s = small(&st);
        let image = render(&old, &s).unwrap();
        let before = image.probe.at_field(12.0, 12.0, &image.field).unwrap();
        assert!(before.z.is_some());
        let new_text = (0..3).flat_map(|y| (0..3).map(move |x| format!("{x} {y} 1000\n"))).collect::<String>();
        let new = load(new_text.as_bytes(), &mut st, None, |_| {}).unwrap();
        assert_ne!(before.z, image.probe.at_field(12.0, 12.0, &new.field).unwrap().z);
        assert!(matches!(render_with_progress(&new, &s, |_| false), Err(e) if e == RENDER_CANCELLED));
        drop(old);
        assert_eq!(image.probe.at_field(12.0, 12.0, &image.field), Some(before));
    }

    #[test]
    fn forced_mesh_reparse_survives_settings_changes() {
        let mut st = store();
        let bytes = text(3);
        let s = small(&st);
        let original = reparse(bytes.as_bytes(), &s, |_| {}).unwrap();
        assert!(matches!(&*original.field, Field::Grid(_)));
        let mesh = reparse_with_mesh(bytes.as_bytes(), &s, true, |_| {}).unwrap();
        assert!(mesh.data.mesh_auto);
        assert!(matches!(&*mesh.field, Field::Mesh(_)));
        let mut changed = s;
        changed.spline = !changed.spline;
        let reparsed = reparse_with_mesh(bytes.as_bytes(), &changed, mesh.data.mesh_auto, |_| {}).unwrap();
        assert!(matches!(&*reparsed.field, Field::Mesh(_)));
    }

    #[test]
    fn rendering_reports_progress_and_cancels_without_a_partial_result() {
        let mut st = store();
        let l = load(text(3).as_bytes(), &mut st, None, |_| {}).unwrap();
        let s = small(&st);
        let mut last = 0.0;
        let result = render_with_progress(&l, &s, |p| {
            assert!(p >= last, "progress regressed from {last} to {p}");
            last = p;
            p < 0.5
        });
        let e = result.err().expect("cancellation must not return an image");
        assert_eq!(e, RENDER_CANCELLED);
        assert!(last >= 0.5);

        let mut land = s.clone();
        land.land = true;
        let mut last = 0.0;
        render_with_progress(&l, &land, |p| {
            assert!(p >= last, "land progress regressed from {last} to {p}");
            last = p;
            true
        }).unwrap();
        assert_eq!(last, 1.0);
    }

    #[test]
    fn irregular_data_warns() {
        let mut st = store();
        st.set_checked("CBDataCheck", true);
        let l = load(b"0 0 1\n1 0 2\n2 0 3\n0 1 4\n1 1 5\n", &mut st, None, |_| {}).unwrap();
        assert!(l.warnings.iter().any(|w| w.contains("格子状")));
    }

    #[test]
    fn empty_data_is_an_error() {
        let mut st = store();
        assert!(load(b"", &mut st, None, |_| {}).is_err());
    }

    #[test]
    fn stale_when_columns_change() {
        let mut st = store();
        let l = load(text(3).as_bytes(), &mut st, None, |_| {}).unwrap();
        let mut s = Settings::from_store(&st);
        assert!(!l.is_stale(&s));
        s.col_z = 2;
        assert!(l.is_stale(&s));
    }

    #[test]
    fn panel_rows_start_at_the_top_and_transpose() {
        let mut st = store();
        st.set_checked("CBpanel", true);
        let src = b"1 2 3\n4 5 6\n";
        let l = load(src, &mut st, None, |_| {}).unwrap();
        let Field::Panel(p) = &*l.field else { panic!("not a panel") };
        assert_eq!((p.rows, p.cols), (2, 3));
        assert_eq!(p.z[0], Some(1.0));
        st.set_checked("CBtenchi", true);
        let l = load(src, &mut st, None, |_| {}).unwrap();
        let Field::Panel(p) = &*l.field else { panic!("not a panel") };
        assert_eq!((p.rows, p.cols), (3, 2));
        assert_eq!(p.z[1], Some(4.0));
        // the first row of the data is the top row of the image
        let mut st2 = store();
        st2.set_checked("CBpanel", true);
        let l = load(src, &mut st2, None, |_| {}).unwrap();
        let mut s = small(&st2);
        s.panel = true;
        s.margin = 0;
        s.size = 6;
        let r = render(&l, &s).unwrap();
        let px = |x: u32, y: u32| {
            let p = r.pixmap.pixel(x, y).unwrap();
            (p.red(), p.green(), p.blue())
        };
        assert_ne!(px(0, 0), px(0, 3)); // 1 (min) at the top left, 4 below
    }

    #[test]
    fn huge_sizes_error_instead_of_allocating() {
        let mut st = store();
        let l = load(text(3).as_bytes(), &mut st, None, |_| {}).unwrap();
        let mut s = small(&st);
        s.size = 1_000_000;
        assert!(render(&l, &s).is_err());
        assert_eq!(required_bytes(&l, &s), 0);
    }

    #[test]
    fn original_samples_render() {
        let dir = Path::new("original avese");
        if !dir.exists() {
            return;
        }
        for n in 1..=6 {
            let p = dir.join(format!("sample{n}.txt"));
            let bytes = std::fs::read(&p).unwrap();
            let mut st = store();
            let l = load(&bytes, &mut st, Some(dir), |_| {}).unwrap_or_else(|e| panic!("sample{n}: {e}"));
            let r = render(&l, &Settings::from_store(&st)).unwrap_or_else(|e| panic!("sample{n}: {e}"));
            assert!(r.pixmap.width() > 10, "sample{n}");
        }
    }
}
