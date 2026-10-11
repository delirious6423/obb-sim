//! Loads `data/internal/params.json`, the league-wide aggregates produced by
//! `tools/build_params.py`. The file holds rates and spreads only: no player
//! id, name or stat line.

use std::path::Path;

use serde::Deserialize;
use sim_core::{Baserunning, Fielding, GameParams, OUTCOMES, Rates, RatesError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DataError {
    #[error("cannot read {path}: {source}. Run `python3 -I tools/build_params.py` first.")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("cannot parse params file: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid league rates: {0:?}")]
    Rates(RatesError),
}

/// The eight outcome rates keyed by name, in the file.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Eight {
    pub k: f64,
    pub bb: f64,
    pub hbp: f64,
    pub single: f64,
    pub double: f64,
    pub triple: f64,
    pub hr: f64,
    pub ipo: f64,
}

impl Eight {
    /// In [`sim_core::Outcome`] index order.
    #[must_use]
    pub fn array(&self) -> [f64; OUTCOMES] {
        [
            self.k,
            self.bb,
            self.hbp,
            self.single,
            self.double,
            self.triple,
            self.hr,
            self.ipo,
        ]
    }
}

#[derive(Debug, Clone, Deserialize)]
struct TalentFile {
    batter: Eight,
    pitcher: Eight,
}

#[derive(Debug, Clone, Deserialize)]
struct FieldingFile {
    ground: f64,
    fly: f64,
    line: f64,
    error_rate: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct BaserunningFile {
    single_r1_to_3: [f64; 3],
    single_r2_scores: [f64; 3],
    double_r1_scores: [f64; 3],
    dp_given_ground_r1: [f64; 2],
    ground_r3_scores: [f64; 2],
    ground_r2_to_3: [f64; 2],
    fly_r3_scores: [f64; 2],
    fly_r2_to_3: [f64; 2],
    sb2_attempt: f64,
    sb2_success: f64,
    sb3_attempt: f64,
    sb3_success: f64,
}

/// League run environment from the data file.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RunEnvironment {
    /// Runs per team-game as played (pitchers batting in part of the sample).
    pub runs_per_team_game: f64,
    /// The same with pitcher plate appearances replaced by average-batter ones,
    /// which is what a universal designated hitter looks like.
    pub runs_per_team_game_dh: f64,
    pub extra_inning_share: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct ParamsFile {
    source: String,
    pa: u64,
    league: Eight,
    talent: TalentFile,
    pitches_per_pa: Eight,
    fielding: FieldingFile,
    baserunning: BaserunningFile,
    run_environment: RunEnvironment,
}

/// True-talent spread: relative standard deviation of each outcome's rate
/// across players, in [`sim_core::Outcome`] order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Talent {
    pub batter_sd: [f64; OUTCOMES],
    pub pitcher_sd: [f64; OUTCOMES],
}

/// Everything calibration needs from the data file.
#[derive(Debug, Clone)]
pub struct Calibration {
    pub source: String,
    pub pa: u64,
    pub params: GameParams,
    pub talent: Talent,
    pub run_environment: RunEnvironment,
}

impl Calibration {
    pub fn load(path: &Path) -> Result<Self, DataError> {
        let text = std::fs::read_to_string(path).map_err(|source| DataError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_json(&text)
    }

    pub fn from_json(text: &str) -> Result<Self, DataError> {
        let f: ParamsFile = serde_json::from_str(text)?;
        let league = Rates::new(f.league.array()).map_err(DataError::Rates)?;
        let b = &f.baserunning;
        let params = GameParams {
            league,
            fielding: Fielding {
                ground: f.fielding.ground,
                fly: f.fielding.fly,
                line: f.fielding.line,
                error_rate: f.fielding.error_rate,
            },
            running: Baserunning {
                single_r1_to_3: b.single_r1_to_3,
                single_r2_scores: b.single_r2_scores,
                double_r1_scores: b.double_r1_scores,
                dp_given_ground_r1: b.dp_given_ground_r1,
                ground_r3_scores: b.ground_r3_scores,
                ground_r2_to_3: b.ground_r2_to_3,
                fly_r3_scores: b.fly_r3_scores,
                fly_r2_to_3: b.fly_r2_to_3,
                sb2_attempt: b.sb2_attempt,
                sb2_success: b.sb2_success,
                sb3_attempt: b.sb3_attempt,
                sb3_success: b.sb3_success,
            },
            pitches: f.pitches_per_pa.array(),
            // 2015-2019 rules: no automatic runner in extra innings.
            extra_inning_runner: false,
        };
        Ok(Self {
            source: f.source,
            pa: f.pa,
            params,
            run_environment: f.run_environment,
            talent: Talent {
                batter_sd: f.talent.batter.array(),
                pitcher_sd: f.talent.pitcher.array(),
            },
        })
    }
}
