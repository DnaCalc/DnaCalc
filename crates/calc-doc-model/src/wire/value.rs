//! The wire forms of the values an operation's arguments hold. A capability's declaration names
//! each argument's type, and its generated codec writes and reads the argument through
//! [`WireValue`].

use super::DecodeError;
use super::json::{self, Fields, Node};
use crate::input::{CellInput, CellText, FiniteNumber, FormulaText};
use crate::layout::{BlockId, BlockKind, CanvasArea, PlacementId, Spot};
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
                if formula.is_form_explicit() {
                    out.push_str(match formula.form() {
                        crate::FormulaForm::Dynamic => "{\"form\":\"dynamic\",\"text\":",
                        crate::FormulaForm::Legacy => "{\"form\":\"legacy\",\"text\":",
                    });
                    write_units(out, formula.text());
                    out.push('}');
                } else {
                    write_units(out, formula.text());
                }
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
            _ => {
                let (text, form) = if matches!(value, Node::Object(members) if members.iter().any(|(key,_)| key == "form"))
                {
                    let fields = Fields::new(value, &at, &["form", "text"])?;
                    let members = fields.members();
                    let form = members
                        .iter()
                        .find(|(name, _)| name == "form")
                        .map(|(_, value)| value);
                    let text = members
                        .iter()
                        .find(|(name, _)| name == "text")
                        .map(|(_, value)| value);
                    if members.len() != 2
                        || !matches!(form,Some(Node::Text(name)) if name == "dynamic" || name == "legacy")
                    {
                        return Err(json::malformed(
                            &at,
                            "a formula with explicit form and text",
                        ));
                    }
                    (
                        read_units(
                            text.ok_or_else(|| json::malformed(&at, "formula text"))?,
                            &at,
                        )?,
                        Some(
                            if matches!(form,Some(Node::Text(name)) if name == "dynamic") {
                                crate::FormulaForm::Dynamic
                            } else {
                                crate::FormulaForm::Legacy
                            },
                        ),
                    )
                } else {
                    (read_units(value, &at)?, None)
                };
                (match form {
                    Some(form) => FormulaText::with_form(text, form),
                    None => FormulaText::new(text),
                })
                .map(CellInput::Formula)
                .map_err(|error| DecodeError::InvalidFormula { at, error })
            }
        }
    }
}

/// The text of a nonzero 128-bit ID as a UUID is written: 32 lowercase hex digits in groups of 8,
/// 4, 4, 4 and 12 joined by `-`.
const ID_FORM: &str =
    "a nonzero ID as 32 lowercase hex digits in groups of 8, 4, 4, 4 and 12 joined by -";

impl WireValue for BlockId {
    fn write(&self, out: &mut String) {
        out.push('"');
        out.push_str(&self.to_string());
        out.push('"');
    }

    fn read(node: &Node, at: &str) -> Result<Self, DecodeError> {
        match node {
            Node::Text(text) => BlockId::parse(text),
            _ => None,
        }
        .ok_or_else(|| json::malformed(at, ID_FORM))
    }
}

impl WireValue for PlacementId {
    fn write(&self, out: &mut String) {
        out.push('"');
        out.push_str(&self.to_string());
        out.push('"');
    }

    fn read(node: &Node, at: &str) -> Result<Self, DecodeError> {
        match node {
            Node::Text(text) => PlacementId::parse(text),
            _ => None,
        }
        .ok_or_else(|| json::malformed(at, ID_FORM))
    }
}

/// A tree placement's parent: a placement ID, or `null` for a root.
impl WireValue for Option<PlacementId> {
    fn write(&self, out: &mut String) {
        match self {
            Some(id) => id.write(out),
            None => out.push_str("null"),
        }
    }

    fn read(node: &Node, at: &str) -> Result<Self, DecodeError> {
        match node {
            Node::Null => Some(None),
            Node::Text(text) => PlacementId::parse(text).map(Some),
            _ => None,
        }
        .ok_or_else(|| json::malformed(at, "null, or a nonzero ID as a UUID is written"))
    }
}

impl WireValue for BlockKind {
    fn write(&self, out: &mut String) {
        out.push('"');
        out.push_str(self.name());
        out.push('"');
    }

    fn read(node: &Node, at: &str) -> Result<Self, DecodeError> {
        match node {
            Node::Text(text) => BlockKind::from_name(text),
            _ => None,
        }
        .ok_or_else(|| json::malformed(at, "\"prose\" or \"calculation\""))
    }
}

/// A number as the 16 lowercase hex digits of its binary64 bits, in a string.
impl WireValue for FiniteNumber {
    fn write(&self, out: &mut String) {
        out.push('"');
        json::push_hex(out, self.to_bits(), 16);
        out.push('"');
    }

    fn read(node: &Node, at: &str) -> Result<Self, DecodeError> {
        read_number(node, at)
    }
}

/// The views a spot object may name, exactly one at a time.
const SPOT_VIEWS: [&str; 4] = ["notebook", "tree", "canvas", "grid"];

fn spot_field<T: WireValue>(args: &Fields<'_>, name: &str) -> Result<T, DecodeError> {
    T::read(args.node(name)?, &args.path(name))
}

/// Appends `{"<view>":{<members>}}`, the members written by `members`.
fn write_spot(out: &mut String, view: &str, members: impl FnOnce(&mut String)) {
    out.push_str("{\"");
    out.push_str(view);
    out.push_str("\":{");
    members(out);
    out.push_str("}}");
}

fn write_member<T: WireValue>(out: &mut String, first: bool, name: &str, value: &T) {
    if !first {
        out.push(',');
    }
    out.push('"');
    out.push_str(name);
    out.push_str("\":");
    value.write(out);
}

/// A spot is an object with exactly one member, named for its view, whose value holds the spot's
/// members in this order: `{"notebook":{"position":<n>}}`,
/// `{"tree":{"parent":<id or null>,"position":<n>}}`,
/// `{"canvas":{"position":<n>,"x":"<bits>","y":"<bits>","width":"<bits>","height":"<bits>"}}`
/// or `{"grid":{"position":<n>}}`.
impl WireValue for Spot {
    fn write(&self, out: &mut String) {
        match self {
            Spot::Notebook { position } => write_spot(out, "notebook", |out| {
                write_member(out, true, "position", position);
            }),
            Spot::Tree { parent, position } => write_spot(out, "tree", |out| {
                write_member(out, true, "parent", parent);
                write_member(out, false, "position", position);
            }),
            Spot::Canvas { position, area } => write_spot(out, "canvas", |out| {
                write_member(out, true, "position", position);
                write_member(out, false, "x", &area.x);
                write_member(out, false, "y", &area.y);
                write_member(out, false, "width", &area.width);
                write_member(out, false, "height", &area.height);
            }),
            Spot::Grid { position } => write_spot(out, "grid", |out| {
                write_member(out, true, "position", position);
            }),
        }
    }

    fn read(node: &Node, at: &str) -> Result<Self, DecodeError> {
        let fields = Fields::new(node, at, &SPOT_VIEWS)?;
        let [(view, value)] = fields.members() else {
            return Err(json::malformed(
                at,
                "exactly one of notebook, tree, canvas or grid",
            ));
        };
        let at = fields.path(view);
        match view.as_str() {
            "notebook" => {
                let args = Fields::new(value, &at, &["position"])?;
                Ok(Spot::Notebook {
                    position: spot_field(&args, "position")?,
                })
            }
            "tree" => {
                let args = Fields::new(value, &at, &["parent", "position"])?;
                Ok(Spot::Tree {
                    parent: spot_field(&args, "parent")?,
                    position: spot_field(&args, "position")?,
                })
            }
            "canvas" => {
                let args = Fields::new(value, &at, &["position", "x", "y", "width", "height"])?;
                Ok(Spot::Canvas {
                    position: spot_field(&args, "position")?,
                    area: CanvasArea {
                        x: spot_field(&args, "x")?,
                        y: spot_field(&args, "y")?,
                        width: spot_field(&args, "width")?,
                        height: spot_field(&args, "height")?,
                    },
                })
            }
            // Fields::new admitted only SPOT_VIEWS, so this view is "grid".
            _ => {
                let args = Fields::new(value, &at, &["position"])?;
                Ok(Spot::Grid {
                    position: spot_field(&args, "position")?,
                })
            }
        }
    }
}
