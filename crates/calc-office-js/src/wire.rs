//! The batch wire protocol. Values are lossless: `-0.0` keeps its bits, text keeps unpaired
//! surrogates, and Empty, Null and Error stay apart (the client adapter alone folds them into
//! Office.js's `any[][]`).

use calc_object_model::{AutoArray, AutoValue};
use calc_value::ExcelText;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum WireValue {
    Empty,
    Null,
    Missing,
    Bool { v: bool },
    I4 { v: i32 },
    /// Exact IEEE-754 bits as 16 hex digits, and the decimal for readers (ignored on input).
    R8 { bits: String },
    Text { v: String },
    /// UTF-16 code units, used only when the text is not valid UTF-16 (unpaired surrogates).
    Text16 { units: Vec<u16> },
    /// Excel's CVErr number (2042 is #N/A).
    Error { v: i32 },
    Object { h: u64 },
    Array { rows: u32, cols: u32, v: Vec<WireValue> },
}

impl WireValue {
    pub fn r8(value: f64) -> WireValue {
        WireValue::R8 { bits: format!("{:016x}", value.to_bits()) }
    }
    pub fn text(text: &str) -> WireValue {
        WireValue::Text { v: text.to_string() }
    }
}

/// One queued operation. `target` and `out` are client-allocated handle numbers of the batch's
/// context; handle 0 is the Application.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Op {
    /// Bind a context root (`workbook`, `application`) to `out`.
    Root { name: String, out: u64 },
    /// Read a property that yields an object (a path step, no value returned).
    Get { target: u64, name: String, out: u64 },
    /// Call a method; `out` names the handle of an object result, if the client keeps one.
    Call { target: u64, name: String, #[serde(default)] args: Vec<WireValue>, #[serde(default)] out: Option<u64> },
    Set { target: u64, name: String, value: WireValue },
    /// Snapshot properties at this position in the batch.
    Load { target: u64, props: Vec<String> },
    /// Keep the handle's reference lease across `Excel.run` (context.trackedObjects.add).
    Track { target: u64 },
    Untrack { target: u64 },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BatchRequest {
    /// Unique per batch; a lost transport reports an unknown outcome and never replays.
    pub batch_id: u64,
    pub context: u64,
    pub ops: Vec<Op>,
    /// The final sync of `Excel.run`: untracked handles of the context are released after it.
    #[serde(default)]
    pub final_release: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OpResult {
    pub ordinal: usize,
    /// Loaded property values by Office.js name (for `Load`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loaded: Vec<(String, WireValue)>,
    /// The `*OrNullObject` call found nothing: the proxy's `isNullObject` is true.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub null_object: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BatchError {
    /// Office.js error code: `ApiNotFound`, `GeneralException`, `InvalidObjectPath`, `ItemNotFound`.
    pub code: String,
    pub message: String,
    /// The refusal's owning work package when the member is not modelled yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Ordinal of the failing op; later ops did not run and earlier ones stay committed.
    pub ordinal: usize,
    /// Whether the failing op itself committed an authored edit (never for these errors).
    pub committed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BatchReply {
    pub batch_id: u64,
    pub results: Vec<OpResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<BatchError>,
}

pub fn to_wire(value: &AutoValue, handle_of: &mut dyn FnMut(&calc_object_model::ObjectRef) -> u64) -> WireValue {
    match value {
        AutoValue::Empty => WireValue::Empty,
        AutoValue::Null => WireValue::Null,
        AutoValue::Missing => WireValue::Missing,
        AutoValue::Bool(v) => WireValue::Bool { v: *v },
        AutoValue::I2(v) => WireValue::I4 { v: i32::from(*v) },
        AutoValue::I4(v) => WireValue::I4 { v: *v },
        AutoValue::R4(v) => WireValue::r8(f64::from(*v)),
        AutoValue::R8(v) | AutoValue::Date(v) => WireValue::r8(*v),
        AutoValue::Currency(v) => WireValue::r8(*v as f64 / 10_000.0),
        AutoValue::Text(text) => {
            let units = text.utf16_code_units();
            match String::from_utf16(units) {
                Ok(v) => WireValue::Text { v },
                Err(_) => WireValue::Text16 { units: units.to_vec() },
            }
        }
        AutoValue::Error(v) => WireValue::Error { v: *v },
        AutoValue::Object(object) => WireValue::Object { h: handle_of(object) },
        AutoValue::Foreign(h) => WireValue::Object { h: *h },
        AutoValue::Array(array) => WireValue::Array {
            rows: array.rows,
            cols: array.columns,
            v: array.values.iter().map(|v| to_wire(v, handle_of)).collect(),
        },
    }
}

pub fn from_wire(value: &WireValue, object_of: &dyn Fn(u64) -> Option<calc_object_model::ObjectRef>) -> Result<AutoValue, String> {
    Ok(match value {
        WireValue::Empty => AutoValue::Empty,
        WireValue::Null => AutoValue::Null,
        WireValue::Missing => AutoValue::Missing,
        WireValue::Bool { v } => AutoValue::Bool(*v),
        WireValue::I4 { v } => AutoValue::I4(*v),
        WireValue::R8 { bits } => AutoValue::R8(f64::from_bits(
            u64::from_str_radix(bits, 16).map_err(|_| format!("bad r8 bits {bits:?}"))?,
        )),
        WireValue::Text { v } => AutoValue::text(v),
        WireValue::Text16 { units } => AutoValue::Text(ExcelText::from_utf16_code_units(units.clone())),
        WireValue::Error { v } => AutoValue::Error(*v),
        WireValue::Object { h } => AutoValue::Object(object_of(*h).ok_or_else(|| format!("unknown handle {h}"))?),
        WireValue::Array { rows, cols, v } => {
            if (*rows as usize) * (*cols as usize) != v.len() {
                return Err("array size does not match rows x cols".into());
            }
            AutoValue::Array(Box::new(AutoArray {
                rows: *rows,
                columns: *cols,
                lower: (1, 1),
                values: v.iter().map(|x| from_wire(x, object_of)).collect::<Result<_, _>>()?,
            }))
        }
    })
}
