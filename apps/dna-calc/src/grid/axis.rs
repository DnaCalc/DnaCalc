//! Axis commands share the worker's physical-dimension path; viewport zoom stays independent.
use crate::controller::Controller;
use leptos::prelude::*;

pub use calc_value::stored_axis_size as stored_size;

pub fn controls(controller: Controller) -> impl IntoView {
    let column = RwSignal::new("8.43".to_owned());
    let row = RwSignal::new("15".to_owned());
    view! {
        <details class="dc-clipboard-tools" data-dnacalc="axis-tools" name="worksheet-tools">
            <summary>"Rows & columns"</summary>
            <p class="dc-note">
                "Applies to the rows or columns in the current rectangle. Zero hides; Show restores "
                "the retained size. AutoFit uses selected displayed contents. Set column width before "
                "fitting wrapped rows. Empty bands retain their size."
            </p>
            <div class="dc-command-row">
                <label>
                    "New column width (characters)"
                    <input
                        data-dnacalc="column-size"
                        aria-label="New column width in characters"
                        inputmode="decimal"
                        class="dc-axis-size"
                        prop:value=move || column.get()
                        on:input=move |ev| column.set(event_target_value(&ev))
                        on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                            if ev.key() == "Enter" {
                                ev.prevent_default();
                                controller.axis_format(false, Some(column.get_untracked()), None);
                            }
                        }
                    />
                </label>
                <button
                    data-dnacalc="set-column-size"
                    disabled=move || controller.editing.get().is_some()
                    on:click=move |_| {
                        controller.axis_format(false, Some(column.get_untracked()), None)
                    }
                >
                    "Set width"
                </button>
                <button
                    data-dnacalc="autofit-columns"
                    disabled=move || controller.editing.get().is_some()
                    on:click=move |_| controller.autofit(false)
                >
                    "AutoFit width"
                </button>
                <button
                    data-dnacalc="hide-columns"
                    disabled=move || controller.editing.get().is_some()
                    on:click=move |_| controller.axis_format(false, None, Some(true))
                >
                    "Hide columns"
                </button>
                <button
                    data-dnacalc="show-columns"
                    disabled=move || controller.editing.get().is_some()
                    on:click=move |_| controller.axis_format(false, None, Some(false))
                >
                    "Show columns"
                </button>
            </div>
            <div class="dc-command-row">
                <label>
                    "New row height (points)"
                    <input
                        data-dnacalc="row-size"
                        aria-label="New row height in points"
                        inputmode="decimal"
                        class="dc-axis-size"
                        prop:value=move || row.get()
                        on:input=move |ev| row.set(event_target_value(&ev))
                        on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                            if ev.key() == "Enter" {
                                ev.prevent_default();
                                controller.axis_format(true, Some(row.get_untracked()), None);
                            }
                        }
                    />
                </label>
                <button
                    data-dnacalc="set-row-size"
                    disabled=move || controller.editing.get().is_some()
                    on:click=move |_| controller.axis_format(true, Some(row.get_untracked()), None)
                >
                    "Set height"
                </button>
                <button
                    data-dnacalc="autofit-rows"
                    disabled=move || controller.editing.get().is_some()
                    on:click=move |_| controller.autofit(true)
                >
                    "AutoFit height"
                </button>
                <button
                    data-dnacalc="hide-rows"
                    disabled=move || controller.editing.get().is_some()
                    on:click=move |_| controller.axis_format(true, None, Some(true))
                >
                    "Hide rows"
                </button>
                <button
                    data-dnacalc="show-rows"
                    disabled=move || controller.editing.get().is_some()
                    on:click=move |_| controller.axis_format(true, None, Some(false))
                >
                    "Show rows"
                </button>
            </div>
            {outline_controls(controller)}
        </details>
    }
}

fn outline_controls(controller: Controller) -> impl IntoView {
    let rows = RwSignal::new(true);
    let geometry = move || controller.outline_geometry();
    view! {
        <section class="dc-outline-tools" aria-label="Worksheet outline">
            <h3>"Outline"</h3>
            <p class="dc-note">
                "Group detail rows or columns; leave their summary band outside the selection. Group again to nest. Collapse keeps manual hiding and filters."
            </p>
            <div class="dc-command-row">
                <label>
                    "Axis"
                    <select
                        data-dnacalc="outline-axis"
                        prop:value=move || if rows.get() { "rows" } else { "columns" }
                        on:change=move |ev| rows.set(event_target_value(&ev) == "rows")
                    >
                        <option value="rows">"Rows"</option>
                        <option value="columns">"Columns"</option>
                    </select>
                </label>
                <button
                    data-dnacalc="outline-group"
                    disabled=move || controller.editing.get().is_some()
                    on:click=move |_| controller.outline_selection(rows.get_untracked(), 0)
                >
                    "Group selection"
                </button>
                <button
                    data-dnacalc="outline-ungroup"
                    disabled=move || controller.editing.get().is_some()
                    on:click=move |_| controller.outline_selection(rows.get_untracked(), 1)
                >
                    "Ungroup selection"
                </button>
                <button
                    data-dnacalc="outline-collapse"
                    disabled=move || controller.editing.get().is_some()
                    on:click=move |_| controller.outline_selection(rows.get_untracked(), 2)
                >
                    "Hide detail"
                </button>
                <button
                    data-dnacalc="outline-expand"
                    disabled=move || controller.editing.get().is_some()
                    on:click=move |_| controller.outline_selection(rows.get_untracked(), 3)
                >
                    "Show detail"
                </button>
                <label>
                    <input
                        type="checkbox"
                        data-dnacalc="outline-summary-after"
                        prop:checked=move || {
                            geometry()
                                .is_none_or(|g| {
                                    if rows.get() { g.summary_below } else { g.summary_right }
                                })
                        }
                        on:change=move |ev| {
                            controller
                                .outline(
                                    rows.get_untracked(),
                                    crate::wire::OutlineChange::SummaryAfter(
                                        event_target_checked(&ev),
                                    ),
                                )
                        }
                    />
                    "Summary after detail"
                </label>
            </div>
            <div class="dc-command-row" aria-label="Show outline level">
                "Show level "
                {(1u8..=8)
                    .map(move |level| {
                        view! {
                            <button
                                data-dnacalc="outline-level"
                                data-level=level
                                on:click=move |_| {
                                    controller
                                        .outline(
                                            rows.get_untracked(),
                                            crate::wire::OutlineChange::ShowLevel(level),
                                        )
                                }
                            >
                                {level}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>
            <ul class="dc-outline-groups">
                {move || {
                    geometry()
                        .map(|g| {
                            g.outline
                                .into_iter()
                                .filter(|g| g.rows == rows.get())
                                .map(move |g| {
                                    let label = format!(
                                        "{} {}–{} · level {} · summary {}",
                                        if g.rows { "Rows" } else { "Columns" },
                                        g.first,
                                        g.last,
                                        g.level,
                                        g.summary,
                                    );
                                    view! {
                                        <li>
                                            <button
                                                data-dnacalc="outline-toggle"
                                                data-first=g.first
                                                data-last=g.last
                                                data-level=g.level
                                                aria-expanded=(!g.collapsed).to_string()
                                                on:click=move |_| {
                                                    controller
                                                        .outline(
                                                            g.rows,
                                                            crate::wire::OutlineChange::Detail {
                                                                first: g.first,
                                                                last: g.last,
                                                                level: Some(g.level),
                                                                show: g.collapsed,
                                                            },
                                                        )
                                                }
                                            >
                                                {if g.collapsed { "+ " } else { "− " }}
                                                {label}
                                            </button>
                                        </li>
                                    }
                                })
                                .collect_view()
                        })
                }}
            </ul>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dimension_inputs_preserve_documented_units_and_reject_unmeasured_small_widths() {
        assert_eq!(stored_size(false, 20.0), Ok(Some(20.7109375)));
        assert_eq!(stored_size(false, 255.0), Ok(Some(255.7109375)));
        assert_eq!(stored_size(true, 30.0), Ok(Some(30.0)));
        assert_eq!(stored_size(true, 0.0), Ok(None));
        assert_eq!(stored_size(false, 0.0), Ok(None));
        for n in [f64::NAN, f64::INFINITY, -1.0, 256.0, 0.5] {
            assert!(stored_size(false, n).is_err());
        }
        assert!(stored_size(true, 410.0).is_err());
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandEdge {
    pub rows: bool,
    pub index: u32,
    pub pixels: f64,
    pub edge: f64,
}

/// Only visible trailing header edges resize; clipped leading edges and hidden bands do not.
pub fn boundary(
    plan: &super::render_plan::RenderPlan,
    m: &super::geometry::GridMetrics,
    v: &super::geometry::Viewport,
    x: f64,
    y: f64,
) -> Option<BandEdge> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    if y >= 0.0 && y < m.header_h && x >= m.header_w && x < v.width {
        plan.col_headers
            .iter()
            .filter(|h| h.rect.w > 0.0)
            .filter_map(|h| {
                let edge = h.rect.x + h.rect.w;
                (edge > m.header_w && edge <= v.width && (x - edge).abs() <= 4.0).then_some(
                    BandEdge {
                        rows: false,
                        index: h.col,
                        pixels: m.width(h.col),
                        edge,
                    },
                )
            })
            .min_by(|a, b| (x - a.edge).abs().total_cmp(&(x - b.edge).abs()))
    } else if x >= 0.0 && x < m.header_w && y >= m.header_h && y < v.height {
        plan.row_headers
            .iter()
            .filter(|h| h.rect.h > 0.0)
            .filter_map(|h| {
                let edge = h.rect.y + h.rect.h;
                (edge > m.header_h && edge <= v.height && (y - edge).abs() <= 4.0).then_some(
                    BandEdge {
                        rows: true,
                        index: h.row,
                        pixels: m.height(h.row),
                        edge,
                    },
                )
            })
            .min_by(|a, b| (y - a.edge).abs().total_cmp(&(y - b.edge).abs()))
    } else {
        None
    }
}
pub fn input_for_pixels(rows: bool, pixels: f64, zoom: f64) -> f64 {
    let pixels = (pixels / zoom).round().max(0.0);
    if pixels < 3.0 {
        return 0.0;
    }
    if rows {
        (pixels * 0.75).min(409.0)
    } else {
        ((pixels - 5.0) / 7.0).clamp(1.0, 255.0)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeDrag {
    pub band: BandEdge,
    pub sheet: calc_value::SheetId,
    pub generation: u64,
    pub start: f64,
    pub current: f64,
    pub zoom: f64,
}
impl ResizeDrag {
    pub fn input(self) -> f64 {
        input_for_pixels(
            self.band.rows,
            self.band.pixels + self.current - self.start,
            self.zoom,
        )
    }
    pub fn edge(self) -> f64 {
        let input = self.input();
        let pixels = if input == 0.0 {
            0.0
        } else if self.band.rows {
            input / 0.75
        } else {
            input * 7.0 + 5.0
        };
        self.band.edge + pixels * self.zoom - self.band.pixels
    }
}
pub fn selected_bands(areas: &[calc_value::GridArea], rows: bool, index: u32) -> (u32, u32) {
    if let [area] = areas {
        let a = area.first();
        let b = area.last();
        let (first, last, whole) = if rows {
            (a.row(), b.row(), a.column() == 1 && b.column() == 16_384)
        } else {
            (a.column(), b.column(), a.row() == 1 && b.row() == 1_048_576)
        };
        if whole && (first..=last).contains(&index) {
            return (first, last - first + 1);
        }
    }
    (index, 1)
}

#[cfg(test)]
mod drag_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn resize_groups_only_a_single_whole_axis_selection_containing_the_edge() {
        use calc_value::{CellKey, GridArea, SheetId};
        let sheet = SheetId::from_parts(0, 0);
        let area = |r, c, rr, cc| {
            GridArea::new(
                CellKey::new(sheet, r, c).unwrap(),
                CellKey::new(sheet, rr, cc).unwrap(),
            )
            .unwrap()
        };
        let columns = area(1, 1, 1_048_576, 3);
        let rows = area(1, 1, 2, 16_384);
        assert_eq!(selected_bands(&[columns], false, 2), (1, 3));
        assert_eq!(selected_bands(&[columns], false, 4), (4, 1));
        assert_eq!(selected_bands(&[rows], true, 2), (1, 2));
        assert_eq!(selected_bands(&[area(1, 1, 2, 3)], false, 2), (2, 1));
        assert_eq!(selected_bands(&[columns, rows], true, 2), (2, 1));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn resize_edges_skip_hidden_bands_and_preserve_zoomed_units() {
        use super::super::{geometry::*, panes::Panes};
        let mut metrics = GridMetrics::default();
        metrics.cols = Sizes::new(metrics.col_width, [(2, 0.0)]);
        for zoom in [0.5, 1.0, 2.5] {
            let m = metrics.scaled(zoom);
            let v = Viewport {
                scroll_x: 0.0,
                scroll_y: 0.0,
                width: 800.0 * zoom,
                height: 500.0 * zoom,
            };
            let plan = Panes {
                rows: 1,
                columns: 1,
                ..Default::default()
            }
            .plan(&[], &m, &v, 100, 100);
            let edge =
                boundary(&plan, &m, &v, m.header_w + m.width(1) - zoom, 10.0 * zoom).unwrap();
            assert_eq!((edge.rows, edge.index), (false, 1));
            assert!(boundary(&plan, &m, &v, m.header_w + m.width(1) / 2.0, 10.0 * zoom).is_none());
            assert_eq!(input_for_pixels(false, 145.0 * zoom, zoom), 20.0);
            assert_eq!(input_for_pixels(true, 40.0 * zoom, zoom), 30.0);
        }
    }
}
