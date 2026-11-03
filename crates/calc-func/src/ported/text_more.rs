//! Text kernels ported from OxFunc (crates/oxfunc_core/src/functions/ at 6fca3518):
//! number_regex_translate_family.rs (NUMBERVALUE and the REGEX functions' pattern engine, the
//! slice of PCRE2 that OxFunc admits), text_delim_family.rs (TEXTBEFORE, TEXTAFTER) and
//! array_text_split_family.rs (TEXTSPLIT, ARRAYTOTEXT), with the OxFunc surface layer removed.
//!
//! The pattern engine reads literals, `.`, the classes `\d \D \w \W \s \S \h`, bracketed classes
//! and ranges, the anchors `\A \Z \z \b \B` and the quantifiers `* + ?`. Any other pattern syntax
//! (groups, alternation, `^`, `$`, counted repeats, other escapes) is a valid PCRE2 pattern that
//! the engine does not read, and the call is refused as not modelled ([`Unsupported`]).

use crate::value::{ExcelText, WorksheetErrorCode};

/// A pattern the engine does not read: the REGEX call is refused as not modelled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Unsupported;

#[derive(Debug, Clone, PartialEq, Eq)]
enum RegexAtom {
    Literal(char),
    Any,
    Digit,
    NotDigit,
    Word,
    NotWord,
    Space,
    NotSpace,
    HorizontalSpace,
    StartOfText,
    EndOfText,
    WordBoundary,
    NotWordBoundary,
    Class(RegexClass),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegexClass {
    negated: bool,
    items: Vec<RegexClassItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RegexClassItem {
    Single(char),
    Range(char, char),
    Digit,
    Word,
    Space,
    HorizontalSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegexQuantifier {
    One,
    ZeroOrMore,
    OneOrMore,
    ZeroOrOne,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegexToken {
    atom: RegexAtom,
    quantifier: RegexQuantifier,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RegexClassPiece {
    Literal(char),
    Digit,
    Word,
    Space,
    HorizontalSpace,
}

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn chars_equal(lhs: char, rhs: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        lhs == rhs
    } else if lhs.is_ascii() && rhs.is_ascii() {
        lhs.eq_ignore_ascii_case(&rhs)
    } else {
        lhs == rhs
    }
}

fn range_contains(ch: char, start: char, end: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        ch >= start && ch <= end
    } else if ch.is_ascii() && start.is_ascii() && end.is_ascii() {
        let folded = ch.to_ascii_lowercase();
        let lo = start.to_ascii_lowercase();
        let hi = end.to_ascii_lowercase();
        folded >= lo && folded <= hi
    } else {
        ch >= start && ch <= end
    }
}

fn class_item_matches(item: &RegexClassItem, ch: char, case_sensitive: bool) -> bool {
    match item {
        RegexClassItem::Single(expected) => chars_equal(ch, *expected, case_sensitive),
        RegexClassItem::Range(start, end) => range_contains(ch, *start, *end, case_sensitive),
        RegexClassItem::Digit => ch.is_ascii_digit(),
        RegexClassItem::Word => is_word_char(ch),
        RegexClassItem::Space => ch.is_whitespace(),
        RegexClassItem::HorizontalSpace => ch == ' ' || ch == '\t',
    }
}

fn is_word_boundary(chars: &[char], position: usize) -> bool {
    let before = position
        .checked_sub(1)
        .and_then(|index| chars.get(index))
        .copied()
        .is_some_and(is_word_char);
    let after = chars.get(position).copied().is_some_and(is_word_char);
    before != after
}

fn zero_width_atom_matches(atom: &RegexAtom, chars: &[char], position: usize) -> Option<bool> {
    Some(match atom {
        RegexAtom::StartOfText => position == 0,
        RegexAtom::EndOfText => position == chars.len(),
        RegexAtom::WordBoundary => is_word_boundary(chars, position),
        RegexAtom::NotWordBoundary => !is_word_boundary(chars, position),
        _ => return None,
    })
}

fn atom_consumes_input(atom: &RegexAtom) -> bool {
    !matches!(
        atom,
        RegexAtom::StartOfText
            | RegexAtom::EndOfText
            | RegexAtom::WordBoundary
            | RegexAtom::NotWordBoundary
    )
}

fn consuming_atom_matches(atom: &RegexAtom, ch: char, case_sensitive: bool) -> bool {
    match atom {
        RegexAtom::Literal(expected) => chars_equal(ch, *expected, case_sensitive),
        RegexAtom::Any => true,
        RegexAtom::Digit => ch.is_ascii_digit(),
        RegexAtom::NotDigit => !ch.is_ascii_digit(),
        RegexAtom::Word => is_word_char(ch),
        RegexAtom::NotWord => !is_word_char(ch),
        RegexAtom::Space => ch.is_whitespace(),
        RegexAtom::NotSpace => !ch.is_whitespace(),
        RegexAtom::HorizontalSpace => ch == ' ' || ch == '\t',
        RegexAtom::Class(class) => {
            let found = class
                .items
                .iter()
                .any(|item| class_item_matches(item, ch, case_sensitive));
            if class.negated { !found } else { found }
        }
        RegexAtom::StartOfText
        | RegexAtom::EndOfText
        | RegexAtom::WordBoundary
        | RegexAtom::NotWordBoundary => false,
    }
}

fn match_atom_once(
    token: &RegexToken,
    chars: &[char],
    position: usize,
    case_sensitive: bool,
) -> Option<usize> {
    if let Some(matches) = zero_width_atom_matches(&token.atom, chars, position) {
        return matches.then_some(position);
    }
    let ch = *chars.get(position)?;
    if consuming_atom_matches(&token.atom, ch, case_sensitive) {
        Some(position + 1)
    } else {
        None
    }
}

fn match_tokens_from(
    tokens: &[RegexToken],
    token_index: usize,
    chars: &[char],
    position: usize,
    case_sensitive: bool,
) -> Option<usize> {
    if token_index >= tokens.len() {
        return Some(position);
    }

    let token = &tokens[token_index];
    match token.quantifier {
        RegexQuantifier::One => {
            match_atom_once(token, chars, position, case_sensitive).and_then(|next| {
                match_tokens_from(tokens, token_index + 1, chars, next, case_sensitive)
            })
        }
        RegexQuantifier::ZeroOrOne => {
            if let Some(next) =
                match_atom_once(token, chars, position, case_sensitive).and_then(|next| {
                    match_tokens_from(tokens, token_index + 1, chars, next, case_sensitive)
                })
            {
                return Some(next);
            }
            match_tokens_from(tokens, token_index + 1, chars, position, case_sensitive)
        }
        RegexQuantifier::ZeroOrMore | RegexQuantifier::OneOrMore => {
            let minimum = if matches!(token.quantifier, RegexQuantifier::OneOrMore) {
                1usize
            } else {
                0usize
            };
            let mut positions = vec![position];
            let mut cursor = position;
            while let Some(next) = match_atom_once(token, chars, cursor, case_sensitive) {
                positions.push(next);
                cursor = next;
            }
            if positions.len().saturating_sub(1) < minimum {
                return None;
            }
            for count in (minimum..positions.len()).rev() {
                if let Some(end) = match_tokens_from(
                    tokens,
                    token_index + 1,
                    chars,
                    positions[count],
                    case_sensitive,
                ) {
                    return Some(end);
                }
            }
            None
        }
    }
}

fn find_first_match_from(
    chars: &[char],
    tokens: &[RegexToken],
    start_index: usize,
    case_sensitive: bool,
) -> Option<(usize, usize)> {
    for start in start_index..=chars.len() {
        if let Some(end) = match_tokens_from(tokens, 0, chars, start, case_sensitive) {
            if end >= start {
                return Some((start, end));
            }
        }
    }
    None
}

fn find_all_matches(
    chars: &[char],
    tokens: &[RegexToken],
    case_sensitive: bool,
) -> Vec<(usize, usize)> {
    let mut matches = Vec::new();
    let mut cursor = 0usize;
    while cursor <= chars.len() {
        let Some((start, end)) = find_first_match_from(chars, tokens, cursor, case_sensitive)
        else {
            break;
        };
        matches.push((start, end));
        cursor = end.max(start + 1);
    }
    matches
}

fn escaped_literal_char(ch: char) -> Option<char> {
    Some(match ch {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        'f' => '\u{000c}',
        'v' => '\u{000b}',
        'e' => '\u{001b}',
        '\\' | '.' | '*' | '+' | '?' | '[' | ']' | '(' | ')' | '|' | '^' | '$' | '/' => ch,
        _ => return None,
    })
}

fn parse_escape_atom(chars: &[char], index: &mut usize) -> Result<RegexAtom, Unsupported> {
    let Some(ch) = chars.get(*index).copied() else {
        return Err(Unsupported);
    };
    *index += 1;
    Ok(match ch {
        'd' => RegexAtom::Digit,
        'D' => RegexAtom::NotDigit,
        'w' => RegexAtom::Word,
        'W' => RegexAtom::NotWord,
        's' => RegexAtom::Space,
        'S' => RegexAtom::NotSpace,
        'h' => RegexAtom::HorizontalSpace,
        'A' => RegexAtom::StartOfText,
        'Z' | 'z' => RegexAtom::EndOfText,
        'b' => RegexAtom::WordBoundary,
        'B' => RegexAtom::NotWordBoundary,
        ch => {
            let Some(literal) = escaped_literal_char(ch) else {
                // Any other escape sequence is not part of the admitted slice; produce #VALUE!
                // so the caller gets a hard error rather than a silently wrong literal match.
                return Err(Unsupported);
            };
            RegexAtom::Literal(literal)
        }
    })
}

fn parse_class_piece(
    chars: &[char],
    index: &mut usize,
) -> Result<RegexClassPiece, Unsupported> {
    let Some(ch) = chars.get(*index).copied() else {
        return Err(Unsupported);
    };
    if ch == '\\' {
        *index += 1;
        let Some(escaped) = chars.get(*index).copied() else {
            return Err(Unsupported);
        };
        *index += 1;
        return match escaped {
            'd' => Ok(RegexClassPiece::Digit),
            'w' => Ok(RegexClassPiece::Word),
            's' => Ok(RegexClassPiece::Space),
            'h' => Ok(RegexClassPiece::HorizontalSpace),
            // Negated shorthands are not supported inside character classes.
            'D' | 'W' | 'S' => Err(Unsupported),
            '-' => Ok(RegexClassPiece::Literal('-')),
            ch => {
                let Some(literal) = escaped_literal_char(ch) else {
                    // Unrecognized escape inside a character class: hard error, not silent literal.
                    return Err(Unsupported);
                };
                Ok(RegexClassPiece::Literal(literal))
            }
        };
    }
    *index += 1;
    Ok(RegexClassPiece::Literal(ch))
}

fn parse_char_class(chars: &[char], index: &mut usize) -> Result<RegexAtom, Unsupported> {
    let mut negated = false;
    if matches!(chars.get(*index), Some('^')) {
        negated = true;
        *index += 1;
    }

    let mut pieces = Vec::new();
    while let Some(ch) = chars.get(*index).copied() {
        if ch == ']' {
            break;
        }
        pieces.push(parse_class_piece(chars, index)?);
    }

    if !matches!(chars.get(*index), Some(']')) || pieces.is_empty() {
        return Err(Unsupported);
    }
    *index += 1;

    let mut items = Vec::new();
    let mut i = 0usize;
    while i < pieces.len() {
        if i + 2 < pieces.len() {
            if let (
                RegexClassPiece::Literal(start),
                RegexClassPiece::Literal('-'),
                RegexClassPiece::Literal(end),
            ) = (&pieces[i], &pieces[i + 1], &pieces[i + 2])
            {
                items.push(RegexClassItem::Range(*start, *end));
                i += 3;
                continue;
            }
        }
        match &pieces[i] {
            RegexClassPiece::Literal(ch) => items.push(RegexClassItem::Single(*ch)),
            RegexClassPiece::Digit => items.push(RegexClassItem::Digit),
            RegexClassPiece::Word => items.push(RegexClassItem::Word),
            RegexClassPiece::Space => items.push(RegexClassItem::Space),
            RegexClassPiece::HorizontalSpace => items.push(RegexClassItem::HorizontalSpace),
        }
        i += 1;
    }

    Ok(RegexAtom::Class(RegexClass { negated, items }))
}

fn parse_regex_pattern(pattern: &str) -> Result<Vec<RegexToken>, Unsupported> {
    let chars: Vec<char> = pattern.chars().collect();
    if chars.is_empty() {
        return Err(Unsupported);
    }

    let mut tokens = Vec::new();
    let mut index = 0usize;
    while let Some(ch) = chars.get(index).copied() {
        let atom = match ch {
            '.' => {
                index += 1;
                RegexAtom::Any
            }
            '\\' => {
                index += 1;
                parse_escape_atom(&chars, &mut index)?
            }
            '[' => {
                index += 1;
                parse_char_class(&chars, &mut index)?
            }
            '*' | '+' | '?' | '(' | ')' | '{' | '}' | '|' | '^' | '$' => {
                return Err(Unsupported);
            }
            other => {
                index += 1;
                RegexAtom::Literal(other)
            }
        };

        let quantifier = match chars.get(index).copied() {
            Some('*') => {
                index += 1;
                RegexQuantifier::ZeroOrMore
            }
            Some('+') => {
                index += 1;
                RegexQuantifier::OneOrMore
            }
            Some('?') => {
                index += 1;
                RegexQuantifier::ZeroOrOne
            }
            _ => RegexQuantifier::One,
        };

        if !atom_consumes_input(&atom) && quantifier != RegexQuantifier::One {
            return Err(Unsupported);
        }

        tokens.push(RegexToken { atom, quantifier });
    }

    if tokens.is_empty() {
        return Err(Unsupported);
    }
    Ok(tokens)
}


/// The matches' character positions, for REGEXEXTRACT, REGEXTEST and REGEXREPLACE.
pub(crate) fn regex_matches(
    text: &str,
    pattern: &str,
    case_sensitive: bool,
    first_only: bool,
) -> Result<(Vec<char>, Vec<(usize, usize)>), Unsupported> {
    if !case_sensitive && !(text.is_ascii() && pattern.is_ascii()) {
        // Case folding beyond ASCII is not modelled.
        return Err(Unsupported);
    }
    let haystack: Vec<char> = text.chars().collect();
    let tokens = parse_regex_pattern(pattern)?;
    let found = if first_only {
        find_first_match_from(&haystack, &tokens, 0, case_sensitive)
            .into_iter()
            .collect()
    } else {
        find_all_matches(&haystack, &tokens, case_sensitive)
    };
    Ok((haystack, found))
}

// --- NUMBERVALUE (number_regex_translate_family.rs) ---------------------------------------------

pub(crate) fn numbervalue(
    text: &ExcelText,
    decimal_separator: Option<char>,
    group_separator: Option<char>,
) -> Result<f64, WorksheetErrorCode> {
    if decimal_separator.is_some() && decimal_separator == group_separator {
        return Err(WorksheetErrorCode::Value);
    }
    let mut raw: String = text
        .to_string_lossy()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    if raw.is_empty() {
        // NUMBERVALUE("") is 0 (Microsoft's NUMBERVALUE page).
        return Ok(0.0);
    }
    let mut percent_count = 0u32;
    while raw.ends_with('%') {
        raw.pop();
        percent_count += 1;
    }
    if raw.is_empty() {
        return Err(WorksheetErrorCode::Value);
    }
    let mut chars = raw.chars().peekable();
    let mut sign = 1.0;
    if let Some(ch) = chars.peek().copied()
        && (ch == '+' || ch == '-')
    {
        sign = if ch == '-' { -1.0 } else { 1.0 };
        chars.next();
    }
    let mut normalized = String::new();
    let mut seen_digit = false;
    let mut seen_decimal = false;
    for ch in chars {
        if ch.is_ascii_digit() {
            normalized.push(ch);
            seen_digit = true;
            continue;
        }
        if decimal_separator == Some(ch) {
            if seen_decimal {
                return Err(WorksheetErrorCode::Value);
            }
            normalized.push('.');
            seen_decimal = true;
            continue;
        }
        if group_separator == Some(ch) {
            if seen_decimal {
                return Err(WorksheetErrorCode::Value);
            }
            continue;
        }
        return Err(WorksheetErrorCode::Value);
    }
    if !seen_digit {
        return Err(WorksheetErrorCode::Value);
    }
    let mut value = normalized
        .parse::<f64>()
        .map_err(|_| WorksheetErrorCode::Value)?;
    value *= sign;
    for _ in 0..percent_count {
        value /= 100.0;
    }
    Ok(value)
}

// --- TEXTBEFORE and TEXTAFTER (text_delim_family.rs) -------------------------------------------

fn fold_ascii_case(unit: u16) -> u16 {
    if (b'A' as u16..=b'Z' as u16).contains(&unit) {
        unit + 32
    } else {
        unit
    }
}

fn units_match(left: &[u16], right: &[u16], case_insensitive: bool) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(l, r)| {
            if case_insensitive {
                fold_ascii_case(*l) == fold_ascii_case(*r)
            } else {
                l == r
            }
        })
}

/// Whether TEXTBEFORE or TEXTAFTER is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    Before,
    After,
}

/// What a TEXTBEFORE or TEXTAFTER call finds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Delimited {
    Text(ExcelText),
    /// The delimiter's instance is not found: #N/A, or the call's if_not_found.
    NotFound,
    /// The instance number is 0 or beyond the text's nonempty length: #VALUE!.
    BadInstance,
}

/// TEXTBEFORE or TEXTAFTER over any of `delimiters` (the first found at each position).
pub(crate) fn delimited(
    side: Side,
    text: &ExcelText,
    delimiters: &[ExcelText],
    instance: isize,
    case_insensitive: bool,
    match_end: bool,
) -> Delimited {
    let requested = instance.unsigned_abs();
    // The first search of empty text is still a valid search, with no delimiter to find.
    // Excel 16.0.20430: TEXTBEFORE("","-") and TEXTAFTER("","-") are #N/A (the
    // recorded typed Value2 cases w111typed-00274 and w111typed-00266, dc-zsn.7).
    if instance == 0 || requested > text.len_utf16_code_units().max(1) {
        return Delimited::BadInstance;
    }
    let units = text.utf16_code_units();
    if delimiters.iter().any(|d| d.utf16_code_units().is_empty()) {
        let whole = match (side, instance < 0) {
            (Side::After, false) | (Side::Before, true) => text.clone(),
            _ => ExcelText::from_utf16_code_units(Vec::new()),
        };
        return Delimited::Text(whole);
    }
    // The boundaries: (start, length) of each delimiter found, left to right, not overlapping.
    let mut boundaries = Vec::new();
    let mut at = 0usize;
    while at < units.len() {
        let found = delimiters.iter().find_map(|delimiter| {
            let needle = delimiter.utf16_code_units();
            (at + needle.len() <= units.len()
                && units_match(&units[at..at + needle.len()], needle, case_insensitive))
            .then_some(needle.len())
        });
        match found {
            Some(length) => {
                boundaries.push((at, length));
                at += length;
            }
            None => at += 1,
        }
    }
    if match_end {
        if instance < 0 {
            boundaries.insert(0, (0, 0));
        } else {
            boundaries.push((units.len(), 0));
        }
    }
    let selected = if instance < 0 {
        boundaries
            .len()
            .checked_sub(requested)
            .and_then(|index| boundaries.get(index))
    } else {
        boundaries.get(requested - 1)
    };
    match selected {
        Some(&(start, length)) => Delimited::Text(ExcelText::from_utf16_code_units(match side {
            Side::Before => units[..start].to_vec(),
            Side::After => units[start + length..].to_vec(),
        })),
        None => Delimited::NotFound,
    }
}

// --- TEXTSPLIT (array_text_split_family.rs) -----------------------------------------------------

/// The parts of `text` between any of `delimiters`.
pub(crate) fn split(
    text: &ExcelText,
    delimiters: &[ExcelText],
    ignore_empty: bool,
    case_insensitive: bool,
) -> Vec<ExcelText> {
    let units = text.utf16_code_units();
    if delimiters.is_empty() {
        return vec![text.clone()];
    }
    if delimiters.iter().any(|d| d.utf16_code_units().is_empty()) {
        if units.is_empty() {
            return if ignore_empty {
                Vec::new()
            } else {
                vec![ExcelText::from_utf16_code_units(Vec::new())]
            };
        }
        return units
            .iter()
            .map(|unit| ExcelText::from_utf16_code_units(vec![*unit]))
            .collect();
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut at = start;
    while at < units.len() {
        let found = delimiters.iter().find_map(|delimiter| {
            let needle = delimiter.utf16_code_units();
            (at + needle.len() <= units.len()
                && units_match(&units[at..at + needle.len()], needle, case_insensitive))
            .then_some(needle.len())
        });
        match found {
            Some(length) => {
                if at > start || !ignore_empty {
                    out.push(ExcelText::from_utf16_code_units(units[start..at].to_vec()));
                }
                start = at + length;
                at = start;
            }
            None => at += 1,
        }
    }
    if start < units.len() || !ignore_empty {
        out.push(ExcelText::from_utf16_code_units(units[start..].to_vec()));
    }
    out
}

/// A worksheet error's literal, as ARRAYTOTEXT and VALUETOTEXT write it.
pub(crate) fn error_literal(code: WorksheetErrorCode) -> &'static str {
    match code {
        WorksheetErrorCode::Null => "#NULL!",
        WorksheetErrorCode::Div0 => "#DIV/0!",
        WorksheetErrorCode::Value => "#VALUE!",
        WorksheetErrorCode::Ref => "#REF!",
        WorksheetErrorCode::Name => "#NAME?",
        WorksheetErrorCode::Num => "#NUM!",
        WorksheetErrorCode::NA => "#N/A",
        WorksheetErrorCode::Busy => "#BUSY!",
        WorksheetErrorCode::GettingData => "#GETTING_DATA",
        WorksheetErrorCode::Spill => "#SPILL!",
        WorksheetErrorCode::Calc => "#CALC!",
        WorksheetErrorCode::Field => "#FIELD!",
        WorksheetErrorCode::Blocked => "#BLOCKED!",
        WorksheetErrorCode::Connect => "#CONNECT!",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> ExcelText {
        ExcelText::from_interop_assignment(s)
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn regex_slice_matches_and_refuses_the_rest() {
        let (h, m) = regex_matches("ab12cd345", "[0-9]+", true, false).unwrap();
        let found: Vec<String> = m.iter().map(|&(s, e)| h[s..e].iter().collect()).collect();
        assert_eq!(found, ["12", "345"]);
        assert_eq!(regex_matches("abc", "(a|b)", true, true), Err(Unsupported));
        assert_eq!(regex_matches("abc", "^a", true, true), Err(Unsupported));
        assert!(regex_matches("ABC", "b", false, true).unwrap().1.len() == 1);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn delimiters_before_and_after() {
        let text = t("a-b-c");
        let dash = [t("-")];
        assert_eq!(delimited(Side::Before, &text, &dash, 1, false, false), Delimited::Text(t("a")));
        assert_eq!(delimited(Side::After, &text, &dash, -1, false, false), Delimited::Text(t("c")));
        assert_eq!(delimited(Side::Before, &text, &dash, 3, false, false), Delimited::NotFound);
        assert_eq!(delimited(Side::Before, &text, &dash, 3, false, true), Delimited::Text(t("a-b-c")));
        assert_eq!(delimited(Side::Before, &text, &dash, 0, false, false), Delimited::BadInstance);
        assert_eq!(split(&t("a,b,,c"), &[t(",")], true, false), vec![t("a"), t("b"), t("c")]);
        assert_eq!(split(&t("a,b,,c"), &[t(",")], false, false).len(), 4);
        assert_eq!(numbervalue(&t("2.500,27"), Some(','), Some('.')), Ok(2500.27));
        assert_eq!(numbervalue(&t("3.5%"), Some('.'), Some(',')), Ok(0.035));
    }
}
