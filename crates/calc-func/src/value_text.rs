//! Reading text as a number the way Excel's VALUE does under a locale: number text with the
//! locale's group and decimal separators, its currency symbol, a sign, parentheses and a percent
//! sign; a whole number and a fraction (`1 1/2`); and dates and times (`2024/01/05`, `1 jan
//! 2020`, `12:30 pm`), which read as serial date numbers.
//!
//! Ported from OxFml's locale value parser (crates/oxfml_core/src/format/engine.rs and number.rs
//! at 15daac5) and widened to the grammar Excel 16.0.20430 gave on this machine (Windows locale
//! en-ZA: `.` decimal, space as the group separator, `R` currency, year-month-day short dates),
//! read ad hoc through COM for VALUE over about 160 texts and in the capture of the committed
//! oracle workbook dc_e00t. The rules below state what those reads showed:
//!
//! - Number text: ASCII spaces around it and between its parts; at most one sign (`+`, `-`, or
//!   parentheses around the whole), one currency symbol before the digits (before or after the
//!   sign), one `%` before or after (also after closing parentheses); digits with the group
//!   separator, each group after the first at least three digits long; one decimal point; an
//!   exponent `E` with an optional sign. More
//!   than 15 significant digits are cut to 15 (`12345678901234567` reads as 1.23456789012345E+16).
//! - A whole number, spaces and a fraction `n/d` (`1 3/2` is 2.5); a zero denominator is #VALUE!.
//! - Dates: three numbers in the locale's short-date order separated by `/` or `-` (a two-digit
//!   year below 30 is 20xx, else 19xx); two numbers read first as month and day, then as day and
//!   month, then as year and month (day 1); a month name (full, three letters, or `sept`) with a
//!   day and a year (`1 jan 2020`, `1-jan-2020`, `1jan5`, `1 january, 2020`), with a day alone,
//!   or with a number that is a day of that month when it can be and a year otherwise (`jan 20`,
//!   `jan 2020`, `jan 32`). A date without a year is in the year of the calculation's clock.
//! - Times: `h:m`, `h:m:s` or `h:m:s.f` (minutes and seconds up to two digits, hours below
//!   10000, a trailing `:` allowed), or an hour alone with an AM/PM marker; the marker (`a`, `p`,
//!   `am`, `pm`, any case) follows a space and needs an hour from 0 to 12. A time may come
//!   before or after a date.
//!
//! Text this grammar does not read is #VALUE!, including unrecognized letters and symbols
//! (2016–17 Winter, with an en dash, is #VALUE! in dc_e00t's capture). Text holding a
//! digit of an unmeasured script or an unmeasured space beyond ASCII, text read under a profile whose separators or
//! currency lie beyond ASCII, a date without a year when the call has no clock, and number text
//! in another locale's date order (which no read measured) are refused as not modelled.
//! Fn8 measures a single euro currency token at either end of the amount in the
//! dot/space/Rand en-ZA/host profile. Rand stays prefix-only. Currency combined with a
//! percent is rejected by VALUE; NUMBERVALUE removes trailing percents before its host read.
//! Thin space joins the retained negative whitespace controls in that measured profile.
//! Fn4 accepts Arabic-Indic, extended Arabic-Indic, Devanagari and Thai decimal digits in
//! the measured ASCII en-ZA/host profiles, also mixed with ASCII digits. Fullwidth digits are
//! rejected there; unmeasured profiles retain a refusal for these scripts. Script-digit time
//! and fraction forms stay refused. In the measured en-ZA/host profiles, three-component
//! numeric dates require one/two-digit month and day tokens; a longer token is not a padded day.
//! The retained w111 numeric-text readings reject NBSP, em space, narrow NBSP and ideographic
//! space outside a profile's numeric grammar. A valid number using such a profile separator
//! remains refused: those positive forms have not been measured here.

use crate::locale::{DateComponentOrder, DateSystem, FormatProfile, LocaleId};
use crate::ported::date_kernels::{OFFSET_1904, days_in_month, ymd_1900};

/// Maximum text length in the measured scalar numeric readers.
/// Fn8 VALUE/NUMBERVALUE and fn11 DELTA/EXP/PMT measure the same boundary.
pub(crate) const MAX_READER_TEXT: usize = 255;

/// What text reads as.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Reading {
    /// Number text, a fraction or a percentage.
    Number(f64),
    /// A date, a time or both: the serial date number they make.
    DateTime(f64),
}

/// Why text gives no reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unread {
    /// The grammar does not read it: #VALUE!.
    Rejected,
    /// What Excel reads it as is not modelled.
    NotModelled,
}

/// The context text is read in.
pub(crate) struct ReadContext<'a> {
    pub(crate) profile: &'a FormatProfile,
    pub(crate) date_system: DateSystem,
    /// The calculation's clock, as a 1900-system serial, for dates without a year.
    pub(crate) now: Option<f64>,
}

impl Reading {
    pub(crate) fn value(self) -> f64 {
        match self {
            Reading::Number(n) | Reading::DateTime(n) => n,
        }
    }
}

/// The numeric-text host grammar measured by the fn8 scalar questions.
pub(crate) fn measured_host_reader(profile: &FormatProfile) -> bool {
    matches!(profile.id, LocaleId::EnZa | LocaleId::CurrentExcelHost)
        && profile.decimal_separator == "."
        && profile.thousands_separator == " "
        && profile.currency_symbol == "R"
}

/// Spaces with retained negative numeric-text controls. A configured positive
/// separator remains subject to the existing not-modelled treatment below.
pub(crate) fn measured_space(ch: char, profile: &FormatProfile) -> bool {
    matches!(ch, '\u{00a0}' | '\u{2003}' | '\u{202f}' | '\u{3000}')
        || (ch == '\u{2009}' && measured_host_reader(profile))
}

/// Reads `text` (UTF-16 code units).
pub(crate) fn read(units: &[u16], context: &ReadContext<'_>) -> Result<Reading, Unread> {
    let mut text = String::with_capacity(units.len());
    let ascii_profile = [
        context.profile.decimal_separator,
        context.profile.thousands_separator,
        context.profile.currency_symbol,
    ]
    .iter()
    .all(|s| s.is_ascii());
    let measured_digits = ascii_profile
        && matches!(
            context.profile.id,
            LocaleId::EnZa | LocaleId::CurrentExcelHost
        );
    let mut translated_digits = false;
    let mut beyond = false;
    let mut has_measured_space = false;
    for decoded in char::decode_utf16(units.iter().copied()) {
        match decoded {
            Ok(c) if c.is_ascii() => text.push(c),
            Ok('€') if measured_host_reader(context.profile) => text.push('€'),
            Ok(c) if measured_space(c, context.profile) => {
                text.push(c);
                has_measured_space = true;
            }
            // Fn4 qt031..qt096 accepts these four digit scripts in the measured profiles.
            // Fn7 live SQRT/DATEVALUE controls also confirm mixed-script numbers, exponent
            // digits and three-component dates; unmeasured fraction/time forms stay refused.
            Ok(
                c @ ('\u{0660}'..='\u{0669}'
                | '\u{06f0}'..='\u{06f9}'
                | '\u{0966}'..='\u{096f}'
                | '\u{0e50}'..='\u{0e59}'),
            ) if measured_digits => {
                translated_digits = true;
                let zero = match c {
                    '\u{0660}'..='\u{0669}' => 0x0660,
                    '\u{06f0}'..='\u{06f9}' => 0x06f0,
                    '\u{0966}'..='\u{096f}' => 0x0966,
                    _ => 0x0e50,
                };
                text.push(char::from_u32(u32::from('0') + u32::from(c) - zero).unwrap());
            }
            Ok('\u{ff10}'..='\u{ff19}') if measured_digits => return Err(Unread::Rejected),
            // A digit of another script, a space other than ASCII's, or a profile whose
            // separators or currency lie beyond ASCII: what Excel reads is not modelled.
            Ok(c) if c.is_numeric() || c.is_whitespace() || !ascii_profile => {
                return Err(Unread::NotModelled);
            }
            // Any other character beyond ASCII is in no number, date or time form.
            Ok(_) => beyond = true,
            Err(_) => return Err(Unread::NotModelled),
        }
    }
    if beyond {
        return Err(Unread::Rejected);
    }
    let text = text.trim_matches(' ');
    if text.is_empty() {
        return Err(Unread::Rejected);
    }
    if let Some(n) = number(text, context.profile)? {
        if has_measured_space {
            return Err(Unread::NotModelled);
        }
        return Ok(Reading::Number(n));
    }
    // Only ASCII spaces belong to the measured fraction/date/time grammar. The numeric
    // parser above can consume a configured separator; none of these spaces is trimming.
    if has_measured_space {
        return Err(Unread::Rejected);
    }
    if let Some(n) = fraction(text) {
        if translated_digits {
            return Err(Unread::NotModelled);
        }
        return n.map(Reading::Number);
    }
    date_time(text, context, !translated_digits).map(Reading::DateTime)
}

// --- numbers ------------------------------------------------------------------------------------

/// Number text, `None` when the text is not in the number form (it may be a date or a fraction).
fn number(text: &str, profile: &FormatProfile) -> Result<Option<f64>, Unread> {
    let bytes = text.as_bytes();
    let currency = profile.currency_symbol.as_bytes();
    let euro = "€".as_bytes();
    let currency_length = |at: usize| {
        if !currency.is_empty() && bytes[at..].starts_with(currency) {
            currency.len()
        } else if measured_host_reader(profile) && bytes[at..].starts_with(euro) {
            euro.len()
        } else {
            0
        }
    };
    let group = profile.thousands_separator.as_bytes();
    let point = profile.decimal_separator.as_bytes();
    let mut at = 0;
    let (mut signed, mut negative, mut paren, mut has_currency, mut percent) =
        (false, false, false, false, false);
    let skip = |at: &mut usize| {
        while bytes.get(*at) == Some(&b' ') {
            *at += 1;
        }
    };
    loop {
        skip(&mut at);
        match bytes.get(at) {
            Some(b'+' | b'-') if !signed && !paren => {
                signed = true;
                negative = bytes[at] == b'-';
                at += 1;
            }
            Some(b'(') if !signed && !paren => {
                paren = true;
                at += 1;
            }
            Some(b'%') if !percent => {
                percent = true;
                at += 1;
            }
            _ if !has_currency && currency_length(at) > 0 => {
                has_currency = true;
                at += currency_length(at);
            }
            _ => break,
        }
    }
    // The mantissa: digits with group separators, then a decimal point and digits.
    let mut digits = String::new();
    let mut fraction_digits = 0i64;
    let mut any = false;
    let mut group_run: Option<usize> = None;
    loop {
        match bytes.get(at) {
            Some(d) if d.is_ascii_digit() => {
                digits.push(char::from(*d));
                any = true;
                if let Some(run) = group_run.as_mut() {
                    *run += 1;
                }
                at += 1;
            }
            _ if any
                && !group.is_empty()
                && bytes[at..].starts_with(group)
                && bytes.get(at + group.len()).is_some_and(u8::is_ascii_digit) =>
            {
                if group_run.is_some_and(|run| run < 3) {
                    return Ok(None);
                }
                group_run = Some(0);
                at += group.len();
            }
            _ => break,
        }
    }
    if group_run.is_some_and(|run| run < 3) {
        return Ok(None);
    }
    if bytes[at..].starts_with(point) {
        at += point.len();
        while let Some(d) = bytes.get(at).filter(|d| d.is_ascii_digit()) {
            digits.push(char::from(*d));
            fraction_digits += 1;
            any = true;
            at += 1;
        }
    }
    if !any {
        return Ok(None);
    }
    let mut exponent = 0i64;
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        let mut cursor = at + 1;
        let exp_negative = match bytes.get(cursor) {
            Some(b'-') => {
                cursor += 1;
                true
            }
            Some(b'+') => {
                cursor += 1;
                false
            }
            _ => false,
        };
        let start = cursor;
        while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
            exponent = (exponent * 10 + i64::from(bytes[cursor] - b'0')).min(1_000_000);
            cursor += 1;
        }
        if cursor == start {
            return Ok(None);
        }
        if exp_negative {
            exponent = -exponent;
        }
        at = cursor;
    }
    // Euro may also follow the amount, inside or outside parentheses;
    // the configured Rand remains prefix-only (fn8 paired currency controls).
    let take_euro = |at: &mut usize, has_currency: &mut bool| {
        if !*has_currency && measured_host_reader(profile) && bytes[*at..].starts_with(euro) {
            *has_currency = true;
            *at += euro.len();
            skip(at);
        }
    };
    skip(&mut at);
    take_euro(&mut at, &mut has_currency);
    if bytes.get(at) == Some(&b'%') && !percent {
        percent = true;
        at += 1;
        skip(&mut at);
    }
    if paren {
        if bytes.get(at) != Some(&b')') {
            return Ok(None);
        }
        at += 1;
        negative = true;
        skip(&mut at);
    }
    take_euro(&mut at, &mut has_currency);
    // W111 numeric-text-core-heldout: the percent may follow the complete parenthesized
    // amount, with ASCII spaces between. The same flag rejects a second percent.
    if bytes.get(at) == Some(&b'%') && !percent {
        percent = true;
        at += 1;
        skip(&mut at);
    }
    if at != bytes.len() {
        return Ok(None);
    }
    if has_currency && percent && measured_host_reader(profile) {
        return Err(Unread::Rejected);
    }
    let scale = exponent - fraction_digits - if percent { 2 } else { 0 };
    // W111 percent-bounds: a zero coefficient still validates the percent-adjusted
    // exponent. decimal()'s zero fast return otherwise bypasses this admission rule.
    if percent && digits.bytes().all(|digit| digit == b'0') && !(-308..=308).contains(&scale) {
        return Err(Unread::Rejected);
    }
    decimal(&digits, scale)
        .map(|n| Some(if negative && n != 0.0 { -n } else { n }))
        .ok_or(Unread::Rejected)
}

/// The binary64 nearest `digits` (ASCII decimal digits) times 10^scale, its significant digits
/// cut to 15; `None` when the value's decimal point lies outside -308..=308, as Excel's number
/// text reading rejects it.
pub(crate) fn decimal(digits: &str, scale: i64) -> Option<f64> {
    let trimmed = digits.trim_start_matches('0');
    if trimmed.is_empty() {
        return Some(0.0);
    }
    let kept = &trimmed[..trimmed.len().min(15)];
    let dropped = (trimmed.len() - kept.len()) as i64;
    let position = scale + trimmed.len() as i64;
    if !(-308..=308).contains(&position) {
        return None;
    }
    let n: f64 = format!("{kept}e{}", scale + dropped).parse().ok()?;
    Some(if n.is_subnormal() { 0.0 } else { n })
}

/// A whole number and a fraction: `[sign] int spaces num/den`. `None` when the text is not in
/// this form; #VALUE! for a zero denominator.
fn fraction(text: &str) -> Option<Result<f64, Unread>> {
    let (negative, body) = match text.as_bytes().first() {
        Some(b'-') => (true, text[1..].trim_start_matches(' ')),
        Some(b'+') => (false, text[1..].trim_start_matches(' ')),
        _ => (false, text),
    };
    let (whole, rest) = body.split_once(' ')?;
    let rest = rest.trim_start_matches(' ');
    let (numerator, denominator) = rest.split_once('/')?;
    let all_digits =
        |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && s.len() <= 15;
    if !all_digits(whole) || !all_digits(numerator) || !all_digits(denominator) {
        return None;
    }
    let (w, n, d): (f64, f64, f64) = (
        whole.parse().ok()?,
        numerator.parse().ok()?,
        denominator.parse().ok()?,
    );
    if d == 0.0 {
        return Some(Err(Unread::Rejected));
    }
    let value = w + n / d;
    Some(Ok(if negative { -value } else { value }))
}

// --- dates and times ----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Token {
    /// A run of ASCII digits: its value and its length.
    Number(i64, usize),
    /// A run of ASCII letters, lower-cased.
    Word(String),
    /// A separator: `/`, `-`, `:`, `.`, `,` or a run of spaces (as `' '`).
    Sep(char),
}

fn tokens(text: &str) -> Result<Vec<Token>, Unread> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let b = bytes[at];
        if b.is_ascii_digit() {
            let start = at;
            while at < bytes.len() && bytes[at].is_ascii_digit() {
                at += 1;
            }
            let run = &text[start..at];
            if run.len() > 9 {
                return Err(Unread::Rejected);
            }
            out.push(Token::Number(run.parse().expect("digits"), run.len()));
        } else if b.is_ascii_alphabetic() {
            let start = at;
            while at < bytes.len() && bytes[at].is_ascii_alphabetic() {
                at += 1;
            }
            out.push(Token::Word(text[start..at].to_ascii_lowercase()));
        } else if b == b' ' {
            while at < bytes.len() && bytes[at] == b' ' {
                at += 1;
            }
            out.push(Token::Sep(' '));
        } else if matches!(b, b'/' | b'-' | b':' | b'.' | b',') {
            out.push(Token::Sep(char::from(b)));
            at += 1;
        } else {
            return Err(Unread::Rejected);
        }
    }
    Ok(out)
}

fn month_named(word: &str) -> Option<i64> {
    const NAMES: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    if word == "sept" {
        return Some(9);
    }
    NAMES
        .iter()
        .position(|name| *name == word || (word.len() == 3 && name.starts_with(word)))
        .map(|at| at as i64 + 1)
}

/// The time at the start of `tokens[from..]`, its fraction of a day and the tokens it took.
fn time_at(tokens: &[Token], from: usize) -> Option<(f64, usize)> {
    let mut at = from;
    let Token::Number(hour, hour_len) = tokens.get(at)? else {
        return None;
    };
    let (mut hour, hour_len) = (*hour, *hour_len);
    at += 1;
    let mut minute = 0i64;
    let mut second = 0f64;
    let mut colon = false;
    if tokens.get(at) == Some(&Token::Sep(':')) {
        colon = true;
        at += 1;
        if let Some(Token::Number(m, len)) = tokens.get(at) {
            if *len > 2 {
                return None;
            }
            minute = *m;
            at += 1;
            if tokens.get(at) == Some(&Token::Sep(':')) {
                at += 1;
                if let Some(Token::Number(s, len)) = tokens.get(at) {
                    if *len > 2 {
                        return None;
                    }
                    second = *s as f64;
                    at += 1;
                    if tokens.get(at) == Some(&Token::Sep('.'))
                        && let Some(Token::Number(f, len)) = tokens.get(at + 1)
                    {
                        second += *f as f64 / 10f64.powi(*len as i32);
                        at += 2;
                    }
                }
            }
        }
    }
    if hour_len > 4 {
        return None;
    }
    // An AM/PM marker after a space.
    let mut marked = false;
    if tokens.get(at) == Some(&Token::Sep(' '))
        && let Some(Token::Word(word)) = tokens.get(at + 1)
        && matches!(word.as_str(), "a" | "am" | "p" | "pm")
    {
        if hour > 12 {
            return None;
        }
        let pm = word.starts_with('p');
        if hour == 12 {
            hour = 0;
        }
        if pm {
            hour += 12;
        }
        marked = true;
        at += 2;
    }
    if !colon && !marked {
        return None;
    }
    let seconds = (hour * 3600 + minute * 60) as f64 + second;
    Some((seconds / 86_400.0, at - from))
}

/// A two-digit year as Excel reads it: below 30 in the 2000s, else in the 1900s.
fn full_year(year: i64, len: usize) -> i64 {
    if len <= 2 {
        if year < 30 { 2000 + year } else { 1900 + year }
    } else {
        year
    }
}

fn valid_day(year: i64, month: i64, day: i64) -> bool {
    (1..=12).contains(&month)
        && day >= 1
        && (day <= days_in_month(year, month) || (year == 1900 && month == 2 && day == 29))
}

/// The calculation's year, from its clock.
fn this_year(context: &ReadContext<'_>) -> Result<i64, Unread> {
    let now = context.now.ok_or(Unread::NotModelled)?;
    if !(0.0..2_958_466.0).contains(&now) {
        return Err(Unread::NotModelled);
    }
    Ok(ymd_1900(now.trunc() as i64).0)
}

/// The 1900 serial of a date, or #VALUE! outside 1900..=9999.
fn serial_1900(year: i64, month: i64, day: i64) -> Result<i64, Unread> {
    if !(1900..=9999).contains(&year) || !valid_day(year, month, day) {
        return Err(Unread::Rejected);
    }
    Ok(crate::ported::date_kernels::serial_unbounded_1900(
        year, month, day,
    ))
}

/// The date that `tokens` (without a time) name, as a 1900 serial.
fn date_of(tokens: &[Token], context: &ReadContext<'_>) -> Result<i64, Unread> {
    // The elements and the separators between them.
    let mut elements: Vec<&Token> = Vec::new();
    let mut separators: Vec<Option<char>> = Vec::new();
    let mut pending: Option<char> = None;
    let mut had_separator = false;
    for token in tokens {
        match token {
            Token::Sep(c) => {
                if had_separator && !(pending == Some(',') && *c == ' ') {
                    return Err(Unread::Rejected);
                }
                if elements.is_empty() {
                    return Err(Unread::Rejected);
                }
                pending = Some(match (pending, *c) {
                    (Some(','), ' ') => ',',
                    (_, c) => c,
                });
                had_separator = true;
            }
            _ => {
                if !elements.is_empty() {
                    separators.push(pending.take());
                }
                elements.push(token);
                had_separator = false;
                pending = None;
            }
        }
    }
    if had_separator || elements.is_empty() {
        return Err(Unread::Rejected);
    }
    let months: Vec<usize> = elements
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, Token::Word(_)))
        .map(|(i, _)| i)
        .collect();
    let number = |t: &Token| match t {
        Token::Number(n, len) => Some((*n, *len)),
        _ => None,
    };
    match months.as_slice() {
        [] => {
            // Numbers only, separated by / or -.
            if separators.iter().any(|s| !matches!(s, Some('/' | '-'))) {
                return Err(Unread::Rejected);
            }
            let nums: Vec<(i64, usize)> = elements.iter().filter_map(|t| number(t)).collect();
            match nums.as_slice() {
                [a, b, c] => {
                    let (y, m, d) = match context.profile.short_date_order {
                        DateComponentOrder::Ymd => (a, b, c),
                        DateComponentOrder::Mdy | DateComponentOrder::Dmy => {
                            return Err(Unread::NotModelled);
                        }
                    };
                    // Fn4 list-controls ql041..ql085: month/day tokens have one or two
                    // digits. A three/four-digit final token is not a day with leading zeros.
                    if matches!(
                        context.profile.id,
                        LocaleId::EnZa | LocaleId::CurrentExcelHost
                    ) && (m.1 > 2 || d.1 > 2)
                    {
                        return Err(Unread::Rejected);
                    }
                    serial_1900(full_year(y.0, y.1), m.0, d.0)
                }
                [a, b] => {
                    if context.profile.short_date_order != DateComponentOrder::Ymd {
                        return Err(Unread::NotModelled);
                    }
                    // Month and day, or day and month, of the clock's year (2024, a leap year,
                    // stands in to test them when there is no clock); else year and month.
                    let year = this_year(context);
                    let test_year = *year.as_ref().unwrap_or(&2024);
                    if valid_day(test_year, a.0, b.0) {
                        serial_1900(year?, a.0, b.0)
                    } else if valid_day(test_year, b.0, a.0) {
                        serial_1900(year?, b.0, a.0)
                    } else if (1..=12).contains(&b.0) {
                        serial_1900(full_year(a.0, a.1), b.0, 1)
                    } else {
                        Err(Unread::Rejected)
                    }
                }
                _ => Err(Unread::Rejected),
            }
        }
        [at] => {
            let Token::Word(word) = elements[*at] else {
                unreachable!()
            };
            let month = month_named(word).ok_or(Unread::Rejected)?;
            // A comma may follow the month name only.
            for (i, s) in separators.iter().enumerate() {
                if *s == Some(',') && i != *at {
                    return Err(Unread::Rejected);
                }
                if matches!(s, Some('/' | ':' | '.')) {
                    return Err(Unread::Rejected);
                }
            }
            let nums: Vec<(i64, usize)> = elements.iter().filter_map(|t| number(t)).collect();
            match (*at, nums.as_slice()) {
                // day month year
                (1, [d, y]) if elements.len() == 3 => serial_1900(full_year(y.0, y.1), month, d.0),
                // day month
                (1, [d]) if elements.len() == 2 => {
                    let year = this_year(context)?;
                    serial_1900(year, month, d.0)
                }
                // month day-or-year
                (0, [n]) if elements.len() == 2 => {
                    if separators[0] == Some(',') {
                        return Err(Unread::Rejected);
                    }
                    let year = this_year(context);
                    let test_year = *year.as_ref().unwrap_or(&2024);
                    if n.1 <= 2 && valid_day(test_year, month, n.0) {
                        serial_1900(year?, month, n.0)
                    } else {
                        serial_1900(full_year(n.0, n.1), month, 1)
                    }
                }
                _ => Err(Unread::Rejected),
            }
        }
        _ => Err(Unread::Rejected),
    }
}

fn date_time(text: &str, context: &ReadContext<'_>, allow_time: bool) -> Result<f64, Unread> {
    let tokens = tokens(text)?;
    // A time at the start, or after the last space.
    let mut time = None;
    let mut date_tokens: &[Token] = &tokens;
    if let Some((fraction, taken)) = time_at(&tokens, 0) {
        let rest = &tokens[taken..];
        let rest = match rest.first() {
            None => rest,
            Some(Token::Sep(' ')) => &rest[1..],
            _ => return Err(Unread::Rejected),
        };
        time = Some(fraction);
        date_tokens = rest;
    } else {
        for start in (1..tokens.len()).rev() {
            if tokens[start - 1] != Token::Sep(' ') {
                continue;
            }
            if let Some((fraction, taken)) = time_at(&tokens, start)
                && start + taken == tokens.len()
            {
                time = Some(fraction);
                date_tokens = &tokens[..start - 1];
                break;
            }
        }
    }
    // A trailing colon after the time ("12:" and "1:30:").
    if time.is_none()
        && let Some(Token::Sep(':')) = tokens.last()
        && let Some((fraction, taken)) = time_at(&tokens[..tokens.len() - 1], 0)
        && taken == tokens.len() - 1
    {
        time = Some(fraction);
        date_tokens = &[];
    }
    if time.is_none() && tokens.len() >= 2 {
        let trimmed = &tokens[..tokens.len() - 1];
        if tokens.last() == Some(&Token::Sep(':'))
            && let Some((fraction, taken)) = time_at(trimmed, 0)
            && taken == trimmed.len()
        {
            time = Some(fraction);
            date_tokens = &[];
        }
    }
    if time.is_some() && !allow_time {
        return Err(Unread::NotModelled);
    }
    let date = if date_tokens.is_empty() {
        None
    } else {
        Some(date_of(date_tokens, context)?)
    };
    if date.is_none() && time.is_none() {
        return Err(Unread::Rejected);
    }
    let serial = match (date, context.date_system) {
        (None, _) => 0.0,
        (Some(serial), DateSystem::System1900) => serial as f64,
        (Some(serial), DateSystem::System1904) => {
            let serial = serial as f64 - OFFSET_1904;
            if serial < 0.0 {
                return Err(Unread::Rejected);
            }
            serial
        }
    };
    Ok(match time {
        Some(fraction) if date.is_some() => serial + fraction,
        Some(fraction) => fraction,
        None => serial,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locale::{LocaleId, format_profile};

    /// Reads `text` under this machine's Excel settings (en-ZA with `.` and a space), on a clock
    /// in 2026 as the ad hoc reads were.
    fn host(text: &str) -> Result<f64, Unread> {
        let profile = format_profile(LocaleId::CurrentExcelHost);
        let context = ReadContext {
            profile: &profile,
            date_system: DateSystem::System1900,
            now: Some(46318.6),
        };
        read(&text.encode_utf16().collect::<Vec<_>>(), &context).map(Reading::value)
    }

    const V: Result<f64, Unread> = Err(Unread::Rejected);

    /// Every row is Excel's VALUE of the text on this machine (16.0.20430, en-ZA), read ad hoc.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn value_reads_as_excel_did() {
        let rows: &[(&str, Result<f64, Unread>)] = &[
            ("12", Ok(12.0)),
            ("-12", Ok(-12.0)),
            (" 12 ", Ok(12.0)),
            ("1 234.5", Ok(1234.5)),
            ("12 34", V),
            ("1,234", V),
            ("R12", Ok(12.0)),
            ("R 12", Ok(12.0)),
            ("-R12", Ok(-12.0)),
            ("R-12", Ok(-12.0)),
            ("(12)", Ok(-12.0)),
            ("(R12)", Ok(-12.0)),
            ("12%", Ok(0.12)),
            ("%12", Ok(0.12)),
            ("-12%", Ok(-0.12)),
            ("1.5E3", Ok(1500.0)),
            ("$12", V),
            ("12R", V),
            ("1 1/2", Ok(1.5)),
            ("-1 1/2", Ok(-1.5)),
            ("3/4", Ok(46085.0)),
            ("1/2", Ok(46024.0)),
            ("13/2", Ok(46066.0)),
            ("2024/01/05", Ok(45296.0)),
            ("2024-01-05", Ok(45296.0)),
            ("24/1/5", Ok(45296.0)),
            ("05/01/2024", V),
            ("2024/13/01", V),
            ("2024/02/30", V),
            ("2024/02/29", Ok(45351.0)),
            ("1900/02/29", Ok(60.0)),
            ("2024.01.05", V),
            ("12:30", Ok(0.52083333333333337)),
            ("12:30:15", Ok(0.5210069444444444)),
            ("12:30:15.5", Ok(0.52101273148148153)),
            ("25:00", Ok(1.0416666666666667)),
            ("12:60", Ok(0.54166666666666663)),
            ("1:2:3", Ok(0.043090277777777776)),
            ("12:30 pm", Ok(0.52083333333333337)),
            ("12 pm", Ok(0.5)),
            ("12pm", V),
            ("12:30PM", V),
            ("12:30 a", Ok(0.020833333333333332)),
            ("0:30 am", Ok(0.020833333333333332)),
            ("13:30 pm", V),
            ("2024/01/05 12:30", Ok(45296.520833333336)),
            ("12:30 2024/01/05", Ok(45296.520833333336)),
            ("1 jan 2020", Ok(43831.0)),
            ("1 jan", Ok(46023.0)),
            ("jan 2020", Ok(43831.0)),
            ("jan 20", Ok(46042.0)),
            ("jan-20", Ok(46042.0)),
            ("1-jan-2020", Ok(43831.0)),
            ("jan 1 2020", V),
            ("jan 1, 2020", V),
            ("1 january, 2020", Ok(43831.0)),
            ("2020 jan 1", V),
            ("1 jan 29", Ok(47119.0)),
            ("1 jan 30", Ok(10959.0)),
            ("Mon 1 jan 2020", V),
            ("1st jan 2020", V),
            ("2020/1", Ok(43831.0)),
            ("1/2020", V),
            ("2020/12", Ok(44166.0)),
            ("1/13", Ok(46035.0)),
            ("2/29", V),
            ("1:30:00 PM", Ok(0.5625)),
            ("TRUE", V),
            ("1e5", Ok(100000.0)),
            (".5", Ok(0.5)),
            ("5.", Ok(5.0)),
            ("1..5", V),
            ("-", V),
            ("1 000 000", Ok(1000000.0)),
            ("1 00", V),
            ("1 2345", Ok(12345.0)),
            ("R 1 234.50", Ok(1234.5)),
            ("- 12", Ok(-12.0)),
            ("12-", V),
            ("1 jan 2020 10:30", Ok(43831.4375)),
            ("2020-1-2 10:30", Ok(43832.4375)),
            ("2020-01-02T10:30", V),
            ("jan 32", Ok(11689.0)),
            ("feb 30", Ok(10990.0)),
            ("30/2", Ok(10990.0)),
            ("1/32", V),
            ("2024/1/5 25:00", Ok(45297.041666666664)),
            ("12:30 pm 1 jan 2020", Ok(43831.520833333336)),
            ("1 3/2", Ok(2.5)),
            ("1 1/0", V),
            ("12:99", Ok(0.56875)),
            ("100:00", Ok(4.166666666666667)),
            ("12345:00", V),
            ("1899/12/31", V),
            ("2024 01 05", V),
            ("2024/01-05", Ok(45296.0)),
            ("1 jan, 2020", Ok(43831.0)),
            ("jan, 2020", V),
            ("1,jan 2020", V),
            ("Jan-2020", Ok(43831.0)),
            ("2020-jan-1", V),
            ("1-13", Ok(46035.0)),
            ("13-1", Ok(46035.0)),
            ("2020-1", Ok(43831.0)),
            ("12345678901234567", Ok(1.23456789012345E+16)),
            ("+R12", Ok(12.0)),
            ("R+12", Ok(12.0)),
            ("(-12)", V),
            ("(12%)", Ok(-0.12)),
            ("12 %", Ok(0.12)),
            ("1.5 E3", V),
            ("R", V),
            ("1 0000", Ok(10000.0)),
            ("1 234 56", V),
            ("0.5 12:00", V),
            ("12:30:15 pm", Ok(0.5210069444444444)),
            ("1 jan 10000", V),
            ("29 feb 2023", V),
            ("1 dec 99 23:59:59", Ok(36495.999988425923)),
            ("12:", Ok(0.5)),
            (":30", V),
            ("1:30:", Ok(0.0625)),
            ("10:30 a", Ok(0.4375)),
            ("10:30 p", Ok(0.9375)),
            ("1jan5", Ok(38353.0)),
            ("jan1", Ok(46023.0)),
            ("1 1/2 a", V),
            ("10:30 a.m.", V),
            ("1 a 2020", V),
        ];
        for (text, expected) in rows {
            assert_eq!(host(text), *expected, "VALUE({text:?})");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_date_without_a_year_needs_a_clock() {
        let profile = format_profile(LocaleId::CurrentExcelHost);
        let context = ReadContext {
            profile: &profile,
            date_system: DateSystem::System1900,
            now: None,
        };
        let read_text = |text: &str| {
            read(&text.encode_utf16().collect::<Vec<_>>(), &context).map(Reading::value)
        };
        assert_eq!(read_text("1 jan"), Err(Unread::NotModelled));
        assert_eq!(read_text("1/2"), Err(Unread::NotModelled));
        // Forms that need no year are read without a clock.
        assert_eq!(read_text("jan 2020"), Ok(43831.0));
        assert_eq!(read_text("2016-17"), Err(Unread::Rejected));
        assert_eq!(read_text("2020/12"), Ok(44166.0));
        // Excel: "2016\u{2013}17 Winter"+0 is #VALUE! (dc_e00t's capture).
        assert_eq!(read_text("2016\u{2013}17 Winter"), Err(Unread::Rejected));
        assert_eq!(read_text("1\u{00A0}234"), Err(Unread::Rejected));
    }

    /// Scope control: neither an unmeasured space nor a potentially valid non-ASCII
    /// grouping separator becomes a calculated value from the rejection readings.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn unmeasured_unicode_number_forms_keep_their_refusal() {
        let profile = format_profile(LocaleId::EnZa);
        let context = ReadContext {
            profile: &profile,
            date_system: DateSystem::System1900,
            now: None,
        };
        for text in ["1\u{00a0}234,5", "1\u{2009}234", "\u{0661}"] {
            assert_eq!(
                read(&text.encode_utf16().collect::<Vec<_>>(), &context),
                Err(Unread::NotModelled),
                "{text:?}"
            );
        }
    }
}
