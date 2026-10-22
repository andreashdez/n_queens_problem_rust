//! Optional hill climbing: improving swaps on a random share of the non-elites.

use rand::{Rng, RngExt, seq::SliceRandom};

use super::{
    GeneticAlgorithm,
    chromosome::Chromosome,
    selection::{elite_count_for_population, normalize_unit_interval, select_elites_to_front},
};

impl GeneticAlgorithm {
    pub(super) fn improve_population_with_local_search(&mut self, elite_ratio: f32) -> usize {
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
}

fn local_search_candidate_count(non_elite_count: usize, local_search_rate: f32) -> usize {
    if non_elite_count == 0 || local_search_rate <= 0.0 || !local_search_rate.is_finite() {
        return 0;
    }

    #[allow(clippy::cast_precision_loss)]
    let non_elite_count = non_elite_count as f32;
    let count = (non_elite_count * local_search_rate)
        .round()
        .clamp(1.0, non_elite_count);

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = count as usize;
    count
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

        // Score the swap before committing to it. Applying and reverting instead
        // costs two extra conflict recounts on every rejected attempt, and most
        // attempts are rejected.
        let candidate_conflicts_sum = chromosome.conflicts_sum_after_swap(index_one, index_two);
        if candidate_conflicts_sum < current_conflicts_sum {
            chromosome.apply_swap_with_conflicts_sum(index_one, index_two, candidate_conflicts_sum);
            improved = true;
        }
    }

    improved
}

#[cfg(test)]
mod tests {
    use rand::{SeedableRng, rngs::StdRng};

    use super::*;
    use crate::ga::{
        DEFAULT_MIN_DIVERSITY_RATIO, DEFAULT_MUTATION_RATE, DEFAULT_OFFSPRING_RATIO,
        DEFAULT_SELECTION_STRATEGY, DEFAULT_TOURNAMENT_SIZE, GeneticAlgorithmParams,
    };

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
}
