use std::{collections::VecDeque, fmt::Write as _, fs, io, path::Path, sync::Arc};

use eframe::egui;

use super::{
    BoardView, ChartView, NQueensApp, attacking_pairs,
    charts::{Chart, ChartMarker, ChartScale, ChartSeries, plot_height, plot_value},
    config::GuiConfig,
    format_ms,
    palette::Palette,
    worker::RunResult,
};

const MAX_SAVED_RUNS: usize = 10;

#[derive(Clone)]
pub(super) struct SavedRun {
    pub id: u64,
    pub result: Arc<RunResult>,
}

#[derive(Default)]
pub(super) struct RunArchive {
    runs: VecDeque<SavedRun>,
    next_id: u64,
    pub viewed: Option<u64>,
    pub compare: [Option<u64>; 2],
}

impl RunArchive {
    pub fn record(&mut self, result: Arc<RunResult>) -> u64 {
        self.next_id += 1;
        self.runs.push_back(SavedRun {
            id: self.next_id,
            result,
        });
        if self.runs.len() > MAX_SAVED_RUNS {
            let removed = self.runs.pop_front().unwrap().id;
            if self.viewed == Some(removed) {
                self.viewed = None;
            }
            for slot in &mut self.compare {
                if *slot == Some(removed) {
                    *slot = None;
                }
            }
        }
        self.next_id
    }

    pub fn len(&self) -> usize {
        self.runs.len()
    }

    pub fn get(&self, id: u64) -> Option<&SavedRun> {
        self.runs.iter().find(|run| run.id == id)
    }

    fn toggle_comparison(&mut self, slot: usize, id: u64) {
        if self.get(id).is_none() {
            return;
        }
        self.compare[slot] = if self.compare[slot] == Some(id) {
            None
        } else {
            Some(id)
        };
        if self.compare[1 - slot] == Some(id) {
            self.compare[1 - slot] = None;
        }
    }
}

fn comparison_context(a: &GuiConfig, b: &GuiConfig) -> String {
    let seed = if a.seed == b.seed {
        format!("Same seed ({})", a.seed)
    } else {
        format!("Different seeds ({} and {})", a.seed, b.seed)
    };
    if a.board_size == b.board_size {
        format!(
            "{seed} · Both {}×{}. Compare multiple seeds before drawing conclusions about settings.",
            a.board_size, a.board_size
        )
    } else {
        format!(
            "{seed} · Different board sizes ({} and {}): attacking-pair counts are not directly comparable.",
            a.board_size, b.board_size
        )
    }
}

impl SavedRun {
    fn csv(&self) -> String {
        let config = &self.result.config;
        let history = &self.result.history;
        let metrics = &self.result.metrics;
        let common = [
            self.id.to_string(),
            config.seed.to_string(),
            config.board_size.to_string(),
            config.population_size.to_string(),
            config.max_epochs.to_string(),
            config.mutation_rate.to_string(),
            config.elite_ratio.to_string(),
            config.offspring_ratio.to_string(),
            config.min_diversity_ratio.to_string(),
            config.selection_strategy.to_string(),
            config.tournament_size.to_string(),
            config.local_search_rate.to_string(),
            config.local_search_attempts.to_string(),
            config.allow_unsolvable.to_string(),
            metrics.stop_reason().to_string(),
            metrics
                .solved_epoch()
                .map(|epoch| epoch.to_string())
                .unwrap_or_default(),
            metrics.total_elapsed_ms().to_string(),
            history.points.len().to_string(),
            history
                .points
                .last()
                .is_some_and(|last| (history.points.len() as u64) < u64::from(last.epoch()) + 1)
                .to_string(),
        ];
        let mut output = "record_type,run_id,seed,board_size,target_population,max_epochs,mutation_rate,elite_ratio,offspring_ratio,min_diversity_ratio,selection_strategy,tournament_size,local_search_rate,local_search_attempts,allow_unsolvable,stop_reason,solved_epoch,run_elapsed_ms,retained_samples,downsampled,epoch,best_conflicts_sum,average_conflicts_sum,population_size,elapsed_ms,unique_chromosomes,diversity_ratio,epoch_mutation_rate,epoch_elite_ratio,offspring_count,local_search_improvements,stagnation_epochs,diversity_replacements,restart_count,last_restart_epoch\n".to_owned();
        for point in &history.points {
            let values = [
                point.epoch().to_string(),
                point.best_conflicts_sum().to_string(),
                point.average_conflicts_sum().to_string(),
                point.population_size().to_string(),
                point.elapsed_ms().to_string(),
                point.unique_chromosomes().to_string(),
                point.diversity_ratio().to_string(),
                point.mutation_rate().to_string(),
                point.elite_ratio().to_string(),
                point.offspring_count().to_string(),
                point.local_search_improvements().to_string(),
                point.stagnation_epochs().to_string(),
                point.diversity_replacements().to_string(),
                point.restart_count().to_string(),
                point
                    .last_restart_epoch()
                    .map(|epoch| epoch.to_string())
                    .unwrap_or_default(),
            ];
            writeln!(output, "sample,{},{}", common.join(","), values.join(","))
                .expect("writing to a String cannot fail");
        }
        // Preserve restart locations even when the corresponding metric sample was decimated.
        for epoch in &history.restart_epochs {
            let mut values = vec![String::new(); 15];
            values[0] = epoch.to_string();
            writeln!(output, "restart,{},{}", common.join(","), values.join(","))
                .expect("writing to a String cannot fail");
        }
        output
    }

    fn export_csv(&self, path: &Path) -> io::Result<()> {
        // The native save dialog has already confirmed replacing an existing file.
        fs::write(path, self.csv())
    }
}

impl NQueensApp {
    /// Asks for a destination with the native save dialog, then writes the CSV.
    fn export_run(&mut self, id: u64, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(format!("Export metrics · Run #{id}"))
            .set_file_name(format!("n-queens-run-{id}.csv"))
            .add_filter("CSV", &["csv"])
            .save_file()
        else {
            return;
        };
        let Some(run) = self.archive.get(id) else {
            return;
        };
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.display().to_string(),
        );
        match run.export_csv(&path) {
            Ok(()) => self.notify(ctx, format!("Exported run #{id} to {name}"), false),
            Err(error) => self.notify(ctx, format!("Could not export run #{id}: {error}"), true),
        }
    }

    fn rerun_saved(&mut self, id: u64, ctx: &egui::Context) -> bool {
        if self.running.is_some() {
            return false;
        }
        let Some(run) = self.archive.get(id) else {
            return false;
        };
        self.config = run.result.config.clone();
        self.start_run(ctx);
        true
    }

    /// Completing an A/B pair brings the comparison chart into view.
    fn toggle_comparison(&mut self, slot: usize, id: u64) {
        self.archive.toggle_comparison(slot, id);
        if self.archive.compare.iter().all(Option::is_some) {
            self.chart_view = ChartView::Comparison;
        }
    }

    pub(super) fn draw_run_history(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Run history");
        ui.weak("Last 10 completed runs · this session only");
        if self.archive.runs.is_empty() {
            ui.label("Completed and cancelled runs appear here.");
        }
        if self.archive.viewed.is_some() && ui.button("View current run").clicked() {
            self.archive.viewed = None;
            self.selected_queen = None;
            self.board_view = BoardView::default();
        }
        // Only cheap Arc clones: snapshots and boards are never copied during layout.
        let runs: Vec<_> = self.archive.runs.iter().rev().cloned().collect();
        for run in runs {
            let result = &run.result;
            ui.push_id(run.id, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(
                            self.archive.viewed == Some(run.id),
                            format!("Run #{}", run.id),
                        )
                        .clicked()
                    {
                        self.archive.viewed = Some(run.id);
                        self.selected_queen = None;
                        self.board_view = BoardView::default();
                        self.chart_view = ChartView::Metrics;
                    }
                    ui.label(result.metrics.stop_reason().to_string());
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
                ui.label(format!(
                    "{}×{} · seed {} · {} · {}",
                    result.config.board_size,
                    result.config.board_size,
                    result.config.seed,
                    attacking_pairs(result.best_conflicts_sum),
                    format_ms(result.metrics.total_elapsed_ms())
                ));
                ui.collapsing("Settings", |ui| {
                    ui.label(format!(
                        "Population {} · epochs {} · {} selection (tournament size {})",
                        result.config.population_size,
                        result.config.max_epochs,
                        result.config.selection_strategy,
                        result.config.tournament_size
                    ));
                    ui.label(format!(
                        "Mutation {} · elite {} · offspring {} · diversity {}",
                        result.config.mutation_rate,
                        result.config.elite_ratio,
                        result.config.offspring_ratio,
                        result.config.min_diversity_ratio
                    ));
                    ui.label(format!(
                        "Local search {} · attempts {} · unsolvable experiments {}",
                        result.config.local_search_rate,
                        result.config.local_search_attempts,
                        result.config.allow_unsolvable
                    ));
                });
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(self.running.is_none(), egui::Button::new("Rerun"))
                        .clicked()
                    {
                        self.rerun_saved(run.id, ctx);
                    }
                    if ui
                        .button("Copy CLI command")
                        .on_hover_text("Copy a command to run from this project's directory")
                        .clicked()
                    {
                        ctx.copy_text(result.config.cli_command());
                        self.notify(ctx, format!("Copied command for run #{}", run.id), false);
                    }
                    if ui
                        .button("Export metrics")
                        .on_hover_text("Save this run's retained metrics as CSV")
                        .clicked()
                    {
                        self.export_run(run.id, ctx);
                    }
                });
                ui.separator();
            });
        }
    }

    pub(super) fn draw_comparison(&self, ui: &mut egui::Ui) {
        let pair = self
            .archive
            .compare
            .map(|id| id.and_then(|id| self.archive.get(id)));
        let [Some(a), Some(b)] = pair else {
            ui.label("Mark one run A and another B in run history to compare their convergence.");
            return;
        };
        ui.label(comparison_context(&a.result.config, &b.result.config));
        ui.weak(
            "Best-board attacking pairs by epoch · sampled points · dashed lines mark restarts",
        );
        let colors = Palette::of(ui.ctx()).compare();
        let series: Vec<_> = [a, b]
            .iter()
            .enumerate()
            .map(|(index, run)| ChartSeries {
                label: format!("{} · Run #{}", if index == 0 { "A" } else { "B" }, run.id),
                color: colors[index],
                values: run
                    .result
                    .history
                    .points
                    .iter()
                    .map(|point| (point.epoch(), plot_value(point.best_conflicts_sum())))
                    .collect(),
            })
            .collect();
        let markers: Vec<_> = [a, b]
            .iter()
            .enumerate()
            .flat_map(|(index, run)| {
                run.result
                    .history
                    .restart_epochs
                    .iter()
                    .map(move |&epoch| ChartMarker {
                        epoch,
                        label: format!("Run #{} restart", run.id),
                        color: colors[index],
                    })
            })
            .collect();
        Chart {
            title: "Convergence",
            height: plot_height(ui, 1),
            scale: ChartScale::conflicts(self.log_conflicts),
            link: None,
        }
        .show(ui, (a.id, b.id), &series, &markers);
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui::Vec2;

    use super::*;
    use crate::gui::{
        config::GuiPreset,
        worker::{MAX_CHART_POINTS, tests::finished_run as finished},
    };

    #[test]
    fn archive_evicts_oldest_and_clears_only_stale_selections() {
        let result = finished(GuiConfig {
            board_size: 1,
            population_size: 1,
            ..Default::default()
        });
        let mut archive = RunArchive::default();
        for _ in 0..10 {
            archive.record(Arc::clone(&result));
        }
        archive.viewed = Some(1);
        archive.compare = [Some(1), Some(2)];
        assert_eq!(archive.record(result), 11);
        assert_eq!(archive.runs.len(), 10);
        assert!(archive.get(1).is_none());
        assert_eq!(archive.viewed, None);
        assert_eq!(archive.compare, [None, Some(2)]);
        archive.toggle_comparison(0, 2);
        assert_eq!(archive.compare, [Some(2), None]);
        archive.toggle_comparison(1, 999);
        assert_eq!(archive.compare, [Some(2), None]);
    }

    #[test]
    fn saved_rerun_uses_original_configuration_and_preserves_history() {
        let original = GuiPreset::Hybrid.config(987);
        let result = finished(original.clone());
        let mut app = NQueensApp::default();
        let id = app.archive.record(result);
        app.config.seed = 1;
        app.archive.viewed = Some(id);
        app.selected_queen = Some(0);
        app.board_view.set_zoom(8.0, Vec2::splat(0.5));
        assert!(app.rerun_saved(id, &egui::Context::default()));
        assert!(app.config == original);
        assert_eq!(app.archive.runs.len(), 1);
        assert_eq!(app.archive.viewed, None);
        assert_eq!(app.selected_queen, None);
        assert_eq!(app.board_view, BoardView::default());
        assert!(!app.rerun_saved(id, &egui::Context::default()));
    }

    #[test]
    fn seed_and_board_comparisons_are_explicit() {
        let a = GuiConfig::default();
        let mut b = a.clone();
        assert!(comparison_context(&a, &b).starts_with("Same seed"));
        b.seed += 1;
        assert!(comparison_context(&a, &b).starts_with("Different seeds"));
        b.board_size = 8;
        assert!(comparison_context(&a, &b).contains("not directly comparable"));
    }

    #[test]
    fn export_is_rectangular_reproducible_and_does_not_overwrite() {
        let result = finished(GuiConfig {
            board_size: 3,
            population_size: 1,
            max_epochs: 5_000,
            seed: 123,
            allow_unsolvable: true,
            ..Default::default()
        });
        let run = SavedRun { id: 7, result };
        assert!(run.result.history.points.len() <= MAX_CHART_POINTS);
        assert_eq!(run.result.history.points.first().unwrap().epoch(), 0);
        assert_eq!(run.result.history.points.last().unwrap().epoch(), 5_000);
        assert!(!run.result.history.restart_epochs.is_empty());
        let csv = run.csv();
        let mut lines = csv.lines();
        let header: Vec<_> = lines.next().unwrap().split(',').collect();
        let mut samples = 0;
        let mut restarts = 0;
        for line in lines {
            let fields: Vec<_> = line.split(',').collect();
            assert_eq!(header.len(), fields.len());
            let row: std::collections::HashMap<_, _> =
                header.iter().copied().zip(fields.iter().copied()).collect();
            assert_eq!(row["seed"], "123");
            assert_eq!(row["run_id"], "7");
            assert_eq!(row["allow_unsolvable"], "true");
            assert_eq!(row["stop_reason"], "epoch_limit");
            assert_eq!(row["downsampled"], "true");
            match row["record_type"] {
                "sample" => samples += 1,
                "restart" => restarts += 1,
                _ => panic!(),
            }
        }
        assert_eq!(samples, run.result.history.points.len());
        assert_eq!(restarts, run.result.history.restart_epochs.len());
        let path = std::env::temp_dir().join(format!(
            "queens-export-{}-{}.csv",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        run.export_csv(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), csv);
        // A longer existing file is replaced entirely, not partly overwritten.
        std::fs::write(&path, csv.repeat(2)).unwrap();
        run.export_csv(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), csv);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn completing_a_comparison_pair_shows_it() {
        let result = finished(GuiConfig::default());
        let mut app = NQueensApp::default();
        let a = app.archive.record(Arc::clone(&result));
        let b = app.archive.record(result);
        app.toggle_comparison(0, a);
        assert_eq!(app.chart_view, ChartView::Metrics);
        app.toggle_comparison(1, b);
        assert_eq!(app.chart_view, ChartView::Comparison);
    }

    #[test]
    fn history_and_comparison_layout_run_without_a_window() {
        let result = finished(GuiConfig {
            board_size: 3,
            population_size: 1,
            ..Default::default()
        });
        let mut app = NQueensApp::default();
        let a = app.archive.record(Arc::clone(&result));
        let b = app.archive.record(result);
        app.archive.compare = [Some(a), Some(b)];
        app.archive.viewed = Some(a);
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.draw_run_history(ui, &ctx);
            app.draw_comparison(ui);
        });
        assert_ne!(output.shapes.len(), 0);
        output.drop_without_applying_deltas();
    }
}
