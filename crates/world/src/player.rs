//! Players: hidden talent, development, ageing.

use rand_core::RngCore;

use serde::{Deserialize, Serialize};

use crate::rating::{Kind, Model, Z, normal, uniform};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerId(pub u32);

/// Where a prospect comes from; sets the age he turns pro and how uncertain
/// his development is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Origin {
    HighSchool,
    College,
    International,
}

impl Origin {
    #[must_use]
    pub fn entry_age(self) -> i32 {
        match self {
            Origin::HighSchool => 18,
            Origin::College => 21,
            Origin::International => 17,
        }
    }

    /// Spread (years) of when the player develops relative to the typical curve.
    #[must_use]
    pub fn shift_sd(self) -> f64 {
        match self {
            Origin::HighSchool => 1.6,
            Origin::College => 0.9,
            Origin::International => 1.4,
        }
    }
}

/// Where a signed player plays. Order: best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Level {
    Majors,
    Triple,
    Double,
    Single,
}

impl Level {
    pub const MINORS: [Level; 3] = [Level::Triple, Level::Double, Level::Single];

    /// Roster spots per kind of player at this level.
    #[must_use]
    pub fn capacity(self) -> usize {
        match self {
            Level::Majors => 13,
            Level::Triple => 11,
            Level::Double => 11,
            Level::Single => 14,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Contract {
    /// Millions per year.
    pub salary: f64,
    pub years: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Drafted {
    pub year: i32,
    pub pick: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Career {
    /// Seasons with at least one major-league plate appearance or batter faced.
    pub seasons: u16,
    pub war: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub kind: Kind,
    pub origin: Origin,
    pub born: i32,
    /// Hidden: talent at his peak.
    pub peak: Z,
    /// Hidden: years later (+) or earlier (-) than typical that he develops.
    pub shift: f64,
    /// Hidden: how fast he declines relative to typical.
    pub decline: f64,
    /// Hidden: accumulated luck and permanent injury damage, in quality
    /// standard deviations (positive is worse).
    pub drift: f64,
    /// Hidden: current talent (a function of the above and his age).
    pub z: Z,
    /// Hidden: runs per PA/BF above average, now and at peak.
    pub quality: f64,
    pub peak_quality: f64,
    pub team: Option<u8>,
    pub level: Level,
    pub contract: Option<Contract>,
    pub out_games: u16,
    /// Damage from injuries this season, applied at the next progression.
    pub pending_loss: f64,
    /// Years spent unsigned.
    pub free_years: u8,
    pub drafted: Option<Drafted>,
    pub debut: Option<i32>,
    pub career: Career,
}

impl Player {
    #[must_use]
    pub fn age(&self, year: i32) -> i32 {
        year - self.born
    }
}

/// Quality-sd below peak at a given (effective) age. Rises to a peak at 27,
/// then declines. Assumed shape, not measured from data.
#[must_use]
pub fn deficit(age_eff: f64, decline: f64) -> f64 {
    if age_eff < 27.0 {
        1.7 * libm::pow((27.0 - age_eff) / 9.0, 1.3)
    } else if age_eff <= 29.0 {
        0.0
    } else {
        0.13 * decline * libm::pow(age_eff - 29.0, 1.25)
    }
}

/// Talent and quality at `age` for a player with these hidden traits.
#[must_use]
pub fn talent_at(model: &Model, p: &Player, age: i32, era: &Z) -> (Z, f64) {
    let total = deficit(f64::from(age) - p.shift, p.decline) + p.drift;
    let delta = model.decline(p.kind, total * model.quality_sd(p.kind));
    let mut z = p.peak;
    for ((zi, d), e) in z.iter_mut().zip(delta).zip(era) {
        *zi += d + e;
    }
    let q = model.quality(p.kind, &z);
    (z, q)
}

/// How the amateur pool compares with the people who reach the majors.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Pool {
    /// Spread multiplier over the measured major-league talent spread.
    pub spread: f64,
    /// Mean peak quality of batters below the league average, in quality sds.
    pub offset: f64,
    /// The same for pitchers.
    pub offset_pitcher: f64,
}

pub const DEFAULT_POOL: Pool = Pool {
    spread: 1.3,
    offset: 2.5,
    offset_pitcher: 2.4,
};

const ONSETS: &[&str] = &[
    "Br", "Cal", "Dar", "El", "Fen", "Gar", "Hal", "Is", "Jor", "Kel", "Lor", "Mar", "Nor", "Os",
    "Pel", "Quin", "Ros", "Sal", "Tor", "Ul", "Vor", "Wes", "Yar", "Zan",
];
const MIDDLES: &[&str] = &["a", "e", "i", "o", "u", "ae", "io", "ar", "en", "or"];
const ENDS: &[&str] = &["n", "k", "l", "s", "th", "d", "ric", "vin", "tan", "mar"];
const SURNAME_ENDS: &[&str] = &[
    "son", "ford", "wick", "ley", "ton", "man", "worth", "field", "by", "ham",
];

fn pick<'a, R: RngCore>(rng: &mut R, items: &'a [&'a str]) -> &'a str {
    items[(rng.next_u64() % items.len() as u64) as usize]
}

fn make_name<R: RngCore>(rng: &mut R) -> String {
    format!(
        "{}{}{} {}{}{}",
        pick(rng, ONSETS),
        pick(rng, MIDDLES),
        pick(rng, ENDS),
        pick(rng, ONSETS),
        pick(rng, MIDDLES),
        pick(rng, SURNAME_ENDS)
    )
}

/// Slot bonus (millions) for a draft pick.
#[must_use]
pub fn slot_bonus(pick: u16) -> f64 {
    (8.0 * libm::exp(-0.045 * (f64::from(pick) - 1.0))).max(0.15)
}

/// A new amateur, aged for the draft of `year`.
pub fn prospect<R: RngCore>(
    model: &Model,
    rng: &mut R,
    id: PlayerId,
    year: i32,
    origin: Origin,
    pool: Pool,
    era: &[Z; 2],
) -> Player {
    let kind = if uniform(rng) < 0.52 {
        Kind::Batter
    } else {
        Kind::Pitcher
    };
    let draw = model.draw(kind, rng);
    let gap = match kind {
        Kind::Batter => pool.offset,
        Kind::Pitcher => pool.offset_pitcher,
    };
    let offset = model.decline(kind, gap * model.quality_sd(kind));
    let mut peak = [0.0; crate::rating::K];
    for i in 0..peak.len() {
        peak[i] = pool.spread * draw[i] + offset[i];
    }
    let shift = origin.shift_sd() * normal(rng);
    let decline = libm::exp(0.25 * normal(rng));
    let born = year - origin.entry_age();
    let mut p = Player {
        id,
        name: make_name(rng),
        kind,
        origin,
        born,
        peak,
        shift,
        decline,
        drift: 0.0,
        z: peak,
        quality: 0.0,
        peak_quality: model.quality(kind, &peak),
        team: None,
        level: Level::Single,
        contract: None,
        out_games: 0,
        pending_loss: 0.0,
        free_years: 0,
        drafted: None,
        debut: None,
        career: Career::default(),
    };
    let (z, q) = talent_at(
        model,
        &p,
        year - born,
        &era[usize::from(kind == Kind::Pitcher)],
    );
    p.z = z;
    p.quality = q;
    p
}
