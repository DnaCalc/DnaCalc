//! Workbook event positions through the application host route. P22's broader Excel
//! event/reentry matrix remains unrecorded; these controls establish the DNA route.
use calc_host::value::{CellKey, GridArea, SheetId};
use calc_host::{
    BeforeSaveOutcome, CalcMode, CellEdit, CellInput, ExcelSaveContext, FiniteNumber, HostOptions,
    HostProgress, HostSession, HostState, Interest, MemorySource, OpenPolicy, ProjectedValue,
    ProjectionRequest, SaveFailed, SaveOperation, SavePolicy, SkinIntent, WorkBudget,
    WorkbookEventHandler, project,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
#[path = "../../calc-engine/tests/fixtures/seven_cell_async.rs"]
mod async_provider;

fn book() -> HostSession {
    HostSession::open(
        Arc::new(MemorySource::new(calc_host::blank_package())),
        &OpenPolicy::default(),
        HostOptions {
            calc_mode: Some(CalcMode::Manual),
            ..Default::default()
        },
    )
    .unwrap()
}
fn cell(book: &HostSession, column: u32) -> CellKey {
    CellKey::new(book.host().sheets()[0].id(), 1, column).unwrap()
}
fn enter(book: &mut HostSession, column: u32, text: &str) {
    let at = cell(book, column);
    let r = book.dispatch(SkinIntent::EnterCell {
        at,
        text: text.into(),
    });
    assert!(r.accepted(), "{:?}", r.error);
}
fn number(host: &HostState, at: CellKey) -> Option<f64> {
    match project(host, &ProjectionRequest { cells: vec![at] })
        .unwrap()
        .cells()[0]
        .value()
    {
        ProjectedValue::Literal(v) | ProjectedValue::Calculated { value: v, .. } => v.as_number(),
        ProjectedValue::Stale(v) => v.last_value().and_then(|v| v.as_number()),
        _ => None,
    }
}
struct Probe {
    a: CellKey,
    b: CellKey,
    c: CellKey,
    log: RefCell<Vec<String>>,
    fail: Cell<bool>,
    cancel: Cell<bool>,
    save_edit: Cell<bool>,
}

struct StopDelivery {
    calculation: bool,
    retire: bool,
    delivered: RefCell<Vec<SheetId>>,
}
impl StopDelivery {
    fn stop(&self, session: &mut HostSession, sheet: SheetId) {
        self.delivered.borrow_mut().push(sheet);
        if self.retire { session.set_workbook_event_handler(None); }
        else { session.set_workbook_events_enabled(false); }
    }
}
impl WorkbookEventHandler for StopDelivery {
    fn worksheet_change(&self, session: &mut HostSession, sheet: SheetId, _: &[GridArea]) -> Result<(), String> {
        if !self.calculation { self.stop(session, sheet); }
        Ok(())
    }
    fn worksheet_calculate(&self, session: &mut HostSession, sheet: SheetId) -> Result<(), String> {
        if self.calculation { self.stop(session, sheet); }
        Ok(())
    }
    fn before_save(&self, _: &mut HostSession, _: SaveOperation) -> Result<BeforeSaveOutcome, String> {
        Ok(BeforeSaveOutcome::default())
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_stops_a_multi_sheet_batch_when_disabled_or_replaced() {
    for calculation in [false, true] {
        for retire in [false, true] {
            let mut session = book();
            assert!(session.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Sheet {
                sheet: None, change: calc_host::SheetChange::Add { name: "Other".into() },
            })).accepted());
            let edits = session.host().model().sheets().iter().map(|sheet| CellEdit::Set {
                at: CellKey::new(sheet.id(), 1, 1).unwrap(),
                input: calc_host::interpret_entry(if calculation { "=1" } else { "1" }).unwrap().unwrap(),
            }).collect();
            let owner = Rc::new(StopDelivery { calculation, retire, delivered: RefCell::default() });
            if calculation {
                assert!(session.dispatch(SkinIntent::Edit { edits }).accepted());
                session.set_workbook_event_handler(Some(owner.clone()));
                assert!(session.dispatch(SkinIntent::Calculate {
                    scope: calc_host::CalcScope::Workbook, selection: calc_host::EvalSelection::ForceAll,
                }).accepted());
                while session.has_pending_calculation() { session.advance_calculation(); }
            } else {
                session.set_workbook_event_handler(Some(owner.clone()));
                assert!(session.dispatch(SkinIntent::Edit { edits }).accepted());
            }
            assert_eq!(owner.delivered.borrow().len(), 1, "calculation={calculation}, retire={retire}");
            assert!(session.workbook_event_failures().failures.is_empty());
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_automation_change_does_not_follow_a_parked_provider() {
    let mut session = book();
    let counts = Rc::new(RefCell::new(async_provider::Counts::default()));
    let mut functions = calc_host::CallableTable::new();
    functions.register("AsyncEcho", Rc::new(async_provider::Echo(counts.clone()))).unwrap();
    session.set_user_functions(functions);
    enter(&mut session, 1, "1");
    enter(&mut session, 2, "=AsyncEcho(A1)+2");
    session.set_workbook_events_enabled(false);
    session.dispatch(SkinIntent::SetCalcMode(CalcMode::Automatic));
    while session.has_pending_calculation() { session.advance_calculation(); }
    assert!(session.has_unfinished_calculation());
    let probe = Probe::new(&session);
    session.set_workbook_event_handler(Some(probe.clone()));
    session.set_workbook_events_enabled(true);
    let at = cell(&session, 1);
    let receipt = session.dispatch_from(calc_host::CommandIngress::Automation, SkinIntent::EnterCell { at, text: "9".into() });
    assert!(receipt.accepted()); // The authored edit stands even though calculation cannot finish.
    assert!(!session.has_pending_calculation());
    assert!(session.has_unfinished_calculation());
    assert!(probe.log.borrow().iter().all(|entry| !entry.starts_with("change")));
    let failures = session.workbook_event_failures();
    assert_eq!(failures.failures.len(), 1);
    assert!(failures.failures[0].reason.contains("terminal successful calculation"));
    assert_eq!(counts.borrow().prefix, 2); // One invocation before, one after the authored edit.
    assert_eq!(counts.borrow().suffix, 0);
}
impl Probe {
    fn new(book: &HostSession) -> Rc<Self> {
        Rc::new(Self {
            a: cell(book, 1),
            b: cell(book, 2),
            c: cell(book, 3),
            log: RefCell::default(),
            fail: Cell::new(false),
            cancel: Cell::new(false),
            save_edit: Cell::new(false),
        })
    }
}
impl WorkbookEventHandler for Probe {
    fn worksheet_change(
        &self,
        session: &mut HostSession,
        sheet: SheetId,
        target: &[GridArea],
    ) -> Result<(), String> {
        let host = session.host();
        assert_eq!(sheet, self.a.sheet());
        assert!(target.iter().all(|area| area.sheet() == sheet));
        self.log.borrow_mut().push(format!(
            "change {:?}/{:?}/{:?}",
            number(host, self.a),
            number(host, self.b),
            number(host, self.c)
        ));
        if self.fail.get() {
            Err("event owner failure".into())
        } else {
            Ok(())
        }
    }
    fn worksheet_calculate(&self, session: &mut HostSession, _: SheetId) -> Result<(), String> {
        let host = session.host();
        self.log.borrow_mut().push(format!(
            "calculate {:?}/{:?}",
            number(host, self.b),
            number(host, self.c)
        ));
        Ok(())
    }
    fn before_save(
        &self,
        _: &mut HostSession,
        operation: SaveOperation,
    ) -> Result<BeforeSaveOutcome, String> {
        self.log.borrow_mut().push(format!("save {operation:?}"));
        Ok(BeforeSaveOutcome {
            edits: if self.save_edit.replace(false) {
                vec![CellEdit::Set {
                    at: self.a,
                    input: CellInput::Number(FiniteNumber::new(19.0).unwrap()),
                }]
            } else {
                vec![]
            },
            cancel: self.cancel.get(),
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn change_precedes_automatic_calculation_and_calculate_waits_for_terminal_publication() {
    let mut book = book();
    enter(&mut book, 1, "1");
    enter(&mut book, 2, "=A1+1");
    enter(&mut book, 3, "=B1*2");
    enter(&mut book, 6, "=SUM(SEQUENCE(3000))+A1");
    assert!(book.dispatch(SkinIntent::Recalculate).accepted());
    assert!(
        book.dispatch(SkinIntent::SetCalcMode(CalcMode::Automatic))
            .accepted()
    );
    book.set_calculation_budget(WorkBudget::evaluations(1));
    book.subscribe(
        Interest::Area(GridArea::new(cell(&book, 2), cell(&book, 3)).unwrap()),
        Box::new(|_| {}),
    )
    .unwrap();
    let probe = Probe::new(&book);
    book.set_workbook_event_handler(Some(probe.clone()));
    enter(&mut book, 1, "10");
    assert_eq!(
        *probe.log.borrow(),
        ["change Some(10.0)/Some(2.0)/Some(4.0)"]
    );
    let mut partial = false;
    for _ in 0..1000 {
        if !book.has_pending_calculation() {
            break;
        }
        let progress = book.advance_calculation();
        if let HostProgress::Published(delta) = &progress
            && delta.completion().is_none()
        {
            partial = true;
            assert_eq!(
                probe.log.borrow().len(),
                1,
                "a prefix publication is not Worksheet_Calculate"
            );
        }
        assert!(!matches!(progress, HostProgress::Failed(_)), "{progress:?}");
    }
    assert!(!book.has_pending_calculation());
    assert!(
        partial,
        "exercise the application's early-publication route"
    );
    assert_eq!(
        *probe.log.borrow(),
        [
            "change Some(10.0)/Some(2.0)/Some(4.0)",
            "calculate Some(11.0)/Some(22.0)"
        ]
    );
    assert!(book.take_workbook_event_failures().failures.is_empty());

    book.dispatch(SkinIntent::SetCalcMode(CalcMode::Manual));
    probe.log.borrow_mut().clear();
    enter(&mut book, 1, "10");
    assert_eq!(
        probe.log.borrow().len(),
        1,
        "same-value writes still deliver Change"
    );
    book.dispatch(SkinIntent::Edit {
        edits: vec![CellEdit::SetFormat {
            at: cell(&book, 1),
            format: None,
        }],
    });
    assert_eq!(probe.log.borrow().len(), 1, "formatting is not Change");
    book.set_workbook_events_enabled(false);
    enter(&mut book, 1, "11");
    book.dispatch(SkinIntent::Recalculate);
    while book.has_pending_calculation() {
        book.advance_calculation();
    }
    assert_eq!(probe.log.borrow().len(), 1);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn registered_before_save_uses_the_existing_batch_pin_cancel_and_copy_rules() {
    let mut book = book();
    enter(&mut book, 1, "1");
    let probe = Probe::new(&book);
    probe.cancel.set(true);
    probe.save_edit.set(true);
    book.set_workbook_event_handler(Some(probe.clone()));
    let mut bytes = Vec::new();
    assert!(matches!(
        book.save(&SavePolicy::default(), &mut bytes),
        Err(SaveFailed::Cancelled)
    ));
    assert!(bytes.is_empty());
    assert_eq!(
        number(book.host(), probe.a),
        Some(19.0),
        "admitted edits survive cancellation"
    );
    assert_eq!(probe.log.borrow()[0], "save Save");
    assert_eq!(
        probe.log.borrow().len(),
        2,
        "one handler plus its authored Change"
    );
    let context = ExcelSaveContext {
        operation: SaveOperation::SaveCopyAs,
        calculate_before_save: false,
        active_sheet: probe.a.sheet(),
        disabled_sheets: &[],
        before_save: None,
    };
    book.prepare_save_with_context(&SavePolicy::default(), context)
        .unwrap()
        .write(&mut bytes)
        .unwrap();
    assert!(!bytes.is_empty());
    assert_eq!(probe.log.borrow().len(), 2, "SaveCopyAs skips BeforeSave");
    book.set_workbook_events_enabled(false);
    assert!(book.save(&SavePolicy::default(), &mut Vec::new()).is_ok());
    assert_eq!(probe.log.borrow().len(), 2);
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn failed_event_is_counted_once_without_rejecting_or_replaying_the_committed_edit() {
    let mut book = book();
    let probe = Probe::new(&book);
    probe.fail.set(true);
    book.set_workbook_event_handler(Some(probe.clone()));
    for value in 0..140 {
        enter(&mut book, 1, &value.to_string());
    }
    assert_eq!(number(book.host(), probe.a), Some(139.0));
    assert_eq!(probe.log.borrow().len(), 140);
    let failures = book.take_workbook_event_failures();
    assert_eq!(failures.failures.len() as u64 + failures.omitted, 140);
    assert!(
        failures
            .failures
            .iter()
            .all(|f| f.reason == "event owner failure")
    );
    assert!(book.take_workbook_event_failures().failures.is_empty());
}

/// P22 ordinary COM writes: entry order and observed values, separately from UI
/// edits. Each scenario has ten fresh-required Excel trials; no UI ordering is
/// inferred. Nested object writes remain outside the admitted handler surface.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn p22_recorded_com_change_observes_completed_automatic_values() {
    use calc_host::CommandIngress;
    let traces: [(&str, &[&str]); 3] = [
        ("manual-basic", &[
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/manual-basic/01.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/manual-basic/02.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/manual-basic/03.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/manual-basic/04.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/manual-basic/05.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/manual-basic/06.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/manual-basic/07.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/manual-basic/08.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/manual-basic/09.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/manual-basic/10.tsv"),
        ]),
        ("automatic-basic", &[
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-basic/01.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-basic/02.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-basic/03.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-basic/04.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-basic/05.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-basic/06.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-basic/07.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-basic/08.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-basic/09.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-basic/10.tsv"),
        ]),
        ("automatic-events-disabled", &[
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-events-disabled/01.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-events-disabled/02.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-events-disabled/03.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-events-disabled/04.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-events-disabled/05.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-events-disabled/06.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-events-disabled/07.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-events-disabled/08.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-events-disabled/09.tsv"),
            include_str!("../../calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/traces/automatic-events-disabled/10.tsv"),
        ]),
    ];
    for (name, trials) in traces {
        let mut expected_trials = Vec::new();
        for raw in trials {
            let rows: Vec<_> = raw.lines().map(|line| line.split('\t').collect::<Vec<_>>()).collect();
            let mut expected = Vec::new();
            for (i, row) in rows.iter().enumerate() {
                let [_, operation, event, ..] = row.as_slice() else { continue };
                if !matches!(*operation, "COM write A1=5" | "COM same-value rewrite" | "COM ClearContents A1") {
                    continue;
                }
                if !matches!(*event, "Worksheet_Change entry" | "Worksheet_Calculate entry") { continue; }
                let values: Vec<Option<f64>> = rows[i+1..].iter().take(3).map(|cell| {
                    assert_eq!(cell[2], "cell");
                    if let Some((_, bits)) = cell[3].split_once("bits=") {
                        Some(f64::from_bits(u64::from_str_radix(bits, 16).unwrap()))
                    } else {
                        assert!(cell[3].contains("type=Empty"));
                        None
                    }
                }).collect();
                expected.push(if *event == "Worksheet_Change entry" {
                    format!("change {:?}/{:?}/{:?}", values[0],values[1],values[2])
                } else {
                    format!("calculate {:?}/{:?}", values[1],values[2])
                });
            }
            expected_trials.push(expected);
        }
        for expected in &expected_trials { assert_eq!(expected, &expected_trials[0], "{name}: Excel trial disagreement"); }
        for budget in [usize::MAX, 1, 0] {
            let mut book = book();
            enter(&mut book, 1, "1");
            enter(&mut book, 2, "=A1+1");
            enter(&mut book, 3, "=B1*2");
            assert!(book.dispatch(SkinIntent::Recalculate).accepted());
            if name.starts_with("automatic") {
                assert!(book.dispatch(SkinIntent::SetCalcMode(CalcMode::Automatic)).accepted());
            }
            book.set_calculation_budget(WorkBudget::evaluations(budget));
            let probe = Probe::new(&book);
            book.set_workbook_event_handler(Some(probe.clone()));
            book.set_workbook_events_enabled(name != "automatic-events-disabled");
            for text in ["5", "5", "", "5"] {
                let receipt = book.dispatch_from(CommandIngress::Automation, SkinIntent::EnterCell {
                    at: probe.a, text: text.into(),
                });
                assert!(receipt.accepted(), "{receipt:?}");
                assert!(!book.has_pending_calculation());
                assert_eq!(book.intent_log().last().unwrap().ingress, CommandIngress::Automation);
            }
            assert_eq!(*probe.log.borrow(), expected_trials[0], "{name}, budget {budget}");
            assert!(book.workbook_event_failures().failures.is_empty());
        }
    }
}
