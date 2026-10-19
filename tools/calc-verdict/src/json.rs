//! The JSON layer of the record codecs and the report writer: a parse into a tree that keeps every
//! object member in order (duplicates included, so the field reader can refuse them), a field
//! reader that admits exactly the declared names, the hex forms of numbers and text, and an
//! output tree written with a fixed member order.

use crate::DecodeError;
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::fmt;
use std::fmt::Write as _;

/// A parsed JSON value. Integers that fit `u64` are kept exactly; any other number is only
/// recorded as present, because no field of the record formats holds one.
///
/// That holds only while serde_json is built without its `arbitrary_precision` feature. With that
/// feature serde_json hands a non-integer number to `visit_map` as a one-member object, which
/// would then be refused as an object with an unknown member instead of as a number.
pub(crate) enum Node {
    Null,
    Bool(bool),
    Unsigned(u64),
    OtherNumber,
    Text(String),
    Array(Vec<Node>),
    Object(Vec<(String, Node)>),
}

impl Node {
    /// What the node is, for refusal messages.
    fn kind(&self) -> &'static str {
        match self {
            Node::Null => "null",
            Node::Bool(_) => "a boolean",
            Node::Unsigned(_) => "an unsigned integer",
            Node::OtherNumber => "a number that is not an unsigned 64-bit integer",
            Node::Text(_) => "a string",
            Node::Array(_) => "an array",
            Node::Object(_) => "an object",
        }
    }
}

struct NodeVisitor;

impl<'de> Visitor<'de> for NodeVisitor {
    type Value = Node;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Node, E> {
        Ok(Node::Bool(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Node, E> {
        Ok(Node::Unsigned(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Node, E> {
        Ok(u64::try_from(value).map_or(Node::OtherNumber, Node::Unsigned))
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Node, E> {
        Ok(Node::OtherNumber)
    }

    fn visit_str<E>(self, value: &str) -> Result<Node, E> {
        Ok(Node::Text(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Node, E> {
        Ok(Node::Text(value))
    }

    fn visit_unit<E>(self) -> Result<Node, E> {
        Ok(Node::Null)
    }

    fn visit_none<E>(self) -> Result<Node, E> {
        Ok(Node::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element::<Node>()? {
            items.push(item);
        }
        Ok(Node::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
        let mut members = Vec::new();
        while let Some((name, value)) = map.next_entry::<String, Node>()? {
            members.push((name, value));
        }
        Ok(Node::Object(members))
    }
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Node, D::Error> {
        deserializer.deserialize_any(NodeVisitor)
    }
}

/// The JSON document in `bytes`.
pub(crate) fn parse(bytes: &[u8], what: &str) -> Result<Node, DecodeError> {
    serde_json::from_slice::<Node>(bytes)
        .map_err(|e| DecodeError::new(what, format!("is not JSON: {e}")))
}

fn refuse(path: &str, node: &Node, expected: &str) -> DecodeError {
    DecodeError::new(path, format!("is {}, expected {expected}", node.kind()))
}

pub(crate) fn text<'a>(node: &'a Node, path: &str) -> Result<&'a str, DecodeError> {
    match node {
        Node::Text(text) => Ok(text),
        other => Err(refuse(path, other, "a string")),
    }
}

/// A non-empty string.
pub(crate) fn name<'a>(node: &'a Node, path: &str) -> Result<&'a str, DecodeError> {
    let value = text(node, path)?;
    if value.is_empty() {
        return Err(DecodeError::new(path, "is an empty string"));
    }
    Ok(value)
}

pub(crate) fn unsigned(node: &Node, path: &str) -> Result<u64, DecodeError> {
    match node {
        Node::Unsigned(value) => Ok(*value),
        other => Err(refuse(path, other, "an unsigned 64-bit integer")),
    }
}

pub(crate) fn boolean(node: &Node, path: &str) -> Result<bool, DecodeError> {
    match node {
        Node::Bool(value) => Ok(*value),
        other => Err(refuse(path, other, "a boolean")),
    }
}

pub(crate) fn array<'a>(node: &'a Node, path: &str) -> Result<&'a [Node], DecodeError> {
    match node {
        Node::Array(items) => Ok(items),
        other => Err(refuse(path, other, "an array")),
    }
}

/// The members of an object, refusing a duplicate member.
pub(crate) fn members<'a>(node: &'a Node, path: &str) -> Result<&'a [(String, Node)], DecodeError> {
    let Node::Object(members) = node else {
        return Err(refuse(path, node, "an object"));
    };
    for (i, (name, _)) in members.iter().enumerate() {
        if members[..i].iter().any(|(earlier, _)| earlier == name) {
            return Err(DecodeError::new(
                path,
                format!("has the member \"{name}\" more than once"),
            ));
        }
    }
    Ok(members)
}

/// An object whose members are exactly some of `allowed`: an unknown or repeated member is
/// refused.
pub(crate) struct Fields<'a> {
    path: String,
    members: &'a [(String, Node)],
}

impl<'a> Fields<'a> {
    pub(crate) fn new(node: &'a Node, path: &str, allowed: &[&str]) -> Result<Self, DecodeError> {
        let members = members(node, path)?;
        if let Some((name, _)) = members
            .iter()
            .find(|(name, _)| !allowed.contains(&name.as_str()))
        {
            return Err(DecodeError::new(
                path,
                format!("has the unknown member \"{name}\" (allowed: {allowed:?})"),
            ));
        }
        Ok(Fields {
            path: path.to_owned(),
            members,
        })
    }

    /// The path of member `name`.
    pub(crate) fn at(&self, name: &str) -> String {
        format!("{}.{name}", self.path)
    }

    pub(crate) fn optional(&self, name: &str) -> Option<&'a Node> {
        self.members
            .iter()
            .find(|(member, _)| member == name)
            .map(|(_, node)| node)
    }

    pub(crate) fn required(&self, name: &str) -> Result<&'a Node, DecodeError> {
        self.optional(name)
            .ok_or_else(|| DecodeError::new(&self.path, format!("has no member \"{name}\"")))
    }
}

/// An object of exactly one member whose name is one of `allowed`: the name and its value.
pub(crate) fn one_member<'a>(
    node: &'a Node,
    path: &str,
    allowed: &[&str],
) -> Result<(&'a str, &'a Node), DecodeError> {
    let members = members(node, path)?;
    match members {
        [(name, value)] if allowed.contains(&name.as_str()) => Ok((name, value)),
        [(name, _)] => Err(DecodeError::new(
            path,
            format!("has the unknown member \"{name}\" (allowed: one of {allowed:?})"),
        )),
        _ => Err(DecodeError::new(
            path,
            format!(
                "has {} members, expected exactly one of {allowed:?}",
                members.len()
            ),
        )),
    }
}

/// The value of exactly 16 lowercase hex digits: the bits of a binary64.
pub(crate) fn parse_bits(text: &str) -> Option<u64> {
    if text.len() != 16 || !text.bytes().all(is_lower_hex) {
        return None;
    }
    u64::from_str_radix(text, 16).ok()
}

/// The UTF-16 code units of a string of 4 lowercase hex digits per unit.
pub(crate) fn parse_units(text: &str) -> Option<Vec<u16>> {
    if !text.len().is_multiple_of(4) || !text.bytes().all(is_lower_hex) {
        return None;
    }
    text.as_bytes()
        .chunks(4)
        .map(|chunk| {
            std::str::from_utf8(chunk)
                .ok()
                .and_then(|digits| u16::from_str_radix(digits, 16).ok())
        })
        .collect()
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

pub(crate) fn bits_text(bits: u64) -> String {
    format!("{bits:016x}")
}

pub(crate) fn units_text(units: &[u16]) -> String {
    let mut out = String::with_capacity(units.len() * 4);
    for unit in units {
        let _ = write!(out, "{unit:04x}");
    }
    out
}

/// A JSON value to write. Object members keep the order they were pushed in.
pub(crate) enum Out {
    Null,
    Bool(bool),
    Unsigned(u64),
    Text(String),
    Array(Vec<Out>),
    Object(Vec<(&'static str, Out)>),
}

impl Out {
    pub(crate) fn text(value: impl Into<String>) -> Out {
        Out::Text(value.into())
    }

    /// The value as a JSON document: an object's members one per line, indented by depth; an
    /// array of objects one element per line, each element written on that one line; anything
    /// else on one line.
    pub(crate) fn document(&self) -> String {
        let mut out = String::new();
        self.write_layout(&mut out, 0);
        out.push('\n');
        out
    }

    /// The value on one line.
    #[cfg(test)]
    pub(crate) fn compact(&self) -> String {
        let mut out = String::new();
        self.write_compact(&mut out);
        out
    }

    fn write_layout(&self, out: &mut String, depth: usize) {
        let indent = |out: &mut String, depth: usize| {
            for _ in 0..depth {
                out.push_str("  ");
            }
        };
        match self {
            Out::Object(members) if !members.is_empty() => {
                out.push_str("{\n");
                for (i, (name, value)) in members.iter().enumerate() {
                    indent(out, depth + 1);
                    write_string(out, name);
                    out.push_str(": ");
                    value.write_layout(out, depth + 1);
                    if i + 1 < members.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                indent(out, depth);
                out.push('}');
            }
            Out::Array(items)
                if !items.is_empty() && items.iter().all(|item| matches!(item, Out::Object(_))) =>
            {
                out.push_str("[\n");
                for (i, item) in items.iter().enumerate() {
                    indent(out, depth + 1);
                    item.write_compact(out);
                    if i + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                indent(out, depth);
                out.push(']');
            }
            other => other.write_compact(out),
        }
    }

    fn write_compact(&self, out: &mut String) {
        match self {
            Out::Null => out.push_str("null"),
            Out::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Out::Unsigned(value) => {
                let _ = write!(out, "{value}");
            }
            Out::Text(value) => write_string(out, value),
            Out::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_compact(out);
                }
                out.push(']');
            }
            Out::Object(members) => {
                out.push('{');
                for (i, (name, value)) in members.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_string(out, name);
                    out.push(':');
                    value.write_compact(out);
                }
                out.push('}');
            }
        }
    }
}

/// `value` as a JSON string: quote, backslash and the control characters escaped, everything else
/// written as it is.
fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn hex_forms_are_exact_and_lowercase() {
        assert_eq!(parse_bits("3ff0000000000000"), Some(1.0f64.to_bits()));
        assert_eq!(parse_bits("8000000000000000"), Some((-0.0f64).to_bits()));
        assert_eq!(parse_bits("3FF0000000000000"), None);
        assert_eq!(parse_bits("3ff000000000000"), None);
        assert_eq!(parse_bits("+3ff000000000000"), None);
        assert_eq!(bits_text(1.0f64.to_bits()), "3ff0000000000000");
        assert_eq!(parse_units(""), Some(vec![]));
        assert_eq!(parse_units("0041d83d"), Some(vec![0x41, 0xd83d]));
        assert_eq!(parse_units("004"), None);
        assert_eq!(parse_units("0041D83D"), None);
        assert_eq!(units_text(&[0x41, 0xd83d]), "0041d83d");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn duplicate_and_unknown_members_are_refused() {
        let node = parse(br#"{"a": 1, "a": 2}"#, "doc").unwrap();
        let error = Fields::new(&node, "doc", &["a"]).err().unwrap();
        assert!(error.to_string().contains("more than once"), "{error}");
        let node = parse(br#"{"a": 1, "b": 2}"#, "doc").unwrap();
        let error = Fields::new(&node, "doc", &["a"]).err().unwrap();
        assert!(
            error.to_string().contains("unknown member \"b\""),
            "{error}"
        );
        let node = parse(br#"{"a": 1.5}"#, "doc").unwrap();
        let fields = Fields::new(&node, "doc", &["a"]).unwrap();
        assert!(unsigned(fields.required("a").unwrap(), "doc.a").is_err());
        assert!(parse(br#"{"a": 1} x"#, "doc").is_err());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn written_documents_parse_back() {
        let out = Out::Object(vec![
            ("text", Out::text("a\"b\\c\n\u{1}é")),
            ("count", Out::Unsigned(u64::MAX)),
            (
                "items",
                Out::Array(vec![
                    Out::Object(vec![("x", Out::Bool(true))]),
                    Out::Object(vec![("y", Out::Null)]),
                ]),
            ),
            ("empty", Out::Object(vec![])),
        ]);
        let document = out.document();
        let value: serde_json::Value = serde_json::from_str(&document).unwrap();
        assert_eq!(value["text"], "a\"b\\c\n\u{1}é");
        assert_eq!(value["count"], u64::MAX);
        assert_eq!(value["items"][0]["x"], true);
        assert!(value["items"][1]["y"].is_null());
        assert!(document.contains("\n    {\"x\":true},\n"), "{document}");
    }
}
