---
title: "Benchmarks and profiling"
description: "Measured presets, reproducibility details, and phase-level profiling."
---

## Measured N=18 presets

A release-build comparison on macOS/aarch64 with eight Rayon threads used 200 epochs and independent validation seeds 101–120:

| Preset | Population | Selection | Local-search rate | Solved | Median runtime |
| --- | ---: | --- | ---: | ---: | ---: |
| Compact hybrid | 4,000 | Tournament | 0.05 | 20/20 | 41.5 ms |
| Roulette hybrid | 4,000 | Roulette | 0.05 | 20/20 | 69.0 ms |
| Pure GA | 40,000 | Tournament | 0 | 20/20 | 259.5 ms |
| Current default parameters | 40,000 | Roulette | 0 | 20/20 | 373.0 ms |

All other GA parameters use their defaults. These measurements support trying the compact hybrid for N=18; they do not guarantee success or generalize to other board sizes. The default population and 5,000-epoch budget remain unchanged. The GUI includes a **Recommended · 18×18** preset.

```bash
cargo run --release --locked -- \
  --size 18 \
  --population 4000 \
  --epochs 200 \
  --seed 42 \
  --selection tournament \
  --local-search-rate 0.05
```

The repository’s [benchmark report](https://github.com/andreashdez/n_queens_problem_rust/blob/main/benchmarks/README.md) contains the other preset commands, methodology, raw seed results, source archives, and phase measurements.

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
