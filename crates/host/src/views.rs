//! Role-filtered views of the league, built for one club's general manager.
//! Grades come from that club's own (noisy) scouting; prices and stats are
//! public; true talent is never read.

use season::{Schedule, division_of, league_of};
use world::agents::{ClubBooks, GmView, LeagueObs, OwnerObs, Seen, last_crowd, owner_ticket_for};
use world::player::{Level as WLevel, Origin, Player, PlayerId};
use world::rating::Kind;
use world::state::{Phase, Rules, Stance as WStance, TEAMS, World};

use crate::api::{
    ClubBooksRow, Division, DraftView, EconomyView, FinanceRow, FinancesView, GameRow, LeagueView,
    Level, OfferView, OwnerHistoryRow, OwnerView, PlayerRow, PlayerView, Position, RosterView,
    RuleLimits, RulesView, ScheduleView, StadiumView, Stage, Stance, StandingRow, Standings,
    TradeView,
};

pub(crate) fn level(l: WLevel) -> Level {
    match l {
        WLevel::Majors => Level::Majors,
        WLevel::Triple => Level::Triple,
        WLevel::Double => Level::Double,
        WLevel::Single => Level::Single,
    }
}

pub(crate) fn wlevel(l: Level) -> WLevel {
    match l {
        Level::Majors => WLevel::Majors,
        Level::Triple => WLevel::Triple,
        Level::Double => WLevel::Double,
        Level::Single => WLevel::Single,
    }
}

fn origin(o: Origin) -> &'static str {
    match o {
        Origin::HighSchool => "High school",
        Origin::College => "College",
        Origin::International => "International",
    }
}

/// A scouting grade on the 20-80 scale from a quality in runs per plate
/// appearance.
fn grade(w: &World, kind: Kind, q: f64) -> u8 {
    let sd = w.model.quality_sd(kind).max(1e-9);
    (50.0 + 10.0 * q / sd).round().clamp(20.0, 80.0) as u8
}

fn stat_text(w: &World, p: &Player) -> String {
    let Some(st) = w.season.as_ref() else {
        return String::new();
    };
    match st.lines.get(&p.id) {
        Some(world::command::Line::Bat(b)) if b.ab > 0 => format!(
            ".{:03} {} HR {} RBI",
            (1000.0 * f64::from(b.h) / f64::from(b.ab)).round() as u32,
            b.hr,
            b.rbi
        ),
        Some(world::command::Line::Pitch(p)) if p.outs > 0 => format!(
            "{:.2} ERA {}.{} IP {} SO",
            27.0 * f64::from(p.er) / f64::from(p.outs),
            p.outs / 3,
            p.outs % 3,
            p.so
        ),
        _ => String::new(),
    }
}

/// One player as club `me` sees him.
pub(crate) fn row(w: &World, me: u8, id: PlayerId) -> PlayerRow {
    let p = &w.players[&id];
    let s: Seen = GmView::new(w, me).see_player(id);
    PlayerRow {
        id: id.0,
        name: p.name.clone(),
        position: if p.kind == Kind::Batter {
            Position::Batter
        } else {
            Position::Pitcher
        },
        age: s.age,
        level: level(p.level),
        origin: origin(p.origin).to_string(),
        grade_now: grade(w, p.kind, s.now),
        grade_peak: grade(w, p.kind, s.peak),
        salary: s.salary,
        years: s.years,
        ask: s.ask,
        ask_years: s.ask_years,
        demand: s.demand,
        out_games: w
            .season
            .as_ref()
            .and_then(|st| st.out.get(&id).copied())
            .unwrap_or(p.out_games),
        club: p.team,
        stat: stat_text(w, p),
    }
}

pub(crate) fn rows(w: &World, me: u8, ids: impl IntoIterator<Item = PlayerId>) -> Vec<PlayerRow> {
    ids.into_iter().map(|id| row(w, me, id)).collect()
}

pub(crate) fn roster(w: &World, me: u8, club: u8) -> RosterView {
    let c = &w.clubs[usize::from(club)];
    let mut players = rows(w, me, c.players.iter().copied());
    players.sort_by(|a, b| {
        a.level
            .cmp(&b.level)
            .then(b.grade_now.cmp(&a.grade_now))
            .then(a.id.cmp(&b.id))
    });
    RosterView {
        club,
        club_name: c.name.clone(),
        own: club == me,
        players,
        payroll: w.payroll(usize::from(club)),
        budget: c.budget,
        problems: if club == me && w.phase != Phase::Unfounded {
            w.roster_problems(club)
        } else {
            Vec::new()
        },
    }
}

pub(crate) fn player(w: &World, me: u8, id: u32) -> Option<PlayerView> {
    let pid = PlayerId(id);
    let p = w
        .players
        .get(&pid)
        .or_else(|| w.retired.get(&pid).map(|(p, _)| p))?;
    let live = w.players.contains_key(&pid);
    let mut r = if live {
        row(w, me, pid)
    } else {
        PlayerRow {
            id,
            name: p.name.clone(),
            position: if p.kind == Kind::Batter {
                Position::Batter
            } else {
                Position::Pitcher
            },
            age: 0,
            level: Level::Single,
            origin: origin(p.origin).to_string(),
            grade_now: 20,
            grade_peak: 20,
            salary: 0.0,
            years: 0,
            ask: 0.0,
            ask_years: 0,
            demand: 0.0,
            out_games: 0,
            club: None,
            stat: String::new(),
        }
    };
    let mut history = Vec::new();
    if let Some(d) = p.drafted {
        history.push(format!("Drafted in {} (pick {})", d.year, d.pick));
    }
    if let Some(y) = p.debut {
        history.push(format!("Major-league debut in {y}"));
    }
    if let Some((_, y)) = w.retired.get(&pid) {
        history.push(format!("Retired in {y}"));
        r.age = p.age(*y);
    }
    Some(PlayerView {
        club_name: p.team.map(|t| w.clubs[usize::from(t)].name.clone()),
        drafted: p.drafted.map(|d| (d.year, d.pick)),
        career_seasons: p.career.seasons,
        career_war: p.career.war,
        history,
        row: r,
    })
}

pub(crate) fn standings(w: &World) -> Standings {
    let recs: Vec<_> = if let Some(st) = w.season.as_ref().filter(|s| s.day > 0) {
        st.records.clone()
    } else if w.last_records.len() == TEAMS {
        w.last_records.clone()
    } else {
        vec![season::Record::default(); w.clubs.len()]
    };
    let leagues = ["Harbor", "Prairie"];
    let places = ["East", "Central", "West"];
    let mut divisions = Vec::new();
    for d in 0..(TEAMS / 5) {
        let mut rows: Vec<StandingRow> = (0..TEAMS)
            .filter(|t| division_of(*t) == d)
            .map(|t| {
                let r = recs.get(t).copied().unwrap_or_default();
                StandingRow {
                    club: t as u8,
                    name: w.clubs[t].name.clone(),
                    w: r.w,
                    l: r.l,
                    rs: r.rs,
                    ra: r.ra,
                }
            })
            .collect();
        rows.sort_by(|a, b| {
            b.w.cmp(&a.w)
                .then((i64::from(b.rs) - i64::from(b.ra)).cmp(&(i64::from(a.rs) - i64::from(a.ra))))
                .then(a.club.cmp(&b.club))
        });
        let first = d * 5;
        divisions.push(Division {
            name: format!("{} League {}", leagues[league_of(first) % 2], places[d % 3]),
            rows,
        });
    }
    Standings {
        year: w.year,
        divisions,
        champion: w
            .history
            .last()
            .filter(|h| h.year == w.year)
            .and_then(|h| h.champion)
            .map(|c| w.clubs[usize::from(c)].name.clone()),
    }
}

/// The most recent champion's name and year.
pub(crate) fn last_champion(w: &World) -> Option<(i32, String)> {
    let h = w.history.last()?;
    let c = h.champion?;
    Some((h.year, w.clubs[usize::from(c)].name.clone()))
}

pub(crate) fn schedule(w: &World, club: u8) -> ScheduleView {
    let sched = Schedule::generate(w.seed, w.year as u32, 5);
    let results = w.season.as_ref().map(|s| &s.results);
    let mut games = Vec::new();
    for (day, fixtures) in sched.days.iter().enumerate() {
        for f in fixtures {
            if f.home != club && f.away != club {
                continue;
            }
            let home = f.home == club;
            let other = if home { f.away } else { f.home };
            let result = results
                .and_then(|r| r.iter().find(|g| g.number == f.number))
                .map(|g| {
                    if home {
                        (g.home_runs, g.away_runs)
                    } else {
                        (g.away_runs, g.home_runs)
                    }
                });
            games.push(GameRow {
                game: f.number,
                day: day as u16,
                home,
                opponent: w.clubs[usize::from(other)].name.clone(),
                result,
            });
        }
    }
    ScheduleView {
        club,
        club_name: w.clubs[usize::from(club)].name.clone(),
        year: w.year,
        games,
    }
}

pub(crate) fn draft(w: &World, me: u8) -> DraftView {
    let Some(d) = w.draft.as_ref() else {
        return DraftView {
            open: false,
            year: w.year,
            on_the_clock: false,
            pick: 0,
            picks: 0,
            international: false,
            ceiling: 0.0,
            clock: None,
            prospects: Vec::new(),
            made: Vec::new(),
        };
    };
    let view = GmView::new(w, me);
    let on = view.observe().draft;
    let international = d.next >= d.amateur_picks;
    let ceiling = on.as_ref().map_or(0.0, |o| o.ceiling);
    let mut prospects = rows(w, me, d.remaining.iter().copied());
    prospects.retain(|p| (p.origin == "International") == international);
    prospects.sort_by(|a, b| {
        (b.grade_now + b.grade_peak)
            .cmp(&(a.grade_now + a.grade_peak))
            .then(a.id.cmp(&b.id))
    });
    let made = w
        .players
        .values()
        .filter_map(|p| {
            let dr = p.drafted?;
            (dr.year == d.year).then(|| {
                (
                    usize::from(dr.pick),
                    p.team
                        .map_or_else(String::new, |t| w.clubs[usize::from(t)].name.clone()),
                    p.name.clone(),
                )
            })
        })
        .collect::<Vec<_>>();
    let mut made = made;
    made.sort();
    DraftView {
        open: true,
        year: d.year,
        on_the_clock: on.is_some(),
        pick: d.next,
        picks: d.order.len(),
        international,
        ceiling,
        clock: d
            .order
            .get(d.next)
            .map(|t| w.clubs[usize::from(*t)].name.clone()),
        prospects,
        made,
    }
}

pub(crate) fn finances(w: &World, me: u8) -> FinancesView {
    let c = &w.clubs[usize::from(me)];
    let history = w
        .finances
        .get(usize::from(me))
        .map(|f| {
            f.iter()
                .enumerate()
                .map(|(i, x)| FinanceRow {
                    year: w.history.get(i).map_or(w.year, |h| h.year),
                    revenue: x.revenue,
                    payroll: x.payroll,
                    other: x.other,
                    spent: x.spent,
                    dividend: x.dividend,
                    attendance: x.attendance,
                    gate: x.gate,
                })
                .collect()
        })
        .unwrap_or_default();
    let mut contracts = rows(
        w,
        me,
        c.players
            .iter()
            .copied()
            .filter(|id| w.players[id].contract.is_some()),
    );
    contracts.sort_by(|a, b| b.salary.total_cmp(&a.salary).then(a.id.cmp(&b.id)));
    FinancesView {
        club_name: c.name.clone(),
        cash: c.cash,
        budget: c.budget,
        payroll: w.payroll(usize::from(me)),
        payroll_floor: w.rules.payroll_floor,
        payroll_cap: w.rules.payroll_cap,
        revenue_prev: c.revenue_prev,
        history,
        contracts,
    }
}

pub(crate) fn trades(w: &World, me: u8) -> TradeView {
    let open = matches!(
        w.phase,
        Phase::FreeAgency | Phase::Progressed | Phase::Drafting
    );
    let offer = |o: &world::state::TradeOffer| OfferView {
        id: o.id,
        from: o.from,
        from_name: w.clubs[usize::from(o.from)].name.clone(),
        give: rows(w, me, o.give.iter().copied()),
        get: rows(w, me, o.get.iter().copied()),
    };
    let incoming = w.offers.iter().filter(|o| o.to == me).map(offer).collect();
    let outgoing = w
        .offers
        .iter()
        .filter(|o| o.from == me)
        .map(offer)
        .collect();
    let mut market: Vec<PlayerRow> = Vec::new();
    for (t, c) in w.clubs.iter().enumerate() {
        if t == usize::from(me) {
            continue;
        }
        market.extend(rows(
            w,
            me,
            c.players
                .iter()
                .copied()
                .filter(|id| w.players[id].level <= WLevel::Triple),
        ));
    }
    market.sort_by(|a, b| b.grade_now.cmp(&a.grade_now).then(a.id.cmp(&b.id)));
    market.truncate(400);
    TradeView {
        open,
        incoming,
        outgoing,
        market,
    }
}

pub(crate) fn stance_view(s: WStance) -> Stance {
    match s {
        WStance::Contend => Stance::Contend,
        WStance::Balanced => Stance::Balanced,
        WStance::Rebuild => Stance::Rebuild,
    }
}

pub(crate) fn stance_world(s: Stance) -> WStance {
    match s {
        Stance::Contend => WStance::Contend,
        Stance::Balanced => WStance::Balanced,
        Stance::Rebuild => WStance::Rebuild,
    }
}

pub(crate) fn rules_view(r: &Rules) -> RulesView {
    RulesView {
        payroll_floor: r.payroll_floor,
        payroll_cap: r.payroll_cap,
        revenue_sharing: r.revenue_sharing,
        war_price: r.war_price,
        draft_rounds: r.draft_rounds,
        international_rounds: r.international_rounds,
        economy: EconomyView {
            scale: r.scale,
            inflation: r.inflation,
            tax_line: r.tax_line,
            tax_rate: r.tax_rate,
        },
    }
}

/// The rules a commissioner asks for. The price level is not theirs to set:
/// it is taken from the rules in force (`now`).
pub(crate) fn rules_world(v: &RulesView, now: &Rules) -> Rules {
    Rules {
        payroll_floor: v.payroll_floor,
        payroll_cap: v.payroll_cap,
        revenue_sharing: v.revenue_sharing,
        war_price: v.war_price,
        draft_rounds: v.draft_rounds,
        international_rounds: v.international_rounds,
        scale: now.scale,
        inflation: v.economy.inflation,
        tax_line: v.economy.tax_line,
        tax_rate: v.economy.tax_rate,
    }
}

/// The owner's desk for one club.
pub(crate) fn owner_desk(w: &World, me: u8, stage: Stage, can_act: bool) -> OwnerView {
    let c = &w.clubs[usize::from(me)];
    let o = OwnerObs::new(w, me);
    let history = w
        .finances
        .get(usize::from(me))
        .map(|f| {
            f.iter()
                .enumerate()
                .map(|(i, x)| OwnerHistoryRow {
                    year: w.history.get(i).map_or(w.year, |h| h.year),
                    attendance: x.attendance,
                    gate: x.gate,
                    revenue: x.revenue,
                    payroll: x.payroll,
                })
                .collect()
        })
        .unwrap_or_default();
    OwnerView {
        club_name: c.name.clone(),
        year: w.year,
        stage,
        can_act,
        budget: c.budget,
        budget_limit: w.budget_limit(usize::from(me)),
        payroll: w.payroll(usize::from(me)),
        cash: c.cash,
        revenue_prev: c.revenue_prev,
        stadium: StadiumView {
            capacity_k: c.stadium.capacity,
            quality: c.stadium.quality,
            ticket: c.stadium.ticket,
        },
        attendance_prev: o.attendance_prev,
        gate_prev: o.gate_prev,
        sold_out: o.sold_out(),
        suggested_ticket: owner_ticket_for(&o),
        ticket_range: w.ticket_range(),
        build_cost: format!(
            "each ${:.2}M adds 0.05k seats and 1/600 quality, paid from cash (cash may not go below zero)",
            w.scale()
        ),
        stance: stance_view(c.stance),
        history,
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

/// The league office: the rules and every club's public books.
pub(crate) fn league_office(w: &World, stage: Stage, can_act: bool) -> LeagueView {
    let l = LeagueObs::new(w);
    let books: Vec<ClubBooksRow> = l
        .clubs
        .iter()
        .map(|b: &ClubBooks| {
            let t = usize::from(b.team);
            let c = &w.clubs[t];
            ClubBooksRow {
                club: b.team,
                name: c.name.clone(),
                market: b.market,
                payroll: b.payroll,
                cash: b.cash,
                revenue_prev: b.revenue_prev,
                ticket: c.stadium.ticket,
                attendance_prev: last_crowd(w, b.team).0,
                budget: c.budget,
            }
        })
        .collect();
    LeagueView {
        year: w.year,
        stage,
        rules: rules_view(&w.rules),
        limits: RuleLimits {
            payroll_floor: (40.0 * w.scale(), 100.0 * w.scale()),
            payroll_cap: (w.rules.payroll_floor + 40.0 * w.scale(), 420.0 * w.scale()),
            revenue_sharing: (0.0, 0.4),
            war_price: (
                world::state::WAR_PRICE_RANGE.0 * w.scale(),
                world::state::WAR_PRICE_RANGE.1 * w.scale(),
            ),
        },
        can_act,
        total_payroll: books.iter().map(|b| b.payroll).sum(),
        total_revenue_prev: books.iter().map(|b| b.revenue_prev).sum(),
        total_cash: books.iter().map(|b| b.cash).sum(),
        median_payroll: median(books.iter().map(|b| b.payroll).collect()),
        median_cash: median(books.iter().map(|b| b.cash).collect()),
        low_cash: books.iter().filter(|b| b.cash < 0.0).count(),
        clubs: books,
    }
}
