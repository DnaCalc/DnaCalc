//! Authored cell input: what a person or a file put into a cell, never what a calculation
//! produced. No variant holds a calculated or cached value; a formula's cached result belongs to
//! the provenance-aware value views, not to its input.

use crate::{FormulaBlock, FormulaForm, FormulaSaveMode};
use calc_value::CellKey;
use calc_value::{EXCEL_TEXT_MAX_UTF16_CODE_UNITS, ExcelText, WorksheetErrorCode};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

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
/// as an xlsx `<f>` element holds it, kept inline or as a member of a shared source. It holds no
/// cached value and no parse result.
///
/// Only one rule is checked here: the text is not empty. Whether a text is valid Excel formula
/// syntax, a text that itself begins with `=` included, is for CalcFml's parser to decide. No
/// length limit is applied here either; Excel's formula-length limit belongs to the parser or the
/// loader.
#[derive(Clone, Debug)]
pub struct FormulaText {
    storage: FormulaStorage,
    form: FormulaForm,
    explicit_form: bool,
}

#[derive(Clone, Debug)]
enum FormulaStorage {
    Inline(ExcelText),
    Member {
        block: Arc<FormulaBlock>,
        at: CellKey,
        text: Arc<OnceLock<ExcelText>>,
    },
}

impl FormulaText {
    pub fn new(text: ExcelText) -> Result<Self, FormulaTextError> {
        let mut formula = Self::with_form(text, FormulaForm::Legacy)?;
        formula.explicit_form = false;
        Ok(formula)
    }

    pub fn with_form(text: ExcelText, form: FormulaForm) -> Result<Self, FormulaTextError> {
        if text.utf16_code_units().is_empty() {
            return Err(FormulaTextError::Empty);
        }
        Ok(Self {
            storage: FormulaStorage::Inline(text),
            form,
            explicit_form: true,
        })
    }

    pub fn member(block: Arc<FormulaBlock>, at: CellKey) -> Option<Self> {
        block.contains(at).then(|| Self {
            form: block.form(),
            explicit_form: true,
            storage: FormulaStorage::Member {
                block,
                at,
                text: Arc::new(OnceLock::new()),
            },
        })
    }
    /// Whether an ingress selected a dialect. `new` retains the legacy API's context-free
    /// spelling; the host can infer its entry form. `with_form` and loaded sources are explicit.
    pub fn is_form_explicit(&self) -> bool {
        self.explicit_form
    }
    pub fn form(&self) -> FormulaForm {
        self.form
    }
    pub fn block(&self) -> Option<&Arc<FormulaBlock>> {
        match &self.storage {
            FormulaStorage::Member { block, .. } => Some(block),
            _ => None,
        }
    }
    pub fn member_at(&self) -> Option<CellKey> {
        match self.storage {
            FormulaStorage::Member { at, .. } => Some(at),
            _ => None,
        }
    }
    pub fn save_mode(&self) -> FormulaSaveMode {
        self.block()
            .map_or(FormulaSaveMode::Individual, |block| block.save_mode())
    }
    /// Whether per-member text has been requested. Engine binding reads the shared source.
    pub fn materialized(&self) -> bool {
        match &self.storage {
            FormulaStorage::Inline(_) => true,
            FormulaStorage::Member { text, .. } => text.get().is_some(),
        }
    }
    pub fn text(&self) -> &ExcelText {
        match &self.storage {
            FormulaStorage::Inline(text) => text,
            FormulaStorage::Member { block, at, text } => {
                text.get_or_init(|| block.source().render(*at))
            }
        }
    }
    /// Render a spelling without retaining a per-member expansion. Storage comparisons
    /// and writers use this; `text` retains an expansion when an editor requests it.
    pub fn rendered(&self) -> ExcelText {
        match &self.storage {
            FormulaStorage::Inline(text) => text.clone(),
            FormulaStorage::Member { block, at, text } => text
                .get()
                .cloned()
                .unwrap_or_else(|| block.source().render(*at)),
        }
    }
    pub fn detached(&self) -> Self {
        Self {
            storage: FormulaStorage::Inline(self.text().clone()),
            form: self.form,
            explicit_form: self.explicit_form,
        }
    }
    /// Page reuse must keep representation changes, even when authored text is equal.
    pub(crate) fn same_storage(&self, other: &Self) -> bool {
        if self.form != other.form || self.explicit_form != other.explicit_form {
            return false;
        }
        match (&self.storage, &other.storage) {
            (FormulaStorage::Inline(a), FormulaStorage::Inline(b)) => a == b,
            (
                FormulaStorage::Member {
                    block: a, at: aa, ..
                },
                FormulaStorage::Member {
                    block: b, at: ba, ..
                },
            ) => Arc::ptr_eq(a, b) && aa == ba,
            _ => false,
        }
    }
}

impl PartialEq for FormulaText {
    fn eq(&self, other: &Self) -> bool {
        self.form == other.form && (self.same_storage(other) || self.rendered() == other.rendered())
    }
}
impl Eq for FormulaText {}
impl Hash for FormulaText {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.form.hash(state);
        self.rendered().hash(state);
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
