use std::{collections::VecDeque, fmt::Write as _, sync::Arc};
#[cfg(not(target_arch = "wasm32"))]
use std::{fs, io, path::Path};

use eframe::egui;

use super::{
    BoardView, ChartView, NQueensApp, Step, attacking_pairs,
    charts::{Chart, ChartMarker, ChartScale, ChartSeries, plot_height, plot_value},
    config::GuiConfig,
    format_ms,
    palette::Palette,
    solver_name,
    worker::{Progress, RunResult},
};
use crate::Solver;

const MAX_SAVED_RUNS: usize = 10;

/// For min-conflicts runs, `epoch` holds the pass and the GA-only columns are empty.
const CSV_HEADER: &str = "record_type,run_id,solver,seed,board_size,target_population,max_epochs,mutation_rate,elite_ratio,offspring_ratio,min_diversity_ratio,selection_strategy,tournament_size,local_search_rate,local_search_attempts,allow_unsolvable,stop_reason,solved_epoch,run_elapsed_ms,retained_samples,downsampled,epoch,best_conflicts_sum,average_conflicts_sum,population_size,elapsed_ms,unique_chromosomes,diversity_ratio,epoch_mutation_rate,epoch_elite_ratio,offspring_count,local_search_improvements,stagnation_epochs,diversity_replacements,restart_count,last_restart_epoch,current_conflicts_sum,swaps";
/// The per-sample columns, from `epoch` to `swaps`.
const CSV_SAMPLE_COLUMNS: usize = 17;

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
    let mut context = if a.board_size == b.board_size {
        format!(
            "{seed} · Both {}×{}. Compare multiple seeds before drawing conclusions about settings.",
            a.board_size, a.board_size
        )
    } else {
        format!(
            "{seed} · Different board sizes ({} and {}): attacking-pair counts are not directly comparable.",
            a.board_size, b.board_size
        )
    };
    if a.solver != b.solver {
        context.push_str(
            " Different solvers: a GA epoch and a min-conflicts pass are different units of work, so the x axis does not line up.",
        );
    }
    context
}

/// A saved run's settings, as the solver uses them.
fn draw_settings(ui: &mut egui::Ui, config: &GuiConfig) {
    if config.solver == Solver::MinConflicts {
        ui.label(format!(
            "Min-conflicts · passes {} · unsolvable experiments {}",
            config.max_epochs, config.allow_unsolvable
        ));
        return;
    }
    ui.label(format!(
        "Population {} · epochs {} · {} selection (tournament size {})",
        config.population_size,
        config.max_epochs,
        config.selection_strategy,
        config.tournament_size
    ));
    ui.label(format!(
        "Mutation {} · elite {} · offspring {} · diversity {}",
        config.mutation_rate,
        config.elite_ratio,
        config.offspring_ratio,
        config.min_diversity_ratio
    ));
    ui.label(format!(
        "Local search {} · attempts {} · unsolvable experiments {}",
        config.local_search_rate, config.local_search_attempts, config.allow_unsolvable
    ));
}

impl SavedRun {
    fn csv(&self) -> String {
        let config = &self.result.config;
        let history = &self.result.history;
        let is_ga = config.solver == Solver::Ga;
        // Min-conflicts leaves the GA's settings and population metrics blank.
        let ga_only = |value: String| if is_ga { value } else { String::new() };
        let optional =
            |value: Option<u32>| value.map(|value| value.to_string()).unwrap_or_default();
        let common = [
            self.id.to_string(),
            config.solver.to_string(),
            config.seed.to_string(),
            config.board_size.to_string(),
            ga_only(config.population_size.to_string()),
            config.max_epochs.to_string(),
            ga_only(config.mutation_rate.to_string()),
            ga_only(config.elite_ratio.to_string()),
            ga_only(config.offspring_ratio.to_string()),
            ga_only(config.min_diversity_ratio.to_string()),
            ga_only(config.selection_strategy.to_string()),
            ga_only(config.tournament_size.to_string()),
            ga_only(config.local_search_rate.to_string()),
            ga_only(config.local_search_attempts.to_string()),
            config.allow_unsolvable.to_string(),
            self.result.stop_reason.to_string(),
            optional(self.result.solved_step),
            self.result.total_elapsed_ms.to_string(),
            history.points.len().to_string(),
            history
                .points
                .last()
                .is_some_and(|last| (history.points.len() as u64) < u64::from(last.step()) + 1)
                .to_string(),
        ];
        let mut output = format!("{CSV_HEADER}\n");
        for point in &history.points {
            let values = match point {
                Progress::Ga(metrics) => [
                    metrics.epoch().to_string(),
                    metrics.best_conflicts_sum().to_string(),
                    metrics.average_conflicts_sum().to_string(),
                    metrics.population_size().to_string(),
                    metrics.elapsed_ms().to_string(),
                    metrics.unique_chromosomes().to_string(),
                    metrics.diversity_ratio().to_string(),
                    metrics.mutation_rate().to_string(),
                    metrics.elite_ratio().to_string(),
                    metrics.offspring_count().to_string(),
                    metrics.local_search_improvements().to_string(),
                    metrics.stagnation_epochs().to_string(),
                    metrics.diversity_replacements().to_string(),
                    metrics.restart_count().to_string(),
                    optional(metrics.last_restart_epoch()),
                    String::new(),
                    String::new(),
                ],
                Progress::MinConflicts(metrics) => {
                    let mut values: [String; CSV_SAMPLE_COLUMNS] = Default::default();
                    values[0] = metrics.pass().to_string();
                    values[1] = metrics.best_conflicts_sum().to_string();
                    values[4] = metrics.elapsed_ms().to_string();
                    values[13] = metrics.restarts().to_string();
                    values[14] = optional(metrics.last_restart_pass());
                    values[15] = metrics.current_conflicts_sum().to_string();
                    values[16] = metrics.swaps().to_string();
                    values
                }
            };
            writeln!(output, "sample,{},{}", common.join(","), values.join(","))
                .expect("writing to a String cannot fail");
        }
        // Preserve restart locations even when the corresponding metric sample was decimated.
        for epoch in &history.restart_epochs {
            let mut values = vec![String::new(); CSV_SAMPLE_COLUMNS];
            values[0] = epoch.to_string();
            writeln!(output, "restart,{},{}", common.join(","), values.join(","))
                .expect("writing to a String cannot fail");
        }
        output
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn export_csv(&self, path: &Path) -> io::Result<()> {
        // The native save dialog has already confirmed replacing an existing file.
        fs::write(path, self.csv())
    }
}

impl NQueensApp {
    /// Asks for a destination with the native save dialog, then writes the CSV.
    #[cfg(not(target_arch = "wasm32"))]
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

    /// Hands the CSV to the browser, which saves it like any other download.
    #[cfg(target_arch = "wasm32")]
    fn export_run(&mut self, id: u64, ctx: &egui::Context) {
        let Some(run) = self.archive.get(id) else {
            return;
        };
        let name = format!("n-queens-run-{id}.csv");
        match download(&name, &run.csv()) {
            Ok(()) => self.notify(ctx, format!("Exported run #{id} as {name}"), false),
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
                ui.label(format!(
                    "{} · {}×{} · seed {} · {} · {}",
                    solver_name(result.config.solver),
                    result.config.board_size,
                    result.config.board_size,
                    result.config.seed,
                    attacking_pairs(result.best_conflicts_sum),
                    format_ms(result.total_elapsed_ms)
                ));
                ui.collapsing("Settings", |ui| draw_settings(ui, &result.config));
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
        let (solver_a, solver_b) = (a.result.config.solver, b.result.config.solver);
        let x_label = if solver_a == solver_b {
            Step::of(solver_a).title
        } else {
            "Epoch / pass"
        };
        ui.label(comparison_context(&a.result.config, &b.result.config));
        ui.weak(format!(
            "Best-board attacking pairs by {} · sampled points · dashed lines mark restarts",
            x_label.to_lowercase()
        ));
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
                    .map(|point| (point.step(), plot_value(point.best_conflicts_sum())))
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
            x_label,
            height: plot_height(ui, 1),
            scale: ChartScale::conflicts(self.log_conflicts),
            link: None,
        }
        .show(ui, (a.id, b.id), &series, &markers);
    }
}

/// Starts a browser download of `contents` through a temporary object URL.
#[cfg(target_arch = "wasm32")]
fn download(file_name: &str, contents: &str) -> Result<(), String> {
    use wasm_bindgen::{JsCast as _, JsValue, closure::Closure};

    /// Long enough for the browser to start reading the blob before its URL is released.
    const REVOKE_AFTER_MS: i32 = 10_000;

    let describe = |error: JsValue| format!("{error:?}");
    let window = web_sys::window().ok_or("no browser window")?;
    let document = window.document().ok_or("no document")?;
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("text/csv");
    let parts = js_sys::Array::of1(&JsValue::from_str(contents));
    let blob =
        web_sys::Blob::new_with_str_sequence_and_options(&parts, &options).map_err(describe)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(describe)?;

    let anchor: web_sys::HtmlAnchorElement = document
        .create_element("a")
        .map_err(describe)?
        .unchecked_into();
    anchor.set_href(&url);
    anchor.set_download(file_name);
    anchor.click();

    let revoke = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            revoke.unchecked_ref(),
            REVOKE_AFTER_MS,
        )
        .map_err(describe)?;
    Ok(())
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
        assert!(!comparison_context(&a, &b).contains("Different solvers"));
        b.solver = Solver::MinConflicts;
        assert!(comparison_context(&a, &b).contains("Different solvers"));
    }

    #[test]
    fn min_conflicts_export_leaves_ga_columns_empty() {
        let result = finished(GuiConfig {
            solver: Solver::MinConflicts,
            board_size: 3,
            max_epochs: 40,
            allow_unsolvable: true,
            ..Default::default()
        });
        let run = SavedRun { id: 3, result };
        let csv = run.csv();
        let mut lines = csv.lines();
        let header: Vec<_> = lines.next().unwrap().split(',').collect();
        let mut samples = 0;
        for line in lines {
            let fields: Vec<_> = line.split(',').collect();
            assert_eq!(header.len(), fields.len());
            let row: std::collections::HashMap<_, _> =
                header.iter().copied().zip(fields.iter().copied()).collect();
            assert_eq!(row["solver"], "min-conflicts");
            assert_eq!(row["max_epochs"], "40");
            assert_eq!(row["stop_reason"], "epoch_limit");
            for column in ["target_population", "mutation_rate", "selection_strategy"] {
                assert_eq!(row[column], "", "{column}");
            }
            if row["record_type"] == "sample" {
                samples += 1;
                assert_eq!(row["average_conflicts_sum"], "");
                assert!(row["current_conflicts_sum"].parse::<u32>().is_ok());
                assert!(row["swaps"].parse::<u64>().is_ok());
            }
        }
        assert_eq!(samples, run.result.history.points.len());
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
        assert_eq!(run.result.history.points.first().unwrap().step(), 0);
        assert_eq!(run.result.history.points.last().unwrap().step(), 5_000);
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
            assert_eq!(row["solver"], "ga");
            assert_eq!(row["target_population"], "1");
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
        let baseline = finished(GuiConfig {
            solver: Solver::MinConflicts,
            board_size: 8,
            ..Default::default()
        });
        let mut app = NQueensApp::default();
        let a = app.archive.record(Arc::clone(&result));
        let b = app.archive.record(baseline);
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
