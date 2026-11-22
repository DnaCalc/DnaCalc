use crate::ported::excel_numeric::crt_log;
use crate::ported::excel_numeric::crt_pow;
use crate::ported::excel_numeric::crt_sin;
use crate::value::WorksheetErrorCode;
pub fn ln_gamma(z: f64) -> f64 {
    const COEFFS: [f64; 9] = [
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
    const G: f64 = 7.0;

    if z < 0.5 {
        return crt_log(std::f64::consts::PI)
            - crt_log(crt_sin(std::f64::consts::PI * z))
            - ln_gamma(1.0 - z);
    }

    let x = COEFFS
        .iter()
        .enumerate()
        .skip(1)
        .fold(COEFFS[0], |acc, (i, coeff)| {
            acc + coeff / (z - 1.0 + i as f64)
        });
    let t = z - 1.0 + G + 0.5;
    0.5 * crt_log(2.0 * std::f64::consts::PI) + (z - 0.5) * crt_log(t) - t + crt_log(x)
}

pub fn gamma(z: f64) -> f64 {
    if z.is_sign_negative() && z.fract() == 0.0 {
        return f64::NAN;
    }
    if z < 0.5 {
        return std::f64::consts::PI / (crt_sin(std::f64::consts::PI * z) * gamma(1.0 - z));
    }
    crate::ported::excel_numeric::excel_exp(ln_gamma(z))
}

// ---------------------------------------------------------------------------
// SOURCE: A. R. DiDonato and A. H. Morris Jr., "Computation of the incomplete gamma function
// ratios and their inverse", ACM Transactions on Mathematical Software 12(4), 1986, Algorithm
// 654 (gratio, NSWC library), as distributed in scipy's cdflib (scipy v0.14, DCDFLIB by Brown,
// Lovato and Russell). Translated from that Fortran into Rust; kept (owner, 2026-10-30).
//
// GRATIO: faithful port of the DCDFLIB / NSWC incomplete-gamma-ratio kernel
// (TOMS 654, DiDonato & Morris; Alfred H. Morris Jr.), transcribed from the
// scipy v0.14 cdflib Fortran sources via the validated Python reference at
// smart-fuzzer/work/w109/G3-01-dist/cdflib_py.py.
//
// Live-Excel identification (W109) proved this is Excel's actual algorithm for
// the incomplete-gamma family (CHIDIST / GAMMA.DIST / GAMMADIST / CHISQ.*),
// Most of its body stores binary64 arithmetic; measured final-series and Temme
// corrections use the shared x87 arithmetic. This kernel replaces the
// previous Newton-Raphson-style series/continued-fraction pair, collapsing the
// catastrophic ULP drift (up to ~6224 ULP) those exhibited versus Excel.
//
// IDENTIFICATION-DRIVEN DEVIATIONS FROM VANILLA NSWC (do not "restore"):
//   1. The `a == 0.5` dispatch to erf/erfc (gratio.f statement 390) is OMITTED:
//      live-Excel proof shows Excel routes a = 0.5 through the general a < 1
//      branches (Excel's ERF.PRECISE *is* that path). So a = 0.5 flows into the
//      a < 1 code below.
//   2. The half-integer closed-form branch (statement 220) needs erfc1(0, sqrt(x)).
//      Excel's erfc IS gratio(0.5, z^2).q, so erfc1(0, sqrt(x)) == Q(0.5, x); we
//      implement it as a depth-1 recursive call `gratio(0.5, x).1`.
//   3. Consequently erf_nswc / erfc1 are not ported. The Temme a >= big branch's
//      single scaled-erfc need (erfc1(1, sqrt(y)) == exp(y)*Q(0.5, y)) is likewise
//      wired through the recursion, keeping the structure complete without an
//      erf implementation. R24's current corpus reaches the Temme branch; its
//      final correction retains PC64 arithmetic through the final product.
// ---------------------------------------------------------------------------

// 1 / sqrt(pi) to binary64 for the half-integer finite sum (cb7 / dc-j9k.63).
const GRATIO_INV_RTPI: f64 = f64::from_bits(0x3fe2_0dd7_5042_9b6d);
// 1 / sqrt(2*pi) to binary64 for the Temme correction term.
const TEMME_RT2PIN: f64 = f64::from_bits(0x3fd9_8845_33d4_3651);
const GRATIO_E: f64 = 2.220446049250313e-16; // spmpar(1) = 2^-52
const GRATIO_SPMPAR3: f64 = 1.7976931348623157e308;

// Full binary64 exact rational Temme expansion coefficients c_k(eta) (DLMF 8.12 / Temme 1979):
// c_0(eta) = 1/(l - 1) - 1/eta, eta * c_k(eta) = c_{k-1}'(eta) + gamma_k * (1 + eta * c_0(eta)).
// Order k = 0..10 and degrees 16 down to 1 cover |1 - x/a| < 0.3, a > 20 to full binary64 precision.
const TEMME_C0: [f64; 17] = [
    -3.333_333_333_333_333_15e-01,
    8.333_333_333_333_332_87e-02,
    -1.481_481_481_481_481_54e-02,
    1.157_407_407_407_407_34e-03,
    3.527_336_860_670_194_24e-04,
    -1.787_551_440_329_217_98e-04,
    3.919_263_178_522_437_67e-05,
    -2.185_448_510_679_991_98e-06,
    -1.854_062_210_715_159_97e-06,
    8.296_711_340_953_086_52e-07,
    -1.766_595_273_682_607_82e-07,
    6.707_853_543_401_498_41e-09,
    1.026_180_978_424_030_86e-08,
    -4.382_036_018_453_352_94e-09,
    9.147_699_582_236_790_21e-10,
    -2.551_419_399_494_624_82e-11,
    -5.830_772_132_550_425_61e-11,
];
const TEMME_C1: [f64; 16] = [
    -1.851_851_851_851_851_92e-03,
    -3.472_222_222_222_222_03e-03,
    2.645_502_645_502_645_36e-03,
    -9.902_263_374_485_596_30e-04,
    2.057_613_168_724_279_79e-04,
    -4.018_775_720_164_608_97e-07,
    -1.809_855_033_448_997_67e-05,
    7.649_160_916_081_109_82e-06,
    -1.612_090_089_456_344_65e-06,
    4.647_127_802_807_434_02e-09,
    1.378_633_446_915_720_92e-07,
    -5.752_545_603_517_704_71e-08,
    1.195_162_859_977_814_77e-08,
    -1.754_324_171_974_764_67e-11,
    -1.009_154_371_060_041_26e-09,
    4.162_792_991_842_582_80e-10,
];
const TEMME_C2: [f64; 14] = [
    4.133_597_883_597_883_37e-03,
    -2.681_327_160_493_827_27e-03,
    7.716_049_382_716_048_95e-04,
    2.009_387_860_082_304_70e-06,
    -1.073_665_322_636_515_99e-04,
    5.292_344_882_912_012_50e-05,
    -1.276_063_518_861_872_84e-05,
    3.423_578_734_096_137_81e-08,
    1.372_195_730_906_293_42e-06,
    -6.298_992_138_380_054_82e-07,
    1.428_061_420_606_424_25e-07,
    -2.047_709_842_199_086_61e-10,
    -1.409_252_991_086_752_03e-08,
    6.228_974_084_922_021_84e-09,
];
const TEMME_C3: [f64; 12] = [
    6.494_341_563_786_007_73e-04,
    2.294_720_936_213_991_68e-04,
    -4.691_894_943_952_557_02e-04,
    2.677_206_320_628_388_54e-04,
    -7.561_801_671_883_976_62e-05,
    -2.396_505_113_867_296_80e-07,
    1.108_265_411_534_730_25e-05,
    -5.674_952_826_991_596_55e-06,
    1.423_090_073_243_588_33e-06,
    -2.786_108_029_152_814_34e-11,
    -1.695_840_409_193_027_82e-07,
    8.099_464_905_388_082_68e-08,
];
const TEMME_C4: [f64; 10] = [
    -8.618_882_909_167_117_26e-04,
    7.840_392_217_200_666_15e-04,
    -2.990_724_803_031_901_77e-04,
    -1.463_845_257_884_341_81e-06,
    6.641_498_215_465_121_89e-05,
    -3.968_365_047_179_434_71e-05,
    1.137_572_697_067_841_87e-05,
    2.507_497_226_237_532_94e-10,
    -1.695_414_953_655_830_54e-06,
    8.907_507_532_205_309_41e-07,
];
const TEMME_C5: [f64; 8] = [
    -3.367_985_533_663_581_31e-04,
    -6.972_813_758_365_857_11e-05,
    2.772_753_244_959_391_83e-04,
    -1.993_257_051_618_884_69e-04,
    6.797_780_477_937_208_00e-05,
    1.419_062_920_643_967_13e-07,
    -1.359_404_818_976_869_26e-05,
    8.018_470_256_334_202_00e-06,
];
const TEMME_C6: [f64; 7] = [
    5.313_079_364_639_922_49e-04,
    -5.921_664_373_536_939_32e-04,
    2.708_782_096_718_045_00e-04,
    7.902_353_232_660_328_15e-07,
    -8.153_969_367_561_969_15e-05,
    5.611_682_753_106_249_70e-05,
    -1.832_911_658_284_337_52e-05,
];
const TEMME_C7: [f64; 5] = [
    3.443_676_068_923_776_52e-04,
    5.171_790_908_260_591_87e-05,
    -3.349_316_108_114_223_38e-04,
    2.812_695_154_763_236_88e-04,
    -1.097_658_224_468_473_11e-04,
];
const TEMME_C8: [f64; 4] = [
    -6.526_239_185_953_093_72e-04,
    8.394_987_206_720_872_65e-04,
    -4.382_970_985_417_209_88e-04,
    -6.969_091_458_420_552_33e-07,
];
const TEMME_C9: [f64; 3] = [
    -5.967_612_901_927_462_58e-04,
    -7.204_895_416_020_010_87e-05,
    6.782_308_837_667_327_99e-04,
];
const TEMME_C10: [f64; 2] = [1.332_445_449_480_065_61e-03, -1.914_438_498_565_477_61e-03];

const TEMME_TABLES: [&[f64]; 11] = [
    &TEMME_C0, &TEMME_C1, &TEMME_C2, &TEMME_C3, &TEMME_C4, &TEMME_C5, &TEMME_C6, &TEMME_C7,
    &TEMME_C8, &TEMME_C9, &TEMME_C10,
];

// gratio.f iop-selected accuracy vector (used by the a < 1 leaf); ind = 0 -> iop = 1 (acc = 5e-15).
const GRATIO_ACC0: [f64; 3] = [5e-15, 5e-7, 5e-4];

// RN53(1/k!), k = 0..19. R35: each adjacent coefficient load is separated
// by an original Excel question, conditional on the other loads and EXP.
const GRATIO_RECIP_FACTORIAL: [u64; 20] = [
    0x3ff0000000000000, 0x3ff0000000000000, 0x3fe0000000000000,
    0x3fc5555555555555, 0x3fa5555555555555, 0x3f81111111111111,
    0x3f56c16c16c16c17, 0x3f2a01a01a01a01a, 0x3efa01a01a01a01a,
    0x3ec71de3a556c734, 0x3e927e4fb7789f5c, 0x3e5ae64567f544e4,
    0x3e21eed8eff8d898, 0x3de6124613a86d09, 0x3da93974a8c07c9d,
    0x3d6ae7f3e733b81f, 0x3d2ae7f3e733b81f, 0x3ce952c77030ad4a,
    0x3ca6827863b97d97, 0x3c62f49b46814157,
];

// RN53(2^(k+1) / (2k+1)!!), k = 0..19, for half-integer shapes a = ii + 0.5 (1 <= ii <= 19)
// in the unified finite-sum domain a < x <= 30.0 (dc-j9k.63).
const GRATIO_HALF_RECIP: [u64; 20] = [
    0x4000_0000_0000_0000, 0x3ff5_5555_5555_5555, 0x3fe1_1111_1111_1111,
    0x3fc3_8138_1381_3814, 0x3fa1_566a_bc01_1567, 0x3f79_37e1_1175_f095,
    0x3f4f_09b2_8ba4_d955, 0x3f20_8db4_8ebe_51c7, 0x3eef_28db_670b_e53b,
    0x3eba_3d5a_71b9_2cd3, 0x3e83_fdfb_c45c_52ea, 0x3e4b_d0ac_3296_b624,
    0x3e11_cd3b_01a8_22a6, 0x3dd5_1929_7d39_0c9f, 0x3d97_47e7_2912_d023,
    0x3d58_0828_6c76_8445, 0x3d17_4dba_97b8_be52, 0x3cd5_4e61_74c6_2a5a,
    0x3c92_6d4d_5e13_2b8c, 0x3c4e_3d36_c1c3_8faa,
];

fn gratio_integer_sum(a: usize, x: f64) -> f64 {
    use crate::ported::excel_numeric::{excel_exp, excel_x87_add, excel_x87_mul};
    let mut polynomial = f64::from_bits(GRATIO_RECIP_FACTORIAL[a - 1]);
    for &coefficient in GRATIO_RECIP_FACTORIAL[..a - 1].iter().rev() {
        polynomial = excel_x87_add(excel_x87_mul(polynomial, x), f64::from_bits(coefficient));
    }
    excel_x87_mul(excel_exp(-x), polynomial)
}

/// Half-integer finite sum for `a = ii + 0.5` (`1 <= ii <= 19`, `a < x <= 30.0`, Excel 16.0.20527).
/// Stored x87 Horner over `GRATIO_HALF_RECIP[0..ii]` scaled by `exp(-x - 0.5*ln(x)) * (1/sqrt(pi)) * x`,
/// added to `ERFC.PRECISE(sqrt(x))` (65/65 0-ULP on cb7 frozen holdout, 60/60 0-ULP on nx6 holdout).
fn gratio_half_integer_sum(ii: usize, x: f64) -> f64 {
    use crate::ported::excel_numeric::{
        excel_exp, excel_log, excel_x87_add, excel_x87_mul, excel_x87_sub,
    };
    let e0 = crate::ported::special_dist_family::erfc_precise_kernel(x.sqrt())
        .unwrap_or_else(|_| gratio(0.5, x).1);
    let ex = excel_exp(excel_x87_sub(-x, excel_x87_mul(0.5, excel_log(x))));
    let t = excel_x87_mul(ex, GRATIO_INV_RTPI);
    let mut polynomial = f64::from_bits(GRATIO_HALF_RECIP[ii - 1]);
    for &coefficient in GRATIO_HALF_RECIP[..ii - 1].iter().rev() {
        polynomial = excel_x87_add(excel_x87_mul(polynomial, x), f64::from_bits(coefficient));
    }
    excel_x87_add(e0, excel_x87_mul(t, excel_x87_mul(x, polynomial)))
}

/// CR-quality normalizer Gamma(a) for the gratio prefactor. Identification
/// (W109 agent-B normalizer sweep): Excel divides exp(t1) by a value equal to
/// the correctly-rounded double Gamma(a) to within 1 ULP (= exp of its
/// internal lgamma), decisively NOT the NSWC gamma routine (+22..+33 ULP).
/// Integer a: exact factorial. Half-integer a: exact odd double-factorial
/// times sqrt(pi-double), one rounding. Generic a: exp(ln_gamma) fallback.
fn gratio_norm_gamma(a: f64) -> f64 {
    if a >= 1.0 && a <= 22.0 && a == a.floor() {
        let mut f = 1.0f64;
        let mut k = 2.0f64;
        while k < a {
            f *= k;
            k += 1.0;
        }
        return f;
    }
    let n_half = a - 0.5;
    if a >= 0.5 && a <= 22.0 && n_half == n_half.floor() {
        let n = n_half as i32;
        let mut df = 1.0f64; // (2n-1)!! exact below 2^53 for n <= 21
        let mut m = 1.0f64;
        for _ in 0..n {
            df *= m;
            m += 2.0;
        }
        let sqrt_pi = std::f64::consts::PI.sqrt();
        return df * sqrt_pi / f64::powi(2.0, n);
    }
    crate::ported::excel_numeric::gamma_excel(a)
}

/// exparg(l) of exparg.f (specialized to this x86_64 host's double range).
fn gratio_exparg(l: i32) -> f64 {
    let lnb = 0.69314718055995;
    if l != 0 {
        let m = -1021 - 1;
        0.99999 * (m as f64 * lnb)
    } else {
        let m = 1024;
        0.99999 * (m as f64 * lnb)
    }
}

/// gam1.f: computation of 1/gamma(a+1) - 1 for -0.5 <= a <= 1.5, with stored PC64
/// `excel_x87_*` Horner and rational ops (cb11 / dc-j9k.67).
fn gratio_gam1(a: f64) -> f64 {
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_sub};
    const P: [f64; 7] = [
        0.577215664901533e+00,
        -0.409078193005776e+00,
        -0.230975380857675e+00,
        0.597275330452234e-01,
        0.766968181649490e-02,
        -0.514889771323592e-02,
        0.589597428611429e-03,
    ];
    const Q: [f64; 5] = [
        0.100000000000000e+01,
        0.427569613095214e+00,
        0.158451672430138e+00,
        0.261132021441447e-01,
        0.423244297896961e-02,
    ];
    const R: [f64; 9] = [
        -0.422784335098468e+00,
        -0.771330383816272e+00,
        -0.244757765222226e+00,
        0.118378989872749e+00,
        0.930357293360349e-03,
        -0.118290993445146e-01,
        0.223047661158249e-02,
        0.266505979058923e-03,
        -0.132674909766242e-03,
    ];
    const S1: f64 = 0.273076135303957e+00;
    const S2: f64 = 0.559398236957378e-01;
    let mut t = a;
    let d = excel_x87_sub(a, 0.5);
    if d > 0.0 {
        t = excel_x87_sub(d, 0.5);
    }
    if t == 0.0 {
        return 0.0;
    }
    if t > 0.0 {
        let mut top = P[6];
        for &coeff in P[..6].iter().rev() {
            top = excel_x87_add(excel_x87_mul(top, t), coeff);
        }
        let mut bot = Q[4];
        for &coeff in Q[..4].iter().rev() {
            bot = excel_x87_add(excel_x87_mul(bot, t), coeff);
        }
        let w = excel_x87_div(top, bot);
        if d > 0.0 {
            excel_x87_mul(
                excel_x87_div(t, a),
                excel_x87_sub(excel_x87_sub(w, 0.5), 0.5),
            )
        } else {
            excel_x87_mul(a, w)
        }
    } else {
        let mut top = R[8];
        for &coeff in R[..8].iter().rev() {
            top = excel_x87_add(excel_x87_mul(top, t), coeff);
        }
        let bot = excel_x87_add(
            excel_x87_mul(excel_x87_add(excel_x87_mul(S2, t), S1), t),
            1.0,
        );
        let w = excel_x87_div(top, bot);
        if d > 0.0 {
            excel_x87_div(excel_x87_mul(t, w), a)
        } else {
            excel_x87_mul(a, excel_x87_add(excel_x87_add(w, 0.5), 0.5))
        }
    }
}

// ---- exp_rd: round-toward-zero exp (W109 chopped-exp identification) -------
// Excel's series-path r = exp(t1)/G publishes a TRUNCATED exp, not nearest:
// floor(true exp) scores 38/45 on the implied-exp corpus vs CR 25/45 and
// fdlibm 28/45 (every real 2010-era MSVC CRT exp refuted by direct binary
// probes: 32-bit SSE2 rounds one-sided HIGH, x87 and x64-AMD are CR here).
// Corpus effect at the series call site: +26/306 training, +3/111 held-out
// b20 (fractional-a normalizer noise caps the held-out margin).
// Implementation: the x87 exponential with a chopping binary64 store (`excel_exp_rz`) (dc-8dg:
// one chain on every target; off x86_64 the software x87 runs it).

pub fn exp_rd(x: f64) -> f64 {
    // W109 F2XM1 identification: the series-site chop is RZ53 of the x87 exponential
    // chain (the hardware on x86_64, the software x87 elsewhere).
    crate::ported::excel_numeric::excel_exp_rz(x)
}

/// gamma_fort.f (Morris's GAMMA): the un-normalized gamma used as the gratio
/// normalizer via r = exp(a*ln x - x) / gamma(a).
#[allow(dead_code)] // superseded by gratio_norm_gamma (identification: Excel != NSWC gamma)
fn gratio_gamma_nswc(a: f64) -> f64 {
    const PI: f64 = 3.1415926535898;
    const D: f64 = 0.41893853320467274178;
    const P: [f64; 7] = [
        0.539637273585445e-03,
        0.261939260042690e-02,
        0.204493667594920e-01,
        0.730981088720487e-01,
        0.279648642639792e+00,
        0.553413866010467e+00,
        1.0,
    ];
    const Q: [f64; 7] = [
        -0.832979206704073e-03,
        0.470059485860584e-02,
        0.225211131035340e-01,
        -0.170458969313360e+00,
        -0.567902761974940e-01,
        0.113062953091122e+01,
        1.0,
    ];
    const R1: f64 = 0.820756370353826e-03;
    const R2: f64 = -0.595156336428591e-03;
    const R3: f64 = 0.793650663183693e-03;
    const R4: f64 = -0.277777777770481e-02;
    const R5: f64 = 0.833333333333333e-01;
    let mut x = a;
    let mut s = 0.0;
    if a.abs() < 15.0 {
        let mut t = 1.0;
        let m = (a as i64) - 1;
        if m >= 0 {
            for _ in 0..m {
                x -= 1.0;
                t = x * t;
            }
            x -= 1.0;
        } else {
            t = a;
            if a <= 0.0 {
                let mm = -m - 1;
                for _ in 0..mm {
                    x += 1.0;
                    t = x * t;
                }
                x = (x + 0.5) + 0.5;
                t = x * t;
                if t == 0.0 {
                    return 0.0;
                }
            }
            if t.abs() < 1e-30 {
                if t.abs() * GRATIO_SPMPAR3 <= 1.0001 {
                    return 0.0;
                }
                return 1.0 / t;
            }
        }
        let mut top = P[0];
        let mut bot = Q[0];
        for i in 1..7 {
            top = P[i] + x * top;
            bot = Q[i] + x * bot;
        }
        let g = top / bot;
        return if a < 1.0 { g / t } else { g * t };
    }
    // |a| >= 15
    if a.abs() >= 1e3 {
        return 0.0;
    }
    if a <= 0.0 {
        x = -a;
        let n = x as i64;
        let mut tt = x - n as f64;
        if tt > 0.9 {
            tt = 1.0 - tt;
        }
        s = crt_sin(PI * tt) / PI;
        if n % 2 == 0 {
            s = -s;
        }
        if s == 0.0 {
            return 0.0;
        }
    }
    let t = 1.0 / (x * x);
    let mut g = ((((R1 * t + R2) * t + R3) * t + R4) * t + R5) / x;
    let lnx = crt_log(x);
    let z = x;
    g = (D + g) + (z - 0.5) * (lnx - 1.0);
    let w = g;
    let tail = g - w;
    if w > 0.99999 * gratio_exparg(0) {
        return 0.0;
    }
    let mut gam = crate::ported::excel_numeric::excel_exp(w) * (1.0 + tail);
    if a < 0.0 {
        gam = (1.0 / (gam * s)) / x;
    }
    gam
}

/// Excel's incomplete-gamma-ratio kernel. Returns `(P(a,x), Q(a,x))`.
/// `(2.0, 2.0)` is the NSWC error sentinel for invalid arguments.
pub fn gratio(a: f64, x: f64) -> (f64, f64) {
    let e = GRATIO_E;
    if a < 0.0 || x < 0.0 || (a == 0.0 && x == 0.0) {
        return (2.0, 2.0);
    }
    if a == 0.0 || x == 0.0 {
        return if x <= a { (0.0, 1.0) } else { (1.0, 0.0) };
    }
    use crate::ported::discrete_dist::poisson_mass;
    if a == 1.0 {
        // Excel dispatches a == 1 to the exponential CDF (W109: proven by the
        // a = 1+2^-20 contradiction probe; emulator race a=1 slice 179/205
        // with this wrapper). Nearest-rounded exp/expm1 — the chopped series
        // exp does NOT apply on this path.
        return (
            -crate::ported::excel_numeric::excel_expm1_internal(-x),
            crate::ported::excel_numeric::excel_exp(-x),
        );
    }
    // Excel uses ind = 0 (unscaled). gratio.f: iop = ind + 1 = 1 selects acc = 5e-15 for the a < 1 leaf.
    let iop = 1usize;
    let acc = GRATIO_ACC0[iop - 1].max(e);

    if a < 1.0 {
        // DEVIATION 1: no a == 0.5 special dispatch; 0.5 flows into these
        // general a < 1 branches.
        if x < 1.1 {
            // statement 160: Taylor series for small a, with TOMS 654 iop = 1 double-precision
            // st200 branch thresholds (cb11 / dc-j9k.67), PC64 excel_x87_* + excel_log(x) staging,
            // and Excel's internal Kahan expm1 (excel_expm1_internal) in st200.
            use crate::ported::excel_numeric::{
                excel_exp, excel_expm1_internal, excel_log, excel_x87_add, excel_x87_div,
                excel_x87_mul, excel_x87_sub,
            };
            const NEG_LN2: f64 = f64::from_bits(0xbfe6_2e42_fefa_39ef); // -ln(2)
            let mut an = 3.0_f64;
            let mut c = x;
            let mut summ = excel_x87_div(x, excel_x87_add(a, 3.0));
            let tol = excel_x87_div(3.0 * acc, excel_x87_add(a, 1.0));
            loop {
                an += 1.0;
                c = -excel_x87_mul(c, excel_x87_div(x, an));
                let t = excel_x87_div(c, excel_x87_add(a, an));
                summ = excel_x87_add(summ, t);
                if t.abs() <= tol {
                    break;
                }
            }
            let inner = excel_x87_add(
                excel_x87_mul(
                    excel_x87_sub(
                        excel_x87_div(summ, 6.0),
                        excel_x87_div(0.5, excel_x87_add(a, 2.0)),
                    ),
                    x,
                ),
                excel_x87_div(1.0, excel_x87_add(a, 1.0)),
            );
            let j = excel_x87_mul(excel_x87_mul(a, x), inner);
            let z = excel_x87_mul(a, excel_log(x));
            let h = gratio_gam1(a);
            let g = excel_x87_add(1.0, h);
            let go200 = if x <= 0.5 {
                z >= NEG_LN2
            } else {
                excel_x87_sub(a, x) < 0.25
            };
            if go200 {
                // statement 200
                let l = excel_expm1_internal(z);
                let w = excel_x87_add(0.5, excel_x87_add(0.5, l));
                let qans = excel_x87_sub(
                    excel_x87_mul(excel_x87_sub(excel_x87_mul(w, j), l), g),
                    h,
                );
                if qans < 0.0 {
                    return (1.0, 0.0);
                }
                return (excel_x87_add(0.5, excel_x87_sub(0.5, qans)), qans);
            }
            // statement 190
            let w = excel_exp(z);
            let ans = excel_x87_mul(
                excel_x87_mul(w, g),
                excel_x87_add(0.5, excel_x87_sub(0.5, j)),
            );
            return (ans, 0.5 + (0.5 - ans));
        }
        // a < 1, x >= 1.1 (cb8 / dc-j9k.64): unified poisson_mass + Modified-Lentz gratio_cf.
        let pm = poisson_mass(a, x);
        if crate::ported::excel_numeric::excel_x87_mul(pm, a) == 0.0 {
            return (1.0, 0.0);
        }
        return gratio_cf(a, x, pm);
    }

    // Unified finite-sum domain for integer and half-integer shapes: 1 < a <= 20, a < x <= 30.0
    // (cb7 / dc-j9k.63). Both integer (GRATIO_RECIP_FACTORIAL) and half-integer (GRATIO_HALF_RECIP)
    // branches use stored x87 Horner and share the exact a < x <= 30.0 boundaries.
    if a <= 20.0 && a < x && x <= 30.0 {
        if a == a.floor() {
            let qans = gratio_integer_sum(a as usize, x);
            return (0.5 + (0.5 - qans), qans);
        }
        let twoa = a + a;
        let m = twoa as i64;
        if twoa == m as f64 {
            let qans = gratio_half_integer_sum((m / 2) as usize, x);
            return (0.5 + (0.5 - qans), qans);
        }
    }

    // Remaining 1 < a <= 20.0: forward Taylor series for x <= a, continued fraction for x > a
    // (no GRATIO_ALOG10 floor and no x0 = 31.0 switch to asymptotic expansion; cb7 §1.3-1.4).
    if a <= 20.0 {
        let pm = poisson_mass(a, x);
        let r = crate::ported::excel_numeric::excel_x87_mul(pm, a);
        if r == 0.0 {
            return if x <= a { (0.0, 1.0) } else { (1.0, 0.0) };
        }
        if x <= a {
            return gratio_series(a, x, pm);
        }
        return gratio_cf(a, x, pm);
    }

    // a > 20.0 (bisected on live Excel 16.0.20527: Temme runs strictly for 0.7 < l < 1.3).
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_sub};
    let l = excel_x87_div(x, a);
    if l == 0.0 {
        return (0.0, 1.0);
    }
    if l > 0.7 && l < 1.3 {
        return gratio_temme(a, x);
    }
    let pm = poisson_mass(a, x);
    let r = excel_x87_mul(pm, a);
    if r == 0.0 {
        return if x <= a { (0.0, 1.0) } else { (1.0, 0.0) };
    }
    if x <= a {
        return gratio_series(a, x, pm);
    }
    // l >= 1.3: x87-stored forward asymptotic expansion (cb7 §1.1, dc-j9k.63).
    let mut amn = a;
    let mut t = 1.0_f64;
    let mut s = 1.0_f64;
    loop {
        amn = excel_x87_sub(amn, 1.0);
        let nt = excel_x87_mul(t, excel_x87_div(amn, x));
        if nt.abs() > t.abs() {
            break;
        }
        t = nt;
        s = excel_x87_add(s, t);
        if excel_x87_div(t.abs(), s) <= f64::EPSILON {
            break;
        }
    }
    let qans = excel_x87_mul(pm, excel_x87_mul(excel_x87_div(a, x), s));
    (0.5 + (0.5 - qans), qans)
}

fn gratio_series(a: f64, x: f64, r_series: f64) -> (f64, f64) {
    // Forward Taylor series (Cephes-igam style): ans accumulates from 1.0, stop at
    // c / ans <= 2^-53, stored x87 product r_series * ans before binary64 publication.
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_div, excel_x87_mul};
    let mut rr = a;
    let mut c = 1.0_f64;
    let mut ans = 1.0_f64;
    loop {
        rr += 1.0;
        c = excel_x87_mul(c, excel_x87_div(x, rr));
        ans = excel_x87_add(ans, c);
        if excel_x87_div(c, ans) <= 1.110_223_024_625_156_5e-16 {
            break;
        }
    }
    let p = excel_x87_mul(r_series, ans);
    (p, 0.5 + (0.5 - p))
}

/// Upper incomplete gamma continued fraction for all `a <= 20` upper-tail calls
/// (`1 < a <= 20, x > a` outside the `a < x <= 30.0` finite sums, and `a < 1, x >= 1.1`,
/// cb8 / dc-j9k.64): Thompson-Barnett Modified Lentz from `b_0 = 0` with `TINY = 1e-30`
/// on the uncontracted Legendre CF (`a_0 = a, b_1 = x, a_{2m-1} = m - a, b_{2m} = 1,
/// a_{2m} = m, b_{2m+1} = x`), every arithmetic step stored to binary64 at x87 PC64,
/// stopping at `k > 0 && del == 1.0`, and multiplying `pm * f` directly.
fn gratio_cf(a: f64, x: f64, pm: f64) -> (f64, f64) {
    use crate::ported::excel_numeric::{
        excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_sub,
    };
    const TINY: f64 = 1.0e-30; // 0x39b08b2a2c280291
    let mut f = TINY;
    let mut c = TINY;
    let mut d = 0.0_f64;
    for k in 0..=10_000usize {
        let ak = if k == 0 {
            a
        } else if (k & 1) == 1 {
            excel_x87_sub(((k + 1) / 2) as f64, a)
        } else {
            (k / 2) as f64
        };
        let bk = if (k & 1) == 1 { 1.0 } else { x };
        d = excel_x87_add(bk, excel_x87_mul(ak, d));
        if d.abs() < TINY {
            d = TINY;
        }
        c = excel_x87_add(bk, excel_x87_div(ak, c));
        if c.abs() < TINY {
            c = TINY;
        }
        d = excel_x87_div(1.0, d);
        let del = excel_x87_mul(c, d);
        f = excel_x87_mul(f, del);
        if k > 0 && del == 1.0 {
            break;
        }
    }
    let qans = excel_x87_mul(pm, f);
    (0.5 + (0.5 - qans), qans)
}

/// Temme expansion for `a > 20` and `0.7 < x/a < 1.3` (dc-ymjf.2.27).
///
/// Measured on Excel 16.0.20527 across five 256-double windows (1,280/1,280 exact `s = sqrt(y)`)
/// and verified on a 300-point frozen-prediction holdout (291/300 0-ULP, 300/300 <= 1 ULP):
/// - `u = (x - a) / a`, `d2 = u + 2`, `r = u / d2`, `sum = r^2/3 + r^4/5 + ...` (each step stored),
///   `z = r * (u - 2 * sum)`, `y = a * z`, `s = sqrt(y)`, `eta = sgn(x - a) * sqrt(2 * z)`;
/// - `h = 0.5 * ERFC.PRECISE(s)` (0.5 at `x == a`), `c = EXP(-y)`,
/// - `t = sum_{k=0}^{10} c_k(eta) * a^{-k}` with full binary64 exact rational Temme coefficients,
/// - `term = ((c * TEMME_RT2PIN) / sqrt(a)) * t`;
/// - `x > a`: `Q = h + term`, `P = 1 - Q`; `x <= a`: `P = h - term`, `Q = 1 - P`.
fn gratio_temme(a: f64, x: f64) -> (f64, f64) {
    use crate::ported::excel_numeric::{
        excel_exp, excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_sub,
    };

    let u = excel_x87_div(excel_x87_sub(x, a), a);
    let d2 = excel_x87_add(u, 2.0);
    let r = excel_x87_div(u, d2);
    let r2 = excel_x87_mul(r, r);
    let mut p = r2;
    let mut sum = 0.0;
    let mut k = 3.0;
    loop {
        let next = excel_x87_add(sum, excel_x87_div(p, k));
        if next == sum {
            break;
        }
        sum = next;
        p = excel_x87_mul(p, r2);
        k += 2.0;
    }
    let inner = excel_x87_sub(u, excel_x87_mul(2.0, sum));
    let z = excel_x87_mul(r, inner);
    let y = excel_x87_mul(a, z);
    let s = y.sqrt();
    let mut eta = (z + z).sqrt();
    if x < a {
        eta = -eta;
    }

    let inv_a = excel_x87_div(1.0, a);
    let horner = |coeffs: &[f64]| -> f64 {
        let mut acc = coeffs[coeffs.len() - 1];
        for &coeff in coeffs[..coeffs.len() - 1].iter().rev() {
            acc = excel_x87_add(excel_x87_mul(acc, eta), coeff);
        }
        acc
    };
    let mut t = horner(TEMME_TABLES[TEMME_TABLES.len() - 1]);
    for &table in TEMME_TABLES[..TEMME_TABLES.len() - 1].iter().rev() {
        t = excel_x87_add(excel_x87_mul(t, inv_a), horner(table));
    }

    let rta = a.sqrt();
    let c = excel_exp(-y);
    let h = if x == a {
        0.5
    } else {
        0.5 * crate::ported::special_dist_family::erfc_precise_kernel(s).unwrap_or(0.0)
    };
    let term = excel_x87_mul(excel_x87_div(excel_x87_mul(c, TEMME_RT2PIN), rta), t);
    if x > a {
        let q = excel_x87_add(h, term);
        let p = excel_x87_add(0.5, excel_x87_sub(0.5, q));
        (p, q)
    } else {
        let p = excel_x87_sub(h, term);
        let q = excel_x87_add(0.5, excel_x87_sub(0.5, p));
        (p, q)
    }
}

pub fn regularized_gamma_p(a: f64, x: f64) -> f64 {
    if !(a > 0.0) || !(x >= 0.0) || !a.is_finite() || !x.is_finite() {
        return f64::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    gratio(a, x).0
}

pub fn regularized_gamma_q(a: f64, x: f64) -> f64 {
    if !(a > 0.0) || !(x >= 0.0) || !a.is_finite() || !x.is_finite() {
        return f64::NAN;
    }
    if x == 0.0 {
        return 1.0;
    }
    gratio(a, x).1
}

// ---------------------------------------------------------------------------
// SOURCE: A. R. DiDonato and A. H. Morris Jr., "Algorithm 708: Significant digit computation of
// the incomplete beta function ratios", ACM Transactions on Mathematical Software 18(3), 1992
// (bratio, NSWC library), as distributed in scipy's cdflib (scipy v0.14, DCDFLIB by Brown,
// Lovato and Russell). Translated from that Fortran into Rust; kept (owner, 2026-10-30).
//
// BRATIO: faithful port of the DCDFLIB / NSWC incomplete-beta-ratio kernel
// (TOMS 708, DiDonato & Morris), transcribed op-for-op from the validated
// Python reference at smart-fuzzer/work/w109/G3-01-dist/agentA_bratio.py, which
// mirrors the scipy v0.14 cdflib Fortran sources. Identification (W109) proved
// this is Excel's incomplete-beta substrate (BETA.DIST / BETADIST / the beta-
// tail bodies behind the T/F/binomial families).
//
// The Python's injectable LOG/EXP/POW are correctly-rounded stand-ins for the
// plain C-runtime log/exp/pow; here they map directly to f64::ln / f64::exp /
// f64::powf. Every arithmetic op, Fortran GOTO branch, DATA constant and the
// `0.5 + (0.5 - x)` idioms are preserved verbatim. Do not reorder or simplify.
//
// All routines are prefixed `bratio_` to avoid any collision with the gratio_*
// / ln_gamma code above; nothing here is shared with that kernel.
// ---------------------------------------------------------------------------

/// Fortran INTEGER assignment from REAL: truncate toward zero.
fn bratio_ftoi(x: f64) -> i32 {
    x as i32
}

/// spmpar(1) = 2**(1-53) = 2^-52.
fn bratio_spmpar1() -> f64 {
    2.0f64.powi(1 - 53)
}

/// exparg(l) (specialized to this x86_64 host's double range).
fn bratio_exparg(l: i32) -> f64 {
    let lnb = 0.69314718055995;
    if l != 0 {
        let m = -1021 - 1;
        0.99999 * (m as f64 * lnb)
    } else {
        let m = 1024;
        0.99999 * (m as f64 * lnb)
    }
}

/// esum: evaluation of exp(mu + x).
fn bratio_esum(mu: i32, x: f64) -> f64 {
    if x > 0.0 {
        if mu > 0 {
            return bratio_esum20(mu, x);
        }
        let w = mu as f64 + x;
        if w < 0.0 {
            return bratio_esum20(mu, x);
        }
        return crate::ported::excel_numeric::excel_exp(w);
    }
    if mu < 0 {
        return bratio_esum20(mu, x);
    }
    let w = mu as f64 + x;
    if w > 0.0 {
        return bratio_esum20(mu, x);
    }
    crate::ported::excel_numeric::excel_exp(w)
}

fn bratio_esum20(mu: i32, x: f64) -> f64 {
    let w = mu as f64;
    crate::ported::excel_numeric::excel_exp(w) * crate::ported::excel_numeric::excel_exp(x)
}

/// alnrel: evaluation of the function ln(1 + a).
fn bratio_alnrel(a: f64) -> f64 {
    const P1: f64 = -0.129418923021993e+01;
    const P2: f64 = 0.405303492862024e+00;
    const P3: f64 = -0.178874546012214e-01;
    const Q1: f64 = -0.162752256355323e+01;
    const Q2: f64 = 0.747811014037616e+00;
    const Q3: f64 = -0.845104217945565e-01;
    // Recorded beta probability 0.375 selects the logarithm at equality.
    if a.abs() >= 0.375 {
        let x = 1.0 + a;
        return crt_log(x);
    }
    let t = a / (a + 2.0);
    let t2 = t * t;
    // Recorded beta tails distinguish the denominator's final PC64 store.
    let denominator =
        crate::ported::excel_numeric::excel_x87_add(((Q3 * t2 + Q2) * t2 + Q1) * t2, 1.0);
    let w = (((P3 * t2 + P2) * t2 + P1) * t2 + 1.0) / denominator;
    2.0 * t * w
}

/// rlog1: evaluation of the function x - ln(1 + x).
fn bratio_rlog1(x: f64) -> f64 {
    const A: f64 = 0.566749439387324e-01;
    const B: f64 = 0.456512608815524e-01;
    const P0: f64 = 0.333333333333333e+00;
    const P1: f64 = -0.224696413112536e+00;
    const P2: f64 = 0.620886815375787e-02;
    const Q1: f64 = -0.127408923933623e+01;
    const Q2: f64 = 0.354508718369557e+00;
    if x < -0.39 || x > 0.57 {
        let w = (x + 0.5) + 0.5;
        return x - crt_log(w);
    }
    let h;
    let w1;
    if x < -0.18 {
        let mut hh = x + 0.3;
        hh /= 0.7;
        h = hh;
        w1 = A - hh * 0.3;
    } else if x > 0.18 {
        h = 0.75 * x - 0.25;
        w1 = B + h / 3.0;
    } else {
        h = x;
        w1 = 0.0;
    }
    let r = h / (h + 2.0);
    let t = r * r;
    let w = ((P2 * t + P1) * t + P0) / ((Q2 * t + Q1) * t + 1.0);
    2.0 * t * (1.0 / (1.0 - r) - r * w) + w1
}

/// gam1: computation of 1/gamma(a+1) - 1 for -0.5 <= a <= 1.5.
fn bratio_gam1(a: f64) -> f64 {
    const P: [f64; 7] = [
        0.577215664901533e+00,
        -0.409078193005776e+00,
        -0.230975380857675e+00,
        0.597275330452234e-01,
        0.766968181649490e-02,
        -0.514889771323592e-02,
        0.589597428611429e-03,
    ];
    const Q: [f64; 5] = [
        0.100000000000000e+01,
        0.427569613095214e+00,
        0.158451672430138e+00,
        0.261132021441447e-01,
        0.423244297896961e-02,
    ];
    const R: [f64; 9] = [
        -0.422784335098468e+00,
        -0.771330383816272e+00,
        -0.244757765222226e+00,
        0.118378989872749e+00,
        0.930357293360349e-03,
        -0.118290993445146e-01,
        0.223047661158249e-02,
        0.266505979058923e-03,
        -0.132674909766242e-03,
    ];
    const S1: f64 = 0.273076135303957e+00;
    const S2: f64 = 0.559398236957378e-01;
    let mut t = a;
    let d = a - 0.5;
    if d > 0.0 {
        t = d - 0.5;
    }
    if t == 0.0 {
        return 0.0;
    }
    if t > 0.0 {
        let top = (((((P[6] * t + P[5]) * t + P[4]) * t + P[3]) * t + P[2]) * t + P[1]) * t + P[0];
        let bot = (((Q[4] * t + Q[3]) * t + Q[2]) * t + Q[1]) * t + 1.0;
        let w = top / bot;
        if d > 0.0 {
            (t / a) * ((w - 0.5) - 0.5)
        } else {
            a * w
        }
    } else {
        let top = (((((((R[8] * t + R[7]) * t + R[6]) * t + R[5]) * t + R[4]) * t + R[3]) * t
            + R[2])
            * t
            + R[1])
            * t
            + R[0];
        let bot = (S2 * t + S1) * t + 1.0;
        let w = top / bot;
        if d > 0.0 {
            t * w / a
        } else {
            a * ((w + 0.5) + 0.5)
        }
    }
}

/// gamln1: evaluation of ln(gamma(1 + a)) for -0.2 <= a <= 1.25.
fn bratio_gamln1(a: f64) -> f64 {
    const P0: f64 = 0.577215664901533e+00;
    const P1: f64 = 0.844203922187225e+00;
    const P2: f64 = -0.168860593646662e+00;
    const P3: f64 = -0.780427615533591e+00;
    const P4: f64 = -0.402055799310489e+00;
    const P5: f64 = -0.673562214325671e-01;
    const P6: f64 = -0.271935708322958e-02;
    const Q1: f64 = 0.288743195473681e+01;
    const Q2: f64 = 0.312755088914843e+01;
    const Q3: f64 = 0.156875193295039e+01;
    const Q4: f64 = 0.361951990101499e+00;
    const Q5: f64 = 0.325038868253937e-01;
    const Q6: f64 = 0.667465618796164e-03;
    const R0: f64 = 0.422784335098467e+00;
    const R1: f64 = 0.848044614534529e+00;
    const R2: f64 = 0.565221050691933e+00;
    const R3: f64 = 0.156513060486551e+00;
    const R4: f64 = 0.170502484022650e-01;
    const R5: f64 = 0.497958207639485e-03;
    const S1: f64 = 0.124313399877507e+01;
    const S2: f64 = 0.548042109832463e+00;
    const S3: f64 = 0.101552187439830e+00;
    const S4: f64 = 0.713309612391000e-02;
    const S5: f64 = 0.116165475989616e-03;
    if a >= 0.6 {
        let x = (a - 0.5) - 0.5;
        let denominator = crate::ported::excel_numeric::excel_x87_mul_add(
            (((S5 * x + S4) * x + S3) * x + S2) * x + S1,
            x,
            1.0,
        );
        let w = (((((R5 * x + R4) * x + R3) * x + R2) * x + R1) * x + R0) / denominator;
        return x * w;
    }
    let numerator_mid =
        crate::ported::excel_numeric::excel_x87_mul_add(((P6 * a + P5) * a + P4) * a + P3, a, P2);
    let w = ((numerator_mid * a + P1) * a + P0)
        / ((((((Q6 * a + Q5) * a + Q4) * a + Q3) * a + Q2) * a + Q1) * a + 1.0);
    -a * w
}

/// gamln: evaluation of ln(gamma(a)) for positive a.
fn bratio_gamln(a: f64) -> f64 {
    const D: f64 = 0.418938533204673e0;
    const C0: f64 = 0.833333333333333e-01;
    const C1: f64 = -0.277777777760991e-02;
    const C2: f64 = 0.793650666825390e-03;
    const C3: f64 = -0.595202931351870e-03;
    const C4: f64 = 0.837308034031215e-03;
    const C5: f64 = -0.165322962780713e-02;
    if a <= 0.8 {
        return bratio_gamln1(a) - crt_log(a);
    }
    if a <= 2.25 {
        let t = (a - 0.5) - 0.5;
        return bratio_gamln1(t);
    }
    if a < 10.0 {
        let n = bratio_ftoi(a - 1.25);
        let mut t = a;
        let mut w = 1.0;
        for _ in 0..n {
            t -= 1.0;
            w = t * w;
        }
        return bratio_gamln1(t - 1.0) + crt_log(w);
    }
    let t = crt_pow(1.0 / a, 2.0);
    let w = (((((C5 * t + C4) * t + C3) * t + C2) * t + C1) * t + C0) / a;
    (D + w) + (a - 0.5) * (crt_log(a) - 1.0)
}

/// algdiv: computation of ln(gamma(b)/gamma(a+b)) when b >= 8.
fn bratio_algdiv(a: f64, b: f64) -> f64 {
    const C0: f64 = 0.833333333333333e-01;
    const C1: f64 = -0.277777777760991e-02;
    const C2: f64 = 0.793650666825390e-03;
    const C3: f64 = -0.595202931351870e-03;
    const C4: f64 = 0.837308034031215e-03;
    const C5: f64 = -0.165322962780713e-02;
    let (h, c, x, d);
    if a > b {
        h = b / a;
        c = 1.0 / (1.0 + h);
        x = h / (1.0 + h);
        d = a + (b - 0.5);
    } else {
        h = a / b;
        c = h / (1.0 + h);
        x = 1.0 / (1.0 + h);
        d = b + (a - 0.5);
    }
    let x2 = x * x;
    let s3 = 1.0 + (x + x2);
    let s5 = 1.0 + (x + x2 * s3);
    let s7 = 1.0 + (x + x2 * s5);
    let s9 = 1.0 + (x + x2 * s7);
    let s11 = 1.0 + (x + x2 * s9);
    let t = crt_pow(1.0 / b, 2.0);
    let mut w = ((((C5 * s11 * t + C4 * s9) * t + C3 * s7) * t + C2 * s5) * t + C1 * s3) * t + C0;
    w *= c / b;
    let u = d * bratio_alnrel(a / b);
    let v = a * (crt_log(b) - 1.0);
    if u > v { (w - v) - u } else { (w - u) - v }
}

/// bcorr: evaluation of del(a0) + del(b0) - del(a0 + b0) where a0, b0 >= 8.
fn bratio_bcorr(a0: f64, b0: f64) -> f64 {
    const C0: f64 = 0.833333333333333e-01;
    const C1: f64 = -0.277777777760991e-02;
    const C2: f64 = 0.793650666825390e-03;
    const C3: f64 = -0.595202931351870e-03;
    const C4: f64 = 0.837308034031215e-03;
    const C5: f64 = -0.165322962780713e-02;
    let a = a0.min(b0);
    let b = a0.max(b0);
    let h = a / b;
    let c = h / (1.0 + h);
    let x = 1.0 / (1.0 + h);
    let x2 = x * x;
    let s3 = 1.0 + (x + x2);
    let s5 = 1.0 + (x + x2 * s3);
    let s7 = 1.0 + (x + x2 * s5);
    let s9 = 1.0 + (x + x2 * s7);
    let s11 = 1.0 + (x + x2 * s9);
    let mut t = crt_pow(1.0 / b, 2.0);
    let mut w = ((((C5 * s11 * t + C4 * s9) * t + C3 * s7) * t + C2 * s5) * t + C1 * s3) * t + C0;
    w *= c / b;
    t = crt_pow(1.0 / a, 2.0);
    (((((C5 * t + C4) * t + C3) * t + C2) * t + C1) * t + C0) / a + w
}

/// gsumln: evaluation of ln(gamma(a + b)) for 1 <= a <= 2 and 1 <= b <= 2.
fn bratio_gsumln(a: f64, b: f64) -> f64 {
    let x = a + b - 2.0;
    if x <= 0.25 {
        return bratio_gamln1(1.0 + x);
    }
    if x <= 1.25 {
        return bratio_gamln1(x) + bratio_alnrel(x);
    }
    bratio_gamln1(x - 1.0) + crt_log(x * (1.0 + x))
}

/// betaln: evaluation of the logarithm of the beta function.
pub(super) fn bratio_betaln(a0: f64, b0: f64) -> f64 {
    const E: f64 = 0.918938533204673e0;
    let mut a = a0.min(b0);
    let b = a0.max(b0);
    if a >= 8.0 {
        let w = bratio_bcorr(a, b);
        let h = a / b;
        let c = h / (1.0 + h);
        let u = -(a - 0.5) * crt_log(c);
        let v = b * bratio_alnrel(h);
        if u > v {
            return (((-0.5 * crt_log(b) + E) + w) - v) - u;
        }
        return (((-0.5 * crt_log(b) + E) + w) - u) - v;
    }
    if a >= 1.0 {
        // At a = 2, the recorded beta and F tails use the reduction arm.
        if a < 2.0 {
            if b <= 2.0 {
                return bratio_gamln(a) + bratio_gamln(b) - bratio_gsumln(a, b);
            }
            let w = 0.0;
            if b < 8.0 {
                return bratio_betaln_60(a, b, w);
            }
            return bratio_gamln(a) + bratio_algdiv(a, b);
        }
        // a >= 2
        if b > 1000.0 {
            let n = bratio_ftoi(a - 1.0);
            let mut w = 1.0;
            for _ in 0..n {
                a -= 1.0;
                w *= a / (1.0 + a / b);
            }
            return (crt_log(w) - n as f64 * crt_log(b)) + (bratio_gamln(a) + bratio_algdiv(a, b));
        }
        let n = bratio_ftoi(a - 1.0);
        let mut w = 1.0;
        for _ in 0..n {
            a -= 1.0;
            let h = a / b;
            w *= h / (1.0 + h);
        }
        let w = crt_log(w);
        if b >= 8.0 {
            return w + bratio_gamln(a) + bratio_algdiv(a, b);
        }
        return bratio_betaln_60(a, b, w);
    }
    // a < 1
    if b >= 8.0 {
        return bratio_gamln(a) + bratio_algdiv(a, b);
    }
    bratio_gamln(a) + (bratio_gamln(b) - bratio_gamln(a + b))
}

fn bratio_betaln_60(a: f64, b0: f64, w: f64) -> f64 {
    let n = bratio_ftoi(b0 - 1.0);
    let mut z = 1.0;
    let mut b = b0;
    for _ in 0..n {
        b -= 1.0;
        z *= b / (a + b);
    }
    w + crt_log(z) + (bratio_gamln(a) + (bratio_gamln(b) - bratio_gsumln(a, b)))
}

/// `ln B(a, b)` as Excel 16.0.20527 forms it across all shape pairs (dc-6332.8, dc-j9k.54):
///
/// - `small == 1`: `-LN(large)`.
/// - `large <= 8`: `(GAMMALN(large) - GAMMALN(a + b)) + GAMMALN(small)`, each step stored.
/// - `small < 8, large > 8`: `(a + b)`-factored six-term stored PC64 form:
///   `(((GAMMALN(a) + (s(b) - s(a + b))) + 0.5 ln(1 + a/b)) + a) - a LN(a + b) - b ln(1 + a/b)`
///   (30/30 fresh Touchstone PDFs, 1,033/1,033 half-shape series rows, 23/30 integer 2..7 CDFs vs
///   16/30 Pochhammer).
/// - `small >= 8, large >= 8`: `(((-a ln(1 + b/a) - b ln(1 + a/b)) + ln sqrt(2 pi))
///   + 0.5 LN((a + b)/(a * b))) + (s(a) + s(b) - s(a + b))` (30/40 fresh Touchstone `ge8_bpser` vs
///   19/40 TOMS `betaln`).
///
/// Falls back to [`bratio_betaln`] only if the direct form overflows to non-finite.
pub(crate) fn excel_ln_beta(a: f64, b: f64) -> f64 {
    use crate::ported::discrete_dist::stirling_error;
    use crate::ported::excel_numeric::{
        excel_log, excel_log1p_times, excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_sub,
        gammaln_excel,
    };
    const LN_SQRT_2PI: f64 = 0.918_938_533_204_672_8;
    let (small, large) = if a <= b { (a, b) } else { (b, a) };
    if small == 1.0 {
        return -excel_log(large);
    }
    if large <= 8.0 {
        return excel_x87_add(
            excel_x87_sub(gammaln_excel(large), gammaln_excel(excel_x87_add(a, b))),
            gammaln_excel(small),
        );
    }
    let ab = excel_x87_add(small, large);
    if small < 8.0 {
        let h = excel_x87_div(small, large);
        let ds = excel_x87_sub(stirling_error(large), stirling_error(ab));
        let s = excel_x87_add(gammaln_excel(small), ds);
        let s = excel_x87_add(s, excel_log1p_times(0.5, h));
        let s = excel_x87_add(s, small);
        let s = excel_x87_sub(s, excel_x87_mul(small, excel_log(ab)));
        let s = excel_x87_sub(s, excel_log1p_times(large, h));
        return if s.is_finite() {
            s
        } else {
            bratio_betaln(a, b)
        };
    }
    let s3 = excel_x87_sub(
        excel_x87_add(stirling_error(small), stirling_error(large)),
        stirling_error(ab),
    );
    let alp = excel_log1p_times(small, excel_x87_div(large, small));
    let blp = excel_log1p_times(large, excel_x87_div(small, large));
    let norm = excel_x87_mul(
        0.5,
        excel_log(excel_x87_div(ab, excel_x87_mul(small, large))),
    );
    let s = excel_x87_add(
        excel_x87_add(
            excel_x87_add(excel_x87_sub(-alp, blp), LN_SQRT_2PI),
            norm,
        ),
        s3,
    );
    if s.is_finite() {
        s
    } else {
        bratio_betaln(a, b)
    }
}

/// `x^a y^b / B(a, b)` as Excel's recurrence arm forms it: the stored `x y` times the beta density
/// (dc-6332.8: with Excel's own density, all 80 calls at each of BETA.DIST(x, a, 2) for eleven
/// `a`, less one row).
fn excel_brcomp(a: f64, b: f64, x: f64, y: f64) -> Option<f64> {
    use crate::ported::excel_numeric::excel_x87_mul;
    let density = crate::ported::discrete_dist::beta_density_at(x, y, a, b)?;
    Some(excel_x87_mul(excel_x87_mul(x, y), density))
}

/// bpser: power series expansion for Ix(a, b) when b <= 1 or b*x <= 0.7.
/// Direct `bpser`/`bpser_sym` exits pass `flush_subnormal_scaled = true` (dc-j9k.44.1);
/// `bup_160` passes `false` so a subnormal series term still contributes to a normal
/// recurrence sum (the ~100-ULP `bup_160` fallback when `scaled` underflows is tracked
/// under `dc-j9k.55`).
fn bratio_bpser(a: f64, b: f64, x: f64, flush_subnormal_scaled: bool) -> f64 {
    if x == 0.0 {
        return 0.0;
    }
    crate::ported::excel_numeric::excel_beta_power_series(
        a,
        b,
        x,
        excel_ln_beta(a, b),
        flush_subnormal_scaled,
    )
}

/// bup: Ix(a, b) - Ix(a+n, b) evaluated for n a positive integer.
fn bratio_bup(a: f64, b: f64, x: f64, y: f64, n: i32, eps: f64) -> f64 {
    use crate::ported::excel_numeric::excel_x87_mul;
    let apb = a + b;
    let ap1 = a + 1.0;
    let mut mu: i32 = 0;
    let mut d = 1.0;
    if !(n == 1 || a < 1.0) {
        if apb >= 1.1 * ap1 {
            mu = bratio_ftoi(bratio_exparg(1).abs());
            let k = bratio_ftoi(bratio_exparg(0));
            if k < mu {
                mu = k;
            }
            let t = mu as f64;
            d = crate::ported::excel_numeric::excel_exp(-t);
        }
    }
    let bp = match crate::ported::discrete_dist::beta_density_at(x, y, a, b)
        .filter(|density| density.is_finite())
    {
        // Excel's own form has no scaling by exp(mu).
        Some(density) => {
            use crate::ported::excel_numeric::{
                excel_beta_up_sum_small_b, excel_exp, excel_log, excel_x87_add, excel_x87_div,
            };
            // Excel flushes a subnormal beta density before multiplying by `scale = x * y / a`
            // (dc-j9k.45.1: verified to 1 ULP across 9 independent `b0 < 0.25` transitions where
            // `scale > 1` would otherwise magnify a subnormal density into a normal first term,
            // including `a = 1015, b = 2.001` where `exp(stored)` is normal and `density` is subnormal).
            if density.is_subnormal() {
                return 0.0;
            }
            let xy = excel_x87_mul(x, y);
            d = 1.0;
            // The division store flushes before the density multiplier.
            // At the minimum normal coordinate it leaves the bgrat seed zero.
            let scale = excel_x87_div(xy, a);
            if scale.is_subnormal() {
                return 0.0;
            }
            let first = excel_x87_mul(scale, density);
            // Excel flushes the stored first term `(x * y / a) * density` before
            // the recurrence (dc-iuwct.63: measured at b0 = 0.5, 0.1, 0.01 where
            // `x * y * density` is already subnormal while `first` is normal).
            if first.is_subnormal() {
                return 0.0;
            }
            // Universal finite-recurrence sum (dc-j9k.58):
            // - For b <= 1 (the 20-step shift before bgrat), coefficients are stored binary64
            //   while x^m and the sum accumulate in extended precision.
            // - For b > 1 (the b-reduction loop across all n >= 2), powers are stored exp(m * ln x)
            //   and summed in binary64.
            if n >= 2 && first != 0.0 {
                if b <= 1.0 {
                    let sum = excel_beta_up_sum_small_b(a, b, x, n);
                    return excel_x87_mul(first, sum);
                }
                let log_x = excel_log(x);
                let ab = excel_x87_add(a, b);
                let a1 = excel_x87_add(a, 1.0);
                let mut coefficient = 1.0;
                let mut sum = 1.0;
                for m in 1..n {
                    let k = (m - 1) as f64;
                    let ratio = excel_x87_div(excel_x87_add(ab, k), excel_x87_add(a1, k));
                    coefficient = excel_x87_mul(coefficient, ratio);
                    let power = excel_exp(excel_x87_mul(m as f64, log_x));
                    sum = excel_x87_add(sum, excel_x87_mul(coefficient, power));
                }
                return excel_x87_mul(first, sum);
            }
            first
        }
        None => bratio_brcmp1(mu, a, b, x, y) / a,
    };
    if n == 1 || bp == 0.0 {
        return bp;
    }
    let nm1 = n - 1;
    let mut w = d;
    let mut k: i32 = 0;
    if b > 1.0 {
        if y > 1.0e-4 {
            let r = (b - 1.0) * x / y - a;
            if r >= 1.0 {
                k = nm1;
                let t = nm1 as f64;
                if r < t {
                    k = bratio_ftoi(r);
                }
            }
        } else {
            k = nm1;
        }
    }
    for i in 1..=k {
        let l = (i - 1) as f64;
        d = excel_x87_mul(((apb + l) / (ap1 + l)) * x, d);
        w += d;
    }
    if k != nm1 {
        for i in (k + 1)..=nm1 {
            let l = (i - 1) as f64;
            d = excel_x87_mul(((apb + l) / (ap1 + l)) * x, d);
            w += d;
            if d <= eps * w {
                break;
            }
        }
    }
    // The recurrence and its final product have measured binary64 stores from PC64.
    excel_x87_mul(bp, w)
}

/// brcomp: evaluation of x^a * y^b / beta(a, b).
fn bratio_brcomp(a: f64, b: f64, x: f64, y: f64) -> f64 {
    const CONST: f64 = 0.398942280401433e0;
    if x == 0.0 || y == 0.0 {
        return 0.0;
    }
    let a0 = a.min(b);
    if a0 >= 8.0 {
        let (h, x0, y0, lambda_);
        if a > b {
            h = b / a;
            x0 = 1.0 / (1.0 + h);
            y0 = h / (1.0 + h);
            lambda_ = (a + b) * y - b;
        } else {
            h = a / b;
            x0 = h / (1.0 + h);
            y0 = 1.0 / (1.0 + h);
            lambda_ = a - (a + b) * x;
        }
        let mut e = -lambda_ / a;
        let u = if e.abs() > 0.6 {
            e - crt_log(x / x0)
        } else {
            bratio_rlog1(e)
        };
        e = lambda_ / b;
        let v = if e.abs() > 0.6 {
            e - crt_log(y / y0)
        } else {
            bratio_rlog1(e)
        };
        let z = crate::ported::excel_numeric::excel_exp(-(a * u + b * v));
        return CONST
            * (b * x0).sqrt()
            * z
            * crate::ported::excel_numeric::excel_exp(-bratio_bcorr(a, b));
    }

    let lnx;
    let lny;
    if x > 0.375 {
        if y > 0.375 {
            lnx = crt_log(x);
            lny = crt_log(y);
        } else {
            lnx = bratio_alnrel(-y);
            lny = crt_log(y);
        }
    } else {
        lnx = crt_log(x);
        lny = bratio_alnrel(-x);
    }
    let mut z = a * lnx + b * lny;
    if a0 >= 1.0 {
        z -= bratio_betaln(a, b);
        return crate::ported::excel_numeric::excel_exp(z);
    }
    let mut b0 = a.max(b);
    if b0 >= 8.0 {
        let u = bratio_gamln1(a0) + bratio_algdiv(a0, b0);
        return a0 * crate::ported::excel_numeric::excel_exp(z - u);
    }
    if b0 > 1.0 {
        let mut u = bratio_gamln1(a0);
        let n = bratio_ftoi(b0 - 1.0);
        if n >= 1 {
            let mut c = 1.0;
            for _ in 0..n {
                b0 -= 1.0;
                c *= b0 / (a0 + b0);
            }
            u = crt_log(c) + u;
        }
        z -= u;
        b0 -= 1.0;
        let apb = a0 + b0;
        let t;
        if apb > 1.0 {
            let uu = a0 + b0 - 1.0;
            t = (1.0 + bratio_gam1(uu)) / apb;
        } else {
            t = 1.0 + bratio_gam1(apb);
        }
        return a0 * crate::ported::excel_numeric::excel_exp(z) * (1.0 + bratio_gam1(b0)) / t;
    }
    let br = crate::ported::excel_numeric::excel_exp(z);
    if br == 0.0 {
        return br;
    }
    let apb = a + b;
    let zz;
    if apb > 1.0 {
        let u = a + b - 1.0;
        zz = (1.0 + bratio_gam1(u)) / apb;
    } else {
        zz = 1.0 + bratio_gam1(apb);
    }
    let c = (1.0 + bratio_gam1(a)) * (1.0 + bratio_gam1(b)) / zz;
    br * (a0 * c) / (1.0 + a0 / b0)
}

/// brcmp1: evaluation of exp(mu) * x^a * y^b / beta(a, b).
fn bratio_brcmp1(mu: i32, a: f64, b: f64, x: f64, y: f64) -> f64 {
    const CONST: f64 = 0.398942280401433e0;
    let a0 = a.min(b);
    if a0 >= 8.0 {
        let (h, x0, y0, lambda_);
        if a > b {
            h = b / a;
            x0 = 1.0 / (1.0 + h);
            y0 = h / (1.0 + h);
            lambda_ = (a + b) * y - b;
        } else {
            h = a / b;
            x0 = h / (1.0 + h);
            y0 = 1.0 / (1.0 + h);
            lambda_ = a - (a + b) * x;
        }
        let mut e = -lambda_ / a;
        let u = if e.abs() > 0.6 {
            e - crt_log(x / x0)
        } else {
            bratio_rlog1(e)
        };
        e = lambda_ / b;
        let v = if e.abs() > 0.6 {
            e - crt_log(y / y0)
        } else {
            bratio_rlog1(e)
        };
        let z = bratio_esum(mu, -(a * u + b * v));
        return CONST
            * (b * x0).sqrt()
            * z
            * crate::ported::excel_numeric::excel_exp(-bratio_bcorr(a, b));
    }

    let lnx;
    let lny;
    if x > 0.375 {
        if y > 0.375 {
            lnx = crt_log(x);
            lny = crt_log(y);
        } else {
            lnx = bratio_alnrel(-y);
            lny = crt_log(y);
        }
    } else {
        lnx = crt_log(x);
        lny = bratio_alnrel(-x);
    }
    let mut z = a * lnx + b * lny;
    if a0 >= 1.0 {
        z -= bratio_betaln(a, b);
        return bratio_esum(mu, z);
    }
    let mut b0 = a.max(b);
    if b0 >= 8.0 {
        let u = bratio_gamln1(a0) + bratio_algdiv(a0, b0);
        return a0 * bratio_esum(mu, z - u);
    }
    if b0 > 1.0 {
        let mut u = bratio_gamln1(a0);
        let n = bratio_ftoi(b0 - 1.0);
        if n >= 1 {
            let mut c = 1.0;
            for _ in 0..n {
                b0 -= 1.0;
                c *= b0 / (a0 + b0);
            }
            u = crt_log(c) + u;
        }
        z -= u;
        b0 -= 1.0;
        let apb = a0 + b0;
        let t;
        if apb > 1.0 {
            let uu = a0 + b0 - 1.0;
            t = (1.0 + bratio_gam1(uu)) / apb;
        } else {
            t = 1.0 + bratio_gam1(apb);
        }
        return a0 * bratio_esum(mu, z) * (1.0 + bratio_gam1(b0)) / t;
    }
    let br = bratio_esum(mu, z);
    if br == 0.0 {
        return br;
    }
    let apb = a + b;
    let zz;
    if apb > 1.0 {
        let u = a + b - 1.0;
        zz = (1.0 + bratio_gam1(u)) / apb;
    } else {
        zz = 1.0 + bratio_gam1(apb);
    }
    let c = (1.0 + bratio_gam1(a)) * (1.0 + bratio_gam1(b)) / zz;
    br * (a0 * c) / (1.0 + a0 / b0)
}

/// bfrac: continued fraction expansion for Ix(a, b) when a, b > 1.
fn bratio_bfrac(a: f64, b: f64, x: f64, y: f64) -> f64 {
    // Below 0.99, the continued fraction uses the density at x and its stored complement.
    // Above 0.99, 1 - x cancels up to 53 bits for F/T callers, so Excel reflects the
    // prefactor to the complementary density using the caller's y (dc-j9k.58).
    // When the density normalizer overflows, retain the scaled beta prefactor.
    let bf = if x > 0.99 {
        excel_brcomp(b, a, y, x)
    } else {
        excel_brcomp(a, b, x, crate::ported::excel_numeric::excel_x87_sub(1.0, x))
    }
    .filter(|value| value.is_finite())
    .unwrap_or_else(|| bratio_brcomp(a, b, x, y));
    crate::ported::excel_numeric::excel_beta_fraction(a, b, x, y, bf)
}

/// bgrat: asymptotic expansion for Ix(a, b) when a is larger than b. Adds the
/// expansion to `w`; returns (w_new, ierr).
fn bratio_bgrat(a: f64, b: f64, x: f64, y: f64, w: f64, eps: f64) -> (f64, i32) {
    // `DBL_MIN / f64::EPSILON = 2^-970`: below this, `eps * total` flushes to zero.
    const TWO_TO_MINUS_970: f64 = f64::from_bits(0x0350_0000_0000_0000);
    let bm1 = (b - 0.5) - 0.5;
    let nu = a + 0.5 * bm1;
    // Excel switches the logarithm at the stored 0.99 threshold for all b <= 1
    // (dc-j9k.58), consuming the companion coordinate above 0.99.
    let lnx = if x < 0.99 {
        crate::ported::excel_numeric::excel_log(x)
    } else {
        crate::ported::excel_numeric::excel_log1p_times(1.0, -y)
    };
    let z = -nu * lnx;
    if b * z == 0.0 {
        return (if w < TWO_TO_MINUS_970 { 0.0 } else { w }, 1);
    }
    let mut r =
        b * (1.0 + bratio_gam1(b)) * crate::ported::excel_numeric::excel_exp(b * crt_log(z));
    r = r
        * crate::ported::excel_numeric::excel_exp(a * lnx)
        * crate::ported::excel_numeric::excel_exp(0.5 * bm1 * lnx);
    // The gamma-family pieces, as the gamma upper tail forms them: the prefactor `r` is `z` times
    // the gamma density at `b`.
    if let Ok(dens) = crate::ported::beta_gamma_stats_family::gamma_dist_kernel(z, b, 1.0, false) {
        r = z * dens;
    }
    if r == 0.0 {
        return (if w < TWO_TO_MINUS_970 { 0.0 } else { w }, 1);
    }
    // Q(b, z): half-shape uses Excel's erf/erfc both directly and when seeded
    // by a finite recurrence (w > 0); unit shape uses exp(-z); all other
    // fractional shapes `b < 1` share Excel's unified `regularized_gamma_q`
    // across both `z >= 1.1` (CF) and `z < 1.1` (cb11 `gratio` series/st200,
    // dc-i2wk3). Its prefactor and the expansion normalizer `r` have separate
    // rounding steps (dc-6332.4).
    let q = if b == 0.5 {
        let root = crate::ported::excel_numeric::excel_sqrt_staged(z);
        if root <= 0.5 {
            0.5 + (0.5 - crate::ported::normal_dist_common::erf_approx(root))
        } else {
            crate::ported::normal_dist_common::erfc_upper(root)
        }
    } else if b == 1.0 {
        crate::ported::excel_numeric::excel_exp(-z)
    } else {
        regularized_gamma_q(b, z)
    };
    let n = a + b;
    let s_n = crate::ported::discrete_dist::stirling_error(n);
    let s_a = crate::ported::discrete_dist::stirling_error(a);
    crate::ported::excel_numeric::excel_beta_bgrat(a, b, bm1, nu, lnx, z, r, q, w, eps, s_n, s_a)
}

/// basym: asymptotic expansion for Ix(a, b) when `min(a, b) > 10_000` and
/// `lambda <= 0.03 * min(a, b)` (dc-iuwct.52). Uses Excel's `rlog1` (`e * r - 2 * atanh_tail(r)`),
/// scaled `erfc` (`normal_dist_common::erfc_scaled` above 0.5, `erf_approx` below 0.5),
/// Stirling correction `s(a) + s(b) - s(a + b)`, and `excel_exp`.
fn bratio_basym(a: f64, b: f64, lambda_: f64, eps: f64) -> f64 {
    use crate::ported::discrete_dist::stirling_error;
    use crate::ported::excel_numeric::{
        excel_exp, excel_rlog1, excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_sub,
    };
    use crate::ported::normal_dist_common::{erf_approx, erfc_scaled};

    const NUM: usize = 20;
    const E0: f64 = 1.128_379_167_095_512_6; // 2 / sqrt(pi)
    const E1: f64 = 0.353_553_390_593_273_73; // 1 / (2 * sqrt(2))
    let mut a0 = [0.0f64; 22];
    let mut b0 = [0.0f64; 22];
    let mut c = [0.0f64; 22];
    let mut d = [0.0f64; 22];
    let (h, r0, r1, w0) = if a >= b {
        let h = b / a;
        (h, 1.0 / (1.0 + h), (b - a) / a, 1.0 / (b * (1.0 + h)).sqrt())
    } else {
        let h = a / b;
        (h, 1.0 / (1.0 + h), (b - a) / b, 1.0 / (a * (1.0 + h)).sqrt())
    };
    let e1 = excel_x87_div(-lambda_, a);
    let e2 = excel_x87_div(lambda_, b);
    let f = excel_x87_add(
        excel_x87_mul(a, excel_rlog1(e1)),
        excel_x87_mul(b, excel_rlog1(e2)),
    );
    let t = excel_exp(-f);
    if t == 0.0 {
        return 0.0;
    }
    let z0 = f.sqrt();
    let z = 0.5 * (z0 / E1);
    let z2 = f + f;
    a0[1] = (2.0 / 3.0) * r1;
    c[1] = -0.5 * a0[1];
    d[1] = -c[1];
    let scaled_erfc = if z0 >= 0.5 {
        erfc_scaled(z0)
    } else {
        excel_x87_mul(
            excel_exp(f),
            excel_x87_add(0.5, excel_x87_sub(0.5, erf_approx(z0))),
        )
    };
    let mut j0 = (0.5 / E0) * scaled_erfc;
    let mut j1 = E1;
    let mut summ = j0 + d[1] * w0 * j1;
    let mut s = 1.0;
    let h2 = h * h;
    let mut hn = 1.0;
    let mut w = w0;
    let mut znm1 = z;
    let mut zn = z2;
    let mut n = 2usize;
    while n <= NUM {
        hn = h2 * hn;
        a0[n] = 2.0 * r0 * (1.0 + h * hn) / (n as f64 + 2.0);
        let np1 = n + 1;
        s += hn;
        a0[np1] = 2.0 * r1 * s / (n as f64 + 3.0);
        for i in n..=np1 {
            let r = -0.5 * (i as f64 + 1.0);
            b0[1] = r * a0[1];
            for m in 2..=i {
                let mut bsum = 0.0;
                let mm1 = m - 1;
                for jj in 1..=mm1 {
                    let mmj = m - jj;
                    bsum += (jj as f64 * r - mmj as f64) * a0[jj] * b0[mmj];
                }
                b0[m] = r * a0[m] + bsum / m as f64;
            }
            c[i] = b0[i] / (i as f64 + 1.0);
            let mut dsum = 0.0;
            let im1 = i - 1;
            for jj in 1..=im1 {
                let imj = i - jj;
                dsum += d[imj] * c[jj];
            }
            d[i] = -(dsum + c[i]);
        }
        j0 = E1 * znm1 + (n as f64 - 1.0) * j0;
        j1 = E1 * zn + n as f64 * j1;
        znm1 = z2 * znm1;
        zn = z2 * zn;
        w = w0 * w;
        let t0 = d[n] * w * j0;
        w = w0 * w;
        let t1 = d[np1] * w * j1;
        summ += t0 + t1;
        if (t0.abs() + t1.abs()) <= eps * summ {
            break;
        }
        n += 2;
    }
    let bc = (stirling_error(a) + stirling_error(b)) - stirling_error(a + b);
    let u = excel_exp(-bc);
    E0 * t * u * summ
}

/// bratio: evaluation of the incomplete beta function Ix(a, b). Returns
/// (w, w1) = (Ix(a, b), 1 - Ix(a, b)). Domain errors (the Fortran ierr != 0
/// cases) return (NaN, NaN).
pub fn bratio(a: f64, b: f64, x: f64, y: f64) -> (f64, f64) {
    let eps0 = bratio_spmpar1();
    let mut w;
    let mut w1 = 0.0; // read as the seed of the bgrat_150 branch
    if a < 0.0 || b < 0.0 {
        return (f64::NAN, f64::NAN);
    }
    if a == 0.0 && b == 0.0 {
        return (f64::NAN, f64::NAN);
    }
    if x < 0.0 || x > 1.0 {
        return (f64::NAN, f64::NAN);
    }
    if y < 0.0 || y > 1.0 {
        return (f64::NAN, f64::NAN);
    }
    let z = ((x + y) - 0.5) - 0.5;
    if z.abs() > 3.0 * eps0 {
        return (f64::NAN, f64::NAN);
    }
    if x == 0.0 {
        if a == 0.0 {
            return (f64::NAN, f64::NAN);
        }
        return (0.0, 1.0);
    }
    if y == 0.0 {
        if b == 0.0 {
            return (f64::NAN, f64::NAN);
        }
        return (1.0, 0.0);
    }
    if a == 0.0 {
        return (1.0, 0.0);
    }
    if b == 0.0 {
        return (0.0, 1.0);
    }
    let eps = eps0.max(1.0e-15);
    // Excel 20527's half/half integral is the arcsine graph, with the
    // complementary coordinate at and above 0.9. Public controls also show that
    // this arm forms its own 1-x rather than retaining a wrapper's y.
    if a == 0.5 && b == 0.5 {
        use crate::ported::excel_numeric::{
            excel_sqrt_staged, excel_x87_div, excel_x87_mul, excel_x87_sub,
        };
        let coordinate = if x < 0.9 { x } else { excel_x87_sub(1.0, x) };
        let angle = crate::ported::asin::asin_kernel(excel_sqrt_staged(coordinate)).unwrap_or(f64::NAN);
        let small = excel_x87_div(excel_x87_mul(2.0, angle), std::f64::consts::PI);
        return if x < 0.9 {
            (small, 0.5 + (0.5 - small))
        } else {
            (0.5 + (0.5 - small), small)
        };
    }
    // At the remote end of each unit-shape arm Excel forms the small tail
    // with the corrected exponential, then reflects around the stored half.
    use crate::ported::excel_numeric::{
        excel_exp, excel_expm1_spilled, excel_log, excel_log1p_times, excel_x87_add as add,
        excel_x87_mul as mul, excel_x87_sub as sub,
    };
    if b == 1.0 {
        // Both sides consume the same stored a * LN(x). At x > 0.9 the
        // corrected exponential is published through the opposite tail.
        let exponent = mul(a, excel_log(x));
        if x <= 0.9 {
            let w = excel_exp(exponent);
            return (w, add(0.5, sub(0.5, w)));
        }
        let w1 = -excel_expm1_spilled(exponent);
        return (add(0.5, sub(0.5, w1)), w1);
    }
    if a == 1.0 {
        if x >= 0.1 {
            let w1 = excel_exp(mul(b, excel_log(y)));
            return (add(0.5, sub(0.5, w1)), w1);
        }
        let w = -excel_expm1_spilled(excel_log1p_times(b, -x));
        return (w, add(0.5, sub(0.5, w)));
    }
    if a.max(b) < 1.0e-3 * eps {
        return (b / (a + b), a / (a + b));
    }

    let mut ind = 0;
    let mut a0 = a;
    let mut b0 = b;
    let mut x0 = x;
    let mut y0 = y;
    if a0.min(b0) <= 1.0 {
        // procedure for a0 <= 1 or b0 <= 1
        if x > 0.5 {
            ind = 1;
            a0 = b;
            b0 = a;
            x0 = y;
            y0 = x;
        }
        let branch: &str;
        if a0.max(b0) > 1.0 {
            if b0 <= 1.0 {
                branch = "bpser";
            } else if x0 >= 0.3 {
                branch = "bpser_sym";
            } else if x0 < 0.1 {
                let raw_xb = mul(x0, b0);
                let xb = if raw_xb.is_subnormal() { 0.0 } else { raw_xb };
                if xb == 0.0 {
                    return (f64::NAN, f64::NAN);
                }
                if excel_exp(mul(a0, excel_log(xb))) <= 0.7 {
                    branch = "bpser";
                } else if b0 > 15.0 {
                    branch = "bgrat_150";
                } else {
                    branch = "bup_140";
                }
            } else if b0 > 15.0 {
                branch = "bgrat_150";
            } else {
                branch = "bup_140";
            }
        } else if a0 >= 0.2_f64.min(b0) {
            branch = "bpser";
        } else if crt_pow(x0, a0) <= 0.9 {
            branch = "bpser";
        } else if x0 >= 0.3 {
            branch = "bpser_sym";
        } else {
            branch = "bup_140";
        }

        match branch {
            "bpser" => {
                w = bratio_bpser(a0, b0, x0, true);
                w1 = 0.5 + (0.5 - w);
            }
            "bpser_sym" => {
                w1 = bratio_bpser(b0, a0, y0, true);
                w = 0.5 + (0.5 - w1);
            }
            "bup_140" => {
                let n = 20;
                w1 = bratio_bup(b0, a0, y0, x0, n, eps);
                b0 += n as f64;
                let (nw1, _ie) = bratio_bgrat(b0, a0, y0, x0, w1, eps);
                w1 = nw1;
                w = 0.5 + (0.5 - w1);
            }
            "bgrat_150" => {
                let (nw1, _ie) = bratio_bgrat(b0, a0, y0, x0, w1, eps);
                w1 = nw1;
                w = 0.5 + (0.5 - w1);
            }
            _ => unreachable!(),
        }
        if ind != 0 {
            std::mem::swap(&mut w, &mut w1);
        }
        return (w, w1);
    }

    // Both shapes exceed one. The stored PC64 products decide the side at
    // the mean and the power-series boundary at b*x = 0.7. Excel routes
    // min(a0, b0) > 10_000 and lambda <= 0.03 * min(a0, b0) to basym
    // (dc-iuwct.52); smaller shapes and farther tails use bfrac.
    let mut lambda_ = if a >= b {
        sub(mul(add(a, b), y), b)
    } else {
        sub(a, mul(add(a, b), x))
    };
    if lambda_ < 0.0 {
        ind = 1;
        a0 = b;
        b0 = a;
        x0 = y;
        y0 = x;
        lambda_ = lambda_.abs();
    }
    let min_ab = a0.min(b0);
    let branch: &str;
    if b0 < 40.0 && mul(b0, x0) <= 0.7 {
        branch = "bpser";
    } else if b0 < 40.0 {
        branch = "bup_160";
    } else if min_ab > 10_000.0 && lambda_ <= mul(0.03, min_ab) {
        branch = "basym";
    } else {
        branch = "bfrac";
    }

    match branch {
        "bpser" => {
            w = bratio_bpser(a0, b0, x0, true);
            w1 = 0.5 + (0.5 - w);
        }
        "basym" => {
            w = bratio_basym(a0, b0, lambda_, eps);
            w1 = 0.5 + (0.5 - w);
        }
        "bfrac" => {
            w = bratio_bfrac(a0, b0, x0, y0);
            w1 = 0.5 + (0.5 - w);
        }
        "bup_160" => {
            let mut n = bratio_ftoi(b0);
            b0 -= n as f64;
            if b0 == 0.0 {
                n -= 1;
                b0 = 1.0;
            }
            w = bratio_bup(b0, a0, y0, x0, n, eps);
            if x0 <= 0.7 {
                w += bratio_bpser(a0, b0, x0, false);
                // The series term can be subnormal and still contribute to a
                // normal recurrence. Excel flushes only the final stored sum.
                if w.is_subnormal() {
                    w = 0.0;
                }
                w1 = 0.5 + (0.5 - w);
            } else {
                if a0 <= 15.0 {
                    let nn = 20;
                    w += bratio_bup(a0, b0, x0, y0, nn, eps);
                    a0 += nn as f64;
                }
                let (nw, _ie) = bratio_bgrat(a0, b0, x0, y0, w, eps);
                w = nw;
                w1 = 0.5 + (0.5 - w);
            }
        }
        _ => unreachable!(),
    }
    if ind != 0 {
        std::mem::swap(&mut w, &mut w1);
    }
    (w, w1)
}

/// [`bratio`] for the F and t wrappers. With the first shape 1 (the arm `0.5 + (0.5 - exp(b
/// LN(y)))`) Excel takes the stored `1 - x` there, not the wrapper's own second argument:
/// FDIST(f, 7, 2) is that arm on 199 of 200 Excel 16.0.20527 calls with `1 - x` and on 166 with
/// `1 / (1 + d2 / (d1 f))` (dc-6332.8). F.TEST does not come through here: its bank rows hold the
/// pair it passes.
pub fn bratio_from_wrapper(a: f64, b: f64, x: f64, y: f64) -> (f64, f64) {
    if a == 1.0 {
        return bratio(a, b, x, crate::ported::excel_numeric::excel_x87_sub(1.0, x));
    }
    bratio(a, b, x, y)
}

pub fn regularized_beta(x: f64, a: f64, b: f64) -> f64 {
    if !(0.0..=1.0).contains(&x) || !(a > 0.0) || !(b > 0.0) {
        return f64::NAN;
    }
    if x == 0.0 {
        return 0.0;
    }
    if x == 1.0 {
        return 1.0;
    }
    let y = 1.0 - x;
    bratio(a, b, x, y).0
}

pub fn bisect_inverse<F>(target: f64, lo: f64, hi: f64, f: F) -> f64
where
    F: Fn(f64) -> f64,
{
    // Excel publishes fully-converged roots of its own forward (W109 agent-C
    // verdict; the early-stop 4*EPS relative cutoff produced up to +1.9M-ULP
    // errors at small roots on the b14 corpora). Bisect on the float lattice
    // until lo and hi are adjacent doubles, maintaining f(lo) < target <= f(hi),
    // and publish hi.
    let key = |x: f64| -> i64 {
        let i = x.to_bits() as i64;
        if i < 0 { i64::MIN.wrapping_sub(i) } else { i }
    };
    let unkey = |k: i64| -> f64 {
        if k < 0 {
            f64::from_bits(i64::MIN.wrapping_sub(k) as u64)
        } else {
            f64::from_bits(k as u64)
        }
    };
    let mut lok = key(lo) as i128;
    let mut hik = key(hi) as i128;
    while hik - lok > 1 {
        let midk = lok + (hik - lok) / 2;
        if f(unkey(midk as i64)) >= target {
            hik = midk;
        } else {
            lok = midk;
        }
    }
    unkey(hik as i64)
}

/// Invert a forward surface on the float lattice, publishing the adjacent
/// point with the smaller residual and keeping the upper point on a tie.
/// The bracketing and calculation remain the existing inverse route.
pub fn bisect_inverse_closest<F>(target: f64, lo: f64, hi: f64, f: F) -> f64
where
    F: Fn(f64) -> f64,
{
    let upper = bisect_inverse(target, lo, hi, &f);
    let lower = upper.next_down();
    if lower >= lo && (target - f(lower)).abs() < (f(upper) - target).abs() {
        lower
    } else {
        upper
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Excel 20527, ax3/evidence/dispatch-stores-answers.jsonl, row 512.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn pro10_bgrat_gamma_tail_matches_excel() {
        let value = crate::ported::beta_gamma_stats_family::beta_dist_kernel(
            f64::from_bits(0x3fa6fa5cf5fcc070),
            f64::from_bits(0x3ff8ff2b2536d599),
            f64::from_bits(0x40409fe32635df40),
            true,
            0.0,
            1.0,
        )
        .unwrap();
        assert_eq!(value.to_bits(), 0x3fe32d285762b933);
    }

    /// bgrat at `eps`: Excel build 20131 BETA.DIST rows (`cache/oracle/build-20131/
    /// BETA.DIST.jsonl` oc315, oc248, oc1424: a > 15, b <= 1, x > 0.5, the swapped bgrat arm),
    /// 31, 33 and 13 units low at TOMS 708's 15 * eps.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn bgrat_runs_at_eps_on_the_large_a_small_b_arm() {
        use crate::ported::beta_gamma_stats_family::beta_dist_kernel;
        let rows = [
            (0.9375, 20.45, 0.75, 0x3fc753be39b00d25_u64),
            (0.96875, 61.0, 0.2, 0x3f8d6072e16929ad),
            (0.984375, 320.0, 0.05, 0x3f1043f27e8b35ad),
        ];
        for (x, a, b, excel) in rows {
            let v = beta_dist_kernel(x, a, b, true, 0.0, 1.0).unwrap();
            // Exact before dc-6332.4's gamma-family bgrat pieces (which gained 75 bank rows and
            // lost 35, these three among them): now 3, 5 and 3 units off, against 31, 33 and 13
            // low at 15 * eps.
            let off = (v.to_bits() as i64 - excel as i64).abs();
            assert!(off <= 5, "BETA.DIST({x}, {a}, {b}, TRUE) is {off} units off");
        }
    }

    /// The power-series arm and the recurrence arm as read from Excel 16.0.20527 (dc-6332.8,
    /// `docs/research/beta-family-2026-11-12.md`): BETA.DIST(x, a, b, TRUE) answers of the Excel
    /// answer service on 2026-11-12. The first three rows run the series alone; the others add
    /// one or two recurrence terms built on the beta density.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn series_and_recurrence_arms_match_excel() {
        let rows = [
            (0.125, 3.0, 2.0, 0x3f7d000000000008_u64),
            (0.0625, 2.5, 3.5, 0x3f83647429738e30),
            (0.03125, 5.5, 2.5, 0x3e71e41a7f4fbc68),
            (0.5, 6.0, 2.0, 0x3fb0000000000000),
            (0.4375, 4.5, 3.0, 0x3fc71928b890bf88),
            (0.5, 4.0, 1.5, 0x3fbe6ed4f23ba3e3),
            (0.375, 5.5, 2.5, 0x3fa14e512187e2c5),
            (0.5, 4.0, 4.0, 0x3fe0000000000001),
            (0.625, 6.0, 3.0, 0x3fd7a9bd3fffffff),
        ];
        for (x, a, b, excel) in rows {
            let v = regularized_beta(x, a, b);
            assert_eq!(v.to_bits(), excel, "BETA.DIST({x}, {a}, {b}, TRUE)");
        }
    }

    /// Frozen live-Excel 16.0.20527.20196 exact-bit witnesses for the dc-j9k.58
    /// (Carbon cb5 + nx4) beta-family arms:
    /// - `bfrac` above `0.99` reflecting to `excel_brcomp(b, a, y, x)` with caller `y`
    /// - `basym` removed (large shapes near the mean routed through `bfrac`)
    /// - universal `bup` with `((x * y) / a) * density`, stored `exp(m * ln x)` for `b > 1`,
    ///   and extended-precision `x^m` / sum for `b <= 1`
    /// - `bgrat` unified `0.99` `lnx` switch, seeded `b == 0.5` `erfc`, `b == 1` `exp(-z)`,
    ///   and `z >= 1.1` `excel_gamma_fraction`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cb5_beta_arms_match_frozen_excel_bits() {
        use crate::ported::beta_gamma_stats_family::beta_dist_kernel;
        use crate::ported::chi_f_t_family::f_dist_rt_kernel;

        // BETA.DIST(x, a, b, TRUE) witnesses from frozen_nx4_predictions.jsonl
        // (SHA-256 57699658f78d61b7358ae434b78b2aa4e2c791a02659f4dc6acdcc565abafb98).
        let beta_cases: &[(&str, u64, u64, u64, u64)] = &[
            // bup b > 1 with ((x * y) / a) * density and universal stored powers (n >= 4)
            ("bup_gt1_fm1_1", 0x3fc94caa5b6132fc, 0x4031841205bc01a3, 0x403194538ef34d6a, 0x3f03cb415e5af338),
            ("bup_gt1_fm1_2", 0x3fe2e1fc694b97a3, 0x40396d8adab9f55a, 0x4029b844d013a92a, 0x3fc525a2730504eb),
            // bup b <= 1 twenty-step shift (Ext80 x^m and sum)
            ("bup_le1_tw_1", 0x3fe864b8183aadef, 0x402781d69a0ebe02, 0x3fd6666666666666, 0x3f7d868cc72f9a4d),
            ("bup_le1_tw_2", 0x3fe7e45940e913f1, 0x4021d3e8264db606, 0x3fe0000000000000, 0x3f98f2457fd8decd),
            ("bup_le1_tw_3", 0x3febc19db22115a5, 0x401af3e750349b38, 0x3fe4cccccccccccd, 0x3fce36c253e3a859),
            // basym removed: min(a, b) > 100, lam <= 0.03 * min routed to bfrac
            ("nobasym_bfrac", 0x3fe8b39c264e61b9, 0x408c13d0cc36028e, 0x407045afcd4cd9f0, 0x3fd8b59226eee2a7),
            // bgrat non-half: 0.99 lnx switch and shared gratio gamma tail
            ("bgrat_nonhalf_lt099", 0x3fef923d77e76095, 0x404b1ccadfc42d47, 0x3fea3d70a3d70a3d, 0x3fd9205960ff4c65),
            // bgrat b == 1.0 (via bup_160 integer reduction)
            ("bgrat_b1_1", 0x3fed3c81e194f9c8, 0x40510da80d0bfa8f, 0x4014000000000000, 0x3fcf00b7a8d17d09),
            ("bgrat_b1_2", 0x3fe9e364aa6c8848, 0x40579c5d96765bae, 0x4014000000000000, 0x3ee907eeb3e59914),
            // bgrat seeded b == 0.5 (w > 0 via bup_160 half-integer reduction)
            ("bgrat_half_seed_1", 0x3feb445227042fb4, 0x404eddd651bd2ee5, 0x401a000000000000, 0x3fb4ab5520d1e0c4),
            ("bgrat_half_seed_2", 0x3fe7c5b31fd87d38, 0x40406ac8529ebcb2, 0x4012000000000000, 0x3f8ea80116cb69fc),
        ];
        for &(label, xb, ab, bb, expected) in beta_cases {
            let actual = beta_dist_kernel(
                f64::from_bits(xb),
                f64::from_bits(ab),
                f64::from_bits(bb),
                true,
                0.0,
                1.0,
            )
            .unwrap()
            .to_bits();
            assert_eq!(
                actual, expected,
                "{label}: got {actual:#018x}, expected {expected:#018x}"
            );
        }
        // bgrat_nonhalf_gt099 (z = 0.7569 < 1.1): Excel 16.0.20527 returns 0x3fd860408858b4c4;
        // with bgrat in Ext80 and unified onto Excel's shared gratio (dc-i2wk3), the remaining
        // +1 ULP residual in gratio(0.82, 0.7569) (dc-j9k.5) is no longer masked by f64 bgrat
        // rounding (0x3fd860408858b4c5, +1 ULP).
        let gt099_actual = beta_dist_kernel(
            f64::from_bits(0x3fefd1fa4d71df13),
            f64::from_bits(0x4060ce516890686c),
            f64::from_bits(0x3fea3d70a3d70a3d),
            true,
            0.0,
            1.0,
        )
        .unwrap()
        .to_bits();
        assert!(
            gt099_actual.abs_diff(0x3fd860408858b4c4) <= 1,
            "bgrat_nonhalf_gt099: got {gt099_actual:#018x}, expected 0x3fd860408858b4c4"
        );

        // F.DIST.RT(f, d1, d2) hitting bfrac above 0.99 with caller complement y.
        let f_rt_cases: &[(&str, u64, u64, u64, u64)] = &[
            ("bfrac_gt099_f1", 0x3ffd36e5b980f016, 0x4060600000000000, 0x40e6c52000000000, 0x3e5c45ced9a87bfa),
            ("bfrac_gt099_f2", 0x400d5beee90bd5df, 0x4055c00000000000, 0x40e7b20000000000, 0x3a3f326b823acea1),
            ("bfrac_gt099_f3", 0x40001098953505b6, 0x405d000000000000, 0x40e70f8000000000, 0x3e0b7251aceef1b0),
        ];
        for &(label, fb, d1b, d2b, expected) in f_rt_cases {
            let actual = f_dist_rt_kernel(
                f64::from_bits(fb),
                f64::from_bits(d1b),
                f64::from_bits(d2b),
            )
            .unwrap()
            .to_bits();
            assert_eq!(
                actual, expected,
                "{label}: got {actual:#018x}, expected {expected:#018x}"
            );
        }
    }

    /// The half-integer finite sum with sqrt(pi) to binary64, against Excel build 20131's
    /// answers (`cache/oracle/build-20131/CHIDIST.jsonl` oc65, `GAMMA.DIST.jsonl` oc73 and oc75).
    /// With the 15-digit 1.77245385090552 they were 12, 6 and 4 units low.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn half_integer_finite_sum_uses_binary64_sqrt_pi() {
        use crate::ported::beta_gamma_stats_family::gamma_dist_kernel;
        use crate::ported::chi_f_t_family::chisq_dist_rt_kernel;
        assert_eq!(chisq_dist_rt_kernel(8.0, 5.0).unwrap().to_bits(), 0x3fc3ff876f754b0e);
        let p3 = gamma_dist_kernel(3.0, 2.5, 1.0, true).unwrap();
        assert_eq!(p3.to_bits(), 0x3fe6337461fff386);
        let p4 = gamma_dist_kernel(4.0, 2.5, 1.0, true).unwrap();
        assert_eq!(p4.to_bits(), 0x3feb001e2422ad3c);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn gamma_and_ln_gamma_match_known_values() {
        assert!((gamma(5.0) - 24.0).abs() < 1e-10);
        assert!((gamma(0.5) - std::f64::consts::PI.sqrt()).abs() < 1e-10);
        assert!((ln_gamma(5.0) - crt_log(24.0_f64)).abs() < 1e-10);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn regularized_gamma_matches_known_values() {
        assert!(regularized_gamma_p(0.5, 0.5).is_finite());
        assert!((regularized_gamma_p(2.0, 2.0) - 0.593_994_150_290_161_9).abs() < 1e-10);
        assert!((regularized_gamma_q(2.0, 2.0) - 0.406_005_849_709_838_1).abs() < 1e-10);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn regularized_beta_matches_known_values() {
        assert!((regularized_beta(0.5, 2.0, 2.0) - 0.5).abs() < 1e-12);
        assert!((regularized_beta(0.25, 2.0, 3.0) - 0.261_718_75).abs() < 1e-12);
    }

    fn ulp_gap(a: f64, x: f64) -> u64 {
        let (ab, bb) = (a.to_bits(), x.to_bits());
        if ab >= bb { ab - bb } else { bb - ab }
    }

    // Pinned live-Excel (16.0) witnesses proving the GRATIO port reproduces
    // Excel's incomplete-gamma-ratio kernel bit-for-bit. Both cases have a > x
    // (statement 20 -> gamma_nswc normalizer -> statement 50 Taylor tail), the
    // Q side being returned as 0.5 + (0.5 - P). Q(5, 0.5) and Q(1, 0.5) are the
    // internal arguments behind CHIDIST(1,10) and CHIDIST(1,2) respectively.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn gratio_pinned_live_excel_witnesses() {
        // CHIDIST(1,10) => Q(df/2=5, x/2=0.5).
        let (p5, q5) = gratio(5.0, 0.5);
        assert_eq!(
            q5.to_bits(),
            0x3fef_fe97_0c1f_f154,
            "Q(5,0.5) bits {:#018x} (gap {} ULP vs live Excel)",
            q5.to_bits(),
            ulp_gap(q5, f64::from_bits(0x3fef_fe97_0c1f_f154))
        );
        // CHIDIST(1,2) => Q(df/2=1, x/2=0.5).
        let (p1, q1) = gratio(1.0, 0.5);
        assert_eq!(
            q1.to_bits(),
            0x3fe3_68b2_fc6f_960a,
            "Q(1,0.5) bits {:#018x} (gap {} ULP vs live Excel)",
            q1.to_bits(),
            ulp_gap(q1, f64::from_bits(0x3fe3_68b2_fc6f_960a))
        );
        // The returned (P, Q) pair is complementary to within rounding.
        assert!((p5 + q5 - 1.0).abs() < 1e-15);
        assert!((p1 + q1 - 1.0).abs() < 1e-15);
    }

    // The public wrappers must preserve the previous kernel's invalid-input and
    // boundary contract (NAN for out-of-domain, 0/1 at x == 0).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn regularized_gamma_domain_contract_preserved() {
        assert!(regularized_gamma_p(-1.0, 1.0).is_nan());
        assert!(regularized_gamma_q(-1.0, 1.0).is_nan());
        assert!(regularized_gamma_p(1.0, -1.0).is_nan());
        assert!(regularized_gamma_q(1.0, -1.0).is_nan());
        assert!(regularized_gamma_p(f64::NAN, 1.0).is_nan());
        assert!(regularized_gamma_q(1.0, f64::INFINITY).is_nan());
        assert_eq!(regularized_gamma_p(2.5, 0.0), 0.0);
        assert_eq!(regularized_gamma_q(2.5, 0.0), 1.0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn gratio_temme_corpus_and_boundary_excel_bits() {
        // Corpus cumulative POISSON.DIST(k, x, TRUE) = Q(k+1, x) cases (dc96l3_discrete!D186
        // and dcj9k_r25_discrete!D147..D198), exact Excel 16.0.20527 bit patterns.
        let q_cases: &[(f64, u64, u64)] = &[
            // dc96l3_discrete!D186: POISSON.DIST(1000, 1000, TRUE)
            (1001.0, 1000.0_f64.to_bits(), 0x3fe0_44e3_b89f_0052),
            // dcj9k_r25_discrete!D147, D150, D153: k=33, x around 27
            (34.0, 0x403a_ffff_ffff_ffff, 0x3fec_89b0_efdf_3c2d),
            (34.0, 0x403b_0000_0000_0000, 0x3fec_89b0_efdf_3c2c),
            (34.0, 0x403b_0000_0000_0001, 0x3fec_89b0_efdf_3c2b),
            // dcj9k_r25_discrete!D156, D159, D162: k=33, x around 121/3
            (34.0, 0x4044_2aaa_aaaa_aaaa, 0x3fc1_e252_7475_4da2),
            (34.0, 0x4044_2aaa_aaaa_aaab, 0x3fc1_e252_7475_4d9a),
            (34.0, 0x4044_2aaa_aaaa_aaac, 0x3fc1_e252_7475_4d90),
            // dcj9k_r25_discrete!D168: k=100, x = 900/11
            (101.0, 0x4054_745d_1745_d174, 0x3fef_4a89_0c3f_40ee),
            // dcj9k_r25_discrete!D174, D177: k=100, x around 1100/9
            (101.0, 0x405e_8e38_e38e_38e3, 0x3f96_a06a_3c48_31bc),
            (101.0, 0x405e_8e38_e38e_38e4, 0x3f96_a06a_3c48_31ad),
            // dcj9k_r25_discrete!D183, D186, D189: k=1000, x around 9000/11
            (1001.0, 0x4089_9174_5d17_45d0, 0x3fef_ffff_ffce_c3ec),
            (1001.0, 0x4089_9174_5d17_45d1, 0x3fef_ffff_ffce_c3ec),
            (1001.0, 0x4089_9174_5d17_45d2, 0x3fef_ffff_ffce_c3ec),
            // dcj9k_r25_discrete!D192, D195, D198: k=1000, x around 11000/9
            (1001.0, 0x4093_18e3_8e38_e38d, 0x3dc0_583a_7310_66a0),
            (1001.0, 0x4093_18e3_8e38_e38e, 0x3dc0_583a_7310_65e7),
            (1001.0, 0x4093_18e3_8e38_e38f, 0x3dc0_583a_7310_651f),
            // x == a complement direction (Q = 1 - P, even LSB)
            (21.0, 21.0_f64.to_bits(), 0x3fde_2471_a884_3ce8),
            (101.0, 101.0_f64.to_bits(), 0x3fdf_2731_a5de_67a4),
            (1001.0, 1001.0_f64.to_bits(), 0x3fdf_bb22_bd73_2f30),
        ];
        for &(a, x_bits, expected) in q_cases {
            let x = f64::from_bits(x_bits);
            let actual = gratio(a, x).1.to_bits();
            assert_eq!(
                actual,
                expected,
                "Q({a}, {x_bits:#018x}): got {actual:#018x}, expected {expected:#018x} ({} ULP)",
                ulp_gap(f64::from_bits(actual), f64::from_bits(expected))
            );
        }

        // Exact a > 20.0 boundary at x == a (P side, GAMMA.DIST(a, a, 1, TRUE)):
        // a = 20.0 uses after40, next_up(20.0) enters Temme.
        assert_eq!(gratio(20.0, 20.0).0.to_bits(), 0x3fe0_f3a7_0847_030b);
        let a_next = f64::from_bits(0x4034_0000_0000_0001);
        assert_eq!(gratio(a_next, a_next).0.to_bits(), 0x3fe0_f3a7_0847_0306);

        // Large y > 709 inside Temme must stay finite and monotone (no exp(y)*0 NaN).
        let (p_hi, q_hi) = gratio(100_000.0, 113_000.0);
        assert_eq!(q_hi, 0.0);
        assert_eq!(p_hi, 1.0);
        let (p_lo, q_lo) = gratio(100_000.0, 88_000.0);
        assert_eq!(p_lo, 0.0);
        assert_eq!(q_lo, 1.0);
    }

    /// Excel 16.0.20527 (cb7 / dc-j9k.63): half-integer stored Horner (`GRATIO_HALF_RECIP`,
    /// `1 < a <= 20, a < x <= 30.0`) including pinned `CHIDIST(8, 5)` (`oc65`), x87-stored
    /// forward asymptotic expansion (`a > 20, x/a >= 1.3`), and frozen unseen holdouts.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cb7_gratio_half_integer_horner_and_forward_asymptotic_match_excel_bits() {
        let q_cases: &[(u64, u64, u64)] = &[
            // CHIDIST(8, 5) (oc65): Q(2.5, 4.0)
            (0x4004_0000_0000_0000, 0x4010_0000_0000_0000, 0x3fc3_ff87_6f75_4b0e),
            // nx6 unseen frozen half-integer CHIDIST holdouts (df = 25, 5, 19 -> a = 12.5, 2.5, 9.5; x = arg0 / 2)
            (0x4029_0000_0000_0000, 0x403d_90d4_8d5d_5ed6, 0x3f22_0869_b3ad_909a),
            (0x4004_0000_0000_0000, 0x4030_945a_95e9_7e53, 0x3ecd_5b10_a9b1_9b14),
            (0x4023_0000_0000_0000, 0x4037_eedb_7978_efd0, 0x3f31_93ad_bba5_c2d5),
            // nx6 unseen frozen forward asymptotic POISSON.DIST holdouts (a = k + 1 > 20, x/a >= 1.3)
            (0x405e_c000_0000_0000, 0x4066_598f_6245_c872, 0x3ed1_b9f2_916b_7c02),
            (0x4054_8000_0000_0000, 0x4064_0339_820c_e1f0, 0x3d90_10ff_3428_3db1),
            (0x4057_c000_0000_0000, 0x4060_ce4c_7038_ffd4, 0x3f22_de20_5e50_6850),
        ];
        for &(a_bits, x_bits, expected) in q_cases {
            let actual = gratio(f64::from_bits(a_bits), f64::from_bits(x_bits))
                .1
                .to_bits();
            assert_eq!(
                actual,
                expected,
                "Q({a_bits:#018x}, {x_bits:#018x}): got {actual:#018x}, expected {expected:#018x}"
            );
        }
    }

    /// Excel 16.0.20527 (cb8 / dc-j9k.64): unified Thompson-Barnett Modified-Lentz `gratio_cf`
    /// across all `a <= 20` upper-tail calls (`1 < a <= 20, x > a` outside the `a < x <= 30.0`
    /// finite sums, and `a < 1, x >= 1.1`), verified on Carbon's frozen cb8 holdouts.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cb8_gratio_cf_unified_matches_excel_bits() {
        // Upper tail Q(a, x): POISSON.DIST(k, x, TRUE) = Q(k + 1, x) and CHIDIST(x, df) = Q(df/2, x/2).
        let q_cases: &[(u64, u64, u64)] = &[
            // dyadic_pois_k5_9: POISSON.DIST(1, 32 + 1u/3u, TRUE) -> Q(2, x)
            (0x4000_0000_0000_0000, 0x4040_0000_0000_0001, 0x3d5d_6888_1f1c_6f57),
            (0x4000_0000_0000_0000, 0x4040_0000_0000_0003, 0x3d5d_6888_1f1c_6ede),
            // pois_cf_general_a_le_20: k = 18 (a = 19) and k = 2 (a = 3) with x > 30
            (0x4033_0000_0000_0000, 0x403e_e4fc_21b0_8a53, 0x3f81_c96f_3e33_ccff),
            (0x4008_0000_0000_0000, 0x403e_c677_29b8_7d8e, 0x3db7_f279_6108_d23e),
            // chi_and_gamma_cf_1_to_20: CHIDIST(x, 24) -> Q(12, x/2) and CHIDIST(x, 4) -> Q(2, x/2)
            (0x4028_0000_0000_0000, 0x403e_22f4_2b5c_e4b9, 0x3f0e_a464_814b_b970),
            (0x4000_0000_0000_0000, 0x403e_96c4_9fac_7117, 0x3d7c_db56_58e4_5eb9),
        ];
        for &(a_bits, x_bits, expected) in q_cases {
            let actual = gratio(f64::from_bits(a_bits), f64::from_bits(x_bits))
                .1
                .to_bits();
            assert_eq!(
                actual,
                expected,
                "cb8 Q({a_bits:#018x}, {x_bits:#018x}): got {actual:#018x}, expected {expected:#018x}"
            );
        }
        // alt1_cf_x_ge_1p1: GAMMA.DIST(x, a, beta, TRUE) with a < 1 and x/beta >= 1.1.
        let p_cases: &[(u64, u64, u64, u64)] = &[
            (0x400f_15f1_f172_f422, 0x3fd7_3c49_dd27_9384, 0x400b_3a5d_dccf_779a, 0x3fed_3632_e959_7e9a),
            (0x3f9a_d9cf_49f0_a809, 0x3fc1_daff_f1d5_ace0, 0x3f97_7f3d_3dc6_7f5f, 0x3fef_1b32_d9da_beb6),
        ];
        for &(x_bits, a_bits, b_bits, expected) in p_cases {
            let got = crate::ported::beta_gamma_stats_family::gamma_dist_kernel(
                f64::from_bits(x_bits),
                f64::from_bits(a_bits),
                f64::from_bits(b_bits),
                true,
            )
            .unwrap()
            .to_bits();
            assert_eq!(
                got, expected,
                "cb8 GAMMA.DIST({x_bits:#018x}, {a_bits:#018x}, {b_bits:#018x}, TRUE)"
            );
        }
    }

    /// Excel 16.0.20527 (cb11 / dc-j9k.67): TOMS 654 `iop = 1` double-precision `st200` branch
    /// (`if x <= 0.5 { z >= -LN2 } else { a - x < 0.25 }`), PC64 `excel_x87_*` + `excel_log(x)`
    /// staging, and `excel_expm1_internal(z)` in `st200` (replacing NSWC `gratio_rexp`).
    /// Verified against Carbon cb11 probe3 straddles and nx9 frozen probe1
    /// (`SHA-256 016793aefa18576c87dd5ac5807d36aebfec845bd138a977b0b51a64aab940ea`).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cb11_gratio_alt1_iop1_st200_and_expm1_match_excel_bits() {
        let p_cases: &[(&str, u64, u64, u64)] = &[
            // cb11 P3_go200_x05_seam (a = 0.8125): x <= 0.5 stays in st200 with excel_expm1_internal
            ("x05_seam_m5", 0x3fea_0000_0000_0000, 0x3fdf_ffff_ffff_fffb, 0x3fdf_7f43_b8ba_f0bc),
            ("x05_seam_0", 0x3fea_0000_0000_0000, 0x3fe0_0000_0000_0000, 0x3fdf_7f43_b8ba_f0bc),
            // cb11 P3_go200_a075_zln2_bit (a = 0.75): exact -2u Kahan expm1 step at z = -ln(2)+1u
            ("zln2_kahan_step", 0x3fe8_0000_0000_0000, 0x3fd9_65fe_a53d_6e3e, 0x3fdd_93aa_2ee8_b43a),
            ("zln2_next", 0x3fe8_0000_0000_0000, 0x3fd9_65fe_a53d_6e3f, 0x3fdd_93aa_2ee8_b43e),
            // cb11 P3_go200_a0875_slope (a = 0.875, x = 0.6255 -> a - x < 0.25 -> st200)
            ("a0875_slope", 0x3fec_0000_0000_0000, 0x3fe4_0418_9374_bc6a, 0x3fe0_e820_2515_38fc),
            // nx9 probe1 A_rexp_3way_gt015: excel_expm1_internal vs gratio_rexp and CR expm1
            ("nx9_gt015_a0875", 0x3fec_0000_0000_0000, 0x3fdd_16c9_b22d_9ec9, 0x3fdb_7a43_42d0_744a),
            ("nx9_gt015_a075", 0x3fe8_0000_0000_0000, 0x3fd9_81bf_5eb9_8f50, 0x3fdd_a6d3_6550_5484),
            // nx9 probe1 B_rexp_poly_vs_expm1_le015: |z| <= 0.15 uses excel_expm1_internal
            ("nx9_le015_a0875_1", 0x3fec_0000_0000_0000, 0x3fea_ff59_0668_f14f, 0x3fe4_1c79_1532_cf68),
            ("nx9_le015_a0875_2", 0x3fec_0000_0000_0000, 0x3feb_0af6_6509_62a9, 0x3fe4_2127_0576_1661),
            ("nx9_le015_a075", 0x3fe8_0000_0000_0000, 0x3fea_3c8b_59cd_8561, 0x3fe5_bc5d_d1ae_7f9d),
        ];
        for &(label, a_bits, x_bits, expected) in p_cases {
            let actual = gratio(f64::from_bits(a_bits), f64::from_bits(x_bits))
                .0
                .to_bits();
            assert_eq!(
                actual, expected,
                "cb11 {label} P({a_bits:#018x}, {x_bits:#018x}): got {actual:#018x}, expected {expected:#018x}"
            );
        }
    }

    /// Excel 16.0.20527 (dc-j9k.45.1 / nx13 Touchstone probes `probe_44_45_edge`, `edge2`,
    /// `edge3`, `probe_45_1_dens`, `probe_45_1_wide`):
    /// In `bratio_bup`, when `scale = (x * y) / a >= 1` (`a = b0 < x * y <= 0.25`), `first = scale * density`
    /// can be normal (up to ~250 * DBL_MIN) while `density` is still subnormal. Excel flushes the
    /// subnormal `density` to `0.0` before multiplying by `scale`, verified to 1 ULP across 9
    /// independent `(a, b, x)` transitions spanning `b0 in {0.001, 0.01, 0.2}`, `n in {2, 3, 4}`,
    /// and `x in [0.35, 0.69]`, alongside the `scale < 1` (`b0 = 0.5`) `first.is_subnormal()` rule.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_j9k_45_1_bup_flushes_subnormal_density_and_first_term() {
        use crate::ported::beta_gamma_stats_family::{beta_dist_kernel, flush_subnormal};
        use crate::ported::chi_f_t_family::f_dist_rt_kernel;

        // 9 exact 1-ULP adjacent-double transitions (x_zero -> 0, x_nz -> nonzero) on Excel 16.0.20527
        // where scale = x*y/b0 > 1 and density crosses DBL_MIN while first = scale*density is normal.
        // Note a1015_b2.001 specifically separates density.is_subnormal() from exp(stored).is_subnormal():
        // at 0x3fe002acc4bc5036, exp(stored) = 0x001000000000007c (normal) while density = 0x000fffffffffff9e (subnormal).
        let pairs: &[(&str, f64, f64, u64, u64, u64)] = &[
            ("a1020_b2.001", 1020.0, 2.001, 0x3fe0109c9a3853c6, 0x3fe0109c9a3853c7, 0x011f08016e795101),
            ("a1015_b2.001", 1015.0, 2.001, 0x3fe002acc4bc5036, 0x3fe002acc4bc5037, 0x011efc38cea84c8a),
            ("a1005_b2.001", 1005.0, 2.001, 0x3fdfcd0f52de5e59, 0x3fdfcd0f52de5e5a, 0x011ee3d200165e6a),
            ("a1020_b3.01",  1020.0, 3.01,  0x3fe00717925e83af, 0x3fe00717925e83b0, 0x0168792da3db6a99),
            ("a1018_b3.01",  1018.0, 3.01,  0x3fe00185b7079e0d, 0x3fe00185b7079e0e, 0x016871b0ec1215b2),
            ("a1020_b2.2",   1020.0, 2.2,   0x3fdfebfac8772e79, 0x3fdfebfac8772e7a, 0x00a0af334801317c),
            ("a675_b2.01",    675.0, 2.01,  0x3fd6827366ccdcae, 0x3fd6827366ccdcaf, 0x00e356e3a9fc719f),
            ("a1900_b2.01",  1900.0, 2.01,  0x3fe613582f24c1d3, 0x3fe613582f24c1d4, 0x00e86c0a67a73c1d),
            ("a1020_b4.001", 1020.0, 4.001, 0x3fe0109c9a3853c4, 0x3fe0109c9a3853c5, 0x02147bc1ec3bf9bf),
        ];

        for &(label, a, b, x_zr, x_nz, ex_nz) in pairs {
            let got_zr = flush_subnormal(
                beta_dist_kernel(f64::from_bits(x_zr), a, b, true, 0.0, 1.0).unwrap(),
            )
            .to_bits();
            assert_eq!(
                got_zr, 0,
                "{label} zero-side ({x_zr:#018x}): got {got_zr:#018x}, expected 0"
            );
            let got_nz = flush_subnormal(
                beta_dist_kernel(f64::from_bits(x_nz), a, b, true, 0.0, 1.0).unwrap(),
            )
            .to_bits();
            assert!(
                got_nz != 0 && got_nz.abs_diff(ex_nz) < 0x1000,
                "{label} nonzero-side ({x_nz:#018x}): got {got_nz:#018x}, expected ~{ex_nz:#018x}"
            );
        }

        // Deeper subnormal density (0.1 * DBL_MIN) with normal first term (~25 * DBL_MIN) flushes to 0.
        assert_eq!(
            flush_subnormal(
                beta_dist_kernel(f64::from_bits(0x3fe0075699ca4958), 1020.0, 2.001, true, 0.0, 1.0)
                    .unwrap()
            )
            .to_bits(),
            0
        );

        // When scale < 1 (b0 = 0.5), first = scale * density crosses DBL_MIN before density does
        // (probe_44_45_edge3: dens=1.5*DBL_MIN -> first=0.75*DBL_MIN -> 0; dens=2.5*DBL_MIN -> 0x009ac488062b5001).
        assert_eq!(
            flush_subnormal(
                beta_dist_kernel(f64::from_bits(0x3fdfd8a06c843d1f), 1020.0, 2.5, true, 0.0, 1.0)
                    .unwrap()
            )
            .to_bits(),
            0
        );
        let got_b25_nz = flush_subnormal(
            beta_dist_kernel(f64::from_bits(0x3fdfdcb66dc2fb20), 1020.0, 2.5, true, 0.0, 1.0)
                .unwrap(),
        )
        .to_bits();
        assert!(
            got_b25_nz != 0 && got_b25_nz.abs_diff(0x009ac488062b5001) < 0x1000,
            "b=2.5 first>DBL_MIN: got {got_b25_nz:#018x}, expected ~0x009ac488062b5001"
        );

        // Original dc-j9k.45.1 F.DIST.RT controls (f-tail-zero-controls[21-23, 69-71]) stay 0.
        for &(x_bits, d1, d2) in &[
            (0x4063d510b1fba2fe_u64, 20.0_f64, 1000.0_f64),
            (0x4063da231f774b78, 20.0, 1000.0),
            (0x406407d4ab62a10e, 20.0, 1000.0),
            (0x408086e33efc5d29, 6.0, 1000.0),
            (0x40808b1d44e3698e, 6.0, 1000.0),
            (0x4080b131397cdb8b, 6.0, 1000.0),
        ] {
            assert_eq!(f_dist_rt_kernel(f64::from_bits(x_bits), d1, d2).unwrap().to_bits(), 0);
        }
    }
}
