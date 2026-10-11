use std::collections::BTreeMap;

use protocol::{Actor, Command, Event, GameId, PlayerId, TeamId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::permissions::{PermissionError, authorize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    pub id: TeamId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub team: TeamId,
    pub name: String,
    pub rating: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub id: GameId,
    pub home: TeamId,
    pub away: TeamId,
    /// `(home_runs, away_runs)` once played.
    pub result: Option<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LeagueError {
    #[error(transparent)]
    Permission(#[from] PermissionError),
    #[error("unknown team {0:?}")]
    UnknownTeam(TeamId),
    #[error("unknown game {0:?}")]
    UnknownGame(GameId),
    #[error("a team cannot play itself")]
    SameTeam,
    #[error("game {0:?} was already played")]
    AlreadyPlayed(GameId),
    #[error("team {0:?} has no players")]
    EmptyRoster(TeamId),
    #[error("rating {0} is outside 1..=100")]
    BadRating(u8),
    #[error("a game cannot end tied {home_runs}-{away_runs}")]
    TiedResult { home_runs: u32, away_runs: u32 },
    #[error("expected {kind} id {expected}, got {got}")]
    NonSequentialId {
        kind: &'static str,
        expected: u32,
        got: u32,
    },
    #[error("no more {0} ids")]
    IdSpaceExhausted(&'static str),
    #[error("snapshot covers {applied} commands but the log has only {log_len}")]
    SnapshotAheadOfLog { applied: u64, log_len: u64 },
    #[error("snapshot says {applied} commands applied but its state says {state_applied}")]
    SnapshotMismatch { applied: u64, state_applied: u64 },
    #[error("corrupt league data: {0}")]
    Corrupt(&'static str),
}

/// All league state. Fields are private so that only events can change it.
/// Maps are `BTreeMap` so iteration order is always the same.
///
/// Deserializing validates the data (see `League::validate`), so a loaded
/// snapshot cannot hold a state that events could never have produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawLeague")]
pub struct League {
    seed: u64,
    teams: BTreeMap<TeamId, Team>,
    players: BTreeMap<PlayerId, Player>,
    games: BTreeMap<GameId, Game>,
    next_team: u32,
    next_player: u32,
    next_game: u32,
    // Bookkeeping, not league state: `execute` bumps this directly instead of
    // going through an event so a snapshot's `applied` can be checked against
    // the state it carries. Events describe the league, not how it was driven.
    commands_applied: u64,
}

/// Unvalidated shape of [`League`] used only while deserializing.
#[derive(Deserialize)]
struct RawLeague {
    seed: u64,
    teams: BTreeMap<TeamId, Team>,
    players: BTreeMap<PlayerId, Player>,
    games: BTreeMap<GameId, Game>,
    next_team: u32,
    next_player: u32,
    next_game: u32,
    commands_applied: u64,
}

impl TryFrom<RawLeague> for League {
    type Error = LeagueError;

    fn try_from(raw: RawLeague) -> Result<Self, LeagueError> {
        let league = Self {
            seed: raw.seed,
            teams: raw.teams,
            players: raw.players,
            games: raw.games,
            next_team: raw.next_team,
            next_player: raw.next_player,
            next_game: raw.next_game,
            commands_applied: raw.commands_applied,
        };
        league.validate()?;
        Ok(league)
    }
}

impl League {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            teams: BTreeMap::new(),
            players: BTreeMap::new(),
            games: BTreeMap::new(),
            next_team: 0,
            next_player: 0,
            next_game: 0,
            commands_applied: 0,
        }
    }

    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// How many commands `execute` has accepted. Zero for a league rebuilt
    /// with `from_events`, since events carry no commands.
    #[must_use]
    pub fn commands_applied(&self) -> u64 {
        self.commands_applied
    }

    pub fn teams(&self) -> impl Iterator<Item = &Team> {
        self.teams.values()
    }

    pub fn games(&self) -> impl Iterator<Item = &Game> {
        self.games.values()
    }

    #[must_use]
    pub fn roster(&self, team: TeamId) -> Vec<&Player> {
        self.players.values().filter(|p| p.team == team).collect()
    }

    /// Decide what a command does, without changing anything and without
    /// checking who issued it. Use [`League::execute`] to change the league.
    pub fn decide(&self, command: &Command) -> Result<Vec<Event>, LeagueError> {
        match command {
            Command::CreateTeam { name } => Ok(vec![Event::TeamCreated {
                team: TeamId(self.next_team),
                name: name.clone(),
            }]),
            Command::SignPlayer { team, name, rating } => {
                self.require_team(*team)?;
                if !(1..=100).contains(rating) {
                    return Err(LeagueError::BadRating(*rating));
                }
                Ok(vec![Event::PlayerSigned {
                    team: *team,
                    player: PlayerId(self.next_player),
                    name: name.clone(),
                    rating: *rating,
                }])
            }
            Command::ScheduleGame { home, away } => {
                self.require_team(*home)?;
                self.require_team(*away)?;
                if home == away {
                    return Err(LeagueError::SameTeam);
                }
                Ok(vec![Event::GameScheduled {
                    game: GameId(self.next_game),
                    home: *home,
                    away: *away,
                }])
            }
            Command::PlayGame { game } => {
                let g = self
                    .games
                    .get(game)
                    .ok_or(LeagueError::UnknownGame(*game))?;
                if g.result.is_some() {
                    return Err(LeagueError::AlreadyPlayed(*game));
                }
                let result = sim_core::play_game(
                    self.seed,
                    u64::from(game.0),
                    self.strength(g.home)?,
                    self.strength(g.away)?,
                );
                Ok(vec![Event::GamePlayed {
                    game: *game,
                    home_runs: result.home_runs,
                    away_runs: result.away_runs,
                }])
            }
        }
    }

    /// Authorize the actor, decide the command, then apply its events.
    /// The only way a command changes the league.
    ///
    /// `decide` produces events that `apply` accepts, so apply cannot fail
    /// halfway through a command's events.
    pub fn execute(&mut self, actor: &Actor, command: &Command) -> Result<Vec<Event>, LeagueError> {
        authorize(actor, command)?;
        let events = self.decide(command)?;
        for event in &events {
            self.apply(event)?;
        }
        self.commands_applied = self.commands_applied.saturating_add(1);
        Ok(events)
    }

    /// Rebuild a league by applying events to a fresh league. Any event that
    /// does not fit the state it arrives at is an error.
    pub fn from_events(seed: u64, events: &[Event]) -> Result<Self, LeagueError> {
        let mut league = Self::new(seed);
        for event in events {
            league.apply(event)?;
        }
        Ok(league)
    }

    /// The only place state changes. The event is checked first; on error the
    /// state is untouched.
    pub(crate) fn apply(&mut self, event: &Event) -> Result<(), LeagueError> {
        match event {
            Event::TeamCreated { team, name } => {
                let next = next_id("team", team.0, self.next_team)?;
                self.teams.insert(
                    *team,
                    Team {
                        id: *team,
                        name: name.clone(),
                    },
                );
                self.next_team = next;
            }
            Event::PlayerSigned {
                team,
                player,
                name,
                rating,
            } => {
                self.require_team(*team)?;
                if !(1..=100).contains(rating) {
                    return Err(LeagueError::BadRating(*rating));
                }
                let next = next_id("player", player.0, self.next_player)?;
                self.players.insert(
                    *player,
                    Player {
                        id: *player,
                        team: *team,
                        name: name.clone(),
                        rating: *rating,
                    },
                );
                self.next_player = next;
            }
            Event::GameScheduled { game, home, away } => {
                self.require_team(*home)?;
                self.require_team(*away)?;
                if home == away {
                    return Err(LeagueError::SameTeam);
                }
                let next = next_id("game", game.0, self.next_game)?;
                self.games.insert(
                    *game,
                    Game {
                        id: *game,
                        home: *home,
                        away: *away,
                        result: None,
                    },
                );
                self.next_game = next;
            }
            Event::GamePlayed {
                game,
                home_runs,
                away_runs,
            } => {
                if home_runs == away_runs {
                    return Err(LeagueError::TiedResult {
                        home_runs: *home_runs,
                        away_runs: *away_runs,
                    });
                }
                let g = self
                    .games
                    .get(game)
                    .ok_or(LeagueError::UnknownGame(*game))?;
                if g.result.is_some() {
                    return Err(LeagueError::AlreadyPlayed(*game));
                }
                let (home, away) = (g.home, g.away);
                // A game cannot have been played by a team with no players.
                self.strength(home)?;
                self.strength(away)?;
                if let Some(g) = self.games.get_mut(game) {
                    g.result = Some((*home_runs, *away_runs));
                }
            }
        }
        Ok(())
    }

    /// Check that loaded data is a state events could have produced: ids are
    /// 0..n in order and match their keys, counters equal the counts, every
    /// reference resolves, ratings are in range, no game is tied, and every
    /// played game had players on both sides.
    fn validate(&self) -> Result<(), LeagueError> {
        let corrupt = LeagueError::Corrupt;
        let is_sequence = |i: usize, key: u32| u32::try_from(i).ok() == Some(key);
        let counts = |len: usize, next: u32| u32::try_from(len).ok() == Some(next);

        for (i, (key, team)) in self.teams.iter().enumerate() {
            if !is_sequence(i, key.0) || team.id != *key {
                return Err(corrupt("team ids must run 0..n and match their keys"));
            }
        }
        for (i, (key, player)) in self.players.iter().enumerate() {
            if !is_sequence(i, key.0) || player.id != *key {
                return Err(corrupt("player ids must run 0..n and match their keys"));
            }
            if !self.teams.contains_key(&player.team) {
                return Err(corrupt("player belongs to an unknown team"));
            }
            if !(1..=100).contains(&player.rating) {
                return Err(corrupt("player rating outside 1..=100"));
            }
        }
        for (i, (key, game)) in self.games.iter().enumerate() {
            if !is_sequence(i, key.0) || game.id != *key {
                return Err(corrupt("game ids must run 0..n and match their keys"));
            }
            if !self.teams.contains_key(&game.home) || !self.teams.contains_key(&game.away) {
                return Err(corrupt("game refers to an unknown team"));
            }
            if game.home == game.away {
                return Err(corrupt("game has the same team on both sides"));
            }
            if game.result.is_some_and(|(h, a)| h == a) {
                return Err(corrupt("game result is a tie"));
            }
            if game.result.is_some()
                && (self.strength(game.home).is_err() || self.strength(game.away).is_err())
            {
                return Err(corrupt("played game has a team with no players"));
            }
        }
        if !counts(self.teams.len(), self.next_team)
            || !counts(self.players.len(), self.next_player)
            || !counts(self.games.len(), self.next_game)
        {
            return Err(corrupt("id counters do not match the number of records"));
        }
        Ok(())
    }

    fn require_team(&self, team: TeamId) -> Result<(), LeagueError> {
        if self.teams.contains_key(&team) {
            Ok(())
        } else {
            Err(LeagueError::UnknownTeam(team))
        }
    }

    /// Mean player rating on the team.
    fn strength(&self, team: TeamId) -> Result<u32, LeagueError> {
        let (sum, count) = self
            .players
            .values()
            .filter(|p| p.team == team)
            .fold((0u64, 0u64), |(s, c), p| (s + u64::from(p.rating), c + 1));
        let mean = sum
            .checked_div(count)
            .ok_or(LeagueError::EmptyRoster(team))?;
        Ok(u32::try_from(mean).unwrap_or(u32::MAX))
    }
}

/// Ids are handed out in order. An event must use exactly the next id, and
/// there must be a further id left for the one after it.
fn next_id(kind: &'static str, got: u32, expected: u32) -> Result<u32, LeagueError> {
    if got != expected {
        return Err(LeagueError::NonSequentialId {
            kind,
            expected,
            got,
        });
    }
    expected
        .checked_add(1)
        .ok_or(LeagueError::IdSpaceExhausted(kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_rejects_game_played_for_unknown_game() {
        let mut league = League::new(1);
        let before = league.clone();
        let err = league
            .apply(&Event::GamePlayed {
                game: GameId(3),
                home_runs: 2,
                away_runs: 1,
            })
            .unwrap_err();
        assert_eq!(err, LeagueError::UnknownGame(GameId(3)));
        assert_eq!(league, before);
    }

    #[test]
    fn last_id_is_refused_instead_of_overflowing() {
        assert_eq!(
            next_id("team", u32::MAX, u32::MAX),
            Err(LeagueError::IdSpaceExhausted("team"))
        );
    }
}
