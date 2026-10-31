//! Workbook search and selection arithmetic over the application's existing projections.
use crate::{cells, wire, workbook::Workbook};
use calc_host::value::{CellKey, CoreRef, GridArea};
use calc_host::{CellEdit, ProjectedValue, ProjectionRequest, Selection};
use std::collections::BTreeSet;

const SEARCH_CELLS: usize = 100_000;
const SEARCH_CHARACTERS: usize = 5_000_000;
const SEARCH_HITS: usize = 1000;

fn same(a: char, b: char, case: bool) -> bool {
    if case {
        a == b
    } else {
        a.to_lowercase().eq(b.to_lowercase())
    }
}
#[derive(Clone, Copy)]
enum Token {
    Star,
    Any,
    Char(char),
}
fn tokens(pattern: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        out.push(match c {
            '*' => Token::Star,
            '?' => Token::Any,
            '~' => Token::Char(chars.next().unwrap_or('~')),
            c => Token::Char(c),
        });
    }
    out
}
/// Excel's documented *, ? and ~ pattern syntax, with explicit case and whole-cell options.
pub fn matches(text: &str, needle: &str, case: bool, whole: bool, wildcards: bool) -> bool {
    if needle.is_empty() {
        return false;
    }
    if !wildcards {
        if case {
            return if whole {
                text == needle
            } else {
                text.contains(needle)
            };
        }
        let text = text.chars().collect::<Vec<_>>();
        let needle = needle.chars().collect::<Vec<_>>();
        return if whole {
            text.len() == needle.len() && text.iter().zip(&needle).all(|(a, b)| same(*a, *b, false))
        } else {
            text.windows(needle.len())
                .any(|w| w.iter().zip(&needle).all(|(a, b)| same(*a, *b, false)))
        };
    }
    let mut pattern = tokens(needle);
    if !whole {
        pattern.insert(0, Token::Star);
        pattern.push(Token::Star);
    }
    let text = text.chars().collect::<Vec<_>>();
    let (mut at, mut p, mut star, mut retry) = (0usize, 0usize, None, 0usize);
    while at < text.len() {
        if p < pattern.len()
            && match pattern[p] {
                Token::Any => true,
                Token::Char(c) => same(text[at], c, case),
                Token::Star => false,
            }
        {
            at += 1;
            p += 1;
        } else if p < pattern.len() && matches!(pattern[p], Token::Star) {
            star = Some(p);
            p += 1;
            retry = at;
        } else if let Some(previous) = star {
            retry += 1;
            at = retry;
            p = previous + 1;
        } else {
            return false;
        }
    }
    while p < pattern.len() && matches!(pattern[p], Token::Star) {
        p += 1;
    }
    p == pattern.len()
}
/// Literal replacements retain original UTF-8 boundaries, even when case folding expands a char.
pub fn replace_literal(
    text: &str,
    needle: &str,
    replacement: &str,
    case: bool,
    whole: bool,
) -> String {
    if !matches(text, needle, case, whole, false) {
        return text.into();
    }
    if whole {
        return replacement.into();
    }
    if case {
        return text.replace(needle, replacement);
    }
    let source = text.char_indices().collect::<Vec<_>>();
    let needle = needle.chars().collect::<Vec<_>>();
    let mut output = String::new();
    let (mut at, mut kept) = (0usize, 0usize);
    while at + needle.len() <= source.len() {
        if source[at..at + needle.len()]
            .iter()
            .zip(&needle)
            .all(|((_, a), b)| same(*a, *b, false))
        {
            let first = source[at].0;
            let last = source
                .get(at + needle.len())
                .map(|(i, _)| *i)
                .unwrap_or(text.len());
            output.push_str(&text[kept..first]);
            output.push_str(replacement);
            kept = last;
            at += needle.len();
        } else {
            at += 1;
        }
    }
    output.push_str(&text[kept..]);
    output
}

pub fn find(
    book: &Workbook,
    selection: &Selection,
    options: &wire::FindOptions,
) -> Result<wire::Found, String> {
    if options.text.is_empty() || options.text.chars().count() > 256 {
        return Err("Enter 1–256 search characters".into());
    }
    let host = book.host();
    let sheet = options.sheet.id();
    if host.model().sheet_by_id(sheet).is_none() {
        return Err("The worksheet is unavailable".into());
    }
    let mut candidates = Vec::new();
    let mut partial = false;
    let mut visited = 0usize;
    'sheets: for entry in host.model().sheets() {
        if options.scope != wire::FindScope::Workbook && entry.id() != sheet {
            continue;
        }
        let mut cells = BTreeSet::new();
        let areas = if options.formulas {
            host.input_cells(entry.id())
                .into_iter()
                .map(GridArea::cell)
                .collect()
        } else {
            host.occupied_areas(entry.id())
        };
        let areas = areas.into_iter().flat_map(|area| {
            if options.scope != wire::FindScope::Selection {
                return vec![area];
            }
            selection
                .areas
                .iter()
                .filter_map(|selected| {
                    if selected.sheet() != area.sheet() {
                        return None;
                    }
                    let first = CellKey::new(
                        area.sheet(),
                        area.first().row().max(selected.first().row()),
                        area.first().column().max(selected.first().column()),
                    )
                    .ok()?;
                    let last = CellKey::new(
                        area.sheet(),
                        area.last().row().min(selected.last().row()),
                        area.last().column().min(selected.last().column()),
                    )
                    .ok()?;
                    if first.row() > last.row() || first.column() > last.column() {
                        return None;
                    }
                    GridArea::new(first, last).ok()
                })
                .collect()
        });
        for area in areas {
            for row in area.first().row()..=area.last().row() {
                for col in area.first().column()..=area.last().column() {
                    visited += 1;
                    if visited > SEARCH_CELLS {
                        partial = true;
                        break;
                    }
                    let at = CellKey::new(entry.id(), row, col).unwrap();
                    cells.insert(at);
                    if cells.len() + candidates.len() > SEARCH_CELLS {
                        partial = true;
                        break;
                    }
                }
                if partial {
                    break;
                }
            }
            if partial {
                break;
            }
        }
        candidates.extend(cells.into_iter().take(SEARCH_CELLS - candidates.len()));
        if partial {
            break 'sheets;
        }
    }
    let page = calc_host::project(host, &ProjectionRequest { cells: candidates })
        .map_err(|e| e.to_string())?;
    let (mut rows, mut scanned, mut characters) = (Vec::new(), 0usize, 0usize);
    let pattern_work = options.text.chars().count().max(1);
    for cell in page.cells() {
        let shown = book.shown(cell);
        let source = cells::input_text(cell.input());
        let text = if options.formulas {
            &source
        } else {
            &shown.text
        };
        characters += text.chars().count().saturating_mul(pattern_work);
        if characters > SEARCH_CHARACTERS {
            partial = true;
            break;
        }
        scanned += 1;
        if matches(
            text,
            &options.text,
            options.case_sensitive,
            options.whole,
            options.wildcards,
        ) {
            if rows.len() == SEARCH_HITS {
                partial = true;
                break;
            }
            let at = cell.at();
            let label = dna_formula_ui::display::cell(&cells::WorkbookNames(host), at);
            let hidden = host
                .model()
                .sheet_by_id(at.sheet())
                .is_some_and(|s| s.visibility() != calc_host::SheetVisibility::Visible);
            rows.push(wire::FoundRow {
                at: wire::Key::of(at),
                label,
                source,
                display: shown.text,
                provenance: shown.provenance,
                hidden,
            });
        }
    }
    Ok(wire::Found {
        revision: host.revision().get(),
        rows,
        scanned: scanned as u64,
        partial,
    })
}

pub fn replace_edits(
    book: &Workbook,
    selection: &Selection,
    options: &wire::FindOptions,
    replacement: &str,
    only: Option<CellKey>,
) -> Result<Vec<CellEdit>, String> {
    if !options.formulas || options.wildcards {
        return Err("Replacement uses literal formulas/entries; turn off wildcards and choose Formulas & entries".into());
    }
    let found = find(book, selection, options)?;
    if found.partial {
        return Err(
            "Narrow the search before replacing; the result set exceeds the search limit".into(),
        );
    }
    let host = book.host();
    let mut edits = Vec::new();
    for row in found.rows {
        let at = row.at.cell().unwrap();
        if only.is_some_and(|only| only != at) {
            continue;
        }
        if host
            .loaded()
            .sheet(at.sheet())
            .is_some_and(|s| s.protected() || s.is_macro_sheet())
        {
            return Err(format!(
                "{} is on a protected or macro worksheet; nothing changed",
                row.label
            ));
        }
        let text = replace_literal(
            &row.source,
            &options.text,
            replacement,
            options.case_sensitive,
            options.whole,
        );
        if text == row.source {
            continue;
        }
        let input = calc_host::interpret_entry_in(&text, Some(&host.entry_environment()))
            .map_err(|e| e.to_string())?;
        if let Err(failure) = host.check_entry(at, input.as_ref()) {
            return Err(format!(
                "{}: {failure}. Enter this cell individually to choose a validation decision; nothing changed",
                row.label
            ));
        }
        edits.push(match input {
            Some(input) => CellEdit::Set { at, input },
            None => CellEdit::Clear {
                area: GridArea::cell(at),
            },
        });
    }
    Ok(edits)
}

pub fn selection_stats(
    book: &Workbook,
    selection: &Selection,
) -> Result<wire::SelectionStats, String> {
    if selection.areas.iter().map(|a| a.cell_count()).sum::<u64>() > 10_000 {
        return Err("Selection summary is limited to 10000 cells; select a smaller range".into());
    }
    let mut cells = BTreeSet::new();
    for area in &selection.areas {
        for row in area.first().row()..=area.last().row() {
            for col in area.first().column()..=area.last().column() {
                cells.insert(CellKey::new(area.sheet(), row, col).unwrap());
            }
        }
    }
    let page = calc_host::project(
        book.host(),
        &ProjectionRequest {
            cells: cells.into_iter().collect(),
        },
    )
    .map_err(|e| e.to_string())?;
    let mut out = wire::SelectionStats {
        selected: page.cells().len() as u64,
        ..Default::default()
    };
    let (mut sum, mut correction) = (0.0, 0.0);
    let mut min = None::<f64>;
    let mut max = None::<f64>;
    for cell in page.cells() {
        let value = match cell.value() {
            ProjectedValue::Literal(value) => {
                out.literal += 1;
                Some(value)
            }
            ProjectedValue::Calculated { value, .. } => {
                out.calculated += 1;
                Some(value)
            }
            ProjectedValue::FileCached(_) => {
                out.cached += 1;
                None
            }
            ProjectedValue::Stale(_) => {
                out.stale += 1;
                None
            }
            ProjectedValue::NotModeled { .. } => {
                out.refused += 1;
                None
            }
            ProjectedValue::Empty => {
                out.blank += 1;
                None
            }
        };
        let Some(value) = value else {
            continue;
        };
        match value.core_ref() {
            CoreRef::Empty => out.blank += 1,
            CoreRef::Text(t) if t.utf16_code_units().is_empty() => {
                out.count += 1;
                out.empty_text += 1;
            }
            CoreRef::Number(n) => {
                out.count += 1;
                out.numeric += 1;
                let y = n - correction;
                let next = sum + y;
                correction = (next - sum) - y;
                sum = next;
                min = Some(min.map_or(n, |m| m.min(n)));
                max = Some(max.map_or(n, |m| m.max(n)));
            }
            CoreRef::Error(_) => {
                out.count += 1;
                out.errors += 1;
            }
            _ => out.count += 1,
        }
    }
    let display = |n: f64| {
        if n.is_finite() {
            n.to_string()
        } else {
            "Overflow".into()
        }
    };
    out.sum = if out.numeric == 0 {
        "—".into()
    } else {
        display(sum)
    };
    out.average = if out.numeric == 0 {
        "—".into()
    } else {
        display(sum / out.numeric as f64)
    };
    out.min = min.map(display).unwrap_or_else(|| "—".into());
    out.max = max.map(display).unwrap_or_else(|| "—".into());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn documented_wildcards_and_literal_utf8_replacements() {
        assert!(matches("North 2026", "north ??26", false, true, true));
        assert!(!matches("North 2026", "north ??26", true, true, true));
        assert!(matches("sum*?~", "sum~*~?~~", true, true, true));
        assert!(matches("before x after", "x", true, false, true));
        assert!(!matches("before x after", "x", true, true, true));
        assert!(!matches("anything", "", false, false, false));
        assert_eq!(
            replace_literal("Été éTÉ", "été", "été!", false, false),
            "été! été!"
        );
        assert_eq!(replace_literal("İ x", "i", "q", false, false), "İ x");
        assert_eq!(replace_literal("aaaa", "aa", "b", true, false), "bb");
        assert_eq!(replace_literal("aaaa", "aa", "b", true, true), "aaaa");
    }
}
