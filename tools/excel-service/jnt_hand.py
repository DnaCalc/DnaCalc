"""Hand-written legacy cases (dc-jnt): Excel facts that OxFunc's unit tests assert with literals.

Each case is (function, arguments, legacy expectation, source test). An argument is ("n", number),
("s", text) written into the formula, ("b", logical), ("e", error literal) or ("-",) for an empty cell.
The legacy expectation is a hypothesis, never the answer: jnt_import.py `hand` asks Excel through the
question service (tools/excel-service/client.py) and keeps Excel's answer; a difference is listed in
contradicted-<sheet>.json. An expectation is ("number", x), ("approx", x) (the legacy test used a 1e-12
tolerance), ("error", "num") or None (the legacy test named no value).
Ported from OxFunc@05196c39:crates/oxfunc_core/src/functions/{date_fn,date_parts_family,date_value_family,
date_week_family,workday_networkdays_family}.rs unit tests.
"""
import datetime


def serial(y, m, d):
    return float((datetime.date(y, m, d) - datetime.date(1899, 12, 30)).days)


def n(v):
    return ("n", float(v))


def s(t):
    return ("s", t)


def num(v):
    return ("number", float(v))


def err(code):
    return ("error", code)


EMPTY = ("-",)
T = ("b", True)
F = ("b", False)

DATE_FN = "date_fn.rs"
PARTS = "date_parts_family.rs"
VALUE = "date_value_family.rs"
WEEK = "date_week_family.rs"
WORK = "workday_networkdays_family.rs"

J10 = [
    # date_fn.rs
    ("DATE", [n(1900), n(1), n(1)], num(1), DATE_FN + " eval_date_handles_1900_serial_baselines"),
    ("DATE", [n(1900), n(2), n(29)], num(60), DATE_FN + " eval_date_preserves_excel_1900_leap_bug_seed"),
    ("DATE", [n(2024), n(14), n(1)], num(45689), DATE_FN + " eval_date_normalizes_month_overflow"),
    ("DATE", [n(1900), n(1), n(0)], num(0), DATE_FN + " eval_date_allows_serial_zero_boundary"),
    ("DATE", [n(1900), n(0), n(1)], err("num"), DATE_FN + " eval_date_rejects_month_zero_boundary"),
    ("DATE", [n(1900), n(3), n(0)], num(60), DATE_FN + " eval_date_normalizes_march_zero_to_excel_1900_leap_bug_day"),
    ("DATE", [n(1900), n(1), n(60)], num(60), DATE_FN + " eval_date_normalizes_january_sixtieth_to_excel_1900_leap_bug_day"),
    ("DATE", [n(0), n(1), n(1)], num(1), DATE_FN + " eval_date_normalizes_short_year_and_truncates_day"),
    ("DATE", [n(2008), n(1), n(2.9)], num(39449), DATE_FN + " eval_date_normalizes_short_year_and_truncates_day"),
    # date_parts_family.rs
    ("DAY", [n(1)], num(1), PARTS + " serial_parts_match_excel_probe_rows"),
    ("MONTH", [n(1)], num(1), PARTS + " serial_parts_match_excel_probe_rows"),
    ("YEAR", [n(1)], num(1900), PARTS + " serial_parts_match_excel_probe_rows"),
    ("DAY", [n(60)], num(29), PARTS + " serial_parts_match_excel_probe_rows"),
    ("MONTH", [n(60)], num(2), PARTS + " serial_parts_match_excel_probe_rows"),
    ("YEAR", [n(60)], num(1900), PARTS + " serial_parts_match_excel_probe_rows"),
    ("DAY", [n(61.9)], num(1), PARTS + " serial_parts_match_excel_probe_rows"),
    ("MONTH", [n(61.9)], num(3), PARTS + " serial_parts_match_excel_probe_rows"),
    ("YEAR", [n(61.9)], num(1900), PARTS + " serial_parts_match_excel_probe_rows"),
    ("DAY", [n(-1)], err("num"), PARTS + " serial_parts_match_excel_probe_rows"),
    ("DAYS", [n(61), n(60)], num(1), PARTS + " days_kernel_matches_excel_probe_rows"),
    ("DAYS", [n(1), n(2)], num(-1), PARTS + " days_kernel_matches_excel_probe_rows"),
    ("DAYS", [n(61.9), n(60.1)], num(1), PARTS + " days_kernel_matches_excel_probe_rows"),
    ("DAYS", [n(1), n(-1)], err("num"), PARTS + " days_kernel_matches_excel_probe_rows"),
    ("DAYS", [EMPTY, n(1)], num(-1), PARTS + " blank_and_missing_inputs_coerce_to_zero_like_excel"),
    ("DAYS", [T, n(1)], num(0), PARTS + " logical_inputs_follow_numeric_coercion"),
    ("HOUR", [n(0)], num(0), PARTS + " time_parts_match_excel_probe_rows"),
    ("MINUTE", [n(0)], num(0), PARTS + " time_parts_match_excel_probe_rows"),
    ("SECOND", [n(0)], num(0), PARTS + " time_parts_match_excel_probe_rows"),
    ("HOUR", [n(1.9)], num(21), PARTS + " time_parts_match_excel_probe_rows"),
    ("MINUTE", [n(1.9)], num(36), PARTS + " time_parts_match_excel_probe_rows"),
    ("SECOND", [n(1.9)], num(0), PARTS + " time_parts_match_excel_probe_rows"),
    ("HOUR", [n(-0.1)], err("num"), PARTS + " time_parts_match_excel_probe_rows"),
    ("TIME", [n(1), n(2), n(3)], num(0.043090277777777776), PARTS + " time_kernel_matches_excel_probe_rows"),
    ("TIME", [n(27), n(0), n(0)], num(0.125), PARTS + " time_kernel_matches_excel_probe_rows"),
    ("TIME", [n(0), n(120), n(0)], num(0.08333333333333333), PARTS + " time_kernel_matches_excel_probe_rows"),
    ("TIME", [n(0), n(0), n(120)], num(0.001388888888888889), PARTS + " time_kernel_matches_excel_probe_rows"),
    ("TIME", [n(-1), n(60), n(0)], num(0), PARTS + " time_kernel_matches_excel_probe_rows"),
    ("TIME", [n(-1), n(0), n(1)], err("num"), PARTS + " time_kernel_matches_excel_probe_rows"),
    ("TIME", [n(32768), n(0), n(0)], err("num"), PARTS + " time_kernel_matches_excel_probe_rows"),
    ("TIME", [EMPTY, n(2), n(3)], num(0.0014236111111111112), PARTS + " time_surface_blanks_missing_text_and_logicals_match_excel"),
    ("TIME", [T, n(2), n(3)], num(0.043090277777777776), PARTS + " time_surface_blanks_missing_text_and_logicals_match_excel"),
    ("TIME", [s("1"), s("2"), s("3")], num(0.043090277777777776), PARTS + " time_surface_blanks_missing_text_and_logicals_match_excel"),
    # date_value_family.rs
    ("DATEVALUE", [s("2024-02-03")], num(45325), VALUE + " datevalue_matches_seed_rows"),
    ("DATEVALUE", [s("2024-02-03 6:35 AM")], num(45325), VALUE + " datevalue_matches_seed_rows"),
    ("DATEVALUE", [s("6:35 AM")], num(0), VALUE + " datevalue_matches_seed_rows"),
    ("DATEVALUE", [s("22-Aug-2008")], num(39682), VALUE + " datevalue_matches_seed_rows"),
    ("DATEVALUE", [s("1/2/2024")], err("value"), VALUE + " datevalue_matches_seed_rows"),
    ("TIMEVALUE", [s("2:24 AM")], ("approx", 0.1), VALUE + " timevalue_matches_seed_rows"),
    ("TIMEVALUE", [s("22-Aug-2008 6:35 AM")], ("approx", 0.2743055555555556), VALUE + " timevalue_matches_seed_rows"),
    ("TIMEVALUE", [s("2024-02-03")], num(0), VALUE + " timevalue_matches_seed_rows"),
    ("TIMEVALUE", [s("22-Aug-2008")], num(0), VALUE + " timevalue_matches_seed_rows"),
    ("TIMEVALUE", [s("1/2/2024 6:35 AM")], err("value"), VALUE + " timevalue_matches_seed_rows"),
    ("DAYS360", [n(serial(2011, 1, 30)), n(serial(2011, 2, 1)), F], num(1), VALUE + " days360_matches_seed_rows"),
    ("DAYS360", [n(serial(2011, 1, 1)), n(serial(2011, 12, 31)), F], num(360), VALUE + " days360_matches_seed_rows"),
    ("DAYS360", [n(serial(2011, 1, 31)), n(serial(2011, 2, 28)), F], num(28), VALUE + " days360_matches_seed_rows"),
    ("DAYS360", [n(serial(2011, 1, 31)), n(serial(2011, 2, 28)), T], num(28), VALUE + " days360_matches_seed_rows"),
    ("DAYS360", [n(serial(2024, 2, 29)), n(serial(2024, 3, 31)), F], num(30), VALUE + " days360_matches_seed_rows"),
    ("DAYS360", [n(serial(2024, 2, 29)), n(serial(2024, 3, 31)), T], num(31), VALUE + " days360_matches_seed_rows"),
    ("DAYS360", [n(serial(2011, 2, 28)), n(serial(2011, 3, 31)), F], num(30), VALUE + " days360_matches_seed_rows"),
    ("DAYS360", [n(serial(2011, 2, 28)), n(serial(2011, 3, 31)), T], num(32), VALUE + " days360_matches_seed_rows"),
    ("DATEDIF", [n(serial(2001, 1, 1)), n(serial(2003, 1, 1)), s("Y")], num(2), VALUE + " datedif_matches_seed_rows"),
    ("DATEDIF", [n(serial(2001, 6, 1)), n(serial(2002, 8, 15)), s("D")], num(440), VALUE + " datedif_matches_seed_rows"),
    ("DATEDIF", [n(serial(2001, 6, 1)), n(serial(2002, 8, 15)), s("YD")], num(75), VALUE + " datedif_matches_seed_rows"),
    ("DATEDIF", [n(serial(2001, 6, 1)), n(serial(2002, 8, 15)), s("M")], num(14), VALUE + " datedif_matches_seed_rows"),
    ("DATEDIF", [n(serial(2001, 6, 1)), n(serial(2002, 8, 15)), s("YM")], num(2), VALUE + " datedif_matches_seed_rows"),
    ("DATEDIF", [n(serial(2001, 1, 31)), n(serial(2001, 2, 28)), s("MD")], num(28), VALUE + " datedif_matches_seed_rows"),
    ("DATEDIF", [n(serial(2001, 1, 31)), n(serial(2001, 3, 1)), s("MD")], num(-2), VALUE + " datedif_matches_seed_rows"),
    ("DATEDIF", [n(serial(2001, 1, 30)), n(serial(2001, 3, 1)), s("MD")], num(-1), VALUE + " datedif_matches_seed_rows"),
    ("DATEDIF", [n(serial(2000, 2, 29)), n(serial(2001, 2, 28)), s("Y")], num(0), VALUE + " datedif_matches_seed_rows"),
    ("DATEDIF", [n(serial(2000, 2, 29)), n(serial(2001, 2, 28)), s("YM")], num(11), VALUE + " datedif_matches_seed_rows"),
    ("DATEDIF", [n(serial(2000, 2, 29)), n(serial(2001, 2, 28)), s("MD")], num(30), VALUE + " datedif_matches_seed_rows"),
    ("DATEDIF", [n(serial(2001, 2, 28)), n(serial(2001, 1, 31)), s("D")], err("num"), VALUE + " datedif_rejects_invalid_order_and_unit"),
    ("DATEDIF", [n(serial(2001, 1, 1)), n(serial(2001, 2, 1)), s("Q")], err("num"), VALUE + " datedif_rejects_invalid_order_and_unit"),
    ("DATEDIF", [s("2020-01-15"), s("2024-03-20"), s("Y")], num(4), VALUE + " eval_datedif_surface_accepts_iso_date_text_args"),
    ("DATEDIF", [n(serial(2020, 1, 15)), n(serial(2024, 3, 20)), s("Y")], num(4), VALUE + " eval_datedif_surface_keeps_direct_serial_control"),
    ("DATEVALUE", [("e", "#N/A")], err("na"), VALUE + " error_args_propagate_unchanged_through_value_family"),
    ("TIMEVALUE", [("e", "#N/A")], err("na"), VALUE + " error_args_propagate_unchanged_through_value_family"),
    ("DAYS360", [("e", "#N/A"), n(44197)], err("na"), VALUE + " error_args_propagate_unchanged_through_value_family"),
    ("DATEDIF", [("e", "#N/A"), n(44197), s("D")], err("na"), VALUE + " error_args_propagate_unchanged_through_value_family"),
    ("DATEVALUE", [("e", "#DIV/0!")], err("div0"), VALUE + " error_args_propagate_unchanged_through_value_family"),
    # date_week_family.rs
    ("EDATE", [n(45322), n(1)], num(45351), WEEK + " edate_kernel_matches_seed_rows"),
    ("EDATE", [n(45322), n(-1)], num(45291), WEEK + " edate_kernel_matches_seed_rows"),
    ("EDATE", [n(0), n(0)], num(0), WEEK + " edate_kernel_matches_seed_rows"),
    ("EDATE", [n(60), n(0)], num(59), WEEK + " edate_kernel_matches_seed_rows"),
    ("EDATE", [n(60), n(1)], num(89), WEEK + " edate_kernel_matches_seed_rows"),
    ("EDATE", [n(-1), n(1)], err("num"), WEEK + " edate_kernel_matches_seed_rows"),
    ("EOMONTH", [n(45306), n(0)], num(45322), WEEK + " eomonth_kernel_matches_seed_rows"),
    ("EOMONTH", [n(45322), n(1)], num(45351), WEEK + " eomonth_kernel_matches_seed_rows"),
    ("EOMONTH", [n(0), n(0)], num(31), WEEK + " eomonth_kernel_matches_seed_rows"),
    ("EOMONTH", [n(60), n(0)], num(59), WEEK + " eomonth_kernel_matches_seed_rows"),
    ("EOMONTH", [n(-1), n(1)], err("num"), WEEK + " eomonth_kernel_matches_seed_rows"),
    ("WEEKDAY", [n(1)], num(1), WEEK + " weekday_kernel_matches_seed_rows"),
    ("WEEKDAY", [n(0)], num(7), WEEK + " weekday_kernel_matches_seed_rows"),
    ("WEEKDAY", [n(60)], num(4), WEEK + " weekday_kernel_matches_seed_rows"),
    ("WEEKDAY", [n(45292), n(2)], num(1), WEEK + " weekday_kernel_matches_seed_rows"),
    ("WEEKDAY", [n(45292), n(3)], num(0), WEEK + " weekday_kernel_matches_seed_rows"),
    ("WEEKDAY", [n(45292), n(12)], num(7), WEEK + " weekday_kernel_matches_seed_rows"),
    ("WEEKDAY", [n(45292), n(17)], num(2), WEEK + " weekday_kernel_matches_seed_rows"),
    ("WEEKDAY", [n(45292), n(0)], err("num"), WEEK + " weekday_kernel_matches_seed_rows"),
    ("WEEKNUM", [n(45292)], num(1), WEEK + " weeknum_kernel_matches_seed_rows"),
    ("WEEKNUM", [n(45292), n(2)], num(1), WEEK + " weeknum_kernel_matches_seed_rows"),
    ("WEEKNUM", [n(45292), n(17)], num(1), WEEK + " weeknum_kernel_matches_seed_rows"),
    ("WEEKNUM", [n(45292), n(21)], num(1), WEEK + " weeknum_kernel_matches_seed_rows"),
    ("WEEKNUM", [n(0)], num(0), WEEK + " weeknum_kernel_matches_seed_rows"),
    ("WEEKNUM", [n(0), n(2)], num(1), WEEK + " weeknum_kernel_matches_seed_rows"),
    ("WEEKNUM", [n(60)], num(9), WEEK + " weeknum_kernel_matches_seed_rows"),
    ("WEEKNUM", [n(45292), n(3)], err("num"), WEEK + " weeknum_kernel_matches_seed_rows"),
    ("ISOWEEKNUM", [n(45292)], num(1), WEEK + " isoweeknum_kernel_matches_seed_rows"),
    ("ISOWEEKNUM", [n(44196)], num(53), WEEK + " isoweeknum_kernel_matches_seed_rows"),
    ("ISOWEEKNUM", [n(60)], num(9), WEEK + " isoweeknum_kernel_matches_seed_rows"),
    ("ISOWEEKNUM", [n(0)], num(52), WEEK + " isoweeknum_kernel_matches_seed_rows"),
    ("ISOWEEKNUM", [n(-1)], err("num"), WEEK + " isoweeknum_kernel_matches_seed_rows"),
    # workday_networkdays_family.rs (one holiday, as a cell)
    ("WORKDAY", [n(serial(2024, 1, 2)), n(5), n(serial(2024, 1, 1))], num(serial(2024, 1, 9)),
     WORK + " workday_and_networkdays_default_weekend_match_baseline_examples"),
    ("NETWORKDAYS", [n(serial(2024, 1, 1)), n(serial(2024, 1, 10)), n(serial(2024, 1, 1))], num(7),
     WORK + " workday_and_networkdays_default_weekend_match_baseline_examples"),
    ("NETWORKDAYS", [n(serial(2024, 1, 10)), n(serial(2024, 1, 1)), n(serial(2024, 1, 1))], num(-7),
     WORK + " workday_and_networkdays_default_weekend_match_baseline_examples"),
    ("WORKDAY.INTL", [n(serial(2012, 1, 1)), n(30), n(17)], num(40944), WORK + " intl_weekend_numbers_and_masks_are_honored"),
    ("WORKDAY.INTL", [n(serial(2012, 1, 1)), n(90), n(11)], num(41013), WORK + " intl_weekend_numbers_and_masks_are_honored"),
    ("NETWORKDAYS.INTL", [n(serial(2006, 1, 1)), n(serial(2006, 2, 1)), n(7), n(serial(2006, 1, 2))], None,
     WORK + " intl_weekend_numbers_and_masks_are_honored (one of its two holidays)"),
    ("NETWORKDAYS.INTL", [n(serial(2006, 1, 1)), n(serial(2006, 2, 1)), s("0010001"), n(serial(2006, 1, 2))], None,
     WORK + " intl_weekend_numbers_and_masks_are_honored (one of its two holidays)"),
]

CASES = {"J10": J10}
