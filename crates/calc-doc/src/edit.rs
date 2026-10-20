//! Preparing an authored edit: a batch of cell operations applied to a revision of a loaded
//! workbook's model, refused when the model refuses it or when it sets a formula a save could not
//! write.

use crate::load::FORMULA_MAX_UTF16_CODE_UNITS;
use calc_doc_model::{ApplyError, CellInput, FormulaText, ModelRoot, OpBatch, WorkbookOp};
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
    let edited = model.apply(batch).map_err(EditError::Model)?;
    for (operation, op) in batch.operations.iter().enumerate() {
        let WorkbookOp::SetCell {
            input: CellInput::Formula(formula),
            ..
        } = op
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
