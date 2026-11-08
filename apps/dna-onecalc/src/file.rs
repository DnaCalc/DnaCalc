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
//! The file lists add-ins and never loads one: a reopen shows the list, and loading is the
//! person's choice. A file outside the constraints is refused with an [`OpenRefusal`], which
//! says why and offers the same file in DNA Calc. A session with a VBA module saves as .xlsm once
//! CalcVBA's blob path lands (dc-0f1k.4); until then [`save`] refuses it, naming that bead.

use crate::grid;
use crate::session::{AddinRef, InputValue, Session, SessionOptions};
use calc_doc::{
    CustomXmlItem, SessionArray, SessionConstant, SessionFile, SessionFormula, SessionValue,
    XmlEvent, XmlPart, read_session, write_session,
};
use calc_fml::FormulaForm;
use calc_func::value::{CalcValue, CellKey, CoreValue};

/// The namespace of OneCalc's custom XML item.
pub const NAMESPACE: &str = "urn:dnacalc:onecalc-session";

/// The version of the item this module reads and writes.
pub const VERSION: &str = "1";

/// The bead that lands the .xlsm save of a session with a VBA module.
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

/// A1 text of a cell: `H1`, `AA10`.
pub(crate) fn a1(row: u32, column: u32) -> String {
    let mut letters = Vec::new();
    let mut n = column;
    while n > 0 {
        n -= 1;
        letters.push(char::from(b'A' + (n % 26) as u8));
        n /= 26;
    }
    letters.reverse();
    format!("{}{row}", letters.into_iter().collect::<String>())
}

/// The session a file holds.
pub fn open(bytes: &[u8]) -> Result<Session, OpenRefusal> {
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
    if let Some(item) = &file.custom {
        let (options, addins) = read_item(&item.content).map_err(outside)?;
        session.set_options(options);
        session.set_addins(addins);
    }
    Ok(session)
}

/// The bytes of the file `session` is saved as, with its calculated result as the formula's cached
/// result when that is a single value.
pub fn save(session: &mut Session) -> Result<Vec<u8>, String> {
    if session.has_vba_module() {
        return Err(format!(
            "A session with a VBA module saves as .xlsm once the CalcVBA blob path lands ({VBA_BEAD}); unload the module to save the session as .xlsx."
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
    let cached = if formula.is_some() {
        let calculation = session.calculate();
        calculation.result.as_ref().and_then(cached_value)
    } else {
        None
    };
    let file = SessionFile {
        sheet: sheet_name,
        formula,
        constants,
        custom: Some(CustomXmlItem {
            namespace: NAMESPACE.to_owned(),
            content: write_item(session.options(), session.addins()),
        }),
        cached,
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
        let reopened = open(&bytes).expect("reopened");
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
        let reopened = open(&save(&mut original).unwrap()).unwrap();
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
            cached: None,
        };
        let mut plain = open(&write_session(&file).unwrap()).unwrap();
        assert_eq!(plain.form(), FormulaForm::Legacy);
        assert!(plain.is_plain());
        // H1 is on the rows of A1:A2: implicit intersection takes A1, and the dynamic form spills.
        let value = plain.calculate().result.unwrap();
        assert_eq!(value.core(), &CoreValue::Number(6.0));
        let state = crate::shown::authored_of(&plain);
        assert!(state.legacy && state.plain);
        let again = open(&save(&mut plain).unwrap()).unwrap();
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
            cached: None,
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
}
