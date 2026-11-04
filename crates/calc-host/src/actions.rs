//! Workbook commands shared by every host client. Excel rules are prepared here before
//! the session commits one transaction, records history and recalculates.
use crate::value::{CellKey, GridArea, SheetId};
use crate::{Axis, CellEdit, HostState, SheetEdit, SheetKey, SheetName, SheetVisibility};
use std::collections::{BTreeMap, BTreeSet};

/// Sort columns in priority order, with an independent direction per column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortKey {
    pub column: u32,
    pub descending: bool,
}

/// Optional first-key custom order or bounded case-sensitive text keys.
#[derive(Clone,Debug,Default,PartialEq,Eq)]
pub struct SortOptions {pub case_sensitive:bool,pub custom_order:Vec<String>}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SheetChange {
    Add { name: String },
    Rename { name: String },
    Move { position: u32 },
    Remove,
    Hide,
    Show,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionalChange {
    /// Add a cell, formula, statistical or duplicate condition with a solid fill.
    Condition {
        kind: String,
        applies: String,
        color: String,
        formulas: Vec<String>,
        operator: String,
        rank: u32,
        percent: bool,
        stop: bool,
    },
    Add {
        kind: String,
        applies: String,
        color: String,
    },
    Update {
        applies: String,
        formulas: Vec<String>,
        stop: bool,
    },
    Move {
        up: bool,
    },
    Remove,
}
/// Authored workbook commands. The session is the sole dispatch route for all clients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkbookCommand {
    /// Append the worksheets of an OOXML template as one undoable transaction.
    InsertTemplate { bytes: std::sync::Arc<[u8]>, name:Option<String>, revision:u64 },
    ReviewAction{at:CellKey,revision:u64,action:crate::ReviewAction},
    Review {
        at: CellKey,
        revision: u64,
        index: u32,
        text: String,
    },
    Name(crate::NameEdit),
    TableFilter {sheet:SheetId,revision:u64,part:String,change:crate::FilterChange},
    Table {revision:u64,change:crate::TableChange},
    AutoFilter { sheet:SheetId, revision:u64, change:crate::FilterChange },
    FormatCells {
        areas: Vec<GridArea>,
        patch: crate::FormatPatch,
    },
    Sheet {
        sheet: Option<SheetId>,
        change: SheetChange,
    },
    Outline { sheet: SheetId, rows: bool, revision: u64, change: crate::OutlineChange },
    AxisFormat {
        sheet: SheetId,
        rows: bool,
        first: u32,
        count: u32,
        size_bits: Option<u64>,
        hidden: Option<bool>,
    },
    FitContents {
        sheet: SheetId,
        rows: bool,
        revision: u64,
        sizes: Vec<(u32, u64)>,
    },
    FreezePanes {
        sheet: SheetId,
        rows: u32,
        columns: u32,
    },
    SplitPanes { sheet:SheetId, revision:u64, x:u64, y:u64, row:u32, column:u32 },
    Conditional {
        sheet: SheetId,
        revision: u64,
        index: Option<u32>,
        change: ConditionalChange,
    },
    Replace {
        options: crate::search::SearchOptions,
        replacement: String,
        revision: u64,
        only: Option<CellKey>,
    },
    SortRowsWithOptions {area:GridArea,keys:Vec<SortKey>,header:bool,revision:u64,options:SortOptions},
    SortRows {
        area: GridArea,
        keys: Vec<SortKey>,
        header: bool,
        revision: u64,
    },
    Paste(crate::paste::PastePlan),
    ShiftCells {
        area: GridArea,
        down: bool,
        insert: bool,
    },
}

pub(crate) fn prepare(
    host: &HostState,
    keys: &mut BTreeSet<SheetKey>,
    command: WorkbookCommand,
) -> Result<Vec<CellEdit>, String> {
    match command {
        WorkbookCommand::InsertTemplate{..}=>Err("Template insertion must use the session transaction (dc-bctq)".into()),
        WorkbookCommand::ReviewAction{at,revision,action}=>crate::review::prepare(host,at,revision,action),
        WorkbookCommand::Review{at,revision,index,text}=>crate::review::prepare(host,at,revision,crate::ReviewAction::Edit{index,text}),

        WorkbookCommand::Outline {sheet,rows,revision,change} => crate::outline::prepare(host,sheet,rows,revision,change),
        WorkbookCommand::SplitPanes {sheet,revision,x,y,row,column}=>{
            if revision!=host.revision().get() {return Err("The workbook changed before splitting (dc-971p.16)".into());}
            if host.model().sheet_by_id(sheet).is_none() {return Err("The worksheet no longer exists".into());}
            let (x,y)=(f64::from_bits(x),f64::from_bits(y));
            if x==0. && y==0. && !matches!(host.model().definitions().pane_layout(sheet),Some(calc_doc_model::PaneLayout::Split(_))) {return Ok(Vec::new());}
            let layout=if x==0. && y==0. {None}else{Some(calc_doc_model::PaneLayout::Split(calc_doc_model::SplitPanes::new(x,y,row,column).ok_or("Invalid split position (dc-971p.16)")?))};
            Ok(vec![CellEdit::PaneLayout{sheet,layout}])
        }
        WorkbookCommand::TableFilter{sheet,revision,part,change}=>crate::filter::prepare_for(host,sheet,revision,Some(&part),change),
        WorkbookCommand::Table {revision,change}=>crate::tables::prepare(host,revision,change),
        WorkbookCommand::AutoFilter { sheet,revision,change }=>crate::filter::prepare(host,sheet,revision,change),
        WorkbookCommand::SortRowsWithOptions{area,keys,header,revision,options}=>sort_rows(host,area,&keys,header,revision,&options),
        WorkbookCommand::SortRows {
            area,
            keys,
            header,
            revision,
        } => sort_rows(host, area, &keys, header, revision,&SortOptions::default()),
        WorkbookCommand::Name(edit) => crate::names::prepare(host, edit),
        WorkbookCommand::FormatCells { areas, patch } => {
            crate::format::prepare(host, &areas, &patch)
        }
        WorkbookCommand::Sheet { sheet, change } => sheet_edits(host, keys, sheet, change),
        WorkbookCommand::AxisFormat {
            sheet,
            rows,
            first,
            count,
            size_bits,
            hidden,
        } => axis_format_edits(host, sheet, rows, first, count, size_bits, hidden),
        WorkbookCommand::FitContents {
            sheet,
            rows,
            revision,
            sizes,
        } => {
            if revision != host.revision().get() {
                return Err("The workbook changed while fitting; try again".into());
            }
            if sizes.len() > 16384 {
                return Err("Fit at most 16384 rows or columns at once".into());
            }
            let axis = if rows { Axis::Rows } else { Axis::Columns };
            let mut edits = Vec::new();
            let mut seen = BTreeSet::new();
            for (index, size) in sizes {
                if !seen.insert(index) {
                    return Err("A fitted band was repeated".into());
                }
                let hidden = host
                    .model()
                    .definitions()
                    .axis_layouts()
                    .get(sheet, axis, index)
                    .hidden();
                edits.extend(axis_format_edits(
                    host,
                    sheet,
                    rows,
                    index,
                    1,
                    Some(size),
                    Some(hidden),
                )?);
            }
            Ok(edits)
        }
        WorkbookCommand::FreezePanes {
            sheet,
            rows,
            columns,
        } => {
            if host.model().sheet_by_id(sheet).is_none() {
                return Err("The sheet is unavailable".into());
            }
            let panes = crate::FrozenPanes::new(rows, columns);
            if panes.is_none() && (rows != 0 || columns != 0) {
                return Err("The frozen boundary is outside the worksheet".into());
            }
            Ok(if panes == host.model().definitions().frozen_panes(sheet) {
                Vec::new()
            } else {
                vec![CellEdit::FrozenPanes { sheet, panes }]
            })
        }
        WorkbookCommand::Conditional {
            sheet,
            revision,
            index,
            change,
        } => conditional_edits(host, sheet, revision, index, change),
        WorkbookCommand::Replace {
            options,
            replacement,
            revision,
            only,
        } => {
            if revision != host.revision().get() {
                return Err("The workbook changed; run Find again before replacing".into());
            }
            let edits = crate::search::replace_edits(host, &options, &replacement, only)?;
            if edits.is_empty() {
                return Err("No entries changed".into());
            }
            Ok(edits)
        }
        WorkbookCommand::Paste(plan) => plan.into_edits(host),
        WorkbookCommand::ShiftCells { area, down, insert } => {
            crate::shift_cell_edits(host, area, down, insert)
        }
    }
}

fn sheet_edits(
    host: &HostState,
    sheet_keys: &mut BTreeSet<SheetKey>,
    sheet: Option<SheetId>,
    change: SheetChange,
) -> Result<Vec<CellEdit>, String> {
    use SheetChange as C;
    let model = host.model();
    let id = sheet;
    let require = || {
        id.filter(|id| model.sheet_by_id(*id).is_some())
            .ok_or_else(|| "The worksheet is unavailable".to_owned())
    };
    let name = |text: String| -> Result<SheetName, String> {
        if text.eq_ignore_ascii_case("History") {
            return Err("History is reserved by Excel; choose another worksheet name".to_owned());
        }
        SheetName::new(crate::value::ExcelText::from_utf16_code_units(
            text.encode_utf16().collect(),
        ))
        .map_err(|e| e.to_string())
    };
    let edit = match change {
        C::Add { name: text } => {
            let key = (1u32..)
                .map(SheetKey::new)
                .find(|key| {
                    !sheet_keys.contains(key) && model.sheets().iter().all(|s| s.key() != *key)
                })
                .ok_or("No sheet keys remain")?;
            sheet_keys.insert(key);
            SheetEdit::Insert {
                key,
                name: name(text)?,
                position: std::num::NonZeroU32::new(model.sheets().len() as u32 + 1).unwrap(),
            }
        }
        C::Rename { name: text } => SheetEdit::Rename {
            sheet: require()?,
            name: name(text)?,
        },
        C::Move { position } => {
            if position == 0 || position as usize > model.sheets().len() {
                return Err("Choose a worksheet position in the workbook".to_owned());
            }
            SheetEdit::Move {
                sheet: require()?,
                position: std::num::NonZeroU32::new(position).unwrap(),
            }
        }
        C::Remove => SheetEdit::Remove { sheet: require()? },
        C::Hide | C::Show => {
            let sheet = require()?;
            if model.sheet_by_id(sheet).unwrap().visibility() == SheetVisibility::VeryHidden {
                return Err("This sheet is VeryHidden; unhiding it is unavailable here".to_owned());
            }
            SheetEdit::SetVisibility {
                sheet,
                visibility: if change == C::Hide {
                    SheetVisibility::Hidden
                } else {
                    SheetVisibility::Visible
                },
            }
        }
    };
    Ok(vec![CellEdit::Sheet(edit)])
}

pub use calc_value::stored_axis_size;

pub(crate) fn axis_format_edits(
    host: &HostState,
    sheet: SheetId,
    rows: bool,
    first: u32,
    count: u32,
    size_bits: Option<u64>,
    hidden: Option<bool>,
) -> Result<Vec<CellEdit>, String> {
    let loaded = host
        .loaded()
        .sheet(sheet)
        .ok_or("The sheet is unavailable")?;
    if loaded.geometry().outline_unsupported {return Err("Imported outline metadata is not modelled for dimension edits (dc-971p.39); nothing changed".into());}
    if loaded.protected() {
        return Err("Dimensions on a protected sheet are not available here".to_owned());
    }
    let axis = if rows {
        crate::Axis::Rows
    } else {
        crate::Axis::Columns
    };
    let last = first
        .checked_add(count)
        .and_then(|n| n.checked_sub(1))
        .filter(|n| first >= 1 && count > 0 && *n <= axis.size())
        .ok_or("The dimensions are outside the worksheet")?;
    if count > 16_384 {
        return Err("Change at most 16384 rows or columns at once".to_owned());
    }
    if rows && (hidden.is_some() || size_bits.is_some()) && host.model().definitions().auto_filter(sheet).is_some_and(|f|f.unsupported && first<=f.area.last().row() && last>f.area.first().row()) {
        return Err("Row visibility for imported filter metadata is not modelled (dc-971p.37); nothing changed".into());
    }
    if rows && host.model().definitions().hidden_rows(sheet,first,last).any(|(row,why)|
        why==crate::RowHidden::Filtered && (!host.model().definitions().auto_filter(sheet).is_some_and(|f|f.filtered_rows.contains(&row)) || host.model().definitions().tables().iter().any(|t|t.sheet==sheet && row>t.area.first().row() && row<=t.area.last().row()))) {
        return Err("Rows hidden by an imported table filter cannot be resized or shown here (dc-971p.37)".into());
    }
    let size = size_bits
        .map(|b| stored_axis_size(rows, f64::from_bits(b)))
        .transpose()
        .map_err(str::to_owned)?;
    let mut edits = Vec::new();
    for index in first..=last {
        let old = host
            .model()
            .definitions()
            .axis_layouts()
            .get(sheet, axis, index);
        let mut new = old;
        if let Some(size) = size {
            new = match size {
                None => old.with_hidden(true),
                Some(n) => old.with_size(Some(n), !rows).expect("a checked size").with_hidden(false),
            };
        }
        if let Some(hidden) = hidden {
            new = if !hidden && new.size() == Some(0.0) {
                new.with_size(None,false).expect("default size").with_hidden(false)
            } else {
                new.with_hidden(hidden)
            };
        }
        if old != new {
            edits.push(CellEdit::AxisFormat(
                crate::AxisFormatEdit::new(sheet, axis, index, new).expect("checked band indices"),
            ));
        }
    }
    Ok(edits)
}

pub(crate) fn conditional_edits(
    host: &HostState,
    sheet: crate::value::SheetId,
    revision: u64,
    index: Option<u32>,
    change: ConditionalChange,
) -> Result<Vec<CellEdit>, String> {
    if revision != host.revision().get() {
        return Err("Rules changed; reopen the current rule and try again".into());
    }
    if host.loaded().sheet(sheet).is_some_and(|s| s.protected()) {
        return Err("Worksheet protection prevents conditional rule edits".into());
    }
    if host
        .loaded()
        .sheet(sheet)
        .is_some_and(|s| s.conditional_formats().iter().any(|b| b.unread_container))
    {
        return Err("This worksheet has additional conditional-rule metadata; editing its rules is unavailable and the file is preserved (dc-971p.8)".into());
    }
    let mut rules = host.model().definitions().conditional_rules(sheet).to_vec();
    let ranges = |text: &str| -> Result<Vec<GridArea>, String> {
        let at = CellKey::new(sheet, 1, 1).unwrap();
        let mut areas = Vec::new();
        for token in text.split_whitespace() {
            match host.resolve_reference_text(at, token) {
                crate::ReferenceTarget::Area(a) if a.sheet() == sheet => areas.push(a),
                _ => {
                    return Err("Applies to needs space-separated ranges on this worksheet".into());
                }
            }
        }
        if areas.is_empty() || areas.len() > 128 {
            return Err("Choose between 1 and 128 ranges".into());
        }
        Ok(areas)
    };
    let mut staged_styles = None;
    let added = match &change {
        ConditionalChange::Add {kind,applies,color} => Some(crate::new_conditional_rule(ranges(applies)?,kind,color.trim_start_matches('#'))?),
        ConditionalChange::Condition {kind,applies,color,formulas,operator,rank,percent,stop} => {
            let areas=ranges(applies)?;
            let color=crate::format::color(color)?.ok_or("Choose a fill colour")?;
            let dxf=calc_doc_model::DifferentialFormat {fill:Some(calc_doc_model::Fill {pattern_type:Some("solid".into()),fg_color:Some(color),..Default::default()}),..Default::default()};
            let mut styles=(**host.styles()).clone();
            let id=styles.differential_formats.iter().position(|old|old==&dxf).unwrap_or_else(||{let id=styles.differential_formats.len();styles.differential_formats.push(dxf);id});
            let mut converted=Vec::new();
            for formula in formulas {
                let text=crate::FormulaText::new(crate::value::ExcelText::from_utf16_code_units(formula.trim().trim_start_matches('=').encode_utf16().collect())).map_err(|e|e.to_string())?;
                let form=crate::file_form_with_links(&text,host.model().definitions().names(),&host.model().definitions().external_file_names()).map_err(|e|format!("Conditional formula is unavailable: {e:?}"))?;
                converted.push(String::from_utf16_lossy(&form.text).trim_start_matches('=').to_owned());
            }
            let mut source=calc_doc::new_condition_rule(areas,kind,operator,converted,*rank,*percent,id as u32)?;
            source.stop=*stop;
            staged_styles=Some(styles);
            Some(source)
        }
        _=>None,
    };
    if let Some(mut source)=added {
        source.priority=rules.iter().map(|r|r.priority).max().unwrap_or(0).saturating_add(1);
        rules.push(source);
    } else {
        let i = index
            .map(|i| i as usize)
            .filter(|i| *i < rules.len())
            .ok_or("Select a rule first")?;
        let parsed = crate::conditional_rule(&rules[i])?;
        match change {
            ConditionalChange::Update {
                applies,
                formulas,
                stop,
            } => {
                let areas = ranges(&applies)?;
                if parsed.extension_id.is_some() {
                    return Err("Editing extension-backed rules is unavailable; the imported rule is preserved (dc-971p.8)".into());
                }
                let mut formulas = formulas;
                if formulas != rules[i].formulas {
                    if !matches!(parsed.kind.as_str(), "cellIs" | "expression")
                        || formulas.len() != rules[i].formulas.len()
                    {
                        return Err("This rule condition cannot be edited yet (dc-971p.8)".into());
                    }
                    for formula in &mut formulas {
                        let text = crate::FormulaText::new(
                            crate::value::ExcelText::from_utf16_code_units(
                                formula
                                    .trim()
                                    .trim_start_matches('=')
                                    .encode_utf16()
                                    .collect(),
                            ),
                        )
                        .map_err(|e| e.to_string())?;
                        let form = crate::file_form_with_links(
                            &text,
                            host.model().definitions().names(),
                            &host.model().definitions().external_file_names(),
                        )
                        .map_err(|e| format!("Conditional formula is unavailable: {e:?}"))?;
                        *formula = String::from_utf16_lossy(&form.text)
                            .trim_start_matches('=')
                            .to_owned();
                    }
                }
                rules[i].areas = areas;
                rules[i].formulas = formulas;
                rules[i].stop =
                    stop && !matches!(parsed.kind.as_str(), "colorScale" | "dataBar" | "iconSet");
                rules[i].authored = true;
            }
            ConditionalChange::Remove => {
                if parsed.extension_id.is_some() {
                    return Err("Removing extension-backed rules is unavailable; the imported rule is preserved (dc-971p.8)".into());
                }
                rules.remove(i);
            }
            ConditionalChange::Move { up } => {
                let mut order = (0..rules.len()).collect::<Vec<_>>();
                order.sort_by_key(|&n| rules[n].priority);
                let old = order.iter().position(|&n| n == i).unwrap();
                let destination = if up {
                    old.saturating_sub(1)
                } else {
                    (old + 1).min(order.len() - 1)
                };
                order.swap(old, destination);
                for (rank, index) in order.into_iter().enumerate() {
                    rules[index].priority = rank as i64 + 1;
                    rules[index].authored = true;
                }
            }
            _ => unreachable!(),
        }
    }
    let mut order = (0..rules.len()).collect::<Vec<_>>();
    order.sort_by_key(|&n| rules[n].priority);
    for (rank, index) in order.into_iter().enumerate() {
        rules[index].priority = rank as i64 + 1;
    }
    let mut edits=vec![CellEdit::ConditionalRules {sheet,rules}];
    if let Some(styles)=staged_styles && &styles!=host.styles().as_ref() {edits.insert(0,CellEdit::Styles(std::sync::Arc::new(styles)));}
    Ok(edits)
}

/// A bounded row permutation. Ordering comes from the existing evaluator; edits use the same
/// immutable copy snapshots and reference relocation as Paste, in one transaction. Source-only
/// metadata and unsupported qualified references remain refused. Notes and validations follow
/// the same validated permutation without overwriting one row’s metadata with another’s.
fn sort_rows(
    host: &HostState,
    area: GridArea,
    keys: &[SortKey],
    header: bool,
    revision: u64,
    options:&SortOptions,
) -> Result<Vec<CellEdit>, String> {
    let unavailable = |detail: &str| {
        format!("Sorting {detail} is not modelled (dc-971p.38); nothing was changed")
    };
    if revision != host.revision().get() {
        return Err("The workbook changed before sorting; select the range again".into());
    }
    crate::check_destination(host, area, crate::PasteMode::All).map_err(|e| e.to_string())?;
    if keys.is_empty() || keys.len() > 32 {
        return Err("Choose 1 to 32 sort keys".into());
    }
    let mut columns = BTreeSet::new();
    for key in keys {
        if !(area.first().column()..=area.last().column()).contains(&key.column) {
            return Err("Every sort column must lie in the selected range".into());
        }
        if !columns.insert(key.column) {
            return Err("Choose each sort column only once".into());
        }
    }
    let first = area.first().row() + u32::from(header);
    if first > area.last().row() {
        return Err("Select at least one data row below the header".into());
    }
    let data = GridArea::new(
        CellKey::new(area.sheet(), first, area.first().column()).unwrap(),
        area.last(),
    )
    .unwrap();
    if host
        .merged_areas(area.sheet())
        .iter()
        .any(|a| a.intersection(data).is_some())
    {
        return Err(unavailable("merged cells"));
    }
    for table in host.model().definitions().tables().iter().filter(|t|t.area.intersection(data).is_some()) {
        let top=table.area.first().row().checked_add(table.header_rows).ok_or_else(||unavailable("invalid table boundaries"))?;
        let bottom=table.area.last().row().checked_sub(table.totals_rows).ok_or_else(||unavailable("invalid table boundaries"))?;
        let complete = table.header_rows<=1 && table.totals_rows<=1 && top<=bottom
            && data.first().row()==top && data.last().row()==bottom
            && data.first().column()==table.area.first().column() && data.last().column()==table.area.last().column();
        if !complete {return Err(unavailable("a partial table or its header/total cells; select the complete data body"));}
        if table.opaque || table.axis_opaque || table.column_formulas.iter().any(Option::is_some)
            || table.filter.as_ref().is_some_and(|f|f.unsupported||!f.columns.is_empty()||!f.filtered_rows.is_empty()) {
            return Err(unavailable("filtered tables or unread/calculated table metadata"));
        }
    }
    if host
        .model()
        .definitions()
        .arrays()
        .any(|a| a.area().intersection(data).is_some())
        || host
            .engine
            .spills()
            .any(|a| a.area().intersection(data).is_some())
    {
        return Err(unavailable(
            "array formulas; paste their Values into an ordinary range first",
        ));
    }
    if host.model().definitions().axis_layouts()
        .outline_groups(area.sheet(), Axis::Rows).iter().any(|group| {
            group.first.min(group.summary) <= data.last().row()
                && group.last.max(group.summary) >= data.first().row()
        }) {
        return Err(unavailable("outline groups or their summary rows"));
    }
    if host.sheet_geometry(area.sheet()).is_some_and(|g| {
        g.rows
            .range(first..=area.last().row())
            .any(|(_, r)| r.hidden)
    }) {
        return Err(unavailable("hidden or filtered rows"));
    }
    if host
        .model()
        .definitions()
        .unloaded_in(data)
        .next()
        .is_some()
    {
        return Err(unavailable("unloaded cell content"));
    }
    if host.model().definitions().auto_filter(area.sheet()).is_some_and(|filter| filter.area.intersection(data).is_some()) {
        return Err(unavailable("AutoFilter ranges; remove the filter first"));
    }
    if !host.loaded.cell_sort_metadata_supported(area.sheet()) {
        return Err(unavailable("source cell decorations or worksheet objects"));
    }
    if host.axis_edits.iter().any(|e|e.sheet()==area.sheet())
        && host.model().definitions().review_notes(area.sheet()).iter().any(|n|
            data.contains(n.at) && matches!(n.source.as_ref(),Some(calc_doc_model::ReviewSource::Note{..}))) {
        return Err(unavailable("notes after row or column edits; save and reopen first"));
    }
    if host.axis_edits.iter().any(|e|e.sheet()==area.sheet()) && !host.model().definitions().conditional_rules(area.sheet()).is_empty(){return Err(unavailable("conditional formatting after row or column edits; save and reopen first"));}
    let review = host.sheet_review(area.sheet());
    if !review.unsupported.is_empty()
        || review
            .links
            .iter()
            .any(|l| data.intersection(l.area).is_some())
    {
        return Err(unavailable("cells with unread review fields or hyperlinks"));
    }
    // Pointwise rules follow the same row permutation below; unread and population-based
    // rules retain their refusal. No formula value is inferred from a saved cache.
    if review.conditional_extensions
        || host.loaded.sheet(area.sheet()).is_some_and(|sheet| {
            sheet.conditional_formats().iter().any(|block| block.unread_container)
        })
        || !crate::copy_metadata::conditional_sort_supported(host,area.sheet())
    {
        return Err(unavailable("a sheet with conditional formatting"));
    }
    for row in first..=area.last().row() {
        for col in area.first().column()..=area.last().column() {
            let at = CellKey::new(area.sheet(), row, col).unwrap();
            if let Some(crate::CellInput::Formula(formula)) = host.model().cell(at) {
                if !sort_formula_references_supported(host, formula, at) {
                    return Err(unavailable("qualified or unsupported formula references"));
                }
            }
        }
    }
    let cells = (first..=data.last().row()).flat_map(|row| keys.iter().map(move |key|
        CellKey::new(area.sheet(), row, key.column).unwrap())).collect();
    let page = crate::project(host, &crate::ProjectionRequest { cells }).map_err(|e| e.to_string())?;
    if page.cells().iter().any(|cell| !matches!(cell.value(),
        crate::ProjectedValue::Empty | crate::ProjectedValue::Literal(_) | crate::ProjectedValue::Calculated { .. })) {
        return Err(unavailable("keys without current calculated values; calculate the range first"));
    }
    if data.row_count()<2{return Ok(Vec::new());}
    let (case_widths,custom)=sort_option_keys(host,data,keys,&page,options)?;
    let count = data.row_count();
    if count < 2 {
        return Ok(Vec::new());
    }
    let label=sort_label;
    // Empty keys stay last in both directions. Original row index is an explicit tie key.
    // Grid sorting treats all errors as equal. Normalise their codes through the evaluator,
    // whose SORTBY otherwise orders different error codes. No host comparator is introduced.
    let mut formula = format!("_xlfn.SORTBY(_xlfn.SEQUENCE({count})");
    let mut sort_arguments=3usize; // row indexes plus the final tie key/direction
    for (index,key) in keys.iter().enumerate() {
        let range=format!("{}:{}",label(key.column,first),label(key.column,data.last().row()));
        let direction=if key.descending{-1}else{1};
        formula.push_str(&format!(",ISBLANK({range}),1"));sort_arguments+=2;
        if case_widths[index]>0 {
            formula.push_str(&format!(",IF(ISTEXT({range}),1,IF(ISNUMBER({range}),0,IF(ISLOGICAL({range}),2,3))),{direction},IF(ISTEXT({range}),0,IFERROR({range},NA())),{direction}"));sort_arguments+=4;
            for position in 1..=case_widths[index] {
                formula.push_str(&format!(",IF(ISTEXT({range}),IFERROR(CODE(LOWER(MID({range},{position},1))),0),0),{direction},IF(ISTEXT({range}),IFERROR(CODE(MID({range},{position},1))<91,FALSE),FALSE),{direction}"));sort_arguments+=4;
            }
        }else{
            let value=if index==0 {custom.as_ref().map(|list|format!("IF(ISBLANK({range}),0,_xlfn.XMATCH({range},{list},0))"))}else{None}.unwrap_or_else(||format!("IFERROR({range},NA())"));
            formula.push_str(&format!(",{value},{direction}"));sort_arguments+=2;
        }
    }
    if sort_arguments>255{return Err(unavailable("case-sensitive keys beyond 255 sort arguments"));}
    formula.push_str(&format!(",_xlfn.SEQUENCE({count}),1)"));
    if formula.encode_utf16().count()>8192{return Err(unavailable("case-sensitive keys beyond the formula-length limit"));}
    let value = calc_engine::DetachedFormula::compile_dynamic(host.model(), &formula, data.first())
        .and_then(|f| f.evaluate(&host.engine, host.model(), data.first()))
        .map_err(|e| format!("Sort could not calculate the row order (dc-971p.38): {e}"))?;
    let calc_value::CoreRef::Array(order) = value.core_ref() else {
        return Err(match value.core_ref() {
            calc_value::CoreRef::Error(code) => format!(
                "Sort could not calculate the row order: {} (dc-971p.38)",
                code.excel_spelling()
            ),
            _ => "Sort did not return a row order (dc-971p.38)".into(),
        });
    };
    if order.shape().rows != count as usize || order.shape().cols != 1 {
        return Err("Sort returned an unexpected row count (dc-971p.38)".into());
    }
    let mut seen = BTreeSet::new();
    let mut edits = Vec::new();
    let mut rows = BTreeMap::new();
    for index in 0..count {
        let Some(calc_value::CoreRef::Number(number)) =
            order.get(index as usize, 0).map(|v| v.core_ref())
        else {
            return Err("Sort did not return numeric row positions (dc-971p.38)".into());
        };
        if number < 1.
            || number > f64::from(count)
            || number.fract() != 0.
            || !seen.insert(number as u32)
        {
            return Err("Sort returned invalid row positions (dc-971p.38)".into());
        }
        let source_row = first + number as u32 - 1;
        let target_row = first + index;
        rows.insert(source_row,target_row);
        if source_row == target_row {
            continue;
        }
        let band = |row| {
            GridArea::new(
                CellKey::new(area.sheet(), row, data.first().column()).unwrap(),
                CellKey::new(area.sheet(), row, data.last().column()).unwrap(),
            )
            .unwrap()
        };
        let copy =
            crate::CopySnapshot::capture(host, band(source_row)).map_err(|e| e.to_string())?;
        edits.extend(
            copy.sorted_row_edits(host, band(target_row))
                .map_err(|e| e.to_string())?,
        );
    }
    crate::copy_metadata::permute_rows(host,data,&rows,&mut edits).map_err(|e|e.to_string())?;
    Ok(edits)
}

// Qualified relative references have distinct Excel sort behaviour. The only newly
// admitted qualifier is one valid, fully fixed cell on a different worksheet. Check
// its identity with the canonical binder and its invariance with the copy relocator.
pub(crate) fn sort_formula_references_supported(
    host: &HostState,
    formula: &crate::FormulaText,
    at: CellKey,
) -> bool {
    use calc_fml::syntax::{SyntaxChild, SyntaxKind};
    let tree = calc_fml::parse_formula(&format!("={}", formula.text().to_string_lossy()));
    if !tree.diagnostics().is_empty() {
        return false;
    }
    let step = |index, maximum| {
        if index < maximum {
            index + 1
        } else {
            index - 1
        }
    };
    let probe = CellKey::new(
        at.sheet(),
        step(at.row(), crate::value::MAX_ROWS),
        step(at.column(), crate::value::MAX_COLUMNS),
    )
    .unwrap();
    let mut qualified = false;
    let mut reference_operator = false;
    let mut nodes = vec![tree.root()];
    while let Some(node) = nodes.pop() {
        match node.kind() {
            SyntaxKind::SheetSpan3DReferenceExpr => return false,
            SyntaxKind::RangeExpr
            | SyntaxKind::IntersectionExpr
            | SyntaxKind::UnionExpr
            | SyntaxKind::PostfixExpr => reference_operator = true,
            SyntaxKind::QualifiedReferenceExpr => {
                qualified = true;
                let Some((_, target)) = calc_fml::reference::qualified_name(node) else {
                    return false;
                };
                // Excel names cannot start with '$'. Positive binding below still rejects
                // invalid dollar-prefixed cells, rather than treating unchanged text as proof.
                if !target.text().starts_with('$') {
                    return false;
                }
                let source = node.text();
                let source = source.trim();
                let crate::ReferenceTarget::Area(target) = host.resolve_reference_text(at, source)
                else {
                    return false;
                };
                if target.sheet() == at.sheet()
                    || target.row_count() != 1
                    || target.column_count() != 1
                {
                    return false;
                }
                let Some(crate::CellInput::Formula(reference)) =
                    crate::interpret_entry(&format!("={source}")).ok().flatten()
                else {
                    return false;
                };
                if calc_engine::relocate_formula(host.model(), &reference, at, probe).as_ref()
                    != Some(&reference)
                {
                    return false;
                }
            }
            _ => {}
        }
        nodes.extend(node.children().iter().filter_map(|child| match child {
            SyntaxChild::Node(node) => Some(node),
            _ => None,
        }));
    }
    // A qualifier can govern both endpoints of a range. Do not admit just its first
    // absolute endpoint while the second endpoint still changes under relocation.
    !qualified || !reference_operator
}

fn sort_label(column:u32,row:u32)->String {
        let mut col = column;
        let mut chars = Vec::new();
        while col > 0 {
            chars.push(char::from(b'A' + ((col - 1) % 26) as u8));
            col = (col - 1) / 26;
        }
        format!("{}{}", chars.into_iter().rev().collect::<String>(), row)
    }

/// Additional keys use ordinary worksheet functions in the same detached evaluation as Sort.
/// Case-sensitive ASCII order follows Microsoft's character-by-character a,A,b,B sequence:
/// https://support.microsoft.com/en-us/excel/sort-data-in-a-range-or-table-in-excel
fn sort_option_keys(host:&HostState,data:GridArea,keys:&[SortKey],page:&crate::ProjectionPage,options:&SortOptions)->Result<(Vec<usize>,Option<String>),String>{
    let unavailable=|detail:&str|format!("Sorting {detail} is not modelled (dc-971p.38); nothing changed");
    if options.case_sensitive && !options.custom_order.is_empty(){return Err(unavailable("case-sensitive custom lists"));}
    let mut widths=vec![0;keys.len()];
    if options.case_sensitive {
        for cell in page.cells(){
            let value=match cell.value(){crate::ProjectedValue::Literal(v)|crate::ProjectedValue::Calculated{value:v,..}=>Some(v),_=>None};
            if let Some(text)=value.and_then(|v|v.as_text()){
                let units=text.utf16_code_units();
                if units.len()>52 || units.iter().any(|u|!u8::try_from(*u).is_ok_and(|c|c.is_ascii_alphanumeric())) {return Err(unavailable("case-sensitive labels outside 52 ASCII letters or digits"));}
                let index=keys.iter().position(|k|k.column==cell.at().column()).expect("projected sort key");widths[index]=widths[index].max(units.len());
            }
        }
    }
    let order=&options.custom_order;
    if order.is_empty(){return Ok((widths,None));}
    if keys[0].descending {return Err("Reverse the custom list entries to reverse their order (dc-971p.38); nothing changed".into());}
    if order.len()>32 || order.iter().any(String::is_empty) || order.join("\n").encode_utf16().count()>255 {return Err("Enter 1 to 32 nonempty custom entries, at most 255 characters in total (dc-971p.38)".into());}
    let list=format!("{{{}}}",order.iter().map(|s|format!("\"{}\"",s.replace('"',"\"\""))).collect::<Vec<_>>().join(";"));
    let evaluate=|text:&str|calc_engine::DetachedFormula::compile_dynamic(host.model(),text,data.first()).and_then(|f|f.evaluate(&host.engine,host.model(),data.first())).map_err(|e|format!("Custom order could not be calculated (dc-971p.38): {e}"));
    let unique=evaluate(&format!("_xlfn.XMATCH({list},{list},0)"))?;
    let distinct=match unique.core_ref(){
        calc_value::CoreRef::Array(positions)=>positions.shape().rows==order.len() && positions.shape().cols==1 && positions.iter_row_major().enumerate().all(|(i,v)|matches!(v.core_ref(),calc_value::CoreRef::Number(n)if n==(i+1)as f64)),
        calc_value::CoreRef::Number(n)=>order.len()==1 && n==1.,
        _=>false,
    };
    if !distinct{return Err("Every custom entry must be distinct, ignoring case (dc-971p.38); nothing changed".into());}
    let range=format!("{}:{}",sort_label(keys[0].column,data.first().row()),sort_label(keys[0].column,data.last().row()));
    let ranks=evaluate(&format!("_xlfn.XMATCH({range},{list},0)"))?;
    let calc_value::CoreRef::Array(ranks)=ranks.core_ref()else{return Err("The custom key did not return row positions (dc-971p.38)".into());};
    if ranks.shape().rows!=data.row_count()as usize || ranks.shape().cols!=1{return Err("The custom key returned an unexpected row count (dc-971p.38)".into());}
    for cell in page.cells().iter().filter(|c|c.at().column()==keys[0].column){
        if matches!(cell.value(),crate::ProjectedValue::Empty){continue;}
        let value=match cell.value(){crate::ProjectedValue::Literal(v)|crate::ProjectedValue::Calculated{value:v,..}=>v,_=>return Err(unavailable("custom keys without current values"))};
        let rank=ranks.get((cell.at().row()-data.first().row())as usize,0);
        if value.as_text().is_none() || !rank.is_some_and(|v|matches!(v.core_ref(),calc_value::CoreRef::Number(n)if n>=1.&&n<=order.len()as f64)) {return Err("Every nonblank first-key value must be text included in the custom list (dc-971p.38); nothing changed".into());}
    }
    Ok((widths,Some(list)))
}
