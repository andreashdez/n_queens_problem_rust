use rand::Rng;

use self::chromosome::Chromosome;

pub mod chromosome;

const MIN_TO_MATE: u16 = 10;
const MAX_TO_MATE: u16 = 50;
const MAX_EPOCH_COUNT: u16 = 5000;

pub struct GeneticAlgorithm {
    population: Vec<Chromosome>,
}

impl GeneticAlgorithm {
    fn new(population: Vec<Chromosome>) -> Self {
        Self { population }
    }

    pub fn get_population_size(&self) -> u16 {
        self.population.len() as u16
    }

    pub fn run_algorithm(&mut self) {
        self.calc_fitness();
        for epoch in 0..MAX_EPOCH_COUNT {
            self.mate_random_chromosomes(MIN_TO_MATE, MAX_TO_MATE);
            self.calc_fitness();
            log::info!("epoch: {}", epoch);
            log::info!(
                "best chromosome conflicts sum: {}",
                self.get_best_chromosome().get_conflicts_sum()
            );
            if self.get_best_chromosome().get_conflicts_sum() == 0 {
                return;
            }
        }
        log::warn!("no solution found")
    }

    pub fn get_best_chromosome(&self) -> &Chromosome {
        self.population
            .iter()
            .min_by_key(|chromosome| chromosome.get_conflicts_sum())
            .unwrap()
    }

    pub fn get_worst_chromosome(&self) -> &Chromosome {
        self.population
            .iter()
            .max_by_key(|chromosome| chromosome.get_conflicts_sum())
            .unwrap()
    }

    fn calc_fitness(&mut self) {
        let most_conflicts = self.get_worst_chromosome().get_conflicts_sum() as f32;
        let least_conflicts = self.get_best_chromosome().get_conflicts_sum() as f32;
        let diff_conflicts = most_conflicts - least_conflicts;
        log::debug!(
            "calculating fitness [worst_score={}, best_score={}, diff={}]",
            most_conflicts,
            least_conflicts,
            diff_conflicts
        );
        for chromosome in &mut self.population {
            let conflicts_sum = chromosome.get_conflicts_sum() as f32;
            let fitness = (most_conflicts - conflicts_sum).powi(3) / diff_conflicts.powi(3);
            chromosome.set_fitness(fitness);
            log::trace!(
                "calculating fitness for chromosome [conflicts={}, fitness={}]",
                conflicts_sum,
                fitness
            );
        }
    }

    fn mate_random_chromosomes(&mut self, min_to_mate: u16, max_to_mate: u16) {
        let mate_amount = rand::rng().random_range(min_to_mate..max_to_mate);
        let fitness_sum = self
            .population
            .iter()
            .map(|chromosome| chromosome.get_fitness())
            .sum::<f32>();
        log::debug!(
            "select random chromosomes [mate_amount={}, fitness_sum={}]",
            mate_amount,
            fitness_sum
        );
        for _ in 0..mate_amount {
            let parent_one = self
                .select_random_chromosome(fitness_sum)
                .unwrap_or_else(|| self.get_best_chromosome());
            let parent_two = self
                .select_random_chromosome(fitness_sum)
                .unwrap_or_else(|| self.get_worst_chromosome());
            let child = mate_chromosomes(parent_one, parent_two);
            self.population.push(child);
        }
    }

    fn select_random_chromosome(&self, fitness_sum: f32) -> Option<&Chromosome> {
        let roulette_spin = rand::rng().random_range(0.0..fitness_sum);
        let mut selection_rank = 0.0;
        for chromosome in &self.population {
            selection_rank += chromosome.get_fitness();
            if selection_rank > roulette_spin {
                log::trace!("selecting chromosome: {:?}", chromosome);
                return Some(&chromosome);
            }
        }
        None
    }
}

pub fn build_genetic_algorithm(size: u16, initial_population: u16) -> GeneticAlgorithm {
    let mut population: Vec<Chromosome> = Vec::new();
    for _ in 0..initial_population {
        let positions = chromosome::generate_distinct_random_values(size);
        let chromosome = Chromosome::new(positions);
        population.push(chromosome);
    }
    GeneticAlgorithm::new(population)
}

fn mate_chromosomes(parent_one: &Chromosome, parent_two: &Chromosome) -> Chromosome {
    log::debug!("mate chromosomes");
    log::trace!("parent_one={:?}", *parent_one);
    log::trace!("parent_two={:?}", *parent_two);
    let child_genes = pmx(parent_one.get_positions(), parent_two.get_positions());
    let child = Chromosome::new(child_genes);
    log::trace!("child={:?}", child);
    child
}

fn pmx(parent_one: Vec<u16>, parent_two: Vec<u16>) -> Vec<u16> {
    let chromosome_size = parent_one.len();
    let chromosome_half_size = chromosome_size / 2;
    let point_one = rand::rng().random_range(0..chromosome_half_size);
    let point_two = rand::rng().random_range(chromosome_half_size..chromosome_size);
    log::debug!(
        "partially mapped crossover [point_one={}, point_two={}]",
        point_one,
        point_two
    );
    let mut child_genes = vec![None; parent_one.len()];
    for i in point_one..point_two {
        child_genes[i] = Some(parent_one[i]);
    }
    log::debug!("child positions one: {:?}", child_genes);
    for i in point_one..point_two {
        if !child_genes.contains(&Some(parent_two[i])) {
            let position = find_position(i, &parent_one, &parent_two, &child_genes);
            child_genes[position] = Some(parent_two[i]);
        }
    }
    log::debug!("child positions two: {:?}", child_genes);
    for i in 0..chromosome_size {
        match child_genes[i] {
            None => child_genes[i] = Some(parent_two[i]),
            Some(_) => {}
        }
    }
    log::debug!("child positions three: {:?}", child_genes);
    child_genes.iter().map(|gene| gene.unwrap()).collect()
}

fn find_position(
    index: usize,
    parent_one: &Vec<u16>,
    parent_two: &Vec<u16>,
    child: &Vec<Option<u16>>,
) -> usize {
    let position = parent_two
        .iter()
        .position(|&p| p == parent_one[index])
        .unwrap();
    log::trace!("checking position {}", position);
    match child[position] {
        None => position,
        Some(_) => find_position(position, parent_one, parent_two, child),
    }
}

#[cfg(test)]
mod tests {
    use super::{GeneticAlgorithm, chromosome::Chromosome};

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
        let mut genetic_algorithm = GeneticAlgorithm::new(population);
        genetic_algorithm.calc_fitness();
        assert_eq!(genetic_algorithm.get_worst_chromosome().get_fitness(), 0.0);
        assert_eq!(genetic_algorithm.get_best_chromosome().get_fitness(), 1.0);
    }
}
