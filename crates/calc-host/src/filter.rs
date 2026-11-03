//! AutoFilter commands use detached CalcFml criteria evaluation over published values.
use crate::value::{CellKey, CoreRef, GridArea, SheetId};
use crate::{CellEdit, HostState, ProjectedValue, ProjectionRequest};
pub use calc_doc_model::{AutoFilter, FilterCriteria, FilterOperator, FilterPredicate};
use calc_engine::DetachedFormula;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilterChange {
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
fn validate_area(host: &HostState, area: GridArea) -> Result<(), String> {
    let sheet = host
        .loaded()
        .sheet(area.sheet())
        .ok_or_else(|| refusal("sheet is unavailable"))?;
    if sheet.protected() || sheet.is_macro_sheet() {
        return Err(refusal("requires an editable worksheet"));
    }
    if area.row_count() < 2 || area.row_count() > 32769 || area.column_count() > 256 {
        return Err(refusal(
            "supports a header and 1–32768 data rows in at most 256 columns",
        ));
    }
    if host.model().definitions().tables().iter().any(|t| {
        t.sheet == area.sheet()
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
    if revision != host.revision().get() {
        return Err("The workbook changed; reopen the filter and try again".into());
    }
    let old = host.model().definitions().auto_filter(sheet);
    if let Some(filter) = old
        && host.model().definitions().tables().iter().any(|t| {
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
            validate_area(host, area)?;
            Some(AutoFilter::new(area))
        }
        change => {
            let mut f = old
                .cloned()
                .ok_or("Select a range and enable AutoFilter first")?;
            validate_area(host, f.area)?;
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
            f.filtered_rows = apply(host, &f)?;
            Some(f)
        }
    };
    // Removal is explicit; protected sheets still refuse before changing any authored state.
    if host
        .loaded()
        .sheet(sheet)
        .is_none_or(|s| s.protected() || s.is_macro_sheet())
    {
        return Err(refusal("requires an editable worksheet"));
    }
    Ok(if next.as_ref() == old {
        Vec::new()
    } else {
        vec![CellEdit::AutoFilter {
            sheet,
            filter: next,
        }]
    })
}

fn apply(host: &HostState, filter: &AutoFilter) -> Result<BTreeSet<u32>, String> {
    let first = filter.area.first().row() + 1;
    let work = filter
        .columns
        .values()
        .map(|c| match c {
            FilterCriteria::Values { values, .. } => values.len().max(1),
            FilterCriteria::Custom { predicates, .. } => predicates.len(),
        })
        .sum::<usize>();
    if work.saturating_mul(filter.area.row_count() as usize) > 2_000_000 {
        return Err(refusal(
            "criteria need more than 2000000 row comparisons; use fewer values or a smaller range",
        ));
    }
    let cells = filter
        .columns
        .keys()
        .flat_map(|col| {
            (first..=filter.area.last().row())
                .map(move |row| CellKey::new(filter.area.sheet(), row, *col).unwrap())
        })
        .collect();
    let page = crate::project(host, &ProjectionRequest { cells }).map_err(|e| refusal(e))?;
    if page.cells().iter().any(|c| {
        !matches!(
            c.value(),
            ProjectedValue::Empty | ProjectedValue::Literal(_) | ProjectedValue::Calculated { .. }
        )
    }) {
        return Err(refusal(
            "needs current calculated values; calculate the range before applying criteria",
        ));
    }
    let mut plans = Vec::new();
    for (col, criteria) in &filter.columns {
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
            FilterCriteria::Custom { and, predicates } => {
                let terms = predicates
                    .iter()
                    .map(|p| format!("{}>0", count(format!("{}{}", p.operator.symbol(), p.value))))
                    .collect::<Vec<_>>();
                format!("{}({})", if *and { "AND" } else { "OR" }, terms.join(","))
            }
        };
        let plan = DetachedFormula::compile(host.model(), &formula, origin)
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
                .evaluate(&host.engine, host.model(), at)
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
    let f = host
        .model()
        .definitions()
        .auto_filter(sheet)
        .ok_or("Enable AutoFilter first")?;
    validate_area(host, f.area)?;
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
        apply(host, &single)?
    } else {
        BTreeSet::new()
    };
    let mut blank = match f.columns.get(&column) {
        Some(FilterCriteria::Values { blank, .. }) => *blank,
        Some(FilterCriteria::Custom { .. }) => false,
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
