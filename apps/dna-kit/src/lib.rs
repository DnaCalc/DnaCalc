//! Strand components and the registration surface for formula views.

mod workbook;
pub use workbook::{WorkbookContext, WorkbookView};

use calc_skin::{
    ARRAY_PAGE_COLUMNS, ARRAY_PAGE_ROWS, ArrayPosition, CallerSnapshot, FormulaPort, Provenance,
    RawValue, Shown, TrialCell, TrialValue,
};
use leptos::prelude::*;
use std::rc::Rc;

pub struct FormulaContext {
    pub port: Rc<dyn FormulaPort>,
    pub initial_formula: String,
    pub caller: Option<CallerSnapshot>,
    pub clock: fn() -> Option<f64>,
    pub status: RwSignal<&'static str>,
}

pub trait FormulaView {
    fn mount(context: FormulaContext) -> AnyView;
}

#[derive(Default)]
pub struct Registry {
    formula: Option<fn(FormulaContext) -> AnyView>,
    workbook: Option<fn(WorkbookContext) -> AnyView>,
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
            view! {
                <dl class="dna-fe-value" class:dna-fe-error=is_error>
                    <div><dt>"Raw value"</dt><dd data-skin0="raw">{raw}</dd></div>
                    <div><dt>"Formatted"</dt><dd data-skin0="formatted">{cell.formatted}</dd></div>
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
                <button type="button" disabled=position.row == 0
                    on:click=move |_| page.run(ArrayPosition { row: position.row.saturating_sub(ARRAY_PAGE_ROWS), ..position })>"Previous rows"</button>
                <button type="button" disabled=position.row + visible_rows >= rows
                    on:click=move |_| page.run(ArrayPosition { row: position.row + visible_rows, ..position })>"Next rows"</button>
                <button type="button" disabled=position.column == 0
                    on:click=move |_| page.run(ArrayPosition { column: position.column.saturating_sub(ARRAY_PAGE_COLUMNS), ..position })>"Previous columns"</button>
                <button type="button" disabled=position.column + visible_columns >= columns
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
