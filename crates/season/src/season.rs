use rayon::prelude::*;
use sim_core::{Baseline, Env, GameOutcome, LINEUP, Manager, NullSink, simulate_game_managed};

use crate::club::Club;
use crate::playoffs::{self, Playoffs};
use crate::schedule::{Fixture, Schedule, TEAMS};
use crate::totals::{BatTotals, PitchTotals};

/// Unique id (and so RNG stream) of a regular-season game.
#[must_use]
pub fn game_id(season: u32, number: u16) -> u64 {
    (u64::from(season) << 20) | u64::from(number)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Record {
    pub w: u16,
    pub l: u16,
    pub rs: u32,
    pub ra: u32,
}

impl Record {
    #[must_use]
    pub fn run_diff(&self) -> i64 {
        i64::from(self.rs) - i64::from(self.ra)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamSeason {
    pub record: Record,
    /// By batting-order slot (the order never changes within a season).
    pub batting: [BatTotals; LINEUP],
    /// By staff index: the rotation, then the bullpen.
    pub pitching: Vec<PitchTotals>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeasonResult {
    pub season: u32,
    pub schedule: Schedule,
    pub teams: Vec<TeamSeason>,
    pub playoffs: Playoffs,
}

impl SeasonResult {
    /// Hash of everything in the result; equal results have equal digests.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut h = 0xCBF2_9CE4_8422_2325u64;
        let mut eat = |x: u64| {
            for byte in x.to_le_bytes() {
                h ^= u64::from(byte);
                h = h.wrapping_mul(0x0000_0100_0000_01B3);
            }
        };
        eat(u64::from(self.season));
        for f in self.schedule.days.iter().flatten() {
            eat(u64::from(f.number));
            eat(u64::from(f.away) << 8 | u64::from(f.home));
        }
        for t in &self.teams {
            // Everything in the team's season, via its derived Debug text.
            for byte in format!("{t:?}").bytes() {
                eat(u64::from(byte));
            }
        }
        for s in &self.playoffs.series {
            eat(u64::from(s.a) << 16 | u64::from(s.b) << 8 | u64::from(s.round));
            eat(u64::from(s.a_wins) << 8 | u64::from(s.b_wins));
        }
        eat(self.playoffs.champion as u64);
        h
    }
}

/// Play one scheduled game again from its stream, exactly as it was played.
#[must_use]
pub fn replay_game(env: &Env, clubs: &[Club], f: &Fixture, seed: u64, season: u32) -> GameOutcome {
    play_fixture(
        env,
        &clubs[usize::from(f.away)],
        &clubs[usize::from(f.home)],
        f,
        seed,
        season,
    )
}

/// Like [`replay_game`] for clubs given by reference (rosters that change during a season).
#[must_use]
pub fn replay_game_with(
    env: &Env,
    clubs: &[&Club],
    f: &Fixture,
    seed: u64,
    season: u32,
) -> GameOutcome {
    play_fixture(
        env,
        clubs[usize::from(f.away)],
        clubs[usize::from(f.home)],
        f,
        seed,
        season,
    )
}

fn play_fixture(
    env: &Env,
    away: &Club,
    home: &Club,
    f: &Fixture,
    seed: u64,
    season: u32,
) -> GameOutcome {
    let a = &away.rotation[usize::from(f.away_starter)];
    let h = &home.rotation[usize::from(f.home_starter)];
    let mut ma = away
        .manager
        .as_ref()
        .map(|m| m.make(usize::from(f.away_starter)));
    let mut mh = home
        .manager
        .as_ref()
        .map(|m| m.make(usize::from(f.home_starter)));
    let (mut ba, mut bh) = (Baseline, Baseline);
    let ra: &mut dyn Manager = match ma.as_mut() {
        Some(b) => b.as_mut(),
        None => &mut ba,
    };
    let rh: &mut dyn Manager = match mh.as_mut() {
        Some(b) => b.as_mut(),
        None => &mut bh,
    };
    simulate_game_managed(
        env,
        a,
        h,
        seed,
        game_id(season, f.number),
        &mut NullSink,
        [ra, rh],
    )
}

fn record_game(teams: &mut [TeamSeason], clubs: &[Club], f: &Fixture, o: &GameOutcome) {
    let ids = [usize::from(f.away), usize::from(f.home)];
    let starters = [usize::from(f.away_starter), usize::from(f.home_starter)];
    for side in 0..2 {
        let t = &mut teams[ids[side]];
        let other = o.runs[1 - side];
        t.record.rs += u32::from(o.runs[side]);
        t.record.ra += u32::from(other);
        if o.runs[side] > other {
            t.record.w += 1;
        } else {
            t.record.l += 1;
        }
        for (slot, line) in o.bat[side].iter().enumerate() {
            t.batting[slot].add(line);
        }
        for (i, line) in o.pitch[side].iter().enumerate() {
            if line.used {
                let staff = clubs[ids[side]].staff_index(starters[side], i);
                t.pitching[staff].add(line);
            }
        }
    }
}

/// Play a full season: the regular season day by day (the games of a day run in
/// parallel), then the playoffs. The result does not depend on thread count.
#[must_use]
pub fn play_season(env: &Env, clubs: &[Club], seed: u64, season: u32) -> SeasonResult {
    assert_eq!(clubs.len(), TEAMS, "a season needs {TEAMS} clubs");
    let rotation = clubs[0].rotation.len();
    let schedule = Schedule::generate(seed, season, rotation);
    let mut teams: Vec<TeamSeason> = clubs
        .iter()
        .map(|c| TeamSeason {
            record: Record::default(),
            batting: [BatTotals::default(); LINEUP],
            pitching: vec![PitchTotals::default(); c.staff_size()],
        })
        .collect();
    for day in &schedule.days {
        let outcomes: Vec<GameOutcome> = day
            .par_iter()
            .map(|f| replay_game(env, clubs, f, seed, season))
            .collect();
        for (f, o) in day.iter().zip(&outcomes) {
            record_game(&mut teams, clubs, f, o);
        }
    }
    let records: Vec<Record> = teams.iter().map(|t| t.record).collect();
    let playoffs = playoffs::play_playoffs(env, clubs, &records, seed, season);
    SeasonResult {
        season,
        schedule,
        teams,
        playoffs,
    }
}
