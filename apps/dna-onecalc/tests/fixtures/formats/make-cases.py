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


main()
