//! Bond pricing, yield, accrued interest and coupon-schedule kernels (PRICE, YIELD, ACCRINT,
//! DURATION, the COUP functions and their relatives).
//!
//! Parts of this file are a translation, changed, of ExcelFinancialFunctions by Luca Bolognese
//! (<https://github.com/fsprojects/ExcelFinancialFunctions>, `bonds.fs` and `daycountbasis.fs`;
//! Copyright 2010-2013, Apache License 2.0): `yielddisc_kernel` (`yieldDisc`),
//! `pricemat_kernel`'s factors and price association (`getMatFactors`, `priceMat`), the accrued-interest
//! kernel (after `accrInt`), and the day-count and coupon-date helpers named after their F#
//! counterparts in their doc comments (`changeMonth`, `dateDiff360Us`, `findPcdNcd`,
//! `IDayCount.DaysBetween`, `actualCoupDays`, `IDayCount.CoupDays`). They were rewritten in Rust
//! and changed where Excel measurement required it. The licence text is in
//! `deploy/notices/ported.txt`. The rest of the file is our own work.

use crate::locale::{
    DateSystem as WorkbookDateSystem, excel_serial_from_ymd, ymd_from_excel_serial,
};
use crate::ported::excel_numeric::{
    excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_sub,
};
use crate::value::{CalcValue, CoreValue, WorksheetErrorCode};

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
    if !v.is_finite() || v < 0.0 {
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
pub(crate) fn freq(v: f64) -> Result<i64, BondCoreEvalError> {
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
/// A date serial that admits 0 (1900-01-00): ACCRINTM, PRICEMAT and YIELDMAT read it as a date,
/// and a relationship check fails it after (Excel 20430, dc-j9k.382-.389: `=ACCRINTM(0,0,0.05,1000)`
/// is 0 and `=PRICEMAT(0,50000,0,0.05,0.06)` is #NUM!).
fn dser_from_zero(v: f64) -> Result<i64, BondCoreEvalError> {
    if v.is_finite() && (0.0..1.0).contains(&v.trunc()) {
        return Ok(0);
    }
    dser(v)
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
    excel_serial_from_ymd(WorkbookDateSystem::System1900, ty, tm, td)
        .filter(|&v| v >= 1.0)
        .map(|v| v as i64)
}
fn d360us(s: i64, e: i64) -> Result<f64, BondCoreEvalError> {
    let (sy, sm, mut sd) = ymd_from_excel_serial(WorkbookDateSystem::System1900, s as f64)
        .ok_or(derr(WorksheetErrorCode::Value))?;
    let (mut ey, mut em, mut ed) = ymd_from_excel_serial(WorkbookDateSystem::System1900, e as f64)
        .ok_or(derr(WorksheetErrorCode::Value))?;
    // ATP securities (YIELD, PRICE, DURATION, YIELDDISC, YIELDMAT) use Gregorian dim(1900, 2) = 28
    // rather than treating fictitious 1900-02-29 (serial 60) as February month-end (cb26 live Excel),
    // and check ed == 31 before promoting start_last_feb -> sd = 30 (nl27 patient-zero re-sweep).
    let start_last_feb = sm == 2 && sd == dim(sy, sm);
    let end_last_feb = em == 2 && ed == dim(ey, em);
    if ed == 31 {
        if sd < 30 {
            (ey, em) = if em == 12 { (ey + 1, 1) } else { (ey, em + 1) };
            ed = 1;
        } else {
            ed = 30;
        }
    }
    if sd == 31 || start_last_feb {
        sd = 30;
    }
    if start_last_feb && end_last_feb {
        ed = 30;
    }
    Ok(((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64)
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
/// The coupon period holding settlement. Each coupon date is counted back from maturity itself,
/// `n` periods at once, never from the coupon after it: a maturity on the 29th or 30th keeps its
/// day in every month long enough for it, and only a month-end maturity has month-end coupons.
/// Counting back one period at a time turned a coupon clamped to February 28 into a month end,
/// and every earlier coupon with it (Excel 16.0.20527, 2026-11-07: settlement 44617, maturity
/// 45899 (2025-08-30), semiannual, Actual/actual has COUPPCD 44438 (2021-08-30) and PRICE at a
/// 5% yield of 112.56619226472357; quarterly, Actual/360, 36417 to 37769 (2003-05-28): COUPPCD
/// 36400, PRICE 118.45016379152898). COUPPCD and its family already count from maturity.
fn period(c: Ctx) -> Result<Period, BondCoreEvalError> {
    let mpc = 12 / c.frequency;
    let mut next = c.maturity;
    let mut n = 1i64;
    loop {
        let back = mpc.checked_mul(n).ok_or(derr(WorksheetErrorCode::Num))?;
        let prev = addm(c.maturity, -back).ok_or(derr(WorksheetErrorCode::Num))?;
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
///   **binary exponentiation** with products stored from PC64. The initial 25
///   pilot controls did not distinguish the product word. R33 adjacent windows
///   separate it from strict multiplication on 961 of 10,752 public POWER controls;
///   the paired weighted-redemption and PRICE compositions match all 10,752.
/// * **Fractional exponent** (off-coupon) → the `pow` chain
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
    if exp.fract() == 0.0 && exp.abs() < u32::MAX as f64 {
        // R33 on-coupon POWER/PRICE/DURATION controls distinguish the
        // stored-PC64 products. Reuse the shared integer-power loop directly;
        // the worksheet POWER wrapper's base-ten policy is a separate rule.
        let p = crate::ported::power_fn::binexp_positive(base, exp.abs() as u64);
        return if exp >= 0.0 { p } else { excel_x87_div(1.0, p) };
    }
    if exp < 0.0 {
        excel_x87_div(1.0, crate::ported::excel_numeric::excel_pow_chain(base, -exp))
    } else {
        crate::ported::excel_numeric::excel_pow_chain(base, exp)
    }
}
fn pcomp_disc(
    rate: f64,
    yld: f64,
    red: f64,
    c: Ctx,
    p: Period,
) -> Result<(f64, f64, f64, f64, f64), BondCoreEvalError> {
    let f = c.frequency as f64;
    let e = dc(p.prev, p.next, c.basis, c.frequency)?;
    let a = days_between_num(p.prev, c.settlement, c.basis)?;
    // Excel's internal discount/settlement fraction is the DERIVED complement
    // `dsc = E - A` (universal across bases), NOT the direct day-count span
    // settlement→next-coupon. For bases 0/1/4 the two coincide except on
    // settlement-on-31st 30/360 rows (which `E - A` also fixes); for Actual/360
    // (basis 2) and Actual/365 (basis 3) they differ materially (the ~cents
    // PRICE error, G6-03d). Excel's own COUPDAYSNC still PUBLISHES the actual
    // days at bases 2/3 — PRICE's internal DSC deliberately diverges from the
    // published COUP* function. (W109 G6-03d; b37/b38 live-Excel, build 20131.)
    let dsc = e - a;
    if yld <= -f {
        return Err(derr(WorksheetErrorCode::Num));
    }
    let r_per = excel_x87_div(rate, f);
    let coup = excel_x87_mul(100.0, r_per);
    let accr = excel_x87_mul(100.0, excel_x87_mul(r_per, excel_x87_div(a, e)));
    if p.n == 1 {
        let den = 1.0 + (yld / f) * (dsc / e);
        if den <= 0.0 {
            return Err(derr(WorksheetErrorCode::Num));
        }
        let dirty = (red + coup) / den;
        let ans = dirty - accr;
        if !ans.is_finite() {
            return Err(derr(WorksheetErrorCode::Num));
        }
        return Ok((ans, dirty, coup, a, e));
    }
    let base = excel_x87_add(1.0, excel_x87_div(yld, f));
    if base <= 0.0 {
        return Err(derr(WorksheetErrorCode::Num));
    }
    let off = excel_x87_div(dsc, e);
    let mut pv = 0.0;
    for k in 0..p.n {
        let disc = excel_bond_pow(base, excel_x87_add(off, k as f64));
        if !disc.is_finite() {
            return Err(derr(WorksheetErrorCode::Num));
        }
        pv = excel_x87_add(pv, excel_x87_div(coup, disc));
    }
    let redemption_discount = excel_bond_pow(base, excel_x87_add(off, (p.n - 1) as f64));
    if !redemption_discount.is_finite() {
        return Err(derr(WorksheetErrorCode::Num));
    }
    let dirty = excel_x87_add(pv, excel_x87_div(red, redemption_discount));
    let ans = excel_x87_sub(dirty, accr);
    if !ans.is_finite() {
        return Err(derr(WorksheetErrorCode::Num));
    }
    Ok((ans, dirty, coup, a, e))
}

pub fn accrintm_kernel(
    issue: f64,
    settlement: f64,
    rate_: f64,
    par: Option<f64>,
    basis_: Option<f64>,
) -> Result<f64, BondCoreEvalError> {
    let issue = dser_from_zero(issue)?;
    let settlement = dser_from_zero(settlement)?;
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
    let settlement = dser_from_zero(settlement)?;
    let maturity = dser_from_zero(maturity)?;
    let issue = dser_from_zero(issue)?;
    let rate_ = rate(rate_)?;
    let yld = rate(yld)?;
    let basis_ = basis(basis_.unwrap_or(0.0))?;
    if !(issue < settlement && settlement < maturity) {
        return Err(derr(WorksheetErrorCode::Num));
    }
    let b = days_in_year_for_mat(issue, settlement, basis_)?;
    // Published EFF getMatFactors uses numerator-position day counts. This
    // distinction matters for a February-end issue and a 31st maturity.
    // Its original price association with stored-PC64 operations reproduces
    // all 2,404 recorded PRICEMAT bank outcomes at R23.
    let dim = days_between_num(issue, maturity, basis_)?;
    let a = days_between_num(issue, settlement, basis_)?;
    let dsm = excel_x87_sub(dim, a);
    let future = excel_x87_add(
        100.0,
        excel_x87_mul(excel_x87_mul(excel_x87_div(dim, b), rate_), 100.0),
    );
    let accrued = excel_x87_mul(excel_x87_mul(excel_x87_div(a, b), rate_), 100.0);
    let den = excel_x87_add(1.0, excel_x87_mul(excel_x87_div(dsm, b), yld));
    if den <= 0.0 {
        return Err(derr(WorksheetErrorCode::Num));
    }
    Ok(excel_x87_sub(excel_x87_div(future, den), accrued))
}
pub fn yieldmat_kernel(
    settlement: f64,
    maturity: f64,
    issue: f64,
    rate_: f64,
    price: f64,
    basis_: Option<f64>,
) -> Result<f64, BondCoreEvalError> {
    let settlement = dser_from_zero(settlement)?;
    let maturity = dser_from_zero(maturity)?;
    let issue = dser_from_zero(issue)?;
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
    use crate::ported::excel_numeric::{
        excel_x87_add, excel_x87_div, excel_x87_mul, excel_x87_sub,
    };
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
    let settlement = crate::ported::date_kernels::serial_from_number(settlement).map_err(derr)?;
    let maturity = crate::ported::date_kernels::serial_from_number(maturity).map_err(derr)?;
    let price = pos(price)?;
    let red = pos(red)?;
    let basis_ = basis(basis_.unwrap_or(0.0))?;
    if settlement >= maturity {
        return Err(derr(WorksheetErrorCode::Num));
    }
    // Excel20527: 517 paired YIELDDISC/INTRATE controls identify the same
    // day count and year basis, including serial zero and spans of 1900.
    // The actual/actual year total uses real calendar years; the serial span
    // still includes the fictitious 1900-02-29. Keep the year ratio separate.
    let dim = dd(settlement, maturity, basis_)?;
    let b = if basis_ == DayCountBasis::ActualActual {
        crate::ported::day_count_common::excel_actual_actual_year(settlement, maturity)
            .map_err(derr)?.1
    } else {
        days_in_year_for_mat(settlement, maturity, basis_)?
    };
    // Excel groups the year ratio: ((red - price)/price)*(b/dim) (dc-j9k: 25 of 25 readings in
    // the catalogue oracle's sheet dcj9k_closed_form; the left-to-right form matched 17).
    use crate::ported::excel_numeric::{excel_x87_div, excel_x87_mul, excel_x87_sub};
    Ok(excel_x87_mul(excel_x87_div(excel_x87_sub(red, price), price), excel_x87_div(b, dim)))
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
    Ok(pcomp_disc(rate(rate_)?, rate(yld)?, pos(red)?, c, p)?.0)
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
    let tracing = crate::trace::is_active();
    let c = match ctx(settlement, maturity, frequency, basis_).map_err(|error| {
        // Finite invalid date serials are NUM in YIELD. Preserve the direct
        // numeric API's VALUE result for nonfinite inputs.
        if settlement.is_finite()
            && maturity.is_finite()
            && error == derr(WorksheetErrorCode::Value)
        {
            derr(WorksheetErrorCode::Num)
        } else {
            error
        }
    }) {
        Ok(c) => c,
        Err(e) => {
            if tracing {
                crate::trace::hit(0);
            }
            return Err(e);
        }
    };
    let p = match period(c) {
        Ok(p) => p,
        Err(e) => {
            if tracing {
                crate::trace::hit(0);
            }
            return Err(e);
        }
    };
    let rate_ = match rate(rate_) {
        Ok(r) => r,
        Err(e) => {
            if tracing {
                crate::trace::hit(0);
            }
            return Err(e);
        }
    };
    let price = match pos(price) {
        Ok(pr) => pr,
        Err(e) => {
            if tracing {
                crate::trace::hit(0);
            }
            return Err(e);
        }
    };
    let red = match pos(red) {
        Ok(rd) => rd,
        Err(e) => {
            if tracing {
                crate::trace::hit(0);
            }
            return Err(e);
        }
    };
    // A coupon-date bond priced and redeemed at 100 has yield equal to its coupon
    // rate; Excel does not return that root directly but takes the same near-par
    // seed as every other price near 100 (answer 11 B1 Y2), so no shortcut here.
    if p.n == 1 {
        if tracing {
            crate::trace::hit(1);
        }
        // R33 paired COUP/DAYS360 controls isolate the documented normalized
        // formula. A and DSR share the existing numerator-position day rule;
        // Actual/360 and Actual/365 use the actual coupon length here. The
        // public COUPDAYS constants for those two bases are separate results.
        let f = c.frequency as f64;
        let e = match c.basis {
            DayCountBasis::Us30_360 | DayCountBasis::European30_360 => 360.0 / f,
            _ => act(p.prev, p.next),
        };
        let a = days_between_num(p.prev, c.settlement, c.basis)?;
        let dsc = days_between_num(c.settlement, p.next, c.basis)?;
        // A positive calendar interval can have zero remaining 30/360 days.
        // Original last-day controls across prices/rates publish zero there.
        if dsc == 0.0 {
            if tracing {
                crate::trace::hit(2);
            }
            return Ok(0.0);
        }
        // Each named arithmetic result stores RN53(RN64(operation)). The
        // division ties distinguish this word from strict binary64; the
        // fixed ordinary, month-end and fresh redemption cohorts all agree.
        let coupon = excel_x87_div(rate_, f);
        let normalized_price = excel_x87_div(price, 100.0);
        let normalized_redemption = excel_x87_div(red, 100.0);
        let accrued = excel_x87_mul(excel_x87_div(a, e), coupon);
        let dirty = excel_x87_add(normalized_price, accrued);
        let cash = excel_x87_add(normalized_redemption, coupon);
        let relative_gain = excel_x87_div(excel_x87_sub(cash, dirty), dirty);
        let annualization = excel_x87_div(excel_x87_mul(f, e), dsc);
        return Ok(excel_x87_mul(relative_gain, annualization));
    }
    // Multi-period solver (dc-j9k.98..101 / cb26, live Excel 16.0.20625.20108):
    // 1. Unified 5-basis par-100 x87 seed `y_0` (`whole_years + stub / year_days`,
    //    `d = price - 100.0` hardcoded, never `red`).
    // 2. 1-based all-x87 analytic Newton loop on PRICE's `excel_bond_pow` discount curve for up
    //    to 100 iterations, with `ACCRINT` accrual `100*(rate/f)*(a/e)`, `days_between_num` `a`,
    //    separate `t_cp` / `(f != 1) t_red` finiteness, subnormal slope flush, and `cIter--`
    //    `iter == 99 -> #NUM!`.
    let f = c.frequency as f64;
    let (sy, sm, sd) = ymd_from_excel_serial(WorkbookDateSystem::System1900, c.settlement as f64)
        .ok_or(derr(WorksheetErrorCode::Value))?;
    let (ey, em, ed) = ymd_from_excel_serial(WorkbookDateSystem::System1900, c.maturity as f64)
        .ok_or(derr(WorksheetErrorCode::Value))?;
    let k_yr = (ey - sy) - i64::from(em < sm || (em == sm && ed < sd));
    let sk_base = excel_serial_from_ymd(WorkbookDateSystem::System1900, sy + k_yr, sm, 1)
        .filter(|&v| v >= 1.0)
        .ok_or(derr(WorksheetErrorCode::Num))? as i64;
    let s_k = sk_base + (sd - 1);
    let frac = match c.basis {
        DayCountBasis::Us30_360 => {
            if tracing {
                crate::trace::hit(4);
            }
            d360us(s_k, c.maturity)? / 360.0
        }
        DayCountBasis::European30_360 => {
            if tracing {
                crate::trace::hit(4);
            }
            d360eu(s_k, c.maturity)? / 360.0
        }
        DayCountBasis::ActualActual => {
            if tracing {
                crate::trace::hit(5);
            }
            (c.maturity - s_k) as f64 / dyear(ey)
        }
        DayCountBasis::Actual360 => {
            if tracing {
                crate::trace::hit(5);
            }
            (c.maturity - s_k) as f64 / 360.0
        }
        DayCountBasis::Actual365 => {
            if tracing {
                crate::trace::hit(5);
            }
            (c.maturity - s_k) as f64 / 365.0
        }
    };
    let yrs = excel_x87_add(k_yr as f64, frac);
    let d = excel_x87_sub(price, 100.0);
    let mut yr_r = excel_x87_mul(yrs, rate_);
    if yr_r.abs() < f64::MIN_POSITIVE {
        yr_r = 0.0;
    }
    let tc = excel_x87_mul(yr_r, 100.0);
    let den = excel_x87_add(
        excel_x87_add(
            excel_x87_mul(yrs, 100.0),
            excel_x87_div(excel_x87_mul(d, yrs), 2.0),
        ),
        excel_x87_div(d, 4.0),
    );
    let mut y = excel_x87_div(excel_x87_sub(tc, d), den);
    if !y.is_finite() {
        if tracing {
            crate::trace::hit(3);
        }
        return Err(derr(WorksheetErrorCode::Num));
    }

    let e = dc(p.prev, p.next, c.basis, c.frequency)?;
    let a = days_between_num(p.prev, c.settlement, c.basis)?;
    let r_per = excel_x87_div(rate_, f);
    let cp = excel_x87_mul(100.0, r_per);
    let accr = excel_x87_mul(100.0, excel_x87_mul(r_per, excel_x87_div(a, e)));
    let off = excel_x87_div(e - a, e);

    for iter in 0..100u32 {
        let base = excel_x87_add(1.0, excel_x87_div(y, f));
        if tracing {
            crate::trace::observe_boundary("yield_base_zero", base, 0.0);
        }
        // On a whole coupon interval the discount exponents are integers. Newton
        // can cross -frequency and continue on the negative-base price curve.
        if base == 0.0 || (base < 0.0 && off.fract() != 0.0) {
            if tracing {
                crate::trace::hit(6);
                crate::trace::record_iterations(iter + 1);
            }
            return Err(derr(WorksheetErrorCode::Num));
        }
        if tracing && base < 0.0 {
            crate::trace::hit(7);
        }
        let (mut pv, mut t_cp) = (0.0_f64, 0.0_f64);
        for k in 1..=p.n {
            let ex_pv = excel_x87_add(off, (k - 1) as f64);
            let ex1 = excel_x87_add(off, k as f64);
            let disc = excel_bond_pow(base, ex_pv);
            let disc1 = excel_bond_pow(base, ex1);
            if !disc.is_finite() || !disc1.is_finite() {
                if tracing {
                    crate::trace::hit(9);
                    crate::trace::record_iterations(iter + 1);
                }
                return Err(derr(WorksheetErrorCode::Num));
            }
            pv = excel_x87_add(pv, excel_x87_div(cp, disc));
            let denom = excel_x87_mul(f, disc1);
            if !denom.is_finite() {
                if tracing {
                    crate::trace::hit(9);
                    crate::trace::record_iterations(iter + 1);
                }
                return Err(derr(WorksheetErrorCode::Num));
            }
            t_cp = excel_x87_add(t_cp, excel_x87_div(excel_x87_mul(ex_pv, cp), denom));
        }
        let rex_pv = excel_x87_add(off, (p.n - 1) as f64);
        let rex1 = excel_x87_add(off, p.n as f64);
        let r_disc = excel_bond_pow(base, rex_pv);
        let r_disc1 = excel_bond_pow(base, rex1);
        if !r_disc.is_finite() || !r_disc1.is_finite() {
            if tracing {
                crate::trace::hit(9);
                crate::trace::record_iterations(iter + 1);
            }
            return Err(derr(WorksheetErrorCode::Num));
        }
        pv = excel_x87_add(pv, excel_x87_div(red, r_disc));
        // Redemption derivative finiteness (cb26 Y2-B + nl27 live holdout/pz-Y3):
        // `f * r_disc1` overflow is rejected immediately; if `t_red = (rex_pv * red) / r_denom`
        // overflows (`!t_red.is_finite()`, any frequency), Excel omits `t_red` from `slope`
        // (`slope = -t_cp`), checks `|step| <= 1e-10` first (publishing `y` when `|pv/t_cp| <= 1e-10`),
        // and only returns `#NUM!` when `|step| > 1e-10`. When `t_red` is finite, `t_cp + t_red == +inf`
        // is not rejected (`slope = -inf -> step = -0.0 -> Ok(y)`).
        let r_denom = if c.frequency == 1 {
            r_disc1
        } else {
            let d = excel_x87_mul(f, r_disc1);
            if !d.is_finite() {
                if tracing {
                    crate::trace::hit(9);
                    crate::trace::record_iterations(iter + 1);
                }
                return Err(derr(WorksheetErrorCode::Num));
            }
            d
        };
        if !pv.is_finite() || !t_cp.is_finite() {
            if tracing {
                crate::trace::hit(9);
                crate::trace::record_iterations(iter + 1);
            }
            return Err(derr(WorksheetErrorCode::Num));
        }
        let t_red = excel_x87_div(excel_x87_mul(rex_pv, red), r_denom);
        let t_red_overflow = !t_red.is_finite();
        let mut slope = if t_red_overflow {
            -t_cp
        } else {
            -excel_x87_add(t_cp, t_red)
        };
        // Excel flushes subnormal slope to 0.0; on negative base (`off` integer),
        // `diff / 0.0 == ±inf` publishes `y` (`!step.is_nan() && base < 0.0`, cb26 Y3).
        if slope.abs() < f64::MIN_POSITIVE {
            slope = 0.0;
        }
        let step = excel_x87_div(excel_x87_sub(excel_x87_sub(pv, accr), price), slope);
        if !step.is_finite() {
            if !step.is_nan() && base < 0.0 {
                if tracing {
                    crate::trace::hit(8);
                    crate::trace::record_iterations(iter + 1);
                }
                return Ok(y);
            }
            if tracing {
                crate::trace::hit(9);
                crate::trace::record_iterations(iter + 1);
            }
            return Err(derr(WorksheetErrorCode::Num));
        }
        if tracing {
            crate::trace::observe_boundary("yield_step_tol", step.abs(), 1e-10);
        }
        if step.abs() <= 1e-10 {
            if tracing {
                let iters = iter + 1;
                crate::trace::record_iterations(iters);
                if iters <= 5 {
                    crate::trace::hit(10);
                } else if iters <= 25 {
                    crate::trace::hit(11);
                } else {
                    crate::trace::hit(12);
                }
            }
            // Excel's `while (cIter-- > 0)` leaves `cIter == 0` when breaking on the 100th
            // evaluation (`iter == 99`), triggering `if (cIter == 0) return #NUM!`, whereas
            // 100 non-converged steps exit with `cIter == -1` and publish `y_100`.
            return if iter == 99 {
                Err(derr(WorksheetErrorCode::Num))
            } else {
                Ok(y)
            };
        }
        if t_red_overflow {
            if tracing {
                crate::trace::hit(9);
                crate::trace::record_iterations(iter + 1);
            }
            return Err(derr(WorksheetErrorCode::Num));
        }
        y = excel_x87_sub(y, step);
    }
    if tracing {
        crate::trace::record_iterations(100);
        crate::trace::hit(13);
    }
    if !y.is_finite() {
        if tracing {
            crate::trace::hit(14);
        }
        return Err(derr(WorksheetErrorCode::Num));
    }
    Ok(y)
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
    // uses. R37's general-discount controls distinguish coupon PVs weighted
    // after division from redemption weighted before division. Quotients and
    // both forward running sums store from PC64, as does the final ratio.
    // The fixed program agrees on 144 on-coupon discovery controls, 288
    // holdout controls, 76 separating/adjacent controls, then 288 fresh
    // controls per function (Excel20527; Research evidence/round37).
    // The general fractional power producer remains unresolved. R33
    // exact-dyadic nonzero coupons and zero-coupon
    // controls identify stored-PC64 redemption quotients separately from the
    // unresolved off-coupon producer. In this kernel,
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
    // Excel's redemption time is S(S(off + n) - 1), not S(off + (n - 1)): the
    // two differ whenever the sum crosses a binade (bank: every DURATION and
    // MDURATION miss with n > 1 has them different). The n = 1 bond runs the
    // same weighted ratio, so its answer depends on the yield.
    let base = 1.0 + yld / f;
    let mut num = 0.0;
    let mut den = 0.0;
    for k in 0..p.n {
        let diff = off + k as f64;
        let disc = excel_bond_pow(base, diff);
        if !disc.is_finite() {
            return Err(derr(WorksheetErrorCode::Num));
        }
        let coupon_pv = excel_x87_div(coup, disc);
        num = excel_x87_add(num, diff * coupon_pv);
        den = excel_x87_add(den, coupon_pv);
        if k + 1 == p.n {
            let redemption_time = (off + p.n as f64) - 1.0;
            num = excel_x87_add(num, excel_x87_div(redemption_time * 100.0, disc));
            den = excel_x87_add(den, excel_x87_div(100.0, disc));
        }
    }
    Ok(excel_x87_div(num, den) / f)
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
    let duration = duration_kernel(settlement, maturity, coupon, yld, frequency, basis_)?;
    Ok(excel_x87_div(duration, 1.0 + rate(yld)? / f))
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
    let issue = dser_from_zero(issue)?;
    let first = dser_from_zero(first_interest)?;
    let settlement = dser_from_zero(settlement)?;
    let rate_ = rate(rate_)?;
    // ACCRINT wants a rate above zero (Excel 20625, dc-j9k.495); PRICE and YIELD take a zero coupon.
    if rate_ == 0.0 {
        return Err(derr(WorksheetErrorCode::Num));
    }
    let par = pos(par.unwrap_or(1000.0))?;
    let f = freq(frequency)?;
    let basis_ = basis(basis_.unwrap_or(0.0))?;
    // calc_method: TRUE = FromIssueToSettlement (Excel default), FALSE = FromFirstToSettlement.
    let from_issue = calc_method.unwrap_or(true);
    if !(issue < settlement) {
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
                    && !is_last
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
    use super::*;
    #[allow(unused_imports)]
    use crate::value::{CalcArray, CalcValue, CoreValue, ExcelText};

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
    fn price_counts_coupon_dates_back_from_maturity() {
        // Excel 16.0.20527, 2026-11-07: PRICE at a 5% yield for maturities on the 30th and the
        // 28th, where the coupon before them falls in February.
        let cases = [
            (
                44617.0,
                45899.0,
                0.0895,
                100.0,
                2.0,
                1.0,
                112.56619226472357,
            ),
            (37099.0, 39081.0, 0.046, 100.0, 2.0, 1.0, 98.11585064820228),
            (
                36417.0,
                37769.0,
                0.1049,
                100.0,
                4.0,
                2.0,
                118.45016379152898,
            ),
        ];
        for (s, m, rate, red, f, b, excel) in cases {
            let price = price_kernel(s, m, rate, 0.05, red, f, Some(b)).unwrap();
            assert_eq!(
                price.to_bits(),
                f64::to_bits(excel),
                "PRICE({s},{m}) = {price}, Excel {excel}"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yield_par_identity_and_existing_nonpar_solver() {
        let cat = yield_kernel(44013.0, 44562.0, 0.05, 95.0, 100.0, 2.0, Some(0.0)).unwrap();
        assert_eq!(
            cat.to_bits(),
            0x3fb61465bd6a9983,
            "the nonpar catalogue row matches Excel's analytic Newton solver"
        );
        // The recorded Excel par result is 0.05 itself. Bisection previously
        // returned 0x3fa99999999999a1.
        let par = yield_kernel(44013.0, 44562.0, 0.05, 100.0, 100.0, 2.0, Some(0.0)).unwrap();
        assert_eq!(
            par.to_bits(),
            0x3fa999999999999a,
            "the par yield is the recorded coupon rate"
        );
        assert_ne!(par.to_bits(), 0x3fa99999999999a1);
    }

    /// Near par on a coupon date Excel returns the closed-form seed while the Newton step there
    /// is below 1e-10: Excel 20228 `answers-yield-near-seed-discovery-20260809.json` rows
    /// oncoupon-short-030, oncoupon-long-039 and -040, and Excel 20527 `r18-YIELD.json` rows
    /// 001 and 009 (price 100 + 1 ulp at a zero coupon). The bisection was 10^6 to 10^15 units off.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yield_near_par_returns_the_closed_form_seed() {
        let rows: [(f64, f64, f64, u64, f64, u64); 5] = [
            (44013.0, 44562.0, 0.05, 0x4059000040000000, 2.0, 0x3fa9999604189993),
            (44013.0, 46753.0, 0.05, 0x4058fffff4000004, 2.0, 0x3fa99999c0ebedf7),
            (44013.0, 46753.0, 0.05, 0x405900000c000004, 2.0, 0x3fa9999972474536),
            (44197.0, 45292.0, 0.0, 0x4059000000000001, 1.0, 0xbc8b4e81b4e81b4f),
            (44013.0, 44562.0, 0.0, 0x4059000000000001, 2.0, 0xbc9b4e81b4e81b4f),
        ];
        for (s, m, rate, price, f, excel) in rows {
            let price = f64::from_bits(price);
            let y = yield_kernel(s, m, rate, price, 100.0, f, Some(0.0)).unwrap();
            assert_eq!(y.to_bits(), excel, "YIELD({s}, {m}, {rate}, {price}, 100, {f}, 0)");
        }
    }

    /// W111-5 G8-06, live Excel 20430: ACCRINTM basis 1 uses Excel's actual/actual rule,
    /// equal dates accrue 0, and a zero rate is #NUM!.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn accrintm_actual_actual_and_edges_match_excel() {
        // 2036-10-31 .. 2037-02-13: 105 days over a 365-day year
        let v = accrintm_kernel(
            49979.0,
            50084.0,
            0.13139245608065744,
            Some(100.0),
            Some(1.0),
        )
        .unwrap();
        assert_eq!(v, 100.0 * 0.13139245608065744 * (105.0 / 365.0));
        assert_eq!(
            accrintm_kernel(40000.0, 40000.0, 0.05, Some(1000.0), Some(0.0)).unwrap(),
            0.0
        );
        assert!(accrintm_kernel(40001.0, 40000.0, 0.05, Some(1000.0), Some(0.0)).is_err());
        assert!(accrintm_kernel(40000.0, 40100.0, 0.0, Some(1000.0), Some(0.0)).is_err());
    }

    /// dc-j9k.98 (cb26 live Excel 16.0.20625.20108, Y1 & patient-zero siblings):
    /// Stepping back to serial 0 (1899-12-31) or overflowing a discount factor to +inf
    /// returns #NUM! in YIELD, PRICE, DURATION, and MDURATION, while year-9999 bonds
    /// with prev >= 1 and finite discount factors stay finite.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yield_price_duration_year_9999_prev_zero_and_pow_overflow_return_num() {
        let num_err = Err(BondCoreEvalError::Domain(WorksheetErrorCode::Num));
        // YIELD prev == 0 (Y1A-01..04) and sf2 representative Y1-dc-j9k-98-huge-maturity
        assert_eq!(
            yield_kernel(
                f64::from_bits(0x404e000000000000),
                f64::from_bits(0x4146924080000000),
                f64::from_bits(0x3fbbc7c725bc7504),
                f64::from_bits(0x4062b485c75e100d),
                100.0,
                2.0,
                Some(0.0),
            ),
            num_err
        );
        for &(s, f, b) in &[(60.0, 2.0, 0.0), (25.0, 2.0, 1.0), (60.0, 1.0, 2.0), (80.0, 4.0, 3.0)] {
            assert_eq!(
                yield_kernel(
                    s,
                    2958465.0,
                    0.0,
                    f64::from_bits(0x4058ffef9db22d0e),
                    100.0,
                    f,
                    Some(b),
                ),
                num_err,
                "YIELD prev==0 ({s}, 2958465, f={f}, b={b})"
            );
        }
        // YIELD pow overflow (Y1A-12, Y1A-13)
        assert_eq!(
            yield_kernel(
                45000.0,
                2958465.0,
                f64::from_bits(0x3fb78d4fdf3b645a),
                100.0,
                100.0,
                2.0,
                Some(0.0),
            ),
            num_err
        );
        assert_eq!(
            yield_kernel(
                45000.0,
                2958465.0,
                f64::from_bits(0x3fc851eb851eb852),
                100.0,
                100.0,
                4.0,
                Some(1.0),
            ),
            num_err
        );
        // YIELD finite controls at/near year 9999 with prev >= 1 and finite pow (Y1A-06, Y1A-07)
        assert!(
            yield_kernel(
                60.0,
                f64::from_bits(0x414691f480000000),
                0.0,
                f64::from_bits(0x4058ffef9db22d0e),
                100.0,
                2.0,
                Some(0.0),
            )
            .is_ok()
        );
        assert!(
            yield_kernel(
                425.0,
                2958465.0,
                0.0,
                f64::from_bits(0x4058ffef9db22d0e),
                100.0,
                2.0,
                Some(0.0),
            )
            .is_ok()
        );
        // PRICE prev == 0 (PZP-01..04) and pow overflow (PZP-07..09)
        for &(s, m, r, y, f, b) in &[
            (60.0, 2958465.0, 0.05, 0.0, 2.0, 0.0),
            (60.0, 2958465.0, 0.0, f64::from_bits(0x3ee4f8b588e368f1), 2.0, 1.0),
            (45.0, 2958465.0, 0.0, f64::from_bits(0x3ee4f8b588e368f1), 1.0, 2.0),
            (80.0, 2958465.0, 0.04, 0.0, 4.0, 4.0),
            (43831.0, 2958465.0, 0.05, 0.2, 2.0, 0.0),
            (45000.0, 2958465.0, 0.05, 0.2, 2.0, 1.0),
            (44015.0, 60000.0, 0.05, 8000.0, 2.0, 0.0),
        ] {
            assert_eq!(
                price_kernel(s, m, r, y, 100.0, f, Some(b)),
                num_err,
                "PRICE ({s}, {m}, r={r}, y={y}, f={f}, b={b})"
            );
        }
        // DURATION / MDURATION prev == 0 (PZD-01,02, PZM-01) and pow overflow (PZD-05,06, PZM-03)
        assert_eq!(duration_kernel(60.0, 2958465.0, 0.05, 0.04, 2.0, Some(0.0)), num_err);
        assert_eq!(
            duration_kernel(60.0, 2958465.0, 0.0, f64::from_bits(0x3ee4f8b588e368f1), 2.0, Some(1.0)),
            num_err
        );
        assert_eq!(duration_kernel(45000.0, 2958465.0, 0.05, 0.2, 2.0, Some(0.0)), num_err);
        assert_eq!(duration_kernel(44015.0, 60000.0, 0.05, 8000.0, 2.0, Some(0.0)), num_err);
        assert_eq!(mduration_kernel(60.0, 2958465.0, 0.05, 0.04, 2.0, Some(0.0)), num_err);
        assert_eq!(mduration_kernel(45000.0, 2958465.0, 0.05, 0.2, 2.0, Some(0.0)), num_err);
    }

    /// dc-j9k.99 (cb26 live Excel 16.0.20625.20108, Y2-A & Y2-B pre-registered rival):
    /// Separate derivative component finiteness checks (`t_cp` always, `t_red` only when
    /// `frequency != 1`, without checking `t_cp + t_red` overflow), plus the `cIter--`
    /// off-by-one where convergence on the 100th evaluation (`iter == 99`) returns #NUM!
    /// while >100 non-converged steps publish `y_100`.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yield_derivative_finiteness_and_iter_99_frontier() {
        let num_err = Err(BondCoreEvalError::Domain(WorksheetErrorCode::Num));
        // sf2-YIELD-01776 (Y2-dc-j9k-99-extreme-price-neg)
        assert_eq!(
            yield_kernel(
                f64::from_bits(0x40e50b4000000000),
                f64::from_bits(0x40eb710000000000),
                0.0,
                f64::from_bits(0x40d9204834ac4d58),
                100.0,
                1.0,
                Some(0.0),
            ),
            num_err
        );
        // Y2A-05, Y2A-06: converge on 100th eval (iter == 99) -> #NUM!
        assert_eq!(
            yield_kernel(
                f64::from_bits(0x40e50b8000000000),
                f64::from_bits(0x40eb580000000000),
                0.0,
                f64::from_bits(0x40d8dc4449e4244c),
                100.0,
                1.0,
                Some(0.0),
            ),
            num_err
        );
        assert_eq!(
            yield_kernel(
                f64::from_bits(0x40e5180000000000),
                f64::from_bits(0x40eadb0000000000),
                f64::from_bits(0x3f947ae147ae147b),
                f64::from_bits(0x40e5c13ddb587403),
                100.0,
                2.0,
                Some(0.0),
            ),
            num_err
        );
        // Y2A-11, Y2A-12: >100 non-converged evals -> finite y_100
        assert!(
            yield_kernel(
                f64::from_bits(0x40e50b8000000000),
                f64::from_bits(0x40eb580000000000),
                0.0,
                f64::from_bits(0x40d94e1ba49ddd6c),
                100.0,
                1.0,
                Some(0.0),
            )
            .is_ok()
        );
        assert!(
            yield_kernel(
                f64::from_bits(0x40e5180000000000),
                f64::from_bits(0x40eadb0000000000),
                f64::from_bits(0x3f947ae147ae147b),
                f64::from_bits(0x40e81cc0aa9908dc),
                100.0,
                2.0,
                Some(0.0),
            )
            .is_ok()
        );
        // Y2B-01..02: t_cp, t_red finite with t_cp + t_red == +inf -> finite y_1
        assert!(
            yield_kernel(
                f64::from_bits(0x40e57de000000000),
                f64::from_bits(0x40e9e10000000000),
                f64::from_bits(0x3feccccccccccccd),
                f64::from_bits(0x4102499445b32fc8),
                100.0,
                1.0,
                Some(0.0),
            )
            .is_ok()
        );
        assert!(
            yield_kernel(
                f64::from_bits(0x40e58a6000000000),
                f64::from_bits(0x40e8e70000000000),
                f64::from_bits(0x3ffccccccccccccd),
                f64::from_bits(0x41133a8a953c78b1),
                100.0,
                2.0,
                Some(0.0),
            )
            .is_ok()
        );
        // Y2B-06..10: f in {2, 4} with t_red == +inf or t_cp == +inf -> #NUM!
        for &(s, m, r, pr, red, f, b) in &[
            (0x40e58a6000000000u64, 0x40e8e70000000000, 0x3fd999999999999a, 0x40f670f7da047e00, 100.0, 2.0, 0.0),
            (0x40e596e000000000, 0x40e9258000000000, 0x3fd70a3d70a3d70a, 0x40f85d09f08f0d52, 120.0, 2.0, 1.0),
            (0x40e5a36000000000, 0x40e82b8000000000, 0x3fd999999999999a, 0x40fac56225d71c12, 100.0, 4.0, 2.0),
            (0x40e5afe000000000, 0x40e86a0000000000, 0x4020000000000000, 0x4138dbd01b06b901, 80.0, 4.0, 3.0),
            (0x40e5bc6000000000, 0x40e9640000000000, 0x400fae147ae147af, 0x41277c85aa1f0cd5, 90.0, 2.0, 4.0),
        ] {
            assert_eq!(
                yield_kernel(
                    f64::from_bits(s),
                    f64::from_bits(m),
                    f64::from_bits(r),
                    f64::from_bits(pr),
                    red,
                    f,
                    Some(b),
                ),
                num_err,
                "Y2B-06..10 f={f} b={b}"
            );
        }
        // Y2B-11..15 (winning rival_f1_tred_inf): f == 1 skips t_red finiteness -> finite y_1
        for &(s, m, r, pr, red, b) in &[
            (0x40e57e8000000000u64, 0x40e9fa0000000000, 0x3fb0624dd2f1a9fc, 0x40d8bf36801c4abf, 80.0, 0.0),
            (0x40e58b0000000000, 0x40ea1f8000000000, 0x3fa5c28f5c28f5c3, 0x40d7276e86c94fc2, 85.0, 1.0),
            (0x40e5978000000000, 0x40ea450000000000, 0x3faeb851eb851eb8, 0x40dd9c8f2a005b96, 100.0, 0.0),
            (0x40e5a40000000000, 0x40ea6a8000000000, 0x3fa47ae147ae147b, 0x40db080fd5105928, 100.0, 2.0),
            (0x40e5b08000000000, 0x40ea900000000000, 0x3fb999999999999a, 0x40eda0131c91c6ad, 200.0, 3.0),
        ] {
            assert!(
                yield_kernel(
                    f64::from_bits(s),
                    f64::from_bits(m),
                    f64::from_bits(r),
                    f64::from_bits(pr),
                    red,
                    1.0,
                    Some(b),
                )
                .is_ok(),
                "Y2B-11..15 f=1 b={b}"
            );
        }
    }

    /// dc-j9k.100 (cb26 live Excel 16.0.20625.20108, Y3):
    /// Subnormal slope flushes to 0.0; on negative base (`off` integer), `diff / 0.0 == ±inf`
    /// publishes `y` (`!step.is_nan() && base < 0.0`), covering both zero and tiny coupons.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yield_subnormal_slope_flush_and_neg_base_publish() {
        // sf2-YIELD-01821 (Y3-dc-j9k-100-zero-coupon-negbase)
        let y0 = yield_kernel(
            f64::from_bits(0x40e5efe000000000),
            f64::from_bits(0x40e89cc000000000),
            0.0,
            f64::from_bits(0x41f1c0fe0114d10b),
            100.0,
            4.0,
            Some(2.0),
        )
        .expect("zero-coupon deep-premium negative-base publishes y");
        assert_eq!(y0.to_bits(), 0xc11a3d4e5b669882, "got {y0} ({:#018x})", y0.to_bits());

        // sf2-YIELD-02243 (Y3-dc-j9k-100-tiny-coupon-negbase)
        let y1 = yield_kernel(
            f64::from_bits(0x40e61d8000000000),
            f64::from_bits(0x40e75d2000000000),
            f64::from_bits(0x0010000000000005),
            f64::from_bits(0x41cc388161846e66),
            f64::from_bits(0x3f1fef0875adf64f),
            4.0,
            Some(0.0),
        )
        .expect("tiny-coupon deep-premium negative-base publishes y");
        assert_eq!(y1.to_bits(), 0xc2417b5a123fae6d, "got {y1} ({:#018x})", y1.to_bits());

        // Y3A-01..06: on-coupon even/odd n with tiny/zero rate and pr > red -> finite
        for &(s, m, r, pr, f, b) in &[
            (0x40e566e000000000u64, 0x40e72f8000000000, 0x01a56e1fc2f8f359, 250.0, 2.0, 0.0),
            (0x40e566e000000000, 0x40e7462000000000, 0x01a56e1fc2f8f359, 250.0, 2.0, 0.0),
            (0x40e566e000000000, 0x40e72f8000000000, 0x0010000000000000, 300.0, 2.0, 1.0),
            (0x40e566e000000000, 0x40e813c000000000, 0x0000000000000000, 400.0, 2.0, 0.0),
        ] {
            assert!(
                yield_kernel(
                    f64::from_bits(s),
                    f64::from_bits(m),
                    f64::from_bits(r),
                    pr,
                    100.0,
                    f,
                    Some(b),
                )
                .is_ok(),
                "Y3A ({s:#x}, {m:#x}, r={r:#x})"
            );
        }
    }

    /// dc-j9k.101 (cb26 live Excel 16.0.20625.20108, Y4 & patient-zero siblings):
    /// Gregorian dim(1900, 2) == 28 in d360us, days_between_num accrued span in YIELD/PRICE,
    /// unified 5-basis x87 par-100 seed, ACCRINT accrual 100*(rate/f)*(a/e), 1-based off+k
    /// coupon loop, exp < 0 reciprocal staging in excel_bond_pow, and all-x87 arithmetic.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yield_price_yielddisc_yieldmat_y4_exact_bits() {
        // YIELD fuzz representatives + Y4-A + Y1A finite + Y2B f==1 exact bits
        let yield_cases: &[(&str, u64, u64, u64, u64, u64, u64, Option<u64>, u64)] = &[
            ("Y4-1ulp", 0x40e5564000000000, 0x40e6088000000000, 0x0, 0x40628c434b115350, 0x4059000000000000, 0x4010000000000000, None, 0xbfb98affd89195c8),
            ("Y4-numeric", 0x407a900000000000, 0x4146924080000000, 0x3fbbc7c725bc7504, 0x4062b485c75e100d, 0x4059000000000000, 0x4000000000000000, None, 0x3fb28fe171e72a83),
            ("Y4-gross", 0x404e000000000000, 0x40e1dd2000000000, 0x3fbe6bebee76e9bf, 0x4060c1ad60248e3a, 0x4059000000000000, 0x4010000000000000, Some(0x0), 0x3fb6b1653afc012a),
            ("Y4A-01", 0x404e000000000000, 0x40e1dd2000000000, 0x3fa999999999999a, 0x4058a00000000000, 0x4059000000000000, 0x4000000000000000, Some(0x0), 0x3fa9fe174d19bd37),
            ("Y4A-03", 0x40e6566000000000, 0x40e6c4c000000000, 0x3fa999999999999a, 0x4058cccccccccccd, 0x4059000000000000, 0x4000000000000000, Some(0x0), 0x3fab6bb5046f4d53),
            ("Y4A-05", 0x40e624e000000000, 0x40e6820000000000, 0x0, 0x4058fffffffeaca5, 0x4059000000000000, 0x4000000000000000, Some(0x0), 0x3d9aa1704e20f764),
            ("Y4A-06", 0x404e000000000000, 0x4091d40000000000, 0x0, 0x4058fffffffeaca5, 0x4059000000000000, 0x4000000000000000, Some(0x0), 0x3d92636e8a3cc6ec),
            ("Y4A-07", 0x40e624e000000000, 0x40e6820000000000, 0x0, 0x4058fffffffeaca5, 0x4059000000000000, 0x4000000000000000, Some(0x4010000000000000), 0x3d9aa1704e20f764),
            ("Y4A-09", 0x40e53da000000000, 0x40eadd4000000000, 0x3fac779a6b50b0f2, 0x3f94bf0995aaf78f, 0x4059000000000000, 0x4000000000000000, Some(0x0), 0x4032b23554420a84),
            ("Y4A-13", 0x40e6d7c000000000, 0x40e705a000000000, 0x3fcdfa43fe5c91d1, 0x405d0b851eb851eb, 0x4059000000000000, 0x4000000000000000, Some(0x4000000000000000), 0x3fb0363b09c41c7e),
            ("Y4A-15", 0x40e6d7c000000000, 0x40e705a000000000, 0x3fb8a0902de00d1c, 0x40577eb851eb851e, 0x4059000000000000, 0x4000000000000000, Some(0x4008000000000000), 0x3fc4fdba7bbbab5b),
            ("Y1A-06", 0x404e000000000000, 0x414691f480000000, 0x0, 0x4058ffef9db22d0e, 0x4059000000000000, 0x4000000000000000, Some(0x0), 0x3e15361af22fe494),
            ("Y1A-11", 0x40e5f90000000000, 0x4146924080000000, 0x3fb70a3d70a3d70a, 0x4059000000000000, 0x4059000000000000, 0x4000000000000000, Some(0x0), 0x3fb708d2999e8850),
            ("Y2B-11", 0x40e57e8000000000, 0x40e9fa0000000000, 0x3fb0624dd2f1a9fc, 0x40d8bf36801c4abf, 0x4054000000000000, 0x3ff0000000000000, Some(0x0), 0xbfefffffffffb81d),
        ];
        for &(id, s, m, r, pr, red, f, b, exp) in yield_cases {
            let got = yield_kernel(
                f64::from_bits(s), f64::from_bits(m), f64::from_bits(r),
                f64::from_bits(pr), f64::from_bits(red), f64::from_bits(f),
                b.map(f64::from_bits),
            ).unwrap_or_else(|e| panic!("YIELD {id}: unexpected {e:?}"));
            assert_eq!(got.to_bits(), exp, "YIELD {id}: got {:#018x}, want {exp:#018x}", got.to_bits());
        }

        // PRICE patient-zero probes (PZP-11, 13, 14, 16, 17, 19, 21, 24)
        let price_cases: &[(&str, u64, u64, u64, u64, u64, u64, u64, u64)] = &[
            ("PZP-11", 0x404e000000000000, 0x40e1dd2000000000, 0x3faeb851eb851eb8, 0x3fa999999999999a, 0x4059000000000000, 0x4000000000000000, 0x0, 0x405df6d7d017ad57),
            ("PZP-13", 0x404e000000000000, 0x406e600000000000, 0x3faeb851eb851eb8, 0x3fa999999999999a, 0x4059000000000000, 0x4000000000000000, 0x0, 0x40591f6a3421bfe0),
            ("PZP-14", 0x40e6566000000000, 0x40e6c4c000000000, 0x3fa999999999999a, 0x3fa47ae147ae147b, 0x4059000000000000, 0x4000000000000000, 0x0, 0x405991a792f00f42),
            ("PZP-16", 0x40e6566000000000, 0x40e6698000000000, 0x3fa999999999999a, 0x3fa47ae147ae147b, 0x4059000000000000, 0x4000000000000000, 0x0, 0x4059199bd561ab80),
            ("PZP-17", 0x40e6538000000000, 0x40e7520000000000, 0x3fb758e219652bd4, 0x3fb94467381d7dbf, 0x4059000000000000, 0x3ff0000000000000, 0x0, 0x405832cce049967e),
            ("PZP-19", 0x40e6d7c000000000, 0x40e760e000000000, 0x3fb65fd8adab9f56, 0x3fb694467381d7dc, 0x4059000000000000, 0x4000000000000000, 0x4000000000000000, 0x4058f2e48632674c),
            ("PZP-21", 0x40e6d7c000000000, 0x40e705a000000000, 0x3fb758e219652bd4, 0x3fb9ce075f6fd220, 0x4059000000000000, 0x4000000000000000, 0x4008000000000000, 0x4058c7016d16333d),
            ("PZP-24", 0x40e6d26000000000, 0x40e7d4c000000000, 0x3fb3f141205bc01a, 0x3fb0aa64c2f837b5, 0x4059000000000000, 0x4000000000000000, 0x3ff0000000000000, 0x405a7ddbe1b0309a),
        ];
        for &(id, s, m, r, y, red, f, b, exp) in price_cases {
            let got = price_kernel(
                f64::from_bits(s), f64::from_bits(m), f64::from_bits(r),
                f64::from_bits(y), f64::from_bits(red), f64::from_bits(f),
                Some(f64::from_bits(b)),
            ).unwrap_or_else(|e| panic!("PRICE {id}: unexpected {e:?}"));
            assert_eq!(got.to_bits(), exp, "PRICE {id}: got {:#018x}, want {exp:#018x}", got.to_bits());
        }

        // YIELDDISC & YIELDMAT Gregorian dim(1900, 2) == 28 probes (PZYD-01, 03; PZYM-03)
        assert_eq!(
            yielddisc_kernel(60.0, 2958465.0, 95.0, 100.0, Some(0.0)).unwrap().to_bits(),
            0x3edb4104cb036fc5
        );
        assert_eq!(
            yielddisc_kernel(60.0, 243.0, 98.0, 100.0, Some(0.0)).unwrap().to_bits(),
            0x3fa4c851f9b52011
        );
        assert_eq!(
            yieldmat_kernel(60.0, 243.0, 59.0, 0.05, 99.5, Some(0.0)).unwrap().to_bits(),
            0x3faed9a9e6381cd8
        );
    }

    /// nl27 patient-zero live re-sweep discoveries (Excel 16.0.20625.20108):
    /// 1. YIELD derivative denominator scales by `f * disc1` / `f * r_disc1` and returns #NUM!
    ///    when `f * r_disc1` overflows DBL_MAX (flips at log2(|base^29|) = 1022 for f=4, 1023 for f=2).
    /// 2. d360us checks ed == 31 before start_last_feb -> sd = 30 (Gregorian 1900-02-28 -> 9999-12-31 = 2915941).
    /// 3. Negative sub-unit basis (e.g. -MIN_POS) returns #NUM! across bond_core_family kernels.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn patient_zero_resweep_y3_denom_overflow_d360us_and_neg_basis() {
        let num_err = Err(BondCoreEvalError::Domain(WorksheetErrorCode::Num));
        let red = f64::from_bits(0x3f1fef0875adf64f);
        let tiny_r = f64::from_bits(0x0010000000000005);
        // f=4: pr=1.024e9 finite (4*|b^29| < DBL_MAX), pr=1.025e9 & nl27-YIELD-00001 -> #NUM!
        assert_eq!(
            yield_kernel(45292.0, 47849.0, tiny_r, 1.024e9, red, 4.0, Some(0.0))
                .unwrap()
                .to_bits(),
            0xc242e7987f4592cd
        );
        assert_eq!(
            yield_kernel(45292.0, 47849.0, tiny_r, 1.025e9, red, 4.0, Some(0.0)),
            num_err
        );
        assert_eq!(
            yield_kernel(
                f64::from_bits(0x40e61d9fffffffff),
                47849.0,
                tiny_r,
                f64::from_bits(0x41d003878a4a1991),
                red,
                4.0,
                Some(0.0),
            ),
            num_err
        );
        // f=2: log2(|b^29|)=1022.8 finite, 1023.2 -> #NUM!
        assert_eq!(
            yield_kernel(45292.0, 50406.0, 0.0, 1.121700e9, red, 2.0, Some(0.0)).is_ok(),
            true
        );
        assert_eq!(
            yield_kernel(45292.0, 50406.0, 0.0, 1.132476e9, red, 2.0, Some(0.0)),
            num_err
        );
        // YIELDDISC & YIELDMAT 1900-02-28 (59) -> 9999-12-31 (2958465), basis 0
        assert_eq!(
            yielddisc_kernel(
                59.0,
                2958465.0,
                f64::from_bits(0x4062b485c75e100d),
                100.0,
                Some(0.0),
            )
            .unwrap()
            .to_bits(),
            0xbf0578ff3dd90f37
        );
        assert_eq!(
            yieldmat_kernel(
                60.0,
                2958465.0,
                59.0,
                f64::from_bits(0x3fbbc7c725bc7504),
                f64::from_bits(0x4062b485c75e100d),
                Some(0.0),
            )
            .unwrap()
            .to_bits(),
            0x3fb28ed2799a424f
        );
        // Negative sub-unit basis (-MIN_POS) -> #NUM!
        let neg_min = -f64::MIN_POSITIVE;
        assert_eq!(
            price_kernel(45292.0, 47849.0, 0.05, 0.05, 100.0, 4.0, Some(neg_min)),
            num_err
        );
        assert_eq!(
            duration_kernel(45292.0, 47849.0, 0.05, 0.05, 4.0, Some(neg_min)),
            num_err
        );
        assert_eq!(
            mduration_kernel(45292.0, 47849.0, 0.05, 0.05, 4.0, Some(neg_min)),
            num_err
        );
        assert_eq!(
            yieldmat_kernel(45292.0, 47849.0, 45000.0, 0.05, 100.0, Some(neg_min)),
            num_err
        );
    }

    /// nl27 post-fix live 4,000-row YIELD holdout discoveries (Excel 16.0.20625.20108):
    /// 1. When redemption derivative `t_red` overflows (`!t_red.is_finite()`, any `f`), Excel
    ///    omits `t_red` from `slope` (`slope = -t_cp`), checks `|step| <= 1e-10` first, and
    ///    only returns `#NUM!` if `|step| > 1e-10` (explaining C01 + Y2B-06..15 + f=2).
    /// 2. The multi-period par-100 seed uses `yrs = k_yr + frac` with `f` cancelled and
    ///    flushes subnormal `yrs * rate` to `0.0` (explaining C02 + C03).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn yield_holdout_tred_deferred_overflow_and_yrs_rate_seed() {
        let num_err = Err(BondCoreEvalError::Domain(WorksheetErrorCode::Num));
        // C01 exact 1-ULP frontier (f=1, b=3): lo has finite t_red -> Ok(y_1), hi has t_red=+inf
        // and |pv/t_cp| = 2.13e-5 > 1e-10 -> #NUM!
        let r_c01 = f64::from_bits(0x3fdbdd9c78606766);
        let red_c01 = f64::from_bits(0x40a087957aaf1c15);
        assert_eq!(
            yield_kernel(
                264053.0,
                288025.0,
                r_c01,
                f64::from_bits(0x41366ea781b2db7e),
                red_c01,
                1.0,
                Some(3.0),
            )
            .unwrap()
            .to_bits(),
            0xbfefffc4c824c579
        );
        assert_eq!(
            yield_kernel(
                264053.0,
                288025.0,
                r_c01,
                f64::from_bits(0x41366ea781b2db7f),
                red_c01,
                1.0,
                Some(3.0),
            ),
            num_err
        );
        // C02 & C03: yrs * rate subnormal flush at yrs < 1.0 and exact rate at yrs >= 1.0
        let tiny_r = f64::from_bits(0x0010000000000003);
        assert_eq!(
            yield_kernel(45293.0, 45658.0, tiny_r, 100.0, 100.0, 2.0, Some(0.0))
                .unwrap()
                .to_bits(),
            0x0000000000000000
        );
        assert_eq!(
            yield_kernel(45292.0, 45658.0, tiny_r, 100.0, 100.0, 2.0, Some(0.0))
                .unwrap()
                .to_bits(),
            0x0010000000000003
        );
        assert_eq!(
            yield_kernel(36526.0, 45658.0, tiny_r, 100.0, 100.0, 2.0, Some(0.0))
                .unwrap()
                .to_bits(),
            0x0010000000000003
        );
        assert_eq!(
            yield_kernel(45458.0, 45658.0, f64::from_bits(0x0020000000000000), 100.0, 100.0, 4.0, Some(0.0))
                .unwrap()
                .to_bits(),
            0x0020000000000000
        );
        assert_eq!(
            yield_kernel(45496.0, 45658.0, f64::from_bits(0x0020000000000000), 100.0, 100.0, 4.0, Some(0.0))
                .unwrap()
                .to_bits(),
            0x0000000000000000
        );
    }
}
