/// How a plate appearance can end. The order is the index order of [`Rates`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Outcome {
    Strikeout = 0,
    Walk = 1,
    HitByPitch = 2,
    Single = 3,
    Double = 4,
    Triple = 5,
    HomeRun = 6,
    /// Ball in play that is not a hit: outs, force outs, double plays, errors.
    InPlayOut = 7,
}

pub const OUTCOMES: usize = 8;

impl Outcome {
    pub const ALL: [Outcome; OUTCOMES] = [
        Outcome::Strikeout,
        Outcome::Walk,
        Outcome::HitByPitch,
        Outcome::Single,
        Outcome::Double,
        Outcome::Triple,
        Outcome::HomeRun,
        Outcome::InPlayOut,
    ];

    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub fn is_hit(self) -> bool {
        matches!(
            self,
            Outcome::Single | Outcome::Double | Outcome::Triple | Outcome::HomeRun
        )
    }
}

/// Probability of each [`Outcome`] for one plate appearance. Sums to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates(pub [f64; OUTCOMES]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatesError {
    /// A rate is negative, infinite or not a number.
    BadValue,
    /// Rates do not sum to 1 within 1e-6.
    BadSum,
    /// A rate is zero, which log5 cannot divide by when it is the league rate.
    ZeroLeagueRate,
}

impl Rates {
    /// Rates that must sum to 1 (within 1e-6).
    pub fn new(values: [f64; OUTCOMES]) -> Result<Self, RatesError> {
        if values.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err(RatesError::BadValue);
        }
        let sum: f64 = values.iter().sum();
        if (sum - 1.0).abs() > 1e-6 {
            return Err(RatesError::BadSum);
        }
        Ok(Self(values))
    }

    /// Scale non-negative weights so they sum to 1.
    pub fn from_weights(weights: [f64; OUTCOMES]) -> Result<Self, RatesError> {
        if weights.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err(RatesError::BadValue);
        }
        let sum: f64 = weights.iter().sum();
        if sum <= 0.0 {
            return Err(RatesError::BadSum);
        }
        let mut out = weights;
        for v in &mut out {
            *v /= sum;
        }
        Ok(Self(out))
    }

    #[must_use]
    pub fn get(&self, outcome: Outcome) -> f64 {
        self.0[outcome.index()]
    }
}

/// Per-matchup combination of batter and pitcher against the league
/// (the generalised log5 method).
///
/// For each outcome the weight is `batter * pitcher / league`; the weights are
/// then scaled to sum to 1. A league-average batter against a league-average
/// pitcher gives exactly the league rates. Holds the inverse league rates so the
/// hot loop multiplies instead of divides.
#[derive(Debug, Clone, Copy)]
pub struct Log5 {
    inv_league: [f64; OUTCOMES],
}

impl Log5 {
    pub fn new(league: &Rates) -> Result<Self, RatesError> {
        let mut inv_league = [0.0; OUTCOMES];
        for (inv, l) in inv_league.iter_mut().zip(league.0) {
            if l <= 0.0 {
                return Err(RatesError::ZeroLeagueRate);
            }
            *inv = 1.0 / l;
        }
        Ok(Self { inv_league })
    }

    /// Cumulative odds for one matchup. The last entry is 1 up to rounding.
    /// No allocation: a fixed array on the stack.
    #[must_use]
    pub fn cumulative(&self, batter: &Rates, pitcher: &Rates) -> [f64; OUTCOMES] {
        let mut weights = [0.0; OUTCOMES];
        let mut total = 0.0;
        for (((w, b), p), inv) in weights
            .iter_mut()
            .zip(batter.0)
            .zip(pitcher.0)
            .zip(self.inv_league)
        {
            *w = b * p * inv;
            total += *w;
        }
        let scale = 1.0 / total;
        let mut running = 0.0;
        for w in &mut weights {
            running += *w * scale;
            *w = running;
        }
        weights
    }

    /// Per-outcome probabilities for one matchup (for tests and tools).
    #[must_use]
    pub fn probabilities(&self, batter: &Rates, pitcher: &Rates) -> Rates {
        let cum = self.cumulative(batter, pitcher);
        let mut out = [0.0; OUTCOMES];
        let mut prev = 0.0;
        for (o, c) in out.iter_mut().zip(cum) {
            *o = c - prev;
            prev = c;
        }
        Rates(out)
    }
}

/// Pick the outcome for a uniform draw `u` in `[0, 1)`.
#[must_use]
pub fn pick(cumulative: &[f64; OUTCOMES], u: f64) -> Outcome {
    for (i, c) in cumulative.iter().enumerate() {
        if u < *c {
            return Outcome::ALL[i];
        }
    }
    // Rounding can leave the last cumulative a hair under 1.
    Outcome::InPlayOut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn league() -> Rates {
        Rates::new([0.22, 0.085, 0.01, 0.15, 0.045, 0.004, 0.032, 0.454]).unwrap()
    }

    #[test]
    fn average_vs_average_is_the_league() {
        let l = league();
        let log5 = Log5::new(&l).unwrap();
        let p = log5.probabilities(&l, &l);
        for i in 0..OUTCOMES {
            assert!((p.0[i] - l.0[i]).abs() < 1e-12);
        }
    }

    #[test]
    fn matchup_sums_to_one_and_moves_toward_both_players() {
        let l = league();
        let log5 = Log5::new(&l).unwrap();
        // A power hitter and a strikeout pitcher.
        let batter =
            Rates::from_weights([0.20, 0.09, 0.01, 0.14, 0.045, 0.004, 0.06, 0.451]).unwrap();
        let pitcher =
            Rates::from_weights([0.28, 0.08, 0.01, 0.14, 0.04, 0.004, 0.03, 0.416]).unwrap();
        let p = log5.probabilities(&batter, &pitcher);
        assert!((p.0.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        // More strikeouts than a league hitter would have (pitcher), more
        // homers than the league (batter).
        assert!(p.get(Outcome::Strikeout) > batter.get(Outcome::Strikeout));
        assert!(p.get(Outcome::HomeRun) > l.get(Outcome::HomeRun));
    }

    #[test]
    fn known_two_outcome_log5_value() {
        // Classic log5 for a single event: batter .300, pitcher .250, league .250
        // gives .300 when the pitcher is average. Check the 8-way version
        // reproduces that for one outcome with the others scaled.
        let l = league();
        let log5 = Log5::new(&l).unwrap();
        let batter =
            Rates::from_weights([0.22, 0.085, 0.01, 0.18, 0.045, 0.004, 0.032, 0.424]).unwrap();
        let p = log5.probabilities(&batter, &l);
        // An average pitcher leaves the batter's own rates unchanged.
        for i in 0..OUTCOMES {
            assert!((p.0[i] - batter.0[i]).abs() < 1e-12);
        }
    }

    #[test]
    fn pick_covers_the_whole_unit_interval() {
        let l = league();
        let log5 = Log5::new(&l).unwrap();
        let cum = log5.cumulative(&l, &l);
        assert_eq!(pick(&cum, 0.0), Outcome::Strikeout);
        assert_eq!(pick(&cum, 0.999_999_9), Outcome::InPlayOut);
        assert_eq!(pick(&cum, 0.2), Outcome::Strikeout);
        assert_eq!(pick(&cum, 0.23), Outcome::Walk);
    }

    #[test]
    fn rates_reject_bad_input() {
        assert_eq!(Rates::new([0.5; OUTCOMES]).unwrap_err(), RatesError::BadSum);
        let mut v = [0.125; OUTCOMES];
        v[0] = -0.1;
        assert_eq!(Rates::new(v).unwrap_err(), RatesError::BadValue);
        let mut z = league().0;
        z[3] = 0.0;
        let zr = Rates(z);
        assert_eq!(Log5::new(&zr).unwrap_err(), RatesError::ZeroLeagueRate);
    }
}
