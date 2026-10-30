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
//! (bead dc-zsn.1), which compares the texts' sort keys. calc-func builds its own keys
//! ([`super::sort_key`]: the Unicode Collation Algorithm's default order with Windows' measured
//! departures from it), which order texts as Windows' do, and compares them byte by byte. Primary
//! weights first (a ligature such as U+00E6, U+00DF or U+00DE weighs as ae, ss or th; e followed
//! by a combining acute weighs as U+00E9), then diacritics, then case (which ignores upper and
//! lower case but keeps width), then kana, then the special characters (the apostrophe, the hyphen
//! and the dashes, the minus sign, the control characters) by their positions. So `a-b` follows
//! `ab` and precedes `ac`, `co-op` follows `coop`, Greek follows Latin and CJK follows both.
//! Measured: every text row of the catalogue's comparison oracle (cdoc_jpha) is this rule.
//!
//! A text holding a code unit the sort keys are not built for here is not decided ([`Beyond`]),
//! whichever pass would decide it: one outside the covered scripts and blocks (most symbol
//! blocks, the scripts not covered, the private use area; see `cargo xtask collation`), U+0000,
//! Hangul conjoining jamo, the iteration marks. Neither is one whose key is not built (a combining
//! mark that follows no weighted unit directly, marks adding up past one byte).

use super::Beyond;
use super::sort_key::{refused, sort_key};
use std::cmp::Ordering;

/// The last code unit the first pass reads itself.
const FIRST_PASS_LAST: u16 = 0x007A;

/// The order of two texts as the comparison operators give it, or [`Beyond`] naming the first of
/// them whose sort key is not built here.
pub(super) fn compare(a: &[u16], b: &[u16]) -> Result<Ordering, Beyond> {
    // A text holding a unit the collation refuses is not decided, whichever pass would decide it.
    if let Some(operand) = [a, b]
        .iter()
        .position(|text| text.iter().any(|&code| refused(code)))
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
            return weighed(a, b);
        }
    }
    Ok(a.len().cmp(&b.len()))
}

/// The second pass: the two texts' sort keys.
fn weighed(a: &[u16], b: &[u16]) -> Result<Ordering, Beyond> {
    let left = sort_key(a).ok_or(Beyond { operand: 0 })?;
    let right = sort_key(b).ok_or(Beyond { operand: 1 })?;
    Ok(left.cmp(&right))
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
