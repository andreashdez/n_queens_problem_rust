use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
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
                        ga::GaConfig::new(size, population, epochs, seed)
                            .with_mutation_rate(ga::DEFAULT_MUTATION_RATE)
                            .with_elite_ratio(ga::DEFAULT_ELITE_RATIO),
                    )
                    .expect("benchmark config should be valid")
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

#[cfg(feature = "bench-internals")]
fn benchmark_phases(c: &mut Criterion) {
    use ga::benchmarking;
    let mut group = c.benchmark_group("phases");
    group.sample_size(10);
    type Phase = fn(&mut ga::GeneticAlgorithm) -> usize;
    let phases: [(&str, Phase); 4] = [
        ("crossover", benchmarking::crossover),
        ("mutation", benchmarking::mutation),
        ("local_search", benchmarking::local_search),
        ("diversity_metrics", benchmarking::diversity_metrics),
    ];
    for population in [1_000, 40_000] {
        for (name, phase) in phases {
            group.bench_function(BenchmarkId::new(name, population), |b| {
                b.iter_batched_ref(
                    || {
                        let mut algorithm = ga::build_genetic_algorithm(
                            ga::GaConfig::new(18, population, 1, 42).with_local_search_rate(0.05),
                        )
                        .unwrap();
                        benchmarking::prepare(&mut algorithm);
                        algorithm
                    },
                    |algorithm| black_box(phase(algorithm)),
                    BatchSize::LargeInput,
                );
            });
        }
    }
    group.finish();
}

#[cfg(not(feature = "bench-internals"))]
fn benchmark_phases(_: &mut Criterion) {}

criterion_group!(
    benches,
    benchmark_conflict_count,
    benchmark_epoch_loop,
    benchmark_phases
);
criterion_main!(benches);
