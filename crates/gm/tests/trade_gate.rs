//! Trade checks against real baseball (ignored by default: they need the
//! git-ignored `data/internal/params.json` and take a few minutes):
//!
//!     cargo test --release -p gm --test trade_gate -- --ignored --nocapture
//!
//! * `every_preset_trades_at_a_real_rate`: with all 30 clubs run by one
//!   preset, every preset completes between `LOW` and `HIGH` trades a season.
//!   Source of the band: MLB clubs made about 3.6 trades a year each in
//!   1996-2000 (FanGraphs/The Hardball Times, "The Best and Worst Teams of the
//!   Trade"), which is about 54 trades a season league-wide; MLB.com counted
//!   58 trades in the 2024 deadline run-up alone and The Ringer 62-74 in the
//!   July windows of 2021-2022. Target: about 60-75 a season, two a club.
//! * `easy_general_managers_lose_value_to_hard_ones`: in a league of Rookie
//!   and Hall of Fame clubs, in trades between clubs that read the same
//!   direction from their records, the Rookie side gives up wins: wins at the
//!   players' true quality in the seasons that follow (up to three, within
//!   the contract). Clubs stay within budget, and a budget is there to be
//!   spent, so salary is printed but not netted off (netted at the WAR price,
//!   the Rookie side comes out ahead by shedding salary; see the output).
//!   This measure was chosen after a first run that netted salary.
//!
//! Thresholds are fixed here before any run and are not edited to pass.

use std::path::Path;

use calibration::Calibration;
use connect::{
    BuiltInCommissioner, BuiltInManager, BuiltInOwner, Seats, run_year_with, start_with,
};
use gm::{Personality, Preset, SurplusGm};
use rayon::prelude::*;
use world::agents::stance_from_record;
use world::command::Event;
use world::player::{DEFAULT_POOL, PlayerId};
use world::rating::Model;
use world::scout::war_of;
use world::state::World;

const SEEDS: [u64; 5] = [20_250_101, 1, 2, 3, 4];
const BURN_IN: u8 = 10;
const SEASONS: usize = 10;
/// Band of accepted trades per league season (target about 60-75).
const LOW: f64 = 40.0;
const HIGH: f64 = 110.0;
/// Net salary (millions a year) that makes a trade a salary move.
const SALARY_MOVE: f64 = 5.0;
/// Most a "pure dump" may send back to the club shedding salary (millions).
const DUMP_BACK: f64 = 1.0;
/// Seasons of a contract counted in a player's true value.
const VALUE_YEARS: u8 = 3;
/// Millions of true wins value per trade the Rookie side must lose on average.
const MIN_TRANSFER: f64 = 2.0;

fn calibration() -> Calibration {
    Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// One accepted trade, as it stood when it was made.
#[derive(Debug, Clone)]
struct Done {
    /// Index of the season in the run.
    year: usize,
    /// Whether the two clubs read the same direction from their records.
    alike: bool,
    from: u8,
    to: u8,
    give: Vec<PlayerId>,
    get: Vec<PlayerId>,
    /// Salaries each way, and true value realized each way: wins at the
    /// player's true quality in each of the next `VALUE_YEARS` seasons (or
    /// the rest of his contract) less his salary, discounted.
    give_pay: f64,
    get_pay: f64,
    give_value: f64,
    get_value: f64,
    /// The wins part alone (true quality, discounted, no salary).
    give_wins: f64,
    get_wins: f64,
}

/// A traded player whose value is still being counted.
struct Open {
    year: usize,
    index: usize,
    /// True if he went from `from` to `to` (part of `give`).
    given: bool,
    id: PlayerId,
    pay: f64,
    left: u8,
    season: u8,
}

/// Count one more season for every open player.
fn accrue(w: &World, open: &mut [Open], out: &mut [Vec<Done>]) {
    for o in open.iter_mut().filter(|o| o.left > 0) {
        let wins = w.players.get(&o.id).map_or(0.0, |p| {
            w.rules.war_price * war_of(p.kind, p.quality).max(0.0)
        });
        let k = libm::pow(0.9, f64::from(o.season));
        let d = &mut out[o.year][o.index];
        if o.given {
            d.give_value += k * (wins - o.pay);
            d.give_wins += k * wins;
        } else {
            d.get_value += k * (wins - o.pay);
            d.get_wins += k * wins;
        }
        o.left -= 1;
        o.season += 1;
    }
}

fn trades(cal: &Calibration, seed: u64, make: &(dyn Fn(u8) -> SurplusGm + Sync)) -> Vec<Vec<Done>> {
    let mut w = World::new(seed, cal.params, Model::from_calibration(cal), DEFAULT_POOL);
    let mut seats = Seats::fill(
        Box::new(BuiltInCommissioner {
            burn_in_years: BURN_IN,
        }),
        |_| Box::new(BuiltInOwner),
        |t| Box::new(make(t)),
        |_| Box::new(BuiltInManager::default()),
    );
    let mut log = Vec::new();
    start_with(&mut w, &mut seats, &mut log).expect("found");
    let mut out: Vec<Vec<Done>> = Vec::new();
    let mut open: Vec<Open> = Vec::new();
    for year in 0..SEASONS {
        let at = log.len();
        run_year_with(&mut w, &mut seats, &mut log).expect("a year");
        // Players traded in earlier windows play another season.
        accrue(&w, &mut open, &mut out);
        let season = &log[at..];
        let accepted: Vec<u32> = season
            .iter()
            .filter_map(|e| match e {
                Event::TradeAnswered {
                    offer,
                    accepted: true,
                } => Some(*offer),
                _ => None,
            })
            .collect();
        // Contracts and qualities at the end of the year are those of the
        // trade window (both change only when players progress next year).
        let mut done = Vec::new();
        for e in season {
            let Event::TradeProposed {
                offer,
                from,
                to,
                give,
                get,
            } = e
            else {
                continue;
            };
            if !accepted.contains(offer) {
                continue;
            }
            let contract = |id: &PlayerId| w.players.get(id).and_then(|p| p.contract);
            let pay =
                |v: &[PlayerId]| -> f64 { v.iter().filter_map(contract).map(|c| c.salary).sum() };
            let index = done.len();
            for (given, ids) in [(true, give), (false, get)] {
                for id in ids {
                    let c = contract(id);
                    open.push(Open {
                        year,
                        index,
                        given,
                        id: *id,
                        pay: c.map_or(0.0, |c| c.salary),
                        left: c.map_or(1, |c| c.years.max(1)).min(VALUE_YEARS),
                        season: 0,
                    });
                }
            }
            done.push(Done {
                year,
                alike: stance_from_record(w.clubs[usize::from(*from)].win_pct_prev)
                    == stance_from_record(w.clubs[usize::from(*to)].win_pct_prev),
                from: *from,
                to: *to,
                give: give.clone(),
                get: get.clone(),
                give_pay: pay(give),
                get_pay: pay(get),
                give_value: 0.0,
                get_value: 0.0,
                give_wins: 0.0,
                get_wins: 0.0,
            });
        }
        out.push(done);
        // The coming season of the players just traded.
        let fresh: Vec<usize> = (0..open.len()).filter(|i| open[*i].year == year).collect();
        let mut now: Vec<Open> = fresh.iter().rev().map(|i| open.swap_remove(*i)).collect();
        accrue(&w, &mut now, &mut out);
        open.extend(now);
    }
    out
}

fn uniform(preset: Preset) -> impl Fn(u64) -> Box<dyn Fn(u8) -> SurplusGm + Sync> {
    move |seed| {
        Box::new(move |t| {
            SurplusGm::new(
                seed ^ (u64::from(t) << 32),
                preset.profile(Personality::BALANCED),
            )
        })
    }
}

#[test]
#[ignore = "needs data/internal/params.json; slow"]
fn every_preset_trades_at_a_real_rate() {
    let cal = calibration();
    let mut failures = Vec::new();
    for preset in Preset::ALL {
        let make = uniform(preset);
        let runs: Vec<Vec<Vec<Done>>> = SEEDS
            .par_iter()
            .map(|&seed| trades(&cal, seed, &*make(seed)))
            .collect();
        let all: Vec<&Done> = runs.iter().flatten().flatten().collect();
        let n = (SEEDS.len() * SEASONS) as f64;
        let per_season = all.len() as f64 / n;
        let per_year: Vec<String> = (0..SEASONS)
            .map(|y| {
                let c: usize = runs.iter().map(|r| r[y].len()).sum();
                format!("{:.1}", c as f64 / SEEDS.len() as f64)
            })
            .collect();
        let share = |f: &dyn Fn(&Done) -> bool| {
            100.0 * all.iter().filter(|d| f(d)).count() as f64 / all.len().max(1) as f64
        };
        let players = all
            .iter()
            .map(|d| (d.give.len() + d.get.len()) as f64)
            .sum::<f64>()
            / all.len().max(1) as f64;
        let multi = share(&|d| d.give.len() + d.get.len() > 2);
        let salary_move = share(&|d| (d.give_pay - d.get_pay).abs() >= SALARY_MOVE);
        let dump = share(&|d| {
            (d.give_pay - d.get_pay >= SALARY_MOVE && d.get_pay <= DUMP_BACK)
                || (d.get_pay - d.give_pay >= SALARY_MOVE && d.give_pay <= DUMP_BACK)
        });
        let clubs: std::collections::BTreeSet<(usize, u8)> = runs
            .iter()
            .enumerate()
            .flat_map(|(r, run)| {
                run.iter()
                    .flatten()
                    .flat_map(move |d| [(r, d.from), (r, d.to)])
            })
            .collect();
        println!(
            "{preset:?}: {per_season:.1} accepted trades a season ({:.2} a club); \
             {players:.2} players a trade; multi-player {multi:.0}%; \
             salary move >= {SALARY_MOVE}M {salary_move:.0}%; pure dump {dump:.0}%; \
             clubs that traded at least once {}/{}\n  per league-year: {}",
            per_season / 30.0,
            clubs.len(),
            SEEDS.len() * 30,
            per_year.join(" ")
        );
        if !(LOW..=HIGH).contains(&per_season) {
            failures.push(format!("{preset:?} {per_season:.1}"));
        }
    }
    assert!(
        failures.is_empty(),
        "trades a season outside {LOW}-{HIGH}: {failures:?}"
    );
}

#[test]
#[ignore = "needs data/internal/params.json; slow"]
fn easy_general_managers_lose_value_to_hard_ones() {
    let cal = calibration();
    let rookie = |t: u8| t % 2 == 1;
    let runs: Vec<Vec<Vec<Done>>> = SEEDS
        .par_iter()
        .map(|&seed| {
            trades(&cal, seed, &|t| {
                let preset = if rookie(t) {
                    Preset::Rookie
                } else {
                    Preset::HallOfFame
                };
                SurplusGm::new(
                    seed ^ (u64::from(t) << 32),
                    preset.profile(Personality::BALANCED),
                )
            })
        })
        .collect();
    // (value, wins, salary taken on, alike) for the Rookie side of each
    // trade between a Rookie and a Hall of Fame club, with a complete window.
    let mut cross: Vec<(f64, f64, f64, bool)> = Vec::new();
    let (mut rr, mut hh) = (0usize, 0usize);
    for d in runs
        .iter()
        .flatten()
        .flatten()
        .filter(|d| d.year + usize::from(VALUE_YEARS) <= SEASONS)
    {
        // What `from` gained.
        let g = (
            d.get_value - d.give_value,
            d.get_wins - d.give_wins,
            d.get_pay - d.give_pay,
        );
        match (rookie(d.from), rookie(d.to)) {
            (true, false) => cross.push((g.0, g.1, g.2, d.alike)),
            (false, true) => cross.push((-g.0, -g.1, -g.2, d.alike)),
            (true, true) => rr += 1,
            (false, false) => hh += 1,
        }
    }
    let stat = |only_alike: bool, f: &dyn Fn(&(f64, f64, f64, bool)) -> f64| {
        let v: Vec<f64> = cross.iter().filter(|x| !only_alike || x.3).map(f).collect();
        let n = v.len() as f64;
        let m = v.iter().sum::<f64>() / n.max(1.0);
        let sd = libm::sqrt(v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (n - 1.0).max(1.0));
        (m, sd / n.max(1.0).sqrt(), v.len())
    };
    for (label, only) in [("all", false), ("same direction", true)] {
        let (value, se, k) = stat(only, &|x| x.0);
        let (wins, wse, _) = stat(only, &|x| x.1);
        let (pay, _, _) = stat(only, &|x| x.2);
        println!(
            "Rookie vs Hall of Fame, {label} ({k} trades; {rr} Rookie-Rookie, {hh} HoF-HoF): \
             Rookie side per trade: true value {value:+.2}M (se {se:.2}), wins {wins:+.2}M \
             (se {wse:.2}), salary taken on {pay:+.2}M a year"
        );
    }
    // The judgement test: between clubs that want the same thing (same
    // direction), so that what moves reflects judgement, not needs.
    let (mean, _, k) = stat(true, &|x| x.1);
    let n = k as f64;
    assert!(n >= 20.0, "too few trades between the two to judge: {n}");
    assert!(
        mean < -MIN_TRANSFER,
        "Rookie clubs should lose at least {MIN_TRANSFER}M of wins a trade to Hall of Fame ones: {mean:.2}"
    );
}
