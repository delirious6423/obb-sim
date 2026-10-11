use crate::rates::{Log5, OUTCOMES, Rates, RatesError};

/// What happens to in-play outs. Shares of ground / fly / line sum to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fielding {
    pub ground: f64,
    pub fly: f64,
    pub line: f64,
    /// Chance an in-play out is instead an error that lets the batter reach.
    pub error_rate: f64,
}

/// Baserunning odds, by number of outs where it matters (index = outs).
/// Probabilities, each in `[0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Baserunning {
    /// Single, runner on 1st reaches 3rd (or scores) instead of stopping at 2nd.
    pub single_r1_to_3: [f64; 3],
    /// Single, runner on 2nd scores instead of stopping at 3rd.
    pub single_r2_scores: [f64; 3],
    /// Double, runner on 1st scores instead of stopping at 3rd.
    pub double_r1_scores: [f64; 3],
    /// Ground-ball out with a runner on 1st: turned into a double play (0 or 1 out).
    pub dp_given_ground_r1: [f64; 2],
    /// Ground-ball out, runner on 3rd scores (0 or 1 out).
    pub ground_r3_scores: [f64; 2],
    /// Ground-ball out, runner on 2nd moves up to 3rd when it is open.
    pub ground_r2_to_3: [f64; 2],
    /// Fly-ball out, runner on 3rd scores: the sacrifice fly.
    pub fly_r3_scores: [f64; 2],
    /// Fly-ball out, runner on 2nd tags up to 3rd when it is open.
    pub fly_r2_to_3: [f64; 2],
    /// Per plate appearance with a runner on 1st and 2nd open.
    pub sb2_attempt: f64,
    pub sb2_success: f64,
    /// Per plate appearance with a runner on 2nd and 3rd open.
    pub sb3_attempt: f64,
    pub sb3_success: f64,
}

/// Everything about the run environment that is not a player.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameParams {
    pub league: Rates,
    pub fielding: Fielding,
    pub running: Baserunning,
    /// Mean pitches thrown in a plate appearance, by outcome.
    pub pitches: [f64; OUTCOMES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamsError {
    Rates(RatesError),
    /// A probability is outside `[0, 1]` or not finite.
    BadProbability,
    /// Fielding shares do not sum to 1.
    BadFieldingShares,
    /// A mean pitch count is not positive.
    BadPitches,
}

impl From<RatesError> for ParamsError {
    fn from(e: RatesError) -> Self {
        Self::Rates(e)
    }
}

impl GameParams {
    /// Round, generic baseball-like numbers for tests and demos. They are
    /// typical of modern major-league play but are NOT derived from any data
    /// set; calibrated parameters come from the git-ignored calibration step.
    #[must_use]
    pub fn reference() -> Self {
        let sum_known = 0.222 + 0.085 + 0.010 + 0.150 + 0.045 + 0.004 + 0.032;
        Self {
            league: Rates([
                0.222,
                0.085,
                0.010,
                0.150,
                0.045,
                0.004,
                0.032,
                1.0 - sum_known,
            ]),
            fielding: Fielding {
                ground: 0.46,
                fly: 0.38,
                line: 0.16,
                error_rate: 0.018,
            },
            running: Baserunning {
                single_r1_to_3: [0.28, 0.28, 0.40],
                single_r2_scores: [0.55, 0.60, 0.85],
                double_r1_scores: [0.40, 0.45, 0.85],
                dp_given_ground_r1: [0.45, 0.45],
                ground_r3_scores: [0.45, 0.50],
                ground_r2_to_3: [0.25, 0.20],
                fly_r3_scores: [0.55, 0.60],
                fly_r2_to_3: [0.20, 0.20],
                sb2_attempt: 0.08,
                sb2_success: 0.75,
                sb3_attempt: 0.02,
                sb3_success: 0.70,
            },
            pitches: [4.8, 5.6, 3.3, 3.4, 3.5, 3.6, 3.5, 3.4],
        }
    }

    pub fn validate(&self) -> Result<(), ParamsError> {
        Rates::new(self.league.0)?;
        let f = &self.fielding;
        let shares = f.ground + f.fly + f.line;
        if [f.ground, f.fly, f.line]
            .iter()
            .any(|v| !(0.0..=1.0).contains(v))
            || (shares - 1.0).abs() > 1e-6
        {
            return Err(ParamsError::BadFieldingShares);
        }
        let r = &self.running;
        let probs = r
            .single_r1_to_3
            .iter()
            .chain(&r.single_r2_scores)
            .chain(&r.double_r1_scores)
            .chain(&r.dp_given_ground_r1)
            .chain(&r.ground_r3_scores)
            .chain(&r.ground_r2_to_3)
            .chain(&r.fly_r3_scores)
            .chain(&r.fly_r2_to_3)
            .chain([
                &f.error_rate,
                &r.sb2_attempt,
                &r.sb2_success,
                &r.sb3_attempt,
                &r.sb3_success,
            ]);
        for p in probs {
            if !p.is_finite() || !(0.0..=1.0).contains(p) {
                return Err(ParamsError::BadProbability);
            }
        }
        if self.pitches.iter().any(|p| !p.is_finite() || *p <= 0.0) {
            return Err(ParamsError::BadPitches);
        }
        Ok(())
    }
}

/// [`GameParams`] plus the precomputed log5 table. Build once, reuse for every game.
#[derive(Debug, Clone, Copy)]
pub struct Env {
    pub params: GameParams,
    pub(crate) log5: Log5,
}

impl Env {
    pub fn new(params: GameParams) -> Result<Self, ParamsError> {
        params.validate()?;
        let log5 = Log5::new(&params.league)?;
        Ok(Self { params, log5 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_params_are_valid() {
        GameParams::reference().validate().unwrap();
    }

    #[test]
    fn bad_params_are_refused() {
        let mut p = GameParams::reference();
        p.running.sb2_success = 1.5;
        assert_eq!(p.validate(), Err(ParamsError::BadProbability));
        let mut p = GameParams::reference();
        p.fielding.fly = 0.9;
        assert_eq!(p.validate(), Err(ParamsError::BadFieldingShares));
        let mut p = GameParams::reference();
        p.pitches[0] = 0.0;
        assert_eq!(p.validate(), Err(ParamsError::BadPitches));
    }
}
