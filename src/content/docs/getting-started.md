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

```bash
cargo run --release --locked -- \
  --size 8 --population 256 --epochs 250 --seed 42
```

The terminal reports progress, the best and worst boards, elapsed time, and a stop reason. `solved` means the best board has zero conflicts; `epoch_limit` means the run used its budget without finding a solution. A run can also solve in the initial population, at epoch zero.

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
