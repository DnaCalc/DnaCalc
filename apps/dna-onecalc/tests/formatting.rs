//! DNA OneCalc's Formatting and Conditional Formatting (dc-0f1k): the rules the panel authors give
//! a result the look Excel gave the same rules in calc-host's fixture (dc-6yw: conditional-formats.xlsx
//! authored in Excel by make-cf.ps1, which recorded each cell's DisplayFormat in cf-excel.txt). The
//! fixture is DNA Calc's own, read here from calc-host's tests; the evaluation is calc-doc's `look`
//! module, which calc-host paints a sheet with.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_doc::look::LookColor;
use calc_func::value::{CalcArray, CalcValue, ExcelText};
use dna_onecalc::Session;
use dna_onecalc::cf::{self, CellStyle, CfRuleSpec};
use dna_onecalc::shown::{self, Authored, ViewWire};

const EXCEL: &str = include_str!("../../../crates/calc-host/tests/fixtures/dc_6yw/cf-excel.txt");

/// The ten numbers of columns A to K and N, the words of L, and M's blanks, as make-cf.ps1 wrote them.
const NUMBERS: [f64; 10] = [4.0, 9.0, 1.0, 7.0, 3.0, 10.0, 6.0, 2.0, 8.0, 5.0];
const WORDS: [&str; 10] = [
    "banana", "apple", "cherry", "mango", "kiwi", "orange", "grape", "plum", "lemon", "pear",
];

fn column(values: Vec<CalcValue>) -> CalcArray {
    CalcArray::from_rows(values.into_iter().map(|value| vec![value]).collect()).expect("a column")
}

fn numbers(f: impl Fn(f64) -> f64) -> Vec<CalcValue> {
    NUMBERS.iter().map(|&n| CalcValue::number(f(n))).collect()
}

fn rule(kind: &str, set: impl FnOnce(&mut CfRuleSpec)) -> CfRuleSpec {
    let mut spec = CfRuleSpec::new(kind);
    spec.fill.clear();
    spec.font_color.clear();
    set(&mut spec);
    spec
}

/// What Excel showed for column `letter`: per row, the fill, the font colour, bold, italic and text.
fn excel(letter: &str) -> Vec<(Option<String>, String, bool, bool, String)> {
    let mut rows = vec![None; 10];
    for line in EXCEL.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [address, fill, font, bold, italic, text] = fields[..] else {
            panic!("a line of six fields: {line}");
        };
        if let Some(row) = address
            .strip_prefix(letter)
            .and_then(|r| r.parse::<usize>().ok())
        {
            rows[row - 1] = Some((
                (fill != "-").then(|| fill.to_owned()),
                font.to_owned(),
                bold == "1",
                italic == "1",
                text.to_owned(),
            ));
        }
    }
    rows.into_iter().map(|row| row.expect("ten rows")).collect()
}

fn rgb(color: &Option<LookColor>) -> Option<String> {
    match color {
        Some(LookColor::Rgb(text)) => Some(text.clone()),
        _ => None,
    }
}

/// Runs `rules` over `array` and compares column `which` of the result (0-based) with Excel's column.
fn compare(
    letter: &str,
    rules: Vec<CfRuleSpec>,
    array: CalcArray,
    which: usize,
    check_format: bool,
) -> usize {
    let shape = array.shape();
    let outcome = cf::evaluate(&rules, &array, shape.rows, shape.cols, None);
    assert_eq!(outcome.refused, 0, "{letter}: {:?}", outcome.notes);
    let mut compared = 0;
    for (row, (fill, font, bold, italic, text)) in excel(letter).into_iter().enumerate() {
        let look = &outcome.looks[row][which];
        assert_eq!(rgb(&look.fill), fill, "{letter}{} fill", row + 1);
        assert_eq!(
            rgb(&look.font_color).unwrap_or_else(|| "#000000".to_owned()),
            font,
            "{letter}{} font colour",
            row + 1
        );
        assert_eq!(look.bold.unwrap_or(false), bold, "{letter}{} bold", row + 1);
        assert_eq!(
            look.italic.unwrap_or(false),
            italic,
            "{letter}{} italic",
            row + 1
        );
        if check_format {
            assert_eq!(
                look.number_format.as_deref() == Some("0.0%"),
                text.ends_with('%'),
                "{letter}{} format",
                row + 1
            );
        }
        compared += 1;
    }
    compared
}

/// Every column of A1:N10 whose rule needs no other column: the same rules, authored as the panel
/// authors them, over the same values, give Excel's fill, font colour, bold and italic in all
/// hundred and forty cells (D reads column A, so it is run over A and D together).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_0f1k__rules_authored_in_onecalc_look_as_excel_shows_calc_hosts_fixture() {
    let mut compared = 0;
    compared += compare(
        "A",
        vec![rule("cellIs", |s| {
            s.operand1 = "5".into();
            s.fill = "#ffc7ce".into();
            s.font_color = "#9c0006".into();
            s.stop = true;
        })],
        column(numbers(|n| n)),
        0,
        false,
    );
    compared += compare(
        "B",
        vec![rule("cellIs", |s| {
            s.operator = "between".into();
            s.operand1 = "3".into();
            s.operand2 = "6".into();
            s.fill = "#c6efce".into();
            s.bold = true;
            s.stop = true;
        })],
        column(numbers(|n| n)),
        0,
        false,
    );
    compared += compare(
        "C",
        vec![rule("expression", |s| {
            s.operand1 = "MOD(ROW(),2)=0".into();
            s.fill = "#ffeb9c".into();
            s.stop = true;
        })],
        column(numbers(|n| n)),
        0,
        false,
    );
    let a_and_d = CalcArray::from_rows(
        NUMBERS
            .iter()
            .map(|&n| vec![CalcValue::number(n), CalcValue::number(n * 2.0)])
            .collect(),
    )
    .unwrap();
    compared += compare(
        "D",
        vec![rule("expression", |s| {
            s.operand1 = "$A1>6".into();
            s.font_color = "#0000ff".into();
            s.italic = true;
            s.stop = true;
        })],
        a_and_d,
        1,
        false,
    );
    compared += compare(
        "E",
        vec![rule("colorScale", |_| {})],
        column(numbers(|n| n)),
        0,
        false,
    );
    compared += compare(
        "F",
        vec![rule("colorScale", |s| {
            s.low = "#ff7128".into();
            s.mid.clear();
            s.high = "#ffef9c".into();
        })],
        column(numbers(|n| n)),
        0,
        false,
    );
    compared += compare(
        "G",
        vec![rule("dataBar", |_| {})],
        column(numbers(|n| n)),
        0,
        false,
    );
    compared += compare(
        "H",
        vec![rule("iconSet", |_| {})],
        column(numbers(|n| n)),
        0,
        false,
    );
    compared += compare(
        "I",
        vec![rule("rank", |s| {
            s.rank = 3;
            s.fill = "#ffc000".into();
        })],
        column(numbers(|n| n)),
        0,
        false,
    );
    compared += compare(
        "J",
        vec![rule("average", |s| {
            s.fill = "#83cceb".into();
            s.stop = true;
        })],
        column(numbers(|n| n)),
        0,
        false,
    );
    compared += compare(
        "K",
        vec![rule("duplicates", |s| {
            s.font_color = "#7030a0".into();
        })],
        column(numbers(|n| (n as i64 % 4) as f64)),
        0,
        false,
    );
    let words = WORDS
        .iter()
        .map(|w| CalcValue::text(ExcelText::from_interop_assignment(w)))
        .collect();
    compared += compare(
        "L",
        vec![rule("text", |s| {
            s.operand1 = "an".into();
            s.fill = "#d9d9d9".into();
            s.bold = true;
            s.stop = true;
        })],
        column(words),
        0,
        false,
    );
    let blanks = (0..10)
        .map(|i| {
            if i % 3 == 0 {
                CalcValue::empty()
            } else {
                CalcValue::number(i as f64 + 1.0)
            }
        })
        .collect();
    compared += compare(
        "M",
        vec![rule("blanks", |s| {
            s.fill = "#ddebf7".into();
            s.stop = true;
        })],
        column(blanks),
        0,
        false,
    );
    compared += compare(
        "N",
        vec![
            rule("cellIs", |s| {
                s.operand1 = "7".into();
                s.fill = "#ff0000".into();
                s.stop = true;
            }),
            rule("cellIs", |s| {
                s.operator = "lessThan".into();
                s.operand1 = "9".into();
                s.fill = "#00b050".into();
                s.number_format = "0.0%".into();
                s.stop = true;
            }),
        ],
        column(numbers(|n| n)),
        0,
        true,
    );
    assert_eq!(compared, 140);

    // Column H's icons and G's bars, as cf-excel.pdf shows them (calc-host's dc_6yw case).
    let array = column(numbers(|n| n));
    let outcome = cf::evaluate(&[rule("iconSet", |_| {})], &array, 10, 1, None);
    let icons: Vec<usize> = outcome
        .looks
        .iter()
        .map(|row| row[0].icon.as_ref().expect("an icon").index)
        .collect();
    assert_eq!(icons, vec![1, 2, 0, 1, 0, 2, 1, 0, 2, 1]);
    let outcome = cf::evaluate(&[rule("dataBar", |_| {})], &array, 10, 1, None);
    let bar = |row: usize| outcome.looks[row][0].bar.clone().expect("a bar");
    assert_eq!(bar(2).fraction, 0.0, "1, the lowest");
    assert_eq!(bar(5).fraction, 1.0, "10, the highest");
    assert!((bar(0).fraction - 3.0 / 9.0).abs() < 1e-12, "4");
    assert_eq!(bar(0).color, Some(LookColor::Rgb("#638ec6".to_owned())));
}

fn authored(formula: &str, rules: Vec<CfRuleSpec>, format: &str) -> Authored {
    Authored {
        formula: formula.to_owned(),
        cf_rules: rules,
        number_format: format.to_owned(),
        ..Authored::default()
    }
}

/// The route the application runs: a formula, the number format and the rules authored in the panels,
/// answered as the page draws them. An array result's cells carry their own look and the text a
/// rule's number format gives; a scalar result is a range of one cell; the Formatting panel's choices
/// are the result's style and its effective display lists them.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_0f1k__the_session_answers_an_array_with_each_cells_look_and_a_scalar_as_one_cell() {
    let mut session = Session::new("Sheet1").unwrap();
    let rules = vec![
        rule("cellIs", |s| {
            s.operand1 = "7".into();
            s.fill = "#ff0000".into();
            s.stop = true;
        }),
        rule("cellIs", |s| {
            s.operator = "lessThan".into();
            s.operand1 = "9".into();
            s.fill = "#00b050".into();
            s.number_format = "0.0%".into();
        }),
    ];
    let shown = shown::evaluate(&mut session, &authored("={4;9;1;7;3;10}", rules, "General"));
    let ViewWire::Array { cells, .. } = &shown.view else {
        panic!("{:?}", shown.view)
    };
    let fills: Vec<String> = cells
        .iter()
        .map(|row| {
            row[0]
                .look
                .as_ref()
                .map(|l| l.fill.clone())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(
        fills,
        [
            "#00b050", "#ff0000", "#00b050", "#00b050", "#00b050", "#ff0000"
        ]
    );
    let texts: Vec<&str> = cells.iter().map(|row| row[0].text.as_str()).collect();
    assert_eq!(texts, ["400.0%", "9", "100.0%", "700.0%", "300.0%", "10"]);
    assert!(
        shown
            .effective
            .iter()
            .any(|line| line.contains("holds for")),
        "{:?}",
        shown.effective
    );

    // A scalar is a range of one cell: a colour scale of one value, and a rule on its value.
    let rules = vec![rule("cellIs", |s| {
        s.operand1 = "3".into();
        s.fill = "#ffc7ce".into();
        s.bold = true;
    })];
    let shown = shown::evaluate(&mut session, &authored("=1+4", rules, "General"));
    let ViewWire::Value { look, text, .. } = &shown.view else {
        panic!("{:?}", shown.view)
    };
    assert_eq!(text, "5");
    let look = look.as_ref().expect("the rule holds");
    assert_eq!((look.fill.as_str(), look.bold), ("#ffc7ce", true));
    assert!(
        shown
            .effective
            .iter()
            .any(|line| line.starts_with("Result drawn with")),
        "{:?}",
        shown.effective
    );

    // A number format's colour token is the font colour; the Formatting panel's style is the result's.
    let mut state = authored("=-5", Vec::new(), "0;[Red]-0");
    state.style = CellStyle {
        bold: true,
        fill: "#ffeb9c".into(),
        border_bottom: true,
        ..CellStyle::default()
    };
    let shown = shown::evaluate(&mut session, &state);
    let ViewWire::Value { look, text, .. } = &shown.view else {
        panic!("{:?}", shown.view)
    };
    assert_eq!(text, "-5");
    assert_eq!(look.as_ref().map(|l| l.color.as_str()), Some("#ff0000"));
    assert!(
        shown.style_css.contains("font-weight:700;")
            && shown.style_css.contains("background-color:#ffeb9c;")
    );
    assert!(
        shown
            .effective
            .iter()
            .any(|line| line.starts_with("Font: bold")),
        "{:?}",
        shown.effective
    );
    assert!(
        shown.effective.iter().any(|line| line == "Borders: bottom"),
        "{:?}",
        shown.effective
    );

    // A rule that cannot be built is named and not applied; the result is still calculated.
    let shown = shown::evaluate(
        &mut session,
        &authored("=7", vec![CfRuleSpec::new("cellIs")], "General"),
    );
    assert!(
        shown
            .effective
            .iter()
            .any(|line| line.contains("needs a value")),
        "{:?}",
        shown.effective
    );
    assert!(matches!(&shown.view, ViewWire::Value { text, .. } if text == "7"));
}

/// A long spill is windowed for the scrolling panel: thousands of rows reach it, not the first 32.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_0f1k__a_long_spill_reaches_the_scrolling_panel_in_thousands_of_rows() {
    let mut session = Session::new("Sheet1").unwrap();
    let shown = shown::evaluate(
        &mut session,
        &authored("=SEQUENCE(25000,3)", Vec::new(), "General"),
    );
    let ViewWire::Array {
        total_rows,
        total_cols,
        truncated,
        cells,
    } = &shown.view
    else {
        panic!("{:?}", shown.view)
    };
    assert_eq!((*total_rows, *total_cols, *truncated), (25_000, 3, true));
    assert_eq!(cells.len(), 10_000);
    assert_eq!(cells[9_999][2].text, "30000");
}
