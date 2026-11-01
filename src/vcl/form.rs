//! Generic renderer that draws a DFM-derived form with VCL-style controls at pixel coordinates.
use super::*;
use crate::engine::gradient::{Gradient, GradientKind};
use crate::forms::Node;
use crate::state::Store;
use egui::{Id, Key, Modifiers, Sense, TextureHandle, Ui};

pub enum Ev {
    Click(String),
    Change(String),
}

#[derive(Clone)]
pub struct Fnt {
    pub name: String,
    pub px: f32,
    pub color: u32,
}

impl Fnt {
    pub fn form_default() -> Fnt {
        Fnt { name: "ＭＳ Ｐゴシック".into(), px: 12.0, color: 0 }
    }
    pub fn with(&self, n: &Node) -> Fnt {
        let mut f = self.clone();
        if let Some(s) = n.props.get("Font.Name").and_then(|v| v.as_str()) {
            f.name = s.to_string();
        }
        if let Some(h) = n.int_opt("Font.Height") {
            f.px = h.unsigned_abs() as f32;
        }
        if let Some(c) = n.color("Font.Color") {
            f.color = c;
        }
        f
    }
    pub fn id(&self) -> FontId {
        crate::fonts::font(&self.name, self.px)
    }
}

#[derive(Default)]
pub struct Assets {
    pub plot_icons: Option<TextureHandle>,
    pub app: Option<TextureHandle>,
    pub about: Option<TextureHandle>,
    pub imagelist: Option<TextureHandle>,
    /// Icon sheet with white pixels turned into the blue/white checker of a pressed button.
    pub imagelist_chk: Option<TextureHandle>,
    pub imagelist_chk1: Option<TextureHandle>,
    /// Icon sheet as embossed (gray + white shadow) disabled glyphs.
    pub imagelist_dis: Option<TextureHandle>,
}

pub struct Renderer<'a> {
    pub ui: &'a mut Ui,
    pub store: &'a mut Store,
    pub events: Vec<Ev>,
    pub popup_open: bool,
    pub assets: &'a Assets,
    scope: Id,
    hover: Option<(Id, String)>,
    tabs: Vec<(Vec<i32>, Id)>,
    /// Controls that can take the focus, in draw order (disabled ones are left out).
    focusable: Vec<(Vec<i32>, Id)>,
    buttons: Vec<(Id, String)>,
    alt: bool,
    cur_path: Vec<i32>,
    group: Vec<String>,
}

fn tab_key(scope: Id) -> Id {
    scope.with("taborder")
}

impl<'a> Renderer<'a> {
    pub fn new(ui: &'a mut Ui, store: &'a mut Store, assets: &'a Assets) -> Self {
        let scope = ui.id();
        let ctx = ui.ctx().clone();
        // custom tab order (TabOrder), consuming egui's own Tab handling
        let (tab, shift) = if ui.is_enabled() { ctx.input_mut(|i| {
            let s = i.consume_key(Modifiers::SHIFT, Key::Tab);
            let t = i.consume_key(Modifiers::NONE, Key::Tab);
            (t, s)
        }) } else { (false, false) };
        if tab || shift {
            let list: Vec<Id> = ctx.data(|d| d.get_temp(tab_key(scope))).unwrap_or_default();
            if !list.is_empty() {
                let cur = ctx.memory(|m| m.focused()).and_then(|f| list.iter().position(|x| *x == f));
                let next = match (cur, shift) {
                    (Some(i), false) => (i + 1) % list.len(),
                    (Some(i), true) => (i + list.len() - 1) % list.len(),
                    (None, false) => 0,
                    (None, true) => list.len() - 1,
                };
                ctx.memory_mut(|m| m.request_focus(list[next]));
            }
        }
        let alt = ctx.input(|i| i.modifiers.alt);
        Renderer { ui, store, events: vec![], popup_open: false, assets, scope, hover: None, tabs: vec![], focusable: vec![], buttons: vec![], alt, cur_path: vec![], group: vec![] }
    }

    /// Store the tab order, show tooltips; returns the events.
    pub fn finish(mut self) -> Vec<Ev> {
        let ctx = self.ui.ctx().clone();
        self.tabs.sort_by(|a, b| a.0.cmp(&b.0));
        let list: Vec<Id> = self.tabs.iter().map(|t| t.1).collect();
        ctx.data_mut(|d| d.insert_temp(tab_key(self.scope), list));
        // Enter = default / focused button, handled by the caller through `default_button`
        super::tooltip::update(&ctx, self.hover.as_ref().map(|(i, s)| (*i, s.as_str())));
        if !self.events.is_empty() {
            ctx.request_repaint();
        }
        self.events
    }

    /// Enter / Esc handling: returns the name of the button to activate.
    pub fn key_button(&mut self, form: &Node) -> Option<String> {
        let ctx = self.ui.ctx().clone();
        if self.popup_open {
            return None;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            return find_flag(form, "Cancel");
        }
        if ctx.input(|i| i.key_pressed(Key::Enter)) {
            let foc = ctx.memory(|m| m.focused());
            if let Some((_, n)) = self.buttons.iter().find(|(i, _)| Some(*i) == foc) {
                return Some(n.clone());
            }
            return find_flag(form, "Default");
        }
        None
    }

    fn p(&self) -> Painter {
        self.ui.painter().clone()
    }

    pub fn draw_children(&mut self, n: &Node, org: Pos2, f: &Fnt) {
        self.draw_children_at(n, org, f, None);
    }

    /// Draw the children of `n` (optionally below a tab-order prefix).
    pub fn draw_children_at(&mut self, n: &Node, org: Pos2, f: &Fnt, prefix: Option<i32>) {
        let mut path: Vec<i32> = prefix.into_iter().collect();
        let old = self.enter_group(n);
        for k in &n.kids {
            self.draw_node(k, org, f, &mut path);
        }
        self.group = old;
    }

    fn enter_group(&mut self, n: &Node) -> Vec<String> {
        let g = n.kids.iter().filter(|k| k.class == "TRadioButton").map(|k| k.name.clone()).collect();
        std::mem::replace(&mut self.group, g)
    }

    /// Register a tooltip for a control drawn by the caller.
    pub fn tip(&mut self, id: Id, text: &str, hovered: bool) {
        if hovered && !text.is_empty() {
            self.hover = Some((id, text.to_string()));
        }
    }

    fn is_enabled(&self, n: &Node) -> bool {
        self.ui.is_enabled() && match self.store.get(&n.name) {
            Some(c) => c.enabled,
            None => n.flag("Enabled", true),
        }
    }

    fn set_hover(&mut self, n: &Node, id: Id, r: &egui::Response) {
        if r.hovered() {
            let h = n.hint();
            if !h.is_empty() {
                self.hover = Some((id, h.to_string()));
            }
        }
    }

    fn accel_hit(&self, caption: &str) -> bool {
        if !self.alt {
            return false;
        }
        let (_, _, ch) = parse_caption(caption);
        match ch.and_then(|c| Key::from_name(&c.to_ascii_uppercase().to_string())) {
            Some(k) => self.ui.ctx().input(|i| i.key_pressed(k)),
            None => false,
        }
    }

    pub fn draw_node(&mut self, n: &Node, org: Pos2, f: &Fnt, path: &mut Vec<i32>) {
        if !n.flag("Visible", true) {
            return;
        }
        let (l, t, w, h) = n.rect();
        let r = Rect::from_min_size(org + Vec2::new(l as f32, t as f32), Vec2::new(w as f32, h as f32));
        let f = f.with(n);
        path.push(n.tab_order());
        self.cur_path = path.clone();
        let p = self.p();
        let en = self.is_enabled(n);
        let id = self.scope.with(&n.name);
        // A disabled control cannot take the focus, so page focus skips it, as Delphi does.
        // A read-only edit is not interactive here, so it cannot take the focus either.
        if en && !self.store.readonly(&n.name) && matches!(n.class.as_str(), "TEdit" | "TSpinEdit" | "TCheckBox" | "TRadioButton" | "TComboBox" | "TTrackBar" | "TListBox" | "TButton" | "TSpeedButton") {
            self.focusable.push((self.cur_path.clone(), id));
        }
        match n.class.as_str() {
            "TLabel" => {
                if !n.flag("Transparent", true) {
                    fill(&p, r, n.color("Color").map(col).unwrap_or(FACE));
                }
                let h = match n.ident("Alignment") {
                    "taCenter" => HAlign::Center,
                    "taRightJustify" => HAlign::Right,
                    _ => HAlign::Left,
                };
                let vc = n.ident("Layout") == "tlCenter";
                let vb = n.ident("Layout") == "tlBottom";
                let rr = if vb { Rect::from_min_max(Pos2::new(r.min.x, r.max.y - f.px - 1.0), r.max) } else { r };
                if n.name == "Labfont" {
                    // runtime: white sample panel showing the selected list row's font (face and size in pt)
                    fill(&p, r, HILITE);
                    let sel = self.store.item_index("LBfont").max(0) as usize;
                    let row = self.store.list("LBfont").get(sel).cloned().unwrap_or_default();
                    let parts: Vec<&str> = row.split("\t").collect();
                    let pt: f32 = parts.get(2).and_then(|v| v.trim().parse().ok()).unwrap_or(12.0);
                    let face = parts.get(1).copied().unwrap_or("ＭＳ ゴシック");
                    let face = if face == "terminal" { "ＭＳ ゴシック" } else { face };
                    let fid = crate::fonts::preview_font(face, (pt * 4.0 / 3.0).round().max(10.0));
                    caption(&p, rr, n.str("Caption"), fid, BLACK, h, vc, false);
                } else {
                    caption(&p, rr, n.str("Caption"), f.id(), col(f.color), h, vc, false);
                }
            }
            "TButton" => self.button(n, r, id, &f, en),
            "TSpeedButton" => self.speed_button(n, r, id, &f, en),
            "TCheckBox" | "TRadioButton" => self.check_radio(n, r, id, &f, en, path),
            "TGroupBox" => {
                let top = Rect::from_min_max(Pos2::new(r.min.x, r.min.y + 6.0), r.max);
                etched(&p, top);
                let (s, _, _) = parse_caption(n.str("Caption"));
                if !s.is_empty() {
                    let sz = text_size(&p, &s, &f.id());
                    fill(&p, Rect::from_min_size(Pos2::new(r.min.x + 7.0, r.min.y), Vec2::new(sz.x + 3.0, sz.y.max(12.0))), FACE);
                    caption(&p, Rect::from_min_size(Pos2::new(r.min.x + 8.0, r.min.y), sz), &s, f.id(), col(f.color), HAlign::Left, false, false);
                }
                let old = self.enter_group(n);
                for k in &n.kids {
                    self.draw_node(k, r.min, &f, path);
                }
                self.group = old;
            }
            "TPanel" => {
                let old = self.enter_group(n);
                for k in &n.kids {
                    self.draw_node(k, r.min, &f, path);
                }
                self.group = old;
            }
            "TBevel" => {
                if n.ident("Style") == "bsRaised" {
                    raised1(&p, r)
                } else {
                    sunken1(&p, r)
                }
            }
            "TEdit" => self.edit(n, r, id, &f, en, false),
            "TSpinEdit" => self.spin(n, r, id, &f, en),
            "TComboBox" => self.combo(n, r, id, &f, en),
            "TListBox" => self.listbox(n, r, id, &f),
            "TShape" => {
                let c = self.store.color(&n.name);
                fill(&p, r, col(c));
                rect_outline(&p, r, BLACK);
            }
            "TTrackBar" => self.trackbar(n, r, id),
            "TProgressBar" => {
                sunken1(&p, r);
                fill(&p, r.shrink(1.0), FACE);
                let (lo, hi) = (n.int_opt("Min").unwrap_or(0), n.int_opt("Max").unwrap_or(100));
                let frac = ((self.store.position(&n.name) - lo) as f32 / (hi - lo).max(1) as f32).clamp(0.0, 1.0);
                let inner = r.shrink(2.0);
                fill(&p, Rect::from_min_size(inner.min, Vec2::new((inner.width() * frac).floor(), inner.height())), Color32::from_rgb(0, 120, 212));
            }
            "TPaintBox" => {
                fill(&p, r, FACE);
                self.angle_box(r);
            }
            "TImage" => self.image(n, r),
            "TPageControl" => self.page_control(n, r, id, &f, path),
            "TToolButton" => {
                if n.ident("Style") == "tbsSeparator" {
                    let x = r.min.x + (r.width() / 2.0).floor() - 1.0;
                    vline(&p, x, r.min.y, r.max.y, SHADOW);
                    vline(&p, x + 1.0, r.min.y, r.max.y, HILITE);
                }
            }
            _ => {}
        }
        path.pop();
    }

    fn button(&mut self, n: &Node, r: Rect, id: Id, f: &Fnt, en: bool) {
        let p = self.p();
        let ctx = self.ui.ctx().clone();
        let sense = if en { Sense::click() } else { Sense::hover() };
        let resp = self.ui.interact(r, id, sense);
        if en {
            self.tabs.push((self.cur_path.clone(), id));
            self.buttons.push((id, n.name.clone()));
        }
        let down = en && resp.is_pointer_button_down_on();
        if en && resp.hovered() && ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        let focused = ctx.memory(|m| m.has_focus(id));
        let mut click = en && resp.clicked();
        if en && focused && ctx.input(|i| i.key_pressed(Key::Space)) {
            click = true;
        }
        if en && self.accel_hit(n.str("Caption")) {
            click = true;
        }
        fill(&p, r, FACE);
        let is_default = n.flag("Default", false);
        if is_default {
            rect_outline(&p, r, Color32::from_rgb(100, 100, 100));
            let inner = r.shrink(1.0);
            if down {
                frame1(&p, inner, SHADOW, SHADOW);
                fill(&p, inner.shrink(1.0), FACE);
            } else {
                raised(&p, inner);
            }
        } else if down {
            frame1(&p, r, DKSHADOW, DKSHADOW);
            frame1(&p, r.shrink(1.0), SHADOW, SHADOW);
        } else {
            raised(&p, r);
        }
        let off = if down { Vec2::new(1.0, 1.0) } else { Vec2::ZERO };
        let rr = r.translate(off);
        let cap = n.str("Caption");
        if en {
            caption(&p, rr, cap, f.id(), col(f.color), HAlign::Center, true, self.alt);
        } else {
            caption(&p, rr.translate(Vec2::new(1.0, 1.0)), cap, f.id(), HILITE, HAlign::Center, true, false);
            caption(&p, rr, cap, f.id(), SHADOW, HAlign::Center, true, false);
        }
        // the default button shows the focus cue while nothing else holds the keyboard focus
        if (focused || (is_default && ctx.memory(|m| m.focused().is_none()))) && en {
            focus_rect(&p, r.shrink(4.0));
        }
        self.set_hover(n, id, &resp);
        if click {
            self.events.push(Ev::Click(n.name.clone()));
        }
    }

    fn speed_button(&mut self, n: &Node, r: Rect, id: Id, f: &Fnt, en: bool) {
        let p = self.p();
        let resp = self.ui.interact(r, id, if en { Sense::CLICK } else { Sense::hover() });
        let down = en && resp.is_pointer_button_down_on();
        fill(&p, r, FACE);
        if down {
            frame1(&p, r, DKSHADOW, DKSHADOW);
            frame1(&p, r.shrink(1.0), SHADOW, SHADOW);
        } else {
            raised(&p, r);
        }
        let rr = if down { r.translate(Vec2::new(1.0, 1.0)) } else { r };
        let cap = n.str("Caption");
        if en {
            caption(&p, rr, cap, f.id(), col(f.color), HAlign::Center, true, self.alt);
        } else {
            caption(&p, rr.translate(Vec2::new(1.0, 1.0)), cap, f.id(), HILITE, HAlign::Center, true, false);
            caption(&p, rr, cap, f.id(), SHADOW, HAlign::Center, true, false);
        }
        self.set_hover(n, id, &resp);
        if en && (resp.clicked() || self.accel_hit(cap)) {
            self.events.push(Ev::Click(n.name.clone()));
        }
    }

    fn check_radio(&mut self, n: &Node, r: Rect, id: Id, f: &Fnt, en: bool, path: &[i32]) {
        let p = self.p();
        let ctx = self.ui.ctx().clone();
        let radio = n.class == "TRadioButton";
        let resp = self.ui.interact(r, id, if en { Sense::click() } else { Sense::hover() });
        if en {
            self.tabs.push((path.to_vec(), id));
        }
        if en && resp.hovered() && ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        let focused = ctx.memory(|m| m.has_focus(id));
        let cap = n.str("Caption");
        let mut click = en && resp.clicked();
        if en && focused && ctx.input(|i| i.key_pressed(Key::Space)) {
            click = true;
        }
        if en && self.accel_hit(cap) {
            click = true;
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if click {
            let name = n.name.clone();
            if radio {
                if !self.store.checked(&name) {
                    for g in self.group.clone() {
                        self.store.set_checked(&g, false);
                    }
                    self.store.set_checked(&name, true);
                    self.events.push(Ev::Click(name));
                }
            } else {
                let v = !self.store.checked(&name);
                self.store.set_checked(&name, v);
                self.events.push(Ev::Click(name));
            }
        }
        let on = self.store.checked(&n.name);
        let bx = Rect::from_min_size(Pos2::new(r.min.x, r.min.y + ((r.height() - 13.0) / 2.0).floor()), Vec2::splat(13.0));
        if radio {
            let c = bx.center() + Vec2::new(0.0, 0.0);
            p.circle_filled(c, 6.5, if en { HILITE } else { FACE });
            p.circle_stroke(c, 6.0, stroke(SHADOW));
            p.circle_stroke(c, 5.0, stroke(DKSHADOW));
            if on {
                p.circle_filled(c, 2.5, if en { BLACK } else { SHADOW });
            }
        } else {
            fill(&p, bx, if en { HILITE } else { FACE });
            sunken(&p, bx);
            if on {
                let c = if en { BLACK } else { SHADOW };
                for (i, top) in [2, 3, 4, 3, 2, 1, 0].iter().enumerate() {
                    fill(&p, Rect::from_min_size(Pos2::new(bx.min.x + 3.0 + i as f32, bx.min.y + 3.0 + *top as f32), Vec2::new(1.0, 3.0)), c);
                }
            }
        }
        let tr = Rect::from_min_max(Pos2::new(r.min.x + 18.0, r.min.y), r.max);
        let used = if en {
            caption(&p, tr, cap, f.id(), col(f.color), HAlign::Left, true, self.alt)
        } else {
            caption(&p, tr.translate(Vec2::new(1.0, 1.0)), cap, f.id(), HILITE, HAlign::Left, true, false);
            caption(&p, tr, cap, f.id(), SHADOW, HAlign::Left, true, false)
        };
        if focused && en {
            focus_rect(&p, used.expand2(Vec2::new(2.0, 1.0)));
        }
        self.set_hover(n, id, &resp);
    }

    fn edit(&mut self, n: &Node, r: Rect, id: Id, f: &Fnt, en: bool, _spin: bool) {
        let p = self.p();
        let ctx = self.ui.ctx().clone();
        let ro = self.store.readonly(&n.name);
        let borderless = n.ident("BorderStyle") == "bsNone";
        let bg = if ro { n.color("Color").map(col).unwrap_or(FACE) } else { HILITE };
        let bg = if ro && !borderless { FACE } else { bg };
        fill(&p, r, bg);
        let inner = if borderless {
            r
        } else {
            sunken(&p, r);
            r.shrink(2.0)
        };
        let filter = n.props.contains_key("OnKeyPress");
        let mut s = self.store.text(&n.name).to_string();
        let old = s.clone();
        let font = f.id();
        let te = egui::TextEdit::singleline(&mut s)
            .id(id)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::symmetric(1, 0))
            .font(font)
            .text_color(if en { col(f.color) } else { GRAYTEXT })
            .vertical_align(egui::Align::Center)
            .desired_width(inner.width())
            .interactive(en && !ro);
        let resp = self.ui.put(inner, te);
        self.tabs.push((self.cur_path.clone(), id));
        // a page switch focuses the first tab stop and selects the whole text, as the original does
        let sel_key = id.with("select_all");
        if resp.has_focus() && ctx.data(|d| d.get_temp::<bool>(sel_key)).unwrap_or(false) {
            ctx.data_mut(|d| d.remove_temp::<bool>(sel_key));
            if let Some(mut st) = egui::TextEdit::load_state(&ctx, id) {
                let len = s.chars().count();
                let range = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(len));
                st.cursor.set_char_range(Some(range));
                st.store(&ctx, id);
            }
        }
        if ro {
            // read-only: draw text ourselves (egui disables focus for non-interactive edits)
        }
        if filter {
            s.retain(|c| c.is_ascii_digit() || ".-+".contains(c));
        }
        if s != old {
            self.store.set_text(&n.name, &s);
            self.events.push(Ev::Change(n.name.clone()));
        }
        self.set_hover(n, id, &resp);
    }

    fn spin(&mut self, n: &Node, r: Rect, id: Id, f: &Fnt, en: bool) {
        let p = self.p();
        let ctx = self.ui.ctx().clone();
        let (min, max) = (n.int("MinValue"), n.int("MaxValue"));
        let editable = n.flag("EditorEnabled", true);
        fill(&p, r, HILITE);
        sunken(&p, r);
        let inner = r.shrink(2.0);
        let bw = 15.0;
        let btn = Rect::from_min_max(Pos2::new(inner.max.x - bw, inner.min.y), inner.max);
        let up = Rect::from_min_max(btn.min, Pos2::new(btn.max.x, btn.min.y + (btn.height() / 2.0).floor()));
        let dn = Rect::from_min_max(Pos2::new(btn.min.x, up.max.y), btn.max);
        let er = Rect::from_min_max(inner.min, Pos2::new(btn.min.x, inner.max.y));
        let mut v = self.store.position(&n.name);
        let mut changed = false;
        // buttons
        let ru = self.ui.interact(up, id.with("up"), if en { Sense::CLICK } else { Sense::hover() });
        let rd = self.ui.interact(dn, id.with("dn"), if en { Sense::CLICK } else { Sense::hover() });
        for (rc, resp, dir) in [(up, &ru, 'u'), (dn, &rd, 'd')] {
            fill(&p, rc, FACE);
            if en && resp.is_pointer_button_down_on() {
                frame1(&p, rc, DKSHADOW, DKSHADOW);
                frame1(&p, rc.shrink(1.0), SHADOW, SHADOW);
            } else {
                raised(&p, rc);
            }
            arrow(&p, rc.center() + Vec2::new(0.0, if dir == 'u' { 0.0 } else { 0.0 }), dir, if en { BLACK } else { SHADOW });
        }
        if en && ru.clicked() {
            v = (v + 1).clamp(min, max);
            changed = true;
        }
        if en && rd.clicked() {
            v = (v - 1).clamp(min, max);
            changed = true;
        }
        let focused = ctx.memory(|m| m.has_focus(id));
        if focused && en {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) {
                v = (v + 1).clamp(min, max);
                changed = true;
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) {
                v = (v - 1).clamp(min, max);
                changed = true;
            }
        }
        if changed {
            self.store.set_position(&n.name, v);
            self.events.push(Ev::Change(n.name.clone()));
        }
        // text part
        let mut s = self.store.text(&n.name).to_string();
        let old = s.clone();
        if editable {
            let te = egui::TextEdit::singleline(&mut s)
                .id(id)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::symmetric(1, 0))
                .font(f.id())
                .text_color(if en { col(f.color) } else { GRAYTEXT })
                .vertical_align(egui::Align::Center)
                .desired_width(er.width())
                .interactive(en);
            let resp = self.ui.put(er, te);
            self.tabs.push((self.cur_path.clone(), id));
            s.retain(|c| c.is_ascii_digit() || c == '-');
            if s != old {
                self.store.entry(&n.name).text = s.clone();
                if let Ok(x) = s.parse::<i32>() {
                    if x >= min && x <= max {
                        self.store.entry(&n.name).position = x;
                        self.events.push(Ev::Change(n.name.clone()));
                    }
                }
            }
            if en && resp.lost_focus() {
                let x = self.store.text(&n.name).parse::<i32>().unwrap_or(v).clamp(min, max);
                let before = self.store.position(&n.name);
                self.store.set_position(&n.name, x);
                if before != x {
                    self.events.push(Ev::Change(n.name.clone()));
                }
            }
            self.set_hover(n, id, &resp);
        } else {
            let resp = self.ui.interact(er, id, if en { Sense::click() } else { Sense::hover() });
            self.tabs.push((self.cur_path.clone(), id));
            if en && resp.hovered() && ctx.input(|i| i.pointer.primary_pressed()) {
                ctx.memory_mut(|m| m.request_focus(id));
            }
            caption(&p, Rect::from_min_max(er.min + Vec2::new(2.0, 0.0), er.max), &s, f.id(), if en { col(f.color) } else { GRAYTEXT }, HAlign::Left, true, false);
            if focused {
                let sz = text_size(&p, &s, &f.id());
                let fr = Rect::from_min_size(er.min + Vec2::new(1.0, 1.0), Vec2::new(sz.x + 2.0, er.height() - 2.0));
                fill(&p, fr, SELECT);
                caption(&p, Rect::from_min_max(er.min + Vec2::new(2.0, 0.0), er.max), &s, f.id(), HILITE, HAlign::Left, true, false);
            }
            self.set_hover(n, id, &resp);
        }
    }

    fn combo(&mut self, n: &Node, r: Rect, id: Id, f: &Fnt, en: bool) {
        let p = self.p();
        let ctx = self.ui.ctx().clone();
        let items = n.strings("Items.Strings");
        let resp = self.ui.interact(r, id, if en { Sense::click() } else { Sense::hover() });
        self.tabs.push((self.cur_path.clone(), id));
        let open_id = id.with("open");
        let mut open: bool = ctx.data(|d| d.get_temp(open_id)).unwrap_or(false);
        let was_open = open;
        // the main form's first tab stop (CBmode) starts focused, as in the original
        if en && n.name == "CBmode" && ctx.data(|d| d.get_temp::<bool>(Id::new("cbmode_init"))).is_none() {
            ctx.data_mut(|d| d.insert_temp(Id::new("cbmode_init"), true));
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if en && resp.hovered() && ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if en && resp.clicked() {
            open = !open;
        }
        let focused = ctx.memory(|m| m.has_focus(id));
        let mut idx = self.store.item_index(&n.name);
        let mut changed = false;
        let ih = (n.int_opt("ItemHeight").unwrap_or(12) + 1).max(13) as f32;
        let count = items.len().min(n.int_opt("DropDownCount").unwrap_or(8).max(1) as usize);
        let lr = Rect::from_min_size(Pos2::new(r.min.x, r.max.y - 1.0), Vec2::new(r.width(), count as f32 * ih + 4.0));
        if focused && en && !open {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) && (idx + 1) < items.len() as i32 {
                idx += 1;
                changed = true;
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) && idx > 0 {
                idx -= 1;
                changed = true;
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowDown)) || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F4)) {
                open = true;
            }
        }
        // The foreground list can take focus from the combo. Outside clicks
        // and Tab close it explicitly; focus loss alone must not dismiss it.
        let pointer_in_list = open && ctx.input(|i| i.pointer.interact_pos().is_some_and(|pos| lr.contains(pos)));
        if open && (!en || ctx.input(|i| i.key_pressed(Key::Tab))) {
            open = false;
        }
        // look
        fill(&p, r, if en { HILITE } else { FACE });
        sunken(&p, r);
        let inner = r.shrink(2.0);
        let bw = 16.0;
        let btn = Rect::from_min_max(Pos2::new(inner.max.x - bw, inner.min.y), inner.max);
        fill(&p, btn, FACE);
        if open {
            frame1(&p, btn, SHADOW, SHADOW);
        } else {
            raised(&p, btn);
        }
        arrow(&p, btn.center() + Vec2::new(0.0, 0.0), 'd', if en { BLACK } else { SHADOW });
        let tr = Rect::from_min_max(inner.min + Vec2::new(1.0, 0.0), Pos2::new(btn.min.x, inner.max.y));
        let cur = items.get(idx.max(0) as usize).cloned().unwrap_or_default();
        if focused && en && !open {
            fill(&p, Rect::from_min_max(inner.min, Pos2::new(btn.min.x, inner.max.y)), SELECT);
            focus_rect(&p, Rect::from_min_max(inner.min + Vec2::new(1.0, 1.0), Pos2::new(btn.min.x - 1.0, inner.max.y - 1.0)));
            caption(&p, tr.translate(Vec2::new(1.0, 0.0)), &cur, f.id(), HILITE, HAlign::Left, true, false);
        } else {
            caption(&p, tr.translate(Vec2::new(1.0, 0.0)), &cur, f.id(), if en { col(f.color) } else { GRAYTEXT }, HAlign::Left, true, false);
        }
        // drop-down list
        if open {
            self.popup_open = true;
            let selection_id = open_id.with("selection");
            let mut selection = if was_open { ctx.data(|d| d.get_temp::<usize>(selection_id)).unwrap_or(idx.max(0) as usize) } else { idx.max(0) as usize };
            if count > 0 {
                selection = selection.min(count - 1);
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown)) { selection = (selection + 1).min(count - 1); }
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp)) { selection = selection.saturating_sub(1); }
                if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                    idx = selection as i32;
                    changed = true;
                    open = false;
                }
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                open = false;
            }
            ctx.data_mut(|d| d.insert_temp(selection_id, selection));
            let mut picked: Option<usize> = None;
            let hot_id = open_id.with("hot");
            egui::Area::new(open_id.with("area")).order(egui::Order::Foreground).fixed_pos(lr.min).constrain(false).show(&ctx, |ui| {
                let pp = ui.painter().clone();
                fill(&pp, lr, HILITE);
                rect_outline(&pp, lr, BLACK);
                for (i, it) in items.iter().enumerate().take(count) {
                    let ir = Rect::from_min_size(Pos2::new(lr.min.x + 1.0, lr.min.y + 2.0 + i as f32 * ih), Vec2::new(lr.width() - 2.0, ih));
                    let rs = ui.interact(ir, hot_id.with(i), Sense::click());
                    let sel = rs.hovered() || (i == selection && !pointer_in_list);
                    if sel {
                        fill(&pp, ir, SELECT);
                    }
                    caption(&pp, Rect::from_min_max(ir.min + Vec2::new(3.0, 0.0), ir.max), it, f.id(), if sel { HILITE } else { BLACK }, HAlign::Left, true, false);
                    if rs.clicked() {
                        picked = Some(i);
                    }
                }
            });
            if let Some(i) = picked {
                idx = i as i32;
                changed = true;
                open = false;
            } else if ctx.input(|i| i.pointer.primary_pressed()) {
                let pos = ctx.input(|i| i.pointer.interact_pos());
                if let Some(pos) = pos {
                    if !lr.contains(pos) && !r.contains(pos) {
                        open = false;
                    }
                }
            }
        }
        ctx.data_mut(|d| d.insert_temp(open_id, open));
        if changed {
            self.store.set_item_index(&n.name, idx);
            self.events.push(Ev::Change(n.name.clone()));
        }
        self.set_hover(n, id, &resp);
    }

    fn listbox(&mut self, n: &Node, r: Rect, id: Id, f: &Fnt) {
        let p = self.p();
        let ctx = self.ui.ctx().clone();
        let resp = self.ui.interact(r, id, Sense::click());
        self.tabs.push((self.cur_path.clone(), id));
        let ih = n.int_opt("ItemHeight").unwrap_or(13).max(8) as f32;
        let items: Vec<String> = self.store.list(&n.name).to_vec();
        fill(&p, r, HILITE);
        sunken(&p, r);
        let inner = r.shrink(2.0);
        if resp.hovered() && ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        let mut sel = self.store.item_index(&n.name);
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let i = ((pos.y - inner.min.y) / ih).floor() as i32;
                if i >= 0 && (i as usize) < items.len() {
                    sel = i;
                    self.store.set_item_index(&n.name, i);
                    self.events.push(Ev::Click(n.name.clone()));
                }
            }
        }
        if resp.double_clicked() {
            self.events.push(Ev::Change(format!("{}:dbl", n.name)));
        }
        let focused = ctx.memory(|m| m.has_focus(id));
        let pp = p.with_clip_rect(inner);
        for (i, it) in items.iter().enumerate() {
            let y = inner.min.y + i as f32 * ih;
            let ir = Rect::from_min_size(Pos2::new(inner.min.x, y), Vec2::new(inner.width(), ih));
            let s = i as i32 == sel;
            if s {
                fill(&pp, ir, SELECT);
            }
            let c = if s { HILITE } else { BLACK };
            if n.name == "LBfont" {
                // owner-drawn: every cell uses the list's own font (-19 MS PMincho), left aligned, black column rules
                let (s1, s2) = (inner.min.x + 130.0, inner.min.x + 325.0);
                let parts: Vec<&str> = it.split("\t").collect();
                for (k, x) in [inner.min.x + 2.0, s1 + 3.0, s2 + 4.0].into_iter().enumerate() {
                    let cr = Rect::from_min_max(Pos2::new(x, y), Pos2::new(inner.max.x, y + ih));
                    caption(&pp, cr, parts.get(k).copied().unwrap_or(""), f.id(), c, HAlign::Left, true, false);
                }
                vline(&pp, s1, y, y + ih, BLACK);
                vline(&pp, s2, y, y + ih, BLACK);
                if s {
                    // XOR-style dotted focus rectangle: inverted pixels on every other cell
                    let bg = SELECT;
                    let inv = Color32::from_rgb(255 - bg.r(), 255 - bg.g(), 255 - bg.b());
                    for xx in 0..ir.width() as i32 {
                        for (yy, edge) in [(0, true), (ih as i32 - 1, true)] {
                            let (px, py) = (ir.min.x as i32 + xx, ir.min.y as i32 + yy);
                            if edge {
                                fill(&pp, Rect::from_min_size(Pos2::new(px as f32, py as f32), Vec2::splat(1.0)), if (px + py) % 2 != 0 { inv } else { BLACK });
                            }
                        }
                    }
                    for yy in 1..ih as i32 - 1 {
                        for xx in [0, ir.width() as i32 - 1] {
                            let (px, py) = (ir.min.x as i32 + xx, ir.min.y as i32 + yy);
                            fill(&pp, Rect::from_min_size(Pos2::new(px as f32, py as f32), Vec2::splat(1.0)), if (px + py) % 2 != 0 { inv } else { BLACK });
                        }
                    }
                }
            } else {
                caption(&pp, Rect::from_min_size(Pos2::new(inner.min.x + 2.0, y), Vec2::new(inner.width(), ih)), it, f.id(), c, HAlign::Left, true, false);
            }
            if s && focused && n.name != "LBfont" {
                focus_rect(&pp, ir);
            }
        }
        self.set_hover(n, id, &resp);
    }

    fn trackbar(&mut self, n: &Node, r: Rect, id: Id) {
        let p = self.p();
        let (min, max) = (n.int("Min"), n.int("Max"));
        let vertical = n.ident("Orientation") == "trVertical";
        let freq = n.int_opt("Frequency").unwrap_or(1).max(1);
        let resp = self.ui.interact(r, id, Sense::click_and_drag());
        self.tabs.push((self.cur_path.clone(), id));
        let ctx = self.ui.ctx().clone();
        if resp.hovered() && ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        // tick/thumb-centre range measured from the native control (9.5 px in, 10 px out)
        let (a0, a1) = if vertical { (r.min.y + 9.5, r.max.y - 10.0) } else { (r.min.x + 9.5, r.max.x - 10.0) };
        let mut pos = self.store.position(&n.name);
        let mut changed = false;
        if resp.is_pointer_button_down_on() || resp.dragged() {
            if let Some(pt) = resp.interact_pointer_pos() {
                let t = if vertical { (pt.y - a0) / (a1 - a0) } else { (pt.x - a0) / (a1 - a0) };
                let v = (min as f32 + t.clamp(0.0, 1.0) * (max - min) as f32).round() as i32;
                // the vertical zenith slider has 0 at the top
                if v != pos {
                    pos = v;
                    changed = true;
                }
            }
        }
        if changed {
            self.store.set_position(&n.name, pos);
            self.events.push(Ev::Change(n.name.clone()));
        }
        let t = (pos - min) as f32 / (max - min).max(1) as f32;
        let px = |x: f32, y: f32, w: f32, h: f32, c: Color32| fill(&p, Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h)), c);
        // 9px sunken channel: outer shadow/white, inner dark/light, white fill
        let chan = |x0: f32, y0: f32, w: f32, h: f32| {
            px(x0, y0, w, h, HILITE);
            px(x0, y0, w - 1.0, 1.0, SHADOW);
            px(x0, y0, 1.0, h - 1.0, SHADOW);
            px(x0 + 1.0, y0 + 1.0, w - 3.0, 1.0, DKSHADOW);
            px(x0 + 1.0, y0 + 1.0, 1.0, h - 3.0, DKSHADOW);
            px(x0 + 1.0, y0 + h - 2.0, w - 2.0, 1.0, LIGHT);
            px(x0 + w - 2.0, y0 + 1.0, 1.0, h - 2.0, LIGHT);
        };
        let nticks = (max - min) / freq;
        if vertical {
            chan(r.min.x + 13.0, r.min.y + 6.0, 9.0, r.height() - 12.0);
            for i in 0..=nticks {
                let y = (a0 + i as f32 / nticks.max(1) as f32 * (a1 - a0)).floor();
                px(r.min.x + 6.0, y, 3.0, 1.0, BLACK);
            }
            let yc = (a0 + t * (a1 - a0)).floor();
            let (tx, ty) = (r.min.x + 10.0, yc - 3.0);
            // pentagon pointing left: (row, first col, last col, colour) per row, cols relative to tx
            let rows: [(f32, [(f32, f32, Color32); 4]); 7] = [
                (0.0, [(3.0, 12.0, HILITE), (13.0, 13.0, DKSHADOW), (0.0, -1.0, FACE), (0.0, -1.0, FACE)]),
                (1.0, [(2.0, 2.0, HILITE), (3.0, 11.0, LIGHT), (12.0, 12.0, SHADOW), (13.0, 13.0, DKSHADOW)]),
                (2.0, [(1.0, 1.0, HILITE), (2.0, 2.0, LIGHT), (12.0, 12.0, SHADOW), (13.0, 13.0, DKSHADOW)]),
                (3.0, [(0.0, 0.0, DKSHADOW), (1.0, 1.0, SHADOW), (12.0, 12.0, SHADOW), (13.0, 13.0, DKSHADOW)]),
                (4.0, [(0.0, 0.0, DKSHADOW), (1.0, 1.0, SHADOW), (12.0, 12.0, SHADOW), (13.0, 13.0, DKSHADOW)]),
                (5.0, [(2.0, 2.0, DKSHADOW), (3.0, 12.0, SHADOW), (13.0, 13.0, DKSHADOW), (0.0, -1.0, FACE)]),
                (6.0, [(3.0, 13.0, DKSHADOW), (0.0, -1.0, FACE), (0.0, -1.0, FACE), (0.0, -1.0, FACE)]),
            ];
            for (ry, segs) in rows {
                for (c0, c1, col) in segs {
                    if c1 >= c0 {
                        px(tx + c0, ty + ry, c1 - c0 + 1.0, 1.0, col);
                    }
                }
            }
            // face between the bevels
            for (ry, c0, c1) in [(1.0, 3.0, 11.0), (2.0, 3.0, 11.0), (3.0, 2.0, 11.0), (4.0, 2.0, 11.0)] {
                if ry >= 2.0 {
                    px(tx + c0, ty + ry, c1 - c0 + 1.0, 1.0, FACE);
                }
            }
        } else {
            chan(r.min.x + 6.0, r.min.y + 13.0, r.width() - 13.0, 9.0);
            for i in 0..=nticks {
                let x = (a0 + i as f32 / nticks.max(1) as f32 * (a1 - a0)).floor();
                px(x, r.min.y + 6.0, 1.0, 3.0, BLACK);
            }
            let xc = (a0 + t * (a1 - a0)).floor();
            let (tx, ty) = (xc - 3.0, r.min.y + 10.0);
            // pentagon pointing up: rows 0..13, cols 0..6
            px(tx + 3.0, ty, 1.0, 1.0, DKSHADOW);
            px(tx + 2.0, ty + 1.0, 1.0, 1.0, HILITE);
            px(tx + 3.0, ty + 1.0, 1.0, 1.0, SHADOW);
            px(tx + 4.0, ty + 1.0, 1.0, 1.0, DKSHADOW);
            px(tx + 1.0, ty + 2.0, 1.0, 1.0, HILITE);
            px(tx + 2.0, ty + 2.0, 1.0, 1.0, LIGHT);
            px(tx + 3.0, ty + 2.0, 1.0, 1.0, FACE);
            px(tx + 4.0, ty + 2.0, 1.0, 1.0, SHADOW);
            px(tx + 5.0, ty + 2.0, 1.0, 1.0, DKSHADOW);
            px(tx + 2.0, ty + 1.0, 0.0, 0.0, FACE);
            for ry in 3..12 {
                let y = ty + ry as f32;
                px(tx, y, 1.0, 1.0, HILITE);
                px(tx + 1.0, y, 1.0, 1.0, LIGHT);
                px(tx + 2.0, y, 3.0, 1.0, FACE);
                px(tx + 5.0, y, 1.0, 1.0, SHADOW);
                px(tx + 6.0, y, 1.0, 1.0, DKSHADOW);
            }
            px(tx, ty + 12.0, 1.0, 1.0, HILITE);
            px(tx + 1.0, ty + 12.0, 5.0, 1.0, SHADOW);
            px(tx + 6.0, ty + 12.0, 1.0, 1.0, DKSHADOW);
            px(tx, ty + 13.0, 7.0, 1.0, DKSHADOW);
        }
        if ctx.memory(|m| m.has_focus(id)) {
            focus_rect(&p, r.shrink(1.0));
        }
        self.set_hover(n, id, &resp);
    }

    /// Wire-frame box preview (PBangle), an oblique projection like the bird's-eye view.
    fn angle_box(&mut self, r: Rect) {
        let p = self.p().with_clip_rect(r);
        let (w, ydx, ydy, h, shift) = angle_box_metrics(self.store.position("SEeta"), self.store.position("SEphi"));
        let org = (r.min.x as i32 + 3 + shift, r.min.y as i32 + 97); // top-left pixel of the 2x2 axis corner
        let dot = |x: i32, y: i32| fill(&p, Rect::from_min_size(Pos2::new(x as f32, y as f32), Vec2::splat(1.0)), BLACK);
        let line = |a: (i32, i32), b: (i32, i32), thick: bool| {
            let (mut x, mut y) = a;
            let (dx, dy) = ((b.0 - a.0).abs(), -(b.1 - a.1).abs());
            let (sx, sy) = (if a.0 < b.0 { 1 } else { -1 }, if a.1 < b.1 { 1 } else { -1 });
            let mut err = dx + dy;
            loop {
                dot(x, y);
                if thick {
                    if dx > -dy {
                        dot(x, y + 1);
                    } else {
                        dot(x + 1, y);
                    }
                }
                if x == b.0 && y == b.1 {
                    break;
                }
                let e2 = 2 * err;
                if e2 >= dy {
                    err += dy;
                    x += sx;
                }
                if e2 <= dx {
                    err += dx;
                    y += sy;
                }
            }
        };
        let (ox, oy) = org;
        let (fx, fy) = (ox + w, oy - h); // front top-right corner
        let (bx, by) = (ox + ydx, oy - ydy); // back bottom-left
        let (btop, bright) = (by - h - 1, bx + w - 1);
        // back box
        line((bx, btop), (bright, btop), false);
        line((bx, by), (bright, by), false);
        line((bx, btop), (bx, by), false);
        line((bright, btop), (bright, by), false);
        // front box
        line((ox, fy), (fx, fy), false);
        line((fx, fy), (fx, oy + 1), false);
        // connecting edges
        line((bx, btop), (ox, fy), false);
        line((bright, btop), (fx, fy), false);
        line((bright, by), (fx, oy + 1), false);
        // axes (2px)
        let xend = ox + w + 10;
        line((ox, oy), (xend, oy), true);
        line((ox, oy), (ox, fy - 18), true);
        line((ox + 1, oy - 2), (bx, by + 1), true);
        let blit = |ax: i32, ay: i32, rows: &[(i32, i32, i32)]| {
            for &(dy, x0, x1) in rows {
                for x in x0..=x1 {
                    dot(ax + x, ay + dy);
                }
            }
        };
        // arrow heads: (dy, x0, x1) relative to an anchor
        let (zy, zx) = (fy - 18, ox + 1);
        blit(zx, zy, &[(0, -1, 1), (1, -2, 1), (2, -2, 2), (3, -3, 3), (4, -3, 0), (4, 2, 3)]);
        blit(xend - 9, oy + 1, &[(-3, 5, 6), (-2, 5, 8), (1, 6, 9), (2, 5, 8), (3, 5, 6)]);
        // the depth axis leans the other way for a negative eta: mirror its head and label
        let head = [(-8, 4, 6), (-7, 2, 6), (-6, 2, 6), (-5, 3, 6), (-4, 3, 6), (-3, 2, 3), (-3, 5, 6), (-2, 0, 3), (-1, 0, 2), (0, 0, 1)];
        if ydx < 0 {
            blit(bx, by, &head.map(|(dy, a, b)| (dy, -b, -a)));
        } else {
            blit(bx, by, &head);
        }
        // letter labels: white box with a pixel glyph
        let white = |x: i32, y: i32, ww: i32, hh: i32| fill(&p, Rect::from_min_size(Pos2::new(x as f32, y as f32), Vec2::new(ww as f32, hh as f32)), Color32::WHITE);
        let (lzx, lzy) = (zx - 3, zy - 13);
        white(lzx, lzy, 6, 11);
        blit(lzx, lzy, &[(1, 0, 4), (2, 4, 4), (3, 3, 3), (4, 2, 2), (5, 1, 1), (6, 0, 0), (7, 0, 4)]);
        let (lyx, lyy) = (if ydx < 0 { bx - 9 } else { bx + 3 }, by - 21);
        white(lyx, lyy, 7, 11);
        blit(lyx, lyy, &[(1, 0, 0), (1, 6, 6), (2, 1, 1), (2, 5, 5), (3, 2, 2), (3, 4, 4), (4, 3, 3), (5, 3, 3), (6, 3, 3), (7, 3, 3)]);
        let (lxx, lxy) = (xend + 2, oy - 3);
        white(lxx, lxy, 7, 10);
        blit(lxx, lxy, &[(1, 0, 0), (1, 5, 5), (2, 1, 1), (2, 4, 4), (3, 2, 3), (4, 2, 3), (5, 2, 3), (6, 1, 1), (6, 4, 4), (7, 0, 0), (7, 5, 5)]);
    }

    fn image(&mut self, n: &Node, r: Rect) {
        let p = self.p();
        if n.name == "Image1" && r.width() == 64.0 {
            // About box picture
            if let Some(t) = &self.assets.about {
                p.image(t.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            } else {
                super::super::icons::draw_fallback_logo(&p, r);
            }
        } else if n.name == "Image1" {
            // legend gradient preview
            let ty = self.store.item_index("CBtype");
            let nc = self.store.position("SEcolor").max(2);
            let rev = self.store.checked("CBreverse");
            let fold = self.store.checked("CBfold");
            let grad = Gradient::new(GradientKind::from_index(ty.max(0) as u8), nc as usize, rev, fold);
            sunken1(&p, r.expand(1.0));
            let w = r.width() as i32;
            for x in 0..w {
                let c = grad.pixel_color(x as usize, w as usize);
                let c = Color32::from_rgb(c.0, c.1, c.2);
                fill(&p, Rect::from_min_size(Pos2::new(r.min.x + x as f32, r.min.y), Vec2::new(1.0, r.height())), c);
            }
        }
    }

    fn page_control(&mut self, n: &Node, r: Rect, id: Id, f: &Fnt, path: &mut Vec<i32>) {
        let p = self.p();
        let ctx = self.ui.ctx().clone();
        let th = n.int_opt("TabHeight").unwrap_or(18) as f32;
        let sheets: Vec<&Node> = n.kids.iter().filter(|k| k.class == "TTabSheet").collect();
        let font = f.id();
        let nat: Vec<f32> = sheets.iter().map(|s| tab_width(s.str("Caption"), text_size(&p, s.str("Caption"), &font).x)).collect();
        // dialogs are widened but the tabs are not, so wrap against the original width
        let rows = tab_rows(&nat, r.width() / crate::forms::DIALOG_WIDEN);
        let mut cur = self.store.item_index(&n.name).clamp(0, sheets.len() as i32 - 1) as usize;
        // the row of the selected tab moves to the bottom
        let sel_row = rows.iter().position(|rw| rw.contains(&cur)).unwrap_or(0);
        let mut order: Vec<usize> = (0..rows.len()).filter(|i| *i != sel_row).collect();
        order.push(sel_row);
        let body_top = r.min.y + rows.len() as f32 * th;
        let page = Rect::from_min_max(Pos2::new(r.min.x, body_top), r.max);
        fill(&p, page, FACE);
        raised(&p, page);
        // tabs
        let mut rects: Vec<(usize, Rect)> = vec![];
        for (slot, ri) in order.iter().enumerate() {
            let row = &rows[*ri];
            let total: f32 = row.iter().map(|i| nat[*i]).sum();
            // comctl32 insets the tab rows by 2px on both sides
            let (rx0, rx1) = (r.min.x + 2.0, r.max.x - 2.0);
            let scale = (rx1 - rx0) / total;
            let mut x = rx0;
            let y = r.min.y + slot as f32 * th;
            for (k, i) in row.iter().enumerate() {
                let w = if k + 1 == row.len() { rx1 - x } else { (nat[*i] * scale).floor() };
                rects.push((*i, Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, th))));
                x += w;
            }
        }
        let mut hit: Option<usize> = None;
        for (i, tr) in &rects {
            let rs = self.ui.interact(tr.expand2(Vec2::new(0.0, 0.0)), id.with(*i), Sense::click());
            if rs.clicked() {
                hit = Some(*i);
            }
        }
        // focus the tab strip as a whole for keyboard (Ctrl+Tab style left/right)
        if let Some(i) = hit {
            if i != cur {
                cur = i;
                self.store.set_item_index(&n.name, i as i32);
                self.events.push(Ev::Change(n.name.clone()));
            }
        }
        for (i, tr) in rects.iter() {
            let selected = *i == cur;
            let t = if selected { tr.expand2(Vec2::new(2.0, 0.0)).with_min_y(tr.min.y - 2.0).with_max_y(tr.max.y + 1.0) } else { *tr };
            if selected {
                fill(&p, t, FACE);
                // open the page's top edge below the tab
                hline(&p, t.min.x + 1.0, t.max.x - 1.0, tr.max.y - 0.0, FACE);
                hline(&p, t.min.x + 1.0, t.max.x - 1.0, tr.max.y + 1.0, FACE);
            } else {
                fill(&p, t, FACE);
            }
            // top and left highlight, right shadow
            hline(&p, t.min.x + 2.0, t.max.x - 2.0, t.min.y, HILITE);
            hline(&p, t.min.x + 2.0, t.max.x - 3.0, t.min.y + 1.0, LIGHT);
            vline(&p, t.min.x, t.min.y + 2.0, t.max.y, HILITE);
            vline(&p, t.min.x + 1.0, t.min.y + 2.0, t.max.y, LIGHT);
            vline(&p, t.max.x - 1.0, t.min.y + 2.0, t.max.y, DKSHADOW);
            vline(&p, t.max.x - 2.0, t.min.y + 2.0, t.max.y, SHADOW);
            let tcol = col(f.color);
            caption(&p, Rect::from_min_max(t.min + Vec2::new(0.0, if selected { 1.0 } else { 2.0 }), t.max), sheets[*i].str("Caption"), font.clone(), tcol, HAlign::Center, true, false);
        }
        let _ = ctx;
        // active sheet
        let mark = self.focusable.len();
        if let Some(s) = sheets.get(cur) {
            let org = Pos2::new(r.min.x + 4.0, body_top + 4.0);
            path.push(0);
            let old = self.enter_group(s);
            for k in &s.kids {
                self.draw_node(k, org, f, path);
            }
            self.group = old;
            path.pop();
        }
        // The original puts the focus on the page's first tab stop when the dialog opens and
        // when another tab is shown, with the text of an edit fully selected.
        // A break in the pass count means the dialog was closed and is open again. Its viewport may
        // have been dropped meanwhile, which restarts the count, so a lower number is a break too.
        let seen = self.scope.with((n.name.as_str(), "page_shown"));
        let pass = ctx.cumulative_pass_nr();
        let previous: Option<(usize, u64)> = ctx.data(|d| d.get_temp(seen));
        ctx.data_mut(|d| d.insert_temp(seen, (cur, pass)));
        if previous.is_none_or(|(page, last)| page != cur || pass != last + 1) {
            if let Some(first) = self.focusable[mark..].iter().min_by(|a, b| a.0.cmp(&b.0)).map(|t| t.1) {
                ctx.memory_mut(|m| m.request_focus(first));
                ctx.data_mut(|d| d.insert_temp(first.with("select_all"), true));
            }
        }
    }
}

/// Tab width in the original dialog, measured from its screenshots (comctl32 pads and justifies
/// them per row). Anything else falls back to the same padding over the measured text.
fn tab_width(caption: &str, text: f32) -> f32 {
    match caption {
        "俯瞰図" => 63.0, "分布図" => 64.0, "ベクトル" => 69.0, "等高線" | "座標系" => 63.0, "グリッド" => 64.0,
        "凡例" => 52.0, "ラベル" => 56.0, "入出力" => 59.0, "表示" => 53.0, "フォント" => 61.0, "特殊" => 53.0, "暫定" => 52.0,
        _ => 40.0 + 0.5 * text,
    }
}

/// Greedy fill of tabs into rows of at most `limit` pixels.
fn tab_rows(widths: &[f32], limit: f32) -> Vec<Vec<usize>> {
    let mut rows: Vec<Vec<usize>> = vec![vec![]];
    let mut used = 0.0;
    for (i, w) in widths.iter().enumerate() {
        if used + w > limit && !rows.last().unwrap().is_empty() {
            rows.push(vec![]);
            used = 0.0;
        }
        rows.last_mut().unwrap().push(i);
        used += w;
    }
    rows
}

/// Box of the PBangle preview: (front width, back offset x, back offset y, height, shift of the
/// origin to the right).
///
/// Measured from the original: the front face never changes; eta only shears the back face
/// sideways, by about 58 tan(eta), and phi only moves it up, by about 57 cos(phi). A negative eta
/// leans the box to the left, and the origin moves right to make room. The original draws within
/// one pixel of these numbers.
fn angle_box_metrics(eta: i32, phi: i32) -> (i32, i32, i32, i32, i32) {
    let e = eta.clamp(-45, 45);
    let tan = (e.unsigned_abs() as f64).to_radians().tan();
    let over = (58.0 * tan - 1e-9).ceil().max(0.0) as i32;
    let (dx, shift) = if e < 0 { (-(over - 2).max(0), (67.0 * tan + 1e-9) as i32) } else { (over, 0) };
    let dy = (57.0 * (phi.clamp(0, 85) as f64).to_radians().cos()) as i32;
    (59, dx, dy, 39, shift)
}

fn find_flag(n: &Node, flag: &str) -> Option<String> {
    if n.class == "TButton" && n.flag(flag, false) {
        return Some(n.name.clone());
    }
    n.kids.iter().find_map(|k| find_flag(k, flag))
}

#[cfg(test)]
mod modal_tests {
    use super::*;

    #[test]
    fn angle_box_follows_the_measured_oblique_projection() {
        // eta shears sideways whatever phi is; the front face stays the same size
        for (eta, dx) in [(0, 0), (1, 2), (10, 11), (20, 22), (30, 34), (40, 49), (45, 58)] {
            assert_eq!(angle_box_metrics(eta, 30), (59, dx, 49, 39, 0), "eta={eta}");
            assert_eq!(angle_box_metrics(eta, 80).1, dx, "eta={eta}");
        }
        // phi raises the back face, independently of eta; the original differs by 1 at 60 and 70
        for (phi, dy) in [(0, 57), (10, 56), (20, 53), (30, 49), (40, 43), (50, 36), (75, 14), (85, 4)] {
            assert_eq!(angle_box_metrics(30, phi).2, dy, "phi={phi}");
            assert_eq!(angle_box_metrics(-30, phi).2, dy, "phi={phi}");
        }
        // a negative eta leans left and moves the origin to the right
        for (eta, dx, shift) in [(-1, 0, 1), (-20, -20, 24), (-45, -56, 67)] {
            let m = angle_box_metrics(eta, 30);
            assert_eq!((m.1, m.4), (dx, shift), "eta={eta}");
        }
        // out of range values are clamped, as in the original
        assert_eq!(angle_box_metrics(60, 10), angle_box_metrics(45, 10));
        assert_eq!(angle_box_metrics(-90, 120), angle_box_metrics(-45, 85));
    }

    #[test]
    fn settings_tabs_wrap_into_the_original_two_rows() {
        let forms = crate::forms::Forms::load();
        let pc = forms.config.find("TPCconfig").unwrap();
        let nat: Vec<f32> = pc.kids.iter().filter(|k| k.class == "TTabSheet").map(|s| tab_width(s.str("Caption"), 1000.0)).collect();
        let rows = tab_rows(&nat, pc.int("Width") as f32 / crate::forms::DIALOG_WIDEN);
        assert_eq!(rows.iter().map(Vec::len).collect::<Vec<_>>(), [6, 7]);
    }

    #[test]
    fn page_switch_focuses_the_first_tab_stop_of_the_shown_page() {
        let ctx = egui::Context::default();
        let mut store = Store::default();
        let assets = Assets::default();
        // SEsecond is drawn first but has the higher TabOrder, so it must not take the focus.
        let node = Node::parse(
            r#"{"class":"TPageControl","name":"TPCtest","props":{"Width":391,"Height":276,"TabHeight":18},"kids":[
                {"class":"TTabSheet","name":"TSfirst","props":{"Caption":"A"},"kids":[
                    {"class":"TSpinEdit","name":"SEsecond","props":{"MinValue":0,"MaxValue":10,"TabOrder":1},"kids":[]},
                    {"class":"TSpinEdit","name":"SEfirst","props":{"MinValue":0,"MaxValue":10,"TabOrder":0},"kids":[]}]},
                {"class":"TTabSheet","name":"TSother","props":{"Caption":"B"},"kids":[
                    {"class":"TSpinEdit","name":"SEother","props":{"MinValue":0,"MaxValue":10,"TabOrder":0},"kids":[]}]}]}"#,
        );
        let frame = |store: &mut Store| -> Id {
            let mut raw = egui::RawInput::default();
            raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0)));
            let mut scope = Id::NULL;
            let out = ctx.run_ui(raw, |ui| {
                scope = ui.scope_id();
                let org = ui.max_rect().min;
                let mut r = Renderer::new(ui, store, &assets);
                let mut path = vec![];
                r.draw_node(&node, org, &Fnt::form_default(), &mut path);
                r.finish();
            });
            out.drop_without_applying_deltas();
            scope
        };
        // opening the dialog: the focus lands on the lowest TabOrder of the first page
        frame(&mut store);
        let scope = frame(&mut store);
        assert_eq!(ctx.memory(|m| m.focused()), Some(scope.with("SEfirst")));
        // another tab shows the focus on its own first control
        store.set_item_index("TPCtest", 1);
        frame(&mut store);
        let scope = frame(&mut store);
        assert_eq!(ctx.memory(|m| m.focused()), Some(scope.with("SEother")));
        // and back again
        store.set_item_index("TPCtest", 0);
        frame(&mut store);
        let scope = frame(&mut store);
        assert_eq!(ctx.memory(|m| m.focused()), Some(scope.with("SEfirst")));
        // focus moves away, the dialog closes for a while and opens again on the same page
        ctx.memory_mut(|m| m.surrender_focus(scope.with("SEfirst")));
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |_| {}).drop_without_applying_deltas();
        }
        frame(&mut store);
        let scope = frame(&mut store);
        assert_eq!(ctx.memory(|m| m.focused()), Some(scope.with("SEfirst")));
    }

    #[test]
    fn page_focus_skips_a_disabled_first_control() {
        let ctx = egui::Context::default();
        let mut store = Store::default();
        let assets = Assets::default();
        let node = Node::parse(
            r#"{"class":"TPageControl","name":"TPCoff","props":{"Width":391,"Height":276,"TabHeight":18},"kids":[
                {"class":"TTabSheet","name":"TSoff","props":{"Caption":"A"},"kids":[
                    {"class":"TSpinEdit","name":"SEdisabled","props":{"MinValue":0,"MaxValue":10,"TabOrder":0,"Enabled":false},"kids":[]},
                    {"class":"TSpinEdit","name":"SEenabled","props":{"MinValue":0,"MaxValue":10,"TabOrder":1},"kids":[]}]}]}"#,
        );
        let mut scope = Id::NULL;
        for _ in 0..2 {
            let mut raw = egui::RawInput::default();
            raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0)));
            let out = ctx.run_ui(raw, |ui| {
                scope = ui.scope_id();
                let org = ui.max_rect().min;
                let mut r = Renderer::new(ui, &mut store, &assets);
                let mut path = vec![];
                r.draw_node(&node, org, &Fnt::form_default(), &mut path);
                r.finish();
            });
            out.drop_without_applying_deltas();
        }
        assert_eq!(ctx.memory(|m| m.focused()), Some(scope.with("SEenabled")));
    }

    /// The real settings form: every one of the 13 tabs must focus something when shown.
    #[test]
    fn every_settings_tab_focuses_a_control() {
        let forms = crate::forms::Forms::load();
        let ctx = egui::Context::default();
        let mut store = Store::from_forms(&[&forms.config]);
        let assets = Assets::default();
        let pages = forms.config.find("TPCconfig").map(|n| n.kids.iter().filter(|k| k.class == "TTabSheet").count()).unwrap_or(0);
        assert_eq!(pages, 13);
        for page in 0..pages {
            store.set_item_index("TPCconfig", page as i32);
            let mut focused = None;
            for _ in 0..2 {
                let mut raw = egui::RawInput::default();
                raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0)));
                let out = ctx.run_ui(raw, |ui| {
                    let org = ui.max_rect().min;
                    let mut r = Renderer::new(ui, &mut store, &assets);
                    r.draw_children(&forms.config, org, &Fnt::form_default().with(&forms.config));
                    r.finish();
                });
                out.drop_without_applying_deltas();
                focused = ctx.memory(|m| m.focused());
            }
            assert!(focused.is_some(), "settings page {page} focused nothing");
        }
    }

    #[test]
    fn disabled_parent_controls_do_not_take_tab_or_accept_input() {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::Key { key: Key::Tab, physical_key: Some(Key::Tab), pressed: true, repeat: false, modifiers: Modifiers::NONE });
        let mut store = Store::default();
        let node = Node::parse(r#"{"class":"TSpinEdit","name":"SEsize","props":{"MinValue":1,"MaxValue":20000},"kids":[]}"#);
        store.set_position("SEsize", 300);
        let assets = Assets::default();
        let output = ctx.run_ui(input, |ui| {
            ui.add_enabled_ui(false, |ui| {
                let r = Renderer::new(ui, &mut store, &assets);
                assert!(!r.is_enabled(&node));
            });
        });
        assert!(ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)), "disabled parent must not consume the child's Tab");
        assert_eq!(store.position("SEsize"), 300);
        output.drop_without_applying_deltas();
    }
}

