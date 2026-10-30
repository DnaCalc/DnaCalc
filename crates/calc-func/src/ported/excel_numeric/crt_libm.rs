//! The Windows runtime's `log`, `log10`, `exp`, `sin`, `cos`, `sinh`, `cosh`, `atan2` and `hypot`
//! in portable Rust (dc-twc): the bits the `f64` methods give under MSVC on x86_64 on a CPU with
//! FMA3, on every target. The tests below compare each with the runtime by calling it.
//!
//! # Sources
//! `exp`, `sin`, `cos`, `sinh`, `cosh`, `atan2` and `hypot` are translations of AMD's win-libm,
//! the x64 Windows math library sources AMD published under the MIT licence
//! (https://github.com/amd/win-libm at 6121d02; Copyright (c) 2002-2019 Advanced Micro Devices,
//! Inc.; notice in `deploy/notices/ported.txt`):
//! - `exp`: `exp.asm`, its FMA3 path (`n = trunc(x*64/ln2)`, the two-part reduction by ln2/64,
//!   the Horner polynomial 1/720 to 1/2), with `two_to_jby64_table.asm` and
//!   `two_to_jby64_head_tail_table.asm`;
//! - `sin`, `cos`: `sin.asm` and `cos.asm`, their FMA3 paths, with `Lsincos_array.asm`, and
//!   `remainder_piby2_forFMA3.asm`: the Boldo-Daumas-Li reduction below 2e7 and the general
//!   reduction (a 192-bit product with a window of `L2_by_pi_bits.asm`) from there;
//! - `sinh`, `cosh`: `sinh.c` and `cosh.c`, with `splitexp` from `libm_inlines.h`;
//! - `atan2`: `atan2.c`;
//! - `hypot`: `hypot.c` in its `FAST_BUT_GREATER_THAN_ONE_ULP` configuration.
//!
//! Every table in this file for these functions equals win-libm's value for value (checked,
//! dc-96l). `log` and `log10` are AMD's AOCL-LibM `log_fma3.S` and `log10_fma3.S` (Tang's
//! table-driven logarithm) with their tables, under AOCL-LibM's BSD-3-Clause licence below.
//!
//! # Measured
//! What no source says was established by calling the runtime: that it runs these FMA3 paths and
//! the `hypot` configuration above (the tests below: every port equals the runtime on 1,000,000
//! arguments per region), and which multiply-adds of the C routines `sinh`, `cosh`, `atan2` and
//! `splitexp` are fused, which is a compiler's choice. For the fused placement every possible
//! contraction of the C was made a switch and flipped one at a time against the runtime
//! (dc-96l.3, 2026-10-30; 1,000,000 arguments in each of three regions per function, then
//! 20,000,000 more per region for the switches the first pass could not tell apart). The
//! placement below differed from the runtime on none of the 9,000,000 arguments; 25 of the 65
//! alternatives were refuted by the runtime; the other 40 never change a result on those
//! arguments (some exactly, such as a product by 256, some because the term is far below the
//! last place), so they cannot be told apart by measurement and do not change any result seen.
//! None of this code was read from the runtime itself.
//!
//! A CPU without FMA3 makes the runtime take other code paths, which are not these.
//!
//! The `log` and `log10` tables are AOCL-LibM's (https://github.com/amd/aocl-libm-ose,
//! src/isa/avx2/gas/log_fma3.S and log10_fma3.S at master, 2026-10-25), under its licence: Copyright (C) 2008-2022 Advanced Micro Devices, Inc. All rights reserved.
//! Redistribution and use in source and binary forms, with or without modification, are
//! permitted provided that the following conditions are met: 1. Redistributions of source code
//! must retain the above copyright notice, this list of conditions and the following disclaimer.
//! 2. Redistributions in binary form must reproduce the above copyright notice, this list of
//! conditions and the following disclaimer in the documentation and/or other materials provided
//! with the distribution. 3. Neither the name of the copyright holder nor the names of its
//! contributors may be used to endorse or promote products derived from this software without
//! specific prior written permission. THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND
//! CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
//! IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN
//! NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
//! INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
//! PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
//! INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
//! LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
//! THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//!
//! `f64::mul_add` is the fused multiply-add: the instruction where the target has it, a correctly
//! rounded software form elsewhere, so the bits do not depend on the target.

#![allow(clippy::unreadable_literal, clippy::excessive_precision)]

const SIGN: u64 = 0x8000_0000_0000_0000;
const MANT: u64 = 0x000f_ffff_ffff_ffff;
const INF: u64 = 0x7ff0_0000_0000_0000;
const QNAN_BIT: u64 = 0x0008_0000_0000_0000;
const ONE: u64 = 0x3ff0_0000_0000_0000;
const HALF: u64 = 0x3fe0_0000_0000_0000;
const MANT_ALL8: u64 = 0x000f_f000_0000_0000;
const MANT9: u64 = 0x0000_0800_0000_0000;

fn d(bits: u64) -> f64 {
    f64::from_bits(bits)
}

/// The steps both logarithms share before their tables: special inputs (`Err` with the result),
/// or the exponent as a binary64, the significand bits, the bits the table index is read from,
/// and `x - 1`.
fn log_reduce(x: f64) -> Result<(f64, u64, u64, f64), f64> {
    let ux = x.to_bits();
    let top = ux >> 52;
    let xm1 = x - 1.0;
    let mut xexp = ((top as i64 - 1023) as i32) as f64;
    let mut mant = ux & MANT;
    let mut index_bits = ux;
    if top >= 0x7ff || top == 0 {
        if ux & INF == INF {
            if ux == INF {
                return Err(x);
            }
            if ux == INF | SIGN {
                return Err(f64::NAN);
            }
            return Err(d(ux | QNAN_BIT));
        }
        if x <= 0.0 {
            return Err(if x == 0.0 { f64::NEG_INFINITY } else { f64::NAN });
        }
        // A subnormal: (significand | 1.0) - 1.0 normalizes it.
        let adjusted = d(mant | ONE) - 1.0;
        let bits = adjusted.to_bits();
        xexp = (((bits >> 52) as i64 - 2045) as i32) as f64;
        mant = bits & MANT;
        index_bits = mant;
    }
    Ok((xexp, mant, index_bits, xm1))
}

/// `r` and its index for the table path: `F` is 1/2 plus the first 9 significand bits rounded to
/// 8, `Y` the significand in [1/2, 1), `r = (F - Y) / F`.
fn log_table_r(mant: u64, index_bits: u64) -> (f64, usize) {
    let bits = (index_bits & MANT_ALL8) + ((index_bits & MANT9) << 1);
    let index = (bits >> 44) as usize;
    let y = d(mant | HALF);
    let f = d(bits | HALF);
    ((f - y) * d(LOG_F_INV[index]), index)
}

/// `r + r^2/2 + ... + r^6/6` as `log_fma3.S` forms it.
fn log_poly(r: f64) -> f64 {
    let r2 = r * r;
    let t3 = r.mul_add(d(0x3fc5555555555555), d(0x3fc999999999999a)); // r/6 + 1/5
    let t5 = r.mul_add(d(0x3fd5555555555555), d(0x3fe0000000000000)); // r/3 + 1/2
    let t3 = r.mul_add(t3, d(0x3fd0000000000000)); // + 1/4
    let r4 = r2 * r2;
    let poly = r2.mul_add(t5, r);
    r4.mul_add(t3, poly)
}

const CA1: u64 = 0x3fb55555555554e6;
const CA2: u64 = 0x3f89999999bac6d4;
const CA3: u64 = 0x3f62492307f1519f;
const CA4: u64 = 0x3f3c8034c85dfff0;

/// The near-one path: `r = x - 1`, `u = 2r/(2+r)`, and the odd series in `u`. Returns
/// `(r, the series less the correction r*u/2)` before the final sums, `fma7` choosing the fused
/// last step (`log`) or a product and a sum (`log10`).
fn log_near_one(x: f64, fma7: bool) -> (f64, f64) {
    let r = x - 1.0;
    let u_half = r / (2.0 + r);
    let correction = r * u_half;
    let u = u_half + u_half;
    let u2 = u * u;
    let a = u2.mul_add(d(CA2), d(CA1));
    let b = u2.mul_add(d(CA4), d(CA3));
    let u3 = u2 * u;
    let a = a * u3;
    let u7 = (u3 * u3) * u;
    let a = if fma7 { b.mul_add(u7, a) } else { a + b * u7 };
    (r, a - correction)
}

/// `log(x)` as the runtime's FMA3 `log` gives it.
pub(crate) fn crt_log(x: f64) -> f64 {
    let (xexp, mant, index_bits, xm1) = match log_reduce(x) {
        Ok(parts) => parts,
        Err(special) => return special,
    };
    if xm1.abs() < 0.0625 {
        let (r, series) = log_near_one(x, true);
        return r + series;
    }
    let (r, index) = log_table_r(mant, index_bits);
    let poly = log_poly(r);
    let tail = xexp.mul_add(d(0x3e6efa39ef35793c), -poly);
    let lead = xexp.mul_add(d(0x3fe62e42e0000000), d(LOG_LEAD[index]));
    lead + (d(LOG_TAIL[index]) + tail)
}

/// `log10(x)` as the runtime's FMA3 `log10` gives it.
pub(crate) fn crt_log10(x: f64) -> f64 {
    let (xexp, mant, index_bits, xm1) = match log_reduce(x) {
        Ok(parts) => parts,
        Err(special) => return special,
    };
    const E_LEAD: u64 = 0x3fdbcb7800000000;
    const E_TAIL: u64 = 0x3ea8a93728719535;
    if xm1.abs() < 0.0625 {
        let (r, series) = log_near_one(x, false);
        let hi = d(r.to_bits() & 0xffff_ffff_0000_0000);
        let series = series + (r - hi);
        let a1 = series * d(E_LEAD);
        let a2 = series * d(E_TAIL);
        let h1 = hi * d(E_TAIL);
        let h2 = hi * d(E_LEAD);
        return ((h1 + a2) + a1) + h2;
    }
    let (r, index) = log_table_r(mant, index_bits);
    let poly = log_poly(r) * d(0x3fdbcb7b1526e50e);
    let tail = xexp.mul_add(d(0x3e03ef3fde623e25), -poly);
    let lead = xexp.mul_add(d(0x3fd3441350000000), d(LOG10_LEAD[index]));
    lead + (d(LOG10_TAIL[index]) + tail)
}

/// `e^x` as the runtime's FMA3 `exp` gives it.
pub(crate) fn crt_exp(x: f64) -> f64 {
    let ux = x.to_bits();
    let ax = ux & !SIGN;
    if ax >= INF {
        if ux == INF {
            return x;
        }
        if ux == INF | SIGN {
            return 0.0;
        }
        return d(ux | QNAN_BIT);
    }
    let max_arg = d(0x40862e42fefa39ef);
    if !(x <= max_arg && x >= d(0xc0874046dfefd9d0)) {
        if x > max_arg {
            return f64::INFINITY;
        }
        return if x >= d(0xc0874910d52d3051) { d(1) } else { 0.0 };
    }
    if ax <= 0x3e50_0000_0000_0000 {
        return 1.0 + x;
    }
    let t = x * d(0x40571547652b82fe); // 64/ln2
    let n_d = t.trunc();
    let n = t as i32;
    let head = n_d.mul_add(d(0xbf862e42fefa0000), x);
    let r = n_d * d(0xbd1cf79abc9e3b39) + head;
    let j = (n & 0x3f) as usize;
    let m = n >> 6;
    let mut p = r.mul_add(d(0x3f56c16c16c16c17), d(0x3f81111111111111)); // 1/720, 1/120
    p = r.mul_add(p, d(0x3fa5555555555555)); // 1/24
    p = r.mul_add(p, d(0x3fc5555555555555)); // 1/6
    p = r.mul_add(p, d(0x3fe0000000000000)); // 1/2
    let q = (r * r).mul_add(p, r);
    let f = (q * d(EXP_2_JBY64[j]) + d(EXP_2_JBY64_TAIL[j])) + d(EXP_2_JBY64_HEAD[j]);
    if m > -1022 || (m == -1022 && f >= 1.0) {
        return d(f.to_bits().wrapping_add(((m as u32) as u64) << 52));
    }
    f * d(1u64 << (((m + 1074) as u32) & 63))
}

// ---- sin and cos (win-libm sin.asm, cos.asm, remainder_piby2_forFMA3.asm) ----

/// The sine polynomial's coefficients S0..S5 (of r^3, r^5, ...).
const SIN_C: [u64; 6] = [
    0xbfc5555555555555, 0x3f81111111110bb3, 0xbf2a01a019e83e5c,
    0x3ec71de3796cde01, 0xbe5ae600b42fdfa7, 0x3de5e0b2f9a43bb8,
];
/// The cosine polynomial's coefficients C0..C5 (of r^4, r^6, ...).
const COS_C: [u64; 6] = [
    0x3fa5555555555555, 0xbf56c16c16c16967, 0x3efa01a019f4ec91,
    0xbe927e4fa17f667b, 0x3e21eeb690382eec, 0xbda907db47258aa7,
];
const PI_BY_4: u64 = 0x3fe921fb54442d18;
const TWO_POW_M13: u64 = 0x3f20000000000000;
const TWO_POW_M27: u64 = 0x3e40000000000000;
/// From 2e7 on the reduction is the Payne-Hanek routine.
const LARGE: u64 = 0x417312d000000000;

/// `S1 + x2*(S2 + ... + x2*S5)`, the four steps both sine forms share.
fn sin_poly(x2: f64) -> f64 {
    let mut p = x2.mul_add(d(SIN_C[5]), d(SIN_C[4]));
    p = x2.mul_add(p, d(SIN_C[3]));
    p = x2.mul_add(p, d(SIN_C[2]));
    x2.mul_add(p, d(SIN_C[1]))
}

/// `C0 + x2*(C1 + ... + x2*C5)`.
fn cos_poly(x2: f64) -> f64 {
    let mut p = x2.mul_add(d(COS_C[5]), d(COS_C[4]));
    p = x2.mul_add(p, d(COS_C[3]));
    p = x2.mul_add(p, d(COS_C[2]));
    p = x2.mul_add(p, d(COS_C[1]));
    x2.mul_add(p, d(COS_C[0]))
}

/// The sine of a reduced argument `r + rr`, `|r| <= pi/4`.
fn sin_reduced(r: f64, rr: f64) -> f64 {
    let x2 = r * r;
    let p = sin_poly(x2);
    let x3 = r * x2;
    let mut t = (rr * 0.5) - x3 * p;
    t = x2 * t - rr;
    t = (-x3).mul_add(d(SIN_C[0]), t);
    r - t
}

/// The cosine of a reduced argument `r + rr`, `|r| <= pi/4`.
fn cos_reduced(r: f64, rr: f64) -> f64 {
    let x2 = r * r;
    let half = x2 * 0.5;
    let t = 1.0 - half;
    let mut w = (1.0 - t) - half;
    w = (-r).mul_add(rr, w);
    let x4 = x2 * x2;
    x4.mul_add(cos_poly(x2), w) + t
}

/// The reduction by pi/2 for `pi/4 <= x < 2e7` (x positive): win-libm's
/// `__remainder_piby2_fma3_bdl` (Boldo, Daumas and Li, IEEE Trans. Comp. 58(8), 2009), `r + rr`
/// and the quadrant.
fn reduce_medium(x: f64) -> (f64, f64, u32) {
    const SHIFT: u64 = 0x4338000000000000;
    let npi2 = x.mul_add(d(0x3fe45f306dc9c883), d(SHIFT)) - d(SHIFT);
    let region = npi2 as i32 as u32;
    let piby2_1 = d(0x3ff921fb54442d18);
    let piby2_1tail = d(0x3c91a62633145c00);
    let rhead = (-npi2).mul_add(piby2_1, x);
    let rtail = npi2 * piby2_1tail;
    let product_error = npi2.mul_add(piby2_1tail, -rtail);
    let r1 = rhead - rtail;
    let e1 = (rhead - r1) - rtail;
    let r = (-npi2).mul_add(piby2_1tail, rhead);
    let e2 = ((r1 - r) + e1) - product_error;
    let rr = (-npi2).mul_add(d(0x397b839a252049c0), e2);
    (r, rr, region & 3)
}

/// The bits of 2/pi the general reduction reads, as bytes, least significant first: win-libm's
/// `__L_2_by_pi_bits` (`L2_by_pi_bits.asm`, 158 bytes) and two zero bytes of padding.
#[rustfmt::skip]
const EIGHT_BY_PI: [u8; 160] = [
    0xe0, 0xf1, 0x1b, 0xc1, 0x0c, 0x58, 0x21, 0x74, 0x35, 0x7e, 0xc4, 0x7e, 0xed, 0xaf, 0xa9, 0x4b,
    0x4a, 0x29, 0xde, 0xe7, 0x1c, 0xf4, 0xec, 0xc5, 0x97, 0xaf, 0x1f, 0xeb, 0x9e, 0xd4, 0xb5, 0xa8,
    0x7f, 0x79, 0x9a, 0xfd, 0x18, 0x3d, 0xdd, 0x26, 0x2c, 0x9f, 0x3c, 0xfb, 0xd9, 0xb4, 0x7d, 0xb4,
    0x29, 0x68, 0x2d, 0x46, 0xbc, 0xbc, 0x3f, 0x60, 0x16, 0x78, 0xff, 0x5f, 0xe2, 0x7f, 0xec, 0xa0,
    0xe4, 0xf7, 0x2e, 0x7e, 0x11, 0x72, 0xd2, 0xe7, 0x4c, 0x0d, 0xe6, 0x58, 0x47, 0xe6, 0x04, 0xf9,
    0x7d, 0xd1, 0x9a, 0xc0, 0x71, 0xa6, 0x13, 0x12, 0xed, 0xba, 0xd4, 0xd7, 0x08, 0xa2, 0xfb, 0x9c,
    0xa6, 0xc4, 0x72, 0xac, 0x77, 0xf8, 0x73, 0x48, 0x46, 0x27, 0xa8, 0xbb, 0x24, 0x19, 0x80, 0x4b,
    0x37, 0x09, 0xe9, 0xb8, 0x91, 0xdc, 0x86, 0x15, 0xef, 0x7a, 0xaf, 0x8e, 0x45, 0xf9, 0x07, 0x41,
    0x0e, 0xf1, 0x64, 0x56, 0x8a, 0x6d, 0x03, 0x77, 0xd3, 0xd4, 0x47, 0x5f, 0x9d, 0xf0, 0xa7, 0x54,
    0x10, 0x39, 0xb9, 0x0d, 0xe6, 0x8b, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

fn load_u64(offset: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&EIGHT_BY_PI[offset..offset + 8]);
    u64::from_le_bytes(b)
}

/// The general reduction for `2e7 <= x` (x positive, finite): win-libm's
/// `__remainder_piby2_fma3`, a 192-bit product of the significand with a window of the bits.
fn reduce_large(x: f64) -> (f64, f64, u32) {
    let ux = x.to_bits();
    let e = (ux >> 52).wrapping_sub(1023);
    let off = 0x86usize.wrapping_sub((e >> 3) as usize);
    let m = (ux << 12 >> 12) | (1 << 52);
    let a = load_u64(off);
    let b_lo = load_u64(off + 8);
    let b_hi = load_u64(off + 16);
    let p1 = (a as u128) * (m as u128);
    let mut r8 = p1 as u64;
    let p2 = (b_lo as u128) * (m as u128) + (p1 >> 64);
    let mut r9 = p2 as u64;
    let mut r10 = ((p2 >> 64) as u64).wrapping_add(b_hi.wrapping_mul(m));
    let e7 = e & 7;
    let cl = (54 - e7) as u32;
    let carry = (r10 >> (cl - 1)) & 1;
    let mut region = r10 >> cl;
    let mut sign = 0u64;
    if carry == 1 {
        r10 = !r10;
        r9 = !r9;
        r8 = !r8;
        sign = 0x8000_0000_0000_0000;
    }
    region = (region + carry) & 3;
    let cl = (e7 + 10) as u32;
    r10 = (r10 << cl) >> cl;
    let mut exponent = (e7 as i64) + 10 - 64;
    if r10 == 0 {
        r10 = r9;
        r9 = r8;
        r8 = 0;
        exponent -= 64;
    }
    let top = 63 - r10.leading_zeros() as i64;
    exponent += top;
    let shift = top - 52;
    if shift < 0 {
        let s = (-shift) as u32;
        let carry_bits = r9 >> ((64 - s) & 63);
        r10 = (r10 << s) | carry_bits;
        r9 = (r9 << s) | (r8 >> ((64 - s) & 63));
    } else if shift > 0 {
        let s = shift as u32;
        let low = r10 << ((64 - s) & 63);
        r10 >>= s;
        r9 = (r9 >> s) | low;
    }
    exponent += 1023;
    let hi_bits = (r10 & !(1u64 << 52)) | sign | ((exponent as u64) << 52);
    let hi = d(hi_bits);
    let top9 = 63 - r9.leading_zeros() as i64;
    let s = (64 - top9) as u32 & 63;
    let lo_mant = (r9 << s) >> 12;
    let lo_exp = exponent - (64 - top9 + 52);
    let lo = d(lo_mant | sign | ((lo_exp as u64) << 52));
    let piby2 = d(0x3ff921fb54442d18);
    let piby2_h = d(0x3ff921fb50000000);
    let piby2_t = d(0x3e5110b460000000);
    let hi_head = d(hi_bits & 0xffff_ffff_f800_0000);
    let hi_tail = hi - hi_head;
    let product = hi * piby2;
    let mut t = hi_head * piby2_h - product;
    t = hi_tail.mul_add(piby2_h, t);
    t = hi_head.mul_add(piby2_t, t);
    t = hi_tail.mul_add(piby2_t, t);
    let u = hi.mul_add(d(0x3c91a62633145c06), lo * piby2);
    t += u;
    let r = product + t;
    let rr = (product - r) + t;
    (r, rr, region as u32)
}

/// `sin(x)` as the runtime's FMA3 `sin` gives it.
pub(crate) fn crt_sin(x: f64) -> f64 {
    let ux = x.to_bits();
    let ax = ux & !SIGN;
    if ax >= PI_BY_4 {
        if ax >= INF {
            return if x.is_nan() { d(ux | QNAN_BIT) } else { f64::NAN };
        }
        let (r, rr, region) = if ax >= LARGE { reduce_large(d(ax)) } else { reduce_medium(d(ax)) };
        let v = if region & 1 == 1 { cos_reduced(r, rr) } else { sin_reduced(r, rr) };
        let flip = if region & 2 != 0 { SIGN } else { 0 };
        return d(v.to_bits() ^ (flip ^ (ux & SIGN)));
    }
    if ax >= TWO_POW_M13 {
        let x2 = x * x;
        let p = x2.mul_add(sin_poly(x2), d(SIN_C[0]));
        return (x * x2).mul_add(p, x);
    }
    if ax >= TWO_POW_M27 {
        let x3 = (x * x) * x;
        return (-x3).mul_add(d(0x3fc5555555555555), x);
    }
    x
}

/// `cos(x)` as the runtime's FMA3 `cos` gives it.
pub(crate) fn crt_cos(x: f64) -> f64 {
    let ux = x.to_bits();
    let ax = ux & !SIGN;
    if ax > PI_BY_4 {
        if ax >= INF {
            return if x.is_nan() { d(ux | QNAN_BIT) } else { f64::NAN };
        }
        let (r, rr, region) = if ax >= LARGE { reduce_large(d(ax)) } else { reduce_medium(d(ax)) };
        let v = if region & 1 == 1 { sin_reduced(r, rr) } else { cos_reduced(r, rr) };
        let flip = if (region + 1) & 2 != 0 { SIGN } else { 0 };
        return d(v.to_bits() ^ flip);
    }
    if ax >= TWO_POW_M13 {
        let x2 = x * x;
        let p = x2.mul_add(cos_poly(x2), -0.5);
        return x2.mul_add(p, 1.0);
    }
    if ax >= TWO_POW_M27 {
        return (-(x * 0.5)).mul_add(x, 1.0);
    }
    1.0
}

// ---- sinh and cosh (win-libm sinh.c, cosh.c and splitexp; the fused multiply-adds where
// measurement against the runtime puts them, see the module docs) ----

/// `splitexp(x, 1.0, 32/ln2, ln2/32 lead, ln2/32 trail)`: `e^x = 2^m (z1 + z2)`.
fn splitexp(x: f64) -> (i32, f64, f64) {
    let r = x * d(0x40471547652b82fe); // 32/ln2
    let n = if r > 0.0 { (r + 0.5) as i32 } else { (r - 0.5) as i32 };
    let j = (n & 0x1f) as usize;
    let nd = n as f64;
    let r1 = (-d(0x3f962e42fe000000)).mul_add(nd, x);
    let m = (n - j as i32) / 32;
    let r2 = (-n) as f64 * d(0x3dcf473de6af278e);
    let r = 1.0f64.mul_add(r2, r1);
    let mut p = r.mul_add(d(0x3f56c1728d739765), d(0x3f811115b7aa905e));
    p = r.mul_add(p, d(0x3fa5555555545d4e));
    p = r.mul_add(p, d(0x3fc5555555548f7c));
    p = r.mul_add(p, 0.5);
    let q = (r * r).mul_add(p, r2) + r1;
    let (f1, f2) = (d(EXP2_JBY32_LEAD[j]), d(EXP2_JBY32_TRAIL[j]));
    (m, f1, (f2 + f1).mul_add(q, f2))
}

/// `(z1 + z2) * 2^(m-1)`, scaled in two steps outside the normal exponent range.
fn half_scaled(m: i32, z: f64) -> f64 {
    let m = m - 1;
    if (-1022..=1023).contains(&m) {
        return z * d(((m + 1023) as u64) << 52);
    }
    let m1 = m / 2;
    let m2 = m - m1;
    (z * d(((m1 + 1023) as u64) << 52)) * d(((m2 + 1023) as u64) << 52)
}

/// The sinh(dy) and cosh(dy) polynomials of the fractional part `dy` (`dy2 = dy*dy`): `sdy`
/// without its `dy^3` factor and `cdy` without its `dy2` factor.
fn sinh_cosh_polys(dy2: f64) -> (f64, f64) {
    let mut s = dy2.mul_add(d(0x3d6b4125921ea08b), d(0x3de611cb2bdcb7be));
    for c in [0x3e5ae6460fbe61c0u64, 0x3ec71de3a4e13e7d, 0x3f2a01a01a01ee80, 0x3f811111111110fd, 0x3fc5555555555555] {
        s = dy2.mul_add(s, d(c));
    }
    let mut c = dy2.mul_add(d(0x3da9984b7f63fcd7), d(0x3e21ee56385b7b20));
    for k in [0x3e927e5069f1cb55u64, 0x3efa01a019079011, 0x3f56c16c16c212e5, 0x3fa5555555555502, 0x3fe0000000000000] {
        c = dy2.mul_add(c, d(k));
    }
    (s, c)
}

/// `sinh(x)` as the runtime's FMA3 `sinh` gives it.
pub(crate) fn crt_sinh(x: f64) -> f64 {
    let ux = x.to_bits();
    let aux = ux & !SIGN;
    if aux < 0x3e30_0000_0000_0000 {
        return x;
    }
    if aux >= INF {
        return if aux == INF { x } else { d(ux | QNAN_BIT) };
    }
    let negative = aux != ux;
    let y = d(aux);
    let z = if y >= d(0x408633ce8fb9f87e) {
        f64::INFINITY
    } else if y >= d(0x40420fd21b95825f) {
        let (m, z1, z2) = splitexp(y);
        half_scaled(m, z2 + z1)
    } else {
        let ind = y as i32 as usize;
        let dy = y - ind as f64;
        let dy2 = dy * dy;
        let (sp, cp) = sinh_cosh_polys(dy2);
        let cdy = cp * dy2;
        let dy3 = dy2 * dy;
        let sdy1 = d(dy.to_bits() & 0xffff_ffff_f800_0000);
        let sdy2 = dy3.mul_add(sp, dy - sdy1);
        let (st, ct, cl, sl) = (d(SINH_TAIL[ind]), d(COSH_TAIL[ind]), d(COSH_LEAD[ind]), d(SINH_LEAD[ind]));
        let mut t = sdy2.mul_add(ct, st * cdy);
        t = sdy1.mul_add(ct, t);
        let mut u = t + st;
        u = sdy2.mul_add(cl, u);
        u = cdy.mul_add(sl, u);
        u = sdy1.mul_add(cl, u);
        u + sl
    };
    if negative { -z } else { z }
}

/// `cosh(x)` as the runtime's FMA3 `cosh` gives it.
pub(crate) fn crt_cosh(x: f64) -> f64 {
    let ux = x.to_bits();
    let aux = ux & !SIGN;
    if aux < 0x3e30_0000_0000_0000 {
        return 1.0;
    }
    if aux >= INF {
        return if aux == INF { f64::INFINITY } else { d(ux | QNAN_BIT) };
    }
    let y = d(aux);
    if y >= d(0x408633ce8fb9f87e) {
        return f64::INFINITY;
    }
    if y >= 20.0 {
        let (m, z1, z2) = splitexp(y);
        return half_scaled(m, z2 + z1);
    }
    let ind = y as i32 as usize;
    let dy = y - ind as f64;
    let dy2 = dy * dy;
    let (sp, cp) = sinh_cosh_polys(dy2);
    let sdy = sp * (dy2 * dy);
    let cdy = cp * dy2;
    let (ct, cl, st, sl) = (d(COSH_TAIL[ind]), d(COSH_LEAD[ind]), d(SINH_TAIL[ind]), d(SINH_LEAD[ind]));
    let mut t = cdy.mul_add(ct, st * sdy);
    t = dy.mul_add(st, t);
    let mut u = t + ct;
    u = cdy.mul_add(cl, u);
    u = sdy.mul_add(sl, u);
    u = dy.mul_add(sl, u);
    u + cl
}

// ---- atan2 (win-libm atan2.c; fused multiply-adds as measured) and hypot (win-libm hypot.c,
// FAST_BUT_GREATER_THAN_ONE_ULP: a scaled sqrt(x*x + y*y)) ----

/// `scaleUpDouble1024`: the bits of `x * 2^1024`.
fn scale_up_1024(ux: u64) -> u64 {
    if ux & INF == 0 {
        let y = d(ux | 0x4010_0000_0000_0000);
        let y = if ux & SIGN != 0 { y + 4.0 } else { y - 4.0 };
        return y.to_bits();
    }
    ux.wrapping_add(0x4000_0000_0000_0000)
}

/// `scaleDownDouble`: the bits of `x * 2^-k`, rounded into the subnormal range.
fn scale_down(ux: u64, k: i32) -> u64 {
    let sign = ux & SIGN;
    let ax = ux & !SIGN;
    let n = ((ax & INF) >> 52) as i32 - k;
    let y = if n > 0 {
        (ax & !INF) | ((n as u64) << 52)
    } else {
        let y = (ax & !INF) | 0x0010_0000_0000_0000;
        let shift = 1 - n;
        if shift > 53 {
            0
        } else {
            let y = y >> (shift - 1);
            (y >> 1) + (y & 1)
        }
    };
    y | sign
}

/// `atan2(y, x)` as the runtime's FMA3 `atan2` gives it.
pub(crate) fn crt_atan2(y: f64, x: f64) -> f64 {
    let (mut ux, mut uy) = (x.to_bits(), y.to_bits());
    let (aux, auy) = (ux & !SIGN, uy & !SIGN);
    let mut xexp = ((ux >> 52) & 0x7ff) as i32;
    let mut yexp = ((uy >> 52) & 0x7ff) as i32;
    let (xneg, yneg) = (ux & SIGN != 0, uy & SIGN != 0);
    let pi = d(0x400921fb54442d18);
    let piby2 = d(0x3ff921fb54442d18);
    if aux > INF {
        return d(ux | QNAN_BIT);
    }
    if auy > INF {
        return d(uy | QNAN_BIT);
    }
    if auy == 0 {
        return if xneg { if yneg { -pi } else { pi } } else { y };
    }
    if aux == 0 {
        return if yneg { -piby2 } else { piby2 };
    }
    let (mut x, mut y) = (x, y);
    if xexp < 1021 && yexp < 1021 {
        ux = scale_up_1024(ux);
        uy = scale_up_1024(uy);
        x = d(ux);
        y = d(uy);
        xexp = ((ux >> 52) & 0x7ff) as i32;
        yexp = ((uy >> 52) & 0x7ff) as i32;
    }
    let diffexp = yexp - xexp;
    if diffexp > 56 {
        return if yneg { -piby2 } else { piby2 };
    }
    if diffexp < -28 && !xneg {
        if diffexp < -1074 {
            return if yneg { -0.0 } else { 0.0 };
        }
        if diffexp < -1022 {
            let q = (d(0x4630_0000_0000_0000) * y) / x; // 2^100 * y / x
            return d(scale_down(q.to_bits(), 100));
        }
        return y / x;
    }
    if diffexp < -56 && xneg {
        return if yneg { -pi } else { pi };
    }
    if auy == INF && aux == INF {
        return match (xneg, yneg) {
            (true, true) => d(0xc002d97c7f3321d2),
            (true, false) => d(0x4002d97c7f3321d2),
            (false, true) => d(0xbfe921fb54442d18),
            (false, false) => d(0x3fe921fb54442d18),
        };
    }
    let (mut u, mut v) = (if xneg { -x } else { x }, if yneg { -y } else { y });
    let swap = v > u;
    if swap {
        std::mem::swap(&mut u, &mut v);
    }
    let vbyu = v / u;
    let (mut q1, mut q2);
    if vbyu > 0.0625 {
        let index = vbyu.mul_add(256.0, 0.5) as i32;
        q1 = d(ATAN_JBY256_LEAD[(index - 16) as usize]);
        let tail = d(ATAN_JBY256_TAIL[(index - 16) as usize]);
        let c = index as f64 * d(0x3f70000000000000);
        // scaleDouble_2(., -m) for u's exponent m.
        let neg_m = 0x3ff - ((u.to_bits() >> 52) & 0x7ff) as i32;
        let e1 = neg_m / 2;
        let e2 = neg_m - e1;
        let s1 = d(((e1 + 1023) as u64) << 52);
        let s2 = d(((e2 + 1023) as u64) << 52);
        let us = (s1 * u) * s2;
        let vs = (s1 * v) * s2;
        let u1 = d(us.to_bits() & 0xffff_ffff_f800_0000);
        let mut num = (-u1).mul_add(c, vs);
        num = (-c).mul_add(us - u1, num);
        let den = vs.mul_add(c, us);
        let r = num / den;
        let s = r * r;
        let p = (-s).mul_add(d(0x3fc99992b9802d7d), d(0x3fd5555555550877)) * s;
        q2 = (-r).mul_add(p, r + tail);
    } else if 1e-8 > vbyu {
        q1 = 0.0;
        q2 = vbyu;
    } else {
        let s = vbyu * vbyu;
        let u1 = d(u.to_bits() & 0xffff_ffff_0000_0000);
        let vu1 = d(vbyu.to_bits() & 0xffff_ffff_0000_0000);
        let u2 = u - u1;
        let mut w = (-u1).mul_add(vu1, v);
        w = (-vu1).mul_add(u2, w);
        let vu2 = vbyu - vu1;
        w = (-u).mul_add(vu2, w);
        let mut p = (-s).mul_add(d(0x3fb70c3192d751c2), d(0x3fbc718839b8c4eb));
        for c in [0x3fc2492482bd6be1u64, 0x3fc99999999643a3, 0x3fd5555555555538] {
            p = (-s).mul_add(p, d(c));
        }
        q1 = 0.0;
        q2 = (-(s * vbyu)).mul_add(p, w / u) + vbyu;
    }
    if swap {
        q1 = d(0x3ff921fb54442d18) - q1;
        q2 = d(0x3c91a62633145c07) - q2;
    }
    if xneg {
        q1 = d(0x400921fb50000000) - q1;
        q2 = d(0x3e6110b4611a6263) - q2;
    }
    let q = q1 + q2;
    if yneg { -q } else { q }
}

/// `hypot(x, y)` as the runtime's `hypot` gives it.
pub(crate) fn crt_hypot(x: f64, y: f64) -> f64 {
    let (ax, ay) = (x.to_bits() & !SIGN, y.to_bits() & !SIGN);
    let (ex, ey) = (ax >> 52, ay >> 52);
    if ex == 0x7ff {
        if ax & MANT == 0 || (ey == 0x7ff && ay & MANT == 0) {
            return f64::INFINITY;
        }
        return x * x + y * y;
    }
    if ey == 0x7ff {
        if ay & MANT == 0 {
            return f64::INFINITY;
        }
        return x * x + y * y;
    }
    if ax == 0 {
        return d(ay);
    }
    if ay == 0 {
        return d(ax);
    }
    if ((ex as i32 - ey as i32) + 54) as u32 > 108 {
        return d(ax) + d(ay);
    }
    let (mut a, mut b) = (d(ax), d(ay));
    let mut scale = 0i32;
    if ex > 0x5f3 || ey > 0x5f3 {
        a = d(ax.wrapping_add(0xda80_0000_0000_0000));
        b = d(ay.wrapping_add(0xda80_0000_0000_0000));
        scale = 600;
    } else if ex < 0x20b || ey < 0x20b {
        // Times 2^600; a subnormal through (bits + 601*2^52) - 2^-422.
        let up = |bits: u64, e: u64| {
            if e == 0 { d(bits + 0x2590_0000_0000_0000) + d(0xa590_0000_0000_0000) } else { d(bits + 0x2580_0000_0000_0000) }
        };
        a = up(ax, ex);
        b = up(ay, ey);
        scale = -600;
    }
    let mut r = (a * a + b * b).sqrt();
    if scale != 0 {
        r *= d(((scale + 1023) as u64) << 52);
    }
    r
}

// ---- Tables: log and log10 from AOCL-LibM; exp, sinh, cosh, splitexp and atan2 equal to win-libm's ----

#[rustfmt::skip]
const LOG_LEAD: [u64; 257] = [
    0x0000000000000000, 0x3f6ff00aa0000000, 0x3f7fe02a60000000, 0x3f87dc4750000000,
    0x3f8fc0a8b0000000, 0x3f93cea440000000, 0x3f97b91b00000000, 0x3f9b9fc020000000,
    0x3f9f829b00000000, 0x3fa1b0d980000000, 0x3fa39e87b0000000, 0x3fa58a5ba0000000,
    0x3fa77458f0000000, 0x3fa95c8300000000, 0x3fab42dd70000000, 0x3fad276b80000000,
    0x3faf0a30c0000000, 0x3fb0759830000000, 0x3fb16536e0000000, 0x3fb253f620000000,
    0x3fb341d790000000, 0x3fb42edcb0000000, 0x3fb51b0730000000, 0x3fb60658a0000000,
    0x3fb6f0d280000000, 0x3fb7da7660000000, 0x3fb8c345d0000000, 0x3fb9ab4240000000,
    0x3fba926d30000000, 0x3fbb78c820000000, 0x3fbc5e5480000000, 0x3fbd4313d0000000,
    0x3fbe270760000000, 0x3fbf0a30c0000000, 0x3fbfec9130000000, 0x3fc0671510000000,
    0x3fc0d77e70000000, 0x3fc1478580000000, 0x3fc1b72ad0000000, 0x3fc2266f10000000,
    0x3fc29552f0000000, 0x3fc303d710000000, 0x3fc371fc20000000, 0x3fc3dfc2b0000000,
    0x3fc44d2b60000000, 0x3fc4ba36f0000000, 0x3fc526e5e0000000, 0x3fc59338d0000000,
    0x3fc5ff3070000000, 0x3fc66acd40000000, 0x3fc6d60fe0000000, 0x3fc740f8f0000000,
    0x3fc7ab8900000000, 0x3fc815c0a0000000, 0x3fc87fa060000000, 0x3fc8e928d0000000,
    0x3fc9525a90000000, 0x3fc9bb3620000000, 0x3fca23bc10000000, 0x3fca8becf0000000,
    0x3fcaf3c940000000, 0x3fcb5b5190000000, 0x3fcbc28670000000, 0x3fcc296850000000,
    0x3fcc8ff7c0000000, 0x3fccf63540000000, 0x3fcd5c2160000000, 0x3fcdc1bca0000000,
    0x3fce270760000000, 0x3fce8c0250000000, 0x3fcef0adc0000000, 0x3fcf550a50000000,
    0x3fcfb91860000000, 0x3fd00e6c40000000, 0x3fd0402590000000, 0x3fd071b850000000,
    0x3fd0a324e0000000, 0x3fd0d46b50000000, 0x3fd1058bf0000000, 0x3fd1368700000000,
    0x3fd1675ca0000000, 0x3fd1980d20000000, 0x3fd1c898c0000000, 0x3fd1f8ff90000000,
    0x3fd22941f0000000, 0x3fd2596010000000, 0x3fd2895a10000000, 0x3fd2b93030000000,
    0x3fd2e8e2b0000000, 0x3fd31871c0000000, 0x3fd347dd90000000, 0x3fd3772660000000,
    0x3fd3a64c50000000, 0x3fd3d54fa0000000, 0x3fd4043080000000, 0x3fd432ef20000000,
    0x3fd4618bc0000000, 0x3fd4900680000000, 0x3fd4be5f90000000, 0x3fd4ec9730000000,
    0x3fd51aad80000000, 0x3fd548a2c0000000, 0x3fd5767710000000, 0x3fd5a42ab0000000,
    0x3fd5d1bdb0000000, 0x3fd5ff3070000000, 0x3fd62c82f0000000, 0x3fd659b570000000,
    0x3fd686c810000000, 0x3fd6b3bb20000000, 0x3fd6e08ea0000000, 0x3fd70d42e0000000,
    0x3fd739d7f0000000, 0x3fd7664e10000000, 0x3fd792a550000000, 0x3fd7bede00000000,
    0x3fd7eaf830000000, 0x3fd816f410000000, 0x3fd842d1d0000000, 0x3fd86e9190000000,
    0x3fd89a3380000000, 0x3fd8c5b7c0000000, 0x3fd8f11e80000000, 0x3fd91c67e0000000,
    0x3fd9479410000000, 0x3fd972a340000000, 0x3fd99d9580000000, 0x3fd9c86b00000000,
    0x3fd9f323e0000000, 0x3fda1dc060000000, 0x3fda484090000000, 0x3fda72a490000000,
    0x3fda9cec90000000, 0x3fdac718c0000000, 0x3fdaf12930000000, 0x3fdb1b1e00000000,
    0x3fdb44f770000000, 0x3fdb6eb590000000, 0x3fdb985890000000, 0x3fdbc1e080000000,
    0x3fdbeb4d90000000, 0x3fdc149ff0000000, 0x3fdc3dd7a0000000, 0x3fdc66f4e0000000,
    0x3fdc8ff7c0000000, 0x3fdcb8e070000000, 0x3fdce1af00000000, 0x3fdd0a63a0000000,
    0x3fdd32fe70000000, 0x3fdd5b7f90000000, 0x3fdd83e720000000, 0x3fddac3530000000,
    0x3fddd46a00000000, 0x3fddfc8590000000, 0x3fde248810000000, 0x3fde4c71a0000000,
    0x3fde744260000000, 0x3fde9bfa60000000, 0x3fdec399d0000000, 0x3fdeeb20c0000000,
    0x3fdf128f50000000, 0x3fdf39e5b0000000, 0x3fdf6123f0000000, 0x3fdf884a30000000,
    0x3fdfaf5880000000, 0x3fdfd64f20000000, 0x3fdffd2e00000000, 0x3fe011fab0000000,
    0x3fe02552a0000000, 0x3fe0389ee0000000, 0x3fe04bdf90000000, 0x3fe05f14b0000000,
    0x3fe0723e50000000, 0x3fe0855c80000000, 0x3fe0986f40000000, 0x3fe0ab76b0000000,
    0x3fe0be72e0000000, 0x3fe0d163c0000000, 0x3fe0e44980000000, 0x3fe0f72410000000,
    0x3fe109f390000000, 0x3fe11cb810000000, 0x3fe12f7190000000, 0x3fe1422020000000,
    0x3fe154c3d0000000, 0x3fe1675ca0000000, 0x3fe179eab0000000, 0x3fe18c6e00000000,
    0x3fe19ee6b0000000, 0x3fe1b154b0000000, 0x3fe1c3b810000000, 0x3fe1d610f0000000,
    0x3fe1e85f50000000, 0x3fe1faa340000000, 0x3fe20cdcd0000000, 0x3fe21f0bf0000000,
    0x3fe23130d0000000, 0x3fe2434b60000000, 0x3fe2555bc0000000, 0x3fe2676200000000,
    0x3fe2795e10000000, 0x3fe28b5000000000, 0x3fe29d37f0000000, 0x3fe2af15f0000000,
    0x3fe2c0e9e0000000, 0x3fe2d2b400000000, 0x3fe2e47430000000, 0x3fe2f62a90000000,
    0x3fe307d730000000, 0x3fe3197a00000000, 0x3fe32b1330000000, 0x3fe33ca2b0000000,
    0x3fe34e2890000000, 0x3fe35fa4e0000000, 0x3fe37117b0000000, 0x3fe38280f0000000,
    0x3fe393e0d0000000, 0x3fe3a53730000000, 0x3fe3b68440000000, 0x3fe3c7c7f0000000,
    0x3fe3d90260000000, 0x3fe3ea3390000000, 0x3fe3fb5b80000000, 0x3fe40c7a40000000,
    0x3fe41d8fe0000000, 0x3fe42e9c60000000, 0x3fe43f9fe0000000, 0x3fe4509a50000000,
    0x3fe4618bc0000000, 0x3fe4727430000000, 0x3fe48353d0000000, 0x3fe4942a80000000,
    0x3fe4a4f850000000, 0x3fe4b5bd60000000, 0x3fe4c679a0000000, 0x3fe4d72d30000000,
    0x3fe4e7d810000000, 0x3fe4f87a30000000, 0x3fe50913c0000000, 0x3fe519a4c0000000,
    0x3fe52a2d20000000, 0x3fe53aad00000000, 0x3fe54b2460000000, 0x3fe55b9350000000,
    0x3fe56bf9d0000000, 0x3fe57c57f0000000, 0x3fe58cadb0000000, 0x3fe59cfb20000000,
    0x3fe5ad4040000000, 0x3fe5bd7d30000000, 0x3fe5cdb1d0000000, 0x3fe5ddde50000000,
    0x3fe5ee02a0000000, 0x3fe5fe1ed0000000, 0x3fe60e32f0000000, 0x3fe61e3ef0000000,
    0x3fe62e42e0000000,
];
#[rustfmt::skip]
const LOG_TAIL: [u64; 257] = [
    0x0000000000000000, 0x3db5885e0250435a, 0x3de620cf11f86ed2, 0x3dff0214edba4a25,
    0x3dbf807c79f3db4e, 0x3dea352ba779a52b, 0x3dff56c46aa49fd5, 0x3dfebe465fef5196,
    0x3e0cf0660099f1f8, 0x3e1247b2ff85945d, 0x3e13fd7abf5202b6, 0x3e1f91c9a918d51e,
    0x3e08cb73f118d3ca, 0x3e1d91c7d6fad074, 0x3de1971bec28d14c, 0x3e15b616a423c78a,
    0x3da162a6617cc971, 0x3e166391c4c06d29, 0x3e2d46f5c1d0c4b8, 0x3e2e14282df1f6d3,
    0x3e186f47424a660d, 0x3e2d4c8de077753e, 0x3e2e0c307ed24f1c, 0x3e226ea18763bdd3,
    0x3e25cad69737c933, 0x3e2af62599088901, 0x3e18c66c83d6b2d0, 0x3e1880ceb36fb30f,
    0x3e2495aac6ca17a4, 0x3e2761db4210878c, 0x3e2eb78e862bac2f, 0x3e19b2cd75790dd9,
    0x3e2c55e5cbd3d50f, 0x3db162a6617cc971, 0x3dfdbeabaaa2e519, 0x3e1652cb7150c647,
    0x3e39a11cb2cd2ee2, 0x3e219d0ab1a28813, 0x3e24bd9e80a41811, 0x3e3214b596faa3df,
    0x3e303fea46980bb8, 0x3e31c8ffa5fd28c7, 0x3dce8f743bcd96c5, 0x3dfd98c5395315c6,
    0x3e3996fa3ccfa7b2, 0x3e1cd2af2ad13037, 0x3e1d0da1bd17200e, 0x3e3330410ba68b75,
    0x3df4f27a790e7c41, 0x3e13956a86f6ff1b, 0x3e2c6748723551d9, 0x3e2500de9326cdfc,
    0x3e1086c848df1b59, 0x3e04357ead6836ff, 0x3e24832442408024, 0x3e3d10da8154b13d,
    0x3e39e8ad68ec8260, 0x3e3cfbf706abaf18, 0x3e3fc56ac6326e23, 0x3e39105e3185cf21,
    0x3e3d017fe5b19cc0, 0x3e3d1f6b48dd13fe, 0x3e20b63358a7e73a, 0x3e263063028c211c,
    0x3e2e6a6886b09760, 0x3e3c138bb891cd03, 0x3e369f7722b7221a, 0x3df57d8fac1a628c,
    0x3e3c55e5cbd3d50f, 0x3e1552d2ff48fe2e, 0x3e37b8b26ca431bc, 0x3e292decdc1c5f6d,
    0x3e3abc7c551aaa8c, 0x3e36b540731a354b, 0x3e32d341036b89ef, 0x3e4f9ab21a3a2e0f,
    0x3e239c871afb9fbd, 0x3e3e6add2c81f640, 0x3e435c95aa313f41, 0x3e249d4582f6cc53,
    0x3e47574c1c07398f, 0x3e4ba846dece9e8d, 0x3e16999fafbc68e7, 0x3e4c9145e51b0103,
    0x3e479ef2cb44850a, 0x3e0beec73de11275, 0x3e2ef4351af5a498, 0x3e45713a493b4a50,
    0x3e45c23a61385992, 0x3e42a88309f57299, 0x3e4530faa9ac8ace, 0x3e25fec2d792a758,
    0x3e35a517a71cbcd7, 0x3e3707dc3e1cd9a3, 0x3e3a1a9f8ef43049, 0x3e4409d0276b3674,
    0x3e20e2f613e85bd9, 0x3df0027433001e5f, 0x3e35dde2836d3265, 0x3e2300134d7aaf04,
    0x3e3cb7e0b42724f5, 0x3e2d6e93167e6308, 0x3e3d1569b1526adb, 0x3e0e99fc338a1a41,
    0x3e4eb01394a11b1c, 0x3e04f27a790e7c41, 0x3e25ce3ca97b7af9, 0x3e281f0f940ed857,
    0x3e4d36295d88857c, 0x3e21aca1ec4af526, 0x3e445743c7182726, 0x3e23c491aead337e,
    0x3e3aef401a738931, 0x3e21cede76092a29, 0x3e4fba8f44f82bb4, 0x3e446f5f7f3c3e1a,
    0x3e47055f86c9674b, 0x3e4b41a92b6b6e1a, 0x3e443d162e927628, 0x3e4466174013f9b1,
    0x3e3b05096ad69c62, 0x3e40b169150faa58, 0x3e3cd98b1df85da7, 0x3e468b507b0f8fa8,
    0x3e48422df57499ba, 0x3e11351586970274, 0x3e117e08acba92ee, 0x3e26e04314dd0229,
    0x3e497f3097e56d1a, 0x3e3356e655901286, 0x3e0cb761457f94d6, 0x3e39af67a85a9dac,
    0x3e453410931a909f, 0x3e22c587206058f5, 0x3e223bc358899c22, 0x3e4d7bf8b6d223cb,
    0x3e47991ec5197ddb, 0x3e4a79e6bb3a9219, 0x3e3a4c43ed663ec5, 0x3e461b5a1484f438,
    0x3e4b4e36f7ef0c3a, 0x3e115f026acd0d1b, 0x3e3f36b535cecf05, 0x3e2ffb7fbf3eb5c6,
    0x3e3e6a6886b09760, 0x3e3135eb27f5bbc3, 0x3e470be7d6f6fa57, 0x3e4ce43cc84ab338,
    0x3e4c01d7aac3bd91, 0x3e45c58d07961060, 0x3e3628bcf941456e, 0x3e4c58b2a8461cd2,
    0x3e33071282fb989a, 0x3e420dab6a80f09c, 0x3e44f8d84c397b1e, 0x3e40d0ee08599e48,
    0x3e1d68787e37da36, 0x3e366187d591bafc, 0x3e22346600bae772, 0x3e390377d0d61b8e,
    0x3e4f5e0dd966b907, 0x3e49023cb79a00e2, 0x3e44e05158c28ad8, 0x3e3bfa7b08b18ae4,
    0x3e4ef1e63db35f67, 0x3e0ec2ae39493d4f, 0x3e40afe930ab2fa0, 0x3e225ff8a1810dd4,
    0x3e469743fb1a71a5, 0x3e5f9cc676785571, 0x3e5b524da4cbf982, 0x3e5a4c8b381535b8,
    0x3e5839be809caf2c, 0x3e50968a1cb82c13, 0x3e5eae6a41723fb5, 0x3e5d9c29a380a4db,
    0x3e4094aa0ada625e, 0x3e5973ad6fc108ca, 0x3e4747322fdbab97, 0x3e593692fa9d4221,
    0x3e5c5a992dfbc7d9, 0x3e4e1f33e102387a, 0x3e464fbef14c048c, 0x3e4490f513ca5e3b,
    0x3e37a6af4d4c799d, 0x3e57574c1c07398f, 0x3e57b133417f8c1c, 0x3e5feb9e0c176514,
    0x3e419f25bb3172f7, 0x3e45f68a7bbfb852, 0x3e5ee278497929f1, 0x3e5ccee006109d58,
    0x3e5ce081a07bd8b3, 0x3e570e12981817b8, 0x3e292ab6d93503d0, 0x3e58cb7dd7c3b61e,
    0x3e4efafd0a0b78da, 0x3e5e907267c4288e, 0x3e5d31ef96780875, 0x3e23430dfcd2ad50,
    0x3e344d88d75bc1f9, 0x3e5bec0f055e04fc, 0x3e5d85611590b9ad, 0x3df320568e583229,
    0x3e5a891d1772f538, 0x3e22edc9dabba74d, 0x3e4b9009a1015086, 0x3e52a12a8c5b1a19,
    0x3e3a7885f0fdac85, 0x3e5f4ffcd43ac691, 0x3e52243ae2640aad, 0x3e546513299035d3,
    0x3e5b39c3a62dd725, 0x3e5ba6dd40049f51, 0x3e451d1ed7177409, 0x3e5cb0f2fd7f5216,
    0x3e3ab150cd4e2213, 0x3e5cfd7bf3193844, 0x3e53fff8455f1dbd, 0x3e5fee640b905fc9,
    0x3e54e2adf548084c, 0x3e3b597adc1ecdd2, 0x3e4345bd096d3a75, 0x3e5101b9d2453c8b,
    0x3e508ce55cc8c979, 0x3e5bbf017e595f71, 0x3e37ce733bd393dc, 0x3e233bb0a503f8a1,
    0x3e30e2f613e85bd9, 0x3e5e67555a635b3c, 0x3e2ea88df73d5e8b, 0x3e3d17e03bda18a8,
    0x3e5b607d76044f7e, 0x3e52adc4e71bc2fc, 0x3e5f99dc7362d1d9, 0x3e5473fa008e6a6a,
    0x3e2b75bb09cb0985, 0x3e5ea04dd10b9aba, 0x3e5802d0d6979674, 0x3e174688ccd99094,
    0x3e496f16abb9df22, 0x3e46e66df2aa374f, 0x3e4e66525ea4550a, 0x3e42d02f34f20cbd,
    0x3e46cfce65047188, 0x3e39b78c842d58b8, 0x3e4735e624c24bc9, 0x3e47eba1f7dd1adf,
    0x3e586b3e59f65355, 0x3e1ce38e637f1b4d, 0x3e58d82ec919edc7, 0x3e4c52648ddcfa37,
    0x3e52482ceae1ac12, 0x3e55a312311aba4f, 0x3e411e236329f225, 0x3e5b48c8cd2f246c,
    0x3e6efa39ef35793c,
];
#[rustfmt::skip]
const LOG_F_INV: [u64; 257] = [
    0x4000000000000000, 0x3fffe01fe01fe020, 0x3fffc07f01fc07f0, 0x3fffa11caa01fa12,
    0x3fff81f81f81f820, 0x3fff6310aca0dbb5, 0x3fff44659e4a4271, 0x3fff25f644230ab5,
    0x3fff07c1f07c1f08, 0x3ffee9c7f8458e02, 0x3ffecc07b301ecc0, 0x3ffeae807aba01eb,
    0x3ffe9131abf0b767, 0x3ffe741aa59750e4, 0x3ffe573ac901e574, 0x3ffe3a9179dc1a73,
    0x3ffe1e1e1e1e1e1e, 0x3ffe01e01e01e01e, 0x3ffde5d6e3f8868a, 0x3ffdca01dca01dca,
    0x3ffdae6076b981db, 0x3ffd92f2231e7f8a, 0x3ffd77b654b82c34, 0x3ffd5cac807572b2,
    0x3ffd41d41d41d41d, 0x3ffd272ca3fc5b1a, 0x3ffd0cb58f6ec074, 0x3ffcf26e5c44bfc6,
    0x3ffcd85689039b0b, 0x3ffcbe6d9601cbe7, 0x3ffca4b3055ee191, 0x3ffc8b265afb8a42,
    0x3ffc71c71c71c71c, 0x3ffc5894d10d4986, 0x3ffc3f8f01c3f8f0, 0x3ffc26b5392ea01c,
    0x3ffc0e070381c0e0, 0x3ffbf583ee868d8b, 0x3ffbdd2b899406f7, 0x3ffbc4fd65883e7b,
    0x3ffbacf914c1bad0, 0x3ffb951e2b18ff23, 0x3ffb7d6c3dda338b, 0x3ffb65e2e3beee05,
    0x3ffb4e81b4e81b4f, 0x3ffb37484ad806ce, 0x3ffb2036406c80d9, 0x3ffb094b31d922a4,
    0x3ffaf286bca1af28, 0x3ffadbe87f94905e, 0x3ffac5701ac5701b, 0x3ffaaf1d2f87ebfd,
    0x3ffa98ef606a63be, 0x3ffa82e65130e159, 0x3ffa6d01a6d01a6d, 0x3ffa574107688a4a,
    0x3ffa41a41a41a41a, 0x3ffa2c2a87c51ca0, 0x3ffa16d3f97a4b02, 0x3ffa01a01a01a01a,
    0x3ff9ec8e951033d9, 0x3ff9d79f176b682d, 0x3ff9c2d14ee4a102, 0x3ff9ae24ea5510da,
    0x3ff999999999999a, 0x3ff9852f0d8ec0ff, 0x3ff970e4f80cb872, 0x3ff95cbb0be377ae,
    0x3ff948b0fcd6e9e0, 0x3ff934c67f9b2ce6, 0x3ff920fb49d0e229, 0x3ff90d4f120190d5,
    0x3ff8f9c18f9c18fa, 0x3ff8e6527af1373f, 0x3ff8d3018d3018d3, 0x3ff8bfce8062ff3a,
    0x3ff8acb90f6bf3aa, 0x3ff899c0f601899c, 0x3ff886e5f0abb04a, 0x3ff87427bcc092b9,
    0x3ff8618618618618, 0x3ff84f00c2780614, 0x3ff83c977ab2bedd, 0x3ff82a4a0182a4a0,
    0x3ff8181818181818, 0x3ff8060180601806, 0x3ff7f405fd017f40, 0x3ff7e225515a4f1d,
    0x3ff7d05f417d05f4, 0x3ff7beb3922e017c, 0x3ff7ad2208e0ecc3, 0x3ff79baa6bb6398b,
    0x3ff78a4c8178a4c8, 0x3ff77908119ac60d, 0x3ff767dce434a9b1, 0x3ff756cac201756d,
    0x3ff745d1745d1746, 0x3ff734f0c541fe8d, 0x3ff724287f46debc, 0x3ff713786d9c7c09,
    0x3ff702e05c0b8170, 0x3ff6f26016f26017, 0x3ff6e1f76b4337c7, 0x3ff6d1a62681c861,
    0x3ff6c16c16c16c17, 0x3ff6b1490aa31a3d, 0x3ff6a13cd1537290, 0x3ff691473a88d0c0,
    0x3ff6816816816817, 0x3ff6719f3601671a, 0x3ff661ec6a5122f9, 0x3ff6524f853b4aa3,
    0x3ff642c8590b2164, 0x3ff63356b88ac0de, 0x3ff623fa77016240, 0x3ff614b36831ae94,
    0x3ff6058160581606, 0x3ff5f66434292dfc, 0x3ff5e75bb8d015e7, 0x3ff5d867c3ece2a5,
    0x3ff5c9882b931057, 0x3ff5babcc647fa91, 0x3ff5ac056b015ac0, 0x3ff59d61f123ccaa,
    0x3ff58ed2308158ed, 0x3ff5805601580560, 0x3ff571ed3c506b3a, 0x3ff56397ba7c52e2,
    0x3ff5555555555555, 0x3ff54725e6bb82fe, 0x3ff5390948f40feb, 0x3ff52aff56a8054b,
    0x3ff51d07eae2f815, 0x3ff50f22e111c4c5, 0x3ff5015015015015, 0x3ff4f38f62dd4c9b,
    0x3ff4e5e0a72f0539, 0x3ff4d843bedc2c4c, 0x3ff4cab88725af6e, 0x3ff4bd3edda68fe1,
    0x3ff4afd6a052bf5b, 0x3ff4a27fad76014a, 0x3ff49539e3b2d067, 0x3ff4880522014880,
    0x3ff47ae147ae147b, 0x3ff46dce34596066, 0x3ff460cbc7f5cf9a, 0x3ff453d9e2c776ca,
    0x3ff446f86562d9fb, 0x3ff43a2730abee4d, 0x3ff42d6625d51f87, 0x3ff420b5265e5951,
    0x3ff4141414141414, 0x3ff40782d10e6566, 0x3ff3fb013fb013fb, 0x3ff3ee8f42a5af07,
    0x3ff3e22cbce4a902, 0x3ff3d5d991aa75c6, 0x3ff3c995a47babe7, 0x3ff3bd60d9232955,
    0x3ff3b13b13b13b14, 0x3ff3a524387ac822, 0x3ff3991c2c187f63, 0x3ff38d22d366088e,
    0x3ff3813813813814, 0x3ff3755bd1c945ee, 0x3ff3698df3de0748, 0x3ff35dce5f9f2af8,
    0x3ff3521cfb2b78c1, 0x3ff34679ace01346, 0x3ff33ae45b57bcb2, 0x3ff32f5ced6a1dfa,
    0x3ff323e34a2b10bf, 0x3ff3187758e9ebb6, 0x3ff30d190130d190, 0x3ff301c82ac40260,
    0x3ff2f684bda12f68, 0x3ff2eb4ea1fed14b, 0x3ff2e025c04b8097, 0x3ff2d50a012d50a0,
    0x3ff2c9fb4d812ca0, 0x3ff2bef98e5a3711, 0x3ff2b404ad012b40, 0x3ff2a91c92f3c105,
    0x3ff29e4129e4129e, 0x3ff293725bb804a5, 0x3ff288b01288b013, 0x3ff27dfa38a1ce4d,
    0x3ff27350b8812735, 0x3ff268b37cd60127, 0x3ff25e22708092f1, 0x3ff2539d7e9177b2,
    0x3ff2492492492492, 0x3ff23eb79717605b, 0x3ff23456789abcdf, 0x3ff22a0122a0122a,
    0x3ff21fb78121fb78, 0x3ff21579804855e6, 0x3ff20b470c67c0d9, 0x3ff2012012012012,
    0x3ff1f7047dc11f70, 0x3ff1ecf43c7fb84c, 0x3ff1e2ef3b3fb874, 0x3ff1d8f5672e4abd,
    0x3ff1cf06ada2811d, 0x3ff1c522fc1ce059, 0x3ff1bb4a4046ed29, 0x3ff1b17c67f2bae3,
    0x3ff1a7b9611a7b96, 0x3ff19e0119e0119e, 0x3ff19453808ca29c, 0x3ff18ab083902bdb,
    0x3ff1811811811812, 0x3ff1778a191bd684, 0x3ff16e0689427379, 0x3ff1648d50fc3201,
    0x3ff15b1e5f75270d, 0x3ff151b9a3fdd5c9, 0x3ff1485f0e0acd3b, 0x3ff13f0e8d344724,
    0x3ff135c81135c811, 0x3ff12c8b89edc0ac, 0x3ff12358e75d3033, 0x3ff11a3019a74826,
    0x3ff1111111111111, 0x3ff107fbbe011080, 0x3ff0fef010fef011, 0x3ff0f5edfab325a2,
    0x3ff0ecf56be69c90, 0x3ff0e40655826011, 0x3ff0db20a88f4696, 0x3ff0d24456359e3a,
    0x3ff0c9714fbcda3b, 0x3ff0c0a7868b4171, 0x3ff0b7e6ec259dc8, 0x3ff0af2f722eecb5,
    0x3ff0a6810a6810a7, 0x3ff09ddba6af8360, 0x3ff0953f39010954, 0x3ff08cabb37565e2,
    0x3ff0842108421084, 0x3ff07b9f29b8eae2, 0x3ff073260a47f7c6, 0x3ff06ab59c7912fb,
    0x3ff0624dd2f1a9fc, 0x3ff059eea0727586, 0x3ff05197f7d73404, 0x3ff04949cc1664c5,
    0x3ff0410410410410, 0x3ff038c6b78247fc, 0x3ff03091b51f5e1a, 0x3ff02864fc7729e9,
    0x3ff0204081020408, 0x3ff0182436517a37, 0x3ff0101010101010, 0x3ff0080402010080,
    0x3ff0000000000000,
];
#[rustfmt::skip]
const LOG10_LEAD: [u64; 257] = [
    0x0000000000000000, 0x3f5bbd9e90000000, 0x3f6bafd470000000, 0x3f74b99560000000,
    0x3f7b9476a0000000, 0x3f81344da0000000, 0x3f849b0850000000, 0x3f87fe71c0000000,
    0x3f8b5e9080000000, 0x3f8ebb6af0000000, 0x3f910a83a0000000, 0x3f92b5b5e0000000,
    0x3f945f4f50000000, 0x3f96075300000000, 0x3f97adc3d0000000, 0x3f9952a4f0000000,
    0x3f9af5f920000000, 0x3f9c97c370000000, 0x3f9e3806a0000000, 0x3f9fd6c5b0000000,
    0x3fa0ba01a0000000, 0x3fa187e120000000, 0x3fa25502c0000000, 0x3fa32167c0000000,
    0x3fa3ed1190000000, 0x3fa4b80180000000, 0x3fa58238e0000000, 0x3fa64bb910000000,
    0x3fa7148340000000, 0x3fa7dc98c0000000, 0x3fa8a3fad0000000, 0x3fa96aaac0000000,
    0x3faa30a9d0000000, 0x3faaf5f920000000, 0x3fabba9a00000000, 0x3fac7e8d90000000,
    0x3fad41d510000000, 0x3fae0471a0000000, 0x3faec66470000000, 0x3faf87aeb0000000,
    0x3fb02428c0000000, 0x3fb08426f0000000, 0x3fb0e3d290000000, 0x3fb1432c30000000,
    0x3fb1a23440000000, 0x3fb200eb60000000, 0x3fb25f5210000000, 0x3fb2bd68e0000000,
    0x3fb31b3050000000, 0x3fb378a8e0000000, 0x3fb3d5d330000000, 0x3fb432afa0000000,
    0x3fb48f3ed0000000, 0x3fb4eb8120000000, 0x3fb5477730000000, 0x3fb5a32160000000,
    0x3fb5fe8040000000, 0x3fb6599440000000, 0x3fb6b45df0000000, 0x3fb70eddb0000000,
    0x3fb7691400000000, 0x3fb7c30160000000, 0x3fb81ca630000000, 0x3fb8760300000000,
    0x3fb8cf1830000000, 0x3fb927e640000000, 0x3fb9806d90000000, 0x3fb9d8aea0000000,
    0x3fba30a9d0000000, 0x3fba885fa0000000, 0x3fbadfd070000000, 0x3fbb36fcb0000000,
    0x3fbb8de4d0000000, 0x3fbbe48930000000, 0x3fbc3aea40000000, 0x3fbc910870000000,
    0x3fbce6e410000000, 0x3fbd3c7da0000000, 0x3fbd91d580000000, 0x3fbde6ec00000000,
    0x3fbe3bc1a0000000, 0x3fbe9056b0000000, 0x3fbee4aba0000000, 0x3fbf38c0c0000000,
    0x3fbf8c9680000000, 0x3fbfe02d30000000, 0x3fc019c2a0000000, 0x3fc0434f70000000,
    0x3fc06cbd60000000, 0x3fc0960c80000000, 0x3fc0bf3d00000000, 0x3fc0e84f10000000,
    0x3fc11142f0000000, 0x3fc13a18a0000000, 0x3fc162d080000000, 0x3fc18b6a90000000,
    0x3fc1b3e710000000, 0x3fc1dc4630000000, 0x3fc2048810000000, 0x3fc22cace0000000,
    0x3fc254b4d0000000, 0x3fc27c9ff0000000, 0x3fc2a46e80000000, 0x3fc2cc20b0000000,
    0x3fc2f3b690000000, 0x3fc31b3050000000, 0x3fc3428e20000000, 0x3fc369d020000000,
    0x3fc390f680000000, 0x3fc3b80160000000, 0x3fc3def0e0000000, 0x3fc405c530000000,
    0x3fc42c7e70000000, 0x3fc4531cd0000000, 0x3fc479a070000000, 0x3fc4a00970000000,
    0x3fc4c65800000000, 0x3fc4ec8c30000000, 0x3fc512a640000000, 0x3fc538a630000000,
    0x3fc55e8c50000000, 0x3fc5845890000000, 0x3fc5aa0b40000000, 0x3fc5cfa470000000,
    0x3fc5f52440000000, 0x3fc61a8ad0000000, 0x3fc63fd850000000, 0x3fc6650cd0000000,
    0x3fc68a2880000000, 0x3fc6af2b80000000, 0x3fc6d415e0000000, 0x3fc6f8e7d0000000,
    0x3fc71da170000000, 0x3fc74242e0000000, 0x3fc766cc40000000, 0x3fc78b3da0000000,
    0x3fc7af9730000000, 0x3fc7d3d910000000, 0x3fc7f80350000000, 0x3fc81c1620000000,
    0x3fc8401190000000, 0x3fc863f5c0000000, 0x3fc887c2e0000000, 0x3fc8ab7900000000,
    0x3fc8cf1830000000, 0x3fc8f2a0a0000000, 0x3fc9161270000000, 0x3fc9396db0000000,
    0x3fc95cb280000000, 0x3fc97fe100000000, 0x3fc9a2f950000000, 0x3fc9c5fb70000000,
    0x3fc9e8e7b0000000, 0x3fca0bbdf0000000, 0x3fca2e7e80000000, 0x3fca512960000000,
    0x3fca73bea0000000, 0x3fca963e70000000, 0x3fcab8a8f0000000, 0x3fcadafe20000000,
    0x3fcafd3e30000000, 0x3fcb1f6930000000, 0x3fcb417f40000000, 0x3fcb638070000000,
    0x3fcb856cf0000000, 0x3fcba744b0000000, 0x3fcbc907f0000000, 0x3fcbeab6c0000000,
    0x3fcc0c5130000000, 0x3fcc2dd750000000, 0x3fcc4f4950000000, 0x3fcc70a740000000,
    0x3fcc91f130000000, 0x3fccb32740000000, 0x3fccd44980000000, 0x3fccf55810000000,
    0x3fcd165300000000, 0x3fcd373a60000000, 0x3fcd580e60000000, 0x3fcd78cf00000000,
    0x3fcd997c70000000, 0x3fcdba16a0000000, 0x3fcdda9dd0000000, 0x3fcdfb11f0000000,
    0x3fce1b7330000000, 0x3fce3bc1a0000000, 0x3fce5bfd50000000, 0x3fce7c2660000000,
    0x3fce9c3ce0000000, 0x3fcebc40e0000000, 0x3fcedc3280000000, 0x3fcefc11d0000000,
    0x3fcf1bdee0000000, 0x3fcf3b99d0000000, 0x3fcf5b42a0000000, 0x3fcf7ad980000000,
    0x3fcf9a5e70000000, 0x3fcfb9d190000000, 0x3fcfd932f0000000, 0x3fcff882a0000000,
    0x3fd00be050000000, 0x3fd01b76a0000000, 0x3fd02b0430000000, 0x3fd03a8910000000,
    0x3fd04a0540000000, 0x3fd05978e0000000, 0x3fd068e3f0000000, 0x3fd0784670000000,
    0x3fd087a080000000, 0x3fd096f210000000, 0x3fd0a63b30000000, 0x3fd0b57bf0000000,
    0x3fd0c4b450000000, 0x3fd0d3e460000000, 0x3fd0e30c30000000, 0x3fd0f22bc0000000,
    0x3fd1014310000000, 0x3fd1105240000000, 0x3fd11f5940000000, 0x3fd12e5830000000,
    0x3fd13d4f00000000, 0x3fd14c3dd0000000, 0x3fd15b24a0000000, 0x3fd16a0370000000,
    0x3fd178da50000000, 0x3fd187a940000000, 0x3fd1967060000000, 0x3fd1a52fa0000000,
    0x3fd1b3e710000000, 0x3fd1c296c0000000, 0x3fd1d13eb0000000, 0x3fd1dfdef0000000,
    0x3fd1ee7770000000, 0x3fd1fd0860000000, 0x3fd20b91a0000000, 0x3fd21a1350000000,
    0x3fd2288d70000000, 0x3fd2370010000000, 0x3fd2456b30000000, 0x3fd253ced0000000,
    0x3fd2622b00000000, 0x3fd2707fd0000000, 0x3fd27ecd40000000, 0x3fd28d1360000000,
    0x3fd29b5220000000, 0x3fd2a989a0000000, 0x3fd2b7b9e0000000, 0x3fd2c5e2e0000000,
    0x3fd2d404b0000000, 0x3fd2e21f50000000, 0x3fd2f032c0000000, 0x3fd2fe3f20000000,
    0x3fd30c4470000000, 0x3fd31a42b0000000, 0x3fd32839e0000000, 0x3fd3362a10000000,
    0x3fd3441350000000,
];
#[rustfmt::skip]
const LOG10_TAIL: [u64; 257] = [
    0x0000000000000000, 0x3db20abc22b2208f, 0x3db10f69332e0dd4, 0x3dce950de87ed257,
    0x3dd3f3443b626d69, 0x3df45aeaa5363e57, 0x3dc443683ce1bf0b, 0x3df989cd60c6a511,
    0x3dfd626f201f2e9f, 0x3de94f8bb8dabdcd, 0x3e0088d8ef423015, 0x3e080413a62b79ad,
    0x3e059717c0eed3c4, 0x3dad4a77add44902, 0x3e0e763ff037300e, 0x3de162d74706f6c3,
    0x3e0601cc1f4dbc14, 0x3deaf3e051f6e5bf, 0x3e097a0b1e1af3eb, 0x3dc0a38970c002c7,
    0x3e102e000057c751, 0x3e155b00eecd6e0e, 0x3ddf86297003b5af, 0x3e1057b9b336a36d,
    0x3e134bc84a06ea4f, 0x3e1643da9ea1bcad, 0x3e1d66a7b4f7ea2a, 0x3df6b2e038f7fcef,
    0x3df3e954c670f088, 0x3e047209093acab3, 0x3e1d708fe7275da7, 0x3e1fdf9e7771b9e7,
    0x3e0827bfa70a0660, 0x3e1601cc1f4dbc14, 0x3e0637f6106a5e5b, 0x3e126a13f17c624b,
    0x3e093eb2ce80623a, 0x3e1430d1e91594de, 0x3e1d6b10108fa031, 0x3e16879c0bbaf241,
    0x3dff08015ea6bc2b, 0x3e29b63dcdc6676c, 0x3e2b022cbcc4ab2c, 0x3df917d07ddd6544,
    0x3e1540605703379e, 0x3e0cd18b947a1b60, 0x3e17ad65277ca97e, 0x3e11884dc59f5fa9,
    0x3e1711c46006d082, 0x3e2f092e3c3108f8, 0x3e1714c5e32be13a, 0x3e26bba7fd734f9a,
    0x3dfdf48fb5e08483, 0x3e232f9bc74d0b95, 0x3df973e848790c13, 0x3e1eccbc08c6586e,
    0x3e2115e9f9524a98, 0x3e2f1740593131b8, 0x3e1bcf8b25643835, 0x3e1f5fa81d8bed80,
    0x3e244a4df929d9e4, 0x3e129820d8220c94, 0x3e2a0b489304e309, 0x3e1f4d56aba665fe,
    0x3e210c9019365163, 0x3df80f78fe592736, 0x3e10528825c81cca, 0x3de095537d6d746a,
    0x3e1827bfa70a0660, 0x3e06b0a8ec45933c, 0x3e105af81bf5dba9, 0x3e17e2fa2655d515,
    0x3e0d59ecbfaee4bf, 0x3e1d8b2fda683fa3, 0x3e24b8ddfd3a3737, 0x3e13827e61ae1204,
    0x3e2c8c7b49e90f9f, 0x3e29eaf01597591d, 0x3e19aaa66e317b36, 0x3e2e725609720655,
    0x3e261c33fc7aac54, 0x3e29662bcf61a252, 0x3e1843c811c42730, 0x3e2064bb0b5acb36,
    0x3e0a340c842701a4, 0x3e1a8e55b58f79d6, 0x3de92d219c5e9d9a, 0x3e3f63e60d7ffd6a,
    0x3e2e9b0ed9516314, 0x3e2923901962350c, 0x3e326f8838785e81, 0x3e3b5b6a4caba6af,
    0x3df0226adc8e761c, 0x3e3c4ad7313a1aed, 0x3e1564e87c738d17, 0x3e338fecf18a6618,
    0x3e3d929ef5777666, 0x3e39483bf08da0b8, 0x3e3bdd0eeeaa5826, 0x3e39c4dd590237ba,
    0x3e1af3e9e0ebcac7, 0x3e35ce5382270dac, 0x3e394f74532ab9ba, 0x3e07342795888654,
    0x3e0c5a000be34bf0, 0x3e2711c46006d082, 0x3e250025b4ed8cf8, 0x3e2ed18bcef2d2a0,
    0x3e21282e0c0a7554, 0x3e0d70f33359a7ca, 0x3e2b7f7e13a84025, 0x3e33306ec321891e,
    0x3e3fc7f8038b7550, 0x3e3eb0358cd71d64, 0x3e3a76c822859474, 0x3e3d0ec652de86e3,
    0x3e2fa4cce08658af, 0x3e3b84a2d2c00a9e, 0x3e20a5b0f2c25bd1, 0x3e3dd660225bf699,
    0x3e08b10f859bf037, 0x3e3e8823b590cbe1, 0x3e361311f31e96f6, 0x3e2e1f875ca20f9a,
    0x3e2c95724939b9a5, 0x3e3805957a3e58e2, 0x3e2ff126ea9f0334, 0x3e3953f5598e5609,
    0x3e36c16ff856c448, 0x3e24cb220ff261f4, 0x3e35e120d53d53a2, 0x3e3a527f6189f256,
    0x3e3856fcffd49c0f, 0x3e300c2e8228d7da, 0x3df113d09444dfe0, 0x3e2510630eea59a6,
    0x3e262e780f32d711, 0x3ded3ed91a10f8cf, 0x3e23654a7e4bcd85, 0x3e055b784980ad21,
    0x3e212f2dd4b16e64, 0x3e37c4add939f50c, 0x3e281784627180fc, 0x3dea5162c7e14961,
    0x3e310c9019365163, 0x3e373c4d2ba17688, 0x3e2ae8a5e0e93d81, 0x3e2ab0c6f01621af,
    0x3e301e8b74dd5b66, 0x3e2d206fecbb5494, 0x3df0b48b724fcc00, 0x3e3f831f0b61e229,
    0x3df81a97c407bcaf, 0x3e3e286c1ccbb7aa, 0x3e28630b49220a93, 0x3dff0b15c1a22c5c,
    0x3e355445e71c0946, 0x3e3be630f8066d85, 0x3e2599dff0d96c39, 0x3e36cc85b18fb081,
    0x3e34476d001ea8c8, 0x3e373f889e16d31f, 0x3e3357100d792a87, 0x3e3bd179ae6101f6,
    0x3e0ca31056c3f6e2, 0x3e3d2870629c08fb, 0x3e3aba3880d2673f, 0x3e2c3633cb297da6,
    0x3e21843899efea02, 0x3e3bccc99d2008e6, 0x3e38000544bdd350, 0x3e2b91c226606ae1,
    0x3e2a7adf26b62bdf, 0x3e18764fc8826ec9, 0x3e1f4f3de50f68f0, 0x3df760ca757995e3,
    0x3dfc667ed3805147, 0x3e3733f6196adf6f, 0x3e2fb710f33e836b, 0x3e39886eba641013,
    0x3dfb5368d0af8c1a, 0x3e358c691b8d2971, 0x3dfe9465226d08fb, 0x3e33587e063f0097,
    0x3e3618e702129f18, 0x3e361c33fc7aac54, 0x3e3f07a68408604a, 0x3e3c34bfe4945421,
    0x3e38b1f00e41300b, 0x3e3f434284d61b63, 0x3e3a63095e397436, 0x3e34428656b919de,
    0x3e36ca9201b2d9a6, 0x3e2738823a2a931c, 0x3e3c11880e179230, 0x3e313ddc8d6d52fe,
    0x3e33eed58922e917, 0x3e295992846bdd50, 0x3e0ddb4d5f2e278b, 0x3df1a5f12a0635c4,
    0x3e4642f0882c3c34, 0x3e2aee9ba7f6475e, 0x3e264b7f834a60e4, 0x3e290d42e243792e,
    0x3e4c272008134f01, 0x3e4a782e16d6cf5b, 0x3e44505c79da6648, 0x3e4ca9d4ea4dcd21,
    0x3e297d3d627cd5bc, 0x3e20b15cf9bcaa13, 0x3e315b2063cf76dd, 0x3e2983e6f3aa2748,
    0x3e3f4c64f4ffe994, 0x3e46beba7ce85a0f, 0x3e3b9c69fd4ea6b8, 0x3e2b6aa5835fa4ab,
    0x3e43ccc3790fedd1, 0x3e29c04cc4404fe0, 0x3e40734b7a75d89d, 0x3e1b4404c4e01612,
    0x3e40c565c2ce4894, 0x3e33c71441d935cd, 0x3d72a492556b3b4e, 0x3e20fa090341dc43,
    0x3e2e8f7009e3d9f4, 0x3e4b1bf68b048a45, 0x3e3eee52dffaa956, 0x3e456b0900e465bd,
    0x3e4d929ef5777666, 0x3e486ea28637e260, 0x3e4665aff10ca2f0, 0x3e2f11fdaf48ec74,
    0x3e4cbe1b86a4d1c7, 0x3e25b05bfea87665, 0x3e41cec20a1a4a1d, 0x3e41cd5f0a409b9f,
    0x3e453656c8265070, 0x3e377ed835282260, 0x3e2417bc3040b9d2, 0x3e408eef7b79eff2,
    0x3e4dc76f39dc57e9, 0x3e4c0493a70cf457, 0x3e4a83d6cea5a60c, 0x3e30d6700dc557ba,
    0x3e44c96c12e8bd0a, 0x3e3d2c1993e32315, 0x3e22c721135f8242, 0x3e279a3e4dda747d,
    0x3dfcf89f6941a72b, 0x3e2149a702f10831, 0x3e4ead4b7c8175db, 0x3e4e6930fe63e70a,
    0x3e41e106bed9ee2f, 0x3e2d682b82f11c92, 0x3e3a07f188dba47c, 0x3e40f9342dc172f6,
    0x3e03ef3fde623e25,
];
#[rustfmt::skip]
const EXP_2_JBY64: [u64; 64] = [
    0x3ff0000000000000, 0x3ff02c9a3e778061, 0x3ff059b0d3158574, 0x3ff0874518759bc8,
    0x3ff0b5586cf9890f, 0x3ff0e3ec32d3d1a2, 0x3ff11301d0125b51, 0x3ff1429aaea92de0,
    0x3ff172b83c7d517b, 0x3ff1a35beb6fcb75, 0x3ff1d4873168b9aa, 0x3ff2063b88628cd6,
    0x3ff2387a6e756238, 0x3ff26b4565e27cdd, 0x3ff29e9df51fdee1, 0x3ff2d285a6e4030b,
    0x3ff306fe0a31b715, 0x3ff33c08b26416ff, 0x3ff371a7373aa9cb, 0x3ff3a7db34e59ff7,
    0x3ff3dea64c123422, 0x3ff4160a21f72e2a, 0x3ff44e086061892d, 0x3ff486a2b5c13cd0,
    0x3ff4bfdad5362a27, 0x3ff4f9b2769d2ca7, 0x3ff5342b569d4f82, 0x3ff56f4736b527da,
    0x3ff5ab07dd485429, 0x3ff5e76f15ad2148, 0x3ff6247eb03a5585, 0x3ff6623882552225,
    0x3ff6a09e667f3bcd, 0x3ff6dfb23c651a2f, 0x3ff71f75e8ec5f74, 0x3ff75feb564267c9,
    0x3ff7a11473eb0187, 0x3ff7e2f336cf4e62, 0x3ff82589994cce13, 0x3ff868d99b4492ed,
    0x3ff8ace5422aa0db, 0x3ff8f1ae99157736, 0x3ff93737b0cdc5e5, 0x3ff97d829fde4e50,
    0x3ff9c49182a3f090, 0x3ffa0c667b5de565, 0x3ffa5503b23e255d, 0x3ffa9e6b5579fdbf,
    0x3ffae89f995ad3ad, 0x3ffb33a2b84f15fb, 0x3ffb7f76f2fb5e47, 0x3ffbcc1e904bc1d2,
    0x3ffc199bdd85529c, 0x3ffc67f12e57d14b, 0x3ffcb720dcef9069, 0x3ffd072d4a07897c,
    0x3ffd5818dcfba487, 0x3ffda9e603db3285, 0x3ffdfc97337b9b5f, 0x3ffe502ee78b3ff6,
    0x3ffea4afa2a490da, 0x3ffefa1bee615a27, 0x3fff50765b6e4540, 0x3fffa7c1819e90d8,
];
#[rustfmt::skip]
const EXP_2_JBY64_HEAD: [u64; 64] = [
    0x3ff0000000000000, 0x3ff02c9a30000000, 0x3ff059b0d0000000, 0x3ff0874510000000,
    0x3ff0b55860000000, 0x3ff0e3ec30000000, 0x3ff11301d0000000, 0x3ff1429aa0000000,
    0x3ff172b830000000, 0x3ff1a35be0000000, 0x3ff1d48730000000, 0x3ff2063b80000000,
    0x3ff2387a60000000, 0x3ff26b4560000000, 0x3ff29e9df0000000, 0x3ff2d285a0000000,
    0x3ff306fe00000000, 0x3ff33c08b0000000, 0x3ff371a730000000, 0x3ff3a7db30000000,
    0x3ff3dea640000000, 0x3ff4160a20000000, 0x3ff44e0860000000, 0x3ff486a2b0000000,
    0x3ff4bfdad0000000, 0x3ff4f9b270000000, 0x3ff5342b50000000, 0x3ff56f4730000000,
    0x3ff5ab07d0000000, 0x3ff5e76f10000000, 0x3ff6247eb0000000, 0x3ff6623880000000,
    0x3ff6a09e60000000, 0x3ff6dfb230000000, 0x3ff71f75e0000000, 0x3ff75feb50000000,
    0x3ff7a11470000000, 0x3ff7e2f330000000, 0x3ff8258990000000, 0x3ff868d990000000,
    0x3ff8ace540000000, 0x3ff8f1ae90000000, 0x3ff93737b0000000, 0x3ff97d8290000000,
    0x3ff9c49180000000, 0x3ffa0c6670000000, 0x3ffa5503b0000000, 0x3ffa9e6b50000000,
    0x3ffae89f90000000, 0x3ffb33a2b0000000, 0x3ffb7f76f0000000, 0x3ffbcc1e90000000,
    0x3ffc199bd0000000, 0x3ffc67f120000000, 0x3ffcb720d0000000, 0x3ffd072d40000000,
    0x3ffd5818d0000000, 0x3ffda9e600000000, 0x3ffdfc9730000000, 0x3ffe502ee0000000,
    0x3ffea4afa0000000, 0x3ffefa1be0000000, 0x3fff507650000000, 0x3fffa7c180000000,
];
#[rustfmt::skip]
const EXP_2_JBY64_TAIL: [u64; 64] = [
    0x0000000000000000, 0x3e6cef00c1dcdef9, 0x3e48ac2ba1d73e2a, 0x3e60eb37901186be,
    0x3e69f3121ec53172, 0x3e469e8d10103a17, 0x3df25b50a4ebbf1a, 0x3e6d525bbf668203,
    0x3e68faa2f5b9bef9, 0x3e66df96ea796d31, 0x3e368b9aa7805b80, 0x3e60c519ac771dd6,
    0x3e6ceac470cd83f5, 0x3e5789f37495e99c, 0x3e547f7b84b09745, 0x3e5b900c2d002475,
    0x3e64636e2a5bd1ab, 0x3e4320b7fa64e430, 0x3e5ceaa72a9c5154, 0x3e53967fdba86f24,
    0x3e682468446b6824, 0x3e3f72e29f84325b, 0x3e18624b40c4dbd0, 0x3e5704f3404f068e,
    0x3e54d8a89c750e5e, 0x3e5a74b29ab4cf62, 0x3e5a753e077c2a0f, 0x3e5ad49f699bb2c0,
    0x3e6a90a852b19260, 0x3e56b48521ba6f93, 0x3e0d2ac258f87d03, 0x3e42a91124893ecf,
    0x3e59fcef32422cbe, 0x3e68ca345de441c5, 0x3e61d8bee7ba46e1, 0x3e59099f22fdba6a,
    0x3e4f580c36bea881, 0x3e5b3d398841740a, 0x3e62999c25159f11, 0x3e668925d901c83b,
    0x3e415506dadd3e2a, 0x3e622aee6c57304e, 0x3e29b8bc9e8a0387, 0x3e6fbc9c9f173d24,
    0x3e451f8480e3e235, 0x3e66bbcac96535b5, 0x3e41f12ae45a1224, 0x3e55e7f6fd0fac90,
    0x3e62b5a75abd0e69, 0x3e609e2bf5ed7fa1, 0x3e47daf237553d84, 0x3e12f074891ee83d,
    0x3e6b0aa538444196, 0x3e6cafa29694426f, 0x3e69df20d22a0797, 0x3e640f12f71a1e45,
    0x3e69f7490e4bb40b, 0x3e4ed9942b84600d, 0x3e4bdcdaf5cb4656, 0x3e5e2cffd89cf44c,
    0x3e452486cc2c7b9d, 0x3e6cc2b44eee3fa4, 0x3e66dc8a80ce9f09, 0x3e39e90d82e90a7e,
];

#[rustfmt::skip]
const SINH_LEAD: [u64; 37] = [
    0x0000000000000000, 0x3ff2cd9fc0000000, 0x400d03cf60000000, 0x40240926e0000000,
    0x403b4a3800000000, 0x40528d0160000000, 0x406936d228000000, 0x4081228768000000,
    0x409749ea50000000, 0x40afa71570000000, 0x40c5829dc8000000, 0x40dd3c4488000000,
    0x40f3de1650000000, 0x410b00b590000000, 0x412259ac48000000, 0x4138f0cca8000000,
    0x4150f2ebd0000000, 0x4167093488000000, 0x417f4f2208000000, 0x419546d8f8000000,
    0x41aceb0888000000, 0x41c3a6e1f8000000, 0x41dab5adb8000000, 0x41f226af30000000,
    0x4208ab7fb0000000, 0x4220c3d390000000, 0x4236c93268000000, 0x424ef822f0000000,
    0x42650bba30000000, 0x427c9aae40000000, 0x4293704708000000, 0x42aa6b7658000000,
    0x42c1f43fc8000000, 0x42d866f348000000, 0x42f0953e28000000, 0x430689e220000000,
    0x431ea215a0000000,
];
#[rustfmt::skip]
const SINH_TAIL: [u64; 37] = [
    0x0000000000000000, 0x3e513ae6096a0092, 0x3e5db70cfb79a640, 0x3e8c2526b66dc067,
    0x3e8b81b18647f380, 0x3ebbc1cdd1e1eb08, 0x3ecd9f201534fb09, 0x3edd1c064a4e9954,
    0x3ed4eca65d06ea74, 0x3f00c259bcc0ecc5, 0x3f2b5a6647cf9016, 0x3f09691adefb0870,
    0x3f53410fc29cde38, 0x3f46a31a50b6fb3c, 0x3f57defc71805c40, 0x3f9eb49fd80e0bab,
    0x3f84fffc7bcd5920, 0x3fc03a93b6c63435, 0x3fb1940bb255fd1c, 0x3fded26e14260b50,
    0x3ffb47401fc9f2a2, 0x40267bb3f55634f1, 0x401c435ff8194ddc, 0x404d8fee052ba63a,
    0x40651d7edccde3f6, 0x40704b1644557d1a, 0x4076a6b5ca0a9dc4, 0x40afd9cc72249aba,
    0x40ce58de693edab5, 0x40d8c70158ac6363, 0x40e7614764f43e20, 0x4106337db36fc718,
    0x41212d98b1f611e2, 0x412392bc108b37cc, 0x415ce87bdc3473dc, 0x414bc8d5ae99ad14,
    0x415d20d76744835c,
];
#[rustfmt::skip]
const COSH_LEAD: [u64; 37] = [
    0x3ff0000000000000, 0x3ff8b07550000000, 0x400e18fa08000000, 0x402422a490000000,
    0x403b4ee858000000, 0x40528d6fc8000000, 0x406936e678000000, 0x4081228948000000,
    0x409749eaa8000000, 0x40afa71580000000, 0x40c5829dd0000000, 0x40dd3c4488000000,
    0x40f3de1650000000, 0x410b00b590000000, 0x412259ac48000000, 0x4138f0cca8000000,
    0x4150f2ebd0000000, 0x4167093488000000, 0x417f4f2208000000, 0x419546d8f8000000,
    0x41aceb0888000000, 0x41c3a6e1f8000000, 0x41dab5adb8000000, 0x41f226af30000000,
    0x4208ab7fb0000000, 0x4220c3d390000000, 0x4236c93268000000, 0x424ef822f0000000,
    0x42650bba30000000, 0x427c9aae40000000, 0x4293704708000000, 0x42aa6b7658000000,
    0x42c1f43fc8000000, 0x42d866f348000000, 0x42f0953e28000000, 0x430689e220000000,
    0x431ea215a0000000,
];
#[rustfmt::skip]
const COSH_TAIL: [u64; 37] = [
    0x0000000000000000, 0x3e3d9f5504c2bd28, 0x3e67cb66f0a4c9fd, 0x3e8f58617928e588,
    0x3e6bc7d000c38d48, 0x3eaf7f9d4e329998, 0x3ec6e6e464885269, 0x3ecba3a8b946c154,
    0x3ed3f4e76110d5a4, 0x3f017622515a3e2b, 0x3ee4dc4b528af3d0, 0x3f11156278615e10,
    0x3f535ad50ed821f5, 0x3f46b61055f2935c, 0x3f57e2794a601240, 0x3f9eb4b45f6aadd3,
    0x3f85000b967b3698, 0x3fc03a940fadc092, 0x3fb1940bf3bf874c, 0x3fded26e1a2a2110,
    0x3ffb4740205796d6, 0x40267bb3f55cb85d, 0x401c435ff81e18ac, 0x404d8fee052bdea4,
    0x40651d7edccde926, 0x40704b1644557e0e, 0x4076a6b5ca0a9e1c, 0x40afd9cc72249abe,
    0x40ce58de693edab5, 0x40d8c70158ac6364, 0x40e7614764f43e20, 0x4106337db36fc718,
    0x41212d98b1f611e2, 0x412392bc108b37cc, 0x415ce87bdc3473dc, 0x414bc8d5ae99ad14,
    0x415d20d76744835c,
];
#[rustfmt::skip]
const EXP2_JBY32_LEAD: [u64; 32] = [
    0x3ff0000000000000, 0x3ff059b0d0000000, 0x3ff0b55860000000, 0x3ff11301d0000000,
    0x3ff172b830000000, 0x3ff1d48730000000, 0x3ff2387a60000000, 0x3ff29e9df0000000,
    0x3ff306fe00000000, 0x3ff371a730000000, 0x3ff3dea640000000, 0x3ff44e0860000000,
    0x3ff4bfdad0000000, 0x3ff5342b50000000, 0x3ff5ab07d0000000, 0x3ff6247eb0000000,
    0x3ff6a09e60000000, 0x3ff71f75e0000000, 0x3ff7a11470000000, 0x3ff8258990000000,
    0x3ff8ace540000000, 0x3ff93737b0000000, 0x3ff9c49180000000, 0x3ffa5503b0000000,
    0x3ffae89f90000000, 0x3ffb7f76f0000000, 0x3ffc199bd0000000, 0x3ffcb720d0000000,
    0x3ffd5818d0000000, 0x3ffdfc9730000000, 0x3ffea4afa0000000, 0x3fff507650000000,
];
#[rustfmt::skip]
const EXP2_JBY32_TRAIL: [u64; 32] = [
    0x0000000000000000, 0x3e48ac2ba1d73e2a, 0x3e69f3121ec53172, 0x3df25b50a4ebbf1b,
    0x3e68faa2f5b9bef9, 0x3e368b9aa7805b80, 0x3e6ceac470cd83f6, 0x3e547f7b84b09745,
    0x3e64636e2a5bd1ab, 0x3e5ceaa72a9c5154, 0x3e682468446b6824, 0x3e18624b40c4dbd0,
    0x3e54d8a89c750e5e, 0x3e5a753e077c2a0f, 0x3e6a90a852b19260, 0x3e0d2ac258f87d03,
    0x3e59fcef32422cbf, 0x3e61d8bee7ba46e2, 0x3e4f580c36bea881, 0x3e62999c25159f11,
    0x3e415506dadd3e2a, 0x3e29b8bc9e8a0388, 0x3e451f8480e3e236, 0x3e41f12ae45a1224,
    0x3e62b5a75abd0e6a, 0x3e47daf237553d84, 0x3e6b0aa538444196, 0x3e69df20d22a0798,
    0x3e69f7490e4bb40b, 0x3e4bdcdaf5cb4656, 0x3e452486cc2c7b9d, 0x3e66dc8a80ce9f09,
];

#[rustfmt::skip]
const ATAN_JBY256_LEAD: [u64; 241] = [
    0x3faff55b00000000, 0x3fb0f99e00000000, 0x3fb1f86d00000000, 0x3fb2f71900000000,
    0x3fb3f59f00000000, 0x3fb4f3fd00000000, 0x3fb5f23200000000, 0x3fb6f03b00000000,
    0x3fb7ee1800000000, 0x3fb8ebc500000000, 0x3fb9e94100000000, 0x3fbae68a00000000,
    0x3fbbe39e00000000, 0x3fbce07c00000000, 0x3fbddd2100000000, 0x3fbed98c00000000,
    0x3fbfd5ba00000000, 0x3fc068d500000000, 0x3fc0e6ad00000000, 0x3fc1646500000000,
    0x3fc1e1fa00000000, 0x3fc25f6e00000000, 0x3fc2dcbd00000000, 0x3fc359e800000000,
    0x3fc3d6ee00000000, 0x3fc453ce00000000, 0x3fc4d08700000000, 0x3fc54d1800000000,
    0x3fc5c98100000000, 0x3fc645bf00000000, 0x3fc6c1d400000000, 0x3fc73dbd00000000,
    0x3fc7b97b00000000, 0x3fc8350b00000000, 0x3fc8b06e00000000, 0x3fc92ba300000000,
    0x3fc9a6a800000000, 0x3fca217e00000000, 0x3fca9c2300000000, 0x3fcb169600000000,
    0x3fcb90d700000000, 0x3fcc0ae500000000, 0x3fcc84bf00000000, 0x3fccfe6500000000,
    0x3fcd77d500000000, 0x3fcdf11000000000, 0x3fce6a1400000000, 0x3fcee2e100000000,
    0x3fcf5b7500000000, 0x3fcfd3d100000000, 0x3fd025fa00000000, 0x3fd061ee00000000,
    0x3fd09dc500000000, 0x3fd0d97e00000000, 0x3fd1151a00000000, 0x3fd1509700000000,
    0x3fd18bf500000000, 0x3fd1c73500000000, 0x3fd2025500000000, 0x3fd23d5600000000,
    0x3fd2783700000000, 0x3fd2b2f700000000, 0x3fd2ed9800000000, 0x3fd3281800000000,
    0x3fd3627700000000, 0x3fd39cb400000000, 0x3fd3d6d100000000, 0x3fd410cb00000000,
    0x3fd44aa400000000, 0x3fd4845a00000000, 0x3fd4bdee00000000, 0x3fd4f75f00000000,
    0x3fd530ad00000000, 0x3fd569d800000000, 0x3fd5a2e000000000, 0x3fd5dbc300000000,
    0x3fd6148400000000, 0x3fd64d1f00000000, 0x3fd6859700000000, 0x3fd6bdea00000000,
    0x3fd6f61900000000, 0x3fd72e2200000000, 0x3fd7660700000000, 0x3fd79dc600000000,
    0x3fd7d56000000000, 0x3fd80cd400000000, 0x3fd8442200000000, 0x3fd87b4b00000000,
    0x3fd8b24d00000000, 0x3fd8e92900000000, 0x3fd91fde00000000, 0x3fd9566d00000000,
    0x3fd98cd500000000, 0x3fd9c31600000000, 0x3fd9f93000000000, 0x3fda2f2300000000,
    0x3fda64ee00000000, 0x3fda9a9200000000, 0x3fdad00f00000000, 0x3fdb056400000000,
    0x3fdb3a9100000000, 0x3fdb6f9600000000, 0x3fdba47300000000, 0x3fdbd92800000000,
    0x3fdc0db400000000, 0x3fdc421900000000, 0x3fdc765500000000, 0x3fdcaa6800000000,
    0x3fdcde5300000000, 0x3fdd121500000000, 0x3fdd45ae00000000, 0x3fdd791f00000000,
    0x3fddac6700000000, 0x3fdddf8500000000, 0x3fde127b00000000, 0x3fde454800000000,
    0x3fde77eb00000000, 0x3fdeaa6500000000, 0x3fdedcb600000000, 0x3fdf0ede00000000,
    0x3fdf40dd00000000, 0x3fdf72b200000000, 0x3fdfa45d00000000, 0x3fdfd5e000000000,
    0x3fe0039c00000000, 0x3fe01c3400000000, 0x3fe034b700000000, 0x3fe04d2500000000,
    0x3fe0657e00000000, 0x3fe07dc300000000, 0x3fe095f300000000, 0x3fe0ae0e00000000,
    0x3fe0c61400000000, 0x3fe0de0500000000, 0x3fe0f5e200000000, 0x3fe10daa00000000,
    0x3fe1255d00000000, 0x3fe13cfb00000000, 0x3fe1548500000000, 0x3fe16bfa00000000,
    0x3fe1835a00000000, 0x3fe19aa500000000, 0x3fe1b1dc00000000, 0x3fe1c8fe00000000,
    0x3fe1e00b00000000, 0x3fe1f70400000000, 0x3fe20de800000000, 0x3fe224b700000000,
    0x3fe23b7100000000, 0x3fe2521700000000, 0x3fe268a900000000, 0x3fe27f2600000000,
    0x3fe2958e00000000, 0x3fe2abe200000000, 0x3fe2c22100000000, 0x3fe2d84c00000000,
    0x3fe2ee6200000000, 0x3fe3046400000000, 0x3fe31a5200000000, 0x3fe3302b00000000,
    0x3fe345f000000000, 0x3fe35ba000000000, 0x3fe3713d00000000, 0x3fe386c500000000,
    0x3fe39c3900000000, 0x3fe3b19800000000, 0x3fe3c6e400000000, 0x3fe3dc1c00000000,
    0x3fe3f13f00000000, 0x3fe4064f00000000, 0x3fe41b4a00000000, 0x3fe4303200000000,
    0x3fe4450600000000, 0x3fe459c600000000, 0x3fe46e7200000000, 0x3fe4830a00000000,
    0x3fe4978f00000000, 0x3fe4ac0000000000, 0x3fe4c05e00000000, 0x3fe4d4a800000000,
    0x3fe4e8de00000000, 0x3fe4fd0100000000, 0x3fe5111000000000, 0x3fe5250c00000000,
    0x3fe538f500000000, 0x3fe54cca00000000, 0x3fe5608d00000000, 0x3fe5743c00000000,
    0x3fe587d800000000, 0x3fe59b6000000000, 0x3fe5aed600000000, 0x3fe5c23900000000,
    0x3fe5d58900000000, 0x3fe5e8c600000000, 0x3fe5fbf000000000, 0x3fe60f0800000000,
    0x3fe6220d00000000, 0x3fe634ff00000000, 0x3fe647de00000000, 0x3fe65aab00000000,
    0x3fe66d6600000000, 0x3fe6800e00000000, 0x3fe692a400000000, 0x3fe6a52700000000,
    0x3fe6b79800000000, 0x3fe6c9f700000000, 0x3fe6dc4400000000, 0x3fe6ee7f00000000,
    0x3fe700a700000000, 0x3fe712be00000000, 0x3fe724c300000000, 0x3fe736b600000000,
    0x3fe7489700000000, 0x3fe75a6700000000, 0x3fe76c2400000000, 0x3fe77dd100000000,
    0x3fe78f6b00000000, 0x3fe7a0f400000000, 0x3fe7b26c00000000, 0x3fe7c3d300000000,
    0x3fe7d52800000000, 0x3fe7e66c00000000, 0x3fe7f79e00000000, 0x3fe808c000000000,
    0x3fe819d000000000, 0x3fe82ad000000000, 0x3fe83bbe00000000, 0x3fe84c9c00000000,
    0x3fe85d6900000000, 0x3fe86e2500000000, 0x3fe87ed000000000, 0x3fe88f6b00000000,
    0x3fe89ff500000000, 0x3fe8b06f00000000, 0x3fe8c0d900000000, 0x3fe8d13200000000,
    0x3fe8e17a00000000, 0x3fe8f1b300000000, 0x3fe901db00000000, 0x3fe911f300000000,
    0x3fe921fb00000000,
];
#[rustfmt::skip]
const ATAN_JBY256_TAIL: [u64; 241] = [
    0x3e56e59fbd38db2c, 0x3e64e3aa54dedf96, 0x3e67e105ab1bda88, 0x3e48c5254d013fd0,
    0x3e2cf8ab3ad62670, 0x3e59dca4bec80468, 0x3e53f4b5ec98a8da, 0x3e6b9d49619d81fe,
    0x3e43017887460934, 0x3e511e3eca0b9944, 0x3e54f3f73c5a332e, 0x3e5c71c8ae0e00a6,
    0x3e67cde0f86fbdc7, 0x3e570f328c889c72, 0x3e5c07ae9b994efe, 0x3e40c8021d7b1698,
    0x3e635585edb8cb22, 0x3e70842567b30e96, 0x3e799e811031472e, 0x3e6041821416bcee,
    0x3e7f6086e4dc96f4, 0x3e471a535c5f1b58, 0x3e765f743fe63ca1, 0x3e7dbd733472d014,
    0x3e7d18cc4d8b0d1d, 0x3e78c12553c8fb29, 0x3e753b49e2e8f991, 0x3e77422ae148c141,
    0x3e4e3ec269df56a8, 0x3e7ff6754e7e0ac9, 0x3e7131267b1b5aad, 0x3e7d14fa403a94bc,
    0x3e62f396c089a3d8, 0x3e7c731d78fa95bb, 0x3e7c50f385177399, 0x3e6f41409c6f2c20,
    0x3e7d2d90c4c39ec0, 0x3e680420696f2106, 0x3e4b40327943a2e8, 0x3e65d35e02f3d2a2,
    0x3e64a498288117b0, 0x3e635da119afb324, 0x3e714e85cdb9a908, 0x3e638754e5547b9a,
    0x3e7be40ae6ce3246, 0x3e70c993b3bea7e7, 0x3e71d2dd89ac3359, 0x3e61476603332c46,
    0x3e7f25901bac55b7, 0x3e7f881b7c826e28, 0x3e7441996d698d20, 0x3e8407ac521ea089,
    0x3e82fb0c6c4b1723, 0x3e8ca135966a3e18, 0x3e6b1218e4d646e4, 0x3e6d4e72a350d288,
    0x3e84617e2f04c329, 0x3e6096ec41e82650, 0x3e79f91f25773e6e, 0x3e659c0820f1d674,
    0x3e602bf7a2df1064, 0x3e8fb36bfc40508f, 0x3e7ea08f3f8dc892, 0x3e73ed6254656a0e,
    0x3e6b83f5e5e69c58, 0x3e8d6ec2af768592, 0x3e6493889a226f94, 0x3e85ad8fa65279ba,
    0x3e6b615784d45434, 0x3e809a184368f145, 0x3e761a2439b0d91c, 0x3e7ce1a65e39a978,
    0x3e832a39a93b6a66, 0x3e81c3699af804e7, 0x3e575e0f4e44ede8, 0x3e8f77ced1a7a83b,
    0x3e284e7f0cb1b500, 0x3e8ec6b838b02dfe, 0x3e83ebf4dfbeda87, 0x3e89397aed9cb475,
    0x3e707937bc239c54, 0x3e8aa754553131b6, 0x3e74a05d407c45dc, 0x3e8132231a206dd0,
    0x3e72d8ecfdd69c88, 0x3e7a852c74218606, 0x3e871bf2baeebb50, 0x3e483d7db7491820,
    0x3e6ca50d92b6da14, 0x3e56f5cde8530298, 0x3e7f343198910740, 0x3e70e8d241ccd80a,
    0x3e71535ac619e6c8, 0x3e77316041c36cd2, 0x3e7985a000637d8e, 0x3e6f2f29858c0a68,
    0x3e8879847f96d909, 0x3e8ab3d319e12e42, 0x3e75088162dfc4c2, 0x3e605749a1cd9d8c,
    0x3e5da65c6c6b8618, 0x3e6739bf7df1ad64, 0x3e6bc31252aa3340, 0x3e5e528191ad3aa8,
    0x3e8929d93df19f18, 0x3e5ff11eb693a080, 0x3e455ae3f145a3a0, 0x3e7cbcd8c6c0ca82,
    0x3e70cb04d425d304, 0x3e79adfcab5be678, 0x3e893d90c5662508, 0x3e768489bd35ff40,
    0x3e3586ed3da2b7e0, 0x3e87604d2e850eee, 0x3e7ac1d12bfb53d8, 0x3e39b3d468274740,
    0x3e7fc5d68d10e53c, 0x3e88f9e51884becb, 0x3e8a87f0869c06d1, 0x3e831e7279f685fa,
    0x3e46a8282f9719b0, 0x3e60d2724a8a44e0, 0x3e8a60524b11ad4e, 0x3e575fdf832750f0,
    0x3e8cf06902e4cd36, 0x3e6e82422d4f6d10, 0x3e524a091063e6c0, 0x3e78a1a172dc6f38,
    0x3e929b6619f8a92d, 0x3e79274d9c1b70c8, 0x3e50c34b1fbb7930, 0x3e6639866c20eb50,
    0x3e86d6d0f6832e9e, 0x3e9af54def99f25e, 0x3e916cfc52a00262, 0x3e8dcc1e83569c32,
    0x3e937f7a551ed425, 0x3e9f6360adc98887, 0x3e92c6ec8d35a2c1, 0x3e8bd44df84cb036,
    0x3e9117cf826e310e, 0x3e9ca533f332cfc9, 0x3e90f208509dbc2e, 0x3e8cd07d93c945de,
    0x3e957bdfd67e6d72, 0x3e7aab89c516c658, 0x3e63e823b1a1b8a0, 0x3e8307464a9d6d3c,
    0x3e9c5993cd438843, 0x3e9ba2fca02ab554, 0x3e801a5b6983a268, 0x3e6273d1b350efc8,
    0x3e864c238c37b0c6, 0x3e6aded07370a300, 0x3e878091197eb47e, 0x3e74b0f245e0dabc,
    0x3e9080d9794e2eaf, 0x3e8d4ec242b60c76, 0x3e4221d2f940caa0, 0x3e7cdbc42b2bba5c,
    0x3e6cce37bb440840, 0x3e96c1d999cf1dd0, 0x3e5bed8a07eb0870, 0x3e769ed88f490e3c,
    0x3e6cd41719b73ef0, 0x3e9cbc4ac95b41b7, 0x3e9238f1b890f5d7, 0x3e750c4282259cc4,
    0x3e9713d2de87b3e2, 0x3e81d5a7d2255276, 0x3e9c0dfd48227ac1, 0x3e91c964dab76753,
    0x3e86de56d5704496, 0x3e84aeb71fd19968, 0x3e8fbf91c57b1918, 0x3e9d6bef7fbe5d9a,
    0x3e9464d3dc249066, 0x3e9638e2ec4d9073, 0x3e716f4a7247ea7c, 0x3e31a0a740f1d440,
    0x3e86edbb0114a33c, 0x3e7dbee8bf1d513c, 0x3e95b8bdb0248f73, 0x3e97de3d3f5eac64,
    0x3e8ee24187ae448a, 0x3e9e06c591ec5192, 0x3e74e3861a332738, 0x3e7a9599dcc2bfe4,
    0x3e6f732fbad43468, 0x3e9eb9f573b727d9, 0x3e98b212a2eb9897, 0x3e9384884c167215,
    0x3e90e2d363020051, 0x3e92820879fbd022, 0x3e9a1ab9893e4b30, 0x3e82d1b817a24478,
    0x3e615d7b8ded4878, 0x3e78968f9db3a5e4, 0x3e971c4171fe135f, 0x3e96d80f605d0d8c,
    0x3e7c91f043691590, 0x3e839f8a15fce2b2, 0x3e455beda9d94b80, 0x3e8b12c15d60949a,
    0x3e924167b312bfe3, 0x3e90ab8633070277, 0x3e854554ebbc80ee, 0x3e60204aef5a4bb8,
    0x3e98af08c679cf2c, 0x3e90852a330ae6c8, 0x3e86d3eb9ec32916, 0x3e8685cb7fcbbafe,
    0x3e91f751c1e0bd95, 0x3e5705b1b0f72560, 0x3e9b98d8d808ca92, 0x3e62ea22c75cc980,
    0x3e97aba62bca0350, 0x3e9d73833442278c, 0x3e95a5ca1fb18bf9, 0x3e61a6092b6ecf28,
    0x3e744fd049aac104, 0x3e2c114fd8df5180, 0x3e95972f130feae5, 0x3e7ca034a55fe198,
    0x3e96e2b149990227, 0x3e7b00000294592c, 0x3e98b9bdc442620e, 0x3e8d94fdfabf3e4e,
    0x3e85db30b145ad9a, 0x3e8e3e1eb95022b0, 0x3e9d5b8b45442bd6, 0x3e97a046231ecd2e,
    0x3e9feafe3ef55232, 0x3e9839e7bfd78267, 0x3e645cf49d6fa900, 0x3e4be3132b27f380,
    0x3e9533980bb84f9f, 0x3e5889e2ce3ba390, 0x3e7f7778c3ad0cc8, 0x3e846660cec4eba2,
    0x3e85110b4611a626,
];

#[cfg(test)]
mod tests {
    use super::*;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        /// A positive binary64 with a random significand and a leading exponent in `lo .. lo + span`.
        fn positive(&mut self, lo: i64, span: u64) -> f64 {
            let e = (1023 + lo + (self.next() % span) as i64) as u64;
            d((self.next() & MANT) | (e << 52))
        }
        fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * ((self.next() >> 11) as f64 / (1u64 << 53) as f64)
        }
    }

    /// Counts the cases of `f` where the port and the runtime differ, region by region.
    #[cfg(target_env = "msvc")]
    fn against_the_runtime(
        name: &str,
        port: fn(f64) -> f64,
        runtime: fn(f64) -> f64,
        regions: &[(&str, &dyn Fn(&mut Rng) -> f64)],
    ) -> usize {
        let n: usize = std::env::var("SOFT87_N")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1_000_000);
        let mut total = 0;
        for (seed, (region, draw)) in regions.iter().enumerate() {
            let mut rng = Rng(0x51ed_270b_27a1_ea13 ^ (seed as u64 + 1) * 0x9e37_79b9);
            let mut differ = 0;
            for _ in 0..n {
                let x = std::hint::black_box(draw(&mut rng));
                let (a, b) = (port(x), runtime(x));
                if a.to_bits() != b.to_bits() && !(a.is_nan() && b.is_nan()) {
                    if differ < 3 {
                        eprintln!("{name} {region}: x={x:e} port {:016x} runtime {:016x}", a.to_bits(), b.to_bits());
                    }
                    differ += 1;
                }
            }
            eprintln!("{name} {region}: {differ} of {n} differ");
            total += differ;
        }
        total
    }

    /// dc-twc: the ports against the Windows runtime's own `log`, `log10` and `exp` (what
    /// `f64::ln`, `f64::log10` and `f64::exp` call under MSVC), 1,000,000 random arguments per
    /// region (`SOFT87_N` to change it), every region of each algorithm.
    #[cfg(target_env = "msvc")]
    /// Native-only: the reference is the MSVC runtime's libm, which only Windows has.
    #[test]
    fn log_log10_and_exp_equal_the_windows_runtime() {
        let log_regions: [(&str, &dyn Fn(&mut Rng) -> f64); 5] = [
            ("whole range", &|r| r.positive(-1022, 2046)),
            ("near one", &|r| r.uniform(0.9375, 1.0625)),
            ("table edges", &|r| r.uniform(0.5, 2.0)),
            ("subnormal", &|r| d(r.next() & MANT)),
            ("specials", &|r| [0.0, -0.0, -1.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 1.0][(r.next() % 7) as usize]),
        ];
        let mut total = against_the_runtime("log", crt_log, f64::ln, &log_regions);
        total += against_the_runtime("log10", crt_log10, f64::log10, &log_regions);
        let exp_regions: [(&str, &dyn Fn(&mut Rng) -> f64); 6] = [
            ("[-745, 710]", &|r| r.uniform(-745.2, 709.8)),
            ("[-2, 2]", &|r| r.uniform(-2.0, 2.0)),
            ("small", &|r| r.positive(-60, 40) * if r.next() & 1 == 0 { 1.0 } else { -1.0 }),
            ("subnormal results", &|r| r.uniform(-745.2, -707.0)),
            ("near overflow", &|r| r.uniform(700.0, 710.0)),
            ("specials", &|r| [f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 0.0, -0.0, 1000.0, -1000.0][(r.next() % 7) as usize]),
        ];
        total += against_the_runtime("exp", crt_exp, f64::exp, &exp_regions);
        assert_eq!(total, 0);
    }

    /// dc-twc: the ports against the runtime's own `sin` and `cos`, 1,000,000 random arguments
    /// per region: the polynomial range, the Cody-Waite range, the Payne-Hanek range up to the
    /// largest binary64, the small-argument shortcuts and the special values.
    #[cfg(target_env = "msvc")]
    /// Native-only: the reference is the MSVC runtime's libm, which only Windows has.
    #[test]
    fn sin_and_cos_equal_the_windows_runtime() {
        let sign = |r: &mut Rng| if r.next() & 1 == 0 { 1.0 } else { -1.0 };
        let regions: [(&str, &dyn Fn(&mut Rng) -> f64); 7] = [
            ("[-pi/4, pi/4]", &|r| r.uniform(-0.7853981633974483, 0.7853981633974483)),
            ("[-100, 100]", &|r| r.uniform(-100.0, 100.0)),
            ("to 2e7", &|r| r.positive(-1, 26) * sign(r)),
            ("multiples of pi/2", &|r| ((r.next() % 2_000_000) as f64) * 1.5707963267948966 * sign(r)),
            ("2e7 and beyond", &|r| r.positive(24, 1000) * sign(r)),
            ("small", &|r| r.positive(-40, 30) * sign(r)),
            ("specials", &|r| [f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 0.0, -0.0, 0.7853981633974483, 2e7][(r.next() % 7) as usize]),
        ];
        let mut total = against_the_runtime("sin", crt_sin, f64::sin, &regions);
        total += against_the_runtime("cos", crt_cos, f64::cos, &regions);
        assert_eq!(total, 0);
    }

    /// dc-twc: the ports against the runtime's own `sinh` and `cosh`, 1,000,000 random arguments
    /// per region: the table range (each integer part), the `splitexp` range up to overflow, the
    /// small-argument shortcuts and the special values.
    #[cfg(target_env = "msvc")]
    /// Native-only: the reference is the MSVC runtime's libm, which only Windows has.
    #[test]
    fn sinh_and_cosh_equal_the_windows_runtime() {
        let sign = |r: &mut Rng| if r.next() & 1 == 0 { 1.0 } else { -1.0 };
        let regions: [(&str, &dyn Fn(&mut Rng) -> f64); 5] = [
            ("[-37, 37]", &|r| r.uniform(-37.0, 37.0)),
            ("[-1, 1]", &|r| r.uniform(-1.0, 1.0)),
            ("to overflow", &|r| r.uniform(19.0, 711.0) * sign(r)),
            ("small", &|r| r.positive(-40, 34) * sign(r)),
            ("specials", &|r| [f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 0.0, -0.0, 20.0, 710.4758600739439][(r.next() % 7) as usize]),
        ];
        let mut total = against_the_runtime("sinh", crt_sinh, f64::sinh, &regions);
        total += against_the_runtime("cosh", crt_cosh, f64::cosh, &regions);
        assert_eq!(total, 0);
    }

    /// dc-twc: the ports against the runtime's own `atan2` and `hypot`, 1,000,000 random pairs
    /// per region: every quadrant at mixed magnitudes, ratios around the table and series
    /// thresholds (1/16, 1e-8), exponent gaps past 28, 56 and 1022, subnormals, and the special
    /// values (zeros, infinities, NaNs).
    #[cfg(target_env = "msvc")]
    /// Native-only: the reference is the MSVC runtime's libm, which only Windows has.
    #[test]
    fn atan2_and_hypot_equal_the_windows_runtime() {
        let n: usize = std::env::var("SOFT87_N").ok().and_then(|v| v.parse().ok()).unwrap_or(1_000_000);
        let specials = [0.0, -0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 1.0, -1.0, d(1), d(MANT), f64::MAX];
        let regions: [(&str, &dyn Fn(&mut Rng) -> (f64, f64)); 6] = [
            ("mixed", &|r| {
                let (a, b) = (r.positive(-30, 60), r.positive(-30, 60));
                (if r.next() & 1 == 0 { a } else { -a }, if r.next() & 1 == 0 { b } else { -b })
            }),
            ("near thresholds", &|r| {
                let x = r.positive(-5, 10) * if r.next() & 1 == 0 { 1.0 } else { -1.0 };
                let k = [0.0625, 1e-8, 1.0, 0.5][(r.next() % 4) as usize] * r.uniform(0.9, 1.1);
                (x * k, x)
            }),
            ("exponent gaps", &|r| (r.positive(-1022, 2046) * if r.next() & 1 == 0 { 1.0 } else { -1.0 }, r.positive(-1022, 2046) * if r.next() & 1 == 0 { 1.0 } else { -1.0 })),
            ("subnormal", &|r| (d(r.next() & MANT) * if r.next() & 1 == 0 { 1.0 } else { -1.0 }, r.positive(-1022, 30) * if r.next() & 1 == 0 { 1.0 } else { -1.0 })),
            ("both small", &|r| (r.positive(-1074, 60), r.positive(-1074, 60) * if r.next() & 1 == 0 { 1.0 } else { -1.0 })),
            ("specials", &|r| (specials[(r.next() % 10) as usize], specials[(r.next() % 10) as usize])),
        ];
        let mut total = 0;
        for (name, port, runtime) in [
            ("atan2", crt_atan2 as fn(f64, f64) -> f64, f64::atan2 as fn(f64, f64) -> f64),
            ("hypot", crt_hypot, f64::hypot),
        ] {
            for (seed, (region, draw)) in regions.iter().enumerate() {
                let mut rng = Rng(0x7a7a_2b2b ^ (seed as u64 + 3) * 0x9e37_79b9);
                let mut differ = 0;
                for _ in 0..n {
                    let (a, b) = draw(&mut rng);
                    let (a, b) = (std::hint::black_box(a), std::hint::black_box(b));
                    let (p, q) = (port(a, b), runtime(a, b));
                    if p.to_bits() != q.to_bits() && !(p.is_nan() && q.is_nan()) {
                        if differ < 3 {
                            eprintln!("{name} {region}: ({a:e}, {b:e}) port {:016x} runtime {:016x}", p.to_bits(), q.to_bits());
                        }
                        differ += 1;
                    }
                }
                eprintln!("{name} {region}: {differ} of {n} differ");
                total += differ;
            }
        }
        assert_eq!(total, 0);
    }

    /// Values the three take on every target.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn spot_values() {
        assert_eq!(crt_log(1.0), 0.0);
        assert_eq!(crt_log(3.0).to_bits(), 0x3ff193ea7aad030b); // the runtime's ln(3)
        assert_eq!(crt_log(0.0), f64::NEG_INFINITY);
        assert!(crt_log(-1.0).is_nan());
        assert_eq!(crt_log10(1000.0), 3.0);
        assert_eq!(crt_exp(0.0), 1.0);
        assert_eq!(crt_exp(1.0), std::f64::consts::E);
        assert_eq!(crt_exp(-1000.0), 0.0);
        assert_eq!(crt_exp(1000.0), f64::INFINITY);
    }
}
