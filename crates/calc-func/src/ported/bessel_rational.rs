//! Fixed Bessel seeds and backward recurrence adapted from SheetJS/bessel.
//! Copyright (C) 2013-present SheetJS; Apache-2.0.
//! Reference: bessel.js at 2e2331329891dc5c16bda3461db0b9fe590ff2e9.
//! Changed for DNA Calc: Rust scalar helpers, existing worksheet elementary
//! functions, and a fixed grouping of the Y0 logarithm term. R27's high-J
//! amplitudes adapt one decimal digit transposition and one repeated term
//! from the published tables, inferred from recorded phase-window outputs.
//! The fixed backward program and Y high-region tables retain the published
//! constants. High J/Y correction multiplies the stored8/x ratio, trig leaf,
//! then polynomial, following paired R25 phase readings. See deploy/notices/ported.txt.
//! The I0 and higher-I helpers also adapt ITK's Apache-2.0 GaussianOperator,
//! Copyright NumFOCUS, at 311b7060ef39e371f3cd209ec135284ff5fde735.
//! Their scalar operations use the existing RN53(RN64(operation)) helpers.
use super::excel_numeric::{
    excel_cos, excel_exp, excel_log, excel_sin, excel_sqrt_staged, excel_x87_add, excel_x87_div,
    excel_x87_mul,
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

fn polynomial(x: f64, coefficients: &[f64]) -> f64 {
    polynomial_with(x, coefficients, |a, b| a * b, |a, b| a + b)
}

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

pub(super) fn j0(x: f64) -> f64 {
    let square = x * x;
    polynomial(square, J0_NUM) / polynomial(square, J0_DEN)
}

pub(super) fn j1(x: f64) -> f64 {
    let square = x * x;
    (x * polynomial(square, J1_NUM)) / polynomial(square, J1_DEN)
}

pub(super) fn y0(x: f64) -> f64 {
    let square = x * x;
    let rational = polynomial(square, Y0_NUM) / polynomial(square, Y0_DEN);
    rational + j0(x) * (W * excel_log(x))
}

pub(super) fn y1(x: f64) -> f64 {
    let square = x * x;
    let rational = (x * polynomial(square, Y1_NUM)) / polynomial(square, Y1_DEN);
    rational + W * (j1(x) * excel_log(x) - 1.0 / x)
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
    let twice_over_x = 2.0 / magnitude;
    let mut result = 0.0;
    let mut current = 1.0;
    let mut next = 0.0;
    let mut total = 0.0;
    let mut add_to_total = false;
    for index in (1..=start as i32).rev() {
        let previous = f64::from(index) * twice_over_x * current - next;
        next = current;
        current = previous;
        if current.abs() > 1E10 {
            current *= 1E-10;
            next *= 1E-10;
            result *= 1E-10;
            total *= 1E-10;
        }
        if add_to_total {
            total += current;
        }
        add_to_total = !add_to_total;
        if index == order {
            result = next;
        }
    }
    total = 2.0 * total - current;
    result /= total;
    if x < 0.0 && order % 2 != 0 {
        -result
    } else {
        result
    }
}

pub(super) fn y1_large(x: f64) -> f64 {
    let square = x * x;
    let phase = x - 2.356194491;
    let reduction = 64.0 / square;
    let amplitude = polynomial(reduction, AMP1);
    let correction = polynomial(reduction, PHASE1);
    let leading = excel_sin(phase) * amplitude;
    let trailing = ((8.0 / x) * excel_cos(phase)) * correction;
    let combined = leading + trailing;
    let scale = (W / x).sqrt();
    scale * combined
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

fn j_large(x: f64, offset: f64, amplitude: &[f64], correction: &[f64]) -> f64 {
    let square = x * x;
    let phase = x - offset;
    let reduction = 64.0 / square;
    let amplitude = polynomial(reduction, amplitude);
    let correction = polynomial(reduction, correction);
    let leading = excel_cos(phase) * amplitude;
    let trailing = ((8.0 / x) * excel_sin(phase)) * correction;
    let scale = (W / x).sqrt();
    scale * (leading - trailing)
}

pub(super) fn j0_large(x: f64) -> f64 {
    j_large(x.abs(), 0.785398164, AMP_J0, PHASE_J0)
}

pub(super) fn j1_large(x: f64) -> f64 {
    let value = j_large(x.abs(), 2.356194491, AMP_J1, PHASE1);
    if x < 0.0 { -value } else { value }
}

pub(super) fn y0_large(x: f64) -> f64 {
    let square = x * x;
    let phase = x - 0.785398164;
    let reduction = 64.0 / square;
    let amplitude = polynomial(reduction, AMP0);
    let correction = polynomial(reduction, PHASE_Y0);
    let leading = excel_sin(phase) * amplitude;
    let trailing = ((8.0 / x) * excel_cos(phase)) * correction;
    let scale = (W / x).sqrt();
    scale * (leading + trailing)
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
