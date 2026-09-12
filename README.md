# N-Queens Problem

A Rust implementation of the N-Queens problem using a genetic algorithm.

## Quickstart

```bash
cargo run --release
```

Run with explicit parameters:

```bash
cargo run --release -- --size 18 --population 40000 --epochs 5000 --seed 42 --mutation-rate 0.08 --elite-ratio 0.10 --offspring-ratio 0.10 --selection tournament --tournament-size 3 --local-search-rate 0.05 --local-search-attempts 8
```

Short aliases are also available:

```bash
cargo run --release -- -n 18 -p 40000 -e 5000 -s 42 -m 0.08 -r 0.10 -o 0.10
```

## GUI

Run the native desktop GUI:

```bash
cargo run --release --features gui --bin n_queens_gui
```

The GUI exposes the solver parameters, runs the genetic algorithm on a background thread, supports cancellation, renders the best board, and charts conflict/diversity metrics as epochs complete. Its progress queue holds at most one snapshot, and chart history retains at most 1,024 compact metric points. Intermediate updates may be skipped; the final result is always delivered. Closing the window requests cancellation, and unexpected worker disconnection is shown as an error.

## CLI options

- `-n`, `--size <size>`: board size (number of queens, must be greater than 0). Default: `18`.
- `-p`, `--population <count>`: initial and target population size. Default: `40000`.
- `-e`, `--epochs <count>`: maximum GA epochs. Default: `5000`.
- `-s`, `--seed <u64>`: optional deterministic RNG seed.
- `-m`, `--mutation-rate <0..1>`: probability of mutating each non-elite chromosome. Default: `0.08`.
- `-r`, `--elite-ratio <0..1>`: fraction of top chromosomes retained before random survivor sampling. Default: `0.10`.
- `-o`, `--offspring-ratio <0..1>`: fraction of the target population produced as offspring each epoch. Default: `0.10`.
- `--min-diversity-ratio <0..1>`: minimum unique-chromosome ratio before non-elites are randomly refreshed. Default: `0.10`.
- `--selection <roulette|tournament>`: parent selection strategy. Default: `roulette`.
- `--tournament-size <count>`: candidate count for tournament selection. Default: `3`.
- `--local-search-rate <0..1>`: fraction of non-elite chromosomes improved with local search each epoch. Default: `0`.
- `--local-search-attempts <count>`: random improving swaps attempted per selected chromosome. Default: `8`.
- `--no-board`: skip board rendering output.
- `--metrics-csv <path>`: write per-epoch run metrics to a CSV file (includes best/average conflicts, unique chromosomes, adaptive rates, offspring count, local-search improvements, stagnation, and elapsed ms).
- `--allow-unsolvable`: evolve sizes 2 and 3 for experiments; normally these stop after epoch zero with `unsolvable`.
- `--profile`: include cumulative phase timings in the text or JSON summary.
- `--json`: print a machine-readable JSON summary. This suppresses logs and board rendering so stdout remains valid JSON.
- `--log-level <level>`: log level (`off`, `error`, `warn`, `info`, `debug`, or `trace`). Default: `info`.
- `--quiet`: suppress log output.

If `--seed` is omitted, a random seed is generated and logged.

## Tuning guidance

Run tuning experiments with `cargo run --release`, fixed `--seed` values, and either `--metrics-csv` or the `parameter_sweep` example. Compare configurations across multiple seeds by solve rate first, then median solved epoch and elapsed time.

- Start from the defaults for `--size 18`, then change one family of parameters at a time.
- Increase `--population` when runs fail because the search converges too early. Larger populations preserve more candidates but increase per-epoch work.
- Increase `--epochs` when best conflicts are still improving near the limit. If the run is flat for many epochs, tune exploration instead of only adding epochs.
- Adjust `--mutation-rate` in small steps. Lower values preserve good partial solutions; higher values explore more aggressively. The solver already boosts mutation during stagnation, so treat this as the base rate.
- Adjust `--elite-ratio` to balance preserving winners against premature convergence. Higher values protect good chromosomes; lower values make survivor selection more exploratory.
- Tune `--offspring-ratio` to control GA turnover. For example, `0.10` creates offspring equal to 10% of the target population before survivor selection. Higher values explore faster but add crossover work.
- Tune `--min-diversity-ratio` when metrics show duplicate-heavy populations. If diversity drops below the threshold, the solver refreshes non-elite chromosomes with random permutations.
- Use `--selection tournament` when roulette selection is slow to improve. Larger `--tournament-size` increases selection pressure but can reduce diversity.
- Use `--local-search-rate` for harder boards when the GA often gets close but does not finish. Start low, such as `0.02` to `0.05`, and increase `--local-search-attempts` only if metrics show useful local-search improvements.
- Lower population, offspring ratio, local-search rate, or local-search attempts when elapsed time is the limiting factor rather than solve rate.

## Parameter sweeps

Run multiple seeds per configuration and compare solve rate, median solved epoch, and runtime:

```bash
cargo run --release --example parameter_sweep -- --sizes 18 --populations 40000 --epochs 5000 --seeds 20 --mutation-rates 0.06,0.08 --elite-ratios 0.05,0.10 --offspring-ratios 0.05,0.10 --min-diversity-ratios 0.05,0.10 --selection-strategies roulette,tournament --tournament-sizes 3,5 --local-search-rates 0,0.05 --local-search-attempts 8
```

The sweep prints CSV rows with one aggregate result per parameter combination. Add `--output-dir PATH` to save an experiment to a **new** directory:

```bash
cargo run --release --locked --example parameter_sweep -- --sizes 18 --populations 4000,40000 --epochs 200 --seeds 20 --selection-strategies roulette,tournament --local-search-rates 0,0.05 --output-dir sweep-results
```

- `runs.jsonl`: one flushed record per completed seed, with full GA configuration, stop reason, solved epoch, remaining conflicts, and elapsed nanoseconds/milliseconds.
- `summary.csv`: the same aggregates printed to stdout, written after each configuration.
- `metadata.json`: seed range, command arguments, Git revision/status, compiler, OS/architecture, available CPUs, actual Rayon thread count, build mode, and profiling setting.
- `source/` and `source.patch`: workspace Rust sources, Cargo manifest/lockfile, toolchain file, and tracked changes at experiment start. Use `cargo run` from the matching source tree so the binary matches this snapshot. Identical seeds alone do not promise identical results across dependency/toolchain changes.

The directory must not already exist and must be outside `src`, `examples`, `benches`, and `tests`. Use `--seed-start` for independent validation seeds and `--profile` to include phase timings in each seed record. Interrupted sweeps retain completed seed records even if the current configuration has no aggregate yet. Runtime aggregates include unsolved runs; median solved epoch includes only solved runs. Elapsed time includes population construction.

## Measured N=18 presets

A release-build comparison on macOS/aarch64 with eight Rayon threads used 200 epochs and independent validation seeds 101–120:

| Preset | Population | Selection | Local-search rate | Solved | Median runtime |
| --- | ---: | --- | ---: | ---: | ---: |
| Compact hybrid | 4,000 | Tournament | 0.05 | 20/20 | 41.5 ms |
| Roulette hybrid | 4,000 | Roulette | 0.05 | 20/20 | 69.0 ms |
| Pure GA | 40,000 | Tournament | 0 | 20/20 | 259.5 ms |
| Current default parameters | 40,000 | Roulette | 0 | 20/20 | 373.0 ms |

All other GA parameters use their defaults. These measurements support trying the compact hybrid for N=18; they do not guarantee success or generalize to other board sizes. The default population and 5,000-epoch budget remain unchanged. The GUI includes a **Measured 18×18 values** button.

```bash
cargo run --release --locked -- --size 18 --population 4000 --epochs 200 --seed 42 --selection tournament --local-search-rate 0.05
```

The repository's `benchmarks/README.md` contains the other preset commands, methodology, raw seed results, source archives, and phase measurements.

## Library usage

```rust
use n_queens_problem::ga::{self, GaConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = GaConfig::new(18, 40_000, 5_000, 42)
        .with_mutation_rate(0.08)
        .with_elite_ratio(0.10)
        .with_offspring_ratio(0.10)
        .with_min_diversity_ratio(0.10)
        .with_selection_strategy(ga::SelectionStrategy::Tournament)
        .with_tournament_size(3)
        .with_local_search_rate(0.05)
        .with_local_search_attempts(8)
        .validated()?;

    let mut algorithm = ga::build_genetic_algorithm(config)?;
    let metrics = algorithm.run_algorithm();

    println!(
        "best conflicts: {}",
        algorithm.get_best_chromosome().get_conflicts_sum()
    );
    println!("solved epoch: {:?}", metrics.solved_epoch());

    Ok(())
}
```

`metrics.stop_reason()` reports `StopReason::Solved`, `EpochLimit`, `Cancelled`, or `Unsolvable`. CLI JSON exposes these as `solved`, `epoch_limit`, `cancelled`, and `unsolvable`; CSV adds `stop_reason` and `allow_unsolvable` columns. Unsolved outcomes are valid run results and retain a successful CLI exit status. Invalid input and I/O failures still return an error status.

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

## Docs site (Astro)

```bash
npm install
npm run dev
npm run build
```

## GitHub Wiki sync

`src/content/docs/index.md` is synced to GitHub Wiki `Home.md` by `.github/workflows/wiki-sync.yml`.

- Enable Wiki in the repository settings (`Settings -> General -> Features -> Wikis`).
- Push changes to `src/content/docs/index.md` on `main` or run the workflow manually.

## Development checks

```bash
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test
cargo test --all-features --examples
cargo bench --bench ga
cargo bench --bench ga --features bench-internals -- phases
```

## Example board output (8x8)

```text
╔════╤════╤════╤════╤════╤════╤════╤════╗
║    │    │    │    │    │ 00 │    │    ║
╟────┼────┼────┼────┼────┼────┼────┼────╢
║ 00 │    │    │    │    │    │    │    ║
╟────┼────┼────┼────┼────┼────┼────┼────╢
║    │    │    │    │ 00 │    │    │    ║
╟────┼────┼────┼────┼────┼────┼────┼────╢
║    │ 00 │    │    │    │    │    │    ║
╟────┼────┼────┼────┼────┼────┼────┼────╢
║    │    │    │    │    │    │    │ 00 ║
╟────┼────┼────┼────┼────┼────┼────┼────╢
║    │    │ 00 │    │    │    │    │    ║
╟────┼────┼────┼────┼────┼────┼────┼────╢
║    │    │    │    │    │    │ 00 │    ║
╟────┼────┼────┼────┼────┼────┼────┼────╢
║    │    │    │ 00 │    │    │    │    ║
╚════╧════╧════╧════╧════╧════╧════╧════╝
```

## Profiling

```bash
cargo run --release --locked -- --size 18 --population 4000 --seed 42 --local-search-rate 0.05 --profile --json
cargo bench --bench ga --features bench-internals -- phases
```

Profiling is opt-in and does not change RNG consumption. `phase_timings` is `null` when disabled. Timings are cumulative wall-clock nanoseconds for crossover (including parent selection), mutation, local search, survivor selection, fitness, population metrics (including the uniqueness HashSet), diversity refresh, and restarts. These phases do not overlap. Solver timing excludes population construction, and phase totals exclude bookkeeping, snapshot creation, and callbacks.

The `phases` Criterion group measures crossover, mutation, local search, and population/diversity metrics independently at populations of 1,000 and 40,000. Each sample starts with a fresh seeded population; construction and initial fitness calculation are outside the measured section. Local search uses rate 0.05 with eight attempts. `bench-internals` exposes unstable benchmark hooks and is unnecessary for normal builds.
