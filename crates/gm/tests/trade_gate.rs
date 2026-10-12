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
//!   and Hall of Fame clubs, the Rookie side of their trades loses true value
//!   (true quality, not anyone's opinion) on average.
//!
//! Thresholds are fixed here before any run and are not edited to pass.

use std::path::Path;

use calibration::Calibration;
use connect::{
    BuiltInCommissioner, BuiltInManager, BuiltInOwner, Seats, run_year_with, start_with,
};
use gm::{Personality, Preset, SurplusGm};
use rayon::prelude::*;
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
/// Millions of true value per trade the Rookie side must lose on average.
const MIN_TRANSFER: f64 = 0.5;

fn calibration() -> Calibration {
    Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// One accepted trade, as it stood when it was made.
#[derive(Debug, Clone)]
struct Done {
    from: u8,
    to: u8,
    give: Vec<PlayerId>,
    get: Vec<PlayerId>,
    /// Salaries and true values of the players each way.
    give_pay: f64,
    get_pay: f64,
    give_value: f64,
    get_value: f64,
}

/// True value of a player on his contract: wins at his true quality now,
/// less his pay, over up to `VALUE_YEARS` seasons.
fn true_value(w: &World, id: PlayerId) -> f64 {
    let p = &w.players[&id];
    let (pay, years) = p.contract.map_or((0.0, 1), |c| (c.salary, c.years.max(1)));
    let per = w.rules.war_price * war_of(p.kind, p.quality).max(0.0) - pay;
    (0..years.min(VALUE_YEARS))
        .map(|t| libm::pow(0.9, f64::from(t)) * per)
        .sum()
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
    let mut out = Vec::new();
    for _ in 0..SEASONS {
        let at = log.len();
        run_year_with(&mut w, &mut seats, &mut log).expect("a year");
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
        let year: Vec<Done> = season
            .iter()
            .filter_map(|e| match e {
                Event::TradeProposed {
                    offer,
                    from,
                    to,
                    give,
                    get,
                } if accepted.contains(offer) => {
                    let pay = |v: &[PlayerId]| -> f64 {
                        v.iter()
                            .filter_map(|id| w.players.get(id)?.contract.map(|c| c.salary))
                            .sum()
                    };
                    let value = |v: &[PlayerId]| -> f64 {
                        v.iter()
                            .filter(|id| w.players.contains_key(id))
                            .map(|id| true_value(&w, *id))
                            .sum()
                    };
                    Some(Done {
                        from: *from,
                        to: *to,
                        give: give.clone(),
                        get: get.clone(),
                        give_pay: pay(give),
                        get_pay: pay(get),
                        give_value: value(give),
                        get_value: value(get),
                    })
                }
                _ => None,
            })
            .collect();
        out.push(year);
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
    // Value each club gets (true value in minus true value out), by pairing.
    let mut cross = Vec::new();
    let (mut rr, mut hh) = (0usize, 0usize);
    for d in runs.iter().flatten().flatten() {
        let to_from = d.get_value - d.give_value; // what `from` gained
        match (rookie(d.from), rookie(d.to)) {
            (true, false) => cross.push(to_from),
            (false, true) => cross.push(-to_from),
            (true, true) => rr += 1,
            (false, false) => hh += 1,
        }
    }
    let n = cross.len() as f64;
    let mean = cross.iter().sum::<f64>() / n.max(1.0);
    let sd =
        libm::sqrt(cross.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / (n - 1.0).max(1.0));
    let lost = cross.iter().filter(|x| **x < 0.0).count();
    println!(
        "Rookie vs Hall of Fame: {} trades between them ({rr} Rookie-Rookie, {hh} HoF-HoF); \
         Rookie side's true value per trade {mean:.2}M (se {:.2}); Rookie lost value in {lost}",
        cross.len(),
        sd / n.max(1.0).sqrt()
    );
    assert!(n >= 20.0, "too few trades between the two to judge: {n}");
    assert!(
        mean < -MIN_TRANSFER,
        "Rookie clubs should lose at least {MIN_TRANSFER}M a trade to Hall of Fame ones: {mean:.2}"
    );
}
