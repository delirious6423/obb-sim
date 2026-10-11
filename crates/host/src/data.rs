//! The game's data. League-wide rates and talent spreads were derived from
//! Retrosheet 2015-2019 event files (see THIRD_PARTY.md); the file holds
//! only those aggregates: no player, team or game.

use calibration::Calibration;

const PARAMS: &str = include_str!("../../../data/params.json");

/// The calibration the game ships with.
///
/// # Panics
/// Never in a released build: the embedded file is checked by a test.
#[must_use]
pub fn calibration() -> Calibration {
    Calibration::from_json(PARAMS).expect("the embedded calibration is valid")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_embedded_calibration_loads() {
        let c = super::calibration();
        assert!(c.pa > 100_000);
    }
}
