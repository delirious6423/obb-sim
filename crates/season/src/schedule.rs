//! The regular-season schedule.
//!
//! 30 clubs, two leagues of three five-club divisions. Each club plays 162
//! games: 16 against each division rival, 8 against each other club in its own
//! league, 2 against nine of the fifteen clubs in the other league (the nine
//! rotate each season). Every pair splits its games evenly home and away, so
//! each club hosts 81. Games are packed into days, at most one per club per day.
//! There are no series, travel or off-day rules.

use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

pub const TEAMS: usize = 30;
pub const GAMES_PER_TEAM: u16 = 162;
const LEAGUE_SIZE: usize = 15;
const DIVISION_SIZE: usize = 5;

#[must_use]
pub fn league_of(team: usize) -> usize {
    team / LEAGUE_SIZE
}

/// Global division index 0..6.
#[must_use]
pub fn division_of(team: usize) -> usize {
    team / DIVISION_SIZE
}

/// Games between two different clubs in the given season (always even).
#[must_use]
pub fn pair_games(a: usize, b: usize, season: u32) -> u8 {
    if division_of(a) == division_of(b) {
        16
    } else if league_of(a) == league_of(b) {
        8
    } else {
        let (x, y) = if league_of(a) == 0 { (a, b) } else { (b, a) };
        let offset = (season as usize) % LEAGUE_SIZE;
        if (y + offset - x) % LEAGUE_SIZE < 9 {
            2
        } else {
            0
        }
    }
}

/// One scheduled game. `number` is unique within the season.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fixture {
    pub number: u16,
    pub away: u8,
    pub home: u8,
    /// Which rotation slot starts for each side.
    pub away_starter: u8,
    pub home_starter: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub days: Vec<Vec<Fixture>>,
}

impl Schedule {
    #[must_use]
    pub fn games(&self) -> usize {
        self.days.iter().map(Vec::len).sum()
    }

    #[must_use]
    pub fn fixture(&self, number: u16) -> Option<&Fixture> {
        self.days.iter().flatten().find(|f| f.number == number)
    }

    /// Build the schedule for `season`. `rotation` is the number of starters.
    #[must_use]
    pub fn generate(seed: u64, season: u32, rotation: usize) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ (u64::from(season) << 32) ^ 0x5C4E_D01E);
        let mut rem = [[0u8; TEAMS]; TEAMS];
        let mut hosts = [[0u8; TEAMS]; TEAMS];
        for (i, (rem_row, host_row)) in rem.iter_mut().zip(hosts.iter_mut()).enumerate() {
            for (j, (r, h)) in rem_row.iter_mut().zip(host_row.iter_mut()).enumerate() {
                if i != j {
                    *r = pair_games(i, j, season);
                    *h = *r / 2;
                }
            }
        }
        let mut left = [GAMES_PER_TEAM; TEAMS];
        let mut days: Vec<Vec<Fixture>> = Vec::new();
        let mut number = 0u16;
        let mut starts = [0usize; TEAMS];

        while left.iter().any(|l| *l > 0) {
            // Busiest clubs choose first so everyone finishes together.
            let mut order: Vec<(u16, u64, usize)> = (0..TEAMS)
                .filter(|t| left[*t] > 0)
                .map(|t| (left[t], rng.next_u64(), t))
                .collect();
            order.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let mut used = [false; TEAMS];
            let mut day: Vec<(usize, usize)> = Vec::new();
            for &(_, _, t) in &order {
                if used[t] {
                    continue;
                }
                let best = order
                    .iter()
                    .filter(|(_, _, u)| *u != t && !used[*u] && rem[t][*u] > 0)
                    .min_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                let Some(&(_, _, u)) = best else { continue };
                used[t] = true;
                used[u] = true;
                let t_hosts = if hosts[t][u] > 0 && hosts[u][t] > 0 {
                    rng.next_u64() & 1 == 0
                } else {
                    hosts[t][u] > 0
                };
                let (home, away) = if t_hosts { (t, u) } else { (u, t) };
                hosts[home][away] -= 1;
                rem[t][u] -= 1;
                rem[u][t] -= 1;
                left[t] -= 1;
                left[u] -= 1;
                day.push((home, away));
            }
            day.sort_unstable();
            let fixtures = day
                .into_iter()
                .map(|(home, away)| {
                    let f = Fixture {
                        number,
                        away: away as u8,
                        home: home as u8,
                        away_starter: (starts[away] % rotation) as u8,
                        home_starter: (starts[home] % rotation) as u8,
                    };
                    starts[away] += 1;
                    starts[home] += 1;
                    number += 1;
                    f
                })
                .collect();
            days.push(fixtures);
        }
        Self { days }
    }
}
