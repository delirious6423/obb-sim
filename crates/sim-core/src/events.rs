use crate::rates::Outcome;

/// How a ball in play was hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ball {
    Ground,
    Fly,
    Line,
}

/// Which side of the inning: `Top` is the away team batting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    Top,
    Bottom,
}

/// One finished plate appearance. Players are indexes into their team's lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayRecord {
    pub batter: u8,
    pub pitcher: u8,
    pub outcome: Outcome,
    /// `Some` for balls in play.
    pub ball: Option<Ball>,
    /// The batter reached on an error (the outcome is then `InPlayOut`).
    pub error: bool,
    pub double_play: bool,
    pub sac_fly: bool,
    pub runs: u8,
    pub outs_after: u8,
    pub pitches: u8,
}

/// Something that happened in a game. Plain `Copy` data with no text, so
/// emitting one costs nothing; renderers turn them into words afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameEvent {
    HalfInningStart {
        inning: u8,
        half: Half,
    },
    /// Extra-inning runner placed on second base.
    GhostRunner {
        runner: u8,
    },
    PitcherChange {
        half: Half,
        pitcher: u8,
    },
    Steal {
        runner: u8,
        base: u8,
        success: bool,
    },
    Pa(PlayRecord),
    GameOver,
}

/// Receives game events as they happen.
pub trait Sink {
    fn emit(&mut self, event: GameEvent);
}

/// Throws events away. The compiler removes the calls, so the hot loop pays nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullSink;

impl Sink for NullSink {
    #[inline(always)]
    fn emit(&mut self, _event: GameEvent) {}
}

/// Keeps every event, for play-by-play text. Allocates, so use it for single
/// games you want to read, not for bulk simulation.
#[derive(Debug, Clone, Default)]
pub struct PlayLog {
    pub events: Vec<GameEvent>,
}

impl Sink for PlayLog {
    fn emit(&mut self, event: GameEvent) {
        self.events.push(event);
    }
}
