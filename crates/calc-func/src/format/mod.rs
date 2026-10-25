//! The number-format engine that TEXT, FIXED and DOLLAR render with, ported from OxFml
//! (crates/oxfml_core/src/format/ at 15daac5: engine.rs, number.rs, general.rs, datetime.rs and
//! locale_tables.rs) and adapted to what Excel 16.0.20430 gave on this machine (Windows locale
//! en-ZA with `.` as the decimal separator and a space as the group separator), read ad hoc
//! through COM for about 70 TEXT, FIXED and DOLLAR calls:
//!
//! - A format code reads the locale's own separators: the decimal separator is the decimal
//!   point, and the group separator groups digits between digit placeholders and divides by 1000
//!   when it follows the last one. Any other character, a `,` under en-ZA included, is a literal
//!   written where it stands (`TEXT(1234567.891,"#,##0")` is `1234,568`).
//! - Digits are those of the number rounded first to 15 significant digits and then, a half away
//!   from zero, to the places the code shows; a negative number whose rounding is zero shows no
//!   minus sign.
//! - Sections: one for every number; two for the non-negative and the negative numbers (the
//!   second without a minus sign); three adding zero; a fourth for text; a condition in brackets
//!   chooses its section. Colors and locale tags in brackets are dropped; `[$R-1C09]` writes `R`.
//! - `General` writes at most 11 characters besides the sign, switching to `d.ddddE+xx` for a
//!   number too large or too small for that.
//! - Dates and times (y, m, d, h, s, AM/PM, elapsed [h], [m], [s]) through [`datetime`]; a negative
//!   number under a date code is #VALUE!.
//!
//! A fraction alone (`?/?`, `??/16`) renders as [`FractionCode`] says. A fraction with an integer
//! part (`# ?/?`, which Excel gave #VALUE! for under en-ZA), a literal inside the decimal places,
//! and letters that are format tokens elsewhere (`B`, `e`, `g`, `A/P`) are not modelled and refuse
//! the call.

mod datetime;
mod names;

use crate::decimal::{Direction, ExactDecimal};
use crate::locale::{
    CurrencyNegativePattern, CurrencyPlacement, CurrencySpacing, DateSystem, FormatProfile,
    LocaleId,
};

/// Why a value has no text under a code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// Excel gives #VALUE!.
    Value,
    /// Excel's text is not modelled.
    NotModelled,
}

type Rendered = Result<String, Failure>;

/// The AM or PM designator of a locale.
pub(crate) fn designator(profile: &FormatProfile, am: bool) -> &'static str {
    let lower = matches!(
        profile.id,
        LocaleId::EnZa | LocaleId::CurrentExcelHost
    );
    match (am, lower) {
        (true, true) => "am",
        (false, true) => "pm",
        (true, false) => "AM",
        (false, false) => "PM",
    }
}

// --- digits -------------------------------------------------------------------------------------

/// The magnitude of `x` rounded to 15 significant digits and then to `places` decimal places
/// (negative places round to tens, hundreds and so on), a half away from zero, as its integer
/// digits (no leading zero; empty for zero) and exactly `max(places, 0)` decimal digits.
pub(crate) fn digits(x: f64, places: i32) -> (String, String) {
    let first = fifteen(x.abs());
    let rounded = first.round_at(-places, Direction::Nearest).unwrap_or(first);
    split(&rounded, places.max(0) as usize)
}

/// A finite magnitude rounded to 15 significant digits, a half away from zero.
fn fifteen(x: f64) -> ExactDecimal {
    let exact = ExactDecimal::read(x);
    let length = exact.coefficient.digits().len() as i32;
    if exact.coefficient.is_zero() || length <= 15 {
        return exact;
    }
    exact
        .round_at(exact.exponent + length - 15, Direction::Nearest)
        .unwrap_or(exact)
}

/// A decimal's integer digits (without leading zeros) and `places` decimal digits.
fn split(decimal: &ExactDecimal, places: usize) -> (String, String) {
    if decimal.coefficient.is_zero() {
        return (String::new(), "0".repeat(places));
    }
    let digits = decimal.coefficient.digits();
    let exponent = decimal.exponent;
    let (whole, fraction) = if exponent >= 0 {
        (format!("{digits}{}", "0".repeat(exponent as usize)), String::new())
    } else {
        let point = digits.len() as i64 + exponent as i64;
        if point <= 0 {
            (String::new(), format!("{}{digits}", "0".repeat((-point) as usize)))
        } else {
            let point = point as usize;
            (digits[..point].to_owned(), digits[point..].to_owned())
        }
    };
    let whole = whole.trim_start_matches('0').to_owned();
    let mut fraction = fraction;
    fraction.truncate(places);
    while fraction.len() < places {
        fraction.push('0');
    }
    (whole, fraction)
}

fn grouped(whole: &str, separator: &str) -> String {
    if whole.len() <= 3 {
        return whole.to_owned();
    }
    let mut out = String::new();
    let first = whole.len() % 3;
    if first > 0 {
        out.push_str(&whole[..first]);
    }
    let mut at = first;
    while at < whole.len() {
        if !out.is_empty() {
            out.push_str(separator);
        }
        out.push_str(&whole[at..at + 3]);
        at += 3;
    }
    out
}

// --- FIXED and DOLLAR -----------------------------------------------------------------------------

/// FIXED: the number rounded to `places`, grouped unless `no_commas`.
pub(crate) fn fixed(profile: &FormatProfile, value: f64, places: i32, no_commas: bool) -> String {
    let (whole, fraction) = digits(value, places);
    let negative = value < 0.0 && (whole.chars().chain(fraction.chars()).any(|c| c != '0'));
    let whole = if whole.is_empty() { "0".to_owned() } else { whole };
    let whole = if no_commas {
        whole
    } else {
        grouped(&whole, profile.thousands_separator)
    };
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(&whole);
    if places > 0 {
        out.push_str(profile.decimal_separator);
        out.push_str(&fraction);
    }
    out
}

/// DOLLAR: the number in the locale's currency format, rounded to `places`.
pub(crate) fn dollar(profile: &FormatProfile, value: f64, places: i32) -> String {
    let magnitude = fixed(profile, value.abs(), places, false);
    let spacing = match profile.currency_spacing {
        CurrencySpacing::None => "",
        CurrencySpacing::Space => " ",
        CurrencySpacing::NarrowNoBreakSpace => "\u{202F}",
    };
    let symbol = profile.currency_symbol;
    let body = match profile.currency_placement {
        CurrencyPlacement::Before => format!("{symbol}{spacing}{magnitude}"),
        CurrencyPlacement::After => format!("{magnitude}{spacing}{symbol}"),
    };
    let negative = value < 0.0 && magnitude.chars().any(|c| c.is_ascii_digit() && c != '0');
    if !negative {
        return body;
    }
    match profile.currency_negative_pattern {
        CurrencyNegativePattern::LeadingMinus | CurrencyNegativePattern::MinusBeforeSymbol => {
            format!("-{body}")
        }
        CurrencyNegativePattern::TrailingMinus => format!("{body}-"),
        CurrencyNegativePattern::Parentheses => format!("({body})"),
    }
}

// --- General --------------------------------------------------------------------------------------

/// The General format's text: at most 11 characters besides the sign, as a cell of Excel's
/// standard column width shows a number. Public as [`crate::general_format`] for an app that
/// shows a value as a cell would.
pub fn general(profile: &FormatProfile, value: f64) -> String {
    const WIDTH: i64 = 11;
    if value == 0.0 || value.is_subnormal() {
        return "0".to_owned();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let magnitude = value.abs();
    let first = fifteen(magnitude);
    let length = first.coefficient.digits().len() as i64;
    // The value is 0.d1d2... * 10^point.
    let point = length + first.exponent as i64;
    let fixed_fits = point <= WIDTH && point > -9;
    if fixed_fits {
        let places = if point >= WIDTH {
            0
        } else {
            WIDTH - point.max(1) - 1
        };
        let (whole, fraction) = digits(magnitude, places as i32);
        let fraction = fraction.trim_end_matches('0');
        let whole = if whole.is_empty() { "0".to_owned() } else { whole };
        if whole.len() as i64 <= WIDTH && (whole != "0" || !fraction.is_empty()) {
            let mut out = format!("{sign}{whole}");
            if !fraction.is_empty() {
                out.push_str(profile.decimal_separator);
                out.push_str(fraction);
            }
            return out;
        }
    }
    // Scientific: d.dddddE+xx, the mantissa's places filling what the exponent leaves.
    let mut exponent = point - 1;
    let exponent_width = if exponent.abs() >= 100 { 3 } else { 2 };
    let places = WIDTH - 2 - 2 - exponent_width;
    let (mantissa, carry) = significant(&first, places as usize + 1);
    exponent += carry;
    let (head, tail) = mantissa.split_at(1);
    let tail = tail.trim_end_matches('0');
    let mut out = format!("{sign}{head}");
    if !tail.is_empty() {
        out.push_str(profile.decimal_separator);
        out.push_str(tail);
    }
    out.push_str(&format!(
        "E{}{:02}",
        if exponent < 0 { '-' } else { '+' },
        exponent.abs()
    ));
    out
}

/// The first `count` significant digits of a non-zero decimal, rounded a half away from zero, and
/// 1 when the rounding carried into a new leading digit (the digits are then 1 and zeros).
fn significant(decimal: &ExactDecimal, count: usize) -> (String, i64) {
    let digits = decimal.coefficient.digits();
    if digits.len() <= count {
        return (format!("{digits}{}", "0".repeat(count - digits.len())), 0);
    }
    let dropped = (digits.len() - count) as i32;
    let rounded = decimal
        .round_at(decimal.exponent + dropped, Direction::Nearest)
        .expect("digits below the position");
    let out = rounded.coefficient.digits();
    if out.len() > count {
        (out[..count].to_owned(), 1)
    } else {
        (out, 0)
    }
}

// --- format codes ---------------------------------------------------------------------------------

/// The sections of a code, split at `;` outside quotes and escapes.
fn sections(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let (mut quoted, mut escaped) = (false, false);
    for ch in code.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => {
                current.push(ch);
                escaped = true;
            }
            '"' => {
                current.push(ch);
                quoted = !quoted;
            }
            ';' if !quoted => out.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    out.push(current);
    out
}

/// A bracketed token at the start of `text`: its inside and the rest.
fn bracket(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix('[')?;
    let close = rest.find(']')?;
    Some((&rest[..close], &rest[close + 1..]))
}

fn is_condition(token: &str) -> bool {
    token.starts_with(['<', '>', '='])
}

fn is_elapsed(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    !lower.is_empty()
        && lower.chars().all(|c| matches!(c, 'h' | 'm' | 's'))
        && lower.chars().all(|c| Some(c) == lower.chars().next())
}

fn condition_holds(token: &str, value: f64) -> Option<bool> {
    let (op, rest) = ["<=", ">=", "<>", "<", ">", "="]
        .iter()
        .find_map(|op| token.strip_prefix(op).map(|rest| (*op, rest)))?;
    let threshold: f64 = rest.trim().parse().ok()?;
    Some(match op {
        "<" => value < threshold,
        "<=" => value <= threshold,
        ">" => value > threshold,
        ">=" => value >= threshold,
        "=" => value == threshold,
        _ => value != threshold,
    })
}

/// The leading condition of a section, if it has one.
fn condition_of(section: &str) -> Option<String> {
    let mut rest = section;
    while let Some((token, after)) = bracket(rest) {
        if is_condition(token) {
            return Some(token.to_owned());
        }
        rest = after;
    }
    None
}

/// A section with its bracketed tokens resolved: colors and locale tags dropped, a currency tag
/// `[$sym-xxxx]` written as its symbol (quoted), conditions dropped, elapsed-time tokens kept.
fn strip_tokens(section: &str) -> Result<String, Failure> {
    let mut out = String::new();
    let mut chars = section.chars().peekable();
    let (mut quoted, mut escaped) = (false, false);
    while let Some(ch) = chars.next() {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => {
                out.push(ch);
                escaped = true;
            }
            '"' => {
                out.push(ch);
                quoted = !quoted;
            }
            '[' if !quoted => {
                let mut token = String::new();
                for next in chars.by_ref() {
                    if next == ']' {
                        break;
                    }
                    token.push(next);
                }
                if is_elapsed(&token) {
                    out.push('[');
                    out.push_str(&token);
                    out.push(']');
                } else if let Some(body) = token.strip_prefix('$') {
                    let symbol = body.split('-').next().unwrap_or("");
                    if !symbol.is_empty() {
                        out.push('"');
                        out.push_str(symbol);
                        out.push('"');
                    }
                } else if is_condition(&token)
                    || token.chars().all(|c| c.is_ascii_alphanumeric())
                {
                    // A color ([Red], [Color10]) or a condition.
                } else {
                    return Err(Failure::NotModelled);
                }
            }
            _ => out.push(ch),
        }
    }
    Ok(out)
}

/// The section a number is rendered with, and whether that section shows the number's sign
/// itself (so its magnitude is rendered).
fn choose(sections: &[String], value: f64) -> Result<(String, bool), Failure> {
    let numeric: Vec<&String> = sections.iter().take(3).collect();
    if numeric.iter().any(|s| condition_of(s).is_some()) {
        let mut fallback = None;
        for section in &numeric {
            match condition_of(section) {
                Some(token) => {
                    if condition_holds(&token, value).ok_or(Failure::NotModelled)? {
                        return Ok(((*section).clone(), false));
                    }
                }
                None => {
                    if fallback.is_none() {
                        fallback = Some((*section).clone());
                    }
                }
            }
        }
        return fallback.map(|s| (s, false)).ok_or(Failure::NotModelled);
    }
    Ok(match numeric.len() {
        1 => (numeric[0].clone(), false),
        2 => {
            if value < 0.0 {
                (numeric[1].clone(), true)
            } else {
                (numeric[0].clone(), false)
            }
        }
        _ => {
            if value > 0.0 {
                (numeric[0].clone(), false)
            } else if value < 0.0 {
                (numeric[1].clone(), true)
            } else {
                (numeric[2].clone(), false)
            }
        }
    })
}

/// One element of a numeric section.
#[derive(Clone, Debug, PartialEq)]
enum Item {
    /// `0`, `#` or `?`.
    Digit(char),
    /// The locale's group separator.
    Group,
    /// The locale's decimal separator.
    Point,
    /// `E+`, `E-`, `e+` or `e-`.
    Exponent(bool),
    /// `%`.
    Percent,
    Literal(String),
}

/// A section's items, with quoted text, escapes, `_x` (a space) and `*x` (nothing) resolved.
fn items(section: &str, profile: &FormatProfile) -> Result<Vec<Item>, Failure> {
    let decimal = profile.decimal_separator;
    let group = profile.thousands_separator;
    let chars: Vec<char> = section.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    let literal = |out: &mut Vec<Item>, text: &str| match out.last_mut() {
        Some(Item::Literal(existing)) => existing.push_str(text),
        _ => out.push(Item::Literal(text.to_owned())),
    };
    while at < chars.len() {
        let ch = chars[at];
        let rest: String = chars[at..].iter().collect();
        match ch {
            '"' => {
                let close = chars[at + 1..]
                    .iter()
                    .position(|c| *c == '"')
                    .ok_or(Failure::NotModelled)?;
                let text: String = chars[at + 1..at + 1 + close].iter().collect();
                literal(&mut out, &text);
                at += close + 2;
            }
            '\\' => {
                let next = chars.get(at + 1).ok_or(Failure::NotModelled)?;
                literal(&mut out, &next.to_string());
                at += 2;
            }
            '_' => {
                literal(&mut out, " ");
                at += 2;
            }
            '*' => at += 2,
            '0' | '#' | '?' => {
                out.push(Item::Digit(ch));
                at += 1;
            }
            '%' => {
                out.push(Item::Percent);
                at += 1;
            }
            'E' | 'e' if matches!(chars.get(at + 1), Some('+' | '-')) => {
                out.push(Item::Exponent(chars[at + 1] == '+'));
                at += 2;
            }
            _ if rest.starts_with(decimal) => {
                out.push(Item::Point);
                at += decimal.chars().count();
            }
            _ if !group.is_empty() && rest.starts_with(group) => {
                out.push(Item::Group);
                at += group.chars().count();
            }
            '/' => return Err(Failure::NotModelled),
            c if c.is_ascii_alphabetic() => {
                // A letter that is a token in some other format (dates, eras, A/P) is not modelled.
                if "ymdhsbegaYMDHSBEGA".contains(c) {
                    return Err(Failure::NotModelled);
                }
                literal(&mut out, &c.to_string());
                at += 1;
            }
            c => {
                literal(&mut out, &c.to_string());
                at += 1;
            }
        }
    }
    Ok(out)
}

/// Renders a number with a numeric section's items.
fn numeric(
    profile: &FormatProfile,
    value: f64,
    items: &[Item],
    show_minus: bool,
) -> Rendered {
    let first = items
        .iter()
        .position(|i| matches!(i, Item::Digit(_)))
        .ok_or(Failure::NotModelled)?;
    let last = items
        .iter()
        .rposition(|i| matches!(i, Item::Digit(_)))
        .expect("a digit placeholder");
    let exponent_at = items.iter().position(|i| matches!(i, Item::Exponent(_)));
    let mantissa_end = exponent_at.unwrap_or(last + 1);
    let point_at = items[..mantissa_end]
        .iter()
        .position(|i| *i == Item::Point);
    // Group separators right after the last mantissa placeholder divide by 1000 each.
    let mut scale_groups = 0;
    let mut cursor = items[..mantissa_end]
        .iter()
        .rposition(|i| matches!(i, Item::Digit(_)))
        .map(|p| p + 1)
        .unwrap_or(mantissa_end);
    while items.get(cursor) == Some(&Item::Group) {
        scale_groups += 1;
        cursor += 1;
    }
    let scale_end = cursor;
    let percents = items.iter().filter(|i| **i == Item::Percent).count() as i32;
    let mantissa_last = items[..mantissa_end]
        .iter()
        .rposition(|i| matches!(i, Item::Digit(_)))
        .unwrap_or(first);
    let integer_end = point_at.unwrap_or(mantissa_last + 1);
    let integer: Vec<&Item> = items[first..integer_end.max(first)].iter().collect();
    let fraction: Vec<char> = match point_at {
        Some(p) => items[p + 1..mantissa_end]
            .iter()
            .filter_map(|i| match i {
                Item::Digit(c) => Some(Ok(*c)),
                Item::Group => None,
                _ => Some(Err(Failure::NotModelled)),
            })
            .collect::<Result<Vec<char>, Failure>>()?,
        None => Vec::new(),
    };
    let grouping = integer
        .iter()
        .enumerate()
        .any(|(at, i)| **i == Item::Group && integer[at + 1..].iter().any(|j| matches!(j, Item::Digit(_))));
    let prefix: String = items[..first]
        .iter()
        .map(|i| literal_text(i, profile))
        .collect::<Result<String, Failure>>()?;
    let suffix_from = match exponent_at {
        Some(_) => last + 1,
        None => scale_end.max(last + 1),
    };
    let suffix: String = items[suffix_from..]
        .iter()
        .map(|i| literal_text(i, profile))
        .collect::<Result<String, Failure>>()?;

    let mut scaled = value.abs() * 100f64.powi(percents);
    if scale_groups > 0 {
        scaled /= 1000f64.powi(scale_groups);
    }
    let places = fraction.len() as i32;

    let (body, nonzero) = match exponent_at {
        Some(e) => scientific(profile, scaled, &integer, &fraction, items, e)?,
        None => {
            let (whole, frac) = digits(scaled, places);
            let nonzero = whole.chars().chain(frac.chars()).any(|c| c != '0');
            let whole_text = integer_part(profile, &whole, &integer, grouping)?;
            let frac_text = fraction_part(&frac, &fraction);
            let mut body = whole_text;
            if point_at.is_some() {
                body.push_str(profile.decimal_separator);
                body.push_str(&frac_text);
            }
            (body, nonzero)
        }
    };
    let minus = if show_minus && value < 0.0 && nonzero {
        "-"
    } else {
        ""
    };
    Ok(format!("{minus}{prefix}{body}{suffix}"))
}

fn literal_text(item: &Item, profile: &FormatProfile) -> Result<String, Failure> {
    Ok(match item {
        Item::Literal(text) => text.clone(),
        Item::Percent => "%".to_owned(),
        Item::Group => profile.thousands_separator.to_owned(),
        Item::Point => profile.decimal_separator.to_owned(),
        Item::Digit(_) | Item::Exponent(_) => return Err(Failure::NotModelled),
    })
}

/// The integer part: digits fill the placeholders from the right, the leftmost placeholder takes
/// any digits left over, `0` writes a zero and `?` a space where no digit is left, literals stay
/// where they stand, and grouping writes the locale's group separator between groups of three.
fn integer_part(
    profile: &FormatProfile,
    whole: &str,
    pattern: &[&Item],
    grouping: bool,
) -> Result<String, Failure> {
    let placeholders: Vec<char> = pattern
        .iter()
        .filter_map(|i| match i {
            Item::Digit(c) => Some(*c),
            _ => None,
        })
        .collect();
    if grouping {
        // Literals between placeholders are not modelled together with grouping.
        if pattern
            .iter()
            .any(|i| !matches!(i, Item::Digit(_) | Item::Group))
        {
            return Err(Failure::NotModelled);
        }
        let zeros = placeholders
            .iter()
            .rev()
            .take_while(|_| true)
            .enumerate()
            .filter(|(_, c)| **c == '0')
            .map(|(at, _)| at + 1)
            .max()
            .unwrap_or(0);
        let mut digits = whole.to_owned();
        while digits.len() < zeros {
            digits.insert(0, '0');
        }
        return Ok(grouped(&digits, profile.thousands_separator));
    }
    let chars: Vec<char> = whole.chars().collect();
    let mut next = chars.len();
    let mut out: Vec<String> = Vec::new();
    let leftmost = pattern
        .iter()
        .position(|i| matches!(i, Item::Digit(_)))
        .unwrap_or(0);
    for (at, item) in pattern.iter().enumerate().rev() {
        match item {
            Item::Digit(c) => {
                let mut piece = String::new();
                if next > 0 {
                    next -= 1;
                    piece.push(chars[next]);
                    if at == leftmost && next > 0 {
                        let rest: String = chars[..next].iter().collect();
                        piece.insert_str(0, &rest);
                        next = 0;
                    }
                } else if *c == '0' {
                    piece.push('0');
                } else if *c == '?' {
                    piece.push(' ');
                }
                out.push(piece);
            }
            other => out.push(literal_text(other, profile)?),
        }
    }
    out.reverse();
    Ok(out.concat())
}

/// The decimal places: `0` writes its digit, `#` drops a trailing zero, `?` writes a trailing
/// zero as a space.
fn fraction_part(digits: &str, pattern: &[char]) -> String {
    let chars: Vec<char> = digits.chars().collect();
    let mut out: Vec<char> = chars.clone();
    for at in (0..chars.len()).rev() {
        if chars[at] != '0' {
            break;
        }
        match pattern[at] {
            '#' => {
                out.pop();
            }
            '?' => out[at] = ' ',
            _ => break,
        }
    }
    out.into_iter().collect()
}

/// A scientific mantissa and exponent.
fn scientific(
    profile: &FormatProfile,
    value: f64,
    integer: &[&Item],
    fraction: &[char],
    items: &[Item],
    exponent_at: usize,
) -> Result<(String, bool), Failure> {
    let Item::Exponent(plus) = items[exponent_at] else {
        unreachable!()
    };
    let exponent_digits: Vec<char> = items[exponent_at + 1..]
        .iter()
        .take_while(|i| matches!(i, Item::Digit(_)))
        .map(|i| match i {
            Item::Digit(c) => *c,
            _ => unreachable!(),
        })
        .collect();
    if exponent_digits.is_empty() {
        return Err(Failure::NotModelled);
    }
    let whole_places: Vec<char> = integer
        .iter()
        .filter_map(|i| match i {
            Item::Digit(c) => Some(*c),
            Item::Group => None,
            _ => Some('!'),
        })
        .collect();
    if whole_places.contains(&'!') || whole_places.is_empty() {
        return Err(Failure::NotModelled);
    }
    let width = whole_places.len() as i64;
    let engineering = width > 1 && whole_places[0] == '#';
    if value == 0.0 {
        let mantissa = format!(
            "0{}{}",
            if fraction.is_empty() { "" } else { profile.decimal_separator },
            "0".repeat(fraction.len())
        );
        let exponent = format!("E{}{}", if plus { "+" } else { "" }, "0".repeat(exponent_digits.len()));
        return Ok((format!("{mantissa}{exponent}"), false));
    }
    let first = fifteen(value);
    let length = first.coefficient.digits().len() as i64;
    let point = length + first.exponent as i64; // value = 0.d * 10^point
    let leading = if engineering {
        // The exponent is a multiple of the width.
        let e = (point - 1).div_euclid(width) * width;
        point - e
    } else {
        width.min(point.max(1)).max(1)
    };
    let mut exponent = point - leading;
    let count = (leading + fraction.len() as i64) as usize;
    let (mut mantissa_digits, carry) = significant(&first, count);
    if carry == 1 {
        exponent += 1;
        if engineering && exponent.rem_euclid(width) != 0 {
            // A carry past the width moves to the next multiple.
            mantissa_digits.push('0');
        }
    }
    let lead = leading as usize;
    let (whole, frac) = mantissa_digits.split_at(lead.min(mantissa_digits.len()));
    let frac = fraction_part(&frac[..fraction.len().min(frac.len())], fraction);
    let mut out = whole.to_owned();
    if !fraction.is_empty() {
        out.push_str(profile.decimal_separator);
        out.push_str(&frac);
    }
    let sign = if exponent < 0 {
        "-"
    } else if plus {
        "+"
    } else {
        ""
    };
    out.push_str(&format!(
        "E{sign}{:0width$}",
        exponent.abs(),
        width = exponent_digits.len()
    ));
    Ok((out, true))
}

/// A section that is a fraction alone, with no integer part: numerator placeholders (`0`, `#`,
/// `?`), `/`, and denominator placeholders or a fixed denominator (dc-uar). Excel 16.0.20430
/// (the catalogue oracle's sheet dcuar_text):
///
/// - with placeholders, the fraction is the last convergent of the number's continued fraction,
///   computed in binary64, whose denominator has at most as many digits as there are
///   placeholders (`TEXT(0.7,"?/?")` is `2/3`, `TEXT(0.1,"?/?")` is `0/1` since 1/0.1 is 10);
/// - with a fixed denominator d, the numerator is |x|*d rounded a half up, and a numerator above
///   32767 is #VALUE!;
/// - a numerator placeholder pads on the left (`0` with 0, `?` with a space, `#` with nothing); a
///   denominator of `0`s pads with zeros on the left, one of `?` and `#` is left-aligned and
///   `?` pads with a space on the right;
/// - a minus sign comes first, and not for a numerator of 0.
///
/// A fraction with an integer part (`# ?/?`) is not this form; under en-ZA the space in it is
/// the group separator, and Excel gives those #VALUE! (not modelled here).
struct FractionCode {
    numerator: Vec<char>,
    denominator: Result<u64, Vec<char>>,
}

fn fraction_code(section: &str) -> Option<FractionCode> {
    let (top, bottom) = section.split_once('/')?;
    let placeholders = |s: &str| !s.is_empty() && s.chars().all(|c| matches!(c, '0' | '#' | '?'));
    if !placeholders(top) {
        return None;
    }
    let denominator = if placeholders(bottom) {
        let chars: Vec<char> = bottom.chars().collect();
        if chars.contains(&'0') && chars.iter().any(|c| *c != '0') {
            return None;
        }
        Err(chars)
    } else if !bottom.is_empty()
        && bottom.len() <= 9
        && bottom.chars().all(|c| c.is_ascii_digit())
        && !bottom.starts_with('0')
    {
        Ok(bottom.parse().ok()?)
    } else {
        return None;
    };
    let numerator: Vec<char> = top.chars().collect();
    if numerator.contains(&'0') && numerator.iter().any(|c| *c != '0') {
        return None;
    }
    Some(FractionCode {
        numerator,
        denominator,
    })
}

impl FractionCode {
    fn render(&self, value: f64, show_minus: bool) -> Rendered {
        let x = value.abs();
        let (numerator, denominator) = match &self.denominator {
            Ok(d) => {
                let n = (x * *d as f64 + 0.5).floor();
                if !(n <= 32767.0) {
                    return Err(Failure::Value);
                }
                (n as u64, *d)
            }
            Err(places) => {
                let largest = 10u64.pow(places.len() as u32) - 1;
                convergent(x, largest).ok_or(Failure::NotModelled)?
            }
        };
        let mut out = String::new();
        if show_minus && value < 0.0 && numerator != 0 {
            out.push('-');
        }
        let top = numerator.to_string();
        let missing = self.numerator.len().saturating_sub(top.len());
        for c in &self.numerator[..missing] {
            match c {
                '0' => out.push('0'),
                '?' => out.push(' '),
                _ => {}
            }
        }
        out.push_str(&top);
        out.push('/');
        let bottom = denominator.to_string();
        match &self.denominator {
            Ok(_) => out.push_str(&bottom),
            Err(places) => {
                let missing = places.len().saturating_sub(bottom.len());
                if places.first() == Some(&'0') {
                    out.push_str(&"0".repeat(missing));
                    out.push_str(&bottom);
                } else {
                    out.push_str(&bottom);
                    for c in &places[places.len() - missing..] {
                        if *c == '?' {
                            out.push(' ');
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}

/// Whether `section` is a fraction with an integer part set off by the group separator, under a
/// locale whose group separator is a space: Excel 16.0.20430 under en-ZA gave #VALUE! for every
/// such code it was given (`# ?/?`, `# ??/??`, `# ???/???`, `# ?/8`, `# ?/100`, `0 ?/?`,
/// `#,##0 ?/?`; dc-uar, the catalogue oracle's sheet dcuar_text).
fn mixed_fraction_under_space_grouping(section: &str, profile: &FormatProfile) -> bool {
    if profile.thousands_separator != " " {
        return false;
    }
    let Some((whole, fraction)) = section.rsplit_once(' ') else {
        return false;
    };
    let placeholders = |s: &str| !s.is_empty() && s.chars().all(|c| matches!(c, '0' | '#' | '?'));
    whole.chars().any(|c| matches!(c, '0' | '#'))
        && whole.chars().all(|c| matches!(c, '0' | '#' | '?' | ','))
        && fraction_code(fraction).is_some()
        && fraction.split_once('/').is_some_and(|(top, _)| placeholders(top))
}

/// The last convergent of `x`'s continued fraction, computed in binary64, whose denominator is
/// at most `largest`.
fn convergent(x: f64, largest: u64) -> Option<(u64, u64)> {
    let (mut h0, mut h1, mut k0, mut k1) = (0f64, 1f64, 1f64, 0f64);
    let mut best = None;
    let mut y = x;
    for _ in 0..64 {
        let a = y.floor();
        let (h2, k2) = (a * h1 + h0, a * k1 + k0);
        if k2 > largest as f64 || !h2.is_finite() || h2 >= 1e15 {
            break;
        }
        (h0, h1, k0, k1) = (h1, h2, k1, k2);
        best = Some((h1 as u64, k1 as u64));
        let rest = y - a;
        if rest == 0.0 {
            break;
        }
        y = 1.0 / rest;
    }
    best
}

/// TEXT of a number under `code`.
pub(crate) fn text(profile: &FormatProfile, system: DateSystem, value: f64, code: &str) -> Rendered {
    let sections = sections(code);
    let (section, signed_section) = choose(&sections, value)?;
    let stripped = strip_tokens(&section)?;
    if stripped.trim().eq_ignore_ascii_case("general") {
        let shown = general(profile, if signed_section { value.abs() } else { value });
        return Ok(shown);
    }
    let magnitude_only = signed_section;
    if datetime::looks_like_datetime_format(&stripped) {
        if value < 0.0 {
            return Err(Failure::Value);
        }
        if value >= 2_958_466.0 {
            return Err(Failure::Value);
        }
        if datetime::without_subseconds(&stripped)
            .chars()
            .any(|c| matches!(c, 'b' | 'B' | 'e' | 'g' | 'G' | '0'))
        {
            return Err(Failure::NotModelled);
        }
        return datetime::render_with_datetime_tokens(profile, system, value, &stripped)
            .ok_or(Failure::NotModelled);
    }
    if let Some(fraction) = fraction_code(&stripped) {
        return fraction.render(value, !magnitude_only);
    }
    if mixed_fraction_under_space_grouping(&stripped, profile) {
        return Err(Failure::Value);
    }
    let items = items(&stripped, profile)?;
    if !items.iter().any(|i| matches!(i, Item::Digit(_))) {
        // A section of literals alone writes them (an empty one writes nothing).
        if items.iter().any(|i| matches!(i, Item::Point | Item::Group | Item::Exponent(_))) {
            return Err(Failure::NotModelled);
        }
        if stripped.contains('@') {
            return Err(Failure::NotModelled);
        }
        return items
            .iter()
            .map(|i| literal_text(i, profile))
            .collect::<Result<String, Failure>>();
    }
    if stripped.contains('@') {
        return Err(Failure::NotModelled);
    }
    numeric(profile, value, &items, !magnitude_only)
}

/// TEXT of a text value under `code`: the fourth section, or a first section holding `@`, with
/// `@` written as the text; otherwise the text as it is.
pub(crate) fn text_of_text(text: &str, code: &str, profile: &FormatProfile) -> Rendered {
    let sections = sections(code);
    let section = match sections.get(3) {
        Some(section) => section.clone(),
        None if sections.len() == 1 && sections[0].contains('@') => sections[0].clone(),
        None => return Ok(text.to_owned()),
    };
    let stripped = strip_tokens(&section)?;
    let mut out = String::new();
    for part in stripped.split('@').enumerate() {
        if part.0 > 0 {
            out.push_str(text);
        }
        let items = items(part.1, profile)?;
        for item in &items {
            out.push_str(&literal_text(item, profile)?);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::format_profile;

    fn host() -> FormatProfile {
        format_profile(LocaleId::CurrentExcelHost)
            .with_separators(".", " ")
            .expect("separators")
    }

    fn t(value: f64, code: &str) -> Rendered {
        text(&host(), DateSystem::System1900, value, code)
    }

    /// Every row is Excel's TEXT, FIXED or DOLLAR on this machine (16.0.20430, en-ZA with `.` and
    /// a space), read ad hoc.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn text_renders_as_excel_did() {
        let ok = |s: &str| Ok(s.to_owned());
        let rows: &[(f64, &str, Rendered)] = &[
            (-1.0 / 3.0, "General", ok("-0.333333333")),
            (1.0 / 3.0, "General", ok("0.333333333")),
            (123456789012.0, "General", ok("1.23457E+11")),
            (-123456789012.0, "General", ok("-1.23457E+11")),
            (0.000000000123456, "General", ok("1.23456E-10")),
            (12345678901.0, "General", ok("12345678901")),
            (1234567890.5, "General", ok("1234567891")),
            (0.00001234, "General", ok("0.00001234")),
            (1E+100, "General", ok("1E+100")),
            (1234567.891, "General", ok("1234567.891")),
            (-0.001, "0.00", ok("0.00")),
            (0.5, "#.00", ok(".50")),
            (0.5, "#", ok("1")),
            (0.0, "#", ok("")),
            (1234.5, "0", ok("1235")),
            (2.5, "0", ok("3")),
            (-2.5, "0", ok("-3")),
            (1.005, "0.00", ok("1.01")),
            (1234567.891, "#,##0", ok("1234,568")),
            (1234567.891, "#,##0.0,", ok("1234,567.9,")),
            (1234567.891, "0,,", ok("1234568,,")),
            (1234.5, "#,##0.00", ok("1,234.50")),
            (0.256, "0%", ok("26%")),
            (0.256, "0.0%", ok("25.6%")),
            (-5.0, "0;(0)", ok("(5)")),
            (-5.0, "0;-0;zero", ok("-5")),
            (0.0, "0;-0;\\z\\e\\r\\o", ok("zero")),
            (0.0, "0;-0;\"zero\"", ok("zero")),
            (5.0, "\"R\"0.00", ok("R5.00")),
            (5.0, "$0.00", ok("$5.00")),
            (5.0, "R0.00", ok("R5.00")),
            (1234.5, "[$R-1C09]#,##0.00", ok("R1,234.50")),
            (1234.5, "0.00E+00", ok("1.23E+03")),
            (0.000123, "0.00E+00", ok("1.23E-04")),
            (123456.0, "##0.0E+0", ok("123.5E+3")),
            (5.0, "00000", ok("00005")),
            (5.25, "?.??", ok("5.25")),
            (5.25, "0.0?", ok("5.25")),
            (5.0, "[Red]0.00", ok("5.00")),
            (150.0, "[>100]\"big\";\"small\"", ok("big")),
            (45000.0, "yyyy-mm-dd", ok("2023-03-15")),
            (45000.0, "dddd, mmmm d, yyyy", ok("Wednesday, March 15, 2023")),
            (45000.75, "hh:mm AM/PM", ok("06:00 pm")),
            (45000.75, "h:mm:ss", ok("18:00:00")),
            (1.5, "[h]:mm", ok("36:00")),
            (45000.0, "ddd dd mmm yy", ok("Wed 15 Mar 23")),
            (45000.0, "m/d/yyyy", ok("3/15/2023")),
            (5.0, "", ok("")),
            (-5.0, "0.00;", ok("")),
            (5.0, "0.00;;;@", ok("5.00")),
            (1234.5, "#,##0.00 \"ZAR\"", ok(",1.23ZAR")),
            (0.5, "0.0 x", ok("0.0x")),
            (-1.0, "yyyy", Err(Failure::Value)),
            (12.345, "0.0_)", ok("12.3 ")),
            (5.0, "*-0", ok("5")),
        ];
        for (value, code, expected) in rows {
            assert_eq!(t(*value, code), *expected, "TEXT({value},{code:?})");
        }
        assert_eq!(text_of_text("x", "@!", &host()), Ok("x!".to_owned()));
        assert_eq!(text_of_text("abc", "0.00", &host()), Ok("abc".to_owned()));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fixed_and_dollar_render_as_excel_did() {
        let p = host();
        assert_eq!(fixed(&p, 1234.567, 2, false), "1 234.57");
        assert_eq!(fixed(&p, -1234.567, 2, false), "-1 234.57");
        assert_eq!(fixed(&p, 1234.567, -2, false), "1 200");
        assert_eq!(fixed(&p, 1234.567, 1, true), "1234.6");
        assert_eq!(fixed(&p, 0.5, 0, false), "1");
        assert_eq!(fixed(&p, 1.005, 2, false), "1.01");
        assert_eq!(fixed(&p, 1E+20, 2, false), "100 000 000 000 000 000 000.00");
        assert_eq!(dollar(&p, 1234.567, 2), "R1 234.57");
        assert_eq!(dollar(&p, -1234.567, 2), "-R1 234.57");
        assert_eq!(dollar(&p, 1234.567, -2), "R1 200");
        assert_eq!(dollar(&p, 0.5, 0), "R1");
    }
}
