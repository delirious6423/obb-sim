//! Playing the games: lineups from rosters, injuries between segments.

use std::collections::BTreeMap;

use rayon::prelude::*;
use season::{BatTotals, Club, PitchTotals, Record, Schedule, TEAMS, replay_game_with};
use sim_core::{GameOutcome, LINEUP, Player as SimPlayer, Team};

use crate::command::{Event, Injury, Line, SeasonLine};
use crate::player::{Level, Player, PlayerId};
use crate::rating::{Kind, normal, uniform};
use crate::scout::{REPLACEMENT, opinion};
use crate::state::{LeagueLine, World};
use crate::util::{stream, tag};

const SEGMENTS: usize = 6;
const ROTATION: usize = 5;
const BULLPEN: usize = 6;

pub(crate) struct Active {
    pub club: Club,
    pub bats: Vec<PlayerId>,
    pub staff: Vec<PlayerId>,
}

/// The healthiest, best nine batters and eleven pitchers a club can field.
pub(crate) fn build_active(w: &World, team: usize, out: &BTreeMap<PlayerId, u16>) -> Active {
    let state = &w.clubs[team];
    let pick = |kind: Kind, need: usize| -> Vec<(PlayerId, f64)> {
        let mut healthy: Vec<(Level, PlayerId, f64)> = Vec::new();
        let mut hurt: Vec<(Level, PlayerId, f64)> = Vec::new();
        for id in &state.players {
            let p = &w.players[id];
            if p.kind != kind {
                continue;
            }
            let now = opinion(w, team as u8, p).now;
            let o = out.get(id).copied().unwrap_or(p.out_games);
            if o == 0 {
                healthy.push((p.level, *id, now));
            } else {
                hurt.push((p.level, *id, now));
            }
        }
        let by = |a: &(Level, PlayerId, f64), b: &(Level, PlayerId, f64)| {
            a.0.cmp(&b.0)
                .then(b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
                .then(a.1.cmp(&b.1))
        };
        healthy.sort_by(by);
        hurt.sort_by(by);
        healthy
            .into_iter()
            .chain(hurt)
            .take(need)
            .map(|(_, id, now)| (id, now))
            .collect()
    };
    let desc = |v: &mut Vec<(PlayerId, f64)>| {
        v.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
    };
    let mut bats = pick(Kind::Batter, LINEUP);
    desc(&mut bats);
    let mut pit = pick(Kind::Pitcher, ROTATION + BULLPEN);
    desc(&mut pit);
    let (rot, pen) = pit.split_at(ROTATION.min(pit.len()));
    let mut pen: Vec<(PlayerId, f64)> = pen.to_vec();
    pen.reverse(); // best last: the closer
    let sim = |id: PlayerId| -> SimPlayer {
        let p = &w.players[&id];
        SimPlayer {
            name: p.name.clone(),
            rates: w.model.rates(p.kind, &p.z),
        }
    };
    let batters: Vec<SimPlayer> = bats.iter().map(|b| sim(b.0)).collect();
    let bullpen: Vec<SimPlayer> = pen.iter().map(|b| sim(b.0)).collect();
    let rotation: Vec<Team> = rot
        .iter()
        .map(|s| {
            let mut pitchers = vec![sim(s.0)];
            pitchers.extend(bullpen.iter().cloned());
            Team::new(state.name.clone(), batters.clone(), pitchers).expect("valid team")
        })
        .collect();
    let mut staff: Vec<PlayerId> = rot.iter().map(|s| s.0).collect();
    staff.extend(pen.iter().map(|b| b.0));
    Active {
        club: Club {
            name: state.name.clone(),
            rotation,
        },
        bats: bats.iter().map(|b| b.0).collect(),
        staff,
    }
}

fn injury_roll(w: &World, seg: usize, p: &Player) -> Option<(u16, f64)> {
    let mut rng = stream(
        w.seed,
        tag::INJURY,
        w.year as u64,
        (seg as u64) << 32 | u64::from(p.id.0),
    );
    let age = f64::from(p.age(w.year));
    let base = match p.kind {
        Kind::Batter => 0.045,
        Kind::Pitcher => 0.06,
    };
    let hazard = base * (1.0 + 0.04 * (age - 28.0).max(0.0));
    if uniform(&mut rng) >= hazard {
        return None;
    }
    let mut games = libm::exp(libm::log(20.0) + 1.0 * normal(&mut rng));
    if uniform(&mut rng) < 0.06 {
        games *= 6.0;
    }
    let games = games.round().clamp(3.0, 400.0) as u16;
    let loss = if games >= 100 && uniform(&mut rng) < 0.3 {
        0.15 + 0.25 * uniform(&mut rng)
    } else {
        0.0
    };
    Some((games, loss))
}

fn totals_of(lines: &BTreeMap<PlayerId, Line>) -> (u32, u32) {
    let mut outs = 0;
    let mut pa = 0;
    for l in lines.values() {
        match l {
            Line::Pitch(p) => outs += p.outs,
            Line::Bat(b) => pa += b.pa,
        }
    }
    (outs, pa)
}

fn war(w: &World, kind: Kind, line: &Line, runs_per_out: f64) -> f64 {
    let lw = crate::rating::LINEAR_WEIGHTS;
    let league = w.model.league().0;
    let v_lg: f64 = league.iter().zip(lw).map(|(r, x)| r * x).sum();
    match (kind, line) {
        (Kind::Batter, Line::Bat(b)) => {
            let singles = b.h - b.doubles - b.triples - b.hr;
            let sum = f64::from(b.so) * lw[0]
                + f64::from(b.bb) * lw[1]
                + f64::from(b.hbp) * lw[2]
                + f64::from(singles) * lw[3]
                + f64::from(b.doubles) * lw[4]
                + f64::from(b.triples) * lw[5]
                + f64::from(b.hr) * lw[6];
            (sum - f64::from(b.pa) * v_lg + f64::from(b.pa) * REPLACEMENT) / 10.0
        }
        (Kind::Pitcher, Line::Pitch(p)) => {
            let saved = runs_per_out * f64::from(p.outs) - f64::from(p.r);
            (saved + f64::from(p.bf) * REPLACEMENT) / 10.0
        }
        _ => 0.0,
    }
}

/// Play the regular season in segments; rosters are re-chosen between them
/// as players get hurt and return.
pub(crate) fn play_regular(w: &World) -> Event {
    let schedule = Schedule::generate(w.seed, w.year as u32, ROTATION);
    let nd = schedule.days.len();
    let mut out: BTreeMap<PlayerId, u16> = w
        .players
        .values()
        .filter(|p| p.team.is_some() && p.out_games > 0)
        .map(|p| (p.id, p.out_games))
        .collect();
    let mut records = vec![Record::default(); TEAMS];
    let mut lines: BTreeMap<PlayerId, Line> = BTreeMap::new();
    let mut loss: BTreeMap<PlayerId, f64> = BTreeMap::new();
    let mut hurt_this_year: BTreeMap<PlayerId, ()> = BTreeMap::new();

    for seg in 0..SEGMENTS {
        let (d0, d1) = (seg * nd / SEGMENTS, (seg + 1) * nd / SEGMENTS);
        let actives: Vec<Active> = (0..TEAMS).map(|t| build_active(w, t, &out)).collect();
        let clubs: Vec<&Club> = actives.iter().map(|a| &a.club).collect();
        let mut bat = vec![[BatTotals::default(); LINEUP]; TEAMS];
        let mut pit = vec![vec![PitchTotals::default(); ROTATION + BULLPEN]; TEAMS];
        for day in &schedule.days[d0..d1] {
            let outcomes: Vec<GameOutcome> = day
                .par_iter()
                .map(|f| replay_game_with(&w.env, &clubs, f, w.seed, w.year as u32))
                .collect();
            for (f, o) in day.iter().zip(&outcomes) {
                let ids = [usize::from(f.away), usize::from(f.home)];
                let starters = [usize::from(f.away_starter), usize::from(f.home_starter)];
                for side in 0..2 {
                    let t = ids[side];
                    let other = o.runs[1 - side];
                    records[t].rs += u32::from(o.runs[side]);
                    records[t].ra += u32::from(other);
                    if o.runs[side] > other {
                        records[t].w += 1;
                    } else {
                        records[t].l += 1;
                    }
                    for (slot, line) in o.bat[side].iter().enumerate() {
                        bat[t][slot].add(line);
                    }
                    for (i, line) in o.pitch[side].iter().enumerate() {
                        if line.used {
                            let staff = clubs[t].staff_index(starters[side], i);
                            pit[t][staff].add(line);
                        }
                    }
                }
            }
        }
        for (t, a) in actives.iter().enumerate() {
            for (slot, id) in a.bats.iter().enumerate() {
                if let Line::Bat(b) = lines.entry(*id).or_insert(Line::Bat(BatTotals::default())) {
                    b.merge(&bat[t][slot]);
                }
            }
            for (i, id) in a.staff.iter().enumerate() {
                if let Line::Pitch(p) = lines
                    .entry(*id)
                    .or_insert(Line::Pitch(PitchTotals::default()))
                {
                    p.merge(&pit[t][i]);
                }
            }
        }
        // Time passes; then this segment's injuries strike.
        let days = (d1 - d0) as u16;
        for v in out.values_mut() {
            *v = v.saturating_sub(days);
        }
        for a in &actives {
            for id in a.bats.iter().chain(&a.staff) {
                let p = &w.players[id];
                if out.get(id).copied().unwrap_or(0) > 0 {
                    continue;
                }
                if let Some((games, l)) = injury_roll(w, seg, p) {
                    out.insert(*id, games);
                    hurt_this_year.insert(*id, ());
                    if l > 0.0 {
                        *loss.entry(*id).or_insert(0.0) += l;
                    }
                }
            }
        }
        // Minor leaguers get hurt too, at the same rates; hurt prospects
        // are how many highly drafted players never arrive.
        let playing: std::collections::BTreeSet<PlayerId> = actives
            .iter()
            .flat_map(|a| a.bats.iter().chain(&a.staff).copied())
            .collect();
        for c in &w.clubs {
            for id in &c.players {
                let p = &w.players[id];
                if p.level == Level::Majors || playing.contains(id) {
                    continue;
                }
                if out.get(id).copied().unwrap_or(0) > 0 {
                    continue;
                }
                if let Some((games, l)) = injury_roll(w, seg, p) {
                    out.insert(*id, games);
                    hurt_this_year.insert(*id, ());
                    if l > 0.0 {
                        *loss.entry(*id).or_insert(0.0) += l;
                    }
                }
            }
        }
        out.retain(|_, v| *v > 0);
    }

    let (outs, _) = totals_of(&lines);
    let total_runs: u32 = records.iter().map(|r| r.rs).sum();
    let runs_per_out = f64::from(total_runs) / f64::from(outs.max(1));
    let mut league = LeagueLine::default();
    let mut season_lines = Vec::with_capacity(lines.len());
    for (id, line) in &lines {
        let kind = w.players[id].kind;
        match line {
            Line::Bat(b) => {
                league.pa += b.pa;
                league.ab += b.ab;
                league.h += b.h;
                league.so += b.so;
                league.bb += b.bb;
                league.hr += b.hr;
            }
            Line::Pitch(p) => {
                league.er += p.er;
                league.outs += p.outs;
            }
        }
        season_lines.push(SeasonLine {
            player: *id,
            line: *line,
            war: war(w, kind, line, runs_per_out),
        });
    }
    league.runs = total_runs;
    league.games = records.iter().map(|r| u32::from(r.w + r.l)).sum();

    // Everyone hurt during the year, or still hurt when it ended.
    let mut injuries = Vec::new();
    let mut ids: Vec<PlayerId> = hurt_this_year.keys().copied().collect();
    ids.extend(out.keys().copied());
    ids.sort();
    ids.dedup();
    for id in ids {
        injuries.push(Injury {
            player: id,
            out_games: out.get(&id).copied().unwrap_or(0),
            loss: loss.get(&id).copied().unwrap_or(0.0),
        });
    }
    // Players hurt in an earlier year who healed this one need clearing too.
    for p in w.players.values() {
        if p.out_games > 0 && !out.contains_key(&p.id) && !injuries.iter().any(|i| i.player == p.id)
        {
            injuries.push(Injury {
                player: p.id,
                out_games: 0,
                loss: 0.0,
            });
        }
    }
    injuries.sort_by_key(|i| i.player);

    Event::SeasonPlayed {
        year: w.year,
        records,
        lines: season_lines,
        injuries,
        league,
    }
}

/// Clubs as they stand (injured players excluded), for the playoffs.
pub(crate) fn playoff_clubs(w: &World) -> Vec<Club> {
    let none = BTreeMap::new();
    (0..TEAMS).map(|t| build_active(w, t, &none).club).collect()
}
