use super::*;
use calc_func::value::{CalcValue, CellKey, IdAllocator, ReferenceLike, SheetId};
fn backends() -> Vec<Backend> {
    let mut v = vec![Backend::Interpreter];
    #[cfg(not(target_arch = "wasm32"))]
    v.push(Backend::Jit);
    v
}
struct Read;
impl EventReadContext for Read {
    fn read_reference(&self, _: &ReferenceLike) -> Result<CalcValue, String> {
        Ok(CalcValue::number(41.0))
    }
}
fn target() -> ReferenceLike {
    let s = IdAllocator::<SheetId>::new().issue().unwrap();
    ReferenceLike::cell(CellKey::new(s, 1, 1).unwrap())
}
fn module(name: &str, kind: ModuleKind, source: &str) -> ProjectModule {
    ProjectModule {
        name: name.into(),
        kind,
        source: source.into(),
    }
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn automatic_private_handlers_share_udf_state_reset_sources_and_unsubscribe_on_drop() {
    let modules = [
        module(
            "Api",
            ModuleKind::Standard,
            "Public Changes As Long\nPublic Calcs As Long\nPublic Last As String\nPublic Function Probe() As String\n Probe = CStr(Changes) & \":\" & CStr(Calcs) & \":\" & Last\nEnd Function\n",
        ),
        module(
            "Sheet1",
            ModuleKind::Worksheet,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Changes = Changes + 1\n Last = Target.Address & \":\" & CStr(Target.Row) & \":\" & CStr(Target.Column) & \":\" & CStr(Target.Count) & \":\" & CStr(Target.Value2)\nEnd Sub\nPrivate Sub Worksheet_Calculate()\n Calcs = Calcs + 1\nEnd Sub\n",
        ),
    ];
    for backend in backends() {
        let project = load_project(&modules, 10000, Some(backend)).unwrap();
        let functions = project.functions();
        assert!(
            !functions
                .names()
                .any(|name| name.to_ascii_lowercase().contains("worksheet_"))
        );
        let source = EventSource::worksheet("S");
        project.attach_document("Sheet1", &source).unwrap();
        assert_eq!(source.subscription_count(), 2);
        assert!(
            project
                .attach_document("Sheet1", &EventSource::worksheet("other"))
                .is_err()
        );
        source.change(target(), &Read, 10000).unwrap();
        source.calculate(&Read, 10000).unwrap();
        assert_eq!(
            functions
                .call("Probe", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "1:1:$A$1:1:1:1:41",
            "{backend:?}"
        );
        project.reset().unwrap();
        assert_eq!(source.subscription_count(), 2);
        source.change(target(), &Read, 10000).unwrap();
        assert_eq!(
            functions
                .call("Probe", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "1:0:$A$1:1:1:1:41"
        );
        drop(functions);
        drop(project);
        assert_eq!(
            source.subscription_count(),
            0,
            "source callbacks cannot retain a dead project"
        );
    }
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn event_budget_exhaustion_does_not_retry_the_handler_prefix() {
    let modules = [
        module(
            "Api",
            ModuleKind::Standard,
            "Public Entries As Long\nPublic Function Probe() As Long\n Probe = Entries\nEnd Function\n",
        ),
        module(
            "Sheet1",
            ModuleKind::Worksheet,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim i As Long\n Entries = Entries + 1\n For i = 1 To 100000\n  i = i\n Next i\nEnd Sub\n",
        ),
    ];
    for backend in backends() {
        let project = load_project(&modules, 10000, Some(backend)).unwrap();
        let source = EventSource::worksheet("S");
        project.attach_document("Sheet1", &source).unwrap();
        assert!(source.change(target(), &Read, 5).is_err());
        assert_eq!(
            project.functions().call("Probe", &[]).unwrap().as_number(),
            Some(1.0)
        );
    }
}
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn before_save_keeps_its_actual_boolean_signature_and_refuses_the_missing_carrier() {
    let modules = [module(
        "ThisWorkbook",
        ModuleKind::Workbook,
        "Private Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)\n Cancel = True\nEnd Sub\n",
    )];
    let error = load_project(&modules, 1000, Some(Backend::Interpreter))
        .err()
        .expect("runtime has no plain Boolean event argument carrier");
    assert!(
        error.contains("BeforeSave") && error.contains("carrier") && error.contains("dc-hsa5.12"),
        "{error}"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn unsupported_document_members_refuse_before_any_subscription_is_published() {
    let modules = [
        module(
            "Api",
            ModuleKind::Standard,
            "Public Entries As Long\nPublic Function Probe() As Long\n Probe = Entries\nEnd Function\n",
        ),
        module(
            "Sheet1",
            ModuleKind::Worksheet,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim s As String\n Entries = Entries + 1\n s = Me.Name\nEnd Sub\n",
        ),
    ];
    for backend in backends() {
        let project = load_project(&modules, 10000, Some(backend)).unwrap();
        let source = EventSource::worksheet("S");
        let error = project.attach_document("Sheet1", &source).unwrap_err();
        assert!(
            error.contains("unsupported") && error.contains("dc-hsa5.12"),
            "{error}"
        );
        assert_eq!(source.subscription_count(), 0);
        assert_eq!(
            project.functions().call("Probe", &[]).unwrap().as_number(),
            Some(0.0)
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn event_read_refusals_and_expired_targets_cannot_be_hidden_by_vba_error_handlers() {
    struct Refused;
    impl EventReadContext for Refused {
        fn read_reference(&self, _: &ReferenceLike) -> Result<CalcValue, String> {
            Err("fixture read not modelled".into())
        }
    }
    struct ValueRead(CalcValue);
    impl EventReadContext for ValueRead {
        fn read_reference(&self, _: &ReferenceLike) -> Result<CalcValue, String> {
            Ok(self.0.clone())
        }
    }
    let modules = [
        module(
            "Api",
            ModuleKind::Standard,
            "Public Held As Excel.Range\nPublic Function ReadHeld() As Double\n On Error Resume Next\n ReadHeld = Held.Value2 + 7#\nEnd Function\n",
        ),
        module(
            "Sheet1",
            ModuleKind::Worksheet,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Dim v As Variant\n On Error Resume Next\n Set Held = Target\n v = Target.Value2\nEnd Sub\n",
        ),
    ];
    for backend in backends() {
        let project = load_project(&modules, 10000, Some(backend)).unwrap();
        let source = EventSource::worksheet("S");
        project.attach_document("Sheet1", &source).unwrap();
        let error = source.change(target(), &Refused, 10000).unwrap_err();
        assert!(
            error.contains("fixture read not modelled") && error.contains("dc-hsa5.12"),
            "{error}"
        );
        for value in [
            CalcValue::error(calc_func::value::WorksheetErrorCode::Busy),
            CalcValue::text(calc_func::value::ExcelText::from_utf16_code_units(vec![
                0xd800,
            ])),
        ] {
            let error = source
                .change(target(), &ValueRead(value), 10000)
                .unwrap_err();
            assert!(
                error.contains("dc-hsa5.12"),
                "a conversion refusal must survive On Error: {error}"
            );
        }
        source.change(target(), &Read, 10000).unwrap();
        let error = project.functions().call("ReadHeld", &[]).unwrap_err();
        assert!(error.contains("admitted event view"), "{error}");
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn document_project_owning_values_preserve_coercion_missing_and_variant_results() {
    let modules = [module(
        "Api",
        ModuleKind::Standard,
        "Public Function Echo(ByVal n As Long, ByVal x As Double, ByVal b As Boolean, ByVal s As String) As String\n Echo = s & \":\" & CStr(n) & \":\" & CStr(x) & \":\" & CStr(b)\nEnd Function\nPublic Function OptionalEcho(Optional v As Variant) As Variant\n If IsMissing(v) Then\n  OptionalEcho = \"missing\"\n Else\n  OptionalEcho = v\n End If\nEnd Function\n",
    )];
    for backend in backends() {
        let project = load_project(&modules, 10000, Some(backend)).unwrap();
        let args = vec![
            CalcValue::number(7.0),
            CalcValue::number(1.25),
            CalcValue::logical(true),
            CalcValue::text(calc_func::value::ExcelText::from_utf16_code_units(
                "hi".encode_utf16().collect(),
            )),
        ];
        assert_eq!(
            project
                .functions()
                .call("Echo", &args)
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "hi:7:1.25:True"
        );
        assert_eq!(
            project
                .functions()
                .call("OptionalEcho", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "missing"
        );
        let error = CalcValue::error(calc_func::value::WorksheetErrorCode::NA);
        assert_eq!(
            project
                .functions()
                .call("OptionalEcho", &[error.clone()])
                .unwrap(),
            error
        );
        let too_many = [module(
            "Api",
            ModuleKind::Standard,
            "Public Function TooMany(a As Long,b As Long,c As Long,d As Long,e As Long) As Long\n TooMany = a+b+c+d+e\nEnd Function\n",
        )];
        let refusal = load_project(&too_many, 10000, Some(backend)).err().unwrap();
        assert!(
            refusal.contains("argument/slot bounds") && refusal.contains("dc-hsa5.12"),
            "{refusal}"
        );
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn event_target_count_uses_long_and_preserves_documented_overflow() {
    let modules = [
        module(
            "Api",
            ModuleKind::Standard,
            "Public Result As Long\nPublic Function Probe() As Long\n Probe = Result\nEnd Function\n",
        ),
        module(
            "Sheet1",
            ModuleKind::Worksheet,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n On Error GoTo Failed\n Result = Target.Count\n Exit Sub\nFailed:\n Result = Err.Number\nEnd Sub\n",
        ),
    ];
    for backend in backends() {
        let project = load_project(&modules, 10000, Some(backend)).unwrap();
        let source = EventSource::worksheet("S");
        project.attach_document("Sheet1", &source).unwrap();
        let sheet = target().sheet();
        source
            .change(
                ReferenceLike::area(calc_func::value::GridArea::full_sheet(sheet)),
                &Read,
                10000,
            )
            .unwrap();
        assert_eq!(
            project.functions().call("Probe", &[]).unwrap().as_number(),
            Some(6.0)
        );
    }
}
