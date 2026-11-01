//! Labels added in the GUI (list `LBlabel`): "x, y  text" rows drawn in addition to the data file's `Labels`.
//! They only live in the settings list; the data file is never written.
use crate::engine::draw::axes::fmt_num;
use crate::engine::data::Label;

pub fn format_line(l: &Label) -> String {
    format!("{}, {}  {}", fmt_num(l.x), fmt_num(l.y), l.text)
}

pub fn parse_line(line: &str) -> Option<Label> {
    let (x, rest) = line.split_once(',')?;
    let rest = rest.trim_start();
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let (y, text) = rest.split_at(end);
    Some(Label { x: x.trim().parse().ok()?, y: y.trim_end_matches(',').parse().ok()?, text: text.trim().to_string() })
}

/// Validate the three edit boxes of the input dialog.
pub fn from_input(x: &str, y: &str, text: &str) -> Result<Label, String> {
    let num = |s: &str, what: &str| s.trim().parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| format!("{what}には数値を入力してください"));
    let text = text.trim();
    if text.is_empty() {
        return Err("文字列を入力してください".into());
    }
    Ok(Label { x: num(x, "X")?, y: num(y, "Y")?, text: text.to_string() })
}

/// Labels of the data file followed by those of the list; list rows that do not parse are skipped.
pub fn merged(file: &[Label], list: &[String]) -> Vec<Label> {
    file.iter().cloned().chain(list.iter().filter_map(|l| parse_line(l))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_round_trip() {
        let l = Label { x: 12.0, y: 3.5, text: "a b, c".into() };
        assert_eq!(format_line(&l), "12, 3.5  a b, c");
        assert_eq!(parse_line(&format_line(&l)), Some(l));
    }

    #[test]
    fn bad_rows_are_skipped() {
        assert_eq!(parse_line("abc"), None);
        assert_eq!(parse_line("1, x  t"), None);
        let file = [Label { x: 0.0, y: 0.0, text: "f".into() }];
        let m = merged(&file, &["5, 6  g".into(), "junk".into()]);
        assert_eq!(m.len(), 2);
        assert_eq!(m[1].text, "g");
    }

    #[test]
    fn input_is_validated() {
        assert!(from_input("1", "2", " hi ").is_ok_and(|l| l.text == "hi"));
        assert!(from_input("a", "2", "hi").is_err());
        assert!(from_input("1", "NaN", "hi").is_err());
        assert!(from_input("1", "2", "  ").is_err());
    }
}
