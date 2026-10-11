use connect::{
    BuiltInCommissioner, BuiltInGm, BuiltInManager, BuiltInOwner, GmAgent, Seats, run_year_with,
    start_with,
};
use gm::bots::{Hoarder, Stretcher, Tanker, TradeSpammer};
use gm::{Personality, Preset, SurplusGm};
use sim_core::GameParams;
use world::command::Event;
use world::player::DEFAULT_POOL;
use world::rating::Model;
use world::state::World;

fn run(
    seed: u64,
    years: usize,
    make: impl Fn(u8) -> Box<dyn GmAgent>,
) -> (World, Vec<Event>, Seats) {
    let mut w = World::new(
        seed,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
    );
    let mut seats = Seats::fill(
        Box::new(BuiltInCommissioner { burn_in_years: 2 }),
        |_| Box::new(BuiltInOwner),
        |t| {
            if t % 2 == 0 {
                make(t)
            } else {
                Box::new(BuiltInGm)
            }
        },
        |_| Box::new(BuiltInManager::default()),
    );
    let mut log = Vec::new();
    start_with(&mut w, &mut seats, &mut log).unwrap();
    for _ in 0..years {
        run_year_with(&mut w, &mut seats, &mut log).unwrap();
    }
    (w, log, seats)
}

fn surplus(t: u8) -> Box<dyn GmAgent> {
    // A zero margin so that even the generic model's small differences trade.
    let eager = Personality {
        trade_margin: 0.0,
        ..Personality::BALANCED
    };
    Box::new(SurplusGm::new(
        u64::from(t),
        Preset::HallOfFame.profile(eager),
    ))
}

#[test]
fn the_ai_general_manager_plays_legal_deterministic_years() {
    let (a, log_a, seats) = run(21, 3, surplus);
    let (b, log_b, _) = run(21, 3, surplus);
    assert!(a == b && log_a == log_b, "same seed, same league");
    assert!(
        seats.violations.is_empty(),
        "illegal actions: {:?}",
        &seats.violations[..seats.violations.len().min(3)]
    );
    let proposed = log_a
        .iter()
        .filter(|e| matches!(e, Event::TradeProposed { .. }))
        .count();
    let answered = log_a
        .iter()
        .filter(|e| matches!(e, Event::TradeAnswered { .. }))
        .count();
    println!("proposed {proposed}, answered {answered}");
    // Whether this small generic league offers a trade worth making varies;
    // the ladder test `trading_happens` checks it on the calibrated league.
    let _ = proposed;
    // Events replay to the same world.
    let rebuilt = World::from_events(
        21,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
        &log_a,
    )
    .unwrap();
    assert!(rebuilt == a);
}

#[test]
fn rookies_do_not_trade_and_veterans_only_answer() {
    for (preset, proposes) in [(Preset::Rookie, false), (Preset::Veteran, false)] {
        let (_, log, _) = run(22, 2, |t| {
            Box::new(SurplusGm::new(
                u64::from(t),
                preset.profile(Personality::BALANCED),
            ))
        });
        let proposed = log.iter().any(|e| matches!(e, Event::TradeProposed { .. }));
        assert_eq!(proposed, proposes, "{preset:?}");
    }
}

#[test]
fn exploit_bots_stay_inside_the_rules() {
    type Maker = fn() -> Box<dyn GmAgent>;
    let bots: [Maker; 4] = [
        || Box::new(Hoarder),
        || Box::new(Tanker),
        || Box::new(Stretcher),
        || Box::new(TradeSpammer),
    ];
    for make in bots {
        let (w, _, seats) = run(23, 2, |_| make());
        assert_eq!(w.history.len(), 2);
        // The league office may have to fill rosters, but nothing else breaks.
        assert!(
            seats
                .violations
                .iter()
                .all(|v| v.seat.starts_with("general manager"))
        );
    }
}

#[test]
fn presets_differ_only_in_knowledge_and_decisions() {
    let ps: Vec<_> = Preset::ALL
        .iter()
        .map(|p| p.profile(Personality::BALANCED))
        .collect();
    for pair in ps.windows(2) {
        assert!(pair[1].extra_noise <= pair[0].extra_noise);
        assert!(pair[1].market_weight >= pair[0].market_weight);
        assert!(pair[1].horizon >= pair[0].horizon);
    }
}
