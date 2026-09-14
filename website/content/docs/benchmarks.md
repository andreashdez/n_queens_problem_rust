---
title: "Benchmarks and profiling"
description: "Measured presets, reproducibility details, and phase-level profiling."
---

## Measured N=18 presets

These measurements were recorded on **2026-09-14**, on macOS/aarch64 with eight Rayon threads and Rust 1.98.1. Each configuration used a 200-epoch budget and independent validation seeds 101–120. They describe the archived implementation, rather than a new timing of your current checkout.

| Preset | Population | Selection | Local-search rate | Solved | Median runtime |
| --- | ---: | --- | ---: | ---: | ---: |
| Compact hybrid | 4,000 | Tournament | 0.05 | 20/20 | 35.5 ms |
| Roulette hybrid | 4,000 | Roulette | 0.05 | 20/20 | 44.0 ms |
| Pure GA | 40,000 | Tournament | 0 | 20/20 | 101.5 ms |
| Classic GA (defaults before 2026-09-14) | 40,000 | Roulette | 0 | 20/20 | 195.5 ms |

All four measured configurations used mutation 0.08, elite ratio 0.10, offspring ratio 0.10, minimum diversity 0.10, tournament size 3, and eight local-search attempts. None is the shipped default. They support trying local search when a board stalls near a solution at N=18; success is not guaranteed, and these results do not generalize to other board sizes.

The GUI's **Local-search hybrid · 18×18** preset uses today's mutation and offspring defaults and a 5,000-epoch budget. It is not the archived compact-hybrid configuration in this table.

## The shipped defaults changed

On 2026-09-14 the defaults moved from a 40,000-member roulette population to a 500-member tournament population with higher turnover: `--population 500`, `--selection tournament`, `--mutation-rate 0.16`, `--offspring-ratio 0.50`. Compared head to head on 50 seeds that no tuning stage had used, both solved 50/50 at board sizes 8, 18, 32, 64, and 100, while the new defaults ran 32x to 68x faster. Every previous value is still available as a flag, and the GUI keeps them as its **Classic genetic algorithm · 18×18** preset. The [benchmark report](https://github.com/andreashdez/n_queens_problem_rust/blob/main/benchmarks/README.md#choosing-the-defaults) records the sweeps and the trade-offs behind each value.

To run the measured compact-hybrid settings, pass every parameter explicitly. For exact reproduction, run this command from the matching archived source tree described below.

```bash
cargo run --release --locked -- \
  --size 18 \
  --population 4000 \
  --epochs 200 \
  --seed 42 \
  --mutation-rate 0.08 --elite-ratio 0.10 --offspring-ratio 0.10 \
  --min-diversity-ratio 0.10 --selection tournament --tournament-size 3 \
  --local-search-rate 0.05 --local-search-attempts 8
```

The repository’s [benchmark report](https://github.com/andreashdez/n_queens_problem_rust/blob/main/benchmarks/README.md) contains the other preset commands, methodology, raw seed results, source archives, and phase measurements.

## Read the comparison fairly

- **Compare solve rate first.** A short runtime can also mean a run exhausted a small budget without solving.
- **Runtime includes population construction and unsolved runs.** Median solved epoch includes successful runs only.
- **Keep the experiment conditions together.** Board size, seed range, thread count, compiler, and source snapshot all affect the comparison.
- **Validate on new seeds.** The exploration used seeds 1–10; the table uses seeds 101–120.

The table shows four selected presets. The complete validation includes all eight combinations of population (4,000 or 40,000), selection strategy, and local-search rate; some solved fewer than 20 of 20 seeds.

## Inspect the evidence

| Evidence | What it contains |
| --- | --- |
| [All validation results](https://github.com/andreashdez/n_queens_problem_rust/blob/main/benchmarks/2026-09-14-validation/summary.csv) | Aggregate results for all eight configurations. |
| [Per-seed records](https://github.com/andreashdez/n_queens_problem_rust/blob/main/benchmarks/2026-09-14-validation/runs.jsonl) | Configuration, result, and elapsed time for each validation run. |
| [Environment metadata](https://github.com/andreashdez/n_queens_problem_rust/blob/main/benchmarks/2026-09-14-validation/metadata.json) | Compiler, machine, thread count, and source state. |
| [Archived source](https://github.com/andreashdez/n_queens_problem_rust/blob/main/benchmarks/2026-09-14-validation/source.tar.gz) | Exact code and lockfile captured for the validation. |
| [Exploration results](https://github.com/andreashdez/n_queens_problem_rust/blob/main/benchmarks/2026-09-14-n18/summary.csv) | Earlier experiments using seeds 1–10. |
| [Criterion output](https://github.com/andreashdez/n_queens_problem_rust/blob/main/benchmarks/2026-09-14-criterion.txt) | Phase estimates and reported outliers. |

## Profiling

```bash
cargo run --release --locked -- \
  --size 18 \
  --population 4000 \
  --seed 42 \
  --local-search-rate 0.05 \
  --profile \
  --json
cargo bench --bench ga --features bench-internals -- phases
```

Profiling is opt-in and does not change RNG consumption. `phase_timings` is `null` when disabled. Timings are cumulative wall-clock nanoseconds for crossover (including parent selection), mutation, local search, survivor selection, fitness, population metrics (including the uniqueness HashSet), diversity refresh, and restarts. These phases do not overlap. Solver timing excludes population construction, and phase totals exclude bookkeeping, snapshot creation, and callbacks.

The `phases` Criterion group measures crossover, mutation, local search, and population/diversity metrics independently at populations of 1,000 and 40,000. Each sample starts with a fresh seeded population; construction and initial fitness calculation are outside the measured section. Local search uses rate 0.05 with eight attempts. `bench-internals` exposes unstable benchmark hooks and is unnecessary for normal builds.

[Run your own parameter sweep →](../tuning/#parameter-sweeps)
