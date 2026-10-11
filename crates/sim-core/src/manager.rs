//! Field-manager decision points. The engine asks a [`Manager`] at the few
//! moments a real manager decides something; the default answer reproduces the
//! engine's built-in behaviour, so a team without a manager plays exactly as
//! before.

use crate::team::MAX_PITCHERS;

/// What a manager may see at a decision point. Only the public state of the
/// game: no hidden talent, no dice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameView {
    pub inning: u8,
    /// Side at bat: 0 = away (top), 1 = home (bottom).
    pub bat: u8,
    pub outs: u8,
    /// Runner on first, second, third.
    pub bases: [bool; 3],
    /// Runs scored so far: `[away, home]`.
    pub runs: [u16; 2],
    /// The fielding team's pitcher now in the game, as an index into its list.
    pub pitcher: u8,
    pub pitchers: u8,
    pub pitches_in_outing: u16,
    pub runs_in_outing: u8,
    /// Which of the fielding team's pitchers have already entered the game.
    pub used: [bool; MAX_PITCHERS],
}

impl GameView {
    /// Lead of the batting side (negative when behind).
    #[must_use]
    pub fn bat_lead(&self) -> i32 {
        let b = usize::from(self.bat);
        i32::from(self.runs[b]) - i32::from(self.runs[1 - b])
    }

    /// Bases as a 0..8 bitmask (bit 0 first base).
    #[must_use]
    pub fn base_state(&self) -> usize {
        usize::from(self.bases[0])
            | usize::from(self.bases[1]) << 1
            | usize::from(self.bases[2]) << 2
    }
}

/// A steal decision for the runner who could go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StealCall {
    /// The engine's own rate for stealing.
    Default,
    Go,
    Hold,
}

/// A pitching-change decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReliefCall {
    /// The engine's own rules (pitch and run limits, closer in a save spot).
    Default,
    Keep,
    /// Bring in this pitcher (an index not yet used).
    Bring(u8),
}

pub trait Manager {
    /// Asked of the batting side before each plate appearance when a runner
    /// could steal; `from` is the base he is on (1 or 2).
    fn steal(&mut self, _view: &GameView, _from: u8) -> StealCall {
        StealCall::Default
    }

    /// Asked of the fielding side at the start of every half-inning
    /// (`half_start`) and after every plate appearance.
    fn relief(&mut self, _view: &GameView, _half_start: bool) -> ReliefCall {
        ReliefCall::Default
    }

    /// Told of every plate appearance about to be played (both sides see the
    /// same state). For recording, not deciding.
    fn observe(&mut self, _view: &GameView) {}

    /// Told the final score, `[away, home]`.
    fn game_over(&mut self, _runs: [u16; 2]) {}
}

/// The built-in behaviour.
#[derive(Debug, Clone, Copy, Default)]
pub struct Baseline;

impl Manager for Baseline {}

/// Builds a fresh manager for each game a club plays; `slot` is the index of
/// the day's starter in the club's rotation.
pub trait ManagerMaker: Send + Sync + std::fmt::Debug {
    fn make(&self, slot: usize) -> Box<dyn Manager>;
}
