# PLAN.md

This file is memory. After a context reset: read this file and `CLAUDE.md`, then continue at the first unticked item.

## How to run

- Per phase: write a short plan, build in small commits, push, check CI is green on all three OSes.
- End of phase: run the exit-gate test, then spawn an independent reviewer subagent that has not seen the work. Ask it to check the phase against the deliverables and the gate. Fix real findings. Note the rest under **Known gaps**.
- Append a log entry after each phase: what was built, the gate result, decisions made.
- Gate fails: fix the root cause, at most 2 attempts. Never change a gate threshold, golden value or test to make it pass. If it still fails, stop and report the numbers.
- Keep `CLAUDE.md` rules: determinism, pure `sim-core`, commands and events, role-filtered views, difficulty changes information and never the dice, fictional data only, every dependency logged in `THIRD_PARTY.md`.

**Stop and ask the user only when:**
- a gate still fails after 2 attempts;
- credentials, a login, a download of a *new* data source, or anything that costs money is needed (Retrosheet 2015-2019 event files are already approved);
- a choice is hard to reverse and could reasonably go either way (UI toolkit, a data format that saves depend on, a public API);
- an earlier phase's design would have to change in a way that breaks its tests.

Otherwise decide, note it in the log, keep going. **Stop after phase 4b and summarize.** Phases 5+ (desktop UI, saves, alpha, owner and commissioner roles, online) need user decisions first.

## Phase 0: workspace (done)

- [x] Six-crate workspace, CI on Linux/macOS/Windows, seeded RNG + golden tests
- [x] Commands, events, log, snapshots, replay tests; role traits and checks
- [x] Review fixes: role check in league, Actor (role + team), validated load, fallible apply

## Phase 1: single game (in progress)

Gate: `cargo test --release -- --ignored calibration` simulates 100,000 games between fictional calibration teams; league K%, BB% and HR/PA each within ±3% **relative** of the 2015-2019 target.

Tidy-up
- [x] CI: `actions/checkout@v7`, `ubuntu-24.04`
- [x] Reject played games whose teams have no players (apply + validate)
- [x] Snapshot `applied` checked against state (`commands_applied`); log `seq` == index on load
- [x] Multi-event commands atomic (clone-and-swap)

Engine (`sim-core`; pure; no I/O; data passed in)
- [x] `GameParams` (league rates, fielding, baserunning, steals) + a clearly-labelled generic `reference()` for tests
- [x] log5 plate-appearance resolver, precomputed, no per-PA allocation
- [x] Baserunning: advancement on hits/outs, double plays, sacrifice flies, rate-based steals
- [x] Fielding: in-play out -> ground/fly/line, error rate
- [x] Game loop: 9 innings, extra innings (runner on second), walk-offs, lineups, starter + relief rule
- [x] Event sink trait: no-op sink for the hot loop, play-by-play + box score renderers
- [x] Non-ignored smoke test that plays a full game (runs in CI)

Calibration (kept out of git)
- [x] `data/internal/` in `.gitignore`; download Retrosheet 2015-2019 event files (approved) into it
- [x] `tools/`: parse events; write league rates + talent spread to `data/internal/params.json`. No player id or name written anywhere.
- [x] Calibration crate: fictional rosters (generated names, talent drawn from the spread), 100k games, table of simulated vs target
- [x] `THIRD_PARTY.md`: Retrosheet notice (no Lahman entry; not used)
- [ ] Gate run + reviewer subagent + log entry

Decisions so far
- ±3% is relative. Extra innings use the runner-on-second rule.
- Universal DH: target rates are for non-pitcher batters.
- Real data yields only league-wide rates and talent spread. Rosters are fictional.

## Phase 2: full season

Gate: 1,000 simulated seasons give realistic AVG, ERA, HR and K% distributions; results identical on any core count.

- [x] (2.0a) Explain runs/team-game 4.74 vs real ~4.5 (errors, steals, extra-inning runner, walk-offs); fix cause; gate runs/team-game at ±3% relative
- [x] (2.0b) Walk-off: no runs credited past the winning run
- [x] (2.0c) Targeted test for the ghost-runner-on-base case; fix if it reproduces, else keep test and log in Known gaps
- [x] Schedule generator, standings, playoffs
- [x] rayon parallel days with per-game RNG streams (results independent of thread count)
- [x] Speed benchmarks in CI
- [x] Thin cli viewer: standings, box scores, player pages
- [x] Gate run (1,000 seasons; all checks PASS, see log)
- [x] Reviewer + fixes + log entry

## Phase 3: players over time

Gate: 50-season runs show no talent inflation or collapse and no insolvent teams; draft-slot value curves and bust rates match real-world ranges.

- [x] Player generation: hidden true talent, correlated archetypes, origin pipelines
- [x] Ratings-to-outcomes fit
- [x] Progression and aging, retirement, injuries
- [x] Minor leagues with level equivalencies
- [x] Draft: noisy team-specific boards, signability
- [x] Minimal economy: revenue, payroll cap, budget; fixed-behavior owner and commissioner agents
- [ ] Gate run + reviewer + log entry. **BLOCKED: gate fails on one metric after 2 fix attempts (see Log); needs a decision.**

## Phase 4a: manager AI

Gate: manager AI beats a baseline manager over a fixed number of seasons; a stub external agent can fill all four roles.

- [ ] Agent connectors for all four roles with typed observations and actions
- [ ] Win-probability and run-expectancy tables built from the sim
- [ ] Manager AI, decision logs, stub external-agent adapter
- [ ] Gate run + reviewer + log entry

## Phase 4b: GM AI

Gate: AI beats baseline agents at a stated confidence level; presets rank in order by win rate against the same reference bot; exploit bots gain no lasting edge over 50 seasons.

- [ ] Surplus-value GM model; trade, free-agency and draft AI with personalities
- [ ] Difficulty profiles and presets: Rookie, Veteran, All-Star, Hall of Fame (information and decision quality only, never the dice)
- [ ] Evaluation ladder with listed exploit bots
- [ ] Gate run + reviewer + log entry
- [ ] Final summary to the user

## Draft bands (Phase 3)

Sources (looked up; nothing downloaded into the repo). All count *signed* draftees unless noted; "reached the majors" = at least one major-league appearance or at-bat, measured years after the draft, so the studies are slightly right-censored (late arrivals missing).

- FanGraphs community post "Success Rate of MLB First-Round Draft Picks by Slot" (2000-2010 drafts; https://community.fangraphs.com/success-rate-of-mlb-first-round-draft-picks-by-slot/). Reached the majors per 55 picks: 1-5 48 (87%), 6-10 41 (75%), 11-15 45 (82%), 16-20 43 (78%), 21-25 36 (65%), 26-30 32 (58%); so 1-10 = 81%, 11-30 = 71%. Average bWAR (column not defined as career or per season): 12.8, 9.5, 8.7, 4.9, 6.5, 4.5. Bust = never reached, or under 1.5 bWAR per season (500 AB / 25 G seasons): 60%, 65%, 71%, 85%, 76%, 84%.
- SABR, "The Chances of a Drafted Baseball Player Making the Major Leagues" (1996-2011 drafts through round 20, data to 1 Aug 2016; https://sabr.org/?p=11541). Reached the majors by round (signed, supplemental picks included): R1 66.7%, R2 49.4%, R3 39.7%, R4 35.0%, R5 33.3%, R6-10 about 20% (17.5-24.4% by round). By round only, no pick ranges, no WAR.
- Baseball America, "How Often Do First-Rounders Make The Big Leagues?" (https://baseballamerica.com/?p=800696; years not stated): 81% of signed first-round hitters get an MLB hit and 81.3% of pitchers an MLB strikeout; second round 63-64%. Other BA/BP figures quoted in search results: 73% of first-rounders 1981-2010, 69.0% of first-rounders 1965-2009 (Baseball Prospectus), second round 46.5%.
- Conforti et al., "Major League Draft WARs" (Journal of Sports Analytics 2022; 2005-2015 drafts): average career WAR *of those who reached the majors* R1 6.93, R2 3.30, R3 2.11.

Ranges now in the gate (`crates/world/tests/world_gate.rs`): reach the majors 1-10 [0.75, 0.87], 11-30 [0.58, 0.82], 31-90 (rounds 2-3) [0.40, 0.51], 91-300 (rounds 4-10) [0.17, 0.35]; mean career WAR per draftee 1-10 [9.5, 12.8], 11-30 [4.5, 8.7], 31-90 [0.8, 3.3] (reach rate x Conforti's among-reached means, up to the among-reached means themselves); bust (never reached or under 1.5 WAR per season) 1-10 [0.60, 0.65], 11-30 [0.71, 0.85]. No source was found for 91-300 WAR and bust (kept [0, 3] and [0.75, 1.0] from the first draft) or for 31-90 bust (not gated).

Comparison of definitions: the sim's "reached" is at least one season with a plate appearance or batter faced at the majors level, over signed draftees, measured 15+ years after the draft (no censoring); the sim's WAR is runs above a replacement of 0.028 runs/PA over a volume of 650 PA (batters) / 450 BF (pitchers) per 10 runs; the sim's bust uses the FanGraphs rule on seasons with major-league playing time. The sourced 11-30 range (0.58-0.82, pooled 0.71) did not include the first sim value of 0.84, so the model was off and step 2 applied.

## Known gaps

- Calibration gate is close to tautological for K/BB/HR (rosters are centred on the target); runs/team-game is now gated and does depend on baserunning/fielding.
- Runs target is DH-equivalent: real 4.48 + pitcher-PA replacement (textbook linear weights, +0.22) = 4.70. Sim is 4.70 under 2015-2019 rules (`extra_inning_runner: false`); the extra-inning runner adds ~0.04. The earlier 4.74 was these two effects, not an engine bug.
- Extra-inning games: sim 9.7% vs real 8.3% (not gated). Likely game-to-game run variance is too low (no park/weather/starter spread); revisit with Phase 2/3.
- Errors per team-game in the sim (~0.32) are about half the real total because only reached-on-error is modelled (not throwing errors that advance runners).
- Errors advance only the batter (and forced runners), not other runners.
- Ghost runner at bat while on base: could not be reproduced (24,000 high-traffic games under a debug assertion; reasoning also shows it cannot happen). The test stays as a guard.
- cli: Phase 1 viewer is `cli game <seed> --pbp` (no `sim` prefix).
- `ai` views: `of(league, team)` constructors accept any team id; scoping depends on the caller.
- `League::from_events` leaves `commands_applied` at 0 (events carry no commands).
- Role/team binding covers only commands that name teams; commands that name a game or player will need their team resolved from state in `league::permissions::target_teams`.

- Season: no series, travel, off-day or weather structure; fixed lineup and 5-man rotation all season (no injuries/rest until Phase 3); playoffs seeded by wins, run difference, club number (no head-to-head).
- Unearned runs: engine now uses the virtual-outs rule, but non-reached-base errors are not modelled, so ER/R is 0.96 vs about 0.93 real; league ERA 4.56 vs about 4.4 for the DH-equivalent environment.
- Season gate bands for spread (0.6-1.5x of real team-season SD, ERA/win% ranges) were fixed before the first run; team AVG spread runs 1.21x real, K%/HR 0.8x.

- Season gate: ERA band (3.9-4.6) and win%/ERA spread bands are hard-coded, not derived from data; spread bands 0.6-1.5x are loose (a 20% error would pass). Derive and tighten when real ERA/win% distributions are added to the tool. K% check compares to the engine's own input rate (same as the real rate).
- Playoff starters resume at `162 % rotation`, not at the club's actual rotation position; ties are broken by run difference then club number (no head-to-head).
- DH adjustment in the tool uses league rates that include pitchers' own PAs (about 0.2% of runs).

- Phase 3 (world crate): design is `world` crate with strict events (`World::apply`), commands via `World::execute`, driver `run_year`, agents (`GmView` shows only noisy opinions); founding runs a burn-in of offseasons (no games) from a seeded population, with opening cash restored afterwards.
- Aging curve (peak 27, deficit by age, decline from 30), injury rates, amateur pool (spread 1.3, offsets 2.2 batters / 2.0 pitchers below league average), scouting noise (1.5 quality-sd on amateurs) and age-scaled development luck are assumptions tuned for stability, not fitted to data (no age data source approved).
- Minors are roster tiers only (no minor-league games); minor leaguers get the same injury rolls as major leaguers; level equivalencies are not modelled.
- Majors still hold some 17-20 year-olds (about 4% of major leaguers) when a teenager out-scouts the field; BB% runs about +10% over the real league (selection on walk-heavy talent), runs/team-game about +4% (4.90 vs 4.70).
- Free agency is one signing per club per round with fixed GM behaviour; releases cost the remaining salary; contracts are salary x years only (no arbitration, no options, no trades yet).
- Phase 3 follow-up (after the draft bands were grounded): one bounded model attempt (young players' yearly development luck 0.22 -> 0.55 scaled by youth; pool offsets recalibrated to 2.5 batters / 2.4 pitchers; founding burn-in 12 -> 30 years because the league mix was still settling) brought "reach the majors" for picks 1-10, 11-30, 31-90 into or next to the sourced ranges, but mean career WAR stays about 2x the sourced range in every bucket (picks 1-10 about 20-25 vs 9.5-12.8), bust rates are 0.47-0.52 / 0.64-0.69 vs 0.60-0.65 / 0.71-0.85, and picks 91-300 reach 0.13 vs 0.17-0.35. Accepted and logged; `draft_gate` fails on all five seeds.
- Phase 3 stability gate over five seeds: K%, BB%, level of HR% and runs, major-league quality worst-season band and solvency pass everywhere; the decade-over-decade ratio check fails on some seeds (HR/PA ratio 1.096 on seed 1, 0.9496 on seed 2 against 0.95-1.05; batter quality drift 0.176 sd on seed 1 against 0.15). Cause: league mix wanders because nothing anchors it (no feedback from run environment to talent) and talent turnover is slow. Across 20 other seeds the mean decade ratio is 1.00-1.02 for K/BB/HR/runs (no trend) with standard deviations of 2.2% (K), 3.2% (BB), 4.4% (HR), 1.9% (runs), so a trend-free model exceeds a +-5% band on a given metric in roughly 10-25% of seeds. Not changed (thresholds fixed); the band or the statistic (mean ratio across seeds, or a per-seed trend test) needs a decision.
- Phase 3 information and limits: asking salaries and amateur bonus demands now come from a market view that is no better informed than a scout (`scout::public_quality`, `scout::amateur_demand`); `apply` validates events before changing anything and enforces roster limits at season start (13 majors per kind, level capacities, 49 per kind) and a hard 60 per kind at signing; club spending that would take cash below -25M is refused (`CREDIT`); bonuses and released contracts show up as `spent` in the next season's books. Not yet done: a commissioner force-pass for a stalled draft.

## Log

- **Phase 0**: workspace, deterministic sim-core, command/event league, role checks. CI green on ubuntu, macOS, Windows; golden values identical on all three.
- **Phase 1**: log5 engine, baserunning, fielding, relief, box score/play-by-play; Retrosheet 2015-2019 aggregate tool; calibration crate. Gate PASS on 100k games: K +0.58%, BB -0.44%, HR/PA -0.42% (all within ±3%). Fixed a parser bug (force outs counted batter out) found by sanity check; reviewer found no blockers; fixed bases-loaded double play advancement. CI green on 3 OSes.
- **Phase 2.0 (pre-work)**: runs/team-game explained (pitcher batting vs universal DH +0.22, extra-inning runner +0.04; not an engine bug) and gated at ±3% against a DH-equivalent target (sim 4.697 vs 4.699). `GameParams.extra_inning_runner` rule flag added (calibration uses 2015-2019 rules). Walk-off no longer credits runs past the winning run (home runs excepted). Ghost-runner guard test added.
- **Phase 2 (built)**: `season` crate: 162-game schedule for 30 clubs (divisions, interleague rotation, home/away balanced, days), play loop with rayon-parallel days and per-game streams (game id = season<<20 | game), standings, 12-team playoffs with series, summaries; cli `season`, `box [--pbp]`, `player`, `bench`; CI runs `bench`. Gate (`cargo test --release -p season -- --ignored season_gate --nocapture`): 1,000 seasons in 1.6 s on 10 threads, identical digests on 1 thread and all threads; league AVG .2556 vs .2539, K% 21.10 vs 21.14, HR/PA 3.207 vs 3.204, ERA 4.56, team-season spreads inside bands. Two gate-definition fixes before any pass: HR target changed from HR/game to HR/PA (per-game compared different PA counts); earned runs now use virtual outs after an error (ERA was 4.66, ER/R 0.98).
- **Phase 1 tidy-up**: four small commits closing the remaining review gaps (see checklist).
- **Phase 3 (built, gate NOT met)**: `world` crate (hidden-talent players, aging, injuries incl. minor leaguers, draft with noisy club boards and signability, free agency, revenue/payroll/budget economy, commands -> events with strict apply, fixed GM and owner agents, burn-in founding). Tests: determinism, event replay equals state, role/phase/club enforcement, roster invariants. Gate (`cargo test --release -p world --test world_gate -- --ignored --nocapture`, 50 seasons): league K/BB/HR/runs inside bands and drift-free, major-league quality drift under 0.15 sd, no insolvent club (lowest cash > 0) all PASS. FAIL: picks 11-30 reach the majors 0.84 vs band 0.40-0.80 (other draft metrics pass: picks 1-10 0.94/WAR 26/bust 0.10; 31-90 0.54/WAR 4.0; 91-300 0.12/WAR 0.3/bust 0.92). Fix attempt 1: amateur scouting noise 0.8/0.7 -> 1.5/1.5 and age-scaled development luck (before: top-10 reach 0.99, WAR 37). Fix attempt 2: opening cash restored after burn-in (cause of negative cash on other seeds) and injuries for minor leaguers (no effect on the failing metric). Gate thresholds were not changed (one test bug fixed: runs/team-game was divided by 2 twice). Stopped per the directive; needs a decision.
- **Phase 3 follow-up**: sourced draft bands (see "Draft bands"), gates split into `stability_gate` and `draft_gate` over five seeds, information leaks closed, apply-side validation, roster limits, cash floor, new tests (retirement, expiry, release dead money, levels, draft bonus refusal, event validation). Both gates still fail; see Known gaps for the numbers.
