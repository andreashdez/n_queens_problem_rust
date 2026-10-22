//! A min-conflicts local search: a non-evolutionary baseline for the genetic algorithm.
//!
//! The board uses the same row permutation as the GA, so queens never share a
//! row or column and only diagonal conflicts remain. The search starts from a
//! greedy board that places most queens on free diagonals. Each pass then visits
//! every column, and an attacked queen swaps rows with the first of a few random
//! partners that lowers the number of attacking pairs. Diagonal occupancy counts
//! make every candidate swap O(1), so a solve takes roughly linear time in the
//! board size. A pass without any improving swap restarts from a new greedy board.
//!
//! ```
//! use n_queens_problem::{
//!     ga::StopReason,
//!     min_conflicts::{self, MinConflictsConfig},
//! };
//!
//! let result = min_conflicts::solve(MinConflictsConfig::new(1_000, 100, 42)).unwrap();
//! assert_eq!(result.stop_reason(), StopReason::Solved);
//! assert_eq!(result.best_chromosome().get_conflicts_sum(), 0);
//! ```

use std::{error::Error, fmt, time::Instant};

use rand::{RngExt, SeedableRng, rngs::StdRng, seq::SliceRandom};

use crate::ga::{StopReason, chromosome::Chromosome};

/// Random partners tried for each attacked queen before the pass moves on.
const PARTNER_TRIES: usize = 32;
/// Random unplaced rows tried per column while building the greedy start board.
const GREEDY_TRIES: usize = 16;

#[derive(Debug, Clone, Copy)]
pub struct MinConflictsConfig {
    pub size: u16,
    /// Repair passes allowed across all restarts; each pass visits every column once.
    pub max_passes: u32,
    pub seed: u64,
    /// Search boards of size 2 or 3 for experiments despite their known impossibility.
    pub allow_unsolvable: bool,
}

impl MinConflictsConfig {
    #[must_use]
    pub const fn new(size: u16, max_passes: u32, seed: u64) -> Self {
        Self {
            size,
            max_passes,
            seed,
            allow_unsolvable: false,
        }
    }

    #[must_use]
    pub const fn with_allow_unsolvable(mut self, allow_unsolvable: bool) -> Self {
        self.allow_unsolvable = allow_unsolvable;
        self
    }

    /// Checks the board size and pass budget.
    ///
    /// # Errors
    /// Returns [`MinConflictsConfigError`] if either is zero.
    pub const fn validate(&self) -> Result<(), MinConflictsConfigError> {
        if self.size == 0 {
            return Err(MinConflictsConfigError::BoardSizeZero);
        }

        if self.max_passes == 0 {
            return Err(MinConflictsConfigError::MaxPassesZero);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinConflictsConfigError {
    BoardSizeZero,
    MaxPassesZero,
}

impl fmt::Display for MinConflictsConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BoardSizeZero => formatter.write_str("board size must be greater than 0"),
            Self::MaxPassesZero => formatter.write_str("max pass count must be greater than 0"),
        }
    }
}

impl Error for MinConflictsConfigError {}

#[derive(Debug, Clone)]
pub struct MinConflictsResult {
    best_chromosome: Chromosome,
    stop_reason: StopReason,
    passes: u32,
    swaps: u64,
    restarts: u32,
    elapsed_ms: u128,
}

impl MinConflictsResult {
    /// The board with the fewest conflicts seen across all restarts.
    #[must_use]
    pub const fn best_chromosome(&self) -> &Chromosome {
        &self.best_chromosome
    }

    /// `Solved`, `EpochLimit` once the pass budget is spent, `Unsolvable`, or
    /// `Cancelled` when a progress callback asked to stop.
    #[must_use]
    pub const fn stop_reason(&self) -> StopReason {
        self.stop_reason
    }

    /// Repair passes run across all restarts. Zero when the greedy start board is solved.
    #[must_use]
    pub const fn passes(&self) -> u32 {
        self.passes
    }

    /// Improving swaps applied.
    #[must_use]
    pub const fn swaps(&self) -> u64 {
        self.swaps
    }

    /// Times a pass made no progress and the search restarted from a new greedy board.
    #[must_use]
    pub const fn restarts(&self) -> u32 {
        self.restarts
    }

    #[must_use]
    pub const fn elapsed_ms(&self) -> u128 {
        self.elapsed_ms
    }
}

/// The search state after one repair pass, or after the greedy start board for pass zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassMetrics {
    pass: u32,
    current_conflicts_sum: u32,
    best_conflicts_sum: u32,
    swaps: u64,
    restarts: u32,
    last_restart_pass: Option<u32>,
    elapsed_ms: u128,
}

impl PassMetrics {
    #[must_use]
    pub const fn pass(&self) -> u32 {
        self.pass
    }

    /// Attacking pairs on the board being repaired; after a restart, the new greedy board.
    #[must_use]
    pub const fn current_conflicts_sum(&self) -> u32 {
        self.current_conflicts_sum
    }

    /// Attacking pairs on the best board seen so far, across restarts.
    #[must_use]
    pub const fn best_conflicts_sum(&self) -> u32 {
        self.best_conflicts_sum
    }

    /// Improving swaps applied so far.
    #[must_use]
    pub const fn swaps(&self) -> u64 {
        self.swaps
    }

    /// Restarts completed so far.
    #[must_use]
    pub const fn restarts(&self) -> u32 {
        self.restarts
    }

    /// The pass that ended with the most recent restart.
    #[must_use]
    pub const fn last_restart_pass(&self) -> Option<u32> {
        self.last_restart_pass
    }

    #[must_use]
    pub const fn elapsed_ms(&self) -> u128 {
        self.elapsed_ms
    }
}

/// Progress passed to [`solve_with_progress`] callbacks.
#[derive(Debug, Clone)]
pub struct PassSnapshot {
    metrics: PassMetrics,
    best_chromosome: Chromosome,
}

impl PassSnapshot {
    #[must_use]
    pub const fn metrics(&self) -> &PassMetrics {
        &self.metrics
    }

    /// The best board seen so far, across restarts.
    #[must_use]
    pub const fn best_chromosome(&self) -> &Chromosome {
        &self.best_chromosome
    }
}

/// Searches for a solution with the configuration's seed.
///
/// Boards of size 2 and 3 stop immediately as [`StopReason::Unsolvable`] unless
/// `allow_unsolvable` is set.
///
/// # Errors
/// Returns [`MinConflictsConfigError`] if [`MinConflictsConfig::validate`] rejects the configuration.
pub fn solve(config: MinConflictsConfig) -> Result<MinConflictsResult, MinConflictsConfigError> {
    search(config, |_, _| true)
}

/// Like [`solve`], reporting pass zero and then every completed pass.
///
/// Returning false from `on_pass` cancels the search after that pass. A solution
/// takes precedence over cancellation. Progress reporting does not change the
/// random sequence, so a run matches [`solve`] for the same configuration.
///
/// # Errors
/// Returns [`MinConflictsConfigError`] if [`MinConflictsConfig::validate`] rejects the configuration.
pub fn solve_with_progress<F>(
    config: MinConflictsConfig,
    mut on_pass: F,
) -> Result<MinConflictsResult, MinConflictsConfigError>
where
    F: FnMut(&PassSnapshot) -> bool,
{
    search(config, |metrics, best_rows| {
        on_pass(&PassSnapshot {
            metrics,
            best_chromosome: Chromosome::new_unchecked(best_rows.to_vec()),
        })
    })
}

/// Counters and the best board, kept across restarts.
struct SearchState {
    started_at: Instant,
    passes: u32,
    swaps: u64,
    restarts: u32,
    last_restart_pass: Option<u32>,
    best_rows: Vec<u16>,
    best_attacking_pairs: u64,
}

impl SearchState {
    fn record_best(&mut self, board: &Board) {
        if board.attacking_pairs < self.best_attacking_pairs {
            self.best_rows.clone_from(&board.rows);
            self.best_attacking_pairs = board.attacking_pairs;
        }
    }

    fn metrics(&self, board: &Board) -> PassMetrics {
        PassMetrics {
            pass: self.passes,
            current_conflicts_sum: pairs_u32(board.attacking_pairs),
            best_conflicts_sum: pairs_u32(self.best_attacking_pairs),
            swaps: self.swaps,
            restarts: self.restarts,
            last_restart_pass: self.last_restart_pass,
            elapsed_ms: self.started_at.elapsed().as_millis(),
        }
    }
}

/// A `u16` board has at most 65,535 × 65,534 / 2 attacking pairs, which fits.
fn pairs_u32(attacking_pairs: u64) -> u32 {
    u32::try_from(attacking_pairs).expect("attacking pairs on a u16 board fit u32")
}

/// The search behind [`solve`] and [`solve_with_progress`]. `on_pass` receives
/// metrics and the best rows by reference, so [`solve`] builds no snapshots.
fn search(
    config: MinConflictsConfig,
    mut on_pass: impl FnMut(PassMetrics, &[u16]) -> bool,
) -> Result<MinConflictsResult, MinConflictsConfigError> {
    config.validate()?;

    let started_at = Instant::now();
    let mut rng = StdRng::seed_from_u64(config.seed);
    let mut board = Board::greedy(config.size, &mut rng);
    let mut state = SearchState {
        started_at,
        passes: 0,
        swaps: 0,
        restarts: 0,
        last_restart_pass: None,
        best_rows: board.rows.clone(),
        best_attacking_pairs: board.attacking_pairs,
    };
    log::info!(
        "running min-conflicts size={} max_passes={} initial_attacking_pairs={}",
        config.size,
        config.max_passes,
        board.attacking_pairs,
    );

    let stop_reason = 'search: {
        let keep_going = on_pass(state.metrics(&board), &state.best_rows);
        if board.attacking_pairs == 0 {
            break 'search StopReason::Solved;
        }
        if !keep_going {
            break 'search StopReason::Cancelled;
        }
        if !config.allow_unsolvable && matches!(config.size, 2 | 3) {
            log::warn!("board size has no solution; use allow_unsolvable for experiments");
            break 'search StopReason::Unsolvable;
        }

        loop {
            if state.passes == config.max_passes {
                break 'search StopReason::EpochLimit;
            }
            state.passes += 1;

            let pass_start_pairs = board.attacking_pairs;
            state.swaps += board.repair_pass(&mut rng);
            state.record_best(&board);
            if board.attacking_pairs != 0 && board.attacking_pairs == pass_start_pairs {
                state.restarts += 1;
                state.last_restart_pass = Some(state.passes);
                log::debug!(
                    "min-conflicts restart pass={} attacking_pairs={}",
                    state.passes,
                    board.attacking_pairs
                );
                board = Board::greedy(config.size, &mut rng);
                state.record_best(&board);
            }

            let keep_going = on_pass(state.metrics(&board), &state.best_rows);
            if board.attacking_pairs == 0 {
                break 'search StopReason::Solved;
            }
            if !keep_going {
                break 'search StopReason::Cancelled;
            }
        }
    };

    let SearchState {
        passes,
        swaps,
        restarts,
        best_rows,
        best_attacking_pairs,
        ..
    } = state;
    if stop_reason == StopReason::Solved {
        log::info!("min-conflicts solved passes={passes} swaps={swaps} restarts={restarts}");
    } else {
        log::warn!(
            "no solution found best_attacking_pairs={best_attacking_pairs} passes={passes} restarts={restarts} stop_reason={stop_reason}"
        );
    }

    let best_chromosome = Chromosome::new_unchecked(best_rows);
    debug_assert_eq!(
        u64::from(best_chromosome.get_conflicts_sum()),
        best_attacking_pairs
    );

    Ok(MinConflictsResult {
        best_chromosome,
        stop_reason,
        passes,
        swaps,
        restarts,
        elapsed_ms: started_at.elapsed().as_millis(),
    })
}

/// A row permutation with occupancy counts for both diagonal directions.
struct Board {
    rows: Vec<u16>,
    /// Queens on each `x + y` diagonal.
    ascending: Vec<u32>,
    /// Queens on each `x + (size - 1) - y` diagonal.
    descending: Vec<u32>,
    attacking_pairs: u64,
}

impl Board {
    /// Places columns left to right, preferring a random unplaced row whose
    /// diagonals are still empty. Rows stay a permutation throughout, and later
    /// columns fall back to any unplaced row once free ones become hard to find.
    fn greedy(size: u16, rng: &mut StdRng) -> Self {
        let mut rows = (0..size).collect::<Vec<_>>();
        rows.shuffle(rng);
        let diagonal_count = 2 * rows.len() - 1;
        let mut board = Self {
            rows,
            ascending: vec![0; diagonal_count],
            descending: vec![0; diagonal_count],
            attacking_pairs: 0,
        };

        let size = board.rows.len();
        for x in 0..size {
            for _ in 0..GREEDY_TRIES {
                let candidate = rng.random_range(x..size);
                if board.is_free(x, board.rows[candidate]) {
                    board.rows.swap(x, candidate);
                    break;
                }
            }
            board.place(x);
        }

        board
    }

    const fn diagonals(&self, x: usize, row: u16) -> (usize, usize) {
        let y = row as usize;
        (x + y, x + (self.rows.len() - 1) - y)
    }

    fn is_free(&self, x: usize, row: u16) -> bool {
        let (ascending, descending) = self.diagonals(x, row);
        self.ascending[ascending] == 0 && self.descending[descending] == 0
    }

    fn is_attacked(&self, x: usize) -> bool {
        let (ascending, descending) = self.diagonals(x, self.rows[x]);
        self.ascending[ascending] > 1 || self.descending[descending] > 1
    }

    /// Adds column `x`'s queen to the diagonal counts.
    fn place(&mut self, x: usize) {
        let (ascending, descending) = self.diagonals(x, self.rows[x]);
        self.attacking_pairs +=
            u64::from(self.ascending[ascending]) + u64::from(self.descending[descending]);
        self.ascending[ascending] += 1;
        self.descending[descending] += 1;
    }

    /// Removes column `x`'s queen from the diagonal counts.
    fn lift(&mut self, x: usize) {
        let (ascending, descending) = self.diagonals(x, self.rows[x]);
        self.ascending[ascending] -= 1;
        self.descending[descending] -= 1;
        self.attacking_pairs -=
            u64::from(self.ascending[ascending]) + u64::from(self.descending[descending]);
    }

    fn swap(&mut self, one: usize, two: usize) {
        if one == two {
            return;
        }
        self.lift(one);
        self.lift(two);
        self.rows.swap(one, two);
        self.place(one);
        self.place(two);
    }

    /// Visits every column once and returns the number of improving swaps applied.
    fn repair_pass(&mut self, rng: &mut StdRng) -> u64 {
        let size = self.rows.len();
        let mut swaps = 0;
        for x in 0..size {
            if !self.is_attacked(x) {
                continue;
            }

            for _ in 0..PARTNER_TRIES {
                let mut partner = rng.random_range(0..(size - 1));
                if partner >= x {
                    partner += 1;
                }

                let before = self.attacking_pairs;
                self.swap(x, partner);
                if self.attacking_pairs < before {
                    swaps += 1;
                    break;
                }
                self.swap(x, partner);
            }

            if self.attacking_pairs == 0 {
                break;
            }
        }

        swaps
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn assert_solution(result: &MinConflictsResult, size: u16) {
        let mut rows = result.best_chromosome().get_positions().to_vec();
        assert_eq!(result.stop_reason(), StopReason::Solved, "size {size}");
        assert_eq!(
            result.best_chromosome().get_conflicts_sum(),
            0,
            "size {size}"
        );
        rows.sort_unstable();
        assert_eq!(rows, (0..size).collect::<Vec<_>>(), "size {size}");
    }

    #[test]
    fn solves_every_small_solvable_size_across_seeds() {
        for size in (1..=64).filter(|size| !matches!(size, 2 | 3)) {
            for seed in 0..20 {
                let result = solve(MinConflictsConfig::new(size, 5_000, seed)).unwrap();
                assert_solution(&result, size);
            }
        }
    }

    #[test]
    fn solves_large_boards() {
        for size in [1_000, 20_000] {
            let result = solve(MinConflictsConfig::new(size, 100, 42)).unwrap();
            assert_solution(&result, size);
        }
    }

    #[test]
    fn impossible_sizes_stop_unless_allowed() {
        for size in [2, 3] {
            let result = solve(MinConflictsConfig::new(size, 10, 42)).unwrap();
            assert_eq!(result.stop_reason(), StopReason::Unsolvable);
            assert_eq!(result.passes(), 0);

            let config = MinConflictsConfig::new(size, 10, 42).with_allow_unsolvable(true);
            let result = solve(config).unwrap();
            assert_eq!(result.stop_reason(), StopReason::EpochLimit);
            assert_eq!(result.passes(), 10);
            assert!(result.best_chromosome().get_conflicts_sum() > 0);
        }
    }

    #[test]
    fn rejects_zero_size_and_budget() {
        assert_eq!(
            solve(MinConflictsConfig::new(0, 10, 42)).unwrap_err(),
            MinConflictsConfigError::BoardSizeZero
        );
        assert_eq!(
            solve(MinConflictsConfig::new(8, 0, 42)).unwrap_err(),
            MinConflictsConfigError::MaxPassesZero
        );
    }

    #[test]
    fn same_seed_gives_same_search() {
        let run = || solve(MinConflictsConfig::new(200, 100, 7)).unwrap();
        let (first, second) = (run(), run());
        assert_eq!(
            first.best_chromosome().get_positions(),
            second.best_chromosome().get_positions()
        );
        assert_eq!(
            (first.passes(), first.swaps(), first.restarts()),
            (second.passes(), second.swaps(), second.restarts())
        );
    }

    #[test]
    fn progress_reports_every_pass_and_matches_solve() {
        for (size, seed) in [(1, 42), (8, 3), (18, 42), (200, 7)] {
            let config = MinConflictsConfig::new(size, 5_000, seed);
            let mut seen = Vec::new();
            let result = solve_with_progress(config, |snapshot| {
                let metrics = *snapshot.metrics();
                assert_eq!(
                    snapshot.best_chromosome().get_conflicts_sum(),
                    metrics.best_conflicts_sum()
                );
                seen.push(metrics);
                true
            })
            .unwrap();
            let plain = solve(config).unwrap();
            assert_eq!(
                result.best_chromosome().get_positions(),
                plain.best_chromosome().get_positions()
            );
            assert_eq!(
                (result.passes(), result.swaps(), result.restarts()),
                (plain.passes(), plain.swaps(), plain.restarts())
            );
            let passes: Vec<_> = seen.iter().map(PassMetrics::pass).collect();
            assert_eq!(passes, (0..=result.passes()).collect::<Vec<_>>());
            let last = seen.last().unwrap();
            assert_eq!(last.best_conflicts_sum(), 0);
            assert_eq!(
                (last.swaps(), last.restarts()),
                (result.swaps(), result.restarts())
            );
            assert!(
                seen.windows(2)
                    .all(|pair| { pair[1].best_conflicts_sum() <= pair[0].best_conflicts_sum() })
            );
        }
    }

    #[test]
    fn progress_can_cancel_but_a_solution_takes_precedence() {
        let config = MinConflictsConfig::new(3, 100, 42).with_allow_unsolvable(true);
        for stop_after in [0, 2] {
            let result =
                solve_with_progress(config, |snapshot| snapshot.metrics().pass() < stop_after)
                    .unwrap();
            assert_eq!(result.stop_reason(), StopReason::Cancelled);
            assert_eq!(result.passes(), stop_after);
        }

        let result = solve_with_progress(MinConflictsConfig::new(1, 10, 42), |_| false).unwrap();
        assert_eq!(result.stop_reason(), StopReason::Solved);
    }

    #[test]
    fn progress_marks_restart_passes() {
        let config = MinConflictsConfig::new(3, 20, 42).with_allow_unsolvable(true);
        let mut last = None;
        let result = solve_with_progress(config, |snapshot| {
            let metrics = snapshot.metrics();
            if metrics.restarts() > last.map_or(0, |last: PassMetrics| last.restarts()) {
                assert_eq!(metrics.last_restart_pass(), Some(metrics.pass()));
            }
            last = Some(*metrics);
            true
        })
        .unwrap();
        assert!(result.restarts() > 0);
        assert_eq!(last.unwrap().restarts(), result.restarts());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn prop_diagonal_counts_track_attacking_pairs(
            size in 1u16..48,
            seed in any::<u64>(),
            swaps in proptest::collection::vec((any::<usize>(), any::<usize>()), 0..64),
        ) {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut board = Board::greedy(size, &mut rng);
            for (one, two) in swaps {
                let size = usize::from(size);
                board.swap(one % size, two % size);
                let chromosome = Chromosome::new(board.rows.clone());
                prop_assert_eq!(
                    u64::from(chromosome.get_conflicts_sum()),
                    board.attacking_pairs
                );
                for x in 0..size {
                    prop_assert_eq!(board.is_attacked(x), chromosome.get_conflicts()[x] > 0);
                }
            }
        }
    }
}
