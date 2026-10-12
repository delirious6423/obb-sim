//! Development aid (ignored): how deep and how long clubs stay in debt under
//! the role mixes of the roles gate, and whether they get out. Same mixes,
//! seeds and seasons as `roles_gate.rs` (copied, not shared, so the gate file
//! stays untouched).
//!
//!     cargo test --release -p connect --test debt_report -- --ignored --nocapture

use std::collections::BTreeMap;
use std::path::Path;

use calibration::Calibration;
use connect::{
    BuiltInCommissioner, BuiltInGm, BuiltInManager, BuiltInOwner, CommissionerAction,
    CommissionerAgent, CommissionerObs, OwnerAction, OwnerAgent, Seats, run_year_with, start_with,
};
use world::agents::{LeagueObs, OwnerObs};
use world::command::Event;
use world::player::DEFAULT_POOL;
use world::rating::Model;
use world::state::{CREDIT, Rules, Stance, TEAMS, TICKET_RANGE, TICKET_REF, World};

const SEASONS: usize = 50;
const BURN_IN: u8 = 30;
const SEEDS: [u64; 5] = [20_250_101, 1, 2, 3, 4];
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
        if r.check().is_ok() && r.check_change(&l.rules).is_ok() {
            return Some(r);
        }
        // The floor and cap clamps can disagree; then move the other two.
        let mut r = l.rules.clone();
        r.revenue_sharing =
            (r.revenue_sharing + if self.up[2] { 0.04 } else { -0.04 }).clamp(0.0, 0.4);
        r.war_price = step(r.war_price, self.up[3]).clamp(2.0, 9.0);
        (r.check().is_ok() && r.check_change(&l.rules).is_ok()).then_some(r)
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

#[derive(Debug, Default)]
struct Debt {
    /// Club-seasons with cash below zero at settlement.
    seasons: u32,
    below_credit: u32,
    deepest: f64,
    longest: u32,
    /// Debt spells that ended (cash back to zero or more).
    recovered: u32,
    /// Clubs still in debt after the last season.
    at_end: u32,
    /// Accepted trades proposed by a club in debt that shed its payroll.
    dumps: u32,
    error: Option<String>,
}

fn play(mut w: World, mix: Mix, seed: u64) -> Debt {
    let mut seats = seats(mix, seed, BURN_IN);
    let mut log = Vec::new();
    let mut d = Debt::default();
    start_with(&mut w, &mut seats, &mut log).expect("found");
    let mut spell = [0u32; TEAMS];
    for _ in 0..SEASONS {
        log.clear();
        if let Err(e) = run_year_with(&mut w, &mut seats, &mut log) {
            d.error = Some(e.to_string());
            break;
        }
        let mut from: BTreeMap<u32, u8> = BTreeMap::new();
        for ev in &log {
            match ev {
                Event::FinancesSettled { clubs, .. } => {
                    for (t, (_, st)) in clubs.iter().enumerate() {
                        if st.cash < 0.0 {
                            d.seasons += 1;
                            d.deepest = d.deepest.min(st.cash);
                            spell[t] += 1;
                            d.longest = d.longest.max(spell[t]);
                        } else if spell[t] > 0 {
                            d.recovered += 1;
                            spell[t] = 0;
                        }
                        if st.cash < -CREDIT {
                            d.below_credit += 1;
                        }
                    }
                }
                Event::TradeProposed { offer, from: f, .. } => {
                    from.insert(*offer, *f);
                }
                Event::TradeAnswered {
                    offer,
                    accepted: true,
                } => {
                    if let Some(f) = from.get(offer)
                        && w.clubs[usize::from(*f)].cash < 0.0
                    {
                        d.dumps += 1;
                    }
                }
                _ => {}
            }
        }
    }
    d.at_end = w.clubs.iter().filter(|c| c.cash < 0.0).count() as u32;
    d
}

#[test]
#[ignore = "development aid; needs data/internal/params.json"]
fn debt_report() {
    let cal = Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let grid: Vec<(Mix, u64)> = MIXES
        .iter()
        .flat_map(|&m| SEEDS.iter().map(move |&s| (m, s)))
        .collect();
    let results: Vec<(Mix, u64, Debt)> = std::thread::scope(|scope| {
        let hs: Vec<_> = grid
            .iter()
            .map(|&(mix, seed)| {
                let cal = &cal;
                scope.spawn(move || {
                    let w =
                        World::new(seed, cal.params, Model::from_calibration(cal), DEFAULT_POOL);
                    (mix, seed, play(w, mix, seed))
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().expect("run")).collect()
    });
    println!("case       seed  debtSeas belowCr  deepest longest recovered atEnd dumps");
    for (mix, seed, d) in &results {
        println!(
            "{:<4} {:>10} {:>9} {:>7} {:>8.1} {:>7} {:>9} {:>5} {:>5} {}",
            mix.number(),
            seed,
            d.seasons,
            d.below_credit,
            d.deepest,
            d.longest,
            d.recovered,
            d.at_end,
            d.dumps,
            d.error.as_deref().unwrap_or("")
        );
    }
}
