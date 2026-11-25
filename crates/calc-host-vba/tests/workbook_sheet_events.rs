//! Workbook-level sheet events (`Workbook_SheetChange`, `Workbook_SheetCalculate`) run through the
//! same composition as the sheet events (dc-d7cgp).
use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, SkinIntent};
use calc_host_vba::{ModuleSource, ModuleTarget};
use std::sync::Arc;

const API: &str = "Public History As String\nPublic Function Trace() As String\n Trace = History\nEnd Function\n";
const BOOK: &str = "Private Sub Workbook_SheetChange(ByVal Sh As Object, ByVal Target As Range)\n History = History & \"change:\" & Sh.Name & \":\" & Target.Address(False, False) & \";\"\nEnd Sub\nPrivate Sub Workbook_SheetCalculate(ByVal Sh As Object)\n History = History & \"calc:\" & Sh.Name & \";\"\nEnd Sub\n";
const SHEET: &str = "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n History = History & \"sheet;\"\nEnd Sub\n";

fn session() -> HostSession {
    HostSession::open(
        Arc::new(MemorySource::new(calc_host::blank_package()).named("book.xlsx")),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap()
}

fn drain(session: &mut HostSession) {
    for _ in 0..1000 {
        if !session.has_pending_calculation() {
            return;
        }
        let progress = session.advance_calculation();
        assert!(!matches!(
            progress,
            calc_host::HostProgress::Failed(_) | calc_host::HostProgress::Waiting { .. }
        ));
    }
    panic!("calculation did not complete");
}

fn trace(project: &calc_host_vba::WorkbookEvents) -> String {
    project.functions().call("Trace", &[]).unwrap().as_text().unwrap().to_string_lossy()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn workbook_sheet_change_and_calculate_follow_the_sheet_event() {
    let mut session = session();
    let sheet = session.host().sheets()[0].id();
    let project = calc_host_vba::prepare(
        session.host(),
        "book.xlsm",
        &[
            ModuleSource { name: "Api".into(), source: API.into(), target: ModuleTarget::Standard },
            ModuleSource { name: "Sheet1".into(), source: SHEET.into(), target: ModuleTarget::Worksheet(sheet) },
            ModuleSource { name: "ThisWorkbook".into(), source: BOOK.into(), target: ModuleTarget::Workbook },
        ],
        10000,
        None,
    )
    .unwrap();
    project.clone().install(&mut session);
    let at = calc_host::value::CellKey::new(sheet, 1, 1).unwrap();
    assert!(session.dispatch(SkinIntent::EnterCell { at, text: "5".into() }).accepted());
    let b1 = calc_host::value::CellKey::new(sheet, 1, 2).unwrap();
    assert!(session.dispatch(SkinIntent::EnterCell { at: b1, text: "=A1+1".into() }).accepted());
    drain(&mut session);
    assert!(session.workbook_event_failures().failures.is_empty(), "{:?}", session.workbook_event_failures());
    let history = trace(&project);
    assert!(history.starts_with("sheet;change:Sheet1:A1;"), "{history}");
    assert!(history.contains("change:Sheet1:B1;"), "{history}");
    assert!(history.contains("calc:Sheet1;"), "{history}");
}

const BOOK2: &str = "Private Sub Workbook_SheetActivate(ByVal Sh As Object)\n History = History & \"wbAct:\" & Sh.Name & \";\"\nEnd Sub\nPrivate Sub Workbook_SheetDeactivate(ByVal Sh As Object)\n History = History & \"wbDeact:\" & Sh.Name & \";\"\nEnd Sub\nPrivate Sub Workbook_NewSheet(ByVal Sh As Object)\n History = History & \"new:\" & Sh.Name & \";\"\nEnd Sub\nPrivate Sub Workbook_AfterSave(ByVal Success As Boolean)\n History = History & \"after:\" & CStr(Success) & \";\"\nEnd Sub\nPrivate Sub Workbook_BeforeClose(Cancel As Boolean)\n History = History & \"close;\"\n Cancel = True\nEnd Sub\n";
const SHEET2: &str = "Private Sub Worksheet_Activate()\n History = History & \"act;\"\nEnd Sub\nPrivate Sub Worksheet_Deactivate()\n History = History & \"deact;\"\nEnd Sub\nPrivate Sub Worksheet_SelectionChange(ByVal Target As Range)\n History = History & \"sel:\" & Target.Address(False, False) & \";\"\nEnd Sub\n";

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_27_activation_new_sheet_after_save_and_before_close_events_fire() {
    use calc_host::value::CellKey;
    let mut session = session();
    let sheet = session.host().sheets()[0].id();
    let project = calc_host_vba::prepare(
        session.host(),
        "book.xlsm",
        &[
            ModuleSource { name: "Api".into(), source: API.into(), target: ModuleTarget::Standard },
            ModuleSource { name: "Sheet1".into(), source: SHEET2.into(), target: ModuleTarget::Worksheet(sheet) },
            ModuleSource { name: "ThisWorkbook".into(), source: BOOK2.into(), target: ModuleTarget::Workbook },
        ],
        10000,
        None,
    )
    .unwrap();
    project.clone().install(&mut session);
    // Moving to another sheet: the left sheet's Deactivate (sheet, then workbook), then the entered one's.
    let add = session.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Sheet {
        sheet: None,
        change: calc_host::SheetChange::Add { name: "S2".into() },
    }));
    assert!(add.accepted(), "{add:?}");
    let s2 = session.host().sheets()[1].id();
    let at = CellKey::new(s2, 1, 1).unwrap();
    let a1 = CellKey::new(sheet, 1, 1).unwrap();
    assert!(session.dispatch(SkinIntent::Select(calc_host::Selection::cell(a1))).accepted());
    assert!(session.dispatch(SkinIntent::Select(calc_host::Selection::cell(at))).accepted());
    assert!(session.dispatch(SkinIntent::Select(calc_host::Selection::cell(a1))).accepted());
    session.notify_after_save(true);
    assert!(session.notify_before_close(), "handler set Cancel");
    assert!(session.workbook_event_failures().failures.is_empty(), "{:?}", session.workbook_event_failures());
    let history = trace(&project);
    // Sheet1 is current; leaving it for S2 deactivates it (sheet, then workbook), then S2 activates.
    let first = history.find("deact;wbDeact:Sheet1;wbAct:S2;").unwrap_or_else(|| panic!("{history}"));
    // Coming back: S2 has no module, so only the workbook's events; Sheet1's own Activate follows.
    assert!(history[first..].contains("wbDeact:S2;act;wbAct:Sheet1;"), "{history}");
    assert!(history.contains("sel:A1;"), "{history}");
    assert!(history.ends_with("after:True;close;"), "{history}");
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_27_sheets_add_from_a_macro_raises_new_sheet_then_the_activation_events() {
    let mut session = session();
    let sheet = session.host().sheets()[0].id();
    let own = "Private Sub Worksheet_Change(ByVal Target As Range)\n ThisWorkbook.Worksheets.Add\nEnd Sub\n";
    let project = calc_host_vba::prepare(
        session.host(),
        "book.xlsm",
        &[
            ModuleSource { name: "Api".into(), source: API.into(), target: ModuleTarget::Standard },
            ModuleSource { name: "Sheet1".into(), source: own.into(), target: ModuleTarget::Worksheet(sheet) },
            ModuleSource { name: "ThisWorkbook".into(), source: BOOK2.into(), target: ModuleTarget::Workbook },
        ],
        10000,
        None,
    )
    .unwrap();
    project.clone().install(&mut session);
    let a1 = calc_host::value::CellKey::new(sheet, 1, 1).unwrap();
    assert!(session.dispatch(SkinIntent::Select(calc_host::Selection::cell(a1))).accepted());
    assert!(session.dispatch(SkinIntent::EnterCell { at: a1, text: "1".into() }).accepted());
    assert!(session.workbook_event_failures().failures.is_empty(), "{:?}", session.workbook_event_failures());
    assert_eq!(session.host().sheets().len(), 2);
    let history = trace(&project);
    assert!(history.starts_with("new:Sheet2;wbDeact:Sheet1;wbAct:Sheet2;"), "{history}");
}
