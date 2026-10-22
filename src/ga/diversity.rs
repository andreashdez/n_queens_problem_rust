//! Diversity upkeep: refreshing a converged population and soft restarts after stagnation.

use super::{
    GeneticAlgorithm,
    chromosome::{self, Chromosome},
    selection::{elite_count_for_population, normalize_unit_interval},
};

const SOFT_RESTART_ELITE_RATIO_SCALE: f32 = 0.4;

impl GeneticAlgorithm {
    pub(super) fn refresh_low_diversity_population(
        &mut self,
        elite_ratio: f32,
        unique_chromosomes: usize,
    ) -> usize {
        if self.population.is_empty() || self.min_diversity_ratio <= 0.0 {
            return 0;
        }

        let min_unique_chromosomes =
            minimum_unique_chromosomes(self.target_population_size, self.min_diversity_ratio);
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

        self.population.sort_by_key(Chromosome::get_conflicts_sum);

        let board_size = self
            .population
            .first()
            .map_or(0, |chromosome| chromosome.get_positions().len());
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

    pub(super) fn soft_restart_population(&mut self, elite_ratio: f32) -> usize {
        if self.population.is_empty() {
            return 0;
        }

        let elite_ratio = normalize_unit_interval(elite_ratio, self.elite_ratio);

        self.population.sort_by_key(Chromosome::get_conflicts_sum);

        let board_size = self
            .population
            .first()
            .map_or(0, |chromosome| chromosome.get_positions().len());
        let board_size = u16::try_from(board_size).expect("board size should fit into u16");

        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "Rounded restart survivor count is clamped to the available population below."
        )]
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

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "The validated positive ratio is rounded up and bounded by the population size."
)]
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

#[cfg(test)]
mod tests {
    use rand::{SeedableRng, rngs::StdRng};

    use super::*;
    use crate::ga::{
        DEFAULT_ELITE_RATIO, DEFAULT_LOCAL_SEARCH_ATTEMPTS, DEFAULT_LOCAL_SEARCH_RATE,
        DEFAULT_MIN_DIVERSITY_RATIO, DEFAULT_MUTATION_RATE, DEFAULT_OFFSPRING_RATIO,
        DEFAULT_SELECTION_STRATEGY, DEFAULT_TOURNAMENT_SIZE, GeneticAlgorithmParams,
        metrics::population_metrics,
    };

    #[test]
    fn test_minimum_unique_chromosomes_scales_with_population() {
        assert_eq!(super::minimum_unique_chromosomes(0, 0.10), 0);
        assert_eq!(super::minimum_unique_chromosomes(1, 0.10), 1);
        assert_eq!(super::minimum_unique_chromosomes(8, 0.10), 2);
        assert_eq!(super::minimum_unique_chromosomes(40_000, 0.10), 4_000);
        assert_eq!(super::minimum_unique_chromosomes(10, 0.0), 0);
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

        let unique_chromosomes =
            population_metrics(&genetic_algorithm.population).unique_chromosomes;
        let replaced_count =
            genetic_algorithm.refresh_low_diversity_population(0.25, unique_chromosomes);
        let positions = genetic_algorithm
            .population
            .iter()
            .map(|chromosome| chromosome.get_positions().to_vec())
            .collect::<Vec<_>>();

        assert_eq!(replaced_count, 2);
        assert!(positions.contains(&solution));
        assert!(population_metrics(&genetic_algorithm.population).unique_chromosomes >= 2);
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
}
