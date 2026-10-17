//! Authored cell input: what a person or a file put into a cell, never what a calculation
//! produced. No variant holds a calculated or cached value; a formula's cached result belongs to
//! the provenance-aware value views, not to its input.

use calc_value::{EXCEL_TEXT_MAX_UTF16_CODE_UNITS, ExcelText, WorksheetErrorCode};
use std::fmt;
use std::hash::{Hash, Hasher};

/// A finite binary64 number. Equality and hashing compare the bits, so `-0.0` and `0.0` are
/// different inputs, and a subnormal keeps its bits.
#[derive(Clone, Copy, Debug)]
pub struct FiniteNumber(f64);

impl FiniteNumber {
    /// `value`, or `None` when it is NaN or an infinity.
    pub const fn new(value: f64) -> Option<Self> {
        if value.is_finite() {
            Some(FiniteNumber(value))
        } else {
            None
        }
    }

    /// The number with these bits, or `None` when they encode NaN or an infinity.
    pub const fn from_bits(bits: u64) -> Option<Self> {
        Self::new(f64::from_bits(bits))
    }

    pub const fn get(self) -> f64 {
        self.0
    }

    pub const fn to_bits(self) -> u64 {
        self.0.to_bits()
    }
}

impl PartialEq for FiniteNumber {
    fn eq(&self, other: &Self) -> bool {
        self.to_bits() == other.to_bits()
    }
}

impl Eq for FiniteNumber {}

impl Hash for FiniteNumber {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_bits().hash(state);
    }
}

/// Text authored into a cell: at most [`EXCEL_TEXT_MAX_UTF16_CODE_UNITS`] UTF-16 code units,
/// kept exactly, an unpaired surrogate included. Empty text is admitted.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CellText(ExcelText);

impl CellText {
    /// `text`, or `None` when it is longer than [`EXCEL_TEXT_MAX_UTF16_CODE_UNITS`] code units.
    pub fn new(text: ExcelText) -> Option<Self> {
        (text.len_utf16_code_units() <= EXCEL_TEXT_MAX_UTF16_CODE_UNITS).then_some(CellText(text))
    }

    pub fn text(&self) -> &ExcelText {
        &self.0
    }
}

/// Why [`FormulaText::new`] refused a text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormulaTextError {
    Empty,
}

impl fmt::Display for FormulaTextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormulaTextError::Empty => f.write_str("the formula text is empty"),
        }
    }
}

impl std::error::Error for FormulaTextError {}

/// The source text of a formula as authored: what follows the `=` that marks a formula entry,
/// as an xlsx `<f>` element holds it, kept as UTF-16 code units. It is text and nothing else: no
/// cached value and no parse result.
///
/// Only one rule is checked here: the text is not empty. Whether a text is valid Excel formula
/// syntax, a text that itself begins with `=` included, is for CalcFml's parser to decide. No
/// length limit is applied here either; Excel's formula-length limit belongs to the parser or the
/// loader.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FormulaText(ExcelText);

impl FormulaText {
    /// `text`, or why it is not a formula's text: it is empty.
    pub fn new(text: ExcelText) -> Result<Self, FormulaTextError> {
        let units = text.utf16_code_units();
        if units.is_empty() {
            return Err(FormulaTextError::Empty);
        }
        Ok(FormulaText(text))
    }

    pub fn text(&self) -> &ExcelText {
        &self.0
    }
}

/// What a cell holds as authored input.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CellInput {
    Number(FiniteNumber),
    Text(CellText),
    Logical(bool),
    Error(WorksheetErrorCode),
    Formula(FormulaText),
}
