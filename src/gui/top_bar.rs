//! The top bar: the one place for run actions, live progress, and the outcome.

use std::{fmt::Write as _, time::Duration};

use eframe::egui::{self, Align, Layout, RichText, ThemePreference, Vec2};

use super::{
    NQueensApp, Step, attacking_pairs,
    charts::plot_value,
    palette::{Palette, Tone},
    worker::{Progress, RunResult},
};
use crate::ga;

const RUN_BUTTON_SIZE: Vec2 = Vec2::new(120.0, 28.0);

impl NQueensApp {
    /// Top-bar status, toned by outcome.
    fn status(&self) -> (String, Option<Tone>) {
        if let Some(run) = self.archive.viewed.and_then(|id| self.archive.get(id)) {
            let (outcome, tone) = run_outcome(&run.result);
            return (format!("Run #{} · {outcome}", run.id), Some(tone));
        }
        if let Some(running) = &self.running {
            if self.cancel_requested {
                let step = Step::of(running.solver);
                return (format!("Cancelling after current {}", step.one), None);
            }
            let text = self.latest_snapshot.as_ref().map_or_else(
                || "Starting solver".to_owned(),
                |snapshot| {
                    format!(
                        "Best board so far: {}",
                        attacking_pairs(snapshot.best_conflicts_sum)
                    )
                },
            );
            return (text, None);
        }
        if let Some(error) = &self.error {
            return (format!("Error: {error}"), Some(Tone::Error));
        }
        self.display_result().map_or_else(
            || ("Ready".to_owned(), None),
            |result| {
                let (outcome, tone) = run_outcome(result);
                (outcome, Some(tone))
            },
        )
    }

    /// The one place for run actions, live progress, and the outcome.
    pub(super) fn draw_top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let palette = Palette::of(ctx);
        ui.horizontal(|ui| {
            if let Some((max_epochs, solver)) = self
                .running
                .as_ref()
                .map(|running| (running.max_epochs, running.solver))
            {
                let step = Step::of(solver);
                if ui
                    .add_enabled(
                        !self.cancel_requested,
                        egui::Button::new(if self.cancel_requested {
                            "Cancelling…"
                        } else {
                            "Cancel run"
                        })
                        .min_size(RUN_BUTTON_SIZE),
                    )
                    .on_hover_text(format!("Stop after the current {} (Esc)", step.one))
                    .clicked()
                {
                    self.cancel_run();
                }
                let (fraction, text) = run_progress(
                    self.latest_snapshot
                        .as_ref()
                        .map(|snapshot| &snapshot.progress),
                    max_epochs,
                    step,
                );
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .text(text)
                        .desired_width(260.0),
                )
                .on_hover_text(format!(
                    "Share of the {} budget used; a solution can stop the run early.",
                    step.one
                ));
            } else if ui
                .add(
                    egui::Button::new(RichText::new("Run solver").strong())
                        .fill(palette.accent)
                        .min_size(RUN_BUTTON_SIZE),
                )
                .on_hover_text("Start a run with the settings on the left (Ctrl/Cmd+Enter)")
                .clicked()
            {
                self.start_run(ctx);
            }
            ui.separator();
            let (text, tone) = self.status();
            let text = RichText::new(text);
            ui.label(match tone {
                Some(tone) => text.color(palette.tone(tone)),
                None => text,
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                draw_theme_choice(ui);
                ui.separator();
                ui.toggle_value(
                    &mut self.show_history,
                    format!("Run history ({})", self.archive.len()),
                )
                .on_hover_text("Show or hide the run history panel");
                self.draw_notice(ui);
            });
        });
    }
}

impl NQueensApp {
    /// Shows the latest notice until it expires.
    fn draw_notice(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|input| input.time);
        if self
            .notice
            .as_ref()
            .is_some_and(|notice| notice.expires_at <= now)
        {
            self.notice = None;
        }
        let Some(notice) = &self.notice else {
            return;
        };
        ui.separator();
        let text = RichText::new(&notice.text);
        ui.label(if notice.is_error {
            text.color(Palette::of(ui.ctx()).error_text)
        } else {
            text.weak()
        });
        ui.ctx()
            .request_repaint_after(Duration::from_secs_f64(notice.expires_at - now));
    }
}

/// Text buttons rather than egui's built-in theme switch, whose icons the
/// default fonts cannot draw. Follows the system until a fixed theme is picked;
/// egui saves the choice with its other options.
fn draw_theme_choice(ui: &mut egui::Ui) {
    let mut preference = ui.options(|options| options.theme_preference);
    let before = preference;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        // Right-to-left layout: added in reverse so they read System, Dark, Light.
        for (choice, label, hover) in [
            (ThemePreference::Light, "Light", "Use the light theme"),
            (ThemePreference::Dark, "Dark", "Use the dark theme"),
            (
                ThemePreference::System,
                "System",
                "Follow the operating system's light or dark mode",
            ),
        ] {
            ui.selectable_value(&mut preference, choice, label)
                .on_hover_text(hover);
        }
    });
    if preference != before {
        ui.ctx().set_theme(preference);
    }
}

fn run_outcome(result: &RunResult) -> (String, Tone) {
    let best = attacking_pairs(result.best_conflicts_sum);
    let step = Step::of(result.config.solver);
    if result.stop_reason == ga::StopReason::Cancelled {
        return (format!("Cancelled · best board has {best}"), Tone::Warning);
    }
    if let Some(solved) = result.solved_step {
        return (format!("Solved at {} {solved}", step.one), Tone::Success);
    }
    if result.stop_reason == ga::StopReason::Unsolvable {
        return ("This board size has no solution".to_owned(), Tone::Warning);
    }
    (
        format!("{} limit reached · best board has {best}", step.title),
        Tone::Warning,
    )
}

/// Share of the budget used, and a label with the rate once time has passed.
fn run_progress(progress: Option<&Progress>, max_steps: u32, step: Step) -> (f32, String) {
    let Some(progress) = progress else {
        return (0.0, "Starting…".to_owned());
    };
    let current = progress.step();
    let fraction = (plot_value(current) / plot_value(max_steps.max(1))).clamp(0.0, 1.0);
    let mut text = format!("{} {current} of {max_steps}", step.title);
    if let Some(ms) = u32::try_from(progress.elapsed_ms())
        .ok()
        .filter(|&ms| ms > 0)
    {
        let rate = f64::from(current) * 1_000.0 / f64::from(ms);
        let many = step.many;
        if rate >= 100.0 {
            let _ = write!(text, " · {rate:.0} {many}/s");
        } else {
            let _ = write!(text, " · {rate:.1} {many}/s");
        }
    }
    (fraction, text)
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "Progress fractions here are exactly representable."
)]
mod tests {
    use super::*;
    use crate::{
        Solver,
        gui::{config::GuiConfig, worker::tests::finished_run},
    };

    #[test]
    fn progress_reports_budget_share_and_clamps() {
        let epochs = Step::of(Solver::Ga);
        assert_eq!(
            run_progress(None, 10, epochs),
            (0.0, "Starting…".to_owned())
        );
        let result = finished_run(GuiConfig {
            board_size: 3,
            population_size: 4,
            max_epochs: 50,
            allow_unsolvable: true,
            ..Default::default()
        });
        let last = result.final_progress.as_ref().unwrap();
        assert_eq!(last.step(), 50);
        let (fraction, text) = run_progress(Some(last), 200, epochs);
        assert_eq!(fraction, 0.25);
        assert!(text.starts_with("Epoch 50 of 200"), "{text}");
        assert_eq!(text.contains("epochs/s"), last.elapsed_ms() > 0);
        assert_eq!(run_progress(Some(last), 10, epochs).0, 1.0);
        assert_eq!(run_progress(Some(last), 0, epochs).0, 1.0);

        let passes = finished_run(GuiConfig {
            solver: Solver::MinConflicts,
            board_size: 3,
            max_epochs: 40,
            allow_unsolvable: true,
            ..Default::default()
        });
        let last = passes.final_progress.as_ref().unwrap();
        let (fraction, text) = run_progress(Some(last), 80, Step::of(Solver::MinConflicts));
        assert_eq!(fraction, 0.5);
        assert!(text.starts_with("Pass 40 of 80"), "{text}");
        assert_eq!(
            run_outcome(&passes).0,
            "Pass limit reached · best board has 1 attacking pair"
        );
    }

    #[test]
    fn status_appears_once_and_is_colored_by_outcome() {
        let solved = finished_run(GuiConfig {
            board_size: 1,
            population_size: 1,
            ..Default::default()
        });
        assert_eq!(
            run_outcome(&solved),
            ("Solved at epoch 0".to_owned(), Tone::Success)
        );
        let unsolvable = finished_run(GuiConfig {
            board_size: 3,
            population_size: 4,
            ..Default::default()
        });
        assert_eq!(
            run_outcome(&unsolvable),
            ("This board size has no solution".to_owned(), Tone::Warning)
        );
        let limited = finished_run(GuiConfig {
            board_size: 3,
            population_size: 4,
            max_epochs: 5,
            allow_unsolvable: true,
            ..Default::default()
        });
        assert!(
            run_outcome(&limited)
                .0
                .starts_with("Epoch limit reached · best board has")
        );

        let solved_by_repair = finished_run(GuiConfig {
            solver: Solver::MinConflicts,
            board_size: 8,
            ..Default::default()
        });
        assert!(
            run_outcome(&solved_by_repair)
                .0
                .starts_with("Solved at pass ")
        );

        let mut app = NQueensApp::default();
        assert_eq!(app.status(), ("Ready".to_owned(), None));
        app.error = Some("boom".into());
        assert_eq!(app.status(), ("Error: boom".to_owned(), Some(Tone::Error)));
        let id = app.archive.record(solved);
        app.archive.viewed = Some(id);
        assert_eq!(
            app.status(),
            (
                format!("Run #{id} · Solved at epoch 0"),
                Some(Tone::Success)
            )
        );
    }
}
