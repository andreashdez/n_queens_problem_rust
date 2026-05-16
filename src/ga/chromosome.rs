use std::{error::Error, fmt, sync::OnceLock};

use rand::{Rng, RngExt, seq::SliceRandom};

pub struct Chromosome {
    positions: Vec<u16>,
    conflicts: OnceLock<Vec<u32>>,
    conflicts_sum: u32,
    fitness: f32,
}

impl Clone for Chromosome {
    fn clone(&self) -> Self {
        let conflicts = OnceLock::new();
        if let Some(cached_conflicts) = self.conflicts.get() {
            let _ = conflicts.set(cached_conflicts.clone());
        }

        Self {
            positions: self.positions.clone(),
            conflicts,
            conflicts_sum: self.conflicts_sum,
            fitness: self.fitness,
        }
    }
}

impl fmt::Debug for Chromosome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Chromosome")
            .field("positions", &self.positions)
            .field("conflicts", &self.get_conflicts())
            .field("conflicts_sum", &self.conflicts_sum)
            .field("fitness", &self.fitness)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromosomeError {
    BoardSizeZero,
    BoardSizeTooLarge,
    PositionOutOfBounds,
    DuplicatePosition,
}

impl fmt::Display for ChromosomeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BoardSizeZero => formatter.write_str("board size must be greater than 0"),
            Self::BoardSizeTooLarge => formatter.write_str("board size exceeds u16 position range"),
            Self::PositionOutOfBounds => {
                formatter.write_str("chromosome position is outside the board")
            }
            Self::DuplicatePosition => formatter.write_str("chromosome positions must be unique"),
        }
    }
}

impl Error for ChromosomeError {}

impl Chromosome {
    pub fn new(positions: Vec<u16>) -> Self {
        Self::try_new(positions).expect("chromosome positions must be a valid permutation")
    }

    pub fn try_new(positions: Vec<u16>) -> Result<Self, ChromosomeError> {
        validate_positions(&positions)?;
        Ok(Self::new_unchecked(positions))
    }

    fn new_unchecked(positions: Vec<u16>) -> Self {
        let conflicts_sum = count_conflicts_sum(&positions);
        log::debug!("chromosome conflicts sum: {conflicts_sum}");
        Self {
            positions,
            conflicts: OnceLock::new(),
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

        let previous_queen_conflicts =
            count_swapped_queen_conflicts_from_positions(&self.positions, index_one, index_two);
        self.positions.swap(index_one, index_two);
        self.recalculate_conflicts_after_swap(index_one, index_two, previous_queen_conflicts);
    }

    pub fn get_positions(&self) -> &[u16] {
        &self.positions
    }

    pub fn get_conflicts(&self) -> &[u32] {
        self.conflicts
            .get_or_init(|| count_conflicts(&self.positions))
            .as_slice()
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

    fn recalculate_conflicts_after_swap(
        &mut self,
        index_one: usize,
        index_two: usize,
        previous_queen_conflicts: u32,
    ) {
        let current_queen_conflicts =
            count_swapped_queen_conflicts_from_positions(&self.positions, index_one, index_two);
        let updated_conflicts_sum = i64::from(self.conflicts_sum)
            + i64::from(current_queen_conflicts)
            - i64::from(previous_queen_conflicts);

        self.conflicts_sum =
            u32::try_from(updated_conflicts_sum).expect("conflicts sum should remain non-negative");
        self.conflicts = OnceLock::new();
        self.fitness = 0.0;
    }
}

fn validate_positions(positions: &[u16]) -> Result<(), ChromosomeError> {
    let size = positions.len();
    if size == 0 {
        return Err(ChromosomeError::BoardSizeZero);
    }

    if size > usize::from(u16::MAX) + 1 {
        return Err(ChromosomeError::BoardSizeTooLarge);
    }

    let mut seen = vec![false; size];
    for &position in positions {
        let position = usize::from(position);
        if position >= size {
            return Err(ChromosomeError::PositionOutOfBounds);
        }
        if seen[position] {
            return Err(ChromosomeError::DuplicatePosition);
        }
        seen[position] = true;
    }

    Ok(())
}

pub fn generate_distinct_random_values(size: u16) -> Vec<u16> {
    assert!(size > 0, "board size must be greater than 0");
    let mut rng = rand::rng();
    generate_distinct_random_values_with_rng(size, &mut rng)
}

pub fn generate_distinct_random_values_with_rng(size: u16, rng: &mut impl Rng) -> Vec<u16> {
    assert!(size > 0, "board size must be greater than 0");
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

fn count_conflicts_sum(positions: &[u16]) -> u32 {
    let size = positions.len();
    if size < 2 {
        return 0;
    }

    if positions.iter().any(|&y| usize::from(y) >= size) {
        log::debug!(
            "found out-of-bounds queen positions for board size {size}; using pairwise conflict counting"
        );
        return count_conflicts_sum_pairwise(positions);
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

    descending_diagonals
        .into_iter()
        .chain(ascending_diagonals)
        .map(conflicting_pair_count)
        .sum()
}

fn conflicting_pair_count(count: u32) -> u32 {
    count.saturating_sub(1) * count / 2
}

fn count_swapped_queen_conflicts_from_positions(
    positions: &[u16],
    index_one: usize,
    index_two: usize,
) -> u32 {
    count_swapped_queen_conflicts(
        positions,
        index_one,
        index_two,
        positions[index_one],
        positions[index_two],
    )
}

fn count_swapped_queen_conflicts(
    positions: &[u16],
    index_one: usize,
    index_two: usize,
    position_one: u16,
    position_two: u16,
) -> u32 {
    positions
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != index_one && *index != index_two)
        .map(|(index, &position)| {
            u32::from(queens_conflict(index_one, position_one, index, position))
                + u32::from(queens_conflict(index_two, position_two, index, position))
        })
        .sum()
}

fn queens_conflict(x_one: usize, y_one: u16, x_two: usize, y_two: u16) -> bool {
    x_one.abs_diff(x_two) == usize::from(y_one.abs_diff(y_two))
}

fn count_conflicts_pairwise(positions: &[u16]) -> Vec<u32> {
    let size = positions.len();
    let mut conflicts = vec![0; size];
    if size < 2 {
        return conflicts;
    }

    for x_two in 0..size - 1 {
        for x_one in x_two + 1..size {
            let y_one = positions[x_one];
            let y_two = positions[x_two];
            if queens_conflict(x_one, y_one, x_two, y_two) {
                log::trace!("found conflict: ({x_one},{y_one}) -> ({x_two},{y_two})");
                conflicts[x_one] += 1;
                conflicts[x_two] += 1;
            }
        }
    }

    conflicts
}

fn count_conflicts_sum_pairwise(positions: &[u16]) -> u32 {
    let size = positions.len();
    if size < 2 {
        return 0;
    }

    let mut conflicts_sum = 0;
    for x_two in 0..size - 1 {
        for x_one in x_two + 1..size {
            let y_one = positions[x_one];
            let y_two = positions[x_two];
            if queens_conflict(x_one, y_one, x_two, y_two) {
                conflicts_sum += 1;
            }
        }
    }

    conflicts_sum
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rand::{SeedableRng, rngs::StdRng};

    use crate::ga::chromosome::{Chromosome, ChromosomeError, generate_distinct_random_values};

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
        let single_queen = Chromosome::new(vec![0]);
        assert_eq!(single_queen.get_conflicts_sum(), 0);
    }

    #[test]
    fn test_mutate_swap_updates_conflict_sum_and_lazy_conflicts() {
        let mut chromosome = Chromosome::new(vec![0, 1, 2, 3, 4, 5, 6, 7]);
        let _ = chromosome.get_conflicts();

        chromosome.mutate_swap_at(0, 4);

        let conflicts = chromosome.get_conflicts();
        let conflicts_sum = conflicts.iter().sum::<u32>() / 2;
        assert_eq!(chromosome.get_conflicts_sum(), conflicts_sum);
        assert_eq!(
            conflicts,
            super::count_conflicts(chromosome.get_positions())
        );
    }

    #[test]
    fn test_try_new_accepts_valid_permutation() {
        let chromosome = Chromosome::try_new(vec![1, 3, 0, 2])
            .expect("valid permutation should construct a chromosome");

        assert_eq!(chromosome.get_positions(), &[1, 3, 0, 2]);
    }

    #[test]
    fn test_try_new_rejects_invalid_positions() {
        assert_eq!(
            Chromosome::try_new(vec![]).unwrap_err(),
            ChromosomeError::BoardSizeZero
        );
        assert_eq!(
            Chromosome::try_new(vec![0, 0]).unwrap_err(),
            ChromosomeError::DuplicatePosition
        );
        assert_eq!(
            Chromosome::try_new(vec![0, 2]).unwrap_err(),
            ChromosomeError::PositionOutOfBounds
        );
        assert_eq!(
            Chromosome::try_new(vec![0; usize::from(u16::MAX) + 2]).unwrap_err(),
            ChromosomeError::BoardSizeTooLarge
        );
    }

    #[test]
    #[should_panic(expected = "chromosome positions must be a valid permutation")]
    fn test_new_rejects_invalid_positions() {
        Chromosome::new(vec![0, 0]);
    }

    #[test]
    #[should_panic(expected = "board size must be greater than 0")]
    fn test_initial_values_generator_rejects_zero_size() {
        generate_distinct_random_values(0);
    }

    #[test]
    #[should_panic(expected = "board size must be greater than 0")]
    fn test_seeded_initial_values_generator_rejects_zero_size() {
        let mut seeded_rng = StdRng::seed_from_u64(1234);
        super::generate_distinct_random_values_with_rng(0, &mut seeded_rng);
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

                let optimized_conflicts_sum = super::count_conflicts_sum(&positions);
                let pairwise_conflicts_sum = pairwise_conflicts.iter().sum::<u32>() / 2;
                assert_eq!(optimized_conflicts_sum, pairwise_conflicts_sum);
            }
        }
    }

    #[test]
    fn test_count_conflicts_with_out_of_bounds_positions() {
        let positions = vec![8, 1, 3, 0];
        let optimized_conflicts = super::count_conflicts(&positions);
        let pairwise_conflicts = super::count_conflicts_pairwise(&positions);
        assert_eq!(optimized_conflicts, pairwise_conflicts);
        assert_eq!(
            super::count_conflicts_sum(&positions),
            pairwise_conflicts.iter().sum::<u32>() / 2
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn prop_mutate_swap_keeps_permutation_invariant(
            size in 1usize..64,
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
            prop_assert_eq!(
                chromosome.get_conflicts_sum(),
                chromosome.get_conflicts().iter().sum::<u32>() / 2,
            );
        }
    }
}
