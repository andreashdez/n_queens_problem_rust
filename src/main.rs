use crate::ga::chromosome::Chromosome;

pub mod ga;
pub mod ui;

fn main() {
    simple_logger::init().unwrap();

    // let mut genetic_algorithm = ga::build_genetic_algorithm(8, 4);
    // let best_chromosome = genetic_algorithm.run_epoch();
    // let conflicts_sum = best_chromosome.get_conflicts_sum();
    // let fitness = best_chromosome.get_fitness();
    // println!("--------------------------------");
    // println!("Best Conflicts: {conflicts_sum}");
    // println!("Fitness:   {fitness}");
    // ui::draw_board(
    //     best_chromosome.get_positions(),
    //     best_chromosome.get_conflicts(),
    // );

    let population = vec![
        Chromosome::new(vec![1, 3, 5, 7, 2, 0, 6, 4]),
        Chromosome::new(vec![0, 2, 4, 6, 1, 3, 5, 7]),
        Chromosome::new(vec![2, 4, 1, 7, 5, 0, 6, 3]),
        Chromosome::new(vec![2, 4, 1, 7, 6, 0, 3, 5]),
        Chromosome::new(vec![2, 4, 5, 7, 6, 0, 3, 1]),
        Chromosome::new(vec![1, 4, 5, 7, 6, 0, 3, 2]),
        Chromosome::new(vec![2, 4, 1, 7, 6, 0, 5, 3]),
    ];
    let mut genetic_algorithm = ga::GeneticAlgorithm::new(population);
    genetic_algorithm.calc_fitness();
    let population = genetic_algorithm.get_population();

    for c in population {
        let conflicts_sum = c.get_conflicts_sum();
        let fitness = c.get_fitness();
        println!("--------------------------------");
        println!("Conflicts: {conflicts_sum}");
        println!("Fitness:   {fitness}");
        ui::draw_board(c.get_positions(), c.get_conflicts());
    }
}
