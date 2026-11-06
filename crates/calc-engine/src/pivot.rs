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
        Number(_) => 0,
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
        (Number(a), Number(b)) => a.parse::<f64>().ok()?.partial_cmp(&b.parse::<f64>().ok()?),
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
    if table.row_fields.len() != 1
        || !table.column_fields.is_empty()
        || !table.page_fields.is_empty()
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
