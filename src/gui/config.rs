//! Run settings as the GUI edits them, and the presets that fill them in.

use serde::{Deserialize, Serialize};

use crate::ga::{self, GaConfig, SelectionStrategy};

/// Saved between sessions; fields missing from an older save keep their defaults.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct GuiConfig {
    pub(super) board_size: u16,
    pub(super) population_size: u32,
    pub(super) max_epochs: u32,
    pub(super) seed: u64,
    pub(super) mutation_rate: f32,
    pub(super) elite_ratio: f32,
    pub(super) offspring_ratio: f32,
    pub(super) min_diversity_ratio: f32,
    #[serde(with = "selection_strategy")]
    pub(super) selection_strategy: SelectionStrategy,
    pub(super) tournament_size: u32,
    pub(super) local_search_rate: f32,
    pub(super) local_search_attempts: u32,
    pub(super) allow_unsolvable: bool,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            board_size: ga::DEFAULT_BOARD_SIZE,
            population_size: u32::try_from(ga::DEFAULT_POPULATION_SIZE)
                .expect("default population fits u32"),
            max_epochs: ga::DEFAULT_MAX_EPOCHS,
            seed: 42,
            mutation_rate: ga::DEFAULT_MUTATION_RATE,
            elite_ratio: ga::DEFAULT_ELITE_RATIO,
            offspring_ratio: ga::DEFAULT_OFFSPRING_RATIO,
            min_diversity_ratio: ga::DEFAULT_MIN_DIVERSITY_RATIO,
            selection_strategy: ga::DEFAULT_SELECTION_STRATEGY,
            tournament_size: u32::try_from(ga::DEFAULT_TOURNAMENT_SIZE)
                .expect("default tournament size fits u32"),
            local_search_rate: ga::DEFAULT_LOCAL_SEARCH_RATE,
            local_search_attempts: u32::try_from(ga::DEFAULT_LOCAL_SEARCH_ATTEMPTS)
                .expect("default local-search budget fits u32"),
            allow_unsolvable: false,
        }
    }
}

impl GuiConfig {
    pub(super) fn normalize(&mut self) {
        self.board_size = self.board_size.max(1);
        self.population_size = self.population_size.max(1);
        self.max_epochs = self.max_epochs.max(1);
        self.tournament_size = self.tournament_size.max(1);
        self.mutation_rate = self.mutation_rate.clamp(0.0, 1.0);
        self.elite_ratio = self.elite_ratio.clamp(0.0, 1.0);
        self.offspring_ratio = self.offspring_ratio.clamp(0.0, 1.0);
        self.min_diversity_ratio = self.min_diversity_ratio.clamp(0.0, 1.0);
        self.local_search_rate = self.local_search_rate.clamp(0.0, 1.0);
    }

    pub(super) fn to_ga_config(&self) -> Result<GaConfig, ga::GaConfigError> {
        GaConfig::new(
            self.board_size,
            self.population_size as usize,
            self.max_epochs,
            self.seed,
        )
        .with_mutation_rate(self.mutation_rate)
        .with_elite_ratio(self.elite_ratio)
        .with_offspring_ratio(self.offspring_ratio)
        .with_min_diversity_ratio(self.min_diversity_ratio)
        .with_selection_strategy(self.selection_strategy)
        .with_tournament_size(self.tournament_size as usize)
        .with_local_search_rate(self.local_search_rate)
        .with_local_search_attempts(self.local_search_attempts as usize)
        .validated()
    }

    /// The local-search hybrid, useful when a board stalls near a solution.
    fn use_hybrid_values(&mut self) {
        *self = Self {
            population_size: 4_000,
            selection_strategy: SelectionStrategy::Tournament,
            local_search_rate: 0.05,
            seed: self.seed,
            ..Self::default()
        };
    }

    /// The pre-2026-09-14 defaults: a large population with roulette selection
    /// and no local search. Spelled out rather than derived from the current
    /// defaults so this stays the classic configuration as those defaults move.
    fn use_classic_values(&mut self) {
        *self = Self {
            population_size: 40_000,
            mutation_rate: 0.08,
            offspring_ratio: 0.10,
            selection_strategy: SelectionStrategy::Roulette,
            local_search_rate: 0.0,
            seed: self.seed,
            ..Self::default()
        };
    }

    /// Tuned for 5000×5000. Crossover dominates each epoch at this size, so a small
    /// population with frequent local search reaches a solution sooner than the
    /// defaults; the epoch budget leaves headroom over the slowest measured seed.
    fn use_large_board_values(&mut self) {
        *self = Self {
            board_size: 5_000,
            population_size: 100,
            max_epochs: 50_000,
            local_search_rate: 0.1,
            local_search_attempts: 16,
            seed: self.seed,
            ..Self::default()
        };
    }

    const fn use_fast_demo_values(&mut self) {
        self.board_size = 8;
        self.population_size = 256;
        self.max_epochs = 250;
        self.mutation_rate = 0.12;
        self.elite_ratio = 0.15;
        self.offspring_ratio = 0.25;
        self.selection_strategy = SelectionStrategy::Tournament;
        self.tournament_size = 3;
        self.local_search_rate = 0.05;
        self.local_search_attempts = 8;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum GuiPreset {
    Demo,
    Default,
    Hybrid,
    Classic,
    LargeBoard,
}

impl GuiPreset {
    pub(super) const ALL: [Self; 5] = [
        Self::Demo,
        Self::Default,
        Self::Hybrid,
        Self::Classic,
        Self::LargeBoard,
    ];

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Demo => "Quick demo · 8×8",
            Self::Default => "Recommended defaults · 18×18",
            Self::Hybrid => "Local-search hybrid · 18×18",
            Self::Classic => "Classic genetic algorithm · 18×18",
            Self::LargeBoard => "Large board · 5000×5000",
        }
    }

    pub(super) fn config(self, seed: u64) -> GuiConfig {
        let mut config = GuiConfig {
            seed,
            ..Default::default()
        };
        match self {
            Self::Demo => config.use_fast_demo_values(),
            Self::Hybrid => config.use_hybrid_values(),
            Self::Classic => config.use_classic_values(),
            Self::LargeBoard => config.use_large_board_values(),
            Self::Default => {}
        }
        config
    }

    pub(super) fn matching(config: &GuiConfig) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|preset| preset.config(config.seed) == *config)
    }
}

impl GuiConfig {
    /// All tokens are typed numbers or fixed enum values, so no shell quoting is needed.
    pub(super) fn cli_command(&self) -> String {
        format!(
            "cargo run --release --locked -- --size {} --population {} --epochs {} --seed {} --mutation-rate {} --elite-ratio {} --offspring-ratio {} --min-diversity-ratio {} --selection {} --tournament-size {} --local-search-rate {} --local-search-attempts {}{}",
            self.board_size,
            self.population_size,
            self.max_epochs,
            self.seed,
            self.mutation_rate,
            self.elite_ratio,
            self.offspring_ratio,
            self.min_diversity_ratio,
            self.selection_strategy,
            self.tournament_size,
            self.local_search_rate,
            self.local_search_attempts,
            if self.allow_unsolvable {
                " --allow-unsolvable"
            } else {
                ""
            }
        )
    }
}

/// Stored by name so the solver's enum needs no serde support.
mod selection_strategy {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    use crate::ga::SelectionStrategy;

    #[allow(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's `with` attribute passes fields by reference."
    )]
    pub(super) fn serialize<S: Serializer>(
        strategy: &SelectionStrategy,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_str(strategy)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<SelectionStrategy, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "roulette" => Ok(SelectionStrategy::Roulette),
            "tournament" => Ok(SelectionStrategy::Tournament),
            other => Err(D::Error::unknown_variant(
                other,
                &["roulette", "tournament"],
            )),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "These tests check exact preset values, not approximate calculations."
)]
mod tests {
    use super::*;

    #[test]
    fn presets_reset_all_parameters_and_preserve_seed() {
        for preset in GuiPreset::ALL {
            let config = preset.config(123);
            assert_eq!(config.seed, 123);
            assert!(config.to_ga_config().is_ok());
            assert!(GuiPreset::matching(&config) == Some(preset));
            assert!(!config.allow_unsolvable);
            let mut custom = config.clone();
            custom.population_size += 1;
            assert!(GuiPreset::matching(&custom).is_none());
        }
        let hybrid = GuiPreset::Hybrid.config(42);
        assert_eq!(hybrid.population_size, 4_000);
        assert_eq!(hybrid.local_search_rate, 0.05);
        assert_eq!(hybrid.selection_strategy, SelectionStrategy::Tournament);

        // The classic preset must keep the pre-2026-09-14 behavior even as the
        // shipped defaults change, so it is checked against literals.
        let classic = GuiPreset::Classic.config(42);
        assert_eq!(classic.population_size, 40_000);
        assert_eq!(classic.mutation_rate, 0.08);
        assert_eq!(classic.offspring_ratio, 0.10);
        assert_eq!(classic.selection_strategy, SelectionStrategy::Roulette);
        assert_eq!(classic.local_search_rate, 0.0);

        let large = GuiPreset::LargeBoard.config(42);
        assert_eq!(large.board_size, 5_000);
        assert_eq!(large.population_size, 100);
        assert_eq!(large.local_search_rate, 0.1);
        assert_eq!(large.local_search_attempts, 16);
        assert!(large.max_epochs > GuiConfig::default().max_epochs);

        // The plain default preset must track the shipped defaults.
        let default = GuiPreset::Default.config(42);
        assert_eq!(
            usize::try_from(default.population_size).unwrap(),
            ga::DEFAULT_POPULATION_SIZE
        );
        assert_eq!(default.selection_strategy, ga::DEFAULT_SELECTION_STRATEGY);
        assert_eq!(default.mutation_rate, ga::DEFAULT_MUTATION_RATE);
    }

    #[test]
    fn command_contains_every_setting_without_changing_saved_values() {
        let mut config = GuiPreset::Hybrid.config(u64::MAX);
        config.allow_unsolvable = true;
        config.local_search_attempts = 17;
        let command = config.cli_command();
        let tokens: Vec<_> = command.split_whitespace().collect();
        let expected = [
            ("--size", config.board_size.to_string()),
            ("--population", config.population_size.to_string()),
            ("--epochs", config.max_epochs.to_string()),
            ("--seed", config.seed.to_string()),
            ("--mutation-rate", config.mutation_rate.to_string()),
            ("--elite-ratio", config.elite_ratio.to_string()),
            ("--offspring-ratio", config.offspring_ratio.to_string()),
            (
                "--min-diversity-ratio",
                config.min_diversity_ratio.to_string(),
            ),
            ("--selection", config.selection_strategy.to_string()),
            ("--tournament-size", config.tournament_size.to_string()),
            ("--local-search-rate", config.local_search_rate.to_string()),
            ("--local-search-attempts", "17".into()),
        ];
        for (flag, value) in expected {
            assert!(tokens.windows(2).any(|pair| pair == [flag, value.as_str()]));
        }
        assert!(tokens.contains(&"--allow-unsolvable"));
    }
}
