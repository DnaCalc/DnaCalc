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

/// The person enabled macros for these local test files.
fn grant() -> calc_host_vba::ProjectGrant {
    calc_host_vba::ProjectGrant::person_enabled(calc_host_vba::FileOrigin::Local).unwrap()
}

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

struct CallerOnlyContext(calc_value::CellKey);
impl calc_func::UserFunctionContext for CallerOnlyContext {
    fn caller(&self) -> Option<calc_value::CellKey> {
        Some(self.0)
    }
    fn reference_qualifier(&mut self, _: calc_value::SheetId) -> Result<String, String> {
        Ok("'[probe.xlsm]S'".into())
    }
    fn refuse(&mut self, reason: &'static str) -> String {
        panic!("an ordinary fresh-project source error needs no host capability: {reason}");
    }
    fn values_of(
        &mut self,
        _: &calc_value::ReferenceLike,
    ) -> Result<calc_value::CalcValue, String> {
        panic!("the original source must fail before a formula value read");
    }
    fn worksheet_function(
        &mut self,
        _: &str,
        _: &[calc_value::CalcValue],
    ) -> Result<calc_value::CalcValue, String> {
        panic!("the original source makes no worksheet callback before its error");
    }
}

fn opened(bytes: &[u8], name: &str) -> HostSession {
    HostSession::open(
        Arc::new(MemorySource::new(bytes.to_vec()).named(name)),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .unwrap()
}

/// Excel's retained local workbook fixture calls MyFunc through its own workbook index.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn local_workbook_qualified_formulas_use_the_installed_vba_project() {
    for bytes in [
        LOCAL,
        include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsb").as_slice(),
    ] {
        let mut session = opened(bytes, "local.xlsm");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        session.set_workbook_events_enabled(false);
        calc_host_vba::activate_file_project(
            &grant(),
            &mut session,
            "local.xlsm",
            100_000,
            Some(calc_vba::Backend::Interpreter),
        )
        .unwrap();
        full(&mut session);
        for row in [1, 2, 3, 4] {
            assert_eq!(value(&session, row, 2).as_number(), Some(102.0), "B{row}");
        }
    }
}

/// An already admitted provider can be bound under the consumer's retained link target.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn linked_workbook_qualified_formulas_use_exact_provider_registrations() {
    let provider = opened(
        include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/provider.xlsm"),
        "provider.xlsm",
    );
    let project = calc_host_vba::prepare_file_project_with_events(
        &grant(),
        provider.host(),
        "provider.xlsm",
        100_000,
        Some(calc_vba::Backend::Interpreter),
        false,
    )
    .unwrap()
    .unwrap();
    let exports = project.functions();
    for bytes in [
        include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/consumer.xlsx").as_slice(),
        include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/consumer.xlsb").as_slice(),
    ] {
        let mut consumer = opened(bytes, "consumer.xlsx");
        consumer.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        let path = consumer
            .host()
            .model()
            .definitions()
            .external_book(1)
            .unwrap()
            .absolute_target
            .as_deref()
            .unwrap();
        let mut registry = calc_host::CallableTable::new();
        for procedure in ["OtherFunc", "Module1.OtherFunc", "HiddenOther", "PrivateOther"] {
            let function = exports.resolve(exports.bind(None, procedure).unwrap()).unwrap();
            registry
                .register_for(Some(path), procedure, function.clone())
                .unwrap();
        }
        consumer.set_user_functions(registry);
        full(&mut consumer);
        for (row, expected) in [(1, 2002.0), (2, 2002.0), (4, 2102.0), (5, 2202.0)] {
            assert_eq!(value(&consumer, row, 2).as_number(), Some(expected), "B{row}");
        }
    }
}

fn mutable_owner(
    session: &mut HostSession,
    sheet_source: &str,
    api_source: &str,
    max_steps: u64,
    backend: calc_vba::Backend,
) -> std::rc::Rc<calc_host_vba::WorkbookEvents> {
    use calc_host_vba::{ModuleSource, ModuleTarget};
    let sheet = session.host().sheets()[0].id();
    let project = calc_host_vba::prepare(
        session.host(),
        "owner.xlsm",
        &[
            ModuleSource {
                name: "Api".into(),
                source: api_source.into(),
                target: ModuleTarget::Standard,
            },
            ModuleSource {
                name: "Sheet1".into(),
                source: sheet_source.into(),
                target: ModuleTarget::Worksheet(sheet),
            },
        ],
        max_steps,
        Some(backend),
    )
    .unwrap();
    project.clone().install(session);
    project
}

const OWNER_STATE: &str = "Public History As String\nPublic Entries As Long\nPublic Function Trace() As String\n Trace = History\nEnd Function\nPublic Function EntryCount() As Long\n EntryCount = Entries\nEnd Function\nPublic Function Twice(ByVal x As Double) As Double\n Twice = x * 2\nEnd Function\n";

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_bare_application_literal_members_and_cvar_use_the_real_owner() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        enter(&mut session, 1, 2, "=D1+1");
        let project = mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n If Target.Column <> 1 Then Exit Sub\n Application.EnableEvents = False\n Application.Range(\"D1\").Value2 = 9&\n Application.Range(\"E1\").Value2 = CVar(\"text\")\n Application.CalculateFull\n History = CStr(Application.Range(\"D1\").Value2) & \":\" & CStr(Application.Range(\"E1\").Value2)\n Application.EnableEvents = True\nEnd Sub\n",
            OWNER_STATE,
            10000,
            backend,
        );
        enter(&mut session, 1, 1, "7");
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{:?}",
            session.workbook_event_failures()
        );
        assert!(session.workbook_events_enabled());
        assert_eq!(value(&session, 1, 2).as_number(), Some(10.0));
        assert_eq!(
            project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "9:text"
        );
        let sheet = session.host().sheets()[0].id();
        assert!(
            matches!(session.host().model().cell(calc_value::CellKey::new(sheet, 1, 4).unwrap()),
            Some(calc_host::CellInput::Number(n)) if n.get() == 9.0)
        );
        assert!(
            matches!(session.host().model().cell(calc_value::CellKey::new(sheet, 1, 5).unwrap()),
            Some(calc_host::CellInput::Text(text)) if text.text().to_string_lossy() == "text")
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_nested_unhandled_fault_aborts_the_caller_and_releases_the_next_event() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        let project = mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim app As Excel.Application\n Set app = Application\n If Target.Column = 1 Then\n History = History & \"outer>\"\n On Error Resume Next\n app.Range(\"D1\").Value2 = Target.Value2\n History = History & \"after;\"\n ElseIf Target.Column = 4 Then\n Entries = Entries + 1\n Err.Raise 5\n Else\n History = History & \"clean;\"\n End If\nEnd Sub\n",
            OWNER_STATE,
            10000,
            backend,
        );
        enter(&mut session, 1, 1, "9");
        assert_eq!(
            project
                .functions()
                .call("EntryCount", &[])
                .unwrap()
                .as_number(),
            Some(1.0)
        );
        assert_eq!(
            project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "outer>"
        );
        let failures = session.take_workbook_event_failures();
        assert_eq!(failures.failures.len(), 2, "{:?}", failures);
        assert!(
            failures
                .failures
                .iter()
                .all(|f| f.reason.contains("5:") && f.reason.contains("dc-hsa5.12"))
        );
        let at = calc_value::CellKey::new(session.host().sheets()[0].id(), 1, 4).unwrap();
        assert!(
            matches!(session.host().model().cell(at), Some(calc_host::CellInput::Number(n)) if n.get() == 9.0)
        );
        enter(&mut session, 1, 2, "2");
        assert!(session.workbook_event_failures().failures.is_empty());
        assert_eq!(
            project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "outer>clean;"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_before_save_commits_nested_writes_before_cancel_without_replaying_them() {
    use calc_host_vba::{ModuleSource, ModuleTarget};
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        let sheet = session.host().sheets()[0].id();
        let api = format!(
            "{OWNER_STATE}Public SaveValue As Variant\nPublic Function SetSaveValue(ByVal x As Variant) As Long\n SaveValue = x\n SetSaveValue = 1\nEnd Function\n"
        );
        let project = calc_host_vba::prepare(session.host(), "owner.xlsm", &[
            ModuleSource { name: "Api".into(), source: api, target: ModuleTarget::Standard },
            ModuleSource { name: "Sheet1".into(), source: "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n History = History & CStr(Target.Value2) & \";\"\nEnd Sub\n".into(), target: ModuleTarget::Worksheet(sheet) },
            ModuleSource { name: "ThisWorkbook".into(), source: "Private Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)\n Dim app As Excel.Application\n Set app = Application\n Entries = Entries + 1\n app.Range(\"A1\").Value2 = SaveValue\n Cancel = True\nEnd Sub\n".into(), target: ModuleTarget::Workbook },
        ], 10000, Some(backend)).unwrap();
        project.clone().install(&mut session);
        project
            .functions()
            .call("SetSaveValue", &[calc_value::CalcValue::number(19.0)])
            .unwrap();
        let result = session.prepare_save_with_context(
            &Default::default(),
            calc_host::ExcelSaveContext {
                operation: calc_host::SaveOperation::Save,
                calculate_before_save: false,
                active_sheet: sheet,
                disabled_sheets: &[],
                before_save: None,
            },
        );
        assert!(matches!(result, Err(calc_host::SaveFailed::Cancelled)));
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{:?}",
            session.workbook_event_failures()
        );
        assert_eq!(
            project
                .functions()
                .call("EntryCount", &[])
                .unwrap()
                .as_number(),
            Some(1.0)
        );
        assert_eq!(
            project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "19;"
        );
        let at = calc_value::CellKey::new(sheet, 1, 1).unwrap();
        assert!(
            matches!(session.host().model().cell(at), Some(calc_host::CellInput::Number(n)) if n.get() == 19.0)
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_reentered_udf_cannot_write_a_saved_event_target_through_iferror() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        enter(&mut session, 1, 2, "=IFERROR(Bad(),0)");
        let api = format!(
            "{OWNER_STATE}Public Saved As Excel.Range\nPublic Function Bad() As Double\n On Error Resume Next\n Saved.Value2 = Saved.Value2 + 100\n Bad = 1\nEnd Function\n"
        );
        mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim app As Excel.Application\n Set app = Application\n Set Saved = Target\n app.Calculate\nEnd Sub\n",
            &api,
            10000,
            backend,
        );
        enter(&mut session, 1, 1, "9");
        let sheet = session.host().sheets()[0].id();
        let a = calc_value::CellKey::new(sheet, 1, 1).unwrap();
        let b = calc_value::CellKey::new(sheet, 1, 2).unwrap();
        assert!(
            matches!(session.host().model().cell(a), Some(calc_host::CellInput::Number(n)) if n.get() == 9.0)
        );
        let page = calc_host::project(
            session.host(),
            &calc_host::ProjectionRequest { cells: vec![b] },
        )
        .unwrap();
        assert!(
            matches!(
                page.cells()[0].value(),
                calc_host::ProjectedValue::NotModeled { .. }
            ),
            "{:?}",
            page.cells()[0].value()
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_multicell_scalar_write_is_one_nested_change() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        let project = mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim app As Excel.Application\n Set app = Application\n If Target.Column = 1 Then\n History = \"outer>\"\n app.Range(\"D1:E2\").Value2 = Target.Value2\n History = History & \"returned;\"\n Else\n Entries = Entries + 1\n History = History & CStr(Target.Count) & \":\" & Target.Address & \";\"\n End If\nEnd Sub\n",
            OWNER_STATE,
            10000,
            backend,
        );
        enter(&mut session, 1, 1, "9");
        assert_eq!(
            project
                .functions()
                .call("EntryCount", &[])
                .unwrap()
                .as_number(),
            Some(1.0)
        );
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{:?}",
            session.workbook_event_failures()
        );
        assert_eq!(
            project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "outer>4:$D$1:$E$2;returned;"
        );
        for row in [1, 2] {
            for col in [4, 5] {
                let at =
                    calc_value::CellKey::new(session.host().sheets()[0].id(), row, col).unwrap();
                assert!(
                    matches!(session.host().model().cell(at), Some(calc_host::CellInput::Number(n)) if n.get() == 9.0)
                );
            }
        }
        session.set_workbook_events_enabled(false);
        assert!(session.dispatch(SkinIntent::Undo).accepted());
        for row in [1, 2] {
            for col in [4, 5] {
                let at =
                    calc_value::CellKey::new(session.host().sheets()[0].id(), row, col).unwrap();
                assert!(
                    session.host().model().cell(at).is_none(),
                    "one Undo restores the whole rectangle"
                );
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_multicell_shape_refusal_is_atomic_and_survives_error_handling() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        enter(&mut session, 1, 10, "1");
        enter(&mut session, 1, 11, "2");
        let project = mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim app As Excel.Application\n Set app = Application\n On Error Resume Next\n app.Range(\"D1:E2\").Value2 = app.Range(\"J1:K1\").Value2\n Entries = Entries + 1\nEnd Sub\n",
            OWNER_STATE,
            10000,
            backend,
        );
        enter(&mut session, 1, 1, "9");
        assert_eq!(
            project
                .functions()
                .call("EntryCount", &[])
                .unwrap()
                .as_number(),
            Some(1.0)
        );
        let failures = session.workbook_event_failures();
        assert_eq!(failures.failures.len(), 1);
        assert!(
            failures.failures[0].reason.contains("dc-hsa5.12")
                && failures.failures[0].reason.contains("shape")
        );
        for row in [1, 2] {
            for col in [4, 5] {
                let at =
                    calc_value::CellKey::new(session.host().sheets()[0].id(), row, col).unwrap();
                assert!(session.host().model().cell(at).is_none());
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_multicell_array_write_preserves_types_and_one_target() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        for (row, col, text) in [
            (1, 10, "3"),
            (1, 11, "'text"),
            (2, 10, "TRUE"),
            (2, 11, "#N/A"),
        ] {
            enter(&mut session, row, col, text);
        }
        let project = mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim app As Excel.Application\n Set app = Application\n If Target.Column = 1 Then\n app.Range(\"D1:E2\").Value2 = app.Range(\"J1:K2\").Value2\n Else\n Entries = Entries + 1\n History = CStr(Target.Count) & \":\" & Target.Address\n End If\nEnd Sub\n",
            OWNER_STATE,
            10000,
            backend,
        );
        enter(&mut session, 1, 1, "9");
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{:?}",
            session.workbook_event_failures()
        );
        assert_eq!(
            project
                .functions()
                .call("EntryCount", &[])
                .unwrap()
                .as_number(),
            Some(1.0)
        );
        assert_eq!(
            project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "4:$D$1:$E$2"
        );
        let sheet = session.host().sheets()[0].id();
        for row in [1, 2] {
            for col in [4, 5] {
                let at = calc_value::CellKey::new(sheet, row, col).unwrap();
                let source = calc_value::CellKey::new(sheet, row, col + 6).unwrap();
                assert_eq!(
                    session.host().model().cell(at),
                    session.host().model().cell(source)
                );
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_multicell_empty_and_late_invalid_elements_keep_atomicity() {
    for backend in backends() {
        for invalid in [false, true] {
            let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
            session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
            for row in [1, 2] {
                for col in [4, 5] {
                    enter(&mut session, row, col, "31");
                }
            }
            let body = if invalid {
                format!(
                    " Dim text As String\n Dim i As Long\n text = \"{}\"\n For i = 1 To 7\n text = text & text\n Next i\n Payload(2,2) = text\n On Error Resume Next\n app.Range(\"D1:E2\").Value2 = Payload\n",
                    "x".repeat(256)
                )
            } else {
                " Dim emptyValue As Variant\n app.Range(\"D1:E2\").Value2 = emptyValue\n".into()
            };
            let source = format!(
                "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim app As Excel.Application\n Set app = Application\n If Target.Column = 1 Then\n{body} Entries = Entries + 1\n Else\n History = CStr(Target.Count)\n End If\nEnd Sub\n"
            );
            let api = format!(
                "{OWNER_STATE}Public Payload As Variant\nPublic Function SetPayload(ByVal x As Variant) As Long\n Payload = x\n SetPayload = 1\nEnd Function\n"
            );
            let project = mutable_owner(&mut session, &source, &api, 10000, backend);
            if invalid {
                let result = project
                    .functions()
                    .call(
                        "SetPayload",
                        &[calc_value::CalcValue::array(
                            calc_value::CalcArray::from_rows(vec![
                                vec![
                                    calc_value::CalcValue::number(1.0),
                                    calc_value::CalcValue::number(2.0),
                                ],
                                vec![
                                    calc_value::CalcValue::number(3.0),
                                    calc_value::CalcValue::empty(),
                                ],
                            ])
                            .unwrap(),
                        )],
                    )
                    .unwrap();
                assert_eq!(result.as_number(), Some(1.0));
            }
            let revision = session.host().revision().get();
            enter(&mut session, 1, 1, "9");
            assert_eq!(
                project
                    .functions()
                    .call("EntryCount", &[])
                    .unwrap()
                    .as_number(),
                Some(1.0)
            );
            if invalid {
                let failures = session.workbook_event_failures();
                assert_eq!(failures.failures.len(), 1);
                assert!(
                    failures.failures[0].reason.contains("text limit"),
                    "{failures:?}"
                );
                assert_eq!(
                    session.host().revision().get(),
                    revision + 1,
                    "no Automation transaction on invalid input"
                );
            } else {
                assert!(session.workbook_event_failures().failures.is_empty());
                assert_eq!(
                    project
                        .functions()
                        .call("Trace", &[])
                        .unwrap()
                        .as_text()
                        .unwrap()
                        .to_string_lossy(),
                    "4"
                );
                assert_eq!(
                    session.host().revision().get(),
                    revision + 2,
                    "one Automation transaction for the rectangle"
                );
            }
            for row in [1, 2] {
                for col in [4, 5] {
                    let at = calc_value::CellKey::new(session.host().sheets()[0].id(), row, col)
                        .unwrap();
                    if invalid {
                        assert!(
                            matches!(session.host().model().cell(at), Some(calc_host::CellInput::Number(n)) if n.get() == 31.0)
                        );
                    } else {
                        assert!(session.host().model().cell(at).is_none());
                    }
                }
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_multicell_auto_calculation_finishes_before_nested_change() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        enter(&mut session, 1, 7, "=SUM(D1:E2)");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Automatic));
        drain(&mut session);
        let project = mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim app As Excel.Application\n Set app = Application\n If Target.Column = 1 Then\n app.Range(\"D1:E2\").Value2 = Target.Value2\n Else\n History = CStr(Target.Count) & \":\" & CStr(app.Range(\"G1\").Value2)\n End If\nEnd Sub\n",
            OWNER_STATE,
            10000,
            backend,
        );
        enter(&mut session, 1, 1, "9");
        drain(&mut session);
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{:?}",
            session.workbook_event_failures()
        );
        assert_eq!(
            project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "4:36"
        );
        assert_eq!(value(&session, 1, 7).as_number(), Some(36.0));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_range_calculate_keeps_its_scope_and_manual_requirement() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        enter(&mut session, 1, 1, "1");
        enter(&mut session, 1, 2, "=BProbe(A1)");
        enter(&mut session, 1, 3, "=Counter(B1)");
        let api = format!(
            "{OWNER_STATE}Public BCalls As Long\nPublic Function BCount() As Long\n BCount = BCalls\nEnd Function\nPublic Function BProbe(ByVal x As Double) As Double\n BCalls = BCalls + 1\n BProbe = x + 1\nEnd Function\nPublic Function Counter(ByVal x As Double) As Double\n Entries = Entries + 1\n Counter = x + 1\nEnd Function\n"
        );
        let project = mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim app As Excel.Application\n Set app = Application\n app.Range(\"B1\").Calculate\n History = CStr(app.Range(\"B1\").Value2) & \":\" & CStr(Target.Value2)\nEnd Sub\n",
            &api,
            10000,
            backend,
        );
        session.set_workbook_events_enabled(false);
        full(&mut session);
        session.set_workbook_events_enabled(true);
        enter(&mut session, 1, 1, "9");
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{:?}",
            session.workbook_event_failures()
        );
        assert_eq!(
            project
                .functions()
                .call("EntryCount", &[])
                .unwrap()
                .as_number(),
            Some(1.0)
        );
        assert_eq!(
            project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "10:9"
        );
        let sheet = session.host().sheets()[0].id();
        let cells = [2, 3].map(|col| calc_value::CellKey::new(sheet, 1, col).unwrap());
        let page = calc_host::project(
            session.host(),
            &calc_host::ProjectionRequest {
                cells: cells.to_vec(),
            },
        )
        .unwrap();
        assert!(
            matches!(page.cells()[0].value(), calc_host::ProjectedValue::Calculated { value, .. } if value.as_number() == Some(10.0))
        );
        let calc_host::ProjectedValue::Stale(stale) = page.cells()[1].value() else {
            panic!("out-of-range C1 must remain stale");
        };
        assert_eq!(stale.last_value().and_then(|v| v.as_number()), Some(3.0));
        assert_eq!(
            project.functions().call("BCount", &[]).unwrap().as_number(),
            Some(2.0)
        );
        session.set_workbook_events_enabled(false);
        assert!(session.dispatch(SkinIntent::Recalculate).accepted());
        drain(&mut session);
        assert_eq!(
            project.functions().call("BCount", &[]).unwrap().as_number(),
            Some(3.0)
        );
        assert_eq!(
            project
                .functions()
                .call("EntryCount", &[])
                .unwrap()
                .as_number(),
            Some(2.0)
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_event_switch_persists_after_failure_and_suppresses_nested_changes() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        let project = mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim app As Excel.Application\n Set app = Application\n Entries = Entries + 1\n app.EnableEvents = False\n app.Range(\"D1\").Value2 = Target.Value2\n Err.Raise 5\nEnd Sub\n",
            OWNER_STATE,
            10000,
            backend,
        );
        enter(&mut session, 1, 1, "9");
        assert!(!session.workbook_events_enabled());
        assert_eq!(session.workbook_event_failures().failures.len(), 1);
        assert_eq!(
            project
                .functions()
                .call("EntryCount", &[])
                .unwrap()
                .as_number(),
            Some(1.0)
        );
        enter(&mut session, 1, 1, "10");
        assert_eq!(
            project
                .functions()
                .call("EntryCount", &[])
                .unwrap()
                .as_number(),
            Some(1.0)
        );
        assert_eq!(session.workbook_event_failures().failures.len(), 1);
        let at = calc_value::CellKey::new(session.host().sheets()[0].id(), 1, 4).unwrap();
        assert!(
            matches!(session.host().model().cell(at), Some(calc_host::CellInput::Number(n)) if n.get() == 9.0)
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_full_and_rebuild_invoke_the_existing_evaluator_without_resetting_vba() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        enter(&mut session, 1, 2, "=Counter()");
        let api = format!(
            "{OWNER_STATE}Public Function Counter() As Long\n Entries = Entries + 1\n Counter = Entries\nEnd Function\n"
        );
        let project = mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim app As Excel.Application\n Set app = Application\n app.CalculateFull\n History = History & \"full:\" & CStr(app.Range(\"B1\").Value2) & \";\"\n app.CalculateFullRebuild\n History = History & \"rebuild:\" & CStr(app.Range(\"B1\").Value2) & \";\"\nEnd Sub\n",
            &api,
            10000,
            backend,
        );
        enter(&mut session, 1, 1, "9");
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{:?}",
            session.workbook_event_failures()
        );
        assert_eq!(value(&session, 1, 2).as_number(), Some(2.0));
        assert_eq!(
            project
                .functions()
                .call("EntryCount", &[])
                .unwrap()
                .as_number(),
            Some(2.0)
        );
        assert_eq!(
            project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "full:1;rebuild:2;"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_calculate_write_completes_nested_calculate_and_change_before_return() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        enter(&mut session, 1, 1, "1");
        enter(&mut session, 1, 2, "=A1+1");
        session.set_workbook_events_enabled(false);
        let project = mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim app As Excel.Application\n Set app = Application\n History = History & \"change:\" & CStr(Target.Value2) & \";\"\nEnd Sub\nPrivate Sub Worksheet_Calculate()\n Dim app As Excel.Application\n Set app = Application\n If Entries = 0 Then\n Entries = 1\n History = History & \"outer>\"\n app.Range(\"A1\").Value2 = app.Range(\"A1\").Value2 + 10\n History = History & \"returned:\" & CStr(app.Range(\"B1\").Value2) & \";\"\n Else\n History = History & \"inner;\"\n End If\nEnd Sub\n",
            OWNER_STATE,
            10000,
            backend,
        );
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Automatic));
        drain(&mut session);
        session.set_workbook_events_enabled(true);
        enter(&mut session, 1, 1, "10");
        drain(&mut session);
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{:?}",
            session.workbook_event_failures()
        );
        assert_eq!(value(&session, 1, 2).as_number(), Some(21.0));
        assert_eq!(
            project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "change:10;outer>inner;change:20;returned:21;"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_recursive_target_writes_share_a_budget_and_release_the_next_event() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        let project = mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n If Target.Column = 1 Then\n Entries = Entries + 1\n Target.Value2 = Target.Value2 + 1\n Else\n History = History & \"clean;\"\n End If\nEnd Sub\n",
            OWNER_STATE,
            40,
            backend,
        );
        enter(&mut session, 1, 1, "1");
        let count = project
            .functions()
            .call("EntryCount", &[])
            .unwrap()
            .as_number()
            .unwrap();
        assert!(
            count > 1.0 && count < 40.0,
            "one shared 40-block budget: {count}"
        );
        let failures = session.take_workbook_event_failures();
        assert!(!failures.failures.is_empty());
        assert_eq!(failures.omitted, 0);
        enter(&mut session, 1, 2, "2");
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{:?}",
            session.workbook_event_failures()
        );
        assert_eq!(
            project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "clean;"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_nested_change_finishes_before_the_outer_write_returns() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        let source = "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim app As Excel.Application\n Set app = Application\n History = History & CStr(Target.Column) & \">\"\n If Target.Column = 1 Then\n app.Range(\"D1\").Value2 = Target.Value2 + 10\n History = History & \"returned:\" & CStr(app.Range(\"D1\").Value2) & \":\" & CStr(Target.Value2) & \";\"\n End If\n History = History & \"<\" & CStr(Target.Column) & \";\"\nEnd Sub\n";
        let project = mutable_owner(&mut session, source, OWNER_STATE, 10000, backend);
        enter(&mut session, 1, 1, "7");
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{:?}",
            session.workbook_event_failures()
        );
        assert_eq!(
            project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "1>4><4;returned:17:7;<1;"
        );
        let sheet = session.host().sheets()[0].id();
        let at = calc_value::CellKey::new(sheet, 1, 4).unwrap();
        assert!(
            matches!(session.host().model().cell(at), Some(calc_host::CellInput::Number(n)) if n.get() == 17.0)
        );
        assert_eq!(
            session.intent_log().last().unwrap().ingress,
            calc_host::CommandIngress::UserInterface
        );
        assert!(
            session
                .intent_log()
                .iter()
                .any(|record| record.ingress == calc_host::CommandIngress::Automation)
        );
        let records = session.intent_log().to_vec();
        let parent = records.last().unwrap().seq;
        assert_eq!(
            records.iter().filter(|r| r.parent == Some(parent)).count(),
            1
        );
        let mut replayed = opened(&calc_host::blank_package(), "owner.xlsx");
        let replay_project = mutable_owner(&mut replayed, source, OWNER_STATE, 10000, backend);
        let outcome = calc_host::replay(&records, &mut replayed);
        assert_eq!(outcome.dispatched, records.len() - 1);
        assert!(outcome.mismatches.is_empty());
        assert!(replayed.workbook_event_failures().failures.is_empty());
        assert_eq!(
            replay_project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "1>4><4;returned:17:7;<1;"
        );
        let mut missing_owner = opened(&calc_host::blank_package(), "owner.xlsx");
        let missing = calc_host::replay(&records, &mut missing_owner);
        assert_eq!(
            missing.mismatches,
            vec![parent],
            "missing child commands must be detected"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn mutable_owner_calculate_reenters_the_same_project_and_preserves_the_outer_target() {
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        enter(&mut session, 1, 1, "1");
        enter(&mut session, 1, 2, "=Twice(A1)");
        let project = mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim app As Excel.Application\n Set app = Application\n History = History & \"change>\"\n app.Calculate\n History = History & \"returned:\" & CStr(app.Range(\"B1\").Value2) & \":\" & CStr(Target.Value2) & \";\"\nEnd Sub\nPrivate Sub Worksheet_Calculate()\n Dim app As Excel.Application\n Set app = Application\n History = History & \"calculate;\"\nEnd Sub\n",
            OWNER_STATE,
            10000,
            backend,
        );
        session.set_calculation_budget(calc_host::WorkBudget::evaluations(1));
        enter(&mut session, 1, 1, "9");
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{:?}",
            session.workbook_event_failures()
        );
        assert!(!session.has_pending_calculation());
        assert_eq!(value(&session, 1, 2).as_number(), Some(18.0));
        assert_eq!(
            project
                .functions()
                .call("Trace", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "change>calculate;returned:18:9;"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn document_event_calculates_paramarray_functions_on_its_owning_session() {
    let examples = include_str!("../../calc-xll-reference/fixture/vba/DnaExamples.bas");
    let start = examples.find("Public Function DnaArgs(").unwrap();
    let end = examples.find("Public Function DnaCounted(").unwrap();
    let functions = &examples[start..end];
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "owner.xlsx");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        enter(&mut session, 1, 2, "=DnaArgs(1,,\"x\")");
        mutable_owner(
            &mut session,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Application.Calculate\nEnd Sub\n",
            functions,
            10000,
            backend,
        );
        enter(&mut session, 1, 1, "9");
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{backend:?}: {:?}",
            session.workbook_event_failures()
        );
        assert!(!session.has_pending_calculation());
        // Recorded in excel-reopen-dnaexamples-xlsm.json.
        assert_eq!(
            value(&session, 1, 2).as_text().unwrap().to_string_lossy(),
            "3:Double:missing:String",
            "{backend:?}"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn original_recalculation_probe_project_is_admitted_without_source_removal() {
    let bytes = include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P04/plain-settles-full-01.xlsm"
    );
    for backend in [None, Some(calc_vba::Backend::Interpreter)] {
        let mut session = opened(bytes, "probe.xlsm");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        let original = session
            .host()
            .loaded()
            .vba_project(&Default::default())
            .unwrap()
            .unwrap();
        assert_eq!(
            calc_vba::read_office_project(&original.bytes)
                .unwrap()
                .modules()
                .iter()
                .find(|m| m.name == "ProbeEvents")
                .unwrap()
                .kind,
            calc_vba::OfficeModuleKind::Class
        );
        let project = calc_host_vba::prepare_file_project_with_events(
            &grant(),
            session.host(),
            "probe.xlsm",
            1_000_000,
            backend,
            false,
        )
        .unwrap()
        .unwrap();
        session.set_workbook_events_enabled(false);
        project.install(&mut session);
        enter(&mut session, 8, 4, "=SizeUdf()");
        enter(&mut session, 9, 4, "=OracleProbe.SizeUdf()");
        enter(&mut session, 10, 4, "=IFERROR(PlainProbe(52,3),999)");
        full(&mut session);
        // The retained workbook's unarmed SizeUdf body assigns v = 2.
        assert!(
            calc_vba::read_office_project(&original.bytes)
                .unwrap()
                .modules()
                .iter()
                .find(|m| m.name == "OracleProbe")
                .unwrap()
                .source
                .contains("v = 2")
        );
        assert_eq!(value(&session, 8, 4).as_number(), Some(2.));
        assert_eq!(value(&session, 9, 4).as_number(), Some(2.));
        assert_eq!(value(&session, 10, 4).as_number(), Some(3.));
        let mut saved = Vec::new();
        session.save(&Default::default(), &mut saved).unwrap();
        let reopened = opened(&saved, "probe.xlsm");
        let retained = reopened
            .host()
            .loaded()
            .vba_project(&Default::default())
            .unwrap()
            .unwrap();
        assert_eq!(
            original, retained,
            "admission and calls must preserve all original source"
        );
        assert!(session.take_workbook_event_failures().failures.is_empty());
        session.set_workbook_events_enabled(true);
        full(&mut session);
        let failures = session.take_workbook_event_failures();
        assert!(!failures.failures.is_empty());
        assert!(
            failures
                .failures
                .iter()
                .all(|f| f.reason.contains("dc-snz.15"))
        );
        assert!(
            calc_host_vba::prepare_file_project(
                &grant(),
                session.host(),
                "probe.xlsm",
                1_000_000,
                backend
            )
            .is_err()
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn file_classes_share_one_session_and_declared_unsupported_members_stay_refused() {
    let original = include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P04/plain-settles-full-01.xlsm"
    );
    let expressions = [
        "Application.ExecuteExcel4Macro(\"1+1\")",
        "ThisWorkbook.Date1904",
        "UnsupportedSave()",
    ];
    let mut module = "Private instance As ProbeEvents\nPublic Function NextCount() As Long\n If instance Is Nothing Then Set instance = New ProbeEvents\n NextCount = instance.NextCount()\nEnd Function\n".to_owned();
    for (index, expression) in expressions.iter().enumerate() {
        module.push_str(&format!("Public Function Unsupported{index}() As Variant\n On Error Resume Next\n Unsupported{index} = {expression}\n Unsupported{index} = 999\nEnd Function\n"));
    }
    module.push_str("Public Function UnsupportedSave() As Long\n On Error Resume Next\n ThisWorkbook.Save\n UnsupportedSave = 999\nEnd Function\n");
    // Descriptor-backed workbook members routed to WorkbookAutomation (dc-piw0.5).
    module.push_str(
        "Public Function BookName() As Variant\n BookName = ThisWorkbook.Name\nEnd Function\n\
         Public Function ActiveBookName() As Variant\n ActiveBookName = ActiveWorkbook.Name\nEnd Function\n\
         Public Function AppVersion() As Variant\n AppVersion = Application.Version\nEnd Function\n\
         Public Function SheetRangeAddr() As Variant\n SheetRangeAddr = Sheet1.Range(\"A1\").Address\nEnd Function\n",
    );
    let bytes = vba_project::replace_sources(
        original,
        &[
            ("OracleProbe", &module),
            (
                "ProbeEvents",
                "Private n As Long\nPublic Function NextCount() As Long\n n = n + 1\n NextCount = n\nEnd Function\n",
            ),
            ("Sheet1", ""),
            ("ThisWorkbook", ""),
        ],
    );
    for backend in backends() {
        let mut session = opened(&bytes, "class.xlsm");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        session.set_workbook_events_enabled(false);
        calc_host_vba::activate_file_project(
            &grant(),
            &mut session,
            "class.xlsm",
            100_000,
            Some(backend),
        )
        .unwrap();
        enter(&mut session, 1, 4, "=NextCount()");
        full(&mut session);
        assert_eq!(value(&session, 1, 4).as_number(), Some(1.));
        full(&mut session);
        assert_eq!(value(&session, 1, 4).as_number(), Some(2.));
        enter(&mut session, 1, 6, "=BookName()");
        enter(&mut session, 2, 6, "=ActiveBookName()");
        enter(&mut session, 3, 6, "=AppVersion()");
        enter(&mut session, 4, 6, "=SheetRangeAddr()");
        full(&mut session);
        assert_eq!(
            value(&session, 1, 6).as_text().map(|t| t.to_string_lossy()),
            Some("class.xlsm".to_owned())
        );
        assert_eq!(
            value(&session, 2, 6).as_text().map(|t| t.to_string_lossy()),
            Some("class.xlsm".to_owned())
        );
        assert_eq!(
            value(&session, 3, 6).as_text().map(|t| t.to_string_lossy()),
            Some("16.0".to_owned())
        );
        assert_eq!(
            value(&session, 4, 6).as_text().map(|t| t.to_string_lossy()),
            Some("$A$1".to_owned())
        );
        for index in 0..expressions.len() {
            enter(
                &mut session,
                index as u32 + 2,
                4,
                &format!("=IFERROR(Unsupported{index}(),777)"),
            );
        }
        full(&mut session);
        for (index, expression) in expressions.iter().enumerate() {
            let at = calc_value::CellKey::new(session.host().sheets()[0].id(), index as u32 + 2, 4)
                .unwrap();
            let page = calc_host::project(
                session.host(),
                &calc_host::ProjectionRequest { cells: vec![at] },
            )
            .unwrap();
            assert!(
                matches!(
                    page.cells()[0].value(),
                    calc_host::ProjectedValue::NotModeled { .. }
                ),
                "{backend:?} {expression}: {:?}",
                page.cells()[0].value()
            );
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn project_class_declarations_cannot_admit_designers_or_conflicting_identities() {
    let original = include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P04/plain-settles-full-01.xlsm"
    );
    for replacement in [
        "Class=ProbeEvents\r\nClass=ProbeEvents",
        "Class=ProbeEvents\r\nBaseClass=ProbeEvents",
        "Class=MissingClass",
        "Class=OracleProbe",
        "BaseClass=ProbeEvents",
        "Document=ProbeEvents/&H00000000",
        "Class=Sheet1\r\nClass=ProbeEvents",
    ] {
        let bytes =
            vba_project::replace_project_declaration(original, "Class=ProbeEvents", replacement);
        let session = opened(&bytes, "invalid.xlsm");
        let error = calc_host_vba::prepare_file_project_with_events(
            &grant(),
            session.host(),
            "invalid.xlsm",
            100_000,
            None,
            false,
        )
        .err()
        .expect("invalid module identity must refuse before compilation");
        assert!(
            error.to_string().contains("dc-snz.15"),
            "{replacement}: {error}"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dynamic_native_sinks_cannot_hide_an_unsupported_subscription_or_retry_a_prefix() {
    let original = include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P04/plain-settles-full-01.xlsm"
    );
    let bytes = vba_project::replace_sources(
        original,
        &[
            (
                "OracleProbe",
                "Private entries As Long\nPublic Function BindSink() As Long\n On Error Resume Next\n entries = entries + 1\n Dim sink As ProbeEvents\n Set sink = New ProbeEvents\n Set sink.XL = Application\n BindSink = 999\nEnd Function\nPublic Function EntryCount() As Long\n EntryCount = entries\nEnd Function\n",
            ),
            (
                "ProbeEvents",
                "Public WithEvents XL As Application\nPrivate Sub XL_AfterCalculate()\nEnd Sub\n",
            ),
            ("Sheet1", ""),
            ("ThisWorkbook", ""),
        ],
    );
    for backend in [None, Some(calc_vba::Backend::Interpreter)] {
        let mut session = opened(&bytes, "sink.xlsm");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        session.set_workbook_events_enabled(false);
        calc_host_vba::activate_file_project(&grant(), &mut session, "sink.xlsm", 100_000, backend)
            .unwrap();
        enter(&mut session, 1, 4, "=IFERROR(BindSink(),777)");
        full(&mut session);
        let at = calc_value::CellKey::new(session.host().sheets()[0].id(), 1, 4).unwrap();
        let page = calc_host::project(
            session.host(),
            &calc_host::ProjectionRequest { cells: vec![at] },
        )
        .unwrap();
        assert!(
            matches!(
                page.cells()[0].value(),
                calc_host::ProjectedValue::NotModeled { .. }
            ),
            "{:?}",
            page.cells()[0].value()
        );
        enter(&mut session, 2, 4, "=EntryCount()");
        let at = calc_value::CellKey::new(session.host().sheets()[0].id(), 2, 4).unwrap();
        session.dispatch(SkinIntent::CalculateRange {
            areas: vec![calc_value::GridArea::new(at, at).unwrap()],
        });
        drain(&mut session);
        assert_eq!(value(&session, 2, 4).as_number(), Some(1.));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let session = opened(&bytes, "sink.xlsm");
        let error = calc_host_vba::prepare_file_project_with_events(
            &grant(),
            session.host(),
            "sink.xlsm",
            100_000,
            Some(calc_vba::Backend::Jit),
            false,
        )
        .err()
        .expect("explicit JIT refuses before entering this project");
        assert!(
            error.to_string().contains("dynamic native WithEvents"),
            "{error}"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn events_off_admission_checks_each_source_identity_including_class_and_name_swaps() {
    let original = include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P04/plain-settles-full-01.xlsm"
    );
    let renamed_class = vba_project::replace_sources(
        original,
        &[
            (
                "OracleProbe",
                "Public Function Probe() As Long\n Dim instance As Renamed\n Set instance = New Renamed\n Probe = instance.Value\nEnd Function\n",
            ),
            (
                "ProbeEvents",
                "Attribute VB_Name = \"Renamed\"\nPublic Function Value() As Long\n Value = 42\nEnd Function\n",
            ),
            ("Sheet1", ""),
            ("ThisWorkbook", ""),
        ],
    );
    let swapped = vba_project::replace_sources(
        LOCAL,
        &[
            (
                "Module1",
                "Attribute VB_Name = \"PrivateModule\"\nPublic Function A() As Long\n A = 1\nEnd Function\n",
            ),
            (
                "PrivateModule",
                "Attribute VB_Name = \"Module1\"\nPublic Function B() As Long\n B = 2\nEnd Function\n",
            ),
            ("Sheet1", ""),
            ("ThisWorkbook", ""),
        ],
    );
    for bytes in [renamed_class, swapped] {
        let session = opened(&bytes, "identity.xlsm");
        let error = calc_host_vba::prepare_file_project_with_events(
            &grant(),
            session.host(),
            "identity.xlsm",
            100_000,
            None,
            false,
        )
        .err()
        .expect("source identities must match their individual directory modules");
        // CalcVBA's reader refuses a module whose VB_Name differs from its directory entry
        // before the compiler's own identity check sees it.
        let error = error.to_string();
        assert!(
            error.contains("directory identity")
                || error.contains("VB_Name differs from directory"),
            "{error}"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn original_probe_families_compile_as_complete_events_off_projects() {
    let sources: &[(&str, &[u8])] = &[
        (
            "P01",
            include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P01/cold-app-01.xlsm"
            ),
        ),
        (
            "P02",
            include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P02/auto-app-01.xlsm"
            ),
        ),
        (
            "P03",
            include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P03/double-direct-01.xlsm"
            ),
        ),
        (
            "P05",
            include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P05/child-scope-existing-child-growth-01.xlsm"
            ),
        ),
        (
            "P06",
            include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P06/disabled-open-auto~manual-true-save-disabled-01-tested.xlsm-01.xlsm"
            ),
        ),
        (
            "P08",
            include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P08/observer-cold-lower-chain-01.xlsm"
            ),
        ),
        (
            "P07",
            include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P07/P07-ui-owned.xlsm"
            ),
        ),
        (
            "P09",
            include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P09/cse-01.xlsm"),
        ),
        (
            "P10",
            include_bytes!(
                "../../calc-engine/tests/fixtures/excel-calc-probes/P10/close-owner-01.xlsm"
            ),
        ),
    ];
    let mut failures = Vec::new();
    for (name, source) in sources {
        let session = opened(source, "probe.xlsm");
        if let Err(error) = calc_host_vba::prepare_file_project_with_events(
            &grant(),
            session.host(),
            "probe.xlsm",
            100_000,
            Some(calc_vba::Backend::Interpreter),
            false,
        ) {
            failures.push(format!("{name}: {error}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn original_p01_p10_projects_keep_all_source_after_admission_and_save() {
    for source in [
        include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P01/cold-app-01.xlsm")
            .as_slice(),
        include_bytes!(
            "../../calc-engine/tests/fixtures/excel-calc-probes/P10/close-owner-01.xlsm"
        )
        .as_slice(),
    ] {
        let mut session = opened(source, "probe.xlsm");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        session.set_workbook_events_enabled(false);
        let original = session
            .host()
            .loaded()
            .vba_project(&Default::default())
            .unwrap()
            .unwrap();
        let owner = calc_host_vba::prepare_file_project_with_events(
            &grant(),
            session.host(),
            "probe.xlsm",
            100_000,
            Some(calc_vba::Backend::Interpreter),
            false,
        )
        .expect("temporary-array bounds and Input$ admit the complete original project")
        .unwrap();
        owner.install(&mut session);
        let mut saved = Vec::new();
        session.save(&Default::default(), &mut saved).unwrap();
        let reopened = opened(&saved, "probe.xlsm");
        assert_eq!(
            original,
            reopened
                .host()
                .loaded()
                .vba_project(&Default::default())
                .unwrap()
                .unwrap()
        );
    }
}

/// RunCase assigned observedSheet in the Excel capture. Fresh file admission
/// must not restore that object from saved formula caches.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn original_p01_fresh_observer_keeps_its_uninitialized_object_error() {
    use calc_value::{CalcValue, WorksheetErrorCode};
    let bytes =
        include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P01/cold-app-01.xlsm");
    let mut session = opened(bytes, "probe.xlsm");
    session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
    session.set_workbook_events_enabled(false);
    let owner = calc_host_vba::prepare_file_project_with_events(
        &grant(),
        session.host(),
        "probe.xlsm",
        100_000,
        Some(calc_vba::Backend::Interpreter),
        false,
    )
    .unwrap()
    .unwrap();
    let caller = calc_value::CellKey::new(session.host().sheets()[0].id(), 1, 3).unwrap();
    let error = owner
        .functions()
        .call_with_context("PeekB1", &[], &mut CallerOnlyContext(caller))
        .expect_err("the fresh project's observedSheet is Nothing");
    assert!(error.contains("VBA error 91:"), "{error}");
    owner.install(&mut session);
    enter(&mut session, 2, 11, "=IFERROR(PeekB1(),777)");
    full(&mut session);
    assert_eq!(
        value(&session, 1, 3),
        CalcValue::error(WorksheetErrorCode::Value)
    );
    assert_eq!(value(&session, 2, 11), CalcValue::number(777.));
    assert!(session.take_workbook_event_failures().failures.is_empty());
}

/// The saved value reflects RunCase's initialized logger state; a fresh project
/// cannot restore those runtime globals. Exercise the uninitialized original source.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn original_p02_fresh_logger_raises_error_nine_and_keeps_its_source() {
    use calc_value::{CalcValue, WorksheetErrorCode};
    let bytes =
        include_bytes!("../../calc-engine/tests/fixtures/excel-calc-probes/P02/auto-app-01.xlsm");
    for backend in [None, Some(calc_vba::Backend::Interpreter)] {
        let mut session = opened(bytes, "probe.xlsm");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        session.set_workbook_events_enabled(false);
        let original = session
            .host()
            .loaded()
            .vba_project(&Default::default())
            .unwrap()
            .unwrap();
        let owner = calc_host_vba::prepare_file_project_with_events(
            &grant(),
            session.host(),
            "probe.xlsm",
            100_000,
            backend,
            false,
        )
        .unwrap()
        .unwrap();
        let functions = owner.functions();
        let caller = calc_value::CellKey::new(session.host().sheets()[0].id(), 1, 11).unwrap();
        let error = functions
            .call_with_context(
                "PlainProbe",
                &[CalcValue::number(34.), CalcValue::number(1.)],
                &mut CallerOnlyContext(caller),
            )
            .expect_err("the original unallocated logger must fail after its caller-address read");
        assert!(error.contains("VBA error 9:"), "{backend:?}: {error}");
        owner.install(&mut session);
        enter(&mut session, 1, 11, "=PlainProbe(34,1)");
        enter(&mut session, 2, 11, "=IFERROR(PlainProbe(34,1),777)");
        full(&mut session);
        assert_eq!(
            value(&session, 1, 11),
            CalcValue::error(WorksheetErrorCode::Value)
        );
        assert_eq!(value(&session, 2, 11), CalcValue::number(777.));
        assert!(session.take_workbook_event_failures().failures.is_empty());
        let mut saved = Vec::new();
        session.save(&Default::default(), &mut saved).unwrap();
        let reopened = opened(&saved, "probe.xlsm");
        assert_eq!(
            original,
            reopened
                .host()
                .loaded()
                .vba_project(&Default::default())
                .unwrap()
                .unwrap()
        );
    }
}

/// The saved P08 readings follow RunCase's assignment of observedSheet. Opening
/// the original project starts a new VBA session with that object set to Nothing.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn original_p08_fresh_observers_raise_error_ninety_one_without_invented_setup() {
    use calc_value::{CalcValue, WorksheetErrorCode};
    let bytes = include_bytes!(
        "../../calc-engine/tests/fixtures/excel-calc-probes/P08/observer-cold-lower-chain-01.xlsm"
    );
    for backend in [None, Some(calc_vba::Backend::Interpreter)] {
        let mut session = opened(bytes, "probe.xlsm");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        session.set_workbook_events_enabled(false);
        let original = session
            .host()
            .loaded()
            .vba_project(&Default::default())
            .unwrap()
            .unwrap();
        let read = calc_vba::read_office_project(&original.bytes).unwrap();
        let source = &read
            .modules()
            .iter()
            .find(|m| m.name == "OracleProbe")
            .unwrap()
            .source;
        assert!(source.contains("Private observedSheet As Worksheet"));
        assert!(source.contains("Set observedSheet = ThisWorkbook.Worksheets(1)"));
        let owner = calc_host_vba::prepare_file_project_with_events(
            &grant(),
            session.host(),
            "probe.xlsm",
            1_000_000,
            backend,
            false,
        )
        .unwrap()
        .unwrap();
        let functions = owner.functions();
        let caller = calc_value::CellKey::new(session.host().sheets()[0].id(), 1, 3).unwrap();
        for name in ["PeekB1", "PeekSeveral"] {
            let error = functions
                .call_with_context(name, &[], &mut CallerOnlyContext(caller))
                .unwrap_err();
            assert!(
                error.contains("VBA error 91:"),
                "{backend:?}: {name}: {error}"
            );
        }
        owner.install(&mut session);
        enter(&mut session, 1, 3, "=PeekB1()");
        enter(&mut session, 1, 6, "=PeekSeveral()");
        enter(&mut session, 2, 6, "=IFERROR(PeekSeveral(),777)");
        full(&mut session);
        assert_eq!(
            value(&session, 1, 3),
            CalcValue::error(WorksheetErrorCode::Value)
        );
        assert_eq!(
            value(&session, 1, 6),
            CalcValue::error(WorksheetErrorCode::Value)
        );
        assert_eq!(value(&session, 2, 6), CalcValue::number(777.));
        assert!(session.workbook_event_failures().failures.is_empty());
        let mut saved = Vec::new();
        session.save(&Default::default(), &mut saved).unwrap();
        let reopened = opened(&saved, "probe.xlsm");
        assert_eq!(
            original,
            reopened
                .host()
                .loaded()
                .vba_project(&Default::default())
                .unwrap()
                .unwrap()
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn events_off_file_range_arguments_keep_the_shared_projection_tokens() {
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            (
                "Module1",
                "Public Function InspectRange(x As Range) As String\n InspectRange = CStr(x.Count) & \":\" & CStr(x.Row) & \":\" & CStr(x.Column) & \":\" & x.Address\nEnd Function\n",
            ),
            ("PrivateModule", ""),
            ("Sheet1", ""),
            ("ThisWorkbook", ""),
        ],
    );
    for backend in backends() {
        let mut session = opened(&bytes, "range.xlsm");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        session.set_workbook_events_enabled(false);
        calc_host_vba::activate_file_project(
            &grant(),
            &mut session,
            "range.xlsm",
            100_000,
            Some(backend),
        )
        .unwrap();
        enter(&mut session, 1, 4, "=InspectRange(A1:B2)");
        full(&mut session);
        assert_eq!(
            value(&session, 1, 4).as_text().unwrap().to_string_lossy(),
            "4:1:1:$A$1:$B$2"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn file_project_discovers_described_procedures_in_xml_and_binary() {
    for (bytes, name, description, category) in [
        (
            include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/metadata-vba.xlsm").as_slice(),
            "metadata-vba.xlsm",
            "VBA-set description",
            "3",
        ),
        (
            include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/metadata-custom-vba.xlsb")
                .as_slice(),
            "metadata-custom-vba.xlsb",
            "Custom category description",
            "20",
        ),
    ] {
        let mut session = opened(bytes, name);
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        session.set_workbook_events_enabled(false);
        let project = calc_host_vba::open_file_project(
            &grant(),
            &mut session,
            name,
            100_000,
            Some(calc_vba::Backend::Interpreter),
        )
        .unwrap()
        .unwrap();
        let functions = project.functions();
        for spelling in ["MetaFunc", "Module1.MetaFunc"] {
            let described = functions.descriptor(None, spelling).unwrap();
            assert_eq!(described.name, spelling);
            assert_eq!(described.origin, "VBA module Module1");
            assert_eq!(described.description, description);
            assert_eq!(described.category, category);
            assert_eq!(described.signature(), format!("{spelling}(x)"));
        }
        assert!(functions.bind(None, "SetMetadataViaVba").is_none());
        enter(&mut session, 1, 4, "=Module1.MetaFunc(2)");
        full(&mut session);
        assert_eq!(value(&session, 1, 2).as_number(), Some(4002.0));
        assert_eq!(value(&session, 1, 4).as_number(), Some(4002.0));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn file_project_sources_compile_with_their_exported_attributes() {
    for (bytes, name) in [
        (
            include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsm").as_slice(),
            "local.xlsm",
        ),
        (
            include_bytes!("../../calc-doc/tests/fixtures/vba_udf_names/local.xlsb").as_slice(),
            "local.xlsb",
        ),
    ] {
        for backend in backends() {
            let mut session = opened(bytes, name);
            calc_host_vba::activate_file_project(
                &grant(),
                &mut session,
                name,
                100_000,
                Some(backend),
            )
            .unwrap();
            assert!(session.dispatch(SkinIntent::Recalculate).accepted());
            drain(&mut session);
            // Private worksheet lookup is recorded in excel-observations.json,
            // distinct from VBA-to-VBA visibility and module registration.
            for (row, expected) in [
                (1, 102.),
                (2, 102.),
                (3, 102.),
                (4, 102.),
                (22, 702.),
                (24, 802.),
                (26, 902.),
                (30, 1202.),
                (31, 1202.),
                (32, 1302.),
            ] {
                assert_eq!(
                    value(&session, row, 2).as_number(),
                    Some(expected),
                    "{backend:?}"
                );
            }
            // The retained Excel observations give #REF! for bare grid-cell
            // callees; the same VBA procedures remain callable by module name.
            for row in [21, 23, 25] {
                assert_eq!(
                    value(&session, row, 2).as_error(),
                    Some(calc_value::WorksheetErrorCode::Ref),
                    "{name} {backend:?} B{row}"
                );
            }
            enter(&mut session, 1, 4, "=PrivateModule.HiddenFunc(3)");
            session.dispatch(SkinIntent::Recalculate);
            drain(&mut session);
            assert_eq!(value(&session, 1, 4).as_number(), Some(1303.));
            let before = session
                .host()
                .loaded()
                .vba_project(&Default::default())
                .unwrap()
                .unwrap();
            let mut saved = Vec::new();
            session.save(&Default::default(), &mut saved).unwrap();
            let mut reopened = opened(&saved, name);
            let after = reopened
                .host()
                .loaded()
                .vba_project(&Default::default())
                .unwrap()
                .unwrap();
            let (before, after) = (
                calc_vba::read_office_project(&before.bytes).unwrap(),
                calc_vba::read_office_project(&after.bytes).unwrap(),
            );
            assert_eq!(before.modules().len(), after.modules().len());
            for (before, after) in before.modules().iter().zip(after.modules()) {
                assert_eq!(
                    before.source, after.source,
                    "internal worksheet entries must not alter saved VBA"
                );
            }
            calc_host_vba::activate_file_project(
                &grant(),
                &mut reopened,
                name,
                100_000,
                Some(backend),
            )
            .unwrap();
            assert!(reopened.dispatch(SkinIntent::Recalculate).accepted());
            drain(&mut reopened);
            assert_eq!(value(&reopened, 30, 2).as_number(), Some(1202.));
            assert_eq!(value(&reopened, 31, 2).as_number(), Some(1202.));
            for row in [21, 23, 25] {
                assert_eq!(
                    value(&reopened, row, 2).as_error(),
                    Some(calc_value::WorksheetErrorCode::Ref)
                );
            }
            for (row, expected) in [(22, 702.), (24, 802.), (26, 902.)] {
                assert_eq!(value(&reopened, row, 2).as_number(), Some(expected));
            }
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn unsupported_private_entries_refuse_only_their_calls_without_running_a_prefix() {
    for signature in [
        "ParamArray x() As Variant",
        "ByRef x() As Double",
        "ByVal x As Object",
    ] {
        let source = format!(
            "Public Entries As Long\nPublic Function Good(ByVal x As Double) As Double\n Good = x + 7\nEnd Function\nPublic Function Probe() As Long\n Probe = Entries\nEnd Function\nPrivate Function Hidden({signature}) As Double\n Entries = Entries + 1\n Hidden = 23\nEnd Function\n"
        );
        let bytes = vba_project::replace_sources(
            LOCAL,
            &[
                ("Module1", source.as_str()),
                ("PrivateModule", "Option Private Module\n"),
                ("Sheet1", ""),
                ("ThisWorkbook", ""),
            ],
        );
        for backend in backends() {
            let mut session = opened(&bytes, "unsupported-private.xlsm");
            assert!(
                session
                    .dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual))
                    .accepted()
            );
            calc_host_vba::activate_file_project(
                &grant(),
                &mut session,
                "unsupported-private.xlsm",
                100_000,
                Some(backend),
            )
            .unwrap();
            enter(&mut session, 1, 4, "=Good(2)");
            enter(&mut session, 2, 4, "=IFERROR(Module1.Hidden(1,2),999)");
            enter(&mut session, 3, 4, "=Probe()");
            full(&mut session);
            assert_eq!(value(&session, 1, 4).as_number(), Some(9.));
            // Demand Probe again after the unsupported invocation has completed,
            // so a cached earlier Probe result cannot hide a procedure prefix.
            let probe =
                calc_host::value::CellKey::new(session.host().sheets()[0].id(), 3, 4).unwrap();
            assert!(
                session
                    .dispatch(SkinIntent::CalculateRange {
                        areas: vec![calc_host::value::GridArea::new(probe, probe).unwrap()]
                    })
                    .accepted()
            );
            drain(&mut session);
            assert_eq!(
                value(&session, 3, 4).as_number(),
                Some(0.),
                "an unsupported entry cannot execute its prefix"
            );
            let at = calc_host::value::CellKey::new(session.host().sheets()[0].id(), 2, 4).unwrap();
            let page = calc_host::project(
                session.host(),
                &calc_host::ProjectionRequest { cells: vec![at] },
            )
            .unwrap();
            let calc_host::ProjectedValue::NotModeled {
                refusal: calc_host::NotModeled::Evaluation(fault),
                ..
            } = page.cells()[0].value()
            else {
                panic!(
                    "a known unsupported signature cannot become a calculated #VALUE! or an IFERROR fallback: {:?}",
                    page.cells()[0].value()
                );
            };
            assert!(format!("{fault:?}").contains("dc-snz.15"), "{fault:?}");
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
        let mut session = opened(&bytes, "file.xlsm");
        calc_host_vba::activate_file_project(
            &grant(),
            &mut session,
            "file.xlsm",
            100_000,
            Some(backend),
        )
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
        let mut reopened = opened(&bytes, "file.xlsm");
        calc_host_vba::activate_file_project(
            &grant(),
            &mut reopened,
            "file.xlsm",
            100_000,
            Some(backend),
        )
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

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_bctq_save_as_keeps_project_state_and_refreshes_document_identity() {
    const MODULE: &str = "Public Seen As String\nPublic Saves As Long\nPublic Opens As Long\nPublic WasUI As Boolean\nPublic Function Probe() As String\n Probe = Seen & \":\" & CStr(Saves) & \":\" & CStr(WasUI) & \":\" & CStr(Opens)\nEnd Function\n";
    const EVENTS: &str = "Private Sub Workbook_Open()\n Opens = Opens + 1\nEnd Sub\nPrivate Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)\n Seen = Me.Name\n Saves = Saves + 1\n WasUI = SaveAsUI\nEnd Sub\n";
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            ("Module1", MODULE),
            ("PrivateModule", "Option Private Module\n"),
            ("Sheet1", ""),
            ("ThisWorkbook", EVENTS),
        ],
    );
    for backend in backends() {
        let mut session = opened(&bytes, "Old.xlsm");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        calc_host_vba::activate_file_project(
            &grant(),
            &mut session,
            "Old.xlsm",
            100_000,
            Some(backend),
        )
        .unwrap();
        enter(&mut session, 1, 4, "=Probe()");
        let save = |session: &mut HostSession, operation| {
            let active_sheet = session.host().sheets()[0].id();
            let prepared = session
                .prepare_save_with_context(
                    &Default::default(),
                    calc_host::ExcelSaveContext {
                        operation,
                        calculate_before_save: false,
                        active_sheet,
                        disabled_sheets: &[],
                        before_save: None,
                    },
                )
                .unwrap();
            let mut saved = Vec::new();
            prepared.write(&mut saved).unwrap();
            assert!(!saved.is_empty());
            full(session);
            value(session, 1, 4).as_text().unwrap().to_string_lossy()
        };
        assert_eq!(
            save(&mut session, calc_host::SaveOperation::SaveAsDialog),
            "Old.xlsm:1:True:1"
        );
        // Event delivery may be disabled during an identity update; the object still changes.
        session.set_workbook_events_enabled(false);
        session.set_file_identity("Budget.xltm", true);
        session.set_workbook_events_enabled(true);
        assert_eq!(
            save(&mut session, calc_host::SaveOperation::Save),
            "Budget.xltm:2:False:1"
        );
        assert_eq!(
            save(&mut session, calc_host::SaveOperation::SaveAs),
            "Budget.xltm:3:False:1"
        );
        assert!(session.take_workbook_event_failures().failures.is_empty());
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn file_event_state_refuses_stale_target_values_even_when_vba_catches_the_error() {
    const MODULE: &str = "Public Held As Double\nPublic Function MyFunc(ByVal x As Double) As Double\n MyFunc = x + 100\nEnd Function\nPublic Function Probe() As Double\n Probe = Held\nEnd Function\n";
    const CHANGE: &str = "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n On Error Resume Next\n Held = Target.Value2\nEnd Sub\n";
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            ("Module1", MODULE),
            ("PrivateModule", "Option Private Module\n"),
            ("Sheet1", CHANGE),
            ("ThisWorkbook", ""),
        ],
    );
    for backend in backends() {
        for file_project in [true, false] {
            let mut session = opened(&bytes, "file.xlsm");
            session.cancel_calculation();
            assert!(
                session
                    .dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual))
                    .accepted()
            );
            // Enter before installing the owner, so setup raises no Change callback.
            enter(&mut session, 1, 4, "=Probe()");
            if file_project {
                calc_host_vba::activate_file_project(
                    &grant(),
                    &mut session,
                    "file.xlsm",
                    100_000,
                    Some(backend),
                )
                .unwrap();
            } else {
                // Preserve the explicit-source API's existing observation policy.
                let sheet = session.host().sheets()[0].id();
                let modules = [
                    calc_host_vba::ModuleSource {
                        name: "Module1".into(),
                        source: MODULE.into(),
                        target: calc_host_vba::ModuleTarget::Standard,
                    },
                    calc_host_vba::ModuleSource {
                        name: "Sheet1".into(),
                        source: CHANGE.into(),
                        target: calc_host_vba::ModuleTarget::Worksheet(sheet),
                    },
                ];
                calc_host_vba::prepare(
                    session.host(),
                    "file.xlsm",
                    &modules,
                    100_000,
                    Some(backend),
                )
                .unwrap()
                .install(&mut session);
            }
            full(&mut session);
            assert_eq!(value(&session, 1, 2).as_number(), Some(102.));
            assert_eq!(value(&session, 1, 4).as_number(), Some(0.));
            enter(&mut session, 1, 2, "=MyFunc(3)");
            full(&mut session);
            assert_eq!(value(&session, 1, 2).as_number(), Some(103.));
            assert_eq!(
                value(&session, 1, 4).as_number(),
                Some(if file_project { 0. } else { 102. }),
                "{backend:?} file={file_project}"
            );
            let failures = session.take_workbook_event_failures();
            if file_project {
                assert_eq!(failures.failures.len(), 1, "{failures:?}");
                assert!(
                    failures.failures[0].reason.contains("dc-snz.15"),
                    "{failures:?}"
                );
            } else {
                assert!(failures.failures.is_empty(), "{failures:?}");
            }
            enter(&mut session, 1, 2, "7");
            full(&mut session);
            assert_eq!(value(&session, 1, 4).as_number(), Some(7.));
            assert!(session.take_workbook_event_failures().failures.is_empty());
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn caller_address_metadata_keeps_clean_f9_quiet_and_tracks_names() {
    const MODULE: &str = r#"Private entries As Long
Public Function Qualified() As String
 entries = entries + 1
 Qualified = Application.Caller.Address(External:=True) & ":" & CStr(entries)
End Function
Public Function TypedAddress() As String
 Dim r As Excel.Range
 Set r = Application.Caller
 TypedAddress = r.Address(True, True, 1, True)
End Function
Public Function ReorderedAddress() As String
 Dim r As Excel.Range
 Set r = Application.Caller
 ReorderedAddress = r.Address(External:=True, ColumnAbsolute:=True, RowAbsolute:=True)
End Function
Public Function LocalAddress() As String
 LocalAddress = Application.Caller.Address(External:=False)
End Function
Public Function RelativeRowAddress() As String
 RelativeRowAddress = Application.Caller.Address(RowAbsolute:=False)
End Function
Public Function R1C1Address() As String
 R1C1Address = Application.Caller.Address(ReferenceStyle:=-4150)
End Function
Public Function UnsupportedAddress() As String
 On Error Resume Next
 UnsupportedAddress = Application.Caller.Address(ReferenceStyle:=99)
 UnsupportedAddress = "caught"
End Function
"#;
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            ("Module1", MODULE),
            ("PrivateModule", "Option Private Module\n"),
            ("Sheet1", ""),
            ("ThisWorkbook", ""),
        ],
    );
    let control = vba_project::minimal_control(&bytes);
    assert_eq!(
        support::d1::part(&bytes, "xl/vbaProject.bin"),
        support::d1::part(&control, "xl/vbaProject.bin")
    );
    for backend in backends() {
        let mut host = calc_host::HostState::open(
            Arc::new(MemorySource::new(control.clone()).named("Old.xlsm")),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        host.set_file(Some(calc_func::WorkbookFile {
            folder: None,
            name: "Old.xlsm".into(),
        }));
        let mut session = HostSession::new(host);
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        session.set_workbook_events_enabled(false);
        calc_host_vba::activate_file_project(
            &grant(),
            &mut session,
            "Old.xlsm",
            100_000,
            Some(backend),
        )
        .unwrap();
        for (row, formula) in [
            (1, "=Qualified()"),
            (2, "=TypedAddress()"),
            (3, "=ReorderedAddress()"),
            (4, "=LocalAddress()"),
        ] {
            enter(&mut session, row, 4, formula);
        }
        full(&mut session);
        let text = |session: &HostSession, row| {
            value(session, row, 4).as_text().unwrap().to_string_lossy()
        };
        assert_eq!(text(&session, 1), "[Old.xlsm]Cases!$D$1:1", "{backend:?}");
        assert_eq!(text(&session, 2), "[Old.xlsm]Cases!$D$2");
        assert_eq!(text(&session, 3), "[Old.xlsm]Cases!$D$3");
        assert_eq!(text(&session, 4), "$D$4");
        session.dispatch(SkinIntent::Recalculate);
        drain(&mut session);
        assert_eq!(
            text(&session, 1),
            "[Old.xlsm]Cases!$D$1:1",
            "a clean F9 must not repeat the address observer"
        );
        let sheet = session.host().sheets()[0].id();
        let renamed = session.dispatch(SkinIntent::Command(calc_host::WorkbookCommand::Sheet {
            sheet: Some(sheet),
            change: calc_host::SheetChange::Rename {
                name: "O'Brien data".into(),
            },
        }));
        assert!(renamed.accepted(), "{renamed:?}");
        session.dispatch(SkinIntent::Recalculate);
        drain(&mut session);
        assert_eq!(text(&session, 1), "'[Old.xlsm]O''Brien data'!$D$1:2");
        assert!(session.dispatch(SkinIntent::Undo).accepted());
        session.dispatch(SkinIntent::Recalculate);
        drain(&mut session);
        assert_eq!(text(&session, 1), "[Old.xlsm]Cases!$D$1:3");
        // A stale file-facts snapshot must not override the model's Save As identity.
        session.set_file_identity("Budget.xltm", true);
        session.dispatch(SkinIntent::Recalculate);
        drain(&mut session);
        assert_eq!(text(&session, 1), "[Budget.xltm]Cases!$D$1:4");
        session.dispatch(SkinIntent::Recalculate);
        drain(&mut session);
        assert_eq!(text(&session, 1), "[Budget.xltm]Cases!$D$1:4");
        enter(&mut session, 11, 4, "=RelativeRowAddress()");
        enter(&mut session, 12, 4, "=R1C1Address()");
        enter(&mut session, 13, 4, "=IFERROR(UnsupportedAddress(),999)");
        let area = calc_value::GridArea::new(
            calc_value::CellKey::new(sheet, 11, 4).unwrap(),
            calc_value::CellKey::new(sheet, 13, 4).unwrap(),
        )
        .unwrap();
        session.dispatch(SkinIntent::CalculateRange { areas: vec![area] });
        drain(&mut session);
        assert_eq!(text(&session, 11), "$D11", "{backend:?}");
        assert_eq!(text(&session, 12), "R12C4", "{backend:?}");
        let at = calc_value::CellKey::new(sheet, 13, 4).unwrap();
        let page = calc_host::project(
            session.host(),
            &calc_host::ProjectionRequest { cells: vec![at] },
        )
        .unwrap();
        assert!(
            matches!(
                page.cells()[0].value(),
                calc_host::ProjectedValue::NotModeled { .. }
            ),
            "{backend:?} ReferenceStyle:=99: {:?}",
            page.cells()[0].value()
        );
    }
}

/// A Guarded grant must bind the project's host, survive policy restoration and keep its audit.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_guarded_grant_denies_native_com_under_an_ambient_full_policy() {
    use calc_vba::policy::{self, Capability, PolicyMode};
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            (
                "Module1",
                "Public Function Probe() As Long\n On Error Resume Next\n Dim o As Object\n Set o = CreateObject(\"WinHttp.WinHttpRequest.5.1\")\n Probe = Err.Number\nEnd Function\n",
            ),
            ("PrivateModule", "Option Private Module\n"),
            ("Sheet1", ""),
            ("ThisWorkbook", ""),
        ],
    );
    for backend in backends() {
        policy::set_thread_policy(Some(PolicyMode::Full));
        let session = opened(&bytes, "guarded.xlsm");
        let grant =
            calc_host_vba::ProjectGrant::trust_all("test", calc_macro_security::VbaPolicy::Guarded);
        let owner = calc_host_vba::prepare_file_project_with_events(
            &grant,
            session.host(),
            "guarded.xlsm",
            100_000,
            Some(backend),
            false,
        )
        .unwrap()
        .unwrap();
        assert_eq!(policy::thread_policy(), Some(PolicyMode::Full));
        let value = owner.functions().call("Probe", &[]).unwrap();
        assert_eq!(value.as_number(), Some(429.));
        assert!(
            policy::drain_audit()
                .denied_set()
                .contains(Capability::ComActivate)
        );
        policy::set_thread_policy(None);
    }
}

/// dc-piw0.5: VBA binds Excel and Office references from calc-object-model's descriptor
/// (real IIDs, DISPIDs, cross-library Shape -> Office.TextFrame2 -> TextRange2,
/// ListObjects, ChartObjects, Range.Formula, For Each, TypeOf Me Is Excel.Worksheet,
/// and Me.Calculate alongside Worksheet_Calculate) and routes every call to WorkbookAutomation.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_piw0_5_excel_and_office_descriptor_binding_routes_tables_charts_shapes_and_iteration() {
    let sheet_source = "\
Private Sub Worksheet_Change(ByVal Target As Excel.Range)
 If Target.Column <> 1 Or Target.Row <> 1 Then Exit Sub
 Application.EnableEvents = False
 Me.Range(\"A1\").Value2 = \"Item\"
 Me.Range(\"B1\").Value2 = \"Amount\"
 Me.Range(\"A2\").Value2 = \"Alpha\"
 Me.Range(\"B2\").Value2 = 10
 Me.Range(\"A3\").Value2 = \"Beta\"
 Me.Range(\"B3\").Value2 = 25
 Me.Range(\"C2\").Formula = \"=B2*2\"
 Dim lo As Excel.ListObject
 Set lo = Me.ListObjects.Add(xlSrcRange, Me.Range(\"A1:B3\"), , xlYes)
 lo.Name = \"SalesTbl\"
 Dim shp As Excel.Shape
 Dim tf As Office.TextFrame2
 Set shp = Me.Shapes.AddShape(msoShapeRectangle, 10, 20, 120, 40)
 Set tf = shp.TextFrame2
 tf.TextRange.Text = \"Report Title\"
 Dim co As Excel.ChartObject
 Set co = Me.ChartObjects.Add(10, 80, 200, 120)
 co.Chart.HasTitle = True
 co.Chart.ChartTitle.Text = \"Sales Chart\"
 co.Chart.SetSourceData Me.Range(\"A1:B3\")
 Dim total As Double
 Dim cell As Excel.Range
 For Each cell In Me.Range(\"B2:B3\")
  total = total + CDbl(cell.Value2)
 Next cell
 Dim isSheet As Boolean
 isSheet = (TypeOf Me Is Excel.Worksheet)
 Application.EnableEvents = True
 Me.Calculate
 History = lo.Name & \":\" & CStr(lo.ListColumns.Count) & \":\" & shp.TextFrame2.TextRange.Text & \":\" & co.Chart.ChartTitle.Text & \":\" & CStr(total) & \":\" & CStr(Me.Range(\"C2\").Value2) & \":\" & CStr(isSheet) & \":\" & CStr(Entries)
End Sub
Private Sub Worksheet_Calculate()
 Entries = Entries + 1
End Sub
";
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "report.xlsm");
        session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
        let project = mutable_owner(&mut session, sheet_source, OWNER_STATE, 50_000, backend);
        enter(&mut session, 1, 1, "go");
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{backend:?}: {:?}",
            session.workbook_event_failures()
        );
        let trace = project
            .functions()
            .call("Trace", &[])
            .unwrap()
            .as_text()
            .unwrap()
            .to_string_lossy();
        assert_eq!(
            trace, "SalesTbl:2:Report Title:Sales Chart:35:20:True:1",
            "{backend:?}"
        );
        let sheet = session.host().sheets()[0].id();
        assert_eq!(
            session.host().model().definitions().tables().len(),
            1,
            "{backend:?}: table committed"
        );
        assert_eq!(
            session.host().model().definitions().shapes(sheet).len(),
            1,
            "{backend:?}: shape committed"
        );
        assert_eq!(
            session.host().model().definitions().charts(sheet).len(),
            1,
            "{backend:?}: chart committed"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_2_vba_formats_a_report_total_through_the_descriptor() {
    let sheet_source = r#"
Private Sub Worksheet_Change(ByVal Target As Excel.Range)
 If Target.Column <> 1 Then Exit Sub
 Dim totals As Excel.Range
 Set totals = Me.Range("D:D")
 totals.NumberFormat = "0.00"
 History = CStr(totals.NumberFormat) & ":" & CStr(Me.Range("D2").Text)
End Sub
"#;
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "format-report.xlsm");
        enter(&mut session, 2, 4, "=15+15");
        drain(&mut session);
        let project = mutable_owner(&mut session, sheet_source, OWNER_STATE, 10_000, backend);
        enter(&mut session, 1, 1, "go");
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{backend:?}: {:?}",
            session.workbook_event_failures()
        );
        let trace = project.functions().call("Trace", &[]).unwrap();
        assert_eq!(
            trace.as_text().unwrap().to_string_lossy(),
            "0.00:30.00",
            "{backend:?}"
        );
        assert_eq!(value(&session, 2, 4).as_number(), Some(30.0), "{backend:?}");
    }
}

/// Excel Q13: a late-bound Let through a read-only table member writes the returned
/// range's default value. An object assignment to the property itself raises 438.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn late_bound_table_range_let_writes_values_and_set_raises_438() {
    let sheet_source = r#"
Private Sub Worksheet_Change(ByVal Target As Excel.Range)
 If Target.Column <> 1 Or Target.Row <> 1 Then Exit Sub
 Application.EnableEvents = False
 Me.Range("A1").Value2 = "H1"
 Me.Range("B1").Value2 = "H2"
 Me.Range("A2").Value2 = 1
 Me.Range("B2").Value2 = 2
 Me.Range("A3").Value2 = 3
 Me.Range("B3").Value2 = 4
 Dim lo As Object
 Set lo = Me.ListObjects.Add(xlSrcRange, Me.Range("A1:B3"), , xlYes)
 On Error Resume Next
 Set lo.Range = Me.Range("D1")
 History = CStr(Err.Number)
 Err.Clear
 On Error GoTo 0
 lo.DataBodyRange = 5
 lo.HeaderRowRange = "z"
 History = History & ":" & CStr(Me.Range("A2").Value2) & ":" & CStr(Me.Range("B3").Value2)
 History = History & ":" & CStr(Me.Range("A1").Value2) & ":" & CStr(Me.Range("B1").Value2)
 History = History & ":" & CStr(Me.ListObjects.Count)
 lo.Range = 9
 History = History & ":" & CStr(Me.Range("A1").Value2) & ":" & CStr(Me.Range("B3").Value2)
 History = History & ":" & CStr(Me.ListObjects.Count)
 Application.EnableEvents = True
End Sub
"#;
    for backend in backends() {
        let mut session = opened(&calc_host::blank_package(), "table-let.xlsm");
        let project = mutable_owner(&mut session, sheet_source, OWNER_STATE, 50_000, backend);
        enter(&mut session, 1, 1, "go");
        let trace = project.functions().call("Trace", &[]).unwrap();
        let trace = trace.as_text().unwrap().to_string_lossy();
        assert!(
            session.workbook_event_failures().failures.is_empty(),
            "{backend:?} at {trace}: {:?}",
            session.workbook_event_failures()
        );
        assert_eq!(trace, "438:5:5:z:z2:1:9:9:0", "{backend:?}");
    }
}

/// dc-1nbp.7: Worksheet and Workbook objects (`Me.Range`, `Worksheets("S").Range`,
/// `ThisWorkbook.Worksheets("S").Range`, `Range.Cells`, `Range.Value2` reads/writes,
/// including late-bound `Worksheets("S").Range(...).Cells(r, c)`) run over the existing
/// event session loan in both Automatic and Manual modes with exact values and handler order.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_1nbp_7_worksheet_and_workbook_event_objects_over_session_loan_in_both_modes() {
    use calc_host::{SheetEdit, SheetKey, SheetName};
    use calc_host_vba::{ModuleSource, ModuleTarget};

    let sheet_source = "\
Private Sub Worksheet_Change(ByVal Target As Excel.Range)
 History = History & \"Chg:\" & Target.Address(False, False) & \";\"
 If Target.Column <> 1 Or Target.Row <> 1 Then Exit Sub
 Dim v As Double
 v = CDbl(Target.Value2)
 Me.Range(\"C1\").Value2 = v * 10
 Worksheets(\"S\").Range(\"D1\").Value2 = CDbl(Me.Range(\"C1\").Value2) + 1
 ThisWorkbook.Worksheets(\"S\").Range(\"E1\").Value2 = CDbl(Worksheets(\"S\").Range(\"D1\").Value2) + 2
 Me.Range(\"A1:B2\").Cells(2, 2).Value2 = v + 5
 Worksheets(\"S\").Range(\"A1:B2\").Cells(2, 2).Value2 = CDbl(Me.Range(\"A1:B2\").Cells(2, 2).Value2) + 100
 Worksheets(\"S\").Cells(3, 1) = 77
 Dim wb As Object
 Set wb = ThisWorkbook
 wb.Worksheets(\"S\").Cells(3, 2).Value2 = 88
End Sub
Private Sub Worksheet_Calculate()
 History = History & \"Calc;\"
End Sub
";
    for backend in backends() {
        for include_this_workbook in [false, true] {
            for manual in [false, true] {
                let mut session = opened(&calc_host::blank_package(), "p22.xlsm");
                assert!(
                    session
                        .dispatch(SkinIntent::Edit {
                            edits: vec![calc_host::CellEdit::Sheet(SheetEdit::Insert {
                                key: SheetKey::new(2),
                                name: SheetName::new(
                                    calc_value::ExcelText::from_interop_assignment("S")
                                )
                                .unwrap(),
                                position: std::num::NonZeroU32::new(2).unwrap(),
                            })],
                        })
                        .accepted()
                );
                let sheet1 = session.host().sheets()[0].id();
                let sheet_s = session.host().sheets()[1].id();
                // Install formulas before attaching event handlers, then calculate baseline.
                session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
                assert!(
                    session
                        .dispatch(SkinIntent::EnterCell {
                            at: calc_value::CellKey::new(sheet1, 1, 6).unwrap(),
                            text: "=C1+S!D1+S!E1+B2+S!B2".into(),
                        })
                        .accepted()
                );
                assert!(
                    session
                        .dispatch(SkinIntent::EnterCell {
                            at: calc_value::CellKey::new(sheet_s, 1, 6).unwrap(),
                            text: "=D1+E1+B2".into(),
                        })
                        .accepted()
                );
                session.dispatch(SkinIntent::Recalculate);
                drain(&mut session);
                if !manual {
                    session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Automatic));
                    drain(&mut session);
                }

                let mut modules = vec![
                    ModuleSource {
                        name: "Api".into(),
                        source: OWNER_STATE.into(),
                        target: ModuleTarget::Standard,
                    },
                    ModuleSource {
                        name: "Sheet1".into(),
                        source: sheet_source.into(),
                        target: ModuleTarget::Worksheet(sheet1),
                    },
                ];
                if include_this_workbook {
                    modules.push(ModuleSource {
                        name: "ThisWorkbook".into(),
                        source: String::new(),
                        target: ModuleTarget::Workbook,
                    });
                }
                let project = calc_host_vba::prepare(
                    session.host(),
                    "p22.xlsm",
                    &modules,
                    50_000,
                    Some(backend),
                )
                .unwrap();
                project.clone().install(&mut session);

                enter(&mut session, 1, 1, "3");
                assert!(
                    session.workbook_event_failures().failures.is_empty(),
                    "{backend:?} wb={include_this_workbook} manual={manual}: {:?}",
                    session.workbook_event_failures()
                );

                let cell_num = |session: &HostSession, sheet: calc_value::SheetId, r, c| {
                    let at = calc_value::CellKey::new(sheet, r, c).unwrap();
                    let page = calc_host::project(
                        session.host(),
                        &calc_host::ProjectionRequest { cells: vec![at] },
                    )
                    .unwrap();
                    match page.cells()[0].value() {
                        calc_host::ProjectedValue::Literal(v)
                        | calc_host::ProjectedValue::Calculated { value: v, .. } => {
                            v.as_number().unwrap()
                        }
                        other => panic!("unexpected value at ({r},{c}): {other:?}"),
                    }
                };
                assert_eq!(cell_num(&session, sheet1, 1, 3), 30.0);
                assert_eq!(cell_num(&session, sheet_s, 1, 4), 31.0);
                assert_eq!(cell_num(&session, sheet_s, 1, 5), 33.0);
                assert_eq!(cell_num(&session, sheet1, 2, 2), 8.0);
                assert_eq!(cell_num(&session, sheet_s, 2, 2), 108.0);
                assert_eq!(cell_num(&session, sheet_s, 3, 1), 77.0);
                assert_eq!(cell_num(&session, sheet_s, 3, 2), 88.0);

                let stale_zero = |session: &HostSession, sheet: calc_value::SheetId, r, c| {
                    let at = calc_value::CellKey::new(sheet, r, c).unwrap();
                    let page = calc_host::project(
                        session.host(),
                        &calc_host::ProjectionRequest { cells: vec![at] },
                    )
                    .unwrap();
                    matches!(
                        page.cells()[0].value(),
                        calc_host::ProjectedValue::Stale(stale)
                            if stale.last_value().and_then(|v| v.as_number()) == Some(0.0)
                    )
                };
                let trace = || {
                    project
                        .functions()
                        .call("Trace", &[])
                        .unwrap()
                        .as_text()
                        .unwrap()
                        .to_string_lossy()
                };
                if manual {
                    assert_eq!(
                        trace(),
                        "Chg:A1;Chg:C1;Chg:B2;",
                        "{backend:?} wb={include_this_workbook} manual pre-recalc"
                    );
                    assert!(stale_zero(&session, sheet1, 1, 6));
                    assert!(stale_zero(&session, sheet_s, 1, 6));
                    session.dispatch(SkinIntent::Recalculate);
                    drain(&mut session);
                    assert_eq!(
                        trace(),
                        "Chg:A1;Chg:C1;Chg:B2;Calc;",
                        "{backend:?} wb={include_this_workbook} manual post-recalc"
                    );
                } else {
                    assert_eq!(
                        trace(),
                        "Chg:A1;Calc;Chg:C1;Calc;Calc;Calc;Chg:B2;Calc;",
                        "{backend:?} wb={include_this_workbook} automatic"
                    );
                }
                assert_eq!(cell_num(&session, sheet1, 1, 6), 210.0);
                assert_eq!(cell_num(&session, sheet_s, 1, 6), 172.0);
            }
        }
    }
}

/// dc-1nbp.7: multi-area writes through Worksheet/Workbook objects keep the same typed
/// refusals (unmaskable by `On Error Resume Next`, atomic on the sheet) as Application.Range.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_1nbp_7_multi_area_writes_keep_typed_refusals_through_error_handling() {
    use calc_host::{SheetEdit, SheetKey, SheetName};
    for backend in backends() {
        for (label, stmt) in [
            (
                "shape_mismatch",
                "Me.Range(\"A1:B1,D1\").Value2 = Array(1, 2)",
            ),
            (
                "multi_area_formula",
                "Worksheets(\"S\").Range(\"D1,E1\").Formula = \"=1+1\"",
            ),
        ] {
            let mut session = opened(&calc_host::blank_package(), "multi.xlsm");
            session.dispatch(SkinIntent::SetCalcMode(calc_host::CalcMode::Manual));
            assert!(
                session
                    .dispatch(SkinIntent::Edit {
                        edits: vec![calc_host::CellEdit::Sheet(SheetEdit::Insert {
                            key: SheetKey::new(2),
                            name: SheetName::new(calc_value::ExcelText::from_interop_assignment(
                                "S"
                            ))
                            .unwrap(),
                            position: std::num::NonZeroU32::new(2).unwrap(),
                        })],
                    })
                    .accepted()
            );
            let source = format!(
                "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n \
                 If Target.Column <> 1 Or Target.Row <> 1 Then Exit Sub\n \
                 On Error Resume Next\n \
                 {stmt}\n \
                 History = \"after:\" & CStr(Err.Number)\n\
                 End Sub\n"
            );
            let _project = mutable_owner(&mut session, &source, OWNER_STATE, 20_000, backend);
            enter(&mut session, 1, 1, "7");
            let failures = session.take_workbook_event_failures();
            assert_eq!(
                failures.failures.len(),
                1,
                "{backend:?} {label}: {failures:?}"
            );
            assert!(
                failures.failures[0].reason.contains("dc-1nbp.6"),
                "{backend:?} {label}: {:?}",
                failures.failures[0]
            );
            let sheet_s = session.host().sheets()[1].id();
            assert!(
                session
                    .host()
                    .model()
                    .cell(calc_value::CellKey::new(sheet_s, 1, 4).unwrap())
                    .is_none(),
                "{backend:?} {label}: S!D1 stayed clean"
            );
        }
    }
}
