# Third-party notices

Every dependency and data source is logged here.

## Data

### Retrosheet (internal calibration only)

Play-by-play event files for 2015-2019 are downloaded into `data/internal/`, which is git-ignored. Only league-wide rates and talent spread are derived from them; no player id, name or stat line is stored.

Required notice, from Retrosheet:

> The information used here was obtained free of charge from and is copyrighted by Retrosheet. Interested parties may contact Retrosheet at "www.retrosheet.org".

Source: https://www.retrosheet.org

## Rust crates (runtime)

| Crate | Use | License |
| --- | --- | --- |
| libm | deterministic math | MIT OR Apache-2.0 |
| rand_chacha, rand_core | seeded RNG streams | MIT OR Apache-2.0 |
| serde, serde_json | data formats | MIT OR Apache-2.0 |
| thiserror | error types | MIT OR Apache-2.0 |
| rayon (and its dependencies: crossbeam-*, either, rayon-core) | parallel days in the season simulator | MIT OR Apache-2.0 |

Licenses above are the crates' published licenses; re-check with `cargo deny` before any release.
