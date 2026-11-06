//! The versioned wire codec for operation batches.
//!
//! A batch is one JSON object, written without whitespace and with its keys in a fixed order:
//!
//! ```text
//! {"format":"dnacalc/workbook-op-batch","version":1,"base":<revision>,"operations":[<envelope>,...]}
//! ```
//!
//! Every integer (the frame's version and base, an envelope's version, and the rows, columns,
//! counts, positions and sheet keys of the arguments) travels as a JSON decimal number. The base
//! is a revision, which may be as large as 2^64 - 1, so a reader that parses the message's numbers
//! as JavaScript numbers (binary64) loses precision above 2^53 and can misread the base. This
//! codec's reader keeps every integer that fits `u64` exactly.
//!
//! Each operation travels in an envelope that names its capability and that capability's
//! version: `{"capability":"cells","version":1,"op":"set_cell","args":{...}}`. Each capability's
//! codec, the list [`op_capabilities`] returns, and the dispatch from an envelope's capability name
//! to its codec are all generated from the one set of declarations in `capability.rs`. Sheets
//! travel as [`SheetKey`]s, never as [`SheetId`]s, and are resolved through the receiving
//! [`ModelRoot`]. Numbers travel as the 16 lowercase hex digits of their binary64 bits and text as
//! 4 lowercase hex digits per UTF-16 code unit, so no decimal conversion touches a number and an
//! unpaired surrogate survives. Block and placement IDs travel as their text form, 32 lowercase
//! hex digits in groups of 8, 4, 4, 4 and 12 joined by `-`; IDs are issued by the caller and
//! decoding checks only their form.
//!
//! Decoding refuses, with a typed [`DecodeError`], anything it does not admit: an unknown format,
//! capability, operation or field, a version other than the declared one, a duplicate or missing
//! field, a non-finite number, and a value outside its type. Nothing is skipped and nothing missing
//! takes a default. Names (the format, capabilities, operations, visibilities and error codes) match
//! exactly, case included; hex digits are lowercase, with no sign or prefix, at their exact width;
//! an integer is a JSON integer from 0, never negative, fractional or a string. A frame's format
//! and version, and an envelope's capability and version, are judged before the other members of
//! their object, so a newer version that adds a member is refused as a version, not as a field.

mod capability;
mod json;
mod value;

use crate::input::FormulaTextError;
use crate::op::{AxisEditError, OpBatch, WorkbookOp};
use crate::revision::AuthoredRevision;
use crate::root::ModelRoot;
use crate::sheet::{SheetKey, SheetNameError};
use calc_value::{GridError, SheetId};
use capability::{WireOp, read_envelope};
use json::{Fields, Node};
use std::fmt;

/// The format name of an operation batch.
pub const BATCH_FORMAT: &str = "dnacalc/workbook-op-batch";

/// The version of the batch frame this codec writes and reads.
pub const BATCH_VERSION: u32 = 1;

/// One capability of the wire codec: its name, its version and its operation names, as its
/// declaration states them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpCapability {
    pub name: &'static str,
    pub version: u32,
    pub operations: &'static [&'static str],
}

/// Every capability this codec writes and reads.
pub fn op_capabilities() -> &'static [OpCapability] {
    capability::CAPABILITIES
}

/// Why [`decode_batch`] refused a message. `at` locates the offending value as a path from the
/// message root `$`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The bytes are not one JSON value.
    Syntax {
        detail: String,
    },
    UnknownFormat {
        at: String,
        format: String,
    },
    /// The frame or a capability carries a version other than the one this codec declares.
    UnsupportedVersion {
        at: String,
        envelope: String,
        found: u64,
        supported: u32,
    },
    UnknownCapability {
        at: String,
        capability: String,
    },
    UnknownOperation {
        at: String,
        capability: &'static str,
        operation: String,
    },
    UnknownField {
        at: String,
        field: String,
    },
    DuplicateField {
        at: String,
        field: String,
    },
    MissingField {
        at: String,
        field: String,
    },
    /// The value at `at` is not what its field holds.
    Malformed {
        at: String,
        expected: &'static str,
    },
    /// The bits encode NaN or an infinity.
    NonFiniteNumber {
        at: String,
        bits: u64,
    },
    /// Cell text longer than [`calc_value::EXCEL_TEXT_MAX_UTF16_CODE_UNITS`] code units.
    TextTooLong {
        at: String,
        units: usize,
    },
    /// The formula text is empty or begins with `=`.
    InvalidFormula {
        at: String,
        error: FormulaTextError,
    },
    InvalidSheetName {
        at: String,
        error: SheetNameError,
    },
    /// The receiving root has no sheet with this key.
    UnknownSheetKey {
        at: String,
        key: SheetKey,
    },
    /// An inserted sheet's key belongs to a sheet of the receiving root or to an earlier insert of
    /// the batch.
    SheetKeyInUse {
        at: String,
        key: SheetKey,
    },
    InvalidCell {
        at: String,
        error: GridError,
    },
    /// An area's top is below its bottom or its left is right of its right.
    InvertedArea {
        at: String,
    },
    InvalidAxisEdit {
        at: String,
        error: AxisEditError,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Syntax { detail } => write!(f, "not one JSON value: {detail}"),
            DecodeError::UnknownFormat { at, format } => {
                write!(f, "{at}: unknown format {format:?}")
            }
            DecodeError::UnsupportedVersion {
                at,
                envelope,
                found,
                supported,
            } => write!(
                f,
                "{at}: {envelope} version {found} is not the supported version {supported}"
            ),
            DecodeError::UnknownCapability { at, capability } => {
                write!(f, "{at}: unknown capability {capability:?}")
            }
            DecodeError::UnknownOperation {
                at,
                capability,
                operation,
            } => write!(f, "{at}: {capability} has no operation {operation:?}"),
            DecodeError::UnknownField { at, field } => write!(f, "{at}: unknown field {field:?}"),
            DecodeError::DuplicateField { at, field } => {
                write!(f, "{at}: field {field:?} appears twice")
            }
            DecodeError::MissingField { at, field } => write!(f, "{at}: missing field {field:?}"),
            DecodeError::Malformed { at, expected } => write!(f, "{at}: expected {expected}"),
            DecodeError::NonFiniteNumber { at, bits } => {
                write!(f, "{at}: bits {bits:016x} are not a finite number")
            }
            DecodeError::TextTooLong { at, units } => {
                write!(
                    f,
                    "{at}: cell text of {units} UTF-16 code units is too long"
                )
            }
            DecodeError::InvalidFormula { at, error } => write!(f, "{at}: {error}"),
            DecodeError::InvalidSheetName { at, error } => write!(f, "{at}: {error}"),
            DecodeError::UnknownSheetKey { at, key } => {
                write!(f, "{at}: no sheet has the key {}", key.get())
            }
            DecodeError::SheetKeyInUse { at, key } => {
                write!(f, "{at}: the sheet key {} is in use", key.get())
            }
            DecodeError::InvalidCell { at, error } => write!(f, "{at}: {error}"),
            DecodeError::InvertedArea { at } => {
                write!(
                    f,
                    "{at}: an area's top is below its bottom or its left is right of its right"
                )
            }
            DecodeError::InvalidAxisEdit { at, error } => write!(f, "{at}: {error}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Why [`encode_batch`] refused a batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeError {
    PageSetup {operation:usize},
    Charts {operation:usize},
    ReviewNotes {
        operation: usize,
    },
    AutoFilter { operation: usize },
    Tables {operation:usize},
    ConditionalRules {
        operation: usize,
    },
    /// An operation names a sheet the root does not hold, so it has no key to travel as.
    UnknownSheet {
        sheet: SheetId,
    },
    /// An inserted sheet's key belongs to a sheet of the root or to an earlier insert of the
    /// batch.
    SheetKeyInUse {
        key: SheetKey,
    },
    /// The operation at this index authors an array formula, which wire version 1 has no form
    /// for.
    ArrayFormula {
        operation: usize,
    },
    /// The operation at this index sets a cell format, which wire version 1 has no form for.
    CellFormat {
        operation: usize,
    },
    /// The operation at this index sets a defined name's formula, which wire version 1 has no
    /// form for.
    NameFormula {
        operation: usize,
    },
    /// The operation at this index sets the iterative-calculation settings, which wire version 1
    /// has no form for.
    Iteration {
        operation: usize,
    },
    /// The operation sets worksheet panes, which wire version 1 has no form for.
    FrozenPanes {
        operation: usize,
    },
    AxisFormat {
        operation: usize,
    },
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::PageSetup {operation}=>write!(f,"page setup changes in operation {operation} are not modelled by wire version 1 (dc-971p.56)"),
            EncodeError::Charts {operation}=>write!(f,"chart changes in operation {operation} are not modelled by this wire version (dc-e5z5)"),
            EncodeError::ReviewNotes { operation } => write!(
                f,
                "operation {operation} changes review notes, which wire version 1 does not encode"
            ),
            EncodeError::Tables {operation}=>write!(f,"operation {operation} changes tables, which wire version 1 does not encode"),
            EncodeError::AutoFilter { operation } => write!(f, "operation {operation} changes AutoFilter, which wire version 1 does not encode"),
            EncodeError::ConditionalRules { operation } => write!(
                f,
                "operation {operation} changes conditional rules, which wire version 1 does not encode"
            ),
            EncodeError::AxisFormat { operation } => write!(
                f,
                "operation {operation} changes physical dimensions, which wire version 1 does not encode"
            ),
            EncodeError::FrozenPanes { operation } => write!(
                f,
                "operation {operation} sets worksheet panes, which wire version 1 has no form for"
            ),
            EncodeError::UnknownSheet { sheet } => {
                write!(f, "the root holds no sheet {sheet:?}")
            }
            EncodeError::SheetKeyInUse { key } => {
                write!(f, "the sheet key {} is in use", key.get())
            }
            EncodeError::ArrayFormula { operation } => write!(
                f,
                "operation {operation} authors an array formula, which wire version 1 has no form for"
            ),
            EncodeError::CellFormat { operation } => write!(
                f,
                "operation {operation} sets a cell format, which wire version 1 has no form for"
            ),
            EncodeError::NameFormula { operation } => write!(
                f,
                "operation {operation} sets a defined name's formula, which wire version 1 has no form for"
            ),
            EncodeError::Iteration { operation } => write!(
                f,
                "operation {operation} sets the iterative-calculation settings, which wire version 1 has no form for"
            ),
        }
    }
}

impl std::error::Error for EncodeError {}

/// The batch as its wire message, with each sheet written as its key in `root`. Like
/// [`decode_batch`], it reads `root` as it is, not as the batch's earlier operations leave it: an
/// operation on a sheet that an earlier remove of the batch takes away still encodes.
///
/// Integers are written as JSON decimal numbers, the base revision among them, so a reader that
/// parses the message's numbers as JavaScript numbers (binary64) loses precision above 2^53.
pub fn encode_batch(batch: &OpBatch, root: &ModelRoot) -> Result<Vec<u8>, EncodeError> {
    let mut claimed = Vec::new();
    let mut operations = Vec::with_capacity(batch.operations.len());
    for (index, operation) in batch.operations.iter().enumerate() {
        if matches!(operation, WorkbookOp::SetArray { .. }) {
            return Err(EncodeError::ArrayFormula { operation: index });
        }
        if matches!(
            operation,
            WorkbookOp::SetFormat { .. } | WorkbookOp::SetStyles(_)
        ) {
            return Err(EncodeError::CellFormat { operation: index });
        }
        if matches!(
            operation,
            WorkbookOp::SetNameFormula { .. } | WorkbookOp::RestoreNames(_)
        ) {
            return Err(EncodeError::NameFormula { operation: index });
        }
        if matches!(operation, WorkbookOp::SetIteration(_)) {
            return Err(EncodeError::Iteration { operation: index });
        }
        if matches!(operation, WorkbookOp::SetPageSetup {..}) {return Err(EncodeError::PageSetup {operation:index});}
        if matches!(operation, WorkbookOp::SetCharts {..}) {return Err(EncodeError::Charts {operation:index});}
        if matches!(operation, WorkbookOp::SetTables(_)) {return Err(EncodeError::Tables{operation:index});}
        if matches!(operation, WorkbookOp::SetAutoFilter { .. }) { return Err(EncodeError::AutoFilter { operation:index }); }
        if matches!(operation, WorkbookOp::SetAxisFormat(_)) {
            return Err(EncodeError::AxisFormat { operation: index });
        }
        if matches!(operation, WorkbookOp::SetReviewLinks { .. } | WorkbookOp::SetReviewNotes { .. } | WorkbookOp::SetValidations { .. }) {
            return Err(EncodeError::ReviewNotes { operation: index });
        }
        if matches!(operation, WorkbookOp::SetConditionalRules { .. }) {
            return Err(EncodeError::ConditionalRules { operation: index });
        }
        if matches!(operation, WorkbookOp::SetFrozenPanes { .. } | WorkbookOp::SetPaneLayout { .. }) {
            return Err(EncodeError::FrozenPanes { operation: index });
        }
        operations.push(WireOp::from_op(operation, root, &mut claimed)?);
    }
    let mut out = String::new();
    out.push_str("{\"format\":\"");
    out.push_str(BATCH_FORMAT);
    out.push_str("\",\"version\":");
    json::push_unsigned(&mut out, u64::from(BATCH_VERSION));
    out.push_str(",\"base\":");
    json::push_unsigned(&mut out, batch.base.get());
    out.push_str(",\"operations\":[");
    for (index, operation) in operations.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        operation.write(&mut out);
    }
    out.push_str("]}");
    Ok(out.into_bytes())
}

/// The members of the batch frame at [`BATCH_VERSION`].
const FRAME_FIELDS: [&str; 4] = ["format", "version", "base", "operations"];

/// The batch a wire message holds, with each sheet key resolved through `root`. The format and
/// then the version are judged before the frame's other members, so a message of another format
/// or of another frame version is refused as such, whatever members it carries. Every frame and
/// envelope member is required: a missing one is refused, never given a default.
///
/// Integers arrive as JSON decimal numbers and are read exactly up to 2^64 - 1; a reader that
/// parses the same message's numbers as JavaScript numbers (binary64) loses precision above 2^53.
///
/// What it does not check, which is left to whoever applies the batch:
///
/// - Keys resolve through `root` as it is, not as the batch's earlier operations leave it. An
///   operation naming the key an earlier insert of the same batch adds is refused with
///   [`DecodeError::UnknownSheetKey`], and an operation naming a key that an earlier remove of the
///   same batch takes away still decodes. The applier checks each operation against the state the
///   earlier ones leave.
/// - The batch's `base` is not compared with `root.revision()`: a batch based on another revision
///   decodes. Comparing them is the host's, when it commits the batch.
/// - An inserted key is refused only when `root` or an earlier insert of the batch holds it.
///   `root` keeps no record of the keys removed sheets held, so a reused key is not detected
///   here; the applier checks it.
pub fn decode_batch(bytes: &[u8], root: &ModelRoot) -> Result<OpBatch, DecodeError> {
    let message = json::parse(bytes)?;
    let format = json::lone_text(&message, "$", "format")?;
    if format != BATCH_FORMAT {
        return Err(DecodeError::UnknownFormat {
            at: "$.format".to_owned(),
            format: format.to_owned(),
        });
    }
    check_version(
        "$.version",
        BATCH_FORMAT,
        json::lone_unsigned(&message, "$", "version")?,
        BATCH_VERSION,
    )?;
    let frame = Fields::new(&message, "$", &FRAME_FIELDS)?;
    let base = AuthoredRevision::new(frame.unsigned("base")?);
    let Node::Array(envelopes) = frame.node("operations")? else {
        return Err(json::malformed("$.operations", "an array of operations"));
    };
    let mut claimed = Vec::new();
    let mut operations = Vec::with_capacity(envelopes.len());
    for (index, envelope) in envelopes.iter().enumerate() {
        let at = format!("$.operations[{index}]");
        let operation = read_envelope(envelope, &at)?;
        operations.push(operation.into_op(root, &mut claimed, &at)?);
    }
    Ok(OpBatch { base, operations })
}

/// Refuses a frame or capability version other than the declared one.
fn check_version(at: &str, envelope: &str, found: u64, supported: u32) -> Result<(), DecodeError> {
    if found == u64::from(supported) {
        return Ok(());
    }
    Err(DecodeError::UnsupportedVersion {
        at: at.to_owned(),
        envelope: envelope.to_owned(),
        found,
        supported,
    })
}
