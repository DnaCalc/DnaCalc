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
    if value.image_value().is_some() {
        return Err(fail("Picture source values are not modelled for pivot caches"));
    }
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
        PivotItem::Number(_) | PivotItem::Date(_) => (
            1,
            match item {
                PivotItem::Date(_) => item.date_serial(),
                PivotItem::Number(s) => s.parse::<f64>().ok(),
                _ => None,
            }
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
pub(crate) fn source_area(host: &HostState, cache: &PivotCache, origin: CellKey) -> Result<GridArea, String> {
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
fn distinct_group_labels(items: &[PivotItem]) -> bool {
    let mut order: Vec<_> = items.iter().collect();
    let mut undecided = false;
    let mut compare = |a: &PivotItem, b: &PivotItem| match (a, b) {
        (PivotItem::Missing, PivotItem::Missing) => std::cmp::Ordering::Equal,
        (PivotItem::Missing, _) => std::cmp::Ordering::Greater,
        (_, PivotItem::Missing) => std::cmp::Ordering::Less,
        (PivotItem::Text(a), PivotItem::Text(b)) => calc_fml::text_order(
            &a.encode_utf16().collect::<Vec<_>>(), &b.encode_utf16().collect::<Vec<_>>())
            .unwrap_or_else(|| { undecided = true; std::cmp::Ordering::Equal }),
        _ => { undecided = true; std::cmp::Ordering::Equal },
    };
    order.sort_by(|a, b| compare(a, b));
    let unique = order.windows(2).all(|pair| compare(pair[0], pair[1]) != std::cmp::Ordering::Equal);
    unique && !undecided
}
/// Read every source field, preserving cache item identities and recording actual values only.
fn snapshot(host: &HostState, cache: &PivotCache, area: GridArea) -> Result<PivotCache, String> {
    if area.cell_count() > 1_000_000 || area.row_count() > 100_001 {
        return Err(fail(
            "Refresh exceeds its budget of 100,000 source records or 1,000,000 source cells",
        ));
    }
    let source_count = cache.fields.iter().take_while(|field|
        !matches!(field.attributes.get("databaseField").map(String::as_str), Some("0" | "false"))).count();
    if area.column_count() as usize != source_count || cache.fields[source_count..].iter().any(|field|
        !matches!(field.attributes.get("databaseField").map(String::as_str), Some("0" | "false"))) {
        return Err(fail("The source column count or grouped-field order changed"));
    }
    if host.declared_date1904() && cache.fields.iter().any(|field| field.attributes.contains_key("formula")) {
        return Err(fail("Calculated pivot fields in the 1904 date system are not modelled"));
    }
    let headers = read_row(host, area, area.first().row())?;
    for (header, field) in headers.iter().zip(&cache.fields[..source_count]) {
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
    for field in &cache.fields[..source_count] {
        if let Some(items) = field.group.as_ref().and_then(|group| group.numeric_items.as_ref()) {
            if !distinct_group_labels(items) { return Err(fail("Ambiguous numeric group labels are not modelled")); }
        }
    }
    for field in &cache.fields[source_count..] {
        if let Some(group) = field.group.as_ref().filter(|group| group.discrete.is_some()) {
            let base = group.attributes.get("base").and_then(|value| value.parse::<usize>().ok())
                .filter(|&base| base < source_count).ok_or_else(|| fail("Invalid discrete grouping base"))?;
            if !distinct_group_labels(&field.items) || !distinct_group_labels(&cache.fields[base].items) {
                return Err(fail("Ambiguous discrete group labels are not modelled"));
            }
        }
    }
    let mut cache = cache.clone();
    let tracked_unused: Vec<_> = cache.fields.iter().map(|field| !field.unused_items.is_empty()).collect();
    if tracked_unused.iter().any(|tracked| *tracked) && !cache.calculated_items.is_empty() {
        return Err(fail("Calculated and retained items together are not modelled"));
    }
    if tracked_unused.iter().any(|tracked| *tracked)
        && !matches!(cache.attributes.get("missingItemsLimit").map(String::as_str), Some("32500" | "0") | None) {
        return Err(fail("This unused-item retention limit is not modelled"));
    }
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
        for field in &cache.fields[source_count..] {
            if field.group.is_none() && field.attributes.contains_key("formula") {
                record.push(PivotItem::Missing);
                continue;
            }
            let group = field.group.as_ref().ok_or_else(|| fail("A grouped cache field is missing its definition"))?;
            let base = group.attributes.get("base").and_then(|v| v.parse::<usize>().ok())
                .filter(|i| *i < source_count).ok_or_else(|| fail("Invalid grouped source field"))?;
            if let Some(discrete) = &group.discrete {
                let PivotItem::Index(index) = record[base] else { return Err(fail("Invalid discrete grouping source")); };
                let mapped = discrete.get(index as usize).copied()
                    .filter(|&index| (index as usize) < field.items.len())
                    .ok_or_else(|| fail("New source items in a discrete group are not modelled"))?;
                record.push(PivotItem::Index(mapped));
                continue;
            }
            let range = group.range.as_ref().ok_or_else(|| fail("A grouped date range is unavailable"))?;
            let date = match values[base].core_ref() {
                CoreRef::Number(n) if n.is_finite() => n,
                _ => return Err(fail("Grouping nonnumeric date values is not modelled")),
            };
            let system = if host.declared_date1904() { calc_func::locale::DateSystem::System1904 }
                else { calc_func::locale::DateSystem::System1900 };
            let bound = |name: &str| -> Result<f64, String> {
                let date = range.get(name).ok_or_else(|| fail("Missing grouping bound"))?;
                if !date.split('-').next().and_then(|v| v.parse::<i64>().ok())
                    .is_some_and(|year| (1900..=9999).contains(&year)) {
                    return Err(fail("Invalid grouping year"));
                }
                let serial = PivotItem::Date(date.clone()).date_serial()
                    .ok_or_else(|| fail("Invalid grouping bound"))?;
                Ok(if host.declared_date1904() { serial - 1462.0 } else { serial })
            };
            let start = bound("startDate")?;
            let end = bound("endDate")?;
            if start > end { return Err(fail("Invalid grouping range")); }
            let min_date = calc_func::locale::excel_serial_from_ymd(system, 1900, 1, 1)
                .ok_or_else(|| fail("Invalid date system"))?;
            let max_date = calc_func::locale::excel_serial_from_ymd(system, 9999, 12, 31)
                .ok_or_else(|| fail("Invalid date system"))? + 1.0;
            if date < min_date || date >= max_date { return Err(fail("Invalid grouped date")); }
            let (year, month, day) = calc_func::locale::ymd_from_excel_serial(system, date)
                .ok_or_else(|| fail("Invalid grouped date"))?;
            let seven_day = range.get("groupBy").is_some_and(|v| v == "days")
                && range.get("groupInterval").is_some_and(|v| v == "7");
            if !seven_day && (date < start || date > end) {
                return Err(fail("Dates outside the saved grouping range are not modelled"));
            }
            let index = match range.get("groupBy").map(String::as_str) {
                Some("days") if seven_day => {
                    if date.floor() < start { Some(0) }
                    else if date.floor() > end { Some(field.items.len() - 1) }
                    else { Some(((date.floor() - start) / 7.0).floor() as usize + 1) }
                }
                Some("days") => {
                    // The saved day labels cover a leap year, so March follows February 29.
                    let first = calc_func::locale::excel_serial_from_ymd(system, 2000, 1, 1);
                    let current = calc_func::locale::excel_serial_from_ymd(system, 2000, month, day);
                    first.zip(current).map(|(first, current)| (current - first) as usize + 1)
                }
                Some("months") => usize::try_from(month).ok(),
                Some("quarters") => usize::try_from((month - 1) / 3 + 1).ok(),
                Some("years") => {
                    let start = range.get("startDate").and_then(|v| v.split('-').next())
                        .and_then(|v| v.parse::<i64>().ok()).ok_or_else(|| fail("Invalid grouping year"))?;
                    usize::try_from(year - start + 1).ok()
                }
                _ => None,
            }.filter(|i| if seven_day { *i < field.items.len() } else { *i > 0 && *i + 1 < field.items.len() })
                .ok_or_else(|| fail("The grouped date is outside its saved items"))?;
            record.push(PivotItem::Index(index as u32));
        }
        if cache.calculated_items.iter().any(|item| matches!(record.get(item.field as usize),
            Some(PivotItem::Index(index)) if *index == item.item)) {
            return Err(fail("A calculated item that also occurs in source data is not modelled"));
        }
        records.push(record);
    }
    if cache.fields.iter().map(|f| f.items.len()).sum::<usize>() > 100_000 {
        return Err(fail(
            "Refreshed shared items exceed the cache reader budget of 100,000",
        ));
    }
    let retaining_items = tracked_unused.iter().any(|tracked| *tracked)
        && cache.attributes.get("missingItemsLimit").map(String::as_str) != Some("0");
    for (index, field) in cache.fields[..source_count].iter_mut().enumerate() {
        field.shared_items = Some(field.items.len() as u32);
        if retaining_items {
            let used: std::collections::BTreeSet<_> = records.iter().filter_map(|record| match record[index] {
                PivotItem::Index(item) => Some(item), _ => None,
            }).collect();
            let unused: Vec<_> = (0..field.items.len() as u32).filter(|item| !used.contains(item)).collect();
            if (tracked_unused[index] && unused.is_empty()) || (!tracked_unused[index] && !unused.is_empty()) {
                return Err(fail("This change to unused cache fields is not modelled"));
            }
            field.unused_items = unused;
        }
    }
    cache.record_count = Some(records.len() as u64);
    cache.snapshot = Some(Arc::new(PivotSnapshot {
        records,
        origin: PivotSnapshotOrigin::SourceRead,
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
/// Apply the walk's explicit data-field number formats to the same styles the grid and save use.
fn append_number_formats(host: &HostState, formats: &BTreeMap<CellKey, u32>, edits: &mut Vec<CellEdit>) -> Result<(), String> {
    if formats.is_empty() { return Ok(()); }
    let mut styles = (**host.styles()).clone();
    let mut assignments = Vec::new();
    let mut derived = BTreeMap::new();
    for (&at, &number_format) in formats {
        let source = host.cell_style(at);
        let style = if let Some(&style) = derived.get(&(source, number_format)) { style } else {
            let mut xf = styles.cell_formats.get(source as usize).cloned().ok_or_else(|| fail("The pivot cell's source format is unavailable"))?;
            xf.number_format_id = Some(number_format);
            xf.apply_number_format = true;
            let style = if let Some(index) = styles.cell_formats.iter().position(|old| *old == xf) { index as u32 }
                else {
                    let index = u32::try_from(styles.cell_formats.len()).map_err(|_| fail("Too many cell formats"))?;
                    styles.cell_formats.push(xf);
                    styles.sources.cell_formats.insert(index, source);
                    index
                };
            if host.style_number_format(&styles, style).is_none() { return Err(fail("The value-field number format is unavailable")); }
            derived.insert((source, number_format), style);
            style
        };
        assignments.push(CellEdit::SetFormat { at, format: Some(style) });
    }
    if styles != **host.styles() { edits.push(CellEdit::Styles(Arc::new(styles))); }
    edits.extend(assignments);
    Ok(())
}

/// Select the captured zero-retention policy. Existing cached items are removed on refresh.
pub(crate) fn prepare_forget_missing_items(host: &HostState, sheet: SheetId, revision: u64, part: &str) -> Result<Vec<CellEdit>, String> {
    prepare_missing_items_limit(host, sheet, revision, part, Some(0))
}
pub(crate) fn prepare_missing_items_limit(host: &HostState, sheet: SheetId, revision: u64, part: &str, limit: Option<u32>) -> Result<Vec<CellEdit>, String> {
    if revision != host.revision().get() { return Err(fail("The workbook changed before setting item retention")); }
    if !matches!(limit, None | Some(0 | 32500)) { return Err(fail("This unused-item retention limit is not modelled")); }
    let before = host.model().definitions().pivots();
    let selected = before.sheet(sheet).and_then(|s| s.tables.iter().find(|t| t.part == part))
        .ok_or_else(|| fail("The selected pivot is unavailable"))?;
    let mut after = (**before).clone();
    let cache = after.caches.caches.get_mut(&selected.cache_id).ok_or_else(|| fail("The source cache is unavailable"))?;
    let attributes = &mut Arc::make_mut(cache).attributes;
    if let Some(limit) = limit { attributes.insert("missingItemsLimit".into(), limit.to_string()); }
    else { attributes.remove("missingItemsLimit"); }
    host.loaded().check_pivot_state(&after).map_err(|e| e.to_string())?;
    Ok(vec![CellEdit::Pivots(Arc::new(after))])
}

/// Recompute an edited field layout from the current cache records, without reading source cells.
pub(crate) fn prepare_set_layout(host: &HostState, sheet: SheetId, revision: u64, part: &str,
    rows: Vec<i32>, columns: Vec<i32>) -> Result<Vec<CellEdit>, String> {
    if revision != host.revision().get() { return Err(fail("The workbook changed before changing the pivot layout")); }
    let before = host.model().definitions().pivots();
    if !before.caches.issues.is_empty() || before.sheets.values().any(|s| !s.issues.is_empty()) {
        return Err(fail("Unread pivot definitions prevent a layout change"));
    }
    let selected = before.sheet(sheet).and_then(|s| s.tables.iter().find(|t| t.part == part))
        .ok_or_else(|| fail("The selected pivot is unavailable"))?;
    if !selected.page_fields.is_empty() || !selected.filters.is_empty() || !selected.unsupported.is_empty() {
        return Err(fail("Changing this pivot's filters or unsupported metadata is not modelled"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for &field in rows.iter().chain(&columns) {
        if (field < 0 && field != -2) || (field >= 0 && field as usize >= selected.fields.len()) || !seen.insert(field) {
            return Err(fail("Choose distinct source fields for the row and column axes"));
        }
    }
    if selected.data_fields.is_empty() || (selected.data_fields.len() > 1) != seen.contains(&-2) {
        return Err(fail("This layout change needs a valid Values position"));
    }
    let cache = before.caches.caches.get(&selected.cache_id).ok_or_else(|| fail("The source cache is unavailable"))?;
    if !cache.unsupported.is_empty() { return Err(fail("This source cache cannot be used for a layout change")); }
    let area = source_area(host, cache, selected.area.first())?;
    let mut cache = (**cache).clone();
    if cache.snapshot.is_none() {
        let saved = host.loaded().pivot_records(cache.id, 100_000).map_err(|e| fail(e.detail))?;
        if saved.truncated || saved.declared_count.is_some_and(|n| n != saved.records.len() as u64)
            || saved.records.iter().any(|row| row.len() != cache.fields.len()) {
            return Err(fail("Saved pivot records are incomplete"));
        }
        let part = cache.records_part.clone().ok_or_else(|| fail("The source cache has no saved records"))?;
        cache.snapshot = Some(Arc::new(PivotSnapshot { records: saved.records,
            origin: PivotSnapshotOrigin::FileRecords { part }, source_revision: revision,
            numeric: host.recalc_context().numeric.clone() }));
    }
    // A field outside the saved axes can be stored inline. Give its records shared-item
    // identities before the walk, preserving the actual file values and their origin.
    let records = &mut Arc::make_mut(cache.snapshot.as_mut().expect("snapshot available")).records;
    if records.iter().any(|row| row.len() != cache.fields.len()) { return Err(fail("Pivot records are incomplete")); }
    for field in rows.iter().chain(&columns).filter_map(|&field| usize::try_from(field).ok()) {
        let saved_field = cache.fields.get_mut(field).ok_or_else(|| fail("Invalid axis field"))?;
        let mut indexes = BTreeMap::new();
        for (index, item) in saved_field.items.iter().enumerate() { indexes.entry(key(item)?).or_insert(index as u32); }
        for record in records.iter_mut() {
            match &record[field] {
                PivotItem::Index(index) if (*index as usize) < saved_field.items.len() => {},
                PivotItem::Index(_) => return Err(fail("A saved pivot item index is invalid")),
                item => {
                    if saved_field.group.is_some() { return Err(fail("Inline grouped cache items are not modelled")); }
                    let identity = key(item)?;
                    let index = if let Some(index) = indexes.get(&identity) { *index } else {
                        let index = u32::try_from(saved_field.items.len()).map_err(|_| fail("Too many saved cache items"))?;
                        saved_field.items.push(item.clone()); indexes.insert(identity, index); index
                    };
                    record[field] = PivotItem::Index(index);
                }
            }
        }
        saved_field.shared_items = Some(saved_field.items.len() as u32);
    }
    if cache.fields.iter().map(|field| field.items.len()).sum::<usize>() > 100_000 {
        return Err(fail("Saved pivot items exceed the cache reader budget"));
    }
    let cache = Arc::new(cache);
    let mut after = (**before).clone();
    let changed = after.sheets.get_mut(&sheet).and_then(|s| s.tables.iter_mut().find(|t| t.part == part))
        .ok_or_else(|| fail("The selected pivot is unavailable"))?;
    changed.row_fields = rows;
    changed.column_fields = columns;
    for field in &mut changed.fields {
        if matches!(field.attributes.get("axis").map(String::as_str), Some("axisRow" | "axisCol")) {
            field.attributes.remove("axis");
        }
    }
    for &field in &changed.row_fields { if field >= 0 { changed.fields[field as usize].attributes.insert("axis".into(), "axisRow".into()); } }
    for &field in &changed.column_fields { if field >= 0 { changed.fields[field as usize].attributes.insert("axis".into(), "axisCol".into()); } }
    after.caches.caches.insert(cache.id, cache.clone());
    let mut formats = BTreeMap::new();
    for tables in after.sheets.values_mut() {
        for table in &mut tables.tables {
            if table.cache_id == cache.id {
                let layout = calc_engine::pivot::layout(table, &cache).map_err(|e| fail(e.detail))?;
                formats.extend(layout.number_formats);
                *table = layout.table;
            }
        }
    }
    preflight_output(host, before, &after, area, cache.id)?;
    host.loaded().check_pivot_state(&after).map_err(|e| e.to_string())?;
    let mut edits = before.tables().filter(|t| t.cache_id == cache.id).map(|t| CellEdit::Clear { area: t.area }).collect::<Vec<_>>();
    append_number_formats(host, &formats, &mut edits)?;
    edits.push(CellEdit::Pivots(Arc::new(after)));
    Ok(edits)
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
    let mut cache = snapshot(host, cache, area)?;
    let mut after = (**before).clone();
    // The captured automatic and maximum policies retain an item removed from either axis.
    // Other arrangements keep their existing refusal until their retention behavior is measured.
    if matches!(cache.attributes.get("missingItemsLimit").map(String::as_str), None | Some("32500"))
        && before.tables().filter(|t| t.cache_id == cache.id).count() == 1
        && selected.row_fields.len() == 1 && selected.column_fields.len() == 1
        && selected.row_fields[0] >= 0 && selected.column_fields[0] >= 0
        && selected.data_fields.len() == 1 && selected.data_fields[0].subtotal == "sum"
        && selected.page_fields.is_empty() && selected.filters.is_empty()
        && cache.calculated_items.is_empty() && cache.fields.iter().all(|f| f.group.is_none())
        && selected.fields.iter().all(|f| f.sort_measure.is_none() && f.items.iter().all(|i| !i.hidden && !i.calculated)) {
        let records = &cache.snapshot.as_ref().expect("snapshot just created").records;
        let table = after.sheets.get_mut(&sheet).and_then(|s| s.tables.iter_mut().find(|t| t.part == part))
            .ok_or_else(|| fail("The selected pivot is unavailable"))?;
        for index in [selected.row_fields[0] as usize, selected.column_fields[0] as usize] {
            let field = cache.fields.get_mut(index).ok_or_else(|| fail("Invalid retained field"))?;
            let used: std::collections::BTreeSet<_> = records.iter().filter_map(|row| match row.get(index) {
                Some(PivotItem::Index(i)) => Some(*i), _ => None,
            }).collect();
            field.unused_items = (0..field.items.len() as u32).filter(|i| !used.contains(i)).collect();
            if field.unused_items.len() > 32500 { return Err(fail("Retaining more than 32500 unused items is not modelled")); }
            for item in &mut table.fields[index].items {
                if item.kind == "data" { item.missing = item.index.is_some_and(|i| field.unused_items.contains(&i)); }
            }
        }
    }
    if cache.attributes.get("missingItemsLimit").map(String::as_str) == Some("0") {
        if before.tables().filter(|t| t.cache_id == cache.id).count() != 1
            || !cache.calculated_items.is_empty() || cache.fields.iter().any(|f| f.group.is_some() || !f.formula_items.is_empty())
            || selected.fields.iter().any(|f| f.items.iter().any(|i| i.hidden || i.calculated))
            || !selected.page_fields.is_empty() || !selected.filters.is_empty() {
            return Err(fail("Removing unused items in this cache is not modelled"));
        }
        let records = &mut Arc::make_mut(cache.snapshot.as_mut().expect("snapshot just created")).records;
        let mut maps = Vec::new();
        for (field_index, field) in cache.fields.iter_mut().enumerate() {
            let used: std::collections::BTreeSet<_> = records.iter().filter_map(|record| match record[field_index] {
                PivotItem::Index(index) => Some(index), _ => None,
            }).collect();
            let mut map = vec![None; field.items.len()];
            let mut items = Vec::new();
            for (old, item) in field.items.iter().enumerate() {
                if used.contains(&(old as u32)) {
                    map[old] = Some(items.len() as u32);
                    items.push(item.clone());
                }
            }
            field.items = items;
            field.shared_items = Some(field.items.len() as u32);
            field.unused_items.clear();
            for record in records.iter_mut() {
                if let PivotItem::Index(index) = &mut record[field_index] {
                    *index = map[*index as usize].ok_or_else(|| fail("Invalid cache item reference"))?;
                }
            }
            maps.push(map);
        }
        let table = after.sheets.get_mut(&sheet).and_then(|s| s.tables.iter_mut().find(|t| t.part == part))
            .ok_or_else(|| fail("The selected pivot is unavailable"))?;
        for (field, map) in table.fields.iter_mut().zip(&maps) {
            field.items.retain_mut(|item| {
                if let Some(old) = item.index {
                    if let Some(Some(new)) = map.get(old as usize) { item.index = Some(*new); item.missing = false; true }
                    else { false }
                } else { true }
            });
        }
    }
    let cache = Arc::new(cache);
    after.caches.caches.insert(cache.id, cache.clone());
    let mut formats = BTreeMap::new();
    for tables in after.sheets.values_mut() {
        for table in &mut tables.tables {
            if table.cache_id == cache.id {
                let layout = calc_engine::pivot::layout(table, &cache).map_err(|e| fail(e.detail))?;
                formats.extend(layout.number_formats);
                *table = layout.table;
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
    append_number_formats(host, &formats, &mut edits)?;
    edits.push(CellEdit::Pivots(Arc::new(after)));
    Ok(edits)
}

/// Headers used by the creation controls and revalidated by the command.
pub fn source_fields(host:&HostState,area:GridArea)->Result<Vec<String>,String>{
    if area.row_count()<2||area.column_count()>256||area.cell_count()>1_000_000||area.row_count()>100_001{return Err(fail("Select headers and at least one data row, with at most 256 columns, 100,000 records and 1,000,000 cells"));}
    let mut names=Vec::<String>::new();
    for value in read_row(host,area,area.first().row())?{
        let CoreRef::Text(text)=value.core_ref()else{return Err(fail("Every pivot source column needs a text header"));};
        let name=String::from_utf16(text.utf16_code_units()).map_err(|_|fail("The source header cannot be represented"))?;
        if name.trim().is_empty()||names.iter().any(|prior|calc_fml::text_order(&prior.encode_utf16().collect::<Vec<_>>(),text.utf16_code_units())!=Some(std::cmp::Ordering::Less)&&calc_fml::text_order(&prior.encode_utf16().collect::<Vec<_>>(),text.utf16_code_units())!=Some(std::cmp::Ordering::Greater)){return Err(fail("Use nonblank, distinct source headers"));}
        names.push(name);
    }Ok(names)
}
pub(crate) fn prepare_create(host:&HostState,revision:u64,source:GridArea,destination:&str,row:u32,value:u32,subtotal:&str)->Result<Vec<CellEdit>,String>{
    prepare_create_layout(host, revision, source, destination, vec![row], Vec::new(), vec![(value, subtotal.into())])
}
pub(crate) fn prepare_create_layout(host:&HostState,revision:u64,source:GridArea,destination:&str,rows:Vec<u32>,columns:Vec<u32>,values:Vec<(u32,String)>)->Result<Vec<CellEdit>,String>{
    if revision!=host.revision().get(){return Err(fail("The source changed; choose its fields again"));}
    if host.loaded().workbook_part().ends_with(".bin")||!host.axis_edits.is_empty(){return Err(fail("Save as XLSX and reopen before creating pivots after structural edits"));}
    let before=host.model().definitions().pivots();if !before.caches.issues.is_empty()||before.sheets.values().any(|s|!s.issues.is_empty()){return Err(fail("Unread pivot metadata must be resolved before creating another pivot"));}
    let names=source_fields(host,source)?;
    if rows.is_empty() || values.is_empty() || rows.iter().chain(&columns).any(|&i| i as usize >= names.len())
        || values.iter().any(|(i,_)| *i as usize >= names.len()) { return Err(fail("Choose source row and value fields")); }
    let mut unique = std::collections::BTreeSet::new();
    if rows.iter().chain(&columns).any(|i| !unique.insert(*i)) { return Err(fail("Choose distinct row and column fields")); }
    let mut data_fields = Vec::new();
    for (value, subtotal) in &values {
        let operation=match subtotal.as_str(){"sum"=>"Sum","count"=>"Count","countNums"=>"Count Numbers","min"=>"Min","max"=>"Max","average"=>"Average",_=>return Err(fail("Choose a supported value aggregation"))};
        let caption=format!("{operation} of {}",names[*value as usize]);
        data_fields.push(PivotDataField{name:Some(caption.clone()),field:*value,subtotal:subtotal.clone(),extended_show:None,attributes:BTreeMap::from([("name".into(),caption),("fld".into(),value.to_string()),("subtotal".into(),subtotal.clone())])});
    }
    let destination=match host.resolve_reference_text(source.first(),destination){calc_fml::reference::TextReference::Area(area)if area.cell_count()==1=>area.first(),_=>return Err(fail("Enter one destination cell, including its sheet when needed"))};
    let target=host.loaded().sheet(destination.sheet()).ok_or_else(||fail("Save and reopen the new destination worksheet first"))?;if target.protected()||target.is_macro_sheet(){return Err(fail("The destination worksheet must be editable"));}
    if before.tables().any(|t|overlaps(source,t.area)){return Err(fail("A pivot source cannot read another pivot output"));}
    let cache_id=(0..u32::MAX).find(|id|!before.caches.caches.contains_key(id)).ok_or_else(||fail("Pivot cache identities are exhausted"))?;
    let reserved=before.tables().map(|t|t.part.clone()).chain(before.caches.caches.values().flat_map(|c|std::iter::once(c.part.clone()).chain(c.records_part.clone()))).collect::<Vec<_>>();
    let part=host.loaded().available_part_name("pivotTables/dnaPivot","xml",&reserved);
    let cache_part=host.loaded().available_part_name("pivotCache/dnaCache","xml",&reserved);
    let records_part=host.loaded().available_part_name("pivotCache/dnaRecords","xml",&reserved);
    let name=(1u32..).map(|i|format!("PivotTable{i}")).find(|name|!before.tables().any(|t|t.name.eq_ignore_ascii_case(name))&&!host.model().definitions().tables().iter().any(|t|t.name.eq_ignore_ascii_case(name))).ok_or_else(||fail("Pivot table names are exhausted"))?;
    let sheet_name=host.model().sheet_by_id(source.sheet()).ok_or_else(||fail("The source worksheet is unavailable"))?.name().text().to_string_lossy();
    let source_range=format!("{}:{}",crate::chart::cell_a1(source.first()),crate::chart::cell_a1(source.last()));
    let cache=PivotCache{id:cache_id,part:cache_part,records_part:Some(records_part),record_count:Some(0),source_type:"worksheet".into(),source_sheet:Some(sheet_name),source_range:Some(source_range),source_name:None,calculated_items:Vec::new(),fields:names.iter().map(|name|PivotCacheField{name:name.clone(),shared_items:Some(0),group:None,items:Vec::new(),unused_items:Vec::new(),formula_items:Vec::new(),attributes:BTreeMap::from([("name".into(),name.clone()),("numFmtId".into(),"0".into())])}).collect(),attributes:BTreeMap::new(),unsupported:Vec::new(),snapshot:None};
    let cache=Arc::new(snapshot(host,&cache,source)?);
    let fields=(0..names.len()).map(|i|{let mut attributes=BTreeMap::from([("showAll".into(),"0".into())]);if rows.contains(&(i as u32)){attributes.insert("axis".into(),"axisRow".into());}if columns.contains(&(i as u32)){attributes.insert("axis".into(),"axisCol".into());}if values.iter().any(|(field,_)| *field as usize==i){attributes.insert("dataField".into(),"1".into());}PivotField{attributes,items:Vec::new(),sort_measure:None}}).collect();
    let mut column_fields: Vec<i32> = columns.into_iter().map(|i| i as i32).collect();
    if data_fields.len() > 1 { column_fields.push(-2); }
    let table=PivotTable{name:name.clone(),part,cache_id,area:GridArea::cell(destination),row_fields:rows.into_iter().map(|i|i as i32).collect(),column_fields,page_fields:Vec::new(),filters:Vec::new(),data_fields,attributes:BTreeMap::from([("name".into(),name),("cacheId".into(),cache_id.to_string()),("dataCaption".into(),"Values".into()),("compact".into(),"1".into()),("outline".into(),"0".into()),("rowGrandTotals".into(),"1".into()),("colGrandTotals".into(),"1".into())]),location:BTreeMap::from([("firstHeaderRow".into(),"1".into()),("firstDataRow".into(),"1".into()),("firstDataCol".into(),"1".into())]),fields,col_items:Vec::new(),row_items:Vec::new(),unsupported:Vec::new()};
    let layout=calc_engine::pivot::layout(&table,&cache).map_err(|e|fail(e.detail))?;
    let mut after=(**before).clone();after.caches.caches.insert(cache_id,cache);after.sheets.entry(destination.sheet()).or_default().tables.push(layout.table);
    preflight_output(host,before,&after,source,cache_id)?;host.loaded().check_pivot_state(&after).map_err(|e|e.to_string())?;
    let mut edits = Vec::new();
    append_number_formats(host, &layout.number_formats, &mut edits)?;
    edits.push(CellEdit::Pivots(Arc::new(after)));
    Ok(edits)
}
