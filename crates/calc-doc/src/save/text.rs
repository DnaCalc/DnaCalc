//! Writing SpreadsheetML text: element content as ECMA-376's ST_Xstring holds it, numbers as a
//! cell's `<v>` holds them, and start tags rebuilt from a part's own bytes so every attribute and
//! namespace declaration the save does not change keeps its source text.

/// Whether `unit` is an ASCII hexadecimal digit, as the loader's `_xHHHH_` decoding reads one.
fn hex_digit(unit: u16) -> bool {
    matches!(unit, 0x30..=0x39 | 0x41..=0x46 | 0x61..=0x66)
}

/// Appends `_xHHHH_` for one UTF-16 code unit.
fn escape_unit(unit: u16, out: &mut Vec<u8>) {
    out.extend_from_slice(format!("_x{unit:04X}_").as_bytes());
}

/// Appends `units` as element content of type ST_Xstring: `&`, `<` and `>` as entities; as an
/// `_xHHHH_` escape each code unit XML 1.0 cannot hold (controls other than tab and line feed,
/// U+FFFE, U+FFFF and unpaired surrogates) and the carriage return, which an XML reader would turn
/// into a line feed; and the underscore that starts text a reader would decode as such an escape,
/// as `_x005F_`. The loader's decoding gives back exactly `units`.
pub(crate) fn xstring(units: &[u16], out: &mut Vec<u8>) {
    let mut index = 0;
    for decoded in char::decode_utf16(units.iter().copied()) {
        let width = decoded.as_ref().map_or(1, |c| c.len_utf16());
        match decoded {
            Err(unpaired) => escape_unit(unpaired.unpaired_surrogate(), out),
            Ok('_')
                if units.len() - index >= 7
                    && units[index + 1] == u16::from(b'x')
                    && units[index + 2..index + 6].iter().all(|&u| hex_digit(u))
                    && units[index + 6] == u16::from(b'_') =>
            {
                escape_unit(0x5F, out)
            }
            Ok('&') => out.extend_from_slice(b"&amp;"),
            Ok('<') => out.extend_from_slice(b"&lt;"),
            Ok('>') => out.extend_from_slice(b"&gt;"),
            Ok(c @ ('\t' | '\n')) => out.push(c as u8),
            Ok(c) if c < ' ' || c == '\u{7F}' || c == '\u{FFFE}' || c == '\u{FFFF}' => {
                escape_unit(c as u16, out)
            }
            Ok(c) => {
                let mut buffer = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
            }
        }
        index += width;
    }
}

/// SpreadsheetML ST_Xstring as a quoted XML attribute, preserving escape-looking text and whitespace.
pub(crate) fn xstring_attribute(units: &[u16], out: &mut Vec<u8>) {
    let mut encoded = Vec::new();
    xstring(units, &mut encoded);
    out.push(b'"');
    for byte in encoded {
        match byte {
            b'"' => out.extend_from_slice(b"&quot;"),
            b'\t' => out.extend_from_slice(b"_x0009_"),
            b'\n' => out.extend_from_slice(b"_x000A_"),
            _ => out.push(byte),
        }
    }
    out.push(b'"');
}

/// Whether text written in a `<t>` element needs `xml:space="preserve"`: it starts or ends with
/// whitespace a reader that collapses it would drop.
pub(crate) fn needs_preserve(units: &[u16]) -> bool {
    let space = |unit: &u16| matches!(*unit, 0x20 | 0x09 | 0x0A | 0x0D);
    units.first().is_some_and(space) || units.last().is_some_and(space)
}

/// Appends an attribute value, quoted with `"`.
pub(crate) fn attribute_value(value: &str, out: &mut Vec<u8>) {
    out.push(b'"');
    for c in value.chars() {
        match c {
            '&' => out.extend_from_slice(b"&amp;"),
            '<' => out.extend_from_slice(b"&lt;"),
            '"' => out.extend_from_slice(b"&quot;"),
            c => {
                let mut buffer = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    out.push(b'"');
}

/// A finite number as a cell's `<v>` holds it: the shortest text that reads back as the same
/// binary64, positional from 1E-5 up to 1E+15 and otherwise with an upper-case `E` and a signed
/// exponent (`1E+16`, `1.5E-7`), which the loader reads back as the same binary64.
pub(crate) fn number(value: f64) -> String {
    let magnitude = value.abs();
    if value == 0.0 || (1e-5..1e15).contains(&magnitude) {
        return format!("{value}");
    }
    let text = format!("{value:e}");
    let (mantissa, exponent) = text.split_once('e').expect("an exponent");
    match exponent.strip_prefix('-') {
        Some(negative) => format!("{mantissa}E-{negative}"),
        None => format!("{mantissa}E+{exponent}"),
    }
}

/// An `A1` address.
pub(crate) fn a1(row: u32, column: u32) -> String {
    format!("{}{row}", crate::load::address::column_letters(column))
}

/// One attribute of a start tag as the part wrote it.
struct RawAttribute<'a> {
    /// The name as written, prefix included.
    name: &'a [u8],
    /// The value with its quotes, as written.
    quoted: &'a [u8],
}

/// A start tag as the part wrote it: its name (prefix included), its attributes and whether it
/// closes itself (`/>`). It is only rebuilt, never read for values: an attribute's value is read
/// from the XML reader's element, with its references replaced, as the loader reads it.
pub(super) struct RawTag<'a> {
    name: &'a [u8],
    attributes: Vec<RawAttribute<'a>>,
    empty: bool,
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

impl<'a> RawTag<'a> {
    /// Splits a start tag the XML reader has already accepted, so it is well formed: `<`, a name,
    /// attributes whose values are quoted, and `>` or `/>`.
    pub(super) fn parse(tag: &'a [u8]) -> Self {
        let end = tag.len() - if tag.ends_with(b"/>") { 2 } else { 1 };
        let empty = tag.ends_with(b"/>");
        let mut at = 1;
        while at < end && !is_space(tag[at]) {
            at += 1;
        }
        let name = &tag[1..at];
        let mut attributes = Vec::new();
        loop {
            while at < end && is_space(tag[at]) {
                at += 1;
            }
            if at >= end {
                break;
            }
            let start = at;
            while tag[at] != b'=' && !is_space(tag[at]) {
                at += 1;
            }
            let attribute = &tag[start..at];
            while tag[at] != b'=' {
                at += 1;
            }
            at += 1;
            while is_space(tag[at]) {
                at += 1;
            }
            let quote = tag[at];
            let value_start = at;
            at += 1;
            while tag[at] != quote {
                at += 1;
            }
            at += 1;
            attributes.push(RawAttribute {
                name: attribute,
                quoted: &tag[value_start..at],
            });
        }
        RawTag {
            name,
            attributes,
            empty,
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.empty
    }

    /// Whether the tag has the attribute `name` (as written, prefix included).
    pub(super) fn has(&self, name: &str) -> bool {
        self.attributes
            .iter()
            .any(|attribute| attribute.name == name.as_bytes())
    }

    /// The name's prefix with its colon (`x:`), or nothing for an unprefixed name: what an element
    /// written beside or inside this one uses for the same namespace.
    pub(super) fn prefix(&self) -> &'a [u8] {
        match self.name.iter().position(|&byte| byte == b':') {
            Some(colon) => &self.name[..=colon],
            None => &[],
        }
    }

    /// The tag rebuilt: each attribute named in `changes` set to its value (first among the
    /// attributes when the tag did not have it) or, for `None`, removed; every other attribute and
    /// namespace declaration as the part wrote it. `empty` says whether the tag closes itself.
    pub(super) fn rebuilt(&self, changes: &[(&str, Option<&str>)], empty: bool) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        out.push(b'<');
        out.extend_from_slice(self.name);
        for (name, value) in changes {
            if let Some(value) = value
                && !self.attributes.iter().any(|a| a.name == name.as_bytes())
            {
                out.push(b' ');
                out.extend_from_slice(name.as_bytes());
                out.push(b'=');
                attribute_value(value, &mut out);
            }
        }
        for attribute in &self.attributes {
            let change = changes
                .iter()
                .find(|(name, _)| attribute.name == name.as_bytes());
            match change {
                Some((_, None)) => continue,
                Some((name, Some(value))) => {
                    out.push(b' ');
                    out.extend_from_slice(name.as_bytes());
                    out.push(b'=');
                    attribute_value(value, &mut out);
                }
                None => {
                    out.push(b' ');
                    out.extend_from_slice(attribute.name);
                    out.push(b'=');
                    out.extend_from_slice(attribute.quoted);
                }
            }
        }
        out.extend_from_slice(if empty { b"/>" } else { b">" });
        out
    }

    /// Replace one ST_Xstring attribute while preserving every other source attribute.
    pub(super) fn rebuilt_xstring(&self, name: &str, units: &[u16], empty: bool) -> Vec<u8> {
        let mut out = self.rebuilt(&[(name, None)], false);
        out.pop();
        out.push(b' ');
        out.extend_from_slice(name.as_bytes());
        out.push(b'=');
        xstring_attribute(units, &mut out);
        out.extend_from_slice(if empty { b"/>" } else { b">" });
        out
    }
}

/// A part's new bytes made from its source bytes: ranges of the source kept as they are, with
/// new bytes put between them and source ranges skipped.
pub(super) struct Splice<'a> {
    source: &'a [u8],
    cursor: usize,
    pub(super) out: Vec<u8>,
}

impl<'a> Splice<'a> {
    pub(super) fn new(source: &'a [u8]) -> Self {
        Splice {
            source,
            cursor: 0,
            out: Vec::with_capacity(source.len() + source.len() / 8),
        }
    }

    pub(super) fn source(&self) -> &'a [u8] {
        self.source
    }

    /// Keeps the source bytes from the cursor up to `at`.
    pub(super) fn keep_to(&mut self, at: usize) {
        self.out.extend_from_slice(&self.source[self.cursor..at]);
        self.cursor = at;
    }

    /// Drops the source bytes from the cursor up to `at`.
    pub(super) fn skip_to(&mut self, at: usize) {
        debug_assert!(at >= self.cursor);
        self.cursor = at;
    }

    /// Replaces the source bytes `range` (at or after the cursor) with `bytes`.
    pub(super) fn replace(&mut self, range: (usize, usize), bytes: &[u8]) {
        self.keep_to(range.0);
        self.out.extend_from_slice(bytes);
        self.skip_to(range.1);
    }

    pub(super) fn put(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }

    /// The new bytes, with the rest of the source kept.
    pub(super) fn finish(mut self) -> Vec<u8> {
        self.keep_to(self.source.len());
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::strings::decode;

    fn written(text: &[u16]) -> String {
        let mut out = Vec::new();
        xstring(text, &mut out);
        String::from_utf8(out).expect("UTF-8")
    }

    /// Escaped text decodes, as the loader decodes it, to exactly the code units written.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn xstring_round_trips() {
        let cases: Vec<Vec<u16>> = vec![
            "a<b>&c \"d\"".encode_utf16().collect(),
            "_x0041_ and _x12G4_ and _x12_".encode_utf16().collect(),
            "a\rb\nc\td\u{1}e\u{7F}\u{FFFE}".encode_utf16().collect(),
            vec![0x61, 0xD800, 0x62, 0xDC00],
            "\u{1F600}".encode_utf16().collect(),
        ];
        for units in cases {
            let text = written(&units);
            assert!(!text.contains('\r'), "{text:?}");
            // An XML reader replaces the five entities before the loader decodes the escapes.
            let read = text
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&");
            assert_eq!(decode(&read), units, "{text:?}");
        }
        assert_eq!(
            written(&"_x0041_".encode_utf16().collect::<Vec<_>>()),
            "_x005F_x0041_"
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn numbers() {
        for (value, text) in [
            (0.1, "0.1"),
            (-2.5, "-2.5"),
            (123456789.0, "123456789"),
            (1e15, "1E+15"),
            (1e308, "1E+308"),
            (6.02214076e23, "6.02214076E+23"),
            (1.5e-7, "1.5E-7"),
            (0.00001, "0.00001"),
            (0.30000000000000004, "0.30000000000000004"),
            (5e-324, "5E-324"),
        ] {
            assert_eq!(number(value), text);
            assert_eq!(text.parse::<f64>().unwrap().to_bits(), value.to_bits());
        }
        assert_eq!(number(-0.0), "-0");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn tags() {
        let tag = RawTag::parse(br#"<x:row r="3" spans='1:8'  x14ac:dyDescent="0.25"/>"#);
        assert_eq!(tag.prefix(), b"x:");
        assert!(tag.is_empty());
        assert_eq!(
            String::from_utf8(tag.rebuilt(&[("spans", None), ("r", Some("3"))], false)).unwrap(),
            r#"<x:row r="3" x14ac:dyDescent="0.25">"#
        );
        let cell = RawTag::parse(br#"<c s="2" t = "s">"#);
        assert_eq!(cell.prefix(), b"");
        assert_eq!(
            String::from_utf8(cell.rebuilt(&[("r", Some("B7")), ("t", None)], true)).unwrap(),
            r#"<c r="B7" s="2"/>"#
        );
    }
}
