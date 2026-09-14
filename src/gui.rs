use std::{
    collections::VecDeque,
    fmt::Write as _,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError, TrySendError},
    },
    thread,
    time::Duration,
};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use rand::RngExt;

use crate::ga::{self, EpochSnapshot, GaConfig, RunMetrics, SelectionStrategy};

mod history;
use history::{ExportDialog, RunArchive};

/// Opens the native solver interface.
///
/// # Errors
/// Returns an eframe error if the window or graphics backend cannot initialize or run.
pub fn run() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 820.0])
            .with_min_inner_size([900.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "N-Queens Genetic Solver",
        native_options,
        Box::new(|cc| Ok(Box::new(NQueensApp::new(cc)))),
    )
}

#[derive(Clone, PartialEq)]
struct GuiConfig {
    board_size: u16,
    population_size: u32,
    max_epochs: u32,
    seed: u64,
    mutation_rate: f32,
    elite_ratio: f32,
    offspring_ratio: f32,
    min_diversity_ratio: f32,
    selection_strategy: SelectionStrategy,
    tournament_size: u32,
    local_search_rate: f32,
    local_search_attempts: u32,
    allow_unsolvable: bool,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            board_size: ga::DEFAULT_BOARD_SIZE,
            population_size: u32::try_from(ga::DEFAULT_POPULATION_SIZE)
                .expect("default population fits u32"),
            max_epochs: ga::DEFAULT_MAX_EPOCHS,
            seed: 42,
            mutation_rate: ga::DEFAULT_MUTATION_RATE,
            elite_ratio: ga::DEFAULT_ELITE_RATIO,
            offspring_ratio: ga::DEFAULT_OFFSPRING_RATIO,
            min_diversity_ratio: ga::DEFAULT_MIN_DIVERSITY_RATIO,
            selection_strategy: ga::DEFAULT_SELECTION_STRATEGY,
            tournament_size: u32::try_from(ga::DEFAULT_TOURNAMENT_SIZE)
                .expect("default tournament size fits u32"),
            local_search_rate: ga::DEFAULT_LOCAL_SEARCH_RATE,
            local_search_attempts: u32::try_from(ga::DEFAULT_LOCAL_SEARCH_ATTEMPTS)
                .expect("default local-search budget fits u32"),
            allow_unsolvable: false,
        }
    }
}

impl GuiConfig {
    fn normalize(&mut self) {
        self.board_size = self.board_size.max(1);
        self.population_size = self.population_size.max(1);
        self.max_epochs = self.max_epochs.max(1);
        self.tournament_size = self.tournament_size.max(1);
        self.mutation_rate = self.mutation_rate.clamp(0.0, 1.0);
        self.elite_ratio = self.elite_ratio.clamp(0.0, 1.0);
        self.offspring_ratio = self.offspring_ratio.clamp(0.0, 1.0);
        self.min_diversity_ratio = self.min_diversity_ratio.clamp(0.0, 1.0);
        self.local_search_rate = self.local_search_rate.clamp(0.0, 1.0);
    }

    fn to_ga_config(&self) -> Result<GaConfig, ga::GaConfigError> {
        GaConfig::new(
            self.board_size,
            self.population_size as usize,
            self.max_epochs,
            self.seed,
        )
        .with_mutation_rate(self.mutation_rate)
        .with_elite_ratio(self.elite_ratio)
        .with_offspring_ratio(self.offspring_ratio)
        .with_min_diversity_ratio(self.min_diversity_ratio)
        .with_selection_strategy(self.selection_strategy)
        .with_tournament_size(self.tournament_size as usize)
        .with_local_search_rate(self.local_search_rate)
        .with_local_search_attempts(self.local_search_attempts as usize)
        .validated()
    }

    /// The local-search hybrid, useful when a board stalls near a solution.
    fn use_hybrid_values(&mut self) {
        *self = Self {
            population_size: 4_000,
            selection_strategy: SelectionStrategy::Tournament,
            local_search_rate: 0.05,
            seed: self.seed,
            ..Self::default()
        };
    }

    /// The pre-2026-09-14 defaults: a large population with roulette selection
    /// and no local search. Spelled out rather than derived from the current
    /// defaults so this stays the classic configuration as those defaults move.
    fn use_classic_values(&mut self) {
        *self = Self {
            population_size: 40_000,
            mutation_rate: 0.08,
            offspring_ratio: 0.10,
            selection_strategy: SelectionStrategy::Roulette,
            local_search_rate: 0.0,
            seed: self.seed,
            ..Self::default()
        };
    }

    const fn use_fast_demo_values(&mut self) {
        self.board_size = 8;
        self.population_size = 256;
        self.max_epochs = 250;
        self.mutation_rate = 0.12;
        self.elite_ratio = 0.15;
        self.offspring_ratio = 0.25;
        self.selection_strategy = SelectionStrategy::Tournament;
        self.tournament_size = 3;
        self.local_search_rate = 0.05;
        self.local_search_attempts = 8;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GuiPreset {
    Demo,
    Default,
    Hybrid,
    Classic,
}

impl GuiPreset {
    const ALL: [Self; 4] = [Self::Demo, Self::Default, Self::Hybrid, Self::Classic];

    const fn label(self) -> &'static str {
        match self {
            Self::Demo => "Quick demo · 8×8",
            Self::Default => "Recommended defaults · 18×18",
            Self::Hybrid => "Local-search hybrid · 18×18",
            Self::Classic => "Classic genetic algorithm · 18×18",
        }
    }

    fn config(self, seed: u64) -> GuiConfig {
        let mut config = GuiConfig {
            seed,
            ..Default::default()
        };
        match self {
            Self::Demo => config.use_fast_demo_values(),
            Self::Hybrid => config.use_hybrid_values(),
            Self::Classic => config.use_classic_values(),
            Self::Default => {}
        }
        config
    }

    fn matching(config: &GuiConfig) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|preset| preset.config(config.seed) == *config)
    }
}

struct RunningRun {
    receiver: Receiver<WorkerMessage>,
    cancel: Arc<AtomicBool>,
}

enum WorkerMessage {
    Snapshot(EpochSnapshot),
    Finished(Arc<RunResult>),
    Failed(String),
}

struct RunResult {
    config: GuiConfig,
    history: MetricHistory,
    metrics: RunMetrics,
    best_positions: Vec<u16>,
    best_conflicts: Vec<u32>,
    best_conflicts_sum: u32,
    population_size: usize,
}

struct ChartMarker {
    epoch: u32,
    label: String,
    color: Color32,
}

struct ChartSeries {
    label: String,
    color: Color32,
    values: Vec<(u32, f32)>,
}

const MAX_CHART_POINTS: usize = 1024;

#[derive(Clone)]
struct MetricHistory {
    restart_epochs: VecDeque<u32>,
    points: Vec<ga::EpochMetrics>,
    stride: u64,
}

impl Default for MetricHistory {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            restart_epochs: VecDeque::new(),
            stride: 1,
        }
    }
}

impl MetricHistory {
    fn push(&mut self, metrics: ga::EpochMetrics, force: bool) {
        if let Some(epoch) = metrics.last_restart_epoch()
            && self.restart_epochs.back().is_none_or(|&last| last < epoch)
        {
            if self.restart_epochs.len() == MAX_CHART_POINTS {
                self.restart_epochs.pop_front();
            }
            self.restart_epochs.push_back(epoch);
        }
        if self
            .points
            .last()
            .is_some_and(|last| last.epoch() == metrics.epoch())
        {
            return;
        }
        if !force && !u64::from(metrics.epoch()).is_multiple_of(self.stride) {
            return;
        }
        if self.points.len() >= MAX_CHART_POINTS {
            self.stride *= 2;
            // Decimate by index as transport can skip epochs; always release half the space.
            let mut index = 0;
            self.points.retain(|_| {
                let keep = index % 2 == 0;
                index += 1;
                keep
            });
        }
        self.points.push(metrics);
    }
}

struct NQueensApp {
    config: GuiConfig,
    running: Option<RunningRun>,
    cancel_requested: bool,
    history: MetricHistory,
    latest_snapshot: Option<EpochSnapshot>,
    selected_queen: Option<usize>,
    result: Option<Arc<RunResult>>,
    archive: RunArchive,
    export: Option<ExportDialog>,
    notice: Option<String>,
    board_view: BoardView,
    error: Option<String>,
}

impl NQueensApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        Self {
            config: GuiConfig::default(),
            running: None,
            cancel_requested: false,
            history: MetricHistory::default(),
            latest_snapshot: None,
            selected_queen: None,
            result: None,
            archive: RunArchive::default(),
            export: None,
            notice: None,
            board_view: BoardView::default(),
            error: None,
        }
    }

    fn start_run(&mut self, ctx: &egui::Context) {
        if self.running.is_some() {
            return;
        }

        self.config.normalize();
        self.history = MetricHistory::default();
        self.latest_snapshot = None;
        self.selected_queen = None;
        self.board_view = BoardView::default();
        self.archive.viewed = None;
        self.result = None;
        self.error = None;
        self.cancel_requested = false;

        let (receiver, cancel) = spawn_solver(self.config.clone());
        self.running = Some(RunningRun { receiver, cancel });
        ctx.request_repaint();
    }

    fn cancel_run(&mut self) {
        if let Some(running) = &self.running {
            running.cancel.store(true, Ordering::Relaxed);
            self.cancel_requested = true;
        }
    }

    fn drain_worker_messages(&mut self, ctx: &egui::Context) {
        let mut finished = false;
        if let Some(running) = &self.running {
            // At most one pending snapshot and one terminal message per frame.
            for _ in 0..2 {
                match running.receiver.try_recv() {
                    Ok(WorkerMessage::Snapshot(snapshot)) => {
                        self.history.push(snapshot.metrics().clone(), false);
                        self.latest_snapshot = Some(snapshot);
                    }
                    Ok(WorkerMessage::Finished(result)) => {
                        self.history = result.history.clone();
                        let previous_view = self.archive.viewed;
                        self.archive.record(Arc::clone(&result));
                        if previous_view != self.archive.viewed {
                            self.selected_queen = None;
                            self.board_view = BoardView::default();
                        }
                        self.result = Some(result);
                        finished = true;
                        break;
                    }
                    Ok(WorkerMessage::Failed(error)) => {
                        self.error = Some(error);
                        finished = true;
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.error =
                            Some("Solver worker disconnected before reporting a result".into());
                        finished = true;
                        break;
                    }
                }
            }
        }

        if finished {
            self.running = None;
            self.cancel_requested = false;
        } else if self.running.is_some() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }

    fn draw_controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
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
                        ui.add(egui::DragValue::new(&mut self.config.seed).speed(1.0))
                            .on_hover_text("Use the same seed and settings to reproduce a run.");
                        if ui
                            .button("Random")
                            .on_hover_text("Choose a new seed")
                            .clicked()
                        {
                            self.config.seed = rand::rng().random::<u64>();
                        }
                    });
                    ui.end_row();
                });
        });
        ui.add_space(12.0);
        if is_running {
            if ui
                .add_enabled(
                    !self.cancel_requested,
                    egui::Button::new(if self.cancel_requested {
                        "Cancelling…"
                    } else {
                        "Cancel run"
                    })
                    .min_size(Vec2::new(ui.available_width(), 36.0)),
                )
                .clicked()
            {
                self.cancel_run();
            }
        } else if ui
            .add(
                egui::Button::new(RichText::new("Run solver").strong())
                    .fill(Color32::from_rgb(36, 91, 122))
                    .min_size(Vec2::new(ui.available_width(), 36.0)),
            )
            .clicked()
        {
            self.start_run(ctx);
        }
        ui.add_space(6.0);
        ui.weak(format!(
            "{} individuals · up to {} epochs",
            self.config.population_size, self.config.max_epochs
        ));
        ui.add_space(10.0);
        egui::CollapsingHeader::new("Advanced settings")
            .default_open(false)
            .show(ui, |ui| {
                // Recompute because the Run button can start a worker in this frame.
                ui.add_enabled_ui(self.running.is_none(), |ui| self.draw_advanced_controls(ui));
            });
        ui.separator();
        self.draw_current_metrics(ui);
        self.draw_run_history(ui, ctx);
    }

    fn draw_advanced_controls(&mut self, ui: &mut egui::Ui) {
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
        ui.checkbox(
            &mut self.config.allow_unsolvable,
            "Experiment on unsolvable boards",
        )
        .on_hover_text(
            "Allow sizes 2 and 3 to evolve for experiments instead of stopping at epoch zero.",
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

        if self.archive.viewed.is_none()
            && let Some(error) = &self.error
        {
            ui.colored_label(Color32::from_rgb(255, 120, 120), error);
            return;
        }

        let Some((metrics, best_conflicts_sum)) = self.current_metrics() else {
            ui.label("No run yet.");
            return;
        };

        egui::Grid::new("metrics_grid")
            .num_columns(2)
            .spacing([12.0, 5.0])
            .show(ui, |ui| {
                metric_row(ui, "Epoch", metrics.epoch().to_string());
                metric_row(ui, "Best conflicts", best_conflicts_sum.to_string());
                metric_row(
                    ui,
                    "Avg conflicts",
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
            });

        if let Some(result) = self.display_result() {
            ui.add_space(6.0);
            if result.metrics.stop_reason() == ga::StopReason::Cancelled {
                ui.colored_label(Color32::from_rgb(245, 190, 95), "Run cancelled");
            } else if let Some(epoch) = result.metrics.solved_epoch() {
                ui.colored_label(
                    Color32::from_rgb(105, 220, 145),
                    format!("Solved at epoch {epoch}"),
                );
            } else {
                let message = if result.metrics.stop_reason() == ga::StopReason::Unsolvable {
                    "This board size has no solution"
                } else {
                    "Epoch limit reached"
                };
                ui.colored_label(Color32::from_rgb(245, 190, 95), message);
            }
            ui.label(format!("Final population: {}", result.population_size));
            ui.label(format!(
                "Total elapsed: {}",
                format_ms(result.metrics.total_elapsed_ms())
            ));
        }
    }

    fn draw_main_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("Board");
            ui.label(self.status_text());
        });
        ui.add_space(8.0);

        let mut selected_queen = self.selected_queen;
        let mut board_view = self.board_view;
        let board = self.current_board();
        ui.vertical_centered(|ui| {
            if let Some((positions, conflicts, conflicts_sum)) = board {
                draw_board(
                    ui,
                    positions,
                    conflicts,
                    conflicts_sum,
                    &mut selected_queen,
                    &mut board_view,
                );
            } else {
                draw_empty_board(ui);
            }
        });

        self.selected_queen = selected_queen;
        self.board_view = board_view;
        ui.separator();
        let history = self
            .archive
            .viewed
            .and_then(|id| self.archive.get(id))
            .map_or(&self.history, |run| &run.result.history);
        draw_charts(ui, history);
        self.draw_comparison(ui);
    }

    fn display_result(&self) -> Option<&RunResult> {
        self.archive
            .viewed
            .and_then(|id| self.archive.get(id))
            .map(|run| run.result.as_ref())
            .or(self.result.as_deref())
    }

    fn current_board(&self) -> Option<(&[u16], &[u32], u32)> {
        if let Some(result) = self.display_result() {
            return Some((
                result.best_positions.as_slice(),
                result.best_conflicts.as_slice(),
                result.best_conflicts_sum,
            ));
        }

        self.latest_snapshot.as_ref().map(|snapshot| {
            (
                snapshot.best_positions(),
                snapshot.best_conflicts(),
                snapshot.best_conflicts_sum(),
            )
        })
    }

    fn status_text(&self) -> String {
        if let Some(run) = self.archive.viewed.and_then(|id| self.archive.get(id)) {
            return format!(
                "Run #{} · {} · {} best conflicts",
                run.id,
                run.result.metrics.stop_reason(),
                run.result.best_conflicts_sum
            );
        }
        if self.cancel_requested {
            return "Cancelling after current epoch".to_owned();
        }

        if self.running.is_some() {
            return self.latest_snapshot.as_ref().map_or_else(
                || "Starting solver".to_owned(),
                |snapshot| {
                    format!(
                        "Running epoch {} with {} best conflicts",
                        snapshot.metrics().epoch(),
                        snapshot.best_conflicts_sum()
                    )
                },
            );
        }

        if let Some(error) = &self.error {
            return format!("Error: {error}");
        }

        if let Some(result) = self.display_result() {
            if result.metrics.stop_reason() == ga::StopReason::Cancelled {
                return format!(
                    "Cancelled with {} best conflicts",
                    result.best_conflicts_sum
                );
            }
            if let Some(epoch) = result.metrics.solved_epoch() {
                return format!("Solved at epoch {epoch}");
            }
            if result.metrics.stop_reason() == ga::StopReason::Unsolvable {
                return "This board size has no solution".to_owned();
            }
            return format!(
                "Epoch limit reached with {} best conflicts",
                result.best_conflicts_sum
            );
        }

        "Ready".to_owned()
    }

    fn current_metrics(&self) -> Option<(&ga::EpochMetrics, u32)> {
        if let Some(result) = self.display_result() {
            return result
                .metrics
                .epochs()
                .last()
                .map(|metrics| (metrics, result.best_conflicts_sum));
        }

        self.latest_snapshot
            .as_ref()
            .map(|snapshot| (snapshot.metrics(), snapshot.best_conflicts_sum()))
    }
}

impl Drop for NQueensApp {
    fn drop(&mut self) {
        if let Some(running) = &self.running {
            running.cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl eframe::App for NQueensApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_worker_messages(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        egui::Panel::top("top_bar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("N-Queens Genetic Solver").strong().size(18.0));
                ui.separator();
                ui.label(self.status_text());
            });
        });

        egui::Panel::left("controls")
            .resizable(false)
            .default_size(315.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_controls(ui, &ctx);
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.draw_main_panel(ui);
            });
        });
        self.draw_export_window(&ctx);
    }
}

fn spawn_solver(config: GuiConfig) -> (Receiver<WorkerMessage>, Arc<AtomicBool>) {
    let (sender, receiver) = mpsc::sync_channel(1);
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = Arc::clone(&cancel);

    thread::spawn(move || {
        let ga_config = match config.to_ga_config() {
            Ok(config) => config,
            Err(error) => {
                let _ = sender.send(WorkerMessage::Failed(format!("Invalid GA config: {error}")));
                return;
            }
        };

        let mut algorithm = match ga::build_genetic_algorithm(ga_config) {
            Ok(algorithm) => algorithm,
            Err(error) => {
                let _ = sender.send(WorkerMessage::Failed(format!("Invalid GA config: {error}")));
                return;
            }
        };
        let progress_sender = sender.clone();
        let mut history = MetricHistory::default();
        let run_metrics = algorithm.run_algorithm_with_options(
            ga::RunOptions {
                allow_unsolvable: config.allow_unsolvable,
                collect_history: false,
                profile: false,
            },
            |snapshot| {
                history.push(snapshot.metrics().clone(), false);
                if cancel_worker.load(Ordering::Relaxed) {
                    return false;
                }

                let connected =
                    match progress_sender.try_send(WorkerMessage::Snapshot(snapshot.clone())) {
                        Ok(()) | Err(TrySendError::Full(_)) => true,
                        Err(TrySendError::Disconnected(_)) => false,
                    };
                connected && !cancel_worker.load(Ordering::Relaxed)
            },
        );

        if let Some(metrics) = run_metrics.epochs().last() {
            history.push(metrics.clone(), true);
        }
        let best_chromosome = algorithm.get_best_chromosome();
        let result = RunResult {
            config,
            history,
            metrics: run_metrics,
            best_positions: best_chromosome.get_positions().to_vec(),
            best_conflicts: best_chromosome.get_conflicts().to_vec(),
            best_conflicts_sum: best_chromosome.get_conflicts_sum(),
            population_size: algorithm.get_population_size(),
        };

        let _ = sender.send(WorkerMessage::Finished(Arc::new(result)));
    });

    (receiver, cancel)
}

fn metric_row(ui: &mut egui::Ui, label: &str, value: String) {
    ui.label(label);
    ui.label(RichText::new(value).monospace());
    ui.end_row();
}

fn format_ratio(value: f32) -> String {
    format!("{:.1}%", value * 100.0)
}

fn format_ms(ms: u128) -> String {
    if ms >= 1_000 {
        let centiseconds = ms / 10 + u128::from(ms % 10 >= 5);
        format!("{}.{:02}s", centiseconds / 100, centiseconds % 100)
    } else {
        format!("{ms} ms")
    }
}

fn draw_empty_board(ui: &mut egui::Ui) {
    let side = ui.available_width().clamp(260.0, 560.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(side, side), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        egui::CornerRadius::ZERO,
        Color32::from_rgb(18, 24, 34),
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "Run the solver to draw a board",
        FontId::proportional(18.0),
        Color32::from_rgb(180, 190, 205),
    );
}

/// Pan is measured in viewport widths, so resizing preserves the viewed board region.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BoardView {
    zoom: f32,
    pan: Vec2,
}

impl Default for BoardView {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

impl BoardView {
    fn set_zoom(&mut self, zoom: f32, anchor: Vec2) {
        if !zoom.is_finite() || !anchor.x.is_finite() || !anchor.y.is_finite() {
            return;
        }
        let next = zoom.clamp(1.0, 1024.0);
        self.pan = anchor - (anchor - self.pan) * (next / self.zoom);
        self.zoom = next;
        self.clamp_pan();
    }

    fn move_by(&mut self, delta: Vec2) {
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return;
        }
        self.pan += delta;
        self.clamp_pan();
    }

    fn clamp_pan(&mut self) {
        self.pan = self.pan.clamp(Vec2::splat(1.0 - self.zoom), Vec2::ZERO);
    }

    fn rect(self, viewport: Rect) -> Rect {
        Rect::from_min_size(
            viewport.min + self.pan * viewport.width(),
            viewport.size() * self.zoom,
        )
    }
}

// A board dimension and its coordinate indices fit the solver's u16 range.
fn board_units(value: usize) -> f32 {
    f32::from(u16::try_from(value).expect("board coordinate fits u16"))
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Chart coordinates are approximate f32 pixels; stored epochs and metrics retain their integer precision."
)]
const fn plot_value(value: u32) -> f32 {
    value as f32
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Screen-space indices intentionally round down; conversion saturates and callers bound indices to the board."
)]
const fn cell_index(value: f32) -> usize {
    value.max(0.0) as usize
}

fn visible_cells(start: f32, cell: f32, min: f32, max: f32, size: usize) -> std::ops::Range<usize> {
    let first = cell_index(((min - start) / cell).floor()).min(size);
    let end = cell_index(((max - start) / cell).ceil()).min(size);
    first..end.max(first)
}

const SELECTED_QUEEN_COLOR: Color32 = Color32::from_rgb(105, 195, 255);
const ATTACKER_COLOR: Color32 = Color32::from_rgb(255, 177, 86);

/// Positions are a permutation, so only diagonals can contain attacking queens.
fn attacking_columns(positions: &[u16], selected: usize) -> Vec<usize> {
    let Some(&row) = positions.get(selected) else {
        return Vec::new();
    };
    positions
        .iter()
        .enumerate()
        .filter(|&(column, &other_row)| {
            column != selected && column.abs_diff(selected) == usize::from(other_row.abs_diff(row))
        })
        .map(|(column, _)| column)
        .collect()
}

fn queen_at_pointer(rect: Rect, positions: &[u16], pointer: Pos2) -> Option<usize> {
    if positions.is_empty()
        || rect.width() <= 0.0
        || rect.height() <= 0.0
        || pointer.x < rect.left()
        || pointer.x >= rect.right()
        || pointer.y < rect.top()
        || pointer.y >= rect.bottom()
    {
        return None;
    }
    let column =
        cell_index((pointer.x - rect.left()) / rect.width() * board_units(positions.len()));
    let row = cell_index((pointer.y - rect.top()) / rect.height() * board_units(positions.len()));
    positions
        .get(column)
        .filter(|&&queen_row| usize::from(queen_row) == row)
        .map(|_| column)
}

fn queen_center(rect: Rect, size: usize, column: usize, row: usize) -> Pos2 {
    let cell = rect.width() / board_units(size);
    Pos2::new(
        (board_units(column) + 0.5).mul_add(cell, rect.left()),
        (board_units(row) + 0.5).mul_add(cell, rect.top()),
    )
}

/// Endpoints of a full diagonal, clipped to the board's outer edges.
fn diagonal_segment(rect: Rect, center: Pos2, descending: bool) -> [Pos2; 2] {
    if descending {
        let before = (center.x - rect.left()).min(center.y - rect.top());
        let after = (rect.right() - center.x).min(rect.bottom() - center.y);
        [center - Vec2::splat(before), center + Vec2::splat(after)]
    } else {
        let before = (center.x - rect.left()).min(rect.bottom() - center.y);
        let after = (rect.right() - center.x).min(center.y - rect.top());
        [
            center + Vec2::new(-before, before),
            center + Vec2::new(after, -after),
        ]
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "Keep immediate-mode board interaction and its layered rendering in the same frame scope."
)]
fn draw_board(
    ui: &mut egui::Ui,
    positions: &[u16],
    conflicts: &[u32],
    conflicts_sum: u32,
    selected: &mut Option<usize>,
    view: &mut BoardView,
) {
    let size = positions.len();
    if size == 0 {
        *selected = None;
        draw_empty_board(ui);
        return;
    }
    if selected.is_some_and(|column| column >= size) {
        *selected = None;
    }

    let conflict_label = if conflicts_sum == 1 {
        "attacking pair"
    } else {
        "attacking pairs"
    };
    ui.label(format!("{size} queens · {conflicts_sum} {conflict_label}"));
    ui.weak("Click a queen to inspect its conflicts. Click it again or an empty square to clear.");
    ui.horizontal_wrapped(|ui| {
        if ui.button("−").on_hover_text("Zoom out").clicked() {
            view.set_zoom(view.zoom / 1.5, Vec2::splat(0.5));
        }
        if ui.button("+").on_hover_text("Zoom in").clicked() {
            view.set_zoom(view.zoom * 1.5, Vec2::splat(0.5));
        }
        ui.label(format!("{:.0}%", view.zoom * 100.0));
        if ui.button("Fit board").clicked() {
            *view = BoardView::default();
        }
        ui.weak("Drag to pan · Ctrl/⌘ + scroll or pinch to zoom");
    });
    let gutter = 28.0;
    let board_side = (ui.available_width() - gutter).clamp(1.0, 620.0);
    let (outer, response) =
        ui.allocate_exact_size(Vec2::splat(board_side + gutter), Sense::click_and_drag());
    let viewport = Rect::from_min_size(outer.min + Vec2::splat(gutter), Vec2::splat(board_side));
    if response.dragged() {
        view.move_by(response.drag_delta() / board_side);
    }
    if let Some(pointer) = response
        .hover_pos()
        .filter(|point| viewport.contains(*point))
    {
        let zoom_delta = ui.input(eframe::egui::InputState::zoom_delta);
        if (zoom_delta - 1.0).abs() > f32::EPSILON {
            view.set_zoom(
                view.zoom * zoom_delta,
                (pointer - viewport.min) / board_side,
            );
        }
    }
    let rect = view.rect(viewport);
    let hovered = response
        .hover_pos()
        .filter(|point| viewport.contains(*point))
        .and_then(|pointer| queen_at_pointer(rect, positions, pointer));
    if response.clicked() {
        *selected = if hovered == *selected { None } else { hovered };
    }
    if response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        *selected = None;
    }
    if let Some(column) = hovered {
        let count = conflicts.get(column).copied().unwrap_or_default();
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(format!(
                "Column {}, row {}\n{} attacking queen{}\nClick to inspect",
                column + 1,
                usize::from(positions[column]) + 1,
                count,
                if count == 1 { "" } else { "s" }
            ));
    }
    let painter = ui.painter_at(viewport);
    let axes_painter = ui.painter_at(outer);
    let cell = rect.width() / board_units(size);
    if cell >= 6.0 {
        draw_board_cells(&painter, rect, size, cell);
    } else {
        draw_sparse_grid(&painter, rect, size, cell);
    }

    let attackers = selected
        .map(|column| attacking_columns(positions, column))
        .unwrap_or_default();
    let mut is_attacker = vec![false; size];
    for &column in &attackers {
        is_attacker[column] = true;
    }
    if let Some(column) = *selected {
        let row = usize::from(positions[column]);
        let center = queen_center(rect, size, column, row);
        for descending in [true, false] {
            if attackers.iter().any(|&other| {
                ((other > column) == (usize::from(positions[other]) > row)) == descending
            }) {
                let segment = diagonal_segment(rect, center, descending);
                painter.line_segment(
                    segment,
                    Stroke::new(
                        (cell * 0.24).clamp(3.0, 10.0),
                        Color32::from_rgba_unmultiplied(255, 177, 86, 50),
                    ),
                );
                painter.line_segment(segment, Stroke::new(1.5, ATTACKER_COLOR));
            }
        }
    }

    // Keep coordinates legible on large boards, including the selected row/column.
    let stride = cell_index((24.0 / cell).ceil()).max(1);
    for index in 0..size {
        let regular = index.is_multiple_of(stride) || index == size - 1;
        let selected_column = *selected == Some(index);
        let selected_row = selected.is_some_and(|column| usize::from(positions[column]) == index);
        let coordinate = (index + 1).to_string();
        let font = FontId::monospace(11.0);
        let x = (board_units(index) + 0.5).mul_add(cell, rect.left());
        let y = (board_units(index) + 0.5).mul_add(cell, rect.top());
        if (regular || selected_column) && x >= viewport.left() && x <= viewport.right() {
            axes_painter.text(
                Pos2::new(x, viewport.top() - 12.0),
                Align2::CENTER_CENTER,
                &coordinate,
                font.clone(),
                if selected_column {
                    SELECTED_QUEEN_COLOR
                } else {
                    Color32::GRAY
                },
            );
        }
        if (regular || selected_row) && y >= viewport.top() && y <= viewport.bottom() {
            axes_painter.text(
                Pos2::new(viewport.left() - 6.0, y),
                Align2::RIGHT_CENTER,
                &coordinate,
                font,
                if selected_row {
                    SELECTED_QUEEN_COLOR
                } else {
                    Color32::GRAY
                },
            );
        }
    }
    for (column, &raw_row) in positions.iter().enumerate() {
        let center = queen_center(rect, size, column, usize::from(raw_row));
        if !viewport.expand(22.0).contains(center) {
            continue;
        }
        let is_selected = *selected == Some(column);
        let color = if is_selected {
            SELECTED_QUEEN_COLOR
        } else if is_attacker[column] {
            ATTACKER_COLOR
        } else if selected.is_some() {
            Color32::from_rgb(80, 99, 114)
        } else {
            queen_color(conflicts.get(column).copied().unwrap_or_default())
        };
        let radius = (cell * 0.34).clamp(2.0, 18.0);
        painter.circle_filled(center, radius, color);
        if is_selected {
            painter.circle_stroke(center, radius + 3.0, Stroke::new(2.0, Color32::WHITE));
        } else if is_attacker[column] {
            // Square outlines distinguish attackers without relying on color alone.
            painter.rect_stroke(
                Rect::from_center_size(center, Vec2::splat((cell * 0.88).max(5.0))),
                egui::CornerRadius::ZERO,
                Stroke::new(1.5, ATTACKER_COLOR),
                egui::StrokeKind::Inside,
            );
        }
        if cell >= 16.0 {
            painter.text(
                center,
                Align2::CENTER_CENTER,
                "Q",
                FontId::proportional((cell * 0.42).clamp(10.0, 24.0)),
                Color32::from_rgb(12, 16, 22),
            );
        }
    }
    if let Some(column) = *selected {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "Selected: column {}, row {}",
                    column + 1,
                    usize::from(positions[column]) + 1
                ))
                .color(SELECTED_QUEEN_COLOR),
            );
            if ui.small_button("Clear selection").clicked() {
                *selected = None;
            }
        });
        if attackers.is_empty() {
            ui.label("This queen has no conflicts.");
        } else {
            let coordinates = attackers
                .iter()
                .take(8)
                .map(|&other| format!("C{} R{}", other + 1, usize::from(positions[other]) + 1))
                .collect::<Vec<_>>()
                .join(", ");
            let more = if attackers.len() > 8 {
                format!(" and {} more", attackers.len() - 8)
            } else {
                String::new()
            };
            ui.label(format!("Attacked by {} queen{}: {coordinates}{more}. Highlighted lines show their shared diagonals.", attackers.len(), if attackers.len() == 1 { "" } else { "s" }));
        }
    } else {
        ui.weak("Coordinates are 1-based: columns left to right, rows top to bottom.");
    }
}

fn draw_board_cells(painter: &egui::Painter, rect: Rect, size: usize, cell: f32) {
    let dark = Color32::from_rgb(25, 32, 44);
    let light = Color32::from_rgb(39, 52, 68);
    let clip = painter.clip_rect();
    for y in visible_cells(rect.top(), cell, clip.top(), clip.bottom(), size) {
        for x in visible_cells(rect.left(), cell, clip.left(), clip.right(), size) {
            let cell_rect = Rect::from_min_max(
                Pos2::new(
                    board_units(x).mul_add(cell, rect.left()),
                    board_units(y).mul_add(cell, rect.top()),
                ),
                Pos2::new(
                    rect.left() + board_units(x + 1) * cell,
                    rect.top() + board_units(y + 1) * cell,
                ),
            );
            let color = if (x + y) % 2 == 0 { light } else { dark };
            painter.rect_filled(cell_rect, egui::CornerRadius::ZERO, color);
        }
    }
}

fn draw_sparse_grid(painter: &egui::Painter, rect: Rect, size: usize, cell: f32) {
    painter.rect_filled(
        rect,
        egui::CornerRadius::ZERO,
        Color32::from_rgb(19, 25, 35),
    );
    let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 210, 220, 45));
    let clip = painter.clip_rect();
    let stride = cell_index((24.0 / cell).ceil()).max(1);
    for index in visible_cells(rect.left(), cell, clip.left(), clip.right(), size + 1)
        .filter(|index| index.is_multiple_of(stride))
    {
        let x = board_units(index).mul_add(cell, rect.left());
        painter.line_segment(
            [Pos2::new(x, clip.top()), Pos2::new(x, clip.bottom())],
            stroke,
        );
    }
    for index in visible_cells(rect.top(), cell, clip.top(), clip.bottom(), size + 1)
        .filter(|index| index.is_multiple_of(stride))
    {
        let y = board_units(index).mul_add(cell, rect.top());
        painter.line_segment(
            [Pos2::new(clip.left(), y), Pos2::new(clip.right(), y)],
            stroke,
        );
    }
}

const fn queen_color(conflicts: u32) -> Color32 {
    match conflicts {
        0 => Color32::from_rgb(95, 220, 140),
        1 | 2 => Color32::from_rgb(245, 190, 85),
        _ => Color32::from_rgb(245, 95, 105),
    }
}

fn draw_charts(ui: &mut egui::Ui, history: &MetricHistory) {
    let snapshots = &history.points;
    let markers: Vec<_> = history
        .restart_epochs
        .iter()
        .map(|&epoch| ChartMarker {
            epoch,
            label: "Soft restart".into(),
            color: Color32::from_rgb(245, 190, 85),
        })
        .collect();
    if snapshots.is_empty() {
        ui.label("Charts appear after the first epoch snapshot.");
        return;
    }
    ui.weak("Hover for retained sample values · R markers indicate soft restarts");
    if let Some(latest) = snapshots.last()
        && latest.restart_count() as usize > markers.len()
    {
        ui.weak(format!(
            "Showing {} of {} restart locations",
            markers.len(),
            latest.restart_count()
        ));
    }

    let conflicts = vec![
        ChartSeries {
            label: "Best conflicts".into(),
            color: Color32::from_rgb(95, 220, 140),
            values: snapshots
                .iter()
                .map(|snapshot| (snapshot.epoch(), plot_value(snapshot.best_conflicts_sum())))
                .collect(),
        },
        ChartSeries {
            label: "Average conflicts".into(),
            color: Color32::from_rgb(110, 190, 255),
            values: snapshots
                .iter()
                .map(|snapshot| (snapshot.epoch(), snapshot.average_conflicts_sum()))
                .collect(),
        },
    ];
    draw_chart(ui, "Conflict history", &conflicts, &markers, 170.0);

    let rates = vec![
        ChartSeries {
            label: "Diversity ratio".into(),
            color: Color32::from_rgb(245, 210, 95),
            values: snapshots
                .iter()
                .map(|snapshot| (snapshot.epoch(), snapshot.diversity_ratio()))
                .collect(),
        },
        ChartSeries {
            label: "Mutation rate".into(),
            color: Color32::from_rgb(245, 120, 170),
            values: snapshots
                .iter()
                .map(|snapshot| (snapshot.epoch(), snapshot.mutation_rate()))
                .collect(),
        },
        ChartSeries {
            label: "Elite ratio".into(),
            color: Color32::from_rgb(160, 135, 255),
            values: snapshots
                .iter()
                .map(|snapshot| (snapshot.epoch(), snapshot.elite_ratio()))
                .collect(),
        },
    ];
    draw_chart(ui, "Population ratios", &rates, &markers, 150.0);
}

fn draw_chart(
    ui: &mut egui::Ui,
    title: &str,
    series: &[ChartSeries],
    markers: &[ChartMarker],
    height: f32,
) {
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(title).strong());
        for line in series {
            ui.colored_label(line.color, &line.label);
        }
    });

    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        egui::CornerRadius::ZERO,
        Color32::from_rgb(16, 22, 31),
    );

    let plot_rect = rect.shrink2(Vec2::new(42.0, 22.0));
    if plot_rect.width() <= 0.0 || plot_rect.height() <= 0.0 {
        return;
    }
    let max_epoch = series
        .iter()
        .flat_map(|line| line.values.iter().map(|(epoch, _)| *epoch))
        .max()
        .unwrap_or(1)
        .max(1);
    let max_value = series
        .iter()
        .flat_map(|line| line.values.iter().map(|(_, value)| *value))
        .fold(0.0_f32, f32::max)
        .max(1.0);

    draw_chart_grid(&painter, plot_rect, max_epoch, max_value);

    for marker in markers {
        let x =
            plot_rect.left() + plot_rect.width() * plot_value(marker.epoch) / plot_value(max_epoch);
        painter.line_segment(
            [
                Pos2::new(x, plot_rect.top()),
                Pos2::new(x, plot_rect.bottom()),
            ],
            Stroke::new(1.0, marker.color.gamma_multiply(0.5)),
        );
        painter.text(
            Pos2::new(x, plot_rect.top()),
            Align2::CENTER_TOP,
            "R",
            FontId::monospace(10.0),
            marker.color,
        );
    }
    for line in series {
        draw_chart_series(&painter, plot_rect, max_epoch, max_value, line);
    }
    if let Some(pointer) = response
        .hover_pos()
        .filter(|point| plot_rect.contains(*point))
    {
        let epoch =
            f64::from((pointer.x - plot_rect.left()) / plot_rect.width()) * f64::from(max_epoch);
        painter.line_segment(
            [
                Pos2::new(pointer.x, plot_rect.top()),
                Pos2::new(pointer.x, plot_rect.bottom()),
            ],
            Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.4)),
        );
        let mut tooltip = String::from("Nearest retained samples (no extrapolation)");
        for line in series {
            if let Some((sample_epoch, value)) = nearest_chart_sample(&line.values, epoch) {
                write!(
                    tooltip,
                    "\n{} · epoch {}: {:.3}",
                    line.label, sample_epoch, value
                )
                .expect("writing to a String cannot fail");
                painter.circle_filled(
                    Pos2::new(
                        plot_rect.left()
                            + plot_rect.width() * plot_value(sample_epoch) / plot_value(max_epoch),
                        plot_rect
                            .height()
                            .mul_add(-(value / max_value).clamp(0.0, 1.0), plot_rect.bottom()),
                    ),
                    4.0,
                    line.color,
                );
            }
        }
        let tolerance = f64::from(max_epoch) * 8.0 / f64::from(plot_rect.width());
        for marker in markers
            .iter()
            .filter(|marker| (f64::from(marker.epoch) - epoch).abs() <= tolerance)
            .take(6)
        {
            write!(tooltip, "\n{} at epoch {}", marker.label, marker.epoch)
                .expect("writing to a String cannot fail");
        }
        response.on_hover_text(tooltip);
    }
}

fn nearest_chart_sample(values: &[(u32, f32)], epoch: f64) -> Option<(u32, f32)> {
    if !epoch.is_finite()
        || epoch < f64::from(values.first()?.0)
        || epoch > f64::from(values.last()?.0)
    {
        return None;
    }
    let right = values.partition_point(|&(sample, _)| f64::from(sample) < epoch);
    match (
        right.checked_sub(1).and_then(|left| values.get(left)),
        values.get(right),
    ) {
        (Some(&a), Some(&b)) => Some(if epoch - f64::from(a.0) <= f64::from(b.0) - epoch {
            a
        } else {
            b
        }),
        (Some(&a), None) => Some(a),
        (None, Some(&b)) => Some(b),
        (None, None) => None,
    }
}

fn draw_chart_grid(painter: &egui::Painter, rect: Rect, max_epoch: u32, max_value: f32) {
    let grid_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(180, 205, 220, 32));
    let text_color = Color32::from_rgb(150, 165, 180);

    for index in 0_u16..=4 {
        let t = f32::from(index) / 4.0;
        let y = rect.height().mul_add(-t, rect.bottom());
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            grid_stroke,
        );
        painter.text(
            Pos2::new(rect.left() - 6.0, y),
            Align2::RIGHT_CENTER,
            format!("{:.1}", max_value * t),
            FontId::monospace(11.0),
            text_color,
        );
    }

    let steps = u16::try_from(max_epoch.min(4)).expect("at most four chart intervals");
    for index in 0..=steps {
        let t = f32::from(index) / f32::from(steps);
        let x = rect.width().mul_add(t, rect.left());
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            grid_stroke,
        );
        painter.text(
            Pos2::new(x, rect.bottom() + 6.0),
            Align2::CENTER_TOP,
            ((u64::from(max_epoch) * u64::from(index) + u64::from(steps) / 2) / u64::from(steps))
                .to_string(),
            FontId::monospace(11.0),
            text_color,
        );
    }
}

fn draw_chart_series(
    painter: &egui::Painter,
    rect: Rect,
    max_epoch: u32,
    max_value: f32,
    series: &ChartSeries,
) {
    let to_pos = |epoch: u32, value: f32| {
        let x = rect
            .width()
            .mul_add(plot_value(epoch) / plot_value(max_epoch), rect.left());
        let y = rect
            .height()
            .mul_add(-(value / max_value).clamp(0.0, 1.0), rect.bottom());
        Pos2::new(x, y)
    };
    let stroke = Stroke::new(2.0, series.color);

    if let Some(&(epoch, value)) = series.values.first() {
        painter.circle_filled(to_pos(epoch, value), 2.5, series.color);
    }

    for pair in series.values.windows(2) {
        let start = to_pos(pair[0].0, pair[0].1);
        let end = to_pos(pair[1].0, pair[1].1);
        painter.line_segment([start, end], stroke);
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "These tests check exact preset values and clamped zoom limits, not approximate calculations."
)]
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
    fn board_zoom_preserves_pointer_anchor_and_pan_is_bounded() {
        let viewport = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::splat(400.0));
        let positions = [1, 3, 0, 2];
        let mut view = BoardView::default();
        let point = queen_center(viewport, 4, 2, 0);
        let anchor = (point - viewport.min) / viewport.width();
        view.set_zoom(4.0, anchor);
        assert_eq!(
            queen_at_pointer(view.rect(viewport), &positions, point),
            Some(2)
        );
        view.move_by(Vec2::splat(100.0));
        assert_eq!(view.pan, Vec2::ZERO);
        view.move_by(Vec2::splat(-100.0));
        assert_eq!(view.pan, Vec2::splat(-3.0));
        let before = view;
        view.set_zoom(f32::NAN, anchor);
        assert_eq!(view, before);
        view.set_zoom(1.0, anchor);
        assert_eq!(view, BoardView::default());
        view.set_zoom(100_000.0, anchor);
        assert_eq!(view.zoom, 1024.0);
    }

    #[test]
    fn visible_cell_range_limits_work_on_large_zoomed_boards() {
        let range = visible_cells(-9_900.0, 10.0, 0.0, 400.0, 65_535);
        assert_eq!(range, 990..1030);
        assert_eq!(visible_cells(500.0, 10.0, 0.0, 400.0, 100), 0..0);
        assert_eq!(visible_cells(0.0, 10.0, 0.0, 1000.0, 4), 0..4);
    }

    #[test]
    fn chart_hover_uses_nearest_sample_and_never_extrapolates() {
        let points = [(0, 10.0), (10, 5.0), (20, 0.0)];
        assert_eq!(nearest_chart_sample(&points, 6.0), Some((10, 5.0)));
        assert_eq!(nearest_chart_sample(&points, 5.0), Some((0, 10.0)));
        assert_eq!(nearest_chart_sample(&points, 20.0), Some((20, 0.0)));
        assert_eq!(nearest_chart_sample(&points, 21.0), None);
        assert_eq!(nearest_chart_sample(&points, -1.0), None);
        assert_eq!(nearest_chart_sample(&points, f64::NAN), None);
        assert_eq!(nearest_chart_sample(&[], 0.0), None);
        assert_eq!(nearest_chart_sample(&[(0, 1.0)], 0.0), Some((0, 1.0)));
    }

    #[test]
    fn zoomed_large_board_layout_is_bounded_without_a_window() {
        let ctx = egui::Context::default();
        let positions: Vec<_> = (0..u16::MAX).collect();
        let conflicts = vec![u32::from(u16::MAX) - 1; positions.len()];
        let mut selected = Some(32_000);
        let mut view = BoardView::default();
        view.set_zoom(1024.0, Vec2::splat(0.5));
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            draw_board(ui, &positions, &conflicts, 1, &mut selected, &mut view);
        });
        assert!(!output.shapes.is_empty());
        assert!(output.shapes.len() < 30_000);
        output.drop_without_applying_deltas();
    }

    #[test]
    fn presets_reset_all_parameters_and_preserve_seed() {
        for preset in GuiPreset::ALL {
            let config = preset.config(123);
            assert_eq!(config.seed, 123);
            assert!(config.to_ga_config().is_ok());
            assert!(GuiPreset::matching(&config) == Some(preset));
            assert!(!config.allow_unsolvable);
            let mut custom = config.clone();
            custom.population_size += 1;
            assert!(GuiPreset::matching(&custom).is_none());
        }
        let hybrid = GuiPreset::Hybrid.config(42);
        assert_eq!(hybrid.population_size, 4_000);
        assert_eq!(hybrid.local_search_rate, 0.05);
        assert_eq!(hybrid.selection_strategy, SelectionStrategy::Tournament);

        // The classic preset must keep the pre-2026-09-14 behavior even as the
        // shipped defaults change, so it is checked against literals.
        let classic = GuiPreset::Classic.config(42);
        assert_eq!(classic.population_size, 40_000);
        assert_eq!(classic.mutation_rate, 0.08);
        assert_eq!(classic.offspring_ratio, 0.10);
        assert_eq!(classic.selection_strategy, SelectionStrategy::Roulette);
        assert_eq!(classic.local_search_rate, 0.0);

        // The plain default preset must track the shipped defaults.
        let default = GuiPreset::Default.config(42);
        assert_eq!(
            usize::try_from(default.population_size).unwrap(),
            ga::DEFAULT_POPULATION_SIZE
        );
        assert_eq!(default.selection_strategy, ga::DEFAULT_SELECTION_STRATEGY);
        assert_eq!(default.mutation_rate, ga::DEFAULT_MUTATION_RATE);
    }

    #[test]
    fn selected_queen_attackers_match_solver_counts() {
        for positions in [
            vec![0, 1, 2, 3, 4],
            vec![4, 3, 2, 1, 0],
            vec![0, 2, 1, 3, 4],
            vec![1, 3, 0, 2],
            vec![0],
        ] {
            let chromosome = ga::chromosome::Chromosome::new(positions.clone());
            for column in 0..positions.len() {
                let attackers = attacking_columns(&positions, column);
                assert_eq!(attackers.len(), chromosome.get_conflicts()[column] as usize);
                assert!(!attackers.contains(&column));
                assert!(
                    attackers
                        .iter()
                        .all(|&other| attacking_columns(&positions, other).contains(&column))
                );
            }
        }
        assert_eq!(attacking_columns(&[0, 2, 1, 3, 4], 1), vec![2]);
        assert!(attacking_columns(&[1, 3, 0, 2], 0).is_empty());
        assert!(attacking_columns(&[], 0).is_empty());
        assert!(attacking_columns(&[0], 5).is_empty());
    }

    #[test]
    fn board_hit_testing_distinguishes_queens_empty_squares_and_edges() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::splat(80.0));
        let positions = [1, 3, 0, 2];
        for (column, &row) in positions.iter().enumerate() {
            assert_eq!(
                queen_at_pointer(
                    rect,
                    &positions,
                    queen_center(rect, 4, column, row as usize)
                ),
                Some(column)
            );
        }
        for point in [
            Pos2::new(10.0, 20.0),
            Pos2::new(9.0, 50.0),
            Pos2::new(90.0, 50.0),
            Pos2::new(50.0, 100.0),
        ] {
            assert_eq!(queen_at_pointer(rect, &positions, point), None);
        }
        assert_eq!(queen_at_pointer(rect, &[], rect.center()), None);
        assert_eq!(queen_at_pointer(rect, &[0], rect.min), Some(0));
    }

    #[test]
    fn highlighted_diagonals_reach_board_edges() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::splat(80.0));
        assert_eq!(
            diagonal_segment(rect, rect.center(), true),
            [rect.left_top(), rect.right_bottom()]
        );
        assert_eq!(
            diagonal_segment(rect, rect.center(), false),
            [rect.left_bottom(), rect.right_top()]
        );
        let center = Pos2::new(20.0, 50.0);
        assert_eq!(
            diagonal_segment(rect, center, true),
            [Pos2::new(10.0, 40.0), Pos2::new(70.0, 100.0)]
        );
        assert_eq!(
            diagonal_segment(rect, center, false),
            [Pos2::new(10.0, 60.0), Pos2::new(50.0, 20.0)]
        );
    }

    #[test]
    fn chart_history_stays_bounded_and_keeps_endpoints() {
        let mut history = MetricHistory::default();
        let mut algorithm = ga::build_genetic_algorithm(GaConfig::new(3, 1, 5_000, 42)).unwrap();
        let metrics = algorithm.run_algorithm_with_options(
            ga::RunOptions {
                allow_unsolvable: true,
                collect_history: false,
                profile: false,
            },
            |snapshot| {
                history.push(snapshot.metrics().clone(), false);
                assert!(history.points.len() <= MAX_CHART_POINTS);
                true
            },
        );
        history.push(metrics.epochs().last().unwrap().clone(), true);
        assert_eq!(history.points.first().unwrap().epoch(), 0);
        assert_eq!(history.points.last().unwrap().epoch(), 5_000);
        assert!(history.points.len() <= MAX_CHART_POINTS);
        assert!(
            history
                .points
                .windows(2)
                .all(|pair| pair[0].epoch() < pair[1].epoch())
        );
    }

    #[test]
    fn disconnected_worker_clears_running_state() {
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(sender);
        let mut app = NQueensApp {
            config: GuiConfig::default(),
            running: Some(RunningRun {
                receiver,
                cancel: Arc::new(AtomicBool::new(false)),
            }),
            cancel_requested: false,
            history: MetricHistory::default(),
            latest_snapshot: None,
            selected_queen: None,
            result: None,
            archive: RunArchive::default(),
            export: None,
            notice: None,
            board_view: BoardView::default(),
            error: None,
        };
        app.drain_worker_messages(&egui::Context::default());
        assert!(app.running.is_none());
        assert!(app.error.as_ref().unwrap().contains("disconnected"));
    }

    #[test]
    fn full_progress_queue_preserves_terminal_result() {
        let (receiver, _) = spawn_solver(GuiConfig {
            board_size: 1,
            population_size: 1,
            ..Default::default()
        });
        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
            WorkerMessage::Snapshot(_)
        ));
        match receiver.recv_timeout(Duration::from_secs(5)).unwrap() {
            WorkerMessage::Finished(result) => {
                assert_eq!(result.metrics.stop_reason(), ga::StopReason::Solved);
                assert_eq!(result.metrics.epochs().len(), 1);
                assert_eq!(result.best_conflicts_sum, 0);
            }
            _ => panic!("expected terminal result"),
        }
    }

    #[test]
    fn cancelled_worker_reports_authoritative_stop_reason() {
        let (receiver, cancel) = spawn_solver(GuiConfig {
            board_size: 3,
            population_size: 8,
            max_epochs: 100_000,
            allow_unsolvable: true,
            ..Default::default()
        });
        cancel.store(true, Ordering::Relaxed);
        loop {
            match receiver.recv_timeout(Duration::from_secs(5)).unwrap() {
                WorkerMessage::Snapshot(_) => {}
                WorkerMessage::Finished(result) => {
                    assert_eq!(result.metrics.stop_reason(), ga::StopReason::Cancelled);
                    break;
                }
                WorkerMessage::Failed(error) => panic!("{error}"),
            }
        }
    }
}
