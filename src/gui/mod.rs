//! egui frontend (the `gui` feature): a port of the Python Tk window.

mod app;
mod dialogs;
pub mod state;

use crate::layout::LayoutStore;
use eframe::egui;

/// Open the window. Returns the exit code.
pub fn run() -> i32 {
    let state = state::GuiState::new(LayoutStore::open_default());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(state.title())
            .with_inner_size([950.0, 700.0])
            .with_min_inner_size([850.0, 600.0]),
        ..Default::default()
    };
    match eframe::run_native(
        "PlaylistGenRs",
        options,
        Box::new(|_cc| Ok(Box::new(app::App::new(state)))),
    ) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Error: could not start the GUI: {e}");
            eprintln!(
                "Use --text for the interactive menu, or pass media directories (see --help)."
            );
            1
        }
    }
}
