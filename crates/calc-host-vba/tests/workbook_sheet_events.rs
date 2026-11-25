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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_qrnem_after_save_false_on_failure_and_sheet_switch_no_selection_change() {
    use calc_host::value::CellKey;

    struct FailingWriter;
    impl calc_host::PackageWrite for FailingWriter {
        fn write_all(&mut self, _bytes: &[u8]) -> Result<(), calc_doc::SinkError> {
            Err(calc_doc::SinkError::new("disk full"))
        }
    }

    const BOOK_EVENTS: &str = "Private Sub Workbook_SheetActivate(ByVal Sh As Object)\n History = History & \"wbAct:\" & Sh.Name & \";\"\nEnd Sub\nPrivate Sub Workbook_SheetDeactivate(ByVal Sh As Object)\n History = History & \"wbDeact:\" & Sh.Name & \";\"\nEnd Sub\nPrivate Sub Workbook_SheetSelectionChange(ByVal Sh As Object, ByVal Target As Range)\n History = History & \"wbSel:\" & Sh.Name & \":\" & Target.Address(False, False) & \";\"\nEnd Sub\nPrivate Sub Workbook_AfterSave(ByVal Success As Boolean)\n History = History & \"after:\" & CStr(Success) & \";\"\nEnd Sub\n";
    let mut session = session();
    let sheet = session.host().sheets()[0].id();
    let project = calc_host_vba::prepare(
        session.host(),
        "book.xlsm",
        &[
            ModuleSource { name: "Api".into(), source: API.into(), target: ModuleTarget::Standard },
            ModuleSource { name: "Sheet1".into(), source: SHEET2.into(), target: ModuleTarget::Worksheet(sheet) },
            ModuleSource { name: "ThisWorkbook".into(), source: BOOK_EVENTS.into(), target: ModuleTarget::Workbook },
        ],
        10000,
        None,
    )
    .unwrap();
    project.clone().install(&mut session);

    // 1. Failing save must raise Workbook_AfterSave(False).
    let failed = session.save(&calc_host::SavePolicy::default(), &mut FailingWriter);
    assert!(failed.is_err(), "FailingWriter must fail save");
    let mut ok_bytes = Vec::new();
    session.save(&calc_host::SavePolicy::default(), &mut ok_bytes).unwrap();
    assert!(trace(&project).contains("after:False;after:True;"), "{}", trace(&project));

    // 2. Sheet switches must fire Deactivate/Activate and MUST NOT fire SelectionChange.
    assert!(
        session
            .dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Sheet {
                sheet: None,
                change: calc_host::SheetChange::Add { name: "S2".into() },
            }))
            .accepted()
    );
    let s2 = session.host().sheets()[1].id();
    let a1 = CellKey::new(sheet, 1, 1).unwrap();
    let b2 = CellKey::new(sheet, 2, 2).unwrap();
    let s2_a1 = CellKey::new(s2, 1, 1).unwrap();
    assert!(session.dispatch(SkinIntent::Select(calc_host::Selection::cell(a1))).accepted());
    let before_switch = trace(&project);
    assert!(session.dispatch(SkinIntent::Select(calc_host::Selection::cell(s2_a1))).accepted());
    assert!(session.dispatch(SkinIntent::Select(calc_host::Selection::cell(a1))).accepted());
    let after_switches = &trace(&project)[before_switch.len()..];
    assert_eq!(
        after_switches,
        "deact;wbDeact:Sheet1;wbAct:S2;wbDeact:S2;act;wbAct:Sheet1;",
        "sheet switches must not fire SelectionChange"
    );
    // Same-sheet selection move still fires SelectionChange.
    assert!(session.dispatch(SkinIntent::Select(calc_host::Selection::cell(b2))).accepted());
    let after_move = trace(&project);
    assert!(after_move.ends_with("sel:B2;wbSel:Sheet1:B2;"), "{after_move}");
    assert!(session.workbook_event_failures().failures.is_empty(), "{:?}", session.workbook_event_failures());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_qrnem_enable_events_false_in_sheet_handler_suppresses_workbook_event() {
    use calc_host::value::CellKey;

    const SHEET_SUPPRESS: &str = "Private Sub Worksheet_SelectionChange(ByVal Target As Range)\n History = History & \"sSel;\"\n Application.EnableEvents = False\nEnd Sub\nPrivate Sub Worksheet_Activate()\n History = History & \"sAct;\"\n Application.EnableEvents = False\nEnd Sub\nPrivate Sub Worksheet_Change(ByVal Target As Range)\n History = History & \"sChg;\"\n Application.EnableEvents = False\nEnd Sub\n";
    const BOOK_WATCH: &str = "Private Sub Workbook_SheetSelectionChange(ByVal Sh As Object, ByVal Target As Range)\n History = History & \"wSel;\"\nEnd Sub\nPrivate Sub Workbook_SheetActivate(ByVal Sh As Object)\n History = History & \"wAct;\"\nEnd Sub\nPrivate Sub Workbook_SheetChange(ByVal Sh As Object, ByVal Target As Range)\n History = History & \"wChg;\"\nEnd Sub\n";

    let mut session = session();
    let sheet = session.host().sheets()[0].id();
    let project = calc_host_vba::prepare(
        session.host(),
        "book.xlsm",
        &[
            ModuleSource { name: "Api".into(), source: API.into(), target: ModuleTarget::Standard },
            ModuleSource { name: "Sheet1".into(), source: SHEET_SUPPRESS.into(), target: ModuleTarget::Worksheet(sheet) },
            ModuleSource { name: "ThisWorkbook".into(), source: BOOK_WATCH.into(), target: ModuleTarget::Workbook },
        ],
        10000,
        None,
    )
    .unwrap();
    project.clone().install(&mut session);
    assert!(
        session
            .dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Sheet {
                sheet: None,
                change: calc_host::SheetChange::Add { name: "S2".into() },
            }))
            .accepted()
    );
    let s2 = session.host().sheets()[1].id();
    let a1 = CellKey::new(sheet, 1, 1).unwrap();
    let s2_a1 = CellKey::new(s2, 1, 1).unwrap();

    // 1. Worksheet_SelectionChange disables events -> Workbook_SheetSelectionChange must NOT fire.
    assert!(session.dispatch(SkinIntent::Select(calc_host::Selection::cell(a1))).accepted());
    assert_eq!(trace(&project), "sSel;");
    // 2. Worksheet_Change disables events -> Workbook_SheetChange must NOT fire.
    session.set_workbook_events_enabled(true);
    assert!(session.dispatch(SkinIntent::EnterCell { at: a1, text: "42".into() }).accepted());
    assert_eq!(trace(&project), "sSel;sChg;");
    // 3. Worksheet_Activate disables events -> Workbook_SheetActivate must NOT fire.
    assert!(session.dispatch(SkinIntent::Select(calc_host::Selection::cell(s2_a1))).accepted());
    session.set_workbook_events_enabled(true);
    assert!(session.dispatch(SkinIntent::Select(calc_host::Selection::cell(a1))).accepted());
    assert_eq!(trace(&project), "sSel;sChg;sAct;");
    assert!(session.workbook_event_failures().failures.is_empty(), "{:?}", session.workbook_event_failures());
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_qrnem_chart_sheet_events_pass_chart_object() {
    const BOOK_CHART: &str = "Private Sub Workbook_NewSheet(ByVal Sh As Object)\n History = History & \"new:\" & TypeName(Sh) & \":\" & Sh.Name & \";\"\nEnd Sub\nPrivate Sub Workbook_SheetActivate(ByVal Sh As Object)\n History = History & \"act:\" & TypeName(Sh) & \":\" & Sh.Name & \";\"\nEnd Sub\nPrivate Sub Workbook_SheetDeactivate(ByVal Sh As Object)\n History = History & \"deact:\" & TypeName(Sh) & \":\" & Sh.Name & \";\"\nEnd Sub\n";
    const TRIGGER: &str = "Private Sub Worksheet_Change(ByVal Target As Range)\n ThisWorkbook.Charts.Add\n ThisWorkbook.Worksheets(1).Activate\nEnd Sub\n";
    let mut session = session();
    let sheet1 = session.host().sheets()[0].id();
    let project = calc_host_vba::prepare(
        session.host(),
        "book.xlsm",
        &[
            ModuleSource { name: "Api".into(), source: API.into(), target: ModuleTarget::Standard },
            ModuleSource { name: "Sheet1".into(), source: TRIGGER.into(), target: ModuleTarget::Worksheet(sheet1) },
            ModuleSource { name: "ThisWorkbook".into(), source: BOOK_CHART.into(), target: ModuleTarget::Workbook },
        ],
        100_000,
        None,
    )
    .unwrap();
    project.clone().install(&mut session);
    let a1 = calc_host::value::CellKey::new(sheet1, 1, 1).unwrap();
    assert!(session.dispatch(SkinIntent::Select(calc_host::Selection::cell(a1))).accepted());
    assert!(session.dispatch(SkinIntent::EnterCell { at: a1, text: "1".into() }).accepted());
    assert!(session.workbook_event_failures().failures.is_empty(), "{:?}", session.workbook_event_failures());
    let history = trace(&project);
    assert_eq!(
        history,
        "new:Chart:Chart1;deact:Worksheet:Sheet1;act:Chart:Chart1;deact:Chart:Chart1;act:Worksheet:Sheet1;"
    );
}
