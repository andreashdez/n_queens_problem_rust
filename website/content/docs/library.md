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

`MinConflictsConfig::new(size, max_passes, seed)` rejects a zero size or pass budget. Sizes 2 and 3 stop as `Unsolvable` unless you add `.with_allow_unsolvable(true)`. The result's `best_chromosome()` is the same `Chromosome` type the GA returns.

[Understand the algorithm →](../algorithm/) · [Development checks →](../development/)
