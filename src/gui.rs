//! The native desktop GUI: app state, the worker message loop and the panel layout.
//!
//! Each panel is drawn by its own module: [`top_bar`] for run actions and status,
//! [`controls`] for the left settings panel, [`board`] for the central board,
//! [`charts`] for the bottom chart panel and [`history`] for the right run-history panel.

use std::{
    sync::{Arc, atomic::Ordering, mpsc::TryRecvError},
    time::Duration,
};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use serde::{Deserialize, Serialize};

use crate::ga::{self, EpochSnapshot};

mod board;
mod charts;
mod config;
mod controls;
mod history;
mod palette;
mod top_bar;
mod worker;

use board::{BoardView, draw_board, draw_empty_board};
use config::GuiConfig;
use history::RunArchive;
use worker::{MetricHistory, RunResult, RunningRun, WorkerMessage, spawn_solver};

/// Opens the native solver interface.
///
/// # Errors
/// Returns an eframe error if the window or graphics backend cannot initialize or run.
pub fn run() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([1000.0, 680.0]),
        ..Default::default()
    };

    eframe::run_native(
        "N-Queens Genetic Solver",
        native_options,
        Box::new(|cc| Ok(Box::new(NQueensApp::new(cc)))),
    )
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
    notice: Option<Notice>,
    /// The seed as typed; it may be invalid or mid-edit, unlike `config.seed`.
    seed_text: String,
    board_view: BoardView,
    error: Option<String>,
    log_conflicts: bool,
    /// Bumped per started run so live charts drop the previous run's zoom.
    chart_generation: u64,
    chart_view: ChartView,
    show_history: bool,
}

/// Settings restored on the next launch.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct SavedSettings {
    config: GuiConfig,
    log_conflicts: bool,
    show_history: bool,
}

impl Default for SavedSettings {
    fn default() -> Self {
        Self {
            config: GuiConfig::default(),
            log_conflicts: true,
            show_history: true,
        }
    }
}

/// Short-lived feedback for an action, shown in the top bar.
struct Notice {
    text: String,
    is_error: bool,
    /// In [`egui::InputState::time`] seconds.
    expires_at: f64,
}

const NOTICE_SECONDS: f64 = 4.0;
/// Errors stay longer so there is time to read them.
const ERROR_NOTICE_SECONDS: f64 = 12.0;

const RUN_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter);
const CANCEL_SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Escape);

/// What the bottom chart panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChartView {
    Metrics,
    Comparison,
}

impl NQueensApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let saved = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, eframe::APP_KEY))
            .unwrap_or_default();
        Self::with_settings(saved)
    }

    fn with_settings(saved: SavedSettings) -> Self {
        let SavedSettings {
            mut config,
            log_conflicts,
            show_history,
        } = saved;
        // A hand-edited or older save can hold values the controls would never produce.
        config.normalize();
        // Field by field: `Drop` rules out struct update syntax.
        let mut app = Self::default();
        app.seed_text = config.seed.to_string();
        app.config = config;
        app.log_conflicts = log_conflicts;
        app.show_history = show_history;
        app
    }

    fn saved_settings(&self) -> SavedSettings {
        SavedSettings {
            config: self.config.clone(),
            log_conflicts: self.log_conflicts,
            show_history: self.show_history,
        }
    }

    fn notify(&mut self, ctx: &egui::Context, text: String, is_error: bool) {
        let seconds = if is_error {
            ERROR_NOTICE_SECONDS
        } else {
            NOTICE_SECONDS
        };
        self.notice = Some(Notice {
            text,
            is_error,
            expires_at: ctx.input(|input| input.time) + seconds,
        });
        // Panels drawn earlier this frame (the top bar) only show it on the next frame.
        ctx.request_repaint();
    }

    /// Runs after the panels so a focused board can claim Escape for its selection first.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.running.is_none() {
            if ctx.input_mut(|input| input.consume_shortcut(&RUN_SHORTCUT)) {
                self.start_run(ctx);
            }
        } else if ctx.input_mut(|input| input.consume_shortcut(&CANCEL_SHORTCUT)) {
            self.cancel_run();
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
        self.chart_generation += 1;
        self.chart_view = ChartView::Metrics;

        let (receiver, cancel) = spawn_solver(self.config.clone());
        self.running = Some(RunningRun {
            receiver,
            cancel,
            max_epochs: self.config.max_epochs,
        });
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

    fn draw_main_panel(&mut self, ui: &mut egui::Ui) {
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
                ui.heading("Board");
                draw_empty_board(ui);
            }
        });

        self.selected_queen = selected_queen;
        self.board_view = board_view;
    }

    fn draw_chart_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.chart_view, ChartView::Metrics, "Run metrics")
                .on_hover_text("Attacking pairs and population ratios of the run on the board");
            ui.selectable_value(&mut self.chart_view, ChartView::Comparison, "Compare A/B")
                .on_hover_text("Best-board attacking pairs of the two runs marked A and B in run history");
            ui.separator();
            ui.checkbox(&mut self.log_conflicts, "Log scale")
                .on_hover_text("Plot attacking pairs on a log scale so small best-board counts stay visible next to the population average.");
            ui.separator();
            ui.weak("Chart tips").on_hover_text(
                "Drag to pan · Ctrl/Cmd + scroll to zoom · double-click to reset · click legend entries to hide them",
            );
        });
        match self.chart_view {
            ChartView::Metrics => {
                let history = self
                    .archive
                    .viewed
                    .and_then(|id| self.archive.get(id))
                    .map_or(&self.history, |run| &run.result.history);
                charts::draw_metric_charts(
                    ui,
                    history,
                    (self.archive.viewed, self.chart_generation),
                    self.log_conflicts,
                );
            }
            ChartView::Comparison => self.draw_comparison(ui),
        }
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

impl Default for NQueensApp {
    fn default() -> Self {
        Self {
            config: GuiConfig::default(),
            running: None,
            cancel_requested: false,
            history: MetricHistory::default(),
            latest_snapshot: None,
            selected_queen: None,
            result: None,
            archive: RunArchive::default(),
            notice: None,
            seed_text: GuiConfig::default().seed.to_string(),
            board_view: BoardView::default(),
            error: None,
            log_conflicts: true,
            chart_generation: 0,
            chart_view: ChartView::Metrics,
            show_history: true,
        }
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
            ui.add_space(4.0);
            self.draw_top_bar(ui, &ctx);
            ui.add_space(4.0);
        });

        egui::Panel::left("controls")
            .resizable(false)
            .default_size(315.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.draw_controls(ui);
                });
            });

        let mut show_history = self.show_history;
        egui::Panel::right("run_history")
            .resizable(true)
            .default_size(300.0)
            .size_range(240.0..=520.0)
            .show_collapsible(ui, &mut show_history, |ui| {
                // Without auto-shrink the panel keeps its width instead of hugging the content.
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| self.draw_run_history(ui, &ctx));
            });
        self.show_history = show_history;

        egui::Panel::bottom("charts")
            .resizable(true)
            .default_size(400.0)
            .size_range(CHART_PANEL_MIN_HEIGHT..=900.0)
            .show(ui, |ui| {
                self.draw_chart_panel(ui);
                // Resizable panels shrink to their content, so claim the rest of the height.
                ui.allocate_space(ui.available_size());
            });

        egui::CentralPanel::default().show(ui, |ui| self.draw_main_panel(ui));
        self.handle_shortcuts(&ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.saved_settings());
    }
}

/// Room for the tab row and two charts at their minimum plot height.
const CHART_PANEL_MIN_HEIGHT: f32 = 300.0;

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
    use std::sync::{atomic::AtomicBool, mpsc};

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
    fn counts_use_one_term_with_correct_plurals() {
        assert_eq!(attacking_pairs(0), "0 attacking pairs");
        assert_eq!(attacking_pairs(1), "1 attacking pair");
        assert_eq!(attacking_pairs(2), "2 attacking pairs");
    }

    #[derive(Default)]
    struct MemoryStorage(std::collections::HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }

        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn settings_survive_a_restart_and_bad_saves_are_repaired() {
        let mut app = NQueensApp::default();
        app.config = config::GuiPreset::Classic.config(987);
        app.log_conflicts = false;
        app.show_history = false;
        let mut storage = MemoryStorage::default();
        eframe::App::save(&mut app, &mut storage);

        let saved = eframe::get_value(&storage, eframe::APP_KEY).unwrap();
        let restored = NQueensApp::with_settings(saved);
        assert!(restored.config == app.config);
        assert_eq!(restored.seed_text, "987");
        assert!(!restored.log_conflicts);
        assert!(!restored.show_history);

        // Missing fields fall back to defaults; out-of-range values are clamped.
        eframe::Storage::set_string(
            &mut storage,
            eframe::APP_KEY,
            "(config: (board_size: 0, mutation_rate: 7.0, selection_strategy: \"roulette\"))"
                .to_owned(),
        );
        let saved: SavedSettings = eframe::get_value(&storage, eframe::APP_KEY).unwrap();
        let repaired = NQueensApp::with_settings(saved);
        assert_eq!(repaired.config.board_size, 1);
        assert!((repaired.config.mutation_rate - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            repaired.config.selection_strategy,
            ga::SelectionStrategy::Roulette
        );
        assert_eq!(repaired.config.seed, GuiConfig::default().seed);
        assert!(repaired.log_conflicts);
    }

    fn frame_at(ctx: &egui::Context, time: f64, events: Vec<egui::Event>, app: &mut NQueensApp) {
        let input = egui::RawInput {
            time: Some(time),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            app.draw_top_bar(ui, &ui.ctx().clone());
            app.handle_shortcuts(ui.ctx());
        })
        .drop_without_applying_deltas();
    }

    fn key(key: Key, modifiers: Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn notices_expire_and_errors_stay_longer() {
        let ctx = egui::Context::default();
        let mut app = NQueensApp::default();
        frame_at(&ctx, 0.0, Vec::new(), &mut app);
        app.notify(&ctx, "Copied".into(), false);
        frame_at(&ctx, NOTICE_SECONDS - 0.5, Vec::new(), &mut app);
        assert!(app.notice.is_some());
        frame_at(&ctx, NOTICE_SECONDS + 0.5, Vec::new(), &mut app);
        assert!(app.notice.is_none());

        app.notify(&ctx, "Could not export".into(), true);
        frame_at(&ctx, 2.0 * NOTICE_SECONDS + 1.0, Vec::new(), &mut app);
        assert!(app.notice.is_some());
        frame_at(
            &ctx,
            NOTICE_SECONDS + ERROR_NOTICE_SECONDS + 1.0,
            Vec::new(),
            &mut app,
        );
        assert!(app.notice.is_none());
    }

    #[test]
    fn keyboard_shortcuts_start_and_cancel_runs() {
        let ctx = egui::Context::default();
        let mut app = NQueensApp::default();
        app.config = config::GuiConfig {
            board_size: 3,
            population_size: 4,
            max_epochs: 100_000,
            allow_unsolvable: true,
            ..Default::default()
        };
        frame_at(&ctx, 0.0, vec![key(Key::Escape, Modifiers::NONE)], &mut app);
        assert!(app.running.is_none());
        frame_at(
            &ctx,
            0.1,
            vec![key(Key::Enter, Modifiers::COMMAND)],
            &mut app,
        );
        assert!(app.running.is_some());
        frame_at(&ctx, 0.2, vec![key(Key::Escape, Modifiers::NONE)], &mut app);
        assert!(app.cancel_requested);
    }

    #[test]
    fn disconnected_worker_clears_running_state() {
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(sender);
        let mut app = NQueensApp::default();
        app.running = Some(RunningRun {
            receiver,
            cancel: Arc::new(AtomicBool::new(false)),
            max_epochs: 1,
        });
        app.drain_worker_messages(&egui::Context::default());
        assert!(app.running.is_none());
        assert!(app.error.as_ref().unwrap().contains("disconnected"));
    }
}
