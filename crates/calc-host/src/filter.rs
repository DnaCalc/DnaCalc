//! AutoFilter commands use detached CalcFml criteria evaluation over published values.
use crate::value::{CellKey, CoreRef, GridArea, SheetId};
use crate::{CellEdit, HostState, ProjectedValue, ProjectionRequest};
pub use calc_doc_model::{AutoFilter, FilterCriteria, FilterOperator, FilterPredicate};
use calc_engine::{DetachedEvaluationContext, DetachedReadPolicy};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilterChange {
    Color {
        column: u32,
        cell_color: bool,
        color: String,
    },
    Enable {
        area: GridArea,
    },
    Column {
        column: u32,
        criteria: Option<FilterCriteria>,
    },
    Clear,
    Remove,
    Reapply,
}
fn refusal(detail: impl std::fmt::Display) -> String {
    format!("AutoFilter {detail} (dc-971p.37); nothing changed")
}
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
fn label(at: CellKey) -> String {
    let mut col = at.column();
    let mut chars = Vec::new();
    while col > 0 {
        chars.push(char::from(b'A' + ((col - 1) % 26) as u8));
        col = (col - 1) / 26;
    }
    format!(
        "{}{}",
        chars.into_iter().rev().collect::<String>(),
        at.row()
    )
}
fn literal(s: &str) -> String {
    s.replace('~', "~~").replace('*', "~*").replace('?', "~?")
}
fn validate_area(host: &HostState, area: GridArea,table:Option<&str>) -> Result<(), String> {
    if host.model().sheet_by_id(area.sheet()).is_none()||host.loaded().sheet(area.sheet()).is_some_and(|s|s.protected()||s.is_macro_sheet()){return Err(refusal("requires an editable worksheet"));}
    if area.row_count() < 2 || area.row_count() > 32769 || area.column_count() > 256 {
        return Err(refusal(
            "supports a header and 1–32768 data rows in at most 256 columns",
        ));
    }
    if host.model().definitions().tables().iter().any(|t| {
        table.is_none() && t.sheet == area.sheet()
            && t.area.first().row() <= area.last().row()
            && t.area.last().row() > area.first().row()
    }) {
        return Err(refusal("on rows containing tables is not modelled"));
    }
    if host
        .merged_areas(area.sheet())
        .iter()
        .any(|a| a.intersection(area).is_some())
    {
        return Err(refusal("for merged ranges is not modelled"));
    }
    Ok(())
}

pub(crate) fn prepare(
    host: &HostState,
    sheet: SheetId,
    revision: u64,
    change: FilterChange,
) -> Result<Vec<CellEdit>, String> {
    prepare_for(host,sheet,revision,None,change)
}
pub(crate) fn prepare_for(host:&HostState,sheet:SheetId,revision:u64,table:Option<&str>,change:FilterChange)->Result<Vec<CellEdit>,String>{
    if revision != host.revision().get() {
        return Err("The workbook changed; reopen the filter and try again".into());
    }
    let mut staged_styles = None;
    let change = match change {
        FilterChange::Color {
            column,
            cell_color,
            color,
        } => {
            let color = crate::format::color(&color)?;
            let mut styles = (**host.styles()).clone();
            let mut dxf = calc_doc_model::DifferentialFormat::default();
            if cell_color {
                dxf.fill = Some(calc_doc_model::Fill {
                    pattern_type: Some(if color.is_some() { "solid" } else { "none" }.into()),
                    fg_color: color,
                    ..Default::default()
                });
            } else {
                dxf.font = Some(calc_doc_model::Font {
                    color: Some(color.unwrap_or(calc_doc_model::StyleColor {
                        auto: true,
                        ..Default::default()
                    })),
                    ..Default::default()
                });
            }
            let dxf_id = if let Some(id) = styles
                .differential_formats
                .iter()
                .position(|old| old == &dxf)
            {
                id as u32
            } else {
                let id = u32::try_from(styles.differential_formats.len())
                    .map_err(|_| refusal("has too many differential formats"))?;
                styles.differential_formats.push(dxf);
                id
            };
            staged_styles = Some(styles);
            FilterChange::Column {
                column,
                criteria: Some(FilterCriteria::Color { dxf_id, cell_color }),
            }
        }
        change => change,
    };
    let target=if let Some(part)=table{Some(host.model().definitions().tables().iter().find(|t|t.part==part&&t.sheet==sheet).ok_or("The table no longer exists")?)}else{None};
    let old=target.map_or_else(||host.model().definitions().auto_filter(sheet),|t|t.filter.as_ref());
    if let Some(filter) = old
        && table.is_none() && host.model().definitions().tables().iter().any(|t| {
            t.sheet == sheet
                && t.area.first().row() <= filter.area.last().row()
                && t.area.last().row() > filter.area.first().row()
        })
    {
        return Err(refusal("on rows containing tables is not modelled"));
    }

    let next = match change {
        FilterChange::Remove => {
            if old.is_none() {
                return Ok(Vec::new());
            }
            None
        }
        FilterChange::Enable { area } => {
            if area.sheet() != sheet {
                return Err(refusal("range belongs to another worksheet"));
            }
            if old.is_some() {
                return Err(refusal(
                    "already exists; remove it before choosing another range",
                ));
            }
            validate_area(host, area,table)?;
            if target.is_some_and(|t|t.header_rows==0||t.area.first()!=area.first()||t.area.last().column()!=area.last().column()||t.area.last().row()-t.totals_rows!=area.last().row()){return Err(refusal("range must be the table header and data rows"));}
            Some(AutoFilter::new(area))
        }
        change => {
            let mut f = old
                .cloned()
                .ok_or("Select a range and enable AutoFilter first")?;
            validate_area(host, f.area,table)?;
            if f.unsupported {
                return Err(refusal(
                    "has imported criteria or metadata whose editing is not modelled; the original XML is retained",
                ));
            }
            match change {
                FilterChange::Column { column, criteria } => {
                    if !(f.area.first().column()..=f.area.last().column()).contains(&column) {
                        return Err(refusal("column is outside its range"));
                    }
                    if let Some(criteria) = criteria {
                        f.columns.insert(column, criteria);
                    } else {
                        f.columns.remove(&column);
                    }
                }
                FilterChange::Clear => f.columns.clear(),
                FilterChange::Reapply => {}
                _ => unreachable!(),
            }
            if !f.valid() {
                return Err(refusal("criteria exceed supported limits"));
            }
            f.filtered_rows = apply(host, &f, staged_styles.as_ref().unwrap_or(host.styles()))?;
            Some(f)
        }
    };
    if host
        .loaded()
        .sheet(sheet)
        .is_some_and(|s| s.geometry().outline_unsupported)
    {
        return Err("AutoFilter visibility with this imported outline is not modelled (dc-971p.39); nothing changed".into());
    }
    // Removal is explicit; protected sheets still refuse before changing any authored state.
    if host
        .loaded()
        .sheet(sheet)
        .is_some_and(|s| s.protected() || s.is_macro_sheet())
    {
        return Err(refusal("requires an editable worksheet"));
    }
    let mut edits = if next.as_ref() == old {
        Vec::new()
    } else {
        if let Some(part)=table{let mut tables=host.model().definitions().tables().to_vec();tables.iter_mut().find(|t|t.part==part).unwrap().filter=next;vec![CellEdit::Tables(tables)]}else{vec![CellEdit::AutoFilter {sheet,filter:next}]}
    };
    if let Some(styles) = staged_styles.filter(|s| s != host.styles().as_ref()) {
        edits.insert(0, CellEdit::Styles(std::sync::Arc::new(styles)));
    }
    Ok(edits)
}

pub(crate) fn apply(
    host: &HostState,
    filter: &AutoFilter,
    styles: &calc_doc_model::StyleTable,
) -> Result<BTreeSet<u32>, String> {
    let first = filter.area.first().row() + 1;
    let mut work = filter
        .columns
        .values()
        .map(|c| match c {
            FilterCriteria::Values { values, .. } => values.len().max(1),
            FilterCriteria::Dates { groups, values, .. } => {
                (groups.len() * 6 + values.len()).max(1)
            }
            FilterCriteria::Top { .. } | FilterCriteria::Color { .. } => 1,
            FilterCriteria::Custom { predicates, .. } => predicates.len(),
        })
        .sum::<usize>();
    if work.saturating_mul(filter.area.row_count() as usize) > 2_000_000 {
        return Err(refusal(
            "criteria need more than 2000000 row comparisons; use fewer values or a smaller range",
        ));
    }
    let cells: Vec<_> = filter
        .columns
        .keys()
        .flat_map(|col| {
            (first..=filter.area.last().row())
                .map(move |row| CellKey::new(filter.area.sheet(), row, *col).unwrap())
        })
        .collect();
    let context = DetachedEvaluationContext::new_with_numeric_mode(&host.engine, host.model(),
        DetachedReadPolicy::CurrentValues, &host.options.numeric)
        .map_err(refusal)?;
    if cells.into_iter().any(|at| context.read_cell(at).is_err()) {
        return Err(refusal(
            "needs current calculated values; calculate the range before applying criteria",
        ));
    }
    let mut plans = Vec::new();
    let mut colors = Vec::new();
    let mut conditional_current = false;
    let mut additional_pointwise = None;
    for (col, criteria) in &filter.columns {
        if let FilterCriteria::Color { dxf_id, cell_color } = criteria {
            let area = GridArea::new(
                CellKey::new(filter.area.sheet(), first, *col).unwrap(),
                CellKey::new(filter.area.sheet(), filter.area.last().row(), *col).unwrap(),
            )
            .unwrap();
            if host.model().definitions().tables().iter().any(|table| {
                table.area.intersection(area).is_some() && table.style.name.as_ref().is_some_and(|s| !s.is_empty())
            }) {
                return Err(refusal("colour filtering on styled tables needs the table's effective colours and is not modelled"));
            }
            if host.loaded().sheet(area.sheet()).is_some_and(|sheet| {
                (sheet.review().conditional_extensions && sheet.review().conditional_extension_areas.as_ref().is_none_or(|areas| {
                    host.current_areas(area.sheet(), areas).iter().any(|a| a.intersection(area).is_some())
                }))
                    || sheet.conditional_formats().iter().any(|block| {
                        block.unread_container
                            && (block.areas.is_empty()
                                || host
                                    .current_areas(area.sheet(), &block.areas)
                                    .iter()
                                    .any(|a| a.intersection(area).is_some()))
                    })
            }) {
                return Err(refusal(
                    "colour filtering with unread conditional formatting or conditional extensions is not modelled",
                ));
            }
            for source in host.model().definitions().conditional_rules(area.sheet())
                .iter().filter(|rule| rule.areas.iter().any(|a| a.intersection(area).is_some()))
            {
                // Detached painting may inspect a held manual-mode observation. Authoring new
                // visibility needs current values; until per-rule read certificates are exposed,
                // require the workbook's published formula results to be current.
                if !conditional_current {
                    if context.all_formula_values_current() != Some(true) {
                        return Err(refusal("needs current calculated conditional-format values; calculate the workbook first"));
                    }
                    conditional_current = true;
                }
                work = work.saturating_add(1);
                if work.saturating_mul(filter.area.row_count() as usize) > 2_000_000 {
                    return Err(refusal("conditional colour criteria need more than 2000000 row comparisons"));
                }
                let rule = calc_doc::conditional_rule(source)
                    .map_err(|_| refusal("a conditional colour rule cannot be interpreted"))?;
                // Additional stored predicates share the bounded source validation used by
                // row movement. Existing cellIs/expression admission stays unchanged.
                let supported = matches!(rule.kind.as_str(), "cellIs" | "expression")
                    || *additional_pointwise.get_or_insert_with(||
                        crate::copy_metadata::conditional_sort_supported(host, area.sheet()));
                if rule.unread || !supported {
                    return Err(refusal("colour filtering with this conditional rule is not modelled"));
                }
                if let Some(id) = rule.dxf {
                    let dxf = host.styles().differential_formats.get(id as usize)
                        .ok_or_else(|| refusal("a conditional colour format is unavailable"))?;
                    // The shared conditional look represents explicit solid colours. Patterned
                    // fills, automatic colours and fill removal need a richer look before filtering.
                    let color = if *cell_color {
                        if let Some(fill) = &dxf.fill {
                            if fill.gradient.is_some() || !fill.other_children.is_empty()
                                || !matches!(fill.pattern_type.as_deref(), None | Some("solid"))
                            {
                                return Err(refusal("colour filtering on conditional pattern fills is not modelled"));
                            }
                            Some(fill.fg_color.as_ref().or(fill.bg_color.as_ref())
                                .ok_or_else(|| refusal("a conditional fill colour is unavailable"))?)
                        } else { None }
                    } else { dxf.font.as_ref().and_then(|font| font.color.as_ref()) };
                    if let Some(color) = color {
                        if color.auto || crate::conditional::resolve(host, color).is_none() {
                            return Err(refusal("a conditional colour cannot be resolved"));
                        }
                        color_value(host, Some(color), None)?;
                    }
                }
            }
            let dxf = styles
                .differential_formats
                .get(*dxf_id as usize)
                .ok_or_else(|| refusal("colour filter format is unavailable"))?;
            let color = if *cell_color {
                fill_color(host, dxf.fill.as_ref())?
            } else {
                font_color(host, dxf.font.as_ref())?
            };
            colors.push((*col, *cell_color, color, host.conditional_formats_for(area)));
            continue;
        }
        let origin = CellKey::new(filter.area.sheet(), first, *col).unwrap();
        let cell = label(origin);
        let count = |criterion: String| format!("COUNTIF({cell},{})", quote(&criterion));
        let formula = match criteria {
            FilterCriteria::Values { values, blank } => {
                let terms = values
                    .iter()
                    .map(|v| count(format!("={}", literal(v))))
                    .collect::<Vec<_>>();
                let any = if terms.is_empty() {
                    "FALSE".into()
                } else {
                    format!(
                        "({})>0",
                        terms
                            .into_iter()
                            .map(|t| format!("({t})"))
                            .collect::<Vec<_>>()
                            .join("+")
                    )
                };
                format!(
                    "IF(IFERROR({cell}=\"\",FALSE),{},{any})",
                    if *blank { "TRUE" } else { "FALSE" }
                )
            }
            FilterCriteria::Dates {
                groups,
                values,
                blank,
            } => {
                let functions = ["YEAR", "MONTH", "DAY", "HOUR", "MINUTE", "SECOND"];
                let mut terms = groups
                    .iter()
                    .map(|g| {
                        format!(
                            "AND({})",
                            g.iter()
                                .zip(functions)
                                .map(|(n, f)| format!("{f}({cell})={n}"))
                                .collect::<Vec<_>>()
                                .join(",")
                        )
                    })
                    .collect::<Vec<_>>();
                let dates = if terms.is_empty() {
                    "FALSE".into()
                } else {
                    format!(
                        "IFERROR(AND(ISNUMBER({cell}),OR({})),FALSE)",
                        terms.join(",")
                    )
                };
                terms = values
                    .iter()
                    .map(|v| format!("{}>0", count(format!("={}", literal(v)))))
                    .collect();
                terms.push(dates);
                format!(
                    "IF(IFERROR({cell}=\"\",FALSE),{},OR({}))",
                    if *blank { "TRUE" } else { "FALSE" },
                    terms.join(",")
                )
            }
            FilterCriteria::Top {
                top,
                percent,
                value,
            } => {
                let start = label(origin).trim_end_matches(char::is_numeric).to_owned();
                let range = format!("${start}${first}:${start}${}", filter.area.last().row());
                let k = if *percent {
                    format!("ROUNDUP(COUNT({range})*{value}/100,0)")
                } else {
                    value.clone()
                };
                let cutoff = format!(
                    "IF(COUNT({range})=0,0,_xlfn.AGGREGATE({},6,{range},MIN(COUNT({range}),MAX(1,{k}))))",
                    if *top { 14 } else { 15 }
                );
                let cutoff_value = context.compile(&cutoff, origin, calc_fml::FormulaForm::Legacy)
                    .map_err(|e| refusal(e))?
                    .evaluate_in(&context, origin)
                    .map_err(|e| refusal(e))?;
                let cutoff = cutoff_value
                    .as_number()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| refusal("could not calculate a numeric Top/Bottom cutoff"))?;
                format!(
                    "AND(ISNUMBER({cell}),IFERROR({cell}{}{cutoff},FALSE))",
                    if *top { ">=" } else { "<=" }
                )
            }
            FilterCriteria::Color { .. } => unreachable!(),
            FilterCriteria::Custom { and, predicates } => {
                let terms = predicates
                    .iter()
                    .map(|p| format!("{}>0", count(format!("{}{}", p.operator.symbol(), p.value))))
                    .collect::<Vec<_>>();
                format!("{}({})", if *and { "AND" } else { "OR" }, terms.join(","))
            }
        };
        let plan = context.compile(&formula, origin, calc_fml::FormulaForm::Legacy)
            .map_err(|e| refusal(format!("could not compile its criteria: {e}")))?;
        plans.push((*col, plan));
    }
    let mut hidden = BTreeSet::new();
    for row in first..=filter.area.last().row() {
        for (col, plan) in &plans {
            let at = CellKey::new(filter.area.sheet(), row, *col).unwrap();
            if host
                .model()
                .definitions()
                .not_available(at, host.model().cell(at).is_some())
            {
                return Err(refusal("cannot read unloaded data"));
            }
            let result = plan
                .evaluate_in(&context, at)
                .map_err(|e| refusal(format!("requires calculated formula values: {e}")))?;
            match result.as_logical() {
                Some(true) => {}
                Some(false) => {
                    hidden.insert(row);
                    break;
                }
                None => return Err(refusal("criteria did not return a logical result")),
            }
        }
        if !hidden.contains(&row) {
            for (col, cell_color, expected, formats) in &colors {
                let at = CellKey::new(filter.area.sheet(), row, *col).unwrap();
                let table = host.styles();
                let default_format = calc_doc_model::CellFormat::default();
                let id = host.cell_style(at);
                let format = table
                    .cell_formats
                    .get(id as usize)
                    .or_else(|| (id == 0).then_some(&default_format))
                    .ok_or_else(|| refusal("cell format is unavailable"))?;
                let parent = format
                    .xf_id
                    .and_then(|i| table.cell_style_formats.get(i as usize));
                let mut actual = if *cell_color {
                    let index = format
                        .fill_id
                        .or_else(|| parent.and_then(|p| p.fill_id))
                        .unwrap_or(0);
                    let fill = table.fills.get(index as usize);
                    if fill.is_none() && index != 0 {
                        return Err(refusal("fill is unavailable"));
                    }
                    fill_color(host, fill)?
                } else {
                    let index = format
                        .font_id
                        .or_else(|| parent.and_then(|p| p.font_id))
                        .unwrap_or(0);
                    let font = table.fonts.get(index as usize);
                    if font.is_none() && index != 0 {
                        return Err(refusal("font is unavailable"));
                    }
                    font_color(host, font)?
                };
                if formats.rule_count() > 0 {
                    let refused = formats.refused();
                    let look = formats.look(host, at).unwrap_or_default();
                    if formats.refused() != refused {
                        return Err(refusal("needs current calculated conditional-format values"));
                    }
                    if let Some(color) = if *cell_color { look.fill } else { look.font_color } {
                        actual = Some(match color {
                            crate::LookColor::Rgb(rgb) => rgb,
                            crate::LookColor::Indexed(index) => color_value(host,
                                Some(&calc_doc_model::StyleColor { indexed: Some(index.to_string()), ..Default::default() }), None)?
                                .ok_or_else(|| refusal("a conditional indexed colour cannot be resolved"))?,
                        });
                    }
                }
                if &actual != expected {
                    hidden.insert(row);
                    break;
                }
            }
        }
    }
    Ok(hidden)
}
/// Distinct raw values from the range, including rows excluded by other columns. File caches,
/// stale values and refusals cannot become filter choices described as calculated.
pub fn filter_values(
    host: &HostState,
    sheet: SheetId,
    column: u32,
) -> Result<(Vec<(String, bool)>, bool), String> {
    filter_values_for(host,sheet,None,column)
}
pub fn filter_values_for(host:&HostState,sheet:SheetId,table:Option<&str>,column:u32)->Result<(Vec<(String,bool)>,bool),String>{
    let f=if let Some(part)=table{host.model().definitions().tables().iter().find(|t|t.sheet==sheet&&t.part==part).and_then(|t|t.filter.as_ref())}else{host.model().definitions().auto_filter(sheet)}.ok_or("Enable AutoFilter first")?;
    validate_area(host, f.area,table)?;
    if !(f.area.first().column()..=f.area.last().column()).contains(&column) {
        return Err(refusal("column is outside its range"));
    }
    let cells = (f.area.first().row() + 1..=f.area.last().row())
        .map(|r| CellKey::new(sheet, r, column).unwrap())
        .collect();
    let page = crate::project(host, &ProjectionRequest { cells }).map_err(|e| e.to_string())?;
    let excluded = if let Some(criteria) = f.columns.get(&column) {
        let mut single = AutoFilter::new(f.area);
        single.columns.insert(column, criteria.clone());
        apply(host, &single, host.styles())?
    } else {
        BTreeSet::new()
    };
    let mut blank = match f.columns.get(&column) {
        Some(FilterCriteria::Values { blank, .. } | FilterCriteria::Dates { blank, .. }) => *blank,
        Some(
            FilterCriteria::Custom { .. }
            | FilterCriteria::Top { .. }
            | FilterCriteria::Color { .. },
        ) => false,
        None => true,
    };
    let mut values = BTreeMap::new();
    for cell in page.cells() {
        let value = match cell.value() {
            ProjectedValue::Empty => {
                blank |= !excluded.contains(&cell.at().row());
                continue;
            }
            ProjectedValue::Literal(v) | ProjectedValue::Calculated { value: v, .. } => v,
            _ => {
                return Err(refusal(
                    "needs calculated values; calculate the range before opening its list",
                ));
            }
        };
        let text = match value.core_ref() {
            CoreRef::Text(t) => t.to_string_lossy(),
            CoreRef::Number(n) => n.to_string(),
            CoreRef::Logical(b) => if b { "TRUE" } else { "FALSE" }.into(),
            CoreRef::Error(e) => e.excel_spelling().into(),
            CoreRef::Empty => {
                blank |= !excluded.contains(&cell.at().row());
                continue;
            }
            _ => return Err(refusal("cannot list a non-scalar value")),
        };
        if !text.is_empty() {
            values.insert(text, !excluded.contains(&cell.at().row()));
        } else {
            blank |= !excluded.contains(&cell.at().row());
        }
        if values.len() > 10000 {
            return Err(refusal(
                "list has more than 10000 distinct values; use custom criteria",
            ));
        }
    }
    Ok((values.into_iter().collect(), blank))
}

fn color_value(
    host: &HostState,
    color: Option<&calc_doc_model::StyleColor>,
    automatic: Option<&str>,
) -> Result<Option<String>, String> {
    match color {
        None => Ok(automatic.map(str::to_owned)),
        Some(c) if c.auto => Ok(automatic.map(str::to_owned)),
        Some(c) => host
            .resolved_style_color(c)
            .map(Some)
            .ok_or_else(|| refusal("stored colour cannot be resolved")),
    }
}
fn font_color(
    host: &HostState,
    font: Option<&calc_doc_model::Font>,
) -> Result<Option<String>, String> {
    if font.and_then(|f| f.color.as_ref()).is_some_and(|c| {
        c.theme.as_deref() == Some("1")
            && c.rgb.is_none()
            && c.indexed.is_none()
            && c.tint.as_deref().is_none_or(|v| v.parse::<f64>() == Ok(0.))
    }) && host.loaded().theme_colors().is_empty()
    {
        return Ok(Some("#000000".into()));
    }
    color_value(host, font.and_then(|f| f.color.as_ref()), Some("#000000"))
}
fn fill_color(
    host: &HostState,
    fill: Option<&calc_doc_model::Fill>,
) -> Result<Option<String>, String> {
    let Some(fill) = fill else {
        return Ok(None);
    };
    if fill.gradient.is_some() || !fill.other_children.is_empty() {
        return Err(refusal("colour filtering on this fill is not modelled"));
    }
    match fill.pattern_type.as_deref().unwrap_or("none") {
        "none" => Ok(None),
        "solid" => color_value(host, fill.fg_color.as_ref(), None)?
            .map(Some)
            .ok_or_else(|| refusal("solid fill colour is unavailable")),
        _ => Err(refusal("colour filtering on pattern fills is not modelled")),
    }
}
