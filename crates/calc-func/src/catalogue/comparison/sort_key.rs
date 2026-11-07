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
//! U+E01EF, a surrogate pair). The Hangul syllables, compatibility jamo and conjoining jamo weigh
//! on one scale of Windows primaries, and a run of conjoining jamo is weighed as Windows' keys write
//! it ([`jamo_cluster`], dc-8wp). A text has incomplete weights here when it holds U+0000, U+30A0 (which
//! `CompareStringEx` orders otherwise than its key), an isolated or medial form of Arabic marks
//! (U+FC5E to U+FC63, U+FE71 and the like, into which the walk's runs are not modelled), or an
//! iteration mark after a special outside fn7's three adjacent forms and four punctuation units.
//! After Han, the measured REPEAT primary is used instead of the character's own primary;
//! fn1 confirms this rule with 時, 人 and 漢, their suffixes and lookup/sort controls.
//!
//! `cargo xtask collation verify` compares the order these weights give with `CompareStringEx`
//! over random texts of many scripts.

use super::hangul_collation::{COMPAT, JAMO, SYLLABLE_RUNS};
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
    /// A Hangul conjoining jamo: its sort key is built from the run of jamo it starts
    /// ([`jamo_cluster`]), not from the unit alone.
    Jamo,
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
    let compat: HashMap<u16, (u16, u8)> = COMPAT.iter().map(|&(c, p, w)| (c, (p, w))).collect();

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
        let weight = if jamo_class(code).is_some() {
            Weight::Jamo
        } else if let Some(&(primary, case)) = compat.get(&code) {
            // A compatibility jamo is one primary among the Hangul syllables' and the conjoining
            // jamo's (measured, hangul_collation.rs), with its own case weight.
            weighted(&mut table, &[HANGUL | u64::from(primary)], &[2], &[case], None)
        } else if !covered {
            Weight::Refused
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
                } => {
                    // A syllable's token is its code unit; its primary is measured.
                    let primaries: Vec<u64> = primaries
                        .iter()
                        .map(|&token| match token.checked_sub(HANGUL) {
                            Some(point @ 0xAC00..=0xD7A3) => {
                                HANGUL | u64::from(syllable_primary(point as u32 - 0xAC00))
                            }
                            _ => token,
                        })
                        .collect();
                    weighted(&mut table, &primaries, diacritic, case, *kana)
                }
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
            let index = point - 0xAC00;
            weighted(&mut table, &[HANGUL | u64::from(syllable_primary(index))], &[2], &[2], None)
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

/// Whether a unit has no measured comparison route.
pub(super) fn refused(code: u16) -> bool {
    match code {
        0x0001..=0x007F => false,
        _ => matches!(table().weights[usize::from(code)], Weight::Refused),
    }
}

/// The kinds of Hangul conjoining jamo that Windows' sort keys tell apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JamoClass {
    /// U+1100 to U+1112, the leading consonants of modern syllables.
    LeadModern,
    /// The other leading consonants (U+1113 to U+115E, U+A960 to U+A97C).
    LeadArchaic,
    /// U+115F.
    LeadFiller,
    /// U+1160.
    VowelFiller,
    /// U+1161 to U+1175.
    VowelModern,
    /// The other vowels (U+1176 to U+11A7, U+D7B0 to U+D7C6).
    VowelArchaic,
    /// U+11A8 to U+11C2.
    TrailModern,
    /// The other trailing consonants (U+11C3 to U+11FF, U+D7CB to U+D7FB).
    TrailArchaic,
}

fn jamo_class(code: u16) -> Option<JamoClass> {
    Some(match code {
        0x1100..=0x1112 => JamoClass::LeadModern,
        0x1113..=0x115E | 0xA960..=0xA97C => JamoClass::LeadArchaic,
        0x115F => JamoClass::LeadFiller,
        0x1160 => JamoClass::VowelFiller,
        0x1161..=0x1175 => JamoClass::VowelModern,
        0x1176..=0x11A7 | 0xD7B0..=0xD7C6 => JamoClass::VowelArchaic,
        0x11A8..=0x11C2 => JamoClass::TrailModern,
        0x11C3..=0x11FF | 0xD7CB..=0xD7FB => JamoClass::TrailArchaic,
        _ => return None,
    })
}

/// The measured weights of a conjoining jamo: primary and the leading, vowel and trailing
/// extension bytes of its key.
fn jamo_entry(code: u16) -> (u16, u8, u8, u8) {
    let at = JAMO.binary_search_by_key(&code, |e| e.0).expect("a measured jamo");
    let (_, p, l, v, t) = JAMO[at];
    (p, l, v, t)
}

/// The primary Windows gives the syllable at `index` from U+AC00: the runs of consecutive
/// primaries are measured, the gaps between them hold compatibility and archaic jamo.
fn syllable_primary(index: u32) -> u16 {
    let at = SYLLABLE_RUNS.partition_point(|&(first, _)| u32::from(first) <= index);
    let (first, p) = SYLLABLE_RUNS[at - 1];
    p + (index - u32::from(first)) as u16
}

/// How many modern trailing consonants weigh at most `t` (the position a trailing consonant
/// takes among the syllables' final consonants).
fn trailing_rank(t: u8) -> u32 {
    JAMO.iter()
        .filter(|e| (0x11A8..=0x11C2).contains(&e.0) && e.4 <= t)
        .count() as u32
}

/// The index among the modern vowels of the last one weighing at most `v`.
fn vowel_rank(v: u8) -> u32 {
    JAMO.iter()
        .filter(|e| (0x1161..=0x1175).contains(&e.0) && e.3 <= v)
        .count() as u32
        - 1
}

/// What a run of conjoining jamo adds to a key: one primary and its three extension bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Triple {
    p: u16,
    l: u8,
    v: u8,
    t: u8,
}

/// The first weight Windows' key holds for the conjoining jamo at the start of `units`, and
/// how many units it takes. Windows' keys write each jamo as a primary and three extension
/// bytes (leading, vowel, trailing weights), and run some neighbours together (measured over
/// every leading consonant and vowel and the trailing consonants):
/// - a vowel filler after a leading consonant adds nothing, once;
/// - a vowel after the leading filler replaces it;
/// - an archaic vowel after a modern leading consonant, or any vowel after an archaic one, makes
///   one weight, placed where the syllable the pair would spell sorts;
/// - a trailing consonant after a leading consonant, a vowel or such a pair sets its trailing
///   byte, and after a modern leading consonant (alone, or with a vowel filler) moves its primary
///   to the syllable with that trailing consonant; after a modern consonant and a modern vowel it
///   joins all three when it is archaic (U+D7DD excepted).
///
/// Anything between two jamo, an ignorable unit or a mark too, ends the run.
fn jamo_cluster(units: &[u16]) -> (Triple, usize) {
    use JamoClass::*;
    let first = units[0];
    let mut class = jamo_class(first).expect("a conjoining jamo");
    let (p, l, v, t) = jamo_entry(first);
    let mut triple = Triple { p, l, v, t };
    let lead_index = u32::from(first.wrapping_sub(0x1100));
    let mut used = 1;
    let mut filled = false;
    let mut trailing_open = !matches!(class, TrailModern | TrailArchaic);
    while let Some(&next) = units.get(used) {
        let Some(next_class) = jamo_class(next) else { break };
        let (_, _, nv, nt) = jamo_entry(next);
        let leading = matches!(class, LeadModern | LeadArchaic | LeadFiller);
        match next_class {
            VowelFiller if leading && !filled => filled = true,
            VowelFiller | VowelModern | VowelArchaic if filled => break,
            VowelModern | VowelArchaic if class == LeadFiller => {
                let (np, nl, nv, nt) = jamo_entry(next);
                triple = Triple { p: np, l: nl, v: nv, t: nt };
                class = next_class;
            }
            VowelArchaic if class == LeadModern => {
                let syllable = lead_index * 588 + vowel_rank(nv) * 28 + 27;
                triple = Triple { p: syllable_primary(syllable), l: triple.l - 0x80, v: nv, t: 2 };
                class = TrailModern;
                trailing_open = true;
            }
            VowelModern | VowelArchaic if class == LeadArchaic => {
                triple = Triple { p: triple.p + 1, l: triple.l - 0x80, v: nv, t: 2 };
                class = TrailModern;
                trailing_open = true;
            }
            VowelModern
                if class == LeadModern
                    && matches!(units.get(used + 1), Some(&t3) if jamo_class(t3) == Some(TrailArchaic) && t3 != 0xD7DD) =>
            {
                let (_, _, _, t3) = jamo_entry(units[used + 1]);
                let syllable = lead_index * 588 + vowel_rank(nv) * 28 + trailing_rank(t3);
                triple = Triple { p: syllable_primary(syllable), l: triple.l - 0x80, v: nv, t: t3 };
                used += 2;
                break;
            }
            TrailModern | TrailArchaic if trailing_open && triple.t == 2 => {
                if class == LeadModern {
                    triple.p = syllable_primary(lead_index * 588 + trailing_rank(nt) - 1);
                }
                triple.t = nt;
                used += 1;
                break;
            }
            _ => break,
        }
        used += 1;
    }
    (triple, used)
}

/// A Hangul primary's key bytes: the syllables' group and the primary.
fn hangul_slot(p: u16) -> [u8; PRIMARY_BYTES] {
    let mut bytes = [0u8; PRIMARY_BYTES];
    bytes[0] = 0xC0;
    bytes[1..9].copy_from_slice(&(HANGUL | u64::from(p)).to_be_bytes());
    bytes
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
    /// The extension bytes of each primary weight that has them (a conjoining jamo's).
    ext: Vec<Option<[u8; 3]>>,
    /// The offset in the text of the unit each primary weight starts at.
    at: Vec<u32>,
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
    /// A conjoining jamo, whose key an iteration mark after it is not measured against.
    Jamo,
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
    /// The extension bytes of the primary weights that have them, by weight index.
    ext: Vec<(usize, [u8; 3])>,
    /// The offset in the text of the unit of each primary weight, by weight index.
    starts_at: Vec<u32>,
    /// The offset in the text of the unit being added.
    at: u32,
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
    /// Records the offset of the unit being added for the primary weights it has added.
    fn mark_weights(&mut self) {
        while self.starts_at.len() < self.primary.len() / PRIMARY_BYTES {
            self.starts_at.push(self.at);
        }
    }

    /// Adds one conjoining jamo weight (see [`jamo_cluster`]).
    fn jamo(&mut self, triple: Triple, before: &mut Before) {
        self.ext.push((
            self.primary.len() / PRIMARY_BYTES,
            [triple.l, triple.v, triple.t],
        ));
        self.primary
            .extend_from_slice(&hangul_slot(triple.p));
        // Windows' key holds the primary and six extension bytes.
        self.windows_length += 8;
        self.steps.push(Step::Base(2));
        self.case.push(2);
        *before = Before::Jamo;
    }

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
            Weight::Refused => return None,
            Weight::Jamo => {
                let (triple, _) = jamo_cluster(&[code]);
                self.jamo(triple, before);
            }
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
                    Before::Jamo => return None,
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
        gathered.at = at as u32;
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
        if table.weights[usize::from(code)] == Weight::Jamo {
            // A run of jamo is one weight: its other units add nothing of their own.
            let (triple, used) = jamo_cluster(&text[at..]);
            gathered.jamo(triple, &mut before);
            gathered.mark_weights();
            for _ in 1..used {
                starts.push(gathered.steps.len() as u32);
            }
            at += used;
            continue;
        }
        let primaries = gathered.primary.len();
        if gathered.unit(table, code, &mut before).is_none() {
            gathered.primary.truncate(primaries);
            return Err(Unfinished {
                primary: gathered.primary,
                // An iteration failure after One or after a jamo (whose weight is measured, 04xx, not modelled) would still add a primary. After a special
                // even a nonempty contribution has not been measured; leave it undecided.
                continuation: if matches!(table.weights[usize::from(code)], Weight::Iteration(_))
                    && matches!(before, Before::One(..) | Before::Jamo)
                {
                    Continuation::More
                } else {
                    Continuation::Unknown
                },
            });
        }
        gathered.mark_weights();
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
        ext: ext_at,
        starts_at,
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
    let mut ext = vec![None; primary.len() / PRIMARY_BYTES];
    for (weight, bytes) in ext_at {
        ext[weight] = Some(bytes);
    }
    Ok(Parts {
        primary,
        ext,
        at: starts_at,
        steps,
        rest,
        starts,
    })
}

/// The order of two texts' primary weights, which Windows compares one weight at a time. A
/// conjoining jamo's extension bytes order two weights of one primary when both have them and
/// both start at the same offset in their texts: that pair is compared once both weights have
/// been reached. A text with fewer weights sorts first. (Measured: a syllable equals the leading
/// and trailing consonants that spell it; the leading consonant sorts below its compatibility
/// form; an ignorable unit before a vowel in one text only leaves it equal to another vowel,
/// though two weights later at one offset in both are compared.)
pub(super) fn primary_order(left: &Parts, right: &Parts) -> Ordering {
    let slot = |parts: &Parts, i: usize| -> [u8; PRIMARY_BYTES] {
        parts.primary[i * PRIMARY_BYTES..(i + 1) * PRIMARY_BYTES]
            .try_into()
            .expect("primaries are whole weights")
    };
    let extension = |x: usize, y: usize| -> Ordering {
        match (left.ext[x], right.ext[y]) {
            (Some(p), Some(q)) if left.at[x] == right.at[y] && slot(left, x) == slot(right, y) => {
                p.cmp(&q)
            }
            _ => Ordering::Equal,
        }
    };
    let (a, b) = (left.ext.len(), right.ext.len());
    for i in 0..a.min(b) {
        let order = slot(left, i).cmp(&slot(right, i));
        if order.is_ne() {
            return order;
        }
        let order = extension(i, i);
        if order.is_ne() {
            return order;
        }
        for x in 0..i {
            let order = extension(x, i).then_with(|| extension(i, x));
            if order.is_ne() {
                return order;
            }
        }
    }
    a.cmp(&b)
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
            primary_order(&left, &right)
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
            parts(&units("\u{1100}\u{1161}")).is_ok(),
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
