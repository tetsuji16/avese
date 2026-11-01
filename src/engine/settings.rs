//! Typed view of the configuration controls. Field comments give the control name.
use crate::engine::scene::{FontRef, Rgb};
use crate::state::Store;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// CBmode: false = SEsize is the width, true = the height
    pub size_by_height: bool,
    pub size: u32,
    /// EDv, EDh: vertical / horizontal magnification
    pub scale_v: f64,
    pub scale_h: f64,
    /// CBland, SElandheight, SEeta, SEphi, CBsurface
    pub land: bool,
    pub land_height: i32,
    pub eta: i32,
    pub phi: i32,
    pub surface: bool,
    /// CBgraph, CBscatter, CBpanel
    pub graph: bool,
    pub scatter: bool,
    pub panel: bool,
    /// SEdotsize, CBscattemode (0 square, 1 disc, 2 hollow square, 3 hollow disc)
    pub dot_size: u32,
    pub scatter_mode: u8,
    /// SPvectorcolor, SPvectorheadcolor
    pub vector_color: Rgb,
    pub vector_head_color: Rgb,
    /// EDvector (percent of the reference length), EDhead (percent)
    pub vector_len: f64,
    pub head_len: f64,
    /// CBskipx, CBskipy, EDstepx, EDstepy, EDoffsetx, EDoffsety
    pub skip_x: bool,
    pub skip_y: bool,
    pub step_x: u32,
    pub step_y: u32,
    pub offset_x: u32,
    pub offset_y: u32,
    /// CBfix, CBpaint, SEvectorheadangle
    pub head_fix: bool,
    pub head_paint: bool,
    pub head_angle: i32,
    /// CBenVMax, EDenVMax
    pub vmax_on: bool,
    pub vmax: f64,
    /// CBvectorhead, CBvector, CBcolorvector
    pub vector_head: bool,
    pub vector: bool,
    pub color_vector: bool,
    /// CBtoko, SPtokocolor, EDdt, RBctmode1 (count), RBctmode2 (interval), SEtoko
    pub toko: bool,
    pub toko_color: Rgb,
    pub toko_dt: f64,
    pub toko_by_count: bool,
    pub toko_by_step: bool,
    pub toko_count: u32,
    /// CBentokomin, CBentokomax, EDtokomin, EDtokomax
    pub toko_min_on: bool,
    pub toko_max_on: bool,
    pub toko_min: f64,
    pub toko_max: f64,
    /// CBaxis, CBaxistitle, EDXtitle, EDYtitle, EDZtitle
    pub axis: bool,
    pub axis_title: bool,
    pub title_x: String,
    pub title_y: String,
    pub title_z: String,
    /// SEaxisheadsize, SEaxislinewidth, CBaxsiscross
    pub axis_head_size: u32,
    pub axis_line_width: u32,
    pub axis_cross: bool,
    /// RBcartesian, RBcylindrical
    pub cartesian: bool,
    pub cylindrical: bool,
    /// CBgrid, SEgridlinewidth, SEgridlinelength, CBgridmesh, CBgridbox, CBgridscale, CBaxisscale
    pub grid: bool,
    pub grid_line_width: u32,
    pub grid_line_length: i32,
    pub grid_mesh: bool,
    pub grid_box: bool,
    pub grid_scale: bool,
    pub axis_scale: bool,
    /// CBmemoryX/Y/Z: 0 auto, 1 x10, 2 x5, 3 x2, 4 standard, 5 x0.5, 6 x0.2, 7 x0.1
    pub memory_x: u8,
    pub memory_y: u8,
    pub memory_z: u8,
    /// CBpoint, CBLabel
    pub point: bool,
    pub label: bool,
    /// LBfont rows: label, legend, axis title, axis scale
    pub fonts: [FontRef; 4],
    /// SEdataX/Y/U/V/Z (1-based columns), CBtenchi (transpose), SEskip
    pub col_x: usize,
    pub col_y: usize,
    pub col_u: usize,
    pub col_v: usize,
    pub col_z: usize,
    pub transpose: bool,
    pub skip: usize,
    /// SPBGcolor, SPFGcolor, SEmar
    pub bg: Rgb,
    pub fg: Rgb,
    pub margin: u32,
    /// CBautosave, SEmaxbmp (MB), RBusebmp, RBusewmf
    pub autosave: bool,
    pub max_bmp_mb: u32,
    pub use_bmp: bool,
    pub use_wmf: bool,
    /// CBDataCheck, CBLoadConf, CBSpline
    pub data_check: bool,
    pub load_conf: bool,
    pub spline: bool,
    /// CBlegend, CBreverse, CBtype (0..9), CBfold, SEcolor, SElegendw, SElegendh
    pub legend: bool,
    pub reverse: bool,
    pub gradient_type: u8,
    pub fold: bool,
    pub n_colors: usize,
    pub legend_w: u32,
    pub legend_h: u32,
    /// CBenZmin, CBenZmax, EDenZmin, EDenZmax
    pub z_min_on: bool,
    pub z_max_on: bool,
    pub z_min: f64,
    pub z_max: f64,
}

fn num(store: &Store, name: &str, lo: i32, hi: i32, def: i32) -> i32 {
    match store.get(name) {
        Some(c) => c.position.clamp(lo, hi),
        None => def,
    }
}

fn real(store: &Store, name: &str, def: f64) -> f64 {
    match store.text(name).trim().parse::<f64>() {
        Ok(v) if v.is_finite() => v,
        _ => def,
    }
}

fn positive(store: &Store, name: &str, def: f64) -> f64 {
    let v = real(store, name, def);
    if v > 0.0 { v } else { def }
}

fn count(store: &Store, name: &str, lo: u32, hi: u32, def: u32) -> u32 {
    real(store, name, def as f64).round().clamp(lo as f64, hi as f64) as u32
}

fn font_row(store: &Store, row: usize, def: (&str, f32)) -> FontRef {
    let line = store.list("LBfont").get(row).map(String::as_str).unwrap_or("");
    let f: Vec<&str> = line.split('\t').collect();
    let family = f.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()).unwrap_or(def.0).to_string();
    let size_pt = f
        .get(2)
        .and_then(|s| s.trim().parse::<f32>().ok())
        .filter(|v| (1.0..=500.0).contains(v))
        .unwrap_or(def.1);
    let style = f.get(3).and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(0) & 0xf;
    FontRef { family, size_pt, style }
}

impl Settings {
    pub fn from_store(s: &Store) -> Settings {
        let color = |n: &str| Rgb::from_tcolor(s.color(n));
        let combo = |n: &str, max: i32| s.item_index(n).clamp(0, max) as u8;
        let step = |n: &str| count(s, n, 1, 100_000, 1);
        Settings {
            size_by_height: s.item_index("CBmode") == 1,
            size: num(s, "SEsize", 1, 20_000, 300) as u32,
            scale_v: positive(s, "EDv", 1.0),
            scale_h: positive(s, "EDh", 1.0),
            land: s.checked("CBland"),
            land_height: num(s, "SElandheight", 1, 1000, 100),
            eta: num(s, "SEeta", -45, 45, 30),
            phi: num(s, "SEphi", 0, 85, 30),
            surface: s.checked("CBsurface"),
            graph: s.checked("CBgraph"),
            scatter: s.checked("CBscatter"),
            panel: s.checked("CBpanel"),
            dot_size: num(s, "SEdotsize", 1, 1000, 3) as u32,
            scatter_mode: combo("CBscattemode", 3),
            vector_color: color("SPvectorcolor"),
            vector_head_color: color("SPvectorheadcolor"),
            vector_len: positive(s, "EDvector", 100.0),
            head_len: positive(s, "EDhead", 40.0),
            skip_x: s.checked("CBskipx"),
            skip_y: s.checked("CBskipy"),
            step_x: step("EDstepx"),
            step_y: step("EDstepy"),
            offset_x: count(s, "EDoffsetx", 0, 100_000, 0),
            offset_y: count(s, "EDoffsety", 0, 100_000, 0),
            head_fix: s.checked("CBfix"),
            head_paint: s.checked("CBpaint"),
            head_angle: num(s, "SEvectorheadangle", 0, 90, 15),
            vmax_on: s.checked("CBenVMax"),
            vmax: positive(s, "EDenVMax", 1.0),
            vector_head: s.checked("CBvectorhead"),
            vector: s.checked("CBvector"),
            color_vector: s.checked("CBcolorvector"),
            toko: s.checked("CBtoko"),
            toko_color: color("SPtokocolor"),
            toko_dt: positive(s, "EDdt", 1.0),
            toko_by_count: s.checked("RBctmode1"),
            toko_by_step: s.checked("RBctmode2"),
            toko_count: num(s, "SEtoko", 1, 1000, 10) as u32,
            toko_min_on: s.checked("CBentokomin"),
            toko_max_on: s.checked("CBentokomax"),
            toko_min: real(s, "EDtokomin", 0.0),
            toko_max: real(s, "EDtokomax", 1.0),
            axis: s.checked("CBaxis"),
            axis_title: s.checked("CBaxistitle"),
            title_x: s.text("EDXtitle").to_string(),
            title_y: s.text("EDYtitle").to_string(),
            title_z: s.text("EDZtitle").to_string(),
            axis_head_size: num(s, "SEaxisheadsize", 0, 1000, 15) as u32,
            axis_line_width: num(s, "SEaxislinewidth", 1, 100, 2) as u32,
            axis_cross: s.checked("CBaxsiscross"),
            cartesian: s.checked("RBcartesian"),
            cylindrical: s.checked("RBcylindrical"),
            grid: s.checked("CBgrid"),
            grid_line_width: num(s, "SEgridlinewidth", 1, 100, 1) as u32,
            grid_line_length: num(s, "SEgridlinelength", -100, 100, 5),
            grid_mesh: s.checked("CBgridmesh"),
            grid_box: s.checked("CBgridbox"),
            grid_scale: s.checked("CBgridscale"),
            axis_scale: s.checked("CBaxisscale"),
            memory_x: combo("CBmemoryX", 7),
            memory_y: combo("CBmemoryY", 7),
            memory_z: combo("CBmemoryZ", 7),
            point: s.checked("CBpoint"),
            label: s.checked("CBLabel"),
            fonts: [
                font_row(s, 0, ("MS Gothic", 14.0)),
                font_row(s, 1, ("MS Gothic", 12.0)),
                font_row(s, 2, ("MS Mincho", 12.0)),
                font_row(s, 3, ("MS Gothic", 6.0)),
            ],
            col_x: num(s, "SEdataX", 1, 1000, 1) as usize,
            col_y: num(s, "SEdataY", 1, 1000, 2) as usize,
            col_u: num(s, "SEdataU", 1, 1000, 4) as usize,
            col_v: num(s, "SEdataV", 1, 1000, 5) as usize,
            col_z: num(s, "SEdataZ", 1, 1000, 3) as usize,
            transpose: s.checked("CBtenchi"),
            skip: num(s, "SEskip", 0, 1000, 0) as usize,
            bg: color("SPBGcolor"),
            fg: color("SPFGcolor"),
            margin: num(s, "SEmar", 0, 1000, 30) as u32,
            autosave: s.checked("CBautosave"),
            max_bmp_mb: num(s, "SEmaxbmp", 1, 1000, 4) as u32,
            use_bmp: s.checked("RBusebmp"),
            use_wmf: s.checked("RBusewmf"),
            data_check: s.checked("CBDataCheck"),
            load_conf: s.checked("CBLoadConf"),
            spline: s.checked("CBSpline"),
            legend: s.checked("CBlegend"),
            reverse: s.checked("CBreverse"),
            gradient_type: combo("CBtype", 8),
            fold: s.checked("CBfold"),
            n_colors: num(s, "SEcolor", 2, 200, 200) as usize,
            legend_w: num(s, "SElegendw", 3, 50_000, 200) as u32,
            legend_h: num(s, "SElegendh", 3, 50_000, 15) as u32,
            z_min_on: s.checked("CBenZmin"),
            z_max_on: s.checked("CBenZmax"),
            z_min: real(s, "EDenZmin", 0.0),
            z_max: real(s, "EDenZmax", 1.0),
        }
    }

    pub fn label_font(&self) -> &FontRef {
        &self.fonts[0]
    }
    pub fn legend_font(&self) -> &FontRef {
        &self.fonts[1]
    }
    pub fn axis_title_font(&self) -> &FontRef {
        &self.fonts[2]
    }
    pub fn axis_scale_font(&self) -> &FontRef {
        &self.fonts[3]
    }

    /// Store holding the control values of the shipped forms.
    pub fn default_store() -> Store {
        let f = crate::forms::Forms::load();
        Store::from_forms(&[&f.form1, &f.config, &f.avi, &f.input])
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings::from_store(&Settings::default_store())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_from_forms() {
        let s = Settings::default();
        assert_eq!(s.n_colors, 200);
        assert_eq!((s.legend_w, s.legend_h), (200, 15));
        assert_eq!(s.eta, 30);
        assert_eq!(s.fonts[3].family, "terminal");
        assert_eq!(s.fonts[3].size_pt, 6.0);
        assert_eq!(s.label_font().family, "ＭＳ Ｐゴシック");
        assert_eq!(s.bg, Rgb::WHITE);
        assert_eq!((s.col_x, s.col_y, s.col_z), (1, 2, 3));
    }

    #[test]
    fn clamps_and_bad_text() {
        let mut st = Settings::default_store();
        st.set_position("SEcolor", 5000);
        st.set_position("SElegendw", 0);
        st.set_position("SEtoko", 99_999);
        st.set_text("EDenZmin", "abc");
        st.set_text("EDv", "-3");
        st.set_text("EDstepx", "0");
        st.set_color("SPBGcolor", 0x0000_00ff);
        st.set_item_index("CBtype", 99);
        st.lists.insert("LBfont".into(), vec!["x".into()]);
        let s = Settings::from_store(&st);
        assert_eq!(s.n_colors, 200);
        assert_eq!(s.legend_w, 3);
        assert_eq!(s.toko_count, 1000);
        assert_eq!(s.z_min, 0.0);
        assert_eq!(s.scale_v, 1.0);
        assert_eq!(s.step_x, 1);
        assert_eq!(s.bg, Rgb(255, 0, 0));
        assert_eq!(s.gradient_type, 8);
        assert_eq!(s.fonts[0].family, "MS Gothic");
    }

    #[test]
    fn empty_store_does_not_panic() {
        let s = Settings::from_store(&Store::default());
        assert_eq!(s.size, 300);
        assert!(!s.size_by_height);
    }

    /// The settings dialog links TBeta/SEeta and TBphi/SEphi one to one. The DFM shows the same
    /// range, hint and default for each pair, so the conversion is the identity (issue #8).
    #[test]
    fn angle_trackbars_and_spins_share_their_range_and_default() {
        let f = crate::forms::Forms::load();
        let cfg = &f.config;
        for (track, spin, lo, hi, default) in [("TBeta", "SEeta", -45, 45, 30), ("TBphi", "SEphi", 0, 85, 30)] {
            let t = cfg.find(track).unwrap_or_else(|| panic!("{track} missing"));
            let s = cfg.find(spin).unwrap_or_else(|| panic!("{spin} missing"));
            assert_eq!((t.int("Min"), t.int("Max")), (lo, hi), "{track} range");
            assert_eq!((s.int("MinValue"), s.int("MaxValue")), (lo, hi), "{spin} range");
            assert_eq!(t.int("Position"), default, "{track} default");
            assert_eq!(s.int("Value"), default, "{spin} default");
        }
    }
}
