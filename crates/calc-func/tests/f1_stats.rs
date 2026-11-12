#![allow(clippy::excessive_precision)]

//! F1 (dc-8dq): the descriptive statistics, the distributions, SUBTOTAL and AGGREGATE, called
//! through `calc_func::invoke`. Expected values are Excel's documented examples (the function
//! pages' sample data) and values computed from the definitions.

mod f1_support;

use calc_func::SubtotalExclusions;
use calc_func::value::WorksheetErrorCode::{Div0, NA, Num, Value};
use f1_support::*;

/// Excel20527: density reuses GAMMA.DIST's stored Poisson mass, avoiding overflow.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chisq_density_and_cdf_underflow_use_measured_gamma_route() {
    exact(
        call(
            "CHISQ.DIST",
            vec![n(f64::from_bits(0x4036_ffff_ffff_ffff)), n(303.0), n(0.0)],
        ),
        f64::from_bits(0x2932_bbd6_5525_05e0),
    );
    exact(
        call(
            "CHISQ.DIST",
            vec![n(f64::from_bits(0x333f_0bd0_d7ee_4008)), n(10.0), n(1.0)],
        ),
        0.0,
    );
}

/// Excel20527 fresh controls: stored tail endpoint, signed modern tail,
/// legacy tails conversion, density domains and the signed inverse graph.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn student_t_stored_tails_and_admission_match_excel() {
    exact(call("T.DIST", vec![n(-1e-9), n(5.0), b(true)]), 0.5);
    exact(call("T.DIST.2T", vec![n(1e-9), n(5.0)]), 1.0);
    exact(call("T.DIST.RT", vec![n(-1.0), n(1.0)]), 0.75);
    error(call("TDIST", vec![n(-1.0), n(1.0), n(1.0)]), Num);
    error(call("T.DIST", vec![n(1e200), n(5.0), b(true)]), Num);
    error(call("T.DIST", vec![n(0.0), n(0.5), b(false)]), Div0);
    error(call("T.DIST", vec![n(0.0), n(0.5), b(true)]), Num);
    exact(
        call("T.DIST", vec![n(0.0), n(1e100), b(false)]),
        f64::from_bits(0x3fd9_8845_33d4_3651),
    );
    let edge = f64::from_bits(0x3fef_ffff_7ff0_0000);
    error(
        call(
            "TDIST",
            vec![n(0.1), n(5.0), n(f64::from_bits(edge.to_bits() - 1))],
        ),
        Num,
    );
    exact(
        call("TDIST", vec![n(0.1), n(5.0), n(edge)]),
        f64::from_bits(0x3fdd_934b_16c7_934b),
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn student_t_inverse_preserves_small_probability_and_signed_two_tail_domain() {
    exact(
        call("T.INV", vec![n(1e-100), n(1.0)]),
        f64::from_bits(0xd497_48e5_3d8d_16b1),
    );
    for name in ["T.INV.2T", "TINV"] {
        error(call(name, vec![n(0.0), n(1.0)]), Div0);
        exact(
            call(name, vec![n(1.1), n(1.0)]),
            f64::from_bits(0xbfc4_45f0_fbb1_cf9c),
        );
        exact(
            call(name, vec![n(1.5), n(2.0)]),
            f64::from_bits(0xbfea_20bd_700c_2c3d),
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn confidence_t_scales_before_multiplying_the_quantile() {
    error(call("CONFIDENCE.T", vec![n(0.05), n(2.0), n(1.9)]), Div0);
    exact(
        call(
            "CONFIDENCE.T",
            vec![n(0.7126096435331697), n(0.00016927413901532965), n(3783.0)],
        ),
        f64::from_bits(0x3eb1_026a_0c77_797d),
    );
}

fn data() -> CalcValue {
    arr(vec![vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]])
}

use calc_func::value::CalcValue;

/// Excel 20527 endpoint controls, independently verified through both aliases.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn inverse_probability_endpoints_follow_excel() {
    for name in ["BETA.INV", "BETAINV"] {
        for p in [0.0, 1.0] {
            error(call(name, vec![n(p), n(3.0), n(0.5)]), Num);
            error(
                call(name, vec![n(p), n(3.0), n(0.5), n(-1e308), n(1e308)]),
                Num,
            );
        }
    }
    for name in ["T.INV.2T", "TINV"] {
        for df in [1.0, 2.0, 3.0, 6.6, 100.0, 1e10] {
            exact(call(name, vec![n(1.0), n(df)]), 0.0);
        }
        for df in [0.0, 0.9, -1.0, 1e10 + 1.0] {
            error(call(name, vec![n(1.0), n(df)]), Num);
        }
        for p in [0.0, -0.1, 2.0] {
            error(call(name, vec![n(p), n(6.0)]), Num);
        }
    }
}

/// Excel 20527 rechecks of the four gained BETAINV bank rows (dc-bc1o).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn inverse_beta_selects_the_nearer_forward_lattice_point() {
    for (p, a, beta, expected) in [
        (0.01, 5.0, 1.5, 0x3fd5_9482_23a6_b46e),
        (0.02, 1.5, 2.5, 0x3fa1_0a18_97b8_fb54),
        (0.07, 6.0, 2.0, 0x3fe0_59a0_fadc_3d9d),
        (0.97, 3.0, 0.8, 0x3fef_dfa5_002e_8e6f),
    ] {
        for name in ["BETA.INV", "BETAINV"] {
            exact(
                call(name, vec![n(p), n(a), n(beta)]),
                f64::from_bits(expected),
            );
        }
    }
}

/// Excel 20527, dc-bc1o: Stirling/deviance normalization and compensated LN
/// preserve the captured PDF, zero density, large degrees and a leaf discriminator.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn student_density_uses_stirling_normalization() {
    for (x, df, expected) in [
        (2.0, 6.0, 0x3fb0_64ab_dc68_a194),
        (-2.0, 6.6, 0x3fb0_64ab_dc68_a194),
        (0.0, 4.0, 0x3fd7_ffff_ffff_ffff),
        (1.0, 1e10, 0x3fce_f8e5_8e2c_7081),
        (-0.08891541647874097, 79.0, 0x3fd9_5997_2a5f_b850),
    ] {
        exact(call("T.DIST", vec![n(x), n(df), b(false)]), f64::from_bits(expected));
    }
}

/// Excel 20527, dc-bc1o: T.TEST passes the rounded beta complement.
/// Captured Sheet1!B70 and fresh paired/equal/unequal-variance controls.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn t_test_uses_the_rounded_beta_complement() {
    let rows: &[(&[f64], &[f64], f64, f64, u64)] = &[
        (
            &[
                2.0,
                1.5,
                2.0,
                0.13333333333333333,
                1.3333333333333333,
                2.0,
                2.0,
                4.0,
                2.0,
            ],
            &[44.0, 1.3333333333333333, 5.0, 1.0, 2.0, 6.0, 6.6, 8.0, 1.0],
            1.0,
            1.0,
            0x3fb84c56c84b091d,
        ),
        (
            &[-13460.047476674108, 4063.7100155633734, 5926.867209324849],
            &[-6326.411088245899, -10894.522745224796, 14042.27879856386],
            1.0,
            1.0,
            0x3fdfb575c0d23bc7,
        ),
        (
            &[
                -2e-05,
                1.0625e-05,
                -1.5625e-05,
                -3.7500000000000005e-06,
                6.875e-06,
            ],
            &[8.750000000000001e-06, -9.375000000000001e-06],
            2.0,
            2.0,
            0x3fe75ef045e61dea,
        ),
        (
            &[
                1.059006167472183e-05,
                2.8472725291889227e-06,
                -1.265201997463216e-05,
            ],
            &[
                -3.9120116780406425e-06,
                -1.5952082508659517e-05,
                -1.027704090396139e-05,
                1.4018636517361448e-05,
                1.3106908972259917e-06,
                2.143459693991247e-06,
                1.6879043741574953e-05,
                8.89884347027774e-06,
                2.088946610560191e-05,
                -1.8774251275521393e-05,
            ],
            1.0,
            3.0,
            0x3fdc4bbde916f980,
        ),
    ];
    for &(x, y, tails, kind, expected) in rows {
        let args = vec![
            arr(vec![x.to_vec()]),
            arr(vec![y.to_vec()]),
            n(tails),
            n(kind),
        ];
        for name in ["T.TEST", "TTEST"] {
            exact(call(name, args.clone()), f64::from_bits(expected));
        }
    }
}

/// Excel 20527, dc-bc1o: a scaled density uses its lower-endpoint coordinate
/// and a stored-PC64 division by the width, after the unit density.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_density_scales_the_unit_interval() {
    for (x, alpha, beta, lower, upper, expected) in [
        (2.0, 2.0 / 15.0, 2.0 / 15.0, 1.0, 6.0, 0x3fb1_1fa0_8dae_a546),
        (25.635113942405695, 2.0, 1.0, 0.0, 51.27022788481139, 0x3f93_f8fc_a0f5_b0ae),
        (20.109578089941277, 2.0, 1.0, 0.0, 95.71736304926452, 0x3f71_fb1c_5506_a16c),
        (43.70425394550647, 1.0, 3.25, 0.0, 55.84508485255309, 0x3f5e_c5ba_d90f_7afd),
        (0.003733975812792778, 1.0, 1.0, -4.337338085534052e-19,
            0.007467951625585556, 0x4060_bcfa_144f_a0a2),
        (0.024923965334892273, 2.0, 1.0, -1.7349352342136207e-18,
            0.09969586133956909, 0x4014_0f9e_9143_b62a),
    ] {
        exact(
            call("BETA.DIST", vec![n(x), n(alpha), n(beta), b(false), n(lower), n(upper)]),
            f64::from_bits(expected),
        );
    }
}

/// Frozen fresh Excel 20527 controls distinguish centred arithmetic, staged
/// roots and final scaling; see correlation-arithmetic-2026-11-12.md.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn correlation_uses_stored_pc64_arithmetic() {
    let rows: &[(&[u64], &[u64], u64, u64)] = &[
        (
            &[0xc0a5_0e00_0000_0000, 0x40bc_6c00_0000_0000, 0x40c1_4e00_0000_0000,
              0x40c0_4e80_0000_0000, 0xc0b2_1300_0000_0000],
            &[0x40c2_ae80_0000_0000, 0x40a8_c400_0000_0000, 0xc0c3_1480_0000_0000,
              0xc0a6_ec00_0000_0000, 0xc0bc_2300_0000_0000],
            0xbfd1_6190_8f20_2263, 0x3fb2_e1a6_1e05_c3f8,
        ),
        (
            &[0xbe84_951d_036a_a800, 0xc13b_009d_dbea_e6cb, 0x414e_14dc_6216_0778],
            &[0xc000_1304_0835_5538, 0xc152_542d_3333_1dd2, 0xbf96_0797_3432_b300],
            0x3fe7_a298_1b19_6261, 0x3fe1_74f4_cdd9_dff8,
        ),
        (
            &[0xbf8c_a36c_3c51_ca3f, 0x4050_33ca_e351_c2b2,
              0xc146_05ba_cecc_aa2a, 0xc077_bd8e_3173_ec62],
            &[0x40be_ad58_6433_3a88, 0xc1a1_9bbf_f139_4e6a,
              0x3e92_5453_d51c_ee94, 0xbe51_fa87_86cb_8e73],
            0xbfd5_563b_3b03_8dec, 0x3fbc_742c_387e_fed5,
        ),
    ];
    for &(x, y, correlation, rsq) in rows {
        let values = |bits: &[u64]| {
            arr(vec![bits.iter().map(|&b| f64::from_bits(b)).collect()])
        };
        let args = vec![values(x), values(y)];
        for function in ["CORREL", "PEARSON"] {
            exact(call(function, args.clone()), f64::from_bits(correlation));
        }
        exact(call("RSQ", args), f64::from_bits(rsq));
    }
    // A first-pass double-rounding control: an ordinary mean makes the
    // correlation exactly zero; Excel retains this nonzero correlation.
    exact(
        call("CORREL", vec![
            arr(vec![vec![1.5, f64::from_bits(0x3ca0_0080_0000_0000), -1.5]]),
            arr(vec![vec![1.0, 2.0, 1.0]]),
        ]),
        f64::from_bits(0x3c82_79a7_4590_331c),
    );
}

/// Excel 20527, dc-wli1: CORREL/PEARSON use population normalisation, while
/// RSQ squares the sample-normalised correlation. These mathematically equal
/// three-point correlations distinguish the stored intermediate values.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn correlation_population_normalisation_and_rsq_sample_normalisation() {
    for (x, y, rsq_bits) in [
        (vec![1.0, 2.0, 3.0], vec![2.0, 5.0, 4.0], 0x3fdb_6db6_db6d_b6da),
        (vec![1.0, 2.0, 4.0], vec![3.0, 1.0, 5.0], 0x3fdb_6db6_db6d_b6dd),
    ] {
        let args = vec![arr(vec![x.clone()]), arr(vec![y.clone()])];
        for function in ["CORREL", "PEARSON"] {
            exact(call(function, args.clone()), f64::from_bits(0x3fe4_f2ec_413c_b52b));
        }
        exact(call("RSQ", args), f64::from_bits(rsq_bits));
        let mut sheet = Sheet::new();
        sheet.column(1, 1, x.into_iter().map(n).collect());
        sheet.column(1, 2, y.into_iter().map(n).collect());
        let args = vec![sheet.area(1, 1, 3, 1), sheet.area(1, 2, 3, 2)];
        exact(
            call_on(&mut sheet, "CORREL", args.clone()),
            f64::from_bits(0x3fe4_f2ec_413c_b52b),
        );
        exact(call_on(&mut sheet, "RSQ", args), f64::from_bits(rsq_bits));
    }
}

/// Excel 20527, dc-ss3b: SUM adjusts only its final numeric addition. The
/// service probes and frozen alternatives are in sum-cancellation-2026-11-12.md.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sum_adjusts_the_last_numeric_addition() {
    exact(call("SUM", vec![n(0.1), n(0.2), n(-0.3)]), 0.0);
    exact(
        call("SUM", vec![n(0.1), n(0.2), n(-0.3), n(0.0)]),
        f64::from_bits(0x3c90_0000_0000_0000),
    );
    exact(
        call("SUM", vec![n(0.1), n(0.2), n(-0.3), n(1e-20)]),
        f64::from_bits(0x3c90_00bc_e508_6492),
    );
    let values = arr(vec![vec![0.1, 0.2, -0.3]]);
    exact(call("SUM", vec![values.clone()]), 0.0);
    exact(call("SUM", vec![n(0.0), values.clone()]), 0.0);
    exact(
        call("SUM", vec![values, n(0.0)]),
        f64::from_bits(0x3c90_0000_0000_0000),
    );
    exact(call("SUM", vec![arr(vec![vec![0.1, 0.2]]), n(-0.3)]), 0.0);
    // The same fold reads referenced cells; trailing nonnumbers do not add.
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(0.1), n(0.2), n(-0.3), t("ignored"), b(true)]);
    let reference = sheet.area(1, 1, 6, 1);
    exact(call_on(&mut sheet, "SUM", vec![reference.clone()]), 0.0);
    sheet.column(6, 1, vec![n(0.0)]);
    exact(
        call_on(&mut sheet, "SUM", vec![reference]),
        f64::from_bits(0x3c90_0000_0000_0000),
    );
    // Relative binary exponents, both signs; no absolute epsilon.
    for scale in [2f64.powi(-900), 1.0, 2f64.powi(900)] {
        for sign in [-1.0, 1.0] {
            let left = sign * scale;
            exact(call("SUM", vec![n(left * (1.0 + 7.0 * f64::EPSILON)), n(-left)]), 0.0);
            exact(
                call("SUM", vec![n(left * (1.0 + 8.0 * f64::EPSILON)), n(-left)]),
                left * 8.0 * f64::EPSILON,
            );
        }
    }
    // The last left operand decides, not the larger operand.
    let a = 2.0 + 4.0 * 2f64.powi(-51);
    let b = -(2.0 - f64::EPSILON);
    exact(call("SUM", vec![n(a), n(b)]), 0.0);
    exact(call("SUM", vec![n(b), n(a)]), 9.0 * f64::EPSILON);
    // Both means adjust their numerator, before division. SUMPRODUCT retains
    // the unadjusted arithmetic result.
    for function in ["AVERAGE", "AVERAGEA"] {
        exact(call(function, vec![arr(vec![vec![0.1, 0.2, -0.3]])]), 0.0);
        exact(
            call(function, vec![arr(vec![vec![0.1, 0.2, -0.3, 0.0]])]),
            f64::from_bits(0x3c70_0000_0000_0000),
        );
        exact(
            call(function, vec![n(1.0 + 8.0 * f64::EPSILON), n(-1.0)]),
            4.0 * f64::EPSILON,
        );
    }
    exact(
        call("SUMPRODUCT", vec![arr(vec![vec![0.1, 0.2, -0.3]])]),
        f64::from_bits(0x3c90_0000_0000_0000),
    );
}


/// Original Excel20527 R38 questions: exact affine data, vector references,
/// duplicate aggregation, missing timestamps and the zero-confidence limit.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_exact_trends_and_timeline_statistics() {
    let y = arr(vec![vec![2.0, 5.0, 8.0, 11.0]]);
    let t = arr(vec![vec![1.0, 2.0, 3.0, 4.0]]);
    exact(
        call("FORECAST.ETS", vec![n(7.0), y.clone(), t.clone(), n(0.0)]),
        20.0,
    );
    exact(
        call("FORECAST.ETS.SEASONALITY", vec![y.clone(), t.clone()]),
        0.0,
    );
    for (stat, expected) in [
        (1.0, 0.9),
        (2.0, 0.001),
        (3.0, f64::EPSILON),
        (4.0, 0.0),
        (5.0, 0.0),
        (6.0, 0.0),
        (7.0, 0.0),
        (8.0, 1.0),
    ] {
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), t.clone(), n(stat), n(0.0)],
            ),
            expected,
        );
    }
    exact(
        call(
            "FORECAST.ETS.CONFINT",
            vec![n(7.0), y.clone(), t.clone(), n(0.95), n(0.0)],
        ),
        0.0,
    );
    exact(
        call(
            "FORECAST.ETS",
            vec![n(5.0), y.clone(), t.clone(), n(8_784.0)],
        ),
        14.0,
    );
    error(
        call(
            "FORECAST.ETS",
            vec![n(5.0), y.clone(), t.clone(), n(8_785.0)],
        ),
        Num,
    );
    error(
        call(
            "FORECAST.ETS.STAT",
            vec![y.clone(), t.clone(), n(8.0), n(8_785.0)],
        ),
        Num,
    );
    exact(
        call(
            "FORECAST.ETS",
            vec![n(4.0 + i32::MAX as f64), y.clone(), t.clone(), n(0.0)],
        ),
        11.0 + 3.0 * i32::MAX as f64,
    );
    error(
        call(
            "FORECAST.ETS",
            vec![n(4.0 + 2_147_483_648.0), y.clone(), t.clone(), n(0.0)],
        ),
        Num,
    );
    error(
        call(
            "FORECAST.ETS.CONFINT",
            vec![n(4.0 + 2_147_483_648.0), y.clone(), t.clone(), n(0.0)],
        ),
        Num,
    );
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(2.0), n(5.0), n(8.0), n(11.0)]);
    sheet.column(1, 2, vec![n(1.0), n(2.0), n(3.0), n(4.0)]);
    let args = vec![
        n(7.0),
        sheet.area(1, 1, 4, 1),
        sheet.area(1, 2, 4, 2),
        n(0.0),
    ];
    exact(call_on(&mut sheet, "FORECAST.ETS", args), 20.0);
    // General non-affine data still has a measured timeline step and a
    // zero confidence interval at confidence zero, independently of its fit.
    let noisy = arr(vec![vec![2.0, 3.0, 5.0, 7.0, 11.0, 13.0, 17.0, 19.0]]);
    let times = arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]]);
    exact(
        call(
            "FORECAST.ETS.STAT",
            vec![noisy.clone(), times.clone(), n(8.0)],
        ),
        1.0,
    );
    exact(
        call(
            "FORECAST.ETS.CONFINT",
            vec![n(9.0), noisy.clone(), times, n(0.0)],
        ),
        0.0,
    );
    // Reversed, duplicated and incomplete timelines use the same preparation.
    for times in [
        vec![8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0],
        vec![1.0, 2.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0],
        vec![1.0, 2.0, 3.0, 5.0, 6.0, 7.0, 8.0, 9.0],
    ] {
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![noisy.clone(), arr(vec![times]), n(8.0)],
            ),
            1.0,
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_errors_and_unresolved_arithmetic_remain_distinct() {
    let y = arr(vec![vec![2.0, 3.0, 5.0, 7.0]]);
    let t = arr(vec![vec![1.0, 2.0, 3.0, 4.0]]);
    error(
        call("FORECAST.ETS", vec![n(5.0), y.clone(), t.clone(), n(0.5)]),
        Num,
    );
    error(
        call(
            "FORECAST.ETS.CONFINT",
            vec![n(5.0), y.clone(), t.clone(), n(1.0)],
        ),
        Num,
    );
    // R38 separates the confidence horizon from FORECAST.ETS's historical
    // interpolation: even confidence zero needs a point after the history.
    error(
        call(
            "FORECAST.ETS.CONFINT",
            vec![n(4.0), y.clone(), t.clone(), n(0.0)],
        ),
        Num,
    );
    error(
        call(
            "FORECAST.ETS.CONFINT",
            vec![n(4.0), y.clone(), t.clone(), n(0.875)],
        ),
        Num,
    );
    error(
        call("FORECAST.ETS.STAT", vec![y.clone(), t.clone(), n(9.0)]),
        Num,
    );
    error(
        call(
            "FORECAST.ETS.SEASONALITY",
            vec![y.clone(), arr(vec![vec![1.0]])],
        ),
        NA,
    );
    let short = arr(vec![vec![11.0, 12.0]]);
    error(
        call(
            "FORECAST.ETS.STAT",
            vec![y.clone(), short.clone(), n(9.0), n(-1.0)],
        ),
        NA,
    );
    error(
        call(
            "FORECAST.ETS.CONFINT",
            vec![n(21.0), y.clone(), short, n(1.0), n(-1.0)],
        ),
        NA,
    );
    error(call("FORECAST.ETS", vec![n(2.0), n(1.0), n(1.0)]), Div0);
    for times in [vec![1.0, 1.0, 1.0, 1.0], vec![1.0, 2.0, 4.0, 8.0]] {
        error(
            call(
                "FORECAST.ETS.SEASONALITY",
                vec![y.clone(), arr(vec![times])],
            ),
            Num,
        );
    }
    // Excel's numeric answer for this adjacent constant is 0x3ff0000000000002,
    // not the input. Its private initialization/optimisation is still open.
    let adjacent = f64::from_bits(0x3ff0_0000_0000_0001);
    let unknown = call(
        "FORECAST.ETS",
        vec![
            n(6.0),
            arr(vec![vec![adjacent; 3]]),
            arr(vec![vec![1.0, 2.0, 3.0]]),
            n(0.0),
        ],
    );
    assert!(matches!(
        unknown,
        Err(calc_func::FunctionFault::NotHeld {
            owner: "dc-j9k",
            ..
        })
    ));
    // The history's initialization is exact, but this extrapolation separates
    // a rounded product/add from a single final store. Keep that stage open.
    assert!(matches!(
        call(
            "FORECAST.ETS",
            vec![
                n(1_757_867_173.0),
                arr(vec![vec![
                    32_830_194_465.0,
                    101_151_455_122.0,
                    169_472_715_779.0,
                    237_793_976_436.0
                ]]),
                t.clone(),
                n(0.0)
            ]
        ),
        Err(calc_func::FunctionFault::NotHeld {
            owner: "dc-j9k",
            ..
        })
    ));
    // The short-history extension (dc-6332.5) now computes this history; its
    // value is not an Excel measurement, so only the calculated outcome is held.
    assert!(matches!(
        call("FORECAST.ETS", vec![n(5.0), y, t, n(0.0)]),
        Ok(_)
    ));
    // Excel keeps the unit in this duplicate reduction: mean=1/3, sum=1.
    // The private reduction is open; an accidentally zero local sum must not
    // turn into an admitted exact trend. The independent step statistic works.
    let cancellation = arr(vec![
        vec![9_007_199_254_740_992.0, 1.0, -9_007_199_254_740_992.0].repeat(4),
    ]);
    let repeated = arr(vec![vec![
        1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 4.0,
    ]]);
    for reduction in [1.0, 7.0] {
        assert!(matches!(
            call(
                "FORECAST.ETS",
                vec![
                    n(5.0),
                    cancellation.clone(),
                    repeated.clone(),
                    n(0.0),
                    n(1.0),
                    n(reduction)
                ]
            ),
            Err(calc_func::FunctionFault::NotHeld {
                owner: "dc-j9k",
                ..
            })
        ));
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![
                    cancellation.clone(),
                    repeated.clone(),
                    n(8.0),
                    n(0.0),
                    n(1.0),
                    n(reduction),
                ],
            ),
            1.0,
        );
    }
    // Refuse the bounded model before multiplying a very large completed
    // timestamp count; those products can overflow usize in a browser.
    let sparse: Vec<f64> = (0..432)
        .map(|i| {
            if i == 0 {
                0.0
            } else {
                1.0 + (i - 1) as f64 * 1_000_000.0
            }
        })
        .collect();
    assert!(matches!(
        call(
            "FORECAST.ETS.STAT",
            vec![arr(vec![vec![1.0; 432]]), arr(vec![sparse]), n(8.0)]
        ),
        Err(calc_func::FunctionFault::NotHeld {
            owner: "dc-j9k",
            ..
        })
    ));
}

/// Excel20527 root-word controls: the sample deviation stores FSQRT from PC64.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sample_moments_store_the_pc64_square_root() {
    let values = [
        0xc011_9ef0_6a93_d623,
        0x401c_df7c_cd1a_a376,
        0xc01f_734e_107b_dd5c,
        0xbfde_9e26_ef0f_1a84,
    ]
    .map(f64::from_bits);
    for (name, expected) in [
        ("SKEW", 0x3fea_710b_3237_584c),
        ("KURT", 0x3fd6_409e_d237_9560),
    ] {
        exact(
            call(name, values.into_iter().map(n).collect()),
            f64::from_bits(expected),
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_small_inputs_record_known_gaps() {
    // Original recorded answers: one gain from each grouped-input half and a
    // matched control three ULP from the unconditional Taylor-series result.
    for (input, expected, calculated) in [
        (
            0x3f1a_529c_35ef_4724,
            0x40c3_7323_fd56_1ac3,
            0x40c37323fd561ac3,
        ),
        (
            0x3f1b_2638_e4e6_7c49,
            0x40c2_db88_6b75_1adc,
            0x40c2db886b751adc,
        ),
        (
            0x3f8f_e551_0015_81b1,
            0x404f_d2d4_c62f_1d4e,
            0x404fd2d4c62f1d4b,
        ),
    ] {
        recorded_calculation(
            call("GAMMA", vec![n(f64::from_bits(input))]),
            f64::from_bits(expected),
            f64::from_bits(calculated),
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn deviations_and_means() {
    exact(call("AVEDEV", vec![data()]), 1.5);
    exact(call("DEVSQ", vec![data()]), 32.0);
    close(
        call("GEOMEAN", vec![arr(vec![vec![1.0, 2.0, 4.0]])]),
        2.0,
        1e-15,
    );
    close(
        call("HARMEAN", vec![arr(vec![vec![1.0, 2.0, 4.0]])]),
        12.0 / 7.0,
        1e-15,
    );
    error(call("GEOMEAN", vec![n(-1.0)]), Num);
    exact(call("MEDIAN", vec![data()]), 4.5);
    exact(call("MEDIAN", vec![n(3.0), n(1.0), n(2.0)]), 2.0);
    error(call("MEDIAN", vec![arr_values(vec![vec![t("a")]])]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn geomean_stores_each_product_from_pc64() {
    // R26 canonical scalar-cell readings: an independently known square
    // rounding tie. Separate GEOMEAN(product,1,1) readings identify the
    // stored-PC64 product without assuming the private root implementation.
    let x = f64::from_bits(0x401f9ff8b1ae9622);
    // The non-power third factor also distinguishes a store after the square
    // from a resident-PC64 accumulator stored only after the last multiply.
    for (third, expected) in [
        (0x3e20000000000000, 0x3f74003a5b535262),
        (0x3ff4cccccccccc61, 0x40115357789642a4),
    ] {
        let third = f64::from_bits(third);
        let expected = f64::from_bits(expected);
        exact(call("GEOMEAN", vec![n(x), n(x), n(third)]), expected);
        let mut sheet = Sheet::new();
        sheet.column(1, 1, vec![n(x), n(x), n(third)]);
        let args = (1..=3).map(|row| sheet.area(row, 1, row, 1)).collect();
        exact(call_on(&mut sheet, "GEOMEAN", args), expected);
    }
}

/// R28 original Excel controls distinguish the root's integer exponent reduction.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn geomean_reduces_the_binary_exponent_before_the_private_root() {
    for (count, input, expected) in [
        (2, 0x3e0f_4000_0000_0030, 0x3eff_9f6e_4990_f23f),
        (3, 0x3c5c_0000_0000_0039, 0x3ebe_9b5d_ba58_18b3),
        (7, 0x48db_ffff_ffff_ffdd, 0x4135_20a4_5ca7_c7c2),
        (10, 0x340f_3fff_ffff_ffd0, 0x3ebf_ec98_2d5b_b8aa),
        (10, 0x340f_4000_0000_0001, 0x3ebf_ec98_2d5b_b8af),
        (11, 0x3234_cccc_cccc_ccd1, 0x3eb0_62de_8ac7_2091),
        (11, 0x32cf_3fff_ffff_ffc4, 0x3ebd_fb27_fa5d_c3fb),
        (11, 0x4db4_cccc_cccc_ccd1, 0x4130_62de_8ac7_2091),
        (16, 0x2c1b_ffff_ffff_fff8, 0x3eb2_11b6_3ba0_196d),
    ] {
        let mut args = vec![n(1.0); count];
        args[0] = n(f64::from_bits(input));
        exact(call("GEOMEAN", args), f64::from_bits(expected));
    }
}

/// Independent public leaf calls account for the six normalization losses.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn geomean_publishes_the_reduced_log_and_exp_argument() {
    for (count, input, expected) in [
        (2, 0x430f_4000_0000_0030, 0x417f_9f6e_4990_f240),
        (3, 0x43dc_0000_0000_0039, 0x413e_9b5d_ba58_18b2),
        (7, 0x375b_ffff_ffff_ffdd, 0x3eb5_20a4_5ca7_c7c3),
        (10, 0x4d0f_4000_0000_0001, 0x413f_ec98_2d5b_b8b0),
        (11, 0x4e4f_3fff_ffff_ffc4, 0x413d_fb27_fa5d_c3fc),
        (16, 0x541b_ffff_ffff_fff8, 0x4132_11b6_3ba0_196e),
    ] {
        let mut args = vec![n(1.0); count];
        args[0] = n(f64::from_bits(input));
        exact(call("GEOMEAN", args), f64::from_bits(expected));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn variances() {
    exact(call("STDEV.P", vec![data()]), 2.0);
    exact(call("STDEVP", vec![data()]), 2.0);
    close(call("STDEV.S", vec![data()]), 2.138089935299395, 1e-15);
    close(call("STDEV", vec![data()]), 2.138089935299395, 1e-15);
    exact(call("VAR.P", vec![data()]), 4.0);
    close(call("VAR.S", vec![data()]), 4.571428571428571, 1e-15);
    error(call("VAR.S", vec![n(1.0)]), Div0);
    // In an array the A-suffixed forms pass over text and logicals, as the plain forms do
    // (dc-bd5: Excel 16.0.20527 gives VARPA({1,TRUE,3}) 1 and STDEVPA({1,"a",3}) 1).
    let mixed = arr_values(vec![vec![n(1.0), b(true), t("x")]]);
    exact(call("VARPA", vec![mixed.clone()]), 0.0);
    exact(call("VARP", vec![mixed]), 0.0);
    let measured = arr_values(vec![vec![n(1.0), b(true), n(3.0)]]);
    exact(call("VARPA", vec![measured]), 1.0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn variance_uses_raw_moments_and_stores_pc64_steps() {
    // dc-j9k.13: unchanged dcj9k_statistics rows3/4, and original Value2
    // bank controls that distinguish stored-PC64 from binary64 arithmetic.
    let values = arr(vec![vec![
        157.0, 376.82, 189.207, 12.38, 12.473, 483.0, 306.24, 324.1852, 482.235, 78.0,
    ]]);
    exact(
        call("VAR.S", vec![values.clone()]),
        f64::from_bits(0x40df28debfbf8a84),
    );
    exact(
        call("VAR.P", vec![values]),
        f64::from_bits(0x40dc0b2edfc5fcac),
    );
    for (function, inputs, expected) in [
        (
            "VAR.S",
            vec![0x401f9ff8b1ae9622, 0x3dec0d7d522e744a],
            0x403f41118f2dd16d,
        ),
        (
            "VARA",
            vec![
                0x4131a34bfb9b7e44,
                0x40fa6b7f92badf71,
                0x3f75f0dffa58b4e0,
                0x4010650ce689e755,
            ],
            0x42526677cc6392c1,
        ),
        (
            "VARPA",
            vec![
                0xbfa2f23c41294257,
                0xbf93ada99cf88290,
                0xbfa7af86a74f6c6f,
                0xbf9b0346769d2b10,
            ],
            0x3f1bbc847d1cdb90,
        ),
        (
            "VARPA",
            vec![
                0xc00bfc19a70301b6,
                0xc02a57246263f85d,
                0xc013c8bda588aba9,
                0x3fdd23da709d15f7,
            ],
            0x40389bdd995cc0d4,
        ),
    ] {
        exact(
            call(
                function,
                inputs.into_iter().map(|b| n(f64::from_bits(b))).collect(),
            ),
            f64::from_bits(expected),
        );
    }
    // Large-offset cancellation takes the centered pass.
    exact(
        call(
            "VAR.S",
            vec![arr(vec![vec![1000000001.0, 1000000002.0, 1000000003.0]])],
        ),
        1.0,
    );
    exact(call("VAR.P", vec![n(7.0)]), 0.0);
    error(call("VAR.S", vec![n(7.0)]), Div0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn variance_centered_guard_uses_the_absolute_mean() {
    // dc-j9k.13: original VAR-00375 and R25 canonical Excel Value2
    // scale probes, build20527. The population and sample switches differ
    // even though power-of-two scaling preserves relative conditioning.
    for (function, exponent, expected) in [
        ("VAR.S", 0, 0x3e6f1f88c6bbc51a),
        ("VAR.S", 2, 0x3eaf1f88c6bbc519),
        ("VAR.P", 2, 0x3e9f1f88c6bbc51a),
        ("VAR.P", 4, 0x3edf1f88c6bbc519),
    ] {
        let inputs = [0x3df64afacd3a935a, 0x3f3650b65dc7a1d6]
            .into_iter()
            .map(|b| n(f64::from_bits(b) * 2.0f64.powi(exponent)))
            .collect();
        exact(call(function, inputs), f64::from_bits(expected));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn stdev_stores_the_extended_square_root() {
    // R25 canonical Excel scalar Value2 readings, build20527. These dyadic
    // triples separate one binary64 rounding from PC64 sqrt followed by a
    // double store; both upward and downward double-rounding edges occur.
    for (function, x, expected) in [
        ("STDEV.S", 0x3ff07f30e248f14c, 0x3fe2c4c55ae2d85e),
        ("STDEV.S", 0x3ff1263494ab94ff, 0x3fe32c5695fa57ce),
        ("STDEV.P", 0x3ffcc9cd40bfadd4, 0x3fe78de332b83efa),
        ("STDEV.P", 0x3ffcb2ce7481aa28, 0x3fe77bdc0a81553c),
    ] {
        exact(
            call(function, vec![n(0.0), n(1.0), n(f64::from_bits(x))]),
            f64::from_bits(expected),
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn variance_cancellation_guard_keeps_the_observed_edge() {
    // Adjacent scalar-input edges bisected in Excel build20527 at both
    // unit and2^20 scales. The old numerator<0.01*squares guard differs.
    for (function, last, first, expected) in [
        (
            "VAR.S",
            0x3ff1f82ec882c0f6,
            0x3fec0fa26efa7e14,
            0x3f8f07c1f07c1e94,
        ),
        (
            "VAR.S",
            0x3ff1f82ec882c0f7,
            0x3fec0fa26efa7e12,
            0x3f8f07c1f07c1f00,
        ),
        (
            "VAR.P",
            0x3ff1f82ec882c0f6,
            0x3fec0fa26efa7e14,
            0x3f84afd6a052bf0d,
        ),
        (
            "VAR.P",
            0x3ff1f82ec882c0f7,
            0x3fec0fa26efa7e12,
            0x3f84afd6a052bf1c,
        ),
    ] {
        exact(
            call(
                function,
                vec![n(f64::from_bits(first)), n(1.0), n(f64::from_bits(last))],
            ),
            f64::from_bits(expected),
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn variance_cancellation_guard_stores_the_ratio() {
    // New R25 scalar readings distinguish the stored ratio from0.99*Q
    // and correction/0.99, which the unit-mean bisections did not separate.
    for (inputs, expected) in [
        (
            [0x3ff855cedad2ef9d, 0x3ffbc048fa348fe5, 0x3fff2ac31996302d],
            0x3fa7564c31110ea2,
        ),
        (
            [0x3ff77a7154a3211f, 0x3ffac620a1ad2489, 0x3ffe11cfeeb727f3],
            0x3fa5b8f7fad4bab8,
        ),
    ] {
        exact(
            call(
                "VAR.S",
                inputs.into_iter().map(|b| n(f64::from_bits(b))).collect(),
            ),
            f64::from_bits(expected),
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn order_statistics() {
    exact(call("MODE.SNGL", vec![data()]), 4.0);
    exact(call("MODE", vec![n(1.0), n(2.0), n(2.0), n(1.0)]), 1.0);
    error(call("MODE.SNGL", vec![arr(vec![vec![1.0, 2.0]])]), NA);
    let (rows, cols, cells) = array(call(
        "MODE.MULT",
        vec![arr(vec![vec![1.0, 3.0, 2.0, 3.0, 2.0]])],
    ));
    assert_eq!((rows, cols, cells), (2, 1, vec![3.0, 2.0]));
    exact(call("LARGE", vec![data(), n(2.0)]), 7.0);
    exact(call("SMALL", vec![data(), n(2.0)]), 4.0);
    error(call("LARGE", vec![data(), n(9.0)]), Num);
    let five = arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]);
    close(
        call("PERCENTILE.INC", vec![five.clone(), n(0.3)]),
        2.2,
        1e-15,
    );
    close(call("PERCENTILE", vec![five.clone(), n(0.3)]), 2.2, 1e-15);
    close(
        call("PERCENTILE.EXC", vec![five.clone(), n(0.3)]),
        1.8,
        1e-15,
    );
    exact(call("QUARTILE.INC", vec![five.clone(), n(1.0)]), 2.0);
    error(call("QUARTILE.EXC", vec![five.clone(), n(4.0)]), Num);
    exact(call("PERCENTRANK.INC", vec![five.clone(), n(2.0)]), 0.25);
    exact(
        call(
            "TRIMMEAN",
            vec![arr(vec![(1..=10).map(f64::from).collect()]), n(0.2)],
        ),
        5.5,
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ranks_read_a_reference() {
    let mut sheet = Sheet::new();
    sheet.column(
        1,
        1,
        [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0].map(n).to_vec(),
    );
    let range = sheet.area(1, 1, 8, 1);
    exact(
        call_on(&mut sheet, "RANK.EQ", vec![n(4.0), range.clone()]),
        5.0,
    );
    exact(
        call_on(&mut sheet, "RANK", vec![n(4.0), range.clone()]),
        5.0,
    );
    exact(
        call_on(&mut sheet, "RANK.AVG", vec![n(4.0), range.clone()]),
        6.0,
    );
    exact(
        call_on(&mut sheet, "RANK.EQ", vec![n(4.0), range.clone(), n(1.0)]),
        2.0,
    );
    error(call_on(&mut sheet, "RANK.EQ", vec![n(3.0), range]), NA);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn paired_statistics() {
    let x = arr(vec![vec![1.0, 2.0, 3.0]]);
    let y = arr(vec![vec![2.0, 4.0, 7.0]]);
    close(
        call("CORREL", vec![x.clone(), y.clone()]),
        0.9933992677987828,
        1e-14,
    );
    close(
        call("PEARSON", vec![x.clone(), y.clone()]),
        0.9933992677987828,
        1e-14,
    );
    close(
        call("RSQ", vec![y.clone(), x.clone()]),
        0.9868421052631579,
        1e-14,
    );
    close(call("SLOPE", vec![y.clone(), x.clone()]), 2.5, 1e-15);
    close(
        call("INTERCEPT", vec![y.clone(), x.clone()]),
        -2.0 / 3.0,
        1e-14,
    );
    close(
        call("FORECAST", vec![n(4.0), y.clone(), x.clone()]),
        28.0 / 3.0,
        1e-14,
    );
    close(
        call("FORECAST.LINEAR", vec![n(4.0), y.clone(), x.clone()]),
        28.0 / 3.0,
        1e-14,
    );
    close(
        call("COVARIANCE.P", vec![x.clone(), y.clone()]),
        5.0 / 3.0,
        1e-15,
    );
    close(call("COVAR", vec![x.clone(), y.clone()]), 5.0 / 3.0, 1e-15);
    close(call("COVARIANCE.S", vec![x.clone(), y.clone()]), 2.5, 1e-15);
    error(call("CORREL", vec![x, arr(vec![vec![1.0, 2.0]])]), NA);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn moments_and_transforms() {
    let d = arr(vec![vec![3.0, 4.0, 5.0, 2.0, 3.0, 4.0, 5.0, 6.0, 4.0, 7.0]]);
    close(call("KURT", vec![d.clone()]), -0.15179963720841538, 1e-12);
    close(call("SKEW", vec![d]), 0.3595430714067975, 1e-12);
    close(
        call("STANDARDIZE", vec![n(42.0), n(40.0), n(1.5)]),
        4.0 / 3.0,
        1e-15,
    );
    error(call("STANDARDIZE", vec![n(42.0), n(40.0), n(0.0)]), Num);
    close(call("FISHER", vec![n(0.75)]), 0.9729550745276566, 1e-15);
    close(call("FISHERINV", vec![n(0.9729550745276566)]), 0.75, 1e-15);
    error(call("FISHER", vec![n(1.0)]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn prob_and_frequency() {
    // Excel 20527: direct text/logicals are ignored, and a nonnumeric bins
    // argument supplies the zero bin, including a referenced error.
    for datum in [t("-1.25"), CalcValue::logical(true), CalcValue::empty()] {
        let (_, _, counts) = array(call(
            "FREQUENCY",
            vec![datum, arr(vec![vec![3.0, 6.0], vec![5.0, 4.0]])],
        ));
        assert_eq!(counts, vec![0.0; 5]);
    }
    for bins in [t("2"), t("hello"), CalcValue::error(NA), CalcValue::empty()] {
        let (_, _, counts) = array(call(
            "FREQUENCY",
            vec![arr(vec![vec![-1.0, 0.0, 1.0, 2.0]]), bins],
        ));
        assert_eq!(counts, vec![2.0, 2.0]);
    }

    let xs = arr(vec![vec![0.0, 1.0, 2.0, 3.0]]);
    let ps = arr(vec![vec![0.2, 0.3, 0.1, 0.4]]);
    close(
        call("PROB", vec![xs.clone(), ps.clone(), n(2.0)]),
        0.1,
        1e-15,
    );
    close(call("PROB", vec![xs, ps, n(1.0), n(3.0)]), 0.8, 1e-15);
    let (rows, cols, cells) = array(call(
        "FREQUENCY",
        vec![
            arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]),
            arr(vec![vec![2.0, 4.0]]),
        ],
    ));
    assert_eq!((rows, cols, cells), (3, 1, vec![2.0, 2.0, 1.0]));
    // Unsorted bins: each counts the numbers above the next smaller bin.
    let (_, _, cells) = array(call(
        "FREQUENCY",
        vec![
            arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]),
            arr(vec![vec![4.0, 2.0]]),
        ],
    ));
    assert_eq!(cells, vec![2.0, 2.0, 1.0]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn regressions() {
    let y = arr(vec![vec![1.0], vec![2.0], vec![3.0]]);
    let x = arr(vec![vec![1.0], vec![2.0], vec![3.0]]);
    let (rows, cols, cells) = array(call("LINEST", vec![y.clone(), x.clone()]));
    assert_eq!((rows, cols), (1, 2));
    assert!(
        (cells[0] - 1.0).abs() < 1e-14 && cells[1].abs() < 1e-14,
        "{cells:?}"
    );
    close(call("TREND", vec![y, x.clone(), n(4.0)]), 4.0, 1e-14);
    let g = arr(vec![vec![2.0], vec![4.0], vec![8.0]]);
    close(
        call("GROWTH", vec![g.clone(), x.clone(), n(4.0)]),
        16.0,
        1e-13,
    );
    let (_, _, cells) = array(call("LOGEST", vec![g, x]));
    assert!(
        (cells[0] - 2.0).abs() < 1e-13 && (cells[1] - 1.0).abs() < 1e-13,
        "{cells:?}"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn regression_discards_a_predictor_below_the_dot_product_roundoff_bound() {
    // Recorded catalogue dcj9k_regression: this second predictor is nearly
    // twice the first. Excel drops the first after pivoting to the larger
    // column, leaving three residual degrees of freedom without a constant.
    let y = arr(vec![vec![1.0], vec![2.0], vec![4.0], vec![7.0]]);
    let x = arr(vec![
        vec![1.0, 2.0],
        vec![2.0, 4.0],
        vec![5.0, 10.0],
        vec![4.0, 8.000001],
    ]);
    for function in ["LINEST", "LOGEST"] {
        let result = call(function, vec![y.clone(), x.clone(), b(false), b(true)]).unwrap();
        let cells = result.as_array().expect("regression statistics array");
        assert_eq!((cells.shape().rows, cells.shape().cols), (5, 3));
        exact(Ok(cells.get(3, 1).unwrap().clone()), 3.0);
        exact(
            Ok(cells.get(0, 1).unwrap().clone()),
            if function == "LOGEST" { 1.0 } else { 0.0 },
        );
        exact(Ok(cells.get(1, 1).unwrap().clone()), 0.0);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn regression_keeps_scaled_independent_predictors_from_r16() {
    // Original R16 capture rows649..672. Changing the units of the second
    // predictor does not make this recorded design rank deficient in Excel.
    let y = arr(vec![vec![1.0], vec![2.0], vec![4.0], vec![7.0]]);
    for factor in [2.0_f64.powi(-20), 2.0_f64.powi(20)] {
        let x = arr([1.0, 2.0, 5.0, 4.0]
            .into_iter()
            .enumerate()
            .map(|(i, v)| {
                vec![
                    v,
                    (2.0 * v + if i == 3 { 2.0_f64.powi(-16) } else { 0.0 }) * factor,
                ]
            })
            .collect());
        for constant in [false, true] {
            for function in ["LINEST", "LOGEST"] {
                let result =
                    call(function, vec![y.clone(), x.clone(), b(constant), b(true)]).unwrap();
                let cells = result.as_array().expect("regression statistics array");
                exact(
                    Ok(cells.get(3, 1).unwrap().clone()),
                    if constant { 1.0 } else { 2.0 },
                );
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn distributions_match_excel_examples() {
    close(
        call("NORM.S.DIST", vec![n(1.0), b(true)]),
        0.8413447460685429,
        1e-12,
    );
    close(call("NORMSDIST", vec![n(1.0)]), 0.8413447460685429, 1e-12);
    close(
        call("NORM.DIST", vec![n(42.0), n(40.0), n(1.5), b(true)]),
        0.9087887802741321,
        1e-12,
    );
    close(
        call("NORM.DIST", vec![n(42.0), n(40.0), n(1.5), b(false)]),
        0.10934004978399575,
        1e-12,
    );
    close(
        call("NORM.INV", vec![n(0.908789), n(40.0), n(1.5)]),
        42.000002,
        1e-7,
    );
    close(call("NORM.S.INV", vec![n(0.908789)]), 1.3333347, 1e-6);
    error(call("NORM.S.INV", vec![n(1.0)]), Num);
    close(
        call("LOGNORM.DIST", vec![n(4.0), n(3.5), n(1.2), b(true)]),
        0.0390835557068005,
        1e-10,
    );
    close(
        call("CHISQ.DIST.RT", vec![n(18.307), n(10.0)]),
        0.0500006,
        1e-5,
    );
    close(call("CHIDIST", vec![n(18.307), n(10.0)]), 0.0500006, 1e-5);
    close(
        call("T.DIST.2T", vec![n(1.959999998), n(60.0)]),
        0.054644930,
        1e-7,
    );
    close(
        call("TDIST", vec![n(1.959999998), n(60.0), n(2.0)]),
        0.054644930,
        1e-7,
    );
    close(
        call("F.DIST.RT", vec![n(15.2068649), n(6.0), n(4.0)]),
        0.01,
        1e-6,
    );
    close(
        call("BINOM.DIST", vec![n(6.0), n(10.0), n(0.5), b(false)]),
        0.205078125,
        1e-15,
    );
    close(
        call("POISSON.DIST", vec![n(2.0), n(5.0), b(false)]),
        0.084224337,
        1e-8,
    );
    close(
        call("EXPON.DIST", vec![n(0.2), n(10.0), b(true)]),
        0.864664716763387,
        1e-12,
    );
    close(
        call("WEIBULL.DIST", vec![n(105.0), n(20.0), n(100.0), b(true)]),
        0.929581390069277,
        1e-12,
    );
    close(
        call(
            "HYPGEOM.DIST",
            vec![n(1.0), n(4.0), n(8.0), n(20.0), b(false)],
        ),
        0.363261093911249,
        1e-12,
    );
    close(
        call("NEGBINOM.DIST", vec![n(10.0), n(5.0), n(0.25), b(false)]),
        0.055048660375178,
        1e-12,
    );
    // fresh-negbinom.dist-00113-fresh-endpoints: retain the recorded exact dyadic sum.
    // Other classified distribution losses do not authorize a tolerance for this answer.
    exact(
        call("NEGBINOM.DIST", vec![n(4.0), n(11.0), n(0.75), b(true)]),
        f64::from_bits(0x3fe5_f7b1_5d80_0000),
    );
    close(
        call(
            "BETA.DIST",
            vec![n(2.0), n(8.0), n(10.0), b(true), n(1.0), n(3.0)],
        ),
        0.685470581,
        1e-9,
    );
    close(
        call("GAMMA.DIST", vec![n(10.00001131), n(9.0), n(2.0), b(true)]),
        0.068094,
        1e-5,
    );
    exact(call("GAMMA", vec![n(5.0)]), 24.0);
    close(call("GAMMALN", vec![n(4.0)]), 6.0f64.ln(), 1e-15);
    close(
        call("CONFIDENCE.NORM", vec![n(0.05), n(2.5), n(50.0)]),
        0.692951912174839,
        1e-12,
    );
    close(call("GAUSS", vec![n(2.0)]), 0.477249868051821, 1e-12);
    close(call("PHI", vec![n(0.75)]), 0.301137432154804, 1e-12);
    close(call("ERF", vec![n(1.0)]), 0.8427007929497149, 1e-14);
    close(call("ERFC", vec![n(1.0)]), 0.1572992070502851, 1e-14);
    close(call("ERF", vec![n(0.0), n(1.0)]), 0.8427007929497149, 1e-14);
    // A cumulative flag is read as a number.
    error(call("NORM.S.DIST", vec![n(1.0), t("yes")]), Value);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn tests_of_samples() {
    let a1 = arr(vec![vec![3.0, 4.0, 5.0, 8.0, 9.0, 1.0, 2.0, 4.0, 5.0]]);
    let a2 = arr(vec![vec![6.0, 19.0, 3.0, 2.0, 14.0, 4.0, 5.0, 17.0, 1.0]]);
    close(
        call("T.TEST", vec![a1.clone(), a2.clone(), n(2.0), n(1.0)]),
        0.196015785,
        1e-8,
    );
    close(
        call(
            "F.TEST",
            vec![
                arr(vec![vec![6.0, 7.0, 9.0, 15.0, 21.0]]),
                arr(vec![vec![20.0, 28.0, 31.0, 38.0, 40.0]]),
            ],
        ),
        0.648317846786174,
        1e-12,
    );
    error(call("T.TEST", vec![a1, a2, n(2.0), n(4.0)]), Num);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn subtotal_needs_the_readers_exclusions() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(1.0), n(2.0), n(3.0), n(10.0)]);
    let range = sheet.area(1, 1, 4, 1);
    // A reader that cannot say which cells hold a SUBTOTAL formula: refused, not guessed.
    refused(call_on(&mut sheet, "SUBTOTAL", vec![n(9.0), range.clone()]));
    // A4 holds a nested SUBTOTAL; row 2 is hidden by hand.
    let a4 = sheet.key(4, 1);
    sheet.exclusions = Some(SubtotalExclusions {
        nested_subtotals: vec![a4],
        hidden_rows: vec![2],
        filtered_rows: vec![],
    });
    exact(
        call_on(&mut sheet, "SUBTOTAL", vec![n(9.0), range.clone()]),
        6.0,
    );
    exact(
        call_on(&mut sheet, "SUBTOTAL", vec![n(109.0), range.clone()]),
        4.0,
    );
    exact(
        call_on(&mut sheet, "SUBTOTAL", vec![n(1.0), range.clone()]),
        2.0,
    );
    exact(
        call_on(&mut sheet, "SUBTOTAL", vec![n(4.0), range.clone()]),
        3.0,
    );
    exact(
        call_on(&mut sheet, "SUBTOTAL", vec![n(103.0), range.clone()]),
        2.0,
    );
    error(
        call_on(&mut sheet, "SUBTOTAL", vec![n(12.0), range.clone()]),
        Value,
    );
    // AGGREGATE's option 4 leaves nothing out, so it needs no exclusions.
    sheet.exclusions = None;
    exact(
        call_on(&mut sheet, "AGGREGATE", vec![n(9.0), n(4.0), range.clone()]),
        16.0,
    );
    refused(call_on(
        &mut sheet,
        "AGGREGATE",
        vec![n(9.0), n(0.0), range],
    ));
    // The array form with option 6 leaves out errors.
    let with_error = arr_values(vec![vec![n(1.0), e(Div0), n(3.0)]]);
    exact(
        call(
            "AGGREGATE",
            vec![n(14.0), n(6.0), with_error.clone(), n(1.0)],
        ),
        3.0,
    );
    error(
        call("AGGREGATE", vec![n(14.0), n(4.0), with_error, n(1.0)]),
        Div0,
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn rounding_to_a_significance() {
    exact(call("CEILING.MATH", vec![n(-8.1), n(2.0)]), -8.0);
    exact(call("CEILING.MATH", vec![n(-5.5), n(2.0), n(-1.0)]), -6.0);
    exact(call("CEILING.MATH", vec![n(24.3), n(5.0)]), 25.0);
    exact(call("FLOOR.MATH", vec![n(-8.1), n(2.0)]), -10.0);
    exact(call("FLOOR.MATH", vec![n(-5.5), n(2.0), n(-1.0)]), -4.0);
    exact(call("CEILING.PRECISE", vec![n(4.3)]), 5.0);
    exact(call("CEILING.PRECISE", vec![n(-4.3), n(2.0)]), -4.0);
    exact(call("ISO.CEILING", vec![n(-4.3), n(2.0)]), -4.0);
    exact(call("FLOOR.PRECISE", vec![n(-3.2), n(-1.0)]), -4.0);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn random_numbers_draw_from_the_services_source() {
    refused(call("RAND", vec![]));
    exact(call_random(&mut Fixed(vec![0.25]), "RAND", vec![]), 0.25);
    refused(call_random(&mut Fixed(vec![1.0]), "RAND", vec![]));
    exact(
        call_random(&mut Fixed(vec![0.5]), "RANDBETWEEN", vec![n(1.0), n(10.0)]),
        6.0,
    );
    exact(
        call_random(&mut Fixed(vec![0.0]), "RANDBETWEEN", vec![n(1.5), n(10.0)]),
        2.0,
    );
    error(
        call_random(&mut Fixed(vec![0.5]), "RANDBETWEEN", vec![n(10.0), n(1.0)]),
        Num,
    );
    let (rows, cols, cells) = array(call_random(
        &mut Fixed(vec![0.0, 0.5, 0.75, 0.99]),
        "RANDARRAY",
        vec![n(2.0), n(2.0), n(1.0), n(4.0), b(true)],
    ));
    assert_eq!((rows, cols, cells), (2, 2, vec![1.0, 3.0, 4.0, 4.0]));
}

/// Recorded build-20131 rows oc1540/oc35142/oc35121 and W111 product-overflow rows.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn exponential_distribution_uses_the_recorded_arithmetic_stores() {
    for (x, lambda, cumulative, expected) in [
        (
            0x3e9f46de57161763,
            0x3ff0000000000000,
            true,
            0x3e9f46dddcce69b3,
        ),
        (
            0x3eeaf5c72d71a9d8,
            0x3fc2294a8d5b7317,
            true,
            0x3ebe9a11ac30f36e,
        ),
        (
            0x3ec7bfe8f913eebb,
            0x3ff406b01029c9a0,
            false,
            0x3ff406ab699425f4,
        ),
    ] {
        exact(
            call(
                "EXPON.DIST",
                vec![
                    n(f64::from_bits(x)),
                    n(f64::from_bits(lambda)),
                    b(cumulative),
                ],
            ),
            f64::from_bits(expected),
        );
    }
    for (x, lambda, cumulative) in [
        (0x418d8b21f25875a1, 0x7f685d78a25c9e53, true),
        (0x42577d4b99895077, 0x7db09552fcb9586c, false),
    ] {
        error(
            call(
                "EXPON.DIST",
                vec![
                    n(f64::from_bits(x)),
                    n(f64::from_bits(lambda)),
                    b(cumulative),
                ],
            ),
            Num,
        );
    }
    // The shape-one gamma CDF calls the same exponential kernel (recorded row oc61017).
    exact(
        call(
            "GAMMA.DIST",
            vec![
                n(f64::from_bits(0x3e54df1215fc525b)),
                n(1.0),
                n(1.0),
                b(true),
            ],
        ),
        f64::from_bits(0x3e54df12129517f4),
    );
}

/// Recorded GAUSS subnormal-power rows and ERFC tail controls (W109/W111).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn normal_error_functions_publish_recorded_underflows() {
    for bits in [1, 2, 0x0008_0000_0000_0000, 0x8008_0000_0000_0000] {
        exact(call("GAUSS", vec![n(f64::from_bits(bits))]), 0.0);
    }
    for function in ["ERFC", "ERFC.PRECISE"] {
        for bits in [0x403a_c3c9_6948_f741, 0x403b_0000_0000_0000] {
            exact(call(function, vec![n(f64::from_bits(bits))]), 0.0);
        }
    }
}

/// Recorded ERFC dependencies and normal tails; original answers and known gaps.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn error_function_tails_record_known_gaps() {
    // These ERFC arguments belong to different halves of the frozen split.
    for function in ["ERFC", "ERFC.PRECISE"] {
        recorded_calculation(
            call(function, vec![n(f64::from_bits(0x4021_0c8a_deec_196e))]),
            f64::from_bits(0x3922_d992_80b1_9df9),
            f64::from_bits(0x3922d99280b19df9),
        );
        recorded_calculation(
            call(function, vec![n(f64::from_bits(0x4024_2bc8_1cf3_8194))]),
            f64::from_bits(0x3680_fa89_4f89_d6df),
            f64::from_bits(0x3680fa894f89d6df),
        );
        // Residuals between 2^-49 and 2^-48, from both split halves.
        recorded_calculation(
            call(function, vec![n(f64::from_bits(0x4019_3db4_8c78_fb1d))]),
            f64::from_bits(0x3c20_95a4_42f6_d57d),
            f64::from_bits(0x3c2095a442f6d57d),
        );
        recorded_calculation(
            call(function, vec![n(f64::from_bits(0x401c_c218_61f7_e239))]),
            f64::from_bits(0x3b0a_c48e_8af8_7271),
            f64::from_bits(0x3b0ac48e8af87271),
        );
        // Residuals between 2^-50 and 2^-49, from both split halves.
        recorded_calculation(
            call(function, vec![n(f64::from_bits(0x4010_5ad1_5827_f62f))]),
            f64::from_bits(0x3e3f_a6da_a7f6_9fce),
            f64::from_bits(0x3e3fa6daa7f69fce),
        );
        recorded_calculation(
            call(function, vec![n(f64::from_bits(0x4010_836a_af1b_be42))]),
            f64::from_bits(0x3e36_a4ab_c77f_b3ec),
            f64::from_bits(0x3e36a4abc77fb3ec),
        );
    }
    recorded_calculation(
        call(
            "NORM.S.DIST",
            vec![n(f64::from_bits(0xc026_a496_5d40_5589)), b(true)],
        ),
        f64::from_bits(0x39da_0ca4_2246_3668),
        f64::from_bits(0x39da0ca422463668),
    );
    recorded_calculation(
        call(
            "NORM.S.DIST",
            vec![n(f64::from_bits(0xc020_0893_4d02_54c4)), b(true)],
        ),
        f64::from_bits(0x3cc3_8f68_673d_153e),
        f64::from_bits(0x3cc38f68673d153e),
    );
    recorded_calculation(
        call(
            "NORM.S.DIST",
            vec![n(f64::from_bits(0xc016_a24f_cfb0_a98c)), b(true)],
        ),
        f64::from_bits(0x3e40_653f_58c2_faf3),
        f64::from_bits(0x3e40653f58c2faf3),
    );
    recorded_calculation(
        call(
            "NORM.S.DIST",
            vec![n(f64::from_bits(0xc016_aa4e_776d_04bd)), b(true)],
        ),
        f64::from_bits(0x3e3f_5501_63f6_fedc),
        f64::from_bits(0x3e3f550163f6fedc),
    );
    // Recorded oc10307 distinguishes the PC64 square in the residual.
    recorded_calculation(
        call("NORM.S.DIST", vec![n(-17.145244843201656), b(true)]),
        f64::from_bits(0x3256_fcaf_346e_9729),
        f64::from_bits(0x3256fcaf346e9729),
    );
    // The same staging reaches the normal wrappers (recorded oc2613).
    for function in ["NORM.DIST", "NORMDIST"] {
        recorded_calculation(
            call(
                function,
                vec![
                    n(-33.26358352947733),
                    n(-3.934034287489425),
                    n(3.7542580713919134),
                    b(true),
                ],
            ),
            f64::from_bits(0x3ce9_47e8_34b5_2568),
            f64::from_bits(0x3ce947e834b52568),
        );
    }
}

/// Original GAMMA answers from both former fitting halves, with known gaps.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_generic_body_records_known_gaps() {
    for (input, expected, calculated) in [
        (
            0x400d_1554_af11_39b0,
            0x400e_f676_6ccb_fa92,
            0x400ef6766ccbfa92,
        ),
        (
            0x400d_4f88_34b9_3ec3,
            0x400f_fe1e_2ca0_074c,
            0x400ffe1e2ca0074c,
        ),
        (
            0x4044_be47_5963_23ad,
            0x4a0b_1f25_9b34_f3ae,
            0x4a0b1f259b34f3ae,
        ),
        (
            0x405c_03e8_db5e_238e,
            0x6562_21fb_163a_ffa9,
            0x656221fb163affa9,
        ),
        // Positive tgamma refinement, from both grouped-argument halves.
        (
            0x3f21_ee1e_5eb2_9a49,
            0x40bc_8d91_582d_997d,
            0x40bc8d91582d997d,
        ),
        (
            0x3f1f_f366_0a69_738e,
            0x40c0_0605_98b3_6e3b,
            0x40c0060598b36e3a,
        ),
        // The same bound admits reduced reflection and the negative fallback.
        (
            0xc060_bad0_74be_eaa3,
            0x10c5_b33f_ccd8_3432,
            0x10c5b33fccd83432,
        ),
        (
            0xbfff_b384_bc86_8d32,
            0x403b_41c1_68e1_0426,
            0x403b41c168e10425,
        ),
        // Old matches differ from the rational by one ULP; keep their route.
        (
            0x400c_14f6_1d18_79ae,
            0x400a_e3a0_184f_4257,
            0x400ae3a0184f4255,
        ),
        (
            0x400d_7b7b_3b82_609e,
            0x4010_6635_56ba_cf83,
            0x4010663556bacf82,
        ),
        (
            0xc02d_5a3f_39dd_c52f,
            0xbd9e_116c_a682_f364,
            0xbd9e116ca682f364,
        ),
        // Negative inputs retain the larger bound.
        (
            0xbff1_1541_05b4_a44e,
            0x402c_e385_d327_12a8,
            0x402ce385d32712a8,
        ),
    ] {
        recorded_calculation(
            call("GAMMA", vec![n(f64::from_bits(input))]),
            f64::from_bits(expected),
            f64::from_bits(calculated),
        );
    }
}

/// Original density answers and current calculated returns; remaining prefix gaps stay visible.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_density_records_known_gaps() {
    for (x, alpha, expected, calculated) in [
        (
            0.12417919307712058,
            6.423680034363982,
            0x3e67_0863_f2ee_9357,
            0x3e670863f2ee9357,
        ),
        (
            0.8537609680018344,
            7.171703572068208,
            0x3f25_2274_a7b8_6ecb,
            0x3f252274a7b86ecb,
        ),
        (
            9.563436515180983,
            6.15888078874286,
            0x3faa_1da4_7b5c_d5ed,
            0x3faa1da47b5cd5ed,
        ),
        // Original normalizer-selection controls; the Excel answers remain unchanged.
        (
            0.0003421170430624811,
            0.943075920351938,
            0x3ff8_4f92_2b67_1613,
            0x3ff84f922b671613,
        ),
        (
            0.007825340264827977,
            0.622814265063597,
            0x4011_2efe_0987_a993,
            0x40112efe0987a993,
        ),
    ] {
        recorded_calculation(
            call("GAMMA.DIST", vec![n(x), n(alpha), n(1.0), b(false)]),
            f64::from_bits(expected),
            f64::from_bits(calculated),
        );
    }
}

/// Recorded GAMMA.DIST density row; the shape-one log normalizer is exactly zero.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_density_uses_the_exact_shape_one_normalizer() {
    exact(
        call(
            "GAMMA.DIST",
            vec![
                n(f64::from_bits(0x3f0b11ad0237a590)),
                n(1.0),
                n(f64::from_bits(0x3fce00f3acb82fad)),
                b(false),
            ],
        ),
        f64::from_bits(0x4011_0f90_28e4_bd60),
    );
}

/// Original R30 Value2 density and ratio-overflow readings (Excel build20527).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_density_multiplies_the_stored_shape_over_x_ratio() {
    // One unit high here: with Excel's own GAMMALN(1.375) in the mass the answer is exact, and
    // calc-func's GAMMALN below 1.5 is several units away.
    recorded_calculation(
        call("GAMMA.DIST", vec![n(0.75), n(0.375), n(1.0), b(false)]),
        f64::from_bits(0x3fce_880d_a95a_bce6),
        f64::from_bits(0x3fce_880d_a95a_bce7),
    );
    error(
        call(
            "GAMMA.DIST",
            vec![n(f64::MIN_POSITIVE), n(4.0), n(f64::MIN_POSITIVE), b(false)],
        ),
        Num,
    );
}

/// Recorded MOD tiny-result and subnormal-admission rows from W111.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mod_reports_a_subnormal_remainder_as_num() {
    // W111 discriminator 24222: the raw remainder underflows before sign adjustment.
    error(
        call(
            "MOD",
            vec![
                n(f64::from_bits(0x0014_c742_6ee3_166b)),
                n(f64::from_bits(0x8014_0ff4_c89d_5c9a)),
            ],
        ),
        Num,
    );
    error(
        call(
            "MOD",
            vec![
                n(f64::from_bits(0x0010_0000_0000_0000)),
                n(f64::from_bits(0x8010_0000_0000_0001)),
            ],
        ),
        Num,
    );
    error(call("MOD", vec![n(f64::from_bits(1)), n(0.0)]), Div0);
    exact(call("MOD", vec![n(f64::from_bits(1)), n(0.1)]), 0.0);
    // W111 tiny-00005 and refinement-00006/01014 separate both binary boundaries: a remainder
    // below 2^-1026 is added to the divisor, and one above it is taken from it.
    exact(
        call(
            "MOD",
            vec![
                n(f64::from_bits(0x0010_0000_0000_0001)),
                n(f64::from_bits(0x8010_0000_0000_0000)),
            ],
        ),
        f64::from_bits(0x8010_0000_0000_0001),
    );
    error(
        call(
            "MOD",
            vec![
                n(f64::from_bits(0x0011_0000_0000_0000)),
                n(f64::from_bits(0x8010_0000_0000_0000)),
            ],
        ),
        Num,
    );
    error(
        call(
            "MOD",
            vec![
                n(f64::from_bits(0x01b0_0000_0000_0001)),
                n(f64::from_bits(0x8010_0000_0000_0000)),
            ],
        ),
        Num,
    );
    // A binary-power divisor preserves a tiny intermediate with a normal final remainder.
    exact(
        call(
            "MOD",
            vec![
                n(f64::from_bits(0x0020800000000000)),
                n(f64::from_bits(0x8020000000000000)),
            ],
        ),
        f64::from_bits(0x801f_0000_0000_0000),
    );
}

/// R33 Excel20527 Value2 logarithm discriminants, with both public aliases.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gammaln_lower_band_uses_worksheet_log() {
    for (input, expected) in [
        (0x3f23_7410_0d8d_21f7, 0x4021_a17b_990b_2f65),
        (0x3eeb_cef4_c976_37f7, 0x4026_7624_1a3b_ff0f),
        (0x3fb3_e222_d4ab_0ead, 0x4004_1f39_682a_2fff),
        (0x3cb6_4fc2_de20_2148, 0x4041_db06_8858_611a),
        (0x3dfb_e7c7_e68d_a078, 0x4035_9fdd_d2a0_ae3e),
    ] {
        for function in ["GAMMALN", "GAMMALN.PRECISE"] {
            exact(
                call(function, vec![n(f64::from_bits(input))]),
                f64::from_bits(expected),
            );
        }
    }
}

/// R33 Excel20527 Value2 control: normalization and fourth-power sums are exact.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn kurt_stores_the_leading_coefficient_before_the_fourth_moment() {
    let values: Vec<_> = [0.0, 0.0, 0.0, -1.0, -2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0]
        .into_iter()
        .map(n)
        .collect();
    exact(call("AVERAGE", values.clone()), 0.0);
    exact(call("STDEV.S", values.clone()), 1.0);
    let expected = f64::from_bits(0x4000_8888_8888_8886);
    exact(call("KURT", values.clone()), expected);
    let mut sheet = Sheet::new();
    sheet.column(1, 1, values);
    let range = sheet.area(1, 1, 11, 1);
    exact(call_on(&mut sheet, "KURT", vec![range]), expected);
}

/// Recorded subnormal inputs reach the measured policies of these functions.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn rounding_and_log_gamma_handle_recorded_subnormal_inputs() {
    for bits in [1, 0x8000_0000_0000_0001] {
        for function in ["ROUND", "ROUNDUP", "ROUNDDOWN", "TRUNC"] {
            exact(call(function, vec![n(f64::from_bits(bits)), n(0.0)]), 0.0);
        }
        for function in ["GAMMALN", "GAMMALN.PRECISE"] {
            error(call(function, vec![n(f64::from_bits(bits))]), Num);
        }
    }
}

/// W111 zero-count Poisson exponential-boundary readings retain the exponential's bits.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn poisson_zero_count_retains_recorded_subnormal_probabilities() {
    for function in ["POISSON.DIST", "POISSON"] {
        for cumulative in [false, true] {
            exact(
                call(
                    function,
                    vec![
                        n(0.0),
                        n(f64::from_bits(0x4086_232b_dd7a_bcd3)),
                        b(cumulative),
                    ],
                ),
                f64::from_bits(0x000f_ffff_ffff_fe7c),
            );
        }
    }
}

/// One recorded semantic input, three count spellings; oc30003 is an old match.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn poisson_first_mass_tiny_mean_matches_recorded_probability() {
    let mean = f64::from_bits(0x3d70_0000_0000_0000);
    let expected = f64::from_bits(0x3d6f_ffff_ffff_e000);
    for function in ["POISSON.DIST", "POISSON"] {
        for count_bits in [
            0x3ff0_0000_0000_0000,
            0x3ff0_0000_0000_0001,
            0x3ffc_0000_0000_0000,
        ] {
            exact(
                call(
                    function,
                    vec![n(f64::from_bits(count_bits)), n(mean), b(false)],
                ),
                expected,
            );
        }
        exact(
            call(
                function,
                vec![n(1.0), n(f64::from_bits(0x3eb0_f2c7_411e_5587)), b(false)],
            ),
            f64::from_bits(0x3eb0_f2c6_21df_3991),
        );
        exact(call(function, vec![n(1.0), n(mean), b(true)]), 1.0);
    }
}

/// Excel 16.0.20527 live answers. The stored logarithm stays in the answer: a tiny mean does not
/// come back as itself.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn poisson_first_mass_tiny_mean_keeps_the_stored_logarithm() {
    for function in ["POISSON.DIST", "POISSON"] {
        for (mean_bits, answer_bits) in [
            (0x0010_0000_0000_0000u64, 0x0010_0000_0000_007cu64),
            (0x3af0_0000_0000_0000, 0x3af0_0000_0000_0000),
            (0x3c80_0000_0000_0000, 0x3c7f_ffff_ffff_ffe4),
            (0x3c8f_ffff_ffff_ffff, 0x3c8f_ffff_ffff_fff5),
            (0x3c90_0000_0000_0000, 0x3c8f_ffff_ffff_fff5),
        ] {
            exact(
                call(
                    function,
                    vec![n(1.0), n(f64::from_bits(mean_bits)), b(false)],
                ),
                f64::from_bits(answer_bits),
            );
        }
    }
}

/// Recorded cumulative gamma row: the final series product has an x87 store.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_cumulative_stores_the_final_series_product() {
    exact(
        call(
            "GAMMA.DIST",
            vec![
                n(f64::from_bits(0x3ee4e69846c122d8)),
                n(f64::from_bits(0x4000000000000000)),
                n(f64::from_bits(0x3ff0000000000000)),
                b(true),
            ],
        ),
        f64::from_bits(0x3dcb4d6c2d100466),
    );
}

/// Original catalogue dc96l3_discrete!D178: POISSON.DIST(33,33.3,TRUE).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn poisson_cumulative_retains_the_temme_correction() {
    for function in ["POISSON.DIST", "POISSON"] {
        exact(
            call(function, vec![n(33.0), n(33.3), b(true)]),
            f64::from_bits(0x3fe0_cfa4_9a24_03b8),
        );
    }
}

/// Fresh Excel20527 integer-gamma questions distinguish stored Horner from
/// strict binary64, divided Horner and forward term summation.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn integer_gamma_cumulative_uses_the_stored_factorial_polynomial() {
    for (count, mean_bits, q_bits) in [
        (7.0, 0x4031_f386_5d1f_64a2, 0x3f68_73f1_6c27_5115),
        (12.0, 0x403a_a514_dfac_1d68, 0x3f54_6b80_ba3c_b6bf),
        (12.0, 0x403b_ee7f_ee6b_86f4, 0x3f43_4258_9b45_3acf),
        (17.0, 0x4033_fafd_a9fa_64ea, 0x3fd3_1ae6_0f88_303d),
        (19.0, 0x4038_8815_3ea5_deb6, 0x3fc3_baf0_b382_440f),
    ] {
        let mean = f64::from_bits(mean_bits);
        let q = f64::from_bits(q_bits);
        for function in ["POISSON.DIST", "POISSON"] {
            exact(call(function, vec![n(count), n(mean), b(true)]), q);
        }
        exact(call("CHIDIST", vec![n(2.0 * mean), n(2.0 * (count + 1.0))]), q);
        for function in ["GAMMA.DIST", "GAMMADIST"] {
            exact(call(function, vec![n(mean), n(count + 1.0), n(1.0), b(true)]), 0.5 + (0.5 - q));
        }
    }
}

/// Recorded lower-series rows from both argument halves: all four now agree with Excel.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_cumulative_records_known_subtraction_gaps() {
    for (x, shape, expected, calculated) in [
        // oc820 (excluded half 1), oc837 (fitting half 0).
        (
            0x3e45_bfed_3f9f_5d2c,
            2.0,
            0x3c8d_90cd_01b5_dad1,
            0x3c8d90cd01b5dad1,
        ),
        (
            0x3e46_4f60_ae25_7bea,
            3.0,
            0x3acc_eb10_4395_7fa3,
            0x3acceb1043957fa3,
        ),
        // oc41045 / oc40295: unbounded correction would lose these matches.
        (
            0x3fb0_eec1_cb3a_8a9b,
            3.0,
            0x3f08_1056_fc2c_909a,
            0x3f081056fc2c909a,
        ),
        (
            0x3fb0_e0e8_3a02_c89a,
            3.0,
            0x3f07_d671_a72d_b3af,
            0x3f07d671a72db3af,
        ),
    ] {
        for function in ["GAMMA.DIST", "GAMMADIST"] {
            recorded_calculation(
                call(
                    function,
                    vec![n(f64::from_bits(x)), n(shape), n(1.0), b(true)],
                ),
                f64::from_bits(expected),
                f64::from_bits(calculated),
            );
        }
    }
}

/// Recorded oc3066 distinguishes the logarithm composed into the lognormal CDF.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn lognormal_uses_its_internal_logarithm() {
    let x = n(f64::from_bits(0x3fe2_57a6_22fc_740d));
    let expected = f64::from_bits(0x3fd2_7dc8_2b76_c882);
    exact(
        call("LOGNORM.DIST", vec![x.clone(), n(0.0), n(1.0), b(true)]),
        expected,
    );
    exact(call("LOGNORMDIST", vec![x, n(0.0), n(1.0)]), expected);
}

/// Build-20131 rows oc66883 and oc69092 distinguish the interior mass's logarithm.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn binomial_mass_uses_the_internal_normalization_logarithm() {
    for (k, p, expected) in [
        (2.0, 0x3ebd_5eba_c7b2_a5ce, 0x3e1d_88cb_f5d2_8843),
        (32.0, 0x3fef_ffe3_e30c_d521, 0x3e7b_0ce6_d3d6_58f5),
    ] {
        for function in ["BINOM.DIST", "BINOMDIST"] {
            exact(
                call(
                    function,
                    vec![n(k), n(34.0), n(f64::from_bits(p)), b(false)],
                ),
                f64::from_bits(expected),
            );
        }
    }
    // oc16921 distinguishes the asymptotic leading Stirling coefficient.
    for function in ["BINOM.DIST", "BINOMDIST"] {
        exact(
            call(
                function,
                vec![
                    n(3.0),
                    n(16.0),
                    n(f64::from_bits(0x3f81_4b09_a96b_5f50)),
                    b(false),
                ],
            ),
            f64::from_bits(0x3f33_c9f8_44fe_e7e4),
        );
    }
    // oc48161 distinguishes the initial deviance-series product store.
    for function in ["BINOM.DIST", "BINOMDIST"] {
        exact(
            call(
                function,
                vec![
                    n(3.0),
                    n(48.0),
                    n(f64::from_bits(0x3fb2_6666_6666_6763)),
                    b(false),
                ],
            ),
            f64::from_bits(0x3fcc_a757_9b2b_c297),
        );
    }
}

/// Original Excel build20527 R27 scalar readings distinguish strict from stored PC64 n*LN(p).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn binomial_all_success_stores_the_x87_log_product() {
    for (trials, probability, expected) in [
        (1_048_573.0, 0x3fef_ffdf_ffff_ff90, 0x3e7e_34c4_aa8e_1c9c),
        (1_048_579.0, 0x3fef_ffdf_ffff_ff90, 0x3e7e_340f_6fb7_3015),
        (16_777_215.0, 0x3fef_fffd_ffff_fc10, 0x3e7e_354a_cc09_8faf),
        (16_777_217.0, 0x3fef_fffd_ffff_fc10, 0x3e7e_3547_0560_4d12),
        (16_777_213.0, 0x3fef_fffe_0000_0030, 0x3e7e_3552_7792_df08),
        (16_777_219.0, 0x3fef_fffe_0000_0030, 0x3e7e_3547_2395_b87f),
        (268_435_453.0, 0x3fef_ffff_dfff_fc00, 0x3e7e_351e_b969_add5),
        (268_435_459.0, 0x3fef_ffff_dfff_fc00, 0x3e7e_351e_042a_e0b8),
        (268_435_455.0, 0x3fef_ffff_dfff_fc20, 0x3e7e_3520_6051_61a3),
        (268_435_457.0, 0x3fef_ffff_dfff_fc20, 0x3e7e_3520_23e7_19ce),
    ] {
        for function in ["BINOM.DIST", "BINOMDIST"] {
            exact(
                call(
                    function,
                    vec![
                        n(trials),
                        n(trials),
                        n(f64::from_bits(probability)),
                        b(false),
                    ],
                ),
                f64::from_bits(expected),
            );
        }
    }
}

/// Original R28 Excel b20527 count edges, including both probability endpoints.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn binomial_rejects_the_signed_count_sentinel_before_probability_shortcuts() {
    let first_rejected = f64::from_bits(0x41df_ffff_ffc0_0000);
    let last_accepted = f64::from_bits(0x41df_ffff_ffbf_ffff);
    for function in ["BINOM.DIST", "BINOMDIST"] {
        for probability in [0.0, 1.0, 1.0 - 2.0_f64.powi(-24)] {
            for cumulative in [false, true] {
                for trials in [first_rejected, first_rejected + 0.5, 1_099_511_627_776.0] {
                    error(
                        call(
                            function,
                            vec![n(trials), n(trials), n(probability), b(cumulative)],
                        ),
                        Num,
                    );
                    error(
                        call(
                            function,
                            vec![n(0.0), n(trials), n(probability), b(cumulative)],
                        ),
                        Num,
                    );
                }
            }
        }
        // The immediately preceding binary64 count truncates to 2^31-2.
        exact(
            call(
                function,
                vec![n(last_accepted), n(last_accepted), n(1.0), b(false)],
            ),
            1.0,
        );
        exact(
            call(
                function,
                vec![n(last_accepted), n(last_accepted), n(0.0), b(false)],
            ),
            0.0,
        );
        exact(
            call(
                function,
                vec![
                    n(last_accepted),
                    n(last_accepted),
                    n(1.0 - 2.0_f64.powi(-24)),
                    b(false),
                ],
            ),
            f64::from_bits(0x3464_2eb5_104a_866b),
        );
    }
}

/// Recorded tiny ERF answers: the stored-product route below 2^-27 (dc-j9k.6) reproduces them.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn tiny_erf_matches_recorded_answers() {
    for (input, expected) in [
        (0x3d70_5e80_0000_0000, 0x3d72_7879_0ff4_a4d4),
        (0x3d73_5c29_021a_4811, 0x3d75_d86d_8b33_b7f4),
    ] {
        exact(
            call("ERF.PRECISE", vec![n(f64::from_bits(input))]),
            f64::from_bits(expected),
        );
        exact(
            call("ERF", vec![n(f64::from_bits(input))]),
            f64::from_bits(expected),
        );
    }
    // gauss-discovery-00059: the tiny wrapper uses the same ERF calculation.
    exact(
        call("GAUSS", vec![n(f64::from_bits(0x005f_e189_b32b_8147))]),
        f64::from_bits(0x0049_6ff7_0f27_3fff),
    );
}

/// Recorded b17-s20-03582 and b16-g25-01840: the linear route below 2^-16 (dc-j9k.6).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn small_erf_matches_recorded_answers() {
    for (input, expected) in [
        (0x3eb4_5231_7b1d_dc40, 0x3eb6_ee0b_e91e_09f3),
        (0x3e61_cc00_0000_0000, 0x3e64_14e5_3e86_1661),
    ] {
        for function in ["ERF.PRECISE", "ERF"] {
            exact(
                call(function, vec![n(f64::from_bits(input))]),
                f64::from_bits(expected),
            );
        }
    }
}

/// Recorded oc1302 and smallz-0018: the (4, 5) stored rational below 0.5 (dc-j9k.6).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn small_erf_matches_recorded_curvature_rows() {
    for (input, expected) in [
        (0x3f1c_0000_0000_0000, 0x3f1f_9838_ca70_8460),
        (0x3f83_0000_0000_0000, 0x3f85_7047_602b_fba1),
    ] {
        for function in ["ERF.PRECISE", "ERF"] {
            exact(
                call(function, vec![n(f64::from_bits(input))]),
                f64::from_bits(expected),
            );
        }
    }
}

/// R30 Excel 16.0.20527 positive-ERF and negative-ERFC readings at two independently selected
/// complement ties, plus a negative-ERF control from the separately measured odd wrapper.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn erf_complements_keep_the_recorded_pc64_store() {
    for function in ["ERF", "ERF.PRECISE"] {
        exact(
            call(function, vec![n(f64::from_bits(0x4017_afb4_8dc9_65a6))]),
            1.0,
        );
        exact(
            call(function, vec![n(-f64::from_bits(0x4017_afb4_8dc9_65a6))]),
            -1.0,
        );
    }
    for function in ["ERFC", "ERFC.PRECISE"] {
        exact(
            call(function, vec![n(-f64::from_bits(0x4017_744f_8f74_e8ca))]),
            2.0,
        );
    }
}

/// Recorded beta rows oc1582/oc640 and adjacent FDIST refinement inputs.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_and_f_tails_keep_the_recorded_arithmetic_stores() {
    for (x, alpha, beta, expected) in [
        (
            0x3fd2_d8a7_e6f4_345c,
            0x4000_0000_0000_0000,
            0x4014_0000_0000_0000,
            0x3fe2_2b76_1604_a6c4,
        ),
        (
            0x3fef_8f5e_328d_786c,
            0x4049_84b8_81a3_be4a,
            0x3fa9_9999_9999_999a,
            0x3f93_d7f8_5447_ca8d,
        ),
    ] {
        exact(
            call(
                "BETA.DIST",
                vec![
                    n(f64::from_bits(x)),
                    n(f64::from_bits(alpha)),
                    n(f64::from_bits(beta)),
                    b(true),
                ],
            ),
            f64::from_bits(expected),
        );
    }
    for x in [0x3ff3_72a3_6987_efc6, 0x3ff3_72a3_6987_efc7] {
        exact(
            call("FDIST", vec![n(f64::from_bits(x)), n(5.0), n(6.0)]),
            f64::from_bits(0x3fd9_d035_ed43_a9d5),
        );
    }
}

/// Recorded oc162/oc2552 distinguish the carries; fcdf-discovery-162 rejects an ALNREL carry.
/// oc162, BETA.DIST(0.75, 50, 0.2, TRUE) = 0x3e50c82c2ab0a678 in Excel, left this test when bgrat
/// moved to `eps` (dc-ymjf.6): it now returns 2 units above, one of 8 BETA.DIST rows lost against
/// 29 gained; recorded for the next round of dc-ymjf.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_normalizer_and_fraction_carry_selected_products_into_the_sum() {
    exact(
        call(
            "BETA.DIST",
            vec![
                n(f64::from_bits(0x3ffb_33f9_cca7_6a46)),
                n(f64::from_bits(0x400d_2340_d24a_6dd6)),
                n(f64::from_bits(0x4010_8821_4494_de74)),
                b(true),
                n(f64::from_bits(0xbfec_78fe_89bf_57d0)),
                n(f64::from_bits(0x4010_2ad2_a17e_4f43)),
            ],
        ),
        f64::from_bits(0x3fe4_0a1a_47f6_ea10),
    );
    exact(
        call(
            "F.DIST",
            vec![
                n(f64::from_bits(0x3ff5_3020_c3d3_d95f)),
                n(4.0),
                n(11.0),
                b(true),
            ],
        ),
        f64::from_bits(0x3fe5_bb90_637b_4cd0),
    );
    // oc2325 distinguishes the earlier Horner product in GAMln1's small-a arm.
    exact(
        call(
            "BETA.DIST",
            vec![
                n(f64::from_bits(0xbfe4_e784_cb53_6e3c)),
                n(f64::from_bits(0x3ff4_5e5f_705d_d315)),
                n(f64::from_bits(0x3ff3_9d61_280e_fe34)),
                b(true),
                n(f64::from_bits(0xbfff_d2b1_1995_df92)),
                n(f64::from_bits(0x4006_c206_44e9_7df9)),
            ],
        ),
        f64::from_bits(0x3fce_646a_71b7_08b3),
    );
    // oc114: the ALNREL boundary includes magnitude 0.375.
    exact(
        call("BETA.DIST", vec![n(0.375), n(3.0), n(7.0), b(true)]),
        f64::from_bits(0x3fe6_fc1b_b000_0000),
    );
    // oc58 and fcdf-retired_wrapper-022 distinguish beta parameter 2.
    exact(
        call("FDIST", vec![n(2.7), n(8.0), n(4.0)]),
        f64::from_bits(0x3fc6_94c2_ffff_fffe),
    );
    exact(
        call("F.DIST", vec![n(4.0), n(4.0), n(8.0), b(true)]),
        f64::from_bits(0x3fee_8d2b_3183_aff0),
    );
}

/// Original catalogue answers, including every recorded selection of both fits.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn linest_stored_reflector_matches_original_excel_selections() {
    {
        // corpus:calc-func_catalogue_oracle.xlsx:dcj9k_linest_stats:1091:4 and the other original selections of this fit.
        let result = call(
            "LINEST",
            vec![
                arr(vec![
                    vec![-17.979],
                    vec![-0.126],
                    vec![8.0],
                    vec![-5.0],
                    vec![1.7],
                    vec![37.0],
                    vec![12.4],
                    vec![14.46],
                    vec![-8.0],
                    vec![24.2],
                ]),
                arr(vec![
                    vec![20.8, 50.0],
                    vec![34.0, 23.41],
                    vec![38.875, -17.7],
                    vec![4.863, 36.9],
                    vec![42.1, 6.0],
                    vec![-14.55, 44.41],
                    vec![-4.57, -12.96],
                    vec![-9.909, 2.589],
                    vec![47.549, 42.0],
                    vec![54.9, -17.63],
                ]),
                b(true),
                b(true),
            ],
        )
        .expect("LINEST array");
        let values = result.as_array().expect("LINEST array");
        for (row, column, expected) in [
            (1, 1, "0xbfd27fdb7ef796a7"),
            (1, 2, "0xbfd1a33c9c5095e3"),
            (1, 3, "0x40311a7710d73964"),
            (2, 1, "0x3fc863a97e13b089"),
            (2, 2, "0x3fc9963b81442b03"),
            (2, 3, "0x401dc5ac009d3792"),
            (3, 1, "0x3fd5207539f192f3"),
            (3, 2, "0x402e02041aa086c9"),
            (3, 3, "error:NA"),
            (4, 1, "0x3ffb98672c7a3bc3"),
            (4, 2, "0x401c000000000000"),
            (4, 3, "error:NA"),
            (5, 1, "0x4088443548feaaee"),
            (5, 2, "0x40989f4ed822bc97"),
            (5, 3, "error:NA"),
        ] {
            let actual = values.get(row - 1, column - 1).expect("recorded selection");
            assert_eq!(
                calc_verdict::encode_result(actual).as_deref(),
                Some(expected),
                "row={row} column={column}"
            );
        }
    }
    {
        // corpus:calc-func_catalogue_oracle.xlsx:dcj9k_linest_stats:1176:4 and the other original selections of this fit.
        let result = call(
            "LINEST",
            vec![
                arr(vec![vec![31.78], vec![5.61], vec![-9.09]]),
                arr(vec![
                    vec![53.56, 30.22, 32.0],
                    vec![-9.0, -17.977, 47.17],
                    vec![9.4, -11.83, 39.39],
                ]),
                b(false),
                b(true),
            ],
        )
        .expect("LINEST array");
        let values = result.as_array().expect("LINEST array");
        for (row, column, expected) in [
            (1, 1, "0x3fe9f2f302983561"),
            (1, 2, "0x4003a2d805c5bf11"),
            (1, 3, "0xbff46aa36a83eafe"),
            (1, 4, "0x0000000000000000"),
            (2, 1, "0x0000000000000000"),
            (2, 2, "0x0000000000000000"),
            (2, 3, "0x0000000000000000"),
            (2, 4, "error:NA"),
            (3, 1, "0x3ff0000000000000"),
            (3, 2, "0x0000000000000000"),
            (3, 3, "error:NA"),
            (3, 4, "error:NA"),
            (4, 1, "error:Num"),
            (4, 2, "0x0000000000000000"),
            (4, 3, "error:NA"),
            (4, 4, "error:NA"),
            (5, 1, "0x409190463f141206"),
            (5, 2, "0x0000000000000000"),
            (5, 3, "error:NA"),
            (5, 4, "error:NA"),
        ] {
            let actual = values.get(row - 1, column - 1).expect("recorded selection");
            assert_eq!(
                calc_verdict::encode_result(actual).as_deref(),
                Some(expected),
                "row={row} column={column}"
            );
        }
    }
}

/// Original R25 full-rank coefficient readings separate the squared-norm product
/// from the reflector. Every selected input cell has its exact operand echo.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn linest_stores_squared_norm_products() {
    let edge = f64::from_bits(0x401f9ff8b1ae9622);
    for (response, predictors, column, expected) in [
        // r25-qr-store-00009
        (
            vec![
                f64::from_bits(0x3ff0000000000000),
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x0000000000000000),
            ],
            vec![
                vec![edge],
                vec![f64::from_bits(0x0000000000000000)],
                vec![f64::from_bits(0x0000000000000000)],
                vec![f64::from_bits(0x3fe0000000000000)],
            ],
            1,
            0x3fc02012dd5f4df4_u64,
        ),
        // r25-qr-store-00026
        (
            vec![
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x3ff0000000000000),
            ],
            vec![
                vec![edge],
                vec![f64::from_bits(0x0000000000000000)],
                vec![f64::from_bits(0x0000000000000000)],
                vec![f64::from_bits(0x3fe0000000000000)],
            ],
            1,
            0x3f805109b424e6d5_u64,
        ),
        // r25-qr-store-00032
        (
            vec![
                f64::from_bits(0x3ff0000000000000),
                f64::from_bits(0x4000000000000000),
                f64::from_bits(0x4010000000000000),
                f64::from_bits(0x401c000000000000),
            ],
            vec![
                vec![edge],
                vec![f64::from_bits(0x0000000000000000)],
                vec![f64::from_bits(0x0000000000000000)],
                vec![f64::from_bits(0x3fe0000000000000)],
            ],
            1,
            0x3fc743871c2f72ef_u64,
        ),
        // r25-qr-store-00063
        (
            vec![
                f64::from_bits(0x3ff0000000000000),
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x0000000000000000),
            ],
            vec![
                vec![edge, f64::from_bits(0x0000000000000000)],
                vec![f64::from_bits(0x0000000000000000), edge],
                vec![
                    f64::from_bits(0x0000000000000000),
                    f64::from_bits(0x3fe0000000000000),
                ],
                vec![
                    f64::from_bits(0x3fe0000000000000),
                    f64::from_bits(0x0000000000000000),
                ],
            ],
            2,
            0x3fc02012dd5f4df4_u64,
        ),
        // r25-qr-store-00067
        (
            vec![
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x3ff0000000000000),
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x0000000000000000),
            ],
            vec![
                vec![edge, f64::from_bits(0x0000000000000000)],
                vec![f64::from_bits(0x0000000000000000), edge],
                vec![
                    f64::from_bits(0x0000000000000000),
                    f64::from_bits(0x3fe0000000000000),
                ],
                vec![
                    f64::from_bits(0x3fe0000000000000),
                    f64::from_bits(0x0000000000000000),
                ],
            ],
            1,
            0x3fc02012dd5f4df4_u64,
        ),
        // r25-qr-store-00070
        (
            vec![
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x3ff0000000000000),
                f64::from_bits(0x0000000000000000),
            ],
            vec![
                vec![edge, f64::from_bits(0x0000000000000000)],
                vec![f64::from_bits(0x0000000000000000), edge],
                vec![
                    f64::from_bits(0x0000000000000000),
                    f64::from_bits(0x3fe0000000000000),
                ],
                vec![
                    f64::from_bits(0x3fe0000000000000),
                    f64::from_bits(0x0000000000000000),
                ],
            ],
            1,
            0x3f805109b424e6d5_u64,
        ),
        // r25-qr-store-00075
        (
            vec![
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x0000000000000000),
                f64::from_bits(0x3ff0000000000000),
            ],
            vec![
                vec![edge, f64::from_bits(0x0000000000000000)],
                vec![f64::from_bits(0x0000000000000000), edge],
                vec![
                    f64::from_bits(0x0000000000000000),
                    f64::from_bits(0x3fe0000000000000),
                ],
                vec![
                    f64::from_bits(0x3fe0000000000000),
                    f64::from_bits(0x0000000000000000),
                ],
            ],
            2,
            0x3f805109b424e6d5_u64,
        ),
        // r25-qr-store-00082
        (
            vec![
                f64::from_bits(0x3ff0000000000000),
                f64::from_bits(0x4000000000000000),
                f64::from_bits(0x4010000000000000),
                f64::from_bits(0x401c000000000000),
            ],
            vec![
                vec![edge, f64::from_bits(0x0000000000000000)],
                vec![f64::from_bits(0x0000000000000000), edge],
                vec![
                    f64::from_bits(0x0000000000000000),
                    f64::from_bits(0x3fe0000000000000),
                ],
                vec![
                    f64::from_bits(0x3fe0000000000000),
                    f64::from_bits(0x0000000000000000),
                ],
            ],
            1,
            0x3fd22a3413e3eace_u64,
        ),
        // r25-qr-store-00083
        (
            vec![
                f64::from_bits(0x3ff0000000000000),
                f64::from_bits(0x4000000000000000),
                f64::from_bits(0x4010000000000000),
                f64::from_bits(0x401c000000000000),
            ],
            vec![
                vec![edge, f64::from_bits(0x0000000000000000)],
                vec![f64::from_bits(0x0000000000000000), edge],
                vec![
                    f64::from_bits(0x0000000000000000),
                    f64::from_bits(0x3fe0000000000000),
                ],
                vec![
                    f64::from_bits(0x3fe0000000000000),
                    f64::from_bits(0x0000000000000000),
                ],
            ],
            2,
            0x3fc743871c2f72ef_u64,
        ),
    ] {
        let result = call(
            "LINEST",
            vec![
                arr(response.into_iter().map(|x| vec![x]).collect()),
                arr(predictors),
                b(false),
                b(true),
            ],
        )
        .expect("LINEST result");
        let values = result.as_array().expect("LINEST array");
        let selected = values.get(0, column - 1).expect("recorded coefficient");
        assert_eq!(
            calc_verdict::encode_result(selected).as_deref(),
            Some(format!("0x{expected:016x}").as_str())
        );
    }
}

/// Excel20527 R37: zero-mean predictors isolate the intercept error association.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn intercept_error_multiplies_residual_by_variance_before_dividing() {
    for (name, first_y, expected) in [
        ("LINEST", 1.0, 0x3fc4e6fdf33cf032_u64),
        ("LOGEST", 2.0, 0x3fbcfa0cce808936_u64),
    ] {
        let tail_y = if name == "LINEST" { 0.0 } else { 1.0 };
        let y = arr(vec![
            vec![first_y],
            vec![tail_y],
            vec![tail_y],
            vec![tail_y],
            vec![tail_y],
        ]);
        let scale = 2.0_f64.powi(-20);
        let x = arr([-4.0, -2.0, 0.0, 2.0, 4.0]
            .into_iter()
            .map(|v| vec![v * scale])
            .collect());
        let value = call(name, vec![y, x, b(true), b(true)]).expect("regression statistics");
        let values = value.as_array().expect("five-row regression result");
        assert_eq!(
            values.row_slice(1).unwrap()[1]
                .as_number()
                .unwrap()
                .to_bits(),
            expected
        );
    }
}

/// Original Excel20527 R39 selections, including a corrected trend state.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_nonseasonal_innovations_and_corrected_trends() {
    let times = arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]]);
    {
        let y = arr(vec![vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.625]]);
        exact(
            call(
                "FORECAST.ETS",
                vec![n(8.0), y.clone(), times.clone(), n(0.0)],
            ),
            f64::from_bits(0x3fe43a6c19723d44),
        );
        exact(
            call(
                "FORECAST.ETS",
                vec![n(9.0), y.clone(), times.clone(), n(0.0)],
            ),
            f64::from_bits(0x3fe7c7f0d0638720),
        );
        exact(
            call(
                "FORECAST.ETS",
                vec![n(10.0), y.clone(), times.clone(), n(0.0)],
            ),
            f64::from_bits(0x3feb55758754d0f9),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(1.0), n(0.0)],
            ),
            f64::from_bits(0x3fd0000000000000),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(2.0), n(0.0)],
            ),
            f64::from_bits(0x3f50624dd2f1a9fc),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(3.0), n(0.0)],
            ),
            f64::from_bits(0x3cb0000000000000),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(4.0), n(0.0)],
            ),
            f64::from_bits(0x3ff2dd19066405d9),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(5.0), n(0.0)],
            ),
            f64::from_bits(0x3ff5e480654ae576),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(6.0), n(0.0)],
            ),
            f64::from_bits(0x3fcda4950a0ae49e),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(7.0), n(0.0)],
            ),
            f64::from_bits(0x3fd34f56945aa88d),
        );
        exact(
            call(
                "FORECAST.ETS.CONFINT",
                vec![n(9.0), y, times.clone(), n(0.95), n(0.0)],
            ),
            f64::from_bits(0x3fe2ec61e6342aab),
        );
    }
    {
        let y = arr(vec![vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.5]]);
        exact(
            call(
                "FORECAST.ETS",
                vec![n(8.0), y.clone(), times.clone(), n(0.0)],
            ),
            f64::from_bits(0x3ff75e17379c910c),
        );
        exact(
            call(
                "FORECAST.ETS",
                vec![n(9.0), y.clone(), times.clone(), n(0.0)],
            ),
            f64::from_bits(0x3ffa52aed0455fdb),
        );
        exact(
            call(
                "FORECAST.ETS",
                vec![n(10.0), y.clone(), times.clone(), n(0.0)],
            ),
            f64::from_bits(0x3ffd474668ee2eaa),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(1.0), n(0.0)],
            ),
            f64::from_bits(0x3feccccccccccccd),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(2.0), n(0.0)],
            ),
            f64::from_bits(0x3f50624dd2f1a9fc),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(3.0), n(0.0)],
            ),
            f64::from_bits(0x3cb0000000000000),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(4.0), n(0.0)],
            ),
            f64::from_bits(0x3ff474806b767d8f),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(5.0), n(0.0)],
            ),
            f64::from_bits(0x3ff7407b4f71fbbe),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(6.0), n(0.0)],
            ),
            f64::from_bits(0x3fd1886e12f7d956),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(7.0), n(0.0)],
            ),
            f64::from_bits(0x3fd684abe5b0b1ae),
        );
        exact(
            call(
                "FORECAST.ETS.CONFINT",
                vec![n(9.0), y, times.clone(), n(0.95), n(0.0)],
            ),
            f64::from_bits(0x3fe61146344afb6b),
        );
    }
    {
        let y = arr(vec![vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0]]);
        exact(
            call(
                "FORECAST.ETS",
                vec![n(8.0), y.clone(), times.clone(), n(0.0)],
            ),
            f64::from_bits(0x3fffffffffffffff),
        );
        exact(
            call(
                "FORECAST.ETS",
                vec![n(9.0), y.clone(), times.clone(), n(0.0)],
            ),
            f64::from_bits(0x4001cf3cf3cf3cf4),
        );
        exact(
            call(
                "FORECAST.ETS",
                vec![n(10.0), y.clone(), times.clone(), n(0.0)],
            ),
            f64::from_bits(0x40039e79e79e79e8),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(1.0), n(0.0)],
            ),
            f64::from_bits(0x3fefef9db22d0e56),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(2.0), n(0.0)],
            ),
            f64::from_bits(0x3f50624dd2f1a9fc),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(3.0), n(0.0)],
            ),
            f64::from_bits(0x3cb0000000000000),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(4.0), n(0.0)],
            ),
            f64::from_bits(0x3fe2f6e3346912e2),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(5.0), n(0.0)],
            ),
            f64::from_bits(0x3ff81b1021607a26),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(6.0), n(0.0)],
            ),
            f64::from_bits(0x3fc5ac7160781594),
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(7.0), n(0.0)],
            ),
            f64::from_bits(0x3fd75260983e90d1),
        );
        exact(
            call(
                "FORECAST.ETS.CONFINT",
                vec![n(9.0), y, times.clone(), n(0.95), n(0.0)],
            ),
            f64::from_bits(0x3fe6dadcbd4fb13f),
        );
    }
}

/// The odd-length correction window includes seven transitions at length 15.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_odd_length_correction_uses_the_final_half_rounded_up() {
    let y = arr(vec![vec![
        f64::from_bits(0x533921fb54442d0b),
        f64::from_bits(0x533921fb54442d15),
        f64::from_bits(0x533921fb54442d25),
        f64::from_bits(0x533921fb54442d23),
        f64::from_bits(0x533921fb54442d25),
        f64::from_bits(0x533921fb54442d23),
        f64::from_bits(0x533921fb54442d17),
        f64::from_bits(0x533921fb54442d18),
        f64::from_bits(0x533921fb54442d12),
        f64::from_bits(0x533921fb54442d18),
        f64::from_bits(0x533921fb54442d0f),
        f64::from_bits(0x533921fb54442d0a),
        f64::from_bits(0x533921fb54442d16),
        f64::from_bits(0x533921fb54442d10),
        f64::from_bits(0x533921fb54442d26),
    ]]);
    let times = arr(vec![(1..=15).map(|i| i as f64).collect()]);
    exact(
        call("FORECAST.ETS", vec![n(20.0), y, times, n(0.0)]),
        f64::from_bits(0x533921fb54442d24),
    );
}

/// A valid confidence can round the internal probability to one.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_confidence_rounded_probability_keeps_the_measured_fallback() {
    let y = arr(vec![vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0]]);
    let times = arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]]);
    exact(
        call(
            "FORECAST.ETS.CONFINT",
            vec![
                n(9.0),
                y,
                times,
                n(f64::from_bits(0x3fef_ffff_ffff_ffff)),
                n(0.0),
            ],
        ),
        f64::from_bits(0x3fd75260983e90d1),
    );
}

/// Odd-history raw moments retain a different divisor from their selected tail.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_raw_moments_preserve_symmetric_history_forecasts_and_statistics() {
    let y = arr(vec![vec![
        -2.0, 8.0, -13.0, 10.0, -14.0, 12.0, -17.0, -11.0, -29.0, 27.0, 30.0, 27.0, -29.0, -11.0,
        -17.0, 12.0, -14.0, 10.0, -13.0, 8.0, -2.0,
    ]]);
    let times = arr(vec![(1..=21).map(|i| i as f64).collect()]);
    for (horizon, expected) in [
        (0.0, 0xbffd7243d1b598bb),
        (1.0, 0xbffd3deb3a1235de),
        (2.0, 0xbffd0992a26ed4a5),
        (5.0, 0xbffc6c88db84adb1),
    ] {
        exact(
            call(
                "FORECAST.ETS",
                vec![n(21.0 + horizon), y.clone(), times.clone(), n(0.0)],
            ),
            f64::from_bits(expected),
        );
    }
    exact(
        call(
            "FORECAST.ETS.STAT",
            vec![y.clone(), times.clone(), n(1.0), n(0.0)],
        ),
        0.002,
    );
    exact(
        call("FORECAST.ETS.STAT", vec![y, times, n(6.0), n(0.0)]),
        f64::from_bits(0x4024d679eaf575de),
    );
    let short = arr(vec![vec![1.0, 2.0, 1.0]]);
    exact(
        call(
            "FORECAST.ETS.STAT",
            vec![short, arr(vec![vec![1.0, 2.0, 3.0]]), n(3.0), n(0.0)],
        ),
        f64::EPSILON,
    );
    let rounded = arr(vec![vec![f64::from_bits(0x3ff4cccccccccccc); 16]]);
    let times = arr(vec![(1..=16).map(|i| i as f64).collect()]);
    exact(
        call(
            "FORECAST.ETS.STAT",
            vec![rounded.clone(), times.clone(), n(3.0), n(0.0)],
        ),
        f64::EPSILON,
    );
    assert!(matches!(
        call("FORECAST.ETS", vec![n(17.0), rounded, times, n(0.0)]),
        Err(calc_func::FunctionFault::NotHeld {
            owner: "dc-j9k",
            ..
        })
    ));
    for values in [
        vec![1e308, -1e308, 1e308],
        vec![2.0_f64.powi(-401), 2.0_f64.powi(-400), 0.0],
    ] {
        assert!(matches!(
            call(
                "FORECAST.ETS.STAT",
                vec![
                    arr(vec![values]),
                    arr(vec![vec![1.0, 2.0, 3.0]]),
                    n(3.0),
                    n(0.0)
                ]
            ),
            Err(calc_func::FunctionFault::NotHeld {
                owner: "dc-j9k",
                ..
            })
        ));
    }
}

/// Excel20527 R46: lag two becomes available after four prepared observations.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_short_seasonality_reports_zero_before_the_first_candidate_lag() {
    for exponent in [-400, 0, 400] {
        let scale = 2.0_f64.powi(exponent);
        for values in [
            vec![scale, -scale],
            vec![scale, -scale, scale],
            vec![scale, -scale, scale, -scale],
            vec![0.0, scale, 0.0, 0.0],
        ] {
            let times = arr(vec![(1..=values.len()).map(|j| j as f64).collect()]);
            exact(
                call("FORECAST.ETS.SEASONALITY", vec![arr(vec![values]), times]),
                0.0,
            );
        }
    }
    // R48's exact alternating region starts at five observations.
    exact(
        call(
            "FORECAST.ETS.SEASONALITY",
            vec![
                arr(vec![vec![1.0, -1.0, 1.0, -1.0, 1.0]]),
                arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]),
            ],
        ),
        2.0,
    );
}

/// Excel20527 R48: exact alternating states continue through partial cycles.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_alternating_affine_fits_start_at_five_and_keep_partial_cycles() {
    for length in 5..=32 {
        let times = arr(vec![(1..=length).map(|j| j as f64).collect()]);
        for exponent in [-400, 0, 400] {
            let scale = 2.0_f64.powi(exponent);
            let y = arr(vec![
                (0..length)
                    .map(|j| if j % 2 == 0 { 0.0 } else { scale })
                    .collect(),
            ]);
            exact(
                call("FORECAST.ETS.SEASONALITY", vec![y.clone(), times.clone()]),
                2.0,
            );
            for season in [1.0, 2.0] {
                for horizon in [0, 1, 2, 5] {
                    let result = call(
                        "FORECAST.ETS",
                        vec![
                            n((length + horizon) as f64),
                            y.clone(),
                            times.clone(),
                            n(season),
                        ],
                    );
                    if length % 2 != 0 || (season == 1.0 && length < 8) {
                        assert!(matches!(
                            result,
                            Err(calc_func::FunctionFault::NotHeld {
                                owner: "dc-j9k",
                                ..
                            })
                        ));
                    } else {
                        let expected = if (length - 1 + horizon) % 2 == 0 {
                            0.0
                        } else {
                            scale
                        };
                        exact(result, expected);
                    }
                }
                for (stat, expected) in [
                    (1.0, 0.751),
                    (2.0, 0.001),
                    (3.0, 0.001),
                    (4.0, 0.0),
                    (5.0, 0.0),
                    (6.0, 0.0),
                    (7.0, 0.0),
                ] {
                    exact(
                        call(
                            "FORECAST.ETS.STAT",
                            vec![y.clone(), times.clone(), n(stat), n(season)],
                        ),
                        expected,
                    );
                }
                exact(
                    call(
                        "FORECAST.ETS.CONFINT",
                        vec![
                            n((length + 5) as f64),
                            y.clone(),
                            times.clone(),
                            n(0.95),
                            n(season),
                        ],
                    ),
                    0.0,
                );
            }
        }
    }
    // A nonzero innovation still needs the unidentified search and initializer.
    assert!(matches!(
        call(
            "FORECAST.ETS",
            vec![
                n(6.0),
                arr(vec![vec![0.0, 1.0, 0.0, 1.0, 0.125]]),
                arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]),
                n(2.0)
            ]
        ),
        Err(calc_func::FunctionFault::NotHeld {
            owner: "dc-j9k",
            ..
        })
    ));
    // Preserve the four fresh Excel-zero answers that stopped the forecast
    // extension. The unproved publication must stay a counted refusal.
    for (offset, drift, amplitude) in [
        (1099511627776.0, -274877906944.0, -1.0),
        (1099511627776.0, -274877906944.0, 1.0),
        (-1099511627776.0, 274877906944.0, -1.0),
        (-1099511627776.0, 274877906944.0, 1.0),
    ] {
        let scale = 2.0_f64.powi(-400);
        let values = (0..5)
            .map(|j| {
                (offset + if j % 2 == 0 { amplitude } else { -amplitude } + drift * j as f64)
                    * scale
            })
            .collect();
        assert!(matches!(
            call(
                "FORECAST.ETS",
                vec![
                    n(5.0),
                    arr(vec![values]),
                    arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]),
                    n(1.0)
                ]
            ),
            Err(calc_func::FunctionFault::NotHeld {
                owner: "dc-j9k",
                ..
            })
        ));
    }
}

/// Excel20527 R47: the explicit two-period request at four observations uses
/// the nonseasonal fit, including its reported smoothing parameters.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_short_period_requests_use_the_nonseasonal_fit() {
    let times = arr(vec![vec![1.0, 2.0, 3.0, 4.0]]);
    for values in [vec![5.0; 4], vec![2.0, 5.0, 8.0, 11.0]] {
        let expected = values[3] + 2.0 * (values[1] - values[0]);
        let y = arr(vec![values]);
        for season in [0.0, 1.0, 2.0] {
            exact(
                call(
                    "FORECAST.ETS",
                    vec![n(6.0), y.clone(), times.clone(), n(season)],
                ),
                expected,
            );
            exact(
                call(
                    "FORECAST.ETS.CONFINT",
                    vec![n(6.0), y.clone(), times.clone(), n(0.95), n(season)],
                ),
                0.0,
            );
            for (statistic, expected) in [(1.0, 0.9), (2.0, 0.001), (3.0, f64::EPSILON), (7.0, 0.0)]
            {
                exact(
                    call(
                        "FORECAST.ETS.STAT",
                        vec![y.clone(), times.clone(), n(statistic), n(season)],
                    ),
                    expected,
                );
            }
        }
    }
    // Explicit period two at four points is nonseasonal; the binary64 short
    // initializer reproduces the recorded Excel forecast (dc-6332.5 harvest).
    exact(
        call(
            "FORECAST.ETS",
            vec![n(5.0), arr(vec![vec![2.0, 7.0, -1.0, 4.0]]), times, n(2.0)],
        ),
        1.5926883521785709,
    );
}

/// R40 fresh Excel20527 selections. The short initializer extends the first
/// two phases to seven before search, while forecasting the original history.
/// The final five-point row separates PC64 seasonal publication by one ULP.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_exact_short_initialization_and_final_seasonal_addition() {
    for (inputs, expected) in [
        (
            vec![0x363e000000000000, 0x362ac00000000000, 0xb6597c0000000000],
            [
                0xb66364fcec311623,
                0x3fb999999999999a,
                0x3fb95810624dd2f2,
                0x363d0c17e6b54aaf,
            ],
        ),
        (
            vec![
                0x404d940000000000,
                0xc034080000000000,
                0xc02a600000000000,
                0x404a7c0000000000,
            ],
            [
                0x4042242b202448ef,
                0x3fd0000000000000,
                0x3f50624dd2f1a9fc,
                0x404911709dfd64dc,
            ],
        ),
        (
            vec![
                0xbbacc40000000000,
                0xbb90280000000000,
                0x3b5a000000000000,
                0xbb97080000000000,
                0x3ba3340000000000,
            ],
            [
                0x3ba36755e5a5d6c9,
                0x3fb999999999999a,
                0x3f50624dd2f1a9fc,
                0x3b93d2aa28c23363,
            ],
        ),
        (
            vec![
                0x3623e80000000000,
                0xb618100000000000,
                0xb5fbc00000000000,
                0xb615180000000000,
                0x362b200000000000,
                0xb601e00000000000,
            ],
            [
                0x3614c41938705503,
                0x3fd0000000000000,
                0x3f50624dd2f1a9fc,
                0x3624efb13de41d62,
            ],
        ),
        (
            vec![
                0xc4b1700000000000,
                0xc46c000000000000,
                0xc4a1b00000000000,
                0x44c0f80000000000,
                0xc4c5400000000000,
                0x44c4380000000000,
                0xc495e00000000000,
            ],
            [
                0x44907efd9abfc7e5,
                0x3f60624dd2f1a9fc,
                0x3f50624dd2f1a9fc,
                0x44bb51cbb6f8bd3d,
            ],
        ),
        (
            vec![
                0xbdfbe00000000000,
                0x3e28280000000000,
                0xbe2ac80000000000,
                0x3df0000000000000,
                0x3dc0000000000000,
            ],
            [
                0xbe058406e1503b12,
                0x3f60624dd2f1a9fc,
                0x3f50624dd2f1a9fc,
                0x3e208a5188fd3683,
            ],
        ),
    ] {
        let count = inputs.len();
        let y = arr(vec![inputs.into_iter().map(f64::from_bits).collect()]);
        let times = arr(vec![(1..=count).map(|i| i as f64).collect()]);
        exact(
            call(
                "FORECAST.ETS",
                vec![n(count as f64 + 1.0), y.clone(), times.clone(), n(0.0)],
            ),
            f64::from_bits(expected[0]),
        );
        for (statistic, bits) in [(1.0, expected[1]), (2.0, expected[2]), (7.0, expected[3])] {
            exact(
                call(
                    "FORECAST.ETS.STAT",
                    vec![y.clone(), times.clone(), n(statistic), n(0.0)],
                ),
                f64::from_bits(bits),
            );
        }
    }
    // A non-dyadic initial quotient is now computed by the binary64 regression
    // (dc-6332.5); no Excel value is recorded for this input, so only the
    // calculated outcome is held.
    assert!(matches!(
        call(
            "FORECAST.ETS",
            vec![
                n(5.0),
                arr(vec![vec![0.0, 0.0, 0.0, 1.0]]),
                arr(vec![vec![1.0, 2.0, 3.0, 4.0]]),
                n(0.0)
            ]
        ),
        Ok(_)
    ));
    // One of 24 separately selected final-product midpoint questions. Strict
    // binary64 multiplication publishes ...e91, Excel's PC64 store ...e92.
    exact(
        call(
            "FORECAST.ETS.CONFINT",
            vec![
                n(8.0),
                arr(vec![vec![1251.65625, 840.28125, -71.3125]]),
                arr(vec![vec![1.0, 2.0, 3.0]]),
                n(0.95),
                n(0.0),
            ],
        ),
        f64::from_bits(0x407652d81eb07e92),
    );
}

/// R40 independent exact-period questions cover the three-cycle boundary.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_exact_seasonal_nonbinary_periods_and_six_observations() {
    for (period, forecast) in [(2, 10.0), (3, 14.0), (5, 22.0), (6, 26.0), (7, 30.0)] {
        let count = 3 * period;
        let values: Vec<_> = (0..count)
            .map(|j| (2 + j + if j % period == 0 { period } else { 0 }) as f64)
            .collect();
        let y = arr(vec![values]);
        let times = arr(vec![(1..=count).map(|i| i as f64).collect()]);
        exact(
            call(
                "FORECAST.ETS",
                vec![
                    n(count as f64 + 1.0),
                    y.clone(),
                    times.clone(),
                    n(period as f64),
                ],
            ),
            forecast,
        );
        for (statistic, expected) in [(1.0, 0.751), (2.0, 0.001), (3.0, 0.001), (7.0, 0.0)] {
            exact(
                call(
                    "FORECAST.ETS.STAT",
                    vec![y.clone(), times.clone(), n(statistic), n(period as f64)],
                ),
                expected,
            );
        }
        exact(
            call(
                "FORECAST.ETS.CONFINT",
                vec![n(count as f64 + 1.0), y, times, n(0.95), n(period as f64)],
            ),
            0.0,
        );
    }
    // The two-cycle odd-period initializer and newly extended forecast
    // publication remain unproved. Keep both original refusal inputs.
    assert!(matches!(
        call(
            "FORECAST.ETS",
            vec![
                n(7.0),
                arr(vec![vec![5.0, 3.0, 4.0, 8.0, 6.0, 7.0]]),
                arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]]),
                n(3.0)
            ]
        ),
        Err(calc_func::FunctionFault::NotHeld {
            owner: "dc-j9k",
            ..
        })
    ));
    assert!(matches!(
        call(
            "FORECAST.ETS",
            vec![
                n(7.0),
                arr(vec![vec![4.0, 3.0, 6.0, 5.0, 8.0, 7.0]]),
                arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]]),
                n(1.0)
            ]
        ),
        Err(calc_func::FunctionFault::NotHeld {
            owner: "dc-j9k",
            ..
        })
    ));
    exact(
        call(
            "FORECAST.ETS",
            vec![
                n(7.0),
                arr(vec![vec![4.0, 3.0, 6.0, 5.0, 8.0, 7.0]]),
                arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]]),
                n(2.0),
            ],
        ),
        10.0,
    );
}

/// Fresh R39 explicit periods: one shared state loop retains zero innovations.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_exact_seasonal_affine_states_and_reported_start() {
    for (pattern, count, expected) in [
        (
            vec![2.0, 3.0],
            8,
            [
                0x400f000000000000u64,
                0x4008000000000000,
                0x4010800000000000,
                0x400c000000000000,
            ],
        ),
        (
            vec![0.25, 1.5, 3.25, -3.5],
            8,
            [
                0xc005000000000000,
                0x3ff4000000000000,
                0x4005000000000000,
                0x3ffc000000000000,
            ],
        ),
        (
            vec![0.5, -2.25, -3.25, 3.0, -1.0, 1.75, 1.25, -0.25],
            16,
            [
                0x3ffa000000000000,
                0x4004000000000000,
                0xbfc0000000000000,
                0x3ff8000000000000,
            ],
        ),
    ] {
        let period = pattern.len();
        let values: Vec<_> = (0..count)
            .map(|j| pattern[j % period] + j as f64 / 8.0)
            .collect();
        let times = arr(vec![(1..=count).map(|j| j as f64).collect()]);
        let y = arr(vec![values.clone()]);
        for (h, bits) in [0.0, 1.0, 2.0, 5.0].into_iter().zip(expected) {
            exact(
                call(
                    "FORECAST.ETS",
                    vec![
                        n(count as f64 + h),
                        y.clone(),
                        times.clone(),
                        n(period as f64),
                    ],
                ),
                f64::from_bits(bits),
            );
        }
        for (statistic, expected) in [0.751, 0.001, 0.001, 0.0, 0.0, 0.0, 0.0]
            .into_iter()
            .enumerate()
        {
            exact(
                call(
                    "FORECAST.ETS.STAT",
                    vec![
                        y.clone(),
                        times.clone(),
                        n((statistic + 1) as f64),
                        n(period as f64),
                    ],
                ),
                expected,
            );
        }
        exact(
            call(
                "FORECAST.ETS.CONFINT",
                vec![
                    n(count as f64 + 10.0),
                    y.clone(),
                    times.clone(),
                    n(0.95),
                    n(period as f64),
                ],
            ),
            0.0,
        );
        let mut noisy = values;
        noisy[count - 1] += 1.0 / 16.0;
        assert!(matches!(
            call(
                "FORECAST.ETS",
                vec![
                    n(count as f64 + 1.0),
                    arr(vec![noisy]),
                    times,
                    n(period as f64)
                ]
            ),
            Err(calc_func::FunctionFault::NotHeld {
                owner: "dc-j9k",
                ..
            })
        ));
    }
    for value in [0.0, 2.0_f64.powi(-400), 2.0_f64.powi(400)] {
        let y = arr(vec![vec![value; 16]]);
        let times = arr(vec![(1..=16).map(|j| j as f64).collect()]);
        exact(
            call(
                "FORECAST.ETS",
                vec![n(17.0), y.clone(), times.clone(), n(8.0)],
            ),
            value,
        );
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(1.0), n(8.0)],
            ),
            0.751,
        );
        exact(
            call(
                "FORECAST.ETS.CONFINT",
                vec![
                    n(17.0),
                    y,
                    times,
                    n(f64::from_bits(0x3fefffffffffffff)),
                    n(8.0),
                ],
            ),
            0.0,
        );
    }
}

/// Fresh detector mapping and automatic-profile coupling retain period two.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_automatic_exact_alternating_states() {
    let y = arr(vec![vec![
        1.0, 127.0, 257.0, 383.0, 513.0, 639.0, 769.0, 895.0,
    ]]);
    let times = arr(vec![(1..=8).map(|j| j as f64).collect()]);
    exact(
        call("FORECAST.ETS.SEASONALITY", vec![y.clone(), times.clone()]),
        2.0,
    );
    exact(
        call("FORECAST.ETS", vec![n(9.0), y.clone(), times.clone()]),
        1025.0,
    );
    exact(
        call("FORECAST.ETS.STAT", vec![y.clone(), times.clone(), n(1.0)]),
        0.751,
    );
    exact(
        call("FORECAST.ETS.STAT", vec![y.clone(), times.clone(), n(3.0)]),
        0.001,
    );
    exact(
        call("FORECAST.ETS.CONFINT", vec![n(9.0), y, times, n(0.95)]),
        0.0,
    );
    for (offset, trend) in [(0.0, 2.0_f64.powi(36)), (2.0_f64.powi(39), 0.0)] {
        let y = arr(vec![
            (0..8)
                .map(|j| offset + if j % 2 == 0 { 1.0 } else { -1.0 } + trend * j as f64)
                .collect(),
        ]);
        let times = arr(vec![(1..=8).map(|j| j as f64).collect()]);
        exact(
            call("FORECAST.ETS.SEASONALITY", vec![y.clone(), times.clone()]),
            2.0,
        );
        exact(
            call("FORECAST.ETS", vec![n(9.0), y, times]),
            549_755_813_889.0,
        );
    }
}

/// R46: the two R41 adjacent histories repeat at three exact money scales.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_adjacent_histories_separate_raw_moment_reset_arithmetic() {
    for (values, beta, forecasts) in [
        (
            [
                0xbfcefa25555eb39e,
                0xbfc4e4f4e3a538d8,
                0x3fc97adbb54b314d,
                0xbfd14efdf9923915,
                0xbfc0ddbeeb06d525,
                0xbfc1471962c9bbe4,
                0xbfd2dffc3cde2a99,
                0xbfc1fd87b671759a,
                0xbfc3cead76b44036,
                0xbfc043112287f4a7,
                0xbf7a17fd88ff2440,
                0xbfd1a52c0f60cf04,
                0xbfc33dfd25ebae60,
                0xbfc6a246ee8e8a90,
                0xbfc3b6d827f1d202,
                0xbfd5c7ed3073c9fa,
                0xbfda3af5c3bbfe44,
                0xbfdc09796cfbefc3,
                0xbfd244b7f9557300,
                0xbfe4fdf759b9f1b5,
                0xbfe693d180891fda,
                0xbfe743de9ec7d442,
                0xbfd407237394c7a0,
                0xbfe4a097169caa4f,
                0xbfe0ad5c8d6d8f86,
                0xbfe5cd117a513caa,
                0xbfe3668f989556d4,
                0xbfe73e90de936e54,
                0xbfe7d84b18521912,
            ],
            0x3f50624dd2f1a9fc,
            [0xbfe7d84b18521912, 0xbfe8a01ab9d192aa],
        ),
        (
            [
                0xbfcefa25555eb396,
                0xbfc4e4f4e3a538d3,
                0x3fc97adbb54b3144,
                0xbfd14efdf9923912,
                0xbfc0ddbeeb06d525,
                0xbfc1471962c9bbe4,
                0xbfd2dffc3cde2a98,
                0xbfc1fd87b671759c,
                0xbfc3cead76b44039,
                0xbfc043112287f4ac,
                0xbf7a17fd88ff2560,
                0xbfd1a52c0f60cf04,
                0xbfc33dfd25ebae66,
                0xbfc6a246ee8e8a96,
                0xbfc3b6d827f1d209,
                0xbfd5c7ed3073c9fc,
                0xbfda3af5c3bbfe45,
                0xbfdc09796cfbefc5,
                0xbfd244b7f9557304,
                0xbfe4fdf759b9f1b4,
                0xbfe693d180891fd9,
                0xbfe743de9ec7d442,
                0xbfd407237394c7a7,
                0xbfe4a097169caa50,
                0xbfe0ad5c8d6d8f88,
                0xbfe5cd117a513caa,
                0xbfe3668f989556d5,
                0xbfe73e90de936e54,
                0xbfe7d84b18521913,
            ],
            0x3fc5555821294574,
            [0xbfe61fb9811b5204, 0xbfe6fa560c4d00c6],
        ),
    ] {
        let y = arr(vec![values.into_iter().map(f64::from_bits).collect()]);
        let times = arr(vec![(1..=29).map(|j| j as f64).collect()]);
        exact(
            call(
                "FORECAST.ETS.STAT",
                vec![y.clone(), times.clone(), n(2.0), n(0.0)],
            ),
            f64::from_bits(beta),
        );
        for (horizon, expected) in forecasts.into_iter().enumerate() {
            exact(
                call(
                    "FORECAST.ETS",
                    vec![n(29.0 + horizon as f64), y.clone(), times.clone(), n(0.0)],
                ),
                f64::from_bits(expected),
            );
        }
    }
}

/// Excel 20527: lower-tail seed underflow is decided before scale, while
/// positive tiny arguments must not be mistaken for the gamma endpoint.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_positive_underflow_and_inverse_seed_match_excel() {
    exact(
        call("GAMMA.DIST", vec![n(1e-300), n(1e-100), n(1.0), b(true)]),
        1.0,
    );
    for name in ["GAMMA.INV", "GAMMAINV"] {
        for scale in [1.0, 1e100] {
            exact(call(name, vec![n(1e-100), n(1e-100), n(scale)]), 0.0);
            error(call(name, vec![n(1e-100), n(0.32), n(scale)]), Div0);
        }
    }
}

/// Excel 20527: the scaled Temme tail stays finite across the search bracket;
/// the one-degree right inverse preserves Excel's two stored complements.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chi_inverse_scaled_tail_and_probability_fold_match_excel() {
    exact(call("CHISQ.DIST.RT", vec![n(62995.5), n(45000.0)]), 0.0);
    for name in ["CHIINV", "CHISQ.INV.RT"] {
        exact(call(name, vec![n(1.0), n(1e100)]), 0.0);
        error(call(name, vec![n(1.0), n(0.5)]), Num);
        error(call(name, vec![n(1e-100), n(1.0)]), Num);
        exact(
            call(name, vec![n(f64::from_bits(0x3f7d_b69d_98cd_5de8)), n(1.0)]),
            f64::from_bits(0x401c_d5ed_c89c_33c2),
        );
        exact(
            call(
                name,
                vec![n(f64::from_bits(0x3f98_5dc7_98f6_0e40)), n(45000.0)],
            ),
            f64::from_bits(0x40e6_4387_fb78_1d75),
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn f_inverse_probability_endpoints_validate_degrees() {
    exact(call("F.INV", vec![n(0.0), n(5.0), n(8.0)]), 0.0);
    error(call("F.INV", vec![n(0.0), n(0.5), n(8.0)]), Num);
    error(call("F.INV", vec![n(1.0), n(5.0), n(8.0)]), Num);
    for name in ["F.INV.RT", "FINV"] {
        exact(call(name, vec![n(1.0), n(5.0), n(8.0)]), 0.0);
        error(call(name, vec![n(1.0), n(5.0), n(0.5)]), Num);
        error(call(name, vec![n(0.0), n(5.0), n(8.0)]), Num);
    }
}


/// Excel20527 unit-shape inverses use the shared distribution logarithm,
/// then publish a stored scale with final subnormal flush.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_unit_inverse_and_final_product_match_excel() {
    for name in ["GAMMA.INV", "GAMMAINV"] {
        exact(
            call(name, vec![n(0.1), n(1.0), n(1.0)]),
            f64::from_bits(0x3fba_f8e8_210a_415e),
        );
        for shape in [0.5, 1.0, 2.0] {
            exact(
                call(name, vec![n(0.1), n(shape), n(f64::MIN_POSITIVE)]),
                0.0,
            );
        }
    }
    for name in ["CHIINV", "CHISQ.INV.RT"] {
        exact(
            call(name, vec![n(0.001), n(2.0)]),
            f64::from_bits(0x402b_a18a_998f_ffa0),
        );
    }
}

/// The reflected search publishes its finite lattice floor, rather than
/// the much smaller mathematical root obtained by a direct-coordinate search.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_inverse_upper_probability_search_preserves_excel_floor() {
    for name in ["BETA.INV", "BETAINV"] {
        exact(
            call(name, vec![n(0.75), n(0.0008), n(1.0)]),
            f64::from_bits(0x3ca0_0000_0000_0000),
        );
    }
}


/// Excel20527: the zero-density inverse arm publishes its initial proposal,
/// including the endpoint fallback, before the ordinary inverse search.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_inverse_zero_density_keeps_the_measured_initial_proposal() {
    for name in ["BETA.INV", "BETAINV"] {
        exact(
            call(
                name,
                vec![
                    n(f64::from_bits(0x28b3_d596_a03f_8745)),
                    n(f64::from_bits(0x4073_0960_aa5d_3427)),
                    n(f64::from_bits(0x3fed_f483_11ed_453e)),
                ],
            ),
            f64::from_bits(0x3f71_d2a4_ab12_c880),
        );
        exact(
            call(name, vec![n(0.1), n(1e100), n(1.0)]),
            f64::from_bits(0x3fef_fffd_e721_0be9),
        );
        exact(
            call(name, vec![n(0.9), n(1.0), n(1e100)]),
            f64::from_bits(0x3eb0_c6f7_a0b8_0000),
        );
    }
}

/// Captured Excel20527 tiny-tail answers pass through the same beta initial
/// proposal into Student-t and then the application's confidence calculation.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn student_t_and_confidence_keep_the_zero_density_beta_proposal() {
    exact(
        call(
            "TINV",
            vec![
                n(f64::from_bits(0x34a7_b5b3_0b92_82c0)),
                n(f64::from_bits(0x4070_eff7_3350_b850)),
            ],
        ),
        f64::from_bits(0x4073_70ba_f9a5_4a38),
    );
    exact(
        call(
            "CONFIDENCE.T",
            vec![
                n(1e-100),
                n(100.0),
                n(f64::from_bits(0x4088_f661_69ab_758e)),
            ],
        ),
        f64::from_bits(0x4071_3e70_48ac_bac8),
    );
}
