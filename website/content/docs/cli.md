---
title: "CLI reference"
description: "Command-line flags, defaults, output modes, and stop reasons."
---

## Quickstart

[Build the project first](../getting-started/#1-get-the-project), then run commands from its directory. For a small seeded experiment:

```bash
cargo run --release --locked -- \
  --size 8 --population 256 --epochs 250 --seed 42
```

Use `cargo run --release --locked -- --help` to see the CLI's current options. Arguments after the standalone `--` go to the solver rather than Cargo.

## CLI options

### Board and run budget

| Option | Alias | Default | Purpose |
| --- | --- | --- | --- |
| `--size` | `-n` | `18` | Board dimension, from 1 to 65535. |
| `--population` | `-p` | `500` | Positive initial and target population size. |
| `--epochs` | `-e` | `5000` | Positive maximum number of evolution epochs. |
| `--seed` | `-s` | Generated | Unsigned 64-bit seed for reproducible runs. |
| `--allow-unsolvable` | — | Off | Evolve sizes 2 and 3 for experiments instead of stopping at epoch zero. |

### Evolution and local search

All rates must be finite numbers in `0..=1`. These are base settings; mutation and elite rates can adapt during stagnation.

| Option | Alias | Default | Purpose |
| --- | --- | --- | --- |
| `--mutation-rate` | `-m` | `0.16` | Mutation probability for each non-elite chromosome. |
| `--elite-ratio` | `-r` | `0.10` | Fraction protected during survivor selection. |
| `--offspring-ratio` | `-o` | `0.50` | Offspring as a fraction of the target population. |
| `--min-diversity-ratio` | — | `0.10` | Minimum unique-board ratio before refreshing non-elites. |
| `--selection` | — | `tournament` | Parent selection: `roulette` or `tournament`. |
| `--tournament-size` | — | `3` | Positive candidate count per tournament. |
| `--local-search-rate` | — | `0` | Fraction of non-elites selected for improving swaps. Zero disables local search. |
| `--local-search-attempts` | — | `8` | Swap attempts per selected candidate. Zero performs no attempts. |

[Choose settings for a specific symptom →](../tuning/#tuning-guidance)

### Output and diagnostics

| Option | Alias | Default | Purpose |
| --- | --- | --- | --- |
| `--no-board` | — | Off | Skip the terminal board. |
| `--metrics-csv` | — | No file | Write every epoch's metrics to the supplied path. |
| `--json` | — | Off | Emit one JSON summary, suppressing logs and board output. |
| `--profile` | — | Off | Include cumulative solver phase timings in the summary. |
| `--log-level` | — | `info` | `off`, `error`, `warn`, `info`, `debug`, or `trace`. |
| `--quiet` | — | Off | Suppress logs; text summaries and board output still appear. |
| `--help` | `-h` | — | Print available options. |
| `--version` | `-V` | — | Print the program version. |

If `--seed` is omitted, the generated seed appears in logs and the JSON summary. Use an explicit seed when suppressing logs.

## Export every epoch

```bash
cargo run --release --locked -- \
  --size 18 --population 4000 --epochs 200 --seed 42 \
  --selection tournament --local-search-rate 0.05 \
  --metrics-csv run-42.csv --json
```

The CSV includes configuration, outcomes, elapsed time, best/average conflicts, diversity, adaptive rates, offspring, local-search improvements, and stagnation. Epoch zero is included. CLI CSV export **replaces an existing file** at the chosen path. The [GUI export](../gui/#export-a-run) keeps bounded samples and refuses to overwrite files.

## Read an unsolved outcome

A successful process exit does not necessarily mean a solution was found. For example, this deliberately impossible board stops without evolving:

<!-- docs-check: unsolvable -->
```bash
cargo run --release --locked -- \
  --size 3 --population 16 --epochs 10 --seed 42 --json
```

Selected fields from the JSON result:

<!-- docs-result: unsolvable -->
```json
{
  "board_size": 3,
  "solved_epoch": null,
  "stop_reason": "unsolvable"
}
```

## Outcomes and exit status

`metrics.stop_reason()` reports `StopReason::Solved`, `EpochLimit`, `Cancelled`, or `Unsolvable`. CLI JSON exposes these as `solved`, `epoch_limit`, `cancelled`, and `unsolvable`; CSV adds `stop_reason` and `allow_unsolvable` columns. Unsolved outcomes are valid run results and retain a successful CLI exit status. Invalid input and I/O failures still return an error status.

[Start your first run →](../getting-started/) · [Compare configurations →](../tuning/)
