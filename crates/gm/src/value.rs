//! Valuing players from what a general manager can see.

use world::agents::{GmObs, Seen};
use world::player::{Level, deficit};
use world::rating::Kind;
use world::scout::{quality_from_war, war_of};
use world::util::noise;

use crate::profile::Profile;

/// A player as this general manager believes him to be.
#[derive(Debug, Clone, Copy)]
pub struct Est {
    pub seen: Seen,
    /// Believed current quality and peak quality.
    pub q: f64,
    pub peak: f64,
}

/// Spread of believed qualities, per kind, used to scale noise and ageing.
#[derive(Debug, Clone, Copy)]
pub struct Context {
    pub sd: [f64; 2],
    pub war_price: f64,
    /// The league minimum salary (current dollars).
    pub min_salary: f64,
    pub year: i32,
    seed: u64,
}

fn sd_of(values: impl Iterator<Item = f64>) -> f64 {
    let v: Vec<f64> = values.collect();
    if v.len() < 2 {
        return 0.01;
    }
    let m = v.iter().sum::<f64>() / v.len() as f64;
    let var = v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (v.len() - 1) as f64;
    libm::sqrt(var).max(1e-4)
}

fn slot(kind: Kind) -> usize {
    usize::from(kind == Kind::Pitcher)
}

impl Context {
    #[must_use]
    pub fn new(obs: &GmObs, seed: u64) -> Self {
        let sd = [Kind::Batter, Kind::Pitcher].map(|k| {
            sd_of(
                obs.roster
                    .iter()
                    .filter(|s| s.kind == k && s.level <= Level::Triple)
                    .map(|s| s.now),
            )
        });
        Self {
            sd,
            war_price: obs.war_price,
            min_salary: obs.money.min_salary,
            year: obs.year,
            seed,
        }
    }

    /// Believed quality now: own report, plus extra error, pooled with the
    /// market's view when the asking price carries one.
    #[must_use]
    pub fn estimate(&self, s: &Seen, p: &Profile) -> Est {
        let sd = self.sd[slot(s.kind)];
        let id = u64::from(s.id.0);
        let year = self.year as u64;
        let own = s.now + p.extra_noise * sd * noise(self.seed, 0xE5, id, year * 2);
        let peak = s.peak + p.extra_noise * sd * noise(self.seed, 0xE5, id, year * 2 + 1);
        let q = if s.ask > self.min_salary + 1e-9 && p.market_weight > 0.0 {
            let market = quality_from_war(s.kind, s.ask / self.war_price);
            (1.0 - p.market_weight) * own + p.market_weight * market
        } else {
            own
        };
        Est { seen: *s, q, peak }
    }

    /// Believed quality `t` seasons from now.
    #[must_use]
    pub fn quality_at(&self, e: &Est, t: u8, p: &Profile) -> f64 {
        if !p.projects_aging || t == 0 {
            return e.q;
        }
        let sd = self.sd[slot(e.seen.kind)];
        let age = f64::from(e.seen.age);
        let ahead = f64::from(t);
        let drift = sd * (deficit(age, 1.0) - deficit(age + ahead, 1.0));
        let mut q = e.q + drift;
        if age < 27.0 {
            q = q.min(e.q.max(e.peak));
        }
        q
    }

    /// Wins value minus pay over the first `years` seasons (at most the
    /// profile's horizon), each discounted.
    #[must_use]
    pub fn surplus(&self, e: &Est, salary: f64, years: u8, p: &Profile) -> f64 {
        let n = years.min(p.horizon).max(1);
        let mut total = 0.0;
        let mut d = 1.0;
        for t in 0..n {
            let q = self.quality_at(e, t, p);
            total += d * (self.war_price * war_of(e.seen.kind, q) - salary);
            d *= 0.9;
        }
        total
    }

    /// Surplus of a player on his present contract.
    #[must_use]
    pub fn contract_surplus(&self, e: &Est, p: &Profile) -> f64 {
        self.surplus(e, e.seen.salary, e.seen.years.max(1), p)
    }

    /// Average believed quality over the horizon (what a roster slot yields).
    #[must_use]
    pub fn mean_quality(&self, e: &Est, p: &Profile) -> f64 {
        let n = p.horizon.max(1);
        (0..n).map(|t| self.quality_at(e, t, p)).sum::<f64>() / f64::from(n)
    }

    /// The contract length (1 to 7): keep adding seasons while the next
    /// one is still worth more than it costs at the asking salary.
    #[must_use]
    pub fn best_years(&self, e: &Est, p: &Profile) -> u8 {
        if !p.picks_years {
            return e.seen.ask_years;
        }
        let mut years = 1;
        for t in 1..7u8 {
            let q = self.quality_at(e, t, p);
            if self.war_price * war_of(e.seen.kind, q) - e.seen.ask > 0.0 {
                years = t + 1;
            } else {
                break;
            }
        }
        years
    }
}
