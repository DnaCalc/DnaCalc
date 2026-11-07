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
                        | (["cacheFields", "cacheField"], "fieldGroup" | "mpMap" | "extLst") => cache.unsupported.push(e.name.local.to_string()),
                        ([], "cacheSource") => {
                            cache.source_type = required(&e, "type", &events)?.into()
                        }
                        (["cacheSource"], "worksheetSource") => {
                            cache.source_sheet = e.attribute("sheet").map(str::to_owned);
                            cache.source_range = e.attribute("ref").map(str::to_owned);
                            cache.source_name = e.attribute("name").map(str::to_owned);
                        }
                        (["cacheFields"], "cacheField") => { if e.attribute("formula").is_some() || matches!(e.attribute("databaseField"),Some("0"|"false")) {cache.unsupported.push("calculated cache field".into());} cache.fields.push(PivotCacheField {
                            name: decoded(required(&e, "name", &events)?,&events)?,
                            shared_items: None,
                            items: Vec::new(),
                            attributes: attrs(&e),
                        }); },
                        (["cacheFields", "cacheField"], "sharedItems") => {
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
                            if let Some(field) = cache.fields.last_mut() {
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
                if path.pop().is_none() {
                    break;
                }
            }
            XmlEvent::Text(_) => {}
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
    let mut data_fields = Vec::new();
    let mut path = Vec::<String>::new();
    while let Some(event) = events.next()? {
        match event {
            XmlEvent::Start(e) => {
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
                        (["pivotFields"], "pivotField") => fields.push(PivotField { attributes:attrs(&e),items:Vec::new() }),
                        (["pivotFields","pivotField","items"],"item") => {
                            if e.attribute("n").is_some(){unsupported.push("custom item captions".into());}
                            if let Some(field)=fields.last_mut() {field.items.push(PivotFieldItem {index:optional_integer(&e,"x",&events)?,kind:e.attribute("t").unwrap_or("data").into(),hidden:matches!(e.attribute("h"),Some("1"|"true"))});}
                        }
                        (["rowItems"],"i") => row_items.push(PivotAxisItem {kind:e.attribute("t").unwrap_or("data").into(),repeat:optional_integer(&e,"r",&events)?.unwrap_or(0),data_index:optional_integer(&e,"i",&events)?.unwrap_or(0),indexes:Vec::new()}),
                        (["colItems"],"i") => col_items.push(PivotAxisItem {kind:e.attribute("t").unwrap_or("data").into(),repeat:optional_integer(&e,"r",&events)?.unwrap_or(0),data_index:optional_integer(&e,"i",&events)?.unwrap_or(0),indexes:Vec::new()}),
                        (["colItems","i"],"x") => {if let Some(item)=col_items.last_mut(){item.indexes.push(optional_integer(&e,"v",&events)?.unwrap_or(0));}}
                        (["rowItems","i"],"x") => {if let Some(item)=row_items.last_mut(){item.indexes.push(optional_integer(&e,"v",&events)?.unwrap_or(0));}}
                        ([],"filters"|"formats"|"conditionalFormats"|"rowHierarchies"|"colHierarchies"|"calculatedMembers")
                        | (["pivotFields","pivotField"],"autoSortScope"|"memberProperties") => unsupported.push(e.name.local.to_string()),
                        (["pivotFields","pivotField"],"extLst") => {}
                        (["pivotFields","pivotField","extLst"],"ext") => {
                            if e.attribute("uri") != Some(FILL_DOWN_URI) { unsupported.push("extLst".into()); }
                        }
                        (["rowFields"], "field") => row_fields.push(integer(&e, "x", &events)?),
                        (["colFields"], "field") => column_fields.push(integer(&e, "x", &events)?),
                        (["pageFields"], "pageField") => {
                            page_fields.push(integer(&e, "fld", &events)?)
                        }
                        (["dataFields"], "dataField") => data_fields.push(PivotDataField {
                            name: e.attribute("name").map(|s|decoded(s,&events)).transpose()?,
                            field: integer(&e, "fld", &events)?,
                            subtotal: e.attribute("subtotal").unwrap_or("sum").into(),
                            attributes: attrs(&e),
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
                if path.pop().is_none() {
                    break;
                }
            }
            XmlEvent::Text(_) => {}
        }
    }
    Ok(PivotTable {
        name,
        part: part.into(),
        cache_id,
        area: location.ok_or_else(|| events.invalid("The pivot has no saved location"))?,
        row_fields,
        column_fields,
        page_fields,
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
