//! The OneCalc file: a constrained Excel workbook, read and written through calc-doc.
//!
//! A session is a valid .xlsx with one sheet, at most one formula cell (the session's one formula,
//! at its anchor) and the scratch grid's constants as constant cells on that sheet
//! ([`calc_doc::SessionFile`] holds that content and does the reading and writing; this module
//! maps it to and from a [`Session`]). What Excel's format has no place for goes in one custom XML
//! item of our own namespace ([`NAMESPACE`]), which Excel keeps when it saves the file: the
//! locale and number format, and the add-in list (kind, path as chosen, file hash).
//!
//! ```text
//! <onecalc xmlns="urn:dnacalc:onecalc-session" version="1">
//!   <options locale="de-DE" numberFormat="0,00"/>
//!   <addins><addin kind="xll" path="C:\libs\My.xll" hash="sha256:..."/></addins>
//! </onecalc>
//! ```
//!
//! The Formatting and Conditional Formatting panels are saved as ordinary Excel content: the
//! style as the cell format of the formula's cell and of the cells its result fills, the rules as
//! conditional formats (`cfRule` and `dxf`) over the same cells, with their formulas stored for
//! the range's first cell. Excel opens the file and shows the same formats and rules; a reopen
//! restores the panels.
//!
//! The file lists add-ins and never loads one: a reopen shows the list, and loading is the
//! person's choice. A file outside the constraints is refused with an [`OpenRefusal`], which
//! says why and offers the same file in DNA Calc.
//!
//! A session file can carry a VBA project: it is then an .xlsm (theory 5a, dc-0f1k.4). calc-doc
//! reads and writes the `vbaProject.bin` part as an opaque blob; [`open`] hands it over with the
//! session, [`crate::Session::open_project`] has CalcVBA compile it, and [`save`] puts the part
//! back as the session holds it: unchanged, or changed when a module was edited. A module typed
//! into a session without a project has no project part to go into, so such a session is not
//! saved until the module is unloaded.

use crate::grid;
use crate::session::{AddinRef, InputValue, Session, SessionOptions};
use crate::cf::{CellStyle, CfRuleSpec};
use calc_doc::{
    CustomXmlItem, SessionArray, SessionConstant, SessionFile, SessionFormula, SessionLook,
    SessionValue, XmlEvent, XmlPart, read_session, write_session,
};
use calc_fml::FormulaForm;
use calc_func::value::{CalcValue, CellKey, CoreValue};

/// The namespace of OneCalc's custom XML item.
pub const NAMESPACE: &str = "urn:dnacalc:onecalc-session";

/// The version of the item this module reads and writes.
pub const VERSION: &str = "1";

/// The bead that owns the .xlsm save of a session with VBA.
pub const VBA_BEAD: &str = "dc-0f1k.4";

/// What a refused file says it can do instead.
pub const OFFER: &str = "Open in DNA Calc";

/// Why a file is not a OneCalc session: the reason, and the offer to open it in DNA Calc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRefusal {
    pub reason: String,
}

impl std::fmt::Display for OpenRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "This file is outside what a OneCalc session holds: {}. {OFFER}.",
            self.reason
        )
    }
}

fn outside(reason: impl std::fmt::Display) -> OpenRefusal {
    OpenRefusal {
        reason: reason.to_string(),
    }
}

/// A column's letters: `A`, `AA`.
pub(crate) fn column_letters(column: u32) -> String {
    let mut letters = Vec::new();
    let mut n = column;
    while n > 0 {
        n -= 1;
        letters.push(char::from(b'A' + (n % 26) as u8));
        n /= 26;
    }
    letters.reverse();
    letters.into_iter().collect()
}

/// A1 text of a cell: `H1`, `AA10`.
pub(crate) fn a1(row: u32, column: u32) -> String {
    format!("{}{row}", column_letters(column))
}

/// The session a file holds, and its VBA project part when it carries one (for
/// [`crate::Session::open_project`], which compiles it).
pub fn open(bytes: &[u8]) -> Result<(Session, Option<calc_doc::SessionVba>), OpenRefusal> {
    let file = read_session(bytes.to_vec(), NAMESPACE).map_err(outside)?;
    let mut session = Session::new(&file.sheet).map_err(|error| {
        outside(format!(
            "its sheet: {}",
            crate::display::sheet_error(&error)
        ))
    })?;
    let (_, sheet) = session.sheets().next().expect("a session has its sheet");
    // The formula's cell first: a constant never takes it. Without a formula the cell is the
    // surface's default, H1, or J1 (outside the grid) when a constant holds H1.
    let anchor = match &file.formula {
        Some(formula) => a1(formula.row, formula.column),
        // The look's cell when a session without a formula keeps formats on one.
        None if file.look.is_some() => {
            let look = file.look.as_ref().expect("checked");
            a1(look.row, look.column)
        }
        None => {
            let taken = file.constants.iter().any(|c| (c.row, c.column) == (1, grid::COLUMNS));
            a1(1, if taken { grid::COLUMNS + 2 } else { grid::COLUMNS })
        }
    };
    session.set_formula_cell(&anchor).map_err(|_| {
        outside(format!(
            "the formula is in {anchor}, which the session cannot enter it in"
        ))
    })?;
    if let Some(formula) = &file.formula {
        session.set_formula(&format!("={}", formula.text));
        match formula.array {
            SessionArray::Legacy => session.set_form(FormulaForm::Legacy),
            // A plain `<f>` is a legacy formula: implicit intersection, not the dynamic form.
            // (Excel 365 itself stores a formula that cannot give several values this way, where
            // the two forms agree; that one stays in the dynamic form.)
            SessionArray::Plain if calc_doc::plain_formula_needs_legacy(&formula.text) => {
                session.set_plain_legacy()
            }
            SessionArray::Plain | SessionArray::Dynamic => session.set_form(FormulaForm::Dynamic),
        }
    }
    for SessionConstant { row, column, value } in &file.constants {
        if *row > grid::ROWS || *column > grid::COLUMNS {
            return Err(outside(format!(
                "a constant in {}, outside the scratch grid A1:{}",
                a1(*row, *column),
                grid::address(grid::ROWS, grid::COLUMNS)
            )));
        }
        let input = match value {
            SessionValue::Number(number) => InputValue::number(*number).map_err(|_| {
                outside(format!("the number in {} is not one a cell holds", a1(*row, *column)))
            })?,
            SessionValue::Text(text) => InputValue::text(text).map_err(|_| {
                outside(format!("the text in {} is too long", a1(*row, *column)))
            })?,
            SessionValue::Logical(value) => InputValue::logical(*value),
            SessionValue::Error(code) => InputValue::error(*code),
        };
        let cell = CellKey::new(sheet, *row, *column).expect("a cell of the grid");
        session
            .set_input_at(cell, input)
            .map_err(|_| outside(format!("{} holds the formula and a constant", a1(*row, *column))))?;
    }
    let (mut options, addins) = match &file.custom {
        Some(item) => read_item(&item.content).map_err(outside)?,
        None => (SessionOptions::default(), Vec::new()),
    };
    // The date system is the workbook's own setting, not part of our item.
    options.date_1904 = file.date_1904;
    session.set_options(options);
    session.set_addins(addins);
    if let Some(look) = &file.look {
        let cell = session.formula_cell();
        let (rows, columns) = (i64::from(cell.row()) - 1, i64::from(cell.column()) - 1);
        let mut rules = Vec::new();
        if !look.rules.is_empty() && (look.row, look.column) != (cell.row(), cell.column()) {
            return Err(outside(format!(
                "its conditional formats start in {}, not in the formula's cell {}",
                a1(look.row, look.column),
                a1(cell.row(), cell.column())
            )));
        }
        for (index, rule) in look.rules.iter().enumerate() {
            rules.push(
                CfRuleSpec::of_session_rule(rule, rows, columns)
                    .map_err(|why| outside(format!("conditional format {}: {why}", index + 1)))?,
            );
        }
        let style = look.format.as_ref().map(CellStyle::of_session_format).unwrap_or_default();
        session.set_look(style, rules);
    }
    Ok((session, file.vba))
}

/// The VBA project of an add-in file (an .xlam), as CalcVBA reads its project part (calc-doc
/// reads only the package). An add-in without a VBA project is refused.
pub fn read_vba_addin(bytes: &[u8]) -> Result<calc_vba::OfficeProject, String> {
    let vba = calc_doc::read_vba_file(bytes.to_vec())
        .map_err(|refusal| refusal.to_string())?
        .ok_or("the add-in has no VBA project")?;
    calc_vba::read_office_project(&vba.project)
}

/// The bytes of the file `session` is saved as, with its calculated result as the formula's cached
/// result when that is a single value.
pub fn save(session: &mut Session) -> Result<Vec<u8>, String> {
    if session.has_vba_module() {
        return Err(format!(
            "A VBA module typed into a session without a VBA project has no project to be saved in ({VBA_BEAD}): open an .xlsm session to keep modules in its project, or unload the module to save the session as .xlsx."
        ));
    }
    let (_, sheet) = session.sheets().next().expect("a session has its sheet");
    let sheet_name = session.sheet_name(sheet).expect("its name").to_owned();
    let formula_cell = session.formula_cell();
    let constants = session
        .inputs()
        .filter(|(_, input)| !input.value().is_empty())
        .filter_map(|(cell, input)| {
            let value = match input.value().core() {
                CoreValue::Number(number) => SessionValue::Number(*number),
                CoreValue::Text(text) => SessionValue::Text(text.to_string_lossy()),
                CoreValue::Logical(value) => SessionValue::Logical(*value),
                CoreValue::Error(code) => SessionValue::Error(*code),
                _ => return None,
            };
            Some(SessionConstant {
                row: cell.row(),
                column: cell.column(),
                value,
            })
        })
        .collect();
    let text = session.formula().trim();
    let formula = (!text.is_empty()).then(|| SessionFormula {
        row: formula_cell.row(),
        column: formula_cell.column(),
        text: text.strip_prefix('=').unwrap_or(text).to_owned(),
        array: match session.form() {
            FormulaForm::Legacy if session.is_plain() => SessionArray::Plain,
            FormulaForm::Legacy => SessionArray::Legacy,
            FormulaForm::Dynamic => SessionArray::Dynamic,
        },
    });
    let mut shape = (1u32, 1u32);
    let cached = if formula.is_some() {
        let calculation = session.calculate();
        // The cells the result fills: the window the panels draw and test the rules over.
        if let Some(CoreValue::Array(array)) = calculation.result.as_ref().map(CalcValue::core) {
            let s = array.shape();
            let columns = s.cols.min(dna_formula_ui::result::MAX_SCROLL_WINDOW_COLS).max(1);
            let rows = s
                .rows
                .min(dna_formula_ui::result::MAX_SCROLL_WINDOW_ROWS)
                .min((dna_formula_ui::result::MAX_SCROLL_WINDOW_CELLS / columns).max(1))
                .max(1);
            shape = (rows as u32, columns as u32);
        }
        calculation.result.as_ref().and_then(cached_value)
    } else {
        None
    };
    let (style, specs) = session.look();
    let look = if style.is_default() && specs.is_empty() {
        None
    } else {
        let (dr, dc) = (i64::from(formula_cell.row()) - 1, i64::from(formula_cell.column()) - 1);
        let mut rules = Vec::new();
        for (index, spec) in specs.iter().enumerate() {
            rules.push(spec.session_rule(index as i64 + 1, dr, dc).map_err(|why| {
                format!("Conditional format {} cannot be saved: {why}. Complete or remove it first.", index + 1)
            })?);
        }
        Some(SessionLook {
            row: formula_cell.row(),
            column: formula_cell.column(),
            rows: shape.0,
            columns: shape.1,
            format: (!style.is_default()).then(|| style.session_format()),
            rules,
        })
    };
    let file = SessionFile {
        sheet: sheet_name,
        formula,
        constants,
        custom: Some(CustomXmlItem {
            namespace: NAMESPACE.to_owned(),
            content: write_item(session.options(), session.addins()),
        }),
        date_1904: session.options().date_1904,
        cached,
        vba: session.project().map(|project| project.vba.clone()),
        look,
    };
    write_session(&file).map_err(|refusal| refusal.to_string())
}

fn cached_value(value: &CalcValue) -> Option<SessionValue> {
    Some(match value.core() {
        CoreValue::Number(number) => SessionValue::Number(*number),
        CoreValue::Text(text) => SessionValue::Text(text.to_string_lossy()),
        CoreValue::Logical(value) => SessionValue::Logical(*value),
        CoreValue::Error(code) => SessionValue::Error(*code),
        _ => return None,
    })
}

fn escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// The custom XML item's content for `options` and `addins`.
pub fn write_item(options: &SessionOptions, addins: &[AddinRef]) -> Vec<u8> {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<onecalc xmlns=\"{NAMESPACE}\" version=\"{VERSION}\"><options locale=\"{}\" numberFormat=\"{}\"/><addins>",
        escape(&options.locale),
        escape(&options.number_format)
    );
    for addin in addins {
        out.push_str(&format!(
            "<addin kind=\"{}\" path=\"{}\" hash=\"{}\"/>",
            escape(&addin.kind),
            escape(&addin.path),
            escape(&addin.hash)
        ));
    }
    out.push_str("</addins></onecalc>");
    out.into_bytes()
}

/// The options and add-in list a custom XML item holds. An item of another version, or without
/// its document element, is refused; an option a version 1 item lacks keeps its default.
pub fn read_item(content: &[u8]) -> Result<(SessionOptions, Vec<AddinRef>), String> {
    let part = XmlPart::from_bytes("customXml/onecalc", content.to_vec());
    let mut reader = part.reader();
    let mut options = SessionOptions::default();
    let mut addins = Vec::new();
    let mut root = false;
    while let Some(event) = reader.next_event().map_err(|error| error.to_string())? {
        let XmlEvent::Start(element) = event else {
            continue;
        };
        if element.name.namespace.as_deref() != Some(NAMESPACE) {
            continue;
        }
        match element.name.local.as_str() {
            "onecalc" => {
                root = true;
                if element.attribute("version") != Some(VERSION) {
                    return Err(format!(
                        "the OneCalc part is version {:?}, which this version does not read",
                        element.attribute("version").unwrap_or("")
                    ));
                }
            }
            "options" => {
                if let Some(locale) = element.attribute("locale") {
                    if !SessionOptions::LOCALES.contains(&locale) {
                        return Err(format!("the locale {locale:?} is not one OneCalc offers"));
                    }
                    options.locale = locale.to_owned();
                }
                if let Some(format) = element.attribute("numberFormat") {
                    options.number_format = format.to_owned();
                }
            }
            "addin" => addins.push(AddinRef {
                kind: element.attribute("kind").unwrap_or("").to_owned(),
                path: element.attribute("path").unwrap_or("").to_owned(),
                hash: element.attribute("hash").unwrap_or("").to_owned(),
            }),
            _ => {}
        }
    }
    if !root {
        return Err("the OneCalc part has no onecalc element".to_owned());
    }
    Ok((options, addins))
}

/// A grid entry's text for an input: what typed into the grid gives the input back
/// ([`grid::entry`]).
pub fn entry_text(input: &InputValue) -> String {
    let value = input.value();
    match value.core() {
        CoreValue::Number(number) => {
            if number.abs() >= 1e15 || (*number != 0.0 && number.abs() < 1e-5) {
                format!("{number:e}")
            } else {
                format!("{number}")
            }
        }
        CoreValue::Text(text) => {
            let text = text.to_string_lossy();
            // A leading apostrophe makes the rest text, so text the grid would read as a number,
            // a logical, an error, a formula or a blank keeps one.
            match grid::entry(&text) {
                grid::Entry::Value(read) if read.value() == value => text,
                _ => format!("'{text}"),
            }
        }
        CoreValue::Logical(true) => "TRUE".to_owned(),
        CoreValue::Logical(false) => "FALSE".to_owned(),
        CoreValue::Error(code) => crate::display::error_spelling(*code).to_owned(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        let mut session = Session::new("Sheet1").unwrap();
        session.set_formula("=SUM(A1:A3)+LEN(B1)");
        session.set_formula_cell("H1").unwrap();
        for (address, text) in [("A1", "1.5"), ("A2", "2"), ("B1", "'12"), ("B2", "TRUE"), ("B3", "#N/A"), ("C1", "é & < >")] {
            let grid::Entry::Value(value) = grid::entry(text) else {
                panic!("{text}")
            };
            session.set_input(address, value).unwrap();
        }
        session.set_options(SessionOptions {
            locale: "de-DE".into(),
            number_format: "0,00".into(),
            date_1904: false,
        });
        session.set_addins(vec![AddinRef {
            kind: "xll".into(),
            path: "C:\\libs\\My & Add.xll".into(),
            hash: "sha256:abcd".into(),
        }]);
        session
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_session_saves_as_a_workbook_and_reopens_the_same() {
        let mut original = session();
        let bytes = save(&mut original).expect("saved");
        let reopened = open(&bytes).expect("reopened").0;
        assert_eq!(reopened.formula(), original.formula());
        assert_eq!(reopened.formula_cell(), original.formula_cell());
        assert_eq!(reopened.options(), original.options());
        assert_eq!(reopened.addins(), original.addins());
        let held = |s: &Session| -> Vec<_> {
            s.inputs()
                .filter(|(_, i)| !i.value().is_empty())
                .map(|(cell, input)| (cell.row(), cell.column(), input.clone()))
                .collect()
        };
        assert_eq!(held(&reopened), held(&original));
        assert_eq!(reopened.form(), FormulaForm::Dynamic);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_legacy_array_form_is_saved_as_an_array_formula() {
        let mut original = session();
        original.set_formula("=A1:A2*2");
        original.set_form(FormulaForm::Legacy);
        let reopened = open(&save(&mut original).unwrap()).unwrap().0;
        assert_eq!(reopened.form(), FormulaForm::Legacy);
        assert_eq!(reopened.formula(), "=A1:A2*2");
    }

    // A plain `<f>` (no array attribute, no dynamic marker) is a legacy formula: implicit
    // intersection. It is not the dynamic form, and it saves back as plain.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_plain_file_formula_is_the_legacy_form_and_saves_as_plain() {
        let file = SessionFile {
            sheet: "Sheet1".into(),
            formula: Some(SessionFormula {
                row: 1,
                column: 8,
                text: "A1:A2*2".into(),
                array: SessionArray::Plain,
            }),
            constants: vec![
                SessionConstant { row: 1, column: 1, value: SessionValue::Number(3.0) },
                SessionConstant { row: 2, column: 1, value: SessionValue::Number(4.0) },
            ],
            custom: None,
            date_1904: false,
            cached: None,
            vba: None,
            look: None,
        };
        let mut plain = open(&write_session(&file).unwrap()).unwrap().0;
        assert_eq!(plain.form(), FormulaForm::Legacy);
        assert!(plain.is_plain());
        // H1 is on the rows of A1:A2: implicit intersection takes A1, and the dynamic form spills.
        let value = plain.calculate().result.unwrap();
        assert_eq!(value.core(), &CoreValue::Number(6.0));
        let state = crate::shown::authored_of(&plain);
        assert!(state.legacy && state.plain);
        let again = open(&save(&mut plain).unwrap()).unwrap().0;
        assert!(again.is_plain(), "saved as a plain formula");
        // Entering it as an array formula, or as the dynamic form, leaves the plain form.
        plain.set_form(FormulaForm::Legacy);
        assert!(!plain.is_plain());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_session_with_a_vba_module_refuses_to_save_naming_the_bead() {
        let mut original = session();
        original
            .load_vba_module("Module1", calc_vba::EXAMPLE_MODULE, calc_vba::DEFAULT_STEP_BUDGET)
            .unwrap();
        let refusal = save(&mut original).unwrap_err();
        assert!(refusal.contains(VBA_BEAD), "{refusal}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_file_outside_the_constraints_offers_dna_calc() {
        let refusal = open(b"not a workbook").unwrap_err();
        assert!(refusal.to_string().ends_with("Open in DNA Calc."), "{refusal}");
        // A constant outside the scratch grid is refused, naming the cell.
        let file = SessionFile {
            sheet: "Sheet1".into(),
            formula: None,
            constants: vec![SessionConstant {
                row: 99,
                column: 26,
                value: SessionValue::Number(4.0),
            }],
            custom: None,
            date_1904: false,
            cached: None,
            vba: None,
            look: None,
        };
        let refusal = open(&write_session(&file).unwrap()).unwrap_err();
        assert!(refusal.reason.contains("Z99"), "{refusal}");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn grid_entries_read_back_from_inputs() {
        for text in ["1.5", "-2", "1e21", "0.1", "TRUE", "FALSE", "#N/A", "abc", "'12", "'TRUE", "'=1", "'#N/A", "'1e3"] {
            let grid::Entry::Value(value) = grid::entry(text) else {
                panic!("{text}")
            };
            let back = entry_text(&value);
            let grid::Entry::Value(again) = grid::entry(&back) else {
                panic!("{back}")
            };
            assert_eq!(again, value, "{text} -> {back}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_custom_item_refuses_another_version_and_an_unknown_locale() {
        let item = |attributes: &str, body: &str| {
            format!("<onecalc xmlns=\"{NAMESPACE}\" {attributes}>{body}</onecalc>").into_bytes()
        };
        assert!(read_item(&item("version=\"2\"", "")).is_err());
        assert!(read_item(&item("version=\"1\"", "<options locale=\"xx-XX\"/>")).is_err());
        assert!(read_item(b"<other/>").is_err());
        let (options, addins) = read_item(&item("version=\"1\"", "")).unwrap();
        assert_eq!(options, SessionOptions::default());
        assert!(addins.is_empty());
    }

    fn look_style() -> CellStyle {
        CellStyle {
            font_family: "Consolas".into(),
            font_size: 14,
            bold: true,
            italic: true,
            underline: true,
            strike: true,
            text_color: "#336699".into(),
            fill: "#ffee99".into(),
            horizontal: "center".into(),
            wrap: true,
            border_top: true,
            border_bottom: true,
            border_left: false,
            border_right: true,
            border_color: "#112233".into(),
        }
    }

    fn look_rules() -> Vec<CfRuleSpec> {
        let with = |kind: &str, edit: &dyn Fn(&mut CfRuleSpec)| {
            let mut spec = CfRuleSpec::new(kind);
            edit(&mut spec);
            spec
        };
        vec![
            with("cellIs", &|s| {
                s.operator = "between".into();
                s.operand1 = "2".into();
                s.operand2 = "B1+5".into();
                s.stop = true;
                s.bold = true;
                s.number_format = "0.00\"x\"".into();
            }),
            with("text", &|s| {
                s.operator = "beginsWith".into();
                s.operand1 = "a \"q\" & b".into();
            }),
            with("blanks", &|s| s.operator = "noBlanks".into()),
            with("errors", &|_| {}),
            with("dates", &|s| s.operator = "lastWeek".into()),
            with("expression", &|s| {
                s.operand1 = "AND(A1>2,B1<$C$3,\"A1\"<>\"\")".into();
                s.italic = true;
                s.underline = true;
                s.strike = true;
            }),
            with("rank", &|s| {
                s.operator = "bottom".into();
                s.rank = 25;
                s.percent = true;
            }),
            with("average", &|s| {
                s.operator = "below".into();
                s.equal = true;
            }),
            with("duplicates", &|_| {}),
            with("uniques", &|s| s.fill = "#c6efce".into()),
            with("colorScale", &|s| s.mid = String::new()),
            with("colorScale", &|s| s.low = "#112233".into()),
            with("dataBar", &|s| s.bar_color = "#ff0000".into()),
            with("iconSet", &|s| {
                s.icon_set = "4TrafficLights".into();
                s.reverse = true;
            }),
        ]
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_formatting_and_the_rules_are_saved_as_ordinary_excel_formats_and_restored() {
        let mut original = Session::new("Sheet1").unwrap();
        original.set_formula("=SEQUENCE(3,2)");
        original.set_formula_cell("K4").unwrap();
        original.set_look(look_style(), look_rules());
        let bytes = save(&mut original).unwrap();

        // The panels come back as they were.
        let reopened = open(&bytes).unwrap().0;
        let (style, rules) = reopened.look();
        assert_eq!(style, &look_style());
        assert_eq!(rules, look_rules().as_slice());

        // The file holds them as cell formats on the formula's cell and the cells its result
        // fills (K4:L6), and as conditional formats over the same cells: no custom XML for them.
        let document = calc_doc::open(
            std::sync::Arc::new(calc_doc::MemorySource::new(bytes.clone())),
            &calc_doc::OpenPolicy::default(),
        )
        .unwrap();
        let sheet = String::from_utf8(document.package().read_part("xl/worksheets/sheet1.xml").unwrap()).unwrap();
        assert!(sheet.contains("sqref=\"K4:L6\""), "{sheet}");
        for cell in ["K4", "L4", "K5", "L5", "K6", "L6"] {
            assert!(sheet.contains(&format!("<c r=\"{cell}\" s=\"1\"")), "{cell}: {sheet}");
        }
        // A rule's formula is stored for the range's first cell: the panel's A1 is K4.
        assert!(sheet.contains("<formula>AND(K4&gt;2,L4&lt;$C$3,\"A1\"&lt;&gt;\"\")</formula>"), "{sheet}");
        assert!(sheet.contains("<formula>LEFT(K4,LEN(\"a \"\"q\"\" &amp; b\"))=\"a \"\"q\"\" &amp; b\"</formula>"), "{sheet}");
        let custom = String::from_utf8(document.package().read_part("customXml/item1.xml").unwrap_or_default()).unwrap();
        assert!(!custom.contains("cfRule") && !custom.contains("Consolas"), "{custom}");

        // Saved again, the same file content.
        let mut again = open(&bytes).unwrap().0;
        assert_eq!(save(&mut again).unwrap().len(), bytes.len());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_scalar_result_and_an_empty_session_keep_their_look_and_an_unfinished_rule_is_not_dropped() {
        let mut scalar = Session::new("Sheet1").unwrap();
        scalar.set_formula("=1+1");
        scalar.set_look(CellStyle { bold: true, ..CellStyle::default() }, vec![CfRuleSpec::new("duplicates")]);
        let reopened = open(&save(&mut scalar).unwrap()).unwrap().0;
        assert!(reopened.look().0.bold);
        assert_eq!(reopened.look().1.len(), 1);

        // No formula yet: the look stays on the formula's cell.
        let mut empty = Session::new("Sheet1").unwrap();
        empty.set_formula_cell("C7").unwrap();
        empty.set_look(CellStyle { fill: "#ff0000".into(), ..CellStyle::default() }, Vec::new());
        let reopened = open(&save(&mut empty).unwrap()).unwrap().0;
        assert_eq!(reopened.look().0.fill, "#ff0000");
        assert_eq!(crate::file::a1(reopened.formula_cell().row(), reopened.formula_cell().column()), "C7");

        // A rule that is not finished (it needs a value) stops the save with its reason.
        let mut unfinished = Session::new("Sheet1").unwrap();
        unfinished.set_formula("=1");
        unfinished.set_look(CellStyle::default(), vec![CfRuleSpec::new("cellIs")]);
        let refusal = save(&mut unfinished).unwrap_err();
        assert!(refusal.contains("Conditional format 1 cannot be saved"), "{refusal}");
        // So does a solid data bar, which the standard rule cannot hold.
        let mut solid = Session::new("Sheet1").unwrap();
        solid.set_formula("=1");
        solid.set_look(CellStyle::default(), vec![CfRuleSpec { gradient: false, ..CfRuleSpec::new("dataBar") }]);
        assert!(save(&mut solid).unwrap_err().contains("gradient"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rule_formulas_move_with_their_range_and_back() {
        use crate::cf::shift_formula;
        assert_eq!(shift_formula("A1>B$2+$C3+$D$4", 3, 10).unwrap(), "K4>L$2+$C6+$D$4");
        assert_eq!(shift_formula("K4>L$2+$C6+$D$4", -3, -10).unwrap(), "A1>B$2+$C3+$D$4");
        // Text, function names that look like cells, names and a sheet's references.
        assert_eq!(
            shift_formula("LOG10(A1)+\"A1\"&'My A1'!B2+Sheet1!C3+x1y", 1, 1).unwrap(),
            "LOG10(B2)+\"A1\"&'My A1'!C3+Sheet1!D4+x1y"
        );
        assert!(shift_formula("A1", -1, 0).is_err());
    }
}
