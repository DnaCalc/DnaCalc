//! Strand components, the workbook session as signals, and the registration surface for views.

pub mod data_validation;
pub mod dialog;
pub mod dialogs;
pub mod workbook_open;
pub mod live_tree;
pub mod name_manager;
pub mod page;
pub mod paste_special;
pub mod scenario_manager;
pub mod session;
pub mod sort_dialog;
pub mod view;
pub use session::{Focus, Following, Session};
pub use view::{
    DockSide, DockSpec, Fit, Located, MountedView, ToolSpec, View, ViewCommand, ViewSpec, WayForward,
};
/// The runtime types a view meets through the session, so a view crate needs no second import.
pub use dna_skin_runtime::Live;
pub use dna_skin_runtime::client::{EditSession, PAGE_BUDGET, Seam, Watch, refusal_text};
pub use dna_skin_runtime::commands::{self, InputMode, Invocation, KeyInput};

use calc_skin::{
    ARRAY_PAGE_COLUMNS, ARRAY_PAGE_ROWS, ArrayPosition, FormulaPort, Provenance, RawValue, Shown,
    Trial, TrialCell, TrialValue,
};
use leptos::prelude::*;
use std::rc::Rc;

pub struct FormulaContext {
    pub port: Rc<dyn FormulaPort>,
    pub draft: Trial,
    pub auto_preview: bool,
    pub persist: Option<fn(Trial)>,
    pub clock: fn() -> Option<f64>,
    pub status: RwSignal<&'static str>,
    pub apply: Option<Callback<String>>,
}

pub trait FormulaView {
    fn mount(context: FormulaContext) -> AnyView;
}

#[derive(Default)]
pub struct Registry {
    formula: Option<fn(FormulaContext) -> AnyView>,
    views: Vec<Rc<dyn View>>,
    tools: Vec<ToolSpec>,
}

impl Registry {
    pub fn formula<V: FormulaView>(&mut self) {
        self.formula = Some(V::mount);
    }

    pub fn mount_formula(&self, context: FormulaContext) -> Option<AnyView> {
        self.formula.map(|mount| mount(context))
    }
}

#[component]
pub fn ValueView(shown: Shown<TrialValue>, page: Callback<ArrayPosition>) -> impl IntoView {
    let provenance = match shown.provenance {
        Provenance::Empty => "Empty",
        Provenance::Literal => "Literal",
        Provenance::Calculated { .. } => "Calculated",
        Provenance::AsSaved => "Saved value",
        Provenance::External { .. } => "External",
        Provenance::Stale { .. } => "Out of date",
        Provenance::Refused { .. } => "Refused",
    };
    let provenance_note = match shown.provenance {
        Provenance::External { source, .. } => Some(source),
        Provenance::Refused { reason, .. } => Some(reason),
        _ => None,
    };
    let body = match shown.value {
        TrialValue::Scalar(cell) => {
            let raw = cell.raw.display();
            let is_error = matches!(cell.raw, RawValue::Error(_));
            let matched = cell.conditional_match;
            view! {
                <dl class="dna-fe-value" class:dna-fe-error=is_error>
                    <div><dt>"Raw value"</dt><dd data-skin0="raw">{raw}</dd></div>
                    <div><dt>"Formatted"</dt><dd data-skin0="formatted">{cell.formatted}</dd></div>
                    {matched.then(|| view! { <div class="dna-fe-rule-match" data-skin0="rule-match">"Rule matched"</div> })}
                </dl>
            }
            .into_any()
        }
        TrialValue::Array {
            rows,
            columns,
            position,
            cells,
        } => view! {
            <DataFrame rows columns position cells page/>
        }
        .into_any(),
    };
    view! {
        <div class="dna-fe-result" data-skin0="result">
            <p class="dna-fe-provenance">{provenance} {provenance_note}</p>
            {body}
        </div>
    }
}

#[component]
pub fn DataFrame(
    rows: usize,
    columns: usize,
    position: ArrayPosition,
    cells: Vec<Vec<TrialCell>>,
    page: Callback<ArrayPosition>,
) -> impl IntoView {
    let visible_rows = cells.len();
    let visible_columns = cells.first().map_or(0, Vec::len);
    let headers = (position.column + 1..=position.column + visible_columns)
        .map(|column| {
            view! {
                <th scope="col" aria-colindex=column + 1>{column}</th>
            }
        })
        .collect_view();
    let body = cells
        .into_iter()
        .enumerate()
        .map(|(row, cells)| {
            let contents = cells.into_iter().enumerate().map(|(column, cell)| {
            let raw = cell.raw.display();
            let is_error = matches!(cell.raw, RawValue::Error(_));
            let differs = raw != cell.formatted;
            view! {
                <td aria-colindex=position.column + column + 2 class:dna-fe-error=is_error>
                    <span>{cell.formatted}</span>
                    {differs.then(|| view! { <small class="dna-fe-raw">"Raw " {raw}</small> })}
                    {cell.conditional_match.then(|| view! { <small class="dna-fe-rule-match" data-skin0="rule-match">"Rule matched"</small> })}
                </td>
            }
        }).collect_view();
            let row = position.row + row + 1;
            view! { <tr aria-rowindex=row + 1><th scope="row">{row}</th>{contents}</tr> }
        })
        .collect_view();
    view! {
        <div class="dna-fe-array" data-skin0="array">
            <div class="dna-fe-actions" role="group" aria-label="Array pages">
                <button type="button" data-skin0="prev-rows" disabled=position.row == 0
                    on:click=move |_| page.run(ArrayPosition { row: position.row.saturating_sub(ARRAY_PAGE_ROWS), ..position })>"Previous rows"</button>
                <button type="button" data-skin0="next-rows" disabled={position.row + visible_rows >= rows}
                    on:click=move |_| page.run(ArrayPosition { row: position.row + visible_rows, ..position })>"Next rows"</button>
                <button type="button" data-skin0="prev-columns" disabled=position.column == 0
                    on:click=move |_| page.run(ArrayPosition { column: position.column.saturating_sub(ARRAY_PAGE_COLUMNS), ..position })>"Previous columns"</button>
                <button type="button" data-skin0="next-columns" disabled={position.column + visible_columns >= columns}
                    on:click=move |_| page.run(ArrayPosition { column: position.column + visible_columns, ..position })>"Next columns"</button>
            </div>
            <table aria-rowcount=rows.saturating_add(1).to_string()
                   aria-colcount=columns.saturating_add(1).to_string()>
                <caption>{format!("Rows {}–{} of {rows} · Columns {}–{} of {columns}", position.row + 1, position.row + visible_rows, position.column + 1, position.column + visible_columns)}</caption>
                <thead><tr><th scope="col">"Row"</th>{headers}</tr></thead>
                <tbody>{body}</tbody>
            </table>
        </div>
    }
}

/// Render a captured Unix timestamp as an ISO 8601 UTC time in the browser.
#[cfg(target_arch = "wasm32")]
pub fn format_utc_millis(unix_ms: u64) -> Option<String> {
    use leptos::web_sys::js_sys::Date;
    let date = Date::new(&leptos::wasm_bindgen::JsValue::from_f64(unix_ms as f64));
    if date.get_time().is_finite() {
        date.to_iso_string().as_string()
    } else {
        None
    }
}

/// ISO 8601 browser rendering is unavailable in native unit tests.
#[cfg(not(target_arch = "wasm32"))]
pub fn format_utc_millis(_: u64) -> Option<String> {
    None
}
