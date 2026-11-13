//! Automation values: the one value model every projection lowers to and from.

use crate::object::ObjectRef;
use calc_value::ExcelText;

/// A value crossing the object model, as Automation types it. Projections map their own
/// representation (COM VARIANT, CalcVBA's Variant, JavaScript) to and from this, losslessly.
#[derive(Clone, Debug)]
pub enum AutoValue {
    /// VT_EMPTY.
    Empty,
    /// VT_NULL.
    Null,
    /// An omitted optional argument (VT_ERROR with DISP_E_PARAMNOTFOUND on COM).
    Missing,
    /// VT_BOOL.
    Bool(bool),
    /// VT_I2.
    I2(i16),
    /// VT_I4 (also enum arguments).
    I4(i32),
    /// VT_R4.
    R4(f32),
    /// VT_R8.
    R8(f64),
    /// VT_CY: the raw 64-bit integer, value times 10,000.
    Currency(i64),
    /// VT_DATE: the OLE date number.
    Date(f64),
    /// VT_BSTR: UTF-16 code units, unpaired surrogates kept.
    Text(ExcelText),
    /// VT_ERROR holding Excel's CVErr number (2000 + the XLL error code: 2007 is #DIV/0!).
    /// [`AutoValue::scode`] gives the COM SCODE (`0x800A0000 | number`).
    Error(i32),
    /// VT_DISPATCH: an object of this model.
    Object(ObjectRef),
    /// A foreign object passed in by a client (an add-in's own IDispatch): the projection keeps
    /// it alive under this handle and the core passes it through unchanged.
    Foreign(u64),
    /// VT_ARRAY | VT_VARIANT, two-dimensional, 1-based as Excel's Range.Value returns.
    Array(Box<AutoArray>),
}

/// A rectangular array of values in row-major order.
#[derive(Clone, Debug)]
pub struct AutoArray {
    pub rows: u32,
    pub columns: u32,
    /// Lower bounds of the two dimensions (Range.Value uses 1, 1).
    pub lower: (i32, i32),
    pub values: Vec<AutoValue>,
}

impl AutoValue {
    pub fn text(text: &str) -> AutoValue {
        AutoValue::Text(ExcelText::from_utf16_code_units(text.encode_utf16().collect()))
    }
    /// The COM SCODE of an [`AutoValue::Error`].
    pub fn scode(number: i32) -> i32 {
        (0x800A_0000u32 | (number as u32 & 0xFFFF)) as i32
    }
    pub fn is_missing(&self) -> bool {
        matches!(self, AutoValue::Missing)
    }
    pub fn as_text(&self) -> Option<String> {
        match self {
            AutoValue::Text(text) => Some(String::from_utf16_lossy(text.utf16_code_units())),
            _ => None,
        }
    }
}
