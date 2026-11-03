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
        host: &HostState,
        sheet: SheetId,
        target: &[GridArea],
    ) -> Result<(), String> {
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
    fn worksheet_calculate(&self, host: &HostState, _: SheetId) -> Result<(), String> {
        self.log.borrow_mut().push(format!(
            "calculate {:?}/{:?}",
            number(host, self.b),
            number(host, self.c)
        ));
        Ok(())
    }
    fn before_save(
        &self,
        _: &HostState,
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
