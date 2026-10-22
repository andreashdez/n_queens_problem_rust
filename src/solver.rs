//! The search algorithms that the CLI, the GUI, and the parameter sweep can run.

use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Solver {
    /// The genetic algorithm in [`crate::ga`].
    #[default]
    Ga,
    /// The local-search baseline in [`crate::min_conflicts`].
    MinConflicts,
}

impl Solver {
    pub const ALL: [Self; 2] = [Self::Ga, Self::MinConflicts];

    /// The name used on the command line and in exported data.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ga => "ga",
            Self::MinConflicts => "min-conflicts",
        }
    }
}

impl fmt::Display for Solver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl FromStr for Solver {
    type Err = String;

    /// Accepts [`Solver::name`] in any letter case.
    fn from_str(raw_value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|solver| raw_value.eq_ignore_ascii_case(solver.name()))
            .ok_or_else(|| "must be one of: ga, min-conflicts".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_in_any_case() {
        for solver in Solver::ALL {
            assert_eq!(solver.to_string().parse(), Ok(solver));
            assert_eq!(solver.name().to_uppercase().parse(), Ok(solver));
        }
        assert_eq!(Solver::default(), Solver::Ga);
        assert!("genetic".parse::<Solver>().is_err());
    }
}
