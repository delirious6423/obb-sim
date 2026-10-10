use protocol::Command;

use crate::views::{CommissionerView, FieldManagerView, GmView, OwnerView};

/// Commands proposed by a seat are only proposals. Run them through
/// [`crate::vet`] first; `Session::submit` enforces the same rules regardless.
pub trait CommissionerAgent {
    fn act(&mut self, view: &CommissionerView) -> Vec<Command>;
}

pub trait OwnerAgent {
    fn act(&mut self, view: &OwnerView) -> Vec<Command>;
}

pub trait GmAgent {
    fn act(&mut self, view: &GmView) -> Vec<Command>;
}

pub trait FieldManagerAgent {
    fn act(&mut self, view: &FieldManagerView) -> Vec<Command>;
}
