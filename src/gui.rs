//! The desktop and web GUI.
//!
//! The toolkit-free part holds the state and everything the GUI says and
//! computes: [`state`] for the app state and its actions, [`worker`] for running
//! the solver, [`board`] for board geometry, [`charts`] for chart data,
//! [`history`] for saved runs and CSV export, [`config`] for run settings and
//! [`palette`] for colors. [`slint_ui`] draws it all with Slint.

use crate::Solver;

mod board;
mod charts;
mod config;
mod history;
mod palette;
pub mod slint_ui;
mod state;
mod worker;

#[cfg(not(target_arch = "wasm32"))]
pub use slint_ui::run;
#[cfg(target_arch = "wasm32")]
pub use slint_ui::run_web;

/// What the GUI calls one iteration of a solver: a GA epoch or a min-conflicts pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Step {
    /// Capitalized, for labels and the start of a sentence.
    title: &'static str,
    one: &'static str,
    many: &'static str,
}

impl Step {
    const fn of(solver: Solver) -> Self {
        match solver {
            Solver::Ga => Self {
                title: "Epoch",
                one: "epoch",
                many: "epochs",
            },
            Solver::MinConflicts => Self {
                title: "Pass",
                one: "pass",
                many: "passes",
            },
        }
    }
}

/// The short solver name shown in run history and comparisons.
const fn solver_name(solver: Solver) -> &'static str {
    match solver {
        Solver::Ga => "GA",
        Solver::MinConflicts => "Min-conflicts",
    }
}

/// The one name the GUI uses for a conflict count.
fn attacking_pairs(count: u32) -> String {
    if count == 1 {
        "1 attacking pair".to_owned()
    } else {
        format!("{count} attacking pairs")
    }
}

fn format_ms(ms: u128) -> String {
    if ms >= 1_000 {
        let centiseconds = ms / 10 + u128::from(ms % 10 >= 5);
        format!("{}.{:02}s", centiseconds / 100, centiseconds % 100)
    } else {
        format!("{ms} ms")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_time_display_rounds_without_narrowing_or_overflow() {
        assert_eq!(format_ms(999), "999 ms");
        assert_eq!(format_ms(1_000), "1.00s");
        assert_eq!(format_ms(1_004), "1.00s");
        assert_eq!(format_ms(1_005), "1.01s");
        assert_eq!(format_ms(9_995), "10.00s");
        assert_eq!(
            format_ms(u128::MAX),
            "340282366920938463463374607431768211.46s"
        );
    }

    #[test]
    fn steps_are_named_for_their_solver() {
        assert_eq!(Step::of(Solver::Ga).title, "Epoch");
        assert_eq!(Step::of(Solver::MinConflicts).many, "passes");
        assert_eq!(solver_name(Solver::MinConflicts), "Min-conflicts");
    }

    #[test]
    fn counts_use_one_term_with_correct_plurals() {
        assert_eq!(attacking_pairs(0), "0 attacking pairs");
        assert_eq!(attacking_pairs(1), "1 attacking pair");
        assert_eq!(attacking_pairs(2), "2 attacking pairs");
    }
}
