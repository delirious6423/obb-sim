//! Phase 1 exit gate. Ignored by default because it needs the git-ignored
//! `data/internal/params.json` and takes a while:
//!
//!     cargo test --release -- --ignored calibration
//!
//! Passes only if league K%, BB% and HR per plate appearance are each within
//! +-3% (relative) of the 2015-2019 target.

use std::path::Path;

use calibration::{Calibration, Report, simulate};
use sim_core::Outcome;

const GAMES: u64 = 100_000;
const TOLERANCE: f64 = 0.03;
const SEED: u64 = 20_240_601;

#[test]
#[ignore = "needs data/internal/params.json; run with --release -- --ignored calibration"]
fn calibration_gate() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/internal/params.json");
    let cal = Calibration::load(&path).unwrap_or_else(|e| panic!("{e}"));
    let totals = simulate(&cal, GAMES, SEED);
    let report = Report::new(
        &cal,
        totals,
        vec![Outcome::Strikeout, Outcome::Walk, Outcome::HomeRun],
        TOLERANCE,
    );
    println!(
        "\nTarget: {} ({} PA)\n{}",
        cal.source,
        cal.pa,
        report.table()
    );
    assert!(
        report.passed(),
        "K%, BB% and HR/PA must each be within +-3% relative of the target"
    );
}
