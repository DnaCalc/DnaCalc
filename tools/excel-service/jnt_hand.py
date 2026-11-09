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


def lit(v):
    return ("lit", float(v))


def txt(v):
    return ("text", str(v))


def f64_bits(b):
    import struct
    return struct.unpack(">d", struct.pack(">Q", b))[0]


EMPTY = ("-",)
T = ("b", True)
F = ("b", False)

DATE_FN = "date_fn.rs"
PARTS = "date_parts_family.rs"
VALUE = "date_value_family.rs"
WEEK = "date_week_family.rs"
WORK = "workday_networkdays_family.rs"


def cell_s(v):
    return ("cell_s", str(v))


def lit_b(v):
    return ("lit_b", bool(v))


def b(v):
    return ("b", bool(v))


def e(code):
    return ("e", str(code))



def arr_num(rows):
    return ("arr_num", rows)


def omitted():
    return ("omitted",)

T_ = ("logical", True)
F_ = ("logical", False)

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

OPA = "op_add.rs"
OAF = "operator_arithmetic_family.rs"
OCC = "operator_compare_concat_family.rs"


def f(text, legacy, source):
    return ("=" + text, [], legacy, source)



def arr_num(rows):
    return ("arr_num", rows)


def omitted():
    return ("omitted",)

T_ = ("logical", True)
F_ = ("logical", False)

J11 = [
    f("2+3", num(5), OPA + " eval_op_add_two_numbers"),
    f("1E-308+1E-308", num(0), OPA + " eval_op_add_flushes_excel_denormalized_results"),
    f('"2"+TRUE', num(3), OPA + " eval_op_add_numeric_text_and_logical"),
    f('"abc"+1', err("value"), OPA + " eval_op_add_non_numeric_text_fails"),
    f('-"2"', num(-2), OAF + " negate_follows_numeric_coercion"),
    f("-TRUE", num(-1), OAF + " negate_follows_numeric_coercion"),
    f('+"2"', None, OAF + " unary_plus_is_type_preserving_identity"),
    f("+TRUE", T_, OAF + " unary_plus_is_type_preserving_identity"),
    f("+2", num(2), OAF + " unary_plus_is_type_preserving_identity"),
    f("+#N/A", err("na"), OAF + " unary_plus_is_type_preserving_identity"),
    f("+Z1", num(0), OAF + " unary_plus_is_type_preserving_identity (empty cell)"),
    f("5%", None, OAF + " percent_lifts_arrays_elementwise"),
    f("5-2", num(3), OAF + " subtract_multiply_divide_and_power_cover_seed_numeric_lanes"),
    f("5*2", num(10), OAF + " subtract_multiply_divide_and_power_cover_seed_numeric_lanes"),
    f("5/2", num(2.5), OAF + " subtract_multiply_divide_and_power_cover_seed_numeric_lanes"),
    f("2^3", num(8), OAF + " subtract_multiply_divide_and_power_cover_seed_numeric_lanes"),
    f("1/0", err("div0"), OAF + " divide_by_zero_maps_domain_error"),
    f("+1E-309", num(0), OAF + " arithmetic_kernels_flush_excel_denormalized_results_to_zero"),
    f("-1E-309", num(0), OAF + " arithmetic_kernels_flush_excel_denormalized_results_to_zero"),
    f("1E-307%", num(0), OAF + " arithmetic_kernels_flush_excel_denormalized_results_to_zero"),
    f("2E-308-1E-308", num(0), OAF + " arithmetic_kernels_flush_excel_denormalized_results_to_zero"),
    f("1E-307*0.01", num(0), OAF + " arithmetic_kernels_flush_excel_denormalized_results_to_zero"),
    f("2.3E-308/2", num(0), OAF + " arithmetic_kernels_flush_excel_denormalized_results_to_zero"),
    f("5E-308/2", num(2.5e-308), OAF + " arithmetic_kernels_flush_excel_denormalized_results_to_zero"),
    f('"a"&1', None, OCC + " concat_matches_seed_scalar_lanes"),
    f("1&TRUE", None, OCC + " concat_matches_seed_scalar_lanes"),
    f('1="1"', F_, OCC + " comparisons_match_case_mixed_and_type_ordering_lanes"),
    f('"10">2', T_, OCC + " comparisons_match_case_mixed_and_type_ordering_lanes"),
    f('"a"="A"', None, OCC + " comparisons_match_case_mixed_and_type_ordering_lanes"),
    f('"a">1', None, OCC + " comparisons_match_case_mixed_and_type_ordering_lanes"),
    f("TRUE>1", None, OCC + " comparisons_match_case_mixed_and_type_ordering_lanes"),
    f("Z1=0", T_, OCC + " comparisons_handle_blank_against_number_text_and_logical"),
    f('Z1=""', T_, OCC + " comparisons_handle_blank_against_number_text_and_logical"),
    f("Z1=FALSE", T_, OCC + " comparisons_handle_blank_against_number_text_and_logical"),
    f("Z1<1", T_, OCC + " comparisons_handle_blank_against_number_text_and_logical"),
    f("1=#N/A", err("na"), OCC + " comparison_error_operand_propagates"),
    f("0.1+0.2=0.3", T_, OCC + " comparisons_use_excel_near_equal_numeric_ordering"),
    f("1+1E-14=1", None, OCC + " comparisons_use_excel_near_equal_numeric_ordering"),
    f("(0.1+0.2)>0.3", None, OCC + " comparisons_use_excel_near_equal_numeric_ordering"),
    f("(0.1+0.2)<0.3", None, OCC + " comparisons_use_excel_near_equal_numeric_ordering"),
]

PARITY_DB_DDB = 'db_ddb_excel_parity_20260929.rs'
PARITY_VDB = 'vdb_switched_excel_parity_20260929.rs'
PARITY_SYD = 'syd_excel_parity_20260929.rs'
PARITY_SLN = 'sln_excel_parity_20260929.rs'
PARITY_BITWISE = 'bitwise_excel_parity_20260929.rs'
PARITY_AGG = 'aggregate_prepared_excel_parity_20260929.rs'
COMPLEX_FN = 'complex_family.rs'

J4 = [
    ("VDB", [n(f64_bits(0x37ed6424f6a0cf14)), n(f64_bits(0x3815bce117a42154)), n(f64_bits(0x3fef45ede58066c6)), n(f64_bits(0x3fd56f481e27ec1c)), n(f64_bits(0x3feae91f71f3e6ab)), n(f64_bits(0x4000000000000000)), lit(0)], num(f64_bits(0xb8024839e169ef5c)), PARITY_VDB + " check_1"),
    ("VDB", [n(f64_bits(0x4a1199fae464d8ef)), n(f64_bits(0x4a1199fae464d8ee)), n(f64_bits(0x3fe4ed5cecb52239)), n(f64_bits(0x3fbed55d0079ad30)), n(f64_bits(0x3fceb660f2210928)), n(f64_bits(0x0000000000000000)), lit(0)], num(f64_bits(0x46a7638891ffafe2)), PARITY_VDB + " check_2"),
    ("VDB", [n(f64_bits(0x4937ad196b14dad2)), n(f64_bits(0x4933fcb6ed2a5d5e)), n(f64_bits(0x4041540fae6125ce)), n(f64_bits(0x3fe8290874c98230)), n(f64_bits(0x4022cfceb12002a8)), n(f64_bits(0x0000000000000000)), lit(0)], num(f64_bits(0x48ed77772288692e)), PARITY_VDB + " check_3"),
    ("VDB", [n(f64_bits(0x30507accf46c7fa5)), n(f64_bits(0x30507accf46c7fa4)), n(f64_bits(0x403223af1474b00f)), n(f64_bits(0x3fdffe0b58e433bc)), n(f64_bits(0x402f3832997baf68)), n(f64_bits(0x40210a47ff6a7425)), lit(0)], num(f64_bits(0x0000000000000000)), PARITY_VDB + " check_4"),
    ("VDB", [n(f64_bits(0x41c6a4abe3b9442a)), n(f64_bits(0x41c5f7f9d51118c3)), n(f64_bits(0x40379e7792105073)), n(f64_bits(0x3fed8673e0373101)), n(f64_bits(0x3ffec339f01b9881)), n(f64_bits(0x3fd0000000000000)), lit(0)], num(f64_bits(0x415e60de336f4b4a)), PARITY_VDB + " check_5"),
    ("VDB", [n(f64_bits(0x3b3279d807a14e37)), n(f64_bits(0x3b4fb7e6e9aa9f5b)), n(f64_bits(0x4048cf3b7f3a4eed)), n(f64_bits(0x4045f8a92a4035dd)), n(f64_bits(0x4048c6a8db4d5a6d)), n(f64_bits(0x0000000000000000)), lit(0)], num(f64_bits(0x37ce1ea55cdb9aa3)), PARITY_VDB + " check_6"),
    ("VDB", [n(f64_bits(0x362839702f5ec8db)), n(f64_bits(0x363f788b54a90925)), n(f64_bits(0x4049a1eef243509d)), n(f64_bits(0x40426dea9f901b59)), n(f64_bits(0x40434696906e04c1)), n(f64_bits(0x0000000000000000)), lit(0)], num(f64_bits(0x32a18ac0477e27d8)), PARITY_VDB + " check_7"),
    ("VDB", [n(f64_bits(0x3afc74d6e67507da)), n(f64_bits(0x3b1bde61bc60c367)), n(f64_bits(0x4022ae0ea5e22607)), n(f64_bits(0x3fd6a2f34a48d964)), n(f64_bits(0x401c41b3ef7bccb2)), n(f64_bits(0x4000000000000000)), lit(0)], num(f64_bits(0xbb14c12c02c38171)), PARITY_VDB + " check_8"),
    ("VDB", [n(f64_bits(0x3d892a343c22bf6e)), n(f64_bits(0x3da7f820739c6e3f)), n(f64_bits(0x4029526227df5474)), n(f64_bits(0x4021336336538e3f)), n(f64_bits(0x4021da2d7a69a01e)), n(f64_bits(0x0000000000000000)), lit(0)], num(f64_bits(0x3a0e28d44ba17614)), PARITY_VDB + " check_9"),
    ("VDB", [n(f64_bits(0x48d6c325f46597cf)), n(f64_bits(0x48eeeee15b5b54c1)), n(f64_bits(0x404735b2a3f64f49)), n(f64_bits(0x3ff350c705499c93)), n(f64_bits(0x403e7b8b4a58a26c)), n(f64_bits(0x0000000000000000)), lit(0)], num(f64_bits(0x4594b86517481cf9)), PARITY_VDB + " check_10"),
    ("VDB", [n(f64_bits(0x3a6b3cfed21cf476)), n(f64_bits(0x3a88a4bbde6dbb4f)), n(f64_bits(0x404c1f8bba3d2c96)), n(f64_bits(0x4048d85688e7d2f2)), n(f64_bits(0x404a46c1e94d6cdb)), n(f64_bits(0x0000000000000000)), lit(0)], num(f64_bits(0x36fadd8ba169dd60)), PARITY_VDB + " check_11"),
    ("VDB", [n(f64_bits(0x001b00b3c87c2144)), n(f64_bits(0x0020fac6b213cb21)), n(f64_bits(0x5613a18b0163bf88)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x408c200000000000)), n(f64_bits(0x402590a52fb5b809)), lit(0)], num(f64_bits(0x8006f4d99bab74fe)), PARITY_VDB + " check_12"),
    ("VDB", [n(f64_bits(0x00bb9705d6aa16e7)), n(f64_bits(0x00bb9705d6aa16e6)), n(f64_bits(0x403e049552c3e7e1)), n(f64_bits(0x3fe9c085c9e9d59c)), n(f64_bits(0x4030fa455f21923a)), n(f64_bits(0x40163a25be2f6d65)), lit(0)], num(f64_bits(0x0000000000000400)), PARITY_VDB + " check_13"),
    ("VDB", [n(f64_bits(0x00df155e0307a238)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x407430734313c2e2)), n(f64_bits(0x40532bbd6b23d98c)), n(f64_bits(0x407430734313c2e2)), n(f64_bits(0x401486af7910d3b6)), lit(0)], num(f64_bits(0x00c2356f8ee55aea)), PARITY_VDB + " check_14"),
    ("VDB", [n(f64_bits(0x00eab8f6acfa977f)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x40f21f6a65d73429)), n(f64_bits(0x4081356f0128278d)), n(f64_bits(0x4086db5f85ccb3a6)), n(f64_bits(0x401f5cd2c3f51f67)), lit(0)], num(f64_bits(0x008e6481ed405ea3)), PARITY_VDB + " check_15"),
    ("VDB", [n(f64_bits(0x012f6c3f495f93cc)), n(f64_bits(0x012f6c3f495f93cb)), n(f64_bits(0x405be86a1a2b9eb9)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x3ff18cdf9648dbc4)), n(f64_bits(0x4011d5665dec0256)), lit(0)], num(f64_bits(0x0000000000020000)), PARITY_VDB + " check_16"),
    ("VDB", [n(f64_bits(0x007efc36eeb4043f)), n(f64_bits(0x007efc36eeb4043e)), n(f64_bits(0x404f7544f411d469)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x404f7544f411d469)), n(f64_bits(0x402575f6070c68af)), lit(0)], num(f64_bits(0x0000000000000040)), PARITY_VDB + " check_17"),
    ("VDB", [n(f64_bits(0x0065712c8842e127)), n(f64_bits(0x0065712c8842e126)), n(f64_bits(0x4035864698fab518)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4013eb0b091ca752)), n(f64_bits(0x400cb8851866ef89)), lit(0)], num(f64_bits(0x0000000000000020)), PARITY_VDB + " check_18"),
    ("VDB", [n(f64_bits(0x001af144dd21206e)), n(f64_bits(0x001bcc5df08ef736)), n(f64_bits(0x7cceef9b94f783e4)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x408c200000000000)), n(f64_bits(0x012ba6aa80fd5065)), lit(0)], num(f64_bits(0x8000db19136dd6c8)), PARITY_VDB + " check_19"),
    ("VDB", [n(f64_bits(0x00f370027b8c891e)), n(f64_bits(0x00f370027b8c891d)), n(f64_bits(0x407cc67148ef84d6)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x407b3b9c8fd9a649)), n(f64_bits(0x4020a7ef605047eb)), lit(0)], num(f64_bits(0x0000000000004000)), PARITY_VDB + " check_20"),
    ("VDB", [n(f64_bits(0x002ded34e2129a73)), n(f64_bits(0x002e30e740cf52c5)), n(f64_bits(0x4074b05ba21d5691)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x406df902815c44b3)), n(f64_bits(0x0000000000000000)), lit(0)], num(f64_bits(0x80008764bd7970a4)), PARITY_VDB + " check_21"),
    ("VDB", [n(f64_bits(0x008b7ae0197db747)), n(f64_bits(0x008b7ae0197db746)), n(f64_bits(0x40797dfa808c00e4)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4070b6f56e0858f5)), n(f64_bits(0x4008241dba679bb0)), lit(0)], num(f64_bits(0x0000000000000080)), PARITY_VDB + " check_22"),
    ("VDB", [n(f64_bits(0x004a758ad516ab07)), n(f64_bits(0x004b6aba320406be)), n(f64_bits(0x7c785da870457170)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x408c200000000000)), n(f64_bits(0x0000000000000000)), lit(0)], num(f64_bits(0x8007a97ae76addb8)), PARITY_VDB + " check_23"),
    ("VDB", [n(f64_bits(0x018b3a4ff60c1689)), n(f64_bits(0x018b3a4ff60c1688)), n(f64_bits(0x40ce6723fefcd53b)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x40888a68ae364256)), n(f64_bits(0x3fff6487ebe519be)), lit(0)], num(f64_bits(0x0000000000800000)), PARITY_VDB + " check_24"),
    ("VDB", [n(f64_bits(0x7d163882a7a27e76)), n(f64_bits(0x7fefffffffffffff)), n(f64_bits(0x3ff16e863b64af6b)), n(f64_bits(0x3fc47b7ebc0bf9ee)), n(f64_bits(0x3fd44fe9cc953c8e)), n(f64_bits(0x4006621b9a6f48ff)), lit(0)], err("num"), PARITY_VDB + " check_25"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x3feeffffffffffff)), lit(0)], num(f64_bits(0x0020000000000000)), PARITY_VDB + " check_26"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3fd0000000000000)), n(f64_bits(0x3ff4000000000000)), n(f64_bits(0x3feeffffffffffff)), lit(0)], num(f64_bits(0x0020000000000000)), PARITY_VDB + " check_27"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3feeffffffffffff)), lit(0)], num(f64_bits(0x0020000000000000)), PARITY_VDB + " check_28"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x3fef000000000000)), lit(0)], num(f64_bits(0x0020000000000000)), PARITY_VDB + " check_29"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3fd0000000000000)), n(f64_bits(0x3ff4000000000000)), n(f64_bits(0x3fef000000000000)), lit(0)], num(f64_bits(0x0020000000000000)), PARITY_VDB + " check_30"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3fef000000000000)), lit(0)], num(f64_bits(0x0020000000000000)), PARITY_VDB + " check_31"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x3fef000000000001)), lit(0)], num(f64_bits(0x001f000000000001)), PARITY_VDB + " check_32"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3fd0000000000000)), n(f64_bits(0x3ff4000000000000)), n(f64_bits(0x3fef000000000001)), lit(0)], num(f64_bits(0x0022492492492492)), PARITY_VDB + " check_33"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3fef000000000001)), lit(0)], num(f64_bits(0x0020800000000000)), PARITY_VDB + " check_34"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x3feeffffffffe000)), lit(0)], num(f64_bits(0x0020000000000000)), PARITY_VDB + " check_35"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3fd0000000000000)), n(f64_bits(0x3ff4000000000000)), n(f64_bits(0x3feeffffffffe000)), lit(0)], num(f64_bits(0x0020000000000000)), PARITY_VDB + " check_36"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3feeffffffffe000)), lit(0)], num(f64_bits(0x0020000000000000)), PARITY_VDB + " check_37"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x3fef000000002000)), lit(0)], num(f64_bits(0x001f000000002000)), PARITY_VDB + " check_38"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3fd0000000000000)), n(f64_bits(0x3ff4000000000000)), n(f64_bits(0x3fef000000002000)), lit(0)], num(f64_bits(0x0022492492492492)), PARITY_VDB + " check_39"),
    ("VDB", [n(f64_bits(0x0030000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x4000000000000000)), n(f64_bits(0x3fef000000002000)), lit(0)], num(f64_bits(0x00207ffffffff000)), PARITY_VDB + " check_40"),
    ("VDB", [n(f64_bits(0x0170000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4010000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x3fefffffffffffff)), lit(0)], num(f64_bits(0x014fffffffffffff)), PARITY_VDB + " check_41"),
    ("VDB", [n(f64_bits(0x0170000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4010000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x3ff0000000000000)), lit(0)], num(f64_bits(0x0150000000000000)), PARITY_VDB + " check_42"),
    ("VDB", [n(f64_bits(0x0170000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4010000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x3ff0000000000001)), lit(0)], num(f64_bits(0x0150000000000001)), PARITY_VDB + " check_43"),
    ("VDB", [n(f64_bits(0x0170000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x4010000000000000)), n(f64_bits(0x0000000000000000)), n(f64_bits(0x3ff0000000000000)), n(f64_bits(0x3fefffffff800000)), lit(0)], num(f64_bits(0x014fffffff800000)), PARITY_VDB + " check_44"),
    # db_ddb_excel_parity_20260929.rs
    ("DB", [n(1000), n(100), n(0.5), n(0.5), n(1)], num(f64_bits(0x4054a00000000000)), PARITY_DB_DDB + " db_period_month_and_book_value_graph"),
    ("DB", [n(1000), n(100), n(0.5), n(1.99), n(1.5)], num(f64_bits(0x408a050ccccccccc)), PARITY_DB_DDB + " db_period_month_and_book_value_graph"),
    ("DB", [n(1000), n(100), n(0.5), n(1), n(6)], num(f64_bits(0x406f3f3333333334)), PARITY_DB_DDB + " db_period_month_and_book_value_graph"),
    ("DB", [n(1000), n(100), n(10), n(11), n(1)], num(f64_bits(0x40374721db621c07)), PARITY_DB_DDB + " db_period_month_and_book_value_graph"),
    ("DB", [n(1), n(2000), n(10), n(10), n(12)], num(f64_bits(0xc09098da01837956)), PARITY_DB_DDB + " db_period_month_and_book_value_graph"),
    ("DB", [n(0), n(2000), n(10), n(2), n(12)], num(0), PARITY_DB_DDB + " db_period_month_and_book_value_graph"),
    ("DB", [n(1000), n(100), n(10), n(11), n(12)], err("num"), PARITY_DB_DDB + " db_period_month_and_book_value_graph"),
    ("DB", [n(1000), n(100), n(10), n(1), n(0.5)], err("num"), PARITY_DB_DDB + " db_period_month_and_book_value_graph"),
    ("DDB", [n(1000), n(100), n(10), n(0.5), n(2)], num(f64_bits(0x4069000000000000)), PARITY_DB_DDB + " ddb_fractional_period_and_integer_power_publication"),
    ("DDB", [n(1000), n(100), n(10), n(10), n(2)], num(f64_bits(0x403ad7f29abcaf4f)), PARITY_DB_DDB + " ddb_fractional_period_and_integer_power_publication"),
    ("DDB", [n(1000), n(100), n(10), n(10), n(0.5)], num(f64_bits(0x403f8331440a866c)), PARITY_DB_DDB + " ddb_fractional_period_and_integer_power_publication"),
    ("DDB", [n(1000), n(100), n(10), n(10), n(1)], num(f64_bits(0x40435efb7556c434)), PARITY_DB_DDB + " ddb_fractional_period_and_integer_power_publication"),
    ("DDB", [n(1), n(0), n(10), n(10), n(2)], num(f64_bits(0x3f9b7cdfd9d7bdc2)), PARITY_DB_DDB + " ddb_fractional_period_and_integer_power_publication"),
    ("DDB", [n(1000), n(2000), n(10), n(1), n(2)], num(0), PARITY_DB_DDB + " ddb_fractional_period_and_integer_power_publication"),
    # syd_excel_parity_20260929.rs
    ("SYD", [n(-100), n(0), n(10), n(1)], num(f64_bits(0xc0322e8ba2e8ba2f)), PARITY_SYD + " syd_accepts_negative_cost_but_rejects_negative_salvage"),
    ("SYD", [n(100), n(-20), n(10), n(1)], err("num"), PARITY_SYD + " syd_accepts_negative_cost_but_rejects_negative_salvage"),
    ("SYD", [n(100), n(0), n(10), n(0)], err("num"), PARITY_SYD + " syd_accepts_negative_cost_but_rejects_negative_salvage"),
    ("SYD", [n(100), n(0), n(1e154), n(1e154)], num(0), PARITY_SYD + " syd_publishes_internal_stages_before_the_quotient"),
    ("SYD", [n(1e-300), n(0), n(1e-300), n(1e-300)], num(2), PARITY_SYD + " syd_publishes_internal_stages_before_the_quotient"),
    # sln_excel_parity_20260929.rs
    ("SLN", [n(100), n(0), n(-1)], num(-100), PARITY_SLN + " sln_signs_exact_zero_life_and_publication"),
    ("SLN", [n(-3), n(7), n(2)], num(-5), PARITY_SLN + " sln_signs_exact_zero_life_and_publication"),
    ("SLN", [n(1), n(0), n(1e-13)], num(1e13), PARITY_SLN + " sln_signs_exact_zero_life_and_publication"),
    ("SLN", [n(1), n(0), n(0)], err("div0"), PARITY_SLN + " sln_signs_exact_zero_life_and_publication"),
    ("SLN", [n(1), n(1), n(-1)], num(0), PARITY_SLN + " sln_signs_exact_zero_life_and_publication"),
    # bitwise_excel_parity_20260929.rs
    ("BITAND", [n(0.9), n(1.0)], err("num"), PARITY_BITWISE + " bitwise_dispatch_rejects_fractional_operands_on_both_sides"),
    ("BITOR", [n(0.9), n(1.0)], err("num"), PARITY_BITWISE + " bitwise_dispatch_rejects_fractional_operands_on_both_sides"),
    ("BITXOR", [n(0.9), n(1.0)], err("num"), PARITY_BITWISE + " bitwise_dispatch_rejects_fractional_operands_on_both_sides"),
    ("BITLSHIFT", [n(0.9), n(0.0)], err("num"), PARITY_BITWISE + " bitwise_dispatch_rejects_fractional_operands_on_both_sides"),
    ("BITRSHIFT", [n(0.9), n(0.0)], err("num"), PARITY_BITWISE + " bitwise_dispatch_rejects_fractional_operands_on_both_sides"),
    ("BITLSHIFT", [n(1.0), n(47.0)], num(140737488355328.0), PARITY_BITWISE + " bitwise_dispatch_shift_direction_overflow_and_shortcuts"),
    ("BITLSHIFT", [n(1.0), n(53.0)], num(0.0), PARITY_BITWISE + " bitwise_dispatch_shift_direction_overflow_and_shortcuts"),
    ("BITRSHIFT", [n(1.0), n(-53.0)], num(0.0), PARITY_BITWISE + " bitwise_dispatch_shift_direction_overflow_and_shortcuts"),
    ("BITLSHIFT", [n(197029275778236.0), n(-13.5)], num(24051425265.0), PARITY_BITWISE + " bitwise_dispatch_shift_direction_overflow_and_shortcuts"),
    # aggregate_prepared_excel_parity_20260929.rs
    ("HARMEAN", [EMPTY, n(8.0)], num(8.0), PARITY_AGG + " blank_cells_are_ignored_but_explicit_missing_contributes_zero"),
    ("DEVSQ", [EMPTY, n(8.0)], num(0.0), PARITY_AGG + " blank_cells_are_ignored_but_explicit_missing_contributes_zero"),
    ("HARMEAN", [EMPTY], err("na"), PARITY_AGG + " blank_cells_are_ignored_but_explicit_missing_contributes_zero"),
    ("DEVSQ", [EMPTY], err("num"), PARITY_AGG + " blank_cells_are_ignored_but_explicit_missing_contributes_zero"),
    # complex_family.rs
    ("COMPLEX", [n(3), n(4)], txt("3+4i"), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("COMPLEX", [n(3), n(-4)], txt("3-4i"), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("COMPLEX", [n(0), n(1)], txt("i"), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("COMPLEX", [n(0), n(-1)], txt("-i"), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("COMPLEX", [n(3), n(0)], txt("3"), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("COMPLEX", [n(0), n(0)], txt("0"), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("IMCONJUGATE", [s("3+4i")], txt("3-4i"), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("IMCONJUGATE", [s("3+4j")], txt("3-4j"), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("IMABS", [s("3+4i")], num(5.0), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("IMREAL", [s("3+4i")], num(3.0), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("IMAGINARY", [s("3+4i")], num(4.0), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("IMDIV", [s("3+4i"), s("1-2i")], txt("-1+2i"), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("IMSUM", [s("3"), s("1+2j")], txt("4+2j"), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("IMPRODUCT", [s("3+4i"), s("1-2i")], txt("11-2i"), COMPLEX_FN + " complex_rendering_and_basic_arithmetic_match_observations"),
    ("IMPOWER", [s("3+4i"), n(2)], txt("-7+24i"), COMPLEX_FN + " power_and_domain_lanes_match_excel_observations"),
    ("IMPOWER", [s("3+4i"), n(0)], txt("1"), COMPLEX_FN + " power_and_domain_lanes_match_excel_observations"),
    ("IMARGUMENT", [n(0)], err("div0"), COMPLEX_FN + " power_and_domain_lanes_match_excel_observations"),
    ("IMDIV", [s("3+4i"), n(0)], err("num"), COMPLEX_FN + " power_and_domain_lanes_match_excel_observations"),
    ("IMLN", [n(0)], err("num"), COMPLEX_FN + " power_and_domain_lanes_match_excel_observations"),
    ("IMSUM", [s("3+4i"), T], err("value"), COMPLEX_FN + " power_and_domain_lanes_match_excel_observations"),
    ("IMTAN", [s("3+4i")], txt("-0.000187346204629478+0.999355987381473i"), COMPLEX_FN + " tan_and_cot_use_excel_stable_identities"),
    ("IMCOT", [s("3+4i")], txt("-0.000187587737983659-1.00064439247156i"), COMPLEX_FN + " tan_and_cot_use_excel_stable_identities"),
]

J6 = [
    # --- blank_cell_coercion_truth_table.rs ---
    ("SUM", [EMPTY], num(0), "blank_cell_coercion_truth_table.rs SUM(blank)"),
    ("SUM", [EMPTY, n(1)], num(1), "blank_cell_coercion_truth_table.rs SUM(blank, 1)"),
    ("ABS", [EMPTY], num(0), "blank_cell_coercion_truth_table.rs ABS(blank)"),
    ("ROUND", [EMPTY, lit(2)], num(0), "blank_cell_coercion_truth_table.rs ROUND(blank, 2)"),
    ("PRODUCT", [EMPTY, n(3)], num(3), "blank_cell_coercion_truth_table.rs PRODUCT(blank, 3)"),
    ("AVERAGE", [EMPTY, n(2)], num(2), "blank_cell_coercion_truth_table.rs AVERAGE(blank, 2)"),
    ("COUNT", [EMPTY], num(0), "blank_cell_coercion_truth_table.rs COUNT(blank)"),
    ("COUNT", [EMPTY, n(1)], num(1), "blank_cell_coercion_truth_table.rs COUNT(blank, 1)"),
    ("COUNTA", [EMPTY], num(0), "blank_cell_coercion_truth_table.rs COUNTA(blank)"),
    ("COUNTA", [EMPTY, n(1)], num(1), "blank_cell_coercion_truth_table.rs COUNTA(blank, 1)"),
    ("MAX", [EMPTY], num(0), "blank_cell_coercion_truth_table.rs MAX(blank)"),
    ("MAX", [EMPTY, n(2)], num(2), "blank_cell_coercion_truth_table.rs MAX(blank, 2)"),
    ("NOT", [EMPTY], T_, "blank_cell_coercion_truth_table.rs NOT(blank)"),
    ("LEN", [EMPTY], num(0), "blank_cell_coercion_truth_table.rs LEN(blank)"),
    ("EXACT", [EMPTY, s("")], T_, "blank_cell_coercion_truth_table.rs EXACT(blank, '')"),
    ("EXACT", [EMPTY, n(0)], F_, "blank_cell_coercion_truth_table.rs EXACT(blank, 0)"),
    ("ISBLANK", [EMPTY], T_, "blank_cell_coercion_truth_table.rs ISBLANK(blank)"),
    ("ISNUMBER", [EMPTY], F_, "blank_cell_coercion_truth_table.rs ISNUMBER(blank)"),
    ("ISTEXT", [EMPTY], F_, "blank_cell_coercion_truth_table.rs ISTEXT(blank)"),
    ("ISNONTEXT", [EMPTY], T_, "blank_cell_coercion_truth_table.rs ISNONTEXT(blank)"),
    ("ISLOGICAL", [EMPTY], F_, "blank_cell_coercion_truth_table.rs ISLOGICAL(blank)"),
    ("ISERR", [EMPTY], F_, "blank_cell_coercion_truth_table.rs ISERR(blank)"),
    ("ISERROR", [EMPTY], F_, "blank_cell_coercion_truth_table.rs ISERROR(blank)"),
    ("ISNA", [EMPTY], F_, "blank_cell_coercion_truth_table.rs ISNA(blank)"),
    ("TYPE", [EMPTY], num(1), "blank_cell_coercion_truth_table.rs TYPE(blank)"),
    ("N", [EMPTY], num(0), "blank_cell_coercion_truth_table.rs N(blank)"),
    ("T", [EMPTY], txt(""), "blank_cell_coercion_truth_table.rs T(blank)"),
    ("CELL", [s("type"), EMPTY], txt("b"), "blank_cell_coercion_truth_table.rs CELL('type', blank)"),

    # --- and_fn.rs ---
    ("AND", [b(True), b(True)], T_, "and_fn.rs eval_and_returns_true_when_all_true"),
    ("AND", [b(True), b(False)], F_, "and_fn.rs eval_and_returns_false_when_any_false"),
    ("AND", [b(False), b(True)], F_, "and_fn.rs eval_and_returns_false_when_first_false"),
    ("AND", [b(True), n(0)], F_, "and_fn.rs eval_and_returns_false_when_any_arg_is_zero"),
    ("AND", [b(True), n(1)], T_, "and_fn.rs eval_and_treats_nonzero_numbers_as_true"),
    ("AND", [n(1), n(2)], T_, "and_fn.rs eval_and_two_nonzero_numbers"),
    ("AND", [n(0), n(1)], F_, "and_fn.rs eval_and_zero_and_nonzero"),
    ("AND", [s("TRUE")], T_, "and_fn.rs eval_and_coerces_direct_logical_spellings TRUE"),
    ("AND", [s("FALSE")], F_, "and_fn.rs eval_and_coerces_direct_logical_spellings FALSE"),
    ("AND", [s("fAlSe")], F_, "and_fn.rs eval_and_coerces_direct_logical_spellings fAlSe"),
    ("AND", [b(True), s("FALSE")], F_, "and_fn.rs eval_and_coerces_direct_logical_spellings TRUE, FALSE"),
    ("AND", [b(True), s("x")], T_, "and_fn.rs eval_and_ignores_direct_text_that_is_not_a_logical_spelling TRUE, x"),
    ("AND", [n(1), s("x")], T_, "and_fn.rs eval_and_ignores_direct_text_that_is_not_a_logical_spelling 1, x"),
    ("AND", [b(True), s("0")], T_, "and_fn.rs eval_and_ignores_direct_text_that_is_not_a_logical_spelling TRUE, 0"),
    ("AND", [s("1")], err("value"), "and_fn.rs eval_and_direct_text_is_value_error"),
    ("AND", [s("x")], err("value"), "and_fn.rs eval_and_direct_text_no_logical_seen"),
    ("AND", [b(False), e("#DIV/0!")], err("div0"), "and_fn.rs eval_and_eager_error_propagation"),
    ("AND", [b(False), e("#N/A"), e("#DIV/0!")], err("na"), "and_fn.rs eval_and_first_error_wins"),
    ("AND", [e("#DIV/0!"), e("#N/A")], err("div0"), "and_fn.rs eval_and_div0_first"),
    ("AND", [e("#N/A"), e("#DIV/0!")], err("na"), "and_fn.rs eval_and_na_first"),
    ("AND", [cell_s("x"), b(True)], T_, "and_fn.rs eval_and_ignores_reference_text"),

    # --- or_fn.rs ---
    ("OR", [b(False), b(False)], F_, "or_fn.rs eval_or_all_false"),
    ("OR", [b(True), b(False)], T_, "or_fn.rs eval_or_one_true"),
    ("OR", [n(0), n(0)], F_, "or_fn.rs eval_or_zeroes"),
    ("OR", [n(0), n(1)], T_, "or_fn.rs eval_or_zero_and_one"),
    ("OR", [s("TRUE")], T_, "or_fn.rs eval_or_coerces_direct_text_true"),
    ("OR", [s("FALSE")], F_, "or_fn.rs eval_or_coerces_direct_text_false"),
    ("OR", [b(False), s("TRUE")], T_, "or_fn.rs eval_or_coerces_direct_text_in_fold"),
    ("OR", [b(False), s("x")], F_, "or_fn.rs eval_or_ignores_direct_text"),
    ("OR", [b(False), s("1")], F_, "or_fn.rs eval_or_ignores_numeric_text"),
    ("OR", [s("x")], err("value"), "or_fn.rs eval_or_no_values_error"),
    ("OR", [b(True), e("#DIV/0!")], err("div0"), "or_fn.rs eval_or_eager_error_propagation"),
    ("OR", [e("#DIV/0!"), e("#N/A")], err("div0"), "or_fn.rs eval_or_div0_first"),
    ("OR", [e("#N/A"), e("#DIV/0!")], err("na"), "or_fn.rs eval_or_na_first"),

    # --- xor_fn.rs ---
    ("XOR", [b(True), b(False)], T_, "xor_fn.rs eval_xor_true_false"),
    ("XOR", [b(True), b(True)], F_, "xor_fn.rs eval_xor_true_true"),
    ("XOR", [b(False), b(False)], F_, "xor_fn.rs eval_xor_false_false"),
    ("XOR", [n(1), n(0)], T_, "xor_fn.rs eval_xor_1_0"),
    ("XOR", [n(1), n(1)], F_, "xor_fn.rs eval_xor_1_1"),
    ("XOR", [s("TRUE"), s("FALSE")], T_, "xor_fn.rs eval_xor_coerces_text"),
    ("XOR", [s("TRUE"), s("TRUE")], F_, "xor_fn.rs eval_xor_coerces_two_trues"),
    ("XOR", [b(True), e("#DIV/0!")], err("div0"), "xor_fn.rs eval_xor_eager_error"),

    # --- not_fn.rs ---
    ("NOT", [b(True)], F_, "not_fn.rs eval_not_true"),
    ("NOT", [b(False)], T_, "not_fn.rs eval_not_false"),
    ("NOT", [n(1)], F_, "not_fn.rs eval_not_one"),
    ("NOT", [n(0)], T_, "not_fn.rs eval_not_zero"),
    ("NOT", [n(-5)], F_, "not_fn.rs eval_not_negative"),
    ("NOT", [s("TRUE")], F_, "not_fn.rs eval_not_text_true"),
    ("NOT", [s("FALSE")], T_, "not_fn.rs eval_not_text_false"),
    ("NOT", [s("x")], err("value"), "not_fn.rs eval_not_text_x"),
    ("NOT", [e("#N/A")], err("na"), "not_fn.rs eval_not_error"),

    # --- if_fn.rs ---
    ("IF", [b(True), n(1), n(2)], num(1), "if_fn.rs eval_if_true_branch"),
    ("IF", [b(False), n(1), n(2)], num(2), "if_fn.rs eval_if_false_branch"),
    ("IF", [n(1), s("yes"), s("no")], txt("yes"), "if_fn.rs eval_if_nonzero_condition"),
    ("IF", [n(0), s("yes"), s("no")], txt("no"), "if_fn.rs eval_if_zero_condition"),
    ("IF", [b(True), n(1)], num(1), "if_fn.rs eval_if_omitted_false_true_case"),
    ("IF", [b(False), n(1)], F_, "if_fn.rs eval_if_omitted_false_false_case"),
    ("IF", [s("x"), n(1), n(2)], err("value"), "if_fn.rs eval_if_invalid_condition_type"),
    ("IF", [e("#N/A"), n(1), n(2)], err("na"), "if_fn.rs eval_if_error_condition"),

    # --- iferror.rs ---
    ("IFERROR", [n(10), n(99)], num(10), "iferror.rs eval_iferror_non_error"),
    ("IFERROR", [e("#DIV/0!"), n(99)], num(99), "iferror.rs eval_iferror_catches_div0"),
    ("IFERROR", [e("#N/A"), n(99)], num(99), "iferror.rs eval_iferror_catches_na"),
    ("IFERROR", [e("#VALUE!"), n(99)], num(99), "iferror.rs eval_iferror_catches_value"),
    ("IFERROR", [s("ok"), n(99)], txt("ok"), "iferror.rs eval_iferror_text_value"),
    ("IFNA", [n(10), n(99)], num(10), "iferror.rs eval_ifna_non_error"),
    ("IFNA", [e("#N/A"), n(99)], num(99), "iferror.rs eval_ifna_catches_na"),
    ("IFNA", [e("#DIV/0!"), n(99)], err("div0"), "iferror.rs eval_ifna_does_not_catch_div0"),

    # --- choose_ifs_family.rs ---
    ("IFS", [b(True), n(1), b(True), n(2)], num(1), "choose_ifs_family.rs eval_ifs_first_true"),
    ("IFS", [b(False), n(1), b(True), n(2)], num(2), "choose_ifs_family.rs eval_ifs_second_true"),
    ("IFS", [b(False), n(1), b(False), n(2)], err("na"), "choose_ifs_family.rs eval_ifs_none_true"),
    ("IFS", [e("#DIV/0!"), n(1), b(True), n(2)], err("div0"), "choose_ifs_family.rs eval_ifs_first_error"),
    ("CHOOSE", [n(1), s("a"), s("b"), s("c")], txt("a"), "choose_ifs_family.rs eval_choose_1"),
    ("CHOOSE", [n(2), s("a"), s("b"), s("c")], txt("b"), "choose_ifs_family.rs eval_choose_2"),
    ("CHOOSE", [n(3), s("a"), s("b"), s("c")], txt("c"), "choose_ifs_family.rs eval_choose_3"),
    ("CHOOSE", [n(0), s("a"), s("b")], err("value"), "choose_ifs_family.rs eval_choose_0"),
    ("CHOOSE", [n(4), s("a"), s("b"), s("c")], err("value"), "choose_ifs_family.rs eval_choose_overflow"),
    ("CHOOSE", [n(-1), s("a"), s("b")], err("value"), "choose_ifs_family.rs eval_choose_negative"),

    # --- switch_fn.rs ---
    ("SWITCH", [n(1), n(1), s("one"), n(2), s("two"), s("other")], txt("one"), "switch_fn.rs eval_switch_match_1"),
    ("SWITCH", [n(2), n(1), s("one"), n(2), s("two"), s("other")], txt("two"), "switch_fn.rs eval_switch_match_2"),
    ("SWITCH", [n(3), n(1), s("one"), n(2), s("two"), s("other")], txt("other"), "switch_fn.rs eval_switch_default"),
    ("SWITCH", [n(3), n(1), s("one"), n(2), s("two")], err("na"), "switch_fn.rs eval_switch_no_match_no_default"),
    ("SWITCH", [s("b"), s("a"), n(1), s("B"), n(2), n(3)], num(2), "switch_fn.rs eval_switch_case_insensitive"),
    ("SWITCH", [s("c"), s("a"), n(1), s("B"), n(2), n(3)], num(3), "switch_fn.rs eval_switch_text_default"),
    ("SWITCH", [e("#DIV/0!"), n(1), n(10), n(20)], err("div0"), "switch_fn.rs eval_switch_error_expression"),

    # --- is_predicates_family.rs & isnumber.rs ---
    ("ISBLANK", [s("")], F_, "is_predicates_family.rs isblank_empty_string"),
    ("ISBLANK", [n(0)], F_, "is_predicates_family.rs isblank_zero"),
    ("ISNUMBER", [n(1)], T_, "isnumber.rs isnumber_one"),
    ("ISNUMBER", [n(0)], T_, "isnumber.rs isnumber_zero"),
    ("ISNUMBER", [n(-3.14)], T_, "isnumber.rs isnumber_negative_float"),
    ("ISNUMBER", [s("1")], F_, "isnumber.rs isnumber_numeric_text"),
    ("ISNUMBER", [b(True)], F_, "isnumber.rs isnumber_boolean"),
    ("ISNUMBER", [e("#N/A")], F_, "isnumber.rs isnumber_error"),
    ("ISTEXT", [s("abc")], T_, "is_predicates_family.rs istext_string"),
    ("ISTEXT", [s("")], T_, "is_predicates_family.rs istext_empty_string"),
    ("ISTEXT", [n(123)], F_, "is_predicates_family.rs istext_number"),
    ("ISNONTEXT", [n(123)], T_, "is_predicates_family.rs isnontext_number"),
    ("ISNONTEXT", [b(True)], T_, "is_predicates_family.rs isnontext_logical"),
    ("ISNONTEXT", [e("#N/A")], T_, "is_predicates_family.rs isnontext_error"),
    ("ISNONTEXT", [s("x")], F_, "is_predicates_family.rs isnontext_text"),
    ("ISLOGICAL", [b(True)], T_, "is_predicates_family.rs islogical_true"),
    ("ISLOGICAL", [b(False)], T_, "is_predicates_family.rs islogical_false"),
    ("ISLOGICAL", [n(1)], F_, "is_predicates_family.rs islogical_one"),
    ("ISERR", [e("#DIV/0!")], T_, "is_predicates_family.rs iserr_div0"),
    ("ISERR", [e("#N/A")], F_, "is_predicates_family.rs iserr_na"),
    ("ISERROR", [e("#DIV/0!")], T_, "is_predicates_family.rs iserror_div0"),
    ("ISERROR", [e("#N/A")], T_, "is_predicates_family.rs iserror_na"),
    ("ISERROR", [n(1)], F_, "is_predicates_family.rs iserror_number"),
    ("ISNA", [e("#N/A")], T_, "is_predicates_family.rs isna_na"),
    ("ISNA", [e("#DIV/0!")], F_, "is_predicates_family.rs isna_div0"),
    ("ISODD", [n(1)], T_, "is_predicates_family.rs isodd_1"),
    ("ISODD", [n(2)], F_, "is_predicates_family.rs isodd_2"),
    ("ISODD", [n(1.9)], T_, "is_predicates_family.rs isodd_1_9"),
    ("ISODD", [n(-3)], T_, "is_predicates_family.rs isodd_neg3"),
    ("ISEVEN", [n(0)], T_, "is_predicates_family.rs iseven_0"),
    ("ISEVEN", [n(2)], T_, "is_predicates_family.rs iseven_2"),
    ("ISEVEN", [n(3)], F_, "is_predicates_family.rs iseven_3"),
    ("ISEVEN", [n(-4)], T_, "is_predicates_family.rs iseven_neg4"),

    # --- type_fn.rs, error_type_fn.rs, n_fn.rs, t_fn.rs ---
    ("TYPE", [n(1)], num(1), "type_fn.rs type_number"),
    ("TYPE", [s("x")], num(2), "type_fn.rs type_text"),
    ("TYPE", [s("")], num(2), "type_fn.rs type_empty_text"),
    ("TYPE", [b(True)], num(4), "type_fn.rs type_logical_true"),
    ("TYPE", [b(False)], num(4), "type_fn.rs type_logical_false"),
    ("TYPE", [e("#DIV/0!")], num(16), "type_fn.rs type_error_div0"),
    ("TYPE", [e("#N/A")], num(16), "type_fn.rs type_error_na"),
    ("ERROR.TYPE", [e("#NULL!")], num(1), "error_type_fn.rs error_type_null"),
    ("ERROR.TYPE", [e("#DIV/0!")], num(2), "error_type_fn.rs error_type_div0"),
    ("ERROR.TYPE", [e("#VALUE!")], num(3), "error_type_fn.rs error_type_value"),
    ("ERROR.TYPE", [e("#REF!")], num(4), "error_type_fn.rs error_type_ref"),
    ("ERROR.TYPE", [e("#NAME?")], num(5), "error_type_fn.rs error_type_name"),
    ("ERROR.TYPE", [e("#NUM!")], num(6), "error_type_fn.rs error_type_num"),
    ("ERROR.TYPE", [e("#N/A")], num(7), "error_type_fn.rs error_type_na"),
    ("ERROR.TYPE", [n(1)], err("na"), "error_type_fn.rs error_type_non_error"),
    ("N", [n(7)], num(7), "n_fn.rs n_number"),
    ("N", [b(True)], num(1), "n_fn.rs n_true"),
    ("N", [b(False)], num(0), "n_fn.rs n_false"),
    ("N", [s("text")], num(0), "n_fn.rs n_text"),
    ("N", [e("#DIV/0!")], err("div0"), "n_fn.rs n_error"),
    ("T", [s("hello")], txt("hello"), "t_fn.rs t_text"),
    ("T", [n(123)], txt(""), "t_fn.rs t_number"),
    ("T", [b(True)], txt(""), "t_fn.rs t_boolean"),
    ("T", [e("#DIV/0!")], err("div0"), "t_fn.rs t_error"),
    ("NA", [], err("na"), "misc_switch_info_family.rs na_call"),
    ("EXACT", [s("abc"), s("ABC")], F_, "excel_casing.rs exact_case_mismatch"),
    ("EXACT", [s("abc"), s("abc")], T_, "excel_casing.rs exact_identical"),
    ("EXACT", [s("\u00df"), s("ss")], F_, "excel_casing.rs exact_eszett"),
    ("CELL", [s("type"), cell_s("x")], txt("l"), "cell.rs cell_type_text"),
    ("CELL", [s("type"), n(123)], txt("v"), "cell.rs cell_type_number"),
]

J7 = [
    # --- xmatch.rs ---
    ("XMATCH", [lit(2), arr_num([[1, 2, 3]])], num(2), "xmatch.rs eval_xmatch_adapter_prepared_defaults_to_exact_forward"),
    ("XMATCH", [lit(2), arr_num([[1, 2, 2]]), lit(0), lit(-1)], num(3), "xmatch.rs eval_xmatch_adapter_prepared_reverse_mode_returns_last_match"),
    ("XMATCH", [lit(9), arr_num([[1, 2, 3]])], err("na"), "xmatch.rs eval_xmatch_adapter_prepared_not_found_returns_na_lane"),
    ("XMATCH", [lit(0.3), arr_num([[0.30000000000000004]]), lit(0)], err("na"), "xmatch.rs eval_xmatch_adapter_prepared_exact_mode_keeps_near_equal_numbers_distinct"),
    ("XMATCH", [lit(1), arr_num([[1]]), lit(9)], err("value"), "xmatch.rs eval_xmatch_adapter_prepared_rejects_invalid_modes"),
    ("XMATCH", [lit(2.5), arr_num([[1, 2, 3]]), lit(1)], num(3), "xmatch.rs eval_xmatch_adapter_prepared_supports_approximate_modes exact_or_larger"),
    ("XMATCH", [lit(2.5), arr_num([[1, 2, 3]]), lit(-1)], num(2), "xmatch.rs eval_xmatch_adapter_prepared_supports_approximate_modes exact_or_smaller"),
    ("XMATCH", [lit(2), arr_num([[1, 2, 3]]), lit(2), lit(2)], err("value"), "xmatch.rs eval_xmatch_adapter_prepared_rejects_wildcard_binary_search"),
    ("XMATCH", [lit(2), arr_num([[1, 2, 3]]), lit(0), lit(1)], num(2), "xmatch.rs eval_xmatch_adapter_prepared_supports_binary_modes forward"),

    # --- xlookup.rs ---
    ("XLOOKUP", [lit(2), arr_num([[1], [2]]), arr_num([[10], [20]])], num(20), "xlookup.rs eval_xlookup_exact_forward_returns_value"),
    ("XLOOKUP", [lit(9), arr_num([[1], [2]]), arr_num([[10], [20]]), s("fallback")], txt("fallback"), "xlookup.rs eval_xlookup_returns_if_not_found_fallback"),
    ("XLOOKUP", [lit(9), arr_num([[1], [2]]), arr_num([[10], [20]])], err("na"), "xlookup.rs eval_xlookup_not_found_no_fallback"),
    ("XLOOKUP", [lit(2.5), arr_num([[1], [2], [3]]), arr_num([[10], [20], [30]]), omitted(), lit(1)], num(30), "xlookup.rs eval_xlookup_approximate_larger"),
    ("XLOOKUP", [lit(2.5), arr_num([[1], [2], [3]]), arr_num([[10], [20], [30]]), omitted(), lit(-1)], num(20), "xlookup.rs eval_xlookup_approximate_smaller"),
    ("XLOOKUP", [lit(2), arr_num([[1], [2], [3], [2]]), arr_num([[10], [20], [30], [40]]), omitted(), lit(0), lit(-1)], num(40), "xlookup.rs eval_xlookup_reverse_exact"),
    ("XLOOKUP", [lit(1), arr_num([[1], [2]]), arr_num([[10], [20], [30]])], err("value"), "xlookup.rs eval_xlookup_rejects_orientation_mismatch"),
    ("XLOOKUP", [lit(2), arr_num([[1, 2]]), arr_num([[10, 20]])], num(20), "xlookup.rs eval_xlookup_horizontal_lookup"),

    # --- match_fn.rs ---
    ("MATCH", [lit(3), arr_num([[1, 3, 5]])], num(2), "match_fn.rs eval_match_default_match_type_uses_approximate_next_smaller"),
    ("MATCH", [lit(3), arr_num([[1, 3, 5]]), lit(0)], num(2), "match_fn.rs eval_match_exact_returns_first_index"),
    ("MATCH", [lit(4), arr_num([[1, 3, 5]]), lit(1)], num(2), "match_fn.rs eval_match_default_and_type_one_approximate"),
    ("MATCH", [lit(4), arr_num([[5, 3, 1]]), lit(-1)], num(2), "match_fn.rs eval_match_type_negative_one_descending"),
    ("MATCH", [lit(0), arr_num([[1, 3, 5]]), lit(1)], err("na"), "match_fn.rs match_below_lowest_returns_na"),
    ("MATCH", [lit(6), arr_num([[5, 3, 1]]), lit(-1)], err("na"), "match_fn.rs match_above_highest_descending_returns_na"),

    # --- index.rs ---
    ("INDEX", [arr_num([[10], [20], [30]]), lit(2)], num(20), "index.rs eval_index_column_vector_single_position"),
    ("INDEX", [arr_num([[10, 20, 30]]), lit(2)], num(20), "index.rs eval_index_row_vector_single_position"),
    ("INDEX", [arr_num([[1, 2], [3, 4]]), lit(2), lit(1)], num(3), "index.rs eval_index_array_position_returns_payload_value"),
    ("INDEX", [arr_num([[1, 2], [3, 4]]), lit(1), lit(2)], num(2), "index.rs eval_index_array_position_row1_col2"),
    ("INDEX", [arr_num([[1, 2], [3, 4]]), lit(5), lit(1)], err("ref"), "index.rs eval_index_array_bounds_checked row"),
    ("INDEX", [arr_num([[1, 2], [3, 4]]), lit(1), lit(5)], err("ref"), "index.rs eval_index_array_bounds_checked col"),
    ("INDEX", [lit(100), lit(1), lit(1)], num(100), "index.rs eval_index_scalar_source"),
    ("INDEX", [lit(100), lit(2), lit(1)], err("ref"), "index.rs eval_index_scalar_out_of_bounds"),

    # --- vhlookup_family.rs ---
    ("VLOOKUP", [lit(2), arr_num([[1, 10], [2, 20], [3, 30]]), lit(2), lit_b(False)], num(20), "vhlookup_family.rs vlookup_exact"),
    ("VLOOKUP", [lit(2.5), arr_num([[1, 10], [2, 20], [3, 30]]), lit(2), lit_b(True)], num(20), "vhlookup_family.rs vlookup_approximate"),
    ("VLOOKUP", [lit(0), arr_num([[1, 10], [2, 20], [3, 30]]), lit(2), lit_b(True)], err("na"), "vhlookup_family.rs vlookup_below_lowest"),
    ("VLOOKUP", [lit(2), arr_num([[1, 10], [2, 20]]), lit(3), lit_b(False)], err("ref"), "vhlookup_family.rs lookup_indices_truncate_and_validate_bounds"),
    ("VLOOKUP", [lit(2), arr_num([[1, 10], [2, 20]]), lit(0), lit_b(False)], err("value"), "vhlookup_family.rs lookup_indices_below_one_return_value_error"),
    ("VLOOKUP", [lit(9), arr_num([[1, 10], [2, 20]]), lit(2), lit_b(False)], err("na"), "vhlookup_family.rs exact_lookup_no_match_returns_not_available"),
    ("HLOOKUP", [lit(2), arr_num([[1, 2, 3], [10, 20, 30]]), lit(2), lit_b(False)], num(20), "vhlookup_family.rs hlookup_exact"),
    ("HLOOKUP", [lit(2.5), arr_num([[1, 2, 3], [10, 20, 30]]), lit(2), lit_b(True)], num(20), "vhlookup_family.rs hlookup_approximate"),
    ("HLOOKUP", [lit(0), arr_num([[1, 2, 3], [10, 20, 30]]), lit(2), lit_b(True)], err("na"), "vhlookup_family.rs hlookup_below_lowest"),
    ("HLOOKUP", [lit(2), arr_num([[1, 2], [10, 20]]), lit(3), lit_b(False)], err("ref"), "vhlookup_family.rs hlookup_indices_out_of_bounds"),
    ("HLOOKUP", [lit(2), arr_num([[1, 2], [10, 20]]), lit(0), lit_b(False)], err("value"), "vhlookup_family.rs hlookup_indices_below_one"),

    # --- rows_fn.rs ---
    ("ROWS", [arr_num([[1, 2, 3]])], num(1), "rows_fn.rs rows_row_vector"),
    ("ROWS", [arr_num([[1], [2], [3]])], num(3), "rows_fn.rs rows_col_vector"),
    ("ROWS", [arr_num([[1, 2], [3, 4], [5, 6]])], num(3), "rows_fn.rs rows_matrix"),
    ("ROWS", [lit(42)], num(1), "rows_fn.rs rows_scalar_number"),
    ("ROWS", [s("text")], num(1), "rows_fn.rs rows_scalar_text"),
    ("ROWS", [lit_b(True)], num(1), "rows_fn.rs rows_scalar_logical"),
    ("ROWS", [EMPTY], num(1), "rows_fn.rs rows_single_cell"),

    # --- columns_fn.rs ---
    ("COLUMNS", [arr_num([[1, 2, 3]])], num(3), "columns_fn.rs columns_row_vector"),
    ("COLUMNS", [arr_num([[1], [2], [3]])], num(1), "columns_fn.rs columns_col_vector"),
    ("COLUMNS", [arr_num([[1, 2], [3, 4], [5, 6]])], num(2), "columns_fn.rs columns_matrix"),
    ("COLUMNS", [lit(42)], num(1), "columns_fn.rs columns_scalar_number"),
    ("COLUMNS", [s("text")], num(1), "columns_fn.rs columns_scalar_text"),
    ("COLUMNS", [lit_b(True)], num(1), "columns_fn.rs columns_scalar_logical"),
    ("COLUMNS", [EMPTY], num(1), "columns_fn.rs columns_single_cell"),

    # --- row_fn.rs ---

    # --- column_fn.rs ---

    # --- reference_metadata_family.rs ---
    ("ADDRESS", [lit(1), lit(1)], txt("$A$1"), "reference_metadata_family.rs address_1_1"),
    ("ADDRESS", [lit(2), lit(3)], txt("$C$2"), "reference_metadata_family.rs address_2_3"),
    ("ADDRESS", [lit(3), lit(2)], txt("$B$3"), "reference_metadata_family.rs address_default_a1_is_absolute"),
    ("ADDRESS", [lit(3), lit(2), lit(1)], txt("$B$3"), "reference_metadata_family.rs address_abs_row_abs_col"),
    ("ADDRESS", [lit(3), lit(2), lit(2)], txt("B$3"), "reference_metadata_family.rs address_abs_row_rel_col"),
    ("ADDRESS", [lit(3), lit(2), lit(3)], txt("$B3"), "reference_metadata_family.rs address_rel_row_abs_col"),
    ("ADDRESS", [lit(3), lit(2), lit(4)], txt("B3"), "reference_metadata_family.rs address_rel_row_rel_col"),
    ("ADDRESS", [lit(3), lit(2), lit(1), lit_b(True), s("Sheet1")], txt("Sheet1!$B$3"), "reference_metadata_family.rs address_quoted_sheet_text"),
    ("ADDRESS", [lit(3), lit(2), lit(1), lit_b(False)], txt("R3C2"), "reference_metadata_family.rs address_r1c1_abs"),
    ("ADDRESS", [lit(3), lit(2), lit(4), lit_b(False)], txt("R[3]C[2]"), "reference_metadata_family.rs address_r1c1_rel"),
    ("ADDRESS", [lit(3), lit(2), lit(2), lit_b(False)], txt("R3C[2]"), "reference_metadata_family.rs address_r1c1_mixed_row_abs"),
    ("ADDRESS", [lit(3), lit(2), lit(3), lit_b(False)], txt("R[3]C2"), "reference_metadata_family.rs address_r1c1_mixed_col_abs"),
    ("ADDRESS", [lit(0), lit(3)], err("value"), "reference_metadata_family.rs address_row_zero_value_error"),
    ("ADDRESS", [lit(3), lit(0)], err("value"), "reference_metadata_family.rs address_col_zero_value_error"),
    ("ADDRESS", [lit(3), lit(2), lit(5)], err("value"), "reference_metadata_family.rs address_abs_num_out_of_range"),
    ("AREAS", [EMPTY], num(1), "reference_metadata_family.rs areas_single_cell"),

    # --- offset.rs ---
    ("OFFSET", [EMPTY, lit(-100), lit(0)], err("ref"), "offset.rs offset_negative_row_off_top"),
    ("OFFSET", [EMPTY, lit(0), lit(-5)], err("ref"), "offset.rs offset_negative_col_off_left"),

    # --- indirect.rs ---
    ("INDIRECT", [s("invalid reference")], err("ref"), "indirect.rs eval_indirect_rejects_invalid_reference"),
    ("INDIRECT", [lit(123)], err("ref"), "indirect.rs eval_indirect_rejects_non_text_reference"),
    ("INDIRECT", [lit_b(True)], err("ref"), "indirect.rs eval_indirect_rejects_boolean_reference"),
]

J8 = [
    # --- number_regex_translate_family.rs ---
    ("REGEXTEST", [s("abc123"), s(r"\d+")], T_, "number_regex_translate_family.rs regextest_surface_returns_logical"),
    ("REGEXTEST", [s("abc"), s(r"\d+")], F_, "number_regex_translate_family.rs regextest_no_match"),
    ("REGEXTEST", [s("abc"), s("ABC"), lit(0)], F_, "number_regex_translate_family.rs regextest_case_sensitive"),
    ("REGEXTEST", [s("abc"), s("ABC"), lit(1)], T_, "number_regex_translate_family.rs regextest_case_insensitive"),
    ("REGEXTEST", [s(""), s("^$")], T_, "number_regex_translate_family.rs regextest_empty_string"),
    ("REGEXTEST", [s("hello world"), s(r"\bworld\b")], T_, "number_regex_translate_family.rs regextest_word_boundary"),
    ("REGEXTEST", [s("hello"), s("[")], err("value"), "number_regex_translate_family.rs regextest_syntax_error"),
    ("REGEXEXTRACT", [s("Code-42"), s(r"[A-Z][a-z]+-\d+")], txt("Code-42"), "number_regex_translate_family.rs regexextract_surface_returns_text"),
    ("REGEXEXTRACT", [s("Order-12345"), s(r"\d+")], txt("12345"), "number_regex_translate_family.rs regexextract_digits"),
    ("REGEXEXTRACT", [s("abc"), s(r"\d+")], err("na"), "number_regex_translate_family.rs regexextract_no_match"),
    ("REGEXEXTRACT", [s("abc"), s("[")], err("value"), "number_regex_translate_family.rs regexextract_syntax_error"),
    ("REGEXREPLACE", [s("abc123def"), s(r"\d+"), s("-")], txt("abc-def"), "number_regex_translate_family.rs regexreplace_digits"),
    ("REGEXREPLACE", [s("abc"), s(r"\d+"), s("-")], txt("abc"), "number_regex_translate_family.rs regexreplace_no_match"),
    ("REGEXREPLACE", [s("hello world"), s("world"), s("there")], txt("hello there"), "number_regex_translate_family.rs regexreplace_literal"),
    ("REGEXREPLACE", [s("abc"), s("["), s("x")], err("value"), "number_regex_translate_family.rs regexreplace_syntax_error"),

    # --- text_delim_family.rs ---
    ("TEXTBEFORE", [s("hello-world"), s("-")], txt("hello"), "text_delim_family.rs textbefore_basic"),
    ("TEXTBEFORE", [s("a-b-c-d"), s("-"), lit(2)], txt("a-b"), "text_delim_family.rs textbefore_instance_2"),
    ("TEXTBEFORE", [s("a-b-c-d"), s("-"), lit(-1)], txt("a-b-c"), "text_delim_family.rs textbefore_instance_neg1"),
    ("TEXTBEFORE", [s("hello-world"), s("x")], err("na"), "text_delim_family.rs textbefore_not_found"),
    ("TEXTBEFORE", [s("hello-world"), s("x"), lit(1), lit(0), lit(0), s("fallback")], txt("fallback"), "text_delim_family.rs textbefore_if_not_found"),
    ("TEXTBEFORE", [s("hello-world"), s("WORLD"), lit(1), lit(1)], txt("hello"), "text_delim_family.rs textbefore_case_insensitive"),
    ("TEXTBEFORE", [s("hello-world"), s("WORLD"), lit(1), lit(0)], err("na"), "text_delim_family.rs textbefore_case_sensitive_na"),
    ("TEXTBEFORE", [s("hello-world"), s("x"), lit(1), lit(0), lit(1)], txt("hello-world"), "text_delim_family.rs textbefore_match_end"),
    ("TEXTAFTER", [s("hello-world"), s("-")], txt("world"), "text_delim_family.rs textafter_basic"),
    ("TEXTAFTER", [s("a-b-c-d"), s("-"), lit(2)], txt("c-d"), "text_delim_family.rs textafter_instance_2"),
    ("TEXTAFTER", [s("a-b-c-d"), s("-"), lit(-1)], txt("d"), "text_delim_family.rs textafter_instance_neg1"),
    ("TEXTAFTER", [s("hello-world"), s("x")], err("na"), "text_delim_family.rs textafter_not_found"),
    ("TEXTAFTER", [s("hello-world"), s("x"), lit(1), lit(0), lit(0), s("fallback")], txt("fallback"), "text_delim_family.rs textafter_if_not_found"),
    ("TEXTAFTER", [s("hello-world"), s("HELLO"), lit(1), lit(1)], txt("world"), "text_delim_family.rs textafter_case_insensitive"),
    ("TEXTAFTER", [s("hello-world"), s("HELLO"), lit(1), lit(0)], err("na"), "text_delim_family.rs textafter_case_sensitive_na"),
    ("TEXTAFTER", [s("hello-world"), s("x"), lit(1), lit(0), lit(1)], txt(""), "text_delim_family.rs textafter_match_end"),

    # --- array_text_split_family.rs ---
    ("TEXTSPLIT", [s("a-b-c"), s("-")], txt("a"), "array_text_split_family.rs textsplit_first_cell"),
    ("ARRAYTOTEXT", [arr_num([[1.0, 2.0], [3.0, 4.0]])], txt("1, 2, 3, 4"), "array_text_split_family.rs arraytotext_concise"),
    ("ARRAYTOTEXT", [arr_num([[1.0, 2.0], [3.0, 4.0]]), lit(1)], txt("{1,2;3,4}"), "array_text_split_family.rs arraytotext_strict"),
    ("ARRAYTOTEXT", [lit(42)], txt("42"), "array_text_split_family.rs arraytotext_scalar"),

    # --- numeric_text.rs ---
    ("VALUETOTEXT", [lit(123)], txt("123"), "numeric_text.rs valuetotext_number"),
    ("VALUETOTEXT", [s("hello")], txt("hello"), "numeric_text.rs valuetotext_text_concise"),
    ("VALUETOTEXT", [s("hello"), lit(1)], txt('"hello"'), "numeric_text.rs valuetotext_text_strict"),
    ("VALUETOTEXT", [lit_b(True)], txt("TRUE"), "numeric_text.rs valuetotext_bool"),
    ("NUMBERVALUE", [s("1234.56")], num(1234.56), "numeric_text.rs numbervalue_standard"),
    ("NUMBERVALUE", [s("1,234.56"), s("."), s(",")], num(1234.56), "numeric_text.rs numbervalue_custom_en"),
    ("NUMBERVALUE", [s("1.234,56"), s(","), s(".")], num(1234.56), "numeric_text.rs numbervalue_custom_de"),
    ("NUMBERVALUE", [s("25%")], num(0.25), "numeric_text.rs numbervalue_percent"),
    ("NUMBERVALUE", [s(" 123.45 ")], num(123.45), "numeric_text.rs numbervalue_whitespace"),
    ("NUMBERVALUE", [s("123.45"), s(","), s(",")], err("value"), "numeric_text.rs numbervalue_same_separators"),
    ("NUMBERVALUE", [s("abc")], err("value"), "numeric_text.rs numbervalue_non_numeric"),
    ("VALUE", [s("1234.5")], num(1234.5), "numeric_text.rs value_simple"),
    ("VALUE", [s(" -1234.5 ")], num(-1234.5), "numeric_text.rs value_negative_trimmed"),
    ("VALUE", [s("1.23E+03")], num(1230.0), "numeric_text.rs value_scientific"),
    ("VALUE", [s("50%")], num(0.5), "numeric_text.rs value_percent"),
    ("VALUE", [s("xyz")], err("value"), "numeric_text.rs value_non_numeric"),
    ("VALUE", [s("")], err("value"), "numeric_text.rs value_empty_string"),

    # --- fixed_fn.rs ---
    ("FIXED", [lit(1234.567)], txt("1,234.57"), "fixed_fn.rs fixed_default_decimals"),
    ("FIXED", [lit(1234.567), lit(1)], txt("1,234.6"), "fixed_fn.rs fixed_1_decimal"),
    ("FIXED", [lit(1234.567), lit(0)], txt("1,235"), "fixed_fn.rs fixed_0_decimals"),
    ("FIXED", [lit(1234.567), lit(-1)], txt("1,230"), "fixed_fn.rs fixed_neg_decimal"),
    ("FIXED", [lit(1234.567), lit(2), lit_b(True)], txt("1234.57"), "fixed_fn.rs fixed_no_commas"),
    ("FIXED", [lit(1234.567), lit(2), lit_b(False)], txt("1,234.57"), "fixed_fn.rs fixed_with_commas"),
    ("FIXED", [lit(-1234.567), lit(2)], txt("-1,234.57"), "fixed_fn.rs fixed_negative"),
    ("FIXED", [lit(0), lit(2)], txt("0.00"), "fixed_fn.rs fixed_zero"),

    # --- dollar_fn.rs ---
    ("DOLLAR", [lit(1234.567)], txt("$1,234.57"), "dollar_fn.rs dollar_default"),
    ("DOLLAR", [lit(1234.567), lit(2)], txt("$1,234.57"), "dollar_fn.rs dollar_2_decimals"),
    ("DOLLAR", [lit(1234.567), lit(0)], txt("$1,235"), "dollar_fn.rs dollar_0_decimals"),
    ("DOLLAR", [lit(1234.567), lit(-2)], txt("$1,200"), "dollar_fn.rs dollar_neg_decimals"),
    ("DOLLAR", [lit(-1234.567), lit(2)], txt("($1,234.57)"), "dollar_fn.rs dollar_negative"),

    # --- dollar_fraction_family.rs ---
    ("DOLLARDE", [lit(1.02), lit(16)], num(1.125), "dollar_fraction_family.rs dollarde_16ths"),
    ("DOLLARDE", [lit(1.1), lit(8)], num(1.125), "dollar_fraction_family.rs dollarde_8ths"),
    ("DOLLARDE", [lit(1.0), lit(16)], num(1.0), "dollar_fraction_family.rs dollarde_exact_integer"),
    ("DOLLARDE", [lit(1.02), lit(0)], err("num"), "dollar_fraction_family.rs dollarde_zero_fraction"),
    ("DOLLARDE", [lit(1.02), lit(-1)], err("num"), "dollar_fraction_family.rs dollarde_negative_fraction"),
    ("DOLLARFR", [lit(1.125), lit(16)], num(1.02), "dollar_fraction_family.rs dollarfr_16ths"),
    ("DOLLARFR", [lit(1.125), lit(8)], num(1.1), "dollar_fraction_family.rs dollarfr_8ths"),
    ("DOLLARFR", [lit(1.0), lit(16)], num(1.0), "dollar_fraction_family.rs dollarfr_exact_integer"),
    ("DOLLARFR", [lit(1.125), lit(0)], err("num"), "dollar_fraction_family.rs dollarfr_zero_fraction"),
    ("DOLLARFR", [lit(1.125), lit(-1)], err("num"), "dollar_fraction_family.rs dollarfr_negative_fraction"),

    # --- clean_fn.rs ---
    ("CLEAN", [s("hello")], txt("hello"), "clean_fn.rs clean_clean_text"),
    ("CLEAN", [s("")], txt(""), "clean_fn.rs clean_empty"),
    ("CLEAN", [lit(123)], txt("123"), "clean_fn.rs clean_coerced_number"),

    # --- exact_fn.rs ---
    ("EXACT", [s("word"), s("word")], T_, "exact_fn.rs exact_equal"),
    ("EXACT", [s("word"), s("Word")], F_, "exact_fn.rs exact_case_mismatch"),
    ("EXACT", [s("word"), s("word ")], F_, "exact_fn.rs exact_space_mismatch"),
    ("EXACT", [lit(123), s("123")], F_, "exact_fn.rs exact_number_vs_text"),
    ("EXACT", [lit_b(True), s("TRUE")], F_, "exact_fn.rs exact_bool_vs_text"),
    ("EXACT", [lit_b(True), lit_b(True)], T_, "exact_fn.rs exact_bool_equal"),
    ("EXACT", [lit(123), lit(123)], T_, "exact_fn.rs exact_number_equal"),
    ("EXACT", [s(""), s("")], T_, "exact_fn.rs exact_empty_strings"),

    # --- textjoin.rs ---
    ("CONCAT", [s("a"), s("b"), s("c")], txt("abc"), "textjoin.rs concat_scalars"),
    ("CONCAT", [arr_num([[1.0, 2.0], [3.0, 4.0]])], txt("1234"), "textjoin.rs concat_array"),
    ("CONCAT", [s("prefix-"), lit(42), s("-suffix")], txt("prefix-42-suffix"), "textjoin.rs concat_mixed"),
    ("CONCAT", [s("")], txt(""), "textjoin.rs concat_empty"),
    ("TEXTJOIN", [s("-"), lit_b(True), s("a"), s("b"), s("c")], txt("a-b-c"), "textjoin.rs textjoin_basic"),
    ("TEXTJOIN", [s("-"), lit_b(True), s("a"), s(""), s("c")], txt("a-c"), "textjoin.rs textjoin_ignore_empty"),
    ("TEXTJOIN", [s("-"), lit_b(False), s("a"), s(""), s("c")], txt("a--c"), "textjoin.rs textjoin_keep_empty"),
    ("TEXTJOIN", [s(","), lit_b(True), arr_num([[1.0, 2.0], [3.0, 4.0]])], txt("1,2,3,4"), "textjoin.rs textjoin_array"),
    ("TEXTJOIN", [s(""), lit_b(True), s("x"), s("y")], txt("xy"), "textjoin.rs textjoin_empty_delim"),

    # --- text_scalar_misc.rs ---
    ("CONCATENATE", [s("hello"), s(" "), s("world")], txt("hello world"), "text_scalar_misc.rs concatenate_strings"),
    ("CONCATENATE", [lit(1), lit(2), lit(3)], txt("123"), "text_scalar_misc.rs concatenate_numbers"),
    ("CONCATENATE", [s("val="), lit_b(True)], txt("val=TRUE"), "text_scalar_misc.rs concatenate_bool"),
    ("UPPER", [s("hello World")], txt("HELLO WORLD"), "text_scalar_misc.rs upper_mixed"),
    ("UPPER", [s("123 abc!")], txt("123 ABC!"), "text_scalar_misc.rs upper_digits_symbols"),
    ("LOWER", [s("Hello World")], txt("hello world"), "text_scalar_misc.rs lower_mixed"),
    ("LOWER", [s("123 ABC!")], txt("123 abc!"), "text_scalar_misc.rs lower_digits_symbols"),
    ("PROPER", [s("hello world")], txt("Hello World"), "text_scalar_misc.rs proper_words"),
    ("PROPER", [s("HELLO WORLD")], txt("Hello World"), "text_scalar_misc.rs proper_uppercase"),
    ("PROPER", [s("mr. john smith")], txt("Mr. John Smith"), "text_scalar_misc.rs proper_punctuation"),
    ("PROPER", [s("123apple")], txt("123Apple"), "text_scalar_misc.rs proper_digits_prefix"),
    ("PROPER", [s("it's great")], txt("It'S Great"), "text_scalar_misc.rs proper_apostrophe"),
    ("TRIM", [s("  hello   world  ")], txt("hello world"), "text_scalar_misc.rs trim_extra_spaces"),
    ("TRIM", [s("hello")], txt("hello"), "text_scalar_misc.rs trim_no_spaces"),
    ("TRIM", [s("")], txt(""), "text_scalar_misc.rs trim_empty"),
    ("REPT", [s("abc"), lit(3)], txt("abcabcabc"), "text_scalar_misc.rs rept_3"),
    ("REPT", [s("x"), lit(5)], txt("xxxxx"), "text_scalar_misc.rs rept_single_char"),
    ("REPT", [s("abc"), lit(0)], txt(""), "text_scalar_misc.rs rept_0"),
    ("REPT", [s("abc"), lit(1.9)], txt("abc"), "text_scalar_misc.rs rept_truncation"),
    ("REPT", [s("abc"), lit(-1)], err("value"), "text_scalar_misc.rs rept_negative"),
    ("CHAR", [lit(65)], txt("A"), "text_scalar_misc.rs char_65"),
    ("CHAR", [lit(97)], txt("a"), "text_scalar_misc.rs char_97"),
    ("CHAR", [lit(48)], txt("0"), "text_scalar_misc.rs char_48"),
    ("CHAR", [lit(32)], txt(" "), "text_scalar_misc.rs char_32"),
    ("CHAR", [lit(0)], err("value"), "text_scalar_misc.rs char_0"),
    ("CHAR", [lit(256)], err("value"), "text_scalar_misc.rs char_256"),
    ("CODE", [s("A")], num(65), "text_scalar_misc.rs code_A"),
    ("CODE", [s("Apple")], num(65), "text_scalar_misc.rs code_first_char"),
    ("CODE", [s("1")], num(49), "text_scalar_misc.rs code_digit"),
    ("CODE", [s("")], err("value"), "text_scalar_misc.rs code_empty"),

    # --- text_slice_family.rs ---
    ("LEFT", [s("abcdef"), lit(3)], txt("abc"), "text_slice_family.rs left_3"),
    ("LEFT", [s("abcdef"), lit(0)], txt(""), "text_slice_family.rs left_0"),
    ("LEFT", [s("abcdef"), lit(10)], txt("abcdef"), "text_slice_family.rs left_excess"),
    ("LEFT", [s("abcdef")], txt("a"), "text_slice_family.rs left_default_1"),
    ("LEFT", [s("abcdef"), lit(-1)], err("value"), "text_slice_family.rs left_negative"),
    ("LEFT", [lit(12345), lit(2)], txt("12"), "text_slice_family.rs left_coerced_number"),
    ("RIGHT", [s("abcdef"), lit(3)], txt("def"), "text_slice_family.rs right_3"),
    ("RIGHT", [s("abcdef"), lit(0)], txt(""), "text_slice_family.rs right_0"),
    ("RIGHT", [s("abcdef"), lit(10)], txt("abcdef"), "text_slice_family.rs right_excess"),
    ("RIGHT", [s("abcdef")], txt("f"), "text_slice_family.rs right_default_1"),
    ("RIGHT", [s("abcdef"), lit(-1)], err("value"), "text_slice_family.rs right_negative"),
    ("RIGHT", [lit(12345), lit(2)], txt("45"), "text_slice_family.rs right_coerced_number"),
    ("MID", [s("abcdef"), lit(2), lit(3)], txt("bcd"), "text_slice_family.rs mid_normal"),
    ("MID", [s("abcdef"), lit(1), lit(2)], txt("ab"), "text_slice_family.rs mid_from_start"),
    ("MID", [s("abcdef"), lit(5), lit(10)], txt("ef"), "text_slice_family.rs mid_past_end"),
    ("MID", [s("abcdef"), lit(10), lit(2)], txt(""), "text_slice_family.rs mid_start_beyond"),
    ("MID", [s("abcdef"), lit(0), lit(2)], err("value"), "text_slice_family.rs mid_zero_start"),
    ("MID", [s("abcdef"), lit(2), lit(-1)], err("value"), "text_slice_family.rs mid_negative_length"),
    ("MID", [s("abcdef"), lit(2), lit(0)], txt(""), "text_slice_family.rs mid_zero_length"),
    ("MID", [lit(12345), lit(2), lit(2)], txt("23"), "text_slice_family.rs mid_coerced_number"),
    ("LEN", [s("abcdef")], num(6), "text_slice_family.rs len_string"),
    ("LEN", [s("")], num(0), "text_slice_family.rs len_empty"),
    ("LEN", [lit(12345)], num(5), "text_slice_family.rs len_number"),
    ("LEN", [lit_b(True)], num(4), "text_slice_family.rs len_boolean"),

    # --- text_search_replace_family.rs ---
    ("FIND", [s("c"), s("abcdef")], num(3), "text_search_replace_family.rs find_basic"),
    ("FIND", [s("C"), s("abcdef")], err("value"), "text_search_replace_family.rs find_case_sensitive"),
    ("FIND", [s("c"), s("abcdefabc"), lit(4)], num(9), "text_search_replace_family.rs find_start_num"),
    ("FIND", [s(""), s("abcdef")], num(1), "text_search_replace_family.rs find_empty_needle"),
    ("FIND", [s("x"), s("abcdef")], err("value"), "text_search_replace_family.rs find_not_found"),
    ("FIND", [s("c"), s("abcdef"), lit(0)], err("value"), "text_search_replace_family.rs find_start_zero"),
    ("FIND", [s("c"), s("abcdef"), lit(10)], err("value"), "text_search_replace_family.rs find_start_beyond"),
    ("SEARCH", [s("c"), s("abcdef")], num(3), "text_search_replace_family.rs search_basic"),
    ("SEARCH", [s("C"), s("abcdef")], num(3), "text_search_replace_family.rs search_case_insensitive"),
    ("SEARCH", [s("?d"), s("abcdef")], num(3), "text_search_replace_family.rs search_wildcard_question"),
    ("SEARCH", [s("*e"), s("abcdef")], num(1), "text_search_replace_family.rs search_wildcard_asterisk"),
    ("SEARCH", [s("~?"), s("abc?def")], num(4), "text_search_replace_family.rs search_escaped_question"),
    ("SEARCH", [s("~*"), s("abc*def")], num(4), "text_search_replace_family.rs search_escaped_asterisk"),
    ("SEARCH", [s("x"), s("abcdef")], err("value"), "text_search_replace_family.rs search_not_found"),
    ("SEARCH", [s(""), s("abcdef")], num(1), "text_search_replace_family.rs search_empty_needle"),
    ("REPLACE", [s("abcdef"), lit(2), lit(3), s("XYZ")], txt("aXYZef"), "text_search_replace_family.rs replace_middle"),
    ("REPLACE", [s("abcdef"), lit(2), lit(0), s("XYZ")], txt("aXYZbcdef"), "text_search_replace_family.rs replace_insert"),
    ("REPLACE", [s("abcdef"), lit(1), lit(6), s("")], txt(""), "text_search_replace_family.rs replace_delete_all"),
    ("REPLACE", [s("abcdef"), lit(0), lit(2), s("X")], err("value"), "text_search_replace_family.rs replace_start_zero"),
    ("REPLACE", [s("abcdef"), lit(2), lit(-1), s("X")], err("value"), "text_search_replace_family.rs replace_negative_len"),
    ("SUBSTITUTE", [s("banana"), s("a"), s("o")], txt("bonono"), "text_search_replace_family.rs substitute_all"),
    ("SUBSTITUTE", [s("banana"), s("a"), s("o"), lit(1)], txt("bonana"), "text_search_replace_family.rs substitute_instance_1"),
    ("SUBSTITUTE", [s("banana"), s("a"), s("o"), lit(2)], txt("baona"), "text_search_replace_family.rs substitute_instance_2"),
    ("SUBSTITUTE", [s("banana"), s("a"), s("o"), lit(3)], txt("banoa"), "text_search_replace_family.rs substitute_instance_3"),
    ("SUBSTITUTE", [s("banana"), s("a"), s("o"), lit(4)], txt("banana"), "text_search_replace_family.rs substitute_instance_beyond"),
    ("SUBSTITUTE", [s("banana"), s("x"), s("o")], txt("banana"), "text_search_replace_family.rs substitute_not_found"),
    ("SUBSTITUTE", [s("banana"), s("a"), s("")], txt("bnn"), "text_search_replace_family.rs substitute_delete"),
    ("SUBSTITUTE", [s("banana"), s("A"), s("o")], txt("banana"), "text_search_replace_family.rs substitute_case_sensitive"),
    ("SUBSTITUTE", [s("banana"), s("a"), s("o"), lit(0)], err("value"), "text_search_replace_family.rs substitute_instance_zero"),
    ("SUBSTITUTE", [s("banana"), s("a"), s("o"), lit(-1)], err("value"), "text_search_replace_family.rs substitute_instance_negative"),

    # --- text_unicode_fn.rs ---
    ("UNICHAR", [lit(65)], txt("A"), "text_unicode_fn.rs unichar_65"),
    ("UNICHAR", [lit(8364)], txt("€"), "text_unicode_fn.rs unichar_euro"),
    ("UNICHAR", [lit(0)], err("value"), "text_unicode_fn.rs unichar_zero"),
    ("UNICODE", [s("A")], num(65), "text_unicode_fn.rs unicode_A"),
    ("UNICODE", [s("€")], num(8364), "text_unicode_fn.rs unicode_euro"),
    ("UNICODE", [s("")], err("value"), "text_unicode_fn.rs unicode_empty"),

    # --- text_b_compat_family.rs ---
    ("LENB", [s("hello")], num(5), "text_b_compat_family.rs lenb_ascii"),
    ("LEFTB", [s("hello"), lit(3)], txt("hel"), "text_b_compat_family.rs leftb_ascii"),
    ("RIGHTB", [s("hello"), lit(3)], txt("llo"), "text_b_compat_family.rs rightb_ascii"),
    ("MIDB", [s("hello"), lit(2), lit(3)], txt("ell"), "text_b_compat_family.rs midb_ascii"),
    ("FINDB", [s("l"), s("hello")], num(3), "text_b_compat_family.rs findb_ascii"),
    ("SEARCHB", [s("l"), s("hello")], num(3), "text_b_compat_family.rs searchb_ascii"),
    ("REPLACEB", [s("hello"), lit(2), lit(3), s("XYZ")], txt("hXYZo"), "text_b_compat_family.rs replaceb_ascii"),

    # --- text_compat_locale_family.rs ---
    ("ASC", [s("hello")], txt("hello"), "text_compat_locale_family.rs asc_ascii"),

    # --- web_text_xml_family.rs ---
    ("ENCODEURL", [s("https://example.com/test?a=1&b=2")], txt("https%3A%2F%2Fexample.com%2Ftest%3Fa%3D1%26b%3D2"), "web_text_xml_family.rs encodeurl_url"),
    ("ENCODEURL", [s("hello world")], txt("hello%20world"), "web_text_xml_family.rs encodeurl_space"),
    ("ENCODEURL", [lit(123)], txt("123"), "web_text_xml_family.rs encodeurl_number"),
    ("FILTERXML", [s("<root><a>1</a><a>2</a></root>"), s("//a[1]")], num(1), "web_text_xml_family.rs filterxml_first_element"),
    ("FILTERXML", [s("<root><a>hello</a></root>"), s("//a")], txt("hello"), "web_text_xml_family.rs filterxml_text_node"),
    ("FILTERXML", [s("<root><a>1</a></root>"), s("//b")], err("value"), "web_text_xml_family.rs filterxml_empty_nodeset"),
    ("FILTERXML", [s("not xml"), s("//a")], err("value"), "web_text_xml_family.rs filterxml_invalid_xml"),

    # --- format_time_fraction_accounting_tests.rs, ftc_0654, locale_format_expansion ---
    ("TEXT", [lit(0.625), s("h:mm")], txt("15:00"), "format_time_fraction_accounting_tests.rs text_h_mm"),
    ("TEXT", [lit(0.625), s("hh:mm")], txt("15:00"), "format_time_fraction_accounting_tests.rs text_hh_mm"),
    ("TEXT", [lit(0.625), s("HH:mm:ss")], txt("15:00:00"), "format_time_fraction_accounting_tests.rs text_HH_mm_ss"),
    ("TEXT", [lit(0.625), s("h:mm AM/PM")], txt("3:00 PM"), "format_time_fraction_accounting_tests.rs text_ampm"),
    ("TEXT", [lit(0.0), s("h:mm AM/PM")], txt("12:00 AM"), "format_time_fraction_accounting_tests.rs text_midnight"),
    ("TEXT", [lit(0.5), s("h:mm AM/PM")], txt("12:00 PM"), "format_time_fraction_accounting_tests.rs text_noon"),
    ("TEXT", [lit(45293.625), s("yyyy-mm-dd hh:mm:ss")], txt("2024-01-02 15:00:00"), "format_time_fraction_accounting_tests.rs text_datetime"),
    ("TEXT", [lit(1.625), s("[h]:mm:ss")], txt("39:00:00"), "format_time_fraction_accounting_tests.rs text_elapsed_hours"),
    ("TEXT", [lit(1.625), s("[m]:ss")], txt("2340:00"), "format_time_fraction_accounting_tests.rs text_elapsed_minutes"),
    ("TEXT", [lit(1.625), s("[s]")], txt("140400"), "format_time_fraction_accounting_tests.rs text_elapsed_seconds"),
    ("TEXT", [lit(0.25), s("# ?/?")], txt(" 1/4"), "ftc_0654_fraction_format_engine_tests.rs text_fraction_quarter"),
    ("TEXT", [lit(0.5), s("# ?/?")], txt(" 1/2"), "ftc_0654_fraction_format_engine_tests.rs text_fraction_half"),
    ("TEXT", [lit(1234.5), s("#,##0.00")], txt("1,234.50"), "locale_format_expansion_tests.rs text_thousands_sep"),
    ("TEXT", [lit(1234.5), s("0.00E+00")], txt("1.23E+03"), "locale_format_expansion_tests.rs text_scientific"),
    ("TEXT", [lit(-12.34), s('$#,##0.00;($#,##0.00);"-"')], txt("($12.34)"), "locale_format_expansion_tests.rs text_accounting_negative"),
    ("TEXT", [lit(0), s('$#,##0.00;($#,##0.00);"-"')], txt("-"), "locale_format_expansion_tests.rs text_accounting_zero"),
    ("TEXT", [lit(1234567), s('#,, "M"')], txt("1 M"), "locale_format_expansion_tests.rs text_scale_millions"),
    ("TEXT", [lit(0.85), s("0.0%")], txt("85.0%"), "locale_format_expansion_tests.rs text_percent"),
    ("TEXT", [lit(45293), s("yyyy-mm-dd")], txt("2024-01-02"), "locale_format_expansion_tests.rs text_date_iso"),
    ("TEXT", [lit(12.5), s("General")], txt("12.5"), "locale_format_expansion_tests.rs text_general_number"),
    ("TEXT", [s("abc"), s("General")], txt("abc"), "locale_format_expansion_tests.rs text_general_text"),
    ("TEXT", [lit(1234), s("00000")], txt("01234"), "locale_format_expansion_tests.rs text_leading_zeros"),
]

CASES = {"J10": J10, "J11": J11, "J4": J4, "J6": J6, "J7": J7, "J8": J8}



