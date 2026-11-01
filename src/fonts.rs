//! System font loading (MS fonts are never bundled; they are read at run time if present).
use egui::{FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;

pub const PGOTHIC: &str = "pgothic";
pub const GOTHIC: &str = "gothic";
pub const PMINCHO: &str = "pmincho";
pub const MINCHO: &str = "mincho";

fn read(path: &str) -> Option<Arc<Vec<u8>>> {
    std::fs::read(path).ok().map(Arc::new)
}

/// Generic CJK fallbacks (path, face index).
const FALLBACKS: &[(&str, u32)] = &[
    ("C:\\Windows\\Fonts\\meiryo.ttc", 0),
    ("C:\\Windows\\Fonts\\YuGothR.ttc", 0),
    ("C:\\Windows\\Fonts\\yugothic.ttf", 0),
    ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 0),
    ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 0),
    ("/usr/share/fonts/truetype/fonts-japanese-gothic.ttf", 0),
    ("/usr/share/fonts/ipa-gothic/ipag.ttf", 0),
    ("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", 0),
];

fn is_japanese_fallback_filename(name: &str) -> bool {
    let name = name.to_lowercase();
    ["notosanscjk", "notoserifcjk", "notosansjp", "notoserifjp", "ipaex", "ipag", "ipamp", "ipamincho", "hiragino", "ヒラギノ", "yugoth", "yumin", "msgothic", "msmincho"]
        .iter().any(|x| name.contains(x))
}

/// Find Japanese-capable system fonts on distributions that install fonts in
/// a versioned or user-specific directory. The fixed paths above stay first
/// so the preferred family remains stable when it is available.
fn discovered_fallbacks() -> Vec<(String, u32)> {
    let mut dirs: Vec<String> = Vec::new();
    #[cfg(target_os = "linux")]
    dirs.extend([
        "/usr/share/fonts".to_owned(),
        "/usr/local/share/fonts".to_owned(),
        std::env::var("HOME").map(|h| format!("{h}/.local/share/fonts")).unwrap_or_default(),
        std::env::var("HOME").map(|h| format!("{h}/.fonts")).unwrap_or_default(),
    ]);
    #[cfg(target_os = "macos")]
    dirs.extend([
        "/System/Library/Fonts".to_owned(),
        "/Library/Fonts".to_owned(),
        std::env::var("HOME").map(|h| format!("{h}/Library/Fonts")).unwrap_or_default(),
    ]);

    let mut found = Vec::new();
    let mut pending = dirs.into_iter().filter(|p| !p.is_empty()).map(std::path::PathBuf::from).collect::<Vec<_>>();
    let mut visited = 0usize;
    while let Some(dir) = pending.pop() {
        visited += 1;
        if visited > 20_000 { break; }
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            // Do not follow symlinks: font directories can contain cycles.
            if kind.is_dir() {
                pending.push(path);
                continue;
            }
            if !kind.is_file() { continue; }
            let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("");
            if !["ttf", "ttc", "otf"].iter().any(|x| ext.eq_ignore_ascii_case(x)) {
                continue;
            }
            let name = path.file_name().and_then(|x| x.to_str()).unwrap_or("");
            if is_japanese_fallback_filename(name) {
                found.push((path.to_string_lossy().into_owned(), 0));
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

pub fn install(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let base: Vec<String> = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();

    let mut fallback: Option<String> = None;
    let mut paths = FALLBACKS.iter().map(|(p, i)| ((*p).to_owned(), *i)).collect::<Vec<_>>();
    paths.extend(discovered_fallbacks());
    for (i, (p, idx)) in paths.iter().enumerate() {
        if let Some(b) = read(p) {
            let key = format!("fallback{i}");
            defs.font_data.insert(key.clone(), Arc::new(FontData::from_blob(b, *idx)));
            fallback = Some(key);
            break;
        }
    }

    let ms = [
        (PGOTHIC, "C:\\Windows\\Fonts\\msgothic.ttc", 2u32),
        (GOTHIC, "C:\\Windows\\Fonts\\msgothic.ttc", 0),
        (PMINCHO, "C:\\Windows\\Fonts\\msmincho.ttc", 1),
        (MINCHO, "C:\\Windows\\Fonts\\msmincho.ttc", 0),
    ];
    for (name, path, idx) in ms {
        let mut preview = Vec::new();
        if let Some(b) = read(path) {
            let key = format!("{name}-face");
            defs.font_data.insert(key.clone(), Arc::new(FontData::from_blob(b, idx)));
            preview.push(key);
        }
        if let Some(f) = &fallback { preview.push(f.clone()); }
        preview.extend(base.iter().cloned());
        defs.families.insert(FontFamily::Name(format!("{name}-preview").into()), preview.clone());
        let mut chain = fallback.iter().cloned().collect::<Vec<_>>();
        chain.extend(preview);
        defs.families.insert(FontFamily::Name(name.into()), chain);
    }
    // system menu font (Yu Gothic UI), used by the main menu and popups
    {
        let mut chain: Vec<String> = Vec::new();
        if let Some(f) = &fallback { chain.push(f.clone()); }
        if let Some(b) = read("C:\\Windows\\Fonts\\YuGothR.ttc") {
            defs.font_data.insert("menu-face".into(), Arc::new(FontData::from_blob(b, 1)));
            chain.push("menu-face".into());
        }
        chain.extend(base.iter().cloned());
        defs.families.insert(FontFamily::Name("menuui".into()), chain);
    }
    // egui's own proportional family also gets the CJK fallback (tooltips, rfd-free text)
    if let Some(f) = &fallback {
        defs.families.entry(FontFamily::Proportional).or_default().push(f.clone());
    }
    ctx.set_fonts(defs);
}

/// Map a VCL font name to the loaded family.
pub fn family_for(name: &str) -> &'static str {
    match name {
        "ＭＳ ゴシック" | "MS Gothic" => GOTHIC,
        "ＭＳ Ｐ明朝" | "MS PMincho" => PMINCHO,
        "ＭＳ 明朝" | "MS Mincho" => MINCHO,
        _ => PGOTHIC,
    }
}

pub fn font(name: &str, px: f32) -> FontId {
    FontId::new(px, FontFamily::Name(family_for(name).into()))
}

/// Preserve the selected legacy face in the font preview.
pub fn preview_font(name: &str, px: f32) -> FontId {
    FontId::new(px, FontFamily::Name(format!("{}-preview", family_for(name)).into()))
}

/// Default UI font, using a system Japanese sans face when available.
pub fn ui() -> FontId {
    font("ＭＳ Ｐゴシック", 12.0)
}

/// Menu font, using the same preferred face as other controls.
pub fn menu() -> FontId {
    FontId::new(12.0, FontFamily::Name("menuui".into()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn japanese_system_font_names_include_localized_hiragino() {
        assert!(super::is_japanese_fallback_filename("ヒラギノ角ゴシック W6.ttc"));
        assert!(super::is_japanese_fallback_filename("NotoSansCJK-Regular.ttc"));
        assert!(!super::is_japanese_fallback_filename("DejaVuSans.ttf"));
    }
}
