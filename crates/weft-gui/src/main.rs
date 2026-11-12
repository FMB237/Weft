//! Desktop GUI entry point for Weft.
//!
//! Launches the eframe/egui desktop application.

use weft_gui::app::EditorApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Weft"),
        ..Default::default()
    };

    eframe::run_native(
        "Weft",
        options,
        Box::new(|cc| Ok(Box::new(EditorApp::new(cc)))),
    )
}
