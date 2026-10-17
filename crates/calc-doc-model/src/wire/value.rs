//! The wire forms of the values an operation's arguments hold. A capability's declaration names
//! each argument's type, and its generated codec writes and reads the argument through
//! [`WireValue`].

use super::DecodeError;
use super::json::{self, Fields, Node};
use crate::input::{CellInput, CellText, FiniteNumber, FormulaText};
use crate::sheet::{SheetKey, SheetName, SheetVisibility};
use calc_value::{ExcelText, WorksheetErrorCode};
use std::num::NonZeroU32;

/// A value with one wire form: `write` appends it and `read` admits exactly that form.
pub(crate) trait WireValue: Sized {
    fn write(&self, out: &mut String);
    fn read(node: &Node, at: &str) -> Result<Self, DecodeError>;
}

impl WireValue for u32 {
    fn write(&self, out: &mut String) {
        json::push_unsigned(out, u64::from(*self));
    }

    fn read(node: &Node, at: &str) -> Result<Self, DecodeError> {
        match node {
            Node::Unsigned(value) => u32::try_from(*value).ok(),
            _ => None,
        }
        .ok_or_else(|| json::malformed(at, "an integer from 0 to 4294967295"))
    }
}

impl WireValue for NonZeroU32 {
    fn write(&self, out: &mut String) {
        self.get().write(out);
    }

    fn read(node: &Node, at: &str) -> Result<Self, DecodeError> {
        match node {
            Node::Unsigned(value) => u32::try_from(*value).ok().and_then(NonZeroU32::new),
            _ => None,
        }
        .ok_or_else(|| json::malformed(at, "an integer from 1 to 4294967295"))
    }
}

impl WireValue for SheetKey {
    fn write(&self, out: &mut String) {
        self.get().write(out);
    }

    fn read(node: &Node, at: &str) -> Result<Self, DecodeError> {
        u32::read(node, at).map(SheetKey::new)
    }
}

/// The text of a string of 4 lowercase hex digits per UTF-16 code unit.
fn read_units(node: &Node, at: &str) -> Result<ExcelText, DecodeError> {
    match node {
        Node::Text(text) => json::parse_units(text),
        _ => None,
    }
    .map(ExcelText::from_utf16_code_units)
    .ok_or_else(|| json::malformed(at, "4 lowercase hex digits per UTF-16 code unit"))
}

fn write_units(out: &mut String, text: &ExcelText) {
    out.push('"');
    json::push_units(out, text.utf16_code_units());
    out.push('"');
}

impl WireValue for SheetName {
    fn write(&self, out: &mut String) {
        write_units(out, self.text());
    }

    fn read(node: &Node, at: &str) -> Result<Self, DecodeError> {
        SheetName::new(read_units(node, at)?).map_err(|error| DecodeError::InvalidSheetName {
            at: at.to_owned(),
            error,
        })
    }
}

impl WireValue for SheetVisibility {
    fn write(&self, out: &mut String) {
        out.push_str(match self {
            SheetVisibility::Visible => "\"visible\"",
            SheetVisibility::Hidden => "\"hidden\"",
            SheetVisibility::VeryHidden => "\"very_hidden\"",
        });
    }

    fn read(node: &Node, at: &str) -> Result<Self, DecodeError> {
        match node {
            Node::Text(text) => match text.as_str() {
                "visible" => Some(SheetVisibility::Visible),
                "hidden" => Some(SheetVisibility::Hidden),
                "very_hidden" => Some(SheetVisibility::VeryHidden),
                _ => None,
            },
            _ => None,
        }
        .ok_or_else(|| json::malformed(at, "\"visible\", \"hidden\" or \"very_hidden\""))
    }
}

/// The wire name of a worksheet error code.
fn error_name(code: WorksheetErrorCode) -> &'static str {
    match code {
        WorksheetErrorCode::Null => "null",
        WorksheetErrorCode::Div0 => "div0",
        WorksheetErrorCode::Value => "value",
        WorksheetErrorCode::Ref => "ref",
        WorksheetErrorCode::Name => "name",
        WorksheetErrorCode::Num => "num",
        WorksheetErrorCode::NA => "na",
        WorksheetErrorCode::Busy => "busy",
        WorksheetErrorCode::GettingData => "getting_data",
        WorksheetErrorCode::Spill => "spill",
        WorksheetErrorCode::Calc => "calc",
        WorksheetErrorCode::Field => "field",
        WorksheetErrorCode::Blocked => "blocked",
        WorksheetErrorCode::Connect => "connect",
    }
}

/// The input kinds a cell input object may hold, exactly one at a time.
const INPUT_KINDS: [&str; 5] = ["number", "text", "logical", "error", "formula"];

/// The number with these bits: 16 lowercase hex digits encoding a finite binary64.
fn read_number(node: &Node, at: &str) -> Result<FiniteNumber, DecodeError> {
    let bits = match node {
        Node::Text(text) if text.len() == 16 => json::parse_hex(text),
        _ => None,
    }
    .ok_or_else(|| json::malformed(at, "16 lowercase hex digits of binary64 bits"))?;
    FiniteNumber::from_bits(bits).ok_or_else(|| DecodeError::NonFiniteNumber {
        at: at.to_owned(),
        bits,
    })
}

/// A cell input is an object with exactly one member, named for its kind:
/// `{"number":"<bits>"}`, `{"text":"<units>"}`, `{"logical":<bool>}`, `{"error":"<name>"}` or
/// `{"formula":"<units>"}`. A member beside it (a cached value, say) is refused.
impl WireValue for CellInput {
    fn write(&self, out: &mut String) {
        match self {
            CellInput::Number(number) => {
                out.push_str("{\"number\":\"");
                json::push_hex(out, number.to_bits(), 16);
                out.push_str("\"}");
            }
            CellInput::Text(text) => {
                out.push_str("{\"text\":");
                write_units(out, text.text());
                out.push('}');
            }
            CellInput::Logical(value) => out.push_str(if *value {
                "{\"logical\":true}"
            } else {
                "{\"logical\":false}"
            }),
            CellInput::Error(code) => {
                out.push_str("{\"error\":\"");
                out.push_str(error_name(*code));
                out.push_str("\"}");
            }
            CellInput::Formula(formula) => {
                out.push_str("{\"formula\":");
                write_units(out, formula.text());
                out.push('}');
            }
        }
    }

    fn read(node: &Node, at: &str) -> Result<Self, DecodeError> {
        let fields = Fields::new(node, at, &INPUT_KINDS)?;
        let [(kind, value)] = fields.members() else {
            return Err(json::malformed(
                at,
                "exactly one of number, text, logical, error or formula",
            ));
        };
        let at = fields.path(kind);
        match kind.as_str() {
            "number" => read_number(value, &at).map(CellInput::Number),
            "text" => {
                let text = read_units(value, &at)?;
                let units = text.len_utf16_code_units();
                CellText::new(text)
                    .map(CellInput::Text)
                    .ok_or(DecodeError::TextTooLong { at, units })
            }
            "logical" => match value {
                Node::Bool(value) => Ok(CellInput::Logical(*value)),
                _ => Err(json::malformed(&at, "true or false")),
            },
            "error" => match value {
                Node::Text(name) => WorksheetErrorCode::all()
                    .into_iter()
                    .find(|code| error_name(*code) == name)
                    .map(CellInput::Error),
                _ => None,
            }
            .ok_or_else(|| json::malformed(&at, "the wire name of a worksheet error code")),
            // Fields::new admitted only INPUT_KINDS, so this kind is "formula".
            _ => FormulaText::new(read_units(value, &at)?)
                .map(CellInput::Formula)
                .map_err(|error| DecodeError::InvalidFormula { at, error }),
        }
    }
}
