use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use connect::{
    BuiltInManager, CommissionerAction, ExternalAgent, ReferenceTransport, Request, Response,
    Seats, Transport, play_exhibition, run_year_with, start_with,
};
use protocol::Actor;
use sim_core::{Env, GameParams, LINEUP, NullSink, Player, Team, simulate_game};
use world::command::{Command, Event};
use world::driver::run_year;
use world::player::DEFAULT_POOL;
use world::rating::Model;
use world::state::{Phase, Tactics, World};

const BURN_IN: u8 = 2;

fn fresh(seed: u64) -> World {
    World::new(
        seed,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
    )
}

/// The world driver's own run: the yardstick.
fn reference_run(seed: u64, years: usize) -> (World, Vec<Event>) {
    let mut w = fresh(seed);
    let mut log = w
        .execute(
            &Actor::commissioner(),
            Command::Found {
                burn_in_years: BURN_IN,
            },
        )
        .unwrap();
    for _ in 0..years {
        run_year(&mut w, &mut log).unwrap();
    }
    (w, log)
}

fn seat_run(seed: u64, years: usize, mut seats: Seats) -> (World, Vec<Event>, Seats) {
    let mut w = fresh(seed);
    let mut log = Vec::new();
    start_with(&mut w, &mut seats, &mut log).unwrap();
    for _ in 0..years {
        run_year_with(&mut w, &mut seats, &mut log).unwrap();
    }
    (w, log, seats)
}

/// Passes requests to the reference transport and counts them.
struct Counting {
    inner: ReferenceTransport,
    calls: Arc<AtomicU64>,
}

impl Transport for Counting {
    fn call(&mut self, request: &str) -> Result<String, String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.inner.call(request)
    }
}

fn external_seats(calls: &Arc<AtomicU64>, tables_for: fn(u8) -> Tactics) -> Seats {
    let make = |manager: BuiltInManager| Counting {
        inner: ReferenceTransport {
            manager,
            commissioner: connect::BuiltInCommissioner {
                burn_in_years: BURN_IN,
            },
        },
        calls: Arc::clone(calls),
    };
    let base = BuiltInManager::default();
    Seats::fill(
        Box::new(ExternalAgent::new(make(base))),
        |_| Box::new(ExternalAgent::new(make(base))),
        |_| Box::new(ExternalAgent::new(make(base))),
        |t| {
            Box::new(ExternalAgent::new(make(BuiltInManager {
                tactics: tables_for(t),
            })))
        },
    )
}

#[test]
fn built_in_seats_reproduce_the_world_driver() {
    let (w0, log0) = reference_run(11, 2);
    let (w1, log1, seats) = seat_run(11, 2, Seats::built_in(BURN_IN));
    assert!(log0 == log1, "event logs differ");
    assert!(w0 == w1);
    assert!(seats.violations.is_empty());
}

#[test]
fn a_stub_fills_all_four_roles_through_json() {
    let calls = Arc::new(AtomicU64::new(0));
    let (w0, log0) = reference_run(12, 3);
    let (w1, log1, seats) = seat_run(12, 3, external_seats(&calls, |_| Tactics::Baseline));
    assert!(log0 == log1, "the JSON path changed what happened");
    assert!(w0 == w1);
    assert!(seats.violations.is_empty());
    // Owners, GMs and managers were all asked many times.
    assert!(calls.load(Ordering::Relaxed) > 1_000);
}

#[test]
fn a_stub_run_is_deterministic_and_replays() {
    let c = Arc::new(AtomicU64::new(0));
    let tables = |t: u8| {
        if t.is_multiple_of(2) {
            Tactics::Tables
        } else {
            Tactics::Baseline
        }
    };
    let (a, log_a, _) = seat_run(13, 3, external_seats(&c, tables));
    let (b, log_b, _) = seat_run(13, 3, external_seats(&c, tables));
    assert!(log_a == log_b && a == b);
    let tactics_set = log_a
        .iter()
        .filter(|e| matches!(e, Event::TacticsSet { .. }))
        .count();
    assert_eq!(tactics_set, 15, "fifteen clubs choose tables, once");
    let rebuilt = World::from_events(
        13,
        GameParams::reference(),
        Model::generic(),
        DEFAULT_POOL,
        &log_a,
    )
    .unwrap();
    assert!(rebuilt == a);
    // Same as filling the manager seats with the built-in policy directly.
    let mut seats = Seats::built_in(BURN_IN);
    seats.managers = (0..30u8)
        .map(|t| {
            Box::new(BuiltInManager { tactics: tables(t) }) as Box<dyn connect::FieldManagerAgent>
        })
        .collect();
    let (d, log_d, _) = seat_run(13, 3, seats);
    assert!(log_d == log_a && d == a);
}

/// Answers every request with nonsense.
struct Garbage;
impl Transport for Garbage {
    fn call(&mut self, _: &str) -> Result<String, String> {
        Ok("not json".into())
    }
}

/// Answers with legal JSON that asks for illegal things.
struct Hostile;
impl Transport for Hostile {
    fn call(&mut self, request: &str) -> Result<String, String> {
        let r: Request = serde_json::from_str(request).map_err(|e| e.to_string())?;
        let reply = match r {
            Request::Gm(t) => {
                // Sign a player at a price below the ask, release a stranger.
                let bad = t.obs.free_agents.first().map(|f| connect::GmAction::Sign {
                    player: f.id,
                    salary: 0.0,
                    years: 1,
                });
                Response::Gm {
                    actions: bad.into_iter().collect(),
                }
            }
            Request::Owner(_) => Response::Owner {
                action: connect::OwnerAction { payroll: -5.0 },
            },
            _ => return Err("closed".into()),
        };
        serde_json::to_string(&reply).map_err(|e| e.to_string())
    }
}

#[test]
fn a_broken_or_hostile_agent_cannot_stop_the_league() {
    let (w0, log0) = reference_run(14, 2);
    // Garbage everywhere: the commissioner falls back to the calendar, the
    // others do nothing harmful. The league still plays two years.
    let seats = Seats::fill(
        Box::new(ExternalAgent::new(Garbage)),
        |_| Box::new(ExternalAgent::new(Garbage)),
        |_| Box::new(ExternalAgent::new(Garbage)),
        |_| Box::new(ExternalAgent::new(Garbage)),
    );
    let (w, _, seats) = seat_run(14, 2, seats);
    assert_eq!(w.history.len(), 2);
    assert_eq!(w.phase, Phase::Regular);
    // Passing every pick and signing nobody leaves a different league.
    assert!(w != w0 && !log0.is_empty());
    // Draft picks had to be forced through as passes.
    assert!(seats.violations.iter().any(|v| v.seat == "general manager"));

    let seats = Seats::fill(
        Box::new(connect::BuiltInCommissioner {
            burn_in_years: BURN_IN,
        }),
        |_| Box::new(ExternalAgent::new(Hostile)),
        |_| Box::new(ExternalAgent::new(Hostile)),
        |_| Box::new(BuiltInManager::default()),
    );
    let (w, _, seats) = seat_run(14, 1, seats);
    assert_eq!(w.history.len(), 1);
    assert!(
        seats.violations.iter().any(|v| v.seat == "owner"),
        "a negative budget is refused"
    );
    assert!(
        seats.violations.iter().any(|v| v.seat == "general manager"),
        "an under-asking signing is refused"
    );
}

/// Asks for the wrong thing at every step.
struct WrongCalendar;
impl Transport for WrongCalendar {
    fn call(&mut self, _: &str) -> Result<String, String> {
        serde_json::to_string(&Response::Commissioner {
            action: CommissionerAction::CloseDraft,
        })
        .map_err(|e| e.to_string())
    }
}

#[test]
fn a_commissioner_out_of_phase_is_overruled_by_the_calendar() {
    let seats = Seats::fill(
        Box::new(ExternalAgent::new(WrongCalendar)),
        |_| Box::new(connect::BuiltInOwner),
        |_| Box::new(connect::BuiltInGm),
        |_| Box::new(BuiltInManager::default()),
    );
    // Founding itself is out of phase for `CloseDraft`, so found first.
    let mut w = fresh(16);
    let mut log = w
        .execute(
            &Actor::commissioner(),
            Command::Found {
                burn_in_years: BURN_IN,
            },
        )
        .unwrap();
    let mut seats = seats;
    run_year_with(&mut w, &mut seats, &mut log).unwrap();
    assert_eq!(w.history.len(), 1);
    assert!(seats.violations.iter().any(|v| v.seat == "commissioner"));
}

fn team(name: &str) -> Team {
    let rates = GameParams::reference().league;
    let p = |n: usize| Player {
        name: format!("{name} {n}"),
        rates,
    };
    Team::new(name, (0..LINEUP).map(p).collect(), (0..6).map(p).collect()).unwrap()
}

#[test]
fn an_external_field_manager_plays_exhibition_games() {
    let env = Env::new(GameParams::reference()).unwrap();
    let (a, h) = (team("A"), team("H"));
    let calls = Arc::new(AtomicU64::new(0));
    let mk = || {
        ExternalAgent::new(Counting {
            inner: ReferenceTransport::default(),
            calls: Arc::clone(&calls),
        })
    };
    let (mut ma, mut mh) = (mk(), mk());
    for g in 0..20 {
        let plain = simulate_game(&env, &a, &h, 9, g, &mut NullSink);
        let live = play_exhibition(&env, (&a, &mut ma), (&h, &mut mh), 9, g);
        assert_eq!(plain, live, "engine defaults over JSON change nothing");
    }
    assert!(
        calls.load(Ordering::Relaxed) > 20,
        "asked at decision points"
    );
    assert_eq!(ma.failures + mh.failures, 0);
}

#[test]
fn requests_and_responses_survive_json_exactly() {
    let mut w = fresh(15);
    w.execute(&Actor::commissioner(), Command::Found { burn_in_years: 1 })
        .unwrap();
    let turn = connect::GmTurn {
        stage: connect::GmStage::FreeAgency,
        obs: world::agents::GmView::new(&w, 3).observe(),
    };
    let req = Request::Gm(turn);
    let text = serde_json::to_string(&req).unwrap();
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), req);
    let resp = Response::Commissioner {
        action: CommissionerAction::Found { burn_in_years: 7 },
    };
    let text = serde_json::to_string(&resp).unwrap();
    assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), resp);
}
