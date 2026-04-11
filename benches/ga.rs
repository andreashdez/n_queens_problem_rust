use criterion::{BatchSize, BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use n_queens_problem::ga::{self, chromosome::Chromosome};
use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};

fn shuffled_values(size: u16, seed: u64) -> Vec<u16> {
    let mut values = (0..size).collect::<Vec<_>>();
    let mut rng = StdRng::seed_from_u64(seed);
    values.shuffle(&mut rng);
    values
}

fn benchmark_conflict_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("count_conflicts");

    for &size in &[8u16, 32, 64, 128] {
        let positions = shuffled_values(size, 1_000 + u64::from(size));
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &positions,
            |b, positions| {
                b.iter_batched(
                    || positions.clone(),
                    |positions| {
                        let chromosome = Chromosome::new(positions);
                        black_box(chromosome.get_conflicts_sum());
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn benchmark_epoch_loop(c: &mut Criterion) {
    let mut group = c.benchmark_group("epoch_loop");
    group.sample_size(10);

    let cases = [
        ("n12_p600_e80", 12u16, 600usize, 80u32, 42u64),
        ("n16_p1200_e120", 16u16, 1_200usize, 120u32, 1337u64),
    ];

    for (name, size, population, epochs, seed) in cases {
        group.bench_function(name, |b| {
            b.iter_batched(
                || {
                    ga::build_genetic_algorithm(
                        size,
                        population,
                        epochs,
                        seed,
                        ga::DEFAULT_MUTATION_RATE,
                        ga::DEFAULT_ELITE_RATIO,
                    )
                },
                |mut algorithm| {
                    algorithm.run_algorithm();
                    black_box(algorithm.get_best_chromosome().get_conflicts_sum());
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

criterion_group!(benches, benchmark_conflict_count, benchmark_epoch_loop);
criterion_main!(benches);
