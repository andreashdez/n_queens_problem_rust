use self::chromosome::Chromosome;

pub mod chromosome;

pub struct GeneticAlgorithm {
    population: Vec<Chromosome>,
}

impl GeneticAlgorithm {
    pub fn new(size: usize, starting_population: usize) -> Self {
        let mut population: Vec<Chromosome> = Vec::new();
        for _ in 0..starting_population {
            let chromosome = Chromosome::new(size);
            population.push(chromosome);
        }
        Self { population }
    }

    pub fn get_population(&self) -> &Vec<Chromosome> {
        &self.population
    }

    pub fn run_epoch(&mut self) -> &Chromosome {
        self.population
            .sort_by(|a, b| b.get_conflicts_sum().cmp(&a.get_conflicts_sum()));
        self.population.last().unwrap()
    }
}
