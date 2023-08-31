pub mod ga;
pub mod ui;

fn main() {
    simple_logger::init().unwrap();

    let mut genetic_algorithm = ga::build_genetic_algorithm(8, 40);
    let best_chromosome = genetic_algorithm.run_epoch();
    let conflicts_sum = best_chromosome.get_conflicts_sum();
    let fitness = best_chromosome.get_fitness();
    println!("--------------------------------");
    println!("Best Conflicts: {conflicts_sum}");
    println!("Fitness:   {fitness}");
    ui::draw_board(
        best_chromosome.get_positions(),
        best_chromosome.get_conflicts(),
    );
}
