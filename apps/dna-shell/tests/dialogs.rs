//! Classic's dialogs against the real workbook service: the open command, the dialog's fields
//! and keys, OK, and what the workbook holds afterwards.
#![cfg(not(target_arch = "wasm32"))]

use calc_skin::grid::Area;
use calc_skin::query::Query;
use calc_skin::{
    BorderLine, EdgeStroke, FillPattern, FormatColor, HorizontalAlignment, SourceRef, WorkbookPort, query,
};
use dna_kit::dialog::{self, Focus as DialogFocus, KeyAction, Value};
use dna_kit::{Focus, KeyInput};

mod page;
use page::Page;

fn enter(page: &Page, row: u32, column: u32, text: &str) {
    let at = page.at(row, column);
    page.session.select_cell(at);
    page.settle();
    page.session.begin_edit(at, Some(text.into()));
    page.session.commit_edit();
    page.settle();
}

fn select(page: &Page, first: (u32, u32), last: (u32, u32)) -> Area {
    let area = Area(page.at(first.0, first.1), page.at(last.0, last.1));
    let mut selection = page.session.selection().expect("a selection");
    selection.target = SourceRef::Cells { sheet: page.sheet, areas: vec![area] };
    selection.active = Some(area.0);
    page.session.select(selection);
    page.settle();
    area
}

fn format_of(page: &Page, area: Area) -> calc_skin::CellFormatFacts {
    let request = query::CellFormatFor { areas: vec![area], active: area.0 };
    query::CellFormatFor::decode(page.port.ask(request.request(), dna_kit::PAGE_BUDGET).unwrap()).unwrap().body
}

fn key(key: &str, alt: bool, ctrl: bool) -> KeyInput {
    KeyInput { key: key.into(), ctrl, alt, shift: false, meta: false, alt_graph: false, composing: false }
}

fn set(page: &Page, id: &'static str, value: Value) {
    dialog::change(page.session, id, value);
}

/// Native-only: links the workbook service to drive Format Cells over the shell's packet carrier.
#[test]
fn format_cells_opens_on_the_selection_and_commits_its_changed_fields_in_one_undo_step() {
    let page = Page::open();
    let session = page.session;
    enter(&page, 2, 2, "1234.5");
    enter(&page, 3, 2, "=B2*2");
    let area = select(&page, (2, 2), (3, 3));

    // Ctrl+1 opens the dialog on the selection, read from the workbook.
    assert!(page.press("Ctrl+1", Focus::Stage));
    assert_eq!(dialog::open(session), Some("format-cells"));
    dialog::poll(session);
    assert!(dialog::is_ready(session));
    let opened = dialog::state(session).unwrap();
    assert_eq!(opened.spec.tabs[opened.tab].id, "number");
    assert_eq!(opened.choice("number.category"), Some("General"));
    assert_eq!(opened.text("font.name"), "Calibri");
    assert_eq!(dna_shell::workbook::probe(session)["dialog"]["id"], "format-cells");
    assert_eq!(dna_shell::workbook::probe(session)["dialog"]["ready"], true);

    // Number: a category and its options make the code.
    set(&page, "number.category", Value::Choice(Some("Number".into())));
    set(&page, "number.separator", Value::Flag(Some(true)));
    // Ctrl+Page Down walks the tabs; access keys act on the tab shown.
    assert_eq!(dialog::key(session, &key("PageDown", false, true), DialogFocus::Frame), KeyAction::Show(1));
    assert_eq!(dialog::key(session, &key("w", true, false), DialogFocus::Frame), KeyAction::Switch("align.wrap"));
    set(&page, "align.horizontal", Value::Choice(Some("center".into())));
    dialog::show(session, "font");
    set(&page, "font.style", Value::Text("Bold".into()));
    set(&page, "font.color", Value::Choice(Some("C00000".into())));
    dialog::show(session, "border");
    assert_eq!(dialog::key(session, &key("o", true, false), DialogFocus::Frame), KeyAction::Press("border.preset-outline"));
    dialog::show(session, "fill");
    set(&page, "fill.color", Value::Choice(Some("FFFF00".into())));
    dialog::show(session, "protection");
    assert_eq!(dialog::key(session, &key("l", false, false), DialogFocus::Frame), KeyAction::Switch("protect.locked"));

    let before = session.stamp().unwrap();
    // Enter is OK.
    assert_eq!(dialog::key(session, &key("Enter", false, false), DialogFocus::Frame), KeyAction::Accept);
    page.settle();
    assert_eq!(dialog::open(session), None);
    let after = session.stamp().unwrap();
    assert_eq!(after.model.0, before.model.0 + 1, "one workbook edit");

    let facts = format_of(&page, area);
    let common = facts.common;
    assert_eq!(common.number_format.as_deref(), Some("#,##0.00"));
    assert_eq!(facts.sample_text, "1234.5", "the session opens without a locale, so the grid shows General text");
    assert_eq!(common.horizontal, Some(HorizontalAlignment::Center));
    assert_eq!((common.wrap, common.bold, common.italic), (Some(true), Some(true), Some(false)));
    assert_eq!(common.font_color, Some(FormatColor::Rgb([0xC0, 0, 0])));
    let thin = EdgeStroke { line: BorderLine::Thin, color: FormatColor::Automatic };
    assert_eq!((common.borders.left, common.borders.top), (Some(thin), Some(thin)));
    assert_eq!(common.borders.inside_horizontal, Some(EdgeStroke::NONE));
    assert_eq!((common.fill_color, common.pattern), (Some(FormatColor::Rgb([0xFF, 0xFF, 0])), Some(FillPattern::Solid)));
    assert_eq!(common.locked, Some(false));
    assert_eq!(page.shown(page.at(3, 2)), "2469", "the formula still calculates");

    // The dialog reopens on the tab it closed on, as Excel's does, showing what was applied.
    session.command(dna_kit::dialogs::format_cells::OPEN).unwrap();
    page.settle();
    dialog::poll(session);
    let reopened = dialog::state(session).unwrap();
    assert_eq!(reopened.spec.tabs[reopened.tab].id, "protection");
    assert_eq!(reopened.choice("number.category"), Some("Number"));
    assert_eq!(reopened.flag("number.separator"), Some(true));
    assert_eq!(reopened.text("font.style"), "Bold");
    assert_eq!(reopened.choice("border.left"), Some("thin:auto"));
    // Escape cancels and changes nothing.
    set(&page, "protect.hidden", Value::Flag(Some(true)));
    assert_eq!(dialog::key(session, &key("Escape", false, false), DialogFocus::Frame), KeyAction::Cancel);
    page.settle();
    assert_eq!(dialog::open(session), None);
    assert_eq!(session.stamp().unwrap().model, after.model);

    // One Undo takes the whole dialog back.
    session.command("edit.undo").unwrap();
    page.settle();
    let restored = format_of(&page, area).common;
    assert_eq!(restored.number_format.as_deref(), Some("General"));
    assert_eq!((restored.bold, restored.wrap, restored.locked), (Some(false), Some(false), Some(true)));
    assert_eq!(restored.borders.left, Some(EdgeStroke::NONE));
    assert_eq!(restored.pattern, Some(FillPattern::None));
}

/// Native-only: links the workbook service to drive Format Cells over the shell's packet carrier.
#[test]
fn format_cells_keeps_an_entry_it_cannot_accept_and_refuses_a_changed_workbook() {
    let page = Page::open();
    let session = page.session;
    let area = select(&page, (1, 1), (1, 1));
    session.command(dna_kit::dialogs::format_cells::OPEN_FONT).unwrap();
    page.settle();
    dialog::poll(session);
    let opened = dialog::state(session).unwrap();
    assert_eq!(opened.spec.tabs[opened.tab].id, "font", "Ctrl+Shift+F opens the Font tab");
    set(&page, "font.size", Value::Text("500".into()));
    assert!(dialog::accept(session).is_err());
    assert_eq!(dialog::open(session), Some("format-cells"), "the dialog stays open on a bad entry");
    assert_eq!(
        dialog::state(session).unwrap().message.as_deref(),
        Some("Font size must be between 1 and 409 points.")
    );
    set(&page, "font.size", Value::Text("14".into()));
    assert_eq!(dialog::state(session).unwrap().message, None);

    // Another writer changes the workbook under the open dialog.
    let writer = page.host.connect(calc_skin::Origin::Automation, calc_skin::WritePolicy::Commit);
    let mut other = calc_skin::Batch {
        id: "1".into(),
        workbook: None,
        base: None,
        expects: Vec::new(),
        selection_seq: None,
        label: None,
        changes: vec![calc_skin::Change::Model(calc_skin::ModelChange::Enter { at: page.at(5, 5), text: "7".into() })],
    };
    other.workbook = session.stamp().map(|stamp| stamp.workbook);
    writer.commit(other).unwrap();
    page.settle();
    assert!(dialog::accept(session).is_err());
    assert_eq!(
        dialog::state(session).unwrap().message.as_deref(),
        Some("The workbook changed. Reopen Format Cells.")
    );
    assert!(dialog::close(session));
    assert_eq!(format_of(&page, area).common.font_size.as_deref(), Some("11"));
    // With nothing open, OK and the dialog keys do nothing.
    assert!(dialog::accept(session).is_err());
    assert_eq!(dialog::key(session, &key("Enter", false, false), DialogFocus::Frame), KeyAction::Keep);
    assert!(dna_shell::workbook::probe(session)["dialog"].is_null());
}

fn geometry(page: &Page) -> calc_skin::WorksheetGeometry {
    let request = query::Geometry { sheet: page.sheet };
    query::Geometry::decode(page.port.ask(request.request(), dna_kit::PAGE_BUDGET).unwrap()).unwrap().body
}

fn open(page: &Page, command: &'static str) -> dna_kit::dialog::State {
    page.session.command(command).unwrap();
    page.settle();
    dialog::poll(page.session);
    assert!(dialog::is_ready(page.session), "{command} read the workbook");
    dialog::state(page.session).unwrap()
}

/// Native-only: links the workbook service to drive the dialogs over the shell's packet carrier.
#[test]
fn row_height_and_column_width_open_on_the_shared_size_and_commit_one_size() {
    let page = Page::open();
    let session = page.session;
    select(&page, (2, 2), (3, 3));
    let rows = open(&page, dna_kit::dialogs::axis_size::ROW_HEIGHT);
    assert_eq!(rows.spec.title, "Row Height");
    let default = rows.text("size").to_owned();
    assert!(default.parse::<f64>().is_ok_and(|points| points > 0.0), "the default row height: {default:?}");
    // OK without a change commits nothing.
    let before = session.stamp().unwrap();
    dialog::accept(session).unwrap();
    page.settle();
    assert_eq!(session.stamp().unwrap().model, before.model);

    open(&page, dna_kit::dialogs::axis_size::ROW_HEIGHT);
    set(&page, "size", Value::Text("30".into()));
    assert_eq!(dialog::key(session, &key("Enter", false, false), DialogFocus::Field("size")), KeyAction::Accept);
    page.settle();
    assert_eq!(geometry(&page).rows, vec![(2, Some(30.0), false), (3, Some(30.0), false)]);

    let columns = open(&page, dna_kit::dialogs::axis_size::COLUMN_WIDTH);
    assert_eq!(columns.text("size"), "8.43");
    set(&page, "size", Value::Text("300".into()));
    assert!(dialog::accept(session).is_err());
    assert_eq!(dialog::state(session).unwrap().message.as_deref(), Some("Column width must be between 0 and 255."));
    set(&page, "size", Value::Text("12".into()));
    dialog::accept(session).unwrap();
    page.settle();
    assert!(geometry(&page).columns.iter().any(|&(first, last, width, _)| first <= 2 && last >= 3 && width.is_some()));

    // A selection of rows with two heights opens on an empty box.
    select(&page, (1, 1), (2, 1));
    assert_eq!(open(&page, dna_kit::dialogs::axis_size::ROW_HEIGHT).text("size"), "");
    assert!(dialog::close(session));
    session.command("edit.undo").unwrap();
    page.settle();
    session.command("edit.undo").unwrap();
    page.settle();
    assert!(geometry(&page).rows.is_empty());
}

/// Native-only: links the workbook service to drive the dialogs over the shell's packet carrier.
#[test]
fn insert_and_delete_ask_how_to_shift_and_act_at_once_on_whole_rows() {
    let page = Page::open();
    let session = page.session;
    enter(&page, 1, 1, "1");
    enter(&page, 2, 1, "2");
    enter(&page, 1, 2, "10");
    enter(&page, 4, 3, "=A1+A2");

    // Ctrl+Shift+= on a cell opens Insert on Shift cells down.
    select(&page, (2, 1), (2, 1));
    assert!(page.press("Ctrl+Shift+=", Focus::Stage));
    dialog::poll(session);
    let insert = dialog::state(session).unwrap();
    assert_eq!((insert.spec.title, insert.choice("shift")), ("Insert", Some("down")));
    // The access key D is the option already chosen; I chooses Shift cells right.
    assert_eq!(dialog::key(session, &key("i", false, false), DialogFocus::Field("shift")), KeyAction::Choose("shift", "right".into()));
    assert_eq!(dialog::key(session, &key("d", false, false), DialogFocus::Field("shift")), KeyAction::Choose("shift", "down".into()));
    dialog::accept(session).unwrap();
    page.settle();
    assert_eq!((page.shown(page.at(2, 1)), page.shown(page.at(3, 1))), (String::new(), "2".into()));
    assert_eq!(page.shown(page.at(4, 3)), "3", "the formula follows the moved cell");

    // Ctrl+- opens Delete; Entire row removes row 2.
    assert!(page.press("Ctrl+-", Focus::Stage));
    dialog::poll(session);
    assert_eq!(dialog::state(session).unwrap().choice("shift"), Some("up"));
    set(&page, "shift", Value::Choice(Some("row".into())));
    dialog::accept(session).unwrap();
    page.settle();
    assert_eq!(page.shown(page.at(2, 1)), "2");
    assert_eq!(page.shown(page.at(3, 3)), "3", "the formula moved up with its row");

    // Whole rows selected: no dialog, the rows are inserted at once.
    let mut selection = session.selection().unwrap();
    let rows = Area(page.at(1, 1), page.at(1, 16_384));
    selection.target = SourceRef::Cells { sheet: page.sheet, areas: vec![rows] };
    selection.active = Some(rows.0);
    session.select(selection);
    page.settle();
    assert!(page.press("Ctrl+Shift+=", Focus::Stage));
    assert_eq!(dialog::open(session), None);
    assert_eq!((page.shown(page.at(1, 1)), page.shown(page.at(2, 1)), page.shown(page.at(2, 2))), (String::new(), "1".into(), "10".into()));
    // Three Undos return the sheet.
    for _ in 0..3 {
        session.command("edit.undo").unwrap();
        page.settle();
    }
    assert_eq!((page.shown(page.at(1, 1)), page.shown(page.at(2, 1)), page.shown(page.at(4, 3))), ("1".into(), "2".into(), "3".into()));
}
