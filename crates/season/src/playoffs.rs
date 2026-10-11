//! Playoffs: six clubs per league (three division winners, three wild cards).
//! Wild card round best of 3 (seeds 3v6, 4v5; seeds 1 and 2 wait), division
//! series best of 5 (1 v 4/5 winner, 2 v 3/6 winner), league series and World
//! Series best of 7. Ranking: wins, then run difference, then club number.

use rayon::prelude::*;
use sim_core::{Env, NullSink, simulate_game};

use crate::club::Club;
use crate::schedule::{TEAMS, division_of, league_of};
use crate::season::Record;

const PLAYOFF_ID_BASE: u64 = 1 << 19;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Series {
    /// 0 wild card, 1 division series, 2 league series, 3 World Series.
    pub round: u8,
    /// Club with home-field advantage.
    pub a: u8,
    pub b: u8,
    pub a_wins: u8,
    pub b_wins: u8,
}

impl Series {
    #[must_use]
    pub fn winner(&self) -> u8 {
        if self.a_wins > self.b_wins {
            self.a
        } else {
            self.b
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playoffs {
    /// Seeds 1..=6 of each league, best first.
    pub seeds: [[u8; 6]; 2],
    pub series: Vec<Series>,
    pub champion: usize,
}

fn better(records: &[Record], x: usize, y: usize) -> bool {
    key(&records[x], x) > key(&records[y], y)
}

fn key(r: &Record, t: usize) -> (u16, i64, i64) {
    (r.w, r.run_diff(), -(t as i64))
}

fn seeds(records: &[Record], league: usize) -> [u8; 6] {
    let members: Vec<usize> = (0..TEAMS).filter(|t| league_of(*t) == league).collect();
    let mut winners: Vec<usize> = Vec::new();
    for d in 0..6 {
        let best = members
            .iter()
            .copied()
            .filter(|t| division_of(*t) == d)
            .max_by_key(|t| key(&records[*t], *t));
        if let Some(b) = best {
            winners.push(b);
        }
    }
    winners.sort_by_key(|t| std::cmp::Reverse(key(&records[*t], *t)));
    let mut rest: Vec<usize> = members
        .into_iter()
        .filter(|t| !winners.contains(t))
        .collect();
    rest.sort_by_key(|t| std::cmp::Reverse(key(&records[*t], *t)));
    let mut out = [0u8; 6];
    for (i, t) in winners.iter().chain(rest.iter().take(3)).enumerate() {
        out[i] = *t as u8;
    }
    out
}

/// Who hosts each game: true when club `a` does.
fn hosts(best_of: u8) -> &'static [bool] {
    match best_of {
        3 => &[true, true, true],
        5 => &[true, true, false, false, true],
        _ => &[true, true, false, false, false, true, true],
    }
}

struct Played {
    series: Series,
    /// Games each club started, to carry the rotation along.
    started: [usize; 2],
}

#[allow(clippy::too_many_arguments)]
fn series(
    env: &Env,
    clubs: &[Club],
    seed: u64,
    season: u32,
    slot: u64,
    round: u8,
    teams: (u8, u8),
    starts: (usize, usize),
) -> Played {
    let best_of = match round {
        0 => 3,
        1 => 5,
        _ => 7,
    };
    let need = best_of / 2 + 1;
    let (a, b) = teams;
    let (mut sa, mut sb) = starts;
    let mut s = Series {
        round,
        a,
        b,
        a_wins: 0,
        b_wins: 0,
    };
    for (g, a_hosts) in hosts(best_of).iter().enumerate() {
        if s.a_wins == need || s.b_wins == need {
            break;
        }
        let (ca, cb) = (&clubs[usize::from(a)], &clubs[usize::from(b)]);
        let (ta, tb) = (
            &ca.rotation[sa % ca.rotation.len()],
            &cb.rotation[sb % cb.rotation.len()],
        );
        sa += 1;
        sb += 1;
        let id = (u64::from(season) << 20) | PLAYOFF_ID_BASE | (slot << 3) | g as u64;
        let a_won = if *a_hosts {
            let o = simulate_game(env, tb, ta, seed, id, &mut NullSink);
            o.runs[1] > o.runs[0]
        } else {
            let o = simulate_game(env, ta, tb, seed, id, &mut NullSink);
            o.runs[0] > o.runs[1]
        };
        if a_won {
            s.a_wins += 1;
        } else {
            s.b_wins += 1;
        }
    }
    Played {
        series: s,
        started: [sa, sb],
    }
}

/// Play a round: the series are independent, so they run in parallel.
#[allow(clippy::too_many_arguments)]
fn round(
    env: &Env,
    clubs: &[Club],
    seed: u64,
    season: u32,
    round: u8,
    pairs: &[(u8, u8)],
    starts: &mut [usize],
    out: &mut Vec<Series>,
) -> Vec<u8> {
    let played: Vec<Played> = pairs
        .par_iter()
        .enumerate()
        .map(|(i, p)| {
            let slot = u64::from(round) * 8 + i as u64;
            series(
                env,
                clubs,
                seed,
                season,
                slot,
                round,
                *p,
                (starts[usize::from(p.0)], starts[usize::from(p.1)]),
            )
        })
        .collect();
    let mut winners = Vec::with_capacity(played.len());
    for (p, pl) in pairs.iter().zip(&played) {
        starts[usize::from(p.0)] = pl.started[0];
        starts[usize::from(p.1)] = pl.started[1];
        out.push(pl.series);
        winners.push(pl.series.winner());
    }
    winners
}

/// Play the playoffs for a finished regular season, given each club's record.
#[must_use]
pub fn play_playoffs(
    env: &Env,
    clubs: &[Club],
    records: &[Record],
    seed: u64,
    season: u32,
) -> Playoffs {
    let seeds = [seeds(records, 0), seeds(records, 1)];
    let mut starts: Vec<usize> = vec![usize::from(crate::schedule::GAMES_PER_TEAM); TEAMS];
    let mut all = Vec::new();

    // Wild card round: 3v6 and 4v5 in each league.
    let wc_pairs: Vec<(u8, u8)> = seeds
        .iter()
        .flat_map(|s| [(s[2], s[5]), (s[3], s[4])])
        .collect();
    let wc = round(
        env,
        clubs,
        seed,
        season,
        0,
        &wc_pairs,
        &mut starts,
        &mut all,
    );

    // Division series: 1 v (4/5 winner), 2 v (3/6 winner).
    let mut ds_pairs = Vec::new();
    for (l, s) in seeds.iter().enumerate() {
        let (w36, w45) = (wc[l * 2], wc[l * 2 + 1]);
        ds_pairs.push((s[0], w45));
        ds_pairs.push((s[1], w36));
    }
    let ds = round(
        env,
        clubs,
        seed,
        season,
        1,
        &ds_pairs,
        &mut starts,
        &mut all,
    );

    // League series: the better record hosts.
    let ordered = |x: u8, y: u8| {
        if better(records, usize::from(x), usize::from(y)) {
            (x, y)
        } else {
            (y, x)
        }
    };
    let lcs_pairs: Vec<(u8, u8)> = (0..2).map(|l| ordered(ds[l * 2], ds[l * 2 + 1])).collect();
    let lcs = round(
        env,
        clubs,
        seed,
        season,
        2,
        &lcs_pairs,
        &mut starts,
        &mut all,
    );

    let ws_pair = [ordered(lcs[0], lcs[1])];
    let ws = round(env, clubs, seed, season, 3, &ws_pair, &mut starts, &mut all);
    Playoffs {
        seeds,
        series: all,
        champion: usize::from(ws[0]),
    }
}
