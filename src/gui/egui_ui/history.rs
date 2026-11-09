//! The right panel: saved runs, with their actions and comparison marks.

use eframe::egui;

use super::now;
use crate::gui::{
    history::{EMPTY_HISTORY_TEXT, HISTORY_CAPTION, settings_lines},
    state::AppState,
};

impl AppState {
    pub(super) fn draw_run_history(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Run history");
        ui.weak(HISTORY_CAPTION);
        if self.archive.is_empty() {
            ui.label(EMPTY_HISTORY_TEXT);
        }
        if self.archive.viewed.is_some() && ui.button("View current run").clicked() {
            self.view_current_run();
        }
        // Only cheap Arc clones: snapshots and boards are never copied during layout.
        let runs: Vec<_> = self.archive.newest_first().cloned().collect();
        for run in runs {
            let result = &run.result;
            ui.push_id(run.id, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(self.archive.viewed == Some(run.id), run.title())
                        .clicked()
                    {
                        self.view_saved_run(run.id);
                    }
                    ui.label(result.stop_reason.to_string());
                    for (slot, name) in [(0, "A"), (1, "B")] {
                        if ui
                            .selectable_label(self.archive.compare[slot] == Some(run.id), name)
                            .on_hover_text(format!("Select comparison {name}"))
                            .clicked()
                        {
                            self.toggle_comparison(slot, run.id);
                        }
                    }
                });
                ui.label(run.summary());
                ui.collapsing("Settings", |ui| {
                    for line in settings_lines(&result.config) {
                        ui.label(line);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(!self.is_running(), egui::Button::new("Rerun"))
                        .clicked()
                    {
                        self.rerun_saved(run.id);
                    }
                    if ui
                        .button("Copy CLI command")
                        .on_hover_text("Copy a command to run from this project's directory")
                        .clicked()
                        && let Some(command) = self.cli_command(run.id)
                    {
                        ctx.copy_text(command);
                        self.command_copied(run.id, now(ctx));
                        ctx.request_repaint();
                    }
                    if ui
                        .button("Export metrics")
                        .on_hover_text("Save this run's retained metrics as CSV")
                        .clicked()
                    {
                        self.export_run(run.id, now(ctx));
                        ctx.request_repaint();
                    }
                });
                ui.separator();
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        Solver,
        gui::{config::GuiConfig, egui_ui::charts::draw_comparison, worker::tests::finished_run},
    };

    #[test]
    fn history_and_comparison_layout_run_without_a_window() {
        let result = finished_run(GuiConfig {
            board_size: 3,
            population_size: 1,
            ..Default::default()
        });
        let baseline = finished_run(GuiConfig {
            solver: Solver::MinConflicts,
            board_size: 8,
            ..Default::default()
        });
        let mut state = AppState::default();
        let a = state.archive.record(Arc::clone(&result));
        let b = state.archive.record(baseline);
        state.archive.compare = [Some(a), Some(b)];
        state.archive.viewed = Some(a);
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            state.draw_run_history(ui, &ctx);
            draw_comparison(ui, &state.archive, state.log_conflicts);
        });
        assert_ne!(output.shapes.len(), 0);
        output.drop_without_applying_deltas();
    }
}
