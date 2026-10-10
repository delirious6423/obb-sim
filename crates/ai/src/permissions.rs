use league::LeagueError;
use protocol::{Actor, Command};

/// Check every command an agent proposed against its actor (role and team).
/// If any is not allowed, all are rejected and the first offence is reported.
///
/// This is an early check so an agent's bad batch is dropped whole.
/// `Session::submit` runs the same `league::authorize` check on every command anyway.
pub fn vet(actor: &Actor, commands: Vec<Command>) -> Result<Vec<Command>, LeagueError> {
    for command in &commands {
        league::authorize(actor, command)?;
    }
    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommissionerAgent, CommissionerView, GmAgent, GmView};
    use league::{PermissionError, Session};
    use protocol::{GameId, Role, TeamId};

    fn commands() -> Vec<Command> {
        vec![
            Command::CreateTeam {
                name: "Harbor City Gulls".into(),
            },
            Command::SignPlayer {
                team: TeamId(0),
                name: "Alden Marsh".into(),
                rating: 70,
            },
            Command::ScheduleGame {
                home: TeamId(0),
                away: TeamId(1),
            },
            Command::PlayGame { game: GameId(0) },
        ]
    }

    #[test]
    fn each_command_is_accepted_for_exactly_one_role() {
        // Bound to team 0 for team roles, so every target in `commands()` is fair game.
        let actors = [
            Actor::commissioner(),
            Actor::owner(TeamId(0)),
            Actor::gm(TeamId(0)),
            Actor::field_manager(TeamId(0)),
        ];
        for command in commands() {
            let accepted = actors
                .iter()
                .filter(|a| league::authorize(a, &command).is_ok())
                .count();
            // ScheduleGame targets team 1 too, so no team-bound role passes it;
            // only the commissioner does.
            assert_eq!(accepted, 1, "{command:?}");
        }
    }

    #[test]
    fn wrong_role_is_rejected() {
        let create = Command::CreateTeam {
            name: "Pine Ridge Foxes".into(),
        };
        let err = vet(&Actor::gm(TeamId(0)), vec![create.clone()]).unwrap_err();
        assert_eq!(
            err,
            LeagueError::Permission(PermissionError::RoleNotAllowed {
                role: Role::Gm,
                command: create.clone(),
            })
        );
        assert!(vet(&Actor::owner(TeamId(0)), vec![create.clone()]).is_err());
        assert!(vet(&Actor::field_manager(TeamId(0)), vec![create.clone()]).is_err());
        assert!(vet(&Actor::commissioner(), vec![create]).is_ok());
    }

    struct SneakyGm;
    impl GmAgent for SneakyGm {
        fn act(&mut self, view: &GmView) -> Vec<Command> {
            vec![
                Command::SignPlayer {
                    team: view.team,
                    name: "Teo Brandt".into(),
                    rating: 60,
                },
                // A GM trying to play a game: not theirs to do.
                Command::PlayGame { game: GameId(0) },
            ]
        }
    }

    struct PoachingGm;
    impl GmAgent for PoachingGm {
        fn act(&mut self, view: &GmView) -> Vec<Command> {
            // Signs for the other team.
            vec![Command::SignPlayer {
                team: TeamId(view.team.0 + 1),
                name: "Poached".into(),
                rating: 90,
            }]
        }
    }

    struct ScheduleKeeper;
    impl CommissionerAgent for ScheduleKeeper {
        fn act(&mut self, view: &CommissionerView) -> Vec<Command> {
            match view.teams.as_slice() {
                [(a, _), (b, _), ..] if view.games.is_empty() => {
                    vec![Command::ScheduleGame { home: *a, away: *b }]
                }
                _ => vec![],
            }
        }
    }

    fn session_with_two_teams() -> Session {
        let mut session = Session::new(1);
        for name in ["Harbor City Gulls", "Pine Ridge Foxes"] {
            session
                .submit(
                    Actor::commissioner(),
                    Command::CreateTeam { name: name.into() },
                )
                .unwrap();
        }
        session
    }

    #[test]
    fn agent_that_oversteps_is_rejected_whole() {
        let session = session_with_two_teams();
        let actor = Actor::gm(TeamId(0));
        let proposed = SneakyGm.act(&GmView::of(session.league(), TeamId(0)));
        let err = vet(&actor, proposed).unwrap_err();
        assert!(matches!(
            err,
            LeagueError::Permission(PermissionError::RoleNotAllowed {
                command: Command::PlayGame { .. },
                ..
            })
        ));
    }

    #[test]
    fn gm_cannot_sign_for_another_team() {
        let session = session_with_two_teams();
        let actor = Actor::gm(TeamId(0));
        let proposed = PoachingGm.act(&GmView::of(session.league(), TeamId(0)));
        assert_eq!(
            vet(&actor, proposed).unwrap_err(),
            LeagueError::Permission(PermissionError::WrongTeam {
                bound: TeamId(0),
                target: TeamId(1),
            })
        );
    }

    #[test]
    fn vetted_commissioner_commands_reach_the_league() {
        let mut session = session_with_two_teams();
        let view = CommissionerView::of(session.league());
        let actor = Actor::commissioner();
        for command in vet(&actor, ScheduleKeeper.act(&view)).unwrap() {
            session.submit(actor, command).unwrap();
        }
        assert_eq!(session.league().games().count(), 1);
    }
}
