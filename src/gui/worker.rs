//! Runs the solver and keeps the bounded history it reports.
//!
//! Natively the solver runs on a background thread. A browser has no threads,
//! so there the GUI steps the solver itself for a slice of each frame.

use std::{
    collections::VecDeque,
    sync::{Arc, mpsc::TryRecvError},
    time::Duration,
};
#[cfg(not(target_arch = "wasm32"))]
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
};

use super::config::GuiConfig;
use crate::{
    Solver,
    ga::{self, GaRun, GeneticAlgorithm, chromosome::Chromosome},
    min_conflicts::{MinConflictsSearch, PassMetrics},
};

/// How often the GUI checks a running solver: natively for its reports, on
/// the web to give it its next slice.
#[cfg(not(target_arch = "wasm32"))]
pub(super) const POLL_INTERVAL: Duration = Duration::from_millis(33);
#[cfg(target_arch = "wasm32")]
pub(super) const POLL_INTERVAL: Duration = Duration::ZERO;

pub(super) struct RunningRun {
    pub(super) worker: Worker,
    pub(super) max_epochs: u32,
    pub(super) solver: Solver,
}

impl RunningRun {
    pub(super) fn start(config: GuiConfig) -> Self {
        Self {
            max_epochs: config.max_epochs,
            solver: config.solver,
            worker: Worker::start(config),
        }
    }
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

impl Snapshot {
    fn new(progress: Progress, best: &Chromosome) -> Self {
        Self {
            progress,
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

/// A solver run advanced one GA epoch or min-conflicts pass at a time, recording
/// each report in its history.
struct SolverRun {
    config: GuiConfig,
    history: MetricHistory,
    state: SolverState,
}

#[allow(
    clippy::large_enum_variant,
    reason = "The GUI holds a single run, so boxing saves nothing."
)]
enum SolverState {
    Ga {
        algorithm: GeneticAlgorithm,
        run: GaRun,
    },
    MinConflicts(MinConflictsSearch),
}

impl SolverRun {
    /// Starts the run, which records epoch or pass zero.
    fn start(config: GuiConfig) -> Result<Self, String> {
        let state = match config.solver {
            Solver::Ga => {
                let invalid = |error: ga::GaConfigError| format!("Invalid GA config: {error}");
                let ga_config = config.to_ga_config().map_err(invalid)?;
                let mut algorithm = ga::build_genetic_algorithm(ga_config).map_err(invalid)?;
                let run = algorithm.start_run(ga::RunOptions {
                    allow_unsolvable: config.allow_unsolvable,
                    collect_history: false,
                    profile: false,
                });
                SolverState::Ga { algorithm, run }
            }
            Solver::MinConflicts => SolverState::MinConflicts(
                MinConflictsSearch::new(config.to_min_conflicts_config())
                    .map_err(|error| format!("Invalid min-conflicts config: {error}"))?,
            ),
        };
        let mut run = Self {
            config,
            history: MetricHistory::default(),
            state,
        };
        run.record(false);
        Ok(run)
    }

    /// Runs the next epoch or pass and returns true, or returns false once the run has stopped.
    fn step(&mut self) -> bool {
        let stepped = match &mut self.state {
            SolverState::Ga { algorithm, run } => algorithm.step_run(run),
            SolverState::MinConflicts(search) => search.step(),
        };
        if stepped {
            self.record(false);
        }
        stepped
    }

    fn cancel(&mut self) {
        match &mut self.state {
            SolverState::Ga { run, .. } => run.cancel(),
            SolverState::MinConflicts(search) => search.cancel(),
        }
    }

    /// The latest report; `None` only for a GA run with an empty population.
    fn progress(&self) -> Option<Progress> {
        match &self.state {
            SolverState::Ga { run, .. } => run.metrics().epochs().last().cloned().map(Progress::Ga),
            SolverState::MinConflicts(search) => Some(Progress::MinConflicts(search.metrics())),
        }
    }

    fn snapshot(&self) -> Option<Snapshot> {
        let progress = self.progress()?;
        Some(match &self.state {
            SolverState::Ga { algorithm, .. } => {
                Snapshot::new(progress, algorithm.get_best_chromosome())
            }
            SolverState::MinConflicts(search) => {
                Snapshot::new(progress, search.snapshot().best_chromosome())
            }
        })
    }

    fn record(&mut self, force: bool) {
        if let Some(progress) = self.progress() {
            self.history.push(progress, force);
        }
    }

    /// The run's result; a run stopped before it finished reports [`ga::StopReason::Cancelled`].
    fn finish(mut self) -> RunResult {
        self.cancel();
        self.record(true);
        let final_progress = self.progress();
        let Self {
            config,
            history,
            state,
        } = self;
        match state {
            SolverState::Ga { algorithm, run } => {
                let metrics = run.into_metrics();
                let best = algorithm.get_best_chromosome();
                RunResult {
                    config,
                    history,
                    stop_reason: metrics.stop_reason(),
                    solved_step: metrics.solved_epoch(),
                    total_elapsed_ms: metrics.total_elapsed_ms(),
                    final_progress,
                    best_positions: best.get_positions().to_vec(),
                    best_conflicts: best.get_conflicts().to_vec(),
                    best_conflicts_sum: best.get_conflicts_sum(),
                    population_size: Some(algorithm.get_population_size()),
                }
            }
            SolverState::MinConflicts(search) => {
                let result = search.into_result();
                let best = result.best_chromosome();
                RunResult {
                    config,
                    history,
                    stop_reason: result.stop_reason(),
                    solved_step: (result.stop_reason() == ga::StopReason::Solved)
                        .then_some(result.passes()),
                    total_elapsed_ms: result.elapsed_ms(),
                    final_progress,
                    best_positions: best.get_positions().to_vec(),
                    best_conflicts: best.get_conflicts().to_vec(),
                    best_conflicts_sum: best.get_conflicts_sum(),
                    population_size: None,
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) type Worker = ThreadWorker;
#[cfg(target_arch = "wasm32")]
pub(super) type Worker = SteppedWorker;

/// The solver thread and the channel it reports on.
#[cfg(not(target_arch = "wasm32"))]
pub(super) struct ThreadWorker {
    pub(super) receiver: Receiver<WorkerMessage>,
    pub(super) cancel: Arc<AtomicBool>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ThreadWorker {
    fn start(config: GuiConfig) -> Self {
        let (receiver, cancel) = spawn_solver(config);
        Self { receiver, cancel }
    }

    /// The thread runs the solver on its own, unlike [`SteppedWorker`].
    #[allow(
        clippy::unused_self,
        reason = "A method, like SteppedWorker::advance, so the GUI calls either the same way."
    )]
    pub(super) const fn advance(&self) {}

    pub(super) fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub(super) fn try_recv(&self) -> Result<WorkerMessage, TryRecvError> {
        self.receiver.try_recv()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn spawn_solver(config: GuiConfig) -> (Receiver<WorkerMessage>, Arc<AtomicBool>) {
    let (sender, receiver) = mpsc::sync_channel(1);
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_worker = Arc::clone(&cancel);

    thread::spawn(move || {
        let outcome = SolverRun::start(config).map(|mut run| {
            loop {
                if !report(&run, &sender, &cancel_worker) {
                    run.cancel();
                }
                if !run.step() {
                    return run.finish();
                }
            }
        });
        let _ = sender.send(match outcome {
            Ok(result) => WorkerMessage::Finished(Arc::new(result)),
            Err(error) => WorkerMessage::Failed(error),
        });
    });

    (receiver, cancel)
}

/// Offers the latest report to the GUI; false asks the solver to stop.
#[cfg(not(target_arch = "wasm32"))]
fn report(run: &SolverRun, sender: &SyncSender<WorkerMessage>, cancel: &AtomicBool) -> bool {
    if cancel.load(Ordering::Relaxed) {
        return false;
    }
    let Some(snapshot) = run.snapshot() else {
        return true;
    };

    let connected = match sender.try_send(WorkerMessage::Snapshot(snapshot)) {
        Ok(()) | Err(TrySendError::Full(_)) => true,
        Err(TrySendError::Disconnected(_)) => false,
    };
    connected && !cancel.load(Ordering::Relaxed)
}

/// Steps the solver on the GUI thread, for a slice of each frame. Browsers have no threads.
#[cfg(any(test, target_arch = "wasm32"))]
pub(super) struct SteppedWorker {
    run: Option<SolverRun>,
    messages: VecDeque<WorkerMessage>,
}

#[cfg(any(test, target_arch = "wasm32"))]
impl SteppedWorker {
    /// Solver time per frame, leaving the rest of a 60 Hz frame to draw.
    const SLICE: Duration = Duration::from_millis(10);

    fn start(config: GuiConfig) -> Self {
        let mut worker = Self {
            run: None,
            messages: VecDeque::new(),
        };
        match SolverRun::start(config) {
            Ok(run) => worker.run = Some(run),
            Err(error) => worker.messages.push_back(WorkerMessage::Failed(error)),
        }
        worker
    }

    /// Steps the solver for one slice, then queues its latest report or its result.
    /// Each call runs at least one epoch or pass.
    pub(super) fn advance(&mut self) {
        let Some(run) = &mut self.run else {
            return;
        };
        let deadline = web_time::Instant::now() + Self::SLICE;
        loop {
            if !run.step() {
                let result = self.run.take().expect("checked above").finish();
                self.messages
                    .push_back(WorkerMessage::Finished(Arc::new(result)));
                return;
            }
            if web_time::Instant::now() >= deadline {
                break;
            }
        }
        if let Some(snapshot) = run.snapshot() {
            self.messages.push_back(WorkerMessage::Snapshot(snapshot));
        }
    }

    pub(super) fn cancel(&mut self) {
        if let Some(run) = &mut self.run {
            run.cancel();
        }
    }

    pub(super) fn try_recv(&mut self) -> Result<WorkerMessage, TryRecvError> {
        self.messages.pop_front().ok_or(TryRecvError::Empty)
    }
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

    /// Advances a stepped worker frame by frame until it reports a result.
    fn stepped_run(config: GuiConfig) -> (Arc<RunResult>, usize) {
        let mut worker = SteppedWorker::start(config);
        let mut snapshots = 0;
        loop {
            worker.advance();
            while let Ok(message) = worker.try_recv() {
                match message {
                    WorkerMessage::Snapshot(_) => snapshots += 1,
                    WorkerMessage::Finished(result) => return (result, snapshots),
                    WorkerMessage::Failed(error) => panic!("{error}"),
                }
            }
        }
    }

    #[test]
    fn stepped_worker_matches_the_thread_worker() {
        for config in [
            GuiConfig {
                board_size: 12,
                population_size: 40,
                max_epochs: 300,
                ..Default::default()
            },
            GuiConfig {
                solver: Solver::MinConflicts,
                board_size: 200,
                ..Default::default()
            },
        ] {
            let expected = finished_run(config.clone());
            let (result, _) = stepped_run(config);
            assert_eq!(result.stop_reason, expected.stop_reason);
            assert_eq!(result.solved_step, expected.solved_step);
            assert_eq!(result.best_positions, expected.best_positions);
            assert_eq!(result.best_conflicts_sum, expected.best_conflicts_sum);
            assert_eq!(
                result.final_progress.as_ref().map(Progress::step),
                expected.final_progress.as_ref().map(Progress::step)
            );
            let steps = |history: &MetricHistory| {
                history
                    .points
                    .iter()
                    .map(Progress::step)
                    .collect::<Vec<_>>()
            };
            assert_eq!(steps(&result.history), steps(&expected.history));
        }
    }

    #[test]
    fn stepped_worker_reports_between_slices_and_cancels() {
        let mut worker = SteppedWorker::start(GuiConfig {
            board_size: 3,
            population_size: 8,
            max_epochs: u32::MAX,
            allow_unsolvable: true,
            ..Default::default()
        });
        worker.advance();
        assert!(matches!(worker.try_recv(), Ok(WorkerMessage::Snapshot(_))));
        assert!(matches!(worker.try_recv(), Err(TryRecvError::Empty)));

        worker.cancel();
        worker.advance();
        match worker.try_recv() {
            Ok(WorkerMessage::Finished(result)) => {
                assert_eq!(result.stop_reason, ga::StopReason::Cancelled);
            }
            _ => panic!("expected terminal result"),
        }
        // A finished worker has nothing left to run.
        worker.advance();
        assert!(matches!(worker.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn stepped_worker_reports_invalid_configs() {
        let mut worker = SteppedWorker::start(GuiConfig {
            population_size: 0,
            ..Default::default()
        });
        worker.advance();
        assert!(matches!(worker.try_recv(), Ok(WorkerMessage::Failed(_))));
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
