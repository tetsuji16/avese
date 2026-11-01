//! Delaunay triangulation of scattered nodes.
use delaunator::{Point, triangulate};

/// Triangulate `pts`; triangles are counter-clockwise index triples.
/// `progress` receives values from 0.0 to 1.0.
pub fn triangulate_points(pts: &[(f64, f64)], mut progress: impl FnMut(f32)) -> Result<Vec<[usize; 3]>, String> {
    progress(0.0);
    if pts.len() < 3 {
        return Err("三角形分割には 3 点以上が必要です".to_string());
    }
    if pts.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
        return Err("座標が数値ではない点があります".to_string());
    }
    let mut order: Vec<usize> = (0..pts.len()).collect();
    order.sort_by(|&a, &b| pts[a].partial_cmp(&pts[b]).unwrap_or(std::cmp::Ordering::Equal));
    if let Some(w) = order.windows(2).find(|w| pts[w[0]] == pts[w[1]]) {
        let p = pts[w[0]];
        return Err(format!("同じ座標の点があります: ({}, {})", p.0, p.1));
    }
    progress(0.3);
    let input: Vec<Point> = pts.iter().map(|&(x, y)| Point { x, y }).collect();
    let t = triangulate(&input);
    progress(0.9);
    if t.triangles.is_empty() {
        return Err("点が一直線上に並んでいるため、三角形を作れません".to_string());
    }
    let tris = t.triangles.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    progress(1.0);
    Ok(tris)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_gives_two_triangles() {
        let mut last = 0.0;
        let t = triangulate_points(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)], |p| last = p).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(last, 1.0);
        assert!(t.iter().flatten().all(|&i| i < 4));
    }

    #[test]
    fn rejects_duplicates_collinear_and_tiny_input() {
        assert!(triangulate_points(&[(0.0, 0.0), (1.0, 1.0), (0.0, 0.0)], |_| {}).is_err());
        assert!(triangulate_points(&[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0)], |_| {}).is_err());
        assert!(triangulate_points(&[(0.0, 0.0), (1.0, 1.0)], |_| {}).is_err());
        assert!(triangulate_points(&[], |_| {}).is_err());
        assert!(triangulate_points(&[(0.0, 0.0), (f64::NAN, 1.0), (2.0, 5.0)], |_| {}).is_err());
    }
}
