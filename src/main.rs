use std::{env, process};

use rand::Rng;
use simple_logger::SimpleLogger;

pub mod ga;
pub mod ui;

const DEFAULT_BOARD_SIZE: u16 = 18;
const DEFAULT_POPULATION_SIZE: usize = 40_000;
const DEFAULT_MAX_EPOCHS: u32 = 5_000;
const DEFAULT_MUTATION_RATE: f32 = ga::DEFAULT_MUTATION_RATE;
const DEFAULT_ELITE_RATIO: f32 = ga::DEFAULT_ELITE_RATIO;

#[derive(Debug)]
struct RunConfig {
    board_size: u16,
    population_size: usize,
    max_epochs: u32,
    seed: Option<u64>,
    mutation_rate: f32,
    elite_ratio: f32,
    draw_board: bool,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            board_size: DEFAULT_BOARD_SIZE,
            population_size: DEFAULT_POPULATION_SIZE,
            max_epochs: DEFAULT_MAX_EPOCHS,
            seed: None,
            mutation_rate: DEFAULT_MUTATION_RATE,
            elite_ratio: DEFAULT_ELITE_RATIO,
            draw_board: true,
        }
    }
}

impl RunConfig {
    fn from_args() -> Result<Self, String> {
        let mut config = Self::default();
        let args = env::args().skip(1).collect::<Vec<_>>();
        let mut index = 0;

        while index < args.len() {
            let flag = args[index].as_str();

            match flag {
                "-h" | "--help" => {
                    print_usage();
                    process::exit(0);
                }
                "-n" | "--size" => {
                    index += 1;
                    config.board_size = parse_flag_value(flag, args.get(index))?;
                }
                "-p" | "--population" => {
                    index += 1;
                    config.population_size = parse_flag_value(flag, args.get(index))?;
                }
                "-e" | "--epochs" => {
                    index += 1;
                    config.max_epochs = parse_flag_value(flag, args.get(index))?;
                }
                "-s" | "--seed" => {
                    index += 1;
                    config.seed = Some(parse_flag_value(flag, args.get(index))?);
                }
                "-m" | "--mutation-rate" => {
                    index += 1;
                    config.mutation_rate = parse_flag_value(flag, args.get(index))?;
                }
                "-r" | "--elite-ratio" => {
                    index += 1;
                    config.elite_ratio = parse_flag_value(flag, args.get(index))?;
                }
                "--no-board" => {
                    config.draw_board = false;
                }
                _ => {
                    return Err(format!("unknown argument: `{flag}`"));
                }
            }

            index += 1;
        }

        if config.population_size == 0 {
            return Err("`--population` must be greater than 0".to_owned());
        }

        if config.max_epochs == 0 {
            return Err("`--epochs` must be greater than 0".to_owned());
        }

        if !config.mutation_rate.is_finite() || !(0.0..=1.0).contains(&config.mutation_rate) {
            return Err("`--mutation-rate` must be between 0.0 and 1.0".to_owned());
        }

        if !config.elite_ratio.is_finite() || !(0.0..=1.0).contains(&config.elite_ratio) {
            return Err("`--elite-ratio` must be between 0.0 and 1.0".to_owned());
        }

        Ok(config)
    }
}

fn parse_flag_value<T>(flag: &str, value: Option<&String>) -> Result<T, String>
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    let raw_value = value.ok_or_else(|| format!("missing value for `{flag}`"))?;
    raw_value
        .parse::<T>()
        .map_err(|err| format!("invalid value for `{flag}`: `{raw_value}` ({err})"))
}

fn print_usage() {
    println!("N-Queens genetic solver");
    println!();
    println!("Usage:");
    println!("  cargo run -- [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -n, --size <SIZE>              Board size (queens). Default: {DEFAULT_BOARD_SIZE}");
    println!(
        "  -p, --population <COUNT>       Initial and target population. Default: {DEFAULT_POPULATION_SIZE}"
    );
    println!("  -e, --epochs <COUNT>           Max GA epochs. Default: {DEFAULT_MAX_EPOCHS}");
    println!("  -s, --seed <U64>               RNG seed (optional). Default: random");
    println!(
        "  -m, --mutation-rate <0..1>     Mutation probability. Default: {DEFAULT_MUTATION_RATE}"
    );
    println!(
        "  -r, --elite-ratio <0..1>       Elite survivor ratio. Default: {DEFAULT_ELITE_RATIO}"
    );
    println!("      --no-board                 Skip board rendering output");
    println!("  -h, --help                     Show this help message");
    println!();
    println!("Examples:");
    println!("  cargo run --release");
    println!("  cargo run --release -- -n 18 -p 40000 -e 5000 -s 42 -m 0.08 -r 0.10");
}

fn main() {
    SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();

    let run_config = RunConfig::from_args().unwrap_or_else(|error| {
        eprintln!("{error}");
        print_usage();
        process::exit(2);
    });

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
