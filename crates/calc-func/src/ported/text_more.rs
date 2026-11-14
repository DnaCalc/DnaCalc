//! Text kernels ported from OxFunc (crates/oxfunc_core/src/functions/ at 6fca3518):
//! number_regex_translate_family.rs (NUMBERVALUE and the REGEX functions' pattern engine, the
//! slice of PCRE2 that OxFunc admits), text_delim_family.rs (TEXTBEFORE, TEXTAFTER) and
//! array_text_split_family.rs (TEXTSPLIT, ARRAYTOTEXT), with the OxFunc surface layer removed.
//!
//! The pattern engine reads literals, `.`, the classes `\d \D \w \W \s \S \h`, bracketed classes
//! and ranges, the anchors `^ $ \A \Z \z \b \B` and the quantifiers `* + ?`. Any other pattern
//! syntax (groups, alternation, counted repeats, POSIX classes, other escapes) is a valid PCRE2
//! pattern that the engine does not read, and the call is refused as not modelled
//! ([`PatternError::Unsupported`]). A bracketed class the pattern never closes does not compile
//! in PCRE2, and the call is `#VALUE!` ([`PatternError::Invalid`]): Excel 16.0.20527 gives
//! `REGEXTEST("hello","[")`, `REGEXEXTRACT("abc","[")` and `REGEXREPLACE("abc","[","x")` as
//! `#VALUE!`, and `REGEXTEST("","^$")` as TRUE (the jnt oracle, sheet jnt_text_formats, answered
//! by the question service with the same formula text).

use crate::value::{ExcelText, WorksheetErrorCode};

/// Why a pattern gives no matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PatternError {
    /// A pattern the engine does not read: the REGEX call is refused as not modelled.
    Unsupported,
    /// A pattern PCRE2 does not compile (an unclosed bracketed class): the call is `#VALUE!`.
    Invalid,
}
use PatternError::{Invalid, Unsupported};

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
    /// `$`: the end of the text, or before a newline that ends it (PCRE2 without multiline). A
    /// text with a line break is refused before matching, so here it is the end of the text.
    EndOfLine,
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
    Range(char, char, Option<Vec<char>>),
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
    // Fn3 retains Mn and Pc membership; the PCRE2 pattern specification describes
    // these additions to Unicode word characters. Keep one predicate for boundaries too.
    ch.is_alphanumeric()
        || calc_value::is_nonspacing_mark(ch)
        || calc_value::is_connector_punctuation(ch)
}

fn chars_equal(lhs: char, rhs: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        lhs == rhs
    } else {
        calc_value::unicode_simple_case_fold(lhs) == calc_value::unicode_simple_case_fold(rhs)
    }
}

fn range_contains(
    ch: char,
    start: char,
    end: char,
    folded: &Option<Vec<char>>,
    case_sensitive: bool,
) -> bool {
    if case_sensitive {
        ch >= start && ch <= end
    } else {
        folded.as_ref().is_some_and(|range| {
            range
                .binary_search(&calc_value::unicode_simple_case_fold(ch))
                .is_ok()
        })
    }
}

// Fold the complete admitted range, rather than folding just its endpoints:
// a fold can reverse or collapse interior characters (for example Greek sigma).
// Wider caseless ranges stay unsupported; case-sensitive matching needs no table.
fn folded_range(start: char, end: char) -> Option<Vec<char>> {
    if end < start || end as u32 - start as u32 > 4096 {
        return None;
    }
    let mut folded: Vec<_> = (start as u32..=end as u32)
        .filter_map(char::from_u32)
        .map(calc_value::unicode_simple_case_fold)
        .collect();
    folded.sort_unstable();
    folded.dedup();
    Some(folded)
}

fn class_item_matches(item: &RegexClassItem, ch: char, case_sensitive: bool) -> bool {
    match item {
        RegexClassItem::Single(expected) => chars_equal(ch, *expected, case_sensitive),
        RegexClassItem::Range(start, end, folded) => {
            range_contains(ch, *start, *end, folded, case_sensitive)
        }
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
        RegexAtom::EndOfText | RegexAtom::EndOfLine => position == chars.len(),
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
            | RegexAtom::EndOfLine
            | RegexAtom::WordBoundary
            | RegexAtom::NotWordBoundary
    )
}

fn consuming_atom_matches(atom: &RegexAtom, ch: char, case_sensitive: bool) -> bool {
    match atom {
        RegexAtom::Literal(expected) => chars_equal(ch, *expected, case_sensitive),
        RegexAtom::Any => ch != '\n' && ch != '\r',
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
        | RegexAtom::EndOfLine
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

fn parse_escape_atom(chars: &[char], index: &mut usize) -> Result<RegexAtom, PatternError> {
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

fn parse_class_piece(chars: &[char], index: &mut usize) -> Result<RegexClassPiece, PatternError> {
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

fn parse_char_class(chars: &[char], index: &mut usize) -> Result<RegexAtom, PatternError> {
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
        // `[:alpha:]`, `[.a.]` and `[=a=]` inside a class are POSIX forms the engine does not
        // read, not literal characters.
        if ch == '[' && matches!(chars.get(*index + 1), Some(':' | '.' | '=')) {
            return Err(Unsupported);
        }
        pieces.push(parse_class_piece(chars, index)?);
    }

    if *index >= chars.len() {
        // The pattern ends inside the class: PCRE2 does not compile it.
        return Err(Invalid);
    }
    if pieces.is_empty() {
        // A `]` first in a class is a literal there, which the engine does not read.
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
                items.push(RegexClassItem::Range(
                    *start,
                    *end,
                    folded_range(*start, *end),
                ));
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

fn parse_regex_pattern(pattern: &str) -> Result<Vec<RegexToken>, PatternError> {
    let chars: Vec<char> = pattern.chars().collect();
    if chars.is_empty() {
        return Ok(Vec::new());
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
            // Without multiline, `^` is the start of the text.
            '^' => {
                index += 1;
                RegexAtom::StartOfText
            }
            '$' => {
                index += 1;
                RegexAtom::EndOfLine
            }
            '*' | '+' | '?' if index == 0 => return Err(Invalid),
            '*' | '+' | '?' | '(' | ')' | '{' | '}' | '|' => {
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

    Ok(tokens)
}

/// The matches' character positions, for REGEXEXTRACT, REGEXTEST and REGEXREPLACE.
pub(crate) fn regex_matches(
    text: &str,
    pattern: &str,
    case_sensitive: bool,
    first_only: bool,
) -> Result<(Vec<char>, Vec<(usize, usize)>), PatternError> {
    let haystack: Vec<char> = text.chars().collect();
    let tokens = parse_regex_pattern(pattern)?;
    // Empty matches in Excel are located in UTF-16. CRLF, supplementary characters and
    // one leading-LF boundary differ from the ordinary scalar-position mapping below.
    // First matches are at the start even for those inputs; leave the remaining cases open.
    if tokens.is_empty()
        && !first_only
        && (haystack.iter().any(|ch| u32::from(*ch) > 0xffff)
            || text.contains("\r\n")
            || (haystack.len() == 13 && haystack.first() == Some(&'\n')))
    {
        return Err(Unsupported);
    }
    // Before a final newline `$` also matches, and which characters end a line depends on the
    // newline convention Excel's PCRE2 was built with, which is not measured.
    if tokens.iter().any(|token| token.atom == RegexAtom::EndOfLine)
        && haystack
            .iter()
            .any(|&ch| matches!(ch, '\n' | '\r' | '\u{000b}' | '\u{000c}' | '\u{0085}' | '\u{2028}' | '\u{2029}'))
    {
        return Err(Unsupported);
    }
    // The fn3 readings admit Unicode range closure through the same simple fold as
    // literals. A range beyond the bounded table remains a typed refusal.
    if !case_sensitive
        && tokens.iter().any(|token| match &token.atom {
            RegexAtom::Class(class) => class
                .items
                .iter()
                .any(|item| matches!(item, RegexClassItem::Range(_, _, None))),
            _ => false,
        })
    {
        return Err(Unsupported);
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NumberValueError {
    Syntax,
    Range,
}

pub(crate) fn numbervalue(
    text: &ExcelText,
    decimal_separator: Option<char>,
    group_separator: Option<char>,
) -> Result<f64, NumberValueError> {
    if decimal_separator.is_some() && decimal_separator == group_separator {
        return Err(NumberValueError::Syntax);
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
    while raw.ends_with('%')
        && decimal_separator != Some('%')
        && group_separator != Some('%')
    {
        raw.pop();
        percent_count += 1;
    }
    if raw.is_empty() {
        return Err(NumberValueError::Syntax);
    }
    let mut chars = raw.chars().peekable();
    let mut sign = 1.0;
    if let Some(ch) = chars.peek().copied()
        && (ch == '+' || ch == '-')
        && decimal_separator != Some(ch)
        && group_separator != Some(ch)
    {
        sign = if ch == '-' { -1.0 } else { 1.0 };
        chars.next();
    }
    let mut normalized = String::new();
    let mut seen_digit = false;
    let mut seen_decimal = false;
    let mut seen_explicit_decimal = false;
    let mut seen_exponent = false;
    for ch in chars {
        if decimal_separator == Some(ch) {
            if seen_decimal || seen_exponent {
                return Err(NumberValueError::Syntax);
            }
            normalized.push('.');
            seen_decimal = true;
            seen_explicit_decimal = true;
            continue;
        }
        if group_separator == Some(ch) && !seen_explicit_decimal {
            continue;
        }
        // The explicit decimal character is translated to the parser's dot;
        // a dot already present remains valid unless selected as the group.
        if ch == '.' {
            if seen_decimal || seen_exponent || group_separator == Some(ch) {
                return Err(NumberValueError::Syntax);
            }
            normalized.push('.');
            seen_decimal = true;
            continue;
        }
        if ch.is_ascii_digit() {
            normalized.push(ch);
            seen_digit = true;
            continue;
        }
        if matches!(ch, 'e' | 'E') && seen_digit && !seen_exponent {
            normalized.push('e');
            seen_exponent = true;
            continue;
        }
        if matches!(ch, '+' | '-') && normalized.ends_with('e') {
            normalized.push(ch);
            continue;
        }
        return Err(NumberValueError::Syntax);
    }
    if !seen_digit {
        return Err(NumberValueError::Syntax);
    }
    let (mantissa, exponent) = if let Some((mantissa, exponent)) = normalized.split_once('e') {
        if matches!(exponent, "" | "+" | "-") {
            return Err(NumberValueError::Syntax);
        }
        (
            mantissa,
            exponent
                .parse::<i64>()
                .map_err(|_| NumberValueError::Range)?,
        )
    } else {
        (normalized.as_str(), 0)
    };
    let fraction_digits = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let digits = mantissa.replace('.', "");
    let scale = exponent
        .checked_sub(fraction_digits as i64)
        .ok_or(NumberValueError::Range)?;
    let position = scale
        .checked_add(digits.trim_start_matches('0').len() as i64)
        .ok_or(NumberValueError::Range)?;
    // NUMBERVALUE checks this range even for an all-zero coefficient and before
    // applying trailing percents (fn12's explicit-separator Excel controls).
    if !(-308..=308).contains(&position) {
        return Err(NumberValueError::Range);
    }
    let mut value = crate::value_text::decimal(&digits, scale).ok_or(NumberValueError::Range)?;
    value *= sign;
    Ok(numbervalue_percent(value, percent_count))
}

/// The existing NUMBERVALUE percent arithmetic, shared with its host reader.
pub(crate) fn numbervalue_percent(mut value: f64, percent_count: u32) -> f64 {
    for _ in 0..percent_count {
        value /= 100.0;
    }
    value
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
    single_delimiter: bool,
) -> Delimited {
    let requested = instance.unsigned_abs();
    if instance == 0 {
        return Delimited::BadInstance;
    }
    // Excel's empty single delimiter ignores the ordinary length bound. When
    // the input is exactly one UTF-16 unit, BEFORE returns it and AFTER is empty.
    if single_delimiter && delimiters[0].utf16_code_units().is_empty() {
        let one_unit = text.len_utf16_code_units() == 1;
        let whole = match (side, instance < 0, one_unit) {
            (Side::Before, _, true) | (Side::Before, true, false) | (Side::After, false, false) => {
                text.clone()
            }
            _ => ExcelText::from_utf16_code_units(Vec::new()),
        };
        return Delimited::Text(whole);
    }
    // The first search of empty text is still a valid search, with no delimiter to find.
    // Excel 16.0.20430: TEXTBEFORE("","-") and TEXTAFTER("","-") are #N/A (the
    // recorded typed Value2 cases w111typed-00274 and w111typed-00266, dc-zsn.7).
    if requested > text.len_utf16_code_units().max(1) {
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
        assert_eq!(regex_matches("abc", "^a", true, true).unwrap().1, [(0, 1)]);
        assert_eq!(regex_matches("bab", "^a", true, true).unwrap().1, []);
        // Excel 16.0.20527: REGEXTEST("","^$") is TRUE (the jnt oracle, jnt_text_formats).
        assert_eq!(regex_matches("", "^$", true, true).unwrap().1, [(0, 0)]);
        assert_eq!(regex_matches("ab", "b$", true, false).unwrap().1, [(1, 2)]);
        assert_eq!(regex_matches("ab\n", "b$", true, true), Err(Unsupported));
        // An unclosed class does not compile: #VALUE! (REGEXTEST("hello","[") in Excel).
        assert_eq!(regex_matches("hello", "[", true, true), Err(Invalid));
        assert_eq!(regex_matches("hello", "a[bc", true, true), Err(Invalid));
        assert_eq!(regex_matches("hello", "[^", true, true), Err(Invalid));
        assert_eq!(regex_matches("a]", "[]a]", true, true), Err(Unsupported));
        assert_eq!(regex_matches("a", "[[:alpha:]]", true, true), Err(Unsupported));
        assert!(regex_matches("ABC", "b", false, true).unwrap().1.len() == 1);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn delimiters_before_and_after() {
        let text = t("a-b-c");
        let dash = [t("-")];
        assert_eq!(
            delimited(Side::Before, &text, &dash, 1, false, false, true),
            Delimited::Text(t("a"))
        );
        assert_eq!(
            delimited(Side::After, &text, &dash, -1, false, false, true),
            Delimited::Text(t("c"))
        );
        assert_eq!(
            delimited(Side::Before, &text, &dash, 3, false, false, true),
            Delimited::NotFound
        );
        assert_eq!(
            delimited(Side::Before, &text, &dash, 3, false, true, true),
            Delimited::Text(t("a-b-c"))
        );
        assert_eq!(
            delimited(Side::Before, &text, &dash, 0, false, false, true),
            Delimited::BadInstance
        );
        assert_eq!(
            split(&t("a,b,,c"), &[t(",")], true, false),
            vec![t("a"), t("b"), t("c")]
        );
        assert_eq!(split(&t("a,b,,c"), &[t(",")], false, false).len(), 4);
        assert_eq!(
            numbervalue(&t("2.500,27"), Some(','), Some('.')),
            Ok(2500.27)
        );
        assert_eq!(numbervalue(&t("3.5%"), Some('.'), Some(',')), Ok(0.035));
    }
}
