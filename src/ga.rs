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

    fn sort_population(&mut self) {
        self.population
            .sort_by(|a, b| b.get_conflicts_sum().cmp(&a.get_conflicts_sum()));
    }

    pub fn calc_fitness(&mut self) {
        self.sort_population();
        let worst_score = self.population.first().unwrap().get_conflicts_sum() as f32;
        let mut best_score = self.population.last().unwrap().get_conflicts_sum() as f32;
        if best_score == 0.0 {
            best_score = 0.1;
            log::error!("Stuck in homogeneous population")
        }
        for chromosome in &mut self.population {
            let conflicts_sum = chromosome.get_conflicts_sum() as f32;
            let fitness = (worst_score - conflicts_sum) * 100.0 / best_score;
            (*chromosome).set_fitness(fitness);
            log::debug!(
                "calculating fitness [worst_score={}, best_score={}, current_conflicts={}] -> {}",
                worst_score,
                best_score,
                (*chromosome).get_conflicts_sum(),
                fitness
            );
        }
    }

    pub fn run_epoch(&mut self) -> &Chromosome {
        self.calc_fitness();
        self.population.last().unwrap()
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
        let population = genetic_algorithm.get_population();

        assert_eq!(population[0].get_fitness(), 0.0);
        assert_eq!(population[1].get_fitness(), 100.0);
        assert_eq!(population[2].get_fitness(), 200.0);
        assert_eq!(population[3].get_fitness(), 300.0);
        assert_eq!(population[4].get_fitness(), 400.0);
        assert_eq!(population[5].get_fitness(), 500.0);
    }
}
