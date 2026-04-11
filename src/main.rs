use clap::{ArgAction, Parser};
use n_queens_problem::{ga, ui};
use rand::Rng;
use simple_logger::SimpleLogger;

const DEFAULT_BOARD_SIZE: u16 = 18;
const DEFAULT_POPULATION_SIZE: usize = 40_000;
const DEFAULT_MAX_EPOCHS: u32 = 5_000;
const DEFAULT_MUTATION_RATE: f32 = ga::DEFAULT_MUTATION_RATE;
const DEFAULT_ELITE_RATIO: f32 = ga::DEFAULT_ELITE_RATIO;

#[derive(Debug, Parser)]
#[command(name = "n_queens_problem")]
#[command(version)]
#[command(about = "N-Queens genetic solver")]
#[command(
    after_help = "Examples:\n  cargo run --release\n  cargo run --release -- -n 18 -p 40000 -e 5000 -s 42 -m 0.08 -r 0.10"
)]
struct RunConfig {
    #[arg(
        short = 'n',
        long = "size",
        value_name = "SIZE",
        default_value_t = DEFAULT_BOARD_SIZE,
        help = "Board size (number of queens)"
    )]
    board_size: u16,
    #[arg(
        short = 'p',
        long = "population",
        value_name = "COUNT",
        default_value_t = DEFAULT_POPULATION_SIZE,
        value_parser = parse_positive_usize,
        help = "Initial and target population size"
    )]
    population_size: usize,
    #[arg(
        short = 'e',
        long = "epochs",
        value_name = "COUNT",
        default_value_t = DEFAULT_MAX_EPOCHS,
        value_parser = parse_positive_u32,
        help = "Maximum GA epochs"
    )]
    max_epochs: u32,
    #[arg(
        short = 's',
        long = "seed",
        value_name = "U64",
        help = "Deterministic RNG seed"
    )]
    seed: Option<u64>,
    #[arg(
        short = 'm',
        long = "mutation-rate",
        value_name = "0..1",
        default_value_t = DEFAULT_MUTATION_RATE,
        value_parser = parse_unit_interval,
        help = "Mutation probability"
    )]
    mutation_rate: f32,
    #[arg(
        short = 'r',
        long = "elite-ratio",
        value_name = "0..1",
        default_value_t = DEFAULT_ELITE_RATIO,
        value_parser = parse_unit_interval,
        help = "Elite survivor ratio"
    )]
    elite_ratio: f32,
    #[arg(
        long = "no-board",
        action = ArgAction::SetFalse,
        default_value_t = true,
        help = "Skip board rendering output"
    )]
    draw_board: bool,
}

fn parse_positive_usize(raw_value: &str) -> Result<usize, String> {
    let value = raw_value
        .parse::<usize>()
        .map_err(|err| format!("invalid value `{raw_value}`: {err}"))?;

    if value == 0 {
        return Err("must be greater than 0".to_owned());
    }

    Ok(value)
}

fn parse_positive_u32(raw_value: &str) -> Result<u32, String> {
    let value = raw_value
        .parse::<u32>()
        .map_err(|err| format!("invalid value `{raw_value}`: {err}"))?;

    if value == 0 {
        return Err("must be greater than 0".to_owned());
    }

    Ok(value)
}

fn parse_unit_interval(raw_value: &str) -> Result<f32, String> {
    let value = raw_value
        .parse::<f32>()
        .map_err(|err| format!("invalid value `{raw_value}`: {err}"))?;

    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err("must be between 0.0 and 1.0".to_owned());
    }

    Ok(value)
}

fn main() {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();

    let run_config = RunConfig::parse();

    let seed = run_config
        .seed
        .unwrap_or_else(|| rand::rng().random::<u64>());
    log::info!(
        "start n_queens_problem board_size={} population={} epochs={} seed={seed} mutation_rate={} elite_ratio={} draw_board={}",
        run_config.board_size,
        run_config.population_size,
        run_config.max_epochs,
        run_config.mutation_rate,
        run_config.elite_ratio,
        run_config.draw_board,
    );

    let mut genetic_algorithm = ga::build_genetic_algorithm(
        run_config.board_size,
        run_config.population_size,
        run_config.max_epochs,
        seed,
        run_config.mutation_rate,
        run_config.elite_ratio,
    );

    log::info!("done building genetic algorithm");
    genetic_algorithm.run_algorithm();

    let best_chromosome = genetic_algorithm.get_best_chromosome();
    let worst_chromosome = genetic_algorithm.get_worst_chromosome();
    let population_size = genetic_algorithm.get_population_size();

    log::info!("done running epoch");
    println!("--------------------------------");
    println!("Best  = {best_chromosome:?}");
    println!("Worst = {worst_chromosome:?}");
    println!("Final Population: {population_size}");

    if !run_config.draw_board {
        println!("Board rendering disabled (--no-board).");
    } else {
        let best_positions = best_chromosome.get_positions();
        if best_positions.is_empty() {
            println!("Board size is 0; nothing to draw.");
        } else {
            let best_conflicts = best_chromosome.get_conflicts();
            ui::draw_board(best_positions, best_conflicts);
        }
    }

    log::info!("done n_queens_problem");
}
