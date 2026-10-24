//! Acceptance cases owned by the calc-engine package, each registered by its exact name in
//! `crates/calc-engine/acceptance.json`. Every case is `#[test]` natively and
//! `#[wasm_bindgen_test]` on wasm32, with no browser or Node.js configuration of its own:
//! `cargo xtask accept` forces the browser with `WASM_BINDGEN_USE_BROWSER`.
//!
//! The cases drive the engine through its public route over calc-doc-model snapshots: an edit is
//! prepared against the base root with `prepare_edit`, calc-doc-model makes the next root with
//! `ModelRoot::apply`, and the prepared change commits with `commit_edit`.
//!
//! In the ceng-m1.1 cases what a formula node reads is stated by hand with
//! `EngineState::set_prerequisites` and does not come from the formula texts. The ceng-m1.2 cases
//! recalculate through `begin_recalc`, `step` and `finish`, which bind each formula's text
//! through CalcFml and state its prerequisites from the placed references (cells, rectangles and
//! whole rows or columns); a 3-D reference is placed as one reference per sheet of its span (L3,
//! dc-m3u). The dc-qguw cases attach an instrument to `step` and judge what the engine, or a
//! deliberately faulty publication path, publishes against the evaluation record made from what
//! the evaluator reported, with calc-verdict's `judge`.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_doc_model::value::{
    CalcValue, CellKey, ExcelText, GridArea, GridError, IdAllocator, MAX_COLUMNS, MAX_ROWS, NodeId,
    NumericMode, NumericTargetId, SheetId,
};
use calc_doc_model::{
    ArrayFormula, ArrayKind, AuthoredRevision, Axis, AxisChange, AxisEdit, CachedValue, CellInput,
    CellText, DefinedName, Definitions, FileCached, FiniteNumber, FormulaText, ModelRoot,
    NameScope, OpBatch, SheetEdit, SheetEntry, SheetKey, SheetName, SheetVisibility, SkinLayout,
    SourcePart, WorkbookOp,
};
use calc_engine::{
    AxisInterval, CacheWithoutFormula, EngineState, FormulaNode, FormulaResult, InputSeed,
    NodeChange, NotModeled, ObservedEvaluation, PrepareError, Prerequisite, PrerequisiteError,
    ReadObserver, RecalcFailure, RecalcProgress, SheetSpan, StartError, Unobserved, ValueChange,
    ValueDelta, WorkBudget, begin_recalc, commit_edit, finish, prepare_edit, step,
};
use calc_fml::reference::{BindError, CellRun};
use calc_fml::{CompileError, CompiledPlan, EvalFault, ExecutedCall};
use calc_verdict::{
    CaptureRecord, CellAddress, Evaluation as VerdictEvaluation, EvaluationRecord,
    InstrumentRecord, InstrumentedEvaluation, ResultSet, Verdict, VerdictPolicy, VerdictReport,
    evaluation_record_version, judge,
};
use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::ops::ControlFlow;
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
            InputSeed::Cell { dependents, .. }
            | InputSeed::Area { dependents, .. }
            | InputSeed::Spill { dependents, .. } => dependents.clone(),
        })
        .collect()
}

/// The dependents of each cell seed by its cell; panics on an area seed.
fn cell_seed_map(seeds: &[InputSeed<'_>]) -> BTreeMap<CellKey, Vec<NodeId>> {
    seeds
        .iter()
        .map(|seed| match seed {
            InputSeed::Cell { at, dependents, .. } => (*at, dependents.clone()),
            InputSeed::Area { area, .. } | InputSeed::Spill { area, .. } => {
                panic!("unexpected area seed {area:?}")
            }
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
            InputSeed::Spill { anchor, .. } => panic!("unexpected spill seed {anchor:?}"),
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
        "ACCEPT-NOTE: {CASE}: the prerequisites of Alpha!A1 and Beta!A1 are written by hand in the case and do not come from the formula text; this case does not recalculate, so nothing binds the formulas here (begin_recalc's binding is exercised by the ceng-m1.2 cases)"
    );
    for (item, traffic) in [
        (
            "graph::Prerequisite::Cell",
            "product traffic from begin_recalc's CalcFml binding of direct references (ceng-m1.2); stated by hand here",
        ),
        (
            "graph::Prerequisite::Area",
            "product traffic from begin_recalc's CalcFml binding of ranges (ceng-m1.2); stated by hand here",
        ),
        (
            "graph::Prerequisite::AxisValue",
            "product traffic from begin_recalc's CalcFml binding of whole-row and whole-column references (ceng-m1.2); stated by hand here",
        ),
        (
            "graph::Prerequisite::SheetSpan",
            "test-only traffic: CalcFml's binder refuses sheet spans; its product traffic is to come from dc-m3u's 3D span binding and span calculation",
        ),
    ] {
        note!("ACCEPT-NOTE: {CASE}: seams entry {item}: {traffic}");
    }
    for item in [
        "state::PrerequisiteError::StaleInputs",
        "state::PrerequisiteError::UnknownNode",
        "state::PrerequisiteError::UnknownSheet",
    ] {
        note!(
            "ACCEPT-NOTE: {CASE}: seams entry {item} is produced only by this case: the product caller of EngineState::set_prerequisites, begin_recalc, passes the snapshot at the state's revision, a live node and sheets resolved from that snapshot"
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

// ---------------------------------------------------------------------------------------------
// ceng-m1.2: the workbook recalculates through CalcFml. The helpers below drive the product route
// (prepare_edit and commit_edit, then begin_recalc, step and finish) and observe the runs the
// engine's reader reports to the evaluations, through the ReadObserver that step takes.

/// The run of a read, as an observer saw it.
#[derive(Clone, Debug, PartialEq)]
enum Run {
    Values(Vec<CalcValue>),
    Blank(u64),
    Missing(u64),
}

impl Run {
    fn of(run: CellRun<'_>) -> Run {
        match run {
            CellRun::Values(values) => Run::Values(values.to_vec()),
            CellRun::Blank(count) => Run::Blank(count),
            CellRun::Missing(count) => Run::Missing(count),
        }
    }

    fn len(&self) -> u64 {
        match self {
            Run::Values(values) => values.len() as u64,
            Run::Blank(count) | Run::Missing(count) => *count,
        }
    }
}

/// One area an evaluation read: the formula the engine said was reading, the area, and the runs
/// the engine's reader reported for it.
#[derive(Clone, Debug, PartialEq)]
struct Read {
    formula: CellKey,
    area: GridArea,
    runs: Vec<Run>,
}

/// An observer that records every run and never stops a recalculation.
#[derive(Debug, Default)]
struct Recorder {
    reads: Vec<Read>,
}

impl ReadObserver for Recorder {
    fn observe(
        &mut self,
        formula: CellKey,
        area: GridArea,
        offset: u64,
        run: CellRun<'_>,
    ) -> ControlFlow<()> {
        if offset == 0 {
            self.reads.push(Read {
                formula,
                area,
                runs: Vec::new(),
            });
        }
        let read = self.reads.last_mut().expect("a read begins at offset 0");
        assert_eq!(
            (read.formula, read.area),
            (formula, area),
            "a run continued another read"
        );
        assert_eq!(
            offset,
            read.runs.iter().map(Run::len).sum::<u64>(),
            "a run did not start where the previous one ended"
        );
        read.runs.push(Run::of(run));
        ControlFlow::Continue(())
    }
}

impl Recorder {
    /// The formulas in the order their evaluations read, each once: one evaluation runs at a
    /// time, so a formula's reads are consecutive.
    fn evaluation_order(&self) -> Vec<CellKey> {
        let mut order: Vec<CellKey> = Vec::new();
        for read in &self.reads {
            if order.last() != Some(&read.formula) {
                assert!(
                    !order.contains(&read.formula),
                    "{:?} read again after another formula read",
                    read.formula
                );
                order.push(read.formula);
            }
        }
        order
    }

    fn take(&mut self) -> Vec<Read> {
        std::mem::take(&mut self.reads)
    }
}

/// An observer that stops the recalculation at its `stop_at`-th observation (counting from 1) and
/// records every observation before it.
struct StopAt {
    observations: usize,
    stop_at: usize,
    recorder: Recorder,
}

impl ReadObserver for StopAt {
    fn observe(
        &mut self,
        formula: CellKey,
        area: GridArea,
        offset: u64,
        run: CellRun<'_>,
    ) -> ControlFlow<()> {
        self.observations += 1;
        if self.observations == self.stop_at {
            return ControlFlow::Break(());
        }
        self.recorder.observe(formula, area, offset, run)
    }
}

fn sheet_entry(id: SheetId, key: u32, name: &str) -> SheetEntry {
    SheetEntry::new(
        id,
        SheetKey::new(key),
        SheetName::new(ExcelText::from_interop_assignment(name)).unwrap(),
        SheetVisibility::Visible,
    )
}

fn cell_area(at: CellKey) -> GridArea {
    GridArea::cell(at)
}

fn values(numbers: &[f64]) -> Run {
    Run::Values(numbers.iter().map(|n| CalcValue::number(*n)).collect())
}

fn read(formula: CellKey, area: GridArea, runs: Vec<Run>) -> Read {
    Read {
        formula,
        area,
        runs,
    }
}

fn excel_text(value: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_interop_assignment(value))
}

/// Commits the batch `operations` to the state and gives the next root.
fn edit(
    state: &mut EngineState,
    root: &Arc<ModelRoot>,
    operations: Vec<WorkbookOp>,
) -> Arc<ModelRoot> {
    let change = batch(root.revision().get(), operations);
    let prepared = prepare_edit(state, root, &change).unwrap();
    let next = root.apply(&change).unwrap();
    commit_edit(prepared);
    next
}

/// Recalculates under `mode` in one step, shown to `observer`, and publishes.
fn recalc_under(
    state: &mut EngineState,
    inputs: &ModelRoot,
    mode: NumericMode,
    observer: &mut dyn ReadObserver,
) -> ValueDelta {
    let mut work = begin_recalc(state, inputs, mode).unwrap();
    assert_eq!(
        step(state, inputs, &mut work, WorkBudget::UNLIMITED, observer),
        RecalcProgress::Complete
    );
    finish(state, work).unwrap()
}

/// Recalculates under Fast in one step, shown to `observer`, and publishes.
fn recalc(
    state: &mut EngineState,
    inputs: &ModelRoot,
    observer: &mut dyn ReadObserver,
) -> ValueDelta {
    recalc_under(state, inputs, NumericMode::Fast, observer)
}

/// The plan a bound formula cell's node holds.
fn plan_at(state: &EngineState, at: CellKey) -> Arc<CompiledPlan> {
    let node = state.node_at(at).unwrap();
    Arc::clone(state.instance(node).unwrap().plan())
}

/// The address of the placed areas of a bound formula cell's instance. Binding a bound node again
/// would place a new instance, allocated while the node still holds the old one, so at another
/// address: an unchanged address means the recalculations between were not compiling and placing
/// its formula again.
fn instance_address(state: &EngineState, at: CellKey) -> usize {
    let node = state.node_at(at).unwrap();
    state.instance(node).unwrap().areas().as_ptr() as usize
}

/// The calculated value of the formula cell `at`, with the revision and tick of its calculation;
/// panics when its result is not calculated.
fn calculated(state: &EngineState, at: CellKey) -> (CalcValue, u64, u64) {
    match state.result(at) {
        Some(FormulaResult::Calculated { value, calculation }) => (
            value.clone(),
            calculation.revision().get(),
            calculation.tick().get(),
        ),
        other => panic!("{at:?} is not calculated: {other:?}"),
    }
}

/// The cells whose published result is calculated.
fn calculated_cells(state: &EngineState) -> Vec<CellKey> {
    state
        .results()
        .filter(|(_, result)| result.calculated().is_some())
        .map(|(at, _)| at)
        .collect()
}

/// The cells each change of a delta names, in order, with `true` for a published result.
fn delta_cells(delta: &ValueDelta) -> Vec<(CellKey, bool)> {
    delta
        .changes()
        .iter()
        .map(|change| match change {
            ValueChange::Published { at, .. } => (*at, true),
            ValueChange::Removed { at, .. } => (*at, false),
        })
        .collect()
}

/// The published generation, cloned: each cell's result, value and provenance.
fn generation(state: &EngineState) -> Vec<(CellKey, FormulaResult)> {
    state
        .results()
        .map(|(at, result)| (at, result.clone()))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// ceng_m1_2__chain_recalculates

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn ceng_m1_2__chain_recalculates() {
    const CASE: &str = "ceng_m1_2__chain_recalculates";
    note!(
        "ACCEPT-NOTE: {CASE}: every expected value, prerequisite, read and evaluation order below is derived by hand in the case from the literals and the formula texts (labelled 'hand-derived'); none is read back from the engine, a capture or Excel, and agreement with Excel is dc-35p's round trip"
    );

    // Three sheets in the order Inputs, Work, Report, whose IDs were issued in the order Report,
    // Work, Inputs: the ready queue breaks ties by anchor, sheet ID first, so a ready Report
    // formula comes before a ready Work formula although Report is the last sheet.
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let report = ids.issue().unwrap();
    let work_sheet = ids.issue().unwrap();
    let inputs = ids.issue().unwrap();
    let sheets = vec![
        sheet_entry(inputs, 1, "Inputs"),
        sheet_entry(work_sheet, 2, "Work"),
        sheet_entry(report, 3, "Report"),
    ];
    let i = |row, column| key(inputs, row, column);
    let w = |row, column| key(work_sheet, row, column);
    let r = |row, column| key(report, row, column);

    // Literals on Inputs; formulas on Work and Report that read them and each other through
    // direct references, a rectangle, a whole column, a cross-sheet reference and an IF. The
    // texts are as an xlsx <f> element holds them, without the leading '='.
    let load = batch(
        0,
        vec![
            set(i(1, 1), number(10.0)),
            set(i(2, 1), number(20.0)),
            set(i(3, 1), number(30.0)),
            set(i(1, 2), number(4.0)),
            set(i(1, 3), number(2.5)),
            set(i(5, 3), number(7.5)),
            set(w(1, 1), formula("SUM(Inputs!A1:A3)")),
            set(w(2, 1), formula("A1*Inputs!B1")),
            set(w(1, 2), formula("SUM(Inputs!C:C)")),
            set(w(1, 3), formula("Report!A1+1")),
            set(r(1, 1), formula("IF(Work!A2>100,Work!A2-100,Work!B1)")),
            set(r(1, 2), formula("SUM(Work!A1:A2)+Work!C1")),
            set(r(1, 3), formula("inputs!B1*2")),
        ],
    );
    let root1 = ModelRoot::new(rev(0), sheets)
        .unwrap()
        .apply(&load)
        .unwrap();
    let mut state = EngineState::new(&root1);
    assert_eq!(state.node_count(), 7);
    assert_eq!(
        state.pending().count(),
        7,
        "every node of a new state is pending"
    );
    assert_eq!(state.results().count(), 0);
    let node = |state: &EngineState, at: CellKey| state.node_at(at).unwrap();

    // 1. The first recalculation binds each formula through CalcFml and calculates every node.
    let mut recorder = Recorder::default();
    let mut work = begin_recalc(&mut state, &root1, NumericMode::Fast).unwrap();
    assert_eq!(
        (work.revision(), work.tick().get(), work.scheduled()),
        (rev(1), 1, 7)
    );

    // What each formula reads, hand-derived from its text: the placed references, one
    // prerequisite each (Work!A2 appears twice in Report!A1's IF and is one edge). "inputs!" names
    // the sheet Inputs: sheet names match without case.
    let column_c = AxisInterval::new(inputs, Axis::Columns, 3, 3).unwrap();
    for (at, expected) in [
        (w(1, 1), vec![Prerequisite::Area(area(inputs, 1, 1, 3, 1))]),
        (
            w(2, 1),
            vec![Prerequisite::Cell(w(1, 1)), Prerequisite::Cell(i(1, 2))],
        ),
        (w(1, 2), vec![Prerequisite::AxisValue(column_c)]),
        (w(1, 3), vec![Prerequisite::Cell(r(1, 1))]),
        (
            r(1, 1),
            vec![Prerequisite::Cell(w(2, 1)), Prerequisite::Cell(w(1, 2))],
        ),
        (
            r(1, 2),
            vec![
                Prerequisite::Area(area(work_sheet, 1, 1, 2, 1)),
                Prerequisite::Cell(w(1, 3)),
            ],
        ),
        (r(1, 3), vec![Prerequisite::Cell(i(1, 2))]),
    ] {
        assert_eq!(
            state.node(node(&state, at)).unwrap().prerequisites(),
            Some(&expected[..]),
            "prerequisites of {at:?}"
        );
    }

    // Steps of three evaluations, then the rest; nothing is published before finish.
    assert_eq!(
        step(
            &state,
            &root1,
            &mut work,
            WorkBudget::evaluations(3),
            &mut recorder
        ),
        RecalcProgress::Pending { remaining: 4 }
    );
    assert_eq!(
        state.results().count(),
        0,
        "a result was published before finish"
    );
    assert_eq!(
        step(
            &state,
            &root1,
            &mut work,
            WorkBudget::UNLIMITED,
            &mut recorder
        ),
        RecalcProgress::Complete
    );
    assert_eq!(
        state.results().count(),
        0,
        "a result was published before finish"
    );
    let delta = finish(&mut state, work).unwrap();
    assert_eq!((delta.revision(), delta.tick().get()), (rev(1), 1));

    // The reads, hand-derived: Report!C1 reads only a literal and is ready first (Report's sheet
    // ID is the smallest); then Work!A1 and Work!B1 (row 1 before row 2); Work!A2 once Work!A1
    // is evaluated; Report!A1 once Work!A2 and Work!B1 are; Work!C1 after Report!A1; Report!B1
    // last. Each formula cell read hands over the value just calculated. Report!A1's IF reads
    // Work!A2 for its condition and again for the branch it takes (240 > 100); it never reads
    // Work!B1. The whole column C of Inputs is read as its two values and blank runs.
    let first_reads = vec![
        read(r(1, 3), cell_area(i(1, 2)), vec![values(&[4.0])]),
        read(
            w(1, 1),
            area(inputs, 1, 1, 3, 1),
            vec![values(&[10.0]), values(&[20.0]), values(&[30.0])],
        ),
        read(
            w(1, 2),
            area(inputs, 1, 3, MAX_ROWS, 3),
            vec![
                values(&[2.5]),
                Run::Blank(3),
                values(&[7.5]),
                Run::Blank(u64::from(MAX_ROWS) - 5),
            ],
        ),
        read(w(2, 1), cell_area(w(1, 1)), vec![values(&[60.0])]),
        read(w(2, 1), cell_area(i(1, 2)), vec![values(&[4.0])]),
        read(r(1, 1), cell_area(w(2, 1)), vec![values(&[240.0])]),
        read(r(1, 1), cell_area(w(2, 1)), vec![values(&[240.0])]),
        read(w(1, 3), cell_area(r(1, 1)), vec![values(&[140.0])]),
        read(
            r(1, 2),
            area(work_sheet, 1, 1, 2, 1),
            vec![values(&[60.0]), values(&[240.0])],
        ),
        read(r(1, 2), cell_area(w(1, 3)), vec![values(&[141.0])]),
    ];
    assert_eq!(
        recorder.evaluation_order(),
        [
            r(1, 3),
            w(1, 1),
            w(1, 2),
            w(2, 1),
            r(1, 1),
            w(1, 3),
            r(1, 2)
        ]
    );
    assert_eq!(recorder.take(), first_reads);

    // The published values, hand-derived: Work!A1 = 10+20+30, Work!A2 = 60*4, Work!B1 = 2.5+7.5,
    // Report!A1 = 240-100, Work!C1 = 140+1, Report!B1 = (60+240)+141, Report!C1 = 4*2. Each is
    // calculated at revision 1 by tick 1 under Fast, which has no Excel target.
    let first_values = [
        (w(1, 1), 60.0),
        (w(2, 1), 240.0),
        (w(1, 2), 10.0),
        (r(1, 1), 140.0),
        (w(1, 3), 141.0),
        (r(1, 2), 441.0),
        (r(1, 3), 8.0),
    ];
    for (at, expected) in first_values {
        assert_eq!(
            calculated(&state, at),
            (CalcValue::number(expected), 1, 1),
            "{at:?}"
        );
        let Some((_, calculation)) = state.result(at).and_then(FormulaResult::calculated) else {
            unreachable!()
        };
        assert_eq!(
            (calculation.mode(), calculation.target()),
            (&NumericMode::Fast, None)
        );
    }
    // The delta lists the cells by sheet ID, then row, then column.
    assert_eq!(
        delta_cells(&delta),
        [
            (r(1, 1), true),
            (r(1, 2), true),
            (r(1, 3), true),
            (w(1, 1), true),
            (w(1, 2), true),
            (w(1, 3), true),
            (w(2, 1), true),
        ]
    );
    for change in delta.changes() {
        let ValueChange::Published {
            at,
            node: n,
            result,
        } = change
        else {
            panic!("unexpected {change:?}");
        };
        assert_eq!(Some(*n), state.node_at(*at));
        assert_eq!(Some(result), state.result(*at));
    }
    assert_eq!(state.pending().count(), 0);
    // Work!A2's bound plan, placed at its anchor. Steps 2, 3 and 7 evaluate it again; none binds
    // it again (see the end of the case).
    let w_a2_plan = plan_at(&state, w(2, 1));
    let w_a2_address = instance_address(&state, w(2, 1));
    note!(
        "{CASE}: the first recalculation bound 7 formulas through CalcFml and published 7 calculated values equal to the hand-derived ones, read in the hand-derived order"
    );

    // 2. A literal edit: Inputs!A2 from 20 to 70. The seed lists Work!A1 (it reads the rectangle
    //    Inputs!A1:A3 through the edge the binding filed); the dirty closure adds what reads
    //    Work!A1 and onward: Work!A2, Report!A1, Work!C1 and Report!B1. Work!B1 and Report!C1 are
    //    not dirty and keep their tick-1 results.
    let change = batch(1, vec![set(i(2, 1), number(70.0))]);
    let n_w_a1 = node(&state, w(1, 1));
    let prepared = prepare_edit(&mut state, &root1, &change).unwrap();
    assert_eq!(
        prepared.seeds(),
        [InputSeed::Cell {
            at: i(2, 1),
            before: Some(&number(20.0)),
            dependents: vec![n_w_a1]
        }]
    );
    let root2 = root1.apply(&change).unwrap();
    commit_edit(prepared);
    assert_eq!(state.pending().collect::<Vec<_>>(), [node(&state, w(1, 1))]);

    let mut work = begin_recalc(&mut state, &root2, NumericMode::Fast).unwrap();
    assert_eq!((work.tick().get(), work.scheduled()), (2, 5));
    let mut remaining = 5;
    loop {
        match step(
            &state,
            &root2,
            &mut work,
            WorkBudget::evaluations(1),
            &mut recorder,
        ) {
            RecalcProgress::Pending { remaining: left } => {
                remaining -= 1;
                assert_eq!(left, remaining);
                // Nothing of this recalculation is published while it runs.
                assert_eq!(calculated(&state, w(1, 1)), (CalcValue::number(60.0), 1, 1));
            }
            RecalcProgress::Complete => break,
            RecalcProgress::Failed => panic!("the recalculation failed"),
        }
    }
    let delta = finish(&mut state, work).unwrap();
    assert_eq!(
        recorder.evaluation_order(),
        [w(1, 1), w(2, 1), r(1, 1), w(1, 3), r(1, 2)]
    );
    assert_eq!(
        recorder.take(),
        [
            read(
                w(1, 1),
                area(inputs, 1, 1, 3, 1),
                vec![values(&[10.0]), values(&[70.0]), values(&[30.0])],
            ),
            read(w(2, 1), cell_area(w(1, 1)), vec![values(&[110.0])]),
            read(w(2, 1), cell_area(i(1, 2)), vec![values(&[4.0])]),
            read(r(1, 1), cell_area(w(2, 1)), vec![values(&[440.0])]),
            read(r(1, 1), cell_area(w(2, 1)), vec![values(&[440.0])]),
            read(w(1, 3), cell_area(r(1, 1)), vec![values(&[340.0])]),
            read(
                r(1, 2),
                area(work_sheet, 1, 1, 2, 1),
                vec![values(&[110.0]), values(&[440.0])],
            ),
            read(r(1, 2), cell_area(w(1, 3)), vec![values(&[341.0])]),
        ]
    );
    // Hand-derived: Work!A1 = 10+70+30, Work!A2 = 110*4, Report!A1 = 440-100, Work!C1 = 341,
    // Report!B1 = (110+440)+341.
    for (at, expected) in [
        (w(1, 1), 110.0),
        (w(2, 1), 440.0),
        (r(1, 1), 340.0),
        (w(1, 3), 341.0),
        (r(1, 2), 891.0),
    ] {
        assert_eq!(
            calculated(&state, at),
            (CalcValue::number(expected), 2, 2),
            "{at:?}"
        );
    }
    assert_eq!(calculated(&state, w(1, 2)), (CalcValue::number(10.0), 1, 1));
    assert_eq!(calculated(&state, r(1, 3)), (CalcValue::number(8.0), 1, 1));
    assert_eq!(delta.changes().len(), 5);
    note!(
        "{CASE}: Inputs!A2 20 -> 70 recalculated the 5 formulas of its closure one step at a time, published nothing before finish, and left Work!B1 and Report!C1 at tick 1"
    );

    // 3. Inputs!B1 from 4 to 0.5. Work!A2 and Report!C1 read it; Report!C1 is ordered first (the
    //    smaller sheet ID). Work!A2 becomes 55, so Report!A1's IF takes its third argument: it
    //    reads Work!A2 for the condition and Work!B1, never Work!A2 again.
    let root3 = edit(&mut state, &root2, vec![set(i(1, 2), number(0.5))]);
    recalc(&mut state, &root3, &mut recorder);
    assert_eq!(
        recorder.evaluation_order(),
        [r(1, 3), w(2, 1), r(1, 1), w(1, 3), r(1, 2)]
    );
    let reads = recorder.take();
    assert_eq!(
        reads
            .iter()
            .filter(|read| read.formula == r(1, 1))
            .cloned()
            .collect::<Vec<_>>(),
        [
            read(r(1, 1), cell_area(w(2, 1)), vec![values(&[55.0])]),
            read(r(1, 1), cell_area(w(1, 2)), vec![values(&[10.0])]),
        ]
    );
    // Hand-derived: Report!C1 = 0.5*2, Work!A2 = 110*0.5, Report!A1 = Work!B1 = 10, Work!C1 = 11,
    // Report!B1 = (110+55)+11.
    for (at, expected) in [
        (r(1, 3), 1.0),
        (w(2, 1), 55.0),
        (r(1, 1), 10.0),
        (w(1, 3), 11.0),
        (r(1, 2), 176.0),
    ] {
        assert_eq!(
            calculated(&state, at),
            (CalcValue::number(expected), 3, 3),
            "{at:?}"
        );
    }

    // 4. A clear of Inputs!C1:C5 empties both values of column C. Its seed is the area seed of
    //    prepare_edit's clear path, listing Work!B1 through its whole-column edge; Work!B1 becomes
    //    0, and Report!A1 (whose IF now returns Work!B1), Work!C1 and Report!B1 follow.
    let clear_c = batch(3, vec![clear(area(inputs, 1, 3, 5, 3))]);
    let n_w_b1 = node(&state, w(1, 2));
    let prepared = prepare_edit(&mut state, &root3, &clear_c).unwrap();
    assert_eq!(
        prepared.seeds(),
        [InputSeed::Area {
            area: area(inputs, 1, 3, 5, 3),
            dependents: vec![n_w_b1]
        }]
    );
    let root4 = root3.apply(&clear_c).unwrap();
    commit_edit(prepared);
    recalc(&mut state, &root4, &mut recorder);
    assert_eq!(
        recorder.evaluation_order(),
        [w(1, 2), r(1, 1), w(1, 3), r(1, 2)]
    );
    assert_eq!(
        recorder.take()[0],
        read(
            w(1, 2),
            area(inputs, 1, 3, MAX_ROWS, 3),
            vec![Run::Blank(u64::from(MAX_ROWS))],
        )
    );
    // Hand-derived: Work!B1 = SUM of an empty column = 0, Report!A1 = 0, Work!C1 = 0+1,
    // Report!B1 = (110+55)+1.
    for (at, expected) in [
        (w(1, 2), 0.0),
        (r(1, 1), 0.0),
        (w(1, 3), 1.0),
        (r(1, 2), 166.0),
    ] {
        assert_eq!(
            calculated(&state, at),
            (CalcValue::number(expected), 4, 4),
            "{at:?}"
        );
    }
    note!(
        "ACCEPT-NOTE: {CASE}: the clear of Inputs!C1:C5 drove the dirty closure through prepare_edit's area path (ReverseIndex::dependents_of_cells_in_area) by a whole-column edge; the clear the ceng-m1.1 reviewer asked for, on the middle sheet of a sheet span (Alpha!J5 of Gamma:Beta!J5), is not stated by hand here; L3 (dc-m3u) binds 3-D references as one placed reference per sheet, and dc_m3u__tables_and_3d_through_the_engine edits the middle sheet of a span"
    );

    // 5. Work!C1's formula is replaced by the literal 5, and formulas enter Report!D1 and, one row
    //    down, Report!D2, a copy of it. The removed formula's result stays published until the
    //    next publication removes it; the new nodes are bound and calculated, and Report!B1, which
    //    read Work!C1, recalculates.
    let n_c1 = node(&state, w(1, 3));
    let root5 = edit(
        &mut state,
        &root4,
        vec![
            set(w(1, 3), number(5.0)),
            set(r(1, 4), formula("Work!C1*2")),
            set(r(2, 4), formula("Work!C2*2")),
        ],
    );
    assert_eq!(state.node_at(w(1, 3)), None);
    assert_eq!(calculated(&state, w(1, 3)), (CalcValue::number(1.0), 4, 4));
    let delta = recalc(&mut state, &root5, &mut recorder);
    assert_eq!(
        state.node(node(&state, r(1, 4))).unwrap().prerequisites(),
        Some(&[Prerequisite::Cell(w(1, 3))][..])
    );
    assert_eq!(
        state.node(node(&state, r(2, 4))).unwrap().prerequisites(),
        Some(&[Prerequisite::Cell(w(2, 3))][..])
    );
    assert_eq!(recorder.evaluation_order(), [r(1, 2), r(1, 4), r(2, 4)]);
    assert_eq!(
        recorder.take(),
        [
            read(
                r(1, 2),
                area(work_sheet, 1, 1, 2, 1),
                vec![values(&[110.0]), values(&[55.0])],
            ),
            read(r(1, 2), cell_area(w(1, 3)), vec![values(&[5.0])]),
            read(r(1, 4), cell_area(w(1, 3)), vec![values(&[5.0])]),
            read(r(2, 4), cell_area(w(2, 3)), vec![Run::Blank(1)]),
        ]
    );
    // Hand-derived: Report!B1 = (110+55)+5, Report!D1 = 5*2, Report!D2 = (blank Work!C2)*2 = 0.
    assert_eq!(
        calculated(&state, r(1, 2)),
        (CalcValue::number(170.0), 5, 5)
    );
    assert_eq!(calculated(&state, r(1, 4)), (CalcValue::number(10.0), 5, 5));
    assert_eq!(calculated(&state, r(2, 4)), (CalcValue::number(0.0), 5, 5));
    assert_eq!(state.result(w(1, 3)), None);
    assert_eq!(
        delta_cells(&delta),
        [
            (r(1, 2), true),
            (r(1, 4), true),
            (r(2, 4), true),
            (w(1, 3), false)
        ]
    );
    assert_eq!(
        delta.changes()[3],
        ValueChange::Removed {
            at: w(1, 3),
            node: n_c1
        }
    );
    // The engine's plan cache holds one plan per normal form: Report!D1 and its copy Report!D2
    // were compiled to one shared plan, placed at two anchors; Report!B1's formula has another.
    let d1_plan = plan_at(&state, r(1, 4));
    assert!(
        Arc::ptr_eq(&d1_plan, &plan_at(&state, r(2, 4))),
        "a formula and its copy one row down did not share a plan"
    );
    assert!(!Arc::ptr_eq(&d1_plan, &plan_at(&state, r(1, 2))));
    let placed = |at: CellKey| {
        state
            .instance(node(&state, at))
            .unwrap()
            .areas()
            .iter()
            .map(|placed| placed.unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(placed(r(1, 4)), [cell_area(w(1, 3))]);
    assert_eq!(placed(r(2, 4)), [cell_area(w(2, 3))]);
    let d1_address = instance_address(&state, r(1, 4));
    note!(
        "{CASE}: Report!D1 and its copy Report!D2 share one compiled plan (Arc {:p}) placed at two anchors",
        Arc::as_ptr(&d1_plan)
    );

    // 6. Report!C1's formula is replaced by a different one, SUM(Inputs!2:2)-Work!C1, and Work!C1
    //    changes from 5 to 6. The replaced node keeps its ID and is unbound until the
    //    recalculation binds the new formula: its prerequisites become the whole row 2 of Inputs
    //    and Work!C1, and no longer Inputs!B1. Report!D1 is evaluated again with the plan bound
    //    in step 5.
    let n_r_c1 = node(&state, r(1, 3));
    let (n_r_b1, n_r_d1) = (node(&state, r(1, 2)), node(&state, r(1, 4)));
    let change = batch(
        5,
        vec![
            set(r(1, 3), formula("SUM(Inputs!2:2)-Work!C1")),
            set(w(1, 3), number(6.0)),
        ],
    );
    let prepared = prepare_edit(&mut state, &root5, &change).unwrap();
    assert_eq!(
        prepared.node_changes(),
        [NodeChange::Replaced {
            node: n_r_c1,
            at: r(1, 3)
        }]
    );
    assert_eq!(
        cell_seed_map(prepared.seeds()),
        BTreeMap::from([(r(1, 3), vec![]), (w(1, 3), vec![n_r_b1, n_r_d1])])
    );
    let root6 = root5.apply(&change).unwrap();
    commit_edit(prepared);
    assert_eq!(state.node_at(r(1, 3)), Some(n_r_c1));
    assert_eq!(state.node(n_r_c1).unwrap().prerequisites(), None);
    assert!(state.instance(n_r_c1).is_none());
    let delta = recalc(&mut state, &root6, &mut recorder);
    let row_2 = AxisInterval::new(inputs, Axis::Rows, 2, 2).unwrap();
    assert_eq!(
        state.node(n_r_c1).unwrap().prerequisites(),
        Some(&[Prerequisite::AxisValue(row_2), Prerequisite::Cell(w(1, 3))][..])
    );
    assert_eq!(recorder.evaluation_order(), [r(1, 2), r(1, 3), r(1, 4)]);
    assert_eq!(
        recorder.take(),
        [
            read(
                r(1, 2),
                area(work_sheet, 1, 1, 2, 1),
                vec![values(&[110.0]), values(&[55.0])],
            ),
            read(r(1, 2), cell_area(w(1, 3)), vec![values(&[6.0])]),
            read(
                r(1, 3),
                area(inputs, 2, 1, 2, MAX_COLUMNS),
                vec![values(&[70.0]), Run::Blank(u64::from(MAX_COLUMNS) - 1)],
            ),
            read(r(1, 3), cell_area(w(1, 3)), vec![values(&[6.0])]),
            read(r(1, 4), cell_area(w(1, 3)), vec![values(&[6.0])]),
        ]
    );
    // Hand-derived: Report!B1 = (110+55)+6, Report!C1 = SUM of row 2 of Inputs (Inputs!A2 = 70)
    // - 6, Report!D1 = 6*2.
    for (at, expected) in [(r(1, 2), 171.0), (r(1, 3), 64.0), (r(1, 4), 12.0)] {
        assert_eq!(
            calculated(&state, at),
            (CalcValue::number(expected), 6, 6),
            "{at:?}"
        );
    }
    assert_eq!(delta.changes().len(), 3);
    assert_eq!(
        instance_address(&state, r(1, 4)),
        d1_address,
        "Report!D1 was bound again"
    );
    assert!(Arc::ptr_eq(&plan_at(&state, r(1, 4)), &d1_plan));

    // 7. Inputs!B1 from 0.5 to 2 and Inputs!D2 from empty to 30. Inputs!B1's seed lists Work!A2
    //    and no longer Report!C1, whose old formula read it; Inputs!D2's lists Report!C1 through
    //    the whole-row edge of its new formula.
    let n_w_a2 = node(&state, w(2, 1));
    let change = batch(
        6,
        vec![set(i(1, 2), number(2.0)), set(i(2, 4), number(30.0))],
    );
    let prepared = prepare_edit(&mut state, &root6, &change).unwrap();
    assert_eq!(
        cell_seed_map(prepared.seeds()),
        BTreeMap::from([(i(1, 2), vec![n_w_a2]), (i(2, 4), vec![n_r_c1])])
    );
    let root7 = root6.apply(&change).unwrap();
    commit_edit(prepared);
    recalc(&mut state, &root7, &mut recorder);
    // Hand-derived order: Report!C1 and Work!A2 are ready first and Report's sheet ID is the
    // smaller; Report!A1 and Report!B1 follow Work!A2. Work!C1 is a literal now, so nothing reads
    // Report!A1.
    assert_eq!(
        recorder.evaluation_order(),
        [r(1, 3), w(2, 1), r(1, 1), r(1, 2)]
    );
    let reads = recorder.take();
    assert_eq!(
        reads[0],
        read(
            r(1, 3),
            area(inputs, 2, 1, 2, MAX_COLUMNS),
            vec![
                values(&[70.0]),
                Run::Blank(2),
                values(&[30.0]),
                Run::Blank(u64::from(MAX_COLUMNS) - 4),
            ],
        )
    );
    // Hand-derived: Report!C1 = (70+30)-6, Work!A2 = 110*2, Report!A1 = 220-100 (220 > 100),
    // Report!B1 = (110+220)+6.
    for (at, expected) in [
        (r(1, 3), 94.0),
        (w(2, 1), 220.0),
        (r(1, 1), 120.0),
        (r(1, 2), 336.0),
    ] {
        assert_eq!(
            calculated(&state, at),
            (CalcValue::number(expected), 7, 7),
            "{at:?}"
        );
    }
    // Work!A2 was evaluated by the recalculations of ticks 1, 2, 3 and 7 with the plan bound at
    // tick 1: no recalculation compiled or placed it again.
    assert_eq!(
        instance_address(&state, w(2, 1)),
        w_a2_address,
        "Work!A2 was bound again"
    );
    assert!(Arc::ptr_eq(&plan_at(&state, w(2, 1)), &w_a2_plan));
    note!(
        "{CASE}: seven recalculations over three sheets published the hand-derived values at ticks 1 to 7; a formula replaced by a literal had its result removed, a formula replaced by another was bound again with its new prerequisites (a whole row among them), and Work!A2 kept the instance bound at tick 1 through 4 evaluations"
    );
}

// ---------------------------------------------------------------------------------------------
// ceng_m1_2__wrong_cache_not_pass

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn ceng_m1_2__wrong_cache_not_pass() {
    const CASE: &str = "ceng_m1_2__wrong_cache_not_pass";
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let data = ids.issue().unwrap();
    let calc = ids.issue().unwrap();
    let sheets = vec![sheet_entry(data, 1, "Data"), sheet_entry(calc, 2, "Calc")];
    let d = |row, column| key(data, row, column);
    let c = |row, column| key(calc, row, column);
    let unpaired = CellInput::Formula(
        FormulaText::new(ExcelText::from_utf16_code_units(vec![0x22, 0xD800, 0x22])).unwrap(),
    );
    let load = batch(
        0,
        vec![
            set(d(1, 1), number(3.0)),
            set(d(2, 1), number(4.0)),
            set(c(1, 1), formula("Data!A1*Data!A2")),
            set(c(2, 1), formula("A1+Data!A1")),
            set(c(3, 1), formula("IF(A2>10,\"big\",\"small\")")),
            set(c(4, 1), formula("1E-300*1E-10")),
            set(c(5, 1), formula("A4+1")),
            // A reference to another workbook (L3 binds 3-D references, which this row held
            // before; external references stay refused).
            set(c(6, 1), formula("SUM([1]Data!A1)")),
            set(c(7, 1), unpaired),
            set(c(8, 1), formula("0.1+0.2-0.3")),
            // A range where a legacy formula takes one value at a place whose evaluation is not
            // measured (CHISQ.TEST's actual range; XLOOKUP's lookup array, which this row used
            // before, was measured by L3 and lifts): a typed refusal. (A range at the formula's
            // root is intersected with its row, as Excel does: `=Data!A1:A2` in row 9 is
            // #VALUE!.)
            set(c(9, 1), formula("_xlfn.CHISQ.TEST(Data!A1:A2*1,Data!A1:A2)")),
            set(c(1, 3), formula("A2*2")),
        ],
    );
    let root1 = ModelRoot::new(rev(0), sheets.clone())
        .unwrap()
        .apply(&load)
        .unwrap();
    let all_formulas = vec![
        c(1, 1),
        c(1, 3),
        c(2, 1),
        c(3, 1),
        c(4, 1),
        c(5, 1),
        c(6, 1),
        c(7, 1),
        c(8, 1),
        c(9, 1),
    ];

    // The file's caches are deliberately wrong, except Calc!A4's, which holds the square root of
    // 2 that Excel would show: a cache is not evidence of a calculation even when it is right.
    let part = Arc::new(SourcePart::new(
        "xl/worksheets/sheet2.xml".to_owned(),
        0x5EED_CAFE,
        4096,
    ));
    let cached_number = |value: f64| {
        FileCached::new(
            CachedValue::Number(FiniteNumber::new(value).unwrap()),
            Arc::clone(&part),
        )
    };
    let cached_text = |value: &str| {
        FileCached::new(
            CachedValue::Text(CellText::new(ExcelText::from_interop_assignment(value)).unwrap()),
            Arc::clone(&part),
        )
    };
    let caches = vec![
        (c(1, 1), cached_number(999.0)),
        (c(2, 1), cached_number(999.0)),
        (c(3, 1), cached_text("wrong")),
        (c(4, 1), cached_number(std::f64::consts::SQRT_2)),
        (c(5, 1), cached_number(std::f64::consts::SQRT_2 + 1.0)),
        (c(6, 1), cached_number(999.0)),
        (c(7, 1), cached_text("wrong")),
        (c(8, 1), cached_number(999.0)),
        (c(9, 1), cached_number(999.0)),
        (c(1, 3), cached_number(999.0)),
    ];
    let cache_values = [
        CalcValue::number(999.0),
        excel_text("wrong"),
        CalcValue::number(std::f64::consts::SQRT_2),
        CalcValue::number(std::f64::consts::SQRT_2 + 1.0),
    ];
    assert_eq!(
        EngineState::with_file_caches(&root1, [(d(1, 1), cached_number(3.0))]).err(),
        Some(CacheWithoutFormula { at: d(1, 1) })
    );
    let mut state = EngineState::with_file_caches(&root1, caches.clone()).unwrap();

    // 1. As opened, every formula's result is the file's cache, FileCached and never calculated,
    //    and every node is pending: every result is stale.
    assert_eq!(state.pending().count(), 10);
    for (at, cache) in &caches {
        assert_eq!(
            state.result(*at),
            Some(&FormulaResult::FileCached(cache.clone()))
        );
        assert_eq!(state.result(*at).unwrap().calculated(), None);
    }
    assert_eq!(calculated_cells(&state), []);
    assert_eq!(state.stale(&root1), Some(all_formulas.clone()));
    let opened = generation(&state);

    // Refusals before any calculation: a snapshot at another revision, and one at the state's
    // revision that lacks a formula the state holds.
    assert_eq!(
        begin_recalc(
            &mut state,
            &ModelRoot::new(rev(0), sheets.clone()).unwrap(),
            NumericMode::Fast
        )
        .err(),
        Some(StartError::StaleInputs {
            engine: rev(1),
            inputs: rev(0)
        })
    );
    assert!(matches!(
        begin_recalc(
            &mut state,
            &ModelRoot::new(rev(1), sheets.clone()).unwrap(),
            NumericMode::Fast
        ),
        Err(StartError::ForeignInputs { .. })
    ));
    assert_eq!(
        state.stale(&ModelRoot::new(rev(0), sheets.clone()).unwrap()),
        None
    );

    // 2. A graph that misses an edge. Calc!C1 reads Calc!A2, which comes after it in the order of
    //    the cells. The first work binds every formula and is finished unevaluated, which is
    //    refused as incomplete. Calc!C1's prerequisites are then stated as none, by hand, so the
    //    next work orders Calc!C1 before Calc!A2. Its evaluation reads Calc!A2, which that work
    //    calculates and had not evaluated: the reader hands over no value (not the cache 999) and
    //    the recalculation fails. Nothing is published.
    let work = begin_recalc(&mut state, &root1, NumericMode::Fast).unwrap();
    assert_eq!((work.tick().get(), work.scheduled()), (1, 10));
    assert_eq!(
        finish(&mut state, work).err(),
        Some(RecalcFailure::Incomplete { remaining: 10 })
    );
    let n_c1 = state.node_at(c(1, 3)).unwrap();
    let c1_reads = state.node(n_c1).unwrap().prerequisites().unwrap().to_vec();
    assert_eq!(c1_reads, [Prerequisite::Cell(c(2, 1))]);
    state.set_prerequisites(&root1, n_c1, vec![]).unwrap();
    let mut recorder = Recorder::default();
    let mut work = begin_recalc(&mut state, &root1, NumericMode::Fast).unwrap();
    assert_eq!(work.tick().get(), 2);
    assert_eq!(
        step(
            &state,
            &root1,
            &mut work,
            WorkBudget::UNLIMITED,
            &mut recorder
        ),
        RecalcProgress::Failed
    );
    // Hand-derived: Calc!A1 is ready first and evaluated; Calc!C1 (row 1) is then ordered
    // before Calc!A2 (row 2).
    assert_eq!(
        recorder.take(),
        [
            read(c(1, 1), cell_area(d(1, 1)), vec![values(&[3.0])]),
            read(c(1, 1), cell_area(d(2, 1)), vec![values(&[4.0])]),
            read(c(1, 3), cell_area(c(2, 1)), vec![Run::Missing(1)]),
        ]
    );
    assert_eq!(
        finish(&mut state, work).err(),
        Some(RecalcFailure::UnscheduledRead {
            formula: c(1, 3),
            cell: c(2, 1)
        })
    );
    assert_eq!(generation(&state), opened);
    assert_eq!(state.pending().count(), 10);
    state.set_prerequisites(&root1, n_c1, c1_reads).unwrap();
    note!(
        "ACCEPT-NOTE: {CASE}: the missed edge of step 2 is made by hand through EngineState::set_prerequisites, which is public for ceng-m1.1's cases; begin_recalc's binding files an edge for every placed reference, so the product route makes no such graph"
    );

    // 3. The first recalculation to finish calculates every formula and replaces every cache.
    let delta = recalc(&mut state, &root1, &mut recorder);
    assert_eq!(delta.tick().get(), 3);
    assert_eq!(delta.changes().len(), 10);
    // Hand-derived: Calc!A1 = 3*4, Calc!A2 = 12+3, Calc!A3 = "big" (15 > 10), Calc!C1 = 15*2.
    assert_eq!(calculated(&state, c(1, 1)), (CalcValue::number(12.0), 1, 3));
    assert_eq!(calculated(&state, c(2, 1)), (CalcValue::number(15.0), 1, 3));
    assert_eq!(calculated(&state, c(3, 1)), (excel_text("big"), 1, 3));
    assert_eq!(calculated(&state, c(1, 3)), (CalcValue::number(30.0), 1, 3));
    // The rest are typed refusals, published without a value, never their caches.
    let refusal =
        |state: &EngineState, at: CellKey, revision: u64, tick: u64| match state.result(at) {
            Some(FormulaResult::NotModeled {
                refusal,
                calculation,
            }) => {
                assert_eq!(
                    (calculation.revision(), calculation.tick().get()),
                    (rev(revision), tick),
                    "{at:?}"
                );
                refusal.clone()
            }
            other => panic!("{at:?} is not a refusal: {other:?}"),
        };
    let NotModeled::Evaluation(EvalFault::Function(power)) = refusal(&state, c(4, 1), 1, 3) else {
        panic!("Calc!A4: {:?}", state.result(c(4, 1)));
    };
    assert!(
        format!("{power:?}").starts_with("NotImplemented"),
        "{power:?}"
    );
    assert_eq!(
        refusal(&state, c(5, 1), 1, 3),
        NotModeled::Evaluation(EvalFault::MissingInput { cell: c(4, 1) })
    );
    assert!(matches!(
        refusal(&state, c(6, 1), 1, 3),
        NotModeled::Compile(CompileError::Reference {
            error: BindError::ExternalWorkbook,
            ..
        })
    ));
    assert_eq!(refusal(&state, c(7, 1), 1, 3), NotModeled::FormulaText);
    // Hand-derived in binary64: 0.1+0.2 is 0.30000000000000004, and that minus 0.3 is
    // 5.551115123125783e-17, within the band CalcFml refuses.
    assert_eq!(
        refusal(&state, c(8, 1), 1, 3),
        NotModeled::Evaluation(EvalFault::NearZeroFinalSum {
            result: 0.1_f64 + 0.2 - 0.3
        })
    );
    assert_eq!(
        refusal(&state, c(9, 1), 1, 3),
        NotModeled::Evaluation(EvalFault::AreaAsValue {
            area: area(data, 1, 1, 2, 1)
        })
    );
    assert_eq!(
        calculated_cells(&state),
        [c(1, 1), c(1, 3), c(2, 1), c(3, 1)]
    );
    assert!(
        state
            .results()
            .all(|(_, result)| !matches!(result, FormulaResult::FileCached(_))),
        "a file cache is still published after the recalculation"
    );
    assert_eq!(state.stale(&root1), Some(vec![]));
    // No read handed over a cache: Calc!A2 read 12 from Calc!A1, Calc!A3 and Calc!C1 read 15
    // from Calc!A2, and Calc!A5's read of Calc!A4, which is not modelled, was given no value.
    let reads = recorder.take();
    assert!(
        reads
            .iter()
            .all(|read| read.runs.iter().all(|run| match run {
                Run::Values(seen) => seen.iter().all(|value| !cache_values.contains(value)),
                Run::Blank(_) | Run::Missing(_) => true,
            }))
    );
    assert!(reads.contains(&read(c(2, 1), cell_area(c(1, 1)), vec![values(&[12.0])])));
    assert!(reads.contains(&read(c(3, 1), cell_area(c(2, 1)), vec![values(&[15.0])])));
    assert!(reads.contains(&read(c(1, 3), cell_area(c(2, 1)), vec![values(&[15.0])])));
    assert!(reads.contains(&read(c(5, 1), cell_area(c(4, 1)), vec![Run::Missing(1)])));
    note!(
        "{CASE}: 10 formulas opened with wrong FileCached results; 0 counted calculated before the first recalculation, 4 after it (the hand-derived 12, 15, \"big\" and 30), 6 typed refusals with no value (a final sum near zero and a range as a value among them), and no read handed over a cache"
    );
    note!(
        "ACCEPT-NOTE: {CASE}: NotModeled::Result (an array, callable, reference, empty or non-finite result) is not reachable through CalcFml's evaluator today (IF gives 0 for an omitted branch, and arithmetic overflow is calc-func's #NUM!); calc-engine's unit test result::tests::only_scalar_values_are_published exercises it"
    );

    // 4. An injected evaluation failure. Data!A1 from 3 to 5 makes Calc!A1, Calc!A2 and Calc!A9
    //    pending, and the dirty closure adds Calc!A3 and Calc!C1. The observer stops the
    //    recalculation at its third observation, Calc!A2's read of Calc!A1, after Calc!A1 was
    //    evaluated and staged: the recalculation fails, and the published generation is
    //    unchanged, value for value and provenance for provenance.
    let calculated_once = generation(&state);
    let root2 = edit(&mut state, &root1, vec![set(d(1, 1), number(5.0))]);
    let pending_before = state.pending().collect::<Vec<_>>();
    assert_eq!(pending_before.len(), 3);
    let stale_before = vec![c(1, 1), c(1, 3), c(2, 1), c(3, 1), c(9, 1)];
    assert_eq!(state.stale(&root2), Some(stale_before.clone()));
    let mut stopper = StopAt {
        observations: 0,
        stop_at: 3,
        recorder: Recorder::default(),
    };
    let mut work = begin_recalc(&mut state, &root2, NumericMode::Fast).unwrap();
    assert_eq!((work.tick().get(), work.scheduled()), (4, 5));
    assert_eq!(
        step(
            &state,
            &root2,
            &mut work,
            WorkBudget::UNLIMITED,
            &mut stopper
        ),
        RecalcProgress::Failed
    );
    assert_eq!(
        stopper.recorder.reads,
        [
            read(c(1, 1), cell_area(d(1, 1)), vec![values(&[5.0])]),
            read(c(1, 1), cell_area(d(2, 1)), vec![values(&[4.0])]),
        ]
    );
    assert_eq!(
        step(
            &state,
            &root2,
            &mut work,
            WorkBudget::UNLIMITED,
            &mut stopper
        ),
        RecalcProgress::Failed
    );
    assert_eq!(
        stopper.observations, 3,
        "a step after the failure evaluated again"
    );
    assert_eq!(
        finish(&mut state, work).err(),
        Some(RecalcFailure::ReadFailed {
            formula: c(2, 1),
            area: cell_area(c(1, 1))
        })
    );
    assert_eq!(generation(&state), calculated_once);
    assert_eq!(state.pending().collect::<Vec<_>>(), pending_before);
    assert_eq!(state.stale(&root2), Some(stale_before));
    note!(
        "{CASE}: an observer that stopped the recalculation at Calc!A2's read aborted it after Calc!A1 was staged; all {} published results are unchanged in value and provenance, and the 5 results of the closure are listed stale",
        calculated_once.len()
    );
    note!(
        "ACCEPT-NOTE: {CASE}: a caller cannot substitute what an evaluation reads: ReadObserver::observe is given each run the engine's reader reports, borrowed, and no reader or sink, and returns only whether to stop; stopping, its one effect, published nothing above. The round-1 ReadLayer, which reported runs of its own, is gone, so no substituting layer can be written"
    );

    // The same recalculation with no observer publishes, at tick 5. Hand-derived: Calc!A1 = 5*4,
    // Calc!A2 = 20+5, Calc!A3 = "big", Calc!C1 = 25*2.
    recalc(&mut state, &root2, &mut Unobserved);
    assert_eq!(calculated(&state, c(1, 1)), (CalcValue::number(20.0), 2, 5));
    assert_eq!(calculated(&state, c(2, 1)), (CalcValue::number(25.0), 2, 5));
    assert_eq!(calculated(&state, c(3, 1)), (excel_text("big"), 2, 5));
    assert_eq!(calculated(&state, c(1, 3)), (CalcValue::number(50.0), 2, 5));
    assert!(matches!(
        refusal(&state, c(9, 1), 2, 5),
        NotModeled::Evaluation(EvalFault::AreaAsValue { .. })
    ));

    // 5. A cycle in the dirty closure refuses the recalculation: Calc!B1 and Calc!B2 read each
    //    other and Calc!B3 reads Calc!B2. The published generation stays as it was and the nodes
    //    stay pending, so the next recalculation is refused the same way. Replacing Calc!B2's
    //    formula by a literal breaks the cycle.
    let before_cycle = generation(&state);
    let root3 = edit(
        &mut state,
        &root2,
        vec![
            set(c(1, 2), formula("B2+1")),
            set(c(2, 2), formula("B1+1")),
            set(c(3, 2), formula("B2*2")),
        ],
    );
    let cycle = Some(StartError::Cycle {
        cells: vec![c(1, 2), c(2, 2), c(3, 2)],
    });
    assert_eq!(
        begin_recalc(&mut state, &root3, NumericMode::Fast).err(),
        cycle
    );
    assert_eq!(generation(&state), before_cycle);
    assert_eq!(state.pending().count(), 3);
    assert_eq!(state.stale(&root3), Some(vec![c(1, 2), c(2, 2), c(3, 2)]));
    assert_eq!(
        begin_recalc(&mut state, &root3, NumericMode::Fast).err(),
        cycle
    );
    let root4 = edit(&mut state, &root3, vec![set(c(2, 2), number(1.0))]);
    let delta = recalc(&mut state, &root4, &mut Unobserved);
    // Hand-derived: Calc!B1 = 1+1, Calc!B3 = 1*2; the refused recalculations used no tick.
    assert_eq!(delta.tick().get(), 6);
    assert_eq!(calculated(&state, c(1, 2)), (CalcValue::number(2.0), 4, 6));
    assert_eq!(calculated(&state, c(3, 2)), (CalcValue::number(2.0), 4, 6));

    // 6. A recalculation publishes nothing when it is finished before it evaluated everything,
    //    when an edit is committed before a step, when an edit is committed after its last step
    //    and before it finishes, and when another recalculation published after it began.
    let root5 = edit(&mut state, &root4, vec![set(d(2, 1), number(6.0))]);
    let before = generation(&state);
    let work = begin_recalc(&mut state, &root5, NumericMode::Fast).unwrap();
    assert_eq!(
        finish(&mut state, work).err(),
        Some(RecalcFailure::Incomplete { remaining: 5 })
    );
    let mut work = begin_recalc(&mut state, &root5, NumericMode::Fast).unwrap();
    let root6 = edit(&mut state, &root5, vec![set(d(2, 1), number(7.0))]);
    assert_eq!(
        step(
            &state,
            &root6,
            &mut work,
            WorkBudget::UNLIMITED,
            &mut Unobserved
        ),
        RecalcProgress::Failed
    );
    assert_eq!(
        finish(&mut state, work).err(),
        Some(RecalcFailure::Superseded { revision: rev(5) })
    );
    assert_eq!(generation(&state), before);
    let mut work = begin_recalc(&mut state, &root6, NumericMode::Fast).unwrap();
    assert_eq!(
        step(
            &state,
            &root6,
            &mut work,
            WorkBudget::UNLIMITED,
            &mut Unobserved
        ),
        RecalcProgress::Complete
    );
    let root7 = edit(&mut state, &root6, vec![set(d(2, 1), number(8.0))]);
    assert_eq!(
        finish(&mut state, work).err(),
        Some(RecalcFailure::Superseded { revision: rev(6) })
    );
    assert_eq!(generation(&state), before);
    let mut first = begin_recalc(&mut state, &root7, NumericMode::Fast).unwrap();
    let mut second = begin_recalc(&mut state, &root7, NumericMode::Fast).unwrap();
    assert_eq!((first.tick().get(), second.tick().get()), (10, 11));
    for work in [&mut first, &mut second] {
        assert_eq!(
            step(&state, &root7, work, WorkBudget::UNLIMITED, &mut Unobserved),
            RecalcProgress::Complete
        );
    }
    finish(&mut state, second).unwrap();
    let published = generation(&state);
    assert_eq!(
        finish(&mut state, first).err(),
        Some(RecalcFailure::Superseded { revision: rev(7) })
    );
    assert_eq!(generation(&state), published);
    // Hand-derived: Calc!A1 = 5*8, Calc!A2 = 40+5, Calc!C1 = 45*2, by the second work's tick.
    assert_eq!(
        calculated(&state, c(1, 1)),
        (CalcValue::number(40.0), 7, 11)
    );
    assert_eq!(
        calculated(&state, c(2, 1)),
        (CalcValue::number(45.0), 7, 11)
    );
    assert_eq!(
        calculated(&state, c(1, 3)),
        (CalcValue::number(90.0), 7, 11)
    );
    note!(
        "{CASE}: a cycle (refused twice), an unfinished work, works superseded by an edit before a step and after the last step, and a work superseded by another's publication were refused with the generation unchanged"
    );

    // 7. One state, two numeric modes. Calc!A10 = A1 enters, reading Calc!A1, which was
    //    calculated under Fast, and so does Calc!A11 = Data!A1. The recalculation under Exact
    //    calculates every formula node, not only the two new ones, so no input calculated under
    //    Fast is read by a result published under Exact. calc-func has no Exact arithmetic: Calc!A1
    //    is refused under Exact and Calc!A10, which reads it, is refused too; the bare reference
    //    Calc!A11 is calculated, with the Excel target in its provenance.
    let target = NumericTargetId::new("16.0.19029.20000", "Current", "Excel 2016", "x64").unwrap();
    let exact = NumericMode::Exact(target.clone());
    let root8 = edit(
        &mut state,
        &root7,
        vec![
            set(c(10, 1), formula("A1")),
            set(c(11, 1), formula("Data!A1")),
        ],
    );
    assert_eq!(state.stale(&root8), Some(vec![c(10, 1), c(11, 1)]));
    let mut work = begin_recalc(&mut state, &root8, exact.clone()).unwrap();
    let scheduled = work.scheduled();
    assert_eq!(
        step(
            &state,
            &root8,
            &mut work,
            WorkBudget::UNLIMITED,
            &mut Unobserved
        ),
        RecalcProgress::Complete
    );
    let delta = finish(&mut state, work).unwrap();
    assert_eq!(
        refusal(&state, c(10, 1), 8, 12),
        NotModeled::Evaluation(EvalFault::MissingInput { cell: c(1, 1) }),
        "Calc!A10 under Exact read Calc!A1's Fast result"
    );
    let NotModeled::Evaluation(EvalFault::Function(fault)) = refusal(&state, c(1, 1), 8, 12) else {
        panic!("Calc!A1 under Exact: {:?}", state.result(c(1, 1)));
    };
    assert!(
        format!("{fault:?}").starts_with("ExactUnavailable"),
        "{fault:?}"
    );
    let Some(FormulaResult::Calculated { value, calculation }) = state.result(c(11, 1)) else {
        panic!("Calc!A11 under Exact: {:?}", state.result(c(11, 1)));
    };
    assert_eq!(value, &CalcValue::number(5.0));
    assert_eq!(calculation.target(), Some(&target));
    assert_eq!(calculated_cells(&state), [c(11, 1)]);
    let node_count = state.node_count();
    assert_eq!(node_count, 14);
    assert_eq!(scheduled, node_count);
    assert_eq!(delta.changes().len(), node_count);
    for (at, result) in state.results() {
        let calculation = match result {
            FormulaResult::Calculated { calculation, .. }
            | FormulaResult::NotModeled { calculation, .. } => calculation,
            FormulaResult::FileCached(_) => panic!("{at:?} holds a file cache"),
        };
        assert_eq!(
            (
                calculation.revision(),
                calculation.tick().get(),
                calculation.mode()
            ),
            (rev(8), 12, &exact),
            "{at:?}"
        );
    }
    // Back under Fast, every node is calculated again, Calc!A10 from Calc!A1's Fast result.
    // Hand-derived: Calc!A1 = 5*8, Calc!A10 = Calc!A1, Calc!A11 = Data!A1.
    let mut work = begin_recalc(&mut state, &root8, NumericMode::Fast).unwrap();
    assert_eq!(work.scheduled(), node_count);
    assert_eq!(
        step(
            &state,
            &root8,
            &mut work,
            WorkBudget::UNLIMITED,
            &mut Unobserved
        ),
        RecalcProgress::Complete
    );
    finish(&mut state, work).unwrap();
    assert_eq!(
        calculated(&state, c(1, 1)),
        (CalcValue::number(40.0), 8, 13)
    );
    assert_eq!(
        calculated(&state, c(10, 1)),
        (CalcValue::number(40.0), 8, 13)
    );
    assert_eq!(
        calculated(&state, c(11, 1)),
        (CalcValue::number(5.0), 8, 13)
    );
    assert!(
        state
            .results()
            .filter_map(|(_, result)| result.calculated())
            .all(|(_, calculation)| calculation.mode() == &NumericMode::Fast)
    );
    // Under the published mode, with nothing pending, nothing is calculated.
    let work = begin_recalc(&mut state, &root8, NumericMode::Fast).unwrap();
    assert_eq!(work.scheduled(), 0);
    assert_eq!(finish(&mut state, work).unwrap().changes(), []);
    note!(
        "{CASE}: under Exact after Fast, all {node_count} formula nodes were calculated again and none published as Calculated from a Fast result (Calc!A10 refused, Calc!A11 calculated with its target); back under Fast all {node_count} again"
    );
}

// ---------------------------------------------------------------------------------------------
// dc-qguw: the evaluation record a verdict counts calculations against is made from what CalcFml's
// evaluator reports to the instrument on the evaluation route (ReadObserver::called and
// ReadObserver::evaluated, shown by `step`), never from the results the engine publishes. The
// cases below make that record (calc-verdict's version 2, with what each evaluation returned),
// publish results as the engine published them or as a deliberately faulty publication path
// would, and judge both with calc_verdict::judge, the one verdict path.
//
// Natively each case also writes the capture record, result set and evaluation record it judged
// under target/verdict/dc-qguw/, where the bead's `cargo xtask verdict` commands read them.

/// The instrument on the evaluation route, as a caller attaches it to `step`: every call and every
/// evaluation outcome the evaluator reported, with the formula and the calculation the engine
/// gave. It observes reads without recording them.
#[derive(Debug, Default)]
struct Instrument {
    /// Each call as `<cell> <notation> <spelling>`, with ` *` when compile inserted it.
    calls: Vec<String>,
    evaluations: Vec<Recorded>,
}

/// One evaluation the instrument was shown: its cell, revision, tick and numeric mode, and what it
/// returned.
type Recorded = (CellKey, u64, u64, NumericMode, Result<CalcValue, EvalFault>);

impl ReadObserver for Instrument {
    fn observe(&mut self, _: CellKey, _: GridArea, _: u64, _: CellRun<'_>) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn called(&mut self, formula: CellKey, call: ExecutedCall) {
        let target = call.target();
        self.calls.push(format!(
            "R{}C{} {:?} {}{}",
            formula.row(),
            formula.column(),
            target.notation(),
            target.spelling(),
            if call.synthetic() { " *" } else { "" }
        ));
    }

    fn evaluated(&mut self, evaluation: ObservedEvaluation<'_>) {
        // The cell, numeric mode and outcome are those of the report CalcFml's evaluator made,
        // which the engine can only pass on; the engine adds the revision and tick.
        let calculation = evaluation.calculation();
        let report = evaluation.report();
        assert_eq!(report.numeric_mode(), calculation.mode());
        self.evaluations.push((
            report.anchor().cell(),
            calculation.revision().get(),
            calculation.tick().get(),
            report.numeric_mode().clone(),
            report.outcome().cloned().map_err(Clone::clone),
        ));
    }
}

/// The case and source every document of the dc-qguw cases names, and the fingerprint of the
/// calculation context the engine calculates under here: Fast, its one setting.
const QGUW_CASE: &str = "dc_qguw__workbook";
const QGUW_SOURCE: &str = "fixture:dc-qguw/workbook";
const QGUW_CONTEXT: &str = "calc-engine;numeric_mode=fast";

/// The dc-qguw workbook: Data!A1 = 3 and Data!A2 = 4, and on Calc three formulas whose file caches
/// hold the answers (hand-derived): A1 `=Data!A1*Data!A2` (3 * 4 = 12), A2 `=-Data!A1+10`
/// (-3 + 10 = 7, a final sum whose near-zero check runs) and A3 `=1E-300*1E-10` (a product below
/// the normal range, whose cache holds 0, which calc-func does not compute: its evaluation is
/// refused). The engine state is opened with those caches.
struct QguwBook {
    data: SheetId,
    calc: SheetId,
    root: Arc<ModelRoot>,
    state: EngineState,
}

impl QguwBook {
    fn new() -> Self {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let data = ids.issue().unwrap();
        let calc = ids.issue().unwrap();
        let sheets = vec![sheet_entry(data, 1, "Data"), sheet_entry(calc, 2, "Calc")];
        let load = batch(
            0,
            vec![
                set(key(data, 1, 1), number(3.0)),
                set(key(data, 2, 1), number(4.0)),
                set(key(calc, 1, 1), formula("Data!A1*Data!A2")),
                set(key(calc, 2, 1), formula("-Data!A1+10")),
                set(key(calc, 3, 1), formula("1E-300*1E-10")),
            ],
        );
        let root = ModelRoot::new(rev(0), sheets)
            .unwrap()
            .apply(&load)
            .unwrap();
        let part = Arc::new(SourcePart::new(
            "xl/worksheets/sheet2.xml".to_owned(),
            0x0D0C_9000,
            2048,
        ));
        let cache = |value: f64| {
            FileCached::new(
                CachedValue::Number(FiniteNumber::new(value).unwrap()),
                Arc::clone(&part),
            )
        };
        let state = EngineState::with_file_caches(
            &root,
            [
                (key(calc, 1, 1), cache(12.0)),
                (key(calc, 2, 1), cache(7.0)),
                (key(calc, 3, 1), cache(0.0)),
            ],
        )
        .unwrap();
        QguwBook {
            data,
            calc,
            root,
            state,
        }
    }

    fn c(&self, row: u32) -> CellKey {
        key(self.calc, row, 1)
    }

    /// The cell `at` as the verdict's documents name it.
    fn address(&self, at: CellKey) -> CellAddress {
        let sheet = if at.sheet() == self.data {
            "Data"
        } else {
            assert_eq!(at.sheet(), self.calc);
            "Calc"
        };
        CellAddress::new(sheet, at.row(), at.column()).unwrap()
    }

    /// The expectation the results are judged against: the workbook's file caches, read as a
    /// file-cache replay of caches of unknown freshness (no Excel ran; the caches are the
    /// hand-derived answers above).
    fn capture(&self) -> Vec<u8> {
        format!(
            r#"{{"format": "dnacalc-capture-record", "version": 1, "case": "{QGUW_CASE}", "source": "{QGUW_SOURCE}",
  "acquisition": {{"kind": "file_cache_replay"}}, "environment": null,
  "sheets": [{{"name": "Data", "visibility": "visible"}}, {{"name": "Calc", "visibility": "visible"}}],
  "formulas": [{{"sheet": "Calc", "row": 1, "column": 1, "kind": "ordinary"}},
               {{"sheet": "Calc", "row": 2, "column": 1, "kind": "ordinary"}},
               {{"sheet": "Calc", "row": 3, "column": 1, "kind": "ordinary"}}],
  "observations": [{{"sheet": "Calc", "row": 1, "column": 1, "unknown_freshness_cache": {{"t": "n", "v": "12"}}}},
                   {{"sheet": "Calc", "row": 2, "column": 1, "unknown_freshness_cache": {{"t": "n", "v": "7"}}}},
                   {{"sheet": "Calc", "row": 3, "column": 1, "unknown_freshness_cache": {{"t": "n", "v": "0"}}}}]}}
"#
        )
        .into_bytes()
    }

    /// The evaluation record made from what `instrument` was shown: calc-verdict's version 2, with
    /// what each evaluation returned (a refusal as no value, with the evaluator's fault), each entry
    /// made as a writer makes it ([`InstrumentedEvaluation::new`]).
    fn record(&self, instrument: &Instrument) -> InstrumentRecord {
        InstrumentRecord {
            case: QGUW_CASE.to_owned(),
            source: QGUW_SOURCE.to_owned(),
            evaluations: instrument
                .evaluations
                .iter()
                .map(|(at, revision, tick, mode, outcome)| {
                    assert_eq!(mode, &NumericMode::Fast, "the context names Fast");
                    InstrumentedEvaluation::new(
                        VerdictEvaluation {
                            cell: self.address(*at),
                            tick: *tick,
                            revision: *revision,
                            context: QGUW_CONTEXT.to_owned(),
                        },
                        outcome
                            .clone()
                            .map_err(|fault| format!("CalcFml refused: {fault:?}")),
                    )
                })
                .collect(),
        }
    }

    /// A result set of the run at the state's revision: one result per entry, each a cell, its
    /// provenance as the result set writes it, and its number, if any.
    fn results(&self, published: &[(CellKey, String, Option<f64>)]) -> Vec<u8> {
        let results: Vec<String> = published
            .iter()
            .map(|(at, provenance, value)| {
                let address = self.address(*at);
                let value = value
                    .map(|n| format!(r#", "value": {{"number": "{:016x}"}}"#, n.to_bits()))
                    .unwrap_or_default();
                format!(
                    r#"    {{"sheet": "{}", "row": {}, "column": {}, "provenance": {provenance}{value}}}"#,
                    address.sheet(),
                    address.row(),
                    address.column()
                )
            })
            .collect();
        format!(
            "{{\"format\": \"dnacalc-result-set\", \"version\": 1, \"case\": \"{QGUW_CASE}\", \"source\": \"{QGUW_SOURCE}\",\n  \"run\": {{\"revision\": {}, \"context\": \"{QGUW_CONTEXT}\"}},\n  \"results\": [\n{}\n  ]}}\n",
            self.state.revision().get(),
            results.join(",\n")
        )
        .into_bytes()
    }

    /// The engine's published generation as the result set states it: a calculated result with
    /// its tick and revision, a file cache as file-cached, a refusal as not modelled.
    fn published(&self) -> Vec<(CellKey, String, Option<f64>)> {
        self.state
            .results()
            .map(|(at, result)| match result {
                FormulaResult::Calculated { value, calculation } => (
                    at,
                    calculated_claim(calculation.tick().get(), calculation.revision().get()),
                    Some(value.as_number().expect("the case's values are numbers")),
                ),
                FormulaResult::FileCached(cache) => (
                    at,
                    format!(
                        r#"{{"file_cached": {{"source": "{}"}}}}"#,
                        cache.source().name()
                    ),
                    Some(cache_number(cache)),
                ),
                FormulaResult::NotModeled { refusal, .. } => (
                    at,
                    format!(
                        r#"{{"not_modeled": {{"feature": "not modelled by calc-engine", "diagnostic": "{}"}}}}"#,
                        refusal.to_string().replace('"', "'")
                    ),
                    None,
                ),
            })
            .collect()
    }
}

fn calculated_claim(tick: u64, revision: u64) -> String {
    format!(
        r#"{{"calculated": {{"tick": {tick}, "revision": {revision}, "context": "{QGUW_CONTEXT}"}}}}"#
    )
}

fn cache_number(cache: &FileCached) -> f64 {
    match cache.value() {
        CachedValue::Number(number) => number.get(),
        other => panic!("the case's caches are numbers: {other:?}"),
    }
}

/// The report calc_verdict::judge computes, from the bytes of the three documents, as `cargo
/// xtask verdict` reads them: the evaluation record's own version chooses how it is read.
fn qguw_judged(capture: &[u8], results: &[u8], evaluations: &[u8]) -> VerdictReport {
    let capture = CaptureRecord::decode(capture).expect("the capture record decodes");
    let results = ResultSet::decode(results).expect("the result set decodes");
    let policy = VerdictPolicy::default();
    match evaluation_record_version(evaluations).expect("an evaluation record") {
        1 => judge(
            &capture,
            &results,
            &EvaluationRecord::decode(evaluations).expect("the version 1 record decodes"),
            &policy,
        ),
        _ => judge(
            &capture,
            &results,
            &InstrumentRecord::decode(evaluations).expect("the version 2 record decodes"),
            &policy,
        ),
    }
    .expect("the documents are consistent")
}

/// Each inventory cell's verdict as `<cell> <verdict>`, with ` tick=<tick>` and what an unproven
/// cell's evaluation returned, then ` equal=<bool>` when a value that is not calculated was
/// compared with the expectation.
fn qguw_cells(report: &VerdictReport) -> Vec<String> {
    report
        .cells
        .iter()
        .map(|cell| {
            let mut line = format!("{} {}", cell.formula.cell.a1(), cell.verdict.name());
            if let Verdict::Unproven { tick, returned } = &cell.verdict {
                line.push_str(&format!(" tick={tick}"));
                match returned {
                    None => {}
                    Some(Ok(value)) => line.push_str(&format!(
                        " returned={}",
                        value.as_number().expect("the case's values are numbers")
                    )),
                    Some(Err(_)) => line.push_str(" returned=no_value"),
                }
            }
            if let Some(equal) = cell.value_equals_expectation {
                line.push_str(&format!(" equal={equal}"));
            }
            line
        })
        .collect()
}

/// `(denominator, calculated, matched, mismatched, unproven, not_modeled,
/// not_calculated_equal_to_expectation, evaluator_calls, calculated_without_value_check)` of a
/// report.
fn qguw_counts(report: &VerdictReport) -> [u64; 9] {
    let c = &report.counts;
    [
        c.denominator,
        c.calculated,
        c.matched,
        c.mismatched,
        c.unproven,
        c.not_modeled,
        c.not_calculated_equal_to_expectation,
        c.evaluator_calls,
        c.calculated_without_value_check,
    ]
}

/// The evaluations `instrument` was shown, in order, each as its cell, revision, tick, numeric
/// mode and the value it returned (`None` for a refusal).
fn qguw_evaluations(
    instrument: &Instrument,
) -> Vec<(CellKey, u64, u64, NumericMode, Option<CalcValue>)> {
    instrument
        .evaluations
        .iter()
        .map(|(at, revision, tick, mode, outcome)| {
            (*at, *revision, *tick, mode.clone(), outcome.clone().ok())
        })
        .collect()
}

/// Writes the documents a case judged under target/verdict/dc-qguw/<case>/, natively, for the
/// bead's `cargo xtask verdict` commands; the browser has no file system to write to.
fn qguw_write(case: &str, documents: &[(&str, &[u8])]) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/verdict/dc-qguw")
            .join(case);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (name, bytes) in documents {
            std::fs::write(dir.join(name), bytes).unwrap();
        }
        note!(
            "ACCEPT-NOTE: dc_qguw__{case}: wrote {} documents to target/verdict/dc-qguw/{case}/ for cargo xtask verdict",
            documents.len()
        );
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = documents;
        note!(
            "ACCEPT-NOTE: dc_qguw__{case}: the browser has no file system; the documents for cargo xtask verdict are written by the native run"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// dc_qguw__fault_injection

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_qguw__fault_injection() {
    const CASE: &str = "dc_qguw__fault_injection";
    note!(
        "ACCEPT-NOTE: {CASE}: the expectations are the workbook's file caches, hand-derived (3*4 = 12, -3+10 = 7, 1E-300*1E-10 shown as 0); no Excel ran, and agreement with Excel is dc-35p's round trip"
    );
    let mut book = QguwBook::new();
    let capture = book.capture();
    let (a1, a2, a3) = (book.c(1), book.c(2), book.c(3));
    assert_eq!(book.state.revision().get(), 1);

    // 1. The fault. A recalculation begins (tick 1, revision 1), and one step with a budget of one
    //    evaluation runs its first node, A1, with the instrument attached; A2 and A3 are not yet
    //    evaluated at tick 1. A faulty publication path then publishes every formula's perfect
    //    cached answer under calculated provenance at tick 1 and revision 1. The record made from
    //    what the evaluator reported holds A1's evaluation and nothing else, so the verdict counts
    //    A1 calculated and matched and the two cells not yet evaluated unproven, though their
    //    published values equal the expectation and their claims name the tick that is running.
    assert_eq!(book.state.pending().count(), 3);
    let opened = book.published();
    assert!(
        opened
            .iter()
            .all(|(_, provenance, _)| provenance.contains("file_cached"))
    );
    let root = Arc::clone(&book.root);
    let mut instrument = Instrument::default();
    let mut work = begin_recalc(&mut book.state, &root, NumericMode::Fast).unwrap();
    assert_eq!(
        step(
            &book.state,
            &root,
            &mut work,
            WorkBudget::evaluations(1),
            &mut instrument
        ),
        RecalcProgress::Pending { remaining: 2 }
    );
    assert_eq!(
        qguw_evaluations(&instrument),
        [(a1, 1, 1, NumericMode::Fast, Some(CalcValue::number(12.0)))]
    );
    assert_eq!(instrument.calls, ["R1C1 Infix *"]);
    let faulty_claims: Vec<(CellKey, String, Option<f64>)> = opened
        .iter()
        .map(|(at, _, value)| (*at, calculated_claim(1, 1), *value))
        .collect();
    let faulty = book.results(&faulty_claims);
    let one_step_record = book.record(&instrument).encode().unwrap();
    let report = qguw_judged(&capture, &faulty, one_step_record.as_bytes());
    assert_eq!(
        qguw_cells(&report),
        [
            "A1 matched",
            "A2 unproven tick=1 equal=true",
            "A3 unproven tick=1 equal=true",
        ]
    );
    assert_eq!(qguw_counts(&report), [3, 1, 1, 0, 2, 0, 2, 1, 0]);
    assert_eq!(report.evaluation_record_version, 2);
    // Until finish, the engine itself publishes the file's caches as file-cached, which the
    // verdict counts apart.
    let as_opened = book.results(&book.published());
    let report = qguw_judged(&capture, &as_opened, one_step_record.as_bytes());
    assert_eq!(
        (report.counts.calculated, report.counts.file_cached),
        (0, 3)
    );
    note!(
        "ACCEPT-NOTE: {CASE}: after one step of budget 1 at tick 1, three perfect cached answers published as calculated at tick 1: calculated 1 of 3 (A1, evaluated, matched), unproven 2 (A2 and A3, not yet evaluated), not_calculated_equal_to_expectation 2, evaluator_calls 1"
    );

    // 2. The recalculation completes with the instrument attached and publishes.
    assert_eq!(
        step(
            &book.state,
            &root,
            &mut work,
            WorkBudget::UNLIMITED,
            &mut instrument
        ),
        RecalcProgress::Complete
    );
    let delta = finish(&mut book.state, work).unwrap();
    assert_eq!(delta.tick().get(), 1);
    // What the evaluator reported, in the order of evaluation: one evaluation per formula, each
    // at revision 1 and tick 1 under Fast. A3's evaluation was refused (calc-func does not compute
    // a product below the normal range), and the engine published no value for it.
    assert_eq!(
        qguw_evaluations(&instrument),
        [
            (a1, 1, 1, NumericMode::Fast, Some(CalcValue::number(12.0))),
            (a2, 1, 1, NumericMode::Fast, Some(CalcValue::number(7.0))),
            (a3, 1, 1, NumericMode::Fast, None),
        ]
    );
    assert!(matches!(
        instrument.evaluations[2].4,
        Err(EvalFault::Function(_))
    ));
    // Each call the evaluations ran. A2's final sum is checked near zero by two negations compile
    // inserted, marked synthetic; the negation A2's formula writes is not.
    assert_eq!(
        instrument.calls,
        [
            "R1C1 Infix *",
            "R2C1 Prefix -",
            "R2C1 Infix +",
            "R2C1 Prefix - *",
            "R2C1 Prefix - *",
            "R3C1 Infix *",
        ]
    );
    let published = book.published();
    let calculated_results = book.results(&published);
    let record = book.record(&instrument).encode().unwrap();
    // The calculated claims for A1 and A2 are the very claims the faulty path made: the result
    // set alone cannot tell a calculation from a relabelled cache. Only the record differs.
    assert_eq!(published[..2], faulty_claims[..2]);
    let report = qguw_judged(&capture, &calculated_results, record.as_bytes());
    assert_eq!(
        qguw_cells(&report),
        ["A1 matched", "A2 matched", "A3 not_modeled"]
    );
    assert_eq!(qguw_counts(&report), [3, 2, 2, 0, 0, 1, 0, 3, 0]);
    // The faulty claims against the complete record: A2, now evaluated at tick 1, is matched;
    // A3's cached answer is still unproven, its evaluation having returned no value.
    let report = qguw_judged(&capture, &faulty, record.as_bytes());
    assert_eq!(
        qguw_cells(&report),
        [
            "A1 matched",
            "A2 matched",
            "A3 unproven tick=1 returned=no_value equal=true",
        ]
    );
    assert_eq!(qguw_counts(&report), [3, 2, 2, 0, 1, 0, 1, 3, 0]);
    note!(
        "ACCEPT-NOTE: {CASE}: the recalculation completed through step and finish with the instrument attached: the engine's results calculated 2 of 3, matched 2 (A1 12, A2 7), not_modeled 1 (A3, refused), evaluator_calls 3; the faulty claims against the complete record: A2 matched, A3 unproven (returned no value); A2's two near-zero negations reported synthetic"
    );

    qguw_write(
        "fault_injection",
        &[
            ("capture.json", &capture),
            ("results_faulty.json", &faulty),
            ("evaluations_one_step.json", one_step_record.as_bytes()),
            ("results_calculated.json", &calculated_results),
            ("evaluations_calculated.json", record.as_bytes()),
        ],
    );
}

// ---------------------------------------------------------------------------------------------
// dc_qguw__published_value_differs

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_qguw__published_value_differs() {
    const CASE: &str = "dc_qguw__published_value_differs";
    note!(
        "ACCEPT-NOTE: {CASE}: the expectations are the workbook's file caches, hand-derived; no Excel ran"
    );
    let mut book = QguwBook::new();
    let capture = book.capture();
    let (a1, a3) = (book.c(1), book.c(3));
    let mut instrument = Instrument::default();
    let root = Arc::clone(&book.root);
    recalc(&mut book.state, &root, &mut instrument);
    let record = book.record(&instrument);
    let encoded = record.encode().unwrap();

    // A faulty publication path after the recalculation: A1 is published as 13, not the 12 its
    // evaluation returned, and A3, whose evaluation was refused, is published with the file's
    // cached answer under calculated provenance, at the tick that evaluated it (a hidden
    // fallback). A2 is published as the engine published it.
    let faulty: Vec<(CellKey, String, Option<f64>)> = book
        .published()
        .into_iter()
        .map(|(at, provenance, value)| {
            if at == a1 {
                (at, provenance, Some(13.0))
            } else if at == a3 {
                (at, calculated_claim(1, 1), Some(0.0))
            } else {
                (at, provenance, value)
            }
        })
        .collect();
    let faulty = book.results(&faulty);
    let report = qguw_judged(&capture, &faulty, encoded.as_bytes());
    assert_eq!(
        qguw_cells(&report),
        [
            "A1 unproven tick=1 returned=12 equal=false",
            "A2 matched",
            "A3 unproven tick=1 returned=no_value equal=true",
        ]
    );
    assert_eq!(qguw_counts(&report), [3, 1, 1, 0, 2, 0, 1, 3, 0]);
    assert_eq!(report.evaluation_record_version, 2);
    note!(
        "ACCEPT-NOTE: {CASE}: A1 evaluated to 12 and published as 13, A3 refused and published with its cached answer, both under calculated provenance at tick 1: calculated 1 of 3, matched 1 (A2), unproven 2"
    );

    // The same evaluations without what they returned (an evaluation record of version 1) prove
    // whatever was published at their tick: A1's 13 counts as calculated, and A3's cached answer
    // as calculated and matched. The report says so: it names the record's version 1 and counts
    // all three calculated cells as proven without a value check (bead dc-i1ro moves the version 1
    // records to version 2 and refuses version 1).
    let listed = EvaluationRecord {
        case: record.case.clone(),
        source: record.source.clone(),
        evaluations: record
            .evaluations
            .iter()
            .map(|entry| entry.evaluation.clone())
            .collect(),
    };
    let capture_record = CaptureRecord::decode(&capture).unwrap();
    let faulty_set = ResultSet::decode(&faulty).unwrap();
    let without_values = judge(
        &capture_record,
        &faulty_set,
        &listed,
        &VerdictPolicy::default(),
    )
    .unwrap();
    assert_eq!(
        qguw_cells(&without_values),
        ["A1 mismatched", "A2 matched", "A3 matched"]
    );
    assert_eq!(qguw_counts(&without_values), [3, 3, 2, 1, 0, 0, 0, 3, 3]);
    assert_eq!(without_values.evaluation_record_version, 1);
    let written = without_values.encode();
    assert!(
        written.contains("\"evaluation_record_version\": 1")
            && written.contains("\"calculated_without_value_check\": 3"),
        "{written}"
    );
    assert!(
        without_values
            .summary()
            .contains("(evaluation record version 1, 3 calculated without a value check)"),
        "{}",
        without_values.summary()
    );
    note!(
        "ACCEPT-NOTE: {CASE}: judged against the same evaluations without returned values (version 1), the faulty publication counts calculated 3 of 3 and matched 2, A3's cached answer among them, and the report marks the weaker proof: evaluation_record_version 1, calculated_without_value_check 3"
    );

    qguw_write(
        "published_value_differs",
        &[
            ("capture.json", &capture),
            ("results.json", &faulty),
            ("evaluations.json", encoded.as_bytes()),
        ],
    );
}

// ---------------------------------------------------------------------------------------------
// L1 (dc-mzk) through the engine's route: formulas that use defined names (workbook and sheet
// scope, named LAMBDAs), array constants, LET, LAMBDA, the reference operators and OFFSET are
// bound by begin_recalc through CalcFml, with the snapshot's names, and calculated by step. Every
// expected value is derived by hand from the literals and the formula texts.

fn l1_name(name: &str, scope: NameScope, formula: &str) -> DefinedName {
    DefinedName {
        name: name.to_owned(),
        scope,
        formula: formula.to_owned(),
        hidden: false,
        comment: None,
        function: false,
        vb_procedure: false,
        xlm: false,
    }
}

/// Builds a two-sheet workbook (Data, Calc) with `cells` and `names`, recalculates it once and
/// gives the state.
fn l1_calculated(
    cells: Vec<(CellKey, CellInput)>,
    names: Vec<DefinedName>,
    sheets: (SheetId, SheetId),
) -> EngineState {
    l1_calculated_with(cells, names, vec![], sheets)
}

/// [`l1_calculated`] with the array formulas `arrays`.
fn l1_calculated_with(
    cells: Vec<(CellKey, CellInput)>,
    names: Vec<DefinedName>,
    arrays: Vec<ArrayFormula>,
    sheets: (SheetId, SheetId),
) -> EngineState {
    let entries = vec![
        sheet_entry(sheets.0, 1, "Data"),
        sheet_entry(sheets.1, 2, "Calc"),
    ];
    let root: Arc<ModelRoot> = ModelRoot::with_definitions(
        AuthoredRevision::new(0),
        entries,
        cells,
        SkinLayout::default(),
        Definitions::new(arrays, names, vec![], []),
    )
    .unwrap();
    let mut state = EngineState::new(&root);
    let mut work = begin_recalc(&mut state, &root, NumericMode::Fast).unwrap();
    assert_eq!(
        step(
            &state,
            &root,
            &mut work,
            WorkBudget::UNLIMITED,
            &mut Unobserved
        ),
        RecalcProgress::Complete
    );
    finish(&mut state, work).unwrap();
    state
}

fn l1_value(state: &EngineState, at: CellKey) -> CalcValue {
    match state.result(at) {
        Some(FormulaResult::Calculated { value, .. }) => value.clone(),
        other => panic!("{at:?}: {other:?}"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_mzk__l1_through_the_engine() {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let (data, calc) = (ids.issue().unwrap(), ids.issue().unwrap());
    let d = |row, column| CellKey::new(data, row, column).unwrap();
    let c = |row, column| CellKey::new(calc, row, column).unwrap();
    let mut cells = vec![
        (d(1, 1), number(10.0)),
        (d(2, 1), number(20.0)),
        (d(3, 1), number(30.0)),
        (d(1, 2), number(0.25)),
    ];
    let formulas = [
        // 10+20+30.
        (c(1, 1), "SUM(Inputs)"),
        // Calc's own Rate, 0.5, shadows the workbook's: 60 * (1 + 0.5).
        (c(2, 1), "SUM(Inputs)*(1+Rate)"),
        (c(3, 1), "Rate"),
        // On Data the workbook's Rate, Data!B1: 0.25.
        (d(1, 3), "Rate*1"),
        // The named LAMBDA: 7^2.
        (c(4, 1), "Square(7)"),
        // An array constant: 1+2+3+4.
        (c(5, 1), "SUM({1,2;3,4})"),
        // LET and a LET-bound LAMBDA, in the file form: (5+1)*2.
        (
            c(6, 1),
            "_xlfn.LET(_xlpm.f,_xlfn.LAMBDA(_xlpm.x,_xlpm.x+1),_xlpm.f(5)*2)",
        ),
        // OFFSET from Data!A1 down two rows: 30.
        (c(7, 1), "OFFSET(Data!A1,2,0)"),
        // The union: 10 + 30.
        (c(8, 1), "SUM((Data!A1,Data!A3))"),
        // The intersection of A1:A3 and A2:B2: 20.
        (c(9, 1), "SUM(Data!A1:A3 Data!A2:B2)"),
        // A name the workbook does not hold: #NAME?.
        (c(10, 1), "NoSuchName"),
        // A legacy formula intersects a range with its own row: Calc!A2 is row 2, Data!A2 = 20.
        (c(2, 3), "Data!A1:A3*1"),
        // SUMPRODUCT lifts: 10*1 + 20*2 + 30*3.
        (c(1, 4), "SUMPRODUCT(Data!A1:A3*{1;2;3})"),
    ];
    cells.extend(formulas.iter().map(|(at, text)| (*at, formula(text))));
    let names = vec![
        l1_name("Inputs", NameScope::Workbook, "Data!$A$1:$A$3"),
        l1_name("Rate", NameScope::Workbook, "Data!$B$1"),
        l1_name("Rate", NameScope::Sheet(calc), "0.5"),
        l1_name(
            "Square",
            NameScope::Workbook,
            "_xlfn.LAMBDA(_xlpm.x,_xlpm.x*_xlpm.x)",
        ),
    ];
    let state = l1_calculated(cells, names, (data, calc));
    let n = CalcValue::number;
    for (at, expected) in [
        (c(1, 1), n(60.0)),
        (c(2, 1), n(90.0)),
        (d(1, 3), n(0.25)),
        (c(3, 1), n(0.5)),
        (c(4, 1), n(49.0)),
        (c(5, 1), n(10.0)),
        (c(6, 1), n(12.0)),
        (c(7, 1), n(30.0)),
        (c(8, 1), n(40.0)),
        (c(9, 1), n(20.0)),
        (
            c(10, 1),
            CalcValue::error(calc_doc_model::value::WorksheetErrorCode::Name),
        ),
        (c(2, 3), n(20.0)),
        (c(1, 4), n(140.0)),
    ] {
        assert_eq!(l1_value(&state, at), expected, "{at:?}");
    }
}

/// An array formula of one cell evaluates as an array formula: a legacy (Ctrl+Shift+Enter) one
/// shows the first element of its result, a dynamic-array one its one value; one whose result has
/// more values spills them (L2). Hand-derived: Data!A1:A3 = 10, 20, 30, so SUM(A1:A3*{1;2;3}) is
/// 140 and A1:A3*2 is 20, 40, 60.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_mzk__one_cell_array_formulas() {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let (data, calc) = (ids.issue().unwrap(), ids.issue().unwrap());
    let d = |row, column| CellKey::new(data, row, column).unwrap();
    let c = |row, column| CellKey::new(calc, row, column).unwrap();
    let cells = vec![
        (d(1, 1), number(10.0)),
        (d(2, 1), number(20.0)),
        (d(3, 1), number(30.0)),
        (c(1, 1), formula("SUM(Data!A1:A3*Data!A1:A3/10)")),
        (c(2, 1), formula("Data!A1:A3*2")),
        (c(3, 1), formula("SUM(Data!A1:A3*{1;2;3})")),
        (c(5, 1), formula("Data!A1:A3*2")),
    ];
    let one = |at: CellKey, kind| ArrayFormula::new(GridArea::cell(at), kind);
    let arrays = vec![
        one(c(1, 1), ArrayKind::Legacy),
        one(c(2, 1), ArrayKind::Legacy),
        one(c(3, 1), ArrayKind::Dynamic { collapsed: false }),
        ArrayFormula::new(
            GridArea::new(c(5, 1), c(7, 1)).unwrap(),
            ArrayKind::Dynamic { collapsed: false },
        ),
    ];
    let state = l1_calculated_with(cells, vec![], arrays, (data, calc));
    assert_eq!(l1_value(&state, c(1, 1)), CalcValue::number(140.0));
    assert_eq!(l1_value(&state, c(2, 1)), CalcValue::number(20.0));
    assert_eq!(l1_value(&state, c(3, 1)), CalcValue::number(140.0));
    assert_eq!(l1_value(&state, c(5, 1)), CalcValue::number(20.0));
    let spill = state.spill(c(5, 1)).expect("a spill");
    assert_eq!(spill.area(), GridArea::new(c(5, 1), c(7, 1)).unwrap());
    assert_eq!(spill.value_at(c(7, 1)), &CalcValue::number(60.0));
}

// ---------------------------------------------------------------------------------------------
// L2 (dc-0k9): dynamic dependencies (INDIRECT and OFFSET), spills and `#`, volatile functions and
// random numbers, recalculated through begin_recalc, step and finish on the one graph. Every
// expected value is derived by hand from the literals and the formula texts; the spill rules
// follow Excel 16.0 as measured through COM on 2026-10-23 (cited at each case).

/// A two-sheet workbook (Data, Calc) at `revision` with `cells` and the array formulas `arrays`.
fn l2_root(
    revision: u64,
    sheets: (SheetId, SheetId),
    cells: Vec<(CellKey, CellInput)>,
    arrays: Vec<ArrayFormula>,
) -> Arc<ModelRoot> {
    ModelRoot::with_definitions(
        AuthoredRevision::new(revision),
        vec![
            sheet_entry(sheets.0, 1, "Data"),
            sheet_entry(sheets.1, 2, "Calc"),
        ],
        cells,
        SkinLayout::default(),
        Definitions::new(arrays, vec![], vec![], []),
    )
    .unwrap()
}

/// A dynamic-array formula's file area: its anchor alone, as a formula entered in Excel 365 whose
/// result has not spilled is saved.
fn l2_dynamic(at: CellKey) -> ArrayFormula {
    ArrayFormula::new(GridArea::cell(at), ArrayKind::Dynamic { collapsed: false })
}

/// Records which formulas waited for a cell, and how many outcomes the evaluator reported for
/// each formula.
#[derive(Default)]
struct Waits {
    deferred: Vec<(CellKey, CellKey)>,
    evaluated: BTreeMap<CellKey, usize>,
}

impl ReadObserver for Waits {
    fn observe(&mut self, _: CellKey, _: GridArea, _: u64, _: CellRun<'_>) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn evaluated(&mut self, evaluation: ObservedEvaluation<'_>) {
        *self.evaluated.entry(evaluation.formula()).or_default() += 1;
    }

    fn deferred(&mut self, formula: CellKey, cell: CellKey) {
        self.deferred.push((formula, cell));
    }
}

/// The refusal's owner of a formula cell's not-modelled result.
fn l2_refusal_owner(state: &EngineState, at: CellKey) -> &'static str {
    match state.result(at) {
        Some(FormulaResult::NotModeled {
            refusal: NotModeled::Evaluation(EvalFault::NotModelled { owner, .. }),
            ..
        }) => owner,
        other => panic!("{at:?}: {other:?}"),
    }
}

fn l2_error(code: calc_doc_model::value::WorksheetErrorCode) -> CalcValue {
    CalcValue::error(code)
}

/// INDIRECT is bound against the workbook's sheets and recalculated through the engine. A formula
/// whose INDIRECT reads a formula cell ordered after it waits for that cell, and so does a formula
/// that reads the waiting one; the discarded attempts are not reported as evaluated. The areas
/// INDIRECT read become dynamic prerequisites, so the next recalculation orders each formula after
/// the cells it read and nothing waits.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_0k9__indirect_through_the_engine() {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let (data, calc) = (ids.issue().unwrap(), ids.issue().unwrap());
    let d = |row, column| CellKey::new(data, row, column).unwrap();
    let c = |row, column| CellKey::new(calc, row, column).unwrap();
    let cells = vec![
        (d(1, 1), number(5.0)),
        (d(2, 1), formula("A1*2")),
        (c(1, 2), number(5.0)),
        // Calc!A5 is ordered after Calc!A1, which reads it through INDIRECT: 5*3.
        (c(1, 1), formula("INDIRECT(\"A\"&B1)")),
        // Data!A1:A2 = 5, 10.
        (c(2, 1), formula("SUM(INDIRECT(\"Data!A1:A2\"))")),
        // Reads Calc!A1, which waits: 15+1.
        (c(3, 1), formula("A1+1")),
        // A sheet the workbook does not hold: #REF!.
        (c(4, 1), formula("INDIRECT(\"Nowhere!A1\")")),
        (c(5, 1), formula("Data!A1*3")),
    ];
    let root = l2_root(0, (data, calc), cells, vec![]);
    let mut state = EngineState::new(&root);
    let mut waits = Waits::default();
    recalc(&mut state, &root, &mut waits);
    let n = CalcValue::number;
    assert_eq!(l1_value(&state, c(1, 1)), n(15.0));
    assert_eq!(l1_value(&state, c(2, 1)), n(15.0));
    assert_eq!(l1_value(&state, c(3, 1)), n(16.0));
    assert_eq!(
        l1_value(&state, c(4, 1)),
        l2_error(calc_doc_model::value::WorksheetErrorCode::Ref)
    );
    assert_eq!(
        waits.deferred,
        [(c(1, 1), c(5, 1)), (c(3, 1), c(1, 1))],
        "Calc!A1 waits for Calc!A5, Calc!A3 for Calc!A1"
    );
    assert!(
        waits.evaluated.values().all(|count| *count == 1),
        "{:?}",
        waits.evaluated
    );
    let dynamic = |state: &EngineState, at| {
        state
            .node(state.node_at(at).unwrap())
            .unwrap()
            .dynamic_prerequisites()
            .to_vec()
    };
    assert_eq!(dynamic(&state, c(1, 1)), [Prerequisite::Cell(c(5, 1))]);
    assert_eq!(
        dynamic(&state, c(2, 1)),
        [Prerequisite::Area(GridArea::new(d(1, 1), d(2, 1)).unwrap())]
    );
    assert!(state.is_volatile(state.node_at(c(1, 1)).unwrap()));
    assert!(!state.is_volatile(state.node_at(c(5, 1)).unwrap()));

    // Data!A1 = 7: Calc!A5 = 21, Data!A2 = 14, so Calc!A1 = 21, Calc!A2 = 21, Calc!A3 = 22, and
    // the dynamic prerequisites order every formula after what it reads.
    let root = edit(&mut state, &root, vec![set(d(1, 1), number(7.0))]);
    // Calc!A2 names no Data cell in its text: its dynamic prerequisite Data!A1:A2 makes it stale.
    assert!(state.stale(&root).unwrap().contains(&c(2, 1)));
    let mut waits = Waits::default();
    recalc(&mut state, &root, &mut waits);
    assert_eq!(l1_value(&state, c(1, 1)), n(21.0));
    assert_eq!(l1_value(&state, c(2, 1)), n(21.0));
    assert_eq!(l1_value(&state, c(3, 1)), n(22.0));
    assert_eq!(waits.deferred, []);
}

/// OFFSET into a formula cell the schedule orders later: the formula waits for it.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_0k9__offset_into_a_later_formula_cell() {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let (data, calc) = (ids.issue().unwrap(), ids.issue().unwrap());
    let c = |row, column| CellKey::new(calc, row, column).unwrap();
    let cells = vec![
        // Calc!A1 reads Calc!B1 and, through OFFSET, Calc!A5 = 2*3.
        (c(1, 1), formula("OFFSET(B1,4,-1)")),
        (c(2, 1), formula("1+1")),
        (c(5, 1), formula("2*3")),
    ];
    let state = l1_calculated(cells, vec![], (data, calc));
    assert_eq!(l1_value(&state, c(2, 1)), CalcValue::number(2.0));
    assert_eq!(l1_value(&state, c(5, 1)), CalcValue::number(6.0));
    assert_eq!(l1_value(&state, c(1, 1)), CalcValue::number(6.0));
}

/// Two formulas that read each other only through INDIRECT are a circular reference the order
/// cannot see: the one that would wait on itself is refused (circular references are L3's), the
/// other reads it as a cell the reader cannot supply, and the recalculation completes.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_0k9__circular_through_indirect() {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let (data, calc) = (ids.issue().unwrap(), ids.issue().unwrap());
    let c = |row, column| CellKey::new(calc, row, column).unwrap();
    let cells = vec![
        (c(1, 1), formula("INDIRECT(\"A2\")")),
        (c(2, 1), formula("INDIRECT(\"A1\")+1")),
        (c(3, 1), formula("1+2")),
    ];
    let state = l1_calculated(cells, vec![], (data, calc));
    assert_eq!(l2_refusal_owner(&state, c(2, 1)), "dc-m3u");
    assert!(matches!(
        state.result(c(1, 1)),
        Some(FormulaResult::NotModeled {
            refusal: NotModeled::Evaluation(EvalFault::MissingInput { .. }),
            ..
        })
    ));
    assert_eq!(l1_value(&state, c(3, 1)), CalcValue::number(3.0));
}

/// A dynamic-array formula spills from its anchor; a formula that read a cell of the spill before
/// the anchor was evaluated is calculated again in a further wave; `#` reads the spilled range;
/// a value in the way gives #SPILL! and clearing it lets the result spill; a spill that grows
/// reaches the formulas reading the cells it gained; and clearing the anchor reaches the formulas
/// that read its old area.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_0k9__spills() {
    use calc_doc_model::value::WorksheetErrorCode;
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let (data, calc) = (ids.issue().unwrap(), ids.issue().unwrap());
    let d = |row, column| CellKey::new(data, row, column).unwrap();
    let c = |row, column| CellKey::new(calc, row, column).unwrap();
    let cells = vec![
        (d(1, 1), number(10.0)),
        (d(2, 1), number(20.0)),
        (d(3, 1), number(30.0)),
        (d(4, 1), number(40.0)),
        (d(1, 2), number(3.0)),
        // Spills Calc!B5:B7 = 20, 40, 60.
        (c(5, 2), formula("OFFSET(Data!A1,0,0,Data!B1,1)*2")),
        // Evaluated before Calc!B5 (row 1), so in a second wave: 60+1.
        (c(1, 1), formula("B7+1")),
        // Calc!B8 is blank until the spill grows: 0+5.
        (c(2, 1), formula("B8+5")),
        (c(1, 4), formula("SUM(B5#)")),
        (c(1, 5), formula("ROWS(B5#)")),
        // Blocked by Calc!F2: #SPILL!.
        (c(1, 6), formula("{1;2;3}")),
        (c(2, 6), number(9.0)),
    ];
    let arrays = vec![l2_dynamic(c(5, 2)), l2_dynamic(c(1, 6))];
    let root = l2_root(0, (data, calc), cells, arrays);
    let mut state = EngineState::new(&root);
    recalc(&mut state, &root, &mut Unobserved);
    let n = CalcValue::number;
    assert_eq!(l1_value(&state, c(5, 2)), n(20.0));
    let spill = state.spill(c(5, 2)).expect("Calc!B5 spills");
    assert_eq!(spill.area(), GridArea::new(c(5, 2), c(7, 2)).unwrap());
    assert_eq!(spill.value_at(c(6, 2)), &n(40.0));
    assert_eq!(
        state.spill_covering(c(7, 2)).map(|s| s.anchor()),
        Some(c(5, 2))
    );
    assert_eq!(l1_value(&state, c(1, 1)), n(61.0));
    assert_eq!(l1_value(&state, c(2, 1)), n(5.0));
    assert_eq!(l1_value(&state, c(1, 4)), n(120.0));
    assert_eq!(l1_value(&state, c(1, 5)), n(3.0));
    assert_eq!(
        l1_value(&state, c(1, 6)),
        l2_error(WorksheetErrorCode::Spill)
    );
    assert_eq!(state.spill(c(1, 6)), None);

    // Data!B1 = 4 and Calc!F2 cleared: Calc!B5:B8 = 20, 40, 60, 80, so Calc!A2 = 85, the sum is
    // 200 and the rows 4; Calc!F1 spills 1, 2, 3.
    let root = edit(
        &mut state,
        &root,
        vec![set(d(1, 2), number(4.0)), clear(GridArea::cell(c(2, 6)))],
    );
    let delta = recalc(&mut state, &root, &mut Unobserved);
    assert_eq!(l1_value(&state, c(2, 1)), n(85.0));
    assert_eq!(l1_value(&state, c(1, 4)), n(200.0));
    assert_eq!(l1_value(&state, c(1, 5)), n(4.0));
    assert_eq!(l1_value(&state, c(1, 6)), n(1.0));
    assert_eq!(
        state.spill(c(1, 6)).map(|s| s.value_at(c(3, 6)).clone()),
        Some(n(3.0))
    );
    assert_eq!(
        delta
            .spill_changes()
            .iter()
            .map(|change| (change.anchor, change.after.map(|a| a.cell_count())))
            .collect::<Vec<_>>(),
        [(c(1, 6), Some(3)), (c(5, 2), Some(4))]
    );

    // A value entered into the spill blocks it, and its readers see the blank cells: Calc!B5 is
    // #SPILL!, Calc!A1 = 0+1, Calc!A2 = 0+5.
    let root = edit(&mut state, &root, vec![set(c(7, 2), number(1.0))]);
    recalc(&mut state, &root, &mut Unobserved);
    assert_eq!(
        l1_value(&state, c(5, 2)),
        l2_error(WorksheetErrorCode::Spill)
    );
    assert_eq!(l1_value(&state, c(1, 1)), n(2.0));
    assert_eq!(l1_value(&state, c(2, 1)), n(5.0));

    // Removing the blocking value and then the anchor itself: the formulas that read its old area
    // are stale at once and read blank cells after the recalculation, and `#` on an empty cell is
    // #REF!.
    let root = edit(&mut state, &root, vec![clear(GridArea::cell(c(7, 2)))]);
    recalc(&mut state, &root, &mut Unobserved);
    assert_eq!(l1_value(&state, c(1, 1)), n(61.0));
    let root = edit(&mut state, &root, vec![clear(GridArea::cell(c(5, 2)))]);
    let stale = state.stale(&root).unwrap();
    assert!(
        stale.contains(&c(1, 1)) && stale.contains(&c(2, 1)),
        "{stale:?}"
    );
    recalc(&mut state, &root, &mut Unobserved);
    assert_eq!(state.spill(c(5, 2)), None);
    assert_eq!(l1_value(&state, c(1, 1)), n(1.0));
    assert_eq!(l1_value(&state, c(2, 1)), n(5.0));
    assert_eq!(l1_value(&state, c(1, 4)), l2_error(WorksheetErrorCode::Ref));
}

/// Two spills that would overlap: the one whose file area already holds the cells fills them and
/// the other shows #SPILL!, whichever is evaluated first. Excel 16.0 (COM, 2026-10-23): with
/// `={1,2,3}` entered in E2 and then `={7;8;9}` in F1, F1 is #SPILL!; with `={7;8;9}` in J1 and
/// then `={1,2,3}` in I2, I2 is #SPILL!; a full rebuild keeps both. Excel saves a blocked anchor's
/// area as the anchor alone.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_0k9__spill_precedence() {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let (data, calc) = (ids.issue().unwrap(), ids.issue().unwrap());
    let c = |row, column| CellKey::new(calc, row, column).unwrap();
    let dynamic = |first, last| {
        ArrayFormula::new(
            GridArea::new(first, last).unwrap(),
            ArrayKind::Dynamic { collapsed: false },
        )
    };
    let cells = vec![
        (c(2, 5), formula("{1,2,3}")),
        (c(1, 6), formula("{7;8;9}")),
        (c(1, 10), formula("{7;8;9}")),
        (c(2, 9), formula("{1,2,3}")),
    ];
    let arrays = vec![
        dynamic(c(2, 5), c(2, 7)),
        dynamic(c(1, 6), c(1, 6)),
        dynamic(c(1, 10), c(3, 10)),
        dynamic(c(2, 9), c(2, 9)),
    ];
    let root = l2_root(0, (data, calc), cells, arrays);
    let mut state = EngineState::new(&root);
    recalc(&mut state, &root, &mut Unobserved);
    let spill_error = l2_error(calc_doc_model::value::WorksheetErrorCode::Spill);
    assert_eq!(l1_value(&state, c(2, 5)), CalcValue::number(1.0));
    assert_eq!(l1_value(&state, c(1, 6)), spill_error);
    assert_eq!(l1_value(&state, c(1, 10)), CalcValue::number(7.0));
    assert_eq!(l1_value(&state, c(2, 9)), spill_error);
    assert_eq!(
        state.spill_covering(c(2, 6)).map(|s| s.anchor()),
        Some(c(2, 5))
    );
}

/// A legacy (Ctrl+Shift+Enter) array formula fills its fixed area: one row repeats down the rows
/// and a cell past the result shows #N/A (Excel 16.0, COM, 2026-10-23: `={1,2}` over L1:N2 shows
/// 2 in M2 and #N/A in N2). A formula reading the area reads the filled values.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_0k9__legacy_array_fill() {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let (data, calc) = (ids.issue().unwrap(), ids.issue().unwrap());
    let c = |row, column| CellKey::new(calc, row, column).unwrap();
    let cells = vec![
        (c(1, 12), formula("{1,2}")),
        // 1+2+1+2.
        (c(4, 1), formula("SUM(L1:M2)")),
    ];
    let arrays = vec![ArrayFormula::new(
        GridArea::new(c(1, 12), c(2, 14)).unwrap(),
        ArrayKind::Legacy,
    )];
    let root = l2_root(0, (data, calc), cells, arrays);
    let mut state = EngineState::new(&root);
    recalc(&mut state, &root, &mut Unobserved);
    let fill = state.spill(c(1, 12)).expect("a fill");
    assert!(fill.is_fixed());
    assert_eq!(fill.value_at(c(2, 13)), &CalcValue::number(2.0));
    assert_eq!(
        fill.value_at(c(2, 14)),
        &l2_error(calc_doc_model::value::WorksheetErrorCode::NA)
    );
    assert_eq!(l1_value(&state, c(4, 1)), CalcValue::number(6.0));
}

/// A formula that calls a volatile function, and every formula that reads it, is calculated by
/// every recalculation; another formula only when what it reads changes. RAND draws from the
/// state's seed, the recalculation's tick and the formula's cell: a number in [0, 1) that a new
/// recalculation draws again, and that two states with one seed draw alike.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_0k9__volatile_and_random() {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let (data, calc) = (ids.issue().unwrap(), ids.issue().unwrap());
    let d = |row, column| CellKey::new(data, row, column).unwrap();
    let c = |row, column| CellKey::new(calc, row, column).unwrap();
    let cells = vec![
        (c(1, 1), formula("RAND()")),
        (c(2, 1), formula("A1*0+1")),
        (c(3, 1), formula("1+1")),
        (c(4, 1), formula("RANDBETWEEN(1,6)")),
    ];
    let root = l2_root(0, (data, calc), cells, vec![]);
    let mut state = EngineState::new(&root);
    state.set_random_seed(42);
    recalc(&mut state, &root, &mut Unobserved);
    let first = l1_value(&state, c(1, 1)).as_number().expect("a number");
    assert!((0.0..1.0).contains(&first));
    let die = l1_value(&state, c(4, 1)).as_number().expect("a number");
    assert!(die.fract() == 0.0 && (1.0..=6.0).contains(&die));

    let mut twin = EngineState::new(&root);
    twin.set_random_seed(42);
    recalc(&mut twin, &root, &mut Unobserved);
    assert_eq!(l1_value(&twin, c(1, 1)), CalcValue::number(first));

    // An edit no formula reads: the volatile formulas and their reader are calculated again.
    let root = edit(&mut state, &root, vec![set(d(9, 9), number(1.0))]);
    let delta = recalc(&mut state, &root, &mut Unobserved);
    assert_eq!(
        delta_cells(&delta),
        [(c(1, 1), true), (c(2, 1), true), (c(4, 1), true)]
    );
    assert_ne!(l1_value(&state, c(1, 1)), CalcValue::number(first));
}

/// SUBTOTAL and AGGREGATE read through the engine leave out the cells holding a SUBTOTAL or
/// AGGREGATE formula and the rows the snapshot holds hidden: function numbers 1 to 11 keep rows
/// hidden by hand, 101 to 111 leave them out, and every number leaves out rows a filter hides.
/// Data!A1:A4 = 1, 2, 3, 4 with row 2 hidden by hand and row 3 by a filter, and Data!A5 a
/// SUBTOTAL: SUBTOTAL(9) is 1+2+4, SUBTOTAL(109) is 1+4, and AGGREGATE(9,3) (hidden rows,
/// errors and nested subtotals left out) is 1+4.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_0k9__subtotal_and_aggregate() {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let (data, calc) = (ids.issue().unwrap(), ids.issue().unwrap());
    let d = |row, column| CellKey::new(data, row, column).unwrap();
    let c = |row, column| CellKey::new(calc, row, column).unwrap();
    let cells = vec![
        (d(1, 1), number(1.0)),
        (d(2, 1), number(2.0)),
        (d(3, 1), number(3.0)),
        (d(4, 1), number(4.0)),
        (d(5, 1), formula("SUBTOTAL(9,A1:A4)")),
        (c(1, 1), formula("SUBTOTAL(9,Data!A1:A5)")),
        (c(2, 1), formula("SUBTOTAL(109,Data!A1:A5)")),
        (c(3, 1), formula("_xlfn.AGGREGATE(9,3,Data!A1:A5)")),
    ];
    let root = ModelRoot::with_definitions(
        AuthoredRevision::new(0),
        vec![sheet_entry(data, 1, "Data"), sheet_entry(calc, 2, "Calc")],
        cells,
        SkinLayout::default(),
        Definitions::new(vec![], vec![], vec![], []).with_hidden_rows([
            (data, 2, calc_doc_model::RowHidden::Manual),
            (data, 3, calc_doc_model::RowHidden::Filtered),
        ]),
    )
    .unwrap();
    let mut state = EngineState::new(&root);
    recalc(&mut state, &root, &mut Unobserved);
    let n = CalcValue::number;
    assert_eq!(l1_value(&state, d(5, 1)), n(7.0));
    assert_eq!(l1_value(&state, c(1, 1)), n(7.0));
    assert_eq!(l1_value(&state, c(2, 1)), n(5.0));
    assert_eq!(l1_value(&state, c(3, 1)), n(5.0));
}

// ---------------------------------------------------------------------------------------------
// L3 (dc-m3u): tables, 3-D references, cycles and iteration, structural edits and data tables
// through the engine.

/// A root of the sheets `names` (keys 1, 2, ...), `cells` and `definitions`, at revision 0.
fn l3_root(
    sheets: &[(SheetId, &str)],
    cells: Vec<(CellKey, CellInput)>,
    definitions: Definitions,
) -> Arc<ModelRoot> {
    let entries = sheets
        .iter()
        .zip(1u32..)
        .map(|((id, name), key)| sheet_entry(*id, key, name))
        .collect();
    ModelRoot::with_definitions(
        AuthoredRevision::new(0),
        entries,
        cells,
        SkinLayout::default(),
        definitions,
    )
    .unwrap()
}

/// A 3-D reference and structured references through the engine: the 3-D sum reads each sheet
/// of its span and an edit of the middle sheet recalculates it; a structured reference reads the
/// table's column and follows an edit of its data. Values hand-derived from the cells.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
fn dc_m3u__tables_and_3d_through_the_engine() {
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    let (f, ja, fe, ma, tab) = (
        ids.issue().unwrap(),
        ids.issue().unwrap(),
        ids.issue().unwrap(),
        ids.issue().unwrap(),
        ids.issue().unwrap(),
    );
    let at = |sheet, row, column| CellKey::new(sheet, row, column).unwrap();
    let cells = vec![
        (at(ja, 1, 1), number(1.0)),
        (at(fe, 1, 1), number(2.0)),
        (at(ma, 1, 1), number(3.0)),
        (at(tab, 1, 1), text("Name")),
        (at(tab, 1, 2), text("Qty")),
        (at(tab, 2, 1), text("a")),
        (at(tab, 2, 2), number(2.0)),
        (at(tab, 3, 1), text("b")),
        (at(tab, 3, 2), number(3.0)),
        (at(f, 1, 1), formula("SUM(Ja:Ma!A1)")),
        (at(f, 2, 1), formula("SUM(Sales[Qty])")),
        (at(f, 3, 1), formula("ROWS(Sales)")),
        (at(tab, 2, 3), formula("Sales[[#This Row],[Qty]]*10")),
        (at(tab, 3, 3), formula("Sales[[#This Row],[Qty]]*10")),
    ];
    let table = calc_doc_model::Table {
        name: "Sales".to_owned(),
        display_name: "Sales".to_owned(),
        part: "xl/tables/table1.xml".to_owned(),
        sheet: tab,
        area: area(tab, 1, 1, 3, 3),
        header_rows: 1,
        totals_rows: 0,
        columns: vec!["Name".to_owned(), "Qty".to_owned(), "Ten".to_owned()],
    };
    let root = l3_root(
        &[(f, "F"), (ja, "Ja"), (fe, "Fe"), (ma, "Ma"), (tab, "Tab")],
        cells,
        Definitions::new(vec![], vec![], vec![table], []),
    );
    let mut state = EngineState::new(&root);
    recalc(&mut state, &root, &mut Unobserved);
    let n = CalcValue::number;
    assert_eq!(l1_value(&state, at(f, 1, 1)), n(6.0));
    assert_eq!(l1_value(&state, at(f, 2, 1)), n(5.0));
    assert_eq!(l1_value(&state, at(f, 3, 1)), n(2.0));
    assert_eq!(l1_value(&state, at(tab, 2, 3)), n(20.0));
    assert_eq!(l1_value(&state, at(tab, 3, 3)), n(30.0));
    // The middle sheet of the span, and a cell of the table's column.
    let root = edit(
        &mut state,
        &root,
        vec![set(at(fe, 1, 1), number(20.0)), set(at(tab, 3, 2), number(7.0))],
    );
    let delta = recalc(&mut state, &root, &mut Unobserved);
    // ROWS(Sales) and both rows of the calculated column read areas holding Tab!B3 (the data
    // rows, the Qty column), so each is calculated again.
    assert_eq!(
        delta_cells(&delta),
        [
            (at(f, 1, 1), true),
            (at(f, 2, 1), true),
            (at(f, 3, 1), true),
            (at(tab, 2, 3), true),
            (at(tab, 3, 3), true)
        ]
    );
    assert_eq!(l1_value(&state, at(f, 1, 1)), n(24.0));
    assert_eq!(l1_value(&state, at(f, 2, 1)), n(9.0));
    assert_eq!(l1_value(&state, at(tab, 3, 3)), n(70.0));
}
