//! Fictional calibration rosters.
//!
//! Every player is invented: names come from a syllable generator, and rates
//! are the league rates times a random talent multiplier per outcome, drawn
//! from the spread measured in the data. No real player, name or stat line is
//! involved. The population is then rescaled so its average player is exactly
//! league average, which keeps sampling luck out of the calibration check.

use std::collections::BTreeSet;

use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use sim_core::{OUTCOMES, Player, Rates, Team};

use crate::data::Talent;

/// Starters in a rotation (the engine uses pitcher 0 as the day's starter).
pub const ROTATION: usize = 5;
/// Relievers per team; the last is the closer.
pub const BULLPEN: usize = 5;
const BATTERS: usize = 9;

/// A generated player: a name and unnormalised-then-normalised rates.
#[derive(Debug, Clone)]
pub struct Specimen {
    pub name: String,
    pub rates: [f64; OUTCOMES],
}

#[derive(Debug, Clone)]
pub struct TeamSpec {
    pub name: String,
    pub batters: Vec<Specimen>,
    pub rotation: Vec<Specimen>,
    pub bullpen: Vec<Specimen>,
}

struct Draw {
    rng: ChaCha8Rng,
}

impl Draw {
    fn uniform(&mut self) -> f64 {
        (self.rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Standard normal by Box-Muller, using libm for deterministic math.
    fn normal(&mut self) -> f64 {
        let u1 = self.uniform().max(1e-300);
        let u2 = self.uniform();
        libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(2.0 * core::f64::consts::PI * u2)
    }

    fn pick<'a>(&mut self, items: &'a [&'a str]) -> &'a str {
        items[(self.rng.next_u64() % items.len() as u64) as usize]
    }
}

const ONSETS: &[&str] = &[
    "Br", "Cal", "Dar", "El", "Fen", "Gar", "Hal", "Is", "Jor", "Kel", "Lor", "Mar", "Nor", "Os",
    "Pel", "Quin", "Ros", "Sal", "Tor", "Ul", "Vor", "Wes", "Yar", "Zan",
];
const MIDDLES: &[&str] = &["a", "e", "i", "o", "u", "ae", "io", "ar", "en", "or"];
const ENDS: &[&str] = &["n", "k", "l", "s", "th", "d", "ric", "vin", "tan", "mar"];
const SURNAME_ENDS: &[&str] = &[
    "son", "ford", "wick", "ley", "ton", "man", "worth", "field", "by", "ham",
];
const CITIES: &[&str] = &[
    "Harbor City",
    "Pine Ridge",
    "Stone Valley",
    "Red Mesa",
    "Lake Aurel",
    "Copper Bay",
    "Iron Hollow",
    "Silver Fork",
    "Cedar Point",
    "Maple Run",
    "Granite Falls",
    "North Haven",
    "Amber Coast",
    "Willow Creek",
    "Falcon Rock",
];
const MASCOTS: &[&str] = &[
    "Gulls", "Foxes", "Herons", "Badgers", "Marlins", "Wolves", "Otters", "Rams", "Kestrels",
    "Bison", "Lynx", "Pelicans", "Stags", "Cranes", "Mariners",
];

fn make_name(draw: &mut Draw, used: &mut BTreeSet<String>) -> String {
    loop {
        let first = format!(
            "{}{}{}",
            draw.pick(ONSETS),
            draw.pick(MIDDLES),
            draw.pick(ENDS)
        );
        let last = format!(
            "{}{}{}",
            draw.pick(ONSETS),
            draw.pick(MIDDLES),
            draw.pick(SURNAME_ENDS)
        );
        let name = format!("{first} {last}");
        if used.insert(name.clone()) {
            return name;
        }
    }
}

/// League rates times lognormal multipliers with the given relative spread.
fn draw_rates(draw: &mut Draw, league: &Rates, sd: &[f64; OUTCOMES]) -> [f64; OUTCOMES] {
    let mut out = [0.0; OUTCOMES];
    for i in 0..OUTCOMES {
        // exp(sd*z - sd^2/2) has mean 1.
        let m = libm::exp(sd[i] * draw.normal() - 0.5 * sd[i] * sd[i]);
        out[i] = league.0[i] * m;
    }
    normalise(&mut out);
    out
}

fn normalise(r: &mut [f64; OUTCOMES]) {
    let sum: f64 = r.iter().sum();
    for v in r.iter_mut() {
        *v /= sum;
    }
}

/// Rescale a weighted population so its weighted mean rates equal `league`.
fn centre(population: &mut [(f64, &mut [f64; OUTCOMES])], league: &Rates) {
    let total: f64 = population.iter().map(|(w, _)| *w).sum();
    for _ in 0..30 {
        let mut mean = [0.0; OUTCOMES];
        for (w, r) in population.iter() {
            for i in 0..OUTCOMES {
                mean[i] += w * r[i] / total;
            }
        }
        for (_, r) in population.iter_mut() {
            for i in 0..OUTCOMES {
                r[i] *= league.0[i] / mean[i];
            }
            normalise(r);
        }
    }
}

/// Generate `teams` fictional teams, deterministically from `seed`.
#[must_use]
pub fn generate(league: &Rates, talent: &Talent, teams: usize, seed: u64) -> Vec<TeamSpec> {
    let mut draw = Draw {
        rng: ChaCha8Rng::seed_from_u64(seed),
    };
    let mut used = BTreeSet::new();
    let mut specs: Vec<TeamSpec> = Vec::with_capacity(teams);
    for t in 0..teams {
        // City + mascot, made unique by sticking to the lists and adding an
        // ordinal only if they run out.
        let city = CITIES[t % CITIES.len()];
        let mascot = MASCOTS[(t / CITIES.len() + t) % MASCOTS.len()];
        let name = format!("{city} {mascot}");
        let mut mk = |draw: &mut Draw, sd: &[f64; OUTCOMES]| Specimen {
            name: make_name(draw, &mut used),
            rates: draw_rates(draw, league, sd),
        };
        specs.push(TeamSpec {
            name,
            batters: (0..BATTERS)
                .map(|_| mk(&mut draw, &talent.batter_sd))
                .collect(),
            rotation: (0..ROTATION)
                .map(|_| mk(&mut draw, &talent.pitcher_sd))
                .collect(),
            bullpen: (0..BULLPEN)
                .map(|_| mk(&mut draw, &talent.pitcher_sd))
                .collect(),
        });
    }

    // Centre batters (equal plate appearances per lineup slot) ...
    let mut batters: Vec<(f64, &mut [f64; OUTCOMES])> = specs
        .iter_mut()
        .flat_map(|t| t.batters.iter_mut().map(|p| (1.0, &mut p.rates)))
        .collect();
    centre(&mut batters, league);
    // ... and pitchers by innings share: starters about two thirds, the
    // bullpen the rest.
    let mut pitchers: Vec<(f64, &mut [f64; OUTCOMES])> = specs
        .iter_mut()
        .flat_map(|t| {
            t.rotation
                .iter_mut()
                .map(|p| (0.66 / ROTATION as f64, &mut p.rates))
                .chain(
                    t.bullpen
                        .iter_mut()
                        .map(|p| (0.34 / BULLPEN as f64, &mut p.rates)),
                )
        })
        .collect();
    centre(&mut pitchers, league);
    specs
}

/// One playable `Team` per rotation slot: the same players with a different
/// starter. Index with the starter number.
#[must_use]
pub fn teams_by_starter(spec: &TeamSpec) -> Vec<Team> {
    let player = |s: &Specimen| Player {
        name: s.name.clone(),
        rates: Rates(s.rates),
    };
    (0..spec.rotation.len())
        .map(|starter| {
            let mut pitchers = vec![player(&spec.rotation[starter])];
            pitchers.extend(spec.bullpen.iter().map(player));
            Team::new(
                spec.name.clone(),
                spec.batters.iter().map(player).collect(),
                pitchers,
            )
            .expect("generated teams are valid")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::GameParams;

    fn talent() -> Talent {
        Talent {
            batter_sd: [0.20, 0.25, 0.2, 0.10, 0.2, 0.5, 0.40, 0.05],
            pitcher_sd: [0.20, 0.20, 0.2, 0.08, 0.15, 0.4, 0.25, 0.04],
        }
    }

    #[test]
    fn generation_is_deterministic_and_seed_sensitive() {
        let league = GameParams::reference().league;
        let a = generate(&league, &talent(), 6, 1);
        let b = generate(&league, &talent(), 6, 1);
        let c = generate(&league, &talent(), 6, 2);
        let names = |v: &[TeamSpec]| -> Vec<String> {
            v.iter()
                .flat_map(|t| t.batters.iter().map(|p| p.name.clone()))
                .collect()
        };
        assert_eq!(names(&a), names(&b));
        assert_ne!(names(&a), names(&c));
        assert_eq!(a[0].batters[0].rates, b[0].batters[0].rates);
    }

    #[test]
    fn populations_average_exactly_to_the_league() {
        let league = GameParams::reference().league;
        let teams = generate(&league, &talent(), 30, 7);
        let n = (teams.len() * 9) as f64;
        let mut mean = [0.0; OUTCOMES];
        for p in teams.iter().flat_map(|t| &t.batters) {
            for (m, r) in mean.iter_mut().zip(p.rates) {
                *m += r / n;
            }
        }
        for (m, l) in mean.iter().zip(league.0) {
            assert!((m / l - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn players_differ_and_every_row_is_a_probability_vector() {
        let league = GameParams::reference().league;
        let teams = generate(&league, &talent(), 10, 3);
        let all: Vec<&Specimen> = teams
            .iter()
            .flat_map(|t| t.batters.iter().chain(&t.rotation).chain(&t.bullpen))
            .collect();
        for p in &all {
            assert!((p.rates.iter().sum::<f64>() - 1.0).abs() < 1e-9);
            assert!(p.rates.iter().all(|r| *r > 0.0));
        }
        let hr: Vec<f64> = all.iter().map(|p| p.rates[6]).collect();
        let (lo, hi) = hr
            .iter()
            .fold((1.0f64, 0.0f64), |(l, h), v| (l.min(*v), h.max(*v)));
        assert!(hi / lo > 1.5, "talent spread should be visible: {lo}..{hi}");
        let names: BTreeSet<&String> = all.iter().map(|p| &p.name).collect();
        assert_eq!(names.len(), all.len(), "names are unique");
    }

    #[test]
    fn builds_playable_teams() {
        let league = GameParams::reference().league;
        let teams = generate(&league, &talent(), 2, 5);
        let variants = teams_by_starter(&teams[0]);
        assert_eq!(variants.len(), ROTATION);
        assert_ne!(
            variants[0].pitchers()[0].name,
            variants[1].pitchers()[0].name
        );
        assert_eq!(variants[0].pitchers().len(), 1 + BULLPEN);
    }
}
