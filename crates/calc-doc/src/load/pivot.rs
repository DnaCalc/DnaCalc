//! Saved pivot metadata. Cache records and unknown fields remain in the retained OPC package;
//! reading a definition neither refreshes it nor promotes saved output to calculated values.
use super::{Events, LoadError, MAIN_NAMESPACE as M, read_xml};
use crate::{
    opc::{Package, TargetMode},
    xml::{XmlElement, XmlEvent},
};
use calc_doc_model::value::{CellKey, GridArea, SheetId};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub use calc_doc_model::pivot::*;
fn decoded(value:&str,events:&Events<'_>)->Result<String,LoadError>{String::from_utf16(&super::strings::decode(value)).map_err(|_|events.invalid("A pivot string has an unpaired UTF-16 code unit"))}
const X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const FILL_DOWN_URI: &str = "{2946ED86-A175-432a-8AC1-64E0C546D7DE}";
fn attrs(e: &XmlElement) -> BTreeMap<String, String> {
    e.attributes
        .iter()
        .filter(|a| a.name.namespace.is_none())
        .map(|a| (a.name.local.to_string(), a.value.clone()))
        .collect()
}
fn rid(e: &XmlElement) -> Option<&str> {
    e.attributes
        .iter()
        .find(|a| a.name.is(R, "id"))
        .map(|a| a.value.as_str())
}
fn required<'a>(e: &'a XmlElement, key: &str, events: &Events<'_>) -> Result<&'a str, LoadError> {
    e.attribute(key)
        .ok_or_else(|| events.invalid("A required pivot attribute is missing"))
}
fn integer<T: std::str::FromStr>(
    e: &XmlElement,
    key: &str,
    events: &Events<'_>,
) -> Result<T, LoadError> {
    required(e, key, events)?
        .parse()
        .map_err(|_| events.invalid("A pivot index is invalid"))
}
fn optional_integer<T: std::str::FromStr>(
    e: &XmlElement,
    key: &str,
    events: &Events<'_>,
) -> Result<Option<T>, LoadError> {
    e.attribute(key)
        .map(|s| {
            s.parse()
                .map_err(|_| events.invalid("A pivot count is invalid"))
        })
        .transpose()
}
fn related(package: &Package, part: &str, id: &str, kind: &str) -> Option<String> {
    package
        .relationships_of(Some(part))?
        .items()
        .iter()
        .find(|r| r.id == id && r.kind == format!("{R}/{kind}") && r.mode == TargetMode::Internal)?
        .resolved
        .clone()
}
fn issue(part: &str, error: impl std::fmt::Display) -> PivotIssue {
    PivotIssue {
        part: part.into(),
        detail: error.to_string(),
        owner: "dc-8wp",
    }
}

/// Read the declared cache identities, not every part whose name resembles a cache.
pub(crate) fn read_caches(package: &Package, workbook: &str) -> PivotCaches {
    let mut out = PivotCaches::default();
    let mut seen = BTreeSet::new();
    let result = (|| -> Result<(), LoadError> {
        let xml = read_xml(package, workbook)?;
        let mut events = Events::new(&xml);
        events.root(M, "workbook", "The pivot owner is not a workbook")?;
        loop {
            match events.next()? {
                Some(XmlEvent::Start(e)) if e.name.is(M, "pivotCaches") => {
                    loop {
                        match events.next()? {
                            Some(XmlEvent::Start(e)) if e.name.is(M, "pivotCache") => {
                                let id = integer::<u32>(&e, "cacheId", &events)?;
                                let part = rid(&e).and_then(|id| {
                                    related(package, workbook, id, "pivotCacheDefinition")
                                });
                                events.skip()?;
                                if !seen.insert(id) {
                                    out.caches.remove(&id);
                                    out.issues.push(issue(workbook, "Duplicate pivot cache ID"));
                                    continue;
                                }
                                match part{
                        Some(part)=>match read_cache(package,&part,id){Ok(cache)=>{if out.caches.insert(id,Arc::new(cache)).is_some(){out.issues.push(issue(&part,"Duplicate pivot cache ID"));out.caches.remove(&id);}},Err(e)=>out.issues.push(issue(&part,e))},
                        None=>out.issues.push(issue(workbook,format!("Pivot cache {id} has no internal definition relationship"))),
                    }
                            }
                            Some(XmlEvent::Start(_)) => events.skip()?,
                            Some(XmlEvent::End(_)) => break,
                            Some(XmlEvent::Text(_)) => {}
                            None => {
                                return Err(events.invalid("The workbook ends inside pivotCaches"));
                            }
                        }
                    }
                }
                Some(XmlEvent::Start(_)) => events.skip()?,
                Some(XmlEvent::End(_)) => break,
                Some(XmlEvent::Text(_)) => {}
                None => return Err(events.invalid("The workbook ends before its close")),
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        out.issues.push(issue(workbook, e));
    }
    out
}
fn read_cache(package: &Package, part: &str, id: u32) -> Result<PivotCache, LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    let root = events.root(
        M,
        "pivotCacheDefinition",
        "The cache part has no pivotCacheDefinition root",
    )?;
    let records_part = rid(&root).and_then(|id| related(package, part, id, "pivotCacheRecords"));
    if rid(&root).is_some() && records_part.is_none() {
        return Err(events.invalid("The pivot cache records relationship is unavailable"));
    }
    let mut cache = PivotCache {
        id,
        part: part.into(),
        records_part,
        record_count: optional_integer(&root, "recordCount", &events)?,
        source_type: String::new(),
        source_sheet: None,
        source_range: None,
        source_name: None,
        fields: Vec::new(),
        attributes: attrs(&root),
        unsupported: Vec::new(),
        snapshot: None,
    };
    let mut path = Vec::<String>::new();
    let mut item_count = 0usize;
    let mut group_items_seen = false;
    let mut discrete_count = None;
    while let Some(event) = events.next()? {
        match event {
            XmlEvent::Start(e) => {
                if e.name.namespace.as_deref() == Some(M) {
                    match (
                        path.iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .as_slice(),
                        e.name.local.as_str(),
                    ) {
                        ([], "calculatedItems" | "calculatedMembers" | "cacheHierarchies" | "dimensions" | "measureGroups" | "maps")
                        | (["cacheFields", "cacheField"], "mpMap" | "extLst") => cache.unsupported.push(e.name.local.to_string()),
                        ([], "cacheSource") => {
                            cache.source_type = required(&e, "type", &events)?.into()
                        }
                        (["cacheSource"], "worksheetSource") => {
                            cache.source_sheet = e.attribute("sheet").map(str::to_owned);
                            cache.source_range = e.attribute("ref").map(str::to_owned);
                            cache.source_name = e.attribute("name").map(str::to_owned);
                        }
                        (["cacheFields"], "cacheField") => { group_items_seen = false; discrete_count = None; if e.attribute("formula").is_some() {cache.unsupported.push("calculated cache field".into());} cache.fields.push(PivotCacheField {
                            name: decoded(required(&e, "name", &events)?,&events)?,
                            shared_items: None,
                            group: None,
                            items: Vec::new(),
                            unused_items: Vec::new(),
                            attributes: attrs(&e),
                        }); },
                        (["cacheFields", "cacheField"], "fieldGroup") => {
                            if cache.fields.last().is_some_and(|field| field.group.is_some()) {
                                cache.unsupported.push("duplicate field group".into());
                            }
                            if let Some(field) = cache.fields.last_mut() {
                                field.group = Some(PivotCacheGroup { attributes: attrs(&e), range: None, discrete: None });
                            }
                        }
                        (["cacheFields", "cacheField", "fieldGroup"], "rangePr") => {
                            if cache.fields.last().and_then(|field| field.group.as_ref()).is_some_and(|group| group.range.is_some()) {
                                cache.unsupported.push("duplicate grouping range".into());
                            }
                            if let Some(group) = cache.fields.last_mut().and_then(|field| field.group.as_mut()) {
                                group.range = Some(attrs(&e));
                            }
                        }
                        (["cacheFields", "cacheField", "fieldGroup"], "discretePr") => {
                            if e.attributes.iter().any(|attr| attr.name.local != "count" || attr.name.namespace.is_some())
                                || cache.fields.last().and_then(|field| field.group.as_ref()).is_none_or(|group| group.discrete.is_some()) {
                                cache.unsupported.push("unsupported discrete grouping".into());
                            }
                            discrete_count = optional_integer::<u32>(&e, "count", &events)?.map(|count| count as usize);
                            if let Some(group) = cache.fields.last_mut().and_then(|field| field.group.as_mut()) {
                                group.discrete = Some(Vec::new());
                            }
                        }
                        (["cacheFields", "cacheField", "fieldGroup", "discretePr"], "x") => {
                            if e.attributes.iter().any(|attr| attr.name.local != "v" || attr.name.namespace.is_some()) {
                                cache.unsupported.push("unsupported discrete grouping item".into());
                            }
                            let index = integer(&e, "v", &events)?;
                            if let Some(discrete) = cache.fields.last_mut().and_then(|field| field.group.as_mut()).and_then(|group| group.discrete.as_mut()) {
                                if discrete.len() >= 100_000 { return Err(events.invalid("Pivot grouping exceeds 100,000 items (dc-8wp)")); }
                                discrete.push(index);
                            }
                        }
                        (["cacheFields", "cacheField", "fieldGroup"], "groupItems") => {
                            if group_items_seen { cache.unsupported.push("duplicate group items".into()); }
                            if cache.fields.last().is_none_or(|field| !matches!(field.attributes.get("databaseField").map(String::as_str), Some("0" | "false"))) {
                                cache.unsupported.push("group items on a source field".into());
                            }
                            group_items_seen = true;
                        }
                        (["cacheFields", "cacheField", "fieldGroup", "groupItems"], _) => {
                            if item_count >= 100_000 { return Err(events.invalid("Pivot group item reading exceeds 100,000 items (dc-8wp)")); }
                            item_count += 1;
                            if let Some(field) = cache.fields.last_mut() { field.items.push(item(&e, &events)?); }
                            events.skip()?;
                            continue;
                        }
                        (["cacheFields", "cacheField", "fieldGroup"], child) if child != "groupItems" => {
                            cache.unsupported.push(format!("unsupported field group {child}"));
                        }
                        (["cacheFields", "cacheField"], "sharedItems") => {
                            if cache.fields.last().is_some_and(|field| matches!(field.attributes.get("databaseField").map(String::as_str), Some("0" | "false"))) {
                                cache.unsupported.push("shared items on a grouped field".into());
                            }
                            if let Some(field) = cache.fields.last_mut() {
                                field.shared_items = optional_integer(&e, "count", &events)?;
                            }
                        }
                        (["cacheFields", "cacheField", "sharedItems"], _) => {
                            if item_count >= 100_000 {
                                return Err(events.invalid(
                                    "Pivot shared item reading exceeds 100,000 items (dc-8wp)",
                                ));
                            }
                            item_count += 1;
                            if e.attribute("u").is_some_and(|value| !matches!(value, "0" | "1" | "false" | "true")) {
                                cache.unsupported.push("invalid unused-item flag".into());
                            }
                            if let Some(field) = cache.fields.last_mut() {
                                if matches!(e.attribute("u"), Some("1" | "true")) {
                                    field.unused_items.push(field.items.len() as u32);
                                }
                                field.items.push(item(&e, &events)?);
                            }
                            events.skip()?;
                            continue;
                        }
                        _ => {}
                    }
                }
                path.push(if e.name.namespace.as_deref() == Some(M) {
                    e.name.local.to_string()
                } else {
                    "#other-namespace".into()
                });
            }
            XmlEvent::End(_) => {
                if path.iter().map(String::as_str).collect::<Vec<_>>() == ["cacheFields", "cacheField", "fieldGroup", "discretePr"] {
                    let actual = cache.fields.last().and_then(|field| field.group.as_ref())
                        .and_then(|group| group.discrete.as_ref()).map(Vec::len);
                    if discrete_count.is_none() || discrete_count != actual {
                        cache.unsupported.push("invalid discrete grouping count".into());
                    }
                }
                if path.pop().is_none() {
                    break;
                }
            }
            XmlEvent::Text(_) => {}
        }
    }
    for (field_index, field) in cache.fields.iter().enumerate() {
        let virtual_field = matches!(field.attributes.get("databaseField").map(String::as_str), Some("0" | "false"));
        if let Some(group) = &field.group {
            if group.attributes.keys().any(|name| name != "base" && name != "par") {
                cache.unsupported.push("unsupported field-group attributes".into());
            }
            if virtual_field {
                let supported = !group.attributes.contains_key("par") && group.discrete.is_none()
                    && group.attributes.get("base").and_then(|v| v.parse::<usize>().ok()).is_some()
                    && group.range.as_ref().is_some_and(|range| {
                        matches!(range.get("groupBy").map(String::as_str), Some("days" | "months" | "quarters" | "years"))
                            && range.contains_key("startDate") && range.contains_key("endDate")
                            && ["autoStart", "autoEnd"].iter().all(|key| range.get(*key).is_none_or(|v|
                                matches!(v.as_str(), "0" | "1" | "false" | "true")))
                            && range.keys().all(|k| matches!(k.as_str(), "groupBy" | "startDate" | "endDate" | "autoStart" | "autoEnd" | "groupInterval"))
                    }) && field.items.iter().all(|item| matches!(item, PivotItem::Text(_)))
                    && group.range.as_ref().is_some_and(|range| match range.get("groupBy").map(String::as_str) {
                        Some("days") if !range.contains_key("groupInterval") => field.items.len() == 368,
                        Some("days") => {
                            let fixed = |key: &str| matches!(range.get(key).map(String::as_str), Some("0" | "false"));
                            let bound = |key: &str| range.get(key).and_then(|value| PivotItem::Date(value.clone()).date_serial());
                            match (bound("startDate"), bound("endDate")) {
                                (Some(start), Some(end)) if range.get("groupInterval").is_some_and(|v| v == "7")
                                    && fixed("autoStart") && fixed("autoEnd") && start.fract() == 0.0
                                    && end.fract() == 0.0 && end >= start =>
                                        (((end - start) / 7.0).floor() as usize).checked_add(3) == Some(field.items.len()),
                                _ => false,
                            }
                        }
                        Some("months") => !range.contains_key("groupInterval") && field.items.len() == 14,
                        Some("quarters") => !range.contains_key("groupInterval") && field.items.len() == 6,
                        Some("years") if !range.contains_key("groupInterval") => {
                            let year = |key: &str| range.get(key).and_then(|s| s.split('-').next()).and_then(|s| s.parse::<i64>().ok());
                            match (year("startDate"), year("endDate")) {
                                (Some(first), Some(last)) if (1900..=9999).contains(&first) && (first..=9999).contains(&last) => usize::try_from(last - first + 3).ok() == Some(field.items.len()),
                                _ => false,
                            }
                        }
                        _ => false,
                    });
                let discrete_supported = !group.attributes.contains_key("par") && group.range.is_none()
                    && group.attributes.get("base").and_then(|value| value.parse::<usize>().ok())
                        .and_then(|base| cache.fields.get(base)).is_some_and(|base| {
                            base.group.as_ref().and_then(|parent| parent.attributes.get("par"))
                                .and_then(|value| value.parse::<usize>().ok()) == Some(field_index)
                                && base.items.iter().all(|item| matches!(item, PivotItem::Text(_) | PivotItem::Missing))
                                && group.discrete.as_ref().is_some_and(|mapping| mapping.len() == base.items.len()
                                    && mapping.iter().all(|&index| (index as usize) < field.items.len()))
                        }) && field.items.iter().all(|item| matches!(item, PivotItem::Text(_) | PivotItem::Missing));
                if !supported && !discrete_supported { cache.unsupported.push("unsupported grouping".into()); }
            } else if group.range.is_some() || group.discrete.is_some() || group.attributes.contains_key("base")
                || group.attributes.get("par").and_then(|v| v.parse::<usize>().ok()).is_none_or(|i| i >= cache.fields.len()) {
                cache.unsupported.push("unsupported source-field grouping".into());
            }
        } else if virtual_field {
            cache.unsupported.push("calculated cache field".into());
        }
    }
    Ok(cache)
}

/// Read the pivotParts collection in the worksheet's existing event stream.
pub(crate) fn read_parts(
    package: &Package,
    sheet_part: &str,
    sheet: SheetId,
    events: &mut Events<'_>,
    out: &mut SheetPivots,
) -> Result<(), LoadError> {
    let mut seen = out
        .tables
        .iter()
        .map(|t| t.part.clone())
        .collect::<BTreeSet<_>>();
    loop {
        match events.next()? {
            Some(XmlEvent::Start(e)) if e.name.is(M, "pivotPart") => {
                let part = rid(&e).and_then(|id| related(package, sheet_part, id, "pivotTable"));
                events.skip()?;
                match part {
                    Some(part) if !seen.insert(part.clone()) => out
                        .issues
                        .push(issue(&part, "Duplicate pivot part reference")),
                    Some(part) => match read_table(package, &part, sheet) {
                        Ok(table) => out.tables.push(table),
                        Err(e) => out.issues.push(issue(&part, e)),
                    },
                    None => out.issues.push(issue(
                        sheet_part,
                        "The saved pivot has no internal definition relationship",
                    )),
                }
            }
            Some(XmlEvent::Start(_)) => events.skip()?,
            Some(XmlEvent::End(_)) => return Ok(()),
            Some(XmlEvent::Text(_)) => {}
            None => return Err(events.invalid("The worksheet ends inside pivotParts")),
        }
    }
}

/// Worksheet pivots are implicit relationship targets in SpreadsheetML. Retain support for
/// the older synthetic fixture's explicit list, without reading any definition twice.
/// https://learn.microsoft.com/en-us/office/open-xml/spreadsheet/working-with-pivottables
pub(crate) fn read_related(package:&Package,sheet_part:&str,sheet:SheetId,out:&mut SheetPivots){
    let mut seen=out.tables.iter().map(|t|t.part.clone()).chain(out.issues.iter().map(|i|i.part.clone())).collect::<BTreeSet<_>>();
    for rel in package.relationships_of(Some(sheet_part)).into_iter().flat_map(|r|r.items()).filter(|r|r.kind==format!("{R}/pivotTable")){
        let Some(part)=rel.resolved.as_deref().filter(|_|rel.mode==TargetMode::Internal)else{out.issues.push(issue(sheet_part,"A pivot relationship has no internal definition target"));continue;};
        if seen.insert(part.to_owned()){match read_table(package,part,sheet){Ok(table)=>out.tables.push(table),Err(e)=>out.issues.push(issue(part,e)),}}
    }
}

fn read_table(package: &Package, part: &str, sheet: SheetId) -> Result<PivotTable, LoadError> {
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    let root = events.root(
        M,
        "pivotTableDefinition",
        "The table part has no pivotTableDefinition root",
    )?;
    let name = decoded(required(&root, "name", &events)?,&events)?;
    let cache_id = integer(&root, "cacheId", &events)?;
    let mut location = None;
    let mut location_attrs = BTreeMap::new();
    let mut fields = Vec::<PivotField>::new();
    let mut row_items = Vec::<PivotAxisItem>::new();
    let mut col_items = Vec::<PivotAxisItem>::new();
    let mut unsupported = Vec::new();
    let mut row_fields = Vec::new();
    let mut column_fields = Vec::new();
    let mut page_fields = Vec::new();
    let mut filters = Vec::<PivotFilter>::new();
    let mut invalid_filters = Vec::<bool>::new();
    let mut filter_type = String::new();
    let mut auto_filters = 0usize;
    let mut filter_columns = 0usize;
    let mut data_fields = Vec::new();
    let mut path = Vec::<String>::new();
    // The supported scope selects a single data field over the whole opposite axis.
    let mut sort_scope: Option<(usize, bool, Option<u32>)> = None;
    let mut data_extension: Option<(usize, bool, Option<String>)> = None;
    while let Some(event) = events.next()? {
        match event {
            XmlEvent::Start(e) => {
                let filter_path: Vec<_> = path.iter().map(String::as_str).collect();
                if let Some((step, invalid, selected)) = sort_scope.as_mut() {
                    let expected = match (*step, filter_path.as_slice(), e.name.local.as_str()) {
                        (0, ["pivotFields", "pivotField", "autoSortScope"], "pivotArea") =>
                            e.attributes.len() == 3 && e.attribute("dataOnly") == Some("0")
                                && e.attribute("outline") == Some("0") && e.attribute("fieldPosition") == Some("0"),
                        (1, ["pivotFields", "pivotField", "autoSortScope", "pivotArea"], "references") =>
                            e.attributes.len() == 1 && e.attribute("count") == Some("1"),
                        (2, ["pivotFields", "pivotField", "autoSortScope", "pivotArea", "references"], "reference") =>
                            e.attributes.len() == 3 && e.attribute("field") == Some("4294967294")
                                && e.attribute("count") == Some("1") && e.attribute("selected") == Some("0"),
                        (3, ["pivotFields", "pivotField", "autoSortScope", "pivotArea", "references", "reference"], "x") => {
                            *selected = e.attribute("v").and_then(|v| v.parse().ok());
                            e.attributes.len() == 1 && selected.is_some()
                        }
                        _ => false,
                    };
                    *invalid |= !expected || e.name.namespace.as_deref() != Some(M)
                        || e.attributes.iter().any(|a| a.name.namespace.is_some());
                    *step += 1;
                }
                if let Some((step, invalid, show)) = data_extension.as_mut() {
                    let known = match (*step, filter_path.as_slice(), e.name.local.as_str()) {
                        (0, ["dataFields", "dataField", "extLst"], "ext") =>
                            e.name.namespace.as_deref() == Some(M) && e.attributes.len() == 1
                                && e.attribute("uri") == Some("{E15A36E0-9728-4e99-A89B-3F7291B0FE68}"),
                        (1, ["dataFields", "dataField", "extLst", "ext"], "dataField") => {
                            *show = e.attribute("pivotShowAs").filter(|v| matches!(*v,
                                "percentOfParentRow" | "percentOfParentCol" | "percentOfRunningTotal" | "rankAscending" | "rankDescending"))
                                .map(str::to_owned);
                            e.name.namespace.as_deref() == Some(X14) && e.attributes.len() == 1 && show.is_some()
                        }
                        _ => false,
                    };
                    *invalid |= !known || e.attributes.iter().any(|a| a.name.namespace.is_some());
                    *step += 1;
                }
                if filter_path.starts_with(&["filters", "filter"]) {
                    let known = e.name.namespace.as_deref() == Some(M) && matches!(
                        (filter_path.as_slice(), e.name.local.as_str()),
                        (["filters", "filter"], "autoFilter") |
                        (["filters", "filter", "autoFilter"], "filterColumn") |
                        (["filters", "filter", "autoFilter", "filterColumn"], "top10" | "customFilters") |
                        (["filters", "filter", "autoFilter", "filterColumn", "customFilters"], "customFilter")
                    );
                    if !known { if let Some(invalid) = invalid_filters.last_mut() { *invalid = true; } }
                }
                if e.name.namespace.as_deref() == Some(X14)
                    && e.name.local == "pivotField"
                    && path.iter().map(String::as_str).collect::<Vec<_>>() == ["pivotFields", "pivotField", "extLst", "ext"]
                {
                    // x14:pivotField carries Repeat All Item Labels as fillDownLabels.
                    let known = e.attributes.iter().all(|a| a.name.local == "fillDownLabels");
                    match (known, e.attribute("fillDownLabels"), fields.last_mut()) {
                        (true, Some(v), Some(field)) => {
                            field.attributes.insert("fillDownLabels".into(), v.into());
                        }
                        (true, None, Some(_)) => {}
                        _ => unsupported.push("extLst".into()),
                    }
                }
                if e.name.namespace.as_deref() == Some(M) {
                    match (
                        path.iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .as_slice(),
                        e.name.local.as_str(),
                    ) {
                        ([], "location") => {
                            location_attrs = attrs(&e);
                            if location.is_some() {
                                return Err(events.invalid("Duplicate pivot locations"));
                            }
                            location = Some({
                                let (r, c, rr, cc) =
                                    super::address::area(required(&e, "ref", &events)?)
                                        .ok_or_else(|| {
                                            events.invalid("The pivot location is invalid")
                                        })?;
                                let a = CellKey::new(sheet, r, c).map_err(|_| {
                                    events.invalid("The pivot location is outside the worksheet")
                                })?;
                                let b = CellKey::new(sheet, rr, cc).map_err(|_| {
                                    events.invalid("The pivot location is outside the worksheet")
                                })?;
                                GridArea::new(a, b)
                                    .map_err(|_| events.invalid("The pivot location is invalid"))?
                            });
                        }
                        (["pivotFields"], "pivotField") => fields.push(PivotField { attributes:attrs(&e),items:Vec::new(),sort_measure:None }),
                        (["pivotFields","pivotField","items"],"item") => {
                            if e.attribute("n").is_some(){unsupported.push("custom item captions".into());}
                            if e.attribute("m").is_some_and(|value| !matches!(value, "0" | "1" | "false" | "true")) {
                                unsupported.push("invalid missing-item flag".into());
                            }
                            if let Some(field)=fields.last_mut() {field.items.push(PivotFieldItem {index:optional_integer(&e,"x",&events)?,kind:e.attribute("t").unwrap_or("data").into(),hidden:matches!(e.attribute("h"),Some("1"|"true")),missing:matches!(e.attribute("m"),Some("1"|"true"))});}
                        }
                        (["rowItems"],"i") => row_items.push(PivotAxisItem {kind:e.attribute("t").unwrap_or("data").into(),repeat:optional_integer(&e,"r",&events)?.unwrap_or(0),data_index:optional_integer(&e,"i",&events)?.unwrap_or(0),indexes:Vec::new()}),
                        (["colItems"],"i") => col_items.push(PivotAxisItem {kind:e.attribute("t").unwrap_or("data").into(),repeat:optional_integer(&e,"r",&events)?.unwrap_or(0),data_index:optional_integer(&e,"i",&events)?.unwrap_or(0),indexes:Vec::new()}),
                        (["colItems","i"],"x") => {if let Some(item)=col_items.last_mut(){item.indexes.push(optional_integer(&e,"v",&events)?.unwrap_or(0));}}
                        (["rowItems","i"],"x") => {if let Some(item)=row_items.last_mut(){item.indexes.push(optional_integer(&e,"v",&events)?.unwrap_or(0));}}
                        ([],"conditionalFormats"|"rowHierarchies"|"colHierarchies"|"calculatedMembers")
                        | (["pivotFields","pivotField"],"memberProperties") => unsupported.push(e.name.local.to_string()),
                        (["pivotFields","pivotField"],"autoSortScope") => {
                            if sort_scope.is_some() || !e.attributes.is_empty()
                                || fields.last().is_none_or(|field| field.sort_measure.is_some()) {
                                unsupported.push("unsupported value sort".into());
                            }
                            sort_scope = Some((0, false, None));
                        }
                        (["pivotFields","pivotField"],"extLst") => {}
                        (["pivotFields","pivotField","extLst"],"ext") => {
                            if e.attribute("uri") != Some(FILL_DOWN_URI) { unsupported.push("extLst".into()); }
                        }
                        (["rowFields"], "field") => row_fields.push(integer(&e, "x", &events)?),
                        (["colFields"], "field") => column_fields.push(integer(&e, "x", &events)?),
                        (["pageFields"], "pageField") => {
                            if e.attribute("hier").is_some_and(|h| h != "-1") {
                                unsupported.push("hierarchical page field".into());
                            }
                            page_fields.push(PivotPageField {
                                field: integer(&e, "fld", &events)?,
                                selected_item: optional_integer(&e, "item", &events)?,
                            })
                        }
                        (["filters"], "filter") => {
                            filter_type = e.attribute("type").unwrap_or("").into();
                            auto_filters = 0;
                            filter_columns = 0;
                            invalid_filters.push(e.attributes.iter().any(|a| !matches!(a.name.local.as_str(),
                                "fld" | "type" | "evalOrder" | "id" | "iMeasureFld"))
                                || e.attribute("evalOrder").is_some_and(|v| v != "-1"));
                            filters.push(PivotFilter {
                                field: integer(&e, "fld", &events)?,
                                measure: optional_integer(&e, "iMeasureFld", &events)?.unwrap_or(u32::MAX),
                                kind: PivotFilterKind::Unsupported,
                            });
                        }
                        (["filters"], _) => unsupported.push("unsupported pivot filter element".into()),
                        (["filters", "filter"], "autoFilter") => {
                            auto_filters += 1;
                            if auto_filters != 1 || e.attributes.iter().any(|a| a.name.local != "ref") {
                                if let Some(invalid) = invalid_filters.last_mut() { *invalid = true; }
                            }
                        }
                        (["filters", "filter", "autoFilter"], "filterColumn") => {
                            filter_columns += 1;
                            if filter_columns != 1 || e.attribute("colId") != Some("0") || e.attributes.iter().any(|a| a.name.local != "colId") {
                                if let Some(invalid) = invalid_filters.last_mut() { *invalid = true; }
                            }
                        }
                        (["filters", "filter", "autoFilter", "filterColumn"], "customFilters") => {
                            if e.attributes.iter().any(|a| a.name.local != "and") || matches!(e.attribute("and"), Some("1" | "true")) {
                                if let Some(invalid) = invalid_filters.last_mut() { *invalid = true; }
                            }
                        }
                        (["filters", "filter", "autoFilter", "filterColumn"], "top10") => {
                            if e.attributes.iter().any(|a| !matches!(a.name.local.as_str(), "val" | "filterVal" | "top" | "percent")) {
                                if let Some(invalid) = invalid_filters.last_mut() { *invalid = true; }
                            }
                            if filters.last().is_some_and(|f| !matches!(f.kind, PivotFilterKind::Unsupported)) {
                                if let Some(invalid) = invalid_filters.last_mut() { *invalid = true; }
                            }
                            if filter_type == "count" && !matches!(e.attribute("top"), Some("0" | "false"))
                                && !matches!(e.attribute("percent"), Some("1" | "true")) {
                                if let (Some(filter), Some(count)) = (filters.last_mut(), e.attribute("val").and_then(|v| v.parse::<u32>().ok()).filter(|n| *n > 0)) {
                                    filter.kind = PivotFilterKind::TopCount(count);
                                }
                            }
                        }
                        (["filters", "filter", "autoFilter", "filterColumn", "customFilters"], "customFilter") => {
                            if e.attributes.iter().any(|a| !matches!(a.name.local.as_str(), "operator" | "val"))
                                || filters.last().is_some_and(|f| !matches!(f.kind, PivotFilterKind::Unsupported)) {
                                if let Some(invalid) = invalid_filters.last_mut() { *invalid = true; }
                            }
                            if filter_type == "valueGreaterThan" && e.attribute("operator") == Some("greaterThan") {
                                if let (Some(filter), Some(value)) = (filters.last_mut(), e.attribute("val")) {
                                    filter.kind = PivotFilterKind::ValueGreaterThan(value.into());
                                }
                            }
                        }
                        (["dataFields", "dataField"], "extLst") => {
                            if data_extension.is_some() || !e.attributes.is_empty()
                                || data_fields.last().is_none_or(|field: &PivotDataField| field.extended_show.is_some()
                                    || field.attributes.get("showDataAs").is_some_and(|v| v != "normal")) {
                                unsupported.push("data field extension".into());
                            }
                            data_extension = Some((0, false, None));
                        },
                        (["dataFields"], "dataField") => data_fields.push(PivotDataField {
                            name: e.attribute("name").map(|s|decoded(s,&events)).transpose()?,
                            field: integer(&e, "fld", &events)?,
                            subtotal: e.attribute("subtotal").unwrap_or("sum").into(),
                            attributes: attrs(&e),
                            extended_show: None,
                        }),
                        _ => {}
                    }
                }
                path.push(if e.name.namespace.as_deref() == Some(M) {
                    e.name.local.to_string()
                } else {
                    "#other-namespace".into()
                });
            }
            XmlEvent::End(_) => {
                if path.iter().map(String::as_str).collect::<Vec<_>>() == ["dataFields", "dataField", "extLst"] {
                    match data_extension.take() {
                        Some((2, false, Some(show))) => data_fields.last_mut().unwrap().extended_show = Some(show),
                        _ => unsupported.push("data field extension".into()),
                    }
                }
                if path.iter().map(String::as_str).collect::<Vec<_>>() == ["pivotFields", "pivotField", "autoSortScope"] {
                    match sort_scope.take() {
                        Some((4, false, Some(measure))) => fields.last_mut().unwrap().sort_measure = Some(measure),
                        _ => unsupported.push("unsupported value sort".into()),
                    }
                }
                if path.pop().is_none() {
                    break;
                }
            }
            XmlEvent::Text(_) => {}
        }
    }
    if invalid_filters.iter().any(|invalid| *invalid)
        || filters.iter().any(|f| matches!(f.kind, PivotFilterKind::Unsupported)) {
        unsupported.push("unsupported pivot filter".into());
    }
    Ok(PivotTable {
        name,
        part: part.into(),
        cache_id,
        area: location.ok_or_else(|| events.invalid("The pivot has no saved location"))?,
        row_fields,
        column_fields,
        page_fields,
        filters,
        data_fields,
        attributes: attrs(&root),
        location: location_attrs, fields, row_items, col_items, unsupported,
    })
}

fn item(e: &XmlElement, events: &Events<'_>) -> Result<PivotItem, LoadError> {
    Ok(match e.name.local.as_str() {
        "m" => PivotItem::Missing,
        "n" => PivotItem::Number(required(e, "v", events)?.into()),
        "s" => PivotItem::Text(decoded(required(e, "v", events)?,events)?),
        "d" => PivotItem::Date(required(e, "v", events)?.into()),
        "e" => PivotItem::Error(required(e, "v", events)?.into()),
        "x" => PivotItem::Index(integer(e, "v", events)?),
        "b" => PivotItem::Boolean(match required(e, "v", events)? {
            "1" | "true" => true,
            "0" | "false" => false,
            _ => return Err(events.invalid("A pivot Boolean is invalid")),
        }),
        _ => PivotItem::Other {
            namespace: e.name.namespace.as_deref().map(str::to_owned),
            element: e.name.local.to_string(),
            attributes: attrs(e),
        },
    })
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PivotRecords {
    pub declared_count: Option<u64>,
    pub records: Vec<Vec<PivotItem>>,
    pub truncated: bool,
}
/// Bounded inspection of records; opening a workbook never expands the records part.
pub(crate) fn read_records(
    package: &Package,
    cache: &PivotCache,
    max_records: usize,
) -> Result<PivotRecords, PivotIssue> {
    let part = cache
        .records_part
        .as_deref()
        .ok_or_else(|| issue(&cache.part, "This cache has no saved records part"))?;
    let read = (|| -> Result<PivotRecords, LoadError> {
        let xml = read_xml(package, part)?;
        let mut events = Events::new(&xml);
        let root = events.root(
            M,
            "pivotCacheRecords",
            "The records part has no pivotCacheRecords root",
        )?;
        let mut result = PivotRecords {
            declared_count: optional_integer(&root, "count", &events)?,
            records: Vec::new(),
            truncated: false,
        };
        let mut items = 0usize;
        loop {
            match events.next()? {
                Some(XmlEvent::Start(e)) if e.name.is(M, "r") => {
                    if result.records.len() >= max_records.min(100_000) {
                        result.truncated = true;
                        return Ok(result);
                    }
                    let mut row = Vec::new();
                    loop {
                        match events.next()? {
                            Some(XmlEvent::Start(e)) => {
                                if items >= 1_000_000 {
                                    return Err(events.invalid("Pivot records exceed the 1,000,000-item reading budget (dc-8wp)"));
                                }
                                items += 1;
                                row.push(if e.name.namespace.as_deref() == Some(M) {
                                    item(&e, &events)?
                                } else {
                                    PivotItem::Other {
                                        namespace: e.name.namespace.as_deref().map(str::to_owned),
                                        element: e.name.local.to_string(),
                                        attributes: attrs(&e),
                                    }
                                });
                                events.skip()?;
                            }
                            Some(XmlEvent::End(_)) => break,
                            Some(XmlEvent::Text(_)) => {}
                            None => return Err(events.invalid("The records end inside a row")),
                        }
                    }
                    result.records.push(row);
                }
                Some(XmlEvent::Start(_)) => events.skip()?,
                Some(XmlEvent::End(_)) => break,
                Some(XmlEvent::Text(_)) => {}
                None => return Err(events.invalid("The records end before their close")),
            }
        }
        Ok(result)
    })();
    read.map_err(|e| issue(part, e))
}
