//! "画像のセーブ" dialog. On Windows the common dialog is called directly so that the chosen
//! filter (plain or RLE BMP share an extension) tells the format; elsewhere rfd is used.
use crate::engine::export::ImageFormat;
use std::path::{Path, PathBuf};

/// Filter list in dialog order; the 1-based index of the selected entry is what the dialog reports.
pub const FILTERS: [(&str, &str, ImageFormat); 5] = [
    ("ビットマップファイル(*.bmp)", "bmp", ImageFormat::Bmp),
    ("RLE 圧縮 ビットマップファイル(*.bmp)", "bmp", ImageFormat::RleBmp),
    ("PNGファイル(*.png)", "png", ImageFormat::Png),
    ("エンハンストメタファイル(*.emf)", "emf", ImageFormat::Emf),
    ("ウィンドウズメタファイル(*.wmf)", "wmf", ImageFormat::Wmf),
];

pub fn format_for_index(i: u32) -> Option<ImageFormat> {
    FILTERS.get((i as usize).checked_sub(1)?).map(|f| f.2)
}

/// Filter to preselect: the metafile when RBusewmf is chosen, else the plain bitmap.
pub fn default_index(use_wmf: bool) -> u32 {
    if use_wmf { 5 } else { 1 }
}

/// The typed name with the filter's extension appended when it has none that fits the format.
pub fn with_extension(path: &Path, fmt: ImageFormat) -> PathBuf {
    let want = FILTERS.iter().find(|f| f.2 == fmt).map(|f| f.1).unwrap_or("bmp");
    let ok = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(want));
    if ok {
        return path.to_path_buf();
    }
    let mut s = path.as_os_str().to_os_string();
    s.push(".");
    s.push(want);
    PathBuf::from(s)
}

pub fn pick(use_wmf: bool) -> Option<(PathBuf, ImageFormat)> {
    let (path, fmt) = imp::pick(default_index(use_wmf))?;
    let target = with_extension(&path, fmt);
    // Native overwrite confirmation applies to the entered path. If we add an
    // extension afterwards, ask again for the actual destination.
    if target != path && target.exists() {
        let answer = rfd::MessageDialog::new()
            .set_title("画像のセーブ")
            .set_description(format!("{} は既に存在します。上書きしますか？", target.display()))
            .set_level(rfd::MessageLevel::Warning)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if !matches!(answer, rfd::MessageDialogResult::Yes) {
            return None;
        }
    }
    Some((target, fmt))
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::UI::Controls::Dialogs::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    pub fn pick(index: u32) -> Option<(PathBuf, ImageFormat)> {
        let mut filter: Vec<u16> = vec![];
        for (name, ext, _) in FILTERS {
            filter.extend(wide(name));
            filter.push(0);
            filter.extend(wide(&format!("*.{ext}")));
            filter.push(0);
        }
        filter.push(0);
        let title: Vec<u16> = wide("画像のセーブ").into_iter().chain([0]).collect();
        let mut file = [0u16; 1024];
        for (i, c) in wide("default").into_iter().enumerate() {
            file[i] = c;
        }
        unsafe {
            let mut o: OPENFILENAMEW = std::mem::zeroed();
            o.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
            o.lpstrFilter = filter.as_ptr();
            o.nFilterIndex = index;
            o.lpstrFile = file.as_mut_ptr();
            o.nMaxFile = file.len() as u32;
            o.lpstrTitle = title.as_ptr();
            o.Flags = OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | OFN_EXPLORER | OFN_HIDEREADONLY;
            if GetSaveFileNameW(&mut o) == 0 {
                return None;
            }
            let n = file.iter().position(|&c| c == 0).unwrap_or(file.len());
            let path = PathBuf::from(std::ffi::OsString::from_wide(&file[..n]));
            format_for_index(o.nFilterIndex).map(|f| (path, f))
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn pick(index: u32) -> Option<(PathBuf, ImageFormat)> {
        // rfd does not expose the selected filter. Ask about BMP compression
        // after choosing a BMP destination instead of presenting two identical filters.
        let initial = if index == default_index(true) { "default.wmf" } else { "default.bmp" };
        let p = rfd::FileDialog::new()
            .set_title("画像のセーブ（拡張子で形式を指定）")
            .set_file_name(initial)
            .add_filter("画像ファイル (*.bmp;*.png;*.emf;*.wmf)", &["bmp", "png", "emf", "wmf"])
            .save_file()?;
        let fmt = ImageFormat::from_extension(&p).unwrap_or(ImageFormat::Bmp);
        if fmt != ImageFormat::Bmp {
            return Some((p, fmt));
        }
        let answer = rfd::MessageDialog::new()
            .set_title("BMP の圧縮")
            .set_description("RLE 圧縮を使用しますか？")
            .set_buttons(rfd::MessageButtons::YesNoCancel)
            .show();
        match answer {
            rfd::MessageDialogResult::Yes => Some((p, ImageFormat::RleBmp)),
            rfd::MessageDialogResult::No => Some((p, ImageFormat::Bmp)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_index_maps_to_format() {
        assert_eq!(format_for_index(1), Some(ImageFormat::Bmp));
        assert_eq!(format_for_index(2), Some(ImageFormat::RleBmp));
        assert_eq!(format_for_index(3), Some(ImageFormat::Png));
        assert_eq!(format_for_index(4), Some(ImageFormat::Emf));
        assert_eq!(format_for_index(5), Some(ImageFormat::Wmf));
        assert_eq!(format_for_index(0), None);
        assert_eq!(format_for_index(6), None);
        assert_eq!(default_index(true), 5);
        assert_eq!(default_index(false), 1);
    }

    #[test]
    fn extension_is_appended_when_missing_or_wrong() {
        let p = |s: &str, f| with_extension(Path::new(s), f).to_string_lossy().to_string();
        assert_eq!(p("a", ImageFormat::Png), "a.png");
        assert_eq!(p("a.PNG", ImageFormat::Png), "a.PNG");
        assert_eq!(p("a.png", ImageFormat::RleBmp), "a.png.bmp");
        assert_eq!(p("a.v2", ImageFormat::Wmf), "a.v2.wmf");
        assert_eq!(p("a.bmp", ImageFormat::RleBmp), "a.bmp");
    }
}
