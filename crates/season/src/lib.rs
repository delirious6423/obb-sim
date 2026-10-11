//! A full season: schedule, games, standings, playoffs.
//!
//! Pure like `sim-core`: the same seed gives the same season on any machine and
//! any number of threads. Every game has its own RNG stream (picked by season
//! and game number), so games can be played in parallel in any order.

mod club;
mod playoffs;
mod schedule;
mod season;
mod summary;
mod totals;

pub use club::{Club, fictional_league};
pub use playoffs::{Playoffs, Series};
pub use schedule::{Fixture, GAMES_PER_TEAM, Schedule, TEAMS, division_of, league_of, pair_games};
pub use season::{Record, SeasonResult, TeamSeason, game_id, play_season, replay_game};
pub use summary::{LeagueLine, SeasonSummary, TeamLine, summarize};
pub use totals::{BatTotals, PitchTotals};
