//! Resolve a pivot request to a displayed cell; values still come from the ordinary cell reader.
use super::*;
use calc_fml::PivotLookup as Lookup;
use calc_value::CoreRef;

fn equal(a: &str, b: &str) -> Option<bool> {
    calc_fml::text_order(&a.encode_utf16().collect::<Vec<_>>(), &b.encode_utf16().collect::<Vec<_>>())
        .map(|order| order == Ordering::Equal)
}

fn matches_item(item: &PivotItem, value: &CalcValue) -> Option<bool> {
    match (item, value.core_ref()) {
        (PivotItem::Missing, CoreRef::Text(s)) => Some(String::from_utf16(s.utf16_code_units()).ok()? == "(blank)"),
        (PivotItem::Text(a), CoreRef::Text(b)) => equal(a, &String::from_utf16(b.utf16_code_units()).ok()?),
        (PivotItem::Number(a), CoreRef::Text(b)) => Some(*a == String::from_utf16(b.utf16_code_units()).ok()?),
        (PivotItem::Date(a), CoreRef::Text(b)) => Some(a.split('T').next()? == String::from_utf16(b.utf16_code_units()).ok()?),
        (PivotItem::Number(a), CoreRef::Number(b)) => Some(a.parse::<f64>().ok()? == b),
        (PivotItem::Date(_), CoreRef::Number(b)) => Some(item.date_serial()? == b),
        (PivotItem::Boolean(a), CoreRef::Logical(b)) => Some(*a == b),
        (PivotItem::Other { .. } | PivotItem::Index(_), _) => None,
        _ => Some(false),
    }
}

/// Decode the compressed axis paths and find the displayed subtotal or item for the requested
/// prefix. An omitted inner field requires its visible subtotal, not a fresh calculation.
fn axis_position(table: &PivotTable, cache: &PivotCache, fields: &[i32], items: &[PivotAxisItem],
    wanted: &BTreeMap<usize, CalcValue>, measure: usize) -> Result<Option<usize>, ()> {
    if fields.is_empty() { return Ok(Some(0)); }
    let mut prefix = Vec::<u32>::new();
    let mut found = None;
    for (position, axis) in items.iter().enumerate() {
        let repeat = axis.repeat as usize;
        if repeat > prefix.len() { return Err(()); }
        prefix.truncate(repeat);
        prefix.extend_from_slice(&axis.indexes);
        if axis.kind == "grand" {
            if fields.iter().filter(|&&f| f >= 0).any(|f| wanted.contains_key(&(*f as usize))) { continue; }
            if fields.contains(&-2) && axis.data_index as usize != measure { continue; }
        } else {
            if axis.kind != "data" && axis.kind != "default" { continue; }
            if fields.iter().any(|&field| field >= 0)
                && !fields.iter().any(|&field| field >= 0 && wanted.contains_key(&(field as usize))) { continue; }
            let mut matched = true;
            for (level, field) in fields.iter().enumerate() {
                if *field == -2 {
                    if prefix.get(level).copied().map(|value| value as usize) != Some(measure) { matched = false; break; }
                    continue;
                }
                let field = usize::try_from(*field).map_err(|_| ())?;
                match (wanted.get(&field), prefix.get(level)) {
                    (None, None) => {},
                    (None, Some(_)) => { matched = false; break; },
                    (Some(_), None) => { matched = false; break; },
                    (Some(value), Some(index)) => {
                        let item = table.fields.get(field).and_then(|f| f.items.get(*index as usize)).ok_or(())?;
                        if item.hidden || item.kind != "data" { matched = false; break; }
                        let key = item.index.and_then(|i| cache.fields.get(field)?.items.get(i as usize)).ok_or(())?;
                        if !matches_item(key, value).ok_or(())? { matched = false; break; }
                    }
                }
            }
            if !matched { continue; }
        }
        if found.replace(position).is_some() { return Err(()); }
    }
    Ok(found)
}

/// Resolve only the error cases measured for a single-table model pivot. Successful requests
/// still need the model-pivot walk and must not read the saved output cells as results.
pub(crate) fn invalid_model_request(state: &PivotState, data: &calc_doc_model::DataModel,
    model: &calc_fml::CubeModel, data_field: &str, reference: &[GridArea],
    filters: &[(String, CalcValue)]) -> Option<Lookup> {
    if data.model_connection.is_none() || !data.other_connections.is_empty() || model.tables.len() != 1 {
        return None;
    }
    let mut tables = state.tables().filter(|t| reference.iter().any(|a| a.intersection(t.area).is_some()));
    let table = tables.next()?;
    if tables.next().is_some() || !table.unsupported.is_empty() || table.row_fields.len() != 1
        || table.column_fields.len() != 1 || table.row_fields[0] < 0 || table.column_fields[0] < 0
        || table.data_fields.len() != 1 || !table.page_fields.is_empty() || !table.filters.is_empty() {
        return None;
    }
    let cache = state.caches.caches.get(&table.cache_id)?;
    if cache.source_type != "external" || cache.source_connection_id.is_none()
        || cache.source_connection_id != data.model_connection_id
        || cache.unsupported.iter().any(|item| !matches!(item.as_str(),
        "cacheHierarchies" | "dimensions" | "measureGroups" | "maps")) {
        return None;
    }
    if table.row_fields[0] == table.column_fields[0] { return None; }
    let quote = |name: &str| format!("[{}]", name.replace(']', "]]"));
    let source = &model.tables[0];
    for index in [table.row_fields[0] as usize, table.column_fields[0] as usize] {
        let field = cache.fields.get(index)?;
        if table.fields.get(index).is_none_or(|field| field.items.is_empty()) || !source.columns.iter().any(|column| {
            let name = format!("{}.{}.{}", quote(&source.name), quote(&column.name), quote(&column.name));
            equal(&field.name, &name) == Some(true)
        }) { return None; }
    }
    if table.fields.get(table.data_fields[0].field as usize).is_none() { return None; }
    let offset = |name: &str| table.location.get(name).and_then(|value| value.parse::<u32>().ok());
    if offset("firstDataRow").and_then(|row| table.area.first().row().checked_add(row)).is_none_or(|row| row > table.area.last().row())
        || offset("firstDataCol").and_then(|col| table.area.first().column().checked_add(col)).is_none_or(|col| col > table.area.last().column()) {
        return None;
    }
    // The source connection and saved value field must both identify this retained model.
    let field = cache.fields.get(table.data_fields[0].field as usize)?;
    let held = model.pivot_measure(&field.name).ok()?;
    let missing = Lookup::Error(WorksheetErrorCode::Ref);
    let requested = match model.pivot_measure(data_field) {
        Ok(measure) => measure,
        Err(calc_fml::CubeFailure::Error(WorksheetErrorCode::Ref)) => return Some(missing),
        Err(_) => return None,
    };
    if requested != held { return Some(missing); }
    for (hierarchy, value) in filters {
        // A held CUBE member, or a non-text argument, may have separate Excel rules.
        if value.cube_value().is_some() { return None; }
        let CoreRef::Text(text) = value.core_ref() else { return None; };
        let Ok(text) = String::from_utf16(text.utf16_code_units()) else { return None; };
        match model.pivot_key(hierarchy, &text) {
            Ok(_) => {},
            Err(calc_fml::CubeFailure::Error(WorksheetErrorCode::Ref)) => return Some(missing),
            Err(_) => return None,
        }
    }
    None
}

pub fn data_cell(state: &PivotState, data_field: &str, reference: &[GridArea], filters: &[(String, CalcValue)]) -> Lookup {
    let missing = Lookup::Error(WorksheetErrorCode::Ref);
    let mut tables = state.tables().filter(|t| reference.iter().any(|a| a.intersection(t.area).is_some()));
    let Some(table) = tables.next() else { return if state.sheets.values().any(|s| !s.issues.is_empty()) { Lookup::NotModelled } else { missing }; };
    if tables.next().is_some() { return missing; }
    if !table.unsupported.is_empty() { return Lookup::NotModelled; }
    if table.row_fields.iter().chain(&table.column_fields).filter(|&&f| f >= 0).any(|&f| table.fields.get(f as usize).is_none_or(|field| field.items.is_empty())) { return Lookup::NotModelled; }
    let Some(cache) = state.caches.caches.get(&table.cache_id) else { return Lookup::NotModelled; };
    if !cache.unsupported.is_empty() { return Lookup::NotModelled; }
    let mut measure = None;
    for (i, data) in table.data_fields.iter().enumerate() {
        let Some(field) = cache.fields.get(data.field as usize) else { return Lookup::NotModelled; };
        match equal(data_field, data.name.as_deref().unwrap_or(&field.name)) {
            Some(true) => { measure = Some(i); break; },
            Some(false) => {},
            None => return Lookup::NotModelled,
        }
    }
    if measure.is_none() {
        for (i, data) in table.data_fields.iter().enumerate() {
            let Some(field) = cache.fields.get(data.field as usize) else { return Lookup::NotModelled; };
            match equal(data_field, &field.name) {
                Some(true) => { measure = Some(i); break; },
                Some(false) => {},
                None => return Lookup::NotModelled,
            }
        }
    }
    let Some(measure) = measure else { return missing; };
    let mut wanted: BTreeMap<usize, CalcValue> = BTreeMap::new();
    for (name, value) in filters {
        if matches!(value.core_ref(), CoreRef::Error(_)) { return missing; }
        let mut field = None;
        for (i, f) in cache.fields.iter().enumerate() {
            let base = equal(name, &f.name);
            let caption = table.fields.get(i).and_then(|f| f.attributes.get("name"))
                .map(|caption| equal(name, caption)).unwrap_or(Some(false));
            if base == Some(true) || caption == Some(true) { field = Some(i); break; }
            if base.is_none() || caption.is_none() { return Lookup::NotModelled; }
        }
        let Some(field) = field else { return missing; };
        if let Some(previous) = wanted.get(&field) {
            let same = match (previous, value.core_ref()) {
                (v, CoreRef::Text(b)) => match v.core_ref() { CoreRef::Text(a) => match calc_fml::text_order(a.utf16_code_units(), b.utf16_code_units()) { Some(order) => order == Ordering::Equal, None => return Lookup::NotModelled }, _ => false },
                (v, CoreRef::Number(b)) => matches!(v.core_ref(), CoreRef::Number(a) if a == b),
                _ => false,
            };
            if !same { return missing; }
        } else { wanted.insert(field, value.clone()); }
    }
    for page in &table.page_fields {
        if let Some(value) = wanted.remove(&(page.field as usize)) {
            let Some(item) = page.selected_item.and_then(|i| table.fields.get(page.field as usize)?.items.get(i as usize)) else { return Lookup::NotModelled; };
            let Some(key) = item.index.and_then(|i| cache.fields.get(page.field as usize)?.items.get(i as usize)) else { return Lookup::NotModelled; };
            match matches_item(key, &value) { Some(true) => {}, Some(false) => return missing, None => return Lookup::NotModelled }
        }
    }
    if wanted.keys().any(|f| !table.row_fields.contains(&(*f as i32)) && !table.column_fields.contains(&(*f as i32))) { return missing; }
    let row = match axis_position(table, cache, &table.row_fields, &table.row_items, &wanted, measure) { Ok(Some(p)) => p, Ok(None) => return missing, Err(()) => return Lookup::NotModelled };
    let col = match axis_position(table, cache, &table.column_fields, &table.col_items, &wanted, measure) { Ok(Some(p)) => p, Ok(None) => return missing, Err(()) => return Lookup::NotModelled };
    let offset = |key: &str| table.location.get(key).and_then(|s| s.parse::<u32>().ok());
    let Some(r) = offset("firstDataRow").and_then(|r| table.area.first().row().checked_add(r)).and_then(|r| r.checked_add(row as u32)) else { return Lookup::NotModelled; };
    let Some(c) = offset("firstDataCol").and_then(|c| table.area.first().column().checked_add(c)).and_then(|c| c.checked_add(col as u32)) else { return Lookup::NotModelled; };
    match CellKey::new(table.area.sheet(), r, c) { Ok(at) if table.area.contains(at) => Lookup::Cell(at), _ => Lookup::NotModelled }
}
