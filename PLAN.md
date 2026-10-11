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

- [ ] Schedule generator, standings, playoffs
- [ ] rayon parallel days with per-game RNG streams (results independent of thread count)
- [ ] Speed benchmarks in CI
- [ ] Thin cli viewer: standings, box scores, player pages
- [ ] Gate run + reviewer + log entry

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

- Calibration gate is close to tautological for K/BB/HR (rosters are centred on the target). Run environment (4.70 runs/team/game vs ~4.5 real; ghost runner and universal DH explain part) is printed but not gated. Revisit in Phase 2 when 1,000-season distributions are gated.
- Errors advance only the batter (and forced runners), not other runners.
- Walk-off: runs after the winning run can still be credited within the same PA.
- Ghost runner could in theory still be on base when his turn comes up (not reproduced).
- cli: Phase 1 viewer is `cli game <seed> --pbp` (no `sim` prefix).
- `ai` views: `of(league, team)` constructors accept any team id; scoping depends on the caller.
- `League::from_events` leaves `commands_applied` at 0 (events carry no commands).
- Role/team binding covers only commands that name teams; commands that name a game or player will need their team resolved from state in `league::permissions::target_teams`.

## Log

- **Phase 0**: workspace, deterministic sim-core, command/event league, role checks. CI green on ubuntu, macOS, Windows; golden values identical on all three.
- **Phase 1**: log5 engine, baserunning, fielding, relief, box score/play-by-play; Retrosheet 2015-2019 aggregate tool; calibration crate. Gate PASS on 100k games: K +0.58%, BB -0.44%, HR/PA -0.42% (all within ±3%). Fixed a parser bug (force outs counted batter out) found by sanity check; reviewer found no blockers; fixed bases-loaded double play advancement. CI green on 3 OSes.
- **Phase 1 tidy-up**: four small commits closing the remaining review gaps (see checklist).
