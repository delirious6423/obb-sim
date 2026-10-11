//! Simulate many games between generated teams and compare rates to targets.

use std::fmt::Write as _;

use sim_core::{Env, GameOutcome, NullSink, OUTCOMES, Outcome, Team, simulate_game};

use crate::data::Calibration;
use crate::roster::{ROTATION, generate, teams_by_starter};

pub const TEAMS: usize = 30;

/// Plate-appearance outcome counts and a few run-environment totals.
#[derive(Debug, Clone, Copy, Default)]
pub struct Totals {
    pub games: u64,
    pub pa: u64,
    pub counts: [u64; OUTCOMES],
    pub runs: u64,
    pub sb: u64,
    pub cs: u64,
    pub errors: u64,
    pub extra_inning_games: u64,
}

impl Totals {
    fn add(&mut self, g: &GameOutcome) {
        self.games += 1;
        self.runs += u64::from(g.runs[0]) + u64::from(g.runs[1]);
        self.errors += u64::from(g.errors[0]) + u64::from(g.errors[1]);
        if g.innings > 9 {
            self.extra_inning_games += 1;
        }
        for b in g.bat.iter().flatten() {
            let pa = u64::from(b.pa);
            self.pa += pa;
            let hits_other =
                u64::from(b.h) - u64::from(b.doubles) - u64::from(b.triples) - u64::from(b.hr);
            self.counts[Outcome::Strikeout.index()] += u64::from(b.so);
            self.counts[Outcome::Walk.index()] += u64::from(b.bb);
            self.counts[Outcome::HitByPitch.index()] += u64::from(b.hbp);
            self.counts[Outcome::Single.index()] += hits_other;
            self.counts[Outcome::Double.index()] += u64::from(b.doubles);
            self.counts[Outcome::Triple.index()] += u64::from(b.triples);
            self.counts[Outcome::HomeRun.index()] += u64::from(b.hr);
            self.sb += u64::from(b.sb);
            self.cs += u64::from(b.cs);
        }
        let known: u64 = self.counts[..Outcome::InPlayOut.index()].iter().sum();
        self.counts[Outcome::InPlayOut.index()] = self.pa - known;
    }

    #[must_use]
    pub fn rate(&self, outcome: Outcome) -> f64 {
        self.counts[outcome.index()] as f64 / self.pa as f64
    }
}

/// Play `games` games between random pairs of generated teams (random starters).
/// Deterministic in `seed`.
#[must_use]
pub fn simulate(cal: &Calibration, games: u64, seed: u64) -> Totals {
    let env = Env::new(cal.params).expect("calibration parameters are valid");
    let specs = generate(&cal.params.league, &cal.talent, TEAMS, seed);
    let teams: Vec<Vec<Team>> = specs.iter().map(teams_by_starter).collect();

    // Schedule choices come from a small LCG of our own, kept apart from the
    // game streams so choosing opponents never touches the dice.
    let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
    let mut next = move |bound: usize| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((state >> 33) as usize) % bound
    };

    let mut totals = Totals::default();
    for g in 0..games {
        let away = next(TEAMS);
        let home = (away + 1 + next(TEAMS - 1)) % TEAMS;
        let a = &teams[away][next(ROTATION)];
        let h = &teams[home][next(ROTATION)];
        let outcome = simulate_game(&env, a, h, seed, g, &mut NullSink);
        totals.add(&outcome);
    }
    totals
}

/// A comparison of simulated and target rates.
#[derive(Debug, Clone)]
pub struct Report {
    pub totals: Totals,
    pub target: [f64; OUTCOMES],
    /// Outcomes that must be within `tolerance` (relative) for the gate.
    pub gated: Vec<Outcome>,
    pub tolerance: f64,
    /// Target runs per team-game (designated-hitter equivalent), gated too.
    pub runs_target: f64,
    pub extra_inning_target: f64,
}

impl Report {
    #[must_use]
    pub fn new(cal: &Calibration, totals: Totals, gated: Vec<Outcome>, tolerance: f64) -> Self {
        Self {
            totals,
            target: cal.params.league.0,
            gated,
            tolerance,
            runs_target: cal.run_environment.runs_per_team_game_dh,
            extra_inning_target: cal.run_environment.extra_inning_share,
        }
    }

    #[must_use]
    pub fn runs_per_team_game(&self) -> f64 {
        self.totals.runs as f64 / (2.0 * self.totals.games as f64)
    }

    #[must_use]
    pub fn runs_relative_error(&self) -> f64 {
        self.runs_per_team_game() / self.runs_target - 1.0
    }

    #[must_use]
    pub fn relative_error(&self, outcome: Outcome) -> f64 {
        self.totals.rate(outcome) / self.target[outcome.index()] - 1.0
    }

    #[must_use]
    pub fn passed(&self) -> bool {
        self.runs_relative_error().abs() <= self.tolerance
            && self
                .gated
                .iter()
                .all(|o| self.relative_error(*o).abs() <= self.tolerance)
    }

    #[must_use]
    pub fn table(&self) -> String {
        const NAMES: [&str; OUTCOMES] = [
            "K%",
            "BB%",
            "HBP%",
            "1B%",
            "2B%",
            "3B%",
            "HR/PA",
            "In-play out%",
        ];
        let mut s = String::new();
        let t = &self.totals;
        let _ = writeln!(s, "{} games, {} plate appearances", t.games, t.pa);
        let _ = writeln!(
            s,
            "{:<14} {:>10} {:>10} {:>9}  gate (+-3%)",
            "rate", "simulated", "target", "rel err"
        );
        for o in Outcome::ALL {
            let gated = self.gated.contains(&o);
            let err = self.relative_error(o);
            let verdict = if !gated {
                "-"
            } else if err.abs() <= self.tolerance {
                "PASS"
            } else {
                "FAIL"
            };
            let _ = writeln!(
                s,
                "{:<14} {:>9.4}% {:>9.4}% {:>+8.2}%  {}",
                NAMES[o.index()],
                100.0 * t.rate(o),
                100.0 * self.target[o.index()],
                100.0 * err,
                verdict
            );
        }
        let runs_verdict = if self.runs_relative_error().abs() <= self.tolerance {
            "PASS"
        } else {
            "FAIL"
        };
        let _ = writeln!(
            s,
            "{:<14} {:>10.3} {:>10.3} {:>+8.2}%  {}",
            "runs/team-game",
            self.runs_per_team_game(),
            self.runs_target,
            100.0 * self.runs_relative_error(),
            runs_verdict
        );
        let _ = writeln!(
            s,
            "extra-inning games {:.1}% (target {:.1}%, not gated)",
            100.0 * t.extra_inning_games as f64 / t.games as f64,
            100.0 * self.extra_inning_target
        );
        let _ = writeln!(
            s,
            "steals {} / caught {} ({:.0}% success), errors {}",
            t.sb,
            t.cs,
            100.0 * t.sb as f64 / (t.sb + t.cs).max(1) as f64,
            t.errors
        );
        s
    }
}
