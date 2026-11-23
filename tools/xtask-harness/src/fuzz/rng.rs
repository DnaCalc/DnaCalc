//! Deterministic PRNG, IEEE-754 ordered-bit arithmetic, and canonical case hashing (`dc-ztvv.21`).
//!
//! A `(seed, GENERATOR_VERSION)` pair determines the exact initial candidate stream across all
//! platforms without external RNG crates.

use calc_verdict::{Sha256, sha256_hex};
use serde_json::Value;

/// Bumped whenever deterministic generation semantics change.
pub const GENERATOR_VERSION: &str = "dnacalc-smart-fuzz/1";

/// Deterministic SplitMix64 + Xoshiro256** PRNG.
#[derive(Clone, Debug)]
pub struct Rng {
    state: [u64; 4],
}

fn splitmix64(x: &mut u64) -> u64 {
    *x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl Rng {
    pub fn from_seed(seed: u64) -> Self {
        let mut sm = seed ^ 0x517c_c1b7_2722_0a95;
        let state = [
            splitmix64(&mut sm),
            splitmix64(&mut sm),
            splitmix64(&mut sm),
            splitmix64(&mut sm),
        ];
        Self { state }
    }

    /// Derive an independent deterministic child stream keyed by `(base_seed, tag)`.
    pub fn from_seed_and_tag(seed: u64, tag: &str) -> Self {
        let digest = sha256_hex(format!("{GENERATOR_VERSION}|{seed}|{tag}").as_bytes());
        let mut state = [0u64; 4];
        for (i, slot) in state.iter_mut().enumerate() {
            let chunk = &digest[i * 16..(i + 1) * 16];
            *slot = u64::from_str_radix(chunk, 16).unwrap_or(1);
        }
        if state == [0; 4] {
            state[0] = 1;
        }
        Self { state }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let result = self.state[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.state[1] << 17;
        self.state[2] ^= self.state[0];
        self.state[3] ^= self.state[1];
        self.state[1] ^= self.state[2];
        self.state[0] ^= self.state[3];
        self.state[2] ^= t;
        self.state[3] = self.state[3].rotate_left(45);
        result
    }

    /// Uniform in `[0.0, 1.0)`.
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / ((1u64 << 53) as f64))
    }

    /// Uniform in `[lo, hi)`.
    #[inline]
    pub fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }

    /// Uniform integer in `0..upper` (returns 0 if `upper == 0`).
    #[inline]
    pub fn gen_range(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            return 0;
        }
        let u = upper as u64;
        let zone = u64::MAX - (u64::MAX % u);
        loop {
            let v = self.next_u64();
            if v < zone {
                return (v % u) as usize;
            }
        }
    }

    /// Uniform integer in `[lo, hi]` inclusive.
    #[inline]
    pub fn gen_i64_inclusive(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        let span = (hi as i128 - lo as i128 + 1) as u128;
        if span >= u64::MAX as u128 {
            return lo;
        }
        lo + (self.gen_range(span as usize) as i64)
    }

    #[inline]
    pub fn chance(&mut self, probability: f64) -> bool {
        self.next_f64() < probability
    }

    #[inline]
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.gen_range(items.len())]
    }

    /// Random finite normal binary64 with binary exponent in `[exp_min, exp_max]` inclusive
    /// (clamped to `[-1022, 1023]`) and optional random sign.
    pub fn log_uniform(&mut self, exp_min: i32, exp_max: i32, allow_negative: bool) -> f64 {
        let lo = exp_min.clamp(-1022, 1023);
        let hi = exp_max.clamp(lo, 1023);
        let exp = self.gen_i64_inclusive(lo as i64, hi as i64) as i32;
        let biased = (exp + 1023) as u64;
        let mantissa = self.next_u64() & ((1u64 << 52) - 1);
        let sign = if allow_negative && (self.next_u64() & 1) != 0 {
            1u64 << 63
        } else {
            0
        };
        f64::from_bits(sign | (biased << 52) | mantissa)
    }
}

/// Canonical hex encoding of a finite binary64 for WitnessSet JSON arguments.
/// Normalizes `-0.0` to `+0.0` unless explicitly preserved via [`hex_bits_raw`].
pub fn hex_f64(value: f64) -> Value {
    let bits = if value == 0.0 { 0u64 } else { value.to_bits() };
    Value::String(format!("0x{bits:016x}"))
}

/// Raw bit encoding (preserves `-0.0` or subnormals when explicitly desired).
#[allow(dead_code)]
pub fn hex_bits_raw(bits: u64) -> Value {
    Value::String(format!("0x{bits:016x}"))
}

/// Parse a WitnessSet hex double `"0x..."` into its raw `u64` bits.
pub fn parse_hex_bits(value: &Value) -> Option<u64> {
    let hex = value.as_str()?.strip_prefix("0x")?;
    if hex.len() != 16 {
        return None;
    }
    u64::from_str_radix(hex, 16).ok()
}

/// Total order-preserving bijection from finite IEEE-754 `f64` to `u64`.
/// `-f64::MAX < ... < -0.0 < +0.0 < ... < +f64::MAX`.
#[inline]
pub fn f64_to_ordered(value: f64) -> u64 {
    let bits = value.to_bits();
    if (bits >> 63) != 0 {
        !bits
    } else {
        bits ^ (1u64 << 63)
    }
}

/// Inverse of [`f64_to_ordered`].
#[inline]
pub fn ordered_to_f64(ordered: u64) -> f64 {
    let bits = if (ordered >> 63) != 0 {
        ordered ^ (1u64 << 63)
    } else {
        !ordered
    };
    f64::from_bits(bits)
}

/// Ordered coordinate of `+f64::MIN_POSITIVE` and `-f64::MIN_POSITIVE`.
pub fn min_pos_ordered() -> u64 {
    f64_to_ordered(f64::MIN_POSITIVE)
}
pub fn min_neg_ordered() -> u64 {
    f64_to_ordered(-f64::MIN_POSITIVE)
}
pub fn max_pos_ordered() -> u64 {
    f64_to_ordered(f64::MAX)
}
pub fn max_neg_ordered() -> u64 {
    f64_to_ordered(-f64::MAX)
}

/// Whether `value` survives Excel's `Value2` cell write without modification
/// (finite, non-subnormal, not `-0.0`).
#[inline]
pub fn is_value2_exact(value: f64) -> bool {
    value.is_finite()
        && !value.is_subnormal()
        && !(value == 0.0 && value.is_sign_negative())
}

/// Step a finite double by `delta` ULPs in IEEE-754 total order.
/// When `value2_safe` is true, skips subnormals and `-0.0` (so `±MIN_POSITIVE` steps to `0.0`
/// and `0.0` steps to `±MIN_POSITIVE`), ensuring Excel `Value2` round-trips identically.
pub fn step_ulps(value: f64, delta: i64, value2_safe: bool) -> Option<f64> {
    if !value.is_finite() {
        return None;
    }
    if delta == 0 {
        return Some(if value2_safe && value == 0.0 { 0.0 } else { value });
    }
    if !value2_safe {
        let ord = f64_to_ordered(value) as i128 + delta as i128;
        if ord < max_neg_ordered() as i128 || ord > max_pos_ordered() as i128 {
            return None;
        }
        let out = ordered_to_f64(ord as u64);
        return out.is_finite().then_some(out);
    }
    // Compress the subnormal gap (-MIN_POSITIVE, +MIN_POSITIVE) to a single point (0.0).
    let pos_min = min_pos_ordered();
    let neg_min = min_neg_ordered();
    let to_compact = |v: f64| -> i128 {
        if v == 0.0 || v.is_subnormal() {
            0
        } else if v > 0.0 {
            (f64_to_ordered(v) - pos_min) as i128 + 1
        } else {
            -((neg_min - f64_to_ordered(v)) as i128 + 1)
        }
    };
    let from_compact = |c: i128| -> Option<f64> {
        if c == 0 {
            Some(0.0)
        } else if c > 0 {
            let ord = pos_min.checked_add((c - 1) as u64)?;
            (ord <= max_pos_ordered()).then(|| ordered_to_f64(ord))
        } else {
            let dist = (-c - 1) as u64;
            let ord = neg_min.checked_sub(dist)?;
            (ord >= max_neg_ordered()).then(|| ordered_to_f64(ord))
        }
    };
    from_compact(to_compact(value) + delta as i128)
}

/// Canonical deduplication key for one function call.
pub fn canonical_case_key(function: &str, args: &[Value]) -> String {
    format!("{function}|{}", Value::Array(args.to_vec()))
}

/// Incremental SHA-256 hasher over a stream of `(region, args)` pairs (identical digest to
/// [`hash_case_stream`], without holding all cases in memory).
pub struct StreamCaseHasher {
    sha: Sha256,
}

impl StreamCaseHasher {
    pub fn new(function: &str) -> Self {
        let mut sha = Sha256::new();
        sha.update(GENERATOR_VERSION.as_bytes());
        sha.update(b"\n");
        sha.update(function.as_bytes());
        sha.update(b"\n");
        Self { sha }
    }

    pub fn update_case(&mut self, region: &str, args: &[Value]) {
        self.sha.update(region.as_bytes());
        self.sha.update(b"\t");
        let text = serde_json::to_string(&Value::Array(args.to_vec())).unwrap_or_default();
        self.sha.update(text.as_bytes());
        self.sha.update(b"\n");
    }

    pub fn finish_hex(self) -> String {
        self.sha.finish_hex()
    }
}

/// Canonical SHA-256 over a sequence of `(region, args)` pairs.
pub fn hash_case_stream<'a>(
    function: &str,
    items: impl IntoIterator<Item = (&'a str, &'a [Value])>,
) -> String {
    let mut hasher = StreamCaseHasher::new(function);
    for (region, args) in items {
        hasher.update_case(region, args);
    }
    hasher.finish_hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_f64_bijection_is_strictly_monotone() {
        let samples = [
            -f64::MAX,
            -1e100,
            -1.0,
            -f64::MIN_POSITIVE,
            -0.0,
            0.0,
            f64::MIN_POSITIVE,
            1.0,
            1e100,
            f64::MAX,
        ];
        for pair in samples.windows(2) {
            assert!(
                f64_to_ordered(pair[0]) < f64_to_ordered(pair[1]),
                "{:?} vs {:?}",
                pair[0],
                pair[1]
            );
            assert_eq!(
                ordered_to_f64(f64_to_ordered(pair[0])).to_bits(),
                pair[0].to_bits()
            );
        }
    }

    #[test]
    fn value2_safe_step_ulps_skips_subnormals_cleanly() {
        assert_eq!(step_ulps(0.0, 1, true), Some(f64::MIN_POSITIVE));
        assert_eq!(step_ulps(0.0, -1, true), Some(-f64::MIN_POSITIVE));
        assert_eq!(step_ulps(f64::MIN_POSITIVE, -1, true), Some(0.0));
        assert_eq!(step_ulps(f64::MIN_POSITIVE, -2, true), Some(-f64::MIN_POSITIVE));
        assert_eq!(step_ulps(1.0, 1, true), Some(f64::from_bits(0x3ff0_0000_0000_0001)));
        assert_eq!(step_ulps(f64::MAX, 1, true), None);
    }

    #[test]
    fn rng_seed_and_tag_are_deterministic() {
        let mut a = Rng::from_seed_and_tag(42, "RATE/region");
        let mut b = Rng::from_seed_and_tag(42, "RATE/region");
        let mut c = Rng::from_seed_and_tag(43, "RATE/region");
        for _ in 0..32 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(Rng::from_seed_and_tag(42, "RATE/region").next_u64(), c.next_u64());
    }
}
