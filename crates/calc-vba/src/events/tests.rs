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
fn unsupported_document_events_refuse_admission_and_inactive_source_stays_inactive() {
    for (kind, name) in [(ModuleKind::Workbook, "Workbook_Activate"), (ModuleKind::Worksheet, "Worksheet_SelectionChange")] {
        let source = format!("Private Sub {name}()\nEnd Sub\n");
        let refusal = load_project(&[module("Doc", kind, &source)], 10000, Some(Backend::Interpreter)).err().unwrap();
        assert!(refusal.contains(name) && refusal.contains("dc-hsa5.12"), "{refusal}");
        let inactive = format!("#Const IncludeEvent = 0\n#If IncludeEvent Then\n{source}#End If\n");
        assert!(load_project(&[module("Doc", kind, &inactive)], 10000, Some(Backend::Interpreter)).is_ok());
    }
    let renamed = module("Doc", ModuleKind::Workbook,
        "Attribute VB_Name = \"Other\"\nPrivate Sub Workbook_Open()\nEnd Sub\n");
    let refusal = load_project(&[renamed], 10000, Some(Backend::Interpreter)).err().unwrap();
    assert!(refusal.contains("directory identity") && refusal.contains("dc-snz.15"), "{refusal}");
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
fn automatic_before_save_keeps_boolean_arguments_shared_state_and_reset_subscriptions() {
    let modules = [
        module(
            "Api",
            ModuleKind::Standard,
            "Public Entries As Long\nPublic Last As Boolean\nPublic Function SaveState() As String\n SaveState = CStr(Entries) & \":\" & CStr(Last)\nEnd Function\n",
        ),
        module(
            "ThisWorkbook",
            ModuleKind::Workbook,
            "Private Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)\n Entries = Entries + 1\n Last = SaveAsUI\n Cancel = SaveAsUI\nEnd Sub\n",
        ),
    ];
    for backend in backends() {
        let project = load_project(&modules, 10000, Some(backend)).unwrap();
        let functions = project.functions();
        assert!(
            !functions
                .names()
                .any(|name| name.eq_ignore_ascii_case("Workbook_BeforeSave"))
        );
        let source = EventSource::workbook("Book");
        project.attach_document("ThisWorkbook", &source).unwrap();
        assert_eq!(source.subscription_count(), 1);
        assert_eq!(source.before_save(false, &Read, 10000), Ok(false));
        assert_eq!(source.before_save(true, &Read, 10000), Ok(true));
        assert_eq!(
            functions
                .call("SaveState", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "2:True"
        );
        project.reset().unwrap();
        assert_eq!(source.subscription_count(), 1);
        assert_eq!(source.before_save(false, &Read, 10000), Ok(false));
        assert_eq!(
            functions
                .call("SaveState", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "1:False"
        );
        drop(functions);
        drop(project);
        assert_eq!(source.subscription_count(), 0);
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn automatic_before_save_rejects_a_non_boolean_handler_before_attachment() {
    let modules = [module(
        "ThisWorkbook",
        ModuleKind::Workbook,
        "Private Sub Workbook_BeforeSave(ByVal SaveAsUI As Double, Cancel As Boolean)\n Cancel = True\nEnd Sub\n",
    )];
    let error = load_project(&modules, 1000, Some(Backend::Interpreter))
        .err()
        .expect("mismatched event signature");
    assert!(
        error.contains("event handler")
            && error.contains("signature does not match")
            && error.contains("BeforeSave")
            && error.contains("dc-hsa5.12"),
        "{error}"
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn automatic_before_save_drains_unhandled_and_budget_faults_without_repeating_entries() {
    for backend in backends() {
        for (failure, number) in [
            ("Err.Raise 5", "5"),
            ("For i = 1 To 100000\n i = i\nNext i", "-1"),
        ] {
            let modules = [
                module(
                    "Api",
                    ModuleKind::Standard,
                    "Public Entries As Long\nPublic Function EntryCount() As Long\n EntryCount = Entries\nEnd Function\n",
                ),
                module(
                    "ThisWorkbook",
                    ModuleKind::Workbook,
                    &format!(
                        "Private Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)\n Dim i As Long\n Entries = Entries + 1\n Cancel = True\n If Entries = 1 Then\n {failure}\n End If\nEnd Sub\n"
                    ),
                ),
            ];
            let project = load_project(&modules, 10000, Some(backend)).unwrap();
            let source = EventSource::workbook("Book");
            project.attach_document("ThisWorkbook", &source).unwrap();
            // Keep the same event callsite; no UDF probe/reset may clear a leaked fault.
            let outcomes: Vec<_> = (0..2)
                .map(|_| source.before_save(false, &Read, 1000))
                .collect();
            let error = outcomes[0].as_ref().unwrap_err();
            assert!(
                error.contains("BeforeSave") && error.contains(&format!("({number}:")),
                "{backend:?}: {error}"
            );
            assert_eq!(outcomes[1], Ok(true), "{backend:?}: {failure}");
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
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn automatic_before_save_allows_a_caught_ordinary_error_to_cancel() {
    let modules = [module(
        "ThisWorkbook",
        ModuleKind::Workbook,
        "Private Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)\n On Error Resume Next\n Err.Raise 5\n Cancel = True\nEnd Sub\n",
    )];
    for backend in backends() {
        let project = load_project(&modules, 10000, Some(backend)).unwrap();
        let source = EventSource::workbook("Book");
        project.attach_document("ThisWorkbook", &source).unwrap();
        assert_eq!(source.before_save(false, &Read, 10000), Ok(true));
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn document_sources_validate_the_interface_before_exposing_self_members() {
    let modules = [
        module(
            "Api",
            ModuleKind::Standard,
            "Public Entries As Long\nPublic Last As String\nPublic Function Probe() As Long\n Probe = Entries\nEnd Function\nPublic Function LastName() As String\n LastName = Last\nEnd Function\n",
        ),
        module(
            "Sheet1",
            ModuleKind::Worksheet,
            "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Entries = Entries + 1\n Last = Me.Name\nEnd Sub\n",
        ),
    ];
    for backend in backends() {
        let project = load_project(&modules, 10000, Some(backend)).unwrap();
        let wrong = EventSource::workbook("other");
        let error = project.attach_document("Sheet1", &wrong).unwrap_err();
        assert!(
            error.contains("automatic document attachment")
                && error.contains("procedure argument count/type mismatch")
                && error.contains("dc-hsa5.12"),
            "{error}"
        );
        assert_eq!(wrong.subscription_count(), 0);
        assert_eq!(
            project.functions().call("Probe", &[]).unwrap().as_number(),
            Some(0.0)
        );
        let source = EventSource::worksheet("S");
        project.attach_document("Sheet1", &source).unwrap();
        assert_eq!(source.subscription_count(), 1);
        source.change(target(), &Read, 10000).unwrap();
        assert_eq!(
            project.functions().call("Probe", &[]).unwrap().as_number(),
            Some(1.0)
        );
        assert_eq!(
            project
                .functions()
                .call("LastName", &[])
                .unwrap()
                .as_text()
                .unwrap()
                .to_string_lossy(),
            "S"
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
