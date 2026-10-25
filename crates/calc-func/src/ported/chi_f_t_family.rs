use crate::ported::special_dist_family::erfc_of_sqrt_half_x;
use crate::ported::special_math_common::{bisect_inverse, bratio, gamma, regularized_gamma_q};
use crate::value::WorksheetErrorCode;
use crate::ported::excel_numeric::crt_pow;

fn truncate_positive_integer(value: f64) -> Result<f64, WorksheetErrorCode> {
    let truncated = value.trunc();
    // Excel docs: #NUM! when deg_freedom > 1e10 (strictly greater); 1e10 itself is admitted.
    if !truncated.is_finite() || truncated < 1.0 || truncated > 1e10 {
        Err(WorksheetErrorCode::Num)
    } else {
        Ok(truncated)
    }
}

fn validate_probability_open_unit(probability: f64) -> Result<f64, WorksheetErrorCode> {
    if !probability.is_finite() || probability <= 0.0 || probability >= 1.0 {
        Err(WorksheetErrorCode::Num)
    } else {
        Ok(probability)
    }
}

fn validate_nonnegative_x(x: f64) -> Result<f64, WorksheetErrorCode> {
    if !x.is_finite() || x < 0.0 {
        Err(WorksheetErrorCode::Num)
    } else {
        Ok(x)
    }
}

fn density_flag(value: f64) -> bool {
    value != 0.0
}

pub fn chisq_pdf_kernel(x: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let k = truncate_positive_integer(deg_freedom)?;
    if x == 0.0 {
        return if k == 1.0 {
            // CDF limit is finite but density diverges to +inf; Excel returns #NUM!
            Err(WorksheetErrorCode::Num)
        } else if k == 2.0 {
            Ok(0.5)
        } else {
            Ok(0.0)
        };
    }
    let half_k = k / 2.0;
    Ok(crt_pow(x, half_k - 1.0) * (-x / 2.0).exp() / (crt_pow(2.0_f64, half_k) * gamma(half_k)))
}

pub fn chisq_dist_kernel(
    x: f64,
    deg_freedom: f64,
    cumulative: bool,
) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let k = truncate_positive_integer(deg_freedom)?;
    if cumulative {
        // Inverse-problem identity, live Excel 16.0 b20228: CHISQ.DIST(x,1,TRUE)
        // == ERF.PRECISE(SQRT(x/2)) == GAMMA.DIST(x,0.5,2,TRUE) on 154/154
        // distinct x. Rival stagings SQRT(x)/SQRT(2) and SQRT(x)*(1/SQRT(2))
        // fail the same bank.
        // Inverse-problem identity, live Excel 16.0 b20228:
        // CHISQ.DIST(x,df,TRUE) == GAMMA.DIST(x, df/2, 2, TRUE) on 88/88
        // across df in {1,2,3,4,5,6,8,10}. 1-CHIDIST is not the CDF (49/88).
        // Routes df=1/2 through the landed ERF/EXPON gamma specials.
        crate::ported::beta_gamma_stats_family::gamma_dist_kernel(x, k / 2.0, 2.0, true).map_err(
            |e| match e {
                code => {
                    code
                }
                _ => WorksheetErrorCode::Value,
            },
        )
    } else {
        chisq_pdf_kernel(x, k)
    }
}

pub fn chisq_dist_rt_kernel(x: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let k = truncate_positive_integer(deg_freedom)?;
    // Same capture: CHIDIST(x,1) == CHISQ.DIST.RT(x,1) == ERFC.PRECISE(SQRT(x/2))
    // bit-exactly. Routing through the published ERFC kernel is the identified
    // graph, not a numeric fit.
    if k == 1.0 {
        erfc_of_sqrt_half_x(x)
    } else if k == 2.0 {
        // Inverse-problem identity, live Excel 16.0 b20228: CHIDIST(x,2)
        // == EXP(-x/2) == EXP(-(x/2)) on 68/68 nonnegative rows. The
        // worksheet EXP is the signed-off x87 elementary. The complementary
        // CDF 1-EXP(-x/2) is NOT this graph (9/11 on a follow-up bank).
        Ok(crate::ported::excel_numeric::excel_exp(-(x / 2.0)))
    } else {
        Ok(regularized_gamma_q(k / 2.0, x / 2.0))
    }
}

fn search_upper_bound<F>(target: f64, initial_hi: f64, f: F) -> f64
where
    F: Fn(f64) -> f64,
{
    let mut hi = initial_hi.max(1.0);
    for _ in 0..200 {
        if f(hi) >= target {
            return hi;
        }
        hi *= 2.0;
    }
    hi
}

pub fn chisq_inv_kernel(probability: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let k = truncate_positive_integer(deg_freedom)?;
    // Inverse-problem identity, live Excel 16.0 b20228: CHISQ.INV(p, df)
    // == GAMMA.INV(p, df/2, 2) on 63/63 interior pairs. Endpoints also
    // match: CHISQ.INV(0,2)=0 and CHISQ.INV(1,2)=#NUM!. CHIINV /
    // CHISQ.INV.RT is the right-tail inverse, not GAMMA.INV(1-p,...) (34/63).
    if !probability.is_finite() {
        return Err(WorksheetErrorCode::Num);
    }
    crate::ported::beta_gamma_stats_family::gamma_inv_kernel(probability, k / 2.0, 2.0).map_err(
        |e| match e {
            code => code,
            _ => WorksheetErrorCode::Value,
        },
    )
}

pub fn chisq_inv_rt_kernel(probability: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let p = validate_probability_open_unit(probability)?;
    let k = truncate_positive_integer(deg_freedom)?;
    // Excel inverts its published right-tail surface (CHIDIST = Q) directly,
    // not P at 1-p: the complement staging carries a systematic -5..-33-ULP
    // bias on the b14 CHIINV corpus (W109). Q is decreasing in x, so invert
    // the negated forward at -p to keep the increasing-forward convention.
    let hi = search_upper_bound(-p, k, |x| -regularized_gamma_q(k / 2.0, x / 2.0));
    Ok(bisect_inverse(-p, 0.0, hi, |x| {
        -regularized_gamma_q(k / 2.0, x / 2.0)
    }))
}

pub fn f_pdf_kernel(x: f64, deg1: f64, deg2: f64) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let d1 = truncate_positive_integer(deg1)?;
    let d2 = truncate_positive_integer(deg2)?;
    if x == 0.0 {
        return if d1 < 2.0 {
            // Density diverges to +inf; Excel returns #NUM!
            Err(WorksheetErrorCode::Num)
        } else if d1 == 2.0 {
            Ok(1.0)
        } else {
            Ok(0.0)
        };
    }
    let half_d1 = d1 / 2.0;
    let half_d2 = d2 / 2.0;
    let num = crt_pow(d1 / d2, half_d1) * crt_pow(x, half_d1 - 1.0);
    let den = gamma(half_d1) * gamma(half_d2) / gamma(half_d1 + half_d2)
        * crt_pow(1.0 + d1 * x / d2, (d1 + d2) / 2.0);
    Ok(num / den)
}

pub fn f_dist_kernel(
    x: f64,
    deg1: f64,
    deg2: f64,
    cumulative: bool,
) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let d1 = truncate_positive_integer(deg1)?;
    let d2 = truncate_positive_integer(deg2)?;
    if cumulative {
        // W109: Excel's beta wrappers pass the ACCURATE complement pair, not
        // 1-z (identified stagings; b22 check).
        let num = d1 * x;
        let den = num + d2;
        Ok(bratio(d1 / 2.0, d2 / 2.0, num / den, d2 / den).0)
    } else {
        f_pdf_kernel(x, d1, d2)
    }
}

pub fn f_dist_rt_kernel(x: f64, deg1: f64, deg2: f64) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let d1 = truncate_positive_integer(deg1)?;
    let d2 = truncate_positive_integer(deg2)?;
    // W109 identified staging: x = d2/den, y = d1*F/den (accurate complement).
    let num = d1 * x;
    let den = d2 + num;
    Ok(bratio(d2 / 2.0, d1 / 2.0, d2 / den, num / den).0)
}

pub fn f_inv_kernel(probability: f64, deg1: f64, deg2: f64) -> Result<f64, WorksheetErrorCode> {
    let p = validate_probability_open_unit(probability)?;
    let d1 = truncate_positive_integer(deg1)?;
    let d2 = truncate_positive_integer(deg2)?;
    let fwd = move |x: f64| {
        let num = d1 * x;
        let den = num + d2;
        bratio(d1 / 2.0, d2 / 2.0, num / den, d2 / den).0
    };
    let hi = search_upper_bound(p, 1.0, fwd);
    Ok(bisect_inverse(p, 0.0, hi, fwd))
}

pub fn f_inv_rt_kernel(probability: f64, deg1: f64, deg2: f64) -> Result<f64, WorksheetErrorCode> {
    let p = validate_probability_open_unit(probability)?;
    let d1 = truncate_positive_integer(deg1)?;
    let d2 = truncate_positive_integer(deg2)?;
    // Invert the published right-tail surface (f_dist_rt's accurate complement
    // form) directly at p, not the CDF at 1-p — same principle as CHIINV
    // (W109 b14+b19: the complement staging carries a systematic small-p bias).
    // The surface is decreasing in x, so invert the negated forward at -p.
    let f = move |x: f64| {
        let num = d1 * x;
        let den = d2 + num;
        -bratio(d2 / 2.0, d1 / 2.0, d2 / den, num / den).0
    };
    let hi = search_upper_bound(-p, 1.0, f);
    Ok(bisect_inverse(-p, 0.0, hi, f))
}

fn t_pdf(x: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let v = truncate_positive_integer(deg_freedom)?;
    if v == 1.0 {
        // Live Excel 16.0 b20326: T.DIST(x,1,FALSE)=1/(PI()*(1+x*x)) 20/20.
        // 1/PI()/(1+x*x) is 12/20; 1/(PI()+PI()*x*x) is 18/20.
        return Ok(1.0 / (std::f64::consts::PI * (1.0 + x * x)));
    }
    if v == 2.0 {
        // Live Excel 16.0 b20326: T.DIST(x,2,FALSE)=POWER(2+x*x,-1.5) 15/15.
        // 1/EXP(1.5*LN) is 15/15 in Excel but only 10/15 via spilled excel_exp/log.
        // excel_pow_chain is 15/15 including |x|=4,8.
        return Ok(crate::ported::excel_numeric::excel_pow_chain(2.0 + x * x, -1.5));
    }
    let numerator = gamma((v + 1.0) / 2.0);
    let denominator = (v * std::f64::consts::PI).sqrt()
        * gamma(v / 2.0)
        * crt_pow(1.0 + x * x / v, (v + 1.0) / 2.0);
    Ok(numerator / denominator)
}

fn t_cdf(x: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let v = truncate_positive_integer(deg_freedom)?;
    // W109 identified staging: x = df/den, y = t^2/den (accurate complement).
    let t2 = x * x;
    let den = v + t2;
    let ib = bratio(v / 2.0, 0.5, v / den, t2 / den).0;
    Ok(if x >= 0.0 { 1.0 - 0.5 * ib } else { 0.5 * ib })
}

pub fn t_dist_kernel(
    x: f64,
    deg_freedom: f64,
    cumulative: bool,
) -> Result<f64, WorksheetErrorCode> {
    if cumulative {
        t_cdf(x, deg_freedom)
    } else {
        t_pdf(x, deg_freedom)
    }
}

pub fn t_dist_rt_kernel(x: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let v = truncate_positive_integer(deg_freedom)?;
    let t2 = x * x;
    let den = v + t2;
    Ok(0.5 * bratio(v / 2.0, 0.5, v / den, t2 / den).0)
}

pub fn t_dist_2t_kernel(x: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let x = validate_nonnegative_x(x)?;
    let v = truncate_positive_integer(deg_freedom)?;
    let t2 = x * x;
    let den = v + t2;
    Ok(bratio(v / 2.0, 0.5, v / den, t2 / den).0)
}

pub fn t_inv_kernel(probability: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let p = validate_probability_open_unit(probability)?;
    let v = truncate_positive_integer(deg_freedom)?;
    if p == 0.5 {
        return Ok(0.0);
    }
    if p < 0.5 {
        return Ok(-t_inv_kernel(1.0 - p, v)?);
    }
    if v == 1.0 {
        // Live Excel 16.0 b20326: T.INV(p,1)=T.INV.2T(2*(1-p),1) 56/56.
        return t_inv_2t_kernel(2.0 * (1.0 - p), v);
    }
    let hi = search_upper_bound(p, 1.0, |x| t_cdf(x, v).unwrap_or(1.0));
    Ok(bisect_inverse(p, 0.0, hi, |x| t_cdf(x, v).unwrap_or(1.0)))
}

pub fn t_inv_2t_kernel(probability: f64, deg_freedom: f64) -> Result<f64, WorksheetErrorCode> {
    let p = validate_probability_open_unit(probability)?;
    let v = truncate_positive_integer(deg_freedom)?;
    if v == 1.0 {
        // Live Excel 16.0 b20326: T.INV.2T(p,1)=1/TAN(PI()*p/2) 9/9.
        // Cauchy closed form; the BETA.INV sqrt graph misses 3/9 here.
        // p=0.5 is the exact quartile 1; worksheet TAN(PI()/4) is 1, but
        // excel_tan(π/4) is 1 ULP high so 1/tan would publish 1+1 ULP.
        if p == 0.5 {
            return Ok(1.0);
        }
        let arg = std::f64::consts::PI * p / 2.0;
        return Ok(1.0 / crate::ported::tan::tan_kernel(arg));
    }
    // df>=2: Excel is SQRT(df*(1/BETA.INV(p,df/2,0.5)-1)) 135/135 vs the
    // worksheet BETA.INV, but production BETA.INV is still a few ULP off
    // Excel, so the composition is not landable yet. Keep the published
    // two-tail bratio inverter (W109 b19).
    let f = move |x: f64| {
        let t2 = x * x;
        let den = v + t2;
        -bratio(v / 2.0, 0.5, v / den, t2 / den).0
    };
    let hi = search_upper_bound(-p, 1.0, f);
    Ok(bisect_inverse(-p, 0.0, hi, f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::WorksheetErrorCode;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chisq_df1_matches_published_erf_erfc_sqrt_half_x() {
        use crate::ported::special_dist_family::{erf_of_sqrt_half_x, erfc_of_sqrt_half_x};
        for x in [0.0, 0.25, 0.5, 1.0, 2.0, 3.841458820694124, 10.0, 20.0] {
            let rt = chisq_dist_rt_kernel(x, 1.0).unwrap();
            let cdf = chisq_dist_kernel(x, 1.0, true).unwrap();
            assert_eq!(
                rt.to_bits(),
                erfc_of_sqrt_half_x(x).unwrap().to_bits(),
                "rt x={x}"
            );
            assert_eq!(
                cdf.to_bits(),
                erf_of_sqrt_half_x(x).unwrap().to_bits(),
                "cdf x={x}"
            );
        }
        // Exact publication at zero does not depend on the ERFC body.
        assert_eq!(
            chisq_dist_rt_kernel(0.0, 1.0).unwrap().to_bits(),
            0x3ff0000000000000
        );
        // Live Excel 16.0 b20228 CHIDIST(1,1) is 0x3fd44ed0bb7cb209.
        // The current ERFC.PRECISE body is still one ULP off that witness;
        // the df=1 dispatch only claims the composition, not the ERFC body.
        for x in [0.0, 0.125, 0.5, 1.0, 2.0, 8.0, 16.0, 40.0] {
            let rt = chisq_dist_rt_kernel(x, 2.0).unwrap();
            let exp = crate::ported::excel_numeric::excel_exp(-(x / 2.0));
            assert_eq!(rt.to_bits(), exp.to_bits(), "df=2 rt x={x}");
            let cdf = chisq_dist_kernel(x, 2.0, true).unwrap();
            let expon =
                crate::ported::discrete_dist_family::expon_dist_kernel(x / 2.0, 1.0, true)
                    .unwrap();
            assert_eq!(cdf.to_bits(), expon.to_bits(), "df=2 cdf x={x}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn even_df_chidist_matches_poisson_cdf_identity() {
        // Live Excel 16.0 b20228: CHIDIST(x, 2(k+1)) == POISSON.DIST(k, x/2, TRUE).
        // After the Poisson CDF dispatch, OxFunc reproduces that identity.
        for i in 1..40 {
            let x = 0.5 * f64::from(i);
            let chi4 = chisq_dist_rt_kernel(x, 4.0).unwrap();
            let pois1 =
                crate::ported::discrete_dist_family::poisson_dist_kernel(1.0, x / 2.0, true)
                    .unwrap();
            assert_eq!(chi4.to_bits(), pois1.to_bits(), "df=4 x={x}");
            let chi6 = chisq_dist_rt_kernel(x, 6.0).unwrap();
            let pois2 =
                crate::ported::discrete_dist_family::poisson_dist_kernel(2.0, x / 2.0, true)
                    .unwrap();
            assert_eq!(chi6.to_bits(), pois2.to_bits(), "df=6 x={x}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chisq_cdf_matches_gamma_scale_two_identity() {
        for df in [1.0, 2.0, 3.0, 4.0, 5.0, 10.0] {
            for x in [0.0, 0.5, 1.0, 2.0, 8.0, 20.0] {
                let chi = chisq_dist_kernel(x, df, true).unwrap();
                let gam = crate::ported::beta_gamma_stats_family::gamma_dist_kernel(
                    x,
                    df / 2.0,
                    2.0,
                    true,
                )
                .unwrap();
                assert_eq!(chi.to_bits(), gam.to_bits(), "df={df} x={x}");
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chisq_inv_matches_gamma_inv_scale_two_identity() {
        for df in [1.0, 2.0, 3.0, 4.0, 5.0, 10.0] {
            for p in [0.01, 0.05, 0.25, 0.5, 0.9, 0.99] {
                let chi = chisq_inv_kernel(p, df).unwrap();
                let gam =
                    crate::ported::beta_gamma_stats_family::gamma_inv_kernel(p, df / 2.0, 2.0)
                        .unwrap();
                assert_eq!(chi.to_bits(), gam.to_bits(), "df={df} p={p}");
            }
        }
        assert_eq!(chisq_inv_kernel(0.0, 2.0), Ok(0.0));
        assert_eq!(chisq_inv_kernel(1.0, 2.0), Err(WorksheetErrorCode::Num));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chisq_dist_rt_df2_matches_live_excel_exp_pins() {
        // Live Excel 16.0 b20326: CHISQ.DIST.RT(x,2)=EXP(-x/2) 7/7.
        assert_eq!(
            chisq_dist_rt_kernel(1.0, 2.0).unwrap().to_bits(),
            0x3fe368b2fc6f960a
        );
        assert_eq!(
            chisq_dist_rt_kernel(2.0, 2.0).unwrap().to_bits(),
            0x3fd78b56362cef38
        );
        assert_eq!(
            chisq_dist_rt_kernel(10.0, 2.0).unwrap().to_bits(),
            0x3f7b993fe00d5376
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn t_inv_2t_df1_matches_live_excel_tan_pins() {
        // Live Excel 16.0 build 20326 / CV2, Value2 cell-ref injection.
        // T.INV.2T(p,1)=1/TAN(PI()*p/2), with p=0.5 published as exact 1.
        let rows: [(f64, u64); 9] = [
            (0.001_f64, 0x4083e4f438b2cde9),
            (0.01_f64, 0x404fd410182c3c38),
            (0.02_f64, 0x403fd20d55634e2e),
            (0.05_f64, 0x40296993aacc4d25),
            (0.1_f64, 0x4019414813ba662c),
            (0.2_f64, 0x40089f188bdcd7b0),
            (0.3_f64, 0x3fff66da45fee3f1),
            (0.4_f64, 0x3ff605a90c73ab79),
            (0.5_f64, 0x3ff0000000000000),
        ];
        for (p, bits) in rows {
            assert_eq!(
                t_inv_2t_kernel(p, 1.0).unwrap().to_bits(),
                bits,
                "T.INV.2T({p},1)"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn t_inv_df1_matches_two_tail_double_prob() {
        for p in [0.6, 0.75, 0.9, 0.95, 0.99, 0.995] {
            let one = t_inv_kernel(p, 1.0).unwrap();
            let two = t_inv_2t_kernel(2.0 * (1.0 - p), 1.0).unwrap();
            assert_eq!(one.to_bits(), two.to_bits(), "T.INV({p},1) vs 2T");
            let neg = t_inv_kernel(1.0 - p, 1.0).unwrap();
            assert_eq!(one.to_bits(), (-neg).to_bits(), "T.INV oddness p={p}");
        }
        // Live Excel 16.0 b20326 Value2 T.INV(p,1)
        let pins: [(f64, u64); 8] = [
            (0.55, 0x3fc445f0fbb1cf9c),
            (0.6, 0x3fd4cb7bfb4961b0),
            (0.75, 0x3ff0000000000000),
            (0.9, 0x40089f188bdcd7b1),
            (0.95, 0x4019414813ba6625),
            (0.975, 0x40296993aacc4d1e),
            (0.99, 0x403fd20d55634e26),
            (0.995, 0x404fd410182c3c30),
        ];
        for (p, bits) in pins {
            assert_eq!(
                t_inv_kernel(p, 1.0).unwrap().to_bits(),
                bits,
                "T.INV({p},1)"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chisq_family_matches_known_rows() {
        assert!((chisq_dist_kernel(0.5, 1.0, true).unwrap() - 0.520_499_88).abs() < 1e-8);
        assert!(
            (chisq_dist_kernel(2.0, 3.0, false).unwrap() - 0.207_553_748_710_297).abs() < 1e-10
        );
        assert!((chisq_dist_rt_kernel(18.307, 10.0).unwrap() - 0.050_000_589_130_671).abs() < 1e-8);
        assert!((chisq_inv_rt_kernel(0.0500006, 10.0).unwrap() - 18.307).abs() < 1e-4);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn f_family_matches_known_rows() {
        assert!((f_dist_kernel(15.2069, 6.0, 4.0, true).unwrap() - 0.99).abs() < 5e-5);
        assert!((f_dist_kernel(15.2069, 6.0, 4.0, false).unwrap() - 0.001_223_8).abs() < 5e-7);
        assert!((f_inv_rt_kernel(0.01, 6.0, 4.0).unwrap() - 15.20686).abs() < 1e-4);
        assert!((f_inv_kernel(0.99, 6.0, 4.0).unwrap() - 15.20686).abs() < 1e-4);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn t_dist_df2_pdf_matches_live_excel_pow_chain_pins() {
        let pins = [
            (-8.0_f64, 0x3f5e8e78c21b35f0u64),
            (-4.0, 0x3f8ad1536fff177d),
            (-2.0, 0x3fb16b28f55d72d3),
            (-1.0, 0x3fc8a2345cc04426),
            (-0.5, 0x3fd2f684bda12f68),
            (0.0, 0x3fd6a09e667f3bcd),
            (0.5, 0x3fd2f684bda12f68),
            (1.0, 0x3fc8a2345cc04426),
            (2.0, 0x3fb16b28f55d72d3),
            (4.0, 0x3f8ad1536fff177d),
            (8.0, 0x3f5e8e78c21b35f0),
            (0.1, 0x3fd67571ae3ae506),
            (3.0, 0x3f9c11662a4daec1),
            (10.0, 0x3f4fcf18ff68ff16),
            (0.25, 0x3fd59b52a2911afa),
        ];
        for (x, bits) in pins {
            assert_eq!(
                t_dist_kernel(x, 2.0, false).unwrap().to_bits(),
                bits,
                "x={x}"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn t_dist_df1_pdf_matches_live_excel_pins() {
        assert_eq!(
            t_dist_kernel(0.0, 1.0, false).unwrap().to_bits(),
            0x3fd45f306dc9c883
        );
        assert_eq!(
            t_dist_kernel(1.0, 1.0, false).unwrap().to_bits(),
            0x3fc45f306dc9c883
        );
        assert_eq!(
            t_dist_kernel(0.5, 1.0, false).unwrap().to_bits(),
            0x3fd04c26be3b06cf
        );
        assert_eq!(
            t_dist_kernel(-2.0, 1.0, false).unwrap().to_bits(),
            0x3fb04c26be3b06cf
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn t_family_matches_known_rows() {
        assert!((t_dist_kernel(60.0, 1.0, true).unwrap() - 0.994_695_33).abs() < 1e-8);
        assert!((t_dist_kernel(8.0, 3.0, false).unwrap() - 0.000_736_91).abs() < 1e-8);
        assert!((t_dist_2t_kernel(1.959_999_998, 60.0).unwrap() - 0.054_644_93).abs() < 1e-7);
        assert!((t_dist_rt_kernel(1.959_999_998, 60.0).unwrap() - 0.027_322_465).abs() < 1e-8);
        assert!((t_inv_2t_kernel(0.054_644_93, 60.0).unwrap() - 1.959_999_998).abs() < 1e-6);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn compatibility_aliases_follow_modern_functions() {
        assert!((chisq_dist_rt_kernel(18.307, 10.0).unwrap() - 0.050_000_589_130_671).abs() < 1e-8);
        assert!((f_dist_rt_kernel(15.2069, 6.0, 4.0).unwrap() - 0.01).abs() < 5e-5);
        assert!((t_dist_2t_kernel(1.959_999_998, 60.0).unwrap() - 0.054_644_93).abs() < 1e-7);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn invalid_domains_return_num_errors() {
        assert_eq!(
            chisq_dist_kernel(-1.0, 1.0, true),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(
            f_dist_kernel(-1.0, 1.0, 1.0, true),
            Err(WorksheetErrorCode::Num)
        );
        assert_eq!(t_dist_2t_kernel(-1.0, 10.0), Err(WorksheetErrorCode::Num));
        assert_eq!(t_dist_rt_kernel(-1.0, 10.0), Err(WorksheetErrorCode::Num));
    }

    // BUG-FUNC-039 item 1: x=0 divergence published as #NUM!, not +inf.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn chisq_pdf_x0_df1_returns_num_not_inf() {
        assert_eq!(chisq_pdf_kernel(0.0, 1.0), Err(WorksheetErrorCode::Num));
        // df=2 at x=0 is finite (0.5); must not be broken by the fix.
        assert!((chisq_pdf_kernel(0.0, 2.0).unwrap() - 0.5).abs() < 1e-15);
        // df=3 at x=0 is 0.0.
        assert_eq!(chisq_pdf_kernel(0.0, 3.0), Ok(0.0));
    }

    // BUG-FUNC-039 item 1: F.DIST(0, d1<2, d2, FALSE) -> #NUM!.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn f_pdf_x0_d1lt2_returns_num_not_inf() {
        assert_eq!(f_pdf_kernel(0.0, 1.0, 5.0), Err(WorksheetErrorCode::Num));
        // d1=2 at x=0 is finite (1.0); must not be broken.
        assert!((f_pdf_kernel(0.0, 2.0, 5.0).unwrap() - 1.0).abs() < 1e-15);
        // d1=3 at x=0 is 0.0.
        assert_eq!(f_pdf_kernel(0.0, 3.0, 5.0), Ok(0.0));
    }

    // BUG-FUNC-039 item 2: df exactly 1e10 must be admitted; df > 1e10 rejected.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn df_boundary_at_1e10_is_admitted() {
        // df = 1e10 exactly: should succeed (just needs to parse without error).
        assert!(chisq_dist_kernel(1.0, 1e10, false).is_ok());
        // df = 1e10 + 1 (next representable float above 1e10): must be rejected.
        let above = 1e10 + 1.0;
        assert_eq!(
            chisq_dist_kernel(1.0, above, false),
            Err(WorksheetErrorCode::Num)
        );
        // F family too.
        assert!(f_dist_kernel(1.0, 1e10, 1e10, false).is_ok());
        assert_eq!(
            f_dist_kernel(1.0, above, 1e10, false),
            Err(WorksheetErrorCode::Num)
        );
    }
}
