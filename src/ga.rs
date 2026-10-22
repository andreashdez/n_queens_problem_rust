//! The genetic-algorithm solver.
//!
//! This module owns [`GeneticAlgorithm`], its epoch loop, fitness scoring, and the
//! adaptive mutation and elitism policy. Each phase of an epoch lives in its own
//! module: `crossover` for mating, `mutation`, `local_search`, `selection` for
//! parent and survivor selection, and `diversity` for refreshes and soft restarts.
//! `config` holds the public configuration and `metrics` the run metrics.

use rand::{SeedableRng, rngs::StdRng};
use rayon::prelude::*;
use web_time::Instant;

use self::{
    chromosome::Chromosome,
    crossover::offspring_count_for_population,
    metrics::{EpochRecordContext, PopulationMetrics, measure, population_metrics},
};
pub use self::{
    config::{
        DEFAULT_BOARD_SIZE, DEFAULT_ELITE_RATIO, DEFAULT_LOCAL_SEARCH_ATTEMPTS,
        DEFAULT_LOCAL_SEARCH_RATE, DEFAULT_MAX_EPOCHS, DEFAULT_MIN_DIVERSITY_RATIO,
        DEFAULT_MUTATION_RATE, DEFAULT_OFFSPRING_RATIO, DEFAULT_POPULATION_SIZE,
        DEFAULT_SELECTION_STRATEGY, DEFAULT_TOURNAMENT_SIZE, GaConfig, GaConfigError,
        SelectionStrategy,
    },
    metrics::{EpochMetrics, EpochSnapshot, PhaseTimings, RunMetrics, RunOptions, StopReason},
};

pub mod chromosome;
mod config;
mod crossover;
mod diversity;
mod local_search;
mod metrics;
mod mutation;
mod selection;

const TARGET_EPOCH_PROGRESS_LOGS: u32 = 20;
const TARGET_STAGNATION_RESETS: u32 = 20;
const MIN_STAGNATION_RESET_EPOCHS: u32 = 50;
const MAX_STAGNATION_RESET_EPOCHS: u32 = 500;
const MUTATION_STAGNATION_BOOST_SCALE: f32 = 3.0;
const MAX_ADAPTIVE_MUTATION_RATE: f32 = 0.60;
const MIN_ADAPTIVE_ELITE_RATIO: f32 = 0.01;
const MIN_ADAPTIVE_ELITE_RATIO_SCALE: f32 = 0.25;

pub struct GeneticAlgorithm {
    population: Vec<Chromosome>,
    best_chromosome: Option<Chromosome>,
    target_population_size: usize,
    max_epoch_count: u32,
    rng: StdRng,
    mutation_rate: f32,
    elite_ratio: f32,
    offspring_ratio: f32,
    min_diversity_ratio: f32,
    selection_strategy: SelectionStrategy,
    tournament_size: usize,
    local_search_rate: f32,
    local_search_attempts: usize,
}

#[derive(Debug, Clone, Copy)]
struct GeneticAlgorithmParams {
    target_population_size: usize,
    max_epoch_count: u32,
    mutation_rate: f32,
    elite_ratio: f32,
    offspring_ratio: f32,
    min_diversity_ratio: f32,
    selection_strategy: SelectionStrategy,
    tournament_size: usize,
    local_search_rate: f32,
    local_search_attempts: usize,
}

/// A run in progress, advanced one epoch at a time by [`GeneticAlgorithm::step_run`].
///
/// Step a run only with the algorithm that started it, which holds its population.
#[derive(Debug)]
pub struct GaRun {
    options: RunOptions,
    metrics: RunMetrics,
    started_at: Instant,
    best_conflicts_sum: u32,
    offspring_count: usize,
    stagnation_reset_interval: u32,
    stagnation_epochs: u32,
    /// Epochs completed after epoch zero.
    epochs_run: u32,
    finished: bool,
}

impl GaRun {
    #[must_use]
    pub const fn metrics(&self) -> &RunMetrics {
        &self.metrics
    }

    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Stops the run as [`StopReason::Cancelled`]; a stopped run keeps its stop reason.
    pub fn cancel(&mut self) {
        if !self.finished {
            self.metrics.stop_reason = StopReason::Cancelled;
            self.finish();
        }
    }

    #[must_use]
    pub fn into_metrics(self) -> RunMetrics {
        self.metrics
    }

    fn finish(&mut self) {
        self.metrics
            .set_total_elapsed_ms(self.started_at.elapsed().as_millis());
        self.finished = true;
    }
}

impl GeneticAlgorithm {
    const fn new(population: Vec<Chromosome>, rng: StdRng, params: GeneticAlgorithmParams) -> Self {
        Self {
            population,
            best_chromosome: None,
            target_population_size: params.target_population_size,
            max_epoch_count: params.max_epoch_count,
            rng,
            mutation_rate: params.mutation_rate,
            elite_ratio: params.elite_ratio,
            offspring_ratio: params.offspring_ratio,
            min_diversity_ratio: params.min_diversity_ratio,
            selection_strategy: params.selection_strategy,
            tournament_size: params.tournament_size,
            local_search_rate: params.local_search_rate,
            local_search_attempts: params.local_search_attempts,
        }
    }

    pub const fn get_population_size(&self) -> usize {
        self.population.len()
    }

    pub fn run_algorithm(&mut self) -> RunMetrics {
        self.run_algorithm_with_progress(|_| true)
    }

    pub fn run_algorithm_with_progress<F>(&mut self, on_epoch: F) -> RunMetrics
    where
        F: FnMut(&EpochSnapshot) -> bool,
    {
        self.run_algorithm_with_options(RunOptions::default(), on_epoch)
    }

    /// Reports epoch zero and each completed epoch. Returning false requests cancellation.
    /// Known impossible boards stop after epoch zero unless `allow_unsolvable` is set.
    pub fn run_algorithm_with_options<F>(
        &mut self,
        options: RunOptions,
        mut on_epoch: F,
    ) -> RunMetrics
    where
        F: FnMut(&EpochSnapshot) -> bool,
    {
        let mut run = self.start_run(options);
        loop {
            if let Some(snapshot) = self.epoch_snapshot(&run)
                && !on_epoch(&snapshot)
            {
                run.cancel();
            }
            if !self.step_run(&mut run) {
                return run.into_metrics();
            }
        }
    }

    /// Scores the starting population and records epoch zero.
    ///
    /// Advance the run with [`Self::step_run`], which
    /// [`Self::run_algorithm_with_options`] does until it stops. An empty or
    /// already solved population stops the run here.
    pub fn start_run(&mut self, options: RunOptions) -> GaRun {
        let started_at = Instant::now();
        let mut run = GaRun {
            options,
            metrics: RunMetrics {
                profile: options.profile,
                latest_only: !options.collect_history,
                ..RunMetrics::default()
            },
            started_at,
            best_conflicts_sum: 0,
            offspring_count: offspring_count_for_population(
                self.target_population_size,
                self.offspring_ratio,
            ),
            stagnation_reset_interval: stagnation_reset_interval(self.max_epoch_count),
            stagnation_epochs: 0,
            epochs_run: 0,
            finished: false,
        };

        if self.population.is_empty() {
            log::warn!("cannot run algorithm with empty population");
            run.finish();
            return run;
        }

        measure(
            options.profile,
            &mut run.metrics.phase_timings.fitness_ns,
            || self.calc_fitness(),
        );
        let initial_population_metrics = measure(
            options.profile,
            &mut run.metrics.phase_timings.population_metrics_ns,
            || population_metrics(&self.population),
        );
        self.update_best_chromosome(initial_population_metrics);
        run.best_conflicts_sum = self.get_best_chromosome().get_conflicts_sum();
        let mut initial_record_metrics = initial_population_metrics;
        initial_record_metrics.best_conflicts_sum = run.best_conflicts_sum;
        run.metrics.record_epoch(
            0,
            initial_record_metrics,
            EpochRecordContext {
                mutation_rate: self.mutation_rate,
                elite_ratio: self.elite_ratio,
                offspring_count: run.offspring_count,
                local_search_improvements: 0,
                stagnation_epochs: 0,
                diversity_replacements: 0,
                elapsed_ms: started_at.elapsed().as_millis(),
            },
        );

        if run.best_conflicts_sum == 0 {
            log::info!("ga solved in initial population");
            run.metrics.mark_solved(0);
            run.finish();
        }
        run
    }

    /// Runs the next epoch and returns true, or returns false once the run has stopped.
    ///
    /// Before the first epoch, a known impossible board stops the run as
    /// [`StopReason::Unsolvable`] unless `allow_unsolvable` is set.
    pub fn step_run(&mut self, run: &mut GaRun) -> bool {
        if run.finished {
            return false;
        }

        if run.epochs_run == 0 {
            if !run.options.allow_unsolvable
                && matches!(self.population[0].get_positions().len(), 2 | 3)
            {
                log::warn!("board size has no solution; use allow_unsolvable for experiments");
                run.metrics.stop_reason = StopReason::Unsolvable;
                run.finish();
                return false;
            }

            log::info!(
                "running ga epochs={} population_size={} progress_log_interval={} stagnation_reset_interval={} initial_best_conflicts_sum={} base_mutation_rate={} base_elite_ratio={} offspring_ratio={} offspring_count={} selection_strategy={} tournament_size={} local_search_rate={} local_search_attempts={}",
                self.max_epoch_count,
                self.get_population_size(),
                epoch_progress_log_interval(self.max_epoch_count),
                run.stagnation_reset_interval,
                run.best_conflicts_sum,
                self.mutation_rate,
                self.elite_ratio,
                self.offspring_ratio,
                run.offspring_count,
                self.selection_strategy,
                self.tournament_size,
                self.local_search_rate,
                self.local_search_attempts,
            );
        }

        if run.epochs_run == self.max_epoch_count {
            log::warn!(
                "no solution found best_conflicts_sum={} epochs={} population_size={}",
                run.best_conflicts_sum,
                self.max_epoch_count,
                self.get_population_size(),
            );
            run.finish();
            return false;
        }

        run.epochs_run += 1;
        self.run_epoch(run);
        true
    }

    #[allow(
        clippy::too_many_lines,
        reason = "Keep epoch ordering, profiling, and stop decisions together."
    )]
    fn run_epoch(&mut self, run: &mut GaRun) {
        let epoch_number = run.epochs_run;
        let profile = run.options.profile;

        if run.stagnation_epochs >= run.stagnation_reset_interval {
            let (_, reset_elite_ratio) = adaptive_ga_parameters(
                self.mutation_rate,
                self.elite_ratio,
                run.stagnation_epochs,
                run.stagnation_reset_interval,
            );
            run.metrics.restart_count += 1;
            run.metrics.last_restart_epoch = Some(epoch_number);
            let replaced_count =
                measure(profile, &mut run.metrics.phase_timings.restart_ns, || {
                    self.soft_restart_population(reset_elite_ratio)
                });
            measure(profile, &mut run.metrics.phase_timings.fitness_ns, || {
                self.calc_fitness();
            });

            let post_reset_population_metrics = measure(
                profile,
                &mut run.metrics.phase_timings.population_metrics_ns,
                || population_metrics(&self.population),
            );
            self.update_best_chromosome(post_reset_population_metrics);
            let post_reset_best_conflicts_sum = post_reset_population_metrics.best_conflicts_sum;
            run.best_conflicts_sum = run
                .best_conflicts_sum
                .min(self.get_best_chromosome().get_conflicts_sum());

            log::info!(
                "ga stagnation reset epoch={epoch_number} stagnant_epochs={} replaced={replaced_count} best_conflicts_sum={post_reset_best_conflicts_sum} elite_ratio={reset_elite_ratio:.4} population_size={}",
                run.stagnation_epochs,
                self.get_population_size(),
            );

            run.stagnation_epochs = 0;
        }

        let (epoch_mutation_rate, epoch_elite_ratio) = adaptive_ga_parameters(
            self.mutation_rate,
            self.elite_ratio,
            run.stagnation_epochs,
            run.stagnation_reset_interval,
        );

        let offspring_count = run.offspring_count;
        measure(profile, &mut run.metrics.phase_timings.crossover_ns, || {
            self.mate_random_chromosomes(offspring_count);
        });
        measure(profile, &mut run.metrics.phase_timings.mutation_ns, || {
            self.mutate_population(epoch_mutation_rate, epoch_elite_ratio);
        });
        let local_search_improvements = measure(
            profile,
            &mut run.metrics.phase_timings.local_search_ns,
            || self.improve_population_with_local_search(epoch_elite_ratio),
        );
        measure(
            profile,
            &mut run.metrics.phase_timings.survivor_selection_ns,
            || self.select_survivors(epoch_elite_ratio),
        );
        measure(profile, &mut run.metrics.phase_timings.fitness_ns, || {
            self.calc_fitness();
        });

        let mut epoch_population_metrics = measure(
            profile,
            &mut run.metrics.phase_timings.population_metrics_ns,
            || population_metrics(&self.population),
        );
        let diversity_replacements = measure(
            profile,
            &mut run.metrics.phase_timings.diversity_refresh_ns,
            || {
                self.refresh_low_diversity_population(
                    epoch_elite_ratio,
                    epoch_population_metrics.unique_chromosomes,
                )
            },
        );
        if diversity_replacements > 0 {
            measure(profile, &mut run.metrics.phase_timings.fitness_ns, || {
                self.calc_fitness();
            });
            epoch_population_metrics = measure(
                profile,
                &mut run.metrics.phase_timings.population_metrics_ns,
                || population_metrics(&self.population),
            );
        }

        let epoch_best_conflicts_sum = epoch_population_metrics.best_conflicts_sum;
        let population_size = epoch_population_metrics.population_size;

        let is_improvement = epoch_best_conflicts_sum < run.best_conflicts_sum;
        if is_improvement {
            self.update_best_chromosome(epoch_population_metrics);
            run.best_conflicts_sum = epoch_best_conflicts_sum;
            run.stagnation_epochs = 0;
        } else {
            run.stagnation_epochs += 1;
        }

        let best_conflicts_sum = run.best_conflicts_sum;
        let stagnation_epochs = run.stagnation_epochs;
        let mut epoch_record_metrics = epoch_population_metrics;
        epoch_record_metrics.best_conflicts_sum = best_conflicts_sum;
        run.metrics.record_epoch(
            epoch_number,
            epoch_record_metrics,
            EpochRecordContext {
                mutation_rate: epoch_mutation_rate,
                elite_ratio: epoch_elite_ratio,
                offspring_count,
                local_search_improvements,
                stagnation_epochs,
                diversity_replacements,
                elapsed_ms: run.started_at.elapsed().as_millis(),
            },
        );

        if best_conflicts_sum == 0 {
            log::info!(
                "ga solved epoch={epoch_number} population_size={population_size} mutation_rate={epoch_mutation_rate:.4} elite_ratio={epoch_elite_ratio:.4} local_search_improvements={local_search_improvements}"
            );
            run.metrics.mark_solved(epoch_number);
            run.finish();
        } else if is_improvement {
            log::info!(
                "ga improvement epoch={epoch_number} best_conflicts_sum={best_conflicts_sum} population_size={population_size} mutation_rate={epoch_mutation_rate:.4} elite_ratio={epoch_elite_ratio:.4} local_search_improvements={local_search_improvements}",
            );
        } else if epoch_number.is_multiple_of(epoch_progress_log_interval(self.max_epoch_count))
            || epoch_number == self.max_epoch_count
        {
            log::info!(
                "ga progress epoch={epoch_number} best_conflicts_sum={best_conflicts_sum} population_size={population_size} stagnant_epochs={stagnation_epochs} mutation_rate={epoch_mutation_rate:.4} elite_ratio={epoch_elite_ratio:.4} local_search_improvements={local_search_improvements} diversity_replacements={diversity_replacements}",
            );
        }
    }

    /// The latest epoch with the best board so far; `None` before epoch zero is recorded.
    pub fn epoch_snapshot(&self, run: &GaRun) -> Option<EpochSnapshot> {
        let metrics = run.metrics.epochs().last()?;
        let best_chromosome = self.get_best_chromosome();
        Some(EpochSnapshot {
            metrics: metrics.clone(),
            best_positions: best_chromosome.get_positions().to_vec(),
            best_conflicts: best_chromosome.get_conflicts().to_vec(),
            best_conflicts_sum: best_chromosome.get_conflicts_sum(),
        })
    }

    pub fn get_best_chromosome(&self) -> &Chromosome {
        self.best_chromosome
            .as_ref()
            .unwrap_or_else(|| self.get_population_best_chromosome())
    }

    fn get_population_best_chromosome(&self) -> &Chromosome {
        self.population
            .iter()
            .min_by_key(|chromosome| chromosome.get_conflicts_sum())
            .expect("population is never empty while running")
    }

    /// Returns a chromosome with the largest conflict count.
    ///
    /// # Panics
    /// Panics if the population is empty. Algorithms created by
    /// [`build_genetic_algorithm`] maintain a nonempty population.
    pub fn get_worst_chromosome(&self) -> &Chromosome {
        self.population
            .iter()
            .max_by_key(|chromosome| chromosome.get_conflicts_sum())
            .expect("population is never empty while running")
    }

    fn update_best_chromosome(&mut self, population_metrics: PopulationMetrics) {
        let Some(candidate_index) = population_metrics.best_index else {
            return;
        };

        let Some(candidate) = self.population.get(candidate_index) else {
            return;
        };

        let is_improvement = self
            .best_chromosome
            .as_ref()
            .is_none_or(|best| candidate.get_conflicts_sum() < best.get_conflicts_sum());
        if is_improvement {
            self.best_chromosome = Some(candidate.clone());
        }
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "Fitness is intentionally calculated in f32 to preserve seeded selection behavior."
    )]
    fn calc_fitness(&mut self) {
        if self.population.is_empty() {
            return;
        }

        let most_conflicts = self.get_worst_chromosome().get_conflicts_sum() as f32;
        let least_conflicts = self.get_population_best_chromosome().get_conflicts_sum() as f32;
        let diff_conflicts = most_conflicts - least_conflicts;

        log::debug!(
            "calculating fitness [worst_score={most_conflicts}, best_score={least_conflicts}, diff={diff_conflicts}]",
        );

        if diff_conflicts.abs() <= f32::EPSILON {
            log::debug!("all chromosomes have equal conflicts; assigning uniform fitness");
            self.population.par_iter_mut().for_each(|chromosome| {
                chromosome.set_fitness(1.0);
            });
            return;
        }

        let diff_conflicts_pow3 = diff_conflicts.powi(3);
        self.population.par_iter_mut().for_each(|chromosome| {
            let conflicts_sum = chromosome.get_conflicts_sum() as f32;
            let fitness = (most_conflicts - conflicts_sum).powi(3) / diff_conflicts_pow3;
            chromosome.set_fitness(fitness);
            log::trace!(
                "calculating fitness for chromosome [conflicts={conflicts_sum}, fitness={fitness}]",
            );
        });
    }
}

/// Builds the initial population using the configuration's seed.
///
/// # Errors
/// Returns [`GaConfigError`] if [`GaConfig::validate`] rejects the configuration.
pub fn build_genetic_algorithm(config: GaConfig) -> Result<GeneticAlgorithm, GaConfigError> {
    config.validate()?;

    let target_population_size = config.initial_population;
    let mut rng = StdRng::seed_from_u64(config.seed);
    let mut population: Vec<Chromosome> = Vec::with_capacity(target_population_size);

    for _ in 0..target_population_size {
        let positions = chromosome::generate_distinct_random_values_with_rng(config.size, &mut rng);
        let chromosome = Chromosome::new(positions);
        population.push(chromosome);
    }

    Ok(GeneticAlgorithm::new(
        population,
        rng,
        GeneticAlgorithmParams {
            target_population_size,
            max_epoch_count: config.max_epoch_count,
            mutation_rate: config.mutation_rate,
            elite_ratio: config.elite_ratio,
            offspring_ratio: config.offspring_ratio,
            min_diversity_ratio: config.min_diversity_ratio,
            selection_strategy: config.selection_strategy,
            tournament_size: config.tournament_size,
            local_search_rate: config.local_search_rate,
            local_search_attempts: config.local_search_attempts,
        },
    ))
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Stagnation is normalized to an approximate f32 ratio for adaptive rates."
)]
fn adaptive_ga_parameters(
    base_mutation_rate: f32,
    base_elite_ratio: f32,
    stagnation_epochs: u32,
    stagnation_reset_interval: u32,
) -> (f32, f32) {
    let stagnation_ratio = if stagnation_reset_interval == 0 {
        0.0
    } else {
        (stagnation_epochs as f32 / stagnation_reset_interval as f32).clamp(0.0, 1.0)
    };

    let mutation_ceiling = MAX_ADAPTIVE_MUTATION_RATE.max(base_mutation_rate);
    let adaptive_mutation_rate = (base_mutation_rate
        * MUTATION_STAGNATION_BOOST_SCALE.mul_add(stagnation_ratio, 1.0))
    .clamp(0.0, mutation_ceiling);

    let adaptive_elite_scale =
        (1.0 - MIN_ADAPTIVE_ELITE_RATIO_SCALE).mul_add(-stagnation_ratio, 1.0);
    let min_elite_ratio = base_elite_ratio.min(MIN_ADAPTIVE_ELITE_RATIO);
    let adaptive_elite_ratio =
        (base_elite_ratio * adaptive_elite_scale).clamp(min_elite_ratio, 1.0);

    (adaptive_mutation_rate, adaptive_elite_ratio)
}

fn epoch_progress_log_interval(max_epoch_count: u32) -> u32 {
    (max_epoch_count / TARGET_EPOCH_PROGRESS_LOGS).max(1)
}

fn stagnation_reset_interval(max_epoch_count: u32) -> u32 {
    (max_epoch_count / TARGET_STAGNATION_RESETS)
        .clamp(MIN_STAGNATION_RESET_EPOCHS, MAX_STAGNATION_RESET_EPOCHS)
}

/// Unstable hooks for the phase benchmarks; not part of the solver API.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub mod benchmarking {
    use super::{
        GeneticAlgorithm, crossover::offspring_count_for_population, metrics::population_metrics,
    };

    pub fn prepare(algorithm: &mut GeneticAlgorithm) {
        algorithm.calc_fitness();
    }
    pub fn crossover(algorithm: &mut GeneticAlgorithm) -> usize {
        let count = offspring_count_for_population(
            algorithm.target_population_size,
            algorithm.offspring_ratio,
        );
        algorithm.mate_random_chromosomes(count);
        algorithm.population.len()
    }
    pub fn mutation(algorithm: &mut GeneticAlgorithm) -> usize {
        algorithm.mutate_population(algorithm.mutation_rate, algorithm.elite_ratio);
        algorithm.population.len()
    }
    pub fn local_search(algorithm: &mut GeneticAlgorithm) -> usize {
        algorithm.improve_population_with_local_search(algorithm.elite_ratio)
    }
    pub fn diversity_metrics(algorithm: &mut GeneticAlgorithm) -> usize {
        population_metrics(&algorithm.population).unique_chromosomes
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "Tests assert exact copied rates, seeded results, and fitness endpoints; approximate metrics use tolerances."
)]
mod tests {
    use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};

    use super::*;

    pub(super) fn build_test_algorithm(population: Vec<Chromosome>) -> GeneticAlgorithm {
        let target_population_size = population.len().max(1);
        GeneticAlgorithm::new(
            population,
            StdRng::seed_from_u64(7),
            GeneticAlgorithmParams {
                target_population_size,
                max_epoch_count: 10,
                mutation_rate: DEFAULT_MUTATION_RATE,
                elite_ratio: DEFAULT_ELITE_RATIO,
                offspring_ratio: DEFAULT_OFFSPRING_RATIO,
                min_diversity_ratio: DEFAULT_MIN_DIVERSITY_RATIO,
                selection_strategy: DEFAULT_SELECTION_STRATEGY,
                tournament_size: DEFAULT_TOURNAMENT_SIZE,
                local_search_rate: DEFAULT_LOCAL_SEARCH_RATE,
                local_search_attempts: DEFAULT_LOCAL_SEARCH_ATTEMPTS,
            },
        )
    }

    pub(super) fn shuffled_values(size: usize, seed: u64) -> Vec<u16> {
        let mut values =
            (0..u16::try_from(size).expect("size should fit into u16")).collect::<Vec<_>>();
        let mut rng = StdRng::seed_from_u64(seed);
        values.shuffle(&mut rng);
        values
    }

    #[test]
    fn restart_metadata_reports_actual_epochs() {
        let config = GaConfig::new(3, 1, 110, 42)
            .with_mutation_rate(0.0)
            .with_offspring_ratio(0.0);
        let metrics = build_genetic_algorithm(config)
            .unwrap()
            .run_algorithm_with_options(
                super::RunOptions {
                    allow_unsolvable: true,
                    ..Default::default()
                },
                |_| true,
            );
        assert_eq!(metrics.epochs()[0].restart_count(), 0);
        assert_eq!(metrics.epochs()[0].last_restart_epoch(), None);
        assert_eq!(metrics.epochs()[50].restart_count(), 0);
        assert_eq!(metrics.epochs()[51].restart_count(), 1);
        assert_eq!(metrics.epochs()[70].last_restart_epoch(), Some(51));
        assert_eq!(metrics.epochs()[101].restart_count(), 2);
        assert_eq!(
            metrics.epochs().last().unwrap().last_restart_epoch(),
            Some(101)
        );
    }

    #[test]
    fn stop_reasons_cover_solve_impossibility_budget_and_cancellation() {
        use super::{RunOptions, StopReason};
        let mut solved = build_genetic_algorithm(GaConfig::new(1, 1, 2, 42)).unwrap();
        assert_eq!(
            solved.run_algorithm_with_progress(|_| false).stop_reason(),
            StopReason::Solved
        );
        for size in [2, 3] {
            let config = GaConfig::new(size, 8, 3, 42);
            let result = build_genetic_algorithm(config).unwrap().run_algorithm();
            assert_eq!(result.stop_reason(), StopReason::Unsolvable);
            assert_eq!(result.epochs().len(), 1);
            let options = RunOptions {
                allow_unsolvable: true,
                ..Default::default()
            };
            let result = build_genetic_algorithm(config)
                .unwrap()
                .run_algorithm_with_options(options, |_| true);
            assert_eq!(result.stop_reason(), StopReason::EpochLimit);
            assert_eq!(result.epochs().last().unwrap().epoch(), 3);
            for stop_at in [0, 1] {
                let result = build_genetic_algorithm(config)
                    .unwrap()
                    .run_algorithm_with_options(options, |s| s.metrics().epoch() < stop_at);
                assert_eq!(result.stop_reason(), StopReason::Cancelled);
                assert_eq!(result.epochs().last().unwrap().epoch(), stop_at);
            }
        }
    }

    #[test]
    fn stepping_a_run_matches_running_it_to_completion() {
        use super::{RunOptions, StopReason};
        let config = GaConfig::new(12, 40, 300, 7);
        let mut expected_algorithm = build_genetic_algorithm(config).unwrap();
        let expected = expected_algorithm.run_algorithm();

        let mut algorithm = build_genetic_algorithm(config).unwrap();
        let mut run = algorithm.start_run(RunOptions::default());
        let mut steps = 0;
        while algorithm.step_run(&mut run) {
            steps += 1;
        }
        assert!(run.is_finished());
        assert!(!algorithm.step_run(&mut run));
        // Cancelling a stopped run keeps its stop reason.
        run.cancel();
        let metrics = run.into_metrics();
        assert_ne!(metrics.stop_reason(), StopReason::Cancelled);
        assert_eq!(metrics.stop_reason(), expected.stop_reason());
        assert_eq!(metrics.solved_epoch(), expected.solved_epoch());
        assert_eq!(metrics.epochs().last().unwrap().epoch(), steps);
        assert_eq!(
            algorithm.get_best_chromosome().get_positions(),
            expected_algorithm.get_best_chromosome().get_positions()
        );
    }

    #[test]
    fn profiling_and_history_options_preserve_seeded_results() {
        let config = GaConfig::new(18, 40, 12, 42).with_local_search_rate(0.05);
        let mut reference = build_genetic_algorithm(config).unwrap();
        let full = reference.run_algorithm();
        let mut observed = build_genetic_algorithm(config).unwrap();
        let mut epochs = Vec::new();
        let compact = observed.run_algorithm_with_options(
            super::RunOptions {
                profile: true,
                collect_history: false,
                ..Default::default()
            },
            |s| {
                epochs.push((s.metrics().epoch(), s.best_conflicts_sum()));
                true
            },
        );
        assert_eq!(full.stop_reason(), compact.stop_reason());
        assert_eq!(full.solved_epoch(), compact.solved_epoch());
        assert_eq!(
            reference.get_best_chromosome().get_positions(),
            observed.get_best_chromosome().get_positions()
        );
        assert_eq!(
            epochs,
            full.epochs()
                .iter()
                .map(|e| (e.epoch(), e.best_conflicts_sum()))
                .collect::<Vec<_>>()
        );
        assert_eq!(compact.epochs().len(), 1);
        assert_eq!(
            compact.epochs()[0].epoch(),
            full.epochs().last().unwrap().epoch()
        );
        assert!(full.phase_timings().is_none());
        let timings = compact.phase_timings().unwrap();
        assert!(timings.population_metrics_ns > 0);
        assert!(timings.crossover_ns > 0);
        assert!(timings.mutation_ns > 0);
        assert!(timings.local_search_ns > 0);
    }

    #[test]
    fn test_fitness_calculation() {
        let chromosome_one = Chromosome::new(vec![0, 2, 4, 6, 1, 3, 5, 7]);
        let chromosome_two = Chromosome::new(vec![2, 4, 1, 7, 5, 0, 6, 3]);
        let chromosome_three = Chromosome::new(vec![2, 4, 1, 7, 6, 0, 3, 5]);
        let chromosome_four = Chromosome::new(vec![2, 4, 5, 7, 6, 0, 3, 1]);
        let chromosome_five = Chromosome::new(vec![1, 4, 5, 7, 6, 0, 3, 2]);
        let chromosome_six = Chromosome::new(vec![2, 4, 1, 7, 6, 0, 5, 3]);
        let population = vec![
            chromosome_one,
            chromosome_two,
            chromosome_three,
            chromosome_four,
            chromosome_five,
            chromosome_six,
        ];

        let mut genetic_algorithm = build_test_algorithm(population);
        genetic_algorithm.calc_fitness();

        assert_eq!(genetic_algorithm.get_worst_chromosome().get_fitness(), 0.0);
        assert_eq!(genetic_algorithm.get_best_chromosome().get_fitness(), 1.0);
    }

    #[test]
    fn test_fitness_calculation_with_uniform_conflicts() {
        let chromosome_one = Chromosome::new(vec![0, 1, 2, 3]);
        let chromosome_two = Chromosome::new(vec![0, 1, 2, 3]);
        let population = vec![chromosome_one, chromosome_two];

        let mut genetic_algorithm = build_test_algorithm(population);
        genetic_algorithm.calc_fitness();

        for chromosome in genetic_algorithm.population {
            assert_eq!(chromosome.get_fitness(), 1.0);
        }
    }

    #[test]
    fn test_build_algorithm_rejects_zero_and_handles_one() {
        assert!(matches!(
            build_genetic_algorithm(
                GaConfig::new(0, 8, 10, 42)
                    .with_mutation_rate(DEFAULT_MUTATION_RATE)
                    .with_elite_ratio(DEFAULT_ELITE_RATIO),
            ),
            Err(GaConfigError::BoardSizeZero)
        ));

        let mut one_board = build_genetic_algorithm(
            GaConfig::new(1, 8, 10, 42)
                .with_mutation_rate(DEFAULT_MUTATION_RATE)
                .with_elite_ratio(DEFAULT_ELITE_RATIO),
        )
        .expect("valid config should build");
        one_board.run_algorithm();
        assert_eq!(one_board.get_best_chromosome().get_conflicts_sum(), 0);
    }

    #[test]
    fn test_build_algorithm_validates_public_config() {
        assert!(matches!(
            build_genetic_algorithm(
                GaConfig::new(8, 8, 10, 42)
                    .with_mutation_rate(f32::NAN)
                    .with_elite_ratio(DEFAULT_ELITE_RATIO),
            ),
            Err(GaConfigError::InvalidMutationRate)
        ));
    }

    #[test]
    fn test_run_metrics_include_initial_epoch_and_elapsed_time() {
        let mut genetic_algorithm = build_genetic_algorithm(
            GaConfig::new(8, 32, 5, 42)
                .with_mutation_rate(DEFAULT_MUTATION_RATE)
                .with_elite_ratio(DEFAULT_ELITE_RATIO),
        )
        .expect("valid config should build");

        let run_metrics = genetic_algorithm.run_algorithm();
        assert!(!run_metrics.epochs().is_empty());
        let initial_epoch = &run_metrics.epochs()[0];
        assert_eq!(initial_epoch.epoch(), 0);
        assert!(run_metrics.total_elapsed_ms() >= initial_epoch.elapsed_ms());
        assert!(
            initial_epoch.average_conflicts_sum()
                >= f32::from(u16::try_from(initial_epoch.best_conflicts_sum()).unwrap())
        );
        assert!(initial_epoch.unique_chromosomes() > 0);
        assert!(initial_epoch.diversity_ratio() > 0.0);
        assert_eq!(initial_epoch.mutation_rate(), DEFAULT_MUTATION_RATE);
        assert_eq!(initial_epoch.elite_ratio(), DEFAULT_ELITE_RATIO);
        assert_eq!(
            initial_epoch.offspring_count(),
            super::offspring_count_for_population(32, DEFAULT_OFFSPRING_RATIO)
        );
        assert_eq!(initial_epoch.local_search_improvements(), 0);
        assert_eq!(initial_epoch.stagnation_epochs(), 0);
        assert_eq!(initial_epoch.diversity_replacements(), 0);
    }

    #[test]
    fn test_run_algorithm_with_progress_reports_snapshots_and_can_cancel() {
        let mut genetic_algorithm = build_genetic_algorithm(
            GaConfig::new(3, 8, 5, 42)
                .with_mutation_rate(DEFAULT_MUTATION_RATE)
                .with_elite_ratio(DEFAULT_ELITE_RATIO),
        )
        .expect("valid config should build");
        let mut snapshots = Vec::new();

        let run_metrics = genetic_algorithm.run_algorithm_with_progress(|snapshot| {
            snapshots.push(snapshot.clone());
            false
        });

        assert_eq!(run_metrics.epochs().len(), 1);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].metrics().epoch(), 0);
        assert_eq!(snapshots[0].best_positions().len(), 3);
        assert_eq!(snapshots[0].best_conflicts().len(), 3);
        assert_eq!(
            snapshots[0].best_conflicts_sum(),
            run_metrics.epochs()[0].best_conflicts_sum()
        );
    }

    #[test]
    fn test_run_metrics_include_adaptive_epoch_details() {
        let mut genetic_algorithm = build_genetic_algorithm(
            GaConfig::new(3, 8, 2, 42)
                .with_mutation_rate(0.0)
                .with_elite_ratio(0.25)
                .with_offspring_ratio(0.0),
        )
        .expect("valid config should build");

        let run_metrics = genetic_algorithm.run_algorithm_with_options(
            super::RunOptions {
                allow_unsolvable: true,
                ..Default::default()
            },
            |_| true,
        );

        assert_eq!(run_metrics.solved_epoch(), None);
        assert_eq!(run_metrics.epochs().len(), 3);

        let first_epoch = &run_metrics.epochs()[1];
        assert_eq!(first_epoch.mutation_rate(), 0.0);
        assert_eq!(first_epoch.elite_ratio(), 0.25);
        assert_eq!(first_epoch.offspring_count(), 0);
        assert_eq!(first_epoch.local_search_improvements(), 0);
        assert_eq!(first_epoch.stagnation_epochs(), 1);
        assert_eq!(first_epoch.diversity_replacements(), 0);
        assert!(
            first_epoch.average_conflicts_sum()
                >= f32::from(u16::try_from(first_epoch.best_conflicts_sum()).unwrap())
        );
        assert!(first_epoch.unique_chromosomes() > 0);

        let second_epoch = &run_metrics.epochs()[2];
        assert_eq!(second_epoch.stagnation_epochs(), 2);
        assert!(second_epoch.elite_ratio() < first_epoch.elite_ratio());
    }

    #[test]
    fn test_run_preserves_best_chromosome_seen_across_epochs() {
        let mut genetic_algorithm = build_genetic_algorithm(
            GaConfig::new(12, 20, 100, 123)
                .with_mutation_rate(1.0)
                .with_elite_ratio(0.0)
                .with_offspring_ratio(1.0),
        )
        .expect("valid config should build");

        let run_metrics = genetic_algorithm.run_algorithm();
        let best_seen_conflicts = run_metrics
            .epochs()
            .iter()
            .map(super::EpochMetrics::best_conflicts_sum)
            .min()
            .expect("run should record at least the initial epoch");

        assert_eq!(
            genetic_algorithm.get_best_chromosome().get_conflicts_sum(),
            best_seen_conflicts
        );
        assert!(
            genetic_algorithm
                .get_population_best_chromosome()
                .get_conflicts_sum()
                >= best_seen_conflicts
        );
    }

    #[test]
    fn test_run_metrics_mark_initial_solve_epoch() {
        let mut genetic_algorithm = build_genetic_algorithm(
            GaConfig::new(1, 8, 10, 42)
                .with_mutation_rate(DEFAULT_MUTATION_RATE)
                .with_elite_ratio(DEFAULT_ELITE_RATIO),
        )
        .expect("valid config should build");

        let run_metrics = genetic_algorithm.run_algorithm();
        assert_eq!(run_metrics.solved_epoch(), Some(0));
        assert_eq!(run_metrics.epochs()[0].best_conflicts_sum(), 0);
    }

    #[test]
    fn test_stagnation_reset_interval_is_bounded() {
        assert_eq!(super::stagnation_reset_interval(10), 50);
        assert_eq!(super::stagnation_reset_interval(5_000), 250);
        assert_eq!(super::stagnation_reset_interval(100_000), 500);
    }

    #[test]
    fn test_adaptive_ga_parameters_follow_stagnation() {
        let stagnation_reset_interval = 200;

        let (mutation_fresh, elite_fresh) =
            super::adaptive_ga_parameters(0.08, 0.10, 0, stagnation_reset_interval);
        let (mutation_mid, elite_mid) =
            super::adaptive_ga_parameters(0.08, 0.10, 100, stagnation_reset_interval);
        let (mutation_stale, elite_stale) =
            super::adaptive_ga_parameters(0.08, 0.10, 200, stagnation_reset_interval);

        assert_eq!(mutation_fresh, 0.08);
        assert_eq!(elite_fresh, 0.10);
        assert!(mutation_mid > mutation_fresh);
        assert!(mutation_stale > mutation_mid);
        assert!(elite_mid < elite_fresh);
        assert!(elite_stale < elite_mid);
        assert!(elite_stale >= super::MIN_ADAPTIVE_ELITE_RATIO);
    }
}
