---
title: N-Queens Problem
description: A Rust implementation of the N-Queens problem using a genetic algorithm.
---

A Rust implementation of the N-Queens problem using a genetic algorithm.

## Quickstart

```bash
cargo run --release
```

Run with explicit parameters:

```bash
cargo run --release -- --size 18 --population 40000 --epochs 5000 --seed 42 --mutation-rate 0.08 --elite-ratio 0.10 --offspring-ratio 0.10
```

Short aliases are also available:

```bash
cargo run --release -- -n 18 -p 40000 -e 5000 -s 42 -m 0.08 -r 0.10 -o 0.10
```

## CLI options

- `-n`, `--size <size>`: board size (number of queens). Default: `18`.
- `-p`, `--population <count>`: initial and target population size. Default: `40000`.
- `-e`, `--epochs <count>`: maximum GA epochs. Default: `5000`.
- `-s`, `--seed <u64>`: optional deterministic RNG seed.
- `-m`, `--mutation-rate <0..1>`: probability of mutating each non-elite chromosome. Default: `0.08`.
- `-r`, `--elite-ratio <0..1>`: fraction of top chromosomes retained before random survivor sampling. Default: `0.10`.
- `-o`, `--offspring-ratio <0..1>`: fraction of the target population produced as offspring each epoch. Default: `0.10`.
- `--min-diversity-ratio <0..1>`: minimum unique-chromosome ratio before non-elites are randomly refreshed. Default: `0.10`.
- `--no-board`: skip board rendering output.
- `--metrics-csv <path>`: write per-epoch run metrics to a CSV file (includes best/average conflicts, unique chromosomes, adaptive rates, offspring count, stagnation, and elapsed ms).
- `--json`: print a machine-readable JSON summary. This suppresses logs and board rendering so stdout remains valid JSON.
- `--log-level <level>`: log level (`off`, `error`, `warn`, `info`, `debug`, or `trace`). Default: `info`.
- `--quiet`: suppress log output.

If `--seed` is omitted, a random seed is generated and logged.

Tune `--offspring-ratio` to control GA turnover. For example, `0.10` creates offspring equal to 10% of the target population before survivor selection.
Tune `--min-diversity-ratio` to control diversity recovery. When unique chromosome diversity drops below the threshold, the solver refreshes non-elite chromosomes with random permutations.

## Parameter sweeps

Run multiple seeds per configuration and compare solve rate, median solved epoch, and runtime:

```bash
cargo run --release --example parameter_sweep -- --sizes 18 --populations 40000 --epochs 5000 --seeds 20 --mutation-rates 0.06,0.08 --elite-ratios 0.05,0.10 --offspring-ratios 0.05,0.10 --min-diversity-ratios 0.05,0.10
```

The sweep prints CSV rows with one aggregate result per parameter combination.

## Library configuration

Use `GaConfig::validated()` or `GaConfig::try_new()` to get explicit errors for invalid public configuration values.

## Development checks

```bash
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test
cargo bench --bench ga
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
