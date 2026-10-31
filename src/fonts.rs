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

pub fn install(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let base: Vec<String> = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();

    let mut fallback: Option<String> = None;
    for (i, (p, idx)) in FALLBACKS.iter().enumerate() {
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
        let mut chain: Vec<String> = Vec::new();
        if let Some(b) = read(path) {
            let key = format!("{name}-face");
            defs.font_data.insert(key.clone(), Arc::new(FontData::from_blob(b, idx)));
            chain.push(key);
        }
        if let Some(f) = &fallback {
            chain.push(f.clone());
        }
        chain.extend(base.iter().cloned());
        defs.families.insert(FontFamily::Name(name.into()), chain);
    }
    // system menu font (Yu Gothic UI), used by the main menu and popups
    {
        let mut chain: Vec<String> = Vec::new();
        if let Some(b) = read("C:\\Windows\\Fonts\\YuGothR.ttc") {
            defs.font_data.insert("menu-face".into(), Arc::new(FontData::from_blob(b, 1)));
            chain.push("menu-face".into());
        }
        if let Some(f) = &fallback {
            chain.push(f.clone());
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

/// Default UI font (MS PGothic 12).
pub fn ui() -> FontId {
    font("ＭＳ Ｐゴシック", 12.0)
}

/// System menu font (Yu Gothic UI 12px when installed).
pub fn menu() -> FontId {
    FontId::new(12.0, FontFamily::Name("menuui".into()))
}
