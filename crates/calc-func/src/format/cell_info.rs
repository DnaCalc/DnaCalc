//! What CELL("format"), CELL("color") and CELL("parentheses") report of a number format code
//! (dc-uar). Excel classifies the code's first section, not the value: `G` (General, text,
//! fractions and codes it does not class), `F<n>` (fixed), `C<n>` (currency), `P<n>` (percent),
//! `S<n>` (scientific) with n the decimal places, `D1` to `D9` (dates and times); `-` follows
//! when the second section has a color, and `()` when the first section shows a parenthesis.
//!
//! Measured in Excel 16.0.20527 on 2026-10-29 (en-ZA: currency symbol `R`, a space as the group
//! separator): the catalogue oracle's dcuar_cell rows, a cell holding 1.5 under each of 219 codes
//! the file states (custom codes and built-in IDs). A code set through COM's `NumberFormat`
//! classes differently (`#,##0` is then `F0` and `R 0` `C0`); only the file's code matters here,
//! since DNA Calc reads files. The rules that fit every reading:
//!
//! - **Dates and times.** Weekdays (`ddd`), eras (`e`, `b`, `g`) and elapsed times (`[h]`) are
//!   `G`. A code whose first date or time token is an hour is a time: with minutes and seconds
//!   `D8` (`D6` with `AM/PM` or `A/P`), with minutes alone `D9` (`D7` with `AM/PM` or `A/P`),
//!   otherwise `G`. A code that starts with minutes or seconds is `G`. Otherwise the order in
//!   which day, month and year first appear decides: d m y `D1`, d m `D2`, m y `D3`, m d y `D4`,
//!   m d `D5`, any other `G` (`yyyy-mm-dd` and `mmm` are `G`). An `m` after an hour or before a
//!   second is minutes. A locale tag with a locale ID (`[$-409]`, `[$-1C09]`) shifts a date's
//!   class down by two (`[$-409]m/d/yy` is `D2`, `[$-409]d-mmm-yy` `G`) and makes a time `G`.
//!   The built-in dates of ID 14 and 22 are the locale's short date (`yyyy/mm/dd` under en-ZA,
//!   so `G`), the others are their codes.
//! - **Numbers**, in this order: `General` is `G`; a quoted literal that is the profile's currency
//!   symbol alone is `C` (`"R"0`, `0.00 "R"`; an unquoted `R` is a literal: `R 0` is `F0`); a
//!   `%` not escaped is `P`; an exponent is `S`; a fraction is `G`; a group separator after a
//!   digit placeholder gives `,` (`#,##0.00` is `,2`, `$#,##0` and `#,##0,` `,0`); digit
//!   placeholders are `F`; anything else is `G`. The decimal places are the placeholders after
//!   the decimal point (before an exponent), at most 15.
//! - `-` follows when the second section has a color, `()` when the first shows a parenthesis
//!   (escaped or not; not quoted, not `_(` padding).

use crate::locale::FormatProfile;

/// What CELL reports of a number format code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FormatClass {
    /// CELL("format").
    pub(crate) code: String,
    /// CELL("color"): the second section has a color.
    pub(crate) color: bool,
    /// CELL("parentheses"): the first section shows a parenthesis.
    pub(crate) parentheses: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    /// A literal character written as it stands, escaped (`\x`) or not.
    Literal(char),
    /// A quoted literal.
    Quoted(String),
    /// `_x` or `*x`.
    Spacing,
    Color,
    Condition,
    Elapsed,
    /// `[$...]`: with a locale ID (`[$-409]`) or not (`[$R]`).
    Locale { with_id: bool },
    General,
    Placeholder,
    Point,
    Group,
    Percent,
    Exponent,
    Slash,
    At,
    Day(usize),
    Month(usize),
    Year,
    Hour,
    Second,
    AmPm,
    AmPmShort,
    Era,
}

/// The code's sections, split at `;` outside quotes, brackets and escapes.
fn sections(code: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut start, mut quoted, mut bracket, mut escaped) = (0, false, false, false);
    for (i, c) in code.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if !quoted => escaped = true,
            '"' if !bracket => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            ';' if !quoted && !bracket => {
                out.push(&code[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&code[start..]);
    out
}

fn tokens(section: &str) -> Vec<Token> {
    let chars: Vec<char> = section.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let starts = |i: usize, word: &str| -> bool {
        let w: Vec<char> = word.chars().collect();
        i + w.len() <= chars.len()
            && chars[i..i + w.len()]
                .iter()
                .zip(&w)
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
    };
    while i < chars.len() {
        let c = chars[i];
        let run = |i: usize, lower: char| {
            chars[i..]
                .iter()
                .take_while(|x| x.to_ascii_lowercase() == lower)
                .count()
        };
        match c {
            '"' => {
                let end = chars[i + 1..]
                    .iter()
                    .position(|&x| x == '"')
                    .map_or(chars.len(), |p| i + 1 + p);
                out.push(Token::Quoted(chars[i + 1..end].iter().collect()));
                i = end + 1;
            }
            '\\' => {
                if let Some(&x) = chars.get(i + 1) {
                    out.push(Token::Literal(x));
                }
                i += 2;
            }
            '_' | '*' => {
                out.push(Token::Spacing);
                i += 2;
            }
            '[' => {
                let end = chars[i + 1..]
                    .iter()
                    .position(|&x| x == ']')
                    .map_or(chars.len(), |p| i + 1 + p);
                let inner: String = chars[i + 1..end].iter().collect();
                let lower = inner.to_ascii_lowercase();
                out.push(if let Some(rest) = inner.strip_prefix('$') {
                    Token::Locale {
                        with_id: rest.contains('-'),
                    }
                } else if inner.starts_with(['<', '>', '=']) {
                    Token::Condition
                } else if !lower.is_empty()
                    && (lower.chars().all(|x| x == 'h')
                        || lower.chars().all(|x| x == 'm')
                        || lower.chars().all(|x| x == 's'))
                {
                    Token::Elapsed
                } else {
                    Token::Color
                });
                i = end + 1;
            }
            _ if starts(i, "general") => {
                out.push(Token::General);
                i += 7;
            }
            _ if starts(i, "am/pm") => {
                out.push(Token::AmPm);
                i += 5;
            }
            _ if starts(i, "a/p") => {
                out.push(Token::AmPmShort);
                i += 3;
            }
            '0' | '#' | '?' => {
                out.push(Token::Placeholder);
                i += 1;
            }
            '.' => {
                out.push(Token::Point);
                i += 1;
            }
            ',' => {
                out.push(Token::Group);
                i += 1;
            }
            '%' => {
                out.push(Token::Percent);
                i += 1;
            }
            '/' => {
                out.push(Token::Slash);
                i += 1;
            }
            '@' => {
                out.push(Token::At);
                i += 1;
            }
            'E' | 'e' if matches!(chars.get(i + 1), Some('+' | '-')) => {
                out.push(Token::Exponent);
                i += 2;
            }
            _ => match c.to_ascii_lowercase() {
                'd' => {
                    let n = run(i, 'd');
                    out.push(Token::Day(n));
                    i += n;
                }
                'm' => {
                    let n = run(i, 'm');
                    out.push(Token::Month(n));
                    i += n;
                }
                'y' => {
                    i += run(i, 'y');
                    out.push(Token::Year);
                }
                'h' => {
                    i += run(i, 'h');
                    out.push(Token::Hour);
                }
                's' => {
                    i += run(i, 's');
                    out.push(Token::Second);
                }
                'e' | 'g' | 'b' => {
                    i += run(i, c.to_ascii_lowercase());
                    out.push(Token::Era);
                }
                _ => {
                    out.push(Token::Literal(c));
                    i += 1;
                }
            },
        }
    }
    out
}

fn is_date_time(token: &Token) -> bool {
    matches!(
        token,
        Token::Day(_)
            | Token::Month(_)
            | Token::Year
            | Token::Hour
            | Token::Second
            | Token::AmPm
            | Token::AmPmShort
            | Token::Era
            | Token::Elapsed
    )
}

/// The class of a date or time section: `None` for `G`.
fn date_time_class(tokens: &[Token]) -> Option<u8> {
    if tokens.iter().any(|t| {
        matches!(t, Token::Era | Token::Elapsed) || matches!(t, Token::Day(n) if *n >= 3)
    }) {
        return None;
    }
    let tagged = tokens
        .iter()
        .any(|t| matches!(t, Token::Locale { with_id: true }));
    // The date and time tokens in order, each month token resolved to month (`M`) or minute.
    let parts: Vec<&Token> = tokens
        .iter()
        .filter(|t| is_date_time(t) && !matches!(t, Token::AmPm | Token::AmPmShort))
        .collect();
    let minute = |at: usize| {
        matches!(parts[at], Token::Month(_))
            && ((at > 0 && matches!(parts[at - 1], Token::Hour))
                || matches!(parts.get(at + 1), Some(Token::Second)))
    };
    match parts.first()? {
        Token::Hour => {
            if tagged {
                return None;
            }
            let has_minute = (0..parts.len()).any(minute);
            if !has_minute {
                return None;
            }
            let seconds = parts.iter().any(|t| matches!(t, Token::Second));
            let am_pm = tokens.iter().any(|t| matches!(t, Token::AmPm | Token::AmPmShort));
            Some(match (seconds, am_pm) {
                (true, true) => 6,
                (true, false) => 8,
                (false, true) => 7,
                (false, false) => 9,
            })
        }
        Token::Second => None,
        Token::Month(_) if minute(0) => None,
        _ => {
            let mut order = Vec::new();
            for (at, part) in parts.iter().enumerate() {
                let key = match part {
                    Token::Day(_) => 'd',
                    Token::Month(_) if !minute(at) => 'm',
                    Token::Year => 'y',
                    _ => continue,
                };
                if !order.contains(&key) {
                    order.push(key);
                }
            }
            let class: u8 = match order.as_slice() {
                ['d', 'm', 'y'] => 1,
                ['d', 'm'] => 2,
                ['m', 'y'] => 3,
                ['m', 'd', 'y'] => 4,
                ['m', 'd'] => 5,
                _ => return None,
            };
            if tagged {
                class.checked_sub(2).filter(|c| *c > 0)
            } else {
                Some(class)
            }
        }
    }
}

/// The decimal places a number section shows: the placeholders after its decimal point, before
/// an exponent, at most 15.
fn places(tokens: &[Token]) -> usize {
    let Some(point) = tokens.iter().position(|t| *t == Token::Point) else {
        return 0;
    };
    tokens[point + 1..]
        .iter()
        .take_while(|t| **t != Token::Exponent)
        .filter(|t| **t == Token::Placeholder)
        .count()
        .min(15)
}

fn number_class(tokens: &[Token], profile: &FormatProfile) -> String {
    if tokens.iter().any(|t| *t == Token::General) {
        return "G".to_owned();
    }
    let n = places(tokens);
    let symbol = profile.currency_symbol;
    let currency = !symbol.is_empty()
        && tokens
            .iter()
            .any(|t| matches!(t, Token::Quoted(q) if q == symbol));
    if currency {
        return format!("C{n}");
    }
    if tokens.iter().any(|t| *t == Token::Percent) {
        return format!("P{n}");
    }
    if tokens.iter().any(|t| *t == Token::Exponent) {
        return format!("S{n}");
    }
    let slash = tokens.iter().position(|t| *t == Token::Slash);
    if slash.is_some_and(|at| {
        tokens[..at].iter().any(|t| *t == Token::Placeholder)
            || tokens[at + 1..].iter().any(|t| *t == Token::Placeholder)
    }) {
        return "G".to_owned();
    }
    if tokens.iter().any(|t| *t == Token::Placeholder) {
        let grouped = tokens.windows(2).any(|w| w == [Token::Placeholder, Token::Group]);
        return if grouped { format!(",{n}") } else { format!("F{n}") };
    }
    "G".to_owned()
}

/// What CELL reports of the number format `code` under `profile`; `builtin` is the built-in
/// format ID the cell's format names, when it names one.
pub(crate) fn classify(code: &str, builtin: Option<u32>, profile: &FormatProfile) -> FormatClass {
    let short_date;
    let code = match builtin {
        Some(14) => profile.short_date_pattern,
        Some(22) => {
            short_date = format!("{} h:mm", profile.short_date_pattern);
            short_date.as_str()
        }
        _ => code,
    };
    let parts = sections(code);
    let first = tokens(parts[0]);
    let mut class = if first.iter().any(is_date_time) {
        date_time_class(&first).map_or_else(|| "G".to_owned(), |c| format!("D{c}"))
    } else {
        number_class(&first, profile)
    };
    let color = parts
        .get(1)
        .is_some_and(|second| tokens(second).iter().any(|t| *t == Token::Color));
    let parentheses = first.iter().any(|t| *t == Token::Literal('('));
    if color {
        class.push('-');
    }
    if parentheses {
        class.push_str("()");
    }
    FormatClass {
        code: class,
        color,
        parentheses,
    }
}

#[cfg(test)]
mod tests {
    use super::classify;
    use crate::locale::{LocaleId, format_profile};

    /// A sample of the Excel readings (en-ZA, Excel 16.0.20527, 2026-10-29); the catalogue
    /// oracle's dcuar_cell rows hold all of them.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn classes_match_excel_readings() {
        let za = format_profile(LocaleId::EnZa);
        for (code, want) in [
            ("General", "G"),
            ("0", "F0"),
            ("#,##0.00", ",2"),
            ("$#,##0_);[Red]($#,##0)", ",0-"),
            ("R #,##0.00", ",2"),
            ("R 0", "F0"),
            ("\"R\"0", "C0"),
            ("0.00 \"R\"", "C2"),
            ("\"R \"", "G"),
            ("[$R-1C09]#,##0.00", ",2"),
            ("0%", "P0"),
            ("0 %", "P0"),
            ("R0%", "P0"),
            (r"0.00\%", "F2"),
            ("0.00E+00", "S2"),
            ("# ?/?", "G"),
            ("(0)", "F0()"),
            (r"\(0\)", "F0()"),
            ("\"(\"0\")\"", "F0"),
            ("_(0.0_)", "F1"),
            (";[Red]0", "G-"),
            ("0.0000000000000000000000000000000", "F15"),
            ("m/d/yy", "D4"),
            ("d-mmm-yy", "D1"),
            ("d-mmm", "D2"),
            ("mmm-yy", "D3"),
            ("mm/dd", "D5"),
            ("yyyy-mm-dd", "G"),
            ("dddd d mmmm yyyy", "G"),
            ("h:mm AM/PM", "D7"),
            ("h:mm:ss AM/PM", "D6"),
            ("h:mm", "D9"),
            ("h:mm:ss", "D8"),
            ("h:mm A/P", "D7"),
            ("h:mm d/m/yy", "D9"),
            ("mm:ss", "G"),
            ("[h]:mm:ss", "G"),
            ("[$-409]mmmm d, yyyy", "D2"),
            ("[$-409]mmm-yy", "D1"),
            ("[$-409]d-mmm-yy", "G"),
            ("[$-409]h:mm", "G"),
            ("m/d/yy;[Red]m/d/yy", "D4-"),
        ] {
            assert_eq!(classify(code, None, &za).code, want, "{code}");
        }
        assert_eq!(classify("m/d/yyyy", Some(14), &za).code, "G");
    }
}
