# Project rules

@AGENTS.md

Open-source (Apache-2.0, see `LICENSE`), OOTP-style baseball management sim in Rust. **Fictional players and teams only.**

## Architecture rules (always apply)

- `sim-core` is pure: no I/O, no clock, no global state.
- **Determinism**
  - Never iterate a `HashMap`/`HashSet` in sim code. Use `BTreeMap` or `IndexMap`. (`clippy.toml` bans the std hash types; do not work around it.)
  - Use `rand_chacha` with **one RNG stream per game**.
  - Use the `libm` crate for math (no `f64::exp`, `powf`, `ln`, etc. in sim code).
- **Every change to the league is a command that produces events.** Nothing edits state directly. State changes only by applying events.
- **Four roles** — commissioner, owner, GM, field manager — can each be a human or an AI agent. Agents only see what their role is allowed to see.
- **Difficulty settings change what the AI knows and how well it decides, never the dice.**

## Definition of done

Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` before saying anything is done. Report failures as they are. `cargo test --workspace` also runs the doctests. `AGENTS.md` lists the other commands.

## Crates

| Crate | Role |
| --- | --- |
| `sim-core` | Seeded RNG and pure game simulation |
| `protocol` | Commands, events, actors, command log, snapshot type, roles, ids |
| `league` | League state; permissions; `execute(actor, command) -> events`; `apply(event)` |
| `ai` | Role traits, role views, `vet` (early batch check) |
| `storage` | Save files: local multi-slot store (SQLite), manifests, checksums, `.bsave` bundles; knows nothing about the game |
| `cli` | Binary `sim`: season, box score and player viewers, `bench`, roster rename tool |
| `calibration` | Calibration of the engine against league-wide rates; generated rosters; file I/O lives here, outside `sim-core` |
| `season` | Full season: schedule, games, standings, playoffs; pure like `sim-core` |
| `world` | The living league: players over time, minor leagues, draft, free agency, club finances |
| `tactics` | Field-manager intelligence: run-expectancy and win-probability tables and a manager that uses them |
| `connect` | Agent connectors: typed observations and actions for each of the four roles; built-in seats; JSON for external seats |
| `gm` | General-manager AI: surplus-value model, personalities, difficulty presets, trade packages, exploit bots and the reference GM for the ladder |
| `host` | The protocol (`host::api`), the in-process league host and threaded client; the UI and any future server talk to the league only through it |
| `ui` | Desktop front end (egui), binary `game`; `game --self-test` is the headless smoke test |

The members list in the root `Cargo.toml` is the source of truth; keep this table in step with it.

## Notes

- Adding a `Command` variant must force a decision in `league::permissions` (`required_role` and `target_teams`; the matches are exhaustive on purpose).
- Every command is issued by an `Actor` (role + team; the commissioner has no team). `League::execute` always authorizes, so `Session::submit` and log replay cannot skip it.
- `League::apply` is fallible and strict. `from_events` and `Deserialize` validate; never trust loaded data.

## Working method

`AGENT_HANDOFF.md` is the entry point for a new session; it orders the other documents. `PLAN.md` is the project's memory and the source of truth for what to do next (its HANDOFF section first, then the first unticked item); `ISSUES.md` holds the failing gates in detail. Each fact lives in one place: put a pointer, not a copy, in the other files. Log every new dependency or data source in `THIRD_PARTY.md`. Calibration data lives in git-ignored `data/internal/` and holds no real player id, name or stat line.

## Claude Code tools

- Symbols, types, references: the rust-analyzer LSP plugin. Use it before text search.
- Fuzzy paths and repeated indexed searches: fff MCP (`find_files`, `grep`, `multi_grep`). Exact strings: `rg`.
- Plugins and MCP servers load when the app starts. If they are missing, tell the user to restart the app.

## Skills (these limits win over the skill text)

- `impeccable-rust`: unsafe code, concurrency changes, rewrites, big optimizations, verification reviews. Pick only the checks that match the change. Run only `impeccable doctor` and `impeccable bench-guard`. Never run `impeccable setup`, `impeccable run`, or `impeccable sanitize memory|leak`; they build a large Docker image. Ignore any step that says to commit.
- `hotpath_init`: first instrumentation only. Use 3 to 5 functions in total with `#[hotpath::measure]`; no `measure_all` in the first pass. Merge passthroughs into the existing `[features]` and never edit `default`. Do not change public signatures. Do not enable `hotpath-cloud`. Never overwrite an existing `hotpath/policy.toml`. Never run `hotpath init` or `hotpath init-ci` in a session. Do not instrument `sim-core` or `season` until the user says so.
- `simplified-technical-english`: new or edited technical docs only (`AGENTS.md`, `docs/`). Not for code, messages, chat answers, `PLAN.md`, or unrelated files.
