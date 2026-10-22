//! Per-epoch and per-run metrics, phase profiling, and the population statistics behind them.

use std::{
    collections::HashSet,
    fmt,
    hash::{BuildHasher, Hasher},
};

use web_time::Instant;

use super::chromosome::Chromosome;

#[derive(Debug, Clone)]
pub struct EpochMetrics {
    epoch: u32,
    best_conflicts_sum: u32,
    population_size: usize,
    elapsed_ms: u128,
    average_conflicts_sum: f32,
    unique_chromosomes: usize,
    mutation_rate: f32,
    elite_ratio: f32,
    offspring_count: usize,
    local_search_improvements: usize,
    stagnation_epochs: u32,
    diversity_replacements: usize,
    restart_count: u32,
    last_restart_epoch: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct EpochRecordContext {
    pub(super) mutation_rate: f32,
    pub(super) elite_ratio: f32,
    pub(super) offspring_count: usize,
    pub(super) local_search_improvements: usize,
    pub(super) stagnation_epochs: u32,
    pub(super) diversity_replacements: usize,
    pub(super) elapsed_ms: u128,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PopulationMetrics {
    pub(super) population_size: usize,
    pub(super) best_index: Option<usize>,
    pub(super) best_conflicts_sum: u32,
    pub(super) average_conflicts_sum: f32,
    pub(super) unique_chromosomes: usize,
}

impl EpochMetrics {
    /// Number of soft restarts completed by this epoch.
    #[must_use]
    pub const fn restart_count(&self) -> u32 {
        self.restart_count
    }

    /// Actual epoch of the most recent soft restart, even if intervening snapshots were skipped.
    #[must_use]
    pub const fn last_restart_epoch(&self) -> Option<u32> {
        self.last_restart_epoch
    }

    #[must_use]
    pub const fn epoch(&self) -> u32 {
        self.epoch
    }

    #[must_use]
    pub const fn best_conflicts_sum(&self) -> u32 {
        self.best_conflicts_sum
    }

    #[must_use]
    pub const fn population_size(&self) -> usize {
        self.population_size
    }

    #[must_use]
    pub const fn elapsed_ms(&self) -> u128 {
        self.elapsed_ms
    }

    #[must_use]
    pub const fn average_conflicts_sum(&self) -> f32 {
        self.average_conflicts_sum
    }

    #[must_use]
    pub const fn unique_chromosomes(&self) -> usize {
        self.unique_chromosomes
    }

    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "Diversity is an approximate f32 ratio; integer population counts remain exact."
    )]
    pub fn diversity_ratio(&self) -> f32 {
        if self.population_size == 0 {
            0.0
        } else {
            self.unique_chromosomes as f32 / self.population_size as f32
        }
    }

    #[must_use]
    pub const fn mutation_rate(&self) -> f32 {
        self.mutation_rate
    }

    #[must_use]
    pub const fn elite_ratio(&self) -> f32 {
        self.elite_ratio
    }

    #[must_use]
    pub const fn offspring_count(&self) -> usize {
        self.offspring_count
    }

    #[must_use]
    pub const fn local_search_improvements(&self) -> usize {
        self.local_search_improvements
    }

    #[must_use]
    pub const fn stagnation_epochs(&self) -> u32 {
        self.stagnation_epochs
    }

    #[must_use]
    pub const fn diversity_replacements(&self) -> usize {
        self.diversity_replacements
    }
}

/// Why a run stopped. Finding a solution takes precedence over callback cancellation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StopReason {
    Solved,
    #[default]
    EpochLimit,
    Cancelled,
    Unsolvable,
}

impl fmt::Display for StopReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Solved => "solved",
            Self::EpochLimit => "epoch_limit",
            Self::Cancelled => "cancelled",
            Self::Unsolvable => "unsolvable",
        })
    }
}

/// Controls observation and termination without changing the GA's random sequence.
#[derive(Debug, Clone, Copy)]
pub struct RunOptions {
    /// Evolve boards of size 2 or 3 for experiments despite their known impossibility.
    pub allow_unsolvable: bool,
    /// Retain every epoch; when false, retain only the latest metrics.
    pub collect_history: bool,
    /// Measure cumulative wall time of solver phases, excluding population construction.
    pub profile: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            allow_unsolvable: false,
            collect_history: true,
            profile: false,
        }
    }
}

/// Cumulative wall-clock nanoseconds. Phases do not overlap; bookkeeping and callbacks
/// are excluded. Population metrics include the uniqueness `HashSet` construction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhaseTimings {
    pub crossover_ns: u128,
    pub mutation_ns: u128,
    pub local_search_ns: u128,
    pub survivor_selection_ns: u128,
    pub fitness_ns: u128,
    pub population_metrics_ns: u128,
    pub diversity_refresh_ns: u128,
    pub restart_ns: u128,
}

impl PhaseTimings {
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "crossover_ns": self.crossover_ns,
            "mutation_ns": self.mutation_ns,
            "local_search_ns": self.local_search_ns,
            "survivor_selection_ns": self.survivor_selection_ns,
            "fitness_ns": self.fitness_ns,
            "population_metrics_ns": self.population_metrics_ns,
            "diversity_refresh_ns": self.diversity_refresh_ns,
            "restart_ns": self.restart_ns,
        })
    }
}

pub(super) fn measure<T>(enabled: bool, total_ns: &mut u128, work: impl FnOnce() -> T) -> T {
    let start = enabled.then(Instant::now);
    let result = work();
    if let Some(start) = start {
        *total_ns += start.elapsed().as_nanos();
    }
    result
}

#[derive(Debug, Clone, Default)]
pub struct RunMetrics {
    pub(super) epochs: Vec<EpochMetrics>,
    pub(super) solved_epoch: Option<u32>,
    pub(super) total_elapsed_ms: u128,
    pub(super) stop_reason: StopReason,
    pub(super) phase_timings: PhaseTimings,
    pub(super) profile: bool,
    pub(super) latest_only: bool,
    pub(super) restart_count: u32,
    pub(super) last_restart_epoch: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct EpochSnapshot {
    pub(super) metrics: EpochMetrics,
    pub(super) best_positions: Vec<u16>,
    pub(super) best_conflicts: Vec<u32>,
    pub(super) best_conflicts_sum: u32,
}

impl RunMetrics {
    #[must_use]
    pub const fn stop_reason(&self) -> StopReason {
        self.stop_reason
    }

    /// None when profiling was not requested.
    #[must_use]
    pub fn phase_timings(&self) -> Option<&PhaseTimings> {
        self.profile.then_some(&self.phase_timings)
    }

    #[must_use]
    pub fn epochs(&self) -> &[EpochMetrics] {
        &self.epochs
    }

    #[must_use]
    pub const fn solved_epoch(&self) -> Option<u32> {
        self.solved_epoch
    }

    #[must_use]
    pub const fn total_elapsed_ms(&self) -> u128 {
        self.total_elapsed_ms
    }

    pub(super) fn record_epoch(
        &mut self,
        epoch: u32,
        population_metrics: PopulationMetrics,
        context: EpochRecordContext,
    ) {
        if self.latest_only {
            self.epochs.clear();
        }
        self.epochs.push(EpochMetrics {
            epoch,
            best_conflicts_sum: population_metrics.best_conflicts_sum,
            population_size: population_metrics.population_size,
            elapsed_ms: context.elapsed_ms,
            average_conflicts_sum: population_metrics.average_conflicts_sum,
            unique_chromosomes: population_metrics.unique_chromosomes,
            mutation_rate: context.mutation_rate,
            elite_ratio: context.elite_ratio,
            offspring_count: context.offspring_count,
            local_search_improvements: context.local_search_improvements,
            stagnation_epochs: context.stagnation_epochs,
            diversity_replacements: context.diversity_replacements,
            restart_count: self.restart_count,
            last_restart_epoch: self.last_restart_epoch,
        });
    }

    pub(super) const fn mark_solved(&mut self, solved_epoch: u32) {
        self.solved_epoch = Some(solved_epoch);
        self.stop_reason = StopReason::Solved;
    }

    pub(super) const fn set_total_elapsed_ms(&mut self, total_elapsed_ms: u128) {
        self.total_elapsed_ms = total_elapsed_ms;
    }
}

impl EpochSnapshot {
    #[must_use]
    pub const fn metrics(&self) -> &EpochMetrics {
        &self.metrics
    }

    #[must_use]
    pub fn best_positions(&self) -> &[u16] {
        &self.best_positions
    }

    #[must_use]
    pub fn best_conflicts(&self) -> &[u32] {
        &self.best_conflicts
    }

    #[must_use]
    pub const fn best_conflicts_sum(&self) -> u32 {
        self.best_conflicts_sum
    }
}

/// Builds [`FxHasher`] for the per-epoch uniqueness set.
///
/// The population is not adversarial, so the default `SipHash` resistance buys
/// nothing here while dominating the cost of the diversity metric. Uniqueness
/// stays exact: `HashSet` still compares positions on collision.
#[derive(Debug, Clone, Copy, Default)]
struct FxBuildHasher;

impl BuildHasher for FxBuildHasher {
    type Hasher = FxHasher;

    fn build_hasher(&self) -> FxHasher {
        FxHasher { hash: 0 }
    }
}

/// The `rustc-hash` multiply-xor-rotate hasher, inlined to avoid a dependency.
struct FxHasher {
    hash: u64,
}

impl FxHasher {
    const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

    const fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(Self::SEED);
    }
}

impl Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (words, remainder) = bytes.as_chunks::<{ size_of::<u64>() }>();
        for &word in words {
            self.add(u64::from_ne_bytes(word));
        }

        if !remainder.is_empty() {
            let mut word = [0u8; size_of::<u64>()];
            word[..remainder.len()].copy_from_slice(remainder);
            self.add(u64::from_ne_bytes(word));
        }
    }

    fn write_u16(&mut self, value: u16) {
        self.add(u64::from(value));
    }

    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }

    fn finish(&self) -> u64 {
        self.hash
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "The average is an approximate f32 metric; exact conflict sums are stored separately."
)]
pub(super) fn population_metrics(population: &[Chromosome]) -> PopulationMetrics {
    if population.is_empty() {
        return PopulationMetrics {
            population_size: 0,
            best_index: None,
            best_conflicts_sum: 0,
            average_conflicts_sum: 0.0,
            unique_chromosomes: 0,
        };
    }

    let mut best_conflicts_sum = u32::MAX;
    let mut best_index = None;
    let mut total_conflicts_sum = 0u64;
    let mut unique_chromosomes = HashSet::with_capacity_and_hasher(population.len(), FxBuildHasher);

    for (index, chromosome) in population.iter().enumerate() {
        let conflicts_sum = chromosome.get_conflicts_sum();
        if conflicts_sum < best_conflicts_sum {
            best_conflicts_sum = conflicts_sum;
            best_index = Some(index);
        }
        total_conflicts_sum += u64::from(conflicts_sum);
        unique_chromosomes.insert(chromosome.get_positions());
    }

    PopulationMetrics {
        population_size: population.len(),
        best_index,
        best_conflicts_sum,
        average_conflicts_sum: total_conflicts_sum as f32 / population.len() as f32,
        unique_chromosomes: unique_chromosomes.len(),
    }
}
