//! Preparing an authored edit: a batch of cell operations applied to a revision of a loaded
//! workbook's model, refused when the model refuses it or when it sets a formula a save could not
//! write.

use crate::load::FORMULA_MAX_UTF16_CODE_UNITS;
use calc_doc_model::value::{CellKey, GridArea};
use calc_doc_model::{
    ApplyError, ArrayKind, CellInput, FormulaText, ModelRoot, OpBatch, WorkbookOp,
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
    /// The model refused the batch: it was prepared against another revision, names a sheet the
    /// model does not hold, or holds a row, column or sheet edit, which this package does not
    /// author yet.
    Model(ApplyError),
    /// The formula the operation at this index sets is longer than
    /// [`FORMULA_MAX_UTF16_CODE_UNITS`], so [`crate::load`] would refuse the saved package.
    FormulaTooLong { operation: usize, units: usize },
    /// CalcFml's parser reads the formula the operation at this index sets with a diagnostic:
    /// malformed text, or syntax that parser does not read, such as trim references (`A1.:B10`),
    /// which Microsoft documents as Excel formula syntax (not measured here). A save writes a
    /// formula's text as it was authored, so only text the parser reads without a diagnostic is
    /// admitted; how Excel treats text the parser does not read is left to infra-m1.3's round
    /// trip.
    FormulaSyntax { operation: usize, message: String },
    /// The operation at this index changes part of the legacy array formula anchored at
    /// `anchor`: it sets or clears some but not all of the array's cells, which Excel refuses. A
    /// value set into a dynamic array's spill area is admitted: it blocks the spill, and the
    /// engine shows #SPILL! at the anchor. Setting a dynamic array's anchor replaces the array,
    /// and clearing any of its cells is admitted.
    ArrayPart { operation: usize, anchor: CellKey },
    /// The operation at this index sets an error constant Excel cannot hold in a cell: one of the
    /// newer errors (#SPILL!, #CALC!, #FIELD!, #BLOCKED!, #CONNECT!, #BUSY!, #GETTING_DATA).
    /// Excel stores the newer errors only as formula results (with value metadata); typed as a
    /// constant it keeps #SPILL! and the others as text and #GETTING_DATA as #N/A (measured,
    /// tests/fixtures/d1/excel-errors.json), so a save could not write the cell faithfully.
    ErrorConstant {
        operation: usize,
        code: calc_doc_model::value::WorksheetErrorCode,
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
