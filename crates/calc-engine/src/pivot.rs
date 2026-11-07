//! Explicit worksheet pivot refresh over captured source records. This is aggregation, not a
//! second worksheet evaluator. Ordinary source edits do not refresh these snapshots.
use crate::{CalcTick, Calculation};
use calc_doc_model::{AuthoredRevision, pivot::*};
use calc_value::{CalcValue, CellKey, ExcelText, GridArea, NumericMode, WorksheetErrorCode};
use std::{cmp::Ordering, collections::BTreeMap};

pub fn refusal(part: &str, detail: impl Into<String>) -> PivotIssue {
    PivotIssue {
        part: part.into(),
        detail: detail.into(),
        owner: "dc-8wp",
    }
}
pub fn item_value(item: &PivotItem) -> Result<CalcValue, PivotIssue> {
    Ok(match item {
        PivotItem::Missing => CalcValue::empty(),
        PivotItem::Text(s) => {
            CalcValue::text(ExcelText::from_utf16_code_units(s.encode_utf16().collect()))
        }
        PivotItem::Number(s) => CalcValue::number(
            s.parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| refusal("pivot", "Invalid numeric cache item"))?,
        ),
        PivotItem::Boolean(b) => CalcValue::logical(*b),
        PivotItem::Error(s) => CalcValue::error(
            WorksheetErrorCode::all()
                .into_iter()
                .find(|e| e.excel_spelling() == s)
                .ok_or_else(|| refusal("pivot", "Unknown cache error"))?,
        ),
        PivotItem::Date(_) => CalcValue::number(
            item.date_serial()
                .ok_or_else(|| refusal("pivot", "Invalid date cache item"))?,
        ),
        _ => {
            return Err(refusal(
                "pivot",
                "Grouped, date and unknown cache items need measured refresh semantics",
            ));
        }
    })
}
fn compare(a: &PivotItem, b: &PivotItem) -> Option<Ordering> {
    use PivotItem::*;
    let rank = |x: &PivotItem| match x {
        Number(_) | Date(_) => 0,
        Text(_) => 1,
        Boolean(_) => 2,
        Error(_) => 3,
        Missing => 4,
        _ => 5,
    };
    if rank(a) != rank(b) {
        return Some(rank(a).cmp(&rank(b)));
    }
    match (a, b) {
        (Number(_) | Date(_), Number(_) | Date(_)) => {
            let value = |x: &PivotItem| match x {
                Number(n) => n.parse::<f64>().ok(),
                other => other.date_serial(),
            };
            value(a)?.partial_cmp(&value(b)?)
        }
        (Text(a), Text(b)) => calc_fml::text_order(
            &a.encode_utf16().collect::<Vec<_>>(),
            &b.encode_utf16().collect::<Vec<_>>(),
        ),
        (Boolean(a), Boolean(b)) => Some(a.cmp(b)),
        (Error(a), Error(b)) => Some(a.cmp(b)),
        (Missing, Missing) => Some(Ordering::Equal),
        _ => None,
    }
}
fn resolve<'a>(
    cache: &'a PivotCache,
    record: &'a [PivotItem],
    field: usize,
) -> Result<&'a PivotItem, PivotIssue> {
    let value = record
        .get(field)
        .ok_or_else(|| refusal(&cache.part, "A cache record has too few fields"))?;
    match value {
        PivotItem::Index(i) => cache
            .fields
            .get(field)
            .and_then(|f| f.items.get(*i as usize))
            .ok_or_else(|| refusal(&cache.part, "A cache record index is out of bounds")),
        _ => Ok(value),
    }
}
#[derive(Default)]
struct Total {
    sum: f64,
    numbers: u64,
    nonempty: u64,
    min: Option<f64>,
    max: Option<f64>,
    error: Option<WorksheetErrorCode>,
}
impl Total {
    fn push(&mut self, value: &PivotItem) -> Result<(), PivotIssue> {
        let value = item_value(value)?;
        match value.core_ref() {
            calc_value::CoreRef::Number(n) => {
                self.sum += n;
                self.numbers += 1;
                self.nonempty += 1;
                self.min = Some(self.min.map_or(n, |a| a.min(n)));
                self.max = Some(self.max.map_or(n, |a| a.max(n)));
            }
            calc_value::CoreRef::Error(e) => {
                self.nonempty += 1;
                self.error.get_or_insert(e);
            }
            calc_value::CoreRef::Text(_) | calc_value::CoreRef::Logical(_) => self.nonempty += 1,
            _ => {}
        }
        Ok(())
    }
    fn result(&self, kind: &str) -> CalcValue {
        // UI28 P6: a sum over source cells that are all empty is a blank pivot cell, not 0.
        if kind == "sum" && self.nonempty == 0 {
            return CalcValue::empty();
        }
        if !matches!(kind, "count" | "countNums")
            && let Some(e) = self.error
        {
            return CalcValue::error(e);
        }
        let n = match kind {
            "count" => self.nonempty as f64,
            "countNums" => self.numbers as f64,
            "min" => self.min.unwrap_or(0.),
            "max" => self.max.unwrap_or(0.),
            "average" if self.numbers == 0 => return CalcValue::error(WorksheetErrorCode::Div0),
            "average" => self.sum / self.numbers as f64,
            _ => self.sum,
        };
        if n.is_finite() {
            CalcValue::number(n)
        } else {
            CalcValue::error(WorksheetErrorCode::Num)
        }
    }
}
struct Group {
    key: PivotItem,
    shared: u32,
    first: usize,
    total: Total,
    old: usize,
    hidden: bool,
}
#[derive(Debug)]
pub struct PivotLayout {
    pub table: PivotTable,
    pub cells: BTreeMap<CellKey, CalcValue>,
}

/// The distinct labels of one row field in their final order (hidden items included and
/// flagged), each with the totals of its records. The order is the saved manual order, or the
/// shared text comparator ascending or descending.
fn field_groups(
    table: &PivotTable,
    cache: &PivotCache,
    snapshot: &PivotSnapshot,
    row: usize,
    measure: usize,
) -> Result<Vec<Group>, PivotIssue> {
    let bad = |why: &str| refusal(&table.part, why);
    let field = &table.fields[row];
    if field
        .attributes
        .get("showAll")
        .is_none_or(|v| v != "0" && v != "false")
    {
        return Err(bad(
            "Showing items without source data needs measured layout semantics",
        ));
    }
    let mut order: Vec<_> = (0..snapshot.records.len()).collect();
    let mut undecided = false;
    for r in &snapshot.records {
        if r.len() != cache.fields.len() {
            return Err(bad("A source record does not match the cache fields"));
        }
        let key = resolve(cache, r, row)?;
        if compare(key, key).is_none() {
            return Err(bad("The shared comparator cannot order this pivot label"));
        }
        resolve(cache, r, measure)?;
    }
    order.sort_by(|&a, &b| {
        compare(
            resolve(cache, &snapshot.records[a], row).unwrap(),
            resolve(cache, &snapshot.records[b], row).unwrap(),
        )
        .unwrap_or_else(|| {
            undecided = true;
            Ordering::Equal
        })
    });
    if undecided {
        return Err(bad("The shared comparator cannot order these pivot labels"));
    }
    let mut groups = Vec::<Group>::new();
    for source in order {
        let record = &snapshot.records[source];
        let key = resolve(cache, record, row)?;
        if groups
            .last()
            .is_none_or(|g| compare(&g.key, key) != Some(Ordering::Equal))
        {
            let PivotItem::Index(shared) = record[row] else {
                return Err(bad("A refreshed row label must name a shared cache item"));
            };
            groups.push(Group {
                key: key.clone(),
                shared,
                first: source,
                total: Total::default(),
                old: usize::MAX,
                hidden: false,
            });
        }
        let group = groups.last_mut().unwrap();
        group.total.push(resolve(cache, record, measure)?)?;
    }
    for (position, item) in field.items.iter().enumerate() {
        if item.kind != "data" {
            continue;
        }
        let Some(key) = item
            .index
            .and_then(|i| cache.fields[row].items.get(i as usize))
        else {
            return Err(bad("A saved field item has an invalid cache index"));
        };
        let mut unknown = false;
        let found = groups.binary_search_by(|g| {
            compare(&g.key, key).unwrap_or_else(|| {
                unknown = true;
                Ordering::Equal
            })
        });
        if unknown {
            return Err(bad(
                "The shared comparator cannot compare the saved item to current labels",
            ));
        }
        if let Ok(i) = found {
            groups[i].old = groups[i].old.min(position);
            groups[i].hidden |= item.hidden;
        }
    }
    match field
        .attributes
        .get("sortType")
        .map(String::as_str)
        .unwrap_or("manual")
    {
        "manual" => groups.sort_by_key(|g| (g.old, g.first)),
        "ascending" => {}
        "descending" => groups.reverse(),
        _ => return Err(bad("Unknown pivot sort order")),
    }
    Ok(groups)
}
/// The item lists of the page fields after a refresh. A page field showing (All) filters
/// nothing, so only its item list follows the cache (UI28 pvt_report_filter); a selected item or
/// a hidden item filters the source and refuses.
fn page_field_items(
    table: &PivotTable,
    cache: &PivotCache,
    snapshot: &PivotSnapshot,
    measure: usize,
    next: &mut PivotTable,
) -> Result<(), PivotIssue> {
    for &page in &table.page_fields {
        let page = page as usize;
        if page >= cache.fields.len()
            || page >= next.fields.len()
            || table.fields[page].items.iter().any(|i| i.hidden)
        {
            return Err(refusal(
                &table.part,
                "A filtered or invalid page field is not modelled",
            ));
        }
        let groups = field_groups(table, cache, snapshot, page, measure)?;
        let mut items = groups
            .iter()
            .filter(|g| !g.hidden)
            .map(|g| PivotFieldItem {
                index: Some(g.shared),
                kind: "data".into(),
                hidden: false,
            })
            .collect::<Vec<_>>();
        items.push(PivotFieldItem {
            index: None,
            kind: "default".into(),
            hidden: false,
        });
        next.fields[page].items = items;
    }
    Ok(())
}

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
    if table.row_fields.len() == 1
        && table.column_fields.len() == 1
        && (table.column_fields[0] == -2) == (table.data_fields.len() > 1)
        && !table.data_fields.is_empty()
    {
        return layout_matrix(table, cache, snapshot);
    }
    if table.row_fields.len() >= 2
        && table.column_fields.is_empty()
        && table.data_fields.len() == 1
    {
        return layout_two_rows(table, cache, snapshot);
    }
    if table.row_fields.len() != 1
        || !table.column_fields.is_empty()
        || table.data_fields.len() != 1
    {
        return Err(bad(
            "Refresh currently supports one row field and one value field; column, page and multi-field layouts remain unmeasured",
        ));
    }
    if table.location.get("firstDataRow").is_some_and(|v| v != "1")
        || table.location.get("firstDataCol").is_some_and(|v| v != "1")
    {
        return Err(bad("Other pivot header offsets remain unmeasured"));
    }
    let row = usize::try_from(table.row_fields[0]).map_err(|_| bad("The row field is invalid"))?;
    let data = &table.data_fields[0];
    let measure = data.field as usize;
    if row >= cache.fields.len()
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
    let field = &table.fields[row];
    let mut groups = field_groups(table, cache, snapshot, row, measure)?;
    groups.retain(|g| !g.hidden);
    let grand = table
        .attributes
        .get("colGrandTotals")
        .is_none_or(|v| v != "0" && v != "false");
    let height = 1 + groups.len() as u32 + u32::from(grand);
    let first = table.area.first();
    let last = CellKey::new(
        first.sheet(),
        first
            .row()
            .checked_add(height - 1)
            .ok_or_else(|| bad("Pivot output exceeds the worksheet"))?,
        first
            .column()
            .checked_add(1)
            .ok_or_else(|| bad("Pivot output exceeds the worksheet"))?,
    )
    .map_err(|_| bad("Pivot output exceeds the worksheet"))?;
    let mut next = table.clone();
    next.area = GridArea::new(first, last).map_err(|_| bad("Invalid pivot output area"))?;
    next.location.insert("firstHeaderRow".into(), "1".into());
    next.location.insert("firstDataRow".into(), "1".into());
    next.location.insert("firstDataCol".into(), "1".into());
    let mut cells = BTreeMap::new();
    let at = |row: u32, column: u32| {
        CellKey::new(first.sheet(), first.row() + row, first.column() + column)
            .expect("validated pivot bounds")
    };
    let text =
        |s: &str| CalcValue::text(ExcelText::from_utf16_code_units(s.encode_utf16().collect()));
    let row_caption = if table
        .attributes
        .get("compact")
        .is_none_or(|v| v != "0" && v != "false")
    {
        table
            .attributes
            .get("rowHeaderCaption")
            .map(String::as_str)
            .unwrap_or("Row Labels")
    } else {
        &cache.fields[row].name
    };
    cells.insert(at(0, 0), text(row_caption));
    cells.insert(
        at(0, 1),
        text(data.name.as_deref().unwrap_or(&cache.fields[measure].name)),
    );
    next.fields[row].items.clear();
    next.row_items.clear();
    for (i, group) in groups.iter().enumerate() {
        next.fields[row].items.push(PivotFieldItem {
            index: Some(group.shared),
            kind: "data".into(),
            hidden: false,
        });
        next.row_items.push(PivotAxisItem {
            kind: "data".into(),
            indexes: vec![i as u32],
            ..Default::default()
        });
        let label = if group.key == PivotItem::Missing {
            text("(blank)")
        } else {
            item_value(&group.key)?
        };
        cells.insert(at(i as u32 + 1, 0), label);
        cells.insert(at(i as u32 + 1, 1), group.total.result(&data.subtotal));
    }
    // Hidden source groups stay hidden after refresh and retain their cache identities.
    for item in field.items.iter().filter(|i| i.hidden) {
        if !next.fields[row].items.iter().any(|n| n.index == item.index) {
            next.fields[row].items.push(item.clone());
        }
    }
    if grand {
        let mut total = Total::default();
        let mut visible = groups.iter().map(|g| &g.key).collect::<Vec<_>>();
        visible.sort_by(|a, b| compare(a, b).expect("validated pivot labels"));
        for record in &snapshot.records {
            let key = resolve(cache, record, row)?;
            if visible
                .binary_search_by(|a| compare(a, key).expect("validated pivot labels"))
                .is_ok()
            {
                total.push(resolve(cache, record, measure)?)?;
            }
        }
        next.fields[row].items.push(PivotFieldItem {
            index: None,
            kind: "default".into(),
            hidden: false,
        });
        next.row_items.push(PivotAxisItem {
            kind: "grand".into(),
            indexes: vec![0],
            ..Default::default()
        });
        cells.insert(
            at(height - 1, 0),
            text(
                table
                    .attributes
                    .get("grandTotalCaption")
                    .map(String::as_str)
                    .unwrap_or("Grand Total"),
            ),
        );
        cells.insert(at(height - 1, 1), total.result(&data.subtotal));
    }
    page_field_items(table, cache, snapshot, measure, &mut next)?;
    Ok(PivotLayout { table: next, cells })
}

fn truthy(value: Option<&String>, default: bool) -> bool {
    value.map_or(default, |v| v != "0" && v != "false")
}
fn label_text(group: &Group) -> Result<String, PivotIssue> {
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
enum Form {
    Compact,
    Outline,
    Tabular,
}

/// Two or more row fields and one value field in compact, outline or tabular form, with the
/// subtotal above or below each group (or none), and optional repeated labels. UI28 P2 to P4 and
/// P14 to P17 and UI30 P8 (Excel 16.0.20527): rows, row items and item lists compared cell for
/// cell. Column fields, page fields and several value fields remain unmeasured here.
fn layout_two_rows(
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
        let mut path = Vec::with_capacity(depth);
        for (level, &field) in rows.iter().enumerate() {
            path.push(find(level, resolve(cache, record, field)?)?);
        }
        if path.iter().enumerate().any(|(l, &g)| levels[l][g].hidden) {
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
        if subtotal_on[level] {
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
        ) || d.attributes.get("showDataAs").is_some_and(|x| x != "normal")
            || d.field as usize >= cache.fields.len()
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
                    cells.insert((r, 1 + n as u32), t.result(kind));
                }
            }
            if col_grand {
                cells.insert((r, 1 + visible_cols.len() as u32), row_totals[&g].result(kind));
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
                cells.insert((r, 1 + n as u32), col_totals[&h].result(kind));
            }
            if col_grand {
                cells.insert((r, 1 + visible_cols.len() as u32), grand_total.result(kind));
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

#[derive(Clone, Debug, Default)]
pub struct PivotResults {
    pub cells: BTreeMap<CellKey, (CalcValue, Calculation)>,
    pub issues: Vec<PivotIssue>,
}
/// Rebuild only derived output; snapshot records remain the actual source readings.
pub(crate) fn calculate(state: &PivotState, tick: CalcTick) -> PivotResults {
    let mut out = PivotResults::default();
    for table in state.tables() {
        let Some(cache) = state.caches.caches.get(&table.cache_id) else {
            continue;
        };
        let Some(snapshot) = cache.snapshot.as_ref() else {
            continue;
        };
        match layout(table, cache) {
            Ok(layout) if layout.table.area == table.area => {
                let calculation = Calculation::new(
                    AuthoredRevision::new(snapshot.source_revision),
                    tick,
                    snapshot.numeric.clone(),
                );
                for (at, value) in layout.cells {
                    out.cells.insert(at, (value, calculation.clone()));
                }
            }
            Ok(_) => out.issues.push(refusal(
                &table.part,
                "The stored output extent does not match its source snapshot",
            )),
            Err(issue) => out.issues.push(issue),
        }
    }
    out
}

/// Locate a visible saved/refreshed pivot value. The function kernel reads that cell through
/// the ordinary reference reader; definitions never supply the value of GETPIVOTDATA.
pub fn data_cell(state:&PivotState,data_field:&str,reference:&[GridArea],filters:&[(String,CalcValue)])->calc_fml::PivotLookup {
    use calc_fml::PivotLookup as Lookup;
    let missing=Lookup::Error(WorksheetErrorCode::Ref);
    let mut matching=state.tables().filter(|t|reference.iter().any(|a|a.intersection(t.area).is_some()));
    let Some(table)=matching.next()else{return if state.sheets.values().any(|s|!s.issues.is_empty()){Lookup::NotModelled}else{missing};};
    // Excel chooses the most recently created table for an ambiguous range. Imported packages
    // do not establish that creation order, so preserve the refusal until it is measured.
    if matching.next().is_some(){return Lookup::NotModelled;}
    if !table.unsupported.is_empty() || table.row_fields.len()!=1 || !table.column_fields.is_empty() || !table.page_fields.is_empty() || table.data_fields.len()!=1{return Lookup::NotModelled;}
    let Some(cache)=state.caches.caches.get(&table.cache_id)else{return Lookup::NotModelled;};
    if !cache.unsupported.is_empty(){return Lookup::NotModelled;}
    let Ok(row)=usize::try_from(table.row_fields[0])else{return Lookup::NotModelled;};
    let Some(field)=cache.fields.get(row)else{return Lookup::NotModelled;};
    let data=&table.data_fields[0];let Some(value_field)=cache.fields.get(data.field as usize)else{return Lookup::NotModelled;};
    let equal=|a:&str,b:&str|calc_fml::text_order(&a.encode_utf16().collect::<Vec<_>>(),&b.encode_utf16().collect::<Vec<_>>());
    let caption=data.name.as_deref().unwrap_or(&value_field.name);
    let (caption_match,base_match)=(equal(data_field,caption),equal(data_field,&value_field.name));
    if caption_match.is_none()||base_match.is_none(){return Lookup::NotModelled;}
    if caption_match!=Some(Ordering::Equal)&&base_match!=Some(Ordering::Equal){return missing;}
    if filters.len()>1{return Lookup::NotModelled;}
    if table.fields.iter().any(|f|f.attributes.contains_key("name")){return Lookup::NotModelled;}
    let Some(items)=table.fields.get(row)else{return Lookup::NotModelled;};
    let (Some(first_row),Some(first_col))=(table.location.get("firstDataRow").and_then(|s|s.parse::<u32>().ok()),table.location.get("firstDataCol").and_then(|s|s.parse::<u32>().ok()))else{return Lookup::NotModelled;};
    if first_row!=1||first_col!=1{return Lookup::NotModelled;}
    let mut selected=None;
    if let Some((name,wanted))=filters.first(){
        match equal(name,&field.name){Some(Ordering::Equal)=>{},Some(_)=>return missing,None=>return Lookup::NotModelled}
        let wanted=match wanted.core_ref(){
            calc_value::CoreRef::Text(s)=>match String::from_utf16(s.utf16_code_units()){Ok(s)=>PivotItem::Text(s),Err(_)=>return Lookup::NotModelled},
            calc_value::CoreRef::Number(n) if n.is_finite()&&!wanted.is_excel_overflow()=>PivotItem::Number(n.to_string()),
            calc_value::CoreRef::Logical(b)=>PivotItem::Boolean(b),calc_value::CoreRef::Empty=>PivotItem::Missing,
            calc_value::CoreRef::Error(e)=>return Lookup::Error(e),_=>return Lookup::NotModelled,
        };
        for (position,axis)in table.row_items.iter().enumerate(){
            if axis.kind!="data"{continue;}
            if axis.repeat!=0||axis.data_index!=0||axis.indexes.len()!=1{return Lookup::NotModelled;}
            let Some(item)=items.items.get(axis.indexes[0] as usize)else{return Lookup::NotModelled;};
            if item.hidden||item.kind!="data"{continue;}
            let Some(key)=item.index.and_then(|i|field.items.get(i as usize))else{return Lookup::NotModelled;};
            match compare(key,&wanted){Some(Ordering::Equal)=>{if selected.replace(position as u32).is_some(){return Lookup::NotModelled;}},Some(_)=>{},None=>return Lookup::NotModelled}
        }
    }else{
        for (position,axis)in table.row_items.iter().enumerate(){if axis.kind=="grand"{if selected.replace(position as u32).is_some(){return Lookup::NotModelled;}}}
    }
    let Some(position)=selected else{return missing;};
    let Some(row)=table.area.first().row().checked_add(first_row).and_then(|r|r.checked_add(position))else{return Lookup::NotModelled;};
    let Some(col)=table.area.first().column().checked_add(first_col)else{return Lookup::NotModelled;};
    match CellKey::new(table.area.sheet(),row,col){Ok(at) if table.area.contains(at)=>Lookup::Cell(at),_=>Lookup::NotModelled}
}

fn plain_text(s: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_utf16_code_units(s.encode_utf16().collect()))
}
fn axis_item(kind: &str, repeat: u32, indexes: Vec<u32>) -> PivotAxisItem {
    PivotAxisItem {
        kind: kind.into(),
        repeat,
        indexes,
        ..Default::default()
    }
}
fn group_label(group: &Group) -> Result<CalcValue, PivotIssue> {
    if group.key == PivotItem::Missing {
        Ok(plain_text("(blank)"))
    } else {
        item_value(&group.key)
    }
}
/// The rows of a pivot with several row fields, written depth first: a group's own row (compact
/// and outline) or its first leaf row (tabular), its children, then its total row if it has one.
struct Walk<'a> {
    levels: &'a [Vec<Group>],
    /// `totals[k]` holds the total of each path of `k + 1` groups.
    totals: &'a [BTreeMap<Vec<usize>, Total>],
    position: &'a [Vec<Option<u32>>],
    form: Form,
    subtotal_top: bool,
    subtotal_on: &'a [bool],
    fill_down: bool,
    kind: &'a str,
    value_column: u32,
    r: u32,
    /// Tabular: how many leading levels of the current path already show their labels.
    printed: usize,
    cells: BTreeMap<(u32, u32), CalcValue>,
    axis: Vec<PivotAxisItem>,
}
impl Walk<'_> {
    /// The groups present under `prefix`, in the order of the next field's items.
    fn children(&self, prefix: &[usize]) -> Vec<usize> {
        let level = prefix.len();
        let mut low = prefix.to_vec();
        low.push(0);
        let mut found: Vec<usize> = self.totals[level]
            .range(low..)
            .take_while(|(k, _)| k[..level] == *prefix)
            .map(|(k, _)| k[level])
            .collect();
        found.dedup();
        found
    }
    fn visit(&mut self, path: &mut Vec<usize>) -> Result<(), PivotIssue> {
        for g in self.children(path) {
            path.push(g);
            self.group(path)?;
            path.pop();
        }
        Ok(())
    }
    fn group(&mut self, path: &mut Vec<usize>) -> Result<(), PivotIssue> {
        let level = path.len() - 1;
        let depth = self.levels.len();
        let g = path[level];
        let group = &self.levels[level][g];
        let x = self.position[level][g].expect("visible group");
        let total = self.totals[level][&*path].result(self.kind);
        let label_column = if self.form == Form::Compact { 0 } else { level as u32 };
        if level + 1 == depth {
            match self.form {
                Form::Compact | Form::Outline => {
                    self.cells.insert((self.r, label_column), group_label(group)?);
                    self.cells.insert((self.r, self.value_column), total);
                    self.axis.push(axis_item("data", level as u32, vec![x]));
                }
                Form::Tabular => {
                    // Labels print from the first level not yet shown (or every level when the
                    // labels repeat); the row item names the same levels.
                    let from = self.printed.min(level);
                    let mut indexes = Vec::new();
                    for (l, &gl) in path.iter().enumerate() {
                        if l >= from {
                            indexes.push(self.position[l][gl].expect("visible group"));
                        }
                        if l >= from || self.fill_down {
                            self.cells
                                .insert((self.r, l as u32), group_label(&self.levels[l][gl])?);
                        }
                    }
                    self.cells.insert((self.r, self.value_column), total);
                    self.axis.push(axis_item("data", from as u32, indexes));
                    self.printed = depth;
                }
            }
            self.r += 1;
            return Ok(());
        }
        match self.form {
            Form::Compact | Form::Outline => {
                self.cells.insert((self.r, label_column), group_label(group)?);
                if self.subtotal_top && self.subtotal_on[level] {
                    self.cells.insert((self.r, self.value_column), total);
                }
                self.axis.push(axis_item("data", level as u32, vec![x]));
                self.r += 1;
            }
            Form::Tabular => self.printed = self.printed.min(level),
        }
        self.visit(path)?;
        if !self.subtotal_on[level] {
            return Ok(());
        }
        let group = &self.levels[level][g];
        let total = self.totals[level][&*path].result(self.kind);
        let total_label = plain_text(&format!("{} Total", label_text(group)?));
        match self.form {
            Form::Compact | Form::Outline if !self.subtotal_top => {
                self.cells.insert((self.r, label_column), total_label);
                self.cells.insert((self.r, self.value_column), total);
                self.axis.push(axis_item("default", level as u32, vec![x]));
                self.r += 1;
            }
            Form::Tabular => {
                self.cells.insert((self.r, level as u32), total_label);
                self.cells.insert((self.r, self.value_column), total);
                self.axis.push(axis_item("default", level as u32, vec![x]));
                self.r += 1;
                self.printed = level;
            }
            _ => {}
        }
        Ok(())
    }
}
