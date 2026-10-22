#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    n_queens_problem::gui::run()
}

/// `web/index.html` provides the canvas and the loading message.
#[cfg(target_arch = "wasm32")]
fn main() {
    n_queens_problem::gui::run_web("n_queens_canvas", "n_queens_status");
}
