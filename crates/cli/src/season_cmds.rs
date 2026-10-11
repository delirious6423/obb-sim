//! Season viewer: standings, box scores, player pages, speed check.
//!
//! The league is generated from the seed (fictional clubs). If
//! `data/internal/params.json` exists it is used for the run environment,
//! otherwise generic parameters.

use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

use calibration::{Calibration, Talent};
use season::{Club, SeasonResult, TEAMS, fictional_league, league_of, play_season, replay_game};
use sim_core::{Env, GameParams};

struct World {
    env: Env,
    clubs: Vec<Club>,
    seed: u64,
    year: u32,
}

fn load_params() -> (GameParams, Talent, &'static str) {
    match Calibration::load(Path::new("data/internal/params.json")) {
        Ok(cal) => (cal.params, cal.talent, "data/internal/params.json"),
        Err(_) => (
            GameParams::reference(),
            Talent::generic(),
            "generic parameters",
        ),
    }
}

fn world(seed: u64, year: u32) -> Result<(World, &'static str), String> {
    let (params, talent, source) = load_params();
    let env = Env::new(params).map_err(|e| format!("bad parameters: {e:?}"))?;
    let clubs = fictional_league(
        &params.league,
        &talent,
        TEAMS,
        seed ^ (u64::from(year) << 16),
    );
    Ok((
        World {
            env,
            clubs,
            seed,
            year,
        },
        source,
    ))
}

fn number(args: &[String], i: usize, name: &str) -> Result<u64, String> {
    args.get(i)
        .ok_or_else(|| format!("missing <{name}>"))?
        .parse()
        .map_err(|_| format!("<{name}> must be a number"))
}

pub fn run(args: &[String]) -> Result<(), String> {
    match args[0].as_str() {
        "season" => {
            let seed = number(args, 1, "seed").unwrap_or(1);
            let year = number(args, 2, "year").unwrap_or(0) as u32;
            let (w, source) = world(seed, year)?;
            println!("seed {seed}, season {year} ({source})\n");
            let r = play_season(&w.env, &w.clubs, w.seed, w.year);
            print!("{}", standings(&w, &r));
            print!("{}", playoffs(&w, &r));
            Ok(())
        }
        "box" => {
            let (seed, year, game) = (
                number(args, 1, "seed")?,
                number(args, 2, "year")? as u32,
                number(args, 3, "game")? as u16,
            );
            let (w, _) = world(seed, year)?;
            let schedule = season::Schedule::generate(seed, year, w.clubs[0].rotation.len());
            let f = schedule
                .fixture(game)
                .ok_or_else(|| format!("no game {game} (0..{})", schedule.games()))?;
            let away = &w.clubs[usize::from(f.away)].rotation[usize::from(f.away_starter)];
            let home = &w.clubs[usize::from(f.home)].rotation[usize::from(f.home_starter)];
            let mut log = sim_core::PlayLog::default();
            let id = season::game_id(year, f.number);
            let o = sim_core::simulate_game(&w.env, away, home, seed, id, &mut log);
            debug_assert_eq!(o, replay_game(&w.env, &w.clubs, f, seed, year));
            if args.iter().any(|a| a == "--pbp") {
                println!("{}", sim_core::play_by_play(away, home, &log.events));
            }
            println!("{}", sim_core::box_score(away, home, &o));
            Ok(())
        }
        "player" => {
            let (seed, year, team) = (
                number(args, 1, "seed")?,
                number(args, 2, "year")? as u32,
                number(args, 3, "team")? as usize,
            );
            let which = args
                .get(4)
                .ok_or("missing <b1..b9 | p1..p10> (batting slot or pitcher)")?;
            if team >= TEAMS {
                return Err(format!("team must be 0..{TEAMS}"));
            }
            let (w, _) = world(seed, year)?;
            let r = play_season(&w.env, &w.clubs, seed, year);
            print!("{}", player_page(&w, &r, team, which)?);
            Ok(())
        }
        "bench" => {
            bench(number(args, 1, "seasons").unwrap_or(200) as u32);
            Ok(())
        }
        other => Err(format!("unknown command {other}")),
    }
}

fn standings(w: &World, r: &SeasonResult) -> String {
    let mut s = String::new();
    for d in 0..6 {
        let league = if league_of(d * 5) == 0 {
            "League A"
        } else {
            "League B"
        };
        let _ = writeln!(s, "{league} division {}", d % 3 + 1);
        let mut members: Vec<usize> = (d * 5..d * 5 + 5).collect();
        members.sort_by_key(|t| {
            let rec = &r.teams[*t].record;
            std::cmp::Reverse((rec.w, rec.run_diff(), -(*t as i64)))
        });
        let lead = r.teams[members[0]].record;
        let _ = writeln!(
            s,
            "  {:<22} {:>3} {:>3}  {:>5} {:>5} {:>5}",
            "", "W", "L", "PCT", "GB", "RD"
        );
        for t in &members {
            let rec = &r.teams[*t].record;
            let games = f64::from(rec.w + rec.l);
            let gb =
                (f64::from(lead.w) - f64::from(rec.w) + f64::from(rec.l) - f64::from(lead.l)) / 2.0;
            let _ = writeln!(
                s,
                "  {:<22} {:>3} {:>3}  {:>5.3} {:>5.1} {:>+5}   (club {t})",
                w.clubs[*t].name,
                rec.w,
                rec.l,
                f64::from(rec.w) / games,
                gb,
                rec.run_diff()
            );
        }
        let _ = writeln!(s);
    }
    s
}

fn playoffs(w: &World, r: &SeasonResult) -> String {
    let names = [
        "Wild Card",
        "Division Series",
        "League Series",
        "World Series",
    ];
    let mut s = String::from("Playoffs\n");
    for series in &r.playoffs.series {
        let _ = writeln!(
            s,
            "  {:<16} {} {}-{} {}",
            names[usize::from(series.round)],
            w.clubs[usize::from(series.a)].name,
            series.a_wins,
            series.b_wins,
            w.clubs[usize::from(series.b)].name
        );
    }
    let _ = writeln!(s, "Champion: {}", w.clubs[r.playoffs.champion].name);
    s
}

fn player_page(w: &World, r: &SeasonResult, team: usize, which: &str) -> Result<String, String> {
    let club = &w.clubs[team];
    let t = &r.teams[team];
    let (kind, idx) = which.split_at(1);
    let n: usize = idx
        .parse()
        .map_err(|_| "player is b1..b9 or p1..p10".to_string())?;
    let mut s = String::new();
    match kind {
        "b" if (1..=9).contains(&n) => {
            let b = &t.batting[n - 1];
            let name = &club.rotation[0].batters()[n - 1].name;
            let _ = writeln!(s, "{name} ({}), season {}", club.name, w.year);
            let _ = writeln!(
                s,
                "G {}  PA {}  AB {}  H {}  2B {}  3B {}  HR {}  R {}  RBI {}  BB {}  SO {}  SB {}  CS {}",
                b.g,
                b.pa,
                b.ab,
                b.h,
                b.doubles,
                b.triples,
                b.hr,
                b.r,
                b.rbi,
                b.bb,
                b.so,
                b.sb,
                b.cs
            );
            let _ = writeln!(
                s,
                "AVG {:.3}  OBP {:.3}  SLG {:.3}",
                b.avg(),
                b.obp(),
                b.slg()
            );
        }
        "p" if (1..=t.pitching.len()).contains(&n) => {
            let p = &t.pitching[n - 1];
            let name = club.pitcher_name(n - 1);
            let _ = writeln!(s, "{name} ({}), season {}", club.name, w.year);
            let _ = writeln!(
                s,
                "G {}  IP {}.{}  H {}  R {}  ER {}  BB {}  SO {}  HR {}  ERA {:.2}",
                p.g,
                p.outs / 3,
                p.outs % 3,
                p.h,
                p.r,
                p.er,
                p.bb,
                p.so,
                p.hr,
                p.era()
            );
        }
        _ => return Err(format!("player is b1..b9 or p1..p{}", t.pitching.len())),
    }
    Ok(s)
}

/// Print speed numbers; CI runs this so slowdowns show up in the logs.
fn bench(seasons: u32) {
    let (w, source) = match world(1, 0) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };
    println!("bench ({source}), {seasons} seasons of 2430 games + playoffs");
    let games = f64::from(seasons) * 2430.0;
    for threads in [1, rayon::current_num_threads()] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("thread pool");
        let started = Instant::now();
        let digest = pool.install(|| {
            (0..seasons)
                .map(|s| play_season(&w.env, &w.clubs, 1, s).digest())
                .fold(0u64, |a, d| a ^ d)
        });
        let secs = started.elapsed().as_secs_f64();
        println!(
            "  {threads:>2} thread(s) (parallel days): {:>8.1} seasons/s, {:>10.0} games/s, digest {digest:016x}",
            f64::from(seasons) / secs,
            games / secs
        );
    }
}
