//! Bounded, example-based text extraction through the existing paste preview and history route.
use crate::value::{CellKey, CoreRef, ExcelText, GridArea};
use crate::{
    CellEdit, CellInput, CellText, HostState, PasteMode, ProjectedValue, ProjectionRequest,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
enum Extract {
    Whole,
    First,
    Last,
    Before(char),
    After(char),
}
#[derive(Clone, Copy)]
enum LetterCase {
    Keep,
    Upper,
    Lower,
}
#[derive(Clone)]
struct Pattern {
    extract: Extract,
    case: LetterCase,
    prefix: String,
    suffix: String,
}
fn extract(source: &str, kind: Extract, case: LetterCase) -> Option<String> {
    let source = source.trim();
    let text = match kind {
        Extract::Whole => source,
        Extract::First => source.split_whitespace().next()?,
        Extract::Last => source.split_whitespace().next_back()?,
        Extract::Before(c) => source.split_once(c)?.0.trim(),
        Extract::After(c) => source.rsplit_once(c)?.1.trim(),
    };
    if text.is_empty() {
        return None;
    }
    Some(match case {
        LetterCase::Keep => text.into(),
        LetterCase::Upper if text.is_ascii() => text.to_ascii_uppercase(),
        LetterCase::Lower if text.is_ascii() => text.to_ascii_lowercase(),
        _ => return None,
    })
}
impl Pattern {
    fn apply(&self, source: &str) -> Option<String> {
        Some(format!(
            "{}{}{}",
            self.prefix,
            extract(source, self.extract, self.case)?,
            self.suffix
        ))
    }
}
fn infer(examples: &[(String, String)]) -> Result<Vec<Pattern>, String> {
    let Some((source, example)) = examples.first() else {
        return Ok(Vec::new());
    };
    let mut kinds = vec![Extract::Whole, Extract::First, Extract::Last];
    for delimiter in ['@', ',', '-', '_', '.'] {
        kinds.push(Extract::Before(delimiter));
        kinds.push(Extract::After(delimiter));
    }
    let mut patterns = Vec::new();
    let mut work = 0usize;
    let limit = || {
        "Narrow the examples; Flash Fill exceeded its pattern search limit (dc-971p.47); nothing changed".to_owned()
    };
    for kind in kinds {
        for case in [LetterCase::Keep, LetterCase::Upper, LetterCase::Lower] {
            let Some(core) = extract(source, kind, case) else {
                continue;
            };
            for (at, _) in example.char_indices() {
                work += core.len();
                if work > 5_000_000 {
                    return Err(limit());
                }
                if !example[at..].starts_with(&core) {
                    continue;
                }
                let pattern = Pattern {
                    extract: kind,
                    case,
                    prefix: example[..at].into(),
                    suffix: example[at + core.len()..].into(),
                };
                let mut fits = true;
                for (source, expected) in examples {
                    work += source.len() + expected.len();
                    if work > 5_000_000 {
                        return Err(limit());
                    }
                    if pattern.apply(source).as_deref() != Some(expected) {
                        fits = false;
                        break;
                    }
                }
                if fits {
                    patterns.push(pattern);
                    if patterns.len() > 256 {
                        return Err(limit());
                    }
                }
            }
        }
    }
    Ok(patterns)
}
fn text(value: &ExcelText) -> Result<String, String> {
    String::from_utf16(value.utf16_code_units())
        .map_err(|_| "Flash Fill needs text with valid Unicode characters (dc-971p.47)".into())
}

pub(crate) fn prepare(host: &HostState, to: GridArea) -> Result<Vec<CellEdit>, String> {
    let refuse = |detail: &str| format!("{detail} (dc-971p.47); nothing changed");
    if to.column_count() != 1 || to.row_count() < 3 || to.first().column() == 1 {
        return Err(refuse(
            "Select one output column with two text examples and blank cells below; source text belongs immediately to its left",
        ));
    }
    if to.cell_count() > 10_000 {
        return Err(refuse(
            "Flash Fill currently accepts at most 10,000 selected rows",
        ));
    }
    crate::check_destination(host, to, PasteMode::Values).map_err(|e| e.to_string())?;
    let source_column = to.first().column() - 1;
    let cells = (to.first().row()..=to.last().row())
        .map(|row| CellKey::new(to.sheet(), row, source_column).unwrap())
        .collect();
    let page = crate::project(host, &ProjectionRequest { cells })
        .map_err(|_| refuse("The source cells are unavailable"))?;
    let mut sources = BTreeMap::new();
    let mut text_bytes = 0usize;
    for cell in page.cells() {
        let value = match cell.value() {
            ProjectedValue::Empty => None,
            ProjectedValue::Literal(v) | ProjectedValue::Calculated { value: v, .. } => {
                match v.core_ref() {
                    CoreRef::Empty => None,
                    CoreRef::Text(t) => {
                        let t = text(t)?;
                        (!t.is_empty()).then_some(t)
                    }
                    _ => return Err(refuse("Flash Fill currently uses text sources")),
                }
            }
            _ => {
                return Err(refuse(
                    "Calculate the source cells before Flash Fill; saved or stale values cannot be used",
                ));
            }
        };
        text_bytes += value.as_ref().map_or(0, String::len);
        if text_bytes > 1_000_000 {
            return Err(refuse(
                "Narrow the selection; Flash Fill accepts at most 1,000,000 source/example text bytes",
            ));
        }
        sources.insert(cell.at().row(), value);
    }
    let mut examples = Vec::new();
    let mut blanks = Vec::new();
    for row in to.first().row()..=to.last().row() {
        let at = CellKey::new(to.sheet(), row, to.first().column()).unwrap();
        let source = sources.get(&row).and_then(|s| s.as_ref());
        match host.model().cell(at) {
            Some(CellInput::Text(example)) => {
                let example = text(example.text())?;
                text_bytes += example.len();
                if text_bytes > 1_000_000 {
                    return Err(refuse(
                        "Narrow the selection; Flash Fill accepts at most 1,000,000 source/example text bytes",
                    ));
                }
                examples.push((
                    source
                        .ok_or_else(|| refuse("Every example needs source text to its left"))?
                        .clone(),
                    example,
                ));
            }
            Some(_) => {
                return Err(refuse(
                    "Keep text examples in the output column; formulas and numeric inputs are not replaced",
                ));
            }
            None => {
                if let Some(source) = source {
                    blanks.push((at, source));
                }
            }
        }
    }
    if examples.len() < 2 {
        return Err(refuse(
            "Enter at least two matching text examples in the selected output column",
        ));
    }
    if blanks.is_empty() {
        return Err(refuse(
            "There are no blank output cells with source text to fill",
        ));
    }
    let mut prediction: Option<Vec<String>> = None;
    for pattern in infer(&examples)? {
        let mut generated_bytes = 0usize;
        let mut candidate = Some(Vec::new());
        for (_, source) in &blanks {
            let Some(value) = pattern.apply(source) else {
                candidate = None;
                break;
            };
            generated_bytes += value.len();
            if generated_bytes > 1_000_000 {
                return Err(refuse(
                    "Narrow the selection; generated text exceeds 1,000,000 bytes",
                ));
            }
            candidate.as_mut().unwrap().push(value);
        }
        let Some(candidate) = candidate else {
            continue;
        };
        if prediction.as_ref().is_some_and(|old| *old != candidate) {
            return Err(refuse(
                "The examples allow different results; add another example to show the intended pattern",
            ));
        }
        prediction = Some(candidate);
    }
    let prediction = prediction.ok_or_else(|| {
        refuse(
            "No supported text pattern fits all examples and remaining rows; use first/last words, delimiter parts or fixed prefixes/suffixes",
        )
    })?;
    blanks
        .into_iter()
        .zip(prediction)
        .map(|((at, _), value)| {
            let input = CellText::new(ExcelText::from_utf16_code_units(
                value.encode_utf16().collect(),
            ))
            .ok_or_else(|| refuse("A generated value exceeds the cell text limit"))?;
            Ok(CellEdit::Set {
                at,
                input: CellInput::Text(input),
            })
        })
        .collect()
}
