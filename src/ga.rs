use rand::{Rng, SeedableRng, rngs::StdRng};

use self::chromosome::Chromosome;

pub mod chromosome;

const MIN_TO_MATE: usize = 10;
const MAX_TO_MATE: usize = 50;
const TARGET_EPOCH_PROGRESS_LOGS: u32 = 20;
pub const DEFAULT_MUTATION_RATE: f32 = 0.08;
pub const DEFAULT_ELITE_RATIO: f32 = 0.10;

pub struct GeneticAlgorithm {
    population: Vec<Chromosome>,
    target_population_size: usize,
    max_epoch_count: u32,
    rng: StdRng,
    mutation_rate: f32,
    elite_ratio: f32,
}

impl GeneticAlgorithm {
    fn new(
        population: Vec<Chromosome>,
        target_population_size: usize,
        max_epoch_count: u32,
        rng: StdRng,
        mutation_rate: f32,
        elite_ratio: f32,
    ) -> Self {
        Self {
            population,
            target_population_size,
            max_epoch_count,
            rng,
            mutation_rate,
            elite_ratio,
        }
    }

    pub fn get_population_size(&self) -> usize {
        self.population.len()
    }

    pub fn run_algorithm(&mut self) {
        if self.population.is_empty() {
            log::warn!("cannot run algorithm with empty population");
            return;
        }

        self.calc_fitness();
        let mut best_conflicts_sum = self.get_best_chromosome().get_conflicts_sum();
        if best_conflicts_sum == 0 {
            log::info!("ga solved in initial population");
            return;
        }

        let progress_log_interval = epoch_progress_log_interval(self.max_epoch_count);
        log::info!(
            "running ga epochs={} population_size={} progress_log_interval={} initial_best_conflicts_sum={best_conflicts_sum}",
            self.max_epoch_count,
            self.get_population_size(),
            progress_log_interval,
        );

        for epoch in 0..self.max_epoch_count {
            self.mate_random_chromosomes(MIN_TO_MATE, MAX_TO_MATE);
            self.mutate_population(self.mutation_rate);
            self.select_survivors();
            self.calc_fitness();

            let epoch_best_conflicts_sum = self.get_best_chromosome().get_conflicts_sum();
            let population_size = self.get_population_size();
            let epoch_number = epoch + 1;

            if epoch_best_conflicts_sum == 0 {
                log::info!("ga solved epoch={epoch_number} population_size={population_size}");
                return;
            }

            let is_improvement = epoch_best_conflicts_sum < best_conflicts_sum;
            if is_improvement {
                best_conflicts_sum = epoch_best_conflicts_sum;
                log::info!(
                    "ga improvement epoch={epoch_number} best_conflicts_sum={best_conflicts_sum} population_size={population_size}",
                );
                continue;
            }

            let is_periodic_log = epoch_number % progress_log_interval == 0;
            let is_last_epoch = epoch_number == self.max_epoch_count;
            if is_periodic_log || is_last_epoch {
                log::info!(
                    "ga progress epoch={epoch_number} best_conflicts_sum={best_conflicts_sum} population_size={population_size}",
                );
            }
        }

        log::warn!(
            "no solution found best_conflicts_sum={best_conflicts_sum} epochs={} population_size={}",
            self.max_epoch_count,
            self.get_population_size(),
        )
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
            for chromosome in &mut self.population {
                chromosome.set_fitness(1.0);
            }
            return;
        }

        for chromosome in &mut self.population {
            let conflicts_sum = chromosome.get_conflicts_sum() as f32;
            let fitness = (most_conflicts - conflicts_sum).powi(3) / diff_conflicts.powi(3);
            chromosome.set_fitness(fitness);
            log::trace!(
                "calculating fitness for chromosome [conflicts={conflicts_sum}, fitness={fitness}]",
            );
        }
    }

    fn mate_random_chromosomes(&mut self, min_to_mate: usize, max_to_mate: usize) {
        if self.population.is_empty() {
            return;
        }

        let mate_amount = if max_to_mate > min_to_mate {
            self.rng.random_range(min_to_mate..max_to_mate)
        } else {
            min_to_mate
        };

        let fitness_sum = self
            .population
            .iter()
            .map(|chromosome| chromosome.get_fitness())
            .sum::<f32>();

        if fitness_sum <= f32::EPSILON {
            log::debug!("fitness sum is near zero; selecting parents uniformly at random");
        }

        log::debug!(
            "select random chromosomes [mate_amount={mate_amount}, fitness_sum={fitness_sum}]",
        );

        for _ in 0..mate_amount {
            let Some(parent_one_index) = self.select_parent_index(fitness_sum) else {
                break;
            };
            let Some(parent_two_index) = self.select_parent_index(fitness_sum) else {
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

    fn select_parent_index(&mut self, fitness_sum: f32) -> Option<usize> {
        if self.population.is_empty() {
            return None;
        }

        if fitness_sum <= f32::EPSILON {
            return Some(self.rng.random_range(0..self.population.len()));
        }

        if let Some(index) = self.select_random_chromosome_index(fitness_sum) {
            return Some(index);
        }

        Some(self.rng.random_range(0..self.population.len()))
    }

    fn select_random_chromosome_index(&mut self, fitness_sum: f32) -> Option<usize> {
        if self.population.is_empty() || fitness_sum <= f32::EPSILON {
            return None;
        }

        let roulette_spin = self.rng.random_range(0.0..fitness_sum);
        let mut selection_rank = 0.0;

        for (index, chromosome) in self.population.iter().enumerate() {
            selection_rank += chromosome.get_fitness();
            if selection_rank >= roulette_spin {
                log::trace!(
                    "selecting chromosome index={index} selection_rank={selection_rank} roulette_spin={roulette_spin}",
                );
                return Some(index);
            }
        }

        self.population.len().checked_sub(1)
    }

    fn mutate_population(&mut self, mutation_rate: f32) {
        if self.population.len() < 2 {
            return;
        }

        let best_index = self
            .population
            .iter()
            .enumerate()
            .min_by_key(|(_, chromosome)| chromosome.get_conflicts_sum())
            .map(|(index, _)| index);

        let rng = &mut self.rng;
        for (index, chromosome) in self.population.iter_mut().enumerate() {
            if Some(index) == best_index {
                continue;
            }

            if rng.random::<f32>() < mutation_rate {
                chromosome.mutate_swap(rng);
            }
        }
    }

    fn select_survivors(&mut self) {
        if self.population.len() <= self.target_population_size {
            return;
        }

        self.population
            .sort_by_key(|chromosome| chromosome.get_conflicts_sum());

        let elite_count =
            ((self.target_population_size as f32) * self.elite_ratio).round() as usize;
        let elite_count = elite_count
            .min(self.target_population_size)
            .min(self.population.len());

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
}

pub fn build_genetic_algorithm(
    size: u16,
    initial_population: usize,
    max_epoch_count: u32,
    seed: u64,
    mutation_rate: f32,
    elite_ratio: f32,
) -> GeneticAlgorithm {
    let target_population_size = initial_population.max(1);
    let mutation_rate = normalize_unit_interval(mutation_rate, DEFAULT_MUTATION_RATE);
    let elite_ratio = normalize_unit_interval(elite_ratio, DEFAULT_ELITE_RATIO);
    let mut rng = StdRng::seed_from_u64(seed);
    let mut population: Vec<Chromosome> = Vec::with_capacity(target_population_size);

    for _ in 0..target_population_size {
        let positions = chromosome::generate_distinct_random_values_with_rng(size, &mut rng);
        let chromosome = Chromosome::new(positions);
        population.push(chromosome);
    }

    GeneticAlgorithm::new(
        population,
        target_population_size,
        max_epoch_count,
        rng,
        mutation_rate,
        elite_ratio,
    )
}

fn normalize_unit_interval(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

fn epoch_progress_log_interval(max_epoch_count: u32) -> u32 {
    (max_epoch_count / TARGET_EPOCH_PROGRESS_LOGS).max(1)
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
    let point_two = rng.random_range(chromosome_half_size..chromosome_size);

    log::trace!("partially mapped crossover [point_one={point_one}, point_two={point_two}]");

    let mut parent_two_positions = vec![usize::MAX; chromosome_size];
    for (index, &gene) in parent_two.iter().enumerate() {
        parent_two_positions[usize::from(gene)] = index;
    }

    let mut child_genes = vec![None; parent_one.len()];
    let mut child_used = vec![false; chromosome_size];

    for i in point_one..point_two {
        let gene = parent_one[i];
        child_genes[i] = Some(gene);
        child_used[usize::from(gene)] = true;
    }

    log::trace!("child positions one: {child_genes:?}");

    for (i, &gene) in parent_two
        .iter()
        .enumerate()
        .take(point_two)
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
    use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};

    use super::{
        DEFAULT_ELITE_RATIO, DEFAULT_MUTATION_RATE, GeneticAlgorithm, build_genetic_algorithm,
        chromosome::Chromosome, pmx,
    };

    fn build_test_algorithm(population: Vec<Chromosome>) -> GeneticAlgorithm {
        let target_population_size = population.len().max(1);
        GeneticAlgorithm::new(
            population,
            target_population_size,
            10,
            StdRng::seed_from_u64(7),
            DEFAULT_MUTATION_RATE,
            DEFAULT_ELITE_RATIO,
        )
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
        let mut zero_board =
            build_genetic_algorithm(0, 8, 10, 42, DEFAULT_MUTATION_RATE, DEFAULT_ELITE_RATIO);
        zero_board.run_algorithm();
        assert_eq!(zero_board.get_best_chromosome().get_conflicts_sum(), 0);

        let mut one_board =
            build_genetic_algorithm(1, 8, 10, 42, DEFAULT_MUTATION_RATE, DEFAULT_ELITE_RATIO);
        one_board.run_algorithm();
        assert_eq!(one_board.get_best_chromosome().get_conflicts_sum(), 0);
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
}
