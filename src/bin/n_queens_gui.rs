#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    n_queens_problem::gui::run()
}

/// Trunk's `index.html` provides the canvas.
#[cfg(target_arch = "wasm32")]
fn main() {
    n_queens_problem::gui::run_web("n_queens_canvas");
}
