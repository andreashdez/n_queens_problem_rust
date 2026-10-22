//! The background solver thread and the bounded history it reports.

use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
};

use super::config::GuiConfig;
use crate::{
    Solver,
    ga::{self, EpochSnapshot},
    min_conflicts::{self, PassMetrics, PassSnapshot},
};

pub(super) struct RunningRun {
    pub(super) receiver: Receiver<WorkerMessage>,
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) max_epochs: u32,
    pub(super) solver: Solver,
}

pub(super) enum WorkerMessage {
    Snapshot(Snapshot),
    Finished(Arc<RunResult>),
    Failed(String),
}

/// One progress report: a GA epoch or a min-conflicts repair pass.
#[derive(Clone)]
pub(super) enum Progress {
    Ga(ga::EpochMetrics),
    MinConflicts(PassMetrics),
}

impl Progress {
    /// The GA epoch or min-conflicts pass this report describes.
    pub(super) const fn step(&self) -> u32 {
        match self {
            Self::Ga(metrics) => metrics.epoch(),
            Self::MinConflicts(metrics) => metrics.pass(),
        }
    }

    pub(super) const fn best_conflicts_sum(&self) -> u32 {
        match self {
            Self::Ga(metrics) => metrics.best_conflicts_sum(),
            Self::MinConflicts(metrics) => metrics.best_conflicts_sum(),
        }
    }

    pub(super) const fn elapsed_ms(&self) -> u128 {
        match self {
            Self::Ga(metrics) => metrics.elapsed_ms(),
            Self::MinConflicts(metrics) => metrics.elapsed_ms(),
        }
    }

    /// GA soft restarts or min-conflicts restarts completed so far.
    pub(super) const fn restart_count(&self) -> u32 {
        match self {
            Self::Ga(metrics) => metrics.restart_count(),
            Self::MinConflicts(metrics) => metrics.restarts(),
        }
    }

    pub(super) const fn last_restart_step(&self) -> Option<u32> {
        match self {
            Self::Ga(metrics) => metrics.last_restart_epoch(),
            Self::MinConflicts(metrics) => metrics.last_restart_pass(),
        }
    }
}

/// The best board so far, with the progress report it belongs to.
#[derive(Clone)]
pub(super) struct Snapshot {
    pub(super) progress: Progress,
    pub(super) best_positions: Vec<u16>,
    pub(super) best_conflicts: Vec<u32>,
    pub(super) best_conflicts_sum: u32,
}

impl From<&EpochSnapshot> for Snapshot {
    fn from(snapshot: &EpochSnapshot) -> Self {
        Self {
            progress: Progress::Ga(snapshot.metrics().clone()),
            best_positions: snapshot.best_positions().to_vec(),
            best_conflicts: snapshot.best_conflicts().to_vec(),
            best_conflicts_sum: snapshot.best_conflicts_sum(),
        }
    }
}

impl From<&PassSnapshot> for Snapshot {
    fn from(snapshot: &PassSnapshot) -> Self {
        let best = snapshot.best_chromosome();
        Self {
            progress: Progress::MinConflicts(*snapshot.metrics()),
            best_positions: best.get_positions().to_vec(),
            best_conflicts: best.get_conflicts().to_vec(),
            best_conflicts_sum: best.get_conflicts_sum(),
        }
    }
}

pub(super) struct RunResult {
    pub(super) config: GuiConfig,
    pub(super) history: MetricHistory,
    pub(super) stop_reason: ga::StopReason,
    /// The epoch or pass that found the solution.
    pub(super) solved_step: Option<u32>,
    pub(super) total_elapsed_ms: u128,
    /// The last progress report, shown as the run's final metrics.
    pub(super) final_progress: Option<Progress>,
    pub(super) best_positions: Vec<u16>,
    pub(super) best_conflicts: Vec<u32>,
    pub(super) best_conflicts_sum: u32,
    /// Only the GA has a population.
    pub(super) population_size: Option<usize>,
}

pub(super) const MAX_CHART_POINTS: usize = 1024;

#[derive(Clone)]
pub(super) struct MetricHistory {
    pub(super) restart_epochs: VecDeque<u32>,
    pub(super) points: Vec<Progress>,
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
    pub(super) fn push(&mut self, progress: Progress, force: bool) {
        if let Some(step) = progress.last_restart_step()
            && self.restart_epochs.back().is_none_or(|&last| last < step)
        {
            if self.restart_epochs.len() == MAX_CHART_POINTS {
                self.restart_epochs.pop_front();
            }
            self.restart_epochs.push_back(step);
        }
        if self
            .points
            .last()
            .is_some_and(|last| last.step() == progress.step())
        {
            return;
        }
        if !force && !u64::from(progress.step()).is_multiple_of(self.stride) {
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
        self.points.push(progress);
    }
}

pub(super) fn spawn_solver(config: GuiConfig) -> (Receiver<WorkerMessage>, Arc<AtomicBool>) {
    let (sender, receiver) = mpsc::sync_channel(1);
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = Arc::clone(&cancel);

    thread::spawn(move || {
        let outcome = match config.solver {
            Solver::Ga => run_ga(config, &sender, &cancel_worker),
            Solver::MinConflicts => run_min_conflicts(config, &sender, &cancel_worker),
        };
        let _ = sender.send(match outcome {
            Ok(result) => WorkerMessage::Finished(Arc::new(result)),
            Err(error) => WorkerMessage::Failed(error),
        });
    });

    (receiver, cancel)
}

/// Records a report and offers it to the GUI; false asks the solver to stop.
fn report(
    history: &mut MetricHistory,
    progress: Progress,
    snapshot: impl FnOnce() -> Snapshot,
    sender: &SyncSender<WorkerMessage>,
    cancel: &AtomicBool,
) -> bool {
    history.push(progress, false);
    if cancel.load(Ordering::Relaxed) {
        return false;
    }

    let connected = match sender.try_send(WorkerMessage::Snapshot(snapshot())) {
        Ok(()) | Err(TrySendError::Full(_)) => true,
        Err(TrySendError::Disconnected(_)) => false,
    };
    connected && !cancel.load(Ordering::Relaxed)
}

fn run_ga(
    config: GuiConfig,
    sender: &SyncSender<WorkerMessage>,
    cancel: &AtomicBool,
) -> Result<RunResult, String> {
    let invalid = |error: ga::GaConfigError| format!("Invalid GA config: {error}");
    let ga_config = config.to_ga_config().map_err(invalid)?;
    let mut algorithm = ga::build_genetic_algorithm(ga_config).map_err(invalid)?;
    let mut history = MetricHistory::default();
    let run_metrics = algorithm.run_algorithm_with_options(
        ga::RunOptions {
            allow_unsolvable: config.allow_unsolvable,
            collect_history: false,
            profile: false,
        },
        |snapshot| {
            report(
                &mut history,
                Progress::Ga(snapshot.metrics().clone()),
                || Snapshot::from(snapshot),
                sender,
                cancel,
            )
        },
    );

    let final_progress = run_metrics.epochs().last().cloned().map(Progress::Ga);
    if let Some(progress) = &final_progress {
        history.push(progress.clone(), true);
    }
    let best_chromosome = algorithm.get_best_chromosome();
    Ok(RunResult {
        config,
        history,
        stop_reason: run_metrics.stop_reason(),
        solved_step: run_metrics.solved_epoch(),
        total_elapsed_ms: run_metrics.total_elapsed_ms(),
        final_progress,
        best_positions: best_chromosome.get_positions().to_vec(),
        best_conflicts: best_chromosome.get_conflicts().to_vec(),
        best_conflicts_sum: best_chromosome.get_conflicts_sum(),
        population_size: Some(algorithm.get_population_size()),
    })
}

fn run_min_conflicts(
    config: GuiConfig,
    sender: &SyncSender<WorkerMessage>,
    cancel: &AtomicBool,
) -> Result<RunResult, String> {
    let mut history = MetricHistory::default();
    let mut final_progress = None;
    let result = min_conflicts::solve_with_progress(config.to_min_conflicts_config(), |snapshot| {
        let progress = Progress::MinConflicts(*snapshot.metrics());
        final_progress = Some(progress.clone());
        report(
            &mut history,
            progress,
            || Snapshot::from(snapshot),
            sender,
            cancel,
        )
    })
    .map_err(|error| format!("Invalid min-conflicts config: {error}"))?;

    if let Some(progress) = &final_progress {
        history.push(progress.clone(), true);
    }
    let best_chromosome = result.best_chromosome();
    Ok(RunResult {
        config,
        history,
        stop_reason: result.stop_reason(),
        solved_step: (result.stop_reason() == ga::StopReason::Solved).then_some(result.passes()),
        total_elapsed_ms: result.elapsed_ms(),
        final_progress,
        best_positions: best_chromosome.get_positions().to_vec(),
        best_conflicts: best_chromosome.get_conflicts().to_vec(),
        best_conflicts_sum: best_chromosome.get_conflicts_sum(),
        population_size: None,
    })
}

#[cfg(test)]
pub(super) mod tests {
    use std::time::Duration;

    use super::*;
    use crate::ga::GaConfig;

    /// Runs the solver to completion on a background worker, as the app does.
    pub(in crate::gui) fn finished_run(config: GuiConfig) -> Arc<RunResult> {
        let (receiver, _) = spawn_solver(config);
        loop {
            match receiver.recv_timeout(Duration::from_secs(10)).unwrap() {
                WorkerMessage::Finished(result) => return result,
                WorkerMessage::Snapshot(_) => {}
                WorkerMessage::Failed(error) => panic!("{error}"),
            }
        }
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
                history.push(Progress::Ga(snapshot.metrics().clone()), false);
                assert!(history.points.len() <= MAX_CHART_POINTS);
                true
            },
        );
        history.push(Progress::Ga(metrics.epochs().last().unwrap().clone()), true);
        assert_eq!(history.points.first().unwrap().step(), 0);
        assert_eq!(history.points.last().unwrap().step(), 5_000);
        assert!(history.points.len() <= MAX_CHART_POINTS);
        assert!(
            history
                .points
                .windows(2)
                .all(|pair| pair[0].step() < pair[1].step())
        );
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
                assert_eq!(result.stop_reason, ga::StopReason::Solved);
                assert_eq!(result.final_progress.as_ref().unwrap().step(), 0);
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
                    assert_eq!(result.stop_reason, ga::StopReason::Cancelled);
                    break;
                }
                WorkerMessage::Failed(error) => panic!("{error}"),
            }
        }
    }

    #[test]
    fn min_conflicts_runs_report_passes_and_can_be_cancelled() {
        let result = finished_run(GuiConfig {
            solver: Solver::MinConflicts,
            board_size: 200,
            ..Default::default()
        });
        assert_eq!(result.stop_reason, ga::StopReason::Solved);
        assert_eq!(result.best_conflicts_sum, 0);
        assert_eq!(result.population_size, None);
        let last = result.final_progress.as_ref().unwrap();
        assert!(matches!(last, Progress::MinConflicts(_)));
        assert_eq!(result.solved_step, Some(last.step()));
        assert_eq!(result.history.points.first().unwrap().step(), 0);
        assert_eq!(result.history.points.last().unwrap().step(), last.step());

        let (receiver, cancel) = spawn_solver(GuiConfig {
            solver: Solver::MinConflicts,
            board_size: 3,
            max_epochs: u32::MAX,
            allow_unsolvable: true,
            ..Default::default()
        });
        cancel.store(true, Ordering::Relaxed);
        loop {
            match receiver.recv_timeout(Duration::from_secs(5)).unwrap() {
                WorkerMessage::Snapshot(_) => {}
                WorkerMessage::Finished(result) => {
                    assert_eq!(result.stop_reason, ga::StopReason::Cancelled);
                    break;
                }
                WorkerMessage::Failed(error) => panic!("{error}"),
            }
        }
    }
}
