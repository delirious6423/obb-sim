# Agent handoff: baseball-sim (written 2026-10-12)

Single entry point for an agent taking over. Read this, then `CLAUDE.md` and `AGENTS.md` (rules), then `PLAN.md` (HANDOFF section and Phase 7b), `ISSUES.md` (the two failing gates in detail) and `OOTP_BENCHMARK.md` (feature gaps). Do not duplicate their content here; this file only orders them and adds what they lack.

## 1. What this is
A closed-source, OOTP-style baseball management sim in Rust, fictional players and teams only. Goal: an open, faster, better OOTP 27 with agent-native connectors. Four roles (commissioner, owner, GM, field manager), each human or AI. Online multiplayer is designed in but is NOT to be started yet (owner order: fix everything wrong in the local game first).

## 2. Where we are
- Phases 0-7 built; `v0.1` tagged; Phase 7b (base-game completeness) in progress; Phase 8 (online) not started.
- `main` is pushed and tested at unit level (`cargo test --workspace`: 212 passed, 0 failed, 23 ignored long gates). CI was not checked after the last pushes; check it first (`gh run list`), per job on ubuntu, macos, windows.
- Long gates were last run per worker, not all together on the final `main`. First task of any new session: run them all once on `main` and record the numbers (commands in `PLAN.md` HANDOFF, "Gate commands").

## 3. The two failing gates (your main job, if you want it)
Full attempt logs, numbers and open questions are in `ISSUES.md` sections 1 and 2. Do not repeat the listed attempts. Summary of what is known:

**Hoarder gate** (`exploit_bots_gain_no_lasting_edge`): the overspending bot beats the reference by +0.026..+0.038 win% (limit: z < 2 and <= 0.015). Debt was proven NOT to be the cause. The sim prices a win at about $4.6M; real MLB free agents cost about $9-10M per WAR. Untested fixes:
1. Branch `worktree-agent-a4ba1b1810eb11ab0`, commit f155ad9 ("free agents at real $/WAR; revenue and costs rebalanced to MLB"): merge it into a scratch branch and run `cargo test --release -p gm --test ladder mechanism -- --ignored --nocapture` (about 80 s), then the gate. Its uncommitted-then-WIP inflation work (39057f9) is unfinished and unverified.
2. The reference GM leaves about 23M of its budget unspent (ReferenceGm in `crates/gm/src/bots.rs`); a reference that spends sensibly narrows the gap.
3. Check how winning feeds revenue (hype to market revenue in `settle()`, `crates/world/src/decide.rs`) and diminishing returns of roster quality on wins.
4. A definition question for the OWNER, not for you to decide: is a win edge bought at full price an "exploit"? Do not rewrite the test without the owner's yes.

**Draft gate** (`draft_gate`): picks 1-10 mean career WAR 13.4-14.5 (band 9.5-12.8), bust .67-.69 (band .60-.65), picks 31-90 reach and WAR above band. The sourced bands come from studies of classes 5-15 years after the draft; the gate scores complete careers (measured in the study window the sim is in band for WAR). The sim also lacked talent past pick 300 and uses ~28 players per club-year vs ~47 in MLB. Untested/unverified:
1. Branch `worktree-agent-a48d2592c901712db` (5 commits: era pitch limits, playing-time customs, weekly rosters, short IL stints, deeper talent pipeline, agent control of playing time, role preset, usage dev aids). Never gated. Merge to a scratch branch, run stability, calibration, manager gates and `draft_gate` plus the study-window dev aid `crates/world/tests/draft_window_dev.rs`.
2. Owner decision pending: score the gate over the study window, build the pipeline, or both.

## 4. Owner rules that bind you
(Copied in `PLAN.md` HANDOFF "Owner rules"; summary.) Never change a gate threshold, golden or test to pass without the owner's explicit yes. Anchor every rate to real MLB statistics and cite the URL. Strategy is fluid: trigger/rule-based usage, defined roles only as optional presets, flexible rotation size, excellent defaults. No bailouts: debt freezes signing and pays down from profit. Every difficulty trades; difficulty changes decisions and information, never the dice. Add inflation. The sim must not get slower (`cargo run --release -p cli -- bench 20`, a season about 0.13 s). Flag repeated failures early and ask for help.

## 5. Practicalities
- Needs `data/internal/params.json` for ignored gates: `mkdir -p data/internal && cp data/params.json data/internal/params.json` (identical files; `data/internal/` is git-ignored).
- Hard rules: determinism (BTreeMap/IndexMap only, `libm`, one RNG stream per game), every change is a command producing events, pure `sim-core`, role-filtered views, `#[serde(default)]` on new saved fields (old saves must load), no hard payroll cap, log every dependency in `THIRD_PARTY.md`.
- Heavy gate runs take 2-5 minutes each and slow each other: at most about 3 at once; iterate on 1-2 seeds, run the 5-seed gate once at the end.
- Git: small commits, push, stage files by name, never commit `data/internal/`. Do not leave leftover `until ...; sleep` wait loops that search for their own command line.
- Worktree branches `worktree-agent-*` on origin: most are merged or superseded (listed in `PLAN.md` HANDOFF). Delete them only after `git branch --merged main` confirms.

## 6. Next steps in order
1. Check CI per job; run all long gates on `main`; record results in `PLAN.md`.
2. Discuss the two failing gates with the owner (questions in `ISSUES.md`); only then attempt fixes.
3. Phase 7b batches 1-4 (`PLAN.md`): inflation + repricing + luxury tax + richer gate model; service time/options/waivers/40-man/arbitration; injuries, fatigue, platoons, park factors, trigger-based usage with UI rule list and JSON; deeper trade AI, scouting budget, calendar events, command-log saves.
4. Combined gate pass, independent reviewer, CI green on three OSes, owner playtest. Only then Phase 8.
