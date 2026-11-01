//! Work that must not block the window: loading, drawing and the batch jobs run on a worker thread.
use crate::engine::data::Label;
use crate::engine::draw::axes::fmt_num;
use crate::engine::draw::ProbeHit;
use crate::engine::export::{self, ImageFormat};
use crate::engine::field::Field;
use crate::engine::pipeline::{self, Loaded, Rendered};
use crate::engine::settings::Settings;
use crate::state::Store;
use crate::{batch, labels};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Instant;

pub const CANCELLED: &str = "中止しました";

/// State shared with the worker: cancel request and progress of the current task / of the whole job.
pub struct Sh {
    cancel: AtomicBool,
    task: AtomicU32,
    all: AtomicU32,
}

impl Sh {
    fn new() -> Sh {
        Sh { cancel: AtomicBool::new(false), task: AtomicU32::new(0), all: AtomicU32::new(0) }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    fn set_task(&self, v: f32) {
        self.task.store(v.to_bits(), Ordering::Relaxed);
    }
    fn set_all(&self, v: f32) {
        self.all.store(v.to_bits(), Ordering::Relaxed);
    }
    pub fn task(&self) -> f32 {
        f32::from_bits(self.task.load(Ordering::Relaxed))
    }
    pub fn all(&self) -> f32 {
        f32::from_bits(self.all.load(Ordering::Relaxed))
    }
}

pub struct Handle<T> {
    rx: Receiver<Result<T, String>>,
    pub sh: Arc<Sh>,
    pub started: Instant,
}

impl<T> Handle<T> {
    pub fn poll(&self) -> Option<Result<T, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("処理が中断されました".into())),
        }
    }
}

/// Run `f` on a thread; a panic becomes an error and the window is woken when the result is ready.
pub fn spawn<T: Send + 'static>(ctx: &egui::Context, f: impl FnOnce(&Sh) -> Result<T, String> + Send + 'static) -> Handle<T> {
    let (tx, rx) = channel();
    let sh = Arc::new(Sh::new());
    let (sh2, ctx2) = (sh.clone(), ctx.clone());
    std::thread::spawn(move || {
        let r = catch_unwind(AssertUnwindSafe(|| f(&sh2))).unwrap_or_else(|_| Err("処理中にエラーが発生しました".into()));
        let _ = tx.send(r);
        ctx2.request_repaint();
    });
    // The job may outlive the input event that started it. Wake the UI now so
    // it begins polling and can show progress before the worker finishes.
    ctx.request_repaint();
    Handle { rx, sh, started: Instant::now() }
}

pub struct Built {
    pub bytes: Arc<Vec<u8>>,
    pub store: Store,
    pub loaded: Loaded,
}

/// Read a data file and build its field; embedded settings land in the returned store.
pub fn load_job(ctx: &egui::Context, path: PathBuf, mut store: Store) -> Handle<Built> {
    spawn(ctx, move |sh| {
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        let mut loaded = pipeline::load(&bytes, &mut store, Some(&dir), |p| sh.set_task(p)).map_err(|e| format!("{}: {e}", path.display()))?;
        loaded.path = Some(path);
        Ok(Built { bytes: Arc::new(bytes), store, loaded })
    })
}

/// Parse the held bytes again because a setting that changes how they are read has changed.
pub fn reparse_job(ctx: &egui::Context, bytes: Arc<Vec<u8>>, path: Option<PathBuf>, s: Settings) -> Handle<Loaded> {
    reparse_job_with_mesh(ctx, bytes, path, s, false)
}

pub fn reparse_job_with_mesh(ctx: &egui::Context, bytes: Arc<Vec<u8>>, path: Option<PathBuf>, s: Settings, mesh_auto: bool) -> Handle<Loaded> {
    spawn(ctx, move |sh| {
        let mut l = pipeline::reparse_with_mesh(&bytes, &s, mesh_auto, |p| sh.set_task(p))?;
        l.path = path;
        Ok(l)
    })
}

pub struct MeshDone {
    pub loaded: Loaded,
    pub result: Result<(), String>,
}

/// Build a triangulated field from the current data on a worker thread.
pub fn mesh_job(ctx: &egui::Context, loaded: Loaded, bytes: Arc<Vec<u8>>, path: Option<PathBuf>, s: Settings) -> Handle<MeshDone> {
    spawn(ctx, move |sh| {
        let mut cancelled = false;
        let generated = pipeline::reparse_with_mesh(&bytes, &s, true, |p| {
            sh.set_task(p);
            cancelled |= sh.cancelled();
        });
        let generated = match generated {
            Ok(_) if cancelled || sh.cancelled() => Err(CANCELLED.to_string()),
            Ok(mut generated) => {
                generated.path = path;
                Ok(generated)
            }
            Err(e) => Err(e),
        };
        let (loaded, result) = match generated {
            Ok(generated) => (generated, Ok(())),
            Err(e) => (loaded, Err(e)),
        };
        Ok(MeshDone { loaded, result })
    })
}

pub struct RenderDone {
    pub loaded: Loaded,
    pub result: Result<Rendered, String>,
}

/// Draw with the labels of the data file plus the ones added in the dialog.
pub fn render_job(ctx: &egui::Context, mut loaded: Loaded, s: Settings, gui_labels: Vec<String>) -> Handle<RenderDone> {
    spawn(ctx, move |sh| {
        let file: Vec<Label> = std::mem::take(&mut loaded.data.labels);
        loaded.data.labels = labels::merged(&file, &gui_labels);
        let result = catch_unwind(AssertUnwindSafe(|| pipeline::render_with_progress(&loaded, &s, |p| {
            sh.set_task(p);
            !sh.cancelled()
        }))).unwrap_or_else(|_| Err("描画中にエラーが発生しました".into()))
            .map_err(|e| if e == pipeline::RENDER_CANCELLED { CANCELLED.into() } else { e });
        loaded.data.labels = file;
        Ok(RenderDone { loaded, result })
    })
}

pub enum BatchKind {
    Bmp(Vec<PathBuf>),
    Avi { path: PathBuf, fps: f64, size: Option<(i32, i32)> },
}

/// Load and draw one file with its own copy of the settings, so that embedded settings do not leak.
fn render_file(file: &str, base: &Store, sh: &Sh) -> Result<Rendered, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
    let mut store = base.clone();
    let dir = Path::new(file).parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let loaded = pipeline::load(&bytes, &mut store, Some(dir), |p| sh.set_task(p * 0.9)).map_err(|e| format!("{file}: {e}"))?;
    let r = pipeline::render_with_progress(&loaded, &Settings::from_store(&store), |p| {
        sh.set_task(0.9 + p * 0.1);
        !sh.cancelled()
    }).map_err(|e| if e == pipeline::RENDER_CANCELLED { CANCELLED.into() } else { format!("{file}: {e}") })?;
    sh.set_task(1.0);
    Ok(r)
}

/// Batch creation of BMP files or one AVI; Ok carries the message for the user.
pub fn batch_job(ctx: &egui::Context, files: Vec<String>, base: Store, kind: BatchKind) -> Handle<String> {
    spawn(ctx, move |sh| match kind {
        BatchKind::Bmp(targets) => {
            let (mut done, mut errs) = (0, vec![]);
            for (i, (f, t)) in files.iter().zip(&targets).enumerate() {
                if sh.cancelled() {
                    return Err(CANCELLED.into());
                }
                sh.set_all(i as f32 / files.len() as f32);
                match render_file(f, &base, sh).and_then(|r| export::save_image(t, &r, ImageFormat::Bmp)) {
                    Ok(()) => done += 1,
                    Err(e) if e == CANCELLED => return Err(e),
                    Err(e) => errs.push(e),
                }
            }
            if errs.is_empty() { Ok(format!("{done} 個の BMP ファイルを作成しました。")) } else { Err(format!("{done} 個作成、{} 個失敗しました。\n{}", errs.len(), errs.join("\n"))) }
        }
        BatchKind::Avi { path, fps, size } => {
            let first = match files.first() {
                Some(f) => render_file(f, &base, sh)?,
                None => return Err("ファイルがありません".into()),
            };
            let [fw, fh] = first.size();
            let (w, h) = batch::avi_size(size, (fw as u32, fh as u32));
            let mut first = Some(first);
            export::avi::write_avi_cancellable(
                &path,
                files.len(),
                w,
                h,
                fps,
                |i| {
                    if sh.cancelled() {
                        return Err(CANCELLED.into());
                    }
                    let r = match first.take().filter(|_| i == 0) {
                        Some(r) => r,
                        None => render_file(&files[i], &base, sh)?,
                    };
                    let [fw, fh] = r.size();
                    Ok((r.rgba(), fw as u32, fh as u32))
                },
                |p| {
                    sh.set_all(p);
                    if sh.cancelled() { Err(CANCELLED.to_string()) } else { Ok(()) }
                },
            ).map_err(|e| if sh.cancelled() { CANCELLED.to_string() } else { e })?;
            Ok(format!("{} フレーム ({w} x {h}) の AVI ファイルを作成しました。", files.len()))
        }
    })
}

/// Fill the read-only range boxes of the settings dialog from the loaded data; boxes whose
/// "指定" checkbox is on keep what the user typed.
pub fn readback(store: &mut Store, f: &Field) {
    if let Some((x0, x1, y0, y1)) = f.extent() {
        for (n, v) in [("EDenXmin", x0), ("EDenXmax", x1), ("EDenYmin", y0), ("EDenYmax", y1)] {
            store.set_text(n, &fmt_num(v));
        }
    }
    let zr = f.data_z_range();
    let auto = |store: &mut Store, cb: &str, ed: &str, v: Option<f64>| {
        if let (false, Some(v)) = (store.checked(cb), v) {
            store.set_text(ed, &fmt_num(v));
        }
    };
    auto(store, "CBenZmin", "EDenZmin", zr.map(|r| r.0));
    auto(store, "CBenZmax", "EDenZmax", zr.map(|r| r.1));
    auto(store, "CBentokomin", "EDtokomin", zr.map(|r| r.0));
    auto(store, "CBentokomax", "EDtokomax", zr.map(|r| r.1));
    auto(store, "CBenVMax", "EDenVMax", f.max_vector());
}

/// Value rounded to 5 significant digits, as shown in the status bar.
fn short(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return fmt_num(v);
    }
    let d = 4 - v.abs().log10().floor() as i32;
    let k = 10f64.powi(d.clamp(-300, 300));
    fmt_num((v * k).round() / k)
}

/// The three status bar panels for a mouse position: X, Y, Z (θ and r for cylindrical data).
pub fn probe_cells(hit: Option<ProbeHit>, cyl: bool) -> [String; 3] {
    let Some(h) = hit else { return Default::default() };
    let (a, b) = if cyl { ("θ", "r") } else { ("X", "Y") };
    [format!("{a} = {}", short(h.x)), format!("{b} = {}", short(h.y)), h.z.map(|z| format!("Z = {}", short(z))).unwrap_or_default()]
}

/// Warning list for a message box: the first ten, then a count.
pub fn warnings_text(w: &[String]) -> String {
    let mut s = w.iter().take(10).cloned().collect::<Vec<_>>().join("\n");
    if w.len() > 10 {
        s += &format!("\n... 他 {} 件", w.len() - 10);
    }
    s
}

/// True when drawing needs more than the SEmaxbmp limit.
pub fn over_limit(bytes: u64, max_mb: u32) -> bool {
    bytes > max_mb as u64 * 1024 * 1024
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::settings::Settings;

    fn grid() -> Loaded {
        let mut st = Settings::default_store();
        let mut t = String::new();
        for y in 0..3 {
            for x in 0..4 {
                t += &format!("{x} {y} {}\n", x as f64 * 0.5 + y as f64);
            }
        }
        pipeline::load(t.as_bytes(), &mut st, None, |_| {}).unwrap()
    }

    #[test]
    fn readback_fills_unspecified_ranges_only() {
        let l = grid();
        let mut st = Settings::default_store();
        st.set_checked("CBenZmax", true);
        st.set_text("EDenZmax", "99");
        readback(&mut st, &l.field);
        assert_eq!(st.text("EDenXmin"), "0");
        assert_eq!(st.text("EDenXmax"), "3");
        assert_eq!(st.text("EDenYmax"), "2");
        assert_eq!(st.text("EDenZmin"), "0");
        assert_eq!(st.text("EDenZmax"), "99");
        assert_eq!(st.text("EDtokomax"), "3.5");
    }

    #[test]
    fn cancelled_mesh_job_returns_the_original_loaded_field() {
        let original = grid();
        let mut source = String::new();
        for y in 0..120 {
            for x in 0..120 { source += &format!("{x} {y} {}\n", x + y); }
        }
        let bytes = Arc::new(source.into_bytes());
        let ctx = egui::Context::default();
        let handle = mesh_job(&ctx, original, bytes, None, Settings::from_store(&Settings::default_store()));
        handle.sh.cancel();
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(Ok(done)) = handle.poll() {
                assert_eq!(done.result.unwrap_err(), CANCELLED);
                assert!(!done.loaded.data.mesh_auto);
                assert!(matches!(&*done.loaded.field, Field::Grid(_)));
                assert_eq!(done.loaded.field.extent(), Some((0.0, 3.0, 0.0, 2.0)));
                break;
            }
            assert!(Instant::now() < deadline, "mesh worker did not finish after cancellation");
            std::thread::yield_now();
        }
    }

    #[test]
    fn status_cells() {
        let hit = ProbeHit { x: 1.23456789, y: 0.0, z: Some(1234.5678) };
        assert_eq!(probe_cells(Some(hit), false), ["X = 1.2346", "Y = 0", "Z = 1234.6"]);
        assert_eq!(probe_cells(Some(ProbeHit { z: None, ..hit }), true)[2], "");
        assert_eq!(probe_cells(Some(hit), true)[0], "θ = 1.2346");
        assert_eq!(probe_cells(None, false), ["", "", ""]);
    }

    #[test]
    fn limit_and_warning_text() {
        assert!(!over_limit(4 * 1024 * 1024, 4));
        assert!(over_limit(4 * 1024 * 1024 + 1, 4));
        let w: Vec<String> = (0..12).map(|i| i.to_string()).collect();
        assert!(warnings_text(&w).ends_with("他 2 件"));
        assert_eq!(warnings_text(&w[..2]), "0\n1");
    }

    #[test]
    fn worker_render_restores_file_labels_and_draws_gui_labels() {
        let ctx = egui::Context::default();
        let mut l = grid();
        l.data.labels = vec![Label { x: 1.0, y: 1.0, text: "file".into() }];
        let mut st = Settings::default_store();
        st.set_checked("CBLabel", true);
        let s = Settings::from_store(&st);
        let h = render_job(&ctx, l, s, vec!["3, 4  gui".into()]);
        let done = loop {
            if let Some(r) = h.poll() {
                break r.unwrap();
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(done.loaded.data.labels.len(), 1);
        let r = done.result.unwrap();
        let texts: Vec<_> = r.scene.ops.iter().filter_map(|o| if let crate::engine::scene::Op::Text { text, .. } = o { Some(text.as_str()) } else { None }).collect();
        assert!(texts.contains(&"gui") && texts.contains(&"file"));
    }

    #[test]
    fn worker_panics_become_errors() {
        let ctx = egui::Context::default();
        let h: Handle<()> = spawn(&ctx, |_| -> Result<(), String> { panic!("boom") });
        let r = loop {
            if let Some(r) = h.poll() {
                break r;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert!(r.is_err());
    }
}
