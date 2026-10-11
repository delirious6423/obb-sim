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

- [ ] Player generation: hidden true talent, correlated archetypes, origin pipelines
- [ ] Ratings-to-outcomes fit
- [ ] Progression and aging, retirement, injuries
- [ ] Minor leagues with level equivalencies
- [ ] Draft: noisy team-specific boards, signability
- [ ] Minimal economy: revenue, payroll cap, budget; fixed-behavior owner and commissioner agents
- [ ] Gate run + reviewer + log entry

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

## Log

- **Phase 0**: workspace, deterministic sim-core, command/event league, role checks. CI green on ubuntu, macOS, Windows; golden values identical on all three.
- **Phase 1**: log5 engine, baserunning, fielding, relief, box score/play-by-play; Retrosheet 2015-2019 aggregate tool; calibration crate. Gate PASS on 100k games: K +0.58%, BB -0.44%, HR/PA -0.42% (all within ±3%). Fixed a parser bug (force outs counted batter out) found by sanity check; reviewer found no blockers; fixed bases-loaded double play advancement. CI green on 3 OSes.
- **Phase 2.0 (pre-work)**: runs/team-game explained (pitcher batting vs universal DH +0.22, extra-inning runner +0.04; not an engine bug) and gated at ±3% against a DH-equivalent target (sim 4.697 vs 4.699). `GameParams.extra_inning_runner` rule flag added (calibration uses 2015-2019 rules). Walk-off no longer credits runs past the winning run (home runs excepted). Ghost-runner guard test added.
- **Phase 2 (built)**: `season` crate: 162-game schedule for 30 clubs (divisions, interleague rotation, home/away balanced, days), play loop with rayon-parallel days and per-game streams (game id = season<<20 | game), standings, 12-team playoffs with series, summaries; cli `season`, `box [--pbp]`, `player`, `bench`; CI runs `bench`. Gate (`cargo test --release -p season -- --ignored season_gate --nocapture`): 1,000 seasons in 1.6 s on 10 threads, identical digests on 1 thread and all threads; league AVG .2556 vs .2539, K% 21.10 vs 21.14, HR/PA 3.207 vs 3.204, ERA 4.56, team-season spreads inside bands. Two gate-definition fixes before any pass: HR target changed from HR/game to HR/PA (per-game compared different PA counts); earned runs now use virtual outs after an error (ERA was 4.66, ER/R 0.98).
- **Phase 1 tidy-up**: four small commits closing the remaining review gaps (see checklist).
