//! The commissioner's calendar and the founding burn-in: runs a league year
//! by issuing commands for every role, the way a front end would.

use protocol::{Actor, TeamId};
use rand_core::RngCore;

use crate::agents::{GmView, draft_pick, free_agency_step, owner_budget, set_levels};
use crate::command::{Command, Event, WorldError};
use crate::player::{Contract, Level, Origin, PlayerId, prospect, talent_at};
use crate::rating::{Kind, normal, uniform};
use crate::scout::asking_salary;
use crate::state::{MINOR_COST, Phase, TEAMS, World};
use crate::util::{stream, tag};

fn gm(team: u8) -> Actor {
    Actor::gm(TeamId(u32::from(team)))
}

fn owner(team: u8) -> Actor {
    Actor::owner(TeamId(u32::from(team)))
}

fn exec(w: &mut World, log: &mut Vec<Event>, actor: Actor, cmd: Command) -> Result<(), WorldError> {
    log.extend(w.execute(&actor, cmd)?);
    Ok(())
}

/// Everything between the end of one season's finances and the start of the
/// next: budgets, ageing, draft, free agency, roster setting.
pub fn offseason(w: &mut World, log: &mut Vec<Event>) -> Result<(), WorldError> {
    let c = Actor::commissioner();
    for t in 0..TEAMS as u8 {
        exec(w, log, owner(t), owner_budget(w, t))?;
    }
    exec(w, log, c, Command::ProgressPlayers)?;
    exec(w, log, c, Command::OpenDraft)?;
    while let Some(d) = &w.draft {
        let Some(&team) = d.order.get(d.next) else {
            break;
        };
        let cmd = draft_pick(&GmView::new(w, team)).expect("a club is on the clock");
        exec(w, log, gm(team), cmd)?;
    }
    exec(w, log, c, Command::CloseDraft)?;
    // Free agency: clubs take turns signing one player at a time, starting
    // from a different club each year.
    let first = (w.year as usize) % TEAMS;
    loop {
        let mut signed = false;
        for k in 0..TEAMS {
            let team = ((first + k) % TEAMS) as u8;
            if let Some(cmd) = free_agency_step(&GmView::new(w, team)) {
                exec(w, log, gm(team), cmd)?;
                signed = true;
            }
        }
        if !signed {
            break;
        }
    }
    for team in 0..TEAMS as u8 {
        for cmd in set_levels(&GmView::new(w, team)) {
            exec(w, log, gm(team), cmd)?;
        }
    }
    exec(w, log, c, Command::StartSeason)?;
    Ok(())
}

/// One full league year from the start of a regular season.
pub fn run_year(w: &mut World, log: &mut Vec<Event>) -> Result<(), WorldError> {
    let c = Actor::commissioner();
    exec(w, log, c, Command::PlaySeason)?;
    exec(w, log, c, Command::PlayPlayoffs)?;
    exec(w, log, c, Command::SettleFinances)?;
    offseason(w, log)
}

/// Build the opening league inside a scratch world: a seeded population of
/// all ages, then `years` offseasons so the age and talent mix settles.
pub(crate) fn burn_in(w: &mut World, years: u8) {
    seed_population(w);
    for t in 0..TEAMS as u8 {
        if let Command::SetBudget { payroll, .. } = owner_budget(w, t) {
            w.clubs[usize::from(t)].budget = payroll;
        }
    }
    for _ in 0..years {
        offseason(w, &mut Vec::new()).expect("burn-in offseason");
        w.phase = Phase::Offseason;
    }
    w.phase = Phase::Regular;
    if years == 0 {
        // Nothing ran: the seeded league is already in order.
        return;
    }
    w.history.clear();
    w.last_records.clear();
    for f in &mut w.finances {
        f.clear();
    }
}

fn seed_population(w: &mut World) {
    let mut rng = stream(w.seed, tag::GENERATE, 0, 0);
    let year = w.year;
    for kind in [Kind::Batter, Kind::Pitcher] {
        let per_club: usize = Level::Majors.capacity()
            + Level::Triple.capacity()
            + Level::Double.capacity()
            + Level::Single.capacity();
        let total = per_club * TEAMS + 200;
        let mut people = Vec::with_capacity(total);
        while people.len() < total {
            let origin = match rng.next_u64() % 3 {
                0 => Origin::HighSchool,
                1 => Origin::College,
                _ => Origin::International,
            };
            let age = 18 + (rng.next_u64() % 18) as i32;
            // `prospect` ages a player from his entry age; ask for one who
            // would enter at `age`, then set the real age.
            let id = PlayerId(w.next_player);
            let mut p = prospect(
                &w.model,
                &mut rng,
                id,
                year - age + origin.entry_age(),
                origin,
                w.pool,
            );
            if p.kind != kind {
                continue;
            }
            w.next_player += 1;
            p.drift = 0.3 * normal(&mut rng);
            let (z, q) = talent_at(&w.model, &p, age);
            p.z = z;
            p.quality = q;
            people.push(p);
        }
        // The best fill the higher levels; each level is dealt to clubs at random.
        people.sort_by(|a, b| {
            b.quality
                .partial_cmp(&a.quality)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.id.cmp(&b.id))
        });
        let mut at = 0;
        for level in [Level::Majors, Level::Triple, Level::Double, Level::Single] {
            let n = level.capacity() * TEAMS;
            let mut group: Vec<_> = people[at..at + n].to_vec();
            at += n;
            for i in (1..group.len()).rev() {
                let j = (rng.next_u64() % (i as u64 + 1)) as usize;
                group.swap(i, j);
            }
            for (i, mut p) in group.into_iter().enumerate() {
                let team = (i % TEAMS) as u8;
                p.team = Some(team);
                p.level = level;
                let years = 1 + (rng.next_u64() % 4) as u8;
                p.contract = Some(Contract {
                    salary: if level == Level::Majors {
                        asking_salary(w, &p)
                    } else {
                        MINOR_COST
                    },
                    years,
                });
                w.clubs[usize::from(team)].players.insert(p.id);
                w.players.insert(p.id, p);
            }
        }
        for mut p in people.split_off(at) {
            p.free_years = (uniform(&mut rng) * 2.0) as u8;
            w.free_agents.insert(p.id);
            w.players.insert(p.id, p);
        }
    }
}
