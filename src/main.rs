use crate::ga::GeneticAlgorithm;

pub mod ga;
pub mod ui;

fn main() {
    simple_logger::init().unwrap();
    let mut genetic_algorithm = GeneticAlgorithm::new(8, 4);
    for chromosome in genetic_algorithm.get_population() {
        let conflicts_sum = chromosome.get_conflicts_sum();
        println!("--------------------------------");
        println!("Conflicts: {conflicts_sum}");
        ui::draw_board(chromosome.get_positions(), chromosome.get_conflicts());
    }
    let best_chromosome = genetic_algorithm.run_epoch();
    let conflicts_sum = best_chromosome.get_conflicts_sum();
    println!("--------------------------------");
    println!("Best Conflicts: {conflicts_sum}");
    ui::draw_board(
        best_chromosome.get_positions(),
        best_chromosome.get_conflicts(),
    );
    println!("--------------------------------");
}
