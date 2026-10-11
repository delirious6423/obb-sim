# Third-party notices

Every dependency and data source is logged here.

## Data

### Retrosheet (internal calibration only)

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
| eframe, egui, egui_extras 0.36 (and their dependencies: winit, glow, glutin, epaint, ...) | desktop window and widgets (`ui`) | MIT OR Apache-2.0 (transitive licenses to be audited in Phase 6 with `cargo deny`) |
| tempfile (dev only) | tests | MIT OR Apache-2.0 |

Licenses above are the crates' published licenses; re-check with `cargo deny` before any release.

## Licence audit (2026-10-11, Phase 6)

`python3 tools/licenses.py` lists the licence of each external crate in `Cargo.lock` (455 crates at this date) and fails on a crate with no permissive option. Result: every crate offers MIT, Apache-2.0, BSD, ISC, Zlib, Unlicense, Unicode-3.0 or BSL-1.0 terms. Three crates are dual-licensed with a copyleft alternative; we use them under the permissive option: `r-efi` (MIT OR Apache-2.0 OR LGPL-2.1-or-later), `self_cell` (Apache-2.0 OR GPL-2.0-only). No crate is used under GPL/LGPL terms.

Fonts: `epaint` (via egui) embeds default fonts under the SIL OFL 1.1 and the Ubuntu Font Licence (crate licence expression `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`). Their notices are in the Credits screen.

`cargo deny` is not run (not installed; no global installs). Re-run the script before every release.
