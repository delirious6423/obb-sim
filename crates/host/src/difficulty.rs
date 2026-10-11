//! From the protocol's difficulty settings to the general-manager AI's
//! profile. Difficulty changes what the AI knows and how well it decides;
//! the dice are the same for every setting.

use gm::{Personality, Preset, Profile, TradeMode};

use crate::api::{Difficulty, Levers, Preset as ApiPreset, TradeLevel};

pub(crate) fn profile(d: &Difficulty) -> Profile {
    match d {
        Difficulty::Preset(p) => {
            let preset = match p {
                ApiPreset::Rookie => Preset::Rookie,
                ApiPreset::Veteran => Preset::Veteran,
                ApiPreset::AllStar => Preset::AllStar,
                ApiPreset::HallOfFame => Preset::HallOfFame,
            };
            preset.profile(Personality::BALANCED)
        }
        Difficulty::Custom(l) => custom(l),
    }
}

fn custom(l: &Levers) -> Profile {
    Profile {
        extra_noise: l.extra_noise.clamp(0.0, 3.0),
        market_weight: l.market_weight.clamp(0.0, 1.0),
        horizon: l.horizon.clamp(1, 7),
        projects_aging: l.projects_aging,
        picks_years: l.picks_years,
        trades: match l.trades {
            TradeLevel::Off => TradeMode::Off,
            TradeLevel::Respond => TradeMode::Respond,
            TradeLevel::Active => TradeMode::Active,
        },
        spends_budget: l.spends_budget,
        personality: Personality {
            youth: l.youth.clamp(0.0, 1.0),
            thrift: l.thrift.clamp(-3.0, 3.0),
            trade_margin: l.trade_margin.clamp(0.0, 6.0),
        },
    }
}

/// The levers a preset stands for, for a settings screen to start from.
#[must_use]
pub fn levers_of(p: ApiPreset) -> Levers {
    let prof = profile(&Difficulty::Preset(p));
    Levers {
        extra_noise: prof.extra_noise,
        market_weight: prof.market_weight,
        horizon: prof.horizon,
        projects_aging: prof.projects_aging,
        picks_years: prof.picks_years,
        trades: match prof.trades {
            TradeMode::Off => TradeLevel::Off,
            TradeMode::Respond => TradeLevel::Respond,
            TradeMode::Active => TradeLevel::Active,
        },
        spends_budget: prof.spends_budget,
        youth: prof.personality.youth,
        thrift: prof.personality.thrift,
        trade_margin: prof.personality.trade_margin,
    }
}
