# Open issues for discussion (written 2026-10-12)

Every number below was measured in a run; "sim" is our game, "real" is MLB with a source. Nothing here changed a gate threshold, golden value or test unless stated.

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

**Plan on the table.** Reprice free agents to real $/WAR and rebalance revenue and costs. Branch `worktree-agent-a4ba1b1810eb11ab0`, commit f155ad9, plus a WIP commit 39057f9 (inflation, unfinished). Never tested against the gate.

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
