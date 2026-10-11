//! Shared vocabulary: ids, roles, commands, events, the command log and snapshots.
//!
//! This crate holds data only. It never decides anything; the `league` crate does.

use serde::{Deserialize, Serialize};

macro_rules! id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub u32);
    };
}

id!(
    /// Identifies a team.
    TeamId
);
id!(
    /// Identifies a player.
    PlayerId
);
id!(
    /// Identifies a scheduled game. Also selects the game's RNG stream.
    GameId
);

/// The four seats in a league. Each can be a human or an AI agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Role {
    Commissioner,
    Owner,
    Gm,
    FieldManager,
}

/// Who is issuing a command: a seat plus, for every seat except the
/// commissioner, the one team it is bound to. The commissioner has no team
/// and acts league-wide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Actor {
    pub role: Role,
    pub team: Option<TeamId>,
}

impl Actor {
    #[must_use]
    pub fn commissioner() -> Self {
        Self {
            role: Role::Commissioner,
            team: None,
        }
    }

    #[must_use]
    pub fn owner(team: TeamId) -> Self {
        Self {
            role: Role::Owner,
            team: Some(team),
        }
    }

    #[must_use]
    pub fn gm(team: TeamId) -> Self {
        Self {
            role: Role::Gm,
            team: Some(team),
        }
    }

    #[must_use]
    pub fn field_manager(team: TeamId) -> Self {
        Self {
            role: Role::FieldManager,
            team: Some(team),
        }
    }
}

/// A request to change the league. Commands can be refused; events cannot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    CreateTeam {
        name: String,
    },
    SignPlayer {
        team: TeamId,
        name: String,
        rating: u8,
    },
    ScheduleGame {
        home: TeamId,
        away: TeamId,
    },
    PlayGame {
        game: GameId,
    },
}

/// A fact about what happened. State changes only by applying events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    TeamCreated {
        team: TeamId,
        name: String,
    },
    PlayerSigned {
        team: TeamId,
        player: PlayerId,
        name: String,
        rating: u8,
    },
    GameScheduled {
        game: GameId,
        home: TeamId,
        away: TeamId,
    },
    GamePlayed {
        game: GameId,
        home_runs: u32,
        away_runs: u32,
    },
}

/// A command plus who issued it and its position in the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggedCommand {
    pub seq: u64,
    pub actor: Actor,
    pub command: Command,
}

/// Append-only record of accepted commands. Replaying it rebuilds the league.
///
/// Deserializing checks that each entry's `seq` equals its index.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawCommandLog")]
pub struct CommandLog {
    entries: Vec<LoggedCommand>,
}

#[derive(Deserialize)]
struct RawCommandLog {
    entries: Vec<LoggedCommand>,
}

impl TryFrom<RawCommandLog> for CommandLog {
    type Error = String;

    fn try_from(raw: RawCommandLog) -> Result<Self, String> {
        for (i, entry) in raw.entries.iter().enumerate() {
            if u64::try_from(i).ok() != Some(entry.seq) {
                return Err(format!(
                    "log entry {i} has seq {}; seq must equal the index",
                    entry.seq
                ));
            }
        }
        Ok(Self {
            entries: raw.entries,
        })
    }
}

impl CommandLog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a command and return its sequence number (starting at 0).
    pub fn append(&mut self, actor: Actor, command: Command) -> u64 {
        let seq = self.entries.len() as u64;
        self.entries.push(LoggedCommand {
            seq,
            actor,
            command,
        });
        seq
    }

    #[must_use]
    pub fn entries(&self) -> &[LoggedCommand] {
        &self.entries
    }

    /// Entries with `seq >= from`.
    #[must_use]
    pub fn since(&self, from: u64) -> &[LoggedCommand] {
        let start = usize::try_from(from).map_or(self.entries.len(), |s| s.min(self.entries.len()));
        &self.entries[start..]
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// State captured after `applied` commands from the log have been applied.
/// To restore: load `state`, then replay `log.since(applied)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot<S> {
    pub applied: u64,
    pub state: S,
}
