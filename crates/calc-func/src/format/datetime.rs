//! Date and time number formats, ported from OxFml (crates/oxfml_core/src/format/datetime.rs at
//! 15daac5): the tokens y, m, d, h, s, AM/PM and the elapsed [h], [m] and [s], with m read as
//! minutes beside an hour or a second. AM/PM in either case writes the locale's designators
//! (Excel on this machine, Windows locale en-ZA, wrote `pm` for `hh:mm AM/PM`).

use crate::locale::{DateSystem as WorkbookDateSystem, FormatProfile, LocaleId, ymd_from_excel_serial};

use super::names::{month_name, weekday_name};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateTimeTokenKind {
    Year,
    Month,
    Minute,
    Day,
    Hour,
    Second,
    ElapsedHour,
    ElapsedMinute,
    ElapsedSecond,
    /// The decimal places after a seconds token (`ss.00`), `width` of them.
    SubSecond,
    AmPmUpper,
    AmPmLower,
    Era,
    EraYear,
    /// An uppercase E without an exponent sign is invalid in the measured host grammar.
    UpperExponent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DateTimePart {
    Literal(String),
    Token {
        kind: DateTimeTokenKind,
        width: usize,
    },
}

pub(crate) fn has_invalid_exponent(section: &str) -> bool {
    section.contains('E') && tokenize_datetime_format(section, None).iter().any(|part| {
        matches!(part, DateTimePart::Token { kind: DateTimeTokenKind::UpperExponent, .. })
    })
}

pub(crate) fn looks_like_datetime_format(section: &str, decimal: char) -> bool {
    let parts = tokenize_datetime_format(section, None);
    let has_datetime_token = parts.iter().any(|part| match part {
        DateTimePart::Token { .. } => true,
        DateTimePart::Literal(_) => false,
    });
    has_datetime_token
        && !section.contains('#')
        && !without_subseconds(section, decimal).contains('0')
        && !section.contains('?')
}

/// The section with the decimal places of a seconds token (`s.0`, `ss.00`, `[ss].000`) removed:
/// the zeros left over belong to no date or time token.
pub(crate) fn without_subseconds(section: &str, decimal: char) -> String {
    let chars: Vec<char> = section.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        out.push(chars[index]);
        let after_second = matches!(chars[index], 's' | 'S' | ']')
            && (chars[index] != ']' || out.to_ascii_lowercase().ends_with("s]"));
        if after_second
            && matches!(chars.get(index + 1), Some(&c) if c == '.' || c == decimal)
            && chars.get(index + 2) == Some(&'0')
        {
            index += 2;
            while chars.get(index) == Some(&'0') {
                index += 1;
            }
            continue;
        }
        index += 1;
    }
    out
}

pub(crate) fn render_with_datetime_tokens(
    profile: &FormatProfile,
    date_system: WorkbookDateSystem,
    value: f64,
    section: &str,
    name_locale: Option<LocaleId>,
    untagged_locale: bool,
) -> Option<String> {
    if !value.is_finite() {
        return None;
    }

    let parts = classify_minute_tokens(tokenize_datetime_format(section, Some(profile)));
    if !parts
        .iter()
        .any(|part| matches!(part, DateTimePart::Token { .. }))
    {
        return None;
    }

    let has_date_tokens = parts.iter().any(|part| {
        matches!(
            part,
            DateTimePart::Token {
                kind: DateTimeTokenKind::Year | DateTimeTokenKind::Month | DateTimeTokenKind::Day
                    | DateTimeTokenKind::Era | DateTimeTokenKind::EraYear | DateTimeTokenKind::UpperExponent,
                ..
            }
        )
    });
    let has_ampm = parts.iter().any(|part| {
        matches!(
            part,
            DateTimePart::Token {
                kind: DateTimeTokenKind::AmPmUpper | DateTimeTokenKind::AmPmLower,
                ..
            }
        )
    });
    // The time is rounded to the places of a sub-second token, when the section has one (Excel
    // 16.0.20430, dc-uar: TEXT(9000.79348,"mm:ss.00") is 02:36.67).
    let places = parts
        .iter()
        .find_map(|part| match part {
            DateTimePart::Token {
                kind: DateTimeTokenKind::SubSecond,
                width,
            } => Some(*width),
            _ => None,
        })
        .unwrap_or(0);
    if places > 3 {
        return None;
    }
    let time = TimeParts::from_serial(value, places as u32);
    // The date is the day the rounded time falls in: a time that rounds up to midnight shows the
    // next day (Excel 16.0.20430, dc-uar: TEXT(0.99999999,"yyyy-mm-dd hh:mm:ss.00") is
    // 1900-01-01 00:00:00.00).
    let date = if has_date_tokens {
        Some(ymd_from_excel_serial(
            date_system,
            time.elapsed_seconds.div_euclid(86_400) as f64,
        )?)
    } else {
        None
    };
    // TEXT follows the same serial calendar as WEEKDAY, including fictitious
    // 1900-02-29. Use the day after the existing time rounding, not Gregorian y/m/d.
    let weekday_index = date.and_then(|_| {
        let serial = time.elapsed_seconds.div_euclid(86_400) as f64
            + if date_system == WorkbookDateSystem::System1904 {
                crate::ported::date_kernels::OFFSET_1904
            } else {
                0.0
            };
        crate::ported::date_kernels::weekday(serial, None)
            .ok()
            .map(|day| day as usize - 1)
    });
    // A locale tag selects names, while separators and other formatting fields remain those
    // of the workbook profile. Fn1 measured German, French, Japanese and Russian names.
    let names_profile = FormatProfile { id: name_locale.unwrap_or(profile.id), ..*profile };

    let mut rendered = String::new();
    for part in parts {
        match part {
            DateTimePart::Literal(text) => {
                rendered.push_str(&render_datetime_literal(profile, &text))
            }
            DateTimePart::Token { kind, width } => {
                let fragment = match kind {
                    DateTimeTokenKind::Year => {
                        let (year, _, _) = date?;
                        if width <= 2 {
                            format!("{:02}", year.rem_euclid(100))
                        } else {
                            format!("{year:04}")
                        }
                    }
                    DateTimeTokenKind::Month => {
                        let (_, month, _) = date?;
                        match width {
                            1 => month.to_string(),
                            2 => format!("{month:02}"),
                            3 => month_name(&names_profile, month, true, name_locale.is_some()),
                            // mmmmm is the month name's first letter (measured, Excel 16.0 build
                            // 20527, dc-0f1k formats oracle).
                            5 => month_name(&names_profile, month, false, name_locale.is_some())
                                .chars()
                                .next()
                                .map(String::from)
                                .unwrap_or_default(),
                            _ => month_name(&names_profile, month, false, name_locale.is_some()),
                        }
                    }
                    DateTimeTokenKind::Minute => {
                        if width <= 1 {
                            time.minute.to_string()
                        } else {
                            format!("{:02}", time.minute)
                        }
                    }
                    DateTimeTokenKind::Day => {
                        let (_, _, day) = date?;
                        match width {
                            1 => day.to_string(),
                            2 => format!("{day:02}"),
                            3 => weekday_name(&names_profile, weekday_index?, true, name_locale.is_some()),
                            _ => weekday_name(&names_profile, weekday_index?, false, name_locale.is_some()),
                        }
                    }
                    DateTimeTokenKind::Hour => {
                        let hour = if has_ampm {
                            let hour = time.hour % 12;
                            if hour == 0 { 12 } else { hour }
                        } else {
                            time.hour
                        };
                        if width <= 1 {
                            hour.to_string()
                        } else {
                            format!("{hour:02}")
                        }
                    }
                    DateTimeTokenKind::Second => {
                        if width <= 1 {
                            time.second.to_string()
                        } else {
                            format!("{:02}", time.second)
                        }
                    }
                    DateTimeTokenKind::ElapsedHour => {
                        format_elapsed(value, time.elapsed_hours, width)
                    }
                    DateTimeTokenKind::ElapsedMinute => {
                        format_elapsed(value, time.elapsed_minutes, width)
                    }
                    DateTimeTokenKind::ElapsedSecond => {
                        format_elapsed(value, time.elapsed_seconds, width)
                    }
                    DateTimeTokenKind::SubSecond => {
                        format!("{}{:0width$}", profile.decimal_separator, time.subsecond)
                    }
                    DateTimeTokenKind::AmPmUpper => {
                        super::designator(profile, time.hour < 12).to_string()
                    }
                    DateTimeTokenKind::AmPmLower => {
                        super::designator(profile, time.hour < 12).to_string()
                    }
                    DateTimeTokenKind::Era | DateTimeTokenKind::EraYear | DateTimeTokenKind::UpperExponent => {
                        let (year, month, day) = date?;
                        // Fn5 zero section and fn8 width/era-boundary controls:
                        // an untagged e is Gregorian; g contributes no era name.
                        if untagged_locale && super::measured_host(profile) {
                            match kind {
                                DateTimeTokenKind::EraYear => { rendered.push_str(&year.to_string()); continue; }
                                DateTimeTokenKind::Era => continue,
                                _ => return None,
                            }
                        }
                        // Fn3 measures g/gg/ggg and e/ee on both sides of the
                        // Heisei/Reiwa transition. Calendar start dates are public facts:
                        // https://www.npb.go.jp/en/products/gazette/books/kanpo_history.html
                        if name_locale != Some(LocaleId::JaJp) { return None; }
                        let (base_year, latin, short, long) = if (year, month, day) >= (2019, 5, 1) {
                            (2018, "R", "令", "令和")
                        } else if (year, month, day) >= (1989, 1, 8) {
                            (1988, "H", "平", "平成")
                        } else {
                            return None;
                        };
                        match (kind, width) {
                            (DateTimeTokenKind::Era, 1) => latin.to_owned(),
                            (DateTimeTokenKind::Era, 2) => short.to_owned(),
                            (DateTimeTokenKind::Era, 3) => long.to_owned(),
                            (DateTimeTokenKind::EraYear | DateTimeTokenKind::UpperExponent, 1) => (year - base_year).to_string(),
                            (DateTimeTokenKind::EraYear | DateTimeTokenKind::UpperExponent, 2) => format!("{:02}", year - base_year),
                            _ => return None,
                        }
                    }
                };
                rendered.push_str(&fragment);
            }
        }
    }

    Some(rendered)
}

fn tokenize_datetime_format(section: &str, profile: Option<&FormatProfile>) -> Vec<DateTimePart> {
    let chars: Vec<char> = section.chars().collect();
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut index = 0;
    let mut in_quotes = false;
    let mut quoted_literal = String::new();
    let mut quoted_escape = false;
    // The character a code writes the decimal point as: `.`, or the host's in a localized code.
    let decimal = profile
        .and_then(|p| p.format_code_decimal_token.chars().next())
        .unwrap_or('.');

    while index < chars.len() {
        let ch = chars[index];
        if ch == '"' {
            if in_quotes {
                literal.push_str(if quoted_escape { &quoted_literal } else {
                    profile.map_or(quoted_literal.as_str(), |p| super::currency_literal(p, &quoted_literal))
                });
                quoted_escape = false;
                quoted_literal.clear();
            }
            in_quotes = !in_quotes;
            index += 1;
            continue;
        }
        if ch == '\\' {
            if let Some(next) = chars.get(index + 1) {
                if in_quotes { quoted_literal.push(*next); quoted_escape = true; } else { literal.push(*next); }
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if in_quotes {
            quoted_literal.push(ch);
            index += 1;
            continue;
        }

        if let Some((kind, width, consumed)) = elapsed_token_at(&chars, index) {
            push_literal(&mut parts, &mut literal);
            parts.push(DateTimePart::Token { kind, width });
            index += consumed;
            continue;
        }

        if (ch == '.' || ch == decimal)
            && chars.get(index + 1) == Some(&'0')
            && matches!(
                parts.last(),
                Some(DateTimePart::Token {
                    kind: DateTimeTokenKind::Second | DateTimeTokenKind::ElapsedSecond,
                    ..
                })
            )
            && literal.is_empty()
        {
            let width = chars[index + 1..].iter().take_while(|c| **c == '0').count();
            parts.push(DateTimePart::Token {
                kind: DateTimeTokenKind::SubSecond,
                width,
            });
            index += 1 + width;
            continue;
        }

        let remaining: String = chars[index..].iter().take(5).collect();
        if remaining.eq_ignore_ascii_case("am/pm") {
            push_literal(&mut parts, &mut literal);
            let kind = if remaining == "am/pm" {
                DateTimeTokenKind::AmPmLower
            } else {
                DateTimeTokenKind::AmPmUpper
            };
            parts.push(DateTimePart::Token { kind, width: 5 });
            index += 5;
            continue;
        }

        let lower = ch.to_ascii_lowercase();
        let kind = match lower {
            'y' => Some(DateTimeTokenKind::Year),
            'm' => Some(DateTimeTokenKind::Month),
            'd' => Some(DateTimeTokenKind::Day),
            'h' => Some(DateTimeTokenKind::Hour),
            's' => Some(DateTimeTokenKind::Second),
            'g' => Some(DateTimeTokenKind::Era),
            'e' if ch == 'E' && !matches!(chars.get(index + 1), Some('+' | '-')) => Some(DateTimeTokenKind::UpperExponent),
            'e' => Some(DateTimeTokenKind::EraYear),
            _ => None,
        };

        if let Some(kind) = kind {
            let width = chars[index..]
                .iter()
                .take_while(|candidate| candidate.to_ascii_lowercase() == lower)
                .count();
            // Mixed e/E runs are invalid in untagged host formats too.
            // Tagged Japanese rendering keeps its prior case-insensitive width.
            let kind = if lower == 'e' && chars[index..index + width].contains(&'E')
                && (width > 1 || !matches!(chars.get(index + width), Some('+' | '-'))) {
                DateTimeTokenKind::UpperExponent
            } else { kind };
            push_literal(&mut parts, &mut literal);
            parts.push(DateTimePart::Token { kind, width });
            index += width;
        } else {
            let atom = ch.to_string();
            literal.push_str(profile.map_or(atom.as_str(), |p| super::currency_literal(p, &atom)));
            index += 1;
        }
    }

    // Preserve the previous treatment of an unfinished quote.
    literal.push_str(&quoted_literal);
    push_literal(&mut parts, &mut literal);
    parts
}

fn elapsed_token_at(chars: &[char], index: usize) -> Option<(DateTimeTokenKind, usize, usize)> {
    if chars.get(index) != Some(&'[') {
        return None;
    }
    let close = chars[index + 1..].iter().position(|ch| *ch == ']')? + index + 1;
    let token: String = chars[index + 1..close].iter().collect();
    let lower = token.to_ascii_lowercase();
    if lower.is_empty() || !lower.chars().all(|ch| matches!(ch, 'h' | 'm' | 's')) {
        return None;
    }
    let first = lower.chars().next()?;
    if !lower.chars().all(|ch| ch == first) {
        return None;
    }
    let kind = match first {
        'h' => DateTimeTokenKind::ElapsedHour,
        'm' => DateTimeTokenKind::ElapsedMinute,
        's' => DateTimeTokenKind::ElapsedSecond,
        _ => return None,
    };
    Some((kind, lower.len(), close - index + 1))
}

fn classify_minute_tokens(parts: Vec<DateTimePart>) -> Vec<DateTimePart> {
    let token_indexes: Vec<usize> = parts
        .iter()
        .enumerate()
        .filter_map(|(index, part)| matches!(part, DateTimePart::Token { .. }).then_some(index))
        .collect();
    let mut classified = parts;

    for (token_position, part_index) in token_indexes.iter().enumerate() {
        let DateTimePart::Token { kind, width } = classified[*part_index].clone() else {
            continue;
        };
        if kind != DateTimeTokenKind::Month {
            continue;
        }
        let previous_kind = token_position
            .checked_sub(1)
            .and_then(|previous| token_indexes.get(previous))
            .and_then(|index| token_kind(&classified[*index]));
        let next_kind = token_indexes
            .get(token_position + 1)
            .and_then(|index| token_kind(&classified[*index]));
        if previous_kind.is_some_and(is_time_neighbor) || next_kind.is_some_and(is_time_neighbor) {
            classified[*part_index] = DateTimePart::Token {
                kind: DateTimeTokenKind::Minute,
                width,
            };
        }
    }

    classified
}

fn token_kind(part: &DateTimePart) -> Option<DateTimeTokenKind> {
    match part {
        DateTimePart::Token { kind, .. } => Some(*kind),
        DateTimePart::Literal(_) => None,
    }
}

fn is_time_neighbor(kind: DateTimeTokenKind) -> bool {
    matches!(
        kind,
        DateTimeTokenKind::Hour
            | DateTimeTokenKind::Second
            | DateTimeTokenKind::ElapsedHour
            | DateTimeTokenKind::ElapsedSecond
            | DateTimeTokenKind::AmPmUpper
            | DateTimeTokenKind::AmPmLower
    )
}

fn push_literal(parts: &mut Vec<DateTimePart>, literal: &mut String) {
    if !literal.is_empty() {
        parts.push(DateTimePart::Literal(std::mem::take(literal)));
    }
}

fn render_datetime_literal(profile: &FormatProfile, text: &str) -> String {
    text.replace('/', profile.date_separator)
        .replace(':', profile.time_separator)
}

fn format_elapsed(value: f64, magnitude: i64, width: usize) -> String {
    let prefix = if value.is_sign_negative() && magnitude != 0 {
        "-"
    } else {
        ""
    };
    let magnitude = magnitude.abs();
    if width <= 1 {
        format!("{prefix}{magnitude}")
    } else {
        format!("{prefix}{magnitude:0width$}")
    }
}

#[derive(Debug, Clone, Copy)]
struct TimeParts {
    hour: i64,
    minute: i64,
    second: i64,
    elapsed_hours: i64,
    elapsed_minutes: i64,
    elapsed_seconds: i64,
    /// The `places` decimal places of the seconds.
    subsecond: i64,
}

impl TimeParts {
    /// The time of day of a serial rounded to `places` decimal places of a second.
    fn from_serial(value: f64, places: u32) -> Self {
        let unit = 10i64.pow(places);
        let total_units = (value * 86_400.0 * unit as f64).round() as i64;
        let total_seconds = total_units.div_euclid(unit);
        let seconds_in_day = total_seconds.rem_euclid(86_400);
        Self {
            hour: seconds_in_day / 3_600,
            minute: (seconds_in_day % 3_600) / 60,
            second: seconds_in_day % 60,
            elapsed_hours: total_seconds / 3_600,
            elapsed_minutes: total_seconds / 60,
            elapsed_seconds: total_seconds,
            subsecond: total_units.rem_euclid(unit),
        }
    }
}
