use simple_logger::SimpleLogger;

pub mod ga;
pub mod ui;

fn main() {
    SimpleLogger::new()
        // .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();

    log::info!("start n_queens_problem");
    let mut genetic_algorithm = ga::build_genetic_algorithm(8, 40);
    log::info!("done building genetic algorithm");
    let best_chromosome = genetic_algorithm.run_algorithm();
    log::info!("done running epoch");
    println!("--------------------------------");
    println!("Best Chromosome: {:?}", best_chromosome);
    ui::draw_board(
        best_chromosome.get_positions(),
        best_chromosome.get_conflicts(),
    );
    log::info!("done n_queens_problem");
}
