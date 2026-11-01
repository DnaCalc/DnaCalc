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
//!
//! The cdoc-m1.1 cases read the packages under `tests/fixtures/cdoc_m1_1/`, written by the
//! committed generator beside them, together with its `manifest.json`, which records what the
//! generator wrote as .NET computed it. Every other package they open is built here by
//! `cdoc_m1_1_support`, whose ZIP reader is also what inspects every saved package.
//!
//! The cdoc-m1.2 cases load the packages under `tests/fixtures/cdoc_m1_2/` and compare what the
//! model and the records beside it hold with that generator's `manifest.json`: the values, texts,
//! styles, shared formula groups and caches it wrote, the formula texts it states each cell holds,
//! the CRC-32 and size it computed of each sheet's part, and the date system and number of defined
//! names of each workbook part. The other packages they load are built here and say so.
//!
//! The cdoc-m1.3 cases edit cdoc-m1.2's cells.xlsx and packages built here, save them through
//! calc-doc's route, and read what was written with `cdoc_m1_1_support`'s ZIP reader and
//! `cdoc_m1_3_support`'s own DEFLATE decoder and XML parser, against expectations written here.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

mod cdoc_m1_1_support;

use calc_doc::model::value::{
    CellKey, ExcelText, GridArea, GridError, IdAllocator, MAX_COLUMNS, MAX_ROWS, SheetId,
    WorksheetErrorCode,
};
use calc_doc::model::{
    ApplyError, AuthoredRevision, Axis, AxisChange, AxisEdit, AxisEditError, BATCH_FORMAT,
    BATCH_VERSION, BlockId, BlockKind, CanvasArea, CellInput, CellPage, CellText, DecodeError,
    EncodeError, FiniteNumber, FormulaText, FormulaTextError, ModelRoot, ModelRootError, OpBatch,
    PAGE_COLUMNS, PAGE_ROWS, PlacementId, SHEET_NAME_MAX_UTF16_CODE_UNITS, SheetEdit, SheetEntry,
    SheetKey, SheetName, SheetNameError, SheetVisibility, SkinLayoutEdit, Spot, WorkbookOp,
    decode_batch, encode_batch, op_capabilities,
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
const SKIN_LAYOUT_V1: &[u8] = include_bytes!("fixtures/skin_layout_v1.json");

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

/// The block of the skin layout fixture whose ID ends in the hex digit `n`:
/// `5e1f0c3a-7b2d-4c8e-9a41-0d6b3f2e8c0<n>`.
fn fixture_block(n: u8) -> BlockId {
    BlockId::new(0x5e1f_0c3a_7b2d_4c8e_9a41_0d6b_3f2e_8c00 | u128::from(n)).expect("a nonzero ID")
}

/// The placement of the skin layout fixture whose ID ends in the two hex digits of `n`:
/// `9c4d2b17-3e5a-4f60-8b72-1a0c5d3e7f<nn>`.
fn fixture_placement(n: u8) -> PlacementId {
    PlacementId::new(0x9c4d_2b17_3e5a_4f60_8b72_1a0c_5d3e_7f00 | u128::from(n))
        .expect("a nonzero ID")
}

fn bits(bits: u64) -> FiniteNumber {
    FiniteNumber::from_bits(bits).expect("a finite number")
}

/// A canvas spot at `position` with the area whose x, y, width and height have these bits.
fn canvas_spot(position_value: u32, [x, y, width, height]: [u64; 4]) -> Spot {
    Spot::Canvas {
        position: position(position_value),
        area: CanvasArea {
            x: bits(x),
            y: bits(y),
            width: bits(width),
            height: bits(height),
        },
    }
}

fn tree_spot(parent: Option<u8>, position_value: u32) -> Spot {
    Spot::Tree {
        parent: parent.map(fixture_placement),
        position: position(position_value),
    }
}

/// The operations `skin_layout_v1.json` holds, in this session's IDs: five blocks created (prose
/// and calculation, a cell and a range, on keys 1 and 2), placements added in all four views
/// (tree roots and children, canvas areas with a negative coordinate and a zero), three moved
/// (a tree child to the roots, a canvas card to the top with a new area, a notebook entry to the
/// end), one placement removed and one block removed.
fn skin_layout_ops(s: &Session) -> Vec<WorkbookOp> {
    let area = |sheet, top, left, bottom, right| {
        GridArea::new(cell(sheet, top, left), cell(sheet, bottom, right)).expect("one sheet")
    };
    let create = |n, kind, anchor| {
        WorkbookOp::EditSkinLayout(SkinLayoutEdit::CreateBlock {
            block: fixture_block(n),
            kind,
            anchor,
        })
    };
    let add = |p, b, spot| {
        WorkbookOp::EditSkinLayout(SkinLayoutEdit::AddPlacement {
            placement: fixture_placement(p),
            block: fixture_block(b),
            spot,
        })
    };
    let moved = |p, spot| {
        WorkbookOp::EditSkinLayout(SkinLayoutEdit::MovePlacement {
            placement: fixture_placement(p),
            spot,
        })
    };
    let notebook = |n| Spot::Notebook {
        position: position(n),
    };
    let grid = |n| Spot::Grid {
        position: position(n),
    };
    vec![
        create(1, BlockKind::Prose, area(s.sheet1, 1, 1, 1, 1)),
        create(2, BlockKind::Calculation, area(s.sheet1, 5, 2, 5, 2)),
        create(3, BlockKind::Calculation, area(s.sheet1, 5, 2, 6, 2)),
        create(4, BlockKind::Calculation, area(s.data, 1, 2, 3, 2)),
        create(5, BlockKind::Prose, area(s.sheet1, 3, 1, 3, 1)),
        add(0x11, 1, notebook(1)),
        add(0x12, 2, notebook(2)),
        add(0x13, 3, notebook(3)),
        add(0x14, 5, notebook(2)),
        add(0x21, 1, tree_spot(None, 1)),
        add(0x22, 2, tree_spot(Some(0x21), 1)),
        add(0x23, 3, tree_spot(Some(0x21), 1)),
        add(
            0x31,
            2,
            canvas_spot(
                1,
                [
                    0x4024_0000_0000_0000,
                    0x4034_0000_0000_0000,
                    0x4069_0000_0000_0000,
                    0x4054_0000_0000_0000,
                ],
            ),
        ),
        add(
            0x32,
            3,
            canvas_spot(
                2,
                [
                    0xc041_c000_0000_0000,
                    0x4061_8000_0000_0000,
                    0x406e_0000_0000_0000,
                    0x405e_0000_0000_0000,
                ],
            ),
        ),
        add(
            0x33,
            4,
            canvas_spot(
                1,
                [
                    0,
                    0x4072_c000_0000_0000,
                    0x4066_8000_0000_0000,
                    0x404e_0000_0000_0000,
                ],
            ),
        ),
        add(0x41, 3, grid(1)),
        add(0x42, 2, grid(1)),
        moved(0x23, tree_spot(None, 2)),
        moved(
            0x31,
            canvas_spot(
                3,
                [
                    0x407a_4400_0000_0000,
                    0x4034_0000_0000_0000,
                    0x4069_0000_0000_0000,
                    0x4058_0000_0000_0000,
                ],
            ),
        ),
        moved(0x11, notebook(4)),
        WorkbookOp::EditSkinLayout(SkinLayoutEdit::RemovePlacement {
            placement: fixture_placement(0x14),
        }),
        WorkbookOp::EditSkinLayout(SkinLayoutEdit::RemoveBlock {
            block: fixture_block(5),
        }),
    ]
}

type Fixture = (&'static str, &'static [u8], fn(&Session) -> Vec<WorkbookOp>);

const FIXTURES: [Fixture; 4] = [
    ("cells_v1.json", CELLS_V1, cells_ops),
    ("axes_v1.json", AXES_V1, axes_ops),
    ("sheets_v1.json", SHEETS_V1, sheets_ops),
    ("skin_layout_v1.json", SKIN_LAYOUT_V1, skin_layout_ops),
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
        WorkbookOp::SetConditionalRules { sheet, rules } => format!(
            "conditional {sheet:?} {:?}",
            rules
                .iter()
                .map(|r| (&r.areas, r.priority, r.stop, &r.formulas))
                .collect::<Vec<_>>()
        ),
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
        WorkbookOp::SetArray { array, input } => format!(
            "array {:?} R{}C{}:R{}C{} {:?} {}",
            array.area().sheet(),
            array.area().first().row(),
            array.area().first().column(),
            array.area().last().row(),
            array.area().last().column(),
            array.kind(),
            describe_input(input)
        ),
        WorkbookOp::SetFormat { at, format } => format!(
            "format {:?} R{}C{} {:?}",
            at.sheet(),
            at.row(),
            at.column(),
            format
        ),
        WorkbookOp::SetNameFormula {
            name,
            scope,
            formula,
        } => format!("name {name} {scope:?} {formula:?}"),
        WorkbookOp::RestoreNames(names) => format!("retained names {names:?}"),
        WorkbookOp::SetAxisFormat(edit) => format!("axis format {edit:?}"),
        WorkbookOp::SetFrozenPanes { sheet, panes } => format!(
            "panes {sheet:?} {:?}",
            panes.map(|p| (p.rows(), p.columns()))
        ),
        WorkbookOp::SetIteration(settings) => format!(
            "iteration {} {} {:016x}",
            settings.enabled(),
            settings.max_iterations(),
            settings.max_change().to_bits()
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
        WorkbookOp::EditSkinLayout(edit) => match edit {
            SkinLayoutEdit::CreateBlock {
                block,
                kind,
                anchor,
            } => format!(
                "layout create {:032x} {kind:?} {:?} R{}C{}:R{}C{}",
                block.get(),
                anchor.sheet(),
                anchor.first().row(),
                anchor.first().column(),
                anchor.last().row(),
                anchor.last().column()
            ),
            SkinLayoutEdit::RemoveBlock { block } => {
                format!("layout remove block {:032x}", block.get())
            }
            SkinLayoutEdit::AddPlacement {
                placement,
                block,
                spot,
            } => format!(
                "layout place {:032x} block {:032x} {}",
                placement.get(),
                block.get(),
                describe_spot(spot)
            ),
            SkinLayoutEdit::MovePlacement { placement, spot } => format!(
                "layout move {:032x} {}",
                placement.get(),
                describe_spot(spot)
            ),
            SkinLayoutEdit::RemovePlacement { placement } => {
                format!("layout remove placement {:032x}", placement.get())
            }
        },
    }
}

/// A spot's view, position and what the view keeps, from its observable parts: IDs as their
/// 128-bit values, numbers as their bits.
fn describe_spot(spot: &Spot) -> String {
    match spot {
        Spot::Notebook { position } => format!("notebook {position}"),
        Spot::Tree { parent, position } => format!(
            "tree under {} at {position}",
            parent.map_or_else(|| "none".to_owned(), |p| format!("{:032x}", p.get()))
        ),
        Spot::Canvas { position, area } => format!(
            "canvas {position} {:016x} {:016x} {:016x} {:016x}",
            area.x.get().to_bits(),
            area.y.get().to_bits(),
            area.width.get().to_bits(),
            area.height.get().to_bits()
        ),
        Spot::Grid { position } => format!("grid {position}"),
    }
}

fn describe_all(ops: &[WorkbookOp]) -> Vec<String> {
    ops.iter().map(describe).collect()
}

/// The features an operation exercises, by an exhaustive match: a variant added to the model
/// stops this file compiling until the cases cover it.
fn features(op: &WorkbookOp, out: &mut BTreeSet<String>) {
    let feature = match op {
        // Wire version 1 refuses conditional-rule state as EncodeError::ConditionalRules.
        WorkbookOp::SetConditionalRules { .. } => "op:conditional_rules".to_owned(),
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
        // Wire version 1 has no form for an authored array formula (EncodeError::ArrayFormula),
        // so no fixture holds one and it is not among the required features.
        WorkbookOp::SetArray { .. } => "op:set_array".to_owned(),
        // Wire version 1 has no form for a cell format either (EncodeError::CellFormat).
        WorkbookOp::SetFormat { .. } => "op:set_format".to_owned(),
        // Nor for a name's formula (EncodeError::NameFormula).
        WorkbookOp::SetNameFormula { .. } => "op:set_name_formula".to_owned(),
        WorkbookOp::RestoreNames(_) => "op:restore_names".to_owned(),
        // Nor for the iteration settings (EncodeError::Iteration).
        WorkbookOp::SetIteration(_) => "op:set_iteration".to_owned(),
        // Wire version 1 has no pane form (EncodeError::FrozenPanes).
        WorkbookOp::SetFrozenPanes { .. } => "op:set_frozen_panes".to_owned(),
        // Wire version 1 has no physical-dimension form (EncodeError::AxisFormat).
        WorkbookOp::SetAxisFormat(_) => "op:set_axis_format".to_owned(),
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
        WorkbookOp::EditSkinLayout(edit) => {
            out.insert("op:edit_skin_layout".to_owned());
            let spot = |spot: &Spot| match spot {
                Spot::Notebook { .. } => "spot:notebook",
                Spot::Tree { parent: None, .. } => "spot:tree_root",
                Spot::Tree {
                    parent: Some(_), ..
                } => "spot:tree_child",
                Spot::Canvas { .. } => "spot:canvas",
                Spot::Grid { .. } => "spot:grid",
            };
            match edit {
                SkinLayoutEdit::CreateBlock { kind, .. } => {
                    out.insert("layout:create_block".to_owned());
                    match kind {
                        BlockKind::Prose => "kind:prose".to_owned(),
                        BlockKind::Calculation => "kind:calculation".to_owned(),
                    }
                }
                SkinLayoutEdit::RemoveBlock { .. } => "layout:remove_block".to_owned(),
                SkinLayoutEdit::AddPlacement { spot: at, .. } => {
                    out.insert("layout:add_placement".to_owned());
                    spot(at).to_owned()
                }
                SkinLayoutEdit::MovePlacement { spot: at, .. } => {
                    out.insert("layout:move_placement".to_owned());
                    spot(at).to_owned()
                }
                SkinLayoutEdit::RemovePlacement { .. } => "layout:remove_placement".to_owned(),
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
        "op:edit_skin_layout",
        "layout:create_block",
        "layout:remove_block",
        "layout:add_placement",
        "layout:move_placement",
        "layout:remove_placement",
        "kind:prose",
        "kind:calculation",
        "spot:notebook",
        "spot:tree_root",
        "spot:tree_child",
        "spot:canvas",
        "spot:grid",
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

/// The operations of `skin_layout_v1.json`, in order: block IDs
/// `5e1f0c3a-7b2d-4c8e-9a41-0d6b3f2e8c0<n>`, placement IDs
/// `9c4d2b17-3e5a-4f60-8b72-1a0c5d3e7f<nn>`, and the canvas numbers 10, 20, 200, 80, -35.5, 140,
/// 240, 120, 0, 300, 180, 60, 420.25 and 96 as the bits written out here.
fn skin_layout_literal() -> Vec<LiteralOp> {
    let block = |n: &str| format!("\"5e1f0c3a-7b2d-4c8e-9a41-0d6b3f2e8c0{n}\"");
    let placement = |n: &str| format!("\"9c4d2b17-3e5a-4f60-8b72-1a0c5d3e7f{n}\"");
    let create = |n: &str, kind: &str, rest: &str| {
        literal(
            "create_block",
            &format!("block {n}"),
            &format!(r#""block":{},"kind":"{kind}",{rest}"#, block(n)),
        )
    };
    let add = |p: &str, b: &str, spot: &str| {
        literal(
            "add_placement",
            &format!("placement {p} of block {b}"),
            &format!(
                r#""placement":{},"block":{},"spot":{spot}"#,
                placement(p),
                block(b)
            ),
        )
    };
    let moved = |p: &str, spot: &str| {
        literal(
            "move_placement",
            &format!("placement {p}"),
            &format!(r#""placement":{},"spot":{spot}"#, placement(p)),
        )
    };
    vec![
        create(
            "1",
            "prose",
            r#""sheet":1,"top":1,"left":1,"bottom":1,"right":1"#,
        ),
        create(
            "2",
            "calculation",
            r#""sheet":1,"top":5,"left":2,"bottom":5,"right":2"#,
        ),
        create(
            "3",
            "calculation",
            r#""sheet":1,"top":5,"left":2,"bottom":6,"right":2"#,
        ),
        create(
            "4",
            "calculation",
            r#""sheet":2,"top":1,"left":2,"bottom":3,"right":2"#,
        ),
        create(
            "5",
            "prose",
            r#""sheet":1,"top":3,"left":1,"bottom":3,"right":1"#,
        ),
        add("11", "1", r#"{"notebook":{"position":1}}"#),
        add("12", "2", r#"{"notebook":{"position":2}}"#),
        add("13", "3", r#"{"notebook":{"position":3}}"#),
        add("14", "5", r#"{"notebook":{"position":2}}"#),
        add("21", "1", r#"{"tree":{"parent":null,"position":1}}"#),
        add(
            "22",
            "2",
            r#"{"tree":{"parent":"9c4d2b17-3e5a-4f60-8b72-1a0c5d3e7f21","position":1}}"#,
        ),
        add(
            "23",
            "3",
            r#"{"tree":{"parent":"9c4d2b17-3e5a-4f60-8b72-1a0c5d3e7f21","position":1}}"#,
        ),
        add(
            "31",
            "2",
            r#"{"canvas":{"position":1,"x":"4024000000000000","y":"4034000000000000","width":"4069000000000000","height":"4054000000000000"}}"#,
        ),
        add(
            "32",
            "3",
            r#"{"canvas":{"position":2,"x":"c041c00000000000","y":"4061800000000000","width":"406e000000000000","height":"405e000000000000"}}"#,
        ),
        add(
            "33",
            "4",
            r#"{"canvas":{"position":1,"x":"0000000000000000","y":"4072c00000000000","width":"4066800000000000","height":"404e000000000000"}}"#,
        ),
        add("41", "3", r#"{"grid":{"position":1}}"#),
        add("42", "2", r#"{"grid":{"position":1}}"#),
        moved("23", r#"{"tree":{"parent":null,"position":2}}"#),
        moved(
            "31",
            r#"{"canvas":{"position":3,"x":"407a440000000000","y":"4034000000000000","width":"4069000000000000","height":"4058000000000000"}}"#,
        ),
        moved("11", r#"{"notebook":{"position":4}}"#),
        literal(
            "remove_placement",
            "placement 14",
            &format!(r#""placement":{}"#, placement("14")),
        ),
        literal(
            "remove_block",
            "block 5",
            &format!(r#""block":{}"#, block("5")),
        ),
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

/// Compares the bytes of the four golden fixtures, without parsing them and so apart from the
/// codec and its JSON tokenizer, with text written in this file from the wire format's
/// description: each fixture's frame up to its first operation, every one of its operations as a
/// whole envelope (capability, version, operation name and every argument, members in wire
/// order), and its closing `]}`. Every argument is written out here
/// except the 131,068 digits of the 32,767-unit text, which are built here from that text's rule
/// (A to Z repeating), not by the codec. So a fixture changed to match a mistake the encoder and
/// the decoder share fails here when the mistake changes the bytes of any operation the fixtures
/// hold; a mistake that shows only on values no fixture holds is not caught here. Returns, per
/// fixture, the number of operations compared.
fn check_fixture_members() -> [(&'static str, usize); 4] {
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
    let layout = compare_fixture(
        "skin_layout_v1.json",
        SKIN_LAYOUT_V1,
        "skin_layout",
        &skin_layout_literal(),
    );
    [
        ("cells_v1.json", cells),
        ("axes_v1.json", axes),
        ("sheets_v1.json", sheets),
        ("skin_layout_v1.json", layout),
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
        "ACCEPT-NOTE: {CASE}: the bytes of the four fixtures' frames and of all {total} of their operations {compared:?} equal text written in the test from the wire format's description, every argument of every operation included: bits 8000000000000000 (-0.0), 0000000000000001 and 7fefffffffffffff, a 32,767-unit text of 131,068 hex digits built from its rule, a text whose last unit is U+D83D, the logicals true and false, the 14 error wire names, row 1048576 and column 16384, the top, left, bottom and right of each clear (the full sheet, B2:C3 and B3:D7), every axis and sheet edit's arguments, every skin layout edit's block and placement IDs, kinds, anchors and spots (canvas areas as their bits), the capability names and versions, and the sheet keys"
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
        ("skin_layout", "create_block"),
        ("skin_layout", "remove_block"),
        ("skin_layout", "add_placement"),
        ("skin_layout", "move_placement"),
        ("skin_layout", "remove_placement"),
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
    // existing sheet: all but insert, which names a new key (an insert of a key in use is refused
    // in the table above), and the skin layout operations other than create_block, which name
    // blocks and placements only.
    const NAMING_NO_SHEET: [&str; 5] = [
        "insert",
        "remove_block",
        "add_placement",
        "move_placement",
        "remove_placement",
    ];
    let unknown_key_ops: BTreeSet<String> = unknown_key_rows()
        .into_iter()
        .map(|(op, _, _)| op.to_owned())
        .collect();
    let naming_ops: BTreeSet<String> = op_capabilities()
        .iter()
        .flat_map(|capability| capability.operations.iter())
        .filter(|op| !NAMING_NO_SHEET.contains(op))
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
        (
            "create_block",
            WorkbookOp::EditSkinLayout(SkinLayoutEdit::CreateBlock {
                block: fixture_block(1),
                kind: BlockKind::Prose,
                anchor: GridArea::full_sheet(stranger),
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
        (
            "skin_layout",
            "create_block",
            "\"block\":\"5e1f0c3a-7b2d-4c8e-9a41-0d6b3f2e8c01\",\"kind\":\"prose\",\"sheet\":3,\"top\":1,\"left\":1,\"bottom\":1,\"right\":1".to_owned(),
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
    // wasm-bindgen and wasm-bindgen-test are wasm32-only dev-dependencies: they run
    // calc-doc-model's unit tests in the browsers (dc-msg). The closure below follows them too.
    assert_eq!(
        direct,
        BTreeSet::from([
            "calc-value",
            "serde",
            "serde_json",
            "wasm-bindgen",
            "wasm-bindgen-test"
        ]),
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

// ---------------------------------------------------------------------------------------------
// cdoc-m1.1: bounded package reading and raw preservation.

use calc_doc::{
    MemorySource, OpenError, OpenPolicy, OpenedDocument, PackageLimits, PackageRead, PartReadError,
    SinkError, SourceError, TargetMode, UnsafeName, WriteError, XmlError, XmlEvent, XmlLimits,
    XmlPart, prepare_save,
};
use cdoc_m1_1_support::{
    ChangingSource, EMPTY_STORED_BLOCK, FailingSource, FullSink, RawEntry, RecordingSource,
    UnreadableRangeSource, Zip, ZipEntry, build_zip, end_record, final_stored_block, fixed_zeros,
    flushed, parse_zip, sha256,
};

const M1_1_PACKAGE: &[u8] = include_bytes!("fixtures/cdoc_m1_1/package.xlsx");
const M1_1_LYING_SIZE: &[u8] = include_bytes!("fixtures/cdoc_m1_1/lying-size.xlsx");
const M1_1_MANIFEST: &str = include_str!("fixtures/cdoc_m1_1/manifest.json");

const SPREADSHEET_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";

fn memory(bytes: &[u8]) -> Arc<dyn PackageRead> {
    Arc::new(MemorySource::new(bytes.to_vec()))
}

fn opened(source: Arc<dyn PackageRead>, policy: &OpenPolicy) -> OpenedDocument {
    match calc_doc::open(source, policy) {
        Ok(document) => document,
        Err(error) => panic!("the package opens, but: {error}"),
    }
}

fn refused(source: Arc<dyn PackageRead>, policy: &OpenPolicy) -> OpenError {
    match calc_doc::open(source, policy) {
        Ok(_) => panic!("the package is refused, but it opened"),
        Err(error) => error,
    }
}

/// The plan of a save of a snapshot of an opened document as opened, which is never refused.
fn unedited(snapshot: &calc_doc::DocumentSnapshot) -> calc_doc::SavePlan {
    prepare_save(snapshot, &calc_doc::SavePolicy::default())
        .expect("a snapshot of an opened document as opened is not refused")
}

/// Saves the document's snapshot unchanged into a new buffer, with the save's receipt.
fn saved_with_receipt(document: &OpenedDocument) -> (Vec<u8>, calc_doc::SaveReceipt) {
    let mut out = Vec::new();
    let receipt = calc_doc::write(unedited(&document.snapshot()), &mut out).expect("a save");
    assert_eq!(
        receipt.bytes_written,
        out.len() as u64,
        "the receipt counts the bytes the sink received"
    );
    (out, receipt)
}

/// Saves the document's snapshot unchanged into a new buffer.
fn saved(document: &OpenedDocument) -> Vec<u8> {
    saved_with_receipt(document).0
}

/// What the case's own pass over a part's XML events finds.
#[derive(Debug, PartialEq)]
struct XmlFacts {
    namespace: Option<String>,
    root: String,
    elements: u64,
    attributes: u64,
    text_sha256: String,
}

fn xml_facts(part: &XmlPart) -> XmlFacts {
    let mut reader = part.reader();
    let mut root = None;
    let (mut elements, mut attributes, mut text) = (0, 0, String::new());
    while let Some(event) = reader.next_event().expect("well-formed XML") {
        match event {
            XmlEvent::Start(element) => {
                root.get_or_insert_with(|| element.name.clone());
                elements += 1;
                attributes += element.attributes.len() as u64;
            }
            XmlEvent::Text(content) => text.push_str(&content),
            XmlEvent::End(_) => {}
        }
    }
    let root = root.expect("a document element");
    XmlFacts {
        namespace: root.namespace.map(String::from),
        root: String::from(root.local),
        elements,
        attributes,
        text_sha256: sha256(text.as_bytes()),
    }
}

fn manifest_xml_facts(xml: &Value) -> XmlFacts {
    XmlFacts {
        namespace: Some(xml["namespace"].as_str().expect("namespace").to_owned())
            .filter(|namespace| !namespace.is_empty()),
        root: xml["root"].as_str().expect("root").to_owned(),
        elements: xml["elements"].as_u64().expect("elements"),
        attributes: xml["attributes"].as_u64().expect("attributes"),
        text_sha256: xml["text_sha256"].as_str().expect("text_sha256").to_owned(),
    }
}

/// The stored fields two archives' entries must share for the entry to count as preserved.
fn preserved_fields(entry: &ZipEntry, archive: &[u8]) -> Vec<String> {
    vec![
        format!("name {}", entry.name),
        format!(
            "made by {} needs {}",
            entry.version_made_by, entry.version_needed
        ),
        format!("flags {:#06x} method {}", entry.flags, entry.method),
        format!("time {:#06x} date {:#06x}", entry.time, entry.date),
        format!(
            "crc {:08x} sizes {} {}",
            entry.crc32, entry.compressed_size, entry.size
        ),
        format!(
            "attributes {} {}",
            entry.internal_attributes, entry.external_attributes
        ),
        format!("local header {:02x?}", &entry.local_header[4..26]),
        format!("local extra {:02x?}", entry.local_extra),
        format!("central extra {:02x?}", entry.central_extra),
        format!("comment {:02x?}", entry.comment),
        format!("descriptor {:02x?}", entry.descriptor),
        format!("payload {}", sha256(entry.payload(archive))),
    ]
}

fn names_in_stored_order(zip: &Zip) -> Vec<String> {
    let mut entries: Vec<&ZipEntry> = zip.entries.iter().collect();
    entries.sort_by_key(|entry| entry.local_offset);
    entries.iter().map(|entry| entry.name.clone()).collect()
}

// ---------------------------------------------------------------------------------------------
// Case cdoc_m1_1__raw_preservation.

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cdoc_m1_1__raw_preservation() {
    const CASE: &str = "cdoc_m1_1__raw_preservation";
    assert_eq!(
        sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "the case's SHA-256 matches FIPS 180-4's example"
    );
    let manifest: Value = serde_json::from_str(M1_1_MANIFEST).expect("manifest.json");
    let fixture = &manifest["package"];
    let entries = fixture["entries"].as_array().expect("entries");
    let policy = OpenPolicy::default();

    // The fixture as the case's own reader sees it agrees with what the generator recorded.
    let input = parse_zip(M1_1_PACKAGE);
    assert_eq!(
        M1_1_PACKAGE.len() as u64,
        fixture["length"].as_u64().unwrap()
    );
    assert_eq!(input.entries.len(), entries.len());
    for (entry, recorded) in input.entries.iter().zip(entries) {
        let field = |key: &str| recorded[key].as_u64().unwrap();
        assert_eq!(entry.name, recorded["name"].as_str().unwrap());
        assert_eq!(u64::from(entry.method), field("method"));
        assert_eq!(u64::from(entry.flags), field("flags"));
        assert_eq!(u64::from(entry.crc32), field("crc32"));
        assert_eq!(u64::from(entry.compressed_size), field("compressed_size"));
        assert_eq!(u64::from(entry.size), field("size"));
        assert_eq!(u64::from(entry.local_offset), field("local_offset"));
        assert_eq!(entry.descriptor.len() as u64, field("descriptor"));
        assert_eq!(entry.local_extra.len() as u64, field("local_extra"));
        assert_eq!(entry.central_extra.len() as u64, field("central_extra"));
        assert_eq!(
            entry.comment,
            recorded["comment"].as_str().unwrap().as_bytes()
        );
        assert_eq!(
            sha256(entry.payload(M1_1_PACKAGE)),
            recorded["payload_sha256"].as_str().unwrap(),
            "{}: the payload's SHA-256 as the generator recorded it",
            entry.name
        );
    }
    let gap_end = input.entry("xl/worksheets/sheet2.xml").local_offset as usize;
    let gap = fixture["unreferenced_bytes"].as_u64().unwrap() as usize;
    let before_gap = input
        .entries
        .iter()
        .map(ZipEntry::end)
        .filter(|&end| end <= gap_end)
        .max()
        .unwrap();
    assert_eq!(gap_end - before_gap, gap, "no entry stores the gap's bytes");

    // Open: the part directory, content types and relationships.
    let document = opened(memory(M1_1_PACKAGE), &policy);
    let package = document.package();
    let recorded_parts: Vec<&Value> = entries
        .iter()
        .filter(|entry| entry["part"].as_bool().unwrap())
        .collect();
    let directory: Vec<(String, Option<String>, bool, u64, u64, u32)> = package
        .parts()
        .iter()
        .map(|part| {
            (
                part.name().to_owned(),
                part.content_type().map(str::to_owned),
                part.compressed(),
                part.compressed_size(),
                part.size(),
                part.crc32(),
            )
        })
        .collect();
    let expected_directory: Vec<(String, Option<String>, bool, u64, u64, u32)> = recorded_parts
        .iter()
        .map(|entry| {
            (
                entry["name"].as_str().unwrap().to_owned(),
                entry["content_type"].as_str().map(str::to_owned),
                entry["method"].as_u64() == Some(8),
                entry["compressed_size"].as_u64().unwrap(),
                entry["size"].as_u64().unwrap(),
                entry["crc32"].as_u64().unwrap() as u32,
            )
        })
        .collect();
    assert_eq!(
        directory, expected_directory,
        "the part directory: every entry but [Content_Types].xml and the directory entry, with the content type the generator assigned"
    );
    let relationships: Vec<Value> = package
        .relationships()
        .iter()
        .map(|set| {
            json!({
                "part": set.part(),
                "source": set.source(),
                "items": set.items().iter().map(|item| json!({
                    "id": item.id,
                    "type": item.kind,
                    "target": item.target,
                    "mode": match item.mode {
                        TargetMode::Internal => "Internal",
                        TargetMode::External => "External",
                    },
                    "resolved": item.resolved,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    assert_eq!(
        Value::Array(relationships),
        fixture["relationships"],
        "every relationships part, its source and each relationship with the part its target denotes"
    );
    let workbook_relationships = package
        .relationships_of(Some("xl/workbook.xml"))
        .expect("the workbook's relationships");
    assert_eq!(workbook_relationships.items().len(), 4);

    // Lazy reads: each part's content and, for XML parts, what its reader yields.
    let mut xml_parts = 0;
    for recorded in &recorded_parts {
        let name = recorded["name"].as_str().unwrap();
        let content = package.read_part(name).expect("a part's content");
        assert_eq!(
            sha256(&content),
            recorded["content_sha256"].as_str().unwrap(),
            "{name}: the content's SHA-256 as the generator recorded it before deflating"
        );
        if !recorded["xml"].is_null() {
            let part = package.read_xml(name).expect("an XML part");
            assert_eq!(
                xml_facts(&part),
                manifest_xml_facts(&recorded["xml"]),
                "{name}: the document element, element and attribute counts and text .NET's XmlDocument reads"
            );
            xml_parts += 1;
        }
    }
    assert_eq!(
        package.read_part("XL/WORKBOOK.XML"),
        package.read_part("xl/workbook.xml"),
        "part names compare ignoring ASCII case"
    );

    // A no-op save, inspected by the case's own reader.
    let plan = unedited(&document.snapshot());
    assert_eq!(plan.copied_entries(), entries.len());
    let (out, receipt) = saved_with_receipt(&document);
    assert_eq!(
        receipt.gap_bytes_dropped, 37,
        "the receipt reports the 37 bytes between entries that the save did not write"
    );
    assert_eq!(receipt.gap_bytes_dropped, gap as u64);
    let output = parse_zip(&out);
    assert_eq!(
        output
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect::<Vec<_>>(),
        input
            .entries
            .iter()
            .map(|e| e.name.clone())
            .collect::<Vec<_>>(),
        "the central directory keeps its order"
    );
    assert_eq!(
        names_in_stored_order(&output),
        names_in_stored_order(&input),
        "the entries keep the order they were stored in"
    );
    for ((before, after), recorded) in input.entries.iter().zip(&output.entries).zip(entries) {
        assert_eq!(
            preserved_fields(after, &out),
            preserved_fields(before, M1_1_PACKAGE),
            "{}: every stored field and the payload",
            before.name
        );
        assert_eq!(
            sha256(after.payload(&out)),
            recorded["payload_sha256"].as_str().unwrap(),
            "{}: the saved payload's SHA-256 is the one the generator recorded",
            before.name
        );
        let shift = if (before.local_offset as usize) >= gap_end {
            gap
        } else {
            0
        };
        assert_eq!(
            after.local_offset as usize,
            before.local_offset as usize - shift,
            "{}: moved back by the unreferenced bytes before it, and no further",
            before.name
        );
    }
    assert_eq!(output.comment, input.comment, "the archive comment");
    assert_eq!(out.len(), M1_1_PACKAGE.len() - gap);
    assert_eq!(output.central_size, input.central_size);
    assert_eq!(output.central_offset, input.central_offset - gap);

    // The saved package reopens with the same structure and content, and saves to itself.
    let reopened = opened(memory(&out), &policy);
    let again = reopened.package();
    assert_eq!(again.content_types(), package.content_types());
    assert_eq!(again.relationships(), package.relationships());
    assert_eq!(
        again
            .parts()
            .iter()
            .map(|part| (part.name(), part.content_type(), part.crc32()))
            .collect::<Vec<_>>(),
        package
            .parts()
            .iter()
            .map(|part| (part.name(), part.content_type(), part.crc32()))
            .collect::<Vec<_>>()
    );
    for recorded in &recorded_parts {
        let name = recorded["name"].as_str().unwrap();
        assert_eq!(
            sha256(&again.read_part(name).expect("a part's content")),
            recorded["content_sha256"].as_str().unwrap(),
            "{name} after the save"
        );
    }
    let (second, second_receipt) = saved_with_receipt(&reopened);
    assert!(second == out, "saving the saved package changes no byte");
    assert_eq!(
        second_receipt.gap_bytes_dropped, 0,
        "the saved package has no bytes outside its entries to drop"
    );

    // Bytes before the first entry and after the last one are dropped and counted too.
    let gapped_entries = base_entries();
    let mut gapped = b"prefix".to_vec();
    let mut centrals = Vec::new();
    for entry in &gapped_entries {
        centrals.extend_from_slice(&entry.central(gapped.len() as u32));
        gapped.extend_from_slice(&entry.local());
    }
    gapped.extend_from_slice(b"tail");
    let directory = gapped.len() as u32;
    gapped.extend_from_slice(&centrals);
    gapped.extend_from_slice(&end_record(3, centrals.len() as u32, directory));
    let (gapped_out, gapped_receipt) = saved_with_receipt(&opened(memory(&gapped), &policy));
    assert_eq!(
        gapped_receipt.gap_bytes_dropped, 10,
        "the 6 bytes before the first entry and the 4 after the last"
    );
    assert_eq!(gapped_out.len(), gapped.len() - 10);
    assert_eq!(gapped_out, build_zip(&gapped_entries));

    // A save proves it copied what was opened.
    let changing = Arc::new(ChangingSource::new(M1_1_PACKAGE.to_vec()));
    let source: Arc<dyn PackageRead> = changing.clone();
    let changed = opened(source, &policy);
    let sheet = input.entry("xl/worksheets/sheet1.xml");
    changing.change(|bytes| bytes[sheet.payload_start + 100] ^= 0x01);
    let mut partial = Vec::new();
    let error = calc_doc::write(unedited(&changed.snapshot()), &mut partial)
        .expect_err("a source changed since opening");
    assert_eq!(
        error,
        WriteError::SourceChanged {
            entry: "xl/worksheets/sheet1.xml".to_owned()
        }
    );
    let lazy = changed
        .package()
        .read_part("xl/worksheets/sheet1.xml")
        .expect_err("a changed payload");
    changing.change(|bytes| bytes.truncate(1000));
    let error = calc_doc::write(unedited(&changed.snapshot()), &mut Vec::new())
        .expect_err("a source that shrank");
    assert!(matches!(error, WriteError::Source(_)), "{error:?}");
    let mut full = FullSink {
        accepted: Vec::new(),
        capacity: 10_000,
    };
    let error = calc_doc::write(unedited(&document.snapshot()), &mut full)
        .expect_err("a sink that fills up");
    assert_eq!(
        error,
        WriteError::Sink(SinkError::new("the medium is full"))
    );
    assert!(
        full.accepted.len() <= 10_000 && out.starts_with(&full.accepted),
        "the sink holds a prefix of the package"
    );

    note!(
        "ACCEPT-NOTE: {CASE}: package.xlsx ({} bytes, {} entries, {} parts, {} XML parts read lazily to the .NET XmlDocument counts, {} relationships parts) saved unchanged to {} bytes: every entry's stored fields and payload SHA-256 equal the source's and the generator's, read back by the case's own ZIP reader; the {gap} unreferenced bytes were dropped, the receipt reported gap_bytes_dropped {}, and the entries after them moved back by {gap}; the reopened package has the same content types, relationships and part contents and saves to itself byte for byte with gap_bytes_dropped {}; a package with 6 bytes before its first entry and 4 after its last saved {} bytes shorter with gap_bytes_dropped {}",
        M1_1_PACKAGE.len(),
        entries.len(),
        recorded_parts.len(),
        xml_parts,
        package.relationships().len(),
        out.len(),
        receipt.gap_bytes_dropped,
        second_receipt.gap_bytes_dropped,
        gapped.len() - gapped_out.len(),
        gapped_receipt.gap_bytes_dropped
    );
    note!(
        "ACCEPT-NOTE: {CASE}: a payload byte changed after opening: the save refused with SourceChanged after {} bytes, and the lazy read with {lazy:?}; a source that shrank refused with Source; a sink full at 10000 bytes refused with Sink after taking a prefix of {} bytes",
        partial.len(),
        full.accepted.len()
    );
}

// ---------------------------------------------------------------------------------------------
// Case cdoc_m1_1__malformed_zip.

fn base_content_types() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="{}"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="bin" ContentType="application/octet-stream"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/></Types>"#,
        calc_doc::CONTENT_TYPES_NAMESPACE
    )
}

fn base_relationships() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{}"><Relationship Id="rId1" Type="{OFFICE_DOCUMENT}" Target="xl/workbook.xml"/></Relationships>"#,
        calc_doc::RELATIONSHIPS_NAMESPACE
    )
}

fn base_workbook() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="{SPREADSHEET_MAIN}"><sheets/></workbook>"#
    )
}

/// The three entries of a minimal package that opens.
fn base_entries() -> Vec<RawEntry> {
    vec![
        RawEntry::stored("[Content_Types].xml", base_content_types().as_bytes()),
        RawEntry::stored("_rels/.rels", base_relationships().as_bytes()),
        RawEntry::stored("xl/workbook.xml", base_workbook().as_bytes()),
    ]
}

/// The minimal package with `extra` entries after its own.
fn package_with(extra: Vec<RawEntry>) -> Vec<u8> {
    let mut entries = base_entries();
    entries.extend(extra);
    build_zip(&entries)
}

/// The minimal package with one entry's content replaced.
fn package_replacing(name: &str, content: &[u8]) -> Vec<u8> {
    let entries: Vec<RawEntry> = base_entries()
        .into_iter()
        .map(|entry| {
            if entry.name == name.as_bytes() {
                RawEntry::stored(name, content)
            } else {
                entry
            }
        })
        .collect();
    build_zip(&entries)
}

/// The end record's offset in an archive without a comment.
fn end_offset(bytes: &[u8]) -> usize {
    bytes.len() - 22
}

/// The most compressed bytes a deflated entry of `size` bytes may store: the expansion of stored
/// or fixed Huffman blocks, size + ceil(size / 8) + ceil(size / 64), and 1024 bytes for flush
/// markers and empty blocks.
fn deflate_bound(size: u64) -> u64 {
    size + size.div_ceil(8) + size.div_ceil(64) + 1024
}

/// A valid raw DEFLATE stream of `content` padded with `blocks` empty stored blocks before its
/// final block: 5 * `blocks` + 5 + `content.len()` bytes.
fn padded_stream(blocks: usize, content: &[u8]) -> Vec<u8> {
    let mut stream = EMPTY_STORED_BLOCK.repeat(blocks);
    stream.extend_from_slice(&final_stored_block(content));
    stream
}

/// One read of a part, as [`counted_read`] observed it.
struct CountedRead {
    /// What the read returned.
    result: Result<Vec<u8>, PartReadError>,
    /// The bytes this thread asked the allocator for during the read.
    allocated: u64,
    /// The byte ranges the read asked the source for.
    reads: Vec<(u64, u64)>,
}

/// Reads the part `name` of a document opened from `source`, counting what the read allocated
/// and recording what it read.
fn counted_read(document: &OpenedDocument, source: &RecordingSource, name: &str) -> CountedRead {
    let reads_before = source.reads().len();
    let before = cdoc_m1_1_support::allocated();
    let result = document.package().read_part(name);
    let allocated = cdoc_m1_1_support::allocated() - before;
    let reads = source.reads()[reads_before..].to_vec();
    CountedRead {
        result,
        allocated,
        reads,
    }
}

/// The ranges among `reads` that touch the payload of `entry`.
fn payload_reads(reads: &[(u64, u64)], entry: &ZipEntry) -> Vec<(u64, u64)> {
    reads
        .iter()
        .copied()
        .filter(|&(start, end)| {
            start < entry.payload_end as u64 && end > entry.payload_start as u64
        })
        .collect()
}

/// The files in the case's working directory and its parent, where a naive extractor of the
/// planted names would write. Browsers give the case no file system.
#[cfg(not(target_arch = "wasm32"))]
fn file_system_listing() -> Vec<String> {
    let mut listing = Vec::new();
    for directory in [".", ".."] {
        for entry in std::fs::read_dir(directory).expect("a readable directory") {
            let entry = entry.expect("a directory entry");
            let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            listing.push(format!(
                "{directory}/{} {size}",
                entry.file_name().to_string_lossy()
            ));
        }
    }
    listing.sort();
    listing
}

#[cfg(target_arch = "wasm32")]
fn file_system_listing() -> Vec<String> {
    Vec::new()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cdoc_m1_1__malformed_zip() {
    const CASE: &str = "cdoc_m1_1__malformed_zip";
    let manifest: Value = serde_json::from_str(M1_1_MANIFEST).expect("manifest.json");
    let default = OpenPolicy::default();
    let listing_before = file_system_listing();
    let mut refusals: Vec<String> = Vec::new();
    let mut expect = |label: &str, error: OpenError, expected: OpenError| {
        assert_eq!(error, expected, "{label}");
        refusals.push(format!("{label}: {error}"));
    };

    // Control: the minimal package opens, so each refusal below is its planted defect's.
    let base_bytes = build_zip(&base_entries());
    let base = opened(memory(&base_bytes), &default);
    assert_eq!(
        base.package()
            .parts()
            .iter()
            .map(|part| part.name())
            .collect::<Vec<_>>(),
        ["_rels/.rels", "xl/workbook.xml"]
    );
    assert_eq!(
        base.package().relationships()[0].items()[0]
            .resolved
            .as_deref(),
        Some("xl/workbook.xml")
    );

    // Path traversal: names a naive extractor would write outside its directory.
    let traversal = [
        ("../cdoc_m1_1_zip_slip.xml", UnsafeName::DotSegment),
        ("xl/../../cdoc_m1_1_zip_slip.xml", UnsafeName::DotSegment),
        ("xl/./cdoc_m1_1_zip_slip.xml", UnsafeName::DotSegment),
        ("/tmp/cdoc_m1_1_zip_slip.xml", UnsafeName::Absolute),
        ("..\\cdoc_m1_1_zip_slip.xml", UnsafeName::Backslash),
        ("C:/cdoc_m1_1_zip_slip.xml", UnsafeName::Colon),
        ("xl/workbook.xml:cdoc_m1_1_zip_slip", UnsafeName::Colon),
        ("xl//cdoc_m1_1_zip_slip.xml", UnsafeName::EmptySegment),
        (
            "xl/cdoc_m1_1\u{1}zip_slip.xml",
            UnsafeName::ControlCharacter,
        ),
        ("", UnsafeName::Empty),
    ];
    #[cfg(not(target_arch = "wasm32"))]
    let planted_targets: Vec<std::path::PathBuf> = traversal
        .iter()
        .filter(|(name, _)| !name.is_empty() && !name.contains('\u{1}'))
        .map(|(name, _)| std::path::Path::new(".").join(name.replace('\\', "/")))
        .collect();
    #[cfg(not(target_arch = "wasm32"))]
    for target in &planted_targets {
        assert!(
            !target.exists(),
            "{} exists before the case",
            target.display()
        );
    }
    for (name, reason) in traversal {
        let bytes = package_with(vec![RawEntry::stored(name, b"planted")]);
        expect(
            &format!("entry name {name:?}"),
            refused(memory(&bytes), &default),
            OpenError::UnsafeEntryName {
                name: name.to_owned(),
                reason,
            },
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    for target in &planted_targets {
        assert!(
            !target.exists(),
            "{} exists after the case",
            target.display()
        );
    }
    let mut unflagged = RawEntry::stored("x", b"planted");
    unflagged.name = b"xl/caf\xe9.bin".to_vec();
    let bytes = package_with(vec![unflagged.clone()]);
    let end = end_offset(&bytes);
    let central = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as u64;
    let fourth_record = central
        + base_entries()
            .iter()
            .map(|entry| 46 + entry.name.len() as u64)
            .sum::<u64>();
    expect(
        "a Latin-1 name without the UTF-8 flag",
        refused(memory(&bytes), &default),
        OpenError::EntryNameEncoding {
            offset: fourth_record,
        },
    );
    let mut flagged = unflagged;
    flagged.flags = 1 << 11;
    expect(
        "an invalid UTF-8 name with the UTF-8 flag",
        refused(memory(&package_with(vec![flagged])), &default),
        OpenError::EntryNameEncoding {
            offset: fourth_record,
        },
    );

    // Declared-size bombs, refused from the central directory before any payload is read.
    let empty_deflate = [0x03u8, 0x00];
    let mut bomb_entries = vec![RawEntry::deflated(
        "xl/media/bomb.bin",
        &empty_deflate,
        0xFFFF_FFFE,
        0,
    )];
    bomb_entries.push(RawEntry::stored(
        "xl/media/filler.bin",
        &vec![0x5Au8; 100_000],
    ));
    bomb_entries.extend(base_entries());
    let bomb_bytes = build_zip(&bomb_entries);
    let bomb_entry = parse_zip(&bomb_bytes).entry("xl/media/bomb.bin").clone();
    let recording = Arc::new(RecordingSource::new(bomb_bytes.clone()));
    let source: Arc<dyn PackageRead> = recording.clone();
    expect(
        "an entry declaring 4 GiB - 2 bytes",
        refused(source, &default),
        OpenError::PartTooLarge {
            name: "xl/media/bomb.bin".to_owned(),
            declared: 0xFFFF_FFFE,
            limit: 1 << 30,
        },
    );
    let reads = recording.reads();
    let touched: Vec<&(u64, u64)> = reads
        .iter()
        .filter(|(start, end)| {
            *start < bomb_entry.end() as u64 && *end > u64::from(bomb_entry.local_offset)
        })
        .collect();
    assert_eq!(
        touched,
        Vec::<&(u64, u64)>::new(),
        "no read reached the bomb entry's local header or payload"
    );
    let read_bytes: u64 = reads.iter().map(|(start, end)| end - start).sum();
    let tight = OpenPolicy {
        package: PackageLimits {
            max_part_bytes: 1000,
            ..PackageLimits::default()
        },
        ..OpenPolicy::default()
    };
    expect(
        "a 2000-byte entry under a 1000-byte part limit",
        refused(
            memory(&package_with(vec![RawEntry::stored(
                "xl/media/big.bin",
                &[7u8; 2000],
            )])),
            &tight,
        ),
        OpenError::PartTooLarge {
            name: "xl/media/big.bin".to_owned(),
            declared: 2000,
            limit: 1000,
        },
    );
    let many: Vec<RawEntry> = (0..6)
        .map(|i| {
            RawEntry::deflated(
                &format!("xl/media/big{i}.bin"),
                &empty_deflate,
                900_000_000,
                0,
            )
        })
        .collect();
    let declared = 6 * 900_000_000
        + base_entries()
            .iter()
            .map(|entry| u64::from(entry.size))
            .sum::<u64>();
    expect(
        "six entries declaring 900 MB each",
        refused(memory(&package_with(many)), &default),
        OpenError::PackageExpansionTooLarge {
            declared,
            limit: 4 << 30,
        },
    );
    let few = OpenPolicy {
        package: PackageLimits {
            max_entries: 4,
            ..PackageLimits::default()
        },
        ..OpenPolicy::default()
    };
    expect(
        "five entries under a four-entry limit",
        refused(
            memory(&package_with(vec![
                RawEntry::stored("xl/a.bin", b"a"),
                RawEntry::stored("xl/b.bin", b"b"),
            ])),
            &few,
        ),
        OpenError::TooManyEntries {
            declared: 5,
            limit: 4,
        },
    );
    let small = OpenPolicy {
        package: PackageLimits {
            max_package_bytes: 100,
            ..PackageLimits::default()
        },
        ..OpenPolicy::default()
    };
    expect(
        "a package over a 100-byte package limit",
        refused(memory(&base_bytes), &small),
        OpenError::PackageTooLarge {
            length: base_bytes.len() as u64,
            limit: 100,
        },
    );

    // Overlapping entries: the outer entry's payload is the inner entry, so the inner payload
    // would be counted, and expanded, twice.
    let inner = RawEntry::stored("xl/media/b.bin", b"the inner entry");
    let outer = RawEntry::stored("xl/media/a.bin", &inner.local());
    let mut overlap = Vec::new();
    let mut centrals = Vec::new();
    for entry in base_entries() {
        let offset = overlap.len() as u32;
        overlap.extend_from_slice(&entry.local());
        centrals.extend_from_slice(&entry.central(offset));
    }
    let outer_offset = overlap.len() as u32;
    overlap.extend_from_slice(&outer.local());
    centrals.extend_from_slice(&outer.central(outer_offset));
    centrals.extend_from_slice(&inner.central(outer_offset + 30 + outer.name.len() as u32));
    let directory = overlap.len() as u32;
    overlap.extend_from_slice(&centrals);
    overlap.extend_from_slice(&end_record(5, centrals.len() as u32, directory));
    expect(
        "an entry stored inside another",
        refused(memory(&overlap), &default),
        OpenError::OverlappingEntries {
            first: "xl/media/a.bin".to_owned(),
            second: "xl/media/b.bin".to_owned(),
        },
    );

    // Compressed payloads larger than their declared size allows: refused before any of the
    // payload is read. lying-size.xlsx's zeros.bin is 64 MiB of zeros deflated, declaring 4096
    // bytes.
    let lying = &manifest["lying_size"];
    let declared_size = lying["declared_size"].as_u64().unwrap();
    let expanded_size = lying["expanded_size"].as_u64().unwrap();
    let lying_in = parse_zip(M1_1_LYING_SIZE);
    let lying_entry = lying_in.entry("xl/media/zeros.bin").clone();
    let lying_source = Arc::new(RecordingSource::new(M1_1_LYING_SIZE.to_vec()));
    let source: Arc<dyn PackageRead> = lying_source.clone();
    let document = opened(source, &default);
    let zeros = document
        .package()
        .part("xl/media/zeros.bin")
        .expect("the part");
    assert_eq!(zeros.size(), declared_size);
    assert_eq!(
        u64::from(zeros.crc32()),
        lying["declared_crc32"].as_u64().unwrap()
    );
    assert_eq!(
        zeros.compressed_size(),
        u64::from(lying_entry.compressed_size)
    );
    assert_eq!(deflate_bound(declared_size), 5696, "4096 + 512 + 64 + 1024");
    let CountedRead {
        result,
        allocated: lying_allocated,
        reads: lying_reads,
    } = counted_read(&document, &lying_source, "xl/media/zeros.bin");
    assert_eq!(
        result,
        Err(PartReadError::CompressedPayloadTooLarge {
            compressed: u64::from(lying_entry.compressed_size),
            declared: declared_size,
            bound: 5696,
        })
    );
    assert_eq!(
        payload_reads(&lying_reads, &lying_entry),
        Vec::<(u64, u64)>::new(),
        "the refused read asked the source for none of the payload"
    );
    assert!(
        lying_allocated < u64::from(lying_entry.compressed_size),
        "refusing a {}-byte payload allocated {lying_allocated} bytes",
        lying_entry.compressed_size
    );
    assert!(document.package().read_xml("xl/media/zeros.bin").is_err());
    let lying_saved = saved(&document);
    let lying_out = parse_zip(&lying_saved);
    assert_eq!(
        sha256(lying_out.entry("xl/media/zeros.bin").payload(&lying_saved)),
        sha256(lying_entry.payload(M1_1_LYING_SIZE)),
        "the refused part's payload is still preserved by a save"
    );
    // A payload that is valid DEFLATE of exactly its declared content and CRC, padded with
    // 800 000 empty stored blocks to 4 000 012 bytes where its declared 7 bytes allow 1033:
    // refused by its sizes alone, before any of it is read.
    let padded = padded_stream(800_000, b"content");
    assert_eq!(padded.len(), 4_000_012);
    assert_eq!(deflate_bound(7), 1033, "7 + 1 + 1 + 1024");
    assert_eq!(
        cdoc_m1_3_support::inflate(&padded),
        b"content",
        "the case's own decoder reads the padded stream as the declared content"
    );
    let padded_bytes = package_with(vec![RawEntry::deflated(
        "xl/media/padded.bin",
        &padded,
        7,
        cdoc_m1_1_support::crc32(b"content"),
    )]);
    let padded_entry = parse_zip(&padded_bytes)
        .entry("xl/media/padded.bin")
        .clone();
    let padded_source = Arc::new(RecordingSource::new(padded_bytes));
    let source: Arc<dyn PackageRead> = padded_source.clone();
    let document = opened(source, &default);
    let CountedRead {
        result,
        allocated: padded_allocated,
        reads: padded_reads,
    } = counted_read(&document, &padded_source, "xl/media/padded.bin");
    assert_eq!(
        result,
        Err(PartReadError::CompressedPayloadTooLarge {
            compressed: 4_000_012,
            declared: 7,
            bound: 1033,
        }),
        "an entry with an oversized compressed payload"
    );
    assert_eq!(
        payload_reads(&padded_reads, &padded_entry),
        Vec::<(u64, u64)>::new(),
        "the refused read asked the source for none of the payload"
    );
    assert!(
        padded_allocated < 64 * 1024,
        "refusing a 4000012-byte payload allocated {padded_allocated} bytes"
    );
    // The same padding on a structural part, which opening reads: refused at open.
    let rels = base_relationships();
    let rels_stream = padded_stream(800_000, rels.as_bytes());
    let rels_entries: Vec<RawEntry> = base_entries()
        .into_iter()
        .map(|entry| {
            if entry.name == b"_rels/.rels" {
                RawEntry::deflated(
                    "_rels/.rels",
                    &rels_stream,
                    rels.len() as u32,
                    cdoc_m1_1_support::crc32(rels.as_bytes()),
                )
            } else {
                entry
            }
        })
        .collect();
    let rels_source = memory(&build_zip(&rels_entries));
    let before = cdoc_m1_1_support::allocated();
    let error = refused(rels_source, &default);
    let rels_allocated = cdoc_m1_1_support::allocated() - before;
    expect(
        "_rels/.rels padded to over 4 MB",
        error,
        OpenError::Part {
            part: "_rels/.rels".to_owned(),
            error: PartReadError::CompressedPayloadTooLarge {
                compressed: rels_stream.len() as u64,
                declared: rels.len() as u64,
                bound: deflate_bound(rels.len() as u64),
            },
        },
    );
    assert!(
        rels_allocated < 1 << 20,
        "opening a package whose {}-byte relationships payload is refused allocated {rels_allocated} bytes",
        rels_stream.len()
    );

    // Expansion past the declared size within the compressed-size bound: one fixed Huffman block
    // of 45 000 matches of 258 zeros, 73 128 bytes where its declared 65 536 bytes allow 74 757,
    // with the CRC-32 of 65 536 zeros, so a reader that stopped at the declared size would find
    // the declared CRC. Refused when read, the decoder's output stopped at the declared size.
    let zeros_stream = fixed_zeros(45_000);
    let zeros_expanded = cdoc_m1_3_support::inflate(&zeros_stream);
    assert_eq!(zeros_expanded.len(), 1 + 258 * 45_000);
    assert!(zeros_expanded.iter().all(|&byte| byte == 0));
    drop(zeros_expanded);
    assert_eq!(zeros_stream.len(), 73_128);
    assert_eq!(deflate_bound(65_536), 75_776, "65536 + 8192 + 1024 + 1024");
    let zeros_bytes = package_with(vec![RawEntry::deflated(
        "xl/media/zeros.bin",
        &zeros_stream,
        65_536,
        cdoc_m1_1_support::crc32(&vec![0u8; 65_536]),
    )]);
    let zeros_source = Arc::new(RecordingSource::new(zeros_bytes));
    let source: Arc<dyn PackageRead> = zeros_source.clone();
    let document = opened(source, &default);
    let CountedRead {
        result,
        allocated: zeros_allocated,
        ..
    } = counted_read(&document, &zeros_source, "xl/media/zeros.bin");
    assert_eq!(
        result,
        Err(PartReadError::ExpandsPastDeclaredSize { declared: 65_536 })
    );
    assert!(
        zeros_allocated < 1 << 20,
        "reading a part that expands to {} bytes allocated {zeros_allocated} bytes",
        1 + 258 * 45_000
    );
    let zeros_saved = saved(&document);
    assert_eq!(
        sha256(
            parse_zip(&zeros_saved)
                .entry("xl/media/zeros.bin")
                .payload(&zeros_saved)
        ),
        sha256(&zeros_stream),
        "the refused part's payload is still preserved by a save"
    );

    // A part whose decoder buffer grows: one fixed Huffman block of 3876 matches of 258 zeros,
    // declaring its 1 000 009 bytes. The decoder starts its buffer at twice the payload, under a
    // quarter of the declared size, and doubles it toward the declared size. The largest single
    // allocation of the read is that buffer at the declared size, never more: growing it the way
    // a vector grows by default would ask for about 1.6 MB.
    let grown_stream = fixed_zeros(3_876);
    let grown_size: usize = 1 + 258 * 3_876;
    assert!(
        2 * grown_stream.len() < grown_size / 2,
        "the buffer starts below half the declared size and grows"
    );
    let grown_bytes = package_with(vec![RawEntry::deflated(
        "xl/media/grown.bin",
        &grown_stream,
        u32::try_from(grown_size).expect("a declared size"),
        cdoc_m1_1_support::crc32(&vec![0u8; grown_size]),
    )]);
    let grown_source = Arc::new(RecordingSource::new(grown_bytes));
    let source: Arc<dyn PackageRead> = grown_source.clone();
    let document = opened(source, &default);
    cdoc_m1_1_support::reset_largest();
    let grown = document.package().read_part("xl/media/grown.bin");
    let grown_largest = cdoc_m1_1_support::largest();
    let grown = grown.expect("a part of exactly its declared size and CRC reads");
    assert_eq!(grown.len(), grown_size);
    assert!(grown.iter().all(|&byte| byte == 0));
    assert!(
        grown_largest <= grown_size as u64,
        "reading a part declaring {grown_size} bytes asked for {grown_largest} bytes at once"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: a fixed Huffman payload of {} bytes declaring {grown_size} read back; the largest single allocation of the read was {grown_largest} bytes",
        grown_stream.len()
    );

    // Payloads that are short, corrupt, carry the wrong CRC, lie at or past the compressed-size
    // bound or hold bytes after their final block. `stored_block` is a raw DEFLATE stream of one
    // final stored block (RFC 1951 section 3.2.4) holding `content`.
    let stored_block = final_stored_block;
    // 33 bytes, which allow 33 + 5 + 1 + 1024 = 1063 compressed bytes: 205 empty stored blocks
    // and the final block are exactly 1063.
    let at_bound: &[u8] = b"0123456789abcdefghijklmnopqrstuvw";
    assert_eq!(deflate_bound(33), 1063);
    assert_eq!(padded_stream(205, at_bound).len(), 1063);
    // What a streaming encoder writes when it ends an entry with a sync flush and then finishes:
    // a flush marker and an empty final block after the content, 7 bytes for an empty entry and
    // 9 for one byte, past that expansion of 0 and 1 bytes (0 and 3) and past the 5 and 8 that a
    // 5-byte allowance would give.
    let flushed_empty = flushed(b"");
    let flushed_one = flushed(b"a");
    assert_eq!(flushed_empty, [0x00, 0x00, 0x00, 0xFF, 0xFF, 0x03, 0x00]);
    assert_eq!(flushed_one.len(), 9);
    assert!(cdoc_m1_3_support::inflate(&flushed_empty).is_empty());
    assert_eq!(cdoc_m1_3_support::inflate(&flushed_one), b"a");
    let huffman_zeros = vec![0u8; 1 + 258 * 10];
    let defects = package_with(vec![
        RawEntry::deflated(
            "xl/media/at-bound.bin",
            &padded_stream(205, at_bound),
            33,
            cdoc_m1_1_support::crc32(at_bound),
        ),
        RawEntry::deflated(
            "xl/media/past-bound.bin",
            &padded_stream(206, at_bound),
            33,
            cdoc_m1_1_support::crc32(at_bound),
        ),
        RawEntry::deflated(
            "xl/media/huffman.bin",
            &fixed_zeros(10),
            huffman_zeros.len() as u32,
            cdoc_m1_1_support::crc32(&huffman_zeros),
        ),
        RawEntry::deflated("xl/media/empty.bin", &empty_deflate, 0, 0),
        RawEntry::deflated("xl/media/flushed-empty.bin", &flushed_empty, 0, 0),
        RawEntry::deflated(
            "xl/media/flushed-one.bin",
            &flushed_one,
            1,
            cdoc_m1_1_support::crc32(b"a"),
        ),
        RawEntry::deflated(
            "xl/media/trailing.bin",
            &[stored_block(b"content"), vec![0x00]].concat(),
            7,
            cdoc_m1_1_support::crc32(b"content"),
        ),
        RawEntry::deflated("xl/media/trailing-bits.bin", &[0x03, 0x00, 0x00], 0, 0),
        RawEntry::deflated(
            "xl/media/huffman-trailing.bin",
            &[fixed_zeros(10), vec![0xAB]].concat(),
            huffman_zeros.len() as u32,
            cdoc_m1_1_support::crc32(&huffman_zeros),
        ),
        RawEntry::deflated("xl/media/short.bin", &empty_deflate, 10, 0),
        RawEntry::deflated("xl/media/reserved.bin", &[0x07], 1, 0),
        RawEntry::deflated("xl/media/truncated.bin", &[0x02], 1, 0),
        RawEntry {
            crc32: 0,
            ..RawEntry::stored("xl/media/crc.bin", b"content")
        },
        RawEntry::deflated("xl/media/deflated-crc.bin", &stored_block(b"content"), 7, 0),
        RawEntry::deflated(
            "xl/media/deflated.bin",
            &stored_block(b"content"),
            7,
            cdoc_m1_1_support::crc32(b"content"),
        ),
        RawEntry::deflated(
            "xl/media/one-past.bin",
            &stored_block(b"content"),
            6,
            cdoc_m1_1_support::crc32(b"conten"),
        ),
    ]);
    let document = opened(memory(&defects), &default);
    let package = document.package();
    assert_eq!(
        package.read_part("xl/media/deflated.bin"),
        Ok(b"content".to_vec()),
        "control: a deflated payload of exactly its declared size and CRC reads"
    );
    assert_eq!(
        package.read_part("xl/media/deflated-crc.bin"),
        Err(PartReadError::CrcMismatch {
            declared: 0,
            actual: cdoc_m1_1_support::crc32(b"content")
        }),
        "a deflated payload of its declared size with the wrong CRC"
    );
    assert_eq!(
        package.read_part("xl/media/one-past.bin"),
        Err(PartReadError::ExpandsPastDeclaredSize { declared: 6 }),
        "a deflated payload one byte past its declared size"
    );
    assert_eq!(
        package.read_part("xl/media/short.bin"),
        Err(PartReadError::ShorterThanDeclared {
            declared: 10,
            actual: 0
        })
    );
    assert_eq!(
        package.read_part("xl/media/reserved.bin"),
        Err(PartReadError::CorruptDeflate {
            detail: "the stream is not valid DEFLATE"
        }),
        "a final block of type 3, which DEFLATE reserves"
    );
    assert_eq!(
        package.read_part("xl/media/truncated.bin"),
        Err(PartReadError::CorruptDeflate {
            detail: "the stream ends before its final block does"
        }),
        "a non-final fixed block cut off five bits in"
    );
    assert_eq!(
        package.read_part("xl/media/crc.bin"),
        Err(PartReadError::CrcMismatch {
            declared: 0,
            actual: cdoc_m1_1_support::crc32(b"content")
        })
    );
    assert_eq!(
        package.read_part("xl/missing.xml"),
        Err(PartReadError::NotAPart {
            name: "xl/missing.xml".to_owned()
        })
    );
    assert_eq!(
        package.read_part("xl/media/at-bound.bin"),
        Ok(at_bound.to_vec()),
        "control: a payload of exactly the compressed size its declared size allows reads"
    );
    assert_eq!(
        package.read_part("xl/media/past-bound.bin"),
        Err(PartReadError::CompressedPayloadTooLarge {
            compressed: 1068,
            declared: 33,
            bound: 1063,
        }),
        "one empty stored block more"
    );
    assert_eq!(
        package.read_part("xl/media/flushed-empty.bin"),
        Ok(Vec::new()),
        "an empty entry ended with a sync flush and an empty final block"
    );
    assert_eq!(
        package.read_part("xl/media/flushed-one.bin"),
        Ok(b"a".to_vec()),
        "a one-byte entry ended with a sync flush and an empty final block"
    );
    assert_eq!(
        package.read_part("xl/media/huffman.bin"),
        Ok(huffman_zeros.clone()),
        "control: a fixed Huffman block of its declared size and CRC reads"
    );
    assert_eq!(
        package.read_part("xl/media/empty.bin"),
        Ok(Vec::new()),
        "control: an empty fixed block, whose 10 bits end in its second byte, reads"
    );
    assert_eq!(
        package.read_part("xl/media/trailing.bin"),
        Err(PartReadError::BytesAfterFinalBlock {
            compressed: 13,
            consumed: 12,
        }),
        "a final stored block of the declared content and CRC, then one byte"
    );
    assert_eq!(
        package.read_part("xl/media/trailing-bits.bin"),
        Err(PartReadError::BytesAfterFinalBlock {
            compressed: 3,
            consumed: 2,
        }),
        "an empty fixed block, then one byte after the byte its last bit ends in"
    );
    assert_eq!(
        package.read_part("xl/media/huffman-trailing.bin"),
        Err(PartReadError::BytesAfterFinalBlock {
            compressed: 20,
            consumed: 19,
        }),
        "a fixed Huffman block of 148 bits of the declared size and CRC, then one byte"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: deflated payloads read through the output-limited decoder: one of exactly its declared 7 bytes and CRC, a fixed Huffman block of its declared 2581 zeros, an empty block, an empty and a one-byte entry each ended with a sync-flush marker and an empty final block (7 and 9 bytes) and a padded payload of exactly the 1063 bytes 33 declared bytes allow read back; refused were one past its declared 6 bytes (ExpandsPastDeclaredSize), one of 0 bytes declaring 10 (ShorterThanDeclared), one with the wrong CRC (CrcMismatch), a reserved block type and a cut-off block (CorruptDeflate), 1068 bytes where 33 declared bytes allow 1063 (CompressedPayloadTooLarge), and a stored block, an empty fixed block and a fixed Huffman block each of its declared size and CRC with one byte after its final block (BytesAfterFinalBlock, 13 bytes consumed 12, 3 consumed 2, 20 consumed 19); a stored payload with the wrong CRC (CrcMismatch)"
    );
    let mut wrong = base_entries();
    let actual = wrong[0].crc32;
    wrong[0].crc32 ^= 1;
    expect(
        "[Content_Types].xml with the wrong CRC",
        refused(memory(&build_zip(&wrong)), &default),
        OpenError::Part {
            part: "[Content_Types].xml".to_owned(),
            error: PartReadError::CrcMismatch {
                declared: actual ^ 1,
                actual,
            },
        },
    );

    // Entity expansion: a document type declaration is refused before any entity it declares.
    let ct = calc_doc::CONTENT_TYPES_NAMESPACE;
    let rels = calc_doc::RELATIONSHIPS_NAMESPACE;
    let external_types = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE Types [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><Types xmlns="{ct}"><Default Extension="xml" ContentType="&xxe;"/></Types>"#
    );
    expect(
        "[Content_Types].xml declaring an external entity",
        refused(
            memory(&package_replacing(
                "[Content_Types].xml",
                external_types.as_bytes(),
            )),
            &default,
        ),
        OpenError::Xml {
            part: "[Content_Types].xml".to_owned(),
            error: XmlError::DocumentTypeDeclaration,
        },
    );
    let mut laughs = String::from(r#"<?xml version="1.0"?><!DOCTYPE lolz [<!ENTITY lol0 "lol">"#);
    for level in 1..10 {
        laughs.push_str(&format!(
            "<!ENTITY lol{level} \"{}\">",
            format!("&lol{};", level - 1).repeat(10)
        ));
    }
    laughs.push_str(&format!(
        r#"]><Relationships xmlns="{rels}">&lol9;</Relationships>"#
    ));
    expect(
        "_rels/.rels expanding an entity to 10^9 copies",
        refused(
            memory(&package_replacing("_rels/.rels", laughs.as_bytes())),
            &default,
        ),
        OpenError::Xml {
            part: "_rels/.rels".to_owned(),
            error: XmlError::DocumentTypeDeclaration,
        },
    );
    let lazy_cases: Vec<(&str, String, XmlLimits, XmlError)> = vec![
        (
            "an external entity in a workbook",
            format!(
                r#"<?xml version="1.0"?><!DOCTYPE workbook [<!ENTITY xxe SYSTEM "http://example.invalid/xxe">]><workbook xmlns="{SPREADSHEET_MAIN}">&xxe;</workbook>"#
            ),
            XmlLimits::default(),
            XmlError::DocumentTypeDeclaration,
        ),
        (
            "an unterminated document type declaration",
            r#"<?xml version="1.0"?><!DOCTYPE workbook [<!ENTITY xxe SYSTEM "http://example.invalid/xxe">"#
                .to_owned(),
            XmlLimits::default(),
            XmlError::DocumentTypeDeclaration,
        ),
        (
            "an undeclared entity in text",
            format!(r#"<workbook xmlns="{SPREADSHEET_MAIN}">&xxe;</workbook>"#),
            XmlLimits::default(),
            XmlError::UndefinedEntity {
                name: "xxe".to_owned(),
            },
        ),
        (
            "an undeclared entity in an attribute",
            format!(r#"<workbook xmlns="{SPREADSHEET_MAIN}" a="&xxe;"/>"#),
            XmlLimits::default(),
            XmlError::UndefinedEntity {
                name: "xxe".to_owned(),
            },
        ),
        (
            "five nested elements under a depth limit of four",
            format!(r#"<workbook xmlns="{SPREADSHEET_MAIN}"><a><b><c><d/></c></b></a></workbook>"#),
            XmlLimits {
                max_depth: 4,
                ..XmlLimits::default()
            },
            XmlError::DepthLimit { limit: 4 },
        ),
        (
            "four attributes under a limit of three",
            format!(r#"<workbook xmlns="{SPREADSHEET_MAIN}" a="1" b="2" c="3"/>"#),
            XmlLimits {
                max_attributes: 3,
                ..XmlLimits::default()
            },
            XmlError::AttributeLimit { limit: 3 },
        ),
        (
            "an undeclared prefix",
            "<x:workbook/>".to_owned(),
            XmlLimits::default(),
            XmlError::UnboundPrefix {
                prefix: "x".to_owned(),
            },
        ),
        (
            "a declared Latin-1 encoding",
            format!(
                r#"<?xml version="1.0" encoding="ISO-8859-1"?><workbook xmlns="{SPREADSHEET_MAIN}"/>"#
            ),
            XmlLimits::default(),
            XmlError::UnsupportedEncoding {
                encoding: "ISO-8859-1".to_owned(),
            },
        ),
    ];
    let mut lazy_refusals = Vec::new();
    for (label, workbook, limits, expected) in lazy_cases {
        let policy = OpenPolicy {
            xml: limits,
            ..OpenPolicy::default()
        };
        let document = opened(
            memory(&package_replacing("xl/workbook.xml", workbook.as_bytes())),
            &policy,
        );
        let part = document
            .package()
            .read_xml("xl/workbook.xml")
            .expect("the part decompresses");
        let mut reader = part.reader();
        let error = loop {
            match reader.next_event() {
                Ok(Some(_)) => {}
                Ok(None) => panic!("{label}: the reader refuses the part"),
                Err(error) => break error,
            }
        };
        assert_eq!(error, expected, "{label}");
        assert_eq!(
            reader.next_event(),
            Err(expected),
            "{label}: the refusal is repeated"
        );
        lazy_refusals.push(format!("{label}: {error}"));
    }
    let mut utf16 = vec![0xFF, 0xFE];
    for unit in format!(r#"<workbook xmlns="{SPREADSHEET_MAIN}"/>"#).encode_utf16() {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    let document = opened(
        memory(&package_replacing("xl/workbook.xml", &utf16)),
        &default,
    );
    assert_eq!(
        document
            .package()
            .read_xml("xl/workbook.xml")
            .unwrap()
            .reader()
            .next_event(),
        Err(XmlError::UnsupportedEncoding {
            encoding: "UTF-16".to_owned()
        })
    );
    lazy_refusals.push("a UTF-16 workbook: UnsupportedEncoding".to_owned());
    let document = opened(
        memory(&package_replacing(
            "xl/workbook.xml",
            format!(r#"<workbook xmlns="{SPREADSHEET_MAIN}"></sheets>"#).as_bytes(),
        )),
        &default,
    );
    let part = document.package().read_xml("xl/workbook.xml").unwrap();
    let mut reader = part.reader();
    assert!(matches!(reader.next_event(), Ok(Some(XmlEvent::Start(_)))));
    assert!(
        matches!(reader.next_event(), Err(XmlError::Malformed { .. })),
        "a mismatched end tag"
    );
    lazy_refusals.push("a mismatched end tag: Malformed".to_owned());
    // Control: the same limits admit a document within them.
    let within = OpenPolicy {
        xml: XmlLimits {
            max_depth: 4,
            max_attributes: 3,
        },
        ..OpenPolicy::default()
    };
    let document = opened(
        memory(&package_replacing(
            "xl/workbook.xml",
            format!(
                r#"<workbook xmlns="{SPREADSHEET_MAIN}" a="&amp;&#65;"><a><b><c/></b></a></workbook>"#
            )
            .as_bytes(),
        )),
        &within,
    );
    let part = document.package().read_xml("xl/workbook.xml").unwrap();
    let mut reader = part.reader();
    let Ok(Some(XmlEvent::Start(root))) = reader.next_event() else {
        panic!("the document element");
    };
    assert_eq!(
        root.attribute("a"),
        Some("&A"),
        "predefined and character references are replaced"
    );
    let mut elements = 1;
    while let Some(event) = reader.next_event().expect("within the limits") {
        if let XmlEvent::Start(_) = event {
            elements += 1;
        }
    }
    assert_eq!(elements, 4);

    // Archive structure.
    expect(
        "a text file",
        refused(memory(b"this is a text file, not a ZIP archive"), &default),
        OpenError::NotAZipArchive,
    );
    expect(
        "an empty source",
        refused(memory(&[]), &default),
        OpenError::NotAZipArchive,
    );
    let mut zip64 = base_bytes.clone();
    let end = end_offset(&zip64);
    zip64[end + 8..end + 12].copy_from_slice(&[0xFF; 4]);
    expect(
        "an end record whose entry counts are ZIP64's",
        refused(memory(&zip64), &default),
        OpenError::Zip64Archive,
    );
    let mut located = base_bytes.clone();
    let end = end_offset(&located);
    let mut locator = vec![0x50, 0x4b, 0x06, 0x07];
    locator.extend_from_slice(&[0; 16]);
    located.splice(end..end, locator);
    expect(
        "a ZIP64 locator before the end record",
        refused(memory(&located), &default),
        OpenError::Zip64Archive,
    );
    let mut spanned = base_bytes.clone();
    let end = end_offset(&spanned);
    spanned[end + 4] = 1;
    expect(
        "an end record on disk 1",
        refused(memory(&spanned), &default),
        OpenError::SpannedArchive,
    );
    let mut shifted = base_bytes.clone();
    let end = end_offset(&shifted);
    shifted[end + 16] = shifted[end + 16].wrapping_add(1);
    expect(
        "a central directory offset one byte off",
        refused(memory(&shifted), &default),
        OpenError::MalformedArchive {
            offset: end as u64,
            detail: "the central directory does not end where the end record starts",
        },
    );
    let mut renamed = base_bytes.clone();
    renamed[30] = b'{';
    expect(
        "a local header naming another entry",
        refused(memory(&renamed), &default),
        OpenError::MalformedArchive {
            offset: 0,
            detail: "a local header's name differs from its central record's",
        },
    );
    expect(
        "an encrypted entry",
        refused(
            memory(&package_with(vec![RawEntry {
                flags: 1,
                ..RawEntry::stored("xl/media/secret.bin", b"secret")
            }])),
            &default,
        ),
        OpenError::EncryptedEntry {
            name: "xl/media/secret.bin".to_owned(),
        },
    );
    expect(
        "a bzip2 entry",
        refused(
            memory(&package_with(vec![RawEntry {
                method: 12,
                ..RawEntry::stored("xl/media/b.bz2", b"BZh")
            }])),
            &default,
        ),
        OpenError::UnsupportedCompression {
            name: "xl/media/b.bz2".to_owned(),
            method: 12,
        },
    );
    expect(
        "two names equal ignoring case",
        refused(
            memory(&package_with(vec![
                RawEntry::stored("xl/media/a.bin", b"a"),
                RawEntry::stored("XL/MEDIA/A.BIN", b"b"),
            ])),
            &default,
        ),
        OpenError::DuplicateEntryName {
            name: "XL/MEDIA/A.BIN".to_owned(),
        },
    );
    expect(
        "a package without [Content_Types].xml",
        refused(memory(&build_zip(&base_entries()[1..])), &default),
        OpenError::MissingContentTypes,
    );
    expect(
        "[Content_Types].xml whose document element is not Types",
        refused(
            memory(&package_replacing(
                "[Content_Types].xml",
                format!(r#"<Typez xmlns="{ct}"/>"#).as_bytes(),
            )),
            &default,
        ),
        OpenError::InvalidContentTypes {
            detail: "the document element is not Types",
        },
    );
    expect(
        "a relationship whose TargetMode is neither Internal nor External",
        refused(
            memory(&package_replacing(
                "_rels/.rels",
                format!(
                    r#"<Relationships xmlns="{rels}"><Relationship Id="rId1" Type="{OFFICE_DOCUMENT}" Target="xl/workbook.xml" TargetMode="Sideways"/></Relationships>"#
                )
                .as_bytes(),
            )),
            &default,
        ),
        OpenError::InvalidRelationships {
            part: "_rels/.rels".to_owned(),
            detail: "a TargetMode other than Internal or External",
        },
    );
    expect(
        "a source that fails every read",
        refused(Arc::new(FailingSource { size: 1000 }), &default),
        OpenError::Source(SourceError::new("the medium is unreadable")),
    );

    // Sources that fail at one place only. A 200 000-byte first entry puts its local header and
    // most of its payload before the last 65 557 bytes, which the end record search reads.
    let filler_first = {
        let mut entries = vec![RawEntry::stored(
            "xl/media/filler.bin",
            &vec![0x5Au8; 200_000],
        )];
        entries.extend(base_entries());
        build_zip(&entries)
    };
    let filler_zip = parse_zip(&filler_first);
    assert_eq!(filler_zip.entry("xl/media/filler.bin").local_offset, 0);
    let tail = (
        (filler_first.len() - 65_557) as u64,
        filler_first.len() as u64,
    );
    let central = (
        filler_zip.central_offset as u64,
        (filler_zip.central_offset + filler_zip.central_size) as u64,
    );
    let local_headers: Vec<(u64, u64)> = filler_zip
        .entries
        .iter()
        .map(|entry| {
            (
                u64::from(entry.local_offset),
                u64::from(entry.local_offset) + 30,
            )
        })
        .collect();
    let mut source_failures = Vec::new();
    // (label, unreadable bytes, the failed read, the local headers read before it): the first
    // is the first entry's local header, read right after the central directory; the second is
    // the second 64 KiB chunk of the first entry's stored bytes, read after every local header,
    // in the pass that records each entry's CRC-32 of its stored bytes.
    for (label, unreadable, failed_read, headers_before) in [
        (
            "a source whose first entry's local header is unreadable",
            0..30u64,
            (0, 30),
            0,
        ),
        (
            "a source with one unreadable byte inside the first entry's payload",
            100_000..100_001u64,
            (65_536, 131_072),
            local_headers.len(),
        ),
    ] {
        let unreadable_source = Arc::new(UnreadableRangeSource::new(
            filler_first.clone(),
            unreadable.clone(),
        ));
        let source: Arc<dyn PackageRead> = unreadable_source.clone();
        expect(
            label,
            refused(source, &default),
            OpenError::Source(UnreadableRangeSource::error(&unreadable)),
        );
        let reads = unreadable_source.reads();
        let (last, earlier) = reads.split_last().expect("reads");
        assert_eq!(
            (last.0, last.1, last.2),
            (failed_read.0, failed_read.1, true),
            "{label}: open stopped at the failed read"
        );
        let earlier: Vec<(u64, u64)> = earlier
            .iter()
            .map(|read| {
                assert!(!read.2, "{label}: only the last read failed");
                (read.0, read.1)
            })
            .collect();
        assert_eq!(
            earlier[..2],
            [tail, central],
            "{label}: the end record search, then the central directory"
        );
        assert_eq!(
            local_headers
                .iter()
                .filter(|header| earlier.contains(header))
                .count(),
            headers_before,
            "{label}: the local headers read before the failure"
        );
        source_failures.push(format!(
            "{label}: refused with Source at bytes {}..{}, after {} successful reads ({headers_before} of {} local headers)",
            failed_read.0,
            failed_read.1,
            earlier.len(),
            local_headers.len()
        ));
    }
    let changing_source = Arc::new(UnreadableRangeSource::new(base_bytes.clone(), 0..0));
    let source: Arc<dyn PackageRead> = changing_source.clone();
    let document = opened(source, &default);
    let workbook = parse_zip(&base_bytes).entry("xl/workbook.xml").clone();
    let payload = workbook.payload_start as u64..workbook.payload_end as u64;
    changing_source.set_unreadable(payload.clone());
    assert_eq!(
        document.package().read_part("xl/workbook.xml"),
        Err(PartReadError::Source(UnreadableRangeSource::error(
            &payload
        ))),
        "a part whose payload became unreadable after open"
    );
    assert_eq!(
        document.package().read_part("_rels/.rels"),
        Ok(base_relationships().into_bytes()),
        "a part outside the unreadable bytes still reads"
    );
    source_failures.push(format!(
        "xl/workbook.xml read after its payload bytes {}..{} became unreadable: refused with Source; _rels/.rels still read",
        payload.start, payload.end
    ));

    let listing_after = file_system_listing();
    assert_eq!(
        listing_after, listing_before,
        "the files in the working directory and its parent are unchanged"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: {} packages refused at open with the expected typed error: {:?}",
        refusals.len(),
        refusals
    );
    note!(
        "ACCEPT-NOTE: {CASE}: the 4 GiB declared-size bomb was refused after {} reads of {read_bytes} bytes of a {}-byte archive, none reaching its entry's bytes {}..{}",
        reads.len(),
        bomb_bytes.len(),
        bomb_entry.local_offset,
        bomb_entry.end()
    );
    note!(
        "ACCEPT-NOTE: {CASE}: lying-size.xlsx opened; reading xl/media/zeros.bin (declared {declared_size} bytes, a {}-byte payload that expands to {expanded_size}) refused with CompressedPayloadTooLarge (bound 5696) after this thread allocated {lying_allocated} bytes and with no read of its payload; a no-op save kept its payload's SHA-256",
        lying_entry.compressed_size
    );
    note!(
        "ACCEPT-NOTE: {CASE}: an entry with an oversized compressed payload (valid DEFLATE of its declared 7 bytes and CRC padded to 4000012 bytes, bound 1033) refused when read with CompressedPayloadTooLarge after this thread allocated {padded_allocated} bytes, {} reads of the source, none of the payload; the same padding on _rels/.rels ({} bytes) refused at open with Part(CompressedPayloadTooLarge) after this thread allocated {rels_allocated} bytes",
        padded_reads.len(),
        rels_stream.len()
    );
    note!(
        "ACCEPT-NOTE: {CASE}: a fixed Huffman payload of 73128 bytes (within the 75776 its declared 65536 bytes allow) that expands to {} bytes refused with ExpandsPastDeclaredSize after this thread allocated {zeros_allocated} bytes; a no-op save kept its payload's SHA-256",
        1 + 258 * 45_000
    );
    note!(
        "ACCEPT-NOTE: {CASE}: {} parts opened, then refused when read: {:?}",
        lazy_refusals.len(),
        lazy_refusals
    );
    note!(
        "ACCEPT-NOTE: {CASE}: sources that fail at one place only, each read recorded by the source: {:?}",
        source_failures
    );
    #[cfg(not(target_arch = "wasm32"))]
    note!(
        "ACCEPT-NOTE: {CASE}: the {} entries of . and .. are unchanged after every refusal, and none of the {} planted paths exists",
        listing_after.len(),
        planted_targets.len()
    );
    #[cfg(target_arch = "wasm32")]
    note!(
        "ACCEPT-NOTE: {CASE}: the browser gives the case no file system to inspect; calc-doc's non-test code names no std::fs (the os-imports architecture check)"
    );
}

// ---------------------------------------------------------------------------------------------
// cdoc-m1.2: a workbook's cells, formulas and file caches loaded into the model.

use calc_doc::model::CachedValue;
use calc_doc::{CellError, DateSystem, FORMULA_MAX_UTF16_CODE_UNITS, LoadError, LoadedWorkbook};

const M1_2_CELLS: &[u8] = include_bytes!("fixtures/cdoc_m1_2/cells.xlsx");
const M1_2_EXTENT: &[u8] = include_bytes!("fixtures/cdoc_m1_2/extent.xlsx");
const M1_2_CORNER: &[u8] = include_bytes!("fixtures/cdoc_m1_2/corner.xlsx");
const M1_2_MANIFEST: &str = include_str!("fixtures/cdoc_m1_2/manifest.json");

const RELATIONSHIP_ATTRIBUTES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const WORKSHEET_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";

/// The worksheet errors the fixtures write, each with the spelling a cell's `<v>` gives it.
const ERROR_SPELLINGS: [(WorksheetErrorCode, &str); 7] = [
    (WorksheetErrorCode::Null, "#NULL!"),
    (WorksheetErrorCode::Div0, "#DIV/0!"),
    (WorksheetErrorCode::Value, "#VALUE!"),
    (WorksheetErrorCode::Ref, "#REF!"),
    (WorksheetErrorCode::Name, "#NAME?"),
    (WorksheetErrorCode::Num, "#NUM!"),
    (WorksheetErrorCode::NA, "#N/A"),
];

/// What a cell or a cached result holds, in the terms the generator's manifest records it.
#[derive(Clone, Debug, PartialEq)]
enum Held {
    Number(u64),
    Text(Vec<u16>),
    Logical(bool),
    Error(String),
    Formula(Vec<u16>),
}

/// The code units of the manifest's upper-case hexadecimal, four digits each.
fn units_of_hex(hex: &str) -> Vec<u16> {
    assert_eq!(hex.len() % 4, 0, "{hex}");
    (0..hex.len())
        .step_by(4)
        .map(|at| u16::from_str_radix(&hex[at..at + 4], 16).expect("hexadecimal"))
        .collect()
}

/// A value as the manifest records it.
fn recorded(value: &Value) -> Held {
    let field = |key: &str| value[key].as_str().expect(key);
    match field("kind") {
        "number" => Held::Number(u64::from_str_radix(field("bits"), 16).expect("bits")),
        "text" => Held::Text(units_of_hex(field("units"))),
        "logical" => Held::Logical(value["value"].as_bool().expect("value")),
        "error" => Held::Error(field("spelling").to_owned()),
        "formula" => Held::Formula(units(field("text"))),
        other => panic!("the manifest records a value of kind {other}"),
    }
}

fn spelling(code: WorksheetErrorCode) -> String {
    let (_, spelling) = ERROR_SPELLINGS
        .iter()
        .find(|(known, _)| *known == code)
        .unwrap_or_else(|| panic!("{code:?} is not an error the fixtures write"));
    (*spelling).to_owned()
}

fn input_held(input: &CellInput) -> Held {
    match input {
        CellInput::Number(number) => Held::Number(number.to_bits()),
        CellInput::Text(text) => Held::Text(text.text().utf16_code_units().to_vec()),
        CellInput::Logical(logical) => Held::Logical(*logical),
        CellInput::Error(code) => Held::Error(spelling(*code)),
        CellInput::Formula(formula) => Held::Formula(formula.text().utf16_code_units().to_vec()),
    }
}

/// A held value as a note shows it: a number as its value and bits, a text as a string.
fn shown(held: &Held) -> String {
    match held {
        Held::Number(bits) => format!("{} (bits {bits:016X})", f64::from_bits(*bits)),
        Held::Text(units) => format!("text {:?}", String::from_utf16_lossy(units)),
        Held::Logical(logical) => (if *logical { "TRUE" } else { "FALSE" }).to_owned(),
        Held::Error(spelling) => spelling.clone(),
        Held::Formula(units) => format!("formula {:?}", String::from_utf16_lossy(units)),
    }
}

fn cached_held(value: &CachedValue) -> Held {
    match value {
        CachedValue::Number(number) => Held::Number(number.to_bits()),
        CachedValue::Text(text) => Held::Text(text.text().utf16_code_units().to_vec()),
        CachedValue::Logical(logical) => Held::Logical(*logical),
        CachedValue::Error(code) => Held::Error(spelling(*code)),
    }
}

/// The (row, column) of an A1 address the manifest records, read here.
fn a1(text: &str) -> (u32, u32) {
    let digits = text.find(|c: char| c.is_ascii_digit()).expect("a row");
    let column = text[..digits].bytes().fold(0u32, |column, letter| {
        column * 26 + u32::from(letter - b'A') + 1
    });
    (text[digits..].parse().expect("a row number"), column)
}

/// The package the manifest records under `file`.
fn m1_2_record<'a>(manifest: &'a Value, file: &str) -> &'a Value {
    manifest["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .find(|package| package["file"] == file)
        .unwrap_or_else(|| panic!("the manifest records {file}"))
}

/// Opens `bytes` and loads its workbook with sheet IDs from `ids`.
fn m1_2_load(bytes: &[u8], ids: &mut IdAllocator<SheetId>) -> Result<LoadedWorkbook, LoadError> {
    let document = opened(memory(bytes), &OpenPolicy::default());
    calc_doc::load(&document, ids)
}

/// The input of the cell at `row` and `column` of a loaded workbook's first sheet.
fn first_sheet_cell(loaded: &LoadedWorkbook, row: u32, column: u32) -> Option<Held> {
    let sheet = loaded.model().sheets()[0].id();
    loaded
        .model()
        .cell(cell(sheet, row, column))
        .map(input_held)
}

/// Checks a loaded workbook's sheets and every one of their cells against the generator's record
/// of the package: the model holds exactly the recorded cells with the recorded inputs, the
/// records beside it hold exactly the recorded styles and formulas, and each formula's cache is
/// the recorded one, FileCached with the identity of the sheet's part. Returns, for each cache
/// written deliberately wrong, a description of it.
fn check_loaded(loaded: &LoadedWorkbook, record: &Value) -> Vec<String> {
    let model = loaded.model();
    assert_eq!(model.revision(), AuthoredRevision::INITIAL);
    assert_eq!(loaded.workbook_part(), record["workbook_part"]);
    assert_eq!(
        u64::from(loaded.cell_formats()),
        record["cell_formats"].as_u64().unwrap()
    );
    let date_system = match record["date_system"].as_u64().expect("date_system") {
        1900 => DateSystem::Date1900,
        1904 => DateSystem::Date1904,
        other => panic!("the manifest records the date system {other}"),
    };
    assert_eq!(
        loaded.date_system(),
        date_system,
        "the date system the workbook part names"
    );
    assert_eq!(
        loaded.defined_names_not_loaded() as u64,
        record["defined_names"].as_u64().expect("defined_names"),
        "the defined names the workbook part holds, counted and not loaded"
    );
    let sheets = record["sheets"].as_array().expect("sheets");
    assert_eq!(model.sheets().len(), sheets.len(), "the model's sheets");
    assert_eq!(loaded.sheets().len(), sheets.len(), "the records' sheets");
    let mut wrong_caches = Vec::new();
    for ((entry, sheet), recorded_sheet) in model.sheets().iter().zip(loaded.sheets()).zip(sheets) {
        let field = |key: &str| recorded_sheet[key].as_str().expect(key);
        let name = field("name");
        assert_eq!(entry.id(), sheet.id(), "{name}: workbook order");
        assert_eq!(
            entry.name().text().utf16_code_units(),
            units_of_hex(field("name_units")).as_slice()
        );
        assert_eq!(
            u64::from(entry.key().get()),
            recorded_sheet["sheet_id"].as_u64().unwrap(),
            "{name}: its sheetId is its key"
        );
        let visibility = match field("state") {
            "visible" => SheetVisibility::Visible,
            "hidden" => SheetVisibility::Hidden,
            other => panic!("the manifest records the state {other}"),
        };
        assert_eq!(entry.visibility(), visibility, "{name}");
        assert_eq!(sheet.relationship(), field("relationship"), "{name}");
        let part = sheet.part();
        assert_eq!(
            (part.name(), u64::from(part.crc32()), part.size()),
            (
                field("part"),
                recorded_sheet["part_crc32"].as_u64().unwrap(),
                recorded_sheet["part_size"].as_u64().unwrap()
            ),
            "{name}: the part's name, and the CRC-32 and size the generator computed of its content"
        );

        // The model holds exactly the recorded cells, in row-major order, with their inputs.
        let cells = recorded_sheet["cells"].as_array().expect("cells");
        let expected: Vec<(u32, u32, Held)> = cells
            .iter()
            .map(|cell| {
                (
                    cell["row"].as_u64().unwrap() as u32,
                    cell["column"].as_u64().unwrap() as u32,
                    recorded(&cell["input"]),
                )
            })
            .collect();
        let held: Vec<(u32, u32, Held)> = model
            .sheet_cells(entry.id())
            .expect("the sheet's cells")
            .map(|(at, input)| (at.row(), at.column(), input_held(input)))
            .collect();
        assert_eq!(
            held, expected,
            "{name}: the occupied cells and their inputs"
        );
        for empty in recorded_sheet["styled_empty"].as_array().unwrap() {
            let (row, column) = (
                empty["row"].as_u64().unwrap() as u32,
                empty["column"].as_u64().unwrap() as u32,
            );
            assert_eq!(model.cell(cell(entry.id(), row, column)), None);
            assert_eq!(
                u64::from(sheet.style(row, column)),
                empty["style"].as_u64().unwrap(),
                "{name}: a styled cell without a value keeps its style beside the model"
            );
        }
        for empty in recorded_sheet["unmaterialized"].as_array().unwrap() {
            let (row, column) = a1(empty.as_str().unwrap());
            assert_eq!(model.cell(cell(entry.id(), row, column)), None);
            assert_eq!(sheet.style(row, column), 0);
        }

        // Styles: every cell written with a style index other than 0, with or without a value.
        let mut styles: Vec<(u32, u32, u32)> = cells
            .iter()
            .chain(recorded_sheet["styled_empty"].as_array().unwrap())
            .map(|cell| {
                (
                    cell["row"].as_u64().unwrap() as u32,
                    cell["column"].as_u64().unwrap() as u32,
                    cell["style"].as_u64().unwrap() as u32,
                )
            })
            .filter(|&(_, _, style)| style != 0)
            .collect();
        styles.sort();
        assert_eq!(
            sheet.styled_cells().collect::<Vec<_>>(),
            styles,
            "{name}: the styled cells"
        );

        // Formulas: the shared formula group and the cache of each, beside its input.
        let formulas: Vec<&Value> = cells
            .iter()
            .filter(|cell| cell["input"]["kind"] == "formula")
            .collect();
        assert_eq!(
            sheet
                .formulas()
                .map(|(row, column, _)| (row, column))
                .collect::<Vec<_>>(),
            formulas
                .iter()
                .map(|cell| (
                    cell["row"].as_u64().unwrap() as u32,
                    cell["column"].as_u64().unwrap() as u32
                ))
                .collect::<Vec<_>>(),
            "{name}: a formula record for each formula cell and no other"
        );
        for recorded_cell in formulas {
            let reference = recorded_cell["ref"].as_str().unwrap();
            let (row, column) = a1(reference);
            let formula = sheet.formula(row, column).expect("a formula record");
            match (formula.shared(), &recorded_cell["shared"]) {
                (None, Value::Null) => {}
                (Some(group), recorded_group) if !recorded_group.is_null() => {
                    let master = a1(recorded_group["master"].as_str().unwrap());
                    let area = recorded_group["ref"].as_str().unwrap();
                    let (first, last) = area.split_once(':').expect("an area");
                    let ((top, left), (bottom, right)) = (a1(first), a1(last));
                    assert_eq!(
                        u64::from(group.index()),
                        recorded_group["si"].as_u64().unwrap(),
                        "{name}!{reference}"
                    );
                    assert_eq!(group.master(), cell(entry.id(), master.0, master.1));
                    assert_eq!(
                        group.area(),
                        GridArea::new(cell(entry.id(), top, left), cell(entry.id(), bottom, right))
                            .unwrap()
                    );
                }
                (got, expected) => {
                    panic!("{name}!{reference}: group {got:?}, recorded {expected}")
                }
            }
            match (formula.cache(), &recorded_cell["cache"]) {
                (None, Value::Null) => {}
                (Some(cache), recorded_cache) if !recorded_cache.is_null() => {
                    assert_eq!(
                        cached_held(cache.value()),
                        recorded(recorded_cache),
                        "{name}!{reference}: the cache is the value written in <v>"
                    );
                    assert!(
                        Arc::ptr_eq(cache.source(), sheet.part()),
                        "{name}!{reference}: the cache names the sheet's part as its source"
                    );
                    let correct = &recorded_cell["correct"];
                    if !correct.is_null() {
                        assert_ne!(
                            cached_held(cache.value()),
                            recorded(correct),
                            "{name}!{reference}: the fixture's cache is wrong on purpose"
                        );
                        let Some(CellInput::Formula(text)) =
                            model.cell(cell(entry.id(), row, column))
                        else {
                            panic!("{name}!{reference}: the input is the formula, not a value");
                        };
                        wrong_caches.push(format!(
                            "{name}!{reference} input formula {:?}, cache FileCached {} from {} (CRC-32 {:08x}, {} bytes), where the generator states {}",
                            text.text().to_string_lossy(),
                            shown(&cached_held(cache.value())),
                            cache.source().name(),
                            cache.source().crc32(),
                            cache.source().size(),
                            shown(&recorded(correct))
                        ));
                    }
                }
                (got, expected) => {
                    panic!("{name}!{reference}: cache {got:?}, recorded {expected}")
                }
            }
        }
    }
    wrong_caches
}

/// A package of one worksheet, "Long", whose `<sheetData>` holds `sheet_data`.
fn m1_2_one_sheet(sheet_data: &str) -> Vec<u8> {
    m1_2_package("", "", sheet_data)
}

/// A package of one worksheet, "Long", whose `<sheetData>` holds `sheet_data`, and whose workbook
/// part holds `before` before its `<sheets>` and `after` after them.
fn m1_2_package(before: &str, after: &str, sheet_data: &str) -> Vec<u8> {
    let content_types = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="{}"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
        calc_doc::CONTENT_TYPES_NAMESPACE
    );
    let workbook = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="{SPREADSHEET_MAIN}" xmlns:r="{RELATIONSHIP_ATTRIBUTES}">{before}<sheets><sheet name="Long" sheetId="1" r:id="rId1"/></sheets>{after}</workbook>"#
    );
    let workbook_relationships = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{}"><Relationship Id="rId1" Type="{WORKSHEET_RELATIONSHIP}" Target="worksheets/sheet1.xml"/></Relationships>"#,
        calc_doc::RELATIONSHIPS_NAMESPACE
    );
    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="{SPREADSHEET_MAIN}"><sheetData>{sheet_data}</sheetData></worksheet>"#
    );
    build_zip(&[
        RawEntry::stored("[Content_Types].xml", content_types.as_bytes()),
        RawEntry::stored("_rels/.rels", base_relationships().as_bytes()),
        RawEntry::stored("xl/workbook.xml", workbook.as_bytes()),
        RawEntry::stored(
            "xl/_rels/workbook.xml.rels",
            workbook_relationships.as_bytes(),
        ),
        RawEntry::stored("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ])
}

// ---------------------------------------------------------------------------------------------
// Case cdoc_m1_2__authored_vs_cache.

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cdoc_m1_2__authored_vs_cache() {
    const CASE: &str = "cdoc_m1_2__authored_vs_cache";
    let manifest: Value = serde_json::from_str(M1_2_MANIFEST).expect("manifest.json");
    let record = m1_2_record(&manifest, "cells.xlsx");
    assert_eq!(M1_2_CELLS.len() as u64, record["length"].as_u64().unwrap());

    // cells.xlsx through open and load, against the generator's record of what it wrote.
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let loaded = match m1_2_load(M1_2_CELLS, &mut ids) {
        Ok(loaded) => loaded,
        Err(error) => panic!("cells.xlsx loads, but: {error}"),
    };
    for sheet in loaded.model().sheets() {
        assert!(
            ids.is_live(sheet.id()),
            "the caller's allocator issued each sheet's ID"
        );
    }
    let wrong_caches = check_loaded(&loaded, record);
    let cell_count: usize = record["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sheet| sheet["cells"].as_array().unwrap().len())
        .sum();
    let formula_count: usize = loaded
        .sheets()
        .iter()
        .map(|sheet| sheet.formulas().count())
        .sum();
    let cached_count = loaded
        .sheets()
        .iter()
        .flat_map(|sheet| sheet.formulas())
        .filter(|(_, _, formula)| formula.cache().is_some())
        .count();
    assert!(!wrong_caches.is_empty(), "the fixture writes wrong caches");

    // The D8:E9 group: each member holds the text the generator states for it, whose ranges end
    // in a function call (INDEX, OFFSET, _xlfn.XLOOKUP) or a sheet-qualified cell. check_loaded
    // compared every cell already; this names the group's cells in the receipt.
    let group_texts: Vec<(String, String)> = record["sheets"][0]["cells"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|cell| cell["shared"]["si"] == 3)
        .map(|cell| {
            (
                cell["ref"].as_str().unwrap().to_owned(),
                cell["input"]["text"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(group_texts.len(), 4, "the generator writes the D8:E9 group");
    for (reference, text) in &group_texts {
        let (row, column) = a1(reference);
        assert_eq!(
            first_sheet_cell(&loaded, row, column),
            Some(Held::Formula(units(text))),
            "Cells!{reference} holds the text the generator states"
        );
    }

    // Excel's formula-length limit, counted on the decoded text and, for a shared formula's
    // member, on the master's text moved to it.
    assert_eq!(FORMULA_MAX_UTF16_CODE_UNITS, 8192);
    let ones = |count: usize| "1".repeat(count);
    let longest = format!("A9+{}", ones(FORMULA_MAX_UTF16_CODE_UNITS - 3));
    let too_long = format!("A9+{}", ones(FORMULA_MAX_UTF16_CODE_UNITS - 2));
    let refusal = |sheet_data: &str, ids: &mut IdAllocator<SheetId>| -> LoadError {
        match m1_2_load(&m1_2_one_sheet(sheet_data), ids) {
            Ok(_) => panic!("the package is refused, but it loaded"),
            Err(error) => error,
        }
    };
    let slots = ids.slot_count();
    assert_eq!(
        refusal(
            &format!(r#"<row r="1"><c r="A1"><f>{too_long}</f></c></row>"#),
            &mut ids
        ),
        LoadError::Cell {
            part: "xl/worksheets/sheet1.xml".to_owned(),
            row: 1,
            column: 1,
            error: CellError::FormulaTooLong {
                units: FORMULA_MAX_UTF16_CODE_UNITS + 1
            }
        },
        "a formula one code unit past the limit"
    );
    assert_eq!(
        refusal(
            &format!(
                r#"<row r="1"><c r="A1"><f t="shared" ref="A1:A2" si="0">{longest}</f></c></row><row r="2"><c r="A2"><f t="shared" si="0"/></c></row>"#
            ),
            &mut ids
        ),
        LoadError::Cell {
            part: "xl/worksheets/sheet1.xml".to_owned(),
            row: 2,
            column: 1,
            error: CellError::FormulaTooLong {
                units: FORMULA_MAX_UTF16_CODE_UNITS + 1
            }
        },
        "a shared formula's master at the limit whose member's text (A10 for A9) is past it"
    );
    let escaped = format!("A9+{}_x0031_", ones(FORMULA_MAX_UTF16_CODE_UNITS - 4));
    let within = m1_2_load(
        &m1_2_one_sheet(&format!(
            r#"<row r="1"><c r="A1"><f>{escaped}</f><v>1</v></c><c r="B1"><f>LEN("_x0041_")&amp;"_x005F_x0041_"</f></c></row>"#
        )),
        &mut ids,
    )
    .expect("a formula at the limit loads");
    assert_eq!(
        ids.slot_count(),
        slots + 1,
        "the refused loads released the sheet IDs they issued, so every load reused one slot"
    );
    assert_eq!(
        first_sheet_cell(&within, 1, 1),
        Some(Held::Formula(units(&longest))),
        "the _x0031_ escape decodes to one code unit, so the text is exactly at the limit"
    );
    assert_eq!(
        first_sheet_cell(&within, 1, 2),
        Some(Held::Formula(units(r#"LEN("A")&"_x0041_""#))),
        "ST_Formula is an ST_Xstring: _x0041_ is A and _x005F_ an underscore"
    );
    assert_eq!(
        within.sheets()[0]
            .formula(1, 1)
            .and_then(|formula| formula.cache())
            .map(|cache| cached_held(cache.value())),
        Some(Held::Number(1f64.to_bits()))
    );

    // Test-built packages from here on, each with its own sheet IDs.
    let mut built_ids: IdAllocator<SheetId> = IdAllocator::new();

    // An empty <v> of a number-typed formula cell is no cache: the cell loads as its formula
    // alone. An empty <v> of a str-typed formula cell is the empty text.
    let empty = m1_2_load(
        &m1_2_one_sheet(
            r#"<row r="1"><c r="A1"><f>1+1</f><v/></c><c r="B1"><f>2+2</f><v></v></c><c r="C1" t="n"><f>3*3</f><v> </v></c><c r="D1" t="str"><f>""</f><v></v></c></row>"#,
        ),
        &mut built_ids,
    )
    .expect("formulas with empty cached values load");
    for (column, text) in [(1, "1+1"), (2, "2+2"), (3, "3*3")] {
        assert_eq!(
            first_sheet_cell(&empty, 1, column),
            Some(Held::Formula(units(text))),
            "column {column}: the formula is the input"
        );
        assert_eq!(
            empty.sheets()[0]
                .formula(1, column)
                .expect("a formula record")
                .cache(),
            None,
            "column {column}: an empty <v> gives no FileCached value"
        );
    }
    assert_eq!(
        empty.sheets()[0]
            .formula(1, 4)
            .and_then(|formula| formula.cache())
            .map(|cache| cached_held(cache.value())),
        Some(Held::Text(Vec::new())),
        "an empty <v> of type str is the empty text"
    );

    // What load does not read, each refused with its typed variant.
    let refused_cells: [(&str, &str, u32, u32, CellError); 6] = [
        (
            "a formula of a kind SpreadsheetML does not define",
            r#"<row r="1"><c r="A1"><f t="future" ref="A1">1+1</f><v>2</v></c></row>"#,
            1,
            1,
            CellError::UnsupportedFormula {
                kind: "future".to_owned(),
            },
        ),
        (
            "a cell of type d",
            r#"<row r="1"><c r="A1" t="d"><v>2026-10-20T00:00:00</v></c></row>"#,
            1,
            1,
            CellError::UnsupportedType {
                kind: "d".to_owned(),
            },
        ),
        (
            "a shared member whose moved text leaves the grid",
            r#"<row r="1048575"><c r="A1048575"><f t="shared" ref="A1048575:A1048576" si="0">A1048576</f></c></row><row r="1048576"><c r="A1048576"><f t="shared" si="0"/></c></row>"#,
            MAX_ROWS,
            1,
            CellError::SharedFormulaPastGrid,
        ),
        (
            "a shared member whose si has no master",
            r#"<row r="1"><c r="A1"><f t="shared" si="5"/></c></row>"#,
            1,
            1,
            CellError::SharedFormula {
                detail: "a member whose si has no master",
            },
        ),
        (
            "a shared member of a group whose text holds a cell outside the grid",
            r#"<row r="1"><c r="A1"><f t="shared" ref="A1:A2" si="0">$XFE1+A1</f></c></row><row r="2"><c r="A2"><f t="shared" si="0"/></c></row>"#,
            2,
            1,
            CellError::SharedFormula {
                detail: "a cell reference outside the grid",
            },
        ),
        (
            "a shared member of a group whose text holds an unpaired surrogate",
            r#"<row r="1"><c r="A1"><f t="shared" ref="A1:A2" si="0">"_xD800_"&amp;A1</f></c></row><row r="2"><c r="A2"><f t="shared" si="0"/></c></row>"#,
            2,
            1,
            CellError::SharedFormula {
                detail: "a member of a group whose text holds an unpaired surrogate",
            },
        ),
    ];
    let mut refused_names = Vec::new();
    for (what, sheet_data, row, column, error) in refused_cells {
        assert_eq!(
            refusal(sheet_data, &mut built_ids),
            LoadError::Cell {
                part: "xl/worksheets/sheet1.xml".to_owned(),
                row,
                column,
                error,
            },
            "{what}"
        );
        refused_names.push(what);
    }

    // The date system and the defined names of the workbook part.
    let with_workbook = |before: &str, after: &str, ids: &mut IdAllocator<SheetId>| {
        m1_2_load(
            &m1_2_package(before, after, r#"<row r="1"><c r="A1"><v>1</v></c></row>"#),
            ids,
        )
    };
    let two_names = r#"<definedNames><definedName name="Rate">Long!$A$1</definedName><definedName name="Local" localSheetId="0">2</definedName></definedNames>"#;
    let mut systems = Vec::new();
    for (before, after, system, names) in [
        ("", "", DateSystem::Date1900, 0),
        (
            r#"<workbookPr date1904="1"/>"#,
            two_names,
            DateSystem::Date1904,
            2,
        ),
        (
            r#"<workbookPr date1904="true" defaultThemeVersion="164011"/>"#,
            "",
            DateSystem::Date1904,
            0,
        ),
        (
            r#"<workbookPr date1904="0"/>"#,
            "<definedNames/>",
            DateSystem::Date1900,
            0,
        ),
        (
            r#"<workbookPr date1904="false"/>"#,
            two_names,
            DateSystem::Date1900,
            2,
        ),
        (r#"<workbookPr/>"#, "", DateSystem::Date1900, 0),
    ] {
        let loaded = match with_workbook(before, after, &mut built_ids) {
            Ok(loaded) => loaded,
            Err(error) => panic!("{before}{after}: the package loads, but: {error}"),
        };
        assert_eq!(loaded.date_system(), system, "{before}");
        assert_eq!(loaded.defined_names_not_loaded(), names, "{after}");
        systems.push(format!(
            "{before:?} {after:?}: {:?}, {} defined names not loaded",
            loaded.date_system(),
            loaded.defined_names_not_loaded()
        ));
    }
    for (before, detail) in [
        (
            r#"<workbookPr date1904="yes"/>"#,
            "a workbookPr whose date1904 is not a boolean",
        ),
        (
            r#"<workbookPr/><workbookPr date1904="1"/>"#,
            "a workbook with two workbookPr elements",
        ),
    ] {
        match with_workbook(before, "", &mut built_ids) {
            Ok(_) => panic!("{before}: refused, but it loaded"),
            Err(error) => assert_eq!(
                error,
                LoadError::InvalidPart {
                    part: "xl/workbook.xml".to_owned(),
                    detail,
                },
                "{before}"
            ),
        }
    }

    note!(
        "ACCEPT-NOTE: {CASE}: cells.xlsx loaded through calc_doc::open and calc_doc::load: {} sheets in workbook order and {cell_count} occupied cells equal the generator's record input for input, with no cell for the 2 styled cells without a value or the 2 empty cell elements; {formula_count} formulas keep their text as input (shared formula members each with the generator's stated text) and {cached_count} caches are FileCached beside them, each naming its sheet's part by the name, CRC-32 and size the generator computed",
        loaded.model().sheets().len()
    );
    note!(
        "ACCEPT-NOTE: {CASE}: {} caches written deliberately wrong kept apart from their formulas: {:?}",
        wrong_caches.len(),
        wrong_caches
    );
    note!(
        "ACCEPT-NOTE: {CASE}: formula length: {} code units load and {} are refused with FormulaTooLong, also for a shared member whose moved text passes the limit; a decoded _xHHHH_ escape counts as one unit; the refused loads released their sheet IDs",
        FORMULA_MAX_UTF16_CODE_UNITS,
        FORMULA_MAX_UTF16_CODE_UNITS + 1
    );
    note!(
        "ACCEPT-NOTE: {CASE}: cells.xlsx's D8:E9 shared group loaded with the generator's stated text for each member: {:?}",
        group_texts
    );
    note!(
        "ACCEPT-NOTE: {CASE}: test-built package: formulas with <v/>, <v></v> and <v> </v> (no t, and t=\"n\") load as their formulas with no FileCached value; an empty <v> with t=\"str\" is the empty text"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: test-built packages refused with LoadError::Cell and the expected CellError: {:?}",
        refused_names
    );
    note!(
        "ACCEPT-NOTE: {CASE}: test-built packages: {:?}; date1904=\"yes\" and two workbookPr elements are refused with LoadError::InvalidPart; cells.xlsx names 1900 and holds 1 defined name, as its generator recorded",
        systems
    );
}

// ---------------------------------------------------------------------------------------------
// Case cdoc_m1_2__sparse_extent.

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cdoc_m1_2__sparse_extent() {
    const CASE: &str = "cdoc_m1_2__sparse_extent";
    let manifest: Value = serde_json::from_str(M1_2_MANIFEST).expect("manifest.json");

    // Each load measured by the bytes this thread asks the allocator for during calc_doc::load.
    let measured = |bytes: &[u8]| -> (LoadedWorkbook, u64) {
        let document = opened(memory(bytes), &OpenPolicy::default());
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let before = cdoc_m1_1_support::allocated();
        let loaded = calc_doc::load(&document, &mut ids);
        let used = cdoc_m1_1_support::allocated() - before;
        match loaded {
            Ok(loaded) => (loaded, used),
            Err(error) => panic!("the package loads, but: {error}"),
        }
    };
    let _ = measured(M1_2_CORNER);
    let (corner, corner_bytes) = measured(M1_2_CORNER);
    let (extent, extent_bytes) = measured(M1_2_EXTENT);

    let mut pages = Vec::new();
    for (file, bytes, loaded) in [
        ("corner.xlsx", M1_2_CORNER, &corner),
        ("extent.xlsx", M1_2_EXTENT, &extent),
    ] {
        let record = m1_2_record(&manifest, file);
        assert_eq!(bytes.len() as u64, record["length"].as_u64().unwrap());
        assert!(
            check_loaded(loaded, record).is_empty(),
            "{file}: no formulas"
        );
        let recorded_cell = &record["sheets"][0]["cells"][0];
        let (row, column) = a1(recorded_cell["ref"].as_str().unwrap());
        let sheet = loaded.model().sheets()[0].id();
        let sheet_pages: Vec<&Arc<CellPage>> =
            loaded.model().pages(sheet).expect("the sheet").collect();
        assert_eq!(sheet_pages.len(), 1, "{file}: one page for its one cell");
        let page = sheet_pages[0];
        assert_eq!(
            (page.first_row(), page.first_column()),
            (
                (row - 1) / PAGE_ROWS * PAGE_ROWS + 1,
                (column - 1) / PAGE_COLUMNS * PAGE_COLUMNS + 1
            ),
            "{file}: the page is the block that holds row {row}, column {column}"
        );
        assert_eq!(
            page.iter()
                .map(|(row, column, input)| (row, column, input_held(input)))
                .collect::<Vec<_>>(),
            vec![(row, column, recorded(&recorded_cell["input"]))],
            "{file}: the page holds the one cell"
        );
        assert_eq!(
            loaded
                .model()
                .cells(GridArea::full_sheet(sheet))
                .expect("the sheet")
                .count(),
            1,
            "{file}: a whole-sheet read yields the one cell"
        );
        pages.push((
            file,
            page.first_row(),
            page.last_row(),
            page.first_column(),
            page.last_column(),
        ));
    }
    assert_eq!(
        first_sheet_cell(&extent, MAX_ROWS, MAX_COLUMNS),
        Some(Held::Number(42f64.to_bits()))
    );
    assert_eq!(first_sheet_cell(&extent, 1, 1), None);
    assert!(
        extent
            .model()
            .page_of(cell(extent.model().sheets()[0].id(), 1, 1))
            .is_none(),
        "no page for the block at A1"
    );
    assert!(
        extent_bytes <= corner_bytes + 1024,
        "loading XFD1048576 asked the allocator for {extent_bytes} bytes and loading A1 for {corner_bytes}: the extent a sheet's cells span costs no allocation"
    );

    note!(
        "ACCEPT-NOTE: {CASE}: each package's one cell, as the generator recorded it, is the only cell of a model with one page (file, first row, last row, first column, last column): {:?}",
        pages
    );
    note!(
        "ACCEPT-NOTE: {CASE}: calc_doc::load asked this thread's allocator for {extent_bytes} bytes for extent.xlsx (only XFD1048576) and {corner_bytes} bytes for corner.xlsx (only A1); a dense sheet of {MAX_ROWS} rows by {MAX_COLUMNS} columns has {} cells",
        u64::from(MAX_ROWS) * u64::from(MAX_COLUMNS)
    );
}

// ---------------------------------------------------------------------------------------------
// cdoc-m1.3: authored cell changes, saved into a workbook the case reads with its own instruments.
//
// Each case edits a loaded workbook through calc_doc::prepare_edit, saves the edited revision
// through calc_doc::prepare_save and calc_doc::write, and reads the saved package with
// `cdoc_m1_1_support`'s ZIP reader, `cdoc_m1_3_support`'s DEFLATE decoder and XML parser, and
// expectations written here from the bead's rulings: never with calc-doc's loader, except to
// reload the saved package at the end. That Excel opens the package without repair and computes
// it is the owner's Excel step, which these cases do not show.

mod cdoc_m1_3_support;

use calc_doc::{DocumentSnapshot, EditError, SavePolicy, SaveReceipt, SaveRefusal, prepare_edit};
use cdoc_m1_1_support::crc32;
use cdoc_m1_3_support::{Element, inflate, parse_xml, xstring_units};

const SHARED_STRINGS_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings";
const SHARED_STRINGS_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";
const CALC_CHAIN_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/calcChain";
const CALC_CHAIN_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml";
const TABLE_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";
const TABLE_TYPE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml";
const WORKSHEET_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";

/// A part's content as the case's own instruments read it from a package: the stored payload,
/// inflated by the case's own decoder when deflated, checked against the CRC-32 and the size its
/// entry declares.
fn m1_3_content(zip: &Zip, package: &[u8], name: &str) -> Vec<u8> {
    let entry = zip.entry(name);
    let payload = entry.payload(package);
    let content = match entry.method {
        0 => payload.to_vec(),
        8 => inflate(payload),
        method => panic!("{name}: the compression method {method}"),
    };
    assert_eq!(
        content.len() as u32,
        entry.size,
        "{name}: the declared size"
    );
    assert_eq!(crc32(&content), entry.crc32, "{name}: the declared CRC-32");
    content
}

/// The column letters of `column` (1 is A), written here.
fn m1_3_letters(column: u32) -> String {
    let mut letters = Vec::new();
    let mut rest = column;
    while rest > 0 {
        letters.push(b'A' + ((rest - 1) % 26) as u8);
        rest = (rest - 1) / 26;
    }
    letters.reverse();
    String::from_utf8(letters).expect("letters")
}

/// Every occupied cell of every sheet of a model, by sheet position, as (row, column, input).
fn m1_3_cells(model: &ModelRoot) -> Vec<Vec<(u32, u32, CellInput)>> {
    model
        .sheets()
        .iter()
        .map(|sheet| {
            model
                .sheet_cells(sheet.id())
                .expect("the sheet's cells")
                .map(|(at, input)| (at.row(), at.column(), input.clone()))
                .collect()
        })
        .collect()
}

/// Each sheet's name, key and visibility, by position.
fn m1_3_sheets(model: &ModelRoot) -> Vec<(String, u32, SheetVisibility)> {
    model
        .sheets()
        .iter()
        .map(|sheet| {
            (
                sheet.name().text().to_string_lossy(),
                sheet.key().get(),
                sheet.visibility(),
            )
        })
        .collect()
}

/// The revision `operations` make of `model`, through calc_doc::prepare_edit.
fn m1_3_edit(model: &ModelRoot, operations: Vec<WorkbookOp>) -> Arc<ModelRoot> {
    let batch = OpBatch {
        base: model.revision(),
        operations,
    };
    match prepare_edit(model, &batch) {
        Ok(prepared) => prepared.into_model(),
        Err(error) => panic!("the edit is prepared, but: {error}"),
    }
}

/// The save route a host runs: prepare under `policy`, then write to a sink of its own. Returns
/// what the route ended with and every byte the sink received.
fn m1_3_route(
    snapshot: &DocumentSnapshot,
    policy: &SavePolicy,
) -> (Result<SaveReceipt, SaveRefusal>, Vec<u8>) {
    let mut sink = Vec::new();
    let result = calc_doc::prepare_save(snapshot, policy)
        .map(|plan| calc_doc::write(plan, &mut sink).expect("a write to memory"));
    (result, sink)
}

/// Saves a snapshot under the default policy.
fn m1_3_save(snapshot: &DocumentSnapshot) -> (Vec<u8>, SaveReceipt) {
    let (result, bytes) = m1_3_route(snapshot, &SavePolicy::default());
    match result {
        Ok(receipt) => {
            assert_eq!(receipt.bytes_written, bytes.len() as u64);
            (bytes, receipt)
        }
        Err(refusal) => panic!("the snapshot saves, but: {refusal}"),
    }
}

/// The `<c>` elements of a worksheet's `<sheetData>` in document order, each with its (row,
/// column), checking as it goes that every row and cell carries `r`, rows ascend, and cells ascend
/// within their row and lie in it.
fn m1_3_sheet_cells(worksheet: &Element) -> Vec<((u32, u32), &Element)> {
    let data = worksheet.child("sheetData").expect("sheetData");
    let mut cells = Vec::new();
    let mut previous_row = 0;
    for row in data.all("row") {
        let number: u32 = row
            .attribute("r")
            .expect("a row with r")
            .parse()
            .expect("a row number");
        assert!(
            number > previous_row,
            "row {number} after row {previous_row}"
        );
        previous_row = number;
        let mut previous_column = 0;
        for c in row.all("c") {
            let (r, column) = a1(c.attribute("r").expect("a cell with r"));
            assert_eq!(r, number, "a cell of row {number}");
            assert!(
                column > previous_column,
                "{c:?} after column {previous_column}"
            );
            previous_column = column;
            cells.push(((r, column), c));
        }
    }
    cells
}

/// The dimension the case expects of cells at these addresses: their area, `A1` for none.
fn m1_3_area(addresses: impl Iterator<Item = (u32, u32)>) -> String {
    let mut area: Option<(u32, u32, u32, u32)> = None;
    for (row, column) in addresses {
        area = Some(match area {
            None => (row, column, row, column),
            Some((t, l, b, r)) => (t.min(row), l.min(column), b.max(row), r.max(column)),
        });
    }
    match area {
        None => "A1".to_owned(),
        Some((t, l, b, r)) if (t, l) == (b, r) => format!("{}{t}", m1_3_letters(l)),
        Some((t, l, b, r)) => format!("{}{t}:{}{b}", m1_3_letters(l), m1_3_letters(r)),
    }
}

/// A package whose workbook part holds `workbook` (inside `<workbook>`), whose workbook part's
/// relationships are `relationships` (ID, type, target), and which holds `parts`, each with the
/// content type override given beside it (if any).
fn m1_3_package(
    workbook: &str,
    relationships: &[(&str, &str, &str)],
    parts: &[(&str, Option<&str>, String)],
) -> Vec<u8> {
    m1_3_package_marked("", workbook, relationships, parts)
}

/// The UTF-8 byte order mark a part may start with.
const M1_3_BOM: &str = "\u{FEFF}";

/// [`m1_3_package`] whose `[Content_Types].xml`, workbook part and workbook relationships part
/// start with `mark` (nothing, or a byte order mark).
fn m1_3_package_marked(
    mark: &str,
    workbook: &str,
    relationships: &[(&str, &str, &str)],
    parts: &[(&str, Option<&str>, String)],
) -> Vec<u8> {
    let mut overrides = r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#
        .to_owned();
    for (name, content_type, _) in parts {
        if let Some(content_type) = content_type {
            overrides.push_str(&format!(
                r#"<Override PartName="/{name}" ContentType="{content_type}"/>"#
            ));
        }
    }
    let content_types = format!(
        r#"{mark}<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="{}"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>{overrides}</Types>"#,
        calc_doc::CONTENT_TYPES_NAMESPACE
    );
    let workbook = format!(
        r#"{mark}<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="{SPREADSHEET_MAIN}" xmlns:r="{RELATIONSHIP_ATTRIBUTES}">{workbook}</workbook>"#
    );
    let mut items = String::new();
    for (id, kind, target) in relationships {
        items.push_str(&format!(
            r#"<Relationship Id="{id}" Type="{kind}" Target="{target}"/>"#
        ));
    }
    let workbook_relationships = format!(
        r#"{mark}<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{}">{items}</Relationships>"#,
        calc_doc::RELATIONSHIPS_NAMESPACE
    );
    let mut entries = vec![
        RawEntry::stored("[Content_Types].xml", content_types.as_bytes()),
        RawEntry::stored("_rels/.rels", base_relationships().as_bytes()),
        RawEntry::stored("xl/workbook.xml", workbook.as_bytes()),
        RawEntry::stored(
            "xl/_rels/workbook.xml.rels",
            workbook_relationships.as_bytes(),
        ),
    ];
    for (name, _, content) in parts {
        entries.push(RawEntry::stored(name, content.as_bytes()));
    }
    build_zip(&entries)
}

/// A worksheet part whose `<worksheet>` holds `inner`.
fn m1_3_worksheet(inner: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="{SPREADSHEET_MAIN}" xmlns:r="{RELATIONSHIP_ATTRIBUTES}">{inner}</worksheet>"#
    )
}

/// numbers.xlsx, built here: one sheet "Numbers" holding A1 = 1 and A2 = A1+1 with the cached
/// result 2, a calculation chain naming A2, no shared strings part, and a workbook part without
/// `calcPr` whose `definedNames` is followed by an `extLst`.
fn m1_3_numbers() -> Vec<u8> {
    m1_3_package(
        r#"<workbookPr/><sheets><sheet name="Numbers" sheetId="1" r:id="rId1"/></sheets><definedNames><definedName name="Two">Numbers!$A$2</definedName></definedNames><extLst><ext uri="{00000000-0000-0000-0000-000000000000}"/></extLst>"#,
        &[
            ("rId1", WORKSHEET_RELATIONSHIP, "worksheets/sheet1.xml"),
            ("rId2", CALC_CHAIN_RELATIONSHIP, "calcChain.xml"),
        ],
        &[
            (
                "xl/worksheets/sheet1.xml",
                Some(WORKSHEET_TYPE),
                m1_3_worksheet(
                    r#"<dimension ref="A1:A2"/><sheetData><row r="1"><c r="A1"><v>1</v></c></row><row r="2"><c r="A2"><f>A1+1</f><v>2</v></c></row></sheetData>"#,
                ),
            ),
            (
                "xl/calcChain.xml",
                Some(CALC_CHAIN_TYPE),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><calcChain xmlns="{SPREADSHEET_MAIN}"><c r="A2" i="1"/></calcChain>"#
                ),
            ),
        ],
    )
}

/// shaped.xlsx, a package built here that adds a byte order mark, and `\r\n` after the
/// declaration, to the workbook part, its relationships, the content types, the sheet "Shaped" and
/// the shared strings, so a rewrite is shown to keep what precedes a part's document element;
/// "Shaped" declares `mc:Ignorable` namespaces, carries `xr:uid` on the worksheet and on a data
/// validation and `x14ac:dyDescent` on its rows and format, and holds A1 = 1, B1 = the shared
/// string "label" and A2 = A1*2 with the cached result 2. The sheet "Empty" has an empty
/// `<sheetData/>`. A calculation chain names A2.
fn m1_3_shaped() -> Vec<u8> {
    let declared = |root: String| {
        format!("{M1_3_BOM}<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n{root}")
    };
    let shaped = declared(format!(
        r#"<worksheet xmlns="{SPREADSHEET_MAIN}" xmlns:r="{RELATIONSHIP_ATTRIBUTES}" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" mc:Ignorable="x14ac xr xr2 xr3" xmlns:x14ac="http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac" xmlns:xr="http://schemas.microsoft.com/office/spreadsheetml/2014/revision" xmlns:xr2="http://schemas.microsoft.com/office/spreadsheetml/2015/revision2" xmlns:xr3="http://schemas.microsoft.com/office/spreadsheetml/2016/revision3" xr:uid="{{6A1B2C3D-0000-4000-8000-000000000001}}"><dimension ref="A1:B2"/><sheetViews><sheetView tabSelected="1" workbookViewId="0"/></sheetViews><sheetFormatPr defaultRowHeight="14.4" x14ac:dyDescent="0.3"/><sheetData><row r="1" spans="1:2" x14ac:dyDescent="0.3"><c r="A1"><v>1</v></c><c r="B1" t="s"><v>0</v></c></row><row r="2" spans="1:2" x14ac:dyDescent="0.3"><c r="A2"><f>A1*2</f><v>2</v></c></row></sheetData><dataValidations count="1"><dataValidation type="whole" allowBlank="1" showInputMessage="1" showErrorMessage="1" sqref="C1" xr:uid="{{6A1B2C3D-0000-4000-8000-000000000002}}"><formula1>0</formula1><formula2>10</formula2></dataValidation></dataValidations><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/></worksheet>"#
    ));
    m1_3_package_marked(
        M1_3_BOM,
        r#"<sheets><sheet name="Shaped" sheetId="1" r:id="rId1"/><sheet name="Empty" sheetId="2" r:id="rId2"/></sheets><calcPr calcId="191029"/>"#,
        &[
            ("rId1", WORKSHEET_RELATIONSHIP, "worksheets/sheet1.xml"),
            ("rId2", WORKSHEET_RELATIONSHIP, "worksheets/sheet2.xml"),
            ("rId3", SHARED_STRINGS_RELATIONSHIP, "sharedStrings.xml"),
            ("rId4", CALC_CHAIN_RELATIONSHIP, "calcChain.xml"),
        ],
        &[
            ("xl/worksheets/sheet1.xml", Some(WORKSHEET_TYPE), shaped),
            (
                "xl/worksheets/sheet2.xml",
                Some(WORKSHEET_TYPE),
                m1_3_worksheet(r#"<dimension ref="A1"/><sheetData/>"#),
            ),
            (
                "xl/sharedStrings.xml",
                Some(SHARED_STRINGS_TYPE),
                declared(format!(
                    r#"<sst xmlns="{SPREADSHEET_MAIN}" count="1" uniqueCount="1"><si><t>label</t></si></sst>"#
                )),
            ),
            (
                "xl/calcChain.xml",
                Some(CALC_CHAIN_TYPE),
                declared(format!(
                    r#"<calcChain xmlns="{SPREADSHEET_MAIN}"><c r="A2" i="1"/></calcChain>"#
                )),
            ),
        ],
    )
}

// ---------------------------------------------------------------------------------------------
// Case cdoc_m1_3__formula_authoring.

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cdoc_m1_3__formula_authoring() {
    const CASE: &str = "cdoc_m1_3__formula_authoring";
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let loaded = m1_2_load(M1_2_CELLS, &mut ids).expect("cells.xlsx loads");
    let base = Arc::clone(loaded.model());
    let base_cells = m1_3_cells(&base);
    let sheet = base.sheets()[0].id();
    assert_eq!(base.sheets()[0].name().text().to_string_lossy(), "Cells");
    assert_eq!(base.cell(cell(sheet, 3, 10)), None, "J3 is absent");

    // prepare_edit refuses a formula a save could not write, and a batch the model refuses.
    let batch = |operations: Vec<WorkbookOp>| OpBatch {
        base: base.revision(),
        operations,
    };
    let refused = |operations: Vec<WorkbookOp>| match prepare_edit(&base, &batch(operations)) {
        Ok(_) => panic!("the edit is refused, but it was prepared"),
        Err(error) => error,
    };
    let unclosed = refused(vec![set(sheet, 3, 10, formula("SUM(A6:A10"))]);
    assert!(
        matches!(unclosed, EditError::FormulaSyntax { operation: 0, .. }),
        "{unclosed:?}"
    );
    let marked = refused(vec![
        set(sheet, 3, 10, number(1.0)),
        set(sheet, 3, 11, formula("=A1")),
    ]);
    assert!(
        matches!(marked, EditError::FormulaSyntax { operation: 1, .. }),
        "a formula's text written with its = is refused: {marked:?}"
    );
    let long: String = "1+".repeat(4_096) + "1";
    assert_eq!(
        refused(vec![set(sheet, 3, 10, formula(&long))]),
        EditError::FormulaTooLong {
            operation: 0,
            units: 8_193
        }
    );
    // The deep chain `1+1+...+1` of 4,095 additions, 8,191 code units: prepared, saved (where
    // prepare_save checks it again), read back by the case's own reader and reloaded.
    let deep = "1".to_owned() + &"+1".repeat(4_095);
    assert_eq!(deep.len(), 8_191);
    let deep_model = m1_3_edit(&base, vec![set(sheet, 3, 10, formula(&deep))]);
    let (deep_out, _) = m1_3_save(&loaded.snapshot(&deep_model));
    let deep_zip = parse_zip(&deep_out);
    let deep_sheet = m1_3_content(&deep_zip, &deep_out, "xl/worksheets/sheet2.xml");
    let deep_root = parse_xml(&deep_sheet);
    let deep_cells = m1_3_sheet_cells(&deep_root);
    let (_, deep_cell) = deep_cells
        .iter()
        .find(|(at, _)| *at == (3, 10))
        .expect("the saved sheet has J3");
    assert_eq!(
        deep_cell.child("f").map(|f| f.text()),
        Some(deep.clone()),
        "J3's <f> holds the deep chain"
    );
    assert!(deep_cell.child("v").is_none());
    let mut deep_ids: IdAllocator<SheetId> = IdAllocator::new();
    let deep_reloaded = m1_2_load(&deep_out, &mut deep_ids).expect("the saved package loads");
    assert_eq!(
        m1_3_cells(deep_reloaded.model()),
        m1_3_cells(&deep_model),
        "the reloaded package holds the deep chain"
    );
    let stale = prepare_edit(
        &base,
        &OpBatch {
            base: AuthoredRevision::new(5),
            operations: vec![set(sheet, 3, 10, number(1.0))],
        },
    )
    .expect_err("a stale batch");
    assert_eq!(
        stale,
        EditError::Model(ApplyError::StaleBase {
            batch: AuthoredRevision::new(5),
            root: AuthoredRevision::INITIAL
        })
    );
    assert_eq!(
        refused(vec![WorkbookOp::EditSheet(SheetEdit::Remove { sheet })]),
        EditError::Model(ApplyError::SheetEdit { operation: 0 })
    );
    assert_eq!(
        m1_3_cells(&base),
        base_cells,
        "a refused edit changed nothing"
    );

    // The edit: a formula into an absent cell of an existing row, and every other kind of change.
    let new_formula = "SUM(A6:A10)*2";
    let escaped_formula = r#"IF(A1<1,"a&b_x0041_","")"#;
    let operations = vec![
        set(sheet, 3, 10, formula(new_formula)),
        set(sheet, 11, 2, formula(escaped_formula)),
        set(sheet, 14, 3, text(units("new text"))),
        set(sheet, 1, 9, text(units(" Grüße, 数据 "))),
        set(sheet, 2, 9, text(units("boldplain"))),
        set(sheet, 1, 1, number(42.0)),
        set(sheet, 5, 1, formula("A1*3")),
        set(sheet, 8, 3, number(7.0)),
        set(sheet, 1, 11, CellInput::Logical(true)),
        set(sheet, 2, 11, CellInput::Error(WorksheetErrorCode::NA)),
        WorkbookOp::Clear {
            area: GridArea::new(cell(sheet, 1, 2), cell(sheet, 1, 3)).unwrap(),
        },
        WorkbookOp::Clear {
            area: GridArea::new(cell(sheet, 2, 3), cell(sheet, 2, 3)).unwrap(),
        },
    ];
    let model = m1_3_edit(&base, operations);
    assert_eq!(model.revision(), AuthoredRevision::new(1));
    assert_eq!(
        model.cell(cell(sheet, 3, 10)),
        Some(&formula(new_formula)),
        "the edited revision holds the new formula"
    );
    assert_eq!(
        m1_3_cells(&base),
        base_cells,
        "the loaded revision is unchanged"
    );
    let edited_cells = m1_3_cells(&model);

    let snapshot = loaded.snapshot(&model);
    assert!(Arc::ptr_eq(snapshot.model().expect("a model"), &model));
    let (out, receipt) = m1_3_save(&snapshot);
    assert_eq!(receipt.revision, Some(AuthoredRevision::new(1)));

    // The package, entry by entry, with the case's own ZIP reader.
    let input = parse_zip(M1_2_CELLS);
    let output = parse_zip(&out);
    let names = |zip: &Zip| -> Vec<String> { zip.entries.iter().map(|e| e.name.clone()).collect() };
    assert_eq!(
        names(&output),
        names(&input),
        "the central directory keeps its entries and order"
    );
    assert_eq!(
        names_in_stored_order(&output),
        names_in_stored_order(&input),
        "the entries keep the order they were stored in"
    );
    // The workbook part is not among them: a save asks for no full calculation (dc-35p), so its
    // calcPr stays as it was.
    let rewritten: BTreeSet<&str> = [
        "xl/worksheets/sheet1.xml",
        "xl/worksheets/sheet2.xml",
        "xl/sharedStrings.xml",
    ]
    .into();
    let mut untouched = Vec::new();
    for source in &input.entries {
        let saved = output.entry(&source.name);
        if rewritten.contains(source.name.as_str()) {
            assert_eq!(
                (saved.method, saved.flags, saved.time, saved.date),
                (source.method, source.flags, source.time, source.date),
                "{}: a rewritten part keeps its method, flags and timestamp",
                source.name
            );
            assert!(saved.descriptor.is_empty() && saved.local_extra.is_empty());
            assert_ne!(
                m1_3_content(&output, &out, &source.name),
                m1_3_content(&input, M1_2_CELLS, &source.name),
                "{} is rewritten",
                source.name
            );
        } else {
            assert_eq!(
                preserved_fields(saved, &out),
                preserved_fields(source, M1_2_CELLS),
                "{}: an untouched part keeps every stored byte",
                source.name
            );
            untouched.push(source.name.clone());
        }
    }
    assert_eq!(receipt.entries_copied, untouched.len());
    assert_eq!(receipt.parts_written, rewritten.len());

    // The edited sheet, with the case's own XML parser.
    let source_sheet = m1_3_content(&input, M1_2_CELLS, "xl/worksheets/sheet2.xml");
    let saved_sheet = m1_3_content(&output, &out, "xl/worksheets/sheet2.xml");
    let (before, after) = (parse_xml(&source_sheet), parse_xml(&saved_sheet));
    assert_eq!(
        &saved_sheet[..after.start_tag.1],
        &source_sheet[..before.start_tag.1],
        "the declaration and the worksheet start tag"
    );
    let outside = |root: &Element, bytes: &[u8]| -> Vec<(String, Vec<u8>)> {
        root.elements()
            .filter(|element| !matches!(element.local(), "sheetData" | "dimension"))
            .map(|element| (element.name.clone(), element.bytes(bytes).to_vec()))
            .collect()
    };
    assert_eq!(
        outside(&after, &saved_sheet),
        outside(&before, &source_sheet),
        "every element outside sheetData but the dimension keeps its bytes"
    );
    assert_eq!(
        after.elements().map(|e| e.name.clone()).collect::<Vec<_>>(),
        before
            .elements()
            .map(|e| e.name.clone())
            .collect::<Vec<_>>(),
        "the worksheet's elements keep their order"
    );
    let saved_cells = m1_3_sheet_cells(&after);
    let source_cells = m1_3_sheet_cells(&before);
    assert_eq!(
        before
            .child("sheetData")
            .unwrap()
            .all("row")
            .filter(|row| row.attribute("spans").is_some())
            .count(),
        1,
        "the source's row 1 carries spans"
    );
    assert!(
        after
            .child("sheetData")
            .unwrap()
            .all("row")
            .all(|row| row.attribute("spans").is_none()),
        "no saved row carries spans, a hint of the columns the source's rows held"
    );
    let dimension = after.child("dimension").expect("a dimension");
    assert_eq!(
        dimension.attribute("ref"),
        Some(m1_3_area(saved_cells.iter().map(|(at, _)| *at)).as_str()),
        "the dimension is the area of the cells written"
    );
    assert_eq!(dimension.attribute("ref"), Some("A1:K14"));
    let rows: Vec<u32> = after
        .child("sheetData")
        .unwrap()
        .all("row")
        .map(|row| row.attribute("r").unwrap().parse().unwrap())
        .collect();
    assert_eq!(
        rows,
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 14],
        "rows in order"
    );
    let saved_at: BTreeMap<(u32, u32), &Element> = saved_cells.iter().copied().collect();
    let bytes_at = |at: &str| -> String {
        let element = saved_at
            .get(&a1(at))
            .unwrap_or_else(|| panic!("the saved sheet has a cell {at}"));
        String::from_utf8(element.bytes(&saved_sheet).to_vec()).unwrap()
    };

    // The new formula: <f> holds its text, and no <v> or t claims a result nothing calculated.
    let new = saved_at[&a1("J3")];
    assert_eq!(new.attributes, [("r".to_owned(), "J3".to_owned())]);
    assert_eq!(
        new.elements().map(|e| e.name.clone()).collect::<Vec<_>>(),
        ["f"]
    );
    assert_eq!(new.child("f").unwrap().text(), new_formula);
    assert!(new.child("f").unwrap().attributes.is_empty());
    assert_eq!(
        bytes_at("J3"),
        format!("<c r=\"J3\"><f>{new_formula}</f></c>")
    );

    // Every other change, as the rulings write it.
    let expected = [
        (
            "B11",
            r#"<c r="B11"><f>IF(A1&lt;1,"a&amp;b_x005F_x0041_","")</f></c>"#,
        ),
        ("C14", r#"<c r="C14" t="s"><v>5</v></c>"#),
        ("I1", r#"<c r="I1" t="s"><v>1</v></c>"#),
        ("I2", r#"<c r="I2" t="s"><v>4</v></c>"#),
        ("A1", r#"<c r="A1"><v>42</v></c>"#),
        ("A5", r#"<c r="A5"><f>A1*3</f></c>"#),
        ("C8", r#"<c r="C8"><v>7</v></c>"#),
        ("K1", r#"<c r="K1" t="b"><v>1</v></c>"#),
        ("K2", r#"<c r="K2" t="e"><v>#N/A</v></c>"#),
        ("B1", r#"<c r="B1" s="1"/>"#),
        // The shared group C6:C10, one of whose cells changed, as individual formulas.
        ("C6", r#"<c r="C6"><f>A6*2+$A$1</f></c>"#),
        ("C7", r#"<c r="C7"><f>A7*2+$A$1</f></c>"#),
        ("C9", r#"<c r="C9"><f>A9*2+$A$1</f></c>"#),
        ("C10", r#"<c r="C10"><f>A10*2+$A$1</f></c>"#),
    ];
    for (at, xml) in expected {
        assert_eq!(bytes_at(at), xml, "{at}");
    }
    assert_eq!(
        xstring_units(&saved_at[&a1("B11")].child("f").unwrap().text()),
        units(escaped_formula),
        "the escaped formula reads back as authored"
    );
    for gone in ["C1", "C2"] {
        assert!(!saved_at.contains_key(&a1(gone)), "{gone} is removed");
    }
    let row = |number: u32| {
        after
            .child("sheetData")
            .unwrap()
            .all("row")
            .find(|row| row.attribute("r") == Some(&number.to_string()))
            .unwrap_or_else(|| panic!("row {number}"))
            .clone()
    };
    assert_eq!(row(11).attributes, [("r".to_owned(), "11".to_owned())]);
    assert_eq!(
        row(3).attributes,
        [
            ("r".to_owned(), "3".to_owned()),
            ("ht".to_owned(), "20".to_owned()),
            ("customHeight".to_owned(), "1".to_owned())
        ],
        "row 3 keeps its height"
    );
    assert_eq!(
        String::from_utf8(row(12).bytes(&saved_sheet).to_vec()).unwrap(),
        r#"<row r="12" ht="30" customHeight="1"/>"#
    );

    // Every other cell: a formula keeps its <f> (as the file wrote it, for a group none of whose
    // cells changed) and loses its <v> and t; any other cell keeps its bytes.
    let changed: BTreeSet<(u32, u32)> = [
        "J3", "B11", "C14", "I1", "I2", "A1", "A5", "C8", "K1", "K2", "B1", "C1", "C2", "C6", "C7",
        "C9", "C10",
    ]
    .iter()
    .map(|at| a1(at))
    .collect();
    let mut kept_formulas = 0;
    let mut kept_values = 0;
    for (at, source) in &source_cells {
        if changed.contains(at) {
            continue;
        }
        let saved = saved_at[at];
        match source.child("f") {
            Some(source_f) => {
                let saved_f = saved.child("f").expect("the formula is kept");
                assert_eq!(
                    saved_f.bytes(&saved_sheet),
                    source_f.bytes(&source_sheet),
                    "{at:?}"
                );
                assert_eq!(
                    saved.child("v").map(|v| v.text()),
                    None,
                    "{at:?}: no cached result"
                );
                assert_eq!(saved.attribute("t"), None, "{at:?}: no result type");
                assert_eq!(
                    saved.attribute("s"),
                    source.attribute("s"),
                    "{at:?}: its style"
                );
                kept_formulas += 1;
            }
            None => {
                assert_eq!(
                    saved.bytes(&saved_sheet),
                    source.bytes(&source_sheet),
                    "{at:?}"
                );
                kept_values += 1;
            }
        }
    }
    for (at, saved) in &saved_cells {
        if saved.child("f").is_some() {
            assert!(
                saved.child("v").is_none() && saved.attribute("t").is_none(),
                "{at:?}: no formula keeps a cached result"
            );
        }
    }

    // The shared strings: the held items keep their bytes and indices; new text is appended,
    // except text a plain item already holds; the counts follow.
    let source_strings = m1_3_content(&input, M1_2_CELLS, "xl/sharedStrings.xml");
    let saved_strings = m1_3_content(&output, &out, "xl/sharedStrings.xml");
    let (sst_before, sst_after) = (parse_xml(&source_strings), parse_xml(&saved_strings));
    let items = |root: &Element, bytes: &[u8]| -> Vec<String> {
        root.all("si")
            .map(|si| String::from_utf8(si.bytes(bytes).to_vec()).unwrap())
            .collect()
    };
    let held = items(&sst_before, &source_strings);
    let now = items(&sst_after, &saved_strings);
    assert_eq!(held.len(), 4);
    assert_eq!(&now[..4], &held[..], "the held items keep their bytes");
    assert_eq!(
        &now[4..],
        ["<si><t>boldplain</t></si>", "<si><t>new text</t></si>"],
        "new text is appended in the order the rows meet it; the rich item that reads boldplain is not reused"
    );
    assert_eq!(sst_before.attribute("count"), Some("5"));
    assert_eq!(
        sst_after.attribute("count"),
        Some("7"),
        "5 references, 3 added (C14, I1, I2) and 1 removed (C2)"
    );
    assert_eq!(sst_after.attribute("uniqueCount"), Some("6"));

    // The workbook part: byte for byte, with no fullCalcOnLoad: every formula is saved without a
    // value, which Excel calculates when it opens the package (tests/fixtures/d1/excel-open.json).
    let source_workbook = m1_3_content(&input, M1_2_CELLS, "xl/workbook.xml");
    let saved_workbook = m1_3_content(&output, &out, "xl/workbook.xml");
    assert_eq!(saved_workbook, source_workbook);
    let book_after = parse_xml(&saved_workbook);
    let calc_after = book_after.child("calcPr").unwrap();
    assert_eq!(calc_after.attribute("fullCalcOnLoad"), None);
    assert_eq!(calc_after.attribute("calcId"), Some("191029"));

    // The other sheet, untouched by the edit, loses its formula's cached result only.
    let saved_second = m1_3_content(&output, &out, "xl/worksheets/sheet1.xml");
    let source_second = m1_3_content(&input, M1_2_CELLS, "xl/worksheets/sheet1.xml");
    let second = parse_xml(&saved_second);
    let second_cells = m1_3_sheet_cells(&second);
    assert_eq!(
        String::from_utf8(second_cells[0].1.bytes(&saved_second).to_vec()).unwrap(),
        "<c r=\"A1\"><f>Cells!A1+1</f></c>"
    );
    assert_eq!(
        second_cells[1].1.bytes(&saved_second),
        m1_3_sheet_cells(&parse_xml(&source_second))[1]
            .1
            .bytes(&source_second)
    );
    assert_eq!(
        second.child("dimension").unwrap().attribute("ref"),
        Some("A1:B1")
    );

    // The caches dropped, counted from the source parts with the case's own parser: every formula
    // cell holding a <v> that the edit neither set nor cleared, on both sheets. The second sheet
    // holds no edited cell, so it was rewritten only to drop its formula's cache.
    let edited: BTreeSet<(u32, u32)> = [
        "J3", "B11", "C14", "I1", "I2", "A1", "A5", "C8", "K1", "K2", "B1", "C1", "C2",
    ]
    .iter()
    .map(|at| a1(at))
    .collect();
    let cached = |cells: &[((u32, u32), &Element)], edited: &BTreeSet<(u32, u32)>| {
        cells
            .iter()
            .filter(|(at, c)| {
                !edited.contains(at) && c.child("f").is_some() && c.child("v").is_some()
            })
            .count()
    };
    let source_second_root = parse_xml(&source_second);
    let dropped = cached(&source_cells, &edited)
        + cached(&m1_3_sheet_cells(&source_second_root), &BTreeSet::new());
    assert_eq!(
        dropped, 16,
        "15 formula caches on Cells and 1 on Second Sheet"
    );
    assert_eq!(
        (receipt.caches_dropped, receipt.sheets_rewritten_for_caches),
        (dropped, 1),
        "the receipt counts the caches dropped and the sheet rewritten only for its caches"
    );

    // Reloaded through calc_doc::open and calc_doc::load, the package holds the edited revision.
    let mut reload_ids: IdAllocator<SheetId> = IdAllocator::new();
    let reloaded = m1_2_load(&out, &mut reload_ids).expect("the saved package loads");
    assert_eq!(m1_3_sheets(reloaded.model()), m1_3_sheets(&model));
    assert_eq!(
        m1_3_cells(reloaded.model()),
        edited_cells,
        "the saved package holds the edited revision's cells"
    );
    for loaded_sheet in reloaded.sheets() {
        for (row, column, record) in loaded_sheet.formulas() {
            assert_eq!(
                record.cache(),
                None,
                "({row}, {column}) has no cached result"
            );
        }
    }
    let cells_sheet = &reloaded.sheets()[0];
    assert_eq!(cells_sheet.style(1, 2), 1, "B1 keeps its style");
    assert_eq!(cells_sheet.style(1, 8), 2, "H1 keeps its style");
    assert_eq!(cells_sheet.formula(6, 3).unwrap().shared(), None);
    assert_eq!(
        cells_sheet
            .formula(6, 5)
            .unwrap()
            .shared()
            .map(|group| group.index()),
        Some(1),
        "a group none of whose cells changed stays shared"
    );

    // numbers.xlsx: a workbook without shared strings and with a calculation chain.
    let numbers = m1_3_numbers();
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let loaded_numbers = m1_2_load(&numbers, &mut ids).expect("numbers.xlsx loads");
    let numbers_sheet = loaded_numbers.model().sheets()[0].id();

    // A changed value leaves the set of formula cells, and so the chain, as it was.
    let value_only = m1_3_edit(
        loaded_numbers.model(),
        vec![set(numbers_sheet, 1, 1, number(5.0))],
    );
    let (value_out, value_receipt) = m1_3_save(&loaded_numbers.snapshot(&value_only));
    assert_eq!(
        (
            value_receipt.caches_dropped,
            value_receipt.sheets_rewritten_for_caches
        ),
        (1, 0),
        "A2's cache is dropped on the sheet the edit changed"
    );
    let (numbers_zip, value_zip) = (parse_zip(&numbers), parse_zip(&value_out));
    assert_eq!(names(&value_zip), names(&numbers_zip));
    for name in [
        "[Content_Types].xml",
        "xl/_rels/workbook.xml.rels",
        "xl/calcChain.xml",
    ] {
        assert_eq!(
            preserved_fields(value_zip.entry(name), &value_out),
            preserved_fields(numbers_zip.entry(name), &numbers),
            "{name} is kept when no formula cell came or went"
        );
    }

    // A text and a formula added: a shared strings part is added, the chain is removed.
    let added = m1_3_edit(
        loaded_numbers.model(),
        vec![
            set(numbers_sheet, 1, 2, text(units("hello"))),
            set(numbers_sheet, 2, 2, formula("A2*2")),
        ],
    );
    let (added_out, added_receipt) = m1_3_save(&loaded_numbers.snapshot(&added));
    let added_zip = parse_zip(&added_out);
    assert_eq!(
        names(&added_zip),
        [
            "[Content_Types].xml",
            "_rels/.rels",
            "xl/workbook.xml",
            "xl/_rels/workbook.xml.rels",
            "xl/worksheets/sheet1.xml",
            "xl/sharedStrings.xml"
        ],
        "the chain is gone and the shared strings part is added last"
    );
    // The workbook part is copied: no calcPr is added (dc-35p).
    assert_eq!(added_receipt.entries_copied, 2);
    assert_eq!(added_receipt.parts_written, 4);
    assert_eq!(
        preserved_fields(added_zip.entry("_rels/.rels"), &added_out),
        preserved_fields(numbers_zip.entry("_rels/.rels"), &numbers)
    );
    let types = parse_xml(&m1_3_content(&added_zip, &added_out, "[Content_Types].xml"));
    let overrides: Vec<(String, String)> = types
        .all("Override")
        .map(|o| {
            (
                o.attribute("PartName").unwrap().to_owned(),
                o.attribute("ContentType").unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        overrides,
        [
            (
                "/xl/workbook.xml".to_owned(),
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"
                    .to_owned()
            ),
            (
                "/xl/worksheets/sheet1.xml".to_owned(),
                WORKSHEET_TYPE.to_owned()
            ),
            (
                "/xl/sharedStrings.xml".to_owned(),
                SHARED_STRINGS_TYPE.to_owned()
            ),
        ]
    );
    let relationships = parse_xml(&m1_3_content(
        &added_zip,
        &added_out,
        "xl/_rels/workbook.xml.rels",
    ));
    let items: Vec<(String, String, String)> = relationships
        .all("Relationship")
        .map(|r| {
            (
                r.attribute("Id").unwrap().to_owned(),
                r.attribute("Type").unwrap().to_owned(),
                r.attribute("Target").unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        items,
        [
            (
                "rId1".to_owned(),
                WORKSHEET_RELATIONSHIP.to_owned(),
                "worksheets/sheet1.xml".to_owned()
            ),
            (
                "rId3".to_owned(),
                SHARED_STRINGS_RELATIONSHIP.to_owned(),
                "sharedStrings.xml".to_owned()
            ),
        ]
    );
    let strings_entry = added_zip.entry("xl/sharedStrings.xml");
    assert_eq!(strings_entry.method, 8, "the added part is deflated");
    let strings = parse_xml(&m1_3_content(
        &added_zip,
        &added_out,
        "xl/sharedStrings.xml",
    ));
    assert_eq!(
        (strings.local(), strings.attribute("xmlns")),
        ("sst", Some(SPREADSHEET_MAIN))
    );
    assert_eq!(
        (strings.attribute("count"), strings.attribute("uniqueCount")),
        (Some("1"), Some("1"))
    );
    assert_eq!(
        strings.all("si").map(|si| si.text()).collect::<Vec<_>>(),
        ["hello"]
    );
    let book = parse_xml(&m1_3_content(&added_zip, &added_out, "xl/workbook.xml"));
    assert_eq!(
        book.elements().map(|e| e.name.clone()).collect::<Vec<_>>(),
        ["workbookPr", "sheets", "definedNames", "extLst"],
        "no calcPr is added: the save asks for no full calculation"
    );
    let numbers_xml = m1_3_content(&added_zip, &added_out, "xl/worksheets/sheet1.xml");
    let numbers_root = parse_xml(&numbers_xml);
    let numbers_cells: Vec<(String, String)> = m1_3_sheet_cells(&numbers_root)
        .iter()
        .map(|(_, c)| {
            (
                c.attribute("r").unwrap().to_owned(),
                String::from_utf8(c.bytes(&numbers_xml).to_vec()).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        numbers_cells,
        [
            ("A1".to_owned(), r#"<c r="A1"><v>1</v></c>"#.to_owned()),
            (
                "B1".to_owned(),
                r#"<c r="B1" t="s"><v>0</v></c>"#.to_owned()
            ),
            ("A2".to_owned(), r#"<c r="A2"><f>A1+1</f></c>"#.to_owned()),
            ("B2".to_owned(), r#"<c r="B2"><f>A2*2</f></c>"#.to_owned()),
        ]
    );
    assert_eq!(
        numbers_root.child("dimension").unwrap().attribute("ref"),
        Some("A1:B2")
    );
    let mut reload_ids: IdAllocator<SheetId> = IdAllocator::new();
    let reloaded_numbers = m1_2_load(&added_out, &mut reload_ids).expect("it loads");
    assert_eq!(m1_3_cells(reloaded_numbers.model()), m1_3_cells(&added));

    // The formula cleared: the set of formula cells changed by a removal alone, so the chain goes
    // with its relationship and its content type override, and no part is added.
    let removed = m1_3_edit(
        loaded_numbers.model(),
        vec![WorkbookOp::Clear {
            area: GridArea::new(cell(numbers_sheet, 2, 1), cell(numbers_sheet, 2, 1)).unwrap(),
        }],
    );
    assert_eq!(removed.cell(cell(numbers_sheet, 2, 1)), None);
    let (removed_out, removed_receipt) = m1_3_save(&loaded_numbers.snapshot(&removed));
    let removed_zip = parse_zip(&removed_out);
    assert_eq!(
        names(&removed_zip),
        [
            "[Content_Types].xml",
            "_rels/.rels",
            "xl/workbook.xml",
            "xl/_rels/workbook.xml.rels",
            "xl/worksheets/sheet1.xml"
        ],
        "the chain is gone and nothing is added"
    );
    assert_eq!(
        (
            removed_receipt.caches_dropped,
            removed_receipt.sheets_rewritten_for_caches
        ),
        (0, 0),
        "the only formula was cleared, not kept with its cache dropped"
    );
    let removed_types = parse_xml(&m1_3_content(
        &removed_zip,
        &removed_out,
        "[Content_Types].xml",
    ));
    let removed_overrides: Vec<&str> = removed_types
        .all("Override")
        .map(|o| o.attribute("PartName").unwrap())
        .collect();
    assert_eq!(
        removed_overrides,
        ["/xl/workbook.xml", "/xl/worksheets/sheet1.xml"],
        "the chain's override is gone"
    );
    assert!(
        removed_types
            .all("Override")
            .all(|o| o.attribute("ContentType") != Some(CALC_CHAIN_TYPE))
    );
    let removed_relationships = parse_xml(&m1_3_content(
        &removed_zip,
        &removed_out,
        "xl/_rels/workbook.xml.rels",
    ));
    let removed_items: Vec<(&str, &str)> = removed_relationships
        .all("Relationship")
        .map(|r| (r.attribute("Id").unwrap(), r.attribute("Type").unwrap()))
        .collect();
    assert_eq!(
        removed_items,
        [("rId1", WORKSHEET_RELATIONSHIP)],
        "the chain's relationship is gone"
    );
    let removed_sheet = m1_3_content(&removed_zip, &removed_out, "xl/worksheets/sheet1.xml");
    let removed_root = parse_xml(&removed_sheet);
    let removed_cells: Vec<String> = m1_3_sheet_cells(&removed_root)
        .iter()
        .map(|(_, c)| String::from_utf8(c.bytes(&removed_sheet).to_vec()).unwrap())
        .collect();
    assert_eq!(removed_cells, [r#"<c r="A1"><v>1</v></c>"#]);
    assert_eq!(
        removed_root.child("dimension").unwrap().attribute("ref"),
        Some("A1")
    );
    let mut reload_ids: IdAllocator<SheetId> = IdAllocator::new();
    let reloaded_removed = m1_2_load(&removed_out, &mut reload_ids).expect("it loads");
    assert_eq!(m1_3_cells(reloaded_removed.model()), m1_3_cells(&removed));

    // shaped.xlsx: a test-built package to whose parts the test adds a byte order mark, and a
    // sheet with an empty sheetData. Both sheets are edited.
    let shaped = m1_3_shaped();
    let shaped_zip = parse_zip(&shaped);
    for name in [
        "[Content_Types].xml",
        "xl/workbook.xml",
        "xl/_rels/workbook.xml.rels",
        "xl/worksheets/sheet1.xml",
        "xl/sharedStrings.xml",
    ] {
        assert!(
            m1_3_content(&shaped_zip, &shaped, name).starts_with(&[0xEF, 0xBB, 0xBF]),
            "the source's {name} starts with a byte order mark"
        );
    }
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let loaded_shaped = m1_2_load(&shaped, &mut ids).expect("shaped.xlsx loads");
    let (shaped_sheet, empty_sheet) = (
        loaded_shaped.model().sheets()[0].id(),
        loaded_shaped.model().sheets()[1].id(),
    );
    let shaped_model = m1_3_edit(
        loaded_shaped.model(),
        vec![
            set(shaped_sheet, 1, 1, number(3.0)),
            set(shaped_sheet, 2, 2, formula("A2+1")),
            set(shaped_sheet, 3, 1, text(units("added"))),
            set(empty_sheet, 2, 3, number(5.0)),
            set(empty_sheet, 4, 4, formula("C2*2")),
        ],
    );
    let (shaped_out, shaped_receipt) = m1_3_save(&loaded_shaped.snapshot(&shaped_model));
    assert_eq!(
        (
            shaped_receipt.caches_dropped,
            shaped_receipt.sheets_rewritten_for_caches
        ),
        (1, 0),
        "A2's cache"
    );
    let shaped_out_zip = parse_zip(&shaped_out);
    assert_eq!(
        names(&shaped_out_zip),
        [
            "[Content_Types].xml",
            "_rels/.rels",
            "xl/workbook.xml",
            "xl/_rels/workbook.xml.rels",
            "xl/worksheets/sheet1.xml",
            "xl/worksheets/sheet2.xml",
            "xl/sharedStrings.xml"
        ],
        "formulas were added, so the chain is gone"
    );
    // Each rewritten part keeps its byte order mark and its declaration, and each but the shared
    // strings, whose counts change, its document element's start tag. The workbook part is not
    // rewritten (it is compared byte for byte below).
    for name in [
        "[Content_Types].xml",
        "xl/_rels/workbook.xml.rels",
        "xl/worksheets/sheet1.xml",
        "xl/sharedStrings.xml",
    ] {
        let (source, saved) = (
            m1_3_content(&shaped_zip, &shaped, name),
            m1_3_content(&shaped_out_zip, &shaped_out, name),
        );
        assert_ne!(saved, source, "{name} is rewritten");
        let root = parse_xml(&source).start_tag;
        let end = if name == "xl/sharedStrings.xml" {
            root.0
        } else {
            root.1
        };
        assert_eq!(
            &saved[..end],
            &source[..end],
            "{name}: the byte order mark, the declaration and the root start tag"
        );
    }
    let (source_book, saved_book) = (
        m1_3_content(&shaped_zip, &shaped, "xl/workbook.xml"),
        m1_3_content(&shaped_out_zip, &shaped_out, "xl/workbook.xml"),
    );
    assert_eq!(
        saved_book, source_book,
        "the workbook part is kept byte for byte"
    );
    assert_eq!(
        parse_xml(&saved_book).child("calcPr").unwrap().attributes,
        [("calcId".to_owned(), "191029".to_owned())]
    );
    let shaped_types = parse_xml(&m1_3_content(
        &shaped_out_zip,
        &shaped_out,
        "[Content_Types].xml",
    ));
    assert!(
        shaped_types
            .all("Override")
            .all(|o| o.attribute("PartName") != Some("/xl/calcChain.xml")),
        "the chain's override is gone"
    );
    let shaped_relationships = parse_xml(&m1_3_content(
        &shaped_out_zip,
        &shaped_out,
        "xl/_rels/workbook.xml.rels",
    ));
    assert_eq!(
        shaped_relationships
            .all("Relationship")
            .map(|r| r.attribute("Id").unwrap())
            .collect::<Vec<_>>(),
        ["rId1", "rId2", "rId3"],
        "the chain's relationship is gone"
    );
    let shaped_strings = parse_xml(&m1_3_content(
        &shaped_out_zip,
        &shaped_out,
        "xl/sharedStrings.xml",
    ));
    assert_eq!(
        shaped_strings
            .all("si")
            .map(|si| si.text())
            .collect::<Vec<_>>(),
        ["label", "added"]
    );
    assert_eq!(
        (
            shaped_strings.attribute("count"),
            shaped_strings.attribute("uniqueCount")
        ),
        (Some("2"), Some("2"))
    );

    // The sheet "Shaped": rows keep x14ac:dyDescent and lose spans, every element outside
    // sheetData but the dimension keeps its bytes (the xr:uid attributes among them).
    let (source_shaped, saved_shaped) = (
        m1_3_content(&shaped_zip, &shaped, "xl/worksheets/sheet1.xml"),
        m1_3_content(&shaped_out_zip, &shaped_out, "xl/worksheets/sheet1.xml"),
    );
    let (shaped_before, shaped_after) = (parse_xml(&source_shaped), parse_xml(&saved_shaped));
    assert_eq!(
        shaped_after.attribute("xr:uid"),
        Some("{6A1B2C3D-0000-4000-8000-000000000001}")
    );
    assert_eq!(
        outside(&shaped_after, &saved_shaped),
        outside(&shaped_before, &source_shaped),
        "every element outside sheetData but the dimension keeps its bytes"
    );
    assert_eq!(
        shaped_after.child("dimension").unwrap().attribute("ref"),
        Some("A1:B3")
    );
    let shaped_rows: Vec<String> = shaped_after
        .child("sheetData")
        .unwrap()
        .all("row")
        .map(|row| String::from_utf8(row.bytes(&saved_shaped).to_vec()).unwrap())
        .collect();
    assert_eq!(
        shaped_rows,
        [
            r#"<row r="1" x14ac:dyDescent="0.3"><c r="A1"><v>3</v></c><c r="B1" t="s"><v>0</v></c></row>"#,
            r#"<row r="2" x14ac:dyDescent="0.3"><c r="A2"><f>A1*2</f></c><c r="B2"><f>A2+1</f></c></row>"#,
            r#"<row r="3"><c r="A3" t="s"><v>1</v></c></row>"#,
        ]
    );

    // The sheet whose sheetData was empty.
    let saved_empty = m1_3_content(&shaped_out_zip, &shaped_out, "xl/worksheets/sheet2.xml");
    let empty_root = parse_xml(&saved_empty);
    assert_eq!(
        String::from_utf8(
            empty_root
                .child("sheetData")
                .unwrap()
                .bytes(&saved_empty)
                .to_vec()
        )
        .unwrap(),
        r#"<sheetData><row r="2"><c r="C2"><v>5</v></c></row><row r="4"><c r="D4"><f>C2*2</f></c></row></sheetData>"#
    );
    assert_eq!(
        empty_root.child("dimension").unwrap().attribute("ref"),
        Some("C2:D4")
    );
    let mut reload_ids: IdAllocator<SheetId> = IdAllocator::new();
    let reloaded_shaped = m1_2_load(&shaped_out, &mut reload_ids).expect("it loads");
    assert_eq!(
        m1_3_sheets(reloaded_shaped.model()),
        m1_3_sheets(&shaped_model)
    );
    assert_eq!(
        m1_3_cells(reloaded_shaped.model()),
        m1_3_cells(&shaped_model),
        "the saved shaped.xlsx holds the edited revision"
    );

    // extlst.xlsx: a row and the shared strings table that each end in an extLst. CT_Row is c*
    // then extLst?, and CT_Sst si* then extLst?, so the cells added to the row go before its
    // extLst and the appended item before the table's.
    let ext = r#"<extLst><ext uri="{00000000-0000-0000-0000-000000000001}"/></extLst>"#;
    let extlst = m1_3_package(
        r#"<sheets><sheet name="Ext" sheetId="1" r:id="rId1"/></sheets>"#,
        &[
            ("rId1", WORKSHEET_RELATIONSHIP, "worksheets/sheet1.xml"),
            ("rId2", SHARED_STRINGS_RELATIONSHIP, "sharedStrings.xml"),
        ],
        &[
            (
                "xl/worksheets/sheet1.xml",
                Some(WORKSHEET_TYPE),
                m1_3_worksheet(&format!(
                    r#"<dimension ref="A1"/><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c>{ext}</row></sheetData>"#
                )),
            ),
            (
                "xl/sharedStrings.xml",
                Some(SHARED_STRINGS_TYPE),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><sst xmlns="{SPREADSHEET_MAIN}" count="1" uniqueCount="1"><si><t>a</t></si>{ext}</sst>"#
                ),
            ),
        ],
    );
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let loaded_ext = m1_2_load(&extlst, &mut ids).expect("extlst.xlsx loads");
    let ext_sheet = loaded_ext.model().sheets()[0].id();
    let ext_model = m1_3_edit(
        loaded_ext.model(),
        vec![
            set(ext_sheet, 1, 2, text(units("b"))),
            set(ext_sheet, 1, 3, formula("A1&B1")),
        ],
    );
    let (ext_out, _) = m1_3_save(&loaded_ext.snapshot(&ext_model));
    let ext_zip = parse_zip(&ext_out);
    let ext_xml = m1_3_content(&ext_zip, &ext_out, "xl/worksheets/sheet1.xml");
    let ext_root = parse_xml(&ext_xml);
    let ext_row = ext_root
        .child("sheetData")
        .and_then(|data| data.child("row"))
        .expect("row 1");
    let row_children: Vec<(String, Option<String>)> = ext_row
        .elements()
        .map(|e| (e.name.clone(), e.attribute("r").map(str::to_owned)))
        .collect();
    assert_eq!(
        row_children,
        [
            ("c".to_owned(), Some("A1".to_owned())),
            ("c".to_owned(), Some("B1".to_owned())),
            ("c".to_owned(), Some("C1".to_owned())),
            ("extLst".to_owned(), None),
        ],
        "the cells added to row 1 come before its extLst"
    );
    assert_eq!(
        ext_row.child("extLst").map(|e| e.bytes(&ext_xml)),
        Some(ext.as_bytes()),
        "the row's extLst keeps its bytes"
    );
    let ext_strings_xml = m1_3_content(&ext_zip, &ext_out, "xl/sharedStrings.xml");
    let ext_strings = parse_xml(&ext_strings_xml);
    let sst_children: Vec<(String, String)> = ext_strings
        .elements()
        .map(|e| (e.name.clone(), e.text()))
        .collect();
    assert_eq!(
        sst_children,
        [
            ("si".to_owned(), "a".to_owned()),
            ("si".to_owned(), "b".to_owned()),
            ("extLst".to_owned(), String::new()),
        ],
        "the appended item comes before the table's extLst"
    );
    assert_eq!(
        ext_strings
            .child("extLst")
            .map(|e| e.bytes(&ext_strings_xml)),
        Some(ext.as_bytes()),
        "the table's extLst keeps its bytes"
    );
    let mut reload_ids: IdAllocator<SheetId> = IdAllocator::new();
    let reloaded_ext = m1_2_load(&ext_out, &mut reload_ids).expect("it loads");
    assert_eq!(m1_3_cells(reloaded_ext.model()), m1_3_cells(&ext_model));

    note!(
        "ACCEPT-NOTE: {CASE}: cells.xlsx edited at revision 1 (formula SUM(A6:A10)*2 into the absent J3, a formula in a new row 11, texts, a value, a formula, a shared-group member, a logical and an error set, B1:C1 and C2 cleared) saved to {} bytes: the case's own reader finds J3 as <c r=\"J3\"><f>{new_formula}</f></c> with no <v>, rows 1-12 and 14 in order, the dimension A1:K14 equal to the cells' area, 2 strings appended to 4 held ones, the workbook part byte for byte (no fullCalcOnLoad), {kept_formulas} other formulas with their <f> kept and their <v> dropped, {kept_values} other cells byte for byte, and {} untouched parts byte for byte ({:?}); reloaded, the model equals the edited revision",
        out.len(),
        untouched.len(),
        untouched
    );
    note!(
        "ACCEPT-NOTE: {CASE}: cells.xlsx: the receipt counts {} caches dropped and {} sheet rewritten only for its caches, equal to the {dropped} formula cells with a <v> the edit did not set or clear that the case counts in the source parts",
        receipt.caches_dropped,
        receipt.sheets_rewritten_for_caches
    );
    note!(
        "ACCEPT-NOTE: {CASE}: numbers.xlsx (no shared strings, a calculation chain): a value edit keeps the chain, rels and content types byte for byte (1 cache dropped); a text and a formula added write a deflated xl/sharedStrings.xml with rId3 and its override, drop xl/calcChain.xml with its relationship and override, and add calcPr before extLst; the formula A2 cleared drops the chain, its relationship and its override with nothing added (0 caches dropped); prepare_edit refused SUM(A6:A10 and =A1 (FormulaSyntax), an 8,193-unit formula (FormulaTooLong), a stale base and a sheet removal (Model)"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: the deep chain 1+1+...+1 (4,095 additions, {} UTF-16 code units) set in J3 of cells.xlsx was prepared by prepare_edit, checked again and saved by prepare_save and write ({} bytes), read back by the case's own reader as J3's <f> with no <v>, and reloaded to the edited revision",
        deep.len(),
        deep_out.len()
    );
    note!(
        "ACCEPT-NOTE: {CASE}: extlst.xlsx (row 1 and the sst each end in an extLst): B1 = \"b\" and C1 = A1&B1 added; the case's own reader finds row 1's children c A1, c B1, c C1, extLst and the sst's si \"a\", si \"b\", extLst, both extLst elements byte for byte; reloaded, the model equals the edited revision"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: shaped.xlsx (test-built; a byte order mark added to the content types, workbook, its rels, the sheet Shaped and the shared strings; mc:Ignorable, xr:uid and x14ac:dyDescent on the sheet; a second sheet with <sheetData/>): both sheets edited and saved to {} bytes; each rewritten part keeps its mark, declaration and root start tag, the workbook part is kept byte for byte (no fullCalcOnLoad), rows keep x14ac:dyDescent and lose spans, the empty sheetData holds rows 2 and 4, the chain is dropped; reloaded, the model equals the edited revision",
        shaped_out.len()
    );
}

// ---------------------------------------------------------------------------------------------
// Case cdoc_m1_3__save_refusal.

/// Checks a refused save: the route's sink received no byte (observed), and the snapshot held the
/// caller's revision itself. That the caller's revision keeps its number and cells holds by type
/// (the snapshot and the save hold an immutable `Arc<ModelRoot>`); the comparisons below restate
/// it rather than measure it.
fn m1_3_refused(
    snapshot: &DocumentSnapshot,
    policy: &SavePolicy,
    model: &Arc<ModelRoot>,
    cells: &[Vec<(u32, u32, CellInput)>],
) -> SaveRefusal {
    let revision = model.revision();
    let (result, sink) = m1_3_route(snapshot, policy);
    let refusal = match result {
        Ok(_) => panic!("the save is refused, but it wrote {} bytes", sink.len()),
        Err(refusal) => refusal,
    };
    assert!(sink.is_empty(), "{refusal}: no byte reached the sink");
    assert!(Arc::ptr_eq(snapshot.model().expect("a model"), model));
    assert_eq!(
        model.revision(),
        revision,
        "{refusal}: the revision is the one held"
    );
    assert_eq!(
        m1_3_cells(model),
        cells,
        "{refusal}: the revision's cells are unchanged"
    );
    refusal
}

/// Every byte of a source, read through its own `read_at`.
fn m1_3_source_bytes(source: &dyn PackageRead) -> Vec<u8> {
    let mut bytes = vec![0; source.size() as usize];
    source
        .read_at(0, &mut bytes)
        .expect("the test's source reads");
    bytes
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cdoc_m1_3__save_refusal() {
    const CASE: &str = "cdoc_m1_3__save_refusal";
    let default = SavePolicy::default();

    // A part the save rewrites changed in the source after opening.
    let changing = Arc::new(ChangingSource::new(M1_2_CELLS.to_vec()));
    let source: Arc<dyn PackageRead> = changing.clone();
    let document = opened(source, &OpenPolicy::default());
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let loaded = calc_doc::load(&document, &mut ids).expect("cells.xlsx loads");
    let sheet = loaded.model().sheets()[0].id();
    let model = m1_3_edit(
        loaded.model(),
        vec![set(sheet, 3, 10, formula("SUM(A6:A10)*2"))],
    );
    let cells = m1_3_cells(&model);
    let input = parse_zip(M1_2_CELLS);
    let payload = input.entry("xl/worksheets/sheet2.xml").payload_start + 10;
    changing.change(|bytes| bytes[payload] ^= 0x40);
    let snapshot = loaded.snapshot(&model);
    let part = m1_3_refused(&snapshot, &default, &model, &cells);
    assert!(
        matches!(&part, SaveRefusal::Part { part, .. } if part == "xl/worksheets/sheet2.xml"),
        "{part:?}"
    );
    // The source image, read through the test's own source: the refused save changed no byte of
    // it but the one the test changed.
    let mut changed_image = M1_2_CELLS.to_vec();
    changed_image[payload] ^= 0x40;
    assert!(
        m1_3_source_bytes(&*changing) == changed_image,
        "the source holds cells.xlsx with only the test's change"
    );
    changing.change(|bytes| bytes[payload] ^= 0x40);
    assert!(
        m1_3_source_bytes(&*changing) == M1_2_CELLS,
        "restored, the source holds cells.xlsx byte for byte"
    );
    // The refused save left nothing behind in the snapshot: with the byte restored it saves.
    let (retried, _) = m1_3_save(&snapshot);
    // Loaded with the same allocator, so its sheets have other IDs than `loaded`'s.
    let fresh = m1_2_load(M1_2_CELLS, &mut ids).expect("cells.xlsx loads");
    let fresh_sheet = fresh.model().sheets()[0].id();
    let fresh_model = m1_3_edit(
        fresh.model(),
        vec![set(fresh_sheet, 3, 10, formula("SUM(A6:A10)*2"))],
    );
    let (unrefused, _) = m1_3_save(&fresh.snapshot(&fresh_model));
    assert!(
        retried == unrefused,
        "the retried save equals a save never refused"
    );

    // A model that is not a revision of the loaded workbook: one another load of the same file
    // made with the same allocator (other sheet IDs), and one a load with an allocator of its own
    // made, whose sheets equal `loaded`'s in ID, key, name and visibility, so only the lineage each
    // load gave its model tells it apart.
    let foreign = m1_3_refused(
        &loaded.snapshot(&fresh_model),
        &default,
        &fresh_model,
        &m1_3_cells(&fresh_model),
    );
    assert_eq!(foreign, SaveRefusal::ForeignModel);
    let mut own_ids: IdAllocator<SheetId> = IdAllocator::new();
    let twin = m1_2_load(M1_2_CELLS, &mut own_ids).expect("cells.xlsx loads");
    assert_eq!(
        twin.model().sheets(),
        loaded.model().sheets(),
        "the twin's sheets equal the loaded ones, IDs included"
    );
    let twin_sheet = twin.model().sheets()[0].id();
    let twin_model = m1_3_edit(
        twin.model(),
        vec![set(twin_sheet, 3, 10, formula("SUM(A6:A10)*3"))],
    );
    let twin_refusal = m1_3_refused(
        &loaded.snapshot(&twin_model),
        &default,
        &twin_model,
        &m1_3_cells(&twin_model),
    );
    assert_eq!(twin_refusal, SaveRefusal::ForeignModel);
    let (twin_out, _) = m1_3_save(&twin.snapshot(&twin_model));
    assert!(
        !twin_out.is_empty(),
        "the twin's own snapshot of the same revision saves"
    );

    // A package past the policy's bound, then within the default one.
    let small = SavePolicy {
        format: calc_doc::SaveFormat::Xml,
        max_package_bytes: 1_000,
    };
    let too_large = m1_3_refused(
        &fresh.snapshot(&fresh_model),
        &small,
        &fresh_model,
        &m1_3_cells(&fresh_model),
    );
    assert!(
        matches!(
            too_large,
            SaveRefusal::PackageTooLarge {
                bytes,
                entries: 8,
                limit: 1_000
            } if bytes == unrefused.len() as u64
        ),
        "{too_large:?}"
    );

    // Revisions made by ModelRoot::apply, not prepare_edit: a formula of 9,001 code units, which
    // calc_doc::load refuses on reopen, and an unclosed SUM( are refused by the save as
    // prepare_edit refuses the same batch.
    let applied = |text: &str| {
        let batch = OpBatch {
            base: fresh.model().revision(),
            operations: vec![set(fresh_sheet, 3, 10, formula(text))],
        };
        let edit = prepare_edit(fresh.model(), &batch).expect_err("prepare_edit refuses it");
        let model = fresh.model().apply(&batch).expect("the model applies it");
        (edit, model)
    };
    let over = "1+".repeat(4_500) + "1";
    assert_eq!(over.len(), 9_001);
    let (over_edit, over_model) = applied(&over);
    assert_eq!(
        over_edit,
        EditError::FormulaTooLong {
            operation: 0,
            units: 9_001
        }
    );
    let over_refusal = m1_3_refused(
        &fresh.snapshot(&over_model),
        &default,
        &over_model,
        &m1_3_cells(&over_model),
    );
    assert_eq!(
        over_refusal,
        SaveRefusal::FormulaTooLong {
            part: "xl/worksheets/sheet2.xml".to_owned(),
            row: 3,
            column: 10,
            units: 9_001
        }
    );
    let (open_edit, open_model) = applied("SUM(");
    let EditError::FormulaSyntax {
        operation: 0,
        message: edit_message,
    } = open_edit
    else {
        panic!("prepare_edit reads SUM( with a diagnostic, but: {open_edit:?}");
    };
    let open_refusal = m1_3_refused(
        &fresh.snapshot(&open_model),
        &default,
        &open_model,
        &m1_3_cells(&open_model),
    );
    assert_eq!(
        open_refusal,
        SaveRefusal::FormulaSyntax {
            part: "xl/worksheets/sheet2.xml".to_owned(),
            row: 3,
            column: 10,
            message: edit_message.clone()
        }
    );

    // An entry the save copies untouched, xl/styles.xml, changed in the source after the package
    // was opened: prepare_save, which does not read it, plans the save, and write refuses it
    // before the sink receives a byte. Then a source cut short: the same.
    let copying = Arc::new(ChangingSource::new(M1_2_CELLS.to_vec()));
    let copying_source: Arc<dyn PackageRead> = copying.clone();
    let copying_document = opened(copying_source, &OpenPolicy::default());
    let mut copying_ids: IdAllocator<SheetId> = IdAllocator::new();
    let copying_loaded = calc_doc::load(&copying_document, &mut copying_ids).expect("it loads");
    let copying_sheet = copying_loaded.model().sheets()[0].id();
    let copying_model = m1_3_edit(
        copying_loaded.model(),
        vec![set(copying_sheet, 3, 10, formula("SUM(A6:A10)*2"))],
    );
    let styles = input.entry("xl/styles.xml");
    let styles_byte = styles.payload_start + 10;
    copying.change(|bytes| bytes[styles_byte] ^= 0x40);
    let mut copied_sink = Vec::new();
    let plan = calc_doc::prepare_save(&copying_loaded.snapshot(&copying_model), &default)
        .expect("the save is planned without reading xl/styles.xml");
    let copied_error =
        calc_doc::write(plan, &mut copied_sink).expect_err("a copied entry changed in the source");
    assert_eq!(
        copied_error,
        WriteError::SourceChanged {
            entry: "xl/styles.xml".to_owned()
        }
    );
    assert!(
        copied_sink.is_empty(),
        "{copied_error}: no byte reached the sink, but {} did",
        copied_sink.len()
    );
    let mut copied_image = M1_2_CELLS.to_vec();
    copied_image[styles_byte] ^= 0x40;
    assert!(
        m1_3_source_bytes(&*copying) == copied_image,
        "the source holds cells.xlsx with only the test's change"
    );
    // A package whose last stored entry, which nothing reads and the save copies, is cut short in
    // the source after opening.
    let tail = m1_3_package(
        r#"<sheets><sheet name="One" sheetId="1" r:id="rId1"/></sheets>"#,
        &[("rId1", WORKSHEET_RELATIONSHIP, "worksheets/sheet1.xml")],
        &[
            (
                "xl/worksheets/sheet1.xml",
                Some(WORKSHEET_TYPE),
                m1_3_worksheet(r#"<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>"#),
            ),
            (
                "docProps/extra.xml",
                None,
                "<extra>copied</extra>".to_owned(),
            ),
        ],
    );
    let tail_zip = parse_zip(&tail);
    let extra = tail_zip.entry("docProps/extra.xml");
    assert_eq!(
        names_in_stored_order(&tail_zip).last().map(String::as_str),
        Some("docProps/extra.xml")
    );
    let cut = Arc::new(ChangingSource::new(tail.clone()));
    let cut_source: Arc<dyn PackageRead> = cut.clone();
    let cut_document = opened(cut_source, &OpenPolicy::default());
    let mut cut_ids: IdAllocator<SheetId> = IdAllocator::new();
    let cut_loaded = calc_doc::load(&cut_document, &mut cut_ids).expect("it loads");
    let cut_sheet = cut_loaded.model().sheets()[0].id();
    let cut_model = m1_3_edit(cut_loaded.model(), vec![set(cut_sheet, 1, 1, number(2.0))]);
    let cut_at = extra.payload_start + 3;
    cut.change(|bytes| bytes.truncate(cut_at));
    let mut short_sink = Vec::new();
    let plan = calc_doc::prepare_save(&cut_loaded.snapshot(&cut_model), &default)
        .expect("the save is planned without reading docProps/extra.xml");
    let short_error = calc_doc::write(plan, &mut short_sink).expect_err("a source cut short");
    assert!(
        matches!(short_error, WriteError::Source(_)),
        "{short_error:?}"
    );
    assert!(
        short_sink.is_empty(),
        "{short_error}: no byte reached the sink, but {} did",
        short_sink.len()
    );
    assert!(
        m1_3_source_bytes(&*cut) == tail[..cut_at],
        "the source holds the package's first {cut_at} bytes"
    );

    // A table of A1:B4 with a header row and a totals row: a change in either is refused, since the
    // table part names the header row's cells and the totals row's functions; a change in its data
    // rows or beside it is saved, and the table part and the sheet's rels keep their bytes.
    let table = m1_3_package(
        r#"<sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets>"#,
        &[("rId1", WORKSHEET_RELATIONSHIP, "worksheets/sheet1.xml")],
        &[
            (
                "xl/worksheets/sheet1.xml",
                Some(WORKSHEET_TYPE),
                m1_3_worksheet(
                    r#"<dimension ref="A1:B4"/><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Name</t></is></c><c r="B1" t="inlineStr"><is><t>Value</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>x</t></is></c><c r="B2"><v>1</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>y</t></is></c><c r="B3"><v>2</v></c></row><row r="4"><c r="A4" t="inlineStr"><is><t>Total</t></is></c><c r="B4"><f>SUBTOTAL(109,B2:B3)</f><v>3</v></c></row></sheetData><tableParts count="1"><tablePart r:id="rId1"/></tableParts>"#,
                ),
            ),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                None,
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{}"><Relationship Id="rId1" Type="{TABLE_RELATIONSHIP}" Target="../tables/table1.xml"/></Relationships>"#,
                    calc_doc::RELATIONSHIPS_NAMESPACE
                ),
            ),
            (
                "xl/tables/table1.xml",
                Some(TABLE_TYPE),
                format!(
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><table xmlns="{SPREADSHEET_MAIN}" id="1" name="Table1" displayName="Table1" ref="A1:B4" totalsRowCount="1"><autoFilter ref="A1:B3"/><tableColumns count="2"><tableColumn id="1" name="Name" totalsRowLabel="Total"/><tableColumn id="2" name="Value" totalsRowFunction="sum"/></tableColumns></table>"#
                ),
            ),
        ],
    );
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let loaded_table = m1_2_load(&table, &mut ids).expect("the table package loads");
    let data = loaded_table.model().sheets()[0].id();
    let in_header = m1_3_edit(
        loaded_table.model(),
        vec![set(data, 1, 2, text(units("Amount")))],
    );
    let edit_in_table = m1_3_refused(
        &loaded_table.snapshot(&in_header),
        &default,
        &in_header,
        &m1_3_cells(&in_header),
    );
    assert_eq!(
        edit_in_table,
        SaveRefusal::EditInTable {
            part: "xl/worksheets/sheet1.xml".to_owned(),
            table: "xl/tables/table1.xml".to_owned(),
            row: 1,
            column: 2
        }
    );
    let in_totals = m1_3_edit(loaded_table.model(), vec![set(data, 4, 2, number(0.0))]);
    assert_eq!(
        m1_3_refused(
            &loaded_table.snapshot(&in_totals),
            &default,
            &in_totals,
            &m1_3_cells(&in_totals),
        ),
        SaveRefusal::EditInTable {
            part: "xl/worksheets/sheet1.xml".to_owned(),
            table: "xl/tables/table1.xml".to_owned(),
            row: 4,
            column: 2
        }
    );
    let table_zip = parse_zip(&table);
    for (what, operations) in [
        ("in the data rows", vec![set(data, 2, 2, number(10.0))]),
        ("beside the table", vec![set(data, 5, 4, number(3.0))]),
    ] {
        let edited = m1_3_edit(loaded_table.model(), operations);
        let (edited_out, edited_receipt) = m1_3_save(&loaded_table.snapshot(&edited));
        assert_eq!(edited_receipt.caches_dropped, 1, "{what}: B4's cache");
        let edited_zip = parse_zip(&edited_out);
        for name in [
            "xl/tables/table1.xml",
            "xl/worksheets/_rels/sheet1.xml.rels",
        ] {
            assert_eq!(
                preserved_fields(edited_zip.entry(name), &edited_out),
                preserved_fields(table_zip.entry(name), &table),
                "{what}: {name}"
            );
        }
        let mut reload_ids: IdAllocator<SheetId> = IdAllocator::new();
        let reloaded = m1_2_load(&edited_out, &mut reload_ids).expect("it loads");
        assert_eq!(
            m1_3_cells(reloaded.model()),
            m1_3_cells(&edited),
            "{what}: the saved package holds the edited revision"
        );
    }

    // Two sheets that are one part.
    let shared = m1_3_package(
        r#"<sheets><sheet name="One" sheetId="1" r:id="rId1"/><sheet name="Two" sheetId="2" r:id="rId2"/></sheets>"#,
        &[
            ("rId1", WORKSHEET_RELATIONSHIP, "worksheets/sheet1.xml"),
            ("rId2", WORKSHEET_RELATIONSHIP, "worksheets/sheet1.xml"),
        ],
        &[(
            "xl/worksheets/sheet1.xml",
            Some(WORKSHEET_TYPE),
            m1_3_worksheet(r#"<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>"#),
        )],
    );
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let loaded_shared = m1_2_load(&shared, &mut ids).expect("the package loads");
    let one = loaded_shared.model().sheets()[0].id();
    let shared_model = m1_3_edit(loaded_shared.model(), vec![set(one, 1, 1, number(2.0))]);
    assert_eq!(
        m1_3_refused(
            &loaded_shared.snapshot(&shared_model),
            &default,
            &shared_model,
            &m1_3_cells(&shared_model)
        ),
        SaveRefusal::SharedSheetPart {
            part: "xl/worksheets/sheet1.xml".to_owned()
        }
    );

    // A workbook part with two calcPr elements, which the loader does not read: a cell edit asks
    // for no full calculation (dc-35p), so the save copies the part as written instead of
    // refusing to rewrite it.
    let two_calc = m1_3_package(
        r#"<sheets><sheet name="One" sheetId="1" r:id="rId1"/></sheets><calcPr calcId="1"/><calcPr calcId="2"/>"#,
        &[("rId1", WORKSHEET_RELATIONSHIP, "worksheets/sheet1.xml")],
        &[(
            "xl/worksheets/sheet1.xml",
            Some(WORKSHEET_TYPE),
            m1_3_worksheet(r#"<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>"#),
        )],
    );
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let loaded_two = m1_2_load(&two_calc, &mut ids).expect("the package loads");
    let first = loaded_two.model().sheets()[0].id();
    let two_model = m1_3_edit(loaded_two.model(), vec![set(first, 1, 1, number(2.0))]);
    let (two_saved, _) = m1_3_save(&loaded_two.snapshot(&two_model));
    assert_eq!(
        m1_3_content(&parse_zip(&two_saved), &two_saved, "xl/workbook.xml"),
        m1_3_content(&parse_zip(&two_calc), &two_calc, "xl/workbook.xml"),
        "the workbook part with two calcPr elements is copied as written"
    );

    // Since dc-ndi, array formulas, cell metadata (cm) and rich values (vm) load (the d1_* test
    // files cover them); an array formula whose cm resolves to no dynamic-array properties (here,
    // a workbook without a metadata part) is still refused at load.
    for (what, cell_xml, expected) in [(
        "an array formula whose cm resolves to nothing",
        r#"<c r="A1" cm="1"><f t="array" ref="A1">1+1</f><v>2</v></c>"#,
        CellError::UnsupportedMetadata,
    )] {
        let package = m1_3_package(
            r#"<sheets><sheet name="One" sheetId="1" r:id="rId1"/></sheets>"#,
            &[("rId1", WORKSHEET_RELATIONSHIP, "worksheets/sheet1.xml")],
            &[(
                "xl/worksheets/sheet1.xml",
                Some(WORKSHEET_TYPE),
                m1_3_worksheet(&format!(
                    r#"<sheetData><row r="1">{cell_xml}</row></sheetData>"#
                )),
            )],
        );
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        match m1_2_load(&package, &mut ids) {
            Ok(_) => panic!("{what}: the package loads, but it is refused at load"),
            Err(error) => assert_eq!(
                error,
                LoadError::Cell {
                    part: "xl/worksheets/sheet1.xml".to_owned(),
                    row: 1,
                    column: 1,
                    error: expected
                },
                "{what}"
            ),
        }
    }

    note!(
        "ACCEPT-NOTE: {CASE}: each refusal of prepare_save wrote no byte to the route's sink, and the snapshot held the caller's revision itself (its revision and cells held unchanged by type, an immutable Arc): {part}; {foreign} (another load's model, with other sheet IDs and with equal ones); {too_large}; {over_refusal} and {open_refusal}, each for a revision ModelRoot::apply made of a batch prepare_edit refuses; {edit_in_table}, and the same for the totals row B4; two sheets of one part; a workbook part with two calcPr elements, copied as written"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: write checks every entry it copies before its first byte: {copied_error} (a copied entry changed after opening, which prepare_save does not read) and {short_error} (a source cut short in a copied entry) each left the sink empty; the test's own sources then held the opened bytes with only the test's change"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: after the Part refusal the test's source held cells.xlsx with only the test's one-byte change, and restored, cells.xlsx byte for byte; the restored source saved the same snapshot to {} bytes equal to a save never refused; an edit in the table's data rows (B2) and one beside it (D5) each saved with the table part and its sheet's rels byte for byte and reloaded to the edited revision",
        retried.len()
    );
    note!(
        "ACCEPT-NOTE: {CASE}: an array formula whose cell metadata resolves to no dynamic-array properties is refused at load with LoadError::Cell (UnsupportedMetadata); arrays, cm and vm that resolve load and save (dc-ndi's d1_* tests)"
    );
}

// ---------------------------------------------------------------------------------------------
// cdoc-m2.1: the DNA view manifest and its locator names, saved inside the workbook and read back.
//
// The cases load the packages under `tests/fixtures/cdoc_m2_1/`, written by the committed
// generator beside them, and compare what the loaded model holds with that generator's
// `manifest.json`; packages for single rules (a locator that is a user's name, a manifest the XML
// reader refuses, a namespace prefix) are built in the cases, with their expectations beside them. They edit through calc_doc::prepare_edit (the layout edits decoded from the
// golden fixture skin_layout_v1.json among them), save through calc_doc::prepare_save and
// calc_doc::write, and read what was written with `cdoc_m1_1_support`'s ZIP reader and
// `cdoc_m1_3_support`'s DEFLATE decoder and XML parser, against expectations written here or
// taken from the generator's record: never with calc-doc's loader, except to reload the saved
// package. That Excel opens a saved package without a repair prompt and keeps the manifest and the
// hidden names through its own save is the owner's Excel step, which these cases do not show.

use calc_doc::LayoutWarning;
use calc_doc::model::{Block, LayoutError, LocatorName, Placement, SkinLayout, ViewKind};

const M2_1_NOTEBOOK: &[u8] = include_bytes!("fixtures/cdoc_m2_1/notebook.xlsx");
const M2_1_V1: &[u8] = include_bytes!("fixtures/cdoc_m2_1/manifest-v1.xlsx");
const M2_1_V2: &[u8] = include_bytes!("fixtures/cdoc_m2_1/manifest-v2.xlsx");
const M2_1_UNREADABLE: &[u8] = include_bytes!("fixtures/cdoc_m2_1/manifest-unreadable.xlsx");
const M2_1_MANIFEST: &str = include_str!("fixtures/cdoc_m2_1/manifest.json");

const DNA_NAMESPACE: &str = "urn:dnacalc:view-manifest";
const DATASTORE_NAMESPACE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/customXml";
const CUSTOM_XML_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml";
const CUSTOM_XML_PROPS_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps";
const CUSTOM_XML_PROPS_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.customXmlProperties+xml";

/// A 128-bit ID as a UUID is written, formatted here: 32 lowercase hex digits in groups of 8, 4,
/// 4, 4 and 12 joined by `-`.
fn m2_1_uuid(value: u128) -> String {
    let hex = format!("{value:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// The locator name of the block with this ID, as the bead's rule gives it, built here.
fn m2_1_locator(block: u128) -> String {
    format!("_dna.block.{block:032x}")
}

/// An area in A1 form, `B5` or `B5:B6`.
fn m2_1_area(area: GridArea) -> String {
    let (first, last) = (area.first(), area.last());
    let corner = |at: CellKey| format!("{}{}", m1_3_letters(at.column()), at.row());
    if first == last {
        corner(first)
    } else {
        format!("{}:{}", corner(first), corner(last))
    }
}

/// A model's skin layout, line by line, from its observable parts: each block (ID, kind, locator
/// name and anchor as its sheet's name and an A1 area), then each view's placements in its order
/// (ID, block, a tree placement's parent, a canvas placement's area as decimal numbers).
fn m2_1_layout(model: &ModelRoot) -> Vec<String> {
    let layout = model.layout();
    let mut lines = Vec::new();
    for block in layout.blocks() {
        let anchor = match block.anchor() {
            Some(area) => {
                let sheet = model
                    .sheet_by_id(area.sheet())
                    .expect("an anchor's sheet")
                    .name()
                    .text()
                    .to_string_lossy();
                format!("{sheet}!{}", m2_1_area(area))
            }
            None => "none".to_owned(),
        };
        lines.push(format!(
            "block {} {:?} {} {anchor}",
            m2_1_uuid(block.id().get()),
            block.kind(),
            block.locator().as_str()
        ));
    }
    for view in ViewKind::ALL {
        for placement in layout.view(view) {
            let mut line = format!(
                "{view:?} {} block {}",
                m2_1_uuid(placement.id().get()),
                m2_1_uuid(placement.block().get())
            );
            if view == ViewKind::Tree {
                let parent = placement
                    .parent()
                    .map_or_else(|| "none".to_owned(), |p| m2_1_uuid(p.get()));
                line.push_str(&format!(" parent {parent}"));
            }
            if let Some(area) = placement.canvas_area() {
                line.push_str(&format!(
                    " at {} {} {} {}",
                    area.x.get(),
                    area.y.get(),
                    area.width.get(),
                    area.height.get()
                ));
            }
            lines.push(line);
        }
    }
    lines
}

/// The layout lines the generator's record of a version 1 manifest states, with the anchors it
/// states.
fn m2_1_recorded_layout(record: &Value) -> Vec<String> {
    let text = |value: &Value, key: &str| value[key].as_str().expect(key).to_owned();
    let mut lines = Vec::new();
    for block in record["blocks"].as_array().expect("blocks") {
        let kind = match block["kind"].as_str().expect("a kind") {
            "prose" => "Prose",
            "calculation" => "Calculation",
            other => panic!("the kind {other}"),
        };
        let anchor = match &block["anchor"] {
            Value::Null => "none".to_owned(),
            anchor => format!("{}!{}", text(anchor, "sheet"), text(anchor, "ref")),
        };
        lines.push(format!(
            "block {} {kind} {} {anchor}",
            text(block, "id"),
            text(block, "locator")
        ));
    }
    for (key, view) in [
        ("notebook", "Notebook"),
        ("tree", "Tree"),
        ("canvas", "Canvas"),
        ("grid", "Grid"),
    ] {
        for placement in record[key].as_array().expect("a view") {
            let mut line = format!(
                "{view} {} block {}",
                text(placement, "id"),
                text(placement, "block")
            );
            if key == "tree" {
                let parent = placement["parent"].as_str().unwrap_or("none");
                line.push_str(&format!(" parent {parent}"));
            }
            if key == "canvas" {
                let number = |key: &str| text(placement, key).parse::<f64>().expect("a number");
                line.push_str(&format!(
                    " at {} {} {} {}",
                    number("x"),
                    number("y"),
                    number("width"),
                    number("height")
                ));
            }
            lines.push(line);
        }
    }
    lines
}

/// Loads a package with a fresh sheet ID allocator, refused loads failing the case.
fn m2_1_load(bytes: &[u8]) -> LoadedWorkbook {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    m1_2_load(bytes, &mut ids).unwrap_or_else(|error| panic!("the package loads, but: {error}"))
}

/// One sheet's name and its occupied cells, as (row, column, input) in row-major order.
type SheetCells = (String, Vec<(u32, u32, CellInput)>);

/// Every cell the generator's record states, by sheet name, as (row, column, input).
fn m2_1_recorded_cells(record: &Value) -> Vec<SheetCells> {
    record["sheets"]
        .as_array()
        .expect("sheets")
        .iter()
        .map(|sheet| {
            let cells = sheet["cells"]
                .as_array()
                .expect("cells")
                .iter()
                .map(|cell| {
                    let (row, column) = a1(cell["ref"].as_str().expect("a ref"));
                    let input = match cell["kind"].as_str().expect("a kind") {
                        "text" => text(units_of_hex(cell["units"].as_str().expect("units"))),
                        "number" => CellInput::Number(bits(
                            u64::from_str_radix(cell["bits"].as_str().expect("bits"), 16)
                                .expect("hex bits"),
                        )),
                        "formula" => formula(cell["text"].as_str().expect("a formula")),
                        other => panic!("the cell kind {other}"),
                    };
                    (row, column, input)
                })
                .collect();
            (sheet["name"].as_str().expect("a name").to_owned(), cells)
        })
        .collect()
}

/// A model's cells by sheet name.
fn m2_1_cells(model: &ModelRoot) -> Vec<SheetCells> {
    model
        .sheets()
        .iter()
        .zip(m1_3_cells(model))
        .map(|(sheet, cells)| (sheet.name().text().to_string_lossy(), cells))
        .collect()
}

/// The stored bytes of an entry: its local header, payload and data descriptor.
fn m2_1_stored<'a>(zip: &Zip, package: &'a [u8], name: &str) -> &'a [u8] {
    let entry = zip.entry(name);
    &package[entry.local_offset as usize..entry.end()]
}

fn m2_1_names(zip: &Zip) -> Vec<String> {
    zip.entries.iter().map(|entry| entry.name.clone()).collect()
}

/// Whether `haystack` holds `needle` as a run of bytes.
fn m2_1_holds(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// One `definedName` element: its name, localSheetId, hidden, text and bytes.
type DefinedNameRecord = (String, Option<String>, Option<String>, String, Vec<u8>);

/// The workbook part's `definedName` elements: (name, localSheetId, hidden, text with its
/// `_xHHHH_` escapes decoded, the element's bytes).
fn m2_1_defined_names(workbook: &[u8]) -> Vec<DefinedNameRecord> {
    let root = parse_xml(workbook);
    root.child("definedNames")
        .map(|names| {
            names
                .all("definedName")
                .map(|name| {
                    (
                        name.attribute("name").expect("a name").to_owned(),
                        name.attribute("localSheetId").map(str::to_owned),
                        name.attribute("hidden").map(str::to_owned),
                        String::from_utf16(&xstring_units(&name.text())).expect("text"),
                        name.bytes(workbook).to_vec(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A layout edit.
fn m2_1_op(edit: SkinLayoutEdit) -> WorkbookOp {
    WorkbookOp::EditSkinLayout(edit)
}

/// What prepare_edit made of a batch on `model`, or why it refused it.
fn m2_1_try(model: &ModelRoot, operations: Vec<WorkbookOp>) -> Result<Arc<ModelRoot>, EditError> {
    prepare_edit(
        model,
        &OpBatch {
            base: model.revision(),
            operations,
        },
    )
    .map(|prepared| prepared.into_model())
}

/// A package built here: a workbook whose part holds `workbook` after its one sheet "Numbers"
/// (A1 = 1), with a DNA manifest `manifest` as customXml/item1.xml reached from the workbook part
/// when given (a second one as customXml/item2.xml when `second` is given).
fn m2_1_built(workbook: &str, manifest: Option<&str>, second: Option<&str>) -> Vec<u8> {
    let mut relationships = vec![("rId1", WORKSHEET_RELATIONSHIP, "worksheets/sheet1.xml")];
    let mut parts = vec![(
        "xl/worksheets/sheet1.xml",
        Some(WORKSHEET_TYPE),
        m1_3_worksheet(r#"<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>"#),
    )];
    if let Some(manifest) = manifest {
        relationships.push(("rId2", CUSTOM_XML_RELATIONSHIP, "../customXml/item1.xml"));
        parts.push(("customXml/item1.xml", None, manifest.to_owned()));
    }
    if let Some(second) = second {
        relationships.push(("rId3", CUSTOM_XML_RELATIONSHIP, "../customXml/item2.xml"));
        parts.push(("customXml/item2.xml", None, second.to_owned()));
    }
    m1_3_package(
        &format!(r#"<sheets><sheet name="Numbers" sheetId="1" r:id="rId1"/></sheets>{workbook}"#),
        &relationships,
        &parts,
    )
}

/// [`m2_1_built`] with custom XML items given as bytes, so an item need not be UTF-8: the `n`-th
/// of `items` is `customXml/item<n>.xml`, reached from the workbook part by `rId<n + 1>`, with no
/// item properties.
fn m2_1_built_bytes(workbook: &str, items: &[Vec<u8>]) -> Vec<u8> {
    let content_types = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="{}"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{WORKSHEET_TYPE}"/></Types>"#,
        calc_doc::CONTENT_TYPES_NAMESPACE
    );
    let workbook = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="{SPREADSHEET_MAIN}" xmlns:r="{RELATIONSHIP_ATTRIBUTES}"><sheets><sheet name="Numbers" sheetId="1" r:id="rId1"/></sheets>{workbook}</workbook>"#
    );
    let mut relationships = format!(
        r#"<Relationship Id="rId1" Type="{WORKSHEET_RELATIONSHIP}" Target="worksheets/sheet1.xml"/>"#
    );
    for n in 1..=items.len() {
        relationships.push_str(&format!(
            r#"<Relationship Id="rId{}" Type="{CUSTOM_XML_RELATIONSHIP}" Target="../customXml/item{n}.xml"/>"#,
            n + 1
        ));
    }
    let relationships = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="{}">{relationships}</Relationships>"#,
        calc_doc::RELATIONSHIPS_NAMESPACE
    );
    let sheet = m1_3_worksheet(r#"<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>"#);
    let mut entries = vec![
        RawEntry::stored("[Content_Types].xml", content_types.as_bytes()),
        RawEntry::stored("_rels/.rels", base_relationships().as_bytes()),
        RawEntry::stored("xl/workbook.xml", workbook.as_bytes()),
        RawEntry::stored("xl/_rels/workbook.xml.rels", relationships.as_bytes()),
        RawEntry::stored("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ];
    let names: Vec<String> = (1..=items.len())
        .map(|n| format!("customXml/item{n}.xml"))
        .collect();
    for (name, item) in names.iter().zip(items) {
        entries.push(RawEntry::stored(name, item));
    }
    build_zip(&entries)
}

/// `text` as UTF-16LE with its byte order mark, encoded here.
fn m2_1_utf16le(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Case cdoc_m2_1__manifest_roundtrip.

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cdoc_m2_1__manifest_roundtrip() {
    const CASE: &str = "cdoc_m2_1__manifest_roundtrip";
    let manifest: Value = serde_json::from_str(M2_1_MANIFEST).expect("the generator's manifest");
    let block = |n: u8| 0x5e1f_0c3a_7b2d_4c8e_9a41_0d6b_3f2e_8c00u128 | u128::from(n);
    let placement = |n: u8| 0x9c4d_2b17_3e5a_4f60_8b72_1a0c_5d3e_7f00u128 | u128::from(n);
    let b = |n: u8| m2_1_uuid(block(n));
    let p = |n: u8| m2_1_uuid(placement(n));

    // notebook.xlsx holds no manifest: it loads with an empty layout, no warning, and its cells
    // and names as the generator wrote them.
    let record = m1_2_record(&manifest, "notebook.xlsx");
    let loaded = m2_1_load(M2_1_NOTEBOOK);
    assert!(
        loaded.model().layout().is_empty(),
        "no layout without a manifest"
    );
    assert_eq!(loaded.layout_warnings(), &[] as &[LayoutWarning]);
    assert_eq!(loaded.manifest_part(), None);
    assert_eq!(m2_1_cells(loaded.model()), m2_1_recorded_cells(record));
    assert_eq!(loaded.defined_names_not_loaded(), 2, "Local and Rate");
    let notebook = loaded.model().sheets()[0].id();
    assert_eq!(
        m1_3_sheets(loaded.model()),
        [
            ("Notebook".to_owned(), 1, SheetVisibility::Visible),
            ("Data".to_owned(), 2, SheetVisibility::Visible)
        ]
    );

    // One batch: two cells authored in the backing sheet (a prose text and a formula), the layout
    // edits of the golden fixture decoded through the loaded root (its keys 1 and 2 are Notebook
    // and Data), and two blocks over the new cells with their notebook placements.
    let decoded = decode_batch(SKIN_LAYOUT_V1, loaded.model()).expect("the golden fixture");
    assert_eq!(decoded.operations.len(), 22);
    let prose_text = "Both results, added.";
    let mut operations = vec![
        set(notebook, 7, 1, text(units(prose_text))),
        set(notebook, 8, 2, formula("B5+B6")),
    ];
    operations.extend(decoded.operations);
    operations.extend([
        m2_1_op(SkinLayoutEdit::CreateBlock {
            block: fixture_block(6),
            kind: BlockKind::Calculation,
            anchor: GridArea::cell(cell(notebook, 8, 2)),
        }),
        m2_1_op(SkinLayoutEdit::CreateBlock {
            block: fixture_block(7),
            kind: BlockKind::Prose,
            anchor: GridArea::cell(cell(notebook, 7, 1)),
        }),
        m2_1_op(SkinLayoutEdit::AddPlacement {
            placement: fixture_placement(0x15),
            block: fixture_block(6),
            spot: Spot::Notebook {
                position: position(4),
            },
        }),
        m2_1_op(SkinLayoutEdit::AddPlacement {
            placement: fixture_placement(0x16),
            block: fixture_block(7),
            spot: Spot::Notebook {
                position: position(4),
            },
        }),
    ]);
    let edited = m1_3_edit(loaded.model(), operations);
    let block_line = |n: u8, kind: &str, anchor: &str| {
        format!("block {} {kind} {} {anchor}", b(n), m2_1_locator(block(n)))
    };
    let expected = vec![
        block_line(1, "Prose", "Notebook!A1"),
        block_line(2, "Calculation", "Notebook!B5"),
        block_line(3, "Calculation", "Notebook!B5:B6"),
        block_line(4, "Calculation", "Data!B1:B3"),
        block_line(6, "Calculation", "Notebook!B8"),
        block_line(7, "Prose", "Notebook!A7"),
        format!("Notebook {} block {}", p(0x12), b(2)),
        format!("Notebook {} block {}", p(0x13), b(3)),
        format!("Notebook {} block {}", p(0x11), b(1)),
        format!("Notebook {} block {}", p(0x16), b(7)),
        format!("Notebook {} block {}", p(0x15), b(6)),
        format!("Tree {} block {} parent none", p(0x21), b(1)),
        format!("Tree {} block {} parent {}", p(0x22), b(2), p(0x21)),
        format!("Tree {} block {} parent none", p(0x23), b(3)),
        format!("Canvas {} block {} at 0 300 180 60", p(0x33), b(4)),
        format!("Canvas {} block {} at -35.5 140 240 120", p(0x32), b(3)),
        format!("Canvas {} block {} at 420.25 20 200 96", p(0x31), b(2)),
        format!("Grid {} block {}", p(0x42), b(2)),
        format!("Grid {} block {}", p(0x41), b(3)),
    ];
    assert_eq!(m2_1_layout(&edited), expected, "the layout the batch made");
    assert_eq!(
        edited.cell(cell(notebook, 8, 2)),
        Some(&formula("B5+B6")),
        "the formula is an ordinary cell"
    );
    assert!(
        loaded.model().layout().is_empty(),
        "the loaded revision is unchanged"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: notebook.xlsx loaded with no layout and no warning; one prepare_edit batch of 28 operations (2 cells, the 22 layout edits of skin_layout_v1.json decoded through the loaded root, 2 more blocks and 2 placements) made 6 blocks and 13 placements in the four views, read back as {} layout lines equal to literal expectations",
        expected.len()
    );

    // The save, read with the case's own instruments.
    let (saved, receipt) = m1_3_save(&loaded.snapshot(&edited));
    let source = parse_zip(M2_1_NOTEBOOK);
    let zip = parse_zip(&saved);
    let mut expected_names = m2_1_names(&source);
    expected_names.extend(
        [
            "customXml/item2.xml",
            "customXml/itemProps2.xml",
            "customXml/_rels/item2.xml.rels",
        ]
        .map(str::to_owned),
    );
    assert_eq!(
        m2_1_names(&zip),
        expected_names,
        "the saved package's entries"
    );
    for name in [
        "_rels/.rels",
        "xl/worksheets/sheet2.xml",
        "customXml/item1.xml",
        "customXml/itemProps1.xml",
        "customXml/_rels/item1.xml.rels",
    ] {
        assert_eq!(
            m2_1_stored(&zip, &saved, name),
            m2_1_stored(&source, M2_1_NOTEBOOK, name),
            "{name}: copied as stored"
        );
    }
    assert_eq!(receipt.entries_copied, 5);
    assert_eq!(receipt.parts_written, 8, "5 parts rewritten and 3 added");

    let content_types = m1_3_content(&zip, &saved, "[Content_Types].xml");
    let types = parse_xml(&content_types);
    let override_of = |part: &str| {
        types
            .all("Override")
            .find(|o| o.attribute("PartName") == Some(part))
            .and_then(|o| o.attribute("ContentType"))
            .map(str::to_owned)
    };
    assert_eq!(
        override_of("/customXml/itemProps2.xml").as_deref(),
        Some(CUSTOM_XML_PROPS_TYPE)
    );
    assert_eq!(
        override_of("/customXml/item2.xml"),
        None,
        "the xml default covers the item"
    );
    assert!(
        types
            .all("Default")
            .any(|d| d.attribute("Extension") == Some("xml")
                && d.attribute("ContentType") == Some("application/xml")),
        "the default for xml is application/xml"
    );

    let workbook_relationships = m1_3_content(&zip, &saved, "xl/_rels/workbook.xml.rels");
    let source_relationships = m1_3_content(&source, M2_1_NOTEBOOK, "xl/_rels/workbook.xml.rels");
    let source_items: Vec<Vec<u8>> = parse_xml(&source_relationships)
        .all("Relationship")
        .map(|r| r.bytes(&source_relationships).to_vec())
        .collect();
    let saved_root = parse_xml(&workbook_relationships);
    let saved_items: Vec<&Element> = saved_root.all("Relationship").collect();
    assert_eq!(saved_items.len(), source_items.len() + 1);
    for (kept, source_item) in saved_items.iter().zip(&source_items) {
        assert_eq!(kept.bytes(&workbook_relationships), source_item.as_slice());
    }
    let added = saved_items.last().expect("the added relationship");
    assert_eq!(added.attribute("Type"), Some(CUSTOM_XML_RELATIONSHIP));
    assert_eq!(added.attribute("Target"), Some("../customXml/item2.xml"));
    let added_id = added.attribute("Id").expect("an Id");
    assert_eq!(
        saved_items
            .iter()
            .filter(|r| r.attribute("Id") == Some(added_id))
            .count(),
        1,
        "the added relationship's ID is unique"
    );

    let item_relationships = m1_3_content(&zip, &saved, "customXml/_rels/item2.xml.rels");
    let item_root = parse_xml(&item_relationships);
    let item_items: Vec<&Element> = item_root.all("Relationship").collect();
    assert_eq!(item_items.len(), 1);
    assert_eq!(
        item_items[0].attribute("Type"),
        Some(CUSTOM_XML_PROPS_RELATIONSHIP)
    );
    assert_eq!(item_items[0].attribute("Target"), Some("itemProps2.xml"));

    let props = m1_3_content(&zip, &saved, "customXml/itemProps2.xml");
    let props_root = parse_xml(&props);
    assert_eq!(props_root.local(), "datastoreItem");
    let prefix = props_root.name.split(':').next().expect("a prefix");
    assert_eq!(
        props_root.attribute(&format!("xmlns:{prefix}")),
        Some(DATASTORE_NAMESPACE)
    );
    let item_id = props_root
        .attribute(&format!("{prefix}:itemID"))
        .expect("an item ID");
    let braced = item_id.len() == 38
        && item_id.starts_with('{')
        && item_id.ends_with('}')
        && item_id[1..37].char_indices().all(|(at, c)| {
            if [8, 13, 18, 23].contains(&at) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        });
    assert!(braced, "the item ID {item_id} is a braced GUID");
    let schema = props_root
        .child("schemaRefs")
        .and_then(|refs| refs.child("schemaRef"))
        .and_then(|r| r.attribute(&format!("{prefix}:uri")));
    assert_eq!(schema, Some(DNA_NAMESPACE));

    // The manifest: its version, blocks and placements as the case expects them.
    let item = m1_3_content(&zip, &saved, "customXml/item2.xml");
    let manifest_root = parse_xml(&item);
    assert_eq!(manifest_root.name, "manifest");
    assert_eq!(manifest_root.attribute("xmlns"), Some(DNA_NAMESPACE));
    assert_eq!(manifest_root.attribute("version"), Some("1"));
    let written_blocks: Vec<(String, String, String)> = manifest_root
        .child("blocks")
        .expect("blocks")
        .all("block")
        .map(|block| {
            let attribute = |name: &str| block.attribute(name).expect(name).to_owned();
            (attribute("id"), attribute("kind"), attribute("locator"))
        })
        .collect();
    let expected_blocks: Vec<(String, String, String)> = [
        (1, "prose"),
        (2, "calculation"),
        (3, "calculation"),
        (4, "calculation"),
        (6, "calculation"),
        (7, "prose"),
    ]
    .into_iter()
    .map(|(n, kind)| (b(n), kind.to_owned(), m2_1_locator(block(n))))
    .collect();
    assert_eq!(written_blocks, expected_blocks, "the manifest's blocks");
    let views = manifest_root.child("views").expect("views");
    let written_view = |name: &str| -> Vec<Vec<(String, String)>> {
        views
            .child(name)
            .map(|view| {
                view.all("placement")
                    .map(|placement| placement.attributes.clone())
                    .collect()
            })
            .unwrap_or_default()
    };
    let attrs = |pairs: &[(&str, String)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect()
    };
    assert_eq!(
        written_view("notebook"),
        [(0x12, 2), (0x13, 3), (0x11, 1), (0x16, 7), (0x15, 6)]
            .map(|(pl, bl)| attrs(&[("id", p(pl)), ("block", b(bl))]))
            .to_vec()
    );
    assert_eq!(
        written_view("tree"),
        vec![
            attrs(&[("id", p(0x21)), ("block", b(1))]),
            attrs(&[("id", p(0x22)), ("block", b(2)), ("parent", p(0x21))]),
            attrs(&[("id", p(0x23)), ("block", b(3))]),
        ]
    );
    let canvas = written_view("canvas");
    let canvas_read: Vec<(String, String, [u64; 4])> = canvas
        .iter()
        .map(|attributes| {
            let value = |name: &str| {
                attributes
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.clone())
                    .expect(name)
            };
            let number = |name: &str| value(name).parse::<f64>().expect("a number").to_bits();
            (
                value("id"),
                value("block"),
                [number("x"), number("y"), number("width"), number("height")],
            )
        })
        .collect();
    assert_eq!(
        canvas_read,
        vec![
            (
                p(0x33),
                b(4),
                [0.0f64, 300.0, 180.0, 60.0].map(f64::to_bits)
            ),
            (
                p(0x32),
                b(3),
                [-35.5f64, 140.0, 240.0, 120.0].map(f64::to_bits)
            ),
            (
                p(0x31),
                b(2),
                [420.25f64, 20.0, 200.0, 96.0].map(f64::to_bits)
            ),
        ]
    );
    assert_eq!(
        written_view("grid"),
        [(0x42, 2), (0x41, 3)]
            .map(|(pl, bl)| attrs(&[("id", p(pl)), ("block", b(bl))]))
            .to_vec()
    );
    assert!(
        !m2_1_holds(&item, &b(5)),
        "the block created and removed in the batch is not written"
    );

    // The workbook part: the source's names as it wrote them, a hidden workbook-scoped locator for
    // each block referring to its visible backing cell or range, and calcPr as written.
    let workbook = m1_3_content(&zip, &saved, "xl/workbook.xml");
    let source_workbook = m1_3_content(&source, M2_1_NOTEBOOK, "xl/workbook.xml");
    let names = m2_1_defined_names(&workbook);
    let source_names = m2_1_defined_names(&source_workbook);
    assert_eq!(names.len(), source_names.len() + 6);
    for (kept, source_name) in names.iter().zip(&source_names) {
        assert_eq!(
            kept.4, source_name.4,
            "{}: as the source wrote it",
            source_name.0
        );
    }
    let locators: Vec<(String, Option<String>, Option<String>, String)> = names
        [source_names.len()..]
        .iter()
        .map(|(name, local, hidden, text, _)| {
            (name.clone(), local.clone(), hidden.clone(), text.clone())
        })
        .collect();
    let expected_locators: Vec<(String, Option<String>, Option<String>, String)> = [
        (1, "'Notebook'!$A$1"),
        (2, "'Notebook'!$B$5"),
        (3, "'Notebook'!$B$5:$B$6"),
        (4, "'Data'!$B$1:$B$3"),
        (6, "'Notebook'!$B$8"),
        (7, "'Notebook'!$A$7"),
    ]
    .into_iter()
    .map(|(n, text)| {
        (
            m2_1_locator(block(n)),
            None,
            Some("1".to_owned()),
            text.to_owned(),
        )
    })
    .collect();
    assert_eq!(locators, expected_locators, "the locator names");
    for (_, _, _, text) in &locators {
        let parsed = calc_fml::parse_formula(&format!("={text}"));
        assert!(
            parsed.diagnostics().is_empty(),
            "CalcFml reads the locator {text} without a diagnostic"
        );
    }
    let calc_pr = parse_xml(&workbook)
        .child("calcPr")
        .and_then(|c| c.attribute("fullCalcOnLoad"))
        .map(str::to_owned);
    assert_eq!(
        calc_pr.as_deref(),
        None,
        "a save asks for no full calculation"
    );

    // The formula and the prose are ordinary cells of the backing sheet.
    let sheet = m1_3_content(&zip, &saved, "xl/worksheets/sheet1.xml");
    let sheet_root = parse_xml(&sheet);
    let cells = m1_3_sheet_cells(&sheet_root);
    let cell_at = |row: u32, column: u32| {
        cells
            .iter()
            .find(|(place, _)| *place == (row, column))
            .map(|(_, c)| *c)
            .unwrap_or_else(|| panic!("a cell at {row}, {column}"))
    };
    let b8 = cell_at(8, 2);
    assert_eq!(b8.child("f").map(Element::text).as_deref(), Some("B5+B6"));
    assert!(b8.child("v").is_none(), "no cached value for a new formula");
    let a7 = cell_at(7, 1);
    assert_eq!(a7.attribute("t"), Some("s"));
    let index: usize = a7
        .child("v")
        .map(Element::text)
        .expect("an index")
        .parse()
        .expect("a number");
    let strings = m1_3_content(&zip, &saved, "xl/sharedStrings.xml");
    let item_text = parse_xml(&strings)
        .all("si")
        .nth(index)
        .map(Element::text)
        .expect("the shared string");
    assert_eq!(item_text, prose_text);
    note!(
        "ACCEPT-NOTE: {CASE}: the saved package ({} bytes, {} entries) holds the manifest in one CustomXML item customXml/item2.xml with its item properties (datastore item {item_id}, schemaRef {DNA_NAMESPACE}), the item's relationships part and a customXml relationship {added_id} from the workbook part; [Content_Types].xml gives the properties part its content type; the workbook part holds Local and Rate as written and 6 hidden locator names, each read by CalcFml without a diagnostic; B8's formula and A7's prose are ordinary cells; 5 entries were copied as stored",
        saved.len(),
        zip.entries.len()
    );

    // Reopened with calc-doc's loader: the same cells, the same layout, no warning.
    let reopened = m2_1_load(&saved);
    assert_eq!(reopened.layout_warnings(), &[] as &[LayoutWarning]);
    assert_eq!(reopened.manifest_part(), Some("customXml/item2.xml"));
    assert_eq!(
        m2_1_layout(reopened.model()),
        expected,
        "the layout after save and open"
    );
    assert_eq!(m2_1_cells(reopened.model()), m2_1_cells(&edited));
    assert_eq!(reopened.defined_names_not_loaded(), 2, "Local and Rate");

    // A revision of the reopened workbook that changes only the layout: the canvas card of R to
    // the top, the tree placement of C under R's, and the block D removed after its one placement.
    // Only the manifest and the workbook part (D's locator removed) are rewritten.
    let relaid = m1_3_edit(
        reopened.model(),
        vec![
            m2_1_op(SkinLayoutEdit::MovePlacement {
                placement: fixture_placement(0x32),
                spot: canvas_spot(
                    3,
                    [
                        (-35.5f64).to_bits(),
                        140f64.to_bits(),
                        240f64.to_bits(),
                        120f64.to_bits(),
                    ],
                ),
            }),
            m2_1_op(SkinLayoutEdit::MovePlacement {
                placement: fixture_placement(0x22),
                spot: tree_spot(Some(0x23), 1),
            }),
            m2_1_op(SkinLayoutEdit::RemovePlacement {
                placement: fixture_placement(0x33),
            }),
            m2_1_op(SkinLayoutEdit::RemoveBlock {
                block: fixture_block(4),
            }),
        ],
    );
    let relaid_expected: Vec<String> = expected
        .iter()
        .filter(|line| !line.contains(&b(4)))
        .filter(|line| !line.starts_with("Tree") && !line.starts_with("Canvas"))
        .cloned()
        .chain([
            format!("Tree {} block {} parent none", p(0x21), b(1)),
            format!("Tree {} block {} parent none", p(0x23), b(3)),
            format!("Tree {} block {} parent {}", p(0x22), b(2), p(0x23)),
            format!("Canvas {} block {} at 420.25 20 200 96", p(0x31), b(2)),
            format!("Canvas {} block {} at -35.5 140 240 120", p(0x32), b(3)),
        ])
        .collect();
    let mut sorted_expected = relaid_expected.clone();
    // The lines in layout order: blocks, then notebook, tree, canvas and grid.
    let rank = |line: &String| {
        ["block", "Notebook", "Tree", "Canvas", "Grid"]
            .iter()
            .position(|kind| line.starts_with(kind))
            .expect("a line kind")
    };
    sorted_expected.sort_by_key(rank);
    assert_eq!(m2_1_layout(&relaid), sorted_expected, "the relaid layout");
    assert_eq!(
        m2_1_cells(&relaid),
        m2_1_cells(reopened.model()),
        "a layout edit changes no cell"
    );
    let (resaved, resaved_receipt) = m1_3_save(&reopened.snapshot(&relaid));
    let rezip = parse_zip(&resaved);
    assert_eq!(m2_1_names(&rezip), m2_1_names(&zip));
    assert_eq!(
        resaved_receipt.parts_written, 2,
        "the manifest and the workbook part"
    );
    assert_eq!(resaved_receipt.caches_dropped, 0);
    for name in m2_1_names(&zip) {
        if name == "customXml/item2.xml" || name == "xl/workbook.xml" {
            continue;
        }
        assert_eq!(
            m2_1_stored(&rezip, &resaved, &name),
            m2_1_stored(&zip, &saved, &name),
            "{name}: copied as stored"
        );
    }
    let reworkbook = m1_3_content(&rezip, &resaved, "xl/workbook.xml");
    let renames = m2_1_defined_names(&reworkbook);
    let d_locator = m2_1_locator(block(4));
    assert!(
        renames.iter().all(|(name, ..)| *name != d_locator),
        "D's locator is removed"
    );
    let kept_names: Vec<&Vec<u8>> = names
        .iter()
        .filter(|(name, ..)| *name != d_locator)
        .map(|(.., bytes)| bytes)
        .collect();
    assert_eq!(
        renames.iter().map(|(.., bytes)| bytes).collect::<Vec<_>>(),
        kept_names,
        "every other defined name as the earlier save wrote it"
    );
    let calc_pr_of = |workbook: &[u8]| {
        parse_xml(workbook)
            .child("calcPr")
            .map(|c| c.bytes(workbook).to_vec())
    };
    assert_eq!(
        calc_pr_of(&reworkbook),
        calc_pr_of(&workbook),
        "calcPr untouched"
    );
    let reitem = m1_3_content(&rezip, &resaved, "customXml/item2.xml");
    let earlier_blocks = manifest_root.child("blocks").expect("blocks");
    for kept in earlier_blocks.all("block") {
        let bytes = String::from_utf8(kept.bytes(&item).to_vec()).expect("UTF-8");
        assert_eq!(
            m2_1_holds(&reitem, &bytes),
            kept.attribute("id") != Some(b(4).as_str()),
            "{bytes}: kept unless removed"
        );
    }
    let rereopened = m2_1_load(&resaved);
    assert_eq!(m2_1_layout(rereopened.model()), sorted_expected);
    assert_eq!(rereopened.layout_warnings(), &[] as &[LayoutWarning]);
    note!(
        "ACCEPT-NOTE: {CASE}: reopened, the saved package gave the same 19 layout lines, the same cells and no warning; a layout-only revision (a canvas card to the top, a tree placement under another, a block removed with its placement) rewrote only the manifest and the workbook part (D's locator removed, every other defined name and calcPr byte for byte), kept each remaining block element of the earlier manifest byte for byte, and reopened to {} lines",
        sorted_expected.len()
    );

    // manifest-v1.xlsx: a version 1 manifest the generator wrote, with fields version 1 does not
    // name. It loads as the generator's record states, the block without a locator name warned.
    let record = m1_2_record(&manifest, "manifest-v1.xlsx");
    let dna = &record["dna_manifest"];
    let v1 = m2_1_load(M2_1_V1);
    assert_eq!(m2_1_layout(v1.model()), m2_1_recorded_layout(dna));
    assert_eq!(m2_1_cells(v1.model()), m2_1_recorded_cells(record));
    assert_eq!(v1.manifest_part(), Some("customXml/item2.xml"));
    let m = |n: u8| 0x0f3e_7a21_5c6b_4d8e_a1f2_3b4c_5d6e_7f00u128 | u128::from(n);
    let q = |n: u8| 0x7d2c_1b0a_9e8f_4a6b_b5c4_d3e2_f1a0_b900u128 | u128::from(n);
    let m_id = |n: u8| BlockId::new(m(n)).expect("an ID");
    let q_id = |n: u8| PlacementId::new(q(n)).expect("an ID");
    assert_eq!(
        v1.layout_warnings(),
        &[LayoutWarning::UnresolvedLocator {
            block: m_id(4),
            locator: LocatorName::new(&m2_1_locator(m(4))).expect("a locator name"),
        }]
    );
    assert_eq!(v1.defined_names_not_loaded(), 2, "Local and Rate");
    let v1_data = v1.model().sheets()[1].id();

    // A revision of the read version 1 manifest's workbook that changes only a cell: the
    // manifest's item, item properties and relationships parts are copied as stored, byte for
    // byte, with definedNames as written, and the layout reopens as it was.
    let v1_cell_edit = m1_3_edit(v1.model(), vec![set(v1_data, 2, 2, number(9.0))]);
    assert!(
        Arc::ptr_eq(v1_cell_edit.layout(), v1.model().layout()),
        "a cell edit shares the layout"
    );
    let (v1_cell_saved, v1_cell_receipt) = m1_3_save(&v1.snapshot(&v1_cell_edit));
    let v1_stored = parse_zip(M2_1_V1);
    let v1_cell_zip = parse_zip(&v1_cell_saved);
    assert_eq!(m2_1_names(&v1_cell_zip), m2_1_names(&v1_stored));
    for name in [
        "customXml/item2.xml",
        "customXml/itemProps2.xml",
        "customXml/_rels/item2.xml.rels",
        "xl/_rels/workbook.xml.rels",
        "[Content_Types].xml",
    ] {
        assert_eq!(
            m2_1_stored(&v1_cell_zip, &v1_cell_saved, name),
            m2_1_stored(&v1_stored, M2_1_V1, name),
            "{name}: copied as stored after a cell-only save"
        );
    }
    let defined_names_bytes = |workbook: &[u8]| {
        parse_xml(workbook)
            .child("definedNames")
            .map(|names| names.bytes(workbook).to_vec())
    };
    assert_eq!(
        defined_names_bytes(&m1_3_content(
            &v1_cell_zip,
            &v1_cell_saved,
            "xl/workbook.xml"
        )),
        defined_names_bytes(&m1_3_content(&v1_stored, M2_1_V1, "xl/workbook.xml")),
        "definedNames as written"
    );
    assert_eq!(
        v1_cell_receipt.parts_written, 2,
        "the edited sheet and the sheet whose caches are dropped; the workbook part is copied"
    );
    let v1_cell_reopened = m2_1_load(&v1_cell_saved);
    assert_eq!(
        m2_1_layout(v1_cell_reopened.model()),
        m2_1_recorded_layout(dna)
    );
    assert_eq!(v1_cell_reopened.layout_warnings(), v1.layout_warnings());
    assert_eq!(
        v1_cell_reopened
            .model()
            .cell(cell(v1_cell_reopened.model().sheets()[1].id(), 2, 2)),
        Some(&number(9.0))
    );
    note!(
        "ACCEPT-NOTE: {CASE}: a cell-only save of manifest-v1.xlsx copied customXml/item2.xml, customXml/itemProps2.xml, customXml/_rels/item2.xml.rels, the workbook relationships and [Content_Types].xml as stored byte for byte, kept definedNames as written, rewrote {} parts, and reopened to the same {} layout lines",
        v1_cell_receipt.parts_written,
        m2_1_recorded_layout(dna).len()
    );

    let v1_edited = m1_3_edit(
        v1.model(),
        vec![
            m2_1_op(SkinLayoutEdit::MovePlacement {
                placement: q_id(0x12),
                spot: Spot::Notebook {
                    position: position(1),
                },
            }),
            m2_1_op(SkinLayoutEdit::MovePlacement {
                placement: q_id(0x31),
                spot: canvas_spot(
                    2,
                    [
                        50f64.to_bits(),
                        60f64.to_bits(),
                        200f64.to_bits(),
                        80f64.to_bits(),
                    ],
                ),
            }),
            m2_1_op(SkinLayoutEdit::MovePlacement {
                placement: q_id(0x23),
                spot: Spot::Tree {
                    parent: Some(q_id(0x22)),
                    position: position(1),
                },
            }),
            m2_1_op(SkinLayoutEdit::RemovePlacement {
                placement: q_id(0x42),
            }),
            m2_1_op(SkinLayoutEdit::RemoveBlock { block: m_id(4) }),
            m2_1_op(SkinLayoutEdit::CreateBlock {
                block: m_id(5),
                kind: BlockKind::Calculation,
                anchor: GridArea::new(cell(v1_data, 1, 2), cell(v1_data, 3, 2)).expect("an area"),
            }),
            m2_1_op(SkinLayoutEdit::AddPlacement {
                placement: q_id(0x25),
                block: m_id(5),
                spot: Spot::Tree {
                    parent: Some(q_id(0x21)),
                    position: position(1),
                },
            }),
        ],
    );
    let mu = |n: u8| m2_1_uuid(m(n));
    let qu = |n: u8| m2_1_uuid(q(n));
    let v1_expected = vec![
        format!("block {} Prose {} Notebook!A1", mu(1), m2_1_locator(m(1))),
        format!(
            "block {} Calculation {} Notebook!B5",
            mu(2),
            m2_1_locator(m(2))
        ),
        format!(
            "block {} Calculation {} Notebook!B5:B6",
            mu(3),
            m2_1_locator(m(3))
        ),
        format!(
            "block {} Calculation {} Data!B1:B3",
            mu(5),
            m2_1_locator(m(5))
        ),
        format!("Notebook {} block {}", qu(0x12), mu(2)),
        format!("Notebook {} block {}", qu(0x11), mu(1)),
        format!("Notebook {} block {}", qu(0x13), mu(3)),
        format!("Tree {} block {} parent none", qu(0x21), mu(1)),
        format!("Tree {} block {} parent {}", qu(0x25), mu(5), qu(0x21)),
        format!("Tree {} block {} parent {}", qu(0x22), mu(2), qu(0x21)),
        format!("Tree {} block {} parent {}", qu(0x23), mu(3), qu(0x22)),
        format!("Canvas {} block {} at -35.5 140 240 120", qu(0x32), mu(3)),
        format!("Canvas {} block {} at 50 60 200 80", qu(0x31), mu(2)),
        format!("Grid {} block {}", qu(0x41), mu(2)),
    ];
    assert_eq!(m2_1_layout(&v1_edited), v1_expected);
    let (v1_saved, v1_receipt) = m1_3_save(&v1.snapshot(&v1_edited));
    let v1_source = parse_zip(M2_1_V1);
    let v1_zip = parse_zip(&v1_saved);
    assert_eq!(
        m2_1_names(&v1_zip),
        m2_1_names(&v1_source),
        "no part added or removed"
    );
    assert_eq!(
        v1_receipt.parts_written, 2,
        "the manifest and the workbook part"
    );
    let source_item = m1_3_content(&v1_source, M2_1_V1, "customXml/item2.xml");
    let v1_item = m1_3_content(&v1_zip, &v1_saved, "customXml/item2.xml");
    let unknown: Vec<&str> = dna["unknown_fields"]
        .as_array()
        .expect("unknown fields")
        .iter()
        .map(|field| field.as_str().expect("a field"))
        .collect();
    assert_eq!(unknown.len(), 9);
    for field in &unknown {
        assert!(m2_1_holds(&source_item, field), "{field}: in the source");
        assert!(m2_1_holds(&v1_item, field), "{field}: kept by the save");
    }
    let v1_root = parse_xml(&v1_item);
    let source_root = parse_xml(&source_item);
    assert_eq!(
        &v1_item[..v1_root.start_tag.1],
        &source_item[..source_root.start_tag.1],
        "the declaration and the manifest element's start tag as written"
    );
    let v1_canvas = v1_root
        .child("views")
        .and_then(|views| views.child("canvas"))
        .expect("canvas");
    let moved_card = v1_canvas
        .all("placement")
        .find(|placement| placement.attribute("id") == Some(qu(0x31).as_str()))
        .expect("the moved card");
    for (name, value) in [
        ("x", "50"),
        ("y", "60"),
        ("width", "200"),
        ("height", "80"),
        ("zoom", "2"),
    ] {
        assert_eq!(
            moved_card.attribute(name),
            Some(value),
            "the moved card's {name}"
        );
    }
    let v1_tree = v1_root
        .child("views")
        .and_then(|views| views.child("tree"))
        .expect("tree");
    let moved_node = v1_tree
        .all("placement")
        .find(|placement| placement.attribute("id") == Some(qu(0x23).as_str()))
        .expect("the moved tree placement");
    assert_eq!(moved_node.attribute("parent"), Some(qu(0x22).as_str()));
    assert!(
        !m2_1_holds(&v1_item, &mu(4)),
        "the removed block and its grid placement are gone"
    );
    assert!(
        m2_1_holds(&v1_item, &qu(0x51)),
        "the unknown view's placement is kept"
    );
    let v1_workbook = m1_3_content(&v1_zip, &v1_saved, "xl/workbook.xml");
    let v1_source_workbook = m1_3_content(&v1_source, M2_1_V1, "xl/workbook.xml");
    let v1_names = m2_1_defined_names(&v1_workbook);
    let v1_source_names = m2_1_defined_names(&v1_source_workbook);
    assert_eq!(v1_names.len(), v1_source_names.len() + 1);
    for (kept, source_name) in v1_names.iter().zip(&v1_source_names) {
        assert_eq!(
            kept.4, source_name.4,
            "{}: as the source wrote it",
            source_name.0
        );
    }
    let added_name = v1_names.last().expect("the added locator");
    assert_eq!(
        (
            &added_name.0,
            &added_name.1,
            added_name.2.as_deref(),
            added_name.3.as_str()
        ),
        (&m2_1_locator(m(5)), &None, Some("1"), "'Data'!$B$1:$B$3")
    );
    assert_eq!(
        parse_xml(&v1_workbook)
            .child("calcPr")
            .map(|c| c.bytes(&v1_workbook).to_vec()),
        parse_xml(&v1_source_workbook)
            .child("calcPr")
            .map(|c| c.bytes(&v1_source_workbook).to_vec()),
        "a layout-only save leaves calcPr as written"
    );
    let v1_reopened = m2_1_load(&v1_saved);
    assert_eq!(m2_1_layout(v1_reopened.model()), v1_expected);
    assert_eq!(v1_reopened.layout_warnings(), &[] as &[LayoutWarning]);
    note!(
        "ACCEPT-NOTE: {CASE}: manifest-v1.xlsx loaded as the generator's record states (4 blocks, the one without a locator name warned UnresolvedLocator and kept without an anchor, 9 placements in four views); after moving three placements, removing a placement and the unanchored block and creating a block on Data, the save rewrote only the manifest and the workbook part, and each of the {} unknown fields the generator wrote is in the saved manifest byte for byte (the moved canvas card's zoom kept on its rebuilt start tag); reopened, {} layout lines and no warning",
        unknown.len(),
        v1_expected.len()
    );

    // Refusals: each layout edit prepare_edit refuses, with its typed reason and the operation's
    // index, against the notebook's loaded revision, which stays as it was.
    let base = loaded.model();
    let create = |n: u8| {
        m2_1_op(SkinLayoutEdit::CreateBlock {
            block: fixture_block(n),
            kind: BlockKind::Prose,
            anchor: GridArea::cell(cell(notebook, 1, 1)),
        })
    };
    let add = |pl: u8, bl: u8, spot: Spot| {
        m2_1_op(SkinLayoutEdit::AddPlacement {
            placement: fixture_placement(pl),
            block: fixture_block(bl),
            spot,
        })
    };
    let notebook_spot = |n: u32| Spot::Notebook {
        position: position(n),
    };
    let layout_refusal = |operation: usize, error: LayoutError| {
        EditError::Model(ApplyError::Layout { operation, error })
    };
    let refusals: Vec<(&str, Vec<WorkbookOp>, EditError)> = vec![
        (
            "a second block with one ID",
            vec![create(1), create(1)],
            layout_refusal(1, LayoutError::DuplicateBlock(fixture_block(1))),
        ),
        (
            "a placement of a block that does not exist",
            vec![add(0x11, 9, notebook_spot(1))],
            layout_refusal(0, LayoutError::UnknownBlock(fixture_block(9))),
        ),
        (
            "a removal of a block that does not exist",
            vec![m2_1_op(SkinLayoutEdit::RemoveBlock {
                block: fixture_block(9),
            })],
            layout_refusal(0, LayoutError::UnknownBlock(fixture_block(9))),
        ),
        (
            "a removal of a block still placed",
            vec![
                create(1),
                add(0x11, 1, notebook_spot(1)),
                m2_1_op(SkinLayoutEdit::RemoveBlock {
                    block: fixture_block(1),
                }),
            ],
            layout_refusal(2, LayoutError::BlockPlaced(fixture_block(1))),
        ),
        (
            "a second placement with one ID, in another view",
            vec![
                create(1),
                add(0x11, 1, notebook_spot(1)),
                add(
                    0x11,
                    1,
                    Spot::Grid {
                        position: position(1),
                    },
                ),
            ],
            layout_refusal(2, LayoutError::DuplicatePlacement(fixture_placement(0x11))),
        ),
        (
            "a move of a placement that does not exist",
            vec![m2_1_op(SkinLayoutEdit::MovePlacement {
                placement: fixture_placement(0x19),
                spot: notebook_spot(1),
            })],
            layout_refusal(0, LayoutError::UnknownPlacement(fixture_placement(0x19))),
        ),
        (
            "a removal of a placement that does not exist",
            vec![m2_1_op(SkinLayoutEdit::RemovePlacement {
                placement: fixture_placement(0x19),
            })],
            layout_refusal(0, LayoutError::UnknownPlacement(fixture_placement(0x19))),
        ),
        (
            "a move into another view",
            vec![
                create(1),
                add(0x11, 1, notebook_spot(1)),
                m2_1_op(SkinLayoutEdit::MovePlacement {
                    placement: fixture_placement(0x11),
                    spot: Spot::Grid {
                        position: position(1),
                    },
                }),
            ],
            layout_refusal(
                2,
                LayoutError::OtherView {
                    placement: fixture_placement(0x11),
                    view: ViewKind::Notebook,
                },
            ),
        ),
        (
            "a tree parent that is a notebook placement",
            vec![
                create(1),
                add(0x11, 1, notebook_spot(1)),
                add(0x21, 1, tree_spot(Some(0x11), 1)),
            ],
            layout_refusal(2, LayoutError::ParentNotInTree(fixture_placement(0x11))),
        ),
        (
            "a tree placement moved under its own child",
            vec![
                create(1),
                add(0x21, 1, tree_spot(None, 1)),
                add(0x22, 1, tree_spot(Some(0x21), 1)),
                m2_1_op(SkinLayoutEdit::MovePlacement {
                    placement: fixture_placement(0x21),
                    spot: tree_spot(Some(0x22), 1),
                }),
            ],
            layout_refusal(
                3,
                LayoutError::ParentCycle {
                    placement: fixture_placement(0x21),
                    parent: fixture_placement(0x22),
                },
            ),
        ),
        (
            "a tree placement moved under itself",
            vec![
                create(1),
                add(0x21, 1, tree_spot(None, 1)),
                m2_1_op(SkinLayoutEdit::MovePlacement {
                    placement: fixture_placement(0x21),
                    spot: tree_spot(Some(0x21), 1),
                }),
            ],
            layout_refusal(
                2,
                LayoutError::ParentCycle {
                    placement: fixture_placement(0x21),
                    parent: fixture_placement(0x21),
                },
            ),
        ),
        (
            "a removal of a tree placement with a child",
            vec![
                create(1),
                add(0x21, 1, tree_spot(None, 1)),
                add(0x22, 1, tree_spot(Some(0x21), 1)),
                m2_1_op(SkinLayoutEdit::RemovePlacement {
                    placement: fixture_placement(0x21),
                }),
            ],
            layout_refusal(3, LayoutError::HasChildren(fixture_placement(0x21))),
        ),
        (
            "a notebook position past the end",
            vec![create(1), add(0x11, 1, notebook_spot(3))],
            layout_refusal(
                1,
                LayoutError::PositionPastEnd {
                    position: position(3),
                    last: 1,
                },
            ),
        ),
        (
            "a child position past the end of its parent's children",
            vec![
                create(1),
                add(0x21, 1, tree_spot(None, 1)),
                add(0x22, 1, tree_spot(Some(0x21), 2)),
            ],
            layout_refusal(
                2,
                LayoutError::PositionPastEnd {
                    position: position(2),
                    last: 1,
                },
            ),
        ),
        (
            "a block anchored on a sheet the workbook does not hold",
            vec![m2_1_op(SkinLayoutEdit::CreateBlock {
                block: fixture_block(1),
                kind: BlockKind::Calculation,
                anchor: GridArea::full_sheet(session(0).stranger),
            })],
            EditError::Model(ApplyError::UnknownSheet {
                operation: 0,
                sheet: session(0).stranger,
            }),
        ),
    ];
    for (label, operations, expected) in &refusals {
        assert_eq!(
            m2_1_try(base, operations.clone()).err().as_ref(),
            Some(expected),
            "{label}"
        );
    }
    assert!(base.layout().is_empty() && base.revision() == AuthoredRevision::INITIAL);

    // A locator name in use: a manifest built here holds a block A whose locator is the name a new
    // block B would get; creating B is refused through the route.
    let (a_id, b_id) = (m(0xa), m(0xb));
    let in_use = m2_1_built(
        &format!(
            r#"<definedNames><definedName name="{}" hidden="1">Numbers!$A$1</definedName></definedNames>"#,
            m2_1_locator(b_id)
        ),
        Some(&format!(
            r#"<manifest xmlns="{DNA_NAMESPACE}" version="1"><blocks><block id="{}" kind="prose" locator="{}"/></blocks></manifest>"#,
            m2_1_uuid(a_id),
            m2_1_locator(b_id)
        )),
        None,
    );
    let in_use_loaded = m2_1_load(&in_use);
    assert_eq!(in_use_loaded.layout_warnings(), &[] as &[LayoutWarning]);
    let numbers = in_use_loaded.model().sheets()[0].id();
    assert_eq!(
        m2_1_try(
            in_use_loaded.model(),
            vec![m2_1_op(SkinLayoutEdit::CreateBlock {
                block: BlockId::new(b_id).expect("an ID"),
                kind: BlockKind::Prose,
                anchor: GridArea::cell(cell(numbers, 1, 1)),
            })]
        )
        .err(),
        Some(layout_refusal(
            0,
            LayoutError::LocatorInUse {
                block: BlockId::new(b_id).expect("an ID"),
                other: BlockId::new(a_id).expect("an ID"),
            }
        ))
    );

    // A locator name a user already holds: the workbook part of a package built here holds the
    // name a new block would get (in upper case); the edit is prepared, the save is refused and
    // writes nothing.
    let z_id = m(0xc);
    let taken = m2_1_built(
        &format!(
            r#"<definedNames><definedName name="{}">Numbers!$A$1</definedName></definedNames>"#,
            m2_1_locator(z_id).to_ascii_uppercase()
        ),
        None,
        None,
    );
    let taken_loaded = m2_1_load(&taken);
    let taken_operations = vec![m2_1_op(SkinLayoutEdit::CreateBlock {
        block: BlockId::new(z_id).expect("an ID"),
        kind: BlockKind::Calculation,
        anchor: GridArea::cell(cell(taken_loaded.model().sheets()[0].id(), 1, 1)),
    })];
    let taken_refusal = || SaveRefusal::LocatorNameTaken {
        part: "xl/workbook.xml".to_owned(),
        name: m2_1_locator(z_id),
    };
    assert_eq!(
        taken_loaded.check_layout_save(&taken_operations),
        Err(taken_refusal()),
        "the check before prepare_edit gives the save's refusal"
    );
    let taken_edit = m1_3_edit(taken_loaded.model(), taken_operations);
    let (refused_save, sink) =
        m1_3_route(&taken_loaded.snapshot(&taken_edit), &SavePolicy::default());
    assert_eq!(refused_save, Err(taken_refusal()));
    assert!(sink.is_empty(), "a refused save writes no byte");

    // Only a hidden, workbook-scoped name of a locator's form is a locator. A manifest built here
    // names as locators a user's visible name Rate (block R), a user's hidden name Secret (U), a
    // visible name of a locator's form (V), a name of a locator's form the workbook holds only
    // scoped to a sheet (S), and a name H the workbook holds both scoped to the sheet (A2) and
    // hidden and workbook-scoped (A1).
    let r_id = m(0x21);
    let u_id = m(0x25);
    let v_id = m(0x22);
    let s_id = m(0x23);
    let h_id = m(0x24);
    let rate = r#"<definedName name="Rate">Numbers!$A$1</definedName>"#.to_owned();
    let secret = r#"<definedName name="Secret" hidden="1">Numbers!$A$1</definedName>"#.to_owned();
    let visible = format!(
        r#"<definedName name="{}">Numbers!$A$1</definedName>"#,
        m2_1_locator(v_id)
    );
    let sheet_scoped = format!(
        r#"<definedName name="{}" localSheetId="0" hidden="1">Numbers!$A$1</definedName>"#,
        m2_1_locator(s_id)
    );
    let h_sheet_scoped = format!(
        r#"<definedName name="{}" localSheetId="0" hidden="1">Numbers!$A$2</definedName>"#,
        m2_1_locator(h_id)
    );
    let h_locator = format!(
        r#"<definedName name="{}" hidden="1">Numbers!$A$1</definedName>"#,
        m2_1_locator(h_id)
    );
    let block_xml = |id: u128, locator: &str| {
        format!(
            r#"<block id="{}" kind="calculation" locator="{locator}"/>"#,
            m2_1_uuid(id)
        )
    };
    let users = m2_1_built(
        &format!(
            "<definedNames>{rate}{secret}{visible}{sheet_scoped}{h_sheet_scoped}{h_locator}</definedNames>"
        ),
        Some(&format!(
            r#"<manifest xmlns="{DNA_NAMESPACE}" version="1"><blocks>{}{}{}{}{}</blocks></manifest>"#,
            block_xml(r_id, "Rate"),
            block_xml(u_id, "Secret"),
            block_xml(v_id, &m2_1_locator(v_id)),
            block_xml(s_id, &m2_1_locator(s_id)),
            block_xml(h_id, &m2_1_locator(h_id)),
        )),
        None,
    );
    let users_loaded = m2_1_load(&users);
    let unresolved = |id: u128, locator: &str| LayoutWarning::UnresolvedLocator {
        block: BlockId::new(id).expect("an ID"),
        locator: LocatorName::new(locator).expect("a locator name"),
    };
    assert_eq!(
        users_loaded.layout_warnings(),
        &[
            unresolved(r_id, "Rate"),
            unresolved(u_id, "Secret"),
            unresolved(v_id, &m2_1_locator(v_id)),
            unresolved(s_id, &m2_1_locator(s_id)),
        ]
    );
    let users_numbers = users_loaded.model().sheets()[0].id();
    let anchors: Vec<Option<GridArea>> = users_loaded
        .model()
        .layout()
        .blocks()
        .iter()
        .map(Block::anchor)
        .collect();
    assert_eq!(
        anchors,
        [
            None,
            None,
            None,
            None,
            Some(GridArea::cell(cell(users_numbers, 1, 1)))
        ],
        "H resolves to its workbook-scoped name's A1, its sheet-scoped name skipped"
    );
    assert_eq!(users_loaded.defined_names_not_loaded(), 5);
    // Removing all five blocks removes H's hidden workbook-scoped locator and nothing else.
    let remove_all: Vec<WorkbookOp> = [r_id, u_id, v_id, s_id, h_id]
        .into_iter()
        .map(|id| {
            m2_1_op(SkinLayoutEdit::RemoveBlock {
                block: BlockId::new(id).expect("an ID"),
            })
        })
        .collect();
    assert_eq!(users_loaded.check_layout_save(&remove_all), Ok(()));
    let users_edit = m1_3_edit(users_loaded.model(), remove_all);
    assert!(users_edit.layout().is_empty());
    let (users_saved, _) = m1_3_save(&users_loaded.snapshot(&users_edit));
    let users_zip = parse_zip(&users_saved);
    let users_workbook = m1_3_content(&users_zip, &users_saved, "xl/workbook.xml");
    let users_names = parse_xml(&users_workbook)
        .child("definedNames")
        .map(|names| String::from_utf8(names.bytes(&users_workbook).to_vec()).expect("UTF-8"));
    assert_eq!(
        users_names,
        Some(format!(
            "<definedNames>{rate}{secret}{visible}{sheet_scoped}{h_sheet_scoped}</definedNames>"
        )),
        "Rate, Secret, the visible name and both sheet-scoped names as the source wrote them"
    );
    // Creating V again would take the name the user's visible definedName holds: the check and
    // the save both refuse it.
    let recreate_v = vec![
        m2_1_op(SkinLayoutEdit::RemoveBlock {
            block: BlockId::new(v_id).expect("an ID"),
        }),
        m2_1_op(SkinLayoutEdit::CreateBlock {
            block: BlockId::new(v_id).expect("an ID"),
            kind: BlockKind::Calculation,
            anchor: GridArea::cell(cell(users_numbers, 1, 1)),
        }),
    ];
    let v_refusal = || SaveRefusal::LocatorNameTaken {
        part: "xl/workbook.xml".to_owned(),
        name: m2_1_locator(v_id),
    };
    assert_eq!(
        users_loaded.check_layout_save(&recreate_v),
        Err(v_refusal())
    );
    let recreated_v = m1_3_edit(users_loaded.model(), recreate_v);
    let (refused_v, sink) =
        m1_3_route(&users_loaded.snapshot(&recreated_v), &SavePolicy::default());
    assert_eq!(refused_v, Err(v_refusal()));
    assert!(sink.is_empty(), "a refused save writes no byte");
    // Creating H again over A3 replaces its own locator, which the check admits.
    let recreate_h = vec![
        m2_1_op(SkinLayoutEdit::RemoveBlock {
            block: BlockId::new(h_id).expect("an ID"),
        }),
        m2_1_op(SkinLayoutEdit::CreateBlock {
            block: BlockId::new(h_id).expect("an ID"),
            kind: BlockKind::Calculation,
            anchor: GridArea::cell(cell(users_numbers, 3, 1)),
        }),
    ];
    assert_eq!(users_loaded.check_layout_save(&recreate_h), Ok(()));
    let recreated_h = m1_3_edit(users_loaded.model(), recreate_h);
    let (h_saved, _) = m1_3_save(&users_loaded.snapshot(&recreated_h));
    let h_zip = parse_zip(&h_saved);
    let h_names = m2_1_defined_names(&m1_3_content(&h_zip, &h_saved, "xl/workbook.xml"));
    let h_rows: Vec<(String, Option<String>, Option<String>, String)> = h_names
        .into_iter()
        .map(|(name, local, hidden, text, _)| (name, local, hidden, text))
        .collect();
    let row = |name: &str, local: Option<&str>, hidden: Option<&str>, text: &str| {
        (
            name.to_owned(),
            local.map(str::to_owned),
            hidden.map(str::to_owned),
            text.to_owned(),
        )
    };
    assert_eq!(
        h_rows,
        vec![
            row("Rate", None, None, "Numbers!$A$1"),
            row("Secret", None, Some("1"), "Numbers!$A$1"),
            row(&m2_1_locator(v_id), None, None, "Numbers!$A$1"),
            row(&m2_1_locator(s_id), Some("0"), Some("1"), "Numbers!$A$1"),
            row(&m2_1_locator(h_id), Some("0"), Some("1"), "Numbers!$A$2"),
            row(&m2_1_locator(h_id), None, Some("1"), "'Numbers'!$A$3"),
        ]
    );
    note!(
        "ACCEPT-NOTE: {CASE}: of five blocks whose locators are a user's visible Rate and hidden Secret, a visible and a sheet-scoped name of a locator's form, and a name held both sheet-scoped and hidden workbook-scoped, only the last resolved (to its workbook-scoped A1) and the others warned UnresolvedLocator; removing all five removed only that hidden locator and kept the other five definedName elements byte for byte; re-creating the block whose name a visible definedName holds was refused LocatorNameTaken by check_layout_save and by the save, with nothing written; re-creating H over A3 replaced its own locator"
    );

    // A workbook part without definedNames: a block's save inserts the element before calcPr, as
    // the schema orders them; removing the block again removes the element, and the workbook
    // part is the source's byte for byte.
    let bare = m2_1_built(r#"<calcPr calcId="191029"/>"#, None, None);
    let bare_loaded = m2_1_load(&bare);
    let bare_numbers = bare_loaded.model().sheets()[0].id();
    let n_id = m(0x31);
    let bare_edit = m1_3_edit(
        bare_loaded.model(),
        vec![
            m2_1_op(SkinLayoutEdit::CreateBlock {
                block: BlockId::new(n_id).expect("an ID"),
                kind: BlockKind::Calculation,
                anchor: GridArea::cell(cell(bare_numbers, 1, 1)),
            }),
            m2_1_op(SkinLayoutEdit::AddPlacement {
                placement: q_id(0x71),
                block: BlockId::new(n_id).expect("an ID"),
                spot: notebook_spot(1),
            }),
        ],
    );
    let (bare_saved, _) = m1_3_save(&bare_loaded.snapshot(&bare_edit));
    let bare_zip = parse_zip(&bare_saved);
    let bare_source = parse_zip(&bare);
    let bare_source_workbook =
        String::from_utf8(m1_3_content(&bare_source, &bare, "xl/workbook.xml")).expect("UTF-8");
    let bare_workbook =
        String::from_utf8(m1_3_content(&bare_zip, &bare_saved, "xl/workbook.xml")).expect("UTF-8");
    assert_eq!(
        bare_workbook,
        bare_source_workbook.replace(
            r#"<calcPr calcId="191029"/>"#,
            &format!(
                r#"<definedNames><definedName name="{}" hidden="1">'Numbers'!$A$1</definedName></definedNames><calcPr calcId="191029"/>"#,
                m2_1_locator(n_id)
            )
        ),
        "definedNames inserted before calcPr, everything else as written"
    );
    let bare_reopened = m2_1_load(&bare_saved);
    assert_eq!(bare_reopened.layout_warnings(), &[] as &[LayoutWarning]);
    let bare_unplaced = m1_3_edit(
        bare_reopened.model(),
        vec![
            m2_1_op(SkinLayoutEdit::RemovePlacement {
                placement: q_id(0x71),
            }),
            m2_1_op(SkinLayoutEdit::RemoveBlock {
                block: BlockId::new(n_id).expect("an ID"),
            }),
        ],
    );
    let (bare_resaved, _) = m1_3_save(&bare_reopened.snapshot(&bare_unplaced));
    let bare_rezip = parse_zip(&bare_resaved);
    assert_eq!(
        String::from_utf8(m1_3_content(&bare_rezip, &bare_resaved, "xl/workbook.xml"))
            .expect("UTF-8"),
        bare_source_workbook,
        "the last locator's removal removes definedNames: the source's workbook part"
    );
    assert!(
        m2_1_load(&bare_resaved).model().layout().is_empty(),
        "no block after the removal"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: a workbook part without definedNames got one before calcPr holding the new block's hidden locator, every other byte as written; removing that block removed the element, leaving the source's workbook part byte for byte"
    );

    // The save reads the workbook part's definedNames for every rewrite of the part. A cell-only
    // save does not rewrite the part (it asks for no full calculation, dc-35p), so a part with two
    // definedNames elements, which loads, is copied as written.
    let doubled = m2_1_built(
        r#"<definedNames><definedName name="One">Numbers!$A$1</definedName></definedNames><definedNames><definedName name="Two">Numbers!$A$1</definedName></definedNames>"#,
        None,
        None,
    );
    let doubled_loaded = m2_1_load(&doubled);
    assert_eq!(doubled_loaded.defined_names_not_loaded(), 2);
    let doubled_edit = m1_3_edit(
        doubled_loaded.model(),
        vec![set(
            doubled_loaded.model().sheets()[0].id(),
            2,
            1,
            number(2.0),
        )],
    );
    let (doubled_saved, _) = m1_3_save(&doubled_loaded.snapshot(&doubled_edit));
    assert_eq!(
        m1_3_content(
            &parse_zip(&doubled_saved),
            &doubled_saved,
            "xl/workbook.xml"
        ),
        m1_3_content(&parse_zip(&doubled), &doubled, "xl/workbook.xml"),
        "the workbook part with two definedNames elements is copied as written"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: a workbook part with two definedNames elements loaded, and a cell-only save of it copied the part as written (the cell save path no longer rewrites the workbook part, dc-35p)"
    );

    // A manifest with a namespace prefix, comments between children, and unknown children between
    // known ones: a save of a layout edit writes new elements with the prefix, keeps every
    // comment and unknown child in its place among the known children, drops a moved tree
    // placement's parent, and adds a view the manifest lacked after the last one it holds. The
    // whole saved manifest is compared with text written here.
    let (b1, b2, b3) = (m(0x41), m(0x42), m(0x43));
    let (t1, t2, n1, n2, n3, c1) = (q(0x81), q(0x82), q(0x83), q(0x84), q(0x85), q(0x86));
    let id = |value: u128| m2_1_uuid(value);
    let d_block = |block: u128, kind: &str| {
        format!(
            r#"<d:block id="{}" kind="{kind}" locator="{}"/>"#,
            id(block),
            m2_1_locator(block)
        )
    };
    let d_placement = |placement: u128, block: u128, rest: &str| {
        format!(
            r#"<d:placement id="{}" block="{}"{rest}/>"#,
            id(placement),
            id(block)
        )
    };
    let declaration = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
    let root_tag = format!(r#"<d:manifest xmlns:d="{DNA_NAMESPACE}" version="1">"#);
    let prefixed_source = format!(
        "{declaration}{root_tag}\n<d:blocks>\n<!-- first -->{}\n<d:note>between</d:note>\n{}\n</d:blocks>\n<d:views>\n<d:tree>{}<!-- c -->{}<x:mark xmlns:x=\"urn:example:ext\"/></d:tree>\n<d:notebook>{}<d:extra/>{}</d:notebook>\n</d:views>\n</d:manifest>",
        d_block(b1, "prose"),
        d_block(b2, "calculation"),
        d_placement(t1, b1, ""),
        d_placement(t2, b2, &format!(r#" parent="{}""#, id(t1))),
        d_placement(n1, b1, ""),
        d_placement(n2, b2, ""),
    );
    let locators = |blocks: &[u128]| {
        let mut out = String::from("<definedNames>");
        for &block in blocks {
            out.push_str(&format!(
                r#"<definedName name="{}" hidden="1">Numbers!$A$1</definedName>"#,
                m2_1_locator(block)
            ));
        }
        out + "</definedNames>"
    };
    let prefixed = m2_1_built(&locators(&[b1, b2]), Some(&prefixed_source), None);
    let prefixed_loaded = m2_1_load(&prefixed);
    assert_eq!(prefixed_loaded.layout_warnings(), &[] as &[LayoutWarning]);
    let p_numbers = prefixed_loaded.model().sheets()[0].id();
    let b_id = |value: u128| BlockId::new(value).expect("an ID");
    let p_id = |value: u128| PlacementId::new(value).expect("an ID");
    let prefixed_edit = m1_3_edit(
        prefixed_loaded.model(),
        vec![
            m2_1_op(SkinLayoutEdit::MovePlacement {
                placement: p_id(t2),
                spot: Spot::Tree {
                    parent: None,
                    position: position(1),
                },
            }),
            m2_1_op(SkinLayoutEdit::MovePlacement {
                placement: p_id(n2),
                spot: notebook_spot(1),
            }),
            m2_1_op(SkinLayoutEdit::CreateBlock {
                block: b_id(b3),
                kind: BlockKind::Calculation,
                anchor: GridArea::cell(cell(p_numbers, 1, 1)),
            }),
            m2_1_op(SkinLayoutEdit::AddPlacement {
                placement: p_id(n3),
                block: b_id(b3),
                spot: notebook_spot(3),
            }),
            m2_1_op(SkinLayoutEdit::AddPlacement {
                placement: p_id(c1),
                block: b_id(b3),
                spot: canvas_spot(
                    1,
                    [
                        1f64.to_bits(),
                        2f64.to_bits(),
                        3f64.to_bits(),
                        4f64.to_bits(),
                    ],
                ),
            }),
        ],
    );
    let (prefixed_saved, _) = m1_3_save(&prefixed_loaded.snapshot(&prefixed_edit));
    let prefixed_zip = parse_zip(&prefixed_saved);
    let prefixed_item = String::from_utf8(m1_3_content(
        &prefixed_zip,
        &prefixed_saved,
        "customXml/item1.xml",
    ))
    .expect("UTF-8");
    let canvas_card = d_placement(c1, b3, r#" x="1" y="2" width="3" height="4""#);
    let expected_item = format!(
        "{declaration}{root_tag}\n<d:blocks>\n<!-- first -->{}\n<d:note>between</d:note>\n{}{}\n</d:blocks>\n<d:views>\n<d:tree>{}<!-- c -->{}<x:mark xmlns:x=\"urn:example:ext\"/></d:tree>\n<d:notebook>{}<d:extra/>{}{}</d:notebook><d:canvas>{canvas_card}</d:canvas>\n</d:views>\n</d:manifest>",
        d_block(b1, "prose"),
        d_block(b2, "calculation"),
        d_block(b3, "calculation"),
        d_placement(t2, b2, ""),
        d_placement(t1, b1, ""),
        d_placement(n2, b2, ""),
        d_placement(n1, b1, ""),
        d_placement(n3, b3, ""),
    );
    assert_eq!(
        prefixed_item, expected_item,
        "the rewritten prefixed manifest"
    );
    let prefixed_reopened = m2_1_load(&prefixed_saved);
    assert_eq!(prefixed_reopened.layout_warnings(), &[] as &[LayoutWarning]);
    assert_eq!(
        m2_1_layout(prefixed_reopened.model()),
        m2_1_layout(&prefixed_edit),
        "the prefixed manifest reopens to the edited layout"
    );
    // Removing B1 with its two placements: each remaining block and placement takes the next
    // place in order, and the unknown children and comments stay where they were among them.
    let prefixed_removed = m1_3_edit(
        prefixed_reopened.model(),
        vec![
            m2_1_op(SkinLayoutEdit::RemovePlacement {
                placement: p_id(t1),
            }),
            m2_1_op(SkinLayoutEdit::RemovePlacement {
                placement: p_id(n1),
            }),
            m2_1_op(SkinLayoutEdit::RemoveBlock { block: b_id(b1) }),
        ],
    );
    let (removed_saved, _) = m1_3_save(&prefixed_reopened.snapshot(&prefixed_removed));
    let removed_zip = parse_zip(&removed_saved);
    let removed_item = String::from_utf8(m1_3_content(
        &removed_zip,
        &removed_saved,
        "customXml/item1.xml",
    ))
    .expect("UTF-8");
    let expected_removed = format!(
        "{declaration}{root_tag}\n<d:blocks>\n<!-- first -->{}\n<d:note>between</d:note>\n{}\n</d:blocks>\n<d:views>\n<d:tree>{}<!-- c --><x:mark xmlns:x=\"urn:example:ext\"/></d:tree>\n<d:notebook>{}<d:extra/>{}</d:notebook><d:canvas>{canvas_card}</d:canvas>\n</d:views>\n</d:manifest>",
        d_block(b2, "calculation"),
        d_block(b3, "calculation"),
        d_placement(t2, b2, ""),
        d_placement(n2, b2, ""),
        d_placement(n3, b3, ""),
    );
    assert_eq!(
        removed_item, expected_removed,
        "the manifest after B1's removal"
    );
    assert_eq!(
        m2_1_layout(m2_1_load(&removed_saved).model()),
        m2_1_layout(&prefixed_removed)
    );
    assert_eq!(
        String::from_utf8(m1_3_content(
            &removed_zip,
            &removed_saved,
            "xl/workbook.xml"
        ))
        .expect("UTF-8")
        .matches("<definedName ")
        .count(),
        2,
        "B1's locator removed, B2's and B3's kept"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: a manifest written with the prefix d:, comments between children and unknown children between known ones was rewritten after a tree placement moved to the root (its parent attribute dropped), a notebook placement moved first, and a block added with a notebook placement and a new canvas view: the saved manifest equals the text written here, new elements prefixed and every comment and unknown child in its place; after removing a block with its two placements it again equals the text written here"
    );

    // The model's own refusals of a layout built directly.
    let anchored = |id: u128, locator: &str, anchor| {
        Block::new(
            BlockId::new(id).expect("an ID"),
            BlockKind::Prose,
            LocatorName::new(locator).expect("a locator name"),
            anchor,
        )
    };
    assert_eq!(
        SkinLayout::new(
            vec![anchored(m(1), "Same", None), anchored(m(2), "SAME", None)],
            Vec::new()
        ),
        Err(LayoutError::LocatorInUse {
            block: m_id(2),
            other: m_id(1),
        }),
        "locator names compare ignoring ASCII case"
    );
    assert_eq!(
        SkinLayout::new(
            vec![anchored(m(1), "One", None)],
            vec![
                Placement::tree(q_id(1), m_id(1), Some(q_id(2))),
                Placement::tree(q_id(2), m_id(1), Some(q_id(1))),
            ]
        ),
        Err(LayoutError::ParentCycle {
            placement: q_id(1),
            parent: q_id(2),
        })
    );
    let stranger = session(0).stranger;
    let elsewhere = SkinLayout::new(
        vec![anchored(m(1), "One", Some(GridArea::full_sheet(stranger)))],
        Vec::new(),
    )
    .expect("a layout");
    assert_eq!(
        ModelRoot::with_layout(AuthoredRevision::INITIAL, Vec::new(), Vec::new(), elsewhere),
        Err(ModelRootError::BlockOnUnknownSheet(m_id(1)))
    );
    for (label, text, ok) in [
        (
            "a canonical ID",
            "0f3e7a21-5c6b-4d8e-a1f2-3b4c5d6e7f01",
            true,
        ),
        (
            "upper-case digits",
            "0F3E7A21-5C6B-4D8E-A1F2-3B4C5D6E7F01",
            false,
        ),
        ("no hyphens", "0f3e7a215c6b4d8ea1f23b4c5d6e7f01", false),
        ("braces", "{0f3e7a21-5c6b-4d8e-a1f2-3b4c5d6e7f01}", false),
        ("the nil ID", "00000000-0000-0000-0000-000000000000", false),
    ] {
        assert_eq!(BlockId::parse(text).is_some(), ok, "{label}");
    }
    for (label, text, ok) in [
        (
            "a derived name",
            "_dna.block.0f3e7a215c6b4d8ea1f23b4c5d6e7f01",
            true,
        ),
        ("a cell address", "AB12", false),
        ("an R1C1 address", "R2C3", false),
        ("a space", "a name", false),
        ("a leading digit", "1name", false),
    ] {
        assert_eq!(LocatorName::new(text).is_some(), ok, "{label}");
    }

    // The wire codec refuses a layout operation it does not admit.
    let layout_envelope = |op: &str, args: &str| batch_of(&[envelope("skin_layout", op, args)]);
    let id1 = "\"5e1f0c3a-7b2d-4c8e-9a41-0d6b3f2e8c01\"";
    let wire_refusals: Vec<(&str, Vec<u8>, DecodeError)> = vec![
        (
            "an upper-case ID",
            layout_envelope(
                "remove_block",
                "\"block\":\"5E1F0C3A-7B2D-4C8E-9A41-0D6B3F2E8C01\"",
            ),
            DecodeError::Malformed {
                at: at(".args.block"),
                expected: "a nonzero ID as 32 lowercase hex digits in groups of 8, 4, 4, 4 and 12 joined by -",
            },
        ),
        (
            "the nil ID",
            layout_envelope(
                "remove_block",
                "\"block\":\"00000000-0000-0000-0000-000000000000\"",
            ),
            DecodeError::Malformed {
                at: at(".args.block"),
                expected: "a nonzero ID as 32 lowercase hex digits in groups of 8, 4, 4, 4 and 12 joined by -",
            },
        ),
        (
            "an unknown kind",
            layout_envelope(
                "create_block",
                &format!(
                    "\"block\":{id1},\"kind\":\"chart\",\"sheet\":1,\"top\":1,\"left\":1,\"bottom\":1,\"right\":1"
                ),
            ),
            DecodeError::Malformed {
                at: at(".args.kind"),
                expected: "\"prose\" or \"calculation\"",
            },
        ),
        (
            "a spot naming two views",
            layout_envelope(
                "move_placement",
                &format!(
                    "\"placement\":{id1},\"spot\":{{\"notebook\":{{\"position\":1}},\"grid\":{{\"position\":1}}}}"
                ),
            ),
            DecodeError::Malformed {
                at: at(".args.spot"),
                expected: "exactly one of notebook, tree, canvas or grid",
            },
        ),
        (
            "a spot naming no known view",
            layout_envelope(
                "move_placement",
                &format!("\"placement\":{id1},\"spot\":{{\"timeline\":{{\"position\":1}}}}"),
            ),
            DecodeError::UnknownField {
                at: at(".args.spot"),
                field: "timeline".to_owned(),
            },
        ),
        (
            "a tree spot without its parent",
            layout_envelope(
                "move_placement",
                &format!("\"placement\":{id1},\"spot\":{{\"tree\":{{\"position\":1}}}}"),
            ),
            DecodeError::MissingField {
                at: at(".args.spot.tree"),
                field: "parent".to_owned(),
            },
        ),
        (
            "a tree parent that is a number",
            layout_envelope(
                "move_placement",
                &format!(
                    "\"placement\":{id1},\"spot\":{{\"tree\":{{\"parent\":1,\"position\":1}}}}"
                ),
            ),
            DecodeError::Malformed {
                at: at(".args.spot.tree.parent"),
                expected: "null, or a nonzero ID as a UUID is written",
            },
        ),
        (
            "a position of 0",
            layout_envelope(
                "move_placement",
                &format!("\"placement\":{id1},\"spot\":{{\"grid\":{{\"position\":0}}}}"),
            ),
            DecodeError::Malformed {
                at: at(".args.spot.grid.position"),
                expected: "an integer from 1 to 4294967295",
            },
        ),
        (
            "an infinite canvas coordinate",
            layout_envelope(
                "move_placement",
                &format!(
                    "\"placement\":{id1},\"spot\":{{\"canvas\":{{\"position\":1,\"x\":\"7ff0000000000000\",\"y\":\"0000000000000000\",\"width\":\"0000000000000000\",\"height\":\"0000000000000000\"}}}}"
                ),
            ),
            DecodeError::NonFiniteNumber {
                at: at(".args.spot.canvas.x"),
                bits: 0x7ff0_0000_0000_0000,
            },
        ),
        (
            "an inverted anchor",
            layout_envelope(
                "create_block",
                &format!(
                    "\"block\":{id1},\"kind\":\"prose\",\"sheet\":1,\"top\":2,\"left\":1,\"bottom\":1,\"right\":1"
                ),
            ),
            DecodeError::InvertedArea { at: at(".args") },
        ),
    ];
    let fixture_root = session(0).root;
    for (label, bytes, expected) in &wire_refusals {
        assert_refused(label, bytes, &fixture_root, expected);
    }
    note!(
        "ACCEPT-NOTE: {CASE}: prepare_edit refused {} layout batches, each with its typed ApplyError::Layout (DuplicateBlock, UnknownBlock twice, BlockPlaced, DuplicatePlacement, UnknownPlacement twice, OtherView, ParentNotInTree, ParentCycle twice, HasChildren, PositionPastEnd twice) or ApplyError::UnknownSheet and its operation's index, leaving the loaded revision as it was; a block whose derived locator name an existing block's locator holds was refused (LocatorInUse) through load and prepare_edit; a locator name the workbook already holds (in another case) was refused at prepare_save (LocatorNameTaken) with nothing written; SkinLayout::new refused two locators equal ignoring case and a tree cycle, ModelRoot::with_layout an anchor on a sheet it does not hold; the decoder refused {} malformed skin_layout messages",
        refusals.len(),
        wire_refusals.len()
    );
}

// ---------------------------------------------------------------------------------------------
// Case cdoc_m2_1__unknown_version.

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn cdoc_m2_1__unknown_version() {
    const CASE: &str = "cdoc_m2_1__unknown_version";
    let manifest: Value = serde_json::from_str(M2_1_MANIFEST).expect("the generator's manifest");
    let record = m1_2_record(&manifest, "manifest-v2.xlsx");
    let part = "customXml/item2.xml";

    // A manifest of version 2: the workbook loads with every cell and formula the generator wrote,
    // an empty layout, and the typed recovery warning instead of the views.
    let v2 = m2_1_load(M2_1_V2);
    assert_eq!(
        v2.layout_warnings(),
        &[LayoutWarning::UnsupportedVersion {
            part: part.to_owned(),
            version: Some("2".to_owned()),
        }]
    );
    assert!(v2.model().layout().is_empty(), "no layout is invented");
    assert_eq!(v2.manifest_part(), Some(part));
    assert_eq!(m2_1_cells(v2.model()), m2_1_recorded_cells(record));
    let recorded_names = record["defined_names"].as_array().expect("names").len();
    assert_eq!(recorded_names, 4);
    assert_eq!(
        v2.defined_names_not_loaded(),
        recorded_names,
        "no locator of an unread manifest is loaded"
    );
    let recorded_custom: Vec<(String, u64, u64)> = record["custom_xml"]
        .as_array()
        .expect("custom XML parts")
        .iter()
        .map(|part| {
            (
                part["part"].as_str().expect("a part").to_owned(),
                part["crc32"].as_u64().expect("a CRC-32"),
                part["size"].as_u64().expect("a size"),
            )
        })
        .collect();
    let source = parse_zip(M2_1_V2);
    for (name, crc, size) in &recorded_custom {
        let entry = source.entry(name);
        assert_eq!(
            (u64::from(entry.crc32), u64::from(entry.size)),
            (*crc, *size),
            "{name}: the generator's record"
        );
    }

    // Saved as loaded, nothing is rewritten.
    let (unedited, unedited_receipt) = m1_3_save(&v2.snapshot(v2.model()));
    assert_eq!(unedited_receipt.parts_written, 0);
    let unedited_zip = parse_zip(&unedited);
    for name in m2_1_names(&source) {
        assert_eq!(
            m2_1_stored(&unedited_zip, &unedited, &name),
            m2_1_stored(&source, M2_1_V2, &name),
            "{name}"
        );
    }

    // A cell edit saves: the manifest's three parts are copied as stored, byte for byte, and every
    // defined name, the locators of the unread manifest among them, is kept as the source wrote it.
    let data = v2.model().sheets()[1].id();
    let edited = m1_3_edit(v2.model(), vec![set(data, 2, 2, number(6.0))]);
    let (saved, receipt) = m1_3_save(&v2.snapshot(&edited));
    let zip = parse_zip(&saved);
    assert_eq!(
        m2_1_names(&zip),
        m2_1_names(&source),
        "no part added or removed"
    );
    for name in [
        part,
        "customXml/itemProps2.xml",
        "customXml/_rels/item2.xml.rels",
    ] {
        assert_eq!(
            m2_1_stored(&zip, &saved, name),
            m2_1_stored(&source, M2_1_V2, name),
            "{name}: copied as stored"
        );
    }
    let workbook = m1_3_content(&zip, &saved, "xl/workbook.xml");
    let source_workbook = m1_3_content(&source, M2_1_V2, "xl/workbook.xml");
    let defined = |workbook: &[u8]| {
        parse_xml(workbook)
            .child("definedNames")
            .map(|names| names.bytes(workbook).to_vec())
    };
    assert_eq!(
        defined(&workbook),
        defined(&source_workbook),
        "definedNames as written"
    );
    assert_eq!(
        receipt.parts_written, 2,
        "the edited sheet and the sheet whose caches are dropped; the workbook part is copied"
    );
    let reopened = m2_1_load(&saved);
    assert_eq!(reopened.layout_warnings(), v2.layout_warnings());
    assert!(reopened.model().layout().is_empty());
    assert_eq!(m2_1_cells(reopened.model()), m2_1_cells(&edited));
    assert_eq!(
        reopened
            .model()
            .cell(cell(reopened.model().sheets()[1].id(), 2, 2)),
        Some(&number(6.0))
    );

    // A layout edit is prepared, and its save refused: the manifest it did not read is never
    // overwritten. Nothing is written. The check before prepare_edit gives the same refusal for
    // the layout edit and admits the cell edit, so a caller can drop the layout edit before the
    // two share a revision, and the cell edit alone then saves.
    let notebook = v2.model().sheets()[0].id();
    let create_block = WorkbookOp::EditSkinLayout(SkinLayoutEdit::CreateBlock {
        block: fixture_block(1),
        kind: BlockKind::Prose,
        anchor: GridArea::cell(cell(notebook, 1, 1)),
    });
    let not_writable = || SaveRefusal::LayoutNotWritable {
        part: part.to_owned(),
    };
    let cell_edit = set(data, 3, 2, number(4.0));
    assert_eq!(
        v2.check_layout_save(&[cell_edit.clone(), create_block.clone()]),
        Err(not_writable())
    );
    assert_eq!(
        v2.check_layout_save(std::slice::from_ref(&cell_edit)),
        Ok(())
    );
    let relaid = m1_3_edit(v2.model(), vec![create_block]);
    let (refused, sink) = m1_3_route(&v2.snapshot(&relaid), &SavePolicy::default());
    assert_eq!(refused, Err(not_writable()));
    assert!(sink.is_empty(), "a refused save writes no byte");
    let cells_only = m1_3_edit(v2.model(), vec![cell_edit]);
    let (cells_saved, _) = m1_3_save(&v2.snapshot(&cells_only));
    assert_eq!(
        m2_1_stored(&parse_zip(&cells_saved), &cells_saved, part),
        m2_1_stored(&source, M2_1_V2, part),
        "the cell edit alone saves, the manifest copied as stored"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: manifest-v2.xlsx loaded with LayoutWarning::UnsupportedVersion (version \"2\") for {part}, an empty layout and every cell the generator wrote ({} sheets); saved as loaded it rewrote nothing; after a cell edit the manifest, its item properties and its relationships were copied as stored byte for byte and definedNames (its 2 locators among the 4 names) as written, and it reopened with the same warning and the edit; check_layout_save refused a batch holding a layout edit with SaveRefusal::LayoutNotWritable and admitted the cell edit alone, a layout edit's save was refused with the same refusal and nothing written, and the cell edit alone saved",
        m2_1_cells(v2.model()).len()
    );

    // A version 1 manifest that cannot be read: one placement names a block it does not hold.
    let unreadable = m2_1_load(M2_1_UNREADABLE);
    let unreadable_record = m1_2_record(&manifest, "manifest-unreadable.xlsx");
    match unreadable.layout_warnings() {
        [
            LayoutWarning::UnreadableManifest {
                part: found,
                detail,
            },
        ] => {
            assert_eq!(found, part);
            assert!(
                detail.contains("6c5d4e3f-2a1b-4c0d-8e9f-a0b1c2d3e4f9"),
                "the detail names the missing block: {detail}"
            );
        }
        other => panic!("the warnings {other:?}"),
    }
    assert!(unreadable.model().layout().is_empty());
    assert_eq!(
        m2_1_cells(unreadable.model()),
        m2_1_recorded_cells(unreadable_record)
    );
    let unreadable_edit = m1_3_edit(
        unreadable.model(),
        vec![set(unreadable.model().sheets()[1].id(), 3, 2, number(8.0))],
    );
    let (unreadable_saved, _) = m1_3_save(&unreadable.snapshot(&unreadable_edit));
    let unreadable_zip = parse_zip(&unreadable_saved);
    let unreadable_source = parse_zip(M2_1_UNREADABLE);
    assert_eq!(
        m2_1_stored(&unreadable_zip, &unreadable_saved, part),
        m2_1_stored(&unreadable_source, M2_1_UNREADABLE, part),
        "the unreadable manifest is copied as stored"
    );

    // Packages built here: a manifest element without a version, two manifests, and a version 1
    // manifest that is not well formed. Each loads its one cell with a warning and no layout.
    let dna = |inner: &str| format!(r#"<manifest xmlns="{DNA_NAMESPACE}"{inner}</manifest>"#);
    let no_version = m2_1_built("", Some(&dna("><blocks/>")), None);
    let two = m2_1_built(
        "",
        Some(&dna(r#" version="1">"#)),
        Some(&dna(r#" version="1">"#)),
    );
    let broken = m2_1_built(
        "",
        Some(&format!(
            r#"<manifest xmlns="{DNA_NAMESPACE}" version="1"><blocks><block></blocks></manifest>"#
        )),
        None,
    );
    let mut kinds = Vec::new();
    for (label, package) in [
        ("no version", &no_version),
        ("two", &two),
        ("broken", &broken),
    ] {
        let loaded = m2_1_load(package);
        let numbers = loaded.model().sheets()[0].id();
        assert_eq!(
            loaded.model().cell(cell(numbers, 1, 1)),
            Some(&number(1.0)),
            "{label}: the cell"
        );
        assert!(loaded.model().layout().is_empty(), "{label}: no layout");
        match loaded.layout_warnings() {
            [LayoutWarning::UnsupportedVersion { part, version }] => {
                assert_eq!(label, "no version");
                assert_eq!((part.as_str(), version), ("customXml/item1.xml", &None));
                kinds.push("UnsupportedVersion");
            }
            [LayoutWarning::UnreadableManifest { part, detail }] => {
                assert_eq!(part, "customXml/item1.xml", "{label}");
                if label == "two" {
                    assert!(detail.contains("customXml/item2.xml"), "{detail}");
                } else {
                    assert_eq!(label, "broken");
                }
                kinds.push("UnreadableManifest");
            }
            other => panic!("{label}: the warnings {other:?}"),
        }
    }
    note!(
        "ACCEPT-NOTE: {CASE}: manifest-unreadable.xlsx loaded with LayoutWarning::UnreadableManifest naming the missing block, its cells and an empty layout, and a cell edit's save copied the manifest as stored; packages built here with a manifest without a version, two manifests and a malformed manifest loaded their cell with {kinds:?} and no layout"
    );

    // DNA manifests the XML reader refuses before their document element, built here: one in
    // UTF-16, one declaring another encoding, and one whose document element's start tag is
    // malformed. Each still counts as the package's DNA manifest: it loads with a typed warning
    // naming its part, a layout save is refused before any byte is written (no second manifest
    // is added), and a cell edit's save copies it as stored.
    let dna_text =
        format!(r#"<manifest xmlns="{DNA_NAMESPACE}" version="1"><blocks/><views/></manifest>"#);
    let unreadable_items: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "UTF-16",
            m2_1_utf16le(&format!(
                r#"<?xml version="1.0" encoding="UTF-16"?>{dna_text}"#
            )),
            "UTF-16",
        ),
        (
            "ISO-8859-1",
            format!(r#"<?xml version="1.0" encoding="ISO-8859-1"?>{dna_text}"#).into_bytes(),
            "ISO-8859-1",
        ),
        (
            "a malformed start tag",
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><manifest xmlns="{DNA_NAMESPACE}" version=1><blocks/></manifest>"#
            )
            .into_bytes(),
            "malformed XML",
        ),
    ];
    let item1 = "customXml/item1.xml";
    for (label, item, reason) in &unreadable_items {
        let package = m2_1_built_bytes("", std::slice::from_ref(item));
        let loaded = m2_1_load(&package);
        match loaded.layout_warnings() {
            [LayoutWarning::UnreadableManifest { part, detail }] => {
                assert_eq!(part, item1, "{label}");
                assert!(
                    detail.contains(reason) && detail.contains("manifest's namespace"),
                    "{label}: {detail}"
                );
            }
            other => panic!("{label}: the warnings {other:?}"),
        }
        assert_eq!(loaded.manifest_part(), Some(item1), "{label}");
        assert!(loaded.model().layout().is_empty(), "{label}: no layout");
        let numbers = loaded.model().sheets()[0].id();
        assert_eq!(
            loaded.model().cell(cell(numbers, 1, 1)),
            Some(&number(1.0)),
            "{label}: the cell"
        );
        let layout_edit = vec![m2_1_op(SkinLayoutEdit::CreateBlock {
            block: fixture_block(1),
            kind: BlockKind::Prose,
            anchor: GridArea::cell(cell(numbers, 1, 1)),
        })];
        let refusal = || SaveRefusal::LayoutNotWritable {
            part: item1.to_owned(),
        };
        assert_eq!(
            loaded.check_layout_save(&layout_edit),
            Err(refusal()),
            "{label}"
        );
        let relaid = m1_3_edit(loaded.model(), layout_edit);
        let (refused, sink) = m1_3_route(&loaded.snapshot(&relaid), &SavePolicy::default());
        assert_eq!(refused, Err(refusal()), "{label}");
        assert!(sink.is_empty(), "{label}: a refused save writes no byte");
        let cells = m1_3_edit(loaded.model(), vec![set(numbers, 2, 1, number(2.0))]);
        let (saved, _) = m1_3_save(&loaded.snapshot(&cells));
        let saved_zip = parse_zip(&saved);
        let source_zip = parse_zip(&package);
        assert_eq!(
            m2_1_names(&saved_zip),
            m2_1_names(&source_zip),
            "{label}: no part added"
        );
        assert_eq!(
            m2_1_stored(&saved_zip, &saved, item1),
            m2_1_stored(&source_zip, &package, item1),
            "{label}: copied as stored"
        );
    }

    // Beside a manifest that reads, a UTF-16 one makes two DNA manifests; a UTF-16 item that does
    // not name the manifest's namespace is not one, and the package then holds none.
    let utf16_dna = &unreadable_items[0].1;
    let both = m2_1_built_bytes(
        "",
        &[
            format!(r#"<?xml version="1.0" encoding="UTF-8"?>{dna_text}"#).into_bytes(),
            utf16_dna.clone(),
        ],
    );
    match m2_1_load(&both).layout_warnings() {
        [LayoutWarning::UnreadableManifest { part, detail }] => {
            assert_eq!(part, item1);
            assert!(
                detail.contains("2 DNA manifests") && detail.contains("customXml/item2.xml"),
                "{detail}"
            );
        }
        other => panic!("the warnings {other:?}"),
    }
    let unrelated = m2_1_built_bytes(
        "",
        &[m2_1_utf16le(
            r#"<?xml version="1.0" encoding="UTF-16"?><properties xmlns="urn:example:unrelated"/>"#,
        )],
    );
    let unrelated_loaded = m2_1_load(&unrelated);
    assert_eq!(unrelated_loaded.layout_warnings(), &[] as &[LayoutWarning]);
    assert_eq!(unrelated_loaded.manifest_part(), None);
    note!(
        "ACCEPT-NOTE: {CASE}: DNA manifests the XML reader refuses before their document element ({}) each loaded with LayoutWarning::UnreadableManifest naming customXml/item1.xml and the reader's reason, their cell and no layout; check_layout_save and the save both refused a layout edit with SaveRefusal::LayoutNotWritable, nothing written and no second manifest added; a cell edit's save copied the item as stored; a readable manifest beside a UTF-16 one was reported as two DNA manifests, and a UTF-16 item outside the manifest's namespace was not taken for one",
        unreadable_items
            .iter()
            .map(|(label, ..)| *label)
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// dc-0k9 (L2): the rows `<row hidden="1">` marks load as hidden rows of the model, which SUBTOTAL
/// and AGGREGATE read through the engine; a sheet with no filter range holds them as hidden by
/// hand.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_0k9__hidden_rows_load() {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let loaded = m1_2_load(
        &m1_2_one_sheet(
            r#"<row r="1"><c r="A1"><v>1</v></c></row><row r="2" hidden="1"><c r="A2"><v>2</v></c></row><row r="4" hidden="true"/><row r="5" hidden="0"><c r="A5"><v>5</v></c></row>"#,
        ),
        &mut ids,
    )
    .expect("the package loads");
    let model = loaded.model();
    let sheet = model.sheets()[0].id();
    assert_eq!(
        model
            .definitions()
            .hidden_rows(sheet, 1, 10)
            .collect::<Vec<_>>(),
        [
            (2, calc_doc::model::RowHidden::Manual),
            (4, calc_doc::model::RowHidden::Manual)
        ]
    );
}
