//! Office.js batch messages shared by the client and workbook host.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum WireValue {
    Empty,
    Null,
    Missing,
    Bool {
        v: bool,
    },
    I4 {
        v: i32,
    },
    /// Exact IEEE-754 bits as 16 hex digits.
    R8 {
        bits: String,
    },
    Text {
        v: String,
    },
    /// UTF-16 code units, used only when the text is not valid UTF-16 (unpaired surrogates).
    Text16 {
        units: Vec<u16>,
    },
    /// Excel's CVErr number (2042 is #N/A).
    Error {
        v: i32,
    },
    Object {
        h: u64,
    },
    Array {
        rows: u32,
        cols: u32,
        v: Vec<WireValue>,
    },
}

impl WireValue {
    pub fn r8(value: f64) -> WireValue {
        WireValue::R8 {
            bits: format!("{:016x}", value.to_bits()),
        }
    }
    pub fn text(text: &str) -> WireValue {
        WireValue::Text {
            v: text.to_string(),
        }
    }
}

/// One queued operation. `target` and `out` are client-allocated handle numbers of the batch's
/// context.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Op {
    /// Bind a context root (`workbook`, `application`) to `out`.
    Root {
        name: String,
        out: u64,
    },
    /// Read a property that yields an object (a path step, no value returned).
    Get {
        target: u64,
        name: String,
        out: u64,
    },
    /// Call a method; `out` names the handle of an object result, if the client keeps one.
    Call {
        target: u64,
        name: String,
        #[serde(default)]
        args: Vec<WireValue>,
        #[serde(default)]
        out: Option<u64>,
    },
    Set {
        target: u64,
        name: String,
        value: WireValue,
    },
    /// Snapshot properties at this position in the batch.
    Load {
        target: u64,
        props: Vec<String>,
    },
    /// Keep the handle's reference lease across `Excel.run` (context.trackedObjects.add).
    Track {
        target: u64,
    },
    Untrack {
        target: u64,
    },
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
pub struct ReleaseRequest {
    pub context: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BatchReply {
    pub batch_id: u64,
    pub results: Vec<OpResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<BatchError>,
}
