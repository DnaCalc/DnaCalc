//! The JSON layer of the wire codec: a parse into a tree that keeps every object member in order
//! (duplicates included, so the field reader can refuse them), a reader for one member that runs
//! before an object's other members are checked, a field reader that admits exactly the declared
//! names, and the hex forms of numbers and text.

use super::DecodeError;
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::fmt;

/// A parsed JSON value. Integers that fit `u64` are kept exactly; any other number is only
/// recorded as present, because no field of the wire format holds one.
///
/// That holds only while serde_json is built without its `arbitrary_precision` feature, in this
/// package and in any build whose features unify with it. With that feature, serde_json 1.0.154
/// still hands an integer that fits `u64` or `i64` (`-0` apart) to `visit_u64` or `visit_i64`, but
/// hands any other number (`1.5`, `1e2`, an integer past `u64`) to `visit_map` as a one-member
/// object, which [`NodeVisitor`] records as [`Node::Object`]: such a number where an object
/// belongs would then be refused as an unknown field instead of as malformed. The acceptance
/// case's refusal of a fractional number in place of an operation's arguments fails if that ever
/// changes.
pub(crate) enum Node {
    Null,
    Bool(bool),
    Unsigned(u64),
    OtherNumber,
    Text(String),
    Array(Vec<Node>),
    Object(Vec<(String, Node)>),
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

    fn visit_seq<A: SeqAccess<'de>>(self, mut items: A) -> Result<Node, A::Error> {
        let mut out = Vec::new();
        while let Some(item) = items.next_element()? {
            out.push(item);
        }
        Ok(Node::Array(out))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut members: A) -> Result<Node, A::Error> {
        let mut out = Vec::new();
        while let Some(member) = members.next_entry::<String, Node>()? {
            out.push(member);
        }
        Ok(Node::Object(out))
    }
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(NodeVisitor)
    }
}

/// The one JSON value of `bytes`; trailing text, invalid UTF-8 and nesting deeper than
/// serde_json's limit are refused.
pub(crate) fn parse(bytes: &[u8]) -> Result<Node, DecodeError> {
    serde_json::from_slice(bytes).map_err(|error| DecodeError::Syntax {
        detail: error.to_string(),
    })
}

pub(crate) fn malformed(at: &str, expected: &'static str) -> DecodeError {
    DecodeError::Malformed {
        at: at.to_owned(),
        expected,
    }
}

/// The path of the member `name` of the object at `at`.
pub(crate) fn path(at: &str, name: &str) -> String {
    format!("{at}.{name}")
}

fn as_text<'a>(node: &'a Node, at: &str) -> Result<&'a str, DecodeError> {
    match node {
        Node::Text(text) => Ok(text),
        _ => Err(malformed(at, "a string")),
    }
}

fn as_unsigned(node: &Node, at: &str) -> Result<u64, DecodeError> {
    match node {
        Node::Unsigned(value) => Ok(*value),
        _ => Err(malformed(at, "an integer from 0 to 18446744073709551615")),
    }
}

/// The member `name` of the object `node`, read before the object's other members are checked,
/// so that a format or a version is judged before the members that version would declare. Refused
/// when `node` is not an object or when `name` is absent or appears more than once.
pub(crate) fn lone_member<'a>(
    node: &'a Node,
    at: &str,
    name: &str,
) -> Result<&'a Node, DecodeError> {
    let Node::Object(members) = node else {
        return Err(malformed(at, "an object"));
    };
    let mut found = members
        .iter()
        .filter(|(member, _)| member == name)
        .map(|(_, value)| value);
    let Some(value) = found.next() else {
        return Err(DecodeError::MissingField {
            at: at.to_owned(),
            field: name.to_owned(),
        });
    };
    if found.next().is_some() {
        return Err(DecodeError::DuplicateField {
            at: at.to_owned(),
            field: name.to_owned(),
        });
    }
    Ok(value)
}

/// The string member `name` of the object `node`, read as [`lone_member`] reads it.
pub(crate) fn lone_text<'a>(node: &'a Node, at: &str, name: &str) -> Result<&'a str, DecodeError> {
    as_text(lone_member(node, at, name)?, &path(at, name))
}

/// The integer member `name` of the object `node`, read as [`lone_member`] reads it.
pub(crate) fn lone_unsigned(node: &Node, at: &str, name: &str) -> Result<u64, DecodeError> {
    as_unsigned(lone_member(node, at, name)?, &path(at, name))
}

/// The members of one object, checked against its declared field names: every member is
/// declared and appears once. A declared field that is absent is refused when it is read.
pub(crate) struct Fields<'a> {
    at: &'a str,
    members: &'a [(String, Node)],
}

impl<'a> Fields<'a> {
    pub(crate) fn new(node: &'a Node, at: &'a str, declared: &[&str]) -> Result<Self, DecodeError> {
        let Node::Object(members) = node else {
            return Err(malformed(at, "an object"));
        };
        for (index, (name, _)) in members.iter().enumerate() {
            if !declared.contains(&name.as_str()) {
                return Err(DecodeError::UnknownField {
                    at: at.to_owned(),
                    field: name.clone(),
                });
            }
            if members[..index].iter().any(|(earlier, _)| earlier == name) {
                return Err(DecodeError::DuplicateField {
                    at: at.to_owned(),
                    field: name.clone(),
                });
            }
        }
        Ok(Fields { at, members })
    }

    /// The members in message order.
    pub(crate) fn members(&self) -> &'a [(String, Node)] {
        self.members
    }

    pub(crate) fn path(&self, name: &str) -> String {
        path(self.at, name)
    }

    pub(crate) fn node(&self, name: &str) -> Result<&'a Node, DecodeError> {
        self.members
            .iter()
            .find(|(member, _)| member == name)
            .map(|(_, value)| value)
            .ok_or_else(|| DecodeError::MissingField {
                at: self.at.to_owned(),
                field: name.to_owned(),
            })
    }

    pub(crate) fn text(&self, name: &str) -> Result<&'a str, DecodeError> {
        as_text(self.node(name)?, &self.path(name))
    }

    pub(crate) fn unsigned(&self, name: &str) -> Result<u64, DecodeError> {
        as_unsigned(self.node(name)?, &self.path(name))
    }
}

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Appends the decimal form of `value`.
pub(crate) fn push_unsigned(out: &mut String, value: u64) {
    out.push_str(&value.to_string());
}

/// Appends the low `digits` hex digits of `value`, most significant first, in lowercase.
pub(crate) fn push_hex(out: &mut String, value: u64, digits: u32) {
    for place in (0..digits).rev() {
        let nibble = (value >> (place * 4)) & 0xF;
        out.push(char::from(HEX_DIGITS[nibble as usize]));
    }
}

/// The value of a run of lowercase hex digits that fits `u64`, or `None` for any other text.
pub(crate) fn parse_hex(text: &str) -> Option<u64> {
    if text.is_empty() || text.len() > 16 {
        return None;
    }
    text.bytes().try_fold(0u64, |value, byte| {
        let nibble = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => return None,
        };
        Some((value << 4) | u64::from(nibble))
    })
}

/// Appends 4 lowercase hex digits per UTF-16 code unit.
pub(crate) fn push_units(out: &mut String, units: &[u16]) {
    for unit in units {
        push_hex(out, u64::from(*unit), 4);
    }
}

/// The code units of a string of 4 lowercase hex digits each, or `None` for any other text.
pub(crate) fn parse_units(text: &str) -> Option<Vec<u16>> {
    if !text.len().is_multiple_of(4) {
        return None;
    }
    text.as_bytes()
        .chunks(4)
        .map(|chunk| {
            let digits = std::str::from_utf8(chunk).ok()?;
            parse_hex(digits).and_then(|unit| u16::try_from(unit).ok())
        })
        .collect()
}
