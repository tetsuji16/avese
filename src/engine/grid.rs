#[derive(Clone, Debug)]
pub struct Grid {
    pub nx: usize,
    pub ny: usize,
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    pub z: Vec<Option<f64>>,
}

use crate::engine::data::Point;

impl Grid {
    pub fn from_points(points: &[Point]) -> Option<Grid> {
        if points.is_empty() { return None; }
        let mut xs: Vec<f64> = points.iter().filter_map(|p| p.x).collect();
        let mut ys: Vec<f64> = points.iter().filter_map(|p| p.y).collect();
        xs.sort_by(|a,b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        xs.dedup_by(|a,b| (*a-*b).abs() < 1e-12);
        ys.sort_by(|a,b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        ys.dedup_by(|a,b| (*a-*b).abs() < 1e-12);
        if xs.is_empty() || ys.is_empty() { return None; }
        let nx = xs.len();
        let ny = ys.len();
        let mut z = vec![None; nx*ny];
        for p in points {
            if let (Some(x), Some(y)) = (p.x, p.y) {
                let xi = xs.iter().position(|v| (*v-x).abs()<1e-12);
                let yi = ys.iter().position(|v| (*v-y).abs()<1e-12);
                if let (Some(xi), Some(yi)) = (xi, yi) {
                    z[yi*nx + xi] = p.z;
                }
            }
        }
        Some(Grid { nx, ny, xs, ys, z })
    }
    pub fn z_range(&self) -> Option<(f64, f64)> {
        let vals: Vec<f64> = self.z.iter().filter_map(|v| *v).collect();
        if vals.is_empty() { return None; }
        let mn = vals.iter().cloned().fold(f64::INFINITY, f64::min);
        let mx = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        Some((mn, mx))
    }
    pub fn sample(&self, x: f64, y: f64) -> Option<f64> {
        // find cell
        if self.xs.is_empty() || self.ys.is_empty() { return None; }
        if x < self.xs[0] || x > *self.xs.last().unwrap() { return None; }
        if y < self.ys[0] || y > *self.ys.last().unwrap() { return None; }
        // find i
        let mut i = 0usize;
        while i+1 < self.xs.len() && x > self.xs[i+1] { i += 1; }
        let mut j = 0usize;
        while j+1 < self.ys.len() && y > self.ys[j+1] { j += 1; }
        if i+1 >= self.xs.len() || j+1 >= self.ys.len() {
            // on boundary last: clamp
            if i+1 >= self.xs.len() { i = self.xs.len().saturating_sub(2); }
            if j+1 >= self.ys.len() { j = self.ys.len().saturating_sub(2); }
        }
        let x0 = self.xs[i]; let x1 = self.xs[i+1];
        let y0 = self.ys[j]; let y1 = self.ys[j+1];
        let z00 = self.z[j*self.nx + i];
        let z10 = self.z[j*self.nx + (i+1)];
        let z01 = self.z[(j+1)*self.nx + i];
        let z11 = self.z[(j+1)*self.nx + (i+1)];
        let (Some(a),Some(b),Some(c),Some(d)) = (z00,z10,z01,z11) else { return None; };
        let dx = if (x1-x0).abs() < 1e-12 { 0.0 } else { (x-x0)/(x1-x0) };
        let dy = if (y1-y0).abs() < 1e-12 { 0.0 } else { (y-y0)/(y1-y0) };
        let v = a*(1.0-dx)*(1.0-dy) + b*dx*(1.0-dy) + c*(1.0-dx)*dy + d*dx*dy;
        Some(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::data::Point;
    fn pt(x:f64,y:f64,z:Option<f64>) -> Point { Point{x:Some(x),y:Some(y),u:None,v:None,z} }
    #[test]
    fn grid_3x3() {
        let mut pts = Vec::new();
        for y in 0..3 { for x in 0..3 { pts.push(pt(x as f64, y as f64, Some((x+y) as f64))); } }
        let g = Grid::from_points(&pts).unwrap();
        assert_eq!(g.nx,3); assert_eq!(g.ny,3);
    }
    #[test]
    fn missing() {
        let pts = vec![pt(0.0,0.0,Some(1.0)), pt(1.0,0.0,None)];
        let g = Grid::from_points(&pts).unwrap();
        assert_eq!(g.z[1], None);
    }
    #[test]
    fn interp() {
        let pts = vec![pt(0.0,0.0,Some(0.0)), pt(1.0,0.0,Some(10.0)), pt(0.0,1.0,Some(0.0)), pt(1.0,1.0,Some(10.0))];
        let g = Grid::from_points(&pts).unwrap();
        let v = g.sample(0.5,0.5).unwrap();
        assert!((v-5.0).abs()<1e-9);
    }
    #[test]
    fn out_of_range() {
        let pts = vec![pt(0.0,0.0,Some(0.0)), pt(1.0,0.0,Some(1.0))];
        let g = Grid::from_points(&pts).unwrap();
        assert!(g.sample(2.0,0.0).is_none());
    }
    #[test]
    fn zrange() {
        let pts = vec![pt(0.0,0.0,Some(2.0)), pt(1.0,0.0,Some(5.0)), pt(0.0,1.0,None)];
        let g = Grid::from_points(&pts).unwrap();
        assert_eq!(g.z_range(), Some((2.0,5.0)));
    }
    #[test]
    fn empty_none() {
        assert!(Grid::from_points(&[]).is_none());
    }
    #[test]
    fn all_none_grid() {
        let pts = vec![pt(0.0,0.0,None), pt(1.0,0.0,None)];
        let g = Grid::from_points(&pts).unwrap();
        assert!(g.z_range().is_none());
    }
}