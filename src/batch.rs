//! Batch dialog logic that does not need a window: list editing and output planning.
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

/// Case-insensitive name order in which digit runs compare as numbers (frame2 before frame10).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let run = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.next_if(char::is_ascii_digit) {
                        s.push(c);
                    }
                    s.trim_start_matches('0').to_string()
                };
                let (na, nb) = (run(&mut a), run(&mut b));
                let o = na.len().cmp(&nb.len()).then_with(|| na.cmp(&nb));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// Sort by file name first, then by the full path.
pub fn sort_files(list: &mut [String]) {
    let name = |s: &str| Path::new(s).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| s.to_string());
    list.sort_by(|a, b| natural_cmp(&name(a), &name(b)).then_with(|| natural_cmp(a, b)));
}

/// Swap the selected row with its neighbour; returns the new selection.
pub fn move_item<T>(list: &mut [T], sel: i32, down: bool) -> Option<i32> {
    let j = if down { sel + 1 } else { sel - 1 };
    if sel < 0 || j < 0 || j as usize >= list.len() || sel as usize >= list.len() {
        return None;
    }
    list.swap(sel as usize, j as usize);
    Some(j)
}

/// Remove the selected row; returns the selection to use afterwards (-1 when the list is empty).
pub fn remove_item<T>(list: &mut Vec<T>, sel: i32) -> Option<i32> {
    if sel < 0 || sel as usize >= list.len() {
        return None;
    }
    list.remove(sel as usize);
    Some(sel.min(list.len() as i32 - 1))
}

/// One BMP per listed file in `dir`; files with the same stem get `_2`, `_3`, ...
pub fn bmp_targets(dir: &Path, files: &[String]) -> Vec<PathBuf> {
    let mut used: Vec<String> = vec![];
    files
        .iter()
        .map(|f| {
            let stem = Path::new(f).file_stem().map(|s| s.to_string_lossy().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "image".into());
            let mut name = format!("{stem}.bmp");
            let mut n = 2;
            while used.iter().any(|u| u.eq_ignore_ascii_case(&name)) {
                name = format!("{stem}_{n}.bmp");
                n += 1;
            }
            used.push(name.clone());
            dir.join(name)
        })
        .collect()
}

/// AVI frame size: the dialog's width and height when "サイズ指定" is on, else the first frame's.
pub fn avi_size(spec: Option<(i32, i32)>, first: (u32, u32)) -> (u32, u32) {
    match spec {
        Some((w, h)) if w > 0 && h > 0 => (w as u32, h as u32),
        _ => first,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn sort_is_natural_and_case_blind() {
        let mut l = v(&["d/b10.txt", "d/B2.txt", "e/a.txt", "d/b1.txt", "d/b02.txt"]);
        sort_files(&mut l);
        assert_eq!(l, v(&["e/a.txt", "d/b1.txt", "d/B2.txt", "d/b02.txt", "d/b10.txt"]));
    }

    #[test]
    fn move_and_remove_keep_the_selection_valid() {
        let mut l = v(&["a", "b", "c"]);
        assert_eq!(move_item(&mut l, 0, true), Some(1));
        assert_eq!(l, v(&["b", "a", "c"]));
        assert_eq!(move_item(&mut l, 0, false), None);
        assert_eq!(move_item(&mut l, 2, true), None);
        assert_eq!(move_item(&mut l, -1, true), None);
        assert_eq!(remove_item(&mut l, 2), Some(1));
        assert_eq!(remove_item(&mut l, 5), None);
        assert_eq!(remove_item(&mut l, 0), Some(0));
        assert_eq!(remove_item(&mut l, 0), Some(-1));
    }

    #[test]
    fn bmp_names_are_unique() {
        let t = bmp_targets(Path::new("out"), &v(&["x/a.txt", "y/A.dat", "z/b", "w/a.txt"]));
        let names: Vec<_> = t.iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(names, ["a.bmp", "A_2.bmp", "b.bmp", "a_3.bmp"]);
    }

    #[test]
    fn avi_size_follows_the_checkbox() {
        assert_eq!(avi_size(None, (10, 20)), (10, 20));
        assert_eq!(avi_size(Some((64, 48)), (10, 20)), (64, 48));
        assert_eq!(avi_size(Some((0, 48)), (10, 20)), (10, 20));
    }
}
