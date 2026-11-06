//! Explicit pivot refresh commands. Source formulas must supply current calculated values;
//! file caches, stale results and refusals never become refresh inputs.
use crate::{CellEdit, HostState, ProjectedValue, ProjectionRequest, project};
use calc_doc_model::pivot::*;
use calc_value::{CalcValue, CellKey, CoreRef, GridArea, SheetId};
use std::{collections::BTreeMap, sync::Arc};
fn fail(detail: impl AsRef<str>) -> String {
    format!("{} (dc-8wp)", detail.as_ref())
}
fn observed(cell: &crate::ProjectedCell) -> Result<CalcValue, String> {
    match cell.value() {
        ProjectedValue::Empty => Ok(CalcValue::empty()),
        ProjectedValue::Literal(v) => Ok(v.clone()),
        ProjectedValue::Calculated { value, calculation }
            if !calculation.reads_external_links() && !calculation.held_by_circular_rule() =>
        {
            Ok(value.clone())
        }
        _ => Err(fail(
            "Calculate the source formulas before refreshing; saved caches, stale values and refused formulas cannot supply source data",
        )),
    }
}
fn item(value: &CalcValue) -> Result<PivotItem, String> {
    Ok(match value.core_ref() {
        CoreRef::Empty => PivotItem::Missing,
        CoreRef::Number(n) if n.is_finite() && !value.is_excel_overflow() => {
            PivotItem::Number(n.to_string())
        }
        CoreRef::Text(s) => PivotItem::Text(
            String::from_utf16(s.utf16_code_units())
                .map_err(|_| fail("Unpaired UTF-16 source labels are not modelled"))?,
        ),
        CoreRef::Logical(b) => PivotItem::Boolean(b),
        CoreRef::Error(e) => PivotItem::Error(e.excel_spelling().into()),
        _ => return Err(fail("This source value cannot be stored in a pivot cache")),
    })
}
fn key(item: &PivotItem) -> Result<(u8, String), String> {
    Ok(match item {
        PivotItem::Missing => (0, String::new()),
        PivotItem::Number(s) => (
            1,
            s.parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .map(|n| format!("{:016x}", if n == 0. { 0 } else { n.to_bits() }))
                .ok_or_else(|| fail("Invalid saved numeric cache item"))?,
        ),
        PivotItem::Text(s) => (2, s.clone()),
        PivotItem::Boolean(b) => (3, u8::from(*b).to_string()),
        PivotItem::Error(s) => (4, s.clone()),
        _ => {
            return Err(fail(
                "Grouped, date and unknown cache items need measured refresh semantics",
            ));
        }
    })
}
fn read_row(host: &HostState, area: GridArea, row: u32) -> Result<Vec<CalcValue>, String> {
    let cells = (area.first().column()..=area.last().column())
        .map(|c| CellKey::new(area.sheet(), row, c).expect("source bounds"))
        .collect();
    let page = project(host, &ProjectionRequest { cells }).map_err(|e| e.to_string())?;
    page.cells().iter().map(observed).collect()
}
fn source_area(host: &HostState, cache: &PivotCache, origin: CellKey) -> Result<GridArea, String> {
    if cache.source_type != "worksheet" {
        return Err(fail("Only worksheet source data can be refreshed"));
    }
    let quote = |name: &str| format!("'{}'", name.replace('\'', "''"));
    if let Some(name) = &cache.source_name {
        if let Some(table) = host.model().definitions().tables().iter().find(|t| {
            t.name.eq_ignore_ascii_case(name) || t.display_name.eq_ignore_ascii_case(name)
        }) {
            if table.header_rows != 1 || table.area.row_count() <= table.totals_rows {
                return Err(fail("The source table needs one header row"));
            }
            let last = CellKey::new(
                table.sheet,
                table.area.last().row() - table.totals_rows,
                table.area.last().column(),
            )
            .map_err(|_| fail("Invalid table extent"))?;
            return GridArea::new(table.area.first(), last)
                .map_err(|_| fail("Invalid table source"));
        }
    }
    let reference = match (&cache.source_sheet, &cache.source_range, &cache.source_name) {
        (Some(sheet), _, Some(name)) => format!("{}!{name}", quote(sheet)),
        (_, _, Some(name)) => name.clone(),
        (Some(sheet), Some(range), _) => format!("{}!{range}", quote(sheet)),
        _ => return Err(fail("The cache has no worksheet source reference")),
    };
    match host.resolve_reference_text(origin, &reference) {
        calc_fml::reference::TextReference::Area(area) => Ok(area),
        _ => Err(fail(
            "The pivot source name or worksheet range is unavailable",
        )),
    }
}
/// Read every source field, preserving cache item identities and recording actual values only.
fn snapshot(host: &HostState, cache: &PivotCache, area: GridArea) -> Result<PivotCache, String> {
    if area.cell_count() > 1_000_000 || area.row_count() > 100_001 {
        return Err(fail(
            "Refresh exceeds its budget of 100,000 source records or 1,000,000 source cells",
        ));
    }
    if area.column_count() as usize != cache.fields.len() {
        return Err(fail(
            "The source column count changed; field remapping is not modelled",
        ));
    }
    let headers = read_row(host, area, area.first().row())?;
    for (header, field) in headers.iter().zip(&cache.fields) {
        let CoreRef::Text(text) = header.core_ref() else {
            return Err(fail("Every source column needs a text header"));
        };
        if calc_fml::text_order(
            text.utf16_code_units(),
            &field.name.encode_utf16().collect::<Vec<_>>(),
        ) != Some(std::cmp::Ordering::Equal)
        {
            return Err(fail(
                "The source header names changed; field remapping is not modelled",
            ));
        }
    }
    let mut cache = cache.clone();
    let mut indexes = Vec::new();
    for field in &cache.fields {
        let mut map = BTreeMap::new();
        for (i, item) in field.items.iter().enumerate() {
            map.entry(key(item)?).or_insert(i as u32);
        }
        indexes.push(map);
    }
    let mut records = Vec::new();
    for row in area.first().row() + 1..=area.last().row() {
        let values = read_row(host, area, row)?;
        let mut record = Vec::new();
        for (field, value) in values.iter().enumerate() {
            let value = item(value)?;
            let next = cache.fields[field].items.len() as u32;
            let index = *indexes[field].entry(key(&value)?).or_insert_with(|| {
                cache.fields[field].items.push(value);
                next
            });
            record.push(PivotItem::Index(index));
        }
        records.push(record);
    }
    if cache.fields.iter().map(|f| f.items.len()).sum::<usize>() > 100_000 {
        return Err(fail(
            "Refreshed shared items exceed the cache reader budget of 100,000",
        ));
    }
    for field in &mut cache.fields {
        field.shared_items = Some(field.items.len() as u32);
    }
    cache.record_count = Some(records.len() as u64);
    cache.snapshot = Some(Arc::new(PivotSnapshot {
        records,
        source_revision: host.revision().get(),
        numeric: host.recalc_context().numeric.clone(),
    }));
    Ok(cache)
}
fn overlaps(a: GridArea, b: GridArea) -> bool {
    a.intersection(b).is_some()
}
fn preflight_output(
    host: &HostState,
    before: &PivotState,
    after: &PivotState,
    source: GridArea,
    cache_id: u32,
) -> Result<(), String> {
    if after
        .tables()
        .filter(|t| {
            after
                .caches
                .caches
                .get(&t.cache_id)
                .is_some_and(|c| c.snapshot.is_some())
        })
        .map(|t| t.area.cell_count())
        .sum::<u64>()
        > 1_000_000
    {
        return Err(fail(
            "Refreshed pivot output exceeds the workbook budget of 1,000,000 cells",
        ));
    }
    let changed = after
        .tables()
        .filter(|t| t.cache_id == cache_id)
        .collect::<Vec<_>>();
    for (index, table) in changed.iter().enumerate() {
        let area = table.area;
        crate::can_edit(host, area.first())?;
        if overlaps(area, source) {
            return Err(fail("Pivot output cannot overlap its source data"));
        }
        if changed
            .iter()
            .skip(index + 1)
            .any(|t| overlaps(area, t.area))
            || after
                .tables()
                .any(|t| t.cache_id != cache_id && overlaps(area, t.area))
        {
            return Err(fail("There is not enough space between pivot tables"));
        }
        if host
            .model()
            .definitions()
            .tables()
            .iter()
            .any(|t| overlaps(area, t.area))
            || host
                .model()
                .definitions()
                .arrays_meeting(area)
                .next()
                .is_some()
            || host.engine.spills().any(|s| overlaps(area, s.area()))
        {
            return Err(fail(
                "Pivot output cannot overwrite a table or array result",
            ));
        }
        if host
            .loaded()
            .sheet(area.sheet())
            .is_some_and(|s| s.merged_areas().iter().any(|m| overlaps(area, *m)))
        {
            return Err(fail("Pivot output cannot cross merged cells"));
        }
        let old = before
            .tables()
            .find(|t| t.part == table.part)
            .map(|t| t.area);
        if host
            .model()
            .cells(area)
            .into_iter()
            .flatten()
            .any(|(at, _)| old.is_none_or(|a| !a.contains(at)))
        {
            return Err(fail(
                "Move the cells beside this pivot before refreshing into a larger range",
            ));
        }
    }
    Ok(())
}
pub(crate) fn prepare_refresh(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    part: &str,
) -> Result<Vec<CellEdit>, String> {
    if revision != host.revision().get() {
        return Err(fail("The workbook changed before pivot refresh"));
    }
    let before = host.model().definitions().pivots();
    let selected = before
        .sheet(sheet)
        .and_then(|s| s.tables.iter().find(|t| t.part == part))
        .ok_or_else(|| fail("The selected pivot is unavailable"))?;
    if !before.caches.issues.is_empty() || before.sheets.values().any(|s| !s.issues.is_empty()) {
        return Err(fail(
            "Unread pivot definitions prevent a shared-cache refresh",
        ));
    }
    let cache = before
        .caches
        .caches
        .get(&selected.cache_id)
        .ok_or_else(|| fail("The source cache is unavailable"))?;
    if !cache.unsupported.is_empty() {
        return Err(fail("Grouped or calculated caches cannot be refreshed"));
    }
    let area = source_area(host, cache, selected.area.first())?;
    if before.tables().any(|t| overlaps(area, t.area)) {
        return Err(fail("A pivot source cannot read another pivot output"));
    }
    let cache = Arc::new(snapshot(host, cache, area)?);
    let mut after = (**before).clone();
    after.caches.caches.insert(cache.id, cache.clone());
    for tables in after.sheets.values_mut() {
        for table in &mut tables.tables {
            if table.cache_id == cache.id {
                *table = calc_engine::pivot::layout(table, &cache)
                    .map_err(|e| fail(e.detail))?
                    .table;
            }
        }
    }
    preflight_output(host, before, &after, area, cache.id)?;
    host.loaded()
        .check_pivot_state(&after)
        .map_err(|e| e.to_string())?;
    let mut edits = before
        .tables()
        .filter(|t| t.cache_id == cache.id)
        .map(|t| CellEdit::Clear { area: t.area })
        .collect::<Vec<_>>();
    edits.push(CellEdit::Pivots(Arc::new(after)));
    Ok(edits)
}
