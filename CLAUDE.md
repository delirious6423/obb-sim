# Project rules

Closed-source, OOTP-style baseball management sim in Rust. **Fictional players and teams only.**

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

Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` before saying anything is done. Report failures as they are.

## Crates

| Crate | Role |
| --- | --- |
| `sim-core` | Seeded RNG and pure game simulation |
| `protocol` | Commands, events, actors, command log, snapshot type, roles, ids |
| `league` | League state; permissions; `execute(actor, command) -> events`; `apply(event)` |
| `ai` | Role traits, role views, `vet` (early batch check) |
| `storage` | Persistence (not started) |
| `cli` | Binary front end |

## Notes

- Adding a `Command` variant must force a decision in `league::permissions` (`required_role` and `target_teams`; the matches are exhaustive on purpose).
- Every command is issued by an `Actor` (role + team; the commissioner has no team). `League::execute` always authorizes, so `Session::submit` and log replay cannot skip it.
- `League::apply` is fallible and strict. `from_events` and `Deserialize` validate; never trust loaded data.
