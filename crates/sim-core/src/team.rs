use crate::rates::Rates;

/// Batters in a batting order.
pub const LINEUP: usize = 9;
/// Pitchers a team may carry into a game: a starter, relievers and a closer.
pub const MAX_PITCHERS: usize = 12;

/// A player as the game engine sees them: a name for text output and the
/// outcome rates that drive the dice.
#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    pub name: String,
    pub rates: Rates,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamError {
    /// Fewer than nine batters.
    ShortLineup,
    /// No pitcher, or more than [`MAX_PITCHERS`].
    PitcherCount,
}

/// One side of a game.
///
/// The first nine batters are the batting order. Pitcher 0 starts; the last
/// pitcher is the closer (when there are at least two); those in between come
/// out of the bullpen in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Team {
    pub name: String,
    batters: Vec<Player>,
    pitchers: Vec<Player>,
}

impl Team {
    pub fn new(
        name: impl Into<String>,
        batters: Vec<Player>,
        pitchers: Vec<Player>,
    ) -> Result<Self, TeamError> {
        if batters.len() < LINEUP {
            return Err(TeamError::ShortLineup);
        }
        if pitchers.is_empty() || pitchers.len() > MAX_PITCHERS {
            return Err(TeamError::PitcherCount);
        }
        Ok(Self {
            name: name.into(),
            batters,
            pitchers,
        })
    }

    #[must_use]
    pub fn batters(&self) -> &[Player] {
        &self.batters
    }

    #[must_use]
    pub fn pitchers(&self) -> &[Player] {
        &self.pitchers
    }
}
