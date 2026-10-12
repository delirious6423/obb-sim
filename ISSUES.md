# Open issues for discussion (written 2026-10-12)

Sections 1-5 are the historical handoff. Section 6 records the independent review and bounded experiments that followed; its source corrections supersede the historical diagnoses. Predictions are labeled separately from measured results. No gate threshold or golden was changed in this follow-up.

## 1. Hoarder gate (`cargo test --release -p gm --test ladder exploit_bots_gain_no_lasting_edge -- --ignored --nocapture`)

**What the gate says.** Six clubs run by a deliberately lopsided "exploit" bot play against 24 reference clubs for 50 seasons on 5 seeds. In seasons 26-50 no bot may beat the reference by z >= 2 or by more than 0.015 win%. The hoarder signs the best-looking free agent at any asking price until its budget is used up. Tanker (spends the minimum), stretcher (7-year contracts) and trade spammer all pass.

**Current result.** Hoarder +0.026 to +0.038 win% (z 7 to 10), about 6 extra wins a season. At v0.1 it was -0.027, so something in Phase 7 flipped it.

**What changed since v0.1 and why the hoarder lost then (history; the credit line, the rescue and receivership named below were all removed by the debt rule, attempt 5).** At v0.1 the hoarder went broke (mean cash -233M). A club below the credit line (-25M) could not sign or release anyone, so it was frozen with bad contracts and lost. Phase 7 added a rescue (cash reset to 10% of revenue) and a lender limit, and the hoarder stayed afloat and won. Today only the lender limit remains (`World::budget_limit`).

**Attempts (all measured, none passed)**
1. Receivership: a rescued club loses its dearest contracts down to the payroll floor, applied at settlement. Edge barely moved (0.0414 to 0.0392). Kept. Applying it also at season start left illegal rosters and failed the roles gate.
2. Pin ticket prices, to test whether gate pricing was the cause. The edge got bigger (+0.051). Reverted. Not the cause.
3. Rescue carries the deficit (no cash reset). Ladder passes (hoarder -0.066), but the roles gate fails: hundreds of club-seasons below the credit line in the reckless mixes.
4. Rescue resets cash to exactly the credit line. Ladder fails (+0.033).
5. Owner's design: no bailout at all, debt freezes signing, trades must shed salary. Built and merged (this is the current rule; `PLAN.md` Phase 7b, `crates/world/tests/debt.rs`). Ladder still fails, worse (+0.037, z 10.7). Conclusion: debt was never the hoarder's edge.
6. Dead-money idea (released players' salary stays on the payroll): seed 1 edge +0.046 to +0.029. Not committed.
7. Blocking the hoarder's wasteful minimum-salary signings raised its edge to +0.077.

**What the data says.** On seed 1 over seasons 26-50 the hoarder spends about 217M of payroll against 189M for the reference. It is in debt in about 7% of club-seasons and never below -39M. It fully uses a budget the reference leaves about 23M short. The extra 28-30M buys about 6.5 wins, or about $4.6M per win.

**Real anchors.** Free agents cost about $9-10M per WAR (FanGraphs series: 9.0M in 2014-16; informal 10.2M in 2023 and 8.9M in 2024). Payroll explains about 16-20% of the variation in wins in a typical year (r about 0.4), and almost none in 2023. So in real baseball outspending does buy wins, but at about twice our price.

**Plan on the table.** Reprice free agents to real $/WAR and rebalance revenue and costs. Branch `worktree-agent-a4ba1b1810eb11ab0`, commit 710e0b0, plus a WIP commit b07653c (inflation, unfinished). Never tested against the gate.

**Questions for the owner**
- Is the test right? It treats any lasting win edge as an exploit. In real baseball a team that pays more wins more, and that is not an exploit. The alternative is a net-value test: a bot is an exploit only if it gains wins without paying for them. That is a definition change and needs the owner's yes.
- If repricing fixes it, is a bot allowed a small edge up to 0.015 as the gate already says?
- Do we make the reference GM spend its whole budget too (it leaves about 23M unused)? That would shrink the hoarder's edge.

## 2. Draft gate (`cargo test --release -p world --test world_gate draft_gate -- --ignored --nocapture`)

**What the gate says.** For draftees by pick range, five seeds: reach rate (share who play in the majors), mean career WAR, and bust rate (never reach, or under 1.5 WAR a season). Bands are sourced in PLAN.md "Draft bands".

**Current result (after the merged calibration work).**
- Picks 1-10: mean career WAR 13.4-14.5 (band 9.5-12.8), bust 0.67-0.69 (band 0.60-0.65). Fails.
- Picks 11-30: passes except one seed's reach.
- Picks 31-90: reach 0.55-0.57 (band 0.40-0.51), WAR 3.5-3.8 (band 0.8-3.3). Fails.
- Picks 91-300: passes.
- At the start of this session picks 1-10 were 24-26 WAR, about twice the band.

**Attempts**
1. Earlier sessions: development-luck and pool-offset changes (PLAN.md Phase 3 follow-up).
2. Steeper post-peak decline (0.13 to 0.25): no effect on top-pick WAR or career length (10.0 to 9.7 seasons).
3. A persistent per-player growth factor: WAR per reached player rose (28 to 30), top-pick reach fell, net worse. WAR is convex in quality, so more spread raises the mean.
4. Retiring fringe players earlier: no effect on the top buckets, late-pick reach fell. Reverted.
5. Rest days (each regular sits about 1 day in 7) plus a 4-man bench. Batters with 650+ plate appearances fell from 60% to 6%. Kept.
6. Pool spread 1.3 to 1.0 (offsets 2.5/2.4). Picks 1-10 WAR went from 23.8-25.9 to 16.6-17.5, late-pick reach passed. Kept. It broke stability on one seed (pitchers -0.359 against the +-0.35 band).
7. Founding league now carries the burn-in era shift. Fixed the stability regression. Kept.
8. Pool offsets 2.8/2.7: no effect on rounds 2-3, 6+ WAR seasons fell too low. Reverted.
9. Roster cut kept contracts it cannot afford; replacement level set per kind (0.0292 batters, 0.0228 pitchers). Kept.

**Diagnosis**
- The real numbers (Baseball America, FanGraphs, Conforti 2022) come from drafts 5-15 years old when measured, so careers are cut short. Our gate scores complete careers. Measured in the study window, picks 1-10 are 9.6-11.0 WAR (in band) and 31-90 WAR is 2.3-2.6 (in band). Bust 0.64-0.71 and 31-90 reach 0.53-0.54 still miss.
- Structure: the sim has no talent past pick 300 (undrafted players are deleted) and clubs use about 28 players a year against about 47 in MLB (828 pitchers in 2019, 27.6 per team). So top-300 draftees earn about 65% of league WAR against about 48% in MLB.
- League check: 6+ WAR seasons per year were 25-28 (real 10-15), now about 11.

**Partly built, unverified.** Branch `worktree-agent-a48d2592c901712db` (5 commits): real MLB usage benchmarks, era pitch limits, playing-time customs, weekly rosters, short IL stints, a deeper talent pipeline, agent control of playing time, a role preset.

**Questions for the owner**
- Score careers over the study window (definition change, cited to FanGraphs and Conforti), build the missing pipeline and usage fixes, or both? Recommended: both.
- Is it acceptable to merge the unverified usage branch, or hold it until each gate is re-run?

## 3. How the work is being run
- Several attempts failed because each worker started cold and re-measured the same things. Workers should get the full history above.
- Hard calibration and design problems now go to Opus from the start.
- Heavy gate runs overlap on one machine and slow each other. Limit to about 3 at a time; iterate on 1-2 seeds and run all 5 once.
- Question: keep long unattended workers, or shorter steps where the owner sees numbers before the next step?

## 4. Decisions on branches
- Economy branch (repricing, partial inflation): merge, hold, or discard?
- Usage branch: merge, hold, or discard?
- Main is pushed. Its long gates have not been re-run on the combined state.

## 5. Review notes on merged main (verifier, 2026-10-12; the test edits listed here still wait for the owner's yes)
- Fixed: ENGINE_VERSION bumped to "rules 2" (old saves now show the 'results may differ' notice); `.bsave` import now checks the calibration id and runs `World::restore` before writing into the save folder.
- Season-in-progress saves made before rest days/bench keep their recorded games but play the rest under the new rules; old box scores may not replay (they fail gracefully, return None).
- Test edits since the last push that the owner has NOT individually approved (all follow from removing the credit line): world.rs release filter `owed < cash + CREDIT` -> `owed <= cash`; economy.rs ballpark test cash `-CREDIT + 1` -> `4.0`; crates/gm/tests/agent.rs and storage/tests/local_store.rs changed (the first replaced rookies_do_not_trade_and_veterans_only_answer with every_preset_proposes_trades). Approved: roles-gate debt criterion, deleted rescue test, credit-line test rename, trade-gate assertion on wins.
- `World::restore` does not check budget against the lender limit (closing-time clamp covers it).

## 6. Independent review and bounded experiments (2026-10-12–13, base e232181)

### Status and location of the unmerged work

**Documented in the original repository on 2026-10-13; no candidate code has been applied here.** The reviewed original code remains at `e232181c80671ceb808eda07c56222f91eb567b4`, with WAR price 4.4 and rules version 2. All candidate results below refer to a separate local checkout, not to current main. The candidate passes the hoarder gate but still fails draft calibration and has a repeatable 11.88% offseason slowdown. **Hold the combined candidate; it is not ready to merge.**

- **History rewrite (2026-10-13):** every commit was re-authored to the owner's GitHub identity before the repository went public; file trees are unchanged, hashes are not. Hashes in this file are the new ones. The local Codex candidate below and its bundle were built on the pre-rewrite hash `af61282` of `e232181`; move them with `git rebase --onto e232181 af61282 <branch>`.
- Candidate branch: `codex/review-calibration-gates`, tip `a6e16650a764ad114fa13407dcc727e2309f5cbf`.
- Local checkout: a separate clone on the owner's machine (not pushed).
- Separate commits: `b98ccb3` shared roster-release cash plus regression and rules 3; `a983bf9` price/configuration experiment; `a6e1665` review and verification documentation. The release-only commit has not had its own full gate run; final verification covers the combined candidate.
- Report, verified patch/bundle, and logs: kept with that local clone (not in this repository). The report is `final-report.txt`; `baseball-sim-candidate.bundle` requires base `e232181`; `baseball-sim-review-package.zip` includes the evidence. Candidate code and these commits were not pushed; remote CI was not verified because network access was unavailable.
- Neither draft experiment nor the large economy/inflation or usage branch was merged. Do not repeat the completed attempts below without a new, explicit hypothesis and owner decision.

### Review corrections and unresolved defects

Line references below identify the reviewed base, not subsequent line shifts. P1/P2 denote release priority. The malformed-snapshot scenario requires inconsistent input; ordinary valid saves were not shown to corrupt spontaneously.

- **P1, invalid snapshot:** `crates/world/src/state.rs:664-669` accepts Settling with empty `last_records`; `crates/world/src/decide.rs:559` then panics. Reproduced restore followed by `SettleFinances`. Import uses the same restore validator (`crates/host/src/session.rs:1083`), established by source inspection rather than an end-to-end malformed-bundle test. Desktop panic handling discards the active league.
- **P1, calibration identity:** `crates/host/src/session.rs:1134-1135` compares only source and PA count. A saved game loaded successfully after changing `extra_inning_runner` with those labels unchanged. Content identity and legacy-save migration remain unresolved.
- **P1, draft measurement:** `crates/world/tests/world_gate.rs:310-317` uses WAR per season containing any playing time, whereas the cited FanGraphs study uses 500 AB or 25 pitching appearances. Reach/bust use signed picks, WAR uses all picks; the source's 55 selections per five-slot group do not establish the signed-only denominator asserted in PLAN. No measurement or band was changed.
- **P2, lender bypass:** `crates/world/src/decide.rs:494-502` checks signing against stored budget. Legal budget, stadium-purchase and signing commands in FreeAgency reached payroll 224.682M against a current lender limit of 194.874M. The offseason-only host clamp (`crates/host/src/session.rs:676-679`) does not cover this path; the section 5 parenthetical is incorrect.
- **P2, event atomicity:** `crates/world/src/apply.rs:670-680` removes a free agent before validating its club. An invalid team 250 panicked in `kind_count` (line 36) after removal. Event errors are not universally atomic.
- **P2, trade observation:** `crates/gm/src/trade.rs:348-355` requires incoming players in the market, but `crates/world/src/agents.rs:333` omits Single-A/Double-A. Reproduced a legal Double-A exchange judged `Stale`; adding only the offered player to the observation changed the verdict to `Gain(-0.6555)`.
- **P2, experimental branch scope:** the usage branch combines 1,500 near-pool-quality prospects with rounds, roster limits, minor costs, injury and usage changes. Its claim that early draft talent is unaffected was not established. Hold the bundle; test coherent pieces separately.

Debt-only interventions failed to remove the hoarder's edge; that does not prove debt has zero causal effect. Separate-league payroll differences do not estimate marginal dollars per WAR. The verified historical free-agent anchor is approximately $9M/WAR in 2014-16, net of minimum salary: https://blogs.fangraphs.com/the-linearity-of-cost-per-win/ . The informal 2023/2024 values in section 1 were not verified. Win-driven revenue already exists through smoothed `hype`; its causal contribution and diminishing returns from roster upgrades were not separately measured.

Draft sources do not all represent a 5-15-year window. Conforti observes 2005-15 through 2020 (https://journals.sagepub.com/doi/pdf/10.3233/JSA-200586); SABR observes 1996-2011 through August 2016 (https://sabr.org/journal/article/the-chances-of-a-drafted-baseball-player-making-the-major-leagues-a-quantitative-study/). The June 2016 FanGraphs article does not document an exact 2015 cutoff (https://community.fangraphs.com/success-rate-of-mlb-first-round-draft-picks-by-slot/). The gate also includes active players drafted 15 years earlier, so “complete careers” is too strong. The year-modulo-11 dev aid is only an approximation. The 828-pitcher source consolidates traded players, so dividing by 30 is not average distinct pitchers used per club (https://journals.sagepub.com/doi/10.1177/23259671251346984). The separate 47-per-club claim remains unverified.

The trade anchor is misapplied: the historical table lists 523 transactions in 1996-2000, arithmetically 104.6/year; multiplying its 3.58 per team by 30 and dividing by two again is unsupported (https://tht.fangraphs.com/the-best-and-worst-teams-of-the-trade/). The author later corrected omitted minor-league players without republishing the volume table (https://tht.fangraphs.com/the-best-and-worst-teams-of-the-trade-revisted/). That data caveat and the ambiguous per-team prose require a transaction-definition audit; 104.6 is not an independently validated modern target. MLB counted 58 in the 2024 deadline run-up alone (https://www.mlb.com/news/mlb-executives-react-to-2024-trade-deadline). The owner's near-54 target and the 40-110 gate remain unchanged pending a decision.

### Bounded experiments

- **Economy attempt 1:** price scalar only, 4.4 -> 9.0; no new revenue, cost, bailout or tax rule. The scalar also changes GM surplus/trade valuations and founding burn-in decisions. The source is net of minimum salary; the simulation uses `max(price * WAR, minimum)`, so this is an approximate anchor. Seed-1 mechanism prediction: hoarder edge between -0.015 and +0.015. Measured +0.0009, versus unchanged-base +0.0522. Reference-only payroll/budget 175.9/212.1 -> 214.7/211.3M; hoarder 215.0/204.9 -> 207.3/192.9M. This print-only diagnostic is not a full-gate pass. Candidate rules version is 3. Saved rules are retained on load, so existing leagues are not repriced.
- The unmerged candidate's commissioner price settings use a shared 2-20 range, replacing 2-9, so the new 9 default has room for the unchanged 10% annual changes. This is a configuration range, not an empirical gate band. The existing commissioner test's 5% increase remains valid without editing it. The roles adversary still clamps price to 9; its gate does not cover the entire new range.
- **Economy attempt 1 regression:** the default-profile workspace suite failed `roles_smoke`, case 6 seed 2, with one non-reckless insolvency. Do not treat a calibrated long-roles pass as a substitute for this short generic-model test.
- **Economy attempt 2, confirmed accounting defect:** `crates/world/src/agents.rs:602` on the reviewed base reset planned release cash inside each player-kind loop. An isolated price-9 trace reproduced team 2, year 2034: 75.406994M available, 59.743497M batter cuts plus 51.824151M pitcher cuts, leaving -36.160654M; next settlement left -20.113163M. No stadium spending occurred and signing commitments were below the current lender limit, so the stale-limit hypothesis was ruled out for this case. The fix shares one cash allowance across both kinds and debits mandatory fallback releases too. It changes no economic rate, bailout or debt rule. The same trace then had zero non-reckless insolvencies; that club's post-cut/settlement cash was 10.083497/20.338867M. New test `crates/world/tests/roster_release_cash.rs::release_plans_share_cash_across_batters_and_pitchers` failed before the fix and passed after it, including unavoidable-debt fallback accounting. No existing test was edited.
- **Draft attempt 1:** retain undrafted domestic players as free agents; no new pool talent or rounds. On seeds 1/2, top-10 mean WAR approximately 11.91/14.17 (predicted 12.5-14.5), middle reach .547/.528 (predicted .50-.57), late reach .166/.160 (below .17 floor). Failed. A new regression test for close/replay/restore passed, was preserved with the experimental patch, and was removed when reverting the isolated experiment. No existing test was edited. An earlier supplementary diagnostic recorded about 303 retained players and 131 WAR/year from undrafted players, suggesting a substantial change; those counts remain provisional because that extra diagnostic was not rerun with an exclusive target.
- **Draft attempt 2:** 10 -> 20 rounds from the unchanged 600-player pool, independently of attempt 1 and at the base price. MLB's 20-round structure is documented at https://www.mlb.com/amp/news/chicago-cubs-2021-mlb-draft-preview.html . Predictions: top WAR 12-14, middle reach .50-.55, late reach .17-.25. Measured seeds 1/2: top WAR approximately 13.95/15.28, middle reach .567/.540, late reach .196/.189. Failed. No tests changed. WAR reconstructed from rounded print-only output; bust was not printed. Stopped after two attempts; neither draft patch is adopted.

### Definition proposals awaiting the owner

Specify one draft study's cohort, observation cutoff, signed/reached denominators, pick grouping and workload normalization before replacing any gate. Keep the legacy result visible during comparison. Any revised bands require source derivation and explicit approval. For a possible economic exploit definition, compare paired-seed wins and guaranteed commitments over the same horizon, and report raw win edge as well; do not substitute $9M per standings win. The current win-edge definition remains in force.

The earlier unapproved test changes in section 5 remain owner decisions. Existing save-validation, lender and trade-observation defects above are not fixed by repricing. Both large experimental branches remain held, including unfinished inflation.

Verification provenance: early release builds shared an output directory across checkouts. A standalone runtime check later found a stale 4.4 library while candidate source said 9.0; Cargo had reported it fresh. Those cached results are not final acceptance evidence. Final verification uses a separate candidate-only output directory, path dependencies on the candidate checkout, and an isolated trace that verifies the active 9.0 default. The original default-profile baseline passed 212 tests, failed 0, ignored 23; its output directory was never shared with another debug build.

### Final candidate verification (2026-10-12–13, candidate-only build directory)

- Exploits, five seeds / 50 seasons: **PASS**. Hoarder -.0406 (SE .0092, z -4.40), tanker -.0917 (SE .0031, z -29.52), stretcher -.0098 (SE .0032, z -3.09), trade spammer -.0021 (SE .0030, z -.71). Prediction: all pass unchanged limits. Actual: all pass; 863.80 seconds under concurrent load.
- Roles, all 30 case/seed combinations: **PASS**, zero ordinary-AI debt settlements and zero roster failures; longest observed debt spell 7 seasons (prediction <=10). Maximum payroll/current-limit ratios were 1.235 for ordinary AI and 1.076 for reckless clubs; passing this gate does not resolve the separately reproduced lender-limit bypass. 373.30 seconds under concurrent load.
- Stability, five seeds: **PASS**, 673.79 seconds. All original rate, decade-drift, quality and cash limits passed. Prediction: pass unchanged bands. Detailed measurements are retained with the verification logs.
- Trade, five seeds: **PASS**, both unchanged tests, 262.23 seconds. Accepted trades/season: Rookie 61.4, Veteran 58.0, All-Star 55.2, Hall of Fame 55.5 (forecast 49-70; existing gate 40-110). Rookie value loss against HoF: -8.68M/trade across 794 trades, -6.51M for the 349 same-direction trades. The near-54 design request is closest for the harder presets; Rookie is 7.4/year above 54.
- Ladder ranking: **PASS**, 169.87 seconds. Rookie -.0586 < Veteran -.0060 < All-Star +.0224 < HoF +.0288; HoF minus Rookie +.0874, z 21.84. Prediction: this order and HoF +.02 to +.06. Values are win-rate fractions, not percentage-point units.
- AI versus reference: **PASS**, HoF +.0288 (SE .0027, z 10.79, n 100), 82.63 seconds; within the forecast +.02 to +.06 and above z=2.
- Draft, five seeds: **FAIL**, 393.81 seconds. Top-10 reach .8857-.9371, WAR 12.2132-14.3635, bust .5571-.6086; picks 11-30 reach .8229-.8500, WAR 8.6518-9.1302, bust .6886-.7486; picks 31-90 reach .6433-.6648, WAR 3.8252-4.1828; picks 91-300 reach .2536-.2659, WAR .8088-.9585, bust .9650-.9707. Prediction: still fail, top WAR 11-15 and middle reach .60-.67; both forecast ranges held. Late picks passed every metric; early/middle picks did not. No band was changed. This is not an overall calibration pass.
- Manager: **PASS**, +.0089 (SE .0016, z 5.70, n 1200), 89.15 seconds; forecast +.006 to +.010, z>=2.
- Calibration: **PASS**, K +.56%, BB -.46%, HR/PA -.36%, runs -.14% relative errors, all within unchanged 3% bands; .62 seconds.
- Season: **PASS**, 1,000 seasons with identical results on one/all threads; AVG .2556, runs/team-game 4.741, ERA 4.56; 19.03 seconds total. Forecast: original bands pass and thread-count identity holds.
- Ordinary workspace suite: **PASS**, 213 passed / 0 failed / 23 ignored, matching the forecast. This includes the previously failing generic `roles_smoke` and the new release-cash regression. `cargo fmt --all` and the final format check passed.
- Clippy, workspace/all targets with warnings denied: **PASS**, zero warnings, matching the forecast.
- Draft experiment provenance: both original two-seed experiments were reverified from exact saved patches in exclusive source/target pairs. Every printed seed statistic and histogram matched the exploratory logs; runtime checks confirmed price 4.4 and rounds 10 versus 20. Source/calibration hashes were unchanged. Diagnostic runtimes 189.70s / 115.99s; retention regression passed again. No new tuning, test edit or gate change was made. Both experiments still miss original bands.
- Quiet performance, one warmup plus five alternating pairs, separate targets: CLI digest **PASS**, `312166ce07b4b7ae` throughout, but **NO-SLOWDOWN REQUIREMENT UNMET**. CLI median seasons/s: 1 thread 76.8 -> 75.5 (-1.69%), 10 threads 133.3 -> 133.1 (-.15%). Host median seconds: founding .66 -> .67, 30 days .11 -> .11, season .15 -> .14, offseason 1.01 -> 1.13 (+11.88%). Offseason ranges were 1.00-1.02 vs 1.12-1.14 across all five measured pairs. Existing host timing limits passed, but that does not satisfy the stricter no-regression requirement. Forecast of no repeatable loss was wrong. Stop after two economy approaches; hold the combined candidate, do not tune a third approach without a new decision.

### Complete test, golden and band inventory

- This original-repository follow-up changes documentation only. No original source, test, golden, gate threshold, saved field, dependency or lockfile is changed.
- Candidate: one new test, `crates/world/tests/roster_release_cash.rs::release_plans_share_cash_across_batters_and_pitchers`, covering affordable cuts and mandatory-debt fallback accounting. It failed before the fix and passed after it. No existing test, golden or empirical gate band changed.
- Abandoned draft attempt 1: one new test, `crates/world/tests/undrafted.rs::undrafted_domestic_players_remain_available_after_close_replay_and_restore`, was added, passed, and removed when the isolated experiment was reverted. It is preserved only in `draft-retention-attempt.patch`. Attempt 2 changed no tests.
- The candidate's production commissioner range changes 2-9 to 2-20; this is a configuration limit, not an empirical gate band. Original code still uses 2-9. No gate-definition or band revision has been approved.
- Earlier unapproved test edits in section 5 remain separate owner decisions; this review does not approve them retroactively.

### Asks for the owner

1. Keep the combined candidate held. Authorize a separate performance follow-up or discard the price experiment; do not treat passing the looser 3-second host limit as satisfying the no-slowdown requirement.
2. Approve or reject a source-matched draft comparison before changing any gate definition or band. Specify one study's cohort, cutoff, signed/reached denominators, compensation-pick grouping and workload normalization; derive proposed bands with citations and retain the unchanged legacy result alongside it. A window approximation alone does not justify wider bands.
3. Decide whether the failed draft patches should be discarded or retained only as experiment history. Keep the large economy/inflation and usage branches held until separately justified and verified.
4. Decide whether near 54 trades/year is a game-design target or an empirical claim to replace after a transaction-definition audit. The 40-110 gate remains unchanged; the candidate's Rookie rate is 61.4/year.
5. Resolve the earlier unapproved test edits in section 5. The open save-validation, calibration-identity, lender-limit, invalid-event and lower-minors trade defects also remain to be addressed before release.
