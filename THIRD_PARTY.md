# Third-party notices

Every dependency and data source is logged here.

## Data

### Retrosheet (calibration data)

Play-by-play event files for 2015-2019 are downloaded into `data/internal/`, which is git-ignored. Only league-wide rates and talent spread are derived from them; no player id, name or stat line is stored.

Required notice, from Retrosheet:

> The information used here was obtained free of charge from and is copyrighted by Retrosheet. Interested parties may contact Retrosheet at "www.retrosheet.org".

`data/params.json` (committed, embedded in the game by the `host` crate) holds only the league-wide aggregates derived from those files: rates, talent spreads, correlations and run environment, 6 KB, no player, team or game. The raw files stay in `data/internal/` (git-ignored). The notice above must be shown in the game's credits.

Source: https://www.retrosheet.org

## Rust crates (runtime)

| Crate | Use | License |
| --- | --- | --- |
| libm | deterministic math | MIT OR Apache-2.0 |
| rand_chacha, rand_core | seeded RNG streams | MIT OR Apache-2.0 |
| serde, serde_json | data formats | MIT OR Apache-2.0 |
| thiserror | error types | MIT OR Apache-2.0 |
| rayon (and its dependencies: crossbeam-*, either, rayon-core) | parallel days in the season simulator | MIT OR Apache-2.0 |
| rusqlite 0.40 (feature `bundled`, builds SQLite from C source with `cc`) and its dependencies (libsqlite3-sys, fallible-iterator, hashlink, ...) | save files (`storage`) | MIT (SQLite itself: public domain) |
| sha2 0.11 (and digest, crypto-common, ...) | save checksums | MIT OR Apache-2.0 |
| eframe, egui, egui_extras 0.36 (and their dependencies: winit, glow, glutin, epaint, ...) | desktop window and widgets (`ui`) | MIT OR Apache-2.0 (transitive licences audited with `tools/licenses.py`, see below) |
| tempfile (dev only) | tests | MIT OR Apache-2.0 |

Licenses above are the crates' published licenses; re-check with `cargo deny` before any release.

## Licence audit (2026-10-11, Phase 6)

`python3 tools/licenses.py` lists the licence of each external crate in `Cargo.lock` (455 crates at this date) and fails on a crate with no permissive option. Result: every crate offers MIT, Apache-2.0, BSD, ISC, Zlib, Unlicense, Unicode-3.0 or BSL-1.0 terms. Two crates (three `Cargo.lock` entries) are dual-licensed with a copyleft alternative; we use them under the permissive option: `r-efi` (two versions; MIT OR Apache-2.0 OR LGPL-2.1-or-later), `self_cell` (Apache-2.0 OR GPL-2.0-only). No crate is used under GPL/LGPL terms.

Fonts: `epaint` (via egui) embeds default fonts under the SIL OFL 1.1 and the Ubuntu Font Licence (crate licence expression `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`). Their notices are in the Credits screen.

`cargo deny` is not run (it needs a `deny.toml`, which the owner has not written). Re-run the script before every release; it was last run on 2026-10-12 (455 crates, all with a permissive option). Every direct dependency of every crate in the workspace is in the table above; check the table against `crates/*/Cargo.toml` when a dependency is added.

## Reviewed free-agent price benchmark (2026-10-13; unmerged experiment)

Matt Swartz, FanGraphs, "The Linearity of Cost per Win" (2017-07-14), reports
about $9M per WAR for the 2014-16 free-agent market, net of minimum salary.
This is the approximate anchor for the unmerged price experiment described in
`ISSUES.md` section 6. Original code at e232181 still defaults to 4.4; this
entry documents the reviewed source and does not adopt the candidate.
The simulator uses `max(price * WAR, minimum)`, so the scalar does not exactly
replicate that net-of-minimum estimate. It is not total payroll per standings
win or a verified current-market price. No player data or dependency was added.

Source: https://blogs.fangraphs.com/the-linearity-of-cost-per-win/
