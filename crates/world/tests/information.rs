//! Market prices must not leak true talent: an asking salary or a bonus demand
//! is no better informed than a scout's report. A general manager that reads
//! prices can therefore recover true ratings no better than scouting does.

use protocol::Actor;
use sim_core::GameParams;
use world::command::Command;
use world::player::{DEFAULT_POOL, Origin, PlayerId, prospect};
use world::rating::{Kind, Model};
use world::scout::{
    amateur_demand, asking_salary, opinion, peak_quality, public_quality, quality_from_war,
};
use world::state::{MIN_SALARY, World};
use world::util::{stream, tag};

fn founded(seed: u64) -> World {
    let mut w = World::new(
        seed,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
    );
    w.execute(&Actor::commissioner(), Command::Found { burn_in_years: 2 })
        .expect("found");
    w
}

fn corr(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let cov: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let va: f64 = a.iter().map(|x| (x - ma).powi(2)).sum();
    let vb: f64 = b.iter().map(|y| (y - mb).powi(2)).sum();
    cov / (va * vb).sqrt()
}

#[test]
fn an_asking_salary_is_the_market_view_and_no_better_than_a_scout() {
    let w = founded(11);
    let (mut price_q, mut scout_q, mut truth) = (vec![], vec![], vec![]);
    for p in w.players.values() {
        let ask = asking_salary(&w, p);
        if ask <= MIN_SALARY + 1e-9 {
            continue; // floored: carries no quality at all
        }
        // The price is a plain function of the market's view...
        let implied = quality_from_war(p.kind, ask / w.rules.war_price);
        assert!(
            (implied - public_quality(&w, p)).abs() < 1e-9,
            "the price must be the market view, nothing more"
        );
        // ...and that view is as noisy as a scout's, so reading prices
        // recovers true quality no better than scouting does.
        price_q.push(implied);
        scout_q.push(opinion(&w, 3, p).now);
        truth.push(p.quality);
    }
    assert!(price_q.len() > 200, "need a sample, got {}", price_q.len());
    let (from_price, from_scout) = (corr(&price_q, &truth), corr(&scout_q, &truth));
    println!("price {from_price:.3}, scout {from_scout:.3}");
    assert!(from_price <= from_scout + 0.03);
}

#[test]
fn a_bonus_demand_is_no_better_informed_than_a_scout() {
    let w = founded(12);
    let mut rng = stream(w.seed, tag::DRAFT_CLASS, 99, 0);
    let (mut demand, mut scout_peak, mut truth) = (vec![], vec![], vec![]);
    for i in 0..1500 {
        let origin = if i % 3 == 0 {
            Origin::HighSchool
        } else {
            Origin::College
        };
        let p = prospect(
            &w.model,
            &mut rng,
            PlayerId(1_000_000 + i),
            w.year,
            origin,
            w.pool,
            &w.era,
        );
        if p.kind != Kind::Batter {
            continue;
        }
        demand.push(libm::log(amateur_demand(&w, &p)));
        scout_peak.push(opinion(&w, 3, &p).peak);
        truth.push(peak_quality(&w, &p));
    }
    let (from_demand, from_scout) = (corr(&demand, &truth), corr(&scout_peak, &truth));
    println!("demand {from_demand:.3}, scout {from_scout:.3}");
    assert!(from_demand <= from_scout + 0.03);
}
