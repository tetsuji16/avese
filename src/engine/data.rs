#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub u: Option<f64>,
    pub v: Option<f64>,
    pub z: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct DataConfig {
    pub skip: usize,
    pub x_col: usize,
    pub y_col: usize,
    pub u_col: usize,
    pub v_col: usize,
    pub z_col: usize,
}

pub fn parse_data(input: &str, cfg: &DataConfig) -> (Vec<Point>, Vec<String>) {
    let mut points = Vec::new();
    let mut errors = Vec::new();
    let lines: Vec<&str> = input.lines().collect();
    let mut line_no = 0usize;
    for (i, raw) in lines.iter().enumerate() {
        line_no = i + 1;
        let line = raw.trim();
        if line.is_empty() { continue; }
        if line.starts_with('#') || line.starts_with(';') { continue; }
        // skip first N data? spec: skip first `skip` lines (raw lines including comments?) 
        // 要確認: skip は生行カウントかデータ行カウントか。ここでは生行カウントで先頭 skip 行を飛ばす。
        if i < cfg.skip { continue; }
        // split
        let cells: Vec<&str> = line.split(|c: char| c == ' ' || c == '\t' || c == ',')
            .filter(|s| !s.is_empty())
            .collect();
        // parse each needed column
        let get = |col: usize| -> Option<Option<f64>> {
            if col == 0 { return Some(None); }
            let idx = col - 1;
            if idx >= cells.len() { return Some(None); }
            let cell = cells[idx].trim();
            if cell == "-" { return Some(None); }
            match cell.parse::<f64>() {
                Ok(v) => Some(Some(v)),
                Err(_) => None,
            }
        };
        // validate all requested columns parse (None column missing -> None)
        let mut ok = true;
        let x = match get(cfg.x_col) { Some(v) => v, None => { ok = false; None } };
        let y = match get(cfg.y_col) { Some(v) => v, None => { ok = false; None } };
        let u = match get(cfg.u_col) { Some(v) => v, None => { ok = false; None } };
        let v = match get(cfg.v_col) { Some(v) => v, None => { ok = false; None } };
        let z = match get(cfg.z_col) { Some(v) => v, None => { ok = false; None } };
        if !ok {
            errors.push(format!("line {}: invalid number", line_no));
            continue;
        }
        points.push(Point { x, y, u, v, z });
    }
    (points, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cfg() -> DataConfig {
        DataConfig { skip: 0, x_col: 1, y_col: 2, u_col: 3, v_col: 4, z_col: 5 }
    }
    #[test]
    fn basic() {
        let input = "1 2 3 4 5\n";
        let (pts, errs) = parse_data(input, &cfg());
        assert!(errs.is_empty());
        assert_eq!(pts.len(), 1);
        assert_eq!(pts[0].x, Some(1.0));
    }
    #[test]
    fn e_notation() {
        let input = "1.5E-3 2e+4 3 4 5\n";
        let (pts, _) = parse_data(input, &cfg());
        assert!((pts[0].x.unwrap() - 0.0015).abs() < 1e-12);
    }
    #[test]
    fn missing() {
        let input = "1 2 3 4 -\n";
        let (pts, _) = parse_data(input, &cfg());
        assert_eq!(pts[0].z, None);
    }
    #[test]
    fn comments() {
        let input = "# c\n; d\n1 2 3 4 5\n";
        let (pts, _) = parse_data(input, &cfg());
        assert_eq!(pts.len(), 1);
    }
    #[test]
    fn skip() {
        let input = "header\n1 2 3 4 5\n";
        let mut c = cfg(); c.skip = 1;
        let (pts, _) = parse_data(input, &c);
        assert_eq!(pts.len(), 1);
    }
    #[test]
    fn comma() {
        let input = "1,2,3,4,5\n";
        let (pts, _) = parse_data(input, &cfg());
        assert_eq!(pts.len(), 1);
    }
    #[test]
    fn invalid_line() {
        let input = "a b c d e\n1 2 3 4 5\n";
        let (pts, errs) = parse_data(input, &cfg());
        assert_eq!(pts.len(), 1);
        assert_eq!(errs.len(), 1);
    }
}
