//! Modal dialogs (settings, batch list, label input, about, mesh progress) shown as OS windows.
use crate::batch;
use crate::forms::{Forms, Node};
use crate::labels;
use crate::platform_win32 as plat;
use crate::state::Store;
use crate::vcl::form::{Assets, Ev, Fnt, Renderer};
use crate::vcl::FACE;
use crate::view::{Item, ViewState, DIALOG_NAMES};
use egui::{Context, Pos2, Vec2, ViewportBuilder, ViewportId};

pub const T_CONFIG: &str = "設定";
pub const T_AVI: &str = "連続処理";
pub const T_INPUT: &str = "ラベルの追加";
pub const T_ABOUT: &str = "バージョン情報";
// the original's caption is 三角メッシュ自動生成, but this window serves loading and batch jobs too
pub const T_MESH: &str = "処理中";

fn title(source: &'static str) -> String { crate::i18n::text(source) }

const OPEN_FILTERS: [(&str, &[&str]); 4] = [
    ("aveseデータファイル(*.txt;*.dat)", &["txt", "dat"]),
    ("テキストファイル(*.txt)", &["txt"]),
    ("データファイル(*.dat)", &["dat"]),
    ("すべてのファイル(*.*)", &["*"]),
];

pub fn notice(title: &str, text: &str) {
    rfd::MessageDialog::new().set_title(crate::i18n::text(title)).set_description(crate::i18n::text(text)).set_level(rfd::MessageLevel::Info).show();
}

pub fn warn(title: &str, text: &str) {
    rfd::MessageDialog::new().set_title(crate::i18n::text(title)).set_description(crate::i18n::text(text)).set_level(rfd::MessageLevel::Warning).show();
}

pub fn error(title: &str, text: &str) {
    rfd::MessageDialog::new().set_title(crate::i18n::text(title)).set_description(crate::i18n::text(text)).set_level(rfd::MessageLevel::Error).show();
}

/// Yes/No question; true for Yes.
pub fn confirm(title: &str, text: &str) -> bool {
    let r = rfd::MessageDialog::new().set_title(crate::i18n::text(title)).set_description(crate::i18n::text(text)).set_level(rfd::MessageLevel::Warning).set_buttons(rfd::MessageButtons::YesNo).show();
    matches!(r, rfd::MessageDialogResult::Yes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrregularChoice { Generate, Abort, Ignore }

pub fn pick_open_files() -> Vec<std::path::PathBuf> {
    let mut d = rfd::FileDialog::new().set_title(title("データロード"));
    for (n, e) in OPEN_FILTERS {
        d = d.add_filter(crate::i18n::text(n), e);
    }
    d.pick_files().unwrap_or_default()
}

pub fn pick_ini_open() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new().set_title(title("設定の読込")).add_filter(title("設定ファイル(*.ini)"), &["ini"]).pick_file()
}

pub fn pick_ini_save() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new().set_title(title("設定に名前を付けて保存")).add_filter(title("設定ファイル(*.ini)"), &["ini"]).set_file_name("avese.ini").save_file()
}

/// Read-only/colour interlocks between the settings checkboxes and their edit boxes.
pub fn apply_interlocks(s: &mut Store) {
    for (cb, ed) in [("CBenVMax", "EDenVMax"), ("CBentokomin", "EDtokomin"), ("CBentokomax", "EDtokomax"), ("CBenZmin", "EDenZmin"), ("CBenZmax", "EDenZmax")] {
        let on = s.checked(cb);
        s.set_readonly(ed, !on);
    }
    for ed in ["EDenXmin", "EDenXmax", "EDenYmin", "EDenYmax"] {
        s.set_readonly(ed, true);
    }
    // SEAVIwitdh/SEAVIheight stay enabled: the DFM has no Enabled=False nor a CBAVIsize handler, and orig_batch.png shows them enabled.
}

pub enum BatchReq {
    Bmp(std::path::PathBuf),
    Avi(std::path::PathBuf),
}

enum Picker {
    Color { key: String, value: egui::Color32 },
    Font { row: usize, name: String, size: i32, style: i32 },
}

#[derive(Default)]
pub struct Dialogs {
    pub config: Option<Store>, // snapshot restored on cancel
    pub avi: bool,
    pub input: bool,
    pub about: bool,
    pub mesh: bool,
    /// the progress window was closed while a job runs
    pub mesh_cancel: bool,
    /// set when the settings dialog was closed with OK
    pub config_ok: bool,
    /// a job is running: the batch dialog ignores its buttons
    pub busy: bool,
    /// request from the batch dialog, taken by the app
    pub batch_req: Option<BatchReq>,
    /// Completion message displayed inside the batch window after the root is restored.
    pub batch_notice: Option<String>,
    picker: Option<Picker>,
    pub main_hwnd: Option<isize>,
    styled: Vec<String>,
    blocked: Vec<isize>,
}

pub struct Shared<'a> {
    pub forms: &'a Forms,
    pub store: &'a mut Store,
    pub assets: &'a Assets,
    pub center: Pos2,
}

fn viewport(ctx: &Context, key: &str, title: &str, size: Vec2, min: Option<Vec2>, resizable: bool, center: Pos2, mut body: impl FnMut(&mut egui::Ui) -> bool) -> bool {
    let mut vb = ViewportBuilder::default()
        .with_title(title)
        .with_inner_size(size)
        .with_resizable(resizable)
        .with_position(Pos2::new((center.x - size.x / 2.0).max(0.0), (center.y - size.y / 2.0).max(0.0)));
    if !resizable {
        vb = vb.with_minimize_button(false).with_maximize_button(false);
    }
    if let Some(m) = min {
        vb = vb.with_min_inner_size(m);
    }
    let mut close = false;
    ctx.show_viewport_immediate(ViewportId::from_hash_of(key), vb, |ctx, _| {
        if ctx.input(|i| i.viewport().close_requested()) {
            close = true;
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(FACE)).show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            if body(ui) {
                close = true;
            }
        });
    });
    close
}

/// Consume an unmodified Escape so one press dismisses only the active window.
fn escape_pressed(ctx: &Context) -> bool {
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
}

fn client_size(n: &Node) -> Vec2 {
    let w = n.int_opt("ClientWidth").unwrap_or(n.int("Width"));
    let h = n.int_opt("ClientHeight").unwrap_or(n.int("Height"));
    Vec2::new(w as f32, h as f32)
}

fn find_mut<'a>(n: &'a mut Node, name: &str) -> Option<&'a mut Node> {
    if n.name == name {
        return Some(n);
    }
    n.kids.iter_mut().find_map(|k| find_mut(k, name))
}

fn set_prop(n: &mut Node, k: &str, v: i32) {
    n.props.insert(k.into(), serde_json::json!(v));
}

impl Dialogs {
    pub fn any(&self) -> bool {
        self.config.is_some() || self.avi || self.input || self.about || self.mesh
    }

    /// Window that must receive the modal state for a native call (colour dialog, message box).
    pub fn top_hwnd(&self) -> Option<isize> {
        let order: [(bool, String); 5] = [(self.input, title(T_INPUT)), (self.config.is_some(), title(T_CONFIG)), (self.avi, title(T_AVI)), (self.about, title(T_ABOUT)), (self.mesh, title(T_MESH))];
        for (on, t) in order {
            if on {
                if let Some(h) = plat::find(Some(&t)) {
                    return Some(h);
                }
            }
        }
        self.main_hwnd
    }

    /// Run a blocking native call with the topmost window disabled.
    pub fn modal<R>(&self, f: impl FnOnce() -> R) -> R {
        let top = self.top_hwnd();
        if let Some(t) = top {
            plat::enable(t, false);
        }
        let r = f();
        if let Some(t) = top {
            plat::enable(t, true);
            plat::activate(t);
        }
        r
    }

    pub fn open_config(&mut self, store: &Store) {
        if self.config.is_none() {
            self.config = Some(store.clone());
        }
    }

    /// Per-frame: show open dialogs, apply OS ownership/modality.
    pub fn show(&mut self, ctx: &Context, sh: &mut Shared) {
        if self.config.is_some() {
            let size = client_size(&sh.forms.config);
            let mut act: Option<bool> = None;
            let snap = self.config.clone().unwrap();
            let mut want_input = false;
            let mut form = sh.forms.config.clone();
            patch_font_sample(&mut form, sh.store);
            let mut picker = self.picker.take();
            let nested_input_open = self.input;
            let this: &Dialogs = self;
            let close = viewport(ctx, "config", &title(T_CONFIG), size, None, false, sh.center, |ui| {
                let blocked = nested_input_open || picker.is_some();
                let (a, inp) = ui.add_enabled_ui(!blocked, |ui| config_body(ui, &form, &sh.forms.config, sh.store, sh.assets, this, &mut picker)).inner;
                show_picker(ui, sh.store, &mut picker);
                if a.is_some() {
                    act = a;
                }
                want_input |= inp;
                false
            });
            self.picker = picker;
            if want_input {
                self.input = true;
            }
            if close && (nested_input_open || self.picker.is_some()) {
                ctx.send_viewport_cmd_to(ViewportId::from_hash_of("config"), egui::ViewportCommand::CancelClose);
            }
            // A nested label or picker owns this dialog until it is dismissed,
            // including on platforms without native owner-window disabling.
            match (act, close && !nested_input_open && self.picker.is_none()) {
                (Some(true), _) => {
                    self.config = None;
                    self.config_ok = true;
                }
                (Some(false), _) | (None, true) => {
                    *sh.store = snap;
                    self.config = None;
                }
                _ => {}
            }
            if self.config.is_none() { self.picker = None; }
        }
        if self.input {
            let form = &sh.forms.input;
            let size = client_size(form);
            let mut done: Option<bool> = None;
            let c = viewport(ctx, "input", &title(T_INPUT), size, None, false, sh.center, |ui| {
                let origin = ui.max_rect().min;
                let mut r = Renderer::new(ui, sh.store, sh.assets);
                r.draw_children(form, origin, &Fnt::form_default().with(form));
                let key = r.key_button(form);
                let mut evs = r.finish();
                if let Some(k) = key {
                    evs.push(Ev::Click(k));
                }
                for e in evs {
                    if let Ev::Click(n) = e {
                        match n.as_str() {
                            "OKBtn" | "OK" | "OKButton" => done = Some(true),
                            "CancelBtn" | "Cancel" | "CancelButton" => done = Some(false),
                            _ => {}
                        }
                    }
                }
                false
            });
            if done == Some(true) {
                match labels::from_input(sh.store.text("EDX"), sh.store.text("EDY"), sh.store.text("EDstr")) {
                    Ok(l) => {
                        let list = sh.store.lists.entry("LBlabel".into()).or_default();
                        list.push(labels::format_line(&l));
                        let n = list.len() as i32 - 1;
                        sh.store.set_item_index("LBlabel", n);
                    }
                    Err(e) => {
                        self.modal(|| notice("avese", &e));
                        done = None;
                    }
                }
            }
            if done.is_some() || c {
                self.input = false;
            }
        }
        if self.avi {
            let form = sh.forms.avi.clone();
            let size = client_size_avi(&form);
            let mut notice = self.batch_notice.take();
            let me: &Dialogs = self;
            let mut req = None;
            let c = viewport(ctx, "avi", &title(T_AVI), size, Some(Vec2::new(292.0, 171.0)), true, sh.center, |ui| {
                req = ui.add_enabled_ui(notice.is_none() && !me.busy, |ui| avi_body(ui, &form, sh.store, sh.assets, me)).inner;
                if let Some(text) = notice.as_deref() {
                    let mut close_notice = false;
                    egui::Modal::new(egui::Id::new("batch-complete")).show(ui.ctx(), |ui| {
                        ui.label(text);
                        close_notice = ui.button("OK").clicked() || escape_pressed(ui.ctx());
                    });
                    if close_notice { notice = None; }
                    false
                } else {
                    !me.busy && escape_pressed(ui.ctx())
                }
            });
            self.batch_notice = notice;
            if c && self.busy {
                ctx.send_viewport_cmd_to(ViewportId::from_hash_of("avi"), egui::ViewportCommand::CancelClose);
            }
            if req.is_some() {
                self.batch_req = req;
            }
            if c && !self.busy {
                self.avi = false;
            }
        }
        if self.about {
            let form = &sh.forms.about;
            let c = viewport(ctx, "about", &title(T_ABOUT), client_size(form), None, false, sh.center, |ui| {
                let origin = ui.max_rect().min;
                let mut r = Renderer::new(ui, sh.store, sh.assets);
                r.draw_children(form, origin, &Fnt::form_default().with(form));
                let key = r.key_button(form);
                let popup_open = r.popup_open;
                let evs = r.finish();
                key.is_some() || (!popup_open && escape_pressed(ui.ctx()))
                    || evs.iter().any(|e| matches!(e, Ev::Click(n) if n == "OKButton"))
            });
            if c {
                self.about = false;
            }
        }
        if self.mesh {
            let form = &sh.forms.mesh;
            let c = viewport(ctx, "mesh", &title(T_MESH), client_size(form), None, false, sh.center, |ui| {
                let origin = ui.max_rect().min;
                let mut r = Renderer::new(ui, sh.store, sh.assets);
                r.draw_children(form, origin, &Fnt::form_default().with(form));
                r.finish();
                escape_pressed(ui.ctx())
            });
            if c {
                self.mesh = false;
                self.mesh_cancel = true;
            }
        }
        self.sync_native();
    }

    /// Style new dialog windows (owner, dialog frame) and keep owners disabled while a dialog is open.
    fn sync_native(&mut self) {
        let open: [(bool, String); 5] = [(self.config.is_some(), title(T_CONFIG)), (self.avi, title(T_AVI)), (self.input, title(T_INPUT)), (self.about, title(T_ABOUT)), (self.mesh, title(T_MESH))];
        for (on, t) in open {
            if on && !self.styled.contains(&t) {
                if let Some(h) = plat::find(Some(&t)) {
                    let owner = if t == title(T_INPUT) { plat::find(Some(&title(T_CONFIG))).or(self.main_hwnd) } else { self.main_hwnd };
                    if t != title(T_AVI) {
                        plat::style_dialog(h, owner);
                    } else if let Some(o) = owner {
                        plat::style_dialog(h, Some(o));
                    }
                    if let Some(o) = owner {
                        plat::enable(o, false);
                        self.blocked.push(o);
                    }
                    self.styled.push(t);
                }
            } else if !on && self.styled.contains(&t) {
                self.styled.retain(|x| *x != t);
            }
        }
        // re-enable windows that no longer have an open dialog above them
        let main_needed = self.any();
        let cfg_needed = self.input;
        let mut keep = vec![];
        for h in std::mem::take(&mut self.blocked) {
            let is_main = Some(h) == self.main_hwnd;
            if (is_main && main_needed) || (!is_main && cfg_needed) {
                keep.push(h);
            } else {
                plat::enable(h, true);
                if is_main {
                    plat::activate(h);
                }
            }
        }
        self.blocked = keep;
    }
}

fn client_size_avi(form: &Node) -> Vec2 {
    // Width/Height are window sizes (326x396) with an 8px/29px frame
    Vec2::new(form.int("Width") as f32 - 8.0, form.int("Height") as f32 - 29.0)
}

/// Selected row of LBfont drives the sample label font.
fn patch_font_sample(form: &mut Node, store: &Store) {
    let sel = store.item_index("LBfont").max(0) as usize;
    if let Some(row) = store.list("LBfont").get(sel) {
        let c: Vec<&str> = row.split('\t').collect();
        if let Some(l) = find_mut(form, "Labfont") {
            if let Some(name) = c.get(1) {
                l.props.insert("Font.Name".into(), serde_json::json!(name));
            }
            if let Some(sz) = c.get(2).and_then(|s| s.parse::<i32>().ok()) {
                set_prop(l, "Font.Height", -(sz * 96 / 72));
            }
        }
    }
}

/// Returns (Some(ok) when closed by a button, wants the label input dialog).
fn config_body(ui: &mut egui::Ui, form: &Node, orig: &Node, store: &mut Store, assets: &Assets, d: &Dialogs, picker: &mut Option<Picker>) -> (Option<bool>, bool) {
    let enabled = ui.is_enabled();
    let origin = ui.max_rect().min;
    apply_interlocks(store);
    let mut r = Renderer::new(ui, store, assets);
    r.draw_children(form, origin, &Fnt::form_default().with(orig));
    let key = if enabled { r.key_button(orig) } else { None };
    let mut evs = r.finish();
    if !enabled { evs.clear(); }
    if let Some(k) = key {
        evs.push(Ev::Click(k));
    }
    let mut result = None;
    let mut input = false;
    let owner = d.top_hwnd();
    for e in evs {
        match e {
            Ev::Click(n) => match n.as_str() {
                "OKBtn" => result = Some(true),
                "CancelBtn" => result = Some(false),
                "BTDefAngle" => {
                    store.set_position("SEeta", 30);
                    store.set_position("SEphi", 30);
                    store.set_position("TBeta", 30);
                    store.set_position("TBphi", 30);
                }
                "BTlabellistadd" => input = true,
                "BTlabellistdel" => {
                    let i = store.item_index("LBlabel");
                    let l = store.lists.entry("LBlabel".into()).or_default();
                    if i >= 0 && (i as usize) < l.len() {
                        l.remove(i as usize);
                        let n = l.len() as i32;
                        store.set_item_index("LBlabel", i.min(n - 1));
                    }
                }
                "CCBvector" | "CCBvectorhead" | "CCBtoko" | "CCBback" | "CCBfront" => {
                    let sp = match n.as_str() {
                        "CCBvector" => "SPvectorcolor",
                        "CCBvectorhead" => "SPvectorheadcolor",
                        "CCBtoko" => "SPtokocolor",
                        "CCBback" => "SPBGcolor",
                        _ => "SPFGcolor",
                    };
                    #[cfg(windows)]
                    {
                        if let Some(c) = plat::choose_color(owner, store.color(sp)) { store.set_color(sp, c); }
                    }
                    #[cfg(not(windows))]
                    {
                        let c = crate::engine::scene::Rgb::from_tcolor(store.color(sp));
                        *picker = Some(Picker::Color { key: sp.into(), value: egui::Color32::from_rgb(c.0, c.1, c.2) });
                    }
                }
                "SpeedButton1" => {
                    #[cfg(windows)]
                    { font_dialog(store, owner); }
                    #[cfg(not(windows))]
                    { *picker = font_picker(store); }
                }
                other => {
                    // checkboxes/radios of the view items share the menu rules
                    if let Some((item, name)) = DIALOG_NAMES.iter().find(|(_, nm)| *nm == other) {
                        let mut vs = ViewState::from_store(store);
                        let on = store.checked(name);
                        if matches!(item, Item::Cart | Item::Cyl) {
                            vs.set(*item, true);
                        } else {
                            vs.set(*item, on);
                        }
                        vs.write_store(store);
                    }
                    if other == "CBaxisscale" && store.checked("CBaxisscale") {
                        store.set_checked("CBgridscale", true);
                    }
                }
            },
            Ev::Change(n) => match n.as_str() {
                "LBfont:dbl" => {
                    #[cfg(windows)]
                    { font_dialog(store, owner); }
                    #[cfg(not(windows))]
                    { *picker = font_picker(store); }
                }
                "TBeta" => {
                    let v = store.position("TBeta");
                    store.set_position("SEeta", v);
                    store.set_text("SEeta", &v.to_string());
                }
                "TBphi" => {
                    let v = store.position("TBphi");
                    store.set_position("SEphi", v);
                    store.set_text("SEphi", &v.to_string());
                }
                "SEeta" => {
                    let v = store.position("SEeta");
                    store.set_position("TBeta", v);
                }
                "SEphi" => {
                    let v = store.position("SEphi");
                    store.set_position("TBphi", v);
                }
                _ => {}
            },
        }
    }
    apply_interlocks(store);
    (result, input)
}

fn font_picker(store: &Store) -> Option<Picker> {
    let row = store.item_index("LBfont");
    if row < 0 { return None; }
    let row = row as usize;
    let fields = store.list("LBfont").get(row)?.split('\t').map(str::to_owned).collect::<Vec<_>>();
    Some(Picker::Font {
        row,
        name: fields.get(1).cloned().unwrap_or_default(),
        size: fields.get(2).and_then(|s| s.parse().ok()).unwrap_or(9),
        style: fields.get(3).and_then(|s| s.parse().ok()).unwrap_or(0),
    })
}

fn show_picker(ui: &mut egui::Ui, store: &mut Store, picker: &mut Option<Picker>) {
    let Some(active) = picker.as_mut() else { return };
    // An open color or font popup gets the first Escape; a subsequent press
    // cancels the picker without applying its temporary values.
    let popup_open = egui::Popup::is_any_open(ui.ctx());
    let mut accepted = false;
    let mut cancelled = false;
    match active {
        Picker::Color { key, value } => {
            let response = egui::Window::new(title("色の選択")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ui.ctx(), |ui| {
                egui::color_picker::color_edit_button_srgba(ui, value, egui::color_picker::Alpha::Opaque);
                ui.horizontal(|ui| { accepted = ui.button("OK").clicked(); cancelled = ui.button(title("キャンセル")).clicked(); });
            });
            cancelled |= response.is_none();
            if accepted {
                let [r, g, b, _] = value.to_array();
                store.set_color(key, r as u32 | ((g as u32) << 8) | ((b as u32) << 16));
            }
        }
        Picker::Font { row, name, size, style } => {
            let response = egui::Window::new(title("フォントの選択")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO).show(ui.ctx(), |ui| {
                ui.label(title("フォント名"));
                ui.text_edit_singleline(name);
                egui::ComboBox::from_label(title("候補")).selected_text(title("フォント候補")).show_ui(ui, |ui| {
                    for family in ["ＭＳ Ｐゴシック", "ＭＳ ゴシック", "Noto Sans CJK JP", "IPAexゴシック", "游ゴシック", "Hiragino Sans", "Arial"] {
                        if ui.selectable_label(*name == family, family).clicked() { *name = family.into(); }
                    }
                });
                ui.add(egui::Slider::new(size, 1..=96).text(title("サイズ")));
                for (label, bit) in [("太字", 1), ("斜体", 2), ("下線", 4), ("取消線", 8)] {
                    let mut on = *style & bit != 0;
                    if ui.checkbox(&mut on, title(label)).changed() {
                        if on { *style |= bit; } else { *style &= !bit; }
                    }
                }
                ui.horizontal(|ui| { accepted = ui.button("OK").clicked(); cancelled = ui.button(title("キャンセル")).clicked(); });
            });
            cancelled |= response.is_none();
            if accepted {
                if let Some(line) = store.lists.get_mut("LBfont").and_then(|rows| rows.get_mut(*row)) {
                    let caption = line.split('\t').next().unwrap_or("");
                    *line = format!("{caption}\t{name}\t{size}\t{style}");
                }
            }
        }
    }
    cancelled |= !popup_open && escape_pressed(ui.ctx());
    if accepted || cancelled { *picker = None; }
}

fn font_dialog(store: &mut Store, owner: Option<isize>) {
    let sel = store.item_index("LBfont");
    if sel < 0 {
        return;
    }
    let sel = sel as usize;
    let Some(row) = store.list("LBfont").get(sel).cloned() else { return };
    let c: Vec<String> = row.split('\t').map(String::from).collect();
    let name = c.get(1).cloned().unwrap_or_default();
    let size = c.get(2).and_then(|s| s.parse().ok()).unwrap_or(9);
    let style = c.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    if let Some((n, s, st)) = plat::choose_font(owner, &name, size, style) {
        let new = format!("{}\t{}\t{}\t{}", c.first().cloned().unwrap_or_default(), n, s, st);
        if let Some(r) = store.lists.get_mut("LBfont").and_then(|l| l.get_mut(sel)) {
            *r = new;
        }
    }
}

/// Draws the batch dialog and runs its list buttons; BMP/AVI creation is returned as a request.
fn avi_body(ui: &mut egui::Ui, form: &Node, store: &mut Store, assets: &Assets, d: &Dialogs) -> Option<BatchReq> {
    let rect = ui.max_rect();
    let (w, h) = (rect.width().round() as i32, rect.height().round() as i32);
    let mut f = form.clone();
    if let Some(p) = find_mut(&mut f, "Panel1") {
        set_prop(p, "Top", h - 100);
        set_prop(p, "Width", w);
    }
    if let Some(b) = find_mut(&mut f, "Bevel1") {
        set_prop(b, "Width", w);
    }
    if let Some(l) = find_mut(&mut f, "savelist") {
        set_prop(l, "Width", w);
        set_prop(l, "Height", h - 100);
    }
    apply_interlocks(store);
    // dropped files are appended to the list
    let dropped: Vec<std::path::PathBuf> = ui.ctx().input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
    if !d.busy {
        for p in dropped {
            store.lists.entry("savelist".into()).or_default().push(p.display().to_string());
        }
    }
    let mut r = Renderer::new(ui, store, assets);
    r.draw_children(&f, rect.min, &Fnt::form_default().with(form));
    let evs = r.finish();
    if d.busy {
        return None;
    }
    let mut req = None;
    for e in evs {
        let Ev::Click(n) = e else { continue };
        let sel = store.item_index("savelist");
        let list = store.lists.entry("savelist".into()).or_default();
        match n.as_str() {
            "SortList" => batch::sort_files(list),
            "listup" | "listdown" => {
                if let Some(j) = batch::move_item(list, sel, n == "listdown") {
                    store.set_item_index("savelist", j);
                }
            }
            "addlist" => {
                let files = d.modal(pick_open_files);
                for p in files {
                    list.push(p.display().to_string());
                }
            }
            "DeleteList" => {
                if let Some(i) = batch::remove_item(list, sel) {
                    store.set_item_index("savelist", i);
                }
            }
            "makeBMP" | "makeAVI" => {
                if list.is_empty() {
                    d.modal(|| notice("avese", &title("リストにファイルがありません。")));
                } else if n == "makeBMP" {
                    req = d.modal(|| rfd::FileDialog::new().set_title(title("BMP を作成するフォルダ")).pick_folder()).map(BatchReq::Bmp);
                } else {
                    let p = d.modal(|| rfd::FileDialog::new().set_title(title("AVI作成")).add_filter("AVI-Files (*.avi)", &["avi"]).set_file_name("default.avi").save_file());
                    req = p.map(|p| BatchReq::Avi(if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("avi")) { p } else { p.with_added_extension("avi") }));
                }
            }
            _ => {}
        }
    }
    req
}
