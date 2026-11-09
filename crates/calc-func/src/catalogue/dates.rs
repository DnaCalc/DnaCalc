//! The date and time family fragment ported from OxFunc (functions/ at 6fca3518): date_fn.rs,
//! date_parts_family.rs, date_value_family.rs, date_week_family.rs, day_count_common.rs,
//! workday_networkdays_family.rs, discount_bill_yearfrac_family.rs's YEARFRAC, now_fn.rs and
//! today_fn.rs. The kernels are in [`crate::ported::date_kernels`] and count in the 1900 date
//! system; this fragment converts the serials of a 1904 workbook to and from it.
//!
//! Serial dates count in the date system of the call's environment
//! ([`crate::locale::CalcEnvironment`]); a call without one is refused as not modelled, and so
//! are NOW and TODAY when the environment holds no clock. A date given as text is read as VALUE
//! reads it ([`crate::value_text`]), which needs the environment's locale profile.
//!
//! EDATE, EOMONTH and WEEKNUM's date give #VALUE! for a logical and #N/A for an omitted
//! argument. Fn7 measures #N/A for omitted required WORKDAY and NETWORKDAYS arguments too;
//! a blank reference supplies serial zero. A holiday list reads the numbers its cells hold.

use super::kit::{fail, num, out, reject_logical, text_arg, values};
use crate::call::EvalServices;
use crate::kernel::{Exit, Site, Step};
use crate::locale::{CalcEnvironment, DateSystem};
use crate::ported::date_kernels::{self as k, OFFSET_1904, WeekendMask};
use crate::value::{CalcValue, CoreRef, WorksheetErrorCode};
use std::collections::BTreeSet;

type W = WorksheetErrorCode;

use calc_catalogue::ported::dates::{CODE, ENTRIES as DECLARATIONS};

/// The call's environment, or the refusal of a call that has none.
fn environment<'a>(site: &'a Site<'_>) -> Step<&'a CalcEnvironment> {
    site.environment().ok_or_else(|| site.not_modelled(None))
}

fn system(site: &Site<'_>) -> Step<DateSystem> {
    environment(site).map(|environment| environment.date_system)
}

/// A serial of the call's date system as a 1900 serial; a negative serial stays negative.
fn to_1900(system: DateSystem, serial: f64) -> f64 {
    match system {
        DateSystem::System1904 if serial >= 0.0 => serial + OFFSET_1904,
        _ => serial,
    }
}

/// A 1900 serial as a serial of the call's date system: before 1 January 1904 is #NUM! there.
fn from_1900(system: DateSystem, serial: f64) -> Result<f64, W> {
    match system {
        DateSystem::System1900 => Ok(serial),
        DateSystem::System1904 => {
            let serial = serial - OFFSET_1904;
            if serial < 0.0 {
                Err(W::Num)
            } else {
                Ok(serial)
            }
        }
    }
}

/// Argument `index` as a date of the call's system, read as a 1900 serial.
fn date_arg(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<f64> {
    let system = system(site)?;
    Ok(to_1900(system, num(site, a, index)?))
}

/// A date argument under OxFunc's strict rule: a logical is #VALUE!, an omitted one #N/A.
fn strict_date_arg(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<f64> {
    match a[index].core_ref() {
        CoreRef::Logical(_) => fail(W::Value),
        CoreRef::Missing => fail(W::NA),
        _ => date_arg(site, a, index),
    }
}

/// A number argument under the strict rule.
fn strict_num(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<f64> {
    match a[index].core_ref() {
        CoreRef::Logical(_) => fail(W::Value),
        CoreRef::Missing => fail(W::NA),
        _ => num(site, a, index),
    }
}

/// Fn7's required WORKDAY/NETWORKDAYS slots: omitted is #N/A; a blank reads as zero.
fn present_num(site: &Site<'_>, a: &[CalcValue], index: usize) -> Step<f64> {
    match a[index].core_ref() {
        CoreRef::Missing => fail(W::NA),
        _ => num(site, a, index),
    }
}

/// A serial result of a 1900 kernel, in the call's system.
fn serial_out(site: &Site<'_>, result: Result<f64, W>) -> Step<CalcValue> {
    let system = system(site)?;
    out(site, result.and_then(|serial| from_1900(system, serial)))
}

/// A unary kernel of a date.
fn of_date(site: &Site<'_>, a: &[CalcValue], kernel: fn(f64) -> Result<f64, W>) -> Step<CalcValue> {
    let serial = date_arg(site, a, 0)?;
    out(site, kernel(serial))
}

/// The weekend of WORKDAY.INTL and NETWORKDAYS.INTL: Saturday and Sunday when the call leaves it
/// out, a code, or a mask of seven 0 and 1 characters from Monday.
fn weekend(site: &Site<'_>, a: &[CalcValue], index: usize, workday: bool) -> Step<WeekendMask> {
    let Some(value) = a.get(index) else {
        return Ok(WeekendMask::saturday_sunday());
    };
    match value.core_ref() {
        CoreRef::Missing | CoreRef::Empty => Ok(WeekendMask::saturday_sunday()),
        // An error value is no weekend: WORKDAY.INTL's #NUM!, NETWORKDAYS.INTL's #VALUE! (Excel
        // 16.0.20430, 2026-10-25, dc-6au: `=WORKDAY.INTL(1,1,#REF!)` and `(1,1,#DIV/0!)` #NUM!,
        // `=NETWORKDAYS.INTL(1,1,#REF!)` and `(1,1,#DIV/0!)` #VALUE!).
        CoreRef::Error(_) if workday => fail(W::Num),
        CoreRef::Error(_) => fail(W::Value),
        CoreRef::Text(text) => {
            let raw = text.to_string_lossy();
            if raw.len() == 7 && raw.chars().all(|ch| ch == '0' || ch == '1') {
                let mask = WeekendMask::from_mask_text(&raw).expect("a checked mask");
                if mask.is_all_days() && workday {
                    return fail(W::Value);
                }
                return Ok(mask);
            }
            let code = num(site, a, index)?;
            WeekendMask::from_number(code.trunc() as i64).ok_or(Exit::Error(W::Num))
        }
        _ => {
            let code = num(site, a, index)?;
            if !code.is_finite() {
                return fail(W::Num);
            }
            WeekendMask::from_number(code.trunc() as i64).ok_or(Exit::Error(W::Num))
        }
    }
}

/// The holidays a list gives, as 1900 serials: each number it holds (text, logicals and blank
/// cells in a reference or an array are skipped; a direct value is read as a number).
fn holidays(
    site: &Site<'_>,
    a: &[CalcValue],
    index: usize,
    sv: &mut EvalServices<'_>,
) -> Step<BTreeSet<i64>> {
    let mut found = BTreeSet::new();
    if index >= a.len() || a[index].is_missing() {
        return Ok(found);
    }
    let system = system(site)?;
    let mut numbers = Vec::new();
    values(site, a, index, index + 1, sv, &mut |held, value| {
        let n = if held {
            match value.core_ref() {
                CoreRef::Number(n) => n,
                CoreRef::Error(code) => return fail(code),
                CoreRef::Array(_) | CoreRef::Reference(_) => {
                    return Err(site.not_modelled(Some(index)));
                }
                _ => return Ok(()),
            }
        } else {
            site.number_argument(index, value)?
        };
        numbers.push(n);
        Ok(())
    })?;
    for n in numbers {
        found.insert(k::serial_from_number(to_1900(system, n)).map_err(Exit::Error)?);
    }
    Ok(found)
}

fn workday(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    intl: bool,
) -> Step<CalcValue> {
    reject_logical(&a[0])?;
    let start = super::kit::checked_date_serial(to_1900(system(site)?, present_num(site, a, 0)?))?;
    reject_logical(&a[1])?;
    let days = present_num(site, a, 1)?;
    let (mask, list) = if intl {
        (weekend(site, a, 2, true)?, 3)
    } else {
        (WeekendMask::saturday_sunday(), 2)
    };
    let holidays = holidays(site, a, list, sv)?;
    serial_out(site, k::workday(start, days, mask, &holidays))
}

fn networkdays(
    site: &Site<'_>,
    a: &[CalcValue],
    sv: &mut EvalServices<'_>,
    intl: bool,
) -> Step<CalcValue> {
    // NETWORKDAYS.INTL decodes its weekend before reading either date.
    let (mask, list) = if intl {
        (weekend(site, a, 2, false)?, 3)
    } else {
        (WeekendMask::saturday_sunday(), 2)
    };
    reject_logical(&a[0])?;
    let system = system(site)?;
    let start = super::kit::checked_date_serial(to_1900(system, present_num(site, a, 0)?))?;
    reject_logical(&a[1])?;
    let end = super::kit::checked_date_serial(to_1900(system, present_num(site, a, 1)?))?;
    let holidays = holidays(site, a, list, sv)?;
    out(site, k::networkdays(start, end, mask, &holidays))
}

/// The optional return type of WEEKDAY and WEEKNUM.
fn return_type(site: &Site<'_>, a: &[CalcValue], strict: bool) -> Step<Option<f64>> {
    match a.get(1) {
        None => Ok(None),
        // WEEKDAY reads an explicit placeholder as zero; WEEKNUM keeps its default.
        Some(value) if value.is_missing() && strict => Ok(None),
        Some(_) if strict => strict_num(site, a, 1).map(Some),
        Some(_) => num(site, a, 1).map(Some),
    }
}

/// The calculation's clock as a 1900 serial, or the refusal of a call without one.
fn now(site: &Site<'_>) -> Step<f64> {
    environment(site)?
        .now
        .filter(|now| now.is_finite() && *now >= 0.0)
        .ok_or_else(|| site.not_modelled(None))
}

// 3-D positions: dc-10y, Excel 16.0.20430, 2026-10-25; catalogue oracle ThreeD sheet.
ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    1 date_fn |s, a, sv| {
        let system = system(s)?;
        let (y, m, d) = (num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?);
        out(s, k::date(y, m, d, system))
    }
    2 time_fn |s, a, sv| {
        let (h, m, sec) = (num(s, a, 0)?, num(s, a, 1)?, num(s, a, 2)?);
        out(s, k::time(h, m, sec))
    }
    3 year_fn |s, a, sv| {
        of_date(s, a, |x| k::serial_to_ymd(x).map(|(y, _, _)| y as f64))
    }
    4 month_fn |s, a, sv| {
        of_date(s, a, |x| k::serial_to_ymd(x).map(|(_, m, _)| m as f64))
    }
    5 day_fn |s, a, sv| {
        of_date(s, a, |x| k::serial_to_ymd(x).map(|(_, _, d)| d as f64))
    }
    6 hour_fn |s, a, sv| {
        of_date(s, a, |x| k::hms(x).map(|(h, _, _)| h))
    }
    7 minute_fn |s, a, sv| {
        of_date(s, a, |x| k::hms(x).map(|(_, m, _)| m))
    }
    8 second_fn |s, a, sv| {
        of_date(s, a, |x| k::hms(x).map(|(_, _, sec)| sec))
    }
    9 days_fn |s, a, sv| {
        if let Some(code) = a.iter().find_map(CalcValue::as_error) {
            return fail(code);
        }
        let end = date_arg(s, a, 0)?;
        let start = date_arg(s, a, 1)?;
        out(s, k::days(end, start))
    }
    10 days360_fn |s, a, sv| {
        if let Some(code) = a.iter().find_map(CalcValue::as_error) {
            return fail(code);
        }
        let start = date_arg(s, a, 0)?;
        let end = date_arg(s, a, 1)?;
        let european = a.len() > 2 && num(s, a, 2)? != 0.0;
        out(s, k::days360(start, end, european))
    }
    11 datedif_fn |s, a, sv| {
        if let Some(code) = a.iter().find_map(CalcValue::as_error) {
            return fail(code);
        }
        let start = date_arg(s, a, 0)?;
        let end = date_arg(s, a, 1)?;
        let unit = text_arg(s, a, 2)?;
        let unit = k::datedif_unit(&unit.to_string_lossy()).map_err(Exit::Error)?;
        out(s, k::datedif(start, end, unit))
    }
    12 datevalue_fn |s, a, sv| {
        let reading = super::text_format::read_value(s, a, 0)?;
        match reading {
            crate::value_text::Reading::DateTime(serial) => s.number(serial.floor()),
            crate::value_text::Reading::Number(_) => fail(W::Value),
        }
    }
    13 timevalue_fn |s, a, sv| {
        let reading = super::text_format::read_value(s, a, 0)?;
        match reading {
            crate::value_text::Reading::DateTime(serial) => s.number(serial - serial.floor()),
            crate::value_text::Reading::Number(_) => fail(W::Value),
        }
    }
    14 edate_fn |s, a, sv| {
        let start = strict_date_arg(s, a, 0)?;
        let months = strict_num(s, a, 1)?;
        serial_out(s, k::edate(start, months))
    }
    15 eomonth_fn |s, a, sv| {
        let start = strict_date_arg(s, a, 0)?;
        let months = strict_num(s, a, 1)?;
        serial_out(s, k::eomonth(start, months))
    }
    16 weekday_fn |s, a, sv| {
        let serial = date_arg(s, a, 0)?;
        let kind = return_type(s, a, false)?;
        out(s, k::weekday(serial, kind))
    }
    17 weeknum_fn |s, a, sv| {
        let serial = strict_date_arg(s, a, 0)?;
        let kind = return_type(s, a, true)?;
        // Q3, dc-1943: these types can inherit the preceding WEEKNUM call's state.
        // The function service has no such state, so it cannot promise a result.
        if (0.0..(k::MAX_SERIAL_1900 + 1) as f64).contains(&serial)
            && kind.is_some_and(|v| !(0.0..22.0).contains(&k::date_integer_floor(v))) {
            return Err(s.not_modelled(Some(1)));
        }
        out(s, k::weeknum(serial, kind))
    }
    18 isoweeknum_fn |s, a, sv| {
        let serial = date_arg(s, a, 0)?;
        out(s, k::isoweeknum(serial))
    }
    19 workday_fn |s, a, sv| {
        workday(s, a, sv, false)
    }
    20 workday_intl_fn |s, a, sv| {
        workday(s, a, sv, true)
    }
    21 networkdays_fn |s, a, sv| {
        networkdays(s, a, sv, false)
    }
    22 networkdays_intl_fn |s, a, sv| {
        networkdays(s, a, sv, true)
    }
    23 yearfrac_fn |s, a, sv| {
        let start = super::kit::checked_date_serial(strict_date_arg(s, a, 0)?)?;
        let end = super::kit::checked_date_serial(strict_date_arg(s, a, 1)?)?;
        let basis = if a.len() > 2 && !a[2].is_missing() { super::kit::num_without_logical(s, a, 2)? } else { 0.0 };
        out(s, k::yearfrac(start, end, basis))
    }
    24 now_fn |s, a, sv| {
        let now = now(s)?;
        serial_out(s, Ok(now))
    }
    25 today_fn |s, a, sv| {
        let now = now(s)?;
        serial_out(s, Ok(now.floor()))
    }
}
