//! Phase 0 demo: build a tiny league, play a game, replay the log and compare.

use league::Session;
use protocol::{Actor, Command, GameId, TeamId};

fn main() {
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
