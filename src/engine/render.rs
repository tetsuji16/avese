use crate::engine::canvas::{Canvas, FontSpec};
use crate::engine::gradient::Gradient;
use crate::engine::grid::Grid;

#[derive(Clone, Debug)]
pub struct RenderConfig {
    pub width: u32,
    pub height: u32,
    pub margin: u32,
    pub bg: u32,
    pub legend: bool,
    pub legend_w: u32,
    pub legend_h: u32,
    pub gradient: Gradient,
    pub z_min: Option<f64>,
    pub z_max: Option<f64>,
    pub flip_y: bool,
}

pub fn render_distribution(grid: &Grid, cfg: &RenderConfig) -> Canvas {
    let mut canvas = Canvas::new(cfg.width, cfg.height, cfg.bg);
    let m = cfg.margin as i32;
    // legend bar below? spec: left upper margin inside; legend true -> draw gradient bar height legend_h width legend_w below? 
    // We'll draw legend bar at bottom-left area: x=m, y=height - margin - legend_h
    let (zmin, zmax) = match (cfg.z_min, cfg.z_max) {
        (Some(a), Some(b)) => (a, b),
        (Some(a), None) => (a, grid.z_range().map(|r| r.1).unwrap_or(a)),
        (None, Some(b)) => (grid.z_range().map(|r| r.0).unwrap_or(b), b),
        (None, None) => grid.z_range().unwrap_or((0.0, 1.0)),
    };
    let range = (zmax - zmin).abs().max(1e-12);
    // draw area
    let area_x0 = m;
    let area_y0 = m;
    let legend_space = if cfg.legend { cfg.legend_h as i32 + 20 } else { 0 };
    let area_w = cfg.width as i32 - 2*m;
    let area_h = cfg.height as i32 - 2*m - legend_space;
    for py in 0..area_h {
        for px in 0..area_w {
            // map pixel to grid coords
            let fx = px as f64 / (area_w.max(1)-1).max(1) as f64;
            let fy = py as f64 / (area_h.max(1)-1).max(1) as f64;
            // x coordinate
            let x = grid.xs.first().copied().unwrap_or(0.0) + fx * (grid.xs.last().copied().unwrap_or(0.0) - grid.xs.first().copied().unwrap_or(0.0));
            let yfrac = if cfg.flip_y { 1.0 - fy } else { fy };
            let y = grid.ys.first().copied().unwrap_or(0.0) + yfrac * (grid.ys.last().copied().unwrap_or(0.0) - grid.ys.first().copied().unwrap_or(0.0));
            if let Some(v) = grid.sample(x, y) {
                let t = ((v - zmin) / range).clamp(0.0, 1.0);
                let color = cfg.gradient.color(t);
                canvas.set_pixel(area_x0 + px, area_y0 + py, color, 255);
            }
        }
    }
    // legend
    if cfg.legend {
        let lx = m;
        let ly = cfg.height as i32 - m - cfg.legend_h as i32;
        let lw = cfg.legend_w as i32;
        let lh = cfg.legend_h as i32;
        for i in 0..lw {
            let t = i as f64 / (lw-1).max(1) as f64;
            let color = cfg.gradient.color(t);
            for j in 0..lh {
                canvas.set_pixel(lx + i, ly + j, color, 255);
            }
        }
        // min/max text
        let spec = FontSpec { name: "MS Gothic".into(), size: 12.0 };
        canvas.text(lx, ly + lh + 2, &format!("{:.3}", zmin), &spec, 0x000000);
        canvas.text(lx + lw - 40, ly + lh + 2, &format!("{:.3}", zmax), &spec, 0x000000);
    }
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::grid::Grid;
    use crate::engine::data::Point;
    fn pt(x:f64,y:f64,z:Option<f64>) -> Point { Point{x:Some(x),y:Some(y),u:None,v:None,z} }
    #[test]
    fn colors_endpoints() {
        let pts = vec![pt(0.0,0.0,Some(0.0)), pt(1.0,0.0,Some(1.0)), pt(0.0,1.0,Some(0.0)), pt(1.0,1.0,Some(1.0))];
        let g = Grid::from_points(&pts).unwrap();
        let grad = crate::engine::gradient::Gradient::new(crate::engine::gradient::GradientKind::Gray, 2, false, false);
        let cfg = RenderConfig { width: 4, height: 4, margin: 0, bg: 0xffffff, legend: false, legend_w: 10, legend_h: 10, gradient: grad, z_min: Some(0.0), z_max: Some(1.0), flip_y: true };
        let c = render_distribution(&g, &cfg);
        // leftmost pixel (x=0) should be dark-ish, rightmost light-ish
        let left = c.get_pixel(0,0).unwrap();
        let right = c.get_pixel(3,0).unwrap();
        assert!(left.0 < right.0);
    }
    #[test]
    fn none_bg() {
        // grid with a None cell; pixel mapping roughly; just ensure no panic
        let pts = vec![pt(0.0,0.0,Some(0.0))];
        // need 2x2? single point grid nx=1 ny=1; sample only exact. Use 2x2 with None
        let pts = vec![pt(0.0,0.0,Some(0.0)), pt(1.0,0.0,Some(1.0)), pt(0.0,1.0,Some(0.0)), pt(1.0,1.0,None)];
        let g = Grid::from_points(&pts).unwrap();
        let grad = crate::engine::gradient::Gradient::new(crate::engine::gradient::GradientKind::Gray, 2, false, false);
        let cfg = RenderConfig { width: 4, height: 4, margin: 0, bg: 0xabcdef, legend: true, legend_w: 4, legend_h: 4, gradient: grad, z_min: None, z_max: None, flip_y: true };
        let c = render_distribution(&g, &cfg);
        // find a bg pixel possible; just no panic
        let _ = c.get_pixel(3,3);
    }
    #[test]
    fn legend_no_panic() {
        let pts = vec![pt(0.0,0.0,Some(0.0)), pt(1.0,1.0,Some(1.0))];
        let g = Grid::from_points(&pts).unwrap();
        let grad = crate::engine::gradient::Gradient::new(crate::engine::gradient::GradientKind::Gray, 2, false, false);
        let cfg = RenderConfig { width: 8, height: 8, margin: 1, bg: 0xffffff, legend: true, legend_w: 8, legend_h: 4, gradient: grad, z_min: None, z_max: None, flip_y: true };
        let _ = render_distribution(&g, &cfg);
    }
}