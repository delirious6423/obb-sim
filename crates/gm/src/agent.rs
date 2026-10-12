use connect::{GmAction, GmAgent, GmStage, GmTurn};
use world::agents::{GmObs, Seen, set_levels};
use world::player::{Origin, PlayerId};
use world::rating::Kind;
use world::state::{
    CREDIT, MAJORS_PER_KIND, MIN_SALARY, OFFERS_PER_CLUB, ROSTER_MAX_PER_KIND, Stance,
};

use crate::profile::{Profile, TradeMode};
use crate::trade;
use crate::value::{Context, Est};

/// Quality gain over the thirteenth man a free agent must offer.
const UPGRADE_MARGIN: f64 = 0.0015;
/// Proposals per trade turn.
const PROPOSALS: usize = 3;

/// A general manager that values players by surplus over their contracts.
#[derive(Debug, Clone)]
pub struct SurplusGm {
    seed: u64,
    profile: Profile,
    /// Offers made in this year's trade window.
    sent: Vec<trade::Sent>,
    sent_year: i32,
}

impl SurplusGm {
    /// `seed` fixes this manager's own judgement errors (same seed, same
    /// mistakes); give each club a different one.
    #[must_use]
    pub fn new(seed: u64, profile: Profile) -> Self {
        Self {
            seed,
            profile,
            sent: Vec::new(),
            sent_year: 0,
        }
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
        let room = |ask: f64| {
            ask <= MIN_SALARY + 1e-9
                || (obs.payroll + ask <= obs.budget + 1e-9 && obs.payroll + ask <= obs.payroll_cap)
        };
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
                if !room(price) {
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

    fn trade(&mut self, obs: &GmObs) -> Vec<GmAction> {
        let Some(t) = &obs.trade else {
            return Vec::new();
        };
        let p = self.profile;
        if p.trades == TradeMode::Off {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut accepted = false;
        for (offer, accept) in trade::answers(obs, &p, self.seed) {
            accepted |= accept;
            out.push(GmAction::RespondTrade { offer, accept });
        }
        // New offers wait for a turn that starts from the roster as it is.
        if accepted {
            return out;
        }
        if p.trades == TradeMode::Active {
            if self.sent_year != obs.year {
                self.sent_year = obs.year;
                self.sent.clear();
            }
            // No offer made twice in a window, one open offer per partner, no
            // player in two open offers, and no more open offers than the
            // league allows.
            let mut used: Vec<PlayerId> = t
                .outgoing
                .iter()
                .flat_map(|o| o.give.iter().chain(&o.get).copied())
                .collect();
            let max = PROPOSALS.min(OFFERS_PER_CLUB.saturating_sub(t.outgoing.len()));
            let deals = trade::deals(obs, t, &p, self.seed, &self.sent);
            let made = trade::proposals(deals, max, trade::salary_room(obs, t), &mut used);
            for a in &made {
                if let GmAction::ProposeTrade { partner, give, get } = a {
                    self.sent.push((*partner, give.clone(), get.clone()));
                }
            }
            out.extend(made);
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
