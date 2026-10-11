//! What a club believes about a player, and what the market pays.
//!
//! A club never sees true talent: it sees quality plus noise that is fixed by
//! (league seed, club, player, year), so the same club gives the same opinion
//! all year and different clubs disagree.

use crate::player::{Level, Origin, Player};
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

fn view(w: &World, key: u64, p: &Player) -> Opinion {
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
    let y = w.year as u64;
    Opinion {
        now: p.quality + sd_now * sdq * noise(w.seed, key, id, y * 2),
        peak: p.peak_quality + sd_peak * sdq * noise(w.seed, key, id, y * 2 + 1),
    }
}

/// A club's opinion of any player. `club` identifies whose eyes these are.
#[must_use]
pub fn opinion(w: &World, club: u8, p: &Player) -> Opinion {
    view(w, u64::from(club), p)
}

/// Club key of the market's consensus view.
const MARKET_KEY: u64 = 9_999;

/// Expected wins above replacement over a season at quality `q`.
#[must_use]
pub fn war_of(kind: Kind, q: f64) -> f64 {
    let volume = match kind {
        Kind::Batter => 650.0,
        Kind::Pitcher => 450.0,
    };
    (q + REPLACEMENT) * volume / 10.0
}

/// The market's (club-independent) view of a player's current quality: no
/// better informed than a scout, so prices never reveal true talent.
#[must_use]
pub fn public_quality(w: &World, p: &Player) -> f64 {
    view(w, MARKET_KEY, p).now
}

/// Signing bonus an amateur asks (millions), from the market's view of his
/// ceiling.
#[must_use]
pub fn amateur_demand(w: &World, p: &Player) -> f64 {
    let sdq = w.model.quality_sd(p.kind);
    let ceiling = (view(w, MARKET_KEY, p).peak / sdq / w.pool.spread.max(1.0)).clamp(-3.0, 4.0);
    let premium = if p.origin == Origin::HighSchool {
        1.3
    } else {
        1.0
    };
    let n = noise(w.seed, MARKET_KEY + 1, u64::from(p.id.0), 0);
    (0.15 * libm::exp(0.9 * ceiling + 0.3 * n) * premium).clamp(0.1, 8.0)
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
