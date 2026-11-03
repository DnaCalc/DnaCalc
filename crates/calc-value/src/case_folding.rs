//! Unicode case-folding table behind [`crate::name_case`] (dc-cpr, ported from OxFml 15daac5
//! `crates/oxfml_core/src/binding/case_folding.rs`, W079 lane 7, `fml-kt8.7`).
//!
//! Two folds from `CaseFolding.txt`, both without the Turkic `T` mappings:
//!
//! - **simple** (status `C` + `S`): one code point to one code point. `ẞ`
//!   (U+1E9E) folds to `ß`, `ς` to `σ`, `İ` (U+0130) and `ı` (U+0131) stay as
//!   they are, `ß` stays `ß`.
//! - **full** (status `C` + `F`): the `F` rows expand to two or three code
//!   points — `ß` and `ẞ` both fold to `ss`, `İ` to `i̇` (`i` + U+0307), the
//!   ligatures `ﬁ`/`ﬀ` to `fi`/`ff`. Every code point without an `F` row folds
//!   as in the simple fold.
//!
//! Generated from `CaseFolding-15.0.0.txt` (2022-02-02;
//! `https://www.unicode.org/Public/15.0.0/ucd/CaseFolding.txt`, sha256
//! `cdd49e55eae3bbf1f0a3f6580c974a0263cb86a6a08daa10fbf705b4808a56f7`) on
//! 2026-09-15 — the same Unicode version as `combining_marks.rs`:
//!
//! ```text
//! python gen.py CaseFolding-15.0.0.txt   # gen.py:
//! import sys
//! simple, full = {}, {}
//! for line in open(sys.argv[1], encoding='utf-8'):
//!     line = line.split('#')[0].strip()
//!     if not line: continue
//!     cp, status, mapping, _ = [p.strip() for p in line.split(';')]
//!     cp, m = int(cp, 16), [int(x, 16) for x in mapping.split()]
//!     if status in ('C', 'S'): simple[cp] = m[0]
//!     elif status == 'F': full[cp] = m
//! cps, runs, i = sorted(simple), [], 0
//! while i < len(cps):
//!     cp, delta = cps[i], simple[cps[i]] - cps[i]
//!     best = (cp, cp, 1, 1)
//!     for stride in (1, 2):
//!         j, n = i, 1
//!         while j + 1 < len(cps) and cps[j+1] == cps[j] + stride and simple[cps[j+1]] - cps[j+1] == delta:
//!             j, n = j + 1, n + 1
//!         if n > best[3]: best = (cp, cps[j], stride, n)
//!     runs.append((best[0], best[1], best[2], delta)); i += best[3]
//! print("const SIMPLE_FOLD_RUNS: &[(u32, u32, u8, i32)] = &[")
//! for lo, hi, stride, delta in runs: print(f"    (0x{lo:04X}, 0x{hi:04X}, {stride}, {delta}),")
//! print("];")
//! print("const FULL_FOLDS: &[(u32, [u32; 3])] = &[")
//! for cp in sorted(full):
//!     m = full[cp] + [0] * (3 - len(full[cp]))
//!     print(f"    (0x{cp:04X}, [0x{m[0]:04X}, 0x{m[1]:04X}, 0x{m[2]:04X}]),")
//! print("];")
//! ```
//!
//! Cross-check at generation time: Python 3.12.10 `str.casefold()`
//! (`unicodedata.unidata_version == "15.0.0"`) agrees with the `C` + `F`
//! mapping on all 0x110000 code points (0 mismatches).
//!
//! `SIMPLE_FOLD_RUNS` packs the 1,454 `C`/`S` rows into 202 runs
//! `(start, end, stride, delta)`: every code point `start + k * stride <= end`
//! folds to itself `+ delta`; a code point inside a stride-2 run but off its
//! stride has no row and folds to itself. Runs are sorted and disjoint (the
//! lookup relies on it). `FULL_FOLDS` lists the 104 `F` rows, sorted, with the
//! mapping zero-padded to three code points.

use std::cmp::Ordering;

/// Simple case fold (`C` + `S`) of one character.
pub(crate) fn simple_fold(ch: char) -> char {
    let code_point = ch as u32;
    let Ok(index) = SIMPLE_FOLD_RUNS.binary_search_by(|&(start, end, _, _)| {
        if code_point < start {
            Ordering::Greater
        } else if code_point > end {
            Ordering::Less
        } else {
            Ordering::Equal
        }
    }) else {
        return ch;
    };
    let (start, _, stride, delta) = SIMPLE_FOLD_RUNS[index];
    if !(code_point - start).is_multiple_of(u32::from(stride)) {
        return ch;
    }
    char::from_u32(code_point.wrapping_add_signed(delta)).unwrap_or(ch)
}

/// Full case fold (`C` + `F`) of one character, appended to `out`.
pub(crate) fn push_full_fold(ch: char, out: &mut String) {
    let code_point = ch as u32;
    match FULL_FOLDS.binary_search_by_key(&code_point, |&(from, _)| from) {
        Ok(index) => {
            for mapped in FULL_FOLDS[index].1 {
                if mapped == 0 {
                    break;
                }
                out.push(char::from_u32(mapped).unwrap_or(ch));
            }
        }
        Err(_) => out.push(simple_fold(ch)),
    }
}

const SIMPLE_FOLD_RUNS: &[(u32, u32, u8, i32)] = &[
    (0x0041, 0x005A, 1, 32),
    (0x00B5, 0x00B5, 1, 775),
    (0x00C0, 0x00D6, 1, 32),
    (0x00D8, 0x00DE, 1, 32),
    (0x0100, 0x012E, 2, 1),
    (0x0132, 0x0136, 2, 1),
    (0x0139, 0x0147, 2, 1),
    (0x014A, 0x0176, 2, 1),
    (0x0178, 0x0178, 1, -121),
    (0x0179, 0x017D, 2, 1),
    (0x017F, 0x017F, 1, -268),
    (0x0181, 0x0181, 1, 210),
    (0x0182, 0x0184, 2, 1),
    (0x0186, 0x0186, 1, 206),
    (0x0187, 0x0187, 1, 1),
    (0x0189, 0x018A, 1, 205),
    (0x018B, 0x018B, 1, 1),
    (0x018E, 0x018E, 1, 79),
    (0x018F, 0x018F, 1, 202),
    (0x0190, 0x0190, 1, 203),
    (0x0191, 0x0191, 1, 1),
    (0x0193, 0x0193, 1, 205),
    (0x0194, 0x0194, 1, 207),
    (0x0196, 0x0196, 1, 211),
    (0x0197, 0x0197, 1, 209),
    (0x0198, 0x0198, 1, 1),
    (0x019C, 0x019C, 1, 211),
    (0x019D, 0x019D, 1, 213),
    (0x019F, 0x019F, 1, 214),
    (0x01A0, 0x01A4, 2, 1),
    (0x01A6, 0x01A6, 1, 218),
    (0x01A7, 0x01A7, 1, 1),
    (0x01A9, 0x01A9, 1, 218),
    (0x01AC, 0x01AC, 1, 1),
    (0x01AE, 0x01AE, 1, 218),
    (0x01AF, 0x01AF, 1, 1),
    (0x01B1, 0x01B2, 1, 217),
    (0x01B3, 0x01B5, 2, 1),
    (0x01B7, 0x01B7, 1, 219),
    (0x01B8, 0x01B8, 1, 1),
    (0x01BC, 0x01BC, 1, 1),
    (0x01C4, 0x01C4, 1, 2),
    (0x01C5, 0x01C5, 1, 1),
    (0x01C7, 0x01C7, 1, 2),
    (0x01C8, 0x01C8, 1, 1),
    (0x01CA, 0x01CA, 1, 2),
    (0x01CB, 0x01DB, 2, 1),
    (0x01DE, 0x01EE, 2, 1),
    (0x01F1, 0x01F1, 1, 2),
    (0x01F2, 0x01F4, 2, 1),
    (0x01F6, 0x01F6, 1, -97),
    (0x01F7, 0x01F7, 1, -56),
    (0x01F8, 0x021E, 2, 1),
    (0x0220, 0x0220, 1, -130),
    (0x0222, 0x0232, 2, 1),
    (0x023A, 0x023A, 1, 10795),
    (0x023B, 0x023B, 1, 1),
    (0x023D, 0x023D, 1, -163),
    (0x023E, 0x023E, 1, 10792),
    (0x0241, 0x0241, 1, 1),
    (0x0243, 0x0243, 1, -195),
    (0x0244, 0x0244, 1, 69),
    (0x0245, 0x0245, 1, 71),
    (0x0246, 0x024E, 2, 1),
    (0x0345, 0x0345, 1, 116),
    (0x0370, 0x0372, 2, 1),
    (0x0376, 0x0376, 1, 1),
    (0x037F, 0x037F, 1, 116),
    (0x0386, 0x0386, 1, 38),
    (0x0388, 0x038A, 1, 37),
    (0x038C, 0x038C, 1, 64),
    (0x038E, 0x038F, 1, 63),
    (0x0391, 0x03A1, 1, 32),
    (0x03A3, 0x03AB, 1, 32),
    (0x03C2, 0x03C2, 1, 1),
    (0x03CF, 0x03CF, 1, 8),
    (0x03D0, 0x03D0, 1, -30),
    (0x03D1, 0x03D1, 1, -25),
    (0x03D5, 0x03D5, 1, -15),
    (0x03D6, 0x03D6, 1, -22),
    (0x03D8, 0x03EE, 2, 1),
    (0x03F0, 0x03F0, 1, -54),
    (0x03F1, 0x03F1, 1, -48),
    (0x03F4, 0x03F4, 1, -60),
    (0x03F5, 0x03F5, 1, -64),
    (0x03F7, 0x03F7, 1, 1),
    (0x03F9, 0x03F9, 1, -7),
    (0x03FA, 0x03FA, 1, 1),
    (0x03FD, 0x03FF, 1, -130),
    (0x0400, 0x040F, 1, 80),
    (0x0410, 0x042F, 1, 32),
    (0x0460, 0x0480, 2, 1),
    (0x048A, 0x04BE, 2, 1),
    (0x04C0, 0x04C0, 1, 15),
    (0x04C1, 0x04CD, 2, 1),
    (0x04D0, 0x052E, 2, 1),
    (0x0531, 0x0556, 1, 48),
    (0x10A0, 0x10C5, 1, 7264),
    (0x10C7, 0x10C7, 1, 7264),
    (0x10CD, 0x10CD, 1, 7264),
    (0x13F8, 0x13FD, 1, -8),
    (0x1C80, 0x1C80, 1, -6222),
    (0x1C81, 0x1C81, 1, -6221),
    (0x1C82, 0x1C82, 1, -6212),
    (0x1C83, 0x1C84, 1, -6210),
    (0x1C85, 0x1C85, 1, -6211),
    (0x1C86, 0x1C86, 1, -6204),
    (0x1C87, 0x1C87, 1, -6180),
    (0x1C88, 0x1C88, 1, 35267),
    (0x1C90, 0x1CBA, 1, -3008),
    (0x1CBD, 0x1CBF, 1, -3008),
    (0x1E00, 0x1E94, 2, 1),
    (0x1E9B, 0x1E9B, 1, -58),
    (0x1E9E, 0x1E9E, 1, -7615),
    (0x1EA0, 0x1EFE, 2, 1),
    (0x1F08, 0x1F0F, 1, -8),
    (0x1F18, 0x1F1D, 1, -8),
    (0x1F28, 0x1F2F, 1, -8),
    (0x1F38, 0x1F3F, 1, -8),
    (0x1F48, 0x1F4D, 1, -8),
    (0x1F59, 0x1F5F, 2, -8),
    (0x1F68, 0x1F6F, 1, -8),
    (0x1F88, 0x1F8F, 1, -8),
    (0x1F98, 0x1F9F, 1, -8),
    (0x1FA8, 0x1FAF, 1, -8),
    (0x1FB8, 0x1FB9, 1, -8),
    (0x1FBA, 0x1FBB, 1, -74),
    (0x1FBC, 0x1FBC, 1, -9),
    (0x1FBE, 0x1FBE, 1, -7173),
    (0x1FC8, 0x1FCB, 1, -86),
    (0x1FCC, 0x1FCC, 1, -9),
    (0x1FD8, 0x1FD9, 1, -8),
    (0x1FDA, 0x1FDB, 1, -100),
    (0x1FE8, 0x1FE9, 1, -8),
    (0x1FEA, 0x1FEB, 1, -112),
    (0x1FEC, 0x1FEC, 1, -7),
    (0x1FF8, 0x1FF9, 1, -128),
    (0x1FFA, 0x1FFB, 1, -126),
    (0x1FFC, 0x1FFC, 1, -9),
    (0x2126, 0x2126, 1, -7517),
    (0x212A, 0x212A, 1, -8383),
    (0x212B, 0x212B, 1, -8262),
    (0x2132, 0x2132, 1, 28),
    (0x2160, 0x216F, 1, 16),
    (0x2183, 0x2183, 1, 1),
    (0x24B6, 0x24CF, 1, 26),
    (0x2C00, 0x2C2F, 1, 48),
    (0x2C60, 0x2C60, 1, 1),
    (0x2C62, 0x2C62, 1, -10743),
    (0x2C63, 0x2C63, 1, -3814),
    (0x2C64, 0x2C64, 1, -10727),
    (0x2C67, 0x2C6B, 2, 1),
    (0x2C6D, 0x2C6D, 1, -10780),
    (0x2C6E, 0x2C6E, 1, -10749),
    (0x2C6F, 0x2C6F, 1, -10783),
    (0x2C70, 0x2C70, 1, -10782),
    (0x2C72, 0x2C72, 1, 1),
    (0x2C75, 0x2C75, 1, 1),
    (0x2C7E, 0x2C7F, 1, -10815),
    (0x2C80, 0x2CE2, 2, 1),
    (0x2CEB, 0x2CED, 2, 1),
    (0x2CF2, 0x2CF2, 1, 1),
    (0xA640, 0xA66C, 2, 1),
    (0xA680, 0xA69A, 2, 1),
    (0xA722, 0xA72E, 2, 1),
    (0xA732, 0xA76E, 2, 1),
    (0xA779, 0xA77B, 2, 1),
    (0xA77D, 0xA77D, 1, -35332),
    (0xA77E, 0xA786, 2, 1),
    (0xA78B, 0xA78B, 1, 1),
    (0xA78D, 0xA78D, 1, -42280),
    (0xA790, 0xA792, 2, 1),
    (0xA796, 0xA7A8, 2, 1),
    (0xA7AA, 0xA7AA, 1, -42308),
    (0xA7AB, 0xA7AB, 1, -42319),
    (0xA7AC, 0xA7AC, 1, -42315),
    (0xA7AD, 0xA7AD, 1, -42305),
    (0xA7AE, 0xA7AE, 1, -42308),
    (0xA7B0, 0xA7B0, 1, -42258),
    (0xA7B1, 0xA7B1, 1, -42282),
    (0xA7B2, 0xA7B2, 1, -42261),
    (0xA7B3, 0xA7B3, 1, 928),
    (0xA7B4, 0xA7C2, 2, 1),
    (0xA7C4, 0xA7C4, 1, -48),
    (0xA7C5, 0xA7C5, 1, -42307),
    (0xA7C6, 0xA7C6, 1, -35384),
    (0xA7C7, 0xA7C9, 2, 1),
    (0xA7D0, 0xA7D0, 1, 1),
    (0xA7D6, 0xA7D8, 2, 1),
    (0xA7F5, 0xA7F5, 1, 1),
    (0xAB70, 0xABBF, 1, -38864),
    (0xFF21, 0xFF3A, 1, 32),
    (0x10400, 0x10427, 1, 40),
    (0x104B0, 0x104D3, 1, 40),
    (0x10570, 0x1057A, 1, 39),
    (0x1057C, 0x1058A, 1, 39),
    (0x1058C, 0x10592, 1, 39),
    (0x10594, 0x10595, 1, 39),
    (0x10C80, 0x10CB2, 1, 64),
    (0x118A0, 0x118BF, 1, 32),
    (0x16E40, 0x16E5F, 1, 32),
    (0x1E900, 0x1E921, 1, 34),
];
const FULL_FOLDS: &[(u32, [u32; 3])] = &[
    (0x00DF, [0x0073, 0x0073, 0x0000]),
    (0x0130, [0x0069, 0x0307, 0x0000]),
    (0x0149, [0x02BC, 0x006E, 0x0000]),
    (0x01F0, [0x006A, 0x030C, 0x0000]),
    (0x0390, [0x03B9, 0x0308, 0x0301]),
    (0x03B0, [0x03C5, 0x0308, 0x0301]),
    (0x0587, [0x0565, 0x0582, 0x0000]),
    (0x1E96, [0x0068, 0x0331, 0x0000]),
    (0x1E97, [0x0074, 0x0308, 0x0000]),
    (0x1E98, [0x0077, 0x030A, 0x0000]),
    (0x1E99, [0x0079, 0x030A, 0x0000]),
    (0x1E9A, [0x0061, 0x02BE, 0x0000]),
    (0x1E9E, [0x0073, 0x0073, 0x0000]),
    (0x1F50, [0x03C5, 0x0313, 0x0000]),
    (0x1F52, [0x03C5, 0x0313, 0x0300]),
    (0x1F54, [0x03C5, 0x0313, 0x0301]),
    (0x1F56, [0x03C5, 0x0313, 0x0342]),
    (0x1F80, [0x1F00, 0x03B9, 0x0000]),
    (0x1F81, [0x1F01, 0x03B9, 0x0000]),
    (0x1F82, [0x1F02, 0x03B9, 0x0000]),
    (0x1F83, [0x1F03, 0x03B9, 0x0000]),
    (0x1F84, [0x1F04, 0x03B9, 0x0000]),
    (0x1F85, [0x1F05, 0x03B9, 0x0000]),
    (0x1F86, [0x1F06, 0x03B9, 0x0000]),
    (0x1F87, [0x1F07, 0x03B9, 0x0000]),
    (0x1F88, [0x1F00, 0x03B9, 0x0000]),
    (0x1F89, [0x1F01, 0x03B9, 0x0000]),
    (0x1F8A, [0x1F02, 0x03B9, 0x0000]),
    (0x1F8B, [0x1F03, 0x03B9, 0x0000]),
    (0x1F8C, [0x1F04, 0x03B9, 0x0000]),
    (0x1F8D, [0x1F05, 0x03B9, 0x0000]),
    (0x1F8E, [0x1F06, 0x03B9, 0x0000]),
    (0x1F8F, [0x1F07, 0x03B9, 0x0000]),
    (0x1F90, [0x1F20, 0x03B9, 0x0000]),
    (0x1F91, [0x1F21, 0x03B9, 0x0000]),
    (0x1F92, [0x1F22, 0x03B9, 0x0000]),
    (0x1F93, [0x1F23, 0x03B9, 0x0000]),
    (0x1F94, [0x1F24, 0x03B9, 0x0000]),
    (0x1F95, [0x1F25, 0x03B9, 0x0000]),
    (0x1F96, [0x1F26, 0x03B9, 0x0000]),
    (0x1F97, [0x1F27, 0x03B9, 0x0000]),
    (0x1F98, [0x1F20, 0x03B9, 0x0000]),
    (0x1F99, [0x1F21, 0x03B9, 0x0000]),
    (0x1F9A, [0x1F22, 0x03B9, 0x0000]),
    (0x1F9B, [0x1F23, 0x03B9, 0x0000]),
    (0x1F9C, [0x1F24, 0x03B9, 0x0000]),
    (0x1F9D, [0x1F25, 0x03B9, 0x0000]),
    (0x1F9E, [0x1F26, 0x03B9, 0x0000]),
    (0x1F9F, [0x1F27, 0x03B9, 0x0000]),
    (0x1FA0, [0x1F60, 0x03B9, 0x0000]),
    (0x1FA1, [0x1F61, 0x03B9, 0x0000]),
    (0x1FA2, [0x1F62, 0x03B9, 0x0000]),
    (0x1FA3, [0x1F63, 0x03B9, 0x0000]),
    (0x1FA4, [0x1F64, 0x03B9, 0x0000]),
    (0x1FA5, [0x1F65, 0x03B9, 0x0000]),
    (0x1FA6, [0x1F66, 0x03B9, 0x0000]),
    (0x1FA7, [0x1F67, 0x03B9, 0x0000]),
    (0x1FA8, [0x1F60, 0x03B9, 0x0000]),
    (0x1FA9, [0x1F61, 0x03B9, 0x0000]),
    (0x1FAA, [0x1F62, 0x03B9, 0x0000]),
    (0x1FAB, [0x1F63, 0x03B9, 0x0000]),
    (0x1FAC, [0x1F64, 0x03B9, 0x0000]),
    (0x1FAD, [0x1F65, 0x03B9, 0x0000]),
    (0x1FAE, [0x1F66, 0x03B9, 0x0000]),
    (0x1FAF, [0x1F67, 0x03B9, 0x0000]),
    (0x1FB2, [0x1F70, 0x03B9, 0x0000]),
    (0x1FB3, [0x03B1, 0x03B9, 0x0000]),
    (0x1FB4, [0x03AC, 0x03B9, 0x0000]),
    (0x1FB6, [0x03B1, 0x0342, 0x0000]),
    (0x1FB7, [0x03B1, 0x0342, 0x03B9]),
    (0x1FBC, [0x03B1, 0x03B9, 0x0000]),
    (0x1FC2, [0x1F74, 0x03B9, 0x0000]),
    (0x1FC3, [0x03B7, 0x03B9, 0x0000]),
    (0x1FC4, [0x03AE, 0x03B9, 0x0000]),
    (0x1FC6, [0x03B7, 0x0342, 0x0000]),
    (0x1FC7, [0x03B7, 0x0342, 0x03B9]),
    (0x1FCC, [0x03B7, 0x03B9, 0x0000]),
    (0x1FD2, [0x03B9, 0x0308, 0x0300]),
    (0x1FD3, [0x03B9, 0x0308, 0x0301]),
    (0x1FD6, [0x03B9, 0x0342, 0x0000]),
    (0x1FD7, [0x03B9, 0x0308, 0x0342]),
    (0x1FE2, [0x03C5, 0x0308, 0x0300]),
    (0x1FE3, [0x03C5, 0x0308, 0x0301]),
    (0x1FE4, [0x03C1, 0x0313, 0x0000]),
    (0x1FE6, [0x03C5, 0x0342, 0x0000]),
    (0x1FE7, [0x03C5, 0x0308, 0x0342]),
    (0x1FF2, [0x1F7C, 0x03B9, 0x0000]),
    (0x1FF3, [0x03C9, 0x03B9, 0x0000]),
    (0x1FF4, [0x03CE, 0x03B9, 0x0000]),
    (0x1FF6, [0x03C9, 0x0342, 0x0000]),
    (0x1FF7, [0x03C9, 0x0342, 0x03B9]),
    (0x1FFC, [0x03C9, 0x03B9, 0x0000]),
    (0xFB00, [0x0066, 0x0066, 0x0000]),
    (0xFB01, [0x0066, 0x0069, 0x0000]),
    (0xFB02, [0x0066, 0x006C, 0x0000]),
    (0xFB03, [0x0066, 0x0066, 0x0069]),
    (0xFB04, [0x0066, 0x0066, 0x006C]),
    (0xFB05, [0x0073, 0x0074, 0x0000]),
    (0xFB06, [0x0073, 0x0074, 0x0000]),
    (0xFB13, [0x0574, 0x0576, 0x0000]),
    (0xFB14, [0x0574, 0x0565, 0x0000]),
    (0xFB15, [0x0574, 0x056B, 0x0000]),
    (0xFB16, [0x057E, 0x0576, 0x0000]),
    (0xFB17, [0x0574, 0x056D, 0x0000]),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn full(ch: char) -> String {
        let mut out = String::new();
        push_full_fold(ch, &mut out);
        out
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ascii_letters_fold_to_lowercase_and_every_other_ascii_char_is_fixed() {
        for ch in 'A'..='Z' {
            assert_eq!(simple_fold(ch), ch.to_ascii_lowercase());
            assert_eq!(full(ch), ch.to_ascii_lowercase().to_string());
        }
        for ch in ('\0'..='\u{7f}').filter(|ch| !ch.is_ascii_uppercase()) {
            assert_eq!(simple_fold(ch), ch, "{ch:?}");
            assert_eq!(full(ch), ch.to_string(), "{ch:?}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn simple_fold_matches_the_c_and_s_rows() {
        // C rows.
        assert_eq!(simple_fold('Ö'), 'ö');
        assert_eq!(simple_fold('Σ'), 'σ');
        assert_eq!(simple_fold('ς'), 'σ', "final sigma folds to sigma");
        assert_eq!(simple_fold('Д'), 'д');
        assert_eq!(simple_fold('\u{212A}'), 'k', "KELVIN SIGN");
        assert_eq!(simple_fold('\u{13A0}'), '\u{13A0}', "Cherokee folds upward");
        assert_eq!(simple_fold('\u{AB70}'), '\u{13A0}');
        // S rows.
        assert_eq!(simple_fold('\u{1E9E}'), 'ß', "capital sharp s");
        assert_eq!(simple_fold('\u{1F88}'), '\u{1F80}');
        // Rows that only exist as F or T: unchanged under the simple fold.
        assert_eq!(simple_fold('ß'), 'ß');
        assert_eq!(simple_fold('\u{0130}'), '\u{0130}', "I WITH DOT ABOVE");
        assert_eq!(simple_fold('\u{0131}'), '\u{0131}', "DOTLESS I");
        // Stride-2 run: Latin Extended-A alternates upper/lower.
        assert_eq!(simple_fold('\u{0100}'), '\u{0101}');
        assert_eq!(
            simple_fold('\u{0101}'),
            '\u{0101}',
            "off-stride: already lower"
        );
        assert_eq!(simple_fold('\u{0102}'), '\u{0103}');
        // Unmapped scripts and non-letters are fixed points.
        for ch in ['売', '1', '_', '.', '$', '😀', ' ', '\u{0301}'] {
            assert_eq!(simple_fold(ch), ch, "{ch:?}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn full_fold_expands_the_f_rows_and_defers_to_simple_fold_otherwise() {
        assert_eq!(full('ß'), "ss");
        assert_eq!(full('\u{1E9E}'), "ss");
        assert_eq!(full('\u{0130}'), "i\u{0307}");
        assert_eq!(full('\u{FB01}'), "fi");
        assert_eq!(
            full('\u{0390}'),
            "\u{03B9}\u{0308}\u{0301}",
            "three code points"
        );
        assert_eq!(full('ς'), "σ");
        assert_eq!(full('Ö'), "ö");
        assert_eq!(full('\u{0131}'), "\u{0131}");
        assert_eq!(full('売'), "売");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn tables_are_sorted_disjoint_and_carry_every_row() {
        for window in SIMPLE_FOLD_RUNS.windows(2) {
            assert!(
                window[0].1 < window[1].0,
                "{:?} / {:?}",
                window[0],
                window[1]
            );
        }
        for &(start, end, stride, _) in SIMPLE_FOLD_RUNS {
            assert!(start <= end && (stride == 1 || stride == 2));
            assert!((end - start).is_multiple_of(u32::from(stride)));
        }
        for window in FULL_FOLDS.windows(2) {
            assert!(window[0].0 < window[1].0);
        }
        assert_eq!(FULL_FOLDS.len(), 104);
        assert_eq!(SIMPLE_FOLD_RUNS.len(), 202);
        assert_eq!(
            SIMPLE_FOLD_RUNS
                .iter()
                .map(|&(start, end, stride, _)| (end - start) / u32::from(stride) + 1)
                .sum::<u32>(),
            1454
        );
    }
}
