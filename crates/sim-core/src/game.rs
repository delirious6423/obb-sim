use crate::GameRng;

/// Final score of a played game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameResult {
    pub home_runs: u32,
    pub away_runs: u32,
}

const INNINGS: u32 = 9;
const MAX_EXTRA_INNINGS: u32 = 12;
const BASE_RUNS_PER_INNING: f64 = 0.5;
const HOME_EDGE: f64 = 1.03;

/// Expected runs in one half-inning for an offence against a defence.
fn lambda(offence: u32, defence: u32) -> f64 {
    let ratio = f64::from(offence.max(1)) / f64::from(defence.max(1));
    BASE_RUNS_PER_INNING * libm::sqrt(ratio)
}

/// Play one game. A tiny placeholder for the real engine: it is a pure function
/// of its inputs, and `league_seed` plus `game_id` pick the game's RNG stream.
#[must_use]
pub fn play_game(
    league_seed: u64,
    game_id: u64,
    home_strength: u32,
    away_strength: u32,
) -> GameResult {
    let mut rng = GameRng::for_game(league_seed, game_id);
    let home_lambda = lambda(home_strength, away_strength) * HOME_EDGE;
    let away_lambda = lambda(away_strength, home_strength);

    let mut home_runs = 0;
    let mut away_runs = 0;
    for _ in 0..INNINGS {
        away_runs += rng.poisson(away_lambda);
        home_runs += rng.poisson(home_lambda);
    }
    for _ in 0..MAX_EXTRA_INNINGS {
        if home_runs != away_runs {
            break;
        }
        away_runs += rng.poisson(away_lambda);
        home_runs += rng.poisson(home_lambda);
    }
    // No ties: after the extra-inning cap, one coin flip decides.
    if home_runs == away_runs {
        if rng.next_u64() & 1 == 0 {
            home_runs += 1;
        } else {
            away_runs += 1;
        }
    }
    GameResult {
        home_runs,
        away_runs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN_STREAM: [u64; 3] = [
        15_715_005_604_373_573_095,
        939_185_832_570_518_534,
        10_307_165_283_572_921_510,
    ];
    const GOLDEN_SCORES: [(u32, u32); 5] = [(4, 7), (5, 7), (3, 0), (4, 6), (9, 8)];

    #[test]
    fn same_seed_gives_same_result() {
        let first = play_game(42, 7, 60, 55);
        for _ in 0..100 {
            assert_eq!(play_game(42, 7, 60, 55), first);
        }
    }

    /// Pinned outputs: these catch any change in RNG, math or platform behaviour.
    #[test]
    fn golden_values() {
        let mut rng = GameRng::for_game(1, 1);
        assert_eq!(
            GOLDEN_STREAM,
            [rng.next_u64(), rng.next_u64(), rng.next_u64()]
        );
        let scores: Vec<(u32, u32)> = (0..5)
            .map(|g| {
                let r = play_game(42, g, 60, 55);
                (r.home_runs, r.away_runs)
            })
            .collect();
        assert_eq!(scores, GOLDEN_SCORES);
    }

    #[test]
    fn same_seed_gives_same_rng_stream() {
        let mut a = GameRng::for_game(1, 1);
        let mut b = GameRng::for_game(1, 1);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn games_use_independent_streams() {
        let mut a = GameRng::for_game(1, 1);
        let mut b = GameRng::for_game(1, 2);
        let a_vals: Vec<u64> = (0..8).map(|_| a.next_u64()).collect();
        let b_vals: Vec<u64> = (0..8).map(|_| b.next_u64()).collect();
        assert_ne!(a_vals, b_vals);
    }

    #[test]
    fn different_seeds_vary() {
        let results: Vec<GameResult> = (0..20).map(|s| play_game(s, 1, 50, 50)).collect();
        assert!(results.iter().any(|r| *r != results[0]));
    }

    #[test]
    fn games_never_tie() {
        for seed in 0..500 {
            let r = play_game(seed, 1, 50, 50);
            assert_ne!(r.home_runs, r.away_runs);
        }
    }

    #[test]
    fn stronger_team_wins_more_often() {
        let wins = (0..2000)
            .filter(|&g| {
                let r = play_game(9, g, 90, 40);
                r.home_runs > r.away_runs
            })
            .count();
        assert!(wins > 1200, "strong home team won only {wins}/2000");
    }
}
