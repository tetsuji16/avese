//! Form definitions (DFM-derived JSON, see assets/forms/*.json).
use serde_json::{Map, Value};

/// Horizontal scale of every dialog over its DFM width, for the wider system UI font.
pub const DIALOG_WIDEN: f32 = 1.3;

#[derive(Clone)]
pub struct Node {
    pub class: String,
    pub name: String,
    pub props: Map<String, Value>,
    pub kids: Vec<Node>,
}

impl Node {
    fn localize(&mut self, language: crate::i18n::Language) {
        for key in ["Caption", "Hint"] {
            if let Some(value) = self.props.get(key).and_then(Value::as_str) {
                let translated = crate::i18n::text_for(language, value);
                self.props.insert(key.into(), Value::String(translated));
            }
        }
        if let Some(values) = self.props.get_mut("Items.Strings").and_then(Value::as_array_mut) {
            for value in values {
                if let Some(source) = value.as_str() {
                    *value = Value::String(crate::i18n::text_for(language, source));
                }
            }
        }
        for child in &mut self.kids { child.localize(language); }
    }

    pub fn from_value(v: &Value) -> Node {
        Node {
            class: v["class"].as_str().unwrap_or("").to_string(),
            name: v["name"].as_str().unwrap_or("").to_string(),
            props: v["props"].as_object().cloned().unwrap_or_default(),
            kids: v["kids"].as_array().map(|a| a.iter().map(Node::from_value).collect()).unwrap_or_default(),
        }
    }

    pub fn parse(json: &str) -> Node {
        Node::from_value(&serde_json::from_str::<Value>(json).expect("form json"))
    }

    pub fn int_opt(&self, k: &str) -> Option<i32> {
        self.props.get(k).and_then(|v| v.as_i64()).map(|v| v as i32)
    }
    pub fn int(&self, k: &str) -> i32 {
        self.int_opt(k).unwrap_or(0)
    }
    pub fn str(&self, k: &str) -> &str {
        self.props.get(k).and_then(|v| v.as_str()).unwrap_or("")
    }
    pub fn ident(&self, k: &str) -> &str {
        self.props.get(k).and_then(|v| v.get("ident")).and_then(|v| v.as_str()).unwrap_or("")
    }
    pub fn flag(&self, k: &str, default: bool) -> bool {
        self.props.get(k).and_then(|v| v.as_bool()).unwrap_or(default)
    }
    pub fn has_set(&self, k: &str, item: &str) -> bool {
        self.props
            .get(k)
            .and_then(|v| v.get("set"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().any(|x| x.as_str() == Some(item)))
            .unwrap_or(false)
    }
    pub fn strings(&self, k: &str) -> Vec<String> {
        self.props
            .get(k)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default()
    }
    /// Provide horizontal space for the wider system UI font while retaining
    /// existing vertical alignment and the sizes of interactive targets.
    fn widen(&mut self) {
        for key in ["Left", "Width", "ClientWidth"] {
            // Keep bitmaps and graphical previews at their original aspect ratio.
            if key == "Width" && matches!(self.class.as_str(), "TImage" | "TPaintBox") { continue; }
            if let Some(value) = self.int_opt(key) {
                self.props.insert(key.into(), serde_json::json!((value as f64 * DIALOG_WIDEN as f64).round() as i32));
            }
        }
        for child in &mut self.kids { child.widen(); }
    }

    pub fn rect(&self) -> (i32, i32, i32, i32) {
        (self.int("Left"), self.int("Top"), self.int("Width"), self.int("Height"))
    }
    /// Hint text before the first `|`.
    pub fn hint(&self) -> &str {
        self.str("Hint").split('|').next().unwrap_or("")
    }
    pub fn find(&self, name: &str) -> Option<&Node> {
        if self.name == name {
            return Some(self);
        }
        self.kids.iter().find_map(|k| k.find(name))
    }
    /// Resolved colour (BGR TColor) of a colour property, if present.
    pub fn color(&self, k: &str) -> Option<u32> {
        let v = self.props.get(k)?;
        if let Some(i) = v.as_i64() {
            return Some(i as u32 & 0xff_ffff);
        }
        v.get("ident").and_then(|s| s.as_str()).map(color_from_ident)
    }
    pub fn tab_order(&self) -> i32 {
        self.int_opt("TabOrder").unwrap_or(9999)
    }
}

/// VCL colour identifier to TColor (BGR) value.
pub fn color_from_ident(id: &str) -> u32 {
    match id {
        "clBlack" | "clWindowText" | "clMenuText" | "clBtnText" => 0,
        "clWhite" | "clWindow" => 0xff_ffff,
        "clBtnFace" | "clMenu" | "clSilver" | "cl3DFace" => 0xf0_f0f0,
        "clBtnShadow" | "clGray" => 0xa0_a0a0,
        "clRed" => 0x0000ff,
        "clGreen" => 0x008000,
        "clBlue" => 0xff0000,
        "clYellow" => 0x00ffff,
        _ => 0xf0_f0f0,
    }
}

pub struct Forms {
    pub form1: Node,
    pub config: Node,
    pub avi: Node,
    pub input: Node,
    pub about: Node,
    pub mesh: Node,
}

impl Forms {
    pub fn load() -> Forms {
        Self::load_for_language(crate::i18n::Language::detect())
    }

    pub(crate) fn load_for_language(language: crate::i18n::Language) -> Forms {
        let mut config = Node::parse(include_str!("../assets/forms/TDLGCONFIG.json"));
        // measured on the reference screenshots: OK/キャンセル sit 2px above the DFM Top (290)
        for k in config.kids.iter_mut().filter(|k| k.name == "OKBtn" || k.name == "CancelBtn") {
            k.props.insert("Top".into(), serde_json::json!(288));
        }
        let mut forms = Forms {
            form1: Node::parse(include_str!("../assets/forms/TFORM1.json")),
            config,
            avi: Node::parse(include_str!("../assets/forms/TDLGAVI.json")),
            input: Node::parse(include_str!("../assets/forms/TDLGINPUTSTR.json")),
            about: Node::parse(include_str!("../assets/forms/TABOUTBOX.json")),
            mesh: Node::parse(include_str!("../assets/forms/TMESHPROGRESS.json")),
        };
        if language.is_english() {
            for form in [&mut forms.form1, &mut forms.config, &mut forms.avi, &mut forms.input, &mut forms.about, &mut forms.mesh] {
                form.localize(language);
            }
        }
        for form in [&mut forms.config, &mut forms.avi, &mut forms.input, &mut forms.about, &mut forms.mesh] { form.widen(); }
        forms
    }
}
