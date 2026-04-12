# N-Queens Problem

A Rust implementation of the N-Queens problem using a genetic algorithm.

## Quickstart

```bash
cargo run --release
```

Run with explicit parameters:

```bash
cargo run --release -- --size 18 --population 40000 --epochs 5000 --seed 42 --mutation-rate 0.08 --elite-ratio 0.10
```

Short aliases are also available:

```bash
cargo run --release -- -n 18 -p 40000 -e 5000 -s 42 -m 0.08 -r 0.10
```

## CLI options

- `-n`, `--size <size>`: board size (number of queens). Default: `18`.
- `-p`, `--population <count>`: initial and target population size. Default: `40000`.
- `-e`, `--epochs <count>`: maximum GA epochs. Default: `5000`.
- `-s`, `--seed <u64>`: optional deterministic RNG seed.
- `-m`, `--mutation-rate <0..1>`: probability of mutating each non-elite chromosome. Default: `0.08`.
- `-r`, `--elite-ratio <0..1>`: fraction of top chromosomes retained before random survivor sampling. Default: `0.10`.
- `--no-board`: skip board rendering output.
- `--metrics-csv <path>`: write per-epoch run metrics to a CSV file (includes seed, elapsed ms, and best conflicts).

If `--seed` is omitted, a random seed is generated and logged.

## Library usage

```rust
use n_queens_problem::ga::{self, GaConfig};

fn main() {
    let config = GaConfig::new(18, 40_000, 5_000, 42)
        .with_mutation_rate(0.08)
        .with_elite_ratio(0.10);

    let mut algorithm = ga::build_genetic_algorithm(config);
    let metrics = algorithm.run_algorithm();

    println!(
        "best conflicts: {}",
        algorithm.get_best_chromosome().get_conflicts_sum()
    );
    println!("solved epoch: {:?}", metrics.solved_epoch());
}
```

## GitHub Wiki sync

`docs/index.md` is synced to GitHub Wiki `Home.md` by `.github/workflows/wiki-sync.yml`.

- Enable Wiki in the repository settings (`Settings -> General -> Features -> Wikis`).
- Push changes to `docs/index.md` on `main` or run the workflow manually.

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
