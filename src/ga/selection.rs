//! Parent selection (roulette and tournament), survivor selection, and elite bookkeeping.

use rand::RngExt;

use super::{GeneticAlgorithm, SelectionStrategy, chromosome::Chromosome};

impl GeneticAlgorithm {
    pub(super) fn select_parent_index(
        &mut self,
        roulette_selection: Option<(&[f32], f32)>,
    ) -> Option<usize> {
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

    pub(super) fn select_survivors(&mut self, elite_ratio: f32) {
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

        // Keeping a uniform random subset of the non-elites is the same as dropping
        // a uniform random subset of them, so cull the surplus in place instead of
        // moving every survivor into a fresh allocation. Elites hold indices
        // `0..elite_count` and the loop only draws above that, so `swap_remove`
        // always backfills from a non-elite tail slot and elitism is preserved.
        while self.population.len() > self.target_population_size {
            let random_index = self.rng.random_range(elite_count..self.population.len());
            self.population.swap_remove(random_index);
        }
    }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "The validated positive ratio is rounded and bounded by both population sizes."
)]
pub(super) fn elite_count_for_population(
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

pub(super) fn select_elites_to_front(population: &mut [Chromosome], elite_count: usize) {
    if elite_count == 0 || population.is_empty() {
        return;
    }

    let nth_elite_index = elite_count.saturating_sub(1).min(population.len() - 1);
    population.select_nth_unstable_by_key(nth_elite_index, Chromosome::get_conflicts_sum);
}

pub(super) fn cumulative_fitness(population: &[Chromosome]) -> Vec<f32> {
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

pub(super) const fn normalize_unit_interval(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
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
        tests::shuffled_values,
    };

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
    fn test_survivor_selection_trims_to_target_and_keeps_elites() {
        let population = (0..20)
            .map(|seed| Chromosome::new(shuffled_values(8, seed)))
            .collect::<Vec<_>>();
        let elite_count = 3;
        let mut expected_elite_conflicts = population
            .iter()
            .map(Chromosome::get_conflicts_sum)
            .collect::<Vec<_>>();
        expected_elite_conflicts.sort_unstable();
        expected_elite_conflicts.truncate(elite_count);

        let mut genetic_algorithm = GeneticAlgorithm::new(
            population,
            StdRng::seed_from_u64(7),
            GeneticAlgorithmParams {
                target_population_size: 10,
                max_epoch_count: 10,
                mutation_rate: DEFAULT_MUTATION_RATE,
                elite_ratio: 0.3,
                offspring_ratio: DEFAULT_OFFSPRING_RATIO,
                min_diversity_ratio: DEFAULT_MIN_DIVERSITY_RATIO,
                selection_strategy: DEFAULT_SELECTION_STRATEGY,
                tournament_size: DEFAULT_TOURNAMENT_SIZE,
                local_search_rate: DEFAULT_LOCAL_SEARCH_RATE,
                local_search_attempts: DEFAULT_LOCAL_SEARCH_ATTEMPTS,
            },
        );
        genetic_algorithm.select_survivors(0.3);

        assert_eq!(genetic_algorithm.get_population_size(), 10);

        // Comparing the best conflict sums rather than identities keeps the
        // assertion correct when several chromosomes tie for an elite slot.
        let mut survivor_conflicts = genetic_algorithm
            .population
            .iter()
            .map(Chromosome::get_conflicts_sum)
            .collect::<Vec<_>>();
        survivor_conflicts.sort_unstable();
        survivor_conflicts.truncate(elite_count);
        assert_eq!(survivor_conflicts, expected_elite_conflicts);

        // Already at target: trimming again must be a no-op.
        genetic_algorithm.select_survivors(0.3);
        assert_eq!(genetic_algorithm.get_population_size(), 10);
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
}
