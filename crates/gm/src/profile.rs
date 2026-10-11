/// How much a general manager takes part in trading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeMode {
    /// Ignores offers.
    Off,
    /// Answers offers but never proposes.
    Respond,
    /// Proposes and answers.
    Active,
}

/// Taste: changes what the general manager wants, not how well it sees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Personality {
    /// Weight on a player's ceiling over his current level (0..1).
    pub youth: f64,
    /// Surplus (millions per season) a signing must show before the GM acts;
    /// negative means paying above value is acceptable.
    pub thrift: f64,
    /// Margin (millions) a trade must gain before it is offered or accepted.
    pub trade_margin: f64,
}

impl Personality {
    pub const BALANCED: Self = Self {
        youth: 0.3,
        thrift: 0.0,
        trade_margin: 1.5,
    };
    /// Builds through the draft and young talent.
    pub const BUILDER: Self = Self {
        youth: 0.6,
        thrift: 0.5,
        trade_margin: 2.5,
    };
    /// Wants wins now and will overpay for them.
    pub const WIN_NOW: Self = Self {
        youth: 0.1,
        thrift: -1.5,
        trade_margin: 1.0,
    };
    /// Buys only what is cheap.
    pub const BARGAIN_HUNTER: Self = Self {
        youth: 0.3,
        thrift: 1.5,
        trade_margin: 2.0,
    };
}

/// What the general manager knows and how well it decides.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    /// Extra error added to the club's own scouting, in units of the spread
    /// of qualities it sees (0 = trusts the reports as they are).
    pub extra_noise: f64,
    /// Weight put on the market's view, read from asking prices (0..1).
    /// Averaging two independent views cuts the error.
    pub market_weight: f64,
    /// Seasons of a contract looked at when valuing a player.
    pub horizon: u8,
    /// Whether ageing is projected (young players grow, old ones fade).
    pub projects_aging: bool,
    /// Whether contract length is chosen to fit the player (else the ask).
    pub picks_years: bool,
    pub trades: TradeMode,
    /// Whether upgrades are bought with budget that would otherwise go
    /// unspent, even when they are not bargains.
    pub spends_budget: bool,
    pub personality: Personality,
}

/// Difficulty presets: from a general manager who trusts noisy reports and
/// looks one year ahead, to one who pools every view, projects ageing and
/// trades. The dice are the same for all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Preset {
    Rookie,
    Veteran,
    AllStar,
    HallOfFame,
}

impl Preset {
    pub const ALL: [Self; 4] = [Self::Rookie, Self::Veteran, Self::AllStar, Self::HallOfFame];

    #[must_use]
    pub fn profile(self, personality: Personality) -> Profile {
        match self {
            Self::Rookie => Profile {
                extra_noise: 0.9,
                market_weight: 0.0,
                horizon: 1,
                projects_aging: false,
                picks_years: false,
                trades: TradeMode::Off,
                spends_budget: false,
                personality,
            },
            Self::Veteran => Profile {
                extra_noise: 0.4,
                market_weight: 0.2,
                horizon: 2,
                projects_aging: false,
                picks_years: false,
                trades: TradeMode::Respond,
                spends_budget: true,
                personality,
            },
            Self::AllStar => Profile {
                extra_noise: 0.1,
                market_weight: 0.35,
                horizon: 3,
                projects_aging: true,
                picks_years: true,
                trades: TradeMode::Active,
                spends_budget: true,
                personality,
            },
            Self::HallOfFame => Profile {
                extra_noise: 0.0,
                market_weight: 0.5,
                horizon: 5,
                projects_aging: true,
                picks_years: true,
                trades: TradeMode::Active,
                spends_budget: true,
                personality,
            },
        }
    }
}
