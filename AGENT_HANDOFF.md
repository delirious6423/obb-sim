# Agent handoff: obb-sim (written 2026-10-12)

Single entry point for an agent taking over. Read this, then `CLAUDE.md` and `AGENTS.md` (rules), then `PLAN.md` (HANDOFF section and Phase 7b), `ISSUES.md` (the two failing gates in detail) and `OOTP_BENCHMARK.md` (feature gaps). Do not duplicate their content here; this file only orders them and adds what they lack.

## 1. What this is
An open-source (Apache-2.0), OOTP-style baseball management sim in Rust, fictional players and teams only. Goal: an open, faster, better OOTP 27 with agent-native connectors. Four roles (commissioner, owner, GM, field manager), each human or AI. Online multiplayer is designed in but is NOT to be started yet (owner order: fix everything wrong in the local game first).

## 2. Where we are

**2026-10-13 documentation update:** original code remains at `e232181`; no candidate implementation was applied. Read `ISSUES.md` section 6 before continuing. It records the independent review, completed attempts, verified candidate results, performance regression, exact local commit/bundle locations and owner decisions. Candidate results are not current-main results; the candidate is held. This is a documentation-only update.

- Phases 0-7 built; `v0.1` tagged; Phase 7b (base-game completeness) in progress; Phase 8 (online) not started.
- `main` is pushed and tested at unit level (`cargo test --workspace` on 2026-10-12: 212 passed, 0 failed, 23 ignored long gates). CI was not checked after the last pushes (last known green: `ci.yml` on 3fb8072, `release.yml` on 2487a46 and the `v0.1` tag); check it first (`gh run list`), per job on ubuntu, macos, windows.
- Main's historical long-gate coverage remains incomplete. The follow-up ran the full required set on a separate candidate with dedicated build outputs; it did not establish an overall pass. Reuse those recorded results when the source is unchanged. Re-run appropriate gates after new changes or to answer a specific unresolved baseline question; do not confuse the candidate with main.

## 3. The two failing gates

`ISSUES.md` sections 1-2 preserve the historical diagnoses and attempts; section 6 supersedes their unsupported source claims and records the completed follow-up. Do not repeat those experiments.

- **Hoarder:** the local price/accounting candidate passes the unchanged exploit gate, but the combined candidate is held because draft still fails and offseason performance regresses. Neither the large economy branch nor unfinished inflation was merged. Debt-only experiments do not prove that debt has zero causal effect.
- **Draft:** two isolated pipeline/round-count experiments failed and were stopped; both were reverified with separate build directories. Neither is adopted. Source cohorts, cutoffs, denominators and bust normalization must be reconciled before any approved definition or band change.
- **Review:** save validation, calibration identity, lender-limit enforcement, invalid-event atomicity and lower-minors trade observations have open findings. Shared roster-release cash is fixed only in the unmerged candidate.
- Exact file/line findings, predictions versus measurements, test-edit inventory, source citations, candidate provenance and all owner asks live in `ISSUES.md` section 6.

## 4. Owner rules that bind you
(The full list is `PLAN.md` HANDOFF "Owner rules"; this is the summary.) Never change a gate threshold, golden or test to pass without the owner's explicit yes. Anchor every rate to real MLB statistics and cite the URL. Strategy is fluid: trigger/rule-based usage, defined roles only as optional presets, flexible rotation size, excellent defaults. No bailouts: debt freezes signing and pays down from profit. Every difficulty trades; difficulty changes decisions and information, never the dice. Add inflation. The sim must not get slower (`cargo run --release -p cli -- bench 20`, a season about 0.13 s). Flag repeated failures early and ask for help. Test edits made while removing the credit line that the owner has not yet approved one by one are listed in `ISSUES.md` section 5; do not build on them silently.

## 5. Practicalities
- Needs `data/internal/params.json` for ignored gates: `mkdir -p data/internal && cp data/params.json data/internal/params.json` (identical files; `data/internal/` is git-ignored).
- Hard rules: determinism (BTreeMap/IndexMap only, `libm`, one RNG stream per game), every change is a command producing events, pure `sim-core`, role-filtered views, `#[serde(default)]` on new saved fields (old saves must load), no hard payroll cap, log every dependency in `THIRD_PARTY.md`.
- Heavy gate runs take 2-5 minutes each and slow each other: at most about 3 at once; iterate on 1-2 seeds, run the 5-seed gate once at the end.
- Git: small commits, push, stage files by name, never commit `data/internal/`. Do not leave leftover `until ...; sleep` wait loops that search for their own command line.
- Branches on origin besides `main`: the two stopped WIP branches listed in `PLAN.md` and `worktree-agent-a0eb0a354888cdb4a` (superseded; its one commit is in `main`). Details in `PLAN.md` HANDOFF, Branches. Worktrees under `.claude/worktrees/` are git-ignored.

## 6. Next steps in order
1. Read `ISSUES.md` section 6 and resolve its owner decisions before a third economy or draft approach. Keep the held candidate and the large WIP branches separate from main.
2. Use the recorded candidate commit, bundle and evidence locations if follow-up work is authorized. Keep a distinct build-output directory for each source variant; the earlier shared-cache results are not acceptance evidence.
3. Address the open correctness findings and the approved draft/performance follow-up, then continue Phase 7b (`PLAN.md`). Re-run checks for the actual code being proposed; no gate or band may be weakened to pass.
4. Require a combined gate pass, no speed regression, independent review, verified CI jobs on all three OSes and an owner playtest before Phase 8. Remote CI remains unverified; check it when access is available.
