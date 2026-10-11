//! Phase 2 exit gate. Ignored by default (needs the git-ignored
//! `data/internal/params.json` and takes a while):
//!
//!     cargo test --release -p season -- --ignored season_gate --nocapture
//!
//! Plays 1,000 seasons, each with freshly generated fictional clubs, and checks
//! the distributions of AVG, K%, HR and ERA against 2015-2019:
//!
//! - league means of AVG, K% and HR per team-game within +-3% (relative);
//! - league ERA mean inside 3.9-4.6 (the range of real league ERAs, 2015-2019,
//!   widened for the designated-hitter-equivalent run environment) and every
//!   single season inside 3.6-5.0;
//! - the spread across team-seasons (standard deviation) of AVG, K% and HR per
//!   game between 0.6x and 1.5x of the real spread; ERA spread 0.25-0.70;
//!   win percentage spread 0.045-0.085;
//! - season-to-season league AVG range under 0.015;
//! - every season's results identical on 1 thread and on all threads.

use std::path::Path;

use calibration::Calibration;
use rayon::prelude::*;
use season::{Club, SeasonSummary, TEAMS, fictional_league, play_season, summarize};
use sim_core::Env;

const SEASONS: u32 = 1_000;
const SEED: u64 = 20_240_901;
const TOLERANCE: f64 = 0.03;

fn mean_sd(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, var.sqrt())
}

fn run(env: &Env, cal: &Calibration) -> Vec<(u64, SeasonSummary)> {
    (0..SEASONS)
        .into_par_iter()
        .map(|s| {
            let clubs: Vec<Club> = fictional_league(
                &cal.params.league,
                &cal.talent,
                TEAMS,
                SEED ^ (u64::from(s) << 16),
            );
            let r = play_season(env, &clubs, SEED, s);
            (r.digest(), summarize(&r))
        })
        .collect()
}

#[test]
#[ignore = "needs data/internal/params.json; run with --release -- --ignored season_gate"]
fn season_gate() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json");
    let cal = Calibration::load(&path).unwrap_or_else(|e| panic!("{e}"));
    let env = Env::new(cal.params).unwrap();
    let target = cal.run_environment;

    let started = std::time::Instant::now();
    let all = run(&env, &cal);
    let elapsed = started.elapsed();
    let one_thread = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap()
        .install(|| run(&env, &cal));
    let digests_match = all.iter().map(|x| x.0).eq(one_thread.iter().map(|x| x.0));

    let col = |f: fn(&SeasonSummary) -> f64| all.iter().map(|x| f(&x.1)).collect::<Vec<_>>();
    let league_avg = col(|s| s.league.avg);
    let league_k = col(|s| s.league.k_pct);
    let league_hr = col(|s| s.league.hr_pct);
    let league_era = col(|s| s.league.era);
    let league_runs = col(|s| s.league.runs_per_game);
    let team = |f: fn(&season::TeamLine) -> f64| {
        all.iter()
            .flat_map(|x| x.1.teams.iter().map(f))
            .collect::<Vec<_>>()
    };
    let t_avg = mean_sd(&team(|t| t.avg));
    let t_k = mean_sd(&team(|t| t.k_pct));
    let t_hr = mean_sd(&team(|t| t.hr_pct));
    let t_era = mean_sd(&team(|t| t.era));
    let t_win = mean_sd(&team(|t| t.win_pct));

    let real = &target.team_season;
    let k_target = cal.params.league.0[0];
    let mut failures: Vec<String> = Vec::new();
    let mut check = |name: &str, ok: bool, detail: String| {
        println!(
            "{:<34} {:<4} {}",
            name,
            if ok { "PASS" } else { "FAIL" },
            detail
        );
        if !ok {
            failures.push(name.to_string());
        }
    };
    let rel = |v: f64, t: f64| v / t - 1.0;

    println!(
        "\n{SEASONS} seasons, {} team-seasons, {:.1?} on all threads",
        SEASONS as usize * TEAMS,
        elapsed
    );
    let (m, sd) = mean_sd(&league_avg);
    check(
        "league AVG mean",
        rel(m, target.avg).abs() <= TOLERANCE,
        format!(
            "{m:.4} vs {:.4} ({:+.2}%), season sd {sd:.4}",
            target.avg,
            100.0 * rel(m, target.avg)
        ),
    );
    let (m, sd) = mean_sd(&league_k);
    check(
        "league K% mean",
        rel(m, k_target).abs() <= TOLERANCE,
        format!(
            "{:.2}% vs {:.2}% ({:+.2}%), season sd {:.2}pp",
            100.0 * m,
            100.0 * k_target,
            100.0 * rel(m, k_target),
            100.0 * sd
        ),
    );
    let (m, sd) = mean_sd(&league_hr);
    check(
        "league HR/PA mean",
        rel(m, real.hr_pct.mean).abs() <= TOLERANCE,
        format!(
            "{:.3}% vs {:.3}% ({:+.2}%), season sd {:.3}pp",
            100.0 * m,
            100.0 * real.hr_pct.mean,
            100.0 * rel(m, real.hr_pct.mean),
            100.0 * sd
        ),
    );
    let (m, sd) = mean_sd(&league_era);
    let era_lo = league_era.iter().copied().fold(f64::MAX, f64::min);
    let era_hi = league_era.iter().copied().fold(f64::MIN, f64::max);
    check(
        "league ERA mean",
        (3.9..=4.6).contains(&m),
        format!("{m:.2} (3.9-4.6), season sd {sd:.2}"),
    );
    check(
        "league ERA every season",
        era_lo >= 3.6 && era_hi <= 5.0,
        format!("{era_lo:.2} .. {era_hi:.2} (3.6-5.0)"),
    );
    let (m, _) = mean_sd(&league_runs);
    check(
        "league runs/team-game mean",
        rel(m, target.runs_per_team_game_dh).abs() <= TOLERANCE,
        format!("{m:.3} vs {:.3}", target.runs_per_team_game_dh),
    );
    let avg_lo = league_avg.iter().copied().fold(f64::MAX, f64::min);
    let avg_hi = league_avg.iter().copied().fold(f64::MIN, f64::max);
    check(
        "league AVG season range",
        avg_hi - avg_lo < 0.015,
        format!("{avg_lo:.4} .. {avg_hi:.4}"),
    );
    for (name, sim, real_sd) in [
        ("team AVG spread", t_avg.1, real.avg.sd),
        ("team K% spread", t_k.1, real.k_pct.sd),
        ("team HR/PA spread", t_hr.1, real.hr_pct.sd),
    ] {
        let ratio = sim / real_sd;
        check(
            name,
            (0.6..=1.5).contains(&ratio),
            format!("sd {sim:.4} vs real {real_sd:.4} (x{ratio:.2}, band 0.6-1.5)"),
        );
    }
    check(
        "team ERA spread",
        (0.25..=0.70).contains(&t_era.1),
        format!("sd {:.3} (0.25-0.70), mean {:.2}", t_era.1, t_era.0),
    );
    check(
        "team win% spread",
        (0.045..=0.085).contains(&t_win.1),
        format!("sd {:.4} (0.045-0.085)", t_win.1),
    );
    check(
        "identical on 1 thread and all",
        digests_match,
        format!("{} seasons compared", all.len()),
    );

    assert!(failures.is_empty(), "failed: {failures:?}");
}
