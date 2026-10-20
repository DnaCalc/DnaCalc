//! Acceptance cases owned by the calc-engine package, each registered by its exact name in
//! `crates/calc-engine/acceptance.json`. Every case is `#[test]` natively and
//! `#[wasm_bindgen_test]` on wasm32, with no browser or Node.js configuration of its own:
//! `cargo xtask accept` forces the browser with `WASM_BINDGEN_USE_BROWSER`.
//!
//! The cases drive the engine through its public route over calc-doc-model snapshots: an edit is
//! prepared against the base root with `prepare_edit`, calc-doc-model makes the next root with
//! `ModelRoot::apply`, and the prepared change commits with `commit_edit`. What a formula node
//! reads is stated with `EngineState::set_prerequisites`. In the product CalcFml's plan bindings
//! are to state it, one per kind of prerequisite: cells and rectangles with ceng-m1.2 (direct
//! references and ranges), whole rows or columns with ceng-m3.1 (whole-axis ranges), and sheet
//! spans with cfml-m3.2 (3D span binding) and ceng-m3.5 (span calculation). Here the
//! prerequisites are written by hand and do not come from the formula texts, which the engine
//! never reads.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_doc_model::value::{
    CellKey, ExcelText, GridArea, GridError, IdAllocator, MAX_ROWS, NodeId, SheetId,
};
use calc_doc_model::{
    AuthoredRevision, Axis, AxisChange, AxisEdit, CellInput, CellText, FiniteNumber, FormulaText,
    ModelRoot, OpBatch, SheetEdit, SheetEntry, SheetKey, SheetName, SheetVisibility, WorkbookOp,
};
use calc_engine::{
    AxisInterval, EngineState, FormulaNode, InputSeed, NodeChange, PrepareError, Prerequisite,
    PrerequisiteError, SheetSpan, commit_edit, prepare_edit,
};
use std::collections::BTreeMap;
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

// ---------------------------------------------------------------------------------------------
// The workbook: three sheets in the order Gamma, Alpha, Beta, whose IDs were issued in the order
// Alpha, Beta, Gamma, plus one more issued ID that names no sheet of the workbook.

struct Workbook {
    alpha: SheetId,
    beta: SheetId,
    gamma: SheetId,
    absent: SheetId,
    sheets: Vec<SheetEntry>,
}

impl Workbook {
    fn new() -> Self {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let alpha = ids.issue().unwrap();
        let beta = ids.issue().unwrap();
        let gamma = ids.issue().unwrap();
        let absent = ids.issue().unwrap();
        let entry = |id, key, name: &str| {
            SheetEntry::new(
                id,
                SheetKey::new(key),
                SheetName::new(ExcelText::from_interop_assignment(name)).unwrap(),
                SheetVisibility::Visible,
            )
        };
        let sheets = vec![
            entry(gamma, 3, "Gamma"),
            entry(alpha, 1, "Alpha"),
            entry(beta, 2, "Beta"),
        ];
        Workbook {
            alpha,
            beta,
            gamma,
            absent,
            sheets,
        }
    }

    /// A model root of these sheets with no cells, at `revision`.
    fn root(&self, revision: u64) -> Arc<ModelRoot> {
        ModelRoot::new(AuthoredRevision::new(revision), self.sheets.clone()).unwrap()
    }
}

fn key(sheet: SheetId, row: u32, column: u32) -> CellKey {
    CellKey::new(sheet, row, column).unwrap()
}

fn area(sheet: SheetId, top: u32, left: u32, bottom: u32, right: u32) -> GridArea {
    GridArea::new(key(sheet, top, left), key(sheet, bottom, right)).unwrap()
}

fn formula(text: &str) -> CellInput {
    CellInput::Formula(FormulaText::new(ExcelText::from_interop_assignment(text)).unwrap())
}

fn number(value: f64) -> CellInput {
    CellInput::Number(FiniteNumber::new(value).unwrap())
}

fn text(value: &str) -> CellInput {
    CellInput::Text(CellText::new(ExcelText::from_interop_assignment(value)).unwrap())
}

fn set(at: CellKey, input: CellInput) -> WorkbookOp {
    WorkbookOp::SetCell { at, input }
}

fn clear(area: GridArea) -> WorkbookOp {
    WorkbookOp::Clear { area }
}

fn batch(base: u64, operations: Vec<WorkbookOp>) -> OpBatch {
    OpBatch {
        base: AuthoredRevision::new(base),
        operations,
    }
}

fn rev(value: u64) -> AuthoredRevision {
    AuthoredRevision::new(value)
}

/// The dependents each seed of a prepared change lists, in seed order.
fn seed_dependents(seeds: &[InputSeed<'_>]) -> Vec<Vec<NodeId>> {
    seeds
        .iter()
        .map(|seed| match seed {
            InputSeed::Cell { dependents, .. } | InputSeed::Area { dependents, .. } => {
                dependents.clone()
            }
        })
        .collect()
}

/// The dependents of each cell seed by its cell; panics on an area seed.
fn cell_seed_map(seeds: &[InputSeed<'_>]) -> BTreeMap<CellKey, Vec<NodeId>> {
    seeds
        .iter()
        .map(|seed| match seed {
            InputSeed::Cell { at, dependents, .. } => (*at, dependents.clone()),
            InputSeed::Area { area, .. } => panic!("unexpected area seed {area:?}"),
        })
        .collect()
}

fn anchors(state: &EngineState) -> Vec<CellKey> {
    state.nodes().map(FormulaNode::anchor).collect()
}

/// The readers of each cell as `prepare_edit` stages them for one batch that sets the literal 1
/// at every cell; each cell must hold some other input in `inputs`, or none. The prepared change
/// is dropped.
fn readers(
    state: &mut EngineState,
    inputs: &ModelRoot,
    cells: &[CellKey],
) -> BTreeMap<CellKey, Vec<NodeId>> {
    let probe = batch(
        inputs.revision().get(),
        cells.iter().map(|at| set(*at, number(1.0))).collect(),
    );
    let prepared = prepare_edit(state, inputs, &probe).unwrap();
    let found = cell_seed_map(prepared.seeds());
    assert_eq!(found.len(), cells.len(), "a probed cell already held 1");
    found
}

/// Each area with the readers its seed lists, as `prepare_edit` stages them for one batch that
/// clears every area in order; each area must empty a cell of `inputs`. The prepared change is
/// dropped.
fn clear_readers(
    state: &mut EngineState,
    inputs: &ModelRoot,
    areas: &[GridArea],
) -> Vec<(GridArea, Vec<NodeId>)> {
    let probe = batch(
        inputs.revision().get(),
        areas.iter().map(|area| clear(*area)).collect(),
    );
    let prepared = prepare_edit(state, inputs, &probe).unwrap();
    let found: Vec<(GridArea, Vec<NodeId>)> = prepared
        .seeds()
        .iter()
        .map(|seed| match seed {
            InputSeed::Area { area, dependents } => (*area, dependents.clone()),
            InputSeed::Cell { at, .. } => panic!("unexpected cell seed {at:?}"),
        })
        .collect();
    assert_eq!(found.len(), areas.len(), "a probed area emptied no cell");
    found
}

// ---------------------------------------------------------------------------------------------
// ceng_m1_1__two_sheets_one_graph

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn ceng_m1_1__two_sheets_one_graph() {
    const CASE: &str = "ceng_m1_1__two_sheets_one_graph";
    let book = Workbook::new();
    let (alpha, beta, gamma) = (book.alpha, book.beta, book.gamma);
    let root7 = book.root(7);
    let mut state = EngineState::new(&root7);
    assert_eq!(state.revision(), rev(7));
    assert_eq!(state.node_count(), 0);

    // 1. One batch adds a formula at A1 of Alpha and at A1 of Beta (equal rows and columns) and
    //    sets a literal at Alpha!B2. Preparing reserves two different node IDs and stages three
    //    cell seeds; committing records the two nodes under those IDs.
    let a1_alpha = key(alpha, 1, 1);
    let a1_beta = key(beta, 1, 1);
    let b2_alpha = key(alpha, 2, 2);
    let first = batch(
        7,
        vec![
            set(a1_alpha, formula("Beta!B2+SUM(C1:D4)")),
            set(a1_beta, formula("Alpha!B2+SUM(Alpha!G:G)")),
            set(b2_alpha, number(5.0)),
        ],
    );
    let prepared = prepare_edit(&mut state, &root7, &first).unwrap();
    let &[
        NodeChange::Added {
            node: n_alpha,
            at: at_alpha,
        },
        NodeChange::Added {
            node: n_beta,
            at: at_beta,
        },
    ] = prepared.node_changes()
    else {
        panic!(
            "expected two added nodes, got {:?}",
            prepared.node_changes()
        );
    };
    assert_eq!((at_alpha, at_beta), (a1_alpha, a1_beta));
    assert_ne!(n_alpha, n_beta, "equal coordinates on two sheets collided");
    assert_eq!((n_alpha.slot(), n_alpha.generation()), (0, 0));
    assert_eq!((n_beta.slot(), n_beta.generation()), (1, 0));
    assert_eq!(
        prepared.seeds(),
        [
            InputSeed::Cell {
                at: a1_alpha,
                before: None,
                dependents: vec![]
            },
            InputSeed::Cell {
                at: b2_alpha,
                before: None,
                dependents: vec![]
            },
            InputSeed::Cell {
                at: a1_beta,
                before: None,
                dependents: vec![]
            },
        ]
    );
    assert_eq!((prepared.base(), prepared.next()), (rev(7), rev(8)));

    let root8 = root7.apply(&first).unwrap();
    let receipt = commit_edit(prepared);
    assert_eq!(receipt.revision(), rev(8));
    assert_eq!(state.revision(), rev(8));
    assert_eq!(receipt.added(), [(a1_alpha, n_alpha), (a1_beta, n_beta)]);
    assert_eq!(state.node_at(a1_alpha), Some(n_alpha));
    assert_eq!(state.node_at(a1_beta), Some(n_beta));
    assert_eq!(state.node_at(key(gamma, 1, 1)), None);
    assert_eq!(state.node_at(b2_alpha), None, "a literal got a node");
    assert_eq!(state.node_count(), 2);
    assert_eq!(anchors(&state), [a1_alpha, a1_beta]);
    assert_eq!(state.node(n_alpha).unwrap().anchor(), a1_alpha);
    assert_eq!(state.node(n_beta).unwrap().anchor(), a1_beta);
    assert_eq!(state.node(n_alpha).unwrap().prerequisites(), None);
    assert_eq!(anchors(&EngineState::new(&root8)), anchors(&state));
    note!("{CASE}: Alpha!A1 is {n_alpha:?} and Beta!A1 is {n_beta:?} in one state of 2 nodes");

    // 2. The edges of the one graph cross sheets. The prerequisites below are written by hand to
    //    reach each kind of edge; they do not come from the formula texts above.
    note!(
        "ACCEPT-NOTE: {CASE}: the prerequisites of Alpha!A1 and Beta!A1 are written by hand in the case and do not come from the formula text; the engine does not read formula text"
    );
    let first_binding = "the product callers of EngineState::set_prerequisites, of which ceng-m1.2's CalcFml binding of direct references and ranges is the first";
    for (item, owner) in [
        (
            "graph::Prerequisite::Cell",
            "ceng-m1.2's CalcFml binding of direct references",
        ),
        (
            "graph::Prerequisite::Area",
            "ceng-m1.2's CalcFml binding of ranges",
        ),
        (
            "graph::Prerequisite::AxisValue",
            "ceng-m3.1's binding of whole-axis ranges",
        ),
        (
            "graph::Prerequisite::SheetSpan",
            "cfml-m3.2's 3D span binding and ceng-m3.5's span calculation",
        ),
        ("state::PrerequisiteError::StaleInputs", first_binding),
        ("state::PrerequisiteError::UnknownNode", first_binding),
        ("state::PrerequisiteError::UnknownSheet", first_binding),
    ] {
        note!(
            "ACCEPT-NOTE: {CASE}: seams entry {item} has test-only traffic: no non-test code calls EngineState::set_prerequisites; its product traffic is to come from {owner}"
        );
    }
    let alpha_rows_10_12 = AxisInterval::new(alpha, Axis::Rows, 12, 10).unwrap();
    assert_eq!(
        (alpha_rows_10_12.first(), alpha_rows_10_12.last()),
        (10, 12)
    );
    let span = SheetSpan::new(area(gamma, 1, 6, 3, 6), alpha);
    state
        .set_prerequisites(
            &root8,
            n_alpha,
            vec![
                Prerequisite::Cell(key(beta, 2, 2)),
                Prerequisite::Area(area(alpha, 1, 3, 4, 4)),
                Prerequisite::AxisValue(alpha_rows_10_12),
                Prerequisite::SheetSpan(span),
            ],
        )
        .unwrap();
    // Alpha!BJ250:BN260 crosses the rectangle index's block edges between rows 256 and 257 and
    // between columns BL (64) and BM (65).
    state
        .set_prerequisites(
            &root8,
            n_beta,
            vec![
                Prerequisite::Cell(b2_alpha),
                Prerequisite::Area(area(alpha, 1, 7, MAX_ROWS, 7)),
                Prerequisite::AxisValue(AxisInterval::new(alpha, Axis::Columns, 8, 8).unwrap()),
                Prerequisite::Area(area(alpha, 250, 62, 260, 66)),
                // Gamma:Beta!J5 spans all three sheets in the order Gamma, Alpha, Beta, so Alpha
                // lies strictly between its ends.
                Prerequisite::SheetSpan(SheetSpan::new(area(gamma, 5, 10, 5, 10), beta)),
            ],
        )
        .unwrap();
    assert_eq!(
        state.node(n_beta).unwrap().prerequisites().map(<[_]>::len),
        Some(5)
    );

    // A span's sheets are the snapshot's own entries in its current order, borrowed: Gamma and
    // Alpha, not Beta, although Beta's ID lies between theirs.
    let members = span.sheets(&root8).unwrap();
    assert!(std::ptr::eq(members.as_ptr(), root8.sheets().as_ptr()));
    assert_eq!(
        members.iter().map(SheetEntry::id).collect::<Vec<_>>(),
        [gamma, alpha]
    );
    assert!(std::ptr::eq(
        SheetSpan::new(area(alpha, 1, 6, 3, 6), gamma)
            .sheets(&root8)
            .unwrap()
            .as_ptr(),
        members.as_ptr()
    ));
    assert_eq!(
        SheetSpan::new(area(alpha, 1, 1, 1, 1), book.absent).sheets(&root8),
        None
    );

    // Each probe sets one literal that differs from what the cell holds; its seed lists the
    // nodes that read that cell.
    let probes = [
        (b2_alpha, vec![n_beta], "Alpha!B2: Beta!A1's cell edge"),
        (
            key(beta, 2, 2),
            vec![n_alpha],
            "Beta!B2: Alpha!A1's cell edge",
        ),
        (key(gamma, 2, 2), vec![], "Gamma!B2: no edge"),
        (
            key(alpha, 3, 3),
            vec![n_alpha],
            "Alpha!C3: inside Alpha!C1:D4",
        ),
        (
            key(beta, 3, 3),
            vec![],
            "Beta!C3: same coordinates, other sheet",
        ),
        (
            key(alpha, 11, 26),
            vec![n_alpha],
            "Alpha!Z11: rows 10 to 12 of Alpha",
        ),
        (
            key(beta, 11, 26),
            vec![],
            "Beta!Z11: rows 10 to 12 of Alpha only",
        ),
        (key(gamma, 2, 6), vec![n_alpha], "Gamma!F2: inside the span"),
        (key(alpha, 2, 6), vec![n_alpha], "Alpha!F2: inside the span"),
        (
            key(beta, 2, 6),
            vec![],
            "Beta!F2: outside the span in sheet order",
        ),
        (key(alpha, 4, 6), vec![], "Alpha!F4: below the span's rows"),
        (
            key(alpha, 1_000_000, 7),
            vec![n_beta],
            "Alpha!G1000000: in Alpha!G:G",
        ),
        (
            key(beta, 1_000_000, 7),
            vec![],
            "Beta!G1000000: not in Alpha!G:G",
        ),
        (
            key(alpha, 5, 8),
            vec![n_beta],
            "Alpha!H5: column H of Alpha",
        ),
        (
            key(alpha, 256, 64),
            vec![n_beta],
            "Alpha!BL256: block row 0, block column 0",
        ),
        (
            key(alpha, 256, 65),
            vec![n_beta],
            "Alpha!BM256: block row 0, block column 1",
        ),
        (
            key(alpha, 257, 64),
            vec![n_beta],
            "Alpha!BL257: block row 1, block column 0",
        ),
        (
            key(alpha, 257, 65),
            vec![n_beta],
            "Alpha!BM257: block row 1, block column 1",
        ),
        (
            key(alpha, 261, 64),
            vec![],
            "Alpha!BL261: below Alpha!BJ250:BN260",
        ),
        (
            key(alpha, 256, 67),
            vec![],
            "Alpha!BO256: right of Alpha!BJ250:BN260",
        ),
        (
            key(gamma, 5, 10),
            vec![n_beta],
            "Gamma!J5: the first sheet of Gamma:Beta!J5",
        ),
        (
            key(alpha, 5, 10),
            vec![n_beta],
            "Alpha!J5: strictly between Gamma and Beta in sheet order",
        ),
        (
            key(beta, 5, 10),
            vec![n_beta],
            "Beta!J5: the last sheet of Gamma:Beta!J5",
        ),
        (
            key(alpha, 5, 11),
            vec![],
            "Alpha!K5: beside the span's area",
        ),
    ];
    let probe_batch = batch(
        8,
        probes
            .iter()
            .map(|(at, _, _)| set(*at, number(1.0)))
            .collect(),
    );
    let prepared = prepare_edit(&mut state, &root8, &probe_batch).unwrap();
    assert_eq!(prepared.node_changes(), []);
    let found = cell_seed_map(prepared.seeds());
    assert_eq!(found.len(), probes.len());
    for (at, expected, what) in &probes {
        assert_eq!(found.get(at), Some(expected), "{what} ({at:?})");
    }
    drop(prepared);
    note!(
        "{CASE}: {} single-cell probes across three sheets found the expected readers",
        probes.len()
    );

    // Literals placed so the clears below empty a cell: Alpha!D3 is inside Alpha!C1:D4, Alpha!Y10
    // on rows 10 to 12, Alpha!H20 in column H and Gamma!F1 inside the span; no node reads
    // Beta!C2, Gamma!E3 or Beta!F2.
    let fill = batch(
        8,
        vec![
            set(key(beta, 2, 3), number(11.0)),
            set(key(alpha, 10, 25), number(12.0)),
            set(key(alpha, 20, 8), number(13.0)),
            set(key(gamma, 3, 5), number(14.0)),
            set(key(beta, 2, 6), number(15.0)),
            set(key(alpha, 3, 4), number(16.0)),
            set(key(gamma, 1, 6), number(17.0)),
        ],
    );
    let prepared = prepare_edit(&mut state, &root8, &fill).unwrap();
    assert_eq!(
        cell_seed_map(prepared.seeds()),
        BTreeMap::from([
            (key(alpha, 3, 4), vec![n_alpha]),
            (key(alpha, 10, 25), vec![n_alpha]),
            (key(alpha, 20, 8), vec![n_beta]),
            (key(beta, 2, 3), vec![]),
            (key(beta, 2, 6), vec![]),
            (key(gamma, 1, 6), vec![n_alpha]),
            (key(gamma, 3, 5), vec![]),
        ])
    );
    let root9 = root8.apply(&fill).unwrap();
    assert_eq!(commit_edit(prepared).revision(), rev(9));

    // A clear seeds the nodes that read a cell it empties, through a cell edge, a rectangle,
    // whole rows, a whole column or a span, and only those: an edge that meets the cleared area
    // only at cells that were already empty seeds nothing. A clear of empty cells has no seed.
    let clears = batch(
        9,
        vec![
            clear(area(alpha, 1, 2, 2, 3)),
            clear(area(beta, 1, 2, 2, 3)),
            clear(area(alpha, 3, 4, 4, 8)),
            clear(area(alpha, 9, 24, 10, 25)),
            clear(area(alpha, 20, 8, 21, 9)),
            clear(area(gamma, 3, 5, 4, 6)),
            clear(area(gamma, 1, 6, 2, 7)),
            clear(area(beta, 1, 5, 3, 6)),
            clear(area(alpha, 100, 27, 101, 28)),
        ],
    );
    let prepared = prepare_edit(&mut state, &root9, &clears).unwrap();
    assert_eq!(prepared.node_changes(), []);
    assert_eq!(
        prepared.seeds(),
        [
            // Alpha!B1:C2 empties Alpha!B2, Beta!A1's cell edge. It meets Alpha!C1:D4 only at
            // the empty Alpha!C1:C2, so Alpha!A1 is not listed.
            InputSeed::Area {
                area: area(alpha, 1, 2, 2, 3),
                dependents: vec![n_beta]
            },
            // Beta!B1:C2 empties Beta!C2, which no node reads; Beta!B2, Alpha!A1's cell edge, is
            // inside it but empty.
            InputSeed::Area {
                area: area(beta, 1, 2, 2, 3),
                dependents: vec![]
            },
            // Alpha!D3:H4 empties Alpha!D3 inside Alpha!C1:D4. It also meets the span's Alpha!F3,
            // Alpha!G:G and column H of Alpha, at empty cells, so Beta!A1 is not listed.
            InputSeed::Area {
                area: area(alpha, 3, 4, 4, 8),
                dependents: vec![n_alpha]
            },
            // Alpha!X9:Y10 empties Alpha!Y10 on rows 10 to 12.
            InputSeed::Area {
                area: area(alpha, 9, 24, 10, 25),
                dependents: vec![n_alpha]
            },
            // Alpha!H20:I21 empties Alpha!H20 in column H.
            InputSeed::Area {
                area: area(alpha, 20, 8, 21, 9),
                dependents: vec![n_beta]
            },
            // Gamma!E3:F4 empties Gamma!E3, outside the span's column F; the span's Gamma!F3 is
            // inside it but empty.
            InputSeed::Area {
                area: area(gamma, 3, 5, 4, 6),
                dependents: vec![]
            },
            // Gamma!F1:G2 empties Gamma!F1 inside the span.
            InputSeed::Area {
                area: area(gamma, 1, 6, 2, 7),
                dependents: vec![n_alpha]
            },
            // Beta!E1:F3 empties Beta!F2, inside the span's rows and columns, but Beta is outside
            // the span.
            InputSeed::Area {
                area: area(beta, 1, 5, 3, 6),
                dependents: vec![]
            },
        ]
    );
    drop(prepared);
    note!(
        "{CASE}: 8 clears that empty a cell each listed only the readers of the cells they empty; a clear of empty cells staged no seed"
    );

    // Stating a node's prerequisites again replaces the ones it had. Alpha!A1 stops reading
    // Beta!B2, Alpha!C1:D4 and rows 10 to 12 of Alpha, keeps the span, and starts reading
    // Gamma!B2 and Beta!C1:D4 (the rows and columns of Alpha!C1:D4 on another sheet). A value
    // edit to a dropped prerequisite no longer seeds it; one to a kept or new prerequisite does.
    // A clear finds rectangles through its sheet's list of them: emptying Alpha!D3, inside the
    // dropped Alpha!C1:D4, no longer seeds Alpha!A1, and emptying Beta!C2, inside the new
    // Beta!C1:D4, does. Stating the first set again reverses each.
    let recleared = [area(alpha, 3, 4, 4, 8), area(beta, 1, 2, 2, 3)];
    let first_clears = clear_readers(&mut state, &root9, &recleared);
    assert_eq!(
        first_clears,
        [(recleared[0], vec![n_alpha]), (recleared[1], vec![])]
    );
    let reread = [
        key(beta, 2, 2),
        key(alpha, 3, 3),
        key(alpha, 11, 26),
        key(gamma, 2, 6),
        key(gamma, 2, 2),
        key(beta, 3, 3),
    ];
    let first_set = readers(&mut state, &root9, &reread);
    assert_eq!(
        first_set,
        BTreeMap::from([
            (key(beta, 2, 2), vec![n_alpha]),
            (key(alpha, 3, 3), vec![n_alpha]),
            (key(alpha, 11, 26), vec![n_alpha]),
            (key(gamma, 2, 6), vec![n_alpha]),
            (key(gamma, 2, 2), vec![]),
            (key(beta, 3, 3), vec![]),
        ])
    );
    let original = state
        .node(n_alpha)
        .unwrap()
        .prerequisites()
        .unwrap()
        .to_vec();
    let replacement = vec![
        Prerequisite::Cell(key(gamma, 2, 2)),
        Prerequisite::Area(area(beta, 1, 3, 4, 4)),
        Prerequisite::SheetSpan(span),
    ];
    state
        .set_prerequisites(&root9, n_alpha, replacement.clone())
        .unwrap();
    assert_eq!(
        state.node(n_alpha).unwrap().prerequisites(),
        Some(&replacement[..])
    );
    assert_eq!(
        readers(&mut state, &root9, &reread),
        BTreeMap::from([
            // Dropped: the cell edge, the rectangle and the whole rows.
            (key(beta, 2, 2), vec![]),
            (key(alpha, 3, 3), vec![]),
            (key(alpha, 11, 26), vec![]),
            // Kept: the span.
            (key(gamma, 2, 6), vec![n_alpha]),
            // New: the cell edge and the rectangle.
            (key(gamma, 2, 2), vec![n_alpha]),
            (key(beta, 3, 3), vec![n_alpha]),
        ])
    );
    assert_eq!(
        clear_readers(&mut state, &root9, &recleared),
        [(recleared[0], vec![]), (recleared[1], vec![n_alpha])]
    );
    state
        .set_prerequisites(&root9, n_alpha, original.clone())
        .unwrap();
    assert_eq!(
        state.node(n_alpha).unwrap().prerequisites(),
        Some(&original[..])
    );
    assert_eq!(readers(&mut state, &root9, &reread), first_set);
    assert_eq!(clear_readers(&mut state, &root9, &recleared), first_clears);
    note!(
        "{CASE}: Alpha!A1's prerequisites replaced and restated: a dropped prerequisite stopped seeding it, by value edit and by clear, and a kept or new one seeded it"
    );

    // 3. A literal replaces Beta!A1's formula and a new formula replaces Alpha!A1's. The nodes
    //    leaving the graph are not listed as readers of the cells the same batch changes.
    let third = batch(
        9,
        vec![
            set(a1_beta, number(2.0)),
            set(a1_alpha, formula("Gamma!A1")),
            set(b2_alpha, number(3.0)),
            set(key(beta, 2, 2), number(4.0)),
        ],
    );
    let prepared = prepare_edit(&mut state, &root9, &third).unwrap();
    assert_eq!(
        prepared.node_changes(),
        [
            NodeChange::Replaced {
                node: n_alpha,
                at: a1_alpha
            },
            NodeChange::Removed {
                node: n_beta,
                at: a1_beta
            },
        ]
    );
    assert_eq!(
        prepared.seeds(),
        [
            InputSeed::Cell {
                at: a1_alpha,
                before: root9.cell(a1_alpha),
                dependents: vec![]
            },
            InputSeed::Cell {
                at: b2_alpha,
                before: Some(&number(5.0)),
                dependents: vec![]
            },
            InputSeed::Cell {
                at: a1_beta,
                before: root9.cell(a1_beta),
                dependents: vec![]
            },
            InputSeed::Cell {
                at: key(beta, 2, 2),
                before: None,
                dependents: vec![]
            },
        ]
    );
    let root10 = root9.apply(&third).unwrap();
    let receipt = commit_edit(prepared);
    assert_eq!(receipt.revision(), rev(10));
    assert_eq!(receipt.added(), []);
    assert_eq!(receipt.replaced(), [n_alpha]);
    assert_eq!(receipt.removed(), [n_beta]);
    assert_eq!(
        state.node_at(a1_alpha),
        Some(n_alpha),
        "a replaced formula kept its ID"
    );
    assert_eq!(state.node(n_alpha).unwrap().prerequisites(), None);
    assert_eq!(state.node_at(a1_beta), None);
    assert_eq!(state.node(n_beta), None);
    assert_eq!(state.node_count(), 1);

    // The removed and the unbound nodes' edges are gone from the reverse indexes.
    let gone = batch(
        10,
        vec![
            set(b2_alpha, number(1.0)),
            set(key(beta, 2, 2), number(1.0)),
            set(key(alpha, 3, 3), number(1.0)),
            set(key(gamma, 2, 6), number(1.0)),
            set(key(alpha, 1_000_000, 7), number(1.0)),
            set(key(alpha, 10, 25), number(1.0)),
            set(key(alpha, 20, 8), number(1.0)),
            set(key(alpha, 256, 64), number(1.0)),
        ],
    );
    let prepared = prepare_edit(&mut state, &root10, &gone).unwrap();
    assert_eq!(seed_dependents(prepared.seeds()), vec![Vec::new(); 8]);
    drop(prepared);

    // 4. A new formula at Beta!A1 gets a new ID: the slot the removed node released, at its next
    //    generation, so the released ID stays dead.
    let fourth = batch(10, vec![set(a1_beta, formula("1"))]);
    let prepared = prepare_edit(&mut state, &root10, &fourth).unwrap();
    let &[
        NodeChange::Added {
            node: n_beta_again,
            at,
        },
    ] = prepared.node_changes()
    else {
        panic!("expected one added node, got {:?}", prepared.node_changes());
    };
    assert_eq!(at, a1_beta);
    assert_eq!((n_beta_again.slot(), n_beta_again.generation()), (1, 1));
    assert_ne!(n_beta_again, n_beta);
    assert_ne!(n_beta_again, n_alpha);
    let root11 = root10.apply(&fourth).unwrap();
    let receipt = commit_edit(prepared);
    assert_eq!(receipt.added(), [(a1_beta, n_beta_again)]);
    assert_eq!(state.node_at(a1_beta), Some(n_beta_again));
    assert_eq!(state.node(n_beta), None);
    note!("{CASE}: Beta!A1 was {n_beta:?}, removed, and is now {n_beta_again:?}");

    // 5. Later operations of a batch see earlier ones: a formula set at Beta!B1 and then cleared
    //    with Beta!A1:B2 adds nothing, and the clear removes Beta!A1's node and empties Beta!B2.
    let fifth = batch(
        11,
        vec![
            set(key(beta, 1, 2), formula("2")),
            clear(area(beta, 1, 1, 2, 2)),
        ],
    );
    let prepared = prepare_edit(&mut state, &root11, &fifth).unwrap();
    assert_eq!(
        prepared.node_changes(),
        [NodeChange::Removed {
            node: n_beta_again,
            at: a1_beta
        }]
    );
    assert_eq!(
        prepared.seeds(),
        [InputSeed::Area {
            area: area(beta, 1, 1, 2, 2),
            dependents: vec![]
        }]
    );
    let root12 = root11.apply(&fifth).unwrap();
    let receipt = commit_edit(prepared);
    assert_eq!(receipt.removed(), [n_beta_again]);
    assert_eq!(state.revision(), rev(12));
    assert_eq!(anchors(&state), [a1_alpha]);
    assert_eq!(anchors(&EngineState::new(&root12)), [a1_alpha]);

    // 6. What the graph refuses: a batch, state and snapshot at different revisions; a sheet the
    //    snapshot does not hold; structural edits; prerequisites for a dead node or an absent
    //    sheet. A refused batch reserves no ID.
    let root13 = book.root(13);
    assert_eq!(
        prepare_edit(&mut state, &root12, &batch(11, vec![])).err(),
        Some(PrepareError::StaleBase {
            batch: rev(11),
            engine: rev(12),
            inputs: rev(12)
        })
    );
    assert_eq!(
        prepare_edit(&mut state, &root13, &batch(12, vec![])).err(),
        Some(PrepareError::StaleBase {
            batch: rev(12),
            engine: rev(12),
            inputs: rev(13)
        })
    );
    assert_eq!(
        prepare_edit(
            &mut state,
            &root12,
            &batch(
                12,
                vec![
                    set(key(alpha, 1, 2), formula("5")),
                    set(key(book.absent, 1, 1), number(1.0)),
                ]
            )
        )
        .err(),
        Some(PrepareError::UnknownSheet {
            operation: 1,
            sheet: book.absent
        })
    );
    assert_eq!(
        prepare_edit(
            &mut state,
            &root12,
            &batch(12, vec![clear(area(book.absent, 1, 1, 2, 2))])
        )
        .err(),
        Some(PrepareError::UnknownSheet {
            operation: 0,
            sheet: book.absent
        })
    );
    let sheet_name = SheetName::new(ExcelText::from_interop_assignment("Delta")).unwrap();
    let structural = [
        WorkbookOp::EditAxis(AxisEdit::new(alpha, Axis::Rows, AxisChange::Insert, 1, 1).unwrap()),
        WorkbookOp::EditAxis(AxisEdit::new(beta, Axis::Columns, AxisChange::Delete, 2, 3).unwrap()),
        WorkbookOp::EditSheet(SheetEdit::Insert {
            key: SheetKey::new(9),
            name: sheet_name.clone(),
            position: NonZeroU32::new(1).unwrap(),
        }),
        WorkbookOp::EditSheet(SheetEdit::Rename {
            sheet: beta,
            name: sheet_name,
        }),
        WorkbookOp::EditSheet(SheetEdit::Move {
            sheet: beta,
            position: NonZeroU32::new(1).unwrap(),
        }),
        WorkbookOp::EditSheet(SheetEdit::Remove { sheet: gamma }),
    ];
    for op in structural {
        let refused = prepare_edit(
            &mut state,
            &root12,
            &batch(12, vec![set(key(alpha, 9, 9), formula("6")), op.clone()]),
        )
        .err();
        assert_eq!(
            refused,
            Some(PrepareError::StructuralEdit { operation: 1 }),
            "{op:?}"
        );
    }
    let visibility = batch(
        12,
        vec![WorkbookOp::EditSheet(SheetEdit::SetVisibility {
            sheet: alpha,
            visibility: SheetVisibility::Hidden,
        })],
    );
    let prepared = prepare_edit(&mut state, &root12, &visibility).unwrap();
    assert_eq!(
        (prepared.node_changes(), prepared.seeds()),
        (&[][..], &[][..])
    );
    drop(prepared);
    assert_eq!(
        prepare_edit(
            &mut state,
            &root12,
            &batch(
                12,
                vec![WorkbookOp::EditSheet(SheetEdit::SetVisibility {
                    sheet: book.absent,
                    visibility: SheetVisibility::Hidden,
                })]
            )
        )
        .err(),
        Some(PrepareError::UnknownSheet {
            operation: 0,
            sheet: book.absent
        })
    );
    let last_root = book.root(u64::MAX);
    let mut last = EngineState::new(&last_root);
    assert_eq!(
        prepare_edit(&mut last, &last_root, &batch(u64::MAX, vec![])).err(),
        Some(PrepareError::RevisionExhausted)
    );

    assert_eq!(
        state.set_prerequisites(&root12, n_beta, vec![]),
        Err(PrerequisiteError::UnknownNode(n_beta))
    );
    assert_eq!(
        state.set_prerequisites(
            &root12,
            n_alpha,
            vec![
                Prerequisite::Cell(key(alpha, 1, 2)),
                Prerequisite::Cell(key(book.absent, 1, 1))
            ]
        ),
        Err(PrerequisiteError::UnknownSheet(book.absent))
    );
    assert_eq!(
        state.set_prerequisites(
            &root12,
            n_alpha,
            vec![Prerequisite::SheetSpan(SheetSpan::new(
                area(alpha, 1, 1, 1, 1),
                book.absent
            ))]
        ),
        Err(PrerequisiteError::UnknownSheet(book.absent))
    );
    assert_eq!(
        state.set_prerequisites(&root13, n_alpha, vec![]),
        Err(PrerequisiteError::StaleInputs {
            engine: rev(12),
            inputs: rev(13)
        })
    );
    assert_eq!(
        AxisInterval::new(alpha, Axis::Rows, 0, 3),
        Err(GridError::RowOutOfRange(0))
    );
    assert_eq!(
        AxisInterval::new(alpha, Axis::Columns, 2, 16_385),
        Err(GridError::ColumnOutOfRange(16_385))
    );

    // 7. A prepared change that is dropped commits nothing and releases the ID it reserved. Slot
    //    1 was released at generation 1 in step 5 and no refused batch above reserved it, so the
    //    first reservation is slot 1 at generation 2; after the drop the same batch reserves slot 1
    //    at generation 3.
    let pending = batch(12, vec![set(key(alpha, 2, 1), formula("3"))]);
    let prepared = prepare_edit(&mut state, &root12, &pending).unwrap();
    let &[NodeChange::Added { node: reserved, .. }] = prepared.node_changes() else {
        panic!("expected one added node, got {:?}", prepared.node_changes());
    };
    assert_eq!((reserved.slot(), reserved.generation()), (1, 2));
    drop(prepared);
    assert_eq!(state.revision(), rev(12));
    assert_eq!(state.node_count(), 1);
    assert_eq!(state.node_at(key(alpha, 2, 1)), None);
    let prepared = prepare_edit(&mut state, &root12, &pending).unwrap();
    let &[NodeChange::Added { node: again, .. }] = prepared.node_changes() else {
        panic!("expected one added node, got {:?}", prepared.node_changes());
    };
    assert_eq!((again.slot(), again.generation()), (1, 3));
    drop(prepared);
    note!("{CASE}: refusals checked; the state ends at revision 12 with 1 node");
}

// ---------------------------------------------------------------------------------------------
// ceng_m1_1__input_sharing

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn ceng_m1_1__input_sharing() {
    const CASE: &str = "ceng_m1_1__input_sharing";
    let book = Workbook::new();
    let (alpha, beta, gamma) = (book.alpha, book.beta, book.gamma);

    // A document as calc-doc-model holds it: literals and formulas on three sheets, in
    // Arc-shared pages.
    let a1 = key(alpha, 1, 1);
    let b1 = key(alpha, 1, 2);
    let c1 = key(alpha, 1, 3);
    let d1 = key(alpha, 1, 4);
    let beta_a1 = key(beta, 1, 1);
    let beta_z300 = key(beta, 300, 26);
    let gamma_a1 = key(gamma, 1, 1);
    let gamma_b2 = key(gamma, 2, 2);
    let gamma_c3 = key(gamma, 3, 3);
    let load = batch(
        0,
        vec![
            set(a1, number(5.0)),
            set(b1, text("kept in the page")),
            set(c1, formula("A1*2")),
            set(beta_a1, formula("Alpha!A1+1")),
            set(beta_z300, number(7.0)),
            set(gamma_a1, CellInput::Logical(true)),
            set(gamma_c3, formula("Alpha!B1")),
        ],
    );
    let root1 = book.root(0).apply(&load).unwrap();

    // 1. The state adopts the snapshot's formula cells as nodes, issuing IDs in sheet order
    //    (Gamma, Alpha, Beta) and then row-major; literal cells get no node.
    let mut state = EngineState::new(&root1);
    assert_eq!(state.revision(), rev(1));
    assert_eq!(anchors(&state), [c1, beta_a1, gamma_c3]);
    let slots: Vec<(u32, u32)> = [gamma_c3, c1, beta_a1]
        .iter()
        .map(|at| {
            let node = state.node_at(*at).unwrap();
            (node.slot(), node.generation())
        })
        .collect();
    assert_eq!(slots, [(0, 0), (1, 0), (2, 0)]);
    for at in [a1, b1, beta_z300, gamma_a1] {
        assert_eq!(state.node_at(at), None, "the literal at {at:?} got a node");
    }
    assert!(state.nodes().all(|node| node.prerequisites().is_none()));

    // 2. An edit changes Alpha!A1's literal, sets Alpha!B1 to the text it already holds, gives
    //    Gamma!B2 a literal, clears an empty area of Gamma and adds a formula at Alpha!D1. The
    //    engine reads each cell's prior input from the base snapshot's page: Alpha!B1 and the
    //    empty area change no calculation input and seed nothing.
    let edit = batch(
        1,
        vec![
            set(a1, number(6.0)),
            set(b1, text("kept in the page")),
            set(gamma_b2, number(1.0)),
            clear(area(gamma, 10, 1, 20, 5)),
            set(d1, formula("A1+B1")),
        ],
    );
    let prepared = prepare_edit(&mut state, &root1, &edit).unwrap();
    let &[NodeChange::Added { node: n_d1, at }] = prepared.node_changes() else {
        panic!("expected one added node, got {:?}", prepared.node_changes());
    };
    assert_eq!(at, d1);
    let [
        InputSeed::Cell {
            at: seed_a1,
            before: Some(before_a1),
            ..
        },
        InputSeed::Cell {
            at: seed_d1,
            before: None,
            ..
        },
        InputSeed::Cell {
            at: seed_gamma_b2,
            before: None,
            ..
        },
    ] = prepared.seeds()
    else {
        panic!("expected three cell seeds, got {:?}", prepared.seeds());
    };
    assert_eq!((*seed_a1, *seed_d1, *seed_gamma_b2), (a1, d1, gamma_b2));
    assert_eq!(**before_a1, number(5.0));
    let page_a1 = root1.page_of(a1).unwrap();
    let in_page_a1 = page_a1.get(1, 1).unwrap();
    assert!(
        std::ptr::eq(*before_a1, in_page_a1),
        "the engine's read of Alpha!A1 is not the value in the snapshot's page"
    );
    assert!(std::ptr::eq(*before_a1, root1.cell(a1).unwrap()));
    note!(
        "{CASE}: Alpha!A1's prior input as the engine read it is at {:p}, the value in the snapshot's page at {:p}",
        *before_a1,
        in_page_a1
    );

    let root2 = root1.apply(&edit).unwrap();
    let receipt = commit_edit(prepared);
    assert_eq!(receipt.revision(), rev(2));
    assert_eq!(receipt.added(), [(d1, n_d1)]);
    assert_eq!(anchors(&EngineState::new(&root2)), anchors(&state));

    // 3. The next edit, on revision 2. Beta!Z300's page was not touched by the first edit, so the
    //    revision-2 root holds the page allocated at revision 1, and the engine's read of
    //    Beta!Z300 is the value in that allocation. Alpha!B1's page was rebuilt by the first edit;
    //    the engine reads Alpha!B1 from the revision-2 page, its text included. Alpha!C1's formula
    //    is entered again with the same text: its node is replaced, and its unchanged input seeds
    //    nothing.
    let page_z300_1 = root1.page_of(beta_z300).unwrap();
    let page_z300_2 = root2.page_of(beta_z300).unwrap();
    assert!(Arc::ptr_eq(page_z300_1, page_z300_2));
    let n_c1 = state.node_at(c1).unwrap();
    let second = batch(
        2,
        vec![
            set(beta_z300, number(8.0)),
            set(b1, text("replaced")),
            set(c1, formula("A1*2")),
        ],
    );
    let prepared = prepare_edit(&mut state, &root2, &second).unwrap();
    assert_eq!(
        prepared.node_changes(),
        [NodeChange::Replaced { node: n_c1, at: c1 }]
    );
    let [
        InputSeed::Cell {
            at: seed_b1,
            before: Some(before_b1),
            ..
        },
        InputSeed::Cell {
            at: seed_z300,
            before: Some(before_z300),
            ..
        },
    ] = prepared.seeds()
    else {
        panic!("expected two cell seeds, got {:?}", prepared.seeds());
    };
    assert_eq!((*seed_b1, *seed_z300), (b1, beta_z300));
    assert_eq!(**before_z300, number(7.0));
    let in_page_z300 = page_z300_1.get(300, 26).unwrap();
    assert!(
        std::ptr::eq(*before_z300, in_page_z300),
        "the engine's read of Beta!Z300 at revision 2 is not the value allocated at revision 1"
    );
    let in_page_b1 = root2.page_of(b1).unwrap().get(1, 2).unwrap();
    assert!(std::ptr::eq(*before_b1, in_page_b1));
    let (CellInput::Text(read_text), CellInput::Text(page_text)) = (*before_b1, in_page_b1) else {
        panic!("Alpha!B1 is not text: {before_b1:?}");
    };
    assert_eq!(read_text.text().to_string_lossy(), "kept in the page");
    assert!(std::ptr::eq(
        read_text.text().utf16_code_units().as_ptr(),
        page_text.text().utf16_code_units().as_ptr()
    ));
    note!(
        "{CASE}: Beta!Z300 read at revision 2 is at {:p}, the value in the revision-1 page at {:p}",
        *before_z300,
        in_page_z300
    );
    drop(prepared);
}
