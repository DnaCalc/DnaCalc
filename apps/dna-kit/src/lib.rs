//! Strand components and the registration surface for formula views.

mod workbook;
pub use workbook::{WorkbookContext, WorkbookView};

use calc_skin::{FormulaPort, Provenance, RawValue, Shown, TrialCell, TrialValue};
use leptos::prelude::*;
use std::rc::Rc;

pub struct FormulaContext {
    pub port: Rc<dyn FormulaPort>,
    pub initial_formula: String,
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
pub fn ValueView(shown: Shown<TrialValue>) -> impl IntoView {
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
            cells,
        } => view! {
            <DataFrame rows columns cells/>
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
pub fn DataFrame(rows: usize, columns: usize, cells: Vec<Vec<TrialCell>>) -> impl IntoView {
    let visible_rows = cells.len();
    let visible_columns = cells.first().map_or(0, Vec::len);
    let headers = (1..=visible_columns)
        .map(|column| {
            view! {
                <th scope="col">{column}</th>
            }
        })
        .collect_view();
    let body = cells
        .into_iter()
        .enumerate()
        .map(|(row, cells)| {
            let contents = cells.into_iter().map(|cell| {
            let raw = cell.raw.display();
            let is_error = matches!(cell.raw, RawValue::Error(_));
            let differs = raw != cell.formatted;
            view! {
                <td class:dna-fe-error=is_error>
                    <span>{cell.formatted}</span>
                    {differs.then(|| view! { <small class="dna-fe-raw">"Raw " {raw}</small> })}
                </td>
            }
        }).collect_view();
            view! { <tr><th scope="row">{row + 1}</th>{contents}</tr> }
        })
        .collect_view();
    view! {
        <div class="dna-fe-array" data-skin0="array">
            <table aria-rowcount=rows.saturating_add(1).to_string()
                   aria-colcount=columns.saturating_add(1).to_string()>
                <caption>{format!("{visible_rows} of {rows} rows · {visible_columns} of {columns} columns")}</caption>
                <thead><tr><th scope="col">"Row"</th>{headers}</tr></thead>
                <tbody>{body}</tbody>
            </table>
        </div>
    }
}
