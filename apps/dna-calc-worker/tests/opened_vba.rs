//! File projects use the same worker workbook and event route as source projects.
use calc_host::{ProjectedValue, SkinIntent, WorkBudget};
use dna_calc_worker::workbook::{VbaOpenPolicy, Workbook};
#[path = "../../../crates/calc-doc/tests/cdoc_m1_1_support/mod.rs"]
#[allow(dead_code)]
mod support;
#[path = "../../../crates/calc-doc/tests/cdoc_m1_1_support/vba_project.rs"]
mod vba_project;

const LOCAL: &[u8] =
    include_bytes!("../../../crates/calc-doc/tests/fixtures/vba_udf_names/local.xlsm");

fn drain(book: &mut Workbook) {
    for _ in 0..1000 {
        if !book.has_pending_calculation() {
            return;
        }
        let progress = book.advance_calculation();
        assert!(
            !matches!(
                progress,
                calc_host::HostProgress::Failed(_) | calc_host::HostProgress::Waiting { .. }
            ),
            "{progress:?}"
        );
    }
    panic!("worker did not complete");
}
fn at(book: &Workbook, column: u32) -> calc_value::CellKey {
    calc_value::CellKey::new(book.host().sheets()[0].id(), 1, column).unwrap()
}
fn number(book: &Workbook, column: u32) -> f64 {
    match book.project(vec![at(book, column)])[0].value() {
        ProjectedValue::Calculated { value, .. } => value.as_number().unwrap(),
        other => panic!("not an engine calculation: {other:?}"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn worker_opens_file_udfs_and_preserve_policy_executes_no_project() {
    for budget in [WorkBudget::UNLIMITED, WorkBudget::evaluations(1)] {
        let mut binary = Workbook::open_with_budget(
            "local.xlsb",
            include_bytes!("../../../crates/calc-doc/tests/fixtures/vba_udf_names/local.xlsb")
                .to_vec(),
            budget,
        )
        .unwrap();
        binary.dispatch(SkinIntent::Recalculate);
        drain(&mut binary);
        let cells = (1..=4)
            .map(|row| calc_value::CellKey::new(binary.host().sheets()[0].id(), row, 2).unwrap())
            .collect();
        for cell in binary.project(cells) {
            assert!(
                matches!(cell.value(), ProjectedValue::Calculated { value, .. } if value.as_number() == Some(102.)),
                "{:?}",
                cell.value()
            );
        }
        let mut book = Workbook::open_with_budget("local.xlsm", LOCAL.to_vec(), budget).unwrap();
        book.dispatch(SkinIntent::Recalculate);
        drain(&mut book);
        assert_eq!(number(&book, 2), 102.);
        let (saved, _) = book.save().unwrap();
        assert_eq!(
            support::d1::part(&saved, "xl/vbaProject.bin"),
            support::d1::part(LOCAL, "xl/vbaProject.bin")
        );
        let mut book = Workbook::open_with_project_policy(
            "local.xlsm",
            LOCAL.to_vec(),
            budget,
            VbaOpenPolicy::Preserve,
        )
        .unwrap();
        book.dispatch(SkinIntent::Recalculate);
        drain(&mut book);
        assert!(matches!(
            book.project(vec![at(&book, 2)])[0].value(),
            ProjectedValue::NotModeled { .. }
        ));
        assert!(book.take_workbook_event_failures().failures.is_empty());
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn worker_file_events_and_udfs_share_one_state_through_open_edit_and_save() {
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            (
                "Module1",
                "Public Calls As Long\nPublic Function FileState() As Long\n FileState = Calls\nEnd Function\n",
            ),
            ("PrivateModule", "Option Private Module\n"),
            (
                "Sheet1",
                "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n Calls = Calls + 10\nEnd Sub\n",
            ),
            (
                "ThisWorkbook",
                "Private Sub Workbook_Open()\n Calls = Calls + 1\nEnd Sub\nPrivate Sub Workbook_BeforeSave(ByVal SaveAsUI As Boolean, Cancel As Boolean)\n Calls = Calls + 100\n Cancel = True\nEnd Sub\n",
            ),
        ],
    );
    for budget in [WorkBudget::UNLIMITED, WorkBudget::evaluations(1)] {
        let mut book = Workbook::open_with_budget("state.xlsm", bytes.clone(), budget).unwrap();
        let cell = at(&book, 4);
        assert!(
            book.dispatch(SkinIntent::EnterCell {
                at: cell,
                text: "=FileState()".into()
            })
            .accepted()
        );
        book.dispatch(SkinIntent::Recalculate);
        drain(&mut book);
        assert_eq!(number(&book, 4), 11.);
        assert!(book.save().unwrap_err().contains("cancelled"));
        book.dispatch(SkinIntent::Calculate {
            scope: calc_host::CalcScope::Workbook,
            selection: calc_host::EvalSelection::ForceAll,
        });
        drain(&mut book);
        assert_eq!(number(&book, 4), 111.);
        assert!(book.take_workbook_event_failures().failures.is_empty());
    }
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn worker_file_open_fault_is_reported_once_and_unsupported_projects_stay_typed() {
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            (
                "Module1",
                "Public Calls As Long\nPublic Function FileState() As Long\n FileState = Calls\nEnd Function\n",
            ),
            ("PrivateModule", "Option Private Module\n"),
            ("Sheet1", ""),
            (
                "ThisWorkbook",
                "Private Sub Workbook_Open()\n Calls = Calls + 1\n Err.Raise 5\nEnd Sub\n",
            ),
        ],
    );
    let mut book = Workbook::open("fault.xlsm", bytes).unwrap();
    let failures = book.take_workbook_event_failures();
    assert_eq!(failures.failures.len(), 1);
    assert!(matches!(
        failures.failures[0].event,
        calc_host::WorkbookEvent::Open
    ));
    assert!(failures.failures[0].reason.contains("5:"));
    let cell = at(&book, 4);
    book.dispatch(SkinIntent::EnterCell {
        at: cell,
        text: "=FileState()".into(),
    });
    book.dispatch(SkinIntent::Recalculate);
    drain(&mut book);
    assert_eq!(number(&book, 4), 1.);
    assert!(book.take_workbook_event_failures().failures.is_empty());

    let unsupported = include_bytes!(
        "../../../crates/calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/cases/automatic-basic/artifact.xlsm"
    );
    let refusal = match Workbook::open("p22.xlsm", unsupported.to_vec()) {
        Err(dna_calc_worker::workbook::OpenRefused::Vba(
            calc_host_vba::FileProjectError::Preparation(detail),
        )) => detail,
        Err(other) => panic!("expected the remaining automatic-event refusal: {other:?}"), // ubs:ignore[rust.ownership.panic-macro] A different refusal must fail this regression.
        Ok(_) => panic!("unsupported automatic events cannot run on file open"), // ubs:ignore[rust.ownership.panic-macro] Unexpected admission must fail this regression.
    };
    assert!(
        refusal.contains("unsupported automatic-event prefix"),
        "{refusal}"
    );
    assert!(refusal.contains("dc-hsa5.12"), "{refusal}");
    let designer = vba_project::replace_project_declaration(
        unsupported,
        "Class=P22Events",
        "BaseClass=P22Events",
    );
    assert!(matches!(
        Workbook::open("designer.xlsm", designer),
        Err(dna_calc_worker::workbook::OpenRefused::Vba(
            calc_host_vba::FileProjectError::UnsupportedModule(name)
        )) if name == "P22Events"
    ));
    assert!(
        Workbook::open_with_project_policy(
            "p22.xlsm",
            unsupported.to_vec(),
            WorkBudget::evaluations(1),
            VbaOpenPolicy::Preserve
        )
        .is_ok()
    );
}

/// A host that publishes its add-ins' functions to an opened macro workbook (the desktop app with
/// an .xll or .xlam) keeps the workbook's own VBA functions beside them (dc-0f1k.4).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn add_in_functions_join_the_file_projects_functions() {
    let mut book = Workbook::open("local.xlsm", LOCAL.to_vec()).unwrap();
    let addin = calc_vba::load_module(
        "Extra",
        "Public Function AddinSeven() As Double\n AddinSeven = 7\nEnd Function\n",
        1000,
    )
    .unwrap();
    book.set_user_functions(addin);
    let cell = at(&book, 4);
    assert!(
        book.dispatch(SkinIntent::EnterCell {
            at: cell,
            text: "=AddinSeven()+MyFunc(1)".into()
        })
        .accepted()
    );
    book.dispatch(SkinIntent::Calculate {
        scope: calc_host::CalcScope::Workbook,
        selection: calc_host::EvalSelection::ForceAll,
    });
    drain(&mut book);
    assert_eq!(number(&book, 4), 108.);
    assert_eq!(number(&book, 2), 102.);
}

/// A worksheet function whose signature the file-project route cannot carry (a Date result) is a
/// typed refusal of its own; the project's other functions still answer (dc-0f1k.4).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn one_unsupported_signature_refuses_only_its_own_function() {
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            (
                "Module1",
                "Public Function MyFunc(ByVal x As Double) As Double
 MyFunc = x + 100
End Function
Public Function WhenDue() As Date
 WhenDue = DateSerial(2026, 11, 11)
End Function
",
            ),
            ("PrivateModule", "Option Private Module
"),
        ],
    );
    let mut book = Workbook::open("mixed.xlsm", bytes).unwrap();
    let (good, refused) = (at(&book, 4), at(&book, 5));
    for (cell, text) in [(good, "=MyFunc(1)"), (refused, "=WhenDue()")] {
        assert!(
            book.dispatch(SkinIntent::EnterCell {
                at: cell,
                text: text.into()
            })
            .accepted()
        );
    }
    book.dispatch(SkinIntent::Recalculate);
    drain(&mut book);
    assert_eq!(number(&book, 4), 101.);
    assert!(
        !matches!(
            book.project(vec![refused])[0].value(),
            ProjectedValue::Calculated { value, .. } if value.as_number().is_some()
        ),
        "{:?}",
        book.project(vec![refused])[0].value()
    );
}
