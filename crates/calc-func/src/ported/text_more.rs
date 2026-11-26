//! Text kernels ported from OxFunc (crates/oxfunc_core/src/functions/ at 6fca3518):
//! number_regex_translate_family.rs (NUMBERVALUE and the REGEX functions' pattern engine, the
//! slice of PCRE2 that OxFunc admits), text_delim_family.rs (TEXTBEFORE, TEXTAFTER) and
//! array_text_split_family.rs (TEXTSPLIT, ARRAYTOTEXT), with the OxFunc surface layer removed.
//!
//! The pattern engine reads literals, `.`, the classes `\d \D \w \W \s \S \h`, bracketed classes
//! and ranges, the anchors `^ $ \A \Z \z \b \B` and the quantifiers `* + ?`, including lazy `*?`
//! and `+?` character repeats. Captures, optional groups, literal positive lookbehind and named
//! backreferences use the same matcher (dc-ebb.6), as do exact repeats of character atoms and
//! single-character negative lookahead (dc-ebb.7). Alternation, repeated groups and POSIX
//! classes return [`PatternError::Unsupported`]. An unclosed bracketed class
//! does not compile in PCRE2, and the call is `#VALUE!` ([`PatternError::Invalid`]): Excel 16.0.20527 gives
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
    GroupStart {
        capture: Option<usize>,
        skip_to: Option<usize>,
    },
    GroupEnd(usize),
    LookBehind(Vec<char>),
    NotFollowedBy(Box<RegexAtom>),
    Backreference(usize),
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
    Exact(u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegexToken {
    atom: RegexAtom,
    quantifier: RegexQuantifier,
    greedy: bool,
}

impl RegexToken {
    fn once(atom: RegexAtom) -> Self {
        Self {
            atom,
            quantifier: RegexQuantifier::One,
            greedy: true,
        }
    }
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

fn atom_consumes_character(atom: &RegexAtom) -> bool {
    !matches!(
        atom,
        RegexAtom::StartOfText
            | RegexAtom::EndOfText
            | RegexAtom::EndOfLine
            | RegexAtom::WordBoundary
            | RegexAtom::NotWordBoundary
            | RegexAtom::GroupStart { .. }
            | RegexAtom::GroupEnd(_)
            | RegexAtom::LookBehind(_)
            | RegexAtom::NotFollowedBy(_)
            | RegexAtom::Backreference(_)
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
        | RegexAtom::NotWordBoundary
        | RegexAtom::GroupStart { .. }
        | RegexAtom::GroupEnd(_)
        | RegexAtom::LookBehind(_)
        | RegexAtom::NotFollowedBy(_)
        | RegexAtom::Backreference(_) => false,
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

type MatchRange = (usize, usize);

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RegexMatch {
    pub whole: MatchRange,
    pub groups: Vec<Option<MatchRange>>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RegexMatches {
    pub chars: Vec<char>,
    pub found: Vec<RegexMatch>,
    pub group_count: usize,
}

struct RegexMatcher<'a> {
    chars: &'a [char],
    tokens: &'a [RegexToken],
    case_sensitive: bool,
    groups: Vec<Option<MatchRange>>,
    extended: bool,
    remaining_steps: usize,
}

impl RegexMatcher<'_> {
    fn spend(&mut self, steps: usize) -> Result<(), PatternError> {
        self.remaining_steps = self.remaining_steps.checked_sub(steps).ok_or(Unsupported)?;
        Ok(())
    }

    fn matches_at(&self, start: usize, expected: &[char]) -> bool {
        self.chars
            .get(start..start + expected.len())
            .is_some_and(|actual| {
                actual
                    .iter()
                    .zip(expected)
                    .all(|(&a, &b)| chars_equal(a, b, self.case_sensitive))
            })
    }

    fn match_from(&mut self, index: usize, position: usize) -> Result<Option<usize>, PatternError> {
        self.spend(1)?;
        let tokens = self.tokens;
        let Some(token) = tokens.get(index) else {
            return Ok(Some(position));
        };
        let next = index + 1;
        match &token.atom {
            RegexAtom::GroupStart { capture, skip_to } => {
                let previous = capture.map(|i| self.groups[i].replace((position, position)));
                let found = self.match_from(next, position)?;
                if found.is_some() {
                    return Ok(found);
                }
                if let Some((i, previous)) = capture.zip(previous) {
                    self.groups[i] = previous;
                }
                match skip_to {
                    Some(skip_to) => self.match_from(*skip_to, position),
                    None => Ok(None),
                }
            }
            RegexAtom::GroupEnd(i) => {
                let previous = self.groups[*i].expect("the opening capture is active");
                self.groups[*i] = Some((previous.0, position));
                let found = self.match_from(next, position)?;
                if found.is_none() {
                    self.groups[*i] = Some(previous);
                }
                Ok(found)
            }
            RegexAtom::LookBehind(literal) => {
                self.spend(literal.len())?;
                if position
                    .checked_sub(literal.len())
                    .is_some_and(|start| self.matches_at(start, literal))
                {
                    self.match_from(next, position)
                } else {
                    Ok(None)
                }
            }
            RegexAtom::NotFollowedBy(atom) => {
                if self
                    .chars
                    .get(position)
                    .is_some_and(|&ch| consuming_atom_matches(atom, ch, self.case_sensitive))
                {
                    return Ok(None);
                }
                self.match_from(next, position)
            }
            RegexAtom::Backreference(i) => {
                let Some((start, end)) = self.groups[*i] else {
                    return Ok(None);
                };
                self.spend(end - start)?;
                if self.matches_at(position, &self.chars[start..end]) {
                    self.match_from(next, position + end - start)
                } else {
                    Ok(None)
                }
            }
            _ if token.quantifier == RegexQuantifier::One => {
                match match_atom_once(token, self.chars, position, self.case_sensitive) {
                    Some(end) => self.match_from(next, end),
                    None => Ok(None),
                }
            }
            _ => self.match_repeat(token, next, position),
        }
    }

    fn match_repeat(
        &mut self,
        token: &RegexToken,
        next: usize,
        position: usize,
    ) -> Result<Option<usize>, PatternError> {
        let (minimum, limit) = match token.quantifier {
            RegexQuantifier::One => unreachable!("single atoms are matched before repeats"),
            RegexQuantifier::ZeroOrMore => (0, usize::MAX),
            RegexQuantifier::OneOrMore => (1, usize::MAX),
            RegexQuantifier::ZeroOrOne => (0, 1),
            RegexQuantifier::Exact(count) => (usize::from(count), usize::from(count)),
        };
        // Only character atoms can repeat here. Each match advances one character.
        let count = self.chars[position..]
            .iter()
            .take(limit)
            .take_while(|&&ch| consuming_atom_matches(&token.atom, ch, self.case_sensitive))
            .count();
        self.spend(count)?;
        for choice in minimum..=count {
            let length = if token.greedy {
                count - (choice - minimum)
            } else {
                choice
            };
            if let Some(end) = self.match_from(next, position + length)? {
                return Ok(Some(end));
            }
        }
        Ok(None)
    }

    fn find(&mut self, first_only: bool) -> Result<Vec<RegexMatch>, PatternError> {
        let mut found = Vec::new();
        let mut start = 0;
        while start <= self.chars.len() {
            if let Some(end) = self.match_from(0, start)? {
                // Global iteration after a zero-length extended match needs separate observations.
                if self.extended && !first_only && start == end {
                    return Err(Unsupported);
                }
                found.push(RegexMatch {
                    whole: (start, end),
                    groups: self.groups.clone(),
                });
                if first_only {
                    break;
                }
                self.groups.fill(None);
                start = end.max(start + 1);
            } else {
                start += 1;
            }
        }
        Ok(found)
    }
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

fn regex_group_name(chars: &[char], index: &mut usize) -> Result<String, PatternError> {
    let start = *index;
    let first = chars.get(start).ok_or(Unsupported)?;
    if !first.is_ascii_alphabetic() && *first != '_' {
        return Err(Unsupported);
    }
    while chars
        .get(*index)
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
    {
        *index += 1;
    }
    if chars.get(*index) != Some(&'>') {
        return Err(Unsupported);
    }
    let name = chars[start..*index].iter().collect();
    *index += 1;
    Ok(name)
}

fn literal_lookbehind(chars: &[char], index: &mut usize) -> Result<RegexAtom, PatternError> {
    let mut literal = Vec::new();
    while let Some(ch) = chars.get(*index).copied() {
        *index += 1;
        match ch {
            ')' => return Ok(RegexAtom::LookBehind(literal)),
            '.' | '[' | '(' | '|' | '^' | '$' | '*' | '+' | '?' | '{' => return Err(Unsupported),
            '\\' => {
                let escaped = chars.get(*index).and_then(|&ch| escaped_literal_char(ch));
                literal.push(escaped.ok_or(Unsupported)?);
                *index += 1;
            }
            _ => literal.push(ch),
        }
    }
    Err(Unsupported)
}

fn negative_lookahead(chars: &[char], index: &mut usize) -> Result<RegexAtom, PatternError> {
    let ch = *chars.get(*index).ok_or(Unsupported)?;
    *index += 1;
    let atom = match ch {
        '\\' => parse_escape_atom(chars, index)?,
        '[' => parse_char_class(chars, index)?,
        '.' => RegexAtom::Any,
        '(' | ')' | '^' | '$' | '|' | '*' | '+' | '?' | '{' | '}' => return Err(Unsupported),
        literal => RegexAtom::Literal(literal),
    };
    if !atom_consumes_character(&atom) || chars.get(*index) != Some(&')') {
        return Err(Unsupported);
    }
    *index += 1;
    Ok(RegexAtom::NotFollowedBy(Box::new(atom)))
}

/// Leave the cursor on the closing brace, just as a simple quantifier is read at its last character.
fn exact_repeat(chars: &[char], index: &mut usize) -> Result<u16, PatternError> {
    *index += 1;
    let start = *index;
    let mut count = 0u16;
    while let Some(digit) = chars.get(*index).and_then(|ch| ch.to_digit(10)) {
        count = count
            .checked_mul(10)
            .and_then(|n| n.checked_add(digit as u16))
            .ok_or(Unsupported)?;
        *index += 1;
    }
    if *index == start || chars.get(*index) != Some(&'}') {
        return Err(Unsupported);
    }
    Ok(count)
}

struct RegexPattern {
    tokens: Vec<RegexToken>,
    group_count: usize,
    extended: bool,
}

fn parse_regex_pattern(pattern: &str) -> Result<RegexPattern, PatternError> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut tokens = Vec::new();
    let mut names: Vec<Option<String>> = Vec::new();
    let mut open_groups = Vec::new();
    let mut extended = false;
    let mut index = 0usize;
    while let Some(ch) = chars.get(index).copied() {
        index += 1;
        let atom = match ch {
            '.' => RegexAtom::Any,
            '\\' if chars[index..].starts_with(&['k', '<']) => {
                index += 2;
                let name = regex_group_name(&chars, &mut index)?;
                let capture = names
                    .iter()
                    .position(|group| group.as_ref() == Some(&name))
                    .ok_or(Unsupported)?;
                if open_groups
                    .iter()
                    .any(|&(_, current)| current == Some(capture))
                {
                    return Err(Unsupported);
                }
                RegexAtom::Backreference(capture)
            }
            '\\' => parse_escape_atom(&chars, &mut index)?,
            '[' => parse_char_class(&chars, &mut index)?,
            '^' => RegexAtom::StartOfText,
            '$' => RegexAtom::EndOfLine,
            '(' if chars[index..].starts_with(&['?', '<', '=']) => {
                extended = true;
                index += 3;
                literal_lookbehind(&chars, &mut index)?
            }
            '(' if chars[index..].starts_with(&['?', '!']) => {
                extended = true;
                index += 2;
                negative_lookahead(&chars, &mut index)?
            }
            '(' => {
                extended = true;
                let capture = match chars.get(index..index + 2) {
                    Some(['?', ':']) => {
                        index += 2;
                        None
                    }
                    Some(['?', '<']) => {
                        index += 2;
                        let name = regex_group_name(&chars, &mut index)?;
                        if names.iter().any(|group| group.as_ref() == Some(&name)) {
                            return Err(Unsupported);
                        }
                        let capture = names.len();
                        names.push(Some(name));
                        Some(capture)
                    }
                    _ if chars.get(index) == Some(&'?') => return Err(Unsupported),
                    _ => {
                        let capture = names.len();
                        names.push(None);
                        Some(capture)
                    }
                };
                open_groups.push((tokens.len(), capture));
                tokens.push(RegexToken::once(RegexAtom::GroupStart {
                    capture,
                    skip_to: None,
                }));
                continue;
            }
            ')' => {
                let (start, capture) = open_groups.pop().ok_or(Unsupported)?;
                if let Some(capture) = capture {
                    tokens.push(RegexToken::once(RegexAtom::GroupEnd(capture)));
                }
                if chars.get(index) == Some(&'?') {
                    index += 1;
                    tokens[start].atom = RegexAtom::GroupStart {
                        capture,
                        skip_to: Some(tokens.len()),
                    };
                }
                continue;
            }
            '*' | '+' | '?' if index == 1 => return Err(Invalid),
            '*' | '+' | '?' | '{' | '}' | '|' => return Err(Unsupported),
            other => RegexAtom::Literal(other),
        };

        let quantifier = match chars.get(index).copied() {
            Some('*') => RegexQuantifier::ZeroOrMore,
            Some('+') => RegexQuantifier::OneOrMore,
            Some('?') => RegexQuantifier::ZeroOrOne,
            Some('{') => {
                extended = true;
                RegexQuantifier::Exact(exact_repeat(&chars, &mut index)?)
            }
            _ => RegexQuantifier::One,
        };
        let mut greedy = true;
        if quantifier != RegexQuantifier::One {
            if !atom_consumes_character(&atom) {
                return Err(Unsupported);
            }
            index += 1;
            if chars.get(index) == Some(&'?') {
                if matches!(
                    quantifier,
                    RegexQuantifier::ZeroOrOne | RegexQuantifier::Exact(_)
                ) {
                    return Err(Unsupported);
                }
                index += 1;
                greedy = false;
                extended = true;
            }
        }
        tokens.push(RegexToken {
            atom,
            quantifier,
            greedy,
        });
    }
    if !open_groups.is_empty() {
        return Err(Unsupported);
    }
    Ok(RegexPattern {
        tokens,
        group_count: names.len(),
        extended,
    })
}

/// The character positions of whole matches and their numbered capture groups.
pub(crate) fn regex_matches(
    text: &str,
    pattern: &str,
    case_sensitive: bool,
    first_only: bool,
) -> Result<RegexMatches, PatternError> {
    let chars: Vec<char> = text.chars().collect();
    let RegexPattern {
        tokens,
        group_count,
        extended,
    } = parse_regex_pattern(pattern)?;
    // Bound recursive depth and backtracking work for the extended syntax.
    if extended && tokens.len() > 256 {
        return Err(Unsupported);
    }
    // Empty REGEXEXTRACT matches at CRLF, supplementary characters and the leading-LF
    // boundary need an array-shape model. A first match is still at the start.
    // REGEXREPLACE has a separate measured scan for an empty pattern.
    if tokens.is_empty()
        && !first_only
        && (chars.iter().any(|ch| u32::from(*ch) > 0xffff)
            || text.contains("\r\n")
            || (chars.len() == 13 && chars.first() == Some(&'\n')))
    {
        return Err(Unsupported);
    }
    // Before a final newline dollar also matches. Excel's newline convention remains unmeasured.
    if tokens
        .iter()
        .any(|token| token.atom == RegexAtom::EndOfLine)
        && chars.iter().any(|&ch| {
            matches!(
                ch,
                '\n' | '\r' | '\u{000b}' | '\u{000c}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
            )
        })
    {
        return Err(Unsupported);
    }
    // The fn3 readings admit Unicode range closure through the same simple fold as
    // literals. A range beyond the bounded table remains a typed refusal.
    if !case_sensitive
        && tokens.iter().any(|token| {
            let atom = match &token.atom {
                RegexAtom::NotFollowedBy(atom) => atom,
                atom => atom,
            };
            matches!(atom, RegexAtom::Class(class) if class.items.iter().any(|item| {
                matches!(item, RegexClassItem::Range(_, _, None))
            }))
        })
    {
        return Err(Unsupported);
    }
    let found = RegexMatcher {
        chars: &chars,
        tokens: &tokens,
        case_sensitive,
        groups: vec![None; group_count],
        extended,
        remaining_steps: if extended { 1_000_000 } else { usize::MAX },
    }
    .find(first_only)?;
    Ok(RegexMatches {
        chars,
        found,
        group_count,
    })
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

/// TEXTSPLIT's rows of cells: one left-to-right scan of `text`. At each position the column
/// delimiters are tried before the row delimiters (first listed match), so a delimiter in both sets
/// splits columns and a row delimiter inside a longer column delimiter is never reached (Excel
/// 20527, Touchstone 2026-11-26, dc-j9k.548 and .549). With `ignore_empty` an empty cell is dropped
/// and an empty row is dropped; a non-empty row of only delimiters still has one empty cell.
pub(crate) fn split_grid(
    text: &ExcelText,
    columns: Option<&[ExcelText]>,
    rows_by: Option<&[ExcelText]>,
    ignore_empty: bool,
    case_insensitive: bool,
) -> Vec<Vec<ExcelText>> {
    let units = text.utf16_code_units();
    let empty = || ExcelText::from_utf16_code_units(Vec::new());
    let columns = columns.unwrap_or(&[]);
    let rows_by = rows_by.unwrap_or(&[]);
    let mut out: Vec<Vec<ExcelText>> = Vec::new();
    let mut cells: Vec<ExcelText> = Vec::new();
    let mut row_start = 0usize;
    let mut cell_start = 0usize;
    let mut at = 0usize;
    let find = |delimiters: &[ExcelText], at: usize| {
        delimiters.iter().find_map(|delimiter| {
            let needle = delimiter.utf16_code_units();
            (at + needle.len() <= units.len() && units_match(&units[at..at + needle.len()], needle, case_insensitive))
                .then_some(needle.len())
        })
    };
    let end_row = |out: &mut Vec<Vec<ExcelText>>, cells: &mut Vec<ExcelText>, row_start: usize, at: usize| {
        if at > row_start || !ignore_empty {
            if cells.is_empty() {
                cells.push(empty());
            }
            out.push(std::mem::take(cells));
        } else {
            cells.clear();
        }
    };
    while at < units.len() {
        if let Some(length) = find(columns, at) {
            if at > cell_start || !ignore_empty {
                cells.push(ExcelText::from_utf16_code_units(units[cell_start..at].to_vec()));
            }
            at += length;
            cell_start = at;
        } else if let Some(length) = find(rows_by, at) {
            if at > cell_start || !ignore_empty {
                cells.push(ExcelText::from_utf16_code_units(units[cell_start..at].to_vec()));
            }
            end_row(&mut out, &mut cells, row_start, at);
            at += length;
            row_start = at;
            cell_start = at;
        } else {
            at += 1;
        }
    }
    if cell_start < units.len() || !ignore_empty {
        cells.push(ExcelText::from_utf16_code_units(units[cell_start..].to_vec()));
    }
    end_row(&mut out, &mut cells, row_start, units.len());
    if out.is_empty() {
        out.push(vec![empty()]);
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
        let RegexMatches { chars, found, .. } =
            regex_matches("ab12cd345", "[0-9]+", true, false).unwrap();
        let found: Vec<String> = found
            .iter()
            .map(|m| {
                let (start, end) = m.whole;
                chars[start..end].iter().collect()
            })
            .collect();
        assert_eq!(found, ["12", "345"]);
        assert_eq!(regex_matches("abc", "(a|b)", true, true), Err(Unsupported));
        assert_eq!(
            regex_matches("abc", "^a", true, true).unwrap().found[0].whole,
            (0, 1)
        );
        assert_eq!(regex_matches("bab", "^a", true, true).unwrap().found, []);
        // Excel 16.0.20527: REGEXTEST("","^$") is TRUE (the jnt oracle, jnt_text_formats).
        assert_eq!(
            regex_matches("", "^$", true, true).unwrap().found[0].whole,
            (0, 0)
        );
        assert_eq!(
            regex_matches("ab", "b$", true, false).unwrap().found[0].whole,
            (1, 2)
        );
        assert_eq!(regex_matches("ab\n", "b$", true, true), Err(Unsupported));
        // An unclosed class does not compile: #VALUE! (REGEXTEST("hello","[") in Excel).
        assert_eq!(regex_matches("hello", "[", true, true), Err(Invalid));
        assert_eq!(regex_matches("hello", "a[bc", true, true), Err(Invalid));
        assert_eq!(regex_matches("hello", "[^", true, true), Err(Invalid));
        assert_eq!(regex_matches("a]", "[]a]", true, true), Err(Unsupported));
        assert_eq!(
            regex_matches("a", "[[:alpha:]]", true, true),
            Err(Unsupported)
        );
        assert!(regex_matches("ABC", "b", false, true).unwrap().found.len() == 1);
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
            split_grid(&t("a,b,,c"), Some(&[t(",")]), None, true, false),
            vec![vec![t("a"), t("b"), t("c")]]
        );
        assert_eq!(split_grid(&t("a,b,,c"), Some(&[t(",")]), None, false, false)[0].len(), 4);
        assert_eq!(
            numbervalue(&t("2.500,27"), Some(','), Some('.')),
            Ok(2500.27)
        );
        assert_eq!(numbervalue(&t("3.5%"), Some('.'), Some(',')), Ok(0.035));
    }
}
