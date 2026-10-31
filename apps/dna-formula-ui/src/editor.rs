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
use calc_fml::syntax::{TextSpan, TokenKind, lex};

pub use dna_calc_model::{TextHistory, replace_text};

/// Pure completion presentation stored by Calc's model through its open view-state registry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompletionState {
    pub popup: Popup,
}
impl dna_calc_model::ViewState for CompletionState {}

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
        "Tab" => Some(PopupKey::Accept),
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

fn cell_reference_spans(text: &str) -> Vec<TextSpan> {
    let tokens: Vec<_> = lex(text)
        .into_iter()
        .filter(|t| t.kind() != TokenKind::Whitespace)
        .collect();
    tokens
        .iter()
        .enumerate()
        .filter_map(|(i, token)| {
            let next = tokens[i + 1..]
                .iter()
                .find(|t| t.kind() != TokenKind::Whitespace);
            let axis = axis_endpoint(token.text());
            let whole_axis = axis.is_some()
                && [i.checked_sub(1), Some(i + 1)]
                    .into_iter()
                    .flatten()
                    .any(|colon| {
                        if tokens
                            .get(colon)
                            .is_none_or(|t| t.kind() != TokenKind::Colon)
                        {
                            return false;
                        }
                        let other = if colon < i {
                            colon.checked_sub(1)
                        } else {
                            Some(colon + 1)
                        };
                        other
                            .and_then(|other| tokens.get(other))
                            .is_some_and(|other| axis_endpoint(other.text()) == axis)
                    });
            ((crate::display::parse_address(token.text()).is_some() || whole_axis)
                && !next.is_some_and(|t| matches!(t.kind(), TokenKind::Bang | TokenKind::LParen)))
            .then_some(token.span())
        })
        .collect()
}

/// Whole rows and columns have one anchor bit; a colon distinguishes them from names/numbers.
fn axis_endpoint(text: &str) -> Option<bool> {
    let text = text.strip_prefix('$').unwrap_or(text);
    if text.bytes().all(|b| b.is_ascii_alphabetic())
        && crate::display::parse_address(&format!("{text}1")).is_some()
    {
        Some(true)
    } else if !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && text
            .parse::<u32>()
            .is_ok_and(|r| (1..=1_048_576).contains(&r))
    {
        Some(false)
    } else {
        None
    }
}

/// The A1 reference at the caret, or the complete cell/range selection around it.
/// Strings, function names and sheet qualifiers are never editable reference endpoints.
pub fn reference_span(text: &str, start: usize, end: usize) -> Option<TextSpan> {
    if !text.trim_start().starts_with('=') {
        return None;
    }
    let spans = cell_reference_spans(text);
    if start == end {
        spans
            .iter()
            .find(|s| s.start < start && start <= s.end())
            .or_else(|| spans.iter().find(|s| s.start == start))
            .copied()
    } else {
        let selected: Vec<_> = spans
            .into_iter()
            .filter(|s| start <= s.start && s.end() <= end)
            .collect();
        Some(TextSpan::covering(*selected.first()?, *selected.last()?))
    }
}

/// The reference being replaced by a point gesture includes its sheet qualifier.
pub fn point_reference_span(text: &str, start: usize, end: usize) -> Option<TextSpan> {
    let mut span = reference_span(text, start, end)?;
    let tokens: Vec<_> = lex(text)
        .into_iter()
        .filter(|t| t.kind() != TokenKind::Whitespace)
        .collect();
    let i = tokens.iter().position(|t| t.span().start == span.start)?;
    if i >= 2 && tokens[i - 1].kind() == TokenKind::Bang {
        let mut first = i - 2;
        if first >= 2 && tokens[first - 1].kind() == TokenKind::Colon {
            first -= 2;
        }
        if first > 0 && tokens[first - 1].kind() == TokenKind::BracketedQualifier {
            first -= 1;
        }
        span = TextSpan::new(
            tokens[first].span().start,
            span.end() - tokens[first].span().start,
        );
    }
    Some(span)
}

/// A formula caret where a pointed reference can be inserted without joining two operands.
pub fn expects_reference(text: &str, start: usize, end: usize) -> bool {
    if !text.trim_start().starts_with('=') {
        return false;
    }
    let tokens = lex(text);
    if tokens.iter().any(|t| {
        matches!(
            t.kind(),
            TokenKind::StringLiteral | TokenKind::QuotedIdentifier | TokenKind::BracketedQualifier
        ) && t.span().start < start
            && start < t.span().end()
    }) {
        return false;
    }
    if start != end {
        return reference_span(text, start, end).is_some();
    }
    tokens
        .iter()
        .rev()
        .find(|t| {
            t.span().end() <= start && !matches!(t.kind(), TokenKind::Whitespace | TokenKind::Eof)
        })
        .is_some_and(|t| {
            matches!(
                t.kind(),
                TokenKind::Equals
                    | TokenKind::Plus
                    | TokenKind::Minus
                    | TokenKind::Star
                    | TokenKind::Slash
                    | TokenKind::Caret
                    | TokenKind::Ampersand
                    | TokenKind::LParen
                    | TokenKind::Comma
                    | TokenKind::Semicolon
                    | TokenKind::Less
                    | TokenKind::LessEqual
                    | TokenKind::Greater
                    | TokenKind::GreaterEqual
                    | TokenKind::NotEqual
                    | TokenKind::Colon
            )
        })
}

/// F4's relative → absolute → row-absolute → column-absolute cycle, at a cell or
/// over selected A1 endpoints. Qualifiers and text outside those endpoints stay exact.
pub fn cycle_anchors(text: &str, start: usize, end: usize) -> Option<(String, usize, usize)> {
    let span = reference_span(text, start, end)?;
    let units: Vec<u16> = text.encode_utf16().collect();
    let selected = cell_reference_spans(text)
        .into_iter()
        .filter(|s| span.start <= s.start && s.end() <= span.end());
    let mut out = units[..span.start].to_vec();
    let mut from = span.start;
    for at in selected {
        out.extend_from_slice(&units[from..at.start]);
        let endpoint = String::from_utf16_lossy(&units[at.start..at.end()]);
        if crate::display::parse_address(&endpoint).is_none() {
            let next = if endpoint.starts_with('$') {
                endpoint[1..].to_owned()
            } else {
                format!("${endpoint}")
            };
            out.extend(next.encode_utf16());
            from = at.end();
            continue;
        }
        let split = endpoint.find(|c: char| c.is_ascii_digit())?;
        let column_absolute = endpoint.starts_with('$');
        let row_absolute = endpoint[..split].ends_with('$');
        let column = endpoint[..split].trim_matches('$');
        let row = &endpoint[split..];
        let (next_col, next_row) = match (column_absolute, row_absolute) {
            (false, false) => (true, true),
            (true, true) => (false, true),
            (false, true) => (true, false),
            (true, false) => (false, false),
        };
        let next = format!(
            "{}{}{}{}",
            if next_col { "$" } else { "" },
            column,
            if next_row { "$" } else { "" },
            row
        );
        out.extend(next.encode_utf16());
        from = at.end();
    }
    out.extend_from_slice(&units[from..span.end()]);
    let new_end = out.len();
    out.extend_from_slice(&units[span.end()..]);
    Some((
        String::from_utf16_lossy(&out),
        if start == end { new_end } else { span.start },
        new_end,
    ))
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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn text_undo_stays_inside_the_current_edit_and_branches_after_undo() {
        let mut history = TextHistory::default();
        history.reset("=A1".into(), 3);
        history.record("=A1+2".into(), 5);
        assert_eq!(history.step(false), Some(("=A1".into(), 3)));
        assert_eq!(history.step(false), None);
        assert_eq!(history.step(true), Some(("=A1+2".into(), 5)));
        history.step(false);
        history.record("=A1+3".into(), 5);
        assert_eq!(history.step(true), None);
        history.reset("new session".into(), 11);
        assert_eq!(history.step(false), None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn f4_cycles_only_references_and_keeps_strings_qualifiers_and_utf16_offsets() {
        let mut text = "=A1".to_owned();
        for expected in ["=$A$1", "=A$1", "=$A1", "=A1"] {
            let at = text.len();
            text = cycle_anchors(&text, at, at).unwrap().0;
            assert_eq!(text, expected);
        }
        let text = "=\"😀A1\"&'A1'!B2+C3";
        let start = text.encode_utf16().count() - 5;
        let end = text.encode_utf16().count();
        let (next, _, _) = cycle_anchors(text, start, end).unwrap();
        assert_eq!(next, "=\"😀A1\"&'A1'!$B$2+$C$3");
        assert_eq!(cycle_anchors("=LOG10(A1)", 5, 5), None);
        assert_eq!(cycle_anchors("=\"A1\"", 3, 3), None);
        assert_eq!(cycle_anchors("=A1!B2", 3, 3), None);
        assert_eq!(cycle_anchors("=XFE1", 5, 5), None);
        assert_eq!(cycle_anchors("=A1048577", 9, 9), None);
        assert_eq!(cycle_anchors("text A1", 7, 7), None);
        assert_eq!(
            cycle_anchors("=SUM(A1:B3)", 5, 10).unwrap().0,
            "=SUM($A$1:$B$3)"
        );
        assert_eq!(
            point_reference_span("='Q2 Data'!A1+1", 12, 12),
            Some(TextSpan::new(1, 12))
        );
        assert_eq!(cycle_anchors("=SUM(A:B)", 5, 8).unwrap().0, "=SUM($A:$B)");
        assert_eq!(cycle_anchors("=SUM(1:3)", 5, 8).unwrap().0, "=SUM($1:$3)");
        assert_eq!(cycle_anchors("=42", 3, 3), None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_point_slot_excludes_text_and_does_not_concatenate_operands() {
        for text in ["=", "=A1+", "=SUM(", "=IF(A1, ", "=A1:"] {
            assert!(
                expects_reference(
                    text,
                    text.encode_utf16().count(),
                    text.encode_utf16().count()
                ),
                "{text}"
            );
        }
        for (text, at) in [
            ("plain", 5),
            ("=\"A1+", 5),
            ("='A1+", 5),
            ("=42", 3),
            ("=A1", 3),
            ("=SUM(A1)", 8),
        ] {
            assert!(!expects_reference(text, at, at), "{text}");
        }
        assert!(expects_reference("=A1+B2", 4, 6));
        assert_eq!(
            replace_text("=\"😀\"&A1", 6, 8, "B4"),
            ("=\"😀\"&B4".into(), 8)
        );
        assert_eq!(crate::display::parse_address("$a$1"), Some((1, 1)));
        for invalid in ["A$$1", "A1$", "$A$0", "XFE1", "A1048577", "1", "$$A1"] {
            assert_eq!(crate::display::parse_address(invalid), None, "{invalid}");
        }
    }
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
        assert_eq!(
            popup_key("Enter"),
            None,
            "worksheet Enter belongs to the field's owner"
        );
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
