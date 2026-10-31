//! Control-name keyed value store (the names double as INI keys).
use crate::forms::Node;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Ctl {
    pub checked: bool,
    pub text: String,
    pub item_index: i32,
    pub position: i32,
    pub color: u32,
    pub enabled: bool,
    pub readonly: bool,
}

impl Default for Ctl {
    fn default() -> Self {
        Ctl { checked: false, text: String::new(), item_index: 0, position: 0, color: 0xff_ffff, enabled: true, readonly: false }
    }
}

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Store {
    pub map: HashMap<String, Ctl>,
    /// list box contents (LBfont rows are `name\tfont\tsize\tstyle`)
    pub lists: HashMap<String, Vec<String>>,
}

impl Store {
    pub fn from_forms(nodes: &[&Node]) -> Store {
        let mut s = Store::default();
        for n in nodes {
            s.init(n);
        }
        s.map.entry("LBfont".into()).or_default().item_index = 3;
        s
    }

    fn init(&mut self, n: &Node) {
        let mut c = Ctl::default();
        let mut keep = true;
        match n.class.as_str() {
            "TCheckBox" | "TRadioButton" => {
                c.checked = n.flag("Checked", false) || n.ident("State") == "cbChecked";
            }
            "TEdit" => c.text = n.str("Text").to_string(),
            "TSpinEdit" => {
                c.position = n.int("Value");
                c.text = c.position.to_string();
            }
            "TTrackBar" => c.position = n.int("Position"),
            "TComboBox" => c.item_index = n.int_opt("ItemIndex").unwrap_or(0),
            "TListBox" => {
                c.item_index = -1;
                self.lists.insert(n.name.clone(), n.strings("Items.Strings"));
            }
            "TPageControl" => {
                let act = n.ident("ActivePage");
                let sheets: Vec<&Node> = n.kids.iter().filter(|k| k.class == "TTabSheet").collect();
                c.item_index = sheets.iter().position(|k| k.name == act).unwrap_or(0) as i32;
            }
            "TShape" => c.color = n.color("Brush.Color").unwrap_or(0xff_ffff),
            "TToolButton" => c.checked = n.flag("Down", false),
            _ => keep = false,
        }
        if keep {
            c.readonly = n.flag("ReadOnly", false);
            c.enabled = n.flag("Enabled", true);
            self.map.insert(n.name.clone(), c);
        }
        for k in &n.kids {
            self.init(k);
        }
    }

    pub fn get(&self, name: &str) -> Option<&Ctl> {
        self.map.get(name)
    }
    pub fn entry(&mut self, name: &str) -> &mut Ctl {
        self.map.entry(name.to_string()).or_default()
    }
    pub fn checked(&self, name: &str) -> bool {
        self.map.get(name).map(|c| c.checked).unwrap_or(false)
    }
    pub fn set_checked(&mut self, name: &str, v: bool) {
        self.entry(name).checked = v;
    }
    pub fn text(&self, name: &str) -> &str {
        self.map.get(name).map(|c| c.text.as_str()).unwrap_or("")
    }
    pub fn set_text(&mut self, name: &str, v: &str) {
        self.entry(name).text = v.to_string();
    }
    pub fn position(&self, name: &str) -> i32 {
        self.map.get(name).map(|c| c.position).unwrap_or(0)
    }
    pub fn set_position(&mut self, name: &str, v: i32) {
        let c = self.entry(name);
        c.position = v;
        c.text = v.to_string();
    }
    pub fn item_index(&self, name: &str) -> i32 {
        self.map.get(name).map(|c| c.item_index).unwrap_or(0)
    }
    pub fn set_item_index(&mut self, name: &str, v: i32) {
        self.entry(name).item_index = v;
    }
    pub fn color(&self, name: &str) -> u32 {
        self.map.get(name).map(|c| c.color).unwrap_or(0)
    }
    pub fn set_color(&mut self, name: &str, v: u32) {
        self.entry(name).color = v;
    }
    pub fn readonly(&self, name: &str) -> bool {
        self.map.get(name).map(|c| c.readonly).unwrap_or(false)
    }
    pub fn set_readonly(&mut self, name: &str, v: bool) {
        self.entry(name).readonly = v;
    }
    pub fn enabled(&self, name: &str) -> bool {
        self.map.get(name).map(|c| c.enabled).unwrap_or(true)
    }
    pub fn set_enabled(&mut self, name: &str, v: bool) {
        self.entry(name).enabled = v;
    }
    pub fn list(&self, name: &str) -> &[String] {
        self.lists.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }
}
