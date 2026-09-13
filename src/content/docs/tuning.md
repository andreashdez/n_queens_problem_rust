---
title: "Tuning the solver"
description: "Choose parameters and run reproducible experiments across multiple seeds."
---

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

Start with a small sweep: two local-search rates across three seeds, for six runs total.

```bash
cargo run --release --locked --example parameter_sweep -- \
  --sizes 18 --populations 4000 --epochs 200 --seeds 3 \
  --selection-strategies tournament --local-search-rates 0,0.05
```

<details>
<summary>Advanced: compare several parameter families</summary>

The following grid contains 128 configurations × 20 seeds = **2,560 runs**. Narrow the ranges first if you are exploring interactively.

Run multiple seeds per configuration and compare solve rate, median solved epoch, and runtime:

```bash
cargo run --release --example parameter_sweep -- \
  --sizes 18 \
  --populations 40000 \
  --epochs 5000 \
  --seeds 20 \
  --mutation-rates 0.06,0.08 \
  --elite-ratios 0.05,0.10 \
  --offspring-ratios 0.05,0.10 \
  --min-diversity-ratios 0.05,0.10 \
  --selection-strategies roulette,tournament \
  --tournament-sizes 3,5 \
  --local-search-rates 0,0.05 \
  --local-search-attempts 8
```

</details>

The sweep prints CSV rows with one aggregate result per parameter combination. Add `--output-dir PATH` to save an experiment to a **new** directory:

```bash
cargo run --release --locked --example parameter_sweep -- \
  --sizes 18 \
  --populations 4000,40000 \
  --epochs 200 \
  --seeds 20 \
  --selection-strategies roulette,tournament \
  --local-search-rates 0,0.05 \
  --output-dir sweep-results
```

- `runs.jsonl`: one flushed record per completed seed, with full GA configuration, stop reason, solved epoch, remaining conflicts, and elapsed nanoseconds/milliseconds.
- `summary.csv`: the same aggregates printed to stdout, written after each configuration.
- `metadata.json`: seed range, command arguments, Git revision/status, compiler, OS/architecture, available CPUs, actual Rayon thread count, build mode, and profiling setting.
- `source/` and `source.patch`: workspace Rust sources, Cargo manifest/lockfile, toolchain file, and tracked changes at experiment start. Use `cargo run` from the matching source tree so the binary matches this snapshot. Identical seeds alone do not promise identical results across dependency/toolchain changes.

The directory must not already exist and must be outside `src`, `examples`, `benches`, and `tests`. Use `--seed-start` for independent validation seeds and `--profile` to include phase timings in each seed record. Interrupted sweeps retain completed seed records even if the current configuration has no aggregate yet. Runtime aggregates include unsolved runs; median solved epoch includes only solved runs. Elapsed time includes population construction.

[Compare measured N=18 presets →](../benchmarks/) · [Look up CLI defaults →](../cli/#cli-options)
