//! Swap mutation for the non-elite population.

use rand::RngExt;
use rayon::prelude::*;

use super::{
    GeneticAlgorithm,
    selection::{elite_count_for_population, normalize_unit_interval, select_elites_to_front},
};

impl GeneticAlgorithm {
    pub(super) fn mutate_population(&mut self, mutation_rate: f32, elite_ratio: f32) {
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
}

#[cfg(test)]
mod tests {
    use crate::ga::{chromosome::Chromosome, tests::build_test_algorithm};

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
}
