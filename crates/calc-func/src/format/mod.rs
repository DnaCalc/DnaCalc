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
//! - Digits use the shared15-significant-digit conversion (whose remaining midpoint
//!   departures are counted under dc-b1l.25), then round a half away
//!   from zero, to the places the code shows; a negative number whose rounding is zero shows no
//!   minus sign; DOLLAR retains the negative sign when its decimal places are non-negative.
//! - Sections: one for every number; two for the non-negative and the negative numbers (the
//!   second without a minus sign); three adding zero; a fourth for text; a condition in brackets
//!   chooses its section. Colors in brackets are dropped; locale tags select date names; `[$R-1C09]` writes `R`.
//! - `General` writes at most 11 characters besides the sign, switching to `d.ddddE+xx` for a
//!   number too large or too small for that.
//! - Dates and times (y, m, d, h, s, AM/PM, elapsed [h], [m], [s]) through [`datetime`]; a negative
//!   number under a date code is #VALUE!.
//!
//! A fraction (`?/?`, `??/16`, `# ?/?`, `# ??/??`) renders as [`FractionLayout`] says. A literal
//! inside the decimal places and letters that are format tokens elsewhere (`B`, `e`, `g`, `A/P`)
//! are not modelled and refuse the call. Under a host whose group separator is a space (en-ZA) a
//! fraction with an integer part is #VALUE!, as Excel gave it.

pub(crate) mod cell_info;
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

/// The host profiles measured by the scalar formatting questions.
fn measured_host(profile: &FormatProfile) -> bool {
    matches!(profile.id, LocaleId::EnZa | LocaleId::CurrentExcelHost)
}

/// A literal atom exactly naming the measured currency ignores ASCII case.
/// Quoted longer words stay literal; unquoted characters arrive one at a time.
fn currency_literal<'a>(profile: &FormatProfile, text: &'a str) -> &'a str {
    if measured_host(profile) && profile.currency_symbol == "R" && text.eq_ignore_ascii_case("R") {
        profile.currency_symbol
    } else {
        text
    }
}

/// The AM or PM designator of a locale.
pub(crate) fn designator(profile: &FormatProfile, am: bool) -> &'static str {
    let lower = matches!(profile.id, LocaleId::EnZa | LocaleId::CurrentExcelHost);
    match (am, lower) {
        (true, true) => "am",
        (false, true) => "pm",
        (true, false) => "AM",
        (false, false) => "PM",
    }
}

// --- digits -------------------------------------------------------------------------------------

/// An integer format code uses only zero placeholders, literal digits and an
/// optional literal minus sign; none of these reads the locale.
pub(crate) fn integer_code(code: &str) -> bool {
    let digits = code.strip_prefix('-').unwrap_or(code);
    !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit())
}

/// A decimal code whose only tokens are digits, a literal sign and the known
/// decimal point, with an integer placeholder. It reads no other locale facts.
pub(crate) fn plain_decimal_code(code: &str) -> bool {
    let code = code.strip_prefix('-').unwrap_or(code);
    let Some((whole, fraction)) = code.split_once('.') else {
        return false;
    };
    !whole.is_empty()
        && whole.contains('0')
        && whole.bytes().all(|c| c.is_ascii_digit())
        && !fraction.is_empty()
        && fraction.bytes().all(|c| c.is_ascii_digit())
}

/// The magnitude of `x` rounded to 15 significant digits and then to `places` decimal places
/// (negative places round to tens, hundreds and so on), a half away from zero, as its integer
/// digits (no leading zero; empty for zero) and exactly `max(places, 0)` decimal digits.
pub(crate) fn digits(x: f64, places: i32) -> (String, String) {
    let first = fifteen(x.abs());
    let rounded = first.round_at(-places, Direction::Nearest).unwrap_or(first);
    split(&rounded, places.max(0) as usize)
}

/// FIXED/TEXT and concatenation share their first15-digit conversion. Reuse
/// its existing near-tie behavior, including the open dc-b1l.25 departures.
fn fifteen(x: f64) -> ExactDecimal {
    crate::number_text::decimal(x)
}

/// A decimal's integer digits (without leading zeros) and `places` decimal digits.
fn split(decimal: &ExactDecimal, places: usize) -> (String, String) {
    if decimal.coefficient.is_zero() {
        return (String::new(), "0".repeat(places));
    }
    let digits = decimal.coefficient.digits();
    let exponent = decimal.exponent;
    let (whole, fraction) = if exponent >= 0 {
        (
            format!("{digits}{}", "0".repeat(exponent as usize)),
            String::new(),
        )
    } else {
        let point = digits.len() as i64 + exponent as i64;
        if point <= 0 {
            (
                String::new(),
                format!("{}{digits}", "0".repeat((-point) as usize)),
            )
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
/// A missing profile refuses only when the output would contain a locale separator.
pub(crate) fn fixed(
    profile: Option<&FormatProfile>,
    value: f64,
    places: i32,
    no_commas: bool,
) -> Rendered {
    let decimal_separator = if places > 0 {
        Some(profile.ok_or(Failure::NotModelled)?.decimal_separator)
    } else {
        None
    };
    let (whole, fraction) = digits(value, places);
    let negative = value < 0.0 && (whole.chars().chain(fraction.chars()).any(|c| c != '0'));
    let whole = if whole.is_empty() {
        "0".to_owned()
    } else {
        whole
    };
    let whole = if no_commas || whole.len() <= 3 {
        whole
    } else {
        grouped(
            &whole,
            profile.ok_or(Failure::NotModelled)?.thousands_separator,
        )
    };
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(&whole);
    if let Some(separator) = decimal_separator {
        out.push_str(separator);
        out.push_str(&fraction);
    }
    Ok(out)
}

/// DOLLAR: the number in the locale's currency format, rounded to `places`.
pub(crate) fn dollar(profile: &FormatProfile, value: f64, places: i32) -> Rendered {
    let magnitude = fixed(Some(profile), value.abs(), places, false)?;
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
    let negative =
        value < 0.0 && (places >= 0 || magnitude.chars().any(|c| c.is_ascii_digit() && c != '0'));
    if !negative {
        return Ok(body);
    }
    Ok(match profile.currency_negative_pattern {
        CurrencyNegativePattern::LeadingMinus | CurrencyNegativePattern::MinusBeforeSymbol => {
            format!("-{body}")
        }
        CurrencyNegativePattern::TrailingMinus => format!("{body}-"),
        CurrencyNegativePattern::Parentheses => format!("({body})"),
    })
}

// --- General --------------------------------------------------------------------------------------

/// The General format's text: at most 11 characters besides the sign, as a cell of Excel's
/// standard column width shows a number. Public as [`crate::general_format`] for an app that
/// shows a value as a cell would.
pub fn general(profile: &FormatProfile, value: f64) -> String {
    const WIDTH: i64 = 11;
    if !value.is_finite() {
        if !value.is_nan() {
            return "#NUM!".to_owned();
        }
        // Excel's retained exponent-field-2047 readings show two significant digits,
        // without a minus sign, even when the stored encoding has its sign bit set.
        let exact = ExactDecimal::binary(value);
        let exponent = exact.magnitude();
        let (mantissa, carry) = significant(&exact, 2);
        let (head, tail) = mantissa.split_at(1);
        let tail = tail.trim_end_matches('0');
        let point = if tail.is_empty() {
            ""
        } else {
            profile.decimal_separator
        };
        return format!("{head}{point}{tail}E+{}", i64::from(exponent) + carry);
    }
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
        let whole = if whole.is_empty() {
            "0".to_owned()
        } else {
            whole
        };
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

/// Whether a section is a text section: an `@` outside quotes and escapes, and no digit placeholder.
fn is_text_section(section: &str) -> bool {
    let (mut quoted, mut escaped, mut at_sign, mut digit) = (false, false, false, false);
    for ch in section.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            '@' if !quoted => at_sign = true,
            '0' | '#' | '?' if !quoted => digit = true,
            _ => {}
        }
    }
    at_sign && !digit
}

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

/// A section with its bracketed tokens resolved: colors dropped, locale recorded for names, a currency tag
/// `[$sym-xxxx]` written as its symbol (quoted), conditions dropped, elapsed-time tokens kept.
fn strip_tokens(section: &str) -> Result<(String, Option<crate::locale::LocaleId>, bool), Failure> {
    let mut out = String::new();
    let mut name_locale = None;
    let mut locale_declared = false;
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
                    let (symbol, locale) = body.split_once('-').unwrap_or((body, ""));
                    if !locale.is_empty() {
                        locale_declared = true;
                        use crate::locale::LocaleId;
                        name_locale = u16::from_str_radix(locale, 16)
                            .ok()
                            .and_then(LocaleId::from_excel_lcid)
                            .or_else(|| LocaleId::from_bcp47_language_tag(locale));
                    }
                    if !symbol.is_empty() {
                        out.push('"');
                        out.push_str(symbol);
                        out.push('"');
                    }
                } else if is_condition(&token) || token.chars().all(|c| c.is_ascii_alphanumeric()) {
                    // A color ([Red], [Color10]) or a condition.
                } else {
                    return Err(Failure::NotModelled);
                }
            }
            _ => out.push(ch),
        }
    }
    Ok((out, name_locale, locale_declared))
}

/// The section a number is rendered with, and whether that section shows the number's sign
/// itself (so its magnitude is rendered).
fn choose(sections: &[String], value: f64) -> Result<(String, bool), Failure> {
    // A second section that is the text section (`# ?/?;@`) leaves one section for every number
    // (Excel 16.0.20430 shows -0.5 as `- 1/2` under `# ?/?;@`).
    let sections = match sections {
        [first, second] if is_text_section(second) => &sections[..1],
        _ => sections,
    };
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
    /// An active text placeholder, distinguished from a quoted or escaped `@`.
    TextPlaceholder,
    Literal(String),
}

/// A section's items, with quoted text, escapes, `_x` (a space) and `*x` (nothing) resolved.
fn items(section: &str, profile: &FormatProfile) -> Result<Vec<Item>, Failure> {
    let decimal = profile.format_code_decimal_token;
    let group = profile.format_code_group_token;
    let chars: Vec<char> = section.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    let literal = |out: &mut Vec<Item>, text: &str| {
        let text = currency_literal(profile, text);
        match out.last_mut() {
            Some(Item::Literal(existing)) => existing.push_str(text),
            _ => out.push(Item::Literal(text.to_owned())),
        }
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
                let text = next.to_string();
                match out.last_mut() {
                    Some(Item::Literal(existing)) => existing.push_str(&text),
                    _ => out.push(Item::Literal(text)),
                }
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
            '@' => {
                out.push(Item::TextPlaceholder);
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
            'E' if measured_host(profile) => return Err(Failure::Value),
            c if c.is_ascii_alphabetic() => {
                // A letter that is a token in some other format (dates, eras, A/P) is not modelled.
                if "ymdhsbegYMDHSBEG".contains(c)
                    || (matches!(c, 'a' | 'A') && !measured_host(profile))
                {
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
fn numeric(profile: &FormatProfile, value: f64, items: &[Item], show_minus: bool) -> Rendered {
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
    let point_at = items[..mantissa_end].iter().position(|i| *i == Item::Point);
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
    // A group separator beside a literal in the integer part (dc-b1l, Excel 16.0.20430 on
    // 2026-10-25, the catalogue oracle's dcuar_text rows): one that follows a literal and precedes
    // a placeholder is written as the separator's text, a literal (TEXT(5415485.8,"##- 0") is
    // 541548- 6); one that follows a placeholder and precedes a literal is dropped and groups
    // nothing (TEXT(843124.584,"0# ,0") is 84312,5). One between two placeholders groups.
    let separator = Item::Literal(profile.thousands_separator.to_owned());
    let integer_items: Vec<Item> = items[first..integer_end.max(first)]
        .iter()
        .enumerate()
        .filter_map(|(at, item)| {
            let all = &items[first..integer_end.max(first)];
            if *item != Item::Group {
                return Some(item.clone());
            }
            let before = at.checked_sub(1).map(|p| &all[p]);
            let after = all.get(at + 1);
            match (before, after) {
                (Some(Item::Literal(_)), Some(Item::Digit(_))) => Some(separator.clone()),
                (Some(Item::Digit(_)), Some(Item::Literal(_))) => None,
                _ => Some(Item::Group),
            }
        })
        .collect();
    let integer: Vec<&Item> = integer_items.iter().collect();
    let mut fraction_literals: Vec<(usize, String)> = Vec::new();
    let fraction: Vec<char> = match point_at {
        Some(p) => {
            let mut places = Vec::new();
            for item in &items[p + 1..mantissa_end] {
                match item {
                    Item::Digit(c) => places.push(*c),
                    Item::Group => {}
                    Item::Literal(text) => fraction_literals.push((places.len(), text.clone())),
                    _ => return Err(Failure::NotModelled),
                }
            }
            places
        }
        None => Vec::new(),
    };
    // A literal among the decimal places stands after as many of them as precede it in the code
    // (Excel 16.0.20430, dc-uar: TEXT(1234567.891,"# ##0.0,0") is 1 234 567.8,9); modelled for
    // `0` places only.
    if !fraction_literals.is_empty() && fraction.iter().any(|c| *c != '0') {
        return Err(Failure::NotModelled);
    }
    let grouping = integer.iter().enumerate().any(|(at, i)| {
        **i == Item::Group
            && integer[at + 1..]
                .iter()
                .any(|j| matches!(j, Item::Digit(_)))
    });
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
            let mut frac_text = fraction_part(&frac, &fraction);
            for (after, text) in fraction_literals.iter().rev() {
                frac_text.insert_str((*after).min(frac_text.len()), text);
            }
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
        Item::Digit(_) | Item::Exponent(_) | Item::TextPlaceholder => {
            return Err(Failure::NotModelled);
        }
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
        // A literal among grouped placeholders stands where as many digits are to its right as
        // there are placeholders to its right in the code, the grouping counted on the digits
        // alone (Excel 16.0.20430, dc-uar: TEXT(1234567.891,"# ##0,0") is 1 234 56,8, and
        // TEXT(12,"#-# ##0") is -12). Modelled for `0` and `#` placeholders and literals that do
        // not fall beside a group separator.
        // A group separator beside a literal that the caller did not resolve (one between two
        // literals, or at either end) is not modelled.
        let beside_literal = pattern.iter().enumerate().any(|(at, item)| {
            **item == Item::Group
                && (at == 0
                    || !matches!(pattern[at - 1], Item::Digit(_))
                    || !matches!(pattern.get(at + 1), Some(Item::Digit(_))))
        });
        if beside_literal && pattern.iter().any(|i| matches!(i, Item::Literal(_))) {
            return Err(Failure::NotModelled);
        }
        let mut literals: Vec<(usize, String)> = Vec::new();
        let mut right = 0;
        for item in pattern.iter().rev() {
            match item {
                Item::Digit('?') => return Err(Failure::NotModelled),
                Item::Digit(_) => right += 1,
                Item::Group => {}
                other => {
                    // Literals with no placeholder between them stand together, in the code's
                    // order (dc-b1l: TEXT(503134,"0- 0 #") is 503 1- 34).
                    let text = literal_text(other, profile)?;
                    match literals.last_mut() {
                        Some((count, joined)) if *count == right => joined.insert_str(0, &text),
                        _ => literals.push((right, text)),
                    }
                }
            }
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
        let mut text = grouped(&digits, profile.thousands_separator);
        // Literals nearest the right first; each goes left of its digits.
        for (count, literal) in &literals {
            let mut seen = 0;
            let mut at = text.len();
            if *count > 0 {
                at = 0;
                for (index, c) in text.char_indices().rev() {
                    if c.is_ascii_digit() {
                        seen += 1;
                        if seen == *count {
                            at = index;
                            break;
                        }
                    }
                }
                // A literal where a group separator falls stands after it (dc-b1l:
                // TEXT(9514218,"0x#0# ###") is 9 x514 218).
                if seen < *count {
                    at = 0;
                }
            }
            text.insert_str(at, literal);
        }
        return Ok(text);
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
        // Zero fills every whole-number placeholder: `##0.0E+0` gives `000.0E+0` (measured, Excel
        // 16.0 build 20527, dc-0f1k formats oracle).
        let mantissa = format!(
            "{}{}{}",
            "0".repeat(whole_places.len()),
            if fraction.is_empty() {
                ""
            } else {
                profile.decimal_separator
            },
            "0".repeat(fraction.len())
        );
        let exponent = format!(
            "E{}{}",
            if plus { "+" } else { "" },
            "0".repeat(exponent_digits.len())
        );
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

/// A fraction section: an optional integer part, then numerator placeholders, `/` and a
/// denominator, with literal text around them (dc-uar for the fraction alone, dc-0f1k.6 for the
/// rest; every behaviour here is a reading of Excel 16.0.20430, `Range.Text` of a cell and the
/// cell's `TEXT` formula, in apps/dna-onecalc/tests/fixtures/formats):
///
/// - A denominator of placeholders (`?`, `#`) or zeros limits the approximation to as many digits
///   as there are placeholders and takes the last convergent of the number's continued fraction,
///   computed in binary64 (`TEXT(0.7,"?/?")` is `2/3`, `TEXT(0.9,"?/?")` is `8/9` since
///   1/(1/0.9-1) is 8.999..., and `TEXT(0.1,"?/?")` is `0/1`). A fixed denominator `d` takes the
///   numerator |x|*d rounded a half up from the 15 significant digits of the product.
/// - Without an integer part the fraction is improper (`1234.5678` under `??/??` is `100000/81`);
///   a value from 2147483647 up, or a numerator above 32767 over a fixed denominator, shows as a
///   row of `#`, which TEXT gives as #VALUE!. With an integer part (`# ?/?`) the integer part is
///   the whole number, the fraction approximates what is left, and a fraction that rounds up to
///   1 carries into the whole number (`2.999` under `# ?/?` is `3`).
/// - A numerator or denominator pads to its placeholders: digits fill the numerator's placeholders
///   from the right (`0` pads with a zero, `?` with a space, `#` with nothing); a denominator of
///   zeros pads on the left, one of `?` and `#` is left-aligned and `?` pads with a space.
/// - A numerator of 0 with an integer part, and no `0` in the numerator, shows no fraction: the
///   fraction's width in spaces when a `?` stands anywhere in the code, nothing otherwise, and the
///   integer part writes `0` for a value that is wholly zero (`0` under `# ?/?` is `0` and four
///   spaces, under `# #/#` just `0`). A `0` in the numerator writes `0/1` instead.
/// - The text between the integer part and the numerator is written as it stands when the integer
///   part wrote anything (a `?` placeholder writes a space); otherwise it is written as spaces of its
///   width when the numerator's first placeholder is `?`, and not at all when it is not.
/// - A minus sign comes first (before any literal text): always with an integer part (`-0` is
///   shown), and without one only for a numerator other than 0. A `%` scales by 100.
struct FractionLayout {
    /// Literal text before the integer part, or before the numerator when there is none.
    prefix: String,
    /// The integer part's placeholders (none: the fraction stands alone).
    integer: Vec<char>,
    grouping: bool,
    /// Literal text between the integer part and the numerator.
    separator: String,
    numerator: Vec<char>,
    denominator: Denominator,
    suffix: String,
    percents: i32,
}

enum Denominator {
    /// A number written in the code (`8`, `100`).
    Fixed(u64),
    /// Placeholders `?` and `#`: left-aligned, `?` pads with a space.
    Places(Vec<char>),
    /// Zeros: pads with zeros on the left.
    Zeros(usize),
}

enum FractionToken {
    Literal(String),
    Placeholder(char),
    Group,
    Slash,
    Digit(char),
    Percent,
}

/// The fraction `section` writes, if it is one: `Ok(None)` when it has no `/` outside quotes and
/// escapes, `Value` for a code Excel refuses (two fractions, a decimal point, an exponent), and
/// `NotModelled` for the shapes not measured.
fn fraction_layout(
    section: &str,
    profile: &FormatProfile,
) -> Result<Option<FractionLayout>, Failure> {
    let decimal = profile.format_code_decimal_token;
    let group = profile.format_code_group_token;
    let chars: Vec<char> = section.chars().collect();
    let mut tokens: Vec<FractionToken> = Vec::new();
    let mut at = 0;
    // What makes the code not a fraction Excel shows: known only once a `/` is found.
    let mut refusal: Option<Failure> = None;
    let push_literal = |tokens: &mut Vec<FractionToken>, text: &str| match tokens.last_mut() {
        Some(FractionToken::Literal(existing)) => existing.push_str(text),
        _ => tokens.push(FractionToken::Literal(text.to_owned())),
    };
    while at < chars.len() {
        let ch = chars[at];
        let rest: String = chars[at..].iter().collect();
        match ch {
            '"' => {
                let Some(close) = chars[at + 1..].iter().position(|c| *c == '"') else {
                    refusal = Some(Failure::NotModelled);
                    break;
                };
                let text: String = chars[at + 1..at + 1 + close].iter().collect();
                push_literal(&mut tokens, &text);
                at += close + 2;
            }
            '\\' => {
                let Some(next) = chars.get(at + 1) else {
                    refusal = Some(Failure::NotModelled);
                    break;
                };
                push_literal(&mut tokens, &next.to_string());
                at += 2;
            }
            '_' => {
                push_literal(&mut tokens, " ");
                at += 2;
            }
            '*' => at += 2,
            '0' | '#' | '?' => {
                tokens.push(FractionToken::Placeholder(ch));
                at += 1;
            }
            '%' => {
                tokens.push(FractionToken::Percent);
                at += 1;
            }
            '/' => {
                tokens.push(FractionToken::Slash);
                at += 1;
            }
            '1'..='9' => {
                tokens.push(FractionToken::Digit(ch));
                at += 1;
            }
            '@' => {
                refusal.get_or_insert(Failure::NotModelled);
                at += 1;
            }
            'E' | 'e' if matches!(chars.get(at + 1), Some('+' | '-')) => {
                refusal.get_or_insert(Failure::Value);
                at += 2;
            }
            _ if rest.starts_with(decimal) => {
                refusal.get_or_insert(Failure::Value);
                at += decimal.chars().count();
            }
            _ if !group.is_empty() && rest.starts_with(group) => {
                tokens.push(FractionToken::Group);
                at += group.chars().count();
            }
            c if c.is_ascii_alphabetic() => {
                if "ymdhsbegYMDHSBEG".contains(c)
                    || (matches!(c, 'a' | 'A') && !measured_host(profile))
                {
                    refusal.get_or_insert(Failure::NotModelled);
                }
                push_literal(&mut tokens, &c.to_string());
                at += 1;
            }
            c => {
                push_literal(&mut tokens, &c.to_string());
                at += 1;
            }
        }
    }
    let slashes: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, FractionToken::Slash))
        .map(|(i, _)| i)
        .collect();
    let slash = match slashes.as_slice() {
        [] => return Ok(None),
        [one] => *one,
        _ => return Err(refusal.unwrap_or(Failure::Value)),
    };
    if let Some(refusal) = refusal {
        return Err(refusal);
    }
    let mut numerator_from = slash;
    while numerator_from > 0 && matches!(tokens[numerator_from - 1], FractionToken::Placeholder(_))
    {
        numerator_from -= 1;
    }
    let numerator: Vec<char> = tokens[numerator_from..slash]
        .iter()
        .filter_map(|t| match t {
            FractionToken::Placeholder(c) => Some(*c),
            _ => None,
        })
        .collect();
    let mut denominator_to = slash + 1;
    let mut denominator: Vec<char> = Vec::new();
    while let Some(token) = tokens.get(denominator_to) {
        match token {
            FractionToken::Placeholder(c) | FractionToken::Digit(c) => denominator.push(*c),
            _ => break,
        }
        denominator_to += 1;
    }
    if numerator.is_empty() || denominator.is_empty() {
        return Err(Failure::NotModelled);
    }
    let denominator = if denominator.iter().all(|c| matches!(c, '?' | '#')) {
        Denominator::Places(denominator)
    } else if denominator.iter().all(|c| *c == '0') {
        Denominator::Zeros(denominator.len())
    } else if denominator.len() <= 9
        && denominator.iter().all(|c| c.is_ascii_digit())
        && denominator[0] != '0'
    {
        Denominator::Fixed(
            denominator
                .iter()
                .collect::<String>()
                .parse()
                .map_err(|_| Failure::NotModelled)?,
        )
    } else {
        return Err(Failure::NotModelled);
    };
    // The integer part: the placeholders (and group separators) before the literal text that
    // precedes the numerator. Without that text the placeholders beside the numerator are its own.
    let head = &tokens[..numerator_from];
    let mut separator_from = head.len();
    while separator_from > 0 && matches!(head[separator_from - 1], FractionToken::Literal(_)) {
        separator_from -= 1;
    }
    let mut integer_from = separator_from;
    while integer_from > 0
        && matches!(
            head[integer_from - 1],
            FractionToken::Placeholder(_) | FractionToken::Group
        )
    {
        integer_from -= 1;
    }
    let integer_tokens = &head[integer_from..separator_from];
    let has_integer = separator_from < head.len()
        && integer_tokens
            .iter()
            .any(|t| matches!(t, FractionToken::Placeholder(_)));
    let (prefix_to, integer_range, separator_range) = if has_integer {
        (
            integer_from,
            integer_from..separator_from,
            separator_from..head.len(),
        )
    } else {
        (head.len(), 0..0, 0..0)
    };
    let text = |tokens: &[FractionToken]| -> Result<(String, i32), Failure> {
        let mut out = String::new();
        let mut percents = 0;
        for token in tokens {
            match token {
                FractionToken::Literal(text) => out.push_str(text),
                FractionToken::Percent => {
                    out.push('%');
                    percents += 1;
                }
                _ => return Err(Failure::NotModelled),
            }
        }
        Ok((out, percents))
    };
    let (prefix, p1) = text(&head[..prefix_to])?;
    let (separator, p2) = text(&head[separator_range])?;
    let (suffix, p3) = text(&tokens[denominator_to..])?;
    let integer: Vec<char> = head[integer_range.clone()]
        .iter()
        .filter_map(|t| match t {
            FractionToken::Placeholder(c) => Some(*c),
            _ => None,
        })
        .collect();
    let grouping = head[integer_range]
        .iter()
        .any(|t| matches!(t, FractionToken::Group));
    Ok(Some(FractionLayout {
        prefix,
        integer,
        grouping,
        separator,
        numerator,
        denominator,
        suffix,
        percents: p1 + p2 + p3,
    }))
}

/// Digits fill `placeholders` from the right; a placeholder left over pads (`0` with a zero, `?`
/// with a space, `#` with nothing) and digits left over are written whole.
fn fill_placeholders(placeholders: &[char], digits: &str) -> String {
    let missing = placeholders.len().saturating_sub(digits.len());
    let mut out = String::new();
    for c in &placeholders[..missing] {
        match c {
            '0' => out.push('0'),
            '?' => out.push(' '),
            _ => {}
        }
    }
    out.push_str(digits);
    out
}

impl FractionLayout {
    fn denominator_text(&self, denominator: u64) -> String {
        let bottom = denominator.to_string();
        match &self.denominator {
            Denominator::Fixed(_) => bottom,
            Denominator::Zeros(width) => {
                format!("{}{bottom}", "0".repeat(width.saturating_sub(bottom.len())))
            }
            Denominator::Places(places) => {
                let missing = places.len().saturating_sub(bottom.len());
                let mut out = bottom;
                for c in &places[places.len() - missing..] {
                    if *c == '?' {
                        out.push(' ');
                    }
                }
                out
            }
        }
    }

    fn render(&self, profile: &FormatProfile, value: f64, show_minus: bool) -> Rendered {
        let x = value.abs() * 100f64.powi(self.percents);
        if !x.is_finite() {
            return Err(Failure::Value);
        }
        let mixed = !self.integer.is_empty();
        let (whole, rest) = if mixed {
            (x.floor(), x - x.floor())
        } else {
            (0.0, x)
        };
        let (mut numerator, denominator) = match &self.denominator {
            Denominator::Fixed(d) => {
                let (n, _) = digits(rest * *d as f64, 0);
                let n: u64 = if n.is_empty() {
                    0
                } else {
                    n.parse().map_err(|_| Failure::Value)?
                };
                if !mixed && n > 32767 {
                    return Err(Failure::Value);
                }
                (n, *d)
            }
            Denominator::Places(places) => self.approximate(rest, places.len(), mixed)?,
            Denominator::Zeros(width) => self.approximate(rest, *width, mixed)?,
        };
        let mut whole_text = if whole > 0.0 {
            digits(whole, 0).0
        } else {
            String::new()
        };
        if mixed && numerator == denominator {
            // The fraction rounded up to 1: it carries into the whole number.
            whole_text = digits(whole + 1.0, 0).0;
            numerator = 0;
        }
        let minus = if show_minus && value < 0.0 && (mixed || numerator != 0) {
            "-"
        } else {
            ""
        };
        let numerator_text = |n: u64| fill_placeholders(&self.numerator, &n.to_string());
        if !mixed {
            return Ok(format!(
                "{minus}{}{}/{}{}",
                self.prefix,
                numerator_text(numerator),
                self.denominator_text(denominator),
                self.suffix
            ));
        }
        let blank = numerator == 0 && !self.numerator.contains(&'0');
        let pads = self.integer.iter().any(|c| matches!(c, '0' | '?'));
        let separator_shows = !whole_text.is_empty() || pads || self.numerator[0] == '?';
        if whole_text.is_empty() && numerator == 0 && (blank || separator_shows) {
            whole_text = "0".to_owned();
        }
        let padded = fill_placeholders(&self.integer, &whole_text);
        let integer_text = if self.grouping {
            grouped_digits(&padded, profile.thousands_separator)
        } else {
            padded
        };
        let separator = if !integer_text.is_empty() {
            self.separator.clone()
        } else if separator_shows {
            " ".repeat(self.separator.chars().count())
        } else {
            String::new()
        };
        let body = if blank {
            let anywhere = self
                .integer
                .iter()
                .chain(&self.numerator)
                .any(|c| *c == '?')
                || matches!(&self.denominator, Denominator::Places(p) if p.contains(&'?'));
            if anywhere {
                let one = match &self.denominator {
                    Denominator::Fixed(d) => *d,
                    _ => 1,
                };
                let width = self.separator.chars().count()
                    + numerator_text(0).chars().count()
                    + 1
                    + self.denominator_text(one).chars().count();
                " ".repeat(width)
            } else {
                String::new()
            }
        } else {
            format!(
                "{separator}{}/{}",
                numerator_text(numerator),
                self.denominator_text(denominator)
            )
        };
        Ok(format!(
            "{minus}{}{integer_text}{body}{}",
            self.prefix, self.suffix
        ))
    }

    /// The last convergent of `rest` whose denominator has at most `width` digits.
    fn approximate(&self, rest: f64, width: usize, mixed: bool) -> Result<(u64, u64), Failure> {
        if !mixed && rest >= 2_147_483_647.0 {
            return Err(Failure::Value);
        }
        convergent(rest, 10u64.pow(width as u32) - 1).ok_or(Failure::NotModelled)
    }
}

/// `padded` (digits, and the spaces or zeros a placeholder wrote) with `separator` between groups
/// of three digits counted from the right.
fn grouped_digits(padded: &str, separator: &str) -> String {
    let digit_count = padded.chars().filter(|c| c.is_ascii_digit()).count();
    let mut out = String::new();
    let mut seen = 0;
    for c in padded.chars() {
        if c.is_ascii_digit() {
            if seen > 0 && (digit_count - seen) % 3 == 0 {
                out.push_str(separator);
            }
            seen += 1;
        }
        out.push(c);
    }
    out
}

/// Whether `section` is a fraction with an integer part set off by the group separator, under a
/// locale whose group separator is a space: Excel 16.0.20430 under en-ZA gave #VALUE! for every
/// such code it was given (`# ?/?`, `# ??/??`, `# ???/???`, `# ?/8`, `# ?/100`, `0 ?/?`,
/// `#,##0 ?/?`; dc-uar, the catalogue oracle's sheet dcuar_text).
fn mixed_fraction_under_space_grouping(section: &str, profile: &FormatProfile) -> bool {
    if profile.format_code_group_token != " " {
        return false;
    }
    let Some((whole, fraction)) = section.rsplit_once(' ') else {
        return false;
    };
    let placeholders = |s: &str| !s.is_empty() && s.chars().all(|c| matches!(c, '0' | '#' | '?'));
    whole.chars().any(|c| matches!(c, '0' | '#'))
        && whole.chars().all(|c| matches!(c, '0' | '#' | '?' | ','))
        && fraction.split_once('/').is_some_and(|(top, bottom)| {
            placeholders(top)
                && (placeholders(bottom)
                    || (!bottom.is_empty()
                        && bottom.len() <= 9
                        && bottom.chars().all(|c| c.is_ascii_digit())
                        && !bottom.starts_with('0')))
        })
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

/// Why a value has no text as a cell shows it under a number format ([`cell_text`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellFormatError {
    /// The value cannot be shown under the code: a negative number or one past 9999-12-31 under
    /// a date or time code, which Excel's cell shows as a row of `#`.
    Unshowable,
    /// The text under this code is not modelled (the cases [`text`] refuses).
    NotModelled,
}

/// A cell's value as the cell shows it under the number format `code` (dc-09v): a number or a
/// text through the one renderer TEXT uses ([`text`], [`text_of_text`]); a logical as it is, which
/// no format changes. An error or an array gets no text here (`NotModelled`): the caller shows it
/// as it shows it unformatted. The column's width is not applied: a skin that clips or shows `#`
/// for a number wider than its column does so itself.
pub fn cell_text(
    profile: &FormatProfile,
    system: DateSystem,
    value: &crate::value::CalcValue,
    code: &str,
) -> Result<String, CellFormatError> {
    // OOXML cell formats use invariant punctuation. Their output separators still
    // come from the host. Formula TEXT retains its separate localized-code contract.
    let profile = &FormatProfile {
        format_code_decimal_token: ".",
        format_code_group_token: ",",
        format_code_token_policy: crate::locale::FormatCodeTokenPolicy::InvariantExcel,
        ..*profile
    };
    render_cell_text(profile, system, value, code)
}

/// A cell value under a format entered using the selected locale's punctuation.
/// Uses the same value renderer as workbook formats and formula TEXT.
pub fn cell_text_localized(
    profile: &FormatProfile,
    system: DateSystem,
    value: &crate::value::CalcValue,
    code: &str,
) -> Result<String, CellFormatError> {
    render_cell_text(&localized_code_profile(profile), system, value, code)
}

fn render_cell_text(
    profile: &FormatProfile,
    system: DateSystem,
    value: &crate::value::CalcValue,
    code: &str,
) -> Result<String, CellFormatError> {
    use crate::value::CoreRef;
    let failure = |failure: Failure| match failure {
        Failure::Value => CellFormatError::Unshowable,
        Failure::NotModelled => CellFormatError::NotModelled,
    };
    match value.core_ref() {
        CoreRef::Number(number) if value.is_excel_overflow() => {
            if code.trim().eq_ignore_ascii_case("general") {
                Ok(general(profile, number))
            } else {
                // The retained readings cover General only. A custom cell format
                // has no measured rendering yet; the application labels its fallback.
                Err(CellFormatError::NotModelled)
            }
        }
        CoreRef::Number(number) => render_text(profile, system, number, code).map_err(failure),
        CoreRef::Text(text) => {
            render_text_of_text(&text.to_string_lossy(), code, profile).map_err(failure)
        }
        CoreRef::Logical(true) => Ok("TRUE".to_owned()),
        CoreRef::Logical(false) => Ok("FALSE".to_owned()),
        CoreRef::Empty => Ok(String::new()),
        _ => Err(CellFormatError::NotModelled),
    }
}

/// The color a number format's section gives the text a cell shows: one of the eight Excel names
/// (`[Red]`), or an entry of the workbook's 56-color palette (`[Color10]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatColor {
    Black,
    Blue,
    Cyan,
    Green,
    Magenta,
    Red,
    White,
    Yellow,
    /// `[ColorN]`, N from 1 to 56: the palette entry a workbook may redefine
    /// (`<indexedColors>`, its entry N + 7).
    Palette(u8),
}

/// Excel's default 56-color palette, `[Color1]` to `[Color56]`. Excel 16.0.20430 showed
/// `[Color10]`, `[Color45]` and `[Color53]` as entries 10, 45 and 53 here (calc-host's
/// `tests/fixtures/dc_09v/excel-shown.json`).
pub const DEFAULT_PALETTE: [&str; 56] = [
    "#000000", "#ffffff", "#ff0000", "#00ff00", "#0000ff", "#ffff00", "#ff00ff", "#00ffff",
    "#800000", "#008000", "#000080", "#808000", "#800080", "#008080", "#c0c0c0", "#808080",
    "#9999ff", "#993366", "#ffffcc", "#ccffff", "#660066", "#ff8080", "#0066cc", "#ccccff",
    "#000080", "#ff00ff", "#ffff00", "#00ffff", "#800080", "#800000", "#008080", "#0000ff",
    "#00ccff", "#ccffff", "#ccffcc", "#ffff99", "#99ccff", "#ff99cc", "#cc99ff", "#ffcc99",
    "#3366ff", "#33cccc", "#99cc00", "#ffcc00", "#ff9900", "#ff6600", "#666699", "#969696",
    "#003366", "#339966", "#003300", "#333300", "#993300", "#993366", "#333399", "#333333",
];

impl FormatColor {
    /// The color's RGB, as `#rrggbb`: a named color's, or the palette entry's in Excel's default
    /// palette (a workbook that redefines its palette is read by the caller).
    pub const fn rgb(self) -> &'static str {
        match self {
            FormatColor::Palette(index) => DEFAULT_PALETTE[(index as usize).saturating_sub(1) % 56],
            FormatColor::Black => "#000000",
            FormatColor::Blue => "#0000ff",
            FormatColor::Cyan => "#00ffff",
            FormatColor::Green => "#00ff00",
            FormatColor::Magenta => "#ff00ff",
            FormatColor::Red => "#ff0000",
            FormatColor::White => "#ffffff",
            FormatColor::Yellow => "#ffff00",
        }
    }
}

/// The color the section of `code` that shows `value` names (dc-09v): the section [`text`] or
/// [`text_of_text`] would render it with, read up to its first character that is not in a
/// bracketed token; `None` when the section names none or the value takes no section.
pub fn cell_color(code: &str, value: &crate::value::CalcValue) -> Option<FormatColor> {
    use crate::value::CoreRef;
    let sections = sections(code);
    let section = match value.core_ref() {
        CoreRef::Number(number) => choose(&sections, number).ok()?.0,
        CoreRef::Text(_) => match sections.get(3) {
            Some(section) => section.clone(),
            None if sections.len() == 1 && sections[0].contains('@') => sections[0].clone(),
            None => return None,
        },
        _ => return None,
    };
    let mut rest = section.as_str();
    while let Some((token, after)) = bracket(rest) {
        let color = match token.to_ascii_lowercase().as_str() {
            "black" => Some(FormatColor::Black),
            "blue" => Some(FormatColor::Blue),
            "cyan" => Some(FormatColor::Cyan),
            "green" => Some(FormatColor::Green),
            "magenta" => Some(FormatColor::Magenta),
            "red" => Some(FormatColor::Red),
            "white" => Some(FormatColor::White),
            "yellow" => Some(FormatColor::Yellow),
            other => other
                .strip_prefix("color")
                .and_then(|digits| digits.parse::<u8>().ok())
                .filter(|index| (1..=56).contains(index))
                .map(FormatColor::Palette),
        };
        if color.is_some() {
            return color;
        }
        rest = after;
    }
    None
}

/// A declared date-name locale supplies all profile fields used by these date-only
/// sections. Host separators and designators remain unavailable; admit only fields
/// that never read them, then use the same formatter with the locale the code names.
pub(crate) fn text_declared_date_locale(system: DateSystem, value: f64, code: &str) -> Rendered {
    let sections = sections(code);
    if sections
        .iter()
        .any(|section| condition_of(section).is_some())
    {
        return Err(Failure::NotModelled);
    }
    let (section, _) = choose(&sections, value).map_err(|_| Failure::NotModelled)?;
    let (stripped, name_locale, _) = strip_tokens(&section).map_err(|_| Failure::NotModelled)?;
    let locale = name_locale.ok_or(Failure::NotModelled)?;
    if stripped.trim().is_empty()
        || !stripped.chars().all(|c| {
            matches!(
                c,
                'y' | 'Y' | 'm' | 'M' | 'd' | 'D' | 'g' | 'G' | 'e' | 'E' | ' '
            )
        })
    {
        return Err(Failure::NotModelled);
    }
    text(&crate::locale::format_profile(locale), system, value, code)
}

/// TEXT of a number under `code`.
pub(crate) fn text(
    profile: &FormatProfile,
    system: DateSystem,
    value: f64,
    code: &str,
) -> Rendered {
    // Excel 16.0.20527: TEXT(-0.1, "") is "-"; positive values
    // produce empty text. This is specific to TEXT's empty code.
    if code.is_empty() && value.is_finite() {
        return Ok(if value < 0.0 { "-" } else { "" }.to_owned());
    }
    render_text(&localized_code_profile(profile), system, value, code)
}

// Formula TEXT reads host punctuation in its code; cell_text supplies invariant tokens.
// Both call the same section parser and value renderer.
fn localized_code_profile(profile: &FormatProfile) -> FormatProfile {
    FormatProfile {
        format_code_decimal_token: profile.decimal_separator,
        format_code_group_token: profile.thousands_separator,
        format_code_token_policy: crate::locale::FormatCodeTokenPolicy::LocalizedExcel,
        ..*profile
    }
}

fn render_text(profile: &FormatProfile, system: DateSystem, value: f64, code: &str) -> Rendered {
    if !value.is_finite() {
        return Err(Failure::Value);
    }
    let sections = sections(code);
    let (section, signed_section) = choose(&sections, value)?;
    let (stripped, name_locale, locale_declared) = strip_tokens(&section)?;
    if stripped.trim().eq_ignore_ascii_case("general") {
        let shown = general(profile, if signed_section { value.abs() } else { value });
        return Ok(shown);
    }
    let magnitude_only = signed_section;
    if !locale_declared && measured_host(profile) && datetime::has_invalid_exponent(&stripped) {
        return Err(Failure::Value);
    }
    let code_decimal = profile
        .format_code_decimal_token
        .chars()
        .next()
        .unwrap_or('.');
    if datetime::looks_like_datetime_format(&stripped, code_decimal) {
        if value < 0.0 {
            return Err(Failure::Value);
        }
        if value >= 2_958_466.0 {
            return Err(Failure::Value);
        }
        if datetime::without_subseconds(&stripped, code_decimal)
            .chars()
            .any(|c| {
                matches!(c, 'b' | 'B' | '0')
                    || (matches!(c, 'e' | 'g' | 'G')
                        && name_locale != Some(crate::locale::LocaleId::JaJp)
                        && !(!locale_declared && measured_host(profile)))
            })
        {
            return Err(Failure::NotModelled);
        }
        return datetime::render_with_datetime_tokens(
            profile,
            system,
            value,
            &stripped,
            name_locale,
            !locale_declared,
        )
        .ok_or(Failure::NotModelled);
    }
    if mixed_fraction_under_space_grouping(&stripped, profile) {
        return Err(Failure::Value);
    }
    if let Some(layout) = fraction_layout(&stripped, profile)? {
        return layout.render(profile, value, !magnitude_only);
    }
    let mut items = items(&stripped, profile)?;
    if !items.iter().any(|i| matches!(i, Item::Digit(_))) {
        // Fn8 literal-only controls: a run of ordinary-space grouping tokens
        // writes one space. Numeric grouping and trailing scaling stay below.
        if measured_host(profile) && profile.format_code_group_token == " " {
            items.dedup_by(|a, b| *a == Item::Group && *b == Item::Group);
            for item in &mut items {
                if *item == Item::Group {
                    *item = Item::Literal(" ".to_owned());
                }
            }
        }
        // A section of literals alone writes them (an empty one writes nothing).
        if items
            .iter()
            .any(|i| matches!(i, Item::Point | Item::Group | Item::Exponent(_)))
        {
            return Err(Failure::NotModelled);
        }
        if items.iter().any(|i| matches!(i, Item::TextPlaceholder)) {
            // Fn4 qt001..qt030: active @ and @! select General for numbers,
            // including values whose full numeric text has different precision.
            return Ok(general(
                profile,
                if signed_section { value.abs() } else { value },
            ));
        }
        let literal = items
            .iter()
            .map(|i| literal_text(i, profile))
            .collect::<Result<String, Failure>>()?;
        // Keep the ordinary automatic sign when section selection has not
        // suppressed it, even for quoted text (fn4 qt008). Empty stays empty.
        // An integer code without a zero placeholder rounds before deciding
        // whether to show its automatic sign. Quoted literals do not (Excel
        // 16.0.20527: TEXT(-0.1,"365") is "365", but TEXT(-0.1,"\"abc\"") is "-abc").
        let rounded_digit_zero = integer_code(&stripped) && digits(value, 0).0.is_empty();
        return Ok(
            if value < 0.0 && !signed_section && !literal.is_empty() && !rounded_digit_zero {
                format!("-{literal}")
            } else {
                literal
            },
        );
    }
    if items.iter().any(|i| matches!(i, Item::TextPlaceholder)) {
        return Err(Failure::NotModelled);
    }
    numeric(profile, value, &items, !magnitude_only)
}

/// TEXT of a text value under `code`: the fourth section, or a first section holding `@`, with
/// `@` written as the text; otherwise the text as it is.
pub(crate) fn text_of_text(text: &str, code: &str, profile: &FormatProfile) -> Rendered {
    render_text_of_text(text, code, &localized_code_profile(profile))
}
fn render_text_of_text(text: &str, code: &str, profile: &FormatProfile) -> Rendered {
    let sections = sections(code);
    let section = match sections.get(3) {
        Some(section) => section.clone(),
        None if sections.len() == 1 && sections[0].contains('@') => sections[0].clone(),
        None => return Ok(text.to_owned()),
    };
    let (stripped, _, _) = strip_tokens(&section)?;
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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn stored_cell_formats_keep_invariant_tokens_under_localized_formula_separators() {
        let value = crate::value::CalcValue::number(1234567.891);
        assert_eq!(
            cell_text(&host(), DateSystem::System1900, &value, "#,##0.00"),
            Ok("1 234 567.89".into())
        );
        assert_eq!(t(1234567.891, "#,##0"), Ok("1234,568".into()));
        let german = format_profile(LocaleId::DeDe)
            .with_separators(",", ".")
            .unwrap();
        assert_eq!(
            cell_text(&german, DateSystem::System1900, &value, "#,##0.00"),
            Ok("1.234.567,89".into())
        );
        assert_eq!(
            cell_text(
                &german,
                DateSystem::System1900,
                &crate::value::CalcValue::number(45234.567),
                "dd.mm.yyyy"
            ),
            Ok("04.11.2023".into())
        );
        assert_eq!(
            cell_text(
                &german,
                DateSystem::System1900,
                &crate::value::CalcValue::number(12.5),
                "\"v1.2,3 \"0.00"
            ),
            Ok("v1.2,3 12,50".into())
        );
    }

    /// Fraction codes as Excel 16.0.20430 showed them (`Range.Text` of a cell, en-US separators;
    /// apps/dna-onecalc/tests/fixtures/formats holds all 10,000 rows, replayed by that crate).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn fractions_render_as_excel_did() {
        let profile = format_profile(LocaleId::EnUs);
        let cell = |value: f64, code: &str| {
            cell_text(
                &profile,
                DateSystem::System1900,
                &crate::value::CalcValue::number(value),
                code,
            )
        };
        let rows: &[(f64, &str, &str)] = &[
            // The fraction alone: improper, the last convergent within the denominator's digits.
            (1234.5678, "?/?", "8642/7"),
            (1234.5678, "??/??", "100000/81"),
            (0.99, "??/??", "98/99"),
            (0.9, "?/?", "8/9"),
            (0.1, "?/?", "0/1"),
            (0.7, "?/?", "2/3"),
            (-0.004, "?/?", "0/1"),
            (-0.004, "???/???", "-  1/250"),
            (0.5, "??/??", " 1/2 "),
            (0.285, "?/100", "29/100"),
            (1.005, "?/100", "101/100"),
            (1.5, "0/4", "6/4"),
            (0.0625, "00/00", "01/16"),
            (0.3333333, "?/8", "3/8"),
            (0.3333333, "?/?%", "100/3%"),
            // An integer part.
            (1234.5678, "# ?/?", "1234 4/7"),
            (1234.5678, "# ??/??", "1234 46/81"),
            (-1234.5, "# ??/??", "-1234  1/2 "),
            (0.5, "# ?/?", " 1/2"),
            (0.5, "# ??/??", "  1/2 "),
            (0.5, "# #/#", "1/2"),
            (0.5, "0 ?/?", "0 1/2"),
            (0.5, "? ?/?", "  1/2"),
            (2.999, "# ?/?", "3    "),
            (0.99, "# ?/?", "1    "),
            (0.99, "# ??/??", " 98/99"),
            (0.0, "# ?/?", "0    "),
            (0.0, "# #/#", "0"),
            (0.0, "# ??/16", "0      "),
            (0.0, "# 0/0", "0/1"),
            (1.0, "# 0/0", "1 0/1"),
            (0.0, "0 0/0", "0 0/1"),
            (0.000012345, "# ?????/?????", "     1/81004"),
            (-0.004, "# ?/?", "-0    "),
            (-2.75, "# ?/?", "-2 3/4"),
            (1234.5678, "# ?/2", "1234 1/2"),
            (-2.75, "# ?/2", "-3    "),
            (0.5, "# ?/4", " 2/4"),
            (0.999999, "# ?/100", "1      "),
            (2.675, "# ?/100", "2 68/100"),
            (1E+21, "# ?/?", "1000000000000000000000    "),
            (1234.5678, "#,##0 ?/?", "1,234 4/7"),
            (1E+21, "#,##0 ?/?", "1,000,000,000,000,000,000,000    "),
            (0.5, "0000 ?/?", "0000 1/2"),
            (2.999, "00 ?/?", "03    "),
            // Literal text, and the text between the integer part and the numerator.
            (1234.5678, "\"about \"# ?/?", "about 1234 4/7"),
            (-1234.5, "\"about \"# ?/?", "-about 1234 1/2"),
            (1.0, "# ?/?\"x\"", "1    x"),
            (0.5, "#-?/?", " 1/2"),
            (1.5, "#-?/?", "1-1/2"),
            (0.5, "0-?/?", "0-1/2"),
            (1.0, "# \"-\" ?/?", "1      "),
            (0.5, "# \"-\" ?/?", "   1/2"),
            (0.5, "# #?/?#", "1/2"),
            (0.0, "# #?/?#", "0    "),
            (0.5, "# ?0/?", "  1/2"),
            (0.0, "# ?0/?", "0  0/1"),
            (0.5, "$# ?/?", "$ 1/2"),
            (0.5, "# ?/?%", "50    %"),
            (-0.004, "# ?/?%", "- 2/5%"),
            // Sections.
            (-1234.5, "# ?/?;(# ?/?)", "(1234 1/2)"),
            (-0.004, "# ?/?;(# ?/?)", "(0    )"),
            (-0.5, "# ?/?;@", "- 1/2"),
            (0.0, "# ?/?;# ?/?;\"zero\"", "zero"),
            (-1234.5, "[<1]?/?;# ?/?", "-2469/2"),
        ];
        for (value, code, expected) in rows {
            assert_eq!(
                cell(*value, code),
                Ok((*expected).to_owned()),
                "{value} under {code}"
            );
        }
        // A cell Excel fills with `#`, which TEXT gives as #VALUE!.
        for (value, code) in [
            (2147483647.0, "?/?"),
            (1E+10, "??/??"),
            (1234.5678, "?/100"),
            (123456789.125, "?/8"),
            (32767.0, "?/64"),
        ] {
            assert_eq!(
                cell(value, code),
                Err(CellFormatError::Unshowable),
                "{value} under {code}"
            );
        }
        // Codes Excel refuses to store: TEXT gives #VALUE!.
        for code in ["0.0 ?/?", "# ?/? ?/?", "# ?/?E+0", "#,##0.00 ?/?"] {
            assert_eq!(cell(1.5, code), Err(CellFormatError::Unshowable), "{code}");
        }
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
            (
                45000.0,
                "dddd, mmmm d, yyyy",
                ok("Wednesday, March 15, 2023"),
            ),
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
        assert_eq!(fixed(Some(&p), 1234.567, 2, false).unwrap(), "1 234.57");
        assert_eq!(fixed(Some(&p), -1234.567, 2, false).unwrap(), "-1 234.57");
        assert_eq!(fixed(Some(&p), 1234.567, -2, false).unwrap(), "1 200");
        assert_eq!(fixed(Some(&p), 1234.567, 1, true).unwrap(), "1234.6");
        assert_eq!(fixed(Some(&p), 0.5, 0, false).unwrap(), "1");
        assert_eq!(fixed(Some(&p), 1.005, 2, false).unwrap(), "1.01");
        assert_eq!(
            fixed(Some(&p), 1E+20, 2, false).unwrap(),
            "100 000 000 000 000 000 000.00"
        );
        assert_eq!(dollar(&p, 1234.567, 2).unwrap(), "R1 234.57");
        assert_eq!(dollar(&p, -1234.567, 2).unwrap(), "-R1 234.57");
        assert_eq!(dollar(&p, 1234.567, -2).unwrap(), "R1 200");
        assert_eq!(dollar(&p, 0.5, 0).unwrap(), "R1");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_section_names_the_color_of_the_value_it_shows() {
        use crate::value::CalcValue;
        let code = "#,##0;[Red]-#,##0;[Blue]0;[Green]@";
        assert_eq!(cell_color(code, &CalcValue::number(5.0)), None);
        assert_eq!(
            cell_color(code, &CalcValue::number(-5.0)),
            Some(FormatColor::Red)
        );
        assert_eq!(
            cell_color(code, &CalcValue::number(0.0)),
            Some(FormatColor::Blue)
        );
        assert_eq!(
            cell_color(
                code,
                &CalcValue::text(crate::value::ExcelText::from_utf16_code_units(
                    "x".encode_utf16().collect()
                ))
            ),
            Some(FormatColor::Green)
        );
        assert_eq!(
            cell_color("[>100][Magenta]0;0", &CalcValue::number(500.0)),
            Some(FormatColor::Magenta)
        );
        assert_eq!(FormatColor::Red.rgb(), "#ff0000");
        assert_eq!(
            cell_color("0;[Color10]-0", &CalcValue::number(-5.0)),
            Some(FormatColor::Palette(10))
        );
        assert_eq!(FormatColor::Palette(10).rgb(), "#008000");
        assert_eq!(FormatColor::Palette(45).rgb(), "#ff9900");
        assert_eq!(FormatColor::Palette(53).rgb(), "#993300");
        assert_eq!(cell_color("[Color57]0", &CalcValue::number(1.0)), None);
    }
}
