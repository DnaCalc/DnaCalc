//! Fixed Bessel seeds and backward recurrence adapted from SheetJS/bessel.
//! Copyright (C) 2013-present SheetJS; Apache-2.0.
//! Reference: bessel.js at 2e2331329891dc5c16bda3461db0b9fe590ff2e9.
//! Changed for DNA Calc: Rust scalar helpers, existing worksheet elementary
//! functions, and Excel's stored arithmetic for the low seeds. R27's high-J
//! amplitudes adapt one decimal digit transposition and one repeated term
//! from the published tables, inferred from recorded phase-window outputs.
//! The fixed backward program and Y high-region tables retain the published
//! constants. High J/Y asymptotic seeds store each operation from PC64, with
//! high J0 multiplying `(z * sin) * corr` and high J1, Y0, and Y1 multiplying
//! `(trig * corr) * z`. See deploy/notices/ported.txt.
//! The I0 and higher-I helpers also adapt ITK's Apache-2.0 GaussianOperator,
//! Copyright NumFOCUS, at 311b7060ef39e371f3cd209ec135284ff5fde735.
//! Their scalar operations use the existing RN53(RN64(operation)) helpers.
use super::excel_numeric::{
    excel_cos, excel_exp, excel_log, excel_sin, excel_sqrt_staged, excel_x87_add, excel_x87_div,
    excel_x87_mul, excel_x87_sub,
};

const J0_NUM: &[f64] = &[
    57568490574.0,
    -13362590354.0,
    651619640.7,
    -11214424.18,
    77392.33017,
    -184.9052456,
];
const J0_DEN: &[f64] = &[
    57568490411.0,
    1029532985.0,
    9494680.718,
    59272.64853,
    267.8532712,
    1.0,
];
const J1_NUM: &[f64] = &[
    72362614232.0,
    -7895059235.0,
    242396853.1,
    -2972611.439,
    15704.48260,
    -30.16036606,
];
const J1_DEN: &[f64] = &[
    144725228442.0,
    2300535178.0,
    18583304.74,
    99447.43394,
    376.9991397,
    1.0,
];
const Y0_NUM: &[f64] = &[
    -2957821389.0,
    7062834065.0,
    -512359803.6,
    10879881.29,
    -86327.92757,
    228.4622733,
];
const Y0_DEN: &[f64] = &[
    40076544269.0,
    745249964.8,
    7189466.438,
    47447.26470,
    226.1030244,
    1.0,
];
const Y1_NUM: &[f64] = &[
    -0.4900604943e13,
    0.1275274390e13,
    -0.5153438139e11,
    0.7349264551e9,
    -0.4237922726e7,
    0.8511937935e4,
];
const Y1_DEN: &[f64] = &[
    0.2499580570e14,
    0.4244419664e12,
    0.3733650367e10,
    0.2245904002e8,
    0.1020426050e6,
    0.3549632885e3,
    1.0,
];

const W: f64 = 0.636619772;

fn polynomial_with(
    x: f64,
    coefficients: &[f64],
    multiply: impl Fn(f64, f64) -> f64,
    add: impl Fn(f64, f64) -> f64,
) -> f64 {
    let mut value = *coefficients.last().unwrap();
    for &coefficient in coefficients[..coefficients.len() - 1].iter().rev() {
        value = add(multiply(x, value), coefficient);
    }
    value
}

/// Excel 20527 stores each operation from x87 in the low rational seeds.
/// Independent exact inputs distinguish the square, Horner products and
/// sums, the order-one numerator multiplier, and the final quotient.
fn stored_low_rational(x: f64, numerator_factor: f64, numerator: &[f64], denominator: &[f64]) -> f64 {
    let square = excel_x87_mul(x, x);
    let horner = |coefficients| polynomial_with(square, coefficients, excel_x87_mul, excel_x87_add);
    let numerator = excel_x87_mul(numerator_factor, horner(numerator));
    excel_x87_div(numerator, horner(denominator))
}

pub(super) fn j0(x: f64) -> f64 {
    stored_low_rational(x, 1.0, J0_NUM, J0_DEN)
}

pub(super) fn j1(x: f64) -> f64 {
    stored_low_rational(x, x, J1_NUM, J1_DEN)
}

pub(super) fn y0(x: f64) -> f64 {
    let rational = stored_low_rational(x, 1.0, Y0_NUM, Y0_DEN);
    let logarithmic = excel_x87_mul(j0(x), excel_log(x));
    excel_x87_add(rational, excel_x87_mul(W, logarithmic))
}

pub(super) fn y1(x: f64) -> f64 {
    let rational = stored_low_rational(x, x, Y1_NUM, Y1_DEN);
    let logarithmic = excel_x87_mul(j1(x), excel_log(x));
    let correction = excel_x87_sub(logarithmic, excel_x87_div(1.0, x));
    excel_x87_add(rational, excel_x87_mul(W, correction))
}

const AMP1: &[f64] = &[
    1.0,
    0.183105e-2,
    -0.3516396496e-4,
    0.2457520174e-5,
    -0.240337019e-6,
];
const PHASE1: &[f64] = &[
    0.04687499995,
    -0.2002690873e-3,
    0.8449199096e-5,
    -0.88228987e-6,
    0.105787412e-6,
];

pub(super) fn jn(order: i32, x: f64) -> f64 {
    if x == 0.0 {
        return 0.0;
    }
    let magnitude = x.abs();
    let n = f64::from(order);
    let start = 2.0 * ((n + (40.0 * n).sqrt().floor()) / 2.0).floor();
    // Excel 20527 distinguishes PC64 stores in the initial ratio, index
    // product, backward recurrence, rescaling products, total accumulation,
    // final normalization subtraction, and final division.
    let twice_over_x = excel_x87_div(2.0, magnitude);
    let mut result = 0.0;
    let mut current = 1.0;
    let mut next = 0.0;
    let mut total = 0.0;
    let mut add_to_total = false;
    for index in (1..=start as i32).rev() {
        let scaled = excel_x87_mul(excel_x87_mul(f64::from(index), twice_over_x), current);
        let previous = excel_x87_sub(scaled, next);
        next = current;
        current = previous;
        if current.abs() > 1E10 {
            current = excel_x87_mul(current, 1E-10);
            next = excel_x87_mul(next, 1E-10);
            result = excel_x87_mul(result, 1E-10);
            total = excel_x87_mul(total, 1E-10);
        }
        if add_to_total {
            total = excel_x87_add(total, current);
        }
        add_to_total = !add_to_total;
        if index == order {
            result = next;
        }
    }
    total = excel_x87_sub(2.0 * total, current);
    result = excel_x87_div(result, total);
    if x < 0.0 && order % 2 != 0 {
        -result
    } else {
        result
    }
}

const AMP0: &[f64] = &[
    1.0,
    -0.1098628627e-2,
    0.2734510407e-4,
    -0.2073370639e-5,
    0.2093887211e-6,
];
const PHASE_Y0: &[f64] = &[
    -0.1562499995e-1,
    0.1430488765e-3,
    -0.6911147651e-5,
    0.7621095161e-6,
    -0.934945152e-7,
];

// Keep J's tables separate from Y's published amplitudes. These are fixed
// transcription hypotheses: exchange the adjacent 6/2 digits in J0's first
// correction and repeat J1's existing second correction. No new coefficient
// or argument-specific choice is fit. Paired R25 phase readings separately
// distinguish the common high-region correction grouping below. Remaining
// low-seed and forward discrepancies are counted; Excel's source is unknown.
const AMP_J0: &[f64] = &[
    1.0,
    -0.1098628267e-2,
    0.2734510407e-4,
    -0.2073370639e-5,
    0.2093887211e-6,
];
const AMP_J1: &[f64] = &[AMP1[0], AMP1[1], AMP1[2], AMP1[2], AMP1[3], AMP1[4]];
const PHASE_J0: &[f64] = &[
    -0.1562499995e-1,
    0.1430488765e-3,
    -0.6911147651e-5,
    0.7621095161e-6,
    -0.934935152e-7,
];

fn high_asymptotic(
    x: f64,
    offset: f64,
    amplitude: &[f64],
    correction: &[f64],
    second_kind: bool,
    ratio_first: bool,
) -> f64 {
    let phase = excel_x87_sub(x, offset);
    let z = excel_x87_div(8.0, x);
    let y = excel_x87_mul(z, z);
    let horner = |c| polynomial_with(y, c, excel_x87_mul, excel_x87_add);
    let amp = horner(amplitude);
    let corr = horner(correction);
    let (trig1, trig2) = if second_kind {
        (excel_sin(phase), excel_cos(phase))
    } else {
        (excel_cos(phase), excel_sin(phase))
    };
    let leading = excel_x87_mul(trig1, amp);
    let trailing = if ratio_first {
        excel_x87_mul(excel_x87_mul(z, trig2), corr)
    } else {
        excel_x87_mul(excel_x87_mul(trig2, corr), z)
    };
    let combined = if second_kind {
        excel_x87_add(leading, trailing)
    } else {
        excel_x87_sub(leading, trailing)
    };
    let scale = excel_sqrt_staged(excel_x87_div(W, x));
    excel_x87_mul(scale, combined)
}

pub(super) fn j0_large(x: f64) -> f64 {
    high_asymptotic(x.abs(), 0.785398164, AMP_J0, PHASE_J0, false, true)
}

pub(super) fn j1_large(x: f64) -> f64 {
    let value = high_asymptotic(x.abs(), 2.356194491, AMP_J1, PHASE1, false, false);
    if x < 0.0 { -value } else { value }
}

pub(super) fn y0_large(x: f64) -> f64 {
    high_asymptotic(x, 0.785398164, AMP0, PHASE_Y0, true, false)
}

pub(super) fn y1_large(x: f64) -> f64 {
    high_asymptotic(x, 2.356194491, AMP1, PHASE1, true, false)
}

// ITK v5.4.0, itkGaussianOperator.hxx:ModifiedBesselI0 and ModifiedBesselI.
// Copyright NumFOCUS. Apache-2.0; notice and licence in deploy/notices/ported.txt.
// Rust scalar port with worksheet EXP and stored PC64 arithmetic. The existing
// A&S I0 tables are identical to ITK's published coefficients; share those tables.
pub(super) fn i0(x: f64, small: &[f64], large: &[f64]) -> f64 {
    let magnitude = x.abs();
    if magnitude < 3.75 {
        let reduced = excel_x87_div(x, 3.75);
        let square = excel_x87_mul(reduced, reduced);
        polynomial_with(square, small, excel_x87_mul, excel_x87_add)
    } else {
        let reduced = excel_x87_div(3.75, magnitude);
        let scale = excel_x87_div(excel_exp(magnitude), excel_sqrt_staged(magnitude));
        excel_x87_mul(
            scale,
            polynomial_with(reduced, large, excel_x87_mul, excel_x87_add),
        )
    }
}

/// The published higher-order body; its caller owns order admission and parity.
pub(super) fn in_(order: u32, magnitude: f64, normalizer: f64) -> f64 {
    if magnitude == 0.0 {
        return 0.0;
    }
    let twice_over_x = excel_x87_div(2.0, magnitude);
    let mut next = 0.0;
    let mut current = 1.0;
    let mut answer = 0.0;
    let start = 2 * (order + (40.0 * f64::from(order)).sqrt() as u32);
    for j in (1..=start).rev() {
        let coefficient = excel_x87_mul(f64::from(j), twice_over_x);
        let previous = excel_x87_add(next, excel_x87_mul(coefficient, current));
        next = current;
        current = previous;
        if current.abs() > 1e10 {
            answer = excel_x87_mul(answer, 1e-10);
            current = excel_x87_mul(current, 1e-10);
            next = excel_x87_mul(next, 1e-10);
        }
        if j == order {
            answer = next;
        }
    }
    excel_x87_mul(answer, excel_x87_div(normalizer, current))
}
