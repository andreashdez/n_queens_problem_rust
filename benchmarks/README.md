# Measurements from 2026-09-14

These measurements cover 18×18 boards on this machine: macOS/aarch64, eight Rayon threads, Rust 1.98.1, release builds with the updated lockfile. They compare the current solver with a 200-epoch budget. They do not establish optimal settings for other board sizes or machines.

They replace the 2026-09-12 measurements, which described the solver before the epoch-loop optimizations below. Those per-seed results no longer reproduce and were removed; `git log` still has them.

## What changed since 2026-09-12

Six changes reduced per-epoch cost. Five leave results bit-identical; only the survivor-selection change alters what any given seed produces.

- **Crossover** builds PMX children with `Chromosome::new_unchecked`, skipping a permutation check the crossover already guarantees.
- **Crossover** reuses scratch buffers across children and builds the child as a plain `Vec<u16>`, cutting allocations per child from six to two.
- **Crossover** runs on the Rayon pool. Parent selection and the crossover-window draws stay serial and consume the master RNG in their original order; only the PMX construction, which needs no randomness, is parallel. Batches below 512 offspring stay serial, because splitting smaller batches costs more than it saves.
- **Local search** scores a candidate swap with `Chromosome::conflicts_sum_after_swap` and applies it only when it improves. Applying and reverting instead cost two extra conflict recounts on every rejected attempt, and most attempts are rejected.
- **Population metrics** hash the uniqueness set with an inlined FxHash-style `BuildHasher` instead of SipHash. Uniqueness stays exact, because `HashSet` still compares positions on collision.
- **Survivor selection** culls the surplus in place rather than moving every survivor into a fresh allocation. Keeping a uniform random subset of the non-elites is distributionally the same as dropping one, so elitism and the sampling distribution are unchanged — but the RNG draw sequence is not, so per-seed results differ from 2026-09-12.

Per-phase medians over 20 seeds at N=18 and population 40,000 fell from 4.644 to 2.472 ms/epoch, about 47%.

`epoch_loop/n16_p1200_e120` is the one benchmark where both builds exhaust the same 120 epochs without solving, so it needs no adjustment for a changed epoch count: **29.04 ms → 21.25 ms, about 27% faster**. Note that its population of 1,200 produces 120 offspring per epoch, below the parallel threshold, so it measures every change above *except* the parallel crossover. For that one, see `phases/crossover/40000` below.

Solve rates were checked separately over 460 seeds per configuration against commit `b1057a0`. At N=18 with population 40,000 and roulette selection, 458/460 → 460/460; at N=24 with population 2,000 and tournament selection, 280/460 → 287/460. Both differences are well inside sampling error, which is the expected result for a change that alters only the RNG realization. That comparison was run ad hoc during the optimization work; only the current solver's artifacts are committed here.

## Choosing the defaults

The shipped defaults changed on 2026-09-14, from a 40,000-member roulette population to a 500-member tournament population with higher turnover.

| Parameter | Before | After |
| --- | ---: | ---: |
| `--population` | 40,000 | 500 |
| `--selection` | roulette | tournament |
| `--mutation-rate` | 0.08 | 0.16 |
| `--offspring-ratio` | 0.10 | 0.50 |

Board size, epoch budget, elite ratio, minimum diversity, tournament size, and local-search settings are unchanged.

The choice came from five sweep stages covering about 3,200 runs at board sizes 8 to 100, with every stage using a fresh seed range. Both configurations were then compared head to head on seeds 4001–4050, which no tuning stage had used:

| Board | Classic solved | Classic median | New solved | New median | Ratio |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 50/50 | 6.0 ms | 50/50 | under 1 ms | — |
| 18 | 50/50 | 176.0 ms | 50/50 | 5.5 ms | 32x |
| 32 | 50/50 | 979.0 ms | 50/50 | 14.5 ms | 68x |
| 64 | 50/50 | 1,798.5 ms | 50/50 | 31.0 ms | 58x |
| 100 | 50/50 | 2,919.5 ms | 50/50 | 53.0 ms | 55x |

Solve rate is identical at every size, so this is a speed change rather than a reliability trade. Worst-case runtime improved by more than the median: at N=100 the slowest classic run took 3,814 ms against 135 ms for the new defaults.

Three choices were deliberately not the fastest measured option:

- **Population 500, not 250.** Population 250 was faster at most sizes but solved only 39 of 40 seeds at N=100, and its slowest run used 4,499 of the 5,000-epoch budget. Population 500 left far more headroom at 40/40.
- **Mutation 0.16, not 0.32.** The stagnation boost multiplies the base rate by three and clamps at 0.60, so a base of 0.32 saturates that ceiling immediately and the adaptive response stops varying. The two were within noise of each other on runtime.
- **Local search stays disabled.** It roughly halves solved epochs at N=18 but costs about 2x at N=32 and N=64, so it remains an opt-in flag rather than a default.

Roulette selection, the large population, and every previous default remain available as flags, and the GUI keeps them as its **Classic genetic algorithm · 18×18** preset.

- [New defaults](2026-09-14-defaults-new/summary.csv): 250 runs, five board sizes, seeds 4001–4050.
- [Classic defaults](2026-09-14-defaults-classic/summary.csv): the same 250 runs with the pre-2026-09-14 values.

Both sweeps pass every parameter explicitly, so they do not depend on which values happen to be compiled in as defaults.

## Measured presets

All settings use mutation 0.08, elite ratio 0.10, offspring ratio 0.10, minimum diversity 0.10, tournament size 3, and eight local-search attempts. The table uses the independent validation seeds 101–120; the earlier exploration used seeds 1–10. Median runtime includes population construction and all runs. Median solved epoch includes successful runs only.

| Configuration | Population | Selection | Local-search rate | Solved | Median solved epoch | Median runtime |
| --- | ---: | --- | ---: | ---: | ---: | ---: |
| Compact hybrid | 4,000 | Tournament | 0.05 | 20/20 | 60.0 | 35.5 ms |
| Roulette hybrid | 4,000 | Roulette | 0.05 | 20/20 | 73.5 | 44.0 ms |
| Pure GA | 40,000 | Tournament | 0 | 20/20 | 43.5 | 101.5 ms |
| Classic GA (defaults before 2026-09-14) | 40,000 | Roulette | 0 | 20/20 | 74.5 | 195.5 ms |
| Larger hybrid | 40,000 | Tournament | 0.05 | 20/20 | 20.0 | 81.0 ms |

None of these five configurations is the shipped default; see **Choosing the defaults** above. The compact hybrid is worth trying when a board stalls close to a solution at N=18. Twenty successful validation seeds are limited evidence, not a guarantee.

The GUI's **Local-search hybrid · 18×18** preset uses today's mutation and offspring defaults and a 5,000-epoch budget, so it differs from the archived compact-hybrid configuration. These measurements cap every configuration at 200 epochs; the small-population defaults measured above use a 5,000-epoch budget and are not directly comparable.

Do not read the runtime column as a direct measure of the optimizations. Every row solves on a different epoch than it did on 2026-09-12, because survivor selection changed the RNG draw sequence, so each runtime mixes the lower per-epoch cost with a different number of epochs. The 40,000-population rows also gained more than the 4,000-population rows, because only they clear the parallel-crossover threshold. Use the phase figures below for speed, and this table for configuration choice.

The commands below specify every GA parameter. Use the matching archived source tree for exact reproduction; running them in a newer checkout applies the same settings to that implementation.

```bash
# Compact hybrid
cargo run --release --locked -- \
  --size 18 --population 4000 --epochs 200 --seed 42 \
  --mutation-rate 0.08 --elite-ratio 0.10 --offspring-ratio 0.10 \
  --min-diversity-ratio 0.10 --selection tournament --tournament-size 3 \
  --local-search-rate 0.05 --local-search-attempts 8

# Roulette hybrid
cargo run --release --locked -- \
  --size 18 --population 4000 --epochs 200 --seed 42 \
  --mutation-rate 0.08 --elite-ratio 0.10 --offspring-ratio 0.10 \
  --min-diversity-ratio 0.10 --selection roulette --tournament-size 3 \
  --local-search-rate 0.05 --local-search-attempts 8

# Pure GA
cargo run --release --locked -- \
  --size 18 --population 40000 --epochs 200 --seed 42 \
  --mutation-rate 0.08 --elite-ratio 0.10 --offspring-ratio 0.10 \
  --min-diversity-ratio 0.10 --selection tournament --tournament-size 3 \
  --local-search-rate 0 --local-search-attempts 8
```

## Evidence and reproduction

- [Exploration aggregates](2026-09-14-n18/summary.csv): 120 runs across 12 configurations, seeds 1–10.
- [Validation aggregates](2026-09-14-validation/summary.csv): 160 runs across eight configurations, independent seeds 101–120.
- [Profiled aggregates](2026-09-14-profile/summary.csv): 20 additional runs with profiling enabled, seeds 1–10.

Each directory contains `runs.jsonl`, `metadata.json`, and `source.tar.gz`. The archive contains the exact workspace Rust sources, lockfile, toolchain, and tracked patch captured at the start of that experiment. Sources were captured while the optimization change was uncommitted, so `metadata.json` records commit `b1057a0` with a dirty tree and the patch carries the actual solver under test. `metadata.json` records `uname -srvm` rather than `uname -a`, so the committed environment description omits the hostname of the machine that ran the sweep. Archives were created after the runs to keep the committed evidence compact; the sweep itself writes an uncompressed `source/` directory and `source.patch`.

```bash
cargo run --release --locked --example parameter_sweep -- \
  --sizes 18 --populations 1000,4000,40000 --epochs 200 --seeds 10 \
  --mutation-rates 0.08 --elite-ratios 0.10 --offspring-ratios 0.10 \
  --min-diversity-ratios 0.10 \
  --selection-strategies roulette,tournament --tournament-sizes 3 \
  --local-search-rates 0,0.05 --local-search-attempts 8 \
  --output-dir exploration-new
cargo run --release --locked --example parameter_sweep -- \
  --sizes 18 --populations 4000,40000 --epochs 200 --seed-start 101 --seeds 20 \
  --mutation-rates 0.08 --elite-ratios 0.10 --offspring-ratios 0.10 \
  --min-diversity-ratios 0.10 \
  --selection-strategies roulette,tournament --tournament-sizes 3 \
  --local-search-rates 0,0.05 --local-search-attempts 8 \
  --output-dir validation-new
cargo run --release --locked --example parameter_sweep -- \
  --sizes 18 --populations 4000,40000 --epochs 200 --seeds 10 \
  --mutation-rates 0.08 --elite-ratios 0.10 --offspring-ratios 0.10 \
  --min-diversity-ratios 0.10 \
  --selection-strategies tournament --tournament-sizes 3 \
  --local-search-rates 0.05 --local-search-attempts 8 --profile \
  --output-dir profile-new
```

## Phase costs

For the profiled tournament/local-search runs, the aggregate fractions of measured phase time were:

| Phase | Population 4,000 | Population 40,000 |
| --- | ---: | ---: |
| Local search | 28.9% | 44.8% |
| Mutation | 23.5% | 15.5% |
| Crossover and parent selection | 19.7% | 13.8% |
| Fitness | 14.5% | 4.7% |
| Population metrics, including uniqueness | 7.1% | 13.7% |
| Survivor selection | 4.7% | 7.5% |
| Restart | 1.6% | 0.0% |

Diversity refresh rounded to 0.0% in these runs. The denominator excludes construction, callbacks, and other bookkeeping.

Read these shares carefully: the denominator shrank by roughly half, so a phase can hold a larger share while costing the same or less in absolute time. Mutation, fitness, and restart were never touched, and their absolute per-epoch cost is unchanged. Crossover is the clearest real move, from 20.2%/25.3% on 2026-09-12 to 19.7%/13.8%; the gap between the two columns is the parallel threshold, since 4,000 population yields 400 offspring and stays serial while 40,000 yields 4,000 and does not.

Local search is now the largest phase at population 40,000 and the first place to look next. At the shipped defaults, which disable local search, mutation is now the largest phase at roughly 30%, ahead of crossover at 27%.

[Criterion output](2026-09-14-criterion.txt) contains all conflict-count, epoch-loop, and phase benchmarks. The phase group's central estimates were:

| Phase | Population 1,000 | Population 40,000 |
| --- | ---: | ---: |
| Crossover | 24.9 µs | 647 µs |
| Mutation | 41.2 µs | 455 µs |
| Local search | 37.6 µs | 1.43 ms |
| Population/diversity metrics | 9.58 µs | 379 µs |

Against 2026-09-12 those were 44.1 µs/1.85 ms for crossover, 53.0 µs/2.07 ms for local search, and 21.4 µs/886 µs for population/diversity metrics. Mutation was 42.7 µs/484 µs and is unchanged within noise. `phases/crossover/40000` is the cleanest isolated measurement of the parallel crossover: it fell from 1.38 ms to 647 µs once that change landed, while `phases/crossover/1000` builds only 100 offspring, stays serial, and did not move. There is no survivor-selection benchmark in the `phases` group, so that change shows up only in `epoch_loop` and in the profiled fractions above.

The Criterion run used the same solver/harness source as the profile snapshot, with `bench-internals` enabled. These are short diagnostic measurements (10 samples, 0.2-second warmup, 0.5-second target measurement), with outliers noted in the raw output. Use longer measurements for performance decisions.

`epoch_loop/n12_p600_e80` fell from 4.63 ms to 1.28 ms, but that case solves before its 80-epoch cap — at epoch 31 on 2026-09-12 and epoch 12 now — so most of the difference is the shorter run, not a faster epoch. `epoch_loop/n16_p1200_e120` exhausts its budget in both builds and is the comparable case.

```bash
cargo bench --locked --bench ga --features bench-internals -- --warm-up-time 0.2 --measurement-time 0.5 --sample-size 10
```
