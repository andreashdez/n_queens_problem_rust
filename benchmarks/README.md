# Measurements from 2026-09-12

These measurements cover 18×18 boards on this machine: macOS/aarch64, eight Rayon threads, Rust 1.98.1, release builds with the updated lockfile. They compare the corrected solver with a 200-epoch budget. They do not establish optimal settings for other board sizes or machines.

## Measured presets

All settings use mutation 0.08, elite ratio 0.10, offspring ratio 0.10, minimum diversity 0.10, tournament size 3, and eight local-search attempts. The table uses the independent validation seeds 101–120; the earlier exploration used seeds 1–10. Median runtime includes population construction and all runs. Median solved epoch includes successful runs only.

| Configuration | Population | Selection | Local-search rate | Solved | Median solved epoch | Median runtime |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| Compact hybrid | 4,000 | Tournament | 0.05 | 20/20 | 50.5 | 41.5 ms |
| Roulette hybrid | 4,000 | Roulette | 0.05 | 20/20 | 85.5 | 69.0 ms |
| Pure GA | 40,000 | Tournament | 0 | 20/20 | 60.5 | 259.5 ms |
| Current default parameters | 40,000 | Roulette | 0 | 20/20 | 81.0 | 373.0 ms |
| Larger hybrid | 40,000 | Tournament | 0.05 | 20/20 | 15.0 | 112.5 ms |

The compact hybrid is a useful starting point for N=18 and is available through the GUI's **Measured 18×18 values** button. Twenty successful validation seeds are limited evidence, not a guarantee. The population default remains 40,000: it supports the existing pure-GA behavior, while switching the default to local search would change that behavior. The current default epoch budget remains 5,000; these comparisons cap every configuration at 200.

```bash
# Compact hybrid
cargo run --release --locked -- --size 18 --population 4000 --epochs 200 --seed 42 --selection tournament --local-search-rate 0.05

# Roulette hybrid
cargo run --release --locked -- --size 18 --population 4000 --epochs 200 --seed 42 --selection roulette --local-search-rate 0.05

# Pure GA
cargo run --release --locked -- --size 18 --population 40000 --epochs 200 --seed 42 --selection tournament --local-search-rate 0
```

## Evidence and reproduction

- [Exploration aggregates](2026-09-12-n18/summary.csv): 120 runs across 12 configurations, seeds 1–10.
- [Validation aggregates](2026-09-12-validation/summary.csv): 160 runs across eight configurations, independent seeds 101–120.
- [Profiled aggregates](2026-09-12-profile/summary.csv): 20 additional runs with profiling enabled, seeds 1–10.

Each directory contains `runs.jsonl`, `metadata.json`, and `source.tar.gz`. The archive contains the exact workspace Rust sources, lockfile, toolchain, and tracked patch captured at the start of that experiment. Sources were captured while this change was in progress; later GUI/test/doc edits do not affect the measured GA. Archives were created after the runs to keep the committed evidence compact. New sweep invocations write an uncompressed `source/` directory and `source.patch`.

```bash
cargo run --release --locked --example parameter_sweep -- --sizes 18 --populations 1000,4000,40000 --epochs 200 --seeds 10 --selection-strategies roulette,tournament --local-search-rates 0,0.05 --output-dir exploration-new
cargo run --release --locked --example parameter_sweep -- --sizes 18 --populations 4000,40000 --epochs 200 --seed-start 101 --seeds 20 --selection-strategies roulette,tournament --local-search-rates 0,0.05 --output-dir validation-new
cargo run --release --locked --example parameter_sweep -- --sizes 18 --populations 4000,40000 --epochs 200 --seeds 10 --selection-strategies tournament --local-search-rates 0.05 --profile --output-dir profile-new
```

## Phase costs

For the profiled tournament/local-search runs, the aggregate fractions of measured phase time were:

| Phase | Population 4,000 | Population 40,000 |
| --- | ---: | ---: |
| Local search | 31.6% | 35.0% |
| Crossover and parent selection | 20.2% | 25.3% |
| Mutation | 17.6% | 9.1% |
| Population metrics, including uniqueness | 11.2% | 17.0% |
| Fitness | 11.0% | 3.2% |
| Survivor selection | 7.7% | 10.3% |
| Restart | 0.7% | 0.0% |

Diversity refresh rounded to 0.0% in these runs. The denominator excludes construction, callbacks, and other bookkeeping. This points to local search and crossover as the first phases to investigate; a uniqueness-only optimization would address a smaller portion of measured time. Profiling does not itself establish that a proposed optimization will help.

[Criterion output](2026-09-12-criterion.txt) contains all conflict-count, epoch-loop, and phase benchmarks. The phase group's central estimates were:

| Phase | Population 1,000 | Population 40,000 |
| --- | ---: | ---: |
| Crossover | 44.1 µs | 1.85 ms |
| Mutation | 42.7 µs | 484 µs |
| Local search | 53.0 µs | 2.07 ms |
| Population/diversity metrics | 21.4 µs | 886 µs |

The Criterion run used the same solver/harness source as the profile snapshot, with `bench-internals` enabled. These are short diagnostic measurements (10 samples, 0.2-second warmup, 0.5-second target measurement), with outliers noted in the raw output. Use longer measurements for performance decisions.

```bash
cargo bench --locked --bench ga --features bench-internals -- --warm-up-time 0.2 --measurement-time 0.5 --sample-size 10
```
