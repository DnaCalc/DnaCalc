//! The date and time family fragment ported from OxFunc (functions/ at 6fca3518): date_fn.rs,
//! date_parts_family.rs, date_value_family.rs, date_week_family.rs, day_count_common.rs,
//! workday_networkdays_family.rs, discount_bill_yearfrac_family.rs's YEARFRAC, now_fn.rs and
//! today_fn.rs. Serial dates count in the date system of the call's environment
//! ([`crate::locale::CalcEnvironment`]); a call without one is refused as not modelled.

use crate::kernel::{Site, Step};
use crate::value::CalcValue;

const CODE: u16 = 203;

fn refused(site: &Site<'_>) -> Step<CalcValue> {
    Err(site.not_modelled(None))
}

ported_family! {
    code: CODE;
    1 date_fn "DATE" (3, 3) [V "year", V "month", V "day"] |s, a, sv| { refused(s) }
    2 time_fn "TIME" (3, 3) [V "hour", V "minute", V "second"] |s, a, sv| { refused(s) }
    3 year_fn "YEAR" (1, 1) [V "serial_number"] |s, a, sv| { refused(s) }
    4 month_fn "MONTH" (1, 1) [V "serial_number"] |s, a, sv| { refused(s) }
    5 day_fn "DAY" (1, 1) [V "serial_number"] |s, a, sv| { refused(s) }
    6 hour_fn "HOUR" (1, 1) [V "serial_number"] |s, a, sv| { refused(s) }
    7 minute_fn "MINUTE" (1, 1) [V "serial_number"] |s, a, sv| { refused(s) }
    8 second_fn "SECOND" (1, 1) [V "serial_number"] |s, a, sv| { refused(s) }
    9 days_fn "DAYS" (2, 2) [V "end_date", V "start_date"] |s, a, sv| { refused(s) }
    10 days360_fn "DAYS360" (2, 3) [V "start_date", V "end_date", V "method"] |s, a, sv| { refused(s) }
    11 datedif_fn "DATEDIF" (3, 3) [V "start_date", V "end_date", V "unit"] |s, a, sv| { refused(s) }
    12 datevalue_fn "DATEVALUE" (1, 1) [V "date_text"] |s, a, sv| { refused(s) }
    13 timevalue_fn "TIMEVALUE" (1, 1) [V "time_text"] |s, a, sv| { refused(s) }
    14 edate_fn "EDATE" (2, 2) [V "start_date", V "months"] |s, a, sv| { refused(s) }
    15 eomonth_fn "EOMONTH" (2, 2) [V "start_date", V "months"] |s, a, sv| { refused(s) }
    16 weekday_fn "WEEKDAY" (1, 2) [V "serial_number", V "return_type"] |s, a, sv| { refused(s) }
    17 weeknum_fn "WEEKNUM" (1, 2) [V "serial_number", V "return_type"] |s, a, sv| { refused(s) }
    18 isoweeknum_fn "ISOWEEKNUM" (1, 1) [V "date"] |s, a, sv| { refused(s) }
    19 workday_fn "WORKDAY" (2, 3) [V "start_date", V "days", R "holidays"] |s, a, sv| { refused(s) }
    20 workday_intl_fn "WORKDAY.INTL" (2, 4) [V "start_date", V "days", V "weekend", R "holidays"] |s, a, sv| { refused(s) }
    21 networkdays_fn "NETWORKDAYS" (2, 3) [V "start_date", V "end_date", R "holidays"] |s, a, sv| { refused(s) }
    22 networkdays_intl_fn "NETWORKDAYS.INTL" (2, 4) [V "start_date", V "end_date", V "weekend", R "holidays"] |s, a, sv| { refused(s) }
    23 yearfrac_fn "YEARFRAC" (2, 3) [V "start_date", V "end_date", V "basis"] |s, a, sv| { refused(s) }
    24 now_fn "NOW" (0, 0) [] |s, a, sv| { refused(s) }
    25 today_fn "TODAY" (0, 0) [] |s, a, sv| { refused(s) }
}
