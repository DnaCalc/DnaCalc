use crate::value::WorksheetErrorCode;
use crate::ported::normal_dist_common::erf_approx;

const LANCZOS_G: f64 = 7.0;
const LANCZOS_COEFFS: [f64; 9] = [
    0.999_999_999_999_809_9,
    676.520_368_121_885_1,
    -1_259.139_216_722_402_8,
    771.323_428_777_653_1,
    -176.615_029_162_140_6,
    12.507_343_278_686_905,
    -0.138_571_095_265_720_12,
    9.984_369_578_019_572e-6,
    1.505_632_735_149_311_6e-7,
];

fn bool_flag_from_number(n: f64) -> bool {
    n != 0.0
}

fn is_integer_like(x: f64) -> bool {
    // BUG-FUNC-027 CLASS-A2: a fixed absolute tolerance falsely collapses a tiny
    // non-integer such as -1e-200 onto 0. Test integrality relative to magnitude
    // so only genuine integers (and exact 0) qualify.
    (x - x.round()).abs() <= 1.0e-12 * x.abs()
}

fn has_gamma_pole(x: f64) -> bool {
    x <= 0.0 && is_integer_like(x)
}

fn ln_gamma_positive(x: f64) -> Result<f64, WorksheetErrorCode> {
    if !x.is_finite() || x <= 0.0 {
        return Err(WorksheetErrorCode::Num);
    }

    // BUG-FUNC-027 CLASS-A1: for x < 1 the Lanczos term coeff/(z+1) = coeff/x
    // diverges, and for tiny x, z = x - 1.0 loses x entirely so z+1 == 0 yields
    // +Inf. Lift x into the stable region via lnGamma(x) = lnGamma(x+1) - ln(x).
    if x < 1.0 {
        return Ok(ln_gamma_positive(x + 1.0)? - x.ln());
    }

    let z = x - 1.0;
    let mut acc = LANCZOS_COEFFS[0];
    for (i, coeff) in LANCZOS_COEFFS.iter().enumerate().skip(1) {
        acc += coeff / (z + i as f64);
    }

    let t = z + LANCZOS_G + 0.5;
    Ok(0.5 * (2.0 * std::f64::consts::PI).ln() + (z + 0.5) * t.ln() - t + acc.ln())
}

fn erf_interval(lower: f64, upper: f64) -> f64 {
    erf_approx(upper) - erf_approx(lower)
}

pub fn erf_kernel(lower: f64, upper: Option<f64>) -> Result<f64, WorksheetErrorCode> {
    if !lower.is_finite() || upper.is_some_and(|v| !v.is_finite()) {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(match upper {
        Some(upper) => erf_interval(lower, upper),
        None => erf_approx(lower),
    })
}

pub fn erf_precise_kernel(x: f64) -> Result<f64, WorksheetErrorCode> {
    if !x.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(erf_approx(x))
}

/// Published-worksheet staging `SQRT(x/2)` used by Excel's df=1 chi-square
/// and `GAMMA.DIST(x, 0.5, 2, TRUE)` identities. Live Excel 16.0 build 20228
/// matched `ERF.PRECISE` / `ERFC.PRECISE` of this argument on 154/154
/// distinct nonnegative x (divide-by-two first; `SQRT(x)/SQRT(2)` is not
/// the same graph).
pub fn erf_of_sqrt_half_x(x: f64) -> Result<f64, WorksheetErrorCode> {
    erf_precise_kernel((x / 2.0).sqrt())
}

pub fn erfc_of_sqrt_half_x(x: f64) -> Result<f64, WorksheetErrorCode> {
    erfc_precise_kernel((x / 2.0).sqrt())
}

// Excel-emulation for the positive-tail ERFC regime.
//
// Policy: DnaCalc emulates Excel's observed output bits; mathematical
// correct-rounding is diagnostic only. See docs/function-lane/
// ERFC_EXCEL_EMULATION.md for the regime-map evidence and fit methodology.
//
// Approach: libm::erfc base multiplied by a small relative correction
// polynomial fitted to Excel's observed ratio (excel/libm - 1) at 45
// widened witness points. Piecewise in s = 1/x², split at the fdlibm
// subrange boundary x = 2.857. Fit weighted to force corr(s) = 0 (or the
// specific UCRT offset) at all already-green anchors, preserving the
// Matched islands.
//
// Coefficients below come from weighted least-squares fit captured in
// scratch probe `probe_rational_fit_attempt` (uncommitted). Evaluation is
// Horner on normalized u = 2*(s - s_lo)/(s_hi - s_lo) - 1.
//
// Evidence summary vs widened 48-point positive witness set:
// - libm-only baseline: 9 matches
// - prior Windows-MSVC UCRT branch at x>=3: 12 matches
// - this correction-fit kernel (cross-platform):  20 matches, 0 regressions
//   at any already-matched anchor, worst blocked |Δ| = 6 ULP
//
// x < 1.25 and all negatives stay on libm unchanged (already Excel-exact
// across every tested point down to -10).

const ERFC_B_S_MIN: f64 = 1.23114804555247758788e-1; // = 1/2.85^2, min s in Region B training
const ERFC_B_S_MAX: f64 = 6.40000000000000013323e-1; // = 1/1.25^2
const ERFC_B_COEFFS: [f64; 9] = [
    -4.68127849232051076334e-16,
    -7.62455092077679137822e-16,
    6.25451788846640322315e-15,
    1.29557375346293795597e-14,
    -5.44994791140840831850e-15,
    -1.24991935965186578433e-14,
    5.75379785143124545520e-15,
    3.13242732014867690079e-16,
    -6.09757155730509723491e-15,
];

const ERFC_A_S_MIN: f64 = 1.00000000000000002082e-2; // = 1/10^2
const ERFC_A_S_MAX: f64 = 1.18906064209274672794e-1; // = 1/2.9^2
const ERFC_A_COEFFS: [f64; 3] = [
    -2.31218392351115847457e-16,
    -1.54616394379972209297e-17,
    5.48978747768942628168e-16,
];

// fdlibm-inspired positive-tail region split.
const ERFC_BOUNDARY_X: f64 = 2.857;

fn erfc_horner(coeffs: &[f64], u: f64) -> f64 {
    let mut acc = 0.0_f64;
    let mut i = coeffs.len();
    while i > 0 {
        i -= 1;
        acc = acc * u + coeffs[i];
    }
    acc
}

fn excel_erfc(x: f64) -> f64 {
    // Live Excel 16.0 b20326 Value2: ERFC.PRECISE(z) == 1-ERF.PRECISE(z)
    // bit-exactly on 1033/1033 positive z in (0, 0.5] (1/2048 grid plus
    // the 0.5 ulp neighborhood). Negatives are not this identity (237/256
    // in [-0.5, 0)). Do not extend below 0 or above 0.5.
    if x > 0.0 && x <= 0.5 {
        return 1.0 - erf_approx(x);
    }
    let libm_v = libm::erfc(x);
    if !x.is_finite() || x < 1.25 {
        return libm_v;
    }
    let s = 1.0 / (x * x);
    let (s_lo, s_hi, coeffs): (f64, f64, &[f64]) = if x < ERFC_BOUNDARY_X {
        (ERFC_B_S_MIN, ERFC_B_S_MAX, &ERFC_B_COEFFS[..])
    } else {
        (ERFC_A_S_MIN, ERFC_A_S_MAX, &ERFC_A_COEFFS[..])
    };
    let u = 2.0 * (s - s_lo) / (s_hi - s_lo) - 1.0;
    let corr = erfc_horner(coeffs, u);
    libm_v * (1.0 + corr)
}

pub fn erfc_kernel(x: f64) -> Result<f64, WorksheetErrorCode> {
    if !x.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(excel_erfc(x))
}

pub fn erfc_precise_kernel(x: f64) -> Result<f64, WorksheetErrorCode> {
    erfc_kernel(x)
}

pub fn gamma_kernel(x: f64) -> Result<f64, WorksheetErrorCode> {
    if !x.is_finite() || has_gamma_pole(x) {
        return Err(WorksheetErrorCode::Num);
    }

    // Live Excel 16.0 b20326: GAMMA(-0.5) is not -2*GAMMA(0.5) (1 ULP).
    // Seed plus one peel step: GAMMA(-1.5)=GAMMA(-0.5)/(-1.5) is exact.
    // Further negative halves are not a contiguous peel family.
    if x == -0.5 {
        return Ok(f64::from_bits(0xc00c5bf891b4ef6a));
    }
    if x == -1.5 {
        return Ok(f64::from_bits(0xc00c5bf891b4ef6a) / -1.5);
    }

    // Live Excel 16.0 b20326: positive subnormals are #NUM! (same admission
    // as published GAMMALN). Min-normal is admitted.
    if x.is_subnormal() {
        return Err(WorksheetErrorCode::Num);
    }

    // Live Excel 16.0 b20326 Value2: on (0, 1e-16] normals, GAMMA(x) is 1/x
    // bit-exactly (decade grid 1e-307..=1e-16 plus a 27-point neighborhood of
    // 1e-16). First probed miss is 2e-16 (1 ULP). G3-02 remains open above
    // that cutoff.
    if x > 0.0 && x <= 1e-16 {
        return Ok(1.0 / x);
    }

    // Live Excel 16.0 build 20326/CV2 Value2: for positive integers n=1..=88,
    // GAMMA(n) is the reverse native product (n-1)*(n-2)*...*2 (empty
    // product 1 for n=1,2). Forward product diverges at n=26. n=89 and
    // above are not this graph. G3-02 remains open for non-integers and
    // n>=89.
    if (1.0..=88.0).contains(&x) && x.fract() == 0.0 {
        let n = x as u32;
        let mut acc = 1.0;
        for k in (2..n).rev() {
            acc *= k as f64;
        }
        return Ok(acc);
    }

    // Live Excel 16.0 b20326: GAMMA(n+0.5) for n=0..=171 is the native
    // product-first fold (0.5*1.5*...*(n-0.5))*GAMMA(0.5) with seed
    // `0x3ffc5bf891b4ef6b`. Worksheet PRODUCT of those terms times
    // GAMMA(0.5) is 172/172 through 171.5. Stepwise recurrence from
    // 19.5 misses 20.5 (1 ULP); seed-first IEEE also misses 20.5.
    // GAMMA(172.5) is #NUM! (IEEE product overflows). Replaces the
    // former (2n-1)!!/2^n closed form through 15.5 plus recurrence
    // through 19.5.
    const GAMMA_HALF: f64 = f64::from_bits(0x3ffc5bf891b4ef6b);
    if x > 0.0 && x.fract() == 0.5 {
        let n = x as u32;
        let mut prod = 1.0;
        for k in 0..n {
            prod *= 0.5 + k as f64;
            if !prod.is_finite() {
                return Err(WorksheetErrorCode::Num);
            }
        }
        let out = prod * GAMMA_HALF;
        if out.is_finite() {
            return Ok(out);
        }
        return Err(WorksheetErrorCode::Num);
    }

    // Live Excel 16.0 b20326: GAMMA(n+1/4) product-first
    // (0.25*1.25*...*(n-0.75))*GAMMA(0.25) is contiguous exact for
    // n=0..=9 (through 9.25); first miss 10.25. Recurrence from the
    // 0.25 seed misses 6.25. GAMMA(n+3/4) stays seed-first recurrence
    // n=0..=5; product-first already misses 2.75.
    const GAMMA_QUARTER: f64 = f64::from_bits(0x400d013fc47eeeec);
    const GAMMA_THREE_QUARTER: f64 = f64::from_bits(0x3ff39b4e8b50f62d);
    let four = x * 4.0;
    if four.fract() == 0.0 {
        let k = four as u32;
        if k % 4 == 1 {
            let n = (k - 1) / 4;
            if n <= 9 {
                let mut prod = 1.0;
                for i in 0..n {
                    prod *= 0.25 + i as f64;
                }
                return Ok(prod * GAMMA_QUARTER);
            }
        } else if k % 4 == 3 {
            let n = (k - 3) / 4;
            if n <= 5 {
                let mut acc = GAMMA_THREE_QUARTER;
                for i in 1..=n {
                    acc *= i as f64 - 0.25;
                }
                return Ok(acc);
            }
        }
    }

    // Live Excel 16.0 b20326: odd-eighth product-first
    // (seed_frac * (1+seed_frac) * ... ) * GAMMA(seed) is contiguous
    // exact for n+1/8, n+3/8, n+5/8 with n=0..=9 (through 9.125 / 9.375 /
    // 9.625). First misses 10.125 / 10.375 / 10.625. n+7/8 product-first
    // already misses 1.875; keep the seed only.
    const GAMMA_1_8: f64 = f64::from_bits(0x401e22c196233d23);
    const GAMMA_3_8: f64 = f64::from_bits(0x4002f6a73f0a9838);
    const GAMMA_5_8: f64 = f64::from_bits(0x3ff6f3ca0920b668);
    const GAMMA_7_8: f64 = f64::from_bits(0x3ff16f374f724016);
    let eight = x * 8.0;
    if eight.fract() == 0.0 {
        let k = eight as u32;
        match k % 8 {
            1 => {
                let n = (k - 1) / 8;
                if n <= 9 {
                    let mut prod = 1.0;
                    for i in 0..n {
                        prod *= 0.125 + i as f64;
                    }
                    return Ok(prod * GAMMA_1_8);
                }
            }
            3 => {
                let n = (k - 3) / 8;
                if n <= 9 {
                    let mut prod = 1.0;
                    for i in 0..n {
                        prod *= 0.375 + i as f64;
                    }
                    return Ok(prod * GAMMA_3_8);
                }
            }
            5 => {
                let n = (k - 5) / 8;
                if n <= 9 {
                    let mut prod = 1.0;
                    for i in 0..n {
                        prod *= 0.625 + i as f64;
                    }
                    return Ok(prod * GAMMA_5_8);
                }
            }
            7 => {
                if k == 7 {
                    return Ok(GAMMA_7_8);
                }
            }
            _ => {}
        }
    }

    // Live Excel 16.0 b20326: fifths in (0,1) are private seeds (GAMMA(1/5)
    // is 2 ULP from the generic path). Product-first extends 1/5 through
    // n=1; 2/5, 3/5, 4/5 miss at n=1.
    const GAMMA_FIFTH: [(u32, u64, u32); 4] = [
        (1, 0x40125d0622505413, 1),
        (2, 0x4001beca6e4dff14, 0),
        (3, 0x3ff7d3bb4061b952, 0),
        (4, 0x3ff2a0af5617b4b9, 0),
    ];
    if x > 0.0 {
        for &(kk, bits, nmax) in &GAMMA_FIFTH {
            let frac = kk as f64 / 5.0;
            let seed = f64::from_bits(bits);
            for n in 0..=nmax {
                if x.to_bits() == (n as f64 + frac).to_bits() {
                    let mut prod = 1.0;
                    for i in 0..n {
                        prod *= frac + i as f64;
                    }
                    return Ok(prod * seed);
                }
            }
        }
    }

    // Live Excel 16.0 b20326: thirds in (0,1) are private seeds
    // (GAMMA(1/3) is 6 ULP from the generic path). Recurrence not claimed
    // (n+1/3 was 7/21).
    const GAMMA_THIRD: [(u32, u64); 2] = [(1, 0x40056e77539482f2), (2, 0x3ff5aa77928c3679)];
    let three = x * 3.0;
    if three.fract() == 0.0 && (1.0..=2.0).contains(&three) {
        let k = three as u32;
        for &(kk, bits) in &GAMMA_THIRD {
            if kk == k {
                return Ok(f64::from_bits(bits));
            }
        }
    }

    // Live Excel 16.0 b20326: ninths in (0,1) that are not thirds.
    // Product-first: 5/9 n<=3, 7/9 n<=1; 1/9,2/9,4/9,8/9 miss at n=1.
    const GAMMA_NINTH: [(u32, u64, u32); 6] = [
        (1, 0x40210b9dc79fe8d4, 0),
        (2, 0x40106d2331a5de8d, 0),
        (4, 0x3fffe2e4518a6b60, 0),
        (5, 0x3ff99c88812c4a39, 3),
        (7, 0x3ff30adbf89161c8, 1),
        (8, 0x3ff13e800bd48928, 0),
    ];
    if x > 0.0 {
        for &(kk, bits, nmax) in &GAMMA_NINTH {
            let frac = kk as f64 / 9.0;
            let seed = f64::from_bits(bits);
            for n in 0..=nmax {
                if x.to_bits() == (n as f64 + frac).to_bits() {
                    let mut prod = 1.0;
                    for i in 0..n {
                        prod *= frac + i as f64;
                    }
                    return Ok(prod * seed);
                }
            }
        }
    }

    // Live Excel 16.0 b20326: seventeenths in (0,1) are private seeds
    // (GAMMA(1/17) is 4 ULP from the generic path). Product-first nmax:
    // 3/17,4/17,5/17,16/17 n<=1; 7/17,8/17,10/17,11/17,15/17 n<=3;
    // 9/17 and 12/17 n<=4.
    // 1/17,2/17,6/17,13/17,14/17 miss at n=1.
    const GAMMA_SEVENTEENTH: [(u32, u64, u32); 16] = [
        (1, 0x40307a5f0b4f6098, 0),
        (2, 0x40200e57dc97df34, 0),
        (3, 0x4014f615a699dd88, 1),
        (4, 0x400eecca79b8a44f, 1),
        (5, 0x40086f9d13f4bf73, 1),
        (6, 0x40043158acf8e488, 0),
        (7, 0x40013a1f684b241d, 3),
        (8, 0x3ffe1c96ab222e75, 3),
        (9, 0x3ffad2c1068d2f34, 4),
        (10, 0x3ff844e07dd0f45e, 3),
        (11, 0x3ff63f1b584f21ea, 3),
        (12, 0x3ff49f1273ec7a64, 4),
        (13, 0x3ff34d22f6c941f1, 0),
        (14, 0x3ff2388d10a04a21, 0),
        (15, 0x3ff15525bd8d39ed, 3),
        (16, 0x3ff099e6910e9682, 1),
    ];
    if x > 0.0 {
        for &(kk, bits, nmax) in &GAMMA_SEVENTEENTH {
            let frac = kk as f64 / 17.0;
            let seed = f64::from_bits(bits);
            for n in 0..=nmax {
                if x.to_bits() == (n as f64 + frac).to_bits() {
                    let mut prod = 1.0;
                    for i in 0..n {
                        prod *= frac + i as f64;
                    }
                    return Ok(prod * seed);
                }
            }
        }
    }

    // Live Excel 16.0 b20326: thirteenths in (0,1) are private seeds
    // (GAMMA(1/13) is 4 ULP from the generic path). Product-first nmax:
    // 1/13 n<=2, 2/13 n<=3, 3/13 n<=2, 4/13 n<=1, 6/13 n<=3, 7/13 n<=6,
    // 9/13 n<=4, 11/13 n<=1. 5/13,8/13,10/13,12/13 miss at n=1.
    const GAMMA_THIRTEENTH: [(u32, u64, u32); 12] = [
        (1, 0x4028fce1e0ed23fb, 2),
        (2, 0x401839eca7c726a2, 3),
        (3, 0x400f91158829b3d6, 2),
        (4, 0x40074dfec9db3ae6, 1),
        (5, 0x4002798afcfe30c7, 0),
        (6, 0x3ffeb36ee50fd917, 3),
        (7, 0x3ffa637c934edca9, 6),
        (8, 0x3ff7476291fcce66, 0),
        (9, 0x3ff4f76b6a7bb3cb, 4),
        (10, 0x3ff335dc7fec23ed, 0),
        (11, 0x3ff1dbd18812ee11, 1),
        (12, 0x3ff0cfaee504346b, 0),
    ];
    if x > 0.0 {
        for &(kk, bits, nmax) in &GAMMA_THIRTEENTH {
            let frac = kk as f64 / 13.0;
            let seed = f64::from_bits(bits);
            for n in 0..=nmax {
                if x.to_bits() == (n as f64 + frac).to_bits() {
                    let mut prod = 1.0;
                    for i in 0..n {
                        prod *= frac + i as f64;
                    }
                    return Ok(prod * seed);
                }
            }
        }
    }

    // Live Excel 16.0 b20326: elevenths in (0,1) are private seeds
    // (GAMMA(1/11) is 2 ULP from the generic path). Product-first nmax:
    // 2/11,3/11,6/11 n<=1; 4/11 n<=2; 7/11,9/11,10/11 n<=3.
    // 1/11,5/11,8/11 miss at n=1.
    const GAMMA_ELEVENTH: [(u32, u64, u32); 10] = [
        (1, 0x402503020775740e, 0),
        (2, 0x40144f786dca9448, 1),
        (3, 0x400a7575e6fa4759, 1),
        (4, 0x400393ac5e30a513, 2),
        (5, 0x3fff2c8ab61daf0f, 0),
        (6, 0x3ffa1060566708c3, 1),
        (7, 0x3ff694d834d0af13, 3),
        (8, 0x3ff41c2697944352, 0),
        (9, 0x3ff24f81874fd279, 3),
        (10, 0x3ff0fb827c62f539, 3),
    ];
    if x > 0.0 {
        for &(kk, bits, nmax) in &GAMMA_ELEVENTH {
            let frac = kk as f64 / 11.0;
            let seed = f64::from_bits(bits);
            for n in 0..=nmax {
                if x.to_bits() == (n as f64 + frac).to_bits() {
                    let mut prod = 1.0;
                    for i in 0..n {
                        prod *= frac + i as f64;
                    }
                    return Ok(prod * seed);
                }
            }
        }
    }

    // Live Excel 16.0 b20326: sevenths in (0,1) are private seeds
    // (GAMMA(1/7) is 2 ULP from the generic path). Product-first extends
    // 1/7 n<=2; 2/7,3/7,4/7,5/7,6/7 miss at n=1.
    const GAMMA_SEVENTH: [(u32, u64, u32); 6] = [
        (1, 0x401a313769520e5a, 2),
        (2, 0x400931634450f1e8, 0),
        (3, 0x40008a43968d61a6, 0),
        (4, 0x3ff8eff2aa47b664, 0),
        (5, 0x3ff46a774bb2e0cd, 0),
        (6, 0x3ff1b138d04a62f3, 0),
    ];
    if x > 0.0 {
        for &(kk, bits, nmax) in &GAMMA_SEVENTH {
            let frac = kk as f64 / 7.0;
            let seed = f64::from_bits(bits);
            for n in 0..=nmax {
                if x.to_bits() == (n as f64 + frac).to_bits() {
                    let mut prod = 1.0;
                    for i in 0..n {
                        prod *= frac + i as f64;
                    }
                    return Ok(prod * seed);
                }
            }
        }
    }

    // Live Excel 16.0 b20326: 1/12, 5/12, 7/12, 11/12 in (0,1) are private
    // seeds (other twelfths reduce to halves/thirds/quarters/sixths).
    // Product-first extends 1/12 and 5/12 through n=1, 7/12 through n=3.
    const GAMMA_TWELFTH: [(u32, u64, u32); 4] = [
        (1, 0x4026ffb50d1bc6dc, 1),
        (5, 0x4001053ca2989062, 1),
        (7, 0x3ff87597c6695642, 3),
        (11, 0x3ff0e384cb7476b6, 0),
    ];
    if x > 0.0 {
        for &(kk, bits, nmax) in &GAMMA_TWELFTH {
            let frac = kk as f64 / 12.0;
            let seed = f64::from_bits(bits);
            for n in 0..=nmax {
                if x.to_bits() == (n as f64 + frac).to_bits() {
                    let mut prod = 1.0;
                    for i in 0..n {
                        prod *= frac + i as f64;
                    }
                    return Ok(prod * seed);
                }
            }
        }
    }

    // Live Excel 16.0 b20326: odd tenths in (0,1) are private seeds
    // (GAMMA(1/10) is 3 ULP from the generic path). Product-first extends
    // 3/10 through n=1; other odd tenths miss at n=1.
    const GAMMA_ODD_TENTH: [(u32, u64, u32); 4] = [
        (1, 0x402306ea7b280d88, 0),
        (3, 0x4007eebbb8aec4ab, 1),
        (7, 0x3ff4c4d5ab21ea23, 0),
        (9, 0x3ff1191a68f2b5e1, 0),
    ];
    if x > 0.0 {
        for &(kk, bits, nmax) in &GAMMA_ODD_TENTH {
            let frac = kk as f64 / 10.0;
            let seed = f64::from_bits(bits);
            for n in 0..=nmax {
                if x.to_bits() == (n as f64 + frac).to_bits() {
                    let mut prod = 1.0;
                    for i in 0..n {
                        prod *= frac + i as f64;
                    }
                    return Ok(prod * seed);
                }
            }
        }
    }

    // Live Excel 16.0 b20326: odd-sixteenth product-first from the (0,1)
    // seeds. Contiguous exact nmax: 1/16 n<=9, 5/16 n<=4, 9/16 n<=10,
    // 13/16 n<=5. The other four families miss at n=1 (seed only).
    const GAMMA_ODD_SIXTEENTH: [(u32, u64, u32); 8] = [
        (1, 0x402ef66a79533ee8, 9),
        (3, 0x4013a91381a8a4ee, 0),
        (5, 0x4006edc1821c5c71, 4),
        (7, 0x400032cfe11b9bd7, 0),
        (9, 0x3ff94fa627d94f66, 10),
        (11, 0x3ff517bf09b399f5, 0),
        (13, 0x3ff26858f1d7c28d, 5),
        (15, 0x3ff0a490a6519230, 0),
    ];
    let sixteen = x * 16.0;
    if x > 0.0 && sixteen.fract() == 0.0 {
        let k = sixteen as u32;
        if k % 2 == 1 {
            let rem = k % 16;
            let n = (k - rem) / 16;
            for &(kk, bits, nmax) in &GAMMA_ODD_SIXTEENTH {
                if kk == rem && n <= nmax {
                    let seed = f64::from_bits(bits);
                    let frac = rem as f64 / 16.0;
                    let mut prod = 1.0;
                    for i in 0..n {
                        prod *= frac + i as f64;
                    }
                    return Ok(prod * seed);
                }
            }
        }
    }

    // Live Excel 16.0 b20326: selected x in (-1,0) as published-bit
    // seeds. Many of these are exact worksheet GAMMA(x)=GAMMA(x+1)/x
    // peels; others (3/7-1, 5/7-1, 5/16-1, 13/16-1, 5/9-1, 1/12-1,
    // 7/12-1) are 1–4 ULP from that peel and still use the Excel bits.
    // IEEE complement-seed/x is not the graph (1+(-1/3) is not 2/3).
    if x > -1.0 && x < 0.0 {
        const GAMMA_NEG_FRAC: [(u64, u64); 84] = [
            (0xbfd5555555555555, 0xc0103fd9ade928da), // -1/3
            (0xbfe5555555555555, 0xc01012d97eaf6234), // -2/3
            (0xbfd0000000000000, 0xc0139b4e8b50f62d), // -1/4
            (0xbfc999999999999a, 0xc01748db2b9da1e7), // -1/5
            (0xbfc0000000000000, 0xc0216f374f724016), // -1/8
            (0xbfd8000000000000, 0xc00e9a62b6d6488b), // -3/8
            (0xbfe4000000000000, 0xc00e5771fe7759f3), // -5/8
            (0xbfec000000000000, 0xc021386e9eef90a6), // -7/8
            (0xbfe6db6db6db6db7, 0xc011a292496bdc89), // -5/7
            (0xbfeb6db6db6db6db, 0xc01e8ec0a58a6611), // -6/7
            (0xbfcc71c71c71c71c, 0xc0156c3777a38e01), // -2/9
            (0xbfe8e38e38e38e39, 0xc0151e9af6b0b06c), // -7/9
            (0xbfe6666666666666, 0xc011183cf1a167e7), // -7/10
            (0xbfdaaaaaaaaaaaab, 0xc00d59e9547e6786), // -5/12
            (0xbfe2aaaaaaaaaaab, 0xc00d2d8c847340a9), // -7/12
            (0xbfb0000000000000, 0xc030a490a6519230), // -1/16
            (0xbfd4000000000000, 0xc010dfcc07c2e191), // -5/16
            (0xbfdc000000000000, 0xc00ced502d8aa3e2), // -7/16
            (0xbfe2000000000000, 0xc00ccc1c3adbbfb7), // -9/16
            (0xbfee000000000000, 0xc030836bfc70aa15), // -15/16
            // Thirteenths k/13-1 where worksheet GAMMA(x)=GAMMA(x+1)/x is exact.
            // IEEE seed/(x) is not the graph (1/13 peel is 5 ULP).
            (0xbfed89d89d89d89e, 0xc02b11f4b3ab91aa), // 1/13-1
            (0xbfeb13b13b13b13b, 0xc01ca18c0c19e7d7), // 2/13-1
            (0xbfe13b13b13b13b1, 0xc00c820b8b8eb74d), // 6/13-1
            (0xbfdd89d89d89d89e, 0xc00c96719f956f0c), // 7/13-1
            (0xbfd3b13b13b13b14, 0xc011090746848215), // 9/13-1
            (0xbfcd89d89d89d89c, 0xc014cfaedfea7c42), // 10/13-1
            (0xbfc3b13b13b13b14, 0xc01d05347d1ec2db), // 11/13-1
            // Elevenths k/11-1 with exact worksheet peel (8/10; k=1,4 miss 1 ULP).
            (0xbfea2e8ba2e8ba2e, 0xc018d2e886307c56), // 2/11-1
            (0xbfe745d1745d1746, 0xc01230c10ecc110d), // 3/11-1
            (0xbfe1745d1745d174, 0xc00c937f26f08b22), // 5/11-1
            (0xbfdd1745d1745d18, 0xc00cab9d2bd7bcd6), // 6/11-1
            (0xbfd745d1745d1746, 0xc00f0ca9489ef0ba), // 7/11-1
            (0xbfd1745d1745d174, 0xc0126f236047e861), // 8/11-1
            (0xbfc745d1745d1744, 0xc0192d521a0dc168), // 9/11-1
            (0xbfb745d1745d1748, 0xc02759d36b08112c), // 10/11-1
            // Seventeenths k/17-1 with exact worksheet peel (5/16).
            (0xbfec3c3c3c3c3c3c, 0xc0223263939b0e07), // 2/17-1
            (0xbfea5a5a5a5a5a5a, 0xc01973f5b803fab7), // 3/17-1
            (0xbfd6969696969696, 0xc00f841167c5700c), // 11/17-1
            (0xbfce1e1e1e1e1e20, 0xc01481f52635d60f), // 13/17-1
            (0xbfae1e1e1e1e1e20, 0xc031a384fa1f7fe9), // 16/17-1
            (0xbfdb6db6db6db6dc, 0xc00d17f07153aa1f), // 4/7-1
            (0xbfe2492492492492, 0xc00cf1f647776ae2), // 3/7-1
            (0xbfd2492492492492, 0xc011dd28623c84b3), // 5/7-1
            (0xbfe6000000000000, 0xc010ace9d2fd5a80), // 5/16-1
            (0xbfc8000000000000, 0xc0188b2142750366), // 13/16-1
            (0xbfdc71c71c71c71c, 0xc00cd0199151d380), // 5/9-1
            (0xbfed555555555555, 0xc02916f40e4cd8ec), // 1/12-1
            (0xbfdaaaaaaaaaaaaa, 0xc00d59e9547e6784), // 7/12-1 (distinct from -5/12)
            (0xbfe8787878787878, 0xc014385d00d157bc), // 4/17-1
            (0xbfe6969696969696, 0xc0114f0f4378079b), // 5/17-1
            (0xbfe2d2d2d2d2d2d3, 0xc00d4935647fbd65), // 7/17-1
            (0xbfe0f0f0f0f0f0f1, 0xc00c705568bcba1a), // 8/17-1
            (0xbfde1e1e1e1e1e1e, 0xc00c7fed16f60228), // 9/17-1
            (0xbfda5a5a5a5a5a5a, 0xc00d78352b10042a), // 10/17-1
            (0xbfd2d2d2d2d2d2d2, 0xc011873615bc34d5), // 12/17-1
            (0xbfbe1e1e1e1e1e20, 0xc0226a7819660d8a), // 15/17-1
            (0xbfed1745d1745d17, 0xc0271ce8a1cdffa6), // 1/11-1
            (0xbfe45d1745d1745d, 0xc00ec37c94034c8b), // 4/11-1
            (0xbfe89d89d89d89d8, 0xc01484b4654e4e7d), // 3/13-1
            (0xbfe6276276276276, 0xc010d4c63c73aa89), // 4/13-1
            (0xbfe3b13b13b13b14, 0xc00e0581db1d0f46), // 5/13-1
            (0xbfd89d89d89d89d8, 0xc00e433357623f84), // 8/13-1
            (0xbfb3b13b13b13b10, 0xc02b517c3426d534), // 12/13-1
            (0xbfe999999999999a, 0xc016f447aae4691a), // 1/5-1
            (0xbfe3333333333333, 0xc00d93516281fe78), // 2/5-1
            (0xbfd999999999999a, 0xc00dc8aa107a27a7), // 3/5-1
            (0xbfc9999999999998, 0xc01748db2b9da1e8), // 4/5-1
            (0xbfec71c71c71c71c, 0xc0232d118093e5ec), // 1/9-1
            (0xbfe1c71c71c71c72, 0xc00cb29a4962fa3d), // 4/9-1
            (0xbfbc71c71c71c720, 0xc02366500d4f1a4c), // 8/9-1
            (0xbfeccccccccccccd, 0xc0252420fa9e47ee), // 1/10-1
            (0xbfd3333333333334, 0xc0114eb20e9c431d), // 7/10-1
            (0xbfb9999999999998, 0xc0255f61032f6359), // 9/10-1
            (0xbfe2aaaaaaaaaaaa, 0xc00d2d8c847340a8), // 5/12-1 (distinct from -7/12)
            (0xbfb5555555555558, 0xc0295547312eb20d), // 11/12-1
            (0xbfea000000000000, 0xc018328e296d19c4), // 3/16-1
            (0xbfe8000000000000, 0xc013562a82ff49f2), // 1/4-1  (worksheet peel 1 ULP)
            (0xbfc2492492492494, 0xc01ef6236c822d27), // 6/7-1  (distinct from -1/7)
            (0xbfeb6db6db6db6dc, 0xc01e8ec0a58a6616), // 1/7-1  (distinct from -6/7)
            (0xbfee1e1e1e1e1e1e, 0xc0318204fc0456a0), // 1/17-1
            (0xbfe4b4b4b4b4b4b4, 0xc00f34fd6869612f), // 6/17-1
            (0xbfc6969696969698, 0xc019d01d2ce313ac), // 14/17-1
            (0xbfe5555555555556, 0xc01012d97eaf6235), // 1/3-1  (distinct from -2/3)
            (0xbfd5555555555556, 0xc0103fd9ade928db), // 2/3-1  (distinct from -1/3)
        ];
        let xb = x.to_bits();
        for &(xx, gg) in &GAMMA_NEG_FRAC {
            if xb == xx {
                return Ok(f64::from_bits(gg));
            }
        }
    }

    // Second peel into (-2,-1) from exact first-peel thirteenths.
    if x > -2.0 && x < -1.0 {
        const GAMMA_NEG2_FRAC: [(u64, u64); 38] = [
            (0xbffd89d89d89d89e, 0x400f0457b7c6bb2d), // 2/13-2
            (0xbff3b13b13b13b14, 0x4010e8be15ee84f4), // 10/13-2
            (0xbff2762762762762, 0x401926a4f4f886c3), // 11/13-2
            (0xbffd1745d1745d17, 0x400b4e662d355592), // 2/11-2
            (0xbffba2e8ba2e8ba3, 0x40051007f62fa7f4), // 3/11-2
            (0xbff745d1745d1746, 0x4003b5fc0e2451d3), // 6/11-2
            (0xbff1745d1745d174, 0x402567ac77720fc5), // 10/11-2
            (0xbff3c3c3c3c3c3c4, 0x401099f737502731), // 13/17-2
            (0xbff9249249249249, 0x40026b6e2d7a89d6), // 3/7-2
            (0xbffb000000000000, 0x4003c373f094912f), // 5/16-2
            (0xbff3000000000000, 0x4014ab0e88ce53b4), // 13/16-2
            (0xbff71c71c71c71c7, 0x4003f287dac27ebb), // 5/9-2
            (0xbff6aaaaaaaaaaaa, 0x4004b7efff684912), // 7/12-2
            (0xbffccccccccccccd, 0x400981332fa874c7), // 1/5-2
            (0xbff999999999999a, 0x40027c12dd913f0b), // 2/5-2
            (0xbff6666666666666, 0x4005463054e98a0a), // 3/5-2
            (0xbff3333333333333, 0x401367614f035c42), // 4/5-2
            (0xbffe38e38e38e38e, 0x40144dd64be7e463), // 1/9-2
            (0xbff8e38e38e38e39, 0x400272d0e608c570), // 4/9-2
            (0xbff4cccccccccccd, 0x400aa074653f2c2c), // 7/10-2
            (0xbffc3c3c3c3c3c3c, 0x4006ea8b8975c9d5), // 4/17-2
            (0xbff8787878787878, 0x40029837d82ca111), // 8/17-2
            (0xbff7878787878788, 0x4003613ad22c67dd), // 9/17-2
            (0xbffa2e8ba2e8ba2e, 0x4002cccc21903d00), // 4/11-2
            (0xbff13b13b13b13b1, 0x40295df354ff7cd5), // 12/13-2
            (0xbff6db6db6db6db7, 0x40045d8eb5ba90af), // 4/7-2
            (0xbff4924924924924, 0x400bc9cd0a970753), // 5/7-2
            (0xbffdb6db6db6db6e, 0x4010744059232347), // 1/7-2
            (0xbffb6db6db6db6dc, 0x4004930000532bf6), // 2/7-2
            (0xbffb333333333333, 0x40041c83ef183e01), // 3/10-2
            (0xbfff000000000000, 0x40210bca4ee7e960), // 1/16-2
            (0xbff6000000000000, 0x400641bc27e1a91f), // 5/8-2
            (0xbffe000000000000, 0x40125e53dcbb44f5), // 1/8-2
            (0xbff2000000000000, 0x401efe9b37e78e60), // 7/8-2
            (0xbffc000000000000, 0x4006195527ff2ff0), // 1/4-2
            (0xbff7000000000000, 0x40041f85b0607201), // 9/16-2
            (0xbff9000000000000, 0x40026e268c11c261), // 7/16-2
            (0xbfff0f0f0f0f0f0f, 0x402209d6970c3a38), // 1/17-2
        ];
        let xb = x.to_bits();
        for &(xx, gg) in &GAMMA_NEG2_FRAC {
            if xb == xx {
                return Ok(f64::from_bits(gg));
            }
        }
    }

    // Third peel into (-3,-2) from exact second-peel thirteenths (3/3).
    if x > -3.0 && x < -2.0 {
        const GAMMA_NEG3_FRAC: [(u64, u64); 19] = [
            (0xc006c4ec4ec4ec4f, 0xbff5cbb342dead0b), // 2/13-3
            (0xc001d89d89d89d8a, 0xbffe51e215abb09b), // 10/13-3
            (0xc0013b13b13b13b1, 0xc0075abdbee6c648), // 11/13-3
            (0xc0068ba2e8ba2e8c, 0xbff360edac786e4b), // 2/11-3
            (0xc003a2e8ba2e8ba3, 0xbff00f8b020aa17c), // 6/11-3
            (0xc000ba2e8ba2e8ba, 0xc014796d50dc6821), // 10/11-3
            (0xc004924924924925, 0xbfeca71d2a4cd669), // 3/7-3
            (0xc001800000000000, 0xc002e589a1a6b2ee), // 13/16-3
            (0xc0038e38e38e38e4, 0xbff0521210137ef6), // 5/9-3
            (0xc004cccccccccccd, 0xbfec701d061a885f), // 2/5-3
            (0xc003333333333333, 0xbff1ba7d9c17f309), // 3/5-3
            (0xc0051745d1745d17, 0xbfec862ce379bda8), // 4/11-3
            (0xc00199999999999a, 0xc001a3ccd3776b1f), // 4/5-3
            (0xc002666666666666, 0xbff7275a1542102b), // 7/10-3
            (0xc001e1e1e1e1e1e2, 0xbffdb533a6598980), // 13/17-3
            (0xc0036db6db6db6dc, 0xbff0c58495a8b362), // 4/7-3
            (0xc00599999999999a, 0xbfedcb6e1fd81004), // 3/10-3
            (0xc003000000000000, 0xbff2be17b5cb80f2), // 5/8-3
            (0xc003800000000000, 0xbff082e3d8ecac50), // 9/16-3
        ];
        let xb = x.to_bits();
        for &(xx, gg) in &GAMMA_NEG3_FRAC {
            if xb == xx {
                return Ok(f64::from_bits(gg));
            }
        }
    }

    // Fourth peel into (-4,-3) from exact third-peel 13ths/11ths (6/6).
    if x > -4.0 && x < -3.0 {
        const GAMMA_NEG4_FRAC: [(u64, u64); 14] = [
            (0xc00ec4ec4ec4ec4f, 0x3fd6aae36443be34), // 2/13-4
            (0xc009d89d89d89d8a, 0x3fe2c4f9abe43060), // 10/13-4
            (0xc0093b13b13b13b1, 0x3fed9ecb308ed604), // 11/13-4
            (0xc00e8ba2e8ba2e8c, 0x3fd44d29c0dfb07f), // 2/11-4
            (0xc00ba2e8ba2e8ba3, 0x3fd298bbe76aa009), // 6/11-4
            (0xc008ba2e8ba2e8ba, 0x3ffa7f05f02c4a85), // 10/11-4
            (0xc00c924924924925, 0x3fd00ba9eeba6397), // 3/7-4
            (0xc009800000000000, 0x3fe7b6a2acbd1cc6), // 13/16-4
            (0xc00b333333333333, 0x3fd4db6699859665), // 3/5-4
            (0xc009e1e1e1e1e1e2, 0x3fe25d65bf455058), // 13/17-4
            (0xc00b6db6db6db6dc, 0x3fd3911aae9a269d), // 4/7-4
            (0xc00d99999999999a, 0x3fd01aef6b28aeb6), // 3/10-4
            (0xc00b800000000000, 0x3fd336901d00c882), // 9/16-4
            (0xc00e000000000000, 0x3fd124b7f9ca9e35), // 1/4-4
        ];
        let xb = x.to_bits();
        for &(xx, gg) in &GAMMA_NEG4_FRAC {
            if xb == xx {
                return Ok(f64::from_bits(gg));
            }
        }
    }

    // Fifth peel into (-5,-4). 11ths k=2,6,10 exact 3/3; 13ths miss 1-2 ULP.
    if x > -5.0 && x < -4.0 {
        const GAMMA_NEG5_FRAC: [(u64, u64); 9] = [
            (0xc01345d1745d1746, 0xbfb0daa03f849286), // 2/11-5
            (0xc011d1745d1745d2, 0xbfb0b2f1df79de0f), // 6/11-5
            (0xc0105d1745d1745d, 0xbfd9e84a12a87660), // 10/11-5
            (0xc012492492492492, 0xbfac146961c62e4a), // 3/7-5
            (0xc0128ba2e8ba2e8c, 0xbfab11e8576075be), // 4/11-5
            (0xc010f0f0f0f0f0f1, 0xbfc158356d8892fe), // 13/17-5
            (0xc01199999999999a, 0xbfb2f6002e79716d), // 3/5-5
            (0xc011800000000000, 0xbfb44f33f8314ec5), // 5/8-5
            (0xc013c00000000000, 0xbfc31a54d7fe74d3), // 1/16-5
        ];
        let xb = x.to_bits();
        for &(xx, gg) in &GAMMA_NEG5_FRAC {
            if xb == xx {
                return Ok(f64::from_bits(gg));
            }
        }
    }

    // Sixth peel into (-6,-5). 11ths k=6,10 exact; k=2 misses 1 ULP.
    if x > -6.0 && x < -5.0 {
        const GAMMA_NEG6_FRAC: [(u64, u64); 6] = [
            (0xc015d1745d1745d2, 0x3f887deb47c3ce38), // 6/11-6
            (0xc0145d1745d1745d, 0x3fb45b15a0f213de), // 10/11-6
            (0xc016492492492492, 0x3f8428e9328127cc), // 3/7-6
            (0xc01599999999999a, 0x3f8c171cb6a0f3e3), // 3/5-6
            (0xc015800000000000, 0x3f8e3a5941c66955), // 5/8-6
            (0xc017c00000000000, 0x3f99bd13fffdeb8d), // 1/16-6
        ];
        let xb = x.to_bits();
        for &(xx, gg) in &GAMMA_NEG6_FRAC {
            if xb == xx {
                return Ok(f64::from_bits(gg));
            }
        }
    }

    // Seventh peel into (-7,-6). 11ths k=10 exact; k=6 misses 1 ULP.
    if x > -7.0 && x < -6.0 {
        const GAMMA_NEG7_FRAC: [(u64, u64); 3] = [
            (0xc0185d1745d1745d, 0xbf8abc68d3642961), // 10/11-7
            (0xc01a492492492492, 0xbf588ad91c17aae2), // 3/7-7
            (0xc01999999999999a, 0xbf618e71f224986e), // 3/5-7
        ];
        let xb = x.to_bits();
        for &(xx, gg) in &GAMMA_NEG7_FRAC {
            if xb == xx {
                return Ok(f64::from_bits(gg));
            }
        }
    }

    // Eighth peel into (-8,-7). 3/5-8 exact; 5/8-8 misses 1 ULP.
    if x > -8.0 && x < -7.0 {
        const GAMMA_NEG8_FRAC: [(u64, u64); 1] = [
            (0xc01d99999999999a, 0x3f32fadc0cb1f0e5), // 3/5-8
        ];
        let xb = x.to_bits();
        for &(xx, gg) in &GAMMA_NEG8_FRAC {
            if xb == xx {
                return Ok(f64::from_bits(gg));
            }
        }
    }

    // Isolated further peel: 10/11-11 is exact; 10/11-8..10 and -12 miss.
    if x > -11.0 && x < -10.0 {
        const GAMMA_NEG11_FRAC: [(u64, u64); 1] = [
            (0xc0242e8ba2e8ba2f, 0xbec4ceb5db83f371), // 10/11-11
        ];
        let xb = x.to_bits();
        for &(xx, gg) in &GAMMA_NEG11_FRAC {
            if xb == xx {
                return Ok(f64::from_bits(gg));
            }
        }
    }

    let ln_gamma = if x < 0.5 {
        let reflected = 1.0 - x;
        let denom = (std::f64::consts::PI * x).sin();
        if denom == 0.0 || !denom.is_finite() {
            return Err(WorksheetErrorCode::Num);
        }
        std::f64::consts::PI.ln() - denom.abs().ln() - ln_gamma_positive(reflected)?
    } else {
        ln_gamma_positive(x)?
    };

    if ln_gamma > f64::MAX.ln() {
        return Err(WorksheetErrorCode::Num);
    }

    let magnitude = ln_gamma.exp();
    let value = if x < 0.5 && (std::f64::consts::PI * x).sin().is_sign_negative() {
        -magnitude
    } else {
        magnitude
    };

    if !value.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(value)
}

pub fn gammaln_kernel(x: f64) -> Result<f64, WorksheetErrorCode> {
    // Published GAMMALN / GAMMALN.PRECISE surface. The full positive numeric
    // graph remains under W109 identification; this admission boundary is
    // independently pinned. Current-reference Excel rejects every positive
    // binary64 subnormal with #NUM! while admitting `f64::MIN_POSITIVE`; the
    // same guard also retains the non-positive domain. GAMMA and the shared
    // internal lgamma are unaffected.
    if !x.is_finite() || x < f64::MIN_POSITIVE {
        return Err(WorksheetErrorCode::Num);
    }
    // Live Excel 16.0 b20326: GAMMALN=LN(GAMMA) at these landed GAMMA
    // seeds (honest uint64 ULP, per-cell formulas). Not a general
    // composition: neighboring residues in the same families miss 1–40 ULP.
    // 1/2 already matches via the piecewise kernel. Product-first
    // extensions are included only where GAMMA(x) is already a landed
    // special-case and worksheet LN(GAMMA) matches GAMMALN.
    const GAMMALN_LN_GAMMA_X: [u64; 54] = [
        0x3fc999999999999a, // 1/5
        0x3fd5555555555555, // 1/3
        0x3fc2492492492492, // 1/7
        0x3fd2492492492492, // 2/7
        0x3fdb6db6db6db6db, // 3/7
        0x3fe6db6db6db6db7, // 5/7
        0x3fd8000000000000, // 3/8
        0x4011800000000000, // 4.375
        0x401d800000000000, // 7.375
        0x4022c00000000000, // 9.375
        0x3fd4000000000000, // 5/16
        0x4002800000000000, // 2.3125
        0x3fb745d1745d1746, // 1/11
        0x3fc745d1745d1746, // 2/11
        0x3fe1745d1745d174, // 6/11
        0x3fb3b13b13b13b14, // 1/13
        0x3fc3b13b13b13b14, // 2/13
        0x3ff13b13b13b13b1, // 1+1/13
        0x3fbe1e1e1e1e1e1e, // 2/17
        0x4011000000000000, // 4.25
        0x4015000000000000, // 5.25
        0x4019000000000000, // 6.25
        0x401d000000000000, // 7.25
        0x4010800000000000, // 4.125
        0x4018800000000000, // 6.125
        0x401c800000000000, // 7.125
        0x4022400000000000, // 9.125
        0x4016800000000000, // 5.625
        0x401e800000000000, // 7.625
        0x4018400000000000, // 6.0625
        0x401c400000000000, // 7.0625
        0x4012400000000000, // 4.5625
        0x401a400000000000, // 6.5625
        0x4021200000000000, // 8.5625
        0x4025200000000000, // 10.5625
        0x4017400000000000, // 5.8125
        0x40068ba2e8ba2e8c, // 2+9/11
        0x400e8ba2e8ba2e8c, // 3+9/11
        0x400f45d1745d1746, // 3+10/11
        0x400bb13b13b13b14, // 3+6/13
        0x400c4ec4ec4ec4ec, // 3+7/13
        0x400d89d89d89d89e, // 3+9/13
        0x4012c4ec4ec4ec4f, // 4+9/13
        0x400b4b4b4b4b4b4b, // 3+7/17
        0x400c3c3c3c3c3c3c, // 3+9/17
        0x400cb4b4b4b4b4b5, // 3+10/17
        0x400d2d2d2d2d2d2d, // 3+11/17
        0x400c71c71c71c71c, // 3+5/9
        0x3fcc71c71c71c71c, // 2/9
        0x3fd3333333333333, // 3/10
        0x4016276276276276, // 5+7/13
        0x401a276276276276, // 6+7/13
        0x40121e1e1e1e1e1e, // 4+9/17
        0x4012d2d2d2d2d2d3, // 4+12/17
    ];
    if GAMMALN_LN_GAMMA_X.contains(&x.to_bits()) {
        return Ok(crate::ported::excel_numeric::excel_log(gamma_kernel(x)?));
    }
    // Half-integers n+0.5 where GAMMA is the landed product-first fold
    // and worksheet GAMMALN=LN(GAMMA). Neighboring halves miss 1–4 ULP.
    // 0.5 already matches via the piecewise kernel.
    if x > 0.0 && x.fract() == 0.5 {
        const GAMMALN_LN_GAMMA_HALF_N: [u32; 89] = [
            2, 3, 4, 8, 9, 11, 13, 15, 16, 18, 19, 20, 21, 22, 23, 26, 27, 29, 30, 32, 34, 35, 38,
            40, 42, 44, 47, 50, 51, 52, 53, 54, 55, 56, 57, 63, 67, 70, 71, 73, 76, 77, 80, 83, 85,
            86, 92, 93, 94, 95, 96, 98, 102, 104, 105, 106, 110, 111, 112, 113, 122, 123, 125, 128,
            130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 141, 142, 147, 151, 157, 158, 159,
            160, 161, 163, 165, 166, 168, 169, 170,
        ];
        if GAMMALN_LN_GAMMA_HALF_N.contains(&(x as u32)) {
            return Ok(crate::ported::excel_numeric::excel_log(gamma_kernel(x)?));
        }
    }
    // Integers n=4..=88 where GAMMA is the reverse product and worksheet
    // GAMMALN=LN(GAMMA). 41/86 exact; the rest miss 1–2 ULP (n=3 included).
    if (4.0..=88.0).contains(&x) && x.fract() == 0.0 {
        const GAMMALN_LN_GAMMA_INT_N: [u32; 41] = [
            4, 5, 6, 7, 13, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 29, 31, 32, 37, 39, 41, 44, 48,
            49, 51, 53, 58, 59, 60, 64, 65, 67, 72, 76, 77, 78, 79, 84, 85, 86, 88,
        ];
        if GAMMALN_LN_GAMMA_INT_N.contains(&(x as u32)) {
            return Ok(crate::ported::excel_numeric::excel_log(gamma_kernel(x)?));
        }
    }
    Ok(crate::ported::excel_numeric::gammaln_excel(x))
}

pub fn gammaln_precise_kernel(x: f64) -> Result<f64, WorksheetErrorCode> {
    gammaln_kernel(x)
}

pub fn weibull_dist_kernel(
    x: f64,
    alpha: f64,
    beta: f64,
    cumulative: bool,
) -> Result<f64, WorksheetErrorCode> {
    if !x.is_finite()
        || !alpha.is_finite()
        || !beta.is_finite()
        || x < 0.0
        || alpha <= 0.0
        || beta <= 0.0
    {
        return Err(WorksheetErrorCode::Num);
    }

    if x == 0.0 {
        if cumulative {
            return Ok(0.0);
        }
        return Ok(0.0);
    }

    // W109 lane-1 identification (b24 + b27/b27b, all blocks bit-exact): the
    // WEIBULL body is a legacy x87 compilation unit — every op double-rounded
    // through a spilled double local, every pow the raw chain (no shortcuts).
    //   r   = RN53(RN64(x/β))                                   (b27b D2)
    //   t   = exp(RN53(RN64(α·ln r)))                           (b27D 113/113)
    //   cdf = −expm1(−t)                                        (Kahan expm1)
    //   pdf = DR(DR(DR(α/β^α)·x^(α−1))·exp(−t))                 (b27 T3|SS,
    //         division-first association: `alpha / pow(beta, alpha) *
    //         pow(x, alpha-1) * exp(-pow(x/beta, alpha))` left-to-right)
    use crate::ported::excel_numeric::{
        excel_exp, excel_expm1_internal, excel_pow_chain, excel_x87_div, excel_x87_mul,
    };
    let ratio = excel_x87_div(x, beta);
    let power = excel_pow_chain(ratio, alpha);
    let value = if cumulative {
        -excel_expm1_internal(-power)
    } else {
        let pba = excel_pow_chain(beta, alpha);
        let px = excel_pow_chain(x, alpha - 1.0);
        let e = excel_exp(-power);
        excel_x87_mul(excel_x87_mul(excel_x87_div(alpha, pba), px), e)
    };

    if !value.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    Ok(value)
}

pub fn weibull_kernel(
    x: f64,
    alpha: f64,
    beta: f64,
    cumulative: bool,
) -> Result<f64, WorksheetErrorCode> {
    weibull_dist_kernel(x, alpha, beta, cumulative)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64, tol: f64) {
        let delta = (actual - expected).abs();
        assert!(
            delta <= tol,
            "expected {expected}, got {actual}, delta {delta}"
        );
    }

    fn assert_bits_eq(label: &str, actual: f64, expected: f64) {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{label}: actual {actual:e} ({:#018x}) vs expected {expected:e} ({:#018x})",
            actual.to_bits(),
            expected.to_bits()
        );
    }

    #[test]
    fn erfc_one_matches_excel_exact_bits() {
        assert_bits_eq("erfc(1)", erfc_kernel(1.0).unwrap(), 0.15729920705028513);
    }

    #[test]
    fn erfc_positive_half_is_one_minus_erf() {
        // Live Excel 16.0 b20326: ERFC.PRECISE(z)==1-ERF.PRECISE(z) on
        // (0, 0.5]. Production publishes that composition through erf_approx.
        for k in 1..=16 {
            let z = f64::from(k) / 32.0;
            let got = erfc_precise_kernel(z).unwrap();
            let want = 1.0 - erf_precise_kernel(z).unwrap();
            assert_eq!(got.to_bits(), want.to_bits(), "z={z}");
        }
        // Capture pins (cell Value2, build 20326) where 1-erf matches Excel.
        assert_eq!(
            erfc_precise_kernel(0.00048828125).unwrap().to_bits(),
            0x3feffb7c8a31f3f6
        );
        assert_eq!(
            erfc_precise_kernel(0.125).unwrap().to_bits(),
            0x3feb82879728f11e
        );
        assert_eq!(
            erfc_precise_kernel(0.4775390625).unwrap().to_bits(),
            0x3fdff72039b2c25e
        );
        assert_eq!(
            erfc_precise_kernel(0.5).unwrap().to_bits(),
            0x3fdeb02147ce245c
        );
    }

    #[test]
    fn erfc_family_direct_call_witnesses() {
        // Excel-anchored exact bits. These are all in the libm-matching
        // regime (x <= 1.25, or all negatives).
        assert_bits_eq("erfc(0)", erfc_kernel(0.0).unwrap(), 1.0);
        assert_bits_eq("erfc(0.5)", erfc_kernel(0.5).unwrap(), 0.4795001221869535);
        assert_bits_eq("erfc(1)", erfc_kernel(1.0).unwrap(), 0.15729920705028513);
        assert_bits_eq(
            "erfc(1.25)",
            erfc_kernel(1.25).unwrap(),
            0.07709987174354177,
        );
        assert_bits_eq("erfc(-1)", erfc_kernel(-1.0).unwrap(), 1.8427007929497148);
        assert_bits_eq("erfc(-2)", erfc_kernel(-2.0).unwrap(), 1.9953222650189528);

        // Libm-matching pockets within the positive regime (small island).
        assert_bits_eq(
            "erfc(2.75)",
            erfc_kernel(2.75).unwrap(),
            0.00010062192211963684,
        );
        assert_bits_eq("erfc(2.8)", erfc_kernel(2.8).unwrap(), 7.501319466545911e-5);

        // ERFC.PRECISE delegates to the same kernel; spot-check parity at
        // one representative in-range anchor and one blocked-regime pocket.
        assert_bits_eq("erfc.precise(0)", erfc_precise_kernel(0.0).unwrap(), 1.0);
        assert_bits_eq(
            "erfc.precise(1)",
            erfc_precise_kernel(1.0).unwrap(),
            0.15729920705028513,
        );
        assert_bits_eq(
            "erfc.precise(-1)",
            erfc_precise_kernel(-1.0).unwrap(),
            1.8427007929497148,
        );

        // Stable family controls across the full widened positive range.
        let xs: &[f64] = &[
            0.0, 0.5, 1.0, 1.25, 1.5, 1.75, 1.9, 2.0, 2.1, 2.25, 2.5, 2.6, 2.7, 2.75, 2.8, 2.9,
            3.0, 3.5, 4.0, 5.0, 6.0, 8.0, 10.0,
        ];
        let mut prev: Option<f64> = None;
        for &x in xs {
            let v = erfc_kernel(x).unwrap();
            // Range: erfc(x) in (0, 1] for x >= 0 (equals 1 exactly at 0).
            assert!(v > 0.0 && v <= 1.0, "erfc({x}) = {v} out of range");
            // Strict monotone-decreasing on positives.
            if let Some(p) = prev {
                assert!(p > v, "monotone: erfc(prev) > erfc({x}), got {p} !> {v}");
            }
            prev = Some(v);
        }

        // Reflection: erfc(-x) + erfc(x) ≈ 2 (tight ULP-scale bound; exact equality
        // is not guaranteed because both summands are rounded f64 values).
        let e_one = erfc_kernel(1.0).unwrap();
        let e_neg_one = erfc_kernel(-1.0).unwrap();
        let e_two = erfc_kernel(2.0).unwrap();
        let e_neg_two = erfc_kernel(-2.0).unwrap();
        assert!(
            (e_neg_one + e_one - 2.0).abs() < 1e-15,
            "reflection erfc(-1)+erfc(1) = {}",
            e_neg_one + e_one
        );
        assert!(
            (e_neg_two + e_two - 2.0).abs() < 1e-15,
            "reflection erfc(-2)+erfc(2) = {}",
            e_neg_two + e_two
        );
    }

    // Excel-matching exact-bit witnesses for every input where the
    // empirical correction-fit kernel reproduces Excel's bits. These
    // pass on every platform (no UCRT dependency).
    #[test]
    fn erfc_correction_fit_matches_excel_exact_bits() {
        // Newly-matched by the correction fit (previously blocked).
        assert_bits_eq("erfc(1.5)", erfc_kernel(1.5).unwrap(), 0.03389485352468927);
        assert_bits_eq("erfc(1.8)", erfc_kernel(1.8).unwrap(), 0.010909498364269283);
        assert_bits_eq(
            "erfc(2.15)",
            erfc_kernel(2.15).unwrap(),
            0.002361392962674656,
        );
        assert_bits_eq(
            "erfc(2.25)",
            erfc_kernel(2.25).unwrap(),
            0.0014627165866811515,
        );
        assert_bits_eq(
            "erfc(2.4)",
            erfc_kernel(2.4).unwrap(),
            0.0006885138966450787,
        );
        assert_bits_eq(
            "erfc(2.5)",
            erfc_kernel(2.5).unwrap(),
            0.00040695201744495886,
        );
        assert_bits_eq(
            "erfc(2.99)",
            erfc_kernel(2.99).unwrap(),
            2.3525603080640202e-5,
        );
        assert_bits_eq(
            "erfc(3.25)",
            erfc_kernel(3.25).unwrap(),
            4.302779463675121e-6,
        );

        // Large-x anchors preserved from the UCRT round.
        assert_bits_eq("erfc(3)", erfc_kernel(3.0).unwrap(), 2.209049699858544e-5);
        assert_bits_eq("erfc(4)", erfc_kernel(4.0).unwrap(), 1.5417257900280017e-8);
        assert_bits_eq("erfc(8)", erfc_kernel(8.0).unwrap(), 1.1224297172982929e-29);

        // Already-green anchors preserved (fit forces corr -> 0 here).
        assert_bits_eq(
            "erfc(1.85)",
            erfc_kernel(1.85).unwrap(),
            0.008888969943914289,
        );
        assert_bits_eq(
            "erfc(1.95)",
            erfc_kernel(1.95).unwrap(),
            0.005820666407810882,
        );
        assert_bits_eq(
            "erfc(2.75)",
            erfc_kernel(2.75).unwrap(),
            0.00010062192211963684,
        );
        assert_bits_eq("erfc(2.8)", erfc_kernel(2.8).unwrap(), 7.501319466545911e-5);
        assert_bits_eq(
            "erfc(3.001)",
            erfc_kernel(3.001).unwrap(),
            2.1951660917737304e-5,
        );

        // ERFC.PRECISE parity spot-checks — same kernel.
        assert_bits_eq(
            "erfc.precise(1.5)",
            erfc_precise_kernel(1.5).unwrap(),
            0.03389485352468927,
        );
        assert_bits_eq(
            "erfc.precise(3)",
            erfc_precise_kernel(3.0).unwrap(),
            2.209049699858544e-5,
        );
    }

    // Exact-bit Excel witnesses still blocked by the correction-fit kernel.
    // The remaining residual is chaotic at the ULP level — no smoothly
    // representable polynomial correction reproduces these. Kept as
    // #[ignore]d sentinels; enable via `cargo test -- --ignored` when a
    // kernel candidate targeting them is in flight.
    #[test]
    #[ignore = "Excel residual not reproducible via smooth correction polynomial; see docs/function-lane/ERFC_EXCEL_EMULATION.md"]
    fn erfc_remaining_blocked_excel_witnesses() {
        let cases: &[(f64, f64)] = &[
            (1.6, 0.023651616655355978),
            (1.7, 0.01620954140922544),
            (1.75, 0.013328328780817557),
            (1.9, 0.007209570764742528),
            (2.0, 0.0046777349810472645),
            (2.05, 0.0037419039555431272),
            (2.1, 0.002979466656332985),
            (2.35, 0.000889267032132454),
            (2.45, 0.0005305801122510537),
            (2.55, 0.0003106603426391907),
            (2.6, 0.000236034416529349),
            (2.65, 0.00017848775202400087),
            (2.7, 0.0001343327399405242),
            (2.85, 5.565627996139894e-5),
            (2.9, 4.1097878099458844e-5),
            (2.95, 3.0203042064138246e-5),
            (2.999, 2.2230168599834054e-5),
            (3.005, 2.1404577729752717e-5),
            (3.01, 2.0738963637132638e-5),
            (3.02, 1.946639071441418e-5),
            (3.5, 7.430983723414129e-7),
            (3.75, 1.1372725656979669e-7),
            (4.5, 1.9661604415428865e-10),
            (5.0, 1.537459794428034e-12),
            (6.0, 2.151973671249892e-17),
            (7.0, 4.1838256077794166e-23),
            (9.0, 4.137031746513812e-37),
            (10.0, 2.0884875837625446e-45),
        ];
        for (x, excel) in cases.iter().copied() {
            let got = erfc_kernel(x).unwrap();
            assert_eq!(
                got.to_bits(),
                excel.to_bits(),
                "erfc({x}): got {got:e} ({:#018x}), excel {excel:e} ({:#018x})",
                got.to_bits(),
                excel.to_bits()
            );
        }
    }

    #[test]
    fn erf_family_matches_seed_rows() {
        assert_close(erf_kernel(1.0, None).unwrap(), 0.8427007929497149, 1e-15);
        assert_close(
            erf_kernel(0.0, Some(1.0)).unwrap(),
            0.8427007929497149,
            1e-15,
        );
        assert_close(erf_precise_kernel(1.0).unwrap(), 0.8427007929497149, 1e-15);
        assert_close(erfc_kernel(1.0).unwrap(), 0.15729920705028513, 1e-15);
        assert_close(
            erfc_precise_kernel(-1.0).unwrap(),
            1.8427007929497148,
            1e-15,
        );
        assert_close(
            erf_kernel(1.0, Some(2.0)).unwrap(),
            0.15262147206923782,
            1e-15,
        );
        assert_close(
            erf_kernel(2.0, Some(1.0)).unwrap(),
            -0.15262147206923782,
            1e-15,
        );
    }

    #[test]
    fn gamma_family_matches_seed_rows() {
        assert_close(gamma_kernel(5.0).unwrap(), 24.0, 1e-12);
        assert_close(gamma_kernel(0.5).unwrap(), 1.772453850905516, 1e-12);
        assert_close(gamma_kernel(-0.5).unwrap(), -3.5449077018110318, 1e-10);
        assert_close(gammaln_kernel(5.0).unwrap(), 3.1780538303479458, 1e-12);
        assert_close(
            gammaln_precise_kernel(0.5).unwrap(),
            0.5723649429247001,
            1e-12,
        );
        assert_eq!(gamma_kernel(-1.0), Err(WorksheetErrorCode::Num));
        assert_eq!(gammaln_kernel(0.0), Err(WorksheetErrorCode::Num));
        assert_eq!(gamma_kernel(172.0), Err(WorksheetErrorCode::Num));
    }

    #[test]
    fn gamma_positive_integers_1_through_88_match_excel_reverse_product() {
        assert_eq!(gamma_kernel(1.0).unwrap().to_bits(), 0x3ff0000000000000);
        assert_eq!(gamma_kernel(8.0).unwrap().to_bits(), 0x40b3b00000000000);
        assert_eq!(gamma_kernel(10.0).unwrap().to_bits(), 0x4116260000000000);
        assert_eq!(gamma_kernel(26.0).unwrap().to_bits(), 0x4529a940c33f6120);
        assert_eq!(gamma_kernel(50.0).unwrap().to_bits(), 0x4cf7a88e4484be3f);
        assert_eq!(gamma_kernel(88.0).unwrap().to_bits(), 0x5b67c1863ed21d6f);
        assert_eq!(gamma_kernel(0.5).unwrap().to_bits(), 0x3ffc5bf891b4ef6b);
        assert_eq!(gamma_kernel(1.5).unwrap().to_bits(), 0x3fec5bf891b4ef6b);
        assert_eq!(gamma_kernel(5.5).unwrap().to_bits(), 0x404a2be0247739f2);
        assert_eq!(gamma_kernel(8.5).unwrap().to_bits(), 0x40cb693422315f91);
        assert_eq!(gamma_kernel(15.5).unwrap().to_bits(), 0x42537d7bedf4639d);
        // Recurrence from 15.5: live Excel 16.0 b20326 Value2.
        assert_eq!(gamma_kernel(16.5).unwrap().to_bits(), 0x4292e1900e84c080);
        assert_eq!(gamma_kernel(17.5).unwrap().to_bits(), 0x42d3789c8ef8e684);
        assert_eq!(gamma_kernel(18.5).unwrap().to_bits(), 0x43154beb3c603c20);
        assert_eq!(gamma_kernel(19.5).unwrap().to_bits(), 0x43589fc7fdcf4585);
        // Product-first * seed past the recurrence wall, live Excel 16.0 b20326.
        assert_eq!(gamma_kernel(20.5).unwrap().to_bits(), 0x439e02bbbd549cbb);
        assert_eq!(gamma_kernel(40.5).unwrap().to_bits(), 0x49b686d9486ca145);
        assert_eq!(gamma_kernel(45.5).unwrap().to_bits(), 0x4b67352493c5c248);
        assert_eq!(gamma_kernel(80.5).unwrap().to_bits(), 0x5869585f6c854a5d);
        assert_eq!(gamma_kernel(171.5).unwrap().to_bits(), 0x7fe0e1863dcad78a);
        assert_eq!(gamma_kernel(172.5), Err(WorksheetErrorCode::Num));
        // Quarter-integers n=0..=5, live Excel 16.0 b20326 Value2.
        assert_eq!(gamma_kernel(0.25).unwrap().to_bits(), 0x400d013fc47eeeec);
        assert_eq!(gamma_kernel(1.25).unwrap().to_bits(), 0x3fed013fc47eeeec);
        assert_eq!(gamma_kernel(2.25).unwrap().to_bits(), 0x3ff220c7dacf5554);
        assert_eq!(gamma_kernel(3.25).unwrap().to_bits(), 0x400464e0d6293ffe);
        assert_eq!(gamma_kernel(4.25).unwrap().to_bits(), 0x402091f6ae0183fe);
        assert_eq!(gamma_kernel(5.25).unwrap().to_bits(), 0x40419b1618e19c3e);
        assert_eq!(gamma_kernel(6.25).unwrap().to_bits(), 0x40671b8d00a81d12);
        assert_eq!(gamma_kernel(7.25).unwrap().to_bits(), 0x40920d86288356b6);
        assert_eq!(gamma_kernel(8.25).unwrap().to_bits(), 0x40c05c4194b70695);
        assert_eq!(gamma_kernel(9.25).unwrap().to_bits(), 0x40f0df23a15cbec9);
        assert_eq!(gamma_kernel(0.75).unwrap().to_bits(), 0x3ff39b4e8b50f62d);
        assert_eq!(gamma_kernel(1.75).unwrap().to_bits(), 0x3fed68f5d0f97144);
        assert_eq!(gamma_kernel(2.75).unwrap().to_bits(), 0x3ff9bbd716da431c);
        assert_eq!(gamma_kernel(3.75).unwrap().to_bits(), 0x4011b123dfb60e23);
        assert_eq!(gamma_kernel(4.75).unwrap().to_bits(), 0x40309611a1baad41);
        assert_eq!(gamma_kernel(5.75).unwrap().to_bits(), 0x4053b234f00dadbd);
        // Odd eighths, live Excel 16.0 b20326 Value2.
        assert_eq!(gamma_kernel(0.125).unwrap().to_bits(), 0x401e22c196233d23);
        assert_eq!(gamma_kernel(1.125).unwrap().to_bits(), 0x3fee22c196233d23);
        assert_eq!(gamma_kernel(2.125).unwrap().to_bits(), 0x3ff0f38ce473d264);
        assert_eq!(gamma_kernel(3.125).unwrap().to_bits(), 0x400202c5b2bb0f8a);
        assert_eq!(gamma_kernel(4.125).unwrap().to_bits(), 0x401c2454e7444847);
        assert_eq!(gamma_kernel(9.125).unwrap().to_bits(), 0x40e9c048b34aaea7);
        assert_eq!(gamma_kernel(0.375).unwrap().to_bits(), 0x4002f6a73f0a9838);
        assert_eq!(gamma_kernel(1.375).unwrap().to_bits(), 0x3fec71fade8fe454);
        assert_eq!(gamma_kernel(2.375).unwrap().to_bits(), 0x3ff38e5c7902ecfa);
        assert_eq!(gamma_kernel(3.375).unwrap().to_bits(), 0x4007390dcfb37969);
        assert_eq!(gamma_kernel(4.375).unwrap().to_bits(), 0x40239823a73f6e70);
        assert_eq!(gamma_kernel(9.375).unwrap().to_bits(), 0x40f625bd9ee3017b);
        assert_eq!(gamma_kernel(0.625).unwrap().to_bits(), 0x3ff6f3ca0920b668);
        assert_eq!(gamma_kernel(1.625).unwrap().to_bits(), 0x3fecb0bc8b68e402);
        assert_eq!(gamma_kernel(2.625).unwrap().to_bits(), 0x3ff74f9931453942);
        assert_eq!(gamma_kernel(3.625).unwrap().to_bits(), 0x400e987910aadb26);
        assert_eq!(gamma_kernel(9.625).unwrap().to_bits(), 0x41032ebaf2a910f7);
        assert_eq!(gamma_kernel(0.875).unwrap().to_bits(), 0x3ff16f374f724016);
        // Live Excel 16.0 b20326: GAMMALN(0.5)=LN(GAMMA(0.5)) bit-exact.
        assert_eq!(gammaln_kernel(0.5).unwrap().to_bits(), 0x3fe250d048e7a1bd);
        // Live Excel 16.0 b20326: GAMMALN=LN(GAMMA) at these seeds.
        assert_eq!(gammaln_kernel(0.2).unwrap().to_bits(), 0x3ff86290bf25b627);
        assert_eq!(
            gammaln_kernel(2.0 / 9.0).unwrap().to_bits(),
            0x3ff699f87438534a
        );
        assert_eq!(
            gammaln_kernel(3.0 / 10.0).unwrap().to_bits(),
            0x3ff188637a6c4196
        );
        assert_eq!(
            gammaln_kernel(5.0 + 7.0 / 13.0).unwrap().to_bits(),
            0x4010146785c4fdc6
        );
        assert_eq!(
            gammaln_kernel(6.0 + 7.0 / 13.0).unwrap().to_bits(),
            0x4016ed33cd208edd
        );
        assert_eq!(
            gammaln_kernel(4.0 + 9.0 / 17.0).unwrap().to_bits(),
            0x4003f5219f4f774a
        );
        assert_eq!(
            gammaln_kernel(4.0 + 12.0 / 17.0).unwrap().to_bits(),
            0x4005f57b9cdb1bbb
        );
        assert_eq!(
            gammaln_kernel(1.0 / 3.0).unwrap().to_bits(),
            0x3fef8890e16b741b
        );
        assert_eq!(
            gammaln_kernel(1.0 / 7.0).unwrap().to_bits(),
            0x3ffe1113cc526fa6
        );
        assert_eq!(
            gammaln_kernel(2.0 / 7.0).unwrap().to_bits(),
            0x3ff25a9c12810ff0
        );
        assert_eq!(
            gammaln_kernel(3.0 / 7.0).unwrap().to_bits(),
            0x3fe73e3996add6b2
        );
        assert_eq!(
            gammaln_kernel(5.0 / 7.0).unwrap().to_bits(),
            0x3fcf325cd39aec45
        );
        assert_eq!(gammaln_kernel(0.375).unwrap().to_bits(), 0x3feb9e4d53fc3074);
        assert_eq!(
            gammaln_kernel(0.3125).unwrap().to_bits(),
            0x3ff0d8e1683cdcd1
        );
        assert_eq!(
            gammaln_kernel(1.0 / 11.0).unwrap().to_bits(),
            0x4002d0c317ddb0c0
        );
        assert_eq!(
            gammaln_kernel(2.0 / 11.0).unwrap().to_bits(),
            0x3ff9ff587e1ad8ce
        );
        assert_eq!(
            gammaln_kernel(6.0 / 11.0).unwrap().to_bits(),
            0x3fdf3ad24caad6dc
        );
        assert_eq!(
            gammaln_kernel(1.0 / 13.0).unwrap().to_bits(),
            0x400433b1c2265eb7
        );
        assert_eq!(
            gammaln_kernel(2.0 / 13.0).unwrap().to_bits(),
            0x3ffcd17b700fc6c1
        );
        assert_eq!(
            gammaln_kernel(1.0 + 1.0 / 13.0).unwrap().to_bits(),
            0xbfa4549a42cb6665
        );
        assert_eq!(
            gammaln_kernel(2.0 / 17.0).unwrap().to_bits(),
            0x4000a9daf888fcc2
        );
        assert_eq!(gammaln_kernel(2.5).unwrap().to_bits(), 0x3fd2383e809a67e8);
        assert_eq!(gammaln_kernel(3.5).unwrap().to_bits(), 0x3ff3373018970a36);
        assert_eq!(gammaln_kernel(4.5).unwrap().to_bits(), 0x4003a140a3a623cb);
        assert_eq!(gammaln_kernel(8.5).unwrap().to_bits(), 0x402319398ed5be28);
        assert_eq!(gammaln_kernel(9.5).unwrap().to_bits(), 0x402760f04f64ba68);
        assert_eq!(gammaln_kernel(11.5).unwrap().to_bits(), 0x40304ac08b1145d1);
        assert_eq!(gammaln_kernel(4.375).unwrap().to_bits(), 0x400241b90aee4edd);
        assert_eq!(gammaln_kernel(7.375).unwrap().to_bits(), 0x401d2b26dad896ac);
        assert_eq!(gammaln_kernel(9.375).unwrap().to_bits(), 0x4026d4bac34612ef);
        assert_eq!(
            gammaln_kernel(2.3125).unwrap().to_bits(),
            0x3fc4b3a46906fd4c
        );
        assert_eq!(gammaln_kernel(13.5).unwrap().to_bits(), 0x4035429459d98a56);
        assert_eq!(gammaln_kernel(20.5).unwrap().to_bits(), 0x40446a6e9fba19d8);
        assert_eq!(gammaln_kernel(40.5).unwrap().to_bits(), 0x405b1e46dca78c15);
        assert_eq!(gammaln_kernel(55.5).unwrap().to_bits(), 0x4064ca49c7493f43);
        assert_eq!(gammaln_kernel(80.5).unwrap().to_bits(), 0x4070f7b05399f6c8);
        assert_eq!(gammaln_kernel(96.5).unwrap().to_bits(), 0x40757188eed3f4d4);
        assert_eq!(gammaln_kernel(136.5).unwrap().to_bits(), 0x4080a8514c766090);
        assert_eq!(gammaln_kernel(170.5).unwrap().to_bits(), 0x4086000911686cd6);
        assert_eq!(gammaln_kernel(4.0).unwrap().to_bits(), 0x3ffcab0bfa2a2002);
        assert_eq!(gammaln_kernel(5.0).unwrap().to_bits(), 0x40096ca77c922cf9);
        assert_eq!(gammaln_kernel(6.0).unwrap().to_bits(), 0x401326643c4479c9);
        assert_eq!(gammaln_kernel(7.0).unwrap().to_bits(), 0x401a51273acf01ca);
        assert_eq!(gammaln_kernel(13.0).unwrap().to_bits(), 0x4033fcba16d50143);
        assert_eq!(gammaln_kernel(25.0).unwrap().to_bits(), 0x404b6472034e8d14);
        assert_eq!(gammaln_kernel(41.0).unwrap().to_bits(), 0x405b94855c702ba2);
        assert_eq!(gammaln_kernel(60.0).unwrap().to_bits(), 0x406711152043b2c4);
        assert_eq!(gammaln_kernel(88.0).unwrap().to_bits(), 0x40730afd5d851956);
        assert_eq!(gammaln_kernel(4.25).unwrap().to_bits(), 0x4000ea6864c19995);
        assert_eq!(gammaln_kernel(5.25).unwrap().to_bits(), 0x400c7db2a73efc17);
        assert_eq!(gammaln_kernel(6.25).unwrap().to_bits(), 0x4014e0dfde18c6e8);
        assert_eq!(gammaln_kernel(7.25).unwrap().to_bits(), 0x401c35701a50ff06);
        assert_eq!(gammaln_kernel(4.125).unwrap().to_bits(), 0x3fff37280ef6ef35);
        assert_eq!(gammaln_kernel(9.125).unwrap().to_bits(), 0x4025bf06879ad95b);
        assert_eq!(gammaln_kernel(5.625).unwrap().to_bits(), 0x4010a49a664571a8);
        assert_eq!(
            gammaln_kernel(6.0625).unwrap().to_bits(),
            0x401393f1c1c12ab5
        );
        assert_eq!(
            gammaln_kernel(10.5625).unwrap().to_bits(),
            0x402c2b6556c4802a
        );
        assert_eq!(
            gammaln_kernel(5.8125).unwrap().to_bits(),
            0x4011e21e484a69c4
        );
        assert_eq!(
            gammaln_kernel(2.0 + 9.0 / 11.0).unwrap().to_bits(),
            0x3fe1069a7a544ca4
        );
        assert_eq!(
            gammaln_kernel(3.0 + 10.0 / 11.0).unwrap().to_bits(),
            0x3ffadc28e27d15b4
        );
        assert_eq!(
            gammaln_kernel(3.0 + 9.0 / 13.0).unwrap().to_bits(),
            0x3ff6b4a5c4f4a9fe
        );
        assert_eq!(
            gammaln_kernel(4.0 + 9.0 / 13.0).unwrap().to_bits(),
            0x4005cd86f3998423
        );
        assert_eq!(
            gammaln_kernel(3.0 + 11.0 / 17.0).unwrap().to_bits(),
            0x3ff5de146efd5ef9
        );
        assert_eq!(
            gammaln_kernel(3.0 + 5.0 / 9.0).unwrap().to_bits(),
            0x3ff4344afd96d888
        );
        assert_eq!(
            gamma_kernel(1.0 / 11.0).unwrap().to_bits(),
            0x402503020775740e
        );
        assert_eq!(
            gamma_kernel(2.0 / 11.0 - 1.0).unwrap().to_bits(),
            0xc018d2e886307c56
        );
        assert_eq!(
            gamma_kernel(10.0 / 11.0 - 1.0).unwrap().to_bits(),
            0xc02759d36b08112c
        );
        assert_eq!(
            gamma_kernel(2.0 / 17.0 - 1.0).unwrap().to_bits(),
            0xc0223263939b0e07
        );
        assert_eq!(
            gamma_kernel(16.0 / 17.0 - 1.0).unwrap().to_bits(),
            0xc031a384fa1f7fe9
        );
        assert_eq!(
            gamma_kernel(4.0 / 7.0 - 1.0).unwrap().to_bits(),
            0xc00d17f07153aa1f
        );
        assert_eq!(
            gamma_kernel(3.0 / 7.0 - 1.0).unwrap().to_bits(),
            0xc00cf1f647776ae2
        );
        assert_eq!(
            gamma_kernel(5.0 / 7.0 - 1.0).unwrap().to_bits(),
            0xc011dd28623c84b3
        );
        assert_eq!(
            gamma_kernel(0.3125 - 1.0).unwrap().to_bits(),
            0xc010ace9d2fd5a80
        );
        assert_eq!(
            gamma_kernel(0.8125 - 1.0).unwrap().to_bits(),
            0xc0188b2142750366
        );
        assert_eq!(
            gamma_kernel(5.0 / 9.0 - 1.0).unwrap().to_bits(),
            0xc00cd0199151d380
        );
        assert_eq!(
            gamma_kernel(1.0 / 12.0 - 1.0).unwrap().to_bits(),
            0xc02916f40e4cd8ec
        );
        assert_eq!(
            gamma_kernel(7.0 / 12.0 - 1.0).unwrap().to_bits(),
            0xc00d59e9547e6784
        );
        assert_eq!(
            gamma_kernel(3.0 / 7.0 - 2.0).unwrap().to_bits(),
            0x40026b6e2d7a89d6
        );
        assert_eq!(
            gamma_kernel(0.3125 - 2.0).unwrap().to_bits(),
            0x4003c373f094912f
        );
        assert_eq!(
            gamma_kernel(0.8125 - 2.0).unwrap().to_bits(),
            0x4014ab0e88ce53b4
        );
        assert_eq!(
            gamma_kernel(5.0 / 9.0 - 2.0).unwrap().to_bits(),
            0x4003f287dac27ebb
        );
        assert_eq!(
            gamma_kernel(7.0 / 12.0 - 2.0).unwrap().to_bits(),
            0x4004b7efff684912
        );
        assert_eq!(
            gamma_kernel(3.0 / 7.0 - 3.0).unwrap().to_bits(),
            0xbfeca71d2a4cd669
        );
        assert_eq!(
            gamma_kernel(0.8125 - 3.0).unwrap().to_bits(),
            0xc002e589a1a6b2ee
        );
        assert_eq!(
            gamma_kernel(5.0 / 9.0 - 3.0).unwrap().to_bits(),
            0xbff0521210137ef6
        );
        assert_eq!(
            gamma_kernel(3.0 / 7.0 - 4.0).unwrap().to_bits(),
            0x3fd00ba9eeba6397
        );
        assert_eq!(
            gamma_kernel(0.8125 - 4.0).unwrap().to_bits(),
            0x3fe7b6a2acbd1cc6
        );
        assert_eq!(
            gamma_kernel(4.0 / 17.0 - 1.0).unwrap().to_bits(),
            0xc014385d00d157bc
        );
        assert_eq!(
            gamma_kernel(8.0 / 17.0 - 1.0).unwrap().to_bits(),
            0xc00c705568bcba1a
        );
        assert_eq!(
            gamma_kernel(15.0 / 17.0 - 1.0).unwrap().to_bits(),
            0xc0226a7819660d8a
        );
        assert_eq!(
            gamma_kernel(1.0 / 11.0 - 1.0).unwrap().to_bits(),
            0xc0271ce8a1cdffa6
        );
        assert_eq!(
            gamma_kernel(4.0 / 11.0 - 1.0).unwrap().to_bits(),
            0xc00ec37c94034c8b
        );
        assert_eq!(
            gamma_kernel(3.0 / 13.0 - 1.0).unwrap().to_bits(),
            0xc01484b4654e4e7d
        );
        assert_eq!(
            gamma_kernel(12.0 / 13.0 - 1.0).unwrap().to_bits(),
            0xc02b517c3426d534
        );
        assert_eq!(
            gamma_kernel(0.2 - 1.0).unwrap().to_bits(),
            0xc016f447aae4691a
        );
        assert_eq!(
            gamma_kernel(1.0 / 9.0 - 1.0).unwrap().to_bits(),
            0xc0232d118093e5ec
        );
        assert_eq!(
            gamma_kernel(0.1 - 1.0).unwrap().to_bits(),
            0xc0252420fa9e47ee
        );
        assert_eq!(
            gamma_kernel(0.1875 - 1.0).unwrap().to_bits(),
            0xc018328e296d19c4
        );
        assert_eq!(
            gamma_kernel(0.2 - 2.0).unwrap().to_bits(),
            0x400981332fa874c7
        );
        assert_eq!(
            gamma_kernel(0.4 - 2.0).unwrap().to_bits(),
            0x40027c12dd913f0b
        );
        assert_eq!(
            gamma_kernel(0.8 - 2.0).unwrap().to_bits(),
            0x401367614f035c42
        );
        assert_eq!(
            gamma_kernel(1.0 / 9.0 - 2.0).unwrap().to_bits(),
            0x40144dd64be7e463
        );
        assert_eq!(
            gamma_kernel(0.4 - 3.0).unwrap().to_bits(),
            0xbfec701d061a885f
        );
        assert_eq!(
            gamma_kernel(0.6 - 3.0).unwrap().to_bits(),
            0xbff1ba7d9c17f309
        );
        assert_eq!(
            gamma_kernel(0.6 - 4.0).unwrap().to_bits(),
            0x3fd4db6699859665
        );
        assert_eq!(
            gamma_kernel(4.0 / 17.0 - 2.0).unwrap().to_bits(),
            0x4006ea8b8975c9d5
        );
        assert_eq!(
            gamma_kernel(8.0 / 17.0 - 2.0).unwrap().to_bits(),
            0x40029837d82ca111
        );
        assert_eq!(
            gamma_kernel(9.0 / 17.0 - 2.0).unwrap().to_bits(),
            0x4003613ad22c67dd
        );
        assert_eq!(
            gamma_kernel(4.0 / 11.0 - 2.0).unwrap().to_bits(),
            0x4002cccc21903d00
        );
        assert_eq!(
            gamma_kernel(12.0 / 13.0 - 2.0).unwrap().to_bits(),
            0x40295df354ff7cd5
        );
        assert_eq!(
            gamma_kernel(4.0 / 11.0 - 3.0).unwrap().to_bits(),
            0xbfec862ce379bda8
        );
        assert_eq!(
            gamma_kernel(4.0 / 5.0 - 3.0).unwrap().to_bits(),
            0xc001a3ccd3776b1f
        );
        assert_eq!(
            gamma_kernel(7.0 / 10.0 - 3.0).unwrap().to_bits(),
            0xbff7275a1542102b
        );
        assert_eq!(
            gamma_kernel(13.0 / 17.0 - 3.0).unwrap().to_bits(),
            0xbffdb533a6598980
        );
        assert_eq!(
            gamma_kernel(4.0 / 7.0 - 2.0).unwrap().to_bits(),
            0x40045d8eb5ba90af
        );
        assert_eq!(
            gamma_kernel(5.0 / 7.0 - 2.0).unwrap().to_bits(),
            0x400bc9cd0a970753
        );
        assert_eq!(
            gamma_kernel(4.0 / 7.0 - 3.0).unwrap().to_bits(),
            0xbff0c58495a8b362
        );
        assert_eq!(
            gamma_kernel(13.0 / 17.0 - 4.0).unwrap().to_bits(),
            0x3fe25d65bf455058
        );
        assert_eq!(
            gamma_kernel(3.0 / 7.0 - 5.0).unwrap().to_bits(),
            0xbfac146961c62e4a
        );
        assert_eq!(
            gamma_kernel(4.0 / 11.0 - 5.0).unwrap().to_bits(),
            0xbfab11e8576075be
        );
        assert_eq!(
            gamma_kernel(3.0 / 7.0 - 6.0).unwrap().to_bits(),
            0x3f8428e9328127cc
        );
        assert_eq!(
            gamma_kernel(1.0 / 7.0 - 2.0).unwrap().to_bits(),
            0x4010744059232347
        );
        assert_eq!(
            gamma_kernel(2.0 / 7.0 - 2.0).unwrap().to_bits(),
            0x4004930000532bf6
        );
        assert_eq!(
            gamma_kernel(3.0 / 10.0 - 2.0).unwrap().to_bits(),
            0x40041c83ef183e01
        );
        assert_eq!(
            gamma_kernel(1.0 / 16.0 - 2.0).unwrap().to_bits(),
            0x40210bca4ee7e960
        );
        assert_eq!(
            gamma_kernel(1.0 / 8.0 - 2.0).unwrap().to_bits(),
            0x40125e53dcbb44f5
        );
        assert_eq!(
            gamma_kernel(5.0 / 8.0 - 2.0).unwrap().to_bits(),
            0x400641bc27e1a91f
        );
        assert_eq!(
            gamma_kernel(7.0 / 8.0 - 2.0).unwrap().to_bits(),
            0x401efe9b37e78e60
        );
        assert_eq!(
            gamma_kernel(1.0 / 4.0 - 2.0).unwrap().to_bits(),
            0x4006195527ff2ff0
        );
        assert_eq!(
            gamma_kernel(4.0 / 7.0 - 4.0).unwrap().to_bits(),
            0x3fd3911aae9a269d
        );
        assert_eq!(
            gamma_kernel(13.0 / 17.0 - 5.0).unwrap().to_bits(),
            0xbfc158356d8892fe
        );
        assert_eq!(
            gamma_kernel(3.0 / 5.0 - 5.0).unwrap().to_bits(),
            0xbfb2f6002e79716d
        );
        assert_eq!(
            gamma_kernel(3.0 / 7.0 - 7.0).unwrap().to_bits(),
            0xbf588ad91c17aae2
        );
        assert_eq!(
            gamma_kernel(9.0 / 16.0 - 2.0).unwrap().to_bits(),
            0x40041f85b0607201
        );
        assert_eq!(
            gamma_kernel(7.0 / 16.0 - 2.0).unwrap().to_bits(),
            0x40026e268c11c261
        );
        assert_eq!(
            gamma_kernel(3.0 / 10.0 - 3.0).unwrap().to_bits(),
            0xbfedcb6e1fd81004
        );
        assert_eq!(
            gamma_kernel(5.0 / 8.0 - 3.0).unwrap().to_bits(),
            0xbff2be17b5cb80f2
        );
        assert_eq!(
            gamma_kernel(3.0 / 10.0 - 4.0).unwrap().to_bits(),
            0x3fd01aef6b28aeb6
        );
        assert_eq!(
            gamma_kernel(3.0 / 5.0 - 6.0).unwrap().to_bits(),
            0x3f8c171cb6a0f3e3
        );
        assert_eq!(
            gamma_kernel(9.0 / 16.0 - 3.0).unwrap().to_bits(),
            0xbff082e3d8ecac50
        );
        assert_eq!(
            gamma_kernel(9.0 / 16.0 - 4.0).unwrap().to_bits(),
            0x3fd336901d00c882
        );
        assert_eq!(
            gamma_kernel(5.0 / 8.0 - 5.0).unwrap().to_bits(),
            0xbfb44f33f8314ec5
        );
        assert_eq!(
            gamma_kernel(3.0 / 5.0 - 7.0).unwrap().to_bits(),
            0xbf618e71f224986e
        );
        assert_eq!(
            gamma_kernel(1.0 / 4.0 - 4.0).unwrap().to_bits(),
            0x3fd124b7f9ca9e35
        );
        assert_eq!(
            gamma_kernel(1.0 / 16.0 - 5.0).unwrap().to_bits(),
            0xbfc31a54d7fe74d3
        );
        assert_eq!(
            gamma_kernel(5.0 / 8.0 - 6.0).unwrap().to_bits(),
            0x3f8e3a5941c66955
        );
        assert_eq!(
            gamma_kernel(1.0 / 16.0 - 6.0).unwrap().to_bits(),
            0x3f99bd13fffdeb8d
        );
        assert_eq!(
            gamma_kernel(3.0 / 5.0 - 8.0).unwrap().to_bits(),
            0x3f32fadc0cb1f0e5
        );
        assert_eq!(
            gamma_kernel(1.0 / 4.0 - 1.0).unwrap().to_bits(),
            0xc013562a82ff49f2
        );
        assert_eq!(
            gamma_kernel(6.0 / 7.0 - 1.0).unwrap().to_bits(),
            0xc01ef6236c822d27
        );
        assert_eq!(
            gamma_kernel(1.0 / 7.0 - 1.0).unwrap().to_bits(),
            0xc01e8ec0a58a6616
        );
        assert_eq!(
            gamma_kernel(1.0 / 17.0 - 1.0).unwrap().to_bits(),
            0xc0318204fc0456a0
        );
        assert_eq!(
            gamma_kernel(6.0 / 17.0 - 1.0).unwrap().to_bits(),
            0xc00f34fd6869612f
        );
        assert_eq!(
            gamma_kernel(14.0 / 17.0 - 1.0).unwrap().to_bits(),
            0xc019d01d2ce313ac
        );
        assert_eq!(
            gamma_kernel(1.0 / 17.0 - 2.0).unwrap().to_bits(),
            0x402209d6970c3a38
        );
        assert_eq!(
            gamma_kernel(1.0 / 3.0 - 1.0).unwrap().to_bits(),
            0xc01012d97eaf6235
        );
        assert_eq!(
            gamma_kernel(2.0 / 3.0 - 1.0).unwrap().to_bits(),
            0xc0103fd9ade928db
        );
        assert_eq!(
            gamma_kernel(5.0 + 7.0 / 13.0).unwrap().to_bits(),
            0x404bd9370fcb65c6
        );
        assert_eq!(
            gamma_kernel(6.0 + 7.0 / 13.0).unwrap().to_bits(),
            0x4073479c4602f7b1
        );
        assert_eq!(
            gamma_kernel(4.0 + 9.0 / 17.0).unwrap().to_bits(),
            0x40283c6c383a1e14
        );
        assert_eq!(
            gamma_kernel(4.0 + 12.0 / 17.0).unwrap().to_bits(),
            0x402f2001077bf3ab
        );
        assert_eq!(
            gamma_kernel(2.0 / 13.0 - 2.0).unwrap().to_bits(),
            0x400f0457b7c6bb2d
        );
        assert_eq!(
            gamma_kernel(11.0 / 13.0 - 2.0).unwrap().to_bits(),
            0x401926a4f4f886c3
        );
        assert_eq!(
            gamma_kernel(2.0 / 11.0 - 2.0).unwrap().to_bits(),
            0x400b4e662d355592
        );
        assert_eq!(
            gamma_kernel(10.0 / 11.0 - 2.0).unwrap().to_bits(),
            0x402567ac77720fc5
        );
        assert_eq!(
            gamma_kernel(13.0 / 17.0 - 2.0).unwrap().to_bits(),
            0x401099f737502731
        );
        assert_eq!(
            gamma_kernel(2.0 / 13.0 - 3.0).unwrap().to_bits(),
            0xbff5cbb342dead0b
        );
        assert_eq!(
            gamma_kernel(11.0 / 13.0 - 3.0).unwrap().to_bits(),
            0xc0075abdbee6c648
        );
        assert_eq!(
            gamma_kernel(2.0 / 11.0 - 3.0).unwrap().to_bits(),
            0xbff360edac786e4b
        );
        assert_eq!(
            gamma_kernel(10.0 / 11.0 - 3.0).unwrap().to_bits(),
            0xc014796d50dc6821
        );
        assert_eq!(
            gamma_kernel(2.0 / 13.0 - 4.0).unwrap().to_bits(),
            0x3fd6aae36443be34
        );
        assert_eq!(
            gamma_kernel(10.0 / 11.0 - 4.0).unwrap().to_bits(),
            0x3ffa7f05f02c4a85
        );
        assert_eq!(
            gamma_kernel(2.0 / 11.0 - 5.0).unwrap().to_bits(),
            0xbfb0daa03f849286
        );
        assert_eq!(
            gamma_kernel(10.0 / 11.0 - 5.0).unwrap().to_bits(),
            0xbfd9e84a12a87660
        );
        assert_eq!(
            gamma_kernel(6.0 / 11.0 - 6.0).unwrap().to_bits(),
            0x3f887deb47c3ce38
        );
        assert_eq!(
            gamma_kernel(10.0 / 11.0 - 6.0).unwrap().to_bits(),
            0x3fb45b15a0f213de
        );
        assert_eq!(
            gamma_kernel(10.0 / 11.0 - 7.0).unwrap().to_bits(),
            0xbf8abc68d3642961
        );
        assert_eq!(
            gamma_kernel(10.0 / 11.0 - 11.0).unwrap().to_bits(),
            0xbec4ceb5db83f371
        );
        assert_eq!(
            gamma_kernel(1.0 + 2.0 / 11.0).unwrap().to_bits(),
            0x3fed8addb6f81d80
        );
        assert_eq!(
            gamma_kernel(2.0 + 4.0 / 11.0).unwrap().to_bits(),
            0x3ff36a412884def4
        );
        assert_eq!(
            gamma_kernel(3.0 + 7.0 / 11.0).unwrap().to_bits(),
            0x400eff16b2482ffa
        );
        assert_eq!(
            gamma_kernel(3.0 + 9.0 / 11.0).unwrap().to_bits(),
            0x401330e6942a4ed8
        );
        assert_eq!(
            gamma_kernel(3.0 + 10.0 / 11.0).unwrap().to_bits(),
            0x40156f771dde0918
        );
        assert_eq!(
            gamma_kernel(2.0 / 11.0).unwrap().to_bits(),
            0x40144f786dca9448
        );
        assert_eq!(
            gamma_kernel(5.0 / 11.0).unwrap().to_bits(),
            0x3fff2c8ab61daf0f
        );
        assert_eq!(
            gamma_kernel(10.0 / 11.0).unwrap().to_bits(),
            0x3ff0fb827c62f539
        );
        assert_eq!(
            gamma_kernel(1.0 / 13.0).unwrap().to_bits(),
            0x4028fce1e0ed23fb
        );
        assert_eq!(
            gamma_kernel(1.0 / 13.0 - 1.0).unwrap().to_bits(),
            0xc02b11f4b3ab91aa
        );
        assert_eq!(
            gamma_kernel(2.0 / 13.0 - 1.0).unwrap().to_bits(),
            0xc01ca18c0c19e7d7
        );
        assert_eq!(
            gamma_kernel(11.0 / 13.0 - 1.0).unwrap().to_bits(),
            0xc01d05347d1ec2db
        );
        assert_eq!(
            gamma_kernel(1.0 + 1.0 / 13.0).unwrap().to_bits(),
            0x3feec1160123dd84
        );
        assert_eq!(
            gamma_kernel(2.0 + 1.0 / 13.0).unwrap().to_bits(),
            0x3ff08f5a9e270120
        );
        assert_eq!(
            gamma_kernel(3.0 + 2.0 / 13.0).unwrap().to_bits(),
            0x4002867b673e729a
        );
        assert_eq!(
            gamma_kernel(4.0 + 7.0 / 13.0).unwrap().to_bits(),
            0x40288b6041fd0743
        );
        assert_eq!(
            gamma_kernel(4.0 + 9.0 / 13.0).unwrap().to_bits(),
            0x402e860ed048074e
        );
        assert_eq!(
            gamma_kernel(6.0 / 13.0).unwrap().to_bits(),
            0x3ffeb36ee50fd917
        );
        assert_eq!(
            gamma_kernel(12.0 / 13.0).unwrap().to_bits(),
            0x3ff0cfaee504346b
        );
        assert_eq!(
            gamma_kernel(1.0 / 17.0).unwrap().to_bits(),
            0x40307a5f0b4f6098
        );
        assert_eq!(
            gamma_kernel(1.0 + 3.0 / 17.0).unwrap().to_bits(),
            0x3fed97a61860c048
        );
        assert_eq!(
            gamma_kernel(3.0 + 7.0 / 17.0).unwrap().to_bits(),
            0x400826f7fa7c0065
        );
        assert_eq!(
            gamma_kernel(3.0 + 15.0 / 17.0).unwrap().to_bits(),
            0x4014be7ce451b13d
        );
        assert_eq!(
            gamma_kernel(8.0 / 17.0).unwrap().to_bits(),
            0x3ffe1c96ab222e75
        );
        assert_eq!(
            gamma_kernel(16.0 / 17.0).unwrap().to_bits(),
            0x3ff099e6910e9682
        );
        assert_eq!(gamma_kernel(-0.5).unwrap().to_bits(), 0xc00c5bf891b4ef6a);
        assert_eq!(gamma_kernel(-1.5).unwrap().to_bits(), 0x4002e7fb0bcdf4f1);
        assert_eq!(
            gamma_kernel(1.0 / 16.0).unwrap().to_bits(),
            0x402ef66a79533ee8
        );
        assert_eq!(
            gamma_kernel(3.0 / 16.0).unwrap().to_bits(),
            0x4013a91381a8a4ee
        );
        assert_eq!(
            gamma_kernel(5.0 / 16.0).unwrap().to_bits(),
            0x4006edc1821c5c71
        );
        assert_eq!(
            gamma_kernel(7.0 / 16.0).unwrap().to_bits(),
            0x400032cfe11b9bd7
        );
        assert_eq!(
            gamma_kernel(9.0 / 16.0).unwrap().to_bits(),
            0x3ff94fa627d94f66
        );
        assert_eq!(
            gamma_kernel(11.0 / 16.0).unwrap().to_bits(),
            0x3ff517bf09b399f5
        );
        assert_eq!(
            gamma_kernel(13.0 / 16.0).unwrap().to_bits(),
            0x3ff26858f1d7c28d
        );
        assert_eq!(
            gamma_kernel(15.0 / 16.0).unwrap().to_bits(),
            0x3ff0a490a6519230
        );
        assert_eq!(gamma_kernel(1.0625).unwrap().to_bits(), 0x3feef66a79533ee8);
        assert_eq!(gamma_kernel(9.0625).unwrap().to_bits(), 0x40e682cf40f15007);
        assert_eq!(gamma_kernel(4.3125).unwrap().to_bits(), 0x4022027d3db6cf53);
        assert_eq!(gamma_kernel(10.5625).unwrap().to_bits(), 0x4133f93261bf1bc0);
        assert_eq!(gamma_kernel(5.8125).unwrap().to_bits(), 0x4055db68b6f3576c);
        // Live Excel 16.0 b20326 fifths in (0,1). Recurrence n=1 already 1 ULP.
        assert_eq!(
            gamma_kernel(1.0 / 5.0).unwrap().to_bits(),
            0x40125d0622505413
        );
        assert_eq!(
            gamma_kernel(1.0 + 1.0 / 5.0).unwrap().to_bits(),
            0x3fed61a36a1a201f
        );
        assert_eq!(
            gamma_kernel(2.0 / 5.0).unwrap().to_bits(),
            0x4001beca6e4dff14
        );
        assert_eq!(
            gamma_kernel(3.0 / 5.0).unwrap().to_bits(),
            0x3ff7d3bb4061b952
        );
        assert_eq!(
            gamma_kernel(4.0 / 5.0).unwrap().to_bits(),
            0x3ff2a0af5617b4b9
        );
        assert_eq!(
            gamma_kernel(1.0 / 10.0).unwrap().to_bits(),
            0x402306ea7b280d88
        );
        assert_eq!(
            gamma_kernel(3.0 / 10.0).unwrap().to_bits(),
            0x4007eebbb8aec4ab
        );
        assert_eq!(
            gamma_kernel(1.0 + 3.0 / 10.0).unwrap().to_bits(),
            0x3fecb81477381f33
        );
        assert_eq!(
            gamma_kernel(7.0 / 10.0).unwrap().to_bits(),
            0x3ff4c4d5ab21ea23
        );
        assert_eq!(
            gamma_kernel(9.0 / 10.0).unwrap().to_bits(),
            0x3ff1191a68f2b5e1
        );
        assert_eq!(
            gamma_kernel(1.0 / 7.0).unwrap().to_bits(),
            0x401a313769520e5a
        );
        assert_eq!(
            gamma_kernel(1.0 + 1.0 / 7.0).unwrap().to_bits(),
            0x3fedef1ac182598b
        );
        assert_eq!(
            gamma_kernel(2.0 + 1.0 / 7.0).unwrap().to_bits(),
            0x3ff11aeab7b8332b
        );
        assert_eq!(
            gamma_kernel(2.0 / 7.0).unwrap().to_bits(),
            0x400931634450f1e8
        );
        assert_eq!(
            gamma_kernel(3.0 / 7.0).unwrap().to_bits(),
            0x40008a43968d61a6
        );
        assert_eq!(
            gamma_kernel(4.0 / 7.0).unwrap().to_bits(),
            0x3ff8eff2aa47b664
        );
        assert_eq!(
            gamma_kernel(5.0 / 7.0).unwrap().to_bits(),
            0x3ff46a774bb2e0cd
        );
        assert_eq!(
            gamma_kernel(6.0 / 7.0).unwrap().to_bits(),
            0x3ff1b138d04a62f3
        );
        assert_eq!(
            gamma_kernel(1.0 / 12.0).unwrap().to_bits(),
            0x4026ffb50d1bc6dc
        );
        assert_eq!(
            gamma_kernel(5.0 / 12.0).unwrap().to_bits(),
            0x4001053ca2989062
        );
        assert_eq!(
            gamma_kernel(7.0 / 12.0).unwrap().to_bits(),
            0x3ff87597c6695642
        );
        assert_eq!(
            gamma_kernel(11.0 / 12.0).unwrap().to_bits(),
            0x3ff0e384cb7476b6
        );
        assert_eq!(
            gamma_kernel(1.0 + 1.0 / 12.0).unwrap().to_bits(),
            0x3feeaa46bc250925
        );
        assert_eq!(
            gamma_kernel(1.0 + 5.0 / 12.0).unwrap().to_bits(),
            0x3fec5e0fb9a8f0a4
        );
        assert_eq!(
            gamma_kernel(3.0 + 7.0 / 12.0).unwrap().to_bits(),
            0x400d2e10d85584c4
        );
        assert_eq!(
            gamma_kernel(1.0 / 3.0).unwrap().to_bits(),
            0x40056e77539482f2
        );
        assert_eq!(
            gamma_kernel(2.0 / 3.0).unwrap().to_bits(),
            0x3ff5aa77928c3679
        );
        assert_eq!(
            gamma_kernel(1.0 / 9.0).unwrap().to_bits(),
            0x40210b9dc79fe8d4
        );
        assert_eq!(
            gamma_kernel(2.0 / 9.0).unwrap().to_bits(),
            0x40106d2331a5de8d
        );
        assert_eq!(
            gamma_kernel(4.0 / 9.0).unwrap().to_bits(),
            0x3fffe2e4518a6b60
        );
        assert_eq!(
            gamma_kernel(5.0 / 9.0).unwrap().to_bits(),
            0x3ff99c88812c4a39
        );
        assert_eq!(
            gamma_kernel(3.0 + 5.0 / 9.0).unwrap().to_bits(),
            0x400c48114add546c
        );
        assert_eq!(
            gamma_kernel(1.0 + 7.0 / 9.0).unwrap().to_bits(),
            0x3fed9f1d49c5b48d
        );
        assert_eq!(
            gamma_kernel(7.0 / 9.0).unwrap().to_bits(),
            0x3ff30adbf89161c8
        );
        assert_eq!(
            gamma_kernel(8.0 / 9.0).unwrap().to_bits(),
            0x3ff13e800bd48928
        );
        assert_eq!(
            gamma_kernel(-1.0 / 3.0).unwrap().to_bits(),
            0xc0103fd9ade928da
        );
        assert_eq!(
            gamma_kernel(-2.0 / 3.0).unwrap().to_bits(),
            0xc01012d97eaf6234
        );
        assert_eq!(gamma_kernel(-0.25).unwrap().to_bits(), 0xc0139b4e8b50f62d);
        assert_eq!(gamma_kernel(-0.2).unwrap().to_bits(), 0xc01748db2b9da1e7);
        assert_eq!(gamma_kernel(-0.125).unwrap().to_bits(), 0xc0216f374f724016);
        assert_eq!(gamma_kernel(-0.375).unwrap().to_bits(), 0xc00e9a62b6d6488b);
        assert_eq!(gamma_kernel(-0.625).unwrap().to_bits(), 0xc00e5771fe7759f3);
        assert_eq!(gamma_kernel(-0.875).unwrap().to_bits(), 0xc021386e9eef90a6);
        assert_eq!(
            gamma_kernel(-5.0 / 7.0).unwrap().to_bits(),
            0xc011a292496bdc89
        );
        assert_eq!(
            gamma_kernel(-6.0 / 7.0).unwrap().to_bits(),
            0xc01e8ec0a58a6611
        );
        assert_eq!(
            gamma_kernel(-2.0 / 9.0).unwrap().to_bits(),
            0xc0156c3777a38e01
        );
        assert_eq!(
            gamma_kernel(-7.0 / 9.0).unwrap().to_bits(),
            0xc0151e9af6b0b06c
        );
        assert_eq!(
            gamma_kernel(-7.0 / 10.0).unwrap().to_bits(),
            0xc011183cf1a167e7
        );
        assert_eq!(
            gamma_kernel(-5.0 / 12.0).unwrap().to_bits(),
            0xc00d59e9547e6786
        );
        assert_eq!(
            gamma_kernel(-7.0 / 12.0).unwrap().to_bits(),
            0xc00d2d8c847340a9
        );
        assert_eq!(
            gamma_kernel(-1.0 / 16.0).unwrap().to_bits(),
            0xc030a490a6519230
        );
        assert_eq!(
            gamma_kernel(-5.0 / 16.0).unwrap().to_bits(),
            0xc010dfcc07c2e191
        );
        assert_eq!(
            gamma_kernel(-7.0 / 16.0).unwrap().to_bits(),
            0xc00ced502d8aa3e2
        );
        assert_eq!(
            gamma_kernel(-9.0 / 16.0).unwrap().to_bits(),
            0xc00ccc1c3adbbfb7
        );
        assert_eq!(
            gamma_kernel(-15.0 / 16.0).unwrap().to_bits(),
            0xc030836bfc70aa15
        );
    }

    #[test]
    fn gamma_tiny_positive_is_reciprocal_and_subnormals_are_num() {
        assert_eq!(gamma_kernel(1e-16).unwrap().to_bits(), 0x4341c37937e08000);
        assert_eq!(
            gamma_kernel(f64::MIN_POSITIVE).unwrap().to_bits(),
            0x7fd0000000000000
        );
        assert_eq!(
            gamma_kernel(1e-20).unwrap().to_bits(),
            (1.0_f64 / 1e-20).to_bits()
        );
        assert_eq!(
            gamma_kernel(f64::from_bits(1)),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            gamma_kernel(f64::from_bits(0x000f_ffff_ffff_ffff)),
            Err(WorksheetErrorCode::Num)
        );
    }

    // BUG-FUNC-027 CLASS-A1: GAMMALN(1E-300) was +Inf (z+1 == 0 in Lanczos);
    // live Excel 16.0 b20026 = 690.7755278982137 via the recurrence.
    #[test]
    fn gammaln_tiny_positive_uses_recurrence_not_inf() {
        let v = gammaln_kernel(1e-300).unwrap();
        assert!(v.is_finite(), "GAMMALN(1e-300) was non-finite: {v}");
        assert_close(v, 690.7755278982137, 1e-9);
        assert!(gammaln_precise_kernel(1e-300).unwrap().is_finite());
    }

    #[test]
    fn gammaln_rejects_positive_subnormals_and_admits_min_normal() {
        // W109 G3-02 current-reference discovery plus separately frozen
        // answer-blind heldout: 40/40 positive-subnormal rows publish #NUM!
        // across GAMMALN and GAMMALN.PRECISE. The adjacent min-normal endpoint
        // is admitted and its exact published value is pinned here.
        for x in [f64::from_bits(1), f64::from_bits(0x000f_ffff_ffff_ffff)] {
            assert_eq!(gammaln_kernel(x), Err(WorksheetErrorCode::Num));
            assert_eq!(gammaln_precise_kernel(x), Err(WorksheetErrorCode::Num));
        }

        let expected = f64::from_bits(0x4086_232b_dd7a_bcd2);
        assert_bits_eq(
            "gammaln(min-normal)",
            gammaln_kernel(f64::MIN_POSITIVE).unwrap(),
            expected,
        );
        assert_bits_eq(
            "gammaln.precise(min-normal)",
            gammaln_precise_kernel(f64::MIN_POSITIVE).unwrap(),
            expected,
        );
    }

    // BUG-FUNC-027 CLASS-A2: GAMMA(-1E-200) rounds to 0 but is not the pole at 0;
    // live Excel 16.0 b20026 ~ -1E200 (finite). Fine ULP exactness is CLASS-C1.
    #[test]
    fn gamma_tiny_negative_is_not_a_false_pole() {
        let v = gamma_kernel(-1e-200).unwrap();
        assert!(v.is_finite() && v < 0.0, "GAMMA(-1e-200) = {v}");
        assert!(
            (v.abs().log10() - 200.0).abs() < 1.0,
            "magnitude ~1e200: {v}"
        );
        // Exact non-positive-integer poles still error.
        assert_eq!(gamma_kernel(0.0), Err(WorksheetErrorCode::Num));
        assert_eq!(gamma_kernel(-2.0), Err(WorksheetErrorCode::Num));
        // A non-integer negative is finite, not a pole.
        assert!(gamma_kernel(-1.5).unwrap().is_finite());
    }

    #[test]
    fn weibull_family_matches_seed_rows() {
        assert_close(
            weibull_kernel(2.0, 3.0, 4.0, true).unwrap(),
            0.11750309741540463,
            1e-12,
        );
        assert_close(
            weibull_dist_kernel(2.0, 3.0, 4.0, false).unwrap(),
            0.1654681692346117,
            1e-12,
        );
        assert_eq!(weibull_dist_kernel(0.0, 3.0, 4.0, true), Ok(0.0));
        assert_eq!(weibull_dist_kernel(0.0, 3.0, 4.0, false), Ok(0.0));
        assert_eq!(weibull_dist_kernel(0.0, 0.5, 4.0, false), Ok(0.0));
        assert_eq!(weibull_dist_kernel(0.0, 1.0, 4.0, false), Ok(0.0));
        assert_eq!(
            weibull_dist_kernel(-1.0, 3.0, 4.0, true),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            weibull_dist_kernel(2.0, 0.0, 4.0, true),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            weibull_dist_kernel(2.0, 3.0, 0.0, true),
            Err(WorksheetErrorCode::Num)
        );
    }
}
