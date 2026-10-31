use crate::engine::canvas::Canvas;
use crate::engine::data::{parse_data, DataConfig};
use crate::engine::gradient::{Gradient, GradientKind};
use crate::engine::grid::Grid;
use crate::engine::render::{render_distribution, RenderConfig};
use crate::state::Store;

#[derive(Clone, Debug)]
pub struct RenderSettings {
    pub x_col: usize,
    pub y_col: usize,
    pub u_col: usize,
    pub v_col: usize,
    pub z_col: usize,
    pub skip: usize,
    pub n_colors: usize,
    pub reverse: bool,
    pub fold: bool,
    pub gradient_kind: GradientKind,
    pub legend: bool,
    pub legend_w: u32,
    pub legend_h: u32,
    pub z_min: Option<f64>,
    pub z_max: Option<f64>,
    pub bg: u32,
    pub width_mode: bool,
    pub size: u32,
}

impl Default for RenderSettings {
    fn default() -> Self {
        RenderSettings {
            x_col: 1, y_col: 2, u_col: 3, v_col: 4, z_col: 5,
            skip: 0, n_colors: 200, reverse: false, fold: false,
            gradient_kind: GradientKind::Smooth1, legend: false,
            legend_w: 200, legend_h: 15, z_min: None, z_max: None,
            bg: 0xFFFFFF, width_mode: true, size: 300,
        }
    }
}

impl RenderSettings {
    pub fn from_store(store: &Store) -> Self {
        let mut s = RenderSettings::default();
        let get_ctl = |name: &str| store.get(name);
        if let Some(c) = get_ctl("SEdataX") { s.x_col = c.position.max(1) as usize; }
        if let Some(c) = get_ctl("SEdataY") { s.y_col = c.position.max(1) as usize; }
        if let Some(c) = get_ctl("SEdataU") { s.u_col = c.position.max(1) as usize; }
        if let Some(c) = get_ctl("SEdataV") { s.v_col = c.position.max(1) as usize; }
        if let Some(c) = get_ctl("SEdataZ") { s.z_col = c.position.max(1) as usize; }
        if let Some(c) = get_ctl("SEskip") { s.skip = c.position.max(0) as usize; }
        if let Some(c) = get_ctl("SEcolor") { s.n_colors = c.position.clamp(2,200) as usize; if s.n_colors == 0 { s.n_colors = 200; } }
        s.reverse = store.checked("CBreverse");
        s.fold = store.checked("CBfold");
        // combo CBtype: item index 0 = Smooth1
        let ti = store.item_index("CBtype");
        s.gradient_kind = match ti {
            0 => GradientKind::Smooth1,
            1 => GradientKind::Smooth2,
            2 => GradientKind::Smooth3,
            3 => GradientKind::Gray,
            4 => GradientKind::Spectral1,
            5 => GradientKind::Spectral2,
            6 => GradientKind::Spectral3,
            _ => GradientKind::Smooth1,
        };
        s.legend = store.checked("CBlegend");
        if let Some(c) = get_ctl("SElegendw") { s.legend_w = c.position.max(1) as u32; }
        if let Some(c) = get_ctl("SElegendh") { s.legend_h = c.position.max(1) as u32; }
        // CBenZmin / EDenZmin
        if store.checked("CBenZmin") {
            let t = store.text("EDenZmin");
            if let Ok(v) = t.trim().parse::<f64>() { s.z_min = Some(v); }
        }
        if store.checked("CBenZmax") {
            let t = store.text("EDenZmax");
            if let Ok(v) = t.trim().parse::<f64>() { s.z_max = Some(v); }
        }
        // SPBGcolor TColor BGR decimal -> 0xRRGGBB
        let col = store.color("SPBGcolor");
        // col is 0x00BBGGRR? store color is u32 from DFM Brush.Color (BGR). Convert.
        let b = (col >> 16) & 0xff;
        let g = (col >> 8) & 0xff;
        let r = col & 0xff;
        s.bg = (r << 16) | (g << 8) | b;
        // CBmode item 0 = width
        s.width_mode = store.item_index("CBmode") == 0;
        if let Some(c) = get_ctl("SEsize") { s.size = c.position.max(1) as u32; }
        // defaults when missing: position defaults 0; adjust zeros
        if s.size == 0 { s.size = 300; }
        if s.legend_w == 0 { s.legend_w = 200; }
        if s.legend_h == 0 { s.legend_h = 15; }
        s
    }
}

pub fn render_file(bytes: &[u8], settings: &RenderSettings) -> Result<Canvas, String> {
    // decode
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
            cow.into_owned()
        }
    };
    let cfg = DataConfig {
        skip: settings.skip,
        x_col: settings.x_col,
        y_col: settings.y_col,
        u_col: settings.u_col,
        v_col: settings.v_col,
        z_col: settings.z_col,
    };
    let (points, _errs) = parse_data(&text, &cfg);
    let grid = Grid::from_points(&points).ok_or_else(|| "データがありません".to_string())?;
    if grid.nx < 2 || grid.ny < 2 { return Err("格子が小さすぎます (2x2 以上が必要)".to_string()); }
    // dimensions
    let xr = grid.xs.last().copied().unwrap_or(0.0) - grid.xs.first().copied().unwrap_or(0.0);
    let yr = grid.ys.last().copied().unwrap_or(0.0) - grid.ys.first().copied().unwrap_or(0.0);
    let size = settings.size.max(1) as f64;
    let (w, h) = if settings.width_mode {
        let width = size;
        let height = if xr.abs() < 1e-12 { size } else { size * (yr / xr).abs() };
        (width, height)
    } else {
        let height = size;
        let width = if yr.abs() < 1e-12 { size } else { size * (xr / yr).abs() };
        (width, height)
    };
    let w = (w.round().min(8192.0) as u32).max(1);
    let h = (h.round().min(8192.0) as u32).max(1);
    let grad = Gradient::new(settings.gradient_kind, settings.n_colors, settings.reverse, settings.fold);
    let rcfg = RenderConfig {
        width: w, height: h, margin: 0, bg: settings.bg,
        legend: settings.legend, legend_w: settings.legend_w, legend_h: settings.legend_h,
        gradient: grad, z_min: settings.z_min, z_max: settings.z_max, flip_y: true,
    };
    Ok(render_distribution(&grid, &rcfg))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> RenderSettings {
        RenderSettings { x_col: 1, y_col: 2, z_col: 3, size: 20, ..RenderSettings::default() }
    }

    #[test]
    fn grid_3x3_dimensions_and_colours() {
        let mut t = String::new();
        for y in 0..3 { for x in 0..3 { t += &format!("{} {} {}
", x, y, x + y * 3); } }
        let c = render_file(t.as_bytes(), &cfg()).unwrap();
        assert_eq!((c.width, c.height), (20, 20));
        let rgba = c.to_rgba_bytes();
        assert_eq!(rgba.len(), 20 * 20 * 4);
        let mut cols: Vec<[u8; 3]> = rgba.chunks(4).map(|p| [p[0], p[1], p[2]]).collect();
        cols.sort();
        cols.dedup();
        assert!(cols.len() > 1 && cols.first() != cols.last());
    }

    #[test]
    fn shift_jis_bytes_decode() {
        let (sj, _, _) = encoding_rs::SHIFT_JIS.encode("# テスト
0 0 1
1 0 2
0 1 3
1 1 4
");
        assert!(std::str::from_utf8(&sj).is_err());
        assert!(render_file(&sj, &cfg()).is_ok());
    }

    #[test]
    fn empty_data_is_err() {
        assert!(render_file(b"", &cfg()).is_err());
        assert!(render_file(b"1 1 1
", &cfg()).is_err());
    }
}
