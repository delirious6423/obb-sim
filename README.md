# OBB Sim (obb-sim)

A baseball management game in the style of Out of the Park Baseball. You run a
club as general manager: scout and draft, sign free agents, trade, set the
depth chart, and play out the seasons. Every player and club is invented.
The code is open source under the Apache License 2.0 (see `LICENSE`).

**Status: v0.1 alpha.** One person plays the general manager of one club,
and can also take the club's owner seat and the league's commissioner seat.
Every seat the person does not hold, including the other 29 general
managers, is run by the computer. Online leagues are planned, not built.

## Run it

You need Rust (stable, <https://rustup.rs>) and a C compiler.

```bash
cargo run --release -p ui
```

To make a clean build that is checked and copied to `dist/`:

```bash
scripts/build.sh
```

On Windows, run the script in Git Bash, or run the `cargo` command above.

`game --self-test` founds a small league and plays 20 days with no window.
The release workflow runs it (through `scripts/build.sh`) on all three
systems.

## Play

1. **New game**: pick a seed, your club, the years of history to play first,
   and a difficulty. Difficulty changes what the other general managers know
   and how well they decide. It never changes the dice.
   Presets: Rookie, Veteran, All-Star, Hall of Fame. Or set each lever.
2. **Home**: the buttons advance the league (a day, a week, a month, to the
   end of the season, the playoffs, the draft, free agency). A long step shows
   progress and can be cancelled. A cancelled step stops at a safe point.
3. **Depth chart**: promote or demote players between the majors and the
   three minor-league levels. "Auto-arrange" lets your staff do it.
4. **Free agents, Trades, Draft, Finances**: your tools as general manager.
   Grades shown are what your scouts think, not the true talent. With the
   owner seat you also set the budget, ticket price, ballpark spending and
   the club's stance; with the commissioner seat, the league rules.
5. **Saves**: save to a named slot, quick-save, load, delete. The game also
   keeps rotating autosaves. Saves are in `~/.obb-sim/saves`
   (set `OBB_SIM_SAVES` to use another folder; saves made before the
   rename in `~/.baseball-sim/saves` are still found).
   To move a save to another machine, export it to a `.bsave` file and
   import that file there. See "How to move a save" in `GUIDE.md`.

## Edit the fictional data

See `GUIDE.md` (section "Rename clubs and players").

## Build and test

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run --release -p host --example bench -- --check   # speed gate
python3 tools/licenses.py                                # licence audit
```

Project rules and the crate map: `CLAUDE.md`. Where the project stands:
`AGENT_HANDOFF.md`. Plan, gates and known gaps: `PLAN.md`; the failing gates
in detail: `ISSUES.md`. Data sources and licences: `THIRD_PARTY.md`.

## Data

The game ships only league-wide averages (`data/params.json`, a few KB). No
real player, name or stat line is in the repository. Retrosheet notice: the
information used here was obtained free of charge from and is copyrighted by
Retrosheet. Interested parties may contact Retrosheet at "www.retrosheet.org".

## License

Copyright 2026 delirious6423. Licensed under the Apache
License, Version 2.0 (`LICENSE`); see `NOTICE` for attributions and
`THIRD_PARTY.md` for dependencies and data sources.

This is an independent project. It is not affiliated with or endorsed by
Out of the Park Developments, Major League Baseball, or Retrosheet. Names of
other games and leagues are used only to describe and compare.
