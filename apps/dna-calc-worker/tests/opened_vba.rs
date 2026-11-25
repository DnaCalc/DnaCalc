//! File projects use the same worker workbook and event route as source projects.
use calc_host::{ProjectedValue, SkinIntent, WorkBudget};
use dna_calc_worker::workbook::{EnableRefused, FileOrigin, Macros, Workbook};
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

fn not_modelled(book: &Workbook, column: u32) -> bool {
    matches!(
        book.project(vec![at(book, column)])[0].value(),
        ProjectedValue::NotModeled { .. }
    )
}

/// An opened macro workbook runs nothing until the person enables macros (dc-0f1k.22): its
/// function is a typed refusal and its project is held and saved back; enabling compiles it and
/// its function answers. Enabling twice does nothing more.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn an_opened_macro_workbook_runs_nothing_until_macros_are_enabled() {
    for budget in [WorkBudget::UNLIMITED, WorkBudget::evaluations(1)] {
        let mut binary = Workbook::open_with_budget(
            "local.xlsb",
            include_bytes!("../../../crates/calc-doc/tests/fixtures/vba_udf_names/local.xlsb")
                .to_vec(),
            budget,
        )
        .unwrap();
        assert_eq!(
            binary.macros(),
            &Macros::Blocked(calc_host_vba::MacroBlock::Unknown)
        );
        assert!(binary.enable_macros().is_err());
        binary
            .enable_macros_as(dna_calc_worker::workbook::EnableMode::Unblock)
            .unwrap();
        assert_eq!(binary.macros(), &Macros::Enabled);
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
        for origin in [FileOrigin::Local, FileOrigin::Zone(2)] {
            let mut book =
                Workbook::open_with_project_policy("local.xlsm", LOCAL.to_vec(), budget, origin)
                    .unwrap();
            assert_eq!(book.macros(), &Macros::Disabled(origin));
            drain(&mut book);
            assert!(not_modelled(&book, 2), "the project does not run on open");
            assert!(book.take_workbook_event_failures().failures.is_empty());
            let (saved, _) = book.save().unwrap();
            assert_eq!(
                support::d1::part(&saved, "xl/vbaProject.bin"),
                support::d1::part(LOCAL, "xl/vbaProject.bin"),
                "a disabled project is saved back"
            );
            book.enable_macros().unwrap();
            drain(&mut book);
            assert_eq!(number(&book, 2), 102.);
            book.enable_macros().unwrap();
            let (saved, _) = book.save().unwrap();
            assert_eq!(
                support::d1::part(&saved, "xl/vbaProject.bin"),
                support::d1::part(LOCAL, "xl/vbaProject.bin")
            );
        }
        let plain = Workbook::open("plain.xlsx", calc_host::blank_package()).unwrap();
        assert_eq!(plain.macros(), &Macros::None);
    }
}

/// A file with the Mark of the Web (zone 3 or 4) or from the network is blocked: its project is
/// held and saved back, never run, and enabling it is refused (dc-0f1k.22).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_marked_or_network_macro_workbook_is_never_enabled() {
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[(
            "ThisWorkbook",
            "Private Sub Workbook_Open()\n Err.Raise 5\nEnd Sub\n",
        )],
    );
    for origin in [
        FileOrigin::Unknown,
        FileOrigin::Zone(3),
        FileOrigin::Zone(4),
        FileOrigin::Network,
    ] {
        let mut book = Workbook::open_with_project_policy(
            "marked.xlsm",
            bytes.clone(),
            WorkBudget::UNLIMITED,
            origin,
        )
        .unwrap();
        assert_eq!(book.macros(), &Macros::Blocked(origin.block().unwrap()));
        let refused = book.enable_macros().unwrap_err();
        assert!(matches!(refused, EnableRefused::Blocked(_)), "{refused}");
        assert!(
            refused.to_string().contains(match origin {
                FileOrigin::Unknown => "origin could not be verified",
                FileOrigin::Network => "network",
                _ => "Mark of the Web",
            }),
            "{refused}"
        );
        assert_eq!(book.macros(), &Macros::Blocked(origin.block().unwrap()));
        book.dispatch(SkinIntent::Recalculate);
        drain(&mut book);
        assert!(not_modelled(&book, 2));
        assert!(
            book.take_workbook_event_failures().failures.is_empty(),
            "Workbook_Open was never raised"
        );
        let (saved, _) = book.save().unwrap();
        assert_eq!(
            support::d1::part(&saved, "xl/vbaProject.bin"),
            support::d1::part(&bytes, "xl/vbaProject.bin")
        );
    }
}

/// A native host supplies Local origin: the workbook opens disabled and EnableMacros runs it. A native host's origin for the next open
/// (`set_open_origin`) blocks a marked file, and EnableMacros is refused (dc-0f1k.22).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_worker_opens_macros_disabled_and_enables_them_on_request() {
    use dna_calc_worker::server::Server;
    use dna_calc_worker::wire::{MacroStatus, Reply, Request};
    let status = |reply: &Result<Reply, String>| match reply {
        Ok(Reply::Done { outcome, .. }) => outcome.summary.macros.clone(),
        other => panic!("not done: {other:?}"), // ubs:ignore[rust.ownership.panic-macro] The open must answer.
    };
    let mut server = Server::new();
    server.set_open_origin(FileOrigin::Local);
    let opened = server.execute_current(
        Request::Open {
            name: "local.xlsm".into(),
        },
        Some(LOCAL.to_vec()),
    );
    assert_eq!(status(&opened.reply), MacroStatus::Disabled);
    let enabled = server.execute_current(Request::EnableMacros, None);
    assert_eq!(status(&enabled.reply), MacroStatus::Enabled);
    server.set_open_origin(FileOrigin::Zone(3));
    let marked = server.execute_current(
        Request::Open {
            name: "marked.xlsm".into(),
        },
        Some(LOCAL.to_vec()),
    );
    assert!(matches!(
        status(&marked.reply),
        MacroStatus::Blocked { reason } if reason.contains("Mark of the Web")
    ));
    let refused = server.execute_current(Request::EnableMacros, None);
    assert!(
        refused
            .reply
            .as_ref()
            .is_err_and(|why| why.contains("Mark of the Web")),
        "{:?}",
        refused.reply
    );
    // The origin was for that open only: the next unverified file requires Unblock.
    let next = server.execute_current(
        Request::Open {
            name: "local.xlsm".into(),
        },
        Some(LOCAL.to_vec()),
    );
    assert!(
        matches!(status(&next.reply), MacroStatus::Blocked { reason } if reason.contains("origin could not be verified"))
    );
    let plain = server.execute_current(Request::New, None);
    assert_eq!(status(&plain.reply), MacroStatus::None);
    assert!(
        server
            .execute_current(Request::EnableMacros, None)
            .reply
            .is_err()
    );
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
        let mut book = Workbook::open_with_project_policy(
            "state.xlsm",
            bytes.clone(),
            budget,
            FileOrigin::Local,
        )
        .unwrap();
        book.enable_macros().unwrap();
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
fn worker_file_open_fault_is_reported_once_and_designer_projects_stay_typed() {
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
    let mut book = Workbook::open_with_project_policy(
        "fault.xlsm",
        bytes,
        WorkBudget::UNLIMITED,
        FileOrigin::Local,
    )
    .unwrap();
    assert!(book.take_workbook_event_failures().failures.is_empty());
    book.enable_macros().unwrap();
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

    let captured = include_bytes!(
        "../../../crates/calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/cases/automatic-basic/artifact.xlsm"
    );
    let mut p22 = Workbook::open_with_project_policy(
        "p22.xlsm",
        captured.to_vec(),
        WorkBudget::UNLIMITED,
        FileOrigin::Local,
    )
    .unwrap();
    assert_eq!(p22.macros(), &Macros::Disabled(FileOrigin::Local));
    p22.enable_macros().unwrap();
    assert_eq!(p22.macros(), &Macros::Enabled);
    let designer = vba_project::replace_project_declaration(
        captured,
        "Class=P22Events",
        "BaseClass=P22Events",
    );
    // A designer (UserForm) module is admitted as a class: its code loads, its controls are not
    // modelled, and a form feature the code uses refuses at compile time with a typed error
    // (dc-oxh8w.73).
    let mut book = Workbook::open_with_project_policy(
        "designer.xlsm",
        designer,
        WorkBudget::UNLIMITED,
        FileOrigin::Local,
    )
    .unwrap();
    let enabled = book.enable_macros();
    assert!(
        enabled.is_ok(),
        "designer (UserForm) module without form controls must enable as a class: {enabled:?}"
    );
    assert_eq!(book.macros(), &Macros::Enabled);
}

/// A host that publishes its add-ins' functions to an opened macro workbook (the desktop app with
/// an .xll or .xlam) keeps the workbook's own VBA functions beside them (dc-0f1k.4).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn add_in_functions_join_the_file_projects_functions() {
    // The host publishes its add-ins before or after the person enables macros (the desktop app
    // installs them at open, before the bar's button).
    for addins_first in [false, true] {
        let mut book = Workbook::open_with_project_policy(
            "local.xlsm",
            LOCAL.to_vec(),
            WorkBudget::UNLIMITED,
            FileOrigin::Local,
        )
        .unwrap();
        let addin = calc_vba::load_module(
            "Extra",
            "Public Function AddinSeven() As Double\n AddinSeven = 7\nEnd Function\n",
            1000,
        )
        .unwrap();
        if addins_first {
            book.set_user_functions(addin);
            book.enable_macros().unwrap();
        } else {
            book.enable_macros().unwrap();
            book.set_user_functions(addin);
        }
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
}

/// A worksheet function whose signature the document project's owning typed route does not carry
/// (a Date result) runs on the dynamic runtime, as it does from an add-in: it answers Excel's
/// serial beside the project's other functions (dc-hsa5.12; it was a refusal of its own).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_function_the_typed_route_does_not_carry_answers_beside_the_others() {
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
            (
                "PrivateModule",
                "Option Private Module
",
            ),
        ],
    );
    let mut book = Workbook::open_with_project_policy(
        "mixed.xlsm",
        bytes,
        WorkBudget::UNLIMITED,
        FileOrigin::Local,
    )
    .unwrap();
    book.enable_macros().unwrap();
    let (good, date) = (at(&book, 4), at(&book, 5));
    for (cell, text) in [(good, "=MyFunc(1)"), (date, "=WhenDue()")] {
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
    assert_eq!(number(&book, 5), 46337.);
}

/// The macro security level (dc-15dk), end to end through the server's wire: a marked file offers
/// Unblock and enable under Off; Unguarded runs every file on open (a marked one with a notice);
/// Guarded runs it on open like Unguarded, under CalcVBA's capability policy; a pinned level cannot be changed.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_macro_security_level_decides_what_runs_and_what_the_buttons_do() {
    use dna_calc_model::{MacroPolicy, MacroSecurity, Pin};
    use dna_calc_worker::server::Server;
    use dna_calc_worker::wire::{MacroStatus, Reply, Request};
    let summary = |reply: &Result<Reply, String>| match reply {
        Ok(Reply::Done { outcome, .. }) => outcome.summary.clone(),
        other => panic!("not done: {other:?}"), // ubs:ignore[rust.ownership.panic-macro] The request must answer.
    };
    let open = |server: &mut Server, origin: FileOrigin| {
        server.set_open_origin(origin);
        summary(
            &server
                .execute_current(
                    Request::Open {
                        name: "book.xlsm".into(),
                    },
                    Some(LOCAL.to_vec()),
                )
                .reply,
        )
    };

    // Ask (the default): a marked file is blocked for the plain Enable and unblocked by the button.
    let mut server = Server::new();
    assert_eq!(server.macro_policy().level(), MacroSecurity::Off);
    let marked = open(&mut server, FileOrigin::Zone(3));
    assert!(matches!(marked.macros, MacroStatus::Blocked { .. }));
    assert!(marked.macro_note.is_empty());
    assert!(
        server
            .execute_current(Request::EnableMacros, None)
            .reply
            .is_err()
    );
    let unblocked = summary(&server.execute_current(Request::UnblockMacros, None).reply);
    assert_eq!(unblocked.macros, MacroStatus::Enabled);
    assert!(
        unblocked.macro_note.contains("you unblocked it"),
        "{}",
        unblocked.macro_note
    );
    // A network file unblocks the same way, and an unmarked file's Unblock is no more than Enable.
    open(&mut server, FileOrigin::Network);
    let network = summary(&server.execute_current(Request::UnblockMacros, None).reply);
    assert_eq!(network.macros, MacroStatus::Enabled);
    assert!(network.macro_note.contains("network"));
    open(&mut server, FileOrigin::Local);
    let local = summary(&server.execute_current(Request::UnblockMacros, None).reply);
    assert_eq!(
        (local.macros, local.macro_note.as_str()),
        (MacroStatus::Enabled, "")
    );

    // Enable all: the project runs on open, a marked file too, with a notice; choosing the level
    // through the wire changes later opens only.
    let chosen = summary(
        &server
            .execute_current(
                Request::SetMacroSecurity {
                    level: MacroSecurity::Unguarded,
                },
                None,
            )
            .reply,
    );
    assert_eq!(chosen.macro_policy.level(), MacroSecurity::Unguarded);
    let plain = open(&mut server, FileOrigin::Local);
    assert_eq!(
        (plain.macros, plain.macro_note.as_str()),
        (MacroStatus::Enabled, "")
    );
    let marked = open(&mut server, FileOrigin::Zone(4));
    assert_eq!(marked.macros, MacroStatus::Enabled);
    assert!(
        marked.macro_note.contains("Mark of the Web")
            && marked.macro_note.contains("macro security level"),
        "{}",
        marked.macro_note
    );
    assert_eq!(
        open(&mut server, FileOrigin::Network).macros,
        MacroStatus::Enabled
    );

    // Guarded (dc-10un, dc-8ovd) runs the project on open, a marked one too, under the Guarded policy.
    server
        .execute_current(
            Request::SetMacroSecurity {
                level: MacroSecurity::Guarded,
            },
            None,
        )
        .reply
        .unwrap();
    let guarded = open(&mut server, FileOrigin::Local);
    assert_eq!(guarded.macros, MacroStatus::Enabled);
    assert_eq!(guarded.macro_policy.level(), MacroSecurity::Guarded);
    assert_eq!(
        open(&mut server, FileOrigin::Zone(3)).macros,
        MacroStatus::Enabled
    );

    // The flag, the variable or --macros pins a level: the page cannot choose another.
    let mut pinned = Server::new();
    pinned.set_macro_policy(MacroPolicy::resolve(MacroSecurity::Guarded, true, None, None));
    let marked = open(&mut pinned, FileOrigin::Zone(3));
    assert_eq!(marked.macros, MacroStatus::Enabled);
    assert_eq!(marked.macro_policy.pinned(), Some(Pin::Flag));
    assert!(
        marked.macro_note.contains("--trust-macros"),
        "{}",
        marked.macro_note
    );
    let refused = pinned
        .execute_current(
            Request::SetMacroSecurity {
                level: MacroSecurity::Off,
            },
            None,
        )
        .reply;
    assert!(refused.is_err_and(|why| why.contains("pinned")));
    let mut by_variable = Server::new();
    by_variable.set_macro_policy(MacroPolicy::resolve(
        MacroSecurity::Off,
        false,
        Some("1"),
        None,
    ));
    assert_eq!(
        open(&mut by_variable, FileOrigin::Network).macros,
        MacroStatus::Enabled
    );
    // --macros pins a level over the stored one, either way.
    let mut off = Server::new();
    off.set_macro_policy(MacroPolicy::resolve(
        MacroSecurity::Unguarded,
        false,
        None,
        Some(MacroSecurity::Off),
    ));
    assert_eq!(
        open(&mut off, FileOrigin::Local).macros,
        MacroStatus::Disabled
    );
    let mut on = Server::new();
    on.set_macro_policy(MacroPolicy::resolve(
        MacroSecurity::Off,
        false,
        None,
        Some(MacroSecurity::Unguarded),
    ));
    assert_eq!(
        open(&mut on, FileOrigin::Local).macros,
        MacroStatus::Enabled
    );
}

#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_native_origin_is_not_inherited_by_the_next_page_open() {
    use dna_calc_worker::{server::Server, wire::Request};
    let mut server = Server::new();
    server.with_open_origin(FileOrigin::Local, |server| {
        server.execute_current(Request::Ping, None);
    });
    assert!(
        server
            .execute_current(
                Request::Open {
                    name: "page.xlsm".into()
                },
                Some(LOCAL.to_vec())
            )
            .reply
            .is_ok()
    );
    assert_eq!(
        server.book().macros(),
        &Macros::Blocked(calc_host_vba::MacroBlock::Unknown)
    );
}

/// A file chosen through the page's own file input (or dropped on it) reaches the server as bytes
/// with no path, so its origin is unknown: the page's Enable macros is refused (dc-2nn1).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_page_supplied_file_has_no_origin_and_enable_macros_is_refused() {
    use dna_calc_worker::{server::Server, wire::Request};
    let mut server = Server::new();
    assert!(
        server
            .execute_current(
                Request::Open {
                    name: "upload.xlsm".into()
                },
                Some(LOCAL.to_vec())
            )
            .reply
            .is_ok()
    );
    assert_eq!(
        server.book().macros(),
        &Macros::Blocked(calc_host_vba::MacroBlock::Unknown)
    );
    assert!(
        server
            .execute_current(Request::EnableMacros, None)
            .reply
            .is_err()
    );
    assert_eq!(
        server.book().macros(),
        &Macros::Blocked(calc_host_vba::MacroBlock::Unknown)
    );
}

/// Native-only: exercises native panic unwinding through the request boundary.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_native_origin_is_cleared_even_when_the_request_panics() {
    use dna_calc_worker::{server::Server, wire::Request};
    let mut server = Server::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        server.with_open_origin::<()>(FileOrigin::Local, |_| panic!("fixture request"));
    }));
    assert!(result.is_err());
    assert!(
        server
            .execute_current(
                Request::Open {
                    name: "page.xlsm".into()
                },
                Some(LOCAL.to_vec())
            )
            .reply
            .is_ok()
    );
    assert_eq!(
        server.book().macros(),
        &Macros::Blocked(calc_host_vba::MacroBlock::Unknown)
    );
}

/// Excel's answer for each formula of a recorded run: a controller record (`operation`, `value`
/// per line) or a reopen record (`rows` with `formula` and `excel`'s value or error code).
fn excel_answers(record: &str) -> std::collections::BTreeMap<String, serde_json::Value> {
    let record = record.trim_start_matches('\u{feff}');
    let mut answers = std::collections::BTreeMap::new();
    if let Ok(reopen) = serde_json::from_str::<serde_json::Value>(record)
        && let Some(rows) = reopen["rows"].as_array()
    {
        for row in rows {
            let excel = &row["excel"];
            let value = match excel["error_code"].as_i64() {
                Some(code) => serde_json::json!({ "error": match code {
                    -2146826288 => "#NULL!",
                    -2146826281 => "#DIV/0!",
                    -2146826273 => "#VALUE!",
                    -2146826265 => "#REF!",
                    -2146826259 => "#NAME?",
                    -2146826252 => "#NUM!",
                    -2146826246 => "#N/A",
                    _ => "#unknown",
                }}),
                None => excel["value"].clone(),
            };
            answers.insert(row["formula"].as_str().unwrap().to_owned(), value);
        }
        return answers;
    }
    for line in record.lines().filter(|line| !line.trim().is_empty()) {
        let line: serde_json::Value = serde_json::from_str(line).unwrap();
        answers.insert(
            line["operation"].as_str().unwrap().to_owned(),
            line["value"].clone(),
        );
    }
    answers
}

/// Whether the cell's value is Excel's answer: a number equal to it, the same text, logical or
/// error, a blank or 0 for Excel's empty; an array result by its top-left value, as the cell
/// shows it. `None` when the cell holds no calculated value (a refusal).
fn same_as_excel(value: &ProjectedValue, excel: &serde_json::Value) -> Option<bool> {
    use calc_value::CoreRef;
    let ProjectedValue::Calculated { value, .. } = value else {
        return None;
    };
    let mut value = value.clone();
    while let CoreRef::Array(array) = value.core_ref() {
        value = array.get(0, 0)?.clone();
    }
    Some(match (value.core_ref(), excel) {
        (CoreRef::Number(n), serde_json::Value::Number(e)) => Some(n) == e.as_f64(),
        (CoreRef::Text(t), serde_json::Value::String(e)) => t.to_string_lossy() == *e,
        (CoreRef::Logical(b), serde_json::Value::Bool(e)) => b == *e,
        (CoreRef::Error(code), serde_json::Value::Object(e)) => {
            e.get("error").and_then(|e| e.as_str()) == Some(code.excel_spelling())
        }
        (CoreRef::Empty, serde_json::Value::Null) => true,
        (CoreRef::Number(n), serde_json::Value::Null) => n == 0.0,
        _ => false,
    })
}

/// The simple-UDF sets (crates/calc-xll-reference/traces/2026-11-11-simple-udfs) on DNA Calc's
/// file route, the one the page's worker takes: the .xlsm opened, its macros enabled, each case
/// formula entered in Sheet1!H1 in turn and recalculated, compared with Excel's answer for the
/// same formula in the same file. Pass = matched Excel; refused and mismatched are failures.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn the_simple_udf_sets_answer_as_excel_does_in_the_macro_workbooks() {
    const SIMPLE: &str =
        include_str!("../../../crates/calc-xll-reference/fixture/vba/simple-udf-cases.json");
    const X9: &str =
        include_str!("../../../crates/calc-xll-reference/fixture/vba/x9-vba-cases.json");
    let sets: [(&str, &[u8], &str, &str); 3] = [
        (
            "DnaSimple.xlsm",
            include_bytes!("../../../crates/calc-xll-reference/fixture/vba/DnaSimple.xlsm"),
            SIMPLE,
            include_str!(
                "../../../crates/calc-xll-reference/traces/2026-11-11-simple-udfs/excel/simple-inbook-dnasimple-xlsm.controller.jsonl"
            ),
        ),
        (
            "DnaExamples.xlsm",
            include_bytes!("../../../crates/calc-xll-reference/fixture/vba/DnaExamples.xlsm"),
            SIMPLE,
            include_str!(
                "../../../crates/calc-xll-reference/traces/2026-11-11-simple-udfs/onecalc/excel-reopen-simple-xlsm.json"
            ),
        ),
        (
            "DnaExamples.xlsm",
            include_bytes!("../../../crates/calc-xll-reference/fixture/vba/DnaExamples.xlsm"),
            X9,
            include_str!(
                "../../../crates/calc-xll-reference/traces/2026-11-11-simple-udfs/onecalc/excel-reopen-dnaexamples-xlsm.json"
            ),
        ),
    ];
    let mut failures = Vec::new();
    for (name, bytes, cases, record) in sets {
        let answers = excel_answers(record);
        let cases: Vec<serde_json::Value> = serde_json::from_str(cases).unwrap();
        // A file opened from a local path, as the desktop app opens one it is given.
        let mut book = Workbook::open_with_project_policy(
            name,
            bytes.to_vec(),
            WorkBudget::UNLIMITED,
            FileOrigin::Local,
        )
        .unwrap();
        if let Err(refused) = book.enable_macros() {
            failures.push(format!("{name}: macros not enabled: {refused}"));
            continue;
        }
        drain(&mut book);
        let cell = calc_value::CellKey::new(book.host().sheets()[0].id(), 1, 8).unwrap();
        let (mut matched, mut mismatched, mut refused, mut missing) = (0, 0, 0, 0);
        for case in &cases {
            let formula = case["formula"].as_str().unwrap();
            let Some(excel) = answers.get(formula) else {
                missing += 1;
                failures.push(format!("{name} {formula}: no Excel answer"));
                continue;
            };
            book.dispatch(SkinIntent::EnterCell {
                at: cell,
                text: formula.to_owned(),
            });
            drain(&mut book);
            let value = book.project(vec![cell])[0].value().clone();
            match same_as_excel(&value, excel) {
                Some(true) => matched += 1,
                Some(false) => {
                    mismatched += 1;
                    failures.push(format!(
                        "{name} {formula}: Excel {excel}, DNA Calc {value:?}"
                    ));
                }
                None => {
                    refused += 1;
                    failures.push(format!("{name} {formula}: refused {value:?}"));
                }
            }
        }
        println!(
            "{name} ({} cases): matched {matched}, mismatched {mismatched}, refused {refused}, missing {missing}",
            cases.len()
        );
    }
    // dc-hsa5.22: TypeName keeps Range, class and Selection objects intact and
    // the engine Dictionary reports its public type name. Every case must match.
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A worker on its own (a browser page) lists no add-ins, says it loads none and refuses a load or
/// an unload with the reason; the desktop app answers these requests itself (dc-0f1k.23). When the
/// desktop publishes a changed add-in set, a cell that named a function before it was loaded
/// answers without a manual recalculation.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn add_ins_are_the_desktop_apps_and_a_changed_set_recalculates() {
    use dna_calc_worker::server::{ADDINS_DESKTOP_ONLY, Server};
    use dna_calc_worker::wire::{Intent, Key, Reply, Request};
    let mut server = Server::new();
    match server.handle(Request::Addins, None).reply {
        Ok(Reply::Addins(list)) => assert!(!list.native && list.loaded.is_empty()),
        other => panic!("not an add-in list: {other:?}"), // ubs:ignore[rust.ownership.panic-macro] The request must answer.
    }
    for request in [
        Request::LoadAddinNative,
        Request::LoadAddin {
            path: "C:/addins/x.xll".into(),
        },
        Request::UnloadAddin {
            path: "C:/addins/x.xll".into(),
        },
    ] {
        assert_eq!(
            server.handle(request, None).reply,
            Err(ADDINS_DESKTOP_ONLY.to_owned())
        );
    }
    let settle = |server: &mut Server| {
        for _ in 0..1000 {
            if !server.has_pending_calculation() {
                return;
            }
            server.advance_calculation();
        }
        panic!("the worker did not settle"); // ubs:ignore[rust.ownership.panic-macro] A bounded drain must finish.
    };
    struct CalculationEvents(std::cell::Cell<u32>);
    impl calc_host::WorkbookEventObserver for CalculationEvents {
        fn after_calculate(&self, _: &mut calc_host::HostSession) -> Result<(), String> {
            self.0.set(self.0.get() + 1);
            Ok(())
        }
    }
    let events = std::rc::Rc::new(CalculationEvents(std::cell::Cell::new(0)));
    server.set_workbook_event_observer(Some(events.clone()));
    let addin = calc_vba::load_module(
        "Extra",
        "Public Function AddinSeven() As Double\n AddinSeven = 7\nEnd Function\n",
        1000,
    )
    .unwrap();
    // With no formulas, publishing each new table must not generate another event after the
    // native add-in has settled.
    for functions in [addin.clone(), calc_host::CallableTable::new()] {
        server.set_user_functions(functions);
        settle(&mut server);
    }
    assert_eq!(events.0.get(), 0, "empty workbook add-in changes");

    let at = calc_value::CellKey::new(server.book().host().sheets()[0].id(), 1, 1).unwrap();
    assert!(
        server
            .execute_current(
                Request::Intent(Intent::EnterCell {
                    at: Key::of(at),
                    text: "=AddinSeven()".into(),
                }),
                None,
            )
            .reply
            .is_ok()
    );
    settle(&mut server);
    let seven = |server: &Server| {
        matches!(
            server.book().project(vec![at])[0].value(),
            ProjectedValue::Calculated { value, .. } if value.as_number() == Some(7.)
        )
    };
    assert!(!seven(&server), "no add-in is loaded yet");
    events.0.set(0);
    server.set_user_functions(addin);
    settle(&mut server);
    assert!(seven(&server), "{:?}", server.book().project(vec![at])[0].value());
    assert_eq!(events.0.get(), 1, "recalculation after resolving a call");
    server.set_user_functions(calc_host::CallableTable::new());
    settle(&mut server);
    let projected = server.book().project(vec![at]);
    assert!(
        matches!(projected[0].value(), ProjectedValue::Calculated { value, .. }
            if value.as_error() == Some(calc_value::WorksheetErrorCode::Name)),
        "the unloaded function should return #NAME?: {:?}",
        projected[0].value()
    );
}

/// dc-1nbp.7: P22-style two-sheet macro workbook (`Sheet1` + `S`) driven through the DNA Calc
/// page worker (`Server`: Open, EnableMacros, SetCalcMode, EnterCell, Recalculate) in both
/// Automatic and Manual modes. `Worksheet_Change` writes `Me.Range("C1")`,
/// `Worksheets("S").Range("D1")`, `ThisWorkbook.Worksheets("S").Range("E1")`, and
/// `Range.Cells(2, 2).Value2`, producing exact cell values and handler order.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_1nbp_7_page_worker_replays_me_range_and_cross_sheet_change_in_automatic_and_manual_modes() {
    use calc_host::{SheetEdit, SheetKey, SheetName};
    use dna_calc_worker::{
        server::Server,
        wire::{Intent, Key, Reply, Request},
    };

    // Build a two-sheet .xlsm package (Sheet1 + S) carrying the P22-style event project.
    let vba_bytes = vba_project::replace_sources(
        LOCAL,
        &[
            (
                "Module1",
                "Public History As String\nPublic Function Trace() As String\n Trace = History\nEnd Function\n",
            ),
            ("PrivateModule", "Option Private Module\n"),
            (
                "Sheet1",
                "Private Sub Worksheet_Change(ByVal Target As Excel.Range)\n \
                 History = History & \"Chg:\" & Target.Address(False, False) & \";\"\n \
                 If Target.Column <> 1 Or Target.Row <> 1 Then Exit Sub\n \
                 Dim v As Double\n \
                 v = CDbl(Target.Value2)\n \
                 Me.Range(\"C1\").Value2 = v * 10\n \
                 Worksheets(\"S\").Range(\"D1\").Value2 = CDbl(Me.Range(\"C1\").Value2) + 1\n \
                 ThisWorkbook.Worksheets(\"S\").Range(\"E1\").Value2 = CDbl(Worksheets(\"S\").Range(\"D1\").Value2) + 2\n \
                 Me.Range(\"A1:B2\").Cells(2, 2).Value2 = v + 5\n \
                 Worksheets(\"S\").Range(\"A1:B2\").Cells(2, 2).Value2 = CDbl(Me.Range(\"A1:B2\").Cells(2, 2).Value2) + 100\n \
                 Worksheets(\"S\").Cells(3, 1) = 77\n\
                 End Sub\n\
                 Private Sub Worksheet_Calculate()\n \
                 History = History & \"Calc;\"\n\
                 End Sub\n",
            ),
            ("ThisWorkbook", ""),
        ],
    );
    let two_sheet_xlsm = {
        let mut prep = Workbook::open_with_project_policy(
            "p22-page.xlsm",
            vba_bytes,
            WorkBudget::UNLIMITED,
            FileOrigin::Local,
        )
        .unwrap();
        assert!(
            prep.dispatch(SkinIntent::Edit {
                edits: vec![calc_host::CellEdit::Sheet(SheetEdit::Insert {
                    key: SheetKey::new(2),
                    name: SheetName::new(calc_value::ExcelText::from_interop_assignment("S"))
                        .unwrap(),
                    position: std::num::NonZeroU32::new(2).unwrap(),
                })],
            })
            .accepted()
        );
        let sheet1 = prep.host().sheets()[0].id();
        let sheet_s = prep.host().sheets()[1].id();
        assert!(
            prep.dispatch(SkinIntent::Edit {
                edits: vec![calc_host::CellEdit::Clear {
                    area: calc_value::GridArea::new(
                        calc_value::CellKey::new(sheet1, 1, 1).unwrap(),
                        calc_value::CellKey::new(sheet1, 5, 5).unwrap(),
                    )
                    .unwrap(),
                }],
            })
            .accepted()
        );
        assert!(
            prep.dispatch(SkinIntent::EnterCell {
                at: calc_value::CellKey::new(sheet1, 1, 6).unwrap(),
                text: "=C1+S!D1+S!E1+B2+S!B2".into(),
            })
            .accepted()
        );
        assert!(
            prep.dispatch(SkinIntent::EnterCell {
                at: calc_value::CellKey::new(sheet_s, 1, 6).unwrap(),
                text: "=D1+E1+B2".into(),
            })
            .accepted()
        );
        drain(&mut prep);
        prep.save().unwrap().0
    };

    let settle = |server: &mut Server| {
        for _ in 0..1000 {
            if !server.has_pending_calculation() {
                return;
            }
            server.advance_calculation();
        }
        panic!("worker did not settle"); // ubs:ignore[rust.ownership.panic-macro] A bounded drain must finish.
    };
    let read_num = |server: &Server, sheet: calc_value::SheetId, r: u32, c: u32| {
        let at = calc_value::CellKey::new(sheet, r, c).unwrap();
        match server.book().project(vec![at])[0].value() {
            ProjectedValue::Literal(v) | ProjectedValue::Calculated { value: v, .. } => {
                v.as_number().unwrap()
            }
            other => panic!("unexpected value at ({r},{c}): {other:?}"), // ubs:ignore[rust.ownership.panic-macro] Expected number cell.
        }
    };
    let read_trace = |server: &mut Server, sheet_s: calc_value::SheetId, col: u32| {
        let at = calc_value::CellKey::new(sheet_s, 1, col).unwrap();
        assert!(
            server
                .execute_current(
                    Request::Intent(Intent::EnterCell {
                        at: Key::of(at),
                        text: "=Trace()".into(),
                    }),
                    None,
                )
                .reply
                .is_ok()
        );
        // Calculate just S!col so Sheet1's Worksheet_Calculate is unaffected in Manual mode.
        server.book_mut().dispatch(SkinIntent::CalculateRange {
            areas: vec![calc_value::GridArea::cell(at)],
        });
        settle(server);
        match server.book().project(vec![at])[0].value() {
            ProjectedValue::Calculated { value, .. } => value.as_text().unwrap().to_string_lossy(),
            other => panic!("unexpected Trace() value: {other:?}"), // ubs:ignore[rust.ownership.panic-macro] Expected Trace text.
        }
    };

    for manual in [false, true] {
        let mut server = Server::new();
        server.set_open_origin(FileOrigin::Local);
        assert!(
            matches!(
                server
                    .execute_current(
                        Request::Open {
                            name: "p22-page.xlsm".into(),
                        },
                        Some(two_sheet_xlsm.clone()),
                    )
                    .reply,
                Ok(Reply::Done { .. })
            ),
            "manual={manual}: open failed"
        );
        settle(&mut server);
        assert!(
            server
                .execute_current(Request::EnableMacros, None)
                .reply
                .is_ok(),
            "manual={manual}: enable macros failed"
        );
        settle(&mut server);
        assert!(
            server
                .execute_current(Request::Intent(Intent::SetCalcMode { manual }), None)
                .reply
                .is_ok()
        );
        settle(&mut server);

        let sheet1 = server.book().host().sheets()[0].id();
        let sheet_s = server.book().host().sheets()[1].id();
        let a1 = calc_value::CellKey::new(sheet1, 1, 1).unwrap();
        assert!(
            server
                .execute_current(
                    Request::Intent(Intent::EnterCell {
                        at: Key::of(a1),
                        text: "3".into(),
                    }),
                    None,
                )
                .reply
                .is_ok()
        );
        settle(&mut server);
        assert!(
            server.book_mut().take_workbook_event_failures().failures.is_empty(),
            "manual={manual}: event failures"
        );

        assert_eq!(read_num(&server, sheet1, 1, 3), 30.0);
        assert_eq!(read_num(&server, sheet_s, 1, 4), 31.0);
        assert_eq!(read_num(&server, sheet_s, 1, 5), 33.0);
        assert_eq!(read_num(&server, sheet1, 2, 2), 8.0);
        assert_eq!(read_num(&server, sheet_s, 2, 2), 108.0);
        assert_eq!(read_num(&server, sheet_s, 3, 1), 77.0);

        // EnableMacros recalculates the workbook once before the person's edit (leading "Calc;").
        if manual {
            let assert_stale_zero = |sheet: calc_value::SheetId, r: u32, c: u32| {
                let at = calc_value::CellKey::new(sheet, r, c).unwrap();
                let got = server.book().project(vec![at])[0].value().clone();
                assert!(
                    matches!(
                        &got,
                        ProjectedValue::Stale(stale)
                            if stale.last_value().and_then(|v| v.as_number()) == Some(0.0)
                    ),
                    "expected Stale(0.0) at ({r},{c}), got {got:?}"
                );
            };
            assert_stale_zero(sheet1, 1, 6);
            assert_stale_zero(sheet_s, 1, 6);
            assert_eq!(
                read_trace(&mut server, sheet_s, 8),
                "Calc;Chg:A1;Chg:C1;Chg:B2;"
            );
            assert!(
                server
                    .execute_current(Request::Intent(Intent::Recalculate), None)
                    .reply
                    .is_ok()
            );
            settle(&mut server);
            assert_eq!(
                read_trace(&mut server, sheet_s, 9),
                "Calc;Chg:A1;Chg:C1;Chg:B2;Calc;"
            );
        } else {
            assert_eq!(
                read_trace(&mut server, sheet_s, 8),
                "Calc;Chg:A1;Calc;Chg:C1;Calc;Calc;Calc;Chg:B2;Calc;"
            );
        }
        assert_eq!(read_num(&server, sheet1, 1, 6), 210.0);
        assert_eq!(read_num(&server, sheet_s, 1, 6), 172.0);
    }
}

/// A macro's Chart.Export reaches the page: the server queues the file, announces it with
/// `Event::MacroFiles` and hands it over with `Request::TakeMacroFile`, once (dc-oxh8w.54).
/// The desktop shell takes the same files through `Workbook::take_macro_output_files`.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_macro_chart_export_reaches_the_page_as_a_file() {
    use dna_calc_worker::server::Server;
    use dna_calc_worker::wire::{Event, Reply, Request};
    let export = "Private Sub Workbook_Open()\n Dim frame As Object\n Application.Range(\"A1\").Value2 = \"Region\"\n \
        Application.Range(\"B1\").Value2 = \"Q1\"\n Application.Range(\"A2\").Value2 = \"North\"\n \
        Application.Range(\"B2\").Value2 = 10\n \
        Set frame = Application.ActiveSheet.ChartObjects.Add(10, 10, 300, 200)\n \
        frame.Chart.SetSourceData Application.Range(\"A1:B2\")\n frame.Chart.Export \"out.png\", \"PNG\"\nEnd Sub\n";
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            ("Module1", ""),
            ("PrivateModule", "Option Private Module\n"),
            ("Sheet1", ""),
            ("ThisWorkbook", export),
        ],
    );
    let mut server = Server::new();
    server.set_open_origin(FileOrigin::Local);
    let opened = server.execute_current(Request::Open { name: "export.xlsm".into() }, Some(bytes));
    assert!(opened.reply.is_ok(), "{:?}", opened.reply);
    assert!(server.take_events().iter().all(|e| !matches!(e, Event::MacroFiles { .. })), "disabled macros wrote nothing");
    let enabled = server.execute_current(Request::EnableMacros, None);
    assert!(enabled.reply.is_ok(), "{:?}", enabled.reply);
    let mut announced = server.take_events();
    announced.extend(server.take_events());
    let count: u64 = announced
        .iter()
        .filter_map(|e| if let Event::MacroFiles { count, .. } = e { Some(*count) } else { None })
        .sum();
    assert_eq!(count, 1, "{announced:?}");
    let taken = server.execute_current(Request::TakeMacroFile, None);
    assert_eq!(taken.reply, Ok(Reply::MacroFile { name: "out.png".into(), remaining: 0 }));
    // Whatever exporter the process holds draws it (another test may install a stub): the bytes arrive.
    assert!(taken.bytes.as_deref().is_some_and(|file| !file.is_empty()), "{:?}", taken.bytes);
    let again = server.execute_current(Request::TakeMacroFile, None);
    assert!(again.reply.is_err(), "taking the file empties the queue");
}

/// dc-oxh8w.75: the browser worker installs the workbook factory on macro enable so `Workbooks.Add`
/// works and `Workbooks.Open` reads from the page-supplied file loader.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_oxh8w_75__worker_installs_application_factory_and_page_loader() {
    use dna_calc_worker::workbook::{EnableMode, Workbook};
    let macro_open = "Private Sub Workbook_Open()\n \
        Dim added As Workbook\n Set added = Workbooks.Add\n \
        ThisWorkbook.Worksheets(1).Range(\"A1\").Value2 = added.Name\n \
        added.Close False\n \
        Dim other As Workbook\n Set other = Workbooks.Open(\"store.xlsm\")\n \
        ThisWorkbook.Worksheets(1).Range(\"A2\").Value2 = other.Name\n \
        other.Close False\nEnd Sub\n";
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            ("Module1", ""),
            ("PrivateModule", "Option Private Module\n"),
            ("Sheet1", ""),
            ("ThisWorkbook", macro_open),
        ],
    );
    let mut book = Workbook::open_with_project_policy(
        "worker.xlsm",
        bytes,
        calc_host::WorkBudget::UNLIMITED,
        FileOrigin::Local,
    )
    .unwrap();
    book.set_macro_file_loader(Some(std::rc::Rc::new(|path: &str| {
        if path == "store.xlsm" {
            Ok(LOCAL.to_vec())
        } else {
            Err("missing".into())
        }
    })));
    book.enable_macros_as(EnableMode::Person).unwrap();
    let sheet = book.host().model().sheets()[0].id();
    let a1 = book.host().model().cell(calc_value::CellKey::new(sheet, 1, 1).unwrap());
    let a2 = book.host().model().cell(calc_value::CellKey::new(sheet, 2, 1).unwrap());
    assert!(
        matches!(a1, Some(calc_host::CellInput::Text(t)) if t.text().to_string_lossy() == "Book1"),
        "A1: {a1:?}"
    );
    assert!(
        matches!(a2, Some(calc_host::CellInput::Text(t)) if t.text().to_string_lossy() == "store.xlsm"),
        "A2: {a2:?}"
    );
}

/// dc-qrnem (rw12 Finding 15): cooperative ordinary save in the worker raises Workbook_AfterSave(True).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn dc_qrnem_cooperative_save_raises_after_save() {
    use dna_calc_worker::workbook::{EnableMode, Workbook};
    let book_src = "Private Sub Workbook_AfterSave(ByVal Success As Boolean)\n \
        ThisWorkbook.Worksheets(1).Range(\"D1\").Value2 = \"after:\" & CStr(Success)\nEnd Sub\n";
    let bytes = vba_project::replace_sources(
        LOCAL,
        &[
            ("Module1", ""),
            ("PrivateModule", "Option Private Module\n"),
            ("Sheet1", ""),
            ("ThisWorkbook", book_src),
        ],
    );
    let mut book = Workbook::open_with_project_policy(
        "coop.xlsm",
        bytes,
        calc_host::WorkBudget::UNLIMITED,
        FileOrigin::Local,
    )
    .unwrap();
    book.enable_macros_as(EnableMode::Person).unwrap();
    let policy = calc_host::SavePolicy::default();
    let mut save = book.begin_ordinary_save(false, &policy).unwrap();
    while save.needs_advance(book.host()) {
        book.advance_ordinary_save(&mut save, calc_host::WorkBudget::UNLIMITED);
    }
    let (saved, _) = book.finish_ordinary_save(save).unwrap();
    assert!(!saved.is_empty());
    let sheet = book.host().model().sheets()[0].id();
    let d1 = book.host().model().cell(calc_value::CellKey::new(sheet, 1, 4).unwrap());
    assert!(
        matches!(d1, Some(calc_host::CellInput::Text(t)) if t.text().to_string_lossy() == "after:True"),
        "D1: {d1:?}"
    );
}
