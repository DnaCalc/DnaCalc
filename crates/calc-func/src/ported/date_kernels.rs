//! The date and time kernels ported from OxFunc (crates/oxfunc_core/src/functions/ at 6fca3518):
//! date_fn.rs, date_parts_family.rs, date_week_family.rs, date_value_family.rs (DAYS360 and
//! DATEDIF), day_count_common.rs, workday_networkdays_family.rs and
//! discount_bill_yearfrac_family.rs (YEARFRAC), with the OxFunc surface layer removed.
//!
//! Every kernel here counts serials in the 1900 date system, as OxFunc's do; the date family
//! fragment (`catalogue/dates.rs`) converts a 1904-system serial to and from it
//! ([`OFFSET_1904`]).

use crate::locale::{DateSystem, excel_serial_from_ymd, ymd_from_excel_serial};
use crate::value::WorksheetErrorCode;
use std::collections::BTreeSet;

type W = WorksheetErrorCode;

/// The largest serial, 31 December 9999, in the 1900 system.
pub(crate) const MAX_SERIAL_1900: i64 = 2_958_465;

/// The 1900-system serial of 1 January 1904, serial 0 of the 1904 system.
pub(crate) const OFFSET_1904: f64 = 1462.0;

pub(crate) use super::text_kernels::date_integer_floor;

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m, d)
}

/// The 1900 serial of a date, with Excel's 29 February 1900 and day 0 of January 1900, for any
/// day of the month (a day past the month's end runs into the next month).
pub(crate) fn serial_unbounded_1900(year: i64, month: i64, day: i64) -> i64 {
    let base = days_from_civil(1899, 12, 31);
    let month_start = days_from_civil(year, month, 1) - base;
    let month_start = if month_start >= 60 {
        month_start + 1
    } else {
        month_start
    };
    month_start + day - 1
}

/// The year, month and day of a whole 1900 serial (0 is 0 January 1900, 60 is 29 February 1900).
pub(crate) fn ymd_1900(serial: i64) -> (i64, i64, i64) {
    if serial == 0 {
        return (1900, 1, 0);
    }
    if serial == 60 {
        return (1900, 2, 29);
    }
    let adjusted = if serial > 60 { serial - 1 } else { serial };
    civil_from_days(days_from_civil(1899, 12, 31) + adjusted)
}

pub(crate) fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub(crate) fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

// --- DATE and TIME (date_fn.rs, date_parts_family.rs) ------------------------------------------

/// DATE in the 1900 system: the year capped and read as Excel's signed 16-bit year, years below
/// 1900 offset by 1900, the month normalized into the year, and the day running on.
pub(crate) fn date(raw_year: f64, raw_month: f64, raw_day: f64, system: DateSystem) -> Result<f64, W> {
    if !raw_year.is_finite() || !raw_month.is_finite() || !raw_day.is_finite() {
        return Err(W::Num);
    }
    let original_year = date_integer_floor(raw_year).min(10_000.0);
    let year_bits = if (-2_147_483_648.0..2_147_483_648.0).contains(&original_year) {
        original_year as i64
    } else {
        0
    };
    let year = year_bits as i16 as i64;
    let year = year + if year < 1900 { 1900 } else { 0 };
    let month = date_integer_floor(raw_month);
    if !(-32_767.0..32_767.0).contains(&month) {
        return Err(W::Num);
    }
    let month = month as i64;
    let day = date_integer_floor(raw_day);
    let day = if (-32_768.0..=32_767.0).contains(&day) {
        day as i64
    } else {
        32_767
    };
    let month_index = year
        .checked_mul(12)
        .and_then(|v| v.checked_add(month - 1))
        .ok_or(W::Num)?;
    let normalized_year = month_index.div_euclid(12);
    let normalized_month = month_index.rem_euclid(12) + 1;
    if !(1900..=9999).contains(&normalized_year) {
        return Err(W::Num);
    }
    let serial = serial_unbounded_1900(normalized_year, normalized_month, day);
    if !(0..=MAX_SERIAL_1900).contains(&serial) {
        return Err(W::Num);
    }
    match system {
        DateSystem::System1900 => Ok(serial as f64),
        DateSystem::System1904 => {
            // A 1904 workbook has no 29 February 1900; a date before 1 January 1904 is #NUM!.
            let serial = serial as f64 - OFFSET_1904;
            if serial < 0.0 {
                Err(W::Num)
            } else {
                Ok(serial)
            }
        }
    }
}

pub(crate) fn time(hour: f64, minute: f64, second: f64) -> Result<f64, W> {
    if !hour.is_finite() || !minute.is_finite() || !second.is_finite() {
        return Err(W::Num);
    }
    let (hour, minute, second) = (hour.trunc(), minute.trunc(), second.trunc());
    if hour.abs() > 32767.0 || minute.abs() > 32767.0 || second.abs() > 32767.0 {
        return Err(W::Num);
    }
    let total_seconds = hour * 3600.0 + minute * 60.0 + second;
    if total_seconds < 0.0 {
        return Err(W::Num);
    }
    Ok((total_seconds / 86_400.0).rem_euclid(1.0))
}

// --- parts (date_parts_family.rs) --------------------------------------------------------------

/// The year, month and day of a 1900 serial, rounded to the nearest second first.
pub(crate) fn serial_to_ymd(serial: f64) -> Result<(i64, i64, i64), W> {
    if !serial.is_finite() || serial < 0.0 {
        return Err(W::Num);
    }
    let rounded = serial + 0.5 / 86_400.0;
    if rounded >= (MAX_SERIAL_1900 + 1) as f64 {
        return Err(W::Num);
    }
    Ok(ymd_1900(rounded.floor() as i64))
}

pub(crate) fn hms(serial: f64) -> Result<(f64, f64, f64), W> {
    let rounded = serial + 0.5 / 86_400.0;
    if !serial.is_finite() || serial < 0.0 || rounded >= (MAX_SERIAL_1900 + 1) as f64 {
        return Err(W::Num);
    }
    let fractional = rounded - rounded.floor();
    let hours = fractional * 24.0;
    let hour = hours.floor();
    let minutes = (hours - hour) * 60.0;
    let minute = minutes.floor();
    let second = ((minutes - minute) * 60.0).floor().rem_euclid(60.0);
    Ok((hour, minute, second))
}

pub(crate) fn days(end_serial: f64, start_serial: f64) -> Result<f64, W> {
    let range = 0.0..(MAX_SERIAL_1900 + 1) as f64;
    if !range.contains(&end_serial) || !range.contains(&start_serial) {
        return Err(W::Num);
    }
    Ok(end_serial.trunc() - start_serial.trunc())
}

// --- months and weeks (date_week_family.rs) -----------------------------------------------------

fn whole_serial(n: f64) -> Result<i64, W> {
    if !(0.0..(MAX_SERIAL_1900 + 1) as f64).contains(&n) {
        return Err(W::Num);
    }
    Ok(n.trunc() as i64)
}

fn whole(n: f64) -> Result<i64, W> {
    if !n.is_finite() {
        return Err(W::Num);
    }
    Ok(n.trunc() as i64)
}

fn shifted_month(year: i64, month: i64, offset: i64) -> Result<(i64, i64), W> {
    let month_index = year
        .checked_mul(12)
        .and_then(|v| v.checked_add(month - 1))
        .and_then(|v| v.checked_add(offset))
        .ok_or(W::Num)?;
    Ok((month_index.div_euclid(12), month_index.rem_euclid(12) + 1))
}

pub(crate) fn edate(serial: f64, months: f64) -> Result<f64, W> {
    let serial = whole_serial(serial)?;
    let months = whole(months)?;
    let (year, month, day) = ymd_1900(serial);
    let (year, month) = shifted_month(year, month, months)?;
    if !(1900..=9999).contains(&year) {
        return Err(W::Num);
    }
    let day = day.min(days_in_month(year, month));
    excel_serial_from_ymd(DateSystem::System1900, year, month, day).ok_or(W::Num)
}

pub(crate) fn eomonth(serial: f64, months: f64) -> Result<f64, W> {
    let serial = whole_serial(serial)?;
    let months = whole(months)?;
    let (year, month, _) = ymd_1900(serial);
    let (year, month) = shifted_month(year, month, months)?;
    if !(1900..=9999).contains(&year) {
        return Err(W::Num);
    }
    excel_serial_from_ymd(DateSystem::System1900, year, month, days_in_month(year, month))
        .ok_or(W::Num)
}

fn weekday_sunday_one_based(serial: i64) -> i64 {
    (serial - 1).rem_euclid(7) + 1
}

fn remap_weekday(sunday_one_based: i64, return_type: i64) -> Option<f64> {
    match return_type {
        1 => Some(sunday_one_based as f64),
        2 | 11 => Some(((sunday_one_based + 5) % 7 + 1) as f64),
        3 => Some(((sunday_one_based + 5) % 7) as f64),
        12..=17 => {
            let start_day = (return_type - 10).rem_euclid(7) + 1;
            Some(((sunday_one_based - start_day).rem_euclid(7) + 1) as f64)
        }
        _ => None,
    }
}

fn weekday_of(serial: i64, return_type: i64) -> Result<f64, W> {
    remap_weekday(weekday_sunday_one_based(serial), return_type).ok_or(W::Num)
}

pub(crate) fn weekday(serial: f64, return_type: Option<f64>) -> Result<f64, W> {
    if serial < 0.0 {
        return Err(W::Num);
    }
    let serial = whole_serial(serial + 0.5 / 86_400.0)?;
    let return_type = whole(date_integer_floor(return_type.unwrap_or(1.0)))?;
    weekday_of(serial, return_type)
}

fn iso_weeknum_serial(serial: i64) -> Result<f64, W> {
    let weekday_monday = weekday_of(serial, 2)? as i64;
    let thursday = serial + (4 - weekday_monday);
    let (iso_year, _, _) =
        ymd_from_excel_serial(DateSystem::System1900, thursday as f64).ok_or(W::Num)?;
    let jan4 = serial_unbounded_1900(iso_year, 1, 4);
    let jan4_weekday = weekday_of(jan4, 2)? as i64;
    let first_monday = jan4 - (jan4_weekday - 1);
    Ok(((serial - first_monday).div_euclid(7) + 1) as f64)
}

pub(crate) fn weeknum(serial: f64, return_type: Option<f64>) -> Result<f64, W> {
    let serial = whole_serial(serial)?;
    let return_type = whole(date_integer_floor(return_type.unwrap_or(1.0)))?;
    if return_type == 21 {
        return iso_weeknum_serial(serial);
    }
    if remap_weekday(1, return_type).is_none() || return_type == 3 {
        return Err(W::Num);
    }
    let (year, _, _) = ymd_1900(serial);
    let jan1 = serial_unbounded_1900(year, 1, 1);
    let jan1_weekday = weekday(jan1 as f64, Some(return_type as f64))? as i64;
    let week1_start = jan1 - (jan1_weekday - 1);
    Ok(((serial - week1_start).div_euclid(7) + 1) as f64)
}

pub(crate) fn isoweeknum(serial: f64) -> Result<f64, W> {
    iso_weeknum_serial(whole_serial(serial)?)
}

// --- DAYS360 and DATEDIF (date_value_family.rs) -------------------------------------------------

fn ymd_of(serial: f64) -> Result<(i64, i64, i64), W> {
    if !serial.is_finite() {
        return Err(W::Num);
    }
    let whole = serial.trunc() as i64;
    if whole < 0 {
        return Err(W::Num);
    }
    Ok(ymd_1900(whole))
}

fn is_last_day_of_february(year: i64, month: i64, day: i64) -> bool {
    let last = if year == 1900 {
        29
    } else {
        days_in_month(year, month)
    };
    month == 2 && day == last
}

pub(crate) fn days360(start: f64, end: f64, european: bool) -> Result<f64, W> {
    let (sy, sm, mut sd) = ymd_of(start)?;
    let (ey, em, mut ed) = ymd_of(end)?;
    if european {
        if sd == 31 {
            sd = 30;
        }
        if ed == 31 {
            ed = 30;
        }
    } else {
        if sd == 31 || is_last_day_of_february(sy, sm, sd) {
            sd = 30;
        }
        if ed == 31 {
            if sd < 30 {
                let (ny, nm) = if em == 12 { (ey + 1, 1) } else { (ey, em + 1) };
                return Ok((ny - sy) as f64 * 360.0 + (nm - sm) as f64 * 30.0 + (1 - sd) as f64);
            }
            ed = 30;
        }
        if is_last_day_of_february(ey, em, ed) && is_last_day_of_february(sy, sm, sd) {
            ed = 30;
        }
    }
    Ok((ey - sy) as f64 * 360.0 + (em - sm) as f64 * 30.0 + (ed - sd) as f64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DateDiffUnit {
    Y,
    M,
    D,
    Ym,
    Yd,
    Md,
}

pub(crate) fn datedif_unit(unit: &str) -> Result<DateDiffUnit, W> {
    match unit.trim().to_ascii_uppercase().as_str() {
        "Y" => Ok(DateDiffUnit::Y),
        "M" => Ok(DateDiffUnit::M),
        "D" => Ok(DateDiffUnit::D),
        "YM" => Ok(DateDiffUnit::Ym),
        "YD" => Ok(DateDiffUnit::Yd),
        "MD" => Ok(DateDiffUnit::Md),
        _ => Err(W::Num),
    }
}

fn last_day_of_previous_month(year: i64, month: i64) -> i64 {
    let (y, m) = if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    };
    days_in_month(y, m)
}

pub(crate) fn datedif(start_serial: f64, end_serial: f64, unit: DateDiffUnit) -> Result<f64, W> {
    let start = start_serial.trunc();
    let end = end_serial.trunc();
    if !start.is_finite() || !end.is_finite() || start < 0.0 || end < 0.0 || end < start {
        return Err(W::Num);
    }
    let (sy, sm, sd) = ymd_of(start)?;
    let (ey, em, ed) = ymd_of(end)?;
    let serial_of = |y: i64, m: i64, d: i64| {
        excel_serial_from_ymd(DateSystem::System1900, y, m, d).ok_or(W::Num)
    };
    match unit {
        DateDiffUnit::D => Ok(end - start),
        DateDiffUnit::Y => {
            let mut years = ey - sy;
            if (em, ed) < (sm, sd) {
                years -= 1;
            }
            Ok(years as f64)
        }
        DateDiffUnit::M => {
            let mut months = (ey - sy) * 12 + (em - sm);
            if ed < sd {
                months -= 1;
            }
            Ok(months as f64)
        }
        DateDiffUnit::Ym => {
            let mut months = (em - sm).rem_euclid(12);
            if ed < sd {
                months = (months - 1).rem_euclid(12);
            }
            Ok(months as f64)
        }
        DateDiffUnit::Yd => {
            let candidate = serial_of(ey, sm, sd.min(days_in_month(ey, sm)))?;
            if candidate <= end {
                Ok(end - candidate)
            } else {
                let previous = serial_of(ey - 1, sm, sd.min(days_in_month(ey - 1, sm)))?;
                Ok(end - previous)
            }
        }
        DateDiffUnit::Md => Ok(if ed >= sd {
            ed - sd
        } else {
            ed + last_day_of_previous_month(ey, em) - sd
        } as f64),
    }
}

// --- WORKDAY and NETWORKDAYS (workday_networkdays_family.rs) -----------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WeekendMask {
    monday_first: [bool; 7],
}

impl WeekendMask {
    pub(crate) const fn saturday_sunday() -> Self {
        Self {
            monday_first: [false, false, false, false, false, true, true],
        }
    }

    pub(crate) fn from_number(code: i64) -> Option<Self> {
        let monday_first = match code {
            1 => [false, false, false, false, false, true, true],
            2 => [true, false, false, false, false, false, true],
            3 => [true, true, false, false, false, false, false],
            4 => [false, true, true, false, false, false, false],
            5 => [false, false, true, true, false, false, false],
            6 => [false, false, false, true, true, false, false],
            7 => [false, false, false, false, true, true, false],
            11 => [false, false, false, false, false, false, true],
            12 => [true, false, false, false, false, false, false],
            13 => [false, true, false, false, false, false, false],
            14 => [false, false, true, false, false, false, false],
            15 => [false, false, false, true, false, false, false],
            16 => [false, false, false, false, true, false, false],
            17 => [false, false, false, false, false, true, false],
            _ => return None,
        };
        Some(Self { monday_first })
    }

    /// A mask of seven 0 and 1 characters, Monday first.
    pub(crate) fn from_mask_text(text: &str) -> Option<Self> {
        if text.len() != 7 || !text.chars().all(|ch| ch == '0' || ch == '1') {
            return None;
        }
        let mut monday_first = [false; 7];
        for (idx, ch) in text.chars().enumerate() {
            monday_first[idx] = ch == '1';
        }
        Some(Self { monday_first })
    }

    pub(crate) fn is_all_days(&self) -> bool {
        self.monday_first.iter().all(|bit| *bit)
    }

    fn contains_serial(&self, serial: i64) -> bool {
        let monday_idx = ((weekday_sunday_one_based(serial) + 5).rem_euclid(7)) as usize;
        self.monday_first[monday_idx]
    }
}

pub(crate) fn serial_from_number(value: f64) -> Result<i64, W> {
    if !value.is_finite() {
        return Err(W::Num);
    }
    let serial = value.trunc() as i64;
    if !(0..=MAX_SERIAL_1900).contains(&serial) {
        return Err(W::Num);
    }
    Ok(serial)
}

fn is_business_day(serial: i64, weekend: WeekendMask, holidays: &BTreeSet<i64>) -> bool {
    !weekend.contains_serial(serial) && !holidays.contains(&serial)
}

pub(crate) fn workday(
    start_serial: f64,
    days: f64,
    weekend: WeekendMask,
    holidays: &BTreeSet<i64>,
) -> Result<f64, W> {
    if weekend.is_all_days() {
        return Err(W::Value);
    }
    let start = serial_from_number(start_serial)?;
    if !days.is_finite() {
        return Err(W::Num);
    }
    let mut remaining = days.floor() as i64;
    if remaining == 0 {
        return Ok(start as f64);
    }
    let step = if remaining > 0 { 1 } else { -1 };
    let mut current = start;
    while remaining != 0 {
        current += step;
        if !(0..=MAX_SERIAL_1900).contains(&current) {
            return Err(W::Num);
        }
        if is_business_day(current, weekend, holidays) {
            remaining -= step;
        }
    }
    Ok(current as f64)
}

pub(crate) fn networkdays(
    start_serial: f64,
    end_serial: f64,
    weekend: WeekendMask,
    holidays: &BTreeSet<i64>,
) -> Result<f64, W> {
    let start = serial_from_number(start_serial)?;
    let end = serial_from_number(end_serial)?;
    if weekend.is_all_days() {
        return Ok(0.0);
    }
    let (lo, hi, sign) = if start <= end {
        (start, end, 1.0)
    } else {
        (end, start, -1.0)
    };
    // Whole weeks hold the same business days, so only the remainder is walked.
    let per_week = (0..7)
        .filter(|&offset| !weekend.contains_serial(lo + offset))
        .count() as i64;
    let span = hi - lo + 1;
    let weeks = span / 7;
    let mut count = weeks * per_week;
    for serial in lo + weeks * 7..=hi {
        if !weekend.contains_serial(serial) {
            count += 1;
        }
    }
    count -= holidays
        .range(lo..=hi)
        .filter(|&&serial| !weekend.contains_serial(serial))
        .count() as i64;
    Ok(sign * count as f64)
}

// --- YEARFRAC (discount_bill_yearfrac_family.rs, day_count_common.rs) --------------------------

fn real_days_in_year(year: i64) -> f64 {
    if is_leap(year) {
        366.0
    } else {
        365.0
    }
}

/// NASD US (30/360) day count between two 1900 serials.
fn us_30_360(start: i64, end: i64) -> f64 {
    let (sy, sm, mut sd) = ymd_1900(start);
    let (ey, em, mut ed) = ymd_1900(end);
    let feb_end = |y: i64, m: i64| if y == 1900 { 29 } else { days_in_month(y, m) };
    let start_last_feb = sm == 2 && sd == feb_end(sy, sm);
    let end_last_feb = em == 2 && ed == feb_end(ey, em);
    if sd == 31 || start_last_feb {
        sd = 30;
    }
    if ed == 31 {
        if sd < 30 {
            let (ny, nm) = if em == 12 { (ey + 1, 1) } else { (ey, em + 1) };
            return (ny - sy) as f64 * 360.0 + (nm - sm) as f64 * 30.0 + (1 - sd) as f64;
        }
        ed = 30;
    }
    if end_last_feb && start_last_feb {
        ed = 30;
    }
    ((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64
}

fn eu_30_360(start: i64, end: i64) -> f64 {
    let (sy, sm, mut sd) = ymd_1900(start);
    let (ey, em, mut ed) = ymd_1900(end);
    if sd == 31 {
        sd = 30;
    }
    if ed == 31 {
        ed = 30;
    }
    ((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64
}

/// Excel's actual/actual year fraction (YEARFRAC basis 1) for `start <= end`.
fn actual_actual(start: i64, end: i64) -> Result<f64, W> {
    let serial_of = |y: i64, m: i64, d: i64| {
        excel_serial_from_ymd(DateSystem::System1900, y, m, d)
            .map(|v| v as i64)
            .ok_or(W::Value)
    };
    let (sy, sm, sd) = ymd_1900(start);
    let (ey, em, ed) = ymd_1900(end);
    let days = (end - start) as f64;
    let within_a_year = sy == ey || (ey == sy + 1 && (sm > em || (sm == em && sd >= ed)));
    if within_a_year {
        let feb29_in_span = (is_leap(sy) && start < serial_of(sy, 3, 1)?)
            || (is_leap(ey) && end >= serial_of(ey, 3, 1)?)
            || (em == 2 && ed == 29);
        let year_length = if (sy == ey && is_leap(sy)) || feb29_in_span {
            366.0
        } else {
            365.0
        };
        return Ok(days / year_length);
    }
    let years = (ey - sy + 1) as f64;
    let days_in_years: f64 = (sy..=ey).map(real_days_in_year).sum();
    Ok(days / (days_in_years / years))
}

pub(crate) fn yearfrac(start_date: f64, end_date: f64, basis: f64) -> Result<f64, W> {
    if start_date.trunc() < 0.0 || end_date.trunc() < 0.0 {
        return Err(W::Num);
    }
    let serial = |value: f64| -> Result<i64, W> {
        if !value.is_finite() {
            return Err(W::Value);
        }
        let serial = value.trunc() as i64;
        if !(1..=MAX_SERIAL_1900).contains(&serial) {
            return Err(W::Value);
        }
        Ok(serial)
    };
    let (mut start, mut end) = (serial(start_date)?, serial(end_date)?);
    if !basis.is_finite() {
        return Err(W::Num);
    }
    if end < start {
        std::mem::swap(&mut start, &mut end);
    }
    match basis.trunc() as i64 {
        0 => Ok(us_30_360(start, end) / 360.0),
        1 => actual_actual(start, end),
        2 => Ok((end - start) as f64 / 360.0),
        3 => Ok((end - start) as f64 / 365.0),
        4 => Ok(eu_30_360(start, end) / 360.0),
        _ => Err(W::Num),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn date_keeps_the_1900_leap_day_and_normalizes_months() {
        assert_eq!(date(1900.0, 1.0, 1.0, DateSystem::System1900), Ok(1.0));
        assert_eq!(date(1900.0, 2.0, 29.0, DateSystem::System1900), Ok(60.0));
        assert_eq!(date(2024.0, 13.0, 1.0, DateSystem::System1900), Ok(45658.0));
        assert_eq!(date(2024.0, 1.0, 15.0, DateSystem::System1904), Ok(45306.0 - 1462.0));
        assert_eq!(date(1903.0, 12.0, 31.0, DateSystem::System1904), Err(W::Num));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn networkdays_matches_a_day_by_day_walk() {
        let holidays: BTreeSet<i64> = [45005, 45006, 45100].into_iter().collect();
        for weekend in [1, 2, 7, 11, 17] {
            let mask = WeekendMask::from_number(weekend).unwrap();
            for (s, e) in [(45000, 45030), (45030, 45000), (45001, 45001), (45000, 45200)] {
                let (lo, hi) = (s.min(e), s.max(e));
                let walked = (lo..=hi)
                    .filter(|&d| is_business_day(d, mask, &holidays))
                    .count() as f64;
                let sign = if s <= e { 1.0 } else { -1.0 };
                assert_eq!(
                    networkdays(s as f64, e as f64, mask, &holidays),
                    Ok(sign * walked),
                    "{weekend} {s} {e}"
                );
            }
        }
    }
}
