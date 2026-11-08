//! The securities family fragment ported from OxFunc (functions/ at 6fca3518): bond_core_family.rs
//! (PRICE, YIELD, DURATION, MDURATION, PRICEMAT, YIELDMAT, YIELDDISC, ACCRINT, ACCRINTM),
//! coupon_family.rs (the COUP functions), discount_bill_yearfrac_family.rs (DISC, INTRATE,
//! RECEIVED, PRICEDISC and the TBILL functions) and odd_bond_family.rs (ODDFPRICE, ODDFYIELD,
//! ODDLPRICE, ODDLYIELD), with day_count_common.rs. The kernels are OxFunc's
//! ([`crate::ported`]) and count serials in the 1900 date system.
//!
//! Every argument is read through worksheet number coercion, as OxFunc's surfaces read them; an
//! omitted basis is the kernel's default. Dates count in the date system of the call's
//! environment ([`crate::locale::CalcEnvironment`]), converted to the 1900 system for the kernel
//! and back for COUPNCD's and COUPPCD's results; a call without an environment is refused as not
//! modelled.

use super::financial::{OFFSET_1904, date_1900, required_num, required_number};
use super::kit::{fail, given, num, num_without_logical};
use crate::call::EvalServices;
use crate::kernel::{Site, Step};
use crate::locale::DateSystem;
use crate::ported::bond_core_family as bond;
use crate::ported::coupon_family as coupon;
use crate::ported::discount_bill_yearfrac_family as bill;
use crate::ported::odd_bond_family as odd;
use crate::value::{CalcValue, WorksheetErrorCode};

const CODE: u16 = 301;

/// The optional basis at `index`: `None` when the call does not reach it.
fn basis(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<Option<f64>> {
    if index < a.len() {
        num(site, a, index).map(Some)
    } else {
        Ok(None)
    }
}

/// An optional argument at `index`, or `default` when it is omitted.
fn or(site: &Site<'_>, a: &[CalcValue], index: usize, default: f64) -> Step<f64> {
    if given(a, index) { num(site, a, index) } else { Ok(default) }
}

/// The bill functions admit and validate each date before reading the next scalar.
fn bill_arguments(site: &Site<'_>, a: &[CalcValue], sv: &EvalServices<'_>) -> Step<[f64; 3]> {
    required_number(&a[0])?;
    let settlement = super::kit::checked_date_serial(date_1900(site, a, 0, sv)?)?;
    required_number(&a[1])?;
    let maturity = super::kit::checked_date_serial(date_1900(site, a, 1, sv)?)?;
    let rate = super::kit::num_without_logical(site, a, 2)?;
    Ok([settlement, maturity, rate])
}

/// The yield entries admit each calendar serial before the next argument.
/// Fn8 conflicts retain negative/beyond-calendar NUM ahead of later errors;
/// zero and the fictitious1900 day still reach the existing bond kernel.
fn yield_date(site: &Site<'_>, a: &[CalcValue], index: usize, sv: &EvalServices<'_>) -> Step<f64> {
    required_number(&a[index])?;
    super::kit::checked_date_serial(date_1900(site, a, index, sv)?)
}

fn bond_out(site: &Site<'_>, result: Result<f64, bond::BondCoreEvalError>) -> Step<CalcValue> {
    match result {
        Ok(n) => site.number(n),
        Err(error) => fail(bond::map_bond_core_error_to_ws(&error)),
    }
}

fn bill_out(site: &Site<'_>, result: Result<f64, bill::DiscountBillYearfracEvalError>) -> Step<CalcValue> {
    match result {
        Ok(n) => site.number(n),
        Err(error) => fail(bill::map_discount_bill_yearfrac_error_to_ws(&error)),
    }
}

fn odd_out(site: &Site<'_>, result: Result<f64, odd::OddBondEvalError>) -> Step<CalcValue> {
    match result {
        Ok(n) => site.number(n),
        Err(error) => fail(odd::map_odd_bond_error_to_ws(&error)),
    }
}

fn coupon_out(site: &Site<'_>, result: Result<f64, WorksheetErrorCode>) -> Step<CalcValue> {
    match result {
        Ok(n) => site.number(n),
        Err(code) => fail(code),
    }
}

/// A 1900-system serial the kernel returned as a date, in the call's date system: before
/// 1 January 1904 is #NUM! in the 1904 system.
fn date_out(site: &Site<'_>, sv: &EvalServices<'_>, result: Result<f64, WorksheetErrorCode>) -> Step<CalcValue> {
    let serial = match result {
        Ok(serial) => serial,
        Err(code) => return fail(code),
    };
    let Some(environment) = sv.environment() else {
        return Err(site.not_modelled(None));
    };
    match environment.date_system {
        DateSystem::System1900 => site.number(serial),
        DateSystem::System1904 if serial >= OFFSET_1904 => site.number(serial - OFFSET_1904),
        DateSystem::System1904 => fail(WorksheetErrorCode::Num),
    }
}

/// A COUP function: its settlement and maturity dates, frequency and basis. The dates are checked
/// before the frequency and basis are read: a settlement that is no date or on or after maturity
/// is #NUM! whatever they hold (Excel 16.0.20430, 2026-10-25, dc-6au: `=COUPDAYBS(1,1,#REF!)`,
/// `=COUPDAYBS(1,1,2,#REF!)`, `=COUPDAYBS(-1,1,#REF!)` and `=COUPDAYBS(2,1,"a")` are #NUM!, while
/// `=COUPDAYBS(1,2,#REF!)` is #REF! and `=COUPDAYBS(1,2,"a")` #VALUE!; the six COUP functions
/// alike).
fn coupon_args(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &EvalServices<'_>,
) -> Step<(f64, f64, f64, Option<f64>)> {
    let (settlement, maturity) = (date_1900(site, a, 0, sv)?, date_1900(site, a, 1, sv)?);
    if let Err(code) = coupon::check_coupon_dates(settlement, maturity) {
        return fail(code);
    }
    Ok((settlement, maturity, num(site, a, 2)?, basis(site, a, 3)?))
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
ported_family! {
    code: CODE;
    1 price_fn "PRICE" (6, 7) [V "settlement", V "maturity", V "rate", V "yld", V "redemption", V "frequency", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let (settlement, maturity) = (date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?);
        bond_out(s, bond::price_kernel(settlement, maturity, num(s, a, 2)?, num(s, a, 3)?, num(s, a, 4)?, num(s, a, 5)?, Some(or(s, a, 6, 0.0)?)))
    }
    2 yield_fn "YIELD" (6, 7) [V "settlement", V "maturity", V "rate", V "pr", V "redemption", V "frequency", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let settlement = yield_date(s, a, 0, sv)?;
        let maturity = yield_date(s, a, 1, sv)?;
        let (rate, price, redemption) = (required_num(s, a, 2)?, required_num(s, a, 3)?, required_num(s, a, 4)?);
        // Fn8 adjacent-error controls read basis before frequency in YIELD.
        let basis = if given(a, 6) { num_without_logical(s, a, 6)? } else { 0.0 };
        let frequency = required_num(s, a, 5)?;
        bond_out(s, bond::yield_kernel(settlement, maturity, rate, price, redemption, frequency, Some(basis)))
    }
    3 duration_fn "DURATION" (5, 6) [V "settlement", V "maturity", V "coupon", V "yld", V "frequency", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let (settlement, maturity) = (date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?);
        bond_out(s, bond::duration_kernel(settlement, maturity, num(s, a, 2)?, num(s, a, 3)?, num(s, a, 4)?, Some(or(s, a, 5, 0.0)?)))
    }
    4 mduration_fn "MDURATION" (5, 6) [V "settlement", V "maturity", V "coupon", V "yld", V "frequency", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let (settlement, maturity) = (date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?);
        bond_out(s, bond::mduration_kernel(settlement, maturity, num(s, a, 2)?, num(s, a, 3)?, num(s, a, 4)?, Some(or(s, a, 5, 0.0)?)))
    }
    5 pricemat_fn "PRICEMAT" (5, 6) [V "settlement", V "maturity", V "issue", V "rate", V "yld", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let (settlement, maturity, issue) = (date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?, date_1900(s, a, 2, sv)?);
        bond_out(s, bond::pricemat_kernel(settlement, maturity, issue, num(s, a, 3)?, num(s, a, 4)?, Some(or(s, a, 5, 0.0)?)))
    }
    6 yieldmat_fn "YIELDMAT" (5, 6) [V "settlement", V "maturity", V "issue", V "rate", V "pr", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let (settlement, maturity, issue) = (date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?, date_1900(s, a, 2, sv)?);
        bond_out(s, bond::yieldmat_kernel(settlement, maturity, issue, num(s, a, 3)?, num(s, a, 4)?, Some(or(s, a, 5, 0.0)?)))
    }
    7 yielddisc_fn "YIELDDISC" (4, 5) [V "settlement", V "maturity", V "pr", V "redemption", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let (settlement, maturity) = (date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?);
        bond_out(s, bond::yielddisc_kernel(settlement, maturity, num(s, a, 2)?, num(s, a, 3)?, Some(or(s, a, 4, 0.0)?)))
    }
    8 accrint_fn "ACCRINT" (6, 8) [V "issue", V "first_interest", V "settlement", V "rate", V "par", V "frequency", V "basis", V "calc_method"] three_d (None, Some(&[]), None); |s, a, sv| {
        let (issue, first, settlement) = (date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?, date_1900(s, a, 2, sv)?);
        let par = or(s, a, 4, 1000.0)?;
        let calc_method = if given(a, 7) { num(s, a, 7)?.trunc() != 0.0 } else { true };
        bond_out(s, bond::accrint_kernel(issue, first, settlement, num(s, a, 3)?, Some(par), num(s, a, 5)?, Some(or(s, a, 6, 0.0)?), Some(calc_method)))
    }
    // ACCRINTM takes 4 or 5 arguments (dc-mx3x, Excel 16.0.20527, 2026-11-07, Formula2 and Formula
    // alike): `=ACCRINTM(B1,B1,B1)` is refused and `=ACCRINTM(B1,B1,B1,B1)` enters.
    9 accrintm_fn "ACCRINTM" (4, 5) [V "issue", V "settlement", V "rate", V "par", V "basis"] three_d (None, Some(&[3]), None); |s, a, sv| {
        let (issue, settlement) = (date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?);
        bond_out(s, bond::accrintm_kernel(issue, settlement, num(s, a, 2)?, Some(or(s, a, 3, 1000.0)?), Some(or(s, a, 4, 0.0)?)))
    }
    10 coupdaybs_fn "COUPDAYBS" (3, 4) [V "settlement", V "maturity", V "frequency", V "basis"] three_d (None, Some(&[0, 1]), Some(&[2, 3])); |s, a, sv| {
        let (settlement, maturity, frequency, b) = coupon_args(s, a, sv)?;
        coupon_out(s, coupon::coupdaybs_kernel(settlement, maturity, frequency, b))
    }
    11 coupdays_fn "COUPDAYS" (3, 4) [V "settlement", V "maturity", V "frequency", V "basis"] three_d (None, Some(&[0, 1]), Some(&[2, 3])); |s, a, sv| {
        let (settlement, maturity, frequency, b) = coupon_args(s, a, sv)?;
        coupon_out(s, coupon::coupdays_kernel(settlement, maturity, frequency, b))
    }
    12 coupdaysnc_fn "COUPDAYSNC" (3, 4) [V "settlement", V "maturity", V "frequency", V "basis"] three_d (None, Some(&[0, 1]), Some(&[2, 3])); |s, a, sv| {
        let (settlement, maturity, frequency, b) = coupon_args(s, a, sv)?;
        coupon_out(s, coupon::coupdaysnc_kernel(settlement, maturity, frequency, b))
    }
    13 coupncd_fn "COUPNCD" (3, 4) [V "settlement", V "maturity", V "frequency", V "basis"] three_d (None, Some(&[0, 1]), Some(&[2, 3])); |s, a, sv| {
        let (settlement, maturity, frequency, b) = coupon_args(s, a, sv)?;
        date_out(s, sv, coupon::coupncd_kernel(settlement, maturity, frequency, b))
    }
    14 coupnum_fn "COUPNUM" (3, 4) [V "settlement", V "maturity", V "frequency", V "basis"] three_d (None, Some(&[0, 1]), Some(&[2, 3])); |s, a, sv| {
        let (settlement, maturity, frequency, b) = coupon_args(s, a, sv)?;
        coupon_out(s, coupon::coupnum_kernel(settlement, maturity, frequency, b))
    }
    15 couppcd_fn "COUPPCD" (3, 4) [V "settlement", V "maturity", V "frequency", V "basis"] three_d (None, Some(&[0, 1]), Some(&[2, 3])); |s, a, sv| {
        let (settlement, maturity, frequency, b) = coupon_args(s, a, sv)?;
        date_out(s, sv, coupon::couppcd_kernel(settlement, maturity, frequency, b))
    }
    16 disc_fn "DISC" (4, 5) [V "settlement", V "maturity", V "pr", V "redemption", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let (settlement, maturity) = (date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?);
        bill_out(s, bill::disc_kernel(settlement, maturity, num(s, a, 2)?, num(s, a, 3)?, basis(s, a, 4)?))
    }
    17 intrate_fn "INTRATE" (4, 5) [V "settlement", V "maturity", V "investment", V "redemption", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let (settlement, maturity) = (date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?);
        bill_out(s, bill::intrate_kernel(settlement, maturity, num(s, a, 2)?, num(s, a, 3)?, basis(s, a, 4)?))
    }
    18 received_fn "RECEIVED" (4, 5) [V "settlement", V "maturity", V "investment", V "discount", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let (settlement, maturity) = (date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?);
        bill_out(s, bill::received_kernel(settlement, maturity, num(s, a, 2)?, num(s, a, 3)?, basis(s, a, 4)?))
    }
    19 pricedisc_fn "PRICEDISC" (4, 5) [V "settlement", V "maturity", V "discount", V "redemption", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let (settlement, maturity) = (date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?);
        bill_out(s, bill::pricedisc_kernel(settlement, maturity, num(s, a, 2)?, num(s, a, 3)?, basis(s, a, 4)?))
    }
    20 tbilleq_fn "TBILLEQ" (3, 3) [V "settlement", V "maturity", V "discount"] three_d (None, Some(&[]), None); |s, a, sv| {
        let [settlement, maturity, rate] = bill_arguments(s, a, sv)?;
        bill_out(s, bill::tbilleq_kernel(settlement, maturity, rate))
    }
    21 tbillprice_fn "TBILLPRICE" (3, 3) [V "settlement", V "maturity", V "discount"] three_d (None, Some(&[]), None); |s, a, sv| {
        let [settlement, maturity, rate] = bill_arguments(s, a, sv)?;
        bill_out(s, bill::tbillprice_kernel(settlement, maturity, rate))
    }
    22 tbillyield_fn "TBILLYIELD" (3, 3) [V "settlement", V "maturity", V "pr"] three_d (None, Some(&[]), None); |s, a, sv| {
        let [settlement, maturity, rate] = bill_arguments(s, a, sv)?;
        bill_out(s, bill::tbillyield_kernel(settlement, maturity, rate))
    }
    23 oddfprice_fn "ODDFPRICE" (8, 9) [V "settlement", V "maturity", V "issue", V "first_coupon", V "rate", V "yld", V "redemption", V "frequency", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let dates = [date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?, date_1900(s, a, 2, sv)?, date_1900(s, a, 3, sv)?];
        odd_out(s, odd::oddfprice_kernel(dates[0], dates[1], dates[2], dates[3], num(s, a, 4)?, num(s, a, 5)?, num(s, a, 6)?, num(s, a, 7)?, basis(s, a, 8)?))
    }
    24 oddfyield_fn "ODDFYIELD" (8, 9) [V "settlement", V "maturity", V "issue", V "first_coupon", V "rate", V "pr", V "redemption", V "frequency", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let dates = [yield_date(s, a, 0, sv)?, yield_date(s, a, 1, sv)?, yield_date(s, a, 2, sv)?, yield_date(s, a, 3, sv)?];
        let (rate, price, redemption, frequency) = (required_num(s, a, 4)?, required_num(s, a, 5)?, required_num(s, a, 6)?, required_num(s, a, 7)?);
        // ODDFYIELD keeps frequency before basis, unlike YIELD.
        let basis = if a.len() > 8 { Some(num_without_logical(s, a, 8)?) } else { None };
        odd_out(s, odd::oddfyield_kernel(dates[0], dates[1], dates[2], dates[3], rate, price, redemption, frequency, basis))
    }
    25 oddlprice_fn "ODDLPRICE" (7, 8) [V "settlement", V "maturity", V "last_interest", V "rate", V "yld", V "redemption", V "frequency", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let dates = [date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?, date_1900(s, a, 2, sv)?];
        odd_out(s, odd::oddlprice_kernel(dates[0], dates[1], dates[2], num(s, a, 3)?, num(s, a, 4)?, num(s, a, 5)?, num(s, a, 6)?, basis(s, a, 7)?))
    }
    26 oddlyield_fn "ODDLYIELD" (7, 8) [V "settlement", V "maturity", V "last_interest", V "rate", V "pr", V "redemption", V "frequency", V "basis"] three_d (None, Some(&[]), None); |s, a, sv| {
        let dates = [date_1900(s, a, 0, sv)?, date_1900(s, a, 1, sv)?, date_1900(s, a, 2, sv)?];
        odd_out(s, odd::oddlyield_kernel(dates[0], dates[1], dates[2], num(s, a, 3)?, num(s, a, 4)?, num(s, a, 5)?, num(s, a, 6)?, basis(s, a, 7)?))
    }
}
