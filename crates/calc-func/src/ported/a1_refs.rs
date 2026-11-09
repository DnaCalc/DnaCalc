//! A1 reference text, as ADDRESS reads a sheet name that could be a reference; ported from OxFunc
//! (crates/oxfunc_core/src/functions/a1_refs.rs at 6fca3518).

pub const EXCEL_MAX_ROWS: usize = 1_048_576;
pub const EXCEL_MAX_COLS: usize = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A1ReferenceNotation {
    Rect,
    WholeColumn,
    WholeRow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct A1Reference {
    pub prefix: Option<String>,
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
    pub notation: A1ReferenceNotation,
}

impl A1Reference {
    pub fn width(&self) -> usize {
        self.end_col - self.start_col + 1
    }

    pub fn height(&self) -> usize {
        self.end_row - self.start_row + 1
    }
}

fn split_prefix(target: &str) -> (Option<String>, &str) {
    if let Some(idx) = target.rfind('!') {
        let prefix = target[..idx].trim();
        let rest = target[idx + 1..].trim();
        if prefix.is_empty() {
            (None, rest)
        } else {
            (Some(prefix.to_string()), rest)
        }
    } else {
        (None, target.trim())
    }
}

fn parse_cell_token(token: &str) -> Option<(usize, usize)> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut letters = String::new();
    let mut digits = String::new();
    for ch in trimmed.chars() {
        if ch == '$' {
            continue;
        }
        if ch.is_ascii_alphabetic() && digits.is_empty() {
            letters.push(ch.to_ascii_uppercase());
            continue;
        }
        if ch.is_ascii_digit() {
            digits.push(ch);
            continue;
        }
        return None;
    }

    if letters.is_empty() || digits.is_empty() {
        return None;
    }

    let mut col = 0usize;
    for ch in letters.chars() {
        let value = usize::from((ch as u8) - b'A' + 1);
        col = col.checked_mul(26)?.checked_add(value)?;
    }

    let row = digits.parse::<usize>().ok()?;
    if row == 0 || col == 0 {
        return None;
    }

    Some((row, col))
}

fn parse_column_token(token: &str) -> Option<usize> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut letters = String::new();
    for ch in trimmed.chars() {
        if ch == '$' {
            continue;
        }
        if ch.is_ascii_alphabetic() {
            letters.push(ch.to_ascii_uppercase());
            continue;
        }
        return None;
    }

    if letters.is_empty() {
        return None;
    }

    let mut col = 0usize;
    for ch in letters.chars() {
        let value = usize::from((ch as u8) - b'A' + 1);
        col = col.checked_mul(26)?.checked_add(value)?;
    }

    if col == 0 || col > EXCEL_MAX_COLS {
        return None;
    }

    Some(col)
}

fn parse_row_token(token: &str) -> Option<usize> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }

    let digits: String = trimmed.chars().filter(|ch| *ch != '$').collect();
    if digits.is_empty() || !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let row = digits.parse::<usize>().ok()?;
    if row == 0 || row > EXCEL_MAX_ROWS {
        return None;
    }

    Some(row)
}

fn column_label(mut col: usize) -> Option<String> {
    if col == 0 {
        return None;
    }

    let mut chars = Vec::new();
    while col > 0 {
        let rem = (col - 1) % 26;
        chars.push(char::from(b'A' + u8::try_from(rem).ok()?));
        col = (col - 1) / 26;
    }
    chars.reverse();
    Some(chars.into_iter().collect())
}

pub fn parse_a1_reference(target: impl AsRef<str>) -> Option<A1Reference> {
    let (prefix, body) = split_prefix(target.as_ref());
    if body.is_empty() {
        return None;
    }

    let parts: Vec<&str> = body.split(':').collect();
    if parts.len() > 2 {
        return None;
    }

    let parsed = if let Some((start_row, start_col)) = parse_cell_token(parts[0]) {
        let (end_row, end_col) = if parts.len() == 2 {
            parse_cell_token(parts[1])?
        } else {
            (start_row, start_col)
        };
        A1Reference {
            prefix,
            start_row: start_row.min(end_row),
            start_col: start_col.min(end_col),
            end_row: start_row.max(end_row),
            end_col: start_col.max(end_col),
            notation: A1ReferenceNotation::Rect,
        }
    } else if let Some(start_col) = parse_column_token(parts[0]) {
        let end_col = if parts.len() == 2 {
            parse_column_token(parts[1])?
        } else {
            start_col
        };
        A1Reference {
            prefix,
            start_row: 1,
            start_col: start_col.min(end_col),
            end_row: EXCEL_MAX_ROWS,
            end_col: start_col.max(end_col),
            notation: A1ReferenceNotation::WholeColumn,
        }
    } else if let Some(start_row) = parse_row_token(parts[0]) {
        let end_row = if parts.len() == 2 {
            parse_row_token(parts[1])?
        } else {
            start_row
        };
        A1Reference {
            prefix,
            start_row: start_row.min(end_row),
            start_col: 1,
            end_row: start_row.max(end_row),
            end_col: EXCEL_MAX_COLS,
            notation: A1ReferenceNotation::WholeRow,
        }
    } else {
        return None;
    };

    Some(parsed)
}

fn infer_notation(reference: &A1Reference) -> A1ReferenceNotation {
    if reference.start_row == 1 && reference.end_row == EXCEL_MAX_ROWS {
        A1ReferenceNotation::WholeColumn
    } else if reference.start_col == 1 && reference.end_col == EXCEL_MAX_COLS {
        A1ReferenceNotation::WholeRow
    } else {
        A1ReferenceNotation::Rect
    }
}

pub fn format_relative_target(reference: &A1Reference) -> Option<String> {
    let body = match reference.notation {
        A1ReferenceNotation::Rect => {
            let start = format!(
                "{}{}",
                column_label(reference.start_col)?,
                reference.start_row
            );
            let end = format!("{}{}", column_label(reference.end_col)?, reference.end_row);
            if start == end {
                start
            } else {
                format!("{start}:{end}")
            }
        }
        A1ReferenceNotation::WholeColumn => {
            let start = column_label(reference.start_col)?;
            let end = column_label(reference.end_col)?;
            if start == end {
                format!("{start}:{end}")
            } else {
                format!("{start}:{end}")
            }
        }
        A1ReferenceNotation::WholeRow => {
            if reference.start_row == reference.end_row {
                format!("{}:{}", reference.start_row, reference.end_row)
            } else {
                format!("{}:{}", reference.start_row, reference.end_row)
            }
        }
    };

    Some(match &reference.prefix {
        Some(prefix) => format!("{prefix}!{body}"),
        None => body,
    })
}

pub fn format_absolute_address(reference: &A1Reference) -> Option<String> {
    let body = match reference.notation {
        A1ReferenceNotation::Rect => {
            let start = format!(
                "${}${}",
                column_label(reference.start_col)?,
                reference.start_row
            );
            let end = format!(
                "${}${}",
                column_label(reference.end_col)?,
                reference.end_row
            );
            if start == end {
                start
            } else {
                format!("{start}:{end}")
            }
        }
        A1ReferenceNotation::WholeColumn => {
            let start = format!("${}", column_label(reference.start_col)?);
            let end = format!("${}", column_label(reference.end_col)?);
            format!("{start}:{end}")
        }
        A1ReferenceNotation::WholeRow => format!("${}:${}", reference.start_row, reference.end_row),
    };

    Some(match &reference.prefix {
        Some(prefix) => format!("{prefix}!{body}"),
        None => body,
    })
}

pub fn offset_reference(
    base: &A1Reference,
    row_offset: i64,
    col_offset: i64,
    height: Option<usize>,
    width: Option<usize>,
) -> Option<A1Reference> {
    let top = i64::try_from(base.start_row)
        .ok()?
        .checked_add(row_offset)?;
    let left = i64::try_from(base.start_col)
        .ok()?
        .checked_add(col_offset)?;
    if top <= 0 || left <= 0 {
        return None;
    }

    let h = height.unwrap_or_else(|| base.height());
    let w = width.unwrap_or_else(|| base.width());
    if h == 0 || w == 0 {
        return None;
    }

    let start_row = usize::try_from(top).ok()?;
    let start_col = usize::try_from(left).ok()?;
    let end_row = start_row.checked_add(h.checked_sub(1)?)?;
    let end_col = start_col.checked_add(w.checked_sub(1)?)?;

    Some(A1Reference {
        prefix: base.prefix.clone(),
        start_row,
        start_col,
        end_row,
        end_col,
        notation: infer_notation(&A1Reference {
            prefix: base.prefix.clone(),
            start_row,
            start_col,
            end_row,
            end_col,
            notation: A1ReferenceNotation::Rect,
        }),
    })
}
