//! Runs league years by asking each seat what to do, then issuing its
//! answer as a command. Mirrors `world::driver`, which fills every seat with
//! the same built-in policies.

use protocol::{Actor, TeamId};
use world::agents::{GmView, LeagueObs, OwnerObs};
use world::command::{Command, Event, WorldError};
use world::rating::Kind;
use world::state::{MAJORS_PER_KIND, Phase, TEAMS, Tactics, World};

/// Most players the league office signs for one short club.
const MAX_OFFICE_SIGNINGS: usize = 10;

use crate::agent::{CommissionerAgent, FieldManagerAgent, GmAgent, OwnerAgent};
use crate::builtin::{BuiltInCommissioner, BuiltInGm, BuiltInManager, BuiltInOwner};
use crate::model::{CommissionerObs, FieldManagerObs, GmAction, GmStage, GmTurn};

/// An answer the league refused (not a legal command right now).
#[derive(Debug, Clone, PartialEq)]
pub struct Violation {
    pub year: i32,
    pub team: u8,
    pub seat: &'static str,
    pub error: String,
}

pub struct Seats {
    pub commissioner: Box<dyn CommissionerAgent>,
    pub owners: Vec<Box<dyn OwnerAgent>>,
    pub gms: Vec<Box<dyn GmAgent>>,
    pub managers: Vec<Box<dyn FieldManagerAgent>>,
    pub violations: Vec<Violation>,
}

impl Seats {
    /// Every seat filled by the built-in policy.
    #[must_use]
    pub fn built_in(burn_in_years: u8) -> Self {
        Self::fill(
            Box::new(BuiltInCommissioner { burn_in_years }),
            |_| Box::new(BuiltInOwner),
            |_| Box::new(BuiltInGm),
            |_| Box::new(BuiltInManager::default()),
        )
    }

    /// Fill the commissioner's seat and, per club, the other three.
    #[must_use]
    pub fn fill(
        commissioner: Box<dyn CommissionerAgent>,
        mut owner: impl FnMut(u8) -> Box<dyn OwnerAgent>,
        mut gm: impl FnMut(u8) -> Box<dyn GmAgent>,
        mut manager: impl FnMut(u8) -> Box<dyn FieldManagerAgent>,
    ) -> Self {
        Self {
            commissioner,
            owners: (0..TEAMS as u8).map(&mut owner).collect(),
            gms: (0..TEAMS as u8).map(&mut gm).collect(),
            managers: (0..TEAMS as u8).map(&mut manager).collect(),
            violations: Vec::new(),
        }
    }
}

pub fn exec(
    w: &mut World,
    log: &mut Vec<Event>,
    actor: Actor,
    cmd: Command,
) -> Result<(), WorldError> {
    log.extend(w.execute(&actor, cmd)?);
    Ok(())
}

pub fn commissioner_step(
    w: &mut World,
    seats: &mut Seats,
    log: &mut Vec<Event>,
) -> Result<(), WorldError> {
    let obs = CommissionerObs::new(w);
    let action = seats.commissioner.act(&obs);
    match exec(w, log, Actor::commissioner(), action.command()) {
        // An action for the wrong point of the calendar: the league office
        // follows the calendar instead, and the seat is on record.
        Err(e @ WorldError::Phase(_)) => {
            refuse(seats, w, 0, "commissioner", &e);
            let fallback = BuiltInCommissioner::default().act(&obs);
            exec(w, log, Actor::commissioner(), fallback.command())
        }
        r => r,
    }
}

/// Ask the commissioner for rules for the coming season.
pub fn rules_step(w: &mut World, seats: &mut Seats, log: &mut Vec<Event>) {
    if let Some(rules) = seats.commissioner.rules(&LeagueObs::new(w))
        && let Err(e) = exec(w, log, Actor::commissioner(), Command::SetRules { rules })
    {
        refuse(seats, w, 0, "commissioner", &e);
    }
}

/// Let one club's owner set budget, ticket price, ballpark spending and the
/// general manager's stance. A refused command is on record; the rest go on.
pub fn owner_step(w: &mut World, seats: &mut Seats, log: &mut Vec<Event>, t: u8) {
    let action = seats.owners[usize::from(t)].act(&OwnerObs::new(w, t));
    for cmd in action.commands(t) {
        if let Err(e) = exec(w, log, team_actor(Actor::owner, t), cmd) {
            refuse(seats, w, t, "owner", &e);
        }
    }
}

pub fn team_actor(role: fn(TeamId) -> Actor, team: u8) -> Actor {
    role(TeamId(u32::from(team)))
}

pub fn refuse(seats: &mut Seats, w: &World, team: u8, seat: &'static str, e: &WorldError) {
    seats.violations.push(Violation {
        year: w.year,
        team,
        seat,
        error: e.to_string(),
    });
}

/// Let each field manager choose this season's tactics.
pub fn set_tactics(w: &mut World, seats: &mut Seats, log: &mut Vec<Event>) {
    for t in 0..TEAMS as u8 {
        let obs = FieldManagerObs {
            team: t,
            year: w.year,
            tactics: w.clubs[usize::from(t)].tactics,
        };
        let choice: Tactics = seats.managers[usize::from(t)].tactics(&obs);
        if let Some(usage) = seats.managers[usize::from(t)].usage(&obs) {
            let cmd = Command::SetUsage { team: t, usage };
            if let Err(e) = exec(w, log, team_actor(Actor::field_manager, t), cmd) {
                refuse(seats, w, t, "field manager", &e);
            }
        }
        if choice != obs.tactics {
            let cmd = Command::SetTactics {
                team: t,
                tactics: choice,
            };
            if let Err(e) = exec(w, log, team_actor(Actor::field_manager, t), cmd) {
                refuse(seats, w, t, "field manager", &e);
            }
        }
    }
}

/// Found the league if it is not founded yet, then let managers set tactics.
pub fn start_with(
    w: &mut World,
    seats: &mut Seats,
    log: &mut Vec<Event>,
) -> Result<(), WorldError> {
    if w.phase == Phase::Unfounded {
        commissioner_step(w, seats, log)?;
    }
    set_tactics(w, seats, log);
    Ok(())
}

/// One full league year from the start of a regular season to the start of
/// the next.
pub fn run_year_with(
    w: &mut World,
    seats: &mut Seats,
    log: &mut Vec<Event>,
) -> Result<(), WorldError> {
    for _ in 0..3 {
        commissioner_step(w, seats, log)?; // season, playoffs, finances
    }
    rules_step(w, seats, log);
    for t in 0..TEAMS as u8 {
        owner_step(w, seats, log, t);
    }
    commissioner_step(w, seats, log)?; // progress players
    commissioner_step(w, seats, log)?; // open the draft
    while let Some(d) = &w.draft {
        let Some(&team) = d.order.get(d.next) else {
            break;
        };
        draft_turn(w, seats, log, team)?;
    }
    commissioner_step(w, seats, log)?; // close the draft
    trade_rounds(w, seats, log);
    free_agency(w, seats, log);
    roster_stage(w, seats, log, None);
    start_season(w, seats, log)?;
    set_tactics(w, seats, log);
    Ok(())
}

/// Each club (except `skip`) sets its roster levels.
pub fn roster_stage(w: &mut World, seats: &mut Seats, log: &mut Vec<Event>, skip: Option<u8>) {
    for t in 0..TEAMS as u8 {
        if Some(t) == skip {
            continue;
        }
        let turn = GmTurn {
            stage: GmStage::Roster,
            obs: GmView::new(w, t).observe(),
        };
        for a in seats.gms[usize::from(t)].act(&turn) {
            if let Err(e) = exec(w, log, team_actor(Actor::gm, t), a.command(t)) {
                refuse(seats, w, t, "general manager", &e);
            }
        }
    }
}

/// Start the season. If rosters are not in order the league office sets the
/// levels for the clubs, as a built-in general manager would, and the seat
/// that left them so is on record.
pub fn start_season(
    w: &mut World,
    seats: &mut Seats,
    log: &mut Vec<Event>,
) -> Result<(), WorldError> {
    let obs = CommissionerObs::new(w);
    let action = seats.commissioner.act(&obs);
    let cmd = action.command();
    let first = w.execute(&Actor::commissioner(), cmd.clone());
    match first {
        Ok(events) => {
            log.extend(events);
            Ok(())
        }
        Err(e @ WorldError::Invalid(_)) if w.phase == Phase::FreeAgency => {
            for t in 0..TEAMS as u8 {
                // A club short of players signs the best ones it can afford
                // (minimum-salary deals need no budget room).
                for _ in 0..MAX_OFFICE_SIGNINGS {
                    let turn = GmTurn {
                        stage: GmStage::FreeAgency,
                        obs: GmView::new(w, t).observe(),
                    };
                    let short = [Kind::Batter, Kind::Pitcher].iter().any(|k| {
                        turn.obs.roster.iter().filter(|s| s.kind == *k).count() < MAJORS_PER_KIND
                    });
                    let Some(a) = short
                        .then(|| BuiltInGm.act(&turn))
                        .and_then(|v| v.into_iter().next())
                    else {
                        break;
                    };
                    if exec(w, log, team_actor(Actor::gm, t), a.command(t)).is_err() {
                        break;
                    }
                }
                let turn = GmTurn {
                    stage: GmStage::Roster,
                    obs: GmView::new(w, t).observe(),
                };
                for a in BuiltInGm.act(&turn) {
                    // A club that cannot pay its releases stays as it is.
                    let _ = exec(w, log, team_actor(Actor::gm, t), a.command(t));
                }
            }
            refuse(
                seats,
                w,
                0,
                "general manager (league office filled rosters)",
                &e,
            );
            exec(w, log, Actor::commissioner(), cmd)
        }
        Err(e @ WorldError::Phase(_)) => {
            refuse(seats, w, 0, "commissioner", &e);
            let fallback = BuiltInCommissioner::default().act(&obs);
            exec(w, log, Actor::commissioner(), fallback.command())
        }
        Err(e) => Err(e),
    }
}

/// The club on the clock picks; if its answer is refused it passes.
pub fn draft_turn(
    w: &mut World,
    seats: &mut Seats,
    log: &mut Vec<Event>,
    team: u8,
) -> Result<(), WorldError> {
    let turn = GmTurn {
        stage: GmStage::Draft,
        obs: GmView::new(w, team).observe(),
    };
    let actor = team_actor(Actor::gm, team);
    let answer = seats.gms[usize::from(team)].act(&turn).into_iter().next();
    let cmd = answer.map(|a| a.command(team));
    if let Some(cmd) = cmd {
        match exec(w, log, actor, cmd) {
            Ok(()) => return Ok(()),
            Err(e) => refuse(seats, w, team, "general manager", &e),
        }
    } else {
        refuse(
            seats,
            w,
            team,
            "general manager",
            &WorldError::Invalid("no draft action".into()),
        );
    }
    exec(w, log, actor, GmAction::Pass.command(team))
}

/// Clubs take turns signing one player at a time, starting from a different
/// club each year, until nobody signs.
fn free_agency(w: &mut World, seats: &mut Seats, log: &mut Vec<Event>) {
    while free_agency_round(w, seats, log, None) {}
}

/// One round of free agency: each club (except `skip`, a club a person runs)
/// may sign one player. True if anyone signed.
pub fn free_agency_round(
    w: &mut World,
    seats: &mut Seats,
    log: &mut Vec<Event>,
    skip: Option<u8>,
) -> bool {
    let first = (w.year as usize) % TEAMS;
    let mut signed = false;
    for k in 0..TEAMS {
        let team = ((first + k) % TEAMS) as u8;
        if Some(team) == skip {
            continue;
        }
        let turn = GmTurn {
            stage: GmStage::FreeAgency,
            obs: GmView::new(w, team).observe(),
        };
        let Some(a) = seats.gms[usize::from(team)].act(&turn).into_iter().next() else {
            continue;
        };
        match exec(w, log, team_actor(Actor::gm, team), a.command(team)) {
            Ok(()) => signed = true,
            Err(e) => refuse(seats, w, team, "general manager", &e),
        }
    }
    signed
}

/// Trading: clubs propose, then answer what was put to them. A few rounds
/// let counter-offers happen; rounds stop when nobody does anything.
fn trade_rounds(w: &mut World, seats: &mut Seats, log: &mut Vec<Event>) {
    for _ in 0..TRADE_ROUNDS {
        if !trade_round(w, seats, log, None) {
            break;
        }
    }
}

/// One round of trading: each club that trades (except `skip`) answers the
/// offers put to it and makes its own. True if anyone did anything.
pub fn trade_round(
    w: &mut World,
    seats: &mut Seats,
    log: &mut Vec<Event>,
    skip: Option<u8>,
) -> bool {
    let first = (w.year as usize) % TEAMS;
    let mut acted = false;
    for k in 0..TEAMS {
        let team = ((first + k) % TEAMS) as u8;
        if Some(team) == skip || !seats.gms[usize::from(team)].trades() {
            continue;
        }
        let turn = GmTurn {
            stage: GmStage::Trade,
            obs: GmView::new(w, team).observe_trades(),
        };
        for a in seats.gms[usize::from(team)].act(&turn) {
            match exec(w, log, team_actor(Actor::gm, team), a.command(team)) {
                Ok(()) => acted = true,
                Err(e) => refuse(seats, w, team, "general manager", &e),
            }
        }
    }
    acted
}

/// Rounds of trade talks in a window: offers, answers and new offers to
/// other clubs after a no.
pub const TRADE_ROUNDS: usize = 8;
