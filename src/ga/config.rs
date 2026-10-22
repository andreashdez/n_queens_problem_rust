//! Solver configuration: shipped defaults, validation, and the selection strategy.

use std::{error::Error, fmt};

/// Default board size, population, and epoch budget.
///
/// These live here rather than in the binary so the CLI and the GUI cannot drift
/// apart; both read them from this module.
pub const DEFAULT_BOARD_SIZE: u16 = 18;
pub const DEFAULT_POPULATION_SIZE: usize = 500;
pub const DEFAULT_MAX_EPOCHS: u32 = 5_000;

/// A small population with high turnover.
///
/// Measured on 2026-09-14 across board sizes 8 to 100: 50 of 50 seeds solved at
/// every size, matching the previous 40,000/roulette defaults, while running
/// 32x to 67x faster. See `benchmarks/README.md` for the sweeps behind each
/// value.
///
/// `DEFAULT_MUTATION_RATE` is deliberately not the fastest value measured: the
/// stagnation boost multiplies it by
/// [`MUTATION_STAGNATION_BOOST_SCALE`](super::MUTATION_STAGNATION_BOOST_SCALE)
/// and clamps at [`MAX_ADAPTIVE_MUTATION_RATE`](super::MAX_ADAPTIVE_MUTATION_RATE),
/// so a higher base would saturate that ceiling and flatten the adaptive response.
pub const DEFAULT_MUTATION_RATE: f32 = 0.16;
pub const DEFAULT_ELITE_RATIO: f32 = 0.10;
pub const DEFAULT_OFFSPRING_RATIO: f32 = 0.50;
pub const DEFAULT_MIN_DIVERSITY_RATIO: f32 = 0.10;
pub const DEFAULT_SELECTION_STRATEGY: SelectionStrategy = SelectionStrategy::Tournament;
pub const DEFAULT_TOURNAMENT_SIZE: usize = 3;
pub const DEFAULT_LOCAL_SEARCH_RATE: f32 = 0.0;
pub const DEFAULT_LOCAL_SEARCH_ATTEMPTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionStrategy {
    Roulette,
    Tournament,
}

impl fmt::Display for SelectionStrategy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Roulette => formatter.write_str("roulette"),
            Self::Tournament => formatter.write_str("tournament"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GaConfig {
    pub size: u16,
    pub initial_population: usize,
    pub max_epoch_count: u32,
    pub seed: u64,
    pub mutation_rate: f32,
    pub elite_ratio: f32,
    pub offspring_ratio: f32,
    pub min_diversity_ratio: f32,
    pub selection_strategy: SelectionStrategy,
    pub tournament_size: usize,
    pub local_search_rate: f32,
    pub local_search_attempts: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaConfigError {
    BoardSizeZero,
    InitialPopulationZero,
    MaxEpochCountZero,
    InvalidMutationRate,
    InvalidEliteRatio,
    InvalidOffspringRatio,
    InvalidMinDiversityRatio,
    InvalidLocalSearchRate,
    TournamentSizeZero,
}

impl fmt::Display for GaConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BoardSizeZero => formatter.write_str("board size must be greater than 0"),
            Self::InitialPopulationZero => {
                formatter.write_str("initial population must be greater than 0")
            }
            Self::MaxEpochCountZero => {
                formatter.write_str("max epoch count must be greater than 0")
            }
            Self::InvalidMutationRate => {
                formatter.write_str("mutation rate must be finite and between 0.0 and 1.0")
            }
            Self::InvalidEliteRatio => {
                formatter.write_str("elite ratio must be finite and between 0.0 and 1.0")
            }
            Self::InvalidOffspringRatio => {
                formatter.write_str("offspring ratio must be finite and between 0.0 and 1.0")
            }
            Self::InvalidMinDiversityRatio => formatter
                .write_str("minimum diversity ratio must be finite and between 0.0 and 1.0"),
            Self::InvalidLocalSearchRate => {
                formatter.write_str("local search rate must be finite and between 0.0 and 1.0")
            }
            Self::TournamentSizeZero => {
                formatter.write_str("tournament size must be greater than 0")
            }
        }
    }
}

impl Error for GaConfigError {}

impl GaConfig {
    #[must_use]
    pub const fn new(
        size: u16,
        initial_population: usize,
        max_epoch_count: u32,
        seed: u64,
    ) -> Self {
        Self {
            size,
            initial_population,
            max_epoch_count,
            seed,
            mutation_rate: DEFAULT_MUTATION_RATE,
            elite_ratio: DEFAULT_ELITE_RATIO,
            offspring_ratio: DEFAULT_OFFSPRING_RATIO,
            min_diversity_ratio: DEFAULT_MIN_DIVERSITY_RATIO,
            selection_strategy: DEFAULT_SELECTION_STRATEGY,
            tournament_size: DEFAULT_TOURNAMENT_SIZE,
            local_search_rate: DEFAULT_LOCAL_SEARCH_RATE,
            local_search_attempts: DEFAULT_LOCAL_SEARCH_ATTEMPTS,
        }
    }

    /// Creates a configuration and validates its defaults and dimensions.
    ///
    /// # Errors
    /// Returns [`GaConfigError`] if the board size, population, or epoch budget is zero.
    pub fn try_new(
        size: u16,
        initial_population: usize,
        max_epoch_count: u32,
        seed: u64,
    ) -> Result<Self, GaConfigError> {
        Self::new(size, initial_population, max_epoch_count, seed).validated()
    }

    #[must_use]
    pub const fn with_mutation_rate(mut self, mutation_rate: f32) -> Self {
        self.mutation_rate = mutation_rate;
        self
    }

    #[must_use]
    pub const fn with_elite_ratio(mut self, elite_ratio: f32) -> Self {
        self.elite_ratio = elite_ratio;
        self
    }

    #[must_use]
    pub const fn with_offspring_ratio(mut self, offspring_ratio: f32) -> Self {
        self.offspring_ratio = offspring_ratio;
        self
    }

    #[must_use]
    pub const fn with_min_diversity_ratio(mut self, min_diversity_ratio: f32) -> Self {
        self.min_diversity_ratio = min_diversity_ratio;
        self
    }

    #[must_use]
    pub const fn with_selection_strategy(mut self, selection_strategy: SelectionStrategy) -> Self {
        self.selection_strategy = selection_strategy;
        self
    }

    #[must_use]
    pub const fn with_tournament_size(mut self, tournament_size: usize) -> Self {
        self.tournament_size = tournament_size;
        self
    }

    #[must_use]
    pub const fn with_local_search_rate(mut self, local_search_rate: f32) -> Self {
        self.local_search_rate = local_search_rate;
        self
    }

    #[must_use]
    pub const fn with_local_search_attempts(mut self, local_search_attempts: usize) -> Self {
        self.local_search_attempts = local_search_attempts;
        self
    }

    /// Validates and returns this configuration.
    ///
    /// # Errors
    /// Returns the same validation errors as [`Self::validate`].
    pub fn validated(self) -> Result<Self, GaConfigError> {
        self.validate()?;
        Ok(self)
    }

    /// Checks dimensions, budgets, rates, and selection settings.
    ///
    /// # Errors
    /// Returns [`GaConfigError`] for zero dimensions or budgets, non-finite rates,
    /// rates outside `0..=1`, or a zero tournament size.
    pub fn validate(&self) -> Result<(), GaConfigError> {
        if self.size == 0 {
            return Err(GaConfigError::BoardSizeZero);
        }

        if self.initial_population == 0 {
            return Err(GaConfigError::InitialPopulationZero);
        }

        if self.max_epoch_count == 0 {
            return Err(GaConfigError::MaxEpochCountZero);
        }

        if !is_unit_interval(self.mutation_rate) {
            return Err(GaConfigError::InvalidMutationRate);
        }

        if !is_unit_interval(self.elite_ratio) {
            return Err(GaConfigError::InvalidEliteRatio);
        }

        if !is_unit_interval(self.offspring_ratio) {
            return Err(GaConfigError::InvalidOffspringRatio);
        }

        if !is_unit_interval(self.min_diversity_ratio) {
            return Err(GaConfigError::InvalidMinDiversityRatio);
        }

        if !is_unit_interval(self.local_search_rate) {
            return Err(GaConfigError::InvalidLocalSearchRate);
        }

        if self.tournament_size == 0 {
            return Err(GaConfigError::TournamentSizeZero);
        }

        Ok(())
    }
}

fn is_unit_interval(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "Tests assert exact copied rates, seeded results, and fitness endpoints; approximate metrics use tolerances."
)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation_accepts_valid_config() {
        let config = GaConfig::try_new(8, 32, 100, 42)
            .expect("valid default config should pass validation")
            .with_mutation_rate(0.25)
            .with_elite_ratio(0.20)
            .with_offspring_ratio(0.50)
            .with_min_diversity_ratio(0.15)
            .with_selection_strategy(SelectionStrategy::Tournament)
            .with_tournament_size(5)
            .with_local_search_rate(0.25)
            .with_local_search_attempts(12)
            .validated()
            .expect("valid customized config should pass validation");

        assert_eq!(config.size, 8);
        assert_eq!(config.initial_population, 32);
        assert_eq!(config.max_epoch_count, 100);
        assert_eq!(config.min_diversity_ratio, 0.15);
        assert_eq!(config.selection_strategy, SelectionStrategy::Tournament);
        assert_eq!(config.tournament_size, 5);
        assert_eq!(config.local_search_rate, 0.25);
        assert_eq!(config.local_search_attempts, 12);
    }

    #[test]
    fn test_config_validation_rejects_invalid_config() {
        assert_eq!(
            GaConfig::new(0, 32, 100, 42).validate(),
            Err(GaConfigError::BoardSizeZero)
        );
        assert_eq!(
            GaConfig::new(8, 0, 100, 42).validate(),
            Err(GaConfigError::InitialPopulationZero)
        );
        assert_eq!(
            GaConfig::new(8, 32, 0, 42).validate(),
            Err(GaConfigError::MaxEpochCountZero)
        );
        assert_eq!(
            GaConfig::new(8, 32, 100, 42)
                .with_mutation_rate(f32::NAN)
                .validate(),
            Err(GaConfigError::InvalidMutationRate)
        );
        assert_eq!(
            GaConfig::new(8, 32, 100, 42)
                .with_elite_ratio(1.1)
                .validate(),
            Err(GaConfigError::InvalidEliteRatio)
        );
        assert_eq!(
            GaConfig::new(8, 32, 100, 42)
                .with_offspring_ratio(-0.1)
                .validate(),
            Err(GaConfigError::InvalidOffspringRatio)
        );
        assert_eq!(
            GaConfig::new(8, 32, 100, 42)
                .with_min_diversity_ratio(1.1)
                .validate(),
            Err(GaConfigError::InvalidMinDiversityRatio)
        );
        assert_eq!(
            GaConfig::new(8, 32, 100, 42)
                .with_local_search_rate(f32::INFINITY)
                .validate(),
            Err(GaConfigError::InvalidLocalSearchRate)
        );
        assert_eq!(
            GaConfig::new(8, 32, 100, 42)
                .with_tournament_size(0)
                .validate(),
            Err(GaConfigError::TournamentSizeZero)
        );
    }
}
