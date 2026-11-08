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
    if table.fields.iter().any(|field| field.attributes.iter().any(|(name, value)|
        name.ends_with("Subtotal") && name != "defaultSubtotal" && truthy(Some(value), false))) {
        return Err(bad("Custom subtotals are not modelled"));
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
    if !table.row_fields.is_empty() && !table.column_fields.is_empty() && !table.data_fields.is_empty() {
        return layout_columns(table, cache, snapshot);
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
        "sum" | "count" | "countNums" | "min" | "max" | "average" | "product" | "stdDev" | "stdDevp" | "var" | "varp"
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
    let blank_after: Vec<bool> = fields.iter().map(|f| flag(f, "insertBlankRow", false)).collect();
    if (depth == 1 && blank_after[0]) || (1..depth.saturating_sub(1)).any(|level| blank_after[level]) {
        return Err(bad("Blank rows at this field depth are not modelled"));
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
    if !grand_total.supports(&data.subtotal) || totals.iter().any(|level|
        level.values().any(|total| !total.supports(&data.subtotal))) || (depth > 1 &&
        (!grand_total.supports_grouped_product(&data.subtotal) || totals.iter().any(|level|
            level.values().any(|total| !total.supports_grouped_product(&data.subtotal))))) {
        return Err(bad("This numeric input for the value function is not modelled"));
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
        blank_after: &blank_after,
        fill_down,
        kind,
        value_column,
        r: 1,
        printed: 0,
        cells,
        axis,
        row_paths: Vec::new(),
        collect_paths: false,
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

