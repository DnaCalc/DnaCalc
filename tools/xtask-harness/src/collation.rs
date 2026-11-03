//! `cargo xtask collation generate` and `cargo xtask collation verify [--count <n>] [--seed <n>]`:
//! the tables behind calc-func's text collation and its check against Windows (beads dc-zsn.1
//! and dc-zsn.2).
//!
//! Excel compares two texts (the comparison operators, XMATCH, XLOOKUP, LOOKUP, an approximate
//! MATCH, SORT, a criterion) in two passes: its own pass over the code units up to `z`, then the
//! Windows comparison `CompareStringEx` under the user's locale with `NORM_IGNORECASE`, which is a
//! comparison of the two texts' sort keys (`LCMapStringEx` with `LCMAP_SORTKEY`). Measured: every
//! text row of the catalogue's comparison oracle (cdoc_jpha, 1,107 rows across ASCII, Latin, Greek,
//! Cyrillic, CJK and combining marks) is that rule, under en-ZA.
//!
//! calc-func does not carry Windows' sort weights. It carries the Unicode Collation Algorithm's
//! default order as the CLDR root collation gives it (the ICU Windows ships, read here through
//! `ucol_openElements`; the Han characters by the UCA's implicit weights, in code point order), with
//! the Unicode Character Database's ages, decomposition types and names, and the measured ways in
//! which Windows departs from that order. `generate` writes two files:
//!
//! - `root_collation.rs`: Unicode data only, for each code unit calc-func covers: its root
//!   primary weights and the non-common root secondary weights on each, and flags (assigned after
//!   Unicode 6.0, a wide or narrow form, a small or katakana letter).
//! - `windows_collation.rs`: what was measured of Windows, kept as small as the measurements allow:
//!   the code units covered; the Windows group each run of root primaries falls in; the root
//!   primaries Windows orders otherwise than the root does (each placed after the primary it
//!   follows in Windows' order); the diacritic weight Windows gives each root secondary; the
//!   special units (the hyphen, the apostrophe, the controls); the surrogate ranges; the code
//!   units whose weights Windows gives otherwise than the rules derive (each with its weights
//!   stated); the composites whose keys are malformed (a 0x00 diacritic byte), each with the
//!   decomposition `CompareStringEx` compares it as (measured in five contexts); the iteration and
//!   prolonged-sound marks; and the primary such a mark takes after each unit, where it is not the
//!   unit's own (measured after every code unit).
//!
//! Every code unit but U+0000 is covered (dc-zsn.2; dc-zsn.1 covered the scripts whose departures
//! from the root order are few). Refused, so that a text holding one is not decided: the Hangul
//! conjoining jamo (which Windows composes with each other), U+30A0 (which `CompareStringEx` orders
//! before a repeated unit, against its key) and the units compared as marks alone (isolated and
//! medial forms of Arabic marks, into which calc-func's diacritic walk is not modelled); calc-func
//! also refuses an iteration mark after a special. The measured repeat primary after Han is
//! used, as fn1's Excel comparison and ordering controls confirm.
//! The diacritic level is not read from the keys: `CompareStringEx` walks it, and
//! calc-func's walk was measured with `verify` (see sort_key.rs).
//!
//! `generate` checks that the rules and the exceptions give every covered unit the weights
//! Windows gives it, up to the order of the primary weights; `verify` draws random texts from pools
//! of scripts and compares calc-func's order of each pair with Excel's rule computed through
//! `CompareStringEx`: every decided pair must agree. Windows only.

use std::process::ExitCode;

pub const USAGE: &str = "usage: cargo xtask collation generate\n       cargo xtask collation verify [--count <n>] [--seed <n>]";

/// The locale whose collation the tables are of: the Windows locale of the machine Excel was
/// measured on (en-ZA). en-ZA's default sort is Windows' default table.
pub const LOCALE: &str = "en-ZA";

const ROOT_OUT: &str = "crates/calc-func/src/catalogue/comparison/root_collation.rs";
const WINDOWS_OUT: &str = "crates/calc-func/src/catalogue/comparison/windows_collation.rs";

pub fn run(raw: &[String]) -> ExitCode {
    match raw.split_first() {
        Some((command, rest)) if command == "generate" && rest.is_empty() => windows::generate(),
        Some((command, rest)) if command == "verify" => {
            let (mut count, mut seed) = (20_000u32, 0x2545_f491_4f6c_dd1du64);
            let mut args = rest.iter();
            while let Some(arg) = args.next() {
                match (arg.as_str(), args.next().map(|v| v.parse::<u64>())) {
                    ("--count", Some(Ok(value))) => count = value as u32,
                    ("--seed", Some(Ok(value))) => seed = value.max(1),
                    _ => return usage(),
                }
            }
            windows::verify(count, seed)
        }
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

#[cfg(not(windows))]
mod windows {
    use std::process::ExitCode;

    pub fn generate() -> ExitCode {
        eprintln!("xtask collation: Windows only (it asks Windows and its ICU)");
        ExitCode::FAILURE
    }

    pub fn verify(_: u32, _: u64) -> ExitCode {
        generate()
    }
}

#[cfg(windows)]
mod windows {
    use super::{LOCALE, ROOT_OUT, WINDOWS_OUT};
    use std::cmp::Ordering;
    use std::collections::{BTreeMap, BTreeSet, HashMap};
    use std::fmt::Write as _;
    use std::process::ExitCode;
    use windows_sys::Win32::Globalization::{
        CompareStringEx, GetNLSVersionEx, LCMAP_SORTKEY, LCMapStringEx, NLSVERSIONINFOEX,
        NORM_IGNORECASE, U_DT_NARROW, U_DT_WIDE, U_UNICODE_CHAR_NAME, UCHAR_DECOMPOSITION_TYPE,
        u_charAge, u_charName, u_getIntPropertyValue, u_getUnicodeVersion, ucol_close,
        ucol_closeElements, ucol_getVersion, ucol_next, ucol_open, ucol_openElements,
        unorm2_getNFDInstance, unorm2_getNFKDInstance, unorm2_normalize,
    };

    // --- Windows -----------------------------------------------------------------------------

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// The sort key of `units` under the locale with `NORM_IGNORECASE`, without its final 0.
    pub(super) fn key(units: &[u16]) -> Vec<u8> {
        let locale = wide(LOCALE);
        let mut buffer = vec![0u16; 8192];
        // SAFETY: the locale is NUL-terminated; the source holds `units.len()` units; the
        // destination holds `buffer.len()` units, of which a sort key fills `n` bytes.
        let n = unsafe {
            LCMapStringEx(
                locale.as_ptr(),
                LCMAP_SORTKEY | NORM_IGNORECASE,
                units.as_ptr(),
                units.len() as i32,
                buffer.as_mut_ptr(),
                (buffer.len() * 2) as i32,
                std::ptr::null(),
                std::ptr::null(),
                0,
            )
        };
        assert!(n > 0, "LCMapStringEx gave no sort key");
        let bytes: Vec<u8> = buffer
            .iter()
            .flat_map(|unit| unit.to_le_bytes())
            .take(n as usize)
            .collect();
        bytes[..bytes.len() - 1].to_vec()
    }

    /// Excel's order of two texts: its pass over the units up to `z` (letters ignoring case), then
    /// `CompareStringEx` with `NORM_IGNORECASE` of the rest (measured: Excel orders `a` and U+30FC
    /// after `ab`).
    fn excel_order(a: &[u16], b: &[u16]) -> Ordering {
        let fold = |code: u16| {
            if (0x61..=0x7A).contains(&code) {
                code - 0x20
            } else {
                code
            }
        };
        for (at, (&x, &y)) in a.iter().zip(b).enumerate() {
            if x > 0x7A || y > 0x7A || fold(x) != fold(y) {
                return compare_string(&a[at..], &b[at..]);
            }
        }
        a.len().cmp(&b.len())
    }

    /// `CompareStringEx` under the locale with `NORM_IGNORECASE`.
    fn compare_string(a: &[u16], b: &[u16]) -> Ordering {
        let locale = wide(LOCALE);
        // SAFETY: the locale is NUL-terminated and each text holds its stated length.
        let result = unsafe {
            CompareStringEx(
                locale.as_ptr(),
                NORM_IGNORECASE,
                a.as_ptr(),
                a.len() as i32,
                b.as_ptr(),
                b.len() as i32,
                std::ptr::null(),
                std::ptr::null(),
                0,
            )
        };
        match result {
            1 => Ordering::Less,
            2 => Ordering::Equal,
            3 => Ordering::Greater,
            _ => panic!("CompareStringEx failed"),
        }
    }

    /// The kana section's lists (small, iteration, type, width) of a key's extra section, or
    /// `None` when it has another form.
    fn kana_lists(extra: &[u8]) -> Option<[Vec<u8>; 4]> {
        let first = extra.iter().position(|&b| b == 0xFF)?;
        let small = extra[..first].to_vec();
        let rest = &extra[first + 1..];
        let two = rest.iter().position(|&b| b == 0x02)?;
        let iteration = rest[..two].to_vec();
        let rest = &rest[two + 1..];
        let second = rest.iter().position(|&b| b == 0xFF)?;
        let kind = rest[..second].to_vec();
        let rest = &rest[second + 1..];
        let third = rest.iter().position(|&b| b == 0xFF)?;
        let width = rest[..third].to_vec();
        (third + 1 == rest.len()).then_some([small, iteration, kind, width])
    }

    /// A key's sections: primary, diacritic, case, extra (kana) and special.
    fn sections(key: &[u8]) -> [Vec<u8>; 5] {
        let mut parts: [Vec<u8>; 5] = Default::default();
        let mut at = 0;
        for &byte in key {
            if byte == 1 && at < 4 {
                at += 1;
            } else {
                parts[at].push(byte);
            }
        }
        parts
    }

    /// What one code unit adds to a Windows sort key.
    #[derive(Clone, PartialEq, Eq, Debug)]
    enum Unit {
        Ignorable,
        Refused,
        Mark(u8),
        Special([u8; 2]),
        /// A unit `CompareStringEx` compares as this sequence of units (its key is malformed);
        /// `false` when the sequence is its compatibility decomposition without the leading space.
        Expansion(Vec<u16>, bool),
        /// An iteration or prolonged-sound mark: the primary of the unit before it (its vowel's
        /// when `vowel`, for a kana), with its own diacritic entry, its iteration weight in the
        /// kana section, and its own width weight or (`None`) the width of the unit before it.
        Iteration {
            vowel: bool,
            diacritic: u8,
            iteration: u8,
            width: Option<u8>,
        },
        Weighted {
            /// The primary weights, each one Windows token (see [`tokens`]).
            primary: Vec<Vec<u8>>,
            diacritic: Vec<u8>,
            case: Vec<u8>,
            kana: Option<[u8; 3]>,
        },
    }

    /// A Windows primary weight's length from its first (script) byte: four bytes for the Han
    /// script 0xFD, three for scripts 0xA9 to 0xAF, two otherwise.
    fn token_length(script: u8) -> usize {
        match script {
            0xFD => 4,
            0xA9..=0xAF => 3,
            _ => 2,
        }
    }

    fn tokens(primary: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < primary.len() {
            let n = token_length(primary[at]).min(primary.len() - at);
            out.push(primary[at..at + n].to_vec());
            at += n;
        }
        out
    }

    /// Units Windows' keys do not decide: U+0000 (Windows ignores it; Excel gives no formula a
    /// way to make it) and Hangul conjoining jamo, which compose with each other.
    fn refused_block(code: u16) -> bool {
        matches!(
            code,
            0x0000
                | 0x1100..=0x11FF
                | 0xA960..=0xA97F
                | 0xD7B0..=0xD7FF
        )
    }

    /// The Windows weights of one code unit, read from its sort key and two probes after it (an
    /// `a` with U+0332, whose diacritic weight 0x65 no other unit carries alone, and a full-width
    /// `A`, whose case weight is 0x03).
    fn windows_unit(code: u16) -> Unit {
        if refused_block(code) {
            return Unit::Refused;
        }
        let [primary, diacritic, case, extra, special] = sections(&key(&[code]));
        // A primary weight of script 0x03 (the kana digraphs U+309F and U+30FF) is not compared
        // by its key's bytes: CompareStringEx orders such a unit otherwise.
        if primary.first().is_some_and(|&script| script < 0x07) {
            return Unit::Refused;
        }
        if !primary.is_empty() {
            let kana = if extra.is_empty() {
                None
            } else {
                match extra.as_slice() {
                    [0xFF, 0x02, 0xFF, 0xFF] => Some([0xC6, 0xE4, 0xC5]),
                    [0xFF, 0x02, 0xC4, 0xFF, 0xFF] => Some([0xC6, 0xC4, 0xC5]),
                    [0xFF, 0x02, 0xC4, 0xFF, 0xC4, 0xFF] => Some([0xC6, 0xC4, 0xC4]),
                    [0xC4, 0xFF, 0x02, 0xC4, 0xFF, 0xFF] => Some([0xC4, 0xC4, 0xC5]),
                    [0xC4, 0xFF, 0x02, 0xFF, 0xFF] => Some([0xC4, 0xE4, 0xC5]),
                    [0xC4, 0xFF, 0x02, 0xC4, 0xFF, 0xC4, 0xFF] => Some([0xC4, 0xC4, 0xC4]),
                    _ => return Unit::Refused,
                }
            };
            if !special.is_empty() {
                return Unit::Refused;
            }
            let probe = sections(&key(&[code, 0x61, 0x0332]));
            let [probe_a, ..] = sections(&key(&[0x61, 0x0332]));
            if probe[0] != [primary.as_slice(), probe_a.as_slice()].concat() {
                return Unit::Refused;
            }
            let Some(entries) = probe[1].iter().rposition(|&b| b == 0x65) else {
                return Unit::Refused;
            };
            if entries == 0 && primary.len() != 3 {
                return Unit::Refused;
            }
            let case_probe = sections(&key(&[code, 0xFF21]));
            let Some(case_entries) = case_probe[2].iter().rposition(|&b| b == 0x03) else {
                return Unit::Refused;
            };
            if diacritic.len() > entries || case.len() > case_entries {
                return Unit::Refused;
            }
            let pad = |list: &[u8], n: usize| -> Vec<u8> {
                (0..n).map(|i| list.get(i).copied().unwrap_or(2)).collect()
            };
            return Unit::Weighted {
                primary: tokens(&primary),
                diacritic: pad(&diacritic, entries),
                case: pad(&case, case_entries),
                kana,
            };
        }
        if !extra.is_empty() {
            return Unit::Refused;
        }
        if !special.is_empty() {
            return match (special.as_slice(), diacritic.is_empty() && case.is_empty()) {
                ([_, _, w0, w1], true) => Unit::Special([*w0, *w1]),
                _ => Unit::Refused,
            };
        }
        if !diacritic.is_empty() {
            return match (diacritic.as_slice(), case.is_empty()) {
                ([weight], true) => Unit::Mark(*weight),
                _ => Unit::Refused,
            };
        }
        if !case.is_empty() {
            return Unit::Refused;
        }
        Unit::Ignorable
    }

    fn nls_version() -> String {
        let locale = wide(LOCALE);
        let mut info = NLSVERSIONINFOEX {
            dwNLSVersionInfoSize: std::mem::size_of::<NLSVERSIONINFOEX>() as u32,
            dwNLSVersion: 0,
            dwDefinedVersion: 0,
            dwEffectiveId: 0,
            guidCustomVersion: unsafe { std::mem::zeroed() },
        };
        // SAFETY: COMPARE_STRING (1) with a NUL-terminated locale and a sized structure.
        let ok = unsafe { GetNLSVersionEx(1, locale.as_ptr(), &mut info) };
        if ok == 0 {
            return "unknown".to_owned();
        }
        format!("NLS version 0x{:08X}", info.dwNLSVersion)
    }

    // --- Unicode data, through the ICU Windows ships ----------------------------------------

    struct Icu {
        collator: *mut isize,
        nfd: *mut isize,
        nfkd: *mut isize,
    }

    impl Icu {
        fn open() -> Icu {
            let mut status = 0;
            // SAFETY: "" names the root locale; the status is written.
            let collator = unsafe { ucol_open(c"".as_ptr().cast(), &mut status) };
            assert!(
                status <= 0 && !collator.is_null(),
                "ucol_open failed: {status}"
            );
            let mut status = 0;
            // SAFETY: the status is written.
            let nfd = unsafe { unorm2_getNFDInstance(&mut status) };
            assert!(
                status <= 0 && !nfd.is_null(),
                "unorm2_getNFDInstance failed"
            );
            let mut status = 0;
            // SAFETY: the status is written.
            let nfkd = unsafe { unorm2_getNFKDInstance(&mut status) };
            assert!(
                status <= 0 && !nfkd.is_null(),
                "unorm2_getNFKDInstance failed"
            );
            Icu {
                collator: collator.cast(),
                nfd: nfd.cast(),
                nfkd: nfkd.cast(),
            }
        }

        /// The root collation elements of `units`, as ICU's 32-bit CEs.
        fn elements(&self, units: &[u16]) -> Vec<u32> {
            let mut status = 0;
            // SAFETY: the collator is open; the text holds `units.len()` units.
            let iterator = unsafe {
                ucol_openElements(
                    self.collator.cast(),
                    units.as_ptr(),
                    units.len() as i32,
                    &mut status,
                )
            };
            assert!(status <= 0, "ucol_openElements failed: {status}");
            let mut out = Vec::new();
            loop {
                let mut status = 0;
                // SAFETY: the iterator is open.
                let element = unsafe { ucol_next(iterator, &mut status) };
                if element == -1 {
                    break;
                }
                out.push(element as u32);
            }
            // SAFETY: the iterator is open and closed once.
            unsafe { ucol_closeElements(iterator) };
            out
        }

        fn age(&self, code: u32) -> (u8, u8) {
            let mut version = [0u8; 4];
            // SAFETY: the version array holds four bytes.
            unsafe { u_charAge(code as i32, version.as_mut_ptr()) };
            (version[0], version[1])
        }

        fn decomposition_type(&self, code: u32) -> i32 {
            // SAFETY: a property read.
            unsafe { u_getIntPropertyValue(code as i32, UCHAR_DECOMPOSITION_TYPE) }
        }

        fn name(&self, code: u32) -> String {
            let mut buffer = [0u8; 128];
            let mut status = 0;
            // SAFETY: the buffer holds 128 bytes.
            let n = unsafe {
                u_charName(
                    code as i32,
                    U_UNICODE_CHAR_NAME,
                    buffer.as_mut_ptr(),
                    buffer.len() as i32,
                    &mut status,
                )
            };
            String::from_utf8_lossy(&buffer[..n.max(0) as usize]).into_owned()
        }

        fn nfd(&self, code: u16) -> Vec<u16> {
            self.normalize(self.nfd, code)
        }

        fn nfkd(&self, code: u16) -> Vec<u16> {
            self.normalize(self.nfkd, code)
        }

        fn normalize(&self, instance: *mut isize, code: u16) -> Vec<u16> {
            let mut out = [0u16; 32];
            let mut status = 0;
            // SAFETY: one source unit; the destination holds 32.
            let n = unsafe {
                unorm2_normalize(
                    instance.cast(),
                    &code,
                    1,
                    out.as_mut_ptr(),
                    out.len() as i32,
                    &mut status,
                )
            };
            out[..n.max(0) as usize].to_vec()
        }

        fn versions(&self) -> String {
            let mut collation = [0u8; 4];
            let mut unicode = [0u8; 4];
            // SAFETY: four-byte version arrays.
            unsafe {
                ucol_getVersion(self.collator.cast(), collation.as_mut_ptr());
                u_getUnicodeVersion(unicode.as_mut_ptr());
            }
            format!(
                "Unicode {}.{}, root collation version {}.{}",
                unicode[0], unicode[1], collation[0], collation[1]
            )
        }
    }

    impl Drop for Icu {
        fn drop(&mut self) {
            // SAFETY: opened once by `open`.
            unsafe { ucol_close(self.collator.cast()) };
        }
    }

    /// A primary in the root order: the kinds sort root primaries, Hangul syllables, surrogates,
    /// Han characters and the measured extra primaries in that order.
    const HANGUL: u64 = 1 << 48;
    const SURROGATE: u64 = 2 << 48;
    const HAN: u64 = 3 << 48;
    const EXTRA: u64 = 4 << 48;

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

    /// What the Unicode data says of one covered code unit.
    #[derive(Clone, Debug, Default)]
    struct Root {
        /// Root primaries (or a Han, Hangul or surrogate primary).
        primaries: Vec<u64>,
        /// The non-common root secondaries before the first primary, then on each primary.
        secondaries: Vec<Vec<u32>>,
        new: bool,
        wide: bool,
        narrow: bool,
        small: bool,
        katakana: bool,
        kana: bool,
        /// Han, Hangul and surrogates are given by rule, not stored.
        rule: bool,
    }

    fn root(icu: &Icu, code: u16) -> Root {
        let point = u32::from(code);
        let (major, minor) = icu.age(point);
        let mut root = Root {
            new: (major, minor) == (0, 0) || (major, minor) >= (6, 1),
            ..Root::default()
        };
        if (0xAC00..=0xD7A3).contains(&code) {
            root.primaries = vec![HANGUL | u64::from(code)];
            root.secondaries = vec![vec![], vec![]];
            root.rule = true;
            return root;
        }
        if (0xD800..=0xDFFF).contains(&code) {
            root.primaries = vec![SURROGATE | u64::from(code)];
            root.secondaries = vec![vec![], vec![]];
            root.rule = true;
            root.new = false;
            return root;
        }
        let decomposition = icu.nfd(code);
        if let [single] = decomposition.as_slice()
            && let Some(base) = han_base(*single)
        {
            root.primaries = vec![HAN | (base << 32) | u64::from(*single)];
            root.secondaries = vec![vec![], vec![]];
            root.rule = *single == code;
            return root;
        }
        let kind = icu.decomposition_type(point);
        root.wide = kind == U_DT_WIDE;
        root.narrow = kind == U_DT_NARROW;
        let name = icu.name(point);
        root.kana = matches!(code, 0x3041..=0x30FF | 0xFF65..=0xFF9F);
        root.small = root.kana && name.contains("SMALL");
        root.katakana = root.kana && name.contains("KATAKANA");
        let mut secondaries: Vec<Vec<u32>> = vec![vec![]];
        for element in icu.elements(&[code]) {
            let primary = element >> 16;
            let secondary = (element >> 8) & 0xFF;
            if element & 0xC0 == 0xC0 {
                // A continuation: more primary bytes, or more secondary bytes.
                if primary != 0 {
                    let last = root.primaries.last_mut().expect("a continued primary");
                    *last = (*last << 16) | u64::from(primary);
                } else if let Some(last) = secondaries.last_mut().and_then(|list| list.last_mut()) {
                    *last = (*last << 8) | secondary;
                }
                continue;
            }
            if primary != 0 {
                root.primaries.push(u64::from(primary));
                secondaries.push(vec![]);
            }
            if secondary != 0 && secondary != 0x05 {
                secondaries.last_mut().expect("a list").push(secondary);
            }
        }
        root.secondaries = secondaries;
        root
    }

    // --- the model ------------------------------------------------------------------------------

    /// The code units covered: every one but U+0000.
    const COVERED: [(u16, u16); 1] = [(0x0001, 0xFFFF)];

    /// Windows' script bytes that hold one run of a rule's primaries (Hangul's 0xC0 to 0xEC, the
    /// low surrogates' 0xB4 to 0xB8, ...): one group each.
    const MERGED: [(u8, u8); 4] = [(0xA9, 0xAF), (0xB4, 0xB8), (0xC0, 0xEC), (0xFE, 0xFF)];

    fn group_of(script: u8) -> u8 {
        MERGED
            .iter()
            .find(|(a, b)| (*a..=*b).contains(&script))
            .map_or(script, |(a, _)| *a)
    }

    /// The diacritic and case weights of a weighted unit the rules derive.
    fn derived(
        root: &Root,
        secondary: &HashMap<u32, u8>,
        surrogate: Option<(u8, u8)>,
    ) -> Option<(Vec<u8>, Vec<u8>, Option<[u8; 3]>)> {
        let n = root.primaries.len();
        if let Some((d, c)) = surrogate {
            return Some((vec![d; n], vec![c; n], None));
        }
        if !root.secondaries[0].is_empty() || root.secondaries.len() != n + 1 {
            return None;
        }
        let mut diacritic = Vec::with_capacity(n);
        for list in &root.secondaries[1..] {
            let mut sum = 2u32;
            for s in list {
                sum += u32::from(*secondary.get(s)?);
            }
            diacritic.push((sum % 256) as u8);
        }
        let case = vec![if root.wide { 3 } else { 2 }; n];
        let kana = root.kana.then(|| {
            [
                if root.small { 0xC4 } else { 0xC6 },
                if root.katakana { 0xC4 } else { 0xE4 },
                if root.narrow { 0xC4 } else { 0xC5 },
            ]
        });
        Some((diacritic, case, kana))
    }

    fn varint(out: &mut Vec<u8>, mut value: u64) {
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    fn base64(data: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
        for chunk in data.chunks(3) {
            let n = (u32::from(chunk[0]) << 16)
                | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
                | u32::from(*chunk.get(2).unwrap_or(&0));
            for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
                if i <= chunk.len() {
                    out.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn base64_lines(data: &[u8]) -> String {
        let encoded = base64(data);
        let mut lines = String::new();
        for chunk in encoded.as_bytes().chunks(96) {
            let _ = writeln!(
                lines,
                "    \"{}\",",
                std::str::from_utf8(chunk).expect("ASCII")
            );
        }
        lines
    }

    pub fn generate() -> ExitCode {
        let icu = Icu::open();
        let mut windows: Vec<Unit> = (0u32..=0xFFFF).map(|c| windows_unit(c as u16)).collect();

        // Iteration and prolonged-sound marks: a unit whose key alone is the primary FF FF and
        // which, after KA (U+304B), repeats KA's primary or gives A's (U+3042), and which
        // CompareStringEx does not order before a repeated `a` after `a`. Its width weight is its own
        // unless after a narrow KA (U+FF76) it is the narrow one.
        let first_token = |units: &[u16]| tokens(&sections(&key(units))[0]);
        let ka_token = first_token(&[0x304B])[0].clone();
        let a_token = first_token(&[0x3042])[0].clone();
        let none_token = vec![0xFFu8, 0xFF];
        for c in 1u32..=0xFFFF {
            let c = c as u16;
            if refused_block(c) || first_token(&[c]) != [none_token.clone()] {
                continue;
            }
            let after_ka = sections(&key(&[0x304B, c]));
            let after_narrow = sections(&key(&[0xFF76, c]));
            let t = tokens(&after_ka[0]);
            let vowel = match t.as_slice() {
                [x, y] if *x == ka_token && *y == ka_token => false,
                [x, y] if *x == ka_token && *y == a_token => true,
                _ => continue,
            };
            // CompareStringEx itself must repeat the unit before it, at most with a diacritic
            // (U+30A0 orders before a repeated `a`).
            if compare_string(&[0x61, c], &[0x61, 0x61]) == Ordering::Less {
                continue;
            }
            let (Some(lists), Some(narrow)) =
                (kana_lists(&after_ka[3]), kana_lists(&after_narrow[3]))
            else {
                continue;
            };
            let entry =
                |list: &[u8], at: usize, default: u8| list.get(at).copied().unwrap_or(default);
            let own_width = entry(&lists[3], 1, 0xC5);
            let narrow_width = entry(&narrow[3], 1, 0xC5);
            windows[usize::from(c)] = Unit::Iteration {
                vowel,
                diacritic: entry(&after_ka[1], 1, 2),
                iteration: entry(&lists[1], 1, 3),
                width: (narrow_width == own_width).then_some(own_width),
            };
        }

        // Units whose keys are malformed: CompareStringEx compares them as their canonical or
        // compatibility decomposition (measured, in five contexts), or as the compatibility
        // decomposition without its leading space.
        for c in 1u32..=0xFFFF {
            let c = c as u16;
            if windows[usize::from(c)] != Unit::Refused || refused_block(c) {
                continue;
            }
            let nfkd = icu.nfkd(c);
            let mut candidates = vec![icu.nfd(c), nfkd.clone()];
            if nfkd.first() == Some(&0x20) {
                candidates.push(nfkd[1..].to_vec());
            }
            for (n, candidate) in candidates.into_iter().enumerate() {
                if candidate.len() < 2
                    || candidate.iter().any(|&u| {
                        matches!(
                            windows[usize::from(u)],
                            Unit::Refused | Unit::Expansion(..) | Unit::Iteration { .. }
                        )
                    })
                {
                    continue;
                }
                let contexts: [(&[u16], &[u16]); 5] = [
                    (&[], &[]),
                    (&[0x62], &[0x63]),
                    (&[0x5D0], &[0x301]),
                    (&[0x2D], &[]),
                    (&[0x301], &[0x61]),
                ];
                if contexts.iter().all(|(before, after)| {
                    let x = [*before, &[c], *after].concat();
                    let y = [*before, candidate.as_slice(), *after].concat();
                    compare_string(&x, &y) == Ordering::Equal
                }) {
                    // One of marks alone (an isolated or medial form of Arabic marks) stays
                    // refused: calc-func's diacritic walk does not model the runs into it.
                    let weighted = candidate
                        .iter()
                        .any(|&u| matches!(windows[usize::from(u)], Unit::Weighted { .. }));
                    if weighted {
                        windows[usize::from(c)] = Unit::Expansion(candidate, n < 2);
                    }
                    break;
                }
            }
        }

        // The primary an iteration mark takes after each unit of one primary (measured with
        // U+309D, and with U+30FC for the prolonged-sound marks): that primary, but where measured
        // otherwise. After anything else (nothing, a unit of several primaries) it is FF FF; after
        // a special calc-func refuses it.
        let mut repeat_after: BTreeMap<u16, Vec<u8>> = BTreeMap::new();
        let mut vowel_after: BTreeMap<u16, Vec<u8>> = BTreeMap::new();
        for c in 1u32..=0xFFFF {
            let c = c as u16;
            let own = match &windows[usize::from(c)] {
                Unit::Weighted { primary, .. } if primary.len() == 1 => primary[0].clone(),
                _ => continue,
            };
            let after = |m: u16| -> Option<Vec<u8>> {
                let t = tokens(&sections(&key(&[c, m]))[0]);
                (t.len() == 2).then(|| t[1].clone())
            };
            let (Some(repeat), Some(vowel)) = (after(0x309D), after(0x30FC)) else {
                continue;
            };
            if repeat != own {
                repeat_after.insert(c, repeat.clone());
            }
            if vowel != repeat {
                vowel_after.insert(c, vowel);
            }
        }
        let specials: BTreeMap<u16, [u8; 2]> = windows
            .iter()
            .enumerate()
            .filter_map(|(c, u)| match u {
                Unit::Special(w) => Some((c as u16, *w)),
                _ => None,
            })
            .collect();
        let covered = |c: u16| {
            COVERED.iter().any(|(a, b)| (*a..=*b).contains(&c)) || specials.contains_key(&c)
        };
        let units: Vec<u16> = (1u32..=0xFFFF)
            .map(|c| c as u16)
            .filter(|&c| covered(c))
            .collect();
        let roots: HashMap<u16, Root> = units.iter().map(|&c| (c, root(&icu, c))).collect();

        // Surrogates: Windows' ranges, each ignorable or weighted with fixed diacritic and case
        // weights (measured).
        let mut surrogate_rules: Vec<(u16, u16, Option<(u8, u8)>)> = Vec::new();
        for c in 0xD800u16..=0xDFFF {
            let rule = match &windows[usize::from(c)] {
                Unit::Weighted {
                    diacritic, case, ..
                } => Some((diacritic[0], case[0])),
                _ => None,
            };
            match surrogate_rules.last_mut() {
                Some((_, last, r)) if *r == rule && *last + 1 == c => *last = c,
                _ => surrogate_rules.push((c, c, rule)),
            }
        }
        let surrogate_rule = |c: u16| {
            surrogate_rules
                .iter()
                .find(|(a, b, _)| (*a..=*b).contains(&c))
                .map(|r| r.2)
        };

        // The diacritic weight of each root secondary: learned from the marks and the letters
        // carrying one secondary (measured, majority).
        let mut votes: HashMap<u32, HashMap<u8, u32>> = HashMap::new();
        for &c in &units {
            let r = &roots[&c];
            if r.new || r.rule {
                continue;
            }
            match &windows[usize::from(c)] {
                Unit::Mark(w) if r.primaries.is_empty() => {
                    if let [list] = r.secondaries.as_slice()
                        && let [s] = list.as_slice()
                    {
                        *votes.entry(*s).or_default().entry(*w).or_default() += 1;
                    }
                }
                Unit::Weighted { diacritic, .. }
                    if r.secondaries.len() == diacritic.len() + 1
                        && r.secondaries[0].is_empty() =>
                {
                    for (list, &w) in r.secondaries[1..].iter().zip(diacritic) {
                        if let [s] = list.as_slice()
                            && w != 2
                        {
                            *votes.entry(*s).or_default().entry(w - 2).or_default() += 1;
                        }
                    }
                }
                _ => {}
            }
        }
        let majority = |v: &HashMap<u8, u32>| {
            *v.iter()
                .max_by_key(|(w, n)| (**n, std::cmp::Reverse(**w)))
                .expect("a vote")
                .0
        };
        let secondary: HashMap<u32, u8> = votes.iter().map(|(s, v)| (*s, majority(v))).collect();

        // Each root primary's Windows primary: learned from the weighted units (majority).
        let mut token_votes: HashMap<u64, HashMap<Vec<u8>, u32>> = HashMap::new();
        for &c in &units {
            let r = &roots[&c];
            if r.new && !(0xD800..=0xDFFF).contains(&c) {
                continue;
            }
            if let Unit::Weighted { primary, .. } = &windows[usize::from(c)]
                && primary.len() == r.primaries.len()
            {
                for (t, w) in r.primaries.iter().zip(primary) {
                    *token_votes
                        .entry(*t)
                        .or_default()
                        .entry(w.clone())
                        .or_default() += 1;
                }
            }
        }
        let map: BTreeMap<u64, Vec<u8>> = token_votes
            .iter()
            .map(|(t, v)| {
                let best = v
                    .iter()
                    .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                    .expect("a vote");
                (*t, best.0.clone())
            })
            .collect();

        // Predict every covered unit; what the rules do not give is an exception.
        let predict = |c: u16| -> Option<Unit> {
            let r = &roots[&c];
            if (0xD800..=0xDFFF).contains(&c) {
                let rule = surrogate_rule(c)?;
                let Some(rule) = rule else {
                    return Some(Unit::Ignorable);
                };
                let primary = r
                    .primaries
                    .iter()
                    .map(|t| map.get(t).cloned())
                    .collect::<Option<Vec<_>>>()?;
                let (diacritic, case, kana) = derived(r, &secondary, Some(rule))?;
                return Some(Unit::Weighted {
                    primary,
                    diacritic,
                    case,
                    kana,
                });
            }
            if let Some(w) = specials.get(&c) {
                return Some(Unit::Special(*w));
            }
            if r.new {
                return Some(Unit::Ignorable);
            }
            if r.primaries.is_empty() {
                return Some(match r.secondaries.as_slice() {
                    [list] if list.len() == 1 && secondary.contains_key(&list[0]) => {
                        Unit::Mark(secondary[&list[0]])
                    }
                    _ => Unit::Ignorable,
                });
            }
            let primary = r
                .primaries
                .iter()
                .map(|t| map.get(t).cloned())
                .collect::<Option<Vec<_>>>()?;
            let (diacritic, case, kana) = derived(r, &secondary, None)?;
            Some(Unit::Weighted {
                primary,
                diacritic,
                case,
                kana,
            })
        };
        let mut exceptions: BTreeMap<u16, Unit> = BTreeMap::new();
        for &c in &units {
            let actual = &windows[usize::from(c)];
            if *actual == Unit::Refused || predict(c).as_ref() != Some(actual) {
                exceptions.insert(c, actual.clone());
            }
        }

        // The order: root primaries in Windows groups (runs along the root order), each group in
        // root order but for the primaries Windows places otherwise (outside a longest run of
        // primaries both orders agree on), which are placed after the primary before them in
        // Windows' order (their anchor) and numbered among those sharing it. Windows primaries
        // that no root primary gives (an exception's) are extra primaries placed the same way.
        let mut extra: BTreeSet<Vec<u8>> = BTreeSet::new();
        let known: BTreeSet<&Vec<u8>> = map.values().collect();
        for unit in exceptions.values() {
            if let Unit::Weighted { primary, .. } = unit {
                for w in primary {
                    if !known.contains(w) {
                        extra.insert(w.clone());
                    }
                }
            }
        }
        for w in repeat_after
            .values()
            .chain(vowel_after.values())
            .chain(std::iter::once(&none_token))
        {
            if !known.contains(w) {
                extra.insert(w.clone());
            }
        }
        let mut by_group: BTreeMap<u8, Vec<u64>> = BTreeMap::new();
        for (t, w) in &map {
            by_group.entry(group_of(w[0])).or_default().push(*t);
        }
        // token -> (anchor, sub); an absent token is its own anchor with sub 0.
        let mut placed: BTreeMap<u64, (u64, u8)> = BTreeMap::new();
        let mut extra_placed: Vec<(u8, u64, u8, Vec<u8>)> = Vec::new();
        for (group, list) in &by_group {
            let mut first: BTreeMap<&Vec<u8>, u64> = BTreeMap::new();
            let mut sequence = Vec::new();
            for t in list {
                if !first.contains_key(&map[t]) {
                    first.insert(&map[t], *t);
                    sequence.push(*t);
                }
            }
            // Longest strictly increasing run of Windows primaries along the root order.
            let mut tails: Vec<usize> = Vec::new();
            let mut previous: Vec<Option<usize>> = vec![None; sequence.len()];
            for (i, t) in sequence.iter().enumerate() {
                let w = &map[t];
                let at = tails.partition_point(|&j| map[&sequence[j]] < *w);
                previous[i] = at.checked_sub(1).map(|k| tails[k]);
                if at == tails.len() {
                    tails.push(i);
                } else {
                    tails[at] = i;
                }
            }
            let mut agreed: BTreeSet<u64> = BTreeSet::new();
            let mut at = tails.last().copied();
            while let Some(i) = at {
                agreed.insert(sequence[i]);
                at = previous[i];
            }
            let ordered: Vec<(&Vec<u8>, u64)> = {
                let mut v: Vec<_> = agreed.iter().map(|t| (&map[t], *t)).collect();
                v.sort();
                v
            };
            let anchor_of = |w: &Vec<u8>| -> u64 {
                let at = ordered.partition_point(|(x, _)| *x < w);
                if at == 0 { 0 } else { ordered[at - 1].1 }
            };
            let mut pending: BTreeMap<u64, BTreeSet<Vec<u8>>> = BTreeMap::new();
            for t in list {
                let w = &map[t];
                if agreed.contains(t) {
                    continue;
                }
                if agreed.contains(&first[w]) {
                    placed.insert(*t, (first[w], 0));
                } else {
                    pending.entry(anchor_of(w)).or_default().insert(w.clone());
                }
            }
            let group_extra: Vec<Vec<u8>> = extra
                .iter()
                .filter(|w| group_of(w[0]) == *group)
                .cloned()
                .collect();
            for w in &group_extra {
                pending.entry(anchor_of(w)).or_default().insert(w.clone());
            }
            let sub = |anchor: u64, w: &Vec<u8>| -> u8 {
                let rank = pending[&anchor]
                    .iter()
                    .position(|x| x == w)
                    .expect("pending");
                u8::try_from(rank + 1).expect("fewer than 255 placed after one anchor")
            };
            for t in list {
                let w = &map[t];
                if agreed.contains(t) || agreed.contains(&first[w]) {
                    continue;
                }
                let anchor = anchor_of(w);
                placed.insert(*t, (anchor, sub(anchor, w)));
            }
            for w in group_extra {
                let anchor = anchor_of(&w);
                extra_placed.push((*group, anchor, sub(anchor, &w), w));
            }
        }
        // Groups that hold extra primaries but no root primary.
        for w in &extra {
            let group = group_of(w[0]);
            if !by_group.contains_key(&group) {
                let mut list: Vec<&Vec<u8>> =
                    extra.iter().filter(|x| group_of(x[0]) == group).collect();
                list.sort();
                list.dedup();
                let sub = list.iter().position(|x| *x == w).expect("listed") + 1;
                extra_placed.push((
                    group,
                    0,
                    u8::try_from(sub).expect("fewer than 255"),
                    w.clone(),
                ));
            }
        }
        let extra_ids: BTreeMap<Vec<u8>, u64> = extra_placed
            .iter()
            .enumerate()
            .map(|(i, (_, _, _, w))| (w.clone(), EXTRA | i as u64))
            .collect();
        let mut groups: Vec<(u64, u8)> = Vec::new();
        for (t, w) in &map {
            let g = group_of(w[0]);
            if groups.last().is_none_or(|(_, last)| *last != g) {
                groups.push((*t, g));
            }
        }

        // Check: the order the placements give is Windows' order of the primaries.
        let group_at = |t: u64| -> u8 {
            let at = groups.partition_point(|(start, _)| *start <= t);
            groups[at - 1].1
        };
        let slot = |t: u64| -> (u8, u64, u8) {
            let (anchor, sub) = placed.get(&t).copied().unwrap_or((t, 0));
            (group_at(t), anchor, sub)
        };
        let mut all: Vec<((u8, u64, u8), Vec<u8>)> =
            map.iter().map(|(t, w)| (slot(*t), w.clone())).collect();
        all.extend(
            extra_placed
                .iter()
                .map(|(g, a, s, w)| ((*g, *a, *s), w.clone())),
        );
        all.sort();
        for pair in all.windows(2) {
            let ((sa, wa), (sb, wb)) = (&pair[0], &pair[1]);
            assert_eq!(
                sa.cmp(sb),
                wa.cmp(wb),
                "the placements keep Windows' order: {wa:02X?} {wb:02X?}"
            );
        }

        let token_id = |w: &Vec<u8>| -> u64 {
            map.iter()
                .find(|(_, x)| *x == w)
                .map(|(t, _)| *t)
                .or_else(|| extra_ids.get(w).copied())
                .expect("a primary for every Windows primary")
        };

        // --- root_collation.rs: the Unicode data of the covered units the rules do not give.
        let mut data = Vec::new();
        let mut previous = 0u16;
        let mut stored = 0usize;
        for &c in &units {
            let r = &roots[&c];
            if r.rule || (0xD800..=0xDFFF).contains(&c) {
                continue;
            }
            varint(&mut data, u64::from(c - previous));
            previous = c;
            stored += 1;
            let flags = u8::from(r.new)
                | u8::from(r.wide) << 1
                | u8::from(r.narrow) << 2
                | u8::from(r.small) << 3
                | u8::from(r.katakana) << 4
                | u8::from(r.kana) << 5;
            data.push(flags);
            varint(&mut data, r.primaries.len() as u64);
            for t in &r.primaries {
                varint(&mut data, *t);
            }
            for list in &r.secondaries {
                varint(&mut data, list.len() as u64);
                for s in list {
                    varint(&mut data, u64::from(*s));
                }
            }
        }
        let mut new_ranges: Vec<(u16, u16)> = Vec::new();
        for &c in &units {
            if roots[&c].rule && roots[&c].new {
                match new_ranges.last_mut() {
                    Some((_, last)) if *last + 1 == c => *last = c,
                    _ => new_ranges.push((c, c)),
                }
            }
        }
        let root_source = format!(
            "// @generated by `cargo xtask collation generate` (beads dc-zsn.1 and dc-zsn.2). Do not edit by hand.\n\
             //\n\
             // Unicode data only: the CLDR root collation (the Unicode Collation Algorithm's default\n\
             // order as CLDR gives it; {}), read from the ICU that Windows ships,\n\
             // with the Unicode Character Database's ages, decomposition types and names.\n\
             // Copyright (c) 1991-2023 Unicode, Inc., under the Unicode License V3\n\
             // (deploy/notices/ported.txt).\n\
             //\n\
             // UNITS is base64 of {stored} records ({} bytes), one for each code unit the collation covers\n\
             // and does not give by rule, in code unit order, each: the code unit less the one before (a\n\
             // varint); a flags byte (1 assigned after Unicode 6.0 or unassigned, 2 a wide form, 4 a\n\
             // narrow form, 8 a small kana, 16 a katakana, 32 a kana); the count of its root primaries\n\
             // and each (varints; a Han character's is 3 << 48 | base << 32 | code point, base 0 for the\n\
             // unified ideographs and 1 for Extension A); then, for the secondaries before its first\n\
             // primary and for each primary, the count of non-common root secondaries and each.\n\
             // NEW is the Han code units assigned after Unicode 6.0.\n\n\
             pub(super) const NEW: [(u16, u16); {}] = [\n{}];\n\n\
             pub(super) const UNITS: &str = concat!(\n{});\n",
            icu.versions(),
            data.len(),
            new_ranges.len(),
            new_ranges
                .iter()
                .map(|(a, b)| format!("    (0x{a:04X}, 0x{b:04X}),\n"))
                .collect::<String>(),
            base64_lines(&data),
        );

        // --- windows_collation.rs: what was measured.
        let mut exception_data = Vec::new();
        let mut previous = 0u16;
        for (c, unit) in &exceptions {
            varint(&mut exception_data, u64::from(c - previous));
            previous = *c;
            match unit {
                Unit::Ignorable => exception_data.push(0),
                Unit::Refused => exception_data.push(1),
                Unit::Expansion(units, whole) => {
                    exception_data.extend_from_slice(&[5, u8::from(*whole)]);
                    varint(&mut exception_data, units.len() as u64);
                    for u in units {
                        varint(&mut exception_data, u64::from(*u));
                    }
                }
                Unit::Iteration {
                    vowel,
                    diacritic,
                    iteration,
                    width,
                } => {
                    exception_data.extend_from_slice(&[
                        6,
                        u8::from(*vowel),
                        *diacritic,
                        *iteration,
                    ]);
                    match width {
                        None => exception_data.push(0),
                        Some(w) => exception_data.extend_from_slice(&[1, *w]),
                    }
                }
                Unit::Mark(w) => exception_data.extend_from_slice(&[2, *w]),
                Unit::Special([a, b]) => exception_data.extend_from_slice(&[3, *a, *b]),
                Unit::Weighted {
                    primary,
                    diacritic,
                    case,
                    kana,
                } => {
                    exception_data.push(4);
                    varint(&mut exception_data, primary.len() as u64);
                    for w in primary {
                        varint(&mut exception_data, token_id(w));
                    }
                    varint(&mut exception_data, diacritic.len() as u64);
                    exception_data.extend_from_slice(diacritic);
                    varint(&mut exception_data, case.len() as u64);
                    exception_data.extend_from_slice(case);
                    match kana {
                        None => exception_data.push(0),
                        Some([s, t, w]) => exception_data.extend_from_slice(&[1, *s, *t, *w]),
                    }
                }
            }
        }
        let list = |name: &str, kind: &str, rows: Vec<String>| -> String {
            let mut text = format!(
                "#[rustfmt::skip]\npub(super) const {name}: [{kind}; {}] = [\n",
                rows.len()
            );
            for row in rows {
                let _ = writeln!(text, "    {row},");
            }
            text.push_str("];\n\n");
            text
        };
        let mut secondary_rows: Vec<(u32, u8)> = secondary.iter().map(|(s, w)| (*s, *w)).collect();
        secondary_rows.sort();
        let windows_source = format!(
            "// @generated by `cargo xtask collation generate` (beads dc-zsn.1 and dc-zsn.2). Do not edit by hand.\n\
             //\n\
             // What was measured of Windows' sort keys under {LOCALE} with NORM_IGNORECASE ({}),\n\
             // through LCMapStringEx on the machine Excel was measured on: the ways Windows departs\n\
             // from the root collation (root_collation.rs). No Windows table is copied: each entry\n\
             // is a measured fact about one code unit, one root weight or one range.\n\
             //\n\
             // COVERED: the code units the collation decides (the rest are refused).\n\
             // GROUPS: from each root primary on (in root order), the Windows group it falls in.\n\
             // PLACED: root primaries Windows orders otherwise than the root: each after its anchor\n\
             // (a root primary, or 0 for its group's start), numbered among those sharing it (0: equal\n\
             // to the anchor).\n\
             // EXTRA: primaries no root primary gives (an exception's): group, anchor and number; the\n\
             // n-th is the primary 4 << 48 | n.\n\
             // SECONDARY: the diacritic weight Windows gives each root secondary.\n\
             // SPECIAL: the special units and their weights. SURROGATES: ranges of surrogate units,\n\
             // ignorable or weighted with a diacritic and a case weight.\n\
             // EXCEPTIONS is base64 of {} records ({} bytes), one for each covered code unit whose\n\
             // Windows weights the rules do not give, in code unit order: the code unit less the one\n\
             // before (a varint); a tag (0 ignorable, 1 refused, 2 a mark and its weight, 3 a special\n\
             // and its two weights, 4 weighted: the count of its primaries and each (varints), the\n\
             // count of its diacritic entries (a varint) and each, the count of its case entries and\n\
             // each, then 0, or 1 and its small, type and width kana weights; 5 an expansion: 1\n\
             // for a whole decomposition (0 for one without its leading space), the count of its\n\
             // units and each (varints), the sequence CompareStringEx compares it as; 6\n\
             // an iteration mark: 1 for a prolonged-sound mark (the vowel), its diacritic entry, its\n\
             // iteration weight, then 0 (the width of the unit before it), or 1 and its own width).\n\n\
             {}{}{}{}{}{}{}\
             pub(super) const EXCEPTIONS: &str = concat!(\n{});\n",
            nls_version(),
            exceptions.len(),
            exception_data.len(),
            list(
                "COVERED",
                "(u16, u16)",
                COVERED
                    .iter()
                    .map(|(a, b)| format!("(0x{a:04X}, 0x{b:04X})"))
                    .collect()
            ),
            list(
                "GROUPS",
                "(u64, u8)",
                groups
                    .iter()
                    .map(|(t, g)| format!("(0x{t:X}, 0x{g:02X})"))
                    .collect()
            ),
            list(
                "PLACED",
                "(u64, u64, u8)",
                placed
                    .iter()
                    .map(|(t, (a, s))| format!("(0x{t:X}, 0x{a:X}, {s})"))
                    .collect()
            ),
            list(
                "EXTRA",
                "(u8, u64, u8)",
                extra_placed
                    .iter()
                    .map(|(g, a, s, _)| format!("(0x{g:02X}, 0x{a:X}, {s})"))
                    .collect()
            ),
            list(
                "SECONDARY",
                "(u32, u8)",
                secondary_rows
                    .iter()
                    .map(|(s, w)| format!("(0x{s:X}, {w})"))
                    .collect()
            ),
            list(
                "SPECIAL",
                "(u16, [u8; 2])",
                specials
                    .iter()
                    .map(|(c, [a, b])| format!("(0x{c:04X}, [0x{a:02X}, 0x{b:02X}])"))
                    .collect()
            ),
            list(
                "SURROGATES",
                "(u16, u16, Option<(u8, u8)>)",
                surrogate_rules
                    .iter()
                    .map(|(a, b, r)| match r {
                        None => format!("(0x{a:04X}, 0x{b:04X}, None)"),
                        Some((d, c)) => format!("(0x{a:04X}, 0x{b:04X}, Some(({d}, {c})))"),
                    })
                    .collect()
            ),
            base64_lines(&exception_data),
        );
        // The iteration marks' primaries, as primaries of the order above.
        let mut repeat_ranges: Vec<(u16, u16, u64)> = Vec::new();
        for (c, w) in &repeat_after {
            let id = token_id(w);
            match repeat_ranges.last_mut() {
                Some((_, last, t)) if *t == id && *last + 1 == *c => *last = *c,
                _ => repeat_ranges.push((*c, *c, id)),
            }
        }
        let windows_source = format!(
            "{windows_source}\n\
             // The primary an iteration or prolonged-sound mark takes (measured with U+309D and\n\
             // U+30FC after each unit): after a unit of one primary, that primary, but for the units\n\
             // of REPEAT (ranges, whose repeat the key writes otherwise) and, for a prolonged-sound\n\
             // mark, of VOWEL; after nothing or a unit of several primaries, NO_REPEAT. Marks and\n\
             // ignorable units between are passed, and so are other iteration marks.\n\n\
             {}{}\
             pub(super) const NO_REPEAT: u64 = 0x{:X};\n",
            list(
                "REPEAT",
                "(u16, u16, u64)",
                repeat_ranges
                    .iter()
                    .map(|(a, b, t)| format!("(0x{a:04X}, 0x{b:04X}, 0x{t:X})"))
                    .collect()
            ),
            list(
                "VOWEL",
                "(u16, u64)",
                vowel_after
                    .iter()
                    .map(|(c, w)| format!("(0x{c:04X}, 0x{:X})", token_id(w)))
                    .collect()
            ),
            token_id(&none_token),
        );
        for (path, source) in [(ROOT_OUT, &root_source), (WINDOWS_OUT, &windows_source)] {
            if let Err(error) = std::fs::write(path, source) {
                eprintln!("xtask collation: writing {path}: {error}");
                return ExitCode::FAILURE;
            }
        }
        println!(
            "xtask collation: {} covered units; root data for {stored} ({} bytes); measured: {} group runs, {} placed root primaries, {} extra primaries, {} secondary weights, {} specials, {} surrogate ranges, {} unit exceptions ({} bytes)",
            units.len(),
            data.len(),
            groups.len(),
            placed.len(),
            extra_placed.len(),
            secondary.len(),
            specials.len(),
            surrogate_rules.len(),
            exceptions.len(),
            exception_data.len(),
        );
        ExitCode::SUCCESS
    }

    /// xorshift64.
    struct Draws(u64);

    impl Draws {
        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }
    }

    pub fn verify(count: u32, seed: u64) -> ExitCode {
        let range = |a: u16, b: u16| (a..=b).collect::<Vec<u16>>();
        let pools: Vec<(&str, Vec<u16>)> = vec![
            ("ascii", range(0x20, 0x7E)),
            (
                "latin and marks",
                [
                    range(0x20, 0x7E),
                    range(0xA0, 0x24F),
                    range(0x300, 0x36F),
                    range(0x1E00, 0x1EFF),
                ]
                .concat(),
            ),
            (
                "greek",
                [range(0x370, 0x3FF), range(0x1F00, 0x1FFF)].concat(),
            ),
            ("cyrillic", range(0x400, 0x52F)),
            (
                "cjk",
                [
                    range(0x4E00, 0x9FFF),
                    range(0xF900, 0xFAFF),
                    range(0x3000, 0x303F),
                ]
                .concat(),
            ),
            (
                "kana",
                [range(0x3040, 0x30FF), range(0xFF61, 0xFF9F)].concat(),
            ),
            (
                "full width",
                [range(0xFF01, 0xFF5E), range(0x41, 0x5A)].concat(),
            ),
            ("symbols", range(0x2000, 0x2BFF)),
            ("hebrew and arabic", range(0x590, 0x6FF)),
            ("indic", range(0x900, 0xDFF)),
            ("thai, lao, tibetan", range(0xE00, 0xFFF)),
            (
                "hangul",
                [
                    range(0x1100, 0x11FF),
                    range(0xAC00, 0xD7A3),
                    range(0x3130, 0x318F),
                ]
                .concat(),
            ),
            (
                "surrogates",
                [range(0xD800, 0xDFFF), range(0x61, 0x7A)].concat(),
            ),
            (
                "ascii and iteration marks",
                vec![
                    0x61, 0x62, 0x41, 0x2D, 0x301, 0x30FC, 0xFF70, 0x309D, 0x309E, 0x3005, 0x304B,
                    0x30AB, 0xFF76, 0x6642,
                ],
            ),
            (
                "marks, specials and ignorables",
                vec![
                    0x61, 0x62, 0x65, 0x41, 0xE1, 0xE6, 0x301, 0x300, 0x308, 0x30F, 0x327, 0x2D,
                    0x27, 0xAD, 0x200B, 0xE000, 0x5D0, 0x5B4, 0x3099, 0x304B,
                ],
            ),
            (
                "arabic presentation",
                [
                    range(0x600, 0x6FF),
                    range(0xFB50, 0xFDFF),
                    range(0xFE70, 0xFEFF),
                ]
                .concat(),
            ),
            (
                "hebrew presentation",
                [range(0x590, 0x5FF), range(0xFB1D, 0xFB4F)].concat(),
            ),
            (
                "private use",
                [
                    range(0xE000, 0xF8FF),
                    range(0x61, 0x7A),
                    range(0x300, 0x36F),
                ]
                .concat(),
            ),
            ("other scripts", range(0x1000, 0x1FFF)),
            ("yi, vai and the rest", range(0xA000, 0xABFF)),
            ("every unit", range(0x0001, 0xFFFF)),
        ];
        let mut draws = Draws(seed);
        let mut failed = false;
        for (name, pool) in &pools {
            let (mut decided, mut refused, mut wrong) = (0u32, 0u32, 0u32);
            let mut reasons: BTreeMap<String, u32> = BTreeMap::new();
            let mut example = None;
            for _ in 0..count {
                let text = |draws: &mut Draws| -> Vec<u16> {
                    let len = 1 + draws.below(7);
                    (0..len).map(|_| pool[draws.below(pool.len())]).collect()
                };
                let a = text(&mut draws);
                // Half the pairs differ in one unit, so the later units decide.
                let b = if draws.below(2) == 0 {
                    let mut b = a.clone();
                    let at = draws.below(b.len());
                    b[at] = pool[draws.below(pool.len())];
                    b
                } else {
                    text(&mut draws)
                };
                match calc_func::text_order(&a, &b) {
                    None => {
                        refused += 1;
                        {
                            for t in [&a, &b] {
                                let units: Vec<u16> = t
                                    .iter()
                                    .copied()
                                    .filter(|&u| calc_func::text_order(&[u], &[0x7C]).is_none())
                                    .collect();
                                if !units.is_empty() {
                                    for u in units {
                                        *reasons.entry(format!("unit {u:04X}")).or_default() += 1;
                                    }
                                } else if calc_func::text_order(t, &[0x7C]).is_none() {
                                    *reasons.entry(format!("context {t:04X?}")).or_default() += 1;
                                }
                            }
                        }
                    }
                    Some(order) => {
                        decided += 1;
                        if order != excel_order(&a, &b) {
                            wrong += 1;
                            example.get_or_insert((a.clone(), b.clone()));
                        }
                    }
                }
            }
            println!(
                "xtask collation: {name}: {decided} pairs decided, {refused} refused, {wrong} wrong"
            );
            if !reasons.is_empty() {
                let mut v: Vec<_> = reasons.iter().collect();
                v.sort_by(|x, y| y.1.cmp(x.1));
                let units: Vec<String> = v
                    .iter()
                    .filter(|(k, _)| k.starts_with("unit"))
                    .take(40)
                    .map(|(k, n)| format!("{}:{n}", &k[5..]))
                    .collect();
                let ctx: Vec<&String> = v
                    .iter()
                    .filter(|(k, _)| k.starts_with("context"))
                    .take(8)
                    .map(|(k, _)| *k)
                    .collect();
                println!("  refused units: {}", units.join(" "));
                for c in ctx {
                    println!("  {c}");
                }
            }
            if let Some((a, b)) = example {
                println!("  first wrong pair: {a:04X?} / {b:04X?}");
                failed = true;
            }
        }
        if failed {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        }
    }
}
