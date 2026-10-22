//! The background solver thread and the bounded history it reports.

use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TrySendError},
    },
    thread,
};

use super::config::GuiConfig;
use crate::ga::{self, EpochSnapshot, RunMetrics};

pub(super) struct RunningRun {
    pub(super) receiver: Receiver<WorkerMessage>,
    pub(super) cancel: Arc<AtomicBool>,
    pub(super) max_epochs: u32,
}

pub(super) enum WorkerMessage {
    Snapshot(EpochSnapshot),
    Finished(Arc<RunResult>),
    Failed(String),
}

pub(super) struct RunResult {
    pub(super) config: GuiConfig,
    pub(super) history: MetricHistory,
    pub(super) metrics: RunMetrics,
    pub(super) best_positions: Vec<u16>,
    pub(super) best_conflicts: Vec<u32>,
    pub(super) best_conflicts_sum: u32,
    pub(super) population_size: usize,
}

pub(super) const MAX_CHART_POINTS: usize = 1024;

#[derive(Clone)]
pub(super) struct MetricHistory {
    pub(super) restart_epochs: VecDeque<u32>,
    pub(super) points: Vec<ga::EpochMetrics>,
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
    pub(super) fn push(&mut self, metrics: ga::EpochMetrics, force: bool) {
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

pub(super) fn spawn_solver(config: GuiConfig) -> (Receiver<WorkerMessage>, Arc<AtomicBool>) {
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
