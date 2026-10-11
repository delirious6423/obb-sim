//! General-manager AI: values players by what they are worth over their
//! contracts (surplus value), then drafts, signs, trades and sets rosters
//! by those values.
//!
//! A [`Profile`] holds what a general manager knows and how well it decides.
//! Presets differ only in information and decision quality; none touches the
//! dice. A [`Personality`] sets taste (youth, thrift, urgency) at any level.

mod agent;
pub mod bots;
mod profile;
mod value;

pub use agent::SurplusGm;
pub use profile::{Personality, Preset, Profile, TradeMode};
