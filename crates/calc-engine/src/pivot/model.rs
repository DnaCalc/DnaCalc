//! Model pivots use the same retained model and member identity as the CUBE functions.
use super::*;
use calc_fml::{
    CubeColumn, CubeFailure, CubeLookup, CubeMeasure, CubeMember, CubeModel, CubeTable, PivotLookup,
};
use calc_value::CoreRef;
use std::sync::Arc;

/// Build the shared CUBE model from the retained document snapshot.
pub fn cube_model(data: &calc_doc_model::DataModel) -> CubeLookup {
    let Some(connection) = &data.model_connection else {
        return CubeLookup::NotReadable;
    };
    let Some(snapshot) = &data.snapshot else {
        return CubeLookup::NotReadable;
    };
    if snapshot.tables.iter().any(|table| {
        table
            .rows
            .iter()
            .any(|row| row.len() != table.columns.len())
    }) {
        return CubeLookup::NotReadable;
    }
    let tables = snapshot
        .tables
        .iter()
        .map(|table| CubeTable {
            name: table.name.clone(),
            columns: table
                .columns
                .iter()
                .enumerate()
                .map(|(i, name)| CubeColumn {
                    name: name.clone(),
                    cells: table
                        .rows
                        .iter()
                        .map(|row| {
                            row[i]
                                .as_ref()
                                .map_or_else(CalcValue::empty, |v| v.to_value())
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    CubeLookup::Model(Arc::new(CubeModel {
        connection: connection.clone(),
        tables,
        measures: data
            .measures
            .iter()
            .map(|m| CubeMeasure {
                name: m.name.clone(),
                expression: m.expression.clone(),
            })
            .collect(),
    }))
}

/// A newly published model snapshot must be the admitted model, with its source revision and
/// numeric mode intact. Existing snapshots may remain pinned when a different model is refreshed.
pub fn valid_model_snapshot(
    snapshot: &PivotSnapshot,
    current: Option<&calc_doc_model::DataModel>,
    previous: Option<&PivotSnapshot>,
    numeric: Option<&calc_value::NumericMode>,
) -> bool {
    let PivotSnapshotOrigin::DataModel { model } = &snapshot.origin else {
        return true;
    };
    if previous == Some(snapshot) {
        return true;
    }
    if !snapshot.records.is_empty()
        || current != Some(model.as_ref())
        || numeric.is_some_and(|mode| mode != &snapshot.numeric)
    {
        return false;
    }
    let Some(rows) = model.snapshot.as_ref() else {
        return false;
    };
    matches!(rows.tables.as_slice(), [table] if matches!(&table.provenance,
        calc_doc_model::ModelSnapshotProvenance::WorksheetTables { revision } if *revision == snapshot.source_revision))
}

fn bad(table: &PivotTable, detail: &str) -> PivotIssue {
    PivotIssue {
        part: table.part.clone(),
        detail: detail.into(),
        owner: "dc-8wp.19",
    }
}
fn text(value: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_utf16_code_units(
        value.encode_utf16().collect(),
    ))
}
fn bracket(value: &str) -> String {
    format!("[{}]", value.replace(']', "]]"))
}
fn equal(left: &str, right: &str) -> bool {
    calc_fml::text_order(
        &left.encode_utf16().collect::<Vec<_>>(),
        &right.encode_utf16().collect::<Vec<_>>(),
    ) == Some(Ordering::Equal)
}

struct Axis {
    hierarchy: String,
    members: Vec<Option<CubeMember>>,
}
struct Prepared {
    model: Arc<CubeModel>,
    measure: CubeMember,
    rows: Axis,
    columns: Axis,
}

fn axis(
    table: &PivotTable,
    cache: &PivotCache,
    model: &CubeModel,
    field_index: usize,
    items: &[PivotAxisItem],
) -> Result<Axis, PivotIssue> {
    let issue = || bad(table, "This model-pivot axis is not modelled");
    let field = cache.fields.get(field_index).ok_or_else(issue)?;
    let pivot_field = table.fields.get(field_index).ok_or_else(issue)?;
    if pivot_field
        .items
        .iter()
        .any(|item| item.hidden || item.missing || item.calculated || item.kind != "data")
    {
        return Err(issue());
    }
    let source = model.tables.first().ok_or_else(issue)?;
    let column = source
        .columns
        .iter()
        .find(|column| {
            equal(
                &field.name,
                &format!(
                    "{}.{}.{}",
                    bracket(&source.name),
                    bracket(&column.name),
                    bracket(&column.name)
                ),
            )
        })
        .ok_or_else(issue)?;
    let hierarchy = format!("{}.{}", bracket(&source.name), bracket(&column.name));
    let keys = model
        .set(&format!("{hierarchy}.Children"))
        .map_err(|_| issue())?;
    if items.len() != keys.len() + 1 {
        return Err(issue());
    }
    let mut members = Vec::new();
    for (position, item) in items.iter().enumerate() {
        if item.kind == "grand" {
            if position + 1 != items.len() || item.repeat != 0 || item.data_index != 0 {
                return Err(issue());
            }
            members.push(None);
            continue;
        }
        if item.kind != "data"
            || item.repeat != 0
            || item.data_index != 0
            || item.indexes.len() != 1
        {
            return Err(issue());
        }
        let item = pivot_field
            .items
            .get(item.indexes[0] as usize)
            .ok_or_else(issue)?;
        let key = item
            .index
            .and_then(|index| field.items.get(index as usize))
            .ok_or_else(issue)?;
        let PivotItem::Text(key) = key else {
            return Err(issue());
        };
        let member = model
            .pivot_key(&hierarchy, &format!("{hierarchy}.&{}", bracket(key)))
            .map_err(|_| issue())?;
        if !keys.contains(&vec![member.clone()])
            || members.iter().any(|old| old.as_ref() == Some(&member))
        {
            return Err(issue());
        }
        members.push(Some(member));
    }
    if members.last() != Some(&None)
        || members.iter().filter(|member| member.is_some()).count() != keys.len()
    {
        return Err(issue());
    }
    Ok(Axis { hierarchy, members })
}

fn prepare(table: &PivotTable, cache: &PivotCache) -> Result<Prepared, PivotIssue> {
    let issue = || bad(table, "This model-pivot layout or snapshot is not modelled");
    let snapshot = cache.snapshot.as_ref().ok_or_else(issue)?;
    if snapshot.numeric != calc_value::NumericMode::Fast {
        return Err(bad(
            table,
            "Model-pivot arithmetic in Exact mode is not modelled",
        ));
    }
    let PivotSnapshotOrigin::DataModel { model: data } = &snapshot.origin else {
        return Err(issue());
    };
    let rows = data.snapshot.as_ref().ok_or_else(issue)?;
    if rows.tables.len() != 1
        || !matches!(&rows.tables[0].provenance,
        calc_doc_model::ModelSnapshotProvenance::WorksheetTables { revision } if *revision == snapshot.source_revision)
    {
        return Err(issue());
    }
    if data.tables.len() != 1
        || data.model_tables.len() != 1
        || !calc_value::workbook_names_equal(&data.tables[0], &rows.tables[0].name)
        || !calc_value::workbook_names_equal(&data.model_tables[0], &rows.tables[0].name)
        || !snapshot.records.is_empty()
        || data.model_connection_id.is_none()
        || cache.source_connection_id != data.model_connection_id
        || cache.source_type != "external"
        || !data.other_connections.is_empty()
        || !table.unsupported.is_empty()
        || !cache.calculated_items.is_empty()
        || cache
            .fields
            .iter()
            .any(|field| field.group.is_some() || !field.formula_items.is_empty())
        || table
            .fields
            .iter()
            .any(|field| field.sort_measure.is_some())
        || cache.unsupported.iter().any(|item| {
            !matches!(
                item.as_str(),
                "cacheHierarchies" | "dimensions" | "measureGroups" | "maps"
            )
        })
        || table.row_fields.len() != 1
        || table.column_fields.len() != 1
        || table.row_fields[0] < 0
        || table.column_fields[0] < 0
        || table.row_fields[0] == table.column_fields[0]
        || table.data_fields.len() != 1
        || !table.page_fields.is_empty()
        || !table.filters.is_empty()
        || table.data_fields[0].extended_show.is_some()
        || table.data_fields[0]
            .attributes
            .get("showDataAs")
            .is_some_and(|value| value != "normal")
        || table.data_fields[0].attributes.contains_key("numFmtId")
        || table
            .attributes
            .get("rowGrandTotals")
            .is_some_and(|value| value == "0" || value == "false")
        || table
            .attributes
            .get("colGrandTotals")
            .is_some_and(|value| value == "0" || value == "false")
        || table
            .attributes
            .get("showError")
            .is_some_and(|value| value != "0" && value != "false")
        || table
            .attributes
            .get("showMissing")
            .is_some_and(|value| value != "0" && value != "false")
    {
        return Err(issue());
    }
    let CubeLookup::Model(model) = cube_model(data) else {
        return Err(issue());
    };
    if model.tables.len() != 1 {
        return Err(issue());
    }
    let source_field = cache
        .fields
        .get(table.data_fields[0].field as usize)
        .ok_or_else(issue)?;
    let measure = model
        .pivot_measure(&source_field.name)
        .map_err(|_| issue())?;
    if table.data_fields[0]
        .name
        .as_ref()
        .is_some_and(|name| !equal(name, &model.caption(&measure)))
    {
        return Err(issue());
    }
    let rows = axis(
        table,
        cache,
        &model,
        table.row_fields[0] as usize,
        &table.row_items,
    )?;
    let columns = axis(
        table,
        cache,
        &model,
        table.column_fields[0] as usize,
        &table.col_items,
    )?;
    if table.location.get("firstHeaderRow").map(String::as_str) != Some("1")
        || table.location.get("firstDataRow").map(String::as_str) != Some("2")
        || table.location.get("firstDataCol").map(String::as_str) != Some("1")
        || table.area.row_count() != rows.members.len() as u32 + 2
        || table.area.column_count() != columns.members.len() as u32 + 1
    {
        return Err(issue());
    }
    Ok(Prepared {
        model,
        measure,
        rows,
        columns,
    })
}

pub(super) fn layout(table: &PivotTable, cache: &PivotCache) -> Result<PivotLayout, PivotIssue> {
    let p = prepare(table, cache)?;
    let first = table.area.first();
    let key = |row: u32, column: u32| {
        CellKey::new(first.sheet(), first.row() + row, first.column() + column)
            .expect("validated pivot extent")
    };
    let mut cells = BTreeMap::new();
    for row in 0..table.area.row_count() {
        for col in 0..table.area.column_count() {
            cells.insert(key(row, col), CalcValue::empty());
        }
    }
    let grand = table
        .attributes
        .get("grandTotalCaption")
        .map(String::as_str)
        .unwrap_or("Grand Total");
    cells.insert(key(0, 0), text(&p.model.caption(&p.measure)));
    cells.insert(
        key(0, 1),
        text(
            table
                .attributes
                .get("colHeaderCaption")
                .map(String::as_str)
                .unwrap_or("Column Labels"),
        ),
    );
    cells.insert(
        key(1, 0),
        text(
            table
                .attributes
                .get("rowHeaderCaption")
                .map(String::as_str)
                .unwrap_or("Row Labels"),
        ),
    );
    for (column, member) in p.columns.members.iter().enumerate() {
        cells.insert(
            key(1, column as u32 + 1),
            text(
                &member
                    .as_ref()
                    .map_or_else(|| grand.into(), |member| p.model.caption(member)),
            ),
        );
    }
    for (row, member) in p.rows.members.iter().enumerate() {
        cells.insert(
            key(row as u32 + 2, 0),
            text(
                &member
                    .as_ref()
                    .map_or_else(|| grand.into(), |member| p.model.caption(member)),
            ),
        );
        for (column, col_member) in p.columns.members.iter().enumerate() {
            let mut filters = vec![vec![p.measure.clone()]];
            if let Some(member) = member {
                filters.push(vec![member.clone()]);
            }
            if let Some(member) = col_member {
                filters.push(vec![member.clone()]);
            }
            let value = p
                .model
                .value(&filters)
                .map_err(|_| bad(table, "The model measure cannot be calculated"))?;
            let value = if matches!(value.core_ref(), CoreRef::Text(t) if t.utf16_code_units().is_empty())
            {
                // An empty intersection is blank. A measure that returns empty for existing rows
                // needs separate evidence and is not silently converted into a blank cell.
                let mut row_filters = filters[1..].to_vec();
                if row_filters.is_empty() {
                    let all = p
                        .model
                        .member(&p.rows.hierarchy)
                        .map_err(|_| bad(table, "The model hierarchy cannot be read"))?;
                    row_filters.push(vec![all]);
                }
                let exists = p
                    .model
                    .value(&row_filters)
                    .map_err(|_| bad(table, "The model intersection cannot be read"))?;
                if exists.as_number().is_some() {
                    return Err(bad(
                        table,
                        "An empty measure over existing rows is not modelled",
                    ));
                }
                CalcValue::empty()
            } else {
                value
            };
            cells.insert(key(row as u32 + 2, column as u32 + 1), value);
        }
    }
    Ok(PivotLayout {
        table: table.clone(),
        cells,
        number_formats: BTreeMap::new(),
    })
}

pub(crate) fn data_cell(
    state: &PivotState,
    data_field: &str,
    reference: &[GridArea],
    filters: &[(String, CalcValue)],
) -> Option<PivotLookup> {
    let mut tables = state.tables().filter(|table| {
        reference
            .iter()
            .any(|area| area.intersection(table.area).is_some())
    });
    let table = tables.next()?;
    if tables.next().is_some() {
        return None;
    }
    let cache = state.caches.caches.get(&table.cache_id)?;
    if !matches!(
        cache.snapshot.as_ref().map(|snapshot| &snapshot.origin),
        Some(PivotSnapshotOrigin::DataModel { .. })
    ) {
        return None;
    }
    let p = match prepare(table, cache) {
        Ok(p) => p,
        Err(_) => return Some(PivotLookup::NotModelled),
    };
    let missing = PivotLookup::Error(WorksheetErrorCode::Ref);
    let measure = match p.model.pivot_measure(data_field) {
        Ok(value) => value,
        Err(CubeFailure::Error(WorksheetErrorCode::Ref)) => return Some(missing),
        Err(_) => return Some(PivotLookup::NotModelled),
    };
    if measure != p.measure {
        return Some(missing);
    }
    let mut row = None;
    let mut column = None;
    for (hierarchy, value) in filters {
        if value.cube_value().is_some() {
            return Some(PivotLookup::NotModelled);
        }
        let CoreRef::Text(text) = value.core_ref() else {
            return Some(PivotLookup::NotModelled);
        };
        let Ok(value) = String::from_utf16(text.utf16_code_units()) else {
            return Some(PivotLookup::NotModelled);
        };
        let member = match p.model.pivot_key(hierarchy, &value) {
            Ok(value) => value,
            Err(CubeFailure::Error(WorksheetErrorCode::Ref)) => return Some(missing),
            Err(_) => return Some(PivotLookup::NotModelled),
        };
        let target = if equal(hierarchy, &p.rows.hierarchy) {
            &mut row
        } else if equal(hierarchy, &p.columns.hierarchy) {
            &mut column
        } else {
            return Some(missing);
        };
        if target.is_some() {
            return Some(PivotLookup::NotModelled);
        }
        *target = Some(member);
    }
    let locate = |axis: &Axis, wanted: &Option<CubeMember>| {
        axis.members.iter().position(|member| member == wanted)
    };
    let Some(row) = locate(&p.rows, &row) else {
        return Some(missing);
    };
    let Some(column) = locate(&p.columns, &column) else {
        return Some(missing);
    };
    let first = table.area.first();
    let Ok(at) = CellKey::new(
        first.sheet(),
        first.row() + 2 + row as u32,
        first.column() + 1 + column as u32,
    ) else {
        return Some(PivotLookup::NotModelled);
    };
    Some(if table.area.contains(at) {
        PivotLookup::Cell(at)
    } else {
        PivotLookup::NotModelled
    })
}
