use serde::{Deserialize, Serialize};
use world::agents::OwnerObs;
use world::state::Tactics;

use crate::agent::{CommissionerAgent, FieldManagerAgent, GmAgent, LiveAgent, OwnerAgent};
use crate::builtin::{BuiltInCommissioner, BuiltInGm, BuiltInLive, BuiltInManager, BuiltInOwner};
use crate::model::{
    CommissionerAction, CommissionerObs, FieldManagerObs, GameObs, GmAction, GmTurn, OwnerAction,
    ReliefAction, StealAction,
};

/// What an external agent is asked. One JSON object per request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "snake_case")]
pub enum Request {
    Commissioner(CommissionerObs),
    Owner(OwnerObs),
    Gm(GmTurn),
    FieldManager(FieldManagerObs),
    Steal { obs: GameObs, from: u8 },
    Relief { obs: GameObs, half_start: bool },
}

/// What an external agent answers. One JSON object per response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "response", rename_all = "snake_case")]
pub enum Response {
    Commissioner { action: CommissionerAction },
    Owner { action: OwnerAction },
    Gm { actions: Vec<GmAction> },
    FieldManager { tactics: Tactics },
    Steal { action: StealAction },
    Relief { action: ReliefAction },
}

/// A line to whatever fills the seat: text in, text out.
pub trait Transport {
    fn call(&mut self, request: &str) -> Result<String, String>;
}

/// Fills a seat through a [`Transport`]. When the other side fails or answers
/// nonsense, the seat does the harmless thing (keep the budget, sign nobody,
/// keep the tactics, engine defaults in a game; the calendar for a
/// commissioner) and the failure is counted.
pub struct ExternalAgent<T> {
    transport: T,
    pub calls: u64,
    pub failures: u64,
    pub last_error: Option<String>,
}

impl<T: Transport> ExternalAgent<T> {
    #[must_use]
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            calls: 0,
            failures: 0,
            last_error: None,
        }
    }

    fn ask(&mut self, request: &Request) -> Option<Response> {
        self.calls += 1;
        let result = serde_json::to_string(request)
            .map_err(|e| e.to_string())
            .and_then(|text| self.transport.call(&text))
            .and_then(|reply| serde_json::from_str::<Response>(&reply).map_err(|e| e.to_string()));
        match result {
            Ok(r) => Some(r),
            Err(e) => {
                self.failures += 1;
                self.last_error = Some(e);
                None
            }
        }
    }
}

impl<T: Transport> CommissionerAgent for ExternalAgent<T> {
    fn act(&mut self, obs: &CommissionerObs) -> CommissionerAction {
        match self.ask(&Request::Commissioner(obs.clone())) {
            Some(Response::Commissioner { action }) => action,
            other => {
                self.mismatch(other.is_some());
                BuiltInCommissioner::default().act(obs)
            }
        }
    }
}

impl<T: Transport> OwnerAgent for ExternalAgent<T> {
    fn act(&mut self, obs: &OwnerObs) -> OwnerAction {
        match self.ask(&Request::Owner(obs.clone())) {
            Some(Response::Owner { action }) => action,
            other => {
                self.mismatch(other.is_some());
                OwnerAction {
                    payroll: obs.budget.clamp(obs.payroll_floor, obs.payroll_cap),
                    ..OwnerAction::default()
                }
            }
        }
    }
}

impl<T: Transport> GmAgent for ExternalAgent<T> {
    fn trades(&self) -> bool {
        true
    }

    fn act(&mut self, turn: &GmTurn) -> Vec<GmAction> {
        match self.ask(&Request::Gm(turn.clone())) {
            Some(Response::Gm { actions }) => actions,
            other => {
                self.mismatch(other.is_some());
                Vec::new()
            }
        }
    }
}

impl<T: Transport> FieldManagerAgent for ExternalAgent<T> {
    fn tactics(&mut self, obs: &FieldManagerObs) -> Tactics {
        match self.ask(&Request::FieldManager(obs.clone())) {
            Some(Response::FieldManager { tactics }) => tactics,
            other => {
                self.mismatch(other.is_some());
                obs.tactics
            }
        }
    }
}

impl<T: Transport> LiveAgent for ExternalAgent<T> {
    fn steal(&mut self, obs: &GameObs, from: u8) -> StealAction {
        match self.ask(&Request::Steal {
            obs: obs.clone(),
            from,
        }) {
            Some(Response::Steal { action }) => action,
            other => {
                self.mismatch(other.is_some());
                StealAction::Default
            }
        }
    }

    fn relief(&mut self, obs: &GameObs, half_start: bool) -> ReliefAction {
        match self.ask(&Request::Relief {
            obs: obs.clone(),
            half_start,
        }) {
            Some(Response::Relief { action }) => action,
            other => {
                self.mismatch(other.is_some());
                ReliefAction::Default
            }
        }
    }
}

impl<T> ExternalAgent<T> {
    /// A reply of the wrong kind counts as a failure too.
    fn mismatch(&mut self, answered: bool) {
        if answered {
            self.failures += 1;
            self.last_error = Some("reply does not match the request".into());
        }
    }
}

/// An in-process stand-in for an outside program. It sees only the JSON of
/// each request and answers with the built-in policies, so a run through it
/// shows the observations hold everything a decision needs and that the
/// protocol loses nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReferenceTransport {
    pub commissioner: BuiltInCommissioner,
    pub manager: BuiltInManager,
}

impl Transport for ReferenceTransport {
    fn call(&mut self, request: &str) -> Result<String, String> {
        let request: Request = serde_json::from_str(request).map_err(|e| e.to_string())?;
        let response = match request {
            Request::Commissioner(o) => Response::Commissioner {
                action: self.commissioner.act(&o),
            },
            Request::Owner(o) => Response::Owner {
                action: BuiltInOwner.act(&o),
            },
            Request::Gm(t) => Response::Gm {
                actions: BuiltInGm.act(&t),
            },
            Request::FieldManager(o) => Response::FieldManager {
                tactics: self.manager.tactics(&o),
            },
            Request::Steal { obs, from } => Response::Steal {
                action: BuiltInLive.steal(&obs, from),
            },
            Request::Relief { obs, half_start } => Response::Relief {
                action: BuiltInLive.relief(&obs, half_start),
            },
        };
        serde_json::to_string(&response).map_err(|e| e.to_string())
    }
}
