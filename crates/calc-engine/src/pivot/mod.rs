//! Explicit worksheet pivot refresh over captured source records. This is aggregation, not a
//! second worksheet evaluator. Ordinary source edits do not refresh these snapshots.
use crate::{CalcTick, Calculation};
use calc_doc_model::{AuthoredRevision, pivot::*};
use calc_value::{CalcValue, CellKey, ExcelText, GridArea, WorksheetErrorCode};
use std::{cmp::Ordering, collections::BTreeMap};

pub fn refusal(part: &str, detail: impl Into<String>) -> PivotIssue {
    PivotIssue {
        part: part.into(),
        detail: detail.into(),
        owner: "dc-8wp",
    }
}
struct Group {
    pub(super) key: PivotItem,
    pub(super) shared: u32,
    pub(super) first: usize,
    pub(super) total: Total,
    pub(super) old: usize,
    pub(super) hidden: bool,
    pub(super) excluded: bool,
}
#[derive(Debug)]
pub struct PivotLayout {
    pub table: PivotTable,
    pub cells: BTreeMap<CellKey, CalcValue>,
}


mod aggregate;
mod axis;
mod formula;
mod records;
mod render;
use aggregate::Total;
use axis::{Walk, axis_item, render_axes};
use records::{compare, field_groups, page_allows, page_field_items, resolve};
pub use records::item_value;
use render::{Form, label_text, truthy};
pub use render::layout;

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

