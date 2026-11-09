//! The Slint version of the GUI, which is replacing `n_queens_gui`.

// Release builds on Windows run without a console window. Debug builds keep it for log output.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), slint::PlatformError> {
    n_queens_problem::gui::slint_ui::run()
}

/// `website/gui-slint/index.html` provides the canvas and the loading message.
#[cfg(target_arch = "wasm32")]
fn main() {
    n_queens_problem::gui::slint_ui::run_web("n_queens_status");
}
