//! Bounded spreadsheet viewport shared by workbook views.
//!
//! The grid is given the session and nothing else. It keeps its own watches: the cells of the
//! page that holds the active cell, and the geometry of that cell's sheet. It handles no key:
//! its motion and its edit keys are commands ([`commands`]) the keymap resolves.

use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::metrics::{self, SheetBands};
use calc_skin::{Batch, Change, Interest, ModelChange, Provenance, RawValue, Selection, SourceRef, address, query};
use dna_kit::{InputMode, Live, Session, ViewCommand};
use leptos::prelude::*;
use std::collections::BTreeMap;

const PAGE_ROWS: u32 = 36;
const PAGE_COLUMNS: u32 = 18;
const LAST_ROW: u32 = 1_048_576;
const LAST_COLUMN: u32 = 16_384;

/// The page of the sheet that holds `at`: the area the grid watches and draws.
pub fn page_of(at: Key) -> Area {
    let Key(sheet, generation, row, column) = at;
    let row = ((row.max(1) - 1) / PAGE_ROWS) * PAGE_ROWS + 1;
    let column = ((column.max(1) - 1) / PAGE_COLUMNS) * PAGE_COLUMNS + 1;
    Area(
        Key(sheet, generation, row, column),
        Key(
            sheet,
            generation,
            (row + PAGE_ROWS - 1).min(LAST_ROW),
            (column + PAGE_COLUMNS - 1).min(LAST_COLUMN),
        ),
    )
}

/// One step from `at`, staying on the sheet.
fn step(at: Key, rows: i64, columns: i64) -> Key {
    let Key(sheet, generation, row, column) = at;
    let row = (i64::from(row) + rows).clamp(1, i64::from(LAST_ROW)) as u32;
    let column = (i64::from(column) + columns).clamp(1, i64::from(LAST_COLUMN)) as u32;
    Key(sheet, generation, row, column)
}

fn rectangle(anchor: Key, to: Key) -> Area {
    Area(
        Key(anchor.0, anchor.1, anchor.2.min(to.2), anchor.3.min(to.3)),
        Key(anchor.0, anchor.1, anchor.2.max(to.2), anchor.3.max(to.3)),
    )
}

fn selected_areas(session: Session) -> Vec<Area> {
    match session.selection().map(|selection| selection.target) {
        Some(SourceRef::Cells { areas, .. }) => areas,
        _ => Vec::new(),
    }
}

fn select_range(session: Session, anchor: Key, to: Key) {
    session.select(Selection {
        seq: 0,
        target: SourceRef::Cells {
            sheet: Sheet(anchor.0, anchor.1),
            areas: vec![rectangle(anchor, to)],
        },
        active: Some(anchor),
        grouped: Vec::new(),
        item: None,
    });
}

fn far_corner(anchor: Key, areas: &[Area]) -> Key {
    if let [Area(first, last)] = areas {
        let row = if anchor.2 == first.2 { last.2 } else { first.2 };
        let column = if anchor.3 == first.3 { last.3 } else { first.3 };
        Key(anchor.0, anchor.1, row, column)
    } else {
        anchor
    }
}

#[derive(Clone, Copy)]
enum Walk {
    Down,
    Up,
    Right,
    Left,
}

#[derive(Clone, Copy)]
struct TabRun {
    workbook: calc_skin::WorkbookId,
    current: Key,
    start_column: u32,
}

impl Walk {
    fn step(self, from: Key) -> Key {
        match self {
            Self::Down => step(from, 1, 0),
            Self::Up => step(from, -1, 0),
            Self::Right => step(from, 0, 1),
            Self::Left => step(from, 0, -1),
        }
    }

    fn in_selection(self, selection: &mut Selection) -> bool {
        let (Some(from), SourceRef::Cells { areas, .. }) = (selection.active, &selection.target) else {
            return false;
        };
        if matches!(areas.as_slice(), [Area(first, last)] if first == last) {
            return false;
        }
        let Some((index, &Area(first, last))) = areas.iter().enumerate().find(|(_, area)| {
            area.area().zip(from.cell()).is_some_and(|(area, cell)| area.contains(cell))
        }) else {
            return false;
        };
        let Key(sheet, generation, row, column) = from;
        selection.active = Some(match self {
            Self::Down if row < last.2 => self.step(from),
            Self::Down if column < last.3 => Key(sheet, generation, first.2, column + 1),
            Self::Up if row > first.2 => self.step(from),
            Self::Up if column > first.3 => Key(sheet, generation, last.2, column - 1),
            Self::Right if column < last.3 => self.step(from),
            Self::Right if row < last.2 => Key(sheet, generation, row + 1, first.3),
            Self::Left if column > first.3 => self.step(from),
            Self::Left if row > first.2 => Key(sheet, generation, row - 1, last.3),
            Self::Down | Self::Right => areas[(index + 1) % areas.len()].0,
            Self::Up | Self::Left => areas[(index + areas.len() - 1) % areas.len()].1,
        });
        true
    }

    fn navigate(self, session: Session, tab_run: StoredValue<Option<TabRun>>) {
        if let Some(mut selection) = session.selection() {
            if self.in_selection(&mut selection) {
                tab_run.set_value(None);
                session.select(selection);
            } else if let (Some(at), Some(stamp)) = (selection.active, session.stamp()) {
                let workbook = stamp.workbook;
                let run = tab_run.get_value().filter(|run| (run.workbook, run.current) == (workbook, at));
                let start_column = run.map_or(at.3, |run| run.start_column);
                let mut to = self.step(at);
                match self {
                    Self::Right | Self::Left => {
                        tab_run.set_value(Some(TabRun { workbook, current: to, start_column }));
                    }
                    Self::Down | Self::Up => {
                        tab_run.set_value(None);
                        to.3 = start_column;
                    }
                }
                session.select_cell(to);
            }
        }
    }
}

/// The grid's commands for `view`, with the keys of the Excel profile. A view that mounts the
/// grid returns these from `View::commands`.
pub fn commands(view: &calc_skin::ViewId, session: Session) -> Vec<ViewCommand> {
    let tab_run = StoredValue::new(None);
    let moves = |id, label, chords: &[&'static str], rows: i64, columns: i64| {
        ViewCommand::new(view, id, label, chords, move |_| {
            let at = session.active().ok_or("No cell is selected.")?;
            tab_run.set_value(None);
            session.select_cell(step(at, rows, columns));
            Ok(())
        })
        .when("mode:ready")
    };
    let extend = |id, label, chord, rows, columns| {
        ViewCommand::new(view, id, label, &[chord], move |_| {
            let anchor = session.active().ok_or("No cell is selected.")?;
            let to = far_corner(anchor, &selected_areas(session));
            tab_run.set_value(None);
            select_range(session, anchor, step(to, rows, columns));
            Ok(())
        })
        .when("mode:ready")
    };
    let walk = |id, label, chord, direction: Walk| {
        ViewCommand::new(view, id, label, &[chord], move |_| {
            if session.mode().editing() {
                let selection = session.selection();
                let view = session.view();
                session.commit_edit_then(move || {
                    if session.selection() == selection && session.view() == view {
                        direction.navigate(session, tab_run);
                    }
                });
            } else {
                direction.navigate(session, tab_run);
            }
            Ok(())
        })
        .when("!mode:key-tips && !mode:object")
        .in_editor()
    };
    vec![
        moves("grid.move-down", "Move down", &["ArrowDown"], 1, 0),
        moves("grid.move-up", "Move up", &["ArrowUp"], -1, 0),
        moves("grid.move-left", "Move left", &["ArrowLeft"], 0, -1),
        moves("grid.move-right", "Move right", &["ArrowRight"], 0, 1),
        walk("grid.walk-down", "Commit and move down", "Enter", Walk::Down),
        walk("grid.walk-up", "Commit and move up", "Shift+Enter", Walk::Up),
        walk("grid.walk-right", "Commit and move right", "Tab", Walk::Right),
        walk("grid.walk-left", "Commit and move left", "Shift+Tab", Walk::Left),
        extend("grid.extend-down", "Extend down", "Shift+ArrowDown", 1, 0),
        extend("grid.extend-up", "Extend up", "Shift+ArrowUp", -1, 0),
        extend("grid.extend-left", "Extend left", "Shift+ArrowLeft", 0, -1),
        extend("grid.extend-right", "Extend right", "Shift+ArrowRight", 0, 1),
        ViewCommand::new(view, "grid.clear-contents", "Clear Contents", &["Delete"], move |_| {
            let areas = selected_areas(session);
            let stamp = session.stamp().filter(|_| !areas.is_empty())
                .ok_or("Select worksheet cells to clear.")?;
            session.submit(Batch {
                id: String::new(),
                workbook: Some(stamp.workbook),
                base: Some(stamp.model),
                expects: Vec::new(),
                selection_seq: None,
                label: Some("Clear Contents".into()),
                changes: vec![Change::Model(ModelChange::ClearContents { areas })],
            });
            Ok(())
        })
        .when("mode:ready && writable"),
        ViewCommand::new(view, "grid.edit-cell", "Edit the cell", &["F2"], move |_| {
            let at = session.active().ok_or("No cell is selected.")?;
            session.begin_edit(at, None);
            Ok(())
        })
        .when("mode:ready && writable"),
        ViewCommand::new(view, "grid.clear-and-edit", "Clear and edit the cell", &["Backspace"], move |_| {
            let at = session.active().ok_or("No cell is selected.")?;
            session.begin_edit(at, Some(String::new()));
            Ok(())
        })
        .when("mode:ready && writable"),
    ]
}

pub fn mount(session: Session) -> impl IntoView {
    let focus = NodeRef::<leptos::html::Div>::new();
    let active = Memo::new(move |_| session.active());
    let page = Memo::new(move |_| active.get().map(page_of));
    let cells = session.following::<query::Cells>();
    let geometry = session.following::<query::Geometry>();
    let selected = Memo::new(move |_| selected_areas(session));
    let dragging = RwSignal::new(None::<Key>);
    view! {
        <div class="dna-sheet" data-dnacalc="grid" tabindex="0" node_ref=focus
            role="region" aria-label="Worksheet grid"
            on:pointerup=move |_| dragging.set(None)
            on:pointercancel=move |_| dragging.set(None)
        >
            {move || {
                let area = page.get();
                let window = cells.at(area.map(|area| query::Cells { interest: Interest::Area(area) }));
                let geometry = geometry.at(area.map(|Area(first, _)| query::Geometry { sheet: Sheet(first.0, first.1) }));
                let (Some(Area(first, last)), Live::Ready(window)) = (area, window) else {
                    return view! { <p class="dna-sheet-loading">"Loading worksheet…"</p> }.into_any();
                };
                let window = window.body;
                let geometry = match geometry { Live::Ready(answer) => Some(answer.body), _ => None };
                let layout = geometry.as_ref().map(|g| {
                    metrics::sheet_metrics(Some(SheetBands {
                        rows: &g.rows,
                        columns: &g.columns,
                        default_row_height: Some(g.default_row_height),
                        default_column_width: Some(g.default_column_width),
                        base_column_width: None,
                        default_digit_px: Some(g.default_digit_px),
                        ..Default::default()
                    })).scaled(f64::from(g.zoom) / 100.)
                }).unwrap_or_default();
                let cells: BTreeMap<_, _> = window.cells.iter().map(|cell| (cell.at, cell)).collect();
                let columns: Vec<_> = (first.3..=last.3).filter(|&column| layout.width(column) > 0.).collect();
                let headings = columns.iter().copied().map(|column| {
                    let width = format!("{}px", layout.width(column));
                    view! { <th scope="col" style:width=width>{address::column_letters(column)}</th> }
                }).collect_view();
                let rows = (first.2..=last.2).filter(|&row| layout.height(row) > 0.).map(|row| {
                    let row_height = format!("{}px", layout.height(row));
                    let contents = columns.iter().copied().map(|column| {
                        let key = Key(first.0, first.1, row, column);
                        let cell = cells.get(&key).copied();
                        let text = cell.map(|cell| cell.text.clone()).unwrap_or_default();
                        let number = cell.is_some_and(|cell| matches!(cell.shown.value, RawValue::Number { .. }));
                        let error = cell.is_some_and(|cell| matches!(cell.shown.value, RawValue::Error(_)));
                        let provenance = cell.map(|cell| match &cell.shown.provenance {
                            Provenance::Empty => "empty",
                            Provenance::Literal => "literal",
                            Provenance::Calculated { .. } => "calculated",
                            Provenance::AsSaved => "file-cache",
                            Provenance::Stale { .. } => "stale",
                            Provenance::Refused { .. } => "refused",
                            Provenance::External { .. } => "external",
                        }).unwrap_or("empty");
                        let location = format!("{}{row}", address::column_letters(column));
                        let is_selected = move || active.get() == Some(key)
                            || selected.with(|areas| areas.iter().any(|Area(first, last)| {
                                (first.0, first.1) == (key.0, key.1)
                                    && (first.2..=last.2).contains(&key.2)
                                    && (first.3..=last.3).contains(&key.3)
                            }));
                        view! {
                            <td role="gridcell" aria-colindex=column
                                aria-selected=move || is_selected().to_string()
                                class:dna-selected=is_selected
                                class:dna-active=move || active.get() == Some(key)
                                class:dna-number=number class:dna-error=error
                                data-cell=location data-provenance=provenance
                                on:pointerdown=move |event: leptos::ev::PointerEvent| {
                                    if event.button() == 0 && event.pointer_type() == "mouse" && session.mode() == InputMode::Ready {
                                        let anchor = if event.shift_key() {
                                            session.active().unwrap_or(key)
                                        } else {
                                            key
                                        };
                                        dragging.set(Some(anchor));
                                    }
                                }
                                on:pointerenter=move |event: leptos::ev::PointerEvent| {
                                    if event.buttons() & 1 != 0
                                        && let Some(anchor) = dragging.get_untracked()
                                    {
                                        select_range(session, anchor, key);
                                    }
                                }
                                on:click=move |event: leptos::ev::MouseEvent| {
                                    if let Some(grid) = focus.get() { let _ = grid.focus(); }
                                    if event.shift_key() && session.mode() == InputMode::Ready {
                                        select_range(session, session.active().unwrap_or(key), key);
                                    } else {
                                        session.select_cell(key);
                                    }
                                }
                                on:dblclick=move |_| session.begin_edit(key, None)
                            >{text}</td>
                        }
                    }).collect_view();
                    view! { <tr aria-rowindex=row style:height=row_height><th scope="row">{row}</th>{contents}</tr> }
                }).collect_view();
                view! {
                    <table role="grid" aria-label="Worksheet" aria-rowcount="1048576" aria-colcount="16384">
                        <thead><tr><th class="dna-sheet-corner" aria-label="Row headings"></th>{headings}</tr></thead>
                        <tbody>{rows}</tbody>
                    </table>
                }.into_any()
            }}
        </div>
    }
}
