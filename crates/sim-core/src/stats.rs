use crate::team::{LINEUP, MAX_PITCHERS};

/// Longest game the engine plays. A game still tied after this many innings is
/// settled by a coin flip from the game's RNG stream.
pub const MAX_INNINGS: usize = 30;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BatLine {
    pub pa: u8,
    pub ab: u8,
    pub r: u8,
    pub h: u8,
    pub doubles: u8,
    pub triples: u8,
    pub hr: u8,
    pub rbi: u8,
    pub bb: u8,
    pub so: u8,
    pub hbp: u8,
    pub sf: u8,
    pub sb: u8,
    pub cs: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PitchLine {
    pub used: bool,
    /// Outs recorded while this pitcher was in.
    pub outs: u8,
    pub h: u8,
    /// Runs charged to this pitcher, including runners he left on base.
    pub r: u8,
    /// Earned runs. Approximation: a run is unearned if its runner reached on
    /// an error, scored on an error play, or started an extra inning on second.
    pub er: u8,
    pub bb: u8,
    pub so: u8,
    pub hr: u8,
    pub hbp: u8,
    pub bf: u8,
    pub pitches: u16,
}

/// Final result and statistics for one game. Index 0 is the away team, 1 the home team.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameOutcome {
    pub runs: [u16; 2],
    /// Runs by inning (index 0 = first inning).
    pub line: [[u8; MAX_INNINGS]; 2],
    /// Innings in which the away team batted. The home team batted in the same
    /// number, or one fewer when it led after the top of the last inning.
    pub innings: u8,
    pub home_batted_last: bool,
    pub errors: [u8; 2],
    pub bat: [[BatLine; LINEUP]; 2],
    pub pitch: [[PitchLine; MAX_PITCHERS]; 2],
}

impl GameOutcome {
    pub(crate) fn new() -> Self {
        Self {
            runs: [0; 2],
            line: [[0; MAX_INNINGS]; 2],
            innings: 0,
            home_batted_last: false,
            errors: [0; 2],
            bat: [[BatLine::default(); LINEUP]; 2],
            pitch: [[PitchLine::default(); MAX_PITCHERS]; 2],
        }
    }

    #[must_use]
    pub fn away_runs(&self) -> u16 {
        self.runs[0]
    }

    #[must_use]
    pub fn home_runs(&self) -> u16 {
        self.runs[1]
    }

    #[must_use]
    pub fn home_won(&self) -> bool {
        self.runs[1] > self.runs[0]
    }

    #[must_use]
    pub fn hits(&self, side: usize) -> u16 {
        self.bat[side].iter().map(|b| u16::from(b.h)).sum()
    }
}
