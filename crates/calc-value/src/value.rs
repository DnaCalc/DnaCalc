//! The worksheet value model.
//!
//! A [`CalcValue`] is a [`CoreValue`] plus, for a callable, its [`CallableValue`]. Every shared
//! payload (text code units, array cells, the areas of a multi-area reference, a callable and its
//! handle) is immutable behind `std::sync::Arc`, so cloning a value shares its payload and every
//! type here is `Send + Sync`.

use crate::grid::{CellKey, GridArea};
use crate::id::SheetId;
use std::any::Any;
use std::fmt;
use std::sync::Arc;

/// The longest text Excel holds in a cell, in UTF-16 code units. Text assigned through
/// [`ExcelText::from_interop_assignment`] is truncated to it.
pub const EXCEL_TEXT_MAX_UTF16_CODE_UNITS: usize = 32_767;

/// The worksheet error values this model represents. Whether an Excel build shows others is not
/// established here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorksheetErrorCode {
    Null,
    Div0,
    Value,
    Ref,
    Name,
    Num,
    NA,
    Busy,
    GettingData,
    Spill,
    Calc,
    Field,
    Blocked,
    Connect,
}

impl WorksheetErrorCode {
    /// The invariant spelling used in authored worksheet entries and formula text.
    pub const fn excel_spelling(self) -> &'static str {
        match self {
            Self::Null => "#NULL!",
            Self::Div0 => "#DIV/0!",
            Self::Value => "#VALUE!",
            Self::Ref => "#REF!",
            Self::Name => "#NAME?",
            Self::Num => "#NUM!",
            Self::NA => "#N/A",
            Self::Busy => "#BUSY!",
            Self::GettingData => "#GETTING_DATA",
            Self::Spill => "#SPILL!",
            Self::Calc => "#CALC!",
            Self::Field => "#FIELD!",
            Self::Blocked => "#BLOCKED!",
            Self::Connect => "#CONNECT!",
        }
    }

    /// Every worksheet error code, in declaration order.
    pub const fn all() -> [WorksheetErrorCode; 14] {
        [
            Self::Null,
            Self::Div0,
            Self::Value,
            Self::Ref,
            Self::Name,
            Self::Num,
            Self::NA,
            Self::Busy,
            Self::GettingData,
            Self::Spill,
            Self::Calc,
            Self::Field,
            Self::Blocked,
            Self::Connect,
        ]
    }
}

/// Excel text: UTF-16 code units, which need not form valid UTF-16 (a truncation can leave a
/// dangling high surrogate, as Excel's can).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExcelText {
    units: Arc<[u16]>,
}

impl ExcelText {
    /// Text of exactly these code units, without truncation.
    pub fn from_utf16_code_units(units: Vec<u16>) -> Self {
        ExcelText {
            units: units.into(),
        }
    }

    /// Text as Excel stores a string assigned through its interop surface: the UTF-16 encoding of
    /// `input`, truncated to [`EXCEL_TEXT_MAX_UTF16_CODE_UNITS`] code units.
    pub fn from_interop_assignment(input: &str) -> Self {
        let units: Vec<u16> = input
            .encode_utf16()
            .take(EXCEL_TEXT_MAX_UTF16_CODE_UNITS)
            .collect();
        Self::from_utf16_code_units(units)
    }

    pub fn len_utf16_code_units(&self) -> usize {
        self.units.len()
    }

    pub fn utf16_code_units(&self) -> &[u16] {
        &self.units
    }

    /// The text as a Rust string, with each unpaired surrogate replaced by U+FFFD.
    pub fn to_string_lossy(&self) -> String {
        String::from_utf16_lossy(&self.units)
    }

    /// Whether the last code unit is a high surrogate with no low surrogate after it.
    pub fn has_dangling_high_surrogate_tail(&self) -> bool {
        self.units
            .last()
            .is_some_and(|unit| (0xD800..=0xDBFF).contains(unit))
    }
}

/// The rows and columns of an array value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArrayShape {
    pub rows: usize,
    pub cols: usize,
}

impl ArrayShape {
    /// `rows * cols`, or `None` when the product overflows `usize`.
    pub const fn cell_count(self) -> Option<usize> {
        self.rows.checked_mul(self.cols)
    }
}

/// A non-empty rectangular array of values in row-major order. Its cells are shared, immutable,
/// behind an `Arc`. A cell may itself hold an array: GROUPBY and PIVOTBY keep a LAMBDA's array
/// result in its cell (dc-h9u).
///
/// A boxed array ([`CalcArray::boxed`]) is an array taken out of such a cell (INDEX of the cell):
/// Excel 16.0.20527 (2026-10-30) shows its first element and spills nothing, takes its first
/// element where one value is taken (`=INDEX(g,1,2)*10` is 10, TYPE gives the first element's
/// type) and the whole array where an array is taken (ROWS, SUM, VSTACK).
#[derive(Clone, Debug, PartialEq)]
pub struct CalcArray {
    shape: ArrayShape,
    cells: Arc<[CalcValue]>,
    boxed: bool,
}

impl CalcArray {
    /// The array of `shape` with `cells` in row-major order; `None` when a dimension is 0 or the
    /// cell count does not match the shape.
    pub fn new(shape: ArrayShape, cells: Vec<CalcValue>) -> Option<Self> {
        if shape.rows == 0 || shape.cols == 0 || shape.cell_count() != Some(cells.len()) {
            return None;
        }
        Some(CalcArray {
            shape,
            cells: cells.into(),
            boxed: false,
        })
    }

    /// The 1-by-1 array holding `value`.
    pub fn from_scalar(value: CalcValue) -> Self {
        CalcArray {
            shape: ArrayShape { rows: 1, cols: 1 },
            cells: Arc::from([value]),
            boxed: false,
        }
    }

    /// The array of these rows; `None` when there are no rows, a row is empty, or the rows differ
    /// in length.
    pub fn from_rows(rows: Vec<Vec<CalcValue>>) -> Option<Self> {
        let row_count = rows.len();
        let col_count = rows.first()?.len();
        if col_count == 0 || rows.iter().any(|row| row.len() != col_count) {
            return None;
        }
        let cells: Vec<CalcValue> = rows.into_iter().flatten().collect();
        Self::new(
            ArrayShape {
                rows: row_count,
                cols: col_count,
            },
            cells,
        )
    }

    pub const fn shape(&self) -> ArrayShape {
        self.shape
    }

    /// This array as taken out of an array's cell (see the type's documentation).
    pub fn boxed(mut self) -> Self {
        self.boxed = true;
        self
    }

    /// Whether this array was taken out of an array's cell (see the type's documentation).
    pub const fn is_boxed(&self) -> bool {
        self.boxed
    }

    /// Whether a cell of this array holds an array (dc-h9u).
    pub fn holds_arrays(&self) -> bool {
        self.cells
            .iter()
            .any(|cell| matches!(cell.core_ref(), CoreRef::Array(_)) && cell.callable_value().is_none())
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// The cell at 0-based `row` and `col`, or `None` outside the shape.
    pub fn get(&self, row: usize, col: usize) -> Option<&CalcValue> {
        if row >= self.shape.rows || col >= self.shape.cols {
            return None;
        }
        let index = row.checked_mul(self.shape.cols)?.checked_add(col)?;
        self.cells.get(index)
    }

    /// The cells of 0-based `row`, or `None` outside the shape.
    pub fn row_slice(&self, row: usize) -> Option<&[CalcValue]> {
        if row >= self.shape.rows {
            return None;
        }
        let start = row.checked_mul(self.shape.cols)?;
        let end = start.checked_add(self.shape.cols)?;
        self.cells.get(start..end)
    }

    pub fn iter_row_major(&self) -> std::slice::Iter<'_, CalcValue> {
        self.cells.iter()
    }

    /// Whether both arrays have one shape and exposure ([`Self::is_boxed`]) and
    /// representation-equal cells ([`CalcValue::representation_eq`]).
    pub fn representation_eq(&self, other: &Self) -> bool {
        self.shape == other.shape
            && self.boxed == other.boxed
            && self
                .cells
                .iter()
                .zip(other.cells.iter())
                .all(|(left, right)| left.representation_eq(right))
    }
}

/// Why [`ReferenceLike::from_areas`] refused its areas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReferenceError {
    /// A reference holds at least one area.
    NoAreas,
    /// The areas lie on more than one sheet; Excel's union operator works within one sheet.
    /// `first` is the sheet of the first area and `other` that of the first area on another.
    SheetMismatch { first: SheetId, other: SheetId },
}

impl fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReferenceError::NoAreas => f.write_str("a reference holds at least one area"),
            ReferenceError::SheetMismatch { first, other } => write!(
                f,
                "a multi-area reference holds areas of one sheet only, not of {first:?} and {other:?}"
            ),
        }
    }
}

impl std::error::Error for ReferenceError {}

/// A reference value: one or more grid areas of one sheet, in order, or a 3-D reference's areas,
/// one per sheet of its span ([`ReferenceLike::three_d`]). A single cell is a one-cell area. The
/// areas of a multi-area (union) reference are shared behind an `Arc`; one area is held inline
/// and allocates nothing. A structured reference is bound to its areas before it becomes a value;
/// a spill anchor and an external workbook reference are not representable here.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReferenceLike {
    areas: Areas,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Areas {
    One(GridArea),
    /// Two or more areas.
    Many(Arc<[GridArea]>),
}

impl ReferenceLike {
    /// The reference to one cell.
    pub const fn cell(key: CellKey) -> Self {
        Self::area(GridArea::cell(key))
    }

    /// The reference to one area.
    pub const fn area(area: GridArea) -> Self {
        ReferenceLike {
            areas: Areas::One(area),
        }
    }

    /// The reference to these areas, in this order, duplicates kept (as Excel's union keeps
    /// them). One area gives the same value as [`Self::area`]. Refused with the typed error when
    /// there are no areas or when they lie on more than one sheet.
    pub fn from_areas(areas: Vec<GridArea>) -> Result<Self, ReferenceError> {
        let first = areas.first().ok_or(ReferenceError::NoAreas)?.sheet();
        if let Some(other) = areas.iter().map(|area| area.sheet()).find(|s| *s != first) {
            return Err(ReferenceError::SheetMismatch { first, other });
        }
        match areas.as_slice() {
            [one] => Ok(Self::area(*one)),
            _ => Ok(ReferenceLike {
                areas: Areas::Many(areas.into()),
            }),
        }
    }

    /// The reference a 3-D reference (`Sheet1:Sheet3!A1:B2`) names: the same area on each sheet of
    /// the span, in the workbook's sheet order (L3, dc-m3u). Unlike [`Self::from_areas`] the areas
    /// may lie on several sheets; Excel's union, intersection and range operators still refuse
    /// such a reference, and only the functions that take a 3-D reference are given one.
    /// Refused with [`ReferenceError::NoAreas`] when there are no areas.
    pub fn three_d(areas: Vec<GridArea>) -> Result<Self, ReferenceError> {
        match areas.as_slice() {
            [] => Err(ReferenceError::NoAreas),
            [one] => Ok(Self::area(*one)),
            _ => Ok(ReferenceLike {
                areas: Areas::Many(areas.into()),
            }),
        }
    }

    /// The sheet of the reference's first area: the sheet every area lies on, except for a 3-D
    /// reference ([`Self::three_d`]), whose first sheet it is.
    pub fn sheet(&self) -> SheetId {
        self.areas()[0].sheet()
    }

    /// Whether the areas lie on more than one sheet: a 3-D reference ([`Self::three_d`]).
    pub fn spans_sheets(&self) -> bool {
        let first = self.sheet();
        self.areas().iter().any(|area| area.sheet() != first)
    }

    pub fn areas(&self) -> &[GridArea] {
        match &self.areas {
            Areas::One(area) => std::slice::from_ref(area),
            Areas::Many(areas) => areas,
        }
    }

    pub fn area_count(&self) -> usize {
        self.areas().len()
    }
}

/// The number of arguments a callable accepts, from `min` to `max` inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CallableArityShape {
    pub min: usize,
    pub max: usize,
}

impl CallableArityShape {
    pub const fn exact(n: usize) -> Self {
        CallableArityShape { min: n, max: n }
    }

    pub const fn range(min: usize, max: usize) -> Self {
        CallableArityShape { min, max }
    }

    pub const fn accepts(self, argc: usize) -> bool {
        argc >= self.min && argc <= self.max
    }
}

/// The contract of the identity behind a [`CallableValue`]. The value model never inspects it;
/// the evaluator that owns the callable downcasts it through [`OpaqueCallable::as_any`].
/// Implementers are `Send + Sync`: a callable value carries a token or an immutable binding,
/// never thread-bound evaluation state.
pub trait OpaqueCallable: fmt::Debug + Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
}

/// A callable value (a LAMBDA or a function value): its arity, a summary for display, and the
/// shared handle of its identity. Two callable values are equal when their arities are equal and
/// they share one handle.
#[derive(Clone, Debug)]
pub struct CallableValue {
    arity: CallableArityShape,
    summary: Arc<str>,
    handle: Arc<dyn OpaqueCallable>,
}

impl CallableValue {
    pub fn new(arity: CallableArityShape, summary: &str, handle: Arc<dyn OpaqueCallable>) -> Self {
        CallableValue {
            arity,
            summary: summary.into(),
            handle,
        }
    }

    pub const fn arity(&self) -> CallableArityShape {
        self.arity
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub fn handle(&self) -> &Arc<dyn OpaqueCallable> {
        &self.handle
    }
}

impl PartialEq for CallableValue {
    fn eq(&self, other: &Self) -> bool {
        self.arity == other.arity && Arc::ptr_eq(&self.handle, &other.handle)
    }
}

/// The worksheet-visible part of a value.
#[derive(Clone, Debug, PartialEq)]
pub enum CoreValue {
    Number(f64),
    Text(ExcelText),
    Logical(bool),
    Error(WorksheetErrorCode),
    /// An empty cell.
    Empty,
    /// An omitted argument.
    Missing,
    Array(CalcArray),
    Reference(ReferenceLike),
}

/// The picture formats a cell's image holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Tiff,
    Ico,
    Webp,
}

impl ImageFormat {
    /// The format `bytes` start with, or `None` when they start no picture.
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        let starts = |prefix: &[u8]| bytes.starts_with(prefix);
        if starts(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if starts(&[0xff, 0xd8, 0xff]) {
            Some(Self::Jpeg)
        } else if starts(b"GIF87a") || starts(b"GIF89a") {
            Some(Self::Gif)
        } else if starts(b"BM") {
            Some(Self::Bmp)
        } else if starts(b"II*\0") || starts(b"MM\0*") {
            Some(Self::Tiff)
        } else if starts(&[0, 0, 1, 0]) {
            Some(Self::Ico)
        } else if starts(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            Some(Self::Webp)
        } else {
            None
        }
    }

    /// The file extension of a media part holding this format.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Gif => "gif",
            Self::Bmp => "bmp",
            Self::Tiff => "tiff",
            Self::Ico => "ico",
            Self::Webp => "webp",
        }
    }

    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Bmp => "image/bmp",
            Self::Tiff => "image/tiff",
            Self::Ico => "image/x-icon",
            Self::Webp => "image/webp",
        }
    }
}

/// The members or set a CUBE function's value holds, by unique name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CubeValue {
    pub connection: String,
    /// One tuple for a member; a set's tuples in its order.
    pub tuples: Vec<Vec<String>>,
    pub set: bool,
}

/// A picture in a cell, with what IMAGE was given for it.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageValue {
    /// The URL the formula named.
    pub source: String,
    pub alt_text: Option<String>,
    /// 0 fit, 1 fill, 2 original size, 3 custom size.
    pub sizing: u8,
    pub height: Option<f64>,
    pub width: Option<f64>,
    pub format: ImageFormat,
    pub bytes: Arc<[u8]>,
}

impl Eq for ImageValue {}

impl std::hash::Hash for ImageValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.alt_text.hash(state);
        self.sizing.hash(state);
        self.height.map(f64::to_bits).hash(state);
        self.width.map(f64::to_bits).hash(state);
        self.format.hash(state);
        self.bytes.hash(state);
    }
}

impl ImageValue {
    /// Picture bytes alone, as a fetch delivers them.
    pub fn picture(format: ImageFormat, bytes: Arc<[u8]>) -> Self {
        Self {
            source: String::new(),
            alt_text: None,
            sizing: 0,
            height: None,
            width: None,
            format,
            bytes,
        }
    }
}

/// A view of a value's worksheet-visible part, returned by value from [`CalcValue::core_ref`]
/// (dc-dxm, compact value type step 1): a caller that matches on it does not depend on how a
/// value is stored, so the storage can change (a smaller layout, step 2) without touching it.
/// Number, logical, error, empty and missing are copied out; text, array and reference are
/// borrowed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CoreRef<'a> {
    Number(f64),
    Text(&'a ExcelText),
    Logical(bool),
    Error(WorksheetErrorCode),
    /// An empty cell.
    Empty,
    /// An omitted argument.
    Missing,
    Array(&'a CalcArray),
    Reference(&'a ReferenceLike),
}

/// A value: its worksheet-visible [`CoreValue`], and for a callable the [`CallableValue`] behind
/// it (whose worksheet-visible core is `#CALC!`).
#[derive(Clone)]
pub struct CalcValue {
    core: CoreValue,
    metadata: Option<Arc<ValueMetadata>>,
}

// Reuse the optional metadata pointer rather than growing every operand/frame.
// Deep scalar IF evaluations must still fit their existing one-megabyte stack.
enum ValueMetadata {
    Callable(CallableValue),
    ExcelOverflow,
    Image(ImageValue),
    Cube(CubeValue),
}

impl fmt::Debug for CalcValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = f.debug_struct("CalcValue");
        out.field("core", &self.core).field("callable", &self.callable_value());
        if let Some(image) = self.image_value() {
            out.field("image", image);
        }
        if let Some(cube) = self.cube_value() {
            out.field("cube", cube);
        }
        if self.is_excel_overflow() {
            out.field("excel_overflow", &true);
        }
        out.finish()
    }
}

// Four relations between values are kept apart (dc-hsa5.19, Pro B1):
//
// 1. Worksheet comparison: Excel's `=`, `<` and the lookup rules, with coercion and the text
//    collation. It lives in calc-func (`comparison`), never here, and a callable or cube value
//    takes part in it through its worksheet-visible core.
// 2. Runtime identity: two callables are the same callable when they share one handle
//    ([`CalcValue::same_callable`], [`CallableValue`]'s `==`); deep equality of what a closure
//    captured is never asked.
// 3. Representation equality ([`CalcValue::representation_eq`]): the same worksheet-visible core
//    (numbers by bit pattern, so `-0` and `0` differ and a NaN equals itself), the same
//    qualification (callable identity, image, cube members, Excel's overflow number) and, for
//    arrays, the same shape, exposure ([`CalcArray::is_boxed`]) and cells. A caller that decides
//    whether a returned value is the one it already holds (a retransmitted RTD or asynchronous
//    result) asks this.
// 4. Reuse equivalence: whether a result may stand in for another for one observable facet
//    (`ObservableFacets`, calc-fml). No code compares values to decide reuse or an early
//    cutoff: the engine's certificates compare versions and epochs, never values, so there is
//    no value-equality consumer of this kind. `PartialEq` is not that relation.
//
// `PartialEq for CalcValue` is structural equality over every qualification the value carries,
// with plain numbers compared as IEEE numbers (`0.0 == -0.0`, `NaN != NaN`): what a test or a
// derived `PartialEq` of a type that holds values needs. A qualification it left out would make
// two different values equal, so it covers callable identity, image and cube members alike.
impl PartialEq for CalcValue {
    fn eq(&self, other: &Self) -> bool {
        if self.is_excel_overflow() || other.is_excel_overflow() {
            self.is_excel_overflow() == other.is_excel_overflow()
                && self.as_number().map(f64::to_bits) == other.as_number().map(f64::to_bits)
        } else {
            self.core == other.core
                && self.callable_value() == other.callable_value()
                && self.image_value() == other.image_value()
                && self.cube_value() == other.cube_value()
        }
    }
}

impl CalcValue {
    pub const fn new(core: CoreValue) -> Self {
        CalcValue {
            core,
            metadata: None,
        }
    }

    pub fn number(value: f64) -> Self {
        Self::new(CoreValue::Number(value))
    }

    /// The measured rounding conversion past DBL_MAX (dc-j9k.19). The exponent carries
    /// into field 2047 while the significand remains intact. Only that conversion may
    /// originate this qualification; copying the value preserves it. Ordinary number
    /// construction, including an identical NaN from another kernel, never grants it.
    pub fn excel_overflow_number(value: f64) -> Option<Self> {
        value.is_nan().then(|| Self {
            core: CoreValue::Number(value),
            metadata: Some(Arc::new(ValueMetadata::ExcelOverflow)),
        })
    }

    pub fn is_excel_overflow(&self) -> bool {
        matches!(self.metadata.as_deref(), Some(ValueMetadata::ExcelOverflow))
    }

    pub fn text(value: ExcelText) -> Self {
        Self::new(CoreValue::Text(value))
    }

    pub fn logical(value: bool) -> Self {
        Self::new(CoreValue::Logical(value))
    }

    pub fn error(code: WorksheetErrorCode) -> Self {
        Self::new(CoreValue::Error(code))
    }

    pub fn empty() -> Self {
        Self::new(CoreValue::Empty)
    }

    pub fn missing() -> Self {
        Self::new(CoreValue::Missing)
    }

    pub fn array(array: CalcArray) -> Self {
        Self::new(CoreValue::Array(array))
    }

    pub fn reference(reference: ReferenceLike) -> Self {
        Self::new(CoreValue::Reference(reference))
    }

    /// A callable value. Its worksheet-visible core is `#CALC!`, which is what Excel shows for a
    /// callable that reaches a cell.
    pub fn callable(callable: CallableValue) -> Self {
        CalcValue {
            core: CoreValue::Error(WorksheetErrorCode::Calc),
            metadata: Some(Arc::new(ValueMetadata::Callable(callable))),
        }
    }

    pub fn core(&self) -> &CoreValue {
        &self.core
    }

    /// The worksheet-visible part as a [`CoreRef`] view: what callers should match on, rather
    /// than on [`CalcValue::core`]'s stored enum.
    pub fn core_ref(&self) -> CoreRef<'_> {
        match &self.core {
            CoreValue::Number(number) => CoreRef::Number(*number),
            CoreValue::Text(text) => CoreRef::Text(text),
            CoreValue::Logical(logical) => CoreRef::Logical(*logical),
            CoreValue::Error(code) => CoreRef::Error(*code),
            CoreValue::Empty => CoreRef::Empty,
            CoreValue::Missing => CoreRef::Missing,
            CoreValue::Array(array) => CoreRef::Array(array),
            CoreValue::Reference(reference) => CoreRef::Reference(reference),
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self.core {
            CoreValue::Number(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&ExcelText> {
        match &self.core {
            CoreValue::Text(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_logical(&self) -> Option<bool> {
        match self.core {
            CoreValue::Logical(value) => Some(value),
            _ => None,
        }
    }

    /// The error code of the core; a callable reports `Calc`.
    pub fn as_error(&self) -> Option<WorksheetErrorCode> {
        match self.core {
            CoreValue::Error(code) => Some(code),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&CalcArray> {
        match &self.core {
            CoreValue::Array(array) => Some(array),
            _ => None,
        }
    }

    pub fn as_reference(&self) -> Option<&ReferenceLike> {
        match &self.core {
            CoreValue::Reference(reference) => Some(reference),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self.core, CoreValue::Empty)
    }

    pub fn is_missing(&self) -> bool {
        matches!(self.core, CoreValue::Missing)
    }

    pub fn callable_value(&self) -> Option<&CallableValue> {
        match self.metadata.as_deref() {
            Some(ValueMetadata::Callable(callable)) => Some(callable),
            Some(
                ValueMetadata::ExcelOverflow | ValueMetadata::Image(_) | ValueMetadata::Cube(_),
            )
            | None => None,
        }
    }

    /// A picture in a cell, as IMAGE gives it. Excel reads it as #VALUE! wherever one worksheet
    /// value is taken (`Range.Value2`, a file's `<v>`), so that is its core; TYPE gives 128 and
    /// ISERROR FALSE, and the value keeps its picture when it is copied.
    pub fn image(image: ImageValue) -> Self {
        Self {
            core: CoreValue::Error(WorksheetErrorCode::Value),
            metadata: Some(Arc::new(ValueMetadata::Image(image))),
        }
    }

    /// What a CUBE function gives: text to show, holding its members or set for the functions
    /// that read the cell. Everything else reads the text.
    pub fn cube(text: ExcelText, value: CubeValue) -> Self {
        Self {
            core: CoreValue::Text(text),
            metadata: Some(Arc::new(ValueMetadata::Cube(value))),
        }
    }

    pub fn cube_value(&self) -> Option<&CubeValue> {
        match self.metadata.as_deref() {
            Some(ValueMetadata::Cube(value)) => Some(value),
            _ => None,
        }
    }

    pub fn image_value(&self) -> Option<&ImageValue> {
        match self.metadata.as_deref() {
            Some(ValueMetadata::Image(image)) => Some(image),
            _ => None,
        }
    }

    /// Whether both values are callables sharing one handle: runtime identity, the second of the
    /// relations described above [`CalcValue`]'s `PartialEq`. A value that is not a callable is
    /// never the same callable as anything.
    pub fn same_callable(&self, other: &Self) -> bool {
        matches!(
            (self.callable_value(), other.callable_value()),
            (Some(left), Some(right)) if left == right
        )
    }

    /// Representation equality, the third of the relations described above [`CalcValue`]'s
    /// `PartialEq`: the same worksheet-visible core with numbers equal by bit pattern, the same
    /// callable identity, image, cube members and overflow qualification, and arrays of the same
    /// shape and exposure whose cells are representation-equal in turn.
    pub fn representation_eq(&self, other: &Self) -> bool {
        let same_qualification = match (self.metadata.as_deref(), other.metadata.as_deref()) {
            (None, None) | (Some(ValueMetadata::ExcelOverflow), Some(ValueMetadata::ExcelOverflow)) => true,
            (Some(ValueMetadata::Callable(_)), Some(ValueMetadata::Callable(_))) => self.same_callable(other),
            (Some(ValueMetadata::Image(left)), Some(ValueMetadata::Image(right))) => left == right,
            (Some(ValueMetadata::Cube(left)), Some(ValueMetadata::Cube(right))) => left == right,
            _ => false,
        };
        same_qualification
            && match (self.core_ref(), other.core_ref()) {
                (CoreRef::Number(left), CoreRef::Number(right)) => left.to_bits() == right.to_bits(),
                (CoreRef::Array(left), CoreRef::Array(right)) => left.representation_eq(right),
                (left, right) => left == right,
            }
    }
}

/// The value model's unit tests. Each test runs natively and, as a wasm-bindgen test, in Chrome
/// and Firefox.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::CellKey;
    use crate::id::{IdAllocator, SheetId};

    #[derive(Debug)]
    struct TestCallable;

    impl OpaqueCallable for TestCallable {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// dc-dxm: [`CalcValue::core_ref`] gives each variant of the stored core, copying the
    /// small ones and borrowing the rest.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_dxm__core_ref_views_each_kind_of_value() {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = sheets.issue().expect("a sheet");
        let cell = CellKey::new(sheet, 2, 3).expect("a cell");
        let text = ExcelText::from_interop_assignment("abc");
        let array = CalcArray::new(
            ArrayShape { rows: 1, cols: 2 },
            vec![CalcValue::number(1.0), CalcValue::number(2.0)],
        )
        .expect("an array");
        let reference = ReferenceLike::cell(cell);
        let values = [
            CalcValue::number(1.5),
            CalcValue::text(text.clone()),
            CalcValue::logical(true),
            CalcValue::error(WorksheetErrorCode::Div0),
            CalcValue::empty(),
            CalcValue::missing(),
            CalcValue::array(array.clone()),
            CalcValue::reference(reference.clone()),
        ];
        let views = [
            CoreRef::Number(1.5),
            CoreRef::Text(&text),
            CoreRef::Logical(true),
            CoreRef::Error(WorksheetErrorCode::Div0),
            CoreRef::Empty,
            CoreRef::Missing,
            CoreRef::Array(&array),
            CoreRef::Reference(&reference),
        ];
        for (value, view) in values.iter().zip(views) {
            assert_eq!(value.core_ref(), view, "{value:?}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn interop_assignment_truncates_ascii_to_32767_utf16_units() {
        let text = ExcelText::from_interop_assignment(&"x".repeat(40_000));
        assert_eq!(text.len_utf16_code_units(), EXCEL_TEXT_MAX_UTF16_CODE_UNITS);
        assert!(!text.has_dangling_high_surrogate_tail());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn interop_assignment_can_leave_dangling_surrogate_tail() {
        let text = ExcelText::from_interop_assignment(&"\u{1F600}".repeat(40_000));
        assert_eq!(text.len_utf16_code_units(), EXCEL_TEXT_MAX_UTF16_CODE_UNITS);
        assert!(text.has_dangling_high_surrogate_tail());
        assert!(text.to_string_lossy().ends_with('\u{FFFD}'));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn calc_array_preserves_shape_and_row_major_access() {
        let array = CalcArray::from_rows(vec![
            vec![
                CalcValue::number(1.0),
                CalcValue::text(ExcelText::from_utf16_code_units(
                    "x".encode_utf16().collect(),
                )),
            ],
            vec![CalcValue::logical(true), CalcValue::empty()],
        ])
        .unwrap();

        assert_eq!(array.shape(), ArrayShape { rows: 2, cols: 2 });
        assert_eq!(
            array.get(0, 0),
            Some(&CalcValue::new(CoreValue::Number(1.0)))
        );
        assert_eq!(array.get(1, 1), Some(&CalcValue::empty()));
        assert_eq!(array.get(2, 0), None);
        assert_eq!(array.row_slice(1).map(<[CalcValue]>::len), Some(2));
        assert_eq!(array.iter_row_major().count(), 4);
        assert_eq!(
            array
                .iter_row_major()
                .nth(2)
                .and_then(CalcValue::as_logical),
            Some(true)
        );
        assert!(CalcArray::from_rows(vec![vec![CalcValue::empty()], vec![]]).is_none());
        assert!(
            CalcArray::new(ArrayShape { rows: 2, cols: 1 }, vec![CalcValue::empty()]).is_none()
        );
        assert_eq!(
            ArrayShape {
                rows: usize::MAX,
                cols: 2
            }
            .cell_count(),
            None
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn calc_array_admits_nested_arrays_and_missing_elements() {
        let nested = CalcValue::array(CalcArray::from_scalar(CalcValue::number(1.0)));
        assert_eq!(CalcArray::from_scalar(nested).cell_count(), 1);
        assert!(
            CalcArray::from_scalar(CalcValue::missing())
                .get(0, 0)
                .unwrap()
                .is_missing()
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn calc_value_scalar_projection_helpers_are_core_explicit() {
        let number = CalcValue::number(12.5);
        let text = CalcValue::text(ExcelText::from_interop_assignment("x"));
        let logical = CalcValue::logical(true);
        let error = CalcValue::error(WorksheetErrorCode::NA);

        assert_eq!(number.core(), &CoreValue::Number(12.5));
        assert_eq!(number.as_number(), Some(12.5));
        assert_eq!(
            text.as_text().map(ExcelText::to_string_lossy),
            Some("x".to_string())
        );
        assert_eq!(logical.as_logical(), Some(true));
        assert_eq!(error.as_error(), Some(WorksheetErrorCode::NA));
        assert!(CalcValue::empty().is_empty());
        assert!(CalcValue::missing().is_missing());
        assert_eq!(number.as_text(), None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn calc_value_reference_and_array_projection_helpers_are_typed() {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = sheets.issue().unwrap();
        let reference = ReferenceLike::cell(CellKey::new(sheet, 1, 1).unwrap());
        let reference_value = CalcValue::reference(reference.clone());
        assert_eq!(reference_value.as_reference(), Some(&reference));

        let array = CalcArray::from_scalar(CalcValue::number(1.0));
        let array_value = CalcValue::array(array.clone());
        assert_eq!(array_value.as_array(), Some(&array));
        assert_eq!(array_value.as_reference(), None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn union_reference_keeps_typed_areas_in_order() {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = sheets.issue().unwrap();
        let key = |row, column| CellKey::new(sheet, row, column).unwrap();
        let left = GridArea::new(key(1, 1), key(2, 1)).unwrap();
        let right = GridArea::new(key(1, 3), key(2, 3)).unwrap();

        let union = ReferenceLike::from_areas(vec![left, right, left]).unwrap();
        assert_eq!(union.areas(), [left, right, left]);
        assert_eq!(union.area_count(), 3);
        assert_eq!(union.sheet(), sheet);
        assert_eq!(
            ReferenceLike::from_areas(vec![left]),
            Ok(ReferenceLike::area(left))
        );
        assert_eq!(
            ReferenceLike::from_areas(Vec::new()),
            Err(ReferenceError::NoAreas)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_union_reference_holds_areas_of_one_sheet_only() {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let first = sheets.issue().unwrap();
        let second = sheets.issue().unwrap();
        let on = |sheet, row, column| GridArea::cell(CellKey::new(sheet, row, column).unwrap());

        // Areas on two live sheets, wherever the other sheet's area comes in the list.
        assert_eq!(
            ReferenceLike::from_areas(vec![on(first, 1, 1), on(second, 1, 1)]),
            Err(ReferenceError::SheetMismatch {
                first,
                other: second
            })
        );
        assert_eq!(
            ReferenceLike::from_areas(vec![on(second, 2, 2), on(second, 3, 3), on(first, 2, 2)]),
            Err(ReferenceError::SheetMismatch {
                first: second,
                other: first
            })
        );

        // A released slot reissued under a new generation is another sheet.
        sheets.release(first).unwrap();
        let reissued = sheets.issue().unwrap();
        assert_eq!(reissued.slot(), first.slot());
        assert_eq!(
            ReferenceLike::from_areas(vec![on(first, 1, 1), on(reissued, 1, 1)]),
            Err(ReferenceError::SheetMismatch {
                first,
                other: reissued
            })
        );

        // Areas of one sheet are accepted, in order.
        let union = ReferenceLike::from_areas(vec![on(second, 1, 1), on(second, 5, 5)]).unwrap();
        assert_eq!((union.sheet(), union.area_count()), (second, 2));
        assert_eq!(
            ReferenceLike::cell(CellKey::new(second, 1, 1).unwrap()).sheet(),
            second
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn calc_value_callable_constructor_and_projection() {
        let value = CalcValue::callable(CallableValue::new(
            CallableArityShape::exact(1),
            "test.callable",
            Arc::new(TestCallable),
        ));
        assert_eq!(value.core(), &CoreValue::Error(WorksheetErrorCode::Calc));
        assert_eq!(value.as_error(), Some(WorksheetErrorCode::Calc));
        assert_eq!(
            value.callable_value().map(CallableValue::arity),
            Some(CallableArityShape::exact(1))
        );
        assert_eq!(
            value.callable_value().map(CallableValue::summary),
            Some("test.callable")
        );
        assert!(
            CalcValue::error(WorksheetErrorCode::Calc)
                .callable_value()
                .is_none()
        );
        assert_ne!(CalcValue::error(WorksheetErrorCode::Calc), value);
    }

    /// dc-hsa5.19 (Pro B1): `CalcValue::eq` once left cube metadata out, so two different cube
    /// members with one caption, and a cube value and plain text of that caption, compared equal.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn cube_members_with_one_caption_are_not_equal() {
        let caption = ExcelText::from_interop_assignment("Europe");
        let member = |unique: &str| {
            CalcValue::cube(
                caption.clone(),
                CubeValue {
                    connection: "Model".into(),
                    tuples: vec![vec![unique.into()]],
                    set: false,
                },
            )
        };
        let (a, b) = (member("[Geo].[Region].&[1]"), member("[Geo].[Region].&[2]"));
        assert_ne!(a, b);
        assert_eq!(a, member("[Geo].[Region].&[1]"));
        let text = CalcValue::text(caption.clone());
        assert_ne!(a, text);
        assert_ne!(text, a);
        // A set and a member of the same tuples differ too.
        let set = CalcValue::cube(
            caption.clone(),
            CubeValue {
                connection: "Model".into(),
                tuples: vec![vec!["[Geo].[Region].&[1]".into()]],
                set: true,
            },
        );
        assert_ne!(a, set);
        // The same holds inside an array and under representation equality.
        let one = |value: CalcValue| CalcArray::from_scalar(value);
        assert_ne!(CalcValue::array(one(a.clone())), CalcValue::array(one(b.clone())));
        assert!(!a.representation_eq(&b));
        assert!(!a.representation_eq(&text));
        assert!(a.representation_eq(&member("[Geo].[Region].&[1]")));
    }

    /// The relations of the note above `PartialEq for CalcValue`: runtime identity, and
    /// representation equality, which separates what `==` lets coincide.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn representation_equality_is_stricter_than_partial_eq() {
        // Numbers: IEEE equality for `==`, bit patterns for representation equality.
        let (zero, negative) = (CalcValue::number(0.0), CalcValue::number(-0.0));
        assert_eq!(zero, negative);
        assert!(!zero.representation_eq(&negative));
        let nan = CalcValue::number(f64::NAN);
        assert_ne!(nan, nan);
        assert!(nan.representation_eq(&nan.clone()));
        // The overflow number is qualified: an unqualified NaN of the same bits is another value.
        let bits = f64::from_bits(0x7ff8_0000_0000_0001);
        let overflow = CalcValue::excel_overflow_number(bits).expect("a NaN");
        assert!(overflow.representation_eq(&overflow.clone()));
        assert!(!overflow.representation_eq(&CalcValue::number(bits)));
        // Callables: the same handle is the same callable, an equal-looking handle is not.
        let handle: Arc<dyn OpaqueCallable> = Arc::new(TestCallable);
        let make = |handle: &Arc<dyn OpaqueCallable>| {
            CalcValue::callable(CallableValue::new(CallableArityShape::exact(1), "LAMBDA(x, x)", handle.clone()))
        };
        let (one, same) = (make(&handle), make(&handle));
        let other = make(&(Arc::new(TestCallable) as Arc<dyn OpaqueCallable>));
        assert!(one.same_callable(&same) && one.representation_eq(&same));
        assert!(!one.same_callable(&other) && !one.representation_eq(&other));
        assert!(!CalcValue::error(WorksheetErrorCode::Calc).same_callable(&CalcValue::error(WorksheetErrorCode::Calc)));
        assert!(!one.representation_eq(&CalcValue::error(WorksheetErrorCode::Calc)));
        // Arrays: shape, exposure and every cell count.
        let cells = || vec![CalcValue::number(1.0), CalcValue::number(0.0)];
        let row = CalcArray::new(ArrayShape { rows: 1, cols: 2 }, cells()).expect("a row");
        let column = CalcArray::new(ArrayShape { rows: 2, cols: 1 }, cells()).expect("a column");
        assert!(CalcValue::array(row.clone()).representation_eq(&CalcValue::array(row.clone())));
        assert!(!CalcValue::array(row.clone()).representation_eq(&CalcValue::array(column)));
        assert!(!CalcValue::array(row.clone()).representation_eq(&CalcValue::array(row.clone().boxed())));
        let negated = CalcArray::new(
            ArrayShape { rows: 1, cols: 2 },
            vec![CalcValue::number(1.0), CalcValue::number(-0.0)],
        )
        .expect("a row");
        assert_eq!(CalcValue::array(row.clone()), CalcValue::array(negated.clone()));
        assert!(!CalcValue::array(row).representation_eq(&CalcValue::array(negated)));
        // A picture is a value with its own qualification.
        let picture = |bytes: &[u8]| CalcValue::image(ImageValue::picture(ImageFormat::Png, bytes.into()));
        assert!(picture(b"a").representation_eq(&picture(b"a")));
        assert!(!picture(b"a").representation_eq(&picture(b"b")));
        assert!(!picture(b"a").representation_eq(&CalcValue::error(WorksheetErrorCode::Value)));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn callable_values_are_equal_only_when_they_share_a_handle() {
        let handle: Arc<dyn OpaqueCallable> = Arc::new(TestCallable);
        let one = CallableValue::new(CallableArityShape::exact(1), "LAMBDA(x, x)", handle.clone());
        let same = CallableValue::new(CallableArityShape::exact(1), "another summary", handle);
        let other = CallableValue::new(
            CallableArityShape::exact(1),
            "LAMBDA(x, x)",
            Arc::new(TestCallable),
        );
        assert_eq!(one, same);
        assert_ne!(one, other);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn callable_arity_shape_exact_accepts_only_matching_arity() {
        let arity = CallableArityShape::exact(2);
        assert!(!arity.accepts(1));
        assert!(arity.accepts(2));
        assert!(!arity.accepts(3));
        assert!(CallableArityShape::range(1, 3).accepts(3));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn there_are_fourteen_distinct_worksheet_error_codes() {
        let all = WorksheetErrorCode::all();
        for (index, code) in all.iter().enumerate() {
            assert!(!all[..index].contains(code), "{code:?} repeats");
        }
        assert_eq!(all.len(), 14);
    }
}
