use rayon::prelude::*;
use sim_core::{Baseline, Env, GameView, Manager, NullSink, Team, simulate_game_managed};

/// Innings kept apart in the win-probability table; later ones share the last.
pub const INNINGS: usize = 11;
/// Lead (batting side's runs minus the other's) is clamped to `-LEAD..=LEAD`.
pub const LEAD: i32 = 7;
const LEADS: usize = (2 * LEAD + 1) as usize;

/// Raw counts, summed over games. Integers only, so any merge order gives the
/// same totals.
#[derive(Clone)]
struct Counts {
    re_runs: [[u64; 3]; 8],
    re_n: [[u64; 3]; 8],
    wp_wins: Vec<u64>,
    wp_n: Vec<u64>,
}

impl Counts {
    fn new() -> Self {
        let n = INNINGS * 2 * 3 * LEADS;
        Self {
            re_runs: [[0; 3]; 8],
            re_n: [[0; 3]; 8],
            wp_wins: vec![0; n],
            wp_n: vec![0; n],
        }
    }

    fn merge(mut self, o: &Self) -> Self {
        for b in 0..8 {
            for o_ in 0..3 {
                self.re_runs[b][o_] += o.re_runs[b][o_];
                self.re_n[b][o_] += o.re_n[b][o_];
            }
        }
        for i in 0..self.wp_n.len() {
            self.wp_wins[i] += o.wp_wins[i];
            self.wp_n[i] += o.wp_n[i];
        }
        self
    }
}

fn wp_index(inning: u8, bat: u8, outs: u8, lead: i32) -> usize {
    let inn = usize::from(inning.clamp(1, INNINGS as u8)) - 1;
    let l = (lead.clamp(-LEAD, LEAD) + LEAD) as usize;
    ((inn * 2 + usize::from(bat)) * 3 + usize::from(outs.min(2))) * LEADS + l
}

/// Watches the away side's view of every plate appearance of one game.
struct Recorder {
    seen: Vec<GameView>,
}

impl Manager for Recorder {
    fn observe(&mut self, v: &GameView) {
        self.seen.push(*v);
    }
}

fn tally(seen: &[GameView], finals: [u16; 2], c: &mut Counts) {
    let winner = usize::from(finals[1] > finals[0]);
    let mut start = 0;
    while start < seen.len() {
        let (inn, bat) = (seen[start].inning, seen[start].bat);
        let mut end = start;
        while end < seen.len() && seen[end].inning == inn && seen[end].bat == bat {
            end += 1;
        }
        let b = usize::from(bat);
        let after = if end < seen.len() {
            seen[end].runs[b]
        } else {
            finals[b]
        };
        for v in &seen[start..end] {
            let scored = u64::from(after - v.runs[b]);
            let (s, o) = (v.base_state(), usize::from(v.outs.min(2)));
            c.re_runs[s][o] += scored;
            c.re_n[s][o] += 1;
            let i = wp_index(v.inning, v.bat, v.outs, v.bat_lead());
            c.wp_n[i] += 1;
            c.wp_wins[i] += u64::from(winner == b);
        }
        start = end;
    }
}

/// Run expectancy and win probability measured from simulated games.
#[derive(Debug, Clone, PartialEq)]
pub struct Tables {
    /// Expected runs to the end of the half-inning, by base mask and outs.
    re: [[f64; 3]; 8],
    /// Probability the side at bat wins, by inning, side, outs and lead.
    wp: Vec<f64>,
    /// Mean win-probability swing of one run, over all plate appearances.
    mean_swing: f64,
    /// League stolen-base success, to second and to third.
    steal_success: [f64; 2],
    /// Games the tables were built from.
    pub games: u32,
}

impl Tables {
    /// Play `games` games between random pairings of `teams` and count.
    ///
    /// # Panics
    /// With fewer than two teams.
    #[must_use]
    pub fn build(env: &Env, teams: &[Team], games: u32, seed: u64) -> Self {
        assert!(teams.len() >= 2, "need two teams to play");
        let n = teams.len() as u64;
        let counts = (0..games)
            .into_par_iter()
            .fold(Counts::new, |mut c, g| {
                let h = u64::from(g) % n;
                let a = (h + 1 + (u64::from(g) / n) % (n - 1)) % n;
                let mut rec = Recorder { seen: Vec::new() };
                let mut base = Baseline;
                let out = simulate_game_managed(
                    env,
                    &teams[a as usize],
                    &teams[h as usize],
                    seed,
                    u64::from(g),
                    &mut NullSink,
                    [&mut rec, &mut base],
                );
                tally(&rec.seen, out.runs, &mut c);
                c
            })
            .reduce(Counts::new, |a, b| a.merge(&b));
        Self::from_counts(&counts, env, games)
    }

    fn from_counts(c: &Counts, env: &Env, games: u32) -> Self {
        let mut re = [[0.0; 3]; 8];
        for (b, row) in re.iter_mut().enumerate() {
            for (o, cell) in row.iter_mut().enumerate() {
                if c.re_n[b][o] > 0 {
                    *cell = c.re_runs[b][o] as f64 / c.re_n[b][o] as f64;
                }
            }
        }
        // A weak logistic prior keeps thin cells sensible.
        let prior = 3.0;
        let wp: Vec<f64> = (0..c.wp_n.len())
            .map(|i| {
                let lead = (i % LEADS) as f64 - f64::from(LEAD);
                let p0 = 1.0 / (1.0 + libm::exp(-0.7 * lead));
                (c.wp_wins[i] as f64 + prior * p0) / (c.wp_n[i] as f64 + prior)
            })
            .collect();
        let mut t = Self {
            re,
            wp,
            mean_swing: 1.0,
            steal_success: [
                env.params.running.sb2_success,
                env.params.running.sb3_success,
            ],
            games,
        };
        let (mut sum, mut n) = (0.0, 0.0);
        for i in 0..c.wp_n.len() {
            let w = c.wp_n[i] as f64;
            if w > 0.0 {
                let lead = (i % LEADS) as i32 - LEAD;
                let rest = i - i % LEADS;
                sum += w * t.swing_at(rest, lead);
                n += w;
            }
        }
        if n > 0.0 && sum > 0.0 {
            t.mean_swing = sum / n;
        }
        t
    }

    fn swing_at(&self, rest: usize, lead: i32) -> f64 {
        let at = |l: i32| self.wp[rest + (l.clamp(-LEAD, LEAD) + LEAD) as usize];
        (at(lead + 1) - at(lead - 1)).abs() / 2.0
    }

    /// Expected runs for the rest of the half-inning.
    #[must_use]
    pub fn run_expectancy(&self, bases: usize, outs: u8) -> f64 {
        if outs >= 3 {
            0.0
        } else {
            self.re[bases & 7][usize::from(outs)]
        }
    }

    /// Chance the side at bat wins from this state.
    #[must_use]
    pub fn win_probability(&self, v: &GameView) -> f64 {
        self.wp[wp_index(v.inning, v.bat, v.outs, v.bat_lead())]
    }

    /// How much one run matters now, against the average plate appearance
    /// (1.0 = typical; a tie game in the ninth is several times that).
    #[must_use]
    pub fn leverage(&self, v: &GameView) -> f64 {
        let rest = wp_index(v.inning, v.bat, v.outs, 0) - (LEAD as usize);
        self.swing_at(rest, v.bat_lead()) / self.mean_swing
    }

    /// Runs gained by sending a runner from base `from` (1 or 2) if he is safe
    /// with probability `p`, against holding him. `None` when the next base is
    /// taken.
    #[must_use]
    pub fn steal_gain(&self, v: &GameView, from: u8, p: f64) -> Option<f64> {
        let [r1, r2, r3] = v.bases;
        let (mask_ok, mask_out) = match from {
            1 if !r2 => (0b010 | usize::from(r3) << 2, usize::from(r3) << 2),
            2 if !r3 => (0b100 | usize::from(r1), usize::from(r1)),
            _ => return None,
        };
        let now = self.run_expectancy(v.base_state(), v.outs);
        let ok = self.run_expectancy(mask_ok, v.outs);
        let out = self.run_expectancy(mask_out, v.outs + 1);
        Some(p * ok + (1.0 - p) * out - now)
    }

    /// League stolen-base success to second (`from` 1) or third (`from` 2).
    #[must_use]
    pub fn steal_success(&self, from: u8) -> f64 {
        self.steal_success[usize::from(from.saturating_sub(1).min(1))]
    }
}
