use world::agents::{
    GmObs, LeagueObs, OwnerObs, Seen, commissioner_rules_for, draft_pick, free_agency_step,
    owner_budget_for, owner_build_for, owner_stance_for, owner_ticket_for, salary_dumps,
    set_levels,
};
use world::player::PlayerId;
use world::rating::Kind;
use world::state::{MAJORS_PER_KIND, Phase, ROSTER_MAX_PER_KIND, Rules, Tactics};

use crate::agent::{CommissionerAgent, FieldManagerAgent, GmAgent, LiveAgent, OwnerAgent};
use crate::model::{
    CommissionerAction, CommissionerObs, FieldManagerObs, GameObs, GmAction, GmStage, GmTurn,
    OwnerAction, ReliefAction, StealAction,
};

/// Follows the league calendar.
#[derive(Debug, Clone, Copy)]
pub struct BuiltInCommissioner {
    pub burn_in_years: u8,
}

impl Default for BuiltInCommissioner {
    fn default() -> Self {
        Self { burn_in_years: 30 }
    }
}

impl CommissionerAgent for BuiltInCommissioner {
    fn act(&mut self, obs: &CommissionerObs) -> CommissionerAction {
        match obs.phase {
            Phase::Unfounded => CommissionerAction::Found {
                burn_in_years: self.burn_in_years,
            },
            Phase::Regular => CommissionerAction::PlaySeason,
            Phase::Playoffs => CommissionerAction::PlayPlayoffs,
            Phase::Settling => CommissionerAction::SettleFinances,
            Phase::Offseason => CommissionerAction::ProgressPlayers,
            Phase::Progressed => CommissionerAction::OpenDraft,
            Phase::Drafting => CommissionerAction::CloseDraft,
            Phase::FreeAgency => CommissionerAction::StartSeason,
        }
    }

    fn rules(&mut self, obs: &LeagueObs) -> Option<Rules> {
        commissioner_rules_for(obs)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltInOwner;

impl OwnerAgent for BuiltInOwner {
    fn act(&mut self, obs: &OwnerObs) -> OwnerAction {
        let stance = owner_stance_for(obs);
        OwnerAction {
            payroll: owner_budget_for(obs),
            ticket: Some(owner_ticket_for(obs)),
            build: owner_build_for(obs),
            stance: (stance != obs.stance).then_some(stance),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltInGm;

impl GmAgent for BuiltInGm {
    /// Answers offers; a club in debt also offers its dearest contracts to
    /// shed salary (a solvent one proposes nothing).
    fn trades(&self) -> bool {
        true
    }

    fn act(&mut self, turn: &GmTurn) -> Vec<GmAction> {
        let commands = match turn.stage {
            GmStage::Draft => draft_pick(&turn.obs).into_iter().collect(),
            GmStage::FreeAgency => free_agency_step(&turn.obs).into_iter().collect(),
            GmStage::Roster => set_levels(&turn.obs),
            GmStage::Trade => {
                let mut out = plain_trade_answers(&turn.obs);
                out.extend(
                    salary_dumps(&turn.obs)
                        .iter()
                        .filter_map(GmAction::from_command),
                );
                return out;
            }
        };
        commands.iter().filter_map(GmAction::from_command).collect()
    }
}

/// Quality (runs per PA, summed) a trade must add before the plain rule agrees.
const PLAIN_TRADE_MARGIN: f64 = 0.001;

/// A plain rule for trade offers: accept when the players coming in score
/// better on the club's own reports than the players going out, the roster
/// stays within its limits, and any salary taken on fits the budget (none
/// at all while the club is in debt).
#[must_use]
pub fn plain_trade_answers(obs: &GmObs) -> Vec<GmAction> {
    let Some(t) = &obs.trade else {
        return Vec::new();
    };
    // Offers are taken in turn; each accepted one changes the roster and
    // payroll the next is judged against.
    let mut roster: Vec<PlayerId> = obs.roster.iter().map(|s| s.id).collect();
    let mut count =
        [Kind::Batter, Kind::Pitcher].map(|k| obs.roster.iter().filter(|s| s.kind == k).count());
    let mut payroll = obs.payroll;
    let mut gone: Vec<PlayerId> = Vec::new();
    let mut out = Vec::new();
    for o in &t.incoming {
        let sum = |v: &[Seen]| v.iter().map(Seen::score).sum::<f64>();
        let pay = |v: &[Seen]| v.iter().map(|s| s.salary).sum::<f64>();
        let n = |v: &[Seen], k: Kind| v.iter().filter(|s| s.kind == k).count();
        let fits = [Kind::Batter, Kind::Pitcher]
            .into_iter()
            .zip(count)
            .all(|(k, held)| {
                let after = (held + n(&o.give, k)).saturating_sub(n(&o.get, k));
                after <= ROSTER_MAX_PER_KIND
                    && (after >= MAJORS_PER_KIND || n(&o.give, k) >= n(&o.get, k))
            });
        let net = pay(&o.give) - pay(&o.get);
        let affordable = obs.may_take_salary(payroll, net);
        let present = o.get.iter().all(|s| roster.contains(&s.id))
            && o.give.iter().all(|s| {
                !gone.contains(&s.id)
                    && t.market
                        .iter()
                        .any(|h| h.club == o.from && h.seen.id == s.id)
            });
        let accept =
            fits && affordable && present && sum(&o.give) - sum(&o.get) > PLAIN_TRADE_MARGIN;
        if accept {
            roster.retain(|id| !o.get.iter().any(|s| s.id == *id));
            roster.extend(o.give.iter().map(|s| s.id));
            gone.extend(o.give.iter().map(|s| s.id));
            for (i, k) in [Kind::Batter, Kind::Pitcher].into_iter().enumerate() {
                count[i] = count[i] + n(&o.give, k) - n(&o.get, k);
            }
            payroll += net;
        }
        out.push(GmAction::RespondTrade {
            offer: o.id,
            accept,
        });
    }
    out
}

/// Keeps one tactics setting for every season.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltInManager {
    pub tactics: Tactics,
}

impl FieldManagerAgent for BuiltInManager {
    fn tactics(&mut self, _: &FieldManagerObs) -> Tactics {
        self.tactics
    }
}

/// The engine's own rules, inside a game.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltInLive;

impl LiveAgent for BuiltInLive {
    fn steal(&mut self, _: &GameObs, _: u8) -> StealAction {
        StealAction::Default
    }

    fn relief(&mut self, _: &GameObs, _: bool) -> ReliefAction {
        ReliefAction::Default
    }
}
