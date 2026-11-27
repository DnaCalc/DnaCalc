//! The page core against the real workbook service: Classic's commands, the shell's commands,
//! the keymap, the session and `calc-skin-host`, joined by packets as a carrier joins them.
#![cfg(not(target_arch = "wasm32"))]

use calc_host::{HostOptions, HostSession, MemorySource, OpenPolicy, SkinIntent, blank_package};
use calc_skin::grid::{Key, Sheet};
use calc_skin::{Interest, Origin, Packet, SelectionCause, ViewId, WritePolicy, query};
use calc_skin_host::{Connection, SkinHost};
use dna_kit::commands::CommandError;
use dna_kit::{Focus, InputMode, KeyInput, Live, Registry, Seam, Session};
use leptos::prelude::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

struct Page {
    host: SkinHost,
    port: Connection,
    sent: Rc<RefCell<VecDeque<Packet>>>,
    session: Session,
    sheet: Sheet,
    _owner: Owner,
}

impl Page {
    fn open() -> Self {
        let owner = Owner::new();
        owner.set();
        let workbook = HostSession::open(
            Arc::new(MemorySource::new(blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let sheet = Sheet::of(workbook.host().sheets()[0].id());
        let host = SkinHost::new(workbook);
        let port = host.connect(
            Origin::User {
                view: ViewId(dna_view_classic::ID.into()),
            },
            WritePolicy::Commit,
        );
        let sent = Rc::new(RefCell::new(VecDeque::new()));
        let session = Session::new(ViewId(dna_view_classic::ID.into()), {
            let sent = sent.clone();
            move |packet| sent.borrow_mut().push_back(packet)
        });
        let mut registry = Registry::default();
        dna_view_classic::install(&mut registry);
        dna_view_inspector::install(&mut registry);
        dna_view_notebook::install(&mut registry);
        dna_view_app::install(&mut registry);
        dna_shell::workbook::install(&registry, session);
        let page = Self {
            host,
            port,
            sent,
            session,
            sheet,
            _owner: owner,
        };
        page.settle();
        page
    }

    /// Carries packets both ways until neither side has anything to say.
    fn settle(&self) {
        loop {
            let mut moved = false;
            loop {
                let Some(packet) = self.sent.borrow_mut().pop_front() else {
                    break;
                };
                self.session.deliver(self.port.handle(packet));
                moved = true;
            }
            while self.host.advance() {}
            for packet in self.port.next_packets().unwrap() {
                self.session.deliver(packet);
                moved = true;
            }
            if !moved {
                return;
            }
        }
    }

    fn at(&self, row: u32, column: u32) -> Key {
        Key(self.sheet.0, self.sheet.1, row, column)
    }

    fn press(&self, key: &str, focus: Focus) -> bool {
        let mut parts = key.split('+');
        let key = parts.next_back().unwrap();
        let modifiers: Vec<_> = parts.collect();
        let taken = self.session.key(
            &KeyInput {
                key: key.to_owned(),
                ctrl: modifiers.contains(&"Ctrl"),
                alt: modifiers.contains(&"Alt"),
                shift: modifiers.contains(&"Shift"),
                meta: false,
                alt_graph: false,
                composing: false,
            },
            focus,
        );
        self.settle();
        taken
    }

    /// What a view that watches the page of `at` is told about the cell.
    fn shown(&self, at: Key) -> String {
        let cells = self.session.following::<query::Cells>();
        let query = query::Cells {
            interest: Interest::Area(dna_grid::page_of(at)),
        };
        cells.at(Some(query.clone()));
        self.settle();
        let Live::Ready(window) = cells.at(Some(query)) else {
            panic!("the cells of the page did not arrive");
        };
        cells.at(None);
        self.settle();
        window
            .body
            .cells
            .into_iter()
            .find(|cell| cell.at == at)
            .map(|cell| cell.text)
            .unwrap_or_default()
    }
}

/// Native-only: links calc-host and calc-engine, which the page packages do not build for the browser tests.
#[test]
fn a_person_moves_types_commits_and_undoes_through_commands() {
    let page = Page::open();
    let session = page.session;
    assert_eq!(session.active(), Some(page.at(1, 1)));
    assert!(session.summary().is_some());

    // Grid motion is Classic's command, reached through the keymap.
    assert!(page.press("ArrowDown", Focus::Stage));
    assert!(page.press("ArrowRight", Focus::Stage));
    assert_eq!(session.active(), Some(page.at(2, 2)));

    // A printable key on the stage starts the one draft; Enter in the editor sends it through
    // the seam.
    assert!(page.press("=", Focus::Stage));
    assert_eq!(session.mode(), InputMode::Enter);
    assert!(
        !page.press("2", Focus::Editor),
        "the editor types its own text"
    );
    session.edit_text("=21*2".into());
    assert_eq!(session.entry(), "=21*2");
    assert!(
        page.press("ArrowDown", Focus::Editor) == false,
        "motion waits while a draft is open"
    );
    assert!(page.press("Enter", Focus::Editor));
    assert_eq!(session.mode(), InputMode::Ready);
    assert_eq!(session.seam(), Seam::Idle);
    assert_eq!(session.notice(), "");
    assert_eq!(page.shown(page.at(2, 2)), "42");
    assert_eq!(session.active(), Some(page.at(3, 2)));
    assert!(page.press("ArrowUp", Focus::Stage));
    assert_eq!(session.entry(), "=21*2");

    // F2 opens the entry the host reports.
    assert!(page.press("F2", Focus::Stage));
    let draft = session.edit().expect("F2 opens a draft");
    assert_eq!(
        (draft.text.as_str(), draft.mode),
        ("=21*2", InputMode::Edit)
    );
    assert!(page.press("Escape", Focus::Editor));
    assert_eq!(session.edit(), None);

    // The one history.
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert_eq!(page.shown(page.at(2, 2)), "");
    assert!(
        session.notice().starts_with("Undid · "),
        "{}",
        session.notice()
    );
    assert_eq!(session.entry(), "");
    assert!(page.press("Ctrl+y", Focus::Stage));
    assert_eq!(page.shown(page.at(2, 2)), "42");
}

/// Native-only: links calc-host and calc-engine, which the page packages do not build for the browser tests.
#[test]
fn keys_in_another_field_stay_that_fields() {
    let page = Page::open();
    let session = page.session;
    // Enter in the Name Box is the Name Box's, typing is typing, and Alt+Q still opens search.
    assert!(!page.press("Enter", Focus::Field));
    assert!(!page.press("x", Focus::Field));
    assert_eq!(
        (session.active(), session.edit()),
        (Some(page.at(1, 1)), None)
    );
    assert!(page.press("Alt+q", Focus::Field));
}

/// Native-only: tests selection and editor focus through the real workbook page commands.
#[test]
fn the_formula_editor_keeps_native_clipboard_chords_before_a_draft_starts() {
    let page = Page::open();
    let session = page.session;
    assert_eq!(session.mode(), InputMode::Ready);
    assert!(!page.press("Ctrl+c", Focus::Editor));
    assert!(!page.press("Ctrl+Shift+v", Focus::Editor));
    assert!(session.notice().is_empty());
    assert!(page.press("Ctrl+c", Focus::Stage));
    assert!(session.notice().contains("Copied worksheet cells"));
}

/// Native-only: drives the real workbook host through packets.
#[test]
fn classic_key_tips_keep_the_excel_path_and_return_to_the_grid() {
    let page = Page::open();
    let session = page.session;
    assert!(page.press("F10", Focus::Stage));
    assert_eq!(session.mode(), InputMode::KeyTips);
    assert!(!page.press("Tab", Focus::Stage));
    assert_eq!(session.active(), Some(page.at(1, 1)));
    for key in ["h", "v", "Escape", "v", "t"] {
        assert!(page.press(key, Focus::Stage), "{key}");
    }
    assert_eq!(session.mode(), InputMode::Ready);
    assert!(session.notice().contains("dc-s2xup"), "{}", session.notice());
    assert_eq!(session.edit(), None);

    assert!(page.press("F10", Focus::Stage));
    for key in ["e", "s", "v", "Enter"] {
        assert!(page.press(key, Focus::Stage), "{key}");
    }
    assert_eq!(session.mode(), InputMode::Ready);
    assert!(session.notice().contains("Copy worksheet cells"), "{}", session.notice());

    let alt = KeyInput {
        key: "Alt".into(),
        ctrl: false,
        alt: true,
        shift: false,
        meta: false,
        alt_graph: false,
        composing: false,
    };
    assert!(session.key(&alt, Focus::Stage));
    assert_eq!(session.mode(), InputMode::Ready);
    session.key_released(&alt);
    assert_eq!(session.mode(), InputMode::KeyTips);
    assert!(page.press("q", Focus::Stage));
    assert_eq!(session.mode(), InputMode::Ready);

    assert!(page.press("F10", Focus::Stage));
    assert!(page.press("m", Focus::Stage));
    assert!(page.press("Escape", Focus::Stage));
    assert_eq!(session.mode(), InputMode::KeyTips);
    assert!(page.press("Escape", Focus::Stage));
    assert_eq!(session.mode(), InputMode::Ready);
    assert!(page.press("j", Focus::Stage));
    assert_eq!(session.edit().unwrap().text, "j");
    assert!(!page.press("F10", Focus::Editor));
}

/// Native-only: drives workbook clipboard commands through the real host.
#[test]
fn classic_paste_values_keeps_the_copied_result_and_pastes_as_one_edit() {
    let page = Page::open();
    let session = page.session;
    session.begin_edit(page.at(1, 1), Some("=21*2".into()));
    session.commit_edit();
    page.settle();
    session.begin_edit(page.at(1, 2), Some("'001".into()));
    session.commit_edit();
    page.settle();
    session.select_cell(page.at(1, 1));
    page.settle();
    assert_eq!(page.shown(page.at(1, 1)), "42");
    assert!(page.press("Shift+ArrowRight", Focus::Stage));
    assert!(page.press("Ctrl+c", Focus::Stage));

    page.host.with_session(SelectionCause::Open, |host| {
        let receipt = host.dispatch(SkinIntent::EnterCell {
            at: page.at(1, 1).cell().unwrap(),
            text: "=20*2".into(),
        });
        assert!(receipt.error.is_none());
    });
    page.settle();
    session.select_cell(page.at(2, 2));
    page.settle();
    for key in ["F10", "h", "v", "v"] {
        assert!(page.press(key, Focus::Stage), "{key}");
    }
    assert_eq!(page.shown(page.at(2, 2)), "42");
    assert_eq!(page.shown(page.at(2, 3)), "001");
    assert_eq!(session.entry(), "42");
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert_eq!(page.shown(page.at(2, 2)), "");
    assert_eq!(page.shown(page.at(2, 3)), "");
    assert_eq!(page.shown(page.at(1, 1)), "40");

    for key in ["F10", "e", "s", "v", "Enter"] {
        assert!(page.press(key, Focus::Stage), "{key}");
    }
    assert_eq!(session.entry(), "42");
}

/// Native-only: drives workbook clipboard commands through the real host.
#[test]
fn classic_paste_formulas_translates_references_at_the_destination() {
    let page = Page::open();
    let session = page.session;
    session.begin_edit(page.at(1, 2), Some("=A1+$A$1".into()));
    session.commit_edit();
    page.settle();
    assert!(page.press("Ctrl+c", Focus::Stage));
    session.select_cell(page.at(3, 3));
    page.settle();
    for key in ["F10", "h", "v", "f"] {
        assert!(page.press(key, Focus::Stage), "{key}");
    }
    assert_eq!(session.entry(), "=B3+$A$1");
}

/// Native-only: interleaves a real host edit between a paste preview and its commit.
#[test]
fn classic_paste_refuses_if_the_host_changes_after_preview() {
    let page = Page::open();
    let session = page.session;
    session.begin_edit(page.at(1, 1), Some("42".into()));
    session.commit_edit();
    page.settle();
    assert!(page.press("Ctrl+c", Focus::Stage));
    session.select_cell(page.at(2, 2));
    page.settle();
    session.command("classic.paste-values").unwrap();
    let preview = page.sent.borrow_mut().pop_front().expect("paste preview");
    session.deliver(page.port.handle(preview));
    assert_eq!(session.seam(), Seam::Committing);

    page.host.with_session(SelectionCause::Open, |host| {
        let receipt = host.dispatch(SkinIntent::EnterCell {
            at: page.at(2, 2).cell().unwrap(),
            text: "99".into(),
        });
        assert!(receipt.error.is_none());
    });
    page.settle();

    assert!(matches!(session.last_refusal(), Some(calc_skin::Refusal::Conflict { .. })));
    assert_eq!(page.shown(page.at(2, 2)), "99");
}

/// Native-only: drives the real workbook host through packets.
#[test]
fn classic_shift_selects_a_rectangle_and_delete_clears_it_in_one_step() {
    use calc_skin::{SourceRef, grid::Area};

    let page = Page::open();
    let session = page.session;
    for (at, text) in [(page.at(1, 1), "1"), (page.at(2, 2), "=20+22"), (page.at(2, 3), "5")] {
        session.select_cell(at);
        page.settle();
        session.edit_text(text.into());
        session.commit_edit();
        page.settle();
    }
    session.select_cell(page.at(1, 1));
    page.settle();

    assert!(page.press("Shift+ArrowDown", Focus::Stage));
    assert!(page.press("Shift+ArrowRight", Focus::Stage));
    assert_eq!(session.active(), Some(page.at(1, 1)));
    assert_eq!(session.selection().unwrap().target, SourceRef::Cells {
        sheet: page.sheet,
        areas: vec![Area(page.at(1, 1), page.at(2, 2))],
    });
    assert!(page.press("Delete", Focus::Stage));
    assert_eq!(page.shown(page.at(1, 1)), "");
    assert_eq!(page.shown(page.at(2, 2)), "");
    assert_eq!(page.shown(page.at(2, 3)), "5");

    assert!(page.press("Ctrl+z", Focus::Stage));
    assert_eq!(page.shown(page.at(1, 1)), "1");
    assert_eq!(page.shown(page.at(2, 2)), "42");
}

/// Native-only: drives the real workbook host through packets.
#[test]
fn classic_commit_keys_walk_inside_a_selection_and_wrap() {
    let page = Page::open();
    let session = page.session;
    assert!(page.press("Shift+ArrowDown", Focus::Stage));
    assert!(page.press("Shift+ArrowRight", Focus::Stage));
    let selection = session.selection().unwrap().target;
    assert!(page.press("4", Focus::Stage));
    assert_eq!(session.selection().unwrap().target, selection);
    assert!(page.press("Tab", Focus::Editor));
    assert_eq!(session.active(), Some(page.at(1, 2)));
    assert_eq!(page.shown(page.at(1, 1)), "4");
    assert!(page.press("5", Focus::Stage));
    assert!(page.press("Enter", Focus::Editor));
    assert_eq!(session.active(), Some(page.at(2, 2)));
    assert_eq!(page.shown(page.at(1, 2)), "5");
    assert!(page.press("Enter", Focus::Stage));
    assert_eq!(session.active(), Some(page.at(1, 1)));
    assert!(page.press("Shift+Enter", Focus::Stage));
    assert_eq!(session.active(), Some(page.at(2, 2)));
    assert_eq!(session.selection().unwrap().target, selection);
}

/// Native-only: drives the real workbook host through packets.
#[test]
fn classic_enter_returns_to_the_start_column_of_a_tab_run() {
    let page = Page::open();
    assert!(page.press("1", Focus::Stage));
    assert!(page.press("Tab", Focus::Editor));
    assert_eq!(page.session.active(), Some(page.at(1, 2)));
    assert!(page.press("2", Focus::Stage));
    assert!(page.press("Tab", Focus::Editor));
    assert_eq!(page.session.active(), Some(page.at(1, 3)));
    assert!(page.press("3", Focus::Stage));
    assert!(page.press("Enter", Focus::Editor));
    assert_eq!(page.session.active(), Some(page.at(2, 1)));
    assert_eq!(page.shown(page.at(1, 1)), "1");
    assert_eq!(page.shown(page.at(1, 2)), "2");
    assert_eq!(page.shown(page.at(1, 3)), "3");
}

/// Native-only: links calc-host and calc-engine, which the page packages do not build for the browser tests.
#[test]
fn a_command_that_cannot_run_now_says_so_on_the_strip() {
    let page = Page::open();
    let session = page.session;
    assert!(page.press("x", Focus::Stage));
    assert!(matches!(
        session.command("edit.undo"),
        Err(CommandError::NotNow(_))
    ));
    assert_eq!(session.notice(), "Undo is not available now.");
    assert!(matches!(
        session.command("no.such"),
        Err(CommandError::Unknown(_))
    ));
    // The draft is untouched by either refusal.
    assert_eq!(session.edit().map(|draft| draft.text), Some("x".into()));
}

/// Native-only: links calc-host and calc-engine, which the page packages do not build for the browser tests.
#[test]
fn a_reader_runs_again_only_for_what_it_read() {
    let page = Page::open();
    let session = page.session;
    let runs = Arc::new(AtomicU32::new(0));
    let active = Memo::new({
        let runs = runs.clone();
        move |_| {
            runs.fetch_add(1, Ordering::Relaxed);
            session.active()
        }
    });
    assert_eq!(active.get_untracked(), Some(page.at(1, 1)));
    // Typing, committing and the frames that follow do not move the selection.
    page.press("7", Focus::Stage);
    session.commit_edit();
    page.settle();
    assert_eq!(page.shown(page.at(1, 1)), "7");
    assert_eq!(active.get_untracked(), Some(page.at(1, 1)));
    assert_eq!(runs.load(Ordering::Relaxed), 1);
    page.press("ArrowDown", Focus::Stage);
    assert_eq!(active.get_untracked(), Some(page.at(2, 1)));
    assert!(runs.load(Ordering::Relaxed) > 1);
}

/// Native-only: drives the real workbook host through packets.
#[test]
fn inspector_and_dock_switch_without_changing_the_workbook() {
    let page = Page::open();
    let session = page.session;
    let stamp = session.stamp();
    let selection = session.selection();

    session.command("inspector.dock").unwrap();
    assert_eq!(session.view().0, "classic");
    session.command("inspector.enlarge").unwrap();
    assert_eq!(session.view().0, "inspector");
    session.command("inspector.dependents").unwrap();
    session.command("view.classic").unwrap();
    page.settle();
    assert_eq!(session.selection(), selection);
    assert_eq!(session.stamp(), stamp);
}

/// Native-only: drives the real workbook host through packets.
#[test]
fn explorer_trial_has_the_exact_caller_inputs_including_blanks() {
    use calc_skin::{FormulaAnswer, FormulaPort, FormulaQuery, Provenance, RawValue, TrialValue};

    let page = Page::open();
    let session = page.session;
    for (at, text) in [(page.at(1, 1), "42"), (page.at(3, 1), "=SUM(A1:A2)")] {
        session.select_cell(at);
        page.settle();
        session.edit_text(text.into());
        session.commit_edit();
        page.settle();
    }
    let watch = session.watch(query::TrialFor { at: page.at(3, 1) });
    page.settle();
    let Live::Ready(answer) = session.read(watch) else {
        panic!("The workbook did not serve a caller trial");
    };
    assert_eq!(answer.body.formula, "=SUM(A1:A2)");
    let caller = answer.body.caller.as_ref().unwrap();
    assert_eq!(caller.stamp, session.stamp().unwrap());
    assert_eq!(caller.at, page.at(3, 1));
    assert_eq!(caller.sheets.len(), 1);
    assert_eq!(caller.precedents.len(), 2);
    assert_eq!(caller.precedents[0].at, page.at(1, 1));
    assert_eq!(caller.precedents[0].shown.value, RawValue::Number { bits: 42_f64.to_bits() });
    assert_eq!(caller.precedents[0].shown.provenance, Provenance::Literal);
    assert_eq!(caller.precedents[1].at, page.at(2, 1));
    assert_eq!(caller.precedents[1].shown.value, RawValue::Blank);
    assert_eq!(caller.precedents[1].shown.provenance, Provenance::Empty);
    assert_eq!(session.active(), Some(caller.at));
    let stamp = caller.stamp;
    let result = Rc::new(RefCell::new(None));
    session.ask(FormulaQuery::Evaluate(answer.body), Box::new({
        let result = result.clone();
        move |answer| *result.borrow_mut() = Some(answer)
    }));
    page.settle();
    let Some(Ok(FormulaAnswer::Evaluated(shown))) = result.take() else {
        panic!("The session did not return the trial result");
    };
    let TrialValue::Scalar(cell) = shown.value else {
        panic!("Expected one result cell");
    };
    assert_eq!(cell.raw, RawValue::Number { bits: 42_f64.to_bits() });
    assert_eq!(session.stamp(), Some(stamp));
}

/// Native-only: drives the real workbook host through packets.
#[test]
fn inspector_audits_large_ranges_and_previews_a_single_cell_repair() {
    use calc_skin::{Budget, Evidence};
    use dna_kit::commands::CommandId;
    let page = Page::open();
    let session = page.session;
    let at = page.at(1, 1);
    session.edit_text(r#"=SUMIFS(B1:B5000,C1:C5000,"West")"#.into());
    session.commit_edit();
    page.settle();

    let watch = session.following_with_budget::<query::ContributionsAt>(Budget {
        cells: 20_000,
        nodes: 4096,
        edges: 4096,
        bytes: 1_000_000,
        millis: 2000,
    });
    let query = query::ContributionsAt { at };
    watch.at(Some(query.clone()));
    page.settle();
    assert!(matches!(watch.at(Some(query)), Live::Ready(answer)
        if matches!(&answer.body, Evidence::Derived { body, .. } if body.inspected == 5000)));

    let before = session.stamp().unwrap();
    let original = session.entry();
    let replacement = r#"=SUMIFS(B1:B5000,C1:C5000,"West",D1:D5000,"Paid")"#;
    session
        .run(&dna_kit::Invocation {
            command: CommandId::new("inspector.repair"),
            args: serde_json::json!([at, original, replacement, before]),
        })
        .unwrap();
    page.settle();
    let Seam::Review(preview) = session.seam() else {
        panic!("expected explicit review")
    };
    assert_eq!(preview.writes.len(), 1);
    assert_eq!(preview.writes[0].at, at);
    assert_eq!(preview.writes[0].before.as_deref(), Some(original.as_str()));
    assert_eq!(preview.writes[0].after.as_deref(), Some(replacement));
    assert_eq!(session.entry(), original);
    assert_eq!(session.stamp(), Some(before));
    session.command("edit.cancel").unwrap();
    assert_eq!(session.seam(), Seam::Idle);
    session
        .run(&dna_kit::Invocation {
            command: CommandId::new("inspector.repair"),
            args: serde_json::json!([at, original, replacement, before]),
        })
        .unwrap();
    page.settle();
    session.command("edit.apply-review").unwrap();
    page.settle();
    assert_eq!(session.entry(), replacement);
    assert!(session.stamp().unwrap().model > before.model);
}

/// Native-only: drives the real workbook host through packets.
#[test]
fn inspector_run_repair_translates_matching_siblings_and_undoes_once() {
    use calc_skin::grid::Area;
    use dna_kit::commands::CommandId;

    let page = Page::open();
    let session = page.session;
    for row in 1..=5 {
        session.select_cell(page.at(row, 2));
        page.settle();
        let text = if row == 3 { "=99".into() } else { format!("=A{row}+2") };
        session.edit_text(text);
        session.commit_edit();
        page.settle();
    }
    let at = page.at(2, 2);
    session.select_cell(at);
    page.settle();
    let before = session.stamp().unwrap();
    let extent = Area(page.at(1, 2), page.at(5, 2));
    session
        .run(&dna_kit::Invocation {
            command: CommandId::new("inspector.repair-run"),
            args: serde_json::json!([at, "=A2+2", "=A2+$C$1+3", before, extent]),
        })
        .unwrap();
    page.settle();

    let Seam::Review(preview) = session.seam() else {
        panic!("a repair of matching siblings requires explicit review");
    };
    let writes: Vec<_> = preview
        .writes
        .iter()
        .map(|write| (write.at, write.after.as_deref().unwrap()))
        .collect();
    assert_eq!(
        writes,
        vec![
            (page.at(1, 2), "=A1+$C$1+3"),
            (at, "=A2+$C$1+3"),
            (page.at(4, 2), "=A4+$C$1+3"),
            (page.at(5, 2), "=A5+$C$1+3"),
        ]
    );
    assert_eq!(session.stamp(), Some(before));
    session.command("edit.apply-review").unwrap();
    page.settle();
    assert_eq!(page.shown(page.at(1, 2)), "3");
    assert_eq!(page.shown(page.at(3, 2)), "99");
    assert_eq!(page.shown(page.at(5, 2)), "3");
    session.command("edit.undo").unwrap();
    page.settle();
    assert_eq!(page.shown(page.at(1, 2)), "2");
    assert_eq!(page.shown(page.at(5, 2)), "2");
}

/// Native-only: drives the real workbook host through packets.
#[test]
fn inspector_finding_uses_presentation_history_and_undo() {
    let page = Page::open();
    let session = page.session;
    let before = session.stamp().unwrap();
    let text = "Check the Revenue.Window definition";
    session
        .run(&dna_kit::Invocation {
            command: dna_kit::commands::CommandId::new("inspector.note-put"),
            args: serde_json::json!([page.at(1, 1), before, null, null, text]),
        })
        .unwrap();
    page.settle();
    assert_eq!(session.stamp().unwrap().model, before.model);
    assert!(session.stamp().unwrap().view > before.view);
    let notes = session.watch(query::NotesOf { anchor: None });
    page.settle();
    assert!(
        matches!(session.read(notes), Live::Ready(answer) if answer.body.len() == 1
        && answer.body[0].text == text && answer.body[0].source.as_ref() == Some(&session.selection().unwrap().target))
    );
    session.command("edit.undo").unwrap();
    page.settle();
    assert!(matches!(session.read(notes), Live::Ready(answer) if answer.body.is_empty()));
}

/// Native-only: drives the real workbook service by packets.
#[test]
fn notebook_linking_and_reordering_only_edit_the_view_part() {
    let page = Page::open();
    let session = page.session;
    let a1 = page.at(1, 1);
    let b1 = page.at(1, 2);
    session.edit_text("=21*2".into());
    session.commit_edit();
    page.settle();

    let before = session.stamp().unwrap();
    session.command("view.notebook").unwrap();
    page.settle();
    assert_eq!(session.stamp(), Some(before));
    assert_eq!(session.mode(), InputMode::NotebookCommand);

    session.command("notebook.link").unwrap();
    page.settle();
    let linked = session.stamp().unwrap();
    assert_eq!(linked.model, before.model);
    assert!(linked.view > before.view);

    session.select_cell(b1);
    page.settle();
    session.command("notebook.link").unwrap();
    page.settle();
    assert!(page.press("Alt+ArrowUp", Focus::Stage));
    assert!(page.press("Alt+p", Focus::Stage));
    assert_eq!(session.stamp().unwrap().model, before.model);
    assert_eq!(page.shown(a1), "42");
    assert!(page.press("ArrowDown", Focus::Stage));
    assert_eq!(session.active(), Some(a1));
    assert!(page.press("d", Focus::Stage));
    assert!(page.press("d", Focus::Stage));
    assert_eq!(session.stamp().unwrap().model, before.model);
    assert_eq!(page.shown(a1), "42", "unlinking preserves the referenced worksheet cell");
}

/// Native-only: drives Notebook authoring, placement and undo through the workbook service.
#[test]
fn notebook_authors_scalar_and_array_anchors_in_one_history_step_each() {
    use calc_skin::SourceRef;
    use calc_skin::grid::Area;
    use dna_kit::commands::CommandId;

    let page = Page::open();
    let session = page.session;
    session.command("view.notebook").unwrap();
    page.settle();
    let before = session.stamp().unwrap();
    let part = session.watch(query::ViewPartOf);
    let run = |command, args| {
        session
            .run(&dna_kit::Invocation {
                command: CommandId::new(command),
                args,
            })
            .unwrap();
        page.settle();
    };

    session.command("notebook.new-formula").unwrap();
    page.settle();
    assert_eq!(session.stamp(), Some(before), "offering a slot is read-only");
    run("notebook.author", serde_json::json!("=21*2"));
    assert_eq!(page.shown(page.at(2, 2)), "42");

    for formula in ["=SEQUENCE(3,1)", "=SEQUENCE(2,1)"] {
        session.command("notebook.new-formula").unwrap();
        page.settle();
        run("notebook.author-columns", serde_json::json!(2));
        run("notebook.author", serde_json::json!(formula));
    }
    assert_eq!(page.shown(page.at(12, 1)), "3");
    assert_eq!(page.shown(page.at(11, 4)), "2");
    let Live::Ready(authored) = session.read(part) else {
        panic!("Notebook view part");
    };
    assert_eq!(authored.body.anchors.len(), 3);
    assert!(authored.body.anchors.iter().any(|anchor| {
        anchor.source == Some(SourceRef::Spill { anchor: page.at(10, 1) })
            && anchor.area == Some(Area(page.at(10, 1), page.at(12, 1)))
    }));

    session.command("edit.undo").unwrap();
    page.settle();
    assert_eq!(page.shown(page.at(10, 4)), "");
    assert_eq!(page.shown(page.at(12, 1)), "3");
    let Live::Ready(undone) = session.read(part) else {
        panic!("Notebook view part after undo");
    };
    assert_eq!(undone.body.anchors.len(), 2);
}

/// Native-only: drives the real workbook service by packets.
#[test]
fn notebook_escape_holds_the_old_output_until_the_formula_runs() {
    let page = Page::open();
    let session = page.session;
    let a1 = page.at(1, 1);
    let before = session.stamp().unwrap();
    session.command("view.notebook").unwrap();
    session.command("notebook.link").unwrap();
    page.settle();

    assert!(page.press("Enter", Focus::Stage));
    assert_eq!(session.mode(), InputMode::Edit);
    session.edit_text("=6*7".into());
    assert!(!page.press("Enter", Focus::Editor), "plain Enter belongs to the text editor");
    assert!(page.press("Escape", Focus::Editor));
    assert_eq!(session.mode(), InputMode::NotebookCommand);
    assert_eq!(session.held_edit(a1).unwrap().text, "=6*7");
    assert_eq!(page.shown(a1), "");
    assert_eq!(session.stamp().unwrap().model, before.model);
    assert!(!page.press("q", Focus::Stage));
    assert!(session.edit().is_none());

    session.command("view.classic").unwrap();
    page.settle();
    assert_eq!(session.held_edits(), 1);
    assert!(page.press("F2", Focus::Stage));
    assert_eq!(session.entry(), "");
    assert!(page.press("Escape", Focus::Editor));

    session.command("view.notebook").unwrap();
    assert!(page.press("Enter", Focus::Stage));
    assert_eq!(session.entry(), "=6*7");
    assert!(page.press("Ctrl+Enter", Focus::Editor));
    assert_eq!(page.shown(a1), "42");
    assert_eq!(session.held_edits(), 0);
    assert_eq!(session.mode(), InputMode::NotebookCommand);
}


/// Native-only: drives the real workbook service by packets.
#[test]
fn notebook_run_all_gives_each_draft_its_own_undo_step() {
    let page = Page::open();
    let session = page.session;
    let a1 = page.at(1, 1);
    let b1 = page.at(1, 2);
    session.command("view.notebook").unwrap();

    for (at, formula) in [(a1, "=20+1"), (b1, "=A1*2")] {
        session.select_cell(at);
        page.settle();
        session.command("notebook.link").unwrap();
        page.settle();
        assert!(page.press("Enter", Focus::Stage));
        session.edit_text(formula.into());
        assert!(page.press("Escape", Focus::Editor));
    }
    assert_eq!(session.held_edits(), 2);
    assert_eq!((page.shown(a1), page.shown(b1)), ("".into(), "".into()));

    session.command("notebook.run-all").unwrap();
    page.settle();
    assert_eq!(session.held_edits(), 0);
    assert_eq!((page.shown(a1), page.shown(b1)), ("21".into(), "42".into()));

    session.command("edit.undo").unwrap();
    page.settle();
    assert_eq!((page.shown(a1), page.shown(b1)), ("21".into(), "".into()));
    session.command("edit.undo").unwrap();
    page.settle();
    assert_eq!((page.shown(a1), page.shown(b1)), ("".into(), "".into()));
}

/// Native-only: saves and reopens a notebook through the real workbook service.
#[test]
fn notebook_delete_prefix_does_not_carry_into_a_reopened_workbook() {
    let page = Page::open();
    let session = page.session;
    session.command("view.notebook").unwrap();
    session.command("notebook.link").unwrap();
    page.settle();
    let bytes = page.host.with_session(calc_skin::SelectionCause::Open, |workbook| {
        let mut bytes = Vec::new();
        workbook
            .save(&calc_host::SavePolicy::default(), &mut bytes)
            .unwrap();
        bytes
    });

    assert!(page.press("d", Focus::Stage));
    session.open("Reopened notebook.xlsx".into(), bytes);
    page.settle();
    let reopened = session.stamp().unwrap();
    assert_eq!(session.view().0, "classic");
    session.command("view.notebook").unwrap();

    assert!(page.press("d", Focus::Stage));
    assert_eq!(session.stamp().unwrap().view, reopened.view);
    assert!(page.press("d", Focus::Stage));
    assert!(session.stamp().unwrap().view > reopened.view);
}

/// Native-only: opens generated workbooks through the real workbook service.
#[test]
fn opening_app_requires_a_saved_preference_and_a_readable_page() {
    const APP: &str = r#"<decl id="00000000-0000-0000-0000-000000000001" view="app" schema="1"><![CDATA[{"schema":"dna.page/1","title":"Planner","root":{"kind":"Stack","children":[]}}]]></decl>"#;

    let page = Page::open();
    let session = page.session;
    let open = |prefs: &str, docs: &str| {
        let ns = "urn:dnacalc:views:v1";
        let bytes = calc_doc::write_session(&calc_doc::SessionFile {
            sheet: "Loan".into(),
            formula: None,
            constants: Vec::new(),
            custom: Some(calc_doc::CustomXmlItem {
                namespace: ns.into(),
                content: format!(r#"<dnaViews xmlns="{ns}" version="1">{prefs}<decls>{docs}</decls></dnaViews>"#)
                    .into_bytes(),
            }),
            date_1904: false,
            cached: None,
            vba: None,
            look: None,
        }).unwrap();
        session.open("Planner.xlsx".into(), bytes);
        page.settle();
    };

    open("", APP);
    let Live::Ready(part) = session.view_part() else {
        panic!("the authored App declaration did not load");
    };
    assert_eq!(part.body.documents.len(), 1);
    assert_eq!(session.view().0, "classic", "page alone is not consent");

    let preferred = r#"<prefs preferred="app"/>"#;
    open(preferred, APP);
    assert_eq!(session.view().0, "app");
    assert!(page.press("Alt+Shift+1", Focus::Stage));
    assert_eq!(session.view().0, "classic");
    assert!(page.press("F9", Focus::Stage));
    assert_eq!(session.view().0, "classic", "later frames preserve the chosen view");

    for docs in ["".into(), APP.replace("dna.page/1", "dna.page/2")] {
        session.command("view.app").unwrap();
        open(preferred, &docs);
        assert_eq!(session.view().0, "classic");
    }
}

/// Native-only: saves and reopens a Forms maker preference through the real workbook service.
#[test]
fn forms_saves_and_undoes_the_opening_preference_as_presentation() {
    let page = Page::open();
    let session = page.session;
    let save = || {
        page.host.with_session(calc_skin::SelectionCause::Open, |workbook| {
            let mut bytes = Vec::new();
            workbook.save(&calc_host::SavePolicy::default(), &mut bytes).unwrap();
            bytes
        })
    };
    session.command("view.forms").unwrap();
    session.command("forms.create").unwrap();
    page.settle();
    let before = session.stamp().unwrap();
    session.command("forms.prefer-app").unwrap();
    page.settle();
    assert_eq!(session.view().0, "forms");
    let preferred = session.stamp().unwrap();
    assert!(preferred.view > before.view);
    assert_eq!(preferred.model, before.model);
    let app_bytes = save();

    assert!(page.press("Ctrl+z", Focus::Stage));
    let classic_bytes = save();
    session.open("Planner.xlsx".into(), classic_bytes);
    page.settle();
    assert_eq!(session.view().0, "classic");
    session.open("Planner.xlsx".into(), app_bytes);
    page.settle();
    assert_eq!(session.view().0, "app");

    session.command("view.forms").unwrap();
    session.command("forms.prefer-classic").unwrap();
    page.settle();
    session.open("Planner.xlsx".into(), save());
    page.settle();
    assert_eq!(session.view().0, "classic");
}

/// Native-only: exercises the App and Forms commands through the real workbook service.
#[test]
fn app_drafts_only_from_certain_workbook_signals_and_keeps_on_request() {
    use calc_skin::{AnchorId, Batch, Change, Page as PageDocument, PresentationChange, SourceRef, WorkbookPort};
    use std::num::NonZeroU128;

    let page = Page::open();
    let session = page.session;
    let at = page.at(2, 2);
    session.select_cell(page.at(1, 1));
    page.settle();
    assert!(page.press("8", Focus::Stage));
    assert!(page.press("Enter", Focus::Editor));
    session.command("view.app").unwrap();
    assert_eq!(
        session.command("app.draft-from-file"),
        Err(CommandError::Refused("No certain App inputs were found in this workbook.".into()))
    );
    assert_eq!(session.view().0, "app");

    page.host.with_session(calc_skin::SelectionCause::Open, |workbook| {
        let receipt = workbook.dispatch(calc_host::SkinIntent::Edit {
            edits: vec![calc_host::CellEdit::Validations {
                sheet: page.sheet.id(),
                rules: vec![calc_doc::Validation {
                    authored_copy: false,
                    source: None,
                    areas: vec![(2, 2, 2, 2)],
                    kind: "list".into(),
                    operator: "between".into(),
                    allow_blank: true,
                    show_input: true,
                    show_error: true,
                    in_cell_dropdown: true,
                    error_style: "stop".into(),
                    error_title: None,
                    error: Some("Choose Small or Large".into()),
                    prompt_title: Some("Size".into()),
                    prompt: None,
                    formula1: Some(r#""Small,Large""#.into()),
                    formula2: None,
                }],
            }],
        });
        assert!(receipt.accepted(), "{:?}", receipt.error);
    });
    page.settle();
    let before = session.stamp().unwrap();
    session.command("app.draft-from-file").unwrap();
    page.settle();
    assert_eq!(session.view().0, "forms");
    assert_eq!(session.stamp().unwrap(), before);
    let Live::Ready(part) = session.view_part() else { panic!("missing view part"); };
    assert!(part.body.documents.is_empty());
    assert!(part.body.anchors.is_empty());

    session.command("forms.keep-draft").unwrap();
    page.settle();
    let Live::Ready(part) = session.view_part() else { panic!("missing view part"); };
    assert_eq!(part.body.documents.len(), 1);
    assert_eq!(part.body.anchors.len(), 1);
    assert!(part.body.preferred.is_none());
    let document: PageDocument = serde_json::from_value(part.body.documents[0].body.clone()).unwrap();
    let mut addresses = Vec::new();
    document.root.for_each_part(&mut |_, component, bind| {
        assert_eq!(component, "input");
        addresses.push(bind["value"].address.clone());
    });
    assert_eq!(addresses, ["Sheet1!B2"]);
    assert_eq!(session.stamp().unwrap().model, before.model);
    session.command("view.app").unwrap();
    assert_eq!(part.body.anchors[0].area, Some(calc_skin::grid::Area(at, at)));

    assert!(page.press("Ctrl+z", Focus::Stage));
    let Live::Ready(part) = session.view_part() else { panic!("missing view part"); };
    assert!(part.body.documents.is_empty());
    assert!(part.body.anchors.is_empty());

    session.command("app.draft-from-file").unwrap();
    let stamp = session.stamp().unwrap();
    let a1 = page.at(1, 1);
    let area = calc_skin::grid::Area(a1, a1);
    let other = page.host.connect(
        Origin::User { view: ViewId("inspector".into()) },
        WritePolicy::Commit,
    );
    other.commit(Batch {
        id: "1".into(),
        workbook: Some(stamp.workbook),
        base: Some(stamp.model),
        expects: Vec::new(),
        selection_seq: None,
        label: None,
        changes: vec![Change::Presentation(PresentationChange::Bind {
            anchor: AnchorId(NonZeroU128::new(1).unwrap()),
            source: SourceRef::Cells { sheet: page.sheet, areas: vec![area] },
        })],
    }).unwrap();
    // The page has not received the other author's change yet.
    session.command("forms.keep-draft").unwrap();
    page.settle();
    let Live::Ready(part) = session.view_part() else { panic!("missing view part"); };
    assert!(part.body.documents.is_empty(), "a stale draft was accepted");
    assert_eq!(part.body.anchors[0].area, Some(area));

    session.command("view.app").unwrap();
    session.command("app.draft-from-file").unwrap();
    page.host.with_session(calc_skin::SelectionCause::Open, |workbook| {
        let receipt = workbook.dispatch(calc_host::SkinIntent::Edit {
            edits: vec![calc_host::CellEdit::Validations {
                sheet: page.sheet.id(),
                rules: Vec::new(),
            }],
        });
        assert!(receipt.accepted(), "{:?}", receipt.error);
    });
    session.command("forms.keep-draft").unwrap();
    page.settle();
    assert!(matches!(session.last_refusal(), Some(calc_skin::Refusal::Conflict { .. })));
    let Live::Ready(part) = session.view_part() else { panic!("missing view part"); };
    assert!(part.body.documents.is_empty());
    assert_eq!(part.body.anchors.len(), 1);
}

/// Native-only: opens the existing Form Controls fixture through the real workbook service.
#[test]
fn drafts_the_linked_cells_from_excel_form_controls() {
    let page = Page::open();
    let session = page.session;
    session.open(
        "Controls.xlsx".into(),
        include_bytes!("../../../crates/calc-doc/tests/fixtures/drawing/form-controls.xlsx").to_vec(),
    );
    page.settle();
    session.command("view.app").unwrap();
    session.command("app.draft-from-file").unwrap();
    session.command("forms.keep-draft").unwrap();
    page.settle();
    let Live::Ready(part) = session.view_part() else { panic!("missing view part"); };
    let doc: calc_skin::Page = serde_json::from_value(part.body.documents[0].body.clone()).unwrap();
    let mut addresses = Vec::new();
    doc.root.for_each_part(&mut |_, _, bind| addresses.push(bind["value"].address.clone()));
    let expected: Vec<_> = (2..=8).map(|row| format!("Controls!B{row}")).collect();
    assert_eq!(addresses, expected);
}

/// Native-only: saves a protected worksheet and tests bound entry through the real service.
#[test]
fn only_the_unlocked_cell_of_a_protected_sheet_is_drafted_and_accepts_entry() {
    use calc_skin::WorkbookPort;
    use calc_skin::query::Query;

    let page = Page::open();
    let session = page.session;
    let original = page.host.with_session(calc_skin::SelectionCause::Open, |workbook| {
        let at = calc_host::value::CellKey::new(page.sheet.id(), 2, 2).unwrap();
        let mut styles = workbook.host().styles().as_ref().clone();
        let mut format = styles.cell_formats[0].clone();
        format.protection.get_or_insert_with(Default::default).locked = Some(false);
        let unlocked = styles.cell_formats.len() as u32;
        styles.cell_formats.push(format);
        let receipt = workbook.dispatch(calc_host::SkinIntent::Edit {
            edits: vec![
                calc_host::CellEdit::Styles(Arc::new(styles)),
                calc_host::CellEdit::SetFormat { at, format: Some(unlocked) },
            ],
        });
        assert!(receipt.accepted(), "{:?}", receipt.error);
        let policy = calc_doc::SavePolicy {
            views: Some(Arc::new(calc_doc::SaveViews {
                revision: workbook.host().model().revision().get(),
                active_sheet: Some(at.sheet()),
                sheets: Default::default(),
                protections: [(
                    at.sheet(), Some(calc_doc::SheetProtectionSave { contents: true, file: Default::default() }),
                )].into(),
            })),
            ..Default::default()
        };
        let mut bytes = Vec::new();
        workbook.save(&policy, &mut bytes).unwrap();
        bytes
    });
    session.open("Protected.xlsx".into(), original);
    page.settle();
    session.command("view.app").unwrap();
    session.command("app.draft-from-file").unwrap();
    session.command("forms.keep-draft").unwrap();
    page.settle();
    let Live::Ready(part) = session.view_part() else { panic!("missing view part"); };
    let doc: calc_skin::Page = serde_json::from_value(part.body.documents[0].body.clone()).unwrap();
    let mut inputs = Vec::new();
    doc.root.for_each_part(&mut |_, _, bind| inputs.push(bind["value"].clone()));
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].address, "Sheet1!B2");

    let sheet = session.summary().unwrap().sheets[0].sheet;
    let locked = Key(sheet.0, sheet.1, 1, 1);
    let rules = page.port.ask(query::Rules { at: locked }.request(), dna_skin_runtime::client::PAGE_BUDGET);
    assert!(matches!(query::Rules::decode(rules.unwrap()).unwrap().body.refusal, Some(calc_skin::Refusal::Protected(_))));
    session.command("view.app").unwrap();
    let run = |command, args| {
        session.run(&dna_kit::Invocation {
            command: dna_kit::commands::CommandId::new(command),
            args,
        }).unwrap();
        page.settle();
    };
    run("app.draft-input", serde_json::json!([inputs[0], "42"]));
    run("app.commit-input", serde_json::json!(inputs[0]));
    assert_eq!(page.shown(Key(sheet.0, sheet.1, 2, 2)), "42");
}

/// Native-only: exercises the App and Forms commands through the real workbook service.
#[test]
fn forms_and_app_share_the_page_and_history() {
    use calc_skin::{Bound, Page as PageDocument, PageNode, WorkbookPort};
    use calc_skin::query::Query;
    use dna_kit::Invocation;
    use dna_kit::commands::CommandId;

    let page = Page::open();
    let session = page.session;
    let change = |command, args| {
        session.run(&Invocation { command: CommandId::new(command), args }).unwrap();
        page.settle();
        assert_eq!(session.seam(), Seam::Idle);
        assert_eq!(session.notice(), "");
    };
    let at = page.at(1, 1);
    let model = session.stamp().unwrap().model;
    change("view.forms", serde_json::Value::Null);
    change("forms.create", serde_json::Value::Null);
    change("forms.title", serde_json::json!("Planner"));
    change("forms.add-cell", serde_json::json!("Principal"));
    session.select_cell(page.at(2, 1));
    page.settle();
    change("forms.add-cell", serde_json::json!("Term"));
    change("forms.toggle-input", serde_json::Value::Null);
    assert_eq!(session.stamp().unwrap().model, model);
    let declaration = query::ViewPartOf::decode(page.port.ask(query::ViewPartOf.request(), dna_skin_runtime::client::PAGE_BUDGET).unwrap()).unwrap();
    let document: PageDocument = serde_json::from_value(declaration.body.documents[0].body.clone()).unwrap();
    assert_eq!(document.title, "Planner");
    let PageNode::Stack { children } = document.root else { panic!("expected a Stack"); };
    let bindings: Vec<Bound> = children.into_iter().map(|child| {
        let PageNode::Part { bind, .. } = child else { panic!("expected a Part"); };
        bind["value"].clone()
    }).collect();

    assert!(page.press("Alt+Shift+5", Focus::Stage));
    assert_eq!(session.view().0, "app");
    assert!(!page.press("9", Focus::Stage));
    assert!(session.edit().is_none());
    change("app.draft-input", serde_json::json!([bindings[0], "42"]));
    change("app.draft-input", serde_json::json!([bindings[1], "13"]));
    assert_eq!(session.stamp().unwrap().model, model);
    assert_eq!(page.shown(at), "");
    change("app.apply-inputs", serde_json::Value::Null);
    assert_eq!(page.shown(at), "42");
    assert_eq!(page.shown(page.at(2, 1)), "13");
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert_eq!(page.shown(at), "");
    assert_eq!(page.shown(page.at(2, 1)), "");
    assert!(page.press("Ctrl+y", Focus::Stage));
    assert_eq!(page.shown(at), "42");
    assert_eq!(page.shown(page.at(2, 1)), "13");
}
/// Native-only: drives authoring commands and history through the real workbook service.
#[test]
fn forms_commands_change_control_order_labels_and_sources_without_model_edits() {
    let page = Page::open();
    let session = page.session;
    let run = |command, args| {
        session
            .run(&dna_kit::Invocation {
                command: dna_kit::commands::CommandId::new(command),
                args,
            })
            .unwrap();
        page.settle();
        assert_eq!(session.seam(), Seam::Idle);
        assert_eq!(session.notice(), "");
    };
    let controls = || {
        let Live::Ready(part) = session.view_part() else {
            panic!("missing view part");
        };
        let doc: calc_skin::Page =
            serde_json::from_value(part.body.documents[0].body.clone()).unwrap();
        let calc_skin::PageNode::Stack { children } = doc.root else {
            panic!("expected a column");
        };
        children
            .into_iter()
            .map(|child| {
                let calc_skin::PageNode::Part {
                    id, bind, props, ..
                } = child
                else {
                    panic!("expected a control");
                };
                (
                    id,
                    bind["value"].address.clone(),
                    props["label"].as_str().unwrap().to_owned(),
                )
            })
            .collect::<Vec<_>>()
    };
    let nothing = serde_json::Value::Null;
    let model = session.stamp().unwrap().model;
    run("view.forms", nothing.clone());
    run("forms.create", nothing.clone());
    run("forms.add-cell", serde_json::json!("Rate"));
    session.select_cell(page.at(1, 2));
    page.settle();
    run("forms.add-cell", serde_json::json!("Term"));
    run("forms.select-part", serde_json::json!("cell-1"));
    run("forms.move-part-up", nothing.clone());
    run("forms.label-part", serde_json::json!("Term in months"));
    session.select_cell(page.at(2, 2));
    page.settle();
    run("forms.rebind-part", nothing.clone());
    assert_eq!(
        controls(),
        [
            ("cell-1".into(), "Sheet1!B2".into(), "Term in months".into()),
            ("cell-0".into(), "Sheet1!A1".into(), "Rate".into()),
        ]
    );
    run("forms.remove-part", nothing);
    assert_eq!(controls().len(), 1);
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert_eq!(controls()[0].0, "cell-1");
    assert_eq!(session.stamp().unwrap().model, model);
}
