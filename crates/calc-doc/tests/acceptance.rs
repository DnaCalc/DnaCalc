//! Acceptance cases owned by the calc-doc package, each registered by its exact name in
//! `crates/calc-doc/acceptance.json`. Every case is `#[test]` natively and `#[wasm_bindgen_test]`
//! on wasm32, with no browser or Node.js configuration of its own: `cargo xtask accept` forces the
//! browser with `WASM_BINDGEN_USE_BROWSER`. The cases reach the model only through calc-doc's
//! re-export `calc_doc::model` (and calc-value through `calc_doc::model::value`).
//!
//! The golden fixtures under `tests/fixtures/` are checked two ways. The codec decodes each to the
//! expected operations and re-encodes them byte for byte; that alone cannot tell a correct fixture
//! from one changed to match a mistake the encoder and the decoder share. So the bytes of each
//! fixture, its frame and every one of its operations, are also compared with text written here
//! from the wire format's description (`check_fixture_members`). The fixtures and the workspace's
//! `Cargo.lock` are read with `include_bytes!` and `include_str!`, so native and browser runs read
//! the same bytes.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_doc::model::value::{
    CellKey, ExcelText, GridArea, GridError, IdAllocator, MAX_COLUMNS, MAX_ROWS, SheetId,
    WorksheetErrorCode,
};
use calc_doc::model::{
    ApplyError, AuthoredRevision, Axis, AxisChange, AxisEdit, AxisEditError, BATCH_FORMAT,
    BATCH_VERSION, CellInput, CellPage, CellText, DecodeError, EncodeError, FiniteNumber,
    FormulaText, FormulaTextError, ModelRoot, ModelRootError, OpBatch, PAGE_COLUMNS, PAGE_ROWS,
    SHEET_NAME_MAX_UTF16_CODE_UNITS, SheetEdit, SheetEntry, SheetKey, SheetName, SheetNameError,
    SheetVisibility, WorkbookOp, decode_batch, encode_batch, op_capabilities,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::num::NonZeroU32;
use std::sync::Arc;

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

const CELLS_V1: &[u8] = include_bytes!("fixtures/cells_v1.json");
const AXES_V1: &[u8] = include_bytes!("fixtures/axes_v1.json");
const SHEETS_V1: &[u8] = include_bytes!("fixtures/sheets_v1.json");

/// The workspace's committed lockfile.
const CARGO_LOCK: &str = include_str!("../../../Cargo.lock");

/// The revision every fixture batch is based on, and the fixture roots' revision.
const FIXTURE_BASE: u64 = 41;

// ---------------------------------------------------------------------------------------------
// The fixture document, as two sessions see it.

/// The fixture document as one session sees it: sheets with keys 1, 2, 5 and 7 (named Sheet1,
/// Data, Archive and Lookup), whose IDs this session's allocator issued, plus one more ID that
/// names no sheet of the root.
struct Session {
    root: Arc<ModelRoot>,
    sheet1: SheetId,
    data: SheetId,
    archive: SheetId,
    lookup: SheetId,
    stranger: SheetId,
}

/// `skew` IDs are issued and released before the sheets' IDs, so two sessions with different
/// skews give the same keys different IDs.
fn session(skew: usize) -> Session {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let earlier: Vec<SheetId> = (0..skew).map(|_| ids.issue().expect("an ID")).collect();
    for id in earlier {
        ids.release(id).expect("a live ID");
    }
    let mut issue = || ids.issue().expect("an ID");
    let (sheet1, data, archive, lookup, stranger) = (issue(), issue(), issue(), issue(), issue());
    let root = ModelRoot::new(
        AuthoredRevision::new(FIXTURE_BASE),
        vec![
            SheetEntry::new(
                sheet1,
                SheetKey::new(1),
                name("Sheet1"),
                SheetVisibility::Visible,
            ),
            SheetEntry::new(
                data,
                SheetKey::new(2),
                name("Data"),
                SheetVisibility::Visible,
            ),
            SheetEntry::new(
                archive,
                SheetKey::new(5),
                name("Archive"),
                SheetVisibility::Hidden,
            ),
            SheetEntry::new(
                lookup,
                SheetKey::new(7),
                name("Lookup"),
                SheetVisibility::VeryHidden,
            ),
        ],
    )
    .expect("four sheets with distinct keys and IDs");
    Session {
        root,
        sheet1,
        data,
        archive,
        lookup,
        stranger,
    }
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn name(text: &str) -> SheetName {
    SheetName::new(ExcelText::from_utf16_code_units(units(text))).expect("a valid sheet name")
}

fn position(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).expect("a position from 1")
}

/// 32,767 code units, A to Z repeating.
fn longest_text() -> Vec<u16> {
    (0..32_767u32)
        .map(|i| u16::try_from(0x41 + i % 26).expect("a letter"))
        .collect()
}

fn number(value: f64) -> CellInput {
    CellInput::Number(FiniteNumber::new(value).expect("a finite number"))
}

fn text(units: Vec<u16>) -> CellInput {
    CellInput::Text(
        CellText::new(ExcelText::from_utf16_code_units(units)).expect("text within the limit"),
    )
}

fn formula(source: &str) -> CellInput {
    CellInput::Formula(
        FormulaText::new(ExcelText::from_utf16_code_units(units(source))).expect("a formula"),
    )
}

fn cell(sheet: SheetId, row: u32, column: u32) -> CellKey {
    CellKey::new(sheet, row, column).expect("a cell inside the grid")
}

fn set(sheet: SheetId, row: u32, column: u32, input: CellInput) -> WorkbookOp {
    WorkbookOp::SetCell {
        at: cell(sheet, row, column),
        input,
    }
}

/// The operations `cells_v1.json` holds, in this session's IDs.
fn cells_ops(s: &Session) -> Vec<WorkbookOp> {
    let smallest_subnormal = f64::from_bits(1);
    assert!(smallest_subnormal > 0.0 && smallest_subnormal < f64::MIN_POSITIVE);
    let mut ops = vec![
        set(s.sheet1, 1, 1, number(-0.0)),
        set(s.sheet1, 2, 1, number(smallest_subnormal)),
        set(s.sheet1, 3, 1, number(f64::MAX)),
        set(s.sheet1, 4, 1, number(1.5)),
        set(s.sheet1, 1, 2, text(longest_text())),
        set(s.sheet1, 2, 2, text(vec![0x41, 0x62, 0xD83D])),
        set(s.sheet1, 3, 2, text(Vec::new())),
        set(s.sheet1, 4, 2, text(units("=SUM(A1:A3)"))),
        set(s.sheet1, 1, 3, CellInput::Logical(true)),
        set(s.sheet1, 2, 3, CellInput::Logical(false)),
    ];
    for (row, code) in (1..).zip(WorksheetErrorCode::all()) {
        ops.push(set(s.sheet1, row, 4, CellInput::Error(code)));
    }
    ops.push(set(s.sheet1, 1, 5, formula("SUM(A1:A3)")));
    ops.push(set(
        s.data,
        MAX_ROWS,
        MAX_COLUMNS,
        formula("LEN(\"\u{20AC}\u{1F600}\")"),
    ));
    ops.push(WorkbookOp::Clear {
        area: GridArea::full_sheet(s.data),
    });
    ops.push(WorkbookOp::Clear {
        area: GridArea::new(cell(s.sheet1, 2, 2), cell(s.sheet1, 3, 3)).expect("one sheet"),
    });
    // B3:D7: its top, left, bottom and right differ from one another and from its height (5) and
    // width (3).
    ops.push(WorkbookOp::Clear {
        area: GridArea::new(cell(s.sheet1, 3, 2), cell(s.sheet1, 7, 4)).expect("one sheet"),
    });
    ops
}

fn axis(sheet: SheetId, axis: Axis, change: AxisChange, first: u32, count: u32) -> WorkbookOp {
    WorkbookOp::EditAxis(
        AxisEdit::new(sheet, axis, change, first, count).expect("an edit inside the grid"),
    )
}

/// The operations `axes_v1.json` holds, in this session's IDs.
fn axes_ops(s: &Session) -> Vec<WorkbookOp> {
    use AxisChange::{Delete, Insert};
    use calc_doc::model::Axis::{Columns, Rows};
    vec![
        axis(s.sheet1, Rows, Insert, 3, 2),
        axis(s.sheet1, Rows, Insert, 1, MAX_ROWS),
        axis(s.sheet1, Rows, Delete, MAX_ROWS, 1),
        axis(s.data, Columns, Insert, MAX_COLUMNS, 1),
        axis(s.data, Columns, Delete, 1, MAX_COLUMNS),
        axis(s.data, Rows, Delete, 10, 5),
    ]
}

/// The operations `sheets_v1.json` holds, in this session's IDs.
fn sheets_ops(s: &Session) -> Vec<WorkbookOp> {
    vec![
        SheetEdit::Insert {
            key: SheetKey::new(9),
            name: name("Totals \u{20AC}"),
            position: position(3),
        },
        SheetEdit::Rename {
            sheet: s.data,
            name: name("Inputs"),
        },
        SheetEdit::Move {
            sheet: s.sheet1,
            position: position(4),
        },
        SheetEdit::SetVisibility {
            sheet: s.archive,
            visibility: SheetVisibility::Visible,
        },
        SheetEdit::SetVisibility {
            sheet: s.data,
            visibility: SheetVisibility::Hidden,
        },
        SheetEdit::SetVisibility {
            sheet: s.sheet1,
            visibility: SheetVisibility::VeryHidden,
        },
        SheetEdit::Remove { sheet: s.lookup },
    ]
    .into_iter()
    .map(WorkbookOp::EditSheet)
    .collect()
}

type Fixture = (&'static str, &'static [u8], fn(&Session) -> Vec<WorkbookOp>);

const FIXTURES: [Fixture; 3] = [
    ("cells_v1.json", CELLS_V1, cells_ops),
    ("axes_v1.json", AXES_V1, axes_ops),
    ("sheets_v1.json", SHEETS_V1, sheets_ops),
];

// ---------------------------------------------------------------------------------------------
// Independent observations of decoded operations.

fn hex_units(units: &[u16]) -> String {
    units.iter().map(|unit| format!("{unit:04x}")).collect()
}

/// A description of an input built from its observable parts: a number's bits through
/// `f64::to_bits`, text and formulas as their code units.
fn describe_input(input: &CellInput) -> String {
    match input {
        CellInput::Number(number) => format!("number {:016x}", number.get().to_bits()),
        CellInput::Text(text) => format!("text {}", hex_units(text.text().utf16_code_units())),
        CellInput::Logical(value) => format!("logical {value}"),
        CellInput::Error(code) => format!("error {code:?}"),
        CellInput::Formula(formula) => {
            format!("formula {}", hex_units(formula.text().utf16_code_units()))
        }
    }
}

/// A description of an operation built from its observable parts, independent of the model's
/// own equality.
fn describe(op: &WorkbookOp) -> String {
    match op {
        WorkbookOp::SetCell { at, input } => format!(
            "set {:?} R{}C{} {}",
            at.sheet(),
            at.row(),
            at.column(),
            describe_input(input)
        ),
        WorkbookOp::Clear { area } => format!(
            "clear {:?} R{}C{}:R{}C{}",
            area.sheet(),
            area.first().row(),
            area.first().column(),
            area.last().row(),
            area.last().column()
        ),
        WorkbookOp::EditAxis(edit) => format!(
            "axis {:?} {:?} {:?} first {} count {} last {}",
            edit.sheet(),
            edit.axis(),
            edit.change(),
            edit.first(),
            edit.count(),
            edit.last()
        ),
        WorkbookOp::EditSheet(edit) => match edit {
            SheetEdit::Insert {
                key,
                name,
                position,
            } => format!(
                "sheet insert key {} name {} position {position}",
                key.get(),
                hex_units(name.text().utf16_code_units())
            ),
            SheetEdit::Rename { sheet, name } => format!(
                "sheet rename {sheet:?} name {}",
                hex_units(name.text().utf16_code_units())
            ),
            SheetEdit::Move { sheet, position } => {
                format!("sheet move {sheet:?} position {position}")
            }
            SheetEdit::Remove { sheet } => format!("sheet remove {sheet:?}"),
            SheetEdit::SetVisibility { sheet, visibility } => {
                format!("sheet visibility {sheet:?} {visibility:?}")
            }
        },
    }
}

fn describe_all(ops: &[WorkbookOp]) -> Vec<String> {
    ops.iter().map(describe).collect()
}

/// The features an operation exercises, by an exhaustive match: a variant added to the model
/// stops this file compiling until the cases cover it.
fn features(op: &WorkbookOp, out: &mut BTreeSet<String>) {
    let feature = match op {
        WorkbookOp::SetCell { input, .. } => {
            out.insert("op:set_cell".to_owned());
            match input {
                CellInput::Number(_) => "input:number".to_owned(),
                CellInput::Text(_) => "input:text".to_owned(),
                CellInput::Logical(_) => "input:logical".to_owned(),
                CellInput::Error(code) => {
                    out.insert("input:error".to_owned());
                    format!("error:{code:?}")
                }
                CellInput::Formula(_) => "input:formula".to_owned(),
            }
        }
        WorkbookOp::Clear { .. } => "op:clear".to_owned(),
        WorkbookOp::EditAxis(edit) => {
            out.insert("op:edit_axis".to_owned());
            let axis = match edit.axis() {
                Axis::Rows => "rows",
                Axis::Columns => "columns",
            };
            let change = match edit.change() {
                AxisChange::Insert => "insert",
                AxisChange::Delete => "delete",
            };
            format!("axis:{change}_{axis}")
        }
        WorkbookOp::EditSheet(edit) => {
            out.insert("op:edit_sheet".to_owned());
            match edit {
                SheetEdit::Insert { .. } => "sheet:insert".to_owned(),
                SheetEdit::Rename { .. } => "sheet:rename".to_owned(),
                SheetEdit::Move { .. } => "sheet:move".to_owned(),
                SheetEdit::Remove { .. } => "sheet:remove".to_owned(),
                SheetEdit::SetVisibility { visibility, .. } => {
                    out.insert("sheet:set_visibility".to_owned());
                    match visibility {
                        SheetVisibility::Visible => "visibility:visible".to_owned(),
                        SheetVisibility::Hidden => "visibility:hidden".to_owned(),
                        SheetVisibility::VeryHidden => "visibility:very_hidden".to_owned(),
                    }
                }
            }
        }
    };
    out.insert(feature);
}

/// Every feature the round trip must cover.
fn required_features() -> BTreeSet<String> {
    let mut out: BTreeSet<String> = [
        "op:set_cell",
        "op:clear",
        "op:edit_axis",
        "op:edit_sheet",
        "input:number",
        "input:text",
        "input:logical",
        "input:error",
        "input:formula",
        "axis:insert_rows",
        "axis:delete_rows",
        "axis:insert_columns",
        "axis:delete_columns",
        "sheet:insert",
        "sheet:rename",
        "sheet:move",
        "sheet:remove",
        "sheet:set_visibility",
        "visibility:visible",
        "visibility:hidden",
        "visibility:very_hidden",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for code in WorksheetErrorCode::all() {
        out.insert(format!("error:{code:?}"));
    }
    out
}

/// The first byte offset at which two byte strings differ, for a readable failure.
fn first_difference(left: &[u8], right: &[u8]) -> Option<usize> {
    (0..left.len().max(right.len())).find(|i| left.get(*i) != right.get(*i))
}

/// Panics unless `left` and `right` are the same bytes, showing where they first differ under the
/// names `names` gives them.
fn assert_bytes(label: &str, names: (&str, &str), left: &[u8], right: &[u8]) {
    if let Some(offset) = first_difference(left, right) {
        let window = |bytes: &[u8]| {
            let end = bytes.len().min(offset + 60);
            String::from_utf8_lossy(&bytes[offset.min(end)..end]).into_owned()
        };
        let (left_name, right_name) = names;
        panic!(
            "{label}: {left_name} {} bytes, {right_name} {} bytes; first difference at byte {offset}: {left_name} {:?}, {right_name} {:?}",
            left.len(),
            right.len(),
            window(left),
            window(right)
        );
    }
}

fn assert_same_bytes(label: &str, encoded: &[u8], golden: &[u8]) {
    assert_bytes(label, ("encoded", "golden"), encoded, golden);
}

/// The frame's format, version and base, and the (capability, version, operation) of every
/// envelope of a wire message, read with serde_json. serde_json is also the codec's JSON
/// tokenizer, so this reading is not independent of the codec. The comparison that is independent
/// of it is the literal-value comparison, [`check_fixture_members`], which compares the fixtures'
/// bytes with text written in this file without parsing them.
fn inspect(bytes: &[u8]) -> (String, u64, u64, Vec<(String, u64, String)>) {
    let message: Value = serde_json::from_slice(bytes).expect("a JSON message");
    let envelopes = message["operations"]
        .as_array()
        .expect("an operations array")
        .iter()
        .map(|envelope| {
            (
                envelope["capability"]
                    .as_str()
                    .expect("a capability")
                    .to_owned(),
                envelope["version"].as_u64().expect("a version"),
                envelope["op"].as_str().expect("an operation").to_owned(),
            )
        })
        .collect();
    (
        message["format"].as_str().expect("a format").to_owned(),
        message["version"].as_u64().expect("a version"),
        message["base"].as_u64().expect("a base"),
        envelopes,
    )
}

// ---------------------------------------------------------------------------------------------
// The literal-value comparison: the fixtures' bytes compared, without parsing them and so apart
// from the codec and its JSON tokenizer, with text written here from the wire format's
// description: a frame `{"format","version","base","operations"}`, envelopes
// `{"capability","version","op","args"}`, arguments in their declaration order, sheets as their
// keys, numbers as the 16 lowercase hex digits of their binary64 bits, text as 4 lowercase hex
// digits per UTF-16 code unit, and an error code as its wire name.

/// Each worksheet error code with its wire name, in the order the cells fixture holds them (rows
/// 1 to 14 of column 4 on the sheet with key 1).
const ERROR_WIRE_NAMES: [(WorksheetErrorCode, &str); 14] = [
    (WorksheetErrorCode::Null, "null"),
    (WorksheetErrorCode::Div0, "div0"),
    (WorksheetErrorCode::Value, "value"),
    (WorksheetErrorCode::Ref, "ref"),
    (WorksheetErrorCode::Name, "name"),
    (WorksheetErrorCode::Num, "num"),
    (WorksheetErrorCode::NA, "na"),
    (WorksheetErrorCode::Busy, "busy"),
    (WorksheetErrorCode::GettingData, "getting_data"),
    (WorksheetErrorCode::Spill, "spill"),
    (WorksheetErrorCode::Calc, "calc"),
    (WorksheetErrorCode::Field, "field"),
    (WorksheetErrorCode::Blocked, "blocked"),
    (WorksheetErrorCode::Connect, "connect"),
];

/// Every fixture's frame up to its first operation: the format, frame version 1 and base 41.
const FIXTURE_FRAME: &str =
    r#"{"format":"dnacalc/workbook-op-batch","version":1,"base":41,"operations":["#;

/// One fixture operation as written here: its operation name, what it holds (for a failure
/// message), and the members of its arguments in wire order.
type LiteralOp = (&'static str, String, String);

fn literal(op: &'static str, what: &str, args: &str) -> LiteralOp {
    (op, what.to_owned(), args.to_owned())
}

/// The operations of `cells_v1.json`, in order. Every argument is written out here except the
/// digits of the 32,767-unit text, which `long_digits` holds.
fn cells_literal(long_digits: &str) -> Vec<LiteralOp> {
    let set = |what: &str, args: &str| literal("set_cell", what, args);
    let mut out = vec![
        set(
            "-0.0 at A1",
            r#""sheet":1,"row":1,"column":1,"input":{"number":"8000000000000000"}"#,
        ),
        set(
            "the smallest subnormal at A2",
            r#""sheet":1,"row":2,"column":1,"input":{"number":"0000000000000001"}"#,
        ),
        set(
            "the largest finite at A3",
            r#""sheet":1,"row":3,"column":1,"input":{"number":"7fefffffffffffff"}"#,
        ),
        set(
            "1.5 at A4",
            r#""sheet":1,"row":4,"column":1,"input":{"number":"3ff8000000000000"}"#,
        ),
        set(
            "the 32,767-unit text at B1",
            &format!(r#""sheet":1,"row":1,"column":2,"input":{{"text":"{long_digits}"}}"#),
        ),
        set(
            "the text ending in a high surrogate at B2",
            r#""sheet":1,"row":2,"column":2,"input":{"text":"00410062d83d"}"#,
        ),
        set(
            "the empty text at B3",
            r#""sheet":1,"row":3,"column":2,"input":{"text":""}"#,
        ),
        set(
            "the text =SUM(A1:A3) at B4",
            r#""sheet":1,"row":4,"column":2,"input":{"text":"003d00530055004d002800410031003a004100330029"}"#,
        ),
        set(
            "true at C1",
            r#""sheet":1,"row":1,"column":3,"input":{"logical":true}"#,
        ),
        set(
            "false at C2",
            r#""sheet":1,"row":2,"column":3,"input":{"logical":false}"#,
        ),
    ];
    for (row, (_, name)) in (1..).zip(ERROR_WIRE_NAMES) {
        out.push(set(
            &format!("error {name} at D{row}"),
            &format!(r#""sheet":1,"row":{row},"column":4,"input":{{"error":"{name}"}}"#),
        ));
    }
    // SUM(A1:A3); then LEN("€😀"): L E N ( " U+20AC U+D83D U+DE00 " ).
    out.extend([
        set(
            "the formula SUM(A1:A3) at E1",
            r#""sheet":1,"row":1,"column":5,"input":{"formula":"00530055004d002800410031003a004100330029"}"#,
        ),
        set(
            "the formula at XFD1048576",
            r#""sheet":2,"row":1048576,"column":16384,"input":{"formula":"004c0045004e0028002220acd83dde0000220029"}"#,
        ),
        literal(
            "clear",
            "the full sheet of key 2",
            r#""sheet":2,"top":1,"left":1,"bottom":1048576,"right":16384"#,
        ),
        literal(
            "clear",
            "B2:C3",
            r#""sheet":1,"top":2,"left":2,"bottom":3,"right":3"#,
        ),
        literal(
            "clear",
            "B3:D7",
            r#""sheet":1,"top":3,"left":2,"bottom":7,"right":4"#,
        ),
    ]);
    out
}

/// The operations of `axes_v1.json`, in order.
fn axes_literal() -> Vec<LiteralOp> {
    vec![
        literal(
            "insert_rows",
            "rows 3 and 4 of key 1",
            r#""sheet":1,"first":3,"count":2"#,
        ),
        literal(
            "insert_rows",
            "every row of key 1",
            r#""sheet":1,"first":1,"count":1048576"#,
        ),
        literal(
            "delete_rows",
            "the last row of key 1",
            r#""sheet":1,"first":1048576,"count":1"#,
        ),
        literal(
            "insert_columns",
            "the last column of key 2",
            r#""sheet":2,"first":16384,"count":1"#,
        ),
        literal(
            "delete_columns",
            "every column of key 2",
            r#""sheet":2,"first":1,"count":16384"#,
        ),
        literal(
            "delete_rows",
            "rows 10 to 14 of key 2",
            r#""sheet":2,"first":10,"count":5"#,
        ),
    ]
}

/// The operations of `sheets_v1.json`, in order: "Totals €" and "Inputs" as UTF-16 code units.
fn sheets_literal() -> Vec<LiteralOp> {
    vec![
        literal(
            "insert",
            "key 9 named Totals € at position 3",
            r#""key":9,"name":"0054006f00740061006c0073002020ac","position":3"#,
        ),
        literal(
            "rename",
            "key 2 to Inputs",
            r#""sheet":2,"name":"0049006e0070007500740073""#,
        ),
        literal("move", "key 1 to position 4", r#""sheet":1,"position":4"#),
        literal(
            "set_visibility",
            "key 5 visible",
            r#""sheet":5,"visibility":"visible""#,
        ),
        literal(
            "set_visibility",
            "key 2 hidden",
            r#""sheet":2,"visibility":"hidden""#,
        ),
        literal(
            "set_visibility",
            "key 1 very hidden",
            r#""sheet":1,"visibility":"very_hidden""#,
        ),
        literal("remove", "key 7", r#""sheet":7"#),
    ]
}

/// Compares the bytes of one fixture with text written here: [`FIXTURE_FRAME`], then each of
/// `operations` as a whole envelope of `capability` at version 1, separated by commas, then the
/// closing `]}`. Panics at the first part whose bytes differ, naming it. Returns the number of
/// operations compared.
fn compare_fixture(
    file: &str,
    fixture: &[u8],
    capability: &str,
    operations: &[LiteralOp],
) -> usize {
    let mut rest = fixture;
    let mut take = |label: String, literal: &[u8]| {
        let found = &rest[..literal.len().min(rest.len())];
        assert_bytes(&label, ("fixture", "literal"), found, literal);
        rest = &rest[literal.len()..];
    };
    take(
        format!("{file}: the frame before its operations"),
        FIXTURE_FRAME.as_bytes(),
    );
    for (index, (op, what, args)) in operations.iter().enumerate() {
        if index > 0 {
            take(format!("{file}: the comma before operation {index}"), b",");
        }
        take(
            format!("{file}: operation {index} ({op}, {what})"),
            envelope(capability, op, args).as_bytes(),
        );
    }
    take(
        format!("{file}: the end after {} operations", operations.len()),
        b"]}",
    );
    assert!(
        rest.is_empty(),
        "{file}: {} bytes after the frame",
        rest.len()
    );
    operations.len()
}

/// Compares the bytes of the three golden fixtures, without parsing them and so apart from the
/// codec and its JSON tokenizer, with text written in this file from the wire format's
/// description: each fixture's frame up to its first operation, every one of its operations as a
/// whole envelope (capability, version, operation name and every argument, members in wire
/// order), and its closing `]}`. Every argument is written out here
/// except the 131,068 digits of the 32,767-unit text, which are built here from that text's rule
/// (A to Z repeating), not by the codec. So a fixture changed to match a mistake the encoder and
/// the decoder share fails here when the mistake changes the bytes of any operation the fixtures
/// hold; a mistake that shows only on values no fixture holds is not caught here. Returns, per
/// fixture, the number of operations compared.
fn check_fixture_members() -> [(&'static str, usize); 3] {
    let long_digits = hex_units(&longest_text());
    assert_eq!(long_digits.len(), 131_068, "32,767 units of 4 hex digits");
    assert_eq!(
        &long_digits[..12],
        "004100420043",
        "the long text's first units"
    );
    assert_eq!(
        &long_digits[long_digits.len() - 8..],
        "00460047",
        "the long text's last units"
    );
    let cells = compare_fixture(
        "cells_v1.json",
        CELLS_V1,
        "cells",
        &cells_literal(&long_digits),
    );
    let axes = compare_fixture("axes_v1.json", AXES_V1, "axes", &axes_literal());
    let sheets = compare_fixture("sheets_v1.json", SHEETS_V1, "sheets", &sheets_literal());
    [
        ("cells_v1.json", cells),
        ("axes_v1.json", axes),
        ("sheets_v1.json", sheets),
    ]
}

// ---------------------------------------------------------------------------------------------
// Messages for the refusal checks.

fn batch_of(envelopes: &[String]) -> Vec<u8> {
    format!(
        "{{\"format\":\"{BATCH_FORMAT}\",\"version\":{BATCH_VERSION},\"base\":{FIXTURE_BASE},\"operations\":[{}]}}",
        envelopes.join(",")
    )
    .into_bytes()
}

fn envelope(capability: &str, op: &str, args: &str) -> String {
    format!("{{\"capability\":\"{capability}\",\"version\":1,\"op\":\"{op}\",\"args\":{{{args}}}}}")
}

/// One set_cell of A1 on the sheet with key 1, with `input` as its input object's members.
fn set_cell_with(input: &str) -> Vec<u8> {
    batch_of(&[envelope(
        "cells",
        "set_cell",
        &format!("\"sheet\":1,\"row\":1,\"column\":1,\"input\":{{{input}}}"),
    )])
}

fn hex_text(text: &str) -> String {
    hex_units(&units(text))
}

/// `bytes` with the first occurrence of `from` replaced by `to`.
fn replace_first(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).expect("an ASCII fixture");
    assert!(text.contains(from), "the fixture holds {from}");
    text.replacen(from, to, 1).into_bytes()
}

/// The members of a frame or an envelope, each `(name, raw JSON value)`, in order, as one JSON
/// object: with `change` None all of them; with `Some((member, None))` without `member`; with
/// `Some((member, Some(value)))` with `value` in place of `member`'s.
fn object_with(members: &[(&str, &str)], change: Option<(&str, Option<&str>)>) -> String {
    let body: Vec<String> = members
        .iter()
        .filter_map(|(name, valid)| match change {
            Some((member, None)) if member == *name => None,
            Some((member, Some(other))) if member == *name => Some(format!("\"{name}\":{other}")),
            _ => Some(format!("\"{name}\":{valid}")),
        })
        .collect();
    format!("{{{}}}", body.join(","))
}

/// The envelope of a set_cell of A1 on the sheet with key 1 to true, changed by `change` as
/// [`object_with`] changes it.
fn one_envelope(change: Option<(&str, Option<&str>)>) -> String {
    object_with(
        &[
            ("capability", "\"cells\""),
            ("version", "1"),
            ("op", "\"set_cell\""),
            (
                "args",
                "{\"sheet\":1,\"row\":1,\"column\":1,\"input\":{\"logical\":true}}",
            ),
        ],
        change,
    )
}

/// A batch frame (format, version 1, base 41) holding [`one_envelope`] unchanged, changed by
/// `change` as [`object_with`] changes it.
fn frame_with(change: Option<(&str, Option<&str>)>) -> Vec<u8> {
    let operations = format!("[{}]", one_envelope(None));
    object_with(
        &[
            ("format", "\"dnacalc/workbook-op-batch\""),
            ("version", "1"),
            ("base", "41"),
            ("operations", &operations),
        ],
        change,
    )
    .into_bytes()
}

const OPS0: &str = "$.operations[0]";

fn at(path: &str) -> String {
    format!("{OPS0}{path}")
}

fn assert_refused(label: &str, bytes: &[u8], root: &ModelRoot, expected: &DecodeError) {
    match decode_batch(bytes, root) {
        Ok(batch) => panic!("{label}: decoded {:?}", describe_all(&batch.operations)),
        Err(error) => assert_eq!(&error, expected, "{label}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Model roots, axis edits and numbers, against literal values.

/// The sheets of a root as (ID, key, name as hex UTF-16 code units, visibility), read through
/// `ModelRoot::sheets` and each `SheetEntry`'s accessors.
fn root_sheets(root: &ModelRoot) -> Vec<(SheetId, u32, String, SheetVisibility)> {
    root.sheets()
        .iter()
        .map(|sheet| {
            (
                sheet.id(),
                sheet.key().get(),
                hex_units(sheet.name().text().utf16_code_units()),
                sheet.visibility(),
            )
        })
        .collect()
}

/// The fixture root's revision and sheets compared with literal values: revision 41, and the
/// keys, names (Sheet1, Data, Archive and Lookup as hex code units) and visibilities the fixture
/// document gives its sheets, in order, each with the ID this session issued it. `when` names the
/// point of the case, for a failure.
fn check_fixture_root(s: &Session, when: &str) {
    assert_eq!(
        s.root.revision().get(),
        41,
        "ModelRoot::revision of the fixture root, {when}"
    );
    assert_eq!(
        root_sheets(&s.root),
        vec![
            (
                s.sheet1,
                1,
                "005300680065006500740031".to_owned(),
                SheetVisibility::Visible
            ),
            (
                s.data,
                2,
                "0044006100740061".to_owned(),
                SheetVisibility::Visible
            ),
            (
                s.archive,
                5,
                "0041007200630068006900760065".to_owned(),
                SheetVisibility::Hidden
            ),
            (
                s.lookup,
                7,
                "004c006f006f006b00750070".to_owned(),
                SheetVisibility::VeryHidden
            ),
        ],
        "ModelRoot::sheets of the fixture root (ID, key, name, visibility), {when}"
    );
}

/// Roots other than the fixture's: one at `AuthoredRevision::INITIAL` with one hidden sheet, and
/// one at revision 2^64 - 1 with none, each read back through its accessors.
fn check_other_roots(s: &Session) {
    let lone = ModelRoot::new(
        AuthoredRevision::INITIAL,
        vec![SheetEntry::new(
            s.stranger,
            SheetKey::new(3),
            name("Only"),
            SheetVisibility::Hidden,
        )],
    )
    .expect("one sheet");
    assert_eq!(
        lone.revision().get(),
        0,
        "ModelRoot::revision of a root at INITIAL"
    );
    assert_eq!(
        root_sheets(&lone),
        vec![(
            s.stranger,
            3,
            "004f006e006c0079".to_owned(),
            SheetVisibility::Hidden
        )],
        "ModelRoot::sheets of a one-sheet root"
    );
    let empty = ModelRoot::new(AuthoredRevision::new(u64::MAX), Vec::new()).expect("no sheets");
    assert_eq!(
        empty.revision().get(),
        18_446_744_073_709_551_615,
        "ModelRoot::revision of a root at the last revision"
    );
    assert!(empty.sheets().is_empty(), "a root without sheets");
}

/// A root is shared, not copied. What a change to calc-doc-model can break: a clone's sheets are
/// the same slice at the same address, and `sheet_by_key` returns the root's own entry. The
/// `Arc::ptr_eq`, handle-count and `Arc::get_mut` checks only confirm that the case holds the root
/// through `std::sync::Arc`. Returns the number of handles before and while the clone is held.
fn check_root_sharing(first: &Session, later: &Session) -> (usize, usize) {
    let before = Arc::strong_count(&first.root);
    let mut shared = Arc::clone(&first.root);
    assert!(
        Arc::ptr_eq(&shared, &first.root),
        "a clone of a root is the same allocation"
    );
    assert!(
        std::ptr::eq(shared.sheets().as_ptr(), first.root.sheets().as_ptr()),
        "a clone's ModelRoot::sheets is the same slice"
    );
    assert!(
        std::ptr::eq(
            shared.sheet_by_key(SheetKey::new(5)).expect("key 5"),
            &first.root.sheets()[2]
        ),
        "sheet_by_key returns the root's own entry"
    );
    let during = Arc::strong_count(&first.root);
    assert_eq!(during, before + 1, "the clone is a second handle");
    assert!(
        Arc::get_mut(&mut shared).is_none(),
        "a shared root cannot be borrowed mutably"
    );
    assert!(
        !Arc::ptr_eq(&first.root, &later.root),
        "two roots built separately are separate allocations"
    );
    assert!(
        !std::ptr::eq(first.root.sheets().as_ptr(), later.root.sheets().as_ptr()),
        "two roots built separately hold separate sheets"
    );
    (before, during)
}

/// `AxisEdit::last` compared with literal positions: each edit of the axes fixture as decoded,
/// as (operation, first, count, last), then edits built directly. Both include an insert and a
/// delete that reach the grid's last row (1,048,576) and its last column (16,384).
fn check_axis_lasts(s: &Session) -> (usize, usize) {
    let decoded = decode_batch(AXES_V1, &s.root).expect("axes_v1.json");
    let spans: Vec<(&str, u32, u32, u32)> = decoded
        .operations
        .iter()
        .map(|op| {
            let WorkbookOp::EditAxis(edit) = op else {
                panic!("an axes fixture operation decoded as {}", describe(op));
            };
            let kind = match (edit.axis(), edit.change()) {
                (Axis::Rows, AxisChange::Insert) => "insert_rows",
                (Axis::Rows, AxisChange::Delete) => "delete_rows",
                (Axis::Columns, AxisChange::Insert) => "insert_columns",
                (Axis::Columns, AxisChange::Delete) => "delete_columns",
            };
            (kind, edit.first(), edit.count(), edit.last())
        })
        .collect();
    assert_eq!(
        spans,
        vec![
            ("insert_rows", 3, 2, 4),
            ("insert_rows", 1, 1_048_576, 1_048_576),
            ("delete_rows", 1_048_576, 1, 1_048_576),
            ("insert_columns", 16_384, 1, 16_384),
            ("delete_columns", 1, 16_384, 16_384),
            ("delete_rows", 10, 5, 14),
        ],
        "AxisEdit::last of the axes fixture's edits (operation, first, count, last)"
    );
    let built = [
        (Axis::Rows, AxisChange::Insert, 1, 1, 1),
        (Axis::Rows, AxisChange::Insert, 1_048_576, 1, 1_048_576),
        (Axis::Rows, AxisChange::Delete, 1_048_575, 2, 1_048_576),
        (Axis::Columns, AxisChange::Insert, 16_383, 2, 16_384),
        (Axis::Columns, AxisChange::Delete, 7, 3, 9),
        (Axis::Columns, AxisChange::Delete, 16_384, 1, 16_384),
    ];
    for (axis, change, first, count, last) in built {
        let edit = AxisEdit::new(s.sheet1, axis, change, first, count).expect("inside the grid");
        assert_eq!(
            edit.last(),
            last,
            "AxisEdit::last of {change:?} {axis:?} {first}+{count}"
        );
    }
    (spans.len(), built.len())
}

/// `FiniteNumber` compares bits: +0.0 and -0.0 are different numbers and different cell inputs,
/// and each round-trips to its own bits, compared with literal wire text and literal bits.
fn check_signed_zeros(first: &Session, later: &Session) {
    let positive = FiniteNumber::new(0.0).expect("+0.0");
    let negative = FiniteNumber::new(-0.0).expect("-0.0");
    assert_eq!(
        (positive.to_bits(), negative.to_bits()),
        (0x0000_0000_0000_0000, 0x8000_0000_0000_0000),
        "the bits of +0.0 and -0.0"
    );
    assert_ne!(
        positive, negative,
        "+0.0 and -0.0 are different FiniteNumbers"
    );
    assert_ne!(
        number(0.0),
        number(-0.0),
        "+0.0 and -0.0 are different cell inputs"
    );
    let zeros = |s: &Session| OpBatch {
        base: AuthoredRevision::new(FIXTURE_BASE),
        operations: vec![
            set(s.sheet1, 1, 1, number(0.0)),
            set(s.sheet1, 2, 1, number(-0.0)),
        ],
    };
    let literal = format!(
        "{FIXTURE_FRAME}{},{}]}}",
        envelope(
            "cells",
            "set_cell",
            r#""sheet":1,"row":1,"column":1,"input":{"number":"0000000000000000"}"#
        ),
        envelope(
            "cells",
            "set_cell",
            r#""sheet":1,"row":2,"column":1,"input":{"number":"8000000000000000"}"#
        ),
    );
    let wire = encode_batch(&zeros(first), &first.root).expect("+0.0 and -0.0");
    assert_bytes(
        "+0.0 and -0.0 encoded",
        ("encoded", "literal"),
        &wire,
        literal.as_bytes(),
    );
    let decoded = decode_batch(literal.as_bytes(), &later.root).expect("+0.0 and -0.0");
    assert_eq!(
        describe_all(&decoded.operations),
        vec![
            format!("set {:?} R1C1 number 0000000000000000", later.sheet1),
            format!("set {:?} R2C1 number 8000000000000000", later.sheet1),
        ],
        "+0.0 and -0.0 decoded to their own bits"
    );
    let [
        WorkbookOp::SetCell { input: zero, .. },
        WorkbookOp::SetCell {
            input: negative_zero,
            ..
        },
    ] = decoded.operations.as_slice()
    else {
        panic!("two set_cell operations");
    };
    assert_ne!(
        zero, negative_zero,
        "+0.0 and -0.0 decoded are different inputs"
    );
    assert_eq!(decoded, zeros(later), "+0.0 and -0.0 decoded");
}

// ---------------------------------------------------------------------------------------------
// Case cdoc_m0_1__op_roundtrip.

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cdoc_m0_1__op_roundtrip() {
    const CASE: &str = "cdoc_m0_1__op_roundtrip";
    let first = session(0);
    let later = session(7);
    let pairs = [
        (first.sheet1, later.sheet1),
        (first.data, later.data),
        (first.archive, later.archive),
        (first.lookup, later.lookup),
    ];
    for (one, other) in pairs {
        assert_ne!(one, other, "the two sessions give a key different IDs");
    }
    note!(
        "ACCEPT-NOTE: {CASE}: the two sessions give keys 1, 2, 5 and 7 the IDs {:?} and {:?}",
        pairs.map(|(one, _)| (one.slot(), one.generation())),
        pairs.map(|(_, other)| (other.slot(), other.generation()))
    );

    // The roots this case builds, read through their accessors and compared with literal values,
    // and a root that is shared rather than copied.
    for s in [&first, &later] {
        check_fixture_root(s, "before any batch is decoded through it");
    }
    check_other_roots(&first);
    let (handles_before, handles_during) = check_root_sharing(&first, &later);
    note!(
        "ACCEPT-NOTE: {CASE}: ModelRoot::revision and ModelRoot::sheets (with each SheetEntry's id, key, name and visibility) of both sessions' fixture roots equal literal values: revision 41, keys 1, 2, 5 and 7 named Sheet1, Data, Archive and Lookup, visible, visible, hidden and very hidden; so do a root at revision 0 with one hidden sheet and a root at revision 18446744073709551615 with none; a clone of a root holds the same sheets slice and sheet_by_key returns the root's own entry (the case holds roots through std::sync::Arc: {handles_before} then {handles_during} handles), and the two sessions' roots hold separate sheets"
    );

    // The golden fixtures decode to the expected operations, through either session's root, and
    // the expected operations encode to the fixtures byte for byte.
    let mut covered = BTreeSet::new();
    let mut envelopes_seen = BTreeSet::new();
    let mut operation_count = 0;
    for (file, golden, expected_ops) in FIXTURES {
        let (format, version, base, envelopes) = inspect(golden);
        assert_eq!(
            (format.as_str(), version, base),
            (BATCH_FORMAT, u64::from(BATCH_VERSION), FIXTURE_BASE),
            "{file}: frame"
        );
        for (capability, version, op) in &envelopes {
            envelopes_seen.insert((capability.clone(), *version, op.clone()));
        }
        for s in [&first, &later] {
            let expected = OpBatch {
                base: AuthoredRevision::new(FIXTURE_BASE),
                operations: expected_ops(s),
            };
            let decoded = decode_batch(golden, &s.root)
                .unwrap_or_else(|error| panic!("{file}: refused: {error}"));
            assert_eq!(decoded.base.get(), FIXTURE_BASE, "{file}: base");
            assert_eq!(
                describe_all(&decoded.operations),
                describe_all(&expected.operations),
                "{file}: decoded operations"
            );
            assert_eq!(decoded, expected, "{file}: decoded batch");
            let encoded = encode_batch(&expected, &s.root)
                .unwrap_or_else(|error| panic!("{file}: encode refused: {error}"));
            assert_same_bytes(file, &encoded, golden);
            let again = decode_batch(&encoded, &s.root).expect("the encoder's own bytes");
            assert_eq!(
                describe_all(&again.operations),
                describe_all(&expected.operations)
            );
            for op in &decoded.operations {
                features(op, &mut covered);
            }
        }
        operation_count += envelopes.len();
        note!(
            "ACCEPT-NOTE: {CASE}: {file}: {} bytes, {} operations decoded through both sessions' roots and re-encoded byte for byte",
            golden.len(),
            envelopes.len()
        );
    }

    // The literal-value comparison, apart from the codec: the bytes of each frame and of every
    // operation equal text written here from the wire format's description. Then, through the
    // decoder, each error code's wire name decodes to that code.
    let compared = check_fixture_members();
    let total: usize = compared.iter().map(|(_, count)| count).sum();
    assert_eq!(
        ERROR_WIRE_NAMES.map(|(code, _)| code),
        WorksheetErrorCode::all(),
        "the literal table names every error code once, in order"
    );
    let decoded_cells = decode_batch(CELLS_V1, &first.root).expect("cells_v1.json");
    for (index, (code, name)) in ERROR_WIRE_NAMES.iter().enumerate() {
        let row = u32::try_from(index + 1).expect("a row");
        assert_eq!(
            decoded_cells.operations[10 + index],
            set(first.sheet1, row, 4, CellInput::Error(*code)),
            "the wire name {name}"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: the bytes of the three fixtures' frames and of all {total} of their operations {compared:?} equal text written in the test from the wire format's description, every argument of every operation included: bits 8000000000000000 (-0.0), 0000000000000001 and 7fefffffffffffff, a 32,767-unit text of 131,068 hex digits built from its rule, a text whose last unit is U+D83D, the logicals true and false, the 14 error wire names, row 1048576 and column 16384, the top, left, bottom and right of each clear (the full sheet, B2:C3 and B3:D7), every axis and sheet edit's arguments, the capability names and versions, and the sheet keys"
    );

    // Every declared capability is dispatched: for each operation each declaration names, a
    // fixture envelope of that capability, version and operation decodes on its own, and encoding
    // the decoded operation alone gives the same envelope.
    let mut dispatched = Vec::new();
    for capability in op_capabilities() {
        for op_name in capability.operations {
            let found = FIXTURES.iter().find_map(|(file, golden, _)| {
                let message: Value = serde_json::from_slice(golden).expect("a JSON fixture");
                let decoded = decode_batch(golden, &first.root).expect("a fixture");
                message["operations"]
                    .as_array()
                    .expect("operations")
                    .iter()
                    .zip(decoded.operations)
                    .find(|(envelope, _)| {
                        envelope["capability"] == json!(capability.name)
                            && envelope["version"] == json!(capability.version)
                            && envelope["op"] == json!(op_name)
                    })
                    .map(|(envelope, op)| (*file, envelope.clone(), op))
            });
            let Some((file, envelope, op)) = found else {
                panic!(
                    "no fixture envelope for {} v{} {op_name}",
                    capability.name, capability.version
                );
            };
            let alone = OpBatch {
                base: AuthoredRevision::new(FIXTURE_BASE),
                operations: vec![op],
            };
            let bytes = encode_batch(&alone, &first.root).unwrap_or_else(|error| {
                panic!("{} {op_name}: encode refused: {error}", capability.name)
            });
            let written: Value = serde_json::from_slice(&bytes).expect("JSON");
            assert_eq!(
                written["operations"],
                json!([envelope]),
                "{} {op_name}: the envelope from {file}",
                capability.name
            );
            assert_eq!(
                decode_batch(&bytes, &first.root).expect("one operation"),
                alone,
                "{} {op_name}: decoded alone",
                capability.name
            );
            dispatched.push(format!("{}/{op_name}", capability.name));
        }
    }
    note!(
        "ACCEPT-NOTE: {CASE}: every declared operation of every declared capability round-tripped on its own: {dispatched:?}"
    );

    // The fixtures cover every declared operation of every capability, and the declarations
    // name exactly the operations this case expects.
    let expected_envelopes: BTreeSet<(String, u64, String)> = [
        ("cells", "set_cell"),
        ("cells", "clear"),
        ("axes", "insert_rows"),
        ("axes", "delete_rows"),
        ("axes", "insert_columns"),
        ("axes", "delete_columns"),
        ("sheets", "insert"),
        ("sheets", "rename"),
        ("sheets", "move"),
        ("sheets", "remove"),
        ("sheets", "set_visibility"),
    ]
    .into_iter()
    .map(|(capability, op)| (capability.to_owned(), 1, op.to_owned()))
    .collect();
    let declared: BTreeSet<(String, u64, String)> = op_capabilities()
        .iter()
        .flat_map(|capability| {
            capability.operations.iter().map(|op| {
                (
                    capability.name.to_owned(),
                    u64::from(capability.version),
                    (*op).to_owned(),
                )
            })
        })
        .collect();
    assert_eq!(declared, expected_envelopes, "the declared operations");
    assert_eq!(
        envelopes_seen, expected_envelopes,
        "the fixtures' operations"
    );
    let required = required_features();
    assert_eq!(
        covered.difference(&required).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "features outside the required set"
    );
    assert_eq!(
        required.difference(&covered).collect::<Vec<_>>(),
        Vec::<&String>::new(),
        "required features the fixtures miss"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: {operation_count} fixture operations cover {} features: every WorkbookOp variant, every CellInput kind, all 4 AxisEdit kinds, all 5 SheetEdit kinds, all 3 visibilities and all {} worksheet error codes; envelopes {:?}",
        covered.len(),
        WorksheetErrorCode::all().len(),
        op_capabilities()
            .iter()
            .map(|capability| format!("{} v{}", capability.name, capability.version))
            .collect::<Vec<_>>()
    );

    // The edge values survive the round trip as their exact bits and code units.
    let decoded_cells = decode_batch(CELLS_V1, &first.root).expect("cells_v1.json");
    let observed: Vec<String> = describe_all(&decoded_cells.operations);
    for (edge, expected) in [
        ("-0.0", "number 8000000000000000".to_owned()),
        (
            "the smallest subnormal",
            "number 0000000000000001".to_owned(),
        ),
        ("the largest finite", "number 7fefffffffffffff".to_owned()),
        (
            "a 32,767-unit text",
            format!("text {}", hex_units(&longest_text())),
        ),
        ("a high-surrogate tail", "text 00410062d83d".to_owned()),
        (
            "XFD1048576",
            format!(
                "set {:?} R1048576C16384 formula {}",
                first.data,
                hex_text("LEN(\"\u{20AC}\u{1F600}\")")
            ),
        ),
    ] {
        assert!(
            observed.iter().any(|line| line.ends_with(&expected)),
            "{edge} is not among the decoded operations"
        );
    }
    let longest = decoded_cells
        .operations
        .iter()
        .find_map(|op| match op {
            WorkbookOp::SetCell {
                input: CellInput::Text(text),
                ..
            } if text.text().len_utf16_code_units() == 32_767 => Some(text.text().clone()),
            _ => None,
        })
        .expect("the 32,767-unit text");
    let tail = decoded_cells
        .operations
        .iter()
        .find_map(|op| match op {
            WorkbookOp::SetCell {
                input: CellInput::Text(text),
                ..
            } if text.text().has_dangling_high_surrogate_tail() => Some(text.text().clone()),
            _ => None,
        })
        .expect("the text ending in a high surrogate");
    let zero = decoded_cells
        .operations
        .iter()
        .find_map(|op| match op {
            WorkbookOp::SetCell {
                input: CellInput::Number(number),
                ..
            } if number.get() == 0.0 => Some(number.get()),
            _ => None,
        })
        .expect("the zero");
    assert!(zero.is_sign_negative(), "-0.0 keeps its sign");
    note!(
        "ACCEPT-NOTE: {CASE}: edges kept: -0.0 (sign negative), bits 0000000000000001 and 7fefffffffffffff, a {}-unit text, a text ending in U+{:04X}, all {} error codes, and a formula at XFD1048576",
        longest.len_utf16_code_units(),
        tail.utf16_code_units().last().copied().unwrap_or_default(),
        WorksheetErrorCode::all().len()
    );

    // AxisEdit::last and the signed zeros, against literal values.
    let (decoded_edits, built_edits) = check_axis_lasts(&first);
    check_signed_zeros(&first, &later);
    note!(
        "ACCEPT-NOTE: {CASE}: AxisEdit::last equals literal positions for the axes fixture's {decoded_edits} edits as decoded and {built_edits} edits built directly, inserts and deletes reaching row 1048576 and column 16384 among them; +0.0 and -0.0 are different FiniteNumbers and different inputs, encode to literal wire text with bits 0000000000000000 and 8000000000000000, and decode from it to those bits"
    );

    // A batch of every fixture operation, based on the last revision, encoded through one
    // session's root and decoded through the other's: the wire carries keys, not IDs.
    let mixed = |s: &Session| OpBatch {
        base: AuthoredRevision::new(u64::MAX),
        operations: FIXTURES.iter().flat_map(|(_, _, ops)| ops(s)).collect(),
    };
    let wire = encode_batch(&mixed(&first), &first.root).expect("the mixed batch");
    let in_later = decode_batch(&wire, &later.root).expect("the mixed batch, in the later session");
    assert_eq!(in_later, mixed(&later));
    assert_eq!(
        describe_all(&in_later.operations),
        describe_all(&mixed(&later).operations)
    );
    assert_eq!(in_later.base.get(), u64::MAX);
    assert_eq!(
        decode_batch(&wire, &first.root).expect("the mixed batch"),
        mixed(&first)
    );
    assert_eq!(
        encode_batch(&in_later, &later.root).expect("the mixed batch, re-encoded"),
        wire,
        "the same batch encodes to the same bytes in either session"
    );
    assert_eq!(AuthoredRevision::new(u64::MAX).checked_next(), None);
    assert_eq!(
        AuthoredRevision::new(FIXTURE_BASE).checked_next(),
        Some(AuthoredRevision::new(FIXTURE_BASE + 1))
    );
    assert_eq!(AuthoredRevision::INITIAL.get(), 0);
    note!(
        "ACCEPT-NOTE: {CASE}: a {}-operation batch based on revision {} was encoded in one session ({} bytes), decoded in the other with that session's IDs, and re-encoded to the same bytes",
        in_later.operations.len(),
        u64::MAX,
        wire.len()
    );

    // Formatting is not part of the wire contract: the same message with whitespace and its keys
    // in another order decodes to the same operations, and re-encodes to the canonical bytes.
    for (file, golden, expected_ops) in FIXTURES {
        let reordered = serde_json::to_vec_pretty(
            &serde_json::from_slice::<Value>(golden).expect("a JSON fixture"),
        )
        .expect("a JSON value");
        assert_ne!(reordered.as_slice(), golden);
        let decoded = decode_batch(&reordered, &first.root).expect("the reformatted fixture");
        assert_eq!(
            decoded.operations,
            expected_ops(&first),
            "{file}: reformatted"
        );
        assert_same_bytes(
            file,
            &encode_batch(&decoded, &first.root).expect("re-encoded"),
            golden,
        );
    }

    // A newer version or an unknown capability is refused, and so is every other message the
    // codec does not admit. The rows that change one frame or envelope member start from a frame
    // and an envelope that decode.
    let control = OpBatch {
        base: AuthoredRevision::new(FIXTURE_BASE),
        operations: vec![set(first.sheet1, 1, 1, CellInput::Logical(true))],
    };
    assert_eq!(
        decode_batch(&frame_with(None), &first.root),
        Ok(control.clone()),
        "the frame the frame rows change"
    );
    assert_eq!(
        decode_batch(&batch_of(&[one_envelope(None)]), &first.root),
        Ok(control),
        "the envelope the envelope rows change"
    );
    let refusals = refusal_table();
    for (label, bytes, expected) in &refusals {
        assert_refused(label, bytes, &first.root, expected);
    }
    let kinds: BTreeSet<&str> = refusals
        .iter()
        .map(|(_, _, error)| refusal_kind(error))
        .collect();
    note!(
        "ACCEPT-NOTE: {CASE}: {} refused messages, of {} kinds: {:?}",
        refusals.len(),
        kinds.len(),
        kinds
    );
    for (label, bytes) in [
        ("trailing text", b"{} x".to_vec()),
        ("an empty message", Vec::new()),
        ("an unclosed array", b"[".to_vec()),
        (
            "invalid UTF-8 in a string",
            b"{\"format\":\"\xff\"}".to_vec(),
        ),
    ] {
        match decode_batch(&bytes, &first.root) {
            Err(DecodeError::Syntax { detail }) => assert!(!detail.is_empty(), "{label}"),
            other => panic!("{label}: {other:?}"),
        }
    }

    // Only an empty formula text is refused (the refusal rows above). Formula syntax is the
    // parser's, so a text that begins with + or -, holds = after its start, or itself begins with
    // = is kept as given.
    let formula_text =
        |source: &str| FormulaText::new(ExcelText::from_utf16_code_units(units(source)));
    assert_eq!(
        formula_text("=A1").map(|kept| kept.text().utf16_code_units().to_vec()),
        Ok(units("=A1")),
        "FormulaText::new keeps a text that begins with = as given"
    );
    assert_eq!(formula_text(""), Err(FormulaTextError::Empty));
    let mut admitted = Vec::new();
    for source in ["+A1", "-1", "A1=1", "=A1", "=="] {
        let bytes = set_cell_with(&format!("\"formula\":\"{}\"", hex_text(source)));
        let batch = decode_batch(&bytes, &first.root)
            .unwrap_or_else(|error| panic!("the formula {source}: {error}"));
        assert_eq!(
            batch.operations,
            [set(first.sheet1, 1, 1, formula(source))],
            "the formula {source}"
        );
        admitted.push(source);
    }

    // The refusals' messages state the refusal.
    let messages = [
        (SheetNameError::Empty.to_string(), "the sheet name is empty"),
        (
            SheetNameError::ForbiddenCharacter { unit: 0x3A }.to_string(),
            "the sheet name contains U+003A, which a sheet name may not contain",
        ),
        (
            SheetNameError::BoundaryApostrophe.to_string(),
            "the sheet name begins or ends with an apostrophe",
        ),
        (
            AxisEditError::ZeroCount.to_string(),
            "the axis edit covers no position",
        ),
        (
            FormulaTextError::Empty.to_string(),
            "the formula text is empty",
        ),
        (
            DecodeError::InvalidFormula {
                at: at(".args.input.formula"),
                error: FormulaTextError::Empty,
            }
            .to_string(),
            "$.operations[0].args.input.formula: the formula text is empty",
        ),
        (
            DecodeError::InvalidSheetName {
                at: at(".args.name"),
                error: SheetNameError::Empty,
            }
            .to_string(),
            "$.operations[0].args.name: the sheet name is empty",
        ),
    ];
    for (shown, expected) in &messages {
        assert_eq!(shown, expected);
    }
    note!(
        "ACCEPT-NOTE: {CASE}: FormulaText::new refused the empty text (Empty) and kept =A1 as given; the formulas {admitted:?} decoded as given; {} refusal messages equal their literal text",
        messages.len()
    );

    // The decoder refuses an unknown sheet key in every declared operation that names an
    // existing sheet (all but insert, which names a new key; an insert of a key in use is
    // refused in the table above).
    let unknown_key_ops: BTreeSet<String> = unknown_key_rows()
        .into_iter()
        .map(|(op, _, _)| op.to_owned())
        .collect();
    let naming_ops: BTreeSet<String> = op_capabilities()
        .iter()
        .flat_map(|capability| capability.operations.iter())
        .filter(|op| **op != "insert")
        .map(|op| (*op).to_owned())
        .collect();
    assert_eq!(
        unknown_key_ops, naming_ops,
        "an unknown-key refusal for every operation that names a sheet"
    );

    // The encoder refuses what has no wire form: an operation of any kind that names a sheet the
    // root does not hold (no other key stands in for its key), and an inserted key already in
    // use.
    let stranger = first.stranger;
    let unknown_sheet = [
        ("set_cell", set(stranger, 1, 1, CellInput::Logical(true))),
        (
            "clear",
            WorkbookOp::Clear {
                area: GridArea::full_sheet(stranger),
            },
        ),
        (
            "insert_rows",
            axis(stranger, Axis::Rows, AxisChange::Insert, 1, 1),
        ),
        (
            "delete_rows",
            axis(stranger, Axis::Rows, AxisChange::Delete, 2, 3),
        ),
        (
            "insert_columns",
            axis(stranger, Axis::Columns, AxisChange::Insert, 1, 1),
        ),
        (
            "delete_columns",
            axis(stranger, Axis::Columns, AxisChange::Delete, 4, 1),
        ),
        (
            "rename",
            WorkbookOp::EditSheet(SheetEdit::Rename {
                sheet: stranger,
                name: name("Renamed"),
            }),
        ),
        (
            "move",
            WorkbookOp::EditSheet(SheetEdit::Move {
                sheet: stranger,
                position: position(1),
            }),
        ),
        (
            "remove",
            WorkbookOp::EditSheet(SheetEdit::Remove { sheet: stranger }),
        ),
        (
            "set_visibility",
            WorkbookOp::EditSheet(SheetEdit::SetVisibility {
                sheet: stranger,
                visibility: SheetVisibility::Hidden,
            }),
        ),
    ];
    for (label, op) in &unknown_sheet {
        // Alone, and after an operation the root can encode.
        for operations in [
            vec![op.clone()],
            vec![
                set(first.sheet1, 1, 1, CellInput::Logical(true)),
                op.clone(),
            ],
        ] {
            let batch = OpBatch {
                base: AuthoredRevision::new(FIXTURE_BASE),
                operations,
            };
            assert_eq!(
                encode_batch(&batch, &first.root),
                Err(EncodeError::UnknownSheet { sheet: stranger }),
                "{label} on a sheet the root does not hold"
            );
        }
    }
    assert_eq!(
        unknown_sheet
            .iter()
            .map(|(label, _)| (*label).to_owned())
            .collect::<BTreeSet<_>>(),
        naming_ops,
        "an encoder refusal for every operation that names a sheet"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: an unknown sheet was refused by the encoder (EncodeError::UnknownSheet) and an unknown key by the decoder (DecodeError::UnknownSheetKey) in each of {} operations: {:?}",
        naming_ops.len(),
        naming_ops
    );

    // Sheet names: 31 UTF-16 code units is the longest admitted, and it round-trips; 32 and each
    // of the 7 forbidden characters are refused (the table above).
    assert_eq!(SHEET_NAME_MAX_UTF16_CODE_UNITS, 31);
    let longest_name = format!("{}\u{20AC}", "Q".repeat(30));
    assert_eq!(units(&longest_name).len(), 31);
    let rename_longest = |s: &Session| OpBatch {
        base: AuthoredRevision::new(FIXTURE_BASE),
        operations: vec![WorkbookOp::EditSheet(SheetEdit::Rename {
            sheet: s.data,
            name: name(&longest_name),
        })],
    };
    let wire = encode_batch(&rename_longest(&first), &first.root).expect("a 31-unit name");
    let written: Value = serde_json::from_slice(&wire).expect("JSON");
    assert_eq!(
        written["operations"][0]["args"],
        json!({"sheet": 2, "name": format!("{}20ac", "0051".repeat(30))}),
        "the 31-unit name as its code units"
    );
    assert_eq!(
        decode_batch(&wire, &later.root).expect("a 31-unit name"),
        rename_longest(&later)
    );
    let literal = batch_of(&[envelope(
        "sheets",
        "rename",
        &format!("\"sheet\":2,\"name\":\"{}\"", hex_text(&longest_name)),
    )]);
    assert_eq!(
        decode_batch(&literal, &first.root).expect("a 31-unit name"),
        rename_longest(&first)
    );
    note!(
        "ACCEPT-NOTE: {CASE}: a 31-unit sheet name was encoded, decoded in the other session and decoded from a literal message"
    );

    let insert = |key: u32| {
        WorkbookOp::EditSheet(SheetEdit::Insert {
            key: SheetKey::new(key),
            name: name("New"),
            position: position(1),
        })
    };
    for (operations, key) in [(vec![insert(5)], 5), (vec![insert(9), insert(9)], 9)] {
        let batch = OpBatch {
            base: AuthoredRevision::new(FIXTURE_BASE),
            operations,
        };
        assert_eq!(
            encode_batch(&batch, &first.root),
            Err(EncodeError::SheetKeyInUse {
                key: SheetKey::new(key)
            })
        );
    }

    // A root maps each key to one sheet and each ID to one key.
    let entry =
        |id, key| SheetEntry::new(id, SheetKey::new(key), name("S"), SheetVisibility::Visible);
    assert_eq!(
        ModelRoot::new(
            AuthoredRevision::INITIAL,
            vec![entry(first.sheet1, 1), entry(first.data, 1)]
        ),
        Err(ModelRootError::DuplicateSheetKey(SheetKey::new(1)))
    );
    assert_eq!(
        ModelRoot::new(
            AuthoredRevision::INITIAL,
            vec![entry(first.sheet1, 1), entry(first.sheet1, 2)]
        ),
        Err(ModelRootError::DuplicateSheetId(first.sheet1))
    );
    note!(
        "ACCEPT-NOTE: {CASE}: the encoder refused a sheet outside the root and two in-use insert keys; the root refused a duplicate key and a duplicate ID"
    );

    // Decoding every batch above through the two roots, the sheets fixture's insert, rename,
    // visibility changes and remove among them, left both roots unchanged.
    for s in [&first, &later] {
        check_fixture_root(s, "after every batch of the case was decoded through it");
    }
    note!(
        "ACCEPT-NOTE: {CASE}: after every batch of the case was decoded through them, both fixture roots still equal the same literal revision and sheets"
    );
}

fn refusal_kind(error: &DecodeError) -> &'static str {
    match error {
        DecodeError::Syntax { .. } => "Syntax",
        DecodeError::UnknownFormat { .. } => "UnknownFormat",
        DecodeError::UnsupportedVersion { .. } => "UnsupportedVersion",
        DecodeError::UnknownCapability { .. } => "UnknownCapability",
        DecodeError::UnknownOperation { .. } => "UnknownOperation",
        DecodeError::UnknownField { .. } => "UnknownField",
        DecodeError::DuplicateField { .. } => "DuplicateField",
        DecodeError::MissingField { .. } => "MissingField",
        DecodeError::Malformed { .. } => "Malformed",
        DecodeError::NonFiniteNumber { .. } => "NonFiniteNumber",
        DecodeError::TextTooLong { .. } => "TextTooLong",
        DecodeError::InvalidFormula { .. } => "InvalidFormula",
        DecodeError::InvalidSheetName { .. } => "InvalidSheetName",
        DecodeError::UnknownSheetKey { .. } => "UnknownSheetKey",
        DecodeError::SheetKeyInUse { .. } => "SheetKeyInUse",
        DecodeError::InvalidCell { .. } => "InvalidCell",
        DecodeError::InvertedArea { .. } => "InvertedArea",
        DecodeError::InvalidAxisEdit { .. } => "InvalidAxisEdit",
    }
}

/// For each operation that names an existing sheet, a one-operation message whose arguments are
/// valid except that the sheet key 3 is not a key of the fixture root.
fn unknown_key_rows() -> Vec<(&'static str, &'static str, Vec<u8>)> {
    let renamed = hex_text("Renamed");
    [
        (
            "cells",
            "set_cell",
            "\"sheet\":3,\"row\":1,\"column\":1,\"input\":{\"logical\":true}".to_owned(),
        ),
        (
            "cells",
            "clear",
            "\"sheet\":3,\"top\":1,\"left\":1,\"bottom\":2,\"right\":2".to_owned(),
        ),
        (
            "axes",
            "insert_rows",
            "\"sheet\":3,\"first\":1,\"count\":1".to_owned(),
        ),
        (
            "axes",
            "delete_rows",
            "\"sheet\":3,\"first\":2,\"count\":3".to_owned(),
        ),
        (
            "axes",
            "insert_columns",
            "\"sheet\":3,\"first\":1,\"count\":1".to_owned(),
        ),
        (
            "axes",
            "delete_columns",
            "\"sheet\":3,\"first\":4,\"count\":1".to_owned(),
        ),
        (
            "sheets",
            "rename",
            format!("\"sheet\":3,\"name\":\"{renamed}\""),
        ),
        ("sheets", "move", "\"sheet\":3,\"position\":1".to_owned()),
        ("sheets", "remove", "\"sheet\":3".to_owned()),
        (
            "sheets",
            "set_visibility",
            "\"sheet\":3,\"visibility\":\"hidden\"".to_owned(),
        ),
    ]
    .into_iter()
    .map(|(capability, op, args)| {
        (
            op,
            "an unknown sheet key",
            batch_of(&[envelope(capability, op, &args)]),
        )
    })
    .collect()
}

/// Messages the codec refuses, each with the exact refusal, all against the fixture root.
fn refusal_table() -> Vec<(String, Vec<u8>, DecodeError)> {
    let mut out: Vec<(String, Vec<u8>, DecodeError)> = Vec::new();
    let mut add = |label: &str, bytes: Vec<u8>, error: DecodeError| {
        out.push((label.to_owned(), bytes, error));
    };
    let malformed = |path: &str, expected: &'static str| DecodeError::Malformed {
        at: at(path),
        expected,
    };
    let sheet_name = |error| DecodeError::InvalidSheetName {
        at: at(".args.name"),
        error,
    };

    // Versions and envelopes. The format and the version are judged before the other members,
    // so a newer frame or envelope that adds a member is refused for its version, and a foreign
    // format for its format.
    let frame_version = |found: u64| DecodeError::UnsupportedVersion {
        at: "$.version".to_owned(),
        envelope: BATCH_FORMAT.to_owned(),
        found,
        supported: 1,
    };
    add(
        "a newer batch frame",
        replace_first(CELLS_V1, "\"version\":1,\"base\"", "\"version\":2,\"base\""),
        frame_version(2),
    );
    add(
        "an older batch frame (version 0)",
        replace_first(CELLS_V1, "\"version\":1,\"base\"", "\"version\":0,\"base\""),
        frame_version(0),
    );
    add(
        "a newer batch frame with a member it adds",
        replace_first(
            SHEETS_V1,
            "\"version\":1,\"base\":41,",
            "\"version\":2,\"base\":41,\"undo_group\":3,",
        ),
        frame_version(2),
    );
    add(
        "a newer batch frame with a member it adds before its format",
        replace_first(
            AXES_V1,
            "{\"format\":\"dnacalc/workbook-op-batch\",\"version\":1,",
            "{\"source\":\"ui\",\"format\":\"dnacalc/workbook-op-batch\",\"version\":2,",
        ),
        frame_version(2),
    );
    add(
        "a foreign format with a member of its own",
        replace_first(
            AXES_V1,
            "{\"format\":\"dnacalc/workbook-op-batch\"",
            "{\"sheetData\":[],\"format\":\"dnacalc/other\"",
        ),
        DecodeError::UnknownFormat {
            at: "$.format".to_owned(),
            format: "dnacalc/other".to_owned(),
        },
    );
    add(
        "a foreign format at version 7",
        replace_first(
            CELLS_V1,
            "{\"format\":\"dnacalc/workbook-op-batch\",\"version\":1,",
            "{\"format\":\"dnacalc/other\",\"version\":7,",
        ),
        DecodeError::UnknownFormat {
            at: "$.format".to_owned(),
            format: "dnacalc/other".to_owned(),
        },
    );
    for (capability, golden) in [
        ("cells", CELLS_V1),
        ("axes", AXES_V1),
        ("sheets", SHEETS_V1),
    ] {
        add(
            &format!("{capability} version 2 with a member it adds"),
            replace_first(
                golden,
                &format!("\"capability\":\"{capability}\",\"version\":1,"),
                &format!("\"capability\":\"{capability}\",\"version\":2,\"priority\":1,"),
            ),
            DecodeError::UnsupportedVersion {
                at: at(".version"),
                envelope: capability.to_owned(),
                found: 2,
                supported: 1,
            },
        );
    }
    for (capability, golden) in [
        ("cells", CELLS_V1),
        ("axes", AXES_V1),
        ("sheets", SHEETS_V1),
    ] {
        for found in [2u64, 0] {
            add(
                &format!("{capability} version {found}"),
                replace_first(
                    golden,
                    &format!("\"capability\":\"{capability}\",\"version\":1"),
                    &format!("\"capability\":\"{capability}\",\"version\":{found}"),
                ),
                DecodeError::UnsupportedVersion {
                    at: at(".version"),
                    envelope: capability.to_owned(),
                    found,
                    supported: 1,
                },
            );
        }
    }
    add(
        "an unknown capability",
        replace_first(
            CELLS_V1,
            "\"capability\":\"cells\"",
            "\"capability\":\"names\"",
        ),
        DecodeError::UnknownCapability {
            at: at(".capability"),
            capability: "names".to_owned(),
        },
    );
    add(
        "an unknown format",
        replace_first(AXES_V1, BATCH_FORMAT, "dnacalc/other"),
        DecodeError::UnknownFormat {
            at: "$.format".to_owned(),
            format: "dnacalc/other".to_owned(),
        },
    );
    add(
        "an unknown operation",
        batch_of(&[envelope("cells", "fill", "\"sheet\":1")]),
        DecodeError::UnknownOperation {
            at: at(".op"),
            capability: "cells",
            operation: "fill".to_owned(),
        },
    );
    add(
        "an unknown argument",
        replace_first(AXES_V1, "\"count\":2}", "\"count\":2,\"style\":1}"),
        DecodeError::UnknownField {
            at: at(".args"),
            field: "style".to_owned(),
        },
    );
    add(
        "an unknown envelope member",
        replace_first(
            SHEETS_V1,
            "{\"capability\"",
            "{\"source\":\"ui\",\"capability\"",
        ),
        DecodeError::UnknownField {
            at: OPS0.to_owned(),
            field: "source".to_owned(),
        },
    );
    add(
        "an unknown frame member",
        replace_first(SHEETS_V1, "\"base\":41,", "\"base\":41,\"undo_group\":3,"),
        DecodeError::UnknownField {
            at: "$".to_owned(),
            field: "undo_group".to_owned(),
        },
    );
    add(
        "a duplicate argument",
        replace_first(AXES_V1, "\"first\":3,", "\"first\":3,\"first\":3,"),
        DecodeError::DuplicateField {
            at: at(".args"),
            field: "first".to_owned(),
        },
    );
    add(
        "a missing argument",
        replace_first(AXES_V1, ",\"count\":2", ""),
        DecodeError::MissingField {
            at: at(".args"),
            field: "count".to_owned(),
        },
    );
    add(
        "a missing envelope member",
        batch_of(&["{\"capability\":\"cells\",\"op\":\"clear\",\"args\":{}}".to_owned()]),
        DecodeError::MissingField {
            at: OPS0.to_owned(),
            field: "version".to_owned(),
        },
    );
    // A duplicate format, version or capability is refused by the reader that judges that member
    // before the other members of its object. Each of these messages also carries a member its
    // object does not declare, which the check of the other members would report first (as
    // UnknownField), so each refusal shows that the first reader refused the duplicate.
    add(
        "a duplicate frame format",
        format!(
            "{{\"format\":\"{BATCH_FORMAT}\",\"undo_group\":3,\"format\":\"{BATCH_FORMAT}\",\"version\":1,\"base\":{FIXTURE_BASE},\"operations\":[]}}"
        )
        .into_bytes(),
        DecodeError::DuplicateField {
            at: "$".to_owned(),
            field: "format".to_owned(),
        },
    );
    add(
        "a duplicate frame version",
        format!(
            "{{\"format\":\"{BATCH_FORMAT}\",\"version\":1,\"undo_group\":3,\"version\":1,\"base\":{FIXTURE_BASE},\"operations\":[]}}"
        )
        .into_bytes(),
        DecodeError::DuplicateField {
            at: "$".to_owned(),
            field: "version".to_owned(),
        },
    );
    let clear_a1 =
        "\"op\":\"clear\",\"args\":{\"sheet\":1,\"top\":1,\"left\":1,\"bottom\":1,\"right\":1}";
    add(
        "a duplicate envelope capability",
        batch_of(&[format!(
            "{{\"capability\":\"cells\",\"priority\":1,\"capability\":\"cells\",\"version\":1,{clear_a1}}}"
        )]),
        DecodeError::DuplicateField {
            at: OPS0.to_owned(),
            field: "capability".to_owned(),
        },
    );
    add(
        "a duplicate envelope version",
        batch_of(&[format!(
            "{{\"capability\":\"cells\",\"version\":1,\"priority\":1,\"version\":1,{clear_a1}}}"
        )]),
        DecodeError::DuplicateField {
            at: OPS0.to_owned(),
            field: "version".to_owned(),
        },
    );
    // serde_json hands this number to the codec as a number only while it is built without its
    // arbitrary_precision feature (see the codec's JSON layer); with that feature it arrives as a
    // one-member object and this refusal changes.
    add(
        "a fractional number in place of an operation's arguments",
        batch_of(&[
            "{\"capability\":\"cells\",\"version\":1,\"op\":\"clear\",\"args\":1.5}".to_owned(),
        ]),
        malformed(".args", "an object"),
    );
    add(
        "operations that are not an array",
        format!(
            "{{\"format\":\"{BATCH_FORMAT}\",\"version\":1,\"base\":{FIXTURE_BASE},\"operations\":{{}}}}"
        )
        .into_bytes(),
        DecodeError::Malformed {
            at: "$.operations".to_owned(),
            expected: "an array of operations",
        },
    );

    // Every frame member is required and has one type: a missing member is refused, never given
    // a default, and so is a member of another type, a negative or fractional integer included.
    let u64_integer = "an integer from 0 to 18446744073709551615";
    let lone_envelope = one_envelope(None);
    let missing = |field: &str| DecodeError::MissingField {
        at: "$".to_owned(),
        field: field.to_owned(),
    };
    let frame_malformed = |member: &str, expected: &'static str| DecodeError::Malformed {
        at: format!("$.{member}"),
        expected,
    };
    let frame_rows: [(&str, &str, Option<&str>, DecodeError); 17] = [
        ("no format", "format", None, missing("format")),
        (
            "a format that is a number",
            "format",
            Some("1"),
            frame_malformed("format", "a string"),
        ),
        (
            "a format that is null",
            "format",
            Some("null"),
            frame_malformed("format", "a string"),
        ),
        (
            "a format in capitals",
            "format",
            Some("\"DNACALC/WORKBOOK-OP-BATCH\""),
            DecodeError::UnknownFormat {
                at: "$.format".to_owned(),
                format: "DNACALC/WORKBOOK-OP-BATCH".to_owned(),
            },
        ),
        ("no version", "version", None, missing("version")),
        (
            "a version that is a string",
            "version",
            Some("\"1\""),
            frame_malformed("version", u64_integer),
        ),
        (
            "a fractional version",
            "version",
            Some("1.0"),
            frame_malformed("version", u64_integer),
        ),
        (
            "a negative version",
            "version",
            Some("-1"),
            frame_malformed("version", u64_integer),
        ),
        ("no base", "base", None, missing("base")),
        (
            "a base that is a string",
            "base",
            Some("\"41\""),
            frame_malformed("base", u64_integer),
        ),
        (
            "a fractional base",
            "base",
            Some("41.5"),
            frame_malformed("base", u64_integer),
        ),
        (
            "a negative base",
            "base",
            Some("-41"),
            frame_malformed("base", u64_integer),
        ),
        (
            "base -0",
            "base",
            Some("-0"),
            frame_malformed("base", u64_integer),
        ),
        (
            "a base of 2^64",
            "base",
            Some("18446744073709551616"),
            frame_malformed("base", u64_integer),
        ),
        ("no operations", "operations", None, missing("operations")),
        (
            "operations that are null",
            "operations",
            Some("null"),
            frame_malformed("operations", "an array of operations"),
        ),
        (
            "operations that are one envelope, not an array",
            "operations",
            Some(lone_envelope.as_str()),
            frame_malformed("operations", "an array of operations"),
        ),
    ];
    for (label, member, value, error) in frame_rows {
        add(
            &format!("frame row: {label}"),
            frame_with(Some((member, value))),
            error,
        );
    }

    // Every envelope member is required and has one type, likewise.
    let missing_member = |field: &str| DecodeError::MissingField {
        at: OPS0.to_owned(),
        field: field.to_owned(),
    };
    let envelope_rows: [(&str, &str, Option<&str>, DecodeError); 13] = [
        (
            "no capability",
            "capability",
            None,
            missing_member("capability"),
        ),
        (
            "a capability that is a number",
            "capability",
            Some("1"),
            malformed(".capability", "a string"),
        ),
        ("no version", "version", None, missing_member("version")),
        (
            "a version that is a string",
            "version",
            Some("\"1\""),
            malformed(".version", u64_integer),
        ),
        (
            "a fractional version",
            "version",
            Some("1.0"),
            malformed(".version", u64_integer),
        ),
        (
            "a negative version",
            "version",
            Some("-1"),
            malformed(".version", u64_integer),
        ),
        ("no op", "op", None, missing_member("op")),
        (
            "an op that is a number",
            "op",
            Some("1"),
            malformed(".op", "a string"),
        ),
        (
            "an op that is null",
            "op",
            Some("null"),
            malformed(".op", "a string"),
        ),
        ("no args", "args", None, missing_member("args")),
        (
            "args that are an array",
            "args",
            Some("[]"),
            malformed(".args", "an object"),
        ),
        (
            "args that are a string",
            "args",
            Some("\"sheet=1\""),
            malformed(".args", "an object"),
        ),
        (
            "args that are null",
            "args",
            Some("null"),
            malformed(".args", "an object"),
        ),
    ];
    for (label, member, value, error) in envelope_rows {
        add(
            &format!("envelope row: {label}"),
            batch_of(&[one_envelope(Some((member, value)))]),
            error,
        );
    }
    add(
        "envelope row: an envelope that is not an object",
        batch_of(&["\"cells\"".to_owned()]),
        DecodeError::Malformed {
            at: OPS0.to_owned(),
            expected: "an object",
        },
    );

    // Names are spelled exactly, case included, and hex digits are lowercase, unsigned and of
    // their exact width.
    for visibility in [
        "Visible",
        "VISIBLE",
        "Hidden",
        "veryHidden",
        "Very_Hidden",
        "very-hidden",
        "very hidden",
        " visible",
        "visible ",
    ] {
        add(
            &format!("spelling row: visibility {visibility:?}"),
            batch_of(&[envelope(
                "sheets",
                "set_visibility",
                &format!("\"sheet\":2,\"visibility\":\"{visibility}\""),
            )]),
            malformed(
                ".args.visibility",
                "\"visible\", \"hidden\" or \"very_hidden\"",
            ),
        );
    }
    for name in [
        "NA",
        "Na",
        "Div0",
        "DIV0",
        "GETTING_DATA",
        "Getting_Data",
        "n/a",
        "div/0",
        "#DIV/0!",
        "#NULL!",
        "getting-data",
        "gettingdata",
        " na",
    ] {
        add(
            &format!("spelling row: error {name:?}"),
            set_cell_with(&format!("\"error\":\"{name}\"")),
            malformed(
                ".args.input.error",
                "the wire name of a worksheet error code",
            ),
        );
    }
    let set_a1 = "\"sheet\":1,\"row\":1,\"column\":1,\"input\":{\"logical\":true}";
    for capability in ["Cells", "CELLS", "cell", "Sheets", "AXES", " cells"] {
        add(
            &format!("spelling row: capability {capability:?}"),
            batch_of(&[envelope(capability, "set_cell", set_a1)]),
            DecodeError::UnknownCapability {
                at: at(".capability"),
                capability: capability.to_owned(),
            },
        );
    }
    for (capability, operation, args) in [
        ("cells", "SET_CELL", set_a1),
        ("cells", "Set_Cell", set_a1),
        ("cells", "setCell", set_a1),
        ("cells", "set-cell", set_a1),
        ("axes", "insertRows", "\"sheet\":1,\"first\":1,\"count\":1"),
        ("axes", "Insert_Rows", "\"sheet\":1,\"first\":1,\"count\":1"),
        (
            "sheets",
            "setVisibility",
            "\"sheet\":2,\"visibility\":\"hidden\"",
        ),
        ("sheets", "REMOVE", "\"sheet\":7"),
    ] {
        add(
            &format!("spelling row: operation {operation:?}"),
            batch_of(&[envelope(capability, operation, args)]),
            DecodeError::UnknownOperation {
                at: at(".op"),
                capability,
                operation: operation.to_owned(),
            },
        );
    }
    let code_units = "4 lowercase hex digits per UTF-16 code unit";
    for units in [
        "004A", "+041", "00041", "004a004A", "0x41", "-041", " 041", "0041 ",
    ] {
        add(
            &format!("spelling row: text unit {units:?}"),
            set_cell_with(&format!("\"text\":\"{units}\"")),
            malformed(".args.input.text", code_units),
        );
    }
    add(
        "spelling row: formula unit \"+041\"",
        set_cell_with("\"formula\":\"+041\""),
        malformed(".args.input.formula", code_units),
    );
    add(
        "spelling row: sheet name unit \"004E\"",
        batch_of(&[envelope(
            "sheets",
            "rename",
            "\"sheet\":2,\"name\":\"004E\"",
        )]),
        malformed(".args.name", code_units),
    );
    let bits = "16 lowercase hex digits of binary64 bits";
    for (label, spelled) in [
        ("bits with a + sign", "+3ff000000000000"),
        ("bits with a - sign", "-3ff000000000000"),
        ("mixed-case bits", "3fF0000000000000"),
        ("17 hex digits", "03ff0000000000000"),
        ("bits with a 0x prefix", "0x3ff00000000000"),
        ("bits with a leading space", " 3ff000000000000"),
    ] {
        add(
            &format!("spelling row: {label}"),
            set_cell_with(&format!("\"number\":\"{spelled}\"")),
            malformed(".args.input.number", bits),
        );
    }

    // Values outside their type.
    let integer = "an integer from 0 to 4294967295";
    for (label, row) in [
        ("a row as a string", "\"1\""),
        ("a row past u32", "4294967296"),
        ("a negative row", "-1"),
        ("a fractional row", "1.0"),
    ] {
        add(
            label,
            batch_of(&[envelope(
                "cells",
                "set_cell",
                &format!("\"sheet\":1,\"row\":{row},\"column\":1,\"input\":{{\"logical\":true}}"),
            )]),
            malformed(".args.row", integer),
        );
    }
    for (label, input) in [
        ("uppercase bits", "\"number\":\"3FF0000000000000\""),
        ("15 hex digits", "\"number\":\"3ff000000000000\""),
        ("a decimal number", "\"number\":1.5"),
    ] {
        add(
            label,
            set_cell_with(input),
            malformed(".args.input.number", bits),
        );
    }
    add(
        "text of 3 hex digits",
        set_cell_with("\"text\":\"004\""),
        malformed(".args.input.text", code_units),
    );
    add(
        "text as a JSON string",
        set_cell_with("\"text\":\"hello\""),
        malformed(".args.input.text", code_units),
    );
    add(
        "a logical as a string",
        set_cell_with("\"logical\":\"true\""),
        malformed(".args.input.logical", "true or false"),
    );
    add(
        "an error by its display text",
        set_cell_with("\"error\":\"#N/A\""),
        malformed(
            ".args.input.error",
            "the wire name of a worksheet error code",
        ),
    );
    add(
        "an input with no kind",
        set_cell_with(""),
        malformed(
            ".args.input",
            "exactly one of number, text, logical, error or formula",
        ),
    );
    add(
        "an unknown visibility",
        replace_first(SHEETS_V1, "\"visible\"", "\"shown\""),
        DecodeError::Malformed {
            at: "$.operations[3].args.visibility".to_owned(),
            expected: "\"visible\", \"hidden\" or \"very_hidden\"",
        },
    );
    add(
        "position 0",
        replace_first(SHEETS_V1, "\"position\":3", "\"position\":0"),
        malformed(".args.position", "an integer from 1 to 4294967295"),
    );
    for raw in [
        0x7FF0_0000_0000_0000u64,
        0xFFF0_0000_0000_0000,
        0x7FF8_0000_0000_0000,
        0x7FF0_0000_0000_0001,
        0xFFFF_FFFF_FFFF_FFFF,
    ] {
        add(
            &format!("bits {raw:016x}"),
            set_cell_with(&format!("\"number\":\"{raw:016x}\"")),
            DecodeError::NonFiniteNumber {
                at: at(".args.input.number"),
                bits: raw,
            },
        );
    }
    let too_long: String = "0041".repeat(32_768);
    add(
        "text of 32,768 units",
        set_cell_with(&format!("\"text\":\"{too_long}\"")),
        DecodeError::TextTooLong {
            at: at(".args.input.text"),
            units: 32_768,
        },
    );
    add(
        "an empty formula",
        set_cell_with("\"formula\":\"\""),
        DecodeError::InvalidFormula {
            at: at(".args.input.formula"),
            error: FormulaTextError::Empty,
        },
    );
    // Sheet names.
    let rename = |units: &str| {
        batch_of(&[envelope(
            "sheets",
            "rename",
            &format!("\"sheet\":2,\"name\":\"{units}\""),
        )])
    };
    add(
        "an empty name",
        rename(""),
        sheet_name(SheetNameError::Empty),
    );
    for (character, unit) in [
        (':', 0x3A),
        ('\\', 0x5C),
        ('/', 0x2F),
        ('?', 0x3F),
        ('*', 0x2A),
        ('[', 0x5B),
        (']', 0x5D),
    ] {
        add(
            &format!("a name with {character}"),
            rename(&hex_text(&format!("a{character}b"))),
            sheet_name(SheetNameError::ForbiddenCharacter { unit }),
        );
    }
    add(
        "a 32-unit name",
        rename(&hex_text(&"n".repeat(32))),
        sheet_name(SheetNameError::TooLong { units: 32 }),
    );
    add(
        "a name beginning with an apostrophe",
        rename(&hex_text("'x")),
        sheet_name(SheetNameError::BoundaryApostrophe),
    );
    add(
        "a name ending with an apostrophe",
        rename(&hex_text("x'")),
        sheet_name(SheetNameError::BoundaryApostrophe),
    );

    // Sheet keys resolved through the receiving root.
    for (op, label, bytes) in unknown_key_rows() {
        add(
            &format!("{label} ({op})"),
            bytes,
            DecodeError::UnknownSheetKey {
                at: at(".args.sheet"),
                key: SheetKey::new(3),
            },
        );
    }
    add(
        "an unknown key in an axis edit",
        replace_first(AXES_V1, "\"sheet\":1", "\"sheet\":4"),
        DecodeError::UnknownSheetKey {
            at: at(".args.sheet"),
            key: SheetKey::new(4),
        },
    );
    add(
        "an insert of a key the root holds",
        replace_first(SHEETS_V1, "\"key\":9", "\"key\":2"),
        DecodeError::SheetKeyInUse {
            at: at(".args.key"),
            key: SheetKey::new(2),
        },
    );
    let insert = envelope(
        "sheets",
        "insert",
        &format!("\"key\":9,\"name\":\"{}\",\"position\":1", hex_text("New")),
    );
    // Keys resolve through the root as it is, not as the batch's earlier operations leave it.
    add(
        "a cell of the sheet an earlier insert of the batch adds",
        batch_of(&[
            insert.clone(),
            envelope(
                "cells",
                "set_cell",
                "\"sheet\":9,\"row\":1,\"column\":1,\"input\":{\"logical\":true}",
            ),
        ]),
        DecodeError::UnknownSheetKey {
            at: "$.operations[1].args.sheet".to_owned(),
            key: SheetKey::new(9),
        },
    );
    add(
        "two inserts of one key",
        batch_of(&[insert.clone(), insert]),
        DecodeError::SheetKeyInUse {
            at: "$.operations[1].args.key".to_owned(),
            key: SheetKey::new(9),
        },
    );

    // Grid bounds and descriptor checks.
    let set_at = |row: u32, column: u32| {
        batch_of(&[envelope(
            "cells",
            "set_cell",
            &format!(
                "\"sheet\":1,\"row\":{row},\"column\":{column},\"input\":{{\"logical\":true}}"
            ),
        )])
    };
    for (row, column, error) in [
        (0, 1, GridError::RowOutOfRange(0)),
        (MAX_ROWS + 1, 1, GridError::RowOutOfRange(MAX_ROWS + 1)),
        (1, 0, GridError::ColumnOutOfRange(0)),
        (
            1,
            MAX_COLUMNS + 1,
            GridError::ColumnOutOfRange(MAX_COLUMNS + 1),
        ),
    ] {
        add(
            &format!("a cell at row {row}, column {column}"),
            set_at(row, column),
            DecodeError::InvalidCell {
                at: at(".args"),
                error,
            },
        );
    }
    let clear = |top: u32, left: u32, bottom: u32, right: u32| {
        batch_of(&[envelope(
            "cells",
            "clear",
            &format!(
                "\"sheet\":2,\"top\":{top},\"left\":{left},\"bottom\":{bottom},\"right\":{right}"
            ),
        )])
    };
    add(
        "a clear past the last row",
        clear(1, 1, MAX_ROWS + 1, 1),
        DecodeError::InvalidCell {
            at: at(".args"),
            error: GridError::RowOutOfRange(MAX_ROWS + 1),
        },
    );
    add(
        "a clear with its rows inverted",
        clear(3, 1, 2, 1),
        DecodeError::InvertedArea { at: at(".args") },
    );
    add(
        "a clear with its columns inverted",
        clear(1, 3, 1, 2),
        DecodeError::InvertedArea { at: at(".args") },
    );
    let axis_edit = |op: &str, first: u32, count: u32| {
        batch_of(&[envelope(
            "axes",
            op,
            &format!("\"sheet\":1,\"first\":{first},\"count\":{count}"),
        )])
    };
    for (op, first, count, error) in [
        (
            "insert_rows",
            0,
            1,
            AxisEditError::FirstOutOfRange {
                axis: Axis::Rows,
                first: 0,
            },
        ),
        (
            "delete_rows",
            MAX_ROWS + 1,
            1,
            AxisEditError::FirstOutOfRange {
                axis: Axis::Rows,
                first: MAX_ROWS + 1,
            },
        ),
        ("delete_columns", 1, 0, AxisEditError::ZeroCount),
        (
            "insert_columns",
            MAX_COLUMNS,
            2,
            AxisEditError::PastGridEnd {
                axis: Axis::Columns,
                first: MAX_COLUMNS,
                count: 2,
            },
        ),
        (
            "delete_rows",
            2,
            MAX_ROWS,
            AxisEditError::PastGridEnd {
                axis: Axis::Rows,
                first: 2,
                count: MAX_ROWS,
            },
        ),
    ] {
        add(
            &format!("{op} {first}+{count}"),
            axis_edit(op, first, count),
            DecodeError::InvalidAxisEdit {
                at: at(".args"),
                error,
            },
        );
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Case cdoc_m0_1__no_engine_dependency.

/// One `[[package]]` of a Cargo.lock: its name, version and dependency entries (a name, with a
/// version when the lockfile needs one to tell packages apart).
#[derive(Clone, Debug, Default)]
struct LockPackage {
    name: String,
    version: String,
    dependencies: Vec<(String, Option<String>)>,
}

/// The packages of a Cargo.lock (format version 3 or 4, as cargo writes it; CRLF or LF).
fn parse_lock(text: &str) -> Vec<LockPackage> {
    let mut packages = Vec::new();
    let mut current: Option<LockPackage> = None;
    let mut in_dependencies = false;
    for line in text.lines().map(str::trim) {
        if in_dependencies {
            if line == "]" {
                in_dependencies = false;
            } else if let Some(package) = current.as_mut() {
                let entry = line.trim_end_matches(',').trim_matches('"');
                let mut parts = entry.split(' ');
                let name = parts.next().expect("a dependency name").to_owned();
                let version = parts.next().map(str::to_owned);
                package.dependencies.push((name, version));
            }
            continue;
        }
        if line.starts_with('[') {
            packages.extend(current.take());
            if line == "[[package]]" {
                current = Some(LockPackage::default());
            }
            continue;
        }
        let Some(package) = current.as_mut() else {
            continue;
        };
        if let Some(value) = line.strip_prefix("name = ") {
            package.name = value.trim_matches('"').to_owned();
        } else if let Some(value) = line.strip_prefix("version = ") {
            package.version = value.trim_matches('"').to_owned();
        } else if line == "dependencies = [" {
            in_dependencies = true;
        }
    }
    packages.extend(current);
    packages
}

/// The names of every package reachable from `root` through the lockfile's dependency edges
/// (normal, build and dev edges, for every target and feature the lockfile records), `root`
/// included. Panics when an edge names no package or more than one.
fn lock_closure(packages: &[LockPackage], root: &str) -> BTreeSet<String> {
    let find = |name: &str, version: Option<&str>| {
        let found: Vec<&LockPackage> = packages
            .iter()
            .filter(|package| {
                package.name == name && version.is_none_or(|version| package.version == version)
            })
            .collect();
        assert_eq!(found.len(), 1, "the lockfile resolves {name} {version:?}");
        found[0]
    };
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut queue = VecDeque::from([find(root, None)]);
    while let Some(package) = queue.pop_front() {
        if !seen.insert((package.name.clone(), package.version.clone())) {
            continue;
        }
        for (name, version) in &package.dependencies {
            queue.push_back(find(name, version.as_deref()));
        }
    }
    seen.into_iter().map(|(name, _)| name).collect()
}

const FORBIDDEN: [&str; 4] = ["calc-engine", "calc-fml", "calc-host", "calc-doc"];

/// calc-doc-model's dependency closure, read from the committed `Cargo.lock`, holds none of
/// calc-engine, calc-fml, calc-host and calc-doc; and the codec refuses every message that carries a
/// cached or calculated value, and never decodes a value literal as a formula.
///
/// The closure over-approximates the build closure. Cargo.lock lists each package's dependency
/// edges without their kind, target or feature, and it records the dev-dependency edges of
/// workspace members, so the closure followed here is a superset of the packages a build of
/// calc-doc-model uses. A superset without the forbidden packages implies the build closure is
/// without them.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cdoc_m0_1__no_engine_dependency() {
    const CASE: &str = "cdoc_m0_1__no_engine_dependency";

    // calc-doc-model's dependency closure, from the committed lockfile.
    let packages = parse_lock(CARGO_LOCK);
    let model = packages
        .iter()
        .find(|package| package.name == "calc-doc-model")
        .expect("calc-doc-model in Cargo.lock");
    let direct: BTreeSet<&str> = model
        .dependencies
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(
        direct,
        BTreeSet::from(["calc-value", "serde", "serde_json"]),
        "calc-doc-model's direct edges in Cargo.lock"
    );
    let closure = lock_closure(&packages, "calc-doc-model");
    for reached in ["calc-value", "serde_json"] {
        assert!(
            closure.contains(reached),
            "the closure follows edges to {reached}"
        );
    }
    let indirect: Vec<&String> = closure
        .iter()
        .filter(|name| name.as_str() != "calc-doc-model" && !direct.contains(name.as_str()))
        .collect();
    assert!(
        !indirect.is_empty(),
        "the closure reaches past the direct dependencies"
    );
    let found: Vec<&str> = FORBIDDEN
        .into_iter()
        .filter(|name| closure.contains(*name))
        .collect();
    assert_eq!(
        found,
        Vec::<&str>::new(),
        "forbidden packages in calc-doc-model's closure"
    );
    let codec_closure = lock_closure(&packages, "calc-doc");
    assert!(codec_closure.contains("calc-doc-model") && codec_closure.contains("calc-value"));
    // Control: the same check finds an engine edge planted one level down.
    let mut planted = packages.clone();
    planted
        .iter_mut()
        .find(|package| package.name == "calc-value")
        .expect("calc-value in Cargo.lock")
        .dependencies
        .push(("calc-engine".to_owned(), None));
    assert!(
        lock_closure(&planted, "calc-doc-model").contains("calc-engine"),
        "a planted calc-value -> calc-engine edge is found"
    );
    let value_edges: Vec<&str> = packages
        .iter()
        .find(|package| package.name == "calc-value")
        .expect("calc-value in Cargo.lock")
        .dependencies
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    note!(
        "ACCEPT-NOTE: {CASE}: Cargo.lock ({} bytes, {} packages): calc-doc-model's closure has {} packages {:?}, {} of them past its direct dependencies, and none of {:?}; a planted calc-value -> calc-engine edge is found",
        CARGO_LOCK.len(),
        packages.len(),
        closure.len(),
        closure,
        indirect.len(),
        FORBIDDEN
    );
    note!(
        "ACCEPT-NOTE: {CASE}: the closure over-approximates the build closure: Cargo.lock lists edges without their kind, target or feature and records workspace members' dev-dependency edges, so these {} packages are a superset of what a build of calc-doc-model uses (calc-value's edges, followed like any other, are {:?}); a superset without the forbidden packages implies the build closure is without them",
        closure.len(),
        value_edges
    );

    // A formula input that also carries a cached value is refused, and so is any message that
    // holds a calculated value.
    let s = session(0);
    let source = hex_text("SUM(A1:A3)");
    let cached = [
        (
            "a formula with a cached number beside it",
            set_cell_with(&format!(
                "\"formula\":\"{source}\",\"cached\":{{\"number\":\"4018000000000000\"}}"
            )),
            DecodeError::UnknownField {
                at: at(".args.input"),
                field: "cached".to_owned(),
            },
        ),
        (
            "a formula with its value as a second input kind",
            set_cell_with(&format!(
                "\"formula\":\"{source}\",\"number\":\"4018000000000000\""
            )),
            DecodeError::Malformed {
                at: at(".args.input"),
                expected: "exactly one of number, text, logical, error or formula",
            },
        ),
        (
            "a cached value beside the input",
            batch_of(&[envelope(
                "cells",
                "set_cell",
                &format!(
                    "\"sheet\":1,\"row\":1,\"column\":1,\"input\":{{\"formula\":\"{source}\"}},\"value\":{{\"number\":\"4018000000000000\"}}"
                ),
            )]),
            DecodeError::UnknownField {
                at: at(".args"),
                field: "value".to_owned(),
            },
        ),
        (
            "a calculated input kind",
            set_cell_with("\"calculated\":{\"number\":\"4018000000000000\"}"),
            DecodeError::UnknownField {
                at: at(".args.input"),
                field: "calculated".to_owned(),
            },
        ),
        (
            "an operation that publishes a value",
            batch_of(&[envelope(
                "cells",
                "set_value",
                "\"sheet\":1,\"row\":1,\"column\":1,\"value\":{\"number\":\"4018000000000000\"}",
            )]),
            DecodeError::UnknownOperation {
                at: at(".op"),
                capability: "cells",
                operation: "set_value".to_owned(),
            },
        ),
        (
            "a capability of calculated values",
            batch_of(&[envelope(
                "values",
                "publish",
                "\"sheet\":1,\"row\":1,\"column\":1,\"value\":{\"number\":\"4018000000000000\"}",
            )]),
            DecodeError::UnknownCapability {
                at: at(".capability"),
                capability: "values".to_owned(),
            },
        ),
        (
            "a formula whose payload is its cached number",
            set_cell_with("\"formula\":{\"number\":\"4018000000000000\"}"),
            DecodeError::Malformed {
                at: at(".args.input.formula"),
                expected: "4 lowercase hex digits per UTF-16 code unit",
            },
        ),
        (
            "a formula whose payload is a JSON number",
            set_cell_with("\"formula\":6"),
            DecodeError::Malformed {
                at: at(".args.input.formula"),
                expected: "4 lowercase hex digits per UTF-16 code unit",
            },
        ),
    ];
    for (label, bytes, expected) in &cached {
        assert_refused(label, bytes, &s.root, expected);
    }
    note!(
        "ACCEPT-NOTE: {CASE}: {} messages carrying a cached or calculated value were refused: {:?}",
        cached.len(),
        cached
            .iter()
            .map(|(label, _, error)| format!("{label}: {}", refusal_kind(error)))
            .collect::<Vec<_>>()
    );

    // A value literal never decodes as a formula: in the cells fixture, the decoded kind of every
    // input is the kind its own JSON member names (read with serde_json), and text that looks like
    // a formula stays text.
    let message: Value = serde_json::from_slice(CELLS_V1).expect("cells_v1.json");
    let decoded = decode_batch(CELLS_V1, &s.root).expect("cells_v1.json");
    let mut formulas = 0;
    let mut literals = 0;
    for (envelope, op) in message["operations"]
        .as_array()
        .expect("operations")
        .iter()
        .zip(&decoded.operations)
    {
        let Some(input) = envelope["args"]["input"].as_object() else {
            continue;
        };
        assert_eq!(input.len(), 1, "one member per input");
        let member = input.keys().next().expect("one member").as_str();
        let WorkbookOp::SetCell { input, .. } = op else {
            panic!("a set_cell envelope decoded as {}", describe(op));
        };
        let decoded_kind = match input {
            CellInput::Number(_) => "number",
            CellInput::Text(_) => "text",
            CellInput::Logical(_) => "logical",
            CellInput::Error(_) => "error",
            CellInput::Formula(_) => "formula",
        };
        assert_eq!(decoded_kind, member, "{}", describe(op));
        if decoded_kind == "formula" {
            formulas += 1;
        } else {
            literals += 1;
        }
    }
    assert_eq!((formulas, literals), (2, 24));
    for (input, expected) in [
        (
            format!("\"text\":\"{}\"", hex_text("=SUM(A1:A3)")),
            text(units("=SUM(A1:A3)")),
        ),
        (
            format!("\"text\":\"{}\"", hex_text("=1+1")),
            text(units("=1+1")),
        ),
        ("\"number\":\"4018000000000000\"".to_owned(), number(6.0)),
        ("\"logical\":true".to_owned(), CellInput::Logical(true)),
        (
            "\"error\":\"na\"".to_owned(),
            CellInput::Error(WorksheetErrorCode::NA),
        ),
    ] {
        let batch = decode_batch(&set_cell_with(&input), &s.root).expect("a value literal");
        let [WorkbookOp::SetCell { input: decoded, .. }] = batch.operations.as_slice() else {
            panic!("one set_cell");
        };
        assert!(
            !matches!(decoded, CellInput::Formula(_)),
            "{input} stays a literal"
        );
        assert_eq!(
            describe_input(decoded),
            describe_input(&expected),
            "{input}"
        );
    }

    // The encoder writes each input as exactly one member named for its kind, so a formula
    // never travels with a value beside it.
    let one_of_each = OpBatch {
        base: AuthoredRevision::new(FIXTURE_BASE),
        operations: vec![
            set(s.sheet1, 1, 1, number(6.0)),
            set(s.sheet1, 1, 2, text(units("=A1"))),
            set(s.sheet1, 1, 3, CellInput::Logical(false)),
            set(s.sheet1, 1, 4, CellInput::Error(WorksheetErrorCode::Div0)),
            set(s.sheet1, 1, 5, formula("A1*2")),
        ],
    };
    let written: Value =
        serde_json::from_slice(&encode_batch(&one_of_each, &s.root).expect("five inputs"))
            .expect("JSON");
    let members: Vec<Vec<String>> = written["operations"]
        .as_array()
        .expect("operations")
        .iter()
        .map(|envelope| {
            let args = envelope["args"].as_object().expect("args");
            assert_eq!(
                args.keys().map(String::as_str).collect::<BTreeSet<_>>(),
                BTreeSet::from(["sheet", "row", "column", "input"]),
                "set_cell arguments"
            );
            args["input"]
                .as_object()
                .expect("an input object")
                .keys()
                .cloned()
                .collect()
        })
        .collect();
    assert_eq!(
        members,
        [["number"], ["text"], ["logical"], ["error"], ["formula"]]
            .map(|kind| kind.map(str::to_owned).to_vec())
    );
    note!(
        "ACCEPT-NOTE: {CASE}: cells_v1.json decoded to {formulas} formulas and {literals} literals, each the kind its JSON member names; text \"=SUM(A1:A3)\" and \"=1+1\" stay text; the encoder wrote each of 5 input kinds as one member {:?}",
        members
    );
}

// ---------------------------------------------------------------------------------------------
// Case dc_73yy__page_sharing.

/// Applies `operations` to `root` through the route a host uses: the batch is encoded against
/// `root`, decoded against it again, checked equal to the batch built here, and the decoded batch
/// is applied.
fn through_route(root: &Arc<ModelRoot>, operations: Vec<WorkbookOp>) -> Arc<ModelRoot> {
    let batch = OpBatch {
        base: root.revision(),
        operations,
    };
    let bytes = encode_batch(&batch, root).expect("an encodable batch");
    let decoded = decode_batch(&bytes, root).expect("a decodable batch");
    assert_eq!(decoded, batch, "the batch survives the wire");
    root.apply(&decoded).expect("an applicable batch")
}

/// The occupied cells of `area` as (row, column, input), in the order the borrowed read returns
/// them.
fn read_area(root: &ModelRoot, area: GridArea) -> Vec<(u32, u32, CellInput)> {
    root.cells(area)
        .expect("a sheet of the root")
        .map(|(at, input)| {
            assert_eq!(at.sheet(), area.sheet(), "a read stays on its sheet");
            (at.row(), at.column(), input.clone())
        })
        .collect()
}

/// Every page of every sheet of `root`, by (sheet, first row, first column).
fn page_map(root: &ModelRoot) -> BTreeMap<(SheetId, u32, u32), &Arc<CellPage>> {
    let mut pages = BTreeMap::new();
    for sheet in root.sheets() {
        for page in root.pages(sheet.id()).expect("a sheet of the root") {
            let previous = pages.insert((sheet.id(), page.first_row(), page.first_column()), page);
            assert!(previous.is_none(), "one page per block");
        }
    }
    pages
}

/// Each page's block, as (sheet, first row, first column, last row, last column, cells), with
/// every cell of the page inside its block.
fn page_blocks(root: &ModelRoot) -> Vec<(SheetId, u32, u32, u32, u32, usize)> {
    page_map(root)
        .into_iter()
        .map(|((sheet, _, _), page)| {
            for (row, column, _) in page.iter() {
                assert!(
                    (page.first_row()..=page.last_row()).contains(&row)
                        && (page.first_column()..=page.last_column()).contains(&column),
                    "a page's cell lies in its block"
                );
            }
            (
                sheet,
                page.first_row(),
                page.first_column(),
                page.last_row(),
                page.last_column(),
                page.cell_count(),
            )
        })
        .collect()
}

/// Asserts that every page of `old` and `new` is the same allocation, except the pages of the
/// blocks in `changed`, which must be new allocations or absent from `new`. Returns the number of
/// shared pages.
fn check_sharing(
    label: &str,
    old: &ModelRoot,
    new: &ModelRoot,
    changed: &[(SheetId, u32, u32)],
) -> usize {
    let (old_pages, new_pages) = (page_map(old), page_map(new));
    let mut shared = 0;
    for (block, old_page) in &old_pages {
        let new_page = new_pages.get(block);
        if changed.contains(block) {
            if let Some(new_page) = new_page {
                assert!(
                    !Arc::ptr_eq(old_page, new_page),
                    "{label}: the changed page at {block:?} is a new allocation"
                );
            }
        } else {
            let new_page = new_page.unwrap_or_else(|| panic!("{label}: page {block:?} kept"));
            assert!(
                Arc::ptr_eq(old_page, new_page),
                "{label}: the untouched page at {block:?} is the old root's allocation"
            );
            shared += 1;
        }
    }
    for block in new_pages.keys() {
        assert!(
            old_pages.contains_key(block) || changed.contains(block),
            "{label}: no page appears at {block:?} unless the batch changed it"
        );
    }
    shared
}

/// A one-cell change through the operation route makes a root that shares every untouched page
/// with the old root by pointer and holds a new page for the changed block; both roots' borrowed
/// reads return the expected inputs. The batches are built here, sent through the wire codec and
/// applied with `ModelRoot::apply`; every expectation is a literal written here.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_73yy__page_sharing() {
    const CASE: &str = "dc_73yy__page_sharing";
    assert_eq!((PAGE_ROWS, PAGE_COLUMNS), (256, 16), "the page block");
    let s = session(0);
    let (sheet1, data) = (s.sheet1, s.data);
    let r2_formula = formula("Q1*2");

    // Cells in five blocks of Sheet1 and two of Data. A1 is set twice, C3 and C600 are cleared
    // (C600's block then holds nothing), and C4 is set after the clear that covers it.
    let populated = through_route(
        &s.root,
        vec![
            set(sheet1, 1, 1, number(0.0)),
            set(sheet1, 2, 2, text(units("b2"))),
            set(sheet1, 3, 3, number(33.0)),
            set(sheet1, 256, 16, CellInput::Logical(true)),
            set(sheet1, 1, 17, number(17.0)),
            set(sheet1, 2, 18, r2_formula.clone()),
            set(sheet1, 257, 1, number(257.0)),
            set(sheet1, 300, 20, CellInput::Error(WorksheetErrorCode::Div0)),
            set(sheet1, 600, 3, number(600.0)),
            set(sheet1, MAX_ROWS, MAX_COLUMNS, text(units("end"))),
            set(data, 1, 1, number(-1.0)),
            set(data, 5000, 100, formula("A1")),
            set(sheet1, 1, 1, number(1.0)),
            WorkbookOp::Clear {
                area: GridArea::new(cell(sheet1, 3, 3), cell(sheet1, 600, 3)).expect("one sheet"),
            },
            set(sheet1, 4, 3, number(4.0)),
        ],
    );
    assert_eq!(
        populated.revision(),
        AuthoredRevision::new(FIXTURE_BASE + 1)
    );
    assert_eq!(
        page_blocks(&populated),
        vec![
            (sheet1, 1, 1, 256, 16, 4),
            (sheet1, 1, 17, 256, 32, 2),
            (sheet1, 257, 1, 512, 16, 1),
            (sheet1, 257, 17, 512, 32, 1),
            (sheet1, 1_048_321, 16_369, 1_048_576, 16_384, 1),
            (data, 1, 1, 256, 16, 1),
            (data, 4865, 97, 5120, 112, 1),
        ],
        "the populated root's pages: one per occupied block of 256 rows by 16 columns"
    );
    assert!(populated.page_of(cell(sheet1, 600, 3)).is_none());
    let sheet1_before = vec![
        (1, 1, number(1.0)),
        (1, 17, number(17.0)),
        (2, 2, text(units("b2"))),
        (2, 18, r2_formula.clone()),
        (4, 3, number(4.0)),
        (256, 16, CellInput::Logical(true)),
        (257, 1, number(257.0)),
        (300, 20, CellInput::Error(WorksheetErrorCode::Div0)),
        (MAX_ROWS, MAX_COLUMNS, text(units("end"))),
    ];
    assert_eq!(
        read_area(&populated, GridArea::full_sheet(sheet1)),
        sheet1_before,
        "Sheet1's occupied cells in row-major order"
    );
    assert_eq!(
        read_area(&populated, GridArea::full_sheet(data)),
        vec![(1, 1, number(-1.0)), (5000, 100, formula("A1"))]
    );

    // The one-cell change: R2, in the block of rows 1 to 256 and columns Q to AF.
    let changed = through_route(&populated, vec![set(sheet1, 2, 18, number(36.0))]);
    assert_eq!(changed.revision(), AuthoredRevision::new(FIXTURE_BASE + 2));
    let r2_block = (sheet1, 1, 17);
    let shared = check_sharing("one-cell change", &populated, &changed, &[r2_block]);
    assert_eq!(shared, 6, "six of the seven pages are shared");
    let (old_page, new_page) = (
        populated.page_of(cell(sheet1, 2, 18)).expect("R2's page"),
        changed.page_of(cell(sheet1, 2, 18)).expect("R2's page"),
    );
    assert!(!Arc::ptr_eq(old_page, new_page), "the changed page is new");
    assert_eq!(
        page_blocks(&changed),
        page_blocks(&populated),
        "the change keeps every block and its cell count"
    );
    assert_eq!(old_page.get(2, 18), Some(&r2_formula));
    assert_eq!(new_page.get(2, 18), Some(&number(36.0)));
    assert_eq!(old_page.get(1, 17), new_page.get(1, 17));

    // Borrowed reads of both roots.
    let mut sheet1_after = sheet1_before.clone();
    sheet1_after[3] = (2, 18, number(36.0));
    assert_eq!(
        read_area(&populated, GridArea::full_sheet(sheet1)),
        sheet1_before,
        "the old root still reads its own inputs"
    );
    assert_eq!(
        read_area(&changed, GridArea::full_sheet(sheet1)),
        sheet1_after,
        "the new root reads the change"
    );
    let mut same_address = 0;
    for (row, column, expected) in &sheet1_before {
        let at = cell(sheet1, *row, *column);
        let old = populated.cell(at).expect("an occupied cell");
        let new = changed.cell(at).expect("an occupied cell");
        assert_eq!(old, expected, "the old root's {at:?}");
        if (*row, *column) == (2, 18) {
            assert_eq!(new, &number(36.0));
        } else {
            assert_eq!(new, expected, "the new root's {at:?}");
            if populated
                .page_of(at)
                .map(|page| (page.first_row(), page.first_column()))
                != Some((1, 17))
            {
                assert!(
                    std::ptr::eq(old, new),
                    "{at:?} on an untouched page is read from the same allocation"
                );
                same_address += 1;
            }
        }
        let page = changed.page_of(at).expect("the cell's page");
        assert!(
            std::ptr::eq(
                changed.cell(at).expect("a cell"),
                page.get(*row, *column).expect("a cell")
            ),
            "ModelRoot::cell borrows from the page"
        );
    }
    assert_eq!(same_address, 7);
    for (row, column) in [(3, 3), (600, 3), (2, 1), (MAX_ROWS, 1)] {
        assert_eq!(
            changed.cell(cell(sheet1, row, column)),
            None,
            "an empty cell"
        );
    }
    assert_eq!(
        changed.cell(cell(s.archive, 1, 1)),
        None,
        "a sheet without cells"
    );
    assert_eq!(
        changed.cell(cell(s.stranger, 1, 1)),
        None,
        "a sheet the root lacks"
    );
    assert!(changed.cells(GridArea::full_sheet(s.stranger)).is_none());
    assert!(changed.sheet_cells(s.stranger).is_none());
    assert!(changed.pages(s.stranger).is_none());
    assert_eq!(changed.sheet_cells(s.archive).expect("Archive").count(), 0);

    // Rectangles: row-major across the pages of one page row, and clipped to the rectangle.
    let rectangles = [
        (
            (1, 1, 300, 20),
            vec![
                (1, 1, number(1.0)),
                (1, 17, number(17.0)),
                (2, 2, text(units("b2"))),
                (2, 18, number(36.0)),
                (4, 3, number(4.0)),
                (256, 16, CellInput::Logical(true)),
                (257, 1, number(257.0)),
                (300, 20, CellInput::Error(WorksheetErrorCode::Div0)),
            ],
        ),
        (
            (1, 2, 2, 17),
            vec![(1, 17, number(17.0)), (2, 2, text(units("b2")))],
        ),
        ((3, 3, 600, 3), vec![(4, 3, number(4.0))]),
        ((5, 1, 255, MAX_COLUMNS), Vec::new()),
        (
            (256, 16, MAX_ROWS, MAX_COLUMNS),
            vec![
                (256, 16, CellInput::Logical(true)),
                (300, 20, CellInput::Error(WorksheetErrorCode::Div0)),
                (MAX_ROWS, MAX_COLUMNS, text(units("end"))),
            ],
        ),
    ];
    for ((top, left, bottom, right), expected) in &rectangles {
        let area = GridArea::new(cell(sheet1, *top, *left), cell(sheet1, *bottom, *right))
            .expect("one sheet");
        assert_eq!(
            &read_area(&changed, area),
            expected,
            "the occupied cells of rows {top} to {bottom}, columns {left} to {right}"
        );
    }
    assert_eq!(
        read_area(
            &changed,
            GridArea::new(cell(data, 4865, 97), cell(data, 5120, 112)).expect("one sheet")
        ),
        vec![(5000, 100, formula("A1"))]
    );

    // Setting a cell to the input it holds changes no page; clearing a block's only cell
    // removes its page and shares the rest.
    let unchanged = through_route(&changed, vec![set(sheet1, 2, 18, number(36.0))]);
    assert_eq!(
        unchanged.revision(),
        AuthoredRevision::new(FIXTURE_BASE + 3)
    );
    assert_eq!(check_sharing("same input", &changed, &unchanged, &[]), 7);
    let t300_block = (sheet1, 257, 17);
    let cleared = through_route(
        &changed,
        vec![WorkbookOp::Clear {
            area: GridArea::new(cell(sheet1, 290, 17), cell(sheet1, 310, 32)).expect("one sheet"),
        }],
    );
    assert_eq!(check_sharing("clear", &changed, &cleared, &[t300_block]), 6);
    assert!(cleared.page_of(cell(sheet1, 300, 20)).is_none());
    assert_eq!(page_map(&cleared).len(), 6);
    assert_eq!(
        changed.cell(cell(sheet1, 300, 20)),
        Some(&CellInput::Error(WorksheetErrorCode::Div0)),
        "the root the clear was applied to keeps T300"
    );

    // What the route refuses: nothing is made, and the root reads as before.
    let refusals = [
        (
            OpBatch {
                base: populated.revision(),
                operations: vec![set(sheet1, 1, 1, number(2.0))],
            },
            ApplyError::StaleBase {
                batch: populated.revision(),
                root: changed.revision(),
            },
        ),
        (
            OpBatch {
                base: changed.revision(),
                operations: vec![
                    set(sheet1, 1, 1, number(2.0)),
                    set(s.stranger, 1, 1, number(2.0)),
                ],
            },
            ApplyError::UnknownSheet {
                operation: 1,
                sheet: s.stranger,
            },
        ),
        (
            OpBatch {
                base: changed.revision(),
                operations: vec![
                    WorkbookOp::Clear {
                        area: GridArea::full_sheet(sheet1),
                    },
                    axis(sheet1, Axis::Rows, AxisChange::Insert, 1, 1),
                ],
            },
            ApplyError::AxisEdit { operation: 1 },
        ),
        (
            OpBatch {
                base: changed.revision(),
                operations: vec![WorkbookOp::EditSheet(SheetEdit::SetVisibility {
                    sheet: sheet1,
                    visibility: SheetVisibility::Hidden,
                })],
            },
            ApplyError::SheetEdit { operation: 0 },
        ),
    ];
    for (batch, expected) in &refusals {
        assert_eq!(changed.apply(batch).err(), Some(*expected));
    }
    assert_eq!(changed.revision(), AuthoredRevision::new(FIXTURE_BASE + 2));
    assert_eq!(
        read_area(&changed, GridArea::full_sheet(sheet1)),
        sheet1_after,
        "a refused batch leaves the root's cells as they were"
    );
    let last = ModelRoot::new(
        AuthoredRevision::new(u64::MAX),
        vec![SheetEntry::new(
            sheet1,
            SheetKey::new(1),
            name("Sheet1"),
            SheetVisibility::Visible,
        )],
    )
    .expect("one sheet");
    assert_eq!(
        last.apply(&OpBatch {
            base: AuthoredRevision::new(u64::MAX),
            operations: vec![set(sheet1, 1, 1, number(1.0))],
        })
        .err(),
        Some(ApplyError::RevisionExhausted)
    );

    note!(
        "ACCEPT-NOTE: {CASE}: a one-cell change (R2) through encode, decode and ModelRoot::apply kept {shared} of 7 pages pointer-identical and made 1 new page; {same_address} cells on untouched pages read from the same address in both roots; 5 rectangle reads returned literal row-major lists"
    );
}
