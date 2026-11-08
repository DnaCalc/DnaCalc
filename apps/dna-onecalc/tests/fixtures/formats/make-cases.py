"""Writes cases.tsv, the questions OneCalc's number-format oracle asks Excel (dc-0f1k, milestone 3).

    python make-cases.py > cases.tsv

Each row: id, culture, decimal separator, thousands separator, LCID prefix for date names (hex),
date system, value kind, value, format code. A separator is the character itself, or U+NNNN.
The answers are captured by tools/excel-session/formats-job.ps1 into excel-formats.tsv, and the
test `excel_number_formats` (tests/number_formats.rs) replays them through the formatter the
surface uses.

The culture axis is what an owned Excel instance can be told: the separators (the values below are
the Windows separators of the culture) and the [$-LCID] prefix that selects the language of day and
month names. Format codes are written in the culture's separators, as the page's format box takes
them; letters in date codes stay English.
"""

CULTURES = [
    # culture, decimal, thousands, LCID of date names
    ("en-US", ".", ",", "409"),
    ("en-GB", ".", ",", "809"),
    ("en-ZA", ",", "U+00A0", "1C09"),
    ("de-DE", ",", ".", "407"),
    ("fr-FR", ",", "U+202F", "40C"),
]

NUMBERS = ["1234.5678", "-1234.5", "0", "0.5", "0.000012345", "123456789.125", "1E+21", "-0.004"]
DATES = ["45322", "45322.75", "0.5", "36526.999988426", "59.5", "61"]


def number_codes(dec, thou):
    t = "U+00A0" if thou == "U+00A0" else thou
    return [
        "General",
        "0",
        f"0{dec}00",
        f"#{thou}##0",
        f"#{thou}##0{dec}00",
        f"0{dec}0%",
        f"0{dec}00E+00",
        f"##0{dec}0E+0",
        "# ?/?",
        "# ??/??",
        f"$#{thou}##0{dec}00",
        f"#{thou}##0{dec}00;[Red]-#{thou}##0{dec}00",
        f"#{thou}##0{dec}00;(#{thou}##0{dec}00);\"zero\"",
        f"0{dec}00;-0{dec}00;",
        f"[<1]0{dec}000;0{dec}0",
        "000000",
        f"#{dec}#",
        f"?{dec}??",
        "@",
        "\"x\"0",
    ]


DATE_CODES = [
    "yyyy-mm-dd",
    "d/m/yyyy",
    "dd-mmm-yy",
    "dddd d mmmm yyyy",
    "mmm yy",
    "mmmmm",
    "ddd",
    "h:mm:ss",
    "h:mm AM/PM",
    "hh:mm:ss{dec}000",
    "[h]:mm",
    "[mm]:ss",
    "yyyy-mm-dd hh:mm:ss",
    "d mmmm",
    "General",
    f"0{{dec}}00",
]




# dc-0f1k.6: fraction codes. Rows after the date rows so the earlier ids keep their questions.
# kind `number` is a cell (Range.Text); kind `fn` is the cell's formula =TEXT(value,"code"); kind
# `text` is a text cell. Codes here have no decimal point, so one set serves every culture.
FRACTION_VALUES = [
    "1234.5678", "-1234.5", "0", "0.5", "0.000012345", "123456789.125", "1E+21", "-0.004",
    "0.3333333", "0.99", "0.999999", "1.5", "2.999", "-0.5", "-2.75", "3.14159265358979",
    "0.0625", "1", "-1", "0.01", "0.005", "0.995", "99.99", "0.125", "0.4999", "0.5001",
    "1E-7", "1E+15", "7.999999999", "0.1", "0.7", "0.9", "5.05", "12.34", "-0.3333",
    "2", "100.25", "0.04", "0.96", "1.0001", "33.333", "-7.125", "0.6666667", "9.5", "1E+10",
    "0.2", "0.3", "0.4", "0.6", "0.8", "0.45", "0.55", "0.65", "0.35", "0.15", "0.1875", "0.25", "0.375",
    "0.4375", "0.123456789", "0.987654321", "0.142857142857143", "0.333", "0.667", "0.875", "0.8333333",
    "0.1666667", "0.2857143", "0.4285714", "0.5714286", "0.7142857", "0.8571429", "3.142857142857143",
    "3.1415929203539825", "2.718281828459045", "1.41421356237", "0.618033988749895", "0.05", "0.02", "0.003",
    "0.0001", "0.00001", "1E-20", "-1E-7", "-1E+9", "2.675", "0.285", "1.005", "0.03125", "0.0078125",
    "32767", "32767.5", "32768", "65535", "100000.5", "1E+9", "1E+12", "214748364.7", "2147483647",
    "2147483648", "4294967295", "4294967296", "-0.25", "-0.1", "-0.9", "-1.5", "-99.99", "-0.999999",
    "-3.14159265358979", "1.9999999", "0.00999", "0.0049", "49.5", "999.999", "-999.999",
]
FRACTION_VALUES_SMALL = ["1234.5678", "-1234.5", "0", "0.5", "-0.004", "0.3333333", "2.999", "-2.75", "0.01", "99.99", "1", "3.14159265358979"]
FRACTION_CODES = [
    "?/?", "??/??", "???/???", "#/#", "0/0", "00/00", "?/8", "??/16", "#/100", "?/100", "0/4", "?/2", "?/10", "#/8", "?/64", "?/1000",
    "# ?/?", "# ??/??", "# ???/???", "# #/#", "# 0/0", "0 ?/?", "0 0/0", "# ?/2", "# ?/4", "# ?/8", "# ??/16", "# ?/10",
    "# ?/100", "# #/100", "? ?/?", "00 ?/?", "0000 ?/?", "#,##0 ?/?", "# ?/64", "# ?/1000", "# ????/????", "# ?????/?????",
    "# ?/?\"x\"", "\"about \"# ?/?", "# ?/? \"left\"", "[Red]# ?/?", "# ?/?_)", "$# ?/?",
    "# ?/?;-# ?/?", "# ?/?;(# ?/?)", "# ?/?;# ?/?;\"zero\"", "# ?/?;[Red]-# ?/?;\"nil\";\"t:\"@", "[<1]?/?;# ?/?",
    "[>=100]# ?/?;0.00", "# ?/?;;", ";;# ?/?", "?/?;-?/?", "# ?/?;\"neg\"# ?/?", "?/8;(?/8);\"zero\"",
    "# \"and\" ?/?", "# ?\\/?","# ?\"/\"?", "0.0 ?/?", "# ?/? ?/?", "# ??/?", "# ?/??", "# #?/?#",
]
# The layout of the pieces: what stands between the integer part and the numerator, which
# placeholders pad, and what the denominator and the numerator may be.
LAYOUT_CODES = [
    "# ?#/?", "# #?/?", "# ?0/?", "# 0?/?", "# 0#/?", "? #/#", "# 0/?", "# ?/0", "# #/0", "# 0/#", "#  ?/?",
    "#   #/#", "#\\ ?/?","# \"-\" ?/?", "#-?/?", "#x?/?", "0-?/?", "#\" \"?/?", "#?/?", "0?/?", "# ?/?   ",
    " # ?/?", "#_ ?/?", "?# ?/?", "# ?/?%", "?/?%", "#,##0 ?/8", "#,##0.00 ?/?", "[Blue]# ?/?;[Red]# ?/?",
    "[$-409]# ?/?", "[$U+20AC-2] # ?/?", "# ?/?;@", "# ?/?;;;@@", "# ??/?;# ?/??", "# ?/?E+0", 
    "#0 ?/?", "0 #/#", "00 #/#", "? ?/8", "# ?/8.5",
]
LAYOUT_VALUES = ["0", "0.5", "1", "1.5", "2.999", "-0.5", "-1.25", "0.000012345", "-0.004", "12.75", "0.3333333"]
FRACTION_CULTURE_CODES = ["# ?/?", "# ??/??", "?/8", "# ?/?;-# ?/?"]


def fractions(n):
    print_rows = []
    for culture, dec, thou, lcid in CULTURES[:1] + CULTURES[3:4]:
        codes = FRACTION_CODES if culture == "en-US" else FRACTION_CULTURE_CODES
        values = FRACTION_VALUES if culture == "en-US" else FRACTION_VALUES_SMALL
        for value in values:
            for code in codes:
                n += 1
                print_rows.append(("number", culture, dec, thou, value, code, n))
    for value in FRACTION_VALUES_SMALL:
        for code in FRACTION_CODES:
            n += 1
            print_rows.append(("fn", "en-US", ".", ",", value, code, n))
    for value in LAYOUT_VALUES:
        for code in LAYOUT_CODES:
            n += 1
            print_rows.append(("number", "en-US", ".", ",", value, code, n))
    for value in LAYOUT_VALUES[:6]:
        for code in LAYOUT_CODES:
            n += 1
            print_rows.append(("fn", "en-US", ".", ",", value, code, n))
    for value in ["abc", "12", "1/2", "1 1/2"]:
        for code in ["# ?/?", "# ?/?;;;\"t:\"@", "@", "?/8;?/8;?/8;@@"]:
            n += 1
            print_rows.append(("text", "en-US", ".", ",", value, code, n))
    for kind, culture, dec, thou, value, code, k in print_rows:
        print("	".join([f"nf{k:04d}", culture, dec, thou, "", "1900", kind, value, code]))
    return n


def main():
    print("# DNA OneCalc number-format oracle questions: see make-cases.py.")
    n = 0
    for culture, dec, thou, lcid in CULTURES:
        for value in NUMBERS:
            for code in number_codes(dec, thou):
                n += 1
                print("\t".join([f"nf{n:04d}", culture, dec, thou, "", "1900", "number", value, code]))
        for value in ["abc", "12"]:
            for code in ["@", "General", f"0{dec}00", "\"t:\"@", ";;;@"]:
                n += 1
                print("\t".join([f"nf{n:04d}", culture, dec, thou, "", "1900", "text", value, code]))
    # Dates: every date code under each culture's day and month names, in both date systems.
    for system in ("1900", "1904"):
        for culture, dec, thou, lcid in CULTURES:
            for value in DATES:
                for code in DATE_CODES:
                    code = code.replace("{dec}", dec)
                    n += 1
                    print("\t".join([f"nf{n:04d}", culture, dec, thou, lcid, system, "number", value, code]))
    fractions(n)


main()
