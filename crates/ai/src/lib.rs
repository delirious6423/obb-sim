//! The four seats (commissioner, owner, GM, field manager), what each may see,
//! and what each may command.
//!
//! Every agent is bound to an [`protocol::Actor`]: a role plus, except for the
//! commissioner, one team. `league` enforces that on every command.
//!
//! The traits are the seat interface. An AI agent implements one; a human UI
//! can implement the same trait. Each trait takes only its own role's view, so
//! an agent cannot see more than its role allows.
//!
//! Difficulty (what the AI knows, how well it decides) will later shape the
//! view an agent receives and its choices. It never touches the dice.

mod permissions;
mod roles;
mod views;

pub use league::{PermissionError, authorize};
pub use permissions::vet;
pub use roles::{CommissionerAgent, FieldManagerAgent, GmAgent, OwnerAgent};
pub use views::{CommissionerView, FieldManagerView, GmView, OwnerView};
