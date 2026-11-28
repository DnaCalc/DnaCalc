//! Bounded spreadsheet viewport shared by workbook views.
//!
//! The grid is given the session and nothing else. It keeps its own watches: the cells of the
//! page that holds the active cell, any leading frozen bands, and the geometry of that cell's
//! sheet. It handles no key:
//! its motion and its edit keys are commands ([`commands`]) the keymap resolves.

use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::metrics::{self, SheetBands};
use calc_skin::{
    Batch, Change, FillDirection, Interest, ModelChange, Operation, Provenance, RawValue,
    Selection, SourceRef, ThinBorder, address, query,
};
use dna_kit::{InputMode, Invocation, Live, Session, ViewCommand, commands::CommandId};
use leptos::prelude::*;
use std::collections::BTreeMap;

mod trace;

const PAGE_ROWS: u32 = 36;
const PAGE_COLUMNS: u32 = 18;
const LAST_ROW: u32 = 1_048_576;
const LAST_COLUMN: u32 = 16_384;

#[derive(Clone, Copy, Default)]
struct ViewportScroll(RwSignal<bool>);

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

fn range_batch(
    session: Session,
    label: &str,
    refusal: &str,
    change: impl FnOnce(Area) -> ModelChange,
) -> Result<Batch, String> {
    let areas = selected_areas(session);
    let ([selected], Some(stamp)) = (areas.as_slice(), session.stamp()) else {
        return Err(refusal.into());
    };
    Ok(Batch {
        id: String::new(),
        workbook: Some(stamp.workbook),
        base: Some(stamp.model),
        expects: Vec::new(),
        selection_seq: None,
        label: Some(label.into()),
        changes: vec![Change::Model(change(*selected))],
    })
}

fn border_css(border: ThinBorder) -> String {
    match border {
        ThinBorder::Automatic => "1px solid var(--dna-ink)".into(),
        ThinBorder::Rgb([red, green, blue]) => format!("1px solid #{red:02x}{green:02x}{blue:02x}"),
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

fn extend_selection(session: Session, at: Key) {
    let sheet = Sheet(at.0, at.1);
    let mut areas = match session.selection().map(|selection| selection.target) {
        Some(SourceRef::Cells { sheet: selected, areas }) if selected == sheet => areas,
        _ => Vec::new(),
    };
    let added = Area(at, at);
    let active = if let Some(index) = areas.iter().position(|area| *area == added) && areas.len() > 1 {
        areas.remove(index);
        areas[0].0
    } else {
        if !areas.iter().any(|area| area.area().is_some_and(|area| at.cell().is_some_and(|at| area.contains(at)))) {
            areas.push(added);
        }
        at
    };
    session.select(Selection {
        seq: 0,
        target: SourceRef::Cells { sheet, areas },
        active: Some(active),
        grouped: Vec::new(),
        item: None,
    });
}

fn visible_bands(first: u32, last: u32, frozen: u32) -> Vec<(u32, u32)> {
    if frozen == 0 {
        vec![(first, last)]
    } else if first <= frozen + 1 {
        vec![(1, last.max(frozen))]
    } else {
        vec![(1, frozen), (first, last)]
    }
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
    let fill = |id, label, chords: &[&'static str], direction| {
        ViewCommand::new(view, id, label, chords, move |_| {
            let batch = range_batch(session, label, "Select one worksheet range to fill.",
                |selected| ModelChange::FillRange { selected, direction })?;
            session.submit(batch);
            Ok(())
        })
        .when("mode:ready && writable")
    };
    let mut commands = vec![
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
        fill("grid.fill-down", "Fill Down", &["Ctrl+D"], FillDirection::Down),
        fill("grid.fill-right", "Fill Right", &["Ctrl+R"], FillDirection::Right),
        fill("grid.fill-up", "Fill Up", &[], FillDirection::Up),
        fill("grid.fill-left", "Fill Left", &[], FillDirection::Left),
        ViewCommand::new(view, "grid.flash-fill", "Flash Fill", &["Ctrl+E"], move |_| {
            let batch = range_batch(session, "Flash Fill", "Select one worksheet range to fill.",
                |selected| ModelChange::FlashFill { selected })?;
            session.submit_for_review(batch);
            Ok(())
        })
        .when("mode:ready && writable"),
        ViewCommand::new(view, "grid.all-borders", "All Borders", &[], move |_| {
            let batch = range_batch(session, "All Borders", "Select one worksheet range to format.",
                |selected| ModelChange::SetAllBorders { selected })?;
            session.submit(batch);
            Ok(())
        })
        .when("mode:ready && writable"),
        ViewCommand::new(view, "grid.outline-border", "Outline Border", &["Ctrl+Shift+&"], move |_| {
            let batch = range_batch(session, "Outline Border", "Select one worksheet range to format.",
                |selected| ModelChange::SetOutlineBorder { selected })?;
            session.submit(batch);
            Ok(())
        })
        .when("mode:ready && writable"),
        ViewCommand::new(view, "grid.add-selection", "Add cell to selection", &[], move |invocation| {
            let at = serde_json::from_value(invocation.args.clone()).map_err(|_| "Choose a worksheet cell.")?;
            tab_run.set_value(None);
            extend_selection(session, at);
            Ok(())
        }).when("mode:ready"),
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
        ViewCommand::new(view, "grid.hide-columns", "Hide Columns", &["Ctrl+0"], move |_| {
            let Some(SourceRef::Cells { sheet, areas }) =
                session.selection().map(|selection| selection.target)
            else {
                return Err("Select worksheet columns to hide.".into());
            };
            let stamp = session.stamp().filter(|_| !areas.is_empty())
                .ok_or("Select worksheet columns to hide.")?;
            let bands = areas.into_iter().map(|Area(first, last)| (first.3, last.3)).collect();
            session.submit(Batch {
                id: String::new(),
                workbook: Some(stamp.workbook),
                base: Some(stamp.model),
                expects: Vec::new(),
                selection_seq: None,
                label: Some("Hide Columns".into()),
                changes: vec![Change::Model(ModelChange::SetColumnsHidden {
                    sheet,
                    bands,
                    hidden: true,
                })],
            });
            Ok(())
        })
        .when("mode:ready && writable"),
        ViewCommand::new(view, "grid.toggle-gridlines", "Toggle Gridlines", &[], move |_| {
            let (Some(at), Some(stamp)) = (session.active(), session.stamp()) else {
                return Err("No worksheet is selected.".into());
            };
            session.operate(Operation::ToggleGridlines {
                workbook: stamp.workbook,
                sheet: Sheet(at.0, at.1),
            }, None);
            Ok(())
        })
        .when("mode:ready && writable"),
        ViewCommand::new(view, "grid.freeze-panes", "Freeze or Unfreeze Panes", &[], move |_| {
            let (Some(at), Some(stamp)) = (session.active(), session.stamp()) else {
                return Err("No worksheet cell is selected.".into());
            };
            let Area(first, _) = page_of(at);
            let off_origin = (first.2, first.3) != (1, 1)
                || session.local::<ViewportScroll>().0.get_untracked();
            session.operate(Operation::ToggleFreezePanes { workbook: stamp.workbook, at, off_origin }, None);
            Ok(())
        })
        .when("mode:ready && writable"),
        ViewCommand::new(view, "grid.toggle-filter", "Toggle Filter", &["Ctrl+Shift+L"], move |_| {
            let areas = selected_areas(session);
            let ([selected], Some(stamp)) = (areas.as_slice(), session.stamp()) else {
                return Err("Select one worksheet range to filter.".into());
            };
            session.submit(Batch {
                id: String::new(),
                workbook: Some(stamp.workbook),
                base: Some(stamp.model),
                expects: Vec::new(),
                selection_seq: None,
                label: Some("Toggle Filter".into()),
                changes: vec![Change::Model(ModelChange::ToggleAutoFilter { selected: *selected })],
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
    ];
    commands.extend(trace::commands(view, session));
    commands
}

pub fn mount(session: Session) -> impl IntoView {
    mount_with_tints(session, || false)
}

pub fn mount_with_tints(
    session: Session,
    provenance_tints: impl Fn() -> bool + Send + 'static,
) -> impl IntoView {
    worksheet(session, true, provenance_tints)
}

/// The same worksheet surface for selecting sources without entering cells.
pub fn picker(session: Session) -> impl IntoView {
    worksheet(session, false, || false)
}

fn worksheet(
    session: Session,
    editable: bool,
    provenance_tints: impl Fn() -> bool + Send + 'static,
) -> impl IntoView {
    let focus = NodeRef::<leptos::html::Div>::new();
    let content = NodeRef::<leptos::html::Div>::new();
    let active = Memo::new(move |_| session.active());
    let page = Memo::new(move |_| active.get().map(page_of));
    let cells = session.following::<query::Cells>();
    let geometry = session.following::<query::Geometry>();
    let selected = Memo::new(move |_| selected_areas(session));
    let dragging = RwSignal::new(None::<Key>);
    let scroll = session.local::<ViewportScroll>().0;
    if editable {
        scroll.set(false);
    }
    view! {
        <div class="dna-sheet" data-dnacalc="grid" tabindex="0" node_ref=focus
            data-provenance-tints=move || provenance_tints().to_string()
            role="region" aria-label="Worksheet grid"
            on:pointerup=move |_| dragging.set(None)
            on:pointercancel=move |_| dragging.set(None)
            on:scroll=move |_| {
                if editable && let Some(viewport) = focus.get_untracked() {
                    scroll.set(viewport.scroll_top() != 0 || viewport.scroll_left() != 0);
                }
            }
        >
            <div class="dna-sheet-content" node_ref=content>
            {move || {
                let area = page.get();
                let geometry = geometry.at(area.map(|Area(first, _)| query::Geometry { sheet: Sheet(first.0, first.1) }));
                let Some(Area(first, last)) = area else {
                    cells.at(None);
                    return view! { <p class="dna-sheet-loading">"Loading worksheet…"</p> }.into_any();
                };
                let geometry = match geometry { Live::Ready(answer) => Some(answer.body), _ => None };
                // Keep the visible part of each frozen strip when the active cell moves to another page.
                let (frozen_rows, frozen_columns) = geometry.as_ref().map_or((0, 0), |g| {
                    (g.frozen_rows.min(PAGE_ROWS), g.frozen_columns.min(PAGE_COLUMNS))
                });
                let row_bands = visible_bands(first.2, last.2, frozen_rows);
                let column_bands = visible_bands(first.3, last.3, frozen_columns);
                let mut areas = Vec::new();
                for &(top, bottom) in &row_bands {
                    for &(left, right) in &column_bands {
                        areas.push(Area(Key(first.0, first.1, top, left), Key(first.0, first.1, bottom, right)));
                    }
                }
                let interest = match areas.as_slice() {
                    [only] => Interest::Area(*only),
                    _ => Interest::Areas(areas),
                };
                let Live::Ready(window) = cells.at(Some(query::Cells { interest })) else {
                    return view! { <p class="dna-sheet-loading">"Loading worksheet…"</p> }.into_any();
                };
                let window = window.body;
                let hide_gridlines = geometry.as_ref().is_some_and(|g| g.show_grid_lines == Some(false));
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
                let columns: Vec<_> = column_bands.into_iter().flat_map(|(a, b)| a..=b)
                    .filter(|&column| layout.width(column) > 0.).collect();
                let paned = frozen_rows > 0 || frozen_columns > 0;
                let pane_row_edge = (frozen_rows > 0)
                    .then(|| layout.visible_row(frozen_rows, false, frozen_rows));
                let pane_column_edge = (frozen_columns > 0)
                    .then(|| layout.visible_col(frozen_columns, false, frozen_columns));
                let pixels = |dimension| format!("{dimension}px");
                let pinned_left = |column| (column <= frozen_columns)
                    .then(|| pixels(layout.header_w + layout.col_start(column)));
                let table_width = paned.then(|| pixels(layout.header_w + columns.iter()
                    .map(|&column| layout.width(column)).sum::<f64>()));
                let column_widths = paned.then(|| {
                    let widths = columns.iter().map(|&column| {
                        view! { <col style:width=pixels(layout.width(column))/> }
                    }).collect_view();
                    view! { <colgroup><col style:width=pixels(layout.header_w)/>{widths}</colgroup> }
                });
                let headings = columns.iter().copied().map(|column| {
                    view! { <th scope="col" style:width=pixels(layout.width(column))
                        class:dna-frozen-column=column <= frozen_columns
                        class:dna-frozen-column-edge=Some(column) == pane_column_edge
                        style:inset-inline-start=pinned_left(column)>{address::column_letters(column)}</th> }
                }).collect_view();
                let rows = row_bands.iter().copied().flat_map(|(a, b)| a..=b)
                    .filter(|&row| layout.height(row) > 0.).map(|row| {
                    let row_height = pixels(layout.height(row));
                    let pinned_top = (row <= frozen_rows)
                        .then(|| pixels(layout.header_h + layout.row_start(row)));
                    let contents = columns.iter().copied().map(|column| {
                        let key = Key(first.0, first.1, row, column);
                        let cell = cells.get(&key).copied();
                        let text = cell.map(|cell| cell.text.clone()).unwrap_or_default();
                        let borders = cell.map(|cell| cell.borders).unwrap_or_default();
                        let top = borders.top.filter(|_| {
                            let previous = row - 1;
                            let painted = row_bands.iter().any(|&(a, b)| (a..=b).contains(&previous))
                                && layout.height(previous) > 0.
                                && cells.get(&Key(key.0, key.1, previous, column))
                                    .is_some_and(|above| above.borders.bottom.is_some());
                            !painted
                        });
                        let left = borders.left.filter(|_| {
                            let previous = column - 1;
                            let painted = columns.binary_search(&previous).is_ok()
                                && cells.get(&Key(key.0, key.1, row, previous))
                                    .is_some_and(|before| before.borders.right.is_some());
                            !painted
                        });
                        let number = cell.is_some_and(|cell| matches!(cell.shown.value, RawValue::Number { .. }));
                        let error = cell.is_some_and(|cell| matches!(cell.shown.value, RawValue::Error(_)));
                        let filter_header = geometry.as_ref().is_some_and(|geometry| {
                            geometry.auto_filter_areas.iter().any(|Area(first, last)| {
                                row == first.2 && (first.3..=last.3).contains(&column)
                            })
                        });
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
                                class:dna-frozen-row=row <= frozen_rows
                                class:dna-frozen-column=column <= frozen_columns
                                class:dna-frozen-row-edge=Some(row) == pane_row_edge
                                class:dna-frozen-column-edge=Some(column) == pane_column_edge
                                style:inset-block-start=pinned_top.clone()
                                style:inset-inline-start=pinned_left(column)
                                style:border-block-start=top.map(border_css)
                                style:border-inline-start=left.map(border_css)
                                style:border-block-end=borders.bottom.map(border_css)
                                style:border-inline-end=borders.right.map(border_css)
                                data-cell=location data-provenance=provenance
                                data-filter-header=filter_header.to_string()
                                title=filter_header.then_some("AutoFilter is enabled for this column")
                                on:pointerdown=move |event: leptos::ev::PointerEvent| {
                                    if event.button() == 0 && event.pointer_type() == "mouse"
                                        && !event.ctrl_key() && !event.meta_key() && session.mode() == InputMode::Ready
                                    {
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
                                    } else if (event.ctrl_key() || event.meta_key()) && session.mode() == InputMode::Ready {
                                        let _ = session.run(&Invocation {
                                            command: CommandId::new("grid.add-selection"),
                                            args: serde_json::json!(key),
                                        });
                                    } else {
                                        session.select_cell(key);
                                    }
                                }
                                on:dblclick=move |_| {
                                    if editable {
                                        session.begin_edit(key, None);
                                    }
                                }
                            >{text}</td>
                        }
                    }).collect_view();
                    view! { <tr aria-rowindex=row style:height=row_height>
                        <th scope="row" class:dna-frozen-row=row <= frozen_rows
                            class:dna-frozen-row-edge=Some(row) == pane_row_edge style:inset-block-start=pinned_top>{row}</th>
                        {contents}</tr> }
                }).collect_view();
                view! {
                    <table role="grid" aria-label="Worksheet" aria-rowcount="1048576" aria-colcount="16384"
                        class:dna-sheet-gridlines-hidden=hide_gridlines class:dna-sheet-paned=paned style:width=table_width>
                        {column_widths}
                        <thead><tr style:height=paned.then(|| pixels(layout.header_h))>
                            <th class="dna-sheet-corner" aria-label="Row headings"></th>{headings}
                        </tr></thead>
                        <tbody>{rows}</tbody>
                    </table>
                }.into_any()
            }}
            {editable.then(|| trace::layer(session, content, page))}
            </div>
        </div>
    }
}
