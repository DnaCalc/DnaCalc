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
    /// VT_ARRAY | VT_VARIANT, with the Automation rank and lower bounds.
    Array(Box<AutoArray>),
}

/// The dimensions in logical (VBA subscript) order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoArrayShape {
    /// A vector such as Series.Values.
    Vector {
        /// Lower bound of the only dimension.
        lower: i32,
    },
    /// A worksheet-shaped array such as Range.Value.
    Matrix {
        /// Number of rows.
        rows: u32,
        /// Number of columns.
        columns: u32,
        /// Lower bounds of the row and column dimensions.
        lower: (i32, i32),
    },
}

/// An array of Automation values. Matrices store their elements in row-major order.
#[derive(Clone, Debug)]
pub struct AutoArray {
    /// Rank and bounds.
    pub shape: AutoArrayShape,
    /// Elements, with columns varying fastest in a matrix.
    pub values: Vec<AutoValue>,
}

impl AutoArray {
    /// Construct a one-dimensional array with the supplied lower bound.
    pub fn vector(lower: i32, values: Vec<AutoValue>) -> Self {
        Self { shape: AutoArrayShape::Vector { lower }, values }
    }

    /// Construct a two-dimensional array with the supplied row and column bounds.
    pub fn matrix(rows: u32, columns: u32, lower: (i32, i32), values: Vec<AutoValue>) -> Self {
        Self { shape: AutoArrayShape::Matrix { rows, columns, lower }, values }
    }

    /// Shape for consumers that need a grid. A vector provides one row in that grid.
    pub fn grid_size(&self) -> (u32, u32) {
        match self.shape {
            AutoArrayShape::Vector { .. } => (1, self.values.len() as u32),
            AutoArrayShape::Matrix { rows, columns, .. } => (rows, columns),
        }
    }
}

impl AutoValue {
    /// Encode a Rust string as an Automation UTF-16 text value.
    pub fn text(text: &str) -> AutoValue {
        AutoValue::Text(ExcelText::from_utf16_code_units(text.encode_utf16().collect()))
    }
    /// The COM SCODE of an [`AutoValue::Error`].
    pub fn scode(number: i32) -> i32 {
        (0x800A_0000u32 | (number as u32 & 0xFFFF)) as i32
    }
    /// Determine whether this value represents an omitted optional argument.
    pub fn is_missing(&self) -> bool {
        matches!(self, AutoValue::Missing)
    }
    /// Return text as a Rust string, replacing unpaired surrogates. Other value types return None.
    pub fn as_text(&self) -> Option<String> {
        match self {
            AutoValue::Text(text) => Some(String::from_utf16_lossy(text.utf16_code_units())),
            _ => None,
        }
    }
}
