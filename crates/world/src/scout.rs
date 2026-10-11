//! What a club believes about a player, and what the market pays.
//!
//! A club never sees true talent: it sees quality plus noise that is fixed by
//! (league seed, club, player, year), so the same club gives the same opinion
//! all year and different clubs disagree.

use crate::player::{Level, Player};
use crate::rating::Kind;
use crate::state::{MIN_SALARY, World};
use crate::util::noise;

/// Runs per PA of a replacement-level player below the league average.
pub const REPLACEMENT: f64 = 0.028;

fn level_sd(level: Level, has_team: bool) -> f64 {
    if !has_team {
        return 0.25;
    }
    match level {
        Level::Majors => 0.12,
        Level::Triple => 0.22,
        Level::Double => 0.30,
        Level::Single => 0.40,
    }
}

/// Opinion of one club.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Opinion {
    /// Quality now, in runs per PA (positive is good).
    pub now: f64,
    /// Quality at his peak.
    pub peak: f64,
}

/// Standard deviations of scouting error, in quality-sd units.
#[derive(Debug, Clone, Copy)]
pub struct Scouting {
    pub amateur_now: f64,
    pub amateur_peak: f64,
}

pub const SCOUTING: Scouting = Scouting {
    amateur_now: 1.5,
    amateur_peak: 1.5,
};

/// A club's opinion of any player. `club` identifies whose eyes these are.
#[must_use]
pub fn opinion(w: &World, club: u8, p: &Player) -> Opinion {
    let sdq = w.model.quality_sd(p.kind);
    let amateur =
        p.drafted.is_none() && p.team.is_none() && p.career.seasons == 0 && p.contract.is_none();
    let (sd_now, sd_peak) = if amateur {
        (SCOUTING.amateur_now, SCOUTING.amateur_peak)
    } else {
        let s = level_sd(p.level, p.team.is_some());
        (s, s * 1.5)
    };
    let id = u64::from(p.id.0);
    let c = u64::from(club);
    let y = w.year as u64;
    Opinion {
        now: p.quality + sd_now * sdq * noise(w.seed, c, id, y * 2),
        peak: p.peak_quality + sd_peak * sdq * noise(w.seed, c, id, y * 2 + 1),
    }
}

/// Expected wins above replacement over a season at quality `q`.
#[must_use]
pub fn war_of(kind: Kind, q: f64) -> f64 {
    let volume = match kind {
        Kind::Batter => 650.0,
        Kind::Pitcher => 450.0,
    };
    (q + REPLACEMENT) * volume / 10.0
}

/// The market's (club-independent) view of a player's current quality.
#[must_use]
pub fn public_quality(w: &World, p: &Player) -> f64 {
    let sdq = w.model.quality_sd(p.kind);
    p.quality + 0.15 * sdq * noise(w.seed, 9_999, u64::from(p.id.0), w.year as u64)
}

/// What the market asks per year for a player.
#[must_use]
pub fn asking_salary(w: &World, p: &Player) -> f64 {
    let war = war_of(p.kind, public_quality(w, p)).max(0.0);
    (w.rules.war_price * war).max(MIN_SALARY)
}

/// Years the market offers.
#[must_use]
pub fn asking_years(w: &World, p: &Player) -> u8 {
    match p.age(w.year) {
        ..=26 => 5,
        27..=29 => 4,
        30..=32 => 3,
        33..=35 => 2,
        _ => 1,
    }
}
