//! The browser surface over a [`Session`]: a formula box and the cell it is entered in, the
//! session's sheets, a table of typed inputs, and the result, diagnostics and reads of the
//! formula, recalculated on every edit.
//!
//! The surface keeps only what the person typed. On each edit it gives the session the formula
//! cell, every input row and the formula, has the session calculate, and shows the session's
//! answers through [`crate::display`]; no value is computed here. Every element a person uses
//! carries a `data-onecalc` attribute naming its role.

use crate::display;
use crate::session::{InputValue, Session};
use calc_fml::parse_formula;
use calc_fml::syntax::SyntaxKind;
use leptos::mount::mount_to;
use leptos::prelude::*;
use std::any::Any;
use web_sys::HtmlElement;

/// The sheet a new surface's session starts with.
const FIRST_SHEET: &str = "Sheet1";

/// The cell a new surface's formula is entered in.
const FIRST_FORMULA_CELL: &str = "A1";

/// The kinds of value an input row holds: the selector's value and its label.
const KINDS: [(&str, &str); 5] = [
    ("number", "Number"),
    ("text", "Text"),
    ("logical", "Logical"),
    ("error", "Error"),
    ("blank", "Blank"),
];

/// One input row as typed.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    id: u32,
    address: String,
    kind: String,
    entry: String,
}

/// Which field of a row an edit changes.
#[derive(Clone, Copy)]
enum Field {
    Address,
    Kind,
    Entry,
}

/// What the surface shows for one state of its fields.
#[derive(Clone, Debug, Default, PartialEq)]
struct Shown {
    formula_cell: String,
    rows: Vec<(u32, String)>,
    result: String,
    diagnostics: Vec<String>,
    reads: Vec<String>,
}

impl Shown {
    fn row(&self, id: u32) -> String {
        self.rows
            .iter()
            .find(|(row, _)| *row == id)
            .map(|(_, status)| status.clone())
            .unwrap_or_default()
    }
}

/// The surface, mounted; dropping it unmounts the surface.
pub struct Mounted {
    _handle: Box<dyn Any>,
}

/// Mounts a new surface, over a new session with one sheet (`Sheet1`) and the formula entered in
/// its cell A1, as the last child of `parent`.
pub fn mount(parent: HtmlElement) -> Mounted {
    Mounted {
        _handle: Box::new(mount_to(parent, surface)),
    }
}

/// The typed value a row's entry gives under its kind, or why it gives none.
fn entry_value(session: &Session, kind: &str, entry: &str) -> Result<InputValue, String> {
    let text = entry.trim();
    let refused = |error| display::input_error(session, &error);
    match kind {
        "number" => match text.parse::<f64>() {
            Ok(number) => InputValue::number(number).map_err(refused),
            Err(_) => Err(format!("{entry:?} is not a number")),
        },
        "text" => InputValue::text(entry).map_err(refused),
        "logical" if text.eq_ignore_ascii_case("TRUE") => Ok(InputValue::logical(true)),
        "logical" if text.eq_ignore_ascii_case("FALSE") => Ok(InputValue::logical(false)),
        "logical" => Err(format!("{entry:?} is neither TRUE nor FALSE")),
        "error" => error_literal(text)
            .map(InputValue::error)
            .ok_or_else(|| format!("{entry:?} is not a worksheet error such as #N/A")),
        "blank" => Ok(InputValue::blank()),
        other => Err(format!("{other:?} is not a kind of input")),
    }
}

/// The worksheet error `text` spells, read by calc-fml's lexer as an error literal of a formula.
fn error_literal(text: &str) -> Option<calc_func::value::WorksheetErrorCode> {
    let tree = parse_formula(text);
    if !tree.diagnostics().is_empty() {
        return None;
    }
    let expression = tree.expression()?;
    if expression.kind() != SyntaxKind::ErrorLiteralExpr {
        return None;
    }
    expression.tokens().first()?.error_code()
}

/// Gives the session the fields as typed, calculates, and says what to show.
fn recalculate(session: &mut Session, formula: &str, formula_cell: &str, rows: &[Row]) -> Shown {
    session.clear_inputs();
    let formula_cell = match session.set_formula_cell(formula_cell) {
        Ok(cell) => display::cell(session, cell),
        Err(error) => format!(
            "{}; the formula stays in {}",
            display::input_error(session, &error),
            display::cell(session, session.formula_cell())
        ),
    };
    let rows = rows
        .iter()
        .map(|row| (row.id, set_row(session, row)))
        .collect();
    session.set_formula(formula);
    if formula.trim().is_empty() {
        return Shown {
            formula_cell,
            rows,
            result: "no formula".to_owned(),
            ..Shown::default()
        };
    }
    let calculation = session.calculate();
    Shown {
        formula_cell,
        rows,
        result: match &calculation.result {
            Some(value) => display::value(value),
            None => "not calculated".to_owned(),
        },
        diagnostics: calculation
            .diagnostics
            .iter()
            .map(|diagnostic| display::diagnostic(session, diagnostic))
            .collect(),
        reads: calculation
            .reads
            .iter()
            .map(|read| display::read(session, read))
            .collect(),
    }
}

/// Gives the session one row's input and says what became of it.
fn set_row(session: &mut Session, row: &Row) -> String {
    if row.address.trim().is_empty() {
        return "enter a cell".to_owned();
    }
    let value = match entry_value(session, &row.kind, &row.entry) {
        Ok(value) => value,
        Err(why) => return why,
    };
    match session.cell(&row.address) {
        Ok(cell) if session.input(cell).is_some() => {
            return format!("{} is set by an earlier row", display::cell(session, cell));
        }
        Ok(_) => {}
        Err(error) => return display::input_error(session, &error),
    }
    let shown = display::value(value.value());
    match session.set_input(&row.address, value) {
        Ok(cell) => format!("{} = {shown}", display::cell(session, cell)),
        Err(error) => display::input_error(session, &error),
    }
}

fn edit_row(rows: RwSignal<Vec<Row>>, id: u32, field: Field, text: String) {
    rows.update(|rows| {
        if let Some(row) = rows.iter_mut().find(|row| row.id == id) {
            match field {
                Field::Address => row.address = text,
                Field::Kind => row.kind = text,
                Field::Entry => row.entry = text,
            }
        }
    });
}

fn surface() -> impl IntoView {
    let session = StoredValue::new(Session::new(FIRST_SHEET).expect("Sheet1 is a sheet name"));
    let formula = RwSignal::new(String::new());
    let formula_cell = RwSignal::new(FIRST_FORMULA_CELL.to_owned());
    let sheets = RwSignal::new(vec![FIRST_SHEET.to_owned()]);
    let sheet_entry = RwSignal::new(String::new());
    let sheet_status = RwSignal::new(String::new());
    let rows: RwSignal<Vec<Row>> = RwSignal::new(Vec::new());
    let next_row = StoredValue::new(0u32);

    let shown = Memo::new(move |_| {
        let formula = formula.get();
        let formula_cell = formula_cell.get();
        let rows = rows.get();
        // A new sheet changes what the addresses and the formula's references name.
        sheets.track();
        session
            .try_update_value(|session| recalculate(session, &formula, &formula_cell, &rows))
            .unwrap_or_default()
    });

    let add_sheet = move |_| {
        let name = sheet_entry.get_untracked();
        match session.try_update_value(|session| session.add_sheet(&name)) {
            Some(Ok(_)) => {
                sheets.update(|sheets| sheets.push(name));
                sheet_status.set(String::new());
            }
            Some(Err(error)) => sheet_status.set(display::sheet_error(&error)),
            None => {}
        }
    };

    let add_row = move |_| {
        let id = next_row.get_value();
        next_row.set_value(id + 1);
        rows.update(|rows| {
            rows.push(Row {
                id,
                address: String::new(),
                kind: KINDS[0].0.to_owned(),
                entry: String::new(),
            })
        });
    };

    view! {
        <section data-onecalc="surface">
            <p>
                <label>
                    "Formula "
                    <input
                        data-onecalc="formula"
                        type="text"
                        on:input=move |ev| formula.set(event_target_value(&ev))
                    />
                </label>
                " in "
                <input
                    data-onecalc="formula-cell"
                    type="text"
                    value=FIRST_FORMULA_CELL
                    on:input=move |ev| formula_cell.set(event_target_value(&ev))
                />
                " "
                <span data-onecalc="formula-cell-status">
                    {move || shown.with(|shown| shown.formula_cell.clone())}
                </span>
            </p>
            <p>
                "Sheets: "
                <span data-onecalc="sheets">{move || sheets.with(|sheets| sheets.join(", "))}</span>
                " "
                <input
                    data-onecalc="sheet-name"
                    type="text"
                    on:input=move |ev| sheet_entry.set(event_target_value(&ev))
                />
                <button data-onecalc="add-sheet" type="button" on:click=add_sheet>
                    "Add sheet"
                </button>
                " "
                <span data-onecalc="sheet-status">{move || sheet_status.get()}</span>
            </p>
            <table data-onecalc="inputs">
                <thead>
                    <tr>
                        <th>"Cell"</th>
                        <th>"Type"</th>
                        <th>"Value"</th>
                        <th>"Input"</th>
                        <th></th>
                    </tr>
                </thead>
                <tbody>
                    <For
                        each=move || rows.with(|rows| rows.iter().map(|row| row.id).collect::<Vec<_>>())
                        key=|id| *id
                        children=move |id| {
                            view! {
                                <tr data-onecalc="input-row">
                                    <td>
                                        <input
                                            data-onecalc="input-cell"
                                            type="text"
                                            on:input=move |ev| {
                                                edit_row(rows, id, Field::Address, event_target_value(&ev))
                                            }
                                        />
                                    </td>
                                    <td>
                                        <select
                                            data-onecalc="input-kind"
                                            on:change=move |ev| {
                                                edit_row(rows, id, Field::Kind, event_target_value(&ev))
                                            }
                                        >
                                            {KINDS
                                                .iter()
                                                .map(|(kind, label)| {
                                                    view! { <option value=*kind>{*label}</option> }
                                                })
                                                .collect_view()}
                                        </select>
                                    </td>
                                    <td>
                                        <input
                                            data-onecalc="input-value"
                                            type="text"
                                            on:input=move |ev| {
                                                edit_row(rows, id, Field::Entry, event_target_value(&ev))
                                            }
                                        />
                                    </td>
                                    <td data-onecalc="input-status">
                                        {move || shown.with(|shown| shown.row(id))}
                                    </td>
                                    <td>
                                        <button
                                            data-onecalc="remove-input"
                                            type="button"
                                            on:click=move |_| {
                                                rows.update(|rows| rows.retain(|row| row.id != id))
                                            }
                                        >
                                            "Remove"
                                        </button>
                                    </td>
                                </tr>
                            }
                        }
                    />
                </tbody>
            </table>
            <button data-onecalc="add-input" type="button" on:click=add_row>
                "Add input"
            </button>
            <p>
                "Result: "
                <output data-onecalc="result">{move || shown.with(|shown| shown.result.clone())}</output>
            </p>
            <ul data-onecalc="diagnostics">
                {move || {
                    shown
                        .with(|shown| shown.diagnostics.clone())
                        .into_iter()
                        .map(|line| view! { <li>{line}</li> })
                        .collect_view()
                }}
            </ul>
            <ol data-onecalc="reads">
                {move || {
                    shown
                        .with(|shown| shown.reads.clone())
                        .into_iter()
                        .map(|line| view! { <li>{line}</li> })
                        .collect_view()
                }}
            </ol>
        </section>
    }
}
