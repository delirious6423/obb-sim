//! Trading. A club values a deal by what it does to its own roster over the
//! seasons it looks ahead: the thirteen best of each kind play, the rest are
//! depth, and every contract costs its salary. The club's direction changes
//! the weights: a contender prizes this season's wins, a rebuilding club the
//! seasons after and the money, and a club in debt counts money harder.
//!
//! In a real trade both sides gain in their own eyes (one needs pitching and
//! has batters to spare, one wants wins now and the other wants youth or
//! payroll room). So the proposer looks for packages of up to three players
//! a side that help it and that the other club should also want, judging the
//! other club's roster and direction from public facts (standings, payroll,
//! roster sizes) and its players by the proposer's own best estimates.
//!
//! Difficulty changes only how well this is done: a poorly informed general
//! manager misjudges players, overpays to be sure of a yes and accepts
//! lopsided offers (see `Profile::offer_buffer` and `accept_slack`).

use std::collections::BTreeSet;

use connect::GmAction;
use world::agents::{ClubFacts, GmObs, Held, IncomingOffer, Seen, TradeObs, stance_from_record};
use world::player::{Level, PlayerId};
use world::rating::Kind;
use world::scout::war_of;
use world::state::{MAJORS_PER_KIND, ROSTER_MAX_PER_KIND, Stance};

use crate::profile::Profile;
use crate::value::Context;

/// Longest look ahead, in seasons.
const MAX_H: usize = 7;
/// Share of a player's wins that counts by his rank in his kind: the
/// thirteen who play count fully, the next (Triple-A cover, who play when
/// others are hurt) partly, the rest barely.
const COVER: f64 = 0.4;
const DEPTH: f64 = 0.05;
/// Players on each side looked at per partner when building packages.
const CANDIDATES: usize = 7;
/// Most players on one side of a proposed package.
const PACKAGE: usize = 3;
/// Gain (millions) a proposer wants for each player beyond two in a deal:
/// bigger packages are harder to agree and to fit.
const PER_PLAYER: f64 = 1.0;

/// Packages per partner checked exactly after the quick screen.
const SHORTLIST: usize = 8;
/// How much a winning record raises the weight of this season's wins, and
/// lowers the weight of money, per unit of win percentage above .500.
const LENS_NOW: f64 = 5.0;
const LENS_MONEY: f64 = 2.5;
/// Weight of a dollar of salary for a .500 club: below the weight of a
/// dollar of wins, because a general manager is judged on wins and money
/// saved buys wins only if the budget is then spent well.
const MONEY: f64 = 0.5;

fn extra_players(n: usize) -> f64 {
    PER_PLAYER * n.saturating_sub(2) as f64
}

fn slot(kind: Kind) -> usize {
    usize::from(kind == Kind::Pitcher)
}

/// How a club weighs wins against money, season by season.
#[derive(Debug, Clone, Copy)]
pub struct Lens {
    /// Weight of this season's wins (later seasons weigh 1, discounted).
    now: f64,
    /// Weight of a dollar of salary.
    money: f64,
    discount: f64,
}

impl Lens {
    /// From the club's stated direction and last season's record: the better
    /// the club, the more this season's wins are worth to it (a pennant race)
    /// and the less the money; the worse, the more it looks to later seasons.
    #[must_use]
    pub fn of(stance: Stance, win_pct_prev: f64, in_debt: bool) -> Self {
        let lead = (win_pct_prev - 0.5).clamp(-0.15, 0.15);
        let push = match stance {
            Stance::Contend => 0.15,
            Stance::Balanced => 0.0,
            Stance::Rebuild => -0.15,
        };
        let mut l = Self {
            now: (1.0 + LENS_NOW * lead + push).clamp(0.5, 1.6),
            money: MONEY - LENS_MONEY * lead - 0.5 * push,
            discount: 0.9 - 0.5 * lead,
        };
        if in_debt {
            l.money += 0.6;
        }
        l
    }

    fn discount_at(&self, t: usize) -> f64 {
        (0..t).fold(1.0, |d, _| d * self.discount)
    }

    fn wins_at(&self, t: usize) -> f64 {
        self.discount_at(t) * if t == 0 { self.now } else { 1.0 }
    }
}

/// A player as one club values him in a trade: wins value (millions) in each
/// season he is under contract within the look ahead, and his salary.
#[derive(Debug, Clone, Copy)]
pub struct Piece {
    pub id: PlayerId,
    pub kind: Kind,
    pub salary: f64,
    n: usize,
    v: [f64; MAX_H],
}

impl Piece {
    #[must_use]
    pub fn new(ctx: &Context, s: &Seen, p: &Profile) -> Self {
        let e = ctx.estimate(s, p);
        let h = usize::from(p.horizon.clamp(1, MAX_H as u8));
        let n = usize::from(s.years.max(1)).min(h);
        let mut v = [0.0; MAX_H];
        for (t, x) in v.iter_mut().enumerate().take(n) {
            let q = ctx.quality_at(&e, t as u8, p);
            *x = (ctx.war_price * war_of(s.kind, q)).max(0.0);
        }
        Self {
            id: s.id,
            kind: s.kind,
            salary: s.salary,
            n,
            v,
        }
    }

    fn money(&self, lens: &Lens) -> f64 {
        (0..self.n)
            .map(|t| lens.discount_at(t) * lens.money * self.salary)
            .sum()
    }
}

fn rank_weight(r: usize) -> f64 {
    if r < MAJORS_PER_KIND {
        1.0
    } else if r < MAJORS_PER_KIND + Level::Triple.capacity() {
        COVER
    } else {
        DEPTH
    }
}

/// Wins value of one kind's players in one season, best first.
fn ranked(v: &[f64]) -> f64 {
    v.iter().enumerate().map(|(r, x)| rank_weight(r) * x).sum()
}

/// What a whole roster is worth to its club.
#[must_use]
pub fn roster_value<'a>(pieces: impl Iterator<Item = &'a Piece>, lens: &Lens) -> f64 {
    let mut vals: [[Vec<f64>; 2]; MAX_H] = Default::default();
    let mut money = 0.0;
    for pc in pieces {
        for (by_kind, v) in vals.iter_mut().zip(&pc.v).take(pc.n) {
            by_kind[slot(pc.kind)].push(*v);
        }

        money += pc.money(lens);
    }
    let mut total = 0.0;
    for (t, by_kind) in vals.iter_mut().enumerate() {
        for v in by_kind.iter_mut() {
            v.sort_by(|a, b| b.total_cmp(a));
            total += lens.wins_at(t) * ranked(v);
        }
    }
    total - money
}

/// Wins value of a roster in the coming season alone (no money).
#[must_use]
pub fn this_season<'a>(pieces: impl Iterator<Item = &'a Piece>) -> f64 {
    let mut vals: [Vec<f64>; 2] = Default::default();
    for pc in pieces {
        vals[slot(pc.kind)].push(pc.v[0]);
    }
    vals.iter_mut()
        .map(|v| {
            v.sort_by(|a, b| b.total_cmp(a));
            ranked(v)
        })
        .sum()
}

/// Wins value (millions) this season a club will give up in one trade: a
/// contender none (it buys, it does not sell), any other club whatever its
/// valuation allows.
fn now_slack(stance: Stance) -> f64 {
    match stance {
        Stance::Contend => 0.0,
        Stance::Balanced | Stance::Rebuild => f64::INFINITY,
    }
}

/// A club's roster, sorted per season and kind, for quick marginal values.
struct Book {
    lens: Lens,
    s: [[Vec<f64>; 2]; MAX_H],
}

impl Book {
    fn new(pieces: &[Piece], lens: Lens) -> Self {
        let mut s: [[Vec<f64>; 2]; MAX_H] = Default::default();
        for pc in pieces {
            for (by_kind, v) in s.iter_mut().zip(&pc.v).take(pc.n) {
                by_kind[slot(pc.kind)].push(*v);
            }
        }
        for by_kind in &mut s {
            for v in by_kind.iter_mut() {
                v.sort_by(|a, b| b.total_cmp(a));
            }
        }
        Self { lens, s }
    }

    /// Value of adding this player: he takes his rank and everyone below
    /// moves down one.
    fn add(&self, pc: &Piece) -> f64 {
        let mut g = 0.0;
        for t in 0..pc.n {
            let s = &self.s[t][slot(pc.kind)];
            let v = pc.v[t];
            let r = s.partition_point(|x| *x > v);
            let shift: f64 = s[r..]
                .iter()
                .enumerate()
                .map(|(i, x)| (rank_weight(r + i + 1) - rank_weight(r + i)) * x)
                .sum();
            g += self.lens.wins_at(t) * (rank_weight(r) * v + shift);
        }
        g - pc.money(&self.lens)
    }

    /// Value lost by giving this (own) player away: everyone below him
    /// moves up one.
    fn remove(&self, pc: &Piece) -> f64 {
        let mut g = 0.0;
        for t in 0..pc.n {
            let s = &self.s[t][slot(pc.kind)];
            let v = pc.v[t];
            let r = s.partition_point(|x| *x > v);
            let shift: f64 = s
                .iter()
                .enumerate()
                .skip(r + 1)
                .map(|(i, x)| (rank_weight(i - 1) - rank_weight(i)) * x)
                .sum();
            g += self.lens.wins_at(t) * (rank_weight(r) * v - shift);
        }
        g - pc.money(&self.lens)
    }
}

/// Counts of each kind and payroll of one side, to check the league's rules.
#[derive(Debug, Clone, Copy)]
struct Side {
    count: [usize; 2],
    payroll: f64,
    /// Highest payroll this side will go to by taking on salary.
    room: f64,
}

impl Side {
    /// Whether the side can send `out` and take `inn` (mirrors the league's
    /// trade check, plus the side's own payroll limit).
    fn fits(&self, out: &[Piece], inn: &[Piece]) -> bool {
        self.roster_fits(out, inn) && self.payroll_fits(out, inn)
    }

    fn roster_fits(&self, out: &[Piece], inn: &[Piece]) -> bool {
        for kind in [Kind::Batter, Kind::Pitcher] {
            let n = |v: &[Piece]| v.iter().filter(|p| p.kind == kind).count();
            let after = self.count[slot(kind)] + n(inn);
            let Some(after) = after.checked_sub(n(out)) else {
                return false;
            };
            if after > ROSTER_MAX_PER_KIND || (after < MAJORS_PER_KIND && n(inn) < n(out)) {
                return false;
            }
        }
        true
    }

    fn payroll_fits(&self, out: &[Piece], inn: &[Piece]) -> bool {
        let sal = |v: &[Piece]| v.iter().map(|p| p.salary).sum::<f64>();
        let net = sal(inn) - sal(out);
        net <= 1e-9 || self.payroll + net <= self.room + 1e-9
    }
}

/// The club's own record last season, from the public standings.
fn own_record(obs: &GmObs, t: &TradeObs) -> f64 {
    t.clubs
        .iter()
        .find(|f| f.club == obs.team)
        .map_or(0.5, |f| f.win_pct_prev)
}

/// The general manager's own side of the table.
fn my_side(obs: &GmObs) -> Side {
    let n = |k: Kind| obs.roster.iter().filter(|s| s.kind == k).count();
    // A club in debt takes on no salary; otherwise it stays within budget.
    let room = if obs.cash < 0.0 {
        obs.payroll
    } else {
        obs.budget.max(obs.payroll).min(obs.payroll_cap)
    };
    Side {
        count: [n(Kind::Batter), n(Kind::Pitcher)],
        payroll: obs.payroll,
        room,
    }
}

/// How an offer looks to the club it is put to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict {
    /// A player named is no longer where the offer says.
    Stale,
    /// The roster would break the league's limits.
    Roster,
    /// It adds salary the club cannot or will not carry.
    Payroll,
    /// It weakens this season's team of a contender.
    Weaker,
    /// Change in the club's value of its roster (millions).
    Gain(f64),
}

/// What an offer does for this club, as it sees it.
#[must_use]
pub fn judge(obs: &GmObs, t: &TradeObs, o: &IncomingOffer, p: &Profile, seed: u64) -> Verdict {
    let ctx = Context::new(obs, seed);
    // Players must still be where the offer says they are.
    let mine: BTreeSet<PlayerId> = obs.roster.iter().map(|s| s.id).collect();
    if !o.get.iter().all(|s| mine.contains(&s.id))
        || !o.give.iter().all(|s| {
            t.market
                .iter()
                .any(|h| h.club == o.from && h.seen.id == s.id)
        })
    {
        return Verdict::Stale;
    }
    let roster: Vec<Piece> = obs.roster.iter().map(|s| Piece::new(&ctx, s, p)).collect();
    let give: Vec<Piece> = o.give.iter().map(|s| Piece::new(&ctx, s, p)).collect();
    let get: Vec<Piece> = roster
        .iter()
        .filter(|pc| o.get.iter().any(|s| s.id == pc.id))
        .copied()
        .collect();
    let mut side = my_side(obs);
    side.room -= pending_salary(obs, t);
    if !side.roster_fits(&get, &give) {
        return Verdict::Roster;
    }
    if !side.payroll_fits(&get, &give) {
        return Verdict::Payroll;
    }
    let lens = Lens::of(obs.stance, own_record(obs, t), obs.cash < 0.0);
    let kept = || {
        roster
            .iter()
            .filter(|pc| !o.get.iter().any(|s| s.id == pc.id))
            .chain(give.iter())
    };
    if this_season(kept()) < this_season(roster.iter()) - now_slack(obs.stance) - 1e-9 {
        return Verdict::Weaker;
    }
    Verdict::Gain(roster_value(kept(), &lens) - roster_value(roster.iter(), &lens))
}

/// Answers to every offer put to this club: the best acceptable one is
/// accepted, then the rest are judged again against the roster that trade
/// leaves, and so on; whatever is left is declined.
#[must_use]
pub fn answers(obs: &GmObs, p: &Profile, seed: u64) -> Vec<(u32, bool)> {
    let Some(t) = &obs.trade else {
        return Vec::new();
    };
    let bar = p.personality.trade_margin - p.accept_slack();
    let mut now = obs.clone();
    let mut left: Vec<IncomingOffer> = t.incoming.clone();
    let mut out = Vec::new();
    while let Some(tr) = now.trade.as_ref() {
        let mut best: Option<(f64, usize)> = None;
        for (i, o) in left.iter().enumerate() {
            if let Verdict::Gain(g) = judge(&now, tr, o, p, seed)
                && g > bar
                && best.is_none_or(|(b, _)| g > b)
            {
                best = Some((g, i));
            }
        }
        let Some((_, i)) = best else {
            break;
        };
        let o = left.remove(i);
        out.push((o.id, true));
        take(&mut now, &o);
    }
    out.extend(left.iter().map(|o| (o.id, false)));
    out
}

/// The club's view after accepting `o`.
fn take(obs: &mut GmObs, o: &IncomingOffer) {
    let pay = |v: &[Seen]| v.iter().map(|s| s.salary).sum::<f64>();
    obs.payroll += pay(&o.give) - pay(&o.get);
    obs.roster.retain(|s| !o.get.iter().any(|g| g.id == s.id));
    obs.roster.extend(o.give.iter().copied());
    if let Some(t) = &mut obs.trade {
        t.market
            .retain(|h| !o.give.iter().any(|g| g.id == h.seen.id));
        t.market.extend(o.get.iter().map(|s| Held {
            club: o.from,
            seen: *s,
        }));
    }
}

/// An offer already made: partner, players given, players asked for.
pub type Sent = (u8, Vec<PlayerId>, Vec<PlayerId>);

/// A package found for one partner.
#[derive(Debug, Clone)]
pub struct Deal {
    pub partner: u8,
    pub give: Vec<PlayerId>,
    pub get: Vec<PlayerId>,
    /// Gain as the proposer sees it, and as it expects the partner to.
    pub mine: f64,
    pub theirs: f64,
    /// Salary (millions a year) the proposer takes on, net.
    pub salary_in: f64,
}

fn subsets(n: usize) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new()];
    for i in 0..n {
        let len = out.len();
        for j in 0..len {
            if out[j].len() < PACKAGE {
                let mut s = out[j].clone();
                s.push(i);
                out.push(s);
            }
        }
    }
    out
}

/// What the proposer knows when it looks for deals.
struct Search<'a> {
    t: &'a TradeObs,
    ctx: Context,
    p: &'a Profile,
    roster: Vec<Piece>,
    /// Own players that may be offered (major league and Triple-A).
    tradeable: Vec<Piece>,
    book: Book,
    me: Side,
    keep_now: f64,
    cap: f64,
    /// Gain this club needs, and gain it wants the partner to see.
    margin: f64,
    bar: f64,
    subs: Vec<Vec<usize>>,
}

/// The best deal this club can put to each partner, best for itself first.
///
/// The partner is assumed to judge players as this club does (the club's
/// best guess of the truth) but through its own roster and direction, read
/// from the standings: what it gains comes from fit and from wanting wins
/// now or later, not from the proposer's private opinions.
#[must_use]
pub fn deals(obs: &GmObs, t: &TradeObs, p: &Profile, seed: u64, sent: &[Sent]) -> Vec<Deal> {
    let ctx = Context::new(obs, seed);
    let roster: Vec<Piece> = obs.roster.iter().map(|s| Piece::new(&ctx, s, p)).collect();
    let tradeable = obs
        .roster
        .iter()
        .zip(&roster)
        .filter(|(s, _)| s.level <= Level::Triple)
        .map(|(_, pc)| *pc)
        .collect();
    let lens = Lens::of(obs.stance, own_record(obs, t), obs.cash < 0.0);
    let search = Search {
        t,
        ctx,
        p,
        book: Book::new(&roster, lens),
        roster,
        tradeable,
        me: my_side(obs),
        keep_now: now_slack(obs.stance),
        cap: obs.payroll_cap,
        margin: p.personality.trade_margin,
        bar: p.personality.trade_margin - p.accept_slack() + p.offer_buffer(),
        subs: subsets(CANDIDATES),
    };
    let mut out: Vec<Deal> = t
        .clubs
        .iter()
        .filter(|f| f.club != obs.team && !t.outgoing.iter().any(|o| o.to == f.club))
        .filter_map(|f| search.with(f, sent))
        .collect();
    out.sort_by(|a, b| b.mine.total_cmp(&a.mine).then(a.partner.cmp(&b.partner)));
    out
}

impl Search<'_> {
    fn with(&self, f: &ClubFacts, sent: &[Sent]) -> Option<Deal> {
        let their_lens = Lens::of(
            stance_from_record(f.win_pct_prev),
            f.win_pct_prev,
            f.in_debt,
        );
        let theirs: Vec<Piece> = self
            .t
            .market
            .iter()
            .filter(|h| h.club == f.club)
            .map(|h| Piece::new(&self.ctx, &h.seen, self.p))
            .collect();
        let their_book = Book::new(&theirs, their_lens);
        let them = Side {
            count: [usize::from(f.batters), usize::from(f.pitchers)],
            payroll: f.payroll,
            // A club in debt may take on no salary.
            room: if f.in_debt { f.payroll } else { self.cap },
        };
        // Players worth more on the other side than here, best first:
        // (joint gain, value to me, value to them, player).
        let rank = |mut v: Vec<(f64, f64, f64, Piece)>| {
            v.sort_by(|x, y| y.0.total_cmp(&x.0).then(x.3.id.cmp(&y.3.id)));
            v.truncate(CANDIDATES);
            v
        };
        let give = rank(
            self.tradeable
                .iter()
                .map(|pc| {
                    let (a, b) = (their_book.add(pc), self.book.remove(pc));
                    (a - b, b, a, *pc)
                })
                .collect(),
        );
        let get = rank(
            theirs
                .iter()
                .map(|pc| {
                    let (a, b) = (self.book.add(pc), their_book.remove(pc));
                    (a - b, a, b, *pc)
                })
                .collect(),
        );
        // Quick screen of every package by summed marginal values.
        let mut short: Vec<(f64, &Vec<usize>, &Vec<usize>)> = Vec::new();
        for g in self
            .subs
            .iter()
            .filter(|g| g.iter().all(|&i| i < give.len()))
        {
            for r in self
                .subs
                .iter()
                .filter(|r| r.iter().all(|&i| i < get.len()))
            {
                if g.is_empty() && r.is_empty() {
                    continue;
                }
                let mine: f64 = r.iter().map(|&i| get[i].1).sum::<f64>()
                    - g.iter().map(|&i| give[i].1).sum::<f64>();
                let other: f64 = g.iter().map(|&i| give[i].2).sum::<f64>()
                    - r.iter().map(|&i| get[i].2).sum::<f64>();
                let mine = mine - extra_players(g.len() + r.len());
                if mine > self.margin && other > self.bar {
                    short.push((mine, g, r));
                }
            }
        }
        short.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then(a.1.len().cmp(&b.1.len()))
                .then(a.2.len().cmp(&b.2.len()))
                .then(a.1.cmp(b.1))
                .then(a.2.cmp(b.2))
        });
        // Exact check of the best few: whole rosters before and after.
        let before_me = roster_value(self.roster.iter(), &self.book.lens);
        let before_them = roster_value(theirs.iter(), &their_lens);
        let now_me = this_season(self.roster.iter());
        let now_them = this_season(theirs.iter());
        let them_keep = now_slack(stance_from_record(f.win_pct_prev));
        let not_in = |v: &[Piece], pc: &Piece| !v.iter().any(|x| x.id == pc.id);
        let mut checked = 0;
        for (_, g, r) in short {
            let out: Vec<Piece> = g.iter().map(|&i| give[i].3).collect();
            let inn: Vec<Piece> = r.iter().map(|&i| get[i].3).collect();
            let ids = |v: &[Piece]| v.iter().map(|x| x.id).collect::<Vec<_>>();
            if !self.me.fits(&out, &inn)
                || !them.fits(&inn, &out)
                || sent.contains(&(f.club, ids(&out), ids(&inn)))
            {
                continue;
            }
            if checked >= SHORTLIST {
                break;
            }
            checked += 1;
            let mine_after = || {
                self.roster
                    .iter()
                    .filter(|pc| not_in(&out, pc))
                    .chain(inn.iter())
            };
            let their_after = || {
                theirs
                    .iter()
                    .filter(|pc| not_in(&inn, pc))
                    .chain(out.iter())
            };
            if this_season(mine_after()) < now_me - self.keep_now - 1e-9
                || this_season(their_after()) < now_them - them_keep - 1e-9
            {
                continue;
            }
            let mine = roster_value(
                self.roster
                    .iter()
                    .filter(|pc| not_in(&out, pc))
                    .chain(inn.iter()),
                &self.book.lens,
            ) - before_me;
            let other = roster_value(
                theirs
                    .iter()
                    .filter(|pc| not_in(&inn, pc))
                    .chain(out.iter()),
                &their_lens,
            ) - before_them;
            let mine = mine - extra_players(out.len() + inn.len());
            if mine > self.margin && other > self.bar {
                let sal = |v: &[Piece]| v.iter().map(|x| x.salary).sum::<f64>();
                return Some(Deal {
                    partner: f.club,
                    give: out.iter().map(|x| x.id).collect(),
                    get: inn.iter().map(|x| x.id).collect(),
                    mine,
                    theirs: other,
                    salary_in: sal(&inn) - sal(&out),
                });
            }
        }
        None
    }
}

/// Up to `max` proposals: the best deals, one per partner, no player twice,
/// and no more salary taken on, if all are accepted, than `room` allows.
#[must_use]
pub fn proposals(
    deals: Vec<Deal>,
    max: usize,
    mut room: f64,
    used: &mut Vec<PlayerId>,
) -> Vec<GmAction> {
    let mut out = Vec::new();
    for d in deals {
        if out.len() >= max {
            break;
        }
        if d.give.iter().chain(&d.get).any(|id| used.contains(id))
            || (d.salary_in > 1e-9 && d.salary_in > room)
        {
            continue;
        }
        room -= d.salary_in.max(0.0);
        used.extend(d.give.iter().chain(&d.get).copied());
        out.push(GmAction::ProposeTrade {
            partner: d.partner,
            give: d.give,
            get: d.get,
        });
    }
    out
}

/// Salary (millions a year) this club's open offers would add if accepted.
fn pending_salary(obs: &GmObs, t: &TradeObs) -> f64 {
    let pay = |id: &PlayerId| {
        obs.roster
            .iter()
            .chain(t.market.iter().map(|h| &h.seen))
            .find(|s| s.id == *id)
            .map_or(0.0, |s| s.salary)
    };
    t.outgoing
        .iter()
        .map(|o| {
            (o.get.iter().map(pay).sum::<f64>() - o.give.iter().map(pay).sum::<f64>()).max(0.0)
        })
        .sum()
}

/// Salary (millions a year) this club may still take on by trading, after
/// what its open offers would add if accepted.
#[must_use]
pub fn salary_room(obs: &GmObs, t: &TradeObs) -> f64 {
    let me = my_side(obs);
    (me.room - me.payroll - pending_salary(obs, t)).max(0.0)
}
