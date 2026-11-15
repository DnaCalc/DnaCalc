//! The data caches Excel writes with the references of a session-made chart's series (dc-piw0.9):
//! `c:strCache` for a series name and for text categories, `c:numCache` with `General` for values,
//! filled from the cells at save time (a literal cell's input, a formula cell's supplied value).
use crate::load::address;
use calc_doc_model::value::{CellKey, SheetId};
use calc_doc_model::{CachedValue, CellInput, Chart, ChartData, ModelRoot};
use std::collections::BTreeMap;

/// What a save knows of the cells a chart reads.
pub(crate) struct CacheSource<'a> {
    pub model: &'a ModelRoot,
    /// The calculated values the caller supplied for formula cells; without them a formula cell's
    /// point is left out.
    pub values: Option<&'a BTreeMap<CellKey, CachedValue>>,
}

/// `chart` with the cache of every series reference that names cells of the workbook refreshed.
pub(crate) fn filled(chart: &Chart, source: &CacheSource<'_>) -> Chart {
    let mut chart = chart.clone();
    for plot in &mut chart.plots {
        for series in &mut plot.series {
            for data in [&mut series.name, &mut series.categories, &mut series.values]
                .into_iter()
                .flatten()
            {
                if let Some(cache) = cache_of(source, data) {
                    data.cache = cache;
                }
            }
        }
    }
    chart
}

fn cache_of(source: &CacheSource<'_>, data: &ChartData) -> Option<Vec<Option<String>>> {
    let (sheet, top, left, bottom, right) = reference(source.model, data.formula.as_deref()?)?;
    let mut points = Vec::new();
    for row in top..=bottom {
        for column in left..=right {
            let at = CellKey::new(sheet, row, column).ok()?;
            points.push(point(source, at, data.numeric));
        }
    }
    Some(points)
}

/// The sheet and corners of a series reference such as `Sheet1!$B$2:$B$4` or `'My Sheet'!$A$1`.
fn reference(model: &ModelRoot, text: &str) -> Option<(SheetId, u32, u32, u32, u32)> {
    let (sheet, cells) = text.trim_start_matches('=').rsplit_once('!')?;
    let sheet = match sheet.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        Some(quoted) => quoted.replace("''", "'"),
        None => sheet.to_owned(),
    };
    let id = model
        .sheets()
        .iter()
        .find(|s| {
            calc_doc_model::value::workbook_names_equal(&s.name().text().to_string_lossy(), &sheet)
        })?
        .id();
    let (top, left, bottom, right) = address::area(&cells.replace('$', ""))?;
    Some((id, top, left, bottom, right))
}

/// One point of a cache: what the cell shows as text, or as a number for a numeric reference
/// (a text cell is no number, so it leaves no point).
fn point(source: &CacheSource<'_>, at: CellKey, numeric: bool) -> Option<String> {
    let value = match source.model.cell(at)? {
        CellInput::Formula(_) => source.values?.get(&at)?.clone(),
        CellInput::Number(n) => CachedValue::Number(*n),
        CellInput::Text(t) => CachedValue::Text(t.clone()),
        CellInput::Logical(b) => CachedValue::Logical(*b),
        CellInput::Error(e) => CachedValue::Error(*e),
    };
    match value {
        CachedValue::Number(n) => Some(super::super::text::number(n.get())),
        CachedValue::Text(t) if !numeric => Some(t.text().to_string_lossy()),
        CachedValue::Logical(b) if !numeric => Some(if b { "TRUE" } else { "FALSE" }.to_owned()),
        CachedValue::Error(e) if !numeric => Some(e.excel_spelling().to_owned()),
        _ => None,
    }
}
