# OOTP 27 Benchmark vs baseball-sim

Written 2026-10-12. Research only; no code was changed.

## How to read this

- **Doc** = documented at the URL given. **Inferred** = my reading, or a forum/reviewer claim that OOTP has not confirmed. **Undocumented** = I found no figure.
- Most mechanics are documented only in the OOTP 23 online manual (the newest manual I could reach). OOTP 27 notes: I assume the mechanic carried over unless a 27 source says otherwise. This is an inference.
- The OOTP forum (forums.ootpdevelopments.com) returned HTTP 403 to my fetch tool, and the Reddit and Steam review pages were not readable. Forum facts below come from search snippets only. Treat them as thin.
- Manual base: `https://manuals.ootpdevelopments.com/index.php?man=ootp23&page=<slug>`. Slugs are named in each row.
- "Us" columns come from reading `README.md`, `GUIDE.md`, `PLAN.md` and grepping `crates/` (world, gm, connect, host, ui). Where I only grepped, I say so.
- Copyright: all text is my own summary. No manual passage is quoted.

## Summary

OOTP 27 (March 2026) is a mature, deep sim. Its 27 headline changes are: a rebuilt trade AI, four difficulty tiers (Minors, Majors, All-Star, Hall of Fame), many new advanced stats (exit velocity, xBA, barrel rate and others), a World Baseball Classic mode, dynamic weather in the 3D engine, and UI polish (sources: ootpdevelopments.com/?p=35526, SGO review via search, gmgames.org). The core management systems (finance, contracts, 40-man rules, scouting, chemistry, online leagues by file export) have been stable for years.

We already match OOTP on: a hidden-true-talent model with noisy scouting, a draft with team-specific boards, trades, free agency, a stadium/ticket/attendance model, owner and commissioner roles, revenue sharing, difficulty presets that change information only, and a three-level minor-league roster structure.

We are missing most of OOTP's rules layer (waivers, DFA, options, Rule 5, arbitration, service time, no-trade clauses, luxury tax, media/merchandise revenue, an inflation setting), the human layer (personality, morale, chemistry), the in-game layer (fatigue/rest, platoon splits, weather, parks, advanced stats) and the league-wrapper layer (custom leagues, random events, online play, export/mod tools).

Our best chances to win: an open protocol with agent connectors, a deterministic event log (replay, undo, fork), speed (full 162-game season in about 0.13 s on a laptop per `PLAN.md`), human-or-AI per role, and no hidden "CPU cheats" (OOTP's hard modes skew development luck toward the AI, and players dislike that).

## Area 1: Finances

| OOTP 27 feature / mechanic | Source | Do we have it | Priority | Note |
|---|---|---|---|---|
| Seven revenue streams: gate, media, merchandise, playoff, revenue sharing, owner cash infusions, cash from trades. Doc (OOTP 23, assumed in 27). | manual `the_team_financial_model` | Partial. We have gate, local market revenue, revenue sharing (`Rules.revenue_sharing`), a credit line and rescue. No separate media, merchandise, playoff gate or cash-in-trades. | should | Add playoff gate and national/local media split first; they are easy and explain team differences. |
| Expense lines: player salaries, staff salaries, cash in trades, scouting budget, draft budget, revenue-sharing payments. Doc. | same | Partial: payroll, `MINOR_COST`, budget. No staff, scouting or draft budgets. | should | Scouting and draft budget tie into accuracy (see Area 2). |
| Gate = ticket price x attendance. Home keeps 80% of gate, visitor 20% (editable). Attendance driven by market size, fan loyalty, fan interest; winning and stars help. Doc. | manual `the_team_financial_model`; baseballreplayjournal.substack.com/p/gate-revenue-in-ootp | Partial. We have `SetTicketPrice`, capacity, quality, `demand = market base ...`, crowd = min(demand, capacity), home games only. No visitor share, no fan-interest state. | must | Add a lagging "fan interest" per club (rises with wins/stars, decays slowly) and a visitor gate share. OOTP does not model tiered pricing; we can beat it with price tiers and dynamic pricing. |
| Fan interest and loyalty modifiers lag results. Doc (blog); formula undocumented. | substack gate-revenue-in-ootp | No | should | Our design: `interest_t+1 = interest_t + k*(win_pct - 0.5) + star_bonus - decay`. This is my design, not OOTP's. |
| Merchandise: set by baseline, driven by market size, interest, loyalty. Blog says the formula is old and poorly understood. Doc (blog), formula undocumented. | substack.com/p/merchandise-in-ootp | No | later | Cheap to add as a fraction of market revenue scaled by interest. |
| Media revenue: national plus local contracts; commissioner can equalise. Doc (blog). | substack.com/p/introduction-to-ootp-finances, forum snippets | Partial: "45% market" in local revenue. | should | Add a national equal share plus local by market. |
| Revenue sharing: default pool 48% of income (matches the real 2022 MLB rate). Up to 100% acts as a hard cap-like equaliser. Doc (blog). | substack.com/p/revenue-sharing-in-ootp | Yes. Rules range 0.06 to 0.25 in AI use; commissioner can move 0.04 a year. | done | Let human commissioners go to 1.0 as an option. |
| Luxury tax: optional; payroll above a threshold taxed. Default rates undocumented. Doc (existence only). | substack.com/p/revenue-sharing-in-ootp | No (we have payroll floor and hard cap only) | must | Add `tax_threshold`, `tax_rate`; tax pool redistributed. |
| Hard salary cap exists as an option, plus soft cap and floor emulation through budget settings. Doc. | same | Yes (floor and cap in `Rules`). | done | |
| Owner budget: "Team Owner controls Budget" on/off. If on, budget < revenue; owner may pocket shared revenue. Doc. | manual; substack.com/p/introduction-to-ootp-finances | Yes (`SetBudget`, owner AI, stance). | done | We are better: stance (Contend/Balanced/Rebuild) steers the GM. |
| GM can be fired for financial irresponsibility unless commissioner or "cannot be fired". Doc. | manual `the_team_financial_model` | No | should | Needed for a human GM when owner is AI. |
| Cash cap / budget cap per team (commissioner setting). Doc (forum snippet). | forums.ootpdevelopments.com/showthread.php?p=4016835 | Partial (credit line `CREDIT`). | later | |
| Fiscal year runs from the day after the playoffs to the end of the next playoffs. Doc. | manual | Partial (settle at season end). | later | |
| Inflation / salary scale: the manual page I could reach does not mention inflation. Forum users say there are a yearly inflation percentage (one user used 0-2%), per-tier "typical" salaries and an option to re-import adjusted financial settings each year. Default values: undocumented. | forums.ootpdevelopments.com/showthread.php?p=4269294, p=5134004 (search snippets only) | No. We have only a market `war_price` that the commissioner may move 10% a year; `era` shift is for talent, not money. | must | See gap list #3. |
| Debt consequences: manual page does not describe them. Inferred: negative cash blocks signing and can get the GM fired. | manual (silent) | Yes and richer: credit line, lender budget limit, rescue voids contracts above floor. | done | We are ahead on documented rules. |
| Draft budget: separate, use-it-or-lose-it; overage reduces finances by the overage. Doc. | manual `the_team_financial_model` | No (draft costs are bonuses only). | later | |
| Financial projections shown in settings. Doc (blog). | substack revenue-sharing | Partial (Finances screen) | later | |

## Area 2: Player development, aging, injuries, personalities

| OOTP 27 feature / mechanic | Source | Do we have it | Priority | Note |
|---|---|---|---|---|
| Ratings on a 20-80-style display, many hidden fields; development has 8 factors: coaching staff (GM, manager, bench, hitting, pitching coach), playing time, potential, age, challenge, injuries, spring training, chance. Doc. | manual `player_development` | Partial. Hidden peak, shift, decline, drift; luck; no coaching, playing-time or spring-training effect. | should | Add playing-time effect for minors, and staff quality. |
| Aging and development speed multipliers (1.0 normal; 0.5 half; 1.5 fast); "development ends about 25, decline begins about 30" default; talent-change-randomness setting. Doc (blog). | substack.com/p/injuries-and-aging-in-ootp | Partial: peak 27, decline from 30, per-origin shift sd. No user multipliers. | should | Expose three levers in the new-game and commissioner screens. |
| Scouting: director plus budget split across Major, Minor, International, Amateur; accuracy report per player; OSA second opinion; scouting can be turned off (true ratings shown). Doc. | manual `scouting`; substack.com/p/scouting-in-ootp | Partial. We have noisy opinions (1.5 sd on amateurs) and difficulty-dependent info. No budget, no director, no per-player accuracy, no second source. | must | Show an accuracy value on each report and let money buy accuracy. |
| Younger and international players have more scouting error. Doc (forum). | forums showthread.php?p=5188669 (snippet) | Partial: amateurs only. | should | |
| OOTP 26 change: high-accuracy scouts can see the player's development path (fast/slow), not only the average curve. Doc (forum, snippet). | same | No | later | |
| Injury types: day-to-day (reduced effectiveness), out (days to near a year), career-ending (60-day IL, retire at season end). Body areas: arm, leg, back, illness; injury raises same-area risk. Rating losses possible. Doc. | manual `injuries_fatigue` | Partial. `out_games`, `pending_loss` (permanent damage), minors injured too. No body area, no day-to-day tier, no injury-prone trait. | must | |
| Hidden injury proneness (overall, arms, back, legs); can be revealed. Doc (blog, forum). | substack injuries-and-aging | No | should | |
| Overall injury frequency is a global setting (off/low/normal/high). Doc (forum). | forum snippet | No setting | should | Add as a difficulty lever. |
| Pitcher injury risk from abuse points: pitches over 100 earn (pitches-100)^3 points per start (e.g. 160 pitches gives 216,000); also rust and pitch count. Doc (blog; the blog printed a typo, which I corrected). Whether OOTP uses exactly this: unconfirmed. | substack injuries-and-aging | No | should | Good first formula. |
| Fatigue: pitchers mainly by pitches in the last five days and stamina; position players mainly by games played; day off restores; tired players perform worse and are injury-prone. Doc. | manual `injuries_fatigue` | No (fixed lineup and 5-man rotation all season per Known gaps) | must | Largest in-game gap. |
| Injury list: 60-day IL, auto-play stops when a player out 3+ days. Doc. | same | Partial (out games, no IL) | should | |
| Minor-league pitch-count limits for the AI: AAA 110, AA 100, A 95, lower 90. Doc. | same | No | later | |
| Platoon / handedness splits | manual (not located) | No (grep found nothing) | must | Undocumented for OOTP; ratings split vs L/R is standard. |
| Personality: extrovert/ambivert/introvert plus 3-9 secondary traits; relationships +5 / -5 between same / opposite. Leadership, loyalty, greed, work ethic, intelligence shown. Doc (forum, manual). | manual `chemistry`; forum showthread.php?p=4032461 (snippet) | Partial: GM AI has personalities; players have none. | should | |
| Morale: seven levels Angry to Great; four factors (team performance, transactions, own performance, role). Affects performance, extensions, trade requests, role demands. Small development effect. Can be turned off. Doc. | manual `player_morale`, `chemistry` | No | should | Players unhappy at the wrong role is the best hook for human GMs. |
| Team chemistry rating from the mix of personality classes; bench coach summary. Doc. | manual `chemistry` | No | later | |
| Player popularity (drives transactions' morale effect and attendance). Doc (title only). | manual `player_characteristics` | No | later | |
| Suspensions, retirement | manual (titles) | Retirement yes | later | |

## Area 3: Trades, rosters, contracts

| OOTP 27 feature / mechanic | Source | Do we have it | Priority | Note |
|---|---|---|---|---|
| Rebuilt trade AI in 27: biggest change of the year per a reviewer; older version let you flip aging veterans for prospects to every "contender". Builds 27.3.65 / 27.3.66 changed trade volume and types, expiring-contract valuation, salary retention, deadline deals, prospect rankings in trade logs. Internals undocumented. | SGO review and forums.ootpdevelopments.com/showthread.php?p=5265054 (via search) | Partial. `SurplusGm` with personalities; trades rare (3 proposed, 0 accepted per 5-year AI run). | must | See gap #5. |
| Global options: AI trading frequency (Very Low to Very High), trading difficulty (Very Easy to Very Hard, "asks for more"), trading preference (favour veterans to favour prospects). Doc. | manual `trading_options` | Partial: difficulty preset sets decision quality only. | should | Expose frequency and preference as levers. |
| League options: trading on/off, deadline date, cross-league trades, recently-drafted players tradable, 10/5 veto rule, draft-pick trading. Doc. | same | Partial: no deadline, no picks, no 10/5. | should | Draft-pick trading is a big missing piece. |
| Trading deadline: after it, players go on revocable waivers; a claim cancels the trade. Doc. | manual `making_trades` | No | should | |
| "Thought required" vs "instant agreement" trades; "Make this work now" lists what the AI would add; asking for a player signals eagerness, so the AI asks more. Doc. | same | No (our `RespondTrade` is accept/reject) | must | Counteroffers are the key UX piece. |
| Retain salary in trades, cash in trades, research trades (tools). Doc (titles). | manual trading nav | No | should | |
| Commissioner can force a trade. Doc. | same | No | later | |
| Trading block / "untouchables" / AI needs. Found no documentation of them in OOTP. Inferred only. | none | No | should | Our design: block = list of players with asking price; AI ranks needs by positional value over replacement. |
| Waivers, DFA: player demoted without options goes to DFA, default 10 days to decide; a claim cancels a trade. OOTP does not copy MLB exactly. Doc (forum, dev posts). | forums showthread.php?p=... (snippet) | No | must | |
| Minor-league options: three option years after 40-man add; once spent, the player must clear waivers to be demoted. Doc (dev post, 2007; 2018 dev says simplified). | forum snippet | No | must | |
| Rule 5 draft: December; chosen player stays on the active roster or IL all season; eligibility 3-6 years of service, age 19 or older at signing; no fee, no minor-league phase, no offer-back. Doc. | manual `player_drafts`; forum snippet | No | should | |
| Salary arbitration: 3 to 6 years of service plus Super 2 (top 17% of those with 2+ years); GM submits an offer; arbitrator picks the team's offer or the player's demand; morale dip. Configurable. Doc. | manual `salary_arbitration` | No (contracts are salary x years only) | must | |
| Service time and free-agent eligibility. Doc (title). | manual `service_time` | No | must | Prerequisite for arbitration and options. |
| Contracts: guaranteed; release pays the rest; no-trade clauses and incentives possible; extensions use a separate "$ for extensions" budget; under 3 years service auto-renewed at minimum; contracts end the day after the playoffs. Doc. | manual `player_contracts_overview` | Partial: guaranteed, release dead money yes; no clauses, incentives, extensions. | must for extensions, should for the rest | |
| Minor-league contract is $0; adding to the 40-man converts to a one-year league-minimum deal. Doc. | same | Partial (`MINOR_COST` exists) | later | |
| Posting (Japan/Korea players). Doc (title). | manual | No | later | |

## Area 4: Draft, amateurs, minors

| OOTP 27 feature / mechanic | Source | Do we have it | Priority | Note |
|---|---|---|---|---|
| Four draft types: inaugural (dispersal), first-year player, Rule 5, expansion. Doc. | manual `player_drafts`, `the_draft_process` | Partial: first-year draft only | should | Expansion and dispersal drafts matter for custom leagues. |
| Draft steps: pool announced, pre-draft, adjust order/rules, hold, post-draft. Round count, lottery, signing: undocumented in what I reached. | manual `the_draft_process` | Yes: noisy boards, signability; rounds is a commissioner rule (`draft_rounds`, change at most 2 a year). | done | |
| Draft pool export for future classes (added in 27.3.65). Doc (via search). | forums p=5265054 | No | later | Easy for us with open data. |
| International free agents (since OOTP 14): ages mostly 16-18, revealed on a date, with a preset spending cap and a tax and penalty for exceeding it. Doc (newsletter). | ootpdevelopments.com/newsletters/nl0112 (snippet) | Partial: `international_rounds` and `Origin::International`. No spending cap or pool. | should | |
| Hidden international players found by scouts, can be turned off. Doc. | manual `international_scouting` | No | later | |
| High school / college / undrafted free agents | undocumented for OOTP | Partial: HS / College / International origins; undrafted pool in tests | should | |
| Minor-league structure: affiliates, level equivalencies, pitch limits | manual (partial) | Partial: AAA/AA/A roster tiers only; no minor-league games | should | Needed for development "playing time" and box scores. |
| Promotion / relegation; tournaments | manual titles | No | later | |

## Area 5: Game sim and manager

| OOTP 27 feature / mechanic | Source | Do we have it | Priority | Note |
|---|---|---|---|---|
| Depth charts, lineups vs LHP/RHP, platoon at positions, pinch-hit and substitution settings. Doc (forum, snippets). | forums (snippet) | Partial: depth chart and Auto-arrange, one lineup. | must | |
| Team strategy: running, hit-and-run, bunting, steals, intentional walks (IBB logic updated in 27.3.65). Doc. | forum snippets | Partial: tactics (steal, relief) via table manager. | should | |
| Pitcher use: stamina setting; starters about 10-15 more pitches when global stamina set one step up; AI bullpen aggressiveness; closers. Doc (dev post, snippet). | forum snippet | Partial: starter plus relief rule, closer in save spot, leverage tables. | should | |
| Advanced stats added in 27: launch angle, exit velocity, FB%, hard-hit%, barrel%, xBA, xSLG, chase%, K%, with percentile rankings. Doc. | onemoregame.ph (via search), ootpdevelopments.com/?p=35526 | No batted-ball model | should | We have no exit velocity; needs a batted-ball layer. |
| Weather (fog, overcast, rain) in 3D engine; parks incl. new models. Doc. | same | No (Known gaps: no weather, no park factors) | should | Park factors first; they are also needed for variance. |
| 3D game viewer | same | No | later | Not a goal. |
| World Baseball Classic with real federations. Doc. | same | No | later | Fictional data only. |
| Real-league licences (MLB, KBO, Dutch, British). Doc. | same | N/A (closed, fictional) | later | |

## Area 6: League options

| OOTP 27 feature / mechanic | Source | Do we have it | Priority | Note |
|---|---|---|---|---|
| Difficulty: four tiers (Minors easier, Majors baseline, All-Star harder, Hall of Fame hardest); harder tiers skew talent-change randomness toward the AI. Many fine levers. Doc (interview via search). | sportsgamersonline.com road-to-release ep. 4 (fetch blocked; via search) | Yes: Rookie, Veteran, All-Star, Hall of Fame plus custom levers; information and decision quality only, never the dice. | done | A selling point. |
| Commissioner: league events (preseason, spring training, opening day, draft, All-Star Game, deadline, roster expansion, awards, FA filing, arbitration, Rule 5, Winter Meetings); auto-play stops at them. Doc. | manual `league_events` | Partial: phases (season, playoffs, draft, FA); no calendar events. | should | |
| Random events | not found in what I reached | No | later | |
| Custom leagues: league and team creation, rules screen, historical or fictional leagues, import of financial settings per year. Doc. | manual `creating_games`, forum snippets | Partial: seeded fictional league; roster rename tool only. | should | Add a league-rules file (JSON) as the public format. |
| Online leagues (OOTP): commissioner runs league files; GMs export their team via FTP or upload; commissioner imports and sims; a "Commissioner Portal" and real-time-sim broadcast pages exist in the manual; third-party StatsPlus adds Discord and web stats. Doc (manual TOC, 2018 guide via search, statsplus.net). Detail: undocumented. | manual `ootpb_online_leagues`; wiki.statsplus.net | No (Phase 8 not started) | should | Our architecture (commands and events, role filter) suits real server-hosted play. |
| Perfect Team (card-collecting multiplayer mode) returns in 27. Doc. | ootpdevelopments.com/?p=35526 | No | later | |
| Modding / export: CSV files, in-game editors; draft pool export. Doc (manual TOC "about comma separated files", "tools functions and editors"). | manual nav | Partial: names-only roster tool | should | |
| Save format: world snapshot only | n/a | Partial: no command log; AI GMs restart fresh after a load | must | Breaks replay and online play. |

## Area 7: UI, quality of life, community complaints

Evidence is thin here (Reddit and Steam text not readable; forum 403). Items marked inferred are my reading of search snippets.

| OOTP 27 issue or feature | Source | Do we have it | Priority | Note |
|---|---|---|---|---|
| 27 reworked calendars, in-game screens, player pages; dev goal was to modernise and streamline the most-used screens. Doc. | SGO Road to Release (via search) | Partial: egui screens; schedule shows day numbers, not dates | should | |
| Steam rating Mostly Positive (about 83-85% of 365-418 reviews in two snapshots); one critical review says series veterans may be annoyed by this year's update. Doc (Steam, via search). | stmstat.com/app/4045750/reviews; Steam | n/a | n/a | |
| CPU "cheats" in hard modes (better trade and contract outcomes). Forum thread, March 2026. | forums.ootpdevelopments.com/showthread.php?p=5251816 (snippet) | We avoid it by design | should | Marketing point. |
| AI trade frequency too high; AI trades prospects for old players (late-2025 forum complaints). Inferred. | forum snippets | Opposite problem (too few trades) | must | |
| AI 40-man roster logic still weak (reviewer). | SGO review | n/a (no 40-man) | n/a | |
| Records and milestone alerts are minimal; community thread asks for record-breaking alerts (June 2026). Doc. | forum (kq76 thread, via search) | No | later | Easy with an event log. |
| Ratings scale clutter: top "blue" band covers about 37% of the 1-250 range (wishlist, June 2025). Doc (single poster). | forum p=5228546 (snippet) | Our 20-80 grades | later | |
| Contract editor lacks quick-adjust buttons. Single poster. | same | n/a | later | |
| Catcher framing stats requested earlier (2022). | forum snippet | No | later | |
| No KBO All-Star format or historical mode (a refund reason in 26). Single poster. | forum snippet | n/a | later | Shows demand for league-format options. |
| Reports of crashes opening a player profile late in game (single mobile user). | forum snippet | n/a | n/a | |

## Prioritized gap list (top 15)

Each item says what to build. "OOTP:" lines state what is documented; our design numbers are suggestions to tune against our gates, not OOTP values.

1. **Service time, options, waivers/DFA, 40-man split.** OOTP: three option years after a player joins the 40-man roster; when options are gone a demotion needs waivers (doc, forum dev post). DFA window default 10 days; a claim cancels a trade. A player on a league contract generally clears waivers before a demotion (2018 dev post). Build: per-player `service_days`, `options_used`, `on_40man`; commands `DesignateForAssignment`, `ClaimWaiver`, `Outright`; events for each. Everything below (arbitration, Rule 5, extensions) needs `service_days`.
2. **Salary arbitration and extensions.** OOTP: eligible at 3 to under 6 years service plus Super 2 (top 17% of those with 2+ years); GM sends a one-year offer before hearings (November); arbitrator picks team offer or player demand; losing costs morale; under 3 years auto-renewed at minimum; extensions use a separate budget (doc). Build: `SubmitArbOffer`, `ArbHearing` event (pick closer of the two to a market salary with noise), `Extend {years, salary}`. Hearing rule is undocumented: use closer-to-market with noise.
3. **Inflation / salary scale.** OOTP: forum posts mention a yearly inflation percent (one user: 0-2%), per-tier typical salaries, and a setting to re-import adjusted financial values each year. Defaults and exact formula: undocumented. Build: `Rules.inflation` (annual %, default 2%), applied to `war_price`, payroll floor and cap, minimum salary, ticket baseline and revenue baselines. Track a `price_index`; show real and nominal salary. Commissioner may set 0 to 5%. Test: 50-season runs stay solvent with inflation on (extends the Phase 7 gate).
4. **Luxury tax plus full revenue-sharing range.** OOTP: tax optional; payroll above a threshold taxed; sharing default 48% (matching MLB), 100% means a de-facto hard cap (doc, blog). Build: `tax_threshold`, `tax_rate` (suggest 20%, 30% repeat), tax proceeds shared; allow sharing 0 to 1.0.
5. **Trade AI: counteroffers, need-based asks, trading block, untouchables, picks, deadline.** OOTP 27 rebuilt the AI (internals undocumented); manual documents "Make this work now", instant-agreement vs thought-required, eagerness signalling, revocable waivers after the deadline, retained salary and cash (doc). Build: `AskWhatItTakes` command returns the AI's top-N additions; AI valuation = surplus value + positional need weight + team direction (contend/rebuild by stance); player flag `untouchable` for AI; trading block list; draft-pick assets; deadline date and waiver trades. Fix our low AI-to-AI volume (3 proposed, 0 accepted per 5 years) with a frequency lever (Very Low to Very High).
6. **Fatigue and rest.** OOTP: pitcher fatigue from pitches in the last five days plus stamina; hitter fatigue from games played; a day off restores; tired players perform worse and are injury-prone (doc). Build: `fatigue` 0-100 per player, pitcher: sum of pitches in a 5-day window / (k * stamina); position player: +x per game, -y per rest day; performance multiplier (suggest -0.5 talent sd at fatigue 100) and injury-hazard multiplier. Tune against our league-wide rates.
7. **Injury model: tiers, body area, proneness, abuse points.** OOTP: day-to-day vs out vs career-ending; arm/leg/back/illness; same-area recurrence; hidden proneness; rating loss possible; pitcher risk from abuse points, with (pitches-100)^3 as one published formula (doc). Build: `Injury {area, tier, days}`, `proneness` per area (hidden, can be scouted), global frequency lever (off/low/normal/high), 60-day IL.
8. **Personality, morale, chemistry.** OOTP: three primary classes plus 3-9 secondary traits, five visible rating traits; morale from team performance, transactions, own performance and role, seven levels; chemistry from the class mix; morale affects extensions, trade requests, role demands and slightly development (doc). Build: traits `leadership, loyalty, greed, work_ethic, intelligence`; `morale` update after each event; effects: re-sign willingness, trade request, small development multiplier (suggest at most 5%). Chemistry later.
9. **Platoon splits and lineups vs L/R.** OOTP: depth chart supports separate lineups vs LHP and RHP and platoons at a position (doc, forum). Our engine has none. Build: `bats`/`throws`, per-player vs-L and vs-R talent split, two lineups, auto-platoon option. Sizes undocumented; use real MLB platoon splits as a gate.
10. **Scouting budget, director and per-report accuracy.** OOTP: one pool split into Major, Minor, International, Amateur; more money tends to raise accuracy but not guaranteed; each report shows an accuracy rating; younger and international players have more error; a second source (OSA) exists; a director change can shift ratings; scouting can be switched off (doc). Build: `scouting_budget[4]`, director quality, noise sd = base / f(budget), accuracy shown per report, and a free "public consensus" opinion.
11. **Gate model: fan interest, visitor share, playoff gate, media and merchandise.** OOTP: gate = price x attendance; home 80% / away 20% default; fan interest lags results; media national plus local; merchandise baseline (doc). Build: `fan_interest` state; `gate_share_visitor` rule; playoff gate per home game with a multiplier (suggest 1.5 to 3x); national media equal split + local by market; merchandise = c x market x interest. Keep `SetTicketPrice`; add price tiers (OOTP has none).
12. **Draft-pick trading, Rule 5, international spending pool, expansion/dispersal drafts.** OOTP: pick trading option (current-year picks only); Rule 5 in December with roster-for-season rule; international pool with a cap and a tax and penalty (doc); four draft types. Build in that order.
13. **Parks and weather.** OOTP 27 adds dynamic weather (fog, overcast, rain) in 3D; parks have effects (doc for the feature, effects undocumented). Build: per-park factors for AVG/HR/doubles by handedness; weather as temperature and wind modifiers on HR and run rate; also adds game-to-game variance, which `PLAN.md` says is too low.
14. **Save as command log + online league mode.** OOTP uses file export through FTP/commissioner portal (doc). Our events and commands are already serialisable (Phase 5). Build: append-only command log in saves, deterministic AI-GM state restore (currently GMs restart fresh), then Phase 8 server with the four roles per human.
15. **Calendar events and commissioner tools.** OOTP: League Events list stops auto-play (preseason, spring training, deadline, All-Star Game, roster expansion, awards, arbitration, Rule 5, Winter Meetings) (doc). Build: event calendar in the schedule (with real dates, not day numbers), stop-at-event option, commissioner force-trade and rules screen, league-rules JSON.

Also worth a mention (below the top 15): advanced stats (needs a batted-ball model), records alerts, coaching staff effect on development, hidden international players, posting, 10/5 veto, no-trade clauses (documented as a contract feature; details undocumented).

## Opportunities where we can beat OOTP

All are design choices that fit what already exists (event-sourced world, role-filtered views, `connect` crate transports).

1. **Agent-native play.** The `connect` crate already has typed observations and actions, a `Transport` trait and a stub external agent for all four roles (`PLAN.md` Phase 4a). OOTP has no AI-agent API. Publish the protocol (JSON schema) and an MCP-style server so LLM agents, scripts or human tools can be GM, owner, commissioner or manager. Inferred: OOTP has no documented equivalent.
2. **Open, scriptable league files.** OOTP uses CSV import/export and in-game editors (doc: manual TOC). We can offer a documented league-rules JSON, a command-line tool (`cli roster export/import` exists, names only) widened to ratings, contracts and rules, plus a script hook (Lua or WASM) for custom rules and events. Priority: after Rule/contract features.
3. **Replay, fork and undo.** An event log gives replay, "what if" forks and exact rewind. OOTP saves are monolithic. Finish the command log (gap 14) and ship "branch from any day".
4. **Speed.** Our full season takes about 0.13 s and founding 0.7 s on an Apple Silicon laptop (`PLAN.md`, own measurement). Show it: sim 100 seasons in seconds, bulk runs for balance testing and for online catch-up. I did not measure OOTP; do not claim a ratio without a benchmark.
5. **No hidden CPU advantage.** Difficulty changes what the AI knows and how well it decides, never the dice. OOTP's harder tiers skew talent-change randomness in favour of the AI, and players complain about CPU cheating (doc for the skew; forum snippet for the complaint). Say so on the product page, and publish the AI's decision log (we already log decisions) so players can inspect it.
6. **Human or AI in every role.** Owner, commissioner, GM and field manager can each be a person or an agent. OOTP's roles are fixed around the GM. Online leagues can split roles across people (an owner who sets ticket price and stance, a GM who works within it).
7. **Real server-side online leagues.** OOTP online leagues are commissioner-run file exchange (doc: manual TOC and a 2018 guide). With commands and events plus role-filtered views we can offer live turns, deadlines, auto-sim waypoints and spectator feeds without FTP. Phase 8.
8. **Open stat and event feeds.** Publish the event stream for web dashboards (StatsPlus-style tools exist for OOTP as separate third-party software; doc).
9. **Cleaner rules, fewer legacy quirks.** OOTP states its roster rules are close to but not identical to MLB and are simplified; examples: repeated outrights allowed, no Rule 5 fee (doc, forum snippets). We can ship a clean, documented rule set with presets (MLB-like, simple, custom), each described on one page.
10. **Better UX debts we can avoid from day one**: real dates on the schedule (we show day numbers today), records alerts from the event log, accuracy shown on every scouting number, and counteroffer buttons in the trade screen.

## Verification gaps and caveats

- No OOTP 27 manual was available; I used the OOTP 23 manual, 27-era news and reviews, and search snippets of forum threads. Anything about 27 internals (trade AI weights, difficulty levers, injury rates) is undocumented here.
- The inflation mechanic is the weakest item. The only evidence is forum snippets; the manual pages I reached do not mention inflation. I could not confirm defaults. Treat gap 3 as our design, not a copy.
- Several rows quote forum threads I could not open (403). Their URLs come from search results; confirm before relying on them.
- "Do we have it" for items marked No means a keyword grep over `crates/*.rs` found nothing, and `PLAN.md` Known gaps agreed where it speaks to it. Advanced stats and park effects were not separately grepped beyond the terms listed in my search.

## Sources

- https://www.ootpdevelopments.com/?p=35526 (OOTP 27 feature list)
- https://manuals.ootpdevelopments.com/index.php?man=ootp23 (pages: the_team_financial_model, trading_options, making_trades, injuries_fatigue, salary_arbitration, player_contracts_overview, chemistry, player_morale, player_development, scouting, international_scouting, league_events, player_drafts, the_draft_process, roster_rules_and_management, ootpb_online_leagues)
- https://baseballreplayjournal.substack.com/p/introduction-to-ootp-finances
- https://baseballreplayjournal.substack.com/p/gate-revenue-in-ootp
- https://baseballreplayjournal.substack.com/p/revenue-sharing-in-ootp
- https://baseballreplayjournal.substack.com/p/merchandise-in-ootp
- https://baseballreplayjournal.substack.com/p/injuries-and-aging-in-ootp
- https://baseballreplayjournal.substack.com/p/scouting-in-ootp
- https://www.sportsgamersonline.com/news/reviews/ootp-27-review-out-of-the-park-baseball/amp/ (via search summary)
- https://onemoregame.ph/2026/02/out-of-the-park-baseball-27-launch/ (via search summary)
- https://forums.ootpdevelopments.com/showthread.php?p=5265054 (27.3.65 / 27.3.66 changelog, via search summary)
- https://forums.ootpdevelopments.com/showthread.php?p=5251816, p=4269294, p=5134004, p=5188669, p=5228546, p=4032461 (via search summaries only)
- https://ootpdevelopments.com/newsletters/nl0112 (international free agency, via search summary)
- https://wiki.statsplus.net/ (third-party online league tools)
