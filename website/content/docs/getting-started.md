---
title: "Getting started"
description: "Build the project and run your first seeded N-Queens experiment."
---

Start with a small board, then choose the desktop interface or command line for your next experiment.

## 1. Get the project

You need Git and a Rust toolchain with Cargo. The repository's `rust-toolchain.toml` selects stable Rust and includes rustfmt and Clippy. Node.js is only needed when working on this documentation site.

```bash
git clone https://github.com/andreashdez/n_queens_problem_rust.git
cd n_queens_problem_rust
```

Run the following commands from that directory. The first build downloads and compiles dependencies.

## 2. Run a small board

<!-- docs-check: quickstart -->
```bash
cargo run --release --locked -- \
  --size 8 --population 256 --epochs 250 --seed 42 --json
```

The JSON summary contains these fields (other fields are omitted here):

<!-- docs-result: quickstart -->
```json
{
  "board_size": 8,
  "seed": 42,
  "best_chromosome": { "conflicts_sum": 0 },
  "solved_epoch": 0,
  "stop_reason": "solved"
}
```

This seed already places a solution in the initial population: **epoch zero is a valid solve**. Zero conflicts means no queens attack one another. With other settings, `epoch_limit` means the run used its budget without finding a solution. Runtime varies by machine and is not part of the expected result above.

Remove `--json` to see progress logs, the text summary, and a terminal board.

## 3. Choose your interface

### Explore visually

```bash
cargo run --release --locked --features gui --bin n_queens_gui
```

Choose **Quick demo · 8×8**, then **Run solver**. Inspect queens, hover over the charts, and compare completed runs. The GUI is a native desktop application; this documentation website does not run the solver in your browser.

[Follow the GUI guide →](../gui/)

### Run reproducible experiments

Use a fixed seed and keep your source revision and dependency versions when comparing runs. For an N=18 configuration measured in this repository:

```bash
cargo run --release --locked -- \
  --size 18 --population 4000 --epochs 200 --seed 42 \
  --selection tournament --local-search-rate 0.05
```

[Browse CLI options →](../cli/) · [Read the benchmark methodology →](../benchmarks/)

## What counts as a solution?

Place N queens on an N×N board without sharing a row, column, or diagonal. This solver represents each board as a permutation, so it only needs to remove diagonal conflicts. Sizes 2 and 3 have no solution and normally stop immediately.

[Understand the algorithm →](../algorithm/)

## If the first run does not start

| Symptom | Next step |
| --- | --- |
| `cargo` is not found | Install Rust using [rustup](https://rustup.rs/), then open a new terminal. |
| Cargo cannot find `Cargo.toml` | Change into `n_queens_problem_rust`, the cloned repository directory. |
| The first build is taking time | Cargo compiles dependencies on the first run. Later builds reuse them. |
| The GUI cannot open a window | Run it in a desktop session with a graphics backend, or use the CLI on a headless machine. |
| `Stop reason: epoch_limit` | The program ran successfully. Try the [tuning guide](../tuning/) or a different seed. |

## Example board output (8x8)

This is an example of the terminal format, not the exact board from the seed above. Each `00` marks a queen with no conflicts.

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
