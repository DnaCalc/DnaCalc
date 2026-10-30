//! A text's sort key (bead dc-zsn.1): the second pass of Excel's text comparison
//! ([`super::collation`]) compares two texts' keys byte by byte, ordering them as Windows'
//! `CompareStringEx` with `NORM_IGNORECASE` does under the Windows locale en-ZA.
//!
//! The keys are not Windows' keys: they are built from the Unicode Collation Algorithm's default
//! order (the CLDR root collation, [`super::root_collation`]) and the measured ways Windows
//! departs from it ([`super::windows_collation`]), and they order texts as Windows' keys do. Each
//! code unit adds a [`Weight`]; a key has five sections, each ended by 0x01, as Windows' has:
//!
//! 1. the primary weights of the weighted units, in order, each ten bytes: its Windows group, the
//!    root primary it is placed at (its own, or the one Windows places it after) and its number
//!    there;
//! 2. the diacritic weights, one entry per primary weight, a combining mark adding its weight to
//!    the entry before it, trailing 0x02 entries dropped;
//! 3. the case weights, the same way (under `NORM_IGNORECASE` they hold only width and the like);
//! 4. the kana weights, when the text holds kana: the small-kana weights, 0xFF, 0x02, the
//!    hiragana or katakana weights, 0xFF, the width weights, 0xFF, each list without its trailing
//!    default (0xC6, 0xE4, 0xC5);
//! 5. the special weights (the hyphen, the apostrophe and the like), each after its position:
//!    0xFFFF less half the length Windows' primary weights before it have.
//!
//! A unit's weight is derived by rule from the Unicode data: its root primaries placed in
//! Windows' order; a diacritic entry on each, 2 plus the weights Windows gives the root
//! secondaries on it; a case entry, 3 for a wide form and 2 otherwise; kana weights from whether it
//! is small, katakana or a narrow form; ignorable when it was assigned after Unicode 6.0 or has no
//! root weight; a mark when it has one root secondary and nothing else. The Han characters weigh
//! in code point order (the UCA's implicit weights), the Hangul syllables and the surrogates in
//! code unit order. A unit whose Windows weights the rules do not give is a measured exception.
//!
//! An ignorable unit adds nothing, and so does a supplementary variation selector (U+E0100 to
//! U+E01EF, a surrogate pair). A text holding a refused unit (outside the covered ranges, or one
//! Windows' keys do not decide), a combining mark that does not follow a weighted unit or another
//! mark directly (CompareStringEx then departs from the key's bytes), or combining marks that add up
//! past one byte, has no key here ([`None`]).
//!
//! `cargo xtask collation verify` compares the order these keys give with `CompareStringEx` over
//! random texts of many scripts.

use super::root_collation::{NEW, UNITS};
use super::windows_collation::{
    COVERED, EXCEPTIONS, EXTRA, GROUPS, PLACED, SECONDARY, SPECIAL, SURROGATES,
};
use std::collections::HashMap;
use std::sync::OnceLock;

/// The primary of a Hangul syllable, a surrogate, a Han character and a measured extra primary:
/// the kind in bits 48 and up, after the root primaries' kind 0.
const HANGUL: u64 = 1 << 48;
const SURROGATE: u64 = 2 << 48;
const HAN: u64 = 3 << 48;
const EXTRA_KIND: u64 = 4 << 48;

/// Bytes of one primary weight in a key.
const PRIMARY_BYTES: usize = 10;

/// What one UTF-16 code unit adds to a text's sort key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Weight {
    /// Nothing.
    Ignorable,
    /// A unit the key is not built for here.
    Refused,
    /// A combining mark: its diacritic weight, added to the entry before it.
    Mark(u8),
    /// A special weight, placed by its position.
    Special([u8; 2]),
    /// Primary weights (`primaries` of them at `primary` in [`Table::primary`]), as long in
    /// Windows' keys as `windows_length` bytes, with the unit's diacritic and case entries at
    /// `entries` in [`Table::diacritic`] and [`Table::case`] and, for kana, its small, type and
    /// width weights.
    Weighted {
        primary: u32,
        primaries: u8,
        windows_length: u8,
        entries: u32,
        kana: Option<[u8; 3]>,
    },
}

/// Every code unit's weight, built on first use.
struct Table {
    weights: Vec<Weight>,
    primary: Vec<u8>,
    diacritic: Vec<u8>,
    case: Vec<u8>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(build)
}

/// The bytes of a base64 text (the generated tables' encoding).
fn decode64(text: &str) -> Vec<u8> {
    let mut data = Vec::with_capacity(text.len() / 4 * 3);
    let (mut bits, mut count) = (0u32, 0);
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            data.push((bits >> count) as u8);
        }
    }
    data
}

/// A reader of the generated tables' byte streams.
struct Reader {
    data: Vec<u8>,
    at: usize,
}

impl Reader {
    fn byte(&mut self) -> u8 {
        let byte = self.data[self.at];
        self.at += 1;
        byte
    }

    fn varint(&mut self) -> u64 {
        let (mut value, mut shift) = (0u64, 0);
        loop {
            let byte = self.byte();
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return value;
            }
            shift += 7;
        }
    }

    fn done(&self) -> bool {
        self.at >= self.data.len()
    }
}

/// The Unicode data of one code unit (see root_collation.rs).
struct Root {
    flags: u8,
    primaries: Vec<u64>,
    secondaries: Vec<Vec<u32>>,
}

const NEW_FLAG: u8 = 1;
const WIDE: u8 = 2;
const NARROW: u8 = 4;
const SMALL: u8 = 8;
const KATAKANA: u8 = 16;
const KANA: u8 = 32;

/// A measured exception's weights.
enum Exception {
    Ignorable,
    Refused,
    Mark(u8),
    Special([u8; 2]),
    Weighted {
        primaries: Vec<u64>,
        diacritic: Vec<u8>,
        case: Vec<u8>,
        kana: Option<[u8; 3]>,
    },
}

fn in_ranges(code: u16, ranges: &[(u16, u16)]) -> bool {
    ranges.iter().any(|&(a, b)| (a..=b).contains(&code))
}

fn build() -> Table {
    // The Unicode data.
    let mut roots: HashMap<u16, Root> = HashMap::new();
    let mut reader = Reader {
        data: decode64(UNITS),
        at: 0,
    };
    let mut code = 0u16;
    while !reader.done() {
        code += reader.varint() as u16;
        let flags = reader.byte();
        let count = reader.varint() as usize;
        let primaries: Vec<u64> = (0..count).map(|_| reader.varint()).collect();
        let secondaries: Vec<Vec<u32>> = (0..=count)
            .map(|_| {
                let n = reader.varint() as usize;
                (0..n).map(|_| reader.varint() as u32).collect()
            })
            .collect();
        roots.insert(
            code,
            Root {
                flags,
                primaries,
                secondaries,
            },
        );
    }
    // The measured exceptions.
    let mut exceptions: HashMap<u16, Exception> = HashMap::new();
    let mut reader = Reader {
        data: decode64(EXCEPTIONS),
        at: 0,
    };
    let mut code = 0u16;
    while !reader.done() {
        code += reader.varint() as u16;
        let exception = match reader.byte() {
            0 => Exception::Ignorable,
            1 => Exception::Refused,
            2 => Exception::Mark(reader.byte()),
            3 => Exception::Special([reader.byte(), reader.byte()]),
            _ => {
                let n = reader.varint() as usize;
                let primaries = (0..n).map(|_| reader.varint()).collect();
                let diacritic = (0..n).map(|_| reader.byte()).collect();
                let case = (0..n).map(|_| reader.byte()).collect();
                let kana = match reader.byte() {
                    0 => None,
                    _ => Some([reader.byte(), reader.byte(), reader.byte()]),
                };
                Exception::Weighted {
                    primaries,
                    diacritic,
                    case,
                    kana,
                }
            }
        };
        exceptions.insert(code, exception);
    }
    let placed: HashMap<u64, (u64, u8)> = PLACED.iter().map(|&(t, a, s)| (t, (a, s))).collect();
    let secondary: HashMap<u32, u8> = SECONDARY.iter().copied().collect();
    let special: HashMap<u16, [u8; 2]> = SPECIAL.iter().copied().collect();

    // A primary's ten key bytes and its length in Windows' keys.
    let slot = |token: u64| -> ([u8; PRIMARY_BYTES], u8) {
        let (group, anchor, sub) = if token >= EXTRA_KIND {
            let (group, anchor, sub) = EXTRA[(token - EXTRA_KIND) as usize];
            (group, anchor, sub)
        } else {
            let at = GROUPS.partition_point(|&(start, _)| start <= token);
            let group = GROUPS[at.max(1) - 1].1;
            let (anchor, sub) = placed.get(&token).copied().unwrap_or((token, 0));
            (group, anchor, sub)
        };
        let mut bytes = [0u8; PRIMARY_BYTES];
        bytes[0] = group;
        bytes[1..9].copy_from_slice(&anchor.to_be_bytes());
        bytes[9] = sub;
        let length = match group {
            0xFD => 4,
            0xA9 => 3,
            _ => 2,
        };
        (bytes, length)
    };

    let mut table = Table {
        weights: Vec::with_capacity(0x10000),
        primary: Vec::new(),
        diacritic: Vec::new(),
        case: Vec::new(),
    };
    let weighted = |table: &mut Table,
                    primaries: &[u64],
                    diacritic: &[u8],
                    case: &[u8],
                    kana: Option<[u8; 3]>|
     -> Weight {
        let primary = table.primary.len() as u32;
        let entries = table.diacritic.len() as u32;
        let mut windows_length = 0u8;
        for &token in primaries {
            let (bytes, length) = slot(token);
            table.primary.extend_from_slice(&bytes);
            windows_length += length;
        }
        table.diacritic.extend_from_slice(diacritic);
        table.case.extend_from_slice(case);
        Weight::Weighted {
            primary,
            primaries: primaries.len() as u8,
            windows_length,
            entries,
            kana,
        }
    };
    for point in 0u32..=0xFFFF {
        let code = point as u16;
        let covered = code != 0 && (in_ranges(code, &COVERED) || special.contains_key(&code));
        let weight = if !covered {
            Weight::Refused
        } else if let Some(exception) = exceptions.get(&code) {
            match exception {
                Exception::Ignorable => Weight::Ignorable,
                Exception::Refused => Weight::Refused,
                Exception::Mark(w) => Weight::Mark(*w),
                Exception::Special(w) => Weight::Special(*w),
                Exception::Weighted {
                    primaries,
                    diacritic,
                    case,
                    kana,
                } => weighted(&mut table, primaries, diacritic, case, *kana),
            }
        } else if (0xD800..=0xDFFF).contains(&code) {
            match SURROGATES
                .iter()
                .find(|&&(a, b, _)| (a..=b).contains(&code))
            {
                Some((_, _, Some((d, c)))) => {
                    weighted(&mut table, &[SURROGATE | point as u64], &[*d], &[*c], None)
                }
                Some((_, _, None)) => Weight::Ignorable,
                None => Weight::Refused,
            }
        } else if let Some(w) = special.get(&code) {
            Weight::Special(*w)
        } else if (0xAC00..=0xD7A3).contains(&code) {
            weighted(&mut table, &[HANGUL | point as u64], &[2], &[2], None)
        } else if let Some(base) = han_base(code) {
            if in_ranges(code, &NEW) {
                Weight::Ignorable
            } else {
                weighted(
                    &mut table,
                    &[HAN | (base << 32) | point as u64],
                    &[2],
                    &[2],
                    None,
                )
            }
        } else if let Some(root) = roots.get(&code) {
            derive(root, &secondary).map_or(Weight::Refused, |derived| match derived {
                Derived::Ignorable => Weight::Ignorable,
                Derived::Mark(w) => Weight::Mark(w),
                Derived::Weighted {
                    diacritic,
                    case,
                    kana,
                } => weighted(&mut table, &root.primaries, &diacritic, &case, kana),
            })
        } else {
            Weight::Refused
        };
        table.weights.push(weight);
    }
    table
}

/// The UCA's implicit weight base of a Han character (0 for the URO and the unified
/// compatibility ideographs, 1 for Extension A), each ordered by code point.
fn han_base(code: u16) -> Option<u64> {
    match code {
        0x4E00..=0x9FFF
        | 0xFA0E
        | 0xFA0F
        | 0xFA11
        | 0xFA13
        | 0xFA14
        | 0xFA1F
        | 0xFA21
        | 0xFA23
        | 0xFA24
        | 0xFA27..=0xFA29 => Some(0),
        0x3400..=0x4DBF => Some(1),
        _ => None,
    }
}

/// What the rules derive for a unit from its Unicode data.
enum Derived {
    Ignorable,
    Mark(u8),
    Weighted {
        diacritic: Vec<u8>,
        case: Vec<u8>,
        kana: Option<[u8; 3]>,
    },
}

fn derive(root: &Root, secondary: &HashMap<u32, u8>) -> Option<Derived> {
    if root.flags & NEW_FLAG != 0 {
        return Some(Derived::Ignorable);
    }
    if root.primaries.is_empty() {
        return Some(match root.secondaries.as_slice() {
            [list] if list.len() == 1 => match secondary.get(&list[0]) {
                Some(&w) => Derived::Mark(w),
                None => Derived::Ignorable,
            },
            _ => Derived::Ignorable,
        });
    }
    if !root.secondaries[0].is_empty() {
        return None;
    }
    let mut diacritic = Vec::with_capacity(root.primaries.len());
    for list in &root.secondaries[1..] {
        let mut sum = 2u32;
        for s in list {
            sum += u32::from(*secondary.get(s)?);
        }
        diacritic.push((sum % 256) as u8);
    }
    let case = vec![if root.flags & WIDE != 0 { 3 } else { 2 }; root.primaries.len()];
    let kana = (root.flags & KANA != 0).then(|| {
        [
            if root.flags & SMALL != 0 { 0xC4 } else { 0xC6 },
            if root.flags & KATAKANA != 0 {
                0xC4
            } else {
                0xE4
            },
            if root.flags & NARROW != 0 { 0xC4 } else { 0xC5 },
        ]
    });
    Some(Derived::Weighted {
        diacritic,
        case,
        kana,
    })
}

/// Whether the collation builds no sort key for a text holding the code unit `code`.
pub(super) fn refused(code: u16) -> bool {
    match code {
        0x0001..=0x007F => false,
        _ => matches!(table().weights[usize::from(code)], Weight::Refused),
    }
}

/// The sort key of `text`, or `None` when it holds a refused unit or its marks overflow.
pub(super) fn sort_key(text: &[u16]) -> Option<Vec<u8>> {
    let table = table();
    let mut primary: Vec<u8> = Vec::with_capacity(text.len() * PRIMARY_BYTES);
    // Half the length Windows' primary weights so far have.
    let mut windows_length = 0u32;
    let mut diacritic: Vec<u16> = Vec::new();
    let mut case: Vec<u8> = Vec::new();
    let mut special: Vec<u8> = Vec::new();
    let (mut small, mut kind, mut width): (Vec<u8>, Vec<u8>, Vec<u8>) = Default::default();
    // Whether a combining mark here follows a weighted unit or another mark directly.
    let mut attached = false;
    let mut at = 0;
    while at < text.len() {
        let code = text[at];
        if (0xD800..=0xDBFF).contains(&code)
            && let Some(&low) = text.get(at + 1)
            && (0xDC00..=0xDFFF).contains(&low)
        {
            let point = 0x10000 + ((u32::from(code) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
            if (0xE0100..=0xE01EF).contains(&point) {
                at += 2;
                continue;
            }
        }
        match table.weights[usize::from(code)] {
            Weight::Refused => return None,
            Weight::Ignorable => attached = false,
            // A mark that does not follow a weighted unit or another mark directly (one at the
            // start, or after an ignorable or special unit) is in Windows' key, but
            // CompareStringEx does not order it as the key's bytes would: not decided here.
            Weight::Mark(added) => {
                if !attached {
                    return None;
                }
                let last = diacritic.last_mut()?;
                *last += u16::from(added);
                if *last > 0xFF {
                    return None;
                }
            }
            Weight::Special([w0, w1]) => {
                attached = false;
                let position = 0xFFFF - (windows_length / 2) as u16;
                special.extend_from_slice(&position.to_be_bytes());
                special.extend_from_slice(&[w0, w1]);
            }
            Weight::Weighted {
                primary: start,
                primaries,
                windows_length: length,
                entries,
                kana,
            } => {
                attached = true;
                let start = start as usize;
                primary.extend_from_slice(
                    &table.primary[start..start + usize::from(primaries) * PRIMARY_BYTES],
                );
                windows_length += u32::from(length);
                let entries = entries as usize;
                let n = usize::from(primaries);
                diacritic.extend(
                    table.diacritic[entries..entries + n]
                        .iter()
                        .map(|&entry| u16::from(entry)),
                );
                case.extend_from_slice(&table.case[entries..entries + n]);
                if let Some([s, t, w]) = kana {
                    small.push(s);
                    kind.push(t);
                    width.push(w);
                }
            }
        }
        at += 1;
    }
    let trim = |list: &mut Vec<u8>, default: u8| {
        while list.last() == Some(&default) {
            list.pop();
        }
    };
    while diacritic.last() == Some(&2) {
        diacritic.pop();
    }
    trim(&mut case, 2);
    let mut key = primary;
    key.push(1);
    key.extend(diacritic.iter().map(|&entry| entry as u8));
    key.push(1);
    key.extend_from_slice(&case);
    key.push(1);
    if !small.is_empty() {
        trim(&mut small, 0xC6);
        trim(&mut kind, 0xE4);
        trim(&mut width, 0xC5);
        key.extend_from_slice(&small);
        key.extend_from_slice(&[0xFF, 0x02]);
        key.extend_from_slice(&kind);
        key.push(0xFF);
        key.extend_from_slice(&width);
        key.push(0xFF);
    }
    key.push(1);
    key.extend_from_slice(&special);
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn order(a: &str, b: &str) -> Option<Ordering> {
        Some(sort_key(&units(a))?.cmp(&sort_key(&units(b))?))
    }

    /// Orders CompareStringEx gave on the generating machine (en-ZA, NORM_IGNORECASE).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn keys_order_as_windows_orders() {
        for (a, b, expected) in [
            ("e", "\u{e9}", Ordering::Less),
            ("\u{e9}", "e\u{301}", Ordering::Equal),
            ("\u{e9}", "f", Ordering::Less),
            ("a-b", "ab", Ordering::Greater),
            ("a-b", "ac", Ordering::Less),
            ("\u{e6}", "ae", Ordering::Equal),
            ("z", "\u{3b1}", Ordering::Less),
            ("\u{3b1}", "\u{430}", Ordering::Less),
            ("\u{430}", "\u{4e00}", Ordering::Less),
            ("\u{4e00}", "\u{4e01}", Ordering::Less),
            ("\u{ff37}", "w", Ordering::Greater),
            ("\u{3042}", "\u{30a2}", Ordering::Greater),
            ("a\u{ad}b", "ab", Ordering::Equal),
            ("\u{1f600}", "\u{1f601}", Ordering::Less),
        ] {
            assert_eq!(order(a, b), Some(expected), "{a:?} against {b:?}");
        }
        assert_eq!(
            sort_key(&units("\u{1100}\u{1161}")),
            None,
            "conjoining jamo"
        );
        assert_eq!(
            sort_key(&units("\u{2190}")),
            None,
            "an arrow, outside the covered ranges"
        );
    }
}
