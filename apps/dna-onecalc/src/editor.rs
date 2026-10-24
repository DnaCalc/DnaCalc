//! The formula editor's state that is not the text itself: the completion popup, the keys it
//! takes, and the bracket pair at the caret.
//!
//! Ported from DnaOneCalc (at 61e3979): the popup lifecycle from
//! `src/dnaonecalc-host/src/services/completion_popup.rs` (`sync_completion_popup_with_proposals`,
//! `move_completion_selection`, dismiss and accept), the popup keys from
//! `src/dnaonecalc-host/src/ui/editor/commands.rs` (`keydown_to_command`), and
//! `src/dnaonecalc-host/src/ui/editor/bracket_matcher.rs` whole, moved from char offsets to the
//! UTF-16 offsets a browser text field and calc-fml's spans count in. The proposals themselves are
//! calc-fml's ([`calc_fml::assist::complete`]).

use calc_fml::assist::{Completion, Proposal};

/// The completion popup: hidden, or open on a list with one item selected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Popup {
    #[default]
    Hidden,
    Open {
        completion: Completion,
        /// Always inside `completion.proposals`.
        selected: usize,
    },
}

impl Popup {
    /// Reconciles the popup with the completion at the caret after an edit or a caret move.
    /// An empty or absent completion hides it. A list of the same length keeps its selection;
    /// a different list selects its first item. A popup the person dismissed stays hidden until
    /// the prefix changes, so Escape is not undone by the next caret move.
    pub fn sync(&mut self, completion: Option<Completion>, dismissed_prefix: Option<&str>) {
        let Some(completion) = completion.filter(|c| !c.proposals.is_empty()) else {
            *self = Popup::Hidden;
            return;
        };
        if dismissed_prefix == Some(completion.prefix.as_str()) {
            *self = Popup::Hidden;
            return;
        }
        let selected = match self {
            Popup::Open {
                completion: prior,
                selected,
            } if prior.proposals == completion.proposals => *selected,
            _ => 0,
        };
        *self = Popup::Open {
            completion,
            selected,
        };
    }

    pub fn is_open(&self) -> bool {
        matches!(self, Popup::Open { .. })
    }

    /// Moves the selection by `delta`, wrapping at both ends.
    pub fn move_selection(&mut self, delta: i32) {
        if let Popup::Open {
            completion,
            selected,
        } = self
        {
            let len = completion.proposals.len() as i32;
            *selected = ((*selected as i32) + delta).rem_euclid(len) as usize;
        }
    }

    /// Selects the item at `index` when the list has one.
    pub fn select(&mut self, index: usize) {
        if let Popup::Open {
            completion,
            selected,
        } = self
            && index < completion.proposals.len()
        {
            *selected = index;
        }
    }

    /// The selected proposal.
    pub fn selected(&self) -> Option<Proposal> {
        match self {
            Popup::Open {
                completion,
                selected,
            } => completion.proposals.get(*selected).copied(),
            Popup::Hidden => None,
        }
    }

    /// Accepts the selected proposal: the text with the prefix replaced and the caret after the
    /// insertion. The popup closes.
    pub fn accept(&mut self, text: &str) -> Option<(String, usize)> {
        let Popup::Open {
            completion,
            selected,
        } = self
        else {
            return None;
        };
        let proposal = *completion.proposals.get(*selected)?;
        let applied = completion.apply(text, proposal);
        *self = Popup::Hidden;
        Some(applied)
    }
}

/// What a key does to an open popup. Keys reach the popup only while it is open, so the text
/// field keeps its own arrow keys, Enter and Tab otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupKey {
    Next,
    Previous,
    Accept,
    Dismiss,
}

/// The popup command for a key, by the DOM `KeyboardEvent.key` name, while the popup is open.
pub fn popup_key(key: &str) -> Option<PopupKey> {
    match key {
        "ArrowDown" => Some(PopupKey::Next),
        "ArrowUp" => Some(PopupKey::Previous),
        "Enter" | "Tab" => Some(PopupKey::Accept),
        "Escape" => Some(PopupKey::Dismiss),
        _ => None,
    }
}

/// `text` with the UTF-16 range `start..end` replaced by a call of `name`: its name and opening
/// parenthesis, after a `=` when the text is empty. Returns the new text and the caret after the
/// insertion. Offsets past the text are taken as its end.
pub fn insert_call(text: &str, start: usize, end: usize, name: &str) -> (String, usize) {
    let units: Vec<u16> = text.encode_utf16().collect();
    let end = end.min(units.len());
    let start = start.min(end);
    let insert = if text.trim().is_empty() {
        format!("={name}(")
    } else {
        format!("{name}(")
    };
    let mut out: Vec<u16> = units[..start].to_vec();
    out.extend(insert.encode_utf16());
    let caret = out.len();
    out.extend_from_slice(&units[end..]);
    (String::from_utf16_lossy(&out), caret)
}

/// The catalogue's named functions whose name contains `filter` (in any ASCII case) or whose
/// description contains it (in any case), in name order.
pub fn browse(filter: &str) -> Vec<calc_func::FunctionCallTarget<'static>> {
    let filter = filter.trim();
    let upper = filter.to_ascii_uppercase();
    let lower = filter.to_lowercase();
    calc_func::catalogue()
        .surfaces()
        .filter(|target| target.notation() == calc_func::Notation::Call)
        .filter(|target| {
            filter.is_empty()
                || target.spelling().contains(upper.as_str())
                || calc_func::help::description(*target)
                    .is_some_and(|text| text.to_lowercase().contains(lower.as_str()))
        })
        .collect()
}

/// The offsets of a matched bracket pair, in UTF-16 code units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BracketPair {
    pub open: usize,
    pub close: usize,
}

/// The bracket pair of the bracket at the caret, or of the one just before it: nesting is
/// followed, and brackets inside a string literal (between `"`s) are not brackets.
pub fn bracket_pair_at(text: &str, caret: usize) -> Option<BracketPair> {
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.is_empty() {
        return None;
    }
    let probe = bracket_at(&units, caret)
        .or_else(|| caret.checked_sub(1).and_then(|p| bracket_at(&units, p)));
    let (index, unit) = probe?;
    if inside_string(&units, index) {
        return None;
    }
    if is_open(unit) {
        matching_close(&units, index).map(|close| BracketPair { open: index, close })
    } else {
        matching_open(&units, index).map(|open| BracketPair { open, close: index })
    }
}

const QUOTE: u16 = b'"' as u16;

fn inside_string(units: &[u16], index: usize) -> bool {
    units[..index].iter().filter(|&&u| u == QUOTE).count() % 2 == 1
}

fn bracket_at(units: &[u16], index: usize) -> Option<(usize, u16)> {
    units
        .get(index)
        .copied()
        .filter(|&u| is_open(u) || is_close(u))
        .map(|u| (index, u))
}

fn is_open(unit: u16) -> bool {
    matches!(unit, 0x28 | 0x5B | 0x7B)
}

fn is_close(unit: u16) -> bool {
    matches!(unit, 0x29 | 0x5D | 0x7D)
}

fn pairs(open: u16, close: u16) -> bool {
    matches!((open, close), (0x28, 0x29) | (0x5B, 0x5D) | (0x7B, 0x7D))
}

fn matching_close(units: &[u16], open_index: usize) -> Option<usize> {
    let open = units[open_index];
    let mut depth = 0usize;
    let mut in_string = false;
    for (index, &unit) in units.iter().enumerate().skip(open_index) {
        if in_string {
            in_string = unit != QUOTE;
            continue;
        }
        if unit == QUOTE {
            in_string = true;
        } else if is_open(unit) {
            depth += 1;
        } else if is_close(unit) {
            depth -= 1;
            if depth == 0 {
                return pairs(open, unit).then_some(index);
            }
        }
    }
    None
}

fn matching_open(units: &[u16], close_index: usize) -> Option<usize> {
    let close = units[close_index];
    let mut depth = 0usize;
    let mut in_string = false;
    for index in (0..=close_index).rev() {
        let unit = units[index];
        if unit == QUOTE {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if is_close(unit) {
            depth += 1;
        } else if is_open(unit) {
            depth -= 1;
            if depth == 0 {
                return pairs(unit, close).then_some(index);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_fml::assist::complete;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_popup_opens_moves_wraps_and_accepts() {
        let mut popup = Popup::default();
        popup.sync(complete("=SU", 3), None);
        assert!(popup.is_open());
        let first = popup.selected().expect("a first item");
        assert!(first.label().starts_with("SU"), "{}", first.label());
        popup.move_selection(-1);
        let last = popup.selected().expect("the last item");
        assert_ne!(first, last, "moving up from the first wraps to the last");
        popup.move_selection(1);
        assert_eq!(popup.selected(), Some(first));
        // The same list keeps the selection; a narrower one starts again at its first item.
        popup.move_selection(1);
        let second = popup.selected();
        popup.sync(complete("=SU", 3), None);
        assert_eq!(popup.selected(), second);
        popup.sync(complete("=SUM", 4), None);
        let sum_index = match &popup {
            Popup::Open { completion, .. } => completion
                .proposals
                .iter()
                .position(|p| p.label() == "SUM")
                .expect("SUM"),
            Popup::Hidden => panic!("open"),
        };
        popup.select(sum_index);
        assert_eq!(popup.accept("=SUM"), Some(("=SUM(".to_owned(), 5)));
        assert!(!popup.is_open());
        assert_eq!(popup.accept("=SUM"), None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_dismissed_popup_stays_hidden_until_the_prefix_changes() {
        let mut popup = Popup::default();
        popup.sync(complete("=SU", 3), Some("SU"));
        assert!(!popup.is_open());
        popup.sync(complete("=SUM", 4), Some("SU"));
        assert!(popup.is_open());
        popup.sync(complete("=ZZZQ", 5), None);
        assert!(!popup.is_open(), "no proposals hide it");
        popup.sync(None, None);
        assert!(!popup.is_open());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn calls_insert_at_the_selection_and_functions_filter() {
        assert_eq!(insert_call("", 0, 0, "SUM"), ("=SUM(".to_owned(), 5));
        assert_eq!(insert_call("=1+", 3, 3, "PI"), ("=1+PI(".to_owned(), 6));
        assert_eq!(
            insert_call("=1+XX*2", 3, 5, "PI"),
            ("=1+PI(*2".to_owned(), 6)
        );
        assert_eq!(insert_call("=1", 9, 9, "PI"), ("=1PI(".to_owned(), 5));
        let rounds: Vec<&str> = browse("round").iter().map(|t| t.spelling()).collect();
        assert!(
            rounds.contains(&"ROUND") && rounds.contains(&"MROUND"),
            "{rounds:?}"
        );
        // A word of the description finds a function too.
        let absolute: Vec<&str> = browse("absolute value")
            .iter()
            .map(|t| t.spelling())
            .collect();
        assert!(absolute.contains(&"ABS"), "{absolute:?}");
        assert!(browse("").len() > 300);
        assert!(browse("zzzqqq").is_empty());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn popup_keys() {
        assert_eq!(popup_key("ArrowDown"), Some(PopupKey::Next));
        assert_eq!(popup_key("ArrowUp"), Some(PopupKey::Previous));
        assert_eq!(popup_key("Tab"), Some(PopupKey::Accept));
        assert_eq!(popup_key("Enter"), Some(PopupKey::Accept));
        assert_eq!(popup_key("Escape"), Some(PopupKey::Dismiss));
        assert_eq!(popup_key("a"), None);
    }

    // DnaOneCalc's bracket matcher cases, unchanged.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn brackets_match_at_and_after_the_caret() {
        let text = "=SUM(1,2)";
        for caret in [4, 8, 9] {
            assert_eq!(
                bracket_pair_at(text, caret),
                Some(BracketPair { open: 4, close: 8 }),
                "caret {caret}"
            );
        }
        let text = "=FN(A[1]+(2))";
        assert_eq!(
            bracket_pair_at(text, 3),
            Some(BracketPair { open: 3, close: 12 })
        );
        assert_eq!(
            bracket_pair_at(text, 5),
            Some(BracketPair { open: 5, close: 7 })
        );
        assert_eq!(
            bracket_pair_at(text, 9),
            Some(BracketPair { open: 9, close: 11 })
        );
        let text = "=CONCAT(\"(hello)\",1)";
        assert_eq!(bracket_pair_at(text, 9), None);
        assert_eq!(
            bracket_pair_at(text, 7),
            Some(BracketPair { open: 7, close: 19 })
        );
        assert_eq!(bracket_pair_at("=SUM(1,2)", 0), None);
        assert_eq!(bracket_pair_at("=SUM(1,2)", 2), None);
        assert_eq!(bracket_pair_at("=SUM(1,2", 4), None);
        // UTF-16 offsets: the astral character before the call takes two units.
        let text = "=\"\u{1F600}\"&LEN(1)";
        assert_eq!(
            bracket_pair_at(text, 9),
            Some(BracketPair { open: 9, close: 11 })
        );
    }
}
