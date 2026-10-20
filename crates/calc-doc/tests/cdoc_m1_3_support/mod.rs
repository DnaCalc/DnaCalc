//! Instruments for the cdoc-m1.3 cases, independent of calc-doc's own code: a DEFLATE decoder
//! written here from RFC 1951 (calc-doc's reader decodes with miniz_oxide and its writer encodes
//! with it), and an XML parser written here that keeps each element's byte range, so a case reads
//! a saved part's elements and compares their bytes with the source's without calc-doc's loader or
//! XML reader.

// ---------------------------------------------------------------------------------------------
// DEFLATE (RFC 1951), decoded bit by bit.

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// The order the code length code lengths are sent in.
const CODE_LENGTH_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    bit: u32,
}

impl Bits<'_> {
    fn bit(&mut self) -> u32 {
        let byte = *self
            .data
            .get(self.at)
            .expect("the stream ends inside a block");
        let value = u32::from(byte >> self.bit) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.at += 1;
        }
        value
    }

    /// `count` bits, least significant first.
    fn bits(&mut self, count: u32) -> u32 {
        (0..count).fold(0, |value, index| value | (self.bit() << index))
    }

    fn align(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.at += 1;
        }
    }
}

/// A canonical Huffman code: how many codes each length has, and the symbols in code order.
struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Self {
        let mut counts = [0u16; 16];
        for &length in lengths {
            counts[usize::from(length)] += 1;
        }
        counts[0] = 0;
        let mut offsets = [0u16; 16];
        for length in 1..15 {
            offsets[length + 1] = offsets[length] + counts[length];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length != 0 {
                symbols[usize::from(offsets[usize::from(length)])] = symbol as u16;
                offsets[usize::from(length)] += 1;
            }
        }
        Huffman { counts, symbols }
    }

    fn decode(&self, bits: &mut Bits<'_>) -> u16 {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for length in 1..16 {
            code |= bits.bit() as i32;
            let count = i32::from(self.counts[length]);
            if code - first < count {
                return self.symbols[(index + code - first) as usize];
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        panic!("a code that is not in the Huffman code");
    }
}

/// The bytes a raw DEFLATE stream decodes to.
pub fn inflate(data: &[u8]) -> Vec<u8> {
    let mut bits = Bits {
        data,
        at: 0,
        bit: 0,
    };
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = bits.bits(1) == 1;
        match bits.bits(2) {
            0 => {
                bits.align();
                let at = bits.at;
                let length = usize::from(u16::from_le_bytes([data[at], data[at + 1]]));
                let complement = u16::from_le_bytes([data[at + 2], data[at + 3]]);
                assert_eq!(length as u16, !complement, "a stored block's length check");
                out.extend_from_slice(&data[at + 4..at + 4 + length]);
                bits.at = at + 4 + length;
            }
            kind @ (1 | 2) => {
                let (literals, distances) = if kind == 1 {
                    let mut lengths = [0u8; 288];
                    for (symbol, length) in lengths.iter_mut().enumerate() {
                        *length = match symbol {
                            0..=143 => 8,
                            144..=255 => 9,
                            256..=279 => 7,
                            _ => 8,
                        };
                    }
                    (Huffman::new(&lengths), Huffman::new(&[5u8; 30]))
                } else {
                    let literal_count = bits.bits(5) as usize + 257;
                    let distance_count = bits.bits(5) as usize + 1;
                    let code_count = bits.bits(4) as usize + 4;
                    let mut code_lengths = [0u8; 19];
                    for &symbol in &CODE_LENGTH_ORDER[..code_count] {
                        code_lengths[symbol] = bits.bits(3) as u8;
                    }
                    let code = Huffman::new(&code_lengths);
                    let mut lengths = Vec::with_capacity(literal_count + distance_count);
                    while lengths.len() < literal_count + distance_count {
                        match code.decode(&mut bits) {
                            symbol @ 0..=15 => lengths.push(symbol as u8),
                            16 => {
                                let previous = *lengths.last().expect("a length to repeat");
                                let times = 3 + bits.bits(2);
                                lengths.extend((0..times).map(|_| previous));
                            }
                            17 => {
                                let times = 3 + bits.bits(3);
                                lengths.extend((0..times).map(|_| 0));
                            }
                            _ => {
                                let times = 11 + bits.bits(7);
                                lengths.extend((0..times).map(|_| 0));
                            }
                        }
                    }
                    (
                        Huffman::new(&lengths[..literal_count]),
                        Huffman::new(&lengths[literal_count..]),
                    )
                };
                loop {
                    let symbol = literals.decode(&mut bits);
                    match symbol {
                        0..=255 => out.push(symbol as u8),
                        256 => break,
                        _ => {
                            let index = usize::from(symbol - 257);
                            let length = usize::from(LENGTH_BASE[index])
                                + bits.bits(u32::from(LENGTH_EXTRA[index])) as usize;
                            let index = usize::from(distances.decode(&mut bits));
                            let distance = usize::from(DISTANCE_BASE[index])
                                + bits.bits(u32::from(DISTANCE_EXTRA[index])) as usize;
                            assert!(distance <= out.len(), "a distance before the output");
                            for _ in 0..length {
                                out.push(out[out.len() - distance]);
                            }
                        }
                    }
                }
            }
            _ => panic!("a block of the reserved type 3"),
        }
        if last {
            return out;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// XML: elements with their byte ranges.

/// One element: its name as written (prefix included), its attributes as (name as written,
/// value with entities replaced), its children, and the byte ranges of its start tag and of the
/// whole element.
#[derive(Clone, Debug)]
pub struct Element {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub start_tag: (usize, usize),
    pub span: (usize, usize),
}

#[derive(Clone, Debug)]
pub enum Node {
    Element(Element),
    Text(String),
}

impl Element {
    /// The name without its prefix.
    pub fn local(&self) -> &str {
        self.name.rsplit(':').next().unwrap_or(&self.name)
    }

    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|child| match child {
            Node::Element(element) => Some(element),
            Node::Text(_) => None,
        })
    }

    /// The child elements of local name `local`.
    pub fn all<'a>(&'a self, local: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.elements()
            .filter(move |element| element.local() == local)
    }

    pub fn child(&self, local: &str) -> Option<&Element> {
        self.elements().find(|element| element.local() == local)
    }

    /// The element's text, its children's included.
    pub fn text(&self) -> String {
        let mut text = String::new();
        for child in &self.children {
            match child {
                Node::Text(more) => text.push_str(more),
                Node::Element(element) => text.push_str(&element.text()),
            }
        }
        text
    }

    /// The bytes of the whole element in the part it was read from.
    pub fn bytes<'a>(&self, part: &'a [u8]) -> &'a [u8] {
        &part[self.span.0..self.span.1]
    }
}

fn entities(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let end = rest[at..].find(';').expect("an entity ends with ;") + at;
        let name = &rest[at + 1..end];
        match name {
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "amp" => out.push('&'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let code = match name.strip_prefix("#x") {
                    Some(hex) => u32::from_str_radix(hex, 16),
                    None => name
                        .strip_prefix('#')
                        .expect("a character reference")
                        .parse(),
                }
                .expect("a character reference's number");
                out.push(char::from_u32(code).expect("a character"));
            }
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Parses a start tag's text (between `<` and `>` or `/>`) into its name and attributes.
fn tag(text: &str) -> (String, Vec<(String, String)>) {
    let text = text.trim_end();
    let name_end = text
        .find(|c: char| c.is_ascii_whitespace())
        .unwrap_or(text.len());
    let name = text[..name_end].to_owned();
    let mut attributes = Vec::new();
    let mut rest = text[name_end..].trim_start();
    while !rest.is_empty() {
        let equals = rest.find('=').expect("an attribute's =");
        let key = rest[..equals].trim().to_owned();
        let after = rest[equals + 1..].trim_start();
        let quote = after.chars().next().expect("a quoted value");
        assert!(quote == '"' || quote == '\'', "a quoted value: {after}");
        let close = after[1..].find(quote).expect("a closing quote") + 1;
        attributes.push((key, entities(&after[1..close])));
        rest = after[close + 1..].trim_start();
    }
    (name, attributes)
}

/// Parses a part into its document element, asserting it is well formed: tags nest and close
/// with their names, and no document type declaration appears. A leading byte order mark is
/// skipped; every byte range counts from the part's first byte.
pub fn parse_xml(bytes: &[u8]) -> Element {
    let text = std::str::from_utf8(bytes).expect("a UTF-8 part");
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    let mut at = if text.starts_with('\u{FEFF}') {
        '\u{FEFF}'.len_utf8()
    } else {
        0
    };
    while at < text.len() {
        if !text[at..].starts_with('<') {
            let end = text[at..]
                .find('<')
                .map_or(text.len(), |offset| at + offset);
            let content = entities(&text[at..end]);
            match stack.last_mut() {
                Some(open) => open.children.push(Node::Text(content)),
                None => assert!(content.trim().is_empty(), "text outside the root"),
            }
            at = end;
            continue;
        }
        let rest = &text[at..];
        if rest.starts_with("<?") {
            at += rest.find("?>").expect("a declaration's end") + 2;
        } else if rest.starts_with("<!--") {
            at += rest.find("-->").expect("a comment's end") + 3;
        } else if let Some(data) = rest.strip_prefix("<![CDATA[") {
            let end = data.find("]]>").expect("a CDATA end");
            stack
                .last_mut()
                .expect("CDATA inside an element")
                .children
                .push(Node::Text(data[..end].to_owned()));
            at += 9 + end + 3;
        } else if rest.starts_with("<!") {
            panic!("a document type declaration");
        } else if let Some(closing) = rest.strip_prefix("</") {
            let end = closing.find('>').expect("an end tag's >");
            let name = closing[..end].trim();
            let mut element = stack.pop().expect("an open element");
            assert_eq!(element.name, name, "an end tag closes its element");
            at += 2 + end + 1;
            element.span.1 = at;
            match stack.last_mut() {
                Some(parent) => parent.children.push(Node::Element(element)),
                None => root = Some(element),
            }
        } else {
            let mut quote = None;
            let mut end = 1;
            for (offset, c) in rest.char_indices().skip(1) {
                match (quote, c) {
                    (None, '"' | '\'') => quote = Some(c),
                    (Some(open), c) if c == open => quote = None,
                    (None, '>') => {
                        end = offset;
                        break;
                    }
                    _ => {}
                }
            }
            let empty = rest[..end].ends_with('/');
            let inner = &rest[1..if empty { end - 1 } else { end }];
            let (name, attributes) = tag(inner);
            let start_tag = (at, at + end + 1);
            at += end + 1;
            let element = Element {
                name,
                attributes,
                children: Vec::new(),
                start_tag,
                span: (start_tag.0, if empty { at } else { 0 }),
            };
            if empty {
                match stack.last_mut() {
                    Some(parent) => parent.children.push(Node::Element(element)),
                    None => root = Some(element),
                }
            } else {
                stack.push(element);
            }
        }
    }
    assert!(stack.is_empty(), "every element is closed");
    root.expect("a document element")
}

/// Text with each `_xHHHH_` escape of ECMA-376's ST_Xstring replaced by the UTF-16 code unit it
/// names, written here from that description.
pub fn xstring_units(text: &str) -> Vec<u16> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < units.len() {
        let escape = units.get(at..at + 7).and_then(|window| {
            let digits = String::from_utf16(&window[2..6]).ok()?;
            (window[0] == u16::from(b'_')
                && window[1] == u16::from(b'x')
                && window[6] == u16::from(b'_')
                && digits.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| u16::from_str_radix(&digits, 16).expect("hexadecimal"))
        });
        match escape {
            Some(unit) => {
                out.push(unit);
                at += 7;
            }
            None => {
                out.push(units[at]);
                at += 1;
            }
        }
    }
    out
}
