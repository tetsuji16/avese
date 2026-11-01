//! avese: native Rust/egui GUI reimplementation of the Delphi application AV似非.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod app;
mod batch;
mod cli;
mod clipboard;
mod dialogs;
mod engine;
mod fonts;
mod forms;
mod icons;
mod jobs;
mod labels;
mod ini;
mod platform_win32;
mod save_dialog;
mod state;
mod vcl;
mod view;

#[cfg(test)]
fn original_fixture_dir() -> std::path::PathBuf {
    std::env::var_os("AVESE_ORIGINAL_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| "original avese".into())
}

fn main() -> eframe::Result {
    if let Some(code) = cli::run(std::env::args().skip(1).collect()) {
        std::process::exit(code);
    }
    let forms = forms::Forms::load();
    let mut store = state::Store::from_forms(&[&forms.form1, &forms.config, &forms.avi, &forms.input, &forms.about, &forms.mesh]);
    let mut layout = ini::Layout::default();
    let ini_path = app::exe_dir().join("avese.ini");
    if ini_path.exists() {
        let _ = ini::load_file(&ini_path, &mut store, &mut layout);
    }
    let files: Vec<std::path::PathBuf> = std::env::args_os().skip(1).map(std::path::PathBuf::from).filter(|p| !p.to_string_lossy().starts_with("--")).collect();

    let mut vp = egui::ViewportBuilder::default()
        .with_title("avese - nodata")
        .with_inner_size([(layout.form_width - 16).max(200) as f32, (layout.form_height - 39).max(150) as f32])
        .with_min_inner_size([200.0, 150.0])
        .with_position([layout.form_left as f32, layout.form_top as f32])
        .with_drag_and_drop(true);
    vp = vp.with_icon(load_icon());
    let opts = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native(
        "avese",
        opts,
        Box::new(move |cc| {
            cc.egui_ctx.tessellation_options_mut(|o| o.feathering = false);
            let mut vis = egui::Visuals::light();
            vis.selection.bg_fill = egui::Color32::from_rgb(0, 120, 212);
            vis.selection.stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
            cc.egui_ctx.set_visuals(vis);
            fonts::install(&cc.egui_ctx);
            Ok(Box::new(app::App::new(&cc.egui_ctx, forms, store, layout, files)))
        }),
    )
}

fn load_icon() -> egui::IconData {
    let img = image::load_from_memory(include_bytes!("../assets/generated/avese-logo.png"))
        .expect("embedded application icon is valid")
        .resize_exact(64, 64, image::imageops::FilterType::Lanczos3).to_rgba8();
    egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }

}
