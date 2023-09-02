use rand::Rng;

use self::chromosome::Chromosome;

pub mod chromosome;

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
        let worst_score = self.get_worst_chromosome().get_conflicts_sum() as f32;
        let mut best_score = self.get_best_chromosome().get_conflicts_sum() as f32;
        if best_score == 0.0 {
            best_score = 0.1;
            log::debug!("Population contains solution")
        }
        log::debug!(
            "calculating fitness [worst_score={}, best_score={}]",
            worst_score,
            best_score
        );
        for chromosome in &mut self.population {
            let conflicts_sum = chromosome.get_conflicts_sum() as f32;
            let fitness = (worst_score - conflicts_sum) * 100.0 / best_score;
            chromosome.set_fitness(fitness);
            log::trace!(
                "calculating fitness for chromosome [conflicts={}, fitness={}]",
                conflicts_sum,
                fitness
            );
        }
    }

    pub fn calc_rank(&mut self) {
        let fitness_sum = self
            .population
            .iter()
            .map(|chromosome| chromosome.get_fitness())
            .sum::<f32>();
        let rank_total = fitness_sum / self.population.len() as f32;
        log::debug!("calculating rank [rank_total={}]", rank_total);
        for chromosome in &mut self.population {
            let fitness = chromosome.get_fitness();
            let rank = fitness / rank_total;
            chromosome.set_rank(rank);
            log::trace!(
                "calculating rank for chromosome [fitness={}, rank={}]",
                fitness,
                rank
            );
        }
    }

    pub fn select_random_chromosomes(&self, max_to_select: usize) -> Vec<usize> {
        let mut selected_chromosomes = Vec::new();
        for _ in 0..max_to_select {
            let roulette_spin = rand::thread_rng().gen_range(1..self.population.len()) as f32;
            let mut selection_rank = 0.0;
            for (index, chromosome) in self.population.iter().enumerate() {
                selection_rank += chromosome.get_rank();
                if selection_rank > roulette_spin && !selected_chromosomes.contains(&index) {
                    selected_chromosomes.push(index);
                    log::trace!("selecting chromosome: {}", index);
                    break;
                }
            }
        }
        selected_chromosomes
    }

    pub fn run_epoch(&mut self) -> &Chromosome {
        self.calc_fitness();
        self.calc_rank();
        self.select_random_chromosomes(10);
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
        assert_eq!(genetic_algorithm.get_best_chromosome().get_fitness(), 500.0);
    }
}
