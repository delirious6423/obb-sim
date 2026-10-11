//! The game loop: plate appearances, baserunning, fielding, relief pitching.
//!
//! One game is a pure function of its inputs and its RNG stream. The loop
//! allocates nothing per plate appearance: bases, counts and statistics are
//! fixed-size values on the stack.

use crate::events::{Ball, GameEvent, Half, PlayRecord, Sink};
use crate::params::Env;
use crate::rates::{Outcome, pick};
use crate::rng::GameRng;
use crate::stats::{GameOutcome, MAX_INNINGS};
use crate::team::{LINEUP, Team};

#[derive(Debug, Clone, Copy)]
struct Runner {
    /// Position in the batting order.
    slot: u8,
    /// The pitcher charged if this runner scores.
    pitcher: u8,
    earned: bool,
}

struct Sim<'a, S: Sink> {
    env: &'a Env,
    teams: [&'a Team; 2],
    rng: GameRng,
    out: GameOutcome,
    sink: &'a mut S,
    bases: [Option<Runner>; 3],
    outs: u8,
    inning: u8,
    /// Side at bat: 0 = away (top), 1 = home (bottom).
    bat: usize,
    next_batter: [u8; 2],
    pitcher: [u8; 2],
    next_reliever: [u8; 2],
    /// Runs and pitches already on the line when the current pitcher entered.
    outing_r0: [u8; 2],
    outing_p0: [u16; 2],
    /// Runs scored in the plate appearance being resolved.
    pa_runs: u8,
    /// The plate appearance being resolved is a home run (all runners score even
    /// when the winning run has already crossed).
    home_run_pa: bool,
}

/// Play one game and return its result and statistics.
///
/// `league_seed` and `game_id` pick the game's own RNG stream, so the result
/// does not depend on any other game. `away` bats first.
pub fn simulate_game<S: Sink>(
    env: &Env,
    away: &Team,
    home: &Team,
    league_seed: u64,
    game_id: u64,
    sink: &mut S,
) -> GameOutcome {
    let mut out = GameOutcome::new();
    out.pitch[0][0].used = true;
    out.pitch[1][0].used = true;
    let mut sim = Sim {
        env,
        teams: [away, home],
        rng: GameRng::for_game(league_seed, game_id),
        out,
        sink,
        bases: [None; 3],
        outs: 0,
        inning: 1,
        bat: 0,
        next_batter: [0; 2],
        pitcher: [0; 2],
        next_reliever: [1; 2],
        outing_r0: [0; 2],
        outing_p0: [0; 2],
        pa_runs: 0,
        home_run_pa: false,
    };
    sim.run();
    sim.sink.emit(GameEvent::GameOver);
    sim.out
}

impl<S: Sink> Sim<'_, S> {
    fn run(&mut self) {
        for inning in 1..=MAX_INNINGS {
            self.inning = inning as u8;
            self.out.innings = self.inning;
            self.bat = 0;
            self.play_half();
            // Home team ahead after the top of the 9th or later: no bottom half.
            if inning >= 9 && self.out.runs[1] > self.out.runs[0] {
                return;
            }
            self.bat = 1;
            self.out.home_batted_last = true;
            self.play_half();
            if inning >= 9 && self.out.runs[0] != self.out.runs[1] {
                return;
            }
        }
        // Still tied after MAX_INNINGS: a coin flip settles it.
        let side = usize::from(self.rng.next_u64() & 1 == 1);
        self.out.runs[side] += 1;
        self.out.line[side][MAX_INNINGS - 1] += 1;
    }

    fn half(&self) -> Half {
        if self.bat == 0 {
            Half::Top
        } else {
            Half::Bottom
        }
    }

    fn walk_off(&self) -> bool {
        self.bat == 1 && self.inning >= 9 && self.out.runs[1] > self.out.runs[0]
    }

    fn chance(&mut self, p: f64) -> bool {
        self.rng.next_f64() < p
    }

    fn play_half(&mut self) {
        self.bases = [None; 3];
        self.outs = 0;
        self.sink.emit(GameEvent::HalfInningStart {
            inning: self.inning,
            half: self.half(),
        });
        self.relief_check(true);
        if self.inning >= 10 && self.env.params.extra_inning_runner {
            // Runner on second to start every extra inning: the batter who
            // made the last out of the previous inning's order.
            let fld = 1 - self.bat;
            let slot = (self.next_batter[self.bat] + LINEUP as u8 - 1) % LINEUP as u8;
            self.bases[1] = Some(Runner {
                slot,
                pitcher: self.pitcher[fld],
                earned: false,
            });
            self.sink.emit(GameEvent::GhostRunner { runner: slot });
        }
        while self.outs < 3 {
            self.try_steal();
            if self.outs >= 3 {
                break;
            }
            self.plate_appearance();
            if self.walk_off() {
                break;
            }
            if self.outs < 3 {
                self.relief_check(false);
            }
        }
    }

    // ---- bookkeeping -------------------------------------------------

    fn score(&mut self, runner: Runner, earned_ok: bool) {
        let bat = self.bat;
        // Walk-off: play stops when the winning run crosses, so later runners
        // do not score, except on a home run.
        if self.walk_off() && !self.home_run_pa {
            return;
        }
        let fld = 1 - bat;
        self.out.runs[bat] += 1;
        let idx = usize::from(self.inning) - 1;
        self.out.line[bat][idx] = self.out.line[bat][idx].saturating_add(1);
        let b = &mut self.out.bat[bat][usize::from(runner.slot)];
        b.r = b.r.saturating_add(1);
        let p = &mut self.out.pitch[fld][usize::from(runner.pitcher)];
        p.r = p.r.saturating_add(1);
        if runner.earned && earned_ok {
            p.er = p.er.saturating_add(1);
        }
        self.pa_runs = self.pa_runs.saturating_add(1);
    }

    fn add_out(&mut self) {
        self.outs += 1;
        let fld = 1 - self.bat;
        let p = &mut self.out.pitch[fld][usize::from(self.pitcher[fld])];
        p.outs = p.outs.saturating_add(1);
    }

    /// The batter takes first base and any runner forced by that moves up.
    fn batter_to_first(&mut self, batter: Runner, earned_ok: bool) {
        if self.bases[0].is_some() {
            if self.bases[1].is_some() {
                if let Some(r) = self.bases[2].take() {
                    self.score(r, earned_ok);
                }
                self.bases[2] = self.bases[1].take();
            }
            self.bases[1] = self.bases[0].take();
        }
        self.bases[0] = Some(batter);
    }

    // ---- baserunning -------------------------------------------------

    fn try_steal(&mut self) {
        let run = self.env.params.running;
        let (from, attempt, success) = if self.bases[0].is_some() && self.bases[1].is_none() {
            (0, run.sb2_attempt, run.sb2_success)
        } else if self.bases[1].is_some() && self.bases[2].is_none() {
            (1, run.sb3_attempt, run.sb3_success)
        } else {
            return;
        };
        if !self.chance(attempt) {
            return;
        }
        let Some(runner) = self.bases[from].take() else {
            return;
        };
        let ok = self.chance(success);
        let line = &mut self.out.bat[self.bat][usize::from(runner.slot)];
        if ok {
            line.sb = line.sb.saturating_add(1);
            self.bases[from + 1] = Some(runner);
        } else {
            line.cs = line.cs.saturating_add(1);
            self.add_out();
        }
        self.sink.emit(GameEvent::Steal {
            runner: runner.slot,
            base: from as u8 + 2,
            success: ok,
        });
    }

    /// Runners (not the batter) move on a ground-ball out, after any forced moves.
    fn ground_out_runners(&mut self, o: usize, forced2: bool, forced3: bool) {
        let run = self.env.params.running;
        if !forced3
            && let Some(r) = self.bases[2]
            && self.chance(run.ground_r3_scores[o])
        {
            self.bases[2] = None;
            self.score(r, true);
        }
        if !forced2
            && self.bases[1].is_some()
            && self.bases[2].is_none()
            && self.chance(run.ground_r2_to_3[o])
        {
            self.bases[2] = self.bases[1].take();
        }
    }

    // ---- the plate appearance ---------------------------------------

    fn draw_pitches(&mut self, outcome: Outcome) -> u8 {
        let mean = self.env.params.pitches[outcome.index()];
        let base = libm::floor(mean);
        let extra = u32::from(self.chance(mean - base));
        (base as u32 + extra).min(60) as u8
    }

    fn plate_appearance(&mut self) {
        let bat = self.bat;
        let fld = 1 - bat;
        let slot = self.next_batter[bat];
        let pit = self.pitcher[fld];
        let teams = self.teams;
        debug_assert!(
            self.bases.iter().flatten().all(|r| r.slot != slot),
            "batter {slot} is also on base"
        );
        let batter_rates = &teams[bat].batters()[usize::from(slot)].rates;
        let pitcher_rates = &teams[fld].pitchers()[usize::from(pit)].rates;
        let cum = self.env.log5.cumulative(batter_rates, pitcher_rates);
        let outcome = pick(&cum, self.rng.next_f64());
        let pitches = self.draw_pitches(outcome);

        self.pa_runs = 0;
        self.home_run_pa = outcome == Outcome::HomeRun;
        let o = usize::from(self.outs);
        let run = self.env.params.running;
        let batter = Runner {
            slot,
            pitcher: pit,
            earned: true,
        };
        let mut ball = None;
        let mut error = false;
        let mut double_play = false;
        let mut sac_fly = false;

        match outcome {
            Outcome::Strikeout => self.add_out(),
            Outcome::Walk | Outcome::HitByPitch => self.batter_to_first(batter, true),
            Outcome::Single => {
                if let Some(r) = self.bases[2].take() {
                    self.score(r, true);
                }
                if let Some(r) = self.bases[1].take() {
                    if self.chance(run.single_r2_scores[o]) {
                        self.score(r, true);
                    } else {
                        self.bases[2] = Some(r);
                    }
                }
                if let Some(r) = self.bases[0].take() {
                    if self.bases[2].is_none() && self.chance(run.single_r1_to_3[o]) {
                        self.bases[2] = Some(r);
                    } else {
                        self.bases[1] = Some(r);
                    }
                }
                self.bases[0] = Some(batter);
            }
            Outcome::Double => {
                for base in [2, 1] {
                    if let Some(r) = self.bases[base].take() {
                        self.score(r, true);
                    }
                }
                if let Some(r) = self.bases[0].take() {
                    if self.chance(run.double_r1_scores[o]) {
                        self.score(r, true);
                    } else {
                        self.bases[2] = Some(r);
                    }
                }
                self.bases[1] = Some(batter);
            }
            Outcome::Triple => {
                for base in [2, 1, 0] {
                    if let Some(r) = self.bases[base].take() {
                        self.score(r, true);
                    }
                }
                self.bases[2] = Some(batter);
            }
            Outcome::HomeRun => {
                for base in [2, 1, 0] {
                    if let Some(r) = self.bases[base].take() {
                        self.score(r, true);
                    }
                }
                self.score(batter, true);
            }
            Outcome::InPlayOut => {
                let f = self.env.params.fielding;
                let ub = self.rng.next_f64();
                let kind = if ub < f.ground {
                    Ball::Ground
                } else if ub < f.ground + f.fly {
                    Ball::Fly
                } else {
                    Ball::Line
                };
                ball = Some(kind);
                error = self.chance(f.error_rate);
                if error {
                    // Reaches on an error: unearned for him and for runs forced in.
                    self.batter_to_first(
                        Runner {
                            earned: false,
                            ..batter
                        },
                        false,
                    );
                } else {
                    match kind {
                        Ball::Ground => {
                            if o < 2
                                && self.bases[0].is_some()
                                && self.chance(run.dp_given_ground_r1[o])
                            {
                                double_play = true;
                                self.bases[0] = None;
                                self.add_out();
                                self.add_out();
                                if self.outs < 3 {
                                    // The runner on second was forced up; with the
                                    // bases loaded the runner on third was forced home.
                                    if self.bases[1].is_some() {
                                        if let Some(r) = self.bases[2].take() {
                                            self.score(r, true);
                                        }
                                        self.bases[2] = self.bases[1].take();
                                    }
                                    self.ground_out_runners(o, true, false);
                                }
                            } else {
                                self.add_out();
                                if self.outs < 3 {
                                    let mut forced2 = false;
                                    let mut forced3 = false;
                                    if self.bases[0].is_some() {
                                        forced2 = true;
                                        if self.bases[1].is_some() {
                                            forced3 = true;
                                            if let Some(r) = self.bases[2].take() {
                                                self.score(r, true);
                                            }
                                            self.bases[2] = self.bases[1].take();
                                        }
                                        self.bases[1] = self.bases[0].take();
                                    }
                                    self.ground_out_runners(o, forced2, forced3);
                                }
                            }
                        }
                        Ball::Fly => {
                            self.add_out();
                            if self.outs < 3 {
                                if let Some(r) = self.bases[2]
                                    && self.chance(run.fly_r3_scores[o])
                                {
                                    self.bases[2] = None;
                                    self.score(r, true);
                                    sac_fly = true;
                                }
                                if self.bases[1].is_some()
                                    && self.bases[2].is_none()
                                    && self.chance(run.fly_r2_to_3[o])
                                {
                                    self.bases[2] = self.bases[1].take();
                                }
                            }
                        }
                        Ball::Line => self.add_out(),
                    }
                }
            }
        }

        self.record_pa(
            slot,
            pit,
            outcome,
            pitches,
            error,
            double_play,
            sac_fly,
            ball,
        );
        self.next_batter[bat] = (slot + 1) % LINEUP as u8;
    }

    #[allow(clippy::too_many_arguments)]
    fn record_pa(
        &mut self,
        slot: u8,
        pit: u8,
        outcome: Outcome,
        pitches: u8,
        error: bool,
        double_play: bool,
        sac_fly: bool,
        ball: Option<Ball>,
    ) {
        let bat = self.bat;
        let fld = 1 - bat;
        let rbi = if error || double_play {
            0
        } else {
            self.pa_runs
        };

        let b = &mut self.out.bat[bat][usize::from(slot)];
        b.pa += 1;
        b.rbi = b.rbi.saturating_add(rbi);
        match outcome {
            Outcome::Strikeout => {
                b.ab += 1;
                b.so += 1;
            }
            Outcome::Walk => b.bb += 1,
            Outcome::HitByPitch => b.hbp += 1,
            Outcome::Single => {
                b.ab += 1;
                b.h += 1;
            }
            Outcome::Double => {
                b.ab += 1;
                b.h += 1;
                b.doubles += 1;
            }
            Outcome::Triple => {
                b.ab += 1;
                b.h += 1;
                b.triples += 1;
            }
            Outcome::HomeRun => {
                b.ab += 1;
                b.h += 1;
                b.hr += 1;
            }
            Outcome::InPlayOut => {
                if sac_fly {
                    b.sf += 1;
                } else {
                    b.ab += 1;
                }
            }
        }

        let p = &mut self.out.pitch[fld][usize::from(pit)];
        p.bf += 1;
        p.pitches += u16::from(pitches);
        match outcome {
            Outcome::Strikeout => p.so += 1,
            Outcome::Walk => p.bb += 1,
            Outcome::HitByPitch => p.hbp += 1,
            Outcome::HomeRun => {
                p.h += 1;
                p.hr += 1;
            }
            Outcome::Single | Outcome::Double | Outcome::Triple => p.h += 1,
            Outcome::InPlayOut => {}
        }
        if error {
            self.out.errors[fld] = self.out.errors[fld].saturating_add(1);
        }

        self.sink.emit(GameEvent::Pa(PlayRecord {
            batter: slot,
            pitcher: pit,
            outcome,
            ball,
            error,
            double_play,
            sac_fly,
            runs: self.pa_runs,
            outs_after: self.outs,
            pitches,
        }));
    }

    // ---- pitching changes --------------------------------------------

    /// Simple relief rule, checked at the start of each half-inning and
    /// (with higher limits) between batters.
    ///
    /// - The closer, the last pitcher listed, enters in the 9th or later with a
    ///   lead of 1 to 3 runs.
    /// - A starter leaves after 100 pitches or 6 runs at a half-inning start
    ///   (115 or 8 mid-inning). A reliever leaves after 30 pitches or 3 runs
    ///   (45 or 5 mid-inning). The next reliever in the bullpen order comes in.
    fn relief_check(&mut self, half_start: bool) {
        let fld = 1 - self.bat;
        let count = self.teams[fld].pitchers().len();
        if count < 2 {
            return;
        }
        let cur = usize::from(self.pitcher[fld]);
        let line = self.out.pitch[fld][cur];
        let outing_runs = line.r.saturating_sub(self.outing_r0[fld]);
        let outing_pitches = line.pitches.saturating_sub(self.outing_p0[fld]);
        let last = count - 1;

        let lead = i32::from(self.out.runs[fld]) - i32::from(self.out.runs[self.bat]);
        if half_start
            && self.inning >= 9
            && (1..=3).contains(&lead)
            && cur != last
            && !self.out.pitch[fld][last].used
        {
            self.bring_in(fld, last);
            return;
        }

        let (pitch_limit, run_limit) = match (cur == 0, half_start) {
            (true, true) => (100, 6),
            (true, false) => (115, 8),
            (false, true) => (30, 3),
            (false, false) => (45, 5),
        };
        if (outing_pitches >= pitch_limit || outing_runs >= run_limit)
            && let Some(next) = self.pick_reliever(fld, last)
        {
            self.bring_in(fld, next);
        }
    }

    fn pick_reliever(&mut self, fld: usize, last: usize) -> Option<usize> {
        let next = usize::from(self.next_reliever[fld]);
        if next < last {
            self.next_reliever[fld] += 1;
            return Some(next);
        }
        if last > 0 && !self.out.pitch[fld][last].used {
            return Some(last);
        }
        None
    }

    fn bring_in(&mut self, fld: usize, index: usize) {
        self.pitcher[fld] = index as u8;
        self.out.pitch[fld][index].used = true;
        self.outing_r0[fld] = self.out.pitch[fld][index].r;
        self.outing_p0[fld] = self.out.pitch[fld][index].pitches;
        self.sink.emit(GameEvent::PitcherChange {
            half: self.half(),
            pitcher: index as u8,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{NullSink, PlayLog};
    use crate::params::GameParams;
    use crate::rates::Rates;
    use crate::team::Player;

    fn env() -> Env {
        Env::new(GameParams::reference()).unwrap()
    }

    fn avg_team(name: &str) -> Team {
        let league = GameParams::reference().league;
        let batters = (0..9)
            .map(|i| Player {
                name: format!("{name} Batter {}", i + 1),
                rates: league,
            })
            .collect();
        let pitchers = (0..7)
            .map(|i| Player {
                name: format!("{name} Pitcher {}", i + 1),
                rates: league,
            })
            .collect();
        Team::new(name, batters, pitchers).unwrap()
    }

    #[test]
    fn same_inputs_give_the_same_game() {
        let (env, a, h) = (env(), avg_team("Away"), avg_team("Home"));
        let first = simulate_game(&env, &a, &h, 99, 5, &mut NullSink);
        for _ in 0..20 {
            assert_eq!(simulate_game(&env, &a, &h, 99, 5, &mut NullSink), first);
        }
        assert_ne!(simulate_game(&env, &a, &h, 99, 6, &mut NullSink), first);
    }

    #[test]
    fn game_does_not_depend_on_the_sink() {
        let (env, a, h) = (env(), avg_team("Away"), avg_team("Home"));
        let mut log = PlayLog::default();
        let with_log = simulate_game(&env, &a, &h, 7, 3, &mut log);
        let without = simulate_game(&env, &a, &h, 7, 3, &mut NullSink);
        assert_eq!(with_log, without);
        assert!(matches!(log.events.last(), Some(GameEvent::GameOver)));
    }

    /// Teams whose hitters get on base a lot, so extra innings with traffic
    /// and walk-offs are common.
    fn hitting_team(name: &str, k: f64) -> Team {
        // order: K, BB, HBP, 1B, 2B, 3B, HR, in-play out
        let rates = Rates::new([k, 0.15, 0.02, 0.25, 0.08, 0.01, 0.04, 0.45 - k]).unwrap();
        let mk = |i: usize, who: &str| Player {
            name: format!("{name} {who} {i}"),
            rates,
        };
        Team::new(
            name,
            (0..9).map(|i| mk(i, "B")).collect(),
            (0..7).map(|i| mk(i, "P")).collect(),
        )
        .unwrap()
    }

    #[test]
    fn walk_off_ends_the_game_on_the_winning_run() {
        let env = env();
        let (a, h) = (hitting_team("Away", 0.2), hitting_team("Home", 0.2));
        let mut walk_offs = 0;
        for g in 0..3000 {
            let mut log = PlayLog::default();
            let o = simulate_game(&env, &a, &h, 77, g, &mut log);
            let last = log.events.iter().rev().find_map(|e| match e {
                GameEvent::Pa(p) => Some(*p),
                _ => None,
            });
            let last_half = log.events.iter().rev().find_map(|e| match e {
                GameEvent::HalfInningStart { half, .. } => Some(*half),
                _ => None,
            });
            let Some(p) = last else { continue };
            if o.runs[1] > o.runs[0] && p.outs_after < 3 && last_half == Some(Half::Bottom) {
                walk_offs += 1;
                if p.outcome != Outcome::HomeRun {
                    assert_eq!(o.runs[1] - o.runs[0], 1, "game {g}: runs past the winner");
                }
            }
        }
        assert!(walk_offs > 100, "test needs walk-offs, saw {walk_offs}");
    }

    /// The automatic extra-inning runner is the previous batter; he must never
    /// come to bat while still on base (checked by a debug assertion in the
    /// plate-appearance code) however much traffic there is.
    #[test]
    fn ghost_runner_is_never_at_bat_while_on_base() {
        let env = env();
        let mut extras = 0;
        for (i, k) in [0.05, 0.2, 0.3].into_iter().enumerate() {
            let (a, h) = (hitting_team("Away", k), hitting_team("Home", k));
            for g in 0..8_000 {
                let o = simulate_game(&env, &a, &h, 5 + i as u64, g, &mut NullSink);
                if o.innings > 9 {
                    extras += 1;
                }
            }
        }
        assert!(extras > 100, "needs many extra-inning games, saw {extras}");
    }

    /// Whole-game bookkeeping must add up, game after game.
    #[test]
    fn smoke_full_games_are_consistent() {
        let (env, a, h) = (env(), avg_team("Away"), avg_team("Home"));
        let mut extra_innings = 0;
        let mut walk_offs = 0;
        for g in 0..400 {
            let o = simulate_game(&env, &a, &h, 1234, g, &mut NullSink);
            assert_ne!(o.runs[0], o.runs[1], "no ties (game {g})");
            assert!(o.innings >= 9);
            for side in 0..2 {
                let line_sum: u16 = o.line[side].iter().map(|r| u16::from(*r)).sum();
                assert_eq!(line_sum, o.runs[side], "line score (game {g})");
                let batter_runs: u16 = o.bat[side].iter().map(|b| u16::from(b.r)).sum();
                assert_eq!(batter_runs, o.runs[side], "R column (game {g})");
                let charged: u16 = o.pitch[1 - side].iter().map(|p| u16::from(p.r)).sum();
                assert_eq!(charged, o.runs[side], "runs charged to pitchers (game {g})");
                let pa: u16 = o.bat[side].iter().map(|b| u16::from(b.pa)).sum();
                let bf: u16 = o.pitch[1 - side].iter().map(|p| u16::from(p.bf)).sum();
                assert_eq!(pa, bf, "PA == BF (game {g})");
            }
            // The away team always bats a full number of innings.
            let away_bat_outs: u16 = o.pitch[1].iter().map(|p| u16::from(p.outs)).sum();
            assert_eq!(away_bat_outs, 3 * u16::from(o.innings), "game {g}");
            if !o.home_batted_last {
                assert!(o.home_won(), "home skips the bottom only when ahead");
            }
            if o.innings > 9 {
                extra_innings += 1;
            }
            if o.home_won() && o.home_batted_last {
                let home_bat_outs: u16 = o.pitch[0].iter().map(|p| u16::from(p.outs)).sum();
                if home_bat_outs < 3 * u16::from(o.innings) {
                    walk_offs += 1;
                }
            }
        }
        assert!(extra_innings > 0, "400 games should include extra innings");
        assert!(walk_offs > 0, "400 games should include a walk-off");
    }

    #[test]
    fn extra_innings_start_with_a_runner_on_second() {
        let (env, a, h) = (env(), avg_team("Away"), avg_team("Home"));
        for g in 0..2000 {
            let mut log = PlayLog::default();
            let o = simulate_game(&env, &a, &h, 55, g, &mut log);
            if o.innings > 9 {
                let ghosts = log
                    .events
                    .iter()
                    .filter(|e| matches!(e, GameEvent::GhostRunner { .. }))
                    .count();
                // One per half-inning from the 10th on that was played.
                let halves = usize::from(o.innings - 9) * 2 - usize::from(!o.home_batted_last);
                assert_eq!(ghosts, halves);
                return;
            }
        }
        panic!("no extra-inning game in 2000 tries");
    }

    #[test]
    fn reference_environment_scores_a_plausible_number_of_runs() {
        let (env, a, h) = (env(), avg_team("Away"), avg_team("Home"));
        let games = 3000;
        let total: u32 = (0..games)
            .map(|g| {
                let o = simulate_game(&env, &a, &h, 8, g, &mut NullSink);
                u32::from(o.runs[0]) + u32::from(o.runs[1])
            })
            .sum();
        let per_team = f64::from(total) / (2.0 * games as f64);
        assert!((3.5..5.8).contains(&per_team), "{per_team} runs/team/game");
    }

    #[test]
    fn league_average_teams_reproduce_league_rates() {
        let params = GameParams::reference();
        let (env, a, h) = (env(), avg_team("Away"), avg_team("Home"));
        let (mut pa, mut so, mut bb, mut hr) = (0u64, 0u64, 0u64, 0u64);
        for g in 0..3000 {
            let o = simulate_game(&env, &a, &h, 21, g, &mut NullSink);
            for side in 0..2 {
                for b in &o.bat[side] {
                    pa += u64::from(b.pa);
                    so += u64::from(b.so);
                    bb += u64::from(b.bb);
                    hr += u64::from(b.hr);
                }
            }
        }
        let rel = |sim: u64, target: f64| (sim as f64 / pa as f64 - target) / target;
        assert!(rel(so, params.league.0[0]).abs() < 0.03);
        assert!(rel(bb, params.league.0[1]).abs() < 0.03);
        assert!(rel(hr, params.league.0[6]).abs() < 0.03);
    }

    #[test]
    fn stronger_batters_score_more() {
        let params = GameParams::reference();
        let slugger = Rates::from_weights([
            0.20,
            0.10,
            0.01,
            0.155,
            0.055,
            0.005,
            0.055,
            params.league.0[7],
        ])
        .unwrap();
        let mut power = avg_team("Power");
        power = Team::new(
            "Power",
            power
                .batters()
                .iter()
                .map(|p| Player {
                    name: p.name.clone(),
                    rates: slugger,
                })
                .collect(),
            power.pitchers().to_vec(),
        )
        .unwrap();
        let avg = avg_team("Avg");
        let env = env();
        let (mut power_runs, mut avg_runs) = (0u32, 0u32);
        for g in 0..1500 {
            let o = simulate_game(&env, &power, &avg, 3, g, &mut NullSink);
            power_runs += u32::from(o.runs[0]);
            avg_runs += u32::from(o.runs[1]);
        }
        assert!(power_runs > avg_runs + 500, "{power_runs} vs {avg_runs}");
    }
}
