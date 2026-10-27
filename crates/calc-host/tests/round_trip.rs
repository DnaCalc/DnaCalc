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
            oracle.dispatch(SkinIntent::Recalculate);
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
            if let Err(error) = session.save(&SavePolicy::default(), &mut saved) {
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
const GF0_CASES: [(&str, &str, &str, &str, &str, &[u8]); 4] = [
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
];

/// The defined names as `name formula`, in the workbook's order.
fn gf0_names(host: &HostState) -> Vec<String> {
    host.defined_names()
        .iter()
        .map(|name| format!("{} {:?} {}", name.name, name.scope, name.formula))
        .collect()
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
        let (first, last) = from.split_once(':').unwrap();
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
}
