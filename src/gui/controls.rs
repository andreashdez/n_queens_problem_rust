//! The left panel: run settings and the metrics of the run on the board.

use eframe::egui::{self, RichText};
use rand::RngExt;

use super::{NQueensApp, config::GuiPreset, format_ms, palette::Palette, worker::Progress};
use crate::{Solver, ga::SelectionStrategy};

/// Fits the 20 digits of the largest `u64`.
const SEED_FIELD_WIDTH: f32 = 160.0;

impl NQueensApp {
    pub(super) fn draw_controls(&mut self, ui: &mut egui::Ui) {
        let is_running = self.running.is_some();
        ui.heading("Set up a run");
        ui.add_space(8.0);
        ui.add_enabled_ui(!is_running, |ui| {
            let preset = GuiPreset::matching(&self.config);
            ui.label("Preset");
            egui::ComboBox::from_id_salt("run_preset")
                .selected_text(preset.map_or("Custom settings", GuiPreset::label))
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for choice in GuiPreset::ALL {
                        if ui
                            .selectable_label(preset == Some(choice), choice.label())
                            .clicked()
                        {
                            self.config = choice.config(self.config.seed);
                        }
                    }
                });
            ui.add_space(8.0);
            egui::Grid::new("basic_parameters")
                .num_columns(2)
                .spacing([12.0, 10.0])
                .show(ui, |ui| {
                    ui.label("Solver");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.config.solver, Solver::Ga, "Genetic algorithm")
                            .on_hover_text("Evolve a population of boards.");
                        ui.radio_value(&mut self.config.solver, Solver::MinConflicts, "Min-conflicts")
                            .on_hover_text("Repair a single board with improving swaps: a fast baseline for comparison.");
                    });
                    ui.end_row();
                    ui.label("Board size");
                    ui.add(
                        egui::DragValue::new(&mut self.config.board_size)
                            .speed(1.0)
                            .range(1..=u16::MAX),
                    )
                    .on_hover_text("One queen per row and column. Sizes 2 and 3 have no solution.");
                    ui.end_row();
                    ui.label("Seed");
                    ui.horizontal(|ui| {
                        self.draw_seed_field(ui);
                        if ui
                            .button("Random")
                            .on_hover_text("Choose a new seed")
                            .clicked()
                        {
                            self.config.seed = rand::rng().random::<u64>();
                            self.seed_text = self.config.seed.to_string();
                        }
                    });
                    ui.end_row();
                });
        });
        ui.add_space(8.0);
        ui.weak(match self.config.solver {
            Solver::Ga => format!(
                "{} individuals · up to {} epochs",
                self.config.population_size, self.config.max_epochs
            ),
            Solver::MinConflicts => {
                format!("One board · up to {} repair passes", self.config.max_epochs)
            }
        });
        ui.add_space(10.0);
        egui::CollapsingHeader::new("Advanced settings")
            .default_open(false)
            .show(ui, |ui| {
                // Recompute because the top bar can start a worker in this frame.
                ui.add_enabled_ui(self.running.is_none(), |ui| self.draw_advanced_controls(ui));
            });
        ui.separator();
        self.draw_current_metrics(ui);
    }

    /// A text field, since dragging through the `u64` range is impractical.
    fn draw_seed_field(&mut self, ui: &mut egui::Ui) {
        let focus_id = ui.next_auto_id();
        // Follow seeds set elsewhere (presets, reruns, saved settings) unless mid-edit.
        if !ui.memory(|memory| memory.has_focus(focus_id))
            && self.seed_text.parse() != Ok(self.config.seed)
        {
            self.seed_text = self.config.seed.to_string();
        }
        let valid = self.seed_text.parse::<u64>().is_ok();
        let mut field = egui::TextEdit::singleline(&mut self.seed_text)
            .id(focus_id)
            .font(egui::TextStyle::Monospace)
            .desired_width(SEED_FIELD_WIDTH);
        if !valid {
            field = field.text_color(Palette::of(ui.ctx()).error_text);
        }
        let response = ui.add(field).on_hover_text(if valid {
            "Use the same seed and settings to reproduce a run."
        } else {
            "Enter a whole number from 0 to 18446744073709551615. Leaving the field keeps the last valid seed."
        });
        if response.changed() {
            self.seed_text
                .retain(|character| character.is_ascii_digit());
            if let Ok(seed) = self.seed_text.parse() {
                self.config.seed = seed;
            }
        }
    }

    fn draw_advanced_controls(&mut self, ui: &mut egui::Ui) {
        if self.config.solver == Solver::MinConflicts {
            self.draw_min_conflicts_controls(ui);
            return;
        }
        ui.label("Tune search effort and exploration.");
        egui::Grid::new("search_budget")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Population");
                ui.add(
                    egui::DragValue::new(&mut self.config.population_size)
                        .speed(100.0)
                        .range(1..=u32::MAX),
                )
                .on_hover_text(
                    "Number of candidate boards. Larger populations require more time and memory.",
                );
                ui.end_row();
                ui.label("Max epochs");
                ui.add(
                    egui::DragValue::new(&mut self.config.max_epochs)
                        .speed(50.0)
                        .range(1..=u32::MAX),
                )
                .on_hover_text("Stop after this many generations if no solution is found.");
                ui.end_row();
            });
        ui.add_space(8.0);
        ui.label(RichText::new("Exploration").strong());
        ui.add(egui::Slider::new(&mut self.config.mutation_rate, 0.0..=1.0).text("Mutation"))
            .on_hover_text("Chance of swapping two queens in a non-elite board. Automatically increases during stagnation.");
        ui.add(egui::Slider::new(&mut self.config.elite_ratio, 0.0..=1.0).text("Elite fraction"))
            .on_hover_text(
                "Fraction of the strongest boards protected from mutation and kept as survivors.",
            );
        ui.add(egui::Slider::new(&mut self.config.offspring_ratio, 0.0..=1.0).text("Offspring"))
            .on_hover_text("New children per generation, as a fraction of the target population.");
        ui.add(
            egui::Slider::new(&mut self.config.min_diversity_ratio, 0.0..=1.0)
                .text("Min diversity"),
        )
        .on_hover_text(
            "If too few boards are unique, refresh some non-elite boards with random permutations.",
        );
        ui.add_space(8.0);
        ui.label(RichText::new("Parent selection").strong());
        ui.horizontal(|ui| {
            ui.radio_value(
                &mut self.config.selection_strategy,
                SelectionStrategy::Roulette,
                "Roulette",
            )
            .on_hover_text("Sample parents with probability weighted by fitness.");
            ui.radio_value(
                &mut self.config.selection_strategy,
                SelectionStrategy::Tournament,
                "Tournament",
            )
            .on_hover_text("Choose the strongest board among a random sample of candidates.");
        });
        ui.add_enabled_ui(
            self.config.selection_strategy == SelectionStrategy::Tournament,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label("Tournament size");
                    ui.add(
                        egui::DragValue::new(&mut self.config.tournament_size)
                            .speed(1.0)
                            .range(1..=u32::MAX),
                    )
                    .on_hover_text(
                        "More candidates increase selection pressure, which can reduce diversity.",
                    );
                });
            },
        );
        ui.add_space(8.0);
        ui.label(RichText::new("Local search").strong());
        ui.add(egui::Slider::new(&mut self.config.local_search_rate, 0.0..=1.0).text("Fraction"))
            .on_hover_text("Try improving swaps on this fraction of non-elite boards. Zero disables local search.");
        ui.add_enabled_ui(self.config.local_search_rate > 0.0, |ui| {
            ui.horizontal(|ui| {
                ui.label("Swap attempts");
                ui.add(egui::DragValue::new(&mut self.config.local_search_attempts).speed(1.0))
                    .on_hover_text("Number of trial swaps for each selected board; only improvements are kept.");
            });
        });
        ui.separator();
        self.draw_unsolvable_checkbox(ui);
    }

    /// Min-conflicts repairs a single board, so only its pass budget applies.
    fn draw_min_conflicts_controls(&mut self, ui: &mut egui::Ui) {
        ui.label("Min-conflicts has no population or rates to tune.");
        ui.horizontal(|ui| {
            ui.label("Max passes");
            ui.add(
                egui::DragValue::new(&mut self.config.max_epochs)
                    .speed(50.0)
                    .range(1..=u32::MAX),
            )
            .on_hover_text(
                "Stop after this many repair passes, across restarts, if no solution is found.",
            );
        });
        ui.separator();
        self.draw_unsolvable_checkbox(ui);
    }

    fn draw_unsolvable_checkbox(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(
            &mut self.config.allow_unsolvable,
            "Experiment on unsolvable boards",
        )
        .on_hover_text(
            "Let sizes 2 and 3 keep searching for experiments instead of stopping straight away.",
        );
    }

    fn draw_current_metrics(&self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(if self.archive.viewed.is_some() {
                "Viewed run"
            } else {
                "Current run"
            })
            .strong(),
        );

        let Some((progress, best_conflicts_sum)) = self.current_metrics() else {
            ui.label("No metrics yet.");
            return;
        };

        egui::Grid::new("metrics_grid")
            .num_columns(2)
            .spacing([12.0, 5.0])
            .show(ui, |ui| match progress {
                Progress::Ga(metrics) => {
                    metric_row(ui, "Epoch", metrics.epoch().to_string());
                    metric_row(ui, "Best attacking pairs", best_conflicts_sum.to_string());
                    metric_row(
                        ui,
                        "Avg attacking pairs",
                        format!("{:.2}", metrics.average_conflicts_sum()),
                    );
                    metric_row(
                        ui,
                        "Diversity",
                        format!("{:.1}%", metrics.diversity_ratio() * 100.0),
                    );
                    metric_row(ui, "Mutation", format_ratio(metrics.mutation_rate()));
                    metric_row(ui, "Elite", format_ratio(metrics.elite_ratio()));
                    metric_row(ui, "Population", metrics.population_size().to_string());
                    metric_row(ui, "Elapsed", format_ms(metrics.elapsed_ms()));
                }
                Progress::MinConflicts(metrics) => {
                    metric_row(ui, "Pass", metrics.pass().to_string());
                    metric_row(ui, "Best attacking pairs", best_conflicts_sum.to_string());
                    metric_row(
                        ui,
                        "Current board",
                        metrics.current_conflicts_sum().to_string(),
                    );
                    metric_row(ui, "Swaps", metrics.swaps().to_string());
                    metric_row(ui, "Restarts", metrics.restarts().to_string());
                    metric_row(ui, "Elapsed", format_ms(metrics.elapsed_ms()));
                }
            });

        if let Some(result) = self.display_result() {
            ui.add_space(6.0);
            if let Some(population_size) = result.population_size {
                ui.label(format!("Final population: {population_size}"));
            }
            ui.label(format!(
                "Total elapsed: {}",
                format_ms(result.total_elapsed_ms)
            ));
        }
    }
}

fn metric_row(ui: &mut egui::Ui, label: &str, value: String) {
    ui.label(label);
    ui.label(RichText::new(value).monospace());
    ui.end_row();
}

fn format_ratio(value: f32) -> String {
    format!("{:.1}%", value * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(app: &mut NQueensApp) {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| app.draw_controls(ui))
            .drop_without_applying_deltas();
    }

    #[test]
    fn seed_field_follows_seeds_set_elsewhere() {
        let mut app = NQueensApp::default();
        app.config.seed = u64::MAX;
        draw(&mut app);
        assert_eq!(app.seed_text, u64::MAX.to_string());
        // An abandoned invalid edit falls back to the last valid seed.
        app.seed_text = "18446744073709551616".into();
        draw(&mut app);
        assert_eq!(app.seed_text, u64::MAX.to_string());
    }
}
