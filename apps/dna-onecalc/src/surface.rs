//! The browser surface over a [`Session`]: the formula editor (syntax highlighting, bracket pairs,
//! completion of calc-func's function names, signature help and function help), the cell the
//! formula is entered in, the session's sheets, a scratch grid of cells on the first sheet, a table
//! of typed inputs, and the result (its type, its value or array grid, a worksheet error and what it
//! means, where it came from), the diagnostics and the reads, recalculated on every edit.
//!
//! The surface keeps only what the person typed. On each edit it gives the session the formula
//! cell, the grid's entries, every input row and the formula, has the session calculate, and shows
//! the session's answers through [`crate::display`] and [`crate::result`]; no value is computed
//! here. The formula editor is [`dna_formula_ui::formula_field`], which DNA Calc's formula bar shares; its
//! assistance is calc-fml's ([`calc_fml::assist`]) and the function help text is calc-func's
//! ([`calc_func::help`]). Every element a person uses carries a `data-onecalc` attribute naming its
//! role.
//!
//! Ported in part from DnaTreeCalc's Leptos formula skin (`src/dnacalc-formula-skin-leptos/src/lib.rs`
//! at DnaTreeCalc c68ecaa: the array result grid and its CSS; the editor's part moved to
//! [`dna_formula_ui::formula_field`]).
//!
//! No reactive effect writes a signal: the popup is reconciled in the event handlers that change
//! the text or the caret, so a panic can only come from a memo or a view, where the browser smoke
//! test (`cargo xtask app smoke onecalc`) sees it.

use crate::display;
use crate::grid::{self, Entry};
use crate::result::{self, ResultView};
use crate::session::{InputValue, Session};
use calc_fml::syntax::TextSpan;
use calc_fml::{CalcEnvironment, DateSystem, FormatProfile, FormulaForm};
use calc_func::help;
use calc_func::value::CellKey;
use dna_formula_ui::editor::browse;
use dna_formula_ui::formula_field::FormulaAssist;
use dna_formula_ui::theme;
use leptos::mount::mount_to;
use leptos::prelude::*;
use std::any::Any;
use std::collections::BTreeSet;
use web_sys::HtmlElement;

/// The sheet a new surface's session starts with; the scratch grid's cells are on it.
const FIRST_SHEET: &str = "Sheet1";

/// The cell a new surface's formula is entered in: the grid's top right cell.
const FIRST_FORMULA_CELL: &str = "H1";

/// The most functions the function list shows at once.
const MAX_LISTED_FUNCTIONS: usize = 60;

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

/// What one grid cell shows besides its entry.
#[derive(Clone, Debug, Default, PartialEq)]
struct GridShown {
    /// The type of what the cell holds (`number`, `text`, ...), `formula` for the formula's cell,
    /// `refused` for an entry the grid does not hold, and empty for a blank cell.
    kind: &'static str,
    /// The formula's value, for the formula's cell.
    value: String,
    /// A tooltip: why an entry was refused, or what the formula's cell holds.
    note: String,
}

/// What the surface shows for one state of its fields.
#[derive(Clone, Debug, Default, PartialEq)]
struct Shown {
    formula_cell: String,
    rows: Vec<(u32, String)>,
    grid: Vec<GridShown>,
    result: String,
    view: ResultView,
    provenance: String,
    diagnostics: Vec<String>,
    /// The spans of the formula text the diagnostics point at.
    diagnostic_spans: Vec<TextSpan>,
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

    fn cell(&self, index: usize) -> GridShown {
        self.grid.get(index).cloned().unwrap_or_default()
    }
}

/// The surface, mounted; dropping it unmounts the surface.
pub struct Mounted {
    _handle: Box<dyn Any>,
}

/// Mounts a new surface, over a new session with one sheet (`Sheet1`) and the formula entered in
/// its cell H1, as the last child of `parent`.
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
        "error" => grid::error_literal(text)
            .map(InputValue::error)
            .ok_or_else(|| format!("{entry:?} is not a worksheet error such as #N/A")),
        "blank" => Ok(InputValue::blank()),
        other => Err(format!("{other:?} is not a kind of input")),
    }
}

/// The calculation environment the surface gives the session: the 1900 date system, the en-US
/// locale profile, and this browser's clock for NOW and TODAY (no clock off the browser, which
/// refuses them).
fn environment() -> CalcEnvironment {
    CalcEnvironment {
        date_system: DateSystem::System1900,
        profile: FormatProfile::for_culture(LOCALE),
        now: clock(),
    }
}

/// The locale profile the surface calculates and shows numbers in.
const LOCALE: &str = "en-US";

/// The browser's local time as an Excel serial date number in the 1900 system: days since
/// 1899-12-30, the epoch of serial numbers from 1900-03-01 on.
#[cfg(target_arch = "wasm32")]
fn clock() -> Option<f64> {
    let now = js_sys::Date::new_0();
    let local_ms = now.get_time() - now.get_timezone_offset() * 60_000.0;
    Some(local_ms / 86_400_000.0 + 25_569.0)
}

#[cfg(not(target_arch = "wasm32"))]
fn clock() -> Option<f64> {
    None
}

/// The grid's cell at a 0-based index, row by row.
fn grid_cell(session: &Session, index: usize) -> Option<CellKey> {
    let (_, sheet) = session.sheets().next()?;
    let row = index as u32 / grid::COLUMNS + 1;
    let column = index as u32 % grid::COLUMNS + 1;
    CellKey::new(sheet, row, column).ok()
}

/// Gives the session the fields as typed, calculates, and says what to show.
fn recalculate(
    session: &mut Session,
    formula: &str,
    formula_cell: &str,
    rows: &[Row],
    entries: &[String],
) -> Shown {
    session.clear_inputs();
    let formula_cell = match session.set_formula_cell(formula_cell) {
        Ok(cell) => display::cell(session, cell),
        Err(error) => format!(
            "{}; the formula stays in {}",
            display::input_error(session, &error),
            display::cell(session, session.formula_cell())
        ),
    };
    let at = session.formula_cell();

    // The grid's entries first, then the rows, then every grid cell still empty is blank.
    let mut grid = vec![GridShown::default(); entries.len()];
    let mut from_grid = BTreeSet::new();
    for (index, text) in entries.iter().enumerate() {
        let Some(cell) = grid_cell(session, index) else {
            continue;
        };
        if cell == at {
            continue;
        }
        match grid::entry(text) {
            Entry::Blank => {}
            Entry::Value(value) => {
                grid[index].kind = result::kind(value.value());
                if session.set_input_at(cell, value).is_ok() {
                    from_grid.insert(cell);
                }
            }
            Entry::Refused(why) => {
                grid[index].kind = "refused";
                grid[index].note = why;
            }
        }
    }
    let rows = rows
        .iter()
        .map(|row| (row.id, set_row(session, row, &from_grid)))
        .collect();
    for (index, text) in entries.iter().enumerate() {
        let Some(cell) = grid_cell(session, index) else {
            continue;
        };
        if cell != at && grid::entry(text) == Entry::Blank && session.input(cell).is_none() {
            let _ = session.set_input_at(cell, InputValue::blank());
        }
    }

    session.set_formula(formula);
    let formula_index = (0..entries.len()).find(|&index| grid_cell(session, index) == Some(at));
    if formula.trim().is_empty() {
        if let Some(index) = formula_index {
            grid[index] = GridShown {
                kind: "formula",
                value: String::new(),
                note: "the formula's cell: no formula yet".to_owned(),
            };
        }
        return Shown {
            formula_cell,
            rows,
            grid,
            result: "no formula".to_owned(),
            ..Shown::default()
        };
    }
    let calculation = session.calculate();
    let profile = session
        .environment()
        .and_then(|environment| environment.profile);
    let view = ResultView::of(calculation.result.as_ref(), profile.as_ref());
    if let Some(index) = formula_index {
        grid[index] = GridShown {
            kind: "formula",
            value: match &calculation.result {
                Some(value) => result::shown(value, profile.as_ref()),
                None => "(no value)".to_owned(),
            },
            note: format!("the formula's cell: {formula}"),
        };
    }
    let diagnostic_spans = calculation
        .diagnostics
        .iter()
        .filter_map(|diagnostic| match diagnostic {
            crate::Diagnostic::Syntax(syntax) => Some(syntax.span),
            crate::Diagnostic::Compile(error) => Some(compile_span(error)),
            crate::Diagnostic::Eval(_) => None,
        })
        .collect();
    Shown {
        formula_cell,
        rows,
        grid,
        result: match &calculation.result {
            Some(value) => display::value(value),
            None => "not calculated".to_owned(),
        },
        view,
        provenance: result::provenance(session, &calculation).unwrap_or_default(),
        diagnostics: calculation
            .diagnostics
            .iter()
            .map(|diagnostic| display::diagnostic(session, diagnostic))
            .collect(),
        diagnostic_spans,
        reads: calculation
            .reads
            .iter()
            .map(|read| display::read(session, read))
            .collect(),
    }
}

/// Where in the formula text a compile error points.
fn compile_span(error: &calc_fml::CompileError) -> TextSpan {
    use calc_fml::CompileError as E;
    match error {
        E::Syntax { span }
        | E::Reference { span, .. }
        | E::Unsupported { span, .. }
        | E::UnknownFunction { span, .. }
        | E::Arity { span, .. }
        | E::NumberLiteral { span } => *span,
    }
}

/// Gives the session one row's input and says what became of it.
fn set_row(session: &mut Session, row: &Row, from_grid: &BTreeSet<CellKey>) -> String {
    if row.address.trim().is_empty() {
        return "enter a cell".to_owned();
    }
    let value = match entry_value(session, &row.kind, &row.entry) {
        Ok(value) => value,
        Err(why) => return why,
    };
    match session.cell(&row.address) {
        Ok(cell) if from_grid.contains(&cell) => {
            return format!(
                "{} is set in the scratch grid",
                display::cell(session, cell)
            );
        }
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
    let assist = FormulaAssist::new(formula);
    let formula_cell = RwSignal::new(FIRST_FORMULA_CELL.to_owned());
    let form_choice = RwSignal::new(FormulaForm::Dynamic);
    let sheets = RwSignal::new(vec![FIRST_SHEET.to_owned()]);
    let sheet_entry = RwSignal::new(String::new());
    let sheet_status = RwSignal::new(String::new());
    let rows: RwSignal<Vec<Row>> = RwSignal::new(Vec::new());
    let next_row = StoredValue::new(0u32);
    let cells = (grid::ROWS * grid::COLUMNS) as usize;
    let entries: RwSignal<Vec<String>> = RwSignal::new(vec![String::new(); cells]);

    let shown = Memo::new(move |_| {
        let formula = formula.get();
        let formula_cell = formula_cell.get();
        let rows = rows.get();
        let entries = entries.get();
        let form = form_choice.get();
        // A new sheet changes what the addresses and the formula's references name.
        sheets.track();
        session
            .try_update_value(|session| {
                session.set_form(form);
                session.set_environment(Some(environment()));
                recalculate(session, &formula, &formula_cell, &rows, &entries)
            })
            .unwrap_or_default()
    });
    let filter = RwSignal::new(String::new());
    let functions = Memo::new(move |_| filter.with(|filter| browse(filter)));
    let function_list = move || {
        functions.with(|found| {
            let total = found.len();
            let items = found
                .iter()
                .take(MAX_LISTED_FUNCTIONS)
                .map(|target| {
                    let name = target.spelling();
                    let signature = target.signature_help().display;
                    // Microsoft's official description, marked as Microsoft's text in the tooltip.
                    let description = match help::help(*target) {
                        Some(h) if h.microsoft.is_some() => format!("{} (Source: Microsoft)", h.description()),
                        Some(h) => h.description().to_owned(),
                        None => String::new(),
                    };
                    view! {
                        <li>
                            <button
                                type="button"
                                class="oc-fn"
                                data-onecalc="function-entry"
                                data-name=name
                                title=description.clone()
                                on:mousedown=move |ev| ev.prevent_default()
                                on:click=move |_| assist.insert_function(name)
                            >
                                <code>{signature}</code>
                            </button>
                            <small>{description}</small>
                        </li>
                    }
                })
                .collect_view();
            let count = if total > MAX_LISTED_FUNCTIONS {
                format!("{total} functions; the first {MAX_LISTED_FUNCTIONS} listed")
            } else {
                format!("{total} functions")
            };
            view! {
                <p class="oc-prov" data-onecalc="function-count">{count}</p>
                <ul class="oc-fns" data-onecalc="functions">{items}</ul>
            }
        })
    };
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

    let result_view = move || {
        shown.with(|shown| match &shown.view {
            ResultView::NoFormula => view! {
                <p class="oc-prov">"Type a formula, starting with =."</p>
            }
            .into_any(),
            ResultView::Value { kind, text, exact } => {
                let (kind, text) = (*kind, text.clone());
                let exact = exact.clone().map(|exact| {
                    view! {
                        <span class="oc-prov" data-onecalc="result-exact">
                            {format!(" exactly {exact}")}
                        </span>
                    }
                });
                view! {
                    <div>
                        <span class="oc-kind" data-onecalc="result-type">{kind}</span>
                        <span class="oc-value" data-onecalc="result-value">{text}</span>
                        {exact}
                    </div>
                }
                .into_any()
            }
            ResultView::Error { code, meaning } => {
                let (code, meaning) = (*code, *meaning);
                view! {
                    <div>
                        <span class="oc-kind" data-onecalc="result-type">"error"</span>
                        <span class="oc-value oc-err" data-onecalc="result-value">{code}</span>
                        " "
                        <span class="oc-err" data-onecalc="result-error">{meaning}</span>
                    </div>
                }
                .into_any()
            }
            ResultView::Array(window) => {
                let caption = format!(
                    "{} x {} array{}",
                    window.total_rows,
                    window.total_cols,
                    if window.truncated() {
                        format!(
                            ", first {} rows by {} columns shown",
                            window.cells.len(),
                            window.cells.first().map_or(0, Vec::len)
                        )
                    } else {
                        String::new()
                    }
                );
                let body = window
                    .cells
                    .iter()
                    .map(|row| {
                        let cells = row
                            .iter()
                            .map(|cell| {
                                let (kind, text) = (cell.kind, cell.text.clone());
                                view! { <td data-kind=kind title=kind>{text}</td> }
                            })
                            .collect_view();
                        view! { <tr>{cells}</tr> }
                    })
                    .collect_view();
                view! {
                    <div>
                        <span class="oc-kind" data-onecalc="result-type">"array"</span>
                        <span data-onecalc="result-shape">{caption}</span>
                        <table class="oc-array" data-onecalc="result-array" role="grid">
                            <tbody>{body}</tbody>
                        </table>
                    </div>
                }
                .into_any()
            }
            ResultView::NotCalculated => view! {
                <div>
                    <span class="oc-kind" data-onecalc="result-type">"no value"</span>
                    <span class="oc-err">"The formula was not calculated; see below."</span>
                </div>
            }
            .into_any(),
        })
    };

    let grid_header = (1..=grid::COLUMNS)
        .map(|column| view! { <th>{grid::column_letter(column).to_string()}</th> })
        .collect_view();
    let grid_body = (1..=grid::ROWS)
        .map(|row| {
            let cells = (1..=grid::COLUMNS)
                .map(|column| {
                    let index = ((row - 1) * grid::COLUMNS + (column - 1)) as usize;
                    let address = grid::address(row, column);
                    let cell = move || shown.with(|shown| shown.cell(index));
                    let class = move || match cell().kind {
                        "formula" => "oc-formula",
                        "refused" => "oc-refused",
                        _ => "",
                    };
                    let value = move || {
                        let shown = cell();
                        if shown.kind == "formula" {
                            shown.value
                        } else {
                            entries.with(|entries| entries.get(index).cloned().unwrap_or_default())
                        }
                    };
                    view! {
                        <td class=class>
                            <input
                                type="text"
                                data-onecalc="grid-cell"
                                data-cell=address.clone()
                                aria-label=address
                                data-kind=move || cell().kind
                                title=move || cell().note
                                readonly=move || cell().kind == "formula"
                                prop:value=value
                                on:input=move |ev| {
                                    let text = event_target_value(&ev);
                                    entries.update(|entries| {
                                        if let Some(entry) = entries.get_mut(index) {
                                            *entry = text;
                                        }
                                    });
                                }
                            />
                        </td>
                    }
                })
                .collect_view();
            view! { <tr><th>{row}</th>{cells}</tr> }
        })
        .collect_view();

    view! {
        <section class="oc" data-onecalc="surface">
            <header class="oc-head">
                <h1>"DNA OneCalc"</h1>
                <select
                    class="oc-theme"
                    data-onecalc="theme"
                    aria-label="Theme"
                    on:change=move |ev| theme::set(&event_target_value(&ev))
                    prop:value=theme::current()
                >
                    {theme::CHOICES
                        .iter()
                        .map(|(id, label)| view! { <option value=*id>{*label}</option> })
                        .collect_view()}
                </select>
            </header>
            <p class="oc-sub">
                "One Excel formula, calculated in your browser by calc-fml and calc-func. No workbook, no calculation engine."
            </p>
            <div class="oc-row">
                <div class="oc-col">
                    {assist
                        .field(
                            Signal::derive(move || shown.with(|shown| shown.diagnostic_spans.clone())),
                            None,
                            2,
                            "=SUM(A1:A3)",
                        )}
                    {assist.popup()}
                    {assist.signature()}
                    <p>
                        <select
                            data-onecalc="formula-form"
                            aria-label="Formula form"
                            on:change=move |ev| {
                                form_choice
                                    .set(
                                        if event_target_value(&ev) == "legacy" {
                                            FormulaForm::Legacy
                                        } else {
                                            FormulaForm::Dynamic
                                        },
                                    )
                            }
                        >
                            <option value="dynamic">"Excel 365: dynamic arrays"</option>
                            <option value="legacy">"Legacy: implicit intersection"</option>
                        </select>
                        " entered in "
                        <input
                            data-onecalc="formula-cell"
                            type="text"
                            size="10"
                            value=FIRST_FORMULA_CELL
                            on:input=move |ev| formula_cell.set(event_target_value(&ev))
                        />
                        " "
                        <span data-onecalc="formula-cell-status">
                            {move || shown.with(|shown| shown.formula_cell.clone())}
                        </span>
                    </p>
                    <div class="oc-result" data-onecalc="result-panel">
                        <p>
                            "Result: "
                            <output data-onecalc="result">
                                {move || shown.with(|shown| shown.result.clone())}
                            </output>
                        </p>
                        {result_view}
                        <p class="oc-prov" data-onecalc="provenance">
                            {move || shown.with(|shown| shown.provenance.clone())}
                        </p>
                        <p class="oc-prov" data-onecalc="environment">
                            {format!(
                                "Dates in the 1900 system; numbers shown in the {LOCALE} General format; NOW and TODAY read {}.",
                                if cfg!(target_arch = "wasm32") { "this browser's clock" } else { "no clock" },
                            )}
                        </p>
                        <ul class="oc-diags" data-onecalc="diagnostics">
                            {move || {
                                shown
                                    .with(|shown| shown.diagnostics.clone())
                                    .into_iter()
                                    .map(|line| view! { <li>{line}</li> })
                                    .collect_view()
                            }}
                        </ul>
                        <ol class="oc-reads" data-onecalc="reads">
                            {move || {
                                shown
                                    .with(|shown| shown.reads.clone())
                                    .into_iter()
                                    .map(|line| view! { <li>{line}</li> })
                                    .collect_view()
                            }}
                        </ol>
                    </div>
                    {assist.help()}
                    <div class="oc-panel">
                        <h2>"Functions"</h2>
                        <input
                            type="search"
                            data-onecalc="function-filter"
                            placeholder="Find a function by name or description"
                            aria-label="Find a function"
                            size="34"
                            on:input=move |ev| filter.set(event_target_value(&ev))
                        />
                        {function_list}
                    </div>
                </div>
                <div class="oc-col">
                    <div class="oc-panel">
                        <h2>{format!("Scratch grid: {FIRST_SHEET}!A1:{}", grid::address(grid::ROWS, grid::COLUMNS))}</h2>
                        <div class="oc-gridwrap">
                            <table class="oc-grid" data-onecalc="grid">
                                <thead><tr><th></th>{grid_header}</tr></thead>
                                <tbody>{grid_body}</tbody>
                            </table>
                        </div>
                    </div>
                    <div class="oc-panel">
                        <h2>"Sheets"</h2>
                        <span data-onecalc="sheets">{move || sheets.with(|sheets| sheets.join(", "))}</span>
                        " "
                        <input
                            data-onecalc="sheet-name"
                            type="text"
                            size="12"
                            on:input=move |ev| sheet_entry.set(event_target_value(&ev))
                        />
                        <button data-onecalc="add-sheet" type="button" on:click=add_sheet>
                            "Add sheet"
                        </button>
                        " "
                        <span data-onecalc="sheet-status">{move || sheet_status.get()}</span>
                    </div>
                    <div class="oc-panel">
                        <h2>"Typed inputs, on any sheet"</h2>
                        <table class="oc-inputs" data-onecalc="inputs">
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
                                                        size="10"
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
                                                        size="10"
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
                    </div>
                </div>
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(cells: &[(usize, &str)]) -> Vec<String> {
        let mut entries = vec![String::new(); (grid::ROWS * grid::COLUMNS) as usize];
        for (index, text) in cells {
            entries[*index] = (*text).to_owned();
        }
        entries
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_grid_supplies_cells_and_shows_the_formula_cell() {
        let mut session = Session::new(FIRST_SHEET).expect("a session");
        // A1 = 1.5, A2 = 2, A3 typed nothing (blank); the formula in H1.
        let shown = recalculate(
            &mut session,
            "=SUM(A1:A4)*2",
            "H1",
            &[],
            &entries(&[(0, "1.5"), (8, "2")]),
        );
        assert_eq!(shown.result, "number 7");
        assert_eq!(
            shown.view,
            ResultView::Value {
                kind: "number",
                text: "7".to_owned(),
                exact: None
            }
        );
        assert_eq!(shown.grid[7].kind, "formula");
        assert_eq!(shown.grid[7].value, "7");
        assert_eq!(shown.grid[0].kind, "number");
        assert_eq!(
            shown.reads,
            [
                "Sheet1!A1:A4: Sheet1!A1 = number 1.5; Sheet1!A2 = number 2; 2 blank cells from Sheet1!A3"
            ]
        );
        assert!(
            shown.provenance.starts_with("calculated in Sheet1!H1"),
            "{}",
            shown.provenance
        );

        // A formula typed into a grid cell is refused there; the cell then has no input.
        let shown = recalculate(&mut session, "=A1+1", "H1", &[], &entries(&[(0, "=2")]));
        assert_eq!(shown.grid[0].kind, "refused");
        assert_eq!(shown.result, "not calculated");
        assert_eq!(shown.diagnostics, ["missing input: Sheet1!A1 has no input"]);

        // A row naming a grid cell with an entry is refused; one naming an empty one sets it.
        let rows = [
            Row {
                id: 0,
                address: "A1".to_owned(),
                kind: "number".to_owned(),
                entry: "9".to_owned(),
            },
            Row {
                id: 1,
                address: "B1".to_owned(),
                kind: "number".to_owned(),
                entry: "4".to_owned(),
            },
        ];
        let shown = recalculate(&mut session, "=A1*B1", "H1", &rows, &entries(&[(0, "3")]));
        assert_eq!(shown.rows[0].1, "Sheet1!A1 is set in the scratch grid");
        assert_eq!(shown.rows[1].1, "Sheet1!B1 = number 4");
        assert_eq!(shown.result, "number 12");

        // An array result and an error result.
        let shown = recalculate(&mut session, "={1,2;3,4}", "H1", &[], &entries(&[]));
        assert!(
            matches!(&shown.view, ResultView::Array(window) if window.total_rows == 2),
            "{shown:?}"
        );
        assert_eq!(shown.grid[7].value, "{2x2 array}");
        let shown = recalculate(&mut session, "=1/0", "H1", &[], &entries(&[]));
        assert_eq!(
            shown.view,
            ResultView::Error {
                code: "#DIV/0!",
                meaning: "division by zero"
            }
        );
        // A syntax diagnostic points into the text.
        let shown = recalculate(&mut session, "=A1*", "H1", &[], &entries(&[]));
        assert_eq!(shown.diagnostic_spans, [TextSpan::new(4, 0)]);
    }
}
