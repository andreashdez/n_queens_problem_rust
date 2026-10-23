// Release builds on Windows run without a console window. Debug builds keep it for log output.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    n_queens_problem::gui::run()
}

/// `website/gui/index.html` provides the canvas and the loading message.
#[cfg(target_arch = "wasm32")]
fn main() {
    n_queens_problem::gui::run_web("n_queens_canvas", "n_queens_status");
}
