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
