//! Entry points shared by the GUI and the command line: `load` and `render`.
use crate::engine::data::{self, DataFile, DataOptions};
use crate::engine::draw::{self, Probe};
use crate::engine::field::{self, Field};
use crate::engine::raster;
use crate::engine::scene::Scene;
use crate::engine::settings::Settings;
use crate::state::Store;
use std::path::{Path, PathBuf};
use tiny_skia::Pixmap;

pub struct Loaded {
    pub data: DataFile,
    pub field: Field,
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
}

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
    let opts = DataOptions::from_settings(s);
    let df = data::parse(bytes, &opts);
    build(df, s, opts, progress)
}

fn build(df: DataFile, s: &Settings, opts: DataOptions, progress: impl FnMut(f32)) -> Result<Loaded, String> {
    let (field, warnings) = field::build_with_progress(&df, s, progress)?;
    Ok(Loaded { data: df, field, warnings, path: None, opts, spline: s.spline })
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
    let b = draw::build_scene(&loaded.field, s, &loaded.data.labels)?;
    let pixmap = raster::rasterize(&b.scene);
    Ok(Rendered { scene: b.scene, pixmap, probe: b.probe })
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
        let Field::Panel(p) = &l.field else { panic!("not a panel") };
        assert_eq!((p.rows, p.cols), (2, 3));
        assert_eq!(p.z[0], Some(1.0));
        st.set_checked("CBtenchi", true);
        let l = load(src, &mut st, None, |_| {}).unwrap();
        let Field::Panel(p) = &l.field else { panic!("not a panel") };
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
