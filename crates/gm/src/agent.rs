use connect::{GmAction, GmAgent, GmStage, GmTurn};
use world::agents::{GmObs, Held, Seen, set_levels};
use world::player::{Level, Origin};
use world::rating::Kind;
use world::state::{CREDIT, MAJORS_PER_KIND, ROSTER_MAX_PER_KIND, Stance};

use crate::profile::{Profile, TradeMode};
use crate::value::{Context, Est};

/// Quality gain over the thirteenth man a free agent must offer.
const UPGRADE_MARGIN: f64 = 0.0015;
/// Millions of apparent loss, by the market's view, the other side is
/// assumed to put up with, plus this share of the gain the proposer claims
/// (much of a claimed gain is the proposer's own error, which the other
/// side's different error can flip).
const FAIRNESS_SLACK: f64 = 2.0;
const FAIRNESS_SHARE: f64 = 0.6;
/// Quality (runs per PA) a trade may cost if it saves enough money.
const GIVE_UP_QUALITY: f64 = 0.004;
/// Quality the other side should see itself gaining, by the market's view.
const OTHER_SIDE_STEP: f64 = 0.002;
/// Proposals per trade turn.
const PROPOSALS: usize = 2;

/// A general manager that values players by surplus over their contracts.
#[derive(Debug, Clone)]
pub struct SurplusGm {
    seed: u64,
    profile: Profile,
}

impl SurplusGm {
    /// `seed` fixes this manager's own judgement errors (same seed, same
    /// mistakes); give each club a different one.
    #[must_use]
    pub fn new(seed: u64, profile: Profile) -> Self {
        Self { seed, profile }
    }

    #[must_use]
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    fn draft(&self, obs: &GmObs) -> Vec<GmAction> {
        let Some(d) = &obs.draft else {
            return Vec::new();
        };
        let ctx = Context::new(obs, self.seed);
        // The owner's stance moves the taste for youth: win now or build.
        let shift = match obs.stance {
            Stance::Contend => -0.2,
            Stance::Balanced => 0.0,
            Stance::Rebuild => 0.2,
        };
        let youth = (self.profile.personality.youth + shift).clamp(0.0, 1.0);
        let mut best: Option<(f64, Seen)> = None;
        for s in &d.prospects {
            if (s.origin == Origin::International) != d.international
                || s.demand > d.ceiling
                || obs.cash - s.demand < -CREDIT
                || obs.in_debt()
            {
                continue;
            }
            let e = ctx.estimate(s, &self.profile);
            let score = youth * e.peak + (1.0 - youth) * e.q;
            if best.is_none_or(|(b, bs)| (score, bs.id) > (b, s.id)) {
                best = Some((score, *s));
            }
        }
        match best {
            Some((_, s)) => vec![GmAction::Draft { player: s.id }],
            None => vec![GmAction::Pass],
        }
    }

    fn free_agency(&self, obs: &GmObs) -> Vec<GmAction> {
        let ctx = Context::new(obs, self.seed);
        let p = &self.profile;
        let mut best: Option<(f64, Est, u8)> = None;
        let mut fallback: Option<(f64, Est, u8)> = None;
        for kind in [Kind::Batter, Kind::Pitcher] {
            let mut mine: Vec<f64> = obs
                .roster
                .iter()
                .filter(|s| s.kind == kind)
                .map(|s| ctx.mean_quality(&ctx.estimate(s, p), p))
                .collect();
            mine.sort_by(|a, b| b.total_cmp(a));
            if mine.len() >= ROSTER_MAX_PER_KIND {
                continue;
            }
            let short = mine.len() < MAJORS_PER_KIND;
            let bar = if short {
                f64::NEG_INFINITY
            } else {
                mine[MAJORS_PER_KIND - 1] + UPGRADE_MARGIN
            };
            for f in obs.free_agents.iter().filter(|f| f.kind == kind) {
                let e = ctx.estimate(f, p);
                if ctx.mean_quality(&e, p) <= bar {
                    continue;
                }
                let years = ctx.best_years(&e, p);
                let price = f.ask;
                if !obs.may_sign(kind, price) {
                    continue;
                }
                let surplus = ctx.surplus(&e, price, years, p);
                if !short && surplus <= p.personality.thrift {
                    // Not a bargain; remember the best of these in case the
                    // budget would otherwise go unused.
                    if p.spends_budget
                        && fallback.is_none_or(|(s, b, _)| (surplus, b.seen.id) > (s, f.id))
                    {
                        fallback = Some((surplus, e, years));
                    }
                    continue;
                }
                if best.is_none_or(|(s, b, _)| (surplus, b.seen.id) > (s, f.id)) {
                    best = Some((surplus, e, years));
                }
            }
        }
        best.or(fallback)
            .map(|(_, e, years)| GmAction::Sign {
                player: e.seen.id,
                salary: e.seen.ask,
                years,
            })
            .into_iter()
            .collect()
    }

    fn roster(&self, obs: &GmObs) -> Vec<GmAction> {
        let ctx = Context::new(obs, self.seed);
        let mut refined = obs.clone();
        for s in &mut refined.roster {
            let e = ctx.estimate(s, &self.profile);
            s.now = e.q;
            s.peak = e.peak;
        }
        set_levels(&refined)
            .iter()
            .filter_map(GmAction::from_command)
            .collect()
    }

    fn trade(&self, obs: &GmObs) -> Vec<GmAction> {
        let Some(t) = &obs.trade else {
            return Vec::new();
        };
        let p = &self.profile;
        if p.trades == TradeMode::Off {
            return Vec::new();
        }
        let ctx = Context::new(obs, self.seed);
        let value = |s: &Seen| ctx.contract_surplus(&ctx.estimate(s, p), p);
        let margin = p.personality.trade_margin;
        // A club in debt sheds salary first.
        let mut out: Vec<GmAction> = world::agents::salary_dumps(obs)
            .into_iter()
            .filter_map(|c| GmAction::from_command(&c))
            .collect();
        let held = |kind: Kind| obs.roster.iter().filter(|s| s.kind == kind).count();
        for o in &t.incoming {
            let gain: f64 =
                o.give.iter().map(&value).sum::<f64>() - o.get.iter().map(&value).sum::<f64>();
            let fits = [Kind::Batter, Kind::Pitcher].into_iter().all(|k| {
                let n = |v: &[Seen]| v.iter().filter(|s| s.kind == k).count();
                held(k) + n(&o.give) - n(&o.get) <= ROSTER_MAX_PER_KIND
            });
            out.push(GmAction::RespondTrade {
                offer: o.id,
                accept: gain > margin && fits,
            });
        }
        if p.trades == TradeMode::Active {
            out.extend(self.proposals(obs, &ctx, &t.market));
        }
        out
    }

    fn proposals(&self, obs: &GmObs, ctx: &Context, market: &[Held]) -> Vec<GmAction> {
        let p = &self.profile;
        let margin = p.personality.trade_margin;
        // What the market's consensus says, to keep offers plausible.
        let public = Profile {
            extra_noise: 0.0,
            market_weight: 1.0,
            ..*p
        };
        let mut mine: Vec<(f64, f64, Est)> = obs
            .roster
            .iter()
            .filter(|s| s.level <= Level::Triple && s.salary > 0.0)
            .map(|s| {
                let e = ctx.estimate(s, p);
                (ctx.contract_surplus(&e, p), ctx.mean_quality(&e, p), e)
            })
            .collect();
        mine.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.seen.id.cmp(&b.2.seen.id)));
        mine.truncate(14);
        let mut theirs: Vec<(f64, f64, u8, Est)> = market
            .iter()
            .filter(|h| h.seen.salary > 0.0)
            .map(|h| {
                let e = ctx.estimate(&h.seen, p);
                (
                    ctx.contract_surplus(&e, p),
                    ctx.mean_quality(&e, p),
                    h.club,
                    e,
                )
            })
            .collect();
        theirs.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.3.seen.id.cmp(&b.3.seen.id)));
        theirs.truncate(60);
        let mut options: Vec<(f64, u8, Est, Est)> = Vec::new();
        for (vx, qx, x) in &mine {
            for (vy, qy, club, y) in &theirs {
                if x.seen.kind != y.seen.kind || *qy <= qx - GIVE_UP_QUALITY {
                    continue;
                }
                let gain = vy - vx;
                if gain <= margin {
                    continue;
                }
                // The other side should not look robbed by the market's view.
                let (ex, ey) = (
                    ctx.estimate(&x.seen, &public),
                    ctx.estimate(&y.seen, &public),
                );
                // The other side looks at quality; it should see a step up.
                if ex.q - ey.q < OTHER_SIDE_STEP {
                    continue;
                }
                let px = ctx.contract_surplus(&ex, &public);
                let py = ctx.contract_surplus(&ey, &public);
                if px - py < -(FAIRNESS_SLACK + FAIRNESS_SHARE * gain) {
                    continue;
                }
                options.push((gain, *club, *x, *y));
            }
        }
        options.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then(a.3.seen.id.cmp(&b.3.seen.id))
                .then(a.2.seen.id.cmp(&b.2.seen.id))
        });
        let (mut used_clubs, mut used_players) = (Vec::new(), Vec::new());
        let mut out = Vec::new();
        for (_, club, x, y) in options {
            if used_clubs.contains(&club)
                || used_players.contains(&x.seen.id)
                || used_players.contains(&y.seen.id)
            {
                continue;
            }
            used_clubs.push(club);
            used_players.extend([x.seen.id, y.seen.id]);
            out.push(GmAction::ProposeTrade {
                partner: club,
                give: vec![x.seen.id],
                get: vec![y.seen.id],
            });
            if out.len() >= PROPOSALS {
                break;
            }
        }
        out
    }
}

impl GmAgent for SurplusGm {
    fn trades(&self) -> bool {
        self.profile.trades != TradeMode::Off
    }

    fn act(&mut self, turn: &GmTurn) -> Vec<GmAction> {
        match turn.stage {
            GmStage::Draft => self.draft(&turn.obs),
            GmStage::FreeAgency => self.free_agency(&turn.obs),
            GmStage::Roster => self.roster(&turn.obs),
            GmStage::Trade => self.trade(&turn.obs),
        }
    }
}
