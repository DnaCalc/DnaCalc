use crate::locale::{DateSystem as WorkbookDateSystem, excel_serial_from_ymd, ymd_from_excel_serial};
use crate::value::{CalcValue, CoreValue, WorksheetErrorCode};
use crate::ported::excel_numeric::crt_pow;

#[derive(Debug, Clone, PartialEq)]
pub enum BondCoreEvalError {
    ArityMismatch {
        expected_min: usize,
        expected_max: usize,
        actual: usize,
    },
    Domain(WorksheetErrorCode),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayCountBasis {
    Us30_360,
    ActualActual,
    Actual360,
    Actual365,
    European30_360,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ctx {
    settlement: i64,
    maturity: i64,
    frequency: i64,
    basis: DayCountBasis,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Period {
    prev: i64,
    next: i64,
    n: i64,
}

fn max_serial() -> i64 {
    excel_serial_from_ymd(WorkbookDateSystem::System1900, 9999, 12, 31).unwrap() as i64
}
fn derr(c: WorksheetErrorCode) -> BondCoreEvalError {
    BondCoreEvalError::Domain(c)
}
fn dyear(y: i64) -> f64 {
    if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
        366.0
    } else {
        365.0
    }
}
fn dim(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if dyear(y) == 366.0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}
fn act(s: i64, e: i64) -> f64 {
    (e - s) as f64
}
fn basis(v: f64) -> Result<DayCountBasis, BondCoreEvalError> {
    if !v.is_finite() {
        return Err(derr(WorksheetErrorCode::Num));
    }
    match v.trunc() as i64 {
        0 => Ok(DayCountBasis::Us30_360),
        1 => Ok(DayCountBasis::ActualActual),
        2 => Ok(DayCountBasis::Actual360),
        3 => Ok(DayCountBasis::Actual365),
        4 => Ok(DayCountBasis::European30_360),
        _ => Err(derr(WorksheetErrorCode::Num)),
    }
}
fn freq(v: f64) -> Result<i64, BondCoreEvalError> {
    if !v.is_finite() {
        return Err(derr(WorksheetErrorCode::Num));
    }
    match v.trunc() as i64 {
        1 | 2 | 4 => Ok(v.trunc() as i64),
        _ => Err(derr(WorksheetErrorCode::Num)),
    }
}
fn dser(v: f64) -> Result<i64, BondCoreEvalError> {
    if !v.is_finite() {
        return Err(derr(WorksheetErrorCode::Value));
    }
    let s = v.trunc() as i64;
    if s < 1 || s > max_serial() {
        return Err(derr(WorksheetErrorCode::Value));
    }
    ymd_from_excel_serial(WorkbookDateSystem::System1900, s as f64)
        .ok_or(derr(WorksheetErrorCode::Value))?;
    Ok(s)
}
fn pos(v: f64) -> Result<f64, BondCoreEvalError> {
    if !v.is_finite() {
        Err(derr(WorksheetErrorCode::Value))
    } else if v <= 0.0 {
        Err(derr(WorksheetErrorCode::Num))
    } else {
        Ok(v)
    }
}
fn rate(v: f64) -> Result<f64, BondCoreEvalError> {
    if !v.is_finite() {
        Err(derr(WorksheetErrorCode::Value))
    } else if v < 0.0 {
        Err(derr(WorksheetErrorCode::Num))
    } else {
        Ok(v)
    }
}
fn addm(s: i64, m: i64) -> Option<i64> {
    let (y, mo, d) = ymd_from_excel_serial(WorkbookDateSystem::System1900, s as f64)?;
    let idx = y.checked_mul(12)?.checked_add(mo - 1)?.checked_add(m)?;
    let ty = idx.div_euclid(12);
    let tm = idx.rem_euclid(12) + 1;
    let end = d == dim(y, mo);
    let td = if end { dim(ty, tm) } else { d.min(dim(ty, tm)) };
    excel_serial_from_ymd(WorkbookDateSystem::System1900, ty, tm, td).map(|v| v as i64)
}
fn d360us(s: i64, e: i64) -> Result<f64, BondCoreEvalError> {
    crate::ported::day_count_common::us_30_360(s, e).map_err(derr)
}
fn d360eu(s: i64, e: i64) -> Result<f64, BondCoreEvalError> {
    let (sy, sm, mut sd) = ymd_from_excel_serial(WorkbookDateSystem::System1900, s as f64)
        .ok_or(derr(WorksheetErrorCode::Value))?;
    let (ey, em, mut ed) = ymd_from_excel_serial(WorkbookDateSystem::System1900, e as f64)
        .ok_or(derr(WorksheetErrorCode::Value))?;
    if sd == 31 {
        sd = 30;
    }
    if ed == 31 {
        ed = 30;
    }
    Ok(((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64)
}
fn less_or_equal_to_a_year_apart(s: i64, e: i64) -> Result<bool, BondCoreEvalError> {
    let (sy, sm, sd) = ymd_from_excel_serial(WorkbookDateSystem::System1900, s as f64)
        .ok_or(derr(WorksheetErrorCode::Value))?;
    let (ey, em, ed) = ymd_from_excel_serial(WorkbookDateSystem::System1900, e as f64)
        .ok_or(derr(WorksheetErrorCode::Value))?;
    Ok((ey - sy) < 1 || ((ey - sy) == 1 && (em < sm || (em == sm && ed <= sd))))
}
fn consider_as_bisestile(s: i64, e: i64) -> Result<bool, BondCoreEvalError> {
    let (sy, _sm, _sd) = ymd_from_excel_serial(WorkbookDateSystem::System1900, s as f64)
        .ok_or(derr(WorksheetErrorCode::Value))?;
    let (ey, em, ed) = ymd_from_excel_serial(WorkbookDateSystem::System1900, e as f64)
        .ok_or(derr(WorksheetErrorCode::Value))?;
    if sy == ey {
        return Ok(dyear(sy) == 366.0);
    }
    if em == 2 && ed == 29 {
        return Ok(true);
    }
    for year in sy..=ey {
        if dyear(year) == 366.0 {
            let feb29 = excel_serial_from_ymd(WorkbookDateSystem::System1900, year, 2, 29)
                .ok_or(derr(WorksheetErrorCode::Value))? as i64;
            if s <= feb29 && feb29 <= e {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn days_in_year_for_mat(s: i64, e: i64, b: DayCountBasis) -> Result<f64, BondCoreEvalError> {
    match b {
        DayCountBasis::Us30_360 | DayCountBasis::Actual360 | DayCountBasis::European30_360 => {
            Ok(360.0)
        }
        DayCountBasis::Actual365 => Ok(365.0),
        DayCountBasis::ActualActual => {
            if !less_or_equal_to_a_year_apart(s, e)? {
                let (sy, _, _) = ymd_from_excel_serial(WorkbookDateSystem::System1900, s as f64)
                    .ok_or(derr(WorksheetErrorCode::Value))?;
                let (ey, _, _) = ymd_from_excel_serial(WorkbookDateSystem::System1900, e as f64)
                    .ok_or(derr(WorksheetErrorCode::Value))?;
                let tot_years = (ey - sy) + 1;
                let start_of_issue_year =
                    excel_serial_from_ymd(WorkbookDateSystem::System1900, sy, 1, 1)
                        .ok_or(derr(WorksheetErrorCode::Value))? as i64;
                let start_after_end_year =
                    excel_serial_from_ymd(WorkbookDateSystem::System1900, ey + 1, 1, 1)
                        .ok_or(derr(WorksheetErrorCode::Value))? as i64;
                Ok(act(start_of_issue_year, start_after_end_year) / tot_years as f64)
            } else if consider_as_bisestile(s, e)? {
                Ok(366.0)
            } else {
                Ok(365.0)
            }
        }
    }
}
fn yf(s: i64, e: i64, b: DayCountBasis) -> Result<f64, BondCoreEvalError> {
    match b {
        DayCountBasis::Us30_360 => Ok(d360us(s, e)? / 360.0),
        // Excel's actual/actual rule, shared with YEARFRAC basis 1 (W111-5 G8-06).
        DayCountBasis::ActualActual => {
            crate::ported::day_count_common::excel_actual_actual_fraction(s, e).map_err(derr)
        }
        DayCountBasis::Actual360 => Ok(act(s, e) / 360.0),
        DayCountBasis::Actual365 => Ok(act(s, e) / 365.0),
        DayCountBasis::European30_360 => Ok(d360eu(s, e)? / 360.0),
    }
}
fn dc(s: i64, e: i64, b: DayCountBasis, f: i64) -> Result<f64, BondCoreEvalError> {
    match b {
        DayCountBasis::Us30_360 => Ok(360.0 / f as f64),
        DayCountBasis::ActualActual => Ok(act(s, e)),
        DayCountBasis::Actual360 => Ok(360.0 / f as f64),
        DayCountBasis::Actual365 => Ok(365.0 / f as f64),
        DayCountBasis::European30_360 => Ok(360.0 / f as f64),
    }
}
fn dd(s: i64, e: i64, b: DayCountBasis) -> Result<f64, BondCoreEvalError> {
    match b {
        DayCountBasis::Us30_360 => d360us(s, e),
        DayCountBasis::ActualActual | DayCountBasis::Actual360 | DayCountBasis::Actual365 => {
            Ok(act(s, e))
        }
        DayCountBasis::European30_360 => d360eu(s, e),
    }
}
fn ctx(
    settlement: f64,
    maturity: f64,
    frequency: f64,
    b: Option<f64>,
) -> Result<Ctx, BondCoreEvalError> {
    let settlement = dser(settlement)?;
    let maturity = dser(maturity)?;
    let frequency = freq(frequency)?;
    let basis = basis(b.unwrap_or(0.0))?;
    if settlement >= maturity {
        return Err(derr(WorksheetErrorCode::Num));
    }
    Ok(Ctx {
        settlement,
        maturity,
        frequency,
        basis,
    })
}
fn period(c: Ctx) -> Result<Period, BondCoreEvalError> {
    let mpc = 12 / c.frequency;
    let mut next = c.maturity;
    let mut n = 1i64;
    loop {
        let prev = addm(next, -mpc).ok_or(derr(WorksheetErrorCode::Num))?;
        if prev <= c.settlement {
            return Ok(Period { prev, next, n });
        }
        next = prev;
        n += 1;
    }
}
/// Excel's bond discount factor `base^(off+k)` is computed with the C-runtime `pow`.
///
/// Two staged cases, both live-Excel identified:
/// * **Non-negative integer exponent** (on-coupon settlement, where `off = dsc/e = 1`,
///   so every `off+k` is an integer) → the C-runtime `pow` integer special case =
///   **binary exponentiation** (square-and-multiply, plain `f64` multiply). Rust's
///   `f64::powf` uses `exp·ln` even for integers, so on-coupon PRICE drifts 1–2 ULP
///   without this. Held-out validated 25/25 across 5 live-Excel bonds (build 20131).
/// * **Fractional exponent** (off-coupon) → the legacy CRT `pow` chain
///   `exp(RN53(RN64(exp·ln base)))` via the x87 `ln`/`mul`/`exp`, i.e.
///   [`crate::ported::excel_numeric::excel_pow_chain`] — NOT the platform `f64::powf`. The two
///   agree on the vast majority of inputs but diverge ±1 ULP on the Actual/360 (basis 2)
///   and Actual/365 (basis 3) fractional discount ladders at higher yields; the x87
///   chain reproduces live Excel where `powf` does not (W109 G6-03d, b37 3474→3656 / 3446→3658
///   on bases 2/3, b38 held-out 315/315 + 315/315). This is the SAME distribution-substrate
///   pow identified independently in the G3 lane-1 work — a cross-lane confirmation.
///
/// See `work/w109/G6-solvers/YIELD_PRICE_FORWARD_KERNEL.md` and
/// `work/w109/G6-b2b3/agentV_results.md`.
fn excel_bond_pow(base: f64, exp: f64) -> f64 {
    if exp >= 0.0 && exp < 1024.0 && exp.fract() == 0.0 {
        let mut n = exp as u64;
        let mut r = 1.0_f64;
        let mut b = base;
        while n > 0 {
            if n & 1 == 1 {
                r *= b;
            }
            n >>= 1;
            if n > 0 {
                b *= b;
            }
        }
        r
    } else {
        crate::ported::excel_numeric::excel_pow_chain(base, exp)
    }
}
fn pcomp(
    rate: f64,
    yld: f64,
    red: f64,
    c: Ctx,
    p: Period,
) -> Result<(f64, f64, f64, f64, f64), BondCoreEvalError> {
    // `binexp = false` keeps the legacy `powf` discount path. Callers that need Excel's
    // C-runtime integer-`pow` staging (PRICE) pass `true`; YIELD's solver and DURATION keep
    // `false` until their own identifications land (the forward fix is coupled to the YIELD
    // schedule — see the workset notes).
    pcomp_disc(rate, yld, red, c, p, false)
}
fn pcomp_disc(
    rate: f64,
    yld: f64,
    red: f64,
    c: Ctx,
    p: Period,
    binexp: bool,
) -> Result<(f64, f64, f64, f64, f64), BondCoreEvalError> {
    let coup = 100.0 * rate / c.frequency as f64;
    let e = dc(p.prev, p.next, c.basis, c.frequency)?;
    let a = dd(p.prev, c.settlement, c.basis)?;
    // Excel's internal discount/settlement fraction is the DERIVED complement
    // `dsc = E - A` (universal across bases), NOT the direct day-count span
    // settlement→next-coupon. For bases 0/1/4 the two coincide except on
    // settlement-on-31st 30/360 rows (which `E - A` also fixes); for Actual/360
    // (basis 2) and Actual/365 (basis 3) they differ materially (the ~cents
    // PRICE error, G6-03d). Excel's own COUPDAYSNC still PUBLISHES the actual
    // days at bases 2/3 — PRICE's internal DSC deliberately diverges from the
    // published COUP* function. (W109 G6-03d; b37/b38 live-Excel, build 20131.)
    let dsc = e - a;
    if yld <= -(c.frequency as f64) {
        return Err(derr(WorksheetErrorCode::Num));
    }
    let pw = |b: f64, ex: f64| {
        if binexp {
            excel_bond_pow(b, ex)
        } else {
            crt_pow(b, ex)
        }
    };
    let dirty = if p.n == 1 {
        let den = 1.0 + (yld / c.frequency as f64) * (dsc / e);
        if den <= 0.0 {
            return Err(derr(WorksheetErrorCode::Num));
        }
        (red + coup) / den
    } else {
        let base = 1.0 + yld / c.frequency as f64;
        if base <= 0.0 {
            return Err(derr(WorksheetErrorCode::Num));
        }
        let off = dsc / e;
        let mut pv = 0.0;
        for k in 0..p.n {
            pv += coup / pw(base, off + k as f64);
        }
        pv + red / pw(base, off + (p.n - 1) as f64)
    };
    let accr = coup * a / e;
    Ok((dirty - accr, dirty, coup, a, e))
}
fn solve(
    target: f64,
    f: i64,
    fun: impl Fn(f64) -> Result<f64, BondCoreEvalError>,
) -> Result<f64, BondCoreEvalError> {
    let low = -(f as f64) + 1e-10;
    // Price → +∞ as yield → -frequency, but the price kernel can overflow/reject
    // that near-degenerate endpoint. Treat an un-evaluable low-yield endpoint as
    // an effectively infinite price so the "is the target achievable" guard does
    // not spuriously fail a well-posed bond (BUG-FUNC-031). The bisection below
    // moves off `low` on its first step, so it never re-probes the endpoint.
    let lowp = fun(low).unwrap_or(f64::INFINITY);
    if target > lowp {
        return Err(derr(WorksheetErrorCode::Num));
    }
    let mut hi = 1.0;
    let mut hip = fun(hi)?;
    let mut g = 0;
    while hip > target && g < 80 {
        hi *= 2.0;
        hip = fun(hi)?;
        g += 1;
    }
    if hip > target {
        return Err(derr(WorksheetErrorCode::Num));
    }
    // Bisection to adjacent binary64 bounds (full convergence, dc-ifc: Excel's YIELD converges
    // past the former 1e-15 price tolerance), then the bound with the smaller residual.
    let mut lo = low;
    let mut hh = hi;
    for _ in 0..2200 {
        let mid = (lo + hh) / 2.0;
        if mid <= lo || mid >= hh {
            break;
        }
        let mp = fun(mid)?;
        if mp == target {
            return Ok(mid);
        }
        if mp > target { lo = mid } else { hh = mid }
    }
    let (lo_res, hi_res) = (fun(lo).map(|v| (v - target).abs()), fun(hh).map(|v| (v - target).abs()));
    Ok(match (lo_res, hi_res) {
        (Ok(a), Ok(b)) if a < b => lo,
        (Ok(_), Err(_)) => lo,
        _ => hh,
    })
}

pub fn accrintm_kernel(
    issue: f64,
    settlement: f64,
    rate_: f64,
    par: Option<f64>,
    basis_: Option<f64>,
) -> Result<f64, BondCoreEvalError> {
    let issue = dser(issue)?;
    let settlement = dser(settlement)?;
    // ACCRINTM needs a strictly positive rate (rate 0 is #NUM!), and equal issue and
    // settlement dates accrue 0 rather than #NUM! (live Excel 20430, W111-5 G8-06).
    let rate_ = pos(rate_)?;
    let par = pos(par.unwrap_or(1000.0))?;
    let basis_ = basis(basis_.unwrap_or(0.0))?;
    if issue > settlement {
        return Err(derr(WorksheetErrorCode::Num));
    }
    Ok(par * rate_ * yf(issue, settlement, basis_)?)
}
pub fn pricemat_kernel(
    settlement: f64,
    maturity: f64,
    issue: f64,
    rate_: f64,
    yld: f64,
    basis_: Option<f64>,
) -> Result<f64, BondCoreEvalError> {
    let settlement = dser(settlement)?;
    let maturity = dser(maturity)?;
    let issue = dser(issue)?;
    let rate_ = rate(rate_)?;
    let yld = rate(yld)?;
    let basis_ = basis(basis_.unwrap_or(0.0))?;
    if !(issue < settlement && settlement < maturity) {
        return Err(derr(WorksheetErrorCode::Num));
    }
    let b = days_in_year_for_mat(issue, settlement, basis_)?;
    let dim = dd(issue, maturity, basis_)?;
    let a = dd(issue, settlement, basis_)?;
    let dsm = dim - a;
    let future = 100.0 + (dim / b * rate_ * 100.0);
    let accrued = a / b * rate_ * 100.0;
    let den = 1.0 + (dsm / b * yld);
    if den <= 0.0 {
        return Err(derr(WorksheetErrorCode::Num));
    }
    Ok(future / den - accrued)
}
pub fn yieldmat_kernel(
    settlement: f64,
    maturity: f64,
    issue: f64,
    rate_: f64,
    price: f64,
    basis_: Option<f64>,
) -> Result<f64, BondCoreEvalError> {
    let settlement = dser(settlement)?;
    let maturity = dser(maturity)?;
    let issue = dser(issue)?;
    let rate_ = rate(rate_)?;
    let price = pos(price)?;
    let basis_ = basis(basis_.unwrap_or(0.0))?;
    if !(issue < settlement && settlement < maturity) {
        return Err(derr(WorksheetErrorCode::Num));
    }
    // W109 identification (2026-07-11, live build 20131, 1250/1250 bit-exact
    // incl. held-out sweep): the arithmetic is the legacy x87 spill loop —
    // every assignment double-rounded (`RN53(RN64(op))`) — with the PUBLISHED
    // formula's association: `term1 = (1 + DIM/B·rate) - term2`, reusing
    // `term2 = price/100 + A/B·rate` (the former F#-style left chain
    // `dim/b*rate + 1 - price/100 - a/b*rate` is 1-2 ULP off and ruled out).
    use crate::ported::excel_numeric::{excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_sub};
    let b = days_in_year_for_mat(issue, settlement, basis_)?;
    let dim = dd(issue, maturity, basis_)?;
    let a = dd(issue, settlement, basis_)?;
    let dsm = excel_x87_sub(dim, a);
    let dbr = excel_x87_mul(excel_x87_div(dim, b), rate_);
    let accr = excel_x87_mul(excel_x87_div(a, b), rate_);
    let p_norm = excel_x87_div(price, 100.0);
    let term2 = excel_x87_add(p_norm, accr);
    let term1 = excel_x87_sub(excel_x87_add(1.0, dbr), term2);
    let quotient = excel_x87_div(term1, term2);
    let year_ratio = excel_x87_div(b, dsm);
    Ok(excel_x87_mul(quotient, year_ratio))
}
pub fn yielddisc_kernel(
    settlement: f64,
    maturity: f64,
    price: f64,
    red: f64,
    basis_: Option<f64>,
) -> Result<f64, BondCoreEvalError> {
    let settlement = dser(settlement)?;
    let maturity = dser(maturity)?;
    let price = pos(price)?;
    let red = pos(red)?;
    let basis_ = basis(basis_.unwrap_or(0.0))?;
    if settlement >= maturity {
        return Err(derr(WorksheetErrorCode::Num));
    }
    // Faithful port of ExcelFinancialFunctions `yieldDisc` (bonds.fs):
    // `(redemption - pr) / pr * b / dim`, with `dim` the numerator day-count and
    // `b` DaysInYear. The prior `(red/price - 1) / yearfrac` form was algebraically
    // equal but ~5 ULP off (different operation order) and used the YEARFRAC
    // act/act algorithm rather than DaysBetween/DaysInYear.
    let dim = dd(settlement, maturity, basis_)?;
    let b = days_in_year_for_mat(settlement, maturity, basis_)?;
    // Excel groups the year ratio: ((red - price)/price)*(b/dim) (dc-j9k: 25 of 25 readings in
    // the catalogue oracle's sheet dcj9k_closed_form; the left-to-right form matched 17).
    Ok(((red - price) / price) * (b / dim))
}
pub fn price_kernel(
    settlement: f64,
    maturity: f64,
    rate_: f64,
    yld: f64,
    red: f64,
    frequency: f64,
    basis_: Option<f64>,
) -> Result<f64, BondCoreEvalError> {
    let c = ctx(settlement, maturity, frequency, basis_)?;
    let p = period(c)?;
    // PRICE uses Excel's C-runtime integer-`pow` discount staging (binexp). YIELD's solver
    // (below) still calls the legacy `pcomp` because its inversion schedule is not yet
    // identified and swapping the price kernel under it would regress the current witnesses.
    Ok(pcomp_disc(rate(rate_)?, rate(yld)?, pos(red)?, c, p, true)?.0)
}
pub fn yield_kernel(
    settlement: f64,
    maturity: f64,
    rate_: f64,
    price: f64,
    red: f64,
    frequency: f64,
    basis_: Option<f64>,
) -> Result<f64, BondCoreEvalError> {
    let c = ctx(settlement, maturity, frequency, basis_)?;
    let p = period(c)?;
    let rate_ = rate(rate_)?;
    let price = pos(price)?;
    let red = pos(red)?;
    if p.n == 1 {
        let coup = 100.0 * rate_ / c.frequency as f64;
        let e = dc(p.prev, p.next, c.basis, c.frequency)?;
        let a = dd(p.prev, c.settlement, c.basis)?;
        let dsc = dd(c.settlement, p.next, c.basis)?;
        return Ok((((red + coup) / (price + coup * a / e)) - 1.0) * c.frequency as f64 * e / dsc);
    }
    // Solve over candidate yields with `pcomp` directly. `price_kernel` rejects
    // `yld < 0` via `rate(yld)`, but the root-finder must probe negative candidate
    // yields (its bracket runs down to `-frequency`); `pcomp`'s own guards
    // (`yld <= -frequency`, `base <= 0`) keep the domain correct (BUG-FUNC-031).
    solve(price, c.frequency, |cand| {
        Ok(pcomp(rate_, cand, red, c, p)?.0)
    })
}
pub fn duration_kernel(
    settlement: f64,
    maturity: f64,
    coupon: f64,
    yld: f64,
    frequency: f64,
    basis_: Option<f64>,
) -> Result<f64, BondCoreEvalError> {
    let c = ctx(settlement, maturity, frequency, basis_)?;
    let p = period(c)?;
    let coupon = rate(coupon)?;
    let yld = rate(yld)?;
    let f = c.frequency as f64;
    // W109 G6-03c identification (live Excel build 20131, b44 6,360 witnesses).
    // The weighting exponent shares PRICE's schedule quantities: the internal
    // settlement fraction is the DERIVED complement `dsc = E - A`, so `off =
    // (E - A)/E` (NOT the direct settlement->next day-count span the docs-era
    // kernel used — that made bases 2/3 off by cents, 0/1272 exact). The
    // discount factor is Excel's C-runtime `pow` staging (`excel_bond_pow`:
    // binary exponentiation on the on-coupon integer exponents, the legacy x87
    // `exp(RN53(RN64(exp*ln)))` chain off-coupon) — the SAME substrate PRICE
    // uses. The Macaulay body op-graph is a plain-`f64` (SSE2) accrual with the
    // redemption as a SEPARATE term and the numerator weight grouped
    // `(diff*cash)/disc` (left-assoc), NOT `diff*(cash/disc)`; the final
    // `num/den/f` association. This is the one internally-consistent kernel:
    // numerator and denominator discount the SAME cashflows with the SAME `off`.
    let coup = 100.0 * coupon / f;
    let e = dc(p.prev, p.next, c.basis, c.frequency)?;
    // The accrued span A is `CoupDaysBS` (F# `IDayCount.DaysBetween`, numerator
    // position): for US 30/360 this is `dateDiff360US ... ModifyStartDate`
    // (`diff360_us(_, _, false)`), NOT the plain `us_30_360` (`dd`). The two
    // diverge only on 31st/month-end settlements — e.g. a Feb-end previous coupon
    // with a 31st settlement: plain `us_30_360` collapses the end 31->30 AFTER
    // the Feb-end start adjustment (giving 30), while `ModifyStartDate` checks
    // `end==31` BEFORE adjusting the start (giving 31, matching Excel). This is
    // the W109 G6-03c b45 month-end break (the ~2.5e13-ULP FC-45747-b0 explosion).
    let a = days_between_num(p.prev, c.settlement, c.basis)?;
    let off = (e - a) / e;
    if p.n == 1 {
        return Ok((off / f).max(0.0));
    }
    let base = 1.0 + yld / f;
    let mut num = 0.0;
    let mut den = 0.0;
    for k in 0..p.n {
        let diff = off + k as f64;
        let disc = excel_bond_pow(base, diff);
        num += diff * coup / disc;
        den += coup / disc;
        if k + 1 == p.n {
            num += diff * 100.0 / disc;
            den += 100.0 / disc;
        }
    }
    Ok(num / den / f)
}
pub fn mduration_kernel(
    settlement: f64,
    maturity: f64,
    coupon: f64,
    yld: f64,
    frequency: f64,
    basis_: Option<f64>,
) -> Result<f64, BondCoreEvalError> {
    let f = freq(frequency)? as f64;
    Ok(
        duration_kernel(settlement, maturity, coupon, yld, frequency, basis_)?
            / (1.0 + rate(yld)? / f),
    )
}
fn is_month_end(s: i64) -> bool {
    ymd_from_excel_serial(WorkbookDateSystem::System1900, s as f64)
        .map(|(y, mo, d)| d == dim(y, mo))
        .unwrap_or(false)
}
fn last_day_of_feb(s: i64) -> bool {
    ymd_from_excel_serial(WorkbookDateSystem::System1900, s as f64)
        .map(|(y, mo, d)| mo == 2 && d == dim(y, mo))
        .unwrap_or(false)
}
/// F# `changeMonth` with an explicit return-last-day flag (.NET `AddMonths`
/// semantics, or force the target month-end when `last_day` is set).
fn change_month_flag(s: i64, months: i64, last_day: bool) -> Option<i64> {
    let (y, mo, d) = ymd_from_excel_serial(WorkbookDateSystem::System1900, s as f64)?;
    let idx = y
        .checked_mul(12)?
        .checked_add(mo - 1)?
        .checked_add(months)?;
    let ty = idx.div_euclid(12);
    let tm = idx.rem_euclid(12) + 1;
    let td = if last_day {
        dim(ty, tm)
    } else {
        d.min(dim(ty, tm))
    };
    excel_serial_from_ymd(WorkbookDateSystem::System1900, ty, tm, td).map(|v| v as i64)
}
/// F# `dateDiff360Us` (US 30/360) with the `ModifyStartDate`/`ModifyBothDates` modes.
fn diff360_us(s: i64, e: i64, modify_both: bool) -> Result<f64, BondCoreEvalError> {
    let (sy, sm, mut sd) = ymd_from_excel_serial(WorkbookDateSystem::System1900, s as f64)
        .ok_or(derr(WorksheetErrorCode::Value))?;
    let (ey, em, mut ed) = ymd_from_excel_serial(WorkbookDateSystem::System1900, e as f64)
        .ok_or(derr(WorksheetErrorCode::Value))?;
    if last_day_of_feb(e) && (last_day_of_feb(s) || modify_both) {
        ed = 30;
    }
    if ed == 31 && (sd >= 30 || modify_both) {
        ed = 30;
    }
    if sd == 31 {
        sd = 30;
    }
    if last_day_of_feb(s) {
        sd = 30;
    }
    Ok(((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64)
}
/// F# `findPcdNcd`: walk `start` toward `end` by `num_months` steps, returning the
/// bounding pair (the position at/past `end`, and the one before it).
fn find_pcd_ncd_accr(
    start: i64,
    end: i64,
    num_months: i64,
    last_day: bool,
) -> Result<(i64, i64), BondCoreEvalError> {
    let mut front = start;
    let mut trailing = end;
    loop {
        let stop = if num_months > 0 {
            front >= end
        } else {
            front <= end
        };
        if stop {
            return Ok((front, trailing));
        }
        trailing = front;
        front =
            change_month_flag(front, num_months, last_day).ok_or(derr(WorksheetErrorCode::Num))?;
    }
}
/// F# `IDayCount.DaysBetween` (numerator position) per basis.
fn days_between_num(s: i64, e: i64, b: DayCountBasis) -> Result<f64, BondCoreEvalError> {
    match b {
        DayCountBasis::Us30_360 => diff360_us(s, e, false),
        DayCountBasis::ActualActual | DayCountBasis::Actual360 | DayCountBasis::Actual365 => {
            Ok(act(s, e))
        }
        DayCountBasis::European30_360 => d360eu(s, e),
    }
}
/// F# `IDayCount.DaysBetween` (denominator position) per basis — only the
/// ActualActual / Actual360 / European cases are reached (the kernel handles
/// Us30_360 and Actual365 inline).
fn days_between_denum(s: i64, e: i64, b: DayCountBasis) -> Result<f64, BondCoreEvalError> {
    match b {
        DayCountBasis::ActualActual => Ok(act(s, e)),
        DayCountBasis::Actual360 => diff360_us(s, e, false),
        DayCountBasis::European30_360 => d360eu(s, e),
        DayCountBasis::Us30_360 => diff360_us(s, e, false),
        DayCountBasis::Actual365 => Ok(act(s, e)),
    }
}
/// F# `actualCoupDays pcd firstInterest`: the actual length of the coupon period
/// (on `first`'s schedule) that bounds `pcd`. Works in both regimes — `pcd` a
/// period before `first` (odd first coupon) or after it (settlement past the first
/// interest date), where the bounding period differs.
fn actual_coup_days_accr(
    pcd: i64,
    first: i64,
    num_months: i64,
    last_day: bool,
) -> Result<f64, BondCoreEvalError> {
    let (prev, next) = find_pcd_ncd_accr(first, pcd, -num_months, last_day)?;
    Ok(act(prev, next))
}
/// F# `IDayCount.CoupDays pcd firstInterest`: the normal length of the coupon
/// period anchored at `pcd`. ActualActual uses the actual bounding-period days; the
/// 30/360 bases use `360/freq`, Actual/365 uses `365/freq`.
fn coup_days_accr(
    pcd: i64,
    first: i64,
    num_months: i64,
    last_day: bool,
    fc: f64,
    b: DayCountBasis,
) -> Result<f64, BondCoreEvalError> {
    match b {
        DayCountBasis::ActualActual => actual_coup_days_accr(pcd, first, num_months, last_day),
        DayCountBasis::Actual365 => Ok(365.0 / fc),
        _ => Ok(360.0 / fc),
    }
}

/// The coupon period `[B0, B1]` on `first`'s grid schedule that contains `issue`
/// (`B0 <= issue < B1`). Uses a STRICT lower step so an `issue` landing exactly on a grid
/// boundary yields `B1 == issue` (a zero-length stub — the whole periods are then skipped).
fn issue_period_grid(
    issue: i64,
    first: i64,
    num_months: i64,
    last_day: bool,
) -> Result<(i64, i64), BondCoreEvalError> {
    let mut b1 = first;
    while b1 > issue {
        b1 = change_month_flag(b1, -num_months, last_day).ok_or(derr(WorksheetErrorCode::Num))?;
    }
    while b1 < issue {
        b1 = change_month_flag(b1, num_months, last_day).ok_or(derr(WorksheetErrorCode::Num))?;
    }
    let b0 = change_month_flag(b1, -num_months, last_day).ok_or(derr(WorksheetErrorCode::Num))?;
    Ok((b0, b1))
}

pub fn accrint_kernel(
    issue: f64,
    first_interest: f64,
    settlement: f64,
    rate_: f64,
    par: Option<f64>,
    frequency: f64,
    basis_: Option<f64>,
    calc_method: Option<bool>,
) -> Result<f64, BondCoreEvalError> {
    let issue = dser(issue)?;
    let first = dser(first_interest)?;
    let settlement = dser(settlement)?;
    let rate_ = rate(rate_)?;
    let par = pos(par.unwrap_or(1000.0))?;
    let f = freq(frequency)?;
    let basis_ = basis(basis_.unwrap_or(0.0))?;
    // calc_method: TRUE = FromIssueToSettlement (Excel default), FALSE = FromFirstToSettlement.
    let from_issue = calc_method.unwrap_or(true);
    if !(issue < first && issue < settlement) {
        return Err(derr(WorksheetErrorCode::Num));
    }
    // Identified against 145,620 live-Excel witnesses (W109 G6-02, agent-W; b39/b40/b42),
    // 99.99% bit-exact — plain SSE2 `f64` throughout (x87 emulation is strictly worse).
    // Excel normalises the settlement-side fraction by the *canonical* last coupon period
    // length `CoupDays(first - 1 period, first)` (fixes the ~0.07% act/act error of the prior
    // kernel — BUG-FUNC-030). Two calc_method paths, both accruing from `issue`:
    //  * calc_method TRUE  (from_issue) => period-aware WALK: interior whole periods = 1.0,
    //    the act/act issue stub uses its own actual length, and the settlement-side period is
    //    always days/canonical (stays fractional even when settle is on a coupon date). The
    //    forward-collected terms are summed BACKWARD (settlement -> issue).
    //  * calc_method FALSE            => FLAT days(issue->settle)/canonical, EXCEPT when issue
    //    sits in a coupon period earlier than `pcd`: the accrual then counts only the stub in
    //    issue's own grid period plus the span from `pcd` to settle, and SKIPS every whole
    //    coupon period between them (the defining legacy calc_method=FALSE behaviour).
    let fc = f as f64;
    let num_months = 12 / f;
    let end_flag = is_month_end(first);
    let pcd =
        change_month_flag(first, -num_months, end_flag).ok_or(derr(WorksheetErrorCode::Num))?;
    let canonical = coup_days_accr(pcd, first, num_months, end_flag, fc, basis_)?;
    // W109 G6-02 publication identification (build 20228/CV2 NoCache): the
    // coupon and accrual fraction are both stored binary64 values, but their
    // final product is an x87 PC64 multiply followed by the binary64 result
    // store. This rare double-rounding site is decisive on the b43 rate ladder
    // and the frozen publication held-out; all earlier day-count arithmetic
    // remains the identified plain-SSE2 graph.
    let coupon = par * rate_ / fc;

    if !from_issue {
        // ---- calc_method FALSE: FLAT fraction (with the whole-period skip). ----
        let a = if issue < pcd {
            // issue in a coupon period earlier than the canonical (last-before-first) period.
            // Accrue the stub in issue's own grid period [B0,B1]; SKIP the whole grid periods
            // [B1, pcd] entirely; measure the remainder from pcd. Two divisions, summed.
            let (b0, b1) = issue_period_grid(issue, first, num_months, end_flag)?;
            let l_issue = match basis_ {
                DayCountBasis::ActualActual => act(b0, b1),
                DayCountBasis::Actual365 => 365.0 / fc,
                _ => 360.0 / fc,
            };
            let stub = days_between_num(issue, b1, basis_)? / l_issue;
            let rest = days_between_num(pcd, settlement, basis_)? / canonical;
            stub + rest
        } else {
            days_between_num(issue, settlement, basis_)? / canonical
        };
        return Ok(crate::ported::excel_numeric::excel_x87_mul(coupon, a));
    }

    // ---- calc_method TRUE: period walk accruing from issue. ----
    if settlement <= first {
        let first_date = issue.max(pcd);
        let days = days_between_num(first_date, settlement, basis_)?;
        let mut a = days / canonical;
        // datesAggregate1: walk pcd back toward issue, one quasi-coupon period per step.
        let mut front = pcd;
        while front > issue {
            let ncd_i = front;
            let pcd_i = change_month_flag(front, -num_months, end_flag)
                .ok_or(derr(WorksheetErrorCode::Num))?;
            front = pcd_i;
            if issue <= pcd_i {
                a += 1.0;
            } else {
                // The period that contains `issue`: a partial forward fraction.
                let fd = issue.max(pcd_i);
                let days_i = match basis_ {
                    DayCountBasis::Us30_360 => diff360_us(fd, ncd_i, false)?,
                    _ => days_between_num(fd, ncd_i, basis_)?,
                };
                let coup_days_i = match basis_ {
                    DayCountBasis::Us30_360 => diff360_us(pcd_i, ncd_i, true)?,
                    DayCountBasis::ActualActual => act(pcd_i, ncd_i),
                    DayCountBasis::Actual365 => 365.0 / fc,
                    _ => days_between_denum(pcd_i, ncd_i, basis_)?,
                };
                a += days_i / coup_days_i;
            }
        }
        return Ok(crate::ported::excel_numeric::excel_x87_mul(coupon, a));
    }

    // Settlement past the first interest date: forward-collect the period terms, then sum
    // them BACKWARD (settlement side first — the direction Excel accumulates). Interior
    // COMPLETE periods contribute exactly 1.0; the act/act issue stub uses its own actual
    // length; the FINAL period touching settlement is always days/canonical (it stays
    // fractional even when settlement lands exactly on the period-end coupon date).
    let mut p_start = first;
    while p_start > issue {
        p_start = change_month_flag(p_start, -num_months, end_flag)
            .ok_or(derr(WorksheetErrorCode::Num))?;
    }
    let mut terms: Vec<f64> = Vec::new();
    while p_start < settlement {
        let p_end = change_month_flag(p_start, num_months, end_flag)
            .ok_or(derr(WorksheetErrorCode::Num))?;
        let ov_start = issue.max(p_start);
        let ov_end = settlement.min(p_end);
        if ov_end > ov_start {
            let is_last = ov_end == settlement;
            if ov_start == p_start && ov_end == p_end && !is_last {
                terms.push(1.0);
            } else {
                let denom = if matches!(basis_, DayCountBasis::ActualActual)
                    && ov_start == issue
                    && issue > p_start
                {
                    act(p_start, p_end)
                } else {
                    canonical
                };
                terms.push(days_between_num(ov_start, ov_end, basis_)? / denom);
            }
        }
        p_start = p_end;
    }
    let mut a = 0.0;
    for t in terms.iter().rev() {
        a += *t;
    }
    Ok(crate::ported::excel_numeric::excel_x87_mul(coupon, a))
}
pub fn map_bond_core_error_to_ws(e: &BondCoreEvalError) -> WorksheetErrorCode {
    match e {
        BondCoreEvalError::ArityMismatch { .. } => WorksheetErrorCode::Value,
        BondCoreEvalError::Domain(c) => *c,
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText};
    use super::*;

    fn serial(y: i64, m: i64, d: i64) -> f64 {
        excel_serial_from_ymd(WorkbookDateSystem::System1900, y, m, d).unwrap()
    }
    fn num(n: f64) -> CalcValue {
        CalcValue::number(n)
    }
    struct Dummy;
    fn close(a: f64, b: f64, t: f64) {
        assert!((a - b).abs() <= t, "a={a}, b={b}");
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yielddisc_bit_exact_vs_excel() {
        // G6 three-way: the prior `(red/price - 1) / yearfrac` form was ~5 ULP off
        // Excel. The faithful `yieldDisc` port `(red - pr)/pr * b / dim` is now
        // bit-exact with live Excel 16.0 b20026 and the F# reference. Value pinned
        // by exact bits.
        let y = yielddisc_kernel(44013.0, 44562.0, 95.0, 100.0, Some(0.0))
            .expect("yielddisc should succeed");
        assert_eq!(y.to_bits(), 0.035_087_719_298_245_61_f64.to_bits());
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn d360us_rolls_end_day_31_into_next_month() {
        assert_eq!(
            d360us(serial(2023, 11, 15) as i64, serial(2024, 1, 31) as i64),
            Ok(76.0)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn d360us_start_at_least_30_collapses_end_in_place() {
        assert_eq!(
            d360us(serial(2024, 1, 30) as i64, serial(2024, 3, 31) as i64),
            Ok(60.0)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn accrint_leap_february_and_settlement_after_first_bit_exact() {
        // BUG-FUNC-030 residual closed by the faithful F# accrInt port: act/act and
        // act/365 partial periods crossing leap February, and settlement past the first
        // interest date (which F# itself rejects), all match live Excel 16.0 b20026
        // bit-for-bit. Bits pinned from the G6 three-way ledger.
        let p1 = accrint_kernel(
            serial(2019, 3, 1),
            serial(2020, 9, 1),
            serial(2020, 1, 15),
            0.05,
            Some(1000.0),
            2.0,
            Some(1.0),
            None,
        )
        .unwrap();
        assert_eq!(p1.to_bits(), 0x4045_e000_0000_0000); // 43.75, act/act
        let p3 = accrint_kernel(
            serial(2019, 3, 1),
            serial(2020, 9, 1),
            serial(2020, 1, 15),
            0.05,
            Some(1000.0),
            2.0,
            Some(3.0),
            None,
        )
        .unwrap();
        assert_eq!(p3.to_bits(), 0x4045_d96c_b65b_2d97); // act/365
        let after = accrint_kernel(
            serial(2019, 1, 1),
            serial(2020, 1, 1),
            serial(2020, 6, 1),
            0.05,
            Some(1000.0),
            2.0,
            Some(1.0),
            None,
        )
        .unwrap();
        assert_eq!(after.to_bits(), 0x4051_a9bd_37a6_f4df); // settlement after first_interest
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn accrint_staging_bit_exact_vs_excel_w109() {
        // W109 G6-02 (agent-W): staging identification across b39/b40/b42 live-Excel
        // corpora (145,620 witnesses, 99.99% bit-exact). Each pin is a live Excel 16.0
        // witness, not a computed value. `k` = accrint_kernel.
        let k = |i, f, s, r, par, fr, b, c| {
            accrint_kernel(i, f, s, r, Some(par), fr, Some(b), Some(c))
                .unwrap()
                .to_bits()
        };
        // Historic BUG-FUNC-030 catalog family; calc TRUE vs FALSE differ by exactly 1 ULP
        // (the pair pins the calc_method staging discriminator).
        let (i, f, s) = (serial(2019, 4, 10), serial(2019, 7, 1), serial(2020, 3, 15));
        assert_eq!(
            k(i, f, s, 0.05, 997.5, 2.0, 0.0, true),
            0x4047_34aa_aaaa_aaaa
        );
        assert_eq!(
            k(i, f, s, 0.05, 997.5, 2.0, 0.0, false),
            0x4047_34aa_aaaa_aaab
        );
        // calc FALSE, issue 3 periods before pcd (quarterly, act/360): the whole-period skip
        // makes accrual negative for a settlement chronologically after issue.
        assert_eq!(
            k(
                serial(2020, 2, 20),
                serial(2021, 1, 1),
                serial(2020, 7, 1),
                0.05,
                997.5,
                4.0,
                2.0,
                false
            ),
            0xc01c_4333_3333_3333
        );
        // calc FALSE, issue exactly on a grid coupon date (zero stub, all wholes skipped).
        assert_eq!(
            k(
                serial(2020, 4, 1),
                serial(2021, 1, 1),
                serial(2020, 4, 2),
                0.05,
                997.5,
                4.0,
                0.0,
                false
            ),
            0xc038_cc88_8888_8889
        );
        // calc TRUE, act/act annual across a leap interior period (canonical != interior len).
        assert_eq!(
            k(
                serial(2019, 3, 1),
                serial(2021, 3, 1),
                serial(2021, 10, 6),
                0.037,
                1000.0,
                1.0,
                1.0,
                true
            ),
            0x4058_0ccc_cccc_cccd
        );
        // calc TRUE, long forward walk (backward accumulation of many whole periods).
        assert_eq!(
            k(
                serial(2018, 5, 20),
                serial(2018, 11, 1),
                serial(2019, 7, 3),
                0.05,
                997.5,
                2.0,
                0.0,
                true
            ),
            0x404b_ea88_8888_8889
        );
        // calc TRUE, month-end + leap Feb, settlement exactly on a coupon anniversary
        // (final period stays fractional).
        assert_eq!(
            k(
                serial(2019, 2, 28),
                serial(2019, 8, 31),
                serial(2020, 8, 31),
                0.05,
                1000.0,
                2.0,
                0.0,
                true
            ),
            0x4052_c8e3_8e38_e38e
        );
        // calc FALSE, act/act quarterly issue stub measured by its own actual period length.
        assert_eq!(
            k(
                serial(2019, 3, 11),
                serial(2019, 7, 1),
                serial(2019, 7, 4),
                0.05,
                997.5,
                4.0,
                1.0,
                false
            ),
            0x402f_940f_c0fc_0fc2
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn accrint_x87_final_publication_matches_live_excel_w109() {
        // W109 G6-02 final-publication identification. The stored coupon and
        // stored accrual fraction are multiplied through RN53(RN64(coupon*a)).
        // These live-Excel build-20228/CV2 pins include the b43 isolated-rate
        // discriminator plus rows from the frozen 450-row NoCache held-out.
        let cases = [
            (
                [
                    0x40e5_41e0_0000_0000,
                    0x40e5_4fe0_0000_0000,
                    0x40e5_6780_0000_0000,
                    0x3faf_7ced_9168_72b0,
                    0x408f_2c00_0000_0000,
                    0x4010_0000_0000_0000,
                    0x4008_0000_0000_0000,
                    0x3ff0_0000_0000_0000,
                ],
                0x4049_3095_55ee_8cc8,
            ),
            (
                [
                    0x40e5_3d20_0000_0000,
                    0x40e5_6ac0_0000_0000,
                    0x40e5_b900_0000_0000,
                    0x3faf_7ced_9168_72b0,
                    0x408f_2c00_0000_0000,
                    0x3ff0_0000_0000_0000,
                    0x3ff0_0000_0000_0000,
                    0x0000_0000_0000_0000,
                ],
                0x4064_d1e5_8b59_214c,
            ),
            (
                [
                    0x40e5_4ca0_0000_0000,
                    0x40e5_b860_0000_0000,
                    0x40e6_10c0_0000_0000,
                    0x3faf_7ced_9168_72e0,
                    0x40a4_9269_68d9_5565,
                    0x4010_0000_0000_0000,
                    0x0000_0000_0000_0000,
                    0x3ff0_0000_0000_0000,
                ],
                0x4085_bb9c_e8bb_9d6c,
            ),
            (
                [
                    0x40e4_be40_0000_0000,
                    0x40e5_16e0_0000_0000,
                    0x40e4_e460_0000_0000,
                    0x3fb8_59d7_ff5d_f323,
                    0x4091_45e9_f238_5198,
                    0x4010_0000_0000_0000,
                    0x4000_0000_0000_0000,
                    0x0000_0000_0000_0000,
                ],
                0xc051_d15a_0e17_c888,
            ),
            (
                [
                    0x40e6_65e0_0000_0000,
                    0x40e6_7de0_0000_0000,
                    0x40e7_13e0_0000_0000,
                    0x3fc4_c62c_5266_ded7,
                    0x40c1_a670_15f9_623c,
                    0x3ff0_0000_0000_0000,
                    0x3ff0_0000_0000_0000,
                    0x3ff0_0000_0000_0000,
                ],
                0x40b5_d566_21fe_8dba,
            ),
            (
                [
                    0x40e5_d840_0000_0000,
                    0x40e6_1d60_0000_0000,
                    0x40e5_f840_0000_0000,
                    0x3fa9_9999_9999_999a,
                    0x40b5_621e_068e_c68e,
                    0x4000_0000_0000_0000,
                    0x3ff0_0000_0000_0000,
                    0x0000_0000_0000_0000,
                ],
                0xc054_415a_f4e8_4828,
            ),
        ];
        for (args, expected) in cases {
            let a = args.map(f64::from_bits);
            let got = accrint_kernel(
                a[0],
                a[1],
                a[2],
                a[3],
                Some(a[4]),
                a[5],
                Some(a[6]),
                Some(a[7] != 0.0),
            )
            .expect("valid ACCRINT publication witness");
            assert_eq!(got.to_bits(), expected);
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn price_yield_round_trip() {
        let s = serial(2029, 6, 15);
        let m = serial(2029, 11, 15);
        let p = price_kernel(s, m, 0.0575, 0.065, 100.0, 2.0, Some(0.0)).unwrap();
        close(
            yield_kernel(s, m, 0.0575, p, 100.0, 2.0, Some(0.0)).unwrap(),
            0.065,
            1e-10,
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yield_converges_for_well_posed_multi_period_discount_bond() {
        // BUG-FUNC-031: the root-finder probes negative candidate yields (its
        // bracket runs to -frequency), which price_kernel rejected via rate(yld),
        // so a well-posed discount bond returned #NUM!. Solving over pcomp directly
        // (its own yld<=-freq / base<=0 guards handle the domain) fixes it. Excel
        // 16.0 b20026 returns ~0.0862487 for this 3-coupon bond priced at 95.
        let y = yield_kernel(
            serial(2020, 7, 1),
            serial(2022, 1, 1),
            0.05,
            95.0,
            100.0,
            2.0,
            Some(0.0),
        )
        .expect("yield converges for a well-posed discount bond");
        close(y, 0.08624873995, 1e-7);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn duration_relation() {
        let s = serial(2024, 3, 15);
        let m = serial(2031, 11, 15);
        let d = duration_kernel(s, m, 0.06, 0.0675, 2.0, Some(0.0)).unwrap();
        let md = mduration_kernel(s, m, 0.06, 0.0675, 2.0, Some(0.0)).unwrap();
        close(md, d / (1.0 + 0.0675 / 2.0), 1e-12);
    }
    /// W109 YIELDMAT identification pins — live Excel 16.0 build 20131 bits.
    /// Rows: the two former catalog witnesses (basis 1 and 0, previously 1 and
    /// 2 ULP off), and a constructed double-rounding-window row that separates
    /// the spill-loop arithmetic from strict staging. The 1,250-row live sweep
    /// is replayed via the racer work dir (race-validation.json).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yieldmat_matches_live_excel_pinned_witnesses() {
        let rows: &[(u64, u64, u64, u64, u64, f64, u64)] = &[
            // settlement, maturity, issue, rate, price bits; basis; excel bits
            (
                0x40e6324000000000, // 45458
                0x40e678c000000000, // 46022
                0x40e61d8000000000, // 45292
                0x3faae147ae147ae1, // 0.0525
                0x4058a6477d72f020, // 98.59811340546048
                1.0,
                0x3faf3b645a1cabfe,
            ),
            (
                0x40e6324000000000,
                0x40e678c000000000,
                0x40e61d8000000000,
                0x3faae147ae147ae1,
                0x4058a6477d72f020,
                0.0,
                0x3faf37e9d4b23782,
            ),
            // spill-loop discriminator (basis 2, window row win-rand-10280)
            (
                0x40dfca8000000000,
                0x40e0a16000000000,
                0x40deedc000000000,
                0x3fb88743a94c782d,
                0x4048d9ae489dbe14,
                2.0,
                0x3fd2e57422c16238,
            ),
        ];
        for (i, &(s, m, iss, r, p, basis, excel)) in rows.iter().enumerate() {
            let got = yieldmat_kernel(
                f64::from_bits(s),
                f64::from_bits(m),
                f64::from_bits(iss),
                f64::from_bits(r),
                f64::from_bits(p),
                Some(basis),
            )
            .unwrap_or_else(|e| panic!("pin {i}: unexpected error {e:?}"));
            assert_eq!(
                got.to_bits(),
                excel,
                "pin {i}: got 0x{:016x} want 0x{excel:016x}",
                got.to_bits()
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn mat_round_trip() {
        let s = serial(2024, 6, 15);
        let m = serial(2025, 12, 31);
        let i = serial(2024, 1, 1);
        let p = pricemat_kernel(s, m, i, 0.0525, 0.061, Some(1.0)).unwrap();
        close(p, 98.598_113_405_460_48, 1e-12);
        close(
            yieldmat_kernel(s, m, i, 0.0525, p, Some(1.0)).unwrap(),
            0.061,
            1e-12,
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn accrint_slices() {
        // BUG-FUNC-030 witness: an odd first coupon spanning two quasi-coupon
        // periods must sum over the periods, not interpolate issue->first once.
        // Excel 16.0 build 20026 returns 25 exactly; the old kernel returned 12.5.
        close(
            accrint_kernel(
                serial(2020, 1, 1),
                serial(2021, 1, 1),
                serial(2020, 7, 1),
                0.05,
                Some(1000.0),
                2.0,
                Some(0.0),
                Some(true),
            )
            .unwrap(),
            25.0,
            1e-9,
        );

        // Regular one-period first coupon: calc_method TRUE and FALSE coincide
        // (Excel-verified) because FALSE accrues from one period before
        // first_interest, which equals issue here.
        let i = serial(2024, 1, 1);
        let f = serial(2024, 7, 1);
        let s = serial(2024, 10, 1);
        close(
            accrint_kernel(i, f, s, 0.12, Some(1000.0), 2.0, Some(0.0), Some(true)).unwrap(),
            accrint_kernel(i, f, s, 0.12, Some(1000.0), 2.0, Some(0.0), Some(false)).unwrap(),
            1e-12,
        );

        // Long (multi-period) first coupon: TRUE accrues from issue, FALSE from
        // one period before first_interest, so TRUE strictly exceeds FALSE.
        let (li, lf, ls) = (serial(2018, 1, 1), serial(2020, 1, 1), serial(2020, 7, 15));
        assert!(
            accrint_kernel(li, lf, ls, 0.06, Some(1000.0), 2.0, Some(0.0), Some(true)).unwrap()
                > accrint_kernel(li, lf, ls, 0.06, Some(1000.0), 2.0, Some(0.0), Some(false))
                    .unwrap()
        );

        close(
            accrintm_kernel(i, serial(2024, 10, 1), 0.08, Some(1000.0), Some(3.0)).unwrap(),
            1000.0 * 0.08 * (act(i as i64, serial(2024, 10, 1) as i64) / 365.0),
            1e-12,
        );
    }

    // W109: PRICE bit-exact vs live Excel (build 20131) after the C-runtime integer-`pow`
    // (binexp) discount fix. 25 points over 5 bonds (on-coupon short/long + fractional-off),
    // the same identification+held-out ladders that validated `excel_bond_pow` (25/25).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn price_binexp_matches_excel_ladders() {
        // (settle, mat, rate, red, [(yld, excel_price_bits)])
        let ladders: &[(f64, f64, f64, f64, &[(f64, u64)])] = &[
            (
                44013.0,
                44562.0,
                0.05,
                100.0,
                &[
                    (0.04, 0x40595c48c592b01e),
                    (0.05, 0x4059000000000000),
                    (0.06, 0x4058a57c040a3442),
                    (0.08, 0x4057f5975cde5332),
                    (0.10, 0x40574c47c2be592c),
                ],
            ),
            (
                44058.0,
                44562.0,
                0.05,
                100.0,
                &[
                    (0.04, 0x405954ad19241473),
                    (0.05, 0x4058ffa2cc4ca8d4),
                    (0.06, 0x4058ac20a4792f7e),
                    (0.08, 0x405809914321b82a),
                    (0.10, 0x40576cba5c994db4),
                ],
            ),
            (
                44013.0,
                46753.0,
                0.05,
                100.0,
                &[
                    (0.04, 0x405a9b2d2aa614e0),
                    (0.05, 0x4059000000000004),
                    (0.06, 0x405781fc6f8e90d4),
                    (0.08, 0x4054d4a282adf7b8),
                    (0.10, 0x4052834134edf821),
                ],
            ),
            (
                44013.0,
                47119.0,
                0.075,
                102.0,
                &[
                    (0.03, 0x4060e30a168e62dc),
                    (0.05, 0x405d9d18c6e14303),
                    (0.07, 0x405a11be4337deae),
                    (0.09, 0x40570a9f1350b950),
                    (0.11, 0x405472ad4fab7123),
                ],
            ),
            (
                44094.0,
                45658.0,
                0.06,
                103.0,
                &[
                    (0.03, 0x405ca695b231486a),
                    (0.05, 0x405a8ebb3948baab),
                    (0.07, 0x4058a4f75d9fc866),
                    (0.09, 0x4056e4e04e91c98c),
                    (0.11, 0x40554a8265f06f2f),
                ],
            ),
        ];
        for (s, m, rate, red, pts) in ladders {
            for (yld, exp) in *pts {
                let got = price_kernel(*s, *m, *rate, *yld, *red, 2.0, Some(0.0)).unwrap();
                assert_eq!(
                    got.to_bits(),
                    *exp,
                    "PRICE({s},{m},{rate},{yld},{red},2,0) = {:#018x}, expected {exp:#018x}",
                    got.to_bits()
                );
            }
        }
    }

    // W109 G6-03d: PRICE bit-exact vs live Excel (build 20131) after the universal
    // `dsc = E - A` discount-fraction rule + the fractional x87 `pow` chain
    // (`excel_pow_chain`). Held out: b37 bases 2/3 3656+3658 (14 open ±1-ULP
    // extreme-yield residuals), b38 fresh 945/945 all bases. Bits from answers-b37-price.json.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn price_dsc_e_minus_a_and_x87_pow_chain_pins() {
        // (settle, mat, rate, yld, red, freq, basis, excel_bits)
        let rows: &[(f64, f64, f64, f64, f64, f64, f64, u64)] = &[
            // Catalog witness G6-03d (basis 2, Actual/360): was ~cents wrong pre-fix.
            (
                44094.0,
                45658.0,
                0.06,
                0.03,
                103.0,
                2.0,
                2.0,
                0x405ca5adc69c74fb,
            ),
            // Basis-3 (Actual/365) sibling of the catalog witness.
            (
                44094.0,
                45658.0,
                0.06,
                0.03,
                103.0,
                2.0,
                3.0,
                0x405ca62e6ffeec41,
            ),
            // Settlement-on-31st (2020-07-31) US 30/360 (basis 0): the `E - A` rule
            // also corrects the 30/360 last-day-of-month accrual/discount split.
            (
                44043.0,
                45658.0,
                0.06,
                0.03,
                103.0,
                2.0,
                0.0,
                0x405cbcd7f4f1f43d,
            ),
            // Pow-chain discriminator (basis 3, yld 0.2): platform `powf` is 1 ULP off
            // here; the x87 `exp(RN53(RN64(exp·ln base)))` chain reproduces Excel.
            (
                44094.0,
                45658.0,
                0.06,
                0.2,
                103.0,
                2.0,
                3.0,
                0x404f217d3fb8f25d,
            ),
        ];
        for &(s, m, r, y, red, f, b, excel) in rows {
            let got = price_kernel(s, m, r, y, red, f, Some(b)).unwrap();
            assert_eq!(
                got.to_bits(),
                excel,
                "PRICE({s},{m},{r},{y},{red},{f},{b}) = {:#018x}, want {excel:#018x}",
                got.to_bits()
            );
        }
    }

    // W109: YIELD is intentionally NOT changed by the PRICE binexp fix (its solver keeps the
    // legacy `powf` price until its schedule is identified). Pin the current recon witnesses
    // so a future accidental coupling is caught. These are the KNOWN-DRIFTING values
    // (yield-catalog 19 ULP, yield-par 6 ULP vs Excel); the row stays open.
    // W109 G6-03c: DURATION bit-exact vs live Excel (build 20131) after the
    // `off = (E - A)/E` schedule + `excel_bond_pow` discount + `(diff*cash)/disc`
    // Macaulay body op-graph. b44 6,360 live witnesses: 2247 -> 6217 exact
    // (bases 2/3 0/1272 -> 1239/1242; on-coupon 100%). Remaining 143 are the
    // shared fractional x87 pow-chain ±1-2 ULP wall (same as PRICE b37 bases 2/3).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn duration_matches_live_excel_pinned_witnesses() {
        // settlement, maturity, coupon, yld bits; freq; basis; excel bits
        let rows: &[(u64, u64, u64, u64, f64, f64, u64)] = &[
            // Catalog witnesses (on-coupon DA 44013/44562, basis 0) — the two
            // former G6-03c rows, now exact.
            (
                0x40e57da000000000,
                0x40e5c24000000000,
                0x3fa999999999999a,
                0x3fa999999999999a,
                2.0,
                0.0,
                0x3ff76b5d5a9cdbe9,
            ), // yld 0.05
            (
                0x40e57da000000000,
                0x40e5c24000000000,
                0x3fa999999999999a,
                0x3fb47ae147ae147b,
                2.0,
                0.0,
                0x3ff767de5627448a,
            ), // yld 0.08
            // Actual/360 (basis 2) off-coupon — was material 0/1272 pre-fix.
            (
                0x40e57e0000000000,
                0x40e64b4000000000,
                0x3faeb851eb851eb8,
                0x3fa999999999999a,
                2.0,
                2.0,
                0x40100d23fbb83359,
            ),
            // Actual/365 (basis 3) off-coupon — was material 0/1272 pre-fix.
            (
                0x40e57e0000000000,
                0x40e64b4000000000,
                0x3faeb851eb851eb8,
                0x3fb47ae147ae147b,
                2.0,
                3.0,
                0x400fd3bce9b1ada9,
            ),
            // Quarterly (freq 4), basis 2 off-coupon.
            (
                0x40e57e0000000000,
                0x40e64b4000000000,
                0x3fb47ae147ae147b,
                0x3fa999999999999a,
                4.0,
                2.0,
                0x400ee0804a1d1c7c,
            ),
            // Leap-February bond (basis 1 act/act) off-coupon.
            (
                0x40e60ea000000000,
                0x40e652a000000000,
                0x3fa70a3d70a3d70a,
                0x3fa999999999999a,
                2.0,
                1.0,
                0x3ff7578208e817ad,
            ),
            // W109 G6-03c b45 month-end break regression guards (the CoupDaysBS
            // `diff360_us` accrued span). Feb-month-end settlement 2025-02-28,
            // quarterly, basis 0 — same bond family whose 31st-settlement sibling
            // (2025-03-31) exploded ~2.5e13 ULP with the plain `us_30_360` accrued.
            (
                0x40e6528000000000,
                0x40e6802000000000,
                0x3fa0a3d70a3d70a4,
                0x3f1a36e2eb1c432d,
                4.0,
                0.0,
                0x3fef9f4c11283edc,
            ),
            // 31st-of-month settlement 2023-12-31, semiannual, basis 0.
            (
                0x40e61d6000000000,
                0x40e6a66000000000,
                0x3fac28f5c28f5c29,
                0x3f1a36e2eb1c432d,
                2.0,
                0.0,
                0x4006955d65e34aa5,
            ),
        ];
        for (i, &(s, m, cp, y, f, b, excel)) in rows.iter().enumerate() {
            let got = duration_kernel(
                f64::from_bits(s),
                f64::from_bits(m),
                f64::from_bits(cp),
                f64::from_bits(y),
                f,
                Some(b),
            )
            .unwrap_or_else(|e| panic!("pin {i}: unexpected error {e:?}"));
            assert_eq!(
                got.to_bits(),
                excel,
                "pin {i}: got 0x{:016x} want 0x{excel:016x}",
                got.to_bits()
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yield_unchanged_by_price_fix() {
        let cat = yield_kernel(44013.0, 44562.0, 0.05, 95.0, 100.0, 2.0, Some(0.0)).unwrap();
        assert_eq!(
            cat.to_bits(),
            0x3fb61465bd6a9970,
            "yield-catalog must be unchanged"
        );
        // dc-ifc: bisection to adjacent bounds gives 0x3fa99999999999a1 (the 1e-15 price
        // tolerance stopped at 0x3fa99999999999a0); Excel gives 0.05 itself, 0x3fa999999999999a
        // (2026-10-27), and 0x3fb61465bd6a9983 for the catalogue row above.
        let par = yield_kernel(44013.0, 44562.0, 0.05, 100.0, 100.0, 2.0, Some(0.0)).unwrap();
        assert_eq!(
            par.to_bits(),
            0x3fa99999999999a1,
            "yield-par at full convergence"
        );
        assert_ne!(par.to_bits(), 0x3fa999999999999a);
    }

    /// W111-5 G8-06, live Excel 20430: ACCRINTM basis 1 uses Excel's actual/actual rule,
    /// equal dates accrue 0, and a zero rate is #NUM!.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn accrintm_actual_actual_and_edges_match_excel() {
        // 2036-10-31 .. 2037-02-13: 105 days over a 365-day year
        let v = accrintm_kernel(49979.0, 50084.0, 0.13139245608065744, Some(100.0), Some(1.0)).unwrap();
        assert_eq!(v, 100.0 * 0.13139245608065744 * (105.0 / 365.0));
        assert_eq!(accrintm_kernel(40000.0, 40000.0, 0.05, Some(1000.0), Some(0.0)).unwrap(), 0.0);
        assert!(accrintm_kernel(40001.0, 40000.0, 0.05, Some(1000.0), Some(0.0)).is_err());
        assert!(accrintm_kernel(40000.0, 40100.0, 0.0, Some(1000.0), Some(0.0)).is_err());
    }

}
