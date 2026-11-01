//! Main window: menu bar, control bar with five tool bands, scroll box, status bar and the actions.
use crate::batch;
use crate::dialogs::{self, BatchReq, Dialogs, Shared};
use crate::engine::export::{self, bmp};
use crate::engine::pipeline::{self, Loaded, Rendered};
use crate::engine::settings::Settings;
use crate::forms::{Forms, Node};
use crate::ini::{self, Layout};
use crate::jobs::{self, BatchKind, Built, Handle, MeshDone, RenderDone};
use crate::platform_win32 as plat;
use crate::state::Store;
use crate::vcl::form::{Assets, Fnt, Renderer};
use crate::vcl::*;
use crate::view::{Item, ViewState};
use egui::{Context, Id, Key, Modifiers, Order, Pos2, Rect, Sense, Vec2};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const HELP_HTML: &str = include_str!("../assets/help.html");
/// A progress window appears only for jobs that take longer than this.
const PROGRESS_DELAY: std::time::Duration = std::time::Duration::from_millis(400);

enum Job {
    Load(Handle<Built>),
    Reparse(Handle<Loaded>),
    Render(Handle<RenderDone>),
    Mesh(Handle<MeshDone>),
    Batch(Handle<String>),
}

enum PendingIrregular {
    Load(Built),
    Reparse { loaded: Loaded, store: Store },
}

const IRREGULAR_BUTTONS: [&str; 3] = ["はい(Y)", "中止(A)", "無視(I)"];

fn irregular_shortcut(key: Key) -> Option<dialogs::IrregularChoice> {
    match key {
        Key::Y => Some(dialogs::IrregularChoice::Generate),
        Key::N => Some(dialogs::IrregularChoice::Abort),
        Key::A => Some(dialogs::IrregularChoice::Abort),
        Key::I => Some(dialogs::IrregularChoice::Ignore),
        _ => None,
    }
}

fn irregular_close_choice() -> dialogs::IrregularChoice { dialogs::IrregularChoice::Abort }

fn irregular_enter_choice(pressed: bool) -> Option<dialogs::IrregularChoice> {
    pressed.then_some(dialogs::IrregularChoice::Generate)
}

struct AppPrevious {
    store: Store,
    title: String,
    bytes: Option<Arc<Vec<u8>>>,
    data_path: Option<PathBuf>,
    loaded: Option<Loaded>,
    rendered: Option<Rendered>,
    canvas_tex: Option<egui::TextureHandle>,
    status_msg: String,
    pending_warnings: Vec<String>,
    probe_cells: [String; 3],
}

impl Job {
    /// Shared state, start time and whether the progress window may be shown.
    fn sh(&self) -> (&jobs::Sh, std::time::Instant, bool) {
        match self {
            Job::Load(h) => (&h.sh, h.started, true),
            Job::Reparse(h) => (&h.sh, h.started, true),
            Job::Render(h) => (&h.sh, h.started, true),
            Job::Mesh(h) => (&h.sh, h.started, true),
            Job::Batch(h) => (&h.sh, h.started, true),
        }
    }
}

const MENU_H: f32 = 20.0;
const BAR_H: f32 = 64.0;
const STATUS_H: f32 = 22.0;
const NODATA: &str = "avese - nodata";

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

// Toolbar state is shown behind the icon, so the artwork stays legible.
fn toolbar_background(enabled: bool, checked: bool, pressed: bool, hovered: bool) -> Option<(egui::Color32, egui::Color32)> {
    use egui::Color32;
    if !enabled {
        return checked.then_some((Color32::from_gray(225), Color32::from_gray(180)));
    }
    if pressed {
        Some((Color32::from_rgb(169, 204, 244), Color32::from_rgb(42, 106, 180)))
    } else if checked {
        Some((Color32::from_rgb(207, 228, 251), Color32::from_rgb(55, 119, 190)))
    } else if hovered {
        Some((Color32::from_rgb(230, 237, 245), Color32::from_rgb(172, 190, 211)))
    } else {
        None
    }
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
    batch_root_hidden: bool,
    batch_hide_pending: bool,
    batch_progress_frames: u8,
    bytes: Option<Arc<Vec<u8>>>,
    data_path: Option<PathBuf>,
    loaded: Option<Loaded>,
    rendered: Option<Rendered>,
    canvas_tex: Option<egui::TextureHandle>,
    status_msg: String,
    job_result: Option<Result<String, String>>,
    /// X, Y, Z panels of the status bar for the pixel under the mouse
    probe_cells: [String; 3],
    job: Option<Job>,
    pending_warnings: Vec<String>,
    previous: Option<AppPrevious>,
    pending_irregular: Option<PendingIrregular>,
    irregular_focus_initialized: bool,
}

pub fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default()
}

impl App {
    pub fn new(ctx: &Context, forms: Forms, store: Store, layout: Layout, files: Vec<PathBuf>) -> App {
        let mut assets = Assets::default();
        // Embed the generated artwork so packaged and local builds look the same.
        if let Ok(img) = image::load_from_memory(include_bytes!("../assets/generated/plot-icons.png")) {
            let img = img.to_rgba8();
            assets.plot_icons = Some(ctx.load_texture("plot-icons", egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()), egui::TextureOptions::LINEAR));
        }
        if let Ok(img) = image::load_from_memory(include_bytes!("../assets/generated/avese-logo.png")) {
            let img = img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3).to_rgba8();
            let texture = ctx.load_texture("avese-logo", egui::ColorImage::from_rgba_unmultiplied([64, 64], img.as_raw()), egui::TextureOptions::LINEAR);
            assets.about = Some(texture.clone());
            assets.app = Some(texture);
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
            batch_root_hidden: false,
            batch_hide_pending: false,
            batch_progress_frames: 0,
            bytes: None,
            data_path: None,
            loaded: None,
            rendered: None,
            canvas_tex: None,
            status_msg: String::new(),
            job_result: None,
            probe_cells: Default::default(),
            job: None,
            pending_warnings: vec![],
            previous: None,
            pending_irregular: None,
            irregular_focus_initialized: false,
        }
    }

    fn vs(&self) -> ViewState {
        ViewState::from_store(&self.store)
    }

    fn show_error(&self, text: &str) {
        self.dlg.modal(|| dialogs::error("avese", text));
    }

    fn load_files(&mut self, ctx: &Context, files: Vec<PathBuf>) {
        match files.len() {
            0 => {}
            1 => self.start_load(ctx, files[0].clone()),
            _ => {
                self.store.lists.insert("savelist".into(), files.iter().map(|p| p.display().to_string()).collect());
                self.store.set_item_index("savelist", -1);
                self.dlg.avi = true;
            }
        }
    }

    /// Read the file and build its field on a worker; embedded settings are applied when it is done.
    fn start_load(&mut self, ctx: &Context, path: PathBuf) {
        if self.job.is_none() {
            self.job = Some(Job::Load(jobs::load_job(ctx, path, self.store.clone())));
        }
    }

    /// Draw again with the current settings; the data are parsed again only if a setting that affects
    /// reading has changed, so settings embedded in the file are not applied a second time.
    fn redraw(&mut self, ctx: &Context) {
        if self.job.is_some() {
            return;
        }
        let Some(bytes) = self.bytes.clone() else { return };
        let s = Settings::from_store(&self.store);
        if self.loaded.as_ref().is_some_and(|l| !l.is_stale(&s)) {
            self.start_render(ctx);
        } else {
            let mesh_auto = self.loaded.as_ref().is_some_and(|l| l.data.mesh_auto);
            self.job = Some(Job::Reparse(jobs::reparse_job_with_mesh(ctx, bytes, self.data_path.clone(), s, mesh_auto)));
        }
    }

    fn create_mesh(&mut self, ctx: &Context) {
        if self.job.is_some() { return; }
        let Some(loaded) = self.loaded.take() else { return };
        if loaded.data.mesh_auto || (matches!(&*loaded.field, crate::engine::field::Field::Panel(_)) && !loaded.irregular_data) {
            self.loaded = Some(loaded);
            return;
        }
        let Some(bytes) = self.bytes.clone() else {
            self.loaded = Some(loaded);
            return;
        };
        self.job = Some(Job::Mesh(jobs::mesh_job(ctx, loaded, bytes, self.data_path.clone(), Settings::from_store(&self.store))));
    }

    fn restore_previous(&mut self) {
        if let Some(previous) = self.previous.take() {
            self.store = previous.store;
            self.title = previous.title;
            self.bytes = previous.bytes;
            self.data_path = previous.data_path;
            self.loaded = previous.loaded;
            self.rendered = previous.rendered;
            self.canvas_tex = previous.canvas_tex;
            self.status_msg = previous.status_msg;
            self.pending_warnings = previous.pending_warnings;
            self.probe_cells = previous.probe_cells;
        }
    }

    /// Asks before drawing an image larger than SEmaxbmp, then draws on a worker.
    fn start_render(&mut self, ctx: &Context) {
        let Some(loaded) = self.loaded.take() else { return };
        let s = Settings::from_store(&self.store);
        let need = pipeline::required_bytes(&loaded, &s);
        if jobs::over_limit(need, s.max_bmp_mb) {
            let msg = format!("イメージのサイズが {} MB になり、設定された上限 {} MB を超えます。\n描画を続けますか？", need.div_ceil(1024 * 1024), s.max_bmp_mb);
            if !self.dlg.modal(|| dialogs::confirm("avese", &msg)) {
                self.loaded = Some(loaded);
                self.status_msg = "描画を中止しました".into();
                return;
            }
        }
        self.job = Some(Job::Render(jobs::render_job(ctx, loaded, s, self.store.list("LBlabel").to_vec())));
    }

    /// A field was built: show the ranges in the settings dialog, then draw.
    fn built(&mut self, ctx: &Context, loaded: Loaded) {
        if loaded.irregular_data {
            self.irregular_focus_initialized = false;
            self.pending_irregular = Some(PendingIrregular::Reparse { loaded, store: self.store.clone() });
            return;
        }
        jobs::readback(&mut self.store, &loaded.field);
        self.pending_warnings = loaded.warnings.clone();
        self.loaded = Some(loaded);
        self.start_render(ctx);
    }

    fn choose_irregular(&mut self, ctx: &Context, choice: dialogs::IrregularChoice) {
        let Some(pending) = self.pending_irregular.take() else { return };
        self.irregular_focus_initialized = false;
        match pending {
            PendingIrregular::Load(mut b) => {
                if choice == dialogs::IrregularChoice::Abort {
                    self.status_msg = "データ読み込みを中止しました".into();
                    return;
                }
                if choice == dialogs::IrregularChoice::Ignore {
                    b.loaded.warnings.retain(|w| !w.starts_with("データが格子状に並んでいません"));
                }
                if choice == dialogs::IrregularChoice::Generate {
                    self.previous = Some(AppPrevious {
                        store: self.store.clone(), title: self.title.clone(), bytes: self.bytes.clone(),
                        data_path: self.data_path.clone(), loaded: self.loaded.take(), rendered: self.rendered.take(),
                        canvas_tex: self.canvas_tex.take(), status_msg: self.status_msg.clone(),
                        pending_warnings: self.pending_warnings.clone(), probe_cells: self.probe_cells.clone(),
                    });
                }
                self.finish_load(ctx, b, Some(choice));
            }
            PendingIrregular::Reparse { mut loaded, store } => match choice {
                dialogs::IrregularChoice::Abort => {
                    self.store = store;
                    self.status_msg = "データ読み込みを中止しました".into();
                }
                dialogs::IrregularChoice::Generate => {
                    jobs::readback(&mut self.store, &loaded.field);
                    self.loaded = Some(loaded);
                    self.create_mesh(ctx);
                }
                dialogs::IrregularChoice::Ignore => {
                    jobs::readback(&mut self.store, &loaded.field);
                    loaded.warnings.retain(|w| !w.starts_with("データが格子状に並んでいません"));
                    self.pending_warnings = loaded.warnings.clone();
                    self.loaded = Some(loaded);
                    self.start_render(ctx);
                }
            },
        }
    }

    fn finish_load(&mut self, ctx: &Context, b: Built, choice: Option<dialogs::IrregularChoice>) {
        let mut loaded = b.loaded;
        self.store = b.store;
        dialogs::apply_interlocks(&mut self.store);
        let n = loaded.path.as_ref().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        self.title = format!("avese - {n}");
        self.store.set_enabled("TBRedraw", true);
        self.bytes = Some(b.bytes);
        self.data_path = loaded.path.clone();
        if choice == Some(dialogs::IrregularChoice::Generate) {
            self.store.set_checked("CBpanel", false);
            jobs::readback(&mut self.store, &loaded.field);
            self.loaded = Some(loaded);
            self.create_mesh(ctx);
        } else if choice == Some(dialogs::IrregularChoice::Ignore) {
            jobs::readback(&mut self.store, &loaded.field);
            self.pending_warnings = loaded.warnings.clone();
            self.loaded = Some(loaded);
            self.start_render(ctx);
        } else {
            self.built(ctx, loaded);
        }
    }

    fn show_irregular_modal(&mut self, ctx: &Context) {
        if self.pending_irregular.is_none() { return; }
        let mut choice = None;
        let modal = egui::Modal::new(Id::new("irregular-grid-choice")).show(ctx, |ui| {
            ui.set_min_width(350.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(egui_phosphor::regular::WARNING)
                    .font(egui::FontId::new(24.0, egui::FontFamily::Name("phosphor".into())))
                    .color(egui::Color32::from_rgb(210, 150, 0)));
                ui.heading("警告");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text("中止").clicked() {
                        choice = Some(irregular_close_choice());
                    }
                });
            });
            ui.separator();
            ui.label("非構造格子データです\n三角メッシュを自動生成しますか？\n（時間がかかる場合があります）");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let yes = ui.button(IRREGULAR_BUTTONS[0]);
                if !self.irregular_focus_initialized {
                    yes.request_focus();
                    self.irregular_focus_initialized = true;
                }
                if yes.clicked() { choice = Some(dialogs::IrregularChoice::Generate); }
                if ui.button(IRREGULAR_BUTTONS[1]).clicked() { choice = Some(dialogs::IrregularChoice::Abort); }
                if ui.button(IRREGULAR_BUTTONS[2]).clicked() { choice = Some(dialogs::IrregularChoice::Ignore); }
            });
        });
        if choice.is_none() {
            choice = ctx.input_mut(|i| irregular_enter_choice(i.consume_key(Modifiers::NONE, Key::Enter)));
        }
        if choice.is_none() {
            choice = ctx.input_mut(|i| {
                [Key::Y, Key::N, Key::A, Key::I].into_iter().find_map(|key| {
                    (i.consume_key(Modifiers::ALT, key) || i.consume_key(Modifiers::NONE, key))
                        .then(|| irregular_shortcut(key)).flatten()
                })
            });
        }
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested {
            // The dialog is embedded in the root viewport. Turn its title-bar X
            // into a safe abort instead of letting it close the whole app.
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            choice = Some(irregular_close_choice());
        }
        let escape = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        if choice.is_none() && (escape || modal.should_close()) {
            choice = Some(irregular_close_choice());
        }
        if let Some(choice) = choice {
            self.choose_irregular(ctx, choice);
        }
    }

    fn rendered(&mut self, ctx: &Context, done: RenderDone) {
        self.loaded = Some(done.loaded);
        match done.result {
            Ok(r) => {
                let img = egui::ColorImage::from_rgba_premultiplied(r.size(), &r.rgba());
                self.canvas_tex = Some(ctx.load_texture("canvas", img, egui::TextureOptions::NEAREST));
                self.rendered = Some(r);
                self.status_msg = self.pending_warnings.first().cloned().unwrap_or_default();
                if !self.pending_warnings.is_empty() {
                    let text = jobs::warnings_text(&std::mem::take(&mut self.pending_warnings));
                    self.dlg.modal(|| dialogs::warn("avese", &text));
                }
            }
            Err(e) => {
                if e == jobs::CANCELLED {
                    self.status_msg = "描画を中止しました".into();
                    return;
                }
                self.canvas_tex = None;
                self.rendered = None;
                self.status_msg = e.clone();
                self.show_error(&e);
            }
        }
    }

    /// Collect the result of the running job, if it is done.
    fn poll_job(&mut self, ctx: &Context) {
        // reported one frame after the job ended, so the progress window is already closed
        match self.job_result.take() {
            Some(Ok(m)) => {
                self.status_msg.clear();
                if self.dlg.avi {
                    self.dlg.batch_notice = Some(m);
                } else {
                    self.dlg.modal(|| dialogs::notice("avese", &m));
                }
            }
            Some(Err(e)) if self.dlg.avi => {
                self.status_msg = e.clone();
                if e != jobs::CANCELLED { self.dlg.batch_notice = Some(e); }
            }
            Some(Err(e)) => self.job_failed(e),
            None => {}
        }
        let Some(job) = self.job.take() else { return };
        if std::mem::take(&mut self.dlg.mesh_cancel) {
            job.sh().0.cancel();
            // Keep an active viewport available while a slow worker winds down.
            // Otherwise cancelling closes the last visible window before the
            // worker has returned and the main viewport can be restored.
            if matches!(job, Job::Batch(_)) {
                self.batch_hide_pending = false;
                if self.batch_root_hidden {
                    self.batch_root_hidden = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                }
            }
            // loading and parsing have nothing to keep
            if matches!(job, Job::Load(_) | Job::Reparse(_)) {
                self.status_msg = jobs::CANCELLED.into();
                self.dlg.mesh = false;
                return;
            }
        }
        match job {
            Job::Load(h) => match h.poll() {
                None => self.job = Some(Job::Load(h)),
                Some(Err(e)) => self.job_result = Some(Err(e)),
                Some(Ok(b)) => {
                    if b.loaded.irregular_data {
                        self.irregular_focus_initialized = false;
                        self.pending_irregular = Some(PendingIrregular::Load(b));
                    } else {
                        self.finish_load(ctx, b, None);
                    }
                }
            },
            Job::Reparse(h) => match h.poll() {
                None => self.job = Some(Job::Reparse(h)),
                Some(Err(e)) => self.job_result = Some(Err(e)),
                Some(Ok(l)) => self.built(ctx, l),
            },
            Job::Render(h) => match h.poll() {
                None => self.job = Some(Job::Render(h)),
                Some(Err(e)) => self.job_result = Some(Err(e)),
                Some(Ok(d)) => self.rendered(ctx, d),
            },
            Job::Mesh(h) => match h.poll() {
                None => self.job = Some(Job::Mesh(h)),
                Some(Err(e)) => self.job_result = Some(Err(e)),
                Some(Ok(done)) => {
                    let jobs::MeshDone { loaded, result } = done;
                    match result {
                        Ok(()) => {
                            self.previous = None;
                            self.built(ctx, loaded)
                        }
                        Err(e) if e == jobs::CANCELLED => {
                            if self.previous.is_some() {
                                self.restore_previous();
                            } else {
                                self.loaded = Some(loaded);
                            }
                            self.status_msg = "メッシュ生成を中止しました".into();
                        }
                        Err(e) => {
                            if self.previous.is_some() {
                                self.restore_previous();
                            } else {
                                self.loaded = Some(loaded);
                            }
                            self.job_result = Some(Err(e));
                        }
                    }
                }
            },
            Job::Batch(h) => match h.poll() {
                None => self.job = Some(Job::Batch(h)),
                Some(r) => {
                    self.batch_hide_pending = false;
                    if self.batch_root_hidden {
                        self.batch_root_hidden = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    }
                    self.dlg.mesh = false;
                    self.job_result = Some(r);
                },
            },
        }
        if self.job_result.is_some() {
            ctx.request_repaint();
        }
        // progress window and input lock for whatever is still running
        self.dlg.busy = matches!(self.job, Some(Job::Batch(_)));
        match &self.job {
            Some(j) => {
                let (sh, started, can_show) = j.sh();
                let all = if matches!(j, Job::Batch(_)) { sh.all() } else { sh.task() };
                self.store.set_position("ProgressBar2", (sh.task() * 100.0) as i32);
                self.store.set_position("ProgressBar1", (all * 400.0) as i32);
                self.dlg.mesh = can_show && !sh.cancelled() && started.elapsed() > PROGRESS_DELAY;
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
            None => self.dlg.mesh = false,
        }
    }

    fn job_failed(&mut self, e: String) {
        self.status_msg = e.clone();
        if e != jobs::CANCELLED {
            self.show_error(&e);
        }
    }

    fn start_batch(&mut self, ctx: &Context, req: BatchReq) {
        let files = self.store.list("savelist").to_vec();
        let kind = match req {
            BatchReq::Bmp(dir) => {
                let targets = batch::bmp_targets(&dir, &files);
                let exist = targets.iter().filter(|t| t.exists()).count();
                if exist > 0 && !self.dlg.modal(|| dialogs::confirm("avese", &format!("{exist} 個のファイルが既にあります。上書きしますか？"))) {
                    return;
                }
                BatchKind::Bmp(targets)
            }
            BatchReq::Avi(path) => {
                let fps = self.store.position("SEframerate");
                if fps <= 0 {
                    self.dlg.modal(|| dialogs::notice("avese", "フレームレートには 1 以上を指定してください。"));
                    return;
                }
                let size = self.store.checked("CBAVIsize").then(|| (self.store.position("SEAVIwitdh"), self.store.position("SEAVIheight")));
                BatchKind::Avi { path, fps: fps as f64, size }
            }
        };
        self.job = Some(Job::Batch(jobs::batch_job(ctx, files, self.store.clone(), kind)));
        self.batch_hide_pending = true;
        self.batch_progress_frames = 0;
    }

    fn save_image(&mut self) {
        let Some(r) = self.rendered.as_ref() else { return self.no_image() };
        let use_wmf = self.store.checked("RBusewmf");
        let Some((path, fmt)) = self.dlg.modal(|| crate::save_dialog::pick(use_wmf)) else { return };
        if let Err(e) = export::save_image(&path, r, fmt) {
            self.show_error(&e);
        }
    }

    fn copy_clipboard(&mut self) {
        let Some(r) = self.rendered.as_ref() else { return self.no_image() };
        let res = if self.store.checked("RBusewmf") {
            crate::clipboard::copy_emf(&export::emf::encode_emf(&r.scene))
        } else {
            let [w, h] = r.size();
            crate::clipboard::copy_bitmap(&bmp::quantize(r.pixmap.data(), w as u32, h as u32))
        };
        if let Err(e) = res {
            self.show_error(&e);
        }
    }

    fn no_image(&self) {
        self.dlg.modal(|| dialogs::notice("avese", "イメージがありません。先にデータを読み込んでください。"));
    }

    fn close_data(&mut self) {
        self.title = NODATA.into();
        self.store.set_enabled("TBRedraw", false);
        self.bytes = None;
        self.data_path = None;
        self.loaded = None;
        self.rendered = None;
        self.canvas_tex = None;
        self.status_msg.clear();
        self.pending_warnings.clear();
        self.probe_cells = Default::default();
    }

    /// Our own help page, written to the temp folder and opened with the default browser.
    fn open_help(&self) {
        let p = std::env::temp_dir().join("avese-help.html");
        if let Err(e) = std::fs::write(&p, HELP_HTML) {
            self.show_error(&format!("ヘルプを書き出せません: {e}"));
            return;
        }
        #[cfg(windows)]
        let r = std::process::Command::new("cmd").args(["/C", "start", ""]).arg(&p).spawn();
        #[cfg(target_os = "macos")]
        let r = std::process::Command::new("open").arg(&p).spawn();
        #[cfg(all(unix, not(target_os = "macos")))]
        let r = std::process::Command::new("xdg-open").arg(&p).spawn();
        if let Err(e) = r {
            self.show_error(&format!("ブラウザを起動できません: {e}"));
        }
    }

    fn save_ini(&self, p: &Path) {
        if let Err(e) = ini::save_file(p, &self.store, &self.layout) {
            self.dlg.modal(|| dialogs::notice("avese", &format!("設定を保存できません: {e}")));
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
            "menload1" | "TBload1" => {
                let f = self.dlg.modal(dialogs::pick_open_files);
                self.load_files(ctx, f);
            }
            "menreload" | "TBReload" => match self.data_path.clone() {
                Some(p) => self.start_load(ctx, p),
                None => self.dlg.modal(|| dialogs::notice("avese", "再読み込みするデータがありません。")),
            },
            "menload2" => self.dlg.avi = true,
            "mensave" | "TBsave" => self.save_image(),
            "menClipbrd" | "TBclip" => self.copy_clipboard(),
            "menclose" => self.close_data(),
            "menredraw" | "TBRedraw" => self.redraw(ctx),
            "menmeshcreate" => self.create_mesh(ctx),
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
        let en = menu_item_enabled(name, self.redraw_enabled(), self.loaded.as_ref());
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
        // The last two popups have small per-menu right-margin differences in the
        // Win32 reference (redraw +2px, help +1px). Their caption/shortcut widths
        // alone do not account for that native menu-template padding.
        let native_margin = match i {
            3 => 2.0, // 再描画
            4 => 1.0, // ヘルプ
            _ => 0.0,
        };
        let width = (30.0 + (tw_sc + 17.0 + sw).max(tw_plain + 19.0) + 14.0).round() + native_margin;
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
                let en = self.item_state(&vs, &k.name).0;
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
                if let Some((background, border)) = toolbar_background(en, checked, down, resp.hovered()) {
                    let state_rect = rr.shrink(1.0);
                    pp.rect_filled(state_rect, 4, background);
                    pp.rect_stroke(state_rect, 4, egui::Stroke::new(1.0, border), egui::StrokeKind::Inside);
                }
                let icon_size = 22.0_f32.min(rr.height() - 4.0).min(rr.width() - 4.0);
                let ir = Rect::from_center_size(rr.center(), Vec2::splat(icon_size));
                crate::icons::draw_tb(&pp, &self.assets, k.int("ImageIndex").max(0) as usize, ir, en, false);
                r.tip(id, k.hint(), resp.hovered());
                if en && resp.clicked() {
                    acts.push(k.name.clone());
                }
            }
        }
        // size and scale edits are read when the image is drawn again
        let _ = r.finish();
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
        let font = egui::FontId::proportional(12.0);
        let p = ui.painter();
        fill(p, r, FACE);
        // panels end at 100 / 200 / right edge; each later panel starts 2px after the previous boundary
        let mut left = r.min.x;
        for (i, b) in [100.0f32, 200.0, f32::MAX].iter().enumerate() {
            let right = if i == 2 { r.max.x } else { r.min.x + b };
            sunken1(p, Rect::from_min_max(Pos2::new(left, r.min.y + 2.0), Pos2::new(right, r.max.y)));
            left = right + 2.0;
        }
        let [x, y, z] = &self.probe_cells;
        let text = |from: f32, to: f32, s: &str| p.with_clip_rect(Rect::from_min_max(Pos2::new(from + 2.0, r.min.y + 2.0), Pos2::new(to, r.max.y))).text(Pos2::new(from + 4.0, r.center().y + 1.0), egui::Align2::LEFT_CENTER, s, font.clone(), egui::Color32::BLACK);
        text(r.min.x, r.min.x + 100.0, x);
        text(r.min.x + 102.0, r.min.x + 200.0, y);
        text(r.min.x + 202.0, r.max.x - 16.0, if z.is_empty() { &self.status_msg } else { z });
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
        // accelerators work whatever has focus (CBmode starts focused); only copying text stays with a text box
        let in_text = ctx.memory(|m| m.focused()).is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
        acts.extend(shortcut_actions(ctx, in_text).into_iter().map(String::from));
    }
}

fn menu_item_enabled(name: &str, redraw_enabled: bool, loaded: Option<&Loaded>) -> bool {
    match name {
        "menredraw" => redraw_enabled,
        "menmeshcreate" => loaded.is_some_and(|l| !l.data.mesh_auto && !matches!(&*l.field, crate::engine::field::Field::Panel(_))),
        _ => true,
    }
}

/// Ctrl+letter accelerators from the DFM ShortCut values (0x4000 | letter).
/// Ctrl+C (menClipbrd) is missing: egui-winit delivers it as Event::Copy, without a key event.
const SHORTCUTS: [(Key, &str); 5] = [(Key::L, "menload1"), (Key::S, "mensave"), (Key::O, "menconfig"), (Key::R, "menredraw"), (Key::N, "menreload")];

fn shortcut_actions(ctx: &Context, in_text: bool) -> Vec<&'static str> {
    let mut acts: Vec<&'static str> = SHORTCUTS.iter().filter(|(k, _)| ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, *k))).map(|(_, n)| *n).collect();
    if !in_text && ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy))) {
        acts.push("menClipbrd");
    }
    acts
}

#[cfg(test)]
mod shortcut_tests {
    use super::*;

    #[test]
    fn exit_saves_settings_and_labels_only_when_enabled() {
        let path = std::env::temp_dir().join(format!("avese-autosave-{}-{:?}.ini", std::process::id(), std::thread::current().id()));
        let forms = Forms::load();
        let store = Store::from_forms(&[&forms.form1, &forms.config, &forms.avi, &forms.input, &forms.about, &forms.mesh]);
        let mut app = App::new(&Context::default(), forms, store, Layout::default(), vec![]);
        app.ini_path = path.clone();
        std::fs::write(&path, b"unchanged").unwrap();
        app.store.set_checked("CBautosave", false);
        eframe::App::on_exit(&mut app, None);
        assert_eq!(std::fs::read(&path).unwrap(), b"unchanged");

        app.store.set_checked("CBautosave", true);
        app.store.set_checked("CBLoadConf", false);
        app.store.set_checked("CBDataCheck", false);
        app.store.set_position("SEmaxbmp", 17);
        app.store.lists.insert("LBlabel".into(), vec!["1, 2 日本語 😀".into()]);
        eframe::App::on_exit(&mut app, None);
        let mut restored = crate::engine::settings::Settings::default_store();
        crate::ini::load_file(&path, &mut restored, &mut Layout::default()).unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(restored.checked("CBautosave"));
        assert!(!restored.checked("CBLoadConf"));
        assert!(!restored.checked("CBDataCheck"));
        assert_eq!(restored.position("SEmaxbmp"), 17);
        assert_eq!(restored.list("LBlabel"), &["1, 2 日本語 😀"]);
    }

    #[test]
    fn toolbar_states_are_visually_distinct() {
        let normal = toolbar_background(true, false, false, false);
        let hovered = toolbar_background(true, false, false, true);
        let selected = toolbar_background(true, true, false, false);
        let pressed = toolbar_background(true, true, true, true);
        assert_eq!(normal, None);
        assert!(hovered.is_some() && selected.is_some() && pressed.is_some());
        assert_ne!(hovered, selected);
        assert_ne!(selected, pressed);
        assert_eq!(toolbar_background(false, false, false, true), None);
        assert_ne!(toolbar_background(false, true, false, true), selected);
    }

    #[test]
    fn irregular_choices_match_original_labels_and_accelerators() {
        assert_eq!(IRREGULAR_BUTTONS, ["はい(Y)", "中止(A)", "無視(I)"]);
        assert_eq!(irregular_shortcut(Key::Y), Some(dialogs::IrregularChoice::Generate));
        assert_eq!(irregular_shortcut(Key::N), Some(dialogs::IrregularChoice::Abort));
        assert_eq!(irregular_shortcut(Key::A), Some(dialogs::IrregularChoice::Abort));
        assert_eq!(irregular_shortcut(Key::I), Some(dialogs::IrregularChoice::Ignore));
        assert_eq!(irregular_shortcut(Key::Escape), None);
        assert_eq!(irregular_close_choice(), dialogs::IrregularChoice::Abort);
        assert_eq!(irregular_enter_choice(true), Some(dialogs::IrregularChoice::Generate));
        assert_eq!(irregular_enter_choice(false), None);
    }

    #[test]
    fn ctrl_letters_dispatch_to_menu_actions() {
        for (k, name) in SHORTCUTS {
            let ctx = Context::default();
            let mut raw = egui::RawInput::default();
            raw.events.push(egui::Event::Key { key: k, physical_key: Some(k), pressed: true, repeat: false, modifiers: Modifiers::COMMAND });
            let _ = ctx.begin_pass(raw);
            assert_eq!(shortcut_actions(&ctx, true), vec![name]);
            ctx.end_pass().textures_delta.clear();
        }
    }

    #[test]
    fn copy_event_dispatches_to_clipboard() {
        let ctx = Context::default();
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::Copy);
        let _ = ctx.begin_pass(raw);
        assert_eq!(shortcut_actions(&ctx, false), vec!["menClipbrd"]);
        ctx.end_pass().textures_delta.clear();
    }

    #[test]
    fn copy_event_stays_with_a_text_box() {
        let ctx = Context::default();
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::Copy);
        let _ = ctx.begin_pass(raw);
        assert!(shortcut_actions(&ctx, true).is_empty());
        ctx.end_pass().textures_delta.clear();
    }

    #[test]
    fn plain_letter_does_not_dispatch() {
        let ctx = Context::default();
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::Key { key: Key::O, physical_key: Some(Key::O), pressed: true, repeat: false, modifiers: Modifiers::NONE });
        let _ = ctx.begin_pass(raw);
        assert!(shortcut_actions(&ctx, false).is_empty());
        ctx.end_pass().textures_delta.clear();
    }

    #[test]
    fn cancelled_redraw_keeps_previous_canvas_and_allows_redraw() {
        let ctx = Context::default();
        let forms = Forms::load();
        let store = Settings::default_store();
        let mut app = App::new(&ctx, forms, store.clone(), Layout::default(), vec![]);
        let bytes = b"0 0 0\n1 0 1\n0 1 1\n1 1 2\n";
        let mut source = store;
        let loaded = pipeline::load(bytes, &mut source, None, |_| {}).unwrap();
        let mut settings = Settings::from_store(&source);
        settings.size = 20;
        let previous = pipeline::render(&loaded, &settings).unwrap();
        app.rendered(&ctx, RenderDone { loaded, result: Ok(previous) });
        let original_tex = app.canvas_tex.as_ref().unwrap().id();
        let original_pixels = app.rendered.as_ref().unwrap().rgba();
        let loaded = app.loaded.take().unwrap();
        app.rendered(&ctx, RenderDone { loaded, result: Err(jobs::CANCELLED.into()) });
        assert_eq!(app.status_msg, "描画を中止しました");
        assert_eq!(app.canvas_tex.as_ref().unwrap().id(), original_tex);
        assert_eq!(app.rendered.as_ref().unwrap().rgba(), original_pixels);
        let rerendered = pipeline::render(app.loaded.as_ref().unwrap(), &settings).unwrap();
        assert_eq!(rerendered.rgba(), original_pixels);
    }

    #[test]
    fn mesh_menu_is_disabled_without_data_or_after_mesh_generation() {
        let mut store = Settings::default_store();
        let bytes = b"0 0 0\n1 0 1\n0 1 1\n1 1 2\n";
        let loaded = pipeline::load(bytes, &mut store, None, |_| {}).unwrap();
        assert!(!menu_item_enabled("menmeshcreate", true, None));
        assert!(menu_item_enabled("menmeshcreate", true, Some(&loaded)));
        let mesh = pipeline::reparse_with_mesh(bytes, &Settings::from_store(&store), true, |_| {}).unwrap();
        assert!(!menu_item_enabled("menmeshcreate", true, Some(&mesh)));
        assert!(!menu_item_enabled("menredraw", false, Some(&loaded)));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_job(&ctx);
        if let Some(req) = self.dlg.batch_req.take() {
            self.start_batch(&ctx, req);
        }
        if self.first {
            self.first = false;
            dialogs::apply_interlocks(&mut self.store);
            // Create startup dialogs only after the root viewport has been
            // presented once; creating an owned viewport during the first
            // frame can leave it blank on Windows.
            if !self.start_files.is_empty() { ctx.request_repaint(); }
        } else if !self.start_files.is_empty() {
            let files = std::mem::take(&mut self.start_files);
            self.load_files(&ctx, files);
        }
        if !self.started_hwnd {
            self.dlg.main_hwnd = plat::find(None);
            self.started_hwnd = self.dlg.main_hwnd.is_some();
        }
        self.track_window(&ctx);
        let locked = self.dlg.any() || self.job.is_some() || self.pending_irregular.is_some();
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
        let mut cells: [String; 3] = Default::default();
        if let Some(tex) = &self.canvas_tex {
            let sz = tex.size_vec2(); // EDh / EDv are applied when the scene is built
            let inner = sbox.shrink(2.0);
            ui.add_enabled_ui(!locked, |ui| {
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                    let out = egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| ui.add(egui::Image::new((tex.id(), sz))));
                    let img = out.inner.rect;
                    let pos = ctx.input(|i| i.pointer.hover_pos()).filter(|p| inner.contains(*p) && img.contains(*p));
                    if !locked {
                        if let (Some(pos), Some(r)) = (pos, &self.rendered) {
                            let d = pos - img.min;
                            cells = jobs::probe_cells(r.probe.at_field(d.x as f64, d.y as f64, &r.field), r.probe.frame().is_some_and(|f| f.cyl.is_some()));
                        }
                    }
                });
            });
        }
        self.probe_cells = cells;
        ui.add_enabled_ui(!locked, |ui| self.control_bar(ui, cbar, &mut acts, locked));
        self.status_bar(ui, sbar);
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
        self.show_irregular_modal(&ctx);
        // Let the progress viewport be created and rendered for a full frame
        // before hiding its native owner. Hiding the owner in start_batch can
        // prevent Windows from ever showing the newly requested child window.
        if self.batch_hide_pending {
            if self.job.is_none() {
                self.batch_hide_pending = false;
            } else if let Some(progress_hwnd) = self.dlg.mesh.then(|| plat::find(Some(dialogs::T_MESH))).flatten() {
                self.batch_progress_frames = self.batch_progress_frames.saturating_add(1);
                if self.batch_progress_frames >= 2 {
                    self.batch_hide_pending = false;
                    // Windows hides owned windows together with their owner.
                    // Detach the progress window before hiding the root so the
                    // user retains access to the cancellation control.
                    plat::detach_owner(progress_hwnd);
                    self.batch_root_hidden = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
                ctx.request_repaint();
            }
        }
        if cfg_was_open && self.dlg.config.is_none() && std::mem::take(&mut self.dlg.config_ok) && self.bytes.is_some() {
            self.redraw(&ctx);
        }
        if self.dlg.any() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if locked && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.store.checked("CBautosave") {
            let p = self.ini_path.clone();
            self.save_ini(&p);
        }
    }
}
