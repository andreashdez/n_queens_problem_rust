use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process,
};

use clap::{ArgAction, Parser};
use n_queens_problem::{ga, tui};
use rand::RngExt;
use serde_json::json;
use simple_logger::SimpleLogger;

const DEFAULT_BOARD_SIZE: u16 = 18;
const DEFAULT_POPULATION_SIZE: usize = 40_000;
const DEFAULT_MAX_EPOCHS: u32 = 5_000;
const DEFAULT_MUTATION_RATE: f32 = ga::DEFAULT_MUTATION_RATE;
const DEFAULT_ELITE_RATIO: f32 = ga::DEFAULT_ELITE_RATIO;
const DEFAULT_OFFSPRING_RATIO: f32 = ga::DEFAULT_OFFSPRING_RATIO;
const DEFAULT_MIN_DIVERSITY_RATIO: f32 = ga::DEFAULT_MIN_DIVERSITY_RATIO;
const DEFAULT_SELECTION_STRATEGY: ga::SelectionStrategy = ga::DEFAULT_SELECTION_STRATEGY;
const DEFAULT_TOURNAMENT_SIZE: usize = ga::DEFAULT_TOURNAMENT_SIZE;
const DEFAULT_LOCAL_SEARCH_RATE: f32 = ga::DEFAULT_LOCAL_SEARCH_RATE;
const DEFAULT_LOCAL_SEARCH_ATTEMPTS: usize = ga::DEFAULT_LOCAL_SEARCH_ATTEMPTS;

#[derive(Debug, Parser)]
#[command(name = "n_queens_problem")]
#[command(version)]
#[command(about = "N-Queens genetic solver")]
#[command(
    after_help = "Examples:\n  cargo run --release\n  cargo run --release -- -n 18 -p 40000 -e 5000 -s 42 -m 0.08 -r 0.10 -o 0.10 --local-search-rate 0.05"
)]
struct RunConfig {
    #[arg(
        short = 'n',
        long = "size",
        value_name = "SIZE",
        default_value_t = DEFAULT_BOARD_SIZE,
        value_parser = parse_positive_u16,
        help = "Board size (number of queens, must be greater than 0)"
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
        short = 'o',
        long = "offspring-ratio",
        value_name = "0..1",
        default_value_t = DEFAULT_OFFSPRING_RATIO,
        value_parser = parse_unit_interval,
        help = "Fraction of target population produced as offspring each epoch"
    )]
    offspring_ratio: f32,
    #[arg(
        long = "min-diversity-ratio",
        value_name = "0..1",
        default_value_t = DEFAULT_MIN_DIVERSITY_RATIO,
        value_parser = parse_unit_interval,
        help = "Minimum unique-chromosome ratio before random refresh"
    )]
    min_diversity_ratio: f32,
    #[arg(
        long = "selection",
        value_name = "roulette|tournament",
        default_value_t = DEFAULT_SELECTION_STRATEGY,
        value_parser = parse_selection_strategy,
        help = "Parent selection strategy"
    )]
    selection_strategy: ga::SelectionStrategy,
    #[arg(
        long = "tournament-size",
        value_name = "COUNT",
        default_value_t = DEFAULT_TOURNAMENT_SIZE,
        value_parser = parse_positive_usize,
        help = "Candidate count for tournament selection"
    )]
    tournament_size: usize,
    #[arg(
        long = "local-search-rate",
        value_name = "0..1",
        default_value_t = DEFAULT_LOCAL_SEARCH_RATE,
        value_parser = parse_unit_interval,
        help = "Fraction of non-elite chromosomes improved with local search each epoch"
    )]
    local_search_rate: f32,
    #[arg(
        long = "local-search-attempts",
        value_name = "COUNT",
        default_value_t = DEFAULT_LOCAL_SEARCH_ATTEMPTS,
        value_parser = parse_usize,
        help = "Random improving swaps attempted per selected chromosome"
    )]
    local_search_attempts: usize,
    #[arg(
        long = "no-board",
        action = ArgAction::SetFalse,
        default_value_t = true,
        help = "Skip board rendering output"
    )]
    draw_board: bool,
    #[arg(
        long = "metrics-csv",
        value_name = "PATH",
        help = "Write per-epoch run metrics to CSV"
    )]
    metrics_csv: Option<PathBuf>,
    #[arg(
        long = "log-level",
        value_name = "LEVEL",
        default_value = "info",
        value_parser = parse_log_level,
        help = "Log level: off, error, warn, info, debug, or trace"
    )]
    log_level: log::LevelFilter,
    #[arg(
        long = "quiet",
        action = ArgAction::SetTrue,
        help = "Suppress log output"
    )]
    quiet: bool,
    #[arg(
        long = "json",
        action = ArgAction::SetTrue,
        help = "Print a machine-readable JSON summary"
    )]
    json_output: bool,
}

fn chromosome_json(chromosome: &ga::chromosome::Chromosome) -> serde_json::Value {
    json!({
        "positions": chromosome.get_positions(),
        "conflicts": chromosome.get_conflicts(),
        "conflicts_sum": chromosome.get_conflicts_sum(),
    })
}

fn json_ratio(value: f32) -> f64 {
    (f64::from(value) * 1_000_000.0).round() / 1_000_000.0
}

fn print_run_summary_json(
    run_config: &RunConfig,
    seed: u64,
    run_metrics: &ga::RunMetrics,
    best_chromosome: &ga::chromosome::Chromosome,
    worst_chromosome: &ga::chromosome::Chromosome,
    final_population: usize,
    metrics_csv: Option<&Path>,
) -> Result<(), String> {
    let final_epoch = run_metrics.epochs().last();
    let summary = json!({
        "seed": seed,
        "board_size": run_config.board_size,
        "target_population": run_config.population_size,
        "max_epochs": run_config.max_epochs,
        "mutation_rate": json_ratio(run_config.mutation_rate),
        "elite_ratio": json_ratio(run_config.elite_ratio),
        "offspring_ratio": json_ratio(run_config.offspring_ratio),
        "min_diversity_ratio": json_ratio(run_config.min_diversity_ratio),
        "selection_strategy": run_config.selection_strategy.to_string(),
        "tournament_size": run_config.tournament_size,
        "local_search_rate": json_ratio(run_config.local_search_rate),
        "local_search_attempts": run_config.local_search_attempts,
        "final_population": final_population,
        "final_unique_chromosomes": final_epoch.map(|metrics| metrics.unique_chromosomes()),
        "final_diversity_ratio": final_epoch.map(|metrics| json_ratio(metrics.diversity_ratio())),
        "last_local_search_improvements": final_epoch
            .map(|metrics| metrics.local_search_improvements())
            .unwrap_or_default(),
        "last_diversity_replacements": final_epoch
            .map(|metrics| metrics.diversity_replacements())
            .unwrap_or_default(),
        "elapsed_ms": run_metrics.total_elapsed_ms(),
        "solved_epoch": run_metrics.solved_epoch(),
        "metrics_csv": metrics_csv.map(|path| path.display().to_string()),
        "best_chromosome": chromosome_json(best_chromosome),
        "worst_chromosome": chromosome_json(worst_chromosome),
    });

    serde_json::to_writer_pretty(std::io::stdout(), &summary)
        .map_err(|error| format!("failed to write JSON summary: {error}"))?;
    println!();
    Ok(())
}

fn write_run_metrics_csv(
    metrics_path: &Path,
    run_config: &RunConfig,
    seed: u64,
    run_metrics: &ga::RunMetrics,
) -> Result<(), String> {
    if let Some(parent) = metrics_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create metrics directory `{}`: {error}",
                parent.display()
            )
        })?;
    }

    let mut metrics_file = File::create(metrics_path).map_err(|error| {
        format!(
            "failed to create metrics file `{}`: {error}",
            metrics_path.display()
        )
    })?;

    writeln!(
        metrics_file,
        "seed,board_size,target_population,max_epochs,mutation_rate,elite_ratio,offspring_ratio,min_diversity_ratio,selection_strategy,tournament_size,local_search_rate,local_search_attempts,epoch,best_conflicts_sum,population_size,elapsed_ms,average_conflicts_sum,unique_chromosomes,diversity_ratio,epoch_mutation_rate,epoch_elite_ratio,offspring_count,local_search_improvements,stagnation_epochs,diversity_replacements"
    )
    .map_err(|error| {
        format!(
            "failed to write metrics header `{}`: {error}",
            metrics_path.display()
        )
    })?;

    for epoch_metrics in run_metrics.epochs() {
        writeln!(
            metrics_file,
            "{seed},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            run_config.board_size,
            run_config.population_size,
            run_config.max_epochs,
            run_config.mutation_rate,
            run_config.elite_ratio,
            run_config.offspring_ratio,
            run_config.min_diversity_ratio,
            run_config.selection_strategy,
            run_config.tournament_size,
            run_config.local_search_rate,
            run_config.local_search_attempts,
            epoch_metrics.epoch(),
            epoch_metrics.best_conflicts_sum(),
            epoch_metrics.population_size(),
            epoch_metrics.elapsed_ms(),
            epoch_metrics.average_conflicts_sum(),
            epoch_metrics.unique_chromosomes(),
            epoch_metrics.diversity_ratio(),
            epoch_metrics.mutation_rate(),
            epoch_metrics.elite_ratio(),
            epoch_metrics.offspring_count(),
            epoch_metrics.local_search_improvements(),
            epoch_metrics.stagnation_epochs(),
            epoch_metrics.diversity_replacements(),
        )
        .map_err(|error| {
            format!(
                "failed to write metrics row `{}`: {error}",
                metrics_path.display()
            )
        })?;
    }

    Ok(())
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

fn parse_positive_u16(raw_value: &str) -> Result<u16, String> {
    let value = raw_value
        .parse::<u16>()
        .map_err(|err| format!("invalid value `{raw_value}`: {err}"))?;

    if value == 0 {
        return Err("must be greater than 0".to_owned());
    }

    Ok(value)
}

fn parse_usize(raw_value: &str) -> Result<usize, String> {
    raw_value
        .parse::<usize>()
        .map_err(|err| format!("invalid value `{raw_value}`: {err}"))
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

fn parse_selection_strategy(raw_value: &str) -> Result<ga::SelectionStrategy, String> {
    match raw_value.to_ascii_lowercase().as_str() {
        "roulette" => Ok(ga::SelectionStrategy::Roulette),
        "tournament" => Ok(ga::SelectionStrategy::Tournament),
        _ => Err("must be one of: roulette, tournament".to_owned()),
    }
}

fn parse_log_level(raw_value: &str) -> Result<log::LevelFilter, String> {
    match raw_value.to_ascii_lowercase().as_str() {
        "off" => Ok(log::LevelFilter::Off),
        "error" => Ok(log::LevelFilter::Error),
        "warn" | "warning" => Ok(log::LevelFilter::Warn),
        "info" => Ok(log::LevelFilter::Info),
        "debug" => Ok(log::LevelFilter::Debug),
        "trace" => Ok(log::LevelFilter::Trace),
        _ => Err("must be one of: off, error, warn, info, debug, trace".to_owned()),
    }
}

fn main() {
    let run_config = RunConfig::parse();
    let log_level = if run_config.quiet || run_config.json_output {
        log::LevelFilter::Off
    } else {
        run_config.log_level
    };

    SimpleLogger::new().with_level(log_level).init().unwrap();

    let seed = run_config
        .seed
        .unwrap_or_else(|| rand::rng().random::<u64>());
    let ga_config = ga::GaConfig::new(
        run_config.board_size,
        run_config.population_size,
        run_config.max_epochs,
        seed,
    )
    .with_mutation_rate(run_config.mutation_rate)
    .with_elite_ratio(run_config.elite_ratio)
    .with_offspring_ratio(run_config.offspring_ratio)
    .with_min_diversity_ratio(run_config.min_diversity_ratio)
    .with_selection_strategy(run_config.selection_strategy)
    .with_tournament_size(run_config.tournament_size)
    .with_local_search_rate(run_config.local_search_rate)
    .with_local_search_attempts(run_config.local_search_attempts)
    .validated()
    .unwrap_or_else(|error| {
        eprintln!("invalid GA config: {error}");
        process::exit(2);
    });

    log::info!(
        "start n_queens_problem board_size={} population={} epochs={} seed={seed} mutation_rate={} elite_ratio={} offspring_ratio={} min_diversity_ratio={} selection_strategy={} tournament_size={} local_search_rate={} local_search_attempts={} draw_board={}",
        run_config.board_size,
        run_config.population_size,
        run_config.max_epochs,
        run_config.mutation_rate,
        run_config.elite_ratio,
        run_config.offspring_ratio,
        run_config.min_diversity_ratio,
        run_config.selection_strategy,
        run_config.tournament_size,
        run_config.local_search_rate,
        run_config.local_search_attempts,
        run_config.draw_board,
    );

    let mut genetic_algorithm = ga::build_genetic_algorithm(ga_config).unwrap_or_else(|error| {
        eprintln!("invalid GA config: {error}");
        process::exit(2);
    });

    log::info!("done building genetic algorithm");
    let run_metrics = genetic_algorithm.run_algorithm();

    if let Some(metrics_path) = run_config.metrics_csv.as_deref() {
        write_run_metrics_csv(metrics_path, &run_config, seed, &run_metrics).unwrap_or_else(
            |error| {
                eprintln!("{error}");
                process::exit(2);
            },
        );
        if !run_config.json_output {
            println!("Metrics written to {}", metrics_path.display());
        }
    }

    let best_chromosome = genetic_algorithm.get_best_chromosome();
    let worst_chromosome = genetic_algorithm.get_worst_chromosome();
    let population_size = genetic_algorithm.get_population_size();

    if run_config.json_output {
        print_run_summary_json(
            &run_config,
            seed,
            &run_metrics,
            best_chromosome,
            worst_chromosome,
            population_size,
            run_config.metrics_csv.as_deref(),
        )
        .unwrap_or_else(|error| {
            eprintln!("{error}");
            process::exit(2);
        });
        return;
    }

    log::info!("done running epoch");
    println!("--------------------------------");
    println!("Best  = {best_chromosome:?}");
    println!("Worst = {worst_chromosome:?}");
    println!("Final Population: {population_size}");
    println!("Elapsed (ms): {}", run_metrics.total_elapsed_ms());
    if let Some(solved_epoch) = run_metrics.solved_epoch() {
        println!("Solved Epoch: {solved_epoch}");
    }

    if !run_config.draw_board {
        println!("Board rendering disabled (--no-board).");
    } else {
        let best_positions = best_chromosome.get_positions();
        let best_conflicts = best_chromosome.get_conflicts();
        tui::draw_board(best_positions, best_conflicts);
    }

    log::info!("done n_queens_problem");
}
