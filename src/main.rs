use simple_logger::SimpleLogger;

pub mod ga;
pub mod ui;

fn main() {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();

    log::info!("start n_queens_problem");
    let mut genetic_algorithm = ga::build_genetic_algorithm(16, 40000);
    log::info!("done building genetic algorithm");
    genetic_algorithm.run_algorithm();
    let best_chromosome = genetic_algorithm.get_best_chromosome();
    let worst_chromosome = genetic_algorithm.get_worst_chromosome();
    let population_size = genetic_algorithm.get_population_size();
    log::info!("done running epoch");
    println!("--------------------------------");
    println!("Best  = {:?}", best_chromosome);
    println!("Worst = {:?}", worst_chromosome);
    println!("Final Population: {:?}", population_size);
    ui::draw_board(
        best_chromosome.get_positions(),
        best_chromosome.get_conflicts(),
    );
    log::info!("done n_queens_problem");
}
