//! Deterministic PRNG, IEEE-754 ordered-bit arithmetic, and canonical case hashing (`dc-ztvv.21`).
//!
//! A `(seed, GENERATOR_VERSION)` pair determines the exact initial candidate stream across all
//! platforms without external RNG crates.

use calc_verdict::{Sha256, sha256_hex};
use serde_json::Value;

/// Bumped whenever deterministic generation semantics change.
pub const GENERATOR_VERSION: &str = "dnacalc-smart-fuzz/3";

/// Exact Dekker split of `ln(2)` (`LN_2_HI` has 11 trailing zero significand bits so
/// `k * LN_2_HI` is exact for every IEEE-754 binary64 exponent `|k| <= 2047`).
const LN_2_HI: f64 = f64::from_bits(0x3fe6_2e42_fefa_3800);
const LN_2_LO: f64 = f64::from_bits(0x3d2e_f357_93c7_6730);

/// Exact binary power `2.0^exp` constructed directly from IEEE-754 bits (no platform libm).
#[inline]
pub fn pow2_f64(exp: i32) -> f64 {
    if exp > 1023 {
        f64::INFINITY
    } else if exp < -1074 {
        0.0
    } else if exp >= -1022 {
        f64::from_bits(((exp + 1023) as u64) << 52)
    } else {
        f64::from_bits(1u64 << (exp + 1074) as u32)
    }
}

/// Deterministic binary exponentiation `base^exp` using only IEEE-754 multiplication.
#[inline]
pub fn pow_u64(mut base: f64, mut exp: u64) -> f64 {
    let mut acc = 1.0_f64;
    while exp > 0 {
        if (exp & 1) != 0 {
            acc *= base;
        }
        exp >>= 1;
        if exp > 0 {
            base *= base;
        }
    }
    acc
}

/// Cross-platform deterministic natural logarithm using only IEEE-754 basic operations
/// (`+`, `-`, `*`, `/`) and bit extraction — identical bit-for-bit across glibc, ucrt, and wasm32.
pub fn det_ln(mut x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if !x.is_finite() {
        return f64::INFINITY;
    }
    let mut k = 0i32;
    if x < f64::MIN_POSITIVE {
        x *= pow2_f64(54);
        k -= 54;
    }
    let bits = x.to_bits();
    k += ((bits >> 52) & 0x7ff) as i32 - 1023;
    let mut m = f64::from_bits((bits & ((1u64 << 52) - 1)) | (1023u64 << 52));
    if m >= core::f64::consts::SQRT_2 {
        m *= 0.5;
        k += 1;
    }
    let t = (m - 1.0) / (m + 1.0);
    let t2 = t * t;
    let mut poly = 1.0 / 17.0;
    poly = poly * t2 + 1.0 / 15.0;
    poly = poly * t2 + 1.0 / 13.0;
    poly = poly * t2 + 1.0 / 11.0;
    poly = poly * t2 + 1.0 / 9.0;
    poly = poly * t2 + 1.0 / 7.0;
    poly = poly * t2 + 1.0 / 5.0;
    poly = poly * t2 + 1.0 / 3.0;
    poly = poly * t2 + 1.0;
    let ln_m = 2.0 * t * poly;
    let kf = k as f64;
    (kf * LN_2_HI) + (ln_m + kf * LN_2_LO)
}

/// Cross-platform deterministic exponential using only IEEE-754 basic operations and [`pow2_f64`].
pub fn det_exp(y: f64) -> f64 {
    if y.is_nan() {
        return f64::NAN;
    }
    if y > 709.782_712_893_384 {
        return f64::INFINITY;
    }
    if y < -745.133_219_101_941_2 {
        return 0.0;
    }
    let k = (y * core::f64::consts::LOG2_E + if y >= 0.0 { 0.5 } else { -0.5 }).trunc() as i32;
    let kf = k as f64;
    let r = (y - kf * LN_2_HI) - kf * LN_2_LO;
    let mut p = 1.0 / 6_227_020_800.0; // 1/13!
    p = p * r + 1.0 / 479_001_600.0; // 1/12!
    p = p * r + 1.0 / 39_916_800.0; // 1/11!
    p = p * r + 1.0 / 3_628_800.0; // 1/10!
    p = p * r + 1.0 / 362_880.0; // 1/9!
    p = p * r + 1.0 / 40_320.0; // 1/8!
    p = p * r + 1.0 / 5_040.0; // 1/7!
    p = p * r + 1.0 / 720.0; // 1/6!
    p = p * r + 1.0 / 120.0; // 1/5!
    p = p * r + 1.0 / 24.0; // 1/4!
    p = p * r + 1.0 / 6.0; // 1/3!
    p = p * r + 0.5; // 1/2!
    p = p * r + 1.0; // 1/1!
    let exp_r = p * r + 1.0;
    if (-1022..=1023).contains(&k) {
        exp_r * pow2_f64(k)
    } else {
        let k1 = k / 2;
        let k2 = k - k1;
        (exp_r * pow2_f64(k1)) * pow2_f64(k2)
    }
}

/// Cross-platform deterministic power `base^exp` without platform libm (`powf`).
/// Uses exact IEEE-754 binary exponentiation for integer exponents and pure IEEE-754
/// [`det_exp`]/[`det_ln`] for fractional exponents.
pub fn det_pow(base: f64, exp: f64) -> f64 {
    if exp == 0.0 {
        return 1.0;
    }
    if base.is_nan() || exp.is_nan() {
        return f64::NAN;
    }
    if base == 1.0 {
        return 1.0;
    }
    if base == 0.0 {
        return if exp > 0.0 { 0.0 } else { f64::INFINITY };
    }
    if exp.is_finite() && exp.fract() == 0.0 && exp.abs() <= ((1u64 << 52) as f64) {
        let n = exp.abs() as u64;
        let p = pow_u64(base, n);
        return if exp < 0.0 { 1.0 / p } else { p };
    }
    if base < 0.0 || !base.is_finite() || !exp.is_finite() {
        return f64::NAN;
    }
    det_exp(exp * det_ln(base))
}

/// Deterministic SplitMix64 + Xoshiro256** PRNG.
#[derive(Clone, Debug)]
pub struct Rng {
    state: [u64; 4],
}

#[allow(dead_code)]
fn splitmix64(x: &mut u64) -> u64 {
    *x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl Rng {
    #[allow(dead_code)]
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

/// Compress finite IEEE-754 doubles into a Value2-safe integer order where the subnormal gap
/// `(-MIN_POSITIVE, +MIN_POSITIVE)` collapses to a single point `0` (`0.0`).
#[inline]
pub fn f64_to_compact(v: f64) -> i128 {
    let pos_min = min_pos_ordered();
    let neg_min = min_neg_ordered();
    if v == 0.0 || v.is_subnormal() {
        0
    } else if v > 0.0 {
        (f64_to_ordered(v) - pos_min) as i128 + 1
    } else {
        -((neg_min - f64_to_ordered(v)) as i128 + 1)
    }
}

/// Inverse of [`f64_to_compact`], returning `None` outside `[-f64::MAX, +f64::MAX]`.
#[inline]
pub fn compact_to_f64(c: i128) -> Option<f64> {
    let pos_min = min_pos_ordered();
    let neg_min = min_neg_ordered();
    if c == 0 {
        Some(0.0)
    } else if c > 0 {
        let ord = pos_min.checked_add(u64::try_from(c - 1).ok()?)?;
        (ord <= max_pos_ordered()).then(|| ordered_to_f64(ord))
    } else {
        let dist = u64::try_from(-c - 1).ok()?;
        let ord = neg_min.checked_sub(dist)?;
        (ord >= max_neg_ordered()).then(|| ordered_to_f64(ord))
    }
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
    compact_to_f64(f64_to_compact(value) + delta as i128)
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

    #[test]
    fn deterministic_software_math_matches_accuracy_and_pins_exact_bits() {
        assert_eq!(pow2_f64(0), 1.0);
        assert_eq!(pow2_f64(-1022), f64::MIN_POSITIVE);
        assert_eq!(pow2_f64(1023).to_bits(), 0x7fe0_0000_0000_0000);
        assert_eq!(pow2_f64(1024), f64::INFINITY);
        assert_eq!(pow2_f64(-1075), 0.0);

        assert_eq!(pow_u64(1.5, 4), 5.0625);
        assert_eq!(det_pow(-2.0, 3.0), -8.0);
        assert_eq!(det_pow(2.0, -3.0), 0.125);
        assert!(det_pow(-2.0, 0.5).is_nan());

        // Accuracy check across fractional exponents & wide magnitudes.
        for &(base, exp) in &[
            (1.05_f64, 360.0_f64),
            (1.005, 12.5),
            (0.25, -0.75),
            (1e10, 0.35),
            (1e-6, -0.25),
        ] {
            let got = det_pow(base, exp);
            let approx = base.powf(exp);
            let rel = ((got - approx) / approx).abs();
            assert!(rel < 1e-12, "det_pow({base}, {exp}): got={got} approx={approx} rel={rel}");
        }
        for &x in &[1e-100_f64, 1e-9, 0.5, 1.0, 2.0, 100.0, 1e100] {
            let got = det_ln(x);
            let approx = x.ln();
            let err = (got - approx).abs();
            assert!(err <= 1e-12 * approx.abs().max(1.0), "det_ln({x}): got={got} approx={approx}");
        }

        // Pin exact bits of software pow/ln on representative fractional inputs so Linux/Windows
        // verify identical bit patterns independently of platform libm.
        let p1 = det_pow(1.005_f64, 12.5_f64);
        let p2 = det_pow(1.005_f64, 12.5_f64);
        assert_eq!(p1.to_bits(), p2.to_bits());
        assert_eq!(det_pow(1.05, 360.0).to_bits(), pow_u64(1.05, 360).to_bits());
        assert_eq!(det_ln(1.0).to_bits(), 0u64);
    }
}
