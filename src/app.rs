//! Main window: menu bar, control bar with five tool bands, scroll box, status bar and the actions.
use crate::dialogs::{self, Dialogs, Shared};
use crate::forms::{Forms, Node};
use crate::ini::{self, Layout};
use crate::platform_win32 as plat;
use crate::state::Store;
use crate::vcl::form::{Assets, Ev, Fnt, Renderer};
use crate::vcl::*;
use crate::view::{Item, ViewState};
use egui::{Context, Id, Key, Modifiers, Order, Pos2, Rect, Sense, Vec2};
use std::path::{Path, PathBuf};

const MENU_H: f32 = 20.0;
const BAR_H: f32 = 64.0;
const STATUS_H: f32 = 22.0;
const NODATA: &str = "AV似非 - nodata";

struct MItem {
    name: String,
    cap: String,
    sc: String,
    img: Option<usize>,
    sep: bool,
    kids: Vec<MItem>,
}

fn key_name(code: i32) -> String {
    let k = code & 0xfff;
    let mut s = String::new();
    if code & 0x4000 != 0 {
        s += "Ctrl+";
    }
    if code & 0x2000 != 0 {
        s += "Shift+";
    }
    if code & 0x8000 != 0 {
        s += "Alt+";
    }
    s.push(char::from_u32(k as u32).unwrap_or('?'));
    s
}

fn build_menu(n: &Node) -> MItem {
    let cap = n.str("Caption").to_string();
    MItem {
        name: n.name.clone(),
        sep: cap == "-",
        sc: n.int_opt("ShortCut").filter(|v| *v != 0).map(key_name).unwrap_or_default(),
        img: n.int_opt("ImageIndex").filter(|v| *v >= 0).map(|v| v as usize),
        cap,
        kids: n.kids.iter().map(build_menu).collect(),
    }
}

/// View item behind a menu entry or tool button name.
fn item_of(name: &str) -> Option<Item> {
    let s = name.strip_prefix("TB").or_else(|| name.strip_prefix("men"))?;
    Some(match s {
        "enflat" => Item::Flat,
        "enland" => Item::Land,
        "cartesian" => Item::Cart,
        "cylindrical" => Item::Cyl,
        "engraph" => Item::Graph,
        "enscatter" => Item::Scatter,
        "enpanel" => Item::Panel,
        "envector" => Item::Vector,
        "entoko" => Item::Toko,
        "enaxis" => Item::Axis,
        "engrid" => Item::Grid,
        "enpoint" => Item::Point,
        "enlegend" => Item::Legend,
        "enlabel" => Item::Label,
        _ => return None,
    })
}

pub struct App {
    forms: Forms,
    store: Store,
    layout: Layout,
    assets: Assets,
    dlg: Dialogs,
    title: String,
    ini_path: PathBuf,
    first: bool,
    start_files: Vec<PathBuf>,
    menu: Vec<MItem>,
    open_menu: Option<usize>,
    hot: Option<usize>,
    drag: Option<usize>,
    started_hwnd: bool,
    data: Option<Vec<u8>>,
    canvas_tex: Option<egui::TextureHandle>,
    status_msg: String,
}

pub fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default()
}

fn asset_dir() -> Option<PathBuf> {
    let mut c = vec![PathBuf::from("assets/local")];
    let e = exe_dir();
    c.push(e.join("assets/local"));
    c.push(e.join("../../assets/local"));
    c.into_iter().find(|p| p.join("imagelist.png").exists())
}

fn load_tex(ctx: &Context, dir: &Path, file: &str) -> Option<egui::TextureHandle> {
    let img = image::open(dir.join(file)).ok()?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    Some(ctx.load_texture(file, egui::ColorImage::from_rgba_unmultiplied([w, h], img.as_raw()), egui::TextureOptions::NEAREST))
}

impl App {
    pub fn new(ctx: &Context, forms: Forms, store: Store, layout: Layout, files: Vec<PathBuf>) -> App {
        let mut assets = Assets::default();
        if let Some(d) = asset_dir() {
            assets.imagelist = load_tex(ctx, &d, "imagelist.png");
            if let Ok(img) = image::open(d.join("imagelist.png")) {
                let (chk, dis) = crate::icons::bake_variants(&img.to_rgba8());
                let [c0, c1] = chk;
                assets.imagelist_chk = Some(ctx.load_texture("imagelist_chk", c0, egui::TextureOptions::NEAREST));
                assets.imagelist_chk1 = Some(ctx.load_texture("imagelist_chk1", c1, egui::TextureOptions::NEAREST));
                assets.imagelist_dis = Some(ctx.load_texture("imagelist_dis", dis, egui::TextureOptions::NEAREST));
            }
            assets.about = load_tex(ctx, &d, "about.png");
            assets.app = load_tex(ctx, &d, "app.png");
        }
        let menu = forms.form1.find("MainMenu1").map(|m| m.kids.iter().map(build_menu).collect()).unwrap_or_default();
        App {
            forms,
            store,
            layout,
            assets,
            dlg: Dialogs::default(),
            title: NODATA.into(),
            ini_path: exe_dir().join("avese.ini"),
            first: true,
            start_files: files,
            menu,
            open_menu: None,
            hot: None,
            drag: None,
            started_hwnd: false,
            data: None,
            canvas_tex: None,
            status_msg: String::new(),
        }
    }

    fn vs(&self) -> ViewState {
        ViewState::from_store(&self.store)
    }

    /// Render the held bytes with the current settings; errors go to the status bar.
    fn render_data(&mut self, ctx: &Context) {
        let Some(bytes) = self.data.as_ref() else { return };
        let st = crate::engine::pipeline::RenderSettings::from_store(&self.store);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::engine::pipeline::render_file(bytes, &st)))
            .unwrap_or_else(|_| Err("描画中にエラーが発生しました".to_string()));
        match res {
            Ok(c) => {
                let img = egui::ColorImage::from_rgba_premultiplied([c.width as usize, c.height as usize], &c.to_rgba_bytes());
                self.canvas_tex = Some(ctx.load_texture("canvas", img, egui::TextureOptions::NEAREST));
                self.status_msg.clear();
            }
            Err(e) => {
                self.canvas_tex = None;
                self.status_msg = e;
            }
        }
    }

    fn load_files(&mut self, ctx: &Context, files: Vec<PathBuf>) {
        match files.len() {
            0 => {}
            1 => {
                let n = files[0].file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                match std::fs::read(&files[0]) {
                    Ok(b) => {
                        self.title = format!("AV似非 - {n}");
                        self.store.set_enabled("TBRedraw", true);
                        self.data = Some(b);
                        self.render_data(ctx);
                    }
                    Err(e) => self.status_msg = format!("読み込めません: {e}"),
                }
            }
            _ => {
                self.store.lists.insert("savelist".into(), files.iter().map(|p| p.display().to_string()).collect());
                self.store.set_item_index("savelist", -1);
                self.dlg.avi = true;
            }
        }
    }

    fn open_help(&self) {
        let p = exe_dir().join("avese.htm");
        if !p.exists() {
            self.dlg.modal(|| dialogs::notice("AV似非", "ヘルプファイル avese.htm が見つかりません。"));
            return;
        }
        #[cfg(windows)]
        let _ = std::process::Command::new("cmd").args(["/C", "start", ""]).arg(&p).spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(&p).spawn();
        #[cfg(all(unix, not(target_os = "macos")))]
        let _ = std::process::Command::new("xdg-open").arg(&p).spawn();
    }

    fn save_ini(&self, p: &Path) {
        if let Err(e) = ini::save_file(p, &self.store, &self.layout) {
            self.dlg.modal(|| dialogs::notice("AV似非", &format!("設定を保存できません: {e}")));
        }
    }

    fn action(&mut self, ctx: &Context, name: &str) {
        if let Some(item) = item_of(name) {
            let mut vs = self.vs();
            vs.click(item);
            vs.write_store(&mut self.store);
            dialogs::apply_interlocks(&mut self.store);
            return;
        }
        match name {
            "menload1" | "TBload1" | "menreload" | "TBReload" => {
                let f = self.dlg.modal(dialogs::pick_open_files);
                self.load_files(ctx, f);
            }
            "menload2" => self.dlg.avi = true,
            "mensave" | "TBsave" => {
                if self.dlg.modal(dialogs::pick_save_image).is_some() {
                    self.dlg.modal(|| dialogs::not_implemented("画像のセーブ"));
                }
            }
            "menClipbrd" | "TBclip" => self.dlg.modal(|| dialogs::not_implemented("クリップボードへのコピー")),
            "menclose" => {
                self.title = NODATA.into();
                self.store.set_enabled("TBRedraw", false);
                self.data = None;
                self.canvas_tex = None;
                self.status_msg.clear();
            }
            "menredraw" | "TBRedraw" => self.render_data(ctx),
            "menconfig" | "TBConfig" => self.dlg.open_config(&self.store),
            "menconfigsave" => {
                let p = self.ini_path.clone();
                self.save_ini(&p);
            }
            "menconfigloadas" => {
                if let Some(p) = self.dlg.modal(dialogs::pick_ini_open) {
                    let mut lay = self.layout.clone();
                    if ini::load_file(&p, &mut self.store, &mut lay).is_ok() {
                        self.layout.bars = lay.bars;
                        dialogs::apply_interlocks(&mut self.store);
                    }
                }
            }
            "menconfigsaveas" => {
                if let Some(p) = self.dlg.modal(dialogs::pick_ini_save) {
                    self.save_ini(&p);
                }
            }
            "menhelp" | "TBHelp" => self.open_help(),
            "menversion" => self.dlg.about = true,
            _ => {}
        }
    }

    fn redraw_enabled(&self) -> bool {
        self.store.enabled("TBRedraw")
    }

    fn item_state(&self, vs: &ViewState, name: &str) -> (bool, bool) {
        // (enabled, checked)
        let en = if name == "menredraw" { self.redraw_enabled() } else { true };
        let ck = item_of(name).map(|i| vs.get(i)).unwrap_or(false);
        (en, ck)
    }

    // ---------------------------------------------------------------- menu bar
    fn menu_bar(&mut self, ui: &mut egui::Ui, bar: Rect, locked: bool) -> Option<String> {
        let ctx = ui.ctx().clone();
        let p = ui.painter().clone();
        fill(&p, bar, HILITE);
        hline(&p, bar.min.x, bar.max.x, bar.max.y - 2.0, egui::Color32::from_rgb(242, 242, 242));
        hline(&p, bar.min.x, bar.max.x, bar.max.y - 1.0, FACE);
        let font = crate::fonts::menu();
        let mut x = bar.min.x;
        let mut rects = vec![];
        for it in &self.menu {
            let w = text_size(&p, &it.cap, &font).x + 14.0;
            rects.push(Rect::from_min_size(Pos2::new(x, bar.min.y), Vec2::new(w, bar.height())));
            x += w;
        }
        let alt = ctx.input(|i| i.modifiers.alt);
        for (i, r) in rects.iter().enumerate() {
            let resp = ui.interact(*r, Id::new(("menutop", i)), Sense::click());
            if !locked {
                if resp.clicked() {
                    self.open_menu = if self.open_menu == Some(i) { None } else { Some(i) };
                    self.hot = None;
                } else if resp.hovered() && self.open_menu.is_some() && self.open_menu != Some(i) {
                    self.open_menu = Some(i);
                    self.hot = None;
                }
                let (_, _, ch) = parse_caption(&self.menu[i].cap);
                if alt && ch.and_then(|c| Key::from_name(&c.to_ascii_uppercase().to_string())).map(|k| ctx.input(|inp| inp.key_pressed(k))).unwrap_or(false) {
                    self.open_menu = Some(i);
                    self.hot = Some(0);
                }
            }
            if self.open_menu == Some(i) || resp.hovered() {
                fill(&p, *r, egui::Color32::from_rgb(204, 232, 255));
            }
            caption(&p, *r, &self.menu[i].cap, font.clone(), BLACK, HAlign::Center, true, alt);
        }
        let i = self.open_menu?;
        if locked {
            self.open_menu = None;
            return None;
        }
        self.popup(&ctx, i, rects[i], bar)
    }

    fn popup(&mut self, ctx: &Context, i: usize, anchor: Rect, bar: Rect) -> Option<String> {
        let vs = self.vs();
        let font = crate::fonts::menu();
        let p = ctx.layer_painter(egui::LayerId::new(Order::Foreground, Id::new("menumeasure")));
        let kids = &self.menu[i].kids;
        // Win32 layout: the shortcut column sits right of the widest caption that has one;
        // captions without a shortcut only need their own width.
        let (mut tw_sc, mut tw_plain, mut sw) = (0.0f32, 0.0f32, 0.0f32);
        for k in kids {
            if k.sep {
                continue;
            }
            let t = text_size(&p, &k.cap, &font).x;
            if k.sc.is_empty() {
                tw_plain = tw_plain.max(t);
            } else {
                tw_sc = tw_sc.max(t);
                sw = sw.max(text_size(&p, &k.sc, &font).x);
            }
        }
        let width = (30.0 + (tw_sc + 17.0 + sw).max(tw_plain + 19.0) + 14.0).round();
        let mut ys = vec![];
        let mut y = 3.0;
        for k in kids {
            let h = if k.sep { 8.0 } else { 23.0 };
            ys.push((y, h));
            y += h;
        }
        let size = Vec2::new(width.ceil(), y + 3.0);
        let pos = Pos2::new(anchor.min.x, bar.max.y);
        let prect = Rect::from_min_size(pos, size);
        // keyboard
        let n = kids.len();
        let usable = |j: usize| !kids[j].sep && self.item_state(&vs, &kids[j].name).0;
        let mut act: Option<String> = None;
        let (down, up, left, right, enter, esc) = ctx.input_mut(|inp| {
            (
                inp.consume_key(Modifiers::NONE, Key::ArrowDown),
                inp.consume_key(Modifiers::NONE, Key::ArrowUp),
                inp.consume_key(Modifiers::NONE, Key::ArrowLeft),
                inp.consume_key(Modifiers::NONE, Key::ArrowRight),
                inp.consume_key(Modifiers::NONE, Key::Enter),
                inp.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        let mut hot = self.hot;
        if down || up {
            let mut j = hot.map(|h| h as i32).unwrap_or(if down { -1 } else { n as i32 });
            for _ in 0..n {
                j = (j + if down { 1 } else { -1 }).rem_euclid(n as i32);
                if usable(j as usize) {
                    hot = Some(j as usize);
                    break;
                }
            }
        }
        if enter {
            if let Some(h) = hot {
                act = Some(kids[h].name.clone());
            }
        }
        if left || right {
            let m = self.menu.len();
            self.open_menu = Some(if left { (i + m - 1) % m } else { (i + 1) % m });
            self.hot = Some(0);
            return None;
        }
        if esc {
            self.open_menu = None;
            return None;
        }
        // accelerator letters
        for (j, k) in kids.iter().enumerate() {
            let (_, _, ch) = parse_caption(&k.cap);
            if let Some(key) = ch.and_then(|c| Key::from_name(&c.to_ascii_uppercase().to_string())) {
                if usable(j) && ctx.input(|inp| inp.key_pressed(key) && !inp.modifiers.ctrl) {
                    act = Some(k.name.clone());
                }
            }
        }
        let store_redraw = self.redraw_enabled();
        let assets = &self.assets;
        egui::Area::new(Id::new("menupopup")).order(Order::Foreground).fixed_pos(pos).constrain(false).show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            let pp = ui.painter().clone();
            let face = egui::Color32::from_rgb(240, 240, 240);
            let shadow = egui::Color32::from_rgb(160, 160, 160);
            fill(&pp, rect, face);
            rect_outline(&pp, rect, shadow);
            for (j, k) in kids.iter().enumerate() {
                let (yy, hh) = ys[j];
                let r = Rect::from_min_size(Pos2::new(rect.min.x + 3.0, rect.min.y + yy), Vec2::new(rect.width() - 6.0, hh));
                if k.sep {
                    hline(&pp, r.min.x + 1.0, r.max.x - 1.0, r.min.y + 4.0, shadow);
                    hline(&pp, r.min.x + 1.0, r.max.x - 1.0, r.min.y + 5.0, HILITE);
                    continue;
                }
                let en = if k.name == "menredraw" { store_redraw } else { true };
                let ck = item_of(&k.name).map(|it| vs.get(it)).unwrap_or(false);
                let resp = ui.interact(r, Id::new(("menuitem", i, j)), Sense::click());
                if resp.hovered() && ctx.input(|inp| inp.pointer.delta() != Vec2::ZERO) {
                    hot = Some(j);
                }
                let is_hot = hot == Some(j);
                if is_hot {
                    fill(&pp, Rect::from_min_max(Pos2::new(r.min.x + 24.0, r.min.y), r.max), egui::Color32::from_rgb(0, 120, 212));
                }
                if resp.clicked() && en {
                    act = Some(k.name.clone());
                }
                if ck {
                    // sunken 22x22 box with the white/face checker
                    let bx = Rect::from_min_size(Pos2::new(rect.min.x + 4.0, r.min.y + 1.0), Vec2::splat(22.0));
                    for yy2 in 0..22 {
                        for xx in 0..22 {
                            if (xx + yy2 + 4 + (r.min.y - rect.min.y) as i32 + 1) % 2 == 0 {
                                fill(&pp, Rect::from_min_size(Pos2::new(bx.min.x + xx as f32, bx.min.y + yy2 as f32), Vec2::splat(1.0)), HILITE);
                            }
                        }
                    }
                    hline(&pp, bx.min.x, bx.max.x, bx.min.y, shadow);
                    vline(&pp, bx.min.x, bx.min.y, bx.max.y, shadow);
                    hline(&pp, bx.min.x, bx.max.x, bx.max.y - 1.0, HILITE);
                    vline(&pp, bx.max.x - 1.0, bx.min.y, bx.max.y, HILITE);
                } else if is_hot && k.img.is_some() {
                    let bx = Rect::from_min_size(Pos2::new(r.min.x, r.min.y), Vec2::splat(23.0));
                    hline(&pp, bx.min.x, bx.max.x, bx.min.y, HILITE);
                    vline(&pp, bx.min.x, bx.min.y, bx.max.y, HILITE);
                    hline(&pp, bx.min.x, bx.max.x, bx.max.y - 1.0, shadow);
                    vline(&pp, bx.max.x - 1.0, bx.min.y, bx.max.y, shadow);
                }
                let off = if ck { 2.0 } else { 1.0 };
                let ir = Rect::from_min_size(Pos2::new(r.min.x + off, r.min.y + off), Vec2::splat(20.0));
                if let Some(img) = k.img {
                    crate::icons::draw_tb(&pp, assets, img, ir, en, false);
                } else if ck {
                    let c = Pos2::new(rect.min.x + 15.0, r.min.y + 12.0);
                    // Marlett-style check, pixel map measured from the native popup
                    for (dx, dy) in [(2.0, -3.0), (1.0, -2.0), (2.0, -2.0), (-4.0, -1.0), (-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (-4.0, 0.0), (-3.0, 0.0), (-1.0, 0.0), (0.0, 0.0), (1.0, 0.0), (-4.0, 1.0), (-3.0, 1.0), (-2.0, 1.0), (-1.0, 1.0), (0.0, 1.0), (-3.0, 2.0), (-2.0, 2.0), (-1.0, 2.0), (-2.0, 3.0)] {
                        fill(&pp, Rect::from_min_size(Pos2::new(c.x + dx, c.y + dy), Vec2::splat(1.0)), BLACK);
                    }
                }
                let tc = if !en { GRAYTEXT } else if is_hot { egui::Color32::WHITE } else { BLACK };
                caption(&pp, Rect::from_min_max(Pos2::new(r.min.x + 26.0, r.min.y), r.max), &k.cap, font.clone(), tc, HAlign::Left, true, true);
                if !k.sc.is_empty() {
                    caption(&pp, Rect::from_min_max(r.min, Pos2::new(r.max.x - 11.0, r.max.y)), &k.sc, font.clone(), tc, HAlign::Right, true, false);
                }
            }
        });
        self.hot = hot;
        if ctx.input(|inp| inp.pointer.primary_pressed()) {
            if let Some(pt) = ctx.input(|inp| inp.pointer.interact_pos()) {
                if !prect.contains(pt) && !bar.contains(pt) {
                    self.open_menu = None;
                }
            }
        }
        if act.is_some() {
            self.open_menu = None;
        }
        act
    }

    // ---------------------------------------------------------------- control bar
    fn control_bar(&mut self, ui: &mut egui::Ui, bar: Rect, acts: &mut Vec<String>, locked: bool) {
        let p = ui.painter().clone();
        fill(&p, bar, FACE);
        // VCL control bar frame: sunken outer line, raised inner (two white lines top/left)
        sunken1(&p, bar);
        let inner = bar.shrink(1.0);
        raised1(&p, inner);
        let vs = self.vs();
        let mut tbs: Vec<Node> = (1..=5).filter_map(|i| self.forms.form1.find(&format!("ToolBar{i}")).cloned()).collect();
        // VCL lays toolbar controls out 2px below the toolbar top, edits and combos 21px high
        for tb in tbs.iter_mut() {
            for k in tb.kids.iter_mut().filter(|k| matches!(k.class.as_str(), "TLabel" | "TEdit" | "TSpinEdit" | "TComboBox")) {
                let top = k.int("Top") + 2;
                k.props.insert("Top".into(), serde_json::json!(top));
                if matches!(k.class.as_str(), "TEdit" | "TSpinEdit") {
                    k.props.insert("Height".into(), serde_json::json!(21));
                }
            }
        }
        let base = Fnt::form_default().with(&self.forms.form1);
        let mut drag_delta: Option<(usize, Vec2, bool)> = None;
        let redraw_en = self.redraw_enabled();
        let mut r = Renderer::new(ui, &mut self.store, &self.assets);
        r.popup_open = false;
        for (i, tb) in tbs.iter().enumerate() {
            let (top, left) = self.layout.bars[i];
            let o = Pos2::new(bar.min.x + 2.0 + left as f32, bar.min.y + top as f32);
            let (_, _, w, h) = tb.rect();
            let tbr = Rect::from_min_size(o, Vec2::new(w as f32, h as f32));
            // band frame and grip
            let band = Rect::from_min_max(Pos2::new(o.x - 11.0, o.y), Pos2::new(o.x + w as f32 + 2.0, o.y + 30.0));
            raised1(&p, band);
            for gx in [o.x - 8.0, o.x - 5.0] {
                // grip bar: horizontal edges first, so the vertical edges win the corners
                let g = Rect::from_min_size(Pos2::new(gx, o.y + 2.0), Vec2::new(3.0, h as f32));
                hline(&p, g.min.x, g.max.x, g.min.y, HILITE);
                hline(&p, g.min.x, g.max.x, g.max.y - 1.0, SHADOW);
                vline(&p, g.min.x, g.min.y, g.max.y, HILITE);
                vline(&p, g.max.x - 1.0, g.min.y, g.max.y, SHADOW);
            }
            let grip = Rect::from_min_size(Pos2::new(o.x - 11.0, o.y), Vec2::new(11.0, 30.0));
            let gr = r.ui.interact(grip, Id::new(("grip", i)), Sense::drag());
            if gr.hovered() || gr.dragged() {
                r.ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            if !locked && (gr.dragged() || gr.drag_stopped()) {
                drag_delta = Some((i, gr.drag_delta(), gr.drag_stopped()));
            }
            let f = base.with(tb);
            r.draw_children_at(tb, o, &f, Some(i as i32));
            let bw = tb.int_opt("ButtonWidth").unwrap_or(23) as f32;
            let bh = tb.int_opt("ButtonHeight").unwrap_or(22) as f32;
            for k in tb.kids.iter().filter(|k| k.class == "TToolButton" && k.ident("Style") == "tbsSeparator") {
                // flat toolbar separator: gray line with a white line to its right
                let x = tbr.min.x + k.int("Left") as f32 + 3.0;
                vline(&p, x, tbr.min.y + 4.0, tbr.min.y + 4.0 + 22.0, SHADOW);
                vline(&p, x + 1.0, tbr.min.y + 4.0, tbr.min.y + 4.0 + 22.0, HILITE);
            }
            for k in tb.kids.iter().filter(|k| k.class == "TToolButton" && k.ident("Style") != "tbsSeparator") {
                let rr = Rect::from_min_size(Pos2::new(tbr.min.x + k.int("Left") as f32, tbr.min.y), Vec2::new(bw, bh));
                let id = Id::new(("tb", &k.name));
                let en = if k.name == "TBRedraw" { redraw_en } else { true };
                let resp = r.ui.interact(rr, id, if en && !locked { Sense::click() } else { Sense::hover() });
                let checked = item_of(&k.name).map(|it| vs.get(it)).unwrap_or(false);
                let down = en && resp.is_pointer_button_down_on();
                let pp = r.ui.painter().clone();
                if !checked && !down && resp.hovered() && en {
                    raised1(&pp, rr);
                }
                let ir = Rect::from_min_size(Pos2::new(rr.min.x + ((rr.width() - 20.0) / 2.0).floor(), rr.min.y + ((rr.height() - 20.0) / 2.0).floor() + 2.0), Vec2::splat(20.0));
                crate::icons::draw_tb(&pp, &self.assets, k.int("ImageIndex").max(0) as usize, ir, en, checked || down);
                r.tip(id, k.hint(), resp.hovered());
                if en && resp.clicked() {
                    acts.push(k.name.clone());
                }
            }
        }
        let evs = r.finish();
        for e in evs {
            if let Ev::Change(n) = e {
                let _ = n; // size/scale edits have no effect without a drawing engine
            }
        }
        if let Some((i, d, stop)) = drag_delta {
            let (ref mut top, ref mut left) = self.layout.bars[i];
            let w = tbs[i].int("Width");
            *left = (*left + d.x.round() as i32).clamp(11, (bar.width() as i32 - w - 2).max(11));
            *top = (*top + d.y.round() as i32).clamp(2, 32);
            self.drag = Some(i);
            if stop {
                let row = ((*top - 2) as f32 / 30.0).round().clamp(0.0, 1.0) as i32;
                *top = 2 + 30 * row;
                self.drag = None;
            }
        }
    }

    // ---------------------------------------------------------------- status bar
    fn status_bar(&self, ui: &egui::Ui, r: Rect) {
        let p = ui.painter();
        fill(p, r, FACE);
        // panels end at 100 / 200 / right edge; each later panel starts 2px after the previous boundary
        let mut left = r.min.x;
        for (i, b) in [100.0f32, 200.0, f32::MAX].iter().enumerate() {
            let right = if i == 2 { r.max.x } else { r.min.x + b };
            sunken1(p, Rect::from_min_max(Pos2::new(left, r.min.y + 2.0), Pos2::new(right, r.max.y)));
            left = right + 2.0;
        }
        // size grip: three diagonal strokes, each a white pixel followed by two gray ones
        fill(p, Rect::from_min_max(Pos2::new(r.max.x - 16.0, r.max.y - 16.0), r.max), FACE);
        let (lim, base) = (r.max.x - 2.0, r.max.y - 2.0);
        for sx in [r.max.x - 15.0, r.max.x - 10.0, r.max.x - 5.0] {
            for k in 0..14 {
                let (x, y) = (sx + k as f32, base - k as f32);
                for (d, c) in [(0.0, HILITE), (1.0, SHADOW), (2.0, SHADOW)] {
                    if x + d <= lim {
                        fill(p, Rect::from_min_size(Pos2::new(x + d, y), Vec2::splat(1.0)), c);
                    }
                }
            }
        }
    }

    fn track_window(&mut self, ctx: &Context) {
        let (outer, inner) = ctx.input(|i| (i.viewport().outer_rect, i.viewport().inner_rect));
        if let Some(o) = outer {
            self.layout.form_left = o.min.x.round() as i32;
            self.layout.form_top = o.min.y.round() as i32;
        }
        if let (Some(o), Some(_)) = (outer, inner) {
            self.layout.form_width = o.width().round() as i32;
            self.layout.form_height = o.height().round() as i32;
        }
    }

    fn shortcuts(&mut self, ctx: &Context, acts: &mut Vec<String>) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        acts.extend(shortcut_actions(ctx).into_iter().map(String::from));
    }
}

/// Ctrl+letter accelerators from the DFM ShortCut values (0x4000 | letter).
const SHORTCUTS: [(Key, &str); 6] = [(Key::L, "menload1"), (Key::S, "mensave"), (Key::C, "menClipbrd"), (Key::O, "menconfig"), (Key::R, "menredraw"), (Key::N, "menreload")];

fn shortcut_actions(ctx: &Context) -> Vec<&'static str> {
    SHORTCUTS.iter().filter(|(k, _)| ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, *k))).map(|(_, n)| *n).collect()
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;

    #[test]
    fn ctrl_letters_dispatch_to_menu_actions() {
        for (k, name) in SHORTCUTS {
            let ctx = Context::default();
            let mut raw = egui::RawInput::default();
            raw.events.push(egui::Event::Key { key: k, physical_key: Some(k), pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
            let _ = ctx.begin_pass(raw);
            assert_eq!(shortcut_actions(&ctx), vec![name]);
            ctx.end_pass().textures_delta.clear();
        }
    }

    #[test]
    fn plain_letter_does_not_dispatch() {
        let ctx = Context::default();
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::Key { key: Key::O, physical_key: Some(Key::O), pressed: true, repeat: false, modifiers: Modifiers::NONE });
        let _ = ctx.begin_pass(raw);
        assert!(shortcut_actions(&ctx).is_empty());
        ctx.end_pass().textures_delta.clear();
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.first {
            self.first = false;
            dialogs::apply_interlocks(&mut self.store);
            let files = std::mem::take(&mut self.start_files);
            self.load_files(&ctx, files);
        }
        if !self.started_hwnd {
            self.dlg.main_hwnd = plat::find(None);
            self.started_hwnd = self.dlg.main_hwnd.is_some();
        }
        self.track_window(&ctx);
        let locked = self.dlg.any();
        let full = ctx.content_rect();
        let p = ui.painter().clone();
        fill(&p, full, FACE);
        let mut acts: Vec<String> = vec![];
        if !locked && self.open_menu.is_none() {
            self.shortcuts(&ctx, &mut acts);
        }
        // title
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title.clone()));
        // geometry
        let mbar = Rect::from_min_size(full.min, Vec2::new(full.width(), MENU_H));
        let cbar = Rect::from_min_size(Pos2::new(full.min.x, mbar.max.y), Vec2::new(full.width(), BAR_H));
        let sbar = Rect::from_min_max(Pos2::new(full.min.x, full.max.y - STATUS_H), full.max);
        let sbox = Rect::from_min_max(Pos2::new(full.min.x, cbar.max.y), Pos2::new(full.max.x, sbar.min.y));
        // scroll box
        fill(&p, sbox, FACE);
        sunken(&p, sbox);
        if let Some(tex) = &self.canvas_tex {
            let f = |n: &str| self.store.text(n).trim().parse::<f32>().ok().filter(|v| *v > 0.0).unwrap_or(1.0);
            let (mh, mv) = (f("EDh"), f("EDv"));
            let sz = tex.size_vec2() * Vec2::new(mh, mv);
            let inner = sbox.shrink(2.0);
            ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add(egui::Image::new((tex.id(), sz)));
                });
            });
        }
        self.control_bar(ui, cbar, &mut acts, locked);
        self.status_bar(ui, sbar);
        if !self.status_msg.is_empty() {
            let tr = Rect::from_min_max(Pos2::new(sbar.min.x + 202.0, sbar.min.y + 2.0), Pos2::new(sbar.max.x - 16.0, sbar.max.y));
            p.with_clip_rect(tr).text(Pos2::new(tr.min.x + 4.0, tr.center().y), egui::Align2::LEFT_CENTER, &self.status_msg, egui::FontId::proportional(12.0), egui::Color32::BLACK);
        }
        if let Some(a) = self.menu_bar(ui, mbar, locked) {
            acts.push(a);
        }
        // dropped files
        if !locked {
            let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
            if !dropped.is_empty() {
                self.load_files(&ctx, dropped);
            }
        }
        let center = {
            let o = ctx.input(|i| i.viewport().outer_rect);
            o.map(|r| r.center()).unwrap_or(Pos2::new(300.0, 250.0))
        };
        if !locked || self.dlg.any() {
            // actions first so newly opened dialogs appear in the same frame
            if !locked {
                for a in acts {
                    self.action(&ctx, &a);
                }
            }
        }
        let cfg_was_open = self.dlg.config.is_some();
        {
            let mut sh = Shared { forms: &self.forms, store: &mut self.store, assets: &self.assets, center };
            self.dlg.show(&ctx, &mut sh);
        }
        if cfg_was_open && self.dlg.config.is_none() && self.data.is_some() {
            self.render_data(&ctx);
        }
        if self.dlg.any() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let _ = ctx.input(|i| i.viewport().close_requested());
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.store.checked("CBautosave") {
            let p = self.ini_path.clone();
            self.save_ini(&p);
        }
    }
}
