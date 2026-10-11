//! Phase 7 exit gate: any role, human or AI, over fifty seasons on five seeds
//! must not produce runaway payrolls or insolvent clubs. Ignored by default
//! (it needs the git-ignored `data/internal/params.json` and runs six role
//! mixes x five seeds x fifty seasons on threads):
//!
//!     cargo test --release -p connect --test roles_gate -- --ignored --nocapture
//!
//! `roles_smoke` runs the same six mixes on the generic model, two seeds and
//! six seasons, with the same checks, so CI exercises the code.
//!
//! Role mixes (human seats are scripted policies that implement the traits):
//!   1. all built-in AI;
//!   2. human commissioner 'hands-off' (never sets rules) + AI owners;
//!   3. adversarial commissioner (each offseason every rule is drawn at one of
//!      its extremes that `Rules::check` allows) + AI owners;
//!   4. reckless owners on every club (budget at the cap, ticket 12 or 150
//!      alternately, 200 on the ballpark every year, stance flipping) + AI
//!      commissioner;
//!   5. frugal owners on every club (budget at the floor, default ticket, no
//!      building) + AI commissioner. Not reckless, so held to the strict
//!      non-reckless criteria below;
//!   6. one reckless club (club 0), 29 AI owners, adversarial commissioner.
//!
//! Built-in GMs and field managers fill every mix.
//!
//! Measurement: every season, from the first (no warm-up exclusion). A warm-up
//! would only hide early violations, and no criterion needs a steady state.
//! Insolvency is read from the `FinancesSettled` events of each season (cash
//! after the season's settlement). Payroll is `World::payroll` at the end of
//! each year (the cap in force for the coming season is `rules.payroll_cap`).
//!
//! Pass criteria, fixed before the first run (not tuned to results):
//! (a) payroll: every non-reckless club-season has payroll <= 1.25 x the cap
//!     in force. A reckless club-season has payroll <= 1.5 x. Reasoning:
//!     `decide.rs` refuses signings above the cap, so payroll can only pass
//!     the cap when the commissioner lowers it under contracts signed earlier.
//!     Allowing 25% (50% for reckless owners) covers that carry-over while
//!     still catching a runaway.
//! (b) insolvency: no non-reckless club ever has cash below 0 at a season
//!     settlement (that covers case 6's 29 AI clubs and case 5). No club, in
//!     any mix, has cash below -CREDIT, at settlement or at year end. The
//!     engine enforces -CREDIT for discretionary spending only, so settlement
//!     losses are the case that can break it. The league completes every
//!     season with every club's roster legal (`World::roster_problems` empty)
//!     in the reckless mixes (4, 6). Roster legality is reported for the other
//!     mixes but not enforced.
//! (c) every run completes all seasons with no `WorldError` from
//!     `run_year_with`, and the settlement count equals the season count
//!     (a guard against a vacuous pass).
//!
//! A failing case is reported with its numbers; production code is not
//! changed to make it pass.

use std::path::Path;

use calibration::Calibration;
use connect::{
    BuiltInCommissioner, BuiltInGm, BuiltInManager, BuiltInOwner, CommissionerAction,
    CommissionerAgent, CommissionerObs, OwnerAction, OwnerAgent, Seats, run_year_with, start_with,
};
use sim_core::GameParams;
use world::agents::{LeagueObs, OwnerObs};
use world::command::Event;
use world::player::DEFAULT_POOL;
use world::rating::Model;
use world::state::{CREDIT, Rules, Stance, TEAMS, TICKET_RANGE, TICKET_REF, World};

const SEASONS: usize = 50;
const BURN_IN: u8 = 30;
const SEEDS: [u64; 5] = [20_250_101, 1, 2, 3, 4];

const AI_CAP_RATIO: f64 = 1.25;
const RECKLESS_CAP_RATIO: f64 = 1.5;
const RECKLESS_BUDGET_ALL_CAP: f64 = 200.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mix {
    AllAi,
    HandsOff,
    Adversary,
    AllReckless,
    Frugal,
    OneReckless,
}

const MIXES: [Mix; 6] = [
    Mix::AllAi,
    Mix::HandsOff,
    Mix::Adversary,
    Mix::AllReckless,
    Mix::Frugal,
    Mix::OneReckless,
];

impl Mix {
    fn number(self) -> u8 {
        match self {
            Mix::AllAi => 1,
            Mix::HandsOff => 2,
            Mix::Adversary => 3,
            Mix::AllReckless => 4,
            Mix::Frugal => 5,
            Mix::OneReckless => 6,
        }
    }
}

/// Whether club `t` is run by a reckless owner in this mix.
fn reckless(mix: Mix, t: u8) -> bool {
    match mix {
        Mix::AllReckless => true,
        Mix::OneReckless => t == 0,
        _ => false,
    }
}

/// Human commissioner who never changes rules. The calendar is mechanical, so
/// it follows the league office's calendar.
struct HandsOff {
    calendar: BuiltInCommissioner,
}

impl CommissionerAgent for HandsOff {
    fn act(&mut self, obs: &CommissionerObs) -> CommissionerAction {
        self.calendar.act(obs)
    }

    fn rules(&mut self, _: &LeagueObs) -> Option<Rules> {
        None
    }
}

/// Human commissioner who pushes every rule the full allowed step each
/// offseason, toward an extreme chosen by a seeded xorshift stream, so every
/// run is reproducible.
struct Adversary {
    calendar: BuiltInCommissioner,
    state: u64,
    years: u32,
    up: [bool; 4],
}

impl Adversary {
    fn new(seed: u64, burn_in_years: u8) -> Self {
        Self {
            calendar: BuiltInCommissioner { burn_in_years },
            state: seed | 1,
            years: 0,
            up: [false; 4],
        }
    }

    fn coin(&mut self) -> bool {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        (self.state >> 40) & 1 == 1
    }
}

impl CommissionerAgent for Adversary {
    fn act(&mut self, obs: &CommissionerObs) -> CommissionerAction {
        self.calendar.act(obs)
    }

    fn rules(&mut self, l: &LeagueObs) -> Option<Rules> {
        // Rule changes are limited to a tenth a year (0.04 for revenue
        // sharing): push each rule the full allowed step toward one of its
        // extremes, flipping direction by coin every few years.
        self.years += 1;
        if self.years % 3 == 1 {
            self.up = [self.coin(), self.coin(), self.coin(), self.coin()];
        }
        let step = |x: f64, up: bool| if up { x * 1.10 } else { x * 0.90 };
        let mut r = l.rules.clone();
        r.payroll_floor = step(r.payroll_floor, self.up[0]).clamp(40.0, 100.0);
        r.payroll_cap = step(r.payroll_cap, self.up[1]).clamp(r.payroll_floor + 40.0, 420.0);
        r.revenue_sharing =
            (r.revenue_sharing + if self.up[2] { 0.04 } else { -0.04 }).clamp(0.0, 0.4);
        r.war_price = step(r.war_price, self.up[3]).clamp(2.0, 9.0);
        // Keep each step inside the allowed change from the rules in force.
        let ok = r.check().is_ok() && r.check_change(&l.rules).is_ok();
        ok.then_some(r)
    }
}

/// Human owner: budget at the cap, ballpark spending every year (refused
/// once the club cannot pay), ticket 12 or 150 alternately, stance flipping.
#[derive(Default)]
struct Reckless {
    acts: u32,
}

impl OwnerAgent for Reckless {
    fn act(&mut self, obs: &OwnerObs) -> OwnerAction {
        self.acts += 1;
        let ticket = if self.acts.is_multiple_of(2) {
            TICKET_RANGE.0
        } else {
            TICKET_RANGE.1
        };
        let stance = if obs.stance == Stance::Contend {
            Stance::Rebuild
        } else {
            Stance::Contend
        };
        OwnerAction {
            payroll: obs.payroll_cap,
            ticket: Some(ticket),
            build: Some(RECKLESS_BUDGET_ALL_CAP),
            stance: Some(stance),
        }
    }
}

/// Human owner: budget at the floor, default ticket price, never builds.
struct Frugal;

impl OwnerAgent for Frugal {
    fn act(&mut self, obs: &OwnerObs) -> OwnerAction {
        OwnerAction {
            payroll: obs.payroll_floor,
            ticket: Some(TICKET_REF),
            build: None,
            stance: None,
        }
    }
}

fn owner(mix: Mix, t: u8) -> Box<dyn OwnerAgent> {
    if reckless(mix, t) {
        Box::new(Reckless::default())
    } else if mix == Mix::Frugal {
        Box::new(Frugal)
    } else {
        Box::new(BuiltInOwner)
    }
}

fn seats(mix: Mix, seed: u64, burn_in_years: u8) -> Seats {
    let commissioner: Box<dyn CommissionerAgent> = match mix {
        Mix::HandsOff => Box::new(HandsOff {
            calendar: BuiltInCommissioner { burn_in_years },
        }),
        Mix::Adversary | Mix::OneReckless => Box::new(Adversary::new(seed, burn_in_years)),
        _ => Box::new(BuiltInCommissioner { burn_in_years }),
    };
    Seats::fill(
        commissioner,
        |t| owner(mix, t),
        |_| Box::new(BuiltInGm),
        |_| Box::new(BuiltInManager::default()),
    )
}

/// What one run measured.
#[derive(Debug)]
struct Outcome {
    seasons: usize,
    error: Option<String>,
    settlements: usize,
    /// Highest payroll / cap over non-reckless club-seasons, and where.
    max_ratio_ai: f64,
    max_ai_at: (i32, u8),
    /// Highest payroll / cap over reckless club-seasons, and where.
    max_ratio_reckless: f64,
    min_cash_ai: f64,
    min_cash_reckless: f64,
    /// Settlement cash below zero, non-reckless clubs.
    insolvent_ai: u32,
    /// Settlement cash below zero, reckless clubs.
    insolvent_reckless: u32,
    /// Club-seasons (settlement or year end) below -CREDIT, any club.
    below_credit: u32,
    roster_problems: u32,
    violations: usize,
    /// League mean payroll, $M, first and last ten seasons.
    payroll_first: f64,
    payroll_last: f64,
}

fn mean(v: &[f64]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

fn play(mut w: World, mix: Mix, seed: u64, seasons: usize, burn_in: u8) -> Outcome {
    let mut seats = seats(mix, seed, burn_in);
    let mut log = Vec::new();
    let mut o = Outcome {
        seasons: 0,
        error: None,
        settlements: 0,
        max_ratio_ai: 0.0,
        max_ai_at: (0, 0),
        max_ratio_reckless: 0.0,
        min_cash_ai: f64::INFINITY,
        min_cash_reckless: f64::INFINITY,
        insolvent_ai: 0,
        insolvent_reckless: 0,
        below_credit: 0,
        roster_problems: 0,
        violations: 0,
        payroll_first: 0.0,
        payroll_last: 0.0,
    };
    let mut league_payroll = Vec::with_capacity(seasons);
    if let Err(e) = start_with(&mut w, &mut seats, &mut log) {
        o.error = Some(format!("found: {e}"));
        return o;
    }
    for _ in 0..seasons {
        // Only the current season's events are kept (a season has a lot of
        // game events).
        log.clear();
        if let Err(e) = run_year_with(&mut w, &mut seats, &mut log) {
            o.error = Some(format!("year {}: {e}", w.year));
            break;
        }
        o.seasons += 1;
        for ev in &log {
            let Event::FinancesSettled { clubs, .. } = ev else {
                continue;
            };
            o.settlements += 1;
            for (t, (_, st)) in clubs.iter().enumerate() {
                let res = reckless(mix, t as u8);
                if res {
                    o.min_cash_reckless = o.min_cash_reckless.min(st.cash);
                } else {
                    o.min_cash_ai = o.min_cash_ai.min(st.cash);
                }
                if st.cash < 0.0 {
                    if res {
                        o.insolvent_reckless += 1;
                    } else {
                        o.insolvent_ai += 1;
                    }
                }
                if st.cash < -CREDIT {
                    o.below_credit += 1;
                }
            }
        }
        let cap = w.rules.payroll_cap;
        let mut total = 0.0;
        for t in 0..TEAMS {
            let tt = t as u8;
            let pay = w.payroll(t);
            total += pay;
            let ratio = pay / cap;
            let cash = w.clubs[t].cash;
            if reckless(mix, tt) {
                o.max_ratio_reckless = o.max_ratio_reckless.max(ratio);
                o.min_cash_reckless = o.min_cash_reckless.min(cash);
            } else {
                if ratio > o.max_ratio_ai {
                    o.max_ratio_ai = ratio;
                    o.max_ai_at = (w.year, tt);
                }
                o.min_cash_ai = o.min_cash_ai.min(cash);
            }
            if cash < -CREDIT {
                o.below_credit += 1;
            }
            o.roster_problems += w.roster_problems(tt).len() as u32;
        }
        league_payroll.push(total / TEAMS as f64);
    }
    o.violations = seats.violations.len();
    let n = league_payroll.len();
    o.payroll_first = mean(&league_payroll[..n.min(10)]);
    o.payroll_last = mean(&league_payroll[n.saturating_sub(10)..]);
    o
}

/// The criteria above, as failure messages. Empty means pass.
fn verdict(mix: Mix, o: &Outcome, seasons: usize) -> Vec<String> {
    let mut f = Vec::new();
    if let Some(e) = &o.error {
        f.push(format!("WorldError from run_year_with: {e}"));
    }
    if o.seasons != seasons {
        f.push(format!("completed {} of {seasons} seasons", o.seasons));
    }
    if o.settlements != o.seasons {
        f.push(format!(
            "settlement events {} != seasons {}",
            o.settlements, o.seasons
        ));
    }
    if o.max_ratio_ai > AI_CAP_RATIO {
        f.push(format!(
            "payroll/cap {:.3} > {AI_CAP_RATIO} (non-reckless, year {} club {})",
            o.max_ratio_ai, o.max_ai_at.0, o.max_ai_at.1
        ));
    }
    if o.max_ratio_reckless > RECKLESS_CAP_RATIO {
        f.push(format!(
            "payroll/cap {:.3} > {RECKLESS_CAP_RATIO} (reckless)",
            o.max_ratio_reckless
        ));
    }
    if o.insolvent_ai > 0 {
        f.push(format!(
            "{} non-reckless club-seasons below zero cash",
            o.insolvent_ai
        ));
    }
    if o.below_credit > 0 {
        f.push(format!("{} club-seasons below -CREDIT", o.below_credit));
    }
    if matches!(mix, Mix::AllReckless | Mix::OneReckless) && o.roster_problems > 0 {
        f.push(format!("{} roster problems", o.roster_problems));
    }
    f
}

/// Runs every (mix, seed) on its own thread. Results keep grid order.
fn run_grid(
    seeds: &[u64],
    seasons: usize,
    burn_in: u8,
    make: fn(u64) -> World,
) -> Vec<(Mix, u64, Outcome)> {
    let grid: Vec<(Mix, u64)> = MIXES
        .iter()
        .flat_map(|&m| seeds.iter().map(move |&s| (m, s)))
        .collect();
    std::thread::scope(|scope| {
        let handles: Vec<_> = grid
            .iter()
            .map(|&(mix, seed)| {
                scope.spawn(move || {
                    let o = play(make(seed), mix, seed, seasons, burn_in);
                    (mix, seed, o)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("a run panicked"))
            .collect()
    })
}

fn calibrated_world(seed: u64) -> World {
    let cal = Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    World::new(
        seed,
        cal.params,
        Model::from_calibration(&cal),
        DEFAULT_POOL,
    )
}

fn generic_world(seed: u64) -> World {
    World::new(
        seed,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
    )
}

fn money(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.1}")
    } else {
        "-".into()
    }
}

#[test]
#[ignore = "needs data/internal/params.json; run with --release -- --ignored"]
fn roles_gate() {
    let results = run_grid(&SEEDS, SEASONS, BURN_IN, calibrated_world);
    println!(
        "{:<4} {:>10} {:>9} {:>9} {:>8} {:>8} {:>6} {:>6} {:>6} {:>5} {:>7} {:>8} {:>8}  verdict",
        "case",
        "seed",
        "minCashAI",
        "minCashRc",
        "maxRatAI",
        "maxRatRc",
        "insAI",
        "insRc",
        "belowCr",
        "roster",
        "viol",
        "payFirst",
        "payLast"
    );
    let mut failures = Vec::new();
    for (mix, seed, o) in &results {
        let f = verdict(*mix, o, SEASONS);
        println!(
            "{:<4} {:>10} {:>9} {:>9} {:>8.3} {:>8.3} {:>6} {:>6} {:>6} {:>5} {:>7} {:>8} {:>8}  {}",
            mix.number(),
            seed,
            money(o.min_cash_ai),
            money(o.min_cash_reckless),
            o.max_ratio_ai,
            o.max_ratio_reckless,
            o.insolvent_ai,
            o.insolvent_reckless,
            o.below_credit,
            o.roster_problems,
            o.violations,
            money(o.payroll_first),
            money(o.payroll_last),
            if f.is_empty() { "PASS" } else { "FAIL" }
        );
        for m in f {
            failures.push(format!("case {} seed {seed}: {m}", mix.number()));
        }
    }
    assert!(
        failures.is_empty(),
        "roles gate failed:\n{}",
        failures.join("\n")
    );
}

#[test]
fn roles_smoke() {
    let seeds = [SEEDS[1], SEEDS[2]];
    let results = run_grid(&seeds, 6, 2, generic_world);
    assert_eq!(results.len(), MIXES.len() * seeds.len());
    let mut failures = Vec::new();
    for (mix, seed, o) in &results {
        for m in verdict(*mix, o, 6) {
            failures.push(format!("case {} seed {seed}: {m}", mix.number()));
        }
    }
    assert!(
        failures.is_empty(),
        "roles smoke failed:\n{}",
        failures.join("\n")
    );
}
