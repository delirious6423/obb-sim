//! Development aid for the economy balance (ignored; needs the git-ignored
//! `data/internal/params.json`):
//!
//!     cargo test --release -p gm --test economy_probe -- --ignored --nocapture
//!
//! Plays one league per seed (env `PROBE_SEEDS`, comma separated, default
//! "1"; `PROBE_YEARS`, default 40) with every fifth club run by the hoarder
//! bot (or the reference, with `PROBE_BOT=reference`) and prints, over the
//! second half of the run: the market price per win (salary of contracts above
//! twice the minimum over the WAR those players produced), payroll and
//! revenue per club, the payroll/revenue ratio, the correlation of payroll
//! with win%, club-seasons in debt, luxury tax paid and the price scale. The
//! real anchors are in `crates/world/src/state.rs` (economy constants).

use std::collections::BTreeMap;
use std::path::Path;

use calibration::Calibration;
use connect::{
    BuiltInCommissioner, BuiltInManager, BuiltInOwner, GmAgent, Seats, run_year_with, start_with,
};
use gm::bots::{Hoarder, ReferenceGm};
use rayon::prelude::*;
use world::command::Event;
use world::player::{DEFAULT_POOL, PlayerId};
use world::rating::Model;
use world::state::World;

fn tested(team: u8) -> bool {
    team.is_multiple_of(5)
}

fn corr(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        sxy += (a - mx) * (b - my);
        sxx += (a - mx) * (a - mx);
        syy += (b - my) * (b - my);
    }
    sxy / (sxx * syy).sqrt()
}

#[derive(Default, Debug)]
struct Sums {
    fa_salary: f64,
    fa_war: f64,
    payroll: f64,
    revenue: f64,
    club_seasons: f64,
    debt: f64,
    debt_ref: f64,
    edge: f64,
    seasons: f64,
    tax: f64,
    r: f64,
    scale: f64,
    tested_pay: f64,
    ref_pay: f64,
    tested_cash: f64,
    ref_cash: f64,
}

fn run(cal: &Calibration, seed: u64, years: usize, bot: &str) -> Sums {
    let from: usize = std::env::var("PROBE_FROM")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or(years / 2);
    let mut w = World::new(seed, cal.params, Model::from_calibration(cal), DEFAULT_POOL);
    let hoard = bot != "reference";
    let mut seats = Seats::fill(
        Box::new(BuiltInCommissioner { burn_in_years: 10 }),
        |_| Box::new(BuiltInOwner),
        |t| -> Box<dyn GmAgent> {
            if tested(t) && hoard {
                Box::new(Hoarder)
            } else {
                Box::new(ReferenceGm)
            }
        },
        |_| Box::new(BuiltInManager::default()),
    );
    let mut log = Vec::new();
    start_with(&mut w, &mut seats, &mut log).expect("found");
    let mut s = Sums::default();
    for y in 0..years {
        let min = w.min_salary();
        let salary: BTreeMap<PlayerId, f64> = w
            .players
            .values()
            .filter(|p| p.team.is_some())
            .filter_map(|p| p.contract.map(|c| (p.id, c.salary)))
            .filter(|(_, sal)| *sal > 2.0 * min)
            .collect();
        let pay: Vec<f64> = (0..w.clubs.len()).map(|t| w.payroll(t)).collect();
        let before = log.len();
        run_year_with(&mut w, &mut seats, &mut log).expect("a year");
        if y < from {
            continue;
        }
        for e in &log[before..] {
            if let Event::SeasonPlayed { lines, .. } = e {
                for l in lines {
                    if let Some(sal) = salary.get(&l.player) {
                        s.fa_salary += sal;
                        s.fa_war += l.war;
                    }
                }
            }
        }
        let rec = &w.history.last().expect("season").records;
        let wp: Vec<f64> = rec
            .iter()
            .map(|r| f64::from(r.w) / f64::from(r.w + r.l))
            .collect();
        s.r += corr(&pay, &wp);
        let (mut a, mut b) = (0.0, 0.0);
        for (t, &wpt) in wp.iter().enumerate() {
            let f = w.finances[t].last().expect("books");
            s.payroll += f.payroll;
            s.revenue += f.revenue;
            s.tax += f.tax;
            s.club_seasons += 1.0;
            if w.clubs[t].cash < 0.0 {
                s.debt += 1.0;
                if !tested(t as u8) {
                    s.debt_ref += 1.0;
                }
            }
            if tested(t as u8) {
                a += wpt / 6.0;
                s.tested_pay += f.payroll / 6.0;
                s.tested_cash += w.clubs[t].cash / 6.0;
            } else {
                b += wpt / 24.0;
                s.ref_pay += f.payroll / 24.0;
                s.ref_cash += w.clubs[t].cash / 24.0;
            }
        }
        if std::env::var("PROBE_VERBOSE").is_ok() {
            let debt: Vec<usize> = (0..w.clubs.len())
                .filter(|t| w.clubs[*t].cash < 0.0)
                .collect();
            let f = |t: usize| w.finances[t].last().expect("books").clone();
            let worst = (0..w.clubs.len())
                .min_by(|a, b| w.clubs[*a].cash.total_cmp(&w.clubs[*b].cash))
                .expect("clubs");
            let fw = f(worst);
            println!(
                "  seed {seed} year {y}: pay {:.1} rev {:.1} debt {debt:?} worst {worst} cash {:.1} rev {:.1} pay {:.1} other {:.1} spent {:.1} budget {:.1}",
                pay.iter().sum::<f64>() / pay.len() as f64,
                (0..w.clubs.len()).map(|t| f(t).revenue).sum::<f64>() / w.clubs.len() as f64,
                w.clubs[worst].cash,
                fw.revenue,
                fw.payroll,
                fw.other,
                fw.spent,
                w.clubs[worst].budget
            );
        }
        s.edge += a - b;
        s.seasons += 1.0;
        s.scale = w.scale();
    }
    s
}

#[test]
#[ignore = "development aid"]
fn probe() {
    let cal = Calibration::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let seeds: Vec<u64> = std::env::var("PROBE_SEEDS")
        .unwrap_or_else(|_| "1".into())
        .split(',')
        .map(|x| x.trim().parse().expect("seed"))
        .collect();
    let years: usize = std::env::var("PROBE_YEARS")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or(40);
    let bot = std::env::var("PROBE_BOT").unwrap_or_else(|_| "hoarder".into());
    let all: Vec<(u64, Sums)> = seeds
        .par_iter()
        .map(|&seed| (seed, run(&cal, seed, years, &bot)))
        .collect();
    for (seed, s) in &all {
        let n = s.seasons;
        let sc = s.scale.max(1e-9);
        println!(
            "seed {seed} ({bot}): $/WAR {:.2} (real {:.2}) | payroll {:.1} revenue {:.1} ratio {:.2} | r(payroll,win%) {:.2} | debt club-seasons {:.3} (reference clubs {:.3}) | tax/season {:.1} | scale {:.2} | edge {:+.4} | pay tested {:.1} ref {:.1} | cash tested {:.1} ref {:.1}",
            s.fa_salary / s.fa_war,
            s.fa_salary / s.fa_war / sc,
            s.payroll / s.club_seasons,
            s.revenue / s.club_seasons,
            s.payroll / s.revenue,
            s.r / n,
            s.debt / s.club_seasons,
            s.debt_ref / (s.club_seasons * 0.8),
            s.tax / n,
            s.scale,
            s.edge / n,
            s.tested_pay / n,
            s.ref_pay / n,
            s.tested_cash / n,
            s.ref_cash / n,
        );
    }
}
