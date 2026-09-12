use std::{
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

#[derive(Clone)]
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
            board_size: 18,
            population_size: 40_000,
            max_epochs: 5_000,
            seed: 42,
            mutation_rate: ga::DEFAULT_MUTATION_RATE,
            elite_ratio: ga::DEFAULT_ELITE_RATIO,
            offspring_ratio: ga::DEFAULT_OFFSPRING_RATIO,
            min_diversity_ratio: ga::DEFAULT_MIN_DIVERSITY_RATIO,
            selection_strategy: ga::DEFAULT_SELECTION_STRATEGY,
            tournament_size: ga::DEFAULT_TOURNAMENT_SIZE as u32,
            local_search_rate: ga::DEFAULT_LOCAL_SEARCH_RATE,
            local_search_attempts: ga::DEFAULT_LOCAL_SEARCH_ATTEMPTS as u32,
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

    fn use_measured_values(&mut self) {
        *self = Self {
            population_size: 4_000,
            max_epochs: 200,
            selection_strategy: SelectionStrategy::Tournament,
            local_search_rate: 0.05,
            seed: self.seed,
            ..Self::default()
        };
    }

    fn use_fast_demo_values(&mut self) {
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

struct RunningRun {
    receiver: Receiver<WorkerMessage>,
    cancel: Arc<AtomicBool>,
}

enum WorkerMessage {
    Snapshot(EpochSnapshot),
    Finished(RunResult),
    Failed(String),
}

struct RunResult {
    metrics: RunMetrics,
    best_positions: Vec<u16>,
    best_conflicts: Vec<u32>,
    best_conflicts_sum: u32,
    population_size: usize,
}

struct ChartSeries {
    label: &'static str,
    color: Color32,
    values: Vec<(u32, f32)>,
}

const MAX_CHART_POINTS: usize = 1024;

struct MetricHistory {
    points: Vec<ga::EpochMetrics>,
    stride: u64,
}

impl Default for MetricHistory {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            stride: 1,
        }
    }
}

impl MetricHistory {
    fn push(&mut self, metrics: ga::EpochMetrics, force: bool) {
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
    result: Option<RunResult>,
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
            result: None,
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
                        if let Some(metrics) = result.metrics.epochs().last() {
                            self.history.push(metrics.clone(), true);
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
        ui.heading("Parameters");
        ui.add_enabled_ui(!is_running, |ui| {
            ui.checkbox(
                &mut self.config.allow_unsolvable,
                "Experiment on unsolvable boards",
            );
        });
        ui.add_space(6.0);

        egui::Grid::new("parameter_grid")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Board size");
                ui.add_enabled(
                    !is_running,
                    egui::DragValue::new(&mut self.config.board_size)
                        .speed(1.0)
                        .range(1..=u16::MAX),
                );
                ui.end_row();

                ui.label("Population");
                ui.add_enabled(
                    !is_running,
                    egui::DragValue::new(&mut self.config.population_size).speed(500.0),
                );
                ui.end_row();

                ui.label("Max epochs");
                ui.add_enabled(
                    !is_running,
                    egui::DragValue::new(&mut self.config.max_epochs).speed(50.0),
                );
                ui.end_row();

                ui.label("Seed");
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        !is_running,
                        egui::DragValue::new(&mut self.config.seed).speed(1.0),
                    );
                    if ui
                        .add_enabled(!is_running, egui::Button::new("Random"))
                        .clicked()
                    {
                        self.config.seed = rand::rng().random::<u64>();
                    }
                });
                ui.end_row();
            });

        ui.separator();
        ui.label(RichText::new("Genetic algorithm").strong());

        ui.add_enabled(
            !is_running,
            egui::Slider::new(&mut self.config.mutation_rate, 0.0..=1.0).text("Mutation rate"),
        );
        ui.add_enabled(
            !is_running,
            egui::Slider::new(&mut self.config.elite_ratio, 0.0..=1.0).text("Elite ratio"),
        );
        ui.add_enabled(
            !is_running,
            egui::Slider::new(&mut self.config.offspring_ratio, 0.0..=1.0).text("Offspring ratio"),
        );
        ui.add_enabled(
            !is_running,
            egui::Slider::new(&mut self.config.min_diversity_ratio, 0.0..=1.0)
                .text("Min diversity"),
        );

        ui.add_space(4.0);
        ui.label("Selection");
        ui.add_enabled_ui(!is_running, |ui| {
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut self.config.selection_strategy,
                    SelectionStrategy::Roulette,
                    "Roulette",
                );
                ui.radio_value(
                    &mut self.config.selection_strategy,
                    SelectionStrategy::Tournament,
                    "Tournament",
                );
            });
            ui.horizontal(|ui| {
                ui.label("Tournament size");
                ui.add(egui::DragValue::new(&mut self.config.tournament_size).speed(1.0));
            });
        });

        ui.separator();
        ui.label(RichText::new("Local search").strong());
        ui.add_enabled(
            !is_running,
            egui::Slider::new(&mut self.config.local_search_rate, 0.0..=1.0)
                .text("Local search rate"),
        );
        ui.horizontal(|ui| {
            ui.label("Attempts");
            ui.add_enabled(
                !is_running,
                egui::DragValue::new(&mut self.config.local_search_attempts).speed(1.0),
            );
        });

        ui.separator();
        if ui
            .add_enabled(!is_running, egui::Button::new("Run solver"))
            .clicked()
        {
            self.start_run(ctx);
        }
        if ui
            .add_enabled(
                is_running && !self.cancel_requested,
                egui::Button::new("Cancel run"),
            )
            .clicked()
        {
            self.cancel_run();
        }
        if ui
            .add_enabled(!is_running, egui::Button::new("Fast demo values"))
            .clicked()
        {
            self.config.use_fast_demo_values();
        }
        if ui
            .add_enabled(!is_running, egui::Button::new("Measured 18×18 values"))
            .clicked()
        {
            self.config.use_measured_values();
        }
        if ui
            .add_enabled(!is_running, egui::Button::new("Reset defaults"))
            .clicked()
        {
            self.config = GuiConfig::default();
        }

        ui.separator();
        self.draw_current_metrics(ui);
    }

    fn draw_current_metrics(&self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Current run").strong());

        if let Some(error) = &self.error {
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

        if let Some(result) = &self.result {
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

    fn draw_main_panel(&self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("Board");
            ui.label(self.status_text());
        });
        ui.add_space(8.0);

        let board = self.current_board();
        ui.vertical_centered(|ui| {
            if let Some((positions, conflicts, conflicts_sum)) = board {
                draw_board(ui, positions, conflicts, conflicts_sum);
            } else {
                draw_empty_board(ui);
            }
        });

        ui.separator();
        draw_charts(ui, &self.history.points);
    }

    fn current_board(&self) -> Option<(&[u16], &[u32], u32)> {
        if let Some(result) = &self.result {
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

        if let Some(result) = &self.result {
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
        if let Some(result) = &self.result {
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
        let run_metrics = algorithm.run_algorithm_with_options(
            ga::RunOptions {
                allow_unsolvable: config.allow_unsolvable,
                collect_history: false,
                profile: false,
            },
            |snapshot| {
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

        let best_chromosome = algorithm.get_best_chromosome();
        let result = RunResult {
            metrics: run_metrics,
            best_positions: best_chromosome.get_positions().to_vec(),
            best_conflicts: best_chromosome.get_conflicts().to_vec(),
            best_conflicts_sum: best_chromosome.get_conflicts_sum(),
            population_size: algorithm.get_population_size(),
        };

        let _ = sender.send(WorkerMessage::Finished(result));
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
        format!("{:.2}s", ms as f64 / 1_000.0)
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

fn draw_board(ui: &mut egui::Ui, positions: &[u16], conflicts: &[u32], conflicts_sum: u32) {
    let size = positions.len();
    if size == 0 {
        draw_empty_board(ui);
        return;
    }

    ui.label(format!("{size} queens, {conflicts_sum} total conflicts"));
    let side = ui.available_width().clamp(280.0, 620.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(side, side), Sense::hover());
    let painter = ui.painter_at(rect);
    let cell = rect.width() / size as f32;

    if size <= 80 {
        draw_board_cells(&painter, rect, size, cell);
    } else {
        painter.rect_filled(
            rect,
            egui::CornerRadius::ZERO,
            Color32::from_rgb(19, 25, 35),
        );
        draw_sparse_grid(&painter, rect, 16);
    }

    for (x, &raw_y) in positions.iter().enumerate() {
        let y = usize::from(raw_y).min(size - 1);
        let center = Pos2::new(
            rect.left() + (x as f32 + 0.5) * cell,
            rect.top() + (y as f32 + 0.5) * cell,
        );
        let conflict_count = conflicts.get(x).copied().unwrap_or_default();
        let color = queen_color(conflict_count);
        let radius = (cell * 0.34).clamp(2.0, 18.0);

        painter.circle_filled(center, radius, color);
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
}

fn draw_board_cells(painter: &egui::Painter, rect: Rect, size: usize, cell: f32) {
    let dark = Color32::from_rgb(25, 32, 44);
    let light = Color32::from_rgb(39, 52, 68);
    for y in 0..size {
        for x in 0..size {
            let cell_rect = Rect::from_min_max(
                Pos2::new(rect.left() + x as f32 * cell, rect.top() + y as f32 * cell),
                Pos2::new(
                    rect.left() + (x + 1) as f32 * cell,
                    rect.top() + (y + 1) as f32 * cell,
                ),
            );
            let color = if (x + y) % 2 == 0 { light } else { dark };
            painter.rect_filled(cell_rect, egui::CornerRadius::ZERO, color);
        }
    }
}

fn draw_sparse_grid(painter: &egui::Painter, rect: Rect, divisions: usize) {
    painter.rect_filled(
        rect,
        egui::CornerRadius::ZERO,
        Color32::from_rgb(19, 25, 35),
    );
    let stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 210, 220, 45));
    for index in 0..=divisions {
        let t = index as f32 / divisions as f32;
        let x = rect.left() + rect.width() * t;
        let y = rect.top() + rect.height() * t;
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            stroke,
        );
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            stroke,
        );
    }
}

fn queen_color(conflicts: u32) -> Color32 {
    match conflicts {
        0 => Color32::from_rgb(95, 220, 140),
        1 | 2 => Color32::from_rgb(245, 190, 85),
        _ => Color32::from_rgb(245, 95, 105),
    }
}

fn draw_charts(ui: &mut egui::Ui, snapshots: &[ga::EpochMetrics]) {
    if snapshots.is_empty() {
        ui.label("Charts appear after the first epoch snapshot.");
        return;
    }

    let conflicts = vec![
        ChartSeries {
            label: "Best conflicts",
            color: Color32::from_rgb(95, 220, 140),
            values: snapshots
                .iter()
                .map(|snapshot| (snapshot.epoch(), snapshot.best_conflicts_sum() as f32))
                .collect(),
        },
        ChartSeries {
            label: "Average conflicts",
            color: Color32::from_rgb(110, 190, 255),
            values: snapshots
                .iter()
                .map(|snapshot| (snapshot.epoch(), snapshot.average_conflicts_sum()))
                .collect(),
        },
    ];
    draw_chart(ui, "Conflict history", &conflicts, 170.0);

    let rates = vec![
        ChartSeries {
            label: "Diversity ratio",
            color: Color32::from_rgb(245, 210, 95),
            values: snapshots
                .iter()
                .map(|snapshot| (snapshot.epoch(), snapshot.diversity_ratio()))
                .collect(),
        },
        ChartSeries {
            label: "Mutation rate",
            color: Color32::from_rgb(245, 120, 170),
            values: snapshots
                .iter()
                .map(|snapshot| (snapshot.epoch(), snapshot.mutation_rate()))
                .collect(),
        },
        ChartSeries {
            label: "Elite ratio",
            color: Color32::from_rgb(160, 135, 255),
            values: snapshots
                .iter()
                .map(|snapshot| (snapshot.epoch(), snapshot.elite_ratio()))
                .collect(),
        },
    ];
    draw_chart(ui, "Population ratios", &rates, 150.0);
}

fn draw_chart(ui: &mut egui::Ui, title: &str, series: &[ChartSeries], height: f32) {
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(title).strong());
        for line in series {
            ui.colored_label(line.color, line.label);
        }
    });

    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        egui::CornerRadius::ZERO,
        Color32::from_rgb(16, 22, 31),
    );

    let plot_rect = rect.shrink2(Vec2::new(42.0, 22.0));
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

    for line in series {
        draw_chart_series(&painter, plot_rect, max_epoch, max_value, line);
    }
}

fn draw_chart_grid(painter: &egui::Painter, rect: Rect, max_epoch: u32, max_value: f32) {
    let grid_stroke = Stroke::new(1.0, Color32::from_rgba_unmultiplied(180, 205, 220, 32));
    let text_color = Color32::from_rgb(150, 165, 180);

    for index in 0..=4 {
        let t = index as f32 / 4.0;
        let y = rect.bottom() - rect.height() * t;
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

    for index in 0..=4 {
        let t = index as f32 / 4.0;
        let x = rect.left() + rect.width() * t;
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            grid_stroke,
        );
        painter.text(
            Pos2::new(x, rect.bottom() + 6.0),
            Align2::CENTER_TOP,
            format!("{}", (max_epoch as f32 * t).round() as u32),
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
        let x = rect.left() + rect.width() * (epoch as f32 / max_epoch as f32);
        let y = rect.bottom() - rect.height() * (value / max_value).clamp(0.0, 1.0);
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
mod tests {
    use super::*;

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
            result: None,
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
