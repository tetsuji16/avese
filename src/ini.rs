//! INI persistence: section `[avese.exe]`, original key order, Shift-JIS, CRLF.
use crate::state::Store;
use std::path::Path;

pub const SECTION: &str = "avese.exe";

pub const KEYS: &[&str] = &[
    "Form1Top", "Form1Left", "Form1height", "Form1width", "ToolBar1Top", "ToolBar1Left", "ToolBar2Top", "ToolBar2Left", "CBmode", "SEsize", "EDv", "EDh",
    "ToolBar3Top", "ToolBar3Left", "ToolBar4Top", "ToolBar4Left", "ToolBar5Top", "ToolBar5Left", "CBland", "SElandheight", "SEeta", "SEphi", "CBsurface",
    "CBgraph", "CBscatter", "CBpanel", "SEdotsize", "CBscattemode", "SPvectorcolor", "SPvectorheadcolor", "EDvector", "EDhead", "EDstepx", "EDoffsetx",
    "CBskipx", "CBskipy", "EDstepy", "EDoffsety", "CBfix", "CBpaint", "SEvectorheadangle", "CBenVMax", "EDenVMax", "CBvectorhead", "CBvector", "CBtoko",
    "SPtokocolor", "EDdt", "RBctmode1", "RBctmode2", "CBentokomin", "CBentokomax", "EDtokomin", "EDtokomax", "SEtoko", "CBaxis", "CBaxistitle", "EDXtitle",
    "EDYtitle", "EDZtitle", "SEaxisheadsize", "SEaxislinewidth", "CBaxsiscross", "RBcartesian", "RBcylindrical", "CBgrid", "SEgridlinewidth",
    "SEgridlinelength", "CBgridmesh", "CBgridbox", "CBgridscale", "CBaxisscale", "CBmemoryX", "CBmemoryY", "CBmemoryZ", "CBpoint", "CBLabel", "LBlabel",
    "SEdataX", "SEdataY", "SEdataU", "SEdataV", "SEdataZ", "CBtenchi", "SEskip", "SPBGcolor", "SPFGcolor", "EDenYmin", "EDenYmax", "EDenXmin", "EDenXmax",
    "SEmar", "LBfontlabelfontname", "LBfontlabelfontsize", "LBfontlabelfontstyle", "LBfontlegendfontname", "LBfontlegendfontsize", "LBfontlegendfontstyle",
    "LBfontAxisfontname", "LBfontAxisfontsize", "LBfontAxisfontstyle", "LBfontMemfontname", "LBfontMemfontsize", "LBfontMemfontstyle", "LBfont",
    "CBautosave", "SEmaxbmp", "RBusebmp", "RBusewmf", "CBDataCheck", "CBLoadConf", "CBSpline", "CBcolorvector", "CBlegend", "CBreverse", "CBtype", "CBfold",
    "EDenZmin", "EDenZmax", "CBenZmin", "CBenZmax", "SEcolor", "SElegendw", "SElegendh",
];

const COMBOS: [&str; 6] = ["CBmode", "CBscattemode", "CBmemoryX", "CBmemoryY", "CBmemoryZ", "CBtype"];
const FONT_ROWS: [(&str, usize); 4] = [("label", 0), ("legend", 1), ("Axis", 2), ("Mem", 3)];

/// Window geometry kept outside the store.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub form_top: i32,
    pub form_left: i32,
    pub form_height: i32,
    pub form_width: i32,
    /// (Top, Left) of ToolBar1..5
    pub bars: [(i32, i32); 5],
}

impl Default for Layout {
    fn default() -> Self {
        Layout { form_top: 104, form_left: 104, form_height: 1023, form_width: 1920, bars: [(2, 11), (2, 114), (32, 141), (32, 11), (2, 427)] }
    }
}

fn font_field(store: &Store, row: usize, col: usize) -> String {
    store.list("LBfont").get(row).and_then(|r| r.split('\t').nth(col)).unwrap_or("").to_string()
}

fn set_font_field(store: &mut Store, row: usize, col: usize, v: &str) {
    let list = store.lists.entry("LBfont".into()).or_default();
    while list.len() <= row {
        list.push(String::new());
    }
    let mut f: Vec<String> = list[row].split('\t').map(String::from).collect();
    while f.len() <= col {
        f.push(String::new());
    }
    f[col] = v.to_string();
    list[row] = f.join("\t");
}

fn get_value(key: &str, store: &Store, lay: &Layout) -> String {
    match key {
        "Form1Top" => return lay.form_top.to_string(),
        "Form1Left" => return lay.form_left.to_string(),
        "Form1height" => return lay.form_height.to_string(),
        "Form1width" => return lay.form_width.to_string(),
        "LBlabel" | "LBfont" => return store.item_index(key).to_string(),
        _ => {}
    }
    if let Some(rest) = key.strip_prefix("ToolBar") {
        let n = (rest.as_bytes()[0] - b'1') as usize;
        let (t, l) = lay.bars[n];
        return if rest.ends_with("Top") { t } else { l }.to_string();
    }
    if let Some(rest) = key.strip_prefix("LBfont") {
        for (nm, row) in FONT_ROWS {
            if let Some(f) = rest.strip_prefix(nm) {
                let col = match f {
                    "fontname" => 1,
                    "fontsize" => 2,
                    _ => 3,
                };
                return font_field(store, row, col);
            }
        }
    }
    if COMBOS.contains(&key) {
        return store.item_index(key).to_string();
    }
    if key.starts_with("CB") || key.starts_with("RB") {
        return (store.checked(key) as i32).to_string();
    }
    if key.starts_with("SE") {
        return store.position(key).to_string();
    }
    if key.starts_with("SP") {
        return store.color(key).to_string();
    }
    store.text(key).to_string()
}

fn set_value(key: &str, v: &str, store: &mut Store, lay: &mut Layout) {
    let num = v.trim().parse::<i64>().ok();
    match key {
        "Form1Top" => return num.map(|n| lay.form_top = n as i32).unwrap_or(()),
        "Form1Left" => return num.map(|n| lay.form_left = n as i32).unwrap_or(()),
        "Form1height" => return num.map(|n| lay.form_height = n as i32).unwrap_or(()),
        "Form1width" => return num.map(|n| lay.form_width = n as i32).unwrap_or(()),
        "LBlabel" | "LBfont" => return num.map(|n| store.set_item_index(key, n as i32)).unwrap_or(()),
        _ => {}
    }
    if let Some(rest) = key.strip_prefix("ToolBar") {
        if let Some(n) = num {
            let i = (rest.as_bytes()[0] - b'1') as usize;
            if rest.ends_with("Top") {
                lay.bars[i].0 = n as i32;
            } else {
                lay.bars[i].1 = n as i32;
            }
        }
        return;
    }
    if let Some(rest) = key.strip_prefix("LBfont") {
        for (nm, row) in FONT_ROWS {
            if let Some(f) = rest.strip_prefix(nm) {
                let col = match f {
                    "fontname" => 1,
                    "fontsize" => 2,
                    _ => 3,
                };
                set_font_field(store, row, col, v);
                return;
            }
        }
    }
    if COMBOS.contains(&key) {
        if let Some(n) = num {
            store.set_item_index(key, n as i32);
        }
    } else if key.starts_with("CB") || key.starts_with("RB") {
        if let Some(n) = num {
            store.set_checked(key, n != 0);
        }
    } else if key.starts_with("SE") {
        if let Some(n) = num {
            store.set_position(key, n as i32);
        }
    } else if key.starts_with("SP") {
        if let Some(n) = num {
            store.set_color(key, n as u32 & 0xff_ffff);
        }
    } else {
        store.set_text(key, v);
    }
}

pub fn to_text(store: &Store, lay: &Layout) -> String {
    let mut s = format!("[{SECTION}]\r\n");
    for k in KEYS {
        s.push_str(k);
        s.push('=');
        s.push_str(&get_value(k, store, lay));
        s.push_str("\r\n");
    }
    // UTF-8 encoded as hexadecimal keeps arbitrary Unicode and delimiters safe in
    // a Shift-JIS INI without changing the original keys or their order.
    let labels = store.list("LBlabel");
    s.push_str(&format!("LBlabelCount={}\r\n", labels.len()));
    for (i, row) in labels.iter().enumerate() {
        let hex: String = row.as_bytes().iter().map(|b| format!("{b:02X}")).collect();
        s.push_str(&format!("LBlabel{i}={hex}\r\n"));
    }
    s
}

fn decode_label_hex(s: &str) -> Option<String> {
    if s.len() % 2 != 0 || !s.is_ascii() { return None; }
    let bytes = s.as_bytes().chunks_exact(2).map(|pair| {
        let digit = |b: u8| (b as char).to_digit(16).map(|n| n as u8);
        Some(digit(pair[0])? * 16 + digit(pair[1])?)
    }).collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

pub fn apply_text(text: &str, store: &mut Store, lay: &mut Layout) {
    let mut in_sec = false;
    let mut count = None;
    let mut rows = std::collections::BTreeMap::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(sec) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_sec = sec.eq_ignore_ascii_case(SECTION);
            continue;
        }
        if !in_sec {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim();
            if key.eq_ignore_ascii_case("LBlabelCount") {
                count = v.trim().parse::<usize>().ok();
            } else if key.get(..7).is_some_and(|prefix| prefix.eq_ignore_ascii_case("LBlabel")) && key.len() > 7 {
                if let Ok(index) = key[7..].parse::<usize>() {
                    rows.insert(index, decode_label_hex(v));
                }
            } else {
                apply_pair(k, v, store, lay);
            }
        }
    }
    // Missing extension keys mean an older or partial INI: preserve current rows.
    // Reject incomplete/corrupt extensions instead of silently deleting labels.
    if let Some(n) = count {
        if rows.len() == n && (0..n).all(|i| rows.get(&i).is_some_and(Option::is_some)) {
            store.lists.insert("LBlabel".into(), (0..n).map(|i| rows.remove(&i).unwrap().unwrap()).collect());
        }
    }
}

fn apply_pair(k: &str, v: &str, store: &mut Store, lay: &mut Layout) {
    if let Some(key) = KEYS.iter().find(|x| x.eq_ignore_ascii_case(k.trim())) {
        set_value(key, v, store, lay);
    }
}

/// Apply already split `key=value` pairs (unknown keys are ignored).
pub fn apply_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>, store: &mut Store, lay: &mut Layout) {
    for (k, v) in pairs {
        apply_pair(k, v, store, lay);
    }
}

pub fn encode(text: &str) -> Vec<u8> {
    encoding_rs::SHIFT_JIS.encode(text).0.into_owned()
}

pub fn decode(bytes: &[u8]) -> String {
    encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned()
}

pub fn save_file(path: &Path, store: &Store, lay: &Layout) -> std::io::Result<()> {
    std::fs::write(path, encode(&to_text(store, lay)))
}

pub fn load_file(path: &Path, store: &mut Store, lay: &mut Layout) -> std::io::Result<()> {
    let bytes = std::fs::read(path)?;
    apply_text(&decode(&bytes), store, lay);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forms::Forms;

    // key order of the original's default.ini
    const ORIG_ORDER: &str = "Form1Top Form1Left Form1height Form1width ToolBar1Top ToolBar1Left ToolBar2Top ToolBar2Left CBmode SEsize EDv EDh ToolBar3Top ToolBar3Left ToolBar4Top ToolBar4Left ToolBar5Top ToolBar5Left CBland SElandheight SEeta SEphi CBsurface CBgraph CBscatter CBpanel SEdotsize CBscattemode SPvectorcolor SPvectorheadcolor EDvector EDhead EDstepx EDoffsetx CBskipx CBskipy EDstepy EDoffsety CBfix CBpaint SEvectorheadangle CBenVMax EDenVMax CBvectorhead CBvector CBtoko SPtokocolor EDdt RBctmode1 RBctmode2 CBentokomin CBentokomax EDtokomin EDtokomax SEtoko CBaxis CBaxistitle EDXtitle EDYtitle EDZtitle SEaxisheadsize SEaxislinewidth CBaxsiscross RBcartesian RBcylindrical CBgrid SEgridlinewidth SEgridlinelength CBgridmesh CBgridbox CBgridscale CBaxisscale CBmemoryX CBmemoryY CBmemoryZ CBpoint CBLabel LBlabel SEdataX SEdataY SEdataU SEdataV SEdataZ CBtenchi SEskip SPBGcolor SPFGcolor EDenYmin EDenYmax EDenXmin EDenXmax SEmar LBfontlabelfontname LBfontlabelfontsize LBfontlabelfontstyle LBfontlegendfontname LBfontlegendfontsize LBfontlegendfontstyle LBfontAxisfontname LBfontAxisfontsize LBfontAxisfontstyle LBfontMemfontname LBfontMemfontsize LBfontMemfontstyle LBfont CBautosave SEmaxbmp RBusebmp RBusewmf CBDataCheck CBLoadConf CBSpline CBcolorvector CBlegend CBreverse CBtype CBfold EDenZmin EDenZmax CBenZmin CBenZmax SEcolor SElegendw SElegendh";

    fn store() -> Store {
        let f = Forms::load();
        Store::from_forms(&[&f.form1, &f.config, &f.avi, &f.input])
    }

    #[test]
    fn key_order_matches_original() {
        let want: Vec<&str> = ORIG_ORDER.split(' ').collect();
        assert_eq!(KEYS, want.as_slice());
        let text = to_text(&store(), &Layout::default());
        let got: Vec<&str> = text.lines().skip(1).take(KEYS.len()).map(|l| l.split('=').next().unwrap()).collect();
        assert_eq!(got, want);
        assert!(text.starts_with("[avese.exe]\r\n"));
    }


    #[test]
    fn labels_round_trip_unicode_and_empty_list() {
        let mut original = store();
        let labels = vec!["1, 2  日本語 = 😀".to_string(), "3, 4  second".to_string()];
        original.lists.insert("LBlabel".into(), labels.clone());
        original.set_item_index("LBlabel", 1);
        let text = to_text(&original, &Layout::default());
        let mut restored = store();
        apply_text(&decode(&encode(&text)), &mut restored, &mut Layout::default());
        assert_eq!(restored.list("LBlabel"), labels);
        assert_eq!(restored.item_index("LBlabel"), 1);
        assert_eq!(to_text(&restored, &Layout::default()), text);
        original.lists.insert("LBlabel".into(), vec![]);
        apply_text(&to_text(&original, &Layout::default()), &mut restored, &mut Layout::default());
        assert!(restored.list("LBlabel").is_empty());
    }

    #[test]
    fn labels_persist_through_ini_file() {
        let path = std::env::temp_dir().join(format!("avese-labels-{}-{:?}.ini", std::process::id(), std::thread::current().id()));
        let mut original = store();
        let labels = vec!["1, 2 日本語 😀".to_string(), "3, 4 x=y".to_string()];
        original.lists.insert("LBlabel".into(), labels.clone());
        save_file(&path, &original, &Layout::default()).unwrap();
        let mut restored = store();
        load_file(&path, &mut restored, &mut Layout::default()).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(restored.list("LBlabel"), labels);
    }

    /// Every value-bearing control of the settings form reaches the ini, and every key
    /// survives a save and reload. This is the "change a value, OK, reopen" check for all
    /// 13 tabs without driving the dialog.
    #[test]
    fn every_settings_control_is_saved_and_round_trips() {
        fn collect(n: &crate::forms::Node, out: &mut Vec<String>) {
            if matches!(n.class.as_str(), "TSpinEdit" | "TEdit" | "TCheckBox" | "TRadioButton" | "TComboBox" | "TTrackBar" | "TListBox") {
                out.push(n.name.clone());
            }
            for k in &n.kids {
                collect(k, out);
            }
        }
        let forms = Forms::load();
        let mut names = Vec::new();
        collect(&forms.config, &mut names);
        assert!(names.len() > 80, "settings form controls: {}", names.len());
        for name in &names {
            // TBeta/TBphi mirror SEeta/SEphi one to one; the spins are what gets saved
            if name == "TBeta" || name == "TBphi" {
                continue;
            }
            let saved = KEYS.iter().any(|k| *k == name.as_str() || k.starts_with(&format!("{name}font")));
            assert!(saved, "{name} is not written to the ini");
        }

        let mut original = store();
        let mut lay = Layout::default();
        original.lists.insert("LBlabel".into(), vec!["1, 2 label".into(), "3, 4 other".into()]);
        for (i, key) in KEYS.iter().enumerate() {
            let v = if key.starts_with("CB") || key.starts_with("RB") {
                if i % 2 == 0 { "1".to_string() } else { "0".to_string() }
            } else if key.starts_with("SP") {
                (0x0001_0203u32 + i as u32).to_string()
            } else if *key == "LBlabel" || *key == "LBfont" {
                "1".to_string()
            } else if key.starts_with("LBfont") {
                if key.ends_with("fontname") { "ＭＳ ゴシック".to_string() } else { (i + 1).to_string() }
            } else if key.starts_with("SE") {
                (i + 11).to_string()
            } else {
                format!("t{i}")
            };
            set_value(key, &v, &mut original, &mut lay);
        }

        let text = to_text(&original, &lay);
        let mut restored = store();
        let mut restored_lay = Layout::default();
        apply_text(&decode(&encode(&text)), &mut restored, &mut restored_lay);
        assert_eq!(to_text(&restored, &restored_lay), text, "ini text changed after reload");
        assert_eq!(restored_lay, lay, "window layout changed after reload");
        for key in KEYS {
            assert_eq!(get_value(key, &restored, &restored_lay), get_value(key, &original, &lay), "{key}");
        }
    }

    #[test]
    #[ignore = "requires original samples; set AVESE_ORIGINAL_DIR"]
    fn original_sample_ini_round_trip() {
        let bytes = std::fs::read(crate::original_fixture_dir().join("sample4.ini")).expect("original sample4.ini not found");
        let mut original = store();
        let mut layout = Layout::default();
        apply_text(&decode(&bytes), &mut original, &mut layout);
        assert!(!original.checked("CBautosave"));
        assert!(original.checked("CBLoadConf"));
        assert!(original.checked("CBDataCheck"));
        assert_eq!(original.position("SEmaxbmp"), 4);
        let text = to_text(&original, &layout);
        for line in decode(&bytes).lines() {
            let Some((key, value)) = line.split_once('=') else { continue };
            let key = key.trim();
            if let Some(canonical) = KEYS.iter().find(|k| k.eq_ignore_ascii_case(key)) {
                assert_eq!(get_value(canonical, &original, &layout), value.trim_end_matches('\r'), "original key {key} changed");
            }
        }
        let mut restored = store();
        let mut restored_layout = Layout::default();
        apply_text(&decode(&encode(&text)), &mut restored, &mut restored_layout);
        assert_eq!(to_text(&restored, &restored_layout), text);
        assert_eq!(restored_layout, layout);
    }

    #[test]
    fn old_or_corrupt_extension_preserves_labels() {
        let mut s = store();
        s.lists.insert("LBlabel".into(), vec!["keep".into()]);
        for text in ["[avese.exe]\nCBpanel=1\n", "[avese.exe]\nLBlabelCount=1\nLBlabel0=ZZ\n", "[avese.exe]\nLBlabelCount=2\nLBlabel0=61\n"] {
            apply_text(text, &mut s, &mut Layout::default());
            assert_eq!(s.list("LBlabel"), &["keep"]);
        }
    }

    #[test]
    fn default_values() {
        let text = to_text(&store(), &Layout::default());
        for l in ["LBlabel=-1", "LBfont=3", "SPvectorheadcolor=16777215", "SPBGcolor=16777215", "LBfontlabelfontname=ＭＳ Ｐゴシック", "LBfontMemfontsize=6", "SEcolor=200", "EDenVMax=1.0"] {
            assert!(text.contains(l), "{l}");
        }
    }

    #[test]
    fn round_trip_shift_jis() {
        let mut s = store();
        let mut lay = Layout::default();
        s.set_text("EDXtitle", "横軸あ");
        s.set_checked("CBpanel", true);
        s.set_position("SEsize", 777);
        s.set_color("SPBGcolor", 0x123456);
        s.set_item_index("CBtype", 4);
        lay.bars[2] = (62, 99);
        lay.form_width = 640;
        let bytes = encode(&to_text(&s, &lay));
        assert!(!bytes.windows(3).any(|w| w == "横".as_bytes()), "must not be UTF-8");
        let mut s2 = store();
        let mut lay2 = Layout::default();
        apply_text(&decode(&bytes), &mut s2, &mut lay2);
        assert_eq!(lay, lay2);
        assert_eq!(to_text(&s, &lay), to_text(&s2, &lay2));
        assert_eq!(s2.text("EDXtitle"), "横軸あ");
        assert!(s2.checked("CBpanel"));
        assert_eq!(s2.item_index("CBtype"), 4);
    }
}
