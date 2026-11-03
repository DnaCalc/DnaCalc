//! Low-argument Bessel rational seeds adapted from SheetJS/bessel.
//! Copyright (C) 2013-present SheetJS; Apache-2.0.
//! Reference: bessel.js at 2e2331329891dc5c16bda3461db0b9fe590ff2e9.
//! Changed for DNA Calc: Rust scalar helpers, the existing worksheet logarithm,
//! and a fixed grouping of the Y0 logarithm term. See deploy/notices/ported.txt.
//! These published coefficients apply below argument magnitude 8.
use super::excel_numeric::excel_log;

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
    let mut value = *coefficients.last().unwrap();
    for &coefficient in coefficients[..coefficients.len() - 1].iter().rev() {
        value = x * value + coefficient;
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
