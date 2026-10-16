//! Authored cell input: what a person or a file put into a cell, never what a calculation
//! produced. No variant holds a calculated or cached value; a formula's cached result belongs to
//! the provenance-aware value views, not to its input.

use calc_value::{EXCEL_TEXT_MAX_UTF16_CODE_UNITS, ExcelText, WorksheetErrorCode};
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

/// The source text of a formula as authored, without the leading `=` that marks a formula entry
/// (the text an xlsx `<f>` element holds), kept as UTF-16 code units. It is text and nothing
/// else: no cached value and no parse result. Parsing it is CalcFml's work.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FormulaText(ExcelText);

impl FormulaText {
    /// `text`, or `None` when it is empty.
    pub fn new(text: ExcelText) -> Option<Self> {
        (text.len_utf16_code_units() > 0).then_some(FormulaText(text))
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
