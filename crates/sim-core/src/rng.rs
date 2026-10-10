use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// The random stream for a single game.
///
/// Every game gets its own ChaCha stream, derived from the league seed and the
/// game id. A game's dice therefore never depend on which other games ran
/// before it, or on how they were ordered.
#[derive(Debug, Clone)]
pub struct GameRng {
    inner: ChaCha8Rng,
}

impl GameRng {
    #[must_use]
    pub fn for_game(league_seed: u64, game_id: u64) -> Self {
        let mut inner = ChaCha8Rng::seed_from_u64(league_seed);
        inner.set_stream(game_id);
        Self { inner }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.inner.next_u64()
    }

    /// Uniform in `[0, 1)` with 53 bits of precision.
    pub fn next_f64(&mut self) -> f64 {
        const SCALE: f64 = 1.0 / (1u64 << 53) as f64;
        (self.inner.next_u64() >> 11) as f64 * SCALE
    }

    /// Poisson-distributed count (Knuth's method). `lambda` must be small
    /// (below ~30); baseball run rates are far lower.
    pub fn poisson(&mut self, lambda: f64) -> u32 {
        let limit = libm::exp(-lambda);
        let mut count = 0;
        let mut product = self.next_f64();
        while product > limit {
            count += 1;
            product *= self.next_f64();
        }
        count
    }
}
