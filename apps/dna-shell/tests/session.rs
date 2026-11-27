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

/// Native-only: uses the real shell command table, session and workbook host.
#[test]
fn classic_deck_folding_keeps_the_active_cell_and_workbook_revision() {
    let page = Page::open();
    let session = page.session;
    session.select_cell(page.at(2, 2));
    page.settle();
    let before = session.stamp().unwrap().model;
    assert!(page.press("Ctrl+F1", Focus::Stage));
    assert!(page.press("Ctrl+F1", Focus::Stage));
    assert_eq!(session.active(), Some(page.at(2, 2)));
    assert_eq!(session.stamp().unwrap().model, before);
}

/// Native-only: drives the Help chord through the workbook shell and Session.
#[test]
fn classic_help_keeps_the_active_cell_and_workbook_revision() {
    let page = Page::open();
    let session = page.session;
    session.select_cell(page.at(2, 2));
    page.settle();
    let before = session.stamp().unwrap().model;
    assert!(page.press("F1", Focus::Stage));
    assert!(page.press("Escape", Focus::Stage));
    assert_eq!(session.active(), Some(page.at(2, 2)));
    assert_eq!(session.stamp().unwrap().model, before);
}

/// Native-only: drives the Classic command through CalcHost and round-trips a scratch XLSX.
#[test]
fn classic_gridlines_key_tip_persists_as_a_worksheet_view() {
    let page = Page::open();
    let session = page.session;
    page.settle();
    let geometry = session.watch(query::Geometry { sheet: page.sheet });
    page.settle();
    let before = session.stamp().unwrap();
    for key in ["F10", "w", "v", "g"] {
        assert!(page.press(key, Focus::Stage));
    }
    let after = session.stamp().unwrap();
    let show = page.host.with_session(SelectionCause::Open, |book| {
        book.host().sheet_view(page.sheet.id()).unwrap().show_grid_lines.unwrap_or(true)
    });
    assert!(!show, "the Classic command turned off the active sheet's gridlines");
    assert_eq!((after.model, after.calc), (before.model, before.calc));
    assert_ne!(after.view, before.view);
    assert!(session.summary().unwrap().layout_edited);
    assert!(matches!(session.read(geometry), Live::Ready(answer) if answer.stamp.view == after.view));

    let bytes = page.host.with_session(SelectionCause::Open, |book| {
        let mut bytes = Vec::new();
        book.save(&calc_host::SavePolicy::default(), &mut bytes).unwrap();
        bytes
    });
    session.open("Gridlines.xlsx".into(), bytes);
    page.settle();
    let opened_sheet = session.active().unwrap();
    let opened_sheet = Sheet(opened_sheet.0, opened_sheet.1);
    assert_eq!(
        page.host.with_session(SelectionCause::Open, |book| {
            book.host().sheet_view(opened_sheet.id()).unwrap().show_grid_lines
        }),
        Some(false)
    );
}

/// Native-only: drives AutoFilter through the real Classic command and workbook session.
#[test]
fn classic_filter_key_tip_uses_the_current_region_and_undoes_the_toggle() {
    let page = Page::open();
    let session = page.session;
    for (row, column, text) in [
        (1, 1, "Region"),
        (1, 2, "Units"),
        (2, 1, "North"),
        (2, 2, "2"),
        (3, 1, "East"),
        (3, 2, "3"),
        (1, 4, "Outside"),
    ] {
        session.begin_edit(page.at(row, column), Some(text.into()));
        session.commit_edit();
        page.settle();
    }
    session.select_cell(page.at(2, 2));
    page.settle();
    let filter_range = || page.host.with_session(SelectionCause::Open, |book| {
        book.host().model().definitions().auto_filter(page.sheet.id())
            .map(|filter| calc_skin::grid::Area::of(filter.area))
    });
    let range = calc_skin::grid::Area(page.at(1, 1), page.at(3, 2));
    assert_eq!(filter_range(), None);
    for key in ["F10", "a", "t"] {
        assert!(page.press(key, Focus::Stage), "{key}");
    }
    assert_eq!(filter_range(), Some(range), "{} · {:?}", session.notice(), session.seam());
    assert_eq!(session.active(), Some(page.at(2, 2)));
    assert_eq!(page.shown(page.at(2, 2)), "2");
    assert!(page.press("Ctrl+Shift+l", Focus::Stage));
    assert_eq!(filter_range(), None);
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert_eq!(filter_range(), Some(range));
}

/// Native-only: drives CalcHost's fill planner through Classic keyboard commands.
#[test]
fn classic_fill_down_and_right_translate_formulas_as_separate_undo_steps() {
    let page = Page::open();
    let session = page.session;
    for (row, column, text) in [(1, 1, "10"), (2, 1, "30"), (1, 2, "=A1+1"), (4, 1, "4"), (4, 2, "=A4")] {
        session.begin_edit(page.at(row, column), Some(text.into()));
        session.commit_edit();
        page.settle();
    }
    session.select_cell(page.at(1, 2));
    page.settle();
    assert!(page.press("Shift+ArrowDown", Focus::Stage));
    assert!(page.press("Ctrl+d", Focus::Stage));
    assert_eq!(page.shown(page.at(2, 2)), "31");
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert_eq!(page.shown(page.at(2, 2)), "");
    assert_eq!(page.shown(page.at(1, 2)), "11");

    session.select_cell(page.at(4, 2));
    page.settle();
    assert!(page.press("Shift+ArrowRight", Focus::Stage));
    assert!(page.press("Ctrl+r", Focus::Stage));
    assert_eq!(page.shown(page.at(4, 3)), "4");
    session.select_cell(page.at(4, 3));
    page.settle();
    assert_eq!(session.entry(), "=B4");
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert_eq!(page.shown(page.at(4, 3)), "");
}

/// Native-only: drives the Classic name manager through the real workbook service.
#[test]
fn classic_name_manager_versions_and_undoes_defined_names() {
    use calc_skin::{InventoryItem, InventoryKind, NameChange, NameScope};
    use dna_kit::commands::CommandId;

    let page = Page::open();
    let session = page.session;
    session.select_cell(page.at(2, 2));
    page.settle();
    let before = session.stamp().unwrap();
    assert!(page.press("Ctrl+F3", Focus::Stage));
    assert_eq!(session.stamp(), Some(before));
    let names = session.watch(query::Inventory { kinds: vec![InventoryKind::Names] });
    page.settle();
    let change = |base, change| {
        session.run(&dna_kit::Invocation {
            command: CommandId::new("classic.change-name"),
            args: serde_json::json!((base, NameScope::Workbook, change)),
        }).unwrap();
        page.settle();
    };
    let formula = |wanted: &str| {
        let Live::Ready(answer) = session.read(names) else {
            panic!("the name inventory did not arrive");
        };
        answer.body.into_iter().find_map(|item| match item {
            InventoryItem::Name { scope: NameScope::Workbook, name, formula, hidden: false }
                if name == wanted => Some(formula),
            _ => None,
        })
    };
    change(before.model, NameChange::Define { name: "Rate".into(), formula: "=5".into() });
    assert_eq!(formula("Rate").as_deref(), Some("=5"));
    let created = session.stamp().unwrap().model;
    let update = NameChange::SetFormula { name: "Rate".into(), formula: "=7".into() };
    change(before.model, update.clone());
    assert_eq!(session.stamp().unwrap().model, created);
    assert_eq!(formula("Rate").as_deref(), Some("=5"));
    assert_eq!(formula("TaxRate"), None);

    change(created, update);
    assert_eq!(formula("Rate").as_deref(), Some("=7"));
    change(session.stamp().unwrap().model, NameChange::Rename { name: "Rate".into(), to: "TaxRate".into() });
    assert_eq!(formula("TaxRate").as_deref(), Some("=7"));
    assert_eq!(formula("Rate"), None);
    change(session.stamp().unwrap().model, NameChange::Remove { name: "TaxRate".into() });
    assert_eq!(formula("TaxRate"), None);
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert_eq!(formula("TaxRate").as_deref(), Some("=7"));
    assert_eq!(session.active(), Some(page.at(2, 2)));
    assert!(page.press("Escape", Focus::ActionField));
}

/// Native-only: follows Classic's auditing commands through the workbook shell.
#[test]
fn classic_trace_keys_do_not_edit_the_sheet_or_move_its_selection() {
    let page = Page::open();
    let session = page.session;
    session.begin_edit(page.at(2, 2), Some("=A1*2".into()));
    session.commit_edit();
    page.settle();
    let stamp = session.stamp().unwrap();
    for key in ["F10", "m", "p"] {
        assert!(page.press(key, Focus::Stage), "{key}");
    }
    assert!(session.last_refusal().is_none(), "{}", session.notice());
    assert_eq!(session.active(), Some(page.at(2, 2)));
    for key in ["F10", "m", "a", "a"] {
        assert!(page.press(key, Focus::Stage), "{key}");
    }
    assert!(session.last_refusal().is_none(), "{}", session.notice());
    assert_eq!(session.stamp().unwrap().model, stamp.model);
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
    let model = session.stamp().unwrap().model;
    assert!(page.press("F10", Focus::Stage));
    assert_eq!(session.mode(), InputMode::KeyTips);
    assert!(page.press("Tab", Focus::Stage));
    assert!(page.press("ArrowRight", Focus::Stage));
    assert_eq!(session.key_tip_path(), Some(""));
    assert_eq!(session.active(), Some(page.at(1, 1)));
    assert_eq!(session.stamp().unwrap().model, model);
    for key in ["h", "o", "Escape", "o", "i"] {
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

/// Native-only: drives workbook clipboard commands through the real host.
#[test]
fn classic_transpose_rotates_entries_and_preserves_formats_on_blank_cells() {
    use calc_host::value::GridArea;
    use calc_host::{FormatPatch, WorkbookCommand};

    let page = Page::open();
    let session = page.session;
    let format = |at: Key| {
        page.host.with_session(SelectionCause::Open, |host| {
            let receipt = host.dispatch(SkinIntent::Command(WorkbookCommand::FormatCells {
                areas: vec![GridArea::cell(at.cell().unwrap())],
                patch: FormatPatch {
                    number_format: Some("0.00".into()),
                    ..Default::default()
                },
            }));
            assert!(receipt.error.is_none());
        });
        page.settle();
    };
    let transpose = |to| {
        assert!(page.press("Ctrl+c", Focus::Stage));
        session.select_cell(to);
        page.settle();
        for key in ["F10", "h", "v", "t"] {
            assert!(page.press(key, Focus::Stage), "{key}");
        }
    };
    let number_format = |at: Key| {
        page.host.with_session(SelectionCause::Open, |host| {
            host.host().number_format(at.cell().unwrap()).map(str::to_owned)
        })
    };
    for (at, text) in [(page.at(1, 1), "7"), (page.at(1, 2), "8")] {
        session.begin_edit(at, Some(text.into()));
        session.commit_edit();
        page.settle();
    }
    format(page.at(1, 2));
    assert_eq!(number_format(page.at(1, 2)).as_deref(), Some("0.00"));
    session.select_cell(page.at(1, 1));
    page.settle();
    assert!(page.press("Shift+ArrowRight", Focus::Stage));
    transpose(page.at(3, 4));
    assert_eq!(page.shown(page.at(3, 4)), "7");
    assert_eq!(page.shown(page.at(4, 4)), "8");
    assert_eq!(number_format(page.at(4, 4)).as_deref(), Some("0.00"));
    assert_eq!(page.shown(page.at(3, 5)), "");
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert_eq!(page.shown(page.at(3, 4)), "");
    assert_eq!(page.shown(page.at(4, 4)), "");

    let (source, target) = (page.at(5, 1), page.at(5, 4));
    format(source);
    session.select_cell(source);
    page.settle();
    transpose(target);
    assert_eq!(page.shown(target), "");
    assert_eq!(number_format(target).as_deref(), Some("0.00"));
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert_ne!(number_format(target).as_deref(), Some("0.00"));
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

/// Native-only: uses the Classic key map, workbook batches and the real geometry query.
#[test]
fn classic_home_hide_columns_updates_worksheet_geometry_and_undo() {
    use calc_skin::{Selection, SourceRef, grid::Area};

    let page = Page::open();
    let session = page.session;
    let geometry = session.watch(query::Geometry { sheet: page.sheet });
    session.select_cell(page.at(1, 2));
    page.settle();
    let hidden = |column| {
        let Live::Ready(answer) = session.read(geometry) else {
            panic!("worksheet geometry did not arrive");
        };
        answer.body.columns.iter().any(|&(first, last, _, hidden)| {
            (first..=last).contains(&column) && hidden
        })
    };
    assert!(!hidden(2));
    for key in ["F10", "h", "o", "u", "c"] {
        assert!(page.press(key, Focus::Stage), "{key}");
    }
    assert!(hidden(2));
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert!(!hidden(2));

    let (second, fourth) = (page.at(1, 2), page.at(1, 4));
    session.select(Selection {
        seq: 0,
        target: SourceRef::Cells {
            sheet: page.sheet,
            areas: vec![Area(second, second), Area(fourth, fourth)],
        },
        active: Some(second),
        grouped: Vec::new(),
        item: None,
    });
    page.settle();
    assert!(page.press("Ctrl+0", Focus::Stage));
    assert!(hidden(2) && !hidden(3) && hidden(4));
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert!(!hidden(2) && !hidden(4));
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
    let sheet = session.active().unwrap();
    let at = |row, column| Key(sheet.0, sheet.1, row, column);
    assert_eq!(page.shown(at(2, 2)), "42");

    for formula in ["=SEQUENCE(3,1)", "=SEQUENCE(2,1)"] {
        session.command("notebook.new-formula").unwrap();
        page.settle();
        run("notebook.author-columns", serde_json::json!(2));
        run("notebook.author", serde_json::json!(formula));
    }
    assert_eq!(page.shown(at(12, 1)), "3");
    assert_eq!(page.shown(at(11, 4)), "2");
    let Live::Ready(authored) = session.read(part) else {
        panic!("Notebook view part");
    };
    assert_eq!(authored.body.anchors.len(), 3);
    assert!(authored.body.anchors.iter().any(|anchor| {
        anchor.source == Some(SourceRef::Spill { anchor: at(10, 1) })
            && anchor.area == Some(Area(at(10, 1), at(12, 1)))
    }));

    session.command("edit.undo").unwrap();
    page.settle();
    assert_eq!(page.shown(at(10, 4)), "");
    assert_eq!(page.shown(at(12, 1)), "3");
    let Live::Ready(undone) = session.read(part) else {
        panic!("Notebook view part after undo");
    };
    assert_eq!(undone.body.anchors.len(), 2);
}

/// Native-only: drives owned-cell deletion and undo through the workbook service.
#[test]
fn notebook_deletes_owned_spill_anchor_and_declaration_together() {
    let page = Page::open();
    let session = page.session;
    session.command("view.notebook").unwrap();
    session.command("notebook.new-formula").unwrap();
    page.settle();
    for (command, args) in [
        ("notebook.author-columns", serde_json::json!(2)),
        ("notebook.author", serde_json::json!("=SEQUENCE(3,1)")),
    ] {
        session
            .run(&dna_kit::Invocation {
                command: dna_kit::commands::CommandId::new(command),
                args,
            })
            .unwrap();
        page.settle();
    }
    let anchor = session.active().unwrap();
    let filled = Key(anchor.0, anchor.1, anchor.2 + 2, anchor.3);
    assert_eq!(page.shown(filled), "3");
    let part = session.watch(query::ViewPartOf);
    page.settle();

    assert!(page.press("d", Focus::Stage));
    assert!(page.press("d", Focus::Stage));
    assert_eq!(page.shown(anchor), "");
    assert_eq!(page.shown(filled), "");
    let Live::Ready(deleted) = session.read(part) else {
        panic!("Notebook view part after delete");
    };
    assert!(deleted.body.anchors.is_empty());

    session.command("edit.undo").unwrap();
    page.settle();
    assert_eq!(page.shown(filled), "3");
    let Live::Ready(undone) = session.read(part) else {
        panic!("Notebook view part after undo");
    };
    assert_eq!(undone.body.anchors.len(), 1);

    session.command("view.classic").unwrap();
    session.select_cell(page.at(1, 2));
    page.settle();
    session.edit_text("=Analysis!A10+1".into());
    session.commit_edit();
    page.settle();
    assert_eq!(page.shown(page.at(1, 2)), "2");
    session.command("view.notebook").unwrap();
    session
        .run(&dna_kit::Invocation {
            command: dna_kit::commands::CommandId::new("notebook.focus"),
            args: serde_json::json!(undone.body.anchors[0].id),
        })
        .unwrap();
    page.settle();
    session.command("notebook.delete").unwrap();
    page.settle();
    let Seam::Review(preview) = session.seam() else {
        panic!(
            "Review required when another formula reads the cell: {:?}; {}",
            session.seam(),
            session.notice()
        );
    };
    assert_eq!(preview.writes.len(), 1);
    assert_eq!(preview.writes[0].at, anchor);
    assert_eq!(page.shown(filled), "3");
    session.command("edit.apply-review").unwrap();
    page.settle();
    assert_eq!(page.shown(filled), "");
    assert_eq!(page.shown(page.at(1, 2)), "1");
}

/// Native-only: drives the first Notebook formula, structural undo and redo by packets.
#[test]
fn notebook_creates_analysis_only_when_the_first_formula_commits() {
    let page = Page::open();
    let session = page.session;
    session.command("view.notebook").unwrap();
    session.command("notebook.blank").unwrap();
    page.settle();
    let before = session.stamp().unwrap();
    assert_eq!(session.summary().unwrap().sheets.len(), 1);
    assert_eq!(before.model.0, 0);

    session.command("notebook.new-formula").unwrap();
    page.settle();
    assert_eq!(session.stamp(), Some(before));
    let author = |text| {
        session
            .run(&dna_kit::Invocation {
                command: dna_kit::commands::CommandId::new("notebook.author"),
                args: serde_json::json!(text),
            })
            .unwrap();
        page.settle();
    };
    author("=21*");
    assert_eq!(session.stamp(), Some(before));
    assert_eq!(session.summary().unwrap().sheets.len(), 1);

    author("=21*2");
    let summary = session.summary().unwrap();
    let analysis = summary.sheets.iter().find(|sheet| sheet.name == "Analysis").unwrap();
    assert!(analysis.visible && analysis.worksheet);
    let value = Key(analysis.sheet.0, analysis.sheet.1, 2, 2);
    assert_eq!(page.shown(value), "42");
    assert_eq!(page.shown(page.at(2, 2)), "");
    let part = session.watch(query::ViewPartOf);
    page.settle();
    assert!(matches!(session.read(part), Live::Ready(answer) if answer.body.anchors.len() == 1));

    session.command("edit.undo").unwrap();
    page.settle();
    assert_eq!(session.summary().unwrap().sheets.len(), 1);
    assert!(matches!(session.read(part), Live::Ready(answer) if answer.body.anchors.is_empty()));

    session.command("edit.redo").unwrap();
    page.settle();
    assert_eq!(session.summary().unwrap().sheets.len(), 2);
    assert_eq!(page.shown(value), "42");
    assert!(matches!(session.read(part), Live::Ready(answer) if answer.body.anchors.len() == 1));
}

/// Native-only: names and callable LAMBDAs are read and invoked through the workbook service.
#[test]
fn notebook_names_a_spill_renames_its_callers_and_defines_a_lambda() {
    use calc_skin::{InventoryItem, InventoryKind, NameScope};
    use dna_kit::commands::CommandId;

    let page = Page::open();
    let session = page.session;
    let names = session.watch(query::Inventory {
        kinds: vec![InventoryKind::Names],
    });
    let run = |command, args| {
        session
            .run(&dna_kit::Invocation {
                command: CommandId::new(command),
                args,
            })
            .unwrap();
        page.settle();
    };
    session.command("view.notebook").unwrap();
    session.command("notebook.new-formula").unwrap();
    page.settle();
    run("notebook.author-columns", serde_json::json!(1));
    run("notebook.author", serde_json::json!("=SEQUENCE(2)"));
    let output = session.active().unwrap();
    run("notebook.name-output", serde_json::json!("scopeOrders"));

    let Live::Ready(created) = session.read(names) else {
        panic!("Workbook name inventory");
    };
    assert!(
        created.body.iter().any(|record| matches!(record, InventoryItem::Name { scope: NameScope::Workbook, name, formula, .. }
            if name == "scopeOrders" && formula == "=_xlfn.ANCHORARRAY(Analysis!$A$10)")),
        "Workbook names: {:?}; {}",
        created.body,
        session.notice()
    );
    session.command("view.classic").unwrap();
    session.select_cell(page.at(1, 1));
    page.settle();
    session.edit_text("=SUM(scopeOrders)".into());
    session.commit_edit();
    page.settle();
    assert_eq!(page.shown(page.at(1, 1)), "3");

    session.command("view.notebook").unwrap();
    page.settle();
    run("notebook.name-output", serde_json::json!("filteredOrders"));
    assert_eq!(page.shown(output), "1");
    assert_eq!(page.shown(page.at(1, 1)), "3");
    session.select_cell(page.at(1, 1));
    page.settle();
    assert_eq!(session.entry(), "=SUM(filteredOrders)");
    session.command("edit.undo").unwrap();
    page.settle();
    session.select_cell(page.at(1, 1));
    page.settle();
    assert_eq!(session.entry(), "=SUM(scopeOrders)");

    session.command("notebook.delete").unwrap();
    page.settle();
    let Seam::Review(preview) = session.seam() else {
        panic!("Expected a review for removing the name read by SUM");
    };
    assert!(preview.properties.iter().any(|property| matches!(property,
        calc_skin::ModelPropertyDiff::DefinedName { before: Some(name), after: None, .. }
            if name.name == "scopeOrders")));
    assert_eq!(page.shown(output), "1");
    session.command("edit.apply-review").unwrap();
    page.settle();
    assert_eq!(page.shown(output), "");
    assert_eq!(page.shown(page.at(1, 1)), "#NAME?");
    session.command("edit.undo").unwrap();
    page.settle();
    assert_eq!(page.shown(output), "1");
    assert_eq!(page.shown(page.at(1, 1)), "3");

    run(
        "notebook.author-lambda",
        serde_json::json!(["DoubleIt", "=LAMBDA(amount,amount*2)"]),
    );
    let Live::Ready(callable) = session.read(names) else {
        panic!("Workbook name inventory");
    };
    assert!(callable.body.iter().any(|record| matches!(record, InventoryItem::Name { scope: NameScope::Workbook, name, formula, .. }
        if name == "DoubleIt" && formula.contains("LAMBDA"))));
    session.command("view.classic").unwrap();
    session.select_cell(page.at(1, 2));
    page.settle();
    session.edit_text("=DoubleIt(21)".into());
    session.commit_edit();
    page.settle();
    assert_eq!(page.shown(page.at(1, 2)), "42");
}

/// Native-only: exercises a named spill obstruction through the preview and undo route.
#[test]
fn notebook_preview_moves_a_named_spill_obstruction_without_overwriting() {
    use calc_skin::{Batch, Change, InventoryItem, InventoryKind, ModelChange, NameChange, NameScope};
    use dna_kit::commands::CommandId;

    let page = Page::open();
    let session = page.session;
    let run = |command, args| {
        session.run(&dna_kit::Invocation { command: CommandId::new(command), args }).unwrap();
        page.settle();
    };
    session.command("view.notebook").unwrap();
    session.command("notebook.new-formula").unwrap();
    page.settle();
    run("notebook.author-columns", serde_json::json!(1));
    run("notebook.author", serde_json::json!("=SEQUENCE(MAX(1,B2))"));
    let anchor = session.active().unwrap();
    let at = |row, col| Key(anchor.0, anchor.1, row, col);
    assert_eq!(page.shown(anchor), "1");
    let obstruction = at(12, 1);
    let target = at(10, 25);
    let enter = |at, formula: &str| {
        session.select_cell(at);
        page.settle();
        session.edit_text(formula.into());
        session.commit_edit();
        page.settle();
    };
    session.command("view.classic").unwrap();
    enter(obstruction, r#"="Review with Sam""#);
    enter(target, "Reserved");
    let stamp = session.stamp().unwrap();
    session.submit(Batch {
        id: String::new(),
        workbook: Some(stamp.workbook),
        base: Some(stamp.model),
        expects: vec![],
        selection_seq: None,
        label: Some("Name review note".into()),
        changes: vec![Change::Model(ModelChange::Name {
            scope: NameScope::Workbook,
            change: NameChange::Define {
                name: "reviewNote".into(),
                formula: "=Analysis!$A$12".into(),
            },
        })],
    });
    page.settle();
    enter(at(2, 2), "4");
    assert_eq!(page.shown(anchor), "#SPILL!");
    let names = session.watch(query::Inventory { kinds: vec![InventoryKind::Names] });
    let has_note = |address: &str| {
        matches!(session.read(names), Live::Ready(answer) if answer.body.iter().any(|entry| matches!(
            entry, InventoryItem::Name { name, formula, .. } if name == "reviewNote" && formula.ends_with(address)
        )))
    };
    session.command("view.notebook").unwrap();
    page.settle();
    assert!(has_note("$A$12"));
    let blocked = session.stamp().unwrap();
    run("notebook.move-obstruction", serde_json::json!([anchor, obstruction, target]));
    assert_eq!(session.stamp(), Some(blocked));
    assert_eq!(page.shown(target), "Reserved");
    assert_eq!(page.shown(anchor), "#SPILL!");

    let vacant = at(12, 25);
    run("notebook.move-obstruction", serde_json::json!([anchor, obstruction, vacant]));
    let Seam::Review(preview) = session.seam() else {
        panic!("Expected a move review: {}", session.notice());
    };
    assert_eq!(preview.writes.len(), 2);
    assert_eq!(session.stamp(), Some(blocked));
    assert!(has_note("$A$12"));
    session.command("edit.cancel").unwrap();
    page.settle();
    assert!(matches!(session.seam(), Seam::Idle));
    assert_eq!(session.stamp(), Some(blocked));
    assert_eq!(page.shown(anchor), "#SPILL!");
    assert_eq!(page.shown(vacant), "");
    run("notebook.move-obstruction", serde_json::json!([anchor, obstruction, vacant]));
    session.command("edit.apply-review").unwrap();
    page.settle();
    assert_eq!(page.shown(at(13, 1)), "4");
    assert_eq!(page.shown(vacant), "Review with Sam");
    assert!(has_note("$Y$12"));
    session.command("edit.undo").unwrap();
    page.settle();
    assert_eq!(page.shown(anchor), "#SPILL!");
    assert_eq!(page.shown(obstruction), "Review with Sam");
    assert!(has_note("$A$12"));
}

/// Native-only: verifies a moved spill's name, backing anchor and Undo through the workbook packets.
#[test]
fn notebook_moving_a_lane_moves_the_spill_and_the_notebook_anchor() {
    use calc_skin::{InventoryItem, InventoryKind, SourceRef};
    use dna_kit::commands::CommandId;
    let page = Page::open();
    let session = page.session;
    let run = |command, args| {
        session.run(&dna_kit::Invocation { command: CommandId::new(command), args }).unwrap();
        page.settle();
    };
    session.command("view.notebook").unwrap();
    session.command("notebook.new-formula").unwrap();
    page.settle();
    run("notebook.author-columns", serde_json::json!(2));
    run("notebook.author", serde_json::json!("=SEQUENCE(4,2)"));
    let anchor = session.active().unwrap();
    let at = |row, col| Key(anchor.0, anchor.1, row, col);
    let to = at(10, 4);
    run("notebook.name-output", serde_json::json!("scopeOrders"));
    let part = session.watch(query::ViewPartOf);
    let names = session.watch(query::Inventory { kinds: vec![InventoryKind::Names] });
    page.settle();
    run("notebook.move-lane", serde_json::json!([anchor, to]));
    assert!(matches!(session.seam(), Seam::Review(_)));
    assert_eq!(page.shown(at(13, 2)), "8");
    assert_eq!(page.shown(to), "");
    session.command("edit.apply-review").unwrap();
    page.settle();
    assert_eq!(page.shown(anchor), "");
    assert_eq!(page.shown(at(13, 5)), "8");
    assert!(matches!(session.read(part), Live::Ready(answer) if answer.body.anchors.iter().any(|a|
        a.source == Some(SourceRef::Spill { anchor: to }))));
    assert!(matches!(session.read(names), Live::Ready(answer) if answer.body.iter().any(|record| matches!(record,
        InventoryItem::Name { name, formula, .. } if name == "scopeOrders" && formula.contains("$D$10")
    ))));
    session.command("edit.undo").unwrap();
    page.settle();
    assert_eq!(page.shown(at(13, 2)), "8");
    assert_eq!(page.shown(to), "");
    assert!(matches!(session.read(part), Live::Ready(answer) if answer.body.anchors.iter().any(|a|
        a.source == Some(SourceRef::Spill { anchor }))));
    assert!(matches!(session.read(names), Live::Ready(answer) if answer.body.iter().any(|record| matches!(record,
        InventoryItem::Name { name, formula, .. } if name == "scopeOrders" && formula.contains("$A$10")
    ))));
}

/// Native-only: drives Notebook insert, advance, and commit through the Session keymap.
#[test]
fn notebook_jupyter_inserts_code_slots_and_advances_after_a_successful_run() {
    let page = Page::open();
    let session = page.session;
    session.command("view.notebook").unwrap();
    session.command("notebook.blank").unwrap();
    let part = session.watch(query::ViewPartOf);
    page.settle();
    let cells = || {
        let Live::Ready(snapshot) = session.read(part) else {
            panic!("Notebook declaration is unavailable");
        };
        snapshot.body.documents.into_iter()
            .find(|doc| doc.view.0 == dna_view_notebook::ID).unwrap()
            .body["cells"].as_array().unwrap().clone()
    };
    assert!(page.press("B", Focus::Stage));
    page.settle();
    assert_eq!(cells()[0]["kind"], "code-slot");
    let empty = session.stamp().unwrap();
    assert!(page.press("Enter", Focus::Stage));
    page.settle();
    let invoke = |command, args| {
        session.run(&dna_kit::Invocation { command: dna_kit::commands::CommandId::new(command), args })
            .unwrap();
        page.settle();
    };
    invoke("notebook.author", serde_json::json!("=20+22"));
    let at = session.active().unwrap();
    assert_eq!(page.shown(at), "42");
    assert_eq!(cells().len(), 1);
    assert_eq!(cells()[0]["kind"], "formula");
    assert!(session.stamp().unwrap().model > empty.model);

    assert!(page.press("Enter", Focus::Stage));
    session.edit_text("=1+".into());
    assert!(page.press("Shift+Enter", Focus::Editor));
    assert_eq!(cells().len(), 1);
    assert!(session.edit().is_some());
    assert_eq!(page.shown(at), "42");
    session.edit_text("=6*7".into());
    assert!(page.press("Shift+Enter", Focus::Editor));
    page.settle();
    assert_eq!(page.shown(at), "42");
    assert_eq!(cells()[1]["kind"], "code-slot");

    assert!(page.press("K", Focus::Stage));
    assert!(page.press("Enter", Focus::Stage));
    session.edit_text("=6*8".into());
    assert!(page.press("Alt+Enter", Focus::Editor));
    page.settle();
    assert_eq!(page.shown(at), "48");
    assert_eq!(cells().len(), 3);
    assert_eq!(cells()[1]["kind"], "code-slot");
    assert_eq!(cells()[2]["kind"], "code-slot");
    assert!(page.press("A", Focus::Stage));
    assert_eq!(cells()[1]["kind"], "code-slot");
    assert_eq!(cells().len(), 4);
    assert!(page.press("M", Focus::Stage));
    assert_eq!(cells()[1]["kind"], "prose");
    assert!(page.press("Y", Focus::Stage));
    assert_eq!(cells()[1]["kind"], "code-slot");
}

/// Native-only: cut and paste the real Notebook declaration and keep its workbook ownership.
#[test]
fn notebook_cut_and_paste_keeps_the_owned_cell_and_its_name() {
    let page = Page::open();
    let session = page.session;
    session.command("view.notebook").unwrap();
    session.command("notebook.new-formula").unwrap();
    page.settle();
    let run = |command, args| {
        session.run(&dna_kit::Invocation { command: dna_kit::commands::CommandId::new(command), args })
            .unwrap();
        page.settle();
    };
    run("notebook.author", serde_json::json!("=6*7"));
    let at = session.active().unwrap();
    run("notebook.name-output", serde_json::json!("reportTotal"));
    assert_eq!(page.shown(at), "42");
    let part = session.watch(query::ViewPartOf);
    page.settle();
    let cells = || {
        let Live::Ready(answer) = session.read(part) else {
            panic!("Notebook declaration is unavailable");
        };
        answer.body.documents.into_iter()
            .find(|doc| doc.view.0 == dna_view_notebook::ID).unwrap()
            .body["cells"].as_array().unwrap().clone()
    };
    let before = session.stamp().unwrap();
    assert_eq!(cells()[0]["owned"], true);
    assert!(page.press("X", Focus::Stage));
    assert!(cells().is_empty());
    assert_eq!(page.shown(at), "42");
    assert_eq!(session.stamp().unwrap().model, before.model);
    assert!(page.press("V", Focus::Stage));
    assert_eq!(cells().len(), 1);
    assert_eq!(cells()[0]["owned"], true);
    assert_eq!(cells()[0]["output_name"], "reportTotal");
    assert_eq!(session.stamp().unwrap().model, before.model);
    session.command("notebook.delete").unwrap();
    page.settle();
    assert!(cells().is_empty());
    assert_eq!(page.shown(at), "");
    session.command("edit.undo").unwrap();
    page.settle();
    assert_eq!(page.shown(at), "42");
    assert_eq!(cells()[0]["output_name"], "reportTotal");
}

/// Native-only: the Alt+Enter command keeps multiline formula text in the draft.
#[test]
fn notebook_alt_enter_inserts_a_line_inside_a_multiline_formula() {
    let page = Page::open();
    let session = page.session;
    session.command("view.notebook").unwrap();
    session.command("notebook.new-formula").unwrap();
    page.settle();
    session.run(&dna_kit::Invocation {
        command: dna_kit::commands::CommandId::new("notebook.author"),
        args: serde_json::json!("=42"),
    }).unwrap();
    page.settle();
    let active = session.active().unwrap();
    let before = session.stamp().unwrap();
    assert!(page.press("Enter", Focus::Stage));
    let formula = "=LET(x,\n6,x*7)";
    session.edit_text(formula.into());
    assert!(page.press("Alt+Enter", Focus::Editor));
    assert_eq!(session.stamp(), Some(before));
    assert_eq!(session.edit().unwrap().text, format!("{formula}\n"));
    assert_eq!(page.shown(active), "42");
}

/// Native-only: F4 uses the shared formula-anchor cycle through the Notebook keymap.
#[test]
fn notebook_f4_cycles_the_current_formula_reference_without_running() {
    let page = Page::open();
    let session = page.session;
    session.command("view.notebook").unwrap();
    session.command("notebook.new-formula").unwrap();
    page.settle();
    session.run(&dna_kit::Invocation {
        command: dna_kit::commands::CommandId::new("notebook.author"),
        args: serde_json::json!("=42"),
    }).unwrap();
    page.settle();
    let before = session.stamp().unwrap();
    assert!(page.press("Enter", Focus::Stage));
    session.edit_text("=B2".into());
    for reference in ["=$B$2", "=B$2", "=$B2", "=B2"] {
        assert!(page.press("F4", Focus::Editor));
        assert_eq!(session.edit().unwrap().text, reference);
    }
    assert_eq!(session.stamp(), Some(before));
}

/// Native-only: tests inline controls through the Notebook command table and workbook history.
#[test]
fn notebook_control_updates_the_formula_with_one_live_workbook_edit() {
    use calc_skin::query::ViewPartOf;
    let page = Page::open();
    let session = page.session;
    let input = page.at(3, 2);
    let result = page.at(2, 10);
    let enter = |at, text: &str| {
        session.select_cell(at);
        page.settle();
        session.edit_text(text.into());
        session.commit_edit();
        page.settle();
    };
    enter(input, "Pending");
    enter(result, r#"=IF(B3="Pending",3,5)"#);
    assert_eq!(page.shown(result), "3");
    session.command("view.notebook").unwrap();
    let part = session.watch(ViewPartOf);
    page.settle();
    let before = session.stamp().unwrap();
    let run = |command, args| {
        session.run(&dna_kit::Invocation { command: dna_kit::commands::CommandId::new(command), args }).unwrap();
        page.settle();
    };
    run("notebook.add-control", serde_json::json!(["Status", "Sheet1!B3", {
        "type": "choice", "options": ["Pending", "Paid"],
    }]));
    let linked = session.stamp().unwrap();
    assert_eq!(before.model, linked.model);
    assert!(before.view < linked.view);
    let Live::Ready(saved) = session.read(part) else { panic!("Notebook block not saved"); };
    let anchor = saved.body.anchors[0].id;
    run("notebook.set-control", serde_json::json!([anchor, "Paid"]));
    assert_eq!(page.shown(input), "Paid");
    assert_eq!(page.shown(result), "5");
    assert_eq!(session.stamp().unwrap().view, linked.view);
    session.command("edit.undo").unwrap();
    page.settle();
    assert_eq!(page.shown(input), "Pending");
    assert_eq!(page.shown(result), "3");
}

/// Native-only: verifies the block inserter shortcut belongs to Notebook command mode.
#[test]
fn notebook_block_inserter_uses_the_notebook_command_map() {
    let page = Page::open();
    let session = page.session;
    session.command("view.notebook").unwrap();
    page.settle();
    let before = session.stamp().unwrap();
    assert!(page.press("/", Focus::Stage));
    assert_eq!(session.stamp(), Some(before));
    assert!(page.press("/", Focus::Stage));
    assert_eq!(session.stamp(), Some(before));
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

/// Native-only: saves and applies an Excel Scenario Manager record through the App commands.
#[test]
fn app_saves_native_scenario_reviews_replay_and_undoes_it_once() {
    use calc_skin::query::Query;
    use calc_skin::{InventoryItem, InventoryKind, WorkbookPort};
    use serde_json::{Value, json};

    let page = Page::open();
    let session = page.session;
    let run = |command, args| {
        session.run(&dna_kit::Invocation {
            command: dna_kit::commands::CommandId::new(command),
            args,
        }).unwrap();
        page.settle();
    };
    let set = |first: &str, second: &str| {
        page.host.with_session(SelectionCause::Open, |book| {
            for (row, text) in [(1, first), (2, second), (3, "=A1+A2")] {
                let sheet = book.host().sheets()[0].id();
                let at = calc_host::value::CellKey::new(sheet, row, 1).unwrap();
                let edits = book.prepare_cell_entry(at, text).unwrap();
                let receipt = book.dispatch(SkinIntent::Edit { edits });
                assert!(receipt.accepted(), "{:?}", receipt.error);
            }
        });
        page.settle();
    };
    set("2", "3");
    run("view.forms", Value::Null);
    run("forms.create", Value::Null);
    session.select_cell(page.at(1, 1));
    page.settle();
    run("forms.add-cell", json!("First input"));
    session.select_cell(page.at(2, 1));
    page.settle();
    run("forms.add-cell", json!("Second input"));
    run("forms.add-scenario-picker", Value::Null);
    run("view.app", Value::Null);
    let before = session.stamp().unwrap();
    run("app.save-scenario", json!("What & If"));
    let Seam::Review(preview) = session.seam() else {
        panic!("review the scenario before saving");
    };
    assert!(preview.writes.is_empty());
    assert!(matches!(
        preview.properties.as_slice(),
        [calc_skin::ModelPropertyDiff::AddedScenario { values, .. }] if values.len() == 2
    ));
    assert_eq!(session.stamp(), Some(before));
    run("edit.apply-review", Value::Null);
    let saved = page.host.with_session(SelectionCause::Open, |book| {
        let mut bytes = Vec::new();
        book.save(&calc_host::SavePolicy::default(), &mut bytes).unwrap();
        bytes
    });
    let doc = calc_doc::open(Arc::new(MemorySource::new(saved.clone())), &OpenPolicy::default()).unwrap();
    let xml = String::from_utf8(doc.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains("<scenario name=\"What &amp; If\""));
    assert!(xml.contains("r=\"A1\" val=\"2\""));
    assert!(xml.contains("r=\"A2\" val=\"3\""));
    assert!(page.press("Ctrl+z", Focus::Stage));
    let inventory = query::Inventory { kinds: vec![InventoryKind::Scenarios] };
    assert!(query::Inventory::decode(
        page.port.ask(inventory.request(), dna_skin_runtime::client::PAGE_BUDGET).unwrap()
    ).unwrap().body.is_empty());
    session.open("Scenarios.xlsx".into(), saved);
    page.settle();
    run("view.app", Value::Null);
    let answer = query::Inventory::decode(
        page.port.ask(inventory.request(), dna_skin_runtime::client::PAGE_BUDGET).unwrap()
    ).unwrap();
    let [InventoryItem::Scenario(scenario)] = answer.body.as_slice() else {
        panic!("expected the scenario loaded from the worksheet");
    };
    let at = |row| Key(scenario.sheet.0, scenario.sheet.1, row, 1);
    set("4", "5");
    assert_eq!(page.shown(at(3)), "9");
    run("app.review-scenario", json!(scenario));
    let Seam::Review(preview) = session.seam() else {
        panic!("review the changing cells before applying");
    };
    assert_eq!(preview.writes.len(), 2);
    assert_eq!(page.shown(at(3)), "9");
    run("edit.apply-review", Value::Null);
    assert_eq!(page.shown(at(3)), "5");
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert_eq!(page.shown(at(3)), "9");
    run("app.reset-inputs", Value::Null);
    assert_eq!(page.shown(at(1)), "2");
    assert_eq!(page.shown(at(2)), "3");
    assert_eq!(page.shown(at(3)), "5");
    assert!(page.press("Ctrl+z", Focus::Stage));
    assert_eq!(page.shown(at(3)), "9");
    set("4", "=5");
    let before = session.stamp().unwrap();
    run("app.review-scenario", json!(scenario));
    assert_eq!(session.seam(), Seam::Idle);
    assert_eq!(session.stamp(), Some(before));
    assert_eq!(page.shown(at(1)), "4");
}

/// Native-only: this exercises the App command table without a browser print carrier.
#[test]
fn app_print_key_opens_the_executive_summary_and_escape_closes_it() {
    let page = Page::open();
    let session = page.session;
    session.command("view.forms").unwrap();
    session.command("forms.create").unwrap();
    page.settle();
    session.command("view.app").unwrap();
    page.settle();
    let before = session.stamp();
    assert!(page.press("Ctrl+p", Focus::Stage));
    assert_eq!(
        session.command("app.print-report"),
        Err(CommandError::Refused("The browser print carrier is unavailable.".into()))
    );
    assert!(page.press("Escape", Focus::Stage));
    assert_eq!(
        session.command("app.print-report"),
        Err(CommandError::Refused("Open the executive summary first".into()))
    );
    assert_eq!(session.stamp(), before);
}

/// Native-only: compares a live formula and a bound line chart from the reopened workbook.
#[test]
fn app_pins_live_values_and_line_chart_without_editing_the_workbook() {
    use calc_skin::query::Query;
    use calc_skin::{SourceRef, WorkbookPort};
    use serde_json::{Value, json};

    let page = Page::open();
    let session = page.session;
    session.open(
        "Charts.xlsx".into(),
        include_bytes!("../../../crates/calc-host/tests/fixtures/dc_6yw/charts.xlsx").to_vec(),
    );
    page.settle();
    let sheet = session.summary().unwrap().sheets[0].sheet;
    let total = Key(sheet.0, sheet.1, 2, 4);
    let run = |command, args| {
        session.run(&dna_kit::Invocation {
            command: dna_kit::commands::CommandId::new(command),
            args,
        }).unwrap();
        page.settle();
    };
    run("calc.calculate", Value::Null);
    assert_eq!(page.shown(total), "20");
    run("view.forms", Value::Null);
    run("forms.create", Value::Null);
    session.select_cell(total);
    page.settle();
    run("forms.add-cell", json!("Total"));
    run("forms.add-chart", json!(SourceRef::Object { sheet, name: "Chart 3".into() }));
    run("view.app", Value::Null);
    let stamp = session.stamp();
    run("app.pin-baseline", Value::Null);
    assert_eq!(session.stamp(), stamp);
    let Live::Ready(part) = session.view_part() else { panic!("expected the App declaration"); };
    let document: calc_skin::Page = serde_json::from_value(part.body.documents[0].body.clone()).unwrap();
    let mut bindings = std::collections::BTreeMap::new();
    document.root.for_each_part(&mut |_, component, bind| {
        if let Some(bound) = bind.values().next() {
            bindings.insert(component.to_owned(), bound.clone());
        }
    });
    let numeric = query::NumericComparisonOf { bound: bindings["value"].clone() };
    let chart = |sheet| query::ChartImageOf {
        object: SourceRef::Object { sheet, name: "Chart 3".into() },
        comparison: Some(bindings["chart"].clone()),
    };
    let ask = |query| page.port.ask(query, dna_skin_runtime::client::PAGE_BUDGET).unwrap();
    let compared = || query::NumericComparisonOf::decode(ask(numeric.request())).unwrap().body;
    let first = compared().expect("pin the calculated total");
    assert_eq!((first.baseline, first.current), (20f64.to_bits(), 20f64.to_bits()));
    page.host.with_session(SelectionCause::Open, |book| {
        let at = calc_host::value::CellKey::new(sheet.id(), 2, 2).unwrap();
        assert!(book.dispatch(SkinIntent::EnterCell { at, text: "30".into() }).accepted());
    });
    page.settle();
    let current = compared().expect("compare against the pinned publication");
    assert_eq!((current.baseline, current.current), (20f64.to_bits(), 38f64.to_bits()));
    let overlay = query::ChartImageOf::decode(ask(chart(sheet).request())).unwrap().body;
    assert!(overlay.compares_baseline && !overlay.uses_saved_values);
    let saved = page.host.with_session(SelectionCause::Open, |book| {
        let mut bytes = Vec::new();
        book.save(&calc_host::SavePolicy::default(), &mut bytes).unwrap();
        bytes
    });
    session.open("Charts.xlsx".into(), saved);
    page.settle();
    assert_eq!(compared(), None);
    let reopened_sheet = session.summary().unwrap().sheets[0].sheet;
    assert!(!query::ChartImageOf::decode(ask(chart(reopened_sheet).request())).unwrap().body.compares_baseline);
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

/// Native-only: picks worksheet sources served by the real workbook port.
#[test]
fn forms_maker_binds_a_table_candidate_as_a_read_only_frame() {
    use calc_skin::query::Query;
    use calc_skin::{Evidence, PageNode, SourceRef, WorkbookPort};
    let page = Page::open();
    let session = page.session;
    session.open(
        "Tables.xlsx".into(),
        include_bytes!("../../../crates/calc-host/tests/fixtures/pivot/m5-structure/base-table.xlsx")
            .to_vec(),
    );
    page.settle();
    let sheet = session.summary().unwrap().sheets[0].sheet;
    let structure = query::StructureOf { scope: SourceRef::Sheet(sheet) };
    let answer = page.port.ask(structure.request(), dna_skin_runtime::client::PAGE_BUDGET).unwrap();
    let Evidence::Derived { body, .. } = query::StructureOf::decode(answer).unwrap().body else {
        panic!("worksheet structure was unavailable");
    };
    let table = body.tables.iter().find(|table| table.name == "TblSrc").unwrap();
    let model = session.stamp().unwrap().model;
    session.command("view.forms").unwrap();
    session.command("forms.create").unwrap();
    page.settle();
    session.run(&dna_kit::Invocation {
        command: dna_kit::commands::CommandId::new("forms.add-table"),
        args: serde_json::to_value(table).unwrap(),
    }).unwrap();
    page.settle();
    let Live::Ready(part) = session.view_part() else { panic!("missing view part"); };
    let doc: calc_skin::Page = serde_json::from_value(part.body.documents[0].body.clone()).unwrap();
    let PageNode::Stack { children } = doc.root else { panic!("expected a page stack"); };
    let [PageNode::Part { component, bind, .. }] = children.as_slice() else { panic!("expected a table control"); };
    assert_eq!(component, "data-frame");
    assert_eq!(bind["rows"].address, "Src!B2:E14");
    assert_eq!(part.body.anchors[0].area, Some(table.area));
    assert_eq!(session.stamp().unwrap().model, model);
}

/// Native-only: checks the candidate rail's terminal output query against the live graph.
#[test]
fn forms_maker_offers_only_formulas_with_no_readers_as_outputs() {
    use calc_skin::query::Query;
    use calc_skin::{SourceRef, WorkbookPort};

    let page = Page::open();
    let session = page.session;
    for (column, entry) in [(1, "=1"), (2, "=A1+1"), (3, "=B1*2")] {
        let at = page.at(1, column);
        session.select_cell(at);
        page.settle();
        session.begin_edit(at, Some(entry.into()));
        assert!(page.press("Enter", Focus::Editor));
    }
    let query = query::OutputCandidates { scope: SourceRef::Sheet(page.sheet) };
    let answer = page.port.ask(query.request(), dna_skin_runtime::client::PAGE_BUDGET).unwrap();
    assert_eq!(query::OutputCandidates::decode(answer).unwrap().body, [page.at(1, 3)]);
    session.command("view.forms").unwrap();
    session.command("forms.toggle-picker").unwrap();
    session.run(&dna_kit::Invocation {
        command: dna_kit::commands::CommandId::new("forms.pick-cell"),
        args: serde_json::to_value(page.at(1, 3)).unwrap(),
    }).unwrap();
    page.settle();
    assert_eq!(session.active(), Some(page.at(1, 3)));
    assert!(session.edit().is_none());
}
/// Native-only: prepares a reader page and observes its live value through the real workbook port.
#[test]
fn forms_maker_authors_reader_prose_bound_to_the_selected_cell() {
    use calc_skin::query::Query;
    use calc_skin::{PageNode, RawValue, RowWindow, WorkbookPort};
    let page = Page::open();
    let session = page.session;
    let at = page.at(1, 1);
    session.begin_edit(at, Some("=2+5".into()));
    assert!(page.press("Enter", Focus::Editor));
    session.select_cell(at);
    page.settle();
    let model = session.stamp().unwrap().model;
    session.command("view.forms").unwrap();
    session.command("forms.create").unwrap();
    page.settle();
    session
        .run(&dna_kit::Invocation {
            command: dna_kit::commands::CommandId::new("forms.add-prose"),
            args: serde_json::json!({"text": "Calculated amount {=value}", "bind_selected": true}),
        })
        .unwrap();
    page.settle();
    let Live::Ready(part) = session.view_part() else {
        panic!("missing view part");
    };
    let document: calc_skin::Page =
        serde_json::from_value(part.body.documents[0].body.clone()).unwrap();
    let PageNode::Stack { children } = document.root else {
        panic!("expected a page");
    };
    let [
        PageNode::Part {
            component,
            bind,
            props,
            ..
        },
    ] = children.as_slice()
    else {
        panic!("expected reader prose");
    };
    assert_eq!(component, "rich-prose");
    assert_eq!(props["text"], "Calculated amount {=value}");
    assert_eq!(bind["value"].address, "Sheet1!A1");
    let source = query::ResolveBound {
        bound: bind["value"].clone(),
    };
    let ask = |query| {
        page.port
            .ask(query, dna_skin_runtime::client::PAGE_BUDGET)
            .unwrap()
    };
    let resolved = query::ResolveBound::decode(ask(source.request()))
        .unwrap()
        .body;
    let rows = query::Rows {
        source: resolved,
        filter: Vec::new(),
        sort: Vec::new(),
        window: RowWindow {
            offset: 0,
            limit: 1,
        },
    };
    let shown = query::Rows::decode(ask(rows.request())).unwrap().body;
    assert_eq!(shown.rows[0].cells[0].text, "7");
    assert!(matches!(
        shown.rows[0].cells[0].shown.value,
        RawValue::Number { .. }
    ));
    assert!(matches!(
        shown.rows[0].cells[0].shown.provenance,
        calc_skin::Provenance::Calculated { .. }
    ));
    assert_eq!(session.stamp().unwrap().model, model);
}
/// Native-only: binds an existing Excel chart and reopens the authored App page.
#[test]
fn forms_maker_reopens_a_bound_workbook_chart_and_reads_its_image() {
    use calc_skin::query::Query;
    use calc_skin::{InventoryItem, InventoryKind, PageNode, SourceRef, WorkbookPort};

    let page = Page::open();
    let session = page.session;
    session.open(
        "Charts.xlsx".into(),
        include_bytes!(
            "../../../crates/calc-doc/tests/fixtures/preserved_sheets/chart-sheet-and-embedded.xlsx"
        )
        .to_vec(),
    );
    page.settle();
    let ask = |query| {
        page.port
            .ask(query, dna_skin_runtime::client::PAGE_BUDGET)
            .unwrap()
    };
    let inventory = query::Inventory {
        kinds: vec![InventoryKind::Charts],
    };
    let worksheet = session
        .summary()
        .unwrap()
        .sheets
        .into_iter()
        .find(|sheet| sheet.worksheet)
        .unwrap();
    let chart = query::Inventory::decode(ask(inventory.request()))
        .unwrap()
        .body
        .into_iter()
        .find_map(|item| match item {
            InventoryItem::Chart {
                object: source @ SourceRef::Object { sheet, .. },
            } if sheet == worksheet.sheet => Some(source),
            _ => None,
        })
        .expect("the worksheet has an embedded chart");
    session.command("view.forms").unwrap();
    session.command("forms.create").unwrap();
    page.settle();
    session
        .run(&dna_kit::Invocation {
            command: dna_kit::commands::CommandId::new("forms.add-chart"),
            args: serde_json::to_value(chart).unwrap(),
        })
        .unwrap();
    page.settle();
    let model = session.stamp().unwrap().model;
    let bound = || {
        let Live::Ready(part) = session.view_part() else {
            panic!("missing view part");
        };
        let page: calc_skin::Page =
            serde_json::from_value(part.body.documents[0].body.clone()).unwrap();
        let PageNode::Stack { children } = page.root else {
            panic!("expected the app page");
        };
        let [
            PageNode::Part {
                component, bind, ..
            },
        ] = children.as_slice()
        else {
            panic!("expected one chart control");
        };
        assert_eq!(component, "chart");
        bind["object"].clone()
    };
    let binding = bound();
    let bytes = page
        .host
        .with_session(calc_skin::SelectionCause::Open, |workbook| {
            let mut bytes = Vec::new();
            workbook
                .save(&calc_host::SavePolicy::default(), &mut bytes)
                .unwrap();
            bytes
        });
    assert_eq!(session.stamp().unwrap().model, model);
    session.open("Charts.xlsx".into(), bytes);
    page.settle();
    assert_eq!(bound(), binding);
    let source = query::ResolveBound { bound: binding.clone() };
    let resolved = query::ResolveBound::decode(ask(source.request()))
        .unwrap()
        .body;
    let SourceRef::Object { sheet, name } = &resolved else {
        panic!("expected the bound chart");
    };
    let sheet_name = session.summary().unwrap().sheets.into_iter()
        .find(|entry| entry.sheet == *sheet).unwrap().name;
    let alias = query::ResolveBound {
        bound: calc_skin::Bound {
            address: format!("obj:{}/{name}", calc_skin::address::quoted_sheet(&sheet_name)),
            ..binding
        },
    };
    assert_eq!(query::ResolveBound::decode(ask(alias.request())).unwrap().body, resolved);
    let drawing = query::ChartImageOf { object: resolved, comparison: None };
    let chart = query::ChartImageOf::decode(ask(drawing.request())).unwrap().body;
    assert!(chart.data_url.starts_with("data:image/svg+xml;base64,PHN2Zy"));
    assert!(!chart.label.is_empty());
}
/// Native-only: drives App clock commands through the workbook connection.
#[test]
fn app_clock_step_controls_a_workbook_source_and_keeps_its_formula() {
    use calc_skin::{
        AnchorId, Batch, Bound, Change, DocId, ModelChange, Page as Declaration, PageNode,
        PresentationChange, SourceRef, WorkbookPort, grid::Area,
    };
    use std::num::NonZeroU128;

    let page = Page::open();
    page.host.enable_model_clock();
    let session = page.session;
    let at = |col| page.at(1, col);
    let formula = r#"=RTD("DnaCalc.Clock",,"stock","tick",B1,C1,D1,E1)"#;
    let mut changes: Vec<_> = [formula, "2", "FALSE", "0", "0", "=A1>=3"]
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            Change::Model(ModelChange::Enter {
                at: at(index as u32 + 1),
                text: text.into(),
            })
        })
        .collect();
    let mut bind = std::collections::BTreeMap::new();
    for (index, slot) in ["tick", "rate", "running", "step", "reset", "stopped"]
        .into_iter()
        .enumerate()
    {
        let anchor = AnchorId(NonZeroU128::new(index as u128 + 1).unwrap());
        let key = at(index as u32 + 1);
        bind.insert(
            slot.into(),
            Bound {
                anchor,
                address: format!("Sheet1!{}1", char::from(b'A' + index as u8)),
            },
        );
        changes.push(Change::Presentation(PresentationChange::Bind {
            anchor,
            source: SourceRef::Cells {
                sheet: page.sheet,
                areas: vec![Area(key, key)],
            },
        }));
    }
    let body = Declaration {
        schema: calc_skin::PageSchema::V1,
        title: "Clock controls".into(),
        input: calc_skin::InputPolicy::Live,
        root: PageNode::Part {
            id: "clock".into(),
            component: "clock".into(),
            bind,
            props: Default::default(),
        },
    };
    let doc_id = DocId(NonZeroU128::new(12).unwrap());
    changes.push(Change::Presentation(PresentationChange::CreateDoc {
        doc: doc_id,
        view: ViewId("app".into()),
        sheet: None,
        schema: 1,
        expect_view_rev: Some(session.stamp().unwrap().view),
        body: serde_json::to_value(body).unwrap(),
    }));
    let author = page.host.connect(
        Origin::User {
            view: ViewId("classic".into()),
        },
        WritePolicy::Commit,
    );
    author
        .commit(Batch {
            id: "500".into(),
            workbook: None,
            base: None,
            expects: vec![],
            selection_seq: None,
            label: None,
            changes,
        })
        .unwrap();
    page.settle();
    session.command("view.app").unwrap();
    page.settle();
    session
        .run(&dna_kit::Invocation {
            command: dna_kit::commands::CommandId::new("clock.control"),
            args: serde_json::json!([doc_id, "clock", "Step", null]),
        })
        .unwrap();
    page.settle();
    assert_eq!(page.shown(at(1)), "1", "{}", session.notice());
    assert_eq!(page.shown(at(3)), "FALSE");
    assert_eq!(page.shown(at(4)), "1");
    let formula_query = query::Formula { at: at(1) };
    use calc_skin::query::Query;
    let answer = page
        .port
        .ask(formula_query.request(), dna_kit::PAGE_BUDGET)
        .unwrap();
    assert_eq!(query::Formula::decode(answer).unwrap().body.input, formula);
    session.command("view.inspector").unwrap();
    page.settle();
    session
        .run(&dna_kit::Invocation {
            command: dna_kit::commands::CommandId::new("clock.control"),
            args: serde_json::json!([doc_id, "clock", "Reset", null]),
        })
        .unwrap();
    page.settle();
    assert_eq!(page.shown(at(1)), "0");
    assert_eq!(page.shown(at(5)), "1");
}
/// Native-only: exercises the live shell's one keymap and workbook session.
#[test]
fn trying_on_a_keymap_is_reversible_and_preserves_workbook_stamps() {
    let page = Page::open();
    let session = page.session;
    let before = session.stamp();
    let manifest = serde_json::json!({
        "schema": "dna.skin/1",
        "id": "calm-dark-audit",
        "name": "Calm Dark Audit",
        "extends": "dna:default",
        "theme": {
            "base": "cockpit-dark",
            "density": "working",
            "overrides": {"formula.font.size": "15.5px"}
        },
        "commands": [{
            "id": "skin.show-inspector",
            "label": "Show Inspector",
            "steps": [["view.inspector", null]]
        }, {
            "id": "skin.edit",
            "label": "Edit the active cell",
            "steps": [["grid.edit-cell", null]]
        }],
        "keymap": {
            "profile": "excel-365-win-en-us",
            "bindings": [{
                "when": "view:classic && mode:ready",
                "chord": "Ctrl+Shift+E",
                "command": "grid.edit-cell"
            }]
        }
    });
    session
        .run(&dna_kit::Invocation {
            command: dna_kit::commands::CommandId::new("studio.try-on"),
            args: manifest.clone(),
        })
        .unwrap();
    page.settle();
    assert_eq!(session.stamp(), before);
    assert_eq!(session.view(), ViewId("classic".into()));
    session.command("skin.show-inspector").unwrap();
    assert_eq!(session.view(), ViewId("inspector".into()));
    assert!(session.command("skin.edit").is_err());
    assert!(session.edit().is_none());
    session.command("view.classic").unwrap();
    page.settle();
    let mut collision = manifest;
    collision["keymap"]["bindings"][0]["chord"] = serde_json::json!("Ctrl+E");
    assert!(session.run(&dna_kit::Invocation {
        command: dna_kit::commands::CommandId::new("studio.try-on"),
        args: collision,
    }).is_err());
    session.command("studio.close").unwrap();
    assert!(!page.press("F2", Focus::Stage));
    assert!(page.press("Ctrl+Shift+e", Focus::Stage));
    assert!(session.edit().is_some());
    assert!(page.press("Escape", Focus::Editor));
    assert!(session.edit().is_none());
    assert!(page.press("Escape", Focus::Stage));
    assert!(session.command("skin.show-inspector").is_err());
    assert!(!page.press("Ctrl+Shift+e", Focus::Stage));
    assert!(page.press("F2", Focus::Stage));
    assert!(session.edit().is_some());
    assert_eq!(session.stamp(), before);
}
