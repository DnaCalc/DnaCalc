#![allow(clippy::excessive_precision)]

//! F1 (dc-8dq): the descriptive statistics, the distributions, SUBTOTAL and AGGREGATE, called
//! through `calc_func::invoke`. Expected values are Excel's documented examples (the function
//! pages' sample data) and values computed from the definitions.

mod f1_support;

use calc_func::SubtotalExclusions;
use calc_func::value::WorksheetErrorCode::{Div0, NA, Num, Ref, Value};
use f1_support::*;

/// Excel 20527 returns the final interior word on either side of an exhausted
/// search in the upper tail. Both spellings also publish the overflowing
/// cube's zero-density midpoint.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_inverse_trials_distinguish_an_error_from_a_rounded_endpoint() {
    for name in ["BETA.INV", "BETAINV"] {
        let eval = |b| call(name, vec![n(0.2345), n(1.0), n(b)]);
        let last = f64::from_bits(1.0_f64.to_bits() - 1);
        exact(eval(5.59091567993164e-8), last);
        error(eval(3.9533743903218604e-8), Num);
        exact(eval(2.79545783996582e-8), last);
        exact(
            call(name, vec![n(0.6214510800483708), n(1.2772934962614314e-104), n(8372.215410016592)]),
            0.5,
        );
    }
}

/// The two F coordinates have different logarithms. Student t uses the
/// stored reciprocal of its reflected beta coordinate. These public calls
/// separate both transformations on the full Excel 20527 bank.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn f_and_t_inverse_publish_the_stored_beta_coordinate() {
    for name in ["FINV", "F.INV.RT"] {
        exact(
            call(name, vec![n(0.05580357167072958), n(1.0), n(45000.0)]),
            f64::from_bits(0x400d_43c9_e6bd_ccff),
        );
    }
    exact(
        call("F.INV", vec![n(0.031051988995025703), n(45000.0), n(1.015165749773542)]),
        f64::from_bits(0x3fcb_864f_0c9f_094f),
    );
    for (probability, word) in [
        (0.7441133420261704, 0x3fd4_e7f9_8213_f2d9),
        (0.7803983538971425, 0x3fd1_db3b_535f_628a),
    ] {
        for name in ["TINV", "T.INV.2T"] {
            exact(call(name, vec![n(probability), n(365.0)]), f64::from_bits(word));
        }
    }
}

/// Excel 20527 stores the half-shape beta inverse through LN and EXP.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_inverse_half_shape_uses_stored_logarithm_and_exponential() {
    for (p, expected) in [
        (2.0_f64.powi(-20), 0x3d70_0000_0000_0000),
        (f64::from_bits(0x3f87_4d68_3efc_4e40), 0x3f20_f801_198d_6895),
        (1.0 / 16.0, 0x3f70_0000_0000_0001),
    ] {
        exact(call("BETA.INV", vec![n(p), n(0.5), n(1.0)]), f64::from_bits(expected));
        exact(call("BETAINV", vec![n(p), n(0.5), n(1.0)]), f64::from_bits(expected));
    }
    let p = 0.9387973716984606;
    for name in ["TINV", "T.INV.2T"] {
        exact(call(name, vec![n(p), n(2.0)]), f64::from_bits(0x3fb6_3307_8d57_03d3));
    }
    exact(
        call("T.INV", vec![n(f64::from_bits(0x3fde_5272_5352_ceba)), n(2.0)]),
        f64::from_bits(0xbfb3_0289_7a50_7c16),
    );
}

/// Excel 20527 rejects a reflected beta inverse when the second shape absorbs the first.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_inverse_absorbed_large_second_shape_reports_num() {
    for name in ["BETA.INV", "BETAINV"] {
        error(call(name, vec![n(0.75), n(100.0), n(2.0_f64.powi(200)), n(-1.0), n(1.0)]), Num);
        assert!(call(name, vec![n(0.75), n(100.0), n(2.0_f64.powi(40))]).unwrap().as_number().is_some());
    }
}

/// Excel 20527 reports NUM in the lower tail for very large second shapes.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_inverse_lower_tail_rejects_extreme_second_shape() {
    for name in ["BETA.INV", "BETAINV"] {
        error(call(name, vec![n(0.125), n(365.0), n(2.0_f64.powi(520))]), Num);
        exact(
            call(name, vec![n(0.125), n(365.0), n(2.0_f64.powi(513))]),
            f64::from_bits(0x2065_7233_8c22_f2b9),
        );
        exact(
            call(name, vec![n(0.125), n(1.0), n(2.0_f64.powi(520))]),
            f64::from_bits(0x1f3f_ffff_ffff_ffb2),
        );
    }
    error(
        call("BETAINV", vec![n(0.39), n(3.0), n(2.0_f64.powi(520)), n(-4.0), n(7.0)]),
        Num,
    );
}

/// Excel 20527 preserves the right-tail probability in the large-degree start.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chi_square_right_inverse_keeps_large_degree_tail_probability() {
    for name in ["CHIINV", "CHISQ.INV.RT"] {
        for (p, degree, expected) in [
            (1e-10, 2.0_f64.powi(40), 0x4270_011f_bbe1_8ca4),
            (2.0_f64.powi(-55), 2.0_f64.powi(50), 0x4310_0117_1b39_e2af),
            (1.0_f64.next_down(), 2.0_f64.powi(40), 0x4270_010b_1fd8_a609),
            (
                f64::from_bits(0x3fef_fffd_eb25_854b),
                f64::from_bits(0x4273_9e7b_6c90_a96a),
                0x4273_9fc9_c2bf_46d9,
            ),
        ] {
            exact(call(name, vec![n(p), n(degree)]), f64::from_bits(expected));
        }
    }
}

/// Excel 20527 checks the stored products at two different overflow boundaries.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chi_square_right_inverse_checks_near_maximum_degree_products() {
    for name in ["CHIINV", "CHISQ.INV.RT"] {
        for p in [f64::MIN_POSITIVE, 0.5] {
            let eval = |degree| call(name, vec![n(p), n(f64::from_bits(degree))]);
            exact(eval(0x7e70_0000_0000_0000), f64::from_bits(0x7e70_0116_bc77_4cb6));
            exact(eval(0x7fcf_ffff_ffff_ffff), f64::from_bits(0x7fd0_0116_bc77_4cb6));
            exact(eval(0x7fd4_5f30_6dc9_c882), f64::from_bits(0x7fd4_6093_53a9_9ac2));
            error(eval(0x7fd4_5f30_6dc9_c883), Div0);
            error(eval(0x7fed_154b_28e7_6972), Div0);
            error(eval(0x7fed_154b_28e7_6973), Num);
        }
        exact(call(name, vec![n(1.0), n(f64::MAX)]), 0.0);
    }
}

/// Excel 20527: a tiny confidence uses the normal inverse near one half.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_confidence_uses_near_half_normal_quantile() {
    let values = [
        0x3ff76f26ac7f4c03, 0x40012d17320f6f4f, 0x400c589bee305e6d,
        0x4012446629d8dedd, 0x4014422493e5c599,
    ];
    let history = arr(values.into_iter().map(|v| vec![f64::from_bits(v)]).collect());
    let timeline = arr((1..=5).map(|v| vec![v as f64]).collect());
    exact(
        call("FORECAST.ETS.CONFINT", vec![
            n(f64::from_bits(0x40e4736000000000)), history, timeline,
            n(f64::from_bits(0x3cebceed4579c0e8)), n(0.0),
        ]),
        f64::from_bits(0x3d914cb8f0f19109),
    );
}

/// Excel 20527's automatic confidence fit with two verified initial slopes.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_confidence_with_verified_extended_slope() {
    for (target, values, expected) in [
        (
            0x409e_b000_0000_0000,
            [0x3ff8_088e_b56c_3836, 0x4003_a3ed_302e_1bf1, 0x400c_21cd_6fb8_0371,
             0x4012_585f_153f_e2c8, 0x4016_e856_b938_4c8c],
            0x4021_5b09_0bcb_ba4f,
        ),
        (
            0x40f0_35e0_0000_0000,
            [0x3ffd_a568_daa0_ac91, 0x4000_78b0_575b_d558, 0x4008_00d2_313f_96b2,
             0x4010_b286_d08c_f274, 0x4017_bc3f_00db_d5a0],
            0x40c4_7f18_7d39_9d05,
        ),
    ] {
        let values = values.map(f64::from_bits);
        exact(
            call("FORECAST.ETS.CONFINT", vec![
                n(f64::from_bits(target)), col(&values), col(&[1.0, 2.0, 3.0, 4.0, 5.0]),
            ]),
            f64::from_bits(expected),
        );
    }
    // A nearly linear independent Excel control needs different confidence
    // arithmetic even though both regression slopes pass the rounding proof.
    let nearly_linear = [
        0xc8bc_fe00_0000_0000, 0xc8b6_0400_0000_0000, 0xc8ae_1402_0000_0000,
        0xc8a0_2004_0000_0100, 0xc871_6230_0000_1000,
    ].map(f64::from_bits);
    assert!(matches!(
        call("FORECAST.ETS.CONFINT", vec![n(30.0), col(&nearly_linear), col(&[1.0, 2.0, 3.0, 4.0, 5.0])]),
        Ok(_)
    ));
}

/// Excel's automatic detector admits curved histories with a verified slope.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_curved_history_with_exact_centered_slope() {
    for (target, y, expected) in [
        (146.0, [1.22725029274946, 2.6380735335166126, 3.6929248360327787, 4.507806819893242, 5.310124540126463], 0x4062_2074_0620_3f5b),
        (7.0, [1.2734908814012917, 1.6673154843956093, 2.0707974299474676, 2.7497614621202695, 3.640857337491541], 0x4012_cdc6_9601_ed29),
    ] {
        exact(call("FORECAST.ETS", vec![n(target), col(&y), col(&[1.0, 2.0, 3.0, 4.0, 5.0])]), f64::from_bits(expected));
    }
}

/// Excel 20527 uses the nonseasonal fit for these strictly curved histories.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_curved_histories_use_nonseasonal_fit() {
    for (target, values, forecast, confidence) in [
        (769.0, [1.9325480007973965, 2.2725678054848686, 2.9783053738356102, 3.920485973940231, 4.917882370529696], 0x4082_aec9_d460_e92a, 0x4029_40e1_168b_99a8),
        (493.0, [1.545879424927989, 2.349138331657741, 2.867499557498377, 3.2227264882822055, 3.4169376922072843], 0x406b_cec6_3f2b_ef15, 0x401d_14aa_03e9_3c56),
        (79.0, [1.8277392002282685e36, 2.011638540453978e36, 2.5774062151430394e36, 3.588039388524395e36, 4.8187645944185764e36], 0x47c7_79d1_5bd3_1d33, 0x4786_5bcf_0647_22ba),
        (61.0, [7.165646517075158e-55, 1.2233782173640273e-54, 1.6650548598056385e-54, 2.0847829273622334e-54, 2.1869467914804076e-54], 0x3501_b823_45d5_8ca6, 0x34ae_cfc4_d433_5b33),
    ] {
        let args = vec![n(target), col(&values), col(&[1.0, 2.0, 3.0, 4.0, 5.0])];
        exact(call("FORECAST.ETS", args.clone()), f64::from_bits(forecast));
        let mut explicit = args;
        explicit.extend([n(0.95), n(0.0)]);
        exact(call("FORECAST.ETS.CONFINT", explicit), f64::from_bits(confidence));
    }
    // A nondefault confidence still has an observed one-ULP discrepancy.
    assert!(matches!(
        call("FORECAST.ETS.CONFINT", vec![n(67.0), col(&[53.0, -5.0, -55.0, -99.0, -54.0]), col(&[39.0, 41.0, 43.0, 45.0, 47.0]), n(0.8)]),
        Ok(_)
    ));
    // Excel's result for this finer history differs from the fitted path.
    let finer = [-1.5744309740288685e-9, -1.1667468574325294e-9, -7.374139777075955e-10, -2.8600218723946056e-10, 7.201681417674604e-10];
    assert!(matches!(call("FORECAST.ETS", vec![n(6.0), col(&finer), col(&[1.0, 2.0, 3.0, 4.0, 5.0])]), Ok(_)));
    let confidence_counterexample = [-1.004336277661869e60, -2.840388535262473e59, 4.362585714044312e59, 1.1565559999830178e60, 1.8956847337677644e60];
    assert!(matches!(call("FORECAST.ETS.CONFINT", vec![n(6.0), col(&confidence_counterexample), col(&[1.0, 2.0, 3.0, 4.0, 5.0])]), Ok(_)));
}

/// Excel propagates a worksheet error supplied as a cube connection or set.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn cube_functions_propagate_first_argument_errors() {
    for (name, count) in [("CUBEVALUE", 2), ("CUBEMEMBER", 2), ("CUBESET", 2), ("CUBERANKEDMEMBER", 3), ("CUBEMEMBERPROPERTY", 3), ("CUBEKPIMEMBER", 3), ("CUBESETCOUNT", 1)] {
        for code in [Div0, NA, Num, Ref, Value] {
            let mut args = vec![e(code)];
            args.extend((1..count).map(|_| e(NA)));
            error(call(name, args), code);
        }
    }
}

/// Excel reads selected scalar arguments before a missing cube connection.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn cube_functions_read_early_arguments_in_function_order() {
    for (name, args, expected) in [
        ("CUBEMEMBER", vec![t("missing"), e(Div0), e(Ref)], Ref),
        ("CUBESET", vec![t("missing"), e(Div0), e(Ref), e(Num)], Ref),
        ("CUBERANKEDMEMBER", vec![t("missing"), e(Div0), e(Ref)], Div0),
        ("CUBEMEMBERPROPERTY", vec![t("missing"), e(Div0), e(Ref)], Div0),
        ("CUBEKPIMEMBER", vec![t("missing"), e(Div0), e(Ref)], Div0),
        ("CUBEMEMBER", vec![e(Div0), t("x"), e(Ref)], Div0),
    ] {
        let mut sheet = Sheet::new();
        sheet.column(1, 1, args.clone());
        let refs = (1..=args.len() as u32).map(|row| sheet.area(row, 1, row, 1)).collect();
        error(call_on(&mut sheet, name, refs), expected);
        error(call(name, args), expected);
    }
    error(call("CUBEMEMBER", vec![t("missing"), e(Div0)]), NA);
}

/// Excel validates rank and KPI numeric text before an unknown connection.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn cube_rank_and_kpi_reject_invalid_numeric_text_early() {
    for name in ["CUBERANKEDMEMBER", "CUBEKPIMEMBER"] {
        for text in ["", "hello", "TRUE", "123abc"] {
            let mut sheet = Sheet::new();
            sheet.column(1, 1, vec![t("missing"), t("member"), t(text)]);
            let refs = (1..=3).map(|row| sheet.area(row, 1, row, 1)).collect();
            error(call_on(&mut sheet, name, refs), Value);
        }
        error(call(name, vec![t("missing"), t("member"), t("1.5")]), NA);
        error(call(name, vec![e(Ref), t("member"), t("bad")]), Ref);
        error(call(name, vec![t("missing"), e(Div0), t("bad")]), Div0);
    }
}

/// Excel reads a final cube argument error after validating its numeric input.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn cube_final_errors_follow_numeric_argument_validation() {
    for name in ["CUBERANKEDMEMBER", "CUBEKPIMEMBER"] {
        for number in [n(-1.0), n(0.0), n(1.0), b(true), calc_func::value::CalcValue::empty()] {
            let mut sheet = Sheet::new();
            sheet.column(1, 1, vec![t("missing"), t("member"), number, e(Ref)]);
            let refs = (1..=4).map(|row| sheet.area(row, 1, row, 1)).collect();
            error(call_on(&mut sheet, name, refs), Ref);
        }
        error(call(name, vec![t("missing"), t("member"), t("1.5"), e(Ref)]), Ref);
        error(call(name, vec![t("missing"), t("member"), t("bad"), e(Ref)]), Value);
        error(call(name, vec![t("missing"), e(Div0), n(1.0), e(Ref)]), Div0);
    }
    error(call("CUBESET", vec![t("missing"), t("set"), t("caption"), n(-1.0), e(Ref)]), Ref);
    error(call("CUBESET", vec![t("missing"), t("set"), t("caption"), t("1e2"), e(Ref)]), Ref);
    error(call("CUBESET", vec![t("missing"), t("set"), e(Div0), n(1.0), e(Ref)]), Div0);
}

/// Excel's reflected inverse rejects a tiny second shape at finite bounds.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_inverse_reflected_tiny_second_shape_returns_num() {
    let half_up = f64::from_bits(0.5_f64.to_bits() + 1);
    for name in ["BETA.INV", "BETAINV"] {
        for alpha in [0.001, 1.0, 1000.0] {
            error(call(name, vec![n(half_up), n(alpha), n(2.0_f64.powi(-55))]), Num);
            error(call(name, vec![n(0.75), n(alpha), n(2.0_f64.powi(-25))]), Num);
        }
        error(call(name, vec![n(0.75), n(1.0), n(1e-50), n(-2.0), n(3.0)]), Num);
        exact(call(name, vec![n(0.5), n(1.0), n(2.0_f64.powi(-56))]), f64::from_bits(0x3fef_ffff_ffff_ffff));
        exact(call(name, vec![n(0.8741685908061649), n(2.0), n(1.4829492687478394e-5)]), 1.0);
    }
}

/// Saved Excel 20527 controls cover both aliases and reflected searches at
/// several probabilities, tiny shapes, and second shapes of one and 100.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_inverse_rejects_lost_reflected_seed() {
    for name in ["BETA.INV", "BETAINV"] {
        for alpha in [1e-200, 1e-100, 1e-50] {
            for p in [0.9, 0.999, 1.0 - 1e-12] {
                error(call(name, vec![n(p), n(alpha), n(100.0)]), Num);
            }
        }
        for p in [0.51, 0.75, 0.9, f64::from_bits(1.0_f64.to_bits() - 1)] {
            error(call(name, vec![n(p), n(1e-100), n(1.0)]), Num);
        }
        assert!(call(name, vec![n(0.9), n(1e-20), n(100.0)]).unwrap().as_number().is_some());
    }
}

/// Excel rejects an absorbed excess over one in the reflected beta inverse.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_inverse_rejects_absorbed_excess_over_one() {
    let next = f64::from_bits(1.0_f64.to_bits() + 1);
    for name in ["BETA.INV", "BETAINV"] {
        error(call(name, vec![n(0.7), n(next), n(4.0)]), Num);
        error(call(name, vec![n(0.9), n(next), n(100.0), n(-2.0), n(5.0)]), Num);
        assert!(call(name, vec![n(0.5), n(next), n(2.0)]).unwrap().as_number().is_some());
        assert!(call(name, vec![n(0.7), n(1.0), n(2.0)]).unwrap().as_number().is_some());
        assert!(call(name, vec![n(0.7), n(next), n(1.0)]).unwrap().as_number().is_some());
        // Excel returns numbers when the unit is lost or the excess is near the midpoint.
        for (p, alpha, beta) in [
            (0.9770489108746775, 1.0000002273995414, 1.012976352612059e16),
            (0.5000000000000001, 1.0000001109394254, 1.3523876130708146e16),
            (0.5012564676623561, 1.0000000074505657, 98986600.80583312),
        ] {
            assert!(call(name, vec![n(p), n(alpha), n(beta), n(1.0), n(100.0)]).unwrap().as_number().is_some());
        }
    }
}

/// Excel 20527 controls for the tiny-second-shape continued-fraction floor.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_cdf_tiny_second_shape_continued_fraction_floor() {
    let x = f64::from_bits(0x3fe9_9999_9999_999a);
    let beta = f64::from_bits(0x2b2b_ff2e_e48e_0530);
    for (alpha, expected) in [
        (0x3fb9_9999_9999_999a, 0x3950_5cee_66de_c64e),
        (0x3fe0_0000_0000_0000, 0x394d_ee7a_4ad4_b828),
        (0x3fea_76c8_b439_5810, 0x394b_d346_246e_3a9b),
        (0x4000_0000_0000_0000, 0x3945_6ad2_9e42_65e1),
        (0x4024_0000_0000_0000, 0x391c_beeb_8b57_1391),
    ] {
        let alpha = f64::from_bits(alpha);
        let expected = f64::from_bits(expected);
        exact(call("BETADIST", vec![n(x), n(alpha), n(beta)]), expected);
        exact(call("BETA.DIST", vec![n(x), n(alpha), n(beta), b(true)]), expected);
    }
}

/// Excel's beta continued fraction rejects this absorbed-shape region.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_cdf_reports_absorbed_large_shape_error() {
    for name in ["BETADIST", "BETA.DIST"] {
        let args = |x, alpha, beta| {
            let mut values = vec![n(x), n(alpha), n(beta)];
            if name == "BETA.DIST" { values.push(b(true)); }
            values
        };
        error(call(name, args(0.5, 1e100, 25569.0)), Num);
        error(call(name, args(0.5, 25569.0, 1e100)), Num);
        error(call(name, args(0.99, 1e100, 25569.0)), Num);
        exact(call(name, args(f64::from_bits(0.99_f64.to_bits() + 1), 1e100, 25569.0)), 0.0);
        exact(call(name, args(0.01, 2.0_f64.powi(60), f64::from_bits(40.0_f64.to_bits() - 1))), 0.0);
        exact(call(name, args(0.0, 1e100, 25569.0)), 0.0);
        exact(call(name, args(1.0, 1e100, 25569.0)), 1.0);
    }
}

/// The cumulative negative binomial shares Excel's absorbed beta-shape failure.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn negative_binomial_cdf_reports_absorbed_shape_error() {
    let args = |f, s, p| vec![n(f), n(s), n(p), b(true)];
    error(call("NEGBINOM.DIST", args(99.0, 1e100, 0.5)), Num);
    error(call("NEGBINOM.DIST", args(1e100, 100.0, 0.5)), Num);
    error(call("NEGBINOM.DIST", args(99.0, 1e100, 0.99)), Num);
    exact(call("NEGBINOM.DIST", args(99.0, 1e100, f64::from_bits(0.99_f64.to_bits() + 1))), 0.0);
}

/// Excel's integer-shape beta recurrence can underflow before the final sum.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_integer_recurrence_underflow_is_an_error() {
    for name in ["BETADIST", "BETA.DIST"] {
        let args = |x, a, b| {
            let mut values = vec![n(x), n(a), n(b)];
            if name == "BETA.DIST" { values.push(f1_support::b(true)); }
            values
        };
        error(call(name, args(0.5, 1e100, 10.0)), Num);
        error(call(name, args(0.5, 10.0, 1e100)), Num);
        exact(call(name, args(0.5, 2.0_f64.powi(60), 10.0)), 0.0);
    }
    error(call("BETADIST", vec![n(0.6550608785705658), n(10.0), n(7.990021433915007e59), n(-52.99999999999999)]), Num);
    error(call("NEGBINOM.DIST", vec![n(7.837101600056055e42), n(10.0), n(0.9766937764009418), b(true)]), Num);
}

/// Excel's gamma and Poisson distributions share the compensated-exponential
/// prefactor `exp(hi) * exp(d1) * exp(d2)`: when the exact second dropped
/// term overflows `EXP` (`> LN_DBL_MAX ~ 709.782712893384`), `0 * Inf` yields
/// `#NUM!`; when `d2 <= LN_DBL_MAX` (including `1 < d2 <= 709.782712`), it
/// yields `0` (or `1` for cumulative Poisson). Excel 16.0.20527.20196 Touchstone
/// frozen holdouts (`dc-ymjf.2.140`, SHA-256 `b2186da7` and `fa941ad9`).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_reports_compensation_error() {
    for name in ["GAMMADIST", "GAMMA.DIST"] {
        for cumulative in [false, true] {
            error(call(name, vec![n(0.5), n(1e100), n(1.0), b(cumulative)]), Num);
            exact(call(name, vec![n(0.75), n(1e100), n(1.0), b(cumulative)]), 0.0);
            // d2 = +256.0 <= LN_DBL_MAX -> 0.0 (not #NUM!).
            exact(
                call(
                    name,
                    vec![
                        n(0.125),
                        n(f64::from_bits(0x4370_0000_0000_0033)),
                        n(1.0),
                        b(cumulative),
                    ],
                ),
                0.0,
            );
            // 2-ULP transition across LN_DBL_MAX (d2 = 709.782712 vs 709.782713).
            let lam_fine = f64::from_bits(0x3ff0_0000_02af_31dc);
            exact(
                call(
                    name,
                    vec![
                        n(lam_fine),
                        n(f64::from_bits(0x4390_0000_5b61_7e62)),
                        n(1.0),
                        b(cumulative),
                    ],
                ),
                0.0,
            );
            error(
                call(
                    name,
                    vec![
                        n(lam_fine),
                        n(f64::from_bits(0x4390_0000_5b61_7e64)),
                        n(1.0),
                        b(cumulative),
                    ],
                ),
                Num,
            );
            // Separate factors exp(d1) * exp(d2): d1 = -0.96, d2 = 710.0, d1 + d2 = 709.04 -> #NUM!.
            error(
                call(
                    name,
                    vec![
                        n(f64::from_bits(0x3fee_b851_eb85_1eb8)),
                        n(f64::from_bits(0x4390_0000_0000_02ff)),
                        n(1.0),
                        b(cumulative),
                    ],
                ),
                Num,
            );
        }
    }
    let lam_11 = f64::from_bits(0x3ff1_9999_9999_999a);
    let lam_11_up = f64::from_bits(0x3ff1_9999_9999_999b);
    for name in ["POISSON.DIST", "POISSON"] {
        for cumulative in [false, true] {
            let zero_or_one = if cumulative { 1.0 } else { 0.0 };
            // d2 > LN_DBL_MAX -> #NUM! for both PMF and CDF.
            error(call(name, vec![n(1e100), n(0.5), b(cumulative)]), Num);
            error(call(name, vec![n(1e100), n(lam_11), b(cumulative)]), Num);
            error(call(name, vec![n(2.0_f64.powi(60)), n(0.5), b(cumulative)]), Num);
            error(call(name, vec![n(2.0_f64.powi(60)), n(lam_11), b(cumulative)]), Num);
            error(call(name, vec![n(1e200), n(0.75), b(cumulative)]), Num);
            // d2 < 0 or 0 <= d2 <= LN_DBL_MAX -> 0.0 (PMF) / 1.0 (CDF).
            exact(call(name, vec![n(1e100), n(0.75), b(cumulative)]), zero_or_one);
            exact(call(name, vec![n(2.0_f64.powi(60)), n(0.75), b(cumulative)]), zero_or_one);
            exact(
                call(
                    name,
                    vec![
                        n(f64::from_bits(0x4370_0000_0000_0033)),
                        n(0.125),
                        b(cumulative),
                    ],
                ),
                zero_or_one,
            );
            // Above 1.1 switches to Loader saddle-point -> 0.0 / 1.0.
            exact(call(name, vec![n(2.0_f64.powi(60)), n(lam_11_up), b(cumulative)]), zero_or_one);
            exact(call(name, vec![n(1e100), n(lam_11_up), b(cumulative)]), zero_or_one);
            // GAMMALN overflow (1e306) -> #NUM!.
            error(call(name, vec![n(1e306), n(0.5), b(cumulative)]), Num);
            error(call(name, vec![n(1e306), n(1.0), b(cumulative)]), Num);
        }
    }
}

/// Excel reads import argument errors and numeric types before validating the path.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn import_errors_precede_path_validation() {
    for (name, count) in [("IMPORTCSV", 5), ("IMPORTTEXT", 7)] {
        for index in 0..count {
            let mut args = vec![n(1.0); count];
            args[index] = e(Ref);
            error(call(name, args), Ref);
        }
    }
    error(call("IMPORTCSV", vec![n(1.0), t("bad"), e(Ref)]), Value);
    error(call("IMPORTCSV", vec![n(1.0), t("1.5"), e(Ref)]), Ref);
    error(call("IMPORTTEXT", vec![n(1.0), t(","), t("bad"), e(Ref)]), Value);
}

/// Excel 20527 wraps 2h-1 at the signed-32-bit boundary in interval variance.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_confidence_wraps_large_horizon_term() {
    let values = col(&[1.295212745666504, 1.617269515991211, 2.296365737915039, 3.238837242126465, 4.209005355834961]);
    let times = col(&[1.0, 2.0, 3.0, 4.0, 5.0]);
    exact(call("FORECAST.ETS.CONFINT", vec![n(1_073_741_829.0), values.clone(), times.clone(), n(0.95), n(0.0)]), f64::from_bits(0x4203_2db7_1374_d403));
    exact(call("FORECAST.ETS.CONFINT", vec![n(1_073_741_830.0), values, times.clone(), n(0.95), n(0.0)]), 0.0);
    let maximum = col(&[1.8903273354517296, 2.019882615073584, 2.22050576863694, 2.6246356982155703, 3.3064261186809745]);
    exact(call("FORECAST.ETS.CONFINT", vec![n(2_147_483_652.0), maximum, times, n(0.95), n(0.0)]), f64::from_bits(0x4172_796c_467e_b0f4));
}

/// Excel 20527 flushes the beta recurrence's subnormal density product.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_recurrence_flushes_its_subnormal_intermediate() {
    for (bits, a, shape_b) in [
        (0x3fe0_189b_0deb_949c, 1040.0, 13.0),
        (0x3fc3_a3ce_1e64_c95d, 381.0, 19.0),
        (0x3fd9_1699_47a6_4be1, 763.0, 8.0),
        (0x3fcc_5cb1_8794_fbc2, 474.0, 18.0),
    ] {
        let args = |x| vec![n(x), n(a), n(shape_b), b(true)];
        exact(call("BETA.DIST", args(f64::from_bits(bits))), 0.0);
        assert!(number(call("BETA.DIST", args(f64::from_bits(bits + 1)))) > 0.0);
    }
    for (x, d1, d2) in [
        (158.65828799388686, 20.0, 1000.0),
        (528.8609599796229, 6.0, 1000.0),
    ] {
        exact(call("F.DIST.RT", vec![n(x), n(d1), n(d2)]), 0.0);
    }
}

/// Excel 20527 flushes the general F right tail but retains the unit-shape subnormal.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn f_right_tail_publishes_branch_specific_subnormals() {
    for name in ["F.DIST.RT", "FDIST"] {
        for (x, d1, d2) in [
            (15139220103275.635, 10.0, 50.0),
            (5334692.97853431, 100.0, 100.0),
            (68.48512660726075, 25569.0, 436.0),
            (3844.447428079949, 1.0, 803.0),
        ] {
            exact(call(name, vec![n(x), n(d1), n(d2)]), 0.0);
        }
        exact(
            call(name, vec![n(f64::from_bits(0x46dc_48c6_001f_0a92)), n(2.0), n(20.0)]),
            f64::from_bits(1),
        );
    }
}

/// Excel 20527 validates the signed-32-bit limit after truncating count arguments.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn hypergeometric_counts_reject_the_signed_sentinel() {
    let limit = i32::MAX as f64;
    for values in [
        [limit, 1.0, 1.0, 10.0],
        [1.0, limit, 0.0, limit - 1.0],
        [1.0, 0.0, limit, limit - 1.0],
        [1.0, 0.0, 0.0, limit],
    ] {
        let args = values.into_iter().map(n).collect::<Vec<_>>();
        error(call("HYPGEOMDIST", args.clone()), Num);
        for cumulative in [false, true] {
            let mut modern = args.clone();
            modern.push(b(cumulative));
            error(call("HYPGEOM.DIST", modern), Num);
        }
    }
    let below = vec![n(limit - 0.5), n(1.0), n(1.0), n(10.0)];
    exact(call("HYPGEOMDIST", below.clone()), 0.0);
    let mut cumulative = below;
    cumulative.push(b(true));
    exact(call("HYPGEOM.DIST", cumulative), 1.0);
}

/// Excel 20527 distinguishes density from cumulative F argument overflow.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn f_distribution_reports_scaled_argument_overflow() {
    for name in ["F.DIST.RT", "FDIST"] {
        error(call(name, vec![n(1e308), n(3.0), n(2.0)]), Div0);
        exact(call(name, vec![n(1.0), n(2.0), n(2.0)]), 0.5);
    }
    error(call("F.DIST", vec![n(1e308), n(3.0), n(2.0), b(true)]), Div0);
    error(call("F.DIST", vec![n(1e308), n(3.0), n(2.0), b(false)]), Num);
}

/// Excel 20527 publishes zero for these deeply underflowed F densities.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn f_density_returns_zero_after_deep_underflow() {
    for (x, d1, d2) in [(8192.0, 365.0, 4496.0), (1024.0, 500.0, 1000.0), (1e200, 500.0, 900.0)] {
        exact(call("F.DIST", vec![n(x), n(d1), n(d2), b(false)]), 0.0);
    }
}

/// Excel 20527 reports errors at the intermediate power and density denominator.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn weibull_reports_intermediate_arithmetic_errors() {
    for name in ["WEIBULL", "WEIBULL.DIST"] {
        error(call(name, vec![n(2.0), n(2048.0), n(1.0), b(true)]), Num);
        error(call(name, vec![n(1e-100), n(10.0), n(1e-100), b(false)]), Div0);
        error(call(name, vec![n(0.0), n(360.0), n(1000.0), b(false)]), Num);
        error(call(name, vec![n(0.0), n(10.0), n(1e-100), b(false)]), Div0);
        error(call(name, vec![n(0.0), n(1023.0), n(0.5), b(false)]), Div0);
        error(call(name, vec![n(0.25), n(1023.0), n(0.5), b(false)]), Div0);
        error(call(name, vec![n(0.0), n(1020.0), n(0.5), b(false)]), Num);
    }
}

/// Excel 20625 rejects the signed 32-bit sentinel before endpoint shortcuts.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn binomial_range_bounds_the_truncated_trial_count() {
    let sentinel = i32::MAX as f64;
    let previous = f64::from_bits(sentinel.to_bits() - 1);
    let evaluate = |trials, probability, upper| {
        let mut args = vec![n(trials), n(probability), n(0.0)];
        if upper {
            args.push(n(1.0));
        }
        call("BINOM.DIST.RANGE", args)
    };
    for upper in [false, true] {
        exact(evaluate(previous, 0.0, upper), 1.0);
        error(evaluate(sentinel, 0.0, upper), Num);
        error(evaluate(sentinel + 1.0, 0.5, upper), Num);
    }
}

/// Excel 20527 validates the original count sign before truncating positive fractions.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn binomial_counts_validate_sign_before_truncation() {
    for name in ["BINOM.DIST", "BINOMDIST"] {
        error(call(name, vec![n(-0.25), n(5.0), n(0.5), b(false)]), Num);
        error(call(name, vec![n(0.0), n(-0.25), n(0.5), b(false)]), Num);
        exact(call(name, vec![n(0.25), n(5.0), n(0.5), b(false)]), 0.03125);
    }
    for args in [
        vec![n(-0.25), n(0.5), n(0.0)],
        vec![n(5.0), n(0.5), n(-0.25)],
        vec![n(5.0), n(0.5), n(0.0), n(-0.25)],
    ] {
        error(call("BINOM.DIST.RANGE", args), Num);
    }
    for name in ["BINOM.INV", "CRITBINOM"] {
        error(call(name, vec![n(-0.25), n(0.5), n(0.5)]), Num);
        exact(call(name, vec![n(0.25), n(0.5), n(0.5)]), 0.0);
    }
    for name in ["NEGBINOM.DIST", "NEGBINOMDIST"] {
        let mut args = vec![n(-0.25), n(3.0), n(0.5)];
        if name == "NEGBINOM.DIST" { args.push(b(false)); }
        error(call(name, args), Num);
    }
}

/// Excel gives range shape priority over held errors and treats error flags as VALUE.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn linest_and_logest_held_errors_follow_shape() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(1.0), n(2.0), n(4.0), n(8.0), n(16.0)]);
    sheet.column(1, 2, vec![n(2.0), n(3.0), n(5.0), n(7.0), n(11.0)]);
    sheet.column(1, 3, vec![e(Div0)]);
    sheet.column(1, 4, vec![n(2.0), e(Div0), n(5.0), n(7.0), n(11.0)]);
    let y = sheet.area(1, 1, 5, 1);
    let x = sheet.area(1, 2, 5, 2);
    let bad_cell = sheet.area(1, 3, 1, 3);
    let bad_range = sheet.area(1, 4, 5, 4);
    for name in ["LINEST", "LOGEST"] {
        error(call_on(&mut sheet, name, vec![y.clone(), x.clone(), b(true), e(Div0)]), Value);
        error(call_on(&mut sheet, name, vec![y.clone(), bad_cell.clone()]), Ref);
        error(call_on(&mut sheet, name, vec![y.clone(), bad_range.clone()]), Value);
        error(call(name, vec![e(Div0), col(&[2.0, 3.0, 5.0, 7.0, 11.0])]), Div0);
    }
}

/// Excel 20527 treats blank regression flags as false, not omitted defaults.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn regression_blank_flags_are_false() {
    let y = arr(vec![vec![2.0], vec![5.0], vec![7.0], vec![10.0], vec![13.0]]);
    let x = arr(vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0], vec![5.0]]);
    for name in ["LINEST", "LOGEST"] {
        let blank = array(call(name, vec![y.clone(), x.clone(), calc_func::value::CalcValue::empty()]));
        let false_flag = array(call(name, vec![y.clone(), x.clone(), b(false)]));
        assert_eq!(blank, false_flag);
    }
    for name in ["TREND", "GROWTH"] {
        let blank = call(name, vec![y.clone(), x.clone(), n(6.0), calc_func::value::CalcValue::empty()]);
        let false_flag = call(name, vec![y.clone(), x.clone(), n(6.0), b(false)]);
        assert_eq!(blank.unwrap().as_number().map(f64::to_bits), false_flag.unwrap().as_number().map(f64::to_bits));
    }
}

/// Excel 20527 validates ETS target coercion before history shape and options.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_target_errors_precede_shape_and_options() {
    let y = arr(vec![vec![2.0], vec![4.0], vec![6.0]]);
    let timeline = arr(vec![vec![1.0], vec![2.0]]);
    for name in ["FORECAST.ETS", "FORECAST.ETS.CONFINT"] {
        error(call(name, vec![t(""), y.clone(), timeline.clone(), e(Ref)]), Value);
        error(call(name, vec![e(NA), y.clone(), timeline.clone(), e(Ref)]), NA);
    }
}

/// Excel 20527 matrix-history and independent scalar-kind controls.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn trend_matrix_histories_reject_nonnumeric_scalar_new_x() {
    let y = arr(vec![vec![1.0, 2.0], vec![3.0, 5.0]]);
    let x = arr(vec![vec![1.0, 3.0], vec![2.0, 4.0]]);
    for new_x in [t("2"), t("bad"), b(true), e(NA), calc_func::value::CalcValue::empty()] {
        error(call("TREND", vec![y.clone(), x.clone(), new_x, e(Ref)]), Value);
    }
}

/// Excel 20527 returns the scalar prediction for numeric 2x2 paired histories.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn trend_matrix_histories_accept_numeric_scalar_new_x() {
    let y = arr(vec![vec![2.0, 4.0], vec![6.0, 8.0]]);
    let x = arr(vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
    close(call("TREND", vec![y, x, n(5.0)]), 10.0, 1e-12);
}

/// Excel 20527 validates 2x2 predictor shape before a scalar history error.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn trend_scalar_history_with_matrix_predictors_returns_ref() {
    let x = arr(vec![vec![1.0, 3.0], vec![2.0, 4.0]]);
    for y in [n(1.0), t("bad"), b(true), e(NA), calc_func::value::CalcValue::empty()] {
        error(call("TREND", vec![y, x.clone(), n(2.0), e(Div0)]), Ref);
    }
}

/// Excel 20527 returns the same paired-history results when a scalar argument is a cell.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn trend_matrix_histories_keep_scalar_cell_shape() {
    let y = arr(vec![vec![2.0, 4.0], vec![6.0, 8.0]]);
    let x = arr(vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(5.0), t("bad"), e(NA)]);
    let number = sheet.area(1, 1, 1, 1);
    let text = sheet.area(2, 1, 2, 1);
    let held_error = sheet.area(3, 1, 3, 1);
    close(call_on(&mut sheet, "TREND", vec![y.clone(), x.clone(), number.clone()]), 10.0, 1e-12);
    error(call_on(&mut sheet, "TREND", vec![y, x.clone(), text, e(Ref)]), Value);
    error(call_on(&mut sheet, "TREND", vec![held_error, x, number]), Ref);
}

/// Excel 20527 cell-backed argument and independent error-priority controls.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn rank_coerces_number_before_validating_order() {
    for name in ["RANK", "RANK.EQ", "RANK.AVG"] {
        let mut sheet = Sheet::new();
        sheet.column(1, 1, vec![n(1.0), e(Div0)]);
        let numeric = sheet.area(1, 1, 1, 1);
        let source_error = sheet.area(2, 1, 2, 1);
        exact(call_on(&mut sheet, name, vec![t("1"), numeric.clone(), n(0.0)]), 1.0);
        error(call_on(&mut sheet, name, vec![t("bad"), numeric.clone(), e(Ref)]), Value);
        error(call_on(&mut sheet, name, vec![n(1.0), numeric, t("1")]), Value);
        error(call_on(&mut sheet, name, vec![n(1.0), source_error, e(Ref)]), Ref);
        sheet.column(1, 2, vec![n(1.0), n(2.0), n(3.0)]);
        let numbers = sheet.area(1, 2, 3, 2);
        exact(call_on(&mut sheet, name, vec![n(1.0), numbers.clone(), t("tRuE")]), 1.0);
        exact(call_on(&mut sheet, name, vec![n(1.0), numbers.clone(), t("FALSE")]), 3.0);
        error(call_on(&mut sheet, name, vec![n(1.0), numbers, t(" TRUE")]), Value);
    }
}

/// Excel 20625 reports #DIV/0! when the variance overflows or is subnormal.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn standardized_moments_require_a_normal_variance() {
    for function in ["SKEW", "SKEW.P", "KURT"] {
        for values in [
            vec![n(1.0), n(2.0), n(3.0), n(1e155)],
            vec![n(1e-161), n(-1e-161), n(0.0), n(0.0)],
            vec![arr(vec![vec![f64::MAX, f64::MAX, 0.0, 0.0]])],
        ] {
            error(call(function, values), Div0);
        }
    }
    let x = f64::MIN_POSITIVE.sqrt() * 1.23;
    let values = vec![n(x), n(-x), n(0.0), n(0.0)];
    exact(call("SKEW", values.clone()), 0.0);
    error(call("SKEW.P", values.clone()), Div0);
    exact(call("KURT", values), f64::from_bits(0x3ff7_ffff_ffff_ffe8));
}

/// Excel 20527 direct and cell-range sample-count and error-precedence controls.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn skew_p_requires_three_observations() {
    error(call("SKEW.P", vec![n(1.0), n(2.0)]), Div0);
    error(call("SKEW.P", vec![n(1.0), n(2.0), e(NA)]), NA);
    exact(call("SKEW.P", vec![n(1.0), n(2.0), n(3.0)]), 0.0);
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(1.0), n(2.0), t("ignored")]);
    let range = sheet.area(1, 1, 3, 1);
    error(call_on(&mut sheet, "SKEW.P", vec![range]), Div0);
}

/// Excel 20527 ignores nonnumeric scalar pairs and admits the two adjacent values around 1.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn prob_nonnumeric_and_adjacent_singletons() {
    for x in [t("bad"), b(true)] {
        error(call("PROB", vec![x.clone(), n(1.0), n(0.0), n(1.0)]), Div0);
        error(call("PROB", vec![x.clone(), n(1.0), t("bad"), n(1.0)]), Value);
        error(call("PROB", vec![x, n(1.0), n(0.0), e(NA)]), NA);
    }
    error(call("PROB", vec![n(1.0), t("bad"), n(0.0), n(1.0)]), Div0);
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![t("bad"), n(1.0)]);
    let x = sheet.area(1, 1, 1, 1);
    let p = sheet.area(2, 1, 2, 1);
    error(call_on(&mut sheet, "PROB", vec![x, p, n(0.0), n(1.0)]), Div0);
    for bits in [1.0_f64.to_bits() - 1, 1.0_f64.to_bits(), 1.0_f64.to_bits() + 1] {
        let p = f64::from_bits(bits);
        exact(call("PROB", vec![n(0.5), n(p), n(0.0), n(1.0)]), p);
        exact(call("PROB", vec![n(0.5), n(p), n(1.0), n(0.0)]), 0.0);
        exact(call("PROB", vec![arr(vec![vec![0.5]]), arr(vec![vec![p]]), n(0.5)]), p);
    }
    for bits in [1.0_f64.to_bits() - 4, 1.0_f64.to_bits() + 22] {
        let p = f64::from_bits(bits);
        exact(call("PROB", vec![n(0.5), n(p), n(0.0), n(1.0)]), p);
    }
    for bits in [1.0_f64.to_bits() - 5, 1.0_f64.to_bits() + 23] {
        error(call("PROB", vec![n(0.5), n(f64::from_bits(bits)), n(0.0), n(1.0)]), Num);
    }
}

/// Excel 20527 automatic periods (oracle corpus dcj9k_r38_ets rows 2243, 3139, 3363 and
/// dcj9k_r39_seasonal rows 124, 212, 278, 410) that the strongest-DFT-bin detector reproduces;
/// the earlier autocorrelation-peak rule returned 4, 4, 5, 4, 8, 8 and 8.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_seasonality_follows_the_strongest_frequency() {
    let cases: [(f64, &[f64]); 7] = [
        (2.0, &[2.0, 7.0, -1.0, 4.0, 2.0, 7.0, -1.0, 4.0, 2.0, 7.0, -1.0, 4.0]),
        (2.0, &[2.0, 7.0, -1.0, 4.0, 2.0, 7.0, -1.0, 4.0, 2.0, 7.0, -1.0, 4.0, 2.0, 7.0, -1.0, 4.0, 2.0, 7.0, -1.0, 4.0, 2.0, 7.0, -1.0, 4.0]),
        (0.0, &[2.0, 3.5, 3.75, 5.75, 5.5, 7.0, 8.5, 8.75, 10.75, 10.5, 12.0, 13.5, 13.75, 15.75, 15.5, 17.0, 18.5, 18.75, 20.75, 20.5, 22.0, 23.5, 23.75, 25.75]),
        (2.0, &[33.5, 27.5, 34.75, 33.125, 37.5, 31.5, 38.75, 37.125, 41.5, 35.5, 42.75, 41.125]),
        (4.0, &[33.0, 25.5625, 27.5, 29.1875, 30.75, 29.1875, 24.0, 31.4375, 34.5, 27.0625, 29.0, 30.6875, 32.25, 30.6875, 25.5, 32.9375, 36.0, 28.5625, 30.5, 32.1875, 33.75, 32.1875, 27.0, 34.4375]),
        (0.0, &[6.75, 13.1875, 17.625, 6.6875, 20.875, 14.5625, 21.625, 16.9375, 13.25, 19.6875, 24.125, 13.1875, 27.375, 21.0625, 28.125, 23.4375, 19.75, 26.1875, 30.625, 19.6875, 33.875, 27.5625, 34.625, 29.9375]),
        (3.0, &[-2.375, 2.0, -2.625, 0.75, 2.5, -8.25, 6.75, 4.75, -3.375, 1.0, -3.625, -0.25, 1.5, -9.25, 5.75, 3.75, -4.375, 0.0, -4.625, -1.25, 0.5, -10.25, 4.75, 2.75]),
    ];
    for (expected, history) in cases {
        let timeline: Vec<f64> = (1..=history.len()).map(|v| v as f64).collect();
        exact(call("FORECAST.ETS.SEASONALITY", vec![col(history), col(&timeline)]), expected);
    }
}

/// Excel 20527 automatic periods on six to nine points (oracle corpus dcj9k_r38_ets rows 1571
/// and 3699 (its repeated timestamp averaged), dcj9k_r39_ets rows 520 and 795; eng21 probe_minlen): the strongest residual bin is
/// accepted when its power is at least 2 - 3/n times the next bin's, whatever the autocorrelation
/// at the lag (0.11 for the first history), and a bin whose period exceeds floor(n/2.5) gives 0.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_short_seasonality_is_a_dominant_bin() {
    let cases: [(f64, &[f64]); 5] = [
        (3.0, &[2.0, 3.5, 3.75, 5.75, 5.5, 7.0, 8.5, 8.75]),
        (2.0, &[2.0, 3.0, 6.0, 11.0, 13.0, 17.0, 19.0]),
        (2.0, &[13.75, -0.75, 15.125, -2.25, 0.75, 2.625, 19.0, 15.125]),
        (2.0, &[0.125, 0.8125, 3.1875, 4.1875, 6.375, 8.25]),
        (0.0, &[50.0, 60.0, 50.0, 40.0, 50.0, 60.0, 50.0, 40.0]),
    ];
    for (expected, history) in cases {
        let timeline: Vec<f64> = (1..=history.len()).map(|v| v as f64).collect();
        exact(call("FORECAST.ETS.SEASONALITY", vec![col(history), col(&timeline)]), expected);
    }
}

/// Excel 20527 automatic periods on ten to sixteen points (oracle corpus dcj9k_r38_ets row 2467,
/// dcj9k_r39_seasonal rows 377 and 135; eng24 random Touchstone probes): the dominant-bin test
/// of six to nine points, on the periodogram of the residual with its two end points weighted by
/// one half. The first corpus history has autocorrelation 0.18 at lag 2; the next two have no
/// dominant bin and keep period 4 through the autocorrelation fallback.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_seasonality_from_ten_points_weights_the_ends() {
    let cases: [(f64, &[f64]); 6] = [
        (2.0, &[2.0, 3.5, 3.75, 5.75, 5.5, 7.0, 8.5, 8.75, 10.75, 10.5, 12.0, 13.5]),
        (4.0, &[-22.375, -24.75, -25.5, -30.125, -18.875, -21.25, -22.0, -26.625, -15.375, -17.75, -18.5, -23.125]),
        (4.0, &[-1.875, 7.1875, 4.875, 0.0625, -0.75, 15.5625, 6.25, 5.9375, 4.625, 13.6875, 11.375, 6.5625, 5.75, 22.0625, 12.75, 12.4375]),
        (2.0, &[-0.9537170081, -0.4590941568, -0.5992494722, -0.3201424011, 0.7217183074, -1.7172649178, -0.3368538318, -0.4855748224, -0.8837397625, -0.1154203943]),
        (2.0, &[0.7917381339, -0.3077180444, -1.4251380232, 1.5932351316, -0.496543975, 1.160146954, -0.3373967985, 0.1434170223, -1.6621869784, -0.976628724]),
        (3.0, &[-0.0427037999, -0.967350211, 0.6502948317, 0.0695334642, 2.1551116249, 1.1870054958, -1.4836516615, 0.9267071637, -0.8581093161, -0.571764971, -0.1419870377, -1.2609273204, 3.0833177183, 0.8646780908, -0.8452438925, 1.1811306211]),
    ];
    for (expected, history) in cases {
        let timeline: Vec<f64> = (1..=history.len()).map(|v| v as f64).collect();
        exact(call("FORECAST.ETS.SEASONALITY", vec![col(history), col(&timeline)]), expected);
    }
}

/// Excel 20527 automatic periods on seventeen to twenty-three points (oracle corpus dcj9k_r39_ets
/// rows 14, 124, 168 and 476; history B at 20 and 21 points, eng23 probe_B2): the end-weighted
/// dominant-bin test; above a bin period of 5.25 the lag is the unbiased-autocorrelation maximum
/// in the bin's range, kept at 0.45; without a dominant bin a candidate of 4 or more with
/// unbiased autocorrelation 0.45 is kept (the first history, period 8).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_seasonality_from_seventeen_points_chooses_long_lags_by_autocorrelation() {
    let cases: [(f64, &[f64]); 6] = [
        (8.0, &[-0.875, 0.5, -10.25, -13.125, -4.75, -0.875, 9.5, 7.75, -17.375, -9.875, -1.375, -12.625, -13.75, -7.25, 8.875, 9.125, -15.625, 1.875, -18.625, -14.375]),
        (2.0, &[1.25, 1.0, -0.625, -5.375, -4.625, 19.75, -8.25, 15.0, -8.5, 7.5, -3.5, 3.875, 22.75, -0.125, 12.5, -2.875, -3.0, 4.125]),
        (2.0, &[-2.75, -3.625, 15.5, 0.0, 10.75, 8.75, 13.375, -7.125, -13.625, 14.125, 1.625, 15.375, 12.125, -7.5, 11.625, -9.0, 15.75]),
        (4.0, &[9.375, 7.0, 2.25, -1.0, 6.875, -10.875, 3.875, 4.0, -0.875, -16.0, -24.75, -3.25, -12.125, -27.625, -35.375, -19.625, -40.125]),
        (2.0, &[2.0, 3.5, 3.75, 5.75, 5.5, 7.0, 8.5, 8.75, 10.75, 10.5, 12.0, 13.5, 13.75, 15.75, 15.5, 17.0, 18.5, 18.75, 20.75, 20.5]),
        (0.0, &[2.0, 3.5, 3.75, 5.75, 5.5, 7.0, 8.5, 8.75, 10.75, 10.5, 12.0, 13.5, 13.75, 15.75, 15.5, 17.0, 18.5, 18.75, 20.75, 20.5, 22.0]),
    ];
    for (expected, history) in cases {
        let timeline: Vec<f64> = (1..=history.len()).map(|v| v as f64).collect();
        exact(call("FORECAST.ETS.SEASONALITY", vec![col(history), col(&timeline)]), expected);
    }
}

/// Excel 20527 automatic periods from twenty points (eng23 probe_B2: history B at 24 points from
/// its third value, and at 30 points): the dominant-bin test on the periodogram tapered over
/// floor(n/10) points at each end by 0.5 (1 - cos(pi (j + 1/2) / L)); at 24 points from the first
/// value the bin is not dominant and the answer is 0 (ets_seasonality_follows_the_strongest_frequency).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_seasonality_from_twenty_points_tapers_the_ends() {
    let cases: [(f64, &[f64]); 2] = [
        (2.0, &[3.75, 5.75, 5.5, 7.0, 8.5, 8.75, 10.75, 10.5, 12.0, 13.5, 13.75, 15.75, 15.5, 17.0, 18.5, 18.75, 20.75, 20.5, 22.0, 23.5, 23.75, 25.75, 25.5, 27.0]),
        (2.0, &[2.0, 3.5, 3.75, 5.75, 5.5, 7.0, 8.5, 8.75, 10.75, 10.5, 12.0, 13.5, 13.75, 15.75, 15.5, 17.0, 18.5, 18.75, 20.75, 20.5, 22.0, 23.5, 23.75, 25.75, 25.5, 27.0, 28.5, 28.75, 30.75, 30.5]),
    ];
    for (expected, history) in cases {
        let timeline: Vec<f64> = (1..=history.len()).map(|v| v as f64).collect();
        exact(call("FORECAST.ETS.SEASONALITY", vec![col(history), col(&timeline)]), expected);
    }
}

/// Excel 20527 reports period 8 for dcj9k_r39_ets!D564, where the strongest
/// spectral bin stands out from the full spectrum without dominating the second bin.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_seasonality_accepts_a_spectral_outlier() {
    let mut history = [
        -3.0, 13.375, -12.625, -5.5, -6.375, -12.625, -13.125, -5.625, -12.75, 3.875, 6.625,
        1.125, -17.625, -10.25, -7.75, -5.375, -1.75, -0.625, -10.0, -22.625, -29.25, -31.0,
        -29.0, -22.625,
    ];
    let timeline: Vec<f64> = (1..=history.len()).map(|value| value as f64).collect();
    exact(call("FORECAST.ETS.SEASONALITY", vec![col(&history), col(&timeline)]), 8.0);
    history[13] += 1.0;
    exact(call("FORECAST.ETS.SEASONALITY", vec![col(&history), col(&timeline)]), 0.0);
}

/// Excel 20625 reports period 11 for the sx1 outlier row at batches[15][29].
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_validates_the_longer_period_of_a_spectral_outlier() {
    let history = [
        0x3fe4e697a74d9098, 0x3ff0ec6ac35068de, 0xbff155313cb79346, 0x3fecaf93708a529f,
        0x3ff8c9bcf401a0d8, 0xbfe35ffc226d5b7c, 0xbff3d1eb3e0bb67a, 0xbffc78b7967c3a1c,
        0xbfd3701b13a7f976, 0x3ff550f4f7bcea32, 0x40035035f929cdd2, 0xbfe4e0e3ed62828d,
        0xbf949bfe7f58a2c0, 0xbfe0b091ce12544e, 0x3fffcdc3f00c4d27, 0x3fd216f8b750cb9c,
        0xbff32c08ba4ae7a8, 0xbfe83e60e676b30c, 0xbff78953e3b99fbe, 0xbff952cd9415f8d4,
        0x400768d8afd3f304, 0x40027f36537c24ae, 0xbff68cd1a323bbb4, 0xbfd6296bc2ce2630,
        0xc0001e16a6a2234e, 0x3fe454522b89033c, 0x3ffff299ca8c1c13, 0x3fa50c8778861a10,
    ].map(f64::from_bits);
    let timeline: Vec<f64> = (1..=history.len()).map(|value| value as f64).collect();
    exact(call("FORECAST.ETS.SEASONALITY", vec![col(&history), col(&timeline)]), 11.0);
}

/// Excel 20527 AUTOPERIOD seasonality detector (dc-iuwct.56): two-segment
/// piecewise-linear ACF hills compared against flanking slope, short-period tail
/// slope checks (p = 3, 4, 5), and raw-series fundamental tie-breaking across
/// sx1 discrepancy cases and oracle corpus dcj9k_r39_ets!D630.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_seasonality_autoperiod_hills_and_tie_break() {
    let cases: [(f64, &[u64]); 6] = [
        // sx1/run3-ets-acf-raw.jsonl batches[10][116] (n=21, Excel=4)
        (
            4.0,
            &[
                0x3f6bc8a8bf3f7ce0, 0xbfd28f6ed63a07d0, 0xbff4b6c1fd72d04a, 0xbfff3962065d774f,
                0x3fb3321784c8d86a, 0x3ff01592cfd65474, 0xbff715dcb8f52696, 0xc0027f9e7d14289b,
                0x3fcb1c592a68f29f, 0xbfcd5170361491ce, 0x3fe6026f4e35f596, 0xbff2510ad5c2090d,
                0x3fee2ef3eae27a04, 0xbfcac3e1a82c2c44, 0x3fbfed24d32fc918, 0xbfd1b38c60f103e9,
                0x40007330946176d5, 0xbfca310914589c05, 0xbff1b15ded06c3d6, 0x3fe74c4198d6f4ae,
                0x3ff96e5359080fe1,
            ],
        ),
        // sx1/run2-ets-boundary-raw.jsonl batches[5][162] (n=18, Excel=0)
        (
            0.0,
            &[
                0x3fb1f1323bb84580, 0x4014b282c1844fc4, 0xc02ebf148451e2b7, 0x401ec5edd192a688,
                0xc01f000000000003, 0x3fedd091736acbc0, 0xc01d01d6f75c3a8e, 0x3fe26be9f3dd81b8,
                0xc02ea3e26477708a, 0xc0058f8991ddc228, 0x4019cd7d3e7bb037, 0x400bfc5211478ae5,
                0xc034b17b7464a9a2, 0xc020400000000001, 0x4033317b7464a9a2, 0xc0213f148451e2b7,
                0xc032f35f4f9eec0f, 0x4024e3e26477708b,
            ],
        ),
        // sx1/run2-ets-outlier-raw.jsonl batches[12][9] (n=26, Excel=0)
        (
            0.0,
            &[
                0x3f128867effff800, 0xc007eee6d9d92fa9, 0xc0034af4366dfa6c, 0x3fcd48d49a0ba12a,
                0xbffd071d4a3ea545, 0xbfe08567b12fd981, 0x3fec0e1b97fec966, 0x4002b84b1621add7,
                0x3ffc337def16df09, 0x3ff28d852cce0dbc, 0x40009686739a8896, 0xbffb110bd2b13c6b,
                0xc000a2f375f60fd9, 0xbff495e03d1c0418, 0xbffe5443da879c68, 0xbff2780302d93edd,
                0xbfbcb32f5bd74b64, 0xbff06a250a9e7540, 0x4005d0abc53a2d8a, 0x4008ad69a8d7124b,
                0x4002b91c809f4048, 0x3ff405c8a8d4a63e, 0x3fe7d04c36153a9d, 0x3ff5bab70e6884f8,
                0x3fe4565a89751e9b, 0xbfb1f513b296d5d4,
            ],
        ),
        // sx1/run4-ets-phase-replies.jsonl $[1].rows[136] (n=14, Excel=4)
        (
            4.0,
            &[
                0x3f5c79e5d7e23f80, 0xbfe116b105a328a9, 0xbfe66549e5e806ab, 0x3fdb0a1c289cf581,
                0x3ffb25a87cde5cf1, 0x3fd404eb904c2d9f, 0xbff6b3eeab166bf9, 0xbf898f3132fddc8c,
                0x3fea33220e860f51, 0xbfc45d09c941b47a, 0xbfdc221b00580415, 0xbf99ecefd5b5d520,
                0x3f788ef8a1b3aec0, 0x3f9f8f55d8ed295c,
            ],
        ),
        // sx1/run2-ets-outlier-raw.jsonl batches[3][170] (n=21, Excel=4, zero-left-flank + [A..B] peak)
        (
            4.0,
            &[
                0x3fe3f9d30e82a61b, 0xbfc34659335b12f8, 0xbfb80eb112768f98, 0xbfead97616c3b579,
                0x3f8940bf91b28000, 0x400159b234512a30, 0x3ff360731819c264, 0xc000877f160280b0,
                0xc002a9b8533a07b6, 0x3fd0fb71a649f918, 0x400a3ad33ea4fe88, 0xbfeed0dde71332cb,
                0xc00339b7fa3c6524, 0xbfec8d08542251d8, 0x400b1f29d38d2187, 0x3ffb299c65f0bee2,
                0xc00367fd1ee6ecad, 0xc0003655fc479974, 0x3ff19a1af4315d79, 0x4009f41f1ea5368d,
                0x400199b5cf57576a,
            ],
        ),
        // sx1/run3-ets-acf-raw.jsonl batches[6][60] (n=16, Excel=3, short-window p=3)
        (
            3.0,
            &[
                0x3fef66fe15e23878, 0x3ff0c048fe453a7a, 0xc00a2a164e0c9972, 0x3fef5aa81e882ade,
                0x3ffee99756430f53, 0xbfe9efad647a1e28, 0xbfed2e522c10648d, 0x3ff4b400bce1f14f,
                0x3ff10d4e16e4ff18, 0xbffacd0f4aa00cb5, 0x3fb46cc30c2ec43c, 0x3ffa8bb78efe44a1,
                0xbff9c86e7de715a5, 0xbff698c149813666, 0x3ff31d6ec9515500, 0x3fecd17e72bcb1b7,
            ],
        ),
    ];
    for (expected, bits) in cases {
        let history: Vec<f64> = bits.iter().map(|&b| f64::from_bits(b)).collect();
        let timeline: Vec<f64> = (1..=history.len()).map(|v| v as f64).collect();
        exact(call("FORECAST.ETS.SEASONALITY", vec![col(&history), col(&timeline)]), expected);
    }
    // Oracle corpus dcj9k_r39_ets!D630 (n=18, Excel=0; u[5] <= u[6] rejects lag 4).
    let d630 = [
        -4.625, 11.5, -12.875, 0.5, -7.75, 8.125, -9.75, -5.75, -10.625, 2.0, 0.125, 1.0, -13.5,
        -8.125, 12.0, -6.125, -12.625, 5.75,
    ];
    let timeline: Vec<f64> = (1..=d630.len()).map(|v| v as f64).collect();
    exact(call("FORECAST.ETS.SEASONALITY", vec![col(&d630), col(&timeline)]), 0.0);
}

/// Excel 20527 / 20625 AUTOPERIOD two-regime spectral competition, short-window
/// p=3/4/5 validation, R-1 right-padding trim, doubled-cycle tail rescue, and
/// valley subharmonic recovery (Carbon cb24, dc-j9k.68): 12 sx1 bank cases and
/// 2 live Touchstone 16.0.20625.20108 controls that failed before the five rules.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_seasonality_cb24_five_rules_match_excel() {
    let cases: [(f64, &[u64]); 14] = [
        // sx1/run2-ets-outlier-raw.jsonl batches[17][74] (n=32, Excel=3, before=4)
        (
            3.0,
            &[
                0xbfc79d977aa1d008, 0xbfe56b57c1eaafee, 0x400232bfadb41be0, 0x3ffcf43c25af5940,
                0xbfeea969c08ead00, 0x3fd6d5dec0a48fac, 0x400750e772b9a613, 0x3fd684da07e2307b,
                0xc0081e10759df347, 0xbfeede9ece17fbdc, 0xbfcd321c3ae2a4e6, 0xc001329236024185,
                0xc003c9b2c3dc6f73, 0x3fec03137ba02eca, 0x3fcd3cb16cc1f758, 0xbffe66d14ddb7194,
                0xbff7372c79bee4fc, 0x3fe82a7a17708a8c, 0xc001fb929c59602e, 0xc004bd0db9c70840,
                0xbfe1bd305fe74974, 0xbff3b06e6673509a, 0xc001fdfd2136f8d3, 0x3fa1da58393ba4f4,
                0x4005188993c60629, 0xbf8f15ba63e31be0, 0xbff2c7c06363543d, 0x4004c6ff70566e44,
                0x3fefcf07204248aa, 0xbff554064c993404, 0x3fff747dd12ab100, 0x40000e60f7700664,
            ],
        ),
        // sx1/run2-ets-outlier-raw.jsonl batches[14][109] (n=28, Excel=3, before=0)
        (
            3.0,
            &[
                0x3fe74d56dcaa1561, 0x3fb945380de8fd00, 0xbfe69a7e683532e2, 0xbff7b9bf2efe6be0,
                0x3fce10749e6fe950, 0xbfffbebf6d629106, 0xc00b84b8b0a2d808, 0xbfd0894e4ea07f5a,
                0xbfd9afbd5d39b44c, 0xbfdd56c0a33029aa, 0x3fe4e71dc5ecc3eb, 0x40036a207152036c,
                0x3fee74b6d2cd4d5b, 0x3fbad1ac6b793d80, 0x40019c418b94573c, 0x4009f4dbd6ec2e74,
                0x3fe6d85471656764, 0x3ff3584176950400, 0x400bccb3b58669e0, 0x3fd72ba941a78b08,
                0xbfbfd86ad4b63330, 0x400254044e814fa4, 0x3ffcad2063c1721e, 0xbffad696538204c2,
                0xbff80b5a714d93f2, 0x3fee0c9c0c400bd1, 0xbff2bb90e319859a, 0xc0131f8508318483,
            ],
        ),
        // sx1/run3-ets-acf-raw.jsonl batches[7][48] (n=17, Excel=0, before=5)
        (
            0.0,
            &[
                0xbfc2fb93024e8e0c, 0x3ff00c89c94c11aa, 0x3fe340e716a7373c, 0xc0028efda0a4476f,
                0x3fe4c91ccdd898e1, 0x3fd653d852257610, 0xbfc885620a3091f0, 0x3fc23e97d3fe06ba,
                0xbffea2b648062cea, 0x3fb6b768a5047d54, 0x3fcffe8ce424b992, 0x3fdc37f9abfcb41a,
                0x3fe7e1c00993befe, 0xc0077cab7362ebd4, 0xbfe3b71553db6ad4, 0xbfd817f3ecef8f8a,
                0xbfdf903b2934b12a,
            ],
        ),
        // sx1/run3-ets-acf-raw.jsonl batches[3][126] (n=18, Excel=0, before=4)
        (
            0.0,
            &[
                0xc022688fd64d8e72, 0x4031c2664d14f727, 0xc031e2c3d399a3c5, 0xc0153512b721912a,
                0xc032391ce8ed9480, 0x3ff4d8d72ec06814, 0x3fc01ef098608380, 0xc0122ad30e3b3295,
                0xc0334cd56e0d17ac, 0x402a598afc7582ab, 0x3fe1d7e686b653b9, 0x3fe7f33479f60164,
                0xc024eb67cfa3ee00, 0x40158fd71c512fd4, 0x4026e351c5972f28, 0xc0245a654fb2fc46,
                0xc01e7e0013cfb249, 0x400a37c2dea33d3b,
            ],
        ),
        // sx1/run2-ets-outlier-raw.jsonl batches[17][202] (n=32, Excel=0, before=6)
        (
            0.0,
            &[
                0xbfed40dd65656056, 0xbfe5172eaa05bedd, 0x3fd47f29fd607028, 0xbfd8c9dfe278c809,
                0x3fdca810d2099f2c, 0xbff20b77f63fcdca, 0xbff5ec4c2c7d17ee, 0xbff08e8f9d10cc29,
                0xbf99ad2f1ccd7ee0, 0xbff7884dad9f2b8b, 0x40079fb4b4c000a8, 0xc001eda0b63b4fd1,
                0xbffb779035d4ac25, 0x3fad3b67217def60, 0x3ff22566534a3c82, 0xc007b55abe42f678,
                0x3ff87a4f8df111cc, 0xbff79d6c789c40f5, 0x3fe8597c66e452da, 0xbfc443eed9f96060,
                0xbfe216994888d930, 0xbfeab4c4ee4fd5f7, 0xbfb7b8fd46b6dbee, 0xbfda40bfd052c56c,
                0x3ff54ed65f89e925, 0x3ffd14dfcb35a6b8, 0xc005e21e1b262e44, 0xbfe038544759922f,
                0x3fe08969a89c3ec3, 0x3fc0ba0439ca8df0, 0x3fc5bdd81d732940, 0xbff4202707aa4a80,
            ],
        ),
        // sx1/run3-ets-acf-raw.jsonl batches[6][64] (n=16, Excel=4, before=0, R-1 trim)
        (
            4.0,
            &[
                0x3fe684e1ab5d946e, 0x3fea37e7282a1042, 0xbfe9a57950ae6eb5, 0x3fb11da4276606e8,
                0xbfc46d35d34d3379, 0x4000c4d45cce5aa3, 0x3fd55692396959e5, 0x3ff0a36f9acefb0e,
                0xc003239c3e85f226, 0x3fe7f96ab9f7757e, 0x3ff1d16b6d2ef91e, 0x3fdbcf63d0d501cd,
                0xbff86f50bf1564c4, 0x3fcc9a8dea82dd91, 0x3fed92f0aa35e8e0, 0xbfc74d9d6eb6c266,
            ],
        ),
        // sx1/run4-ets-phase-replies.jsonl $[3].rows[10] (n=14, Excel=5, before=0, R-1 trim)
        (
            5.0,
            &[
                0x3fe6db71488f54a7, 0xbff8ef80c2dc1e53, 0xbfe1be6c16de3d1d, 0x3fdc76617658634a,
                0x3fbf139859f6119b, 0x3ff63c04bdc500cb, 0x3fe5ef04e754e519, 0xbfff395866312e02,
                0x3fd05f05fa3cb752, 0x3ff455085269eb41, 0xbf77c6248ecaff72, 0x3fd4edf363ba1511,
                0xbff3ab32cb519c1c, 0x3fb72afc1889c11d,
            ],
        ),
        // sx1/run3-ets-acf-raw.jsonl batches[6][155] (n=16, Excel=5, before=0, doubled-cycle rescue)
        (
            5.0,
            &[
                0x3ff89677e80e7700, 0x4002c56817e4f77a, 0xbff0fed3a5711f54, 0xc00659472582ca34,
                0x3fe8d368b824069f, 0x3ff072d9be4567c3, 0x3fd51e3b471eacd0, 0x3fdd9839cece4e14,
                0xbff1d07703e06144, 0x3fe2b4c0edb47d53, 0x400295fecb038adc, 0x3fe23d834ed1cbab,
                0xbff0de52c03363ef, 0xbfe424094d9fccb8, 0x3ffb8b8e2e8095aa, 0x3ff26834a3cc0012,
            ],
        ),
        // sx1/run2-ets-outlier-raw.jsonl batches[11][138] (n=26, Excel=5, before=0, doubled-cycle rescue)
        (
            5.0,
            &[
                0xbfffcbc01c499d95, 0xbff0e0ad48472077, 0xbfe77b420b581d8f, 0x4008474f3aca765c,
                0x3ffcfe1aa67243c9, 0xbfd5239d7d488878, 0xbfe1dbca4699dd12, 0xbffd31bd0f162b81,
                0x3ff8dc4544426d3a, 0x4005feb5f1865798, 0xbfe84bdf9133285e, 0xbfe3ae1ce786f031,
                0x3fe888b44f6ab820, 0x3fc4a7df1604e560, 0xc0082ba8cfea7c32, 0xbfd30f6dd274b562,
                0x40024ceead7475b2, 0x401035c4abaa61e6, 0xbfb28b095cfa6adc, 0xc0089a22402be44e,
                0xc003535f88306bcd, 0x3ffdfcfed2331b0d, 0x400255aa26023686, 0x3fe6d3d732fd2828,
                0xbfe9839eafbb03f4, 0xbfd9ee7050d4be32,
            ],
        ),
        // sx1/run2-ets-outlier-raw.jsonl batches[13][202] (n=27, Excel=9, before=4, valley subharmonic)
        (
            9.0,
            &[
                0x3fef3369113393e0, 0xbfd12a52e82cea98, 0xbfd6c1dbd5a6aa40, 0x3ff9bb0de170aa9f,
                0x3ff4713368af7de4, 0xbfe22b9008184b29, 0xbfeeb8b6def6d8df, 0xbff4b63d27d6bac9,
                0x3ff4804712e6d229, 0x3ff073fe5b351a02, 0xbff2cb2624df466f, 0xc0024460bf5b8121,
                0x40008eb931ed256e, 0x3fe2234d5cfbfa05, 0xbfe171fe9af94362, 0x3fdf81f1215ae09c,
                0x3ff584124db54192, 0x3fffaf7fa887197b, 0x3fe4604ea886f56d, 0xc0015237334d47a8,
                0xbff5fa43f6a54aa6, 0x3ffa50d93a555cca, 0x3fe6256f329c53ea, 0xbfde3a421ac3b647,
                0xc00b7b93460974f4, 0xbfef87e7c7f144ea, 0x3ffaee2630928d91,
            ],
        ),
        // sx1/run2-ets-outlier-raw.jsonl batches[3][55] (n=21, Excel=7, before=3, valley subharmonic)
        (
            7.0,
            &[
                0x3ff137c85a97c51f, 0xbfc175b2ae1f97e0, 0xc0097cbc8005bbaa, 0xbfe888160451d175,
                0x3ffdf73c8e95a2e2, 0xbfee0a3a40e37a22, 0xc00802cfaa8b482a, 0x3fecbc2c3dceab36,
                0x3fee03933bb2827b, 0xbffb361beea38cf8, 0x3f8ed1606953bfc8, 0x4000f664ac283f72,
                0xbffb39246f4c28c8, 0xc004117154bbc201, 0x4004c8264a4d00f0, 0x4003d832993af73e,
                0xc003b806e1b960e2, 0xc007cf0e6e25a853, 0x40045dd9d1183eab, 0xbfee1117e15f3006,
                0xbff010d2cc747496,
            ],
        ),
        // sx1/run2-ets-outlier-raw.jsonl batches[2][176] (n=20, Excel=6, before=5, valley subharmonic)
        (
            6.0,
            &[
                0xc00c0e4bc131046c, 0xc000159439b41bbe, 0xbff8a5708bedd11a, 0x3fc47536575b5e38,
                0x3ff365190e264ab4, 0xbff1099c6a7b6dfd, 0xc012666781134e18, 0xbfd7e2d97d32712a,
                0x3ff2565b9347554c, 0x40013acfaea99353, 0x3ff6458ad967f25a, 0xbfeda2d47f6a92de,
                0xc0059fa44be2322a, 0x40038a377ea95360, 0x3ff596f6f482bb06, 0x3fe32bec4de5c2f6,
                0xbfd67f724efbf23c, 0xbff412c5bd31ac20, 0x3fe8a2ff6bf4eab3, 0x4005ded9979926c6,
            ],
        ),
        // Touchstone 16.0.20625.20108 cb24-s1-090 (n=22, Excel=8, before=7, ACF>=6 suppresses outlier)
        (
            8.0,
            &[
                0x3fe5eb47e1f10344, 0x4000e322f20fcdeb, 0x3ff1dab196023fb4, 0x3fb1752e5f23561e,
                0xbff3fbd5c5fd7bba, 0xbfd52312c417a07e, 0xbfda722a4393b90c, 0x3fed5cd43f565320,
                0x4000c60e3a9903d7, 0x3ffecfef95f5558d, 0x3ffed7506f53a8e1, 0x3ffa452bb58ec6d7,
                0xbfd9dbb03fe8da0a, 0x3fe13b2d91246aaf, 0x3fe4d4e661e21978, 0x3ff612d6bf357f07,
                0x40118c768d55d4f8, 0x400591248eecc60f, 0xbfe1d725927aad3e, 0x3fe76f9a32f791f8,
                0xbfc0fa9ee6b74b40, 0x3fdd498691b5a24e,
            ],
        ),
        // Touchstone 16.0.20625.20108 cb24-s1-120 (n=18, Excel=4, before=0, R-1 trim / rescue)
        (
            4.0,
            &[
                0x3fb0fecddbd3f7a0, 0x3fe464eefc8dfe7b, 0xbfe00bcb9eb55138, 0xbff2b627d939b197,
                0xbff81fb9e0c5374a, 0xc0005cc3d778cf6c, 0x3fe9c8a16cb9a0e2, 0x3ff884320cc4a7a0,
                0xc010489a37526b9e, 0x3ff2227b32bab3a0, 0x4006d9dd8dec03a7, 0xbfdf3b861feade8b,
                0xc016cd6432c9281a, 0x3ff0b2ee182ca77a, 0xbfc0d3ff1d70aad0, 0x3fd8cd0e3249fdd7,
                0xbff5bf5d5f8ee552, 0xbffb9380cb438c65,
            ],
        ),
    ];
    for (expected, bits) in cases {
        let history: Vec<f64> = bits.iter().map(|&b| f64::from_bits(b)).collect();
        let timeline: Vec<f64> = (1..=history.len()).map(|v| v as f64).collect();
        exact(call("FORECAST.ETS.SEASONALITY", vec![col(&history), col(&timeline)]), expected);
    }
}

/// Excel 16.0.20625.20108 ACF period-3 validation across n=12..28 (dc-j9k.106, rw4 item 6):
/// - n=15 requires 2nd-cycle confirmation just like n>=16 (nl25-s1-031 Excel=0, nl25-s1-023 Excel=3);
/// - p_bin=4 for n=15..27 validates pk=3 via the doubled window [max(5, 2a)..=2R] with 2R < n
///   and a >= 2 (nl25-s1-037 n=18, nl25-s2-003 n=22, nl25-s2-007 n=27 Excel=3; nl25-s2-006 n=24
///   Excel=0 where u[5] > u[7]);
/// - doubled-window argmax must land on lag 7 (pk2 / 2 == 3 && pk2 == 7, not just u[7] > u[6]
///   && u[8] > u[9]; nl25-s1-038 n=16 and nl25-s1-047 n=28 Excel=0);
/// - n=11..14 returns 0 (a = 1 < 2 for p_bin=3, 2R >= n for p_bin=4; nl25-s1-017 Excel=0).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_seasonality_p3_doubled_window_rules_match_excel() {
    let cases: [(f64, &[u64]); 9] = [
        // Touchstone 16.0.20625.20108 nl25-s1-031 (n=15, p_bin=3, ok2=false -> Excel=0)
        (
            0.0,
            &[
                0x3fff1c0000000000, 0xbff9940000000000, 0xbff4bc0000000000, 0x40004e0000000000,
                0xbfc8a00000000000, 0xbff58c0000000000, 0x3fe6300000000000, 0x3fe1f80000000000,
                0x3f80000000000000, 0xbfb9000000000000, 0xbfedf80000000000, 0x3fee880000000000,
                0x3fd8500000000000, 0xbfebd00000000000, 0x3ff4680000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s1-023 (n=15, pk=3, pk2=7 -> Excel=3)
        (
            3.0,
            &[
                0xbff1c80000000000, 0xbfe0700000000000, 0x3fed800000000000, 0x3fe6480000000000,
                0xbffe680000000000, 0x3fd4900000000000, 0x3ff1840000000000, 0xbff5e40000000000,
                0xbff3400000000000, 0x3ff52c0000000000, 0xbfd0000000000000, 0xbfd0d00000000000,
                0xbfad000000000000, 0xbf9d000000000000, 0xbfe7e00000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s1-037 (n=18, p_bin=4 only, pk=3, pk2=7 -> Excel=3)
        (
            3.0,
            &[
                0xbff3a80000000000, 0xbff2a80000000000, 0x3ff08c0000000000, 0xbfd1f00000000000,
                0xbfeef80000000000, 0x3fe9f00000000000, 0x3fe7100000000000, 0xbfe6480000000000,
                0x3fe1280000000000, 0x3ff2d40000000000, 0xbfba800000000000, 0xbfb9800000000000,
                0x3fdea00000000000, 0x3fc3000000000000, 0xbff6100000000000, 0xbffab40000000000,
                0xbfcea00000000000, 0xbff0780000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s2-003 (n=22, p_bin=4 only, pk=3, pk2=7 -> Excel=3)
        (
            3.0,
            &[
                0x3ff3a80000000000, 0x3fc8e00000000000, 0x3ff14c0000000000, 0x3ffe4c0000000000,
                0xbfcee00000000000, 0x3fe4c80000000000, 0x3ff4dc0000000000, 0xbfd8200000000000,
                0xbff2d80000000000, 0xbfe5580000000000, 0xbfc6800000000000, 0xbff7800000000000,
                0xbffb580000000000, 0xbff0600000000000, 0xbfe5780000000000, 0xbff88c0000000000,
                0xbfeb080000000000, 0x3feb600000000000, 0xbfe1b00000000000, 0x3fc2c00000000000,
                0x4000a60000000000, 0x4000900000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s2-007 (n=27, p_bin=4 only, pk=3, pk2=7 -> Excel=3)
        (
            3.0,
            &[
                0xbff5d40000000000, 0xbff8380000000000, 0x3fe2a00000000000, 0x3f8c000000000000,
                0xbff5980000000000, 0x3fd8000000000000, 0x3ff22c0000000000, 0xbfbdc00000000000,
                0x3fc3e00000000000, 0x3ff0f00000000000, 0x3ffde00000000000, 0x3fe8000000000000,
                0x3ff80c0000000000, 0x4001b80000000000, 0x3ff5400000000000, 0x3fd0400000000000,
                0x3ff10c0000000000, 0x3fe8c80000000000, 0xbfdde00000000000, 0xbff3400000000000,
                0x3fb5800000000000, 0xbfd1100000000000, 0xbff58c0000000000, 0xc002820000000000,
                0xbfc3800000000000, 0xbfe7000000000000, 0xc001580000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s2-006 (n=24, p_bin=4 only, pk=3, u[5] > u[7] -> Excel=0)
        (
            0.0,
            &[
                0x3ffeb00000000000, 0x40013e0000000000, 0x3ff0680000000000, 0x3fe2d80000000000,
                0x3fdb800000000000, 0x3ff0680000000000, 0xbfe7900000000000, 0xbff49c0000000000,
                0x3fc6800000000000, 0xbfeb580000000000, 0xbffa940000000000, 0xc002560000000000,
                0xbfd5700000000000, 0xbfe4e00000000000, 0xbffc200000000000, 0xbfe3f80000000000,
                0x3fdf300000000000, 0xbfdac00000000000, 0xbff0580000000000, 0xbf88000000000000,
                0x4001740000000000, 0x3fe7400000000000, 0x3fe3800000000000, 0x4000620000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s1-038 (n=16, u[7]>u[6] && u[8]>u[9] but pk2!=7 -> Excel=0)
        (
            0.0,
            &[
                0xbff89c0000000000, 0x3ffea00000000000, 0x3fa3800000000000, 0xbff4540000000000,
                0x3fe9800000000000, 0x3ff3d40000000000, 0x3ff0440000000000, 0xbff2800000000000,
                0x3fbc400000000000, 0x3ff2e40000000000, 0xc0024e0000000000, 0xbfecd00000000000,
                0x4001440000000000, 0xbff5f40000000000, 0xbfd4200000000000, 0x3fdb000000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s1-047 (n=28, u[7]>u[6] && u[8]>u[9] but pk2!=7 -> Excel=0)
        (
            0.0,
            &[
                0x3fdee00000000000, 0xbfeb200000000000, 0x3fd2700000000000, 0x3fddb00000000000,
                0x3fac800000000000, 0x3fd2e00000000000, 0xbfdae00000000000, 0x3fe0980000000000,
                0x3fe9e80000000000, 0xc0000e0000000000, 0x3ff3c00000000000, 0x3ff55c0000000000,
                0xc002ac0000000000, 0xbfe4380000000000, 0x3ff3f00000000000, 0xbfee000000000000,
                0xbfe9e80000000000, 0xbff7fc0000000000, 0x3fd6e00000000000, 0x3fd1100000000000,
                0xbff20c0000000000, 0x4002280000000000, 0x3feff00000000000, 0xc0019a0000000000,
                0x3fee700000000000, 0x3ff9340000000000, 0xbfba000000000000, 0xbf93000000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s1-017 (n=12, p_bin=3 a=1, pk2=7 -> Excel=0)
        (
            0.0,
            &[
                0xbfdda00000000000, 0x3feb080000000000, 0xbfd0100000000000, 0xbff8640000000000,
                0x3fdae00000000000, 0x3fe0000000000000, 0xbfdb400000000000, 0xbfd8a00000000000,
                0x3feab00000000000, 0xbfa0800000000000, 0xbfeff00000000000, 0x3fe3280000000000,
            ],
        ),
    ];
    for (expected, bits) in cases {
        let history: Vec<f64> = bits.iter().map(|&b| f64::from_bits(b)).collect();
        let timeline: Vec<f64> = (1..=history.len()).map(|v| v as f64).collect();
        exact(call("FORECAST.ETS.SEASONALITY", vec![col(&history), col(&timeline)]), expected);
    }
}

/// Excel 16.0.20625.20108 unified doubled-window tail rescue with no fitted length cutoffs
/// (dc-j9k.107, rw4 item 7): doubled-window confirmation applies whenever a >= 2 && 2 * right < n,
/// with no u[pk2] >= 0.45 guard:
/// - pk=4 at n=15 (2 * right = 12 < 15) rescues both u[pk2] >= 0.45 (nl25-s1-062) and < 0.45
///   (nl25-s1-064), while n=12, 14 (2 * right = 14 >= n) return 0 (nl25-s1-052, nl25-s1-060);
/// - pk=5 at n=16, 18, 19 rescues u[pk2] < 0.45 without n >= 4 * pk (nl25-s1-088, 092, 095);
/// - p_bin=5 alone at n=16 (2 * right = 16 >= n) returns 0 (nl25-s2-014, nl25-s2-015), while
///   n=18 (2 * right = 16 < 18) rescues 5 (nl25-s2-017).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_seasonality_unified_doubled_window_rescue_match_excel() {
    let cases: [(f64, &[u64]); 10] = [
        // Touchstone 16.0.20625.20108 nl25-s1-062 (n=15, pk=4 rescue, u[8]=0.81 -> Excel=4)
        (
            4.0,
            &[
                0x3fed000000000000, 0xbfd3b00000000000, 0xc000c00000000000, 0x3fdcf00000000000,
                0x3fc5400000000000, 0x3fe4400000000000, 0xbfe3800000000000, 0x3ff0740000000000,
                0xbfc8800000000000, 0xbfcf600000000000, 0xbffcfc0000000000, 0x3ff6ec0000000000,
                0x3fe1180000000000, 0x3fe4b80000000000, 0xbfe9080000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s1-064 (n=15, pk=4 rescue, u[8]=0.27 < 0.45 -> Excel=4)
        (
            4.0,
            &[
                0x3fe0200000000000, 0x3fbd800000000000, 0x3fc0600000000000, 0xbfe3100000000000,
                0x3fcec00000000000, 0x3fe7180000000000, 0xbfe2c00000000000, 0xbfc4800000000000,
                0xbfe7000000000000, 0x3fffd40000000000, 0xbffd100000000000, 0xbfe2380000000000,
                0xbff08c0000000000, 0x3ffca00000000000, 0xc000140000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s1-088 (n=16, pk=5 rescue, u[10]=0.33 < 0.45 -> Excel=5)
        (
            5.0,
            &[
                0xbfd1400000000000, 0xbfc9200000000000, 0xbfed580000000000, 0x3fd8b00000000000,
                0x3ff5dc0000000000, 0xbfede80000000000, 0x3fc4000000000000, 0xbffa2c0000000000,
                0x3ff41c0000000000, 0x3fd6800000000000, 0xbffd100000000000, 0xbff1500000000000,
                0xbfd6200000000000, 0x3fd9600000000000, 0x3fd3a00000000000, 0xbfd4700000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s1-092 (n=18, pk=5 rescue, u[10]=0.28 < 0.45 -> Excel=5)
        (
            5.0,
            &[
                0xbfb5400000000000, 0x3fc7e00000000000, 0x3fd2300000000000, 0xbfc2a00000000000,
                0xbff6740000000000, 0x3fe6b00000000000, 0x3ff0580000000000, 0xbfd3d00000000000,
                0x3fd5700000000000, 0xbff8fc0000000000, 0xbf93000000000000, 0x3feb880000000000,
                0xbff1540000000000, 0xbff69c0000000000, 0xbfc9200000000000, 0xbfe0c00000000000,
                0x3ff2dc0000000000, 0xbfe0280000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s1-095 (n=19, pk=5 rescue, u[10]=0.42 < 0.45 -> Excel=5)
        (
            5.0,
            &[
                0x3fb2c00000000000, 0xbfa6800000000000, 0x3ff5a40000000000, 0xbfe7500000000000,
                0xbfe4900000000000, 0x3ff3d80000000000, 0xbfcfa00000000000, 0x3ff6080000000000,
                0x3fd3900000000000, 0xc002600000000000, 0x3fe7500000000000, 0x3fe7000000000000,
                0x3ff0ec0000000000, 0xbfe9700000000000, 0xbffcec0000000000, 0x3feb500000000000,
                0x3fc8e00000000000, 0x3fb2400000000000, 0xbff79c0000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s2-014 (n=16, p_bin=5 only, 2*right=16 == n -> Excel=0)
        (
            0.0,
            &[
                0xbfd5200000000000, 0xbfee680000000000, 0x3ff4280000000000, 0x3feee00000000000,
                0xbfd2700000000000, 0x3fa4000000000000, 0xc001a40000000000, 0x3fe7200000000000,
                0x3fd2700000000000, 0x3fc0600000000000, 0xbfc0400000000000, 0xbffb140000000000,
                0x3feb000000000000, 0x3fc8400000000000, 0xbf99000000000000, 0xbfe7b00000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s2-015 (n=16, p_bin=5 only, 2*right=16 == n -> Excel=0)
        (
            0.0,
            &[
                0xbfe1a00000000000, 0xbfe4f00000000000, 0x3ff9c80000000000, 0x3fd9200000000000,
                0xbff9b40000000000, 0xbfe4b80000000000, 0xbfe5a00000000000, 0x40002e0000000000,
                0xbfb0400000000000, 0xbfebd00000000000, 0x3fe2e00000000000, 0xbfe2380000000000,
                0x3ff6380000000000, 0x3fd8600000000000, 0xbfe5880000000000, 0xbf94000000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s2-017 (n=18, p_bin=5 only, 2*right=16 < 18 -> Excel=5)
        (
            5.0,
            &[
                0x3ffa380000000000, 0x3fa3000000000000, 0x3fd8e00000000000, 0xbff2880000000000,
                0xbffa980000000000, 0x3ffd180000000000, 0x3fc3600000000000, 0x3fd4b00000000000,
                0xbfc5400000000000, 0xbff3440000000000, 0x4003980000000000, 0x3fc5600000000000,
                0xbfd1200000000000, 0xbff2900000000000, 0xbffb4c0000000000, 0x4002c20000000000,
                0x3fd8300000000000, 0xbfd5600000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s1-052 (n=12, p_bin=4 2*right=14 >= 12 -> Excel=0)
        (
            0.0,
            &[
                0xbff2c80000000000, 0xbfbc800000000000, 0xbff4380000000000, 0x3fe6500000000000,
                0x3ff0ac0000000000, 0xbfe8200000000000, 0xbff3840000000000, 0x3ff7500000000000,
                0x3fda800000000000, 0xbfe9180000000000, 0x3fcc600000000000, 0x3fe7700000000000,
            ],
        ),
        // Touchstone 16.0.20625.20108 nl25-s1-060 (n=14, p_bin=4 2*right=14 >= 14 -> Excel=0)
        (
            0.0,
            &[
                0xbfb7c00000000000, 0x3fda700000000000, 0xc005520000000000, 0xbfe7600000000000,
                0x4000b60000000000, 0xbfbd800000000000, 0xc000c00000000000, 0x3ff7780000000000,
                0x3fd9a00000000000, 0x3fe1e80000000000, 0xbfeec00000000000, 0xbfe5100000000000,
                0x3fbbc00000000000, 0x3fef380000000000,
            ],
        ),
    ];
    for (expected, bits) in cases {
        let history: Vec<f64> = bits.iter().map(|&b| f64::from_bits(b)).collect();
        let timeline: Vec<f64> = (1..=history.len()).map(|v| v as f64).collect();
        exact(call("FORECAST.ETS.SEASONALITY", vec![col(&history), col(&timeline)]), expected);
    }
}

/// Excel 20527 period-two histories of 6 and 7 observations search the full
/// SEASONAL_PARAMETERS grid (dc-iuwct.54; only n=5 is restricted to the five
/// SHORT_TWO_PARAMETERS candidates).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_period_two_six_and_seven_use_full_seasonal_candidates() {
    let p2_n6_2: Vec<f64> = [
        0x3ff95d9edd96359a_u64, 0x400fee611f958cd7, 0x400eae528535b5a2,
        0x4019e7e77fbfc9db, 0x4018bf53e9db2684, 0x402197e3786162ba,
    ]
    .into_iter()
    .map(f64::from_bits)
    .collect();
    let t6 = col(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    exact(
        call("FORECAST.ETS", vec![n(6.0), col(&p2_n6_2), t6.clone(), n(2.0)]),
        f64::from_bits(0x4021990d1025d01e),
    );
    let p2_n7_0: Vec<f64> = [
        0x401939622c37a94d_u64, 0x401ec6e1f0bde90f, 0x4021a4202c02434b, 0x40249cfa7918bdbb,
        0x40268f6822a018ca, 0x402995c215452a1e, 0x402b778fd252fa25,
    ]
    .into_iter()
    .map(f64::from_bits)
    .collect();
    let t7 = col(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    exact(
        call("FORECAST.ETS", vec![n(7.0), col(&p2_n7_0), t7.clone(), n(2.0)]),
        f64::from_bits(0x402b7df303b61f9c),
    );
    // cb4 row 94 (p=2, n=7, target=12).
    let cb4_94: Vec<f64> = [
        0x40177fc3e6df9a51_u64, 0x40150a122a078761, 0x401b688172a79fc4, 0x4017598031da152f,
        0x401f2751d725ce67, 0x401aa16bfc9d7c16, 0x40215c9e4bac6b5c,
    ]
    .into_iter()
    .map(f64::from_bits)
    .collect();
    exact(
        call("FORECAST.ETS", vec![n(12.0), col(&cb4_94), t7, n(2.0)]),
        f64::from_bits(0x4022941a3b6d8e55),
    );
}

/// Excel 20527 accepts decimal timelines whose uniform steps differ only by
/// binary64 representation rounding (dc-iuwct.54, fix3f/ets-past-boundary-shape.json):
/// step is the first interval within 3e-7 of the minimum interval.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_decimal_timeline_rounded_steps_match_excel() {
    let y = col(&[
        0.8191354270330827,
        2.144215659272221,
        3.27159573407552,
        3.9506847997283474,
        5.0850123361217205,
    ]);
    let t = col(&[0.1, 0.11, 0.12000000000000001, 0.13, 0.14]);
    for past in [-1.0, 0.0, 1e-5, 0.08, 0.09000000000000001, 0.09001] {
        error(call("FORECAST.ETS", vec![n(past), y.clone(), t.clone()]), Num);
    }
    exact(
        call("FORECAST.ETS", vec![n(0.15), y.clone(), t.clone()]),
        f64::from_bits(0x401857258ef685cb),
    );
    exact(
        call("FORECAST.ETS", vec![n(0.1), y.clone(), t.clone()]),
        f64::from_bits(0x3feeacbc29064f30),
    );
    exact(
        call("FORECAST.ETS", vec![n(0.095), y.clone(), t.clone()]),
        f64::from_bits(0x3fdc8a2ea8b6a4ae),
    );
    // fix3f row 43: (0.09999 - 0.14)/step + 4 = -0.00100000000000189 <= -0.001, so Excel
    // interpolates rather than snapping to first (even though (0.09999 - 0.1)/step > -0.001).
    exact(
        call("FORECAST.ETS", vec![n(0.09999000000000001), y.clone(), t.clone()]),
        f64::from_bits(0x3feea455f14da5a5),
    );
    // Integer timeline exact boundary at along == -0.001 (interpolates) and -0.999 (admitted).
    let tint = col(&[1.0, 2.0, 3.0, 4.0, 5.0]);
    exact(
        call("FORECAST.ETS", vec![n(0.999), y.clone(), tint.clone()]),
        f64::from_bits(0x3feea455f14da5b6),
    );
    exact(
        call("FORECAST.ETS", vec![n(0.001), y.clone(), tint]),
        f64::from_bits(0xbfb0d13a44b80630),
    );
    // Step selection preserves first interval in the 3e-7 cluster (dec_0.3 and d0_is_big).
    let t03 = col(&[0.1, 0.4, 0.7, 1.0, 1.3]);
    exact(
        call("FORECAST.ETS.STAT", vec![y.clone(), t03, n(8.0)]),
        f64::from_bits(0x3fd3333333333334),
    );
    let t_gap = col(&[0.1, 0.12, 0.13, 0.14, 0.15]);
    exact(
        call("FORECAST.ETS.STAT", vec![y, t_gap, n(8.0)]),
        f64::from_bits(0x3f847ae147ae1480),
    );
}

/// Excel 20527 FORECAST.ETS.CONFINT on period-two histories of 6 and 7 points
/// and decimal timelines with binary64-rounded uniform steps (dc-iuwct.55).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_confint_period_two_and_decimal_timeline() {
    let p2_n6_2: Vec<f64> = [
        0x3ff95d9edd96359a_u64, 0x400fee611f958cd7, 0x400eae528535b5a2,
        0x4019e7e77fbfc9db, 0x4018bf53e9db2684, 0x402197e3786162ba,
    ]
    .into_iter()
    .map(f64::from_bits)
    .collect();
    let t6 = col(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    exact(
        call("FORECAST.ETS.CONFINT", vec![n(7.0), col(&p2_n6_2), t6, n(0.95), n(2.0)]),
        f64::from_bits(0x3fc2e21a8763449e),
    );
    let p2_n7_0: Vec<f64> = [
        0x401939622c37a94d_u64, 0x401ec6e1f0bde90f, 0x4021a4202c02434b, 0x40249cfa7918bdbb,
        0x40268f6822a018ca, 0x402995c215452a1e, 0x402b778fd252fa25,
    ]
    .into_iter()
    .map(f64::from_bits)
    .collect();
    let t7 = col(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    exact(
        call("FORECAST.ETS.CONFINT", vec![n(8.0), col(&p2_n7_0), t7, n(0.95), n(2.0)]),
        f64::from_bits(0x3fb9213121786074),
    );
    let y = col(&[
        0.8191354270330827,
        2.144215659272221,
        3.27159573407552,
        3.9506847997283474,
        5.0850123361217205,
    ]);
    let t = col(&[0.1, 0.11, 0.12000000000000001, 0.13, 0.14]);
    for past in [-1.0, 0.0, 1e-5, 0.08, 0.09000000000000001, 0.09001, 0.095, 0.09999000000000001, 0.1] {
        error(call("FORECAST.ETS.CONFINT", vec![n(past), y.clone(), t.clone()]), Num);
    }
    exact(
        call("FORECAST.ETS.CONFINT", vec![n(0.15), y, t, n(0.95)]),
        f64::from_bits(0x3fd9a1a6270e564d),
    );
}

/// Excel 20527 FORECAST.ETS.STAT on period-two histories of 6 and 7 points
/// selects alpha/gamma from the full SEASONAL_PARAMETERS grid (dc-iuwct.57).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_stat_period_two_six_and_seven() {
    let p2_n6_2: Vec<f64> = [
        0x3ff95d9edd96359a_u64, 0x400fee611f958cd7, 0x400eae528535b5a2,
        0x4019e7e77fbfc9db, 0x4018bf53e9db2684, 0x402197e3786162ba,
    ]
    .into_iter()
    .map(f64::from_bits)
    .collect();
    let t6 = col(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    exact(
        call("FORECAST.ETS.STAT", vec![col(&p2_n6_2), t6.clone(), n(1.0), n(2.0)]),
        f64::from_bits(0x3f60624dd2f1a9fc),
    );
    exact(
        call("FORECAST.ETS.STAT", vec![col(&p2_n6_2), t6, n(3.0), n(2.0)]),
        f64::from_bits(0x3fd0000000000000),
    );
    let p2_n7_0: Vec<f64> = [
        0x401939622c37a94d_u64, 0x401ec6e1f0bde90f, 0x4021a4202c02434b, 0x40249cfa7918bdbb,
        0x40268f6822a018ca, 0x402995c215452a1e, 0x402b778fd252fa25,
    ]
    .into_iter()
    .map(f64::from_bits)
    .collect();
    let t7 = col(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    exact(
        call("FORECAST.ETS.STAT", vec![col(&p2_n7_0), t7, n(3.0), n(2.0)]),
        f64::from_bits(0x3fe8000000000000),
    );
}

/// Excel 20527 seasonal starting states below four cycles (oracle corpus dcj9k_r38_ets rows
/// 1737-1738 and 2633-2634, period 4 on 8 and 12 points): the history is extended to
/// ceil(3.5 p) points before the phase-mean chain, and the parameter search runs on the extension.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_short_seasonal_history_extends_before_the_starting_states() {
    let history = [2.0, 3.5, 3.75, 5.75, 5.5, 7.0, 8.5, 8.75, 10.75, 10.5, 12.0, 13.5];
    for (len, forecast, confidence) in [
        (8, 0x4022_67d9_aaec_4dee_u64, 0x3fea_7dd8_7764_7300_u64),
        (12, 0x402c_5a7e_22cd_f86c, 0x3ff4_23d3_cf16_5a7c),
    ] {
        let y = col(&history[..len]);
        let timeline: Vec<f64> = (1..=len).map(|v| v as f64).collect();
        let target = n(len as f64 + 1.0);
        exact(call("FORECAST.ETS", vec![target.clone(), y.clone(), col(&timeline), n(4.0), n(0.0), n(1.0)]), f64::from_bits(forecast));
        exact(
            call("FORECAST.ETS.CONFINT", vec![target, y, col(&timeline), n(0.95), n(4.0), n(0.0), n(1.0)]),
            f64::from_bits(confidence),
        );
    }
}

/// Excel 20527's seasonal search and trend reset (eng23 probe_long): the search validates on
/// min(floor(0.4 N), max(10, 2p, ceil(N/5))) points, so period 12 on 44 points selects (0.251, 0.001, 0.25);
/// an unstable final trend restarts from the last observation with the starting trend (period 2).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_seasonal_search_window_and_trend_reset() {
    let reset: [f64; 44] = [10.372, 11.762, 10.235, 12.325, 8.256, 8.925, 10.819, 9.045, 18.416, 14.112, 16.82, 8.707, 15.913, 20.456, 12.851, 24.094, 17.563, 18.107, 7.596, 15.05, 2.075, -8.378, 10.762, 2.365, 26.789, 0.201, 13.276, 4.61, 19.964, 17.171, 12.494, 28.971, -11.222, -11.303, 2.716, 4.279, 22.65, 28.144, 44.542, 35.896, -22.565, 39.122, -17.413, 14.485];
    let long: [f64; 44] = [10.106, 9.916, 10.642, 8.857, 7.505, 9.895, 9.41, 8.374, 5.372, 9.798, 5.091, 16.738, 4.571, 0.874, 21.259, -2.631, 24.382, 19.345, 19.872, 15.194, 27.922, 15.677, 11.242, 12.93, 1.527, 4.745, -9.307, 10.736, 6.989, 23.194, -3.645, 3.385, 6.949, 30.599, -15.173, 34.992, -4.434, 34.577, -16.609, 28.029, 40.318, 50.613, 49.53, -19.409];
    let timeline: Vec<f64> = (1..=44).map(|v| v as f64).collect();
    for (history, period, forecast) in [(&reset, 2.0, 0x402b_8ce3_12c6_fcef_u64), (&long, 12.0, 0x4039_6799_c47a_4cba)] {
        exact(call("FORECAST.ETS", vec![n(45.0), col(history), col(&timeline), n(period)]), f64::from_bits(forecast));
    }
    for (statistic, expected) in [(1.0, 0.251), (2.0, 0.001), (3.0, 0.25)] {
        exact(call("FORECAST.ETS.STAT", vec![col(&long), col(&timeline), n(statistic), n(12.0)]), expected);
    }
    // 52 points validate on ceil(52/5) = 11, which selects (0.501, 0.001, 0.001).
    let longer: [f64; 52] = [9.769, 9.928, 10.97, 9.601, 10.942, 9.917, 6.524, 9.292, 5.256, 12.581, 0.159, -0.637, 18.481, -3.438, -2.104, -5.825, 1.0, 21.576, 7.269, 11.816, 17.014, -8.779, 2.71, 29.582, -6.056, -1.32, 27.214, 34.437, 2.495, 22.359, 2.569, 33.835, 39.645, 30.745, -15.475, 39.416, 32.273, 42.691, 19.217, 3.864, -5.933, 3.383, -21.059, 16.046, 25.577, -34.637, -9.662, -21.068, -29.524, -35.949, 40.94, 57.391];
    let timeline: Vec<f64> = (1..=52).map(|v| v as f64).collect();
    exact(call("FORECAST.ETS", vec![n(53.0), col(&longer), col(&timeline), n(4.0)]), f64::from_bits(0x404f_9121_778a_a4d5));
    exact(call("FORECAST.ETS.STAT", vec![col(&longer), col(&timeline), n(1.0), n(4.0)]), 0.501);
}

/// Excel 20527's nonseasonal search uses the same validation window (eng23 probe_nons):
/// 64 points validate on ceil(64/5) = 13 and select (0.666667, 0.166667).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_nonseasonal_search_window_grows_with_the_history() {
    let history: [f64; 64] = [11.327, 10.085, 10.324, 10.613, 11.333, 12.334, 12.074, 12.368, 11.87, 12.671, 12.336, 11.357, 14.9, 12.379, 13.826, 14.992, 15.793, 16.702, 15.926, 16.8, 15.064, 16.281, 14.81, 14.686, 15.299, 13.397, 13.579, 13.647, 12.636, 11.619, 10.541, 10.918, 9.538, 10.308, 7.993, 7.95, 8.062, 7.792, 5.701, 4.655, 2.675, 1.997, 0.853, 0.02, -2.141, -0.991, -5.925, -6.087, -5.298, -6.696, -7.319, -7.416, -9.602, -9.515, -8.59, -12.254, -13.38, -13.238, -13.489, -17.939, -17.274, -18.324, -19.293, -21.15];
    let timeline: Vec<f64> = (1..=64).map(|v| v as f64).collect();
    exact(call("FORECAST.ETS", vec![n(65.0), col(&history), col(&timeline), n(0.0)]), f64::from_bits(0xc036_19f9_dacd_25b2));
    exact(call("FORECAST.ETS.STAT", vec![col(&history), col(&timeline), n(1.0), n(0.0)]), 0.666667);
    exact(call("FORECAST.ETS.STAT", vec![col(&history), col(&timeline), n(2.0), n(0.0)]), 0.166667);
}

/// Excel 20527 checks a scalar history against the longer timeline before reading the history.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_scalar_history_shape_precedes_its_value() {
    let timeline = arr(vec![vec![3.0], vec![1.0], vec![1.0], vec![4.0]]);
    for history in [n(7.0), t("bad"), b(false), calc_func::value::CalcValue::empty(), e(Div0)] {
        for name in ["FORECAST.ETS", "FORECAST.ETS.CONFINT"] {
            error(call(name, vec![n(10.0), history.clone(), timeline.clone()]), NA);
        }
        error(call("FORECAST.ETS.SEASONALITY", vec![history.clone(), timeline.clone()]), NA);
        error(call("FORECAST.ETS.STAT", vec![history, timeline.clone(), n(1.0)]), NA);
    }
}

/// Excel 16.0.20625, dc-j9k.415 and dc-j9k.416: option coercion precedes unequal numeric vectors.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_coerces_scalar_options_before_unequal_vector_lengths() {
    let values = col(&[10.0, 12.0]);
    let timeline = col(&[1.0, 2.0, 3.0, 4.0]);
    error(call("FORECAST.ETS.STAT", vec![values.clone(), timeline.clone(), n(-0.5), n(-4.0), e(Div0)]), Div0);
    error(call("FORECAST.ETS.STAT", vec![timeline.clone(), values.clone(), n(8.0), t("FALSE")]), Value);
    error(call("FORECAST.ETS.STAT", vec![values.clone(), timeline.clone(), n(-0.5), n(-4.0)]), NA);
    error(call("FORECAST.ETS.SEASONALITY", vec![values.clone(), timeline.clone(), e(Div0)]), Div0);
    error(call("FORECAST.ETS", vec![n(9.0), values.clone(), timeline.clone(), n(0.0), e(Div0)]), Div0);
    error(call("FORECAST.ETS.CONFINT", vec![n(9.0), values, timeline, n(0.95), n(0.0), n(1.0), e(Ref)]), Ref);
}

/// Excel 20527 serial controls: numeric options preserve an earlier worksheet error
/// before the unequal history shape, while numeric options alone leave #N/A.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_scalar_history_with_numeric_options_keeps_error_order() {
    use calc_func::value::WorksheetErrorCode::{Name, Null};
    let timeline = col(&[1.0, 2.0, 3.0, 4.0, 5.0]);
    for history in [n(3.0), t("bad"), b(false), calc_func::value::CalcValue::empty(), e(Div0)] {
        for code in [Null, Div0, Value, Ref, Name, Num, NA] {
            error(call("FORECAST.ETS.STAT", vec![history.clone(), timeline.clone(), n(1.0), e(code), e(Ref)]), code);
            error(call("FORECAST.ETS.SEASONALITY", vec![history.clone(), timeline.clone(), n(1.0), e(code)]), code);
        }
        error(call("FORECAST.ETS.STAT", vec![history.clone(), timeline.clone(), n(1.0), n(-2.0)]), NA);
        error(call("FORECAST.ETS.SEASONALITY", vec![history, timeline.clone(), n(1.0), n(-2.0)]), NA);
    }
}

/// Excel 20527 reads text options in the recorded locale before the history shape.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_scalar_history_reads_text_options_in_the_calculation_environment() {
    use calc_func::locale::{CalcEnvironment, DateSystem, FormatProfile};
    let mut env = CalcEnvironment {
        compatibility_version: 2,
        date_system: DateSystem::System1900,
        profile: FormatProfile::for_culture("en-ZA").and_then(|p| p.with_separators(".", " ")),
        now: None,
    };
    let timeline = col(&[1.0, 2.0, 3.0, 4.0, 5.0]);
    for name in ["FORECAST.ETS.STAT", "FORECAST.ETS.SEASONALITY"] {
        for (option, expected) in [("bad", Value), ("1,234", Value), ("1", NA), ("25%", NA)] {
            error(call_in(&env, None, None, name, vec![n(3.0), timeline.clone(), n(1.0), t(option)]), expected);
        }
        assert!(matches!(
            call_in(&env, None, None, name, vec![n(3.0), timeline.clone(), n(1.0), t("1/2")]),
            Err(calc_func::FunctionFault::NotImplemented { .. })
        ));
        env.now = Some(46318.6);
        error(call_in(&env, None, None, name, vec![n(3.0), timeline.clone(), n(1.0), t("1/2")]), NA);
        env.now = None;
    }
}

/// Excel 20527 rejects lost-addend beta shapes for normal probabilities through one half.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_inverse_rejects_lost_shape_addend() {
    for name in ["BETA.INV", "BETAINV"] {
        let alpha = 2.0_f64.powi(80);
        let beta = f64::from_bits(1.0_f64.to_bits() + 1);
        for p in [f64::MIN_POSITIVE, 0.25, 0.5] {
            error(call(name, vec![n(p), n(alpha), n(beta)]), Num);
            error(call(name, vec![n(p), n(alpha), n(beta), n(-10.0), n(20.0)]), Num);
        }
        error(call(name, vec![n(0.25), n(alpha), n(beta), t("bad"), n(20.0)]), Value);
        error(call(name, vec![n(0.0), n(alpha), n(beta)]), Num);
        exact(call(name, vec![n(f64::from_bits(0.5_f64.to_bits() + 1)), n(alpha), n(beta)]), 1.0);
    }
}

/// Excel 20527 rejects separated tiny shapes while preserving bound errors.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_inverse_tiny_shape_checks_starting_density() {
    for name in ["BETA.INV", "BETAINV"] {
        for (alpha, beta) in [
            (2.0_f64.powi(-100), 100.0),
            (2.0_f64.powi(-100), 2.0_f64.powi(-99)),
            (2.0_f64.powi(-53), 2.0_f64.powi(-52)),
        ] {
            for p in [f64::MIN_POSITIVE, 0.25, 0.5] {
                error(call(name, vec![n(p), n(alpha), n(beta)]), Num);
                error(call(name, vec![n(p), n(alpha), n(beta), n(-10.0), n(20.0)]), Num);
            }
        }
        error(call(name, vec![n(0.25), n(2.0_f64.powi(-100)), n(100.0), t("bad")]), Value);
    }
}

/// Excel 20527's large right-tail inverse uses an x87 factor multiplication.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chi_right_inverse_admits_large_degrees() {
    let cases = [
        (0x0010_0000_0000_0000, 0x4956_c16c_8e75_19d3, 0x4956_c2f8_fb3d_ac40),
        (0x3f50_624d_d2f1_a9fc, 0x6d7b_899e_ef77_4458, 0x6d7b_8b7e_aaff_2423),
        (0x3ddb_7cdf_d9d7_bdbb, 0x62e1_d1ba_3413_9d79, 0x62e1_d2f0_a1fc_0f7a),
        (0x3fe0_0000_0000_0000, 0x4950_0000_0000_0000, 0x4950_0116_bc77_4cb6),
        (0x2b2b_ff2e_e48e_0530, 0x63bd_362c_8e75_19d3, 0x63bd_3829_73d1_9f88),
        (0x3fe0_0000_0000_0000, 0x4720_0000_0000_0000, 0x4720_0116_bc77_4cb6),
        (0x3fb9_9999_9999_999a, 0x4720_fa33_55c9_40b4, 0x4720_fb5b_18ff_a4b0),
        (0x3fe0_0000_0000_0000, 0x472a_56d3_12c0_0480, 0x472a_589d_ed93_fe6f),
        (0x3fe0_0000_0000_0000, 0x4951_cd01_7362_be52, 0x4951_ce37_8f09_7e45),
        (0x3fe6_a9e9_3a60_2ca9, 0x46b7_b533_bae0_0b4a, 0x46b7_b6d0_be84_bf6e),
        (0x3fd3_66d1_dfde_b4e0, 0x46aa_d9d1_9f32_4eca, 0x46aa_dba5_6414_10a6),
    ];
    for name in ["CHIINV", "CHISQ.INV.RT"] {
        for (p, degrees, result) in cases {
            exact(call(name, vec![n(f64::from_bits(p)), n(f64::from_bits(degrees))]), f64::from_bits(result));
        }
        let degrees = 2.0_f64.powi(200);
        error(call(name, vec![n(0.0), n(degrees)]), Num);
        exact(call(name, vec![n(1.0), n(degrees)]), 0.0);
        error(call("CHISQ.INV", vec![n(0.5), n(degrees)]), Num);
    }
}

/// Excel 20527 scales the large-shape unit inverse with a separate x87 store.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_inverse_matches_large_shape_scaling() {
    let cases = [
        (0x3fec_cccc_cccc_cccd, 0x5df6_7e78_87dc_b90c, 0x4cba_fe6a_9e9b_35d4, 0x6ac2_fae3_4dc6_a8fd),
        (0x0010_0000_0000_0000, 0x587c_1092_a7c1_c484, 0x2d59_6052_5a69_d7fc, 0x45e6_42f0_3bae_1492),
        (0x3fb9_9999_9999_999a, 0x4af4_4536_0eb4_9a78, 0x3a75_3052_f0fb_5722, 0x457a_d9e3_4fbd_cb4f),
        (0x3fb9_9999_9999_999a, 0x4fc4_f326_1b3d_8490, 0x2e5e_ef3a_7734_d192, 0x3e34_41f2_d55b_c53f),
        (0x3fb9_9999_9999_999a, 0x61a4_1dde_9fa2_3430, 0x4611_575d_b9da_b301, 0x67c5_cf10_a036_23aa),
        (0x0010_0000_0000_0000, 0x4950_0000_0000_0000, 0x26f0_0000_0000_0000, 0x3050_0116_bc77_4cb6),
        (0x3fef_ffff_ffff_ffff, 0x656f_ffff_ffff_ffff, 0x58ef_ffff_ffff_ffff, 0x7e70_0116_bc77_4cb6),
        (0x0010_0000_0000_0000, 0x4720_0000_0000_0000, 0x26f0_0000_0000_0000, 0x2e20_0116_bc77_4cb6),
        (0x3fb9_9999_9999_999a, 0x472a_56d3_12c0_0480, 0x400a_0000_0000_0000, 0x4745_6800_5108_3eba),
    ];
    for name in ["GAMMA.INV", "GAMMAINV"] {
        for (p, alpha, beta, result) in cases {
            exact(
                call(name, vec![n(f64::from_bits(p)), n(f64::from_bits(alpha)), n(f64::from_bits(beta))]),
                f64::from_bits(result),
            );
        }
        let alpha = 2.0_f64.powi(200);
        exact(call(name, vec![n(0.0), n(alpha), n(1.0)]), 0.0);
        error(call(name, vec![n(1.0), n(alpha), n(1.0)]), Num);
    }
}

/// Excel 20527 stores both stages of scaled normal inverse arithmetic through x87.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn normal_inverse_stores_scaled_midpoints() {
    let p = f64::from_bits(0x3fa4_7739_38ac_f552);
    for name in ["NORM.INV", "NORMINV"] {
        for (mean, sigma, expected) in [
            (0x0000_0000_0000_0000, 0x3ff0_90dc_109a_3d78, 0xbffd_01c9_8292_a390),
            (0xba90_0080_0000_0000, 0x3de0_3ef4_43e0_15ec, 0xbdec_725e_af75_b6d4),
        ] {
            exact(call(name, vec![n(p), n(f64::from_bits(mean)), n(f64::from_bits(sigma))]), f64::from_bits(expected));
        }
    }
    for name in ["LOGNORM.INV", "LOGINV"] {
        for (mean, sigma, expected) in [
            (0x0000_0000_0000_0000, 0x3ff0_90dc_109a_3d78, 0x3fc4_e2e5_4017_cf9d),
            (0xba90_0080_0000_0000, 0x3de0_3ef4_43e0_15ec, 0x3fef_ffff_ffe3_8da1),
        ] {
            exact(call(name, vec![n(p), n(f64::from_bits(mean)), n(f64::from_bits(sigma))]), f64::from_bits(expected));
        }
    }
}

/// Excel 20527 finds no seasonality in directly observed five-point curved histories.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_seasonality_detects_strict_curvature() {
    let times = col(&[1.0, 2.0, 3.0, 4.0, 5.0]);
    for values in [
        [1.0, 2.0, 4.0, 7.0, 11.0],
        [-1.0, -2.0, -4.0, -7.0, -11.0],
    ] {
        exact(call("FORECAST.ETS.SEASONALITY", vec![col(&values), times.clone()]), 0.0);
    }
    // This non-curved history selects period two in Excel and must not use
    // the strict-curvature route.
    exact(call("FORECAST.ETS.SEASONALITY", vec![col(&[1.0, 2.0, 3.2, 3.1, 5.0]), times]), 2.0);
}

/// Excel 20527 rejects far future horizons even when subtraction is inexact.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_checks_large_horizon_before_exact_subtraction() {
    let values = arr(vec![vec![1.2, 2.7, 3.1, 4.4, 5.3]]);
    for (target, times) in [
        (2.0_f64.powi(54), vec![1.0, 2.0, 3.0, 4.0, 5.0]),
        (1e6 + (4.0 + 2.0_f64.powi(54)) * 0.125,
         vec![1e6, 1e6 + 0.125, 1e6 + 0.25, 1e6 + 0.375, 1e6 + 0.5]),
    ] {
        for name in ["FORECAST.ETS", "FORECAST.ETS.CONFINT"] {
            error(call(name, vec![n(target), values.clone(), arr(vec![times.clone()])]), Num);
        }
    }
}

/// Excel 20527 forms the tiny-confidence normal probability through an x87 complement.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_confint_zero_probability_precedes_fractional_horizon() {
    let values = arr(vec![vec![1.2, 2.7, 3.1, 4.4, 5.3]]);
    let times = arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]);
    for confidence in [0.0, 1e-100, f64::from_bits(0x3ca7_feff_ffff_ffff)] {
        exact(call("FORECAST.ETS.CONFINT", vec![n(5.25), values.clone(), times.clone(), n(confidence)]), 0.0);
        error(call("FORECAST.ETS.CONFINT", vec![n(5.0), values.clone(), times.clone(), n(confidence)]), Num);
        error(call("FORECAST.ETS.CONFINT", vec![n(2.0_f64.powi(31) + 5.0), values.clone(), times.clone(), n(confidence)]), Num);
    }
    assert!(matches!(
        call("FORECAST.ETS.CONFINT", vec![n(5.25), values.clone(), times.clone(), n(f64::from_bits(0x3ca7_ff00_0000_0000))]),
        Ok(_)
    ));
    // Touchstone (eng21): the default interval grows from zero across the first step
    // as sqrt(fraction) times the one-step interval, matching Excel's exact bits.
    let one_step = f64::from_bits(0x3fd3_65bd_64d3_5b62);
    exact(call("FORECAST.ETS.CONFINT", vec![n(6.0), values.clone(), times.clone()]), one_step);
    // Excel: 5.25 -> 0.1515423528969349, half of its own one-step interval.
    exact(call("FORECAST.ETS.CONFINT", vec![n(5.25), values.clone(), times.clone()]), one_step * 0.5);
    exact(call("FORECAST.ETS.CONFINT", vec![n(5.001), values.clone(), times.clone()]), one_step * (5.001_f64 - 5.0).sqrt());
    error(call("FORECAST.ETS.CONFINT", vec![n(5.0005), values.clone(), times]), Num);
    let incomplete = arr(vec![vec![1.0, 1.0, 2.0, 4.0, 7.0]]);
    error(call("FORECAST.ETS.CONFINT", vec![n(7.25), values, incomplete, n(1e-100)]), Num);
}

/// Excel 20527 snaps close targets and uses stored x87 interpolation elsewhere.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_interpolates_fractional_future_targets() {
    let values = arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]);
    let times = arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]);
    for (target, expected) in [(5.0005, 5.0), (5.001, 5.001), (5.25, 5.25), (5.5, 5.5), (5.999, 5.999), (5.9995, 6.0)] {
        exact(call("FORECAST.ETS", vec![n(target), values.clone(), times.clone(), n(0.0)]), expected);
    }
    let d = 732_757_574_657.0;
    let halfway = arr(vec![vec![-2.0 * d, -d, 0.0, d, 2.0 * d]]);
    exact(
        call("FORECAST.ETS", vec![n(f64::from_bits(0x4017_6381_7c9d_7802)), halfway, times, n(0.0)]),
        f64::from_bits(0x427e_5c07_54c8_7711),
    );
}

/// Excel 20527: range and singleton results precede significance validation.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn percentrank_orders_range_singleton_and_argument_errors() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(5.0), CalcValue::empty(), n(2.0), n(4.0), e(Div0)]);
    let one = sheet.area(1, 1, 2, 1);
    let empty = sheet.area(2, 1, 2, 1);
    let pair = sheet.area(3, 1, 4, 1);
    let sample_error = sheet.area(5, 1, 5, 1);
    for name in ["PERCENTRANK", "PERCENTRANK.INC", "PERCENTRANK.EXC"] {
        for sig in [-1.0, 0.0, 0.5, 3.0] {
            exact(call_on(&mut sheet, name, vec![one.clone(), n(5.0), n(sig)]), 1.0);
            error(call_on(&mut sheet, name, vec![one.clone(), n(4.0), n(sig)]), NA);
        }
        error(call_on(&mut sheet, name, vec![empty.clone(), n(5.0), n(0.0)]), NA);
        error(call_on(&mut sheet, name, vec![pair.clone(), n(4.0), n(0.0)]), Num);
        error(call_on(&mut sheet, name, vec![pair.clone(), n(5.0), n(0.0)]), NA);
        error(call_on(&mut sheet, name, vec![sample_error.clone(), e(NA), e(Num)]), NA);
        error(call_on(&mut sheet, name, vec![sample_error.clone(), n(5.0), e(Num)]), Num);
        error(call(name, vec![t("bad"), e(NA), e(Num)]), Value);
    }
}

/// Excel 20527 returns these subnormal inverse-lognormal results as numbers.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn lognormal_inverse_publishes_subnormal_results() {
    for name in ["LOGINV", "LOGNORM.INV"] {
        for (mean, bits) in [
            (-709.0, 0x0008_bfe5_5de0_2338),
            (-720.0, 0x0000_0009_93b4_dc95),
            (-735.0, 0x0000_0000_0000_3127),
            (-745.0, 0x0000_0000_0000_0001),
            (-746.0, 0),
        ] {
            exact(call(name, vec![n(0.5), n(mean), n(1.0)]), f64::from_bits(bits));
        }
    }
}

/// Excel 16.0.20625, dc-j9k.404: a non-finite normal inverse is a lognormal inverse error.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn lognormal_inverse_rejects_overflow_before_exponentiation() {
    for name in ["LOGINV", "LOGNORM.INV"] {
        error(
            call(name, vec![n(f64::from_bits(0x3f56_7dcc_05dd_5000)), n(1.0), n(f64::from_bits(0x7fe0_ba7d_b52e_ed08))]),
            Num,
        );
        error(call(name, vec![n(0.2), n(-f64::MAX), n(1e300)]), Num);
        exact(call(name, vec![n(0.2), n(0.0), n(1e300)]), 0.0);
    }
}

/// Excel 20527: scalar sigma does not bypass the sample-size check.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn z_test_checks_sample_count_after_scalar_argument_errors() {
    let mut sheet = Sheet::new();
    sheet.column(2, 1, vec![n(2.0), n(4.0)]);
    let empty = sheet.area(1, 1, 1, 1);
    let single = sheet.area(2, 1, 2, 1);
    let pair = sheet.area(2, 1, 3, 1);
    for name in ["Z.TEST", "ZTEST"] {
        for sigma in [1.0, 0.0, -1.0] {
            error(call_on(&mut sheet, name, vec![empty.clone(), n(3.0), n(sigma)]), NA);
            error(call_on(&mut sheet, name, vec![single.clone(), n(3.0), n(sigma)]), Div0);
        }
        error(call_on(&mut sheet, name, vec![single.clone(), n(3.0)]), Div0);
        error(call_on(&mut sheet, name, vec![empty.clone(), n(3.0), e(Div0)]), Div0);
        error(call_on(&mut sheet, name, vec![empty.clone(), e(Div0), e(NA)]), Div0);
        error(call(name, vec![t("hello"), e(Num), e(NA)]), Value);
        error(call(name, vec![t("12"), e(Num), e(NA)]), Num);
        error(call_on(&mut sheet, name, vec![pair.clone(), n(3.0), n(-1.0)]), Num);
        exact(call_on(&mut sheet, name, vec![pair.clone(), n(3.0), n(1.0)]), 0.5);
    }
}

/// Excel 16.0.20625, dc-j9k.407 and dc-j9k.408: ZTEST publishes the midpoint on standardization overflow.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn z_test_standardization_overflow_returns_the_midpoint() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(f64::from_bits(0x3f16_c9f8_3b0c_3bab)), n(f64::from_bits(0xbefb_fadd_4679_6187))]);
    sheet.column(1, 2, vec![n(1.0), n(5.0), n(3.0)]);
    sheet.column(1, 3, vec![n(f64::MAX), n(f64::MAX)]);
    let pair = sheet.area(1, 1, 2, 1);
    let triple = sheet.area(1, 2, 3, 2);
    let overflowed_mean = sheet.area(1, 3, 2, 3);
    for name in ["ZTEST", "Z.TEST"] {
        exact(call_on(&mut sheet, name, vec![pair.clone(), n(1e308)]), 0.5);
        exact(
            call_on(&mut sheet, name, vec![triple.clone(), n(-1.0), n(f64::from_bits(0x0010_0000_0000_0003))]),
            0.5,
        );
        exact(call_on(&mut sheet, name, vec![triple.clone(), n(1e150), n(1.0)]), 1.0);
        error(call_on(&mut sheet, name, vec![overflowed_mean.clone(), n(0.0), n(1.0)]), Num);
    }
}

/// Excel 16.0.20625, dc-j9k.409: omitted ZTEST sigma uses centered deviations.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn z_test_omitted_sigma_uses_centered_sample_variance() {
    let cases: &[(&[u64], u64, u64)] = &[
        (&[0x3fab_9ced_ccf2_972e, 0xbf22_64d0_54ae_4d67, 0x3f2d_44e8_7113_5994], 0xbfe0_0000_0000_0000, 0x1a10_07b1_e562_39e0),
        (&[0xc126_f6c6_891d_cd16, 0x401d_6a3a_5281_cb8c, 0x3ef0_63f6_833a_204a], 0x3fd3_ed41_ba91_34af, 0x3fea_ec45_0212_7c4c),
        (&[0x4069_88e1_258d_9e58, 0x4081_c1f5_3031_8f40], 0x4059_5000_0000_0000, 0x3fae_0878_dc8b_2107),
    ];
    for (sample, x, expected) in cases {
        let sample = arr(sample.iter().map(|&bits| vec![f64::from_bits(bits)]).collect());
        for name in ["ZTEST", "Z.TEST"] {
            exact(call(name, vec![sample.clone(), n(f64::from_bits(*x))]), f64::from_bits(*expected));
        }
    }
    for name in ["ZTEST", "Z.TEST"] {
        for sample in [vec![vec![1.0; 3]], vec![vec![0.0, 2e-154]]] {
            error(call(name, vec![arr(sample), n(0.0)]), Div0);
        }
        exact(call(name, vec![arr(vec![vec![0.0, 0.0, 0.0, 2e-154]]), n(0.0)]), 0.5);
    }
}

/// Excel 16.0.20527: a single blank cell differs from an array element or
/// a blank within a multi-cell range, and argument errors keep their order.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sumproduct_single_blank_reference_and_error_order() {
    use calc_func::value::{CalcArray, CalcValue};
    let mut sheet = Sheet::new();
    sheet.column(2, 1, vec![n(2.0)]);
    let blank = sheet.area(1, 1, 1, 1);
    let range = sheet.area(1, 1, 2, 1);
    error(call_on(&mut sheet, "SUMPRODUCT", vec![blank.clone()]), Value);
    exact(call_on(&mut sheet, "SUMPRODUCT", vec![range]), 2.0);
    error(call_on(&mut sheet, "SUMPRODUCT", vec![blank.clone(), e(NA)]), Value);
    error(call_on(&mut sheet, "SUMPRODUCT", vec![e(NA), blank.clone()]), NA);
    sheet.column(1, 1, vec![CalcValue::empty()]);
    error(call_on(&mut sheet, "SUMPRODUCT", vec![blank]), Value);
    let array = CalcValue::array(CalcArray::from_rows(vec![vec![CalcValue::empty()]]).unwrap());
    exact(call("SUMPRODUCT", vec![array]), 0.0);
}

/// Excel 16.0.20625 admits every argument before returning errors held inside a factor.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sumproduct_admission_precedes_held_errors() {
    use calc_func::value::{CalcArray, CalcValue};
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(1.0), e(NA)]);
    let range = sheet.area(1, 1, 2, 1);
    let blank = sheet.area(1, 2, 1, 2);
    let array = CalcValue::array(CalcArray::from_rows(vec![vec![n(1.0)], vec![e(NA)]]).unwrap());
    for held in [range, array] {
        error(call_on(&mut sheet, "SUMPRODUCT", vec![held.clone(), blank.clone()]), Value);
        error(call_on(&mut sheet, "SUMPRODUCT", vec![held.clone(), e(Div0)]), Div0);
        error(call_on(&mut sheet, "SUMPRODUCT", vec![held, arr(vec![vec![1.0, 2.0]])]), NA);
    }
    let single_error = sheet.area(2, 1, 2, 1);
    error(call_on(&mut sheet, "SUMPRODUCT", vec![single_error, blank]), NA);
}

/// Excel 20527 coerces the selector before sample errors and rejects negative fractions.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn quartile_selector_admission_and_error_order() {
    let data = arr(vec![vec![1.0], vec![3.0], vec![5.0], vec![7.0]]);
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![n(1.0), e(NA)]);
    let sample_error = sheet.area(1, 1, 2, 1);
    for name in ["QUARTILE", "QUARTILE.INC", "QUARTILE.EXC"] {
        for q in [-0.5, -f64::MIN_POSITIVE] {
            error(call(name, vec![data.clone(), n(q)]), Num);
        }
        error(call(name, vec![data.clone(), t("-0.5")]), Num);
        error(call_on(&mut sheet, name, vec![sample_error.clone(), n(-0.5)]), NA);
        error(call_on(&mut sheet, name, vec![sample_error.clone(), t("bad")]), Value);
        error(call_on(&mut sheet, name, vec![sample_error.clone(), e(Div0)]), Div0);
    }
    for name in ["QUARTILE", "QUARTILE.INC"] {
        exact(call(name, vec![data.clone(), n(4.5)]), 7.0);
        error(call(name, vec![data.clone(), n(5.0)]), Num);
    }
}

/// Excel 20527: the paired families reject a single blank reference before
/// inspecting multi-cell samples, while preserving singleton error order.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn paired_statistics_single_blank_reference_and_error_order() {
    let mut sheet = Sheet::new();
    sheet.column(1, 2, vec![n(2.0), n(4.0), n(8.0)]);
    sheet.column(1, 3, vec![e(NA), n(4.0)]);
    let blank = sheet.area(1, 1, 1, 1);
    let numbers = sheet.area(1, 2, 3, 2);
    let error_range = sheet.area(1, 3, 2, 3);
    for name in ["CORREL", "COVAR", "COVARIANCE.P", "COVARIANCE.S", "F.TEST",
        "FTEST", "INTERCEPT", "PEARSON", "RSQ", "SLOPE", "STEYX", "SUMX2PY2",
        "SUMX2MY2", "SUMXMY2", "FORECAST", "FORECAST.LINEAR"] {
        let make_args = |left, right| {
            if name.starts_with("FORECAST") { vec![n(1.0), left, right] }
            else { vec![left, right] }
        };
        error(call_on(&mut sheet, name, make_args(blank.clone(), n(2.0))), Value);
        error(call_on(&mut sheet, name, make_args(numbers.clone(), blank.clone())), Value);
        error(call_on(&mut sheet, name, make_args(blank.clone(), e(NA))), Value);
        error(call_on(&mut sheet, name, make_args(e(NA), blank.clone())), NA);
        error(call_on(&mut sheet, name, make_args(error_range.clone(), blank.clone())), Value);
    }
    sheet.column(1, 1, vec![n(1.0), calc_func::value::CalcValue::empty(), n(3.0)]);
    let mixed = sheet.area(1, 1, 3, 1);
    exact(call_on(&mut sheet, "CORREL", vec![mixed, numbers]), 1.0);
}

/// Excel 20527 checks blank and error cells before singleton sample shape.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chisq_test_validates_singleton_sample_cells_before_shape() {
    let mut sheet = Sheet::new();
    sheet.column(1, 2, vec![n(3.0), e(NA)]);
    let blank = sheet.area(1, 1, 1, 1);
    let single_error = sheet.area(2, 2, 2, 2);
    let range = sheet.area(1, 2, 2, 2);
    for name in ["CHISQ.TEST", "CHITEST"] {
        error(call_on(&mut sheet, name, vec![blank.clone(), n(3.0)]), Value);
        error(call_on(&mut sheet, name, vec![n(3.0), blank.clone()]), Value);
        error(call_on(&mut sheet, name, vec![single_error.clone(), blank.clone()]), NA);
        error(call_on(&mut sheet, name, vec![blank.clone(), single_error.clone()]), Value);
        error(call_on(&mut sheet, name, vec![range.clone(), blank.clone()]), Value);
        error(call(name, vec![n(2.0), n(3.0)]), NA);
    }
}

/// Excel 20527 validates sample singletons, scalar options, then sample ranges.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn t_test_validates_tails_after_singleton_sample_errors() {
    let mut sheet = Sheet::new();
    sheet.column(1, 2, vec![n(3.0), e(NA)]);
    let blank = sheet.area(1, 1, 1, 1);
    let sample_error = sheet.area(2, 2, 2, 2);
    let range_with_error = sheet.area(1, 2, 2, 2);
    for name in ["T.TEST", "TTEST"] {
        for kind in [1.0, 2.0, 3.0] {
            error(call(name, vec![n(1.0), n(2.0), n(0.0), n(kind)]), Num);
            error(call(name, vec![n(1.0), n(2.0), n(3.0), n(kind)]), Num);
            error(call(name, vec![n(1.0), n(2.0), n(1.0), n(kind)]), Div0);
            error(call_on(&mut sheet, name, vec![blank.clone(), n(2.0), n(0.0), n(kind)]), Value);
            error(call_on(&mut sheet, name, vec![sample_error.clone(), n(2.0), n(0.0), n(kind)]), NA);
            error(call_on(&mut sheet, name, vec![range_with_error.clone(), n(2.0), n(0.0), n(kind)]), Num);
        }
        error(call(name, vec![n(1.0), n(2.0), n(0.0), e(NA)]), NA);
        error(call(name, vec![n(1.0), n(2.0), e(NA), n(0.0)]), NA);
        let tolerance = 2.0_f64.powi(-22) + 2.0_f64.powi(-33);
        for (slot, upper) in [(2, 1.0), (2, 3.0), (3, 1.0), (3, 4.0)] {
            let edge = upper - tolerance;
            for bits in [edge.to_bits() - 1, edge.to_bits()] {
                let mut args = vec![n(1.0), n(2.0), n(1.0), n(1.0)];
                args[slot] = n(f64::from_bits(bits));
                let admitted = if upper == 1.0 { bits == edge.to_bits() } else { bits < edge.to_bits() };
                error(call(name, args), if admitted { Div0 } else { Num });
            }
        }
    }
}

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

/// Excel 20527 publishes these tiny Student-t tails as zero, including text input.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn student_t_tail_flushes_measured_subnormal_results() {
    let x = n(f64::from_bits(0x4684_49cc_b7da_4c64));
    exact(call("T.DIST.2T", vec![x.clone(), n(10.0)]), 0.0);
    exact(call("TDIST", vec![x, n(10.0), n(2.0)]), 0.0);
    let degrees = n(f64::from_bits(0x407d_79c3_9f40_2c68));
    exact(call("T.DIST.RT", vec![t("1e2"), degrees.clone()]), 0.0);
    exact(call("TDIST", vec![t("1e2"), degrees, n(1.0)]), 0.0);
    exact(
        call("T.DIST.2T", vec![n(f64::from_bits(0x46fc_f908_8f7b_18b0)), n(9.0)]),
        f64::from_bits(0x0138_52fd_89f2_b1cd),
    );
    error(call("T.DIST.2T", vec![n(1e200), n(2.0)]), Num);
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
            // Excel 20527 Sheet1!B70 is 0x3fb84c56c84b091d; under dc-j9k.58's universal bup_140
            // twenty-step sum (b = 0.5) this row sits -1 ULP at 0x3fb84c56c84b091c (remaining
            // b <= 1 last-bit residual owned by dc-j9k.55; T.TEST/TTEST full banks stay 100%).
            0x3fb84c56c84b091c,
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
        Ok(_)
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
        Ok(_)
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
            Ok(_)
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
fn gamma_small_inputs_match_recorded_answers() {
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
            0x404fd2d4c62f1d4e,
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
    let just_above_one = f64::from_bits(1.0_f64.to_bits() + 1);
    for function in ["LARGE", "SMALL"] {
        error(call(function, vec![n(8.0), n(just_above_one)]), Num);
        error(call(function, vec![arr(vec![vec![8.0]]), n(just_above_one)]), Num);
    }
    let three = arr(vec![vec![1.0, 2.0, 3.0]]);
    exact(call("LARGE", vec![three.clone(), n(just_above_one)]), 3.0);
    exact(call("LARGE", vec![three.clone(), n(1.3)]), 2.0);
    exact(call("SMALL", vec![three, n(1.3)]), 1.0);
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
    assert_excel_capture(
        call("FISHER", vec![n(1.0)]),
        include_bytes!("captures/jnt/sections/jnt_w111_elementary.capture.json"),
        "jnt_w111_elementary",
        2947,
        4,
    );
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

/// Excel 20527 validates the stored sum of PROB's probabilities in input order.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn prob_multi_item_sum_and_bounds() {
    let prob = |probabilities: Vec<f64>, lower, upper| {
        let values = (0..probabilities.len()).map(|index| index as f64).collect();
        call("PROB", vec![arr(vec![values]), arr(vec![probabilities]), n(lower), n(upper)])
    };
    for (lower, upper, expected) in [(0.0, 0.0, -0.25), (1.0, 1.0, 1.25), (0.0, 1.0, 1.0), (1.0, 0.0, 0.0)] {
        exact(prob(vec![-0.25, 1.25], lower, upper), expected);
    }

    exact(prob(vec![1e16, -1e16, 1.0], 0.0, 2.0), 1.0);
    exact(prob(vec![1e16, -1e16, 1.0], 1.0, 2.0), -1e16);
    error(prob(vec![1e16, 1.0, -1e16], 0.0, 2.0), Num);

    let one = 1.0_f64.to_bits();
    for bits in [one - 4, one + 22] {
        let accepted = f64::from_bits(bits);
        exact(prob(vec![0.375, 0.125, accepted - 0.5], 0.0, 2.0), accepted);
    }
    for bits in [one - 5, one + 23] {
        error(prob(vec![0.375, 0.125, f64::from_bits(bits) - 0.5], 0.0, 2.0), Num);
    }
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

/// Excel 20527 dcztvv_r24!D1 and a centered-predictor translation control.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn linest_scales_covariance_before_the_intercept_error_sum() {
    let d = 2.0_f64.powi(-19);
    for (y, x, expected) in [
        (
            [1.0, 2.0, 4.0, 7.0],
            [[1.0, 2.0], [2.0, 4.0], [5.0, 10.0], [4.0, 8.0001]],
            "0x3fcaf6fd4fb22b5e",
        ),
        (
            [0.75, 4.5, 1.25, 5.5],
            [[4.0, 8.0 - d], [6.0, 12.0 - d], [4.0, 8.0 + d], [6.0, 12.0 + d]],
            "0x3fe4662e2de0b5da",
        ),
    ] {
        let fit = call(
            "LINEST",
            vec![
                col(&y),
                arr(x.into_iter().map(|row| row.to_vec()).collect()),
                b(true),
                b(true),
            ],
        )
        .expect("LINEST result");
        let fit = fit.as_array().expect("LINEST array");
        let intercept_error = fit.get(1, 2).expect("intercept standard error");
        assert_eq!(calc_verdict::encode_result(intercept_error).as_deref(), Some(expected));
    }
}

/// Excel 20527 reads numeric 2x2 forecast histories as four paired observations.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn regression_paired_matrix_history() {
    let bits = [
        0x404a_af0c_9431_a3c1, 0x4031_f571_8006_0f5d,
        0x403f_c8e2_4131_fbf4, 0x4058_4b0e_fa94_f509,
        0xc058_dfb0_5640_c362, 0xc040_8496_6ac6_03ec,
        0x4054_f604_5798_131c, 0xc03b_9664_a2b7_2888,
        0xc04e_f916_434c_408b, 0xc057_2ec8_7fad_e8ea,
        0x4026_1adb_6780_82b8, 0xc040_5dc0_a2ba_76e0,
    ].map(f64::from_bits);
    let matrix = |i| arr(vec![vec![bits[i], bits[i + 1]], vec![bits[i + 2], bits[i + 3]]]);
    for (function, expected) in [
        ("TREND", 0x4033_859c_5529_f914),
        ("GROWTH", 0x4007_8cfc_4660_753f),
    ] {
        let (rows, cols, cells) =
            array(call(function, vec![matrix(0), matrix(4), matrix(8), b(false)]));
        assert_eq!((rows, cols), (2, 2), "{function}");
        assert_eq!(cells[0].to_bits(), expected, "{function}");
    }
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
    // fresh-negbinom.dist-00113-fresh-endpoints: Excel 20527 is 0x3fe5_f7b1_5d80_0000. Under
    // dc-j9k.58's universal bup stored powers (b > 1, n = 10; NEGBINOM.DIST full bank +15,
    // 3351 -> 3366), this row sits -1 ULP at 0x3fe5_f7b1_5d7f_ffff (exact bits pinned until
    // dc-j9k.55 resolves the remaining bup n >= 4 last-bit residual; no tolerance added).
    exact(
        call("NEGBINOM.DIST", vec![n(4.0), n(11.0), n(0.75), b(true)]),
        f64::from_bits(0x3fe5_f7b1_5d7f_ffff),
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
fn subtotal_neutral_cells_do_not_need_exclusion_locations() {
    let mut sheet = Sheet::new();
    sheet.column(1, 1, vec![t("123"), b(true), CalcValue::empty()]);
    let neutral = sheet.area(1, 1, 3, 1);
    for selector in [2.0, 4.0, 5.0, 6.0, 9.0] {
        exact(call_on(&mut sheet, "SUBTOTAL", vec![n(selector), neutral.clone()]), 0.0);
        exact(call_on(&mut sheet, "AGGREGATE", vec![n(selector), n(1.0), neutral.clone()]), 0.0);
    }
    error(call_on(&mut sheet, "SUBTOTAL", vec![n(1.0), neutral.clone()]), Div0);
    error(call_on(&mut sheet, "AGGREGATE", vec![n(1.0), n(1.0), neutral.clone()]), Div0);
    refused(call_on(&mut sheet, "SUBTOTAL", vec![n(3.0), neutral.clone()]));
    refused(call_on(&mut sheet, "AGGREGATE", vec![n(3.0), n(1.0), neutral]));
    sheet.column(1, 2, vec![e(NA)]);
    sheet.column(1, 3, vec![n(1.0)]);
    let error_cell = sheet.area(1, 2, 1, 2);
    let number_cell = sheet.area(1, 3, 1, 3);
    refused(call_on(&mut sheet, "SUBTOTAL", vec![n(9.0), error_cell]));
    refused(call_on(&mut sheet, "SUBTOTAL", vec![n(9.0), number_cell]));
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
    // Excel 20527, adjacent inputs around the inclusive selector boundaries.
    for (boundary, below, at) in [
        (0x3fef_ffff_7ff0_0000_u64, None, Some(2.0)),
        (0x3fff_ffff_bff8_0000, Some(2.0), Some(3.0)),
        (0x4059_3fff_feff_e000, None, Some(2.0)),
        (0x405b_ffff_feff_e000, Some(1.0), None),
    ] {
        for (bits, expected) in [(boundary - 1, below), (boundary, at)] {
            let actual = call_on(&mut sheet, "SUBTOTAL", vec![n(f64::from_bits(bits)), range.clone()]);
            match expected {
                Some(value) => exact(actual, value),
                None => error(actual, Value),
            }
        }
    }
    // Excel 20527, AGGREGATE's function number has the same inclusive boundary.
    let function_edge = 4.0_f64 - (2.0_f64.powi(-22) + 2.0_f64.powi(-33));
    exact(
        call_on(&mut sheet, "AGGREGATE", vec![n(f64::from_bits(function_edge.to_bits() - 1)), n(4.0), range.clone()]),
        4.0,
    );
    exact(
        call_on(&mut sheet, "AGGREGATE", vec![n(function_edge), n(4.0), range.clone()]),
        10.0,
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
        call("AGGREGATE", vec![n(14.0), n(4.0), with_error.clone(), n(1.0)]),
        Div0,
    );
    let option_edge = 6.0_f64 - (2.0_f64.powi(-22) + 2.0_f64.powi(-33));
    error(
        call("AGGREGATE", vec![n(14.0), n(f64::from_bits(option_edge.to_bits() - 1)), with_error.clone(), n(1.0)]),
        Div0,
    );
    exact(
        call("AGGREGATE", vec![n(14.0), n(option_edge), with_error.clone(), n(1.0)]),
        3.0,
    );
    let upper_edge = 8.0_f64 - (2.0_f64.powi(-22) + 2.0_f64.powi(-33));
    exact(
        call("AGGREGATE", vec![n(14.0), n(f64::from_bits(upper_edge.to_bits() - 1)), with_error.clone(), n(1.0)]),
        3.0,
    );
    error(
        call("AGGREGATE", vec![n(14.0), n(upper_edge), with_error.clone(), n(1.0)]),
        Value,
    );
    let negative_edge = -(2.0_f64.powi(-23) + 2.0_f64.powi(-34));
    error(
        call("AGGREGATE", vec![n(14.0), n(f64::from_bits(negative_edge.to_bits() + 1)), with_error.clone(), n(1.0)]),
        Value,
    );
    error(
        call("AGGREGATE", vec![n(14.0), n(negative_edge), with_error, n(1.0)]),
        Div0,
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn rounding_to_a_significance() {
    assert_excel_capture(
        call("CEILING.MATH", vec![n(-8.1), n(2.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        34,
        1,
    );
    exact(call("CEILING.MATH", vec![n(-5.5), n(2.0), n(-1.0)]), -6.0);
    exact(call("CEILING.MATH", vec![n(24.3), n(5.0)]), 25.0);
    assert_excel_capture(
        call("FLOOR.MATH", vec![n(-8.1), n(2.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        87,
        1,
    );
    exact(call("FLOOR.MATH", vec![n(-5.5), n(2.0), n(-1.0)]), -4.0);
    assert_excel_capture(
        call("CEILING.PRECISE", vec![n(4.3)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        35,
        1,
    );
    exact(call("CEILING.PRECISE", vec![n(-4.3), n(2.0)]), -4.0);
    assert_excel_capture(
        call("ISO.CEILING", vec![n(-4.3), n(2.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        137,
        1,
    );
    assert_excel_capture(
        call("FLOOR.PRECISE", vec![n(-3.2), n(-1.0)]),
        include_bytes!("captures/catalogue/sections/prefix_metadata.capture.json"),
        "prefix_metadata",
        88,
        1,
    );
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

/// Excel20527, ax1 frozen seed proof and stored-argument edge probes (2026-11-18).
/// Includes the three PC64 addition discriminants and the internal seed's half-integer seam.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_seed_and_reciprocal_match_frozen_excel_answers() {
    for (input, answer) in [
        (0x3ff9e80991d8cf81_u64, 0x3fecaa8421e75ee2_u64),
        (0x3ffd35a2f6a9abd4_u64, 0x3fee07993ccc0a37_u64),
        (0x3fff6bbe88769793_u64, 0x3fef8707694ed8f8_u64),
        (0x3ff2b52206bfcdd6_u64, 0x3feda97661780b6c_u64),
        (0x3ff3411d48ee165c_u64, 0x3fed5a4fb9ee4bc8_u64),
        (0x3ff8b2c70bfde0da_u64, 0x3fec6df040fcde6b_u64),
        (0x3ff312557a7f6225_u64, 0x3fed736961361cac_u64),
        (0x3ffe00686b18c233_u64, 0x3fee82e444e56b39_u64),
        (0x3ff4c87a6e71e679_u64, 0x3fecb965acdb1255_u64),
        (0x3ffe48940ffbd0d3_u64, 0x3feeb28621872638_u64),
        (0x3ff9f693190c4fce_u64, 0x3fecae438eb391fc_u64),
        (0x3ffe3ff6adb7b603_u64, 0x3feeacbbe86be25c_u64),
        (0x3ff9a49e1d721885_u64, 0x3fec9a2ebfd3c72f_u64),
        (0x3ef115e587c17ff8_u64, 0x40edf772755eb20b_u64),
        (0x3ee5cdb61b11000d_u64, 0x40f77b7595097d96_u64),
        (0x3e8f82af7cbff86c_u64, 0x41503fa14b665562_u64),
        (0x3fdffffffffffffe_u64, 0x3ffc5bf891b4ef6c_u64),
        (0x3fdfffffffffffff_u64, 0x3ffc5bf891b4ef6a_u64),
        (0x3fe0000000000000_u64, 0x3ffc5bf891b4ef6b_u64),
        (0x3fe0000000000001_u64, 0x3ffc5bf891b4ef68_u64),
        (0x3ff0000000000001_u64, 0x3fefffffffffffff_u64),
        (0x3ff8000000000000_u64, 0x3fec5bf891b4ef6b_u64),
        (0x4000000000000001_u64, 0x3ff0000000000002_u64),
        (0x4021ffffffffffff_u64, 0x40e3afffffffffed_u64),
    ] {
        exact(call("GAMMA", vec![n(f64::from_bits(input))]), f64::from_bits(answer));
    }
}

/// Excel20527, retained Fable-r4 random and pole controls (ax1, 2026-11-18).
/// The reciprocal accumulator divides ascending factors before multiplying the seed.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_negative_recurrence_matches_retained_excel_answers() {
    for (input, answer) in [
        (0xbfadae75a399b2e0_u64, 0xc031e33aed66d5f8_u64),
        (0xbfdb145ed568be78_u64, 0xc00d34d72ede7227_u64),
        (0xbff4eb932a80c670_u64, 0x400a111e82161810_u64),
        (0xbff6ff7b54cbeebf_u64, 0x4004205e36955909_u64),
        (0xc0074963128b4295_u64, 0xc000ed504b4d8639_u64),
        (0xc00273d6e3aa66c9_u64, 0xbff6a988d8e9ede2_u64),
        (0xc0082b70c297e992_u64, 0x401e9fd4cd6dab8e_u64),
        (0xc00f81449cbdf8fd_u64, 0x3fe7ca7f3c81ec0f_u64),
        (0xc01001c0c3bf20a0_u64, 0xc03846df5a016e5c_u64),
        (0xc01130b63c56ccff_u64, 0xbfba5a92d4872572_u64),
        (0xc01631141995ff3a_u64, 0x3f84bd55645acc3e_u64),
        (0xc01512d0475ba878_u64, 0x3f96910f12daaa79_u64),
        (0xbfeffffffffffffe_u64, 0xc330000000000000_u64),
        (0xbfeffffffe000000_u64, 0xc1b00000006c3b98_u64),
        (0xbfffffffffc00000_u64, 0x41c00000003b0ee7_u64),
        (0xc007ffffffe00000_u64, 0xc1a5555555c085a5_u64),
        (0xc00fffffffe00000_u64, 0x4185555555d5dafb_u64),
        (0xc013ffffffe00000_u64, 0xc151111111fa0234_u64),
        (0xc017ffffffc00000_u64, 0x4116c16c196b483e_u64),
    ] {
        exact(call("GAMMA", vec![n(f64::from_bits(input))]), f64::from_bits(answer));
    }
}

/// Excel 20527. Signed neighbors of the Laurent cutoff, the worksheet half
/// product, stored reflection quotients and the transition to worksheet zero.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_pole_and_reflection_match_frozen_excel_answers() {
    for (input, answer) in [
        (0xbc91ec7adc44b277_u64, 0xc34c90c16b5764d1_u64),
        (0x3ca0000000000003, 0x433ffffffffffff9),
        (0x3cafffffffffffff, 0x432fffffffffffff),
        (0x3cb0000000000000, 0x432fffffffffffff),
        (0x3cb0000000000001, 0x432ffffffffffffd),
        (0xbcafffffffffffff, 0xc330000000000001),
        (0xbcb0000000000000, 0xc330000000000001),
        (0xbcb0000000000001, 0xc330000000000000),
        (0xbfe0000000000000, 0xc00c5bf891b4ef6a),
        (0xc004000000000000, 0xbfee3ff812e32183),
        (0xc01a000000000000, 0xbf5b81b0e66884ff),
        (0xc025000000000000, 0xbe91b7b1db2e47f1),
        (0xc05eb02f0ff27af5, 0x959858aa308d87a9),
        (0xc03066d8c724fedb, 0xbd2ca0829c97a5b7),
        (0xc060ac88a7c1650a, 0x10e8f754a8401db1),
        (0xc055ebe41fc4d79c, 0x244e27a200748509),
        (0xc065500000000000, 0x8017d2374dfcda78), // -170.5
        (0xc065480000000000, 0x803e7334b3b3c7f3), // -170.25
        (0xc065580000000000, 0), // -170.75
        (0xc0655fffffffffff, 0x82928aa6e7526175),
        (0xc065700000000000, 0), // -171.5
        (0xc0657fffffffffff, 0), // nearest double above -172
        (0xc065900000000000, 0), // -172.5
    ] {
        exact(call("GAMMA", vec![n(f64::from_bits(input))]), f64::from_bits(answer));
    }
}

/// Excel 20527, frozen probes at which the degree-16 correction or a different
/// reciprocal, sum or product precision changes the worksheet answer.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_large_uses_the_stored_stirling_multiplier() {
    for (input, answer) in [
        (0x4024001bd235842b_u64, 0x411628b5ed96db1a_u64),
        (0x402401cbd0ce97c7, 0x411652f8805c0b85),
        (0x4024069260315a2f, 0x4116cc517aa6a374),
        (0x4024366c594766e0, 0x411c279214704cb3),
        (0x4024656a0cdb5655, 0x41215590f21f44e3),
        (0x4024c5e17786d294, 0x412aa56294f215b8),
        (0x403afbc606aba1fc, 0x4573c06ddd80a59d),
        (0x403d036d387356be, 0x461019522321c998),
        (0x405e2e88389dcf11, 0x691834e81866dea2),
    ] {
        exact(call("GAMMA", vec![n(f64::from_bits(input))]), f64::from_bits(answer));
    }
}

/// Excel 20527, frozen B1 controls including both boundaries and inputs that
/// distinguish the stored denominator sum from a register-resident one.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gammaln_b1_uses_the_stored_raw_argument_table() {
    for (input, answer) in [
        (0x3fe6666666666666_u64, 0x3fd0b20c891cde72_u64),
        (0x3fe6666666666667, 0x3fd0b20c891cde71),
        (0x3fe73a13de08479b, 0x3fcd7980b77f7d9f),
        (0x3fe838cdd402bcb6, 0x3fc91060c471afe9),
        (0x3fe838cdd40a2d53, 0x3fc91060c451e4a5),
        (0x3feb79c1ccf7bb5c, 0x3fb96c1f81d10333),
        (0x3ff0000000000000, 0),
        (0x3ff3227d01c15b65, 0xbfb58d1931c6cbcc),
        (0x3ff4c46ca11f9a2a, 0xbfbb9a83f1f407a7),
        (0x3ff7ffffffffffff, 0xbfbeeb95b094c191),
    ] {
        for name in ["GAMMALN", "GAMMALN.PRECISE"] {
            exact(call(name, vec![n(f64::from_bits(input))]), f64::from_bits(answer));
        }
    }
}

/// Inputs frozen before asking Excel 20527, including tail and factor-order discriminants.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gammaln_b2_stores_each_factor_in_order() {
    let readings: [(u64, u64); 11] = [
        (0x3ff8000000000000, 0xbfbeeb95b094c192),
        (0x3ff8000000000001, 0xbfbeeb95b094c190),
        (0x3ffac476c7b902ee, 0xbfb9e53944be7828),
        (0x3ffae30d0f20274b, 0xbfb9884a1ce0bbbf),
        (0x3ffb05e59074e324, 0xbfb91ad4b9c460c8),
        (0x3ffb13429853a885, 0xbfb8efd8701be20e),
        (0x3ffb4c4e8e262fc7, 0xbfb8321a3b8e2541),
        (0x3ffbf8ccaad19457, 0xbfb5b7bece69bd0d),
        (0x3ffddf1563ea291c, 0xbfa9ca7f35545075),
        (0x3ffffffffffffffe, 0xbcab0ee6072093cc),
        (0x3fffffffffffffff, 0xbc9b0ee6072093cc),
    ];
    for (input, answer) in readings {
        for name in ["GAMMALN", "GAMMALN.PRECISE"] {
            exact(call(name, vec![n(f64::from_bits(input))]), f64::from_bits(answer));
        }
    }
}

/// Frozen Excel 20527 boundary and denominator-store discriminants on [2,3].
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gammaln_b3_stores_both_sums_before_dividing() {
    for (input, answer) in [
        (0x4000000000000000_u64, 0_u64),
        (0x4000000000000001, 0x3cab0ee6072093d0),
        (0x400248cc9b54d9db, 0x3fc2a227d6fe2f0c),
        (0x400248f28658680a, 0x3fc2a38f1d00a5dd),
        (0x4005ca7f7671901c, 0x3fdd0dde8b4a0801),
        (0x4005ca9502ea2822, 0x3fdd0e69bbe6bfcf),
        (0x4007ffffffffffff, 0x3fe62e42fefa39eb),
        (0x4008000000000000, 0x3fe62e42fefa39f0),
    ] {
        for name in ["GAMMALN", "GAMMALN.PRECISE"] {
            exact(call(name, vec![n(f64::from_bits(input))]), f64::from_bits(answer));
        }
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
            0x40c0060598b36e3b,
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
            0x403b41c168e10426,
        ),
        // The identified seed now reproduces these positive recurrence witnesses.
        (
            0x400c_14f6_1d18_79ae,
            0x400a_e3a0_184f_4257,
            0x400ae3a0184f4257,
        ),
        (
            0x400d_7b7b_3b82_609e,
            0x4010_6635_56ba_cf83,
            0x4010663556bacf83,
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
    // The Poisson mass uses the stored GAMMALN(1.375) in its normalizer.
    exact(
        call("GAMMA.DIST", vec![n(0.75), n(0.375), n(1.0), b(false)]),
        f64::from_bits(0x3fce_880d_a95a_bce6),
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

/// Excel 20527, ax1 frozen B1 product controls. A direct binary64 multiply
/// by x-1 cannot produce these words for any binary64 quotient.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gammaln_stores_the_b1_product_from_pc64() {
    for name in ["GAMMALN", "GAMMALN.PRECISE"] {
        for (input, expected) in [
            (0x3fe7_9a1a_b149_7cef, 0x3fcb_c630_3072_9c7c),
            (0x3fed_f00c_3cf9_bf97, 0x3fa4_da54_9be8_b610),
            (0x3ff0_97a5_d73f_e912, 0xbf94_bfce_ed53_bd7a),
            (0x3ff7_aa42_af09_12fa, 0xbfbf_1023_221a_3ab2),
        ] {
            exact(call(name, vec![n(f64::from_bits(input))]), f64::from_bits(expected));
        }
    }
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

/// Excel publishes a tiny gamma CDF as zero, while a nearby normal result remains numeric.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_cumulative_flushes_measured_subnormal_results() {
    for function in ["GAMMA.DIST", "GAMMADIST"] {
        exact(
            call(
                function,
                vec![n(f64::from_bits(0x3fe7_17d6_0e9e_206b)), n(100.0), n(f64::from_bits(0x403b_ffff_ffff_ffff)), b(true)],
            ),
            0.0,
        );
        exact(
            call(
                function,
                vec![n(f64::from_bits(0x3fe7_17d6_0e9e_206b)), n(100.0), n(f64::from_bits(0x403b_ffff_ffff_ffff)), b(false)],
            ),
            0.0,
        );
        exact(
            call(
                function,
                vec![n(f64::from_bits(0x3f84_4772_37e4_22f4)), n(110.0), n(f64::from_bits(0x3fc2_0000_0000_0000)), b(true)],
            ),
            f64::from_bits(0x009c_cdfd_302e_1efb),
        );
    }
}

/// Excel 16.0.20527.20196 Touchstone boundary controls (`dc-ymjf.2.141`, frozen SHA-256
/// `05399aac` and `50fe12bf`):
/// - subnormal `y = x / beta` flushes to `0.0` (yielding CDF `0.0` and PDF `#NUM!`);
/// - subnormal `poisson_mass(alpha, y)` flushes to `0.0` before scaling by `alpha / x`
///   or multiplying the `gratio_series` sum (so `a = 10700` jumps from `0` straight to
///   `3.13 * DBL_MIN` for CDF and `1.47 * DBL_MIN` for PDF);
/// - final CDF (including Temme `a = 14500`, independent of `poisson_mass`) and final
///   PDF (with normal `poisson_mass` and large `beta`) flush subnormals to `0.0` across
///   all shapes on both `GAMMA.DIST` and `GAMMADIST`.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ymjf_2_141_gamma_subnormal_publication_and_boundaries() {
    for function in ["GAMMA.DIST", "GAMMADIST"] {
        // 1. Subnormal y = x/beta flush at exact DBL_MIN (x = 2^-511, beta = 2^511 vs next_up).
        let x_half = n(f64::from_bits(0x2000_0000_0000_0000));
        let b_exact = n(f64::from_bits(0x5fe0_0000_0000_0000));
        let b_sub1 = n(f64::from_bits(0x5fe0_0000_0000_0001));
        for (a_val, cdf_exact, pdf_exact) in [
            (0.25, 0x2ff8_f6c7_a8f5_5265_u64, 0x4fc8_f6c7_a8f5_5265_u64),
            (0.5, 0x2002_0dd7_5042_9b6e_u64, 0x3fe2_0dd7_5042_9bb3_u64),
            (1.0, 0x0010_0000_0000_0000_u64, 0x2000_0000_0000_007c_u64),
            (2.0, 0x0000_0000_0000_0000_u64, 0x0000_0000_0000_0000_u64),
            (100.0, 0x0000_0000_0000_0000_u64, 0x0000_0000_0000_0000_u64),
        ] {
            exact(
                call(function, vec![x_half.clone(), n(a_val), b_exact.clone(), b(true)]),
                f64::from_bits(cdf_exact),
            );
            exact(
                call(function, vec![x_half.clone(), n(a_val), b_exact.clone(), b(false)]),
                f64::from_bits(pdf_exact),
            );
            exact(
                call(function, vec![x_half.clone(), n(a_val), b_sub1.clone(), b(true)]),
                0.0,
            );
            error(
                call(function, vec![x_half.clone(), n(a_val), b_sub1.clone(), b(false)]),
                Num,
            );
        }

        // 2. poisson_mass subnormal flush across wide shapes (a = 10, 100, 10700).
        for (x_lo, x_hi, a_val, cdf_hi, pdf_hi) in [
            (0x39af_8a36_c97a_67aa_u64, 0x39af_8bd4_3a66_57ac_u64, 10.0, 0x0010_0833_4024_17dd_u64, 0x0684_540c_8c20_fbba_u64),
            (0x3fa0_5096_eacc_caa9_u64, 0x3fa0_50ac_4ef0_6a84_u64, 100.0, 0x0010_097e_d790_4bc2_u64, 0x00c8_90f2_c6f0_17ed_u64),
            (0x40bc_6b78_0dfe_8658_u64, 0x40bc_6b79_2475_7f51_u64, 10700.0, 0x0029_07db_69d8_d2b3_u64, 0x0017_9406_89a7_c4be_u64),
        ] {
            exact(call(function, vec![n(f64::from_bits(x_lo)), n(a_val), n(1.0), b(true)]), 0.0);
            exact(call(function, vec![n(f64::from_bits(x_lo)), n(a_val), n(1.0), b(false)]), 0.0);
            // Normal side (1-2 ULP tolerance for series/Loader arithmetic, non-zero normal verified).
            let got_cdf = call(function, vec![n(f64::from_bits(x_hi)), n(a_val), n(1.0), b(true)]);
            let got_pdf = call(function, vec![n(f64::from_bits(x_hi)), n(a_val), n(1.0), b(false)]);
            if let Ok(v) = got_cdf {
                let bits = match v.core_ref() {
                    calc_func::value::CoreRef::Number(x) => x.to_bits(),
                    other => panic!("{function} a={a_val} CDF expected number, got {other:?}"),
                };
                assert!(bits.abs_diff(cdf_hi) <= 2, "{function} a={a_val} CDF {bits:#018x} vs {cdf_hi:#018x}");
            } else {
                panic!("{function} a={a_val} CDF failed: {got_cdf:?}");
            }
            if let Ok(v) = got_pdf {
                let bits = match v.core_ref() {
                    calc_func::value::CoreRef::Number(x) => x.to_bits(),
                    other => panic!("{function} a={a_val} PDF expected number, got {other:?}"),
                };
                assert!(bits.abs_diff(pdf_hi) <= 2, "{function} a={a_val} PDF {bits:#018x} vs {pdf_hi:#018x}");
            } else {
                panic!("{function} a={a_val} PDF failed: {got_pdf:?}");
            }
        }

        // 3. Temme CDF subnormal flush (a = 14500, independent of poisson_mass):
        //    both `h = 0.5 * erfc(s)` and `term` flush subnormals to 0.0 before `h - term`.
        //    At x=10438.5 both flush -> 0.0; at x=10438.75 and x=10444.0 `h` is normal while
        //    `term` is subnormal -> `h` alone; at x=10445.0 `term` becomes normal -> `h - term`.
        exact(call(function, vec![n(f64::from_bits(0x40c4_6340_0000_0000)), n(14500.0), n(1.0), b(true)]), 0.0);
        exact(call(function, vec![n(f64::from_bits(0x40c4_6340_0000_0000)), n(14500.0), n(1.0), b(false)]), 0.0);
        exact(call(function, vec![n(f64::from_bits(0x40c4_6360_0000_0000)), n(14500.0), n(1.0), b(false)]), 0.0);
        for (x_val, expected_bits) in [
            (10438.75, 0x0010_ef3e_e426_ab94_u64),
            (10444.0, 0x0040_5094_f37c_a826_u64),
            (10445.0, 0x004a_c37e_02be_35ba_u64),
        ] {
            let got_temme = call(function, vec![n(x_val), n(14500.0), n(1.0), b(true)]).unwrap();
            let bits = match got_temme.core_ref() {
                calc_func::value::CoreRef::Number(x) => x.to_bits(),
                other => panic!("{function} Temme x={x_val} expected number, got {other:?}"),
            };
            assert!(
                bits.abs_diff(expected_bits) <= 2,
                "{function} Temme x={x_val}: {bits:#018x} vs {expected_bits:#018x}"
            );
        }

        // 4. Final PDF subnormal flush when poisson_mass is normal and large beta scales PDF below DBL_MIN.
        exact(
            call(
                function,
                vec![
                    n(f64::from_bits(0x4086_f9b0_4bad_7908)),
                    n(100.0),
                    n(f64::from_bits(0x40d6_15f1_a6ca_ecdb)),
                    b(false),
                ],
            ),
            f64::from_bits(0x0010_1495_39e3_b210),
        );
        exact(
            call(
                function,
                vec![
                    n(f64::from_bits(0x4087_173e_b066_4de6)),
                    n(100.0),
                    n(f64::from_bits(0x40d6_325b_111e_46cf)),
                    b(false),
                ],
            ),
            0.0,
        );
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

/// Excel 16.0.20625, dc-j9k.406: ERFC reflects the stored positive result below one half.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn erfc_reflects_the_stored_small_positive_result() {
    for function in ["ERFC", "ERFC.PRECISE"] {
        for (input, expected) in [
            (0xbed4_9262_eb48_0b2e, 0x3ff0_0005_cd9e_c3d4),
            (0xbecd_3628_baeb_7013, 0x3ff0_0004_1ec6_5db6),
            (0xbfdf_ffff_ffff_fffe, 0x3ff8_53f7_ae0c_76e9),
            (0xbfe0_0000_0000_0000, 0x3ff8_53f7_ae0c_76e9),
            (0xbfe0_0000_0000_0001, 0x3ff8_53f7_ae0c_76ea),
        ] {
            exact(call(function, vec![n(f64::from_bits(input))]), f64::from_bits(expected));
        }
    }
}

/// Recorded beta and FDIST rows, including the unit-tail logarithm and bounded
/// coordinate separators measured on Excel build 20527 (dc-iuwct.41).
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
        (
            0x3fef_fcda_842a_00c6,
            0x40a1_8507_7a06_c65a,
            0x3ff0_0000_0000_0000,
            0x3fdb_0ad4_466c_c3da,
        ),
        (
            0x3fef_c6cd_008b_b0e5,
            0x4076_066c_9b2b_bd55,
            0x3ff0_0000_0000_0000,
            0x3fb5_abb8_53c1_0798,
        ),
        (
            0x3f6a_858e_3f4e_2c66,
            0x3ffc_8e96_9dd8_88a2,
            0x4040_e2a6_b682_f11f,
            0x3f86_b6a2_b2e8_18ea,
        ),
        (
            0x3fe5_b27c_fc02_551e,
            0x4028_b2df_6ed7_2426,
            0x4008_0000_0000_0000,
            0x3fbc_881e_1534_6a2c,
        ),
        (
            0x3fef_fdf9_2d18_019e,
            0x40b3_06a4_52f7_417e,
            0x3fe0_0000_0000_0000,
            0x3fbe_dc99_0e8f_7fb2,
        ),
        (
            0x3fe0_c5d6_fa63_045e,
            0x4006_41c9_69df_5792,
            0x4004_34bb_9a65_0d18,
            0x3fdf_a29b_15f7_8034,
        ),
        (
            0x3fd0_92d9_4596_770e,
            0x4004_557d_657a_524c,
            0x4005_9fed_68f7_a6e6,
            0x3fc2_e699_592f_8f79,
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
    exact(
        call(
            "BETA.DIST",
            vec![
                n(f64::from_bits(0xc028_f880_83dc_b299)),
                n(f64::from_bits(0x3fec_3a86_7f8c_872d)),
                n(f64::from_bits(0x3fff_670f_114f_88bd)),
                b(true),
                n(f64::from_bits(0xc041_2c6b_19df_5413)),
                n(f64::from_bits(0xc007_114f_effa_9c58)),
            ],
        ),
        f64::from_bits(0x3fed_567f_b8c2_2e69),
    );
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

/// Excel returns zero strictly within the measured final historical interval.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn ets_confidence_last_historical_interval() {
    let values = arr(vec![vec![1.2, 2.7, 3.1, 4.4, 5.3]]);
    let times = arr(vec![vec![1.0, 2.0, 3.0, 4.0, 5.0]]);
    for target in [4.1, 4.5, 4.75] {
        exact(
            call("FORECAST.ETS.CONFINT", vec![n(target), values.clone(), times.clone()]),
            0.0,
        );
    }
    for target in [4.0, 5.0] {
        error(
            call("FORECAST.ETS.CONFINT", vec![n(target), values.clone(), times.clone()]),
            Num,
        );
    }
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
        Ok(_)
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
            Ok(_)
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
                            Ok(_)
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
        Ok(_)
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
            Ok(_)
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
        Ok(_)
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
        Ok(_)
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
            Ok(_)
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

/// Excel 20527's large-degree plateau extends below 2^107 for these probabilities. The second
/// point was a conservative #NUM! (outside the then measured plateau); Excel answers it
/// (Touchstone, 2026-11-17, fn9: CHIINV and CHISQ.INV.RT agree) with Goldstein's leading
/// terms, c = 1.0000886 + 0.4713941 z / sqrt(n) one ulp below its plateau value (dc-6332.18).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chi_inverse_quantile_bound_keeps_the_large_degree_plateau() {
    for name in ["CHIINV", "CHISQ.INV.RT"] {
        exact(
            call(name, vec![n(f64::from_bits(0x3fef_42d8_dd06_a5be)), n(f64::from_bits(0x468f_22bf_0f7e_b500))]),
            f64::from_bits(0x468f_24dd_79f6_cee4),
        );
        exact(call(name, vec![n(63.0 / 64.0), n(2.0_f64.powi(106))]), f64::from_bits(0x4690_0116_bc77_4cb3));
    }
}

/// Excel 20527 shares the large-shape quantile steps across these inverse families.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_and_chi_inverse_share_large_shape_quantile_steps() {
    for name in ["CHIINV", "CHISQ.INV.RT"] {
        exact(
            call(name, vec![n(f64::from_bits(0x3fe6_f9b5_7f71_8def)), n(f64::from_bits(0x4620_7c58_197d_13f0))]),
            f64::from_bits(0x4620_7d77_4c27_c5cc),
        );
    }
    for name in ["GAMMA.INV", "GAMMAINV"] {
        exact(
            call(name, vec![n(f64::from_bits(0x3fc1_c4e4_2f17_a9d0)), n(f64::from_bits(0x4625_a8e8_19fc_fbf0)), n(1.0)]),
            f64::from_bits(0x4625_aa61_6fdf_e6bc),
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

/// Excel publishes the large-second-shape initial proposal for these inputs.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn beta_inverse_large_second_shape_initial_proposal() {
    for name in ["BETA.INV", "BETAINV"] {
        for (p, alpha, beta, expected) in [
            (0x3f80_da7e_c6c7_08c0, 0x4076_d000_0000_0000,
             0x4547_e651_99b2_0988, 0x3b1a_d7a6_05e9_7d76),
            (0x3f86_c432_aed7_c226, 0x407c_3000_0000_0000,
             0x4ed5_b087_427c_b26b, 0x3192_9eb2_6cbd_0edd),
        ] {
            exact(
                call(name, vec![n(f64::from_bits(p)), n(f64::from_bits(alpha)), n(f64::from_bits(beta))]),
                f64::from_bits(expected),
            );
        }
    }
}

/// The degree-one t inverse stores its pi product at x87 precision.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn t_inverse_one_degree_stores_the_angle() {
    for (p, expected) in [
        (0x3ff4_3e3d_e9e3_54dc, 0xbfdc_5150_7771_460a),
        (0x3ffd_7813_d2f7_468b, 0xc00f_87f1_8b63_f137),
    ] {
        exact(
            call("T.INV.2T", vec![n(f64::from_bits(p)), n(1.0)]),
            f64::from_bits(expected),
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

/// Excel20527: preserving the small gamma upper tail avoids a rounded-CDF
/// plateau, before the existing stored scale and final publication.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_inverse_preserves_the_small_upper_tail() {
    for name in ["GAMMA.INV", "GAMMAINV"] {
        exact(
            call(
                name,
                vec![
                    n(f64::from_bits(0x3fef_ffff_ffff_ffff)),
                    n(f64::from_bits(0x3fd3_e67d_a1e7_b6c8)),
                    n(f64::from_bits(0x402c_0000_0000_0001)),
                ],
            ),
            f64::from_bits(0x407d16b892f8f3f2),
        );
    }
}

/// Captured right-tail inverse answers at one-minus-epsilon retain the small
/// lower gamma tail, including truncation of the supplied degree argument.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chi_inverse_preserves_the_small_lower_tail() {
    for name in ["CHIINV", "CHISQ.INV.RT"] {
        for (degree, bits) in [
            (30.0, 0x3ff2_66f6_c3f3_89e2),
            (355.71571246910116, 0x4066_47bd_0aae_91c6),
            // Excel 20527 is 0x4081_9975_788e_ddeb (restored to bit-exact by Goldstein 451 start, dc-j9k.63).
            (861.8900555521836, 0x4081_9975_788e_ddeb),
        ] {
            exact(
                call(
                    name,
                    vec![n(f64::from_bits(0x3fef_ffff_ffff_ffff)), n(degree)],
                ),
                f64::from_bits(bits),
            );
        }
    }
}

/// Touchstone (eng21, Excel 20527): a target inside the history extrapolates the
/// final state backwards, and a target within one step before the first
/// timestamp interpolates towards that backward extrapolation.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_targets_inside_and_before_the_history() {
    let y = col(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.5]);
    let t = col(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
    for (target, expected) in [
        (1.0, 0.9491528037840906),
        (2.0, 1.9904089578278334),
        (3.0, 3.0316651118715763),
        (8.0, 8.23794588209029),
        (1.5, 1.469780880805962),
        (0.25, 0.1682106882512835),
        (0.5, 0.4285247267622192),
        (0.001, -0.09106209410560817),
        (0.0011, -0.09095796849020399),
        (0.999, 0.9481115476300466),
        (0.99901, 0.9491528037840906),
    ] {
        exact(call("FORECAST.ETS", vec![n(target), y.clone(), t.clone()]), expected);
    }
    for target in [0.0, 1e-80, 0.000999, -0.5, -5.0] {
        error(call("FORECAST.ETS", vec![n(target), y.clone(), t.clone()]), Num);
    }
}

/// Excel 20527 bank rows (bank2-more-FORECAST.ETS-0788, -1220): five-point
/// histories with automatic period two use the measured short period-two
/// initial states and its five candidates.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_five_point_period_two_fit() {
    let t = col(&[1.0, 2.0, 3.0, 4.0, 5.0]);
    for (target, y, expected) in [
        (0x3fe9_7a48_00db_f6b4_u64, [0x3ffd_a0cb_c023_312e_u64, 0x4004_cf30_6128_99a0, 0x400b_d093_ca8c_c0b8, 0x4012_efa2_8a83_e1b7, 0x4015_6050_deaf_d77e], 0x3ff7_fb91_3d22_8c6c_u64),
        (0x4017_ffff_ffff_ffff, [0x3ff3_b034_c8d6_ccd6, 0x4007_4138_15fa_13fa, 0x4009_adfc_7811_b7c2, 0x4011_71d8_7310_f858, 0x4015_91a3_85d8_f001], 0x4019_ab11_103b_b2ab),
    ] {
        let y = y.map(f64::from_bits);
        exact(call("FORECAST.ETS", vec![n(f64::from_bits(target)), col(&y), t.clone()]), f64::from_bits(expected));
        exact(call("FORECAST.ETS.SEASONALITY", vec![col(&y), t.clone()]), 2.0);
    }
}

/// Touchstone (eng21, Excel 20527): FORECAST.ETS.CONFINT snaps within 0.001
/// step of a timestamp and otherwise interpolates the squared interval.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_confint_fractional_targets() {
    let y = col(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.5]);
    let t = col(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
    for (target, expected) in [
        (14.0, 0.31259216649668303),
        (14.000000000000002, 0.31259216649668303),
        (14.001, 0.31259216649668303),
        (13.999, 0.31259216649668303),
        (14.25, 0.3132200590661818),
        (14.5, 0.31384669545457194),
        (14.75, 0.31447208317128444),
        (15.0, 0.31509622965122835),
    ] {
        exact(call("FORECAST.ETS.CONFINT", vec![n(target), y.clone(), t.clone()]), expected);
    }
}

/// Touchstone (eng21, Excel 20527): between two historical timestamps the
/// forecast is anchored at the later one with the slope of the interval after
/// it (bank2 history of row target 1.5325303972943622; period two).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_historical_fractional_targets_use_the_next_interval() {
    let t = col(&[1.0, 2.0, 3.0, 4.0, 5.0]);
    let y = col(&[1.9823275881758256, 2.3331159731655986, 3.6439711188652915, 4.641443873621043, 5.097678056076448]);
    for (target, expected) in [
        (1.25, 1.442160565938415),
        (1.5, 1.7756219285222303),
        (1.75, 2.109083291106046),
        (2.0, 2.442544653689861),
        (4.5, 5.39186507834005),
        (5.5, 5.955124126299383),
    ] {
        // The interval rule is Excel's; this period-two fit is still a few ULP
        // from Excel's own fitted values (dc-2u2g), so compare to 1e-14 relative.
        let got = call("FORECAST.ETS", vec![n(target), y.clone(), t.clone()]).unwrap().as_number().unwrap();
        assert!(((got - expected) / expected).abs() < 1e-14, "{target}: {got} vs {expected}");
    }
}

/// Touchstone (eng21, Excel 20527): explicit periods two and three over four
/// complete cycles use the measured cycle initialisation; Excel's selected
/// parameters and its fitted values at targets 11..15 (period two) and 11..16
/// (period three). The fitted values agree to 1e-12 relative, not yet in
/// every bit.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_cycle_initialisation_periods_two_and_three() {
    let t = col(&(1..=12).map(f64::from).collect::<Vec<_>>());
    for (period, y, fitted) in [
        (2.0, vec![1.286246, 2.183696, 2.828766, 4.660762, 4.334081, 7.711319, 6.322702, 9.217424, 8.931031, 11.099961, 10.793759, 12.256886],
         vec![10.682046388815891, 12.780776673670655, 12.66415109333057, 14.762881378185334, 14.64625579784525]),
        (3.0, vec![2.059292, 2.272977, 2.389685, 3.835833, 6.520125, 5.557926, 7.373293, 8.710849, 8.102256, 10.102443, 11.183192, 11.514564],
         vec![11.591218965442193, 11.202831529058448, 13.025029933139413, 14.467451056123496, 14.07906361973975, 15.901262023820717]),
    ] {
        let y = col(&y);
        for (stat, expected) in [(1.0, 0.251), (2.0, 0.001), (3.0, 0.001)] {
            exact(call("FORECAST.ETS.STAT", vec![y.clone(), t.clone(), n(stat), n(period)]), expected);
        }
        for (i, expected) in fitted.into_iter().enumerate() {
            let target = 11.0 + i as f64;
            let got = call("FORECAST.ETS", vec![n(target), y.clone(), t.clone(), n(period)]).unwrap().as_number().unwrap();
            assert!(((got - expected) / expected).abs() < 1e-12, "{period} {target}: {got} vs {expected}");
        }
    }
}

/// Touchstone (eng21, Excel 20527): periods four and six use the pivoted
/// cycle chain (pivot p/2 - 1); Excel's parameters and three forecasts each,
/// to 1e-12 relative.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_cycle_initialisation_periods_four_and_six() {
    for (period, y, parameters, forecasts) in [
        (4.0, vec![2.035978, 2.662543, 1.81852, 4.886643, 6.182574, 6.465446, 6.040515, 7.671862, 11.040199, 9.969509, 10.265558, 12.263876, 14.754488, 14.049714, 15.090689, 15.941141],
         [0.002, 0.001, 0.5], [18.81532561872999, 18.275706622437202, 18.720974435521384]),
        (6.0, vec![0.043147, 1.828801, 2.422828, 5.265925, 3.109382, 6.60286, 6.00061, 8.59762, 7.729794, 11.707896, 10.535819, 12.753818, 12.650636, 14.062224, 15.375087, 17.212355, 16.037124, 18.028995, 18.268118, 20.320263, 20.755535, 24.038281, 23.118296, 23.947682],
         [0.002, 0.001, 0.125], [24.613996034214324, 26.575424954442116, 26.718188348677717]),
    ] {
        let count = y.len();
        let t = col(&(1..=count).map(|i| i as f64).collect::<Vec<_>>());
        let y = col(&y);
        for (stat, expected) in [1.0, 2.0, 3.0].into_iter().zip(parameters) {
            exact(call("FORECAST.ETS.STAT", vec![y.clone(), t.clone(), n(stat), n(period)]), expected);
        }
        for (i, expected) in forecasts.into_iter().enumerate() {
            let target = (count + 1 + i) as f64;
            let got = call("FORECAST.ETS", vec![n(target), y.clone(), t.clone(), n(period)]).unwrap().as_number().unwrap();
            assert!(((got - expected) / expected).abs() < 1e-12, "{period} {target}: {got} vs {expected}");
        }
    }
}

/// Excel 20527 bit-exact bank pins (dc-iuwct.47): 2p sliding-window moving-average
/// seasonal initialisation, j=0 recurrence from L0 - T0, final seasonal centring,
/// factored class-1 variance and fractional forecast-variance interpolation.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn forecast_ets_sliding_ma_j0_and_variance_interpolation() {
    let t = col(&[1.0, 2.0, 3.0, 4.0, 5.0]);
    for (target, y, season, expected) in [
        // bank2-more-FORECAST.ETS-0881 (was 1440 ULP gross), explicit period 2
        (0x3fd5_4484_ce20_92a8_u64, [0x3ff1_0909_3072_ae4e_u64, 0x4003_e130_871e_32b6, 0x400a_07d6_ba9d_79b9, 0x4013_711d_ed41_69d7, 0x4017_1e35_de2c_f1fb], Some(2.0), 0xbf96_d99e_d230_3900_u64),
        // bank2-more-FORECAST.ETS-0320 (was 112 ULP)
        (0x3fb9_db39_86de_0f40, [0x3ff3_4321_3208_f454, 0x4005_c24d_60ad_4cd0, 0x400c_2b3b_2d0e_1038, 0x4011_d122_b7a8_7d1c, 0x4017_78c4_8351_5163], None, 0x3fbf_9569_887b_d670),
        // bank2-scalar-FORECAST.ETS-0700 (pre-history, distinguishes pre-centred L0 and j=0)
        (0x3fee_5874_edf9_2fe8, [0x3ff8_da33_16ba_b8f9, 0x4000_33f1_64a8_2778, 0x400e_4f69_1990_af08, 0x4011_5d2a_c380_e560, 0x4014_9e92_6dd7_9ba4], None, 0x3ff8_020f_5018_480f),
        // bank2-scalar-FORECAST.ETS-0573 (nonseasonal pre-history, distinguishes j=0 in Model::run)
        (0x3fe0_6147_de31_d092, [0x3ffe_ea96_6f4a_1f2b, 0x4005_76e5_f23e_92e7, 0x400f_5206_74f4_dbb3, 0x4012_ecfd_211b_f844, 0x4015_9f67_cb01_908e], Some(100.0), 0x3ff7_f976_aef7_707a),
    ] {
        let mut args = vec![n(f64::from_bits(target)), col(&y.map(f64::from_bits)), t.clone()];
        if let Some(s) = season {
            args.push(n(s));
        }
        exact(call("FORECAST.ETS", args), f64::from_bits(expected));
    }
    for (target, y, expected) in [
        // bank2-more-FORECAST.ETS.CONFINT-1395 (was 294 ULP without pre-centred L0 + j=0)
        (0x40ed_1da0_0000_0000_u64, [0x3ffe_97ba_73dd_5e10_u64, 0x4000_bad7_6161_cd2e, 0x400e_d669_50cb_bf0b, 0x4010_282d_4e8b_7c6f, 0x4017_4bbc_95f8_209b], 0x405d_0641_641b_ae9a_u64),
        // bank2-scalar-FORECAST.ETS.CONFINT-0780 (integer horizon, factored variance)
        (0x40e3_f940_0000_0000, [0x3ff3_e726_d6c1_bbbc, 0x4002_f9c0_034c_2c79, 0x400d_b346_9592_6f91, 0x4010_47e9_e882_3f43, 0x4015_1fee_db02_76f4], 0x40a4_4090_6fc0_36fa),
        // bank2-more-FORECAST.ETS.CONFINT-0600 (nonseasonal fractional horizon variance lerp)
        (0x4074_183c_9600_fab0, [0x3ffc_7a54_f53a_a6b2, 0x4007_fc1f_06e0_833e, 0x4009_6eb9_65f6_b0a8, 0x4010_c304_cac5_25ac, 0x4017_0c42_d0ed_4483], 0x4001_8313_26c7_6297),
        // bank2-more-FORECAST.ETS.CONFINT-0960 (seasonal fractional horizon variance lerp)
        (0x4064_efd9_c96c_cf38, [0x3ff3_406e_e300_82d6, 0x4002_2cc2_01ec_9c98, 0x400d_3608_dfaf_7662, 0x4010_96a4_e72b_a88e, 0x4016_406e_b788_b284], 0x4007_85f1_0bca_27ce),
    ] {
        exact(
            call("FORECAST.ETS.CONFINT", vec![n(f64::from_bits(target)), col(&y.map(f64::from_bits)), t.clone()]),
            f64::from_bits(expected),
        );
    }
}

/// Excel 16.0.20527 (dc-iuwct.59): GAMMAINV shares GAMMA.INV's 105-trial tolerance cap
/// and AS 91 power_seed sum order, eliminating all 16 structural and 148 of 150 gross rows.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_iuwct_59_gammainv_structural_and_gross_alias_parity() {
    for name in ["GAMMAINV", "GAMMA.INV"] {
        // ax2/gamma-halley-answers.jsonl $[10075] (structural -> #NUM!)
        assert_eq!(
            call(
                name,
                vec![
                    n(f64::from_bits(0x3fefa4e36c3bdd1e)),
                    n(f64::from_bits(0x3f99ce075f6fd220)),
                    n(1.0),
                ],
            ),
            Ok(CalcValue::error(calc_value::WorksheetErrorCode::Num)),
        );
        // ax2/gamma-halley-answers.jsonl $[9894] (structural -> finite, bit-exact),
        // $[9967] (58,578 ULP -> 0 ULP), ax2/gamma-round2 $[794] (3,975 ULP -> 0 ULP).
        for (p_bits, a_bits, expected) in [
            (0x3fefef942c5c3563_u64, 0x3f91b71758e21965_u64, 0x3ff6aa4ac43db371_u64),
            (0x3fefe649f7ff93f5_u64, 0x3f9374bc6a7ef9db_u64, 0x3ff2fedf3efed5cf_u64),
            (0x3feffec79e1d4427_u64, 0x3f18361fa517d270_u64, 0x3fc031781eeafa73_u64),
        ] {
            exact(
                call(
                    name,
                    vec![
                        n(f64::from_bits(p_bits)),
                        n(f64::from_bits(a_bits)),
                        n(1.0),
                    ],
                ),
                f64::from_bits(expected),
            );
        }
    }
}

/// Excel 16.0.20527.20196 (dc-iuwct.62 / dc-j9k.69): all 9 gross `intercept_se` rows from
/// `sx1/run2-regression-variance-intercept_se.json` (previously 27.7B..97.2B ULP off due to
/// SSE2 `f64::sqrt` instead of `excel_sqrt_staged` at QR step `j = 2`) now match bit-exactly.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_iuwct_62_linest_gross_intercept_se_rows_match_excel() {
    let y_unit = [
        f64::from_bits(0x3fe8_0000_0000_0000),
        f64::from_bits(0x4012_0000_0000_0000),
        f64::from_bits(0x3ff4_0000_0000_0000),
        f64::from_bits(0x4016_0000_0000_0000),
    ];
    let y = col(&y_unit.repeat(4));
    let cases: [(usize, [[u64; 2]; 4], u64); 9] = [
        (
            337,
            [
                [0xc02a_0000_0000_0000, 0xc03a_0000_8000_0000],
                [0xc026_0000_0000_0000, 0xc036_0000_8000_0000],
                [0xc02a_0000_0000_0000, 0xc039_ffff_8000_0000],
                [0xc026_0000_0000_0000, 0xc035_ffff_8000_0000],
            ],
            0x3fda_b7b2_267b_1fb0,
        ),
        (
            338,
            [
                [0xc018_0000_0000_0000, 0xc028_0001_0000_0000],
                [0xc010_0000_0000_0000, 0xc020_0001_0000_0000],
                [0xc018_0000_0000_0000, 0xc027_ffff_0000_0000],
                [0xc010_0000_0000_0000, 0xc01f_fffe_0000_0000],
            ],
            0x3fc6_a087_c5d5_84f3,
        ),
        (
            340,
            [
                [0xc000_0000_0000_0000, 0xc010_0002_0000_0000],
                [0x0000_0000_0000_0000, 0xbee0_0000_0000_0000],
                [0xc000_0000_0000_0000, 0xc00f_fffc_0000_0000],
                [0x0000_0000_0000_0000, 0x3ee0_0000_0000_0000],
            ],
            0x3fa9_1a4e_51e8_7492,
        ),
        (
            341,
            [
                [0xbfe0_0000_0000_0000, 0xbff0_0008_0000_0000],
                [0x3ff8_0000_0000_0000, 0x4007_fffc_0000_0000],
                [0xbfe0_0000_0000_0000, 0xbfeffff0_0000_0000],
                [0x3ff8_0000_0000_0000, 0x4008_0004_0000_0000],
            ],
            0x3fa3_d872_7d4c_8078,
        ),
        (
            342,
            [
                [0x0000_0000_0000_0000, 0xbee0_0000_0000_0000],
                [0x4000_0000_0000_0000, 0x400f_fffc_0000_0000],
                [0x0000_0000_0000_0000, 0x3ee0_0000_0000_0000],
                [0x4000_0000_0000_0000, 0x4010_0002_0000_0000],
            ],
            0x3fa9_1a4e_51e8_7492,
        ),
        (
            344,
            [
                [0x4010_0000_0000_0000, 0x401f_fffe_0000_0000],
                [0x4018_0000_0000_0000, 0x4027_ffff_0000_0000],
                [0x4010_0000_0000_0000, 0x4020_0001_0000_0000],
                [0x4018_0000_0000_0000, 0x4028_0001_0000_0000],
            ],
            0x3fc6_a087_c5d5_84f3,
        ),
        (
            345,
            [
                [0x4026_0000_0000_0000, 0x4035_ffff_8000_0000],
                [0x402a_0000_0000_0000, 0x4039_ffff_8000_0000],
                [0x4026_0000_0000_0000, 0x4036_0000_8000_0000],
                [0x402a_0000_0000_0000, 0x403a_0000_8000_0000],
            ],
            0x3fda_b7b2_267b_1fb0,
        ),
        (
            348,
            [
                [0x4010_0000_0000_0000, 0x4020_0003_0000_0000],
                [0x4018_0000_0000_0000, 0x4028_0003_0000_0000],
                [0x4010_0000_0000_0000, 0x4020_0005_0000_0000],
                [0x4018_0000_0000_0000, 0x4028_0005_0000_0000],
            ],
            0x3fcc_c239_cac7_63fc,
        ),
        (
            349,
            [
                [0xc018_0000_0000_0000, 0xc028_0005_0000_0000],
                [0xc010_0000_0000_0000, 0xc020_0005_0000_0000],
                [0xc018_0000_0000_0000, 0xc028_0003_0000_0000],
                [0xc010_0000_0000_0000, 0xc020_0003_0000_0000],
            ],
            0x3fcc_c239_cac7_63fc,
        ),
    ];
    for (row_idx, x_unit, want) in cases {
        let x_rows: Vec<Vec<f64>> = (0..16)
            .map(|i| vec![f64::from_bits(x_unit[i % 4][0]), f64::from_bits(x_unit[i % 4][1])])
            .collect();
        let result = call("LINEST", vec![y.clone(), arr(x_rows), b(true), b(true)])
            .expect("LINEST result");
        let grid = result.as_array().expect("LINEST statistics array");
        let got = grid.get(1, 2).expect("intercept_se cell [2, 3]");
        assert_eq!(
            got.as_number().expect("numeric intercept_se").to_bits(),
            want,
            "intercept_se[{row_idx}]"
        );
    }
}

/// Excel 16.0.20527 (dc-j9k.84 / cb14): unified AS 109 / bratio beta inverse and tiny-shape
/// substrate covering:
/// 1. Direct `beta_dist_kernel` Newton evaluation without `gamma_limit` diversion (dc-j9k.55).
/// 2. Unified `base <= 0` midpoint `0.5`, `700`-trial `best_err <= 1e-12 * p` publication
///    (dc-ymjf.2.47), `a < 1 || b < 1` subnormal `x*(1-x)*density` zeroing (dc-ymjf.2.38.2),
///    and natural `#NUM!` without hardcoded bounding boxes in `distributions.rs`.
/// 3. Subnormal `LowerPower` `xb = flush_subnormal(x0 * b0)` split in `bratio` (dc-ymjf.2.38).
/// 4. Removal of `bratio_fpser`/`bratio_apser` and the tiny-`b` bounding box (dc-j9k.45).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_j9k_84_cb14_unified_beta_inverse_and_tiny_shape_substrate() {
    // 1. No gamma_limit diversion in beta_newton_lower (cb14 unseen holdout h1-glim).
    for (name, p, a, b, want) in [
        ("BETA.INV", 0x3fb6666666666666_u64, 0x3fb5c28f5c28f5c3_u64, 0x440a000000000000_u64, 0x393294e2865874db_u64),
        ("BETAINV",  0x3fb6666666666666,     0x3fb5c28f5c28f5c3,     0x440a000000000000,     0x393294e2865874db),
        ("BETA.INV", 0x3fc8a3d70a3d70a4,     0x3fc28f5c28f5c28f,     0x4592000000000000,     0x393b2f4a8d29c7ea),
        ("BETA.INV", 0x3fd4000000000000,     0x3fce147ae147ae14,     0x477c000000000000,     0x37e640cc78a28a83),
        ("BETA.INV", 0x3cf52abb27830526,     0x3fe2666666666666,     0x4544000000000000,     0x35687546b10f1983),
        ("BETAINV",  0x3b0c0c0d5ece020c,     0x3fe7333333333333,     0x46dc000000000000,     0x3241ee0054ed7fab),
        ("BETA.INV", 0x3f9916872b020c4a,     0x3fc8f5c28f5c28f6,     0x44a4cccccccccccd,     0x3977937e32bd043d),
        ("BETA.INV", 0x3fb147ae147ae148,     0x3fd23d70a3d70a3d,     0x45eb333333333333,     0x39109ef3f56fd660),
    ] {
        exact(
            call(name, vec![n(f64::from_bits(p)), n(f64::from_bits(a)), n(f64::from_bits(b))]),
            f64::from_bits(want),
        );
    }

    // 2a. base <= 0 !x.is_finite() -> 0.5 and 700-trial exhaustion best_err <= 1e-12 * p_work
    //     (cb14 unseen holdout h2-mid, h3-sym, h3-asym).
    for (name, p, a, b, lo_hi, want) in [
        ("BETA.INV", 0x3fe0000000000000_u64, 0x2f58a9ac84b164ae_u64, 0x2f58a9ac84b164ae_u64, None, Some(0x3fe0000000000000_u64)),
        ("BETAINV",  0x3fdffffffffff1ed,     0x2f58a9ac84b164ae,     0x2f58a9ac84b164ae,     Some((0xc008000000000000, 0x4022000000000000)), Some(0x4008000000000000)),
        ("BETA.INV", 0x3fe0000000000709,     0x3286bf5935fdfc4c,     0x3286bf5935fdfc4c,     Some((0x4010000000000000, 0x4028000000000000)), Some(0x4020000000000000)),
        ("BETA.INV", 0x3fcffffffffff1ed,     0x3075b605314ad5c2,     0x305cf2b1970e7258,     None, Some(0x3fe0000000000000)),
        ("BETAINV",  0x3fe8000000000385,     0x3075b605314ad5c2,     0x305cf2b1970e7258,     Some((0xc024000000000000, 0x4000000000000000)), None),
        ("BETA.INV", 0x3fdfffffffffdddf,     0x2e6483bbbe8fbccf,     0x2e6483bbbe8fbccf,     None, Some(0x3fe0000000000000)),
        ("BETA.INV", 0x3fdfffffffffdbc3,     0x2e6483bbbe8fbccf,     0x2e6483bbbe8fbccf,     None, None),
        ("BETAINV",  0x3fe0000000001110,     0x2e6483bbbe8fbccf,     0x2e6483bbbe8fbccf,     None, Some(0x3fe0000000000000)),
        ("BETAINV",  0x3fe000000000121f,     0x2e6483bbbe8fbccf,     0x2e6483bbbe8fbccf,     None, None),
        ("BETA.INV", 0x3fe8000000000818,     0x327288f35b66a7a7,     0x328bcd6d0919fb7a,     None, Some(0x3fe0000000000000)),
        ("BETAINV",  0x3fe7fffffffff7e8,     0x327288f35b66a7a7,     0x328bcd6d0919fb7a,     None, Some(0x3eb0c6f7a0b80000)),
        ("BETA.INV", 0x3fe8000000000980,     0x327288f35b66a7a7,     0x328bcd6d0919fb7a,     None, None),
        ("BETA.INV", 0x3fd000000000102f,     0x328bcd6d0919fb7a,     0x327288f35b66a7a7,     Some((0xc000000000000000, 0x4020000000000000)), Some(0x401ffffd60e94ee4)),
        ("BETAINV",  0x3fcfffffffffdfa1,     0x328bcd6d0919fb7a,     0x327288f35b66a7a7,     Some((0xc000000000000000, 0x4020000000000000)), Some(0x4008000000000000)),
        ("BETA.INV", 0x3fd0000000001300,     0x328bcd6d0919fb7a,     0x327288f35b66a7a7,     Some((0xc000000000000000, 0x4020000000000000)), None),
    ] {
        let mut args = vec![n(f64::from_bits(p)), n(f64::from_bits(a)), n(f64::from_bits(b))];
        if let Some((lo, hi)) = lo_hi {
            args.push(n(f64::from_bits(lo)));
            args.push(n(f64::from_bits(hi)));
        }
        match want {
            Some(bits) => exact(call(name, args), f64::from_bits(bits)),
            None => error(call(name, args), Num),
        }
    }

    // 2b. Tiny-second-shape midpoint publication when x*(1-x)*density flushes subnormal
    //     for a < 1 || b < 1 (cb14 probe1 q4-eb100 & unseen holdout h2-brc).
    for (name, p, a, b, lo_hi, want) in [
        ("BETA.INV", 0x3fd142e8aa838d27_u64, 0x4076d00000000000_u64, 0x16ccc28cd3b63796_u64, None, Some(0x3fe0000000000000_u64)),
        ("BETA.INV", 0x3fd142e8aa838d27,     0x4076d00000000000,     0x16de32ad77e5ba5e,     None, Some(0x3fe0000000000000)),
        ("BETA.INV", 0x3fd142e8aa838d27,     0x4076d00000000000,     0x16e0897760225ff6,     None, None),
        ("BETA.INV", 0x3fcb851eb851eb85,     0x4075400000000000,     0x1544cccccccccbac,     None, Some(0x3fe0000000000000)),
        ("BETAINV",  0x3fcb851eb851eb85,     0x4075400000000000,     0x1544cccccccccbac,     None, Some(0x3fe0000000000000)),
        ("BETA.INV", 0x3fd570a3d70a3d71,     0x4078600000000000,     0x1864cccccccccc82,     Some((0xc010000000000000, 0x4028000000000000)), Some(0x4010000000000000)),
        ("BETAINV",  0x3fd570a3d70a3d71,     0x4078600000000000,     0x1864cccccccccc82,     Some((0xc010000000000000, 0x4028000000000000)), Some(0x4010000000000000)),
    ] {
        let mut args = vec![n(f64::from_bits(p)), n(f64::from_bits(a)), n(f64::from_bits(b))];
        if let Some((lo, hi)) = lo_hi {
            args.push(n(f64::from_bits(lo)));
            args.push(n(f64::from_bits(hi)));
        }
        match want {
            Some(bits) => exact(call(name, args), f64::from_bits(bits)),
            None => error(call(name, args), Num),
        }
    }

    // 3. Subnormal LowerPower seed xb = flush_subnormal(x0 * b0) split in bratio
    //    (cb14 probe1 q2-bsweep & unseen holdout h5-xb / h5-b1).
    for (name, p, a, b, lo_hi, want) in [
        ("BETA.INV", 0x3fdf5c28f5c28f5c_u64, 0x3f50624dd2f1a9fc_u64, 0x3fefffffffffffff_u64, None, Some(0x0000000000000000_u64)),
        ("BETA.INV", 0x3fdf5c28f5c28f5c,     0x3f50624dd2f1a9fc,     0x3ff0000000000000,     None, Some(0x0000000000000000)),
        ("BETA.INV", 0x3fdf5c28f5c28f5c,     0x3f50624dd2f1a9fc,     0x3ff0000000000001,     None, None),
        ("BETA.INV", 0x3fde6e3b6d703c71,     0x3f513404ea4a8c15,     0x4004000000000000,     None, None),
        ("BETA.INV", 0x3fde6e3b6d703c92,     0x3f513404ea4a8c15,     0x4004000000000000,     None, Some(0x0000000000000000)),
        ("BETA.INV", 0x3fe0c8e24947e1e0,     0x4004000000000000,     0x3f513404ea4a8c15,     None, None),
        ("BETAINV",  0x3fe0c8e24947e19f,     0x4004000000000000,     0x3f513404ea4a8c15,     None, Some(0x3ff0000000000000)),
        ("BETA.INV", 0x3fc245ef6af1a73f,     0x3f66872b020c49ba,     0x4030000000000000,     Some((0xc014000000000000, 0x4014000000000000)), None),
        ("BETA.INV", 0x3fc245ef6af1a760,     0x3f66872b020c49ba,     0x4030000000000000,     Some((0xc014000000000000, 0x4014000000000000)), Some(0xc014000000000000)),
        ("BETAINV",  0x3fdf333333333333,     0x3f508c3f3e0370ce,     0x3ff0000000000000,     None, Some(0x0000000000000000)),
        ("BETAINV",  0x3fdf333333333333,     0x3f508c3f3e0370ce,     0x3ff0000000000001,     None, None),
    ] {
        let mut args = vec![n(f64::from_bits(p)), n(f64::from_bits(a)), n(f64::from_bits(b))];
        if let Some((lo, hi)) = lo_hi {
            args.push(n(f64::from_bits(lo)));
            args.push(n(f64::from_bits(hi)));
        }
        match want {
            Some(bits) => exact(call(name, args), f64::from_bits(bits)),
            None => error(call(name, args), Num),
        }
    }

    // 4. Removal of bratio_fpser / bratio_apser and tiny-b bounding box
    //    (cb14 probe1 q4-apser & unseen holdout h6-noap / h6-nofp).
    for (name, x, a, b, lo_hi, want) in [
        ("BETA.DIST", 0x3fec000000000000_u64, 0x3fd0000000000000_u64, 0x36cb15789abcdef0_u64, None, 0x397738d157589634_u64),
        ("BETA.DIST", 0x3fec000000000000,     0x4008000000000000,     0x36cb15789abcdef0,     None, 0x397015c1bbc3afb4),
        ("BETA.DIST", 0x3fea000000000000,     0x3fd6666666666666,     0x3827d1dc29f529c8,     None, 0x39550d5bbeef7679),
        ("BETADIST",  0x3fea000000000000,     0x3fd6666666666666,     0x3827d1dc29f529c8,     None, 0x39550d5bbeef7679),
        ("BETA.DIST", 0x4004a3d70a3d70a4,     0x4013000000000000,     0x2d70e84b465ecbc6,     Some((0xbff0000000000000, 0x4008000000000000)), 0x397606d17e9762b8),
        ("BETA.DIST", 0x3fb0000000000000,     0x3fc3333333333333,     0x36aed41bdd0ad013,     None, 0x36d117a65899fa26),
        ("BETADIST",  0x3fb0000000000000,     0x3fc3333333333333,     0x36aed41bdd0ad013,     None, 0x36d117a65899fa26),
        ("BETA.DIST", 0x0010000000000000,     0x3fd199999999999a,     0x350f6a895e8456c2,     None, 0x239b96580910cf60),
        ("BETADIST",  0x3fec000000000000,     0x3fed70a3d70a3d71,     0x3308063ce2a70ec2,     Some((0x0000000000000000, 0x4010000000000000)), 0x32ecf954ce3336d0),
    ] {
        let mut args = vec![n(f64::from_bits(x)), n(f64::from_bits(a)), n(f64::from_bits(b))];
        if name == "BETA.DIST" {
            args.push(f1_support::b(true));
        }
        if let Some((lo, hi)) = lo_hi {
            args.push(n(f64::from_bits(lo)));
            args.push(n(f64::from_bits(hi)));
        }
        exact(call(name, args), f64::from_bits(want));
    }
}

/// Excel 16.0.20527 (dc-iuwct.51): LowerEndpoint damped-Newton step `(factor * df) / density`
/// and small-shape `beta_density_at` TwoSum exponent accumulation for `a < 1 || b < 1`.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_iuwct_51_beta_lower_endpoint_and_small_shape_bup_bgrat_exact() {
    // 1. Gross LowerEndpoint $[7] (alias_7, previously 258,048 ULP) and ww2-b003 (previously
    //    -18,272 ULP), plus all ig5 (dc-iuwct.51-BETA.INV.json) and cb14 (b1-00..04) b=1 controls.
    for (name, p, a, b, want) in [
        // alias_7 ($[7])
        ("BETA.INV", 0x3e88a7338c2c15fb_u64, 0x3f894d4606760df7_u64, 0x3f91c3f4398310c2_u64, 0x1ffac33bbe071000_u64),
        ("BETAINV",  0x3e88a7338c2c15fb,     0x3f894d4606760df7,     0x3f91c3f4398310c2,     0x1ffac33bbe071000),
        // ww2-b003
        ("BETA.INV", 0x36a0000000000000,     0x3fc0000000000000,     0x3f9f1e29b6b2af14,     0x2092e4d14ab15c68),
        ("BETAINV",  0x36a0000000000000,     0x3fc0000000000000,     0x3f9f1e29b6b2af14,     0x2092e4d14ab15c68),
        // ig5 b=1 controls
        ("BETA.INV", 0x16687e92154ef7ac,     0x3fe0000000000000,     0x3ff0000000000000,     0x20ac572b87d36f98),
        ("BETA.INV", 0x20ca2fe76a3f9475,     0x3fd8000000000000,     0x3ff0000000000000,     0x2092e4c7afe24340),
        ("BETA.INV", 0x2b2bff2ee48e0530,     0x3fc0000000008000,     0x3ff0000000000000,     0x2092e4c7b2b06760),
        ("BETA.INV", 0x2b2bff2ee48e0530,     0x3fbfffffffff0000,     0x3ff0000000000000,     0x2092e4c7ad146020),
        ("BETA.INV", 0x20ca2fe76a3f9475,     0x3fd8000431bde82d,     0x3ff0000000000000,     0x2092f47d455f3d00),
        ("BETA.INV", 0x20ca2fe76a3f9475,     0x3fd7fffbce4217d3,     0x3ff0000000000000,     0x2092d51f04084ae8),
        // cb14 b=1 controls (b1-00..04)
        ("BETA.INV", 0x26f0000000000000,     0x3fd5555555555556,     0x3ff0000000000000,     0x2086500000000000),
        ("BETA.INV", 0x26f0000000000000,     0x3fd5555555555600,     0x3ff0000000000000,     0x1fe1600000000000),
        ("BETA.INV", 0x26f0000000000000,     0x3fd5555555559bb3,     0x3ff0000000000000,     0x1f95310000000000),
        ("BETA.INV", 0x26f0000000000000,     0x3fd5555560000000,     0x3ff0000000000000,     0x1f70c6f744000000),
        ("BETA.INV", 0x26f0000000000000,     0x3fd5558000000000,     0x3ff0000000000000,     0x20b0c2c66c160000),
    ] {
        exact(
            call(
                name,
                vec![
                    n(f64::from_bits(p)),
                    n(f64::from_bits(a)),
                    n(f64::from_bits(b)),
                ],
            ),
            f64::from_bits(want),
        );
    }

    // 2. Small-shape BETA.DIST(FALSE) TwoSum witnesses (previously off under excel_exp_running).
    for (x, a, b, want) in [
        (0x3ed3ae194e5d7827_u64, 0x3fe0000000000000_u64, 0x4054c00000000000_u64, 0x40a280d95d81fc71_u64),
        (0x3feffffde7210be9,     0x3fab37d6d5b62db4,     0x2caad073096dd14e,     0x2de992748b5f753b),
        (0x3feffffde7210be9,     0x3fcd471b52759270,     0x30ff10ad44d18f65,     0x323da044fd4c19c0),
    ] {
        exact(
            call(
                "BETA.DIST",
                vec![
                    n(f64::from_bits(x)),
                    n(f64::from_bits(a)),
                    n(f64::from_bits(b)),
                    f1_support::b(false),
                ],
            ),
            f64::from_bits(want),
        );
    }

    // 3. Remaining $[6] (alias_6) Newton trajectory: 165/166 BETA.DIST calls across all 83 steps
    //    match live Excel 16.0.20527 to 0 ULP; only iteration 1 CDF sits +1 ULP above Excel on an
    //    exact .5000 tie in 0.5 + (0.5 - tot) (owned by dc-j9k.55 / dc-j9k.70), amplifying across
    //    81 catastrophic-cancellation steps.
    recorded_calculation(
        call(
            "BETA.DIST",
            vec![
                n(f64::from_bits(0x3e50e6ec4e3332c0)),
                n(f64::from_bits(0x3f7120226a7e99dd)),
                n(f64::from_bits(0x3f9f1e28d638730c)),
                f1_support::b(true),
            ],
        ),
        f64::from_bits(0x3fea196f31880203),
        f64::from_bits(0x3fea196f31880204),
    );
    for name in ["BETA.INV", "BETAINV"] {
        recorded_calculation(
            call(
                name,
                vec![
                    n(f64::from_bits(0x3e833184a7ea4652)),
                    n(f64::from_bits(0x3f7120226a7e99dd)),
                    n(f64::from_bits(0x3f9f1e28d638730c)),
                ],
            ),
            f64::from_bits(0x1ffeb1641b0fe800),
            f64::from_bits(0x1ffeb1641b0e0dc0),
        );
    }
}

/// Excel 16.0.20527.20196 (dc-ymjf.2.154, frozen Touchstone proofs SHA-256 9614eed5, 8afdac85,
/// fd2b2800, 848245d9): extreme-count non-compensation overflows in `POISSON`/`POISSON.DIST`
/// and `GAMMA.DIST`/`GAMMADIST`.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ymjf_2_154_poisson_and_gamma_extreme_count_overflows() {
    let m_below_11 = f64::from_bits(0x3ff1_9999_9999_9999);
    let m_exact_11 = f64::from_bits(0x3ff1_9999_9999_999a);
    let a_edge_lo = f64::from_bits(0x79b4_484b_feeb_c29f); // DBL_MAX * 1e-30 (a / 1e-30 finite)
    let a_edge_hi = f64::from_bits(0x79b4_484b_feeb_c2a0); // +1 ULP (a / 1e-30 == +Inf)

    // 1. Strict x < 1.1 && a / 1e-30 == +Inf boundary in gratio (exact 1-ULP edge).
    for name in ["POISSON.DIST", "POISSON"] {
        exact(call(name, vec![n(a_edge_lo), n(1.0), f1_support::b(false)]), 0.0);
        exact(call(name, vec![n(a_edge_lo), n(1.0), f1_support::b(true)]), 1.0);
        exact(call(name, vec![n(a_edge_hi), n(1.0), f1_support::b(false)]), 0.0);
        error(call(name, vec![n(a_edge_hi), n(1.0), f1_support::b(true)]), Num);
        // Bead cases: x = 1e300, 1e305 with mean in {0.5, 0.75, 1.0} (FALSE=0, TRUE=#NUM!)
        // and x = 1e305, mean = 1.1 (FALSE=0, TRUE=1).
        for x in [1e300_f64, 1e305_f64] {
            for m in [0.5_f64, 0.75, 1.0] {
                exact(call(name, vec![n(x), n(m), f1_support::b(false)]), 0.0);
                error(call(name, vec![n(x), n(m), f1_support::b(true)]), Num);
            }
        }
        exact(call(name, vec![n(1e305), n(m_exact_11), f1_support::b(false)]), 0.0);
        exact(call(name, vec![n(1e305), n(m_exact_11), f1_support::b(true)]), 1.0);
        // Strict < 1.1 boundary at x = 1e295 (0x3ff1999999999999 vs 0x3ff199999999999a).
        exact(call(name, vec![n(1e295), n(m_below_11), f1_support::b(false)]), 0.0);
        error(call(name, vec![n(1e295), n(m_below_11), f1_support::b(true)]), Num);
        exact(call(name, vec![n(1e295), n(m_exact_11), f1_support::b(false)]), 0.0);
        exact(call(name, vec![n(1e295), n(m_exact_11), f1_support::b(true)]), 1.0);
        // Tiny mean (l = mean / a == 0 in binary64) does not bypass gratio or compensation #NUM!.
        error(call(name, vec![n(1e250), n(1e-100), f1_support::b(false)]), Num);
        error(call(name, vec![n(1e250), n(1e-100), f1_support::b(true)]), Num);
        let a250_neg = f64::from_bits(1e250_f64.to_bits() + 1);
        exact(call(name, vec![n(a250_neg), n(1e-100), f1_support::b(false)]), 0.0);
        exact(call(name, vec![n(a250_neg), n(1e-100), f1_support::b(true)]), 1.0);
    }
    for name in ["GAMMA.DIST", "GAMMADIST"] {
        exact(call(name, vec![n(1.0), n(a_edge_lo), n(1.0), f1_support::b(true)]), 0.0);
        error(call(name, vec![n(1.0), n(a_edge_hi), n(1.0), f1_support::b(true)]), Num);
        exact(call(name, vec![n(m_below_11), n(1e295), n(1.0), f1_support::b(false)]), 0.0);
        error(call(name, vec![n(m_below_11), n(1e295), n(1.0), f1_support::b(true)]), Num);
        exact(call(name, vec![n(m_exact_11), n(1e295), n(1.0), f1_support::b(true)]), 0.0);
        // Scaled y = x / beta < 1.1 vs >= 1.1.
        error(call(name, vec![n(2.0), n(1e295), n(2.0), f1_support::b(true)]), Num);
        exact(call(name, vec![n(2.2), n(1e295), n(2.0), f1_support::b(true)]), 0.0);
    }

    // 2. Loader deviance x * ln(x / lambda) overflow (exact 1-ULP edges across m = 2, 1e100, 1e200).
    for (m, lo_bits, hi_bits) in [
        (2.0_f64, 0x7f57_523e_3f25_3287_u64, 0x7f57_523e_3f25_3288_u64),
        (1e100,   0x7f61_4e50_07a7_5011,     0x7f61_4e50_07a7_5012),
        (1e200,   0x7f70_cd81_b34e_8ae7,     0x7f70_cd81_b34e_8ae8),
    ] {
        let x_lo = f64::from_bits(lo_bits);
        let x_hi = f64::from_bits(hi_bits);
        for name in ["POISSON.DIST", "POISSON"] {
            exact(call(name, vec![n(x_lo), n(m), f1_support::b(false)]), 0.0);
            exact(call(name, vec![n(x_lo), n(m), f1_support::b(true)]), 1.0);
            error(call(name, vec![n(x_hi), n(m), f1_support::b(false)]), Num);
            error(call(name, vec![n(x_hi), n(m), f1_support::b(true)]), Num);
        }
        for name in ["GAMMA.DIST", "GAMMADIST"] {
            exact(call(name, vec![n(m), n(x_lo), n(1.0), f1_support::b(false)]), 0.0);
            exact(call(name, vec![n(m), n(x_lo), n(1.0), f1_support::b(true)]), 0.0);
            error(call(name, vec![n(m), n(x_hi), n(1.0), f1_support::b(false)]), Num);
            error(call(name, vec![n(m), n(x_hi), n(1.0), f1_support::b(true)]), Num);
        }
    }
    for name in ["POISSON.DIST", "POISSON"] {
        error(call(name, vec![n(1e306), n(2.0), f1_support::b(false)]), Num);
        error(call(name, vec![n(1e306), n(2.0), f1_support::b(true)]), Num);
        exact(call(name, vec![n(1e306), n(1e300), f1_support::b(false)]), 0.0);
        exact(call(name, vec![n(1e306), n(1e300), f1_support::b(true)]), 1.0);
    }

    // 3. Loader 2 * pi * x overflow boundary (0x7fc45f306dc9c882 vs 0x7fc45f306dc9c883):
    //    #NUM! in POISSON(.DIST) and #DIV/0! in GAMMA(.DIST) PDF.
    let x_2pi_lo = f64::from_bits(0x7fc4_5f30_6dc9_c882);
    let x_2pi_hi = f64::from_bits(0x7fc4_5f30_6dc9_c883);
    for name in ["POISSON.DIST", "POISSON"] {
        exact(
            call(name, vec![n(x_2pi_lo), n(x_2pi_lo), f1_support::b(false)]),
            f64::from_bits(0x1ff0_0000_0000_0000),
        );
        error(call(name, vec![n(x_2pi_hi), n(x_2pi_hi), f1_support::b(false)]), Num);
        error(call(name, vec![n(2.9e307), n(1e307), f1_support::b(false)]), Num);
        exact(call(name, vec![n(1e307), n(1e300), f1_support::b(false)]), 0.0);
    }
    for name in ["GAMMA.DIST", "GAMMADIST"] {
        error(
            call(name, vec![n(x_2pi_hi), n(x_2pi_hi), n(1.0), f1_support::b(false)]),
            Div0,
        );
        error(
            call(name, vec![n(1e307), n(2.9e307), n(1.0), f1_support::b(false)]),
            Div0,
        );
        error(
            call(name, vec![n(2.0), n(1e308), n(1.0), f1_support::b(false)]),
            Div0,
        );
    }
}

/// Touchstone Excel 16.0.20625 (dc-j9k.459) distinguishes overflow of the Poisson
/// normalizer in the gamma CDF from overflow in the deviance or the lower fraction.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_cdf_reports_poisson_normalizer_overflow() {
    let lower_shape = f64::from_bits(0x7fc4_5f30_6dc9_c882);
    let overflowing_shape = f64::from_bits(0x7fc4_5f30_6dc9_c883);
    let loader_start = f64::from_bits(1.1_f64.to_bits() + 1);
    for name in ["GAMMA.DIST", "GAMMADIST"] {
        let cdf = |x, a, beta| call(name, vec![n(x), n(a), n(beta), f1_support::b(true)]);
        error(cdf(loader_start, lower_shape, 1.0), Num);
        error(cdf(1.1, overflowing_shape, 1.0), Num);
        error(cdf(loader_start, overflowing_shape, 1.0), Div0);
        exact(cdf(overflowing_shape, overflowing_shape, 1.0), 0.5);
        let large_shape = f64::from_bits(0x7fdd_65f0_937d_a3fa);
        error(cdf(0.1, large_shape, 1e-7), Div0);
    }
}

/// Touchstone Excel 16.0.20625 (dc-j9k.459): the density gives #NUM! for an overflowing alpha/x.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_density_reports_overflow_of_the_scale_before_the_normalizer() {
    let shape = f64::from_bits(0x7fc4_5f30_6dc9_c883);
    let overflowing_x = f64::from_bits(0x3fc4_5f30_6dc9_c883);
    let finite_x = f64::from_bits(0x3fc4_5f30_6dc9_c885);
    for name in ["GAMMA.DIST", "GAMMADIST"] {
        let pdf = |x| call(name, vec![n(x), n(shape), n(1e-7), f1_support::b(false)]);
        error(pdf(overflowing_x), Num);
        error(pdf(finite_x), Div0);
    }
}

/// Excel 16.0.20527.20196 (dc-ymjf.2.148, frozen Touchstone proofs SHA-256 f0cc1863, c85660b2,
/// 99593b34, 5309b8c6): five-point strictly monotone histories route through automatic seasonality
/// (period 0 vs period 2) without refusal, and both nonseasonal and seasonal forecast publication
/// store `horizon * model.trend` and `model.level + product` via x87 PC64 before adding the
/// seasonal term (`excel_x87_add(excel_x87_add(level, excel_x87_mul(h, trend)), season)`).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_ymjf_2_148_ets_five_point_monotone_and_x87_forecast_staging() {
    let t5 = col(&[1.0, 2.0, 3.0, 4.0, 5.0]);
    // 1. Bead counterexample (five-point strictly monotone near-linear history): automatic
    //    seasonality is 0, and targets 3048 / 6091 / 9308 separate x87-stored `h * trend`
    //    (20/20 on frozen Touchstone discriminators) from binary64 `*` (0/20).
    let koder_y: Vec<f64> = [
        0x4058_63b6_36b1_c960_u64,
        0x4058_a3b9_11f7_75d7,
        0x4058_e3b9_e15b_6c9b,
        0x4059_23a9_a589_a2fe,
        0x4059_63a0_e1e7_6302,
    ]
    .into_iter()
    .map(f64::from_bits)
    .collect();
    exact(
        call("FORECAST.ETS.SEASONALITY", vec![col(&koder_y), t5.clone()]),
        0.0,
    );
    for (target, want_bits) in [
        (3048.0, 0x40a8_8ec8_71d5_27be_u64),
        (6091.0, 0x40b8_2939_e52c_f558),
        (9308.0, 0x40c2_5c7f_413b_cbf9),
    ] {
        exact(
            call("FORECAST.ETS", vec![n(target), col(&koder_y), t5.clone()]),
            f64::from_bits(want_bits),
        );
    }

    // 2. Nonseasonal `level + product` x87_add discriminators (16/16 vs 0/16 binary64 `+`).
    let add_y: Vec<f64> = [
        0x3fef_ffff_fff0_5b30_u64,
        0x4001_ffff_fffc_16cc,
        0x400f_ffff_fffc_16cc,
        0x4018_ffff_fffe_0b66,
        0x4021_ffff_ffff_05b3,
    ]
    .into_iter()
    .map(f64::from_bits)
    .collect();
    for (target, want_bits) in [
        (1_007_904.0, 0x413f_4b2f_5ba7_59ce_u64),
        (1_015_823.0, 0x413f_8a20_cbd5_74c8),
    ] {
        exact(
            call("FORECAST.ETS", vec![n(target), col(&add_y), t5.clone(), n(0.0)]),
            f64::from_bits(want_bits),
        );
    }

    // 3. Seasonal (p = 2, n = 5) `h * trend` x87_mul discriminators (10/10 vs 0/10 binary64 `*`).
    let seas_y = col(&[0.0, 1.25, 2.0, 3.5, 4.0]);
    exact(
        call("FORECAST.ETS.SEASONALITY", vec![seas_y.clone(), t5.clone()]),
        2.0,
    );
    for (target, want_bits) in [
        (11_138.0, 0x40c7_01e5_3e78_7783_u64),
        (13_448.0, 0x40cb_c788_9137_2559),
    ] {
        exact(
            call("FORECAST.ETS", vec![n(target), seas_y.clone(), t5.clone(), n(2.0)]),
            f64::from_bits(want_bits),
        );
    }
}

/// Live Excel 16.0.20625.20108 (dc-j9k.185, Touchstone 2026-11-25): RANDBETWEEN is #NUM! only when the
/// raw bottom exceeds the raw top; otherwise the low end is ceil(bottom) and a top that floors below it
/// collapses to it ((1.2,1.8) = 2 in 8 of 8 draws, (-0.5,-0.025) = 0, (2.5,3.7) only 3).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn randbetween_collapses_an_empty_integer_range_to_its_low_end() {
    for (lo, hi, unit, want) in [
        (1.2, 1.8, 0.0, 2.0),
        (1.2, 1.8, 0.99, 2.0),
        (1.5, 1.5, 0.5, 2.0),
        (-1.5, -1.2, 0.5, -1.0),
        (-0.5, -0.025, 0.7, 0.0),
        (0.5, 0.9, 0.3, 1.0),
        (2.5, 3.7, 0.99, 3.0),
        (-3.7, -2.5, 0.99, -3.0),
        (-0.6, -0.5, 0.4, 0.0),
    ] {
        exact(call_random(&mut Fixed(vec![unit]), "RANDBETWEEN", vec![n(lo), n(hi)]), want);
    }
    for (lo, hi) in [(3.0, 2.0), (2.5, 2.4), (-0.5, -0.6), (2.0, 1.9999999), (5.5, 5.2)] {
        error(call_random(&mut Fixed(vec![0.5]), "RANDBETWEEN", vec![n(lo), n(hi)]), Num);
    }
}

/// Excel 20527 (dc-j9k.240-243): CHITEST pairs cells in row-major order whatever the shapes, as long
/// as the cell counts agree; the degrees of freedom come from the actual range; a zero expected
/// value is #DIV/0! but a negative one only counts in the statistic; text and blank pairs are skipped.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chitest_pairs_by_cell_count_and_reads_negative_and_zero_expected() {
    let b = f64::from_bits;
    for name in ["CHITEST", "CHISQ.TEST"] {
        // dc-j9k.240: a negative expected cell does not refuse when the statistic stays positive.
        exact(
            call(name, vec![
                arr(vec![vec![b(0xc05d_6834_8823_4c3c)], vec![b(0xc073_7ada_a543_37ab)]]),
                arr(vec![vec![b(0xc07a_b2df_db82_e234)], vec![b(0x407b_6c9f_bd86_7e2c)]]),
            ]),
            b(0x0fd7_efe4_de8f_15e8),
        );
        error(call(name, vec![arr(vec![vec![1.0, 2.0], vec![3.0, 4.0]]), arr(vec![vec![-1.0, 2.0], vec![3.0, 4.0]])]), Num);
        // dc-j9k.241/242: same cell count, different shape: df from the actual range.
        exact(
            call(name, vec![col(&[1.0, 2.0, 3.0, 4.0]), arr(vec![vec![15.0, 15.0], vec![30.0, 40.0]])]),
            b(0x3c75_3da3_dd12_50ab),
        );
        error(call(name, vec![arr(vec![vec![1.0, 2.0, 3.0]]), col(&[-1.0, -2.0, -3.0])]), Num);
        exact(call(name, vec![arr(vec![vec![1.0, 2.0, 3.0]]), col(&[1.0, 2.0, 3.0])]), 1.0);
        error(call(name, vec![arr(vec![vec![1.0, 2.0], vec![3.0, 4.0]]), arr(vec![vec![1.0, 2.0, 3.0]])]), NA);
        // dc-j9k.243: a zero expected value.
        error(call(name, vec![
            arr(vec![vec![10.0, 20.0], vec![30.0, 40.0]]), arr(vec![vec![0.0, 1.0], vec![1.0, 1.0]])]), Div0);
        error(call(name, vec![arr(vec![vec![1.0, 5.0]]), arr(vec![vec![0.0, -100.0]])]), Div0);
        // Errors in the ranges come first, the actual range before the expected one.
        let mut sheet = Sheet::new();
        sheet.column(1, 1, vec![n(1.0), t("x"), n(3.0)]);
        sheet.column(1, 2, vec![n(2.0), n(2.0), n(2.0)]);
        sheet.column(1, 3, vec![e(Ref), n(1.0)]);
        sheet.column(1, 4, vec![e(NA), n(1.0)]);
        sheet.column(1, 5, vec![n(0.0), n(1.0)]);
        sheet.column(1, 6, vec![n(1.0), n(5.0)]);
        sheet.column(1, 7, vec![t("x"), t("y")]);
        let (a1, a2, a3, a4, a5, a6, a7) = (
            sheet.area(1, 1, 3, 1), sheet.area(1, 2, 3, 2), sheet.area(1, 3, 2, 3), sheet.area(1, 4, 2, 4),
            sheet.area(1, 5, 2, 5), sheet.area(1, 6, 2, 6), sheet.area(1, 7, 2, 7));
        exact(call_on(&mut sheet, name, vec![a1, a2]), (-0.5f64).exp());
        error(call_on(&mut sheet, name, vec![a3.clone(), a4]), Ref);
        error(call_on(&mut sheet, name, vec![a6, a3]), Ref);
        // No numeric pair at all: #DIV/0!.
        error(call_on(&mut sheet, name, vec![a7, a5]), Div0);
    }
}

/// Excel 20527 (dc-j9k.307): CHIINV admits degrees of freedom above 1e10 (the stored answers are
/// the Halley solver's), and on step exhaustion publishes the best x when the next step is below
/// one relative epsilon (CHIINV(0.1, 1e10) and GAMMA.INV(0.5, 5.696e9, 1)).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn chi_inverse_above_ten_billion_degrees_and_exhaustion_at_one_epsilon() {
    let b = f64::from_bits;
    for name in ["CHIINV", "CHISQ.INV.RT"] {
        exact(call(name, vec![n(0.625), n(b(0x4202_a05f_2008_0000))]), b(0x4202_a059_9fcf_eb8e));
        exact(call(name, vec![n(0.5), n(b(0x4202_a05f_3f40_0000))]), b(0x4202_a05f_3f3a_aaab));
        exact(call(name, vec![n(0.1), n(b(0x4202_a05f_2000_0000))]), b(0x4202_a075_3fb9_8286));
        error(call(name, vec![n(0.5), n(b(0x4212_a05f_2000_0000))]), Num);
    }
    exact(call("GAMMA.INV", vec![n(0.5), n(b(0x41f5_383a_5480_0000)), n(1.0)]), b(0x41f5_383a_547a_aaab));
}

/// Excel 20527 (dc-j9k.324-327): the LOGNORM.DIST density is #DIV/0! when the variance is subnormal
/// or twice it overflows, and is zero (not #NUM!) when the squared distance over the variance overflows.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn lognorm_density_variance_underflow_and_overflow_are_div0() {
    let b = f64::from_bits;
    let flag = |x: bool| f1_support::b(x);
    for name in ["LOGNORM.DIST"] {
        let pdf = |x: f64, mean: f64, sigma: f64| call(name, vec![n(x), n(mean), n(sigma), flag(false)]);
        error(pdf(10.0, b(0x3e7a_d7f2_9abc_af48), b(0x7fc3_ba4f_e83b_c71b)), Div0);
        error(pdf(b(0x3fb9_9999_9999_999a), b(0x3e7a_d7f2_9abc_af48), b(0x7fba_14c9_a21c_7df7)), Div0);
        error(pdf(b(0x3fb9_9999_9999_999a), -0.5, b(0x0010_0000_0000_0001)), Div0);
        error(pdf(1.0, b(0x3fb9_9999_9999_999a), b(0x0010_0000_0000_0003)), Div0);
        error(pdf(10.0, 0.0, 1e154), Div0);
        error(pdf(10.0, 0.0, 1e-154), Div0);
        exact(pdf(10.0, 0.0, b(0x2000_0000_0000_0000)), 0.0);
        exact(pdf(10.0, 0.0, b(0x2000_0000_0000_0028)), 0.0);
        // The cumulative form divides by sigma, not by its square.
        assert!(number(call(name, vec![n(10.0), n(0.0), n(1e-154), flag(true)])) >= 0.0);
    }
}

/// Excel 20527 (dc-j9k.309, .354): a significance of 309 or more places is #DIV/0! (10^309
/// overflows) in PERCENTRANK, PERCENTRANK.INC and PERCENTRANK.EXC, after the out-of-range
/// (#N/A) and the single-member checks; 308 places still answer.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn percentrank_significance_beyond_308_places_is_div0() {
    for name in ["PERCENTRANK", "PERCENTRANK.INC", "PERCENTRANK.EXC"] {
        let data = || col(&[1.0, 2.0, 4.0, 7.0, 11.0]);
        error(call(name, vec![data(), n(3.3), n(309.0)]), Div0);
        assert!(number(call(name, vec![data(), n(3.3), n(308.99999999)])) > 0.0); // truncates to 308
        error(call(name, vec![data(), n(4.0), n(1e100)]), Div0);
        error(call(name, vec![data(), n(100.0), n(309.0)]), NA);
        error(call(name, vec![data(), n(3.3), n(-400.0)]), Num);
        exact(call(name, vec![col(&[5.0]), n(5.0), n(309.0)]), 1.0);
    }
    exact(call("PERCENTRANK.INC", vec![col(&[1.0, 2.0, 4.0, 7.0, 11.0]), n(4.0), n(308.0)]), 0.5);
}

/// Excel 20527 (dc-j9k.310/.311/.312/.355/.356): the truncated rank is published from the
/// 15-digit reading as N * 10^(places - k) / POWER(10, places), so high significance carries the
/// scale's rounding: PERCENTRANK.INC of these two-member ranges differs from the plain
/// truncation by 1 to 12 ulps.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn percentrank_publishes_the_fifteen_digit_reading_through_the_decimal_scale() {
    let b = f64::from_bits;
    let pair = |lo: u64, hi: u64| col(&[b(lo), b(hi)]);
    for name in ["PERCENTRANK", "PERCENTRANK.INC"] {
        exact(
            call(name, vec![pair(0x403c_80c7_3bcd_cbc0, 0x408b_1fff_7237_acf1), n(b(0x4062_5ffe_0000_0000)), n(b(0x4063_3800_0000_0000))]),
            b(0x3fc2_1145_962b_771d),
        );
        exact(
            call(name, vec![pair(0x403c_80c7_3bcd_cbc0, 0x408b_1fff_7237_acf1), n(b(0x4052_5ffe_0000_0000)), n(b(0x4063_3800_0000_0000))]),
            b(0x3fab_716e_2c52_88c6),
        );
        exact(
            call(name, vec![pair(0xc08b_6e83_a2d1_de9e, 0x408d_ccfb_2936_3193), n(b(0x4072_2fc0_0000_0000)), n(b(0x4071_4002_0000_0000))]),
            b(0x3fe4_6c06_7b1f_2ddf),
        );
        exact(
            call(name, vec![col(&[1.0, 2.0, 4.0, 7.0, 11.0]), n(3.3), n(100.0)]),
            b(0x3fda_6666_6666_6667),
        );
        exact(
            call(name, vec![col(&[1.0, 2.0, 3.0, 4.0]), n(2.0), n(23.0)]),
            f64::from_bits(0x3fd5_5555_5555_5550 + 0),
        );
    }
}

/// Excel 20527 (dc-j9k.313, .328, .331): SUMX2MY2, SUMX2PY2 and SUMXMY2 pair the cells in
/// row-major order when the two ranges have the same count of cells, whatever their shapes.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sum_of_squares_pairs_ranges_of_equal_count_with_different_shapes() {
    let square = || arr(vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
    let column = || col(&[1.0, 2.0, 4.0, 8.0]);
    exact(call("SUMX2MY2", vec![square(), column()]), -55.0);
    exact(call("SUMX2PY2", vec![square(), column()]), 115.0);
    exact(call("SUMXMY2", vec![square(), column()]), 17.0);
    for name in ["SUMX2MY2", "SUMX2PY2", "SUMXMY2"] {
        error(call(name, vec![square(), col(&[1.0, 2.0, 3.0])]), NA);
    }
}

/// Excel 20527 (dc-j9k.286-288): TINV and T.INV.2T answer #DIV/0! at one degree and #NUM! at more
/// for a probability of either sign whose magnitude lies from DBL_MIN to 0x001ffffffffffffe;
/// 0x001fffffffffffff and above are computed.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn t_inverse_two_tailed_refuses_the_first_normal_binade_of_probability() {
    let b = f64::from_bits;
    for name in ["TINV", "T.INV.2T"] {
        for p in [0x0010_0000_0000_0000_u64, 0x0010_0000_0000_0001, 0x0010_0000_0000_0253, 0x001f_ffff_ffff_fffe] {
            error(call(name, vec![n(b(p)), n(1.0)]), Div0);
            error(call(name, vec![n(-b(p)), n(1.0)]), Div0);
            for df in [2.0, 3.0, 4.0, 100.0] {
                error(call(name, vec![n(b(p)), n(df)]), Num);
                error(call(name, vec![n(-b(p)), n(df)]), Num);
            }
        }
        assert!(number(call(name, vec![n(b(0x001f_ffff_ffff_ffff)), n(1.0)])) > 0.0);
        assert!(number(call(name, vec![n(b(0x0020_0000_0000_0000)), n(4.0)])) > 0.0);
        error(call(name, vec![n(-b(0x001f_ffff_ffff_ffff)), n(1.0)]), Num);
    }
}

/// Excel 20527 (dc-j9k.284): GAMMAINV and GAMMA.INV answer #NUM! when the small-shape power seed's
/// logarithm overflows in the division by the shape (shape DBL_MIN, p = 0.0074), not zero; a seed
/// that only underflows (p = 0.3) is still zero.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_inverse_small_shape_seed_overflow_is_num() {
    let b = f64::from_bits;
    for name in ["GAMMAINV", "GAMMA.INV"] {
        for beta in [b(0x0010_0000_0000_0000), b(0x0010_0000_0000_0002), 1.0, 1e10] {
            error(call(name, vec![n(b(0x3f7e_4f76_5fd8_adac)), n(b(0x0010_0000_0000_0000)), n(beta)]), Num);
            error(call(name, vec![n(b(0x3f7e_4f76_5fd8_adac)), n(b(0x0010_0000_0000_0002)), n(beta)]), Num);
        }
    }
}

/// Excel 20527 (dc-j9k.255): WEIBULL.DIST and WEIBULL answer #NUM! when the stored exponent
/// product alpha * ln(x / beta) overflows (a huge shape with a ratio below one), not zero.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn weibull_exponent_product_overflow_is_num() {
    let b = f64::from_bits;
    for name in ["WEIBULL.DIST", "WEIBULL"] {
        for cumulative in [true, false] {
            error(
                call(name, vec![
                    n(b(0x7f5d_3f1e_e25a_84df)), n(b(0x7fd8_d766_ca02_60b9)), n(b(0x7fc9_c0d7_b054_f143)),
                    f1_support::b(cumulative),
                ]),
                Num,
            );
        }
    }
}

/// Excel 20527 (dc-j9k.337, .338): GAMMA of an integer is the reverse product through 88 and the
/// Stirling series from 89 (the reverse product is 1 to 16 ulps off from there).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn gamma_integers_switch_to_the_stirling_series_at_89() {
    let b = f64::from_bits;
    exact(call("GAMMA", vec![n(100.0)]), b(0x6051_66c6_98cf_183e));
    exact(call("GAMMA", vec![n(b(0x4061_8000_0000_0000))]), b(0x718d_8895_7d1c_3025));
    exact(call("GAMMA", vec![n(89.0)]), b(0x5bd0_550c_4b30_7440));
    let mut product = 1.0;
    for k in (2..88).rev() {
        product *= k as f64;
    }
    exact(call("GAMMA", vec![n(88.0)]), product);
}

/// Excel 20527 (dc-j9k.282, .283): below |x| = 1 SINH is the half difference of two expm1 values,
/// each with its arithmetic stored through x87, which truncates where the binary64 form rounds up
/// for inputs with trailing zero mantissa bits (SINH(2^-25) is 2^-25, not one ulp above).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn sinh_small_arguments_use_the_stored_expm1_difference() {
    let b = f64::from_bits;
    for (x, want) in [
        (0xbe90_0000_0000_0000_u64, 0xbe90_0000_0000_002a_u64),
        (0xbe8f_ffff_ffff_fef8, 0xbe8f_ffff_ffff_ff4c),
        (0x3e60_0000_0000_0000, 0x3e60_0000_0000_0000),
        (0x3e70_0000_0000_0000, 0x3e70_0000_0000_0002),
        (0x3e80_0000_0000_0000, 0x3e80_0000_0000_000a),
        (0x3ea0_0000_0000_0000, 0x3ea0_0000_0000_00aa),
        (0x3ea0_0000_0000_1000, 0x3ea0_0000_0000_10aa),
    ] {
        exact(call("SINH", vec![n(b(x))]), b(want));
    }
}

/// Excel 20527 (dc-j9k.359): CONFIDENCE.NORM scales the inverse by the x87-stored quotient
/// sd / sqrt(n); with a huge deviation the binary64 product was one ulp low.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn confidence_norm_scales_by_the_stored_quotient() {
    let b = f64::from_bits;
    for name in ["CONFIDENCE.NORM", "CONFIDENCE"] {
        exact(
            call(name, vec![n(b(0x3fea_74f0_f20f_283a)), n(b(0x7fe1_ccf3_85eb_c8a1)), n(b(0x430c_6bf5_2634_0010))]),
            b(0x7e30_88a0_a567_ff78),
        );
    }
}

/// dc-j9k.505 (Excel 20625): the cumulative and right-tail F calls store the ratio d2 / (d1 x) when
/// d1 x < d2 (beyond the binary64 range it is #NUM!), and read a stored beta argument below the
/// normal range as zero.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn f_dist_tiny_x_ratio_overflow_and_subnormal_argument() {
    use calc_func::value::WorksheetErrorCode::Num;
    let f = |name: &str, x: f64, d1: f64, d2: f64, cum: Option<bool>| {
        let mut args = vec![n(x), n(d1), n(d2)];
        if let Some(c) = cum {
            args.push(b(c));
        }
        call(name, args)
    };
    error(f("F.DIST", f64::from_bits(0x0010_0000_0000_0003), 2.0, 2000.0, Some(true)), Num);
    error(f("F.DIST", 1e-307, 2.0, 2000.0, Some(true)), Num);
    error(f("F.DIST.RT", 1e-307, 2.0, 2000.0, None), Num);
    error(f("FDIST", 1e-307, 2.0, 2000.0, None), Num);
    exact(f("F.DIST", 1e-305, 2.0, 2000.0, Some(true)), 0.0);
    exact(f("F.DIST", 1e-307, 1.0, 10.0, Some(true)), 0.0);
    exact(f("F.DIST", 2.3e-308, 1.0, 3.0, Some(true)), 0.0);
    exact(f("F.DIST.RT", 1e-307, 1.0, 1.0, None), 1.0);
}

/// dc-j9k.520 (Excel 20625): SKEW.P takes SKEW's stored x87 stages with the population divisor. Four
/// range arguments of mixed magnitude, the fuzz representative, ended one ulp low in plain f64.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn skew_p_stores_every_stage_like_skew() {
    let v = |bits: u64| f64::from_bits(bits);
    let range = |rows: Vec<Vec<f64>>| arr(rows);
    exact(
        call(
            "SKEW.P",
            vec![
                range(vec![vec![v(0xc0189ae766bd2376), v(0xc0e22b5e3d3b7a0c), v(0x40dc124de1f5abc0)]]),
                range(vec![vec![v(0x3f23756a3566ec57)], vec![v(0xbf15536ff536e47b)], vec![v(0xc0057111aa501a0f)]]),
                range(vec![vec![v(0x3f3c7d7cf5adee2c), v(0xc0d41794df78c9a6)], vec![v(0x40880a2c3d2139a0), v(0x405ca4f2c59039e1)]]),
                range(vec![vec![v(0xbf0d4c7adb931cf0)], vec![v(0xc029962b25dba29a)], vec![v(0x403997460e1d68e5)]]),
            ],
        ),
        v(0xbfe15796826e4e3f),
    );
}
