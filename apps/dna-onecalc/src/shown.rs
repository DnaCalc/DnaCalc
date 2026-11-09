//! What a surface shows for one authored state, as plain data that crosses a [`crate::port`]: the
//! session answers [`evaluate`] with a [`Shown`], and the surface only draws it.
//!
//! The authored state is [`Authored`]: what the person typed (the formula as shown, the cell it
//! is entered in, whether Ctrl+Shift+Enter entered it as a legacy array formula, the scratch
//! grid's entries, the locale and number format) and the clock the page reads. `evaluate` gives the
//! session that state, calculates, and says what to show. [`authored_of`] is the other direction:
//! the state a session holds, for a surface to show after a file opens.

use crate::cf::{self, CellStyle, CfRuleSpec, LookWire};
use crate::display;
use crate::grid::{self, Entry};
use crate::result::{self, ResultView};
use crate::session::{InputValue, Session, SessionOptions};
use calc_fml::FormulaForm;
use calc_func::value::CellKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// What a person has authored, as the surface holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Authored {
    /// The formula as the field shows it (entry spelling), with its leading `=`.
    pub formula: String,
    /// The text of the formula cell's address field.
    pub formula_cell: String,
    /// Entered with Ctrl+Shift+Enter: Excel's legacy array formula, `{=...}`.
    pub legacy: bool,
    /// With `legacy`: a plain legacy formula as a file stores it (implicit intersection), shown
    /// without array braces. Any edit of the text leaves it.
    #[serde(default)]
    pub plain: bool,
    /// The scratch grid's entries, row by row ([`grid::ROWS`] x [`grid::COLUMNS`]).
    pub grid: Vec<String>,
    pub locale: String,
    pub number_format: String,
    /// Serial dates count from 1904 (the workbook's date system), not 1900.
    #[serde(default)]
    pub date_1904: bool,
    /// The clock (a serial date number) the page reads, for NOW and TODAY.
    pub now: Option<f64>,
    /// The Formatting panel: how the result is drawn. Saved in the session file as the cell format
    /// of the formula's cell and the cells its result fills.
    #[serde(default)]
    pub style: CellStyle,
    /// The Conditional Formatting panel's rules, in priority order. Saved in the session file as
    /// conditional formats over the same cells.
    #[serde(default)]
    pub cf_rules: Vec<CfRuleSpec>,
}

impl Default for Authored {
    fn default() -> Self {
        let options = SessionOptions::default();
        Authored {
            formula: String::new(),
            formula_cell: "H1".to_owned(),
            legacy: false,
            plain: false,
            grid: vec![String::new(); (grid::ROWS * grid::COLUMNS) as usize],
            locale: options.locale,
            number_format: options.number_format,
            date_1904: options.date_1904,
            now: None,
            style: CellStyle::default(),
            cf_rules: Vec::new(),
        }
    }
}

impl Authored {
    /// The example the page starts with and "Try SUM" restores: `=SUM(A1:A3)` over 1.5, 2 and a
    /// blank.
    pub fn example_sum() -> Authored {
        let mut authored = Authored {
            formula: "=SUM(A1:A3)".to_owned(),
            ..Authored::default()
        };
        authored.grid[0] = "1.5".to_owned();
        authored.grid[grid::COLUMNS as usize] = "2".to_owned();
        authored
    }
}

/// What one grid cell shows besides its entry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GridShown {
    /// The type of what the cell holds (`number`, `text`, ...), `formula` for the formula's cell,
    /// `refused` for an entry the grid does not hold, and empty for a blank cell.
    pub kind: String,
    /// The formula's value, for the formula's cell.
    pub value: String,
    /// A tooltip: why an entry was refused, or what the formula's cell holds.
    pub note: String,
}

/// One cell of an array window: its value type and its value as text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowCellView {
    pub kind: String,
    pub text: String,
    /// What the rules, the number format's colour and the Formatting panel give the cell.
    #[serde(default)]
    pub look: Option<LookWire>,
}

/// The result as the surface lays it out ([`ResultView`] with owned text, so it serialises).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum ViewWire {
    #[default]
    NoFormula,
    Value {
        kind: String,
        text: String,
        exact: Option<String>,
        #[serde(default)]
        look: Option<LookWire>,
    },
    Error {
        code: String,
        meaning: String,
        #[serde(default)]
        look: Option<LookWire>,
    },
    Array {
        total_rows: usize,
        total_cols: usize,
        truncated: bool,
        cells: Vec<Vec<WindowCellView>>,
    },
    NotCalculated,
}

impl ViewWire {
    fn of(view: &ResultView) -> ViewWire {
        match view {
            ResultView::NoFormula => ViewWire::NoFormula,
            ResultView::Value { kind, text, exact } => ViewWire::Value {
                kind: (*kind).to_owned(),
                text: text.clone(),
                exact: exact.clone(),
                look: None,
            },
            ResultView::Error { code, meaning } => ViewWire::Error {
                code: (*code).to_owned(),
                meaning: (*meaning).to_owned(),
                look: None,
            },
            ResultView::Array(window) => ViewWire::Array {
                total_rows: window.total_rows,
                total_cols: window.total_cols,
                truncated: window.truncated(),
                cells: window
                    .cells
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(|cell| WindowCellView {
                                kind: cell.kind.to_owned(),
                                text: cell.text.clone(),
                                look: None,
                            })
                            .collect()
                    })
                    .collect(),
            },
            ResultView::NotCalculated => ViewWire::NotCalculated,
        }
    }
}

/// What the surface shows for one authored state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shown {
    pub formula_cell: String,
    pub grid: Vec<GridShown>,
    pub result: String,
    pub state: String,
    pub format_note: String,
    pub view: ViewWire,
    pub provenance: String,
    pub diagnostics: Vec<String>,
    /// The spans of the formula text the diagnostics point at, as (start, length) in UTF-16 code
    /// units.
    pub diagnostic_spans: Vec<(usize, usize)>,
    pub reads: Vec<String>,
    /// The Effective Display panel: the number format, the Formatting panel's choices, what the
    /// conditional formats did, and how the result is drawn.
    #[serde(default)]
    pub effective: Vec<String>,
    /// The Formatting panel's CSS declarations for the result.
    #[serde(default)]
    pub style_css: String,
    /// Which table of registered functions (add-ins and a VBA module) the session calls with. The
    /// functions themselves come once per version, by [`crate::port::Request::FunctionDocs`]: a
    /// table of thousands of functions is not sent with every answer.
    #[serde(default)]
    pub functions_version: u64,
}

impl Shown {
    pub fn cell(&self, index: usize) -> GridShown {
        self.grid.get(index).cloned().unwrap_or_default()
    }
}

/// The grid's cell at a 0-based index, row by row.
fn grid_cell(session: &Session, index: usize) -> Option<CellKey> {
    let (_, sheet) = session.sheets().next()?;
    let row = index as u32 / grid::COLUMNS + 1;
    let column = index as u32 % grid::COLUMNS + 1;
    CellKey::new(sheet, row, column).ok()
}

/// Gives the session `authored`, calculates, and says what to show.
pub fn evaluate(session: &mut Session, authored: &Authored) -> Shown {
    let mut shown = evaluate_authored(session, authored);
    shown.functions_version = session.functions_version();
    shown
}

/// Gives the session everything authored except the formula text: the form, the environment, the
/// options, the formula cell and the grid's inputs. Returns how the formula cell is spelled and
/// what each grid cell shows besides its entry.
pub(crate) fn prepare(session: &mut Session, authored: &Authored) -> (String, Vec<GridShown>) {
    let Authored {
        formula_cell,
        legacy,
        plain,
        grid: entries,
        locale,
        number_format,
        date_1904,
        now,
        style,
        cf_rules,
        ..
    } = authored;
    session.set_look(style.clone(), cf_rules.clone());
    session.set_form(if *legacy {
        FormulaForm::Legacy
    } else {
        FormulaForm::Dynamic
    });
    if *legacy && *plain {
        session.set_plain_legacy();
    }
    session.set_environment(Some(crate::port::environment(locale, *now, *date_1904)));
    session.set_options(SessionOptions {
        locale: locale.clone(),
        number_format: number_format.clone(),
        date_1904: *date_1904,
    });
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

    // The grid's entries, then every grid cell still empty is blank.
    let mut shown_grid = vec![GridShown::default(); entries.len()];
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
                shown_grid[index].kind = result::kind(value.value()).to_owned();
                if session.set_input_at(cell, value).is_ok() {
                    from_grid.insert(cell);
                }
            }
            Entry::Refused(why) => {
                shown_grid[index].kind = "refused".to_owned();
                shown_grid[index].note = why;
            }
        }
    }
    for (index, text) in entries.iter().enumerate() {
        let Some(cell) = grid_cell(session, index) else {
            continue;
        };
        if cell != at && grid::entry(text) == Entry::Blank && session.input(cell).is_none() {
            let _ = session.set_input_at(cell, InputValue::blank());
        }
    }
    (formula_cell, shown_grid)
}

fn evaluate_authored(session: &mut Session, authored: &Authored) -> Shown {
    let (formula_cell, mut shown_grid) = prepare(session, authored);
    let Authored {
        formula,
        grid: entries,
        number_format,
        date_1904,
        ..
    } = authored;
    let at = session.formula_cell();
    session.set_formula(formula);
    let formula_index = (0..entries.len()).find(|&index| grid_cell(session, index) == Some(at));
    if formula.trim().is_empty() {
        if let Some(index) = formula_index {
            shown_grid[index] = GridShown {
                kind: "formula".to_owned(),
                value: String::new(),
                note: "the formula's cell: no formula yet".to_owned(),
            };
        }
        return Shown {
            formula_cell,
            grid: shown_grid,
            result: "no formula".to_owned(),
            state: "Ready".to_owned(),
            ..Shown::default()
        };
    }
    let calculation = session.calculate();
    let profile = session
        .environment()
        .and_then(|environment| environment.profile);
    let date_system = if *date_1904 {
        calc_func::locale::DateSystem::System1904
    } else {
        calc_func::locale::DateSystem::System1900
    };
    let (view, format_note) = match result::formatted_view_windowed(
        calculation.result.as_ref(),
        profile.as_ref(),
        number_format,
        date_system,
        result::Windowing::Scrolling,
    ) {
        Ok(view) => (view, String::new()),
        Err(note) => (
            ResultView::windowed(
                calculation.result.as_ref(),
                profile.as_ref(),
                result::Windowing::Scrolling,
            ),
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
        shown_grid[index] = GridShown {
            kind: "formula".to_owned(),
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
            crate::Diagnostic::Syntax(syntax) => Some((syntax.span.start, syntax.span.len)),
            crate::Diagnostic::Compile(error) => {
                let span = error.span();
                Some((span.start, span.len))
            }
            crate::Diagnostic::Eval(_) | crate::Diagnostic::UserFunction(_) => None,
        })
        .collect();
    let (wire, effective) = present(
        session,
        authored,
        calculation.result.as_ref(),
        &view,
        profile.as_ref(),
        date_system,
    );
    Shown {
        formula_cell,
        grid: shown_grid,
        result: match &calculation.result {
            Some(value) => display::value(value),
            None => "not calculated".to_owned(),
        },
        view: wire,
        effective,
        style_css: authored.style.css(),
        state: state.to_owned(),
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
        functions_version: 0,
    }
}

/// The wire form of the result's view with each cell's look: what the number format's colour and
/// the conditional-format rules give it, and the lines of the Effective Display panel.
fn present(
    session: &Session,
    authored: &Authored,
    result: Option<&calc_func::value::CalcValue>,
    view: &ResultView,
    profile: Option<&calc_func::locale::FormatProfile>,
    date_system: calc_func::locale::DateSystem,
) -> (ViewWire, Vec<String>) {
    use calc_func::value::{CalcArray, CoreValue};
    let mut wire = ViewWire::of(view);
    let mut effective = Vec::new();
    let code = authored.number_format.trim();
    let general = code.is_empty() || code.eq_ignore_ascii_case("General");
    effective.push(format!(
        "Number format: {} ({})",
        if general { "General" } else { code },
        authored.locale
    ));
    effective.extend(authored.style.describe());
    let Some(value) = result else {
        return (wire, effective);
    };
    // The cells the result is a range of: an array's, or the one cell of a scalar.
    let scalar;
    let array: &CalcArray = match value.core() {
        CoreValue::Array(array) => array,
        _ => {
            scalar = CalcArray::from_rows(vec![vec![value.clone()]]).expect("a 1 x 1 array");
            &scalar
        }
    };
    let (rows, cols) = match &wire {
        ViewWire::Array { cells, .. } => (cells.len(), cells.first().map_or(0, Vec::len)),
        _ => (1, 1),
    };
    if rows == 0 || cols == 0 {
        return (wire, effective);
    }
    let outcome = if authored.cf_rules.is_empty() {
        cf::Outcome::default()
    } else {
        cf::evaluate(&authored.cf_rules, array, rows, cols, session.environment())
    };
    let look_of = |row: usize, col: usize| -> Option<LookWire> {
        let cell = array.get(row, col)?;
        let rules = outcome
            .looks
            .get(row)
            .and_then(|r| r.get(col))
            .cloned()
            .unwrap_or_default();
        // A number format a rule gave shows the cell's text; the colour a number format names
        // (the cell's own, or the rule's) is the font colour either way.
        let text = match (&rules.number_format, profile) {
            (Some(rule_code), Some(profile)) => {
                result::formatted_text(cell, profile, rule_code, date_system).ok()
            }
            _ => None,
        };
        let format_code = rules.number_format.as_deref().unwrap_or(code);
        let format_color = (!format_code.is_empty() && !format_code.eq_ignore_ascii_case("General"))
            .then(|| calc_func::cell_color(format_code, cell))
            .flatten()
            .map(|color| color.rgb());
        LookWire::of(&rules, format_color, text)
    };
    match &mut wire {
        ViewWire::Array { cells, .. } => {
            for (r, row) in cells.iter_mut().enumerate() {
                for (c, cell) in row.iter_mut().enumerate() {
                    cell.look = look_of(r, c);
                    if let Some(text) = cell.look.as_ref().and_then(|look| look.text.clone()) {
                        cell.text = text;
                    }
                }
            }
        }
        ViewWire::Value { look, text, .. } => {
            *look = look_of(0, 0);
            if let Some(shown) = look.as_ref().and_then(|look| look.text.clone()) {
                *text = shown;
            }
        }
        ViewWire::Error { look, .. } => *look = look_of(0, 0),
        _ => {}
    }
    effective.extend(outcome.notes.iter().cloned());
    if !authored.cf_rules.is_empty() {
        effective.push(format!(
            "Conditional formats changed {} of {} cell(s) shown.",
            outcome.changed,
            rows * cols
        ));
    }
    if rows == 1 && cols == 1 {
        let look = match &wire {
            ViewWire::Value { look, .. } | ViewWire::Error { look, .. } => look.clone(),
            _ => None,
        };
        if let Some(look) = look {
            effective.push(format!("Result drawn with {}.", look.describe()));
        }
    }
    (wire, effective)
}

/// The authored state a session holds: what the surface shows after a file opens.
pub fn authored_of(session: &Session) -> Authored {
    let mut authored = Authored::default();
    let text = session.formula().trim();
    authored.formula = if text.is_empty() {
        String::new()
    } else if text.starts_with('=') {
        display::formula(text)
    } else {
        display::formula(&format!("={text}"))
    };
    let cell = session.formula_cell();
    authored.formula_cell = crate::file::a1(cell.row(), cell.column());
    authored.legacy = session.form() == FormulaForm::Legacy;
    authored.plain = session.is_plain();
    for index in 0..authored.grid.len() {
        if let Some(cell) = grid_cell(session, index)
            && let Some(input) = session.input(cell)
        {
            authored.grid[index] = crate::file::entry_text(input);
        }
    }
    authored.locale = session.options().locale.clone();
    authored.number_format = session.options().number_format.clone();
    authored.date_1904 = session.options().date_1904;
    let (style, rules) = session.look();
    authored.style = style.clone();
    authored.cf_rules = rules.to_vec();
    authored
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

    fn authored(formula: &str, cells: &[(usize, &str)]) -> Authored {
        Authored {
            formula: formula.to_owned(),
            grid: entries(cells),
            ..Authored::default()
        }
    }

    // Converted from the surface's `the_grid_supplies_cells_and_shows_the_formula_cell`: the same
    // cases through `evaluate`; the typed-input rows it also covered are gone with the rows
    // (a session file holds the grid's constants only).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_grid_supplies_cells_and_shows_the_formula_cell() {
        let mut session = Session::new("Sheet1").expect("a session");
        let shown = evaluate(&mut session, &authored("=SUM(A1:A4)*2", &[(0, "1.5"), (8, "2")]));
        assert_eq!(shown.result, "number 7");
        assert_eq!(
            shown.view,
            ViewWire::Value {
                kind: "number".to_owned(),
                text: "7".to_owned(),
                exact: None,
                look: None
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
        let shown = evaluate(&mut session, &authored("=A1+1", &[(0, "=2")]));
        assert_eq!(shown.grid[0].kind, "refused");
        assert_eq!(shown.result, "not calculated");
        assert_eq!(shown.diagnostics, ["missing input: Sheet1!A1 has no input"]);

        // An array result and an error result.
        let shown = evaluate(&mut session, &authored("={1,2;3,4}", &[]));
        assert!(
            matches!(&shown.view, ViewWire::Array { total_rows: 2, .. }),
            "{shown:?}"
        );
        assert_eq!(shown.grid[7].value, "{2x2 array}");
        let shown = evaluate(&mut session, &authored("=1/0", &[]));
        assert_eq!(
            shown.view,
            ViewWire::Error {
                code: "#DIV/0!".to_owned(),
                meaning: "division by zero".to_owned(),
                look: None
            }
        );
        // A syntax diagnostic points into the text.
        let shown = evaluate(&mut session, &authored("=A1*", &[]));
        assert_eq!(shown.diagnostic_spans, [(4, 0)]);
    }

    // Converted from the surface's `restored_authored_inputs_recalculate_and_format_through_the_surface_route`
    // (a JSON session file): the same locale, format and states, the state now entering through a
    // saved and reopened workbook.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_reopened_file_recalculates_and_formats_through_the_surface_route() {
        let mut authored = Authored::example_sum();
        authored.locale = "de-DE".to_owned();
        authored.number_format = "0,00".to_owned();
        let mut session = Session::new("Sheet1").unwrap();
        evaluate(&mut session, &authored);
        let bytes = crate::file::save(&mut session).expect("saved");
        let mut reopened = crate::file::open(&bytes).expect("reopened").0;
        let state = authored_of(&reopened);
        assert_eq!(state, authored);
        let shown = evaluate(&mut reopened, &state);
        assert_eq!(shown.state, "Calculated");
        assert_eq!(shown.result, "number 3.5");
        assert!(matches!(&shown.view, ViewWire::Value { text, .. } if text == "3,50"));
        let missing = evaluate(&mut reopened, &Authored { formula: "=Z99+1".into(), ..state.clone() });
        assert_eq!(missing.state, "Missing input");
        let unknown = evaluate(
            &mut reopened,
            &Authored {
                formula: "=NO_SUCH_ONECALC_FUNCTION()".into(),
                ..state
            },
        );
        assert_eq!(unknown.state, "Unknown name or function");
        assert!(matches!(&unknown.view, ViewWire::Error { code, .. } if code == "#NAME?"));
    }

    // Converted from the surface's `address_drafts_restore_the_effective_sheet_and_cell` (several
    // sheets and anchors in a JSON file): a session has one sheet, so the draft that matters is
    // an invalid address, which keeps the effective anchor and is saved as it.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn an_invalid_address_draft_keeps_the_effective_anchor_and_a_file_keeps_the_anchor() {
        let mut session = Session::new("Sheet1").unwrap();
        let mut state = authored("=B2+1", &[(9, "4")]);
        state.formula_cell = "C7".to_owned();
        let shown = evaluate(&mut session, &state);
        assert_eq!(shown.formula_cell, "Sheet1!C7");
        state.formula_cell = "invalid!".to_owned();
        let shown = evaluate(&mut session, &state);
        assert!(shown.formula_cell.contains("the formula stays in Sheet1!C7"), "{shown:?}");
        assert_eq!(shown.result, "number 5");
        let reopened = crate::file::open(&crate::file::save(&mut session).unwrap()).unwrap().0;
        assert_eq!(authored_of(&reopened).formula_cell, "C7");
        assert_eq!(display::cell(&reopened, reopened.formula_cell()), "Sheet1!C7");
    }

    /// Excel's answers to 3,822 formula entries (dc-mx3x), through the route the page runs:
    /// the formula goes to the session as authored text and the page shows what it answers.
    const ENTRY_ANSWERS: &str =
        include_str!("../../../crates/calc-fml/tests/fixtures/entry/excel-entry.tsv");

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_page_refuses_the_formulas_excel_refuses_on_entry_and_only_those() {
        let mut session = Session::new("Sheet1").unwrap();
        let (mut count, mut wrong) = (0, Vec::new());
        for line in ENTRY_ANSWERS.lines().filter(|line| !line.starts_with('#')) {
            let (answer, formula) = line.split_once('\t').expect("an answer and a formula");
            let shown = evaluate(
                &mut session,
                &Authored {
                    formula: formula.to_owned(),
                    ..Authored::default()
                },
            );
            let refused = shown.diagnostics.iter().any(|diagnostic| {
                diagnostic.starts_with("Excel refuses this formula")
                    || diagnostic.starts_with("syntax at")
            });
            if refused != (answer == "0") {
                wrong.push(format!("{formula}: Excel {answer}, page {:?}", shown.diagnostics));
            }
            count += 1;
        }
        assert_eq!(count, 3822);
        assert!(wrong.is_empty(), "{} differ:\n{}", wrong.len(), wrong.join("\n"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_date_system_changes_the_dates_a_session_calculates_and_shows() {
        let mut session = Session::new("Sheet1").unwrap();
        let mut state = Authored {
            formula: "=DATE(2024,1,31)".to_owned(),
            number_format: "yyyy-mm-dd".to_owned(),
            ..Authored::default()
        };
        let shown = evaluate(&mut session, &state);
        assert_eq!(shown.result, "number 45322");
        assert!(matches!(&shown.view, ViewWire::Value { text, .. } if text == "2024-01-31"));
        state.date_1904 = true;
        let shown = evaluate(&mut session, &state);
        assert_eq!(shown.result, "number 43860");
        assert!(matches!(&shown.view, ViewWire::Value { text, .. } if text == "2024-01-31"), "{shown:?}");
        // The date system is the workbook's own: it goes into the file and comes back.
        let reopened = crate::file::open(&crate::file::save(&mut session).unwrap()).unwrap().0;
        assert!(reopened.options().date_1904);
        assert!(authored_of(&reopened).date_1904);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_shown_state_serialises() {
        let mut session = Session::new("Sheet1").unwrap();
        let state = Authored::example_sum();
        let text = serde_json::to_string(&state).unwrap();
        assert_eq!(serde_json::from_str::<Authored>(&text).unwrap(), state);
        let shown = evaluate(&mut session, &state);
        let text = serde_json::to_string(&shown).unwrap();
        assert_eq!(serde_json::from_str::<Shown>(&text).unwrap(), shown);
    }
}
