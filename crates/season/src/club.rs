use calibration::{Talent, generate, teams_by_starter};
use sim_core::{Rates, Team};

/// A club: a name and its roster as one playable [`Team`] per rotation slot
/// (the same players with a different starter).
#[derive(Debug, Clone, PartialEq)]
pub struct Club {
    pub name: String,
    pub rotation: Vec<Team>,
}

impl Club {
    /// Pitchers on the staff: the rotation plus the bullpen.
    #[must_use]
    pub fn staff_size(&self) -> usize {
        let t = &self.rotation[0];
        self.rotation.len() + t.pitchers().len() - 1
    }

    /// Index into the staff of the pitcher at `index` in the variant whose
    /// starter is `starter`: starters come first, then the bullpen in order.
    #[must_use]
    pub fn staff_index(&self, starter: usize, index: usize) -> usize {
        if index == 0 {
            starter
        } else {
            self.rotation.len() + index - 1
        }
    }

    #[must_use]
    pub fn pitcher_name(&self, staff: usize) -> &str {
        let n = self.rotation.len();
        if staff < n {
            &self.rotation[staff].pitchers()[0].name
        } else {
            &self.rotation[0].pitchers()[staff - n + 1].name
        }
    }
}

/// `teams` invented clubs whose players average out to the league.
#[must_use]
pub fn fictional_league(league: &Rates, talent: &Talent, teams: usize, seed: u64) -> Vec<Club> {
    generate(league, talent, teams, seed)
        .iter()
        .map(|spec| Club {
            name: spec.name.clone(),
            rotation: teams_by_starter(spec),
        })
        .collect()
}
