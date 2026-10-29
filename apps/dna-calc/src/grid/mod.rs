//! The Excel grid skin: the active sheet's cells in rows and columns, with column letters and row
//! numbers, the active cell, an editor over it, scrolling, zoom, and Excel's keys.
//!
//! "Viewing is subscribing": the skin subscribes to the area of the sheet in view
//! ([`calc_host::Interest::Area`]), moves the subscription's interest as the view scrolls, and draws
//! the cells its mirror holds, each with its value's provenance (`data-provenance`): a refused
//! formula shows as refused, a value the file stored as a file cache, a value awaiting
//! recalculation as stale. The subscription ends when the skin is no longer shown.
//!
//! The grid scrolls natively: a scroller holds a spacer as large as the sheet's extent, and a
//! layer pinned to the scroller's view (`position: sticky`) holds the cells of the
//! [`render_plan::RenderPlan`] at their viewport-local rectangles ([`geometry::cell_rect`]). A
//! pointer press is mapped to a cell by [`geometry::hit_test`], the inverse of the same rectangles.
//! The keys follow [`selection::sheet_keymap`]: arrows, Tab, Home and Enter move; F2, a double
//! click or a typed character edit; Enter and Tab enter the edit and move; Escape cancels; Delete
//! clears; Ctrl+Z and Ctrl+Y undo and redo; F9 calculates.
//!
//! Ported from DnaTreeCalc (commit bf6dfd7) `src/dnacalc-stage-sheet/src/lib.rs` (`SheetStage`:
//! select and edit modes, type-to-replace, F2 and double click, the one overlay editor and its
//! focus, the keydown pipeline, reveal on navigation, zoom) with its pure modules
//! ([`geometry`], [`viewport`], [`selection`], [`render_plan`]); the canvas drawing is replaced by
//! positioned elements, and the legacy projection and dispatcher by calc-host's subscription and
//! intents.

pub mod chart;
pub mod geometry;
pub mod icons;
pub mod render_plan;
pub mod selection;
pub mod viewport;

use crate::cells;
use crate::controller::{Controller, EditOrigin};
use crate::skin::{Skin, SkinContext, SkinId};
use crate::style::css;
use calc_host::value::{CellKey, GridArea};
use calc_host::{DeltaMirror, Interest, SubscriptionId};
use geometry::{
    GridMetrics, HitTarget, Viewport, cell_rect, hit_test, visible_col_range, visible_row_range,
};
use leptos::prelude::*;
use render_plan::build_render_plan;
use selection::{
    GridExtent, KeyChord, NavAction, SheetAction, next_active, resolve_action, sheet_keymap,
    typed_char,
};

/// Rows and columns the extent reaches past the used area and the view, so there is room to type.
const MARGIN_ROWS: u32 = 50;
const MARGIN_COLS: u32 = 10;

/// The subscribed area is the view rounded out to blocks of this many rows and columns, so small
/// scrolls do not move the interest.
const BLOCK_ROWS: u32 = 32;
const BLOCK_COLS: u32 = 8;

/// The grid skin.
#[derive(Clone, Copy, Debug, Default)]
pub struct GridSkin;

/// The last row and column of the grid: a whole column or row selected reaches them.
const MAX_ROW: u32 = 1_048_576;
const MAX_COLUMN: u32 = 16_384;

/// The maximum digit width of Excel's default font (Calibri 11) at 100%, in pixels: a column's
/// width in characters, as the file writes it, times this is its width in pixels.
const DIGIT_PX: f64 = 7.0;

/// A sheet's metrics from its geometry as the file states it (calc-host's `sheet_geometry`):
/// Excel's default 64-pixel column and 20-pixel (15-point) row unless the sheet states other
/// defaults, each sized column and row at its size, and a hidden one at 0.
pub fn sheet_metrics(geometry: Option<&calc_host::SheetGeometry>) -> GridMetrics {
    let column_px = |chars: f64| (chars * DIGIT_PX).round();
    let row_px = |points: f64| (points * 96.0 / 72.0).round();
    let col_width = geometry
        .and_then(|g| g.default_column_width)
        .map_or(64.0, column_px);
    let row_height = geometry
        .and_then(|g| g.default_row_height)
        .map_or(20.0, row_px);
    let (columns, rows) = match geometry {
        Some(geometry) => (
            geometry
                .columns
                .iter()
                .flat_map(|span| {
                    let size = if span.hidden {
                        0.0
                    } else {
                        span.width.map_or(col_width, column_px)
                    };
                    (span.first..=span.last.min(16_384)).map(move |column| (column, size))
                })
                .collect::<Vec<_>>(),
            geometry
                .rows
                .iter()
                .map(|(&row, format)| {
                    let size = if format.hidden {
                        0.0
                    } else {
                        format.height.map_or(row_height, row_px)
                    };
                    (row, size)
                })
                .collect::<Vec<_>>(),
        ),
        None => (Vec::new(), Vec::new()),
    };
    GridMetrics {
        row_height,
        col_width,
        header_h: 22.0,
        header_w: 48.0,
        rows: geometry::Sizes::new(row_height, rows),
        cols: geometry::Sizes::new(col_width, columns),
    }
}

impl Skin for GridSkin {
    fn id(&self) -> SkinId {
        SkinId::Grid
    }

    fn title(&self) -> &'static str {
        "Grid"
    }

    fn mount(&self, ctx: SkinContext) -> AnyView {
        grid_view(ctx.controller).into_any()
    }
}

/// The area to subscribe to for a view: its visible rows and columns rounded out to whole blocks.
pub fn interest_area(
    sheet: calc_host::value::SheetId,
    m: &GridMetrics,
    v: &Viewport,
    extent: GridExtent,
) -> Option<GridArea> {
    let (first_row, last_row) = visible_row_range(m, v, extent.rows)?;
    let (first_col, last_col) = visible_col_range(m, v, extent.cols)?;
    let round_down = |n: u32, block: u32| (n - 1) / block * block + 1;
    let round_up = |n: u32, block: u32| n.div_ceil(block) * block;
    let top = round_down(first_row, BLOCK_ROWS);
    let left = round_down(first_col, BLOCK_COLS);
    let bottom = round_up(last_row, BLOCK_ROWS).min(1_048_576);
    let right = round_up(last_col, BLOCK_COLS).min(16_384);
    GridArea::new(
        CellKey::new(sheet, top, left).ok()?,
        CellKey::new(sheet, bottom, right).ok()?,
    )
    .ok()
}

/// The rows and columns the grid scrolls over: the used area, the active cell and the view, with a
/// margin past each.
pub fn extent(
    used: Option<GridArea>,
    active: (u32, u32),
    m: &GridMetrics,
    v: &Viewport,
) -> GridExtent {
    let (used_rows, used_cols) =
        used.map_or((0, 0), |area| (area.last().row(), area.last().column()));
    let view_rows = ((v.scroll_y + v.height) / m.row_height).ceil().max(0.0) as u32;
    let view_cols = ((v.scroll_x + v.width) / m.col_width).ceil().max(0.0) as u32;
    let rows = used_rows.max(active.0).max(view_rows) + MARGIN_ROWS;
    let cols = used_cols.max(active.1).max(view_cols) + MARGIN_COLS;
    GridExtent::new(rows.min(1_048_576), cols.min(16_384))
}

fn grid_view(controller: Controller) -> impl IntoView {
    let zoom = RwSignal::new(1.0f64);
    let viewport = RwSignal::new(Viewport {
        scroll_x: 0.0,
        scroll_y: 0.0,
        width: 1000.0,
        height: 560.0,
    });
    let sheet_metrics = Memo::new(move |_| {
        let sheet = controller.sheet.get();
        controller.with(|book| {
            sheet_metrics(
                sheet
                    .and_then(|sheet| book.host().sheet_geometry(sheet))
                    .as_ref(),
            )
        })
    });
    let metrics = Memo::new(move |_| sheet_metrics.with(|m| m.scaled(zoom.get())));
    let mirror = RwSignal::new(DeltaMirror::default());
    let subscription: StoredValue<Option<(u64, SubscriptionId)>> = StoredValue::new(None);
    let scroller: NodeRef<leptos::html::Div> = NodeRef::new();
    let layer: NodeRef<leptos::html::Div> = NodeRef::new();
    let editor: NodeRef<leptos::html::Input> = NodeRef::new();
    let note = RwSignal::new(String::new());
    // Whether the active cell's validation list is open.
    let list_open = RwSignal::new(false);
    // Whether a press on a cell is being dragged to extend the selection.
    let dragging = StoredValue::new(false);

    let used = Memo::new(move |_| {
        let sheet = controller.sheet.get()?;
        controller.with(|book| book.used_area(sheet))
    });
    let grid_extent = Memo::new(move |_| {
        extent(
            used.get(),
            controller.active.get(),
            &metrics.get(),
            &viewport.get(),
        )
    });
    let interest = Memo::new(move |_| {
        let sheet = controller.sheet.get()?;
        interest_area(sheet, &metrics.get(), &viewport.get(), grid_extent.get())
    });

    // Subscribe to the area in view, and move the interest as the view moves. A new workbook is a
    // new generation, whose session holds none of the old subscriptions.
    Effect::new(move |_| {
        let generation = controller.generation.get();
        let Some(area) = interest.get() else {
            return;
        };
        match subscription.get_value() {
            Some((held, id)) if held == generation => {
                controller.set_interest(id, Interest::Area(area));
            }
            _ => {
                let id = controller.subscribe(Interest::Area(area), mirror);
                subscription.set_value(id.map(|id| (generation, id)));
            }
        }
    });
    on_cleanup(move || {
        if let Some((generation, id)) = subscription.get_value()
            && generation == controller.generation.get_untracked()
        {
            controller.unsubscribe(id);
        }
    });

    let plan = Memo::new(move |_| {
        let sheet = controller.sheet.get();
        // Only the shown sheet's cells: until the interest moves to a sheet just shown, the
        // mirror still holds the previous sheet's. Each in its number format.
        let shown: Vec<cells::CellShown> = controller.with(|book| {
            mirror.with(|mirror| {
                mirror
                    .cells()
                    .filter(|cell| Some(cell.at().sheet()) == sheet)
                    .map(|cell| book.shown(cell))
                    .collect()
            })
        });
        let extent = grid_extent.get();
        build_render_plan(
            &shown,
            &metrics.get(),
            &viewport.get(),
            extent.rows,
            extent.cols,
        )
    });

    // The view's size follows the scroller's.
    let measure = move || {
        if let Some(element) = scroller.get_untracked() {
            let (width, height) = (
                f64::from(element.client_width()),
                f64::from(element.client_height()),
            );
            let (scroll_x, scroll_y) = (
                f64::from(element.scroll_left()),
                f64::from(element.scroll_top()),
            );
            let next = Viewport {
                scroll_x,
                scroll_y,
                width,
                height,
            };
            if width > 0.0 && height > 0.0 && viewport.get_untracked() != next {
                viewport.set(next);
            }
        }
    };
    Effect::new(move |_| {
        if scroller.get().is_some() {
            measure();
        }
    });
    let resize = window_event_listener(leptos::ev::resize, move |_| measure());
    on_cleanup(move || resize.remove());

    // Scrolls the active cell into view, moving as little as possible.
    let reveal = move |row: u32, col: u32| {
        let Some(element) = scroller.get_untracked() else {
            return;
        };
        let m = metrics.get_untracked();
        let v = viewport.get_untracked();
        let x = viewport::reveal_span(
            m.col_start(col),
            m.col_start(col) + m.width(col),
            m.header_w,
            v.width,
            v.scroll_x,
        );
        let y = viewport::reveal_span(
            m.row_start(row),
            m.row_start(row) + m.height(row),
            m.header_h,
            v.height,
            v.scroll_y,
        );
        if x != v.scroll_x {
            element.set_scroll_left(x as i32);
        }
        if y != v.scroll_y {
            element.set_scroll_top(y as i32);
        }
        measure();
    };
    // However the active cell moves (keys, the name box, an undo), it is brought into view.
    Effect::new(move |_| {
        let (row, col) = controller.active.get();
        if scroller.get().is_some() {
            reveal(row, col);
        }
    });
    let focus_grid = move || {
        if let Some(element) = scroller.get_untracked() {
            let _ = element.focus();
        }
    };
    let move_to = move |row: u32, col: u32| controller.select(row, col);
    let go = move |action: NavAction| {
        let next = next_active(
            controller.active.get_untracked(),
            action,
            grid_extent.get_untracked(),
        );
        move_to(next.0, next.1);
    };
    // Enters the cell editor's text and moves as Excel does after Enter or Tab.
    let commit_and = move |action: NavAction| {
        if controller.commit_edit() {
            go(action);
            focus_grid();
        }
    };

    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        if controller.editing.get_untracked().is_some() {
            return;
        }
        let ctrl = ev.ctrl_key() || ev.meta_key();
        let chord = KeyChord::new(ev.key(), ev.shift_key(), ctrl, ev.alt_key());
        if ctrl && !ev.alt_key() {
            match chord.key.as_str() {
                "z" => {
                    ev.prevent_default();
                    controller.undo();
                    return;
                }
                "y" => {
                    ev.prevent_default();
                    controller.redo();
                    return;
                }
                "c" => {
                    ev.prevent_default();
                    controller.copy();
                    return;
                }
                "v" => {
                    ev.prevent_default();
                    controller.paste();
                    return;
                }
                "x" => {
                    ev.prevent_default();
                    controller.cut();
                    return;
                }
                "d" => {
                    ev.prevent_default();
                    controller.fill(true);
                    return;
                }
                "r" => {
                    ev.prevent_default();
                    controller.fill(false);
                    return;
                }
                // Ctrl+Plus (Ctrl+Shift+=) inserts, Ctrl+Minus deletes, as in Excel.
                "+" | "=" if ev.shift_key() || chord.key == "+" => {
                    ev.prevent_default();
                    controller.edit_axis(true);
                    return;
                }
                "-" => {
                    ev.prevent_default();
                    controller.edit_axis(false);
                    return;
                }
                _ => {}
            }
        }
        if chord.key == "F9" {
            ev.prevent_default();
            controller.recalculate();
            return;
        }
        // Alt+Down opens the active cell's validation list, as in Excel; Escape closes it.
        if ev.alt_key() && chord.key == "ArrowDown" {
            ev.prevent_default();
            list_open.set(true);
            return;
        }
        if chord.key == "Escape" && list_open.get_untracked() {
            list_open.set(false);
        }
        if let Some(action) = resolve_action(&sheet_keymap(), &chord) {
            ev.prevent_default();
            note.set(String::new());
            match action {
                SheetAction::Move(nav) => go(nav),
                SheetAction::Extend(nav) => {
                    let corner = next_active(
                        controller.corner.get_untracked(),
                        nav,
                        grid_extent.get_untracked(),
                    );
                    controller.extend(corner);
                    reveal(corner.0, corner.1);
                }
                SheetAction::BeginEdit => controller.begin_edit(EditOrigin::Cell, None),
                SheetAction::Clear => controller.clear_active(),
                SheetAction::Cancel => {}
            }
            return;
        }
        if !ctrl
            && !ev.alt_key()
            && let Some(ch) = typed_char(&ev.key())
        {
            ev.prevent_default();
            controller.begin_edit(EditOrigin::Cell, Some(ch.to_string()));
        }
    };

    // A press on the grid: the cell under it becomes active (an edit in progress is entered
    // first), or with Shift the selection's corner; a column or row header selects the column
    // or row, the corner the sheet; dragging moves the selection's corner.
    let point = move |ev: &leptos::ev::MouseEvent| -> Option<HitTarget> {
        let rect = layer.get_untracked()?.get_bounding_client_rect();
        let x = f64::from(ev.client_x()) - rect.left();
        let y = f64::from(ev.client_y()) - rect.top();
        let extent = grid_extent.get_untracked();
        Some(hit_test(
            &metrics.get_untracked(),
            &viewport.get_untracked(),
            extent.rows,
            extent.cols,
            x,
            y,
        ))
    };
    let on_mousedown = move |ev: leptos::ev::MouseEvent| {
        if ev.button() != 0 {
            return;
        }
        let target = point(&ev);
        if controller.editing.get_untracked() == Some(EditOrigin::Cell)
            && let Some(input) = editor.get_untracked()
            && ev
                .target()
                .is_some_and(|t| t == wasm_bindgen::JsValue::from(input.clone()).into())
        {
            return;
        }
        let Some(target) = target else {
            return;
        };
        if matches!(target, HitTarget::Outside) {
            return;
        }
        if controller.editing.get_untracked().is_some() && !controller.commit_edit() {
            return;
        }
        ev.prevent_default();
        note.set(String::new());
        let shift = ev.shift_key();
        match target {
            HitTarget::Cell { row, col } if shift => controller.extend((row, col)),
            HitTarget::Cell { row, col } => {
                move_to(row, col);
                dragging.set_value(true);
            }
            HitTarget::ColumnHeader { col } if shift => {
                controller.select_range((1, controller.active.get_untracked().1), (MAX_ROW, col))
            }
            HitTarget::ColumnHeader { col } => controller.select_range((1, col), (MAX_ROW, col)),
            HitTarget::RowHeader { row } if shift => {
                controller.select_range((controller.active.get_untracked().0, 1), (row, MAX_COLUMN))
            }
            HitTarget::RowHeader { row } => controller.select_range((row, 1), (row, MAX_COLUMN)),
            HitTarget::Corner => controller.select_range((1, 1), (MAX_ROW, MAX_COLUMN)),
            HitTarget::Outside => {}
        }
        focus_grid();
    };
    let on_mousemove = move |ev: leptos::ev::MouseEvent| {
        if !dragging.get_value() || ev.buttons() & 1 == 0 {
            dragging.set_value(false);
            return;
        }
        if let Some(HitTarget::Cell { row, col }) = point(&ev)
            && controller.corner.get_untracked() != (row, col)
        {
            controller.extend((row, col));
        }
    };
    let on_dblclick = move |ev: leptos::ev::MouseEvent| {
        if let Some(HitTarget::Cell { row, col }) = point(&ev) {
            move_to(row, col);
            controller.begin_edit(EditOrigin::Cell, None);
        }
    };

    // The editor over the active cell takes the focus when it opens, with the caret at the end.
    Effect::new(move |_| {
        if controller.editing.get() == Some(EditOrigin::Cell)
            && let Some(input) = editor.get()
        {
            let _ = input.focus();
            let end = input.value().encode_utf16().count() as u32;
            let _ = input.set_selection_range(end, end);
        }
    });
    let on_editor_key = move |ev: leptos::ev::KeyboardEvent| {
        match ev.key().as_str() {
            "Enter" if !ev.alt_key() => {
                ev.prevent_default();
                commit_and(if ev.shift_key() {
                    NavAction::Up
                } else {
                    NavAction::CommitDown
                });
            }
            "Tab" => {
                ev.prevent_default();
                commit_and(if ev.shift_key() {
                    NavAction::PrevColumn
                } else {
                    NavAction::CommitRight
                });
            }
            "Escape" => {
                ev.prevent_default();
                controller.cancel_edit();
                focus_grid();
            }
            _ => {}
        }
        ev.stop_propagation();
    };

    let px = |n: f64| format!("{n}px");
    let rect_style = move |rect: &geometry::CellRect| {
        format!(
            "left:{};top:{};width:{};height:{}",
            px(rect.x),
            px(rect.y),
            px(rect.w),
            px(rect.h)
        )
    };

    let cells_view = move || {
        plan.with(|plan| {
            plan.cells
                .iter()
                .map(|cell| {
                    let address = cells::address(cell.row, cell.col);
                    let mut style = rect_style(&cell.rect);
                    let conditional = cell.shown.conditional.clone().unwrap_or_default();
                    // A conditional format's font colour comes before the number format's.
                    if let Some(color) = conditional.font_color.as_ref().or(cell.shown.color.as_ref()) {
                        style.push_str(&format!(";color:{color}"));
                    }
                    if let Some(fill) = &conditional.fill {
                        style.push_str(&format!(";background-color:{fill}"));
                    }
                    if let Some((share, color, gradient)) = &conditional.bar {
                        let end = if *gradient { "#ffffff" } else { color.as_str() };
                        style.push_str(&format!(
                            ";background-image:linear-gradient(to right,{color},{end});background-size:{:.1}% 72%;background-position:left center;background-repeat:no-repeat",
                            share * 100.0
                        ));
                    }
                    if conditional.bold {
                        style.push_str(";font-weight:700");
                    }
                    if conditional.italic {
                        style.push_str(";font-style:italic");
                    }
                    match (conditional.underline, conditional.strike) {
                        (true, true) => style.push_str(";text-decoration:underline line-through"),
                        (true, false) => style.push_str(";text-decoration:underline"),
                        (false, true) => style.push_str(";text-decoration:line-through"),
                        (false, false) => {}
                    }
                    let shown = cell.shown.clone();
                    let icon = conditional
                        .icon
                        .as_ref()
                        .map(|(set, index)| (format!("{set}:{index}"), icons::icon_svg(set, *index)));
                    let text = if conditional.hide_value { String::new() } else { shown.text.clone() };
                    let icon_name = icon.as_ref().map(|(name, _)| name.clone());
                    let bar = conditional
                        .bar
                        .as_ref()
                        .map(|(share, _, _)| format!("{share:.4}"));
                    view! {
                        <div
                            class="dc-cell"
                            data-dnacalc="cell"
                            data-cell=address
                            data-kind=shown.kind
                            data-provenance=shown.provenance.name()
                            data-format=shown.format.name()
                            data-formula=if shown.formula { "true" } else { "false" }
                            data-icon=icon_name
                            data-bar=bar
                            use:css=style
                        >
                            {icon.map(|(_, markup)| view! { <span class="dc-icon" inner_html=markup></span> })}
                            {text}
                        </div>
                    }
                })
                .collect_view()
        })
    };
    let headers_view = move || {
        let (row, col) = controller.active.get();
        let (corner_row, corner_col) = controller.corner.get();
        let rows = row.min(corner_row)..=row.max(corner_row);
        let cols = col.min(corner_col)..=col.max(corner_col);
        plan.with(|plan| {
            let columns = plan
                .col_headers
                .iter()
                .map(|header| {
                    let class = if cols.contains(&header.col) { "dc-head dc-on" } else { "dc-head" };
                    view! { <div class=class data-dnacalc="column-header" data-column=header.label.clone() use:css=rect_style(&header.rect)>{header.label.clone()}</div> }
                })
                .collect_view();
            let rows = plan
                .row_headers
                .iter()
                .map(|header| {
                    let class = if rows.contains(&header.row) { "dc-head dc-on" } else { "dc-head" };
                    view! { <div class=class data-dnacalc="row-header" data-row=header.label.clone() use:css=rect_style(&header.rect)>{header.label.clone()}</div> }
                })
                .collect_view();
            (columns, rows)
        })
    };
    let active_rect = move || {
        let (row, col) = controller.active.get();
        cell_rect(&metrics.get(), &viewport.get(), row, col)
    };
    // The selected range, clipped to what a rectangle in the layer can hold.
    let selection_view = move || {
        let area = controller.selected_area()?;
        if area.cell_count() == 1 {
            return None;
        }
        let m = metrics.get();
        let v = viewport.get();
        let (first, last) = (area.first(), area.last());
        let top_left = cell_rect(&m, &v, first.row(), first.column());
        let bottom_right = cell_rect(&m, &v, last.row(), last.column());
        let clip = |near: f64, far: f64, len: f64| (near.max(-2.0), far.min(len + 2.0));
        let (x0, x1) = clip(top_left.x, bottom_right.x + bottom_right.w, v.width);
        let (y0, y1) = clip(top_left.y, bottom_right.y + bottom_right.h, v.height);
        let label = controller.with_untracked(|book| cells::area_label(book.host(), area));
        let label = label.rsplit('!').next().unwrap_or_default().to_owned();
        let style = format!(
            "left:{};top:{};width:{};height:{}",
            px(x0),
            px(y0),
            px((x1 - x0).max(0.0)),
            px((y1 - y0).max(0.0))
        );
        Some(
            view! { <div class="dc-selection" data-dnacalc="selection" data-area=label use:css=style></div> },
        )
    };
    let active_view = move || {
        let rect = active_rect();
        view! { <div class="dc-active" data-dnacalc="active-cell" use:css=rect_style(&rect)></div> }
    };
    // The sheet's charts (dc-6yw), as the workbook holds their series now: read again after every
    // change, drawn where their frames are, the ones in view only.
    let charts = Memo::new(move |_| {
        let sheet = controller.sheet.get()?;
        Some(controller.with(|book| book.host().charts(sheet).0))
    });
    let charts_view = move || {
        let m = metrics.get();
        let v = viewport.get();
        let z = zoom.get();
        let corner = |c: &calc_host::ChartCorner| {
            let r = cell_rect(&m, &v, c.cell.row(), c.cell.column());
            (r.x + c.dx * z, r.y + c.dy * z)
        };
        charts
            .get()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|view| {
                let (x0, y0, x1, y1) = match view.place {
                    calc_host::ChartPlace::Cells { from, to } => {
                        let ((a, b), (c, d)) = (corner(&from), corner(&to));
                        (a, b, c, d)
                    }
                    calc_host::ChartPlace::Sized {
                        from,
                        width,
                        height,
                    } => {
                        let (a, b) = corner(&from);
                        (a, b, a + width * z, b + height * z)
                    }
                    calc_host::ChartPlace::Absolute {
                        x,
                        y,
                        width,
                        height,
                    } => {
                        let origin = cell_rect(&m, &v, 1, 1);
                        let (a, b) = (origin.x + x * z, origin.y + y * z);
                        (a, b, a + width * z, b + height * z)
                    }
                };
                if x1 <= x0 || y1 <= y0 || x1 < 0.0 || y1 < 0.0 || x0 > v.width || y0 > v.height {
                    return None;
                }
                let markup = chart::chart_svg(&view, (x1 - x0) / z, (y1 - y0) / z);
                let kind = view
                    .plots
                    .first()
                    .map(|plot| format!("{:?}", plot.kind).to_lowercase())
                    .unwrap_or_default();
                let style = format!(
                    "left:{};top:{};width:{};height:{}",
                    px(x0),
                    px(y0),
                    px(x1 - x0),
                    px(y1 - y0)
                );
                Some(view! {
                    <div class="dc-chart" data-dnacalc="chart" data-chart=view.name.clone()
                        data-kind=kind use:css=style inner_html=markup></div>
                })
            })
            .collect_view()
    };
    // The active cell's data validation (dc-6yw): a list's drop-down button at its right edge, the
    // list it opens (Alt+Down too, as in Excel), and the rule's input message below the cell.
    let rule = Memo::new(move |_| {
        let at = controller.active_key()?;
        controller.with(|book| book.validation(at))
    });
    Effect::new(move |_| {
        controller.active.track();
        list_open.set(false);
    });
    let dropdown_view = move || {
        let rule = rule.get()?;
        if !rule.dropdown {
            return None;
        }
        let rect = active_rect();
        let style = format!(
            "left:{};top:{};height:{}",
            px(rect.x + rect.w),
            px(rect.y),
            px(rect.h)
        );
        Some(view! {
            <button
                type="button"
                class="dc-dropdown"
                data-dnacalc="dropdown"
                title="Pick from the list"
                use:css=style
                on:mousedown=move |ev| ev.stop_propagation()
                on:click=move |_| list_open.update(|open| *open = !*open)
            >"▼"</button>
        })
    };
    let list_view = move || {
        if !list_open.get() {
            return None;
        }
        let at = controller.active_key_untracked()?;
        let items = controller.with_untracked(|book| book.validation_items(at));
        let rect = active_rect();
        let style = format!("left:{};top:{}", px(rect.x), px(rect.y + rect.h));
        let rows = items
            .into_iter()
            .map(|item| {
                let text = item.clone();
                view! {
                    <li
                        data-dnacalc="dropdown-item"
                        on:click=move |_| {
                            list_open.set(false);
                            controller.enter(at, text.clone());
                            focus_grid();
                        }
                    >{item}</li>
                }
            })
            .collect_view();
        Some(view! {
            <ul class="dc-list" data-dnacalc="dropdown-list" use:css=style
                on:mousedown=move |ev| ev.stop_propagation()>{rows}</ul>
        })
    };
    let prompt_view = move || {
        let (title, text) = rule.get()?.prompt?;
        if list_open.get() {
            return None;
        }
        let rect = active_rect();
        let style = format!(
            "left:{};top:{}",
            px(rect.x + 8.0),
            px(rect.y + rect.h + 4.0)
        );
        Some(view! {
            <div class="dc-prompt" data-dnacalc="validation-prompt" use:css=style>
                {(!title.is_empty()).then(|| view! { <b>{title}</b> })}
                {text}
            </div>
        })
    };
    let editor_view = move || {
        (controller.editing.get() == Some(EditOrigin::Cell)).then(|| {
            let rect = active_rect();
            view! {
                <input
                    class="dc-cell-editor"
                    data-dnacalc="cell-editor"
                    node_ref=editor
                    type="text"
                    spellcheck="false"
                    autocomplete="off"
                    use:css=rect_style(&rect)
                    prop:value=move || controller.edit_text.get()
                    on:input=move |ev| controller.edit_text.set(event_target_value(&ev))
                    on:keydown=on_editor_key
                />
            }
        })
    };
    let space_width = move || {
        let m = metrics.get();
        let extent = grid_extent.get();
        px(m.header_w + m.col_start(extent.cols) + m.width(extent.cols))
    };
    let space_height = move || {
        let m = metrics.get();
        let extent = grid_extent.get();
        px(m.header_h + m.row_start(extent.rows) + m.height(extent.rows))
    };
    // The gridlines: one line after each visible column and row.
    let gridlines = move || {
        let v = viewport.get();
        plan.with(|plan| {
            let columns = plan
                .col_headers
                .iter()
                .filter(|header| header.rect.w > 0.0)
                .map(|header| {
                    let style = format!(
                        "left:{};top:0;width:1px;height:{}",
                        px(header.rect.x + header.rect.w - 1.0),
                        px(v.height)
                    );
                    view! { <div class="dc-line" use:css=style></div> }
                })
                .collect_view();
            let rows = plan
                .row_headers
                .iter()
                .filter(|header| header.rect.h > 0.0)
                .map(|header| {
                    let style = format!(
                        "left:0;top:{};width:{};height:1px",
                        px(header.rect.y + header.rect.h - 1.0),
                        px(v.width)
                    );
                    view! { <div class="dc-line" use:css=style></div> }
                })
                .collect_view();
            (columns, rows)
        })
    };

    view! {
        <div class="dc-grid-skin" data-skin="grid">
            <div
                class="dc-grid"
                data-dnacalc="grid"
                tabindex="0"
                node_ref=scroller
                on:scroll=move |_| measure()
                on:keydown=on_keydown
                on:mousedown=on_mousedown
                on:dblclick=on_dblclick
                on:mousemove=on_mousemove
                on:mouseup=move |_| dragging.set_value(false)
            >
                <div class="dc-space" style:width=space_width style:height=space_height>
                    <div class="dc-layer" node_ref=layer style:width=move || px(viewport.get().width) style:height=move || px(viewport.get().height)>
                        {gridlines}
                        {cells_view}
                        {charts_view}
                        {selection_view}
                        {active_view}
                        {dropdown_view}
                        {prompt_view}
                        {list_view}
                        {move || {
                            let (columns, rows) = headers_view();
                            view! { {columns}{rows} }
                        }}
                        <div class="dc-corner" style:width=move || px(metrics.get().header_w) style:height=move || px(metrics.get().header_h)></div>
                        {editor_view}
                    </div>
                </div>
            </div>
            <div class="dc-gridbar">
                <span data-dnacalc="grid-note" class="dc-note">{move || note.get()}</span>
                <button type="button" data-dnacalc="insert-cells" title="Insert the selected rows or columns (Ctrl++)"
                    on:click=move |_| controller.edit_axis(true)>"Insert"</button>
                <button type="button" data-dnacalc="delete-cells" title="Delete the selected rows or columns (Ctrl+-)"
                    on:click=move |_| controller.edit_axis(false)>"Delete"</button>
                <span class="dc-zoom">
                    <button type="button" data-dnacalc="zoom-out" title="Zoom out"
                        on:click=move |_| zoom.update(|z| *z = viewport::clamp_zoom(*z / viewport::ZOOM_STEP))>"-"</button>
                    <span data-dnacalc="zoom">{move || format!("{:.0}%", zoom.get() * 100.0)}</span>
                    <button type="button" data-dnacalc="zoom-in" title="Zoom in"
                        on:click=move |_| zoom.update(|z| *z = viewport::clamp_zoom(*z * viewport::ZOOM_STEP))>"+"</button>
                </span>
            </div>
        </div>
    }
}

/// Moves the grid's focus to its scroller: the shell calls it after the formula bar enters an
/// edit, so the keys go to the grid again.
pub fn focus_grid() {
    if let Some(element) = leptos::prelude::document()
        .query_selector("[data-dnacalc=\"grid\"]")
        .ok()
        .flatten()
        && let Ok(element) = wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlElement>(element)
    {
        let _ = element.focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_host::value::{IdAllocator, SheetId};

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_interest_is_the_view_rounded_out_to_blocks() {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = ids.issue().unwrap();
        let m = GridMetrics::default();
        let v = Viewport {
            scroll_x: 0.0,
            scroll_y: 0.0,
            width: 800.0,
            height: 400.0,
        };
        let area = interest_area(sheet, &m, &v, GridExtent::new(1000, 100)).unwrap();
        assert_eq!((area.first().row(), area.first().column()), (1, 1));
        assert_eq!((area.last().row(), area.last().column()), (32, 16));
        // A small scroll stays inside the same blocks.
        let scrolled = Viewport {
            scroll_y: 44.0,
            ..v
        };
        assert_eq!(
            interest_area(sheet, &m, &scrolled, GridExtent::new(1000, 100)),
            Some(area)
        );
        // A long one moves the interest.
        let far = Viewport {
            scroll_y: 22.0 * 100.0,
            ..v
        };
        let moved = interest_area(sheet, &m, &far, GridExtent::new(1000, 100)).unwrap();
        assert_eq!(moved.first().row(), 97);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_extent_reaches_past_the_used_area_the_active_cell_and_the_view() {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = ids.issue().unwrap();
        let m = GridMetrics::default();
        let v = Viewport {
            scroll_x: 0.0,
            scroll_y: 0.0,
            width: 800.0,
            height: 440.0,
        };
        let blank = extent(None, (1, 1), &m, &v);
        assert_eq!(blank, GridExtent::new(20 + MARGIN_ROWS, 10 + MARGIN_COLS));
        let used = GridArea::new(
            CellKey::new(sheet, 1, 1).unwrap(),
            CellKey::new(sheet, 500, 30).unwrap(),
        )
        .unwrap();
        assert_eq!(extent(Some(used), (1, 1), &m, &v), GridExtent::new(550, 40));
        assert_eq!(
            extent(Some(used), (900, 2), &m, &v),
            GridExtent::new(950, 40)
        );
    }
}
