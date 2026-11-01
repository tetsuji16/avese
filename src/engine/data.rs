//! Data file parser: header directives, embedded settings, and numeric rows.
use crate::engine::settings::Settings;
use crate::state::Store;
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub u: Option<f64>,
    pub v: Option<f64>,
    pub z: Option<f64>,
}

/// 1-based column numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cols {
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub u: usize,
    pub v: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataOptions {
    /// number of leading data lines to drop
    pub skip: usize,
    pub cols: Cols,
    pub panel: bool,
    pub transpose: bool,
}

impl Default for DataOptions {
    fn default() -> Self {
        DataOptions { skip: 0, cols: Cols { x: 1, y: 2, z: 3, u: 4, v: 5 }, panel: false, transpose: false }
    }
}

impl DataOptions {
    pub fn from_settings(s: &Settings) -> DataOptions {
        DataOptions {
            skip: s.skip,
            cols: Cols { x: s.col_x, y: s.col_y, z: s.col_z, u: s.col_u, v: s.col_v },
            panel: s.panel,
            transpose: s.transpose,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PanelData {
    pub rows: usize,
    pub cols: usize,
    /// row-major, row 0 is the top of the image
    pub z: Vec<Option<f64>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub x: f64,
    pub y: f64,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataFile {
    pub points: Vec<Point>,
    pub panel: Option<PanelData>,
    pub labels: Vec<Label>,
    /// key=value lines of the [AVESE.EXE] section
    pub embedded: Vec<(String, String)>,
    pub include_ini: Option<String>,
    pub mesh_auto: bool,
}

pub fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf][..]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
    }
}

fn is_number_token(t: &str) -> bool {
    !t.is_empty()
        && t.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'))
        && t.bytes().any(|b| b.is_ascii_digit())
}

/// Some(None) for the missing marker `-`, Some(Some(v)) for a number, None otherwise.
fn parse_token(t: &str) -> Option<Option<f64>> {
    if t == "-" {
        return Some(None);
    }
    if !is_number_token(t) {
        return None;
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite()).map(Some)
}

/// A line made only of numbers and `-`; None when it is not a data line.
fn numeric_row(line: &str) -> Option<Vec<Option<f64>>> {
    let mut row = Vec::new();
    for t in line.split(|c: char| c == ' ' || c == '\t' || c == ',' || c == '\u{3000}').filter(|t| !t.is_empty()) {
        row.push(parse_token(t)?);
    }
    if row.is_empty() { None } else { Some(row) }
}

fn strip_keyword<'a>(line: &'a str, kw: &str) -> Option<&'a str> {
    let head = line.get(..kw.len())?;
    if !head.eq_ignore_ascii_case(kw) {
        return None;
    }
    let rest = &line[kw.len()..];
    if rest.is_empty() || rest.starts_with(|c: char| c == ' ' || c == '\t' || c == '\u{3000}') {
        Some(rest.trim())
    } else {
        None
    }
}

pub fn parse(bytes: &[u8], opts: &DataOptions) -> DataFile {
    let text = decode(bytes);
    let mut out = DataFile::default();
    let mut in_avese = false;
    let mut in_header = true;
    let mut to_skip = opts.skip;
    let mut rows: Vec<Vec<Option<f64>>> = Vec::new();

    for raw in text.split(|c| c == '\n' || c == '\r') {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(row) = numeric_row(line) {
            in_header = false;
            if to_skip > 0 {
                to_skip -= 1;
                continue;
            }
            rows.push(row);
            continue;
        }
        if !in_header {
            continue;
        }
        let first = line.chars().next().unwrap_or(' ');
        if let Some(sec) = line.strip_prefix('[').and_then(|l| l.split(']').next()) {
            in_avese = sec.trim().eq_ignore_ascii_case("AVESE.EXE");
            continue;
        }
        if "!\"#$%&'()*;".contains(first) {
            continue;
        }
        if in_avese {
            if let Some((k, v)) = line.split_once('=') {
                out.embedded.push((k.trim().to_string(), v.trim().to_string()));
            }
            continue;
        }
        if let Some(rest) = strip_keyword(line, "Labels") {
            let mut it = rest.splitn(3, ',');
            let x = it.next().and_then(|s| s.trim().parse::<f64>().ok());
            let y = it.next().and_then(|s| s.trim().parse::<f64>().ok());
            if let (Some(x), Some(y)) = (x, y) {
                out.labels.push(Label { x, y, text: it.next().unwrap_or("").trim().to_string() });
            }
        } else if let Some(rest) = strip_keyword(line, "IncludeINI") {
            let name = rest.trim_matches('"').trim();
            if !name.is_empty() {
                out.include_ini = Some(name.to_string());
            }
        } else if let Some(rest) = strip_keyword(line, "Mesh") {
            if rest.eq_ignore_ascii_case("AutoCreate") {
                out.mesh_auto = true;
            }
        }
    }

    if opts.panel {
        out.panel = Some(build_panel(&rows, opts.transpose));
    } else {
        let c = opts.cols;
        let at = |row: &[Option<f64>], col: usize| -> Option<f64> {
            if col == 0 { None } else { row.get(col - 1).copied().flatten() }
        };
        out.points = rows
            .iter()
            .map(|r| Point { x: at(r, c.x), y: at(r, c.y), u: at(r, c.u), v: at(r, c.v), z: at(r, c.z) })
            .collect();
    }
    out
}

fn build_panel(rows: &[Vec<Option<f64>>], transpose: bool) -> PanelData {
    let nr = rows.len();
    let nc = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut z = vec![None; nr * nc];
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            z[r * nc + c] = *v;
        }
    }
    if !transpose {
        return PanelData { rows: nr, cols: nc, z };
    }
    let mut t = vec![None; nr * nc];
    for r in 0..nr {
        for c in 0..nc {
            t[c * nr + r] = z[r * nc + c];
        }
    }
    PanelData { rows: nc, cols: nr, z: t }
}

/// Apply IncludeINI and the embedded settings to `store` (precedence: embedded, then the
/// include file, then the current values). Does nothing unless CBLoadConf is on.
/// `base_dir` is the directory of the data file. Returns warnings.
pub fn apply_embedded(store: &mut Store, df: &DataFile, base_dir: Option<&Path>) -> Vec<String> {
    let mut warnings = Vec::new();
    if !store.checked("CBLoadConf") {
        return warnings;
    }
    let mut scratch = crate::ini::Layout::default();
    if let Some(name) = &df.include_ini {
        let path = match base_dir {
            Some(d) => d.join(name),
            None => Path::new(name).to_path_buf(),
        };
        if let Err(e) = crate::ini::load_file(&path, store, &mut scratch) {
            warnings.push(format!("設定ファイル {} を読み込めません: {}", name, e));
        }
    }
    crate::ini::apply_pairs(df.embedded.iter().map(|(k, v)| (k.as_str(), v.as_str())), store, &mut scratch);
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> DataOptions {
        DataOptions::default()
    }

    #[test]
    fn basic_and_e_notation() {
        let d = parse(b"1 2 3 4 5\n1.5E-3 -1.000E+001 0400 -0380 +2\n", &opts());
        assert_eq!(d.points.len(), 2);
        assert_eq!(d.points[0].x, Some(1.0));
        assert_eq!(d.points[1].x, Some(0.0015));
        assert_eq!(d.points[1].y, Some(-10.0));
        assert_eq!(d.points[1].z, Some(400.0));
        assert_eq!(d.points[1].u, Some(-380.0));
    }

    #[test]
    fn missing_dash_and_short_rows() {
        let d = parse(b"1 2 -\n3 4\n", &opts());
        assert_eq!(d.points[0].z, None);
        assert_eq!(d.points[1].z, None);
        assert_eq!(d.points[1].u, None);
    }

    #[test]
    fn line_endings_and_separators() {
        let d = parse(b"1,2,3\r4\t5 6\r\n7,8,9\n", &opts());
        assert_eq!(d.points.len(), 3);
        assert_eq!(d.points[1].y, Some(5.0));
    }

    #[test]
    fn comments_text_and_non_numeric_lines_are_skipped() {
        let d = parse(b"# c\n! d\nhello 1 2 3\n1 2x 3\nnan 1 2\n1 2 3\n", &opts());
        assert_eq!(d.points.len(), 1);
    }

    #[test]
    fn skip_counts_data_lines() {
        let o = DataOptions { skip: 1, ..opts() };
        let d = parse(b"header\n9 9 9\n1 2 3\n", &o);
        assert_eq!(d.points.len(), 1);
        assert_eq!(d.points[0].x, Some(1.0));
    }

    #[test]
    fn column_selection() {
        let o = DataOptions { cols: Cols { x: 3, y: 1, z: 2, u: 0, v: 9 }, ..opts() };
        let d = parse(b"1 2 3\n", &o);
        assert_eq!((d.points[0].x, d.points[0].y, d.points[0].z), (Some(3.0), Some(1.0), Some(2.0)));
        assert_eq!((d.points[0].u, d.points[0].v), (None, None));
    }

    #[test]
    fn directives_only_before_numbers() {
        let src = "mesh autocreate\nINCLUDEINI \"a.ini\"\n[Data]\nLABELS 10,20,a,b\n1 2 3\nLabels 1,1,late\nMesh AutoCreate\n";
        let d = parse(src.as_bytes(), &opts());
        assert!(d.mesh_auto);
        assert_eq!(d.include_ini.as_deref(), Some("a.ini"));
        assert_eq!(d.labels, vec![Label { x: 10.0, y: 20.0, text: "a,b".into() }]);
        let d = parse(b"1 2 3\nMesh AutoCreate\nIncludeINI x\n", &opts());
        assert!(!d.mesh_auto && d.include_ini.is_none());
    }

    #[test]
    fn embedded_section() {
        let d = parse(b"#c\n[AVESE.EXE]\nCBmode=1\n# x\nEDv = 2\n[Data]\nFoo=1\n1 2 3\n", &opts());
        assert_eq!(d.embedded, vec![("CBmode".into(), "1".into()), ("EDv".into(), "2".into())]);
    }

    #[test]
    fn panel_and_transpose() {
        let o = DataOptions { panel: true, ..opts() };
        let d = parse(b"1 2 3\n4 - 6\n", &o);
        assert!(d.points.is_empty());
        let p = d.panel.unwrap();
        assert_eq!((p.rows, p.cols), (2, 3));
        assert_eq!(p.z[4], None);
        let o = DataOptions { panel: true, transpose: true, ..opts() };
        let p = parse(b"1 2 3\n4 5 6\n", &o).panel.unwrap();
        assert_eq!((p.rows, p.cols), (3, 2));
        assert_eq!(p.z, vec![Some(1.0), Some(4.0), Some(2.0), Some(5.0), Some(3.0), Some(6.0)]);
    }

    #[test]
    fn shift_jis_and_utf8() {
        let (sj, _, _) = encoding_rs::SHIFT_JIS.encode("Labels 1,2,テスト\n1 2 3\n");
        assert!(std::str::from_utf8(&sj).is_err());
        assert_eq!(parse(&sj, &opts()).labels[0].text, "テスト");
        assert_eq!(parse("Labels 1,2,テスト\n1 2 3\n".as_bytes(), &opts()).labels[0].text, "テスト");
    }

    #[test]
    fn empty_and_garbage() {
        assert!(parse(b"", &opts()).points.is_empty());
        assert!(parse(&[0xff, 0xfe, 0x00, 0x80], &opts()).points.is_empty());
    }

    #[test]
    fn embedded_precedence() {
        let mut st = Settings::default_store();
        st.set_checked("CBLoadConf", true);
        st.set_position("SEsize", 111);
        st.set_checked("CBpanel", false);
        let d = parse(b"[AVESE.EXE]\nsesize=222\nCBpanel=1\n1 2 3\n", &opts());
        assert!(apply_embedded(&mut st, &d, None).is_empty());
        assert_eq!(st.position("SEsize"), 222);
        assert!(st.checked("CBpanel"));
        st.set_checked("CBLoadConf", false);
        let d = parse(b"[AVESE.EXE]\nSEsize=5\n1 2 3\n", &opts());
        apply_embedded(&mut st, &d, None);
        assert_eq!(st.position("SEsize"), 222);
    }

    fn sample(name: &str) -> Option<Vec<u8>> {
        std::fs::read(Path::new("original avese").join(name)).ok()
    }

    #[test]
    fn original_samples() {
        let Some(s1) = sample("sample1.txt") else { return };
        assert_eq!(parse(&s1, &opts()).points.len(), 4);
        let s2 = sample("sample2.txt").unwrap();
        assert_eq!(parse(&s2, &opts()).points.len(), 441);
        let s3 = parse(&sample("sample3.txt").unwrap(), &opts());
        assert_eq!(s3.labels.len(), 2);
        assert_eq!(s3.points.len(), 441);
        assert_eq!(s3.points[0].z, Some(393.0));
        let o = DataOptions { panel: true, ..opts() };
        let s4 = parse(&sample("sample4.txt").unwrap(), &o);
        assert_eq!(s4.include_ini.as_deref(), Some("sample4.ini"));
        assert!(s4.panel.is_some_and(|p| p.rows > 1 && p.cols > 1));
        let s5 = parse(&sample("sample5.txt").unwrap(), &opts());
        assert!(s5.embedded.iter().any(|(k, _)| k == "CBmode"));
        let s6 = parse(&sample("sample6.txt").unwrap(), &opts());
        assert!(s6.mesh_auto);
        assert_eq!(s6.points.len(), 13);
    }

    #[test]
    fn original_sample4_include_applies() {
        let Some(bytes) = sample("sample4.txt") else { return };
        let d = parse(&bytes, &opts());
        let mut st = Settings::default_store();
        st.set_checked("CBLoadConf", true);
        st.set_position("SEsize", 999);
        let w = apply_embedded(&mut st, &d, Some(Path::new("original avese")));
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(st.position("SEsize"), 200);
    }
}
