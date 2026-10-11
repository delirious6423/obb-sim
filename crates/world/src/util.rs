//! Small deterministic helpers: stream seeding and hash-based noise.

use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Mix several numbers into one 64-bit seed.
#[must_use]
pub fn mix(parts: &[u64]) -> u64 {
    let mut h = 0x243F_6A88_85A3_08D3u64;
    for p in parts {
        h = splitmix(h ^ splitmix(*p));
    }
    h
}

/// A random stream named by a tag and some numbers; independent of every other.
#[must_use]
pub fn stream(seed: u64, tag: u64, a: u64, b: u64) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(mix(&[seed, tag, a, b]))
}

/// Standard normal noise fixed by its inputs (no state, no stream).
#[must_use]
pub fn noise(seed: u64, a: u64, b: u64, c: u64) -> f64 {
    let h1 = mix(&[seed, a, b, c, 1]);
    let h2 = mix(&[seed, a, b, c, 2]);
    let u1 = ((h1 >> 11) as f64 / (1u64 << 53) as f64).max(1e-300);
    let u2 = (h2 >> 11) as f64 / (1u64 << 53) as f64;
    libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(2.0 * core::f64::consts::PI * u2)
}

/// Stream tags, so no two uses ever share a stream.
pub mod tag {
    pub const GENERATE: u64 = 1;
    pub const PROGRESS: u64 = 2;
    pub const INJURY: u64 = 3;
    pub const DRAFT_CLASS: u64 = 4;
    pub const ORDER: u64 = 5;
    pub const SCOUT: u64 = 6;
    pub const MARKET: u64 = 7;
    pub const FOUND: u64 = 8;
    pub const SEASON: u64 = 9;
}
