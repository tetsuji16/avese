//! Unified scalar/vector field: structured grid, triangle mesh or panel, with interpolation.
use crate::engine::data::{DataFile, PanelData};
use crate::engine::mesh;
use crate::engine::settings::Settings;
use std::collections::HashSet;

const MAX_GRID_CELLS: usize = 50_000_000;

/// Classify raw point data for the non-structured grid warning without building a field.
/// This is also used before IncludeINI can change the input to panel mode.
pub fn has_irregular_grid(df: &DataFile, settings: &Settings) -> bool {
    if !settings.data_check || settings.panel || df.panel.is_some() || df.mesh_auto { return false; }
    let pts: Vec<_> = df.points.iter().filter(|p| p.x.is_some() && p.y.is_some()).collect();
    if pts.is_empty() { return false; }
    let xs = cluster(pts.iter().filter_map(|p| p.x).collect());
    let ys = cluster(pts.iter().filter_map(|p| p.y).collect());
    let (nx, ny) = (xs.len(), ys.len());
    if nx < 2 || ny < 2 { return true; }
    let Some(cells) = nx.checked_mul(ny).filter(|&c| c <= MAX_GRID_CELLS) else { return true; };
    let mut filled = HashSet::with_capacity(pts.len());
    for p in &pts {
        let (Some(x), Some(y)) = (p.x, p.y) else { continue; };
        let k = nearest(&ys, y) * nx + nearest(&xs, x);
        filled.insert(k);
    }
    filled.len() != cells || pts.len() != cells
}

#[derive(Clone, Debug)]
pub enum Field {
    Grid(GridField),
    Mesh(MeshField),
    /// x runs over 0..cols, y over 0..rows with y = 0 at the top row
    Panel(PanelData),
}

#[derive(Clone, Debug)]
pub struct GridField {
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
    /// index = j * nx + i
    pub z: Vec<Option<f64>>,
    pub u: Vec<Option<f64>>,
    pub v: Vec<Option<f64>>,
    regular: bool,
    spline: Option<Spline>,
}

#[derive(Clone, Debug)]
struct Spline {
    /// second derivatives along x of each row of z
    mx: Vec<f64>,
    /// second derivatives along y of each column of z and of mx
    zyy: Vec<f64>,
    mxyy: Vec<f64>,
    row_ok: Vec<bool>,
}

#[derive(Clone, Debug)]
pub struct MeshField {
    pub nodes: Vec<(f64, f64)>,
    pub z: Vec<Option<f64>>,
    pub u: Vec<Option<f64>>,
    pub v: Vec<Option<f64>>,
    pub tris: Vec<[usize; 3]>,
    bins: Bins,
}

#[derive(Clone, Debug)]
struct Bins {
    x0: f64,
    y0: f64,
    cw: f64,
    ch: f64,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<u32>>,
}

/// Mapping between the plane and (theta, r) for cylindrical data (column 1 = theta, 2 = r).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cylinder {
    pub th_min: f64,
    pub th_max: f64,
    pub r_min: f64,
    pub r_max: f64,
}

impl Cylinder {
    /// Offset from the centre, in r units, to the data coordinates (theta, r).
    /// theta_min..theta_max covers the full circle, measured counter-clockwise from +x.
    pub fn to_data(&self, dx: f64, dy: f64) -> (f64, f64) {
        let a = dy.atan2(dx).rem_euclid(std::f64::consts::TAU);
        let th = self.th_min + a / std::f64::consts::TAU * (self.th_max - self.th_min);
        (th, self.r_min + dx.hypot(dy))
    }

    /// Inverse of `to_data` for the offset from the centre.
    pub fn to_plane(&self, theta: f64, r: f64) -> (f64, f64) {
        let span = self.th_max - self.th_min;
        let a = if span.abs() < f64::EPSILON { 0.0 } else { (theta - self.th_min) / span * std::f64::consts::TAU };
        let rr = r - self.r_min;
        (rr * a.cos(), rr * a.sin())
    }
}

pub fn build(df: &DataFile, settings: &Settings) -> Result<(Field, Vec<String>), String> {
    build_with_progress(df, settings, |_| {})
}

pub fn build_with_progress(
    df: &DataFile,
    settings: &Settings,
    progress: impl FnMut(f32),
) -> Result<(Field, Vec<String>), String> {
    let mut warnings = Vec::new();
    if let Some(p) = &df.panel {
        if p.rows == 0 || p.cols == 0 {
            return Err("データがありません".to_string());
        }
        return Ok((Field::Panel(p.clone()), warnings));
    }
    let pts: Vec<_> = df.points.iter().filter(|p| p.x.is_some() && p.y.is_some()).collect();
    if pts.len() < df.points.len() {
        warnings.push(format!("座標が欠けている {} 行を無視しました", df.points.len() - pts.len()));
    }
    if pts.is_empty() {
        return Err("データがありません".to_string());
    }
    if df.mesh_auto {
        let nodes: Vec<(f64, f64)> = pts.iter().map(|p| (p.x.unwrap_or(0.0), p.y.unwrap_or(0.0))).collect();
        let tris = mesh::triangulate_points(&nodes, progress)?;
        let bins = Bins::new(&nodes, &tris);
        let m = MeshField {
            z: pts.iter().map(|p| p.z).collect(),
            u: pts.iter().map(|p| p.u).collect(),
            v: pts.iter().map(|p| p.v).collect(),
            nodes,
            tris,
            bins,
        };
        return Ok((Field::Mesh(m), warnings));
    }

    let xs = cluster(pts.iter().filter_map(|p| p.x).collect());
    let ys = cluster(pts.iter().filter_map(|p| p.y).collect());
    let (nx, ny) = (xs.len(), ys.len());
    if nx < 2 || ny < 2 {
        return Err("格子が小さすぎます (2x2 以上が必要)".to_string());
    }
    let cells = nx.checked_mul(ny).filter(|&c| c <= MAX_GRID_CELLS).ok_or("データが格子状に並んでいません")?;
    let mut z = vec![None; cells];
    let mut u = vec![None; cells];
    let mut v = vec![None; cells];
    let mut filled = vec![false; cells];
    let mut covered = 0usize;
    for p in &pts {
        let (Some(px), Some(py)) = (p.x, p.y) else { continue };
        let k = nearest(&ys, py) * nx + nearest(&xs, px);
        if !filled[k] {
            filled[k] = true;
            covered += 1;
        }
        z[k] = p.z;
        u[k] = p.u;
        v[k] = p.v;
    }
    let regular = covered == cells && pts.len() == cells;
    if !regular && settings.data_check {
        warnings.push(format!("データが格子状に並んでいません ({} 点, {} x {} の格子)", pts.len(), nx, ny));
    }
    let spline = if settings.spline { Some(Spline::new(&xs, &ys, &z)) } else { None };
    Ok((Field::Grid(GridField { xs, ys, z, u, v, regular, spline }), warnings))
}

/// Sorted unique values; values closer than a relative tolerance are merged.
fn cluster(mut vals: Vec<f64>) -> Vec<f64> {
    vals.sort_by(|a, b| a.total_cmp(b));
    let (Some(&lo), Some(&hi)) = (vals.first(), vals.last()) else { return vals };
    let tol = (hi - lo) * 1e-9 + lo.abs().max(hi.abs()) * 1e-12;
    let mut out: Vec<f64> = Vec::new();
    let (mut sum, mut n, mut last) = (0.0, 0usize, f64::NAN);
    for v in vals {
        if n > 0 && v - last > tol {
            out.push(sum / n as f64);
            sum = 0.0;
            n = 0;
        }
        sum += v;
        n += 1;
        last = v;
    }
    if n > 0 {
        out.push(sum / n as f64);
    }
    out
}

fn nearest(sorted: &[f64], v: f64) -> usize {
    let i = sorted.partition_point(|&c| c < v);
    if i == 0 {
        0
    } else if i >= sorted.len() {
        sorted.len() - 1
    } else if v - sorted[i - 1] <= sorted[i] - v {
        i - 1
    } else {
        i
    }
}

/// Second derivatives of the natural cubic spline through (x, y).
fn natural_m2(x: &[f64], y: &[f64]) -> Vec<f64> {
    let n = x.len();
    let mut m = vec![0.0; n];
    if n < 3 {
        return m;
    }
    let k = n - 2;
    let (mut c, mut d) = (vec![0.0; k], vec![0.0; k]);
    for r in 0..k {
        let i = r + 1;
        let (h0, h1) = (x[i] - x[i - 1], x[i + 1] - x[i]);
        let diag = 2.0 * (h0 + h1);
        let rhs = 6.0 * ((y[i + 1] - y[i]) / h1 - (y[i] - y[i - 1]) / h0);
        if r == 0 {
            c[r] = h1 / diag;
            d[r] = rhs / diag;
        } else {
            let denom = diag - h0 * c[r - 1];
            c[r] = h1 / denom;
            d[r] = (rhs - h0 * d[r - 1]) / denom;
        }
    }
    m[k] = d[k - 1];
    for r in (0..k - 1).rev() {
        m[r + 1] = d[r] - c[r] * m[r + 2];
    }
    m
}

impl Spline {
    fn new(xs: &[f64], ys: &[f64], z: &[Option<f64>]) -> Spline {
        let (nx, ny) = (xs.len(), ys.len());
        let row_ok: Vec<bool> = (0..ny).map(|j| (0..nx).all(|i| z[j * nx + i].is_some())).collect();
        let mut mx = vec![0.0; nx * ny];
        for j in (0..ny).filter(|&j| row_ok[j]) {
            let row: Vec<f64> = (0..nx).map(|i| z[j * nx + i].unwrap_or(0.0)).collect();
            mx[j * nx..(j + 1) * nx].copy_from_slice(&natural_m2(xs, &row));
        }
        let mut zyy = vec![0.0; nx * ny];
        let mut mxyy = vec![0.0; nx * ny];
        // A missing value invalidates the x derivatives of its entire row.
        // Build y derivatives within each contiguous run of complete rows so
        // those uncomputed derivatives cannot distort other valid regions.
        let mut start = 0;
        while start < ny {
            if !row_ok[start] { start += 1; continue; }
            let mut end = start + 1;
            while end < ny && row_ok[end] { end += 1; }
            if end - start >= 2 {
                for i in 0..nx {
                    let zc: Vec<f64> = (start..end).map(|j| z[j * nx + i].unwrap_or(0.0)).collect();
                    let mc: Vec<f64> = (start..end).map(|j| mx[j * nx + i]).collect();
                    let (a, b) = (natural_m2(&ys[start..end], &zc), natural_m2(&ys[start..end], &mc));
                    for j in start..end {
                        zyy[j * nx + i] = a[j - start];
                        mxyy[j * nx + i] = b[j - start];
                    }
                }
            }
            start = end;
        }
        Spline { mx, zyy, mxyy, row_ok }
    }
}

impl GridField {
    pub fn nx(&self) -> usize {
        self.xs.len()
    }
    pub fn ny(&self) -> usize {
        self.ys.len()
    }
    pub fn at(&self, i: usize, j: usize) -> Option<f64> {
        self.z.get(j * self.xs.len() + i).copied().flatten()
    }
    pub fn is_regular(&self) -> bool {
        self.regular
    }

    /// Cell index along one axis, with a small tolerance outside the range.
    fn locate(axis: &[f64], p: f64) -> Option<(usize, f64)> {
        let n = axis.len();
        if n < 2 || !p.is_finite() {
            return None;
        }
        let (lo, hi) = (axis[0], axis[n - 1]);
        let tol = (hi - lo) * 1e-9;
        if p < lo - tol || p > hi + tol {
            return None;
        }
        let i = axis.partition_point(|&c| c <= p).saturating_sub(1).min(n - 2);
        let h = axis[i + 1] - axis[i];
        Some((i, ((p - axis[i]) / h).clamp(0.0, 1.0)))
    }

    pub fn has_spline(&self) -> bool {
        self.spline.is_some()
    }

    pub fn sample(&self, x: f64, y: f64) -> Option<f64> {
        let nx = self.xs.len();
        let (i, tx) = Self::locate(&self.xs, x)?;
        let (j, ty) = Self::locate(&self.ys, y)?;
        let k = j * nx + i;
        // None when a corner of the cell is missing, with or without the spline.
        let linear = self.sample_linear(x, y)?;
        if let Some(sp) = &self.spline {
            if sp.row_ok[j] && sp.row_ok[j + 1] {
                let hx = self.xs[i + 1] - self.xs[i];
                let hy = self.ys[j + 1] - self.ys[j];
                let (a, b) = (1.0 - tx, tx);
                let (ca, cb) = (a * a * a - a, b * b * b - b);
                let along_x = |kk: usize| {
                    let s = a * self.z[kk].unwrap_or(0.0)
                        + b * self.z[kk + 1].unwrap_or(0.0)
                        + (ca * sp.mx[kk] + cb * sp.mx[kk + 1]) * hx * hx / 6.0;
                    let syy = a * sp.zyy[kk] + b * sp.zyy[kk + 1] + (ca * sp.mxyy[kk] + cb * sp.mxyy[kk + 1]) * hx * hx / 6.0;
                    (s, syy)
                };
                let ((s0, q0), (s1, q1)) = (along_x(k), along_x(k + nx));
                let (c, d) = (1.0 - ty, ty);
                return Some(c * s0 + d * s1 + ((c * c * c - c) * q0 + (d * d * d - d) * q1) * hy * hy / 6.0);
            }
        }
        Some(linear)
    }

    /// The bilinear height of the cell at (x, y), which is what the bird's-eye surface shows even
    /// with spline interpolation on.
    pub fn sample_linear(&self, x: f64, y: f64) -> Option<f64> {
        let nx = self.xs.len();
        let (i, tx) = Self::locate(&self.xs, x)?;
        let (j, ty) = Self::locate(&self.ys, y)?;
        let k = j * nx + i;
        let (z00, z10, z01, z11) = (self.z[k]?, self.z[k + 1]?, self.z[k + nx]?, self.z[k + nx + 1]?);
        let bottom = z00 * (1.0 - tx) + z10 * tx;
        let top = z01 * (1.0 - tx) + z11 * tx;
        Some(bottom * (1.0 - ty) + top * ty)
    }
}

impl Bins {
    fn new(nodes: &[(f64, f64)], tris: &[[usize; 3]]) -> Bins {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in nodes {
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
        }
        let n = ((tris.len() as f64).sqrt().ceil() as usize).clamp(1, 1024);
        let cw = ((x1 - x0) / n as f64).max(f64::MIN_POSITIVE);
        let ch = ((y1 - y0) / n as f64).max(f64::MIN_POSITIVE);
        let mut b = Bins { x0, y0, cw, ch, nx: n, ny: n, cells: vec![Vec::new(); n * n] };
        for (t, tri) in tris.iter().enumerate() {
            let p = tri.map(|i| nodes[i]);
            let (ax, bx) = (p[0].0.min(p[1].0).min(p[2].0), p[0].0.max(p[1].0).max(p[2].0));
            let (ay, by) = (p[0].1.min(p[1].1).min(p[2].1), p[0].1.max(p[1].1).max(p[2].1));
            let (i0, i1) = (b.col(ax), b.col(bx));
            let (j0, j1) = (b.row(ay), b.row(by));
            for j in j0..=j1 {
                for i in i0..=i1 {
                    b.cells[j * n + i].push(t as u32);
                }
            }
        }
        b
    }
    fn col(&self, x: f64) -> usize {
        (((x - self.x0) / self.cw).floor().max(0.0) as usize).min(self.nx - 1)
    }
    fn row(&self, y: f64) -> usize {
        (((y - self.y0) / self.ch).floor().max(0.0) as usize).min(self.ny - 1)
    }
}

impl MeshField {
    /// Triangle containing (x, y) and its barycentric weights.
    pub fn locate(&self, x: f64, y: f64) -> Option<(usize, [f64; 3])> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let b = &self.bins;
        if x < b.x0 - b.cw * 1e-9 || y < b.y0 - b.ch * 1e-9 || x > b.x0 + b.cw * b.nx as f64 * (1.0 + 1e-9) || y > b.y0 + b.ch * b.ny as f64 * (1.0 + 1e-9) {
            return None;
        }
        let eps = -1e-9;
        for &t in &b.cells[b.row(y) * b.nx + b.col(x)] {
            let tri = self.tris[t as usize];
            let (p0, p1, p2) = (self.nodes[tri[0]], self.nodes[tri[1]], self.nodes[tri[2]]);
            let det = (p1.1 - p2.1) * (p0.0 - p2.0) + (p2.0 - p1.0) * (p0.1 - p2.1);
            if det == 0.0 {
                continue;
            }
            let w0 = ((p1.1 - p2.1) * (x - p2.0) + (p2.0 - p1.0) * (y - p2.1)) / det;
            let w1 = ((p2.1 - p0.1) * (x - p2.0) + (p0.0 - p2.0) * (y - p2.1)) / det;
            let w2 = 1.0 - w0 - w1;
            if w0 >= eps && w1 >= eps && w2 >= eps {
                return Some((t as usize, [w0, w1, w2]));
            }
        }
        None
    }

    pub fn sample(&self, x: f64, y: f64) -> Option<f64> {
        let (t, w) = self.locate(x, y)?;
        let tri = self.tris[t];
        Some(w[0] * self.z[tri[0]]? + w[1] * self.z[tri[1]]? + w[2] * self.z[tri[2]]?)
    }
}

fn min_max(it: impl Iterator<Item = f64>) -> Option<(f64, f64)> {
    it.fold(None, |acc, v| match acc {
        None => Some((v, v)),
        Some((lo, hi)) => Some((lo.min(v), hi.max(v))),
    })
}

impl Field {
    /// (x_min, x_max, y_min, y_max)
    pub fn extent(&self) -> Option<(f64, f64, f64, f64)> {
        match self {
            Field::Grid(g) => Some((*g.xs.first()?, *g.xs.last()?, *g.ys.first()?, *g.ys.last()?)),
            Field::Mesh(m) => {
                let (x0, x1) = min_max(m.nodes.iter().map(|p| p.0))?;
                let (y0, y1) = min_max(m.nodes.iter().map(|p| p.1))?;
                Some((x0, x1, y0, y1))
            }
            Field::Panel(p) => Some((0.0, p.cols as f64, 0.0, p.rows as f64)),
        }
    }

    fn z_values(&self) -> &[Option<f64>] {
        match self {
            Field::Grid(g) => &g.z,
            Field::Mesh(m) => &m.z,
            Field::Panel(p) => &p.z,
        }
    }

    /// Min and max of the data.
    pub fn data_z_range(&self) -> Option<(f64, f64)> {
        min_max(self.z_values().iter().flatten().copied())
    }

    /// Range used for colouring: CBenZmin / CBenZmax override the data; hi > lo is guaranteed.
    pub fn z_range(&self, s: &Settings) -> Option<(f64, f64)> {
        let (dlo, dhi) = self.data_z_range().unwrap_or((0.0, 1.0));
        let mut lo = if s.z_min_on { s.z_min } else { dlo };
        let mut hi = if s.z_max_on { s.z_max } else { dhi };
        if self.data_z_range().is_none() && !s.z_min_on && !s.z_max_on {
            return None;
        }
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }
        if hi - lo < 1e-300 {
            hi = lo + 1.0;
        }
        Some((lo, hi))
    }

    /// Largest vector magnitude over the nodes (None without vector data).
    pub fn max_vector(&self) -> Option<f64> {
        let (u, v) = match self {
            Field::Grid(g) => (&g.u, &g.v),
            Field::Mesh(m) => (&m.u, &m.v),
            Field::Panel(_) => return None,
        };
        min_max(u.iter().zip(v).filter_map(|(a, b)| Some(a.unwrap_or(0.0).hypot(b.unwrap_or(0.0))).filter(|_| a.is_some() || b.is_some())))
            .map(|r| r.1)
    }

    /// True when the data fill a complete grid (always true for panels, false for meshes).
    pub fn is_regular(&self) -> bool {
        match self {
            Field::Grid(g) => g.regular,
            Field::Mesh(_) => false,
            Field::Panel(_) => true,
        }
    }

    pub fn sample(&self, x: f64, y: f64) -> Option<f64> {
        match self {
            Field::Grid(g) => g.sample(x, y),
            Field::Mesh(m) => m.sample(x, y),
            Field::Panel(p) => {
                if !(x >= 0.0 && y >= 0.0) || x >= p.cols as f64 || y >= p.rows as f64 {
                    return None;
                }
                p.z.get(y as usize * p.cols + x as usize).copied().flatten()
            }
        }
    }

    /// Parameters for cylindrical drawing; panel columns represent theta and rows represent radius.
    pub fn cylinder(&self) -> Option<Cylinder> {
        let (th_min, th_max, r_min, r_max) = self.extent()?;
        Some(Cylinder { th_min, th_max, r_min, r_max })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::data::{self, DataOptions, Point};

    fn pt(x: f64, y: f64, z: Option<f64>) -> Point {
        Point { x: Some(x), y: Some(y), u: None, v: None, z }
    }

    fn settings(spline: bool) -> Settings {
        Settings { spline, data_check: true, ..Settings::default() }
    }

    #[test]
    fn irregular_precheck_uses_memory_proportional_to_points_for_sparse_large_grids() {
        let points = (0..7_000).map(|i| crate::engine::data::Point {
            x: Some(i as f64), y: Some(i as f64), z: Some(1.0), u: None, v: None,
        }).collect();
        let df = DataFile { points, ..DataFile::default() };
        let settings = Settings { data_check: true, ..Settings::default() };
        // 7,000 x 7,000 potential cells, but only 7,000 input points.
        assert!(has_irregular_grid(&df, &settings));
    }

    fn grid_df(n: usize, f: impl Fn(f64, f64) -> f64) -> DataFile {
        let mut df = DataFile::default();
        for j in 0..n {
            for i in 0..n {
                df.points.push(pt(i as f64, j as f64, Some(f(i as f64, j as f64))));
            }
        }
        df
    }

    #[test]
    fn bilinear_and_edges() {
        let (f, w) = build(&grid_df(3, |x, y| x + 10.0 * y), &settings(false)).unwrap();
        assert!(w.is_empty() && f.is_regular());
        assert_eq!(f.extent(), Some((0.0, 2.0, 0.0, 2.0)));
        assert_eq!(f.data_z_range(), Some((0.0, 22.0)));
        assert!((f.sample(0.5, 1.5).unwrap() - 15.5).abs() < 1e-9);
        assert_eq!(f.sample(2.0, 2.0), Some(22.0));
        assert_eq!(f.sample(2.1, 0.0), None);
        assert_eq!(f.sample(f64::NAN, 0.0), None);
    }

    #[test]
    fn float_noise_in_coordinates() {
        let mut df = DataFile::default();
        for j in 0..3 {
            for i in 0..3 {
                let n = if (i + j) % 2 == 0 { 1e-13 } else { -1e-13 };
                df.points.push(pt(i as f64 * 0.1 + n, j as f64 * 0.1, Some(1.0)));
            }
        }
        let Field::Grid(g) = build(&df, &settings(false)).unwrap().0 else { panic!() };
        assert_eq!((g.nx(), g.ny()), (3, 3));
        assert!(g.is_regular());
    }

    #[test]
    fn missing_cells_and_irregular_warning() {
        let mut df = grid_df(3, |x, _| x);
        df.points[4].z = None;
        let (f, w) = build(&df, &settings(false)).unwrap();
        assert!(w.is_empty());
        assert_eq!(f.sample(0.5, 0.5), None);
        assert!(f.sample(1.5, 1.5).is_none());
        df.points.pop();
        let (f, w) = build(&df, &settings(false)).unwrap();
        assert!(!f.is_regular());
        assert_eq!(w.len(), 1);
        let st = Settings { data_check: false, ..settings(false) };
        assert!(build(&df, &st).unwrap().1.is_empty());
    }

    #[test]
    fn linear_sample_ignores_the_spline() {
        let (Field::Grid(g), _) = build(&grid_df(5, |x, y| x * x + y), &settings(true)).unwrap() else { panic!("expected grid") };
        assert!((g.sample(0.5, 0.0).unwrap() - 0.5).abs() > 0.01, "the spline bends between the nodes");
        assert_eq!(g.sample_linear(0.5, 0.0), Some(0.5));
        assert_eq!(g.sample_linear(1.0, 2.0), Some(3.0));
    }

    #[test]
    fn spline_reproduces_cubic_nodes_and_is_smooth() {
        let (f, _) = build(&grid_df(5, |x, y| x * x + y), &settings(true)).unwrap();
        assert!((f.sample(2.0, 3.0).unwrap() - 7.0).abs() < 1e-9);
        // natural spline of x^2 deviates from the parabola only slightly between nodes
        let s = f.sample(1.5, 1.0).unwrap();
        assert!((s - 3.25).abs() < 0.2, "{s}");
        let lin = build(&grid_df(5, |x, y| x * x + y), &settings(false)).unwrap().0.sample(1.5, 1.0).unwrap();
        assert!((lin - 3.5).abs() < 1e-9);
        let (g, _) = build(&grid_df(4, |x, y| 2.0 * x - y), &settings(true)).unwrap();
        assert!((g.sample(1.3, 2.2).unwrap() - 0.4).abs() < 1e-9);
    }

    #[test]
    fn spline_falls_back_to_linear_near_holes() {
        let mut df = grid_df(4, |x, y| x * y);
        df.points[5].z = None;
        let (f, _) = build(&df, &settings(true)).unwrap();
        assert!(f.sample(2.5, 2.5).is_some());
        assert_eq!(f.sample(0.5, 0.5), None);
    }

    #[test]
    fn missing_row_does_not_corrupt_spline_in_another_region() {
        let mut with_hole = grid_df(6, |x, y| x * x * (1.0 + y * y));
        with_hole.points[5].z = None;
        let mut clean_run = with_hole.clone();
        clean_run.points.retain(|p| p.y != Some(0.0));
        let field = build(&with_hole, &settings(true)).unwrap().0;
        let expected = build(&clean_run, &settings(true)).unwrap().0;
        let linear = build(&clean_run, &settings(false)).unwrap().0;
        for (x, y) in [(1.5, 1.5), (2.3, 2.4), (3.5, 3.5)] {
            let actual = field.sample(x, y).unwrap();
            assert!((actual - expected.sample(x, y).unwrap()).abs() < 1e-9, "spline crossed a missing row at ({x}, {y})");
            assert!((actual - linear.sample(x, y).unwrap()).abs() > 0.01, "the complete region should retain spline interpolation");
        }
        assert!(field.sample(4.5, 0.5).is_none(), "cells touching the missing value remain transparent");
    }

    #[test]
    fn z_range_overrides() {
        let (f, _) = build(&grid_df(3, |x, y| x + y), &settings(false)).unwrap();
        assert_eq!(f.z_range(&settings(false)), Some((0.0, 4.0)));
        let s = Settings { z_min_on: true, z_min: -1.0, z_max_on: true, z_max: -1.0, ..settings(false) };
        assert_eq!(f.z_range(&s), Some((-1.0, 0.0)));
        let s = Settings { z_max_on: true, z_max: 2.0, ..settings(false) };
        assert_eq!(f.z_range(&s), Some((0.0, 2.0)));
    }

    #[test]
    fn mesh_barycentric() {
        let mut df = DataFile { mesh_auto: true, ..DataFile::default() };
        for (x, y) in [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0), (2.0, 2.0)] {
            df.points.push(pt(x, y, Some(x + 2.0 * y)));
        }
        let (f, _) = build(&df, &settings(false)).unwrap();
        let Field::Mesh(m) = &f else { panic!() };
        assert_eq!(m.tris.len(), 4);
        assert!((f.sample(1.0, 3.0).unwrap() - 7.0).abs() < 1e-9);
        assert!((f.sample(3.9, 0.1).unwrap() - 4.1).abs() < 1e-9);
        assert_eq!(f.sample(5.0, 5.0), None);
        assert_eq!(f.extent(), Some((0.0, 4.0, 0.0, 4.0)));
    }

    #[test]
    fn mesh_errors_do_not_panic() {
        let mut df = DataFile { mesh_auto: true, ..DataFile::default() };
        for i in 0..4 {
            df.points.push(pt(i as f64, i as f64, Some(0.0)));
        }
        assert!(build(&df, &settings(false)).is_err());
        df.points = vec![pt(1.0, 1.0, None), pt(1.0, 1.0, None), pt(2.0, 5.0, None)];
        assert!(build(&df, &settings(false)).is_err());
    }

    #[test]
    fn panel_and_errors() {
        let o = DataOptions { panel: true, ..DataOptions::default() };
        let df = data::parse(b"1 2\n3 -\n", &o);
        let (f, _) = build(&df, &settings(false)).unwrap();
        assert_eq!(f.sample(0.5, 0.5), Some(1.0));
        assert_eq!(f.sample(1.5, 0.5), Some(2.0));
        assert_eq!(f.sample(1.5, 1.5), None);
        assert_eq!(f.sample(2.0, 0.5), None);
        assert_eq!(f.cylinder(), Some(Cylinder { th_min:0.0, th_max:2.0, r_min:0.0, r_max:2.0 }));
        assert!(build(&DataFile::default(), &settings(false)).is_err());
        assert!(build(&data::parse(b"1 1 1\n", &DataOptions::default()), &settings(false)).is_err());
    }

    #[test]
    fn cylinder_round_trip() {
        let c = Cylinder { th_min: 0.0, th_max: 360.0, r_min: 1.0, r_max: 5.0 };
        let (th, r) = c.to_data(0.0, 2.0);
        assert!((th - 90.0).abs() < 1e-9 && (r - 3.0).abs() < 1e-9);
        let (x, y) = c.to_plane(90.0, 3.0);
        assert!(x.abs() < 1e-9 && (y - 2.0).abs() < 1e-9);
    }

    #[test]
    fn max_vector() {
        let mut df = grid_df(2, |_, _| 0.0);
        df.points[3].u = Some(3.0);
        df.points[3].v = Some(4.0);
        let (f, _) = build(&df, &settings(false)).unwrap();
        assert_eq!(f.max_vector(), Some(5.0));
    }

    fn sample_file(name: &str) -> Option<Vec<u8>> {
        std::fs::read(crate::original_fixture_dir().join(name)).ok()
    }

    #[test]
    #[ignore = "requires original samples; set AVESE_ORIGINAL_DIR"]
    fn original_samples() {
        let s2 = sample_file("sample2.txt").expect("original sample2.txt not found");
        let o = DataOptions::default();
        let (f, w) = build(&data::parse(&s2, &o), &settings(false)).unwrap();
        assert!(f.is_regular() && w.is_empty());
        let Field::Grid(g) = &f else { panic!() };
        assert_eq!((g.nx(), g.ny()), (21, 21));
        let s1 = sample_file("sample1.txt").unwrap();
        let (f, _) = build(&data::parse(&s1, &o), &settings(true)).unwrap();
        let Field::Grid(g) = &f else { panic!() };
        assert_eq!((g.nx(), g.ny()), (2, 2));
        let s6 = sample_file("sample6.txt").unwrap();
        let (f, _) = build(&data::parse(&s6, &o), &settings(false)).unwrap();
        let Field::Mesh(m) = &f else { panic!() };
        assert!(!m.tris.is_empty());
        let s4 = sample_file("sample4.txt").unwrap();
        let po = DataOptions { panel: true, ..o };
        assert!(matches!(build(&data::parse(&s4, &po), &settings(false)).unwrap().0, Field::Panel(_)));
    }
}
