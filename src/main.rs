mod app;
mod core;
mod ui;

use app::BackupApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([1120.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "ADB Smart Backup & Cleanup",
        native_options,
        Box::new(|cc| Ok(Box::new(BackupApp::new(cc)))),
    )
}
