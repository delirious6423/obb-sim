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
