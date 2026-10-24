//! The host's round trip over a workbook Excel saved with dynamic arrays, legacy arrays, names
//! and a table (calc-doc's `tests/fixtures/cdoc_m1_2/d1.saved.xlsx`).
//!
//! - dc-ndi (D1): open, edit, recalculate, save with the calculated values as cached values, and
//!   the cells that read a removed array's area recalculated.
//! - dc-35p (D2): the session skins drive ([`calc_host::HostSession`]): cell entries, multi-cell
//!   edits, undo and redo, the calculation mode, the selection and subscriptions with their
//!   deltas, the intent log's replay, and the M1 proof: open, edit, undo, recalculate, save and
//!   reopen.

#![cfg(not(target_arch = "wasm32"))]
// Case names are fixed identities of the form `<bead>__<case>`.
#![allow(non_snake_case)]

use calc_host::value::{CalcValue, CellKey, CoreValue, GridArea, SheetId};
use calc_host::{
    CachedValue, CalcMode, CellEdit, CellInput, CellText, DeltaCause, DeltaMirror, EntryRefusal,
    FiniteNumber, FormulaText, HostCommand, HostDelta, HostOptions, HostProgress, HostSession,
    HostState, IntentError, IntentReceipt, Interest, MemorySource, NotModeled, OpenPolicy,
    PackageRead, ProjectedCell, ProjectedValue, ProjectionRequest, SavePolicy, Selection,
    SkinIntent, SubscriptionId, Unobserved, WorkBudget, dispatch, project, replay, save, step,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

fn fixture() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../calc-doc/tests/fixtures/cdoc_m1_2/d1.saved.xlsx");
    std::fs::read(path).expect("the D1 fixture")
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

#[test]
fn d1_round_trip__a_workbook_with_arrays_opens_calculates_and_saves_its_values() {
    let mut host = open(&fixture());
    let calc = sheet(&host, "Calc");
    recalculate(&mut host);
    // The rest of the workbook calculates. The array at A1:B3 is SEQUENCE(3,2), which the
    // catalogue does not hold yet (F3, dc-728): its anchor is refused, and the cells it fills read
    // as not available.
    assert_eq!(
        calculated(&host, at(calc, "U1")),
        Some(CalcValue::number(2.0))
    );
    assert_eq!(
        calculated(&host, at(calc, "U3")),
        Some(CalcValue::number(21.0))
    );
    assert!(matches!(
        value(&host, at(calc, "A1")),
        ProjectedValue::NotModeled {
            refusal: NotModeled::Compile(_) | NotModeled::Evaluation(_),
            ..
        }
    ));
    assert!(matches!(
        value(&host, at(calc, "B2")),
        ProjectedValue::NotModeled { .. }
    ));
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
    // Calculated cells carry the engine's values, a text with t="str"; the refused anchor has
    // none, which Excel calculates on open because a full calculation is asked for (below).
    assert!(
        xml.contains(r#"<c r="U3"><f>U2*3</f><v>24</v></c>"#),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<c r="U1"><f>Data!A1+1</f><v>2</v></c>"#),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<c r="A1" cm="1"><f t="array" ref="A1:B3">_xlfn.SEQUENCE(3,2)</f></c>"#),
        "{xml}"
    );
    // The refused arrays (SEQUENCE, SORT and FILTER wait on F3, dc-728) are saved without a
    // value, which Excel does not calculate on open (calc-doc's tests/fixtures/d1/
    // excel-open.json, array-no-value), so a full calculation is asked for.
    assert!(part_text(&saved, "xl/workbook.xml").contains(r#"fullCalcOnLoad="1""#));
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

#[test]
fn d1_round_trip__readers_of_a_removed_array_recalculate() {
    let mut host = open(&fixture());
    let calc = sheet(&host, "Calc");
    // Z1 reads B2, a cell the array at A1:B3 fills: not available while the array stands.
    edit(
        &mut host,
        vec![CellEdit::Set {
            at: at(calc, "Z1"),
            input: formula("B2+1"),
        }],
    );
    recalculate(&mut host);
    assert!(matches!(
        value(&host, at(calc, "Z1")),
        ProjectedValue::NotModeled { .. }
    ));
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

/// Every delta received so far applied to a mirror, which must equal what a new subscription to
/// the same interest is given: the deltas carried every change.
fn assert_mirror_current(session: &mut HostSession, interest: Interest, received: &Received) {
    let mut mirror = DeltaMirror::default();
    for delta in received.borrow().iter() {
        mirror.apply(delta).expect("the deltas follow one another");
    }
    let (fresh_id, fresh) = subscribe(session, interest);
    let expected: Vec<ProjectedCell> = fresh.borrow()[0].cells.clone();
    assert!(session.unsubscribe(fresh_id));
    let mirrored: Vec<ProjectedCell> = mirror.cells().cloned().collect();
    assert_eq!(mirrored, expected);
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

#[test]
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
    assert_mirror_current(&mut session, interest, &received);
}

#[test]
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
    assert_mirror_current(&mut session, Interest::Area(area), &received);
}

#[test]
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

#[test]
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

#[test]
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

#[test]
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

#[test]
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

#[test]
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
            .map(|cell| (cell.input().cloned(), format!("{:?}", cell.value())))
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
#[test]
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
    assert!(
        metadata.contains(r#"<valueMetadata count="3">"#),
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
    // The fixture's refused arrays (SEQUENCE, SORT and FILTER wait on F3, dc-728) are saved
    // without a value, which Excel does not calculate on open, so a full calculation is asked
    // for (calc-doc's tests/fixtures/d1/excel-open.json, array-no-value).
    assert!(part_text(&saved, "xl/workbook.xml").contains(r#"fullCalcOnLoad="1""#));
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
/// text typed into the cell, a formula with its `=`) or `undo` or `redo`: open a session, dispatch
/// the steps (each recalculated, as in automatic mode), save. D1's form
/// `DNACALC_D1_ROUND_TRIP=<input.xlsx>|<sheet>|<cell>|<number>|<output.xlsx>` is the one step
/// that sets the cell to the number. The saved file is then captured in Excel (`cargo xtask
/// capture`) and compared (`cargo xtask compare`) outside this test. Without either variable it
/// prints that it did not run.
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
        let intent = match step_text {
            "undo" => SkinIntent::Undo,
            "redo" => SkinIntent::Redo,
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
