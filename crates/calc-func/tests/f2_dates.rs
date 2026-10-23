//! F2 (dc-ip6): the date and time functions, the number-format functions (TEXT, VALUE, FIXED,
//! DOLLAR, NUMBERVALUE), the host-fact functions, ADDRESS, the database functions and the other
//! text functions, called through `calc_func::invoke` in a calculation environment. Expected
//! values are Excel's (16.0.20430 on this machine, Windows locale en-ZA with `.` and a space as
//! separators; "Excel:" rows read ad hoc through COM), Microsoft's documented examples, or
//! OxFunc's tests (OxFunc 6fca3518, functions/).

mod f1_support;

use calc_func::locale::{CalcEnvironment, DateSystem, FormatProfile};
use calc_func::value::{CalcValue, CellKey, SheetId, WorksheetErrorCode::*};
use calc_func::HostFacts;
use f1_support::*;

/// This machine's settings: en-ZA with `.` and a space, the 1900 date system, and a clock in
/// 2026 (VALUE("1/2") read 2 January 2026).
fn host() -> CalcEnvironment {
    CalcEnvironment {
        date_system: DateSystem::System1900,
        profile: Some(
            FormatProfile::for_culture("en-ZA")
                .unwrap()
                .with_separators(".", " ")
                .unwrap(),
        ),
        now: Some(46318.6),
    }
}

fn c(env: &CalcEnvironment, spelling: &str, args: Vec<CalcValue>) -> Result<CalcValue, calc_func::FunctionFault> {
    call_in(env, None, None, spelling, args)
}

#[test]
fn dates_without_an_environment_are_refused() {
    refused(call("DATE", vec![n(2024.0), n(1.0), n(15.0)]));
    refused(call("YEAR", vec![n(45000.0)]));
    refused(call("TEXT", vec![n(1.0), t("0.00")]));
}

#[test]
fn date_parts_and_serials() {
    let e = host();
    exact(c(&e, "DATE", vec![n(2024.0), n(1.0), n(15.0)]), 45306.0);
    exact(c(&e, "DATE", vec![n(1900.0), n(2.0), n(29.0)]), 60.0);
    exact(c(&e, "DATE", vec![n(108.0), n(1.0), n(2.0)]), 39449.0);
    exact(c(&e, "DATE", vec![n(2008.0), n(14.0), n(2.0)]), 39846.0);
    error(c(&e, "DATE", vec![n(10000.0), n(1.0), n(1.0)]), Num);
    exact(c(&e, "YEAR", vec![n(45000.0)]), 2023.0);
    exact(c(&e, "MONTH", vec![n(45000.0)]), 3.0);
    exact(c(&e, "DAY", vec![n(45000.0)]), 15.0);
    exact(c(&e, "HOUR", vec![n(45000.75)]), 18.0);
    exact(c(&e, "MINUTE", vec![n(0.51)]), 14.0);
    exact(c(&e, "SECOND", vec![n(0.5)]), 0.0);
    error(c(&e, "YEAR", vec![n(-1.0)]), Num);
    exact(c(&e, "TIME", vec![n(12.0), n(30.0), n(0.0)]), 0.5208333333333334);
    exact(c(&e, "DAYS", vec![n(45010.0), n(45000.0)]), 10.0);
    exact(c(&e, "DAYS360", vec![n(45000.0), n(45100.0)]), 98.0);
    exact(c(&e, "EDATE", vec![n(45000.0), n(1.0)]), 45031.0);
    exact(c(&e, "EOMONTH", vec![n(45000.0), n(1.0)]), 45046.0);
    error(c(&e, "EDATE", vec![b(true), n(1.0)]), Value);
    exact(c(&e, "WEEKDAY", vec![n(45000.0)]), 4.0);
    exact(c(&e, "WEEKDAY", vec![n(45000.0), n(2.0)]), 3.0);
    exact(c(&e, "WEEKNUM", vec![n(45000.0)]), 11.0);
    exact(c(&e, "ISOWEEKNUM", vec![n(45000.0)]), 11.0);
    exact(c(&e, "DATEDIF", vec![n(45000.0), n(45365.0), t("d")]), 365.0);
    exact(c(&e, "DATEDIF", vec![n(45000.0), n(45365.0), t("m")]), 11.0);
    error(c(&e, "DATEDIF", vec![n(45365.0), n(45000.0), t("d")]), Num);
    exact(c(&e, "NETWORKDAYS", vec![n(45000.0), n(45030.0)]), 23.0);
    exact(c(&e, "WORKDAY", vec![n(45000.0), n(10.0)]), 45014.0);
    exact(c(&e, "NETWORKDAYS.INTL", vec![n(45000.0), n(45030.0), t("0000011")]), 23.0);
    error(c(&e, "WORKDAY.INTL", vec![n(45000.0), n(10.0), t("1111111")]), Value);
    // Microsoft's YEARFRAC example: 2012/1/1 to 2012/7/30, basis 0, 1 and 3.
    close(c(&e, "YEARFRAC", vec![n(40909.0), n(41120.0)]), 0.58055556, 1e-8);
    close(c(&e, "YEARFRAC", vec![n(40909.0), n(41120.0), n(1.0)]), 0.57650273, 1e-8);
    close(c(&e, "YEARFRAC", vec![n(40909.0), n(41120.0), n(3.0)]), 0.57808219, 1e-8);
    exact(c(&e, "TODAY", vec![]), 46318.0);
    exact(c(&e, "NOW", vec![]), 46318.6);
}

#[test]
fn the_1904_date_system() {
    let mut e = host();
    e.date_system = DateSystem::System1904;
    exact(c(&e, "DATE", vec![n(2024.0), n(1.0), n(15.0)]), 45306.0 - 1462.0);
    exact(c(&e, "DATE", vec![n(1904.0), n(1.0), n(1.0)]), 0.0);
    error(c(&e, "DATE", vec![n(1903.0), n(12.0), n(31.0)]), Num);
    exact(c(&e, "YEAR", vec![n(0.0)]), 1904.0);
    exact(c(&e, "YEAR", vec![n(45306.0 - 1462.0)]), 2024.0);
    exact(c(&e, "EOMONTH", vec![n(0.0), n(0.0)]), 30.0);
    exact(c(&e, "WEEKDAY", vec![n(0.0)]), 6.0);
    exact(c(&e, "VALUE", vec![t("2024/01/15")]), 45306.0 - 1462.0);
    text(c(&e, "TEXT", vec![n(0.0), t("yyyy-mm-dd")]), "1904-01-01");
    exact(c(&e, "TODAY", vec![]), 46318.0 - 1462.0);
}

#[test]
fn value_datevalue_timevalue() {
    let e = host();
    // Excel: VALUE("1 234.5") 1234.5, VALUE("R 12") 12, VALUE("12:30") 0.520833..., VALUE("1/2")
    // 46024, DATEVALUE("2024/01/05 12:30") 45296, TIMEVALUE("25:00") 0.041666666666666741.
    exact(c(&e, "VALUE", vec![t("1 234.5")]), 1234.5);
    exact(c(&e, "VALUE", vec![t("R 12")]), 12.0);
    exact(c(&e, "VALUE", vec![t("12:30")]), 0.52083333333333337);
    exact(c(&e, "VALUE", vec![t("1/2")]), 46024.0);
    error(c(&e, "VALUE", vec![t("1,234")]), Value);
    error(c(&e, "VALUE", vec![b(true)]), Value);
    exact(c(&e, "VALUE", vec![n(5.0)]), 5.0);
    exact(c(&e, "DATEVALUE", vec![t("2024/01/05 12:30")]), 45296.0);
    exact(c(&e, "DATEVALUE", vec![t("12:30")]), 0.0);
    error(c(&e, "DATEVALUE", vec![t("12")]), Value);
    error(c(&e, "DATEVALUE", vec![n(45000.0)]), Value);
    exact(c(&e, "TIMEVALUE", vec![t("25:00")]), 0.041666666666666741);
    exact(c(&e, "TIMEVALUE", vec![t("2024/01/05 12:30")]), 0.52083333333575865);
    // Number text in arithmetic reads the same way once the environment holds a profile.
    exact(c(&e, "SUM", vec![t("1 jan 2020"), n(1.0)]), 43832.0);
    exact(c(&e, "NUMBERVALUE", vec![t("1 234.5")]), 1234.5);
    exact(c(&e, "NUMBERVALUE", vec![t("")]), 0.0);
    exact(c(&e, "NUMBERVALUE", vec![t("1,5"), t(","), t(".")]), 1.5);
}

#[test]
fn text_fixed_dollar() {
    let e = host();
    text(c(&e, "TEXT", vec![n(1234.5), t("#,##0.00")]), "1,234.50");
    text(c(&e, "TEXT", vec![n(1234567.891), t("#,##0")]), "1234,568");
    text(c(&e, "TEXT", vec![n(1.0 / 3.0), t("General")]), "0.333333333");
    text(c(&e, "TEXT", vec![n(45000.0), t("dd mmm yyyy")]), "15 Mar 2023");
    text(c(&e, "TEXT", vec![t("abc"), t("0.00")]), "abc");
    text(c(&e, "TEXT", vec![t("12"), t("0.00")]), "12.00");
    text(c(&e, "TEXT", vec![b(true), t("0")]), "TRUE");
    error(c(&e, "TEXT", vec![n(-1.0), t("yyyy")]), Value);
    refused(c(&e, "TEXT", vec![n(5.0), t("# ?/?")]));
    text(c(&e, "FIXED", vec![n(1234.567)]), "1 234.57");
    text(c(&e, "FIXED", vec![n(1234.567), n(1.0), b(true)]), "1234.6");
    text(c(&e, "DOLLAR", vec![n(1234.567)]), "R1 234.57");
    text(c(&e, "DOLLAR", vec![n(-1234.567)]), "-R1 234.57");
}

#[test]
fn delimited_split_regex_and_conversions() {
    let e = host();
    text(c(&e, "TEXTBEFORE", vec![t("a-b-c"), t("-")]), "a");
    text(c(&e, "TEXTAFTER", vec![t("a-b-c"), t("-"), n(-1.0)]), "c");
    text(c(&e, "TEXTBEFORE", vec![t("a-b-c"), t("-"), n(-1.0), n(0.0), n(1.0)]), "a-b");
    error(c(&e, "TEXTAFTER", vec![t("a-b-c"), t("-"), n(-4.0), n(0.0), n(1.0)]), NA);
    text(c(&e, "TEXTBEFORE", vec![t("a-b-c"), t("x"), missing(), missing(), missing(), t("none")]), "none");
    text(c(&e, "TEXTSPLIT", vec![t("ab"), t("-")]), "ab");
    let (rows, cols, _) = {
        let value = c(&e, "TEXTSPLIT", vec![t("1,2;3"), t(","), t(";")]).unwrap();
        let array = value.as_array().expect("an array").clone();
        let shape = array.shape();
        (shape.rows, shape.cols, ())
    };
    assert_eq!((rows, cols), (2, 2));
    assert_eq!(c(&e, "REGEXTEST", vec![t("ab12"), t("[0-9]")]).unwrap().as_logical(), Some(true));
    text(c(&e, "REGEXEXTRACT", vec![t("ab12"), t("[0-9]+")]), "12");
    text(c(&e, "REGEXREPLACE", vec![t("ab12"), t("[0-9]"), t("x")]), "abxx");
    refused(c(&e, "REGEXTEST", vec![t("ab"), t("(a|b)")]));
    text(c(&e, "VALUETOTEXT", vec![t("a\"b"), n(1.0)]), "\"a\"\"b\"");
    text(c(&e, "ARRAYTOTEXT", vec![arr_values(vec![vec![n(1.0), t("a")], vec![b(true), e_na()]]), n(1.0)]), "{1,\"a\";TRUE,#N/A}");
    text(c(&e, "ARRAYTOTEXT", vec![arr_values(vec![vec![n(1.0), t("a")], vec![b(true), e_na()]])]), "1, a, TRUE, #N/A");
    text(c(&e, "ASC", vec![t("ＡＢＣ")]), "ＡＢＣ");
    exact(c(&e, "HYPERLINK", vec![t("x"), n(5.0)]), 5.0);
    text(c(&e, "HYPERLINK", vec![t("https://example.com")]), "https://example.com");
    refused(c(&e, "ENCODEURL", vec![t("a b")]));
    // Excel: BAHTTEXT of these numbers on this machine, read ad hoc.
    for (number, expected) in [
        (0.0, "\u{0e28}\u{0e39}\u{0e19}\u{0e22}\u{0e4c}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}"),
        (1.0, "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}"),
        (11.0, "\u{0e2a}\u{0e34}\u{0e1a}\u{0e40}\u{0e2d}\u{0e47}\u{0e14}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}"),
        (21.0, "\u{0e22}\u{0e35}\u{0e48}\u{0e2a}\u{0e34}\u{0e1a}\u{0e40}\u{0e2d}\u{0e47}\u{0e14}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}"),
        (101.0, "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e23}\u{0e49}\u{0e2d}\u{0e22}\u{0e40}\u{0e2d}\u{0e47}\u{0e14}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}"),
        (1234567.89, "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e25}\u{0e49}\u{0e32}\u{0e19}\u{0e2a}\u{0e2d}\u{0e07}\u{0e41}\u{0e2a}\u{0e19}\u{0e2a}\u{0e32}\u{0e21}\u{0e2b}\u{0e21}\u{0e37}\u{0e48}\u{0e19}\u{0e2a}\u{0e35}\u{0e48}\u{0e1e}\u{0e31}\u{0e19}\u{0e2b}\u{0e49}\u{0e32}\u{0e23}\u{0e49}\u{0e2d}\u{0e22}\u{0e2b}\u{0e01}\u{0e2a}\u{0e34}\u{0e1a}\u{0e40}\u{0e08}\u{0e47}\u{0e14}\u{0e1a}\u{0e32}\u{0e17}\u{0e41}\u{0e1b}\u{0e14}\u{0e2a}\u{0e34}\u{0e1a}\u{0e40}\u{0e01}\u{0e49}\u{0e32}\u{0e2a}\u{0e15}\u{0e32}\u{0e07}\u{0e04}\u{0e4c}"),
        (1000001.0, "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e25}\u{0e49}\u{0e32}\u{0e19}\u{0e40}\u{0e2d}\u{0e47}\u{0e14}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}"),
        (0.5, "\u{0e2b}\u{0e49}\u{0e32}\u{0e2a}\u{0e34}\u{0e1a}\u{0e2a}\u{0e15}\u{0e32}\u{0e07}\u{0e04}\u{0e4c}"),
        (0.01, "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e2a}\u{0e15}\u{0e32}\u{0e07}\u{0e04}\u{0e4c}"),
        (-12.25, "\u{0e25}\u{0e1a}\u{0e2a}\u{0e34}\u{0e1a}\u{0e2a}\u{0e2d}\u{0e07}\u{0e1a}\u{0e32}\u{0e17}\u{0e22}\u{0e35}\u{0e48}\u{0e2a}\u{0e34}\u{0e1a}\u{0e2b}\u{0e49}\u{0e32}\u{0e2a}\u{0e15}\u{0e32}\u{0e07}\u{0e04}\u{0e4c}"),
        (10.0, "\u{0e2a}\u{0e34}\u{0e1a}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}"),
        (20.0, "\u{0e22}\u{0e35}\u{0e48}\u{0e2a}\u{0e34}\u{0e1a}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}"),
        (1000000.0, "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e25}\u{0e49}\u{0e32}\u{0e19}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}"),
        (11000000.0, "\u{0e2a}\u{0e34}\u{0e1a}\u{0e40}\u{0e2d}\u{0e47}\u{0e14}\u{0e25}\u{0e49}\u{0e32}\u{0e19}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}"),
        (1.005, "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e1a}\u{0e32}\u{0e17}\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e2a}\u{0e15}\u{0e32}\u{0e07}\u{0e04}\u{0e4c}"),
        (100000000000.0, "\u{0e2b}\u{0e19}\u{0e36}\u{0e48}\u{0e07}\u{0e41}\u{0e2a}\u{0e19}\u{0e25}\u{0e49}\u{0e32}\u{0e19}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}"),
        (2.999, "\u{0e2a}\u{0e32}\u{0e21}\u{0e1a}\u{0e32}\u{0e17}\u{0e16}\u{0e49}\u{0e27}\u{0e19}"),
    ] {
        text(call("BAHTTEXT", vec![n(number)]), expected);
    }
}

fn e_na() -> CalcValue {
    e(NA)
}

#[test]
fn address_formats_references() {
    text(call("ADDRESS", vec![n(2.0), n(3.0)]), "$C$2");
    text(call("ADDRESS", vec![n(2.0), n(3.0), n(2.0)]), "C$2");
    text(call("ADDRESS", vec![n(2.0), n(3.0), n(2.0), b(false)]), "R2C[3]");
    text(call("ADDRESS", vec![n(2.0), n(3.0), n(1.0), b(false), t("[Book1]Sheet1")]), "[Book1]Sheet1!R2C3");
    text(call("ADDRESS", vec![n(2.0), n(3.0), n(1.0), b(false), t("EXCEL SHEET")]), "'EXCEL SHEET'!R2C3");
    error(call("ADDRESS", vec![n(0.0), n(3.0)]), Value);
}

struct Host {
    sheet: SheetId,
    formulas: Vec<(CellKey, String)>,
}

impl HostFacts for Host {
    fn caller(&mut self) -> Option<CellKey> {
        CellKey::new(self.sheet, 1, 1).ok()
    }
    fn is_formula(&mut self, cell: CellKey) -> Option<bool> {
        Some(self.formulas.iter().any(|(at, _)| *at == cell))
    }
    fn formula_text(&mut self, cell: CellKey) -> Option<Option<String>> {
        Some(self.formulas.iter().find(|(at, _)| *at == cell).map(|(_, f)| f.clone()))
    }
    fn sheet_index(&mut self, sheet: SheetId) -> Option<u32> {
        (sheet == self.sheet).then_some(1)
    }
    fn sheet_count(&mut self) -> Option<u32> {
        Some(3)
    }
}

#[test]
fn host_facts() {
    let e = host();
    let mut sheet = Sheet::new();
    sheet.column(1, 2, vec![t("x"), n(5.0)]);
    let mut facts = Host {
        sheet: sheet.id,
        formulas: vec![(sheet.key(2, 2), "=2+3".to_owned())],
    };
    let b1 = sheet.area(1, 2, 1, 2);
    let b2 = sheet.area(2, 2, 2, 2);
    let b3 = sheet.area(3, 2, 3, 2);
    assert_eq!(call_in(&e, None, Some(&mut facts), "ISFORMULA", vec![b2.clone()]).unwrap().as_logical(), Some(true));
    assert_eq!(call_in(&e, None, Some(&mut facts), "ISFORMULA", vec![b1.clone()]).unwrap().as_logical(), Some(false));
    refused(call("ISFORMULA", vec![b1.clone()]));
    text(call_in(&e, None, Some(&mut facts), "FORMULATEXT", vec![b2.clone()]), "=2+3");
    error(call_in(&e, None, Some(&mut facts), "FORMULATEXT", vec![b1.clone()]), NA);
    exact(call_in(&e, None, Some(&mut facts), "SHEET", vec![]), 1.0);
    exact(call_in(&e, None, Some(&mut facts), "SHEETS", vec![]), 3.0);
    exact(call("SHEETS", vec![b1.clone()]), 1.0);
    text(call_in(&e, None, Some(&mut facts), "CELL", vec![t("address"), b2.clone()]), "$B$2");
    exact(call("CELL", vec![t("row"), b2.clone()]), 2.0);
    exact(call("CELL", vec![t("col"), b2.clone()]), 2.0);
    text(call_on(&mut sheet, "CELL", vec![t("type"), b1.clone()]), "l");
    text(call_on(&mut sheet, "CELL", vec![t("type"), b3.clone()]), "b");
    exact(call_on(&mut sheet, "CELL", vec![t("contents"), b2.clone()]), 5.0);
    error(call("CELL", vec![t("bogus"), b2.clone()]), Value);
    refused(call("CELL", vec![t("width"), b2]));
    refused(call("INFO", vec![t("system")]));
    error(call("INFO", vec![t("bogus")]), Value);
    let col = sheet.area(1, 2, 3, 2);
    let (rows, cols, _) = {
        let value = call_on(&mut sheet, "TRANSPOSE", vec![col]).unwrap();
        let shape = value.as_array().unwrap().shape();
        (shape.rows, shape.cols, ())
    };
    assert_eq!((rows, cols), (1, 3));
}

/// Microsoft's DSUM page example: a tree database and criteria.
#[test]
fn database_functions() {
    let mut sheet = Sheet::new();
    // A1:E7 database, A9:F11 criteria.
    let rows: [[CalcValue; 5]; 7] = [
        [t("Tree"), t("Height"), t("Age"), t("Yield"), t("Profit")],
        [t("Apple"), n(18.0), n(20.0), n(14.0), n(105.0)],
        [t("Pear"), n(12.0), n(12.0), n(10.0), n(96.0)],
        [t("Cherry"), n(13.0), n(14.0), n(9.0), n(105.0)],
        [t("Apple"), n(14.0), n(15.0), n(10.0), n(75.0)],
        [t("Pear"), n(9.0), n(8.0), n(8.0), n(76.8)],
        [t("Apple"), n(8.0), n(9.0), n(6.0), n(45.0)],
    ];
    for (r, row) in rows.iter().enumerate() {
        for (col, value) in row.iter().enumerate() {
            sheet.cells.insert((r as u32 + 1, col as u32 + 1), value.clone());
        }
    }
    let criteria: [[CalcValue; 2]; 3] = [
        [t("Tree"), t("Height")],
        [t("=Apple"), t(">10")],
        [t("Pear"), CalcValue::empty()],
    ];
    for (r, row) in criteria.iter().enumerate() {
        for (col, value) in row.iter().enumerate() {
            if !value.is_empty() {
                sheet.cells.insert((r as u32 + 9, col as u32 + 1), value.clone());
            }
        }
    }
    let db = sheet.area(1, 1, 7, 5);
    let crit = sheet.area(9, 1, 11, 2);
    let apple_only = sheet.area(9, 1, 10, 2);
    // Apple taller than 10 (two) or any Pear (two).
    exact(call_on(&mut sheet, "DCOUNT", vec![db.clone(), t("Age"), crit.clone()]), 4.0);
    close(call_on(&mut sheet, "DSUM", vec![db.clone(), t("Profit"), crit.clone()]), 352.8, 1e-15);
    exact(call_on(&mut sheet, "DMAX", vec![db.clone(), t("Profit"), crit.clone()]), 105.0);
    exact(call_on(&mut sheet, "DMIN", vec![db.clone(), t("Profit"), crit.clone()]), 75.0);
    exact(call_on(&mut sheet, "DAVERAGE", vec![db.clone(), n(3.0), apple_only.clone()]), 17.5);
    exact(call_on(&mut sheet, "DSUM", vec![db.clone(), t("Profit"), apple_only.clone()]), 180.0);
    exact(call_on(&mut sheet, "DPRODUCT", vec![db.clone(), t("Yield"), apple_only.clone()]), 140.0);
    error(call_on(&mut sheet, "DGET", vec![db.clone(), t("Yield"), apple_only.clone()]), Num);
    exact(call_on(&mut sheet, "DCOUNTA", vec![db.clone(), t("Profit"), crit.clone()]), 4.0);
    close(call_on(&mut sheet, "DSTDEV", vec![db.clone(), t("Yield"), crit.clone()]), (19.0f64 / 3.0).sqrt(), 1e-12);
    error(call_on(&mut sheet, "DSUM", vec![db, t("Nope"), crit]), Value);
}
