use super::*;

/// The first refresh scope: one row field, one measure, no column/page/group/calculated fields.
/// It preserves manual item order and applies saved ascending/descending sorts through the
/// shared text comparator. Multi-field compact/outline/tabular layouts remain unmeasured.
pub fn layout(table: &PivotTable, cache: &PivotCache) -> Result<PivotLayout, PivotIssue> {
    let bad = |why: &str| refusal(&table.part, why);
    let snapshot = cache
        .snapshot
        .as_ref()
        .ok_or_else(|| bad("No explicit source refresh has run"))?;
    if snapshot.numeric != NumericMode::Fast {
        return Err(bad("Exact pivot aggregation needs Excel measurements"));
    }
    if !table.unsupported.is_empty() || !cache.unsupported.is_empty() {
        return Err(bad(
            "Grouped, calculated, filtered or extended pivot layouts are not modelled",
        ));
    }
    if !table.filters.is_empty() && (table.filters.len() != 1 || table.row_fields.len() != 1
        || !table.column_fields.is_empty() || table.data_fields.len() != 1
        || table.filters[0].field as i32 != table.row_fields[0] || table.filters[0].measure != 0
        || table.page_fields.iter().any(|page| page.field as i32 == table.row_fields[0])) {
        return Err(bad("This pivot value filter layout is not modelled"));
    }
    if table.row_fields.len() == 1 && table.column_fields.len() >= 2 {
        return layout_columns(table, cache, snapshot);
    }
    if table.row_fields.len() == 1
        && table.column_fields.len() == 1
        && (table.column_fields[0] == -2) == (table.data_fields.len() > 1)
        && !table.data_fields.is_empty()
    {
        return layout_matrix(table, cache, snapshot);
    }
    if !table.row_fields.is_empty()
        && table.column_fields.is_empty()
        && table.data_fields.len() == 1
    {
        return layout_rows(table, cache, snapshot);
    }
    Err(bad("This pivot axis layout is not modelled"))
}

pub(super) fn truthy(value: Option<&String>, default: bool) -> bool {
    value.map_or(default, |v| v != "0" && v != "false")
}
pub(super) fn label_text(group: &Group) -> Result<String, PivotIssue> {
    Ok(match &group.key {
        PivotItem::Missing => "(blank)".into(),
        PivotItem::Text(s) | PivotItem::Number(s) | PivotItem::Error(s) => s.clone(),
        PivotItem::Boolean(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        _ => {
            return Err(refusal(
                "pivot",
                "Grouped, date and unknown cache items need measured refresh semantics",
            ));
        }
    })
}
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Form {
    Compact,
    Outline,
    Tabular,
}

/// Two or more row fields and one value field in compact, outline or tabular form, with the
/// subtotal above or below each group (or none), and optional repeated labels. UI28 P2 to P4 and
/// P14 to P17 and UI30 P8 (Excel 16.0.20527): rows, row items and item lists compared cell for
/// cell. Column fields, page fields and several value fields remain unmeasured here.
fn layout_rows(
    table: &PivotTable,
    cache: &PivotCache,
    snapshot: &PivotSnapshot,
) -> Result<PivotLayout, PivotIssue> {
    let bad = |why: &str| refusal(&table.part, why);
    let rows = table
        .row_fields
        .iter()
        .map(|&r| usize::try_from(r).map_err(|_| bad("A row field is invalid")))
        .collect::<Result<Vec<_>, _>>()?;
    let depth = rows.len();
    let data = &table.data_fields[0];
    let measure = data.field as usize;
    if rows.iter().any(|&r| r >= cache.fields.len())
        || rows
            .iter()
            .enumerate()
            .any(|(i, r)| rows[..i].contains(r))
        || measure >= cache.fields.len()
        || table.fields.len() != cache.fields.len()
    {
        return Err(bad("Pivot field indexes do not match the source cache"));
    }
    if !matches!(
        data.subtotal.as_str(),
        "sum" | "count" | "countNums" | "min" | "max" | "average"
    ) || data
        .attributes
        .get("showDataAs")
        .is_some_and(|x| x != "normal")
    {
        return Err(bad(
            "This value aggregation or Show Values As mode is not modelled",
        ));
    }
    let fields: Vec<&PivotField> = rows.iter().map(|&r| &table.fields[r]).collect();
    let flag = |f: &PivotField, name: &str, default: bool| truthy(f.attributes.get(name), default);
    for name in ["compact", "outline", "subtotalTop"] {
        if fields
            .iter()
            .any(|f| flag(f, name, true) != flag(fields[0], name, true))
        {
            return Err(bad("Row fields in different layout forms remain unmeasured"));
        }
    }
    // A field without its automatic subtotal (UI30 P8): compact and outline show the group's
    // row with no value, tabular has no total row for it. Only the inner levels count, and a
    // two-field pivot without one stays unmeasured.
    let subtotal_on: Vec<bool> = fields
        .iter()
        .map(|f| flag(f, "defaultSubtotal", true))
        .collect();
    if depth == 2 && subtotal_on.iter().any(|on| !on) {
        return Err(bad("Row fields without a subtotal remain unmeasured"));
    }
    let form = if flag(fields[0], "compact", true) {
        Form::Compact
    } else if flag(fields[0], "outline", true) {
        Form::Outline
    } else {
        Form::Tabular
    };
    let subtotal_top = flag(fields[0], "subtotalTop", true) && form != Form::Tabular;
    let fill_down = flag(fields[0], "fillDownLabels", false);
    let levels = rows
        .iter()
        .map(|&r| field_groups(table, cache, snapshot, r, measure))
        .collect::<Result<Vec<_>, _>>()?;
    let sorted = levels
        .iter()
        .map(|groups| {
            let mut idx: Vec<usize> = (0..groups.len()).collect();
            idx.sort_by(|&a, &b| {
                compare(&groups[a].key, &groups[b].key).unwrap_or(Ordering::Equal)
            });
            idx
        })
        .collect::<Vec<_>>();
    let find = |level: usize, key: &PivotItem| -> Result<usize, PivotIssue> {
        sorted[level]
            .binary_search_by(|&i| compare(&levels[level][i].key, key).unwrap_or(Ordering::Equal))
            .map(|at| sorted[level][at])
            .map_err(|_| bad("A source label is missing from its row field"))
    };
    // The total of every group path: `totals[k]` holds the paths of `k + 1` groups.
    let mut totals: Vec<BTreeMap<Vec<usize>, Total>> = (0..depth).map(|_| BTreeMap::new()).collect();
    let mut grand_total = Total::default();
    for record in &snapshot.records {
        if !page_allows(table, cache, record)? { continue; }
        let mut path = Vec::with_capacity(depth);
        for (level, &field) in rows.iter().enumerate() {
            path.push(find(level, resolve(cache, record, field)?)?);
        }
        if path.iter().enumerate().any(|(l, &g)| levels[l][g].hidden || levels[l][g].excluded) {
            continue;
        }
        let value = resolve(cache, record, measure)?;
        for (k, totals) in totals.iter_mut().enumerate() {
            totals.entry(path[..=k].to_vec()).or_default().push(value)?;
        }
        grand_total.push(value)?;
    }
    let mut position = Vec::new();
    for groups in &levels {
        let mut next = 0u32;
        let mut column = Vec::new();
        for group in groups {
            column.push(if group.hidden {
                None
            } else {
                next += 1;
                Some(next - 1)
            });
        }
        position.push(column);
    }
    let label_columns: u32 = if form == Form::Compact { 1 } else { depth as u32 };
    let value_column = label_columns;
    let first = table.area.first();
    let text =
        |s: &str| CalcValue::text(ExcelText::from_utf16_code_units(s.encode_utf16().collect()));
    let mut cells = BTreeMap::<(u32, u32), CalcValue>::new();
    let caption = data.name.as_deref().unwrap_or(&cache.fields[measure].name);
    if form == Form::Compact {
        let compact_caption = table
            .attributes
            .get("rowHeaderCaption")
            .map(String::as_str)
            .unwrap_or("Row Labels");
        cells.insert((0, 0), text(compact_caption));
    } else {
        for (level, &field) in rows.iter().enumerate() {
            cells.insert((0, level as u32), text(&cache.fields[field].name));
        }
    }
    cells.insert((0, value_column), text(caption));
    let axis = Vec::<PivotAxisItem>::new();
    let kind = data.subtotal.as_str();
    if depth > 64 {
        return Err(bad("More than 64 row fields remain unmeasured"));
    }
    let mut walk = Walk {
        levels: &levels,
        totals: &totals,
        position: &position,
        form,
        subtotal_top,
        subtotal_on: &subtotal_on,
        fill_down,
        kind,
        value_column,
        r: 1,
        printed: 0,
        cells,
        axis,
    };
    walk.visit(&mut Vec::new())?;
    let mut r = walk.r;
    let (mut cells, mut axis) = (walk.cells, walk.axis);
    if truthy(table.attributes.get("colGrandTotals"), true) {
        let grand_caption = table
            .attributes
            .get("grandTotalCaption")
            .map(String::as_str)
            .unwrap_or("Grand Total");
        cells.insert((r, 0), text(grand_caption));
        cells.insert((r, value_column), grand_total.result(kind));
        axis.push(axis_item("grand", 0, vec![0]));
        r += 1;
    }
    let last = CellKey::new(
        first.sheet(),
        first
            .row()
            .checked_add(r - 1)
            .ok_or_else(|| bad("Pivot output exceeds the worksheet"))?,
        first
            .column()
            .checked_add(value_column)
            .ok_or_else(|| bad("Pivot output exceeds the worksheet"))?,
    )
    .map_err(|_| bad("Pivot output exceeds the worksheet"))?;
    let mut next = table.clone();
    next.area = GridArea::new(first, last).map_err(|_| bad("Invalid pivot output area"))?;
    next.location.insert("firstHeaderRow".into(), "1".into());
    next.location.insert("firstDataRow".into(), "1".into());
    next.location
        .insert("firstDataCol".into(), label_columns.to_string());
    for (level, &field) in rows.iter().enumerate() {
        let mut items = Vec::new();
        for (i, group) in levels[level].iter().enumerate() {
            if position[level][i].is_some() {
                items.push(PivotFieldItem {
                    index: Some(group.shared),
                    kind: "data".into(),
                    hidden: false,
                });
            }
        }
        for old in table.fields[field].items.iter().filter(|i| i.hidden) {
            items.push(old.clone());
        }
        // A field without its automatic subtotal has no default item (UI30 P8).
        if if depth == 1 { truthy(table.attributes.get("colGrandTotals"), true) } else { subtotal_on[level] } {
            items.push(PivotFieldItem {
                index: None,
                kind: "default".into(),
                hidden: false,
            });
        }
        next.fields[field].items = items;
    }
    next.row_items = axis;
    // Every cell of the output rectangle is pivot-owned, the blank ones included.
    for row in 0..r {
        for column in 0..=value_column {
            cells.entry((row, column)).or_insert_with(CalcValue::empty);
        }
    }
    let cells = cells
        .into_iter()
        .map(|((row, column), value)| {
            (
                CellKey::new(first.sheet(), first.row() + row, first.column() + column)
                    .expect("validated pivot bounds"),
                value,
            )
        })
        .collect();
    page_field_items(table, cache, snapshot, measure, &mut next)?;
    Ok(PivotLayout { table: next, cells })
}

/// One row field with either one column field and one value field, or several value fields
/// placed across the columns (the Values pseudo-field, field -2). UI28 P5, P7 and P13 (Excel
/// 16.0.20527). Page fields, two or more column fields and column subtotals remain unmeasured.
fn layout_matrix(
    table: &PivotTable,
    cache: &PivotCache,
    snapshot: &PivotSnapshot,
) -> Result<PivotLayout, PivotIssue> {
    let bad = |why: &str| refusal(&table.part, why);
    let row = usize::try_from(table.row_fields[0]).map_err(|_| bad("The row field is invalid"))?;
    let values_across = table.column_fields == [-2];
    let col = if values_across {
        None
    } else {
        Some(usize::try_from(table.column_fields[0]).map_err(|_| bad("The column field is invalid"))?)
    };
    if table.data_fields.iter().any(|d| {
        !matches!(
            d.subtotal.as_str(),
            "sum" | "count" | "countNums" | "min" | "max" | "average"
        ) || d.attributes.get("showDataAs").is_some_and(|x| {
            x != "normal" && !(x == "percentOfCol" && !values_across)
        }) || d.field as usize >= cache.fields.len()
    }) || (values_across != (table.data_fields.len() > 1))
        || row >= cache.fields.len()
        || col.is_some_and(|c| c >= cache.fields.len() || c == row)
        || table.fields.len() != cache.fields.len()
    {
        return Err(bad("This pivot layout or value aggregation is not modelled"));
    }
    let measure = table.data_fields[0].field as usize;
    let row_field = &table.fields[row];
    if !truthy(row_field.attributes.get("defaultSubtotal"), true) {
        return Err(bad("Row fields without a subtotal remain unmeasured"));
    }
    let rows = field_groups(table, cache, snapshot, row, measure)?;
    let cols = match col {
        Some(c) => field_groups(table, cache, snapshot, c, measure)?,
        None => Vec::new(),
    };
    let find = |groups: &[Group], key: &PivotItem| -> Result<usize, PivotIssue> {
        groups
            .iter()
            .position(|g| compare(&g.key, key) == Some(Ordering::Equal))
            .ok_or_else(|| bad("A source label is missing from its field"))
    };
    let mut cell_totals = BTreeMap::<(usize, usize), Total>::new();
    let mut row_totals = BTreeMap::<usize, Total>::new();
    let mut col_totals = BTreeMap::<usize, Total>::new();
    let mut grand_total = Total::default();
    let mut value_rows = BTreeMap::<usize, Vec<Total>>::new();
    let mut value_grand: Vec<Total> = table.data_fields.iter().map(|_| Total::default()).collect();
    for record in &snapshot.records {
        if !page_allows(table, cache, record)? { continue; }
        let g = find(&rows, resolve(cache, record, row)?)?;
        if rows[g].hidden {
            continue;
        }
        if let Some(c) = col {
            let h = find(&cols, resolve(cache, record, c)?)?;
            if cols[h].hidden {
                continue;
            }
            let value = resolve(cache, record, measure)?;
            cell_totals.entry((g, h)).or_default().push(value)?;
            row_totals.entry(g).or_default().push(value)?;
            col_totals.entry(h).or_default().push(value)?;
            grand_total.push(value)?;
        } else {
            let totals = value_rows
                .entry(g)
                .or_insert_with(|| table.data_fields.iter().map(|_| Total::default()).collect());
            for (k, d) in table.data_fields.iter().enumerate() {
                let value = resolve(cache, record, d.field as usize)?;
                totals[k].push(value)?;
                value_grand[k].push(value)?;
            }
        }
    }
    let position = |groups: &[Group]| {
        let mut next = 0u32;
        groups
            .iter()
            .map(|g| {
                if g.hidden {
                    None
                } else {
                    next += 1;
                    Some(next - 1)
                }
            })
            .collect::<Vec<_>>()
    };
    let row_position = position(&rows);
    let col_position = position(&cols);
    let visible_cols = cols
        .iter()
        .enumerate()
        .filter(|(h, _)| !cols[*h].hidden && col_totals.contains_key(h))
        .map(|(h, _)| h)
        .collect::<Vec<_>>();
    let text =
        |s: &str| CalcValue::text(ExcelText::from_utf16_code_units(s.encode_utf16().collect()));
    let row_grand = truthy(table.attributes.get("colGrandTotals"), true);
    let col_grand = truthy(table.attributes.get("rowGrandTotals"), true) && !values_across;
    let compact = truthy(table.attributes.get("compact"), true);
    let caption = |d: &PivotDataField| {
        d.name
            .clone()
            .unwrap_or_else(|| cache.fields[d.field as usize].name.clone())
    };
    let header_rows = if values_across { 1u32 } else { 2 };
    let mut cells = BTreeMap::<(u32, u32), CalcValue>::new();
    let row_caption = if compact {
        table
            .attributes
            .get("rowHeaderCaption")
            .cloned()
            .unwrap_or_else(|| "Row Labels".into())
    } else {
        cache.fields[row].name.clone()
    };
    let grand_caption = table
        .attributes
        .get("grandTotalCaption")
        .cloned()
        .unwrap_or_else(|| "Grand Total".into());
    let label = |group: &Group| -> Result<CalcValue, PivotIssue> {
        if group.key == PivotItem::Missing {
            Ok(text("(blank)"))
        } else {
            item_value(&group.key)
        }
    };
    let mut col_items = Vec::<PivotAxisItem>::new();
    let kind = table.data_fields[0].subtotal.as_str();
    let percent = table.data_fields[0].attributes.get("showDataAs")
        .is_some_and(|v| v == "percentOfCol");
    if percent && (visible_cols.iter().any(|h| !matches!(
        col_totals[h].result(kind).core_ref(), calc_value::CoreRef::Number(n) if n != 0.0
    )) || (col_grand && !matches!(
        grand_total.result(kind).core_ref(), calc_value::CoreRef::Number(n) if n != 0.0
    )) || cell_totals.values().chain(row_totals.values()).any(|total| matches!(
        total.result(kind).core_ref(), calc_value::CoreRef::Error(_)
    ))) {
        return Err(bad("Percent of column with zero or error totals is not modelled"));
    }
    let result = |value: &Total, denominator: &Total| {
        let value = value.result(kind);
        if !percent { return value; }
        let divisor = denominator.result(kind);
        match (value.core_ref(), divisor.core_ref()) {
            (calc_value::CoreRef::Empty, _) => CalcValue::empty(),
            (calc_value::CoreRef::Error(e), _) | (_, calc_value::CoreRef::Error(e)) => CalcValue::error(e),
            (calc_value::CoreRef::Number(_), calc_value::CoreRef::Number(0.0)) => CalcValue::error(WorksheetErrorCode::Div0),
            (calc_value::CoreRef::Number(n), calc_value::CoreRef::Number(d)) => {
                let ratio = n / d;
                if ratio.is_finite() { CalcValue::number(ratio) } else { CalcValue::error(WorksheetErrorCode::Num) }
            },
            _ => CalcValue::empty(),
        }
    };
    if values_across {
        cells.insert((0, 0), text(&row_caption));
        for (k, d) in table.data_fields.iter().enumerate() {
            cells.insert((0, 1 + k as u32), text(&caption(d)));
            col_items.push(PivotAxisItem {
                kind: "data".into(),
                data_index: k as u32,
                indexes: vec![k as u32],
                ..Default::default()
            });
        }
    } else {
        cells.insert((0, 0), text(&caption(&table.data_fields[0])));
        let column_caption = table
            .attributes
            .get("colHeaderCaption")
            .cloned()
            .unwrap_or_else(|| if compact { "Column Labels".into() } else { cache.fields[col.unwrap()].name.clone() });
        cells.insert((0, 1), text(&column_caption));
        cells.insert((1, 0), text(&row_caption));
        for (n, &h) in visible_cols.iter().enumerate() {
            cells.insert((1, 1 + n as u32), label(&cols[h])?);
            col_items.push(PivotAxisItem {
                kind: "data".into(),
                indexes: vec![col_position[h].expect("visible group")],
                ..Default::default()
            });
        }
        if col_grand {
            cells.insert((1, 1 + visible_cols.len() as u32), text(&grand_caption));
            col_items.push(PivotAxisItem {
                kind: "grand".into(),
                indexes: vec![0],
                ..Default::default()
            });
        }
    }
    let mut axis = Vec::<PivotAxisItem>::new();
    let mut r = header_rows;
    for (g, group) in rows.iter().enumerate().filter(|(_, g)| !g.hidden) {
        if col.is_some() && !row_totals.contains_key(&g) || col.is_none() && !value_rows.contains_key(&g) {
            continue;
        }
        cells.insert((r, 0), label(group)?);
        if col.is_some() {
            for (n, &h) in visible_cols.iter().enumerate() {
                if let Some(t) = cell_totals.get(&(g, h)) {
                    cells.insert((r, 1 + n as u32), result(t, &col_totals[&h]));
                } else if percent {
                    cells.insert((r, 1 + n as u32), CalcValue::number(0.));
                }
            }
            if col_grand {
                cells.insert((r, 1 + visible_cols.len() as u32), result(&row_totals[&g], &grand_total));
            }
        } else {
            for (k, d) in table.data_fields.iter().enumerate() {
                cells.insert((r, 1 + k as u32), value_rows[&g][k].result(&d.subtotal));
            }
        }
        axis.push(PivotAxisItem {
            kind: "data".into(),
            indexes: vec![row_position[g].expect("visible group")],
            ..Default::default()
        });
        r += 1;
    }
    if row_grand {
        cells.insert((r, 0), text(&grand_caption));
        if col.is_some() {
            for (n, &h) in visible_cols.iter().enumerate() {
                cells.insert((r, 1 + n as u32), result(&col_totals[&h], &col_totals[&h]));
            }
            if col_grand {
                cells.insert((r, 1 + visible_cols.len() as u32), result(&grand_total, &grand_total));
            }
        } else {
            for (k, d) in table.data_fields.iter().enumerate() {
                cells.insert((r, 1 + k as u32), value_grand[k].result(&d.subtotal));
            }
        }
        axis.push(PivotAxisItem {
            kind: "grand".into(),
            indexes: vec![0],
            ..Default::default()
        });
        r += 1;
    }
    let value_columns = if values_across {
        table.data_fields.len() as u32
    } else {
        visible_cols.len() as u32 + u32::from(col_grand)
    };
    let first = table.area.first();
    let last = CellKey::new(
        first.sheet(),
        first
            .row()
            .checked_add(r - 1)
            .ok_or_else(|| bad("Pivot output exceeds the worksheet"))?,
        first
            .column()
            .checked_add(value_columns)
            .ok_or_else(|| bad("Pivot output exceeds the worksheet"))?,
    )
    .map_err(|_| bad("Pivot output exceeds the worksheet"))?;
    let mut next = table.clone();
    next.area = GridArea::new(first, last).map_err(|_| bad("Invalid pivot output area"))?;
    next.location.insert(
        "firstHeaderRow".into(),
        if values_across { "0" } else { "1" }.into(),
    );
    next.location.insert(
        "firstDataRow".into(),
        if values_across { "1" } else { "2" }.into(),
    );
    next.location.insert("firstDataCol".into(), "1".into());
    let rebuild = |field: usize, groups: &[Group], positions: &[Option<u32>]| {
        let mut items = Vec::new();
        for (i, group) in groups.iter().enumerate() {
            if positions[i].is_some() {
                items.push(PivotFieldItem {
                    index: Some(group.shared),
                    kind: "data".into(),
                    hidden: false,
                });
            }
        }
        for old in table.fields[field].items.iter().filter(|i| i.hidden) {
            items.push(old.clone());
        }
        items.push(PivotFieldItem {
            index: None,
            kind: "default".into(),
            hidden: false,
        });
        items
    };
    next.fields[row].items = rebuild(row, &rows, &row_position);
    if let Some(c) = col {
        next.fields[c].items = rebuild(c, &cols, &col_position);
    }
    next.row_items = axis;
    next.col_items = col_items;
    for row in 0..r {
        for column in 0..=value_columns {
            cells.entry((row, column)).or_insert_with(CalcValue::empty);
        }
    }
    let cells = cells
        .into_iter()
        .map(|((row, column), value)| {
            (
                CellKey::new(first.sheet(), first.row() + row, first.column() + column)
                    .expect("validated pivot bounds"),
                value,
            )
        })
        .collect();
    page_field_items(table, cache, snapshot, measure, &mut next)?;
    Ok(PivotLayout { table: next, cells })
}

