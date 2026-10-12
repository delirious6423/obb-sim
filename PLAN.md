# PLAN.md

This file is memory. A new session starts at `AGENT_HANDOFF.md`, which orders the documents; then read the HANDOFF section below and continue at the first unticked item. Facts live in one place: the failing gates in `ISSUES.md`, the feature gaps in `OOTP_BENCHMARK.md`, the rules in `CLAUDE.md` and `AGENTS.md`. Sections marked SUPERSEDED are kept as history; do not act on them.

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

Otherwise decide, note it in the log, keep going. ~~**Stop after phase 4b and summarize.** Phases 5+ (desktop UI, saves, alpha, owner and commissioner roles, online) need user decisions first.~~ (SUPERSEDED 2026-10-11: the owner handed over the full roadmap; Phases 5-7 are built. The current stop rule is in HANDOFF: no Phase 8 work until Phase 7b is done.)

## HANDOFF (written 2026-10-12; read this first on a new day or machine)

**Documentation follow-up (2026-10-13; code unmerged):** original code remains at `af61282`. The independent review, source corrections, bounded experiments, final candidate gate results, measured slowdown and owner decisions are recorded in `ISSUES.md` section 6. Those results belong to the separate local `codex/review-calibration-gates` candidate, not current main. Hold that candidate: hoarder passes, draft fails and the no-slowdown requirement is unmet. No source, test, golden or gate band is changed by this documentation update. Historical claims below about 54 MLB trades/year, uniform study windows and the lender clamp are superseded by section 6.

**State in one paragraph.** Phases 0-7 are built; `v0.1` is tagged. The owner ordered: fix everything wrong in the local game BEFORE any online work (Phase 8 NOT started, do not start it). We are in Phase 7b (see below). Main holds merges that were not yet on CI at the time of writing (trading overhaul, draft/stability calibration, debt rule, save export/import, known-gaps fixes); two stopped work branches hold unmerged, unverified work (see Branches). Two gates fail and need a discussion with the owner before more attempts (see Open issues).

**Resume on a new machine**
1. `git clone` the private repo (`delirious6423/baseball-sim`), `git fetch --all` (WIP branches are pushed).
2. Ignored gates need git-ignored data: `mkdir -p data/internal && cp data/params.json data/internal/params.json` (the two files are identical).
3. Rust stable; `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`. Long gates (release, ignored): see Gate commands.
4. Read this section, `CLAUDE.md`, `AGENTS.md`, `OOTP_BENCHMARK.md`. Memory notes for the assistant live in `~/.claude/projects/.../memory` on the old machine only; the rules they hold are copied under Owner rules below.

**Owner rules (binding; copied from the assistant's memory)**
- Whenever in doubt about a rate/count/percentage, anchor to real MLB statistics (look them up, cite URL in PLAN.md).
- Never change a gate threshold, golden or test to make it pass. A band may change only with a cited source and the owner's yes.
- Reference game is OOTP 27; we build an open, faster, better one with agent-native connectors. See `OOTP_BENCHMARK.md` (gaps list).
- Strategy is a fluid spectrum: rule/trigger-based pitcher and fielder usage; defined roles (closer, 5-man rotation, set order) stay as optional presets of the same engine; rotation size flexible (4-8 starters); default auto-management must be excellent; a person can override any single rule.
- Debt: no bailout. In debt (cash < 0): no signing except minimum-salary roster fills, no payroll-adding trades, held to the payroll floor; dividends are judged on cash after the year's profit. Debts are repaid by the club's own profit (roles-gate criterion: each debt spell ends within 10 seasons). No credit line.
- All difficulties trade; difficulty changes decision quality and information, never the dice; easy AIs giving assets away is part of the game.
- Add an inflation mechanic (league salary/price scale, like OOTP's idea) so money stays meaningful over long runs.
- The sim and UI must not get slower (bench before/after: `cargo run --release -p cli -- bench 20`; a season ~0.13 s).
- Flag consistently failing items early and ask for help; list owner questions in a dedicated Asks section at the end of a report.
- Subagents: Opus for design/calibration problems; Sonnet/Haiku for mechanical work; workers must be given the full history of what was already tried; max about 3 heavy gate processes at once on one machine; iterate on 1-2 seeds, full 5-seed gates once at the end.

**Branches (remote inventory from 2026-10-12; local review candidate noted separately)**
- `main`: merged and tested at unit level (fmt, clippy, workspace tests green at 47c6f73 per test-runner; long gates NOT re-run on the combined state yet). Includes: known-gaps fixes; trading overhaul (`crates/gm/src/trade.rs`: packages up to 3-for-3, salary dumps, contender/rebuilder logic; 49-59 accepted trades/season, real MLB about 54); draft/stability calibration (rest days, 4-man bench, pool spread 1.0, founding era, roster-cut fix); debt rule; `.bsave` export/import (Saves screen; README/GUIDE explain); AGENTS.md/docs.
- `worktree-agent-a4ba1b1810eb11ab0` (STOPPED, unverified; on origin): economy batch 1: commit f155ad9 'free agents at real $/WAR; revenue and costs rebalanced to MLB' plus a WIP commit 39057f9 (inflation commands and tests, unfinished). Do not merge without the gate runs in `ISSUES.md` section 1.
- `worktree-agent-a48d2592c901712db` (STOPPED, unverified; on origin): usage/fatigue/talent pipeline: commits d472b46, 431a90a, c60342b, 12a6edf, 5196ce4 (era pitch limits, playing-time customs, weekly rosters, short IL stints, deeper talent pipeline, agent control of playing-time setup, role preset, MLB usage dev aids). Never gated. See `ISSUES.md` section 2.
- `worktree-agent-a0eb0a354888cdb4a` (on origin): one commit, a6ae8b5, the draft dev aid, already in `main` as 40f862b. Superseded; safe to delete.
- All other worker branches were deleted from origin after their merges (checked 2026-10-12 with `git branch -r`).
- `codex/review-calibration-gates` (local clone only, not pushed; HOLD): review candidate at `a6e1665`. This branch is not applied to the original checkout. Commits, checkout location, bundle and evidence are in `ISSUES.md` section 6.

**Open issues (consistently failing; discuss with the owner before more attempts).** The attempt logs, numbers, anchors and owner questions are in `ISSUES.md` (one source of truth; do not copy them here).
1. **Hoarder gate** (`exploit_bots_gain_no_lasting_edge`, ladder): main retains the historical failure. The unmerged price/accounting candidate passes this gate but is held for draft and performance failures. Debt-only interventions were insufficient, not proof of zero causal effect. See `ISSUES.md` sections 1 and 6; do not repeat the completed attempts.
2. **Draft gate** (`draft_gate`): still fails on the combined candidate after two additional, separately verified draft experiments failed. The sources do not share one observation window, denominator or bust statistic; the gate also includes active careers. See `ISSUES.md` sections 2 and 6. Definition and band changes still require explicit approval.
3. Worker process: attempts failed partly because workers started cold; give full history. `ISSUES.md` section 3.

**Other known gaps.** Older sections and log entries below mention a credit line (`CREDIT`, 25M), rescues and receivership; all three were removed by the debt rule (Phase 7b) and such text is marked SUPERSEDED where it is not a log entry. The Known gaps list at the bottom stays valid unless marked fixed. The roles/owner/commissioner screens, save export/import and the trade UI have not been played by a person.

**Gate commands** (release, ignored; need data/internal/params.json): roles `cargo test --release -p connect --test roles_gate -- --ignored --nocapture`; manager `-p world --test manager_gate`; stability `-p world --test world_gate stability_gate`; draft `-p world --test world_gate draft_gate`; ladder `-p gm --test ladder -- --ignored` (ai_beats_reference, presets_rank_in_order, exploit_bots_gain_no_lasting_edge); trade `-p gm --test trade_gate`; season `-p season -- --ignored season_gate`; calibration `cargo test --release -- --ignored calibration`. Dev aids (ignored, print only, for diagnosing a gate): `-p world --test draft_dev`, `--test draft_window_dev`, `--test league_dev`; `-p gm --test trade_volume`, `--test ladder mechanism`; `-p connect --test debt_report`. Historical main results (per worker, not all on one commit; candidate results are in `ISSUES.md` section 6): roles PASS, manager PASS (+0.0078 z 5.2), stability PASS, trade PASS, ladder ranking PASS, hoarder FAIL, draft FAIL.

**Next steps after the discussion:** decide issues 1 and 2; merge or discard the two stopped branches (re-verify every gate); finish Phase 7b batches 1-4 (see Phase 7b); combined full-gate pass; independent reviewer; push; CI per job (ubuntu, macos, windows); then ask the owner to playtest; only then Phase 8.

## Phase 0: workspace (done)

- [x] Six-crate workspace, CI on Linux/macOS/Windows, seeded RNG + golden tests
- [x] Commands, events, log, snapshots, replay tests; role traits and checks
- [x] Review fixes: role check in league, Actor (role + team), validated load, fallible apply

## Phase 1: single game (done)

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
- [x] Gate run + reviewer subagent + log entry (see Log, Phase 1)

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
- [x] Stability gate passes on all 5 seeds (era shift, see Log). **FAILING GATE: `draft_gate`; current numbers and attempts in `ISSUES.md` section 2** (the Log entries below hold the older numbers).

## Phase 4a: manager AI

Gate: manager AI beats a baseline manager over a fixed number of seasons; a stub external agent can fill all four roles.

- [x] Agent connectors for all four roles with typed observations and actions (`connect` crate)
- [x] Win-probability and run-expectancy tables built from the sim (`tactics::Tables`)
- [x] Manager AI, decision logs, stub external-agent adapter
- [x] Gate run + reviewer + log entry

## Phase 4b: GM AI

Gate: AI beats baseline agents at a stated confidence level; presets rank in order by win rate against the same reference bot; exploit bots gain no lasting edge over 50 seasons.

- [x] Surplus-value GM model; trade, free-agency and draft AI with personalities
- [x] Difficulty profiles and presets: Rookie, Veteran, All-Star, Hall of Fame (information and decision quality only, never the dice)
- [x] Evaluation ladder with listed exploit bots
- [x] Gate run + reviewer + log entry (exploit-bot check passed after the close-out fixes, see Log; the Phase 7 economy broke the hoarder check again: `ISSUES.md` section 1)
- [ ] Final summary to the user (superseded: the user handed over a full roadmap; Phase 5 next)

## Phase 5: UI and saves (done 2026-10-11)

Gate: a headless scripted test plays whole seasons through the public protocol only, saves, reloads and gets identical results; CI speed benchmarks with a numeric target set after profiling a full season.

Design (decided, 2026-10-11)
- `host` crate = the protocol (`host::api`: serde `Request`/`Response`, role-filtered views) plus the in-process league host (`Host`) and a threaded client (`LocalClient`: submit, poll, progress, cancel; simulation on a rayon pool of cores-1). The UI depends on `host::api` and `GameClient` only; it never sees `World`. Online play will put the same types on a network.
- The person is general manager of one club (and sets that club's tactics); owner, commissioner and field-manager seats and the other 29 GMs are built-in. Opponent GMs are `SurplusGm` at the chosen difficulty preset or custom levers; their clubs use the table manager.
- The season is played in resumable day ranges (`Command::PlayDays`, `Event::DaysPlayed`): the same result however the days are split (tested); rosters are chosen per segment so lineup changes take effect at once. Game results are kept until players progress; box scores are replayed from the same dice (None if rosters were changed by hand since).
- Saves: `World::snapshot` / `restore` (validated) as JSON inside `storage` (SQLite, one file per slot, manifest sidecar, SHA-256, `.bak`, rotating autosaves, versioned). Events and commands are serialisable (`float_roundtrip` on, so floats survive). No command log is stored beyond the snapshot yet (a save is a snapshot).
- `data/params.json` (league-wide aggregates, no individual data) is committed and embedded so a clean checkout can run; see THIRD_PARTY.md.

- [x] Serde for events/commands/state; `World::snapshot`/`restore`; day-level season play; box score replay (world)
- [x] `storage` crate (21 tests)
- [x] `host` crate: protocol, session, views, threaded client; headless gate test (`crates/host/tests/headless.rs`) PASSES
- [x] egui UI (`ui` crate, bin `game`): new game with difficulty presets and custom levers, home with sim bar (progress, cancel), standings, schedule and box score, roster (sortable), player page, depth chart and tactics, free agents, trades, draft, finances, saves (save, quick save, load, delete), settings, credits. Compiles and starts; NOT yet played through by a person (see Known gaps)
- [x] CI speed gate: `cargo run --release -p host --example bench -- --check`. Profiled on a development laptop (Apple Silicon, 3 runs): found league 0.70 s, 30 single-day requests 0.11 s, full 2430-game season 0.13 s, offseason (finish, draft, free agency, next season) 0.49 s. Targets about 6x that for shared CI runners: 4.0 / 2.0 / 1.5 / 3.0 s.
- [x] Independent reviewers (backend, UI, Phase 6 additions): findings fixed or logged under Known gaps. CI green per job (ubuntu, macos, windows) on 8e4d820.

## Phase 6: alpha (done 2026-10-11)

- [x] README.md, GUIDE.md (player guide), `scripts/build.sh` (clean release build, self-test, `dist/`), `game --self-test`
- [x] Roster-editing tool for the fictional data: `cargo run --release -p cli -- roster export|import` (renames clubs and players in a save; writes a new slot, 9 tests)
- [x] THIRD_PARTY audit: `python3 tools/licenses.py` (455 crates, all with a permissive option); font notices in the Credits screen
- [x] `.github/workflows/release.yml`: clean build + self-test on all three systems (green by hand on db428f5, and on the tag)
- [x] Tag v0.1

## STOP: summary for the playtest (written 2026-10-11; SUPERSEDED)

Historical: the v0.1 playtest brief, kept as written. Its numbers (trade counts, the credit line in the hoarder note, draft WAR) are as of v0.1; the current state is in HANDOFF and `ISSUES.md`.

**Run it**

```bash
cargo run --release -p ui
```

(Or `scripts/build.sh` for a checked build in `dist/`.) Saves are in `~/.baseball-sim/saves`.

**What to try**
1. New game: pick a club and difficulty (try Rookie, then Hall of Fame, or Custom levers). Founding takes about a second.
2. Home: advance a day, a week, to the end of the season. Press Cancel in the middle of a long step.
3. Depth chart: move players between levels, Auto-arrange, tick Smart tactics.
4. After the season: Finish season, then the Draft (draft or pass), then Trades and Free agents, then Start next season.
5. Saves: save, quick-save, quit, load. Try the roster tool from GUIDE.md.
6. Look for: screens that stay on a spinner, buttons that do nothing, wrong numbers, a roster you cannot make legal.

**What is rough**
- The UI was checked by compile, clippy, a start-up run, a headless self-test and the protocol gate test. No person has played it yet, so layout and feel are untested.
- The schedule shows day numbers, not dates. Box scores exist for the current season only. A save is a snapshot; there is no command log. AI general managers restart fresh after a load, so play after a load can differ from an uninterrupted game.
- Draft class mean career WAR is about twice the sourced range (see FAILING GATES), so top draftees are too good.
- Trades between AI clubs are rare (3 proposed, 0 accepted in one 5-year run).
- Known-gap list in the section below has the rest (save-store edge cases, case-colliding slot names, no check of the calibration id on load).
- Speed: found league 0.7 s, season 0.13 s, offseason 0.5 s on a laptop; CI gate has loose targets (4.0 / 2.0 / 1.5 / 3.0 s).

**FAILING GATES**
- `draft_gate` (Phase 3): reach rates are in range for picks 1-30 and 31-90, but mean career WAR per draftee is about 2x the sourced ranges in every bucket (picks 1-10 about 23-26 vs 9.5-12.8; 11-30 about 12-13 vs 4.5-8.7; 31-90 about 4.2-4.5 vs 0.8-3.3); bust rates are low; picks 91-300 reach 0.13 vs 0.17-0.35. Two root-cause attempts used. Thresholds not changed. It fails on all five seeds.
- `exploit_bots_gain_no_lasting_edge` (Phase 4b, broken by the Phase 7 economy): the hoarder bot (signs the best free agent at any price up to the budget) gains +0.0265 win% in seasons 26-50 (se 0.0037, z 7.08; limit z < 2 and edge <= 0.015). At v0.1 it lost (-0.027) because it went insolvent (mean cash -233) and a club below the credit line could not sign or release. Now it stays solvent (mean cash +95, payroll 213 vs 189 for the reference, profit about 49 vs 62 a year): extra wins cost about 2M each, and the reference leaves about 23M of its budget unspent. Attempt 1: a rescued club loses contracts down to the payroll floor at settlement (kept; test `a_rescued_club_loses_the_contracts_that_broke_it`; also applying it at StartSeason left rosters illegal, so not done there): edge barely moved. Attempt 2: fixed ticket price (diagnostic only, reverted): edge larger, so gate pricing is not the cause. Not fixed; no threshold changed, no payroll cap added. Likely lever: wins are too cheap relative to what they earn (price per WAR, hype to revenue) or the reference GM under-spends; needs an economy balance pass.
- No other gate is failing: manager edge, stability (5 seeds), roles gate (Phase 7), information, Phase 5 headless gate and the speed gate pass.

**Decisions I made that you may want to change**
- `data/params.json` (6 KB of league-wide aggregates, no individual data) is committed and embedded so a clean checkout can run.
- A save is a world snapshot; no command log is stored.
- Roster editing is an offline tool on saves (names only), not an in-game screen.

Phase 7 (owner and commissioner roles) is built (see its section); Phase 8 (online leagues) is NOT started. The user said "keep going" after the Phase 6 playtest stop.

## Phase 7: owner and commissioner roles (built 2026-10-12; the hoarder gate it broke is open, `ISSUES.md` 1)

Gate: any role human or AI over a 50-season run on 5 seeds, no runaway payrolls or insolvent teams.

Design (decided)
- Economy: each club has a `Stadium` (capacity in thousands of seats, quality multiplier, ticket price). Local revenue = 45% market (as before) + gate. Gate = crowd x 81 home games x price, where crowd = min(demand, capacity) and demand = a market base (the old local-revenue model, so default parks and the $35 reference price keep the old revenue level) x (price/35)^-1.5 x quality. So the owner's best price sits where demand just fills the park. Park upkeep is in `other`; quality wears 10% a year toward 1.0.
- Owner commands (offseason only): `SetBudget` (existing; refused above the lenders' limit `World::budget_limit`), `SetTicketPrice`, `BuildStadium {amount}` (paid in cash; refused if cash would go below zero; +0.05k seats and +1/600 quality per $M), `SetStance {Contend|Balanced|Rebuild}` (directive to the GM).
- Commissioner command (Offseason/Progressed only): `SetRules` within `Rules::check` limits; club budgets are clamped to the new floor/cap in the same event. Smarter commissioner uses it against runaway payrolls and insolvency.
- Old saves load: new fields have serde defaults.

- [x] World: stadium, attendance, gate in `settle`; owner/commissioner commands and events; restore validation; 7 tests (`crates/world/tests/economy.rs`)
- [x] Agents (`connect`): owner agent sets price, builds, stance; GM reads stance; commissioner agent adjusts rules; wired in the driver
- [x] Host + protocol: person may also be owner and/or commissioner; requests, views, advance gating
- [x] UI: owner screen (not yet played by a person), commissioner screen
- [x] Gate (`cargo test --release -p connect --test roles_gate -- --ignored --nocapture`): 50 seasons x 5 seeds x role mixes (AI, scripted human incl. adversarial policies); no runaway payroll, no insolvency
- [x] Re-run older gates: stability PASS, manager edge +0.0083 (z 5.4) PASS after a harness fix (reads the SeasonPlayed event, not events[0]). Ladder hoarder FAILS (see FAILING GATES). Final reviewer round: see Log.

## Phase 7b: base game completeness (owner order, 2026-10-12; NO online work until this is done)

Owner decisions (2026-10-12): no online league until every local-game issue is fixed. Reference game is OOTP 27 (see `OOTP_BENCHMARK.md`, sourced; OOTP 23 manual assumed to carry into 27). Whenever in doubt, anchor rates to real MLB statistics and cite them. Efficiency rule: nothing here may slow the sim or the UI (benchmark `cargo run --release -p cli -- bench 20` and `crates/host/examples/bench.rs` before and after each batch; a full season stays about 0.13 s).
- Debt: no bailout. A club with cash < 0 cannot sign (minimum-salary roster fills only), cannot add payroll in trades, is held to the floor; its dividend is judged on cash after the year's profit (it pays only once profit clears the debt and the 35% cushion); profit pays the debt down. Roles-gate criterion changed (owner-approved) from 'no club below -CREDIT' to 'every debt is repaid by the club's own profit, no deadlock'. CREDIT removed. Old rescue test deleted (owner-approved).
- Trades: every difficulty trades; difficulty changes decision quality only. Easy AIs giving assets away is part of the game (owner: accepted). Real anchor: about 54 trades a season league-wide (FanGraphs 1996-2000, 3.6 per club), 21-25 consequential; deadline clustering.
- Strategy is a fluid spectrum (owner, 2026-10-12): fixed roles stay available as an OPTION (closer, setup man, rotation slots, set batting order = presets of the same rule engine, for players who prefer them) but the rich, default-quality option is fluid trigger-based use. Pitcher usage, lineups, platoons and depth are trigger/rule-based policies ('if leverage, inning, score, runners, batter handedness, fatigue/rest, injury... then use X') with era-realistic defaults; the field manager (human via a rule list, AI/agent via JSON) supplies them. Must be cheap to evaluate so the sim stays fast.
- Rotation size is flexible (4-8 starters, six-man, openers, bullpen games), driven by rest, fatigue and off-days; classic 5-man is a preset. Defaults must be excellent: a human who does not want to micromanage gets the AI manager's best auto setup (lineup, platoons, rotation, bullpen, rest, call-ups), never worse than the classic role preset, and can override any single rule without taking over everything. Measure auto vs classic vs old fixed behaviour in paired seasons.
- Free-agent price: about $9-10M per WAR (FanGraphs $/WAR series: 9.0M 2014-16; 2023 10.2M and 2024 8.9M informal). Payroll-win correlation r about 0.4 in a typical year, weak in 2023. Inflation mechanic (our own design; OOTP's is undocumented): a league-wide salary scale that grows yearly and moves prices, floor, cap, min salary, revenue baselines and ticket reference together.

Order of work
- [ ] Batch 0 (partly done): debt rule and the trading overhaul are merged and in `main`; `draft_gate` re-measured (`ISSUES.md` 2; a definition change is the owner's call); the usage/injury/fatigue/talent-pipeline build sits unverified on branch `worktree-agent-a48d2592c901712db` (HANDOFF, Branches).
- [ ] Batch 1: inflation + free-agent repricing to real $/WAR (fixes the hoarder gate); luxury tax; richer gate model (fan interest, home/away split, playoff gate, media, merchandise).
- [ ] Batch 2: service time, minor-league options, waivers/DFA, 40-man; salary arbitration and extensions; Rule 5; international spending pool.
- [ ] Batch 3: trigger-based usage policies for bullpen, rotation, lineup and platoons (UI rule list + JSON for agents); injury tiers and proneness, fatigue, platoon splits, park factors and weather; personality/morale (later).
- [ ] Batch 4: richer trade AI (counteroffers, trading block, untouchables, draft-pick trading, deadline); scouting budget; calendar events; commands log in saves.
- [ ] Re-run all gates on 5 seeds; independent reviewer; CI green on three platforms; then ask the owner to playtest.

## Draft bands (Phase 3)

Sources (looked up; nothing downloaded into the repo). All count *signed* draftees unless noted; "reached the majors" = at least one major-league appearance or at-bat, measured years after the draft, so the studies are slightly right-censored (late arrivals missing).

- FanGraphs community post "Success Rate of MLB First-Round Draft Picks by Slot" (2000-2010 drafts; https://community.fangraphs.com/success-rate-of-mlb-first-round-draft-picks-by-slot/). Reached the majors per 55 picks: 1-5 48 (87%), 6-10 41 (75%), 11-15 45 (82%), 16-20 43 (78%), 21-25 36 (65%), 26-30 32 (58%); so 1-10 = 81%, 11-30 = 71%. Average bWAR (column not defined as career or per season): 12.8, 9.5, 8.7, 4.9, 6.5, 4.5. Bust = never reached, or under 1.5 bWAR per season (500 AB / 25 G seasons): 60%, 65%, 71%, 85%, 76%, 84%.
- SABR, "The Chances of a Drafted Baseball Player Making the Major Leagues" (1996-2011 drafts through round 20, data to 1 Aug 2016; https://sabr.org/?p=11541). Reached the majors by round (signed, supplemental picks included): R1 66.7%, R2 49.4%, R3 39.7%, R4 35.0%, R5 33.3%, R6-10 about 20% (17.5-24.4% by round). By round only, no pick ranges, no WAR.
- Baseball America, "How Often Do First-Rounders Make The Big Leagues?" (https://baseballamerica.com/?p=800696; years not stated): 81% of signed first-round hitters get an MLB hit and 81.3% of pitchers an MLB strikeout; second round 63-64%. Other BA/BP figures quoted in search results: 73% of first-rounders 1981-2010, 69.0% of first-rounders 1965-2009 (Baseball Prospectus), second round 46.5%.
- Conforti et al., "Major League Draft WARs" (Journal of Sports Analytics 2022; 2005-2015 drafts): average career WAR *of those who reached the majors* R1 6.93, R2 3.30, R3 2.11.

Ranges now in the gate (`crates/world/tests/world_gate.rs`): reach the majors 1-10 [0.75, 0.87], 11-30 [0.58, 0.82], 31-90 (rounds 2-3) [0.40, 0.51], 91-300 (rounds 4-10) [0.17, 0.35]; mean career WAR per draftee 1-10 [9.5, 12.8], 11-30 [4.5, 8.7], 31-90 [0.8, 3.3] (reach rate x Conforti's among-reached means, up to the among-reached means themselves); bust (never reached or under 1.5 WAR per season) 1-10 [0.60, 0.65], 11-30 [0.71, 0.85]. No source was found for 91-300 WAR and bust (kept [0, 3] and [0.75, 1.0] from the first draft) or for 31-90 bust (not gated).

Comparison of definitions: the sim's "reached" is at least one season with a plate appearance or batter faced at the majors level, over signed draftees, measured 15+ years after the draft (no censoring); the sim's WAR is runs above a replacement of 0.028 runs/PA over a volume of 650 PA (batters) / 450 BF (pitchers) per 10 runs; the sim's bust uses the FanGraphs rule on seasons with major-league playing time. The sourced 11-30 range (0.58-0.82, pooled 0.71) did not include the first sim value of 0.84, so the model was off and step 2 applied.

## Known gaps

- (2026-10-11, Phase 5) Review notes not fixed: AI general managers are rebuilt fresh on load, so play after a load can differ from an uninterrupted game; `start_next_season` runs its AI rounds before it can refuse; `LocalStore::list` does not show a slot that only has `.bak` files; two autosaves in the same second can pick the same slot; slot names collide by case on macOS/Windows. The UI has no graphical test (only compile, clippy, a start-up run and the protocol gate). Fixed since: a load or import refuses a save made with other calibration data and notes an engine-version change (`Session::from_save`, `Session::import_save`); the Player screen's Back button returns to the screen it came from.

- (2026-10-12, Phase 7) Old saves get `Stadium::for_market(1.0)`; the Overview label "Play the playoffs..." shows in Progressed/Settling for office-holders; the roles gate uses `run_year_with` with built-in GMs and managers (no host-level run, no human GM or field manager); the owner and commissioner screens were not played by a person; the UI ballpark amount range 0.1..1000 is a guess. Fixed or superseded since: the owner desk shows the lenders' limit (`crates/host/tests/roles.rs`); `clamp_budgets` is called at season close; rescues and receivership are gone (debt rule, Phase 7b), so the stale `attendance_prev` after a rescue and the receivership roster notes no longer apply (`crates/gm/tests/debt_offseason.rs` plays a club in debt through an offseason into a legal season start).

- (2026-10-11) ~~`trading_happens` only prints counts and asserts nothing; with the stronger reference it printed 3 proposed, 0 accepted in one 5-year run.~~ Fixed 2026-10-12: the trading overhaul (`crates/gm/src/trade.rs`) and `crates/gm/tests/trade_gate.rs` gate accepted trades a season at 40-110 (measured 49-59; real MLB about 54) and the value lost by easy presets to hard ones.
- (2026-10-11) Runs/team-game in the world runs at about 5.0 vs the DH-equivalent target 4.70 (+7%, inside the +-10% world band).
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

- Season: no series, travel, off-day or weather structure; the `season` crate itself plays a fixed lineup and rotation (the `world` crate adds injuries, rest days and a 4-man bench on top; no fatigue state); playoffs seeded by wins, run difference, club number (no head-to-head).
- Unearned runs: engine now uses the virtual-outs rule, but non-reached-base errors are not modelled, so ER/R is 0.96 vs about 0.93 real; league ERA 4.56 vs about 4.4 for the DH-equivalent environment.
- Season gate bands for spread (0.6-1.5x of real team-season SD, ERA/win% ranges) were fixed before the first run; team AVG spread runs 1.21x real, K%/HR 0.8x.

- Season gate: ERA band (3.9-4.6) and win%/ERA spread bands are hard-coded, not derived from data; spread bands 0.6-1.5x are loose (a 20% error would pass). Derive and tighten when real ERA/win% distributions are added to the tool. K% check compares to the engine's own input rate (same as the real rate).
- Playoff starters resume at `162 % rotation`, not at the club's actual rotation position; ties are broken by run difference then club number (no head-to-head).
- DH adjustment in the tool uses league rates that include pitchers' own PAs (about 0.2% of runs).

- Phase 3 (world crate): design is `world` crate with strict events (`World::apply`), commands via `World::execute`, driver `run_year`, agents (`GmView` shows only noisy opinions); founding runs a burn-in of offseasons (no games) from a seeded population, with opening cash restored afterwards.
- Aging curve (peak 27, deficit by age, decline from 30), injury rates, amateur pool (first spread 1.3 and offsets 2.2 batters / 2.0 pitchers below league average; now spread 1.0 and offsets 2.5 / 2.4 after the Phase 3 follow-up and the 2026-10-12 draft work, `ISSUES.md` 2), scouting noise (1.5 quality-sd on amateurs) and age-scaled development luck are assumptions tuned for stability, not fitted to data (no age data source approved).
- Minors are roster tiers only (no minor-league games); minor leaguers get the same injury rolls as major leaguers; level equivalencies are not modelled.
- Majors still hold some 17-20 year-olds (about 4% of major leaguers) when a teenager out-scouts the field; BB% runs about +10% over the real league (selection on walk-heavy talent), runs/team-game about +4% (4.90 vs 4.70).
- Free agency is one signing per club per round; releases cost the remaining salary; contracts are salary x years only (no arbitration, no options; trades since Phase 4b).
- Phase 3 follow-up (after the draft bands were grounded): one bounded model attempt (young players' yearly development luck 0.22 -> 0.55 scaled by youth; pool offsets recalibrated to 2.5 batters / 2.4 pitchers; founding burn-in 12 -> 30 years because the league mix was still settling) brought "reach the majors" for picks 1-10, 11-30, 31-90 into or next to the sourced ranges, but mean career WAR stays about 2x the sourced range in every bucket (picks 1-10 about 20-25 vs 9.5-12.8), bust rates are 0.47-0.52 / 0.64-0.69 vs 0.60-0.65 / 0.71-0.85, and picks 91-300 reach 0.13 vs 0.17-0.35. Accepted and logged; `draft_gate` fails on all five seeds.
- Phase 3 stability gate over five seeds: K%, BB%, level of HR% and runs, major-league quality worst-season band and solvency pass everywhere; the decade-over-decade ratio check fails on some seeds (HR/PA ratio 1.096 on seed 1, 0.9496 on seed 2 against 0.95-1.05; batter quality drift 0.176 sd on seed 1 against 0.15). Cause: league mix wanders because nothing anchors it (no feedback from run environment to talent) and talent turnover is slow. Across 20 other seeds the mean decade ratio is 1.00-1.02 for K/BB/HR/runs (no trend) with standard deviations of 2.2% (K), 3.2% (BB), 4.4% (HR), 1.9% (runs), so a trend-free model exceeds a +-5% band on a given metric in roughly 10-25% of seeds. Not changed (thresholds fixed); the band or the statistic (mean ratio across seeds, or a per-seed trend test) needs a decision.
- Phase 3 information and limits: asking salaries and amateur bonus demands now come from a market view that is no better informed than a scout (`scout::public_quality`, `scout::amateur_demand`); `apply` validates events before changing anything and enforces roster limits at season start (13 majors per kind, level capacities, 49 per kind) and a hard 60 per kind at signing; club spending that would take cash below -25M was refused (`CREDIT`; SUPERSEDED by the Phase 7b debt rule: a club may go into debt but is then frozen out of new commitments); bonuses and released contracts show up as `spent` in the next season's books. Not yet done: a commissioner force-pass for a stalled draft.
- Phase 4a limits: the table manager's edge (+0.69 win% per club-season, z 4.5) mixes two sources, steal decisions and relief use; it also switches off the engine's closer-in-save-spot rule outside its own leverage rules, so part of the edge may come from that rather than from the tables. Engine steal success is a flat league rate, not runner-specific, so steal decisions carry no skill information. `Go`/`Hold` skip the engine's attempt draw, so managed and baseline games diverge in the dice stream after the first steal (no common random numbers; adds noise, not bias). `DecisionLog` and the shared tables are `Arc`s shared by world clones (diagnostic only, not in `PartialEq`). Live in-game external field managers are exercised in exhibition games only; season batch play uses the built-in `Tactics` policies. An out-of-phase commissioner action is overruled by the calendar and logged as a violation; garbage/failed replies fall back to harmless defaults.
- Phase 4b ladder caveats (reviewer): the win% gap of a tested club is about 1.25x its true edge (the 24 reference clubs absorb part of it); the standard errors treat seasons as independent although they are autocorrelated, so the stretcher failure (z 2.7) is weak and the hoarder failure (z 9) is robust; preset runs include the warm-up seasons; the preset order is partly caused by `spends_budget` (Rookie never spends leftover budget); the reference was strengthened after the bot results were seen. Review fixes applied: releases of excess players are waived only above the per-kind limit, a trade may not leave a club below 13 majors of a kind, a club may have at most 4 open offers, and a league office fallback signs players for a short club before the season starts (the garbage-agent case).

## Log

- **Phase 7 (2026-10-12)**: stadium, gate and ticket price; owner levers (budget, ticket, ballpark, stance) and commissioner rules (10% a year, once a year); lender budget limit; rescue at settlement now also voids contracts above the payroll floor; built-in owner and commissioner agents; host roles and views; owner and commissioner screens. Roles gate (5 seeds x 6 mixes x 50 seasons) PASS; manager gate +0.0083 (z 5.4); stability PASS; hoarder ladder gate FAILING (see FAILING GATES). A first receivership at StartSeason left illegal rosters and failed the roles gate, so it applies at settlement only.

- **Step 1 close-out (2026-10-11)**:
  - *Exploit bots (1a)*: two root causes found by measurement and fixed in the built-in/reference GM, no thresholds changed. (1) The reference left depth unfilled; injuries pull replacements from Triple-A, so the hoarder (best-now at any price, up to 60 per kind) won on depth. `ReferenceGm` now also upgrades the 24th man of each kind with leftover budget. (2) The market asks the same salary whatever the length, so a long deal for a player still improving is surplus; the stretcher won by forcing 7 years. `Seen::sign_years` (7 years up to age 25, at least 5 at 26-27, else the market's ask) is used by the built-in GM step. The exploit test now uses 5 seeds and season-clustered standard errors (one mean per league run, because seasons within a run are autocorrelated). Final (seasons 26-50, 5 seeds): hoarder -0.027 (z -3.2), tanker -0.155, stretcher -0.008 (z -2.2), trade spammer -0.020. Presets: Rookie -0.096, Veteran -0.048, All-Star +0.017, Hall of Fame +0.036 (HoF vs reference z 10.9; HoF minus Rookie z 24.8). Manager gate still passes (edge +0.0058, z 3.65).
  - *Stability / BB% drift (1c)*: cause: nothing pulled the talent mix back, so K/BB/HR random-walked (+-8% a decade, both directions, no trend) and BB% sat +10% over the real league. Fix: `World.era`, a per-kind talent shift moved by `PlayersProgressed{era}` each offseason by half the major leaguers' mean talent offset, added to every player's talent (the whole league re-centres; no club gets anything). `scout::peak_quality` includes it. `stability_gate` now passes on all 5 seeds (BB% mean 8.6-8.7% vs real 8.46%; every decade ratio inside the bands). Caveat from a second opinion: zero mean z does not guarantee stable event rates in general (nonlinear mapping, playing time); it works here on 5 seeds. Alternative if it ever fails: calibrate offsets against playing-time-weighted expected rates.
  - *Draft bands (1b)*: sourced bands already in "Draft bands". Reach rates now inside the sourced ranges for picks 1-10, 11-30, 31-90 on all seeds. FAILING GATE (kept, numbers): picks 1-10 mean career WAR 23-26.5 vs 9.5-12.8; 11-30 11.8-13.1 vs 4.5-8.7; 31-90 4.2-4.5 vs 0.8-3.3; bust 0.46-0.47 / 0.64-0.68 vs 0.60-0.65 / 0.71-0.85; picks 91-300 reach 0.126-0.132 vs 0.17-0.35. Diagnosis: picks 1-10 reached players play about 10.0 seasons at 2.7-2.9 WAR a season (sim); the published studies give about 14.7 WAR among reached for picks 1-5 (12.8/0.87), so careers are probably too long and/or too productive; the sim's mean WAR for an average club (about 30) is fine, so the WAR scale is not the cause. The one bounded model change was already spent in the Phase 3 follow-up. Not changed further.
  - *Leaks (1d)*: asking prices and bonus demands come from the market view (`scout::public_quality`, `amateur_demand`). New tests (`crates/world/tests/information.rs`): an asking price equals the market view exactly, and neither prices nor demands correlate with true quality better than one club's scouting report does (price 0.70 vs scout 0.91; demand 0.42 vs 0.59).
  - *Gaps (1e)*: roster limits, cash floor (`CREDIT`), event validation in `apply`, and tests for retirement, expiry, release dead money and levels were already in (Phase 3 follow-up). Phase 1 follow-ups confirmed done (runs/team-game gated at +-3%, walk-off, ghost-runner guard test).
  - Verifier review: no determinism or double-count bugs. Fixed: `peak_quality` ignored the era; `era` assigned before the year check in `apply`; contract limit used a literal 7 (now `MAX_YEARS`).
- **State verification (2026-10-11, new session)**: HEAD 7ebd0d3 (Phase 4b), `main` in sync with origin. Repo has 12 crates (ai, calibration, cli, connect, gm, league, protocol, season, sim-core, storage, tactics, world). Phases 0-2 done and gated. Phase 3 built, gate NOT met (`draft_gate` fails on all five seeds, `stability_gate` decade-ratio fails on some seeds). Phase 4a done, gate met. Phase 4b built, exploit-bot gate FAILS (hoarder, stretcher); final summary to user still unticked. `storage` is not started. Checks: `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo test --workspace` 99 passed, 0 failed, 12 ignored (ignored = the slow gates). CI on 7ebd0d3, per job: ubuntu-24.04 success, windows-latest success, macos-latest success (previous five runs also green). DISCREPANCIES vs hand-off: (1) the hand-off text is cut off in section 3 (mid-sentence at "PERFORMANCE: always simulate at plate-appearance"), so sections 4-9, including "section 7" and the stop conditions, were not received; (2) the hand-off lists `ai` view, `league`, etc. as before, but PLAN.md says to stop after phase 4b, and 4b's gate is open, so there is no ticked "next item" beyond decisions the user must make; (3) the working tree has uncommitted `CLAUDE.md` changes and untracked `AGENTS.md` and `docs/` (not mine; left unstaged); (4) the hand-off says the UI crate and server come in phases 5 and 8: neither exists yet, as expected.
- **Phase 0**: workspace, deterministic sim-core, command/event league, role checks. CI green on ubuntu, macOS, Windows; golden values identical on all three.
- **Phase 1**: log5 engine, baserunning, fielding, relief, box score/play-by-play; Retrosheet 2015-2019 aggregate tool; calibration crate. Gate PASS on 100k games: K +0.58%, BB -0.44%, HR/PA -0.42% (all within ±3%). Fixed a parser bug (force outs counted batter out) found by sanity check; reviewer found no blockers; fixed bases-loaded double play advancement. CI green on 3 OSes.
- **Phase 2.0 (pre-work)**: runs/team-game explained (pitcher batting vs universal DH +0.22, extra-inning runner +0.04; not an engine bug) and gated at ±3% against a DH-equivalent target (sim 4.697 vs 4.699). `GameParams.extra_inning_runner` rule flag added (calibration uses 2015-2019 rules). Walk-off no longer credits runs past the winning run (home runs excepted). Ghost-runner guard test added.
- **Phase 2 (built)**: `season` crate: 162-game schedule for 30 clubs (divisions, interleague rotation, home/away balanced, days), play loop with rayon-parallel days and per-game streams (game id = season<<20 | game), standings, 12-team playoffs with series, summaries; cli `season`, `box [--pbp]`, `player`, `bench`; CI runs `bench`. Gate (`cargo test --release -p season -- --ignored season_gate --nocapture`): 1,000 seasons in 1.6 s on 10 threads, identical digests on 1 thread and all threads; league AVG .2556 vs .2539, K% 21.10 vs 21.14, HR/PA 3.207 vs 3.204, ERA 4.56, team-season spreads inside bands. Two gate-definition fixes before any pass: HR target changed from HR/game to HR/PA (per-game compared different PA counts); earned runs now use virtual outs after an error (ERA was 4.66, ER/R 0.98).
- **Phase 1 tidy-up**: four small commits closing the remaining review gaps (see checklist).
- **Phase 3 (built, gate NOT met)**: `world` crate (hidden-talent players, aging, injuries incl. minor leaguers, draft with noisy club boards and signability, free agency, revenue/payroll/budget economy, commands -> events with strict apply, fixed GM and owner agents, burn-in founding). Tests: determinism, event replay equals state, role/phase/club enforcement, roster invariants. Gate (`cargo test --release -p world --test world_gate -- --ignored --nocapture`, 50 seasons): league K/BB/HR/runs inside bands and drift-free, major-league quality drift under 0.15 sd, no insolvent club (lowest cash > 0) all PASS. FAIL: picks 11-30 reach the majors 0.84 vs band 0.40-0.80 (other draft metrics pass: picks 1-10 0.94/WAR 26/bust 0.10; 31-90 0.54/WAR 4.0; 91-300 0.12/WAR 0.3/bust 0.92). Fix attempt 1: amateur scouting noise 0.8/0.7 -> 1.5/1.5 and age-scaled development luck (before: top-10 reach 0.99, WAR 37). Fix attempt 2: opening cash restored after burn-in (cause of negative cash on other seeds) and injuries for minor leaguers (no effect on the failing metric). Gate thresholds were not changed (one test bug fixed: runs/team-game was divided by 2 twice). Stopped per the directive; needs a decision.
- **Phase 3 follow-up**: sourced draft bands (see "Draft bands"), gates split into `stability_gate` and `draft_gate` over five seeds, information leaks closed, apply-side validation, roster limits, cash floor, new tests (retirement, expiry, release dead money, levels, draft bonus refusal, event validation). Both gates still fail; see Known gaps for the numbers.
- **Phase 4a**: sim-core `Manager` hooks (steal, relief, observe; `Baseline` identical to before, goldens unchanged); `tactics` crate (run-expectancy and win-probability tables counted from simulated games, leverage, table manager with aggregated decision log); `world` gets `Tactics`, `SetTactics` (field-manager role) and `TacticsSet`; `connect` crate with typed observations/actions for all four roles, built-in agents, JSON `ExternalAgent` with a reference stub transport, seat driver, live-manager adapter. Gate (`cargo test --release -p world --test manager_gate -- --ignored --nocapture`): paired seasons from identical rosters, 5 seeds x 8 years x 30 clubs, tables edge +0.0069 win% per club-season (se 0.0016, z 4.47; threshold z>=2); clubs facing tables without them -0.0069. Stub gate (tests/connect.rs): built-in seats and the JSON stub reproduce the `world::driver` event log exactly, runs are deterministic and replay to the same world, broken/hostile agents cannot stop the league. Independent reviewer: no blockers; fixed an out-of-phase commissioner aborting the year (now overruled by the calendar, with a test); other notes under Known gaps.
- **Phase 4b (built, gate NOT fully met)**: world gains trades (`ProposeTrade`/`RespondTrade` -> `TradeProposed`/`TradeAnswered`, offers lapse at season start, validated in apply, tests in `tests/trades.rs`) and league-office-safe releases (a club over its roster limits may always release the excess). `connect` gains the Trade stage and trade actions; `GmObs` gains contract terms and a trade market view. `gm` crate: `SurplusGm` (multi-year surplus value with ageing projection, market-implied quality pooled with the club's own reports, contract-length choice, draft/free-agency/roster/trade logic), `Personality` (Balanced, Builder, WinNow, BargainHunter), presets Rookie/Veteran/AllStar/HallOfFame (extra scouting error, weight on the market view, horizon, ageing projection, trade mode, spending leftover budget; the dice are untouched), exploit bots (Hoarder, Tanker, Stretcher, TradeSpammer), and `ReferenceGm` (built-in GM + plain trade answering + spending idle budget on one-year upgrades). Ladder (`cargo test --release -p gm --test ladder -- --ignored --nocapture`; six tested clubs vs 24 reference clubs, win% gap per season): Hall of Fame +0.053 (z 10.3) PASS; presets rank Rookie -0.086 < Veteran -0.031 < All-Star +0.041 < Hall of Fame +0.053, HoF minus Rookie z 21 PASS; trading happens (1-5 accepted trades per 5 years in six clubs) PASS. **FAIL**: exploit bots over seasons 26-50 (3 seeds): hoarder +0.025 (z 9.0), stretcher +0.0073 (z 2.7); tanker -0.157 and trade spammer -0.0003 are fine. Fix attempts: (1) price multi-year contracts by the expected trajectory (`asking_salary_for`) -- made it worse (hoarder +0.027, stretcher +0.036, preset order All-Star/HoF flipped), reverted; (2) make the reference spend idle budget on one-year upgrades -- stretcher fell from +0.014 to +0.007, hoarder stayed at +0.025. Cause found by measurement (dev test `mechanism`, seed 1, seasons 11-25): reference clubs use 155M of a 214M budget and sit on 187M cash; the hoarder uses all of it (216M) and stretcher keeps 195M under contract. The reference (like the Phase 3 fixed GM) leaves budget idle, so any bot that spends it wins. Gate thresholds were not changed. Needs a decision: strengthen the reference/fixed GM further (e.g. fill the roster and depth with the budget, re-sign stars), treat spending the budget as intended play and exclude hoarder/stretcher from the exploit list, or cap payroll growth by rule.
