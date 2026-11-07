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
use crate::fixture::{FormulaFixture, FormulaInput};
use crate::grid::{self, Entry};
use crate::result::{self, ResultView};
use crate::session::{InputValue, Session};
use calc_fml::syntax::TextSpan;
use calc_fml::{CalcEnvironment, FormatProfile, FormulaForm};
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
    state: &'static str,
    format_note: String,
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
    crate::port::entry_value(session, kind, entry)
}

/// The calculation environment the surface gives the session: the 1900 date system, the selected
/// locale profile, and this browser's clock for NOW and TODAY (no clock off the browser, which
/// refuses them).
fn environment(locale: &str) -> CalcEnvironment {
    crate::port::environment(locale, dna_formula_ui::web::serial_clock())
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
    number_format: &str,
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
            state: "Ready",
            ..Shown::default()
        };
    }
    let calculation = session.calculate();
    let profile = session
        .environment()
        .and_then(|environment| environment.profile);
    let (view, format_note) = match result::formatted_view(
        calculation.result.as_ref(),
        profile.as_ref(),
        number_format,
    ) {
        Ok(view) => (view, String::new()),
        Err(note) => (
            ResultView::of(calculation.result.as_ref(), profile.as_ref()),
            format!("Format unavailable: {note}. General display is shown."),
        ),
    };
    let state = if matches!(&view, ResultView::Error { code: "#NAME?", .. }) {
        "Unknown name or function"
    } else if matches!(&view, ResultView::Error { .. }) {
        "Worksheet error"
    } else if calculation.result.is_some() {
        "Calculated"
    } else if calculation.diagnostics.iter().any(|d| {
        matches!(
            d,
            crate::Diagnostic::Eval(calc_fml::EvalFault::MissingInput { .. })
        )
    }) {
        "Missing input"
    } else if calculation.diagnostics.iter().any(|d| {
        matches!(
            d,
            crate::Diagnostic::Compile(calc_fml::CompileError::NotModelled {
                gap: calc_fml::Gap::UnknownFunction { .. },
                ..
            })
        )
    }) {
        "Unknown function"
    } else if calculation
        .diagnostics
        .iter()
        .any(|d| matches!(d, crate::Diagnostic::Syntax(_)))
    {
        "Formula syntax"
    } else {
        "Not modelled"
    };
    if let Some(index) = formula_index {
        grid[index] = GridShown {
            kind: "formula",
            value: match &calculation.result {
                Some(value) => {
                    if let ResultView::Value { text, .. } = &view {
                        text.clone()
                    } else {
                        result::shown(value, profile.as_ref())
                    }
                }
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
            crate::Diagnostic::Eval(_) | crate::Diagnostic::UserFunction(_) => None,
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
        state,
        format_note,
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
    error.span()
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
    let from_link = shared_fixture();
    let initial_message = match &from_link {
        Ok(Some(_)) => "Session restored from its link.".to_owned(),
        Err(error) => format!("Link could not be restored: {error}"),
        _ => "Start with a formula or try an example.".to_owned(),
    };
    let initial = from_link.ok().flatten().unwrap_or_default();
    let session = StoredValue::new_local(initial.session().expect("a validated initial fixture"));
    let retained_formula = RwSignal::new(initial.formula.clone());
    let formula = RwSignal::new(display::formula(&initial.formula));
    let assist = FormulaAssist::new(formula);
    assist.start_edit(formula.get_untracked());
    let authored_formula = Memo::new(move |_| {
        let draft = formula.get();
        let retained = retained_formula.get();
        if draft == display::formula(&retained) {
            retained
        } else {
            draft
        }
    });
    let formula_cell = RwSignal::new(initial.formula_cell.clone());
    let form_choice = RwSignal::new(if initial.legacy {
        FormulaForm::Legacy
    } else {
        FormulaForm::Dynamic
    });
    let sheets = RwSignal::new(initial.sheets.clone());
    let sheet_entry = RwSignal::new(String::new());
    let sheet_status = RwSignal::new(String::new());
    let rows: RwSignal<Vec<Row>> = RwSignal::new(
        initial
            .inputs
            .iter()
            .cloned()
            .enumerate()
            .map(|(id, row)| Row {
                id: id as u32,
                address: row.address,
                kind: row.kind,
                entry: row.entry,
            })
            .collect(),
    );
    let next_row = StoredValue::new(initial.inputs.len() as u32);
    let entries: RwSignal<Vec<String>> = RwSignal::new(initial.grid.clone());

    let locale = RwSignal::new(initial.locale.clone());
    let number_format = RwSignal::new(initial.number_format.clone());
    let vba_loaded = RwSignal::new(initial.vba_loaded.clone());
    let fixture_status = RwSignal::new(initial_message);
    let fixture_json = RwSignal::new(String::new());
    let share_link = RwSignal::new(String::new());
    let share_snapshot = RwSignal::new(None::<FormulaFixture>);
    let baseline = RwSignal::new(Some(initial.clone()));
    let import_epoch = StoredValue::new(0u64);
    let pending_fixture = RwSignal::new(None::<(FormulaFixture, bool)>);
    let fixture_dialog = NodeRef::<leptos::html::Dialog>::new();
    let fixture_file = NodeRef::<leptos::html::Input>::new();

    let vba_source = RwSignal::new(initial.vba_draft.clone());
    let vba_status = RwSignal::new(if initial.vba_loaded.is_some() {
        "Fixture VBA module loaded.".to_owned()
    } else {
        "No VBA module loaded.".to_owned()
    });
    let vba_generation = RwSignal::new(0u64);
    let load_vba = move |_| {
        let source = vba_source.get_untracked();
        if let Some(result) = session.try_update_value(|session| {
            session.load_vba_module("Module1", &source, calc_vba::DEFAULT_STEP_BUDGET)
        }) {
            vba_status.set(match result {
                Ok(names) => {
                    vba_loaded.set(Some(source));
                    format!("Loaded: {}", names.join(", "))
                }
                Err(error) => {
                    format!("Module not loaded: {error}. Previous functions remain active.")
                }
            });
            vba_generation.update(|generation| *generation += 1);
        }
    };
    let clear_vba = move |_| {
        session.update_value(|session| session.clear_user_functions());
        vba_loaded.set(None);
        vba_status.set("No VBA module loaded.".to_owned());
        vba_generation.update(|generation| *generation += 1);
    };

    let shown = Memo::new(move |_| {
        let formula = formula.get();
        let formula_cell = formula_cell.get();
        let rows = rows.get();
        let entries = entries.get();
        let form = form_choice.get();
        // A new sheet changes what the addresses and the formula's references name.
        sheets.track();
        vba_generation.track();
        session
            .try_update_value(|session| {
                session.set_form(form);
                session.set_environment(Some(environment(&locale.get())));
                recalculate(
                    session,
                    &formula,
                    &formula_cell,
                    &rows,
                    &entries,
                    &number_format.get(),
                )
            })
            .unwrap_or_default()
    });
    let snapshot = Memo::new(move |_| {
        shown.with(|_| ());
        FormulaFixture {
            version: 1,
            formula: authored_formula.get(),
            formula_cell: formula_cell.get(),
            effective_anchor: session.with_value(|s| display::cell(s, s.formula_cell())),
            sheets: sheets.get(),
            grid: entries.get(),
            inputs: rows
                .get()
                .into_iter()
                .map(|row| FormulaInput {
                    address: row.address,
                    kind: row.kind,
                    entry: row.entry,
                })
                .collect(),
            legacy: form_choice.get() == FormulaForm::Legacy,
            locale: locale.get(),
            number_format: number_format.get(),
            vba_draft: vba_source.get(),
            vba_loaded: vba_loaded.get(),
        }
    });
    let dirty = move || baseline.get().as_ref() != Some(&snapshot.get());
    let apply_fixture = Callback::new(
        move |(fixture, clean): (FormulaFixture, bool)| match fixture.session() {
            Ok(restored) => {
                session.set_value(restored);
                let next = fixture.clone();
                batch(move || {
                    rows.set(
                        next.inputs
                            .into_iter()
                            .enumerate()
                            .map(|(id, row)| Row {
                                id: id as u32,
                                address: row.address,
                                kind: row.kind,
                                entry: row.entry,
                            })
                            .collect(),
                    );
                    next_row.set_value(fixture.inputs.len() as u32);
                    entries.set(next.grid);
                    sheets.set(next.sheets);
                    formula_cell.set(next.formula_cell);
                    form_choice.set(if next.legacy {
                        FormulaForm::Legacy
                    } else {
                        FormulaForm::Dynamic
                    });
                    locale.set(next.locale);
                    number_format.set(next.number_format);
                    vba_source.set(next.vba_draft);
                    vba_loaded.set(next.vba_loaded.clone());
                    vba_status.set(if next.vba_loaded.is_some() {
                        "Fixture VBA module loaded.".into()
                    } else {
                        "No VBA module loaded.".into()
                    });
                    vba_generation.update(|n| *n += 1);
                    retained_formula.set(next.formula.clone());
                    let friendly = display::formula(&next.formula);
                    formula.set(friendly.clone());
                    assist.start_edit(friendly);
                    sheet_entry.set(String::new());
                    sheet_status.set(String::new());
                    share_link.set(String::new());
                    share_snapshot.set(None);
                    baseline.set(clean.then_some(fixture));
                    fixture_status.set(if clean {
                        "Session restored from authored inputs.".into()
                    } else {
                        "Example loaded; export to keep a snapshot.".into()
                    });
                });
            }
            Err(error) => fixture_status.set(error),
        },
    );
    let request_fixture = Callback::new(move |next: (FormulaFixture, bool)| {
        import_epoch.update_value(|n| *n += 1);
        if dirty() {
            pending_fixture.set(Some(next));
        } else {
            apply_fixture.run(next);
        }
    });
    Effect::new(move |_| {
        let pending = pending_fixture.get().is_some();
        if let Some(dialog) = fixture_dialog.get() {
            if pending && !dialog.open() {
                let _ = dialog.show_modal();
            } else if !pending && dialog.open() {
                dialog.close();
            }
        }
    });
    #[cfg(target_arch = "wasm32")]
    {
        let changed = window_event_listener(leptos::ev::hashchange, move |_| {
            import_epoch.update_value(|n| *n += 1);
            match shared_fixture() {
                Ok(Some(fixture)) => request_fixture.run((fixture, true)),
                Err(error) => fixture_status.set(format!("Link could not be restored: {error}")),
                Ok(None) => {}
            }
        });
        on_cleanup(move || changed.remove());
    }
    let keep_fixture = move || {
        pending_fixture.set(None);
        fixture_status
            .set("Current session retained; the requested replacement was not loaded.".into());
        if let Some(dialog) = fixture_dialog.get_untracked() {
            dialog.close();
        }
        if let Some(field) = assist.field.get_untracked() {
            let _ = field.focus();
        }
    };
    let replace_fixture = move || {
        if let Some(next) = pending_fixture.get_untracked() {
            pending_fixture.set(None);
            if let Some(dialog) = fixture_dialog.get_untracked() {
                dialog.close();
            }
            apply_fixture.run(next);
            if let Some(field) = assist.field.get_untracked() {
                let _ = field.focus();
            }
        }
    };
    let restore_json = move || match FormulaFixture::from_json(&fixture_json.get_untracked()) {
        Ok(fixture) => request_fixture.run((fixture, true)),
        Err(error) => fixture_status.set(error),
    };
    let export_fixture = move || {
        let fixture = snapshot.get_untracked();
        match fixture.to_json() {
            Ok(json) => {
                fixture_json.set(json.clone());
                match dna_formula_ui::web::download_bytes(
                    "onecalc-session.json",
                    "application/json",
                    json.as_bytes(),
                ) {
                    Ok(()) => {
                        baseline.set(Some(fixture));
                        fixture_status.set(
                            "Download started: onecalc-session.json. Keep the file before leaving."
                                .into(),
                        );
                    }
                    Err(error) => fixture_status.set(error),
                }
            }
            Err(error) => fixture_status.set(error),
        }
    };
    let create_share = move || {
        let fixture = snapshot.get_untracked();
        match share_url(&fixture) {
            Ok(url) => {
                share_link.set(url);
                share_snapshot.set(Some(fixture));
                fixture_status.set("Share link contains this session's formula and inputs.".into());
            }
            Err(error) => fixture_status.set(error),
        }
    };

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
                        Some(h) if h.microsoft.is_some() => {
                            format!("{} (Source: Microsoft)", h.description())
                        }
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
                <div class="oc-empty"><p>"Start with one Excel formula and supply the inputs it reads."</p><button data-onecalc="try-example" on:click=move |_|request_fixture.run((FormulaFixture::example("sum"),false))>"Try SUM with two inputs"</button></div>
            }
            .into_any(),
            ResultView::Value { kind, text, exact } => {
                let (kind, text) = (*kind, text.clone());
                let exact = exact.clone().map(|exact| {
                    view! {
                        <details class="oc-exact"><summary>"Exact value"</summary><span class="oc-prov" data-onecalc="result-exact">
                            {format!(" exactly {exact}")}
                        </span></details>
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
                "One Excel formula, with the inputs it needs."
            </p>
            <details class="oc-fixtures" data-onecalc="fixture-controls"><summary data-onecalc="fixture-tools">"Formula sessions · "<span data-onecalc="fixture-state">{move ||if dirty(){"Unexported changes"}else{"Unchanged snapshot"}}</span></summary>
                <p>"Keep a formula and its authored inputs as JSON, or create a link that carries the session in its address."</p>
                <div class="oc-session-actions">
                    <button data-onecalc="fixture-export" on:click=move |_|export_fixture()>"Export session…"</button>
                    <button data-onecalc="fixture-import" on:click=move |_|{if let Some(input)=fixture_file.get_untracked(){input.click();}}>"Import session…"</button>
                    <button data-onecalc="fixture-share" on:click=move |_|create_share()>"Create share link"</button>
                </div>
                <input node_ref=fixture_file data-onecalc="fixture-file" type="file" accept="application/json,.json" hidden aria-label="Import formula-session JSON" on:change=move |ev| {
                    import_epoch.update_value(|n| *n += 1);
                    let epoch=import_epoch.get_value();
                    dna_formula_ui::web::read_chosen(event_target::<web_sys::HtmlInputElement>(&ev),Some(FormulaFixture::MAX_BYTES),move |loaded| {
                        if import_epoch.get_value()!=epoch {return;}
                        match loaded.and_then(|(_,bytes)|String::from_utf8(bytes).map_err(|_|"The session file is not UTF-8 text".to_owned())).and_then(|text|FormulaFixture::from_json(&text)) {Ok(fixture)=>request_fixture.run((fixture,true)),Err(error)=>fixture_status.set(error)}
                    });
                }/>
                <label>"Session JSON"<textarea data-onecalc="fixture-json" rows="4" prop:value=move ||fixture_json.get() on:input=move |ev|fixture_json.set(event_target_value(&ev))/></label>
                <button data-onecalc="fixture-restore" on:click=move |_|restore_json()>"Restore JSON"</button>
                <label>"Share link"<input data-onecalc="share-link" readonly prop:value={move ||if share_snapshot.get().as_ref()==Some(&snapshot.get()){share_link.get()}else{String::new()}} on:focus=move |ev|{event_target::<web_sys::HtmlInputElement>(&ev).select();}/></label>
                <p role="status" data-onecalc="fixture-status">{move ||fixture_status.get()}</p>
                <div class="oc-examples"><strong>"Examples"</strong><button data-onecalc="example-sum" on:click=move |_|request_fixture.run((FormulaFixture::example("sum"),false))>"SUM & blank input"</button><button data-onecalc="example-array" on:click=move |_|request_fixture.run((FormulaFixture::example("array"),false))>"Array"</button><button data-onecalc="example-lambda" on:click=move |_|request_fixture.run((FormulaFixture::example("lambda"),false))>"LAMBDA"</button><button data-onecalc="example-missing" on:click=move |_|request_fixture.run((FormulaFixture::example("missing"),false))>"Missing input"</button><button data-onecalc="fixture-new" on:click=move |_|request_fixture.run((FormulaFixture::default(),true))>"New blank session"</button></div>
            </details>
            <dialog class="oc-session-dialog" node_ref=fixture_dialog data-onecalc="fixture-dialog" aria-labelledby="oc-replace-title" on:cancel=move |ev:leptos::ev::Event|{ev.prevent_default();keep_fixture();}>
                <h2 id="oc-replace-title">"Replace unexported changes?"</h2><p>"Export this session first to keep its formula and inputs."</p><button data-onecalc="fixture-keep" autofocus on:click=move |_|keep_fixture()>"Keep editing"</button><button data-onecalc="fixture-replace" on:click=move |_|replace_fixture()>"Replace session"</button>
            </dialog>

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
                            aria-label="Formula form" prop:value=move ||if form_choice.get()==FormulaForm::Legacy{"legacy"}else{"dynamic"}
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
                            aria-label="Formula cell" prop:value=move ||formula_cell.get()
                            on:input=move |ev| formula_cell.set(event_target_value(&ev))
                        />
                        " "
                        <span data-onecalc="formula-cell-status">
                            {move || shown.with(|shown| shown.formula_cell.clone())}
                        </span>
                    </p>
                    <section class="oc-result" data-onecalc="result-panel" aria-labelledby="oc-result-title">
                        <header><h2 id="oc-result-title">"Result"</h2><span class="oc-result-state" data-onecalc="result-state">{move ||shown.with(|s|s.state)}</span></header>
                        <div class="oc-primary-result" data-onecalc="primary-result" aria-live="polite">{result_view}</div>
                        <p class="oc-format-note" data-onecalc="format-note">{move ||shown.with(|s|s.format_note.clone())}</p>
                        <ul class="oc-diags" data-onecalc="diagnostics">{move ||shown.with(|s|s.diagnostics.clone()).into_iter().map(|line|view!{<li>{line}</li>}).collect_view()}</ul>
                        <details class="oc-calculation-details" data-onecalc="result-details"><summary>"Calculation details"</summary>
                            <p>"Raw result: "<output data-onecalc="result">{move ||shown.with(|s|s.result.clone())}</output></p>
                            <p class="oc-prov" data-onecalc="provenance">{move ||shown.with(|s|s.provenance.clone())}</p>
                            <p class="oc-prov" data-onecalc="environment">{move ||format!("Dates use the 1900 system; {} numbers in {} format. NOW and TODAY read the browser clock. Evaluation uses calc-fml and calc-func.",locale.get(),number_format.get())}</p>
                            <ol class="oc-reads" data-onecalc="reads">{move ||shown.with(|s|s.reads.clone()).into_iter().map(|line|view!{<li>{line}</li>}).collect_view()}</ol>
                        </details>
                    </section>
                    <details class="oc-format-controls" data-onecalc="format-controls"><summary data-onecalc="format-tools">"Format & locale"</summary>
                        <label>"Locale"<select data-onecalc="locale" prop:value=move ||locale.get() on:change=move |ev|locale.set(event_target_value(&ev))>{FormulaFixture::LOCALES.into_iter().map(|name|view!{<option value=name>{name}</option>}).collect_view()}</select></label>
                        <label>"Number format (selected locale)"<input data-onecalc="number-format" list="oc-number-formats" prop:value=move ||number_format.get() on:input=move |ev|number_format.set(event_target_value(&ev))/></label>
                        <datalist id="oc-number-formats">{move || {
                            let p=FormatProfile::for_culture(&locale.get()).expect("listed locale");
                            ["General".to_owned(),format!("0{}00",p.decimal_separator),format!("#{}##0{}00",p.thousands_separator,p.decimal_separator),format!("0{}00%",p.decimal_separator),"yyyy-mm-dd".to_owned()].into_iter().map(|code|view!{<option value=code/>}).collect_view()
                        }}</datalist>
                        <p class="oc-prov">{move || {
                            let p=FormatProfile::for_culture(&locale.get()).expect("listed locale");
                            format!("Number formats use the selected locale's separators, for example 0{}00. Changing locale keeps your format text; adjust its separators when needed.",p.decimal_separator)
                        }}</p>
                        <p class="oc-prov">"Formulas use English function names, commas between arguments and a dot for decimals. Numeric inputs also use a dot."</p>
                    </details>
                    {assist.help()}
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
                    <details class="oc-panel oc-vba-panel"><summary data-onecalc="vba-tools">"VBA functions"</summary>
                        <textarea
                            data-onecalc="vba-source"
                            aria-label="VBA standard module"
                            rows="8"
                            class="oc-vba-source"
                            prop:value=move || vba_source.get()
                            on:input=move |ev| vba_source.set(event_target_value(&ev))
                        />
                        <button data-onecalc="vba-load" type="button" on:click=load_vba>"Load module"</button>
                        " "
                        <button data-onecalc="vba-clear" type="button" on:click=clear_vba>"Unload module"</button>
                        <p data-onecalc="vba-status">{move || vba_status.get()}</p>
                        <p class="oc-prov">"Load the example, enter 40 in A1, then try =Module1.Add2(A1,2). Module state lasts until reload or unload. Each call stops at its execution budget. References can supply values, arrays or read-only Excel Range arguments; writes and calculation commands remain unsupported."</p>
                    </details>
                    <div class="oc-panel">
                        <h2>"Sheets"</h2>
                        <span data-onecalc="sheets">{move || sheets.with(|sheets| sheets.join(", "))}</span>
                        " "
                        <input
                            data-onecalc="sheet-name" aria-label="New sheet name" prop:value=move ||sheet_entry.get()
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
                                                        data-onecalc="input-cell" aria-label=format!("Input {} cell",id+1) aria-describedby=format!("oc-input-status-{id}") prop:value=move ||rows.with(|rs|rs.iter().find(|r|r.id==id).map(|r|r.address.clone()).unwrap_or_default())
                                                        type="text"
                                                        size="10"
                                                        on:input=move |ev| {
                                                            edit_row(rows, id, Field::Address, event_target_value(&ev))
                                                        }
                                                    />
                                                </td>
                                                <td>
                                                    <select
                                                        data-onecalc="input-kind" aria-label=format!("Input {} type",id+1) aria-describedby=format!("oc-input-status-{id}") prop:value=move ||rows.with(|rs|rs.iter().find(|r|r.id==id).map(|r|r.kind.clone()).unwrap_or_default())
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
                                                        data-onecalc="input-value" aria-label=format!("Input {} value",id+1) aria-describedby=format!("oc-input-status-{id}") prop:value=move ||rows.with(|rs|rs.iter().find(|r|r.id==id).map(|r|r.entry.clone()).unwrap_or_default())
                                                        type="text"
                                                        size="10"
                                                        on:input=move |ev| {
                                                            edit_row(rows, id, Field::Entry, event_target_value(&ev))
                                                        }
                                                    />
                                                </td>
                                                <td data-onecalc="input-status" id=format!("oc-input-status-{id}") role="status">
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
                    <div class="oc-panel oc-catalogue" data-onecalc="catalogue-panel">
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
            "General",
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
        let shown = recalculate(
            &mut session,
            "=A1+1",
            "H1",
            &[],
            &entries(&[(0, "=2")]),
            "General",
        );
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
        let shown = recalculate(
            &mut session,
            "=A1*B1",
            "H1",
            &rows,
            &entries(&[(0, "3")]),
            "General",
        );
        assert_eq!(shown.rows[0].1, "Sheet1!A1 is set in the scratch grid");
        assert_eq!(shown.rows[1].1, "Sheet1!B1 = number 4");
        assert_eq!(shown.result, "number 12");

        // An array result and an error result.
        let shown = recalculate(
            &mut session,
            "={1,2;3,4}",
            "H1",
            &[],
            &entries(&[]),
            "General",
        );
        assert!(
            matches!(&shown.view, ResultView::Array(window) if window.total_rows == 2),
            "{shown:?}"
        );
        assert_eq!(shown.grid[7].value, "{2x2 array}");
        let shown = recalculate(&mut session, "=1/0", "H1", &[], &entries(&[]), "General");
        assert_eq!(
            shown.view,
            ResultView::Error {
                code: "#DIV/0!",
                meaning: "division by zero"
            }
        );
        // A syntax diagnostic points into the text.
        let shown = recalculate(&mut session, "=A1*", "H1", &[], &entries(&[]), "General");
        assert_eq!(shown.diagnostic_spans, [TextSpan::new(4, 0)]);
    }
}

/// A link carries authored fixture data in its fragment, never a calculated answer.
#[cfg(target_arch = "wasm32")]
fn shared_fixture() -> Result<Option<FormulaFixture>, String> {
    let hash = window().location().hash().map_err(|e| format!("{e:?}"))?;
    let Some(raw) = hash.strip_prefix("#fixture=") else {
        return Ok(None);
    };
    if raw.len() > 131_072 {
        return Err("The shared session link exceeds the supported size".into());
    }
    let text = js_sys::decode_uri_component(raw)
        .map_err(|_| "The shared session link is not valid URI text".to_owned())?
        .as_string()
        .ok_or("The shared session link is not text")?;
    FormulaFixture::from_json(&text).map(Some)
}
#[cfg(not(target_arch = "wasm32"))]
fn shared_fixture() -> Result<Option<FormulaFixture>, String> {
    Ok(None)
}
#[cfg(target_arch = "wasm32")]
fn share_url(fixture: &FormulaFixture) -> Result<String, String> {
    let text = fixture.to_json()?;
    let encoded = js_sys::encode_uri_component(&text)
        .as_string()
        .ok_or("The session could not be encoded")?;
    if encoded.len() > 131_072 {
        return Err("This session is too large for a link; export its JSON file".into());
    }
    let href = window().location().href().map_err(|e| format!("{e:?}"))?;
    Ok(format!(
        "{}#fixture={encoded}",
        href.split('#').next().unwrap_or(&href)
    ))
}
#[cfg(not(target_arch = "wasm32"))]
fn share_url(_fixture: &FormulaFixture) -> Result<String, String> {
    Err("Session links are available in the browser".into())
}

#[cfg(test)]
mod session_files {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn restored_authored_inputs_recalculate_and_format_through_the_surface_route() {
        let mut fixture = FormulaFixture::example("sum");
        fixture.locale = "de-DE".into();
        fixture.number_format = "0,00".into();
        let fixture = FormulaFixture::from_json(&fixture.to_json().unwrap()).unwrap();
        let mut session = fixture.session().unwrap();
        session.set_environment(Some(environment(&fixture.locale)));
        let shown = recalculate(
            &mut session,
            &fixture.formula,
            &fixture.formula_cell,
            &[],
            &fixture.grid,
            &fixture.number_format,
        );
        assert_eq!(shown.state, "Calculated");
        assert_eq!(shown.result, "number 3.5");
        assert!(matches!(shown.view,ResultView::Value {text,..} if text=="3,50"));
        let missing = recalculate(&mut session, "=Z99+1", "H1", &[], &fixture.grid, "General");
        assert_eq!(missing.state, "Missing input");
        let unknown = recalculate(
            &mut session,
            "=NO_SUCH_ONECALC_FUNCTION()",
            "H1",
            &[],
            &fixture.grid,
            "General",
        );
        assert_eq!(unknown.state, "Unknown name or function");
        assert!(matches!(unknown.view, ResultView::Error { code: "#NAME?", .. }));
    }
}

#[cfg(test)]
mod anchor_files {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn address_drafts_restore_the_effective_sheet_and_cell() {
        let mut f = FormulaFixture::default();
        f.sheets.push("Inputs".into());
        f.formula = "=Z99+1".into();
        f.effective_anchor = "Inputs!H1".into();
        f.inputs.push(FormulaInput {
            address: "Z99".into(),
            kind: "number".into(),
            entry: "4".into(),
        });
        for draft in ["H1", "invalid!"] {
            f.formula_cell = draft.into();
            let restored = FormulaFixture::from_json(&f.to_json().unwrap()).unwrap();
            let mut session = restored.session().unwrap();
            let row = Row {
                id: 0,
                address: restored.inputs[0].address.clone(),
                kind: "number".into(),
                entry: "4".into(),
            };
            let shown = recalculate(
                &mut session,
                &restored.formula,
                &restored.formula_cell,
                &[row],
                &restored.grid,
                "General",
            );
            assert_eq!(display::cell(&session, session.formula_cell()), "Inputs!H1");
            assert_eq!(shown.result, "number 5");
        }
    }
}
