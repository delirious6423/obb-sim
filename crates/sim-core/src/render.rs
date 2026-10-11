//! Text output: a box score from a finished game, and play-by-play from the
//! events a [`PlayLog`](crate::PlayLog) recorded. Pure functions that return strings.

use std::fmt::Write as _;

use crate::events::{Ball, GameEvent, Half};
use crate::rates::Outcome;
use crate::stats::GameOutcome;
use crate::team::{LINEUP, Team};

fn ordinal(n: u8) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

fn innings_pitched(outs: u8) -> String {
    format!("{}.{}", outs / 3, outs % 3)
}

/// A readable box score: line score, then batting and pitching for each team.
#[must_use]
pub fn box_score(away: &Team, home: &Team, game: &GameOutcome) -> String {
    let teams = [away, home];
    let name_w = teams
        .iter()
        .flat_map(|t| {
            std::iter::once(t.name.len())
                .chain(t.batters().iter().take(LINEUP).map(|p| p.name.len()))
                .chain(t.pitchers().iter().map(|p| p.name.len()))
        })
        .max()
        .unwrap_or(10)
        .max(10);
    let innings = usize::from(game.innings);

    let mut s = String::new();
    // Line score.
    let _ = write!(s, "{:name_w$} ", "");
    for i in 1..=innings {
        let _ = write!(s, "{:>2}", i % 100);
    }
    let _ = writeln!(s, "   {:>2} {:>2} {:>2}", "R", "H", "E");
    for (side, team) in teams.iter().enumerate() {
        let _ = write!(s, "{:name_w$} ", team.name);
        for i in 0..innings {
            if side == 1 && i == innings - 1 && !game.home_batted_last {
                let _ = write!(s, "{:>2}", "x");
            } else {
                let _ = write!(s, "{:>2}", game.line[side][i]);
            }
        }
        let _ = writeln!(
            s,
            "   {:>2} {:>2} {:>2}",
            game.runs[side],
            game.hits(side),
            game.errors[1 - side]
        );
    }

    for (side, team) in teams.iter().enumerate() {
        let _ = writeln!(s, "\n{} batting", team.name);
        let _ = writeln!(
            s,
            "{:name_w$} {:>3} {:>3} {:>3} {:>3} {:>3} {:>3}",
            "", "AB", "R", "H", "RBI", "BB", "SO"
        );
        let mut totals = [0u32; 6];
        for (slot, b) in game.bat[side].iter().enumerate() {
            let name = &team.batters()[slot].name;
            let _ = writeln!(
                s,
                "{:name_w$} {:>3} {:>3} {:>3} {:>3} {:>3} {:>3}",
                name,
                b.ab,
                b.r,
                b.h,
                b.rbi,
                u16::from(b.bb) + u16::from(b.hbp),
                b.so
            );
            for (t, v) in
                totals
                    .iter_mut()
                    .zip([b.ab, b.r, b.h, b.rbi, b.bb.saturating_add(b.hbp), b.so])
            {
                *t += u32::from(v);
            }
        }
        let _ = writeln!(
            s,
            "{:name_w$} {:>3} {:>3} {:>3} {:>3} {:>3} {:>3}",
            "Totals", totals[0], totals[1], totals[2], totals[3], totals[4], totals[5]
        );
        let mut notes = Vec::new();
        let list = |f: &dyn Fn(&crate::stats::BatLine) -> u8| -> Vec<String> {
            game.bat[side]
                .iter()
                .enumerate()
                .filter(|(_, b)| f(b) > 0)
                .map(|(slot, b)| {
                    let name = &team.batters()[slot].name;
                    if f(b) > 1 {
                        format!("{name} {}", f(b))
                    } else {
                        name.clone()
                    }
                })
                .collect()
        };
        for (label, items) in [
            ("2B", list(&|b| b.doubles)),
            ("3B", list(&|b| b.triples)),
            ("HR", list(&|b| b.hr)),
            ("SB", list(&|b| b.sb)),
            ("CS", list(&|b| b.cs)),
            ("SF", list(&|b| b.sf)),
        ] {
            if !items.is_empty() {
                notes.push(format!("{label}: {}", items.join(", ")));
            }
        }
        if !notes.is_empty() {
            let _ = writeln!(s, "{}", notes.join("; "));
        }

        let _ = writeln!(s, "\n{} pitching", team.name);
        let _ = writeln!(
            s,
            "{:name_w$} {:>4} {:>3} {:>3} {:>3} {:>3} {:>3} {:>3}",
            "", "IP", "H", "R", "ER", "BB", "SO", "NP"
        );
        for (i, p) in game.pitch[side].iter().enumerate() {
            if !p.used {
                continue;
            }
            let _ = writeln!(
                s,
                "{:name_w$} {:>4} {:>3} {:>3} {:>3} {:>3} {:>3} {:>3}",
                team.pitchers()[i].name,
                innings_pitched(p.outs),
                p.h,
                p.r,
                p.er,
                u16::from(p.bb) + u16::from(p.hbp),
                p.so,
                p.pitches
            );
        }
    }
    s
}

/// Play-by-play text, one line per play, with the score after each scoring play.
#[must_use]
pub fn play_by_play(away: &Team, home: &Team, events: &[GameEvent]) -> String {
    let teams = [away, home];
    let mut s = String::new();
    let mut bat = 0usize;
    let mut score = [0u32; 2];

    for event in events {
        match *event {
            GameEvent::HalfInningStart { inning, half } => {
                bat = usize::from(half == Half::Bottom);
                let _ = writeln!(
                    s,
                    "\n{} of the {} - {} batting",
                    if bat == 0 { "Top" } else { "Bottom" },
                    ordinal(inning),
                    teams[bat].name
                );
            }
            GameEvent::GhostRunner { runner } => {
                let _ = writeln!(
                    s,
                    "  {} starts the inning on second base.",
                    teams[bat].batters()[usize::from(runner)].name
                );
            }
            GameEvent::PitcherChange { half, pitcher } => {
                let fielding = usize::from(half == Half::Top);
                let _ = writeln!(
                    s,
                    "  {} comes in to pitch for {}.",
                    teams[fielding].pitchers()[usize::from(pitcher)].name,
                    teams[fielding].name
                );
            }
            GameEvent::Steal {
                runner,
                base,
                success,
            } => {
                let name = &teams[bat].batters()[usize::from(runner)].name;
                let verb = if success {
                    "steals"
                } else {
                    "is caught stealing"
                };
                let _ = writeln!(s, "  {name} {verb} {}.", ordinal(base));
            }
            GameEvent::Pa(pa) => {
                let name = &teams[bat].batters()[usize::from(pa.batter)].name;
                let what = match (pa.outcome, pa.ball) {
                    (Outcome::Strikeout, _) => "strikes out",
                    (Outcome::Walk, _) => "walks",
                    (Outcome::HitByPitch, _) => "is hit by a pitch",
                    (Outcome::Single, _) => "singles",
                    (Outcome::Double, _) => "doubles",
                    (Outcome::Triple, _) => "triples",
                    (Outcome::HomeRun, _) => "homers",
                    (Outcome::InPlayOut, _) if pa.error => "reaches on a fielding error",
                    (Outcome::InPlayOut, _) if pa.double_play => "grounds into a double play",
                    (Outcome::InPlayOut, _) if pa.sac_fly => "hits a sacrifice fly",
                    (Outcome::InPlayOut, Some(Ball::Ground)) => "grounds out",
                    (Outcome::InPlayOut, Some(Ball::Fly)) => "flies out",
                    (Outcome::InPlayOut, Some(Ball::Line)) => "lines out",
                    (Outcome::InPlayOut, None) => "is out",
                };
                let _ = write!(s, "  {name} {what}.");
                if pa.runs > 0 {
                    score[bat] += u32::from(pa.runs);
                    let _ = write!(
                        s,
                        " {} run{} score{}. ({} {}, {} {})",
                        pa.runs,
                        if pa.runs == 1 { "" } else { "s" },
                        if pa.runs == 1 { "s" } else { "" },
                        teams[0].name,
                        score[0],
                        teams[1].name,
                        score[1]
                    );
                }
                if pa.outs_after >= 3 {
                    let _ = write!(s, " Side retired.");
                } else if pa.outs_after > 0 {
                    let _ = write!(s, " ({} out)", pa.outs_after);
                }
                let _ = writeln!(s);
            }
            GameEvent::GameOver => {
                let _ = writeln!(
                    s,
                    "\nFinal: {} {}, {} {}",
                    teams[0].name, score[0], teams[1].name, score[1]
                );
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::simulate_game;
    use crate::events::PlayLog;
    use crate::params::{Env, GameParams};
    use crate::team::Player;

    fn team(name: &str) -> Team {
        let league = GameParams::reference().league;
        let mk = |kind: &str, i: usize| Player {
            name: format!("{name} {kind} {i}"),
            rates: league,
        };
        Team::new(
            name,
            (1..=9).map(|i| mk("Batter", i)).collect(),
            (1..=6).map(|i| mk("Pitcher", i)).collect(),
        )
        .unwrap()
    }

    #[test]
    fn box_score_and_play_by_play_read_sensibly() {
        let env = Env::new(GameParams::reference()).unwrap();
        let (away, home) = (team("Pine Ridge Foxes"), team("Harbor City Gulls"));
        let mut log = PlayLog::default();
        let game = simulate_game(&env, &away, &home, 42, 1, &mut log);

        let box_text = box_score(&away, &home, &game);
        assert!(box_text.contains("Pine Ridge Foxes batting"));
        assert!(box_text.contains("Harbor City Gulls pitching"));
        assert!(box_text.contains("Totals"));
        // The R column of the line score matches the game.
        let first_team_line = box_text
            .lines()
            .find(|l| l.starts_with("Pine Ridge Foxes ") && !l.contains("batting"))
            .unwrap();
        assert!(
            first_team_line
                .split_whitespace()
                .rev()
                .nth(2)
                .is_some_and(|r| r == game.runs[0].to_string()),
            "{first_team_line}"
        );

        let pbp = play_by_play(&away, &home, &log.events);
        assert!(pbp.contains("Top of the 1st"));
        assert!(pbp.contains("Bottom of the 1st") || !game.home_batted_last);
        assert!(pbp.contains(&format!(
            "Final: Pine Ridge Foxes {}, Harbor City Gulls {}",
            game.runs[0], game.runs[1]
        )));
        // One line per plate appearance.
        let pa_events = log
            .events
            .iter()
            .filter(|e| matches!(e, GameEvent::Pa(_)))
            .count();
        let pa_total: u32 = game.bat.iter().flatten().map(|b| u32::from(b.pa)).sum();
        assert_eq!(pa_events as u32, pa_total);
    }

    #[test]
    fn ordinals() {
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(3), "3rd");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(12), "12th");
        assert_eq!(ordinal(21), "21st");
    }

    #[test]
    fn innings_pitched_notation() {
        assert_eq!(innings_pitched(0), "0.0");
        assert_eq!(innings_pitched(19), "6.1");
        assert_eq!(innings_pitched(27), "9.0");
    }
}
