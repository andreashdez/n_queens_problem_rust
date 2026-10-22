pub mod ga;
#[cfg(feature = "gui")]
pub mod gui;
pub mod min_conflicts;
mod solver;
pub mod tui;

pub use solver::Solver;
