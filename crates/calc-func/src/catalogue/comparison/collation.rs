//! Excel's comparison of two texts, as the comparison operators give it under the Windows locale
//! en-ZA (another locale may collate differently).
//!
//! The comparison reads the two texts' UTF-16 code units in two passes.
//!
//! The first pass walks both texts from the start while both code units are at most `z`
//! (U+007A), the letters compared ignoring case and every other code unit as itself. Where they
//! differ, or where either code unit is above `z`, the second pass decides. When the shorter text
//! runs out first it is the lesser, and texts of the same length are equal: so `w` is less than
//! `w` followed by a soft hyphen, while the second pass, which ignores a soft hyphen, holds e with
//! an acute accent (U+00E9) equal to itself followed by one.
//!
//! The second pass compares weights, level by level. Each character has a primary weight (two for
//! a ligature such as U+00E6, U+00DF or U+00DE, which weighs as ae, ss or th), a diacritic weight
//! and a case weight that ignores upper and lower case (a superscript digit or an ordinal indicator
//! keeps one); or it is special (the apostrophe, the hyphen and the dashes, the minus sign, and the
//! control characters other than tab, line feed, vertical tab, form feed and carriage return); or
//! it is ignorable (the soft hyphen, the zero-width non-joiner and joiner, the left-to-right and
//! right-to-left marks, the word joiner, the zero-width no-break space and the ruble sign). The
//! texts' primary weights are compared in order, a text whose weights run out first being the
//! lesser; when they are equal, their diacritic weights, then their case weights; when those are
//! equal too, their special characters in order, each by its position (the count of primary
//! weights before it, an earlier position the greater) and then by its weight. So `a-b` follows
//! `ab` and precedes `ac`, and `co-op` follows `coop`.
//!
//! [`compare`] decides texts of the repertoire: U+0001 to U+007F, U+00A0 to U+017F, and the
//! characters of [`WEIGHTS`] beyond them (common punctuation, spaces and currency signs). Every
//! character of the repertoire has its place in Excel's order. A text holding another character
//! (U+0000 included) is not decided here ([`Beyond`]).
//!
//! [`WEIGHTS`] gives each character's weights as ranks: the primary weights in Excel's order, and
//! the diacritic, case and special weights each in their own order.

use super::Beyond;
use std::cmp::{Ordering, Reverse};

/// A primary weight with its diacritic and case weights, each 0 when the character has none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Unit {
    primary: u16,
    diacritic: u8,
    case: u8,
}

const fn u(primary: u16, diacritic: u8, case: u8) -> Unit {
    Unit {
        primary,
        diacritic,
        case,
    }
}

/// What a character weighs in the second pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Weight {
    /// Nothing at any level.
    Ignorable,
    /// A special weight, compared after every other level, at the character's position.
    Special(u8),
    /// One primary weight.
    One(Unit),
    /// Two primary weights: a ligature weighing as its two letters.
    Two(Unit, Unit),
}

use Weight::{Ignorable, One, Special, Two};

/// The last code unit the first pass reads itself.
const FIRST_PASS_LAST: u16 = 0x007A;

/// Every character of the repertoire, by its UTF-16 code unit, in code-unit order: U+0001 to
/// U+007F at indices 0 to 126, U+00A0 to U+017F at indices 127 to 350, and the rest after them.
#[rustfmt::skip]
const WEIGHTS: [(u16, Weight); 404] = [
    (0x0001, Special(1)),
    (0x0002, Special(2)),
    (0x0003, Special(3)),
    (0x0004, Special(4)),
    (0x0005, Special(5)),
    (0x0006, Special(6)),
    (0x0007, Special(7)),
    (0x0008, Special(8)),
    (0x0009, One(u(3, 0, 0))),
    (0x000A, One(u(4, 0, 0))),
    (0x000B, One(u(5, 0, 0))),
    (0x000C, One(u(6, 0, 0))),
    (0x000D, One(u(7, 0, 0))),
    (0x000E, Special(9)),
    (0x000F, Special(10)),
    (0x0010, Special(11)),
    (0x0011, Special(12)),
    (0x0012, Special(13)),
    (0x0013, Special(14)),
    (0x0014, Special(15)),
    (0x0015, Special(16)),
    (0x0016, Special(17)),
    (0x0017, Special(18)),
    (0x0018, Special(19)),
    (0x0019, Special(20)),
    (0x001A, Special(21)),
    (0x001B, Special(22)),
    (0x001C, Special(23)),
    (0x001D, Special(24)),
    (0x001E, Special(25)),
    (0x001F, Special(26)),
    (0x0020, One(u(1, 0, 0))),
    (0x0021, One(u(13, 0, 0))),
    (0x0022, One(u(14, 0, 0))),
    (0x0023, One(u(15, 0, 0))),
    (0x0024, One(u(16, 0, 0))),
    (0x0025, One(u(17, 0, 0))),
    (0x0026, One(u(18, 0, 0))),
    (0x0027, Special(28)),
    (0x0028, One(u(19, 0, 0))),
    (0x0029, One(u(20, 0, 0))),
    (0x002A, One(u(21, 0, 0))),
    (0x002B, One(u(74, 0, 0))),
    (0x002C, One(u(22, 0, 0))),
    (0x002D, Special(29)),
    (0x002E, One(u(23, 0, 0))),
    (0x002F, One(u(24, 0, 0))),
    (0x0030, One(u(96, 0, 0))),
    (0x0031, One(u(100, 0, 0))),
    (0x0032, One(u(101, 0, 0))),
    (0x0033, One(u(102, 0, 0))),
    (0x0034, One(u(103, 0, 0))),
    (0x0035, One(u(104, 0, 0))),
    (0x0036, One(u(105, 0, 0))),
    (0x0037, One(u(106, 0, 0))),
    (0x0038, One(u(107, 0, 0))),
    (0x0039, One(u(108, 0, 0))),
    (0x003A, One(u(25, 0, 0))),
    (0x003B, One(u(26, 0, 0))),
    (0x003C, One(u(75, 0, 0))),
    (0x003D, One(u(76, 0, 0))),
    (0x003E, One(u(77, 0, 0))),
    (0x003F, One(u(27, 0, 0))),
    (0x0040, One(u(28, 0, 0))),
    (0x0041, One(u(109, 0, 0))),
    (0x0042, One(u(110, 0, 0))),
    (0x0043, One(u(111, 0, 0))),
    (0x0044, One(u(112, 0, 0))),
    (0x0045, One(u(113, 0, 0))),
    (0x0046, One(u(114, 0, 0))),
    (0x0047, One(u(115, 0, 0))),
    (0x0048, One(u(116, 0, 0))),
    (0x0049, One(u(117, 0, 0))),
    (0x004A, One(u(118, 0, 0))),
    (0x004B, One(u(119, 0, 0))),
    (0x004C, One(u(120, 0, 0))),
    (0x004D, One(u(121, 0, 0))),
    (0x004E, One(u(122, 0, 0))),
    (0x004F, One(u(124, 0, 0))),
    (0x0050, One(u(125, 0, 0))),
    (0x0051, One(u(126, 0, 0))),
    (0x0052, One(u(127, 0, 0))),
    (0x0053, One(u(128, 0, 0))),
    (0x0054, One(u(130, 0, 0))),
    (0x0055, One(u(133, 0, 0))),
    (0x0056, One(u(134, 0, 0))),
    (0x0057, One(u(135, 0, 0))),
    (0x0058, One(u(136, 0, 0))),
    (0x0059, One(u(137, 0, 0))),
    (0x005A, One(u(138, 0, 0))),
    (0x005B, One(u(29, 0, 0))),
    (0x005C, One(u(30, 0, 0))),
    (0x005D, One(u(31, 0, 0))),
    (0x005E, One(u(32, 0, 0))),
    (0x005F, One(u(33, 0, 0))),
    (0x0060, One(u(34, 0, 0))),
    (0x0061, One(u(109, 0, 0))),
    (0x0062, One(u(110, 0, 0))),
    (0x0063, One(u(111, 0, 0))),
    (0x0064, One(u(112, 0, 0))),
    (0x0065, One(u(113, 0, 0))),
    (0x0066, One(u(114, 0, 0))),
    (0x0067, One(u(115, 0, 0))),
    (0x0068, One(u(116, 0, 0))),
    (0x0069, One(u(117, 0, 0))),
    (0x006A, One(u(118, 0, 0))),
    (0x006B, One(u(119, 0, 0))),
    (0x006C, One(u(120, 0, 0))),
    (0x006D, One(u(121, 0, 0))),
    (0x006E, One(u(122, 0, 0))),
    (0x006F, One(u(124, 0, 0))),
    (0x0070, One(u(125, 0, 0))),
    (0x0071, One(u(126, 0, 0))),
    (0x0072, One(u(127, 0, 0))),
    (0x0073, One(u(128, 0, 0))),
    (0x0074, One(u(130, 0, 0))),
    (0x0075, One(u(133, 0, 0))),
    (0x0076, One(u(134, 0, 0))),
    (0x0077, One(u(135, 0, 0))),
    (0x0078, One(u(136, 0, 0))),
    (0x0079, One(u(137, 0, 0))),
    (0x007A, One(u(138, 0, 0))),
    (0x007B, One(u(35, 0, 0))),
    (0x007C, One(u(36, 0, 0))),
    (0x007D, One(u(37, 0, 0))),
    (0x007E, One(u(38, 0, 0))),
    (0x007F, Special(27)),
    (0x00A0, One(u(2, 0, 0))),
    (0x00A1, One(u(39, 0, 0))),
    (0x00A2, One(u(58, 0, 0))),
    (0x00A3, One(u(59, 0, 0))),
    (0x00A4, One(u(60, 0, 0))),
    (0x00A5, One(u(61, 0, 0))),
    (0x00A6, One(u(40, 0, 0))),
    (0x00A7, One(u(83, 0, 0))),
    (0x00A8, One(u(41, 0, 0))),
    (0x00A9, One(u(84, 0, 0))),
    (0x00AA, One(u(109, 1, 1))),
    (0x00AB, One(u(79, 0, 0))),
    (0x00AC, One(u(85, 0, 0))),
    (0x00AD, Ignorable),
    (0x00AE, One(u(86, 0, 0))),
    (0x00AF, One(u(42, 0, 0))),
    (0x00B0, One(u(87, 0, 0))),
    (0x00B1, One(u(78, 0, 0))),
    (0x00B2, One(u(101, 0, 1))),
    (0x00B3, One(u(102, 0, 1))),
    (0x00B4, One(u(43, 0, 0))),
    (0x00B5, One(u(88, 0, 0))),
    (0x00B6, One(u(89, 0, 0))),
    (0x00B7, One(u(90, 0, 0))),
    (0x00B8, One(u(44, 0, 0))),
    (0x00B9, One(u(100, 0, 1))),
    (0x00BA, One(u(124, 1, 1))),
    (0x00BB, One(u(80, 0, 0))),
    (0x00BC, One(u(97, 0, 0))),
    (0x00BD, One(u(98, 0, 0))),
    (0x00BE, One(u(99, 0, 0))),
    (0x00BF, One(u(45, 0, 0))),
    (0x00C0, One(u(109, 6, 0))),
    (0x00C1, One(u(109, 5, 0))),
    (0x00C2, One(u(109, 9, 0))),
    (0x00C3, One(u(109, 14, 0))),
    (0x00C4, One(u(109, 10, 0))),
    (0x00C5, One(u(109, 15, 0))),
    (0x00C6, Two(u(109, 0, 0), u(113, 0, 0))),
    (0x00C7, One(u(111, 17, 0))),
    (0x00C8, One(u(113, 6, 0))),
    (0x00C9, One(u(113, 5, 0))),
    (0x00CA, One(u(113, 9, 0))),
    (0x00CB, One(u(113, 10, 0))),
    (0x00CC, One(u(117, 6, 0))),
    (0x00CD, One(u(117, 5, 0))),
    (0x00CE, One(u(117, 9, 0))),
    (0x00CF, One(u(117, 10, 0))),
    (0x00D0, One(u(112, 24, 0))),
    (0x00D1, One(u(122, 14, 0))),
    (0x00D2, One(u(124, 6, 0))),
    (0x00D3, One(u(124, 5, 0))),
    (0x00D4, One(u(124, 9, 0))),
    (0x00D5, One(u(124, 14, 0))),
    (0x00D6, One(u(124, 10, 0))),
    (0x00D7, One(u(81, 0, 0))),
    (0x00D8, One(u(124, 21, 0))),
    (0x00D9, One(u(133, 6, 0))),
    (0x00DA, One(u(133, 5, 0))),
    (0x00DB, One(u(133, 9, 0))),
    (0x00DC, One(u(133, 10, 0))),
    (0x00DD, One(u(137, 5, 0))),
    (0x00DE, Two(u(130, 0, 0), u(116, 0, 0))),
    (0x00DF, Two(u(128, 0, 0), u(128, 0, 0))),
    (0x00E0, One(u(109, 6, 0))),
    (0x00E1, One(u(109, 5, 0))),
    (0x00E2, One(u(109, 9, 0))),
    (0x00E3, One(u(109, 14, 0))),
    (0x00E4, One(u(109, 10, 0))),
    (0x00E5, One(u(109, 15, 0))),
    (0x00E6, Two(u(109, 0, 0), u(113, 0, 0))),
    (0x00E7, One(u(111, 17, 0))),
    (0x00E8, One(u(113, 6, 0))),
    (0x00E9, One(u(113, 5, 0))),
    (0x00EA, One(u(113, 9, 0))),
    (0x00EB, One(u(113, 10, 0))),
    (0x00EC, One(u(117, 6, 0))),
    (0x00ED, One(u(117, 5, 0))),
    (0x00EE, One(u(117, 9, 0))),
    (0x00EF, One(u(117, 10, 0))),
    (0x00F0, One(u(112, 24, 0))),
    (0x00F1, One(u(122, 14, 0))),
    (0x00F2, One(u(124, 6, 0))),
    (0x00F3, One(u(124, 5, 0))),
    (0x00F4, One(u(124, 9, 0))),
    (0x00F5, One(u(124, 14, 0))),
    (0x00F6, One(u(124, 10, 0))),
    (0x00F7, One(u(82, 0, 0))),
    (0x00F8, One(u(124, 21, 0))),
    (0x00F9, One(u(133, 6, 0))),
    (0x00FA, One(u(133, 5, 0))),
    (0x00FB, One(u(133, 9, 0))),
    (0x00FC, One(u(133, 10, 0))),
    (0x00FD, One(u(137, 5, 0))),
    (0x00FE, Two(u(130, 0, 0), u(116, 0, 0))),
    (0x00FF, One(u(137, 10, 0))),
    (0x0100, One(u(109, 13, 0))),
    (0x0101, One(u(109, 13, 0))),
    (0x0102, One(u(109, 12, 0))),
    (0x0103, One(u(109, 12, 0))),
    (0x0104, One(u(109, 16, 0))),
    (0x0105, One(u(109, 16, 0))),
    (0x0106, One(u(111, 5, 0))),
    (0x0107, One(u(111, 5, 0))),
    (0x0108, One(u(111, 9, 0))),
    (0x0109, One(u(111, 9, 0))),
    (0x010A, One(u(111, 7, 0))),
    (0x010B, One(u(111, 7, 0))),
    (0x010C, One(u(111, 11, 0))),
    (0x010D, One(u(111, 11, 0))),
    (0x010E, One(u(112, 11, 0))),
    (0x010F, One(u(112, 11, 0))),
    (0x0110, One(u(112, 19, 0))),
    (0x0111, One(u(112, 19, 0))),
    (0x0112, One(u(113, 13, 0))),
    (0x0113, One(u(113, 13, 0))),
    (0x0114, One(u(113, 12, 0))),
    (0x0115, One(u(113, 12, 0))),
    (0x0116, One(u(113, 7, 0))),
    (0x0117, One(u(113, 7, 0))),
    (0x0118, One(u(113, 16, 0))),
    (0x0119, One(u(113, 16, 0))),
    (0x011A, One(u(113, 11, 0))),
    (0x011B, One(u(113, 11, 0))),
    (0x011C, One(u(115, 9, 0))),
    (0x011D, One(u(115, 9, 0))),
    (0x011E, One(u(115, 12, 0))),
    (0x011F, One(u(115, 12, 0))),
    (0x0120, One(u(115, 7, 0))),
    (0x0121, One(u(115, 7, 0))),
    (0x0122, One(u(115, 17, 0))),
    (0x0123, One(u(115, 17, 0))),
    (0x0124, One(u(116, 9, 0))),
    (0x0125, One(u(116, 9, 0))),
    (0x0126, One(u(116, 19, 0))),
    (0x0127, One(u(116, 19, 0))),
    (0x0128, One(u(117, 14, 0))),
    (0x0129, One(u(117, 14, 0))),
    (0x012A, One(u(117, 13, 0))),
    (0x012B, One(u(117, 13, 0))),
    (0x012C, One(u(117, 12, 0))),
    (0x012D, One(u(117, 12, 0))),
    (0x012E, One(u(117, 16, 0))),
    (0x012F, One(u(117, 16, 0))),
    (0x0130, One(u(117, 7, 0))),
    (0x0131, One(u(117, 2, 0))),
    (0x0132, Two(u(117, 0, 0), u(118, 0, 0))),
    (0x0133, Two(u(117, 0, 0), u(118, 0, 0))),
    (0x0134, One(u(118, 9, 0))),
    (0x0135, One(u(118, 9, 0))),
    (0x0136, One(u(119, 17, 0))),
    (0x0137, One(u(119, 17, 0))),
    (0x0138, One(u(119, 2, 0))),
    (0x0139, One(u(120, 5, 0))),
    (0x013A, One(u(120, 5, 0))),
    (0x013B, One(u(120, 17, 0))),
    (0x013C, One(u(120, 17, 0))),
    (0x013D, One(u(120, 11, 0))),
    (0x013E, One(u(120, 11, 0))),
    (0x013F, One(u(120, 8, 0))),
    (0x0140, One(u(120, 8, 0))),
    (0x0141, One(u(120, 20, 0))),
    (0x0142, One(u(120, 20, 0))),
    (0x0143, One(u(122, 5, 0))),
    (0x0144, One(u(122, 5, 0))),
    (0x0145, One(u(122, 17, 0))),
    (0x0146, One(u(122, 17, 0))),
    (0x0147, One(u(122, 11, 0))),
    (0x0148, One(u(122, 11, 0))),
    (0x0149, One(u(122, 23, 0))),
    (0x014A, One(u(123, 0, 0))),
    (0x014B, One(u(123, 0, 0))),
    (0x014C, One(u(124, 13, 0))),
    (0x014D, One(u(124, 13, 0))),
    (0x014E, One(u(124, 12, 0))),
    (0x014F, One(u(124, 12, 0))),
    (0x0150, One(u(124, 18, 0))),
    (0x0151, One(u(124, 18, 0))),
    (0x0152, Two(u(124, 0, 0), u(113, 0, 0))),
    (0x0153, Two(u(124, 0, 0), u(113, 0, 0))),
    (0x0154, One(u(127, 5, 0))),
    (0x0155, One(u(127, 5, 0))),
    (0x0156, One(u(127, 17, 0))),
    (0x0157, One(u(127, 17, 0))),
    (0x0158, One(u(127, 11, 0))),
    (0x0159, One(u(127, 11, 0))),
    (0x015A, One(u(128, 5, 0))),
    (0x015B, One(u(128, 5, 0))),
    (0x015C, One(u(128, 9, 0))),
    (0x015D, One(u(128, 9, 0))),
    (0x015E, One(u(128, 17, 0))),
    (0x015F, One(u(128, 17, 0))),
    (0x0160, One(u(128, 11, 0))),
    (0x0161, One(u(128, 11, 0))),
    (0x0162, One(u(130, 17, 0))),
    (0x0163, One(u(130, 17, 0))),
    (0x0164, One(u(130, 11, 0))),
    (0x0165, One(u(130, 11, 0))),
    (0x0166, One(u(132, 19, 0))),
    (0x0167, One(u(132, 19, 0))),
    (0x0168, One(u(133, 14, 0))),
    (0x0169, One(u(133, 14, 0))),
    (0x016A, One(u(133, 13, 0))),
    (0x016B, One(u(133, 13, 0))),
    (0x016C, One(u(133, 12, 0))),
    (0x016D, One(u(133, 12, 0))),
    (0x016E, One(u(133, 15, 0))),
    (0x016F, One(u(133, 15, 0))),
    (0x0170, One(u(133, 18, 0))),
    (0x0171, One(u(133, 18, 0))),
    (0x0172, One(u(133, 16, 0))),
    (0x0173, One(u(133, 16, 0))),
    (0x0174, One(u(135, 9, 0))),
    (0x0175, One(u(135, 9, 0))),
    (0x0176, One(u(137, 9, 0))),
    (0x0177, One(u(137, 9, 0))),
    (0x0178, One(u(137, 10, 0))),
    (0x0179, One(u(138, 5, 0))),
    (0x017A, One(u(138, 5, 0))),
    (0x017B, One(u(138, 7, 0))),
    (0x017C, One(u(138, 7, 0))),
    (0x017D, One(u(138, 11, 0))),
    (0x017E, One(u(138, 11, 0))),
    (0x017F, One(u(129, 0, 0))),
    (0x0192, One(u(114, 22, 0))),
    (0x02C6, One(u(32, 0, 0))),
    (0x02DC, One(u(46, 0, 0))),
    (0x2002, One(u(8, 0, 0))),
    (0x2003, One(u(9, 0, 0))),
    (0x2007, One(u(10, 0, 0))),
    (0x2009, One(u(11, 0, 0))),
    (0x200B, One(u(12, 0, 0))),
    (0x200C, Ignorable),
    (0x200D, Ignorable),
    (0x200E, Ignorable),
    (0x200F, Ignorable),
    (0x2010, Special(31)),
    (0x2011, Special(32)),
    (0x2012, Special(33)),
    (0x2013, Special(34)),
    (0x2014, Special(35)),
    (0x2015, Special(36)),
    (0x2018, One(u(47, 0, 0))),
    (0x2019, One(u(48, 0, 0))),
    (0x201A, One(u(49, 0, 0))),
    (0x201B, One(u(50, 0, 0))),
    (0x201C, One(u(51, 0, 0))),
    (0x201D, One(u(52, 0, 0))),
    (0x201E, One(u(53, 0, 0))),
    (0x201F, One(u(54, 0, 0))),
    (0x2020, One(u(92, 0, 0))),
    (0x2021, One(u(93, 0, 0))),
    (0x2022, One(u(94, 0, 0))),
    (0x2026, One(u(91, 0, 0))),
    (0x202F, One(u(1, 3, 0))),
    (0x2030, One(u(95, 4, 0))),
    (0x2032, One(u(55, 0, 0))),
    (0x2033, One(u(55, 11, 0))),
    (0x2039, One(u(56, 0, 0))),
    (0x203A, One(u(57, 0, 0))),
    (0x2060, Ignorable),
    (0x20A1, One(u(69, 0, 0))),
    (0x20A3, One(u(63, 0, 0))),
    (0x20A4, One(u(64, 0, 0))),
    (0x20A6, One(u(65, 0, 0))),
    (0x20A8, One(u(66, 0, 0))),
    (0x20A9, One(u(67, 0, 0))),
    (0x20AA, One(u(70, 0, 0))),
    (0x20AB, One(u(71, 0, 0))),
    (0x20AC, One(u(68, 0, 0))),
    (0x20B1, One(u(62, 0, 0))),
    (0x20B9, One(u(72, 0, 0))),
    (0x20BA, One(u(73, 0, 0))),
    (0x20BD, Ignorable),
    (0x2122, One(u(131, 0, 0))),
    (0x2212, Special(30)),
    (0xFEFF, Ignorable),
];

/// The weights of the character whose code unit is `code`, or `None` beyond the repertoire.
fn weight(code: u16) -> Option<Weight> {
    let index = match code {
        0x0001..=0x007F => usize::from(code) - 1,
        0x00A0..=0x017F => usize::from(code) - 0x00A0 + 127,
        _ => {
            351 + WEIGHTS[351..]
                .binary_search_by_key(&code, |&(c, _)| c)
                .ok()?
        }
    };
    Some(WEIGHTS[index].1)
}

/// The order of two texts as the comparison operators give it, or [`Beyond`] naming the first of
/// them that holds a code unit beyond the repertoire.
pub(super) fn compare(a: &[u16], b: &[u16]) -> Result<Ordering, Beyond> {
    if let Some(operand) = [a, b]
        .iter()
        .position(|text| text.iter().any(|&code| weight(code).is_none()))
    {
        return Err(Beyond { operand });
    }
    let fold = |code: u16| {
        if (0x0061..=FIRST_PASS_LAST).contains(&code) {
            code - 0x20
        } else {
            code
        }
    };
    for (&x, &y) in a.iter().zip(b) {
        if x > FIRST_PASS_LAST || y > FIRST_PASS_LAST || fold(x) != fold(y) {
            return Ok(weighed(a, b));
        }
    }
    Ok(a.len().cmp(&b.len()))
}

/// The second pass, for texts of the repertoire.
fn weighed(a: &[u16], b: &[u16]) -> Ordering {
    let level = |pick: fn(Unit) -> u16| units(a).map(pick).cmp(units(b).map(pick));
    level(|unit| unit.primary)
        .then_with(|| level(|unit| u16::from(unit.diacritic)))
        .then_with(|| level(|unit| u16::from(unit.case)))
        .then_with(|| specials(a).cmp(specials(b)))
}

/// The primary weights of a text of the repertoire, in order, each with its diacritic and case
/// weights.
fn units(text: &[u16]) -> impl Iterator<Item = Unit> + '_ {
    text.iter()
        .flat_map(|&code| match weight(code) {
            Some(One(first)) => [Some(first), None],
            Some(Two(first, second)) => [Some(first), Some(second)],
            Some(Ignorable | Special(_)) | None => [None, None],
        })
        .flatten()
}

/// The special characters of a text of the repertoire, in order, each as it is compared: its
/// position (an earlier position the greater), then its weight.
fn specials(text: &[u16]) -> impl Iterator<Item = (Reverse<usize>, u8)> + '_ {
    text.iter()
        .scan(0usize, |position, &code| {
            Some(match weight(code) {
                Some(Special(special)) => Some((Reverse(*position), special)),
                Some(One(_)) => {
                    *position += 1;
                    None
                }
                Some(Two(..)) => {
                    *position += 2;
                    None
                }
                Some(Ignorable) | None => None,
            })
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units_of(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_table_is_in_code_order_and_indexed_directly() {
        assert!(WEIGHTS.windows(2).all(|pair| pair[0].0 < pair[1].0));
        for (index, &(code, weights)) in WEIGHTS.iter().enumerate() {
            assert_eq!(weight(code), Some(weights), "U+{code:04X} at {index}");
        }
        for code in [
            0x0000, 0x0080, 0x009F, 0x0180, 0x0391, 0x4E2D, 0xD800, 0xFFFF,
        ] {
            assert_eq!(weight(code), None, "U+{code:04X}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_passes_decide_as_the_module_states() {
        let order = |a: &str, b: &str| compare(&units_of(a), &units_of(b));
        assert_eq!(order("w", "w\u{ad}"), Ok(Ordering::Less));
        assert_eq!(order("\u{e9}", "\u{e9}\u{ad}"), Ok(Ordering::Equal));
        assert_eq!(order("a-b", "ab"), Ok(Ordering::Greater));
        assert_eq!(order("a-b", "ac"), Ok(Ordering::Less));
        assert_eq!(order("co-op", "coop"), Ok(Ordering::Greater));
        assert_eq!(order("\u{e6}", "ae"), Ok(Ordering::Equal));
        assert_eq!(order("a", "\u{3b1}"), Err(Beyond { operand: 1 }));
        assert_eq!(order("\u{0}", "\u{3b1}"), Err(Beyond { operand: 0 }));
    }
}
