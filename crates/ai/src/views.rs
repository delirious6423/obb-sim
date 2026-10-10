//! Role-limited views of the league. Each is a copy of just the data the role may see.

use league::League;
use protocol::{GameId, PlayerId, TeamId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameLine {
    pub game: GameId,
    pub home: TeamId,
    pub away: TeamId,
    pub result: Option<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterLine {
    pub player: PlayerId,
    pub name: String,
    pub rating: u8,
}

fn game_lines(league: &League, only: Option<TeamId>) -> Vec<GameLine> {
    league
        .games()
        .filter(|g| only.is_none_or(|t| g.home == t || g.away == t))
        .map(|g| GameLine {
            game: g.id,
            home: g.home,
            away: g.away,
            result: g.result,
        })
        .collect()
}

fn roster_lines(league: &League, team: TeamId) -> Vec<RosterLine> {
    league
        .roster(team)
        .into_iter()
        .map(|p| RosterLine {
            player: p.id,
            name: p.name.clone(),
            rating: p.rating,
        })
        .collect()
}

/// The commissioner sees the whole league's structure and schedule, not player ratings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommissionerView {
    pub teams: Vec<(TeamId, String)>,
    pub games: Vec<GameLine>,
}

impl CommissionerView {
    #[must_use]
    pub fn of(league: &League) -> Self {
        Self {
            teams: league.teams().map(|t| (t.id, t.name.clone())).collect(),
            games: game_lines(league, None),
        }
    }
}

/// An owner sees their own team's name, size and games, not who is on the roster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerView {
    pub team: TeamId,
    pub team_name: Option<String>,
    pub roster_size: usize,
    pub games: Vec<GameLine>,
}

impl OwnerView {
    #[must_use]
    pub fn of(league: &League, team: TeamId) -> Self {
        Self {
            team,
            team_name: league
                .teams()
                .find(|t| t.id == team)
                .map(|t| t.name.clone()),
            roster_size: league.roster(team).len(),
            games: game_lines(league, Some(team)),
        }
    }
}

/// A GM sees their own roster with ratings, and their team's games.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GmView {
    pub team: TeamId,
    pub roster: Vec<RosterLine>,
    pub games: Vec<GameLine>,
}

impl GmView {
    #[must_use]
    pub fn of(league: &League, team: TeamId) -> Self {
        Self {
            team,
            roster: roster_lines(league, team),
            games: game_lines(league, Some(team)),
        }
    }
}

/// A field manager sees their own roster and upcoming games.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldManagerView {
    pub team: TeamId,
    pub roster: Vec<RosterLine>,
    pub upcoming: Vec<GameLine>,
}

impl FieldManagerView {
    #[must_use]
    pub fn of(league: &League, team: TeamId) -> Self {
        let mut upcoming = game_lines(league, Some(team));
        upcoming.retain(|g| g.result.is_none());
        Self {
            team,
            roster: roster_lines(league, team),
            upcoming,
        }
    }
}
