---
title: "CLI reference"
description: "Command-line flags, defaults, output modes, and stop reasons."
---

## Quickstart

```bash
cargo run --release
```

Run with explicit parameters:

```bash
cargo run --release -- \
  --size 18 \
  --population 40000 \
  --epochs 5000 \
  --seed 42 \
  --mutation-rate 0.08 \
  --elite-ratio 0.10 \
  --offspring-ratio 0.10 \
  --selection tournament \
  --tournament-size 3 \
  --local-search-rate 0.05 \
  --local-search-attempts 8
```

Short aliases are also available:

```bash
cargo run --release -- -n 18 -p 40000 -e 5000 -s 42 -m 0.08 -r 0.10 -o 0.10
```

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

## Outcomes and exit status

`metrics.stop_reason()` reports `StopReason::Solved`, `EpochLimit`, `Cancelled`, or `Unsolvable`. CLI JSON exposes these as `solved`, `epoch_limit`, `cancelled`, and `unsolvable`; CSV adds `stop_reason` and `allow_unsolvable` columns. Unsolved outcomes are valid run results and retain a successful CLI exit status. Invalid input and I/O failures still return an error status.

[Start your first run →](../getting-started/) · [Compare configurations →](../tuning/)
