//! Bounded spreadsheet viewport shared by workbook views.

use calc_skin::grid::{Area, Key};
use calc_skin::metrics::{self, SheetBands};
use calc_skin::{Provenance, RawValue, address};
use dna_kit::WorkbookContext;
use leptos::prelude::*;
use std::collections::BTreeMap;

pub fn mount(cx: WorkbookContext) -> impl IntoView {
    let focus = NodeRef::<leptos::html::Div>::new();
    view! {
        <div class="dna-sheet" data-dnacalc="grid" tabindex="0" node_ref=focus
            role="region" aria-label="Worksheet grid"
            on:keydown=move |event: leptos::ev::KeyboardEvent| {
                if event.ctrl_key() || event.meta_key() || event.alt_key() || event.is_composing() {
                    return;
                }
                let Some(key) = cx.active.get_untracked() else { return };
                let Key(sheet, generation, row, column) = key;
                let next = match event.key().as_str() {
                    "ArrowDown" | "Enter" => Key(sheet, generation, row.saturating_add(1), column),
                    "ArrowUp" => Key(sheet, generation, row.saturating_sub(1).max(1), column),
                    "ArrowLeft" => Key(sheet, generation, row, column.saturating_sub(1).max(1)),
                    "ArrowRight" | "Tab" if !event.shift_key() => Key(sheet, generation, row, column.saturating_add(1)),
                    "F2" => {
                        event.prevent_default();
                        cx.edit.run((key, None));
                        return;
                    }
                    "Backspace" => {
                        event.prevent_default();
                        cx.edit.run((key, Some(String::new())));
                        return;
                    }
                    text if text.chars().count() == 1 && !event.ctrl_key() && !event.meta_key() && !event.alt_key() => {
                        event.prevent_default();
                        cx.edit.run((key, Some(text.to_owned())));
                        return;
                    }
                    _ => return,
                };
                if next.cell().is_some() {
                    event.prevent_default();
                    cx.select.run(next);
                }
            }
        >
            {move || {
                let Some(window) = cx.window.get() else { return view! { <p class="dna-sheet-loading">"Loading worksheet…"</p> }.into_any() };
                let Some(Area(first, last)) = window.areas.first().copied() else { return ().into_any() };
                let geometry = cx.geometry.get();
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
                        view! {
                            <td role="gridcell" aria-colindex=column
                                aria-selected=move || (cx.active.get() == Some(key)).to_string()
                                class:dna-selected=move || cx.active.get() == Some(key)
                                class:dna-number=number class:dna-error=error
                                data-cell=location data-provenance=provenance
                                on:click=move |_| {
                                    if let Some(grid) = focus.get() { let _ = grid.focus(); }
                                    cx.select.run(key);
                                }
                                on:dblclick=move |_| cx.edit.run((key, None))
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
