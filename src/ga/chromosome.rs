use rand::{Rng, RngExt, seq::SliceRandom};

#[derive(Debug, Clone)]
pub struct Chromosome {
    positions: Vec<u16>,
    conflicts: Vec<u32>,
    conflicts_sum: u32,
    fitness: f32,
}

impl Chromosome {
    pub fn new(positions: Vec<u16>) -> Self {
        let conflicts = count_conflicts(&positions);
        let conflicts_sum = conflicts.iter().sum::<u32>() / 2;
        log::debug!("chromosome conflicts sum: {conflicts_sum}");
        Self {
            positions,
            conflicts,
            conflicts_sum,
            fitness: 0.0,
        }
    }

    pub fn mutate_swap(&mut self, rng: &mut impl Rng) {
        if self.positions.len() < 2 {
            return;
        }

        let index_one = rng.random_range(0..self.positions.len());
        let mut index_two = rng.random_range(0..(self.positions.len() - 1));
        if index_two >= index_one {
            index_two += 1;
        }

        self.mutate_swap_at(index_one, index_two);
    }

    pub(crate) fn mutate_swap_at(&mut self, index_one: usize, index_two: usize) {
        if self.positions.len() < 2 || index_one == index_two {
            return;
        }

        if index_one >= self.positions.len() || index_two >= self.positions.len() {
            return;
        }

        self.positions.swap(index_one, index_two);
        self.recalculate_conflicts();
    }

    pub fn get_positions(&self) -> &[u16] {
        &self.positions
    }

    pub fn get_conflicts(&self) -> &[u32] {
        &self.conflicts
    }

    pub fn get_conflicts_sum(&self) -> u32 {
        self.conflicts_sum
    }

    pub fn get_fitness(&self) -> f32 {
        self.fitness
    }

    pub fn set_fitness(&mut self, fitness: f32) {
        self.fitness = fitness;
    }

    fn recalculate_conflicts(&mut self) {
        self.conflicts = count_conflicts(&self.positions);
        self.conflicts_sum = self.conflicts.iter().sum::<u32>() / 2;
        self.fitness = 0.0;
    }
}

pub fn generate_distinct_random_values(size: u16) -> Vec<u16> {
    let mut rng = rand::rng();
    generate_distinct_random_values_with_rng(size, &mut rng)
}

pub fn generate_distinct_random_values_with_rng(size: u16, rng: &mut impl Rng) -> Vec<u16> {
    let mut values = (0..size).collect::<Vec<_>>();
    values.shuffle(rng);
    values
}

fn count_conflicts(positions: &[u16]) -> Vec<u32> {
    let size = positions.len();
    let mut conflicts = vec![0; size];
    if size < 2 {
        return conflicts;
    }

    if positions.iter().any(|&y| usize::from(y) >= size) {
        log::debug!(
            "found out-of-bounds queen positions for board size {size}; using pairwise conflict counting"
        );
        return count_conflicts_pairwise(positions);
    }

    let diagonal_span = size * 2 - 1;
    let diagonal_offset = size - 1;
    let mut descending_diagonals = vec![0u32; diagonal_span];
    let mut ascending_diagonals = vec![0u32; diagonal_span];

    for (x, &y) in positions.iter().enumerate() {
        let y = usize::from(y);
        let descending_diagonal = x + diagonal_offset - y;
        let ascending_diagonal = x + y;
        descending_diagonals[descending_diagonal] += 1;
        ascending_diagonals[ascending_diagonal] += 1;
    }

    for (x, &y) in positions.iter().enumerate() {
        let y = usize::from(y);
        let descending_diagonal = x + diagonal_offset - y;
        let ascending_diagonal = x + y;
        let descending_conflicts = descending_diagonals[descending_diagonal].saturating_sub(1);
        let ascending_conflicts = ascending_diagonals[ascending_diagonal].saturating_sub(1);
        conflicts[x] = descending_conflicts + ascending_conflicts;
    }

    conflicts
}

fn count_conflicts_pairwise(positions: &[u16]) -> Vec<u32> {
    let size = positions.len();
    let mut conflicts = vec![0; size];
    if size < 2 {
        return conflicts;
    }

    for x_two in 0..size - 1 {
        for x_one in x_two + 1..size {
            let distance = x_one - x_two;
            let y_one = positions[x_one];
            let y_two = positions[x_two];
            if usize::from(y_one.abs_diff(y_two)) == distance {
                log::trace!("found conflict: ({x_one},{y_one}) -> ({x_two},{y_two})");
                conflicts[x_one] += 1;
                conflicts[x_two] += 1;
            }
        }
    }

    conflicts
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rand::{SeedableRng, rngs::StdRng};

    use crate::ga::chromosome::{Chromosome, generate_distinct_random_values};

    #[test]
    fn test_initial_values_generator() {
        let result = generate_distinct_random_values(8);
        assert!(result.contains(&0));
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
        assert!(result.contains(&4));
        assert!(result.contains(&5));
        assert!(result.contains(&6));
        assert!(result.contains(&7));
    }

    #[test]
    fn test_conflicts_counter() {
        let positions = vec![0, 2, 4, 6, 1, 3, 5, 7];
        let chromosome = Chromosome::new(positions);
        let conflicts_sum = chromosome.get_conflicts_sum();
        assert_eq!(conflicts_sum, 1);
        let positions = vec![2, 4, 1, 7, 5, 0, 6, 3];
        let chromosome = Chromosome::new(positions);
        let conflicts_sum = chromosome.get_conflicts_sum();
        assert_eq!(conflicts_sum, 2);
    }

    #[test]
    fn test_conflicts_counter_small_boards() {
        let empty_board = Chromosome::new(vec![]);
        assert_eq!(empty_board.get_conflicts_sum(), 0);

        let single_queen = Chromosome::new(vec![0]);
        assert_eq!(single_queen.get_conflicts_sum(), 0);
    }

    #[test]
    fn test_initial_values_generator_zero_size() {
        let mut seeded_rng = StdRng::seed_from_u64(1234);
        let values = super::generate_distinct_random_values_with_rng(0, &mut seeded_rng);
        assert!(values.is_empty());

        let values_with_thread_rng = generate_distinct_random_values(0);
        assert!(values_with_thread_rng.is_empty());
    }

    #[test]
    fn test_count_conflicts_matches_pairwise_counter() {
        let mut seeded_rng = StdRng::seed_from_u64(2026);

        for size in 2u16..=64 {
            for _ in 0..50 {
                let positions =
                    super::generate_distinct_random_values_with_rng(size, &mut seeded_rng);
                let optimized_conflicts = super::count_conflicts(&positions);
                let pairwise_conflicts = super::count_conflicts_pairwise(&positions);
                assert_eq!(optimized_conflicts, pairwise_conflicts);
            }
        }
    }

    #[test]
    fn test_count_conflicts_with_out_of_bounds_positions() {
        let positions = vec![8, 1, 3, 0];
        let optimized_conflicts = super::count_conflicts(&positions);
        let pairwise_conflicts = super::count_conflicts_pairwise(&positions);
        assert_eq!(optimized_conflicts, pairwise_conflicts);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn prop_mutate_swap_keeps_permutation_invariant(
            size in 0usize..64,
            initial_seed in any::<u64>(),
            mutation_seed in any::<u64>(),
        ) {
            let size_u16 = u16::try_from(size).expect("size should fit into u16");
            let mut initial_rng = StdRng::seed_from_u64(initial_seed);
            let positions =
                super::generate_distinct_random_values_with_rng(size_u16, &mut initial_rng);

            let mut chromosome = Chromosome::new(positions);
            let mut mutation_rng = StdRng::seed_from_u64(mutation_seed);
            chromosome.mutate_swap(&mut mutation_rng);

            let mut mutated_positions = chromosome.get_positions().to_vec();
            mutated_positions.sort_unstable();
            let expected_positions = (0..size_u16).collect::<Vec<_>>();

            prop_assert_eq!(mutated_positions, expected_positions);
        }
    }
}
