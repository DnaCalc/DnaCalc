//! The constrained workbook a formula session is kept in: an ordinary .xlsx with one sheet, at most
//! one formula cell and constants in the other cells, read and written through this codec.
//!
//! A formula-session application (DNA OneCalc) keeps one Excel formula, the cell it is entered
//! in and the constants it reads. [`SessionFile`] is that content as plain data, and
//! [`read_session`] and [`write_session`] are its two directions, over the codec's own route
//! ([`open`], [`load`], [`prepare_edit`], [`prepare_save`] and [`write()`]): no second reader and
//! no second writer. A session saved here is a valid workbook Excel opens with the same formula
//! and constants, and a workbook Excel saved with that content reads back.
//!
//! What a session file holds:
//! - one sheet, named as the workbook names it;
//! - at most one formula cell: an ordinary formula, a dynamic-array formula (what Excel 365 writes
//!   when a formula can give several values) or a legacy array formula over its one cell
//!   (`{=...}`, entered with Ctrl+Shift+Enter). The text is the file's, with the prefixes Excel
//!   writes (`_xlfn.`); [`write_session`] adds them to a formula given as entered;
//! - constants: numbers, text, logicals and error values, one per cell;
//! - optionally one custom XML item of the application's own namespace for what the format has
//!   no place for (the application's options, its add-in list), kept opaque.
//!
//! A workbook outside that is refused with a typed [`SessionRefusal`] before anything is read
//! into a session: more sheets, formulas or array areas than the content allows, defined names,
//! tables, charts, pivot tables, validations, conditional formats, merged cells, or a VBA project.
//! Nothing is dropped silently except what carries no calculation: styles, the document's
//! properties and a cached result (a reopen calculates the formula again, so a file's cached result
//! is never read as a value).

use crate::{
    CustomXmlItem, LoadedWorkbook, MemorySource, OpenPolicy, PackageRead, SavePolicy,
    load, open, prepare_edit, prepare_save, write,
};
use calc_doc_model::value::{CellKey, ExcelText, GridArea, IdAllocator, SheetId, WorksheetErrorCode};
use calc_doc_model::{
    ArrayFormula, ArrayKind, CachedValue, CellInput, CellText, FiniteNumber, FormulaForm,
    FormulaText, OpBatch, SheetName, WorkbookOp,
};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

/// How the session's formula is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionArray {
    /// An ordinary formula, written with no array marker: Excel 365 stores one that cannot give
    /// several values this way, and any other formula stored this way is a legacy formula (implicit
    /// intersection, [`plain_formula_needs_legacy`]).
    Plain,
    /// A dynamic-array formula.
    Dynamic,
    /// A legacy array formula over its one cell: `{=...}`.
    Legacy,
}

/// The session's one formula and where it is entered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionFormula {
    pub row: u32,
    pub column: u32,
    /// The text after the `=`. Read from a file, it is the file's spelling; given to
    /// [`write_session`], it is the entry spelling and gets the prefixes Excel writes.
    pub text: String,
    pub array: SessionArray,
}

/// Whether the plain formula `text` (the text after the `=`, as a file stores it) gives a different
/// result entered in the dynamic form than in the legacy form of an older Excel (implicit
/// intersection). Excel 365 stores a formula that cannot give several values as a plain formula
/// (`=SUM(A1:A3)`), where the two forms agree; one that can is stored as a dynamic-array formula.
/// A plain formula of the second kind in a file is a legacy formula, which the dynamic form would
/// read differently.
pub fn plain_formula_needs_legacy(text: &str) -> bool {
    let Ok(entered) = FormulaText::with_form(
        ExcelText::from_utf16_code_units(text.encode_utf16().collect()),
        FormulaForm::Dynamic,
    ) else {
        return false;
    };
    crate::file_form(&entered, &[]).is_ok_and(|form| form.dynamic)
}

/// A constant a cell holds.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionValue {
    Number(f64),
    Text(String),
    Logical(bool),
    Error(WorksheetErrorCode),
}

/// One constant cell.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionConstant {
    pub row: u32,
    pub column: u32,
    pub value: SessionValue,
}

/// The content of a session file.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionFile {
    /// The one sheet's name.
    pub sheet: String,
    pub formula: Option<SessionFormula>,
    /// The constants, in row-major order.
    pub constants: Vec<SessionConstant>,
    /// What the application keeps in its own custom XML item, when the file holds one (on a read)
    /// or when a save should write one.
    pub custom: Option<CustomXmlItem>,
    /// Whether the workbook counts serial dates from 1904 (`workbookPr date1904`) rather than 1900.
    pub date_1904: bool,
    /// The calculated result to store as the formula's cached result, which no reader takes as a
    /// value; only a number, text, logical or error. [`read_session`] leaves it `None`.
    pub cached: Option<SessionValue>,
}

/// Why a workbook is not a session file, or why a session could not be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionRefusal {
    /// The bytes are not a package this codec opens, or its workbook does not load.
    Unreadable { detail: String },
    /// The workbook has more or fewer than one sheet.
    Sheets { count: usize },
    /// The workbook has more than one formula cell.
    Formulas { count: usize },
    /// The formula is an array formula over more than its one cell.
    ArrayArea { rows: u32, columns: u32 },
    /// A defined name, table, chart, pivot table, validation, conditional format or merged area:
    /// content this file does not hold.
    Content { what: &'static str },
    /// The workbook carries a VBA project.
    VbaProject,
    /// A cell holds something a constant cannot be here, or text too long.
    Cell { row: u32, column: u32, detail: String },
    /// The session could not be written.
    Write { detail: String },
}

impl fmt::Display for SessionRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { detail } => write!(f, "the file cannot be read as a workbook: {detail}"),
            Self::Sheets { count: 0 } => f.write_str("the workbook has no sheet"),
            Self::Sheets { count } => write!(f, "the workbook has {count} sheets; a session has one"),
            Self::Formulas { count } => {
                write!(f, "the sheet has {count} formulas; a session has one")
            }
            Self::ArrayArea { rows, columns } => write!(
                f,
                "the formula is an array formula over {rows} x {columns} cells; a session's formula fills one cell"
            ),
            Self::Content { what } => write!(f, "the workbook has {what}, which a session does not hold"),
            Self::VbaProject => f.write_str("the workbook carries a VBA project"),
            Self::Cell { row, column, detail } => {
                write!(f, "the cell at row {row}, column {column}: {detail}")
            }
            Self::Write { detail } => write!(f, "the session could not be written: {detail}"),
        }
    }
}

impl std::error::Error for SessionRefusal {}

fn unreadable(detail: impl fmt::Display) -> SessionRefusal {
    SessionRefusal::Unreadable {
        detail: detail.to_string(),
    }
}

fn open_bytes(bytes: Vec<u8>) -> Result<LoadedWorkbook, SessionRefusal> {
    let source: Arc<dyn PackageRead> = Arc::new(MemorySource::new(bytes));
    let document = open(source, &OpenPolicy::default()).map_err(unreadable)?;
    let mut ids: IdAllocator<SheetId> = IdAllocator::new();
    load(&document, &mut ids).map_err(unreadable)
}

fn text_of(text: &ExcelText) -> String {
    text.to_string_lossy()
}

/// Reads the session a workbook holds, refusing a workbook outside the constraints.
/// `namespace` names the application's custom XML item, which is returned when the file holds one.
pub fn read_session(bytes: Vec<u8>, namespace: &str) -> Result<SessionFile, SessionRefusal> {
    let loaded = open_bytes(bytes)?;
    if loaded.has_vba_project() {
        return Err(SessionRefusal::VbaProject);
    }
    let model = loaded.model();
    let sheets = model.sheets();
    let [entry] = sheets else {
        return Err(SessionRefusal::Sheets { count: sheets.len() });
    };
    let sheet = entry.id();
    let definitions = model.definitions();
    if !definitions.names().is_empty() || loaded.defined_names_not_loaded() > 0 {
        return Err(SessionRefusal::Content { what: "defined names" });
    }
    if !definitions.tables().is_empty() {
        return Err(SessionRefusal::Content { what: "tables" });
    }
    if definitions.pivots().tables().next().is_some() {
        return Err(SessionRefusal::Content { what: "pivot tables" });
    }
    if let Some(loaded_sheet) = loaded.sheet(sheet) {
        if !loaded_sheet.merged_areas().is_empty() {
            return Err(SessionRefusal::Content { what: "merged cells" });
        }
        if !loaded_sheet.validations().is_empty() {
            return Err(SessionRefusal::Content { what: "data validation" });
        }
        if !loaded_sheet.conditional_formats().is_empty() {
            return Err(SessionRefusal::Content { what: "conditional formats" });
        }
        if !loaded_sheet.charts().charts.is_empty() || loaded_sheet.charts().unread > 0 {
            return Err(SessionRefusal::Content { what: "charts" });
        }
    }
    let mut formula: Option<SessionFormula> = None;
    let mut formulas = 0usize;
    let mut filled: Vec<GridArea> = Vec::new();
    let mut constants = Vec::new();
    for (at, input) in model.sheet_cells(sheet).into_iter().flatten() {
        let (row, column) = (at.row(), at.column());
        let cell_refusal = |detail: &str| SessionRefusal::Cell {
            row,
            column,
            detail: detail.to_owned(),
        };
        match input {
            CellInput::Formula(text) => {
                formulas += 1;
                let array = match definitions.array(at) {
                    None => SessionArray::Plain,
                    Some(array) => {
                        let area = array.area();
                        let (rows, columns) = (
                            area.last().row() - area.first().row() + 1,
                            area.last().column() - area.first().column() + 1,
                        );
                        match array.kind() {
                            ArrayKind::Legacy if (rows, columns) != (1, 1) => {
                                return Err(SessionRefusal::ArrayArea { rows, columns });
                            }
                            ArrayKind::Legacy => SessionArray::Legacy,
                            ArrayKind::Dynamic { .. } => {
                                filled.push(area);
                                SessionArray::Dynamic
                            }
                        }
                    }
                };
                formula = Some(SessionFormula {
                    row,
                    column,
                    text: text_of(&text.rendered()),
                    array,
                });
            }
            CellInput::Number(number) => constants.push(SessionConstant {
                row,
                column,
                value: SessionValue::Number(number.get()),
            }),
            CellInput::Text(text) => constants.push(SessionConstant {
                row,
                column,
                value: SessionValue::Text(text_of(text.text())),
            }),
            CellInput::Logical(value) => constants.push(SessionConstant {
                row,
                column,
                value: SessionValue::Logical(*value),
            }),
            CellInput::Error(code) => constants.push(SessionConstant {
                row,
                column,
                value: SessionValue::Error(*code),
            }),
            #[allow(unreachable_patterns)]
            _ => return Err(cell_refusal("not a constant or a formula")),
        }
    }
    if formulas > 1 {
        return Err(SessionRefusal::Formulas { count: formulas });
    }
    // A dynamic array's spilled cells hold the result it last had, not constants.
    constants.retain(|constant| {
        !filled.iter().any(|area| {
            CellKey::new(area.sheet(), constant.row, constant.column)
                .is_ok_and(|cell| area.contains(cell))
        })
    });
    let custom = match loaded.custom_xml(namespace).map_err(unreadable)?.into_iter().next() {
        Some((_, content)) => Some(CustomXmlItem {
            namespace: namespace.to_owned(),
            content,
        }),
        None => None,
    };
    Ok(SessionFile {
        sheet: text_of(entry.name().text()),
        formula,
        constants,
        custom,
        date_1904: loaded.date_system() == crate::DateSystem::Date1904,
        cached: None,
    })
}

fn cell_input(value: &SessionValue, row: u32, column: u32) -> Result<CellInput, SessionRefusal> {
    let refused = |detail: &str| SessionRefusal::Cell {
        row,
        column,
        detail: detail.to_owned(),
    };
    Ok(match value {
        SessionValue::Number(number) => CellInput::Number(
            FiniteNumber::new(*number).ok_or_else(|| refused("a number a cell cannot hold"))?,
        ),
        SessionValue::Text(text) => CellInput::Text(
            CellText::new(ExcelText::from_utf16_code_units(text.encode_utf16().collect()))
                .ok_or_else(|| refused("text longer than a cell holds"))?,
        ),
        SessionValue::Logical(value) => CellInput::Logical(*value),
        SessionValue::Error(code) => CellInput::Error(*code),
    })
}

/// The bytes of the workbook `session` is kept in.
pub fn write_session(session: &SessionFile) -> Result<Vec<u8>, SessionRefusal> {
    let write_refused = |detail: String| SessionRefusal::Write { detail };
    // The sheet name is checked as the model checks it before it goes into the new package.
    SheetName::new(ExcelText::from_utf16_code_units(session.sheet.encode_utf16().collect()))
        .map_err(|error| write_refused(format!("the sheet name: {error:?}")))?;
    let loaded = open_bytes(crate::blank::blank_package_named_in(&session.sheet, session.date_1904))?;
    let model = loaded.model();
    let sheet = model.sheets()[0].id();
    let cell = |row: u32, column: u32| {
        CellKey::new(sheet, row, column).map_err(|_| SessionRefusal::Cell {
            row,
            column,
            detail: "outside the sheet".to_owned(),
        })
    };
    let mut operations = Vec::new();
    for constant in &session.constants {
        operations.push(WorkbookOp::SetCell {
            at: cell(constant.row, constant.column)?,
            input: cell_input(&constant.value, constant.row, constant.column)?,
        });
    }
    let mut cached: BTreeMap<CellKey, CachedValue> = BTreeMap::new();
    if let Some(formula) = &session.formula {
        let at = cell(formula.row, formula.column)?;
        let entered = FormulaText::with_form(
            ExcelText::from_utf16_code_units(formula.text.encode_utf16().collect()),
            FormulaForm::Dynamic,
        )
        .map_err(|error| write_refused(error.to_string()))?;
        let form = crate::file_form(&entered, model.definitions().names())
            .map_err(|error| write_refused(format!("the formula has no file form: {error:?}")))?;
        let text = ExcelText::from_utf16_code_units(form.text);
        let kind = match formula.array {
            SessionArray::Legacy => Some(ArrayKind::Legacy),
            // A plain formula stays plain: a legacy formula, even when it could give several values.
            SessionArray::Plain => None,
            SessionArray::Dynamic if form.dynamic => Some(ArrayKind::Dynamic { collapsed: false }),
            SessionArray::Dynamic => None,
        };
        let input = CellInput::Formula(
            FormulaText::with_form(
                text,
                if kind.is_some_and(|kind| kind != ArrayKind::Legacy) {
                    FormulaForm::Dynamic
                } else {
                    FormulaForm::Legacy
                },
            )
            .map_err(|error| write_refused(error.to_string()))?,
        );
        operations.push(match kind {
            Some(kind) => WorkbookOp::SetArray {
                array: ArrayFormula::new(GridArea::cell(at), kind),
                input,
            },
            None => WorkbookOp::SetCell { at, input },
        });
        if let Some(value) = &session.cached {
            let value = match cell_input(value, formula.row, formula.column)? {
                CellInput::Number(number) => CachedValue::Number(number),
                CellInput::Text(text) => CachedValue::Text(text),
                CellInput::Logical(value) => CachedValue::Logical(value),
                CellInput::Error(code) => CachedValue::Error(code),
                CellInput::Formula(_) => unreachable!("a cached value is never a formula"),
            };
            cached.insert(at, value);
        }
    }
    let edited = prepare_edit(
        model,
        &OpBatch {
            base: model.revision(),
            operations,
        },
    )
    .map_err(|error| write_refused(format!("{error:?}")))?
    .into_model();
    let mut snapshot = if cached.is_empty() {
        loaded.snapshot(&edited)
    } else {
        loaded.snapshot_with_values(&edited, cached)
    };
    if let Some(item) = &session.custom {
        snapshot = snapshot.with_custom_xml(vec![item.clone()]);
    }
    let plan = prepare_save(&snapshot, &SavePolicy::default())
        .map_err(|refusal| write_refused(refusal.to_string()))?;
    let mut out = Vec::new();
    write(plan, &mut out).map_err(|error| write_refused(error.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMESPACE: &str = "urn:example:session-options";

    fn custom(text: &str) -> Option<CustomXmlItem> {
        Some(CustomXmlItem {
            namespace: NAMESPACE.to_owned(),
            content: format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<options xmlns=\"{NAMESPACE}\">{text}</options>"
            )
            .into_bytes(),
        })
    }

    fn session() -> SessionFile {
        SessionFile {
            sheet: "Sheet1".to_owned(),
            formula: Some(SessionFormula {
                row: 1,
                column: 8,
                text: "SUM(A1:A3)+LEN(B1)".to_owned(),
                array: SessionArray::Plain,
            }),
            constants: vec![
                SessionConstant { row: 1, column: 1, value: SessionValue::Number(1.5) },
                SessionConstant { row: 1, column: 2, value: SessionValue::Text("é & < >".to_owned()) },
                SessionConstant { row: 2, column: 1, value: SessionValue::Number(2.0) },
                SessionConstant { row: 2, column: 2, value: SessionValue::Logical(true) },
                SessionConstant { row: 3, column: 2, value: SessionValue::Error(WorksheetErrorCode::NA) },
            ],
            custom: custom("<locale>de-DE</locale>"),
            date_1904: false,
            cached: None,
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_session_survives_its_own_file() {
        let original = session();
        let bytes = write_session(&original).expect("written");
        let read = read_session(bytes, NAMESPACE).expect("read back");
        assert_eq!(read, original);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_formula_form_and_entry_spelling_reach_the_file() {
        let mut original = session();
        original.formula = Some(SessionFormula {
            row: 4,
            column: 2,
            text: "SEQUENCE(2,2)".to_owned(),
            array: SessionArray::Dynamic,
        });
        let bytes = write_session(&original).expect("written");
        let read = read_session(bytes, NAMESPACE).expect("read back");
        let formula = read.formula.expect("a formula");
        assert_eq!(formula.text, "_xlfn.SEQUENCE(2,2)");
        assert_eq!(formula.array, SessionArray::Dynamic);
        assert_eq!((formula.row, formula.column), (4, 2));
        // The spilled area is not constants, and the other constants are all kept.
        assert_eq!(read.constants.len(), 5);

        original.formula = Some(SessionFormula {
            row: 1,
            column: 8,
            text: "A1:A2*2".to_owned(),
            array: SessionArray::Legacy,
        });
        let read = read_session(write_session(&original).unwrap(), NAMESPACE).unwrap();
        let formula = read.formula.unwrap();
        assert_eq!((formula.text.as_str(), formula.array), ("A1:A2*2", SessionArray::Legacy));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_cached_result_is_written_and_never_read_as_a_value() {
        let mut original = session();
        original.cached = Some(SessionValue::Number(5.0));
        let bytes = write_session(&original).expect("written");
        let document = crate::open(Arc::new(MemorySource::new(bytes.clone())), &OpenPolicy::default())
            .expect("opens");
        let sheet = String::from_utf8(
            document.package().read_part("xl/worksheets/sheet1.xml").expect("the sheet part"),
        )
        .expect("UTF-8");
        assert!(sheet.contains("<f>SUM(A1:A3)+LEN(B1)</f><v>5</v>"), "{sheet}");
        let read = read_session(bytes, NAMESPACE).expect("read back");
        assert_eq!(read.cached, None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_custom_item_is_replaced_not_duplicated_on_a_second_save() {
        let first = write_session(&session()).unwrap();
        let loaded = open_bytes(first).unwrap();
        assert_eq!(loaded.custom_xml(NAMESPACE).unwrap().len(), 1);
        let mut again = session();
        again.custom = custom("<locale>fr-FR</locale>");
        let read = read_session(write_session(&again).unwrap(), NAMESPACE).unwrap();
        assert!(String::from_utf8(read.custom.unwrap().content).unwrap().contains("fr-FR"));
        // A file without the item reads without one.
        let mut bare = session();
        bare.custom = None;
        assert!(read_session(write_session(&bare).unwrap(), NAMESPACE).unwrap().custom.is_none());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_workbook_outside_the_constraints_is_refused_with_its_reason() {
        // Not a package at all.
        assert!(matches!(
            read_session(b"not a zip".to_vec(), NAMESPACE),
            Err(SessionRefusal::Unreadable { .. })
        ));
        // An empty session is a valid file with one sheet and no cells.
        let empty = SessionFile {
            sheet: "Data".to_owned(),
            formula: None,
            constants: vec![],
            custom: None,
            date_1904: false,
            cached: None,
        };
        assert_eq!(read_session(write_session(&empty).unwrap(), NAMESPACE).unwrap(), empty);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_1904_date_system_is_the_workbooks_own_setting_and_survives_the_file() {
        let mut original = session();
        original.date_1904 = true;
        let bytes = write_session(&original).expect("written");
        let read = read_session(bytes, NAMESPACE).expect("read back");
        assert!(read.date_1904);
        assert_eq!(read, original);
        original.date_1904 = false;
        assert!(!read_session(write_session(&original).unwrap(), NAMESPACE).unwrap().date_1904);
    }
}
