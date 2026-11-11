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
        assert_eq!(binary.macros(), &Macros::Disabled(FileOrigin::Unknown));
        binary.enable_macros().unwrap();
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
        for origin in [FileOrigin::Unknown, FileOrigin::Local, FileOrigin::Zone(2)] {
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
    for origin in [FileOrigin::Zone(3), FileOrigin::Zone(4), FileOrigin::Network] {
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

/// The page's route: an Open names no origin (a browser cannot read one) and the workbook opens
/// with macros disabled; EnableMacros runs the project. A native host's origin for the next open
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
    // The origin was for that open only: the next one is unknown again, so disabled.
    let next = server.execute_current(
        Request::Open {
            name: "local.xlsm".into(),
        },
        Some(LOCAL.to_vec()),
    );
    assert_eq!(status(&next.reply), MacroStatus::Disabled);
    let plain = server.execute_current(Request::New, None);
    assert_eq!(status(&plain.reply), MacroStatus::None);
    assert!(server.execute_current(Request::EnableMacros, None).reply.is_err());
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

    let unsupported = include_bytes!(
        "../../../crates/calc-engine/tests/fixtures/excel-calc-probes/P22/eng9-20261103/cases/automatic-basic/artifact.xlsm"
    );
    // The file opens with its project held; enabling it is what refuses the project.
    let mut p22 = Workbook::open("p22.xlsm", unsupported.to_vec()).unwrap();
    let refusal = match p22.enable_macros() {
        Err(EnableRefused::Project(calc_host_vba::FileProjectError::Preparation(detail))) => {
            detail
        }
        Err(other) => panic!("expected the remaining automatic-event refusal: {other:?}"), // ubs:ignore[rust.ownership.panic-macro] A different refusal must fail this regression.
        Ok(()) => panic!("unsupported automatic events cannot run on file open"), // ubs:ignore[rust.ownership.panic-macro] Unexpected admission must fail this regression.
    };
    assert_eq!(p22.macros(), &Macros::Disabled(FileOrigin::Unknown));
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
        Workbook::open("designer.xlsm", designer).unwrap().enable_macros(),
        Err(EnableRefused::Project(
            calc_host_vba::FileProjectError::UnsupportedModule(name)
        )) if name == "P22Events"
    ));
}

/// A host that publishes its add-ins' functions to an opened macro workbook (the desktop app with
/// an .xll or .xlam) keeps the workbook's own VBA functions beside them (dc-0f1k.4).
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn add_in_functions_join_the_file_projects_functions() {
    // The host publishes its add-ins before or after the person enables macros (the desktop app
    // installs them at open, before the bar's button).
    for addins_first in [false, true] {
        let mut book = Workbook::open("local.xlsm", LOCAL.to_vec()).unwrap();
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
    book.enable_macros().unwrap();
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

/// The macro security level (dc-15dk), end to end through the server's wire: a marked file offers
/// Unblock and enable under Ask; Unguarded runs every file on open (a marked one with a notice);
/// Safe (not available yet) holds a project like Off and says so; a pinned level cannot be changed.
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
    assert!(server.execute_current(Request::EnableMacros, None).reply.is_err());
    let unblocked = summary(&server.execute_current(Request::UnblockMacros, None).reply);
    assert_eq!(unblocked.macros, MacroStatus::Enabled);
    assert!(unblocked.macro_note.contains("you unblocked it"), "{}", unblocked.macro_note);
    // A network file unblocks the same way, and an unmarked file's Unblock is no more than Enable.
    open(&mut server, FileOrigin::Network);
    let network = summary(&server.execute_current(Request::UnblockMacros, None).reply);
    assert_eq!(network.macros, MacroStatus::Enabled);
    assert!(network.macro_note.contains("network"));
    open(&mut server, FileOrigin::Local);
    let local = summary(&server.execute_current(Request::UnblockMacros, None).reply);
    assert_eq!((local.macros, local.macro_note.as_str()), (MacroStatus::Enabled, ""));

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
    assert_eq!((plain.macros, plain.macro_note.as_str()), (MacroStatus::Enabled, ""));
    let marked = open(&mut server, FileOrigin::Zone(4));
    assert_eq!(marked.macros, MacroStatus::Enabled);
    assert!(
        marked.macro_note.contains("Mark of the Web")
            && marked.macro_note.contains("macro security level"),
        "{}",
        marked.macro_note
    );
    assert_eq!(open(&mut server, FileOrigin::Network).macros, MacroStatus::Enabled);

    // Safe is not available yet (dc-10un): it behaves as Off and the policy says so; the person
    // can still enable the file.
    server
        .execute_current(
            Request::SetMacroSecurity {
                level: MacroSecurity::Safe,
            },
            None,
        )
        .reply
        .unwrap();
    let held = open(&mut server, FileOrigin::Local);
    assert_eq!(held.macros, MacroStatus::Disabled);
    assert_eq!(held.macro_policy.level(), MacroSecurity::Safe);
    assert!(held.macro_policy.safe_notice().is_some());
    assert_eq!(
        summary(&server.execute_current(Request::EnableMacros, None).reply).macros,
        MacroStatus::Enabled
    );
    assert!(matches!(
        open(&mut server, FileOrigin::Zone(3)).macros,
        MacroStatus::Blocked { .. }
    ));

    // The flag, the variable or --macros pins a level: the page cannot choose another.
    let mut pinned = Server::new();
    pinned.set_macro_policy(MacroPolicy::resolve(MacroSecurity::Safe, true, None, None));
    let marked = open(&mut pinned, FileOrigin::Zone(3));
    assert_eq!(marked.macros, MacroStatus::Enabled);
    assert_eq!(marked.macro_policy.pinned(), Some(Pin::Flag));
    assert!(marked.macro_note.contains("--trust-macros"), "{}", marked.macro_note);
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
    by_variable.set_macro_policy(MacroPolicy::resolve(MacroSecurity::Off, false, Some("1"), None));
    assert_eq!(open(&mut by_variable, FileOrigin::Network).macros, MacroStatus::Enabled);
    // --macros pins a level over the stored one, either way.
    let mut off = Server::new();
    off.set_macro_policy(MacroPolicy::resolve(
        MacroSecurity::Unguarded,
        false,
        None,
        Some(MacroSecurity::Off),
    ));
    assert_eq!(open(&mut off, FileOrigin::Local).macros, MacroStatus::Disabled);
    let mut on = Server::new();
    on.set_macro_policy(MacroPolicy::resolve(
        MacroSecurity::Off,
        false,
        None,
        Some(MacroSecurity::Unguarded),
    ));
    assert_eq!(open(&mut on, FileOrigin::Local).macros, MacroStatus::Enabled);
}
