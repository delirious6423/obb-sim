use protocol::{Actor, Command, Role, TeamId};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PermissionError {
    #[error("{role:?} may not issue {command:?}")]
    RoleNotAllowed { role: Role, command: Command },
    #[error("{0:?} must be bound to a team")]
    MissingTeam(Role),
    #[error("the commissioner has no team but was bound to {0:?}")]
    CommissionerHasTeam(TeamId),
    #[error("agent bound to {bound:?} cannot target {target:?}")]
    WrongTeam { bound: TeamId, target: TeamId },
}

/// Which role may issue which command.
///
/// The match is exhaustive on purpose: adding a `Command` variant will not
/// compile until someone decides who may issue it. The owner and field manager
/// have no commands yet.
fn required_role(command: &Command) -> Role {
    match command {
        Command::CreateTeam { .. } | Command::ScheduleGame { .. } | Command::PlayGame { .. } => {
            Role::Commissioner
        }
        Command::SignPlayer { .. } => Role::Gm,
    }
}

/// The teams a command acts on. Only team-bound roles are checked against this,
/// so league-wide commands (commissioner only) list none that matter.
/// A future team-bound command that targets a game or player must resolve its
/// team from league state here.
fn target_teams(command: &Command) -> Vec<TeamId> {
    match command {
        Command::CreateTeam { .. } | Command::PlayGame { .. } => vec![],
        Command::SignPlayer { team, .. } => vec![*team],
        Command::ScheduleGame { home, away } => vec![*home, *away],
    }
}

/// Check that `actor` may issue `command`: the role must be the right one, and a
/// team-bound actor may only touch its own team. The commissioner must be unbound.
pub fn authorize(actor: &Actor, command: &Command) -> Result<(), PermissionError> {
    if required_role(command) != actor.role {
        return Err(PermissionError::RoleNotAllowed {
            role: actor.role,
            command: command.clone(),
        });
    }
    match (actor.role, actor.team) {
        (Role::Commissioner, None) => Ok(()),
        (Role::Commissioner, Some(team)) => Err(PermissionError::CommissionerHasTeam(team)),
        (role, None) => Err(PermissionError::MissingTeam(role)),
        (_, Some(bound)) => match target_teams(command).into_iter().find(|t| *t != bound) {
            Some(target) => Err(PermissionError::WrongTeam { bound, target }),
            None => Ok(()),
        },
    }
}
