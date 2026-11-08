use super::*;

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
pub(super) fn compare(a: &PivotItem, b: &PivotItem) -> Option<Ordering> {
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
pub(super) fn resolve<'a>(
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
/// The distinct labels of one row field in their final order (hidden items included and
/// flagged), each with the totals of its records. The order is the saved manual order, or the
/// shared text comparator ascending or descending.
pub(super) fn field_groups(
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
                excluded: false,
            });
        }
        let group = groups.last_mut().unwrap();
        if page_allows(table, cache, record)? {
            group.total.push(resolve(cache, record, measure)?)?;
        }
    }
    // Grouped dates retain boundary and empty-period items for their saved axis indexes.
    // The walk emits only paths with source records.
    if cache.fields[row].group.as_ref().is_some_and(|group| group.range.is_some()) {
        let mut extras = Vec::new();
        let mut indexes = std::collections::BTreeSet::new();
        for item in field.items.iter().filter(|item| item.kind == "data") {
            let shared = item.index.ok_or_else(|| bad("A grouped item has no cache index"))?;
            if !indexes.insert(shared) { return Err(bad("A grouped item is repeated")); }
            let key = cache.fields[row].items.get(shared as usize)
                .ok_or_else(|| bad("A grouped item index is invalid"))?;
            let mut unknown = false;
            let found = groups.binary_search_by(|group| compare(&group.key, key).unwrap_or_else(|| {
                unknown = true;
                Ordering::Equal
            })).is_ok();
            if unknown || compare(key, key).is_none() { return Err(bad("Grouped items cannot be compared")); }
            if !found {
                extras.push(Group { key: key.clone(), shared, first: usize::MAX,
                    total: Total::default(), old: usize::MAX, hidden: false, excluded: false });
            }
        }
        groups.extend(extras);
        let mut unknown = false;
        groups.sort_by(|a, b| compare(&a.key, &b.key).unwrap_or_else(|| {
            unknown = true;
            Ordering::Equal
        }));
        if unknown || groups.windows(2).any(|pair| compare(&pair[0].key, &pair[1].key) == Some(Ordering::Equal)) {
            return Err(bad("Grouped items cannot be ordered uniquely"));
        }
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
    if let Some(filter) = table.filters.iter().find(|filter| filter.field as usize == row) {
        let kind = table.data_fields.get(filter.measure as usize)
            .ok_or_else(|| bad("Invalid value-filter measure"))?.subtotal.as_str();
        let mut scores = Vec::new();
        for (i, group) in groups.iter().enumerate().filter(|(_, group)| !group.hidden) {
            match group.total.result(kind).core_ref() {
                calc_value::CoreRef::Number(n) if n.is_finite() => scores.push((i, n)),
                _ => return Err(bad("A nonnumeric value-filter total is not modelled")),
            }
        }
        match &filter.kind {
            PivotFilterKind::TopCount(count) => {
                scores.sort_by(|a, b| b.1.total_cmp(&a.1));
                let n = (*count as usize).min(scores.len());
                if n > 0 && n < scores.len() && scores[n-1].1 == scores[n].1 {
                    return Err(bad("A tie at the top-count boundary is not modelled"));
                }
                for &(i, _) in scores.iter().skip(n) { groups[i].excluded = true; }
            }
            PivotFilterKind::ValueGreaterThan(value) => {
                let threshold = value.parse::<f64>().ok().filter(|n| n.is_finite())
                    .ok_or_else(|| bad("Invalid value-filter threshold"))?;
                for (i, score) in scores { groups[i].excluded = score <= threshold; }
            }
            PivotFilterKind::Unsupported => return Err(bad("This pivot filter is not modelled")),
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
/// nothing, so its item list follows the cache. Selected or hidden page items keep their saved
/// positions, which the page field uses to identify its selection.
pub(super) fn page_field_items(
    table: &PivotTable,
    cache: &PivotCache,
    snapshot: &PivotSnapshot,
    measure: usize,
    next: &mut PivotTable,
) -> Result<(), PivotIssue> {
    for selection in &table.page_fields {
        let page = selection.field as usize;
        if selection.selected_item.is_some() || table.fields.get(page).is_some_and(|f| f.items.iter().any(|i| i.hidden)) {
            // Selected positions refer to the preserved list. Reordering that list would change
            // the selection written in the original pageField XML.
            if page >= cache.fields.len() || page >= next.fields.len() {
                return Err(refusal(&table.part, "An invalid page field is not modelled"));
            }
            let indices: std::collections::BTreeSet<u32> = table.fields[page].items.iter()
                .filter(|item| item.kind == "data").filter_map(|item| item.index).collect();
            for record in &snapshot.records {
                let key = resolve(cache, record, page)?;
                let known = matches!(record.get(page), Some(PivotItem::Index(i)) if indices.contains(i))
                    || indices.iter().filter_map(|i| cache.fields[page].items.get(*i as usize))
                        .any(|item| compare(item, key) == Some(Ordering::Equal));
                if !known { return Err(refusal(&table.part, "A new item in a filtered page field is not modelled")); }
            }
            continue;
        }
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


/// Select source records for report-filter fields without changing the shared source snapshot.
pub(super) fn page_allows(
    table: &PivotTable, cache: &PivotCache, record: &[PivotItem],
) -> Result<bool, PivotIssue> {
    for page in &table.page_fields {
        let index = page.field as usize;
        let field = table.fields.get(index).ok_or_else(|| refusal(&table.part, "Invalid page field"))?;
        let cache_field = cache.fields.get(index).ok_or_else(|| refusal(&table.part, "Invalid page cache field"))?;
        let key = resolve(cache, record, index)?;
        let equal = |item: &PivotFieldItem| -> Result<bool, PivotIssue> {
            let saved = item.index.and_then(|i| cache_field.items.get(i as usize))
                .ok_or_else(|| refusal(&table.part, "Invalid page item"))?;
            compare(saved, key).map(|o| o == Ordering::Equal)
                .ok_or_else(|| refusal(&table.part, "A page item cannot be compared"))
        };
        if let Some(position) = page.selected_item {
            let item = field.items.get(position as usize).filter(|i| i.kind == "data" && !i.hidden)
                .ok_or_else(|| refusal(&table.part, "Invalid selected page item"))?;
            if !equal(item)? { return Ok(false); }
        } else if field.items.iter().any(|i| i.hidden) {
            let mut found = None;
            for item in field.items.iter().filter(|i| i.kind == "data") {
                if equal(item)? {
                    if found.is_some_and(|hidden| hidden != item.hidden) {
                        return Err(refusal(&table.part, "Conflicting page item visibility is not modelled"));
                    }
                    found = Some(item.hidden);
                }
            }
            match found {
                Some(true) => return Ok(false),
                Some(false) => {},
                None => return Err(refusal(&table.part, "A new item in a filtered page field is not modelled")),
            }
        }
    }
    Ok(true)
}
