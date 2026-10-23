---
title: "Library usage"
description: "Configure the Rust solver, collect progress, request cancellation, and inspect results."
---

`metrics.stop_reason()` reports `StopReason::Solved`, `EpochLimit`, `Cancelled`, or `Unsolvable`. CLI JSON exposes these as `solved`, `epoch_limit`, `cancelled`, and `unsolvable`; CSV adds `stop_reason` and `allow_unsolvable` columns. Unsolved outcomes are valid run results and keep a successful CLI exit status unless you pass `--fail-unsolved`. Invalid input and I/O failures still return an error status.

For progress, cancellation, bounded metric retention, or profiling:

```rust
use n_queens_problem::ga::{self, GaConfig, RunOptions};

fn main() -> Result<(), ga::GaConfigError> {
    let mut solver = ga::build_genetic_algorithm(GaConfig::new(18, 4_000, 200, 42))?;
    let metrics = solver.run_algorithm_with_options(
        RunOptions {
            allow_unsolvable: false,
            collect_history: false, // only the latest epoch is retained in RunMetrics
            profile: true,
        },
        |snapshot| {
            println!("epoch {}", snapshot.metrics().epoch());
            true // return false to cancel after this completed epoch
        },
    );
    println!("{}", metrics.stop_reason());
    Ok(())
}
```

Epoch zero is reported before evolution. Sizes 2 and 3 stop there unless `allow_unsolvable` is true. A discovered solution takes precedence over a callback cancellation request. The existing `run_algorithm()` and `run_algorithm_with_progress()` retain complete history by default. Both parent selection strategies choose exclusively from the population at the start of mating; children first become eligible in the following epoch. This changes tournament trajectories compared with earlier releases.

To drive a run from your own loop instead of a callback, such as a UI frame that runs a few epochs at a time, start it with `start_run()` and call `step_run()` until it returns false:

```rust
use n_queens_problem::ga::{self, GaConfig, RunOptions};

fn main() -> Result<(), ga::GaConfigError> {
    let mut solver = ga::build_genetic_algorithm(GaConfig::new(18, 500, 200, 42))?;
    let mut run = solver.start_run(RunOptions::default());
    while solver.step_run(&mut run) {
        if run.metrics().epochs().len() > 10_000 {
            run.cancel(); // the next step_run() returns false
        }
    }
    println!("{}", run.into_metrics().stop_reason());
    Ok(())
}
```

`epoch_snapshot(&run)` returns the same snapshot the callback receives. Step a run only with the solver that started it. Stepping gives the same results as `run_algorithm_with_options()` for the same seed.

Use `GaConfig::validated()` or `GaConfig::try_new()` to check configuration before building. `ga::build_genetic_algorithm()` also validates its input and returns an error for invalid public configuration values.

For a non-evolutionary reference point, `min_conflicts::solve()` runs the [min-conflicts baseline](../algorithm/#a-min-conflicts-baseline) on a single board:

```rust
use n_queens_problem::min_conflicts::{self, MinConflictsConfig};

fn main() -> Result<(), min_conflicts::MinConflictsConfigError> {
    let result = min_conflicts::solve(MinConflictsConfig::new(1_000, 100, 42))?;
    println!("{} after {} passes", result.stop_reason(), result.passes());
    println!("conflicts: {}", result.best_chromosome().get_conflicts_sum());
    Ok(())
}
```

To step the search yourself, create a `MinConflictsSearch::new(config)?` and call `step()` until it returns false, then take `into_result()`. `metrics()` and `snapshot()` show the latest pass, and `cancel()` stops the search.

`MinConflictsConfig::new(size, max_passes, seed)` rejects a zero size or pass budget. Sizes 2 and 3 stop as `Unsolvable` unless you add `.with_allow_unsolvable(true)`. The result's `best_chromosome()` is the same `Chromosome` type the GA returns.

[Understand the algorithm →](../algorithm/) · [Development checks →](../development/)
