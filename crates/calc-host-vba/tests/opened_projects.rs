//! Exercise the same composition used before the worker's first calculation.
use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, SkinIntent};
use std::sync::Arc;
#[path = "../../calc-doc/tests/cdoc_m1_1_support/mod.rs"]
#[allow(dead_code)]
mod support;
#[path = "../../calc-doc/tests/cdoc_m1_1_support/vba_project.rs"]
mod vba_project;

const LOCAL: &[u8] = include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsm");
const STATE: &str = "Public Opens As Long\nPublic Changes As Long\nPublic Calcs As Long\nPublic Saves As Long\nPublic Function Probe() As String\n Probe = CStr(Opens) & \":\" & CStr(Changes) & \":\" & CStr(Calcs) & \":\" & CStr(Saves)\nEnd Function\n";
const SHEET: &str = "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Changes = Changes + 1\nEnd Sub\nPrivate Sub Worksheet_Calculate()\n Calcs = Calcs + 1\nEnd Sub\n";
const BOOK: &str = "Private Sub Workbook_Open()\n Opens = Opens + 1\nEnd Sub\nPrivate Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)\n Saves = Saves + 1\n Cancel = True\nEnd Sub\n";

fn backends() -> Vec<calc_vba::Backend> {
    let mut backends = vec![calc_vba::Backend::Interpreter];
    #[cfg(not(target_arch = "wasm32"))]
    backends.push(calc_vba::Backend::Jit);
    backends
}

fn enter(session: &mut HostSession, row: u32, column: u32, text: &str) {
    let at = calc_host::value::CellKey::new(session.host().sheets()[0].id(), row, column).unwrap();
    assert!(
        session
            .dispatch(SkinIntent::EnterCell {
                at,
                text: text.into()
            })
            .accepted()
    );
}

fn full(session: &mut HostSession) {
    assert!(
        session
            .dispatch(SkinIntent::Calculate {
                scope: calc_host::CalcScope::Workbook,
                selection: calc_host::EvalSelection::ForceAll,
            })
            .accepted()
    );
    drain(session);
}

fn drain(session: &mut HostSession) {
    for _ in 0..1000 {
        if !session.has_pending_calculation() {
            return;
        }
        let progress = session.advance_calculation();
        assert!(
            !matches!(
                progress,
                calc_host::HostProgress::Failed(_) | calc_host::HostProgress::Waiting { .. }
            ),
            "{progress:?}"
        );
    }
    panic!("file project did not complete");
}

fn value(session: &HostSession, row: u32, column: u32) -> calc_value::CalcValue {
    let at = calc_host::value::CellKey::new(session.host().sheets()[0].id(), row, column).unwrap();
    let page = calc_host::project(
        session.host(),
        &calc_host::ProjectionRequest { cells: vec![at] },
    )
    .unwrap();
    match page.cells()[0].value() {
        calc_host::ProjectedValue::Calculated { value, .. } => value.clone(),
        other => panic!("expected an engine calculation at {at:?}: {other:?}"),
    }
}

fn opened(bytes: &[u8]) -> HostSession {
    HostSession::open(
        Arc::new(MemorySource::new(bytes.to_vec())),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap()
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn file_project_sources_compile_with_their_exported_attributes() {
    for bytes in [
        include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsm").as_slice(),
        include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsb").as_slice(),
    ] {
        for backend in backends() {
            let mut session = opened(bytes);
            calc_host_vba::activate_file_project(
                &mut session,
                "local.xlsm",
                100_000,
                Some(backend),
            )
            .unwrap();
            assert!(session.dispatch(SkinIntent::Recalculate).accepted());
            drain(&mut session);
            for (row, expected) in [(1, 102.), (2, 102.), (3, 102.), (4, 102.)] {
                assert_eq!(
                    value(&session, row, 2).as_number(),
                    Some(expected),
                    "{backend:?}"
                );
            }
            enter(&mut session, 1, 4, "=PrivateModule.HiddenFunc(3)");
            session.dispatch(SkinIntent::Recalculate);
            drain(&mut session);
            assert_eq!(value(&session, 1, 4).as_number(), Some(1303.));
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn file_project_events_share_state_and_reopen_starts_a_new_session() {
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            ("Module1", STATE),
            ("PrivateModule", "Option Private Module\n"),
            ("Sheet1", SHEET),
            ("ThisWorkbook", BOOK),
        ],
    );
    for backend in backends() {
        let mut session = opened(&bytes);
        calc_host_vba::activate_file_project(&mut session, "file.xlsm", 100_000, Some(backend))
            .unwrap();
        session.notify_workbook_open(); // Repeated owner delivery does not enter the handler again.
        enter(&mut session, 1, 4, "=Probe()");
        session.dispatch(SkinIntent::Recalculate);
        drain(&mut session);
        assert_eq!(
            value(&session, 1, 4).as_text().unwrap().to_string_lossy(),
            "1:1:0:0"
        );
        full(&mut session);
        assert_eq!(
            value(&session, 1, 4).as_text().unwrap().to_string_lossy(),
            "1:1:1:0"
        );
        let mut saved = Vec::new();
        assert!(matches!(
            session.save(&Default::default(), &mut saved),
            Err(calc_host::SaveFailed::Cancelled)
        ));
        assert!(saved.is_empty());
        full(&mut session);
        assert_eq!(
            value(&session, 1, 4).as_text().unwrap().to_string_lossy(),
            "1:1:2:1"
        );
        assert!(session.take_workbook_event_failures().failures.is_empty());
        let mut reopened = opened(&bytes);
        calc_host_vba::activate_file_project(&mut reopened, "file.xlsm", 100_000, Some(backend))
            .unwrap();
        enter(&mut reopened, 1, 4, "=Probe()");
        reopened.dispatch(SkinIntent::Recalculate);
        drain(&mut reopened);
        assert_eq!(
            value(&reopened, 1, 4).as_text().unwrap().to_string_lossy(),
            "1:1:0:0"
        );
    }
}
