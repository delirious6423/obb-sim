//! Playing the games: lineups from rosters, injuries between segments.

use std::collections::BTreeMap;
use std::sync::Arc;

use rayon::prelude::*;
use season::{BatTotals, Club, PitchTotals, Schedule, TEAMS, replay_game_with};
use sim_core::{
    GameOutcome, GameView, LINEUP, MAX_PITCHERS, Manager, ManagerMaker, Player as SimPlayer,
    ReliefCall, StealCall, Team,
};
use tactics::{DecisionLog, TableManagerMaker, Tables};

use crate::command::{Event, GameResult, Injury, Line, SeasonLine, SegmentEnd};
use crate::player::{Level, Player, PlayerId};
use crate::rating::{Kind, normal, uniform};
use crate::scout::{opinion, replacement};
use crate::state::{
    InSeason, LeagueLine, PitcherRole, ReliefAction, ReliefRule, Tactics, Usage, World,
};
use crate::util::{stream, tag};

/// Rosters are re-chosen at the start of each segment (about a week), as
/// players get hurt and come back.
const SEGMENTS: usize = 26;
const ROTATION: usize = 5;
/// Relievers in the average teams the manager's tables are built from.
const TABLE_BULLPEN: usize = 6;
/// Healthy batters beyond the nine regulars who can play a game.
const BENCH: usize = 4;
/// Each regular sits about one day in this many (a rest day gives a bench
/// player the slot). Real regulars play about 85% of their club's games.
const REST_CYCLE: usize = 7;

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
                    (0..ROTATION.min(1) + TABLE_BULLPEN)
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
    /// Everyone who can play a game: the nine regulars, then the bench.
    pub bats: Vec<PlayerId>,
    /// The rotation, then the bullpen (closer last).
    pub staff: Vec<PlayerId>,
    name: String,
    usage: Usage,
    rules: Arc<[ReliefRule]>,
    /// The club's table tactics (tables and decision log), if it uses them.
    tables: Option<(Arc<Tables>, Arc<DecisionLog>)>,
    /// Batters in lineup order (regulars, then bench) with their sim ratings.
    batters: Vec<(PlayerId, SimPlayer)>,
    /// Pitchers in `staff` order with their sim ratings and the club's
    /// opinion of them.
    pitchers: Vec<(SimPlayer, f64)>,
    rotation: usize,
}

impl Active {
    /// The club that takes the field on `day` of the season (`None`: the
    /// regulars, for the playoffs). Platoon partners start their share of
    /// games; on a rest day a regular sits in turn and a bench player takes
    /// his slot; the bullpen order turns over by day, so relievers who
    /// pitched lately rest (the closer stays last). Returns the club, the
    /// player id for each lineup slot, and the staff index (into `staff`) of
    /// each pitcher in the game's pitcher list per rotation slot.
    pub(crate) fn club_on(&self, day: Option<usize>) -> (Club, Vec<PlayerId>, Vec<usize>) {
        let mut slots: Vec<usize> = (0..LINEUP.min(self.batters.len())).collect();
        let bench = self.batters.len().saturating_sub(LINEUP);
        if let Some(day) = day {
            // Platoons: the weakest regulars share their slot with the best
            // bench players.
            let platoons = usize::from(self.usage.platoon_slots)
                .min(bench)
                .min(slots.len());
            let mut taken = vec![false; slots.len()];
            for k in 0..platoons {
                let s = slots.len() - 1 - k;
                taken[s] = true;
                let turn = (day * 37 + k * 53) % 100;
                if (turn as f64) < 100.0 * self.usage.platoon_share {
                    slots[s] = LINEUP + k;
                }
            }
            // Rest days come from the rest of the bench (or all of it).
            let (first, spare) = if bench > platoons {
                (platoons, bench - platoons)
            } else {
                // Every bench player is a platoon partner: no stand-ins left.
                (0, 0)
            };
            let mut rested = 0;
            for (s, slot) in slots.iter_mut().enumerate() {
                if taken[s] || spare == 0 {
                    continue;
                }
                if rested < spare && (day + s) % REST_CYCLE == 0 {
                    // Rotate the bench by day so every bench player gets turns.
                    *slot = LINEUP + first + (day + rested) % spare;
                    rested += 1;
                }
            }
        }
        let lineup: Vec<SimPlayer> = slots.iter().map(|&i| self.batters[i].1.clone()).collect();
        let ids: Vec<PlayerId> = slots.iter().map(|&i| self.batters[i].0).collect();
        // The bullpen in today's order: the setup men turn over, the closer
        // (last) stays.
        let rot = self.rotation;
        let pen: Vec<usize> = (rot..self.pitchers.len()).collect();
        let order: Vec<usize> = match (day, pen.len()) {
            (Some(day), n) if n > 2 => {
                let setup = n - 1;
                (0..setup)
                    .map(|i| pen[(i + day) % setup])
                    .chain(std::iter::once(pen[n - 1]))
                    .collect()
            }
            _ => pen,
        };
        let starter_limit = self.usage.starter_pitches;
        let inner: Option<Arc<dyn ManagerMaker>> = self.tables.as_ref().map(|(tables, log)| {
            Arc::new(
                TableManagerMaker::new(
                    Arc::clone(tables),
                    order.iter().map(|&i| self.pitchers[i].1).collect(),
                    Arc::clone(log),
                )
                .with_limits(starter_limit, self.usage.reliever_pitches),
            ) as Arc<dyn ManagerMaker>
        });
        // The set-up man: the best reliever but the closer, wherever today's
        // order puts him (game pitcher indices start at 1 for the bullpen).
        let setup = order
            .iter()
            .take(order.len().saturating_sub(1))
            .enumerate()
            .max_by(|a, b| {
                self.pitchers[*a.1]
                    .1
                    .total_cmp(&self.pitchers[*b.1].1)
                    .then(b.0.cmp(&a.0))
            })
            .map_or(0, |(i, _)| i as u8 + 1);
        let manager: Arc<dyn ManagerMaker> = Arc::new(UsageMaker {
            limit: starter_limit,
            relief: self.usage.reliever_pitches,
            rules: Arc::clone(&self.rules),
            setup,
            inner,
        });
        let bullpen: Vec<SimPlayer> = order.iter().map(|&i| self.pitchers[i].0.clone()).collect();
        let rotation: Vec<Team> = (0..rot)
            .map(|s| {
                let mut staff = vec![self.pitchers[s].0.clone()];
                staff.extend(bullpen.iter().cloned());
                Team::new(self.name.clone(), lineup.clone(), staff).expect("valid team")
            })
            .collect();
        (
            Club {
                name: self.name.clone(),
                rotation,
                manager: Some(manager),
            },
            ids,
            order,
        )
    }

    /// The regulars' club (playoffs and single-game replays).
    pub(crate) fn regulars(&self) -> Club {
        self.club_on(None).0
    }
}

/// The field manager's rule layer, over the table manager or the engine's
/// built-in rules. At every decision point the club's relief rules are
/// checked in order and the first that fires decides; otherwise the table
/// manager (if the club uses tables) decides, or else the era's pitch limits
/// and then the engine's own rules.
#[derive(Debug)]
struct UsageMaker {
    limit: u16,
    relief: u16,
    rules: Arc<[ReliefRule]>,
    /// Game pitcher index of the set-up man (0: none).
    setup: u8,
    inner: Option<Arc<dyn ManagerMaker>>,
}

impl ManagerMaker for UsageMaker {
    fn make(&self, slot: usize) -> Box<dyn Manager> {
        Box::new(UsageManager {
            limit: self.limit,
            relief: self.relief,
            rules: Arc::clone(&self.rules),
            setup: self.setup,
            inner: self.inner.as_ref().map(|m| m.make(slot)),
        })
    }
}

struct UsageManager {
    limit: u16,
    relief: u16,
    rules: Arc<[ReliefRule]>,
    setup: u8,
    inner: Option<Box<dyn Manager>>,
}

impl UsageManager {
    /// The first rule that fires, as a call (`None`: no rule fired, or the
    /// pitcher it names is not available).
    fn rule_call(&self, v: &GameView, half_start: bool) -> Option<ReliefCall> {
        if v.pitchers < 2 {
            return None;
        }
        let closer = v.pitchers - 1;
        let role = if v.pitcher == 0 {
            PitcherRole::Starter
        } else if v.pitcher == closer {
            PitcherRole::Closer
        } else {
            PitcherRole::Reliever
        };
        let lead = -v.bat_lead();
        let fresh = |i: u8| i > 0 && i < v.pitchers && i != v.pitcher && !v.used[usize::from(i)];
        for r in self.rules.iter() {
            let fires = (r.role == PitcherRole::Any || r.role == role)
                && (r.innings.0..=r.innings.1).contains(&v.inning)
                && (i32::from(r.lead.0)..=i32::from(r.lead.1)).contains(&lead)
                && v.pitches_in_outing >= r.min_pitches
                && v.runs_in_outing >= r.min_runs
                && (half_start || !r.inning_start);
            if !fires {
                continue;
            }
            let pick = match r.then {
                ReliefAction::Keep => return Some(ReliefCall::Keep),
                ReliefAction::Closer => Some(closer),
                ReliefAction::SetupMan => Some(self.setup),
                ReliefAction::NextReliever => (1..closer).find(|&i| fresh(i)),
            };
            if let Some(i) = pick.filter(|&i| fresh(i)) {
                return Some(ReliefCall::Bring(i));
            }
        }
        None
    }
}

impl Manager for UsageManager {
    fn steal(&mut self, v: &GameView, from: u8) -> StealCall {
        self.inner
            .as_mut()
            .map_or(StealCall::Default, |m| m.steal(v, from))
    }

    fn relief(&mut self, v: &GameView, half_start: bool) -> ReliefCall {
        if let Some(call) = self.rule_call(v, half_start) {
            return call;
        }
        if let Some(m) = self.inner.as_mut() {
            return m.relief(v, half_start);
        }
        let closer = v.pitchers.saturating_sub(1);
        let starter = v.pitcher == 0;
        // The engine's own pitch and run limits, which `Default` applies.
        let (engine_pitches, engine_runs) = match (starter, half_start) {
            (true, true) => (100, 6),
            (true, false) => (115, 8),
            (false, true) => (30, 3),
            (false, false) => (45, 5),
        };
        let limit = match (starter, half_start) {
            (true, true) => self.limit,
            (true, false) => self.limit + 15,
            (false, true) => self.relief,
            (false, false) => engine_pitches,
        };
        if v.pitchers < 3 || v.pitcher == closer {
            return ReliefCall::Default;
        }
        if v.pitches_in_outing < limit {
            // A limit above the engine's: keep him past the engine's pitch
            // count, but not past its run limit or into a save spot.
            let lead = -v.bat_lead();
            let save = half_start && v.inning >= 9 && (1..=3).contains(&lead);
            if v.pitches_in_outing >= engine_pitches && v.runs_in_outing < engine_runs && !save {
                return ReliefCall::Keep;
            }
            return ReliefCall::Default;
        }
        // The next fresh reliever in the bullpen order (never the closer).
        (1..closer)
            .find(|&i| !v.used[usize::from(i)])
            .map_or(ReliefCall::Default, ReliefCall::Bring)
    }

    fn observe(&mut self, v: &GameView) {
        if let Some(m) = self.inner.as_mut() {
            m.observe(v);
        }
    }

    fn game_over(&mut self, runs: [u16; 2]) {
        if let Some(m) = self.inner.as_mut() {
            m.game_over(runs);
        }
    }
}

/// Days in this year's regular season.
pub(crate) fn season_days(w: &World) -> usize {
    Schedule::generate(w.seed, w.year as u32, ROTATION)
        .days
        .len()
}

/// Keep `keep` of `list` (best first): all but the last `slots` places are
/// fixed; those places go in turn, by segment, to the next `pool` players
/// (the arms and bats shuttled between the majors and Triple-A).
fn shuttle(
    list: Vec<(PlayerId, f64)>,
    keep: usize,
    slots: usize,
    pool: usize,
    seg: Option<usize>,
) -> Vec<(PlayerId, f64)> {
    let fixed = keep.saturating_sub(slots);
    let pool = pool.min(list.len().saturating_sub(fixed));
    let Some(seg) = seg.filter(|_| slots > 0 && pool > slots) else {
        return list.into_iter().take(keep).collect();
    };
    let mut out: Vec<(PlayerId, f64)> = list[..fixed].to_vec();
    for j in 0..slots {
        out.push(list[fixed + (seg * slots + j) % pool]);
    }
    out
}

/// Last segments of the season (about September), when rosters expand.
const SEPTEMBER: usize = 4;

/// The healthiest, best batters and pitchers a club can field: nine
/// regulars and the bench, five starters and the bullpen. `out` is the
/// in-season injury list (absent means healthy); `None` takes each player's
/// own injury, for the playoffs. `seg` is the segment of the regular season
/// (`None` for the playoffs): it turns the shuttle and expands rosters in
/// September.
pub(crate) fn build_active(
    w: &World,
    team: usize,
    out: Option<&BTreeMap<PlayerId, u16>>,
    seg: Option<usize>,
) -> Active {
    let u = w.usage_of(team).clone();
    let september = seg.is_some_and(|s| s + SEPTEMBER >= SEGMENTS);
    let extra = if september {
        usize::from(u.september_extra)
    } else {
        0
    };
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
    // The regulars are the best nine by level then quality; the bench is the
    // next healthy batters. Each group is then ordered by quality.
    let bench_size = BENCH + extra;
    let bat_slots = usize::from(u.batter_shuttle);
    let mut picked = shuttle(
        pick(Kind::Batter, LINEUP + bench_size + 2 * bat_slots),
        LINEUP + bench_size,
        bat_slots,
        3 * bat_slots,
        seg,
    );
    let bench_at = LINEUP.min(picked.len());
    let mut bench = picked.split_off(bench_at);
    let mut bats = picked;
    desc(&mut bats);
    desc(&mut bench);
    bats.extend(bench);
    let bullpen = (usize::from(u.bullpen) + extra).clamp(1, MAX_PITCHERS - 1);
    let pit_slots = usize::from(u.pitcher_shuttle);
    let mut pit = shuttle(
        pick(Kind::Pitcher, ROTATION + bullpen + 2 * pit_slots),
        ROTATION + bullpen,
        pit_slots,
        3 * pit_slots,
        seg,
    );
    desc(&mut pit);
    let rotation = ROTATION.min(pit.len());
    let (rot, pen) = pit.split_at(rotation);
    let mut pen: Vec<(PlayerId, f64)> = pen.to_vec();
    pen.reverse(); // best last: the closer
    let tables = match state.tactics {
        Tactics::Baseline => None,
        Tactics::Tables => Some((Arc::clone(tables(w)), Arc::clone(&w.logs[team]))),
    };
    let sim = |id: PlayerId| -> SimPlayer {
        let p = &w.players[&id];
        SimPlayer {
            name: p.name.clone(),
            rates: w.model.rates(p.kind, &p.z),
        }
    };
    let staff: Vec<(PlayerId, f64)> = rot.iter().chain(pen.iter()).copied().collect();
    Active {
        bats: bats.iter().map(|b| b.0).collect(),
        staff: staff.iter().map(|s| s.0).collect(),
        name: state.name.clone(),
        rules: u.relief_rules.clone().into(),
        usage: u,
        tables,
        batters: bats.iter().map(|b| (b.0, sim(b.0))).collect(),
        pitchers: staff.iter().map(|s| (sim(s.0), s.1)).collect(),
        rotation,
    }
}

/// Whether `p` gets hurt in segment `seg`, which is `frac` of the season:
/// games out and permanent damage. Serious injuries strike at the same rate
/// in every era; short injured-list stints come on top at the era's rate.
fn injury_roll(w: &World, seg: usize, frac: f64, p: &Player) -> Option<(u16, f64)> {
    let mut rng = stream(
        w.seed,
        tag::INJURY,
        w.year as u64,
        (seg as u64) << 32 | u64::from(p.id.0),
    );
    let age = f64::from(p.age(w.year));
    // Per season (the rates were first set per sixth of a season).
    let base = match p.kind {
        Kind::Batter => 0.27,
        Kind::Pitcher => 0.36,
    };
    let aged = 1.0 + 0.04 * (age - 28.0).max(0.0);
    let hazard = base * frac * aged;
    let short = w.usage.short_stints.max(0.0) * frac * aged;
    let roll = uniform(&mut rng);
    if roll >= hazard + short {
        return None;
    }
    if roll >= hazard {
        // A short stint: the 10-day list and a little more.
        let games = libm::exp(libm::log(14.0) + 0.4 * normal(&mut rng));
        return Some((games.round().clamp(7.0, 40.0) as u16, 0.0));
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
            (sum - f64::from(b.pa) * v_lg + f64::from(b.pa) * replacement(Kind::Batter)) / 10.0
        }
        (Kind::Pitcher, Line::Pitch(p)) => {
            let saved = runs_per_out * f64::from(p.outs) - f64::from(p.r);
            (saved + f64::from(p.bf) * replacement(Kind::Pitcher)) / 10.0
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
        .map(|t| build_active(w, t, Some(&st.out), Some(seg)))
        .collect();
    let mut bat: BTreeMap<PlayerId, BatTotals> = BTreeMap::new();
    let mut pit: BTreeMap<PlayerId, PitchTotals> = BTreeMap::new();
    let mut results = Vec::new();
    for (offset, day) in schedule.days[from..to].iter().enumerate() {
        // Lineups are chosen per day (rest days), so the clubs are too.
        let built: Vec<(Club, Vec<PlayerId>, Vec<usize>)> = actives
            .iter()
            .map(|a| a.club_on(Some(from + offset)))
            .collect();
        let clubs: Vec<&Club> = built.iter().map(|b| &b.0).collect();
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
                    bat.entry(built[t].1[slot]).or_default().add(line);
                }
                for (i, line) in o.pitch[side].iter().enumerate() {
                    if line.used {
                        // Index 0 is the day's starter, then the bullpen in
                        // the day's order.
                        let staff = if i == 0 {
                            starters[side]
                        } else {
                            built[t].2[i - 1]
                        };
                        pit.entry(actives[t].staff[staff]).or_default().add(line);
                    }
                }
            }
        }
    }
    let mut lines: BTreeMap<PlayerId, Line> = BTreeMap::new();
    for a in &actives {
        // Every batter on the active list gets a line, even with no playing time.
        for id in &a.bats {
            let totals = bat.remove(id).unwrap_or_default();
            lines.insert(*id, Line::Bat(totals));
        }
        for id in &a.staff {
            if let Line::Pitch(p) = lines
                .entry(*id)
                .or_insert(Line::Pitch(PitchTotals::default()))
                && let Some(t) = pit.get(id)
            {
                p.merge(t);
            }
        }
    }
    let segment_end = seg_end.then(|| {
        // Time passes; then this segment's injuries strike.
        let days = (schedule.days.len() * (seg + 1) / SEGMENTS
            - schedule.days.len() * seg / SEGMENTS) as u16;
        let frac = f64::from(days) / schedule.days.len().max(1) as f64;
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
            if let Some((games, l)) = injury_roll(w, seg, frac, p) {
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
    let actives: Vec<Active> = (0..TEAMS)
        .map(|t| build_active(w, t, Some(out), Some(seg)))
        .collect();
    // The same day's lineups that `play_range` used for this game.
    let owned: Vec<Club> = actives
        .iter()
        .map(|a| a.club_on(Some(usize::from(r.day))).0)
        .collect();
    let clubs: Vec<&Club> = owned.iter().collect();
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
    (0..TEAMS)
        .map(|t| build_active(w, t, None, None).regulars())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The home side fields in the top of `inning` with a lead of `lead`;
    /// `pitcher` is in with `pitches` thrown this outing.
    fn view(inning: u8, lead: i32, pitcher: u8, pitches: u16) -> GameView {
        let mut used = [false; MAX_PITCHERS];
        used[usize::from(pitcher)] = true;
        GameView {
            inning,
            bat: 0,
            outs: 0,
            bases: [false; 3],
            runs: [10, (10 + lead) as u16],
            pitcher,
            pitchers: 9,
            pitches_in_outing: pitches,
            runs_in_outing: 0,
            used,
        }
    }

    fn manager(usage: &Usage) -> UsageManager {
        UsageManager {
            limit: usage.starter_pitches,
            relief: usage.reliever_pitches,
            rules: usage.relief_rules.clone().into(),
            setup: 7,
            inner: None,
        }
    }

    #[test]
    fn the_role_preset_brings_the_set_up_man_and_the_closer() {
        let mut m = manager(&Usage::roles());
        // Starters go to 110 pitches, past the engine's 100.
        assert_eq!(m.relief(&view(6, 0, 0, 105), true), ReliefCall::Keep);
        assert_eq!(m.relief(&view(6, 0, 0, 110), true), ReliefCall::Bring(1));
        assert_eq!(m.relief(&view(8, 2, 0, 80), true), ReliefCall::Bring(7));
        assert_eq!(m.relief(&view(9, 3, 7, 15), true), ReliefCall::Bring(8));
        // Not a save spot, and the starter is below his limit.
        assert_eq!(m.relief(&view(9, 5, 0, 80), true), ReliefCall::Default);
        // These rules wait for the start of an inning.
        assert_eq!(m.relief(&view(8, 2, 0, 80), false), ReliefCall::Default);
    }

    #[test]
    fn pitch_limits_lift_starters_and_relievers_and_a_keep_rule_wins() {
        let mut m = manager(&Usage::MODERN);
        assert_eq!(m.relief(&view(6, 0, 0, 89), true), ReliefCall::Default);
        assert_eq!(m.relief(&view(6, 0, 0, 90), true), ReliefCall::Bring(1));
        assert_eq!(m.relief(&view(7, 0, 2, 12), true), ReliefCall::Bring(1));
        let keep = ReliefRule {
            role: PitcherRole::Starter,
            innings: (1, 30),
            lead: (-30, 30),
            min_pitches: 0,
            min_runs: 0,
            inning_start: false,
            then: ReliefAction::Keep,
        };
        let usage = Usage {
            relief_rules: vec![keep],
            ..Usage::MODERN
        };
        let mut m = manager(&usage);
        assert_eq!(m.relief(&view(6, 0, 0, 120), true), ReliefCall::Keep);
    }
}
