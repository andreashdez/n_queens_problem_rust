//! The GUI's state and every action on it, apart from any toolkit. A toolkit
//! draws from this state, calls these actions on input, and supplies the time
//! in seconds for notices.

use std::{
    fmt::Write as _,
    sync::{Arc, mpsc::TryRecvError},
};

use rand::RngExt as _;
use serde::{Deserialize, Serialize};

use super::{
    Step, attacking_pairs,
    board::BoardView,
    charts::plot_value,
    config::{GuiConfig, GuiPreset},
    format_ms,
    history::RunArchive,
    palette::Tone,
    worker::{MetricHistory, Progress, RunResult, RunningRun, Snapshot, WorkerMessage},
};
use crate::{Solver, ga};

pub(super) const NOTICE_SECONDS: f64 = 4.0;
/// Errors stay longer so there is time to read them.
pub(super) const ERROR_NOTICE_SECONDS: f64 = 12.0;

pub(super) const SEED_HELP: &str = "Use the same seed and settings to reproduce a run.";
pub(super) const INVALID_SEED_HELP: &str = "Enter a whole number from 0 to 18446744073709551615. Leaving the field keeps the last valid seed.";

/// Settings restored on the next launch.
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub(super) struct SavedSettings {
    pub config: GuiConfig,
    pub log_conflicts: bool,
    pub show_history: bool,
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
pub(super) struct Notice {
    pub text: String,
    pub is_error: bool,
    /// On the clock the toolkit passes to [`AppState::notify`], in seconds.
    pub expires_at: f64,
}

/// What the bottom chart panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ChartView {
    Metrics,
    Comparison,
}

/// The board on display: queen rows by column, attackers per queen, and attacking pairs.
pub(super) type BoardState<'a> = (&'a [u16], &'a [u32], u32);

/// The metrics panel under the run settings.
pub(super) struct MetricsPanel {
    pub title: &'static str,
    /// Label and value; `None` before the first progress report.
    pub rows: Option<Vec<(&'static str, String)>>,
    /// Totals of a finished run.
    pub footer: Vec<String>,
}

pub(super) struct AppState {
    pub config: GuiConfig,
    pub running: Option<RunningRun>,
    pub cancel_requested: bool,
    pub history: MetricHistory,
    pub latest_snapshot: Option<Snapshot>,
    pub selected_queen: Option<usize>,
    pub result: Option<Arc<RunResult>>,
    pub archive: RunArchive,
    pub notice: Option<Notice>,
    /// The seed as typed; it may be invalid or mid-edit, unlike `config.seed`.
    pub seed_text: String,
    pub board_view: BoardView,
    pub error: Option<String>,
    pub log_conflicts: bool,
    /// Bumped per started run so live charts drop the previous run's zoom.
    pub chart_generation: u64,
    pub chart_view: ChartView,
    pub show_history: bool,
}

impl Default for AppState {
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

impl Drop for AppState {
    fn drop(&mut self) {
        if let Some(running) = &mut self.running {
            running.worker.cancel();
        }
    }
}

impl AppState {
    pub(super) fn with_settings(saved: SavedSettings) -> Self {
        let SavedSettings {
            mut config,
            log_conflicts,
            show_history,
        } = saved;
        // A hand-edited or older save can hold values the controls would never produce.
        config.normalize();
        // Field by field: `Drop` rules out struct update syntax.
        let mut state = Self::default();
        state.seed_text = config.seed.to_string();
        state.config = config;
        state.log_conflicts = log_conflicts;
        state.show_history = show_history;
        state
    }

    pub(super) fn saved_settings(&self) -> SavedSettings {
        SavedSettings {
            config: self.config.clone(),
            log_conflicts: self.log_conflicts,
            show_history: self.show_history,
        }
    }

    /// Shows `text` until a while after `now`, longer for errors.
    pub(super) fn notify(&mut self, text: String, is_error: bool, now: f64) {
        let seconds = if is_error {
            ERROR_NOTICE_SECONDS
        } else {
            NOTICE_SECONDS
        };
        self.notice = Some(Notice {
            text,
            is_error,
            expires_at: now + seconds,
        });
    }

    /// The notice to show at `now`, dropping it once it has expired.
    pub(super) fn current_notice(&mut self, now: f64) -> Option<&Notice> {
        if self
            .notice
            .as_ref()
            .is_some_and(|notice| notice.expires_at <= now)
        {
            self.notice = None;
        }
        self.notice.as_ref()
    }

    pub(super) const fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Starts a run with the current settings, unless one is running already.
    pub(super) fn start_run(&mut self) {
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

        self.running = Some(RunningRun::start(self.config.clone()));
    }

    /// Asks a running solver to stop after its current epoch or pass.
    pub(super) fn cancel_run(&mut self) {
        if let Some(running) = &mut self.running {
            running.worker.cancel();
            self.cancel_requested = true;
        }
    }

    /// Takes the worker's latest report and its result, if it has finished.
    /// Call it every [`super::worker::POLL_INTERVAL`] while a run is running.
    pub(super) fn drain_worker(&mut self) {
        let mut finished = false;
        if let Some(running) = &mut self.running {
            running.worker.advance();
            // At most one pending snapshot and one terminal message per frame.
            for _ in 0..2 {
                match running.worker.try_recv() {
                    Ok(WorkerMessage::Snapshot(snapshot)) => {
                        self.history.push(snapshot.progress.clone(), false);
                        self.latest_snapshot = Some(snapshot);
                    }
                    Ok(WorkerMessage::Finished(result)) => {
                        self.history = result.history.clone();
                        let previous_view = self.archive.viewed;
                        self.archive.record(Arc::clone(&result));
                        if previous_view != self.archive.viewed {
                            self.reset_board_view();
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
        }
    }

    /// Runs a saved run's settings again. False if a run is running or the run is gone.
    pub(super) fn rerun_saved(&mut self, id: u64) -> bool {
        if self.running.is_some() {
            return false;
        }
        let Some(run) = self.archive.get(id) else {
            return false;
        };
        self.config = run.result.config.clone();
        self.start_run();
        true
    }

    /// Marks or unmarks run `id` as comparison A (slot 0) or B (slot 1).
    /// Completing a pair brings the comparison chart into view.
    pub(super) fn toggle_comparison(&mut self, slot: usize, id: u64) {
        self.archive.toggle_comparison(slot, id);
        if self.archive.compare.iter().all(Option::is_some) {
            self.chart_view = ChartView::Comparison;
        }
    }

    pub(super) fn view_saved_run(&mut self, id: u64) {
        self.archive.viewed = Some(id);
        self.reset_board_view();
        self.chart_view = ChartView::Metrics;
    }

    pub(super) fn view_current_run(&mut self) {
        self.archive.viewed = None;
        self.reset_board_view();
    }

    fn reset_board_view(&mut self) {
        self.selected_queen = None;
        self.board_view = BoardView::default();
    }

    /// Saves run `id`'s metrics as CSV and reports how that went.
    pub(super) fn export_run(&mut self, id: u64, now: f64) {
        let Some(outcome) = self
            .archive
            .get(id)
            .and_then(super::history::SavedRun::export)
        else {
            return;
        };
        let (text, is_error) = outcome;
        self.notify(text, is_error, now);
    }

    /// Run `id`'s settings as a CLI command, for the clipboard.
    pub(super) fn cli_command(&self, id: u64) -> Option<String> {
        self.archive
            .get(id)
            .map(|run| run.result.config.cli_command())
    }

    /// Confirms that the toolkit copied run `id`'s command.
    pub(super) fn command_copied(&mut self, id: u64, now: f64) {
        self.notify(format!("Copied command for run #{id}"), false, now);
    }

    pub(super) fn choose_preset(&mut self, preset: GuiPreset) {
        self.config = preset.config(self.config.seed);
    }

    pub(super) fn randomize_seed(&mut self) {
        self.config.seed = rand::rng().random::<u64>();
        self.seed_text = self.config.seed.to_string();
    }

    /// Shows seeds set elsewhere (presets, reruns, saved settings) in the seed
    /// field. Call it while the field is not being edited.
    pub(super) fn sync_seed_text(&mut self) {
        if self.seed_text.parse() != Ok(self.config.seed) {
            self.seed_text = self.config.seed.to_string();
        }
    }

    /// Call after the seed field changes `seed_text`: keeps only digits, and
    /// takes the seed once it is valid.
    pub(super) fn seed_text_edited(&mut self) {
        self.seed_text
            .retain(|character| character.is_ascii_digit());
        if let Ok(seed) = self.seed_text.parse() {
            self.config.seed = seed;
        }
    }

    pub(super) fn seed_text_is_valid(&self) -> bool {
        self.seed_text.parse::<u64>().is_ok()
    }

    /// The search budget under the basic settings.
    pub(super) fn budget_summary(&self) -> String {
        match self.config.solver {
            Solver::Ga => format!(
                "{} individuals · up to {} epochs",
                self.config.population_size, self.config.max_epochs
            ),
            Solver::MinConflicts => {
                format!("One board · up to {} repair passes", self.config.max_epochs)
            }
        }
    }

    /// The finished run on display: a viewed saved run, or else the last result.
    pub(super) fn display_result(&self) -> Option<&RunResult> {
        self.archive
            .viewed
            .and_then(|id| self.archive.get(id))
            .map(|run| run.result.as_ref())
            .or(self.result.as_deref())
    }

    /// The metrics the charts plot: a viewed saved run's, or the current run's.
    pub(super) fn chart_history(&self) -> &MetricHistory {
        self.archive
            .viewed
            .and_then(|id| self.archive.get(id))
            .map_or(&self.history, |run| &run.result.history)
    }

    pub(super) fn current_board(&self) -> Option<BoardState<'_>> {
        if let Some(result) = self.display_result() {
            return Some((
                result.best_positions.as_slice(),
                result.best_conflicts.as_slice(),
                result.best_conflicts_sum,
            ));
        }

        self.latest_snapshot.as_ref().map(|snapshot| {
            (
                snapshot.best_positions.as_slice(),
                snapshot.best_conflicts.as_slice(),
                snapshot.best_conflicts_sum,
            )
        })
    }

    pub(super) fn current_metrics(&self) -> Option<(&Progress, u32)> {
        if let Some(result) = self.display_result() {
            return result
                .final_progress
                .as_ref()
                .map(|progress| (progress, result.best_conflicts_sum));
        }

        self.latest_snapshot
            .as_ref()
            .map(|snapshot| (&snapshot.progress, snapshot.best_conflicts_sum))
    }

    pub(super) fn metrics_panel(&self) -> MetricsPanel {
        let title = if self.archive.viewed.is_some() {
            "Viewed run"
        } else {
            "Current run"
        };
        let Some((progress, best_conflicts_sum)) = self.current_metrics() else {
            return MetricsPanel {
                title,
                rows: None,
                footer: Vec::new(),
            };
        };
        let rows = match progress {
            Progress::Ga(metrics) => vec![
                ("Epoch", metrics.epoch().to_string()),
                ("Best attacking pairs", best_conflicts_sum.to_string()),
                (
                    "Avg attacking pairs",
                    format!("{:.2}", metrics.average_conflicts_sum()),
                ),
                (
                    "Diversity",
                    format!("{:.1}%", metrics.diversity_ratio() * 100.0),
                ),
                ("Mutation", format_ratio(metrics.mutation_rate())),
                ("Elite", format_ratio(metrics.elite_ratio())),
                ("Population", metrics.population_size().to_string()),
                ("Elapsed", format_ms(metrics.elapsed_ms())),
            ],
            Progress::MinConflicts(metrics) => vec![
                ("Pass", metrics.pass().to_string()),
                ("Best attacking pairs", best_conflicts_sum.to_string()),
                ("Current board", metrics.current_conflicts_sum().to_string()),
                ("Swaps", metrics.swaps().to_string()),
                ("Restarts", metrics.restarts().to_string()),
                ("Elapsed", format_ms(metrics.elapsed_ms())),
            ],
        };
        let mut footer = Vec::new();
        if let Some(result) = self.display_result() {
            if let Some(population_size) = result.population_size {
                footer.push(format!("Final population: {population_size}"));
            }
            footer.push(format!(
                "Total elapsed: {}",
                format_ms(result.total_elapsed_ms)
            ));
        }
        MetricsPanel {
            title,
            rows: Some(rows),
            footer,
        }
    }

    /// Top-bar status, toned by outcome.
    pub(super) fn status(&self) -> (String, Option<Tone>) {
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

    /// The running run's progress bar: share of the budget used and its label,
    /// and the unit the solver steps in.
    pub(super) fn run_progress(&self) -> Option<(f32, String, Step)> {
        let running = self.running.as_ref()?;
        let step = Step::of(running.solver);
        let (fraction, text) = run_progress(
            self.latest_snapshot
                .as_ref()
                .map(|snapshot| &snapshot.progress),
            running.max_epochs,
            step,
        );
        Some((fraction, text, step))
    }
}

fn format_ratio(value: f32) -> String {
    format!("{:.1}%", value * 100.0)
}

pub(super) fn run_outcome(result: &RunResult) -> (String, Tone) {
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
    use std::sync::{atomic::AtomicBool, mpsc};

    use super::*;
    use crate::gui::{
        board::Point,
        worker::{self, tests::finished_run},
    };

    #[test]
    fn settings_survive_a_restart_and_bad_saves_are_repaired() {
        let mut state = AppState::default();
        state.config = GuiPreset::Classic.config(987);
        state.log_conflicts = false;
        state.show_history = false;
        let saved = serde_json::to_string(&state.saved_settings()).unwrap();

        let restored = AppState::with_settings(serde_json::from_str(&saved).unwrap());
        assert!(restored.config == state.config);
        assert_eq!(restored.seed_text, "987");
        assert!(!restored.log_conflicts);
        assert!(!restored.show_history);

        // Missing fields fall back to defaults; out-of-range values are clamped.
        let saved = r#"{"config": {"board_size": 0, "mutation_rate": 7.0, "selection_strategy": "roulette"}}"#;
        let repaired = AppState::with_settings(serde_json::from_str(saved).unwrap());
        assert_eq!(repaired.config.board_size, 1);
        assert!((repaired.config.mutation_rate - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            repaired.config.selection_strategy,
            ga::SelectionStrategy::Roulette
        );
        assert_eq!(repaired.config.seed, GuiConfig::default().seed);
        assert!(repaired.log_conflicts);
    }

    #[test]
    fn notices_expire_and_errors_stay_longer() {
        let mut state = AppState::default();
        state.notify("Copied".into(), false, 0.0);
        assert!(state.current_notice(NOTICE_SECONDS - 0.5).is_some());
        assert!(state.current_notice(NOTICE_SECONDS + 0.5).is_none());

        state.notify("Could not export".into(), true, NOTICE_SECONDS + 0.5);
        let notice = state
            .current_notice(NOTICE_SECONDS.mul_add(2.0, 1.0))
            .unwrap();
        assert!(notice.is_error);
        assert!(
            state
                .current_notice(NOTICE_SECONDS + ERROR_NOTICE_SECONDS + 1.0)
                .is_none()
        );
    }

    #[test]
    fn runs_start_once_and_cancel() {
        let mut state = AppState::default();
        state.config = GuiConfig {
            board_size: 3,
            population_size: 4,
            max_epochs: 100_000,
            allow_unsolvable: true,
            ..Default::default()
        };
        state.cancel_run();
        assert!(!state.cancel_requested);
        state.start_run();
        assert!(state.is_running());
        let generation = state.chart_generation;
        state.start_run();
        assert_eq!(state.chart_generation, generation);
        state.cancel_run();
        assert!(state.cancel_requested);
        assert_eq!(state.status().0, "Cancelling after current epoch");
        while state.is_running() {
            state.drain_worker();
        }
        assert!(!state.cancel_requested);
        assert_eq!(state.archive.len(), 1);
        assert!(state.status().0.starts_with("Cancelled · best board has"));
    }

    #[test]
    fn disconnected_worker_clears_running_state() {
        let (sender, receiver) = mpsc::sync_channel(1);
        drop(sender);
        let mut state = AppState::default();
        state.running = Some(RunningRun {
            worker: worker::Worker {
                receiver,
                cancel: Arc::new(AtomicBool::new(false)),
            },
            max_epochs: 1,
            solver: Solver::Ga,
        });
        state.drain_worker();
        assert!(state.running.is_none());
        assert!(state.error.as_ref().unwrap().contains("disconnected"));
    }

    #[test]
    fn saved_rerun_uses_original_configuration_and_preserves_history() {
        let original = GuiPreset::Hybrid.config(987);
        let result = finished_run(original.clone());
        let mut state = AppState::default();
        let id = state.archive.record(result);
        state.config.seed = 1;
        state.archive.viewed = Some(id);
        state.selected_queen = Some(0);
        state.board_view.set_zoom(8.0, Point::new(0.5, 0.5));
        assert!(state.rerun_saved(id));
        assert!(state.config == original);
        assert_eq!(state.archive.len(), 1);
        assert_eq!(state.archive.viewed, None);
        assert_eq!(state.selected_queen, None);
        assert_eq!(state.board_view, BoardView::default());
        assert!(!state.rerun_saved(id));
        assert_eq!(state.cli_command(id), Some(original.cli_command()));
        assert_eq!(state.cli_command(id + 1), None);
    }

    #[test]
    fn completing_a_comparison_pair_shows_it() {
        let result = finished_run(GuiConfig::default());
        let mut state = AppState::default();
        let a = state.archive.record(Arc::clone(&result));
        let b = state.archive.record(result);
        state.toggle_comparison(0, a);
        assert_eq!(state.chart_view, ChartView::Metrics);
        state.toggle_comparison(1, b);
        assert_eq!(state.chart_view, ChartView::Comparison);
        state.view_saved_run(a);
        assert_eq!(state.chart_view, ChartView::Metrics);
        assert_eq!(state.metrics_panel().title, "Viewed run");
        state.view_current_run();
        assert_eq!(state.archive.viewed, None);
    }

    #[test]
    fn seed_field_follows_seeds_set_elsewhere() {
        let mut state = AppState::default();
        state.config.seed = u64::MAX;
        state.sync_seed_text();
        assert_eq!(state.seed_text, u64::MAX.to_string());
        // An abandoned invalid edit falls back to the last valid seed.
        state.seed_text = "18446744073709551616".into();
        state.seed_text_edited();
        assert!(!state.seed_text_is_valid());
        assert_eq!(state.config.seed, u64::MAX);
        state.sync_seed_text();
        assert_eq!(state.seed_text, u64::MAX.to_string());
        // Typed characters other than digits are dropped.
        state.seed_text = "12a3".into();
        state.seed_text_edited();
        assert_eq!(state.seed_text, "123");
        assert_eq!(state.config.seed, 123);
    }

    #[test]
    fn presets_keep_the_seed() {
        let mut state = AppState::default();
        state.config.seed = 7;
        state.choose_preset(GuiPreset::Classic);
        assert!(state.config == GuiPreset::Classic.config(7));
        assert_eq!(state.budget_summary(), {
            let config = GuiPreset::Classic.config(7);
            format!(
                "{} individuals · up to {} epochs",
                config.population_size, config.max_epochs
            )
        });
        state.config.solver = Solver::MinConflicts;
        assert!(state.budget_summary().starts_with("One board"));
    }

    #[test]
    fn metrics_panel_lists_the_solver_metrics() {
        let mut state = AppState::default();
        let panel = state.metrics_panel();
        assert_eq!(panel.title, "Current run");
        assert!(panel.rows.is_none());
        state.result = Some(finished_run(GuiConfig {
            board_size: 1,
            population_size: 1,
            ..Default::default()
        }));
        let panel = state.metrics_panel();
        let labels: Vec<_> = panel.rows.unwrap().iter().map(|row| row.0).collect();
        assert_eq!(labels[..2], ["Epoch", "Best attacking pairs"]);
        assert_eq!(panel.footer.len(), 2);
        assert!(panel.footer[0].starts_with("Final population: "));
        state.result = Some(finished_run(GuiConfig {
            solver: Solver::MinConflicts,
            board_size: 8,
            ..Default::default()
        }));
        let panel = state.metrics_panel();
        assert_eq!(panel.rows.unwrap()[0].0, "Pass");
        assert_eq!(panel.footer.len(), 1);
    }

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
        assert!(AppState::default().run_progress().is_none());
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

        let mut state = AppState::default();
        assert_eq!(state.status(), ("Ready".to_owned(), None));
        state.error = Some("boom".into());
        assert_eq!(
            state.status(),
            ("Error: boom".to_owned(), Some(Tone::Error))
        );
        let id = state.archive.record(solved);
        state.archive.viewed = Some(id);
        assert_eq!(
            state.status(),
            (
                format!("Run #{id} · Solved at epoch 0"),
                Some(Tone::Success)
            )
        );
    }
}
