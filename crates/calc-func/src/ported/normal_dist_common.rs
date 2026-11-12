use crate::value::WorksheetErrorCode;
pub const SQRT_2PI: f64 = 2.506_628_274_631_000_7;

/// `RN(1/sqrt(2π))` — PHI multiplies by this reciprocal constant (the
/// divide-by-`SQRT_2PI` staging is ruled out; live PHI(0) publishes exactly
/// these bits).
pub(crate) const INV_SQRT_2PI: f64 = f64::from_bits(0x3fd9884533d43651);

/// Worksheet `PHI` numerical graph observed on the reference Excel host.
/// W111 retains a fresh 9,130-row exact numerical replay; surface overflow
/// admission and contextual coercion remain separate obligations.
///
/// ```text
/// sq  = RN53(RN64(x·x))            (x87 double-rounded square)
/// t   = -(sq/2)                    (exact halving + negation)
/// e   = EXP(t)                     (the x87 exponential, `excel_exp`)
/// v   = RN53(RN64(e · RN(1/sqrt(2π))))
/// PHI = +0.0 if |v| < DBL_MIN      (live-pinned publication flush;
///        normal values publish unchanged — boundary probed at bit level)
/// ```
pub fn phi_kernel(x: f64) -> f64 {
    use crate::ported::excel_numeric::{excel_exp, excel_x87_mul};
    let sq = excel_x87_mul(x, x);
    let e = excel_exp(-(sq / 2.0));
    let v = excel_x87_mul(e, INV_SQRT_2PI);
    if v.abs() < f64::MIN_POSITIVE { 0.0 } else { v }
}

/// `N(t)` of Excel's erf rational `x * N(t) / D(t)`, `t = x*x`, lowest power first. Measured from
/// Excel's answers (see [`erf_approx`]); the published table is not identified. Orders 0 and 1 are
/// exact. Orders 2 and above were read from runs of 20,000 consecutive doubles at a dozen sites,
/// so the last bits of order 3 and above can be off, which shows only above 0.25.
const ERF_N: [u64; 5] = [
    0x3ff1_df5e_70c7_956b,
    0x3fc2_01d8_627a_7d2c,
    0x3fa6_f289_d165_b303,
    0x3f5d_93a5_813e_372c,
    0x3f29_3276_7df2_64b2,
];
/// `D(t)`, lowest power first, measured as [`ERF_N`]. The degrees (4, 5) are measured: a fifth
/// term of `N` and a sixth of `D` fit to zero.
const ERF_D: [u64; 6] = [
    0x3fef_ada1_5778_ece6,
    0x3fdd_191a_ca8d_2499,
    0x3fb7_9f85_ba68_803e,
    0x3f85_6de5_da86_af12,
    0x3f46_0975_1b5b_6488,
    0x3ef4_f20a_473a_5ab0,
];

/// One Horner evaluation with every product and sum stored to binary64, as `erf_approx` needs.
fn stored_horner(coefficients: &[u64], t: f64) -> f64 {
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_mul};
    let mut value = 0.0;
    for (i, &bits) in coefficients.iter().enumerate().rev() {
        let c = f64::from_bits(bits);
        value = if i + 1 == coefficients.len() {
            c
        } else {
            excel_x87_add(excel_x87_mul(value, t), c)
        };
    }
    value
}

/// The shared ERF body (ERF, ERF.PRECISE, the below-half ERFC complement and GAUSS's tiny
/// route).
///
/// For `|x| < 0.5` Excel evaluates a (4, 5) rational `x * N(t) / D(t)` as compiled double
/// arithmetic on the x87 at precision 64: every operation is `RN53(RN64(..))`, that is, its
/// result is stored to binary64 before the next one reads it, in the order
///
/// ```text
/// t = x*x;  N = Horner(t);  D = Horner(t);  p = x*N;  y = p/D
/// ```
///
/// Excel shows this directly. Stored `N` and `D` step at edges found by bisection to one input
/// ulp, and the two `D` edges measured agree to 1e-15 only with the 64-bit register's half unit on
/// each. Of the store placements raced on 2,000 consecutive answers at each of about 20 sites up
/// to 0.5, all stores leaves 0.5% of the rows unexplained (the coefficients' last bits) and every
/// other placement more than twice that. The measured coefficients give `N1/N0 - D1/D0 = -1/3`
/// to 2e-16 and reproduce erf to a flat offset of 9e-17 up to 0.5, past which the error climbs at
/// once, which places the approximation's design interval.
///
/// Above 0.5 the magnitude is the stored-PC64 complement of the upper ERFC body. Independent
/// public calls agree on 3,328 adjacent inputs; exponent-edge controls distinguish the final
/// PC64 store from strict binary64 subtraction. Exactly 0.5 keeps its existing boundary body.
/// Subnormal arguments cannot be written to a cell (Excel reads them back as zero).
pub fn erf_approx(x: f64) -> f64 {
    if x.is_normal() && x.abs() < 0.5 {
        use crate::ported::excel_numeric::{excel_x87_div, excel_x87_mul};
        let t = excel_x87_mul(x, x);
        let numerator = stored_horner(&ERF_N, t);
        let denominator = stored_horner(&ERF_D, t);
        return excel_x87_div(excel_x87_mul(x, numerator), denominator);
    }
    if x.abs() > 0.5 {
        use crate::ported::excel_numeric::excel_x87_sub;
        return excel_x87_sub(1.0, erfc_upper(x.abs())).copysign(x);
    }
    libm::erf(x)
}

/// `N(x)` of Excel's `F = N(x) / D(x)` for `1/2 <= x <= 4`, lowest power first. See [`erfc_upper`].
const ERFC_MID_N: [u64; 10] = [
    0x3fef_ffff_ffff_ff9f,
    0x4000_223d_9f70_ed04,
    0x4000_4481_1191_d2cb,
    0x3ff4_db9f_cfb3_c6e7,
    0x3fe2_7fc8_b2d9_7507,
    0x3fc7_6480_f4d9_7fda,
    0x3fa5_0b66_7465_37ea,
    0x3f7a_0160_6645_9c2d,
    0x3f44_1cfb_b83e_d0e4,
    0x3efd_ecdb_b07d_9e2f,
];
/// `D(x)` of the same rational, lowest power first. The constant term is 1.
const ERFC_MID_D: [u64; 11] = [
    0x3ff0_0000_0000_0000,
    0x4009_2929_4792_38db,
    0x4012_5449_c578_e4bd,
    0x4010_5350_002d_8ffa,
    0x4003_bc73_0d3c_1c66,
    0x3ff0_f7f2_7e5d_ccd7,
    0x3fd5_16f2_9541_7ca8,
    0x3fb2_ca1b_b9df_5335,
    0x3f87_1934_f4e7_b285,
    0x3f51_d32a_e54a_52b7,
    0x3f0a_8542_411a_723b,
];

/// `N(x)` of Excel's `F = N(x) / D(x)` for `x > 4`, lowest power first. See [`erfc_upper`].
const ERFC_TAIL_N: [u64; 9] = [
    0x3ff0_00ae_bf22_de1a,
    0x4003_da26_6cd0_39c2,
    0x4008_0fb2_b571_6d18,
    0x4002_6c40_9d38_4171,
    0x3ff3_96f0_fc8e_5c9c,
    0x3fdd_936f_5138_efd9,
    0x3fc0_191f_8363_4b97,
    0x3f96_ca0f_8ecf_c798,
    0x3f64_dc75_6d76_1350,
];
/// `D(x)`, lowest power first, measured as [`ERFC_TAIL_N`]. Excel's constant term reads as 1.
const ERFC_TAIL_D: [u64; 10] = [
    0x3ff0_0000_0000_0000,
    0x400c_e328_ffeb_7964,
    0x4018_50e3_f0a8_dba3,
    0x4019_3da0_f9da_ba61,
    0x4011_e2d9_55b8_fd9e,
    0x4002_3c05_0f5f_c4a3,
    0x3fea_d794_d2e2_c325,
    0x3fcc_d273_9010_3553,
    0x3fa4_324d_3ef0_f238,
    0x3f72_7cdb_b941_913d,
];

/// The upper branch of Excel's erfc for `x >= 0.5`: `Q = E * F`.
///
/// `E` is measured. `z = RN53(RN64(x*x))` and `E = exp(-z)` are stored to binary64 before use: over
/// 220,000 Excel answers up to `x = 26.5` the residual of `Q` against this `E` times the true
/// `exp(x*x)*erfc(x)` stays within about 10 units of 2^-53, where the exact `exp(-x*x)` spreads by
/// hundreds of units. `z`'s double rounding shows directly: plain binary64 `x*x` leaves outliers of
/// up to 1,024 units.
///
/// `F` has two branches, both relative-error minimax rationals of `exp(x*x)*erfc(x)` in `x`, with
/// `D(0) = 1`, Horner, each product, sum and the final quotient stored to binary64 from precision
/// 64, and `Q` the stored product `E * F`. `x = 4` itself falls below: bisected on consecutive
/// doubles, Excel leaves the table below at the next double up.
///
/// - `1/2 <= x <= 4`: degrees (9, 10), on exactly that interval.
/// - `x > 4`: degrees (8, 9), on `[4, 26.54]`. A fresh solve there reproduces the table read from
///   Excel to 1e-12. 26.54 is Cody's XBIG (26.543) to four digits.
///
/// The degrees and intervals are identified by theory. Of 45 solves of (m, m + 1) on candidate
/// intervals, the binade crossings of the Horner stages of (9, 10) on `[1/2, 4]` alone land on the
/// 30 bias steps of Excel's answers: 27 within 0.002, where the next best has 10. Its predicted stage
/// lines then appear in runs of adjacent doubles at the predicted frequency and amplitude, line by
/// line.
///
/// The solve fixes the leading digits only. The two weakest singular directions of the coefficients
/// differ from it by 4e-8 and 2e-10 relative (the error curve moves by 1e-28), so the printed table
/// came from a less converged solve. Those directions were read from the phases of the stage lines
/// over 140 windows. The low bits were not derived: a search of chained moves over the coefficients,
/// on the squared last-bit error of 1,076,000 Excel answers, fixed them. That the search ended on
/// the table rather than on a fit is shown by answers it did not see, not by its count.
/// R35's adjacent-load probe leaves original tail literals undetermined: over
/// `4 < x <= 26.5`, both neighboring loads of N0--N2, D1--D2 and D4 are absorbed at
/// their first stored sum. N3 and D3 absorb the upward neighbor only;163 fresh
/// separating questions reject their downward neighbors and the other tested
/// changes. This identifies equivalent loads, not unique historical literals.
pub fn erfc_upper(x: f64) -> f64 {
    use crate::ported::excel_numeric::{excel_exp, excel_x87_mul};
    let z = excel_x87_mul(x, x);
    let e = excel_exp(-z);
    if e == 0.0 {
        // Past about 27 the exponential is gone and the polynomials below can overflow.
        return 0.0;
    }
    excel_x87_mul(e, erfc_scaled(x))
}

/// `exp(x*x)*erfc(x)` for `x >= 1/2`: the stored `F` of [`erfc_upper`] alone, with no exponential.
/// NSWC's erfc1 with `ind = 1`, which Morris's Temme expansion calls.
pub fn erfc_scaled(x: f64) -> f64 {
    use crate::ported::excel_numeric::excel_x87_div;
    let (n, d): (&[u64], &[u64]) = if x > 4.0 {
        (&ERFC_TAIL_N, &ERFC_TAIL_D)
    } else {
        (&ERFC_MID_N, &ERFC_MID_D)
    };
    excel_x87_div(stored_horner(n, x), stored_horner(d, x))
}

/// Binary64 reciprocal-square-root constant used by the normal wrappers.
pub const FRAC_1_SQRT_2_BITS: u64 = 0x3fe6a09e667f3bcd;

/// Inclusive tiny-direct GAUSS predicate: `abs(x) <= 1e-15` is the odd
/// stored-z route; the successor double is the ordinary G-F3 wrapper.
pub const GAUSS_TINY_MAX_BITS: u64 = 0x3cd203af9ee75616;

/// Native-multiply stored-z route for the distinct tiny GAUSS branch.
/// W111 independently distinguishes the ordinary normal-CDF wrapper's staged
/// RN64 multiply followed by a binary64 store; that wrapper does not call this.
pub fn stored_normal_z(x: f64) -> f64 {
    x.abs() * f64::from_bits(FRAC_1_SQRT_2_BITS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// W109 PHI identification pins — live Excel 16.0 build 20131 (unique
    /// surviving staging; 764/764 answered rows replay via
    /// `verify_phi_promotion`).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn phi_matches_live_excel_pinned_witnesses() {
        // PHI(0) publishes the reciprocal constant bits exactly.
        assert_eq!(phi_kernel(0.0).to_bits(), 0x3fd9884533d43651);
        // Subnormal-flush boundary (bit-pinned): x=37.6157 still publishes a
        // normal value; the next band flushes to +0.
        let x_norm = 37.6157f64;
        assert!(phi_kernel(x_norm) >= f64::MIN_POSITIVE);
        assert_eq!(phi_kernel(37.8511319f64), 0.0);
        assert_eq!(phi_kernel(39.0), 0.0);
    }

    /// Recorded Excel `ERF.PRECISE` answers below 0.5 (`OxFunc-Evidence`: the build-20131
    /// oracle cache, `readings/excel-20527/ERF.PRECISE.json`, W109 banks b16, b17 and
    /// b26-zcross). Each pair is `(argument bits, Excel result bits)`. The first group is
    /// mismatched by `libm::erf`; the second already matched and must stay matched.
    const ERF_TINY_RECORDED: [(u64, u64); 33] = [
        // libm::erf misses these by one unit in the last place.
        (0x3d70000000000000, 0x3d720dd750429b6e),
        (0x3d7cfde8d4cc4149, 0x3d805b5d30f4cc06),
        (0x3d7ab28000000000, 0x3d7e1fe84c83a3a8),
        (0x3d735c41b35a3054, 0x3d75d88967f7ead4),
        (0xbd768d3d87b932e7, 0xbd79726775b984dd),
        (0x3d966c42d2efd513, 0x3d994d30e2d5cb8e),
        (0xbdb06772d52c3fe8, 0xbdb28291fd7f949a),
        (0x3ddba00000000000, 0x3ddf2be5c093005b),
        (0x3df9e0d03f41c4b8, 0x3dfd334d2f1b80bd),
        (0x3e13bb4000000000, 0x3e1643b9ab266415),
        (0x3e1fee0a3d70a456, 0x3e2203b54e22aea5),
        (0x3e20000000000000, 0x3e220dd750429b6e),
        (0xbe223ab45be40eee, 0xbe2491cfea3435ea),
        (0x3e30000000000001, 0x3e320dd750429b6f),
        (0x3e80000000000000, 0x3e820dd750429b56),
        (0x3f30000000000000, 0x3f320dd74a3dfe55),
        (0x3f90000000000000, 0x3f920d77083f17a1),
        (0x3fb0000000000000, 0x3fb207d480e90659),
        (0x3fd0000000000000, 0x3fd1af54e232d60a),
        (0x3ee0000000000000, 0x3ee20dd750411a47),
        // libm::erf already matches these.
        (0x3d7024d3d9e093a8, 0x3d7237658117c44d),
        (0x3d7d758000000000, 0x3d809ed5f357116e),
        (0xbd7a3e0fafbe3c36, 0xbd7d9c85380b2536),
        (0x3d92ac4fd88c57a8, 0x3d9512011d9a0deb),
        (0x3ddde00000000000, 0x3de0daec03ee2f1b),
        (0xbdf3f40396a8a23e, 0xbdf683c6cf6f7a58),
        (0x3e14a4f4d4a539bc, 0x3e174b6f464f49c3),
        (0x3e1fec1eb851eba4, 0x3e22029ffea81b13),
        (0x3e2c000000000000, 0x3e2f9838cc748fff),
        (0x3e20079999999c92, 0x3e22166ab0225804),
        (0xbe2f490522e27e6c, 0xbe31a69b0ec7c0e8),
        (0x3e30d00000000000, 0x3e32f88b3f55fd52),
        (0x3e70000000000000, 0x3e720dd750429b67),
    ];

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn erf_rational_reproduces_recorded_excel_answers() {
        for (x, excel) in ERF_TINY_RECORDED {
            let actual = erf_approx(f64::from_bits(x)).to_bits();
            assert_eq!(
                actual, excel,
                "ERF.PRECISE({x:#018x}): calculated {actual:#018x}, Excel {excel:#018x}"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn erf_measured_coefficients_reproduce_the_taylor_series() {
        // N(0)/D(0) = 2/sqrt(pi) and N'(0)/N(0) - D'(0)/D(0) = -1/3: erf(x)/x's first two Taylor
        // coefficients, which the measurements did not impose.
        let (n0, n1) = (f64::from_bits(ERF_N[0]), f64::from_bits(ERF_N[1]));
        let (d0, d1) = (f64::from_bits(ERF_D[0]), f64::from_bits(ERF_D[1]));
        assert!((n0 / d0 - f64::from_bits(0x3ff2_0dd7_5042_9b6d)).abs() <= 2.0 * f64::EPSILON);
        assert!((n1 / n0 - d1 / d0 + 1.0 / 3.0).abs() < 1e-15);
    }

    /// Fresh `ERFC.PRECISE` answers above 4 from the question service (Excel 16.0 build 20527),
    /// held out of the coefficient reading: `(x bits, answer bits)`.
    const ERFC_TAIL_RECORDED: [(u64, u64); 26] = [
        (0x40108402930e0cfe, 0x3e36883c63d60e12),
        (0x4010924024151932, 0x3e3405114de80265),
        (0x40109689765f9dbe, 0x3e3351885f627dea),
        (0x40109b5244840768, 0x3e32905d9da06fd3),
        (0x401186561022949a, 0x3e03e87552471f61),
        (0x40130d149d4ecfae, 0x3db1f3dcd6d899b8),
        (0x4013c4151072bbda, 0x3d8878903b4994af),
        (0x401446853c9d0d10, 0x3d6aaf8a69f2ac11),
        (0x4016356dbfcf2913, 0x3cf2721271ba052a),
        (0x40166a9ff4f7543d, 0x3ce47a7e0aedf3ad),
        (0x4018aa82f38f4935, 0x3c497c5e2c05b719),
        (0x40198718c8297c6b, 0x3c0a6b29a16ded6b),
        (0x401da0c7181f6595, 0x3ac165ae716256a6),
        (0x401db87f8a2a5126, 0x3ab89940bd6ce926),
        (0x4020238c4208b7c3, 0x39d27df17f4bab41),
        (0x40231df84d856657, 0x37711cd2af8384c9),
        (0x4025ec25ae3d2b6b, 0x34d4c4fe29eb9037),
        (0x40269e22261ede8d, 0x3421e922c4e8d883),
        (0x4028ca2b00fe21c8, 0x31cdb688819cd6c8),
        (0x402949dc0ee700f7, 0x313cf20e9523ae25),
        (0x40317115db752ada, 0x2431dd29a749111d),
        (0x4032dcb45204c473, 0x1f8928daaf4decc9),
        (0x4035016c45b0d013, 0x17d290d09ece5706),
        (0x403516513fb9a44b, 0x17830f7d38c4726d),
        (0x4035a7d11800dd41, 0x1551f599691079f5),
        (0x403a6cdfea446fae, 0x00a03c758075bc76),
    ];

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn erfc_tail_reproduces_recorded_excel_answers() {
        for (x, excel) in ERFC_TAIL_RECORDED {
            let actual = erfc_upper(f64::from_bits(x)).to_bits();
            assert_eq!(
                actual, excel,
                "ERFC.PRECISE({x:#018x}): calculated {actual:#018x}, Excel {excel:#018x}"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn erfc_tail_has_no_overflow_past_the_exponential() {
        assert_eq!(erfc_upper(1e300), 0.0);
        assert_eq!(erfc_upper(f64::MAX), 0.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn phi_matches_excel_probe_lanes() {
        assert!((phi_kernel(0.0) - 0.398942280401433).abs() < 1e-12);
        assert!((phi_kernel(1.0) - 0.241970724519143).abs() < 1e-12);
    }
}
