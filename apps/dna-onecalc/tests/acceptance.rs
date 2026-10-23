//! Acceptance cases owned by the dna-onecalc package, each registered by its exact name in
//! `apps/dna-onecalc/acceptance.json`. Every case is `#[test]` natively and `#[wasm_bindgen_test]`
//! on wasm32, with no browser or Node.js configuration of its own: `cargo xtask accept` forces the
//! browser with `WASM_BINDGEN_USE_BROWSER`.
//!
//! `onecalc_m1_1__reference_without_engine` drives the OneCalc controller (`dna_onecalc::Session`)
//! through its public API on every target, and on wasm32 also mounts the browser surface in an
//! element of the test page and drives it through dispatched DOM events, reading what it shows back
//! from the DOM. Both parts run the same examples (`EXAMPLES`): typed inputs, a formula, and the
//! result, diagnostics and reads expected for them. Expected cells are decoded by hand from the
//! examples' A1 text; expected values are derived by hand from Excel's documented operators and
//! functions and are not compared with Excel here (cfunc-m1.2's oracle capture is that evidence).
//! Two expectations are the implementation's current answer, not Excel's value, and are labelled
//! so: `=A1*1E-300*1E-10` (a product below the normal range) is refused by calc-func as not modelled, and a cell with no input stops the
//! calculation where Excel would read a blank.
//!
//! `onecalc_m1_1__dependency_gate` reads `cargo metadata` itself, for every triple the
//! architecture check resolves, and walks dna-onecalc's resolved graph (its own normal, build and
//! dev dependencies, then their normal and build dependencies) with all features. Native only: it
//! starts cargo.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_fml::CompileError;
use calc_fml::EvalFault;
use calc_fml::reference::BindError;
use calc_fml::syntax::{SyntaxDiagnosticKind, TextSpan};
use calc_func::value::{CalcValue, CellKey, GridArea, SheetId, WorksheetErrorCode};
use calc_func::{FunctionFault, Notation, catalogue};
use dna_onecalc::{
    Calculation, Diagnostic, InputError, InputValue, Read, ReadRun, RunCells, Session, SheetError,
    display,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_test::wasm_bindgen_test;

/// Prints a line the acceptance runner keeps: on stdout natively, on the browser console (which
/// wasm-bindgen-test-runner forwards under `--nocapture`) on wasm32.
macro_rules! note {
    ($($arg:tt)*) => {{
        let line = format!($($arg)*);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_test::console_log!("{}", line);
        #[cfg(not(target_arch = "wasm32"))]
        println!("{line}");
    }};
}

// ---------------------------------------------------------------------------------------------
// The examples both parts run.

/// The kinds of typed input, as the surface's type selector names them.
#[derive(Clone, Copy, Debug)]
enum Kind {
    Number,
    Text,
    Logical,
    Error,
    Blank,
}

impl Kind {
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn selector(self) -> &'static str {
        match self {
            Kind::Number => "number",
            Kind::Text => "text",
            Kind::Logical => "logical",
            Kind::Error => "error",
            Kind::Blank => "blank",
        }
    }
}

/// The input the test means by `kind` and `entry`, built with `InputValue`'s constructors.
fn input(kind: Kind, entry: &str) -> InputValue {
    match kind {
        Kind::Number => InputValue::number(entry.parse().expect("a number entry")).expect("held"),
        Kind::Text => InputValue::text(entry).expect("short text"),
        Kind::Logical => InputValue::logical(entry == "TRUE"),
        Kind::Error => {
            assert_eq!(entry, "#N/A", "the examples use one error");
            InputValue::error(WorksheetErrorCode::NA)
        }
        Kind::Blank => InputValue::blank(),
    }
}

/// An expected line of text: exactly this, or starting with this where the rest is the parser's
/// own message, which the test does not derive.
#[derive(Clone, Copy, Debug)]
enum Line {
    Is(&'static str),
    Starts(&'static str),
}

impl Line {
    fn matches(self, text: &str) -> bool {
        match self {
            Line::Is(expected) => text == expected,
            Line::Starts(prefix) => text.starts_with(prefix),
        }
    }
}

/// Inputs and a formula, entered in Sheet1!C1 of a session with sheets Sheet1 and Sheet2, and
/// what the session shows for them.
struct Example {
    name: &'static str,
    inputs: &'static [(&'static str, Kind, &'static str)],
    formula: &'static str,
    result: &'static str,
    diagnostics: &'static [Line],
    reads: &'static [&'static str],
}

const FORMULA_CELL: &str = "C1";

const TWO_CELLS: &[(&str, Kind, &str)] = &[
    ("A1", Kind::Number, "1.5"),
    ("Sheet2!B2", Kind::Number, "4"),
];
const ONE_CELL: &[(&str, Kind, &str)] = &[("A1", Kind::Number, "1.5")];
const COLUMN: &[(&str, Kind, &str)] = &[
    ("A1", Kind::Number, "1.5"),
    ("A2", Kind::Blank, ""),
    ("A3", Kind::Number, "2"),
];

const READ_A1: &str = "Sheet1!A1: Sheet1!A1 = number 1.5";

/// The examples, in the order both parts run them. The first three are one session edited in
/// turn: two supplied cells (one on Sheet2), one of them changed, then removed.
const EXAMPLES: &[Example] = &[
    Example {
        name: "two supplied cells, one on another sheet",
        inputs: TWO_CELLS,
        formula: "=A1*Sheet2!B2+1",
        // 1.5 * 4 + 1
        result: "number 7",
        diagnostics: &[],
        reads: &[READ_A1, "Sheet2!B2: Sheet2!B2 = number 4"],
    },
    Example {
        name: "the Sheet2 input changed",
        inputs: &[
            ("A1", Kind::Number, "1.5"),
            ("Sheet2!B2", Kind::Number, "10"),
        ],
        formula: "=A1*Sheet2!B2+1",
        // 1.5 * 10 + 1
        result: "number 16",
        diagnostics: &[],
        reads: &[READ_A1, "Sheet2!B2: Sheet2!B2 = number 10"],
    },
    Example {
        name: "the Sheet2 input removed (the implementation's answer: no value is made up)",
        inputs: ONE_CELL,
        formula: "=A1*Sheet2!B2+1",
        result: "not calculated",
        diagnostics: &[Line::Is("missing input: Sheet2!B2 has no input")],
        reads: &[READ_A1, "Sheet2!B2: Sheet2!B2 no input"],
    },
    Example {
        name: "a text input",
        inputs: &[("Sheet2!A1", Kind::Text, "a")],
        formula: "=Sheet2!A1&\"x\"",
        result: "text \"ax\"",
        diagnostics: &[],
        reads: &["Sheet2!A1: Sheet2!A1 = text \"a\""],
    },
    Example {
        name: "a logical input",
        inputs: &[("B1", Kind::Logical, "TRUE")],
        formula: "=IF(B1,\"yes\",\"no\")",
        result: "text \"yes\"",
        diagnostics: &[],
        reads: &["Sheet1!B1: Sheet1!B1 = logical TRUE"],
    },
    Example {
        name: "an error input",
        inputs: &[("A1", Kind::Number, "1.5"), ("B3", Kind::Error, "#N/A")],
        formula: "=A1*B3",
        // An arithmetic operand's error is the result.
        result: "error #N/A",
        diagnostics: &[],
        reads: &[READ_A1, "Sheet1!B3: Sheet1!B3 = error #N/A"],
    },
    Example {
        name: "a blank input inside a range",
        inputs: COLUMN,
        formula: "=SUM(A1:A3)",
        // SUM skips the blank cell: 1.5 + 2
        result: "number 3.5",
        diagnostics: &[],
        reads: &["Sheet1!A1:A3: Sheet1!A1 = number 1.5; Sheet1!A2 blank; Sheet1!A3 = number 2"],
    },
    Example {
        name: "a range reaching a cell with no input (the implementation's answer)",
        inputs: COLUMN,
        formula: "=SUM(A1:A4)",
        result: "not calculated",
        diagnostics: &[Line::Is("missing input: Sheet1!A4 has no input")],
        reads: &[
            "Sheet1!A1:A4: Sheet1!A1 = number 1.5; Sheet1!A2 blank; Sheet1!A3 = number 2; Sheet1!A4 no input",
        ],
    },
    Example {
        name: "a formula the parser refuses",
        inputs: ONE_CELL,
        formula: "=A1*",
        result: "not calculated",
        // The right operand is missing at the end of the text, offset 4.
        diagnostics: &[Line::Starts("syntax at 4..4: ")],
        reads: &[],
    },
    Example {
        name: "a reference to a sheet the session does not have",
        inputs: ONE_CELL,
        formula: "=Sheet9!A1+1",
        result: "not calculated",
        diagnostics: &[Line::Is(
            "reference at 1..10 refused: there is no sheet named Sheet9",
        )],
        reads: &[],
    },
    Example {
        name: "a call calc-func does not model (the implementation's answer)",
        inputs: ONE_CELL,
        // A product below the normal range (1.5E-310), which calc-func refuses.
        formula: "=A1*1E-300*1E-10",
        result: "not calculated",
        diagnostics: &[Line::Is(
            "function fault in the * operator: Excel's result is not modelled for this call",
        )],
        reads: &[READ_A1],
    },
];

/// Checks the text the session's calculation gives for `example`, as the surface shows it.
fn check_text(session: &Session, calculation: &Calculation, example: &Example) {
    let result = match &calculation.result {
        Some(value) => display::value(value),
        None => "not calculated".to_owned(),
    };
    assert_eq!(result, example.result, "{}: result", example.name);
    let diagnostics: Vec<String> = calculation
        .diagnostics
        .iter()
        .map(|diagnostic| display::diagnostic(session, diagnostic))
        .collect();
    check_lines(
        &diagnostics,
        example.diagnostics,
        example.name,
        "diagnostics",
    );
    let reads: Vec<String> = calculation
        .reads
        .iter()
        .map(|read| display::read(session, read))
        .collect();
    assert_eq!(reads, example.reads, "{}: reads", example.name);
}

fn check_lines(shown: &[String], expected: &[Line], example: &str, what: &str) {
    assert_eq!(
        shown.len(),
        expected.len(),
        "{example}: {what} {shown:?}, expected {expected:?}"
    );
    for (line, expected) in shown.iter().zip(expected) {
        assert!(
            expected.matches(line),
            "{example}: {what} line {line:?}, expected {expected:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The controller part.

fn key(sheet: SheetId, row: u32, column: u32) -> CellKey {
    CellKey::new(sheet, row, column).expect("a cell on the grid")
}

fn one_cell_read(cell: CellKey, cells: RunCells) -> Read {
    Read {
        area: GridArea::cell(cell),
        runs: vec![ReadRun { start: cell, cells }],
    }
}

fn numbers(values: &[f64]) -> RunCells {
    RunCells::Values(values.iter().map(|n| CalcValue::number(*n)).collect())
}

/// Replaces the session's inputs with the example's and calculates its formula.
fn run_example(session: &mut Session, example: &Example) -> Calculation {
    session.clear_inputs();
    for (address, kind, entry) in example.inputs {
        session
            .set_input(address, input(*kind, entry))
            .unwrap_or_else(|e| panic!("{}: input {address}: {e:?}", example.name));
    }
    session.set_formula(example.formula);
    session.calculate()
}

/// The controller part of the reference case. Returns the number of examples it checked.
fn controller_part() -> usize {
    let mut session = Session::new("Sheet1").expect("Sheet1 is a sheet name");
    let s2 = session.add_sheet("Sheet2").expect("Sheet2 is a sheet name");
    let s1 = session
        .sheets()
        .find(|(name, _)| *name == "Sheet1")
        .map(|(_, id)| id)
        .expect("the first sheet");
    assert_eq!(
        session.sheets().map(|(name, _)| name).collect::<Vec<_>>(),
        ["Sheet1", "Sheet2"]
    );
    assert_eq!(
        session.formula_cell(),
        key(s1, 1, 1),
        "a new session's formula is in A1"
    );
    assert_eq!(session.set_formula_cell(FORMULA_CELL), Ok(key(s1, 1, 3)));

    let (a1, a2, a3, a4) = (key(s1, 1, 1), key(s1, 2, 1), key(s1, 3, 1), key(s1, 4, 1));
    let sheet2_b2 = key(s2, 2, 2);

    // Two explicitly supplied cells, one on another sheet: the trace shows exactly the two reads.
    assert_eq!(session.set_input("A1", input(Kind::Number, "1.5")), Ok(a1));
    assert_eq!(
        session.set_input("Sheet2!B2", input(Kind::Number, "4")),
        Ok(sheet2_b2)
    );
    session.set_formula(EXAMPLES[0].formula);
    let first = session.calculate();
    assert_eq!(
        first,
        Calculation {
            result: Some(CalcValue::number(7.0)),
            diagnostics: vec![],
            reads: vec![
                one_cell_read(a1, numbers(&[1.5])),
                one_cell_read(sheet2_b2, numbers(&[4.0])),
            ],
        }
    );
    check_text(&session, &first, &EXAMPLES[0]);

    // The same formula after the Sheet2 input changes reads the new value.
    assert_eq!(
        session.set_input("sheet2!b2", input(Kind::Number, "10")),
        Ok(sheet2_b2)
    );
    let second = session.calculate();
    assert_eq!(second.result, Some(CalcValue::number(16.0)));
    assert_eq!(
        second.reads,
        [
            one_cell_read(a1, numbers(&[1.5])),
            one_cell_read(sheet2_b2, numbers(&[10.0]))
        ]
    );
    check_text(&session, &second, &EXAMPLES[1]);

    // With the Sheet2 input removed, the cell is reported missing, never given a value.
    assert_eq!(
        session.remove_input("Sheet2!B2"),
        Ok(Some(input(Kind::Number, "10")))
    );
    assert_eq!(session.input(sheet2_b2), None);
    let third = session.calculate();
    assert_eq!(
        third,
        Calculation {
            result: None,
            diagnostics: vec![Diagnostic::Eval(EvalFault::MissingInput {
                cell: sheet2_b2
            })],
            reads: vec![
                one_cell_read(a1, numbers(&[1.5])),
                one_cell_read(sheet2_b2, RunCells::Missing(1)),
            ],
        }
    );
    check_text(&session, &third, &EXAMPLES[2]);

    // The rest from a fresh set of inputs each.
    let mut checked = 3;
    for example in &EXAMPLES[3..] {
        let calculation = run_example(&mut session, example);
        check_text(&session, &calculation, example);
        checked += 1;
        match example.formula {
            "=SUM(A1:A3)" => assert_eq!(
                calculation.reads,
                [Read {
                    area: GridArea::new(a1, a3).expect("one sheet"),
                    runs: vec![
                        ReadRun {
                            start: a1,
                            cells: numbers(&[1.5])
                        },
                        ReadRun {
                            start: a2,
                            cells: RunCells::Blank(1)
                        },
                        ReadRun {
                            start: a3,
                            cells: numbers(&[2.0])
                        },
                    ],
                }]
            ),
            "=SUM(A1:A4)" => {
                assert_eq!(
                    calculation.diagnostics,
                    [Diagnostic::Eval(EvalFault::MissingInput { cell: a4 })]
                );
                assert_eq!(
                    calculation.reads,
                    [Read {
                        area: GridArea::new(a1, a4).expect("one sheet"),
                        runs: vec![
                            ReadRun {
                                start: a1,
                                cells: numbers(&[1.5])
                            },
                            ReadRun {
                                start: a2,
                                cells: RunCells::Blank(1)
                            },
                            ReadRun {
                                start: a3,
                                cells: numbers(&[2.0])
                            },
                            ReadRun {
                                start: a4,
                                cells: RunCells::Missing(1)
                            },
                        ],
                    }]
                );
            }
            "=A1*" => {
                let [Diagnostic::Syntax(syntax)] = calculation.diagnostics.as_slice() else {
                    panic!("one syntax diagnostic, got {:?}", calculation.diagnostics);
                };
                assert_eq!(syntax.span, TextSpan::new(4, 0));
                assert_eq!(syntax.kind, SyntaxDiagnosticKind::Message);
            }
            "=Sheet9!A1+1" => assert_eq!(
                calculation.diagnostics,
                [Diagnostic::Compile(CompileError::Reference {
                    span: TextSpan::new(1, 9),
                    error: BindError::UnknownSheet {
                        name: "Sheet9".to_owned()
                    },
                })]
            ),
            "=A1*1E-300*1E-10" => {
                let power = catalogue()
                    .resolve_surface(Notation::Infix, "*")
                    .expect("the catalogue holds *");
                assert_eq!(
                    calculation.diagnostics,
                    [Diagnostic::Eval(EvalFault::Function(
                        FunctionFault::NotImplemented {
                            id: power.id(),
                            argument: None,
                        }
                    ))]
                );
            }
            _ => {}
        }
    }

    // Addresses and values the controller refuses, each typed.
    let blank = InputValue::blank;
    assert_eq!(
        session.set_input("A1:A2", blank()),
        Err(InputError::NotACell {
            address: "A1:A2".to_owned()
        })
    );
    assert_eq!(
        session.set_input("Rate", blank()),
        Err(InputError::NotACell {
            address: "Rate".to_owned()
        })
    );
    assert_eq!(
        session.set_input("Sheet9!A1", blank()),
        Err(InputError::Address {
            address: "Sheet9!A1".to_owned(),
            error: BindError::UnknownSheet {
                name: "Sheet9".to_owned()
            },
        })
    );
    let c1 = key(s1, 1, 3);
    assert_eq!(
        session.set_input("C1", blank()),
        Err(InputError::FormulaCell { cell: c1 })
    );
    assert_eq!(session.input(c1), None);
    assert_eq!(session.set_input("A1", blank()), Ok(a1));
    assert_eq!(
        session.set_formula_cell("A1"),
        Err(InputError::FormulaCell { cell: a1 })
    );
    assert_eq!(
        session.formula_cell(),
        c1,
        "a refused formula cell leaves it in place"
    );
    for refused in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e-310] {
        assert!(
            matches!(
                InputValue::number(refused),
                Err(InputError::NumberNotHeld { value }) if value.to_bits() == refused.to_bits()
            ),
            "{refused} is refused"
        );
    }
    let zero = InputValue::number(-0.0).expect("zero is held");
    assert_eq!(
        zero.value().as_number().map(f64::to_bits),
        Some(0.0f64.to_bits()),
        "-0 is held as +0"
    );
    assert!(InputValue::text(&"a".repeat(32_767)).is_ok());
    assert_eq!(
        InputValue::text(&"a".repeat(32_768)),
        Err(InputError::TextTooLong { units: 32_768 })
    );
    // A character outside the Basic Multilingual Plane is two UTF-16 code units.
    assert_eq!(
        InputValue::text(&"\u{1F600}".repeat(16_384)),
        Err(InputError::TextTooLong { units: 32_768 })
    );

    // Sheet names the controller refuses, each typed.
    assert_eq!(
        session.add_sheet("sheet2"),
        Err(SheetError::Duplicate {
            name: "sheet2".to_owned()
        })
    );
    for invalid in [
        "",
        "Q1:Q2",
        "a/b",
        "[x]",
        "'Quoted",
        "Quoted'",
        "history",
        &"S".repeat(32),
    ] {
        assert_eq!(
            session.add_sheet(invalid),
            Err(SheetError::InvalidName {
                name: invalid.to_owned()
            }),
            "{invalid:?} is refused"
        );
    }
    assert!(session.add_sheet(&"S".repeat(31)).is_ok());
    assert!(session.add_sheet("Q2 Data").is_ok());
    assert_eq!(
        session.set_input("'Q2 Data'!B3", input(Kind::Number, "2")),
        Ok(key(session.sheets().last().expect("a sheet").1, 3, 2))
    );
    checked
}

// ---------------------------------------------------------------------------------------------
// The browser part.

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::{EXAMPLES, Example, FORMULA_CELL, check_lines};
    use dna_onecalc::surface::{Mounted, mount};
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Element, Event, EventInit, HtmlElement, HtmlInputElement, HtmlSelectElement};

    /// The surface mounted in a fresh element of the test page.
    pub struct Page {
        root: HtmlElement,
        _mounted: Mounted,
    }

    /// Lets the surface's reactive updates run: several turns of the event loop.
    pub async fn settle() {
        for _ in 0..5 {
            let promise = js_sys::Promise::new(&mut |resolve, _| {
                web_sys::window()
                    .expect("a window")
                    .set_timeout_with_callback(&resolve)
                    .expect("a timer");
            });
            JsFuture::from(promise).await.expect("the timer fires");
        }
    }

    fn dispatch(target: &Element, kind: &str) {
        let init = EventInit::new();
        init.set_bubbles(true);
        let event = Event::new_with_event_init_dict(kind, &init).expect("an event");
        target.dispatch_event(&event).expect("dispatched");
    }

    fn role(role: &str) -> String {
        format!("[data-onecalc=\"{role}\"]")
    }

    impl Page {
        pub fn mount() -> Page {
            let document = web_sys::window()
                .expect("a window")
                .document()
                .expect("a document");
            let root: HtmlElement = document
                .create_element("div")
                .expect("a div")
                .dyn_into()
                .expect("an HTML element");
            document
                .body()
                .expect("a body")
                .append_child(&root)
                .expect("appended");
            let mounted = mount(root.clone());
            Page {
                root,
                _mounted: mounted,
            }
        }

        fn one(&self, name: &str) -> Element {
            self.root
                .query_selector(&role(name))
                .expect("a selector")
                .unwrap_or_else(|| panic!("the surface has no {name}"))
        }

        fn all(&self, scope: &Element, selector: &str) -> Vec<Element> {
            let list = scope.query_selector_all(selector).expect("a selector");
            (0..list.length())
                .map(|i| {
                    list.item(i)
                        .expect("an item")
                        .dyn_into()
                        .expect("an element")
                })
                .collect()
        }

        pub fn type_into(&self, name: &str, text: &str) {
            type_text(&self.one(name), text);
        }

        pub fn click(&self, name: &str) {
            self.one(name)
                .dyn_into::<HtmlElement>()
                .expect("an HTML element")
                .click();
        }

        pub fn text(&self, name: &str) -> String {
            self.one(name).text_content().unwrap_or_default()
        }

        pub fn items(&self, name: &str) -> Vec<String> {
            self.all(&self.one(name), "li")
                .iter()
                .map(|item| item.text_content().unwrap_or_default())
                .collect()
        }

        pub fn rows(&self) -> Vec<Element> {
            self.all(&self.root, &role("input-row"))
        }

        fn in_row(&self, row: usize, name: &str) -> Element {
            let rows = self.rows();
            let row = rows
                .get(row)
                .unwrap_or_else(|| panic!("no input row {row}"));
            row.query_selector(&role(name))
                .expect("a selector")
                .unwrap_or_else(|| panic!("the row has no {name}"))
        }

        pub fn set_row(&self, row: usize, address: &str, kind: &str, entry: &str) {
            type_text(&self.in_row(row, "input-cell"), address);
            let select = self.in_row(row, "input-kind");
            select
                .clone()
                .dyn_into::<HtmlSelectElement>()
                .expect("a select")
                .set_value(kind);
            dispatch(&select, "change");
            type_text(&self.in_row(row, "input-value"), entry);
        }

        pub fn set_row_entry(&self, row: usize, entry: &str) {
            type_text(&self.in_row(row, "input-value"), entry);
        }

        pub fn remove_row(&self, row: usize) {
            self.in_row(row, "remove-input")
                .dyn_into::<HtmlElement>()
                .expect("an HTML element")
                .click();
        }

        pub fn row_statuses(&self) -> Vec<String> {
            (0..self.rows().len())
                .map(|row| {
                    self.in_row(row, "input-status")
                        .text_content()
                        .unwrap_or_default()
                })
                .collect()
        }

        /// Checks what the surface shows against `example`.
        pub fn check(&self, example: &Example) {
            assert_eq!(
                self.text("result"),
                example.result,
                "{}: shown result",
                example.name
            );
            check_lines(
                &self.items("diagnostics"),
                example.diagnostics,
                example.name,
                "shown diagnostics",
            );
            assert_eq!(
                self.items("reads"),
                example.reads,
                "{}: shown reads",
                example.name
            );
        }

        /// Replaces the input rows with the example's and enters its formula.
        pub async fn enter(&self, example: &Example) {
            while !self.rows().is_empty() {
                self.remove_row(0);
                settle().await;
            }
            for _ in example.inputs {
                self.click("add-input");
            }
            settle().await;
            for (row, (address, kind, entry)) in example.inputs.iter().enumerate() {
                self.set_row(row, address, kind.selector(), entry);
            }
            self.type_into("formula", example.formula);
            settle().await;
        }
    }

    fn type_text(target: &Element, text: &str) {
        target
            .clone()
            .dyn_into::<HtmlInputElement>()
            .expect("an input")
            .set_value(text);
        dispatch(target, "input");
    }

    /// The browser part of the reference case. Returns the number of examples it checked.
    pub async fn surface_part() -> usize {
        let page = Page::mount();
        settle().await;
        assert_eq!(page.text("result"), "no formula");
        assert_eq!(page.text("sheets"), "Sheet1");

        page.type_into("formula-cell", FORMULA_CELL);
        page.type_into("sheet-name", "Sheet2");
        page.click("add-sheet");
        settle().await;
        assert_eq!(page.text("sheets"), "Sheet1, Sheet2");
        assert_eq!(page.text("formula-cell-status"), "Sheet1!C1");
        page.click("add-sheet");
        settle().await;
        assert_eq!(
            page.text("sheet-status"),
            "there is already a sheet named Sheet2"
        );

        // Two supplied cells, typed into two rows.
        page.enter(&EXAMPLES[0]).await;
        page.check(&EXAMPLES[0]);
        assert_eq!(
            page.row_statuses(),
            ["Sheet1!A1 = number 1.5", "Sheet2!B2 = number 4"]
        );

        // The Sheet2 row's value edited.
        page.set_row_entry(1, "10");
        settle().await;
        page.check(&EXAMPLES[1]);

        // The Sheet2 row removed.
        page.remove_row(1);
        settle().await;
        assert_eq!(page.rows().len(), 1);
        page.check(&EXAMPLES[2]);

        let mut checked = 3;
        for example in &EXAMPLES[3..] {
            page.enter(example).await;
            page.check(example);
            checked += 1;
        }

        // Retyping row 1 as the formula's cell C1 is refused there, and it removes the row that
        // supplied Sheet2!B2, so =A1*Sheet2!B2+1 has no input for Sheet2!B2 and is not calculated.
        page.enter(&EXAMPLES[0]).await;
        page.set_row(1, "C1", "number", "4");
        settle().await;
        assert_eq!(
            page.row_statuses(),
            [
                "Sheet1!A1 = number 1.5",
                "Sheet1!C1 is the formula's cell, which holds no input"
            ]
        );
        assert_eq!(page.text("result"), "not calculated");
        checked
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn onecalc_m1_1__reference_without_engine() {
    let examples = controller_part();
    note!(
        "ACCEPT-NOTE: onecalc_m1_1__reference_without_engine: {examples} examples calculated through dna_onecalc::Session (calc_fml::compile, instantiate and evaluate over the session's sheets and inputs); each result, diagnostic and read compared with hand-derived values; the refusal of =A1*1E-300*1E-10 and the missing-input stops are the implementation's answers, not Excel's values; not compared with Excel here"
    );
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test]
async fn onecalc_m1_1__reference_without_engine() {
    let examples = controller_part();
    let shown = browser::surface_part().await;
    note!(
        "ACCEPT-NOTE: onecalc_m1_1__reference_without_engine: {examples} examples calculated through dna_onecalc::Session, and {shown} entered in the mounted surface through dispatched input, change and click events and read back from its DOM; each result, diagnostic and read compared with hand-derived values; the refusal of =A1*1E-300*1E-10 and the missing-input stops are the implementation's answers, not Excel's values; not compared with Excel here"
    );
}

// ---------------------------------------------------------------------------------------------
// The dependency case.

#[cfg(not(target_arch = "wasm32"))]
mod metadata {
    use serde_json::Value;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// The triples the architecture check resolves (tools/xtask: SHIPPED_TARGETS), plus the host
    /// triple when it is none of them.
    pub const SHIPPED: [&str; 3] = [
        "x86_64-pc-windows-msvc",
        "x86_64-unknown-linux-gnu",
        "wasm32-unknown-unknown",
    ];

    pub fn cargo() -> String {
        std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
    }

    pub fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("the workspace root")
    }

    /// The host triple cargo reports.
    pub fn host() -> String {
        let out = Command::new(cargo())
            .arg("-vV")
            .output()
            .expect("cargo -vV runs");
        assert!(out.status.success(), "cargo -vV failed");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .expect("cargo -vV names its host")
            .trim()
            .to_owned()
    }

    /// The resolved graph for one triple: package names by ID, and each package's dependencies
    /// with the kinds of each edge (`normal`, `build`, `dev`).
    pub struct Graph {
        pub names: BTreeMap<String, String>,
        pub edges: BTreeMap<String, Vec<(String, BTreeSet<String>)>>,
        pub workspace: BTreeMap<String, String>,
    }

    pub fn resolve(root: &Path, target: &str) -> Graph {
        let out = Command::new(cargo())
            .args([
                "metadata",
                "--format-version",
                "1",
                "--all-features",
                "--locked",
                "--filter-platform",
                target,
            ])
            .current_dir(root)
            .output()
            .expect("cargo metadata runs");
        assert!(
            out.status.success(),
            "cargo metadata --filter-platform {target} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let metadata: Value = serde_json::from_slice(&out.stdout).expect("cargo metadata's JSON");
        let names: BTreeMap<String, String> = metadata["packages"]
            .as_array()
            .expect("packages")
            .iter()
            .map(|package| {
                (
                    package["id"].as_str().expect("an id").to_owned(),
                    package["name"].as_str().expect("a name").to_owned(),
                )
            })
            .collect();
        let workspace = metadata["workspace_members"]
            .as_array()
            .expect("workspace members")
            .iter()
            .map(|id| {
                let id = id.as_str().expect("an id").to_owned();
                (names[&id].clone(), id)
            })
            .collect();
        let mut edges = BTreeMap::new();
        for node in metadata["resolve"]["nodes"]
            .as_array()
            .expect("resolve nodes")
        {
            let id = node["id"].as_str().expect("a node id").to_owned();
            let deps = node["deps"]
                .as_array()
                .expect("deps")
                .iter()
                .map(|dep| {
                    let kinds = dep["dep_kinds"]
                        .as_array()
                        .expect("dep kinds")
                        .iter()
                        .map(|kind| kind["kind"].as_str().unwrap_or("normal").to_owned())
                        .collect();
                    (dep["pkg"].as_str().expect("a package id").to_owned(), kinds)
                })
                .collect();
            edges.insert(id, deps);
        }
        Graph {
            names,
            edges,
            workspace,
        }
    }

    impl Graph {
        /// The names of every package `root` builds with: its own normal, build and dev
        /// dependencies, then their normal and build dependencies, transitively.
        pub fn closure(&self, root: &str) -> BTreeSet<String> {
            let mut seen = BTreeSet::from([root.to_owned()]);
            let mut pending = vec![root.to_owned()];
            let mut names = BTreeSet::new();
            while let Some(id) = pending.pop() {
                for (dep, kinds) in self.edges.get(&id).into_iter().flatten() {
                    let followed =
                        id == root || kinds.iter().any(|kind| kind == "normal" || kind == "build");
                    if followed && seen.insert(dep.clone()) {
                        names.insert(self.names[dep].clone());
                        pending.push(dep.clone());
                    }
                }
            }
            names
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn onecalc_m1_1__dependency_gate() {
    let root = metadata::workspace_root();
    let host = metadata::host();
    let mut targets: Vec<String> = metadata::SHIPPED.iter().map(|t| (*t).to_owned()).collect();
    if !targets.contains(&host) {
        targets.push(host);
    }
    let forbidden = ["calc-engine", "calc-host"];
    let mut sizes = Vec::new();
    for target in &targets {
        let graph = metadata::resolve(&root, target);
        // The forbidden packages are real nodes of this graph, so their absence below is a
        // property of dna-onecalc's dependencies, not of a graph that lacks them.
        for name in forbidden {
            assert!(
                graph.workspace.contains_key(name),
                "[{target}] {name} is a workspace member of the resolved graph"
            );
        }
        let onecalc = &graph.workspace["dna-onecalc"];
        let closure = graph.closure(onecalc);
        // The walk reaches the formula libraries, the UI stack and, through the test-only edges,
        // the test harness: it follows path, registry and dev edges.
        for reached in ["calc-fml", "calc-func", "calc-value", "leptos", "web-sys"] {
            assert!(
                closure.contains(reached),
                "[{target}] dna-onecalc's closure reaches {reached}: {closure:?}"
            );
        }
        let harness = if target == "wasm32-unknown-unknown" {
            "wasm-bindgen-test"
        } else {
            "serde_json"
        };
        assert!(
            closure.contains(harness),
            "[{target}] the walk follows dna-onecalc's dev edges to {harness}"
        );
        // The walk reaches what dna-onecalc itself depends on: CalcFml and, through it, CalcFunc.
        for owned in ["calc-fml", "calc-func"] {
            assert!(
                closure.contains(owned),
                "[{target}] the walk from dna-onecalc reaches {owned}"
            );
        }
        let found: Vec<&str> = forbidden
            .into_iter()
            .filter(|name| closure.contains(*name))
            .collect();
        assert!(
            found.is_empty(),
            "[{target}] dna-onecalc's package graph contains {found:?}"
        );
        sizes.push(format!("{target}: {} packages", closure.len()));
    }
    note!(
        "ACCEPT-NOTE: onecalc_m1_1__dependency_gate: cargo metadata --all-features --locked --filter-platform for each of {}; dna-onecalc's package graph (its normal, build and dev edges, then normal and build edges) holds neither calc-engine nor calc-host ({})",
        targets.join(", "),
        sizes.join("; ")
    );
}
