use rand::Rng;

use self::chromosome::Chromosome;

pub mod chromosome;

const MIN_TO_MATE: usize = 10;
const MAX_TO_MATE: usize = 50;

pub struct GeneticAlgorithm {
    population: Vec<Chromosome>,
}

impl GeneticAlgorithm {
    pub fn new(population: Vec<Chromosome>) -> Self {
        Self { population }
    }

    pub fn get_population(&self) -> &Vec<Chromosome> {
        &self.population
    }

    fn get_best_chromosome(&self) -> &Chromosome {
        self.population
            .iter()
            .min_by_key(|chromosome| chromosome.get_conflicts_sum())
            .unwrap()
    }

    fn get_worst_chromosome(&self) -> &Chromosome {
        self.population
            .iter()
            .max_by_key(|chromosome| chromosome.get_conflicts_sum())
            .unwrap()
    }

    // fn sort_population(&mut self) {
    //     self.population
    //         .sort_by(|a, b| b.get_conflicts_sum().cmp(&a.get_conflicts_sum()));
    // }

    pub fn calc_fitness(&mut self) {
        // self.sort_population();
        // let worst_score = self.population.first().unwrap().get_conflicts_sum() as f32;
        // let mut best_score = self.population.last().unwrap().get_conflicts_sum() as f32;
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

    pub fn mate_random_chromosomes(&self, min_to_mate: usize, max_to_mate: usize) {
        let mate_amount = rand::thread_rng().gen_range(min_to_mate..max_to_mate);
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
            let parent_one = self.select_random_chromosome(fitness_sum).unwrap_or_else(|| self.get_best_chromosome());
            let parent_two = self.select_random_chromosome(fitness_sum).unwrap_or_else(|| self.get_worst_chromosome());
            self.mate_chromosomes(parent_one, parent_two);
        }
    }

    pub fn select_random_chromosome(&self, fitness_sum: f32) -> Option<&Chromosome> {
        let roulette_spin = rand::thread_rng().gen_range(0.0..fitness_sum);
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

    pub fn mate_chromosomes(&self, parent_one: &Chromosome, parent_two: &Chromosome) {
        log::debug!("mate chromosomes");
        log::trace!("parent_one={:?}", *parent_one);
        log::trace!("parent_two={:?}", *parent_two);
        //TODO
    }

    pub fn run_epoch(&mut self) -> &Chromosome {
        self.calc_fitness();
        self.mate_random_chromosomes(MIN_TO_MATE, MAX_TO_MATE);
        self.get_best_chromosome()
    }
}

pub fn build_genetic_algorithm(size: usize, initial_population: usize) -> GeneticAlgorithm {
    let mut population: Vec<Chromosome> = Vec::new();
    for _ in 0..initial_population {
        let positions = chromosome::generate_distinct_random_values(size);
        let chromosome = Chromosome::new(positions);
        population.push(chromosome);
    }
    GeneticAlgorithm::new(population)
}

#[cfg(test)]
mod tests {
    use super::{chromosome::Chromosome, GeneticAlgorithm};

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
