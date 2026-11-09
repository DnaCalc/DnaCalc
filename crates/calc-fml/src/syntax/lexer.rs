//! The lossless lexer: every character of the input lands in exactly one token, whitespace
//! included, so the tokens reproduce the input; spans are UTF-16 code units.

use super::token::{ERROR_LITERALS, TextSpan, Token, TokenKind};
use calc_func::value::is_combining_mark;

/// Splits `input` into tokens, ending with one [`TokenKind::Eof`] token whose span is empty at
/// the end of the input. Unterminated quoted text runs to the end of the input as one token; the
/// parser reports it.
pub fn lex(input: &str) -> Vec<Token> {
    let chars: Vec<char> = input.chars().collect();
    // offsets[i] is the UTF-16 offset of chars[i]; offsets[chars.len()] is the input's length.
    let mut offsets = Vec::with_capacity(chars.len() + 1);
    let mut offset = 0usize;
    for ch in &chars {
        offsets.push(offset);
        offset += ch.len_utf16();
    }
    offsets.push(offset);
    let mut tokens: Vec<Token> = lex_chars(&chars)
        .into_iter()
        .map(|(kind, start, end)| {
            Token::new(
                kind,
                chars[start..end].iter().collect::<String>(),
                TextSpan::new(offsets[start], offsets[end] - offsets[start]),
            )
        })
        .collect();
    tokens.push(Token::new(
        TokenKind::Eof,
        String::new(),
        TextSpan::new(offsets[chars.len()], 0),
    ));
    tokens
}

/// The tokens of `chars` as [`lex`] makes them, each as its kind and the indexes of its first
/// character and of the character after it, without the closing [`TokenKind::Eof`]: the lexer's
/// one loop, which [`lex`] and a caller that needs no token texts (a formula's template) share.
pub(crate) fn lex_chars(chars: &[char]) -> Vec<(TokenKind, usize, usize)> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        let start = index;
        index += 1;

        let kind = match ch {
            '=' => TokenKind::Equals,
            '<' => match chars.get(index).copied() {
                Some('=') => {
                    index += 1;
                    TokenKind::LessEqual
                }
                Some('>') => {
                    index += 1;
                    TokenKind::NotEqual
                }
                _ => TokenKind::Less,
            },
            '>' => match chars.get(index).copied() {
                Some('=') => {
                    index += 1;
                    TokenKind::GreaterEqual
                }
                _ => TokenKind::Greater,
            },
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            '&' => TokenKind::Ampersand,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '%' => TokenKind::Percent,
            '^' => TokenKind::Caret,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            // A '.' right after ':' is the trim marker of `A1:.B10` (and of `1:.3`), never the
            // start of a number.
            '.' if peek_is_ascii_digit(&chars, index)
                && !(start > 0 && chars[start - 1] == ':') =>
            {
                index = consume_number_literal(&chars, start);
                TokenKind::Number
            }
            '.' => TokenKind::Dot,
            '@' => TokenKind::At,
            '#' => match consume_error_literal(&chars, start) {
                Some(end) => {
                    index = end;
                    TokenKind::ErrorLiteral
                }
                None => TokenKind::Hash,
            },
            '!' => TokenKind::Bang,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            ';' => TokenKind::Semicolon,
            '\'' => {
                index = consume_quoted(&chars, index, '\'');
                TokenKind::QuotedIdentifier
            }
            '"' => {
                index = consume_quoted(&chars, index, '"');
                TokenKind::StringLiteral
            }
            '[' => {
                index = start;
                consume_balanced_bracket_group(&chars, &mut index);
                while index < chars.len() && is_identifier_continue(chars[index]) {
                    index += 1;
                }
                TokenKind::BracketedQualifier
            }
            c if c.is_ascii_whitespace() => {
                while index < chars.len() && chars[index].is_ascii_whitespace() {
                    index += 1;
                }
                TokenKind::Whitespace
            }
            c if c.is_ascii_digit() => {
                index = consume_number_literal(&chars, start);
                TokenKind::Number
            }
            c if is_identifier_start(c) => {
                while index < chars.len() {
                    // A '.' followed by '@', '*' or '[' ends the identifier, so the parser sees
                    // the selector form `name.@x` as one unit and refuses it precisely. A '.'
                    // followed by ':' ends it too: `A1.:B10` is a trim reference, not the name
                    // `A1.` in a range.
                    if chars[index] == '.'
                        && matches!(chars.get(index + 1), Some('@' | '*' | '[' | ':'))
                    {
                        break;
                    }
                    if is_identifier_continue(chars[index]) {
                        index += 1;
                        continue;
                    }
                    if chars[index] == '[' {
                        consume_balanced_bracket_group(&chars, &mut index);
                        continue;
                    }
                    break;
                }
                TokenKind::Identifier
            }
            _ => TokenKind::Unknown,
        };

        tokens.push((kind, start, index));
    }
    tokens
}

/// Consumes quoted text whose opening quote precedes `index`: a doubled quote is an escaped
/// quote, and the first single quote closes the text. Returns the index just past the closing
/// quote, or the end of the input when the text is unterminated.
fn consume_quoted(chars: &[char], mut index: usize, quote: char) -> usize {
    while index < chars.len() {
        if chars[index] == quote {
            if chars.get(index + 1) == Some(&quote) {
                index += 2;
                continue;
            }
            return index + 1;
        }
        index += 1;
    }
    index
}

/// Whether `text` (a string literal or quoted name, opening quote included) ends with its
/// closing quote.
pub(crate) fn quoted_text_is_closed(text: &str, quote: char) -> bool {
    let chars: Vec<char> = text.chars().collect();
    if chars.first() != Some(&quote) {
        return false;
    }
    let mut index = 1;
    while index < chars.len() {
        if chars[index] == quote {
            if chars.get(index + 1) == Some(&quote) {
                index += 2;
                continue;
            }
            return index + 1 == chars.len();
        }
        index += 1;
    }
    false
}

fn consume_error_literal(chars: &[char], start: usize) -> Option<usize> {
    for (spelling, _) in &ERROR_LITERALS {
        let end = start + spelling.chars().count();
        if end > chars.len() {
            continue;
        }
        let candidate = chars[start..end].iter().collect::<String>();
        if candidate.eq_ignore_ascii_case(spelling) {
            return Some(end);
        }
    }
    None
}

fn peek_is_ascii_digit(chars: &[char], index: usize) -> bool {
    chars.get(index).is_some_and(|ch| ch.is_ascii_digit())
}

fn consume_number_literal(chars: &[char], mut index: usize) -> usize {
    let mut saw_digits = false;

    while index < chars.len() && chars[index].is_ascii_digit() {
        index += 1;
        saw_digits = true;
    }

    // A '.' followed by ':' is the trim marker of `1.:3`, not a decimal point.
    if index < chars.len() && chars[index] == '.' && chars.get(index + 1) != Some(&':') {
        index += 1;
        while index < chars.len() && chars[index].is_ascii_digit() {
            index += 1;
            saw_digits = true;
        }
    }

    if saw_digits && index < chars.len() && matches!(chars[index], 'E' | 'e') {
        let exponent_start = index;
        let mut probe = index + 1;
        if probe < chars.len() && matches!(chars[probe], '+' | '-') {
            probe += 1;
        }
        if peek_is_ascii_digit(chars, probe) {
            index = probe + 1;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
            }
        } else {
            index = exponent_start;
        }
    }

    index
}

/// The first character of an unquoted name: any Unicode letter, `_` or `\` (Excel's name syntax),
/// and `$` for absolute A1 references (`$A$1`). Whether the token is a cell reference or a name
/// is decided when it is bound, never here.
fn is_identifier_start(ch: char) -> bool {
    ch.is_alphabetic() || matches!(ch, '_' | '$' | '\\')
}

/// The later characters of an unquoted name: letters, digits, combining marks (so a name typed
/// in decomposed form stays one token), `.`, `_` and `$`. The identifier loop in [`lex`] ends a
/// name before a `.` that starts a selector or a trim reference.
fn is_identifier_continue(ch: char) -> bool {
    ch.is_alphanumeric() || is_combining_mark(ch) || matches!(ch, '_' | '.' | '$')
}

fn consume_balanced_bracket_group(chars: &[char], index: &mut usize) {
    if *index >= chars.len() || chars[*index] != '[' {
        return;
    }

    let mut depth = 0usize;
    while *index < chars.len() {
        if chars[*index] == '\'' && is_bracket_escape(chars, *index) {
            *index += 2;
            continue;
        }
        match chars[*index] {
            '[' => {
                depth += 1;
            }
            ']' => {
                depth = depth.saturating_sub(1);
                *index += 1;
                if depth == 0 {
                    break;
                }
                continue;
            }
            _ => {}
        }
        *index += 1;
    }
}

/// Inside brackets, `'` escapes the next `#`, `[`, `]`, `@` or `'`.
fn is_bracket_escape(chars: &[char], index: usize) -> bool {
    matches!(chars.get(index + 1), Some('#' | '[' | ']' | '@' | '\''))
}
