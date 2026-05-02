use std::{collections::HashSet, error::Error, fmt, time::Instant};

use rand::{Rng, RngExt, SeedableRng, rngs::StdRng, seq::SliceRandom};
use rayon::prelude::*;

use self::chromosome::Chromosome;

pub mod chromosome;

const TARGET_EPOCH_PROGRESS_LOGS: u32 = 20;
const TARGET_STAGNATION_RESETS: u32 = 20;
const MIN_STAGNATION_RESET_EPOCHS: u32 = 50;
const MAX_STAGNATION_RESET_EPOCHS: u32 = 500;
const SOFT_RESTART_ELITE_RATIO_SCALE: f32 = 0.4;
const MUTATION_STAGNATION_BOOST_SCALE: f32 = 3.0;
const MAX_ADAPTIVE_MUTATION_RATE: f32 = 0.60;
const MIN_ADAPTIVE_ELITE_RATIO: f32 = 0.01;
const MIN_ADAPTIVE_ELITE_RATIO_SCALE: f32 = 0.25;
pub const DEFAULT_MUTATION_RATE: f32 = 0.08;
pub const DEFAULT_ELITE_RATIO: f32 = 0.10;
pub const DEFAULT_OFFSPRING_RATIO: f32 = 0.10;
pub const DEFAULT_MIN_DIVERSITY_RATIO: f32 = 0.10;
pub const DEFAULT_SELECTION_STRATEGY: SelectionStrategy = SelectionStrategy::Roulette;
pub const DEFAULT_TOURNAMENT_SIZE: usize = 3;
pub const DEFAULT_LOCAL_SEARCH_RATE: f32 = 0.0;
pub const DEFAULT_LOCAL_SEARCH_ATTEMPTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionStrategy {
    Roulette,
    Tournament,
}

impl fmt::Display for SelectionStrategy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Roulette => formatter.write_str("roulette"),
            Self::Tournament => formatter.write_str("tournament"),
        }
    }
}

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
}

#[derive(Debug, Clone, Copy)]
struct EpochRecordContext {
    mutation_rate: f32,
    elite_ratio: f32,
    offspring_count: usize,
    local_search_improvements: usize,
    stagnation_epochs: u32,
    diversity_replacements: usize,
    elapsed_ms: u128,
}

impl EpochMetrics {
    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    pub fn best_conflicts_sum(&self) -> u32 {
        self.best_conflicts_sum
    }

    pub fn population_size(&self) -> usize {
        self.population_size
    }

    pub fn elapsed_ms(&self) -> u128 {
        self.elapsed_ms
    }

    pub fn average_conflicts_sum(&self) -> f32 {
        self.average_conflicts_sum
    }

    pub fn unique_chromosomes(&self) -> usize {
        self.unique_chromosomes
    }

    pub fn diversity_ratio(&self) -> f32 {
        if self.population_size == 0 {
            0.0
        } else {
            self.unique_chromosomes as f32 / self.population_size as f32
        }
    }

    pub fn mutation_rate(&self) -> f32 {
        self.mutation_rate
    }

    pub fn elite_ratio(&self) -> f32 {
        self.elite_ratio
    }

    pub fn offspring_count(&self) -> usize {
        self.offspring_count
    }

    pub fn local_search_improvements(&self) -> usize {
        self.local_search_improvements
    }

    pub fn stagnation_epochs(&self) -> u32 {
        self.stagnation_epochs
    }

    pub fn diversity_replacements(&self) -> usize {
        self.diversity_replacements
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunMetrics {
    epochs: Vec<EpochMetrics>,
    solved_epoch: Option<u32>,
    total_elapsed_ms: u128,
}

#[derive(Debug, Clone)]
pub struct EpochSnapshot {
    metrics: EpochMetrics,
    best_positions: Vec<u16>,
    best_conflicts: Vec<u32>,
    best_conflicts_sum: u32,
}

impl RunMetrics {
    pub fn epochs(&self) -> &[EpochMetrics] {
        &self.epochs
    }

    pub fn solved_epoch(&self) -> Option<u32> {
        self.solved_epoch
    }

    pub fn total_elapsed_ms(&self) -> u128 {
        self.total_elapsed_ms
    }

    fn record_epoch(&mut self, epoch: u32, population: &[Chromosome], context: EpochRecordContext) {
        let (best_conflicts_sum, average_conflicts_sum, unique_chromosomes) =
            population_metrics(population);

        self.epochs.push(EpochMetrics {
            epoch,
            best_conflicts_sum,
            population_size: population.len(),
            elapsed_ms: context.elapsed_ms,
            average_conflicts_sum,
            unique_chromosomes,
            mutation_rate: context.mutation_rate,
            elite_ratio: context.elite_ratio,
            offspring_count: context.offspring_count,
            local_search_improvements: context.local_search_improvements,
            stagnation_epochs: context.stagnation_epochs,
            diversity_replacements: context.diversity_replacements,
        });
    }

    fn mark_solved(&mut self, solved_epoch: u32) {
        self.solved_epoch = Some(solved_epoch);
    }

    fn set_total_elapsed_ms(&mut self, total_elapsed_ms: u128) {
        self.total_elapsed_ms = total_elapsed_ms;
    }
}

impl EpochSnapshot {
    pub fn metrics(&self) -> &EpochMetrics {
        &self.metrics
    }

    pub fn best_positions(&self) -> &[u16] {
        &self.best_positions
    }

    pub fn best_conflicts(&self) -> &[u32] {
        &self.best_conflicts
    }

    pub fn best_conflicts_sum(&self) -> u32 {
        self.best_conflicts_sum
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GaConfig {
    pub size: u16,
    pub initial_population: usize,
    pub max_epoch_count: u32,
    pub seed: u64,
    pub mutation_rate: f32,
    pub elite_ratio: f32,
    pub offspring_ratio: f32,
    pub min_diversity_ratio: f32,
    pub selection_strategy: SelectionStrategy,
    pub tournament_size: usize,
    pub local_search_rate: f32,
    pub local_search_attempts: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaConfigError {
    BoardSizeZero,
    InitialPopulationZero,
    MaxEpochCountZero,
    InvalidMutationRate,
    InvalidEliteRatio,
    InvalidOffspringRatio,
    InvalidMinDiversityRatio,
    InvalidLocalSearchRate,
    TournamentSizeZero,
}

impl fmt::Display for GaConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BoardSizeZero => formatter.write_str("board size must be greater than 0"),
            Self::InitialPopulationZero => {
                formatter.write_str("initial population must be greater than 0")
            }
            Self::MaxEpochCountZero => {
                formatter.write_str("max epoch count must be greater than 0")
            }
            Self::InvalidMutationRate => {
                formatter.write_str("mutation rate must be finite and between 0.0 and 1.0")
            }
            Self::InvalidEliteRatio => {
                formatter.write_str("elite ratio must be finite and between 0.0 and 1.0")
            }
            Self::InvalidOffspringRatio => {
                formatter.write_str("offspring ratio must be finite and between 0.0 and 1.0")
            }
            Self::InvalidMinDiversityRatio => formatter
                .write_str("minimum diversity ratio must be finite and between 0.0 and 1.0"),
            Self::InvalidLocalSearchRate => {
                formatter.write_str("local search rate must be finite and between 0.0 and 1.0")
            }
            Self::TournamentSizeZero => {
                formatter.write_str("tournament size must be greater than 0")
            }
        }
    }
}

impl Error for GaConfigError {}

impl GaConfig {
    pub fn new(size: u16, initial_population: usize, max_epoch_count: u32, seed: u64) -> Self {
        Self {
            size,
            initial_population,
            max_epoch_count,
            seed,
            mutation_rate: DEFAULT_MUTATION_RATE,
            elite_ratio: DEFAULT_ELITE_RATIO,
            offspring_ratio: DEFAULT_OFFSPRING_RATIO,
            min_diversity_ratio: DEFAULT_MIN_DIVERSITY_RATIO,
            selection_strategy: DEFAULT_SELECTION_STRATEGY,
            tournament_size: DEFAULT_TOURNAMENT_SIZE,
            local_search_rate: DEFAULT_LOCAL_SEARCH_RATE,
            local_search_attempts: DEFAULT_LOCAL_SEARCH_ATTEMPTS,
        }
    }

    pub fn try_new(
        size: u16,
        initial_population: usize,
        max_epoch_count: u32,
        seed: u64,
    ) -> Result<Self, GaConfigError> {
        Self::new(size, initial_population, max_epoch_count, seed).validated()
    }

    pub fn with_mutation_rate(mut self, mutation_rate: f32) -> Self {
        self.mutation_rate = mutation_rate;
        self
    }

    pub fn with_elite_ratio(mut self, elite_ratio: f32) -> Self {
        self.elite_ratio = elite_ratio;
        self
    }

    pub fn with_offspring_ratio(mut self, offspring_ratio: f32) -> Self {
        self.offspring_ratio = offspring_ratio;
        self
    }

    pub fn with_min_diversity_ratio(mut self, min_diversity_ratio: f32) -> Self {
        self.min_diversity_ratio = min_diversity_ratio;
        self
    }

    pub fn with_selection_strategy(mut self, selection_strategy: SelectionStrategy) -> Self {
        self.selection_strategy = selection_strategy;
        self
    }

    pub fn with_tournament_size(mut self, tournament_size: usize) -> Self {
        self.tournament_size = tournament_size;
        self
    }

    pub fn with_local_search_rate(mut self, local_search_rate: f32) -> Self {
        self.local_search_rate = local_search_rate;
        self
    }

    pub fn with_local_search_attempts(mut self, local_search_attempts: usize) -> Self {
        self.local_search_attempts = local_search_attempts;
        self
    }

    pub fn validated(self) -> Result<Self, GaConfigError> {
        self.validate()?;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), GaConfigError> {
        if self.size == 0 {
            return Err(GaConfigError::BoardSizeZero);
        }

        if self.initial_population == 0 {
            return Err(GaConfigError::InitialPopulationZero);
        }

        if self.max_epoch_count == 0 {
            return Err(GaConfigError::MaxEpochCountZero);
        }

        if !is_unit_interval(self.mutation_rate) {
            return Err(GaConfigError::InvalidMutationRate);
        }

        if !is_unit_interval(self.elite_ratio) {
            return Err(GaConfigError::InvalidEliteRatio);
        }

        if !is_unit_interval(self.offspring_ratio) {
            return Err(GaConfigError::InvalidOffspringRatio);
        }

        if !is_unit_interval(self.min_diversity_ratio) {
            return Err(GaConfigError::InvalidMinDiversityRatio);
        }

        if !is_unit_interval(self.local_search_rate) {
            return Err(GaConfigError::InvalidLocalSearchRate);
        }

        if self.tournament_size == 0 {
            return Err(GaConfigError::TournamentSizeZero);
        }

        Ok(())
    }
}

pub struct GeneticAlgorithm {
    population: Vec<Chromosome>,
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

impl GeneticAlgorithm {
    fn new(population: Vec<Chromosome>, rng: StdRng, params: GeneticAlgorithmParams) -> Self {
        Self {
            population,
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

    pub fn get_population_size(&self) -> usize {
        self.population.len()
    }

    pub fn run_algorithm(&mut self) -> RunMetrics {
        self.run_algorithm_with_progress(|_| true)
    }

    pub fn run_algorithm_with_progress<F>(&mut self, mut on_epoch: F) -> RunMetrics
    where
        F: FnMut(&EpochSnapshot) -> bool,
    {
        let started_at = Instant::now();
        let mut run_metrics = RunMetrics::default();

        if self.population.is_empty() {
            log::warn!("cannot run algorithm with empty population");
            run_metrics.set_total_elapsed_ms(started_at.elapsed().as_millis());
            return run_metrics;
        }

        self.calc_fitness();
        let mut best_conflicts_sum = self.get_best_chromosome().get_conflicts_sum();
        let offspring_count =
            offspring_count_for_population(self.target_population_size, self.offspring_ratio);
        run_metrics.record_epoch(
            0,
            &self.population,
            EpochRecordContext {
                mutation_rate: self.mutation_rate,
                elite_ratio: self.elite_ratio,
                offspring_count,
                local_search_improvements: 0,
                stagnation_epochs: 0,
                diversity_replacements: 0,
                elapsed_ms: started_at.elapsed().as_millis(),
            },
        );

        if best_conflicts_sum == 0 {
            log::info!("ga solved in initial population");
            run_metrics.mark_solved(0);
            run_metrics.set_total_elapsed_ms(started_at.elapsed().as_millis());
            self.report_latest_epoch(&run_metrics, &mut on_epoch);
            return run_metrics;
        }

        if !self.report_latest_epoch(&run_metrics, &mut on_epoch) {
            run_metrics.set_total_elapsed_ms(started_at.elapsed().as_millis());
            return run_metrics;
        }

        let progress_log_interval = epoch_progress_log_interval(self.max_epoch_count);
        let stagnation_reset_interval = stagnation_reset_interval(self.max_epoch_count);
        let mut stagnation_epochs = 0;
        log::info!(
            "running ga epochs={} population_size={} progress_log_interval={} stagnation_reset_interval={} initial_best_conflicts_sum={best_conflicts_sum} base_mutation_rate={} base_elite_ratio={} offspring_ratio={} offspring_count={} selection_strategy={} tournament_size={} local_search_rate={} local_search_attempts={}",
            self.max_epoch_count,
            self.get_population_size(),
            progress_log_interval,
            stagnation_reset_interval,
            self.mutation_rate,
            self.elite_ratio,
            self.offspring_ratio,
            offspring_count,
            self.selection_strategy,
            self.tournament_size,
            self.local_search_rate,
            self.local_search_attempts,
        );

        for epoch in 0..self.max_epoch_count {
            let epoch_number = epoch + 1;

            if stagnation_epochs >= stagnation_reset_interval {
                let (_, reset_elite_ratio) = adaptive_ga_parameters(
                    self.mutation_rate,
                    self.elite_ratio,
                    stagnation_epochs,
                    stagnation_reset_interval,
                );
                let replaced_count = self.soft_restart_population(reset_elite_ratio);
                self.calc_fitness();

                let post_reset_best_conflicts_sum = self.get_best_chromosome().get_conflicts_sum();
                best_conflicts_sum = best_conflicts_sum.min(post_reset_best_conflicts_sum);

                log::info!(
                    "ga stagnation reset epoch={epoch_number} stagnant_epochs={stagnation_epochs} replaced={replaced_count} best_conflicts_sum={post_reset_best_conflicts_sum} elite_ratio={reset_elite_ratio:.4} population_size={}",
                    self.get_population_size(),
                );

                stagnation_epochs = 0;
            }

            let (epoch_mutation_rate, epoch_elite_ratio) = adaptive_ga_parameters(
                self.mutation_rate,
                self.elite_ratio,
                stagnation_epochs,
                stagnation_reset_interval,
            );

            self.mate_random_chromosomes(offspring_count);
            self.mutate_population(epoch_mutation_rate, epoch_elite_ratio);
            let local_search_improvements =
                self.improve_population_with_local_search(epoch_elite_ratio);
            self.select_survivors(epoch_elite_ratio);
            self.calc_fitness();

            let diversity_replacements = self.refresh_low_diversity_population(epoch_elite_ratio);
            if diversity_replacements > 0 {
                self.calc_fitness();
            }

            let epoch_best_conflicts_sum = self.get_best_chromosome().get_conflicts_sum();
            let population_size = self.get_population_size();

            let is_improvement = epoch_best_conflicts_sum < best_conflicts_sum;
            if is_improvement {
                best_conflicts_sum = epoch_best_conflicts_sum;
                stagnation_epochs = 0;
            } else {
                stagnation_epochs += 1;
            }

            run_metrics.record_epoch(
                epoch_number,
                &self.population,
                EpochRecordContext {
                    mutation_rate: epoch_mutation_rate,
                    elite_ratio: epoch_elite_ratio,
                    offspring_count,
                    local_search_improvements,
                    stagnation_epochs,
                    diversity_replacements,
                    elapsed_ms: started_at.elapsed().as_millis(),
                },
            );

            if epoch_best_conflicts_sum == 0 {
                log::info!(
                    "ga solved epoch={epoch_number} population_size={population_size} mutation_rate={epoch_mutation_rate:.4} elite_ratio={epoch_elite_ratio:.4} local_search_improvements={local_search_improvements}"
                );
                run_metrics.mark_solved(epoch_number);
                run_metrics.set_total_elapsed_ms(started_at.elapsed().as_millis());
                self.report_latest_epoch(&run_metrics, &mut on_epoch);
                return run_metrics;
            }

            if is_improvement {
                log::info!(
                    "ga improvement epoch={epoch_number} best_conflicts_sum={best_conflicts_sum} population_size={population_size} mutation_rate={epoch_mutation_rate:.4} elite_ratio={epoch_elite_ratio:.4} local_search_improvements={local_search_improvements}",
                );
                if !self.report_latest_epoch(&run_metrics, &mut on_epoch) {
                    run_metrics.set_total_elapsed_ms(started_at.elapsed().as_millis());
                    return run_metrics;
                }
                continue;
            }

            let is_periodic_log = epoch_number % progress_log_interval == 0;
            let is_last_epoch = epoch_number == self.max_epoch_count;
            if is_periodic_log || is_last_epoch {
                log::info!(
                    "ga progress epoch={epoch_number} best_conflicts_sum={best_conflicts_sum} population_size={population_size} stagnant_epochs={stagnation_epochs} mutation_rate={epoch_mutation_rate:.4} elite_ratio={epoch_elite_ratio:.4} local_search_improvements={local_search_improvements} diversity_replacements={diversity_replacements}",
                );
            }

            if !self.report_latest_epoch(&run_metrics, &mut on_epoch) {
                run_metrics.set_total_elapsed_ms(started_at.elapsed().as_millis());
                return run_metrics;
            }
        }

        log::warn!(
            "no solution found best_conflicts_sum={best_conflicts_sum} epochs={} population_size={}",
            self.max_epoch_count,
            self.get_population_size(),
        );

        run_metrics.set_total_elapsed_ms(started_at.elapsed().as_millis());
        run_metrics
    }

    fn report_latest_epoch<F>(&self, run_metrics: &RunMetrics, on_epoch: &mut F) -> bool
    where
        F: FnMut(&EpochSnapshot) -> bool,
    {
        let Some(metrics) = run_metrics.epochs().last() else {
            return true;
        };

        let best_chromosome = self.get_best_chromosome();
        let snapshot = EpochSnapshot {
            metrics: metrics.clone(),
            best_positions: best_chromosome.get_positions().to_vec(),
            best_conflicts: best_chromosome.get_conflicts().to_vec(),
            best_conflicts_sum: best_chromosome.get_conflicts_sum(),
        };

        on_epoch(&snapshot)
    }

    pub fn get_best_chromosome(&self) -> &Chromosome {
        self.population
            .iter()
            .min_by_key(|chromosome| chromosome.get_conflicts_sum())
            .expect("population is never empty while running")
    }

    pub fn get_worst_chromosome(&self) -> &Chromosome {
        self.population
            .iter()
            .max_by_key(|chromosome| chromosome.get_conflicts_sum())
            .expect("population is never empty while running")
    }

    fn calc_fitness(&mut self) {
        if self.population.is_empty() {
            return;
        }

        let most_conflicts = self.get_worst_chromosome().get_conflicts_sum() as f32;
        let least_conflicts = self.get_best_chromosome().get_conflicts_sum() as f32;
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

    fn mate_random_chromosomes(&mut self, offspring_count: usize) {
        if self.population.is_empty() || offspring_count == 0 {
            return;
        }

        let roulette_selection = match self.selection_strategy {
            SelectionStrategy::Roulette => {
                let cumulative_fitness = cumulative_fitness(&self.population);
                let fitness_sum = cumulative_fitness.last().copied().unwrap_or_default();
                if fitness_sum <= f32::EPSILON {
                    log::debug!("fitness sum is near zero; selecting parents uniformly at random");
                }

                Some((cumulative_fitness, fitness_sum))
            }
            SelectionStrategy::Tournament => None,
        };

        log::debug!(
            "select random chromosomes [offspring_count={offspring_count}, selection_strategy={} tournament_size={}]",
            self.selection_strategy,
            self.tournament_size,
        );

        for _ in 0..offspring_count {
            let Some(parent_one_index) = self.select_parent_index(roulette_selection.as_ref().map(
                |(cumulative_fitness, fitness_sum)| (cumulative_fitness.as_slice(), *fitness_sum),
            )) else {
                break;
            };
            let Some(parent_two_index) = self.select_parent_index(roulette_selection.as_ref().map(
                |(cumulative_fitness, fitness_sum)| (cumulative_fitness.as_slice(), *fitness_sum),
            )) else {
                break;
            };

            let population = &self.population;
            let rng = &mut self.rng;
            let child = mate_chromosomes(
                population[parent_one_index].get_positions(),
                population[parent_two_index].get_positions(),
                rng,
            );
            self.population.push(child);
        }
    }

    fn select_parent_index(&mut self, roulette_selection: Option<(&[f32], f32)>) -> Option<usize> {
        match self.selection_strategy {
            SelectionStrategy::Roulette => {
                let (cumulative_fitness, fitness_sum) = roulette_selection?;
                self.select_roulette_parent_index(cumulative_fitness, fitness_sum)
            }
            SelectionStrategy::Tournament => self.select_tournament_parent_index(),
        }
    }

    fn select_roulette_parent_index(
        &mut self,
        cumulative_fitness: &[f32],
        fitness_sum: f32,
    ) -> Option<usize> {
        if cumulative_fitness.is_empty() {
            return None;
        }

        if fitness_sum <= f32::EPSILON || !fitness_sum.is_finite() {
            return Some(self.rng.random_range(0..cumulative_fitness.len()));
        }

        if let Some(index) = self.select_random_chromosome_index(cumulative_fitness, fitness_sum) {
            return Some(index);
        }

        Some(self.rng.random_range(0..cumulative_fitness.len()))
    }

    fn select_tournament_parent_index(&mut self) -> Option<usize> {
        let population_size = self.population.len();
        if population_size == 0 {
            return None;
        }

        if self.tournament_size >= population_size {
            return self
                .population
                .iter()
                .enumerate()
                .min_by_key(|(_, chromosome)| chromosome.get_conflicts_sum())
                .map(|(index, _)| index);
        }

        let mut best_index = self.rng.random_range(0..population_size);
        for _ in 1..self.tournament_size {
            let candidate_index = self.rng.random_range(0..population_size);
            if self.population[candidate_index].get_conflicts_sum()
                < self.population[best_index].get_conflicts_sum()
            {
                best_index = candidate_index;
            }
        }

        Some(best_index)
    }

    fn select_random_chromosome_index(
        &mut self,
        cumulative_fitness: &[f32],
        fitness_sum: f32,
    ) -> Option<usize> {
        if cumulative_fitness.is_empty() || fitness_sum <= f32::EPSILON || !fitness_sum.is_finite()
        {
            return None;
        }

        let roulette_spin = self.rng.random_range(0.0..fitness_sum);
        let index =
            cumulative_fitness.partition_point(|selection_rank| *selection_rank < roulette_spin);
        let index = index.min(cumulative_fitness.len() - 1);

        log::trace!(
            "selecting chromosome index={index} selection_rank={} roulette_spin={roulette_spin}",
            cumulative_fitness[index],
        );

        Some(index)
    }

    fn mutate_population(&mut self, mutation_rate: f32, elite_ratio: f32) {
        if self.population.len() < 2 || mutation_rate <= 0.0 || !mutation_rate.is_finite() {
            return;
        }

        let elite_ratio = normalize_unit_interval(elite_ratio, self.elite_ratio);
        let elite_count = elite_count_for_population(
            self.target_population_size,
            self.population.len(),
            elite_ratio,
        );
        select_elites_to_front(&mut self.population, elite_count);

        let mut planned_swaps = vec![None; self.population.len()];
        let rng = &mut self.rng;

        for (index, chromosome) in self.population.iter().enumerate() {
            if index < elite_count {
                continue;
            }

            if rng.random::<f32>() < mutation_rate {
                let chromosome_size = chromosome.get_positions().len();
                if chromosome_size < 2 {
                    continue;
                }

                let index_one = rng.random_range(0..chromosome_size);
                let mut index_two = rng.random_range(0..(chromosome_size - 1));
                if index_two >= index_one {
                    index_two += 1;
                }

                planned_swaps[index] = Some((index_one, index_two));
            }
        }

        self.population
            .par_iter_mut()
            .enumerate()
            .for_each(|(index, chromosome)| {
                if let Some((index_one, index_two)) = planned_swaps[index] {
                    chromosome.mutate_swap_at(index_one, index_two);
                }
            });
    }

    fn improve_population_with_local_search(&mut self, elite_ratio: f32) -> usize {
        if self.population.len() < 2
            || self.local_search_rate <= 0.0
            || self.local_search_attempts == 0
        {
            return 0;
        }

        let elite_ratio = normalize_unit_interval(elite_ratio, self.elite_ratio);
        let elite_count = elite_count_for_population(
            self.target_population_size,
            self.population.len(),
            elite_ratio,
        );
        select_elites_to_front(&mut self.population, elite_count);

        let non_elite_count = self.population.len().saturating_sub(elite_count);
        let candidate_count = local_search_candidate_count(non_elite_count, self.local_search_rate);
        if candidate_count == 0 {
            return 0;
        }

        let mut candidate_indices = (elite_count..self.population.len()).collect::<Vec<_>>();
        candidate_indices.shuffle(&mut self.rng);
        candidate_indices.truncate(candidate_count);

        let attempts = self.local_search_attempts;
        let rng = &mut self.rng;
        let population = &mut self.population;
        candidate_indices
            .into_iter()
            .filter(|&index| {
                improve_chromosome_with_local_search(&mut population[index], attempts, rng)
            })
            .count()
    }

    fn select_survivors(&mut self, elite_ratio: f32) {
        if self.population.len() <= self.target_population_size {
            return;
        }

        let elite_ratio = normalize_unit_interval(elite_ratio, self.elite_ratio);

        let elite_count = elite_count_for_population(
            self.target_population_size,
            self.population.len(),
            elite_ratio,
        );
        select_elites_to_front(&mut self.population, elite_count);

        let mut survivors = self.population.drain(..elite_count).collect::<Vec<_>>();

        while survivors.len() < self.target_population_size {
            if self.population.is_empty() {
                break;
            }

            let random_index = self.rng.random_range(0..self.population.len());
            survivors.push(self.population.swap_remove(random_index));
        }

        self.population = survivors;
    }

    fn refresh_low_diversity_population(&mut self, elite_ratio: f32) -> usize {
        if self.population.is_empty() || self.min_diversity_ratio <= 0.0 {
            return 0;
        }

        let min_unique_chromosomes =
            minimum_unique_chromosomes(self.target_population_size, self.min_diversity_ratio);
        let unique_chromosomes = count_unique_chromosomes(&self.population);
        if unique_chromosomes >= min_unique_chromosomes {
            return 0;
        }

        let elite_ratio = normalize_unit_interval(elite_ratio, self.elite_ratio);
        let elite_count = elite_count_for_population(
            self.target_population_size,
            self.population.len(),
            elite_ratio,
        );
        let replaceable_count = self.population.len().saturating_sub(elite_count);
        let replacement_count =
            (min_unique_chromosomes - unique_chromosomes).min(replaceable_count);
        if replacement_count == 0 {
            return 0;
        }

        self.population
            .sort_by_key(|chromosome| chromosome.get_conflicts_sum());

        let board_size = self
            .population
            .first()
            .map(|chromosome| chromosome.get_positions().len())
            .unwrap_or(0);
        let board_size = u16::try_from(board_size).expect("board size should fit into u16");

        for chromosome in self.population.iter_mut().rev().take(replacement_count) {
            let positions =
                chromosome::generate_distinct_random_values_with_rng(board_size, &mut self.rng);
            *chromosome = Chromosome::new(positions);
        }

        log::info!(
            "ga diversity refresh unique_chromosomes={unique_chromosomes} min_unique_chromosomes={min_unique_chromosomes} replaced={replacement_count}"
        );

        replacement_count
    }

    fn soft_restart_population(&mut self, elite_ratio: f32) -> usize {
        if self.population.is_empty() {
            return 0;
        }

        let elite_ratio = normalize_unit_interval(elite_ratio, self.elite_ratio);

        self.population
            .sort_by_key(|chromosome| chromosome.get_conflicts_sum());

        let board_size = self
            .population
            .first()
            .map(|chromosome| chromosome.get_positions().len())
            .unwrap_or(0);
        let board_size = u16::try_from(board_size).expect("board size should fit into u16");

        let mut elite_count =
            ((self.target_population_size as f32) * elite_ratio * SOFT_RESTART_ELITE_RATIO_SCALE)
                .round() as usize;
        elite_count = elite_count
            .max(1)
            .min(self.target_population_size)
            .min(self.population.len());

        if self.target_population_size > 1 {
            elite_count = elite_count.min(self.target_population_size - 1);
        }

        self.population.truncate(elite_count);

        let mut replaced_count = 0;
        while self.population.len() < self.target_population_size {
            let positions =
                chromosome::generate_distinct_random_values_with_rng(board_size, &mut self.rng);
            self.population.push(Chromosome::new(positions));
            replaced_count += 1;
        }

        replaced_count
    }
}

pub fn build_genetic_algorithm(config: GaConfig) -> GeneticAlgorithm {
    let target_population_size = config.initial_population.max(1);
    let mutation_rate = normalize_unit_interval(config.mutation_rate, DEFAULT_MUTATION_RATE);
    let elite_ratio = normalize_unit_interval(config.elite_ratio, DEFAULT_ELITE_RATIO);
    let offspring_ratio = normalize_unit_interval(config.offspring_ratio, DEFAULT_OFFSPRING_RATIO);
    let min_diversity_ratio =
        normalize_unit_interval(config.min_diversity_ratio, DEFAULT_MIN_DIVERSITY_RATIO);
    let local_search_rate =
        normalize_unit_interval(config.local_search_rate, DEFAULT_LOCAL_SEARCH_RATE);
    let tournament_size = config.tournament_size.max(1);
    let mut rng = StdRng::seed_from_u64(config.seed);
    let mut population: Vec<Chromosome> = Vec::with_capacity(target_population_size);

    for _ in 0..target_population_size {
        let positions = chromosome::generate_distinct_random_values_with_rng(config.size, &mut rng);
        let chromosome = Chromosome::new(positions);
        population.push(chromosome);
    }

    GeneticAlgorithm::new(
        population,
        rng,
        GeneticAlgorithmParams {
            target_population_size,
            max_epoch_count: config.max_epoch_count,
            mutation_rate,
            elite_ratio,
            offspring_ratio,
            min_diversity_ratio,
            selection_strategy: config.selection_strategy,
            tournament_size,
            local_search_rate,
            local_search_attempts: config.local_search_attempts,
        },
    )
}

fn offspring_count_for_population(target_population_size: usize, offspring_ratio: f32) -> usize {
    if target_population_size == 0 || offspring_ratio <= 0.0 || !offspring_ratio.is_finite() {
        return 0;
    }

    ((target_population_size as f64) * f64::from(offspring_ratio))
        .round()
        .max(1.0) as usize
}

fn population_metrics(population: &[Chromosome]) -> (u32, f32, usize) {
    if population.is_empty() {
        return (0, 0.0, 0);
    }

    let mut best_conflicts_sum = u32::MAX;
    let mut total_conflicts_sum = 0u64;
    let mut unique_chromosomes = HashSet::with_capacity(population.len());

    for chromosome in population {
        let conflicts_sum = chromosome.get_conflicts_sum();
        best_conflicts_sum = best_conflicts_sum.min(conflicts_sum);
        total_conflicts_sum += u64::from(conflicts_sum);
        unique_chromosomes.insert(chromosome.get_positions());
    }

    (
        best_conflicts_sum,
        total_conflicts_sum as f32 / population.len() as f32,
        unique_chromosomes.len(),
    )
}

fn count_unique_chromosomes(population: &[Chromosome]) -> usize {
    population
        .iter()
        .map(|chromosome| chromosome.get_positions())
        .collect::<HashSet<_>>()
        .len()
}

fn minimum_unique_chromosomes(target_population_size: usize, min_diversity_ratio: f32) -> usize {
    if target_population_size == 0 || min_diversity_ratio <= 0.0 || !min_diversity_ratio.is_finite()
    {
        return 0;
    }

    let minimum = ((target_population_size as f32) * min_diversity_ratio).ceil() as usize;
    if target_population_size > 1 {
        minimum.max(2).min(target_population_size)
    } else {
        minimum.max(1)
    }
}

fn elite_count_for_population(
    target_population_size: usize,
    population_size: usize,
    elite_ratio: f32,
) -> usize {
    if target_population_size == 0
        || population_size == 0
        || elite_ratio <= 0.0
        || !elite_ratio.is_finite()
    {
        return 0;
    }

    let elite_count = ((target_population_size as f32) * elite_ratio).round() as usize;
    elite_count.min(target_population_size).min(population_size)
}

fn local_search_candidate_count(non_elite_count: usize, local_search_rate: f32) -> usize {
    if non_elite_count == 0 || local_search_rate <= 0.0 || !local_search_rate.is_finite() {
        return 0;
    }

    (((non_elite_count as f32) * local_search_rate)
        .round()
        .max(1.0) as usize)
        .min(non_elite_count)
}

fn improve_chromosome_with_local_search(
    chromosome: &mut Chromosome,
    attempts: usize,
    rng: &mut impl Rng,
) -> bool {
    let chromosome_size = chromosome.get_positions().len();
    if chromosome_size < 2 || attempts == 0 || chromosome.get_conflicts_sum() == 0 {
        return false;
    }

    let mut improved = false;
    for _ in 0..attempts {
        let current_conflicts_sum = chromosome.get_conflicts_sum();
        if current_conflicts_sum == 0 {
            break;
        }

        let index_one = rng.random_range(0..chromosome_size);
        let mut index_two = rng.random_range(0..(chromosome_size - 1));
        if index_two >= index_one {
            index_two += 1;
        }

        chromosome.mutate_swap_at(index_one, index_two);
        if chromosome.get_conflicts_sum() < current_conflicts_sum {
            improved = true;
        } else {
            chromosome.mutate_swap_at(index_one, index_two);
        }
    }

    improved
}

fn select_elites_to_front(population: &mut [Chromosome], elite_count: usize) {
    if elite_count == 0 || population.is_empty() {
        return;
    }

    let nth_elite_index = elite_count.saturating_sub(1).min(population.len() - 1);
    population
        .select_nth_unstable_by_key(nth_elite_index, |chromosome| chromosome.get_conflicts_sum());
}

fn cumulative_fitness(population: &[Chromosome]) -> Vec<f32> {
    let mut selection_rank = 0.0;
    population
        .iter()
        .map(|chromosome| {
            let fitness = chromosome.get_fitness();
            if fitness.is_finite() && fitness > 0.0 {
                selection_rank += fitness;
            }
            selection_rank
        })
        .collect()
}

fn is_unit_interval(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn normalize_unit_interval(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

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
        * (1.0 + MUTATION_STAGNATION_BOOST_SCALE * stagnation_ratio))
        .clamp(0.0, mutation_ceiling);

    let adaptive_elite_scale = 1.0 - ((1.0 - MIN_ADAPTIVE_ELITE_RATIO_SCALE) * stagnation_ratio);
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

fn mate_chromosomes(parent_one: &[u16], parent_two: &[u16], rng: &mut impl Rng) -> Chromosome {
    log::trace!("mate chromosomes");
    log::trace!("parent_one={parent_one:?}");
    log::trace!("parent_two={parent_two:?}");

    let child_genes = pmx(parent_one, parent_two, rng);
    let child = Chromosome::new(child_genes);

    log::trace!("child={child:?}");
    child
}

fn pmx(parent_one: &[u16], parent_two: &[u16], rng: &mut impl Rng) -> Vec<u16> {
    debug_assert_eq!(parent_one.len(), parent_two.len());

    let chromosome_size = parent_one.len();
    if chromosome_size <= 1 {
        return parent_one.to_vec();
    }

    let chromosome_half_size = chromosome_size / 2;
    let point_one = rng.random_range(0..chromosome_half_size);
    let point_two_exclusive = rng.random_range(chromosome_half_size..=chromosome_size);

    log::trace!(
        "partially mapped crossover [point_one={point_one}, point_two_exclusive={point_two_exclusive}]"
    );

    pmx_with_crossover_points(parent_one, parent_two, point_one, point_two_exclusive)
}

fn pmx_with_crossover_points(
    parent_one: &[u16],
    parent_two: &[u16],
    point_one: usize,
    point_two_exclusive: usize,
) -> Vec<u16> {
    debug_assert_eq!(parent_one.len(), parent_two.len());
    debug_assert!(point_one < point_two_exclusive);
    debug_assert!(point_two_exclusive <= parent_one.len());

    let chromosome_size = parent_one.len();

    let mut parent_two_positions = vec![usize::MAX; chromosome_size];
    for (index, &gene) in parent_two.iter().enumerate() {
        parent_two_positions[usize::from(gene)] = index;
    }

    let mut child_genes = vec![None; parent_one.len()];
    let mut child_used = vec![false; chromosome_size];

    for i in point_one..point_two_exclusive {
        let gene = parent_one[i];
        child_genes[i] = Some(gene);
        child_used[usize::from(gene)] = true;
    }

    log::trace!("child positions one: {child_genes:?}");

    for (i, &gene) in parent_two
        .iter()
        .enumerate()
        .take(point_two_exclusive)
        .skip(point_one)
    {
        if !child_used[usize::from(gene)] {
            let position = find_position(i, parent_one, &parent_two_positions, &child_genes);
            child_genes[position] = Some(gene);
            child_used[usize::from(gene)] = true;
        }
    }

    log::trace!("child positions two: {child_genes:?}");

    for i in 0..chromosome_size {
        if child_genes[i].is_none() {
            child_genes[i] = Some(parent_two[i]);
        }
    }

    log::trace!("child positions three: {child_genes:?}");
    child_genes
        .iter()
        .map(|gene| gene.expect("pmx child should not contain empty genes"))
        .collect()
}

fn find_position(
    index: usize,
    parent_one: &[u16],
    parent_two_positions: &[usize],
    child: &[Option<u16>],
) -> usize {
    let mut current_index = index;

    loop {
        let mapped_gene = usize::from(parent_one[current_index]);
        let position = *parent_two_positions
            .get(mapped_gene)
            .expect("parent one genes should fit parent two index map");
        assert_ne!(
            position,
            usize::MAX,
            "parent one genes should always exist in parent two"
        );

        log::trace!("checking position {position}");
        if child[position].is_none() {
            return position;
        }

        current_index = position;
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};

    use super::{
        DEFAULT_ELITE_RATIO, DEFAULT_LOCAL_SEARCH_ATTEMPTS, DEFAULT_LOCAL_SEARCH_RATE,
        DEFAULT_MIN_DIVERSITY_RATIO, DEFAULT_MUTATION_RATE, DEFAULT_OFFSPRING_RATIO,
        DEFAULT_SELECTION_STRATEGY, DEFAULT_TOURNAMENT_SIZE, GaConfig, GaConfigError,
        GeneticAlgorithm, GeneticAlgorithmParams, SelectionStrategy, build_genetic_algorithm,
        chromosome::Chromosome, pmx,
    };

    fn build_test_algorithm(population: Vec<Chromosome>) -> GeneticAlgorithm {
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

    fn shuffled_values(size: usize, seed: u64) -> Vec<u16> {
        let mut values =
            (0..u16::try_from(size).expect("size should fit into u16")).collect::<Vec<_>>();
        let mut rng = StdRng::seed_from_u64(seed);
        values.shuffle(&mut rng);
        values
    }

    #[test]
    fn test_config_validation_accepts_valid_config() {
        let config = GaConfig::try_new(8, 32, 100, 42)
            .expect("valid default config should pass validation")
            .with_mutation_rate(0.25)
            .with_elite_ratio(0.20)
            .with_offspring_ratio(0.50)
            .with_min_diversity_ratio(0.15)
            .with_selection_strategy(SelectionStrategy::Tournament)
            .with_tournament_size(5)
            .with_local_search_rate(0.25)
            .with_local_search_attempts(12)
            .validated()
            .expect("valid customized config should pass validation");

        assert_eq!(config.size, 8);
        assert_eq!(config.initial_population, 32);
        assert_eq!(config.max_epoch_count, 100);
        assert_eq!(config.min_diversity_ratio, 0.15);
        assert_eq!(config.selection_strategy, SelectionStrategy::Tournament);
        assert_eq!(config.tournament_size, 5);
        assert_eq!(config.local_search_rate, 0.25);
        assert_eq!(config.local_search_attempts, 12);
    }

    #[test]
    fn test_config_validation_rejects_invalid_config() {
        assert_eq!(
            GaConfig::new(0, 32, 100, 42).validate(),
            Err(GaConfigError::BoardSizeZero)
        );
        assert_eq!(
            GaConfig::new(8, 0, 100, 42).validate(),
            Err(GaConfigError::InitialPopulationZero)
        );
        assert_eq!(
            GaConfig::new(8, 32, 0, 42).validate(),
            Err(GaConfigError::MaxEpochCountZero)
        );
        assert_eq!(
            GaConfig::new(8, 32, 100, 42)
                .with_mutation_rate(f32::NAN)
                .validate(),
            Err(GaConfigError::InvalidMutationRate)
        );
        assert_eq!(
            GaConfig::new(8, 32, 100, 42)
                .with_elite_ratio(1.1)
                .validate(),
            Err(GaConfigError::InvalidEliteRatio)
        );
        assert_eq!(
            GaConfig::new(8, 32, 100, 42)
                .with_offspring_ratio(-0.1)
                .validate(),
            Err(GaConfigError::InvalidOffspringRatio)
        );
        assert_eq!(
            GaConfig::new(8, 32, 100, 42)
                .with_min_diversity_ratio(1.1)
                .validate(),
            Err(GaConfigError::InvalidMinDiversityRatio)
        );
        assert_eq!(
            GaConfig::new(8, 32, 100, 42)
                .with_local_search_rate(f32::INFINITY)
                .validate(),
            Err(GaConfigError::InvalidLocalSearchRate)
        );
        assert_eq!(
            GaConfig::new(8, 32, 100, 42)
                .with_tournament_size(0)
                .validate(),
            Err(GaConfigError::TournamentSizeZero)
        );
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
    fn test_build_algorithm_handles_n_zero_and_one() {
        let mut zero_board = build_genetic_algorithm(
            GaConfig::new(0, 8, 10, 42)
                .with_mutation_rate(DEFAULT_MUTATION_RATE)
                .with_elite_ratio(DEFAULT_ELITE_RATIO),
        );
        zero_board.run_algorithm();
        assert_eq!(zero_board.get_best_chromosome().get_conflicts_sum(), 0);

        let mut one_board = build_genetic_algorithm(
            GaConfig::new(1, 8, 10, 42)
                .with_mutation_rate(DEFAULT_MUTATION_RATE)
                .with_elite_ratio(DEFAULT_ELITE_RATIO),
        );
        one_board.run_algorithm();
        assert_eq!(one_board.get_best_chromosome().get_conflicts_sum(), 0);
    }

    #[test]
    fn test_run_metrics_include_initial_epoch_and_elapsed_time() {
        let mut genetic_algorithm = build_genetic_algorithm(
            GaConfig::new(8, 32, 5, 42)
                .with_mutation_rate(DEFAULT_MUTATION_RATE)
                .with_elite_ratio(DEFAULT_ELITE_RATIO),
        );

        let run_metrics = genetic_algorithm.run_algorithm();
        assert!(!run_metrics.epochs().is_empty());
        let initial_epoch = &run_metrics.epochs()[0];
        assert_eq!(initial_epoch.epoch(), 0);
        assert!(run_metrics.total_elapsed_ms() >= initial_epoch.elapsed_ms());
        assert!(initial_epoch.average_conflicts_sum() >= initial_epoch.best_conflicts_sum() as f32);
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
        );
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
        );

        let run_metrics = genetic_algorithm.run_algorithm();

        assert_eq!(run_metrics.solved_epoch(), None);
        assert_eq!(run_metrics.epochs().len(), 3);

        let first_epoch = &run_metrics.epochs()[1];
        assert_eq!(first_epoch.mutation_rate(), 0.0);
        assert_eq!(first_epoch.elite_ratio(), 0.25);
        assert_eq!(first_epoch.offspring_count(), 0);
        assert_eq!(first_epoch.local_search_improvements(), 0);
        assert_eq!(first_epoch.stagnation_epochs(), 1);
        assert_eq!(first_epoch.diversity_replacements(), 0);
        assert!(first_epoch.average_conflicts_sum() >= first_epoch.best_conflicts_sum() as f32);
        assert!(first_epoch.unique_chromosomes() > 0);

        let second_epoch = &run_metrics.epochs()[2];
        assert_eq!(second_epoch.stagnation_epochs(), 2);
        assert!(second_epoch.elite_ratio() < first_epoch.elite_ratio());
    }

    #[test]
    fn test_run_metrics_mark_initial_solve_epoch() {
        let mut genetic_algorithm = build_genetic_algorithm(
            GaConfig::new(0, 8, 10, 42)
                .with_mutation_rate(DEFAULT_MUTATION_RATE)
                .with_elite_ratio(DEFAULT_ELITE_RATIO),
        );

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

    #[test]
    fn test_offspring_count_scales_with_population() {
        assert_eq!(super::offspring_count_for_population(40_000, 0.10), 4_000);
        assert_eq!(super::offspring_count_for_population(10, 0.2), 2);
        assert_eq!(super::offspring_count_for_population(10, 0.0), 0);
        assert_eq!(super::offspring_count_for_population(1, 0.001), 1);
    }

    #[test]
    fn test_elite_count_scales_with_target_population() {
        assert_eq!(
            super::elite_count_for_population(40_000, 44_000, 0.10),
            4_000
        );
        assert_eq!(super::elite_count_for_population(10, 12, 0.15), 2);
        assert_eq!(super::elite_count_for_population(10, 12, 0.0), 0);
        assert_eq!(super::elite_count_for_population(10, 1, 0.50), 1);
    }

    #[test]
    fn test_minimum_unique_chromosomes_scales_with_population() {
        assert_eq!(super::minimum_unique_chromosomes(0, 0.10), 0);
        assert_eq!(super::minimum_unique_chromosomes(1, 0.10), 1);
        assert_eq!(super::minimum_unique_chromosomes(8, 0.10), 2);
        assert_eq!(super::minimum_unique_chromosomes(40_000, 0.10), 4_000);
        assert_eq!(super::minimum_unique_chromosomes(10, 0.0), 0);
    }

    #[test]
    fn test_local_search_candidate_count_scales_with_non_elites() {
        assert_eq!(super::local_search_candidate_count(0, 0.50), 0);
        assert_eq!(super::local_search_candidate_count(10, 0.0), 0);
        assert_eq!(super::local_search_candidate_count(10, 0.01), 1);
        assert_eq!(super::local_search_candidate_count(10, 0.25), 3);
        assert_eq!(super::local_search_candidate_count(10, 1.0), 10);
    }

    #[test]
    fn test_local_search_keeps_only_improving_swaps() {
        let mut chromosome = Chromosome::new(vec![0, 1, 2, 3, 4, 5, 6, 7]);
        let initial_conflicts_sum = chromosome.get_conflicts_sum();
        let mut rng = StdRng::seed_from_u64(7);

        let improved = super::improve_chromosome_with_local_search(&mut chromosome, 200, &mut rng);

        assert!(improved);
        assert!(chromosome.get_conflicts_sum() < initial_conflicts_sum);

        let mut positions = chromosome.get_positions().to_vec();
        positions.sort_unstable();
        assert_eq!(positions, (0u16..8).collect::<Vec<_>>());
    }

    #[test]
    fn test_population_local_search_improves_selected_non_elites() {
        let solution = vec![0, 4, 7, 5, 2, 6, 1, 3];
        let duplicate = vec![0, 1, 2, 3, 4, 5, 6, 7];
        let mut population = vec![Chromosome::new(solution.clone())];
        population.extend((0..5).map(|_| Chromosome::new(duplicate.clone())));

        let mut genetic_algorithm = GeneticAlgorithm::new(
            population,
            StdRng::seed_from_u64(7),
            GeneticAlgorithmParams {
                target_population_size: 6,
                max_epoch_count: 10,
                mutation_rate: DEFAULT_MUTATION_RATE,
                elite_ratio: 0.20,
                offspring_ratio: DEFAULT_OFFSPRING_RATIO,
                min_diversity_ratio: DEFAULT_MIN_DIVERSITY_RATIO,
                selection_strategy: DEFAULT_SELECTION_STRATEGY,
                tournament_size: DEFAULT_TOURNAMENT_SIZE,
                local_search_rate: 1.0,
                local_search_attempts: 200,
            },
        );

        let initial_total_conflicts = genetic_algorithm
            .population
            .iter()
            .map(Chromosome::get_conflicts_sum)
            .sum::<u32>();

        let improvements = genetic_algorithm.improve_population_with_local_search(0.20);
        let final_total_conflicts = genetic_algorithm
            .population
            .iter()
            .map(Chromosome::get_conflicts_sum)
            .sum::<u32>();
        let positions = genetic_algorithm
            .population
            .iter()
            .map(|chromosome| chromosome.get_positions().to_vec())
            .collect::<Vec<_>>();

        assert!(improvements > 0);
        assert!(final_total_conflicts < initial_total_conflicts);
        assert!(positions.contains(&solution));
    }

    #[test]
    fn test_mate_random_chromosomes_uses_offspring_count() {
        let population = (0..8)
            .map(|seed| Chromosome::new(shuffled_values(8, seed)))
            .collect::<Vec<_>>();

        let mut genetic_algorithm = build_test_algorithm(population);
        genetic_algorithm.calc_fitness();
        genetic_algorithm.mate_random_chromosomes(3);

        assert_eq!(genetic_algorithm.get_population_size(), 11);
    }

    #[test]
    fn test_tournament_selection_picks_best_when_tournament_covers_population() {
        let solution = vec![0, 4, 7, 5, 2, 6, 1, 3];
        let high_conflict = vec![0, 1, 2, 3, 4, 5, 6, 7];
        let higher_conflict = vec![7, 6, 5, 4, 3, 2, 1, 0];
        let population = vec![
            Chromosome::new(high_conflict),
            Chromosome::new(solution.clone()),
            Chromosome::new(higher_conflict),
        ];

        let mut genetic_algorithm = GeneticAlgorithm::new(
            population,
            StdRng::seed_from_u64(7),
            GeneticAlgorithmParams {
                target_population_size: 3,
                max_epoch_count: 10,
                mutation_rate: DEFAULT_MUTATION_RATE,
                elite_ratio: DEFAULT_ELITE_RATIO,
                offspring_ratio: DEFAULT_OFFSPRING_RATIO,
                min_diversity_ratio: DEFAULT_MIN_DIVERSITY_RATIO,
                selection_strategy: SelectionStrategy::Tournament,
                tournament_size: 3,
                local_search_rate: DEFAULT_LOCAL_SEARCH_RATE,
                local_search_attempts: DEFAULT_LOCAL_SEARCH_ATTEMPTS,
            },
        );

        let selected_index = genetic_algorithm
            .select_tournament_parent_index()
            .expect("non-empty population should select a parent");

        assert_eq!(
            genetic_algorithm.population[selected_index].get_positions(),
            solution.as_slice()
        );
    }

    #[test]
    fn test_mutation_preserves_configured_elite_set() {
        let solution = vec![0, 4, 7, 5, 2, 6, 1, 3];
        let low_conflict = vec![0, 2, 4, 6, 1, 3, 5, 7];
        let high_conflict = vec![0, 1, 2, 3, 4, 5, 6, 7];
        let higher_conflict = vec![7, 6, 5, 4, 3, 2, 1, 0];
        let population = vec![
            Chromosome::new(high_conflict),
            Chromosome::new(solution.clone()),
            Chromosome::new(higher_conflict),
            Chromosome::new(low_conflict.clone()),
        ];

        let mut genetic_algorithm = build_test_algorithm(population);
        genetic_algorithm.mutate_population(1.0, 0.50);

        let positions = genetic_algorithm
            .population
            .iter()
            .map(|chromosome| chromosome.get_positions().to_vec())
            .collect::<Vec<_>>();

        assert!(positions.contains(&solution));
        assert!(positions.contains(&low_conflict));
    }

    #[test]
    fn test_diversity_refresh_preserves_elite_and_replaces_non_elites() {
        let solution = vec![0, 4, 7, 5, 2, 6, 1, 3];
        let duplicate = vec![0, 1, 2, 3, 4, 5, 6, 7];

        let mut population = vec![Chromosome::new(solution.clone())];
        population.extend((0..7).map(|_| Chromosome::new(duplicate.clone())));

        let mut genetic_algorithm = GeneticAlgorithm::new(
            population,
            StdRng::seed_from_u64(7),
            GeneticAlgorithmParams {
                target_population_size: 8,
                max_epoch_count: 10,
                mutation_rate: DEFAULT_MUTATION_RATE,
                elite_ratio: 0.25,
                offspring_ratio: DEFAULT_OFFSPRING_RATIO,
                min_diversity_ratio: 0.50,
                selection_strategy: DEFAULT_SELECTION_STRATEGY,
                tournament_size: DEFAULT_TOURNAMENT_SIZE,
                local_search_rate: DEFAULT_LOCAL_SEARCH_RATE,
                local_search_attempts: DEFAULT_LOCAL_SEARCH_ATTEMPTS,
            },
        );

        let replaced_count = genetic_algorithm.refresh_low_diversity_population(0.25);
        let positions = genetic_algorithm
            .population
            .iter()
            .map(|chromosome| chromosome.get_positions().to_vec())
            .collect::<Vec<_>>();

        assert_eq!(replaced_count, 2);
        assert!(positions.contains(&solution));
        assert!(super::count_unique_chromosomes(&genetic_algorithm.population) >= 2);
    }

    #[test]
    fn test_soft_restart_keeps_best_chromosome_and_refills_population() {
        let solution = vec![0, 4, 7, 5, 2, 6, 1, 3];
        let non_solution = vec![0, 1, 2, 3, 4, 5, 6, 7];

        let mut population = vec![Chromosome::new(solution)];
        population.extend((0..9).map(|_| Chromosome::new(non_solution.clone())));

        let mut genetic_algorithm = GeneticAlgorithm::new(
            population,
            StdRng::seed_from_u64(7),
            GeneticAlgorithmParams {
                target_population_size: 10,
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
        );

        let replaced_count = genetic_algorithm.soft_restart_population(DEFAULT_ELITE_RATIO);

        assert_eq!(replaced_count, 9);
        assert_eq!(genetic_algorithm.get_population_size(), 10);
        assert_eq!(
            genetic_algorithm.get_best_chromosome().get_conflicts_sum(),
            0
        );
    }

    #[test]
    fn test_soft_restart_replaces_at_least_one_chromosome() {
        let population = (0..4)
            .map(|_| Chromosome::new(vec![0, 1, 2, 3]))
            .collect::<Vec<_>>();

        let mut genetic_algorithm = GeneticAlgorithm::new(
            population,
            StdRng::seed_from_u64(7),
            GeneticAlgorithmParams {
                target_population_size: 4,
                max_epoch_count: 10,
                mutation_rate: DEFAULT_MUTATION_RATE,
                elite_ratio: 1.0,
                offspring_ratio: DEFAULT_OFFSPRING_RATIO,
                min_diversity_ratio: DEFAULT_MIN_DIVERSITY_RATIO,
                selection_strategy: DEFAULT_SELECTION_STRATEGY,
                tournament_size: DEFAULT_TOURNAMENT_SIZE,
                local_search_rate: DEFAULT_LOCAL_SEARCH_RATE,
                local_search_attempts: DEFAULT_LOCAL_SEARCH_ATTEMPTS,
            },
        );

        let replaced_count = genetic_algorithm.soft_restart_population(1.0);

        assert!(replaced_count >= 1);
        assert_eq!(genetic_algorithm.get_population_size(), 4);
    }

    #[test]
    fn test_pmx_returns_valid_permutation() {
        let mut rng = StdRng::seed_from_u64(2026);
        let expected_values = (0u16..16).collect::<Vec<_>>();

        for _ in 0..500 {
            let mut parent_one = expected_values.clone();
            let mut parent_two = expected_values.clone();
            parent_one.shuffle(&mut rng);
            parent_two.shuffle(&mut rng);

            let child = pmx(&parent_one, &parent_two, &mut rng);
            let mut child_sorted = child.clone();
            child_sorted.sort_unstable();

            assert_eq!(child.len(), 16);
            assert_eq!(child_sorted, expected_values);
        }
    }

    #[test]
    fn test_pmx_crossover_range_can_include_last_gene() {
        let parent_one = vec![0, 1, 2, 3, 4, 5, 6, 7];
        let parent_two = vec![7, 6, 5, 4, 3, 2, 1, 0];

        let child = super::pmx_with_crossover_points(&parent_one, &parent_two, 3, parent_one.len());
        let mut child_sorted = child.clone();
        child_sorted.sort_unstable();

        assert_eq!(
            child[parent_one.len() - 1],
            parent_one[parent_one.len() - 1]
        );
        assert_eq!(child_sorted, parent_one);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn prop_pmx_preserves_permutation_invariant(
            size in 2usize..64,
            parent_one_seed in any::<u64>(),
            parent_two_seed in any::<u64>(),
            crossover_seed in any::<u64>(),
        ) {
            let parent_one = shuffled_values(size, parent_one_seed);
            let parent_two = shuffled_values(size, parent_two_seed);
            let mut crossover_rng = StdRng::seed_from_u64(crossover_seed);

            let child = pmx(&parent_one, &parent_two, &mut crossover_rng);

            prop_assert_eq!(child.len(), size);

            let mut child_sorted = child.clone();
            child_sorted.sort_unstable();
            let expected_values =
                (0..u16::try_from(size).expect("size should fit into u16")).collect::<Vec<_>>();
            prop_assert_eq!(child_sorted, expected_values);
        }
    }
}
