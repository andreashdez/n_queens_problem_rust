//! Mating: parent pairing, partially mapped crossover (PMX), and parallel offspring building.

use rand::{Rng, RngExt};
use rayon::prelude::*;

use super::{
    GeneticAlgorithm, SelectionStrategy, chromosome::Chromosome, selection::cumulative_fitness,
};

impl GeneticAlgorithm {
    pub(super) fn mate_random_chromosomes(&mut self, offspring_count: usize) {
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

        // Draw every child's inputs up front, in the order the serial loop used,
        // so the master RNG stream is unchanged and the build below is pure.
        let chromosome_size = self.population[0].get_positions().len();
        let mut plans = Vec::with_capacity(offspring_count);
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
            let crossover_points = draw_crossover_points(chromosome_size, &mut self.rng);

            plans.push(OffspringPlan {
                parent_one_index,
                parent_two_index,
                crossover_points,
            });
        }

        let offspring = build_offspring(&self.population, &plans);
        self.population.extend(offspring);
    }
}

/// One child's fully drawn inputs.
///
/// Separating the draw from the build lets the master RNG stay a plain serial
/// sequence while the PMX work, which needs no randomness, runs in parallel.
#[derive(Debug, Clone, Copy)]
struct OffspringPlan {
    parent_one_index: usize,
    parent_two_index: usize,
    crossover_points: Option<(usize, usize)>,
}

/// Smallest batch worth handing to Rayon.
///
/// Mating one child is only tens of microseconds of work, so small batches lose
/// to the split-and-join overhead. Measured crossover cost per epoch at N=18,
/// parallel against serial: 20 offspring 5.0x slower, 60 2.1x, 100 1.6x, 200
/// 1.12x, then 400 1.06x faster, 1,000 1.35x, and 4,000 2.1x. Break-even sits
/// near 350, so this keeps a margin above it.
const MIN_PARALLEL_OFFSPRING: usize = 512;

/// Builds every planned child. Pure: identical plans give identical offspring,
/// in the same order, whether or not this runs in parallel.
fn build_offspring(population: &[Chromosome], plans: &[OffspringPlan]) -> Vec<Chromosome> {
    let build = |scratch: &mut PmxScratch, plan: &OffspringPlan| {
        mate_chromosomes(
            population[plan.parent_one_index].get_positions(),
            population[plan.parent_two_index].get_positions(),
            plan.crossover_points,
            scratch,
        )
    };

    if plans.len() < MIN_PARALLEL_OFFSPRING {
        let mut scratch = PmxScratch::default();
        return plans.iter().map(|plan| build(&mut scratch, plan)).collect();
    }

    plans
        .par_iter()
        .map_init(PmxScratch::default, build)
        .collect()
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "A finite positive ratio is rounded to a nonnegative count; float-to-integer conversion saturates."
)]
pub(super) fn offspring_count_for_population(
    target_population_size: usize,
    offspring_ratio: f32,
) -> usize {
    if target_population_size == 0 || offspring_ratio <= 0.0 || !offspring_ratio.is_finite() {
        return 0;
    }

    ((target_population_size as f64) * f64::from(offspring_ratio))
        .round()
        .max(1.0) as usize
}

/// Reusable buffers for [`pmx_with_crossover_points`].
///
/// Mating a generation runs the same fixed-size bookkeeping thousands of times,
/// so the loop keeps one of these instead of allocating three vectors per child.
#[derive(Debug, Default)]
struct PmxScratch {
    parent_two_positions: Vec<usize>,
    child_used: Vec<bool>,
    child_filled: Vec<bool>,
}

impl PmxScratch {
    /// Clears the buffers and sizes them for one chromosome, keeping capacity.
    fn reset(&mut self, chromosome_size: usize) {
        self.parent_two_positions.clear();
        self.parent_two_positions
            .resize(chromosome_size, usize::MAX);
        self.child_used.clear();
        self.child_used.resize(chromosome_size, false);
        self.child_filled.clear();
        self.child_filled.resize(chromosome_size, false);
    }
}

/// Draws the PMX crossover window, or `None` when the chromosome is too short
/// to cross and the child is just a copy of the first parent.
///
/// Kept separate from the mating itself so the mating loop can consume the
/// master RNG serially and then build children off-thread.
fn draw_crossover_points(chromosome_size: usize, rng: &mut impl Rng) -> Option<(usize, usize)> {
    if chromosome_size <= 1 {
        return None;
    }

    let chromosome_half_size = chromosome_size / 2;
    let point_one = rng.random_range(0..chromosome_half_size);
    let point_two_exclusive = rng.random_range(chromosome_half_size..=chromosome_size);

    log::trace!(
        "partially mapped crossover [point_one={point_one}, point_two_exclusive={point_two_exclusive}]"
    );

    Some((point_one, point_two_exclusive))
}

fn mate_chromosomes(
    parent_one: &[u16],
    parent_two: &[u16],
    crossover_points: Option<(usize, usize)>,
    scratch: &mut PmxScratch,
) -> Chromosome {
    log::trace!("mate chromosomes");
    log::trace!("parent_one={parent_one:?}");
    log::trace!("parent_two={parent_two:?}");

    // PMX always emits a permutation of the parent rows, so skip revalidating it;
    // `prop_pmx_preserves_permutation_invariant` and the debug assertion in
    // `Chromosome::new_unchecked` guard that invariant.
    let child_genes = match crossover_points {
        Some((point_one, point_two_exclusive)) => pmx_with_crossover_points(
            parent_one,
            parent_two,
            point_one,
            point_two_exclusive,
            scratch,
        ),
        None => parent_one.to_vec(),
    };
    let child = Chromosome::new_unchecked(child_genes);

    log::trace!("child={child:?}");
    child
}

fn pmx_with_crossover_points(
    parent_one: &[u16],
    parent_two: &[u16],
    point_one: usize,
    point_two_exclusive: usize,
    scratch: &mut PmxScratch,
) -> Vec<u16> {
    debug_assert_eq!(parent_one.len(), parent_two.len());
    debug_assert!(point_one < point_two_exclusive);
    debug_assert!(point_two_exclusive <= parent_one.len());

    let chromosome_size = parent_one.len();
    scratch.reset(chromosome_size);
    let PmxScratch {
        parent_two_positions,
        child_used,
        child_filled,
    } = scratch;

    for (index, &gene) in parent_two.iter().enumerate() {
        parent_two_positions[usize::from(gene)] = index;
    }

    // `child_filled` tracks which slots hold a gene, so the child itself stays a
    // plain `Vec<u16>` and needs no second pass to unwrap it.
    let mut child_genes = vec![0u16; chromosome_size];

    for i in point_one..point_two_exclusive {
        let gene = parent_one[i];
        child_genes[i] = gene;
        child_filled[i] = true;
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
            let position = find_position(i, parent_one, parent_two_positions, child_filled);
            child_genes[position] = gene;
            child_filled[position] = true;
            child_used[usize::from(gene)] = true;
        }
    }

    log::trace!("child positions two: {child_genes:?}");

    for i in 0..chromosome_size {
        if !child_filled[i] {
            child_genes[i] = parent_two[i];
        }
    }

    log::trace!("child positions three: {child_genes:?}");
    child_genes
}

fn find_position(
    index: usize,
    parent_one: &[u16],
    parent_two_positions: &[usize],
    child_filled: &[bool],
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
        if !child_filled[position] {
            return position;
        }

        current_index = position;
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};

    use super::*;
    use crate::ga::{
        GaConfig, build_genetic_algorithm,
        tests::{build_test_algorithm, shuffled_values},
    };

    #[test]
    fn offspring_only_use_parents_from_the_start_of_mating() {
        for strategy in [SelectionStrategy::Roulette, SelectionStrategy::Tournament] {
            for seed in 0..16 {
                let config = GaConfig::new(8, 8, 1, seed).with_selection_strategy(strategy);
                let mut actual = build_genetic_algorithm(config).unwrap();
                let mut parents = build_genetic_algorithm(config).unwrap();
                actual.calc_fitness();
                parents.calc_fitness();
                let cumulative = super::cumulative_fitness(&parents.population);
                let total = *cumulative.last().unwrap();
                let expected: Vec<_> = (0..32)
                    .map(|_| {
                        let first = parents
                            .select_parent_index(Some((&cumulative, total)))
                            .unwrap();
                        let second = parents
                            .select_parent_index(Some((&cumulative, total)))
                            .unwrap();
                        let chromosome_size = parents.population[first].get_positions().len();
                        super::mate_chromosomes(
                            parents.population[first].get_positions(),
                            parents.population[second].get_positions(),
                            draw_crossover_points(chromosome_size, &mut parents.rng),
                            &mut super::PmxScratch::default(),
                        )
                        .get_positions()
                        .to_vec()
                    })
                    .collect();
                actual.mate_random_chromosomes(32);
                let children: Vec<_> = actual.population[8..]
                    .iter()
                    .map(|c| c.get_positions().to_vec())
                    .collect();
                assert_eq!(children, expected, "strategy={strategy} seed={seed}");
            }
        }
    }

    #[test]
    fn test_offspring_count_scales_with_population() {
        assert_eq!(super::offspring_count_for_population(40_000, 0.10), 4_000);
        assert_eq!(super::offspring_count_for_population(10, 0.2), 2);
        assert_eq!(super::offspring_count_for_population(10, 0.0), 0);
        assert_eq!(super::offspring_count_for_population(1, 0.001), 1);
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

    /// Draws a crossover window and mates one pair, mirroring what the mating
    /// loop does per child across its serial and parallel halves.
    fn pmx_child(parent_one: &[u16], parent_two: &[u16], rng: &mut StdRng) -> Vec<u16> {
        match draw_crossover_points(parent_one.len(), rng) {
            Some((point_one, point_two_exclusive)) => super::pmx_with_crossover_points(
                parent_one,
                parent_two,
                point_one,
                point_two_exclusive,
                &mut PmxScratch::default(),
            ),
            None => parent_one.to_vec(),
        }
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

            let child = pmx_child(&parent_one, &parent_two, &mut rng);
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

        let child = super::pmx_with_crossover_points(
            &parent_one,
            &parent_two,
            3,
            parent_one.len(),
            &mut super::PmxScratch::default(),
        );
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

            let child = pmx_child(&parent_one, &parent_two, &mut crossover_rng);

            prop_assert_eq!(child.len(), size);

            let mut child_sorted = child;
            child_sorted.sort_unstable();
            let expected_values =
                (0..u16::try_from(size).expect("size should fit into u16")).collect::<Vec<_>>();
            prop_assert_eq!(child_sorted, expected_values);
        }
    }
}
