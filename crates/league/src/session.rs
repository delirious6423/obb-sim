use protocol::{Actor, Command, CommandLog, Event, Snapshot};

use crate::state::{League, LeagueError};

/// A league plus the log of commands that built it.
///
/// A command is logged only if the league accepts it, so replaying the log
/// always succeeds and always gives the same league.
#[derive(Debug, Clone)]
pub struct Session {
    league: League,
    log: CommandLog,
}

impl Session {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            league: League::new(seed),
            log: CommandLog::new(),
        }
    }

    #[must_use]
    pub fn league(&self) -> &League {
        &self.league
    }

    #[must_use]
    pub fn log(&self) -> &CommandLog {
        &self.log
    }

    /// Run a command for `actor`. The actor's role and team are always checked.
    /// On success the command is logged and its events returned. On error
    /// nothing changes.
    pub fn submit(&mut self, actor: Actor, command: Command) -> Result<Vec<Event>, LeagueError> {
        let events = self.league.execute(&actor, &command)?;
        self.log.append(actor, command);
        Ok(events)
    }

    #[must_use]
    pub fn snapshot(&self) -> Snapshot<League> {
        Snapshot {
            applied: self.log.len() as u64,
            state: self.league.clone(),
        }
    }

    /// Rebuild from the league seed and a full command log.
    pub fn replay(seed: u64, log: &CommandLog) -> Result<Self, LeagueError> {
        Self::resume(
            Snapshot {
                applied: 0,
                state: League::new(seed),
            },
            log,
        )
    }

    /// Rebuild from a snapshot plus the log entries after it. Every entry is
    /// authorized again, so a tampered log is refused.
    pub fn resume(snapshot: Snapshot<League>, log: &CommandLog) -> Result<Self, LeagueError> {
        let log_len = log.len() as u64;
        if snapshot.applied > log_len {
            return Err(LeagueError::SnapshotAheadOfLog {
                applied: snapshot.applied,
                log_len,
            });
        }
        let state_applied = snapshot.state.commands_applied();
        if snapshot.applied != state_applied {
            return Err(LeagueError::SnapshotMismatch {
                applied: snapshot.applied,
                state_applied,
            });
        }
        let mut league = snapshot.state;
        for entry in log.since(snapshot.applied) {
            league.execute(&entry.actor, &entry.command)?;
        }
        Ok(Self {
            league,
            log: log.clone(),
        })
    }
}
