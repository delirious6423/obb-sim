//! Playing the games: lineups from rosters, injuries between segments.

use std::collections::BTreeMap;
use std::sync::Arc;

use rayon::prelude::*;
use season::{BatTotals, Club, PitchTotals, Schedule, TEAMS, replay_game_with};
use sim_core::{GameOutcome, LINEUP, ManagerMaker, Player as SimPlayer, Team};
use tactics::{TableManagerMaker, Tables};

use crate::command::{Event, GameResult, Injury, Line, SeasonLine, SegmentEnd};
use crate::player::{Level, Player, PlayerId};
use crate::rating::{Kind, normal, uniform};
use crate::scout::{REPLACEMENT, opinion};
use crate::state::{InSeason, LeagueLine, Tactics, World};
use crate::util::{stream, tag};

const SEGMENTS: usize = 6;
const ROTATION: usize = 5;
const BULLPEN: usize = 6;

/// Tables for the table manager, built from average teams the first time
/// they are needed. The result depends only on the game parameters.
/// Building runs a parallel loop, so callers must not hold this from inside
/// another rayon task (`build_active` is only called from sequential code).
fn tables(w: &World) -> &Arc<Tables> {
    w.tables.get_or_init(|| {
        let rates = w.env.params.league;
        let player = |name: &str, n: usize| SimPlayer {
            name: format!("{name} {n}"),
            rates,
        };
        let teams: Vec<Team> = (0..4)
            .map(|i| {
                let name = format!("Table {i}");
                Team::new(
                    &name,
                    (0..LINEUP).map(|n| player(&name, n)).collect(),
                    (0..ROTATION.min(1) + BULLPEN)
                        .map(|n| player(&name, n))
                        .collect(),
                )
                .expect("valid team")
            })
            .collect();
        Arc::new(Tables::build(&w.env, &teams, 20_000, 0x7ab1e5))
    })
}

pub(crate) struct Active {
    pub club: Club,
    pub bats: Vec<PlayerId>,
    pub staff: Vec<PlayerId>,
}

/// Days in this year's regular season.
pub(crate) fn season_days(w: &World) -> usize {
    Schedule::generate(w.seed, w.year as u32, ROTATION)
        .days
        .len()
}

/// The healthiest, best nine batters and eleven pitchers a club can field.
/// `out` is the in-season injury list (absent means healthy); `None` takes
/// each player's own injury, for the playoffs.
pub(crate) fn build_active(
    w: &World,
    team: usize,
    out: Option<&BTreeMap<PlayerId, u16>>,
) -> Active {
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
            let o = out.map_or(p.out_games, |m| m.get(id).copied().unwrap_or(0));
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
    let manager: Option<Arc<dyn ManagerMaker>> = match state.tactics {
        Tactics::Baseline => None,
        Tactics::Tables => Some(Arc::new(TableManagerMaker::new(
            Arc::clone(tables(w)),
            pen.iter().map(|b| b.1).collect(),
            Arc::clone(&w.logs[team]),
        ))),
    };
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
            manager,
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

/// First day of segment `seg` and the day after its last (rosters are
/// re-chosen between segments as players get hurt and return).
fn segment_bounds(seg: usize, days: usize) -> (usize, usize) {
    (seg * days / SEGMENTS, (seg + 1) * days / SEGMENTS)
}

fn segment_of(day: usize, days: usize) -> usize {
    (0..SEGMENTS)
        .find(|s| day < segment_bounds(*s, days).1)
        .unwrap_or(SEGMENTS - 1)
}

/// Play the next `days` days of the regular season (all that is left when
/// `days` is large). The result is the same however the days are split:
/// rosters are chosen at segment starts and the injury rolls depend only on
/// the segment, so stepping day by day gives the same season as one call.
pub(crate) fn play_days(w: &World, days: u16) -> Vec<Event> {
    let schedule = Schedule::generate(w.seed, w.year as u32, ROTATION);
    let nd = schedule.days.len();
    let mut st = w.season.clone().unwrap_or_else(|| InSeason::start(w));
    let target = (usize::from(st.day) + usize::from(days)).min(nd);
    let mut events = Vec::new();
    while usize::from(st.day) < target {
        let from = usize::from(st.day);
        let seg = segment_of(from, nd);
        let (_, end) = segment_bounds(seg, nd);
        let to = end.min(target);
        let ev = play_range(w, &schedule, &st, seg, from, to, to == end);
        st.absorb(&ev);
        events.push(ev);
    }
    if usize::from(st.day) == nd && nd > 0 {
        events.push(finish_season(w, &st));
    }
    events
}

/// Play days `from..to` of segment `seg`; when `seg_end`, time passes and the
/// segment's injuries strike.
fn play_range(
    w: &World,
    schedule: &Schedule,
    st: &InSeason,
    seg: usize,
    from: usize,
    to: usize,
    seg_end: bool,
) -> Event {
    let actives: Vec<Active> = (0..TEAMS)
        .map(|t| build_active(w, t, Some(&st.out)))
        .collect();
    let clubs: Vec<&Club> = actives.iter().map(|a| &a.club).collect();
    let mut bat = vec![[BatTotals::default(); LINEUP]; TEAMS];
    let mut pit = vec![vec![PitchTotals::default(); ROTATION + BULLPEN]; TEAMS];
    let mut results = Vec::new();
    for (offset, day) in schedule.days[from..to].iter().enumerate() {
        let outcomes: Vec<GameOutcome> = day
            .par_iter()
            .map(|f| replay_game_with(&w.env, &clubs, f, w.seed, w.year as u32))
            .collect();
        for (f, o) in day.iter().zip(&outcomes) {
            results.push(GameResult {
                number: f.number,
                day: (from + offset) as u16,
                away: f.away,
                home: f.home,
                away_runs: o.runs[0],
                home_runs: o.runs[1],
            });
            let ids = [usize::from(f.away), usize::from(f.home)];
            let starters = [usize::from(f.away_starter), usize::from(f.home_starter)];
            for side in 0..2 {
                let t = ids[side];
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
    let mut lines: BTreeMap<PlayerId, Line> = BTreeMap::new();
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
    let segment_end = seg_end.then(|| {
        // Time passes; then this segment's injuries strike.
        let days = (schedule.days.len() * (seg + 1) / SEGMENTS
            - schedule.days.len() * seg / SEGMENTS) as u16;
        let mut out = st.out.clone();
        for v in out.values_mut() {
            *v = v.saturating_sub(days);
        }
        let mut loss: BTreeMap<PlayerId, f64> = BTreeMap::new();
        let mut hurt: Vec<PlayerId> = Vec::new();
        let mut roll = |p: &Player, out: &mut BTreeMap<PlayerId, u16>| {
            if out.get(&p.id).copied().unwrap_or(0) > 0 {
                return;
            }
            if let Some((games, l)) = injury_roll(w, seg, p) {
                out.insert(p.id, games);
                hurt.push(p.id);
                if l > 0.0 {
                    *loss.entry(p.id).or_insert(0.0) += l;
                }
            }
        };
        for a in &actives {
            for id in a.bats.iter().chain(&a.staff) {
                roll(&w.players[id], &mut out);
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
                roll(p, &mut out);
            }
        }
        out.retain(|_, v| *v > 0);
        SegmentEnd {
            out,
            loss: loss.into_iter().collect(),
            hurt,
        }
    });
    Event::DaysPlayed {
        year: w.year,
        to_day: to as u16,
        results,
        lines: lines.into_iter().collect(),
        segment_end,
    }
}

/// The season's end: lines with their value above replacement, league totals
/// and everyone hurt during the year or still hurt at its end.
fn finish_season(w: &World, st: &InSeason) -> Event {
    let (outs, _) = totals_of(&st.lines);
    let total_runs: u32 = st.records.iter().map(|r| r.rs).sum();
    let runs_per_out = f64::from(total_runs) / f64::from(outs.max(1));
    let mut league = LeagueLine::default();
    let mut season_lines = Vec::with_capacity(st.lines.len());
    for (id, line) in &st.lines {
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
    league.games = st.records.iter().map(|r| u32::from(r.w + r.l)).sum();

    let mut injuries = Vec::new();
    let mut ids: Vec<PlayerId> = st.hurt.iter().copied().collect();
    ids.extend(st.out.keys().copied());
    ids.sort();
    ids.dedup();
    for id in ids {
        injuries.push(Injury {
            player: id,
            out_games: st.out.get(&id).copied().unwrap_or(0),
            loss: st.loss.get(&id).copied().unwrap_or(0.0),
        });
    }
    // Players hurt in an earlier year who healed this one need clearing too.
    for p in w.players.values() {
        if p.out_games > 0
            && !st.out.contains_key(&p.id)
            && !injuries.iter().any(|i| i.player == p.id)
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
        records: st.records.clone(),
        lines: season_lines,
        injuries,
        league,
    }
}

/// The box score of a game of the season under way (or just finished, until
/// players progress), played again from the same dice. `None` when the game
/// has not been played, or when rosters have since been changed by hand so
/// that it would not come out the same.
pub(crate) fn box_score_text(w: &World, number: u16) -> Option<String> {
    let st = w.season.as_ref()?;
    let r = st.results.iter().find(|r| r.number == number)?;
    let schedule = Schedule::generate(w.seed, w.year as u32, ROTATION);
    let nd = schedule.days.len();
    let seg = segment_of(usize::from(r.day), nd);
    let out = st.seg_starts.get(seg)?;
    let actives: Vec<Active> = (0..TEAMS).map(|t| build_active(w, t, Some(out))).collect();
    let clubs: Vec<&Club> = actives.iter().map(|a| &a.club).collect();
    let f = schedule.fixture(number)?;
    let o = replay_game_with(&w.env, &clubs, f, w.seed, w.year as u32);
    if o.runs != [r.away_runs, r.home_runs] {
        return None;
    }
    let away = clubs[usize::from(f.away)]
        .rotation
        .get(usize::from(f.away_starter))?;
    let home = clubs[usize::from(f.home)]
        .rotation
        .get(usize::from(f.home_starter))?;
    Some(sim_core::box_score(away, home, &o))
}

/// Clubs as they stand (injured players excluded), for the playoffs.
pub(crate) fn playoff_clubs(w: &World) -> Vec<Club> {
    (0..TEAMS).map(|t| build_active(w, t, None).club).collect()
}
