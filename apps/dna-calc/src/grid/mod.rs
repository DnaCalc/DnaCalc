//! The Excel grid skin: the active sheet's cells in rows and columns, with column letters and row
//! numbers, the active cell, an editor over it, scrolling, zoom, and Excel's keys.
//!
//! "Viewing is subscribing": the skin subscribes to the area of the sheet in view
//! (`wire::Interest::Area`), moves the subscription's interest as the view scrolls, and draws
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

mod alignment;
pub mod autofit;
pub mod axis;
mod decoration;
pub mod geometry;
pub mod icons;
mod inspect;
mod painter;
pub mod panes;
pub mod render_plan;
pub mod selection;
mod structure;
pub mod viewport;

use crate::cells;
use crate::controller::{Controller, EditOrigin};
use crate::skin::{Skin, SkinContext, SkinId};
use crate::style::css;
use crate::wire;
use calc_value::{CellKey, GridArea};
use geometry::{GridMetrics, HitTarget, Viewport, cell_rect, visible_col_range, visible_row_range};
use leptos::prelude::*;
use panes::Panes;
use selection::{
    GridExtent, KeyChord, NavAction, SheetAction, resolve_action, sheet_keymap, typed_char,
};

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

/// A sheet's metrics from its geometry as the file states it (calc-host's `sheet_geometry`):
/// Excel's default 64-pixel column and 20-pixel (15-point) row unless the sheet states other
/// defaults, each sized column and row at its size, and a hidden one at 0.
pub use dna_calc_model::geometry::sheet_metrics;

impl Skin for GridSkin {
    fn id(&self) -> SkinId {
        SkinId::Grid
    }

    fn title(&self) -> &'static str {
        "Grid"
    }

    fn input_target(&self) -> &'static str {
        "[data-dnacalc=grid][data-grid-active=true]"
    }

    fn clipboard_owner(&self) -> dna_calc_model::ClipboardOwner {
        dna_calc_model::ClipboardOwner::Workbook
    }

    fn focus_target(&self) -> &'static str {
        "[data-dnacalc=grid][data-grid-active=true][data-surface-ready=true], [data-dnacalc=print-workspace]"
    }

    fn mount(&self, ctx: SkinContext) -> AnyView {
        crate::printing::grid_workspace(ctx.controller)
    }
}

/// The area to subscribe to for a view: its visible rows and columns rounded out to whole blocks.
pub fn interest_area(
    sheet: calc_value::SheetId,
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
    _used: Option<GridArea>,
    _active: (u32, u32),
    _m: &GridMetrics,
    _v: &Viewport,
) -> GridExtent {
    GridExtent::new(MAX_ROW, MAX_COLUMN)
}

#[derive(Clone, Copy, Default, PartialEq)]
struct SplitAxes {
    x: [f64; 2],
    y: [f64; 2],
}
#[derive(Clone, Copy)]
struct PaneContext {
    index: u8,
    layout: RwSignal<wire::Geometry>,
    loaded: RwSignal<bool>,
    source: RwSignal<Option<(u64, calc_value::SheetId)>>,
    active: RwSignal<u8>,
    axes: RwSignal<SplitAxes>,
    zoom: RwSignal<f64>,
    canvas: RwSignal<bool>,
}
impl PaneContext {
    fn split(self) -> bool {
        self.layout.get_untracked().split_window && self.layout.get_untracked().split.is_some()
    }
    fn ready(self, controller: Controller) -> bool {
        self.loaded.get_untracked()
            && self.source.get_untracked()
                == controller
                    .sheet
                    .get_untracked()
                    .map(|s| (controller.generation.get_untracked(), s))
    }
    fn activate(self, controller: Controller) {
        if self.active.get_untracked() != self.index {
            controller.carry_edit_to_bar();
            self.active.set(self.index);
        }
        if let Some(sheet) = controller.sheet.get_untracked() {
            controller.remember_active_grid_pane(sheet, self.index);
        }
    }
}
pub(crate) fn grid_workspace(controller: Controller) -> impl IntoView {
    let canvas = RwSignal::new(painter::initial_canvas());
    let layout = RwSignal::new(wire::Geometry::default());
    let loaded = RwSignal::new(false);
    let active = RwSignal::new(0u8);
    let axes = RwSignal::new(SplitAxes::default());
    let zoom = RwSignal::new(1.);
    let held = RwSignal::new(None);
    let epoch = StoredValue::new(0u64);
    Effect::new(move |_| {
        controller.revision.track();
        let generation = controller.generation.get();
        let Some(sheet) = controller.sheet.get() else {
            return;
        };
        let first = held.get_untracked() != Some((generation, sheet));
        if first {
            loaded.set(false);
        }
        epoch.update_value(|n| *n += 1);
        let ticket = epoch.get_value();
        controller.ask(
            wire::Request::Geometry(wire::Sheet::of(sheet)),
            move |reply| {
                if epoch.try_get_value() != Some(ticket)
                    || controller.generation.get_untracked() != generation
                    || controller.sheet.get_untracked() != Some(sheet)
                {
                    return;
                }
                if let wire::Reply::Geometry(g) = reply {
                    controller.remember_grid_geometry(sheet, g.clone());
                    let slots = panes::split_slots(&g);
                    let selected = if first {
                        controller.active_grid_pane(sheet).unwrap_or(g.active_pane)
                    } else {
                        active.get_untracked()
                    };
                    let selected = if slots.contains(&selected) {
                        selected
                    } else {
                        0
                    };
                    if active.get_untracked() != selected {
                        active.set(selected);
                    }
                    if first || panes::split_slots(&layout.get_untracked()) != slots {
                        let initial_zoom=controller.grid_view_for(sheet,selected).map_or_else(||g.zoom.map_or(1.,|z|viewport::clamp_zoom(f64::from(z)/100.)),|v|v.zoom);
                        let m = sheet_metrics(Some(&g.geometry())).scaled_for_display(initial_zoom, window().device_pixel_ratio());
                        let a = g.top_left.unwrap_or((1, 1));
                        let b = g.pane_top_left.unwrap_or(a);
                        let mut initial = SplitAxes {
                            x: [m.col_start(a.1), m.col_start(b.1)],
                            y: [m.row_start(a.0), m.row_start(b.0)],
                        };
                        for index in slots {
                            if let Some(view) = controller.grid_view_for(sheet, index) {
                                initial.x[usize::from(index & 1)] = view.viewport.scroll_x;
                                initial.y[usize::from(index >> 1)] = view.viewport.scroll_y;
                            }
                        }
                        axes.set(initial);
                        zoom.set(initial_zoom);
                        held.set(Some((generation, sheet)));
                    }
                    if layout.get_untracked() != g {
                        layout.set(g);
                    }
                    if !loaded.get_untracked() {
                        loaded.set(true);
                    }
                }
            },
        );
    });
    let frame = NodeRef::<leptos::html::Div>::new();
    Effect::new(move |_| {
        let g = layout.get();
        if let Some(element) = frame.get() {
            css(element.into(), panes::split_css(&g));
        }
    });
    let ctx = PaneContext {
        index: 0,
        layout,
        loaded,
        source: held,
        active,
        axes,
        zoom,
        canvas,
    };
    view! {<div class="dc-grid-workspace" class:dc-split=move ||layout.get().split_window&&layout.get().split.is_some() node_ref=frame>
        <For each=move ||panes::split_slots(&layout.get()) key=|id|*id children=move |index|grid_pane(controller,PaneContext {index,..ctx})/>
    </div>}
}

fn grid_pane(controller: Controller, ctx: PaneContext) -> impl IntoView {
    let initial = controller
        .sheet
        .get_untracked()
        .and_then(|s| controller.grid_view_for(s, ctx.index));
    let initial_geometry = controller
        .sheet
        .get_untracked()
        .and_then(|s| controller.grid_geometry(s));
    let zoom = ctx.zoom;
    let zoom_text = RwSignal::new(String::new());
    let split_x=RwSignal::new("120".to_owned());let split_y=RwSignal::new("90".to_owned());
    Effect::new(move |_| zoom_text.set(format!("{:.0}", zoom.get() * 100.0)));
    let apply_zoom = move |text: String| match viewport::parse_zoom_percent(&text) {
        Ok(value) => zoom.set(value),
        Err(reason) => {
            controller.say(reason, true);
            zoom_text.set(format!("{:.0}", zoom.get_untracked() * 100.0));
        }
    };
    let viewed_sheet = StoredValue::new(None::<(u64, calc_value::SheetId)>);
    let restoring = StoredValue::new(false);
    let skip_reveal = StoredValue::new(false);
    let last_reveal_epoch = StoredValue::new(controller.reveal_epoch.get_untracked());
    let held_origin = StoredValue::new(None::<(u32, u32)>);
    let viewport = RwSignal::new(initial.map(|s| s.viewport).unwrap_or(Viewport {
        scroll_x: 0.0,
        scroll_y: 0.0,
        width: 1000.0,
        height: 560.0,
    }));
    // The sheet's sizes, its used area and its charts, asked of the workbook's server again
    // when the sheet or the workbook changes (dc-p96).
    let geometry: RwSignal<Option<wire::Geometry>> =
        RwSignal::new(initial_geometry.as_ref().map(wire::Geometry::geometry));
    let panes = RwSignal::new(
        initial_geometry
            .as_ref()
            .map_or(Panes::default(), Panes::from_geometry),
    );
    let split_window = RwSignal::new(initial_geometry.as_ref().is_some_and(|g| g.split_window));
    let used: RwSignal<Option<GridArea>> = RwSignal::new(None);
    let charts: RwSignal<Vec<wire::Chart>> = RwSignal::new(Vec::new());
    Effect::new(move |_| {
        controller.revision.track();
        let Some(sheet) = controller.sheet.get() else {
            return;
        };
        let sheet = wire::Sheet::of(sheet);
        controller.ask(wire::Request::UsedArea(sheet), move |reply| {
            if controller.sheet.get_untracked() != Some(sheet.id()) {
                return;
            }
            if let wire::Reply::UsedArea(area) = reply {
                let area = area.and_then(wire::Area::area);
                if used.try_get_untracked().is_some_and(|old| old != area) {
                    used.try_set(area);
                }
            }
        });
        controller.ask(wire::Request::Charts(sheet), move |reply| {
            if controller.sheet.get_untracked() != Some(sheet.id()) {
                return;
            }
            if let wire::Reply::Charts(drawn) = reply
                && charts.try_get_untracked().is_some_and(|old| old != drawn)
            {
                charts.try_set(drawn);
            }
        });
    });
    Effect::new(move |_| {
        let g = ctx.layout.get();
        let next = Panes::from_geometry(&g);
        if panes.get_untracked() != next {
            panes.set(next);
        }
        let geo = Some(g.geometry());
        if geometry.get_untracked() != geo {
            geometry.set(geo);
        }
        split_window.set(g.split_window);
    });
    let mirror = controller.new_mirror();
    // A value/style publication carries its own geometry. The query snapshot is only the
    // initial layout while this subscription has no committed publication for the sheet.
    let published_geometry = Memo::new(move |_| {
        let sheet = controller.sheet.get();
        mirror.with(|mirror| sheet.and_then(|sheet| mirror.geometry(sheet).cloned()))
    });
    let fallback_panes = panes;
    let panes = Memo::new(move |_| {
        published_geometry
            .get()
            .as_ref()
            .map(Panes::from_geometry)
            .unwrap_or_else(|| fallback_panes.get())
    });
    let sheet_metrics = Memo::new(move |_| {
        if let Some(g) = published_geometry.get() {
            sheet_metrics(Some(&g))
        } else {
            geometry.with(|g| sheet_metrics(g.as_ref()))
        }
    });
    let metrics = Memo::new(move |_| sheet_metrics.with(|m| m.scaled_for_display(zoom.get(), window().device_pixel_ratio())));
    let presentation = Memo::new(
        move |previous: Option<&dna_calc_model::presentation::Presentation>| {
            dna_calc_model::presentation::Presentation::update(
                previous,
                metrics.get(),
                viewport.get(),
                controller.font_revision.get(),
            )
        },
    );
    let subscription: StoredValue<Option<(u64, u64)>> = StoredValue::new(None);
    let scroller: NodeRef<leptos::html::Div> = NodeRef::new();
    let layer: NodeRef<leptos::html::Div> = NodeRef::new();
    let assist = controller.assist.another_field();
    controller.register_editor_surface(
        dna_calc_model::SurfaceDescriptor::editor_with_navigation(EditOrigin::Cell, SkinId::Grid),
        assist,
    );
    let editor = assist.field;
    let note = RwSignal::new(String::new());
    // Whether the active cell's validation list is open.
    let list_open = RwSignal::new(false);
    let list_index = RwSignal::new(0usize);
    let listbox = NodeRef::<leptos::html::Ul>::new();
    let rule: RwSignal<Option<wire::Validation>> = RwSignal::new(None);
    Effect::new(move |_| {
        if list_open.get()
            && let Some(list) = listbox.get()
        {
            let _ = list.focus();
        }
    });

    // Whether a press on a cell is being dragged to extend the selection.
    let dragging = StoredValue::new(false);
    let pointer_down = StoredValue::new(false);
    let point_drag = StoredValue::new(false);
    let drag_pointer = StoredValue::new(None::<(f64, f64)>);
    let drag_timer = StoredValue::new(None::<IntervalHandle>);
    let resizing = RwSignal::new(None::<axis::ResizeDrag>);
    let resize_cursor = RwSignal::new("default");
    let range_drag = RwSignal::new(None::<(GridArea, (u32, u32), u64, u64)>);
    let range_target = RwSignal::new(None::<GridArea>);
    let range_hover = RwSignal::new(false);
    let range_copy = RwSignal::new(false);
    let fill_from = RwSignal::new(None::<GridArea>);
    let fill_target = RwSignal::new(None::<(GridArea, bool)>);

    let grid_extent = Memo::new(move |_| {
        extent(
            used.get(),
            controller.active.get(),
            &metrics.get(),
            &viewport.get(),
        )
    });
    let interest = Memo::new(move |_| {
        let Some(sheet) = controller.sheet.get() else {
            return Vec::new();
        };
        let mut areas =
            panes
                .get()
                .interests(sheet, &metrics.get(), &viewport.get(), grid_extent.get());
        let (row, col) = controller.active.get();
        if let Ok(at) = CellKey::new(sheet, row, col) {
            let at = published_geometry
                .get()
                .as_ref()
                .map_or(at, |g| g.anchor(at));
            if !areas.iter().any(|a| a.contains(at)) {
                areas.push(GridArea::cell(at));
            }
        }
        areas
    });

    // Subscribe to the area in view, and move the interest as the view moves. A new workbook is a
    // new generation, whose session holds none of the old subscriptions.
    Effect::new(move |_| {
        let generation = controller.generation.get();
        let areas = interest.get();
        if areas.is_empty() {
            return;
        }
        let interest = wire::Interest::Areas(areas.into_iter().map(wire::Area::of).collect());
        match subscription.get_value() {
            Some((held, id)) if held == generation => {
                controller.set_interest(id, interest);
            }
            _ => {
                let id = controller.subscribe(interest, mirror);
                subscription.set_value(Some((generation, id)));
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
        let shown: Vec<cells::CellShown> = mirror.with(|mirror| {
            mirror
                .cells()
                .filter(|(at, _)| Some(at.sheet()) == sheet)
                .filter_map(|(_, cell)| cell.shown())
                .collect()
        });
        let extent = grid_extent.get();
        let presentation = presentation.get();
        let published_panes = published_geometry
            .get()
            .as_ref()
            .map(Panes::from_geometry)
            .unwrap_or_else(|| panes.get());
        let fonts = controller.font_revision.get();
        let measure_text = |cell: &cells::CellShown, text: &str| {
            autofit::plan_advance(cell, text, presentation.metrics.scale, fonts)
        };
        let published = published_geometry.get();
        let options = render_plan::PlanOptions {
            geometry: published.as_ref(),
            measure: Some(&measure_text),
        };
        let mut plan = published_panes.plan_with(
            &shown,
            &presentation.metrics,
            &presentation.viewport,
            extent.rows,
            extent.cols,
            &options,
        );
        autofit::align_row_baselines(&mut plan, &shown, published.as_ref(), presentation.metrics.scale);
        plan.presentation = presentation.stamp;
        if let Some(sheet) = sheet {
            mirror.with(|mirror| {
                plan.attach_semantics(
                    mirror,
                    sheet,
                    &controller.areas.get(),
                    controller.active.get(),
                )
            });
        }
        plan
    });
    inspect::register(plan, ctx, controller);
    let canvas_node = NodeRef::<leptos::html::Canvas>::new();
    let colors = painter::watch_colors();
    Effect::new(move |_| {
        let enabled = ctx.canvas.get();
        if let Some(grid) = scroller.get() {
            let _ = grid.set_attribute("data-painter", if enabled { "canvas" } else { "dom" });
            // This surface already reaches Tachys's 26-attribute tuple limit.
            let _ = grid.set_attribute("aria-multiselectable", "true");
            plan.with(|plan| {
                let _ = grid.set_attribute(
                    "aria-busy",
                    if plan.interrupted || plan.publication.is_none() {
                        "true"
                    } else {
                        "false"
                    },
                );
                if let Some(stamp) = plan.publication {
                    let _ = grid.set_attribute("data-paint-workbook", &stamp.workbook.to_string());
                    let _ = grid.set_attribute("data-paint-revision", &stamp.revision.to_string());
                }
            });
        }
        colors.track();
        controller.active_skin.track();
        if let Some(node) = canvas_node.get() {
            if enabled {
                plan.with(|plan| painter::paint(&node, plan, panes.get(), &metrics.get()));
            } else {
                node.set_width(0);
                node.set_height(0);
            }
        }
    });

    let scales = move || {
        let m = metrics.get_untracked();
        let v = viewport.get_untracked();
        (
            viewport::ScrollScale::new(m.header_w + m.col_start(MAX_COLUMN + 1), v.width),
            viewport::ScrollScale::new(m.header_h + m.row_start(MAX_ROW + 1), v.height),
        )
    };
    // Read the current DOM position too when a skin or sheet is left: a scroll event can still be queued.
    let read_viewport = move || {
        if let Some(element) = scroller.get_untracked() {
            let (width, height) = (
                f64::from(element.client_width()),
                f64::from(element.client_height()),
            );
            let (scroll_x, scroll_y) = (
                js_sys::Reflect::get(element.as_ref(), &"scrollLeft".into())
                    .ok()
                    .and_then(|n| n.as_f64())
                    .unwrap_or_else(|| f64::from(element.scroll_left())),
                js_sys::Reflect::get(element.as_ref(), &"scrollTop".into())
                    .ok()
                    .and_then(|n| n.as_f64())
                    .unwrap_or_else(|| f64::from(element.scroll_top())),
            );
            let m = metrics.get_untracked();
            let scale_x =
                viewport::ScrollScale::new(m.header_w + m.col_start(MAX_COLUMN + 1), width);
            let scale_y = viewport::ScrollScale::new(m.header_h + m.row_start(MAX_ROW + 1), height);
            (width > 0.0 && height > 0.0).then_some(Viewport {
                scroll_x: scale_x.logical(scroll_x),
                scroll_y: scale_y.logical(scroll_y),
                width,
                height,
            })
        } else {
            None
        }
    };
    // The view's size follows the scroller's.
    let measure = move || {
        if let Some(next) = read_viewport() {
            if viewport.get_untracked() != next {
                viewport.set(next);
            }
            if ctx.ready(controller)
                && ctx.split()
                && !restoring.get_value()
                && viewed_sheet.get_value()
                    == controller
                        .sheet
                        .get_untracked()
                        .map(|s| (controller.generation.get_untracked(), s))
            {
                let mut axes = ctx.axes.get_untracked();
                axes.x[usize::from(ctx.index & 1)] = next.scroll_x;
                axes.y[usize::from(ctx.index >> 1)] = next.scroll_y;
                if ctx.axes.get_untracked() != axes {
                    ctx.axes.set(axes);
                }
            }
            if !restoring.get_value()
                && let Some((generation, sheet)) = viewed_sheet.get_value()
                && generation == controller.generation.get_untracked()
                && Some(sheet) == controller.sheet.get_untracked()
            {
                controller.remember_grid_view_for(
                    sheet,
                    ctx.index,
                    viewport::GridView {
                        viewport: next,
                        zoom: zoom.get_untracked(),
                        active: controller.active.get_untracked(),
                    },
                );
            }
        }
    };
    Effect::new(move |_| {
        if scroller.get().is_some() {
            measure();
        }
    });

    Effect::new(move |_| {
        let axes = ctx.axes.get();
        let z = zoom.get();
        ctx.source.track();
        ctx.loaded.track();
        if !ctx.ready(controller)
            || !ctx.split()
            || restoring.get_value()
            || viewed_sheet.get_value().is_none()
        {
            return;
        }
        let _ = z;
        if let Some(element) = scroller.get() {
            let v = viewport.get_untracked();
            let x = axes.x[usize::from(ctx.index & 1)];
            let y = axes.y[usize::from(ctx.index >> 1)];
            if (v.scroll_x - x).abs() > 0.5 || (v.scroll_y - y).abs() > 0.5 {
                let (sx, sy) = scales();
                element.scroll_to_with_x_and_y(sx.physical(x), sy.physical(y));
                measure();
            }
        }
    });
    // Scrolls the active cell into view, moving as little as possible.
    let reveal = move |row: u32, col: u32| {
        if !ctx.ready(controller) || ctx.active.get_untracked() != ctx.index {
            return;
        }
        if let Some(held) = held_origin.get_value() {
            if held == (row, col) && controller.editing.get_untracked().is_none() {
                return;
            }
            held_origin.set_value(None);
        }
        let Some(element) = scroller.get_untracked() else {
            return;
        };
        let m = metrics.get_untracked();
        let v = viewport.get_untracked();
        let (x, y) = panes.get_untracked().reveal(&m, &v, row, col);
        let (sx, sy) = scales();
        if x != v.scroll_x || y != v.scroll_y {
            element.scroll_to_with_x_and_y(sx.physical(x), sy.physical(y));
        }
        measure();
    };
    let resize = window_event_listener(leptos::ev::resize, move |_| {
        measure();
        let active = controller.active.get_untracked();
        let sheet = controller.sheet.get_untracked();
        let generation = controller.generation.get_untracked();
        reveal(active.0, active.1);
        request_animation_frame(move || {
            if scroller.try_get_untracked().flatten().is_some()
                && controller.generation.get_untracked() == generation
                && controller.sheet.get_untracked() == sheet
                && controller.active.get_untracked() == active
            {
                measure();
                reveal(active.0, active.1);
            }
        });
    });
    on_cleanup(move || resize.remove());
    Effect::new(move |_| {
        let generation = controller.generation.get();
        let Some(sheet) = controller.sheet.get() else {
            return;
        };
        if let Some((old_generation, old)) = viewed_sheet.get_value()
            && old_generation == generation
            && old != sheet
        {
            controller.remember_grid_view_for(
                old,
                ctx.index,
                viewport::GridView {
                    viewport: read_viewport().unwrap_or_else(|| viewport.get_untracked()),
                    zoom: zoom.get_untracked(),
                    active: controller.active.get_untracked(),
                },
            );
            viewed_sheet.set_value(None);
        }
        if !ctx.loaded.get() || ctx.source.get() != Some((generation, sheet)) {
            return;
        }
        let Some(element) = scroller.get() else {
            return;
        };
        if viewed_sheet.get_value() == Some((generation, sheet)) {
            return;
        }
        restoring.set_value(true);
        viewed_sheet.set_value(Some((generation, sheet)));
        let cached_geometry = controller.grid_geometry(sheet);
        geometry.set(cached_geometry.as_ref().map(wire::Geometry::geometry));
        fallback_panes.set(
            cached_geometry
                .as_ref()
                .map_or(Panes::default(), Panes::from_geometry),
        );
        split_window.set(cached_geometry.as_ref().is_some_and(|g| g.split_window));
        let cached = controller.grid_view_for(sheet, ctx.index);
        zoom.set(cached.map_or_else(||ctx.layout.get_untracked().zoom.map_or(1.,|z|viewport::clamp_zoom(f64::from(z)/100.)), |view| view.zoom));
        let imported = Panes::from_geometry(&ctx.layout.get_untracked()).imported_scroll(
            &ctx.layout.get_untracked(),
            &metrics.get_untracked(),
            ctx.index,
        );
        let (x, y) = cached.map_or(imported, |view| {
            (view.viewport.scroll_x, view.viewport.scroll_y)
        });
        let (sx, sy) = scales();
        element.scroll_to_with_x_and_y(sx.physical(x), sy.physical(y));
        measure();
        if cached.is_none()
            && (ctx.layout.get_untracked().top_left.is_some()
                || ctx.layout.get_untracked().pane_top_left.is_some())
        {
            held_origin.set_value(Some(
                ctx.layout
                    .get_untracked()
                    .active_cell
                    .unwrap_or_else(|| controller.active.get_untracked()),
            ));
        }
        skip_reveal.set_value(
            cached.is_some_and(|view| view.active == controller.active.get_untracked())
                && controller.editing.get_untracked().is_none(),
        );
        restoring.set_value(false);
    });
    Effect::new(move |_| {
        let z = zoom.get();
        if !restoring.get_value()
            && let Some((generation, sheet)) = viewed_sheet.get_value()
            && generation == controller.generation.get_untracked()
            && Some(sheet) == controller.sheet.get_untracked()
        {
            controller.remember_grid_view_for(
                sheet,
                ctx.index,
                viewport::GridView {
                    viewport: viewport.get_untracked(),
                    zoom: z,
                    active: controller.active.get_untracked(),
                },
            );
        }
    });
    on_cleanup(move || {
        if let Some((generation, sheet)) = viewed_sheet.get_value()
            && generation == controller.generation.get_untracked()
        {
            controller.remember_grid_view_for(
                sheet,
                ctx.index,
                viewport::GridView {
                    viewport: read_viewport().unwrap_or_else(|| viewport.get_untracked()),
                    zoom: zoom.get_untracked(),
                    active: controller.active.get_untracked(),
                },
            );
        }
    });
    // However the active cell moves (keys, the name box, an undo), it is brought into view.
    Effect::new(move |_| {
        let epoch = controller.reveal_epoch.get();
        let explicit = epoch != last_reveal_epoch.get_value();
        last_reveal_epoch.set_value(epoch);
        if explicit {
            held_origin.set_value(None);
        }
        metrics.track();
        panes.track();
        ctx.source.track();
        ctx.loaded.track();
        ctx.active.track();
        controller.revision.track();
        controller.editing.track();
        let (row, col) = controller.active.get();
        if restoring.get_value()
            || viewed_sheet.get_value().map(|(_, s)| s) != controller.sheet.get_untracked()
        {
            return;
        }
        if skip_reveal.get_value() {
            skip_reveal.set_value(false);
            if !explicit {
                return;
            }
        }
        if scroller.get().is_some() {
            measure();
            reveal(row, col);
            let requested_sheet = controller.sheet.get_untracked();
            let requested_generation = controller.generation.get_untracked();
            request_animation_frame(move || {
                if scroller.try_get_untracked().flatten().is_none()
                    || metrics.try_get_untracked().is_none()
                    || controller.active.try_get_untracked() != Some((row, col))
                    || controller.sheet.get_untracked() != requested_sheet
                    || controller.generation.get_untracked() != requested_generation
                {
                    return;
                }
                measure();
                reveal(row, col);
            });
        }
    });
    // The formula tools and result disclosure can resize the workspace without a window
    // resize. Observe the actual scroller so an active editor remains visible.
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let observer = StoredValue::new_local(
            None::<(
                web_sys::ResizeObserver,
                wasm_bindgen::closure::Closure<dyn FnMut()>,
            )>,
        );
        Effect::new(move |_| {
            if let Some(element) = scroller.get()
                && observer.with_value(Option::is_none)
            {
                let callback = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
                    let before = viewport.get_untracked();
                    measure();
                    let after = viewport.get_untracked();
                    if (before.width, before.height) != (after.width, after.height)
                        && !restoring.get_value()
                    {
                        let (row, col) = controller.active.get_untracked();
                        reveal(row, col);
                    }
                });
                if let Ok(watch) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
                    watch.observe(&element);
                    observer.set_value(Some((watch, callback)));
                }
            }
        });
        on_cleanup(move || {
            observer.with_value(|observer| {
                if let Some((watch, _)) = observer {
                    watch.disconnect();
                }
            })
        });
    }
    let focus_grid = move || {
        if let Some(element) = scroller.get_untracked() {
            let _ = element.focus();
        }
    };
    let move_to = move |row: u32, col: u32| controller.select(row, col);
    let nav_to = move |nav: selection::Navigation, extend: bool| {
        let old_view = viewport.get_untracked();
        let page_height = (old_view.height / zoom.get_untracked()) as u32;
        controller.navigate_then(nav, extend, page_height, move |outcome| {
            if !outcome.accepted
                || outcome.selection.is_none()
                || scroller.try_get_untracked().flatten().is_none()
                || metrics.try_get_untracked().is_none()
            {
                return;
            }
            if let selection::Navigation::Page { down } = nav {
                let m = metrics.get_untracked();
                let distance = (old_view.height - m.header_h).max(m.row_height);
                if let Some(element) = scroller.get_untracked() {
                    let (sx, sy) = scales();
                    element.scroll_to_with_x_and_y(
                        sx.physical(old_view.scroll_x),
                        sy.physical(old_view.scroll_y + if down { distance } else { -distance }),
                    );
                    measure();
                }
            }
            if extend {
                let (row, col) = controller.corner.get_untracked();
                reveal(row, col);
            }
        });
    };
    let go = move |action: NavAction| nav_to(selection::Navigation::Step(action), false);
    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        if ev.is_composing() || ev.key_code() == 229 {
            return;
        }
        if resizing.get_untracked().is_some() {
            ev.prevent_default();
            if ev.key() == "Escape" {
                resizing.set(None);
                note.set("Resize cancelled".to_owned());
            }
            return;
        }
        if ev.key() == "Escape" && range_drag.get_untracked().is_some() {
            ev.prevent_default();
            range_drag.set(None);
            range_target.set(None);
            range_hover.set(false);
            pointer_down.set_value(false);
            drag_pointer.set_value(None);
            drag_timer.update_value(|timer| {if let Some(timer) = timer.take() {timer.clear();}});
            note.set("Drag cancelled".to_owned());
            return;
        }
        if ev.key() == "Escape" {
            fill_from.set(None);
            fill_target.set(None);
            controller.clipboard.set(None);
            controller.copied.set(None);
            controller.cut.set(false);
        }
        if controller.editing.get_untracked().is_some() {
            use wasm_bindgen::JsCast;
            if ev
                .target()
                .is_some_and(|target| target.dyn_ref::<web_sys::HtmlTextAreaElement>().is_some())
            {
                return;
            }
            controller.focus_editor();
            return;
        }
        let ctrl = ev.ctrl_key() || ev.meta_key();
        let chord = KeyChord::new(ev.key(), ev.shift_key(), ctrl, ev.alt_key());
        if chord.key == "F10" && ev.shift_key() && !ctrl && !ev.alt_key() {
            ev.prevent_default();
            controller.after_input(move || {
                let (row, col) = controller.active.get_untracked();
                let cell = panes.get_untracked().rect(
                    &metrics.get_untracked(),
                    &viewport.get_untracked(),
                    row,
                    col,
                );
                if let Some(layer) = layer.get_untracked() {
                    let rect = layer.get_bounding_client_rect();
                    controller
                        .context_menu
                        .set(Some((rect.left() + cell.x, rect.top() + cell.y + cell.h)));
                }
            });
            return;
        }
        if ctrl && !ev.alt_key() && matches!(chord.key.as_str(), "PageUp" | "PageDown") {
            ev.prevent_default();
            let sheets = controller.sheets();
            if let Some(index) = sheets
                .iter()
                .position(|s| Some(s.id) == controller.sheet.get_untracked())
            {
                let next = if chord.key == "PageUp" {
                    index.saturating_sub(1)
                } else {
                    (index + 1).min(sheets.len() - 1)
                };
                controller.show_sheet(sheets[next].id);
                focus_grid();
            }
            return;
        }
        if ctrl && !ev.alt_key() {
            match chord.key.as_str() {
                "c" => {
                    ev.prevent_default();
                    controller.copy();
                    return;
                }
                "x" => {
                    ev.prevent_default();
                    controller.cut();
                    return;
                }
                "v" => return, // Native paste events supply actual system data.
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
        // Alt+Down opens the active cell's validation list, as in Excel; Escape closes it.
        if ev.alt_key() && chord.key == "ArrowDown" {
            ev.prevent_default();
            if rule.with_untracked(|r| r.as_ref().is_some_and(|r| r.dropdown)) {
                list_open.set(true);
            }
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
                SheetAction::Extend(nav) => nav_to(selection::Navigation::Step(nav), true),
                SheetAction::Navigate(nav, extend) => nav_to(nav, extend),
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
        let target = panes.get_untracked().hit(
            &metrics.get_untracked(),
            &viewport.get_untracked(),
            extent.rows,
            extent.cols,
            x,
            y,
        );
        Some(if let HitTarget::Cell { row, col } = target {
            controller
                .sheet
                .get_untracked()
                .and_then(|sheet| CellKey::new(sheet, row, col).ok())
                .and_then(|at| published_geometry.get_untracked().map(|g| g.anchor(at)))
                .map_or(target, |at| HitTarget::Cell {
                    row: at.row(),
                    col: at.column(),
                })
        } else {
            target
        })
    };
    let stop_drag = move || {
        pointer_down.set_value(false);
        dragging.set_value(false);
        point_drag.set_value(false);
        drag_pointer.set_value(None);
        drag_timer.update_value(|timer| {
            if let Some(timer) = timer.take() {
                timer.clear();
            }
        });
    };
    let drag_tick = move || {
        if (!dragging.get_value() && !point_drag.get_value() && fill_from.get_untracked().is_none() && range_drag.get_untracked().is_none())
            || document().hidden()
        {
            return;
        }
        let Some((x, y)) = drag_pointer.get_value() else {
            return;
        };
        let (Some(element), Some(layer)) = (scroller.get_untracked(), layer.get_untracked()) else {
            return;
        };
        let rect = layer.get_bounding_client_rect();
        let (x, y) = (x - rect.left(), y - rect.top());
        let m = metrics.get_untracked();
        let v = viewport.get_untracked();
        let dx = if x < m.header_w {
            -(m.header_w - x).min(m.col_width * 3.0)
        } else if x >= v.width {
            (x - v.width + 1.0).min(m.col_width * 3.0)
        } else {
            0.0
        };
        let dy = if y < m.header_h {
            -(m.header_h - y).min(m.row_height * 3.0)
        } else if y >= v.height {
            (y - v.height + 1.0).min(m.row_height * 3.0)
        } else {
            0.0
        };
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let (sx, sy) = scales();
        element.scroll_to_with_x_and_y(sx.physical(v.scroll_x + dx), sy.physical(v.scroll_y + dy));
        measure();
        if let HitTarget::Cell { row, col } = panes.get_untracked().hit(
            &m,
            &viewport.get_untracked(),
            MAX_ROW,
            MAX_COLUMN,
            x.clamp(m.header_w + 1.0, (v.width - 1.0).max(m.header_w + 1.0)),
            y.clamp(m.header_h + 1.0, (v.height - 1.0).max(m.header_h + 1.0)),
        ) {
            if let Some((from, grip, _, _)) = range_drag.get_untracked() {
                range_target.set(dna_calc_model::clipboard::drag_destination(from, grip, row, col));
            } else if let Some(from) = fill_from.get_untracked() {
                fill_target.set(crate::clipboard::fill_extension(from, row, col));
            } else if point_drag.get_value() {
                controller.point_at(row, col, true);
            } else {
                controller.extend((row, col));
            }
        }
    };
    let start_drag = move || {
        if drag_timer.get_value().is_none() {
            drag_timer.set_value(
                set_interval_with_handle(drag_tick, std::time::Duration::from_millis(60)).ok(),
            );
        }
    };
    on_cleanup(stop_drag);
    let press = move |target: HitTarget, shift: bool, ctrl: bool| {
        match target {
            HitTarget::Cell { row, col } if ctrl => controller.toggle_cell(row, col),
            HitTarget::Cell { row, col } if shift => controller.extend((row, col)),
            HitTarget::Cell { row, col } => {
                move_to(row, col);
                if pointer_down.get_value() {
                    dragging.set_value(true);
                    start_drag();
                }
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
    let semantic_press = move |target: HitTarget, shift: bool, ctrl: bool| {
        if controller.assist.composing.get_untracked()
            || scroller.try_get_untracked().flatten().is_none()
        {
            return;
        }
        if let HitTarget::Cell { row, col } = target
            && controller.point_at(row, col, shift)
        {
            if pointer_down.get_value() {
                point_drag.set_value(true);
                start_drag();
            }
            return;
        }
        // An ordinary cell press enters the draft first; rejection keeps focus and source.
        if controller.editing.get_untracked().is_some() {
            controller.commit_edit(move |accepted| {
                if accepted {
                    press(target, shift, ctrl);
                } else {
                    controller.focus_editor();
                }
            });
        } else {
            press(target, shift, ctrl);
        }
    };
    let on_contextmenu = move |ev: leptos::ev::MouseEvent| {
        if ev.shift_key() {
            return;
        }
        ev.prevent_default();
        let hit = point(&ev);
        let x = f64::from(ev.client_x());
        let y = f64::from(ev.client_y());
        controller.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                controller.focus_editor();
                return;
            }
            if let Some(HitTarget::Cell { row, col }) = hit {
                if let Some(sheet) = controller.sheet.get_untracked()
                    && let Ok(at) = CellKey::new(sheet, row, col)
                    && !controller
                        .areas
                        .with_untracked(|areas| areas.iter().any(|a| a.contains(at)))
                {
                    controller.select(row, col);
                }
            }
            match hit {
                Some(
                    hit @ (HitTarget::ColumnHeader { .. }
                    | HitTarget::RowHeader { .. }
                    | HitTarget::Corner),
                ) => press(hit, false, false),
                None | Some(HitTarget::Outside) => return,
                _ => {}
            }
            controller.context_menu.set(Some((x, y)));
        });
    };
    // Hit only the existing selection's border; its rectangles come from the common pane map.
    let selection_edge = move |ev: &leptos::ev::MouseEvent| {
        if controller.editing.get_untracked().is_some() || controller.areas.get_untracked().len() != 1 {
            return None;
        }
        let area = controller.selected_area_untracked()?;
        if area.cell_count() > calc_value::COPY_LIMIT { return None; }
        let layer = layer.get_untracked()?;
        let bounds = layer.get_bounding_client_rect();
        let x = f64::from(ev.client_x()) - bounds.left();
        let y = f64::from(ev.client_y()) - bounds.top();
        let m = metrics.get_untracked();
        let tolerance = (m.row_height.min(m.col_width) / 4.0).clamp(0.25, 3.0);
        let near = panes.get_untracked().area_rects(area, &m, &viewport.get_untracked()).iter().any(|r| {
            x >= r.x - tolerance && x <= r.x + r.w + tolerance && y >= r.y - tolerance && y <= r.y + r.h + tolerance
                && ((x-r.x).abs() <= tolerance || (x-r.x-r.w).abs() <= tolerance || (y-r.y).abs() <= tolerance || (y-r.y-r.h).abs() <= tolerance)
        });
        if !near { return None; }
        let HitTarget::Cell { row, col } = point(ev)? else { return None; };
        Some((area, (row.clamp(area.first().row(),area.last().row()), col.clamp(area.first().column(),area.last().column()))))
    };
    let on_mousedown = move |ev: leptos::ev::MouseEvent| {
        if ev.button() != 0 {
            return;
        }
        if controller.editing.get_untracked().is_none() {
            if let (Some(layer), Some(sheet)) =
                (layer.get_untracked(), controller.sheet.get_untracked())
            {
                let rect = layer.get_bounding_client_rect();
                let edge = plan.with_untracked(|p| {
                    axis::boundary(
                        p,
                        &metrics.get_untracked(),
                        &viewport.get_untracked(),
                        f64::from(ev.client_x()) - rect.left(),
                        f64::from(ev.client_y()) - rect.top(),
                    )
                });
                if let Some(band) = edge {
                    ev.prevent_default();
                    focus_grid();
                    let start = if band.rows {
                        f64::from(ev.client_y())
                    } else {
                        f64::from(ev.client_x())
                    };
                    resizing.set(Some(axis::ResizeDrag {
                        band,
                        sheet,
                        generation: controller.generation.get_untracked(),
                        start,
                        current: start,
                        zoom: zoom.get_untracked(),
                    }));
                    return;
                }
            }
        }
        if !ev.shift_key() && !ev.alt_key() {
            if let Some((area, grip)) = selection_edge(&ev) {
                ev.prevent_default();
                focus_grid();
                range_drag.set(Some((area, grip, controller.generation.get_untracked(), controller.summary.get_untracked().revision)));
                range_target.set(None);
                range_copy.set(ev.ctrl_key() || ev.meta_key());
                range_hover.set(true);
                drag_pointer.set_value(Some((f64::from(ev.client_x()), f64::from(ev.client_y()))));
                pointer_down.set_value(true);
                start_drag();
                return;
            }
        }
        let target = point(&ev);
        let Some(target) = target else {
            return;
        };
        if matches!(target, HitTarget::Outside) {
            return;
        }
        ev.prevent_default();
        note.set(String::new());
        drag_pointer.set_value(Some((f64::from(ev.client_x()), f64::from(ev.client_y()))));
        let shift = ev.shift_key();
        let ctrl = ev.ctrl_key() || ev.meta_key();
        if controller.assist.composing.get_untracked() {
            return;
        }
        pointer_down.set_value(true);
        controller.after_input(move || semantic_press(target, shift, ctrl));
    };

    let on_mousemove = move |ev: leptos::ev::MouseEvent| {
        range_copy.set(ev.ctrl_key() || ev.meta_key());
        range_hover.set(selection_edge(&ev).is_some());
        if let Some((from, grip, _, _)) = range_drag.get_untracked() {
            drag_pointer.set_value(Some((f64::from(ev.client_x()), f64::from(ev.client_y()))));
            if ev.buttons() & 1 == 0 {
                range_drag.set(None);
                range_target.set(None);
                stop_drag();
            } else if let Some(HitTarget::Cell { row, col }) = point(&ev) {
                range_target.set(dna_calc_model::clipboard::drag_destination(from, grip, row, col));
            }
            return;
        }
        if let Some(mut drag) = resizing.get_untracked() {
            if ev.buttons() & 1 == 0 {
                resizing.set(None);
                return;
            }
            drag.current = if drag.band.rows {
                f64::from(ev.client_y())
            } else {
                f64::from(ev.client_x())
            };
            resizing.set(Some(drag));
            return;
        }
        if let Some(layer) = layer.get_untracked() {
            let rect = layer.get_bounding_client_rect();
            let edge = plan.with_untracked(|p| {
                axis::boundary(
                    p,
                    &metrics.get_untracked(),
                    &viewport.get_untracked(),
                    f64::from(ev.client_x()) - rect.left(),
                    f64::from(ev.client_y()) - rect.top(),
                )
            });
            resize_cursor.set(edge.map_or("default", |e| {
                if e.rows { "row-resize" } else { "col-resize" }
            }));
        }
        if let Some(from) = fill_from.get_untracked() {
            drag_pointer.set_value(Some((f64::from(ev.client_x()), f64::from(ev.client_y()))));
            if let Some(HitTarget::Cell { row, col }) = point(&ev) {
                fill_target.set(crate::clipboard::fill_extension(from, row, col));
            }
            return;
        }
        if pointer_down.get_value() || dragging.get_value() || point_drag.get_value() {
            drag_pointer.set_value(Some((f64::from(ev.client_x()), f64::from(ev.client_y()))));
            if ev.buttons() & 1 == 0 {
                stop_drag();
                return;
            }
        }
        if point_drag.get_value() {
            if ev.buttons() & 1 == 0 {
                point_drag.set_value(false);
            } else if let Some(HitTarget::Cell { row, col }) = point(&ev) {
                controller.point_at(row, col, true);
            }
            return;
        }
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
    let drag_move = window_event_listener(leptos::ev::mousemove, on_mousemove);
    let drag_up = window_event_listener(leptos::ev::mouseup, move |ev| {
        if ev.button() == 0 {
            if let Some((from, _, generation, revision)) = range_drag.get_untracked() {
                let to = range_target.get_untracked();
                range_drag.set(None); range_target.set(None); range_hover.set(false); stop_drag();
                if let Some(to) = to { controller.drag_range(from, to, ev.ctrl_key() || ev.meta_key(), generation, revision); }
                return;
            }
        }
        if let Some(mut drag) = resizing.get_untracked() {
            resizing.set(None);
            drag.current = if drag.band.rows {
                f64::from(ev.client_y())
            } else {
                f64::from(ev.client_x())
            };
            if (drag.current - drag.start).abs() >= 2.0 {
                controller.resize_axis(drag);
            }
            return;
        }
        let from = fill_from.get_untracked();
        let target = fill_target.get_untracked();
        fill_from.set(None);
        fill_target.set(None);
        stop_drag();
        if let (Some(from), Some((to, down))) = (from, target) {
            controller.prepare_paste_at(
                wire::PasteSource::Fill {
                    from: wire::Area::of(from),
                    down,
                    series: controller.fill_series.get_untracked() == "series",
                },
                to,
                true,
            );
        }
    });
    let drag_blur = window_event_listener(leptos::ev::blur, move |_| {
        range_drag.set(None); range_target.set(None); range_hover.set(false);
        resizing.set(None);
        fill_from.set(None);
        fill_target.set(None);
        stop_drag();
    });
    on_cleanup(move || {
        drag_move.remove();
        drag_up.remove();
        drag_blur.remove();
    });
    let on_dblclick = move |ev: leptos::ev::MouseEvent| {
        if controller.editing.get_untracked().is_some() {
            return;
        }
        if let Some(layer) = layer.get_untracked() {
            let rect = layer.get_bounding_client_rect();
            if let Some(band) = plan.with_untracked(|p| {
                axis::boundary(
                    p,
                    &metrics.get_untracked(),
                    &viewport.get_untracked(),
                    f64::from(ev.client_x()) - rect.left(),
                    f64::from(ev.client_y()) - rect.top(),
                )
            }) {
                ev.prevent_default();
                controller.autofit_header(band);
                return;
            }
        }
        if let Some(HitTarget::Cell { row, col }) = point(&ev) {
            move_to(row, col);
            controller.begin_edit(EditOrigin::Cell, None);
        }
    };

    // The editor over the active cell takes the focus when it opens, with the caret at the end.
    Effect::new(move |_| {
        if ctx.active.get() == ctx.index
            && controller.editing.get() == Some(EditOrigin::Cell)
            && editor.get().is_some()
        {
            controller.activate_editor(EditOrigin::Cell, assist);
            controller.focus_mounted_editor(assist);
        }
    });
    let on_editor_key = Callback::new(move |ev| controller.editor_key(ev, assist));

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

    // Physical fills precede text; physical borders follow it so merged backgrounds
    // and conditional text backgrounds cannot cover the authored edges.
    let decorations_view = move |borders: bool| {
        if ctx.canvas.get() {
            return Vec::new().collect_view();
        }
        plan.with(|plan| plan.cells.iter().filter(|cell| {
            if borders {cell.borders.iter().any(Option::is_some)||!cell.boundary.is_empty()||cell.shown.style.diagonal.iter().any(Option::is_some)} else {
                cell.shown.style.fill.is_some() || cell.shown.style.brush.is_some() || cell.shown.conditional.as_ref().is_some_and(|c|c.fill.is_some()||c.bar.is_some()) || cell.row_span>1 || cell.col_span>1
            }
        }).map(|cell| {
            let expanded = borders && cell.row_span == 1 && cell.col_span == 1;
            let rect = if borders {decoration::border_rect(cell,plan.scale)} else {cell.anchor};
            let mut style=rect_style(&rect);
            style.push_str(";position:absolute;box-sizing:border-box;pointer-events:none");
            style.push_str(&if expanded {panes.get().clip_style(rect,&metrics.get(),&viewport.get(),Some(cell.row),Some(cell.col))} else {panes.get().anchor_clip_style(cell,&metrics.get(),&viewport.get())});
            if borders {
                style.push_str(&decoration::css(&cell.borders,plan.scale));
            } else if let Some(fill)=cell.shown.conditional.as_ref().and_then(|c|c.fill.as_ref()).or(cell.shown.style.fill.as_ref()) {
                style.push_str(&format!(";background-color:{fill}"));
            } else if let Some(brush)=&cell.shown.style.brush {
                if let Some((_,url))=decoration::texture(brush,cell.anchor.w,cell.anchor.h,plan.scale) {
                    let (size,repeat)=if matches!(brush,dna_calc_model::projection::FillShown::Pattern {..}) {("8px 8px".to_owned(),"repeat")}else {("100% 100%".into(),"no-repeat")};
                    style.push_str(&format!(";background-image:url({url});background-size:{size};background-repeat:{repeat}"));
                }
            } else if cell.row_span>1 || cell.col_span>1 {
                style.push_str(";background-color:var(--dna-paper);border-right:1px solid var(--dc-gridline);border-bottom:1px solid var(--dc-gridline)");
            }
            if !borders && let Some((share,color,gradient))=cell.shown.conditional.as_ref().and_then(|c|c.bar.as_ref()) {
                let end=if *gradient {"#ffffff"}else {color.as_str()};
                let under=cell.shown.conditional.as_ref().filter(|c|c.fill.is_none()).and(cell.shown.style.brush.as_ref()).and_then(|brush|decoration::texture(brush,cell.anchor.w,cell.anchor.h,plan.scale)).map(|(_,url)|url);
                if let Some(url)=under {
                    let (size,repeat)=if matches!(cell.shown.style.brush,Some(dna_calc_model::projection::FillShown::Pattern {..})) {("8px 8px".to_owned(),"repeat")}else {("100% 100%".into(),"no-repeat")};
                    style.push_str(&format!(";background-image:linear-gradient(to right,{color},{end}),url({url});background-size:{:.1}% 72%,{size};background-position:left center,0 0;background-repeat:no-repeat,{repeat}",share*100.));
                }
                else {style.push_str(&format!(";background-image:linear-gradient(to right,{color},{end});background-size:{:.1}% 72%;background-position:left center;background-repeat:no-repeat",share*100.));}
            }
            let strokes=if borders {decoration::strokes(cell,plan.scale)}else {Vec::new()};
            let lines=(!strokes.is_empty()).then(|| {
                let left=cell.borders[0].as_ref().map_or(0.,|e|decoration::line(e,plan.scale).width);
                let top=cell.borders[2].as_ref().map_or(0.,|e|decoration::line(e,plan.scale).width);
                let svg_style=format!("position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;overflow:visible",if expanded {0.}else{-left},if expanded {0.}else{-top},cell.anchor.w,cell.anchor.h);
                view! {<svg data-dnacalc="cell-lines" viewBox=format!("0 0 {} {}",cell.anchor.w,cell.anchor.h) use:css=svg_style>
                    {strokes.into_iter().map(|line| {
                        let dash=line.dash.iter().map(|n|n.to_string()).collect::<Vec<_>>().join(" ");
                        view! {<line x1=line.x1 y1=line.y1 x2=line.x2 y2=line.y2 stroke=line.color stroke-width=line.width stroke-dasharray=dash fill="none"/>}
                    }).collect_view()}
                </svg>}
            });
            view!{<div aria-hidden="true" data-dnacalc=if borders {"cell-borders"} else {"cell-decoration"} data-cell=cell.address.clone() use:css=style>{lines}</div>}
        }).collect_view())
    };

    let cells_view = move || {
        if ctx.canvas.get() {
            return Vec::new().collect_view();
        }
        plan.with(|plan| {
            plan.cells
                .iter()
                .map(|cell| {
                    let address = cells::address(cell.row, cell.col);
                    let mut style = rect_style(&cell.rect);
                    style.push_str(&panes.get().cell_clip_style(cell,&metrics.get(),&viewport.get()));

                    let conditional = cell.shown.conditional.clone().unwrap_or_default();
                    // A conditional format's font colour comes before the number format's.
                    if let Some(color) = conditional.font_color.as_ref().or(cell.shown.color.as_ref()).or(cell.shown.style.font_color.as_ref()) {
                        style.push_str(&format!(";color:{color}"));
                    }
                    if conditional.bold {
                        style.push_str(";font-weight:700");
                    }
                    if conditional.italic {
                        style.push_str(";font-style:italic");
                    }
                    let double = autofit::double_underline(&cell.shown.style,Some(&conditional));
                    style.push_str(autofit::decoration_css(conditional.underline || (cell.shown.style.underline && !double), conditional.strike || cell.shown.style.strike));
                    let italic=matches!(cell.shown.provenance,cells::Provenance::FileCache|cells::Provenance::Refused);
                    let mut display_style=cell.shown.style.clone();display_style.pixels=cell.paint_pixels;
                    style.push(';');style.push_str(&autofit::font_css(&display_style,Some(&conditional),italic,plan.scale));
                    let mut layout = alignment::layout(cell, &conditional, italic, plan.scale);
                    if double { layout.span.push_str(&autofit::double_underline_css(cell.paint_pixels,plan.scale)); }
                    style.push_str(&layout.container);
                    let shown = cell.shown.clone();
                    let icon = conditional
                        .icon
                        .as_ref()
                        .map(|(set, index)| (format!("{set}:{index}"), icons::icon_svg(set, *index)));
                    let icon_name = icon.as_ref().map(|(name, _)| name.clone());
                    let bar = conditional
                        .bar
                        .as_ref()
                        .map(|(share, _, _)| format!("{share:.4}"));
                    view! {
                        <div
                            class="dc-cell"
                            aria-hidden="true"
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
                            {icon.map(|(_, markup)| view! { <span class="dc-icon" use:css=layout.icon inner_html=markup></span> })}
                            <span class="dc-cell-text" use:css=layout.span>{layout.lines.into_iter().map(|line| {
                                let contents=match line.segments { Some(parts)=>parts.into_iter().enumerate().map(|(index,part)| {
                                    let style=line.advances.as_ref().and_then(|a|a.get(index)).map(|w|format!("display:inline-block;flex:none;width:{w}px")).unwrap_or_default();
                                    view!{<span use:css=style>{part}</span>}.into_any()
                                }).collect_view().into_any(), None=>line.text.into_any() };
                                view! { <span use:css=line.css>{contents}</span> }
                            }).collect_view()}</span>
                        </div>
                    }
                })
                .collect_view()
        })
    };
    let canvas_icons = move || {
        plan.with(|plan| plan.cells.iter().filter(|_|ctx.canvas.get()).filter_map(|cell| {
            let conditional=cell.shown.conditional.as_ref()?;
            let (set,index)=conditional.icon.as_ref()?;
            let italic=matches!(cell.shown.provenance,cells::Provenance::FileCache|cells::Provenance::Refused);
            let layout=alignment::layout(cell,conditional,italic,plan.scale);
            let style=format!("{}{}",rect_style(&cell.rect),panes.get().cell_clip_style(cell,&metrics.get(),&viewport.get()));
            Some(view!{<div class="dc-canvas-icon" aria-hidden="true" use:css=style><span class="dc-icon" use:css=layout.icon inner_html=icons::icon_svg(set,*index)></span></div>})
        }).collect_view())
    };
    // The semantic rows are already part of the plan. Empty identities, selection and
    // refusal text follow its committed publication rather than painted DOM attributes.
    let semantic_cells = move || {
        plan.with(|plan| {
            plan.semantic_rows
                .iter()
                .map(|row| {
                    let cells=row.cells.iter().map(|cell| view! {
            <span role="gridcell" id=format!("dc-grid-cell-{}-{}-{}",ctx.index,cell.row,cell.col)
                aria-colindex=(cell.col+1).to_string() aria-selected=cell.selected.to_string()
                aria-rowspan=cell.row_span.to_string() aria-colspan=cell.col_span.to_string()
                aria-readonly=(!cell.editability.can_begin).to_string()
                aria-label=cell.label.clone()>{cell.address.clone()}</span>
        }).collect_view();
                    view! { <div role="row" aria-rowindex=(row.row+1).to_string()><span role="rowheader" aria-colindex="1">{row.row.to_string()}</span>{cells}</div> }
                })
                .collect_view()
        })
    };
    let semantic_headers = move || {
        plan.with(|plan| plan.semantic_columns.iter().map(|col| view! {
        <span role="columnheader" aria-colindex=(col+1).to_string()>{cells::column_letters(*col)}</span>
    }).collect_view())
    };
    // A memo suppresses repeated live-region mutations when only paint geometry changes.
    let announcement = Memo::new(move |_| plan.with(|plan| plan.announcement.clone()));
    let headers_view = move || {
        let areas = controller.areas.get();
        plan.with(|plan| {
            let columns = plan
                .col_headers
                .iter()
                .map(|header| {
                    let class = if areas.iter().any(|area| (area.first().column()..=area.last().column()).contains(&header.col)) { "dc-head dc-on" } else { "dc-head" };
                    view! { <div class=class aria-hidden="true" data-dnacalc="column-header" data-column=header.label.clone() use:css=format!("{}{};font-size:{}px",rect_style(&header.rect),panes.get().clip_style(header.rect,&metrics.get(),&viewport.get(),None,Some(header.col)),12.0*zoom.get())>{header.label.clone()}</div> }
                })
                .collect_view();
            let rows = plan
                .row_headers
                .iter()
                .map(|header| {
                    let class = if areas.iter().any(|area| (area.first().row()..=area.last().row()).contains(&header.row)) { "dc-head dc-on" } else { "dc-head" };
                    view! { <div class=class aria-hidden="true" data-dnacalc="row-header" data-row=header.label.clone() use:css=format!("{}{};font-size:{}px",rect_style(&header.rect),panes.get().clip_style(header.rect,&metrics.get(),&viewport.get(),Some(header.row),None),12.0*zoom.get())>{header.label.clone()}</div> }
                })
                .collect_view();
            (columns, rows)
        })
    };
    let active_rect = move || {
        let (row, col) = controller.active.get();
        let m = metrics.get();
        if let Some(merge) = published_geometry.get().and_then(|g| {
            controller
                .sheet
                .get()
                .and_then(|s| CellKey::new(s, row, col).ok())
                .and_then(|at| g.merge_at(at))
        }) {
            let mut rect = panes.get().rect(
                &m,
                &viewport.get(),
                merge.first().row(),
                merge.first().column(),
            );
            rect.w = m.col_start(merge.last().column()) + m.width(merge.last().column())
                - m.col_start(merge.first().column());
            rect.h = m.row_start(merge.last().row()) + m.height(merge.last().row())
                - m.row_start(merge.first().row());
            rect
        } else {
            panes.get().rect(&m, &viewport.get(), row, col)
        }
    };
    // A selection crossing a frozen boundary has one visible rectangle in each pane.
    let selection_view = move || {
        controller.areas.get().into_iter().flat_map(|area| {
            let label=format!("{}:{}",cells::address(area.first().row(),area.first().column()),cells::address(area.last().row(),area.last().column()));
            panes.get().area_rects(area,&metrics.get(),&viewport.get()).into_iter().map(move |rect|view!{<div class="dc-selection" data-dnacalc="selection" data-area=label.clone() use:css=rect_style(&rect)></div>})
        }).collect_view()
    };
    let active_view = move || {
        let (row, col) = controller.active.get();
        let merged = published_geometry.get().and_then(|g| {
            controller
                .sheet
                .get()
                .and_then(|s| CellKey::new(s, row, col).ok())
                .and_then(|at| g.merge_at(at))
        });
        let rects = if let Some(area) = merged {
            panes
                .get()
                .area_rects(area, &metrics.get(), &viewport.get())
        } else {
            vec![active_rect()]
        };
        rects.into_iter().map(|rect|view! { <div class="dc-active" data-dnacalc="active-cell" use:css=format!("{}{}",rect_style(&rect),if merged.is_some(){String::new()}else{panes.get().clip_style(rect,&metrics.get(),&viewport.get(),Some(row),Some(col))})></div> }).collect_view()
    };
    let filter_open=controller.tool_field(|s|&s.filter.open,|s|&mut s.filter.open);
    let filter_headers = move || {
        let g=ctx.layout.get();let filters=g.auto_filter.into_iter().chain(g.tables.into_iter().filter_map(|t|t.filter));let plan=plan.get();let mut buttons=Vec::new();
        for filter in filters {let Some(area)=filter.area.area()else{continue;};let Some(row)=plan.row_headers.iter().find(|r|r.row==area.first().row())else{continue;};let header_row=row.row;
        for c in plan.col_headers.iter().filter(|c|(area.first().column()..=area.last().column()).contains(&c.col)){
            let column=c.col; let applied=filter.columns.contains(&column);
            let mut rect=geometry::CellRect {x:c.rect.x,y:row.rect.y,w:c.rect.w,h:row.rect.h};rect.x+=(rect.w-22.0*plan.scale).max(0.0);rect.w=rect.w.min(22.0*plan.scale);rect.h=rect.h.min(24.0*plan.scale);
            let style=format!("{}{}",rect_style(&rect),panes.get().clip_style(rect,&metrics.get(),&viewport.get(),Some(row.row),Some(c.col)));
            buttons.push(view!{<button class="dc-filter-header" data-dnacalc="filter-header" data-filter-column=column aria-label=format!("Filter column {}{}",column,if applied{" (filtered)"}else{""}) aria-expanded=move ||filter_open.get() title=if applied{"Filter applied"}else{"Filter column"} use:css=style on:pointerdown=move |ev|ev.stop_propagation() on:keydown=move |ev:leptos::ev::KeyboardEvent|ev.stop_propagation() on:click=move |ev|{ev.stop_propagation();controller.select(header_row,column);controller.open_filter(column);}>{if applied{"◆"}else{"▾"}}</button>}.into_any());
        }}buttons.collect_view()
    };
    let outline_controls = move || {
        let geometry=ctx.layout.get();let plan=plan.get();let scale=metrics.get().scale;
        geometry.outline.into_iter().filter_map(move |group|{
            let rect=if group.rows {plan.row_headers.iter().find(|h|h.row==group.summary).map(|h|h.rect)}else{plan.col_headers.iter().find(|h|h.col==group.summary).map(|h|h.rect)}?;
            let style=if group.rows {format!("left:{};top:{}",px((f64::from(group.level-1)*16.+1.)*scale),px(rect.y+2.*scale))}else{format!("left:{};top:{}",px(rect.x+2.*scale),px((f64::from(group.level-1)*16.+1.)*scale))};
            let style=format!("{style};width:{};height:{};font-size:{};line-height:{}",px(14.*scale),px(16.*scale),px(12.*scale),px(12.*scale));
            Some(view!{<button type="button" class="dc-outline-control" data-dnacalc="outline-gutter" data-first=group.first data-last=group.last data-level=group.level aria-expanded=(!group.collapsed).to_string() aria-label=format!("{} {} {} through {}, outline level {}",if group.collapsed{"Expand"}else{"Collapse"},if group.rows{"rows"}else{"columns"},group.first,group.last,group.level) use:css=style on:pointerdown=move |ev|ev.stop_propagation() on:mousedown=move |ev|ev.stop_propagation() on:keydown=move |ev:leptos::ev::KeyboardEvent|ev.stop_propagation() on:click=move |ev|{ev.stop_propagation();controller.outline(group.rows,wire::OutlineChange::Detail{first:group.first,last:group.last,level:Some(group.level),show:group.collapsed});}>{if group.collapsed{"+"}else{"−"}}</button>})
        }).collect_view()
    };
    let copy_outline = move || {
        controller.clipboard.get().filter(|area|Some(area.sheet())==controller.sheet.get()).map(|area|panes.get().area_rects(area,&metrics.get(),&viewport.get()).into_iter().map(|rect|view!{
            <div class="dc-copy-outline" data-dnacalc="copy-outline" data-cut=move ||controller.cut.get().to_string() use:css=rect_style(&rect)></div>
        }).collect_view())
    };
    let move_preview = move || {
        range_target.get().map(|area| {
            let label = format!("{}:{}", cells::address(area.first().row(),area.first().column()),cells::address(area.last().row(),area.last().column()));
            panes.get().area_rects(area,&metrics.get(),&viewport.get()).into_iter().map(move |rect|view!{<div class="dc-fill-preview" data-dnacalc="move-preview" data-area=label.clone() use:css=rect_style(&rect)></div>}).collect_view()
        })
    };
    let fill_preview = move || {
        fill_target.get().map(|(area,_)|panes.get().area_rects(area,&metrics.get(),&viewport.get()).into_iter().map(|rect|view!{<div class="dc-fill-preview" data-dnacalc="fill-preview" use:css=rect_style(&rect)></div>}).collect_view())
    };
    let fill_handle = move || {
        if controller.areas.get().len() != 1 || controller.editing.get().is_some() {
            return None;
        }
        let area = controller.selected_area()?;
        if area.cell_count() > calc_value::COPY_LIMIT {
            return None;
        }
        let rect = panes.get().rect(
            &metrics.get(),
            &viewport.get(),
            area.last().row(),
            area.last().column(),
        );
        let bounds = panes.get().bounds(
            &metrics.get(),
            &viewport.get(),
            area.last().row(),
            area.last().column(),
        );
        if rect.x + rect.w <= bounds.x
            || rect.y + rect.h <= bounds.y
            || rect.x + rect.w > bounds.x + bounds.w
            || rect.y + rect.h > bounds.y + bounds.h
        {
            return None;
        }
        Some(
            view! {<button class="dc-fill-handle" data-dnacalc="fill-handle" title="Drag to fill; Escape cancels. Keyboard: Clipboard & fill, Fill through." tabindex="-1" aria-hidden="true"
                use:css=format!("left:{};top:{}",px(rect.x+rect.w-6.0),px(rect.y+rect.h-6.0))
                on:mousedown=move |ev:leptos::ev::MouseEvent| {ev.stop_propagation();ev.prevent_default();if ev.button()==0 {fill_from.set(Some(area));fill_target.set(None);pointer_down.set_value(true);start_drag();}}
            ></button>},
        )
    };
    // The sheet's charts (dc-6yw), as the workbook holds their series now: read again after every
    // change, drawn where their frames are, the ones in view only.
    let charts_view = move || {
        let m = metrics.get();
        let v = viewport.get();
        let z = zoom.get();
        let corner = |(key, dx, dy): &(wire::Key, f64, f64)| {
            let at = key.cell()?;
            let r = panes.get().rect(&m, &v, at.row(), at.column());
            Some((r.x + dx * z, r.y + dy * z))
        };
        charts
            .get()
            .into_iter()
            .filter_map(|view| {
                let (x0, y0, x1, y1) = match &view.place {
                    wire::ChartPlace::Cells { from, to } => {
                        let ((a, b), (c, d)) = (corner(from)?, corner(to)?);
                        (a, b, c, d)
                    }
                    wire::ChartPlace::Sized {
                        from,
                        width,
                        height,
                    } => {
                        let (a, b) = corner(from)?;
                        (a, b, a + width * z, b + height * z)
                    }
                    wire::ChartPlace::Absolute {
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
                let markup = view.svg.clone();
                let kind = view.kind.clone();
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
    let review_markers = RwSignal::new(Vec::<(wire::Key, String)>::new());
    Effect::new(move |_| {
        let revision = controller.revision.get();
        let generation = controller.generation.get();
        let sheet = controller.sheet.get();
        review_markers.set(Vec::new());
        if let Some(sheet) = sheet {
            controller.ask(
                wire::Request::ReviewMarkers(wire::Sheet::of(sheet)),
                move |reply| {
                    if controller.sheet.get_untracked() == Some(sheet)
                        && controller.revision.get_untracked() == revision
                        && controller.generation.get_untracked() == generation
                        && let wire::Reply::ReviewMarkers(markers) = reply
                    {
                        review_markers.try_set(markers);
                    }
                },
            );
        }
    });
    let markers_view = move || {
        let m = metrics.get();
        let v = viewport.get();
        let p = panes.get();
        review_markers.get().into_iter().filter_map(|(key,kind)|{
            let at=key.cell()?;let rect=p.rect(&m,&v,at.row(),at.column());
            let (fw,fh)=p.size(&m);
            if rect.w<=0.0 || rect.h<=0.0 || rect.x+rect.w<=m.header_w || rect.y+rect.h<=m.header_h || rect.x>=v.width || rect.y>=v.height || (at.column()>p.columns && rect.x<m.header_w+fw) || (at.row()>p.rows && rect.y<m.header_h+fh) {return None;}
            let label=format!("{kind} on {}",controller.cell_label(at));
            Some(view!{<button type="button" class="dc-review-marker" data-dnacalc="review-marker" data-cell=format!("{}{}",dna_formula_ui::display::column_letters(at.column()),at.row()) title=label.clone() aria-label=label use:css=format!("left:{};top:{}",px(rect.x+rect.w-14.0),px(rect.y)) on:mousedown=move |ev|ev.stop_propagation() on:click=move |ev|{ev.stop_propagation();controller.navigate_to(at,move |ok|{if ok {controller.review_open.set(true);}});}>"•"</button>})
        }).collect_view()
    };
    // Invalid-data marks are a view overlay over the same published workbook audit for both painters.
    let invalid_view = move || {
        let m = metrics.get();
        let v = viewport.get();
        let p = panes.get();
        let plan = plan.get();
        let sheet = controller.sheet.get();
        let revision = controller.summary.with(|s|s.revision);
        controller.validation_scan().get().and_then(Result::ok).filter(|r|Some(r.sheet.id())==sheet && r.revision==revision).map(|r|r.invalid.into_iter().filter_map(wire::Key::cell).flat_map(|at|{
            let mut fragments=plan.cells.iter().filter(|c|c.row==at.row() && c.col==at.column()).map(|c|(c.anchor,p.anchor_clip_style(c,&m,&v))).collect::<Vec<_>>();
            if fragments.is_empty() && !p.area_rects(GridArea::cell(at),&m,&v).is_empty() {let rect=p.rect(&m,&v,at.row(),at.column());fragments.push((rect,p.clip_style(rect,&m,&v,Some(at.row()),Some(at.column()))));}
            let address=format!("{}{}",dna_formula_ui::display::column_letters(at.column()),at.row());
            fragments.into_iter().map(move |(rect,clip)|{let style=format!("left:{};top:{};width:{};height:{}{}",px(rect.x),px(rect.y),px(rect.w),px(rect.h),clip);view!{<div class="dc-invalid-circle" data-dnacalc="validation-invalid" data-cell=address.clone() aria-hidden="true" use:css=style></div>}})
        }).collect_view())
    };
    // The active cell's data validation (dc-6yw): a list's drop-down button at its right edge, the
    // list it opens (Alt+Down too, as in Excel), and the rule's input message below the cell.
    Effect::new(move |_| {
        let revision = controller.revision.get();
        let generation = controller.generation.get();
        rule.set(None);
        list_open.set(false);
        let Some(at) = controller.active_key() else {
            return;
        };
        controller.ask(wire::Request::Validation(wire::Key::of(at)), move |reply| {
            if controller.active_key_untracked() == Some(at)
                && controller.revision.get_untracked() == revision
                && controller.generation.get_untracked() == generation
                && let wire::Reply::Validation(found) = reply
            {
                rule.try_set(found);
                list_index.try_set(0);
            }
        });
    });
    let dropdown_view = move || {
        if ctx.active.get() != ctx.index {
            return None;
        }
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
                aria-label="Pick from the validation list"
                aria-controls=format!("dc-validation-list-{}",ctx.index)
                aria-expanded=move ||list_open.get().to_string()
                disabled=move ||controller.editing.get().is_some()
                use:css=style
                on:mousedown=move |ev| ev.stop_propagation()
                on:click=move |_| list_open.update(|open| *open = !*open)
            >"▼"</button>
        })
    };
    let list_view = move || {
        if ctx.active.get() != ctx.index || !list_open.get() {
            return None;
        }
        let at = controller.active_key_untracked()?;
        let items = rule.with_untracked(|rule| rule.as_ref().map(|r| r.items.clone()))?;
        let rect = active_rect();
        let m = metrics.get();
        let (left, top, width, height, above) =
            viewport::validation_popup(rect, viewport.get(), m.header_w, m.header_h, 160.);
        let style = format!("left:{};top:{};max-width:{};max-height:{}{}", px(left), px(top),
            px(width),
            px(height),
            if above {
                ";transform:translateY(-100%)"
            } else {
                ""
            }
        );
        let rows = items
            .into_iter().enumerate()
            .map(|(index,item)| {
                let text = item.clone();
                view! {
                    <li
                        data-dnacalc="dropdown-item" role="option" id=format!("dc-validation-option-{}-{index}",ctx.index) aria-selected=move ||(list_index.get()==index).to_string()
                        on:click=move |_| {
                            list_open.set(false);
                            controller.enter(at, text.clone(), |_| {});
                            focus_grid();
                        }
                    >{item}</li>
                }
            })
            .collect_view();
        Some(view! {
            <ul class="dc-list" id=format!("dc-validation-list-{}",ctx.index) role="listbox" aria-label="Allowed values" tabindex="0" node_ref=listbox aria-activedescendant=move ||format!("dc-validation-option-{}-{}",ctx.index,list_index.get()) data-dnacalc="dropdown-list" use:css=style
                on:keydown=move |ev:leptos::ev::KeyboardEvent|{
                    ev.stop_propagation();let key=ev.key();let items=rule.get_untracked().map(|r|r.items).unwrap_or_default();
                    if let Some(next)=validation_list_index(list_index.get_untracked(),&key,items.len()){ev.prevent_default();list_index.set(next);
                        use wasm_bindgen::JsCast;
                        if let (Some(list),Some(element))=(listbox.get_untracked(),document().get_element_by_id(&format!("dc-validation-option-{}-{next}",ctx.index)).and_then(|e|e.dyn_into::<web_sys::HtmlElement>().ok()) ) {let bounds=list.get_bounding_client_rect();let item=element.get_bounding_client_rect();let scroll=f64::from(list.scroll_top());
                            if item.top()<bounds.top()+1. {list.set_scroll_top((scroll+item.top()-bounds.top()-1.).round() as i32);}else if item.bottom()>bounds.bottom()-1. {list.set_scroll_top((scroll+item.bottom()-bounds.bottom()+1.).round() as i32);}
                        }
                    } else if key=="Escape" {ev.prevent_default();list_open.set(false);focus_grid();}
                    else if key=="Enter" {ev.prevent_default();if let Some(text)=items.get(list_index.get_untracked()){list_open.set(false);controller.enter(at,text.clone(),|_|{});focus_grid();}}
                    else if key=="Tab" {list_open.set(false);}
                }
                on:mousedown=move |ev| ev.stop_propagation()>{rows}</ul>
        })
    };
    let prompt_view = move || {
        if ctx.active.get() != ctx.index {
            return None;
        }

        let (title, text) = rule.get()?.prompt?;
        if list_open.get() {
            return None;
        }
        let rect = active_rect();
        let m = metrics.get();
        let (left, top, width, height, above) =
            viewport::validation_popup(rect, viewport.get(), m.header_w, m.header_h, 180.);
        let style = format!(
            "left:{};top:{};max-width:{};max-height:{};overflow:auto{}",
            px(left),
            px(top),
            px(width.min(220.)),
            px(height),
            if above {
                ";transform:translateY(-100%)"
            } else {
                ""
            }
        );
        let description = if title.is_empty() {
            text.clone()
        } else {
            format!("{title}: {text}")
        };
        Some(view! {
            <div class="dc-prompt" id=format!("dc-validation-prompt-{}",ctx.index) role="note" tabindex="0" on:dblclick=move |ev:leptos::ev::MouseEvent|ev.stop_propagation() on:mousedown=move |ev:leptos::ev::MouseEvent|ev.stop_propagation() on:keydown=prompt_key on:wheel=prompt_wheel aria-label=description data-dnacalc="validation-prompt" use:css=style>
                {(!title.is_empty()).then(|| view! { <b>{title}</b> })}
                {text}
            </div>
        })
    };
    let editor_view = move || {
        (ctx.active.get()==ctx.index && controller.editing.get() == Some(EditOrigin::Cell)).then(|| {
            let mut rect = active_rect();
            let width = (viewport.get().width - metrics.get().header_w).min(320.0).max(120.0);
            rect.x = rect.x.min((viewport.get().width - width).max(metrics.get().header_w));
            rect.w = width;
            view! {
                <div class="dc-cell-editor" use:css=rect_style(&rect)
                    on:mousedown=move |ev| ev.stop_propagation()
                    on:dblclick=move |ev| ev.stop_propagation()
                    on:input=move |_| controller.edit_changed()
                    on:focusin=move |_| controller.activate_editor(EditOrigin::Cell, assist)>
                    {assist.field_named(Signal::derive(Vec::new), Some(on_editor_key), 1, "", Some("cell-editor"))}
                    {assist.popup()}
                    {assist.signature()}
                </div>
            }
        })
    };
    let point_view = move || {
        let pick = controller.pointed.get()?;
        if Some(pick.sheet) != controller.sheet.get() {
            return None;
        }
        let m = metrics.get();
        let v = viewport.get();
        let area = GridArea::new(
            CellKey::new(pick.sheet, pick.anchor.0, pick.anchor.1).ok()?,
            CellKey::new(pick.sheet, pick.corner.0, pick.corner.1).ok()?,
        )
        .ok()?;
        Some(panes.get().area_rects(area,&m,&v).into_iter().map(|rect|view!{<div class="dc-reference" data-dnacalc="point-reference" use:css=rect_style(&rect)></div>}).collect_view())
    };
    let space_width = move || {
        metrics.track();
        viewport.track();
        px(scales().0.surface)
    };
    let space_height = move || {
        metrics.track();
        viewport.track();
        px(scales().1.surface)
    };
    // The gridlines: one line after each visible column and row.
    let gridlines = move || {
        let enabled = !ctx.canvas.get();
        let v = viewport.get();
        plan.with(|plan| {
            let columns = plan
                .col_headers
                .iter()
                .filter(|header| enabled && header.rect.w > 0.0)
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
                .filter(|header| enabled && header.rect.h > 0.0)
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
                data-pane=ctx.index.to_string() data-grid-active=move ||(ctx.active.get()==ctx.index).to_string()
                style:grid-column=move ||if ctx.layout.get().split_window{(1+(ctx.index&1)).to_string()}else{String::new()}
                style:grid-row=move ||if ctx.layout.get().split_window{(1+(ctx.index>>1)).to_string()}else{String::new()}
                on:focusin=move |_|ctx.activate(controller)
                data-surface-ready=move ||ctx.loaded.get().to_string()
                data-dnacalc="grid" data-resizing=move ||resizing.get().is_some().to_string()
                data-frozen-rows=move ||panes.get().rows.to_string()
                data-frozen-columns=move ||panes.get().columns.to_string()
                role="grid"
                aria-label=move || controller.sheet.get().and_then(|id| controller.sheets().into_iter().find(|tab| tab.id == id).map(|tab| format!("{} worksheet", tab.name))).unwrap_or_else(|| "Worksheet".to_owned())
                aria-rowcount="1048577"
                aria-colcount="16385"
                aria-describedby=move ||format!("dc-grid-keyboard-{}{}",ctx.index,if rule.with(|r|r.as_ref().is_some_and(|r|r.prompt.is_some())) && !list_open.get() {format!(" dc-validation-prompt-{}",ctx.index)}else{String::new()})
                aria-activedescendant=move || {
                    plan.with(|plan| plan.semantic_rows.iter().flat_map(|r| &r.cells).find(|cell|cell.active).map(|cell|format!("dc-grid-cell-{}-{}-{}",ctx.index,cell.row,cell.col)))
                }
                tabindex="0"
                node_ref=scroller
                style:cursor=move ||if range_hover.get() || range_drag.get().is_some() {if range_copy.get() {"copy"} else {"move"}} else {resize_cursor.get()}
                on:scroll=move |_| measure()
                on:wheel=move |ev: leptos::ev::WheelEvent| {
                    let (sx,sy) = scales();
                    if sx.logical(1.0) > 1.0 || sy.logical(1.0) > 1.0 {
                        ev.prevent_default();
                        if let Some(element) = scroller.get_untracked() {
                            let v=viewport.get_untracked();
                            let factor=if ev.delta_mode()==1 {metrics.get_untracked().row_height} else if ev.delta_mode()==2 {v.height} else {1.0};
                            element.scroll_to_with_x_and_y(sx.physical(v.scroll_x+ev.delta_x()*factor),sy.physical(v.scroll_y+ev.delta_y()*factor));
                        }
                    }
                }
                on:keydown=on_keydown
                on:contextmenu=on_contextmenu
                on:mousedown=move |ev|{ctx.activate(controller);on_mousedown(ev)}
                on:dblclick=on_dblclick
            >
                <div class="dc-sr-only" role="rowgroup"><div role="row" aria-rowindex="1"><span role="columnheader" aria-colindex="1">"Row"</span>{semantic_headers}</div>{semantic_cells}</div>
                <span class="dc-sr-only" id=format!("dc-grid-keyboard-{}",ctx.index)>"Arrow keys move cells. F2 edits. F6 leaves the worksheet; Shift+F6 returns to the formula bar."</span>
                <div class="dc-space" style:width=space_width style:height=space_height>
                    <div class="dc-layer" node_ref=layer style:font-size=move ||px(13.0*zoom.get()) style:width=move || px(viewport.get().width) style:height=move || px(viewport.get().height)>
                        <canvas class="dc-grid-canvas" aria-hidden="true" data-dnacalc="grid-canvas" node_ref=canvas_node hidden=move ||!ctx.canvas.get() style:width=move ||px(viewport.get().width) style:height=move ||px(viewport.get().height)></canvas>
                        {gridlines}
                        {move || {
                            let m=metrics.get();let v=viewport.get();let (w,h)=panes.get().size(&m);
                            view! {
                                {(w>0.0 && m.header_w+w<v.width).then(||view!{<div class="dc-freeze-line" aria-hidden="true" use:css=format!("left:{};top:0;width:2px;height:{}",px(m.header_w+w-1.0),px(v.height))></div>})}
                                {(h>0.0 && m.header_h+h<v.height).then(||view!{<div class="dc-freeze-line" aria-hidden="true" use:css=format!("left:0;top:{};width:{};height:2px",px(m.header_h+h-1.0),px(v.width))></div>})}
                            }
                        }}
                        {move ||decorations_view(false)}
                        {cells_view}
                        {move ||decorations_view(true)}
                        {canvas_icons}
                        {charts_view}
                        {markers_view}
                        {invalid_view}
                        {selection_view}
                        {active_view}
                        {copy_outline}
                        {filter_headers}
                        {fill_preview}
                        {move_preview}
                        {fill_handle}
                        {point_view}
                        {dropdown_view}
                        {prompt_view}
                        {list_view}
                        {move || {
                            let (columns, rows) = headers_view();
                            view! { {columns}{rows} }
                        }}
                        {outline_controls}
                        <div class="dc-corner" style:width=move || px(metrics.get().header_w) style:height=move || px(metrics.get().header_h)></div>
                        {move ||resizing.get().map(|drag| {
                            let v=viewport.get();let edge=drag.edge();
                            let style=if drag.band.rows {format!("left:0;top:{};width:{};height:2px",px(edge),px(v.width))} else {format!("left:{};top:0;width:2px;height:{}",px(edge),px(v.height))};
                            view!{<div class="dc-resize-preview" data-dnacalc="resize-preview" use:css=style><span>{format!("{:.2} {}",drag.input(),if drag.band.rows {"pt"} else {"ch"})}</span></div>}
                        })}
                        {editor_view}
                    </div>
                </div>
            </div>
            {(ctx.index==0).then(move ||view!{<>
            {crate::clipboard::controls(controller)}
            {axis::controls(controller)}
            {crate::tables::controls(controller)}
            {crate::commands::sort_controls(controller)}
            {crate::commands::filter_controls(controller)}
            {structure::controls(controller)}
            <details class="dc-clipboard-tools" data-dnacalc="pane-tools" name="worksheet-tools">
                <summary>"View & panes"</summary>
                <p data-dnacalc="pane-state">{move ||{let (r,c)=panes.get().counts();format!("Frozen: {r} rows, {c} columns")}}</p>
                {move ||Panes::layout_warning(&ctx.layout.get()).map(|text|view!{<p class="dc-note" role="status">{text}</p>})}
                {move ||(split_window.get()&&ctx.layout.get().split.is_some()).then(||view!{<p class="dc-note">"Split panes scroll independently; adjacent panes share their row or column position. Split sizes adapt to the viewport."</p>})}
                {move || {let (w,h)=panes.get().size(&metrics.get());let m=metrics.get();let v=viewport.get();((w>0.0 && w>=v.width-m.header_w) || (h>0.0 && h>=v.height-m.header_h)).then(||view!{<p class="dc-note">"The frozen area fills this viewport. Unfreeze to reach the cells after it."</p>})}}
                <div class="dc-command-row">
                    <label>"Cell painter "<select data-dnacalc="grid-painter" prop:value=move ||if ctx.canvas.get(){"canvas"}else{"dom"}
                        on:change=move |ev|{let enabled=event_target_value(&ev)=="canvas";painter::remember(enabled);ctx.canvas.set(enabled);}>
                        <option value="dom">"DOM"</option><option value="canvas">"Canvas2D"</option>
                    </select></label>
                    <label>"Split across (pt) "<input data-dnacalc="split-x" aria-label="Horizontal split position in points" type="number" min="0" max="10000" prop:value=move ||split_x.get() on:input=move |ev|split_x.set(event_target_value(&ev))/></label>
                    <label>"Split down (pt) "<input data-dnacalc="split-y" aria-label="Vertical split position in points" type="number" min="0" max="10000" prop:value=move ||split_y.get() on:input=move |ev|split_y.set(event_target_value(&ev))/></label>
                    <button data-dnacalc="split-apply" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.split_panes(split_x.get_untracked(),split_y.get_untracked())>"Split"</button>
                    <button data-dnacalc="split-remove" disabled=move ||controller.editing.get().is_some() || !ctx.layout.get().split_window on:click=move |_|controller.split_panes("0".into(),"0".into())>"Remove split"</button>
                    <button data-dnacalc="freeze-active" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.freeze_panes("active")>"Freeze above & left of active cell"</button>
                    <button data-dnacalc="freeze-row" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.freeze_panes("first-row")>"Freeze first row"</button>
                    <button data-dnacalc="freeze-column" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.freeze_panes("first-column")>"Freeze first column"</button>
                    <button data-dnacalc="unfreeze" disabled=move ||controller.editing.get().is_some() on:click=move |_|controller.freeze_panes("none")>"Unfreeze"</button>
                </div>
            </details>
            <div class="dc-gridbar">
                <span data-dnacalc="grid-note" class="dc-note">{move || note.get()}</span>
                <button type="button" data-dnacalc="insert-cells" title="Insert cells, rows or columns (Ctrl++)"
                    on:click=move |_| controller.edit_axis(true)>"Insert…"</button>
                <button type="button" data-dnacalc="delete-cells" title="Delete cells, rows or columns (Ctrl+-)"
                    on:click=move |_| controller.edit_axis(false)>"Delete…"</button>
                <span class="dc-zoom">
                    <button type="button" data-dnacalc="zoom-out" title="Zoom out"
                        on:click=move |_| zoom.update(|z| *z = viewport::clamp_zoom(*z / viewport::ZOOM_STEP))>"-"</button>
                    <span data-dnacalc="zoom" class="dc-sr-only">{move || format!("{:.0}%", zoom.get() * 100.0)}</span>
                    <label><input data-dnacalc="zoom-percent" aria-label="Worksheet zoom percentage, 10 to 400" inputmode="decimal" prop:value=move ||zoom_text.get() on:input=move |ev|zoom_text.set(event_target_value(&ev)) on:change=move |ev|apply_zoom(event_target_value(&ev)) on:keydown=move |ev:leptos::ev::KeyboardEvent|{if ev.key()=="Enter" {ev.prevent_default();apply_zoom(event_target_value(&ev));}}/>"%"</label>
                    <button type="button" data-dnacalc="zoom-reset" on:click=move |_|zoom.set(1.0)>"Reset"</button>
                    <button type="button" data-dnacalc="zoom-in" title="Zoom in"
                        on:click=move |_| zoom.update(|z| *z = viewport::clamp_zoom(*z * viewport::ZOOM_STEP))>"+"</button>
                </span>
            </div>
            {crate::charts::controls(controller)}
            </>})}
            <span class="dc-sr-only" data-dnacalc="grid-announcement" role="status" aria-live="polite" aria-atomic="true">{move || if ctx.active.get()==ctx.index { announcement.get() } else { String::new() }}</span>
        </div>
    }
}

/// Moves the grid's focus to its scroller: the shell calls it after the formula bar enters an
/// edit, so the keys go to the grid again.
pub fn focus_grid() {
    if let Some(element) = leptos::prelude::document()
        .query_selector("[data-dnacalc=\"grid\"][data-grid-active=true]")
        .ok()
        .flatten()
        .or_else(|| document().query_selector(".dc-main").ok().flatten())
        && let Ok(element) = wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlElement>(element)
    {
        let _ = element.focus();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_value::{IdAllocator, SheetId};

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
    fn the_extent_reaches_the_physical_excel_limits_without_materializing_axes() {
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
        assert_eq!(blank, GridExtent::new(MAX_ROW, MAX_COLUMN));
        let used = GridArea::new(
            CellKey::new(sheet, 1, 1).unwrap(),
            CellKey::new(sheet, 500, 30).unwrap(),
        )
        .unwrap();
        assert_eq!(
            extent(Some(used), (1, 1), &m, &v),
            GridExtent::new(MAX_ROW, MAX_COLUMN)
        );
        assert_eq!(
            extent(Some(used), (900, 2), &m, &v),
            GridExtent::new(MAX_ROW, MAX_COLUMN)
        );
    }
}

// A focused input message owns scrolling even when its content already fits.
fn prompt_key(ev:leptos::ev::KeyboardEvent) {
    use wasm_bindgen::JsCast;
    ev.stop_propagation();
    let Some(node)=ev.current_target().and_then(|n|n.dyn_into::<web_sys::HtmlElement>().ok()) else {return;};
    let page=node.client_height().max(16);let current=node.scroll_top();
    let next=match ev.key().as_str() {
        "Home"=>0,"End"=>node.scroll_height(),
        "ArrowUp"=>current.saturating_sub(16),"ArrowDown"=>current.saturating_add(16),
        "PageUp"=>current.saturating_sub(page),"PageDown"=>current.saturating_add(page),
        " "=>if ev.shift_key(){current.saturating_sub(page)}else{current.saturating_add(page)},
        "ArrowLeft"|"ArrowRight"=>current,
        _=>return,
    };
    ev.prevent_default();node.set_scroll_top(next);
}
fn prompt_wheel(ev:leptos::ev::WheelEvent) {
    use wasm_bindgen::JsCast;
    ev.stop_propagation();
    if ev.ctrl_key(){return;}
    let Some(node)=ev.current_target().and_then(|n|n.dyn_into::<web_sys::HtmlElement>().ok()) else {return;};
    ev.prevent_default();
    let scale=match ev.delta_mode(){1=>16.,2=>f64::from(node.client_height()),_=>1.};
    node.set_scroll_top((f64::from(node.scroll_top())+ev.delta_y()*scale).round().clamp(0.,f64::from(i32::MAX)) as i32);
}

/// Bounded list movement. Empty lists and unrelated keys have no navigation target.
pub fn validation_list_index(index: usize, key: &str, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let index = index.min(len - 1);
    match key {
        "ArrowDown" => Some((index + 1).min(len - 1)),
        "ArrowUp" => Some(index.saturating_sub(1)),
        "Home" => Some(0),
        "End" => Some(len - 1),
        _ => None,
    }
}

/// Print pages consume the existing Canvas2D painter without changing its geometry or metrics.
pub(crate) fn print_advance(
    cell: &dna_calc_model::projection::CellShown,
    text: &str,
    fonts: u64,
) -> Option<f64> {
    autofit::plan_advance(cell, text, 1., fonts)
}
pub(crate) fn print_family(name: &str) -> String {
    autofit::family_css(Some(name))
}
pub(crate) fn align_print_baselines(
    plan: &mut render_plan::RenderPlan,
    shown: &[cells::CellShown],
    geometry: &dna_calc_model::projection::Geometry,
) {
    autofit::align_row_baselines(plan, shown, Some(geometry), 1.);
}
pub(crate) fn paint_print(
    canvas: &web_sys::HtmlCanvasElement,
    plan: &render_plan::RenderPlan,
    metrics: &geometry::GridMetrics,
) {
    painter::paint(canvas, plan, panes::Panes::default(), metrics);
}
