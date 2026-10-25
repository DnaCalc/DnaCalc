//! Date and time number formats, ported from OxFml (crates/oxfml_core/src/format/datetime.rs at
//! 15daac5): the tokens y, m, d, h, s, AM/PM and the elapsed [h], [m] and [s], with m read as
//! minutes beside an hour or a second. AM/PM in either case writes the locale's designators
//! (Excel on this machine, Windows locale en-ZA, wrote `pm` for `hh:mm AM/PM`).

use crate::locale::{DateSystem as WorkbookDateSystem, FormatProfile, ymd_from_excel_serial};

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DateTimePart {
    Literal(String),
    Token {
        kind: DateTimeTokenKind,
        width: usize,
    },
}

pub(crate) fn looks_like_datetime_format(section: &str) -> bool {
    let parts = tokenize_datetime_format(section);
    let has_datetime_token = parts.iter().any(|part| match part {
        DateTimePart::Token { .. } => true,
        DateTimePart::Literal(_) => false,
    });
    has_datetime_token
        && !section.contains('#')
        && !without_subseconds(section).contains('0')
        && !section.contains('?')
}

/// The section with the decimal places of a seconds token (`s.0`, `ss.00`, `[ss].000`) removed:
/// the zeros left over belong to no date or time token.
pub(crate) fn without_subseconds(section: &str) -> String {
    let chars: Vec<char> = section.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        out.push(chars[index]);
        let after_second = matches!(chars[index], 's' | 'S' | ']')
            && (chars[index] != ']' || out.to_ascii_lowercase().ends_with("s]"));
        if after_second && chars.get(index + 1) == Some(&'.') && chars.get(index + 2) == Some(&'0') {
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
) -> Option<String> {
    if !value.is_finite() {
        return None;
    }

    let parts = classify_minute_tokens(tokenize_datetime_format(section));
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
                kind: DateTimeTokenKind::Year | DateTimeTokenKind::Month | DateTimeTokenKind::Day,
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
    let weekday_index = date.map(|(year, month, day)| weekday_from_ymd(year, month, day));

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
                            3 => month_name(profile, month, true).to_string(),
                            _ => month_name(profile, month, false).to_string(),
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
                            3 => weekday_name(profile, weekday_index?, true).to_string(),
                            _ => weekday_name(profile, weekday_index?, false).to_string(),
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
                        format!(".{:0width$}", time.subsecond)
                    }
                    DateTimeTokenKind::AmPmUpper => {
                        super::designator(profile, time.hour < 12).to_string()
                    }
                    DateTimeTokenKind::AmPmLower => {
                        super::designator(profile, time.hour < 12).to_string()
                    }
                };
                rendered.push_str(&fragment);
            }
        }
    }

    Some(rendered)
}

fn tokenize_datetime_format(section: &str) -> Vec<DateTimePart> {
    let chars: Vec<char> = section.chars().collect();
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut index = 0;
    let mut in_quotes = false;

    while index < chars.len() {
        let ch = chars[index];
        if ch == '"' {
            in_quotes = !in_quotes;
            index += 1;
            continue;
        }
        if ch == '\\' {
            if let Some(next) = chars.get(index + 1) {
                literal.push(*next);
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if in_quotes {
            literal.push(ch);
            index += 1;
            continue;
        }

        if let Some((kind, width, consumed)) = elapsed_token_at(&chars, index) {
            push_literal(&mut parts, &mut literal);
            parts.push(DateTimePart::Token { kind, width });
            index += consumed;
            continue;
        }

        if ch == '.'
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
            _ => None,
        };

        if let Some(kind) = kind {
            let width = chars[index..]
                .iter()
                .take_while(|candidate| candidate.to_ascii_lowercase() == lower)
                .count();
            push_literal(&mut parts, &mut literal);
            parts.push(DateTimePart::Token { kind, width });
            index += width;
        } else {
            literal.push(ch);
            index += 1;
        }
    }

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

fn weekday_from_ymd(year: i64, month: i64, day: i64) -> usize {
    let mut month = month;
    let mut year = year;
    if month < 3 {
        month += 12;
        year -= 1;
    }
    let k = year.rem_euclid(100);
    let j = year.div_euclid(100);
    let h =
        (day + (13 * (month + 1)).div_euclid(5) + k + k.div_euclid(4) + j.div_euclid(4) + 5 * j)
            .rem_euclid(7);
    match h {
        0 => 6,
        1 => 0,
        2 => 1,
        3 => 2,
        4 => 3,
        5 => 4,
        _ => 5,
    }
}
