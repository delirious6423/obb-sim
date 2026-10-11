//! Ratings-to-outcomes fit.
//!
//! A player's hidden talent is a vector of z-scores, one per plate-appearance
//! outcome. The outcome rates are the league rates times `exp(sd * z - sd^2/2)`
//! (the same lognormal spread measured in the data, so a population with unit-
//! variance z reproduces the measured talent spread), renormalised to sum to 1.
//! Talent components are correlated as measured (shrunk toward independence).
//!
//! `quality` is the single number used for rankings: runs per plate appearance
//! above the league average, by linear weights (for pitchers, runs saved).

use calibration::{Calibration, Talent};
use rand_core::RngCore;
use sim_core::{OUTCOMES, Rates};

pub const K: usize = OUTCOMES;
/// Talent as z-scores, in [`sim_core::Outcome`] order.
pub type Z = [f64; K];

/// Textbook linear weights (runs relative to an out), in outcome order.
pub const LINEAR_WEIGHTS: [f64; K] = [-0.03, 0.70, 0.73, 0.89, 1.26, 1.59, 2.05, 0.0];

/// The measured talent spread is shrunk by this factor: it was measured among
/// regulars (with outcome correlations only partly known), and 1.0 gives team
/// batting-average spreads about 20% above the real ones.
const SPREAD_SCALE: f64 = 0.85;

/// Share of the measured correlation kept (the measured values partly reflect
/// that rates share a denominator, which the renormalisation already creates).
const CORRELATION_KEPT: f64 = 0.5;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Kind {
    Batter,
    Pitcher,
}

impl Kind {
    fn index(self) -> usize {
        match self {
            Kind::Batter => 0,
            Kind::Pitcher => 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Model {
    league: Rates,
    sd: [[f64; K]; 2],
    chol: [[[f64; K]; K]; 2],
    /// Change in z that lowers quality by one population standard deviation
    /// of quality (used for ageing and injuries).
    dir: [Z; 2],
    quality_sd: [f64; 2],
}

fn cholesky(c: &[[f64; K]; K]) -> [[f64; K]; K] {
    let mut eps = 0.0;
    loop {
        let mut l = [[0.0; K]; K];
        let mut ok = true;
        'rows: for i in 0..K {
            for j in 0..=i {
                let mut sum = c[i][j] + if i == j { eps } else { 0.0 };
                sum -= l[i][..j]
                    .iter()
                    .zip(&l[j][..j])
                    .map(|(a, b)| a * b)
                    .sum::<f64>();
                if i == j {
                    if sum <= 1e-9 {
                        ok = false;
                        break 'rows;
                    }
                    l[i][j] = libm::sqrt(sum);
                } else {
                    l[i][j] = sum / l[j][j];
                }
            }
        }
        if ok {
            return l;
        }
        eps = if eps == 0.0 { 0.01 } else { eps * 2.0 };
    }
}

fn correlation_matrix(measured: &[Vec<f64>]) -> [[f64; K]; K] {
    let mut c = [[0.0; K]; K];
    for (i, row) in c.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for (i, row) in measured.iter().enumerate().take(K - 1) {
        for (j, v) in row.iter().enumerate().take(K - 1) {
            if i != j {
                c[i][j] = CORRELATION_KEPT * v;
            }
        }
    }
    c
}

impl Model {
    /// Fit to measured league rates, talent spread and correlations.
    #[must_use]
    pub fn from_calibration(cal: &Calibration) -> Self {
        Self::build(
            cal.params.league,
            &cal.talent,
            &correlation_matrix(&cal.correlation.batter),
            &correlation_matrix(&cal.correlation.pitcher),
        )
    }

    /// Generic model with independent components, for tests without data.
    #[must_use]
    pub fn generic() -> Self {
        let mut ident = [[0.0; K]; K];
        for (i, row) in ident.iter_mut().enumerate() {
            row[i] = 1.0;
        }
        Self::build(
            sim_core::GameParams::reference().league,
            &Talent::generic(),
            &ident,
            &ident,
        )
    }

    fn build(league: Rates, talent: &Talent, cb: &[[f64; K]; K], cp: &[[f64; K]; K]) -> Self {
        let mut m = Self {
            league,
            sd: [
                talent.batter_sd.map(|x| x * SPREAD_SCALE),
                talent.pitcher_sd.map(|x| x * SPREAD_SCALE),
            ],
            chol: [cholesky(cb), cholesky(cp)],
            dir: [[0.0; K]; 2],
            quality_sd: [0.0; 2],
        };
        for kind in [Kind::Batter, Kind::Pitcher] {
            let k = kind.index();
            let mut g = [0.0; K];
            let h = 1e-4;
            for (i, gi) in g.iter_mut().enumerate() {
                let mut up = [0.0; K];
                let mut dn = [0.0; K];
                up[i] = h;
                dn[i] = -h;
                *gi = (m.quality(kind, &up) - m.quality(kind, &dn)) / (2.0 * h);
            }
            // Variance of quality: g' (L L') g.
            let mut var = 0.0;
            for j in 0..K {
                let mut s = 0.0;
                for (i, gi) in g.iter().enumerate() {
                    s += gi * m.chol[k][i][j];
                }
                var += s * s;
            }
            let sdq = libm::sqrt(var);
            let norm2: f64 = g.iter().map(|x| x * x).sum();
            m.quality_sd[k] = sdq;
            for (d, gi) in m.dir[k].iter_mut().zip(g) {
                *d = gi * sdq / norm2;
            }
        }
        m
    }

    #[must_use]
    pub fn league(&self) -> &Rates {
        &self.league
    }

    /// Outcome rates for talent `z` (clamped to +-4 standard deviations).
    #[must_use]
    pub fn rates(&self, kind: Kind, z: &Z) -> Rates {
        let sd = &self.sd[kind.index()];
        let mut r = [0.0; K];
        for i in 0..K {
            let zi = z[i].clamp(-4.0, 4.0);
            r[i] = self.league.0[i] * libm::exp(sd[i] * zi - 0.5 * sd[i] * sd[i]);
        }
        let sum: f64 = r.iter().sum();
        for v in &mut r {
            *v /= sum;
        }
        Rates::new(r).expect("normalised positive rates")
    }

    /// Runs per plate appearance above the league average, from the batter's
    /// side (positive is good for a batter, bad for a pitcher).
    #[must_use]
    pub fn value(&self, kind: Kind, z: &Z) -> f64 {
        let r = self.rates(kind, z);
        r.0.iter()
            .zip(self.league.0)
            .zip(LINEAR_WEIGHTS)
            .map(|((a, l), w)| (a - l) * w)
            .sum()
    }

    /// Positive is good for the player's own side.
    #[must_use]
    pub fn quality(&self, kind: Kind, z: &Z) -> f64 {
        match kind {
            Kind::Batter => self.value(kind, z),
            Kind::Pitcher => -self.value(kind, z),
        }
    }

    /// Standard deviation of quality over unit-variance talent draws.
    #[must_use]
    pub fn quality_sd(&self, kind: Kind) -> f64 {
        self.quality_sd[kind.index()]
    }

    /// z-vector change that lowers quality by `amount` (in quality units).
    #[must_use]
    pub fn decline(&self, kind: Kind, amount: f64) -> Z {
        let d = &self.dir[kind.index()];
        let scale = amount / self.quality_sd[kind.index()];
        let mut out = [0.0; K];
        for i in 0..K {
            out[i] = -scale * d[i];
        }
        out
    }

    /// Correlated unit-variance talent, from a random source.
    pub fn draw<R: RngCore>(&self, kind: Kind, rng: &mut R) -> Z {
        let mut e = [0.0; K];
        for v in &mut e {
            *v = normal(rng);
        }
        let l = &self.chol[kind.index()];
        let mut z = [0.0; K];
        for i in 0..K {
            for j in 0..=i {
                z[i] += l[i][j] * e[j];
            }
        }
        z
    }
}

/// Uniform in [0, 1).
pub fn uniform<R: RngCore>(rng: &mut R) -> f64 {
    (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

/// Standard normal (Box-Muller with libm, so it is the same on every platform).
pub fn normal<R: RngCore>(rng: &mut R) -> f64 {
    let u1 = uniform(rng).max(1e-300);
    let u2 = uniform(rng);
    libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(2.0 * core::f64::consts::PI * u2)
}
