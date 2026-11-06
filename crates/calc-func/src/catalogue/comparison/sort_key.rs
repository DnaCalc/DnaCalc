//! A text's collation weights (beads dc-zsn.1 and dc-zsn.2): the second pass of Excel's text
//! comparison ([`super::collation`]) orders two texts by them as Windows' `CompareStringEx` with
//! `NORM_IGNORECASE` does under the Windows locale en-ZA.
//!
//! The weights are not Windows' weights: they are built from the Unicode Collation Algorithm's
//! default order (the CLDR root collation, [`super::root_collation`]) and the measured ways Windows
//! departs from it ([`super::windows_collation`]), and they order texts as Windows does. Each code
//! unit adds a [`Weight`]. A text's [`Parts`] are three of the five sections of a Windows sort key,
//! and the steps of its diacritic walk:
//!
//! 1. the primary weights of the weighted units, in order, each ten bytes: its Windows group, the
//!    root primary it is placed at (its own, or the one Windows places it after) and its number
//!    there;
//! 2. the diacritic level, which `CompareStringEx` walks rather than reading the key's sums of
//!    marks ([`diacritic_order`]): each unit's entries (one per primary weight; one of 0x02 for a
//!    private-use character, whose key holds none), marks, specials and ignorable units in order;
//! 3. the case weights, trailing 0x02 entries dropped (under `NORM_IGNORECASE` they hold only width
//!    and the like), then 0x01;
//! 4. the kana weights, when the text holds kana: the small-kana weights, 0xFF, the iteration
//!    weights, 0x02, the hiragana or katakana weights, 0xFF, the width weights, 0xFF, each list
//!    without its trailing default (0xC6, 0x03, 0xE4, 0xC5), then 0x01;
//! 5. the special weights (the hyphen, the apostrophe and the like), each after its position:
//!    0xFFFF less half the length Windows' primary weights before it have.
//!
//! A unit's weight is derived by rule from the Unicode data: its root primaries placed in
//! Windows' order; a diacritic entry on each, 2 plus the weights Windows gives the root
//! secondaries on it; a case entry, 3 for a wide form and 2 otherwise; kana weights from whether it
//! is small, katakana or a narrow form; ignorable when it was assigned after Unicode 6.0 or has no
//! root weight; a mark when it has one root secondary and nothing else. The Han characters weigh
//! in code point order (the UCA's implicit weights), the Hangul syllables and the surrogates in
//! code unit order. A unit whose Windows weights the rules do not give is a measured exception:
//! its weights, or the units `CompareStringEx` compares it as (a composite whose own key is
//! malformed, such as U+0385 or U+309F), or an iteration or prolonged-sound mark (U+3005, U+309D,
//! U+30FC and the like), which takes the primary of the unit before it.
//!
//! An ignorable unit adds no weight, and so does a supplementary variation selector (U+E0100 to
//! U+E01EF, a surrogate pair). A text has incomplete weights here when it holds U+0000, a
//! Hangul conjoining jamo (whose context-dependent continuations are not fully modelled), U+30A0 (which
//! `CompareStringEx` orders otherwise than its key), an isolated or medial form of Arabic marks
//! (U+FC5E to U+FC63, U+FE71 and the like, into which the walk's runs are not modelled), or an
//! iteration mark after a special outside fn7's three adjacent forms and four punctuation units.
//! After Han, the measured REPEAT primary is used instead of the character's own primary;
//! fn1 confirms this rule with 時, 人 and 漢, their suffixes and lookup/sort controls.
//!
//! `cargo xtask collation verify` compares the order these weights give with `CompareStringEx`
//! over random texts of many scripts.

use super::root_collation::{NEW, UNITS};
use super::windows_collation::{
    COVERED, EXCEPTIONS, EXTRA, GROUPS, NO_REPEAT, PLACED, REPEAT, SECONDARY, SPECIAL, SURROGATES,
    VOWEL,
};
use std::cmp::Ordering;
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
    /// Only the first primary group is measured, after a simple ordinary prefix
    /// and before an independent suffix. This cannot give a same-group order.
    FirstPrimaryGroup(u8),
    /// A combining mark: its diacritic weight, added to the entry before it.
    Mark(u8),
    /// A special weight, placed by its position.
    Special([u8; 2]),
    /// A unit compared as the `len` units at `at` in [`Table::expansions`]: its whole
    /// decomposition, or (not `whole`) its compatibility decomposition without the leading space.
    Expansion { at: u32, len: u8, whole: bool },
    /// An iteration or prolonged-sound mark.
    Iteration(Iteration),
    /// Primary weights (`primaries` of them at `primary` in [`Table::primary`]), as long in
    /// Windows' keys as `windows_length` bytes, with the unit's diacritic and case entries at
    /// `entries` in [`Table::diacritic`] and [`Table::case`] and, for kana, its small, type and
    /// width weights.
    Weighted {
        primary: u32,
        primaries: u8,
        windows_length: u8,
        diacritic: u32,
        diacritics: u8,
        case: u32,
        cases: u8,
        kana: Option<[u8; 3]>,
    },
}

/// An iteration or prolonged-sound mark: the primary of the unit before it (for a prolonged-sound
/// mark after a kana, its vowel's), its own diacritic entry and iteration weight, and its own
/// width weight or, when `None`, the one of the unit before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Iteration {
    vowel: bool,
    diacritic: u8,
    iteration: u8,
    width: Option<u8>,
}

/// Every code unit's weight, built on first use.
struct Table {
    weights: Vec<Weight>,
    primary: Vec<u8>,
    diacritic: Vec<u8>,
    case: Vec<u8>,
    /// The units of the expansions.
    expansions: Vec<u16>,
    /// The primaries an iteration mark takes (see windows_collation.rs): REPEAT's ranges, VOWEL's
    /// units and NO_REPEAT, each as key bytes and Windows length.
    repeat: Vec<(u16, u16, Slot)>,
    vowel: HashMap<u16, Slot>,
    no_repeat: Slot,
}

/// A primary's key bytes and its length in Windows' keys.
type Slot = ([u8; PRIMARY_BYTES], u8);

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
    Expansion(Vec<u16>, bool),
    Iteration(Iteration),
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
            5 => {
                let whole = reader.byte() == 1;
                let n = reader.varint() as usize;
                Exception::Expansion((0..n).map(|_| reader.varint() as u16).collect(), whole)
            }
            6 => Exception::Iteration(Iteration {
                vowel: reader.byte() == 1,
                diacritic: reader.byte(),
                iteration: reader.byte(),
                width: match reader.byte() {
                    0 => None,
                    _ => Some(reader.byte()),
                },
            }),
            _ => {
                let n = reader.varint() as usize;
                let primaries = (0..n).map(|_| reader.varint()).collect();
                let n = reader.varint() as usize;
                let diacritic = (0..n).map(|_| reader.byte()).collect();
                let n = reader.varint() as usize;
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
    let slot = |token: u64| -> Slot {
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
        expansions: Vec::new(),
        repeat: REPEAT.iter().map(|&(a, b, t)| (a, b, slot(t))).collect(),
        vowel: VOWEL.iter().map(|&(c, t)| (c, slot(t))).collect(),
        no_repeat: slot(NO_REPEAT),
    };
    let weighted = |table: &mut Table,
                    primaries: &[u64],
                    diacritic: &[u8],
                    case: &[u8],
                    kana: Option<[u8; 3]>|
     -> Weight {
        let primary = table.primary.len() as u32;
        let diacritic_at = table.diacritic.len() as u32;
        let case_at = table.case.len() as u32;
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
            diacritic: diacritic_at,
            diacritics: diacritic.len() as u8,
            case: case_at,
            cases: case.len() as u8,
            kana,
        }
    };
    for point in 0u32..=0xFFFF {
        let code = point as u16;
        let covered = code != 0 && (in_ranges(code, &COVERED) || special.contains_key(&code));
        let weight = if !covered {
            Weight::Refused
        } else if matches!(code, 0x1100 | 0x1161) {
            // Fn7's bare Giyeok/A native keys start C0/EC respectively.
            // Their remaining primary bytes and contextual runs stay unknown.
            Weight::FirstPrimaryGroup(if code == 0x1100 { 0xC0 } else { 0xEC })
        } else if let Some(exception) = exceptions.get(&code) {
            match exception {
                Exception::Ignorable => Weight::Ignorable,
                Exception::Refused => Weight::Refused,
                Exception::Mark(w) => Weight::Mark(*w),
                Exception::Special(w) => Weight::Special(*w),
                // An isolated form of marks compared as the marks (its decomposition without the
                // leading space): the walk's runs into it are not modelled.
                Exception::Expansion(_, false) => Weight::Refused,
                Exception::Expansion(units, whole) => {
                    let at = table.expansions.len() as u32;
                    table.expansions.extend_from_slice(units);
                    Weight::Expansion {
                        at,
                        len: units.len() as u8,
                        whole: *whole,
                    }
                }
                Exception::Iteration(rule) => Weight::Iteration(*rule),
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

/// Whether a unit has no measured comparison route. Partial Jamo groups reach
/// `parts`, which still refuses a comparison needing their unknown bytes.
pub(super) fn refused(code: u16) -> bool {
    match code {
        0x0001..=0x007F => false,
        _ => matches!(table().weights[usize::from(code)], Weight::Refused),
    }
}

/// Fn6's identical modern LV/LVT prefixes have default diacritic and case
/// entries, as fn7's native keys confirm, including after an identical ordinary
/// prefix. Their common weights cancel when
/// neither suffix can extend a Jamo sequence or repeat the preceding primary.
/// This gives no ordering between different Jamo prefixes or between a Jamo
/// sequence and a precomposed syllable.
pub(super) fn shared_jamo_tails<'a>(a: &'a [u16], b: &'a [u16]) -> Option<(usize, &'a [u16], &'a [u16])> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| {
        x == y && ordinary_jamo_prefix(std::slice::from_ref(*x))
    }).count();
    let (a, b) = (&a[prefix..], &b[prefix..]);
    if a.len() < 2 || b.len() < 2 || a[..2] != b[..2]
        || !(0x1100..=0x1112).contains(&a[0]) || !(0x1161..=0x1175).contains(&a[1])
    {
        return None;
    }
    let at = if a.get(2) == b.get(2) && a.get(2).is_some_and(|c| (0x11A8..=0x11C2).contains(c)) {
        3
    } else {
        2
    };
    let (a, b) = (&a[at..], &b[at..]);
    (independent_jamo_suffix(a) && independent_jamo_suffix(b)).then_some((prefix, a, b))
}

/// Simple known entries neither join a Jamo run nor alter its boundary.
/// Keep marks, expansions, specials and ignorables outside this prefix rule.
fn ordinary_jamo_prefix(text: &[u16]) -> bool {
    text.iter().all(|&code| {
        // Paired surrogates can be variation selectors. Hangul syllables and
        // compatibility forms have unmeasured boundaries with conjoining Jamo.
        !matches!(code, 0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F
            | 0xAC00..=0xDFFF | 0xFFA0..=0xFFDC)
            && matches!(table().weights[usize::from(code)],
                Weight::Weighted { primaries: 1, diacritics: 1, cases: 1, .. })
    })
}

/// These units cannot extend a Jamo run or repeat its preceding primary.
fn independent_jamo_suffix(text: &[u16]) -> bool {
    text.iter().all(|&code| matches!(
        table().weights[usize::from(code)],
        Weight::Ignorable | Weight::Mark(_) | Weight::Special(_) | Weight::Weighted { .. }
    ))
}

/// Fn7's three leading consonants start with the same primary pair as their
/// compatibility consonant, before the unmodelled Jamo bytes. This settles a
/// strict primary difference, not an equality with that compatibility form.
/// Native keys and CompareStringEx agree on the measured LV/LVT boundaries;
/// their disagreement for Giyeok versus its compatibility form stays refused.
fn jamo_leading_primary<'a>(table: &'a Table, text: &[u16], at: usize) -> Option<&'a [u8]> {
    let (compatibility, tail) = match text[at] {
        0x1100 => (0x3131, &text[at + 1..]),
        0x1101 => (0x3132, &text[at + 1..]),
        0x1102 => (0x3134, &text[at + 1..]),
        _ => return None,
    };
    if !ordinary_jamo_prefix(&text[..at]) {
        return None;
    }
    let tail = if tail.first() == Some(&0x1161) {
        let tail = &tail[1..];
        if text[at] == 0x1100 && tail.first() == Some(&0x11A8) { &tail[1..] } else { tail }
    } else if text[at] == 0x1100
        && matches!(tail, [0x00AD | 0x034F | 0x200B | 0x0301, 0x1161, ..])
    {
        &tail[2..]
    } else if text[at] == 0x1100 {
        tail
    } else {
        return None;
    };
    if !independent_jamo_suffix(tail) {
        return None;
    }
    match table.weights[compatibility] {
        Weight::Weighted { primary, primaries: 1, .. } => {
            Some(&table.primary[primary as usize..primary as usize + PRIMARY_BYTES])
        }
        _ => None,
    }
}

/// One step of a text's diacritic walk ([`diacritics`]): what each unit gives the diacritic
/// level, in text order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    /// A weighted unit's diacritic entry (one per entry; one of 0x02 for a unit that has none).
    Base(u8),
    /// A combining mark's diacritic weight.
    Mark(u8),
    /// A special unit (the code unit).
    Special(u16),
    /// An ignorable unit: the walk passes it, but it ends a run of marks.
    Ignorable,
    /// The start of an expansion: the walk passes it; it ends a run of marks after a mark, not
    /// after an entry.
    Expansion,
}

/// A text's collation weights: the sort key's primary section, its diacritic walk, and the key's
/// case, kana and special sections (each ended by 0x01 but the last).
pub(super) struct Parts {
    pub(super) primary: Vec<u8>,
    pub(super) steps: Vec<Step>,
    pub(super) rest: Vec<u8>,
    /// Where each code unit's steps start, and after the last unit, the count of steps.
    starts: Vec<u32>,
}

/// A known primary prefix, possibly ending at the measured group byte of an
/// incomplete unit. A guaranteed further primary permits an earlier difference
/// or completed prefix to decide without assigning the remaining unknown weight.
pub(super) struct Unfinished {
    primary: Vec<u8>,
    continuation: Continuation,
}

enum Continuation {
    /// The failed unit may change an earlier primary too.
    Unknown,
    /// The primary prefix is known; later primary or secondary weights are not.
    MayEnd,
    /// More primary bytes must follow the known prefix.
    More,
}

/// An order decided entirely before either unknown primary, or by a completed primary prefix.
/// No diacritic, case or special level can reverse a strict primary order.
pub(super) fn leading_order(
    left: &Result<Parts, Unfinished>,
    right: &Result<Parts, Unfinished>,
) -> Option<Ordering> {
    fn prefix(parts: &Result<Parts, Unfinished>) -> Option<(&[u8], bool, bool)> {
        match parts {
            Ok(parts) => Some((parts.primary.as_slice(), true, false)),
            Err(rest) => match rest.continuation {
                Continuation::More => Some((rest.primary.as_slice(), false, true)),
                Continuation::MayEnd => Some((rest.primary.as_slice(), false, false)),
                Continuation::Unknown => None,
            },
        }
    }
    let (a, a_complete, a_continues) = prefix(left)?;
    let (b, b_complete, b_continues) = prefix(right)?;
    let common = a.len().min(b.len());
    let earlier = a[..common].cmp(&b[..common]);
    if !earlier.is_eq() {
        return Some(earlier);
    }
    if a_complete && (a.len() < b.len() || (a.len() == b.len() && b_continues)) {
        Some(Ordering::Less)
    } else if b_complete && (b.len() < a.len() || (a.len() == b.len() && a_continues)) {
        Some(Ordering::Greater)
    } else {
        None
    }
}

/// What an iteration mark repeats: the last unit before it that is not a mark, an ignorable unit
/// or another iteration mark.
#[derive(Clone, Copy)]
enum Before {
    /// Nothing, or a unit of several primaries (or none).
    Nothing,
    /// A unit of one primary: the unit, its primary and its kana weights.
    One(u16, Slot, Option<[u8; 3]>),
    /// The punctuation unit, its special bytes, and whether it is immediately before us.
    Special(u16, [u8; 2], bool),
}

impl Before {
    fn separate_special(&mut self) {
        if let Self::Special(_, _, adjacent) = self {
            *adjacent = false;
        }
    }
}

/// The weights of a text as they are gathered.
#[derive(Default)]
struct Gathered {
    primary: Vec<u8>,
    // Half the length Windows' primary weights so far have.
    windows_length: u32,
    steps: Vec<Step>,
    case: Vec<u8>,
    special: Vec<u8>,
    small: Vec<u8>,
    iteration: Vec<u8>,
    kind: Vec<u8>,
    width: Vec<u8>,
    /// Whether the units being added are an expansion's.
    expanding: bool,
}

impl Gathered {
    fn kana(&mut self, [small, kind, width]: [u8; 3], iteration: u8) {
        self.small.push(small);
        self.iteration.push(iteration);
        self.kind.push(kind);
        self.width.push(width);
    }

    /// Adds one code unit; `None` when it is refused.
    fn unit(&mut self, table: &Table, code: u16, before: &mut Before) -> Option<()> {
        if matches!(table.weights[usize::from(code)], Weight::Ignorable | Weight::Mark(_)) {
            before.separate_special();
        }
        match table.weights[usize::from(code)] {
            Weight::Refused | Weight::FirstPrimaryGroup(_) => return None,
            Weight::Ignorable if self.expanding => {}
            Weight::Ignorable => self.steps.push(Step::Ignorable),
            Weight::Mark(added) => self.steps.push(Step::Mark(added)),
            Weight::Special([w0, w1]) => {
                self.steps.push(Step::Special(code));
                let position = 0xFFFF - (self.windows_length / 2) as u16;
                self.special.extend_from_slice(&position.to_be_bytes());
                self.special.extend_from_slice(&[w0, w1]);
                *before = Before::Special(code, [w0, w1], true);
            }
            Weight::Expansion { at, len, whole } => {
                let at = at as usize;
                let primaries = self.primary.len();
                // Marks run on into a decomposition after an entry, not after a mark, and an
                // ignorable unit in it does not end them (measured).
                debug_assert!(whole);
                self.steps.push(Step::Expansion);
                let expanding = std::mem::replace(&mut self.expanding, true);
                for &unit in &table.expansions[at..at + usize::from(len)] {
                    self.unit(table, unit, before)?;
                }
                self.expanding = expanding;
                // An iteration mark after a unit that expands to several primaries takes FF FF.
                if self.primary.len() - primaries > PRIMARY_BYTES {
                    *before = Before::Nothing;
                }
            }
            Weight::Iteration(rule) => {
                let ((bytes, length), kana) = match *before {
                    Before::Nothing => (table.no_repeat, None),
                    Before::Special(punctuation, [weight, _], adjacent) => {
                        // Fn7's native keys and Excel comparisons agree here: an
                        // iteration after these punctuation units has primary
                        // 06 followed by the punctuation's first special byte.
                        // This is before every ordinary primary group. Keep the
                        // position-dependent special itself in the final level.
                        if !adjacent || !matches!(code, 0x3005 | 0x309D | 0x30FC)
                            || !matches!(punctuation, 0x0027 | 0x002D | 0x2010 | 0x2212)
                        {
                            return None;
                        }
                        let mut bytes = [0; PRIMARY_BYTES];
                        bytes[0] = 0x06;
                        bytes[1] = weight;
                        ((bytes, 2), None)
                    }
                    Before::One(unit, own, kana) => {
                        let vowel = rule.vowel.then(|| table.vowel.get(&unit)).flatten();
                        let repeat = || {
                            let at = table.repeat.partition_point(|&(start, _, _)| start <= unit);
                            at.checked_sub(1)
                                .map(|at| table.repeat[at])
                                .filter(|&(_, end, _)| unit <= end)
                                .map(|(_, _, slot)| slot)
                        };
                        // A Han character's four-byte primary repeats as the separately measured
                        // REPEAT slot, not as its own primary. Fn1's three Han bases and their
                        // lookup/sort controls confirm this existing measurement.
                        (vowel.copied().or_else(repeat).unwrap_or(own), kana)
                    }
                };
                self.primary.extend_from_slice(&bytes);
                self.windows_length += u32::from(length);
                // With nothing before it, the mark's own diacritic entry is not in the key.
                let diacritic = match *before {
                    Before::Nothing => 2,
                    _ => rule.diacritic,
                };
                self.steps.push(Step::Base(diacritic));
                self.case.push(2);
                if let Some([small, kind, width]) = kana {
                    self.kana([small, kind, rule.width.unwrap_or(width)], rule.iteration);
                }
                before.separate_special();
            }
            Weight::Weighted {
                primary: start,
                primaries,
                windows_length: length,
                diacritic: diacritic_at,
                diacritics,
                case: case_at,
                cases,
                kana,
            } => {
                let start = start as usize;
                let bytes = &table.primary[start..start + usize::from(primaries) * PRIMARY_BYTES];
                self.primary.extend_from_slice(bytes);
                self.windows_length += u32::from(length);
                let (d, nd) = (diacritic_at as usize, usize::from(diacritics));
                if nd == 0 {
                    self.steps.push(Step::Base(2));
                }
                self.steps
                    .extend(table.diacritic[d..d + nd].iter().map(|&w| Step::Base(w)));
                let (c, nc) = (case_at as usize, usize::from(cases));
                self.case.extend_from_slice(&table.case[c..c + nc]);
                if let Some(weights) = kana {
                    self.kana(weights, 3);
                }
                *before = if primaries == 1 {
                    let mut slot = [0u8; PRIMARY_BYTES];
                    slot.copy_from_slice(bytes);
                    Before::One(code, (slot, length), kana)
                } else {
                    Before::Nothing
                };
            }
        }
        Some(())
    }
}

/// The collation weights of `text`, or the primary prefix before an unmodelled unit.
pub(super) fn parts(text: &[u16]) -> Result<Parts, Unfinished> {
    let table = table();
    let mut gathered = Gathered::default();
    let mut before = Before::Nothing;
    let mut starts: Vec<u32> = Vec::with_capacity(text.len() + 1);
    let mut at = 0;
    while at < text.len() {
        let code = text[at];
        starts.push(gathered.steps.len() as u32);
        if (0xD800..=0xDBFF).contains(&code)
            && let Some(&low) = text.get(at + 1)
            && (0xDC00..=0xDFFF).contains(&low)
        {
            let point = 0x10000 + ((u32::from(code) - 0xD800) << 10) + (u32::from(low) - 0xDC00);
            if (0xE0100..=0xE01EF).contains(&point) {
                gathered.steps.push(Step::Ignorable);
                before.separate_special();
                starts.push(gathered.steps.len() as u32);
                at += 2;
                continue;
            }
        }
        let primaries = gathered.primary.len();
        if gathered.unit(table, code, &mut before).is_none() {
            gathered.primary.truncate(primaries);
            if let Some(primary) = jamo_leading_primary(table, text, at) {
                gathered.primary.extend_from_slice(primary);
                return Err(Unfinished { primary: gathered.primary, continuation: Continuation::MayEnd });
            }
            // Fn7's exact bare reversed A/Giyeok pair keeps A's first EC byte.
            // No later byte, ordinary-prefix context or continuation is assigned.
            let bare_reversed_jamo = at == 0 && text == [0x1161, 0x1100];
            if ordinary_jamo_prefix(&text[..at])
                && (independent_jamo_suffix(&text[at + 1..]) || bare_reversed_jamo)
                && let Weight::FirstPrimaryGroup(group) = table.weights[usize::from(code)]
            {
                // A strict difference in this measured byte decides before any
                // unknown byte or later level. Keep the rest explicitly unfinished.
                gathered.primary.push(group);
                return Err(Unfinished { primary: gathered.primary, continuation: Continuation::More });
            }
            return Err(Unfinished {
                primary: gathered.primary,
                // An iteration failure after One would still add a primary. After a special
                // even a nonempty contribution has not been measured; leave it undecided.
                continuation: if matches!(table.weights[usize::from(code)], Weight::Iteration(_))
                    && matches!(before, Before::One(..))
                {
                    Continuation::More
                } else {
                    Continuation::Unknown
                },
            });
        }
        at += 1;
    }
    starts.push(gathered.steps.len() as u32);
    let trim = |list: &mut Vec<u8>, default: u8| {
        while list.last() == Some(&default) {
            list.pop();
        }
    };
    let Gathered {
        primary,
        steps,
        mut case,
        special,
        mut small,
        mut iteration,
        mut kind,
        mut width,
        ..
    } = gathered;
    trim(&mut case, 2);
    let mut rest = case;
    rest.push(1);
    if !small.is_empty() {
        trim(&mut small, 0xC6);
        trim(&mut iteration, 3);
        trim(&mut kind, 0xE4);
        trim(&mut width, 0xC5);
        rest.extend_from_slice(&small);
        rest.push(0xFF);
        rest.extend_from_slice(&iteration);
        rest.push(0x02);
        rest.extend_from_slice(&kind);
        rest.push(0xFF);
        rest.extend_from_slice(&width);
        rest.push(0xFF);
    }
    rest.push(1);
    rest.extend_from_slice(&special);
    Ok(Parts {
        primary,
        steps,
        rest,
        starts,
    })
}

/// The diacritic level of two texts whose primary weights are the same (see [`diacritics`]):
/// `CompareStringEx` first passes the code units the texts start with in common, passing an
/// ignorable unit on either side, and walks the rest.
pub(super) fn diacritic_order(a: &[u16], left: &Parts, b: &[u16], right: &Parts) -> Ordering {
    let ignorable = |parts: &Parts, at: usize| {
        let range = parts.starts[at] as usize..parts.starts[at + 1] as usize;
        parts.steps[range]
            .iter()
            .all(|step| matches!(step, Step::Ignorable | Step::Expansion))
    };
    let (mut i, mut j) = (0, 0);
    loop {
        while i < a.len() && ignorable(left, i) {
            i += 1;
        }
        while j < b.len() && ignorable(right, j) {
            j += 1;
        }
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
        } else {
            break;
        }
    }
    diacritics(
        &left.steps[left.starts[i] as usize..],
        &right.steps[right.starts[j] as usize..],
    )
}

/// The diacritic level of what follows two texts' common start, as `CompareStringEx` walks it
/// (measured; Windows' sort keys hold a sum of marks per entry instead, and so order a few texts
/// otherwise). Both are walked in step, an ignorable unit passed on its own side. Two equal steps
/// pass. Two different entries, or two different marks, compare as sums: each with the marks that
/// follow it directly, added in a byte. A mark against a special passes the special once the walk
/// has passed a special (alone, or two different ones); otherwise, and against anything else, the
/// mark makes its text the greater. Two different specials pass every special and ignorable unit
/// that follows them on both sides; a special against anything else is passed on its own side. The first difference decides; when one side runs out, a mark left in the
/// other makes that one the greater.
fn diacritics(a: &[Step], b: &[Step]) -> Ordering {
    let passed = |steps: &[Step], mut at: usize| {
        while matches!(
            steps.get(at),
            Some(Step::Special(_) | Step::Ignorable | Step::Expansion)
        ) {
            at += 1;
        }
        at
    };
    // The sum of `first` and the marks from `at` on (passing an expansion's start after an
    // entry), and where they end.
    fn run(steps: &[Step], mut at: usize, first: u8, entry: bool) -> (u8, usize) {
        let mut sum = first;
        loop {
            match steps.get(at) {
                Some(Step::Mark(w)) => sum = sum.wrapping_add(*w),
                Some(Step::Expansion) if entry => {}
                _ => return (sum, at),
            }
            at += 1;
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut result = Ordering::Equal;
    let mut decide = |order: Ordering| {
        if result.is_eq() {
            result = order;
        }
    };
    // Whether the walk has passed a special (alone, or two different ones).
    let mut special_passed = false;
    loop {
        while matches!(a.get(i), Some(Step::Ignorable | Step::Expansion)) {
            i += 1;
        }
        while matches!(b.get(j), Some(Step::Ignorable | Step::Expansion)) {
            j += 1;
        }
        let (Some(&x), Some(&y)) = (a.get(i), b.get(j)) else {
            break;
        };
        match (x, y) {
            _ if x == y => {
                i += 1;
                j += 1;
            }
            (Step::Mark(w), Step::Mark(v)) | (Step::Base(w), Step::Base(v)) => {
                let entry = matches!(x, Step::Base(_));
                let (left, next_i) = run(a, i + 1, w, entry);
                let (right, next_j) = run(b, j + 1, v, entry);
                decide(left.cmp(&right));
                i = next_i;
                j = next_j;
            }
            (Step::Mark(_), Step::Special(_)) if special_passed => j += 1,
            (Step::Special(_), Step::Mark(_)) if special_passed => i += 1,
            (Step::Mark(_), _) => {
                decide(Ordering::Greater);
                i += 1;
            }
            (_, Step::Mark(_)) => {
                decide(Ordering::Less);
                j += 1;
            }
            (Step::Special(_), Step::Special(_)) => {
                special_passed = true;
                i = passed(a, i);
                j = passed(b, j);
            }
            (Step::Special(_), _) => {
                special_passed = true;
                i += 1;
            }
            (_, Step::Special(_)) => {
                special_passed = true;
                j += 1;
            }
            _ => {
                i += 1;
                j += 1;
            }
        }
    }
    if a[i.min(a.len())..]
        .iter()
        .any(|s| matches!(s, Step::Mark(_)))
    {
        decide(Ordering::Greater);
    }
    if b[j.min(b.len())..]
        .iter()
        .any(|s| matches!(s, Step::Mark(_)))
    {
        decide(Ordering::Less);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn order(a: &str, b: &str) -> Option<Ordering> {
        let (a, b) = (units(a), units(b));
        let (left, right) = (parts(&a).ok()?, parts(&b).ok()?);
        Some(
            left.primary
                .cmp(&right.primary)
                .then_with(|| diacritic_order(&a, &left, &b, &right))
                .then_with(|| left.rest.cmp(&right.rest)),
        )
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
            ("\u{2191}", "\u{2190}", Ordering::Less),
            ("\u{221a}", "\u{2602}", Ordering::Less),
            ("\u{e000}", "\u{e001}", Ordering::Less),
            ("\u{3131}", "\u{ac00}", Ordering::Less),
            // The diacritic walk, where Windows' keys would order otherwise.
            ("\u{301}a", "a\u{301}", Ordering::Greater),
            ("-\u{301}", "\u{301}", Ordering::Less),
            ("b-\u{301}a", "b\u{301}a", Ordering::Less),
            ("b\u{301}\u{ad}\u{301}", "b\u{308}", Ordering::Less),
            ("B\u{301}\u{ad}\u{301}", "b\u{301}\u{301}", Ordering::Equal),
            ("a\u{e000}\u{301}", "a\u{301}\u{e000}", Ordering::Less),
            // Iteration and prolonged-sound marks, and units compared as their decomposition.
            ("\u{304b}\u{309d}", "\u{304b}\u{304b}", Ordering::Greater),
            ("\u{304b}\u{309e}", "\u{304b}\u{304c}", Ordering::Greater),
            ("\u{30ab}\u{30fc}", "\u{30ab}\u{30a2}", Ordering::Greater),
            ("\u{ff76}\u{ff70}", "\u{30ab}\u{30fc}", Ordering::Less),
            ("\u{309f}", "\u{3088}\u{308a}", Ordering::Equal),
            ("\u{30ff}", "\u{30b3}\u{30c8}", Ordering::Equal),
            ("\u{385}", "\u{a8}\u{301}", Ordering::Equal),
            ("\u{f73}", "\u{f71}\u{f72}", Ordering::Equal),
            ("\u{fb1d}", "\u{5d9}\u{5b4}", Ordering::Equal),
            ("\u{1200}", "\u{10a0}", Ordering::Greater),
            ("\u{a000}", "\u{1780}", Ordering::Greater),
        ] {
            assert_eq!(order(a, b), Some(expected), "{a:?} against {b:?}");
        }
        assert!(
            parts(&units("\u{1100}\u{1161}")).is_err(),
            "conjoining jamo"
        );
        assert!(parts(&units("\u{30a0}")).is_err(), "the double hyphen");
        // Excel fn1 D2, unlike a repetition of the Han character's own primary.
        assert_eq!(order("時々", "時時"), Some(Ordering::Less));
        assert!(
            parts(&units("\u{fc5e}")).is_err(),
            "an isolated form of marks"
        );
    }
}
