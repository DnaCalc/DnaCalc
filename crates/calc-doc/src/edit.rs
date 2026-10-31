//! Preparing an authored edit: a batch of cell operations applied to a revision of a loaded
//! workbook's model, refused when the model refuses it or when it sets a formula a save could not
//! write.

use crate::load::FORMULA_MAX_UTF16_CODE_UNITS;
use calc_doc_model::value::{CellKey, GridArea, MAX_COLUMNS, MAX_ROWS, SheetId};
use calc_doc_model::{
    ApplyError, ArrayKind, Axis, AxisChange, AxisEdit, CellInput, FormulaRewriter, FormulaText,
    ModelRoot, OpBatch, WorkbookOp,
};
use std::fmt;
use std::sync::Arc;

/// The revision an edit makes, not yet adopted by anyone: the revision it was prepared against is
/// unchanged, and the caller decides whether this one replaces it.
#[derive(Debug)]
pub struct PreparedEdit {
    model: Arc<ModelRoot>,
}

impl PreparedEdit {
    /// The revision the edit makes.
    pub fn model(&self) -> &Arc<ModelRoot> {
        &self.model
    }

    pub fn into_model(self) -> Arc<ModelRoot> {
        self.model
    }
}

/// Why [`prepare_edit`] refused a batch. Nothing was made, and the revision it was prepared
/// against is unchanged.
#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
    SheetOperation(String),
    /// The model refused the batch: it was prepared against another revision, names a sheet the
    /// model does not hold, or holds a row, column or sheet edit, which this package does not
    /// author yet.
    Model(ApplyError),
    /// The formula the operation at this index sets is longer than
    /// [`FORMULA_MAX_UTF16_CODE_UNITS`], so [`crate::load`] would refuse the saved package.
    FormulaTooLong {
        operation: usize,
        units: usize,
    },
    /// CalcFml's parser reads the formula the operation at this index sets with a diagnostic:
    /// malformed text, or syntax that parser does not read, such as trim references (`A1.:B10`),
    /// which Microsoft documents as Excel formula syntax (not measured here). A save writes a
    /// formula's text as it was authored, so only text the parser reads without a diagnostic is
    /// admitted; how Excel treats text the parser does not read is not measured.
    FormulaSyntax {
        operation: usize,
        message: String,
    },
    /// The operation at this index changes part of the legacy array formula anchored at
    /// `anchor`: it sets or clears some but not all of the array's cells, which Excel refuses. A
    /// value set into a dynamic array's spill area is admitted: it blocks the spill, and the
    /// engine shows #SPILL! at the anchor. Setting a dynamic array's anchor replaces the array,
    /// and clearing any of its cells is admitted.
    ArrayPart {
        operation: usize,
        anchor: CellKey,
    },
    /// The operation at this index sets an error constant Excel cannot hold in a cell: one of the
    /// newer errors (#SPILL!, #CALC!, #FIELD!, #BLOCKED!, #CONNECT!, #BUSY!, #GETTING_DATA).
    /// Excel stores the newer errors only as formula results (with value metadata); typed as a
    /// constant it keeps #SPILL! and the others as text and #GETTING_DATA as #N/A (measured,
    /// tests/fixtures/d1/excel-errors.json), so a save could not write the cell faithfully.
    ErrorConstant {
        operation: usize,
        code: calc_doc_model::value::WorksheetErrorCode,
    },
    /// The operation at this index of a structural batch ([`prepare_structural_edit`]) is not
    /// an insertion or deletion of rows or columns.
    NotAxisEdit {
        operation: usize,
    },
    /// The axis edit at this index changes the header or totals row of the table `table`,
    /// inserts or deletes columns inside it, or deletes all of it.
    TablePart {
        operation: usize,
        table: String,
    },
    /// The insertion at this index would push cells that hold an input off the grid.
    PushedOffGrid {
        operation: usize,
    },
}

/// Whether Excel holds `code` as a constant in a cell: the seven classic errors.
pub(crate) fn error_constant_writable(code: calc_doc_model::value::WorksheetErrorCode) -> bool {
    use calc_doc_model::value::WorksheetErrorCode::*;
    matches!(code, Null | Div0 | Value | Ref | Name | Num | NA)
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SheetOperation(reason) => f.write_str(reason),
            Self::Model(error) => write!(f, "the model refused the batch: {error}"),
            Self::FormulaTooLong { operation, units } => write!(
                f,
                "operation {operation} sets a formula of {units} UTF-16 code units, more than {FORMULA_MAX_UTF16_CODE_UNITS}"
            ),
            Self::FormulaSyntax { operation, message } => write!(
                f,
                "operation {operation} sets a formula the parser reads with a diagnostic: {message}"
            ),
            Self::ErrorConstant { operation, code } => write!(
                f,
                "operation {operation} sets the error {code:?} as a constant, which Excel does not hold in a cell"
            ),
            Self::ArrayPart { operation, anchor } => write!(
                f,
                "operation {operation} changes part of the array formula anchored at row {} column {}",
                anchor.row(),
                anchor.column()
            ),
            Self::NotAxisEdit { operation } => write!(
                f,
                "operation {operation} of a structural batch is not a row or column edit"
            ),
            Self::TablePart { operation, table } => write!(
                f,
                "operation {operation} changes the header, totals or columns of the table {table}"
            ),
            Self::PushedOffGrid { operation } => write!(
                f,
                "operation {operation} would push cells that hold an input off the grid"
            ),
        }
    }
}

impl std::error::Error for EditError {}

/// Why a formula's text is not one a save writes: what [`prepare_edit`] and
/// [`crate::prepare_save`] each refuse.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FormulaCheck {
    /// Longer than [`FORMULA_MAX_UTF16_CODE_UNITS`]: this many UTF-16 code units.
    TooLong(usize),
    /// CalcFml's parser reads it with this diagnostic.
    Syntax(String),
}

/// Checks a formula's text as the text of an `<f>` element, without the `=` that marks a formula
/// entry, so a text that itself starts with `=` is refused: at most
/// [`FORMULA_MAX_UTF16_CODE_UNITS`] UTF-16 code units, and read by CalcFml's parser without a
/// diagnostic.
pub(crate) fn check_formula(formula: &FormulaText) -> Result<(), FormulaCheck> {
    let units = formula.text().utf16_code_units();
    if units.len() > FORMULA_MAX_UTF16_CODE_UNITS {
        return Err(FormulaCheck::TooLong(units.len()));
    }
    // An unpaired surrogate can only stand inside a quoted text, where the replacement character
    // it reads as here is as acceptable to the parser.
    let source = format!("={}", String::from_utf16_lossy(units));
    match calc_fml::parse_formula(&source).diagnostics().first() {
        Some(diagnostic) => Err(FormulaCheck::Syntax(diagnostic.message.clone())),
        None => Ok(()),
    }
}

/// Applies `batch` to `model` and checks every formula it sets ([`check_formula`]): the batch's
/// next revision, or why it is refused.
pub fn prepare_edit(model: &ModelRoot, batch: &OpBatch) -> Result<PreparedEdit, EditError> {
    check_arrays(model, batch)?;
    let edited = model.apply(batch).map_err(EditError::Model)?;
    for (operation, op) in batch.operations.iter().enumerate() {
        if let WorkbookOp::SetCell {
            input: CellInput::Error(code),
            ..
        } = op
            && !error_constant_writable(*code)
        {
            return Err(EditError::ErrorConstant {
                operation,
                code: *code,
            });
        }
        let (WorkbookOp::SetCell {
            input: CellInput::Formula(formula),
            ..
        }
        | WorkbookOp::SetArray {
            input: CellInput::Formula(formula),
            ..
        }) = op
        else {
            continue;
        };
        check_formula(formula).map_err(|check| match check {
            FormulaCheck::TooLong(units) => EditError::FormulaTooLong { operation, units },
            FormulaCheck::Syntax(message) => EditError::FormulaSyntax { operation, message },
        })?;
    }
    Ok(PreparedEdit { model: edited })
}

/// Refuses an operation that changes part of an array formula ([`EditError::ArrayPart`]), judged
/// against the arrays of `model`, the revision the batch applies to.
fn check_arrays(model: &ModelRoot, batch: &OpBatch) -> Result<(), EditError> {
    let definitions = model.definitions();
    for (operation, op) in batch.operations.iter().enumerate() {
        let area = match op {
            WorkbookOp::SetCell { at, .. } => GridArea::cell(*at),
            WorkbookOp::Clear { area } => *area,
            WorkbookOp::SetArray { array, .. } => array.area(),
            _ => continue,
        };
        for array in definitions.arrays_meeting(area) {
            let whole = array.area();
            let covers = area.contains(whole.first()) && area.contains(whole.last());
            let refused = match array.kind() {
                ArrayKind::Legacy => whole.cell_count() > 1 && !covers,
                ArrayKind::Dynamic { .. } => false,
            };
            if refused {
                return Err(EditError::ArrayPart {
                    operation,
                    anchor: array.anchor(),
                });
            }
        }
    }
    Ok(())
}

/// No sheet is issued by an axis edit.
fn no_sheet() -> SheetId {
    unreachable!("an axis edit issues no sheet")
}

/// [`prepare_edit`] for a batch of row and column insertions and deletions
/// ([`WorkbookOp::EditAxis`]): the next revision as [`ModelRoot::apply_structural`] makes it,
/// moving the cells, array formulas, tables, hidden rows and unloaded cells, and with `rewriter`
/// rewriting every formula's and defined name's references (calc-engine's
/// `ReferenceRewriter`). Refused, with nothing made, as Excel refuses the same edit:
///
/// - an operation that is not an axis edit ([`EditError::NotAxisEdit`]): sheet edits and cell
///   edits are not admitted in a structural batch here;
/// - an edit that changes part of a legacy array formula, inserting inside it or deleting some but
///   not all of its rows or columns ([`EditError::ArrayPart`]);
/// - an edit that changes a table's header or totals row, inserts or deletes columns inside a
///   table, or deletes the whole table ([`EditError::TablePart`]): the table part records its
///   columns and those rows, which this edit would contradict;
/// - an insertion that would push cells holding an input off the grid
///   ([`EditError::PushedOffGrid`]).
///
/// Each edit is checked against the revision the batch's earlier edits leave.
pub fn prepare_structural_edit(
    model: &ModelRoot,
    batch: &OpBatch,
    rewriter: &dyn FormulaRewriter,
) -> Result<PreparedEdit, EditError> {
    let mut current: Option<Arc<ModelRoot>> = None;
    for (operation, op) in batch.operations.iter().enumerate() {
        let WorkbookOp::EditAxis(edit) = op else {
            return Err(EditError::NotAxisEdit { operation });
        };
        let next = {
            let root: &ModelRoot = current.as_deref().unwrap_or(model);
            check_axis_edit(root, operation, *edit)?;
            let single = OpBatch {
                base: root.revision(),
                operations: vec![op.clone()],
            };
            root.apply_structural(&single, rewriter, &mut no_sheet)
                .map_err(EditError::Model)?
        };
        current = Some(next);
    }
    let edited = model
        .apply_structural(batch, rewriter, &mut no_sheet)
        .map_err(EditError::Model)?;
    Ok(PreparedEdit { model: edited })
}

/// The first and last positions of an area on an axis.
fn span(area: GridArea, axis: Axis) -> (u32, u32) {
    match axis {
        Axis::Rows => (area.first().row(), area.last().row()),
        Axis::Columns => (area.first().column(), area.last().column()),
    }
}

/// Refuses an axis edit Excel refuses on `root` (see [`prepare_structural_edit`]).
fn check_axis_edit(root: &ModelRoot, operation: usize, edit: AxisEdit) -> Result<(), EditError> {
    let (first, last) = (edit.first(), edit.last());
    let definitions = root.definitions();
    let on_sheet = |area: GridArea| area.sheet() == edit.sheet();
    // Whether the edit changes part of the span [top, bottom]: an insertion strictly inside it,
    // or a deletion that meets it without covering it.
    let splits = |(top, bottom): (u32, u32)| match edit.change() {
        AxisChange::Insert => top < first && first <= bottom,
        AxisChange::Delete => first <= bottom && top <= last && !(first <= top && bottom <= last),
    };
    for array in definitions.arrays() {
        if on_sheet(array.area())
            && array.kind() == ArrayKind::Legacy
            && splits(span(array.area(), edit.axis()))
        {
            return Err(EditError::ArrayPart {
                operation,
                anchor: array.anchor(),
            });
        }
    }
    for table in definitions.tables() {
        if !on_sheet(table.area) {
            continue;
        }
        let (top, bottom) = span(table.area, Axis::Rows);
        let (left, right) = span(table.area, Axis::Columns);
        let refused = match (edit.axis(), edit.change()) {
            (Axis::Columns, AxisChange::Insert) => left < first && first <= right,
            (Axis::Columns, AxisChange::Delete) => first <= right && left <= last,
            (Axis::Rows, AxisChange::Insert) => {
                // Inside the data rows the table grows; a row inserted at its header or between
                // its data and its totals row would change those rows.
                let header = top + table.header_rows;
                let totals = bottom + 1 - table.totals_rows;
                top < first
                    && (first < header
                        || (table.totals_rows > 0 && first > totals - 1 && first <= bottom))
            }
            (Axis::Rows, AxisChange::Delete) => {
                let header_last = top + table.header_rows - 1;
                let totals_first = bottom + 1 - table.totals_rows;
                let meets = |a: u32, b: u32| a <= b && first <= b && a <= last;
                (table.header_rows > 0 && meets(top, header_last))
                    || (table.totals_rows > 0 && meets(totals_first, bottom))
                    || (first <= top && bottom <= last)
            }
        };
        if refused {
            return Err(EditError::TablePart {
                operation,
                table: table.display_name.clone(),
            });
        }
    }
    if edit.change() == AxisChange::Insert {
        let size = edit.axis().size();
        let band_first = size - edit.count() + 1;
        let band = match edit.axis() {
            Axis::Rows => GridArea::new(
                CellKey::new(edit.sheet(), band_first, 1).expect("in the grid"),
                CellKey::new(edit.sheet(), size, MAX_COLUMNS).expect("in the grid"),
            ),
            Axis::Columns => GridArea::new(
                CellKey::new(edit.sheet(), 1, band_first).expect("in the grid"),
                CellKey::new(edit.sheet(), MAX_ROWS, size).expect("in the grid"),
            ),
        }
        .expect("one sheet");
        if root.cells(band).into_iter().flatten().next().is_some() {
            return Err(EditError::PushedOffGrid { operation });
        }
    }
    Ok(())
}

/// Applies worksheet lifecycle operations using the canonical model reference rewriter.
/// Sheet identities are supplied by the host and retained across Undo and Redo.
pub fn prepare_sheet_edit(
    model: &ModelRoot,
    batch: &OpBatch,
    rewriter: &dyn FormulaRewriter,
    issue: &mut dyn FnMut() -> SheetId,
) -> Result<PreparedEdit, EditError> {
    for (operation, op) in batch.operations.iter().enumerate() {
        if !matches!(op, WorkbookOp::EditSheet(_)) {
            return Err(EditError::NotAxisEdit { operation });
        }
    }
    let edited = model
        .apply_structural(batch, rewriter, issue)
        .map_err(EditError::Model)?;
    if !edited
        .sheets()
        .iter()
        .any(|s| s.visibility() == calc_doc_model::SheetVisibility::Visible)
    {
        return Err(EditError::SheetOperation(
            "Keep at least one visible worksheet".to_owned(),
        ));
    }
    let mut keys = std::collections::BTreeSet::new();
    let mut names = std::collections::BTreeSet::new();
    for entry in edited.sheets() {
        if !keys.insert(entry.key())
            || !names.insert(calc_doc_model::value::fold_workbook_name(
                &entry.name().text().to_string_lossy(),
            ))
        {
            return Err(EditError::SheetOperation(
                "A worksheet with that name or key already exists".to_owned(),
            ));
        }
    }
    for sheet in edited.sheets() {
        for (at, input) in edited.sheet_cells(sheet.id()).into_iter().flatten() {
            if model.cell(at) == Some(input) {
                continue;
            }
            if let CellInput::Formula(formula) = input {
                check_formula(formula).map_err(|check| match check {
                    FormulaCheck::TooLong(units) => EditError::FormulaTooLong {
                        operation: 0,
                        units,
                    },
                    FormulaCheck::Syntax(message) => EditError::FormulaSyntax {
                        operation: 0,
                        message,
                    },
                })?;
            }
        }
    }
    Ok(PreparedEdit { model: edited })
}
