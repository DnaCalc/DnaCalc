//! The host's round trip over a workbook Excel saved with dynamic arrays, legacy arrays, names
//! and a table (calc-doc's `tests/fixtures/cdoc_m1_2/d1.saved.xlsx`).
//!
//! - dc-ndi (D1): open, edit, recalculate, save with the calculated values as cached values, and
//!   the cells that read a removed array's area recalculated.
//! - dc-35p (D2): the session skins drive ([`calc_host::HostSession`]): cell entries, multi-cell
//!   edits, undo and redo, the calculation mode, the selection and subscriptions with their
//!   deltas, the intent log's replay, and the M1 proof: open, edit, undo, recalculate, save and
//!   reopen.

// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

mod contract_support;
#[path = "../../calc-doc/tests/cdoc_m1_1_support/mod.rs"]
mod existing_wire_support;
#[path = "../../calc-engine/tests/fixtures/seven_cell.rs"]
mod seven_cell;
#[path = "../../calc-engine/tests/fixtures/seven_cell_async.rs"]
mod async_provider;

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_scoped_cut_and_cell_shifts_follow_survivors_through_undo_redo() {
    use calc_engine::contract::{CalcEvent, CalcScope};
    let mut book = session(&calc_host::blank_package());
    let calc = book.host().sheets()[0].id();
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
            .accepted()
    );
    for (address, text) in [
        ("Z1", "=1+1"),
        ("Z2", "=2+2"),
        ("Z3", "=3+3"),
        ("AB1", "=99"),
    ] {
        assert!(enter(&mut book, at(calc, address), text).accepted());
    }
    let source = GridArea::new(at(calc, "Z1"), at(calc, "Z3")).unwrap();
    book.set_calculation_budget(WorkBudget::evaluations(1));
    assert!(
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![source]
        })
        .accepted()
    );
    assert!(book.has_pending_calculation());
    book.take_calculation_events();
    assert!(
        book.dispatch(SkinIntent::Copy {
            from: source,
            to: GridArea::cell(at(calc, "AA1")),
            cut: true
        })
        .accepted()
    );
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    assert!(book.dispatch(SkinIntent::Redo).accepted());
    assert!(
        book.dispatch(SkinIntent::Command(
            calc_host::WorkbookCommand::ShiftCells {
                area: GridArea::cell(at(calc, "AA2")),
                down: true,
                insert: true
            }
        ))
        .accepted()
    );
    for _ in 0..16 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    let expected = CalcScope::Range(vec![
        GridArea::new(at(calc, "AA1"), at(calc, "AA4")).unwrap(),
    ]);
    let events = book.take_calculation_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, CalcEvent::Admitted(a) if a.scope == expected))
    );
    assert!(events.iter().any(|event| matches!(event, CalcEvent::Completed(c) if c.scope == expected && c.coverage.required == 3 && c.coverage.calculated == 3)));
    for (address, value) in [("AA1", 2.0), ("AA3", 4.0), ("AA4", 6.0)] {
        assert_eq!(
            calculated(book.host(), at(calc, address)),
            Some(CalcValue::number(value))
        );
    }
    for address in ["Z1", "Z2", "Z3", "AA2", "AB1"] {
        assert!(
            calculated(book.host(), at(calc, address)).is_none(),
            "scope did not widen to {address}"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_cancel_is_terminal_and_advance_creates_no_history() {
    use calc_engine::contract::{CalcEvent, CompletionOutcome};
    let mut book = session(&fixture());
    let calc = sheet(book.host(), "Calc");
    book.take_calculation_events();
    book.set_calculation_budget(WorkBudget::evaluations(1));
    let receipt = book.dispatch(SkinIntent::Edit {
        edits: vec![
            CellEdit::Set {
                at: at(calc, "Z1"),
                input: formula("2+1"),
            },
            CellEdit::Set {
                at: at(calc, "Z2"),
                input: formula("Z1*2"),
            },
            CellEdit::Set {
                at: at(calc, "Z3"),
                input: formula("Z2*2"),
            },
        ],
    });
    assert!(receipt.accepted());
    assert!(book.has_pending_calculation());
    let events = book.take_calculation_events();
    let admission = events
        .iter()
        .find_map(|event| {
            if let CalcEvent::Admitted(receipt) = event {
                Some(receipt)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(admission.source.sequence, receipt.seq);
    let source = admission.source;
    let command = admission.command;
    let history = book.intent_log().len();
    let revision = book.host().revision();
    book.cancel_calculation();
    assert!(!book.has_pending_calculation());
    let terminal = book.take_calculation_events();
    assert_eq!(terminal.iter().filter(|event| matches!(event,CalcEvent::Completed(receipt)
        if receipt.command==command && receipt.outcome==CompletionOutcome::Cancelled && receipt.after_publication.is_none())).count(),1);
    assert_eq!(book.advance_calculation(), HostProgress::Cancelled);
    assert_eq!(book.advance_calculation(), HostProgress::Cancelled);
    assert!(book.take_calculation_events().is_empty());
    assert_eq!(book.intent_log().len(), history);
    assert_eq!(book.host().revision(), revision);
    book.dispatch(SkinIntent::Recalculate);
    for _ in 0..256 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    assert_eq!(
        calculated(book.host(), at(calc, "Z3")),
        Some(CalcValue::number(12.0))
    );
    assert!(
        !book
            .take_calculation_events()
            .iter()
            .any(|event| matches!(event,CalcEvent::Completed(receipt)
        if receipt.command==command && receipt.outcome==CompletionOutcome::Complete))
    );
    assert_ne!(source.sequence, 0);
}

use calc_host::value::{CalcValue, CellKey, CoreValue, GridArea, SheetId};
use calc_host::{
    Axis, AxisChange, AxisEdit, CachedValue, CalcMode, CellEdit, CellInput, CellText, DeltaCause,
    DeltaMirror, EntryRefusal, FiniteNumber, FormulaText, HostCommand, HostDelta, HostOptions,
    HostProgress, HostSession, HostState, IntentError, IntentReceipt, Interest, MemorySource,
    OpenPolicy, PackageRead, ProjectedCell, ProjectedValue, ProjectionRequest, SavePolicy,
    Selection, SkinIntent, SubscriptionId, Unobserved, WorkBudget, dispatch, project, replay, save,
    step,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_14__host_save_preserves_unchanged_xlsb_and_refuses_edits() {
    let bytes = include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/collision.xlsb");
    let mut host = open(bytes);
    let policy = SavePolicy {
        format: calc_host::SaveFormat::Xlsb,
        ..Default::default()
    };
    let mut saved = Vec::new();
    let receipt = save(&host, &policy, &mut saved).unwrap();
    let archive = existing_wire_support::parse_zip(&saved);
    assert!(
        archive
            .entries
            .iter()
            .any(|entry| entry.name == "xl/workbook.bin")
    );
    assert_eq!(receipt.parts_written, 0);
    assert_eq!(receipt.values_written, 0);
    let id = sheet(&host, "Cases");
    edit(
        &mut host,
        vec![CellEdit::Set {
            at: CellKey::new(id, 50, 4).unwrap(),
            input: number(7.0),
        }],
    );
    let mut rejected = Vec::new();
    assert!(matches!(
        save(&host, &policy, &mut rejected),
        Err(calc_host::SaveFailed::Refused(
            calc_host::SaveRefusal::BinaryChangesNotModelled { owner: "dc-snz.14" }
        ))
    ));
    assert!(rejected.is_empty());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_snz_15__project_reading_leaves_host_file_caches_unexecuted() {
    let source =
        include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/disabled-resaved.xlsm");
    let host = open(source);
    let id = host.sheets()[0].id();
    let call = at(id, "B1");
    let before = value(&host, call);
    assert!(matches!(before, ProjectedValue::FileCached(_)));
    let project = host
        .loaded()
        .read_vba_project(&calc_host::VbaReadPolicy::default())
        .unwrap()
        .unwrap();
    assert_eq!(project.code_page, 1252);
    assert_eq!(project.modules.len(), 4);
    assert_eq!(value(&host, call), before);
    let mut saved = Vec::new();
    save(&host, &SavePolicy::default(), &mut saved).unwrap();
    assert_eq!(value(&open(&saved), call), before);
}

/// The D1 fixture, embedded so the same tests run natively and in the browsers.
fn fixture() -> Vec<u8> {
    include_bytes!("../../calc-doc/tests/fixtures/cdoc_m1_2/d1.saved.xlsx").to_vec()
}

fn open(bytes: &[u8]) -> HostState {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    HostState::open(source, &OpenPolicy::default(), HostOptions::default()).expect("it opens")
}

fn sheet(host: &HostState, name: &str) -> SheetId {
    host.sheets()
        .iter()
        .find(|sheet| sheet.name().text().to_string_lossy() == name)
        .expect("the sheet")
        .id()
}

fn at_(sheet: SheetId, address: &str) -> CellKey {
    at(sheet, address)
}

fn at(sheet: SheetId, address: &str) -> CellKey {
    let split = address.find(|c: char| c.is_ascii_digit()).unwrap();
    let (letters, digits) = address.split_at(split);
    let column = letters
        .bytes()
        .fold(0u32, |n, b| n * 26 + u32::from(b - b'A' + 1));
    CellKey::new(sheet, digits.parse().unwrap(), column).unwrap()
}

fn value(host: &HostState, at: CellKey) -> ProjectedValue {
    project(host, &ProjectionRequest { cells: vec![at] })
        .unwrap()
        .cells()[0]
        .value()
        .clone()
}

fn calculated(host: &HostState, at: CellKey) -> Option<CalcValue> {
    match value(host, at) {
        ProjectedValue::Calculated { value, .. } => Some(value),
        _ => None,
    }
}

fn edit(host: &mut HostState, edits: Vec<CellEdit>) {
    dispatch(
        host,
        HostCommand {
            base: host.revision(),
            edits,
        },
    )
    .expect("the edit commits");
}

fn recalculate(host: &mut HostState) {
    step(host, WorkBudget::UNLIMITED, &mut Unobserved);
}

fn number(value: f64) -> CellInput {
    CellInput::Number(FiniteNumber::new(value).unwrap())
}

fn formula(text: &str) -> CellInput {
    CellInput::Formula(
        FormulaText::new(calc_host::value::ExcelText::from_utf16_code_units(
            text.encode_utf16().collect(),
        ))
        .unwrap(),
    )
}

fn part_text(package: &[u8], name: &str) -> String {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(package.to_vec()));
    let document = calc_doc::open(source, &OpenPolicy::default()).expect("the saved package opens");
    String::from_utf8(document.package().read_part(name).expect("the part")).unwrap()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d1_round_trip__a_workbook_with_arrays_opens_calculates_and_saves_its_values() {
    let mut host = open(&fixture());
    let calc = sheet(&host, "Calc");
    recalculate(&mut host);
    // The workbook calculates, its arrays included.
    assert_eq!(
        calculated(&host, at(calc, "U1")),
        Some(CalcValue::number(2.0))
    );
    assert_eq!(
        calculated(&host, at(calc, "U3")),
        Some(CalcValue::number(21.0))
    );
    // SEQUENCE(3,2) at A1 spills 1 to 6 over A1:B3 (F3 and L2): B2 is 4.
    assert_eq!(
        calculated(&host, at(calc, "A1")),
        Some(CalcValue::number(1.0))
    );
    assert_eq!(
        calculated(&host, at(calc, "B2")),
        Some(CalcValue::number(4.0))
    );
    // Edit an input, recalculate, save.
    edit(
        &mut host,
        vec![CellEdit::Set {
            at: at(calc, "U2"),
            input: number(8.0),
        }],
    );
    recalculate(&mut host);
    assert_eq!(
        calculated(&host, at(calc, "U3")),
        Some(CalcValue::number(24.0))
    );
    let mut saved = Vec::new();
    let receipt = save(&host, &SavePolicy::default(), &mut saved).expect("saved");
    assert!(receipt.values_written > 0, "{receipt:?}");
    let xml = part_text(&saved, "xl/worksheets/sheet2.xml");
    // Calculated cells carry the engine's values, a text with t="str"; an array's anchor and the
    // cells its spill fills carry theirs too.
    assert!(
        xml.contains(r#"<c r="U3"><f>U2*3</f><v>24</v></c>"#),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<c r="U1"><f>Data!A1+1</f><v>2</v></c>"#),
        "{xml}"
    );
    assert!(
        xml.contains(
            r#"<c r="A1" cm="1"><f t="array" ref="A1:B3">_xlfn.SEQUENCE(3,2)</f><v>1</v></c>"#
        ),
        "{xml}"
    );
    assert!(xml.contains(r#"<c r="B2"><v>4</v></c>"#), "{xml}");
    // Every array is calculated and saved with its values, so no full calculation is asked for.
    assert!(!part_text(&saved, "xl/workbook.xml").contains("fullCalcOnLoad"));
    let reopened = open(&saved);
    assert!(matches!(
        value(&reopened, at(sheet(&reopened, "Calc"), "U3")),
        ProjectedValue::FileCached(_)
    ));
    if let Ok(dir) = std::env::var("DNACALC_D1_SAVE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join("d1_host_round_trip.xlsx"),
            &saved,
        )
        .unwrap();
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d1_round_trip__readers_of_a_removed_array_recalculate() {
    let mut host = open(&fixture());
    let calc = sheet(&host, "Calc");
    // Z1 reads B2, a cell the array at A1:B3 (SEQUENCE(3,2)) fills: 4+1.
    edit(
        &mut host,
        vec![CellEdit::Set {
            at: at(calc, "Z1"),
            input: formula("B2+1"),
        }],
    );
    recalculate(&mut host);
    assert_eq!(
        calculated(&host, at(calc, "Z1")),
        Some(CalcValue::number(5.0))
    );
    // Replacing the anchor removes the array: B2 is empty from then on, and Z1 recalculates.
    edit(
        &mut host,
        vec![CellEdit::Set {
            at: at(calc, "A1"),
            input: number(9.0),
        }],
    );
    recalculate(&mut host);
    assert_eq!(
        calculated(&host, at(calc, "Z1")),
        Some(CalcValue::number(1.0))
    );
}

/// Opens `bytes` into a session.
fn session(bytes: &[u8]) -> HostSession {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes.to_vec()));
    HostSession::open(source, &OpenPolicy::default(), HostOptions::default()).expect("it opens")
}

/// The deltas a subscription received, in order.
type Received = Rc<RefCell<Vec<HostDelta>>>;

fn subscribe(session: &mut HostSession, interest: Interest) -> (SubscriptionId, Received) {
    let received: Received = Rc::default();
    let sink = Rc::clone(&received);
    let id = session
        .subscribe(
            interest,
            Box::new(move |delta: &HostDelta| sink.borrow_mut().push(delta.clone())),
        )
        .expect("subscribed");
    (id, received)
}

/// Every delta received so far applied to a mirror, which must hold exactly what a fresh
/// projection of every cell of `region` shows, and nothing outside it: the deltas carried every
/// change. `region` covers every cell of the interest that is not empty.
fn assert_mirror_current(session: &HostSession, region: GridArea, received: &Received) {
    let mut mirror = DeltaMirror::default();
    for delta in received.borrow().iter() {
        mirror.apply(delta).expect("the deltas follow one another");
    }
    let held: BTreeMap<CellKey, ProjectedCell> = mirror
        .cells()
        .map(|cell| (cell.at(), cell.clone()))
        .collect();
    assert_eq!(held, fresh_cells(session.host(), region));
}

/// The value a cell projects in the session.
fn session_value(session: &HostSession, at: CellKey) -> ProjectedValue {
    value(session.host(), at)
}

fn session_calculated(session: &HostSession, at: CellKey) -> Option<CalcValue> {
    calculated(session.host(), at)
}

fn enter(session: &mut HostSession, at: CellKey, text: &str) -> IntentReceipt {
    let receipt = session.dispatch(SkinIntent::EnterCell {
        at,
        text: text.to_owned(),
    });
    assert!(receipt.accepted(), "{text}: {receipt:?}");
    receipt
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_22__typed_workbook_commands_keep_history_and_replay() {
    use calc_host::{ConditionalChange, SheetChange, WorkbookCommand as C};
    let bytes = calc_host::blank_package();
    let mut book = session(&bytes);
    let sheet = book.host().sheets()[0].id();
    let at = CellKey::new(sheet, 1, 1).unwrap();
    let apply = |book: &mut HostSession, command| {
        let receipt = book.dispatch(SkinIntent::Command(command));
        assert!(receipt.accepted(), "{:?}", receipt.error);
        receipt
    };
    assert!(enter(&mut book, at, "42").accepted());
    apply(
        &mut book,
        C::Sheet {
            sheet: None,
            change: SheetChange::Add {
                name: "Second".into(),
            },
        },
    );
    let allocated = book.host().sheets()[1].key();
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    apply(
        &mut book,
        C::Sheet {
            sheet: None,
            change: SheetChange::Add {
                name: "Third".into(),
            },
        },
    );
    assert_ne!(
        book.host().sheets()[1].key(),
        allocated,
        "undo must not recycle a sheet key"
    );
    apply(
        &mut book,
        C::AxisFormat {
            sheet,
            rows: true,
            first: 1,
            count: 2,
            size_bits: Some(30f64.to_bits()),
            hidden: None,
        },
    );
    assert_eq!(
        book.host()
            .model()
            .definitions()
            .axis_layouts()
            .get(sheet, Axis::Rows, 2)
            .size(),
        Some(30.0)
    );
    apply(
        &mut book,
        C::FreezePanes {
            sheet,
            rows: 2,
            columns: 1,
        },
    );
    assert_eq!(
        book.host().model().definitions().frozen_panes(sheet),
        calc_host::FrozenPanes::new(2, 1)
    );
    let revision = book.host().revision().get();
    apply(
        &mut book,
        C::Conditional {
            sheet,
            revision,
            index: None,
            change: ConditionalChange::Add {
                kind: "colorScale".into(),
                applies: "A1:A2".into(),
                color: "#FF0000".into(),
            },
        },
    );
    assert_eq!(
        book.host()
            .model()
            .definitions()
            .conditional_rules(sheet)
            .len(),
        1
    );
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    assert!(
        book.host()
            .model()
            .definitions()
            .conditional_rules(sheet)
            .is_empty()
    );
    assert!(book.dispatch(SkinIntent::Redo).accepted());
    let before = (book.host().revision(), book.history_depth());
    let refused = book.dispatch(SkinIntent::Command(C::FitContents {
        sheet,
        rows: true,
        revision: before.0.get(),
        sizes: vec![(1, 25f64.to_bits()), (2, f64::NAN.to_bits())],
    }));
    assert!(!refused.accepted());
    assert_eq!((book.host().revision(), book.history_depth()), before);
    assert_eq!(
        book.host()
            .model()
            .definitions()
            .axis_layouts()
            .get(sheet, Axis::Rows, 1)
            .size(),
        Some(30.0)
    );
    let mut twin = session(&bytes);
    let replayed = replay(book.intent_log(), &mut twin);
    assert!(replayed.mismatches.is_empty(), "{replayed:?}");
    assert_eq!(book.host().sheets(), twin.host().sheets());
    assert_eq!(
        book.host().model().definitions().conditional_rules(sheet),
        twin.host().model().definitions().conditional_rules(sheet)
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_22__paste_and_replace_are_atomic_revision_bound_host_commands() {
    use calc_host::search::{SearchOptions, SearchScope};
    use calc_host::{PasteMode, PasteSource, WorkbookCommand as C, prepare_paste};
    let bytes = calc_host::blank_package();
    let mut book = session(&bytes);
    let sheet = book.host().sheets()[0].id();
    let a1 = CellKey::new(sheet, 1, 1).unwrap();
    let a2 = CellKey::new(sheet, 2, 1).unwrap();
    let source = || PasteSource::External(vec![vec!["4".into()], vec!["=A1+1".into()]]);
    let stale = prepare_paste(book.host(), source(), GridArea::cell(a1), PasteMode::All).unwrap();
    let mut other = session(&bytes);
    assert_eq!(other.host().revision(), book.host().revision());
    assert!(
        !other
            .dispatch(SkinIntent::Command(C::Paste(stale.clone())))
            .accepted()
    );
    assert_eq!(other.history_depth(), (0, 0));
    assert_eq!(other.host().model().cell(a1), None);
    let mut refused_twin = session(&bytes);
    assert!(
        replay(other.intent_log(), &mut refused_twin)
            .mismatches
            .is_empty()
    );
    assert_eq!(refused_twin.host().model().cell(a1), None);
    assert!(replay(other.intent_log(), &mut book).mismatches.is_empty());
    assert_eq!(book.host().model().cell(a1), None);
    assert!(enter(&mut book, a1, "2").accepted());
    let before = (book.host().revision(), book.history_depth());
    assert!(
        !book
            .dispatch(SkinIntent::Command(C::Paste(stale)))
            .accepted()
    );
    assert_eq!((book.host().revision(), book.history_depth()), before);
    let plan = prepare_paste(book.host(), source(), GridArea::cell(a1), PasteMode::All).unwrap();
    assert_eq!(plan.target(), GridArea::new(a1, a2).unwrap());
    assert_eq!(plan.replaces(), 1);
    assert!(
        book.dispatch(SkinIntent::Command(C::Paste(plan)))
            .accepted()
    );
    assert_eq!(session_calculated(&book, a2), Some(CalcValue::number(5.0)));
    let options = SearchOptions {
        sheet,
        areas: vec![GridArea::new(a1, a2).unwrap()],
        text: "A1".into(),
        case_sensitive: false,
        whole: false,
        wildcards: false,
        formulas: true,
        scope: SearchScope::Sheet,
    };
    let revision = book.host().revision().get();
    let replace = C::Replace {
        options,
        revision,
        only: None,
        replacement: "A1*2".into(),
    };
    assert!(
        book.dispatch(SkinIntent::Command(replace.clone()))
            .accepted()
    );
    assert_eq!(session_calculated(&book, a2), Some(CalcValue::number(9.0)));
    let before = (book.host().revision(), book.history_depth());
    assert!(!book.dispatch(SkinIntent::Command(replace)).accepted());
    assert_eq!((book.host().revision(), book.history_depth()), before);
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(session_calculated(&book, a2), Some(CalcValue::number(5.0)));
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(book.host().model().cell(a1), Some(&number(2.0)));
    assert_eq!(book.host().model().cell(a2), None);
    let mut twin = session(&bytes);
    assert!(replay(book.intent_log(), &mut twin).mismatches.is_empty());
    assert_eq!(book.host().model().cell(a1), twin.host().model().cell(a1));
}

/// The cells a delta holds, as (cell, value).
fn delta_values(delta: &HostDelta) -> Vec<(CellKey, ProjectedValue)> {
    delta
        .cells
        .iter()
        .map(|cell| (cell.at(), cell.value().clone()))
        .collect()
}

fn is_calculated(value: &ProjectedValue, expected: f64) -> bool {
    matches!(value, ProjectedValue::Calculated { value, .. } if *value == CalcValue::number(expected))
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_session__edits_undo_redo_and_deltas() {
    let mut session = session(&fixture());
    let calc = sheet(session.host(), "Calc");
    let interest = Interest::Sheet(calc);
    let (id, received) = subscribe(&mut session, interest.clone());
    {
        let first = &received.borrow()[0];
        assert_eq!((first.subscription, first.seq), (id, 1));
        assert_eq!(first.cause, DeltaCause::Subscribed);
        assert!(first.reset);
        // As opened, U3 shows the package's cache.
        assert!(
            delta_values(first)
                .iter()
                .any(|(at, value)| *at == at_(calc, "U3")
                    && matches!(value, ProjectedValue::FileCached(_)))
        );
    }

    // The first recalculation: U3 is now calculated.
    let receipt = session.dispatch(SkinIntent::Recalculate);
    assert!(matches!(receipt.recalc, Some(HostProgress::Published(_))));
    assert_eq!(receipt.deltas, 1);
    assert_eq!(
        session_calculated(&session, at_(calc, "U3")),
        Some(CalcValue::number(21.0))
    );

    // A value entry: one transaction, then the automatic recalculation; two deltas.
    let before = received.borrow().len();
    let receipt = enter(&mut session, at_(calc, "U2"), "8");
    assert!(receipt.committed.is_some());
    assert_eq!(receipt.deltas, 2);
    {
        let deltas = received.borrow();
        let committed = &deltas[before];
        assert!(matches!(committed.cause, DeltaCause::Committed { .. }));
        let values = delta_values(committed);
        assert!(values.contains(&(
            at_(calc, "U2"),
            ProjectedValue::Literal(CalcValue::number(8.0))
        )));
        assert!(
            values
                .iter()
                .any(|(at, value)| *at == at_(calc, "U3")
                    && matches!(value, ProjectedValue::Stale(_)))
        );
        let recalculated = &deltas[before + 1];
        assert!(matches!(
            recalculated.cause,
            DeltaCause::Recalculated { .. }
        ));
        assert!(
            delta_values(recalculated)
                .iter()
                .any(|(at, value)| *at == at_(calc, "U3") && is_calculated(value, 24.0))
        );
    }

    // A formula entry.
    enter(&mut session, at_(calc, "Z1"), "=U3+1");
    assert_eq!(
        session_calculated(&session, at_(calc, "Z1")),
        Some(CalcValue::number(25.0))
    );

    // A multi-cell edit is one step.
    let receipt = session.dispatch(SkinIntent::Edit {
        edits: vec![
            CellEdit::Set {
                at: at_(calc, "U2"),
                input: number(10.0),
            },
            CellEdit::Set {
                at: at_(calc, "Z2"),
                input: formula("Z1*2"),
            },
        ],
    });
    assert!(receipt.accepted(), "{receipt:?}");
    assert_eq!(
        session_calculated(&session, at_(calc, "Z2")),
        Some(CalcValue::number(62.0))
    );
    assert_eq!(session.history_depth(), (3, 0));

    // Undo takes the whole multi-cell edit back and recalculates.
    let before = received.borrow().len();
    let receipt = session.dispatch(SkinIntent::Undo);
    assert!(receipt.accepted(), "{receipt:?}");
    assert!(matches!(receipt.recalc, Some(HostProgress::Published(_))));
    assert_eq!(
        session_value(&session, at_(calc, "Z2")),
        ProjectedValue::Empty
    );
    assert_eq!(
        session_calculated(&session, at_(calc, "Z1")),
        Some(CalcValue::number(25.0))
    );
    assert!(
        received.borrow()[before]
            .cells
            .iter()
            .any(|cell| cell.at() == at_(calc, "Z2")
                && cell.input().is_none()
                && cell.value() == &ProjectedValue::Empty),
        "the cell the undo emptied is delivered as empty"
    );

    // Undo the formula entry, then redo it.
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(
        session_value(&session, at_(calc, "Z1")),
        ProjectedValue::Empty
    );
    assert_eq!(session.history_depth(), (1, 2));
    assert!(session.dispatch(SkinIntent::Redo).accepted());
    assert_eq!(
        session_calculated(&session, at_(calc, "Z1")),
        Some(CalcValue::number(25.0))
    );
    assert_eq!(session.history_depth(), (2, 1));

    // A new edit leaves nothing to redo.
    enter(&mut session, at_(calc, "U2"), "1");
    let receipt = session.dispatch(SkinIntent::Redo);
    assert_eq!(receipt.error, Some(IntentError::NothingToRedo));
    assert_eq!(
        session_calculated(&session, at_(calc, "Z1")),
        Some(CalcValue::number(4.0))
    );

    // Every delta followed the last, and together they hold what the sheet projects now.
    let used = GridArea::new(at_(calc, "A1"), at_(calc, "AF20")).unwrap();
    assert_eq!(interest, Interest::Sheet(calc));
    assert_mirror_current(&session, used, &received);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_session__clear_and_undo_restore_formulas_and_values() {
    let mut session = session(&fixture());
    let calc = sheet(session.host(), "Calc");
    let area = GridArea::new(at_(calc, "U1"), at_(calc, "U3")).unwrap();
    let (_, received) = subscribe(&mut session, Interest::Area(area));
    session.dispatch(SkinIntent::Recalculate);
    let receipt = session.dispatch(SkinIntent::Edit {
        edits: vec![CellEdit::Clear { area }],
    });
    assert!(receipt.accepted(), "{receipt:?}");
    for cell in ["U1", "U2", "U3"] {
        assert_eq!(
            session_value(&session, at_(calc, cell)),
            ProjectedValue::Empty
        );
    }
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    let page = project(
        session.host(),
        &ProjectionRequest {
            cells: vec![at_(calc, "U1"), at_(calc, "U2"), at_(calc, "U3")],
        },
    )
    .unwrap();
    assert_eq!(page.cells()[0].input(), Some(&formula("Data!A1+1")));
    assert_eq!(page.cells()[1].input(), Some(&number(7.0)));
    assert_eq!(page.cells()[2].input(), Some(&formula("U2*3")));
    assert_eq!(
        session_calculated(&session, at_(calc, "U1")),
        Some(CalcValue::number(2.0))
    );
    assert_eq!(
        session_calculated(&session, at_(calc, "U3")),
        Some(CalcValue::number(21.0))
    );
    assert_mirror_current(&session, area, &received);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_session__undo_restores_an_array_formula() {
    let mut session = session(&fixture());
    let calc = sheet(session.host(), "Calc");
    // Replacing the anchor of the dynamic array A1:B3 removes the array; undo authors it again.
    enter(&mut session, at_(calc, "A1"), "5");
    assert_eq!(
        session_value(&session, at_(calc, "B2")),
        ProjectedValue::Empty
    );
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    let page = project(
        session.host(),
        &ProjectionRequest {
            cells: vec![at_(calc, "A1")],
        },
    )
    .unwrap();
    assert_eq!(
        page.cells()[0].input(),
        Some(&formula("_xlfn.SEQUENCE(3,2)"))
    );
    let mut saved = Vec::new();
    session
        .save(&SavePolicy::default(), &mut saved)
        .expect("saved");
    let xml = part_text(&saved, "xl/worksheets/sheet2.xml");
    assert!(
        xml.contains(r#"<f t="array" ref="A1:B3">_xlfn.SEQUENCE(3,2)</f>"#),
        "{xml}"
    );
    // Clearing the legacy array L1:L3 whole, then undoing, restores it with its values.
    let legacy = GridArea::new(at_(calc, "L1"), at_(calc, "L3")).unwrap();
    assert!(
        session
            .dispatch(SkinIntent::Edit {
                edits: vec![CellEdit::Clear { area: legacy }],
            })
            .accepted()
    );
    assert_eq!(
        session_value(&session, at_(calc, "L2")),
        ProjectedValue::Empty
    );
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(
        session_calculated(&session, at_(calc, "L2")),
        Some(CalcValue::number(20.0))
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_session__an_entered_array_formula_spills() {
    let mut session = session(&fixture());
    let calc = sheet(session.host(), "Calc");
    // Entered into one cell, a formula that can return an array is a dynamic array, as Excel
    // stores it, and its result spills.
    enter(&mut session, at_(calc, "Z5"), "=Data!A1:A3*2");
    assert_eq!(
        session_calculated(&session, at_(calc, "Z5")),
        Some(CalcValue::number(2.0))
    );
    assert_eq!(
        session_calculated(&session, at_(calc, "Z7")),
        Some(CalcValue::number(6.0))
    );
    let mut saved = Vec::new();
    session
        .save(&SavePolicy::default(), &mut saved)
        .expect("saved");
    let xml = part_text(&saved, "xl/worksheets/sheet2.xml");
    assert!(
        xml.contains(r#"<c r="Z5" cm="1"><f t="array" ref="Z5:Z7">Data!A1:A3*2</f><v>2</v></c>"#),
        "{xml}"
    );
    assert!(xml.contains(r#"<c r="Z6"><v>4</v></c>"#), "{xml}");
    // Undo takes the array and its spill away; redo brings them back.
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(
        session_value(&session, at_(calc, "Z6")),
        ProjectedValue::Empty
    );
    assert!(session.dispatch(SkinIntent::Redo).accepted());
    assert_eq!(
        session_calculated(&session, at_(calc, "Z6")),
        Some(CalcValue::number(4.0))
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_session__refused_entries_change_nothing() {
    let mut session = session(&fixture());
    let calc = sheet(session.host(), "Calc");
    let (_, received) = subscribe(&mut session, Interest::Sheet(calc));
    let revision = session.host().revision();
    let receipt = session.dispatch(SkinIntent::EnterCell {
        at: at_(calc, "U2"),
        text: "12%".to_owned(),
    });
    assert_eq!(
        receipt.error,
        Some(IntentError::Entry(EntryRefusal::NumberShaped {
            text: "12%".to_owned()
        }))
    );
    // A formula the parser refuses.
    let receipt = session.dispatch(SkinIntent::EnterCell {
        at: at_(calc, "U2"),
        text: "=1+".to_owned(),
    });
    assert!(matches!(receipt.error, Some(IntentError::Command(_))));
    assert_eq!(session.host().revision(), revision);
    assert_eq!(
        received.borrow().len(),
        1,
        "only the subscription's first delta"
    );
    assert_eq!(session.history_depth(), (0, 0));
    assert_eq!(session.intent_log().len(), 2);
    assert!(session.intent_log().iter().all(|record| !record.accepted));
}

/// dc-10d9: a full recalculation (Excel's `CalculateFull`) calculates every formula again, under a
/// new tick, with the plans and the graph kept, and gives the values the first one gave.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_10d9__a_full_recalculation_calculates_every_formula_again() {
    let mut host = open(&fixture());
    let first = match step(&mut host, WorkBudget::UNLIMITED, &mut Unobserved) {
        HostProgress::Published(delta) => delta,
        other => panic!("the first recalculation publishes: {other:?}"),
    };
    assert!(matches!(
        step(&mut host, WorkBudget::UNLIMITED, &mut Unobserved),
        HostProgress::Current
    ));
    calc_host::calculate_full(&mut host);
    let full = match step(&mut host, WorkBudget::UNLIMITED, &mut Unobserved) {
        HostProgress::Published(delta) => delta,
        other => panic!("the full recalculation publishes: {other:?}"),
    };
    assert!(full.tick() > first.tick());
    assert_eq!(full.changes().len(), first.changes().len());
    let calc = sheet(&host, "Calc");
    assert_eq!(
        calculated(&host, at_(calc, "U3")),
        Some(CalcValue::number(21.0))
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_session__manual_mode_recalculates_when_asked() {
    let mut session = session(&fixture());
    let calc = sheet(session.host(), "Calc");
    assert!(
        session
            .dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
            .accepted()
    );
    let receipt = enter(&mut session, at_(calc, "U2"), "8");
    assert_eq!(receipt.recalc, None);
    assert!(matches!(
        session_value(&session, at_(calc, "U3")),
        ProjectedValue::Stale(_)
    ));
    let receipt = session.dispatch(SkinIntent::Recalculate);
    assert!(matches!(receipt.recalc, Some(HostProgress::Published(_))));
    assert_eq!(
        session_calculated(&session, at_(calc, "U3")),
        Some(CalcValue::number(24.0))
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_session__a_selection_interest_follows_the_selection() {
    let mut session = session(&fixture());
    let calc = sheet(session.host(), "Calc");
    let (_, received) = subscribe(&mut session, Interest::Selection);
    assert!(received.borrow()[0].cells.is_empty(), "nothing is selected");
    let receipt = session.dispatch(SkinIntent::Select(Selection::cell(at_(calc, "U3"))));
    assert!(receipt.accepted());
    {
        let deltas = received.borrow();
        let moved = deltas.last().unwrap();
        assert_eq!(moved.cause, DeltaCause::InterestMoved);
        assert!(moved.reset);
        assert_eq!(moved.cells.len(), 1);
        assert_eq!(moved.cells[0].at(), at_(calc, "U3"));
    }
    // An edit outside the selection reaches it only through what the selected cell shows.
    let before = received.borrow().len();
    enter(&mut session, at_(calc, "U2"), "8");
    let deltas = received.borrow();
    assert!(deltas.len() > before);
    for delta in &deltas[before..] {
        assert!(delta.cells.iter().all(|cell| cell.at() == at_(calc, "U3")));
    }
    assert!(is_calculated(deltas.last().unwrap().cells[0].value(), 24.0));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_session__the_intent_log_replays() {
    let mut recorded = session(&fixture());
    let calc = sheet(recorded.host(), "Calc");
    enter(&mut recorded, at_(calc, "U2"), "8");
    enter(&mut recorded, at_(calc, "Z1"), "=U3*2");
    recorded.dispatch(SkinIntent::EnterCell {
        at: at_(calc, "Z2"),
        text: "=1+".to_owned(),
    });
    assert!(recorded.dispatch(SkinIntent::Undo).accepted());
    assert!(
        recorded
            .dispatch(SkinIntent::Select(Selection::cell(at_(calc, "U3"))))
            .accepted()
    );
    let log = recorded.intent_log().to_vec();
    let mut replayed = session(&fixture());
    let outcome = replay(&log, &mut replayed);
    assert_eq!(outcome.dispatched, 5);
    assert_eq!(outcome.accepted, 4);
    assert!(outcome.mismatches.is_empty(), "{outcome:?}");
    let cells: Vec<CellKey> = ["U2", "U3", "Z1", "Z2"]
        .iter()
        .map(|address| at_(calc, address))
        .collect();
    let request = ProjectionRequest { cells };
    let values = |host: &HostState| -> Vec<(Option<CellInput>, String)> {
        project(host, &request)
            .unwrap()
            .cells()
            .iter()
            .map(|cell| {
                (
                    cell.input().cloned(),
                    contract_support::comparable(cell.value()),
                )
            })
            .collect()
    };
    assert_eq!(values(replayed.host()), values(recorded.host()));
    assert_eq!(replayed.selection(), recorded.selection());
}

/// The M1 proof: a workbook Excel saved is opened, edited (values on two sheets and formulas),
/// one edit is undone, the workbook recalculates, and the save is reopened: every formula cell
/// the session calculated, and every cell an array's result fills, is saved with its value and
/// reads back as that file cache, and the reopened workbook recalculates to the same values. The
/// arrays are saved as Excel saves them: a dynamic array's spill with its range and the spilled
/// values alone, a legacy array's area filled, and, once a value blocks the spill, #SPILL! with
/// its value metadata. With `DNACALC_D2_SAVE_DIR` set, both saved packages are also written there
/// for a capture in Excel.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_m1_proof__open_edit_undo_recalculate_save_reopen() {
    let mut session = session(&fixture());
    let data = sheet(session.host(), "Data");
    let calc = sheet(session.host(), "Calc");
    enter(&mut session, at_(data, "A1"), "10");
    enter(&mut session, at_(calc, "U2"), "8");
    enter(&mut session, at_(calc, "Z1"), "=U1+U3");
    enter(&mut session, at_(calc, "Z2"), "=Z1*100");
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(
        session_value(&session, at_(calc, "Z2")),
        ProjectedValue::Empty
    );
    assert_eq!(
        session_calculated(&session, at_(calc, "U1")),
        Some(CalcValue::number(11.0))
    );
    assert_eq!(
        session_calculated(&session, at_(calc, "Z1")),
        Some(CalcValue::number(35.0))
    );
    // F1 is the dynamic array Data!A1:A3*2 over F1:F3, L1 the legacy array Data!A1:A3*10.
    assert_eq!(
        session_calculated(&session, at_(calc, "F2")),
        Some(CalcValue::number(4.0))
    );

    let saved = save_and_check_reopen(&session, &[data, calc], "d2_m1_proof.xlsx");
    let xml = part_text(&saved, "xl/worksheets/sheet2.xml");
    for expected in [
        r#"<c r="F1" cm="1"><f t="array" ref="F1:F3">Data!A1:A3*2</f><v>20</v></c>"#,
        r#"<c r="F2"><v>4</v></c>"#,
        r#"<c r="F3"><v>6</v></c>"#,
        r#"<c r="L1"><f t="array" ref="L1:L3">Data!A1:A3*10</f><v>100</v></c>"#,
        r#"<c r="L2"><v>20</v></c>"#,
        r#"<c r="L3"><v>30</v></c>"#,
    ] {
        assert!(xml.contains(expected), "{expected} in {xml}");
    }

    // A value in F1's spill range blocks it: F1 shows #SPILL!, saved with its rich error.
    enter(&mut session, at_(calc, "F3"), "x");
    assert_eq!(
        session_calculated(&session, at_(calc, "F1")),
        Some(CalcValue::error(
            calc_host::value::WorksheetErrorCode::Spill
        ))
    );
    let blocked = save_and_check_reopen(&session, &[data, calc], "d2_m1_proof_spill.xlsx");
    let xml = part_text(&blocked, "xl/worksheets/sheet2.xml");
    // The cell's attributes in any order: from the start tag that holds r="F1" to its end.
    let f1_at = xml.find(r#" r="F1""#).expect("F1");
    let f1 = &xml[xml[..f1_at].rfind("<c ").expect("F1's start tag")..];
    let f1 = &f1[..f1.find("</c>").expect("F1 ends")];
    for expected in [
        r#"t="e""#,
        r#"cm="1""#,
        r#"vm="3""#,
        r#"<f t="array" ref="F1">Data!A1:A3*2</f><v>#VALUE!</v>"#,
    ] {
        assert!(f1.contains(expected), "{expected} in {f1}");
    }
    let metadata = part_text(&blocked, "xl/metadata.xml");
    // Four value-metadata entries (three while SEQUENCE and FILTER were refused; they are
    // calculated now, F3 and L2, so H1's #SPILL! and J1's #CALC! are written as rich errors too).
    // Excel 16.0.20430 opened this package through COM on 2026-10-23 and showed #SPILL! at F1
    // and H1 and #CALC! at J1.
    assert!(
        metadata.contains(r#"<valueMetadata count="4">"#),
        "{metadata}"
    );
}

/// Saves the session, reopens the saved package and checks every cell of the sheets' first 12
/// rows and 26 columns: the inputs are the session's, each value the session calculated reads
/// back as that file cache, and the reopened workbook recalculates to the same values. Returns
/// the saved package, also written to `DNACALC_D2_SAVE_DIR` as `name` when that is set.
fn save_and_check_reopen(session: &HostSession, sheets: &[SheetId], name: &str) -> Vec<u8> {
    let mut saved = Vec::new();
    let receipt = session
        .save(&SavePolicy::default(), &mut saved)
        .expect("saved");
    assert!(receipt.values_written > 0, "{receipt:?}");
    // The fixture's arrays (SEQUENCE, SORT and FILTER) are calculated and saved with their
    // values, so no full calculation is asked for.
    assert!(!part_text(&saved, "xl/workbook.xml").contains("fullCalcOnLoad"));
    let mut reopened = open(&saved);
    let mut cells = Vec::new();
    let mut reopened_cells = Vec::new();
    for &sheet_id in sheets {
        let name = session
            .host()
            .sheets()
            .iter()
            .find(|entry| entry.id() == sheet_id)
            .unwrap()
            .name()
            .text()
            .to_string_lossy();
        let reopened_sheet = sheet(&reopened, &name);
        for row in 1..=12 {
            for column in 1..=26 {
                cells.push(CellKey::new(sheet_id, row, column).unwrap());
                reopened_cells.push(CellKey::new(reopened_sheet, row, column).unwrap());
            }
        }
    }
    let edited = project(session.host(), &ProjectionRequest { cells }).unwrap();
    let reopened_request = ProjectionRequest {
        cells: reopened_cells,
    };
    let as_opened = project(&reopened, &reopened_request).unwrap();
    recalculate(&mut reopened);
    let recalculated = project(&reopened, &reopened_request).unwrap();
    let mut compared = 0;
    for ((edited, opened), recalculated) in edited
        .cells()
        .iter()
        .zip(as_opened.cells())
        .zip(recalculated.cells())
    {
        assert_eq!(edited.input(), opened.input(), "{:?}", edited.at());
        let ProjectedValue::Calculated { value, .. } = edited.value() else {
            continue;
        };
        compared += 1;
        match opened.value() {
            ProjectedValue::FileCached(cache) => assert_eq!(
                format!("{:?}", cache.value()),
                format!("{:?}", cached_of(value)),
                "{:?}",
                edited.at()
            ),
            other => panic!("{:?} reopened as {other:?}", edited.at()),
        }
        match recalculated.value() {
            ProjectedValue::Calculated { value: again, .. } => {
                assert_eq!(again, value, "{:?}", edited.at())
            }
            other => panic!("{:?} recalculated as {other:?}", edited.at()),
        }
    }
    assert!(compared >= 9, "{compared} calculated cells compared");
    if let Ok(dir) = std::env::var("DNACALC_D2_SAVE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join(name), &saved).unwrap();
    }
    saved
}

/// The cached value a calculated scalar is saved as.
fn cached_of(value: &CalcValue) -> CachedValue {
    match value.core() {
        CoreValue::Number(number) => CachedValue::Number(FiniteNumber::new(*number).unwrap()),
        CoreValue::Text(text) => CachedValue::Text(CellText::new(text.clone()).unwrap()),
        CoreValue::Logical(logical) => CachedValue::Logical(*logical),
        CoreValue::Error(code) => CachedValue::Error(*code),
        other => panic!("not a scalar: {other:?}"),
    }
}

/// The M1 round trip on a workbook from outside the repository, run when `DNACALC_ROUND_TRIP`
/// names `<input.xlsx>|<output.xlsx>|<step>;<step>;...`, each step `<sheet>!<cell>=<entry>` (the
/// text typed into the cell, a formula with its `=`), `<sheet>!insert rows <first> <count>` (or
/// `delete`, or `columns`: rows or columns inserted or deleted, dc-jhn) or `undo` or `redo`: open
/// a session, dispatch
/// the steps (each recalculated, as in automatic mode), save. D1's form
/// `DNACALC_D1_ROUND_TRIP=<input.xlsx>|<sheet>|<cell>|<number>|<output.xlsx>` is the one step
/// that sets the cell to the number. The saved file is then captured in Excel (`cargo xtask
/// capture`) and compared (`cargo xtask compare`) outside this test. Without either variable it
/// prints that it did not run.
///
/// Native-only: it reads and writes the workbook files an environment variable names.
#[test]
fn d1_round_trip__external_workbook() {
    let spec = match (
        std::env::var("DNACALC_ROUND_TRIP"),
        std::env::var("DNACALC_D1_ROUND_TRIP"),
    ) {
        (Ok(spec), _) => spec,
        (_, Ok(d1)) => {
            let parts: Vec<&str> = d1.split('|').collect();
            let [input, sheet_name, cell, number_text, output] = parts[..] else {
                panic!("DNACALC_D1_ROUND_TRIP needs five |-separated fields");
            };
            format!("{input}|{output}|{sheet_name}!{cell}={number_text}")
        }
        _ => {
            println!(
                "ACCEPT-NOT-RUN: d1_round_trip__external_workbook: DNACALC_ROUND_TRIP is not set"
            );
            return;
        }
    };
    let parts: Vec<&str> = spec.splitn(3, '|').collect();
    let [input, output, steps] = parts[..] else {
        panic!("DNACALC_ROUND_TRIP needs <input>|<output>|<steps>");
    };
    let mut session = session(&std::fs::read(input).expect("the input workbook"));
    for step_text in steps.split(';').filter(|text| !text.is_empty()) {
        // dc-jhn: `<sheet>!insert rows <first> <count>`, and likewise `delete` and `columns`.
        let structural = step_text.rsplit_once('!').and_then(|(sheet_name, rest)| {
            let words: Vec<&str> = rest.split(' ').collect();
            let [change, axis, first, count] = words[..] else {
                return None;
            };
            let change = match change {
                "insert" => AxisChange::Insert,
                "delete" => AxisChange::Delete,
                _ => return None,
            };
            let axis = match axis {
                "rows" => Axis::Rows,
                "columns" => Axis::Columns,
                _ => return None,
            };
            let sheet = sheet(session.host(), sheet_name);
            AxisEdit::new(
                sheet,
                axis,
                change,
                first.parse().ok()?,
                count.parse().ok()?,
            )
            .ok()
        });
        let intent = match step_text {
            "undo" => SkinIntent::Undo,
            "redo" => SkinIntent::Redo,
            _ if structural.is_some() => SkinIntent::EditAxis(structural.expect("an axis edit")),
            _ => {
                let (target, text) = step_text.split_once('=').expect("<sheet>!<cell>=<entry>");
                let (sheet_name, cell) = target.rsplit_once('!').expect("<sheet>!<cell>");
                SkinIntent::EnterCell {
                    at: at_(sheet(session.host(), sheet_name), cell),
                    text: text.to_owned(),
                }
            }
        };
        let receipt = session.dispatch(intent);
        assert!(receipt.accepted(), "{step_text}: {:?}", receipt.error);
        let published = match &receipt.recalc {
            Some(HostProgress::Published(delta)) => delta.changes().len(),
            _ => 0,
        };
        println!(
            "ACCEPT-NOTE: d1_round_trip__external_workbook: {step_text}: revision {}, {published} results published",
            receipt.revision.get()
        );
    }
    let mut saved = Vec::new();
    let receipt = session
        .save(&SavePolicy::default(), &mut saved)
        .expect("saved");
    std::fs::write(output, &saved).expect("written");
    println!("ACCEPT-NOTE: d1_round_trip__external_workbook: {receipt:?}");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_session__an_interest_moves_and_an_unknown_subscription_is_refused() {
    let mut session = session(&fixture());
    let calc = sheet(session.host(), "Calc");
    let data = sheet(session.host(), "Data");
    let (id, received) = subscribe(
        &mut session,
        Interest::Area(GridArea::new(at_(calc, "U1"), at_(calc, "U3")).unwrap()),
    );
    assert_eq!(received.borrow()[0].cells.len(), 3);
    // Moving the interest delivers the new area's cells as a reset.
    let receipt = session.dispatch(SkinIntent::SetInterest {
        subscription: id,
        interest: Interest::Area(GridArea::new(at_(data, "A1"), at_(data, "A5")).unwrap()),
    });
    assert!(receipt.accepted());
    {
        let deltas = received.borrow();
        let moved = deltas.last().unwrap();
        assert_eq!(moved.cause, DeltaCause::InterestMoved);
        assert!(moved.reset);
        assert_eq!(moved.cells.len(), 5);
    }
    // An edit of the old area no longer reaches the subscription.
    let before = received.borrow().len();
    enter(&mut session, at_(calc, "U2"), "8");
    assert_eq!(received.borrow().len(), before);
    // A subscription the session no longer holds.
    assert!(session.unsubscribe(id));
    let receipt = session.dispatch(SkinIntent::SetInterest {
        subscription: id,
        interest: Interest::Selection,
    });
    assert_eq!(receipt.error, Some(IntentError::UnknownSubscription(id)));
}

/// dc-rif (D4): DNA Calc's New Workbook opens the blank package as any other: one empty sheet with
/// no used area; entries set cells and recalculate; the used area bounds every non-empty cell,
/// spills included; and the edited workbook saves and reopens with its calculated values cached.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_rif__a_new_workbook_is_edited_saved_and_reopened() {
    let mut session = session(&calc_host::blank_package());
    let sheets = session.host().sheets().to_vec();
    assert_eq!(sheets.len(), 1);
    assert_eq!(sheets[0].name().text().to_string_lossy(), "Sheet1");
    let sheet1 = sheets[0].id();
    assert_eq!(session.host().used_area(sheet1), None);

    for (address, text) in [
        ("B2", "2"),
        ("B3", "3.5"),
        ("C2", "rate"),
        ("D4", "=SUM(B2:B3)*2"),
        ("F2", "={1;2;3}"),
    ] {
        let receipt = session.dispatch(SkinIntent::EnterCell {
            at: at(sheet1, address),
            text: text.to_owned(),
        });
        assert!(receipt.accepted(), "{address}: {:?}", receipt.error);
    }
    assert_eq!(
        calculated(session.host(), at(sheet1, "D4")),
        Some(CalcValue::number(11.0))
    );
    // B2 to D4 hold the inputs; F2's spill fills F2:F4.
    assert_eq!(
        session.host().used_area(sheet1),
        Some(GridArea::new(at(sheet1, "B2"), at(sheet1, "F4")).unwrap())
    );
    // A subscription to the area is given the cells the spill fills.
    let (_, received) = subscribe(
        &mut session,
        Interest::Area(GridArea::new(at(sheet1, "A1"), at(sheet1, "H10")).unwrap()),
    );
    let mut mirror = DeltaMirror::default();
    for delta in received.borrow().iter() {
        mirror.apply(delta).expect("the deltas follow one another");
    }
    match mirror.cell(at(sheet1, "F4")).map(ProjectedCell::value) {
        Some(ProjectedValue::Calculated { value, .. }) => {
            assert_eq!(value, &CalcValue::number(3.0))
        }
        other => panic!("F4 is filled by F2's spill: {other:?}"),
    }

    let mut saved = Vec::new();
    session
        .save(&SavePolicy::default(), &mut saved)
        .expect("the new workbook saves");
    let reopened = open(&saved);
    let sheet1 = sheet(&reopened, "Sheet1");
    assert_eq!(
        reopened.used_area(sheet1).map(|area| area.first()),
        Some(at(sheet1, "B2"))
    );
    match value(&reopened, at(sheet1, "D4")) {
        ProjectedValue::FileCached(cache) => {
            assert_eq!(
                cache.value(),
                &CachedValue::Number(FiniteNumber::new(11.0).unwrap())
            )
        }
        other => panic!("D4 reopens with the value it saved: {other:?}"),
    }
    match value(&reopened, at(sheet1, "C2")) {
        ProjectedValue::Literal(text) => assert_eq!(
            text,
            CalcValue::text(calc_host::value::ExcelText::from_utf16_code_units(
                "rate".encode_utf16().collect()
            ))
        ),
        other => panic!("C2 reopens as its text: {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// dc-35p review: the reviewer's differential, made a test. A random run of entries, clears, undo,
// redo, calculation-mode switches, row and column insertions and deletions (dc-jhn), recalculations
// and saves over Calc!AA1:AF10, moving within AA1:AF30 (empty in the
// fixture) checks after every intent that
// 1. a mirror fed by an area subscription's deltas, and one fed by a sheet subscription's, hold
//    exactly what a fresh projection of every cell of the area shows;
// 2. undo and redo put back exactly the inputs a recorded stack holds;
// 3. every value projected as calculated is what a fresh session with the same inputs calculates;
// 4. every few steps, each value a save writes as a file cache is the session's calculated value.

/// Every cell of an area.
fn area_cells(area: GridArea) -> Vec<CellKey> {
    let (first, last) = (area.first(), area.last());
    (first.row()..=last.row())
        .flat_map(|row| {
            (first.column()..=last.column())
                .map(move |column| CellKey::new(area.sheet(), row, column).unwrap())
        })
        .collect()
}

/// A fresh projection of every cell of `area`, the cells that are not empty.
fn fresh_cells(host: &HostState, area: GridArea) -> BTreeMap<CellKey, ProjectedCell> {
    project(
        host,
        &ProjectionRequest {
            cells: area_cells(area),
        },
    )
    .unwrap()
    .cells()
    .iter()
    .filter(|cell| cell.input().is_some() || cell.value() != &ProjectedValue::Empty)
    .map(|cell| (cell.at(), cell.clone()))
    .collect()
}

/// The inputs of `area`.
fn area_inputs(host: &HostState, area: GridArea) -> BTreeMap<CellKey, CellInput> {
    fresh_cells(host, area)
        .into_iter()
        .filter_map(|(at, cell)| cell.input().cloned().map(|input| (at, input)))
        .collect()
}

/// A subscription's deltas applied to a mirror, its cells within `area`.
fn mirrored(received: &Received, area: GridArea) -> BTreeMap<CellKey, ProjectedCell> {
    let mut mirror = DeltaMirror::default();
    for delta in received.borrow().iter() {
        mirror.apply(delta).expect("the deltas follow one another");
    }
    mirror
        .cells()
        .filter(|cell| area.contains(cell.at()))
        .map(|cell| (cell.at(), cell.clone()))
        .collect()
}

/// The text that enters `input` again.
fn entry_text(input: &CellInput) -> String {
    match input {
        CellInput::Number(number) => format!("{}", number.get()),
        CellInput::Text(text) => format!("'{}", text.text().to_string_lossy()),
        CellInput::Logical(true) => "TRUE".to_owned(),
        CellInput::Logical(false) => "FALSE".to_owned(),
        CellInput::Error(code) => match code {
            calc_host::value::WorksheetErrorCode::NA => "#N/A".to_owned(),
            other => panic!("no error but #N/A is entered here: {other:?}"),
        },
        CellInput::Formula(formula) => format!("={}", formula.text().to_string_lossy()),
    }
}

fn a1(at: CellKey) -> String {
    let mut column = at.column();
    let mut letters = String::new();
    while column > 0 {
        letters.insert(0, (b'A' + ((column - 1) % 26) as u8) as char);
        column = (column - 1) / 26;
    }
    format!("{letters}{}", at.row())
}

/// A fixed-seed xorshift64 generator, so a failure repeats.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

const FORMULAS: [&str; 10] = [
    "={c}+1",
    "=SUM({c}:{d})",
    "=INDIRECT(\"{c}\")",
    "=SEQUENCE({n})",
    "={c}:{d}*2",
    "=Data!A1*{n}",
    "=IF({c}>2,{d},{n})",
    "=OFFSET({c},1,0)",
    "=TRANSPOSE({c}:{d})",
    "=COUNT({c}:{d})",
];

/// One random run; the problems it found.
fn differential_run(
    seed: u64,
    steps: usize,
    contested: &mut usize,
    structural: &mut usize,
    saved_moved: &mut usize,
) -> Vec<String> {
    let structural_before = *structural;
    let mut rng = Rng(seed);
    let mut session = session(&fixture());
    let calc = sheet(session.host(), "Calc");
    // The cells of AA1:AF10 move within AA1:AF30 when rows or columns there are inserted.
    let region = GridArea::new(at_(calc, "AA1"), at_(calc, "AF30")).unwrap();
    let (_, by_area) = subscribe(&mut session, Interest::Area(region));
    let (_, by_sheet) = subscribe(&mut session, Interest::Sheet(calc));
    let mut undo: Vec<BTreeMap<CellKey, CellInput>> = Vec::new();
    let mut redo: Vec<BTreeMap<CellKey, CellInput>> = Vec::new();
    let mut problems = Vec::new();
    let cell = |rng: &mut Rng| {
        CellKey::new(calc, 1 + rng.below(10) as u32, 27 + rng.below(6) as u32).unwrap()
    };
    for step in 0..steps {
        let before = area_inputs(session.host(), region);
        let roll = rng.below(100);
        let intent = if roll < 60 {
            let at = cell(&mut rng);
            let text = match rng.below(10) {
                0..=2 => format!("{}", rng.below(9)),
                3 => String::new(),
                4 => "abc".to_owned(),
                _ => {
                    let template = FORMULAS[rng.below(FORMULAS.len() as u64) as usize];
                    let c = cell(&mut rng);
                    let d =
                        CellKey::new(calc, c.row() + 1 + rng.below(3) as u32, c.column()).unwrap();
                    template
                        .replace("{c}", &a1(c))
                        .replace("{d}", &a1(d))
                        .replace("{n}", &format!("{}", 1 + rng.below(4)))
                }
            };
            SkinIntent::EnterCell { at, text }
        } else if roll < 63 {
            let first = cell(&mut rng);
            let last = CellKey::new(
                calc,
                (first.row() + rng.below(3) as u32).min(10),
                (first.column() + rng.below(2) as u32).min(32),
            )
            .unwrap();
            SkinIntent::Edit {
                edits: vec![CellEdit::Clear {
                    area: GridArea::new(first, last).unwrap(),
                }],
            }
        } else if roll < 78 {
            SkinIntent::Undo
        } else if roll < 86 {
            SkinIntent::Redo
        } else if roll < 91 {
            SkinIntent::SetCalcMode(if rng.below(2) == 0 {
                CalcMode::Manual
            } else {
                CalcMode::Automatic
            })
        } else if roll < 95 {
            // dc-jhn: a row or column of the region inserted or deleted.
            let (axis, first) = if rng.below(2) == 0 {
                (Axis::Rows, 1 + rng.below(10) as u32)
            } else {
                (Axis::Columns, 27 + rng.below(6) as u32)
            };
            let change = if rng.below(2) == 0 {
                AxisChange::Insert
            } else {
                AxisChange::Delete
            };
            SkinIntent::EditAxis(AxisEdit::new(calc, axis, change, first, 1).unwrap())
        } else {
            SkinIntent::Recalculate
        };
        let receipt = session.dispatch(intent.clone());
        let after = area_inputs(session.host(), region);
        let shown = |inputs: &BTreeMap<CellKey, CellInput>| -> Vec<(String, String)> {
            inputs
                .iter()
                .map(|(at, input)| (a1(*at), entry_text(input)))
                .collect()
        };
        if receipt.accepted() {
            match &intent {
                SkinIntent::EnterCell { .. }
                | SkinIntent::Edit { .. }
                | SkinIntent::EditAxis(_) => {
                    if matches!(intent, SkinIntent::EditAxis(_)) {
                        *structural += 1;
                    }
                    undo.push(before.clone());
                    redo.clear();
                }
                SkinIntent::Undo => {
                    let expected = undo.pop().expect("undo had a step");
                    if after != expected {
                        problems.push(format!(
                            "seed {seed} step {step}: undo gave {:?}, not {:?}",
                            shown(&after),
                            shown(&expected)
                        ));
                    }
                    redo.push(before.clone());
                }
                SkinIntent::Redo => {
                    let expected = redo.pop().expect("redo had a step");
                    if after != expected {
                        problems.push(format!(
                            "seed {seed} step {step}: redo gave {:?}, not {:?}",
                            shown(&after),
                            shown(&expected)
                        ));
                    }
                    undo.push(before.clone());
                }
                _ => {}
            }
        }
        // 1. The mirrors.
        let fresh = fresh_cells(session.host(), region);
        for (name, received) in [("area", &by_area), ("sheet", &by_sheet)] {
            let mirror = mirrored(received, region);
            if mirror != fresh {
                let at = mirror
                    .keys()
                    .chain(fresh.keys())
                    .find(|at| mirror.get(at) != fresh.get(at))
                    .copied()
                    .unwrap();
                problems.push(format!(
                    "seed {seed} step {step} ({intent:?}): the {name} mirror holds {} as {:?}, a fresh projection {:?}",
                    a1(at),
                    mirror.get(&at).map(|cell| cell.value().clone()),
                    fresh.get(&at).map(|cell| cell.value().clone())
                ));
            }
        }
        // 3. Every calculated value against a fresh session with the same inputs.
        let calculated: Vec<(CellKey, CalcValue)> = fresh
            .iter()
            .filter_map(|(at, cell)| match cell.value() {
                ProjectedValue::Calculated { value, .. } => Some((*at, value.clone())),
                _ => None,
            })
            .collect();
        if !calculated.is_empty() {
            let mut oracle = self::session(&fixture());
            oracle.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
            for (at, input) in &after {
                let receipt = oracle.dispatch(SkinIntent::EnterCell {
                    at: *at,
                    text: entry_text(input),
                });
                assert!(
                    receipt.accepted(),
                    "the oracle's entry: {:?}",
                    receipt.error
                );
            }
            let fresh_receipt = oracle.dispatch(SkinIntent::Recalculate);
            if let Some(HostProgress::Failed(failure)) = &fresh_receipt.recalc {
                println!(
                    "reference calculation failed for seed {seed} step {step}: {failure:?}; inputs {:?}",
                    shown(&after)
                );
            }
            let expected = fresh_cells(oracle.host(), region);
            // Two dynamic arrays whose results would cover the same cells: the one that filled
            // them first keeps them and the other shows #SPILL! (the engine's rule, as Excel's,
            // spill.rs), so which wins depends on the order the formulas were entered and
            // calculated, which a fresh calculation does not repeat. A step where the session and
            // the fresh calculation disagree on which anchors show #SPILL! is not compared; the
            // run counts it.
            let spilled = |cells: &BTreeMap<CellKey, ProjectedCell>| -> Vec<CellKey> {
                cells
                    .iter()
                    .filter(|(_, cell)| {
                        matches!(cell.value(), ProjectedValue::Calculated { value, .. }
                            if *value == CalcValue::error(calc_host::value::WorksheetErrorCode::Spill))
                    })
                    .map(|(at, _)| *at)
                    .collect()
            };
            if spilled(&fresh) != spilled(&expected) {
                *contested += 1;
                continue;
            }
            for (at, value) in calculated {
                match expected.get(&at).map(|cell| cell.value()) {
                    Some(ProjectedValue::Calculated { value: again, .. }) if *again == value => {}
                    other => problems.push(format!(
                        "seed {seed} step {step} ({intent:?}): {} is calculated as {:?}, a fresh calculation gives {:?}",
                        a1(at),
                        value,
                        other
                    )),
                }
            }
        }
        // 4. A save's caches.
        if step % 10 == 9 {
            let mut saved = Vec::new();
            // This invariant covers the strict current-value policy. Manual's default held
            // cache profile has separate P06 value/metadata/re-entry checks below.
            if let Err(error) = calc_host::prepare_save_with_cache_policy(
                session.host(),
                &SavePolicy::default(),
                calc_host::SaveCachePolicy::CurrentCalculatedOnly,
            )
            .and_then(|plan| plan.write(&mut saved))
            {
                panic!("the save failed: {error}");
            }
            if *structural > structural_before {
                *saved_moved += 1;
            }
            let reopened = open(&saved);
            let reopened_calc = sheet(&reopened, "Calc");
            let reopened_region = GridArea::new(
                CellKey::new(reopened_calc, 1, 27).unwrap(),
                CellKey::new(reopened_calc, 30, 32).unwrap(),
            )
            .unwrap();
            // dc-jhn: the saved package holds the session's inputs where the session holds them,
            // rows and columns inserted or deleted included.
            let reopened_inputs: BTreeMap<CellKey, CellInput> =
                area_inputs(&reopened, reopened_region)
                    .into_iter()
                    .map(|(at, input)| (CellKey::new(calc, at.row(), at.column()).unwrap(), input))
                    .collect();
            if reopened_inputs != after {
                problems.push(format!(
                    "seed {seed} step {step}: the saved package holds {:?}, the session {:?}",
                    shown(&reopened_inputs),
                    shown(&after)
                ));
            }
            for (at, cell) in fresh_cells(&reopened, reopened_region) {
                let ProjectedValue::FileCached(cache) = cell.value() else {
                    continue;
                };
                let here = CellKey::new(calc, at.row(), at.column()).unwrap();
                match fresh.get(&here).map(|cell| cell.value()) {
                    Some(ProjectedValue::Calculated { value, .. })
                        if &cached_of(value) == cache.value() => {}
                    other => problems.push(format!(
                        "seed {seed} step {step}: {} is saved with the cache {:?} while the session shows {:?}",
                        a1(here),
                        cache.value(),
                        other
                    )),
                }
            }
        }
        if problems.len() > 5 {
            break;
        }
    }
    problems
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_review__random_sessions_agree_with_fresh_projections_and_calculations() {
    let mut problems = Vec::new();
    let (mut contested, mut structural, mut saved_moved) = (0, 0, 0);
    for seed in 1..=12u64 {
        problems.extend(differential_run(
            seed * 7919 + 13,
            60,
            &mut contested,
            &mut structural,
            &mut saved_moved,
        ));
    }
    assert!(problems.is_empty(), "{problems:#?}");
    // Measured 2026-10-26: 37 of the 720 steps hold arrays contesting a spill area; the rest are
    // compared. The bound keeps a change that makes most steps contested from passing unseen.
    assert!(contested <= 108, "{contested} of 720 steps not compared");
    assert!(structural > 0, "no row or column was inserted or deleted");
    assert!(
        saved_moved > 0,
        "no save followed a row or column inserted or deleted"
    );
    println!(
        "ACCEPT-NOTE: 12 runs of 60 steps, {structural} rows or columns inserted or deleted, {saved_moved} saves after them; {contested} steps with a contested spill not compared"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_review__subscribers_get_the_cells_a_spill_fills() {
    let mut session = session(&fixture());
    let calc = sheet(session.host(), "Calc");
    let area = GridArea::new(at_(calc, "Z5"), at_(calc, "Z7")).unwrap();
    let (_, by_area) = subscribe(&mut session, Interest::Area(area));
    let (_, by_sheet) = subscribe(&mut session, Interest::Sheet(calc));
    enter(&mut session, at_(calc, "Z5"), "=Data!A1:A3*2");
    let fresh = fresh_cells(session.host(), area);
    assert_eq!(fresh.len(), 3, "Z5 and the two cells its spill fills");
    assert_eq!(mirrored(&by_area, area), fresh);
    assert_eq!(mirrored(&by_sheet, area), fresh);
    // The spill shrinks to one cell when its source does: the cells it no longer fills are
    // delivered as empty.
    enter(&mut session, at_(calc, "Z5"), "=Data!A1*2");
    assert_eq!(mirrored(&by_area, area), fresh_cells(session.host(), area));
    assert_eq!(fresh_cells(session.host(), area).len(), 1);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_review__undo_keeps_a_value_inside_a_dynamic_arrays_area() {
    for second in ["5", ""] {
        let mut session = session(&fixture());
        let calc = sheet(session.host(), "Calc");
        // B2 blocks the spill of SEQUENCE(3,2) at A1: A1 shows #SPILL!.
        enter(&mut session, at_(calc, "B2"), "5");
        let area = GridArea::new(at_(calc, "A1"), at_(calc, "B3")).unwrap();
        let before = area_inputs(session.host(), area);
        assert_eq!(
            session_calculated(&session, at_(calc, "A1")),
            Some(CalcValue::error(
                calc_host::value::WorksheetErrorCode::Spill
            ))
        );
        // Replacing (or clearing) the anchor, then undoing it, leaves B2's value in place.
        enter(&mut session, at_(calc, "A1"), second);
        assert!(session.dispatch(SkinIntent::Undo).accepted());
        assert_eq!(
            area_inputs(session.host(), area),
            before,
            "after {second:?}"
        );
        assert_eq!(
            session_calculated(&session, at_(calc, "A1")),
            Some(CalcValue::error(
                calc_host::value::WorksheetErrorCode::Spill
            )),
            "after {second:?}"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_review__typed_newer_functions_calculate_and_save_with_their_prefix() {
    for (text, stored, expected) in [
        ("=SEQUENCE(3)", "_xlfn.SEQUENCE(3)", CalcValue::number(1.0)),
        (
            "=CONCAT(\"a\",\"b\")",
            "_xlfn.CONCAT(\"a\",\"b\")",
            CalcValue::text(calc_host::value::ExcelText::from_interop_assignment("ab")),
        ),
        (
            "=XLOOKUP(2,Data!A1:A3,Data!A1:A3)",
            "_xlfn.XLOOKUP(2,Data!A1:A3,Data!A1:A3)",
            CalcValue::number(2.0),
        ),
        ("=IFS(TRUE,1)", "_xlfn.IFS(TRUE,1)", CalcValue::number(1.0)),
        (
            "=UNIQUE(Data!A1:A3)",
            "_xlfn.UNIQUE(Data!A1:A3)",
            CalcValue::number(1.0),
        ),
    ] {
        let mut session = session(&fixture());
        let calc = sheet(session.host(), "Calc");
        enter(&mut session, at_(calc, "AA1"), text);
        let page = project(
            session.host(),
            &ProjectionRequest {
                cells: vec![at_(calc, "AA1")],
            },
        )
        .unwrap();
        assert_eq!(page.cells()[0].input(), Some(&formula(stored)), "{text}");
        assert_eq!(
            session_calculated(&session, at_(calc, "AA1")),
            Some(expected),
            "{text}"
        );
        let mut saved = Vec::new();
        session
            .save(&SavePolicy::default(), &mut saved)
            .expect("saved");
        let xml = part_text(&saved, "xl/worksheets/sheet2.xml");
        let escaped = stored.replace('"', "&quot;");
        assert!(
            xml.contains(&format!(">{stored}</f>")) || xml.contains(&format!(">{escaped}</f>")),
            "{text}: {xml}"
        );
        assert!(!xml.contains("#NAME?"), "{text}: {xml}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_review__a_pending_spill_makes_its_readers_stale_in_manual_mode() {
    let mut session = session(&fixture());
    let calc = sheet(session.host(), "Calc");
    enter(&mut session, at_(calc, "AC1"), "1");
    enter(&mut session, at_(calc, "AE6"), "=IF(AD3>2,AD6,3)");
    assert_eq!(
        session_calculated(&session, at_(calc, "AE6")),
        Some(CalcValue::number(3.0))
    );
    assert!(
        session
            .dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
            .accepted()
    );
    enter(&mut session, at_(calc, "AD1"), "=AC1:AC3*2");
    // AD1's result may spill over AD6, which AE6 reads: AE6 no longer stands.
    assert!(matches!(
        session_value(&session, at_(calc, "AE6")),
        ProjectedValue::Stale(_)
    ));
    assert!(session.dispatch(SkinIntent::Recalculate).accepted());
    assert_eq!(
        session_calculated(&session, at_(calc, "AE6")),
        Some(CalcValue::number(3.0))
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2_session__now_and_today_read_the_clock_the_application_sets() {
    let mut session = session(&fixture());
    let calc = sheet(session.host(), "Calc");
    // Without a clock, NOW is refused.
    enter(&mut session, at_(calc, "AA1"), "=NOW()");
    assert!(matches!(
        session_value(&session, at_(calc, "AA1")),
        ProjectedValue::NotModeled { .. }
    ));
    // 45000.75 is 2023-03-15 18:00 in the 1900 date system.
    session.set_clock(Some(45000.75));
    assert!(session.dispatch(SkinIntent::Recalculate).accepted());
    assert_eq!(
        session_calculated(&session, at_(calc, "AA1")),
        Some(CalcValue::number(45000.75))
    );
    enter(&mut session, at_(calc, "AA2"), "=TODAY()");
    assert_eq!(
        session_calculated(&session, at_(calc, "AA2")),
        Some(CalcValue::number(45000.0))
    );
}

/// dc-rif (D4): a skin's selection moves do not grow the intent log without bound, and the
/// receipts' sequence numbers keep counting every intent.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_rif__the_intent_log_keeps_the_latest_records() {
    let mut session = session(&calc_host::blank_package());
    let sheet1 = session.host().sheets()[0].id();
    let total = 2 * calc_host::INTENT_LOG_LIMIT + 3;
    let mut last = 0;
    for n in 0..total {
        let row = (n % 50) as u32 + 1;
        let receipt = session.dispatch(SkinIntent::Select(Selection::cell(
            CellKey::new(sheet1, row, 1).unwrap(),
        )));
        last = receipt.seq;
    }
    assert_eq!(last, total as u64);
    let log = session.intent_log();
    assert!(
        log.len() >= calc_host::INTENT_LOG_LIMIT && log.len() < 2 * calc_host::INTENT_LOG_LIMIT
    );
    assert_eq!(log.last().map(|record| record.seq), Some(total as u64));
    assert!(log.windows(2).all(|pair| pair[1].seq == pair[0].seq + 1));
}

/// dc-09v (D4b): the number format code of each cell and the sheet's row heights, column widths and
/// hidden rows and columns, as Excel wrote them (`tests/fixtures/dc_09v/formats.xlsx`, authored by
/// `make-formats.ps1`, whose `excel-shown.json` holds what Excel reported).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_09v__number_formats_and_sizes_as_the_file_states_them() {
    let host = open(include_bytes!("fixtures/dc_09v/formats.xlsx"));
    let sheet = sheet(&host, "Formats");
    for (address, code) in [
        // Excel on this machine (Windows digit grouping 123456,789) wrote the grouping comma it
        // was given as an escaped literal.
        ("A1", r"#\,##0.00"),
        ("A2", "0%"),
        ("A3", r"yyyy\-mm\-dd"),
        ("A4", r"0.00;[Red]\(0.00\)"),
        ("A5", "@"),
        ("A6", r"#\,##0"),
        ("A8", "h:mm AM/PM"),
        ("A10", "0.00E+00"),
        ("A7", "0.0"),
        ("A12", "General"),
        ("Z99", "General"),
    ] {
        assert_eq!(
            host.number_format(at(sheet, address)),
            Some(code),
            "{address}"
        );
    }
    let geometry = host.sheet_geometry(sheet).expect("the sheet's geometry");
    let width = |column: u32| geometry.column(column).and_then(|span| span.width);
    assert!(
        width(1).is_some_and(|w| w > 16.0 && w < 17.0),
        "{geometry:?}"
    );
    assert!(
        width(2).is_some_and(|w| w > 25.0 && w < 26.0),
        "{geometry:?}"
    );
    assert!(geometry.column(3).is_some_and(|span| span.hidden));
    assert!(!geometry.column(1).is_some_and(|span| span.hidden));
    assert_eq!(geometry.row(3).and_then(|row| row.height), Some(30.0));
    assert!(geometry.row(13).is_some_and(|row| row.hidden));
    assert_eq!(geometry.default_row_height, Some(15.0));
}

/// dc-09v (D4b): copy and paste, and fill, through the session: values copied, formulas relocated
/// (relative references keep their offsets, absolute ones stay, one moved off the grid is
/// #REF!), a target filled by repeating the source, a mismatched target refused, one undo step.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_09v__copy_relocates_formulas_and_fills() {
    let mut session = session(&calc_host::blank_package());
    let s = session.host().sheets()[0].id();
    for (address, text) in [
        ("A1", "1"),
        ("A2", "2"),
        ("A3", "3"),
        ("B1", "=A1*10"),
        ("C1", "=$A$1+A1"),
    ] {
        assert!(
            session
                .dispatch(SkinIntent::EnterCell {
                    at: at(s, address),
                    text: text.to_owned(),
                })
                .accepted()
        );
    }
    let area = |a: &str, b: &str| GridArea::new(at(s, a), at(s, b)).unwrap();
    let formula_at = |session: &HostSession, address: &str| {
        let cells = project(
            session.host(),
            &ProjectionRequest {
                cells: vec![at(s, address)],
            },
        )
        .unwrap();
        match cells.cells()[0].input() {
            Some(CellInput::Formula(formula)) => formula.text().to_string_lossy(),
            other => format!("{other:?}"),
        }
    };
    // Fill down: the first row of B1:C3 copied over the rest.
    let receipt = session.dispatch(SkinIntent::Copy {
        from: area("B1", "C1"),
        to: area("B2", "C3"),
        cut: false,
    });
    assert!(receipt.accepted(), "{:?}", receipt.error);
    assert_eq!(formula_at(&session, "B3"), "A3*10");
    assert_eq!(formula_at(&session, "C2"), "$A$1+A2");
    assert_eq!(
        calculated(session.host(), at(s, "B3")),
        Some(CalcValue::number(30.0))
    );
    assert_eq!(
        calculated(session.host(), at(s, "C3")),
        Some(CalcValue::number(4.0))
    );
    // A paste into one cell takes the source's shape; values copy as they are.
    let receipt = session.dispatch(SkinIntent::Copy {
        from: area("A1", "B2"),
        to: area("E5", "E5"),
        cut: false,
    });
    assert!(receipt.accepted(), "{:?}", receipt.error);
    assert_eq!(formula_at(&session, "F6"), "E6*10");
    assert!(matches!(
        value(session.host(), at(s, "E6")),
        ProjectedValue::Literal(_)
    ));
    // A relative reference moved off the grid is #REF!.
    let receipt = session.dispatch(SkinIntent::Copy {
        from: area("B1", "B1"),
        to: area("A2", "A2"),
        cut: false,
    });
    assert!(receipt.accepted(), "{:?}", receipt.error);
    assert_eq!(formula_at(&session, "A2"), "#REF!*10");
    // A target that is not a multiple of the source is refused, and nothing changes.
    let before = session.host().revision();
    let receipt = session.dispatch(SkinIntent::Copy {
        from: area("A1", "A2"),
        to: area("H1", "H3"),
        cut: false,
    });
    assert!(
        matches!(receipt.error, Some(IntentError::Copy(_))),
        "{:?}",
        receipt.error
    );
    assert_eq!(session.host().revision(), before);
    // One undo takes the last copy back.
    session.dispatch(SkinIntent::Undo);
    assert_eq!(
        calculated(session.host(), at(s, "B2")),
        Some(CalcValue::number(20.0))
    );
    assert!(matches!(
        value(session.host(), at(s, "A2")),
        ProjectedValue::Literal(_)
    ));
}

/// Clipboard capture keeps the authored formula, its value and its formats at copy time.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_5__snapshot_and_paste_special_are_independent_of_later_source_edits() {
    use calc_host::{CopySnapshot, PasteMode};
    let mut session = session(include_bytes!("fixtures/dc_09v/formats.xlsx"));
    let s = sheet(session.host(), "Formats");
    let area = |a: &str, b: &str| GridArea::new(at(s, a), at(s, b)).unwrap();
    assert!(enter(&mut session, at(s, "C1"), "=$A1+A$1+$A$1").accepted());
    let snapshot = CopySnapshot::capture(session.host(), area("C1", "C1")).unwrap();
    assert!(enter(&mut session, at(s, "C1"), "=999").accepted());
    let edits = snapshot
        .edits(
            session.host(),
            area("D2", "D2"),
            PasteMode::Formulas,
            false,
            None,
        )
        .unwrap();
    assert!(session.dispatch(SkinIntent::Edit { edits }).accepted());
    assert_eq!(
        session.host().model().cell(at(s, "D2")),
        Some(&formula("$A2+B$1+$A$1"))
    );
    let snapshot = CopySnapshot::capture(session.host(), area("A3", "A3")).unwrap();
    let style = session.host().cell_style(at(s, "A3"));
    assert!(enter(&mut session, at(s, "A3"), "100").accepted());
    let edits = snapshot
        .edits(
            session.host(),
            area("G3", "G3"),
            PasteMode::All,
            false,
            None,
        )
        .unwrap();
    assert!(session.dispatch(SkinIntent::Edit { edits }).accepted());
    assert_eq!(session.host().cell_style(at(s, "G3")), style);
    assert_eq!(
        session.host().model().cell(at(s, "G3")),
        Some(&number(45322.0))
    );
    let edits = snapshot
        .edits(
            session.host(),
            area("G4", "G4"),
            PasteMode::Formats,
            false,
            None,
        )
        .unwrap();
    assert!(session.dispatch(SkinIntent::Edit { edits }).accepted());
    assert_eq!(session.host().model().cell(at(s, "G4")), None);
    assert_eq!(session.host().cell_style(at(s, "G4")), style);
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(session.host().cell_style(at(s, "G4")), 0);
    assert_eq!(session.host().cell_style(at(s, "G3")), style);

    assert!(enter(&mut session, at(s, "G5"), "45323").accepted());
    let (_, received) = subscribe(&mut session, Interest::Area(area("G5", "G5")));
    received.borrow_mut().clear();
    let edits = snapshot
        .edits(
            session.host(),
            area("G5", "G5"),
            PasteMode::Formats,
            false,
            None,
        )
        .unwrap();
    assert!(session.dispatch(SkinIntent::Edit { edits }).accepted());
    assert_eq!(
        session.host().model().cell(at(s, "G5")),
        Some(&number(45323.0))
    );
    assert!(
        received
            .borrow()
            .iter()
            .any(|delta| delta.cells.iter().any(|cell| cell.at() == at(s, "G5"))),
        "a format-only change reaches the app's subscription"
    );
    received.borrow_mut().clear();
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert!(
        received
            .borrow()
            .iter()
            .any(|delta| delta.cells.iter().any(|cell| cell.at() == at(s, "G5"))),
        "Undo also publishes the restored format"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_7__frozen_panes_follow_history_axes_and_saved_packages() {
    let mut session = session(include_bytes!(
        "../../calc-doc/tests/fixtures/dc_5sr/features.xlsx"
    ));
    let s = session.host().sheets()[0].id();
    let panes = |host: &HostState| {
        host.model()
            .definitions()
            .frozen_panes(s)
            .map(|p| (p.rows(), p.columns()))
    };
    assert_eq!(
        panes(session.host()),
        Some((1, 1)),
        "the imported frozenSplit window"
    );
    let frozen = calc_host::FrozenPanes::new(2, 3).unwrap();
    assert!(
        session
            .dispatch(SkinIntent::Edit {
                edits: vec![CellEdit::FrozenPanes {
                    sheet: s,
                    panes: Some(frozen)
                }]
            })
            .accepted()
    );
    assert_eq!(panes(session.host()), Some((2, 3)));
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(panes(session.host()), Some((1, 1)));
    assert!(session.dispatch(SkinIntent::Redo).accepted());
    assert_eq!(panes(session.host()), Some((2, 3)));
    let save_panes = |session: &HostSession| {
        let mut bytes = Vec::new();
        session.save(&SavePolicy::default(), &mut bytes).unwrap();
        let reopened = open(&bytes);
        let s = reopened.sheets()[0].id();
        reopened
            .model()
            .definitions()
            .frozen_panes(s)
            .map(|p| (p.rows(), p.columns()))
    };
    assert_eq!(save_panes(&session), Some((2, 3)));
    assert!(
        session
            .dispatch(SkinIntent::EditAxis(
                AxisEdit::new(s, Axis::Rows, AxisChange::Insert, 1, 2).unwrap()
            ))
            .accepted()
    );
    assert_eq!(panes(session.host()), Some((4, 3)));
    assert_eq!(save_panes(&session), Some((4, 3)));
    let dimensions_before_unfreeze = session.host().sheet_geometry(s);
    assert!(
        session
            .dispatch(SkinIntent::Edit {
                edits: vec![CellEdit::FrozenPanes {
                    sheet: s,
                    panes: None
                }]
            })
            .accepted()
    );
    assert_eq!(
        session.host().sheet_geometry(s),
        dimensions_before_unfreeze,
        "unfreezing retains physical dimensions and hidden flags"
    );
    assert_eq!(save_panes(&session), None);
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(panes(session.host()), Some((4, 3)));
    let first = GridArea::cell(at(s, "A3"));
    let far = GridArea::cell(at(s, "K3"));
    let (_, received) = subscribe(&mut session, Interest::Areas(vec![first, far]));
    assert_eq!(received.borrow().last().unwrap().cells.len(), 2);
    assert!(
        received
            .borrow()
            .last()
            .unwrap()
            .cells
            .iter()
            .all(|cell| first.contains(cell.at()) || far.contains(cell.at()))
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_5__fill_series_partial_repetitions_and_stale_cut_are_atomic() {
    use calc_host::{CopyRefusal, CopySnapshot, PasteMode};
    let mut session = session(include_bytes!("fixtures/dc_09v/formats.xlsx"));
    let s = sheet(session.host(), "Formats");
    let area = |a: &str, b: &str| GridArea::new(at(s, a), at(s, b)).unwrap();
    let date = CopySnapshot::capture(session.host(), area("A3", "A3")).unwrap();
    let edits = date
        .fill_edits(session.host(), area("A4", "A5"), true, true)
        .unwrap();
    assert!(session.dispatch(SkinIntent::Edit { edits }).accepted());
    assert_eq!(
        session.host().model().cell(at(s, "A5")),
        Some(&number(45324.0))
    );
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert!(enter(&mut session, at(s, "D6"), "1").accepted());
    assert!(enter(&mut session, at(s, "D7"), "3").accepted());
    let seed = CopySnapshot::capture(session.host(), area("D6", "D7")).unwrap();
    let edits = seed
        .fill_edits(session.host(), area("D8", "D10"), true, true)
        .unwrap();
    assert!(session.dispatch(SkinIntent::Edit { edits }).accepted());
    assert_eq!(
        session.host().model().cell(at(s, "D10")),
        Some(&number(9.0))
    );
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(session.host().model().cell(at(s, "D8")), None);
    let edits = seed
        .fill_edits(session.host(), area("D8", "D10"), true, false)
        .unwrap();
    assert!(session.dispatch(SkinIntent::Edit { edits }).accepted());
    assert_eq!(
        session.host().model().cell(at(s, "D10")),
        Some(&number(1.0))
    );
    let revision = session.host().revision();
    assert_eq!(
        seed.edits(session.host(), area("E6", "E7"), PasteMode::All, true, None),
        Err(CopyRefusal::SourceChanged)
    );
    assert_eq!(session.host().revision(), revision);
    assert_eq!(session.host().model().cell(at(s, "D6")), Some(&number(1.0)));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_5__values_require_current_results_and_spill_writes_are_whole() {
    use calc_host::{CopySnapshot, PasteMode};
    let mut session = session(&calc_host::blank_package());
    let s = session.host().sheets()[0].id();
    let area = |a: &str, b: &str| GridArea::new(at(s, a), at(s, b)).unwrap();
    assert!(enter(&mut session, at(s, "A1"), "2").accepted());
    assert!(enter(&mut session, at(s, "B1"), "=A1*3").accepted());
    let values = CopySnapshot::capture(session.host(), area("B1", "B1")).unwrap();
    assert!(enter(&mut session, at(s, "A1"), "9").accepted());
    let edits = values
        .edits(
            session.host(),
            area("C1", "C1"),
            PasteMode::Values,
            false,
            None,
        )
        .unwrap();
    assert!(session.dispatch(SkinIntent::Edit { edits }).accepted());
    assert_eq!(session.host().model().cell(at(s, "C1")), Some(&number(6.0)));
    assert!(enter(&mut session, at(s, "E1"), "=SEQUENCE(3)").accepted());
    let revision = session.host().revision();
    assert!(
        values
            .edits(
                session.host(),
                area("D2", "E2"),
                PasteMode::All,
                false,
                None
            )
            .is_err()
    );
    assert_eq!(session.host().revision(), revision);
    assert!(
        session
            .dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual))
            .accepted()
    );
    assert!(enter(&mut session, at(s, "A1"), "10").accepted());
    let stale = CopySnapshot::capture(session.host(), area("B1", "B1")).unwrap();
    assert!(
        stale
            .edits(
                session.host(),
                area("G1", "G1"),
                PasteMode::Values,
                false,
                None
            )
            .is_err()
    );
    assert!(
        session
            .dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Automatic))
            .accepted()
    );
    assert!(enter(&mut session, at(s, "H1"), "=SEQUENCE(A1)").accepted());
    assert!(
        session
            .dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual))
            .accepted()
    );
    assert!(enter(&mut session, at(s, "A1"), "11").accepted());
    let child = CopySnapshot::capture(session.host(), area("H2", "H2")).unwrap();
    assert!(matches!(
        child.edits(
            session.host(),
            area("J2", "J2"),
            PasteMode::All,
            false,
            None
        ),
        Err(calc_host::CopyRefusal::UnavailableValue { .. })
    ));
    let edits = vec![CellEdit::SetArray {
        array: calc_host::ArrayFormula::new(area("K1", "K3"), calc_host::ArrayKind::Legacy),
        formula: FormulaText::new(calc_host::value::ExcelText::from_interop_assignment(
            "{1;2;3}",
        ))
        .unwrap(),
    }];
    assert!(session.dispatch(SkinIntent::Edit { edits }).accepted());
    let legacy = CopySnapshot::capture(session.host(), area("K1", "K3")).unwrap();
    assert!(matches!(
        legacy.fill_edits(session.host(), area("K4", "K4"), true, false),
        Err(calc_host::CopyRefusal::LegacyFill)
    ));
}

// ---------------------------------------------------------------------------------------------
// dc-jhn (D2b): rows and columns inserted and deleted through the session, against Excel making
// the same edits (tests/fixtures/d2/make-structural.ps1: structural-base.xlsx before,
// structural-after.xlsx after, its capture structural-after.capture.json).

/// The edits make-structural.ps1 makes, in order: sheet, rows or columns, insert or delete, first
/// position, count.
const D2B_STEPS: [(&str, Axis, AxisChange, u32, u32); 8] = [
    ("S", Axis::Rows, AxisChange::Insert, 4, 2),
    ("S", Axis::Rows, AxisChange::Delete, 9, 1),
    ("S", Axis::Columns, AxisChange::Insert, 2, 1),
    ("S", Axis::Columns, AxisChange::Delete, 5, 1),
    ("S", Axis::Rows, AxisChange::Delete, 2, 1),
    ("T", Axis::Rows, AxisChange::Insert, 3, 1),
    ("U", Axis::Rows, AxisChange::Insert, 1, 1),
    ("U", Axis::Columns, AxisChange::Insert, 1, 2),
];

fn d2b_fixture(name: &str) -> Vec<u8> {
    match name {
        "base" => include_bytes!("fixtures/d2/structural-base.xlsx").to_vec(),
        "after" => include_bytes!("fixtures/d2/structural-after.xlsx").to_vec(),
        other => panic!("no fixture {other}"),
    }
}

/// Each sheet's A1:J16 as a host projects it, by sheet name and (row, column).
fn d2b_cells(host: &HostState) -> BTreeMap<(String, u32, u32), ProjectedCell> {
    let mut cells = BTreeMap::new();
    for entry in host.sheets() {
        let name = entry.name().text().to_string_lossy();
        let area = GridArea::new(at_(entry.id(), "A1"), at_(entry.id(), "J16")).unwrap();
        for (at, cell) in fresh_cells(host, area) {
            cells.insert((name.clone(), at.row(), at.column()), cell);
        }
    }
    cells
}

fn d2b_inputs(host: &HostState) -> BTreeMap<(String, u32, u32), CellInput> {
    d2b_cells(host)
        .into_iter()
        .filter_map(|(key, cell)| cell.input().cloned().map(|input| (key, input)))
        .collect()
}

fn d2b_apply(session: &mut HostSession) {
    for (name, axis, change, first, count) in D2B_STEPS {
        let sheet = sheet(session.host(), name);
        let edit = AxisEdit::new(sheet, axis, change, first, count).unwrap();
        let receipt = session.dispatch(SkinIntent::EditAxis(edit));
        assert!(
            receipt.accepted(),
            "{name} {axis:?} {change:?} {first} {count}: {:?}",
            receipt.error
        );
    }
}

/// The capture of structural-after.xlsx: Excel's value of each formula cell, by sheet name and
/// (row, column).
fn d2b_excel_values() -> BTreeMap<(String, u32, u32), CalcValue> {
    let capture = calc_verdict::CaptureRecord::decode(include_bytes!(
        "fixtures/d2/structural-after.capture.json"
    ))
    .expect("the capture decodes");
    capture
        .observations
        .iter()
        .filter_map(|observed| {
            let calc_verdict::Observation::ExcelRaw(excel) = &observed.observation else {
                return None;
            };
            let key = (
                observed.cell.sheet().to_owned(),
                observed.cell.row(),
                observed.cell.column(),
            );
            Some((key, excel.clone()))
        })
        .collect()
}

/// `host` holds what Excel's package holds after the same edits (`excel_after`): every input
/// (each formula's references adjusted as Excel's, #REF! for a deleted target included), the
/// defined names, and, on each sheet's A1:J16, each cell's number format, each row's height and
/// hidden flag and each column's width, the inserted ones taking the row above's or the column to
/// the left's, as Excel's insertion does.
fn d2b_assert_like_excel(host: &HostState, excel_after: &HostState, what: &str) {
    assert_eq!(d2b_inputs(host), d2b_inputs(excel_after), "{what}: inputs");
    let names = |host: &HostState| -> Vec<String> {
        host.defined_names()
            .iter()
            .map(|name| format!("{} {}", name.name, name.formula))
            .collect()
    };
    assert_eq!(names(host), names(excel_after), "{what}: defined names");
    for entry in host.sheets() {
        let name = entry.name().text().to_string_lossy();
        let mine = entry.id();
        let theirs = sheet(excel_after, &name);
        for row in 1..=16 {
            for column in 1..=10 {
                assert_eq!(
                    host.number_format(CellKey::new(mine, row, column).unwrap()),
                    excel_after.number_format(CellKey::new(theirs, row, column).unwrap()),
                    "{what}: {name} row {row} column {column}"
                );
            }
        }
        let (ours, excel) = (
            host.sheet_geometry(mine).unwrap(),
            excel_after.sheet_geometry(theirs).unwrap(),
        );
        for row in 1..=16 {
            assert_eq!(
                ours.row(row).and_then(|format| format.height),
                excel.row(row).and_then(|format| format.height),
                "{what}: {name} row {row} height"
            );
            assert_eq!(
                ours.row(row).is_some_and(|format| format.hidden),
                excel.row(row).is_some_and(|format| format.hidden),
                "{what}: {name} row {row} hidden"
            );
        }
        for column in 1..=10 {
            assert_eq!(
                ours.column(column).and_then(|span| span.width),
                excel.column(column).and_then(|span| span.width),
                "{what}: {name} column {column} width"
            );
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2b_structural__edits_match_excel_and_undo_restores() {
    let mut session = session(&d2b_fixture("base"));
    let base_inputs = d2b_inputs(session.host());
    d2b_apply(&mut session);
    let excel_after = open(&d2b_fixture("after"));
    d2b_assert_like_excel(session.host(), &excel_after, "the session");

    // Every formula cell calculates Excel's value (capture of structural-after.xlsx).
    let excel_values = d2b_excel_values();
    let ours = d2b_cells(session.host());
    for (key, excel) in &excel_values {
        match ours.get(key).map(|cell| cell.value()) {
            Some(ProjectedValue::Calculated { value, .. }) => assert_eq!(value, excel, "{key:?}"),
            other => panic!("{key:?} is {other:?}, Excel shows {excel:?}"),
        }
    }
    assert_eq!(excel_values.len(), 15, "every formula cell of the capture");

    // The save writes the moved rows and columns: the package reopened holds what Excel's holds,
    // with each formula's calculated value as its cached value.
    let mut saved = Vec::new();
    let receipt = session
        .save(&SavePolicy::default(), &mut saved)
        .expect("the save after the edits");
    let reopened = open(&saved);
    d2b_assert_like_excel(&reopened, &excel_after, "the saved package");
    let cached = d2b_cells(&reopened);
    for (key, excel) in &excel_values {
        match cached.get(key).map(|cell| cell.value()) {
            Some(ProjectedValue::FileCached(cache)) => {
                assert_eq!(cache.value(), &cached_of(excel), "{key:?}")
            }
            other => panic!("{key:?} is saved as {other:?}, Excel shows {excel:?}"),
        }
    }
    // Native-only: the package is written where DNACALC_D2B_SAVED names, for
    // tests/fixtures/d2/compare-structural.ps1 to open in Excel.
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(path) = std::env::var("DNACALC_D2B_SAVED") {
        std::fs::write(&path, &saved).expect("written");
        println!("ACCEPT-NOTE: d2b_structural: {receipt:?} written to {path}");
    }
    let _ = receipt;

    // Undo takes the edits back one at a time, to the workbook as opened; redo makes them again.
    for _ in D2B_STEPS {
        assert!(session.dispatch(SkinIntent::Undo).accepted());
    }
    assert_eq!(d2b_inputs(session.host()), base_inputs);
    for _ in D2B_STEPS {
        assert!(session.dispatch(SkinIntent::Redo).accepted());
    }
    assert_eq!(d2b_inputs(session.host()), d2b_inputs(&excel_after));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn d2b_structural__edits_excel_refuses_are_refused() {
    let mut session = session(&fixture());
    let calc = sheet(session.host(), "Calc");
    let data = sheet(session.host(), "Data");
    let revision = session.host().revision();
    let refused = |session: &mut HostSession, edit: AxisEdit| {
        let receipt = session.dispatch(SkinIntent::EditAxis(edit));
        match receipt.error {
            Some(IntentError::Command(calc_host::CommandError::Document(error))) => error,
            other => panic!("{edit:?}: {other:?}"),
        }
    };
    // Deleting one row of the legacy array L1:L3 changes part of it.
    let error = refused(
        &mut session,
        AxisEdit::new(calc, Axis::Rows, AxisChange::Delete, 2, 1).unwrap(),
    );
    assert!(
        matches!(error, calc_host::EditError::ArrayPart { .. }),
        "{error:?}"
    );
    // The table Sales (Data!D1:E6 with its header in row 1): deleting the header row, or
    // inserting a column inside it, is refused.
    let error = refused(
        &mut session,
        AxisEdit::new(data, Axis::Rows, AxisChange::Delete, 1, 1).unwrap(),
    );
    assert!(
        matches!(error, calc_host::EditError::TablePart { .. }),
        "{error:?}"
    );
    let error = refused(
        &mut session,
        AxisEdit::new(data, Axis::Columns, AxisChange::Insert, 5, 1).unwrap(),
    );
    assert!(
        matches!(error, calc_host::EditError::TablePart { .. }),
        "{error:?}"
    );
    assert_eq!(session.host().revision(), revision);
    assert_eq!(session.history_depth(), (0, 0));
}

/// dc-09v (D4b): a copy takes a legacy array whole (and refuses part of one), copies a dynamic
/// array's spilled cells as values when their anchor stays behind; a cut moves cells and every
/// reference to them, as Excel's cut and paste does; one undo takes each back.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_09v__copy_arrays_and_cut_moves_references() {
    let mut session = session(&calc_host::blank_package());
    let s = session.host().sheets()[0].id();
    let enter = |session: &mut HostSession, address: &str, text: &str| {
        let receipt = session.dispatch(SkinIntent::EnterCell {
            at: at(s, address),
            text: text.to_owned(),
        });
        assert!(receipt.accepted(), "{address}: {:?}", receipt.error);
    };
    for (address, text) in [
        ("A1", "1"),
        ("A2", "2"),
        ("A3", "3"),
        ("C1", "=SUM(B1:B3)"),
        ("D1", "={1;2;3}"),
        ("E5", "=SUM(A1:A4)"),
    ] {
        enter(&mut session, address, text);
    }
    let area = |a: &str, b: &str| GridArea::new(at(s, a), at(s, b)).unwrap();
    // B1:B3 holds the legacy array {=A1:A3*10}.
    let receipt = session.dispatch(SkinIntent::Edit {
        edits: vec![CellEdit::SetArray {
            array: calc_host::ArrayFormula::new(area("B1", "B3"), calc_host::ArrayKind::Legacy),
            formula: match formula("A1:A3*10") {
                CellInput::Formula(text) => text,
                _ => unreachable!(),
            },
        }],
    });
    assert!(receipt.accepted(), "{:?}", receipt.error);
    assert_eq!(
        calculated(session.host(), at(s, "C1")),
        Some(CalcValue::number(60.0))
    );
    let input_text = |session: &HostSession, address: &str| match project(
        session.host(),
        &ProjectionRequest {
            cells: vec![at(s, address)],
        },
    )
    .unwrap()
    .cells()[0]
        .input()
    {
        Some(CellInput::Formula(text)) => format!("={}", text.text().to_string_lossy()),
        Some(other) => format!("{other:?}"),
        None => String::new(),
    };
    let copy = |session: &mut HostSession, from: GridArea, to: &str, cut: bool| {
        session.dispatch(SkinIntent::Copy {
            from,
            to: area(to, to),
            cut,
        })
    };
    // The whole array copied to F1: an array over F1:F3, its formula relocated.
    let receipt = copy(&mut session, area("B1", "B3"), "F1", false);
    assert!(receipt.accepted(), "{:?}", receipt.error);
    assert_eq!(input_text(&session, "F1"), "=E1:E3*10");
    assert_eq!(
        calculated(session.host(), at(s, "F3")),
        Some(CalcValue::number(0.0))
    );
    // Part of it is refused.
    let receipt = copy(&mut session, area("B2", "B3"), "H1", false);
    assert!(
        matches!(
            receipt.error,
            Some(IntentError::Copy(
                calc_host::CopyRefusal::PartOfArray { .. }
            ))
        ),
        "{:?}",
        receipt.error
    );
    // The spilled D2:D3 without their anchor copy as values.
    let receipt = copy(&mut session, area("D2", "D3"), "G1", false);
    assert!(receipt.accepted(), "{:?}", receipt.error);
    assert!(matches!(
        value(session.host(), at(s, "G2")),
        ProjectedValue::Literal(_)
    ));
    assert_eq!(input_text(&session, "G2"), format!("{:?}", number(3.0)));
    // Cut A1:A3 to H5: the array's whole-range reference follows; the partial one stays.
    let receipt = copy(&mut session, area("A1", "A3"), "H5", true);
    assert!(receipt.accepted(), "{:?}", receipt.error);
    assert_eq!(input_text(&session, "A1"), "");
    assert_eq!(input_text(&session, "H6"), format!("{:?}", number(2.0)));
    assert_eq!(input_text(&session, "B1"), "=H5:H7*10");
    assert_eq!(input_text(&session, "E5"), "=SUM(A1:A4)");
    assert_eq!(
        calculated(session.host(), at(s, "C1")),
        Some(CalcValue::number(60.0))
    );
    assert_eq!(
        calculated(session.host(), at(s, "E5")),
        Some(CalcValue::number(0.0))
    );
    // One undo takes the move back.
    session.dispatch(SkinIntent::Undo);
    assert_eq!(input_text(&session, "B1"), "=A1:A3*10");
    assert_eq!(input_text(&session, "H5"), "");
    assert_eq!(
        calculated(session.host(), at(s, "E5")),
        Some(CalcValue::number(6.0))
    );
}

/// dc-09v: a copy takes each cell's format with it and a cut moves it, leaving the source in the
/// default format; each is one undo step, and the save writes the formats (each cell's `s`), so
/// the package reopens with them.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_09v__copy_and_cut_take_cell_formats() {
    let mut session = session(include_bytes!("fixtures/dc_09v/formats.xlsx"));
    let s = sheet(session.host(), "Formats");
    let area = |a: &str, b: &str| GridArea::new(at(s, a), at(s, b)).unwrap();
    let code = |host: &HostState, address: &str| {
        host.number_format(at(s, address))
            .map(str::to_owned)
            .unwrap_or_default()
    };
    let copy = |session: &mut HostSession, from: GridArea, to: GridArea, cut: bool| {
        let receipt = session.dispatch(SkinIntent::Copy { from, to, cut });
        assert!(receipt.accepted(), "{:?}", receipt.error);
    };
    copy(&mut session, area("A1", "A1"), area("G1", "G1"), false);
    assert_eq!(code(session.host(), "G1"), r"#\,##0.00");
    copy(&mut session, area("A2", "A3"), area("G2", "G2"), false);
    assert_eq!(code(session.host(), "G2"), "0%");
    assert_eq!(code(session.host(), "G3"), r"yyyy\-mm\-dd");
    // An empty unformatted cell copied over a formatted one takes its format away.
    copy(&mut session, area("Z99", "Z99"), area("G3", "G3"), false);
    assert_eq!(code(session.host(), "G3"), "General");
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(code(session.host(), "G3"), r"yyyy\-mm\-dd");
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(code(session.host(), "G2"), "General");
    assert_eq!(code(session.host(), "G3"), "General");
    assert_eq!(code(session.host(), "G1"), r"#\,##0.00");
    // A cut moves the format: the source is left in the default format.
    copy(&mut session, area("A4", "A4"), area("H4", "H4"), true);
    assert_eq!(code(session.host(), "H4"), r"0.00;[Red]\(0.00\)");
    assert_eq!(code(session.host(), "A4"), "General");
    // A row inserted above it moves the copied format with its cell.
    let receipt = session.dispatch(SkinIntent::EditAxis(
        AxisEdit::new(s, Axis::Rows, AxisChange::Insert, 2, 1).unwrap(),
    ));
    assert!(receipt.accepted(), "{:?}", receipt.error);
    assert_eq!(code(session.host(), "H5"), r"0.00;[Red]\(0.00\)");
    assert_eq!(code(session.host(), "H4"), "General");

    let mut saved = Vec::new();
    session
        .save(&SavePolicy::default(), &mut saved)
        .expect("saved");
    let reopened = open(&saved);
    let r = sheet(&reopened, "Formats");
    for (address, expected) in [
        ("G1", r"#\,##0.00"),
        ("H5", r"0.00;[Red]\(0.00\)"),
        ("H4", "General"),
        ("A5", "General"),
        ("A1", r"#\,##0.00"),
        ("A3", "0%"),
        ("G3", "General"),
    ] {
        assert_eq!(
            reopened.number_format(at(r, address)),
            Some(expected),
            "{address} reopened"
        );
    }
    assert!(matches!(
        value(&reopened, at(r, "H5")),
        ProjectedValue::Literal(_)
    ));
    // Native-only: the package is written where DNACALC_RIF_SAVE_DIR names, for Excel to reopen
    // (tests/fixtures/d2/measure-saved-open.ps1).
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(dir) = std::env::var("DNACALC_RIF_SAVE_DIR") {
        let path = std::path::Path::new(&dir).join("dc-09v-copied-formats.xlsx");
        std::fs::write(&path, &saved).expect("written");
        println!(
            "ACCEPT-NOTE: dc_09v copied formats written to {}",
            path.display()
        );
    }

    // Undo takes the insertion and then the cut back, the source's format with it.
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(code(session.host(), "A4"), r"0.00;[Red]\(0.00\)");
    assert_eq!(code(session.host(), "H4"), "General");
}

/// dc-gf0's cases, as tests/fixtures/dc_gf0/make-cut.ps1 makes them in Excel: name, source sheet,
/// source range, target sheet, target top-left cell, and the package Excel saved after the cut.
const GF0_CASES: [(&str, &str, &str, &str, &str, &[u8]); 5] = [
    (
        "same-sheet",
        "Data",
        "A1:B3",
        "Data",
        "E5",
        include_bytes!("fixtures/dc_gf0/cut-same-sheet.xlsx"),
    ),
    (
        "to-target-sheet",
        "Data",
        "A1:B3",
        "Target Sheet",
        "C2",
        include_bytes!("fixtures/dc_gf0/cut-to-target-sheet.xlsx"),
    ),
    (
        "to-other",
        "Data",
        "A1:A3",
        "Other",
        "E1",
        include_bytes!("fixtures/dc_gf0/cut-to-other.xlsx"),
    ),
    (
        "middle-to-target",
        "Data",
        "A2:A3",
        "Target Sheet",
        "F1",
        include_bytes!("fixtures/dc_gf0/cut-middle-to-target.xlsx"),
    ),
    (
        "local-name-to-other",
        "Data",
        "D2",
        "Other",
        "G1",
        include_bytes!("fixtures/dc_gf0/cut-local-name-to-other.xlsx"),
    ),
];

/// The defined names as `name scope formula`, in name order (a name an edit defines is the
/// workbook's last; Excel writes its names sorted).
fn gf0_names(host: &HostState) -> Vec<String> {
    let mut names: Vec<String> = host
        .defined_names()
        .iter()
        .map(|name| format!("{} {:?} {}", name.name, name.scope, name.formula))
        .collect();
    names.sort();
    names
}

/// Each formula cell's calculated value (A1:J16 of each sheet), by sheet name and position.
fn gf0_values(host: &HostState) -> BTreeMap<(String, u32, u32), CachedValue> {
    d2b_cells(host)
        .into_iter()
        .filter_map(|(key, cell)| match cell.value() {
            ProjectedValue::Calculated { value, .. } => Some((key, cached_of(value))),
            ProjectedValue::FileCached(cache) => Some((key, cache.value().clone())),
            _ => None,
        })
        .collect()
}

/// dc-gf0: a cut and paste moves cells as Excel's does, on one sheet and to another, against the
/// packages Excel saved after the same cuts (tests/fixtures/dc_gf0, Excel 16.0.20430): every
/// cell's input (each reference to the moved cells following them, `#REF!` for a reference to
/// cells the paste wrote over, a range losing a band moved to another sheet or written over, a
/// formula moved to another sheet reading the cells it read), every defined name's formula (an
/// absolute one following the cells, a mixed one kept), and every formula's value as Excel
/// calculated it. One undo step takes it back; the save writes the cells and the names, and the
/// package reopens as Excel's does.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_gf0__cuts_move_cells_references_and_names_as_excel_does() {
    let base = include_bytes!("fixtures/dc_gf0/cut-base.xlsx");
    for (case, from_sheet, from, to_sheet, to, excel) in GF0_CASES {
        let mut session = session(base);
        let opened = open(base);
        let excel_after = open(excel);
        let (first, last) = from.split_once(':').unwrap_or((from, from));
        let source = sheet(session.host(), from_sheet);
        let target = sheet(session.host(), to_sheet);
        let receipt = session.dispatch(SkinIntent::Copy {
            from: GridArea::new(at(source, first), at(source, last)).unwrap(),
            to: GridArea::cell(at(target, to)),
            cut: true,
        });
        assert!(receipt.accepted(), "{case}: {:?}", receipt.error);
        assert_eq!(
            d2b_inputs(session.host()),
            d2b_inputs(&excel_after),
            "{case}: inputs"
        );
        assert_eq!(
            gf0_names(session.host()),
            gf0_names(&excel_after),
            "{case}: names"
        );
        assert_eq!(
            gf0_values(session.host()),
            gf0_values(&excel_after),
            "{case}: values"
        );

        let mut saved = Vec::new();
        session
            .save(&SavePolicy::default(), &mut saved)
            .expect("saved");
        let reopened = open(&saved);
        assert_eq!(
            d2b_inputs(&reopened),
            d2b_inputs(&excel_after),
            "{case}: saved inputs"
        );
        assert_eq!(
            gf0_names(&reopened),
            gf0_names(&excel_after),
            "{case}: saved names"
        );
        // Native-only: the package is written where DNACALC_RIF_SAVE_DIR names, for Excel to
        // reopen (tests/fixtures/d2/measure-saved-open.ps1).
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(dir) = std::env::var("DNACALC_RIF_SAVE_DIR") {
            let path = std::path::Path::new(&dir).join(format!("dc-gf0-cut-{case}.xlsx"));
            std::fs::write(&path, &saved).expect("written");
        }

        assert!(session.dispatch(SkinIntent::Undo).accepted(), "{case}");
        assert_eq!(
            d2b_inputs(session.host()),
            d2b_inputs(&opened),
            "{case}: undone inputs"
        );
        assert_eq!(
            gf0_names(session.host()),
            gf0_names(&opened),
            "{case}: undone names"
        );
        assert_eq!(
            gf0_values(session.host()),
            gf0_values(&opened),
            "{case}: undone values"
        );
    }
}

/// dc-gf0: the precedents and dependents the engine's graph holds, for a skin's trace: a formula's
/// placed references as areas (whole columns as the column's area), and the formula cells that
/// read a cell, in cell order; a literal has no precedents.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_gf0__precedents_and_dependents_from_the_graph() {
    let mut session = session(&calc_host::blank_package());
    let s = session.host().sheets()[0].id();
    for (address, text) in [
        ("A1", "1"),
        ("B1", "=A1*2"),
        ("C1", "=SUM(A1:B1)"),
        ("D1", "=SUM(A:A)+B1"),
    ] {
        let receipt = session.dispatch(SkinIntent::EnterCell {
            at: at(s, address),
            text: text.to_owned(),
        });
        assert!(receipt.accepted(), "{address}: {:?}", receipt.error);
    }
    let host = session.host();
    let area = |a: &str, b: &str| GridArea::new(at(s, a), at(s, b)).unwrap();
    assert_eq!(host.precedents(at(s, "A1")), None);
    assert_eq!(host.precedents(at(s, "B1")), Some(vec![area("A1", "A1")]));
    assert_eq!(host.precedents(at(s, "C1")), Some(vec![area("A1", "B1")]));
    assert_eq!(
        host.precedents(at(s, "D1")),
        Some(vec![area("A1", "A1048576"), area("B1", "B1")])
    );
    assert_eq!(
        host.dependents(at(s, "A1")),
        vec![at(s, "B1"), at(s, "C1"), at(s, "D1")]
    );
    assert_eq!(host.dependents(at(s, "B1")), vec![at(s, "C1"), at(s, "D1")]);
    assert_eq!(host.dependents(at(s, "D1")), Vec::<CellKey>::new());
    assert_eq!(
        host.input_cells(s),
        vec![at(s, "A1"), at(s, "B1"), at(s, "C1"), at(s, "D1")]
    );
    assert_eq!(
        host.formula_cells(s),
        vec![at(s, "B1"), at(s, "C1"), at(s, "D1")]
    );
}

/// dc-1yp: an edited save of a workbook whose metadata part holds the dynamic-array properties
/// (cdoc_m1_2's d1.xlsx: Sheet2!A1 =SEQUENCE(3,2) saved as a dynamic array), which the save
/// extends with value metadata for the errors it writes, reopens, and its model is the session's.
/// The value metadata used to be put inside the dynamic-array properties' futureMetadata record,
/// where the first `<extLst` of the part is, and the loader then refused the saved package.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_1yp__an_edited_save_extends_dynamic_array_metadata_where_it_belongs() {
    let mut session = session(include_bytes!(
        "../../calc-doc/tests/fixtures/cdoc_m1_2/d1.xlsx"
    ));
    let first = session.host().sheets()[0].id();
    for intent in [
        SkinIntent::Recalculate,
        SkinIntent::EnterCell {
            at: at_(first, "H1"),
            text: "12345.5".to_owned(),
        },
        SkinIntent::EnterCell {
            at: at_(first, "I1"),
            text: "=H1*2".to_owned(),
        },
        SkinIntent::Recalculate,
    ] {
        assert!(session.dispatch(intent).accepted());
    }
    let mut saved = Vec::new();
    session.save(&SavePolicy::default(), &mut saved).unwrap();
    let reopened = open(&saved);
    let inputs = |host: &HostState| -> Vec<(CellKey, CellInput)> {
        host.sheets()
            .iter()
            .flat_map(|entry| {
                host.model()
                    .sheet_cells(entry.id())
                    .into_iter()
                    .flatten()
                    .map(|(at, input)| (at, input.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    assert_eq!(inputs(&reopened), inputs(session.host()));
    // A dynamic array's area is the extent of its last result, which the save writes as its
    // ref: compared by anchor and kind; a legacy array by its area.
    let arrays = |host: &HostState| -> Vec<String> {
        host.model()
            .definitions()
            .arrays()
            .map(|array| match array.kind() {
                calc_host::ArrayKind::Legacy => format!("{:?}", array.area()),
                kind => format!("{:?} {kind:?}", array.anchor()),
            })
            .collect()
    };
    assert_eq!(arrays(&reopened), arrays(session.host()));
    let (theirs, mine) = (
        reopened.model().definitions(),
        session.host().model().definitions(),
    );
    assert_eq!(theirs.names(), mine.names());
    assert_eq!(theirs.tables(), mine.tables());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_7__physical_dimensions_recalculate_facts_and_hidden_rows_and_round_trip() {
    let mut session = session(include_bytes!("fixtures/dc_gf0/cut-base.xlsx"));
    let s = sheet(session.host(), "Data");
    assert!(
        session
            .dispatch(SkinIntent::Edit {
                edits: vec![
                    CellEdit::Set {
                        at: at(s, "A1"),
                        input: number(1.0)
                    },
                    CellEdit::Set {
                        at: at(s, "A2"),
                        input: number(2.0)
                    },
                    CellEdit::Set {
                        at: at(s, "C20"),
                        input: formula("SUBTOTAL(109,A1:A2)")
                    },
                    CellEdit::Set {
                        at: at(s, "C21"),
                        input: formula("SUM(CELL(\"width\",B1))")
                    },
                ]
            })
            .accepted()
    );
    let format = |axis, index, size, hidden, custom| {
        CellEdit::AxisFormat(
            calc_host::AxisFormatEdit::new(
                s,
                axis,
                index,
                calc_host::AxisFormat::new(size, hidden, custom).unwrap(),
            )
            .unwrap(),
        )
    };
    assert_eq!(
        calculated(session.host(), at(s, "C20")),
        Some(CalcValue::number(3.0))
    );
    assert!(
        session
            .dispatch(SkinIntent::Edit {
                edits: vec![
                    format(Axis::Columns, 2, Some(20.7109375), false, true),
                    format(Axis::Rows, 9, Some(30.0), false, false),
                    format(Axis::Rows, 2, None, true, false),
                ]
            })
            .accepted()
    );
    assert_eq!(
        calculated(session.host(), at(s, "C20")),
        Some(CalcValue::number(1.0))
    );
    assert_eq!(
        calculated(session.host(), at(s, "C21")),
        Some(CalcValue::number(20.0))
    );
    let mut bytes = Vec::new();
    session.save(&SavePolicy::default(), &mut bytes).unwrap();
    let reopened = open(&bytes);
    let new_s = sheet(&reopened, "Data");
    let g = reopened.sheet_geometry(new_s).unwrap();
    assert_eq!(g.column(2).unwrap().width, Some(20.7109375));
    assert_eq!(g.row(9).unwrap().height, Some(30.0));
    assert!(g.row(2).unwrap().hidden);
    assert_eq!(session.host().model().cell(at(s, "A2")), Some(&number(2.0)));
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(
        calculated(session.host(), at(s, "C20")),
        Some(CalcValue::number(3.0))
    );
    assert!(
        !session
            .host()
            .sheet_geometry(s)
            .unwrap()
            .row(2)
            .is_some_and(|r| r.hidden)
    );
    assert!(session.dispatch(SkinIntent::Redo).accepted());
    assert!(
        session
            .dispatch(SkinIntent::EditAxis(
                AxisEdit::new(s, Axis::Columns, AxisChange::Insert, 2, 1).unwrap()
            ))
            .accepted()
    );
    assert_eq!(
        session
            .host()
            .sheet_geometry(s)
            .unwrap()
            .column(3)
            .unwrap()
            .width,
        Some(20.7109375)
    );
    bytes.clear();
    session.save(&SavePolicy::default(), &mut bytes).unwrap();
    let reopened = open(&bytes);
    let new_s = sheet(&reopened, "Data");
    assert_eq!(
        reopened
            .sheet_geometry(new_s)
            .unwrap()
            .column(3)
            .unwrap()
            .width,
        Some(20.7109375)
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_7__physical_dimension_edits_invalidate_unverified_file_caches() {
    let mut host = open(&fixture());
    let s = sheet(&host, "Calc");
    let cached = at(s, "U3");
    assert!(matches!(
        value(&host, cached),
        ProjectedValue::FileCached(_)
    ));
    let format = calc_host::AxisFormat::new(Some(20.7109375), false, true).unwrap();
    edit(
        &mut host,
        vec![CellEdit::AxisFormat(
            calc_host::AxisFormatEdit::new(s, Axis::Columns, 1, format).unwrap(),
        )],
    );
    assert!(matches!(value(&host, cached), ProjectedValue::Stale(_)));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_971p_7__merged_ranges_follow_the_current_structural_history() {
    let mut session = session(include_bytes!(
        "../../calc-doc/tests/fixtures/dc_5sr/features.xlsx"
    ));
    let s = session.host().sheets()[0].id();
    let merged = |a, b| GridArea::new(at(s, a), at(s, b)).unwrap();
    assert_eq!(session.host().merged_areas(s), vec![merged("H1", "I1")]);
    let axis = AxisEdit::new(s, Axis::Columns, AxisChange::Insert, 8, 1).unwrap();
    assert!(session.dispatch(SkinIntent::EditAxis(axis)).accepted());
    assert_eq!(session.host().merged_areas(s), vec![merged("I1", "J1")]);
    let mut bytes = Vec::new();
    session.save(&SavePolicy::default(), &mut bytes).unwrap();
    let reopened = open(&bytes);
    let ss = reopened.sheets()[0].id();
    assert_eq!(
        reopened.merged_areas(ss),
        vec![GridArea::new(at(ss, "I1"), at(ss, "J1")).unwrap()]
    );
    assert!(session.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(session.host().merged_areas(s), vec![merged("H1", "I1")]);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_scoped_axis_successors_and_undo_keep_the_transformed_area() {
    use calc_engine::contract::CalcEvent;
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    for (address, text) in [
        ("Z1", "=1+1"),
        ("Z2", "=2+2"),
        ("Z3", "=3+3"),
        ("AA100", "=77"),
    ] {
        assert!(enter(&mut book, at(s, address), text).accepted());
    }
    book.set_calculation_budget(WorkBudget::evaluations(1));
    assert!(
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::new(at(s, "Z1"), at(s, "Z3")).unwrap()]
        })
        .accepted()
    );
    assert!(book.has_pending_calculation());
    assert!(
        book.dispatch(SkinIntent::EditAxis(
            AxisEdit::new(s, Axis::Rows, AxisChange::Insert, 1, 2).unwrap()
        ))
        .accepted()
    );
    assert!(
        book.dispatch(SkinIntent::EditAxis(
            AxisEdit::new(s, Axis::Columns, AxisChange::Insert, 1, 1).unwrap()
        ))
        .accepted()
    );
    let moved = GridArea::new(at(s, "AA3"), at(s, "AA5")).unwrap();
    for _ in 0..16 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    let events = book.take_calculation_events();
    let completion = events
        .iter()
        .find_map(|event| match event {
            CalcEvent::Completed(receipt)
                if receipt.outcome == calc_engine::contract::CompletionOutcome::Complete =>
            {
                Some(receipt)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(completion.scope, calc_engine::CalcScope::Range(vec![moved]));
    assert_eq!(completion.coverage.calculated, 3);
    for (address, expected) in [("AA3", 2.0), ("AA4", 4.0), ("AA5", 6.0)] {
        assert_eq!(
            calculated(book.host(), at(s, address)),
            Some(CalcValue::number(expected))
        );
    }
    assert!(
        calculated(book.host(), at(s, "AB102")).is_none(),
        "the successor cannot widen to a pending outside formula"
    );
    assert!(
        book.dispatch(SkinIntent::CalculateRange { areas: vec![moved] })
            .accepted()
    );
    assert!(book.has_pending_calculation());
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    for _ in 0..16 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    let expected = GridArea::new(at(s, "Z3"), at(s, "Z5")).unwrap();
    assert!(book.take_calculation_events().iter().any(|event| matches!(event,CalcEvent::Completed(receipt) if receipt.scope == calc_engine::CalcScope::Range(vec![expected]) && receipt.coverage.calculated == 3 && receipt.outcome == calc_engine::contract::CompletionOutcome::Complete)));
    assert!(calculated(book.host(), at(s, "AA102")).is_none());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_deleting_the_whole_active_scope_completes_empty_without_widening() {
    use calc_engine::contract::{CalcEvent, CompletionOutcome};
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    for (address, text) in [
        ("Z1", "=1+1"),
        ("Z2", "=2+2"),
        ("Z3", "=3+3"),
        ("X10", "=77"),
    ] {
        assert!(enter(&mut book, at(s, address), text).accepted());
    }
    book.set_calculation_budget(WorkBudget::evaluations(1));
    assert!(
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::new(at(s, "Z1"), at(s, "Z3")).unwrap()]
        })
        .accepted()
    );
    assert!(book.has_pending_calculation());
    assert!(
        book.dispatch(SkinIntent::EditAxis(
            AxisEdit::new(s, Axis::Rows, AxisChange::Delete, 1, 3).unwrap()
        ))
        .accepted()
    );
    for _ in 0..16 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    assert!(book.take_calculation_events().iter().any(|event| matches!(event,CalcEvent::Completed(receipt) if receipt.scope == calc_engine::CalcScope::Range(Vec::new()) && receipt.coverage.calculated == 0 && receipt.outcome == CompletionOutcome::Complete)));
    assert!(calculated(book.host(), at(s, "X7")).is_none());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_removing_an_active_sheet_scope_completes_empty() {
    use calc_engine::contract::{CalcEvent, CompletionOutcome};
    let mut book = session(&calc_host::blank_package());
    let original = book.host().sheets()[0].id();
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    assert!(enter(&mut book, at(original, "A1"), "=77").accepted());
    assert!(
        book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Sheet {
            sheet: None,
            change: calc_host::SheetChange::Add {
                name: "Scoped".into()
            }
        }))
        .accepted()
    );
    let s = sheet(book.host(), "Scoped");
    for address in ["A1", "A2", "A3"] {
        assert!(enter(&mut book, at(s, address), "=1+1").accepted());
    }
    book.set_calculation_budget(WorkBudget::evaluations(1));
    assert!(
        book.dispatch(SkinIntent::Calculate {
            scope: calc_engine::CalcScope::Sheet(s),
            selection: calc_engine::EvalSelection::ForceAll
        })
        .accepted()
    );
    assert!(book.has_pending_calculation());
    assert!(
        book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Sheet {
            sheet: Some(s),
            change: calc_host::SheetChange::Remove
        }))
        .accepted()
    );
    for _ in 0..16 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    assert!(book.take_calculation_events().iter().any(|event| matches!(event,CalcEvent::Completed(receipt) if receipt.scope == calc_engine::CalcScope::Range(Vec::new()) && receipt.coverage.calculated == 0 && receipt.outcome == CompletionOutcome::Complete)));
    assert!(calculated(book.host(), at(original, "A1")).is_none());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_prepared_save_keeps_its_model_and_results_after_the_live_host_changes() {
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    assert!(enter(&mut book, at(s, "A1"), "1").accepted());
    assert!(enter(&mut book, at(s, "B1"), "=A1+1").accepted());
    assert_eq!(
        calculated(book.host(), at(s, "B1")),
        Some(CalcValue::number(2.0))
    );
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
            .accepted()
    );
    assert!(enter(&mut book, at(s, "E1"), "=SUM(SEQUENCE(8192))").accepted());
    book.set_calculation_budget(WorkBudget::evaluations(1));
    assert!(
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::cell(at(s, "E1"))]
        })
        .accepted()
    );
    assert!(book.has_pending_calculation());
    book.take_calculation_events();
    let saved = book.prepare_save(&SavePolicy::default()).unwrap();
    let pin = saved.pin().clone();
    assert_eq!(pin.model, book.host().revision());
    assert!(pin.active_calculation.is_some());
    assert!(
        book.take_calculation_events().is_empty(),
        "pinning must not invoke or drain calculation"
    );
    assert!(book.has_pending_calculation());
    book.set_calculation_budget(WorkBudget::UNLIMITED);
    assert!(enter(&mut book, at(s, "A1"), "10").accepted());
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Automatic))
            .accepted()
    );
    for _ in 0..16 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    assert_eq!(
        calculated(book.host(), at(s, "B1")),
        Some(CalcValue::number(11.0))
    );
    assert_eq!(
        calculated(book.host(), at(s, "E1")),
        Some(CalcValue::number(33_558_528.0))
    );
    let live = book.prepare_save(&SavePolicy::default()).unwrap();
    assert_ne!(live.pin().model, pin.model);
    assert_ne!(live.pin().results, pin.results);
    drop(book);
    let mut bytes = Vec::new();
    saved.write(&mut bytes).unwrap();
    let reopened = open(&bytes);
    let s = reopened.sheets()[0].id();
    assert_eq!(reopened.model().cell(at(s, "A1")), Some(&number(1.0)));
    match value(&reopened, at(s, "B1")) {
        ProjectedValue::FileCached(cache) => {
            assert_eq!(cache.value(), &cached_of(&CalcValue::number(2.0)))
        }
        other => panic!("the pinned B1 cache reopened as {other:?}"),
    }
    assert!(
        !matches!(value(&reopened, at(s, "E1")), ProjectedValue::FileCached(_)),
        "a later live result must not enter the pinned save"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_p09_loaded_formula_forms_keep_their_measured_scalar_and_array_behavior() {
    // These existing probe packages hold Excel's measured source forms. Cached values are
    // observed as unverified on load; all expected outputs below are recalculated by the host.
    for (bytes, kind) in [
        (
            &include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P09/formula-01.xlsm"
            )[..],
            None,
        ),
        (
            &include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P09/formula2-01.xlsm"
            )[..],
            Some(calc_host::ArrayKind::Dynamic { collapsed: false }),
        ),
        (
            &include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P09/cse-01.xlsm")[..],
            Some(calc_host::ArrayKind::Legacy),
        ),
    ] {
        let mut book = session(bytes);
        let s = sheet(book.host(), "S");
        let b2 = at(s, "B2");
        assert_eq!(
            book.host()
                .model()
                .definitions()
                .array(b2)
                .map(|array| array.kind()),
            kind
        );
        assert!(
            book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
                .accepted()
        );
        assert!(
            book.dispatch(SkinIntent::CalculateRange {
                areas: vec![GridArea::new(b2, at(s, "B5")).unwrap()]
            })
            .accepted()
        );
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        assert_eq!(
            calculated(book.host(), b2),
            Some(CalcValue::number(if kind.is_none() { 2.0 } else { 1.0 }))
        );
        for (address, expected) in [("B3", 2.0), ("B4", 3.0), ("B5", 4.0)] {
            if kind.is_some() {
                assert_eq!(
                    calculated(book.host(), at(s, address)),
                    Some(CalcValue::number(expected)),
                    "{kind:?} {address}"
                );
            } else {
                assert!(matches!(
                    value(book.host(), at(s, address)),
                    ProjectedValue::Empty
                ));
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_manual_range_keeps_only_its_targets_required_for_the_next_f9() {
    use calc_engine::contract::{CalcEvent, ComputationOrigin, RangePostState};
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    for (address, text) in [("A1", "1"), ("B1", "=A1*2"), ("C1", "=B1+1"), ("K1", "=99")] {
        assert!(enter(&mut book, at(s, address), text).accepted());
    }
    let produced_by = |book: &HostSession, address: &str| {
        let ProjectedValue::Calculated { calculation, .. } = value(book.host(), at(s, address))
        else {
            panic!("expected a calculated {address}")
        };
        match &calculation.stamp().unwrap().computed_by {
            ComputationOrigin::Evaluated { root } => root.pass.command,
            _ => panic!("expected an evaluated origin"),
        }
    };
    let (b_before, k_before) = (produced_by(&book, "B1"), produced_by(&book, "K1"));
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
            .accepted()
    );
    book.take_calculation_events();
    assert!(
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::cell(at(s, "C1"))]
        })
        .accepted()
    );
    let range = produced_by(&book, "C1");
    let events = book.take_calculation_events();
    assert!(events.iter().any(|event|matches!(event,CalcEvent::Admitted(receipt) if receipt.range_post_state==RangePostState::StillRequired)));
    assert!(events.iter().any(|event|matches!(event,CalcEvent::Completed(receipt) if receipt.coverage.calculated==1 && receipt.coverage.boundary_cached==1 && receipt.outcome==calc_engine::contract::CompletionOutcome::Complete)));
    assert!(
        !book.has_pending_calculation(),
        "Manual waits for another authorized command"
    );
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    assert_ne!(
        produced_by(&book, "C1"),
        range,
        "F9 must evaluate the clean range target again"
    );
    assert_eq!(produced_by(&book, "B1"), b_before);
    assert_eq!(
        produced_by(&book, "K1"),
        k_before,
        "an unrelated clean formula does not run"
    );
    assert_eq!(
        calculated(book.host(), at(s, "C1")),
        Some(CalcValue::number(3.0))
    );
    assert!(book.take_calculation_events().iter().any(|event|matches!(event,CalcEvent::Completed(receipt) if receipt.coverage.calculated==1 && receipt.coverage.boundary_cached==0)));
    // Automatic range admission has a different post-state. Do not inherit the Manual option.
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Automatic))
            .accepted()
    );
    book.take_calculation_events();
    assert!(
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::cell(at(s, "C1"))]
        })
        .accepted()
    );
    let automatic_range = produced_by(&book, "C1");
    assert!(book.take_calculation_events().iter().any(|event|matches!(event,CalcEvent::Admitted(receipt) if receipt.range_post_state==RangePostState::Satisfied)));
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    assert_eq!(produced_by(&book, "C1"), automatic_range);
    assert!(book.take_calculation_events().iter().any(
        |event| matches!(event,CalcEvent::Completed(receipt) if receipt.coverage.calculated==0)
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_interest_publishes_the_short_cone_before_resuming_the_long_cell() {
    use calc_engine::contract::CalcEvent;
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual))
            .accepted()
    );
    for (address, text) in [
        ("A1", "=SUM(SEQUENCE(8192))"),
        ("B1", "=3"),
        ("C1", "=B1*2"),
    ] {
        assert!(enter(&mut book, at(s, address), text).accepted());
    }
    let (subscription, received) =
        subscribe(&mut book, Interest::Area(GridArea::cell(at(s, "A1"))));
    book.set_calculation_budget(WorkBudget::evaluations(1));
    let scope = GridArea::new(at(s, "A1"), at(s, "C1")).unwrap();
    assert!(
        book.dispatch(SkinIntent::CalculateRange { areas: vec![scope] })
            .accepted()
    );
    assert!(book.has_pending_calculation());
    book.take_calculation_events();
    assert!(
        book.dispatch(SkinIntent::SetInterest {
            subscription,
            interest: Interest::Area(GridArea::cell(at(s, "C1")))
        })
        .accepted()
    );
    let depth = book.history_depth();
    let first = book.advance_calculation();
    assert!(
        matches!(&first, HostProgress::Published(delta) if delta.completion().is_none() && delta.publication().unwrap().sequence == 1)
    );
    assert_eq!(
        calculated(book.host(), at(s, "B1")),
        Some(CalcValue::number(3.0))
    );
    assert!(calculated(book.host(), at(s, "C1")).is_none());
    let second = book.advance_calculation();
    assert!(
        matches!(&second, HostProgress::Published(delta) if delta.completion().is_none() && delta.publication().unwrap().sequence == 2)
    );
    assert_eq!(
        calculated(book.host(), at(s, "C1")),
        Some(CalcValue::number(6.0))
    );
    assert!(calculated(book.host(), at(s, "A1")).is_none());
    assert!(book.has_pending_calculation());
    assert!(
        !book
            .take_calculation_events()
            .iter()
            .any(|event| matches!(event, CalcEvent::Completed(_)))
    );
    assert!(received.borrow().iter().any(|delta| matches!(delta.cause, calc_host::DeltaCause::Recalculated { publication, .. } if publication.sequence == 2)
        && delta.cells.iter().any(|cell| cell.at() == at(s, "C1"))));
    for _ in 0..32 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    assert_eq!(book.history_depth(), depth);
    assert_eq!(
        calculated(book.host(), at(s, "A1")),
        Some(CalcValue::number(33_558_528.0))
    );
    let events = book.take_calculation_events();
    assert!(events.iter().any(|event| matches!(event, CalcEvent::Completed(receipt) if receipt.coverage.calculated == 3
        && receipt.after_publication.unwrap().sequence == 3 && receipt.scope == calc_engine::CalcScope::Range(vec![scope]))));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_prefix_units_survive_cancel_and_remain_the_terminal_barrier() {
    use calc_engine::contract::{CalcEvent, CompletionOutcome};
    for cancel in [true, false] {
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        for (address, text) in [
            ("A1", "=SUM(SEQUENCE(8192))"),
            ("B1", "=3"),
            ("C1", "=B1*2"),
        ] {
            assert!(enter(&mut book, at(s, address), text).accepted());
        }
        let (subscription, _) = subscribe(&mut book, Interest::Area(GridArea::cell(at(s, "A1"))));
        book.set_calculation_budget(WorkBudget::evaluations(1));
        assert!(
            book.dispatch(SkinIntent::CalculateRange {
                areas: vec![GridArea::new(at(s, "A1"), at(s, "C1")).unwrap()]
            })
            .accepted()
        );
        assert!(
            book.dispatch(SkinIntent::SetInterest {
                subscription,
                interest: Interest::Area(GridArea::cell(at(s, "C1")))
            })
            .accepted()
        );
        book.advance_calculation();
        book.advance_calculation();
        assert_eq!(
            calculated(book.host(), at(s, "C1")),
            Some(CalcValue::number(6.0))
        );
        book.take_calculation_events();
        if cancel {
            book.cancel_calculation();
        } else {
            assert!(enter(&mut book, at(s, "B1"), "4").accepted());
            book.advance_calculation();
        }
        let events = book.take_calculation_events();
        assert!(events.iter().any(|event| matches!(event, CalcEvent::Completed(receipt) | CalcEvent::Superseded(receipt)
            if (if cancel { receipt.outcome == CompletionOutcome::Cancelled } else { matches!(receipt.outcome, CompletionOutcome::Superseded { .. }) })
                && receipt.after_publication.is_some_and(|unit| unit.sequence == 2))));
        if cancel {
            assert!(!book.has_pending_calculation());
            assert_eq!(
                calculated(book.host(), at(s, "B1")),
                Some(CalcValue::number(3.0))
            );
            assert_eq!(
                calculated(book.host(), at(s, "C1")),
                Some(CalcValue::number(6.0))
            );
            assert!(calculated(book.host(), at(s, "A1")).is_none());
        } else {
            for _ in 0..32 {
                if !book.has_pending_calculation() {
                    break;
                }
                book.advance_calculation();
            }
            assert!(!book.has_pending_calculation());
            assert_eq!(
                calculated(book.host(), at(s, "C1")),
                Some(CalcValue::number(8.0))
            );
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_sheet_undo_restores_authored_cells_without_reviving_a_deleted_command_scope() {
    use calc_engine::contract::{CalcEvent, CompletionOutcome};
    let mut book = session(&calc_host::blank_package());
    let original = book.host().sheets()[0].id();
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    assert!(enter(&mut book, at(original, "A1"), "=77").accepted());
    assert!(
        book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Sheet {
            sheet: None,
            change: calc_host::SheetChange::Add {
                name: "RestoredScope".into()
            }
        }))
        .accepted()
    );
    let s = sheet(book.host(), "RestoredScope");
    for address in ["A1", "A2", "A3"] {
        assert!(enter(&mut book, at(s, address), "=2").accepted());
    }
    book.set_calculation_budget(WorkBudget::evaluations(1));
    assert!(
        book.dispatch(SkinIntent::Calculate {
            scope: calc_engine::CalcScope::Sheet(s),
            selection: calc_engine::EvalSelection::ForceAll
        })
        .accepted()
    );
    assert!(book.has_pending_calculation());
    assert!(
        book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Sheet {
            sheet: Some(s),
            change: calc_host::SheetChange::Remove
        }))
        .accepted()
    );
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(sheet(book.host(), "RestoredScope"), s);
    for _ in 0..16 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    assert!(book.take_calculation_events().iter().any(|event| matches!(event, CalcEvent::Completed(receipt)
        if receipt.scope == calc_engine::CalcScope::Range(vec![]) && receipt.coverage.calculated == 0 && receipt.outcome == CompletionOutcome::Complete)));
    for target in [at(original, "A1"), at(s, "A1")] {
        assert!(calculated(book.host(), target).is_none());
    }
    assert!(
        book.dispatch(SkinIntent::Calculate {
            scope: calc_engine::CalcScope::Sheet(s),
            selection: calc_engine::EvalSelection::ForceAll
        })
        .accepted()
    );
    for _ in 0..16 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    assert_eq!(
        calculated(book.host(), at(s, "A3")),
        Some(CalcValue::number(2.0))
    );
    assert!(calculated(book.host(), at(original, "A1")).is_none());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_sheet_undo_keeps_a_surviving_narrow_scope_without_workbook_fallback() {
    use calc_engine::contract::CalcEvent;
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    for (address, text) in [("A1", "=1+1"), ("A2", "=2+2"), ("B1", "=99")] {
        assert!(enter(&mut book, at(s, address), text).accepted());
    }
    assert!(
        book.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Sheet {
            sheet: None,
            change: calc_host::SheetChange::Add {
                name: "TemporaryScope".into()
            }
        }))
        .accepted()
    );
    let scope = GridArea::new(at(s, "A1"), at(s, "A2")).unwrap();
    book.set_calculation_budget(WorkBudget::evaluations(1));
    assert!(
        book.dispatch(SkinIntent::CalculateRange { areas: vec![scope] })
            .accepted()
    );
    assert!(book.has_pending_calculation());
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    for _ in 0..16 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    assert!(book.take_calculation_events().iter().any(|event| matches!(event, CalcEvent::Completed(receipt)
        if receipt.scope == calc_engine::CalcScope::Range(vec![scope]) && receipt.coverage.calculated == 2)));
    assert_eq!(
        calculated(book.host(), at(s, "A2")),
        Some(CalcValue::number(4.0))
    );
    assert!(calculated(book.host(), at(s, "B1")).is_none());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_save_pin_during_prefix_publication_keeps_only_its_installed_values() {
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    for (address, text) in [
        ("A1", "=SUM(SEQUENCE(8192))"),
        ("B1", "=3"),
        ("C1", "=B1*2"),
    ] {
        assert!(enter(&mut book, at(s, address), text).accepted());
    }
    let (subscription, _) = subscribe(&mut book, Interest::Area(GridArea::cell(at(s, "A1"))));
    book.set_calculation_budget(WorkBudget::evaluations(1));
    assert!(
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::new(at(s, "A1"), at(s, "C1")).unwrap()]
        })
        .accepted()
    );
    assert!(
        book.dispatch(SkinIntent::SetInterest {
            subscription,
            interest: Interest::Area(GridArea::cell(at(s, "C1")))
        })
        .accepted()
    );
    book.advance_calculation();
    book.advance_calculation();
    book.take_calculation_events();
    let saved = book.prepare_save(&SavePolicy::default()).unwrap();
    assert!(saved.pin().active_calculation.is_some());
    assert!(book.has_pending_calculation());
    assert!(book.take_calculation_events().is_empty());
    book.cancel_calculation();
    book.set_calculation_budget(WorkBudget::UNLIMITED);
    assert!(enter(&mut book, at(s, "B1"), "4").accepted());
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Automatic))
            .accepted()
    );
    assert_eq!(
        calculated(book.host(), at(s, "C1")),
        Some(CalcValue::number(8.0))
    );
    drop(book);
    let mut bytes = Vec::new();
    saved.write(&mut bytes).unwrap();
    let reopened = open(&bytes);
    let s = reopened.sheets()[0].id();
    for (address, expected) in [("B1", 3.0), ("C1", 6.0)] {
        let ProjectedValue::FileCached(cache) = value(&reopened, at(s, address)) else {
            panic!("a saved prefix value must reopen as an unverified cache");
        };
        assert_eq!(cache.value(), &cached_of(&CalcValue::number(expected)));
    }
    assert!(!matches!(
        value(&reopened, at(s, "A1")),
        ProjectedValue::FileCached(_) | ProjectedValue::Calculated { .. }
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_worker_sized_first_spill_reuse_retains_its_scope_origin_and_stale_qualification() {
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    for &(row, col, text, _) in seven_cell::CELLS {
        assert!(enter(&mut book, CellKey::new(s, row, col).unwrap(), text).accepted());
    }
    for (address, text) in [("A1", "2"), ("F1", "10")] {
        assert!(enter(&mut book, at(s, address), text).accepted());
    }
    let background = GridArea::new(at(s, "I1"), at(s, "I512")).unwrap();
    assert!(
        book.dispatch(SkinIntent::Select(calc_host::Selection {
            active: Some(at(s, "I1")),
            areas: vec![background]
        }))
        .accepted()
    );
    assert!(
        book.dispatch(SkinIntent::EnterSelection {
            text: "=ROW()".into()
        })
        .accepted()
    );
    subscribe(
        &mut book,
        Interest::Areas(vec![
            GridArea::new(at(s, "B1"), at(s, "E10")).unwrap(),
            GridArea::cell(at(s, "G10")),
        ]),
    );
    book.set_calculation_budget(WorkBudget::evaluations(64));
    let mut scope: Vec<_> = seven_cell::PURE_SCOPE
        .iter()
        .map(|&(row, col)| GridArea::cell(CellKey::new(s, row, col).unwrap()))
        .collect();
    scope.push(background);
    assert!(
        book.dispatch(SkinIntent::Calculate {
            scope: calc_engine::CalcScope::Range(scope),
            selection: calc_engine::EvalSelection::DirtyAndVolatile
        })
        .accepted()
    );
    assert!(book.has_pending_calculation());
    assert!(enter(&mut book, at(s, "A1"), "20").accepted());
    for _ in 0..64 {
        if !book.has_pending_calculation() {
            break;
        }
        book.advance_calculation();
    }
    assert!(!book.has_pending_calculation());
    assert!(
        book.host()
            .model()
            .definitions()
            .array(at(s, "D1"))
            .is_some(),
        "the application authors the unknown callback as an array producer"
    );
    for address in ["B1", "C1"] {
        assert!(matches!(
            value(book.host(), at(s, address)),
            ProjectedValue::Calculated { .. }
        ));
    }
    for (address, expected) in [("E1", 1.0), ("E10", 10.0), ("G10", 11.0)] {
        let ProjectedValue::Stale(held) = value(book.host(), at(s, address)) else {
            panic!(
                "the pending out-of-scope D1 producer keeps workbook-wide provenance conservative"
            );
        };
        assert_eq!(held.last_value(), Some(&CalcValue::number(expected)));
        let stamp = held.last_calculation().unwrap().stamp().unwrap();
        assert!(matches!(
            stamp.computed_by,
            calc_engine::contract::ComputationOrigin::Evaluated { .. }
        ));
        if address.starts_with('E') {
            let calc_engine::contract::ComputationOrigin::Evaluated { root } = &stamp.computed_by
            else {
                unreachable!()
            };
            assert_ne!(
                root.pass.command, stamp.certification.command,
                "the installed spill retains its original evaluated identity after reuse"
            );
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_interest_publishes_already_computed_cones_before_another_kernel_quantum() {
    use calc_engine::contract::CalcEvent;
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    for (address, text) in [
        ("A1", "=SUM(SEQUENCE(65536))"),
        ("B1", "=3"),
        ("C1", "=B1*2"),
    ] {
        assert!(enter(&mut book, at(s, address), text).accepted());
    }
    let (subscription, _) = subscribe(&mut book, Interest::Area(GridArea::cell(at(s, "A1"))));
    book.set_calculation_budget(WorkBudget::evaluations(16));
    assert!(
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::new(at(s, "A1"), at(s, "C1")).unwrap()]
        })
        .accepted()
    );
    assert!(book.has_pending_calculation());
    assert!(calculated(book.host(), at(s, "C1")).is_none());
    assert!(
        book.dispatch(SkinIntent::SetInterest {
            subscription,
            interest: Interest::Area(GridArea::cell(at(s, "C1")))
        })
        .accepted()
    );
    book.take_calculation_events();
    let progress = book.advance_calculation();
    assert!(
        matches!(progress, HostProgress::Published(delta) if delta.completion().is_none() && delta.changes().len() == 2)
    );
    assert!(book.has_pending_calculation());
    assert_eq!(
        calculated(book.host(), at(s, "C1")),
        Some(CalcValue::number(6.0))
    );
    assert!(
        !book
            .take_calculation_events()
            .iter()
            .any(|event| matches!(event, CalcEvent::Yielded { .. })),
        "promotion publishes the ready cone without another evaluation slice"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_p06_manual_save_preserves_permitted_scalar_caches_with_dirty_metadata() {
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    for (address, text) in [("A1", "1"), ("B1", "=A1*2"), ("C1", "=B1+1")] {
        assert!(enter(&mut book, at(s, address), text).accepted());
    }
    assert_eq!(
        calculated(book.host(), at(s, "C1")),
        Some(CalcValue::number(3.0))
    );
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    assert!(enter(&mut book, at(s, "A1"), "5").accepted());
    book.take_calculation_events();
    let saved = book.prepare_save(&SavePolicy::default()).unwrap();
    assert_eq!(
        saved.pin().cache_policy,
        calc_host::SaveCachePolicy::PreservePermittedCaches
    );
    assert!(book.take_calculation_events().is_empty());
    let mut strict = Vec::new();
    calc_host::prepare_save_with_cache_policy(
        book.host(),
        &SavePolicy::default(),
        calc_host::SaveCachePolicy::CurrentCalculatedOnly,
    )
    .unwrap()
    .write(&mut strict)
    .unwrap();
    assert!(!matches!(
        value(&open(&strict), at(s, "B1")),
        ProjectedValue::FileCached(_)
    ));
    assert!(enter(&mut book, at(s, "A1"), "7").accepted());
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Automatic));
    assert_eq!(
        calculated(book.host(), at(s, "C1")),
        Some(CalcValue::number(15.0))
    );
    drop(book);
    let mut bytes = Vec::new();
    saved.write(&mut bytes).unwrap();
    let reopened = open(&bytes);
    let s = reopened.sheets()[0].id();
    assert_eq!(reopened.calc_mode(), CalcMode::Manual);
    assert_eq!(reopened.model().cell(at(s, "A1")), Some(&number(5.0)));
    for (address, expected) in [("B1", 2.0), ("C1", 3.0)] {
        let ProjectedValue::FileCached(cache) = value(&reopened, at(s, address)) else {
            panic!("retained caches must reopen as unverified file values");
        };
        assert_eq!(cache.value(), &cached_of(&CalcValue::number(expected)));
    }
    let workbook = part_text(&bytes, "xl/workbook.xml");
    assert!(
        workbook.contains("calcCompleted=\"0\"")
            && workbook.contains("calcOnSave=\"0\"")
            && workbook.contains("calcMode=\"manual\"")
    );
    let sheet_xml = part_text(&bytes, "xl/worksheets/sheet1.xml");
    assert_eq!(sheet_xml.matches("ca=\"1\"").count(), 2);
    let mut normalized = session(&bytes);
    assert!(normalized.dispatch(SkinIntent::Recalculate).accepted());
    let mut current = Vec::new();
    normalized
        .save(&SavePolicy::default(), &mut current)
        .unwrap();
    assert_eq!(
        part_text(&current, "xl/worksheets/sheet1.xml")
            .matches("ca=\"1\"")
            .count(),
        0,
        "calculating imported held caches clears their old dirty markers"
    );
    assert!(part_text(&current, "xl/workbook.xml").contains("calcCompleted=\"1\""));
    let mut imported = HostSession::new(reopened);
    let s = imported.host().sheets()[0].id();
    assert!(enter(&mut imported, at(s, "A1"), "9").accepted());
    let mut again = Vec::new();
    imported.save(&SavePolicy::default(), &mut again).unwrap();
    let again = open(&again);
    let s = again.sheets()[0].id();
    let ProjectedValue::FileCached(cache) = value(&again, at(s, "B1")) else {
        panic!("the imported held cache should survive without evaluation");
    };
    assert_eq!(cache.value(), &cached_of(&CalcValue::number(2.0)));
    // Re-entry, even with identical spelling, cannot attach an orphaned old cache to a new node.
    assert!(enter(&mut imported, at(s, "B1"), "=A1*2").accepted());
    let mut reentered = Vec::new();
    imported
        .save(&SavePolicy::default(), &mut reentered)
        .unwrap();
    let reentered = open(&reentered);
    let s = reentered.sheets()[0].id();
    assert!(!matches!(
        value(&reentered, at(s, "B1")),
        ProjectedValue::FileCached(_) | ProjectedValue::Calculated { .. }
    ));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_p06_excel_saved_manual_cache_image_keeps_its_values_without_vba_entry() {
    let bytes = include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P06/manual-false-save-off-01-tested.xlsm"
    );
    let mut book = session(bytes);
    let s = sheet(book.host(), "S");
    assert_eq!(book.host().calc_mode(), CalcMode::Manual);
    assert_eq!(book.host().model().cell(at(s, "A1")), Some(&number(5.0)));
    assert!(enter(&mut book, at(s, "A1"), "9").accepted());
    book.take_calculation_events();
    let pin = book.prepare_save(&SavePolicy::default()).unwrap();
    assert!(book.take_calculation_events().is_empty());
    let mut saved = Vec::new();
    pin.write(&mut saved).unwrap();
    let reopened = open(&saved);
    let s = sheet(&reopened, "S");
    for (address, expected) in [("B1", 2.0), ("C1", 3.0)] {
        let ProjectedValue::FileCached(cache) = value(&reopened, at(s, address)) else {
            panic!("an Excel saved held cache remains unverified");
        };
        assert_eq!(cache.value(), &cached_of(&CalcValue::number(expected)));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_p02_manual_range_followup_demand_survives_the_p06_save_metadata() {
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    for (address, text) in [("A1", "1"), ("B1", "=A1*2"), ("C1", "=B1+1")] {
        assert!(enter(&mut book, at(s, address), text).accepted());
    }
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    assert!(
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::cell(at(s, "C1"))]
        })
        .accepted()
    );
    assert_eq!(
        calculated(book.host(), at(s, "C1")),
        Some(CalcValue::number(3.0))
    );
    let mut saved = Vec::new();
    book.save(&SavePolicy::default(), &mut saved).unwrap();
    assert_eq!(
        part_text(&saved, "xl/worksheets/sheet1.xml")
            .matches("ca=\"1\"")
            .count(),
        1,
        "the current C1 value still carries its next-F9 obligation"
    );
    assert!(part_text(&saved, "xl/workbook.xml").contains("calcCompleted=\"0\""));
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    let mut after_f9 = Vec::new();
    book.save(&SavePolicy::default(), &mut after_f9).unwrap();
    assert_eq!(
        part_text(&after_f9, "xl/worksheets/sheet1.xml")
            .matches("ca=\"1\"")
            .count(),
        0
    );
    assert!(part_text(&after_f9, "xl/workbook.xml").contains("calcCompleted=\"1\""));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_deleted_cell_shift_scope_does_not_revive_on_undo() {
    use calc_engine::contract::{CalcEvent, CalcScope};
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    for address in ["A1", "A2", "A3"] {
        assert!(enter(&mut book, at(s, address), "=2").accepted());
    }
    book.set_calculation_budget(WorkBudget::evaluations(1));
    let area = GridArea::new(at(s, "A1"), at(s, "A3")).unwrap();
    assert!(
        book.dispatch(SkinIntent::CalculateRange { areas: vec![area] })
            .accepted()
    );
    book.take_calculation_events();
    assert!(
        book.dispatch(SkinIntent::Command(
            calc_host::WorkbookCommand::ShiftCells {
                area,
                down: true,
                insert: false
            }
        ))
        .accepted()
    );
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    book.advance_calculation();
    let events = book.take_calculation_events();
    assert!(events.iter().any(|event| matches!(event, CalcEvent::Completed(c) if c.scope == CalcScope::Range(vec![]) && c.coverage.required == 0)));
    for address in ["A1", "A2", "A3"] {
        assert!(calculated(book.host(), at(s, address)).is_none());
    }
    assert!(
        book.dispatch(SkinIntent::CalculateRange { areas: vec![area] })
            .accepted()
    );
    while book.has_pending_calculation() {
        book.advance_calculation();
    }
    for address in ["A1", "A2", "A3"] {
        assert_eq!(
            calculated(book.host(), at(s, address)),
            Some(CalcValue::number(2.0))
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_product_range_reads_imported_array_children_without_calculating_the_anchor() {
    let mut book = session(&fixture());
    let s = sheet(book.host(), "Calc");
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    assert!(enter(&mut book, at(s, "Z1"), "=B2+1").accepted());
    assert!(enter(&mut book, at(s, "Z2"), "=ROWS(A1#)").accepted());
    assert!(
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::new(at(s, "Z1"), at(s, "Z2")).unwrap()]
        })
        .accepted()
    );
    while book.has_pending_calculation() {
        book.advance_calculation();
    }
    for (address, expected) in [("Z1", 5.0), ("Z2", 3.0)] {
        let ProjectedValue::Stale(held) = value(book.host(), at(s, address)) else {
            panic!("imported inputs cannot establish workbook freshness");
        };
        assert_eq!(held.last_value(), Some(&CalcValue::number(expected)));
        let stamp = held.last_calculation().unwrap().stamp().unwrap();
        assert!(stamp.reads.uses_imported_cache && stamp.reads.uses_boundary_cache);
    }
    let ProjectedValue::Stale(held) = value(book.host(), at(s, "A1")) else {
        panic!("the excluded anchor keeps its unverified file cache");
    };
    assert!(held.file_cache().is_some());
    assert!(held.last_calculation().is_none());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_paste_cut_drops_overwritten_scope_locations_before_undo() {
    use calc_engine::contract::{CalcEvent, CalcScope};
    use calc_host::{CopySnapshot, PasteMode, PasteSource, WorkbookCommand, prepare_paste};
    for overlapping in [false, true] {
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        for (address, text) in [
            ("A1", "=1"),
            ("B1", "=2"),
            ("C1", "=3"),
            ("D1", "=4"),
            ("E1", "=5"),
        ] {
            assert!(enter(&mut book, at(s, address), text).accepted());
        }
        let source = GridArea::new(at(s, "A1"), at(s, "B1")).unwrap();
        let destination = at(s, if overlapping { "B1" } else { "D1" });
        let overwritten = at(s, if overlapping { "C1" } else { "D1" });
        let scope = vec![GridArea::cell(overwritten), GridArea::cell(at(s, "E1"))];
        book.set_calculation_budget(WorkBudget::evaluations(1));
        assert!(
            book.dispatch(SkinIntent::CalculateRange { areas: scope })
                .accepted()
        );
        assert!(book.has_pending_calculation());
        book.take_calculation_events();
        let snapshot = CopySnapshot::capture(book.host(), source).unwrap();
        let plan = prepare_paste(
            book.host(),
            PasteSource::Snapshot {
                snapshot: &snapshot,
                cut: true,
            },
            GridArea::cell(destination),
            PasteMode::All,
        )
        .unwrap();
        assert!(
            book.dispatch(SkinIntent::Command(WorkbookCommand::Paste(plan)))
                .accepted()
        );
        assert!(book.dispatch(SkinIntent::Undo).accepted());
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        let expected = if overlapping {
            CalcScope::Range(vec![GridArea::cell(at(s, "E1"))])
        } else {
            CalcScope::Range(vec![])
        };
        assert!(
            book.take_calculation_events()
                .iter()
                .any(|e| matches!(e,CalcEvent::Completed(c) if c.scope == expected))
        );
        for address in ["A1", "B1", "C1"] {
            assert!(calculated(book.host(), at(s, address)).is_none());
        }
    }
}

fn package_with_names(names: &str) -> Vec<u8> {
    use existing_wire_support::{RawEntry, build_zip, parse_zip};
    let blank = calc_host::blank_package();
    let archive = parse_zip(&blank);
    let entries: Vec<_> = archive
        .entries
        .iter()
        .map(|entry| {
            let payload = entry.payload(&blank);
            if entry.name == "xl/workbook.xml" {
                let bytes = if entry.method == 0 {
                    payload.to_vec()
                } else {
                    miniz_oxide::inflate::decompress_to_vec(payload).unwrap()
                };
                let xml = String::from_utf8(bytes).unwrap().replace(
                    "</sheets>",
                    &format!("</sheets><definedNames>{names}</definedNames>"),
                );
                RawEntry::stored(&entry.name, xml.as_bytes())
            } else if entry.method == 0 {
                RawEntry::stored(&entry.name, payload)
            } else {
                RawEntry::deflated(&entry.name, payload, entry.size, entry.crc32)
            }
        })
        .collect();
    build_zip(&entries)
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn names_batch_delete_preserves_imported_duplicate_fallback() {
    let mut book = session(&package_with_names(
        r#"<definedName name="Rate">2</definedName><definedName name="RATE">7</definedName>"#,
    ));
    let original = book.host().defined_names().to_vec();
    let sheet = book.host().sheets()[0].id();
    let a1 = CellKey::new(sheet, 1, 1).unwrap();
    assert_eq!(book.host().model().definitions().name_table().len(), 2);
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![
                CellEdit::SetName {
                    name: "Rate".into(),
                    scope: calc_doc_model::NameScope::Workbook,
                    formula: None,
                },
                CellEdit::Set {
                    at: a1,
                    input: formula("Rate+1")
                }
            ]
        })
        .accepted()
    );
    while book.has_pending_calculation() {
        book.advance_calculation();
    }
    assert_eq!(calculated(book.host(), a1), Some(CalcValue::number(8.0)));
    let calc_doc_model::CellInput::Formula(text) = book.host().model().cell(a1).unwrap() else {
        panic!("a formula")
    };
    assert_eq!(text.text().to_string_lossy(), "Rate+1");
    assert_eq!(book.host().model().definitions().name_table().len(), 1);
    for _ in 0..2 {
        assert!(book.dispatch(SkinIntent::Undo).accepted());
        assert_eq!(book.host().defined_names(), original.as_slice());
        assert!(book.dispatch(SkinIntent::Redo).accepted());
        assert_eq!(calculated(book.host(), a1), Some(CalcValue::number(8.0)));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_vba_cached_reads_keep_their_view_and_create_no_hidden_dependency() {
    let mut backends = vec![calc_vba::Backend::Interpreter];
    #[cfg(not(target_arch = "wasm32"))]
    backends.push(calc_vba::Backend::Jit);
    for backend in backends {
        let source = "Private held As Range\nPublic Function Hold(ByVal r As Range) As Double\n Set held=r\n Hold=1#\nEnd Function\nPublic Function PeekB1() As Double\n PeekB1=held.Value2\nEnd Function\n";
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        book.set_user_functions(
            calc_vba::load_module_with_backend("Cached", source, 100_000, backend).unwrap(),
        );
        for (address, text) in [
            ("A1", "1"),
            ("B1", "=A1+1"),
            ("D1", "=PeekB1()"),
            ("Z1", "=Hold(B1)"),
            ("E1", "=D1+1"),
        ] {
            enter(&mut book, at(s, address), text);
        }
        let force = |book: &mut HostSession, address: &str| {
            let receipt = book.dispatch(SkinIntent::CalculateRange {
                areas: vec![GridArea::cell(at(s, address))],
            });
            assert!(receipt.accepted(), "{receipt:?}");
            while book.has_pending_calculation() {
                book.advance_calculation();
            }
        };
        let last = |book: &HostSession, address: &str| match value(book.host(), at(s, address)) {
            ProjectedValue::Calculated { value, .. } => Some(value),
            ProjectedValue::Stale(held) => held.last_value().cloned(),
            _ => None,
        };
        force(&mut book, "B1");
        force(&mut book, "Z1");
        force(&mut book, "D1");
        assert_eq!(last(&book, "D1"), Some(CalcValue::number(2.0)));
        enter(&mut book, at(s, "A1"), "10");
        book.take_calculation_events();
        force(&mut book, "D1");
        assert_eq!(last(&book, "D1"), Some(CalcValue::number(2.0)));
        assert_eq!(last(&book, "B1"), Some(CalcValue::number(2.0)));
        let stamp = match value(book.host(), at(s, "D1")) {
            ProjectedValue::Calculated { calculation, .. } => calculation.stamp().unwrap().clone(),
            ProjectedValue::Stale(held) => {
                held.last_calculation().unwrap().stamp().unwrap().clone()
            }
            other => panic!("{other:?}"),
        };
        assert!(
            stamp
                .reads
                .dependencies
                .iter()
                .all(|read| !read.region.contains(at(s, "B1")))
        );
        assert_eq!(stamp.reads.observations.len(), 1);
        assert_eq!(
            stamp.reads.observations[0].region,
            GridArea::cell(at(s, "B1"))
        );
        assert_eq!(
            stamp.reads.observations[0].request.policy,
            calc_fml::ReadPolicy::CachedMacroObservation
        );
        assert!(stamp.reads.observations[0].lineage.as_ref().unwrap().dirty > 0);
        let old_view = stamp.reads.observations[0].view.clone();
        assert!(book.take_calculation_events().iter().all(
            |event| !matches!(event,calc_engine::CalcEvent::Completed(c) if c.coverage.required>1)
        ));
        force(&mut book, "E1");
        let e = match value(book.host(), at(s, "E1")) {
            ProjectedValue::Calculated { calculation, .. } => calculation,
            ProjectedValue::Stale(held) => held.last_calculation().unwrap().clone(),
            other => panic!("{other:?}"),
        };
        assert!(
            e.stamp()
                .unwrap()
                .reads
                .observations
                .iter()
                .any(|read| read.view == old_view)
        );
        force(&mut book, "B1");
        assert_eq!(last(&book, "B1"), Some(CalcValue::number(11.0)));
        assert_eq!(
            last(&book, "D1"),
            Some(CalcValue::number(2.0)),
            "publishing the hidden producer never invokes its observer"
        );
        force(&mut book, "D1");
        assert_eq!(last(&book, "D1"), Some(CalcValue::number(11.0)));
        assert_eq!(old_view.frontier().model, stamp.observed.model);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_vba_vm_slices_resume_the_same_counted_cached_read_in_vm_and_jit() {
    let mut backends = vec![calc_vba::Backend::Interpreter];
    #[cfg(not(target_arch = "wasm32"))]
    backends.push(calc_vba::Backend::Jit);
    for backend in backends {
        let source = "Private entries As Long\nPublic Function CountAndPeek(ByVal r As DnaCalcObjects.Range,ByVal count As Long) As Double\n Dim total As Long\n Dim initial As Long\n initial=count\n entries=entries+1\n Do While count>0\n total=total+1\n count=count-1\n Loop\n CountAndPeek=r.NumericValue+total-initial\nEnd Function\nPublic Function EntryCount() As Long\n EntryCount=entries\nEnd Function\n";
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        book.set_user_functions(
            calc_vba::load_module_with_backend("Counted", source, 100_000, backend).unwrap(),
        );
        for (address, text) in [
            ("A1", "1"),
            ("B1", "=A1+1"),
            ("D1", "=CountAndPeek(B1,2000)"),
            ("E1", "=EntryCount()"),
        ] {
            enter(&mut book, at(s, address), text);
        }
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::cell(at(s, "B1"))],
        });
        enter(&mut book, at(s, "A1"), "10");
        book.take_calculation_events();
        book.set_calculation_budget(WorkBudget::evaluations(1));
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::cell(at(s, "D1"))],
        });
        assert!(
            book.has_pending_calculation(),
            "{backend:?}: must pause inside the procedure"
        );
        let mut slices = 0;
        while book.has_pending_calculation() {
            book.advance_calculation();
            slices += 1;
            assert!(slices < 1000);
        }
        assert!(slices > 3);
        let calculation = match value(book.host(), at(s, "D1")) {
            ProjectedValue::Calculated { value, calculation } => {
                assert_eq!(value, CalcValue::number(2.0));
                calculation
            }
            ProjectedValue::Stale(held) => {
                assert_eq!(held.last_value(), Some(&CalcValue::number(2.0)));
                held.last_calculation().unwrap().clone()
            }
            other => panic!("{backend:?}: {other:?}"),
        };
        let stamp = calculation.stamp().unwrap();
        assert_eq!(stamp.reads.invocations.len(), 1);
        assert!(stamp.reads.invocations[0].completed);
        assert!(
            !stamp.reads.invocations[0].non_preemptible,
            "{backend:?}: actual owning VM/JIT activation required"
        );
        assert_eq!(
            stamp.reads.invocations[0].identity.pass,
            stamp.certification.pass
        );
        assert_ne!(
            Some(stamp.reads.invocations[0].runtime),
            stamp.reads.invocations[0].parent
        );
        assert!(stamp.reads.dependencies.is_empty());
        assert_eq!(stamp.reads.observations.len(), 1);
        assert!(
            book.take_calculation_events()
                .iter()
                .any(|event| matches!(event, calc_engine::CalcEvent::Yielded { .. }))
        );
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::cell(at(s, "E1"))],
        });
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        assert_eq!(
            calculated(book.host(), at(s, "E1")),
            Some(CalcValue::number(1.0))
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_vba_stored_ranges_track_unions_and_never_revive_after_delete_or_reopen() {
    let mut backends = vec![calc_vba::Backend::Interpreter];
    #[cfg(not(target_arch = "wasm32"))]
    backends.push(calc_vba::Backend::Jit);
    for backend in backends {
        let source = "Private kept As Range\nPublic Function Hold(ByVal r As Range) As Long\n Set kept=r\n Hold=r.Count\nEnd Function\nPublic Function WhereHeld() As String\n On Error GoTo Gone\n WhereHeld=kept.Address\n Exit Function\nGone:\n WhereHeld=\"error\" & CStr(Err.Number)\nEnd Function\nPublic Function ReadHeld()\n On Error GoTo Gone\n ReadHeld=kept.Cells(1,1).Value2\n Exit Function\nGone:\n ReadHeld=Err.Number\nEnd Function\nPublic Function AreaCount() As Long\n AreaCount=kept.Areas.Count\nEnd Function\nPublic Function SecondArea()\n SecondArea=kept.Areas.Item(2).Value2\nEnd Function\nPublic Function StillObject() As Boolean\n StillObject=IsObject(kept)\nEnd Function\n";
        // The caller-owned provider intentionally survives the first workbook, like a
        // controller add-in. Its stored Range still belongs to the first incarnation.
        let functions =
            calc_vba::load_module_with_backend("Stored", source, 100_000, backend).unwrap();
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        book.set_user_functions(functions.clone());
        enter(&mut book, at(s, "A2"), "7");
        enter(&mut book, at(s, "C4"), "9");
        let evaluate = |book: &mut HostSession, formula: &str| {
            enter(book, at(s, "H100"), formula);
            let receipt = book.dispatch(SkinIntent::CalculateRange {
                areas: vec![GridArea::cell(at(s, "H100"))],
            });
            assert!(receipt.accepted(), "{receipt:?}");
            while book.has_pending_calculation() {
                book.advance_calculation();
            }
            match value(book.host(), at(s, "H100")) {
                ProjectedValue::Calculated { value, .. } => value,
                ProjectedValue::Stale(held) => held
                    .last_value()
                    .unwrap_or_else(|| panic!("{formula}: {held:?}"))
                    .clone(),
                other => panic!("{formula}: {other:?}"),
            }
        };
        let text = |value: &str| {
            CalcValue::text(calc_host::value::ExcelText::from_utf16_code_units(
                value.encode_utf16().collect(),
            ))
        };
        assert_eq!(
            evaluate(&mut book, "=Hold((A2,C4))"),
            CalcValue::number(2.0)
        );
        assert_eq!(evaluate(&mut book, "=AreaCount()"), CalcValue::number(2.0));
        assert_eq!(evaluate(&mut book, "=SecondArea()"), CalcValue::number(9.0));
        assert_eq!(evaluate(&mut book, "=WhereHeld()"), text("$A$2,$C$4"));
        assert!(
            book.dispatch(SkinIntent::EditAxis(
                AxisEdit::new(s, Axis::Rows, AxisChange::Insert, 1, 1).unwrap()
            ))
            .accepted()
        );
        assert_eq!(evaluate(&mut book, "=WhereHeld()"), text("$A$3,$C$5"));
        assert_eq!(evaluate(&mut book, "=ReadHeld()"), CalcValue::number(7.0));
        assert!(
            book.dispatch(SkinIntent::Copy {
                from: GridArea::cell(at(s, "A3")),
                to: GridArea::cell(at(s, "D6")),
                cut: true
            })
            .accepted()
        );
        assert_eq!(evaluate(&mut book, "=WhereHeld()"), text("$D$6,$C$5"));
        assert_eq!(evaluate(&mut book, "=ReadHeld()"), CalcValue::number(7.0));
        // Calculation has no history entry. Remove the two query authoring steps before
        // undoing the cut, then redo it; the retained object follows both coordinates.
        for _ in 0..3 {
            assert!(book.dispatch(SkinIntent::Undo).accepted());
        }
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::cell(at(s, "H100"))],
        });
        assert_eq!(
            calculated(book.host(), at(s, "H100")),
            Some(CalcValue::number(7.0))
        );
        assert!(book.dispatch(SkinIntent::Redo).accepted());
        assert_eq!(evaluate(&mut book, "=WhereHeld()"), text("$D$6,$C$5"));
        assert_eq!(evaluate(&mut book, "=Hold(D6)"), CalcValue::number(1.0));
        assert!(
            book.dispatch(SkinIntent::EditAxis(
                AxisEdit::new(s, Axis::Rows, AxisChange::Delete, 6, 1).unwrap()
            ))
            .accepted()
        );
        assert_eq!(evaluate(&mut book, "=ReadHeld()"), CalcValue::number(424.0));
        assert_eq!(
            evaluate(&mut book, "=StillObject()"),
            CalcValue::logical(true)
        );
        for _ in 0..3 {
            assert!(book.dispatch(SkinIntent::Undo).accepted());
        }
        assert_eq!(
            evaluate(&mut book, "=ReadHeld()"),
            CalcValue::number(424.0),
            "Undo never resurrects a deleted Range identity"
        );
        assert_eq!(evaluate(&mut book, "=Hold(D6)"), CalcValue::number(1.0));
        assert_eq!(evaluate(&mut book, "=ReadHeld()"), CalcValue::number(7.0));
        drop(book);
        let mut reopened = session(&calc_host::blank_package());
        assert_eq!(
            reopened.host().sheets()[0].id(),
            s,
            "sheet IDs can coincide across workbook incarnations"
        );
        reopened.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        reopened.set_user_functions(functions);
        enter(&mut reopened, at(s, "D6"), "99");
        assert_eq!(
            evaluate(&mut reopened, "=StillObject()"),
            CalcValue::logical(true)
        );
        assert_eq!(
            evaluate(&mut reopened, "=ReadHeld()"),
            CalcValue::number(424.0),
            "a same-name reopen cannot retarget a stored Range"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_vba_reload_revokes_live_publication_and_keeps_old_prefix_effects_once() {
    let mut backends = vec![calc_vba::Backend::Interpreter];
    #[cfg(not(target_arch = "wasm32"))]
    backends.push(calc_vba::Backend::Jit);
    for backend in backends {
        let source = "Private entries As Long\nPublic Function Counted(ByVal r As DnaCalcObjects.Range,ByVal count As Long) As Double\n Dim total As Long\n entries=entries+1\n Do While count>0\n total=total+1\n count=count-1\n Loop\n Counted=r.NumericValue+total\nEnd Function\nPublic Function EntryCount() As Long\n EntryCount=entries\nEnd Function\n";
        let functions =
            calc_vba::load_module_with_backend("Reload", source, 100_000, backend).unwrap();
        let old_counter = functions
            .resolve(functions.bind(None, "EntryCount").unwrap())
            .unwrap()
            .clone();
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        book.set_user_functions(functions);
        enter(&mut book, at(s, "A1"), "2");
        enter(&mut book, at(s, "D1"), "=Counted(A1,2000)");
        book.set_calculation_budget(WorkBudget::evaluations(1));
        book.take_calculation_events();
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::cell(at(s, "D1"))],
        });
        assert!(book.has_pending_calculation());
        let old = book
            .take_calculation_events()
            .into_iter()
            .find_map(|event| match event {
                calc_engine::CalcEvent::Admitted(a) => Some(a.command),
                _ => None,
            })
            .unwrap();
        let replacement = "Private entries As Long\nPublic Function Counted(ByVal value As Double,ByVal ignored As Long) As Double\n entries=entries+1\n Counted=value+50#\nEnd Function\nPublic Function EntryCount() As Long\n EntryCount=entries\nEnd Function\n";
        book.set_user_functions(
            calc_vba::load_module_with_backend("Reload", replacement, 100_000, backend).unwrap(),
        );
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        assert_eq!(old_counter.call(&[]).unwrap(), CalcValue::number(1.0));
        let (number, calculation) = match value(book.host(), at(s, "D1")) {
            ProjectedValue::Calculated { value, calculation } => (value, calculation),
            other => panic!("{other:?}"),
        };
        assert_eq!(number, CalcValue::number(52.0));
        let invocation = &calculation.stamp().unwrap().reads.invocations[0];
        assert_eq!(
            invocation.identity.reason,
            calc_engine::contract::ReevaluationReason::CallableReload
        );
        assert_ne!(invocation.identity.pass.command, old);
        let events = book.take_calculation_events();
        assert!(events.iter().any(|event|matches!(event,calc_engine::CalcEvent::Superseded(terminal) if terminal.command==old)));
        assert!(
            !events.iter().any(
                |event| matches!(event,calc_engine::CalcEvent::Completed(c) if c.command==old)
            )
        );
        enter(&mut book, at(s, "E1"), "=EntryCount()");
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::cell(at(s, "E1"))],
        });
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        assert_eq!(
            calculated(book.host(), at(s, "E1")),
            Some(CalcValue::number(1.0))
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_vba_typed_range_survives_variant_inspection_and_materialization() {
    let mut backends = vec![calc_vba::Backend::Interpreter];
    #[cfg(not(target_arch = "wasm32"))]
    backends.push(calc_vba::Backend::Jit);
    for backend in backends {
        let source = "Private kept As DnaCalcObjects.Range\nPublic Function HoldTyped(ByVal r As DnaCalcObjects.Range) As Long\n Set kept=r\n HoldTyped=1\nEnd Function\nPublic Function InspectHeld() As String\n InspectHeld=TypeName(kept)\nEnd Function\nPublic Function FetchHeld()\n Set FetchHeld=kept\nEnd Function\n";
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        book.set_user_functions(
            calc_vba::load_module_with_backend("Mixed", source, 100_000, backend).unwrap(),
        );
        for (address, text) in [
            ("A1", "7"),
            ("B1", "=HoldTyped(A1)"),
            ("C1", "=InspectHeld()"),
            ("D1", "=FetchHeld()"),
        ] {
            enter(&mut book, at(s, address), text);
        }
        for (address, expected) in [
            ("B1", CalcValue::number(1.0)),
            (
                "C1",
                CalcValue::text(calc_host::value::ExcelText::from_interop_assignment(
                    "Range",
                )),
            ),
            ("D1", CalcValue::number(7.0)),
        ] {
            book.dispatch(SkinIntent::CalculateRange {
                areas: vec![GridArea::cell(at(s, address))],
            });
            while book.has_pending_calculation() {
                book.advance_calculation();
            }
            let actual = match value(book.host(), at(s, address)) {
                ProjectedValue::Calculated { value, .. } => Some(value),
                ProjectedValue::Stale(held) => held.last_value().cloned(),
                _ => None,
            };
            assert_eq!(actual, Some(expected), "{backend:?}: {address}");
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_vba_volatile_spill_sizing_and_skipped_calls_use_executed_effects() {
    let mut backends = vec![calc_vba::Backend::Interpreter];
    #[cfg(not(target_arch = "wasm32"))]
    backends.push(calc_vba::Backend::Jit);
    for backend in backends {
        for volatile in [true, false] {
            let source = format!(
                "Private entries As Long\nPublic Function SizeUdf() As Long\n Application.Volatile {}\n entries=entries+1\n SizeUdf=2+(entries Mod 2)\nEnd Function\nPublic Function EntryCount() As Long\n EntryCount=entries\nEnd Function\n",
                if volatile { "True" } else { "False" }
            );
            let mut book = session(&calc_host::blank_package());
            let s = book.host().sheets()[0].id();
            book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
            book.set_user_functions(
                calc_vba::load_module_with_backend("Sizing", &source, 100_000, backend).unwrap(),
            );
            enter(&mut book, at(s, "A1"), "=_xlfn.SEQUENCE(SizeUdf())");
            enter(&mut book, at(s, "H1"), "=EntryCount()");
            for address in ["A1", "H1"] {
                book.dispatch(SkinIntent::CalculateRange {
                    areas: vec![GridArea::cell(at(s, address))],
                });
                while book.has_pending_calculation() {
                    book.advance_calculation();
                }
            }
            assert_eq!(
                calculated(book.host(), at(s, "H1")),
                Some(CalcValue::number(if volatile { 8.0 } else { 1.0 })),
                "{backend:?}: volatile={volatile}"
            );
            let ProjectedValue::Calculated { calculation, .. } = value(book.host(), at(s, "A1"))
            else {
                panic!("sizing calculation")
            };
            assert_eq!(
                calculation.stamp().unwrap().reads.invocations[0]
                    .identity
                    .reason,
                if volatile {
                    calc_engine::contract::ReevaluationReason::SpillShapePass
                } else {
                    calc_engine::contract::ReevaluationReason::NewCommandDemand
                }
            );
            if volatile {
                assert!(matches!(
                    calculated(book.host(), at(s, "A1")).unwrap().core_ref(),
                    calc_host::value::CoreRef::Error(calc_host::value::WorksheetErrorCode::Spill)
                ));
            }
        }
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        book.set_user_functions(calc_vba::load_module_with_backend("Skipped","Public Function Mark() As Long\n Application.Volatile True\n Mark=7\nEnd Function\n",100_000,backend).unwrap());
        enter(&mut book, at(s, "A1"), "TRUE");
        enter(&mut book, at(s, "B1"), "=IF(A1,Mark(),2)");
        let force = |book: &mut HostSession| {
            book.dispatch(SkinIntent::CalculateRange {
                areas: vec![GridArea::cell(at(s, "B1"))],
            });
            while book.has_pending_calculation() {
                book.advance_calculation();
            }
        };
        force(&mut book);
        enter(&mut book, at(s, "A1"), "FALSE");
        force(&mut book);
        let ProjectedValue::Calculated { calculation, .. } = value(book.host(), at(s, "B1")) else {
            panic!("calculated")
        };
        assert_eq!(calculation.stamp().unwrap().reads.udf_volatile, Some(false));
        assert!(calculation.stamp().unwrap().reads.invocations.is_empty());
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_vba_spill_placement_wait_refuses_without_repeating_the_effect() {
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    book.set_user_functions(calc_vba::load_module_with_backend("Placement","Private entries As Long\nPublic Function Counted() As Long\n entries=entries+1\n Counted=entries\nEnd Function\nPublic Function EntryCount() As Long\n EntryCount=entries\nEnd Function\n",100_000,calc_vba::Backend::Interpreter).unwrap());
    let force = |book: &mut HostSession, addresses: &[&str]| {
        book.dispatch(SkinIntent::CalculateRange {
            areas: addresses
                .iter()
                .map(|address| GridArea::cell(at(s, address)))
                .collect(),
        });
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
    };
    enter(&mut book, at(s, "C1"), "3");
    enter(
        &mut book,
        at(s, "B1"),
        "=IF(TRUE,SEQUENCE(C1+RANDBETWEEN(0,0)),A2)",
    );
    force(&mut book, &["B1"]);
    enter(&mut book, at(s, "A2"), "=SEQUENCE(1,2,Counted())");
    enter(&mut book, at(s, "C1"), "1");
    force(&mut book, &["A2", "B1"]);
    let result = value(book.host(), at(s, "A2"));
    assert!(
        matches!(result, ProjectedValue::NotModeled { .. }),
        "{result:?}"
    );
    enter(&mut book, at(s, "H1"), "=EntryCount()");
    force(&mut book, &["H1"]);
    assert_eq!(
        calculated(book.host(), at(s, "H1")),
        Some(CalcValue::number(1.0))
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_vba_new_registration_rebinds_an_unknown_call_and_preserves_unrelated_results() {
    let mut book = session(&calc_host::blank_package());
    let s = book.host().sheets()[0].id();
    for (address, text) in [("A1", "=Later()"), ("B1", "=A1+1"), ("H1", "=SUM(40,2)")] {
        enter(&mut book, at(s, address), text);
    }
    let ProjectedValue::Calculated {
        calculation: before,
        ..
    } = value(book.host(), at(s, "H1"))
    else {
        panic!("calculated")
    };
    assert_eq!(
        calculated(book.host(), at(s, "A1")),
        Some(CalcValue::error(calc_host::value::WorksheetErrorCode::Name))
    );
    book.set_user_functions(
        calc_vba::load_module_with_backend(
            "Added",
            "Public Function Later() As Long\n Later=52\nEnd Function\nPublic Function SUM(ByVal a As Double,ByVal b As Double) As Double\n SUM=999#\nEnd Function\n",
            100_000,
            calc_vba::Backend::Interpreter,
        )
        .unwrap(),
    );
    while book.has_pending_calculation() {
        book.advance_calculation();
    }
    assert_eq!(
        calculated(book.host(), at(s, "A1")),
        Some(CalcValue::number(52.0))
    );
    assert_eq!(
        calculated(book.host(), at(s, "B1")),
        Some(CalcValue::number(53.0))
    );
    let ProjectedValue::Calculated {
        calculation: after, ..
    } = value(book.host(), at(s, "H1"))
    else {
        panic!("calculated")
    };
    assert_eq!(
        before, after,
        "a new provider cannot reevaluate unrelated formulas"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn contract_vba_input_edit_starts_a_new_invocation_without_replaying_a_resume() {
    let mut backends = vec![calc_vba::Backend::Interpreter];
    #[cfg(not(target_arch = "wasm32"))]
    backends.push(calc_vba::Backend::Jit);
    for backend in backends {
        let source = "Private entries As Long\nPublic Function Counted(ByVal r As DnaCalcObjects.Range,ByVal count As Long) As Double\n Dim initial As Long\n Dim total As Long\n initial=count\n entries=entries+1\n Do While count>0\n total=total+1\n count=count-1\n Loop\n Counted=r.NumericValue+entries+total-initial\nEnd Function\nPublic Function EntryCount() As Long\n EntryCount=entries\nEnd Function\n";
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        book.set_user_functions(
            calc_vba::load_module_with_backend("Edited", source, 100_000, backend).unwrap(),
        );
        enter(&mut book, at(s, "A1"), "1");
        enter(&mut book, at(s, "D1"), "=Counted(A1,2000)");
        enter(&mut book, at(s, "E1"), "=EntryCount()");
        book.set_calculation_budget(WorkBudget::evaluations(1));
        book.take_calculation_events();
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::cell(at(s, "D1"))],
        });
        assert!(book.has_pending_calculation());
        let old = book
            .take_calculation_events()
            .into_iter()
            .find_map(|event| match event {
                calc_engine::CalcEvent::Admitted(receipt) => Some(receipt.command),
                _ => None,
            })
            .unwrap();
        enter(&mut book, at(s, "A1"), "10");
        let revision = book.host().revision();
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        let (number, calculation) = match value(book.host(), at(s, "D1")) {
            ProjectedValue::Calculated { value, calculation } => (value, calculation),
            ProjectedValue::Stale(held) => (
                held.last_value().unwrap().clone(),
                held.last_calculation().unwrap().clone(),
            ),
            other => panic!("edited calculation: {other:?}"),
        };
        assert_eq!(number, CalcValue::number(12.0));
        let stamp = calculation.stamp().unwrap();
        assert_eq!(
            stamp.reads.invocations[0].identity.reason,
            calc_engine::contract::ReevaluationReason::AuthoredOrInputChange
        );
        assert_ne!(stamp.reads.invocations[0].identity.pass.command, old);
        assert!(
            stamp
                .reads
                .observations
                .iter()
                .all(|read| read.view.frontier().model == revision)
        );
        let events = book.take_calculation_events();
        assert!(events.iter().any(|event|matches!(event,calc_engine::CalcEvent::Superseded(receipt) if receipt.command==old)));
        assert!(!events.iter().any(|event|matches!(event,calc_engine::CalcEvent::Completed(receipt) if receipt.command==old)));
        book.dispatch(SkinIntent::CalculateRange {
            areas: vec![GridArea::cell(at(s, "E1"))],
        });
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        assert_eq!(
            calculated(book.host(), at(s, "E1")),
            Some(CalcValue::number(2.0)),
            "one entry per admitted command, no resume replay"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn names_undo_restores_full_imported_records_without_recalculating_unrelated_cells() {
    let mut book = session(&package_with_names(
        r#"<definedName name="Rate" hidden="1" comment="book &amp; note" shortcutKey="K">2</definedName><definedName name="RATE" comment="fallback">9</definedName><definedName name="Rate" localSheetId="0" hidden="1" comment="sheet note">7</definedName><definedName name="MacroKept" hidden="1" comment="all flags" function="1" vbProcedure="1" xlm="1">1</definedName><definedName name="Tail">11</definedName>"#,
    ));
    let s = book.host().sheets()[0].id();
    let scope = calc_doc_model::NameScope::Sheet(s);
    enter(&mut book, at(s, "A1"), "=Rate+1");
    enter(&mut book, at(s, "B1"), "=SUM(40,2)");
    assert_eq!(
        calculated(book.host(), at(s, "A1")),
        Some(CalcValue::number(8.0))
    );
    let original = book.host().defined_names().to_vec();
    let old = book
        .host()
        .model()
        .definitions()
        .name_table()
        .lookup(scope, "Rate")
        .unwrap()
        .id();
    let ProjectedValue::Calculated {
        calculation: before,
        ..
    } = value(book.host(), at(s, "B1"))
    else {
        panic!("calculated")
    };
    let ProjectedValue::Calculated {
        calculation: formula_before,
        ..
    } = value(book.host(), at(s, "A1"))
    else {
        panic!("calculated")
    };
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![CellEdit::SetName {
                name: "Rate".into(),
                scope,
                formula: Some("7".into())
            }]
        })
        .accepted()
    );
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    let ProjectedValue::Calculated {
        calculation: formula_after,
        ..
    } = value(book.host(), at(s, "A1"))
    else {
        panic!("calculated")
    };
    assert_eq!(
        formula_before, formula_after,
        "Undo of an unchanged definition has no evaluation effect"
    );
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![CellEdit::SetName {
                name: "rAtE".into(),
                scope,
                formula: None
            }]
        })
        .accepted()
    );
    assert_eq!(
        calculated(book.host(), at(s, "A1")),
        Some(CalcValue::number(3.0))
    );
    let mut prior = old;
    for again in 0..2 {
        assert!(book.dispatch(SkinIntent::Undo).accepted());
        assert_eq!(book.host().defined_names(), original.as_slice());
        let restored = book
            .host()
            .model()
            .definitions()
            .name_table()
            .lookup(scope, "Rate")
            .unwrap()
            .id();
        assert_ne!(
            restored, prior,
            "Undo creates a new definition identity for a deleted name"
        );
        prior = restored;
        assert_eq!(
            calculated(book.host(), at(s, "A1")),
            Some(CalcValue::number(8.0))
        );
        let ProjectedValue::Calculated {
            calculation: after, ..
        } = value(book.host(), at(s, "B1"))
        else {
            panic!("calculated")
        };
        assert_eq!(before, after);
        if again == 0 {
            assert!(book.dispatch(SkinIntent::Redo).accepted());
        }
    }
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![CellEdit::SetName {
                name: "MacroKept".into(),
                scope: calc_doc_model::NameScope::Workbook,
                formula: None
            }]
        })
        .accepted()
    );
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(book.host().defined_names(), original.as_slice());
    let ProjectedValue::Calculated {
        calculation: after, ..
    } = value(book.host(), at(s, "B1"))
    else {
        panic!("calculated")
    };
    assert_eq!(before, after);
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![
                CellEdit::SetName {
                    name: "MacroKept".into(),
                    scope: calc_doc_model::NameScope::Workbook,
                    formula: None
                },
                CellEdit::SetName {
                    name: "Rate".into(),
                    scope,
                    formula: None
                },
            ]
        })
        .accepted()
    );
    assert!(book.dispatch(SkinIntent::Undo).accepted());
    assert_eq!(
        book.host().defined_names(),
        original.as_slice(),
        "batch restoration keeps imported record order"
    );
    assert!(
        book.dispatch(SkinIntent::Edit {
            edits: vec![CellEdit::SetName {
                name: "Rate".into(),
                scope: calc_doc_model::NameScope::Workbook,
                formula: Some("3".into())
            }]
        })
        .accepted()
    );
    let mut expected = original.clone();
    expected[0].formula = "3".into();
    let mut saved = Vec::new();
    book.save(&SavePolicy::default(), &mut saved).unwrap();
    let reopened = session(&saved);
    assert_eq!(reopened.host().defined_names(), expected.as_slice());
    assert!(part_text(&saved, "xl/workbook.xml").contains("shortcutKey=\"K\""));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn names_history_restores_sequential_same_key_batches_and_saved_duplicate_fallbacks() {
    use calc_doc_model::NameScope;
    let records = r#"<definedName name="Rate" hidden="1" comment="primary">2</definedName><definedName name="RATE" comment="fallback">7</definedName><definedName name="Tail">11</definedName>"#;
    for edits in [
        vec![Some("3"), None],
        vec![None, None],
        vec![None, Some("3")],
        vec![None, None, Some("4")],
        vec![Some("3"), None, Some("4"), None, Some("5")],
    ] {
        let mut book = session(&package_with_names(records));
        let original = book.host().defined_names().to_vec();
        let old = book
            .host()
            .model()
            .definitions()
            .name_table()
            .lookup(NameScope::Workbook, "Rate")
            .unwrap()
            .id();
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: edits
                    .into_iter()
                    .map(|formula| CellEdit::SetName {
                        name: "rAtE".into(),
                        scope: NameScope::Workbook,
                        formula: formula.map(str::to_owned)
                    })
                    .collect()
            })
            .accepted()
        );
        let forward = book.host().defined_names().to_vec();
        let mut saved = Vec::new();
        book.save(&SavePolicy::default(), &mut saved).unwrap();
        assert_eq!(session(&saved).host().defined_names(), forward.as_slice());
        for _ in 0..2 {
            assert!(book.dispatch(SkinIntent::Undo).accepted());
            assert_eq!(book.host().defined_names(), original.as_slice());
            assert_ne!(
                book.host()
                    .model()
                    .definitions()
                    .name_table()
                    .lookup(NameScope::Workbook, "Rate")
                    .unwrap()
                    .id(),
                old
            );
            assert!(book.dispatch(SkinIntent::Redo).accepted());
            assert_eq!(book.host().defined_names(), forward.as_slice());
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn names_history_tracks_a_recreated_definition_through_earlier_undo_and_redo() {
    use calc_doc_model::NameScope;
    let mut book = session(&package_with_names(
        r#"<definedName name="Rate" hidden="1" comment="keep">2</definedName><definedName name="Tail">11</definedName>"#,
    ));
    let original = book.host().defined_names().to_vec();
    let old = book
        .host()
        .model()
        .definitions()
        .name_table()
        .lookup(NameScope::Workbook, "Rate")
        .unwrap()
        .id();
    for formula in [Some("3"), None] {
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: vec![CellEdit::SetName {
                    name: "Rate".into(),
                    scope: NameScope::Workbook,
                    formula: formula.map(str::to_owned)
                }]
            })
            .accepted()
        );
    }
    let mut previous = old;
    for _ in 0..2 {
        assert!(book.dispatch(SkinIntent::Undo).accepted());
        let restored = book
            .host()
            .model()
            .definitions()
            .name_table()
            .lookup(NameScope::Workbook, "Rate")
            .unwrap()
            .id();
        assert_ne!(restored, previous);
        previous = restored;
        assert!(book.dispatch(SkinIntent::Undo).accepted());
        assert_eq!(book.host().defined_names(), original.as_slice());
        assert_eq!(
            book.host()
                .model()
                .definitions()
                .name_table()
                .lookup(NameScope::Workbook, "Rate")
                .unwrap()
                .id(),
            restored
        );
        for _ in 0..2 {
            assert!(book.dispatch(SkinIntent::Redo).accepted());
        }
        assert!(
            book.host()
                .model()
                .definitions()
                .name_table()
                .lookup(NameScope::Workbook, "Rate")
                .is_none()
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn names_saved_equal_duplicate_formulas_keep_their_source_attributes() {
    let mut book = session(&package_with_names(
        r#"<definedName name="Rate" shortcutKey="K">2</definedName><definedName name="Rate" shortcutKey="L">7</definedName>"#,
    ));
    let edit = |book: &mut HostSession, formula: Option<&str>| {
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: vec![CellEdit::SetName {
                    name: "Rate".into(),
                    scope: calc_doc_model::NameScope::Workbook,
                    formula: formula.map(str::to_owned)
                }]
            })
            .accepted()
        );
    };
    edit(&mut book, Some("7"));
    for _ in 0..2 {
        let mut saved = Vec::new();
        book.save(&SavePolicy::default(), &mut saved).unwrap();
        let xml = part_text(&saved, "xl/workbook.xml");
        let first = xml.find("shortcutKey=\"K\"").unwrap();
        let second = xml.find("shortcutKey=\"L\"").unwrap();
        assert!(
            first < second,
            "equal formulas cannot swap source attributes"
        );
        assert_eq!(
            session(&saved).host().defined_names(),
            book.host().defined_names()
        );
        edit(&mut book, None);
        edit(&mut book, None);
        for _ in 0..2 {
            assert!(book.dispatch(SkinIntent::Undo).accepted());
        }
    }
}

mod async_contract {
    use super::*;
    use calc_fml::{
        UserFunction, UserFunctionContext, UserFunctionInvocation, UserFunctionStep,
        UserInvocationFrame,
    };
    use calc_host::{AsyncDisposition as D, AsyncRejection as R, AsyncToken};

    use super::async_provider::{Counts, Echo};
    fn setup(formula: &str) -> (HostSession, SheetId, Rc<RefCell<Counts>>) {
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        let counts = Rc::new(RefCell::new(Counts::default()));
        let mut functions = calc_fml::CallableTable::new();
        functions
            .register("AsyncEcho", Rc::new(Echo(counts.clone())))
            .unwrap();
        book.set_user_functions(functions);
        enter(&mut book, at(s, "A1"), "1");
        enter(&mut book, at(s, "B1"), formula);
        enter(&mut book, at(s, "C1"), "=B1*2");
        book.set_calculation_budget(WorkBudget::evaluations(1));
        book.dispatch(SkinIntent::Recalculate);
        for _ in 0..100 {
            if !book.host().pending_async().is_empty() {
                break;
            }
            assert!(book.has_pending_calculation());
            book.advance_calculation();
        }
        assert_eq!(book.host().pending_async().len(), 1);
        (book, s, counts)
    }
    fn token(book: &HostSession) -> AsyncToken {
        book.host().pending_async()[0].clone()
    }
    fn drain(book: &mut HostSession) {
        for _ in 0..100 {
            if !book.has_pending_calculation() {
                return;
            }
            let progress = book.advance_calculation();
            assert!(!matches!(progress, HostProgress::Failed(_)), "{progress:?}");
        }
        panic!("finite continuation did not finish")
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p07_async_suffix_resumes_once_and_dependents_get_a_later_pass() {
        let (mut book, s, counts) = setup("=10+AsyncEcho(A1)+3");
        let first = token(&book);
        assert_eq!(counts.borrow().prefix, 1);
        assert_eq!(counts.borrow().suffix, 0);
        assert!(
            !book.has_pending_calculation(),
            "an external wait schedules no busy task"
        );
        for _ in 0..3 {
            assert!(matches!(
                book.advance_calculation(),
                HostProgress::Waiting { .. }
            ));
        }
        assert_eq!(
            counts.borrow().resumes,
            1,
            "polling never re-enters a parked provider"
        );
        assert_eq!(
            book.complete_async(&first, CalcValue::number(29.0)),
            D::Accepted
        );
        assert!(book.has_pending_calculation());
        assert_eq!(
            counts.borrow().suffix,
            0,
            "delivery itself runs no provider code"
        );
        assert_eq!(
            book.complete_async(&first, CalcValue::number(29.0)),
            D::Duplicate
        );
        assert_eq!(
            book.complete_async(&first, CalcValue::number(999.0)),
            D::Rejected(R::ConflictingDuplicate)
        );
        drain(&mut book);
        assert_eq!(
            calculated(book.host(), at(s, "B1")),
            Some(CalcValue::number(42.0))
        );
        assert_eq!(
            calculated(book.host(), at(s, "C1")),
            Some(CalcValue::number(84.0))
        );
        assert_eq!((counts.borrow().prefix, counts.borrow().suffix), (1, 1));
        let b = value(book.host(), at(s, "B1"));
        let c = value(book.host(), at(s, "C1"));
        let (
            ProjectedValue::Calculated { calculation: b, .. },
            ProjectedValue::Calculated { calculation: c, .. },
        ) = (b, c)
        else {
            panic!("calculated values")
        };
        assert_eq!(
            b.stamp().unwrap().reads.invocations[0].runtime,
            first.runtime
        );
        assert_eq!(b.stamp().unwrap().certification.pass, first.invocation.pass);
        assert_eq!(
            c.stamp().unwrap().certification.pass.reason,
            calc_engine::contract::PassReason::AsyncDependents
        );
        assert_eq!(
            book.complete_async(&first, CalcValue::number(29.0)),
            D::Duplicate
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p07_two_sites_keep_distinct_tokens_and_one_root_activation() {
        let (mut book, s, counts) = setup("=AsyncEcho(A1)+AsyncEcho(A1)+2");
        let first = token(&book);
        assert_eq!(
            book.complete_async(&first, CalcValue::number(20.0)),
            D::Accepted
        );
        drain(&mut book);
        let second = token(&book);
        assert_ne!(first.id, second.id);
        assert_ne!(first.runtime, second.runtime);
        assert_eq!(first.invocation.pass, second.invocation.pass);
        assert_ne!(first.invocation.call_site, second.invocation.call_site);
        assert_eq!(
            book.complete_async(&first, CalcValue::number(20.0)),
            D::Duplicate
        );
        assert_eq!(
            book.complete_async(&second, CalcValue::number(20.0)),
            D::Accepted
        );
        drain(&mut book);
        assert_eq!(
            calculated(book.host(), at(s, "B1")),
            Some(CalcValue::number(42.0))
        );
        assert_eq!((counts.borrow().prefix, counts.borrow().suffix), (2, 2));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p07_edits_moves_undo_reload_cancel_and_reopen_reject_late_results() {
        for change in 0..7 {
            let (mut book, s, counts) = setup("=AsyncEcho(A1)+1");
            let old = token(&book);
            match change {
                0 => {
                    enter(&mut book, at(s, "B1"), "99");
                }
                1 => {
                    enter(&mut book, at(s, "B1"), "99");
                    book.dispatch(SkinIntent::Undo);
                }
                2 => {
                    enter(&mut book, at(s, "A1"), "2");
                }
                3 => {
                    book.dispatch(SkinIntent::Copy {
                        from: GridArea::cell(at(s, "B1")),
                        to: GridArea::cell(at(s, "D1")),
                        cut: true,
                    });
                }
                4 => {
                    let mut table = calc_fml::CallableTable::new();
                    table
                        .register("AsyncEcho", Rc::new(Echo(counts.clone())))
                        .unwrap();
                    book.set_user_functions(table);
                }
                5 => {
                    book.cancel_calculation();
                }
                6 => {
                    book = session(&calc_host::blank_package());
                }
                _ => unreachable!(),
            }
            let before = book.host().revision();
            assert!(
                matches!(
                    book.complete_async(&old, CalcValue::number(500.0)),
                    D::Rejected(_)
                ),
                "change {change}"
            );
            assert_eq!(book.host().revision(), before);
            assert_eq!(
                counts.borrow().suffix,
                0,
                "change {change} resumed stale code"
            );
            for address in ["B1", "C1", "D1"] {
                assert_ne!(
                    calculated(book.host(), at(s, address)),
                    Some(CalcValue::number(501.0))
                );
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p07_tampering_cannot_consume_or_redirect_the_real_token() {
        let (mut book, s, counts) = setup("=AsyncEcho(A1)");
        let real = token(&book);
        for field in 0..7 {
            let mut fake = real.clone();
            match field {
                0 => fake.target.locus = at(s, "D1"),
                1 => fake.provider.generation += 1,
                2 => fake.runtime += 1,
                3 => fake.request += 1,
                4 => fake.target.formula_generation += 1,
                5 => fake.required += 1,
                6 => fake.invocation.evaluation += 1,
                _ => unreachable!(),
            }
            assert_eq!(
                book.complete_async(&fake, CalcValue::number(999.0)),
                D::Rejected(R::IdentityMismatch)
            );
        }
        assert_eq!(counts.borrow().suffix, 0);
        assert_eq!(
            book.complete_async(&real, CalcValue::number(42.0)),
            D::Accepted
        );
        drain(&mut book);
        assert_eq!(
            calculated(book.host(), at(s, "B1")),
            Some(CalcValue::number(42.0))
        );
    }
    #[derive(Debug)]
    struct LateRead {
        source: CellKey,
        provisional: bool,
        entries: Rc<RefCell<Vec<(u64, Option<f64>)>>>,
    }
    impl UserFunction for LateRead {
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            panic!("reference context required")
        }
        fn reevaluation_policy(&self) -> Option<calc_fml::UserReevaluationReason> {
            Some(if self.provisional {
                calc_fml::UserReevaluationReason::ProvisionalArguments
            } else {
                calc_fml::UserReevaluationReason::XllUncalcedUnwind
            })
        }
        fn call_with_context(
            &self,
            _: &[CalcValue],
            context: &mut dyn UserFunctionContext,
        ) -> Result<CalcValue, String> {
            let index = self.entries.borrow().len();
            self.entries
                .borrow_mut()
                .push((context.invocation().unwrap().id, None));
            let result =
                context.argument_values_of(&calc_host::value::ReferenceLike::cell(self.source));
            match result {
                Ok(value) => {
                    let n = value.as_number().unwrap_or(0.0);
                    self.entries.borrow_mut()[index].1 = Some(n);
                    Ok(CalcValue::number(n))
                }
                Err(message) => {
                    context.request_reevaluation(
                        calc_fml::UserReevaluationReason::XllUncalcedUnwind,
                    )?;
                    Err(message)
                }
            }
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p03_unwind_and_provisional_arguments_are_new_semantic_evaluations() {
        for provisional in [false, true] {
            for budget in [1, usize::MAX] {
                let mut book = session(&calc_host::blank_package());
                let s = book.host().sheets()[0].id();
                book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
                let entries = Rc::new(RefCell::new(Vec::new()));
                let mut functions = calc_fml::CallableTable::new();
                functions
                    .register(
                        "LateRead",
                        Rc::new(LateRead {
                            source: at(s, "D1"),
                            provisional,
                            entries: entries.clone(),
                        }),
                    )
                    .unwrap();
                book.set_user_functions(functions);
                for (address, text) in [("B1", "=LateRead()"), ("C1", "=B1+1"), ("D1", "=10+1")] {
                    enter(&mut book, at(s, address), text);
                }
                book.set_calculation_budget(WorkBudget::evaluations(budget));
                book.dispatch(SkinIntent::Recalculate);
                drain(&mut book);
                assert_eq!(
                    calculated(book.host(), at(s, "B1")),
                    Some(CalcValue::number(11.0)),
                    "provisional={provisional} budget={budget}"
                );
                assert_eq!(
                    calculated(book.host(), at(s, "C1")),
                    Some(CalcValue::number(12.0))
                );
                let entries = entries.borrow();
                assert_eq!(entries.len(), 2);
                assert_ne!(entries[0].0, entries[1].0);
                assert_eq!(entries[0].1, if provisional { Some(0.0) } else { None });
                assert_eq!(entries[1].1, Some(11.0));
                let events = book.take_calculation_events();
                let transitions: Vec<_> = events
                    .iter()
                    .filter_map(|e| {
                        if let calc_engine::CalcEvent::Reevaluated {
                            previous,
                            next,
                            invocations,
                        } = e
                        {
                            Some((previous, next, invocations))
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(transitions.len(), 1);
                let (previous, next, calls) = transitions[0];
                assert_eq!(previous.evaluation, 0);
                assert_eq!(next.evaluation, 1);
                assert_eq!(previous.pass, next.pass);
                assert_eq!(
                    next.reason,
                    if provisional {
                        calc_engine::contract::ReevaluationReason::ProvisionalArguments
                    } else {
                        calc_engine::contract::ReevaluationReason::XllUncalcedUnwind
                    }
                );
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].runtime, entries[0].0);
                let ProjectedValue::Calculated { calculation, .. } =
                    value(book.host(), at(s, "B1"))
                else {
                    panic!("calculated")
                };
                let root = match &calculation.stamp().unwrap().computed_by {
                    calc_engine::contract::ComputationOrigin::Evaluated { root } => root,
                    _ => panic!("evaluated"),
                };
                assert_eq!(root, next);
            }
        }
    }

    #[derive(Debug)]
    struct Reentrant {
        allowed: bool,
        entries: Rc<RefCell<Vec<UserInvocationFrame>>>,
    }
    impl UserFunction for Reentrant {
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            panic!("invocation-aware callback required")
        }
        fn state_lane(&self) -> Option<u64> {
            Some(77)
        }
        fn permits_reentry(&self) -> bool {
            self.allowed
        }
        fn call_with_context(
            &self,
            args: &[CalcValue],
            context: &mut dyn UserFunctionContext,
        ) -> Result<CalcValue, String> {
            self.entries
                .borrow_mut()
                .push(context.invocation().unwrap().clone());
            let n = args[0].as_number().unwrap();
            if n == 0.0 {
                Ok(CalcValue::number(0.0))
            } else {
                let value = context.call_user_function("Nested", &[CalcValue::number(n - 1.0)])?;
                Ok(CalcValue::number(value.as_number().unwrap() + 1.0))
            }
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p03_nested_callbacks_inherit_the_frame_and_enforce_state_lane_policy() {
        for allowed in [false, true] {
            let mut book = session(&calc_host::blank_package());
            let s = book.host().sheets()[0].id();
            book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
            let entries = Rc::new(RefCell::new(Vec::new()));
            let mut functions = calc_fml::CallableTable::new();
            functions
                .register(
                    "Nested",
                    Rc::new(Reentrant {
                        allowed,
                        entries: entries.clone(),
                    }),
                )
                .unwrap();
            book.set_user_functions(functions);
            enter(&mut book, at(s, "B1"), "=IFERROR(Nested(3)+39,999)");
            book.dispatch(SkinIntent::Recalculate);
            drain(&mut book);
            let entries = entries.borrow();
            if allowed {
                assert_eq!(
                    calculated(book.host(), at(s, "B1")),
                    Some(CalcValue::number(42.0))
                );
                assert_eq!(entries.len(), 4);
                for pair in entries.windows(2) {
                    assert_eq!(pair[1].parent, Some(pair[0].id));
                    assert!(Rc::ptr_eq(&pair[0].read_view, &pair[1].read_view));
                }
            } else {
                assert_eq!(entries.len(), 1, "refuse before nested entry or effects");
                assert!(
                    matches!(
                        value(book.host(), at(s, "B1")),
                        ProjectedValue::NotModeled { .. }
                    ),
                    "a policy refusal survives IFERROR"
                );
            }
        }
    }

    #[derive(Debug)]
    struct RtdProbe(Rc<RefCell<usize>>);
    impl UserFunction for RtdProbe {
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            panic!("an admitted external input view is required")
        }
        fn call_with_context(
            &self,
            _: &[CalcValue],
            context: &mut dyn UserFunctionContext,
        ) -> Result<CalcValue, String> {
            *self.0.borrow_mut() += 1;
            Ok(context
                .rtd_value(17)?
                .map(|(_, value)| value)
                .unwrap_or_else(|| CalcValue::error(calc_host::value::WorksheetErrorCode::NA)))
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p03_rtd_events_are_ordered_inputs_and_manual_demand_pins_the_view() {
        use calc_host::RtdDisposition as T;
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        let entries = Rc::new(RefCell::new(0));
        let mut functions = calc_fml::CallableTable::new();
        functions
            .register("RtdProbe", Rc::new(RtdProbe(entries.clone())))
            .unwrap();
        book.set_user_functions(functions);
        let provider = book
            .host()
            .user_function_bindings()
            .bind(None, "RtdProbe")
            .unwrap();
        enter(&mut book, at(s, "B1"), "=RtdProbe()+1");
        let revision = book.host().revision();
        assert_eq!(
            book.update_rtd(provider, 17, 1, CalcValue::number(20.0)),
            T::Accepted
        );
        assert_eq!(*entries.borrow(), 0);
        assert!(!book.has_pending_calculation());
        assert_eq!(book.host().revision(), revision);
        book.dispatch(SkinIntent::Recalculate);
        drain(&mut book);
        assert_eq!(
            calculated(book.host(), at(s, "B1")),
            Some(CalcValue::number(21.0))
        );
        let ProjectedValue::Calculated { calculation, .. } = value(book.host(), at(s, "B1")) else {
            panic!("calculated")
        };
        let read = &calculation.stamp().unwrap().reads.external_inputs[0];
        assert_eq!(read.key.callable, provider);
        assert_eq!(read.key.subscription, 17);
        assert_eq!(read.sequence, 1);
        assert_eq!(
            book.update_rtd(provider, 17, 1, CalcValue::number(20.0)),
            T::Duplicate
        );
        assert_eq!(
            book.update_rtd(provider, 17, 1, CalcValue::number(999.0)),
            T::ConflictingSequence
        );
        assert_eq!(*entries.borrow(), 1);
        assert_eq!(
            book.update_rtd(provider, 17, 2, CalcValue::number(40.0)),
            T::Accepted
        );
        assert_eq!(
            book.update_rtd(provider, 17, 1, CalcValue::number(999.0)),
            T::StaleSequence
        );
        assert!(!book.has_pending_calculation());
        assert!(matches!(
            value(book.host(), at(s, "B1")),
            ProjectedValue::Stale(_)
        ));
        book.dispatch(SkinIntent::Recalculate);
        drain(&mut book);
        assert_eq!(
            calculated(book.host(), at(s, "B1")),
            Some(CalcValue::number(41.0))
        );
        assert_eq!(*entries.borrow(), 2);
        assert_eq!(book.retire_rtd(provider, 17), T::Accepted);
        assert_eq!(
            book.update_rtd(provider, 17, 99, CalcValue::number(999.0)),
            T::Retired
        );
        book.dispatch(SkinIntent::Recalculate);
        drain(&mut book);
        assert_eq!(
            calculated(book.host(), at(s, "B1")).unwrap().as_error(),
            Some(calc_host::value::WorksheetErrorCode::NA)
        );
        let mut replacement = calc_fml::CallableTable::new();
        replacement
            .register("RtdProbe", Rc::new(RtdProbe(entries.clone())))
            .unwrap();
        book.set_user_functions(replacement);
        assert_eq!(
            book.update_rtd(provider, 17, 3, CalcValue::number(999.0)),
            T::ProviderChanged
        );
        book.dispatch(SkinIntent::Recalculate);
        drain(&mut book);
        assert!(matches!(
            calculated(book.host(), at(s, "B1")).unwrap().as_error(),
            Some(calc_host::value::WorksheetErrorCode::NA)
        ));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p07_old_and_new_async_requests_return_in_reverse_order() {
        let (mut book, s, counts) = setup("=AsyncEcho(A1)+1");
        let old = token(&book);
        enter(&mut book, at(s, "A1"), "2");
        for _ in 0..100 {
            book.advance_calculation();
            if book
                .host()
                .pending_async()
                .first()
                .is_some_and(|token| token.id != old.id)
            {
                break;
            }
        }
        let new = token(&book);
        assert_ne!(old.id, new.id);
        assert_ne!(old.invocation.pass.command, new.invocation.pass.command);
        assert_eq!(
            book.complete_async(&new, CalcValue::number(41.0)),
            D::Accepted
        );
        drain(&mut book);
        assert!(matches!(
            book.complete_async(&old, CalcValue::number(999.0)),
            D::Rejected(_)
        ));
        assert_eq!(
            calculated(book.host(), at(s, "B1")),
            Some(CalcValue::number(42.0))
        );
        assert_eq!((counts.borrow().prefix, counts.borrow().suffix), (2, 1));
    }

    #[derive(Debug)]
    struct YieldLate {
        source: CellKey,
        counts: Rc<RefCell<Counts>>,
    }
    #[derive(Debug)]
    struct YieldLateCall {
        value: CalcValue,
        counts: Rc<RefCell<Counts>>,
        remaining: u32,
        entered: bool,
    }
    impl UserFunction for YieldLate {
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            panic!("cooperative entry required")
        }
        fn reevaluation_policy(&self) -> Option<calc_fml::UserReevaluationReason> {
            Some(calc_fml::UserReevaluationReason::ProvisionalArguments)
        }
        fn begin_call(
            &self,
            _: &[CalcValue],
            _: UserInvocationFrame,
            context: &mut dyn UserFunctionContext,
        ) -> Result<Option<Box<dyn UserFunctionInvocation>>, String> {
            let value =
                context.argument_values_of(&calc_host::value::ReferenceLike::cell(self.source))?;
            Ok(Some(Box::new(YieldLateCall {
                value,
                counts: self.counts.clone(),
                remaining: 3,
                entered: false,
            })))
        }
    }
    impl UserFunctionInvocation for YieldLateCall {
        fn resume(
            &mut self,
            _: &mut dyn UserFunctionContext,
            _: u64,
        ) -> Result<UserFunctionStep, String> {
            self.counts.borrow_mut().resumes += 1;
            if !self.entered {
                self.entered = true;
                self.counts.borrow_mut().prefix += 1;
            }
            if self.remaining > 0 {
                self.remaining -= 1;
                return Ok(UserFunctionStep::Suspended);
            }
            self.counts.borrow_mut().suffix += 1;
            Ok(UserFunctionStep::Complete(self.value.clone()))
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p03_provisional_body_finishes_across_safepoints_before_semantic_reentry() {
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        let counts = Rc::new(RefCell::new(Counts::default()));
        let mut functions = calc_fml::CallableTable::new();
        functions
            .register(
                "YieldLate",
                Rc::new(YieldLate {
                    source: at(s, "D1"),
                    counts: counts.clone(),
                }),
            )
            .unwrap();
        book.set_user_functions(functions);
        enter(&mut book, at(s, "B1"), "=YieldLate()");
        enter(&mut book, at(s, "D1"), "=10+1");
        book.set_calculation_budget(WorkBudget::evaluations(1));
        book.dispatch(SkinIntent::Recalculate);
        drain(&mut book);
        assert_eq!(
            calculated(book.host(), at(s, "B1")),
            Some(CalcValue::number(11.0))
        );
        assert_eq!(
            (
                counts.borrow().prefix,
                counts.borrow().suffix,
                counts.borrow().resumes
            ),
            (2, 2, 8)
        );
        assert_eq!(
            book.take_calculation_events()
                .iter()
                .filter(|e| matches!(e, calc_engine::CalcEvent::Reevaluated { .. }))
                .count(),
            1
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p03_many_provisional_parents_share_a_consumer_without_growing_fence_ranks() {
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        let entries = Rc::new(RefCell::new(Vec::new()));
        let mut functions = calc_fml::CallableTable::new();
        functions
            .register(
                "LateRead",
                Rc::new(LateRead {
                    source: at(s, "D1000"),
                    provisional: true,
                    entries: entries.clone(),
                }),
            )
            .unwrap();
        book.set_user_functions(functions);
        let edits = (1..=72)
            .map(|row| CellEdit::Set {
                at: CellKey::new(s, row, 2).unwrap(),
                input: formula("LateRead()"),
            })
            .chain([
                CellEdit::Set {
                    at: at(s, "A999"),
                    input: formula("SUM(B1:B72)"),
                },
                CellEdit::Set {
                    at: at(s, "D1000"),
                    input: formula("10+1"),
                },
            ])
            .collect();
        assert!(book.dispatch(SkinIntent::Edit { edits }).accepted());
        book.set_calculation_budget(WorkBudget::evaluations(1));
        book.dispatch(SkinIntent::Recalculate);
        for _ in 0..1000 {
            if !book.has_pending_calculation() {
                break;
            }
            let progress = book.advance_calculation();
            assert!(!matches!(progress, HostProgress::Failed(_)), "{progress:?}");
        }
        assert!(!book.has_pending_calculation());
        assert_eq!(
            calculated(book.host(), at(s, "A999")),
            Some(CalcValue::number(792.0))
        );
        assert_eq!(entries.borrow().len(), 144);
        assert_eq!(
            book.take_calculation_events()
                .iter()
                .filter(|event| matches!(event, calc_engine::CalcEvent::Reevaluated { .. }))
                .count(),
            72
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p07_clock_input_change_supersedes_a_waiting_command_without_stranding_it() {
        let (mut book, s, counts) = setup("=AsyncEcho(A1)+NOW()");
        let old = token(&book);
        book.set_clock(Some(40000.0));
        assert!(book.has_pending_calculation());
        assert!(matches!(
            book.complete_async(&old, CalcValue::number(999.0)),
            D::Rejected(_)
        ));
        for _ in 0..100 {
            book.advance_calculation();
            if book
                .host()
                .pending_async()
                .first()
                .is_some_and(|token| token.id != old.id)
            {
                break;
            }
        }
        let new = token(&book);
        assert_ne!(new.id, old.id);
        assert_eq!(
            book.complete_async(&new, CalcValue::number(42.0)),
            D::Accepted
        );
        drain(&mut book);
        assert!(!book.has_pending_calculation());
        assert_eq!(
            calculated(book.host(), at(s, "B1")),
            Some(CalcValue::number(40042.0))
        );
        assert_eq!((counts.borrow().prefix, counts.borrow().suffix), (2, 1));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contract_p03_terminal_unavailable_arguments_do_not_enter_a_provisional_body() {
        let mut book = session(&calc_host::blank_package());
        let s = book.host().sheets()[0].id();
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
        let counts = Rc::new(RefCell::new(Counts::default()));
        let mut functions = calc_fml::CallableTable::new();
        functions
            .register(
                "YieldLate",
                Rc::new(YieldLate {
                    source: at(s, "D1"),
                    counts: counts.clone(),
                }),
            )
            .unwrap();
        functions.register("TerminalProducer",Rc::new(TerminalProducer)).unwrap();
        book.set_user_functions(functions);
        enter(&mut book, at(s, "D1"), "=TerminalProducer()");
        book.dispatch(SkinIntent::Recalculate);
        drain(&mut book);
        assert!(matches!(value(book.host(),at(s,"D1")),ProjectedValue::NotModeled{..}), "terminal producer: {:?}", value(book.host(),at(s,"D1")));
        enter(&mut book, at(s, "B1"), "=YieldLate(D1)");
        book.take_calculation_events();
        book.dispatch(SkinIntent::Recalculate);
        drain(&mut book);
        assert_eq!(
            (
                counts.borrow().prefix,
                counts.borrow().suffix,
                counts.borrow().resumes
            ),
            (0, 0, 0)
        );
        assert!(matches!(
            value(book.host(), at(s, "B1")),
            ProjectedValue::NotModeled { .. }
        ));
        assert!(
            !book
                .take_calculation_events()
                .iter()
                .any(|event| matches!(event, calc_engine::CalcEvent::Reevaluated { .. }))
        );
    }
    #[derive(Debug)]
    struct TerminalProducer;
    impl UserFunction for TerminalProducer {
        fn call(&self,_:&[CalcValue])->Result<CalcValue,String>{Err("fixture has no value entry".into())}
        fn call_with_context(&self,_:&[CalcValue],context:&mut dyn UserFunctionContext)->Result<CalcValue,String>{Err(context.refuse_by("fixture absent external facility","dc-hsa5.4"))}
    }

}
