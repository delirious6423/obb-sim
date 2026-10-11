//! Season tests that run in CI with generic (reference) parameters.

use calibration::Talent;
use season::{
    Club, GAMES_PER_TEAM, Schedule, TEAMS, division_of, fictional_league, league_of, pair_games,
    play_season, replay_game, summarize,
};
use sim_core::{Env, GameParams};

fn setup() -> (Env, Vec<Club>) {
    let params = GameParams::reference();
    let clubs = fictional_league(&params.league, &Talent::generic(), TEAMS, 11);
    (Env::new(params).unwrap(), clubs)
}

#[test]
fn schedule_is_complete_balanced_and_conflict_free() {
    for season in [0, 1, 7] {
        let s = Schedule::generate(99, season, 5);
        assert_eq!(s.games(), TEAMS * usize::from(GAMES_PER_TEAM) / 2);
        let mut games = [0u16; TEAMS];
        let mut home = [0u16; TEAMS];
        let mut pair = [[0u8; TEAMS]; TEAMS];
        let mut number = 0u16;
        for day in &s.days {
            let mut busy = [false; TEAMS];
            for f in day {
                assert_eq!(f.number, number, "numbers run in order");
                number += 1;
                let (a, h) = (usize::from(f.away), usize::from(f.home));
                assert_ne!(a, h);
                assert!(!busy[a] && !busy[h], "a club plays twice in one day");
                busy[a] = true;
                busy[h] = true;
                games[a] += 1;
                games[h] += 1;
                home[h] += 1;
                pair[a][h] += 1;
                pair[h][a] += 1;
                assert!(usize::from(f.away_starter) < 5 && usize::from(f.home_starter) < 5);
            }
        }
        assert!(s.days.len() <= 200, "{} days", s.days.len());
        assert!(games.iter().all(|g| *g == GAMES_PER_TEAM));
        assert!(home.iter().all(|h| *h == GAMES_PER_TEAM / 2));
        for (a, row) in pair.iter().enumerate() {
            for (b, n) in row.iter().enumerate() {
                if a != b {
                    assert_eq!(*n, pair_games(a, b, season));
                }
            }
        }
    }
}

#[test]
fn divisions_and_leagues_are_laid_out_as_documented() {
    assert_eq!(league_of(14), 0);
    assert_eq!(league_of(15), 1);
    assert_eq!(division_of(4), 0);
    assert_eq!(division_of(29), 5);
    // Each club meets nine opponents in the other league, twice each.
    for t in 0..TEAMS {
        let other: u32 = (0..TEAMS)
            .filter(|u| league_of(*u) != league_of(t))
            .map(|u| u32::from(pair_games(t, u, 3)))
            .sum();
        assert_eq!(other, 18);
    }
}

#[test]
fn schedule_depends_on_seed_and_season() {
    let a = Schedule::generate(1, 0, 5);
    assert_eq!(a, Schedule::generate(1, 0, 5));
    assert_ne!(a, Schedule::generate(2, 0, 5));
    assert_ne!(a, Schedule::generate(1, 1, 5));
}

#[test]
fn season_adds_up_and_playoffs_crown_a_champion() {
    let (env, clubs) = setup();
    let r = play_season(&env, &clubs, 5, 0);
    let wins: u32 = r.teams.iter().map(|t| u32::from(t.record.w)).sum();
    let losses: u32 = r.teams.iter().map(|t| u32::from(t.record.l)).sum();
    assert_eq!(wins, 2430);
    assert_eq!(losses, 2430);
    assert!(
        r.teams
            .iter()
            .all(|t| t.record.w + t.record.l == GAMES_PER_TEAM)
    );
    let rs: u32 = r.teams.iter().map(|t| t.record.rs).sum();
    let ra: u32 = r.teams.iter().map(|t| t.record.ra).sum();
    assert_eq!(rs, ra);

    // Batting and pitching totals agree with the scoreboard.
    for t in &r.teams {
        let runs: u32 = t.batting.iter().map(|b| b.r).sum();
        assert_eq!(runs, t.record.rs);
        let allowed: u32 = t.pitching.iter().map(|p| p.r).sum();
        assert_eq!(allowed, t.record.ra);
    }

    // Replaying every game from its stream reproduces the season's runs.
    let replayed: u32 = r
        .schedule
        .days
        .iter()
        .flatten()
        .map(|f| {
            let o = replay_game(&env, &clubs, f, 5, 0);
            u32::from(o.runs[0]) + u32::from(o.runs[1])
        })
        .sum();
    assert_eq!(replayed, rs);

    // 4 wild card + 4 division + 2 league + 1 final.
    assert_eq!(r.playoffs.series.len(), 11);
    let rounds: Vec<u8> = r.playoffs.series.iter().map(|s| s.round).collect();
    assert_eq!(rounds, [0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 3]);
    let last = r.playoffs.series.last().unwrap();
    assert_eq!(usize::from(last.winner()), r.playoffs.champion);
    for s in &r.playoffs.series {
        let need = match s.round {
            0 => 2,
            1 => 3,
            _ => 4,
        };
        assert!(s.a_wins.max(s.b_wins) == need && s.a_wins.min(s.b_wins) < need);
    }
    // Seeds are twelve different clubs, six per league.
    let mut all: Vec<u8> = r.playoffs.seeds.iter().flatten().copied().collect();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 12);
    let s = summarize(&r);
    assert!((s.league.runs_per_game - 4.5).abs() < 1.0, "{:?}", s.league);
}

#[test]
fn results_do_not_depend_on_thread_count() {
    let (env, clubs) = setup();
    let digest = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        pool.install(|| {
            (0..3)
                .map(|s| play_season(&env, &clubs, 77, s).digest())
                .collect::<Vec<_>>()
        })
    };
    let one = digest(1);
    assert_eq!(one, digest(2));
    assert_eq!(one, digest(7));
    assert!(one[0] != one[1], "different seasons differ");
}
