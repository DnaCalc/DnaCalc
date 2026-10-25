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
        return std::f64::consts::PI.ln()
            - (std::f64::consts::PI * z).sin().ln()
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
    0.5 * (2.0 * std::f64::consts::PI).ln() + (z - 0.5) * t.ln() - t + x.ln()
}

pub fn gamma(z: f64) -> f64 {
    if z.is_sign_negative() && z.fract() == 0.0 {
        return f64::NAN;
    }
    if z < 0.5 {
        return std::f64::consts::PI / ((std::f64::consts::PI * z).sin() * gamma(1.0 - z));
    }
    crate::ported::excel_numeric::excel_exp(ln_gamma(z))
}

// ---------------------------------------------------------------------------
// GRATIO: faithful port of the DCDFLIB / NSWC incomplete-gamma-ratio kernel
// (TOMS 654, DiDonato & Morris; Alfred H. Morris Jr.), transcribed from the
// scipy v0.14 cdflib Fortran sources via the validated Python reference at
// smart-fuzzer/work/w109/G3-01-dist/cdflib_py.py.
//
// Live-Excel identification (W109) proved this is Excel's actual algorithm for
// the incomplete-gamma family (CHIDIST / GAMMA.DIST / GAMMADIST / CHISQ.*),
// executed in plain SSE2 double with std log/exp. This kernel replaces the
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
//      erf implementation. (No witness in the identification corpus reaches the
//      Temme branch; it is ported for structural fidelity.)
// ---------------------------------------------------------------------------

const GRATIO_ALOG10: f64 = 2.30258509299405;
const GRATIO_RT2PIN: f64 = 0.398942280401433;
const GRATIO_RTPI: f64 = 1.77245385090552;
const GRATIO_THIRD: f64 = 0.333333333333333;
const GRATIO_E: f64 = 2.220446049250313e-16; // spmpar(1) = 2^-52
const GRATIO_SPMPAR3: f64 = 1.7976931348623157e308;

// Temme expansion coefficient tables (d0..d7 of gratio.f).
const D0: [f64; 13] = [
    0.833333333333333e-01,
    -0.148148148148148e-01,
    0.115740740740741e-02,
    0.352733686067019e-03,
    -0.178755144032922e-03,
    0.391926317852244e-04,
    -0.218544851067999e-05,
    -0.185406221071516e-05,
    0.829671134095309e-06,
    -0.176659527368261e-06,
    0.670785354340150e-08,
    0.102618097842403e-07,
    -0.438203601845335e-08,
];
const D10: f64 = -0.185185185185185e-02;
const D1: [f64; 12] = [
    -0.347222222222222e-02,
    0.264550264550265e-02,
    -0.990226337448560e-03,
    0.205761316872428e-03,
    -0.401877572016461e-06,
    -0.180985503344900e-04,
    0.764916091608111e-05,
    -0.161209008945634e-05,
    0.464712780280743e-08,
    0.137863344691572e-06,
    -0.575254560351770e-07,
    0.119516285997781e-07,
];
const D20: f64 = 0.413359788359788e-02;
const D2: [f64; 10] = [
    -0.268132716049383e-02,
    0.771604938271605e-03,
    0.200938786008230e-05,
    -0.107366532263652e-03,
    0.529234488291201e-04,
    -0.127606351886187e-04,
    0.342357873409614e-07,
    0.137219573090629e-05,
    -0.629899213838006e-06,
    0.142806142060642e-06,
];
const D30: f64 = 0.649434156378601e-03;
const D3: [f64; 8] = [
    0.229472093621399e-03,
    -0.469189494395256e-03,
    0.267720632062839e-03,
    -0.756180167188398e-04,
    -0.239650511386730e-06,
    0.110826541153473e-04,
    -0.567495282699160e-05,
    0.142309007324359e-05,
];
const D40: f64 = -0.861888290916712e-03;
const D4: [f64; 6] = [
    0.784039221720067e-03,
    -0.299072480303190e-03,
    -0.146384525788434e-05,
    0.664149821546512e-04,
    -0.396836504717943e-04,
    0.113757269706784e-04,
];
const D50: f64 = -0.336798553366358e-03;
const D5: [f64; 4] = [
    -0.697281375836586e-04,
    0.277275324495939e-03,
    -0.199325705161888e-03,
    0.679778047793721e-04,
];
const D60: f64 = 0.531307936463992e-03;
const D6: [f64; 2] = [-0.592166437353694e-03, 0.270878209671804e-03];
const D70: f64 = 0.344367606892378e-03;

// gratio.f iop-selected constant vectors, indexed [iop-1]. Excel uses ind = 0
// (unscaled) exclusively, hence iop = 1 throughout; the higher-iop entries are
// retained so the branch routing matches the reference exactly.
const GRATIO_ACC0: [f64; 3] = [5e-15, 5e-7, 5e-4];
const GRATIO_BIG: [f64; 3] = [20.0, 14.0, 10.0];
const GRATIO_E00: [f64; 3] = [0.25e-3, 0.25e-1, 0.14];
const GRATIO_X00: [f64; 3] = [31.0, 17.0, 9.7];

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
    crate::ported::excel_numeric::excel_exp(ln_gamma(a))
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

/// rexp.f: evaluation of exp(x) - 1.
fn gratio_rexp(x: f64) -> f64 {
    const P1: f64 = 0.914041914819518e-09;
    const P2: f64 = 0.238082361044469e-01;
    const Q1: f64 = -0.499999999085958e+00;
    const Q2: f64 = 0.107141568980644e+00;
    const Q3: f64 = -0.119041179760821e-01;
    const Q4: f64 = 0.595130811860248e-03;
    if x.abs() <= 0.15 {
        return x * (((P2 * x + P1) * x + 1.0) / ((((Q4 * x + Q3) * x + Q2) * x + Q1) * x + 1.0));
    }
    let w = crate::ported::excel_numeric::excel_exp(x);
    if x <= 0.0 {
        (w - 0.5) - 0.5
    } else {
        w * (0.5 + (0.5 - 1.0 / w))
    }
}

/// rlog.f: computation of x - 1 - ln(x).
fn gratio_rlog(x: f64) -> f64 {
    const A: f64 = 0.566749439387324e-01;
    const B: f64 = 0.456512608815524e-01;
    const P0: f64 = 0.333333333333333e+00;
    const P1: f64 = -0.224696413112536e+00;
    const P2: f64 = 0.620886815375787e-02;
    const Q1: f64 = -0.127408923933623e+01;
    const Q2: f64 = 0.354508718369557e+00;
    if x < 0.61 || x > 1.57 {
        let r = (x - 0.5) - 0.5;
        return r - x.ln();
    }
    let u;
    let w1;
    if x < 0.82 {
        let uu = (x - 0.7) / 0.7;
        u = uu;
        w1 = A - uu * 0.3;
    } else if x > 1.18 {
        let uu = 0.75 * x - 1.0;
        u = uu;
        w1 = B + uu / 3.0;
    } else {
        u = (x - 0.5) - 0.5;
        w1 = 0.0;
    }
    let r = u / (u + 2.0);
    let t = r * r;
    let w = ((P2 * t + P1) * t + P0) / ((Q2 * t + Q1) * t + 1.0);
    2.0 * t * (1.0 / (1.0 - r) - r * w) + w1
}

/// gam1.f: computation of 1/gamma(a+1) - 1 for -0.5 <= a <= 1.5.
fn gratio_gam1(a: f64) -> f64 {
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

// ---- exp_rd: round-toward-zero exp (W109 chopped-exp identification) -------
// Excel's gser-path r = exp(t1)/G publishes a TRUNCATED exp, not nearest:
// floor(true exp) scores 38/45 on the implied-exp corpus vs CR 25/45 and
// fdlibm 28/45 (every real 2010-era MSVC CRT exp refuted by direct binary
// probes: 32-bit SSE2 rounds one-sided HIGH, x87 and x64-AMD are CR here).
// Corpus effect at the series call site: +26/306 training, +3/111 held-out
// b20 (fractional-a normalizer noise caps the held-out margin).
// Implementation: the x87 fFEXP chain with a chopping binary64 store (dc-8dg:
// one chain on every target; off x86_64 the software x87 runs it).

pub fn exp_rd(x: f64) -> f64 {
    // W109 F2XM1 identification: the series-site chop is RZ53 of the x87 fFEXP
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
        s = (PI * tt).sin() / PI;
        if n % 2 == 0 {
            s = -s;
        }
        if s == 0.0 {
            return 0.0;
        }
    }
    let t = 1.0 / (x * x);
    let mut g = ((((R1 * t + R2) * t + R3) * t + R4) * t + R5) / x;
    let lnx = x.ln();
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
    if a * x == 0.0 {
        return if x <= a { (0.0, 1.0) } else { (1.0, 0.0) };
    }
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
    // Excel uses ind = 0 (unscaled). gratio.f: iop = ind + 1 = 1 (only ind of
    // 1 or 2 would remap to iop = 3), selecting the tight acc = 5e-15, big = 20,
    // x0 = 31, e0 = 0.25e-3 constant set.
    let iop = 1usize;
    let acc = GRATIO_ACC0[iop - 1].max(e);
    let e0 = GRATIO_E00[iop - 1];
    let x0 = GRATIO_X00[iop - 1];

    let mut wk = [0.0f64; 21]; // 1-based, indices 1..=20

    if a < 1.0 {
        // DEVIATION 1: no a == 0.5 special dispatch; 0.5 flows into these
        // general a < 1 branches.
        if x < 1.1 {
            // statement 160: Taylor series for small a.
            let mut an = 3.0;
            let mut c = x;
            let mut summ = x / (a + 3.0);
            let tol = 3.0 * acc / (a + 1.0);
            loop {
                an += 1.0;
                c = -c * (x / an);
                let t = c / (a + an);
                summ += t;
                if t.abs() <= tol {
                    break;
                }
            }
            let j = a * x * ((summ / 6.0 - 0.5 / (a + 2.0)) * x + 1.0 / (a + 1.0));
            let z = a * x.ln();
            let h = gratio_gam1(a);
            let g = 1.0 + h;
            let go200 = if x < 0.25 { z > -0.13394 } else { a < x / 2.59 };
            if go200 {
                // statement 200
                let l = gratio_rexp(z);
                let w = 0.5 + (0.5 + l);
                let qans = (w * j - l) * g - h;
                if qans < 0.0 {
                    return (1.0, 0.0);
                }
                return (0.5 + (0.5 - qans), qans);
            }
            // statement 190
            let w = crate::ported::excel_numeric::excel_exp(z);
            let ans = w * g * (0.5 + (0.5 - j));
            return (ans, 0.5 + (0.5 - ans));
        }
        // a < 1, x >= 1.1
        let t1 = a * x.ln() - x;
        let u = a * crate::ported::excel_numeric::excel_exp(t1);
        if u == 0.0 {
            return (1.0, 0.0);
        }
        let r = u * (1.0 + gratio_gam1(a));
        return gratio_cf(a, x, r, e, acc);
    }

    // a >= 1
    if a < GRATIO_BIG[iop - 1] {
        if a > x || x >= x0 {
            // statement 20. Series arm publishes r from the CHOPPED exp
            // (W109 chopped-exp identification); CF/asymptotic arms keep the
            // nearest-rounded exp (chop measurably hurts there).
            let t1 = a * x.ln() - x;
            let g = gratio_norm_gamma(a);
            return gratio_after40(
                a,
                x,
                exp_rd(t1) / g,
                crate::ported::excel_numeric::excel_exp(t1) / g,
                e,
                acc,
                x0,
                &mut wk,
            );
        }
        let twoa = a + a;
        let m = twoa as i64;
        if twoa == m as f64 {
            let i = m / 2;
            let mut summ;
            let mut t;
            let mut n;
            let mut c;
            if a == i as f64 {
                // statement 210: integer a, finite sum
                summ = crate::ported::excel_numeric::excel_exp(-x);
                t = summ;
                n = 1i64;
                c = 0.0;
            } else {
                // statement 220: half-integer a, finite sum.
                // DEVIATION 2: erfc1(0, sqrt(x)) == Q(0.5, x) == gratio(0.5, x).1
                let rtx = x.sqrt();
                summ = gratio(0.5, x).1;
                t = crate::ported::excel_numeric::excel_exp(-x) / (GRATIO_RTPI * rtx);
                n = 0i64;
                c = -0.5;
            }
            while n != i {
                n += 1;
                c += 1.0;
                t = (x * t) / c;
                summ += t;
            }
            let qans = summ;
            return (0.5 + (0.5 - qans), qans);
        }
        let t1 = a * x.ln() - x;
        let g = gratio_norm_gamma(a);
        return gratio_after40(
            a,
            x,
            exp_rd(t1) / g,
            crate::ported::excel_numeric::excel_exp(t1) / g,
            e,
            acc,
            x0,
            &mut wk,
        );
    }

    // statement 30: a >= big
    let l = x / a;
    if l == 0.0 {
        return (0.0, 1.0);
    }
    let s = 0.5 + (0.5 - l);
    let z = gratio_rlog(l);
    if z >= 700.0 / a {
        if s.abs() <= 2.0 * e {
            return (2.0, 2.0);
        }
        return if x <= a { (0.0, 1.0) } else { (1.0, 0.0) };
    }
    let y = a * z;
    let rta = a.sqrt();
    if s.abs() <= e0 / rta {
        return gratio_temme_330(a, l, y, z, e, s, iop);
    }
    if s.abs() <= 0.4 {
        return gratio_temme_270(a, l, y, z, e, s, iop);
    }
    let t = (1.0 / a).powi(2);
    let mut t1 = (((0.75 * t - 1.0) * t + 3.5) * t - 105.0) / (a * 1260.0);
    t1 -= y;
    let r = GRATIO_RT2PIN * rta * crate::ported::excel_numeric::excel_exp(t1);
    // a >= big: no chopped-exp evidence for this r staging; same r both arms.
    gratio_after40(a, x, r, r, e, acc, x0, &mut wk)
}

fn gratio_after40(
    a: f64,
    x: f64,
    r_series: f64,
    r: f64,
    _e: f64,
    acc: f64,
    x0: f64,
    wk: &mut [f64; 21],
) -> (f64, f64) {
    if r == 0.0 {
        return if x <= a { (0.0, 1.0) } else { (1.0, 0.0) };
    }
    if x <= a.max(GRATIO_ALOG10) {
        // Taylor series. Identification (W109 agent-B, a=2 slice): Excel sums
        // FORWARD (1 + c1 + c2 + ...) with 1/a as an OUTER factor — not the
        // NSWC wk[] backward-tail staging (28/45 vs 16/45 bit-exact).
        // Refined (session 4): the exact form is Cephes-igam style — ans
        // accumulates FROM 1.0, stop at c/ans <= MACHEP (2^-53), publication
        // (r/a)*ans. 25/45 at a=2 with CR exp (vs 16/45 for 1+summ staging);
        // the residual is Excel's statically-linked legacy-CRT exp (one-sided
        // low, fdlibm-proxy 28/45).
        let _ = acc;
        let mut rr = a;
        let mut c = 1.0f64;
        let mut ans = 1.0f64;
        loop {
            rr += 1.0;
            c *= x / rr;
            ans += c;
            if c / ans <= 1.110_223_024_625_156_5e-16 {
                break;
            }
        }
        let p = (r_series / a) * ans;
        return (p, 0.5 + (0.5 - p));
    }
    if x < x0 {
        return gratio_cf(a, x, r, _e, acc);
    }
    // statement 100: asymptotic expansion with wk backward-tail summation.
    let mut amn = a - 1.0;
    let mut t = amn / x;
    wk[1] = t;
    let mut n = 20usize;
    let mut broke = false;
    for n_ in 2..=20 {
        amn -= 1.0;
        t *= amn / x;
        if t.abs() <= 1e-3 {
            n = n_;
            broke = true;
            break;
        }
        wk[n_] = t;
    }
    if !broke {
        n = 20;
    }
    let mut summ = t;
    while t.abs() > acc {
        amn -= 1.0;
        t *= amn / x;
        summ += t;
    }
    let mx = n - 1;
    for _ in 0..mx {
        n -= 1;
        summ += wk[n];
    }
    let qans = (r / x) * (1.0 + summ);
    (0.5 + (0.5 - qans), qans)
}

/// statement 250: unnormalized continued fraction (two-term recurrences).
fn gratio_cf(a: f64, x: f64, r: f64, e: f64, acc: f64) -> (f64, f64) {
    let tol = (5.0 * e).max(acc);
    let mut a2nm1 = 1.0;
    let mut a2n = 1.0;
    let mut b2nm1 = x;
    let mut b2n = x + (1.0 - a);
    let mut c = 1.0;
    let mut an0;
    loop {
        a2nm1 = x * a2n + c * a2nm1;
        b2nm1 = x * b2n + c * b2nm1;
        let am0 = a2nm1 / b2nm1;
        c += 1.0;
        let cma = c - a;
        a2n = a2nm1 + cma * a2n;
        b2n = b2nm1 + cma * b2n;
        an0 = a2n / b2n;
        if !((an0 - am0).abs() >= tol * an0) {
            break;
        }
    }
    let qans = r * an0;
    (0.5 + (0.5 - qans), qans)
}

/// statement 270: Temme expansion for a >= big, |1 - x/a| <= 0.4.
fn gratio_temme_270(a: f64, l: f64, y: f64, z: f64, e: f64, s: f64, iop: usize) -> (f64, f64) {
    if s.abs() <= 2.0 * e && a * e * e > 3.28e-3 {
        return (2.0, 2.0);
    }
    let c = crate::ported::excel_numeric::excel_exp(-y);
    // DEVIATION 3: w = 0.5*erfc1(1, sqrt(y)) = 0.5*exp(y)*Q(0.5, y).
    let w = 0.5 * (crate::ported::excel_numeric::excel_exp(y) * gratio(0.5, y).1);
    let u = 1.0 / a;
    let mut zz = (z + z).sqrt();
    if l < 1.0 {
        zz = -zz;
    }
    let z = zz;
    let t;
    if iop == 1 {
        if s.abs() <= 1e-3 {
            return gratio_temme_340(a, l, z, u, c, w);
        }
        let c0 = ((((((((((((D0[12] * z + D0[11]) * z + D0[10]) * z + D0[9]) * z + D0[8])
            * z
            + D0[7])
            * z
            + D0[6])
            * z
            + D0[5])
            * z
            + D0[4])
            * z
            + D0[3])
            * z
            + D0[2])
            * z
            + D0[1])
            * z
            + D0[0])
            * z
            - GRATIO_THIRD;
        let c1 = (((((((((((D1[11] * z + D1[10]) * z + D1[9]) * z + D1[8]) * z + D1[7]) * z
            + D1[6])
            * z
            + D1[5])
            * z
            + D1[4])
            * z
            + D1[3])
            * z
            + D1[2])
            * z
            + D1[1])
            * z
            + D1[0])
            * z
            + D10;
        let c2 =
            (((((((((D2[9] * z + D2[8]) * z + D2[7]) * z + D2[6]) * z + D2[5]) * z + D2[4]) * z
                + D2[3])
                * z
                + D2[2])
                * z
                + D2[1])
                * z
                + D2[0])
                * z
                + D20;
        let c3 = (((((((D3[7] * z + D3[6]) * z + D3[5]) * z + D3[4]) * z + D3[3]) * z + D3[2])
            * z
            + D3[1])
            * z
            + D3[0])
            * z
            + D30;
        let c4 =
            (((((D4[5] * z + D4[4]) * z + D4[3]) * z + D4[2]) * z + D4[1]) * z + D4[0]) * z + D40;
        let c5 = (((D5[3] * z + D5[2]) * z + D5[1]) * z + D5[0]) * z + D50;
        let c6 = (D6[1] * z + D6[0]) * z + D60;
        t = ((((((D70 * u + c6) * u + c5) * u + c4) * u + c3) * u + c2) * u + c1) * u + c0;
    } else if iop == 2 {
        let c0 = (((((D0[5] * z + D0[4]) * z + D0[3]) * z + D0[2]) * z + D0[1]) * z + D0[0]) * z
            - GRATIO_THIRD;
        let c1 = (((D1[3] * z + D1[2]) * z + D1[1]) * z + D1[0]) * z + D10;
        let c2 = D2[0] * z + D20;
        t = (c2 * u + c1) * u + c0;
    } else {
        t = ((D0[2] * z + D0[1]) * z + D0[0]) * z - GRATIO_THIRD;
    }
    gratio_temme_finish(l, c, w, t, a)
}

fn gratio_temme_340(a: f64, l: f64, z: f64, u: f64, c: f64, w: f64) -> (f64, f64) {
    let c0 = ((((((D0[6] * z + D0[5]) * z + D0[4]) * z + D0[3]) * z + D0[2]) * z + D0[1]) * z
        + D0[0])
        * z
        - GRATIO_THIRD;
    let c1 = (((((D1[5] * z + D1[4]) * z + D1[3]) * z + D1[2]) * z + D1[1]) * z + D1[0]) * z + D10;
    let c2 = ((((D2[4] * z + D2[3]) * z + D2[2]) * z + D2[1]) * z + D2[0]) * z + D20;
    let c3 = (((D3[3] * z + D3[2]) * z + D3[1]) * z + D3[0]) * z + D30;
    let c4 = (D4[1] * z + D4[0]) * z + D40;
    let c5 = (D5[1] * z + D5[0]) * z + D50;
    let c6 = D6[0] * z + D60;
    let t = ((((((D70 * u + c6) * u + c5) * u + c4) * u + c3) * u + c2) * u + c1) * u + c0;
    gratio_temme_finish(l, c, w, t, a)
}

/// statement 330: Temme expansion for a >= big, |1 - x/a| <= e0/sqrt(a).
fn gratio_temme_330(a: f64, l: f64, y: f64, z: f64, e: f64, _s: f64, iop: usize) -> (f64, f64) {
    if a * e * e > 3.28e-3 {
        return (2.0, 2.0);
    }
    let c = 0.5 + (0.5 - y);
    let w = (0.5 - y.sqrt() * (0.5 + (0.5 - y / 3.0)) / GRATIO_RTPI) / c;
    let u = 1.0 / a;
    let mut zz = (z + z).sqrt();
    if l < 1.0 {
        zz = -zz;
    }
    let z = zz;
    if iop == 1 {
        return gratio_temme_340(a, l, z, u, c, w);
    }
    let t;
    if iop == 2 {
        let c0 = (D0[1] * z + D0[0]) * z - GRATIO_THIRD;
        let c1 = D1[0] * z + D10;
        t = (D20 * u + c1) * u + c0;
    } else {
        t = D0[0] * z - GRATIO_THIRD;
    }
    gratio_temme_finish(l, c, w, t, a)
}

fn gratio_temme_finish(l: f64, c: f64, w: f64, t: f64, a: f64) -> (f64, f64) {
    let rta = a.sqrt();
    if l >= 1.0 {
        let qans = c * (w + GRATIO_RT2PIN * t / rta);
        (0.5 + (0.5 - qans), qans)
    } else {
        let ans = c * (w - GRATIO_RT2PIN * t / rta);
        (ans, 0.5 + (0.5 - ans))
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
    if a.abs() > 0.375 {
        let x = 1.0 + a;
        return x.ln();
    }
    let t = a / (a + 2.0);
    let t2 = t * t;
    let w = (((P3 * t2 + P2) * t2 + P1) * t2 + 1.0) / (((Q3 * t2 + Q2) * t2 + Q1) * t2 + 1.0);
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
        return x - w.ln();
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
        let w = (((((R5 * x + R4) * x + R3) * x + R2) * x + R1) * x + R0)
            / (((((S5 * x + S4) * x + S3) * x + S2) * x + S1) * x + 1.0);
        return x * w;
    }
    let w = ((((((P6 * a + P5) * a + P4) * a + P3) * a + P2) * a + P1) * a + P0)
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
        return bratio_gamln1(a) - a.ln();
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
        return bratio_gamln1(t - 1.0) + w.ln();
    }
    let t = (1.0 / a).powf(2.0);
    let w = (((((C5 * t + C4) * t + C3) * t + C2) * t + C1) * t + C0) / a;
    (D + w) + (a - 0.5) * (a.ln() - 1.0)
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
    let t = (1.0 / b).powf(2.0);
    let mut w = ((((C5 * s11 * t + C4 * s9) * t + C3 * s7) * t + C2 * s5) * t + C1 * s3) * t + C0;
    w *= c / b;
    let u = d * bratio_alnrel(a / b);
    let v = a * (b.ln() - 1.0);
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
    let mut t = (1.0 / b).powf(2.0);
    let mut w = ((((C5 * s11 * t + C4 * s9) * t + C3 * s7) * t + C2 * s5) * t + C1 * s3) * t + C0;
    w *= c / b;
    t = (1.0 / a).powf(2.0);
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
    bratio_gamln1(x - 1.0) + (x * (1.0 + x)).ln()
}

/// betaln: evaluation of the logarithm of the beta function.
fn bratio_betaln(a0: f64, b0: f64) -> f64 {
    const E: f64 = 0.918938533204673e0;
    let mut a = a0.min(b0);
    let b = a0.max(b0);
    if a >= 8.0 {
        let w = bratio_bcorr(a, b);
        let h = a / b;
        let c = h / (1.0 + h);
        let u = -(a - 0.5) * c.ln();
        let v = b * bratio_alnrel(h);
        if u > v {
            return (((-0.5 * b.ln() + E) + w) - v) - u;
        }
        return (((-0.5 * b.ln() + E) + w) - u) - v;
    }
    if a >= 1.0 {
        if a <= 2.0 {
            if b <= 2.0 {
                return bratio_gamln(a) + bratio_gamln(b) - bratio_gsumln(a, b);
            }
            let w = 0.0;
            if b < 8.0 {
                return bratio_betaln_60(a, b, w);
            }
            return bratio_gamln(a) + bratio_algdiv(a, b);
        }
        // a > 2
        if b > 1000.0 {
            let n = bratio_ftoi(a - 1.0);
            let mut w = 1.0;
            for _ in 0..n {
                a -= 1.0;
                w *= a / (1.0 + a / b);
            }
            return (w.ln() - n as f64 * b.ln()) + (bratio_gamln(a) + bratio_algdiv(a, b));
        }
        let n = bratio_ftoi(a - 1.0);
        let mut w = 1.0;
        for _ in 0..n {
            a -= 1.0;
            let h = a / b;
            w *= h / (1.0 + h);
        }
        let w = w.ln();
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
    w + z.ln() + (bratio_gamln(a) + (bratio_gamln(b) - bratio_gsumln(a, b)))
}

/// erf_nswc: evaluation of the real error function.
fn bratio_erf_nswc(x: f64) -> f64 {
    const C: f64 = 0.564189583547756;
    const A: [f64; 5] = [
        0.771058495001320e-04,
        -0.133733772997339e-02,
        0.323076579225834e-01,
        0.479137145607681e-01,
        0.128379167095513e+00,
    ];
    const B: [f64; 3] = [
        0.301048631703895e-02,
        0.538971687740286e-01,
        0.375795757275549e+00,
    ];
    const P: [f64; 8] = [
        -1.36864857382717e-07,
        5.64195517478974e-01,
        7.21175825088309e+00,
        4.31622272220567e+01,
        1.52989285046940e+02,
        3.39320816734344e+02,
        4.51918953711873e+02,
        3.00459261020162e+02,
    ];
    const Q: [f64; 8] = [
        1.0,
        1.27827273196294e+01,
        7.70001529352295e+01,
        2.77585444743988e+02,
        6.38980264465631e+02,
        9.31354094850610e+02,
        7.90950925327898e+02,
        3.00459260956983e+02,
    ];
    const R: [f64; 5] = [
        2.10144126479064e+00,
        2.62370141675169e+01,
        2.13688200555087e+01,
        4.65807828718470e+00,
        2.82094791773523e-01,
    ];
    const S: [f64; 4] = [
        9.41537750555460e+01,
        1.87114811799590e+02,
        9.90191814623914e+01,
        1.80124575948747e+01,
    ];
    let ax = x.abs();
    if ax <= 0.5 {
        let t = x * x;
        let top = ((((A[0] * t + A[1]) * t + A[2]) * t + A[3]) * t + A[4]) + 1.0;
        let bot = ((B[0] * t + B[1]) * t + B[2]) * t + 1.0;
        return x * (top / bot);
    }
    if ax <= 4.0 {
        let top = ((((((P[0] * ax + P[1]) * ax + P[2]) * ax + P[3]) * ax + P[4]) * ax + P[5]) * ax
            + P[6])
            * ax
            + P[7];
        let bot = ((((((Q[0] * ax + Q[1]) * ax + Q[2]) * ax + Q[3]) * ax + Q[4]) * ax + Q[5]) * ax
            + Q[6])
            * ax
            + Q[7];
        let v = 0.5 + (0.5 - crate::ported::excel_numeric::excel_exp(-x * x) * top / bot);
        return if x < 0.0 { -v } else { v };
    }
    if ax < 5.8 {
        let x2 = x * x;
        let t = 1.0 / x2;
        let top = (((R[0] * t + R[1]) * t + R[2]) * t + R[3]) * t + R[4];
        let bot = (((S[0] * t + S[1]) * t + S[2]) * t + S[3]) * t + 1.0;
        let v0 = (C - top / (x2 * bot)) / ax;
        let v = 0.5 + (0.5 - crate::ported::excel_numeric::excel_exp(-x2) * v0);
        return if x < 0.0 { -v } else { v };
    }
    1.0f64.copysign(x)
}

/// erfc1: evaluation of the complementary error function.
fn bratio_erfc1(ind: i32, x: f64) -> f64 {
    const C: f64 = 0.564189583547756;
    const A: [f64; 5] = [
        0.771058495001320e-04,
        -0.133733772997339e-02,
        0.323076579225834e-01,
        0.479137145607681e-01,
        0.128379167095513e+00,
    ];
    const B: [f64; 3] = [
        0.301048631703895e-02,
        0.538971687740286e-01,
        0.375795757275549e+00,
    ];
    const P: [f64; 8] = [
        -1.36864857382717e-07,
        5.64195517478974e-01,
        7.21175825088309e+00,
        4.31622272220567e+01,
        1.52989285046940e+02,
        3.39320816734344e+02,
        4.51918953711873e+02,
        3.00459261020162e+02,
    ];
    const Q: [f64; 8] = [
        1.0,
        1.27827273196294e+01,
        7.70001529352295e+01,
        2.77585444743988e+02,
        6.38980264465631e+02,
        9.31354094850610e+02,
        7.90950925327898e+02,
        3.00459260956983e+02,
    ];
    const R: [f64; 5] = [
        2.10144126479064e+00,
        2.62370141675169e+01,
        2.13688200555087e+01,
        4.65807828718470e+00,
        2.82094791773523e-01,
    ];
    const S: [f64; 4] = [
        9.41537750555460e+01,
        1.87114811799590e+02,
        9.90191814623914e+01,
        1.80124575948747e+01,
    ];
    let ax = x.abs();
    if ax <= 0.5 {
        let t = x * x;
        let top = ((((A[0] * t + A[1]) * t + A[2]) * t + A[3]) * t + A[4]) + 1.0;
        let bot = ((B[0] * t + B[1]) * t + B[2]) * t + 1.0;
        let mut v = 0.5 + (0.5 - x * (top / bot));
        if ind != 0 {
            v = crate::ported::excel_numeric::excel_exp(t) * v;
        }
        return v;
    }
    let v: f64;
    if ax <= 4.0 {
        let top = ((((((P[0] * ax + P[1]) * ax + P[2]) * ax + P[3]) * ax + P[4]) * ax + P[5]) * ax
            + P[6])
            * ax
            + P[7];
        let bot = ((((((Q[0] * ax + Q[1]) * ax + Q[2]) * ax + Q[3]) * ax + Q[4]) * ax + Q[5]) * ax
            + Q[6])
            * ax
            + Q[7];
        v = top / bot;
    } else {
        if x <= -5.6 {
            let mut vv = 2.0;
            if ind != 0 {
                vv = 2.0 * crate::ported::excel_numeric::excel_exp(x * x);
            }
            return vv;
        }
        if ind == 0 && (x > 100.0 || x * x > -bratio_exparg(1)) {
            return 0.0;
        }
        let t = (1.0 / x).powf(2.0);
        let top = (((R[0] * t + R[1]) * t + R[2]) * t + R[3]) * t + R[4];
        let bot = (((S[0] * t + S[1]) * t + S[2]) * t + S[3]) * t + 1.0;
        v = (C - t * top / bot) / ax;
    }
    if ind != 0 {
        if x < 0.0 {
            return 2.0 * crate::ported::excel_numeric::excel_exp(x * x) - v;
        }
        return v;
    }
    let w = x * x;
    let t = w;
    let e_ = w - t;
    let mut v = ((0.5 + (0.5 - e_)) * crate::ported::excel_numeric::excel_exp(-t)) * v;
    if x < 0.0 {
        v = 2.0 - v;
    }
    v
}

/// rexp: evaluation of the function exp(x) - 1.
fn bratio_rexp(x: f64) -> f64 {
    const P1: f64 = 0.914041914819518e-09;
    const P2: f64 = 0.238082361044469e-01;
    const Q1: f64 = -0.499999999085958e+00;
    const Q2: f64 = 0.107141568980644e+00;
    const Q3: f64 = -0.119041179760821e-01;
    const Q4: f64 = 0.595130811860248e-03;
    if x.abs() <= 0.15 {
        return x * (((P2 * x + P1) * x + 1.0) / ((((Q4 * x + Q3) * x + Q2) * x + Q1) * x + 1.0));
    }
    let w = crate::ported::excel_numeric::excel_exp(x);
    if x <= 0.0 {
        (w - 0.5) - 0.5
    } else {
        w * (0.5 + (0.5 - 1.0 / w))
    }
}

/// psi: evaluation of the digamma function.
fn bratio_psi(xx: f64) -> f64 {
    const PIOV4: f64 = 0.785398163397448e0;
    const DX0: f64 = 1.461632144968362341262659542325721325;
    const P1: [f64; 7] = [
        0.895385022981970e-02,
        0.477762828042627e+01,
        0.142441585084029e+03,
        0.118645200713425e+04,
        0.363351846806499e+04,
        0.413810161269013e+04,
        0.130560269827897e+04,
    ];
    const Q1: [f64; 6] = [
        0.448452573429826e+02,
        0.520752771467162e+03,
        0.221000799247830e+04,
        0.364127349079381e+04,
        0.190831076596300e+04,
        0.691091682714533e-05,
    ];
    const P2: [f64; 4] = [
        -0.212940445131011e+01,
        -0.701677227766759e+01,
        -0.448616543918019e+01,
        -0.648157123766197e+00,
    ];
    const Q2: [f64; 4] = [
        0.322703493791143e+02,
        0.892920700481861e+02,
        0.546117738103215e+02,
        0.777788548522962e+01,
    ];
    let mut xmax1 = 2147483647.0f64;
    xmax1 = xmax1.min(1.0 / bratio_spmpar1());
    let xsmall = 1.0e-9;
    let mut x = xx;
    let mut aug = 0.0;
    if x < 0.5 {
        if x.abs() <= xsmall {
            if x == 0.0 {
                return 0.0;
            }
            aug = -1.0 / x;
        } else {
            let mut w = -x;
            let mut sgn = PIOV4;
            if w <= 0.0 {
                w = -w;
                sgn = -sgn;
            }
            if w >= xmax1 {
                return 0.0;
            }
            let mut nq = w as i64;
            w -= nq as f64;
            nq = (w * 4.0) as i64;
            w = 4.0 * (w - nq as f64 * 0.25);
            let n = nq / 2;
            if (n + n) != nq {
                w = 1.0 - w;
            }
            let z = PIOV4 * w;
            let m = n / 2;
            if (m + m) != n {
                sgn = -sgn;
            }
            let n = (nq + 1) / 2;
            let mut m = n / 2;
            m += m;
            if m != n {
                aug = sgn * ((z.sin() / z.cos()) * 4.0);
            } else {
                if z == 0.0 {
                    return 0.0;
                }
                aug = sgn * ((z.cos() / z.sin()) * 4.0);
            }
        }
        x = 1.0 - x;
    }
    if x <= 3.0 {
        let mut den = x;
        let mut upper = P1[0] * x;
        for i in 0..5 {
            den = (den + Q1[i]) * x;
            upper = (upper + P1[i + 1]) * x;
        }
        den = (upper + P1[6]) / (den + Q1[5]);
        let xmx0 = x - DX0;
        return den * xmx0 + aug;
    }
    if x >= xmax1 {
        return aug + x.ln();
    }
    let w = 1.0 / (x * x);
    let mut den = w;
    let mut upper = P2[0] * w;
    for i in 0..3 {
        den = (den + Q2[i]) * w;
        upper = (upper + P2[i + 1]) * w;
    }
    aug = upper / (den + Q2[3]) - 0.5 / x + aug;
    aug + x.ln()
}

/// fpser: power series for Ix(a, b) when b < eps, x <= 0.5.
fn bratio_fpser(a: f64, b: f64, x: f64, eps: f64) -> f64 {
    let mut fp = 1.0;
    if a > 1.0e-3 * eps {
        fp = 0.0;
        let t = a * x.ln();
        if t < bratio_exparg(1) {
            return fp;
        }
        fp = crate::ported::excel_numeric::excel_exp(t);
    }
    fp = (b / a) * fp;
    let tol = eps / a;
    let mut an = a + 1.0;
    let mut t = x;
    let mut s = t / an;
    loop {
        an += 1.0;
        t = x * t;
        let c = t / an;
        s += c;
        if c.abs() <= tol {
            break;
        }
    }
    fp * (1.0 + a * s)
}

/// apser: Ix(a, b) - a series expansion for a very small (used with x <= 0.5).
fn bratio_apser(a: f64, b: f64, x: f64, eps: f64) -> f64 {
    const G: f64 = 0.577215664901533e0;
    let bx = b * x;
    let mut t = x - bx;
    let c;
    if b * eps <= 2.0e-2 {
        c = x.ln() + bratio_psi(b) + G + t;
    } else {
        c = bx.ln() + G + t;
    }
    let tol = 5.0 * eps * c.abs();
    let mut j = 1.0;
    let mut s = 0.0;
    loop {
        j += 1.0;
        t *= x - bx / j;
        let aj = t / j;
        s += aj;
        if aj.abs() <= tol {
            break;
        }
    }
    -a * (c + s)
}

/// bpser: power series expansion for Ix(a, b) when b <= 1 or b*x <= 0.7.
fn bratio_bpser(a: f64, b: f64, x: f64, eps: f64) -> f64 {
    let mut bp = 0.0;
    if x == 0.0 {
        return bp;
    }
    let a0 = a.min(b);
    if a0 >= 1.0 {
        let z = a * x.ln() - bratio_betaln(a, b);
        bp = crate::ported::excel_numeric::excel_exp(z) / a;
    } else {
        let b0 = a.max(b);
        if b0 >= 8.0 {
            let u = bratio_gamln1(a0) + bratio_algdiv(a0, b0);
            let z = a * x.ln() - u;
            bp = (a0 / a) * crate::ported::excel_numeric::excel_exp(z);
        } else if b0 > 1.0 {
            let mut u = bratio_gamln1(a0);
            let m = bratio_ftoi(b0 - 1.0);
            let mut b0m = b0;
            if m >= 1 {
                let mut c = 1.0;
                for _ in 0..m {
                    b0m -= 1.0;
                    c *= b0m / (a0 + b0m);
                }
                u = c.ln() + u;
            }
            let z = a * x.ln() - u;
            b0m -= 1.0;
            let apb = a0 + b0m;
            let t;
            if apb > 1.0 {
                let uu = a0 + b0m - 1.0;
                t = (1.0 + bratio_gam1(uu)) / apb;
            } else {
                t = 1.0 + bratio_gam1(apb);
            }
            bp = crate::ported::excel_numeric::excel_exp(z) * (a0 / a) * (1.0 + bratio_gam1(b0m)) / t;
        } else {
            bp = x.powf(a);
            if bp == 0.0 {
                return bp;
            }
            let apb = a + b;
            let z;
            if apb > 1.0 {
                let u = a + b - 1.0;
                z = (1.0 + bratio_gam1(u)) / apb;
            } else {
                z = 1.0 + bratio_gam1(apb);
            }
            let c = (1.0 + bratio_gam1(a)) * (1.0 + bratio_gam1(b)) / z;
            bp = bp * c * (b / apb);
        }
    }
    if bp == 0.0 || a <= 0.1 * eps {
        return bp;
    }
    let mut summ = 0.0;
    let mut n = 0.0;
    let mut c = 1.0;
    let tol = eps / a;
    loop {
        n += 1.0;
        c = c * (0.5 + (0.5 - b / n)) * x;
        let w = c / (a + n);
        summ += w;
        if w.abs() <= tol {
            break;
        }
    }
    bp * (1.0 + a * summ)
}

/// bup: Ix(a, b) - Ix(a+n, b) evaluated for n a positive integer.
fn bratio_bup(a: f64, b: f64, x: f64, y: f64, n: i32, eps: f64) -> f64 {
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
    let bp = bratio_brcmp1(mu, a, b, x, y) / a;
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
        d = ((apb + l) / (ap1 + l)) * x * d;
        w += d;
    }
    if k != nm1 {
        for i in (k + 1)..=nm1 {
            let l = (i - 1) as f64;
            d = ((apb + l) / (ap1 + l)) * x * d;
            w += d;
            if d <= eps * w {
                break;
            }
        }
    }
    bp * w
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
            e - (x / x0).ln()
        } else {
            bratio_rlog1(e)
        };
        e = lambda_ / b;
        let v = if e.abs() > 0.6 {
            e - (y / y0).ln()
        } else {
            bratio_rlog1(e)
        };
        let z = crate::ported::excel_numeric::excel_exp(-(a * u + b * v));
        return CONST * (b * x0).sqrt() * z * crate::ported::excel_numeric::excel_exp(-bratio_bcorr(a, b));
    }

    let lnx;
    let lny;
    if x > 0.375 {
        if y > 0.375 {
            lnx = x.ln();
            lny = y.ln();
        } else {
            lnx = bratio_alnrel(-y);
            lny = y.ln();
        }
    } else {
        lnx = x.ln();
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
            u = c.ln() + u;
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
            e - (x / x0).ln()
        } else {
            bratio_rlog1(e)
        };
        e = lambda_ / b;
        let v = if e.abs() > 0.6 {
            e - (y / y0).ln()
        } else {
            bratio_rlog1(e)
        };
        let z = bratio_esum(mu, -(a * u + b * v));
        return CONST * (b * x0).sqrt() * z * crate::ported::excel_numeric::excel_exp(-bratio_bcorr(a, b));
    }

    let lnx;
    let lny;
    if x > 0.375 {
        if y > 0.375 {
            lnx = x.ln();
            lny = y.ln();
        } else {
            lnx = bratio_alnrel(-y);
            lny = y.ln();
        }
    } else {
        lnx = x.ln();
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
            u = c.ln() + u;
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
fn bratio_bfrac(a: f64, b: f64, x: f64, y: f64, lambda_: f64, eps: f64) -> f64 {
    let bf = bratio_brcomp(a, b, x, y);
    if bf == 0.0 {
        return bf;
    }
    let c = 1.0 + lambda_;
    let c0 = b / a;
    let c1 = 1.0 + 1.0 / a;
    let yp1 = y + 1.0;
    let mut n = 0.0;
    let mut p = 1.0;
    let mut s = a + 1.0;
    let mut an = 0.0;
    let mut bn = 1.0;
    let mut anp1 = 1.0;
    let mut bnp1 = c / c1;
    let mut r = c1 / c;
    loop {
        n += 1.0;
        let t0 = n / a;
        let w = n * (b - n) * x;
        let mut e = a / s;
        let alpha = (p * (p + c0) * e * e) * (w * x);
        e = (1.0 + t0) / (c1 + t0 + t0);
        let beta = n + w / s + e * (c + n * yp1);
        p = 1.0 + t0;
        s += 2.0;
        let mut t = alpha * an + beta * anp1;
        an = anp1;
        anp1 = t;
        t = alpha * bn + beta * bnp1;
        bn = bnp1;
        bnp1 = t;
        let r0 = r;
        r = anp1 / bnp1;
        if (r - r0).abs() <= eps * r {
            break;
        }
        an /= bnp1;
        bn /= bnp1;
        anp1 = r;
        bnp1 = 1.0;
    }
    bf * r
}

/// bgrat: asymptotic expansion for Ix(a, b) when a is larger than b. Adds the
/// expansion to `w`; returns (w_new, ierr).
fn bratio_bgrat(a: f64, b: f64, x: f64, y: f64, w: f64, eps: f64) -> (f64, i32) {
    let mut c = [0.0f64; 31];
    let mut d = [0.0f64; 31];
    let bm1 = (b - 0.5) - 0.5;
    let nu = a + 0.5 * bm1;
    let lnx = if y > 0.375 { x.ln() } else { bratio_alnrel(-y) };
    let z = -nu * lnx;
    if b * z == 0.0 {
        return (w, 1);
    }
    let mut r = b * (1.0 + bratio_gam1(b)) * crate::ported::excel_numeric::excel_exp(b * z.ln());
    r = r
        * crate::ported::excel_numeric::excel_exp(a * lnx)
        * crate::ported::excel_numeric::excel_exp(0.5 * bm1 * lnx);
    let mut u = bratio_algdiv(b, a) + b * nu.ln();
    u = r * crate::ported::excel_numeric::excel_exp(-u);
    if u == 0.0 {
        return (w, 1);
    }
    let (_p, q) = bratio_grat1(b, z, r, eps);
    let v = 0.25 * (1.0 / nu).powf(2.0);
    let t2 = 0.25 * lnx * lnx;
    let l = w / u;
    let mut j = q / r;
    let mut summ = j;
    let mut t = 1.0;
    let mut cn = 1.0;
    let mut n2 = 0.0;
    for n in 1..=30usize {
        let bp2n = b + n2;
        j = (bp2n * (bp2n + 1.0) * j + (z + bp2n + 1.0) * t) * v;
        n2 += 2.0;
        t *= t2;
        cn /= n2 * (n2 + 1.0);
        c[n] = cn;
        let mut s = 0.0;
        if n != 1 {
            let nm1 = n - 1;
            let mut coef = b - n as f64;
            for i in 1..=nm1 {
                s += coef * c[i] * d[n - i];
                coef += b;
            }
        }
        d[n] = bm1 * cn + s / n as f64;
        let dj = d[n] * j;
        summ += dj;
        if summ <= 0.0 {
            return (w, 1);
        }
        if dj.abs() <= eps * (summ + l) {
            break;
        }
    }
    (w + u * summ, 0)
}

/// grat1: evaluation of (P(a,x), Q(a,x)) for a <= 1.
fn bratio_grat1(a: f64, x: f64, r: f64, eps: f64) -> (f64, f64) {
    if a * x == 0.0 {
        if x <= a {
            return (0.0, 1.0);
        }
        return (1.0, 0.0);
    }
    if a == 0.5 {
        if x < 0.25 {
            let p = bratio_erf_nswc(x.sqrt());
            return (p, 0.5 + (0.5 - p));
        }
        let q = bratio_erfc1(0, x.sqrt());
        return (0.5 + (0.5 - q), q);
    }
    if x < 1.1 {
        let mut an = 3.0;
        let mut c = x;
        let mut summ = x / (a + 3.0);
        let tol = 0.1 * eps / (a + 1.0);
        loop {
            an += 1.0;
            c = -c * (x / an);
            let t = c / (a + an);
            summ += t;
            if t.abs() <= tol {
                break;
            }
        }
        let j = a * x * ((summ / 6.0 - 0.5 / (a + 2.0)) * x + 1.0 / (a + 1.0));
        let z = a * x.ln();
        let h = bratio_gam1(a);
        let g = 1.0 + h;
        let go50 = if x < 0.25 { z > -0.13394 } else { a < x / 2.59 };
        if go50 {
            let l = bratio_rexp(z);
            let w = 0.5 + (0.5 + l);
            let q = (w * j - l) * g - h;
            if q < 0.0 {
                return (1.0, 0.0);
            }
            return (0.5 + (0.5 - q), q);
        }
        let w = crate::ported::excel_numeric::excel_exp(z);
        let p = w * g * (0.5 + (0.5 - j));
        return (p, 0.5 + (0.5 - p));
    }
    // continued fraction
    let mut a2nm1 = 1.0;
    let mut a2n = 1.0;
    let mut b2nm1 = x;
    let mut b2n = x + (1.0 - a);
    let mut c = 1.0;
    let mut an0;
    loop {
        a2nm1 = x * a2n + c * a2nm1;
        b2nm1 = x * b2n + c * b2nm1;
        let am0 = a2nm1 / b2nm1;
        c += 1.0;
        let cma = c - a;
        a2n = a2nm1 + cma * a2n;
        b2n = b2nm1 + cma * b2n;
        an0 = a2n / b2n;
        if (an0 - am0).abs() < eps * an0 {
            break;
        }
    }
    let q = r * an0;
    (0.5 + (0.5 - q), q)
}

/// basym: asymptotic expansion for Ix(a, b) when a and b are large.
fn bratio_basym(a: f64, b: f64, lambda_: f64, eps: f64) -> f64 {
    let num = 20usize;
    const E0: f64 = 1.12837916709551;
    const E1: f64 = 0.353553390593274;
    let mut a0 = [0.0f64; 22];
    let mut b0 = [0.0f64; 22];
    let mut c = [0.0f64; 22];
    let mut d = [0.0f64; 22];
    let ba = 0.0;
    let (h, r0, r1, w0);
    if a >= b {
        h = b / a;
        r0 = 1.0 / (1.0 + h);
        r1 = (b - a) / a;
        w0 = 1.0 / (b * (1.0 + h)).sqrt();
    } else {
        h = a / b;
        r0 = 1.0 / (1.0 + h);
        r1 = (b - a) / b;
        w0 = 1.0 / (a * (1.0 + h)).sqrt();
    }
    let f = a * bratio_rlog1(-lambda_ / a) + b * bratio_rlog1(lambda_ / b);
    let t = crate::ported::excel_numeric::excel_exp(-f);
    if t == 0.0 {
        return ba;
    }
    let z0 = f.sqrt();
    let z = 0.5 * (z0 / E1);
    let z2 = f + f;
    a0[1] = (2.0 / 3.0) * r1;
    c[1] = -0.5 * a0[1];
    d[1] = -c[1];
    let mut j0 = (0.5 / E0) * bratio_erfc1(1, z0);
    let mut j1 = E1;
    let mut summ = j0 + d[1] * w0 * j1;
    let mut s = 1.0;
    let h2 = h * h;
    let mut hn = 1.0;
    let mut w = w0;
    let mut znm1 = z;
    let mut zn = z2;
    let mut n = 2usize;
    while n <= num {
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
    let u = crate::ported::excel_numeric::excel_exp(-bratio_bcorr(a, b));
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
        if b0 < eps.min(eps * a0) {
            branch = "fpser";
        } else if a0 < eps.min(eps * b0) && b0 * x0 <= 1.0 {
            branch = "apser";
        } else if a0.max(b0) > 1.0 {
            if b0 <= 1.0 {
                branch = "bpser";
            } else if x0 >= 0.3 {
                branch = "bpser_sym";
            } else if x0 < 0.1 && (x0 * b0).powf(a0) <= 0.7 {
                branch = "bpser";
            } else if b0 > 15.0 {
                branch = "bgrat_150";
            } else {
                branch = "bup_140";
            }
        } else if a0 >= 0.2_f64.min(b0) {
            branch = "bpser";
        } else if x0.powf(a0) <= 0.9 {
            branch = "bpser";
        } else if x0 >= 0.3 {
            branch = "bpser_sym";
        } else {
            branch = "bup_140";
        }

        match branch {
            "fpser" => {
                w = bratio_fpser(a0, b0, x0, eps);
                w1 = 0.5 + (0.5 - w);
            }
            "apser" => {
                w1 = bratio_apser(a0, b0, x0, eps);
                w = 0.5 + (0.5 - w1);
            }
            "bpser" => {
                w = bratio_bpser(a0, b0, x0, eps);
                w1 = 0.5 + (0.5 - w);
            }
            "bpser_sym" => {
                w1 = bratio_bpser(b0, a0, y0, eps);
                w = 0.5 + (0.5 - w1);
            }
            "bup_140" => {
                let n = 20;
                w1 = bratio_bup(b0, a0, y0, x0, n, eps);
                b0 += n as f64;
                let (nw1, _ie) = bratio_bgrat(b0, a0, y0, x0, w1, 15.0 * eps);
                w1 = nw1;
                w = 0.5 + (0.5 - w1);
            }
            "bgrat_150" => {
                let (nw1, _ie) = bratio_bgrat(b0, a0, y0, x0, w1, 15.0 * eps);
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

    // procedure for a0 > 1 and b0 > 1
    let mut lambda_ = if a > b {
        (a + b) * y - b
    } else {
        a - (a + b) * x
    };
    if lambda_ < 0.0 {
        ind = 1;
        a0 = b;
        b0 = a;
        x0 = y;
        y0 = x;
        lambda_ = lambda_.abs();
    }
    let branch: &str;
    if b0 < 40.0 && b0 * x0 <= 0.7 {
        branch = "bpser";
    } else if b0 < 40.0 {
        branch = "bup_160";
    } else if a0 > b0 {
        if b0 <= 100.0 {
            branch = "bfrac";
        } else if lambda_ > 0.03 * b0 {
            branch = "bfrac";
        } else {
            branch = "basym";
        }
    } else if a0 <= 100.0 {
        branch = "bfrac";
    } else if lambda_ > 0.03 * a0 {
        branch = "bfrac";
    } else {
        branch = "basym";
    }

    match branch {
        "bpser" => {
            w = bratio_bpser(a0, b0, x0, eps);
            w1 = 0.5 + (0.5 - w);
        }
        "bfrac" => {
            w = bratio_bfrac(a0, b0, x0, y0, lambda_, 15.0 * eps);
            w1 = 0.5 + (0.5 - w);
        }
        "basym" => {
            w = bratio_basym(a0, b0, lambda_, 100.0 * eps);
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
                w += bratio_bpser(a0, b0, x0, eps);
                w1 = 0.5 + (0.5 - w);
            } else {
                if a0 <= 15.0 {
                    let nn = 20;
                    w += bratio_bup(a0, b0, x0, y0, nn, eps);
                    a0 += nn as f64;
                }
                let (nw, _ie) = bratio_bgrat(a0, b0, x0, y0, w, 15.0 * eps);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn gamma_and_ln_gamma_match_known_values() {
        assert!((gamma(5.0) - 24.0).abs() < 1e-10);
        assert!((gamma(0.5) - std::f64::consts::PI.sqrt()).abs() < 1e-10);
        assert!((ln_gamma(5.0) - 24.0_f64.ln()).abs() < 1e-10);
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

    // Restricted historical implementation removed (dc-96l.7).
}
