//! The text of one member of a shared formula group: the group's text, as its master cell holds
//! it, with every relative A1 reference moved by the member's offset from the master, the way
//! copying the master's formula to the member moves it.
//!
//! The text is split by [`calc_fml::syntax::lex`], the formula language's one lexer, and each
//! token is read the way CalcFml's parser and A1 reference binding read it:
//!
//! - An identifier that names a function (a `(` follows it directly), qualifies a reference (a `!`
//!   follows it directly) or is the first or last sheet of a sheet span (`S1:S3!A1`) is a name.
//! - Any other identifier or number that spells a cell of the grid (`B7`, `$B7`, `b$7`) is a cell
//!   reference, qualified or not.
//! - A column (`B`, `$B`) or a row (`7`, `$7`) is a reference only as an end of a range whose
//!   other end, qualified or not and with or without trim markers (`.:`, `:.`), is of the same
//!   form (`B:D`, `$1:3`, `Sheet1!A:Sheet1!C`).
//!
//! Each reference's relative parts are moved and its `$`-absolute parts keep their text, as does a
//! relative part moved by 0; a moved part is written in upper case. Every other token (string
//! literals, quoted and bracketed names, structured references, error literals, names, numbers,
//! operators and whitespace) keeps its text byte for byte.
//!
//! Text this cannot move is refused ([`Unclassified`]), not copied: a cell written with a `$`
//! outside the grid, a range of rows or columns with an end outside the grid, a row or column
//! after a chain of ranges (`A:B:C`), a token with a `$` that is neither a cell nor a range end,
//! an unterminated string literal or quoted name, an unclosed `[` group (a bracketed workbook
//! qualifier or a structured reference), and a character the lexer does not read. A move that
//! takes a reference off the grid is refused too ([`PastGrid`]).

use super::address::{column_letters, column_number, row_number};
use calc_doc_model::value::{MAX_COLUMNS, MAX_ROWS};
use calc_fml::syntax::{Token, TokenKind, lex};

/// A moved reference would lie outside the grid.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct PastGrid;

/// The group's text holds a reference form this does not move; the text says which.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Unclassified(pub(super) &'static str);

/// Whether quoted text (opening quote included) ends with its closing quote. The lexer ends the
/// token at the first quote that is not doubled, so a closed token holds an even number of quotes
/// (the opening, doubled pairs and the closing) and an unterminated one an odd number.
fn closed(text: &str, quote: char) -> bool {
    text.len() >= 2
        && text.ends_with(quote)
        && text.chars().filter(|&c| c == quote).count() % 2 == 0
}

/// Whether every `[` group in a token's text closes, read by the lexer's rules: `[` opens a
/// level, `]` closes one, and inside a group `'` escapes the `#`, `[`, `]`, `@` or `'` after it.
/// The lexer runs an unclosed group to the end of the input, so the token then holds the rest of
/// the formula.
fn brackets_closed(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let mut depth = 0usize;
    let mut at = 0;
    while at < chars.len() {
        match chars[at] {
            '\'' if depth > 0
                && matches!(chars.get(at + 1), Some('#' | '[' | ']' | '@' | '\'')) =>
            {
                at += 2;
                continue;
            }
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
        at += 1;
    }
    depth == 0
}

/// One coordinate of a reference: its 1-based number, whether a `$` makes it absolute, and its
/// text.
#[derive(Debug)]
struct Axis {
    value: u32,
    absolute: bool,
    text: String,
}

impl Axis {
    /// The coordinate's text moved by `by` when it is relative, or its own text when it is
    /// absolute or not moved.
    fn moved(&self, by: i64, limit: u32, render: fn(u32) -> String) -> Result<String, PastGrid> {
        if self.absolute || by == 0 {
            return Ok(self.text.clone());
        }
        let value = i64::from(self.value) + by;
        if !(1..=i64::from(limit)).contains(&value) {
            return Err(PastGrid);
        }
        Ok(render(value as u32))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    Cell,
    Column,
    Row,
}

/// What a token's text spells in A1 notation.
#[derive(Debug)]
enum Spelled {
    Cell {
        column: Axis,
        row: Axis,
    },
    Column(Axis),
    Row(Axis),
    /// Text of the form that lies outside the grid: a cell or a column written with a `$`, or a
    /// row. (A cell or column without a `$` outside the grid, such as `XFE1`, is a name.)
    OutsideGrid(Form),
}

impl Spelled {
    fn form(&self) -> Form {
        match self {
            Spelled::Cell { .. } => Form::Cell,
            Spelled::Column(_) => Form::Column,
            Spelled::Row(_) => Form::Row,
            Spelled::OutsideGrid(form) => *form,
        }
    }
}

/// `text` read as `[$]letters[$]digits` (a cell), `[$]letters` (a column) or `[$]digits` (a
/// row), letters ASCII in either case; `None` for text of no such form.
fn spell(text: &str) -> Option<Spelled> {
    let (first_dollar, rest) = match text.strip_prefix('$') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let letters_end = rest
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(rest.len());
    let (letters, rest) = rest.split_at(letters_end);
    let (second_dollar, digits) = match rest.strip_prefix('$') {
        Some(digits) if !letters.is_empty() => (true, digits),
        _ => (false, rest),
    };
    if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let column_text = &text[..text.len() - rest.len()];
    let row_text = &text[text.len() - digits.len() - usize::from(second_dollar)..];
    let column = || {
        column_number(letters).map(|value| Axis {
            value,
            absolute: first_dollar,
            text: column_text.to_owned(),
        })
    };
    match (letters.is_empty(), digits.is_empty()) {
        (false, false) => match (column(), row_number(digits)) {
            (Some(column), Some(value)) => Some(Spelled::Cell {
                column,
                row: Axis {
                    value,
                    absolute: second_dollar,
                    text: row_text.to_owned(),
                },
            }),
            _ => (first_dollar || second_dollar).then_some(Spelled::OutsideGrid(Form::Cell)),
        },
        (false, true) if !second_dollar => match column() {
            Some(column) => Some(Spelled::Column(column)),
            None => first_dollar.then_some(Spelled::OutsideGrid(Form::Column)),
        },
        (true, false) => Some(match row_number(digits) {
            Some(value) => Spelled::Row(Axis {
                value,
                absolute: first_dollar,
                text: text.to_owned(),
            }),
            None => Spelled::OutsideGrid(Form::Row),
        }),
        _ => None,
    }
}

fn row_text(row: u32) -> String {
    row.to_string()
}

/// A piece of the group's text: kept as written, or one coordinate of a reference.
#[derive(Debug)]
enum Piece {
    Kept(String),
    Column(Axis),
    Row(Axis),
}

/// A shared formula group's text, split into the pieces a member keeps and the coordinates it
/// moves.
#[derive(Debug)]
pub(super) struct Moves {
    pieces: Vec<Piece>,
}

impl Moves {
    /// Reads `text`, refused when it holds a reference form this does not move.
    pub(super) fn new(text: &str) -> Result<Moves, Unclassified> {
        let tokens = lex(text);
        // Text the lexer could not read whole is refused, not copied: a member's text would be
        // the master's unmoved.
        for token in &tokens {
            match token.kind() {
                TokenKind::Unknown => {
                    return Err(Unclassified("a character the formula lexer does not read"));
                }
                TokenKind::StringLiteral if !closed(token.text(), '"') => {
                    return Err(Unclassified("an unterminated string literal"));
                }
                TokenKind::QuotedIdentifier if !closed(token.text(), '\'') => {
                    return Err(Unclassified("an unterminated quoted name"));
                }
                TokenKind::BracketedQualifier | TokenKind::Identifier
                    if !brackets_closed(token.text()) =>
                {
                    return Err(Unclassified("an unclosed [ group"));
                }
                _ => {}
            }
        }
        let kind = |at: usize| tokens.get(at).map(Token::kind);
        let significant = |at: usize| tokens[at].kind() != TokenKind::Whitespace;
        let next = |at: usize| (at + 1..tokens.len()).find(|&next| significant(next));
        let previous = |at: usize| (0..at).rev().find(|&previous| significant(previous));
        let is_deleted_sheet = |at: usize| {
            kind(at) == Some(TokenKind::ErrorLiteral)
                && tokens[at].text().eq_ignore_ascii_case("#REF!")
        };
        // The operand after a qualifier's `!` (or after `#REF!`), which never starts a sheet span.
        let qualifier_target = |at: usize| {
            previous(at).is_some_and(|before| kind(before) == Some(TokenKind::Bang))
                || (at > 0
                    && is_deleted_sheet(at - 1)
                    && matches!(kind(at), Some(TokenKind::Identifier | TokenKind::Number)))
        };

        // Names by position: sheet spans first (as the parser takes them), then qualifiers and
        // function names. `span_bang[head]` is the `!` that ends the span `head` starts.
        let mut name = vec![false; tokens.len()];
        let mut span_bang = vec![None; tokens.len()];
        let mut span_colon = vec![false; tokens.len()];
        for at in 0..tokens.len() {
            if name[at]
                || !matches!(
                    kind(at),
                    Some(TokenKind::Identifier | TokenKind::QuotedIdentifier)
                )
                || qualifier_target(at)
            {
                continue;
            }
            let colon = next(at);
            let last = colon.and_then(next);
            let bang = last.and_then(next);
            if let (Some(colon), Some(last), Some(bang)) = (colon, last, bang)
                && kind(colon) == Some(TokenKind::Colon)
                && kind(last) == Some(TokenKind::Identifier)
                && kind(bang) == Some(TokenKind::Bang)
            {
                name[at] = true;
                name[last] = true;
                span_colon[colon] = true;
                span_bang[at] = Some(bang);
            }
        }
        for (at, named) in name.iter_mut().enumerate() {
            if kind(at) == Some(TokenKind::Identifier)
                && matches!(kind(at + 1), Some(TokenKind::Bang | TokenKind::LParen))
            {
                *named = true;
            }
        }
        // An identifier or number that can be a reference: not a name by position, and not a
        // structured reference (`Table1[Column]`), whose brackets the lexer keeps in the token.
        let candidate = |at: usize| {
            matches!(kind(at), Some(TokenKind::Identifier | TokenKind::Number))
                && !name[at]
                && !tokens[at].text().contains('[')
        };

        // The ends of each range of rows or of columns. A range operator is left-associative, so
        // in a chain (`A:B:C`) a colon whose left operand is the range before it pairs nothing.
        let mut range_end = vec![false; tokens.len()];
        let mut right_of_range = vec![false; tokens.len()];
        for (colon, token) in tokens.iter().enumerate() {
            if token.kind() != TokenKind::Colon || span_colon[colon] {
                continue;
            }
            let before = if colon > 0 && kind(colon - 1) == Some(TokenKind::Dot) {
                colon - 1
            } else {
                colon
            };
            let after = if kind(colon + 1) == Some(TokenKind::Dot) {
                colon + 1
            } else {
                colon
            };
            let Some(left) = previous(before) else {
                continue;
            };
            let Some(start) = next(after) else {
                continue;
            };
            // The right end's own token, past a qualifier or a sheet span.
            let right = if let Some(bang) = span_bang[start] {
                next(bang)
            } else if matches!(
                kind(start),
                Some(
                    TokenKind::Identifier
                        | TokenKind::QuotedIdentifier
                        | TokenKind::BracketedQualifier
                )
            ) && kind(start + 1) == Some(TokenKind::Bang)
            {
                next(start + 1)
            } else if is_deleted_sheet(start)
                && matches!(
                    kind(start + 1),
                    Some(TokenKind::Identifier | TokenKind::Number)
                )
            {
                Some(start + 1)
            } else {
                Some(start)
            };
            let Some(right) = right else {
                continue;
            };
            let chained = right_of_range[left];
            right_of_range[right] = true;
            if chained {
                // `A:B:C` or `1:2:3`: what the last row or column is, Excel's own reading of
                // the chain has not been measured, so it is refused rather than guessed.
                if candidate(right)
                    && spell(tokens[right].text()).is_some_and(|end| end.form() != Form::Cell)
                {
                    return Err(Unclassified("a row or column after a chain of ranges"));
                }
                continue;
            }
            if !candidate(left) || !candidate(right) {
                continue;
            }
            let (Some(a), Some(b)) = (spell(tokens[left].text()), spell(tokens[right].text()))
            else {
                continue;
            };
            match (a, b) {
                (Spelled::Column(_), Spelled::Column(_)) | (Spelled::Row(_), Spelled::Row(_)) => {
                    range_end[left] = true;
                    range_end[right] = true;
                }
                (a, b) if a.form() == b.form() && a.form() != Form::Cell => {
                    return Err(Unclassified(
                        "a range of rows or columns with an end outside the grid",
                    ));
                }
                _ => {}
            }
        }

        fn keep(pieces: &mut Vec<Piece>, text: &str) {
            match pieces.last_mut() {
                Some(Piece::Kept(kept)) => kept.push_str(text),
                _ => pieces.push(Piece::Kept(text.to_owned())),
            }
        }
        let mut pieces: Vec<Piece> = Vec::new();
        for (at, token) in tokens.iter().enumerate() {
            let text = token.text();
            if !candidate(at) {
                keep(&mut pieces, text);
                continue;
            }
            match spell(text) {
                Some(Spelled::Cell { column, row }) => {
                    pieces.push(Piece::Column(column));
                    pieces.push(Piece::Row(row));
                }
                Some(Spelled::Column(column)) if range_end[at] => {
                    pieces.push(Piece::Column(column))
                }
                Some(Spelled::Row(row)) if range_end[at] => pieces.push(Piece::Row(row)),
                Some(Spelled::OutsideGrid(Form::Cell)) => {
                    return Err(Unclassified("a cell reference outside the grid"));
                }
                _ if text.contains('$') => {
                    return Err(Unclassified(
                        "a token with a $ that is neither a cell nor an end of a range of rows or columns",
                    ));
                }
                _ => keep(&mut pieces, text),
            }
        }
        Ok(Moves { pieces })
    }

    /// The text moved by `rows` and `columns`.
    pub(super) fn apply(&self, rows: i64, columns: i64) -> Result<String, PastGrid> {
        let mut out = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Kept(text) => out.push_str(text),
                Piece::Column(column) => {
                    out.push_str(&column.moved(columns, MAX_COLUMNS, column_letters)?)
                }
                Piece::Row(row) => out.push_str(&row.moved(rows, MAX_ROWS, row_text)?),
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum Refused {
        Unclassified(&'static str),
        PastGrid,
    }

    fn shift(text: &str, rows: i64, columns: i64) -> Result<String, Refused> {
        let moves =
            Moves::new(text).map_err(|Unclassified(detail)| Refused::Unclassified(detail))?;
        moves
            .apply(rows, columns)
            .map_err(|PastGrid| Refused::PastGrid)
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn moves_relative_references_only() {
        let cases = [
            // A range whose right end is a function call or a qualified cell.
            ("SUM(A1:INDEX(B:B,2))", 1, 0, "SUM(A2:INDEX(B:B,2))"),
            ("SUM(A1:OFFSET(B2,1,0))", 1, 0, "SUM(A2:OFFSET(B3,1,0))"),
            ("SUM(Sheet1!A1:Sheet1!B1)", 1, 0, "SUM(Sheet1!A2:Sheet1!B2)"),
            (
                "SUM(A1:_xlfn.XLOOKUP(1,A:A,B:B))",
                1,
                1,
                "SUM(B2:_xlfn.XLOOKUP(1,B:B,C:C))",
            ),
            ("SUM(A1:INDEX(B:B,2))", 0, 1, "SUM(B1:INDEX(C:C,2))"),
            ("$A1+A$1", 1, 1, "$A2+B$1"),
            ("\"A1\"&A1", 1, 0, "\"A1\"&A2"),
            ("A1*2+1", 1, 0, "A2*2+1"),
            ("$A$1+A$1+$A1+A1", 2, 3, "$A$1+D$1+$A3+D3"),
            ("SUM(A1:B2)", 1, 1, "SUM(B2:C3)"),
            ("SUM($A1:B$2)", 1, 1, "SUM($A2:C$2)"),
            ("SUM(A:B)+SUM($C:D)", 0, 2, "SUM(C:D)+SUM($C:F)"),
            ("SUM(1:2)+SUM($3:4)", 5, 0, "SUM(6:7)+SUM($3:9)"),
            (
                "Sheet1!A:Sheet1!B+Sheet1!1:2",
                1,
                1,
                "Sheet1!B:Sheet1!C+Sheet1!2:3",
            ),
            ("Sheet1!A1+'My A1'!B2", 1, 0, "Sheet1!A2+'My A1'!B3"),
            ("Sheet1:Sheet3!A1", 1, 0, "Sheet1:Sheet3!A2"),
            ("S1:S3!A1", 1, 0, "S1:S3!A2"),
            ("SUM(A1:S1:S3!B2)", 1, 1, "SUM(B2:S1:S3!C3)"),
            ("A1:B2:C3", 1, 1, "B2:C3:D4"),
            ("\"say \"\"A1\"\"\"&A1", 1, 0, "\"say \"\"A1\"\"\"&A2"),
            ("LOG10(A1)+LOG10", 1, 0, "LOG10(A2)+LOG11"),
            (
                "_xlfn.XLOOKUP(A1,B:B,C:C)",
                1,
                0,
                "_xlfn.XLOOKUP(A2,B:B,C:C)",
            ),
            (
                "Table1[[#This Row],[A1]]+A1",
                1,
                0,
                "Table1[[#This Row],[A1]]+A2",
            ),
            ("Table1[Price$]+A1", 1, 0, "Table1[Price$]+A2"),
            ("Table1['[Col']]+A1", 1, 0, "Table1['[Col']]+A2"),
            ("[1]Sheet1!A1", 1, 0, "[1]Sheet1!A2"),
            ("#REF!A1+#REF!", 1, 0, "#REF!A2+#REF!"),
            (
                "IF(A1,#REF!,#N/A)+#DIV/0!",
                1,
                0,
                "IF(A2,#REF!,#N/A)+#DIV/0!",
            ),
            ("A1#", 1, 0, "A2#"),
            ("@A1:A3", 1, 0, "@A2:A4"),
            ("A1:A3 B2:C2", 1, 0, "A2:A4 B3:C3"),
            ("SUM(A1.:B2)+SUM(A.:.C)", 1, 1, "SUM(B2.:C3)+SUM(B.:.D)"),
            ("1.5E+3+A1+1:2", 1, 0, "1.5E+3+A2+2:3"),
            ("Name1+R1C1+XFE1+B+3", 1, 1, "Name1+R1C1+XFE1+B+3"),
            ("a1+b$2", 1, 1, "B2+C$2"),
            ("a1", 1, 0, "a2"),
            ("{1,2;3,4}", 1, 1, "{1,2;3,4}"),
            ("SUM( A1 , B2 )", 1, 0, "SUM( A2 , B3 )"),
        ];
        for (text, rows, columns, expected) in cases {
            assert_eq!(
                shift(text, rows, columns),
                Ok(expected.to_owned()),
                "{text} moved by ({rows}, {columns})"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn refuses_what_it_cannot_move() {
        assert_eq!(shift("A1048576", 1, 0), Err(Refused::PastGrid));
        assert_eq!(shift("XFD1", 0, 1), Err(Refused::PastGrid));
        assert_eq!(shift("$A1048576+A$1", 1, 0), Err(Refused::PastGrid));
        assert_eq!(shift("A1", -1, 0), Err(Refused::PastGrid));
        assert_eq!(shift("SUM(1:1048576)", 1, 0), Err(Refused::PastGrid));
        assert_eq!(shift("SUM(XFD:XFD)", 0, 1), Err(Refused::PastGrid));
        assert_eq!(
            shift("A$1048576+$XFD1", 1, 1),
            Ok("B$1048576+$XFD2".to_owned())
        );
        for (text, detail) in [
            ("$XFE1+A1", "a cell reference outside the grid"),
            ("A$1048577", "a cell reference outside the grid"),
            (
                "SUM(1:1048577)",
                "a range of rows or columns with an end outside the grid",
            ),
            (
                "SUM(A:$XFE)",
                "a range of rows or columns with an end outside the grid",
            ),
            (
                "$A+1",
                "a token with a $ that is neither a cell nor an end of a range of rows or columns",
            ),
            (
                "SUM($3)",
                "a token with a $ that is neither a cell nor an end of a range of rows or columns",
            ),
            (
                "A$+1",
                "a token with a $ that is neither a cell nor an end of a range of rows or columns",
            ),
            ("SUM(A:B:C)", "a row or column after a chain of ranges"),
            ("SUM(1:2:3)", "a row or column after a chain of ranges"),
            ("A1&\"abc", "an unterminated string literal"),
            ("A1&\"a\"\"", "an unterminated string literal"),
            ("A1+'Sheet 2", "an unterminated quoted name"),
            ("A1+~1", "a character the formula lexer does not read"),
            ("A1+[1Sheet1!B1", "an unclosed [ group"),
            ("A1+Table1[Col+B1", "an unclosed [ group"),
            ("A1+Table1[[#This Row],[Col]+B1", "an unclosed [ group"),
            ("A1+Table1[Col']+B1", "an unclosed [ group"),
        ] {
            assert_eq!(
                shift(text, 1, 0),
                Err(Refused::Unclassified(detail)),
                "{text}"
            );
        }
    }
}
