//! Phase 0 demo: build a tiny league, play a game, replay the log and compare.

use league::Session;
use protocol::{Actor, Command, GameId, TeamId};

/// `sim game [seed] [--pbp]`: play one game between two generic teams and print it.
fn print_game(args: &[String]) {
    use sim_core::{Env, GameParams, PlayLog, Player, Team};
    let seed = args.iter().find_map(|a| a.parse::<u64>().ok()).unwrap_or(1);
    let params = GameParams::reference();
    let env = Env::new(params).expect("reference params are valid");
    let team = |name: &str| {
        let mk = |kind: &str, i: usize| Player {
            name: format!("{name} {kind} {i}"),
            rates: params.league,
        };
        Team::new(
            name,
            (1..=9).map(|i| mk("Batter", i)).collect(),
            (1..=6).map(|i| mk("Pitcher", i)).collect(),
        )
        .expect("valid team")
    };
    let (away, home) = (team("Pine Ridge Foxes"), team("Harbor City Gulls"));
    let mut log = PlayLog::default();
    let game = sim_core::simulate_game(&env, &away, &home, seed, 0, &mut log);
    if args.iter().any(|a| a == "--pbp") {
        println!("{}", sim_core::play_by_play(&away, &home, &log.events));
    }
    println!("{}", sim_core::box_score(&away, &home, &game));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("game") {
        print_game(&args[1..]);
        return;
    }
    const SEED: u64 = 1;
    let mut session = Session::new(SEED);
    let commands = [
        (
            Actor::commissioner(),
            Command::CreateTeam {
                name: "Harbor City Gulls".into(),
            },
        ),
        (
            Actor::commissioner(),
            Command::CreateTeam {
                name: "Pine Ridge Foxes".into(),
            },
        ),
        (
            Actor::gm(TeamId(0)),
            Command::SignPlayer {
                team: TeamId(0),
                name: "Alden Marsh".into(),
                rating: 72,
            },
        ),
        (
            Actor::gm(TeamId(1)),
            Command::SignPlayer {
                team: TeamId(1),
                name: "Nico Vale".into(),
                rating: 58,
            },
        ),
        (
            Actor::commissioner(),
            Command::ScheduleGame {
                home: TeamId(0),
                away: TeamId(1),
            },
        ),
        (Actor::commissioner(), Command::PlayGame { game: GameId(0) }),
    ];
    for (actor, command) in commands {
        if let Err(err) = session.submit(actor, command) {
            eprintln!("command rejected: {err}");
            std::process::exit(1);
        }
    }
    for game in session.league().games() {
        println!("game {}: {:?}", game.id.0, game.result);
    }
    match Session::replay(SEED, session.log()) {
        Ok(replayed) if replayed.league() == session.league() => println!("replay matches"),
        Ok(_) => {
            eprintln!("replay DIVERGED");
            std::process::exit(1);
        }
        Err(err) => {
            eprintln!("replay failed: {err}");
            std::process::exit(1);
        }
    }
}
