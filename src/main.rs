use std::path::PathBuf;

use miru::{config, App};

fn main() -> eframe::Result<()> {
    let initial: Option<PathBuf> = std::env::args().nth(1).map(PathBuf::from);
    let persisted = config::load();
    let (w, h) = (
        persisted.window_width.unwrap_or(1280.0),
        persisted.window_height.unwrap_or(800.0),
    );

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("miru")
        .with_decorations(false)
        .with_inner_size(egui::vec2(w, h))
        .with_min_inner_size(egui::vec2(480.0, 320.0));
    if let Some(icon) = miru::cjk::app_icon(64) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "miru",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc, initial)))),
    )
}
