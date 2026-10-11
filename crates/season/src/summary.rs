use crate::season::SeasonResult;
use crate::totals::ratio;

/// One club's season in the usual rate statistics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TeamLine {
    pub win_pct: f64,
    pub runs_per_game: f64,
    pub avg: f64,
    pub k_pct: f64,
    pub hr_per_game: f64,
    pub hr_pct: f64,
    pub era: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeagueLine {
    pub runs_per_game: f64,
    pub avg: f64,
    pub k_pct: f64,
    pub hr_per_game: f64,
    pub hr_pct: f64,
    pub era: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeasonSummary {
    pub league: LeagueLine,
    pub teams: Vec<TeamLine>,
}

/// Rate statistics for a season, for each club and for the league.
#[must_use]
pub fn summarize(r: &SeasonResult) -> SeasonSummary {
    let mut teams = Vec::with_capacity(r.teams.len());
    let (mut pa, mut ab, mut h, mut so, mut hr) = (0u32, 0u32, 0u32, 0u32, 0u32);
    let (mut er, mut outs, mut runs, mut games) = (0u32, 0u32, 0u32, 0u32);
    for t in &r.teams {
        let (mut tpa, mut tab, mut th, mut tso, mut thr) = (0, 0, 0, 0, 0);
        for b in &t.batting {
            tpa += b.pa;
            tab += b.ab;
            th += b.h;
            tso += b.so;
            thr += b.hr;
        }
        let (mut ter, mut touts) = (0, 0);
        for p in &t.pitching {
            ter += p.er;
            touts += p.outs;
        }
        let g = u32::from(t.record.w + t.record.l);
        teams.push(TeamLine {
            win_pct: ratio(u32::from(t.record.w), g),
            runs_per_game: ratio(t.record.rs, g),
            avg: ratio(th, tab),
            k_pct: ratio(tso, tpa),
            hr_per_game: ratio(thr, g),
            hr_pct: ratio(thr, tpa),
            era: 27.0 * ratio(ter, touts),
        });
        pa += tpa;
        ab += tab;
        h += th;
        so += tso;
        hr += thr;
        er += ter;
        outs += touts;
        runs += t.record.rs;
        games += g;
    }
    SeasonSummary {
        league: LeagueLine {
            runs_per_game: ratio(runs, games),
            avg: ratio(h, ab),
            k_pct: ratio(so, pa),
            hr_per_game: ratio(hr, games),
            hr_pct: ratio(hr, pa),
            era: 27.0 * ratio(er, outs),
        },
        teams,
    }
}
