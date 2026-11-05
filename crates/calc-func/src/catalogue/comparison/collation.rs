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
//! The second pass is Windows' comparison `CompareStringEx` under en-ZA with `NORM_IGNORECASE`
//! (beads dc-zsn.1 and dc-zsn.2) of what follows the units the first pass passed, not of the
//! whole texts: so a prolonged-sound mark after `a` weighs as it does alone, and `a` followed by
//! U+30FC is greater than `ab` (Excel 16.0.20527, the catalogue's sheet dczsn2_collation), while
//! `CompareStringEx` of the whole texts holds it equal to `aa`. calc-func builds its own weights ([`super::sort_key`]: the
//! Unicode Collation Algorithm's default order with Windows' measured departures from it), which
//! order texts as Windows does. Primary weights first (a ligature such as U+00E6, U+00DF or U+00DE weighs as ae, ss or th; e followed
//! by a combining acute weighs as U+00E9), then diacritics, then case (which ignores upper and
//! lower case but keeps width), then kana, then the special characters (the apostrophe, the hyphen
//! and the dashes, the minus sign, the control characters) by their positions. So `a-b` follows
//! `ab` and precedes `ac`, `co-op` follows `coop`, Greek follows Latin and CJK follows both.
//! Measured: every text row of the catalogue's comparison oracle (cdoc_jpha) is this rule.
//!
//! The diacritics are compared as `CompareStringEx` walks them, which a sort key's sums of marks
//! do not always give: a combining mark at the start, after a special or after an ignorable unit,
//! and marks past one byte ([`super::sort_key::diacritic_order`]).
//!
//! The weights cover every UTF-16 code unit but a few (dc-zsn.2). A text holding one of those is
//! not decided ([`Beyond`]) when the second pass needs its weights: U+0000, the Hangul conjoining jamo,
//! U+30A0, the isolated and medial forms of Arabic marks, and an iteration mark after a
//! special outside fn7's three adjacent iteration forms and four punctuation units
//! (see [`super::sort_key`]).
//! Identical complete modern Jamo prefixes with independent known suffixes cancel their common
//! default entries; different Jamo prefixes remain refused. Fn7 matches the ASCII pass followed
//! by CompareStringEx on all284 scalar pairs; suffix sort-key byte order differs on two Hangul
//! pairs. These measured rules and the stored Unicode weights run unchanged on every platform.
//! Bare U+1100/U+1161 at the start of an independent suffix have measured primary
//! groups C0/EC. An unfinished key can decide a strict group difference; it still
//! refuses a same-group comparison or an unmeasured Jamo continuation.
//! Iteration after Han uses the measured repeat primary,
//! confirmed by fn1's comparison and ordering controls. Fn3's previously measured strict
//! prefixes also follow from fn7's positive punctuation primary.

use super::Beyond;
use super::sort_key::{diacritic_order, leading_order, parts, refused, shared_jamo_tails};
use std::cmp::Ordering;

/// The last code unit the first pass reads itself.
const FIRST_PASS_LAST: u16 = 0x007A;

/// The order of two texts as the comparison operators give it, or [`Beyond`] naming the first of
/// them whose weights are not built here.
pub(super) fn compare(a: &[u16], b: &[u16]) -> Result<Ordering, Beyond> {
    // NUL is outside the measured comparison domain, including an ASCII
    // prefix that the first pass would otherwise skip or decide by length.
    if let Some(operand) = [a, b].iter().position(|text| text.contains(&0)) {
        return Err(Beyond { operand });
    }
    let fold = |code: u16| {
        if (0x0061..=FIRST_PASS_LAST).contains(&code) {
            code - 0x20
        } else {
            code
        }
    };
    for (at, (&x, &y)) in a.iter().zip(b).enumerate() {
        if x > FIRST_PASS_LAST || y > FIRST_PASS_LAST || fold(x) != fold(y) {
            // The second pass reads the texts from there on, not from their starts.
            return weighed(&a[at..], &b[at..]);
        }
    }
    Ok(a.len().cmp(&b.len()))
}

/// The second pass: the two texts' primary weights, then their diacritics as `CompareStringEx`
/// walks them ([`diacritic_order`]), then the rest of their sort keys (case, kana, specials).
fn weighed(a: &[u16], b: &[u16]) -> Result<Ordering, Beyond> {
    if let Some((a, b)) = shared_jamo_tails(a, b) {
        // Keep one common default entry for marks at the suffix boundary.
        // Calling this pass directly preserves NLS's treatment of ignorables;
        // Excel's ASCII length shortcut has already finished on the originals.
        let prefix = |tail: &[u16]| std::iter::once(0x0061).chain(tail.iter().copied()).collect::<Vec<_>>();
        return weighed(&prefix(a), &prefix(b));
    }
    if let Some(operand) = [a, b].iter().position(|text| text.iter().any(|&code| refused(code))) {
        return Err(Beyond { operand });
    }
    let left = parts(a);
    let right = parts(b);
    if left.is_err() || right.is_err() {
        return leading_order(&left, &right).or_else(|| {
            match (left.is_ok(), right.is_ok()) {
                (true, false) if measured_iteration_extension(a, b) => Some(Ordering::Less),
                (false, true) if measured_iteration_extension(b, a) => Some(Ordering::Greater),
                _ => None,
            }
        }).ok_or(Beyond {
            operand: usize::from(left.is_ok()),
        });
    }
    let left = left.map_err(|_| Beyond { operand: 0 })?;
    let right = right.map_err(|_| Beyond { operand: 1 })?;
    Ok(left
        .primary
        .cmp(&right.primary)
        .then_with(|| diacritic_order(a, &left, b, &right))
        .then_with(|| left.rest.cmp(&right.rest)))
}

/// Fn3 records a strict order against the exact bare prefix for these three marks immediately
/// after these three specials. This does not assign a primary weight, or compare different
/// prefixes with equal primaries. The two-unit suffix follows by transitivity: a completed
/// prefix precedes that prefix plus its known special, which precedes the measured mark.
/// A combining unit, another mark/special, or a tail beyond this suffix remains unmodelled.
fn measured_iteration_extension(shorter: &[u16], longer: &[u16]) -> bool {
    let Some(suffix) = longer.strip_prefix(shorter) else { return false; };
    let special = |unit: u16| matches!(unit, 0x002D | 0x0027 | 0x2010);
    let mark = |unit: u16| matches!(unit, 0x309D | 0x3005 | 0x30FC);
    match suffix {
        &[iteration] => mark(iteration) && shorter.last().is_some_and(|&unit| special(unit)),
        &[punctuation, iteration] => special(punctuation) && mark(iteration),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units_of(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
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
        assert_eq!(order("a", "\u{3b1}"), Ok(Ordering::Less));
        assert_eq!(order("e\u{301}", "\u{e9}"), Ok(Ordering::Equal));
        assert_eq!(order("\u{411}", "\u{410}"), Ok(Ordering::Greater));
        assert_eq!(order("a", "a\u{4e2d}"), Ok(Ordering::Less));
        assert_eq!(order("a\u{80}", "a"), Ok(Ordering::Greater));
        assert_eq!(
            order("\u{1100}\u{1161}", "a"),
            Err(Beyond { operand: 0 }),
            "conjoining jamo"
        );
    }
}
