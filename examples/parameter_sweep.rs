use std::{error::Error, time::Instant};

use clap::Parser;
use n_queens_problem::ga::{self, GaConfig};

#[derive(Debug, Parser)]
#[command(name = "parameter_sweep")]
#[command(about = "Run N-Queens GA parameter sweeps across multiple seeds")]
#[command(
    after_help = "Example:\n  cargo run --release --example parameter_sweep -- --sizes 18 --populations 40000 --epochs 5000 --seeds 20 --mutation-rates 0.06,0.08 --elite-ratios 0.05,0.10 --offspring-ratios 0.05,0.10 --min-diversity-ratios 0.05,0.10 --selection-strategies roulette,tournament --tournament-sizes 3,5 --local-search-rates 0,0.05 --local-search-attempts 8"
)]
struct SweepConfig {
    #[arg(
        short = 'n',
        long = "sizes",
        value_name = "SIZE[,SIZE]",
        value_delimiter = ',',
        default_value = "18",
        value_parser = parse_positive_u16,
        help = "Board sizes to test"
    )]
    sizes: Vec<u16>,
    #[arg(
        short = 'p',
        long = "populations",
        value_name = "COUNT[,COUNT]",
        value_delimiter = ',',
        default_value = "40000",
        value_parser = parse_positive_usize,
        help = "Population sizes to test"
    )]
    populations: Vec<usize>,
    #[arg(
        short = 'e',
        long = "epochs",
        value_name = "COUNT[,COUNT]",
        value_delimiter = ',',
        default_value = "5000",
        value_parser = parse_positive_u32,
        help = "Maximum epoch counts to test"
    )]
    epochs: Vec<u32>,
    #[arg(
        long = "seed-start",
        default_value_t = 1,
        help = "First deterministic seed"
    )]
    seed_start: u64,
    #[arg(
        long = "seeds",
        default_value_t = 20,
        value_parser = parse_positive_usize,
        help = "Number of consecutive seeds to run per configuration"
    )]
    seed_count: usize,
    #[arg(
        long = "mutation-rates",
        value_name = "RATE[,RATE]",
        value_delimiter = ',',
        default_value = "0.08",
        value_parser = parse_unit_interval,
        help = "Mutation rates to test"
    )]
    mutation_rates: Vec<f32>,
    #[arg(
        long = "elite-ratios",
        value_name = "RATIO[,RATIO]",
        value_delimiter = ',',
        default_value = "0.10",
        value_parser = parse_unit_interval,
        help = "Elite survivor ratios to test"
    )]
    elite_ratios: Vec<f32>,
    #[arg(
        long = "offspring-ratios",
        value_name = "RATIO[,RATIO]",
        value_delimiter = ',',
        default_value = "0.10",
        value_parser = parse_unit_interval,
        help = "Offspring ratios to test"
    )]
    offspring_ratios: Vec<f32>,
    #[arg(
        long = "min-diversity-ratios",
        value_name = "RATIO[,RATIO]",
        value_delimiter = ',',
        default_value = "0.10",
        value_parser = parse_unit_interval,
        help = "Minimum diversity ratios to test"
    )]
    min_diversity_ratios: Vec<f32>,
    #[arg(
        long = "selection-strategies",
        value_name = "STRATEGY[,STRATEGY]",
        value_delimiter = ',',
        default_value = "roulette",
        value_parser = parse_selection_strategy,
        help = "Parent selection strategies to test"
    )]
    selection_strategies: Vec<ga::SelectionStrategy>,
    #[arg(
        long = "tournament-sizes",
        value_name = "COUNT[,COUNT]",
        value_delimiter = ',',
        default_value = "3",
        value_parser = parse_positive_usize,
        help = "Tournament sizes to test"
    )]
    tournament_sizes: Vec<usize>,
    #[arg(
        long = "local-search-rates",
        value_name = "RATE[,RATE]",
        value_delimiter = ',',
        default_value = "0",
        value_parser = parse_unit_interval,
        help = "Local-search rates to test"
    )]
    local_search_rates: Vec<f32>,
    #[arg(
        long = "local-search-attempts",
        value_name = "COUNT[,COUNT]",
        value_delimiter = ',',
        default_value = "8",
        value_parser = parse_usize,
        help = "Local-search swap-attempt counts to test"
    )]
    local_search_attempts: Vec<usize>,
}

struct SweepRun {
    solved_epoch: Option<u32>,
    elapsed_ms: u128,
    best_conflicts_sum: u32,
}

#[derive(Debug, Clone, Copy)]
struct SweepCase {
    size: u16,
    population: usize,
    epochs: u32,
    mutation_rate: f32,
    elite_ratio: f32,
    offspring_ratio: f32,
    min_diversity_ratio: f32,
    selection_strategy: ga::SelectionStrategy,
    tournament_size: usize,
    local_search_rate: f32,
    local_search_attempts: usize,
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

fn parse_positive_usize(raw_value: &str) -> Result<usize, String> {
    let value = raw_value
        .parse::<usize>()
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

fn seed_for_offset(seed_start: u64, seed_offset: usize) -> Result<u64, String> {
    let seed_offset =
        u64::try_from(seed_offset).map_err(|_| "seed offset does not fit into u64".to_owned())?;

    seed_start
        .checked_add(seed_offset)
        .ok_or_else(|| "seed range overflows u64".to_owned())
}

fn run_single_seed(case: SweepCase, seed: u64) -> Result<SweepRun, ga::GaConfigError> {
    let config = GaConfig::new(case.size, case.population, case.epochs, seed)
        .with_mutation_rate(case.mutation_rate)
        .with_elite_ratio(case.elite_ratio)
        .with_offspring_ratio(case.offspring_ratio)
        .with_min_diversity_ratio(case.min_diversity_ratio)
        .with_selection_strategy(case.selection_strategy)
        .with_tournament_size(case.tournament_size)
        .with_local_search_rate(case.local_search_rate)
        .with_local_search_attempts(case.local_search_attempts)
        .validated()?;

    let started_at = Instant::now();
    let mut algorithm = ga::build_genetic_algorithm(config);
    let metrics = algorithm.run_algorithm();

    Ok(SweepRun {
        solved_epoch: metrics.solved_epoch(),
        elapsed_ms: started_at.elapsed().as_millis(),
        best_conflicts_sum: algorithm.get_best_chromosome().get_conflicts_sum(),
    })
}

fn median_u32(values: &mut [u32]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    values.sort_unstable();
    let middle = values.len() / 2;
    if values.len() % 2 == 1 {
        Some(f64::from(values[middle]))
    } else {
        Some((f64::from(values[middle - 1]) + f64::from(values[middle])) / 2.0)
    }
}

fn median_u128(values: &mut [u128]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    values.sort_unstable();
    let middle = values.len() / 2;
    if values.len() % 2 == 1 {
        Some(values[middle] as f64)
    } else {
        Some((values[middle - 1] as f64 + values[middle] as f64) / 2.0)
    }
}

fn format_optional(value: Option<f64>) -> String {
    value.map(|value| format!("{value:.1}")).unwrap_or_default()
}

fn print_summary(case: SweepCase, runs: &[SweepRun]) {
    let solved_count = runs.iter().filter(|run| run.solved_epoch.is_some()).count();
    let solve_rate = solved_count as f64 / runs.len() as f64;
    let total_elapsed_ms = runs.iter().map(|run| run.elapsed_ms).sum::<u128>();
    let best_conflicts_min = runs
        .iter()
        .map(|run| run.best_conflicts_sum)
        .min()
        .unwrap_or_default();

    let mut solved_epochs = runs
        .iter()
        .filter_map(|run| run.solved_epoch)
        .collect::<Vec<_>>();
    let mut elapsed_values = runs.iter().map(|run| run.elapsed_ms).collect::<Vec<_>>();
    let mut best_conflicts = runs
        .iter()
        .map(|run| run.best_conflicts_sum)
        .collect::<Vec<_>>();

    let size = case.size;
    let population = case.population;
    let epochs = case.epochs;
    let mutation_rate = case.mutation_rate;
    let elite_ratio = case.elite_ratio;
    let offspring_ratio = case.offspring_ratio;
    let min_diversity_ratio = case.min_diversity_ratio;
    let selection_strategy = case.selection_strategy;
    let tournament_size = case.tournament_size;
    let local_search_rate = case.local_search_rate;
    let local_search_attempts = case.local_search_attempts;
    let seed_count = runs.len();
    let median_solved_epoch = format_optional(median_u32(&mut solved_epochs));
    let median_elapsed_ms = format_optional(median_u128(&mut elapsed_values));
    let best_conflicts_median = format_optional(median_u32(&mut best_conflicts));

    println!(
        "{size},{population},{epochs},{mutation_rate:.6},{elite_ratio:.6},{offspring_ratio:.6},{min_diversity_ratio:.6},{selection_strategy},{tournament_size},{local_search_rate:.6},{local_search_attempts},{seed_count},{solved_count},{solve_rate:.3},{median_solved_epoch},{median_elapsed_ms},{total_elapsed_ms},{best_conflicts_median},{best_conflicts_min}",
    );
}

fn main() -> Result<(), Box<dyn Error>> {
    let sweep_config = SweepConfig::parse();

    println!(
        "size,population,epochs,mutation_rate,elite_ratio,offspring_ratio,min_diversity_ratio,selection_strategy,tournament_size,local_search_rate,local_search_attempts,seeds,solved,solve_rate,median_solved_epoch,median_elapsed_ms,total_elapsed_ms,best_conflicts_median,best_conflicts_min"
    );

    for &size in &sweep_config.sizes {
        for &population in &sweep_config.populations {
            for &epochs in &sweep_config.epochs {
                for &mutation_rate in &sweep_config.mutation_rates {
                    for &elite_ratio in &sweep_config.elite_ratios {
                        for &offspring_ratio in &sweep_config.offspring_ratios {
                            for &min_diversity_ratio in &sweep_config.min_diversity_ratios {
                                for &selection_strategy in &sweep_config.selection_strategies {
                                    for &tournament_size in &sweep_config.tournament_sizes {
                                        for &local_search_rate in &sweep_config.local_search_rates {
                                            for &local_search_attempts in
                                                &sweep_config.local_search_attempts
                                            {
                                                let case = SweepCase {
                                                    size,
                                                    population,
                                                    epochs,
                                                    mutation_rate,
                                                    elite_ratio,
                                                    offspring_ratio,
                                                    min_diversity_ratio,
                                                    selection_strategy,
                                                    tournament_size,
                                                    local_search_rate,
                                                    local_search_attempts,
                                                };
                                                let mut runs =
                                                    Vec::with_capacity(sweep_config.seed_count);
                                                for seed_offset in 0..sweep_config.seed_count {
                                                    let seed = seed_for_offset(
                                                        sweep_config.seed_start,
                                                        seed_offset,
                                                    )?;
                                                    let run = run_single_seed(case, seed)?;
                                                    runs.push(run);
                                                }

                                                print_summary(case, &runs);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
