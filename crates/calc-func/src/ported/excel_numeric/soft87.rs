//! The x87 in software (dc-8dg): the 80-bit extended format and the x87 operations Excel's CRT
//! sequences reach, in portable Rust, so the chains in [`super`] give the same bits on wasm32 as
//! the hardware gives on x86_64.
//!
//! # What is emulated
//! The extended format (64-bit significand with its explicit integer bit, 15-bit exponent) and,
//! under any control word (precision 24/53/64, the four rounding modes):
//! * `FADD`/`FSUB`/`FMUL`/`FDIV`/`FSQRT`/`FRNDINT`/`FSCALE`/`FPREM1`, exactly as IEEE 754 defines
//!   them (one rounding of the exact result to the precision field);
//! * the binary64 store (`FSTP qword`), including the subnormal range and overflow;
//! * the ROM constants `FLDPI`, `FLDL2E`, `FLDL2T`, `FLDLN2`, `FLDLG2` and `FLD1`;
//! * the transcendental instructions `F2XM1`, `FYL2X`, `FPATAN`, `FPTAN`, `FSIN` and `FCOS`, as
//!   the exact function of their extended operands rounded once to the precision field. The
//!   value is formed in a 128-bit software float (series after argument reduction) whose error
//!   is far below the 64-bit rounding step.
//!
//! The hardware transcendental instructions are microcode, not specified to round correctly; the
//! module tests measure where the exact-then-rounded form and the hardware differ, operation by
//! operation and chain by chain (see `tests`).
//!
//! # Range
//! Results stay inside the extended exponent range in every chain Excel runs; an extended result
//! below `2^-16382` is flushed to zero (the hardware would give an extended subnormal), and
//! `FPREM1` reduces in one exact step whatever the exponent difference (the hardware reports the
//! quotient bits of its last partial step, which can differ when the difference is 64 or more).

#![allow(clippy::many_single_char_names)]

/// An x87 80-bit extended value as its memory image (`fstp tbyte`): bytes 0..8 hold the 64-bit
/// significand with its explicit integer bit, bytes 8..10 the sign bit and the 15-bit exponent
/// (bias 16383). The same image moves through the hardware (`fld tbyte`/`fstp tbyte`) unchanged.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ext80(pub [u8; 10]);

impl core::fmt::Debug for Ext80 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut hex = String::with_capacity(20);
        for b in self.0.iter().rev() {
            hex.push_str(&format!("{b:02x}"));
        }
        write!(f, "Ext80(0x{hex})")
    }
}

/// PC=64, round-to-nearest-even, all exceptions masked (Excel's control word, as measured).
pub(crate) const CW_PC64_RN: u16 = 0x133F;
/// PC=53 (double), round-to-nearest-even, all exceptions masked.
pub(crate) const CW_PC53_RN: u16 = 0x123F;
/// PC=24 (single), round-to-nearest-even, all exceptions masked.
pub(crate) const CW_PC24_RN: u16 = 0x103F;

/// The rounding-control field of a control word.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rc {
    Nearest,
    Down,
    Up,
    Zero,
}

fn rc_of(cw: u16) -> Rc {
    match (cw >> 10) & 3 {
        0 => Rc::Nearest,
        1 => Rc::Down,
        2 => Rc::Up,
        _ => Rc::Zero,
    }
}

/// The significand width the precision-control field selects.
fn precision_of(cw: u16) -> u32 {
    match (cw >> 8) & 3 {
        0 => 24,
        2 => 53,
        _ => 64,
    }
}

// ---------------------------------------------------------------------------------------------
// The format.
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    Zero,
    Finite,
    Inf,
    Nan,
}

/// An unpacked extended value: `sig * 2^(exp - 63)` with the top bit of `sig` set when finite.
#[derive(Clone, Copy, Debug)]
struct Unpacked {
    neg: bool,
    class: Class,
    exp: i32,
    sig: u64,
}

const BIAS: i32 = 16383;

fn unpack(x: &Ext80) -> Unpacked {
    let b = &x.0;
    let sig = u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
    let se = u16::from_le_bytes([b[8], b[9]]);
    let neg = se & 0x8000 != 0;
    let biased = (se & 0x7fff) as i32;
    if biased == 0x7fff {
        let class = if sig << 1 == 0 {
            Class::Inf
        } else {
            Class::Nan
        };
        return Unpacked {
            neg,
            class,
            exp: 0,
            sig,
        };
    }
    if sig == 0 {
        return Unpacked {
            neg,
            class: Class::Zero,
            exp: 0,
            sig: 0,
        };
    }
    // An extended subnormal (biased 0) has the exponent of biased 1; an unnormal is read by its
    // value as well.
    let exp0 = if biased == 0 { 1 - BIAS } else { biased - BIAS };
    let lz = sig.leading_zeros() as i32;
    Unpacked {
        neg,
        class: Class::Finite,
        exp: exp0 - lz,
        sig: sig << lz,
    }
}

fn pack_raw(neg: bool, biased: u16, sig: u64) -> Ext80 {
    let mut out = [0u8; 10];
    out[..8].copy_from_slice(&sig.to_le_bytes());
    let se = biased | if neg { 0x8000 } else { 0 };
    out[8..].copy_from_slice(&se.to_le_bytes());
    Ext80(out)
}

fn zero(neg: bool) -> Ext80 {
    pack_raw(neg, 0, 0)
}

fn inf(neg: bool) -> Ext80 {
    pack_raw(neg, 0x7fff, 0x8000_0000_0000_0000)
}

/// The x87 "real indefinite" quiet NaN, the result of an invalid operation.
fn indefinite() -> Ext80 {
    pack_raw(true, 0x7fff, 0xC000_0000_0000_0000)
}

/// Packs a finite value `sig * 2^(exp - 63)` (top bit of `sig` set), already rounded.
fn pack(neg: bool, exp: i32, sig: u64) -> Ext80 {
    let biased = exp + BIAS;
    if biased >= 0x7fff {
        return inf(neg);
    }
    if biased <= 0 {
        // Below the normal extended range: no chain reaches it (see the module docs).
        return zero(neg);
    }
    pack_raw(neg, biased as u16, sig)
}

/// Where the discarded part of a value lies relative to half a unit of the kept part.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rest {
    Zero,
    Below,
    Half,
    Above,
}

/// `v >> shift` rounded to an integer under `rc`, with `sticky` saying whether nonzero bits lie
/// below `v`. `shift` may exceed 127 (everything is discarded).
fn round_shift(v: u128, shift: u32, sticky: bool, neg: bool, rc: Rc) -> u128 {
    let (keep, rest) = if shift == 0 {
        (v, if sticky { Rest::Below } else { Rest::Zero })
    } else if shift <= 128 {
        let keep = if shift == 128 { 0 } else { v >> shift };
        let low = if shift == 128 {
            v
        } else {
            v & ((1u128 << shift) - 1)
        };
        let half = 1u128 << (shift - 1);
        let rest = if low == 0 && !sticky {
            Rest::Zero
        } else if low < half {
            Rest::Below
        } else if low == half {
            if sticky { Rest::Above } else { Rest::Half }
        } else {
            Rest::Above
        };
        (keep, rest)
    } else {
        (
            0,
            if v == 0 && !sticky {
                Rest::Zero
            } else {
                Rest::Below
            },
        )
    };
    let up = match rc {
        Rc::Nearest => rest == Rest::Above || (rest == Rest::Half && keep & 1 == 1),
        Rc::Zero => false,
        Rc::Up => rest != Rest::Zero && !neg,
        Rc::Down => rest != Rest::Zero && neg,
    };
    keep + up as u128
}

/// Rounds `m * 2^(e - 127)` (top bit of `m` set, `sticky` for nonzero bits below `m`) to `p`
/// significant bits and packs it.
fn round_pack(neg: bool, e: i32, m: u128, sticky: bool, p: u32, rc: Rc) -> Ext80 {
    debug_assert!(m >> 127 == 1);
    let mut keep = round_shift(m, 128 - p, sticky, neg, rc);
    let mut e = e;
    if keep >> p != 0 {
        keep >>= 1;
        e += 1;
    }
    pack(neg, e, (keep << (64 - p)) as u64)
}

/// Normalizes a nonzero `m * 2^(e - 127)` so the top bit of `m` is set, then rounds and packs.
fn normalize_round_pack(neg: bool, e: i32, m: u128, sticky: bool, p: u32, rc: Rc) -> Ext80 {
    let lz = m.leading_zeros();
    round_pack(neg, e - lz as i32, m << lz, sticky, p, rc)
}

// ---------------------------------------------------------------------------------------------
// Loads, stores and constants.
// ---------------------------------------------------------------------------------------------

/// `FLD qword`: exact (every binary64 is an extended value).
pub(crate) fn ext_from_f64(x: f64) -> Ext80 {
    let bits = x.to_bits();
    let neg = bits >> 63 != 0;
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & 0x000f_ffff_ffff_ffff;
    if biased == 0x7ff {
        if frac == 0 {
            return inf(neg);
        }
        // A quiet NaN keeps its payload in the top of the significand.
        return pack_raw(neg, 0x7fff, 0xC000_0000_0000_0000 | (frac << 11));
    }
    if biased == 0 {
        if frac == 0 {
            return zero(neg);
        }
        let lz = frac.leading_zeros() as i32; // >= 12
        // value = frac * 2^-1074; leading bit at 63 - lz.
        let exp = (63 - lz) - 1074;
        return pack(neg, exp, frac << lz);
    }
    pack(neg, biased - 1023, (frac | (1u64 << 52)) << 11)
}

/// `FSTP qword`: the binary64 store, rounded under the control word's rounding field (the
/// precision field does not apply to stores). Overflow and the subnormal range follow the
/// masked-exception results.
pub(crate) fn ext_to_f64(x: &Ext80, cw: u16) -> f64 {
    let u = unpack(x);
    let sign = if u.neg { 1u64 << 63 } else { 0 };
    match u.class {
        Class::Zero => return f64::from_bits(sign),
        Class::Inf => return f64::from_bits(sign | 0x7ff0_0000_0000_0000),
        Class::Nan => return f64::from_bits(sign | 0x7ff8_0000_0000_0000 | ((u.sig << 1) >> 12)),
        Class::Finite => {}
    }
    let rc = rc_of(cw);
    let overflow = || {
        let to_inf = match rc {
            Rc::Nearest => true,
            Rc::Zero => false,
            Rc::Up => !u.neg,
            Rc::Down => u.neg,
        };
        if to_inf {
            f64::from_bits(sign | 0x7ff0_0000_0000_0000)
        } else {
            f64::from_bits(sign | 0x7fef_ffff_ffff_ffff)
        }
    };
    if u.exp > 1023 {
        return overflow();
    }
    if u.exp >= -1022 {
        let mut keep = round_shift(u.sig as u128, 11, false, u.neg, rc) as u64;
        let mut e = u.exp;
        if keep >> 53 != 0 {
            keep >>= 1;
            e += 1;
        }
        if e > 1023 {
            return overflow();
        }
        return f64::from_bits(sign | (((e + 1023) as u64) << 52) | (keep & 0x000f_ffff_ffff_ffff));
    }
    // Subnormal range: units of 2^-1074. value / 2^-1074 = sig * 2^(exp + 1011).
    let shift = (-(u.exp + 1011)) as u32; // >= 12
    let keep = round_shift(u.sig as u128, shift, false, u.neg, rc) as u64;
    f64::from_bits(sign | keep)
}

/// `FLDPI`: pi rounded to the 64-bit significand.
pub(crate) fn ext_pi() -> Ext80 {
    pack(false, 1, 0xC90F_DAA2_2168_C235)
}

/// `FLDL2E`: log2(e).
pub(crate) fn ext_l2e() -> Ext80 {
    pack(false, 0, 0xB8AA_3B29_5C17_F0BC)
}

/// `FLDL2T`: log2(10).
pub(crate) fn ext_l2t() -> Ext80 {
    pack(false, 1, 0xD49A_784B_CD1B_8AFE)
}

/// `FLDLN2`: ln 2.
pub(crate) fn ext_ln2() -> Ext80 {
    pack(false, -1, 0xB172_17F7_D1CF_79AC)
}

/// `FLDLG2`: log10(2).
pub(crate) fn ext_lg2() -> Ext80 {
    pack(false, -2, 0x9A20_9A84_FBCF_F799)
}

/// `FLD1`.
pub(crate) fn ext_one() -> Ext80 {
    pack(false, 0, 0x8000_0000_0000_0000)
}

// ---------------------------------------------------------------------------------------------
// The exact operations.
// ---------------------------------------------------------------------------------------------

/// `FABS`.
pub(crate) fn ext_abs(x: &Ext80, _cw: u16) -> Ext80 {
    let mut out = *x;
    out.0[9] &= 0x7f;
    out
}

/// `FCHS`.
pub(crate) fn ext_chs(x: &Ext80, _cw: u16) -> Ext80 {
    let mut out = *x;
    out.0[9] ^= 0x80;
    out
}

/// Rounds a finite extended value to the precision field (what an operation with an exact
/// result still does under PC=53 or PC=24).
fn round_finite(u: &Unpacked, cw: u16) -> Ext80 {
    round_pack(
        u.neg,
        u.exp,
        (u.sig as u128) << 64,
        false,
        precision_of(cw),
        rc_of(cw),
    )
}

/// `FADD`: `a + b`, rounded once.
pub(crate) fn ext_add(a: &Ext80, b: &Ext80, cw: u16) -> Ext80 {
    add_unpacked(unpack(a), unpack(b), cw)
}

/// `FSUB`: `a - b`, rounded once.
pub(crate) fn ext_sub(a: &Ext80, b: &Ext80, cw: u16) -> Ext80 {
    let mut b = unpack(b);
    b.neg = !b.neg;
    add_unpacked(unpack(a), b, cw)
}

fn add_unpacked(a: Unpacked, b: Unpacked, cw: u16) -> Ext80 {
    let rc = rc_of(cw);
    match (a.class, b.class) {
        (Class::Nan, _) | (_, Class::Nan) => return indefinite(),
        (Class::Inf, Class::Inf) => {
            return if a.neg == b.neg {
                inf(a.neg)
            } else {
                indefinite()
            };
        }
        (Class::Inf, _) => return inf(a.neg),
        (_, Class::Inf) => return inf(b.neg),
        (Class::Zero, Class::Zero) => {
            let neg = if a.neg == b.neg {
                a.neg
            } else {
                rc == Rc::Down
            };
            return zero(neg);
        }
        (Class::Zero, _) => return round_finite(&b, cw),
        (_, Class::Zero) => return round_finite(&a, cw),
        _ => {}
    }
    // |big| >= |small|.
    let (big, small) = if (a.exp, a.sig) >= (b.exp, b.sig) {
        (a, b)
    } else {
        (b, a)
    };
    let d = (big.exp - small.exp) as u32;
    let mb = (big.sig as u128) << 62;
    let ms0 = (small.sig as u128) << 62;
    let ms = if d == 0 {
        ms0
    } else if d >= 126 {
        1 // every bit is below the kept range: jam
    } else {
        let lost = ms0 & ((1u128 << d) - 1) != 0;
        (ms0 >> d) | lost as u128
    };
    let s = if big.neg == small.neg {
        mb + ms
    } else {
        mb - ms
    };
    if s == 0 {
        return zero(rc == Rc::Down);
    }
    // value = s * 2^(big.exp - 125)
    normalize_round_pack(big.neg, big.exp + 2, s, false, precision_of(cw), rc)
}

/// `FMUL`: `a * b`, rounded once.
pub(crate) fn ext_mul(a: &Ext80, b: &Ext80, cw: u16) -> Ext80 {
    let (a, b) = (unpack(a), unpack(b));
    let neg = a.neg != b.neg;
    match (a.class, b.class) {
        (Class::Nan, _) | (_, Class::Nan) => return indefinite(),
        (Class::Inf, Class::Zero) | (Class::Zero, Class::Inf) => return indefinite(),
        (Class::Inf, _) | (_, Class::Inf) => return inf(neg),
        (Class::Zero, _) | (_, Class::Zero) => return zero(neg),
        _ => {}
    }
    let p = (a.sig as u128) * (b.sig as u128); // in [2^126, 2^128)
    // value = p * 2^(a.exp + b.exp - 126)
    normalize_round_pack(
        neg,
        a.exp + b.exp + 1,
        p,
        false,
        precision_of(cw),
        rc_of(cw),
    )
}

/// `FDIV`: `a / b`, rounded once.
pub(crate) fn ext_div(a: &Ext80, b: &Ext80, cw: u16) -> Ext80 {
    let (a, b) = (unpack(a), unpack(b));
    let neg = a.neg != b.neg;
    match (a.class, b.class) {
        (Class::Nan, _) | (_, Class::Nan) => return indefinite(),
        (Class::Inf, Class::Inf) | (Class::Zero, Class::Zero) => return indefinite(),
        (Class::Inf, _) | (_, Class::Zero) => return inf(neg),
        (Class::Zero, _) | (_, Class::Inf) => return zero(neg),
        _ => {}
    }
    let num = (a.sig as u128) << 64;
    let den = b.sig as u128;
    let mut q = num / den;
    let mut r = num % den;
    let mut e = a.exp - b.exp; // leading-bit exponent when q has 65 bits
    if q >> 64 == 0 {
        // q has 64 bits: take one more quotient bit so a guard bit exists.
        r <<= 1;
        q <<= 1;
        if r >= den {
            r -= den;
            q |= 1;
        }
        e -= 1;
    }
    // q has 65 bits (top bit 64); place it at bit 127.
    round_pack(neg, e, q << 63, r != 0, precision_of(cw), rc_of(cw))
}

/// `floor(sqrt(n))` for a 128-bit `n`.
fn isqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }
    let mut x = (n as f64).sqrt() as u128;
    // Two Newton steps from a 50-bit estimate, then settle exactly.
    for _ in 0..2 {
        if x == 0 {
            x = 1;
        }
        x = (x + n / x) / 2;
    }
    while x.checked_mul(x).is_none_or(|sq| sq > n) {
        x -= 1;
    }
    while (x + 1).checked_mul(x + 1).is_some_and(|sq| sq <= n) {
        x += 1;
    }
    x
}

/// `FSQRT`: correctly rounded square root.
pub(crate) fn ext_sqrt(x: &Ext80, cw: u16) -> Ext80 {
    let u = unpack(x);
    match u.class {
        Class::Nan => return indefinite(),
        Class::Zero => return *x,
        Class::Inf => return if u.neg { indefinite() } else { *x },
        Class::Finite => {}
    }
    if u.neg {
        return indefinite();
    }
    // value = sig * 2^(exp - 63); scale so the exponent left over is even.
    let s = if u.exp.rem_euclid(2) == 0 { 63 } else { 64 };
    let n = (u.sig as u128) << s;
    let root = isqrt(n); // 64 bits
    let rem = n - root * root;
    // sqrt = (root + delta) * 2^((exp - 63 - s) / 2), delta in [0, 1); delta >= 1/2 iff rem > root.
    let guard: u128 = if rem == 0 {
        0
    } else if rem > root {
        3 << 62
    } else {
        1 << 62
    };
    let m = (root << 64) | guard;
    let e = (u.exp - 63 - s as i32) / 2 + 63;
    round_pack(false, e, m, false, precision_of(cw), rc_of(cw))
}

/// `FRNDINT`: round to an integer under the rounding field.
pub(crate) fn ext_rndint(x: &Ext80, cw: u16) -> Ext80 {
    let u = unpack(x);
    if u.class != Class::Finite || u.exp >= 63 {
        return *x;
    }
    let shift = (63 - u.exp) as u32;
    let k = round_shift(u.sig as u128, shift, false, u.neg, rc_of(cw));
    if k == 0 {
        return zero(u.neg);
    }
    let lz = k.leading_zeros() as i32; // k <= 2^63
    pack(u.neg, 127 - lz, (k << (lz - 64)) as u64)
}

/// `FSCALE`: `x * 2^trunc(k)`.
pub(crate) fn ext_scale(x: &Ext80, k: &Ext80, cw: u16) -> Ext80 {
    let (u, ku) = (unpack(x), unpack(k));
    if u.class != Class::Finite {
        return *x;
    }
    match ku.class {
        Class::Nan => return indefinite(),
        Class::Inf => return if ku.neg { zero(u.neg) } else { inf(u.neg) },
        Class::Zero => return *x,
        Class::Finite => {}
    }
    let n: i64 = if ku.exp < 0 {
        0
    } else if ku.exp >= 31 {
        if ku.neg { -(1 << 31) } else { 1 << 31 }
    } else {
        let mag = (ku.sig >> (63 - ku.exp)) as i64;
        if ku.neg { -mag } else { mag }
    };
    let e = u.exp as i64 + n;
    if e > i32::MAX as i64 / 2 {
        return inf(u.neg);
    }
    if e < i32::MIN as i64 / 2 {
        return zero(u.neg);
    }
    let _ = cw;
    pack(u.neg, e as i32, u.sig)
}

/// `FPREM1`, run to completion: the IEEE remainder `x - Q*m` (`Q` the integer nearest `x/m`,
/// ties to even), exact, with the low three bits of `|Q|`
/// (bit 0 = Q0, bit 1 = Q1, bit 2 = Q2).
pub(crate) fn ext_prem1_quo(x: &Ext80, modulus: &Ext80, _cw: u16) -> (Ext80, u8) {
    let (a, m) = (unpack(x), unpack(modulus));
    match (a.class, m.class) {
        (Class::Finite, Class::Finite) => {}
        (Class::Zero, Class::Finite) | (Class::Zero, Class::Inf) => return (*x, 0),
        (Class::Finite, Class::Inf) => return (*x, 0),
        _ => return (indefinite(), 0),
    }
    let (am, mm) = (a.sig as u128, m.sig as u128);
    let d = a.exp - m.exp;
    // |x| = Q*|m| + rem, with rem in units of 2^(m.exp - 63) as a signed 129-bit amount.
    let (q, rem_mag, rem_neg) = if d < -1 {
        (0u128, None, false)
    } else if d == -1 {
        // |x|/|m| in [1/4, 1): Q = 1 iff |x| > |m|/2, i.e. a.sig > m.sig (equality is a tie to 0).
        if am > mm {
            // |x| - |m| = (a.sig - 2 m.sig) * 2^(a.exp - 63): negative.
            (1, Some(((2 * mm - am), a.exp)), true)
        } else {
            (0, None, false)
        }
    } else {
        // |x| = am * 2^(m.exp - 63 + d); reduce am * 2^d modulo mm exactly.
        let d = d as u32;
        let (mut q, mut r) = if d <= 63 {
            let n = am << d;
            (n / mm, n % mm)
        } else {
            // Long reduction: the quotient's low bits are those of the whole quotient.
            let mut r = am % mm;
            let mut q_low: u128 = am / mm;
            for _ in 0..d {
                q_low = (q_low << 1) & 0xff;
                r <<= 1;
                if r >= mm {
                    r -= mm;
                    q_low |= 1;
                }
            }
            (q_low, r)
        };
        let twice = 2 * r;
        let up = twice > mm || (twice == mm && q & 1 == 1);
        if up {
            q += 1;
            r = mm - r;
            (q, Some((r, m.exp)), true)
        } else {
            (q, Some((r, m.exp)), false)
        }
    };
    let quo = (q & 7) as u8;
    let rem = match rem_mag {
        None => *x,
        Some((0, _)) => zero(a.neg),
        Some((mag, exp)) => {
            let neg = a.neg != rem_neg;
            // value = mag * 2^(exp - 63)
            normalize_round_pack(neg, exp + 64, mag, false, 64, Rc::Nearest)
        }
    };
    (rem, quo)
}

// ---------------------------------------------------------------------------------------------
// The 128-bit software float the transcendental instructions are formed in.
// ---------------------------------------------------------------------------------------------

/// `m * 2^(e - 127)`, `m`'s top bit set (or `m == 0` for zero). Operations truncate; each keeps
/// about 126 correct bits.
#[derive(Clone, Copy, Debug)]
struct Big {
    neg: bool,
    e: i32,
    m: u128,
}

const BIG_ZERO: Big = Big {
    neg: false,
    e: 0,
    m: 0,
};
/// ln 2 to 128 bits.
const LN2_M: u128 = 0xB172_17F7_D1CF_79AB_C9E3_B398_03F2_F6AF;
/// pi to 128 bits.
const PI_M: u128 = 0xC90F_DAA2_2168_C234_C4C6_628B_80DC_1CD1;
/// sqrt(2) to 128 bits.
const SQRT2_M: u128 = 0xB504_F333_F9DE_6484_597D_89B3_754A_BE9F;

impl Big {
    fn norm(neg: bool, e: i32, m: u128) -> Big {
        if m == 0 {
            return BIG_ZERO;
        }
        let lz = m.leading_zeros();
        Big {
            neg,
            e: e - lz as i32,
            m: m << lz,
        }
    }

    fn from_unpacked(u: &Unpacked) -> Big {
        if u.class == Class::Zero {
            return BIG_ZERO;
        }
        Big {
            neg: u.neg,
            e: u.exp,
            m: (u.sig as u128) << 64,
        }
    }

    fn from_u64(n: u64) -> Big {
        Big::norm(false, 127, n as u128)
    }

    fn ln2() -> Big {
        Big {
            neg: false,
            e: -1,
            m: LN2_M,
        }
    }

    fn pi() -> Big {
        Big {
            neg: false,
            e: 1,
            m: PI_M,
        }
    }

    fn is_zero(&self) -> bool {
        self.m == 0
    }

    fn neg(self) -> Big {
        Big {
            neg: !self.neg,
            ..self
        }
    }

    fn scale2(self, k: i32) -> Big {
        if self.is_zero() {
            self
        } else {
            Big {
                e: self.e + k,
                ..self
            }
        }
    }

    fn add(self, b: Big) -> Big {
        if self.is_zero() {
            return b;
        }
        if b.is_zero() {
            return self;
        }
        let (big, small) = if (self.e, self.m) >= (b.e, b.m) {
            (self, b)
        } else {
            (b, self)
        };
        let d = (big.e - small.e) as u32;
        let sm = if d >= 128 { 0 } else { small.m >> d };
        if big.neg == small.neg {
            let (s, carry) = big.m.overflowing_add(sm);
            if carry {
                Big {
                    neg: big.neg,
                    e: big.e + 1,
                    m: (s >> 1) | (1u128 << 127),
                }
            } else {
                Big {
                    neg: big.neg,
                    e: big.e,
                    m: s,
                }
            }
        } else {
            Big::norm(big.neg, big.e, big.m - sm)
        }
    }

    fn sub(self, b: Big) -> Big {
        self.add(b.neg())
    }

    fn mul(self, b: Big) -> Big {
        if self.is_zero() || b.is_zero() {
            return BIG_ZERO;
        }
        let (a1, a0) = (self.m >> 64, self.m & 0xffff_ffff_ffff_ffff);
        let (b1, b0) = (b.m >> 64, b.m & 0xffff_ffff_ffff_ffff);
        let ll = a0 * b0;
        let lh = a0 * b1;
        let hl = a1 * b0;
        let hh = a1 * b1;
        let mid = (ll >> 64) + (lh & 0xffff_ffff_ffff_ffff) + (hl & 0xffff_ffff_ffff_ffff);
        let hi = hh + (lh >> 64) + (hl >> 64) + (mid >> 64);
        let lo_top = (mid & 0xffff_ffff_ffff_ffff) >> 63; // next bit below hi
        let neg = self.neg != b.neg;
        // product = hi * 2^128 + ..., value = product * 2^(ea + eb - 254)
        if hi >> 127 == 1 {
            Big {
                neg,
                e: self.e + b.e + 1,
                m: hi,
            }
        } else {
            Big {
                neg,
                e: self.e + b.e,
                m: (hi << 1) | lo_top,
            }
        }
    }

    fn div(self, b: Big) -> Big {
        debug_assert!(!b.is_zero());
        if self.is_zero() {
            return BIG_ZERO;
        }
        let neg = self.neg != b.neg;
        let (mut r, mut carry, e) = if self.m >= b.m {
            (self.m, false, self.e - b.e)
        } else {
            (self.m << 1, self.m >> 127 == 1, self.e - b.e - 1)
        };
        let mut q: u128 = 0;
        for _ in 0..128 {
            q <<= 1;
            if carry || r >= b.m {
                r = r.wrapping_sub(b.m);
                q |= 1;
            }
            carry = r >> 127 == 1;
            r <<= 1;
        }
        Big { neg, e, m: q }
    }

    fn div_small(self, n: u64) -> Big {
        let n = n as u128;
        let q = self.m / n;
        let r = self.m % n;
        let lz = q.leading_zeros();
        Big {
            neg: self.neg,
            e: self.e - lz as i32,
            m: (q << lz) | ((r << lz) / n),
        }
    }

    /// An `f64` near the value (for seeds).
    fn approx(&self) -> f64 {
        if self.is_zero() {
            return 0.0;
        }
        let top = (self.m >> 64) as u64 as f64; // in [2^63, 2^64)
        let v = top * 2f64.powi(self.e - 63);
        if self.neg { -v } else { v }
    }

    fn from_f64(x: f64) -> Big {
        let u = unpack(&ext_from_f64(x));
        Big::from_unpacked(&u)
    }

    /// `sqrt` of a positive value: Newton on `1/sqrt`, then one multiply.
    fn sqrt(self) -> Big {
        if self.is_zero() {
            return BIG_ZERO;
        }
        // Keep the exponent even so the seed and the iterations stay near 1.
        let half = self.e.div_euclid(2);
        let x = Big {
            e: self.e - 2 * half,
            ..self
        }; // in [1, 4)
        let mut y = Big::from_f64(1.0 / x.approx().sqrt());
        let three = Big::from_u64(3);
        for _ in 0..3 {
            y = y.mul(three.sub(x.mul(y).mul(y))).scale2(-1);
        }
        x.mul(y).scale2(half)
    }

    /// Exponent of the leading bit (`value` is in `[2^lead, 2^(lead+1))`).
    fn lead(&self) -> i32 {
        self.e
    }

    /// Rounds to an extended value under `cw` (the result is inexact).
    fn to_ext(self, cw: u16) -> Ext80 {
        if self.is_zero() {
            return zero(self.neg);
        }
        round_pack(self.neg, self.e, self.m, true, precision_of(cw), rc_of(cw))
    }
}

/// Sums a series until the next term falls 140 bits below the running sum.
fn negligible(term: &Big, sum: &Big) -> bool {
    term.is_zero() || term.lead() < sum.lead() - 140
}

/// `e^z - 1` for `|z| < 1`.
fn big_expm1(z: Big) -> Big {
    if z.is_zero() {
        return z;
    }
    let mut term = z;
    let mut sum = z;
    let mut n = 2u64;
    loop {
        term = term.mul(z).div_small(n);
        if negligible(&term, &sum) {
            return sum;
        }
        sum = sum.add(term);
        n += 1;
    }
}

/// `ln v` for a positive `v`.
fn big_ln(v: Big) -> Big {
    // v = f * 2^k with f in [1/sqrt2, sqrt2).
    let mut k = v.e;
    let mut f = Big {
        neg: false,
        e: 0,
        m: v.m,
    };
    if f.m > SQRT2_M {
        f = f.scale2(-1);
        k += 1;
    }
    let one = Big::from_u64(1);
    let s = f.sub(one).div(f.add(one));
    let mut sum = s;
    if !s.is_zero() {
        let s2 = s.mul(s);
        let mut power = s;
        let mut j = 3u64;
        loop {
            power = power.mul(s2);
            let term = power.div_small(j);
            if negligible(&term, &sum) {
                break;
            }
            sum = sum.add(term);
            j += 2;
        }
    }
    let atanh2 = sum.scale2(1);
    if k == 0 {
        return atanh2;
    }
    let kb = Big::from_u64(k.unsigned_abs() as u64);
    let kb = if k < 0 { kb.neg() } else { kb };
    kb.mul(Big::ln2()).add(atanh2)
}

/// `atan t` for `0 <= t <= 1`.
fn big_atan_unit(t: Big) -> Big {
    if t.is_zero() {
        return t;
    }
    let one = Big::from_u64(1);
    // Three halvings of the angle: t <- t / (1 + sqrt(1 + t^2)).
    let mut t = t;
    for _ in 0..3 {
        t = t.div(one.add(one.add(t.mul(t)).sqrt()));
    }
    let t2 = t.mul(t);
    let mut sum = t;
    let mut power = t;
    let mut j = 3u64;
    let mut negative = true;
    loop {
        power = power.mul(t2);
        let term = power.div_small(j);
        if negligible(&term, &sum) {
            break;
        }
        sum = if negative {
            sum.sub(term)
        } else {
            sum.add(term)
        };
        negative = !negative;
        j += 2;
    }
    sum.scale2(3)
}

/// `sin r` and `cos r` by their Taylor series.
fn big_sin(r: Big) -> Big {
    if r.is_zero() {
        return r;
    }
    let r2 = r.mul(r).neg();
    let mut term = r;
    let mut sum = r;
    let mut n = 2u64;
    loop {
        term = term.mul(r2).div_small(n * (n + 1));
        if negligible(&term, &sum) {
            return sum;
        }
        sum = sum.add(term);
        n += 2;
    }
}

fn big_cos(r: Big) -> Big {
    let one = Big::from_u64(1);
    if r.is_zero() {
        return one;
    }
    let r2 = r.mul(r).neg();
    let mut term = one;
    let mut sum = one;
    let mut n = 1u64;
    loop {
        term = term.mul(r2).div_small(n * (n + 1));
        if negligible(&term, &sum) {
            return sum;
        }
        sum = sum.add(term);
        n += 2;
    }
}

// ---------------------------------------------------------------------------------------------
// The transcendental instructions.
// ---------------------------------------------------------------------------------------------

/// `F2XM1`: `2^x - 1`, defined for `|x| <= 1`.
pub(crate) fn ext_f2xm1(x: &Ext80, cw: u16) -> Ext80 {
    let u = unpack(x);
    match u.class {
        Class::Zero => return *x,
        Class::Nan => return indefinite(),
        Class::Inf => return if u.neg { ext_chs(&ext_one(), cw) } else { *x },
        Class::Finite => {}
    }
    big_expm1(Big::from_unpacked(&u).mul(Big::ln2())).to_ext(cw)
}

/// `FYL2X`: `y * log2(x)`, `x > 0`.
pub(crate) fn ext_fyl2x(y: &Ext80, x: &Ext80, cw: u16) -> Ext80 {
    let (yu, xu) = (unpack(y), unpack(x));
    if yu.class == Class::Nan || xu.class == Class::Nan || xu.neg && xu.class != Class::Zero {
        return indefinite();
    }
    match (xu.class, yu.class) {
        (Class::Zero, Class::Zero) => return indefinite(),
        (Class::Zero, _) => return inf(!yu.neg),
        (Class::Inf, Class::Zero) => return indefinite(),
        (Class::Inf, _) => return inf(yu.neg),
        (_, Class::Inf) => {
            // log2(x) sign decides; x == 1 gives an invalid 0 * inf.
            let one = xu.exp == 0 && xu.sig == 1 << 63;
            if one {
                return indefinite();
            }
            return inf(yu.neg != (xu.exp < 0));
        }
        _ => {}
    }
    // An exact power of two has an integer logarithm: the product then rounds once, exactly.
    if xu.sig == 1 << 63 {
        if xu.exp == 0 {
            return zero(yu.neg);
        }
        if yu.class == Class::Zero {
            return zero(yu.neg != (xu.exp < 0));
        }
        let k = ext_from_f64(xu.exp as f64);
        return ext_mul(y, &k, cw);
    }
    if yu.class == Class::Zero {
        return zero(yu.neg != (xu.exp < 0));
    }
    let log2 = big_ln(Big::from_unpacked(&xu)).div(Big::ln2());
    Big::from_unpacked(&yu).mul(log2).to_ext(cw)
}

/// `FYL2XP1`: `y * log2(1 + x)`, defined for `|x| < 1 - sqrt(2)/2`.
pub(crate) fn ext_fyl2xp1(y: &Ext80, x: &Ext80, cw: u16) -> Ext80 {
    let (yu, xu) = (unpack(y), unpack(x));
    if yu.class != Class::Finite && yu.class != Class::Zero
        || xu.class != Class::Finite && xu.class != Class::Zero
    {
        return indefinite();
    }
    if xu.class == Class::Zero || yu.class == Class::Zero {
        return zero(yu.neg != xu.neg);
    }
    let one = Big::from_u64(1);
    let xb = Big::from_unpacked(&xu);
    // ln(1+x) = 2 atanh(x / (2 + x)), without forming 1 + x.
    let s = xb.div(one.scale2(1).add(xb));
    let s2 = s.mul(s);
    let mut sum = s;
    let mut power = s;
    let mut j = 3u64;
    loop {
        power = power.mul(s2);
        let term = power.div_small(j);
        if negligible(&term, &sum) {
            break;
        }
        sum = sum.add(term);
        j += 2;
    }
    let log2 = sum.scale2(1).div(Big::ln2());
    Big::from_unpacked(&yu).mul(log2).to_ext(cw)
}

/// `FPATAN`: the angle of the point `(x, y)`, i.e. `atan(y / x)` placed in the quadrant of the
/// operands' signs (`y` is `ST(1)`, `x` is `ST(0)`).
pub(crate) fn ext_fpatan(y: &Ext80, x: &Ext80, cw: u16) -> Ext80 {
    let (yu, xu) = (unpack(y), unpack(x));
    if yu.class == Class::Nan || xu.class == Class::Nan {
        return indefinite();
    }
    let pi = Big::pi();
    let angle = match (yu.class, xu.class) {
        (Class::Zero, _) => {
            if !xu.neg {
                return zero(yu.neg);
            }
            pi
        }
        (_, Class::Zero) => pi.scale2(-1),
        (Class::Inf, Class::Inf) => {
            if xu.neg {
                pi.mul(Big::from_u64(3)).scale2(-2)
            } else {
                pi.scale2(-2)
            }
        }
        (Class::Inf, _) => pi.scale2(-1),
        (_, Class::Inf) => {
            if !xu.neg {
                return zero(yu.neg);
            }
            pi
        }
        _ => {
            let (a, b) = (Big::from_unpacked(&yu), Big::from_unpacked(&xu));
            let (a, b) = (Big { neg: false, ..a }, Big { neg: false, ..b });
            let first = if (yu.exp, yu.sig) <= (xu.exp, xu.sig) {
                big_atan_unit(a.div(b))
            } else {
                pi.scale2(-1).sub(big_atan_unit(b.div(a)))
            };
            if xu.neg { pi.sub(first) } else { first }
        }
    };
    let angle = if yu.neg { angle.neg() } else { angle };
    angle.to_ext(cw)
}

/// The operand of `FSIN`/`FCOS`/`FPTAN` as a [`Big`], or the special result.
fn trig_operand(x: &Ext80) -> Result<Big, Ext80> {
    let u = unpack(x);
    match u.class {
        Class::Zero => Err(*x),
        Class::Nan | Class::Inf => Err(indefinite()),
        Class::Finite => Ok(Big::from_unpacked(&u)),
    }
}

/// `FSIN`.
pub(crate) fn ext_sin(x: &Ext80, cw: u16) -> Ext80 {
    match trig_operand(x) {
        Err(special) => special,
        Ok(r) => big_sin(r).to_ext(cw),
    }
}

/// `FCOS`.
pub(crate) fn ext_cos(x: &Ext80, cw: u16) -> Ext80 {
    if unpack(x).class == Class::Zero {
        return ext_one();
    }
    match trig_operand(x) {
        Err(special) => special,
        Ok(r) => big_cos(r).to_ext(cw),
    }
}

/// `FPTAN` (the tangent; the instruction's pushed `1.0` is not returned).
pub(crate) fn ext_tan(x: &Ext80, cw: u16) -> Ext80 {
    match trig_operand(x) {
        Err(special) => special,
        Ok(r) => big_sin(r).div(big_cos(r)).to_ext(cw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn ext_bits(x: &Ext80) -> u128 {
        let mut b = [0u8; 16];
        b[..10].copy_from_slice(&x.0);
        u128::from_le_bytes(b)
    }

    /// The 128-bit constants against independent series: ln 2 = sum 1/(k 2^k), pi by Machin's
    /// formula, sqrt 2 by squaring.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn constants_match_their_series() {
        let mut ln2 = BIG_ZERO;
        for k in 1..200u64 {
            ln2 = ln2.add(Big::from_u64(1).scale2(-(k as i32)).div_small(k));
        }
        assert!(
            (ln2.m as i128 - LN2_M as i128).abs() <= 256 && ln2.e == -1,
            "{ln2:?}"
        );
        let atan_inv = |n: u64| {
            let x = Big::from_u64(1).div_small(n);
            let mut sum = x;
            let mut power = x;
            let mut j = 3u64;
            let mut negative = true;
            loop {
                power = power.div_small(n * n);
                let term = power.div_small(j);
                if negligible(&term, &sum) {
                    break sum;
                }
                sum = if negative {
                    sum.sub(term)
                } else {
                    sum.add(term)
                };
                negative = !negative;
                j += 2;
            }
        };
        let pi = atan_inv(5).scale2(4).sub(atan_inv(239).scale2(2));
        assert!(
            (pi.m as i128 - PI_M as i128).abs() <= 256 && pi.e == 1,
            "{pi:?}"
        );
        let two = Big {
            neg: false,
            e: 0,
            m: SQRT2_M,
        }
        .mul(Big {
            neg: false,
            e: 0,
            m: SQRT2_M,
        });
        assert!(
            two.e == 0 && two.m >= u128::MAX - 8 || two.e == 1 && two.m <= (1 << 127) + 8,
            "{two:?}"
        );
        // The software series against these constants.
        let ln2_series = big_ln(Big::from_u64(2));
        assert!((ln2_series.m as i128 - LN2_M as i128).abs() <= 64);
        let pi_atan = big_atan_unit(Big::from_u64(1)).scale2(2);
        assert!(
            (pi_atan.m as i128 - PI_M as i128).abs() <= 64,
            "{pi_atan:?}"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn loads_and_stores_round_trip_every_binary64_class() {
        for bits in [
            0u64,
            1 << 63,
            1,
            0x000f_ffff_ffff_ffff,
            0x0010_0000_0000_0000,
            0x3ff0_0000_0000_0000,
            0x7fef_ffff_ffff_ffff,
            0x7ff0_0000_0000_0000,
            0xfff0_0000_0000_0000,
            0x4009_21fb_5444_2d18,
        ] {
            let x = f64::from_bits(bits);
            assert_eq!(
                ext_to_f64(&ext_from_f64(x), CW_PC64_RN).to_bits(),
                bits,
                "{bits:016x}"
            );
        }
        assert!(ext_to_f64(&ext_from_f64(f64::NAN), CW_PC64_RN).is_nan());
        assert_eq!(ext_bits(&ext_one()), 0x3fff_8000_0000_0000_0000);
    }
}

/// The software x87 against the x87 hardware it stands in for.
#[cfg(all(test, target_arch = "x86_64"))]
mod hardware {
    use super::*;
    use crate::ported::excel_numeric::x87::raw as hw;

    pub(super) struct Rng(pub(super) u64);
    impl Rng {
        pub(super) fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        /// An integer in `lo .. lo + span`.
        pub(super) fn int(&mut self, lo: i32, span: u64) -> i32 {
            lo + (self.next() % span) as i32
        }
        /// A random extended value with a leading exponent in `lo .. lo + span` and a random sign.
        pub(super) fn ext(&mut self, lo: i32, span: u64) -> Ext80 {
            let e = self.int(lo, span);
            let neg = self.next() & 1 == 1;
            pack(neg, e, self.next() | 1 << 63)
        }
        /// As [`Rng::ext`], positive.
        pub(super) fn ext_pos(&mut self, lo: i32, span: u64) -> Ext80 {
            let e = self.int(lo, span);
            pack(false, e, self.next() | 1 << 63)
        }
        /// A positive binary64 with a leading exponent in `lo .. lo + span`.
        pub(super) fn f64_in(&mut self, lo: i32, span: u64) -> f64 {
            let e = (self.int(lo, span) + 1023) as u64;
            f64::from_bits((self.next() & 0x000f_ffff_ffff_ffff) | (e << 52))
        }
        pub(super) fn sign(&mut self) -> f64 {
            if self.next() & 1 == 1 { -1.0 } else { 1.0 }
        }
    }

    /// Native-only: compares with the x87 hardware, which only x86_64 has.
    #[test]
    fn constants_equal_the_rom() {
        assert_eq!(ext_pi(), hw::ext_pi());
        assert_eq!(ext_l2e(), hw::ext_l2e());
        assert_eq!(ext_l2t(), hw::ext_l2t());
        assert_eq!(ext_ln2(), hw::ext_ln2());
        assert_eq!(ext_lg2(), hw::ext_lg2());
        assert_eq!(ext_one(), hw::ext_one());
    }

    /// Runs `n` random cases and counts those where the two results differ, printing the first.
    pub(super) fn count<T: PartialEq + core::fmt::Debug, F: FnMut(&mut Rng) -> (T, T, String)>(
        name: &str,
        n: usize,
        seed: u64,
        mut f: F,
    ) -> usize {
        let mut rng = Rng(seed);
        let mut bad = 0;
        for _ in 0..n {
            let (sw, hwv, what) = f(&mut rng);
            if sw != hwv {
                if bad < 5 {
                    eprintln!("{name}: {what}: software {sw:?} hardware {hwv:?}");
                }
                bad += 1;
            }
        }
        eprintln!("{name}: {bad} of {n} differ");
        bad
    }

    /// Native-only: compares with the x87 hardware, which only x86_64 has.
    #[test]
    fn exact_operations_equal_the_hardware() {
        let n = 100_000;
        let cws = [
            CW_PC64_RN,
            CW_PC53_RN,
            CW_PC24_RN,
            CW_PC64_RN | 0x0C00,
            CW_PC64_RN | 0x0400,
            CW_PC64_RN | 0x0800,
        ];
        let mut total = 0;
        for &cw in &cws {
            total += count(&format!("add cw={cw:04x}"), n, 1, |r| {
                let (a, b) = (r.ext(-20, 40), r.ext(-20, 40));
                (
                    ext_add(&a, &b, cw),
                    hw::ext_add(&a, &b, cw),
                    format!("{a:?} {b:?}"),
                )
            });
            total += count(&format!("sub cw={cw:04x}"), n, 2, |r| {
                let e = r.int(0, 8);
                let (a, b) = (r.ext(e, 1), r.ext(e - 1, 3));
                (
                    ext_sub(&a, &b, cw),
                    hw::ext_sub(&a, &b, cw),
                    format!("{a:?} {b:?}"),
                )
            });
            total += count(&format!("mul cw={cw:04x}"), n, 3, |r| {
                let (a, b) = (r.ext(-20, 40), r.ext(-20, 40));
                (
                    ext_mul(&a, &b, cw),
                    hw::ext_mul(&a, &b, cw),
                    format!("{a:?} {b:?}"),
                )
            });
            total += count(&format!("div cw={cw:04x}"), n, 4, |r| {
                let (a, b) = (r.ext(-20, 40), r.ext(-20, 40));
                (
                    ext_div(&a, &b, cw),
                    hw::ext_div(&a, &b, cw),
                    format!("{a:?} {b:?}"),
                )
            });
            total += count(&format!("sqrt cw={cw:04x}"), n, 5, |r| {
                let a = r.ext_pos(-20, 40);
                (ext_sqrt(&a, cw), hw::ext_sqrt(&a, cw), format!("{a:?}"))
            });
            total += count(&format!("rndint cw={cw:04x}"), n, 6, |r| {
                let a = r.ext(-3, 70);
                (ext_rndint(&a, cw), hw::ext_rndint(&a, cw), format!("{a:?}"))
            });
            total += count(&format!("store cw={cw:04x}"), n, 7, |r| {
                let a = match r.next() % 3 {
                    0 => r.ext(-1100, 2200),
                    1 => r.ext(-1100, 60),
                    _ => r.ext(1018, 8),
                };
                let (s, h) = (ext_to_f64(&a, cw), hw::ext_to_f64(&a, cw));
                (s.to_bits(), h.to_bits(), format!("{a:?}"))
            });
        }
        total += count("scale", n, 8, |r| {
            let a = r.ext(-20, 40);
            let k = ext_from_f64(r.int(-100, 200) as f64 + 0.5);
            (
                ext_scale(&a, &k, CW_PC64_RN),
                hw::ext_scale(&a, &k, CW_PC64_RN),
                format!("{a:?} {k:?}"),
            )
        });
        total += count("prem1", n, 9, |r| {
            let a = ext_from_f64(r.f64_in(-4, 36) * r.sign());
            let m = if r.next() & 1 == 1 {
                ext_pi()
            } else {
                ext_scale(&ext_pi(), &ext_chs(&ext_one(), 0), 0)
            };
            (
                ext_prem1_quo(&a, &m, CW_PC64_RN),
                hw::ext_prem1_quo(&a, &m, CW_PC64_RN),
                format!("{a:?}"),
            )
        });
        assert_eq!(total, 0);
    }

    /// Native-only: compares with the x87 hardware, which only x86_64 has.
    #[test]
    #[ignore]
    fn transcendental_operations_against_the_hardware() {
        let n: usize = std::env::var("SOFT87_N")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100_000);
        let cw = CW_PC64_RN;
        count("f2xm1 [0,1/2]", n, 11, |r| {
            let a = r.ext_pos(-3, 2);
            (ext_f2xm1(&a, cw), hw::ext_f2xm1(&a, cw), format!("{a:?}"))
        });
        count("fyl2x ln2*log2(x)", n, 12, |r| {
            let x = ext_from_f64(r.f64_in(-30, 60));
            (
                ext_fyl2x(&ext_ln2(), &x, cw),
                hw::ext_fyl2x(&ext_ln2(), &x, cw),
                format!("{x:?}"),
            )
        });
        count("fyl2x y*log2(x)", n, 13, |r| {
            let x = ext_from_f64(r.f64_in(-30, 60));
            let y = r.ext(-5, 10);
            (
                ext_fyl2x(&y, &x, cw),
                hw::ext_fyl2x(&y, &x, cw),
                format!("{y:?} {x:?}"),
            )
        });
        count("fpatan(x,1)", n, 14, |r| {
            let x = ext_from_f64(r.f64_in(-30, 30) * r.sign());
            (
                ext_fpatan(&x, &ext_one(), cw),
                hw::ext_fpatan(&x, &ext_one(), cw),
                format!("{x:?}"),
            )
        });
        count("fsin |r|<=pi/2", n, 15, |r| {
            let a = r.ext(-5, 6);
            (ext_sin(&a, cw), hw::ext_sin(&a, cw), format!("{a:?}"))
        });
        count("fcos |r|<=pi/4", n, 16, |r| {
            let a = r.ext(-6, 6);
            (ext_cos(&a, cw), hw::ext_cos(&a, cw), format!("{a:?}"))
        });
        count("fptan |r|<=pi/4", n, 17, |r| {
            let a = r.ext(-6, 6);
            (ext_tan(&a, cw), hw::ext_tan(&a, cw), format!("{a:?}"))
        });
    }
}
