use super::*;

fn plain_text(s: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_utf16_code_units(s.encode_utf16().collect()))
}
pub(super) fn axis_item(kind: &str, repeat: u32, indexes: Vec<u32>) -> PivotAxisItem {
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
/// Traverse either axis in field-item order. Visitors decide how each node is rendered.
trait AxisVisitor {
    fn children(&self, prefix: &[usize]) -> Vec<usize>;
    fn enter(&mut self, path: &[usize]) -> Result<bool, PivotIssue>;
    fn leave(&mut self, path: &[usize]) -> Result<(), PivotIssue>;
}
fn walk_axis<V: AxisVisitor>(visitor: &mut V, path: &mut Vec<usize>) -> Result<(), PivotIssue> {
    for child in visitor.children(path) {
        path.push(child);
        if visitor.enter(path)? { walk_axis(visitor, path)?; }
        visitor.leave(path)?;
        path.pop();
    }
    Ok(())
}

/// The rows of a pivot with several row fields, written depth first: a group's own row (compact
/// and outline) or its first leaf row (tabular), its children, then its total row if it has one.
pub(super) struct Walk<'a> {
    pub(super) levels: &'a [Vec<Group>],
    /// `totals[k]` holds the total of each path of `k + 1` groups.
    pub(super) totals: &'a [BTreeMap<Vec<usize>, Total>],
    pub(super) position: &'a [Vec<Option<u32>>],
    pub(super) form: Form,
    pub(super) subtotal_top: bool,
    pub(super) subtotal_on: &'a [bool],
    pub(super) fill_down: bool,
    pub(super) kind: &'a str,
    pub(super) value_column: u32,
    pub(super) r: u32,
    /// Tabular: how many leading levels of the current path already show their labels.
    pub(super) printed: usize,
    pub(super) cells: BTreeMap<(u32, u32), CalcValue>,
    pub(super) axis: Vec<PivotAxisItem>,
}
impl Walk<'_> {
    pub(super) fn visit(&mut self, path: &mut Vec<usize>) -> Result<(), PivotIssue> {
        walk_axis(self, path)
    }
}
impl AxisVisitor for Walk<'_> {
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
    fn enter(&mut self, path: &[usize]) -> Result<bool, PivotIssue> {
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
            return Ok(false);
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
        Ok(true)
    }
    fn leave(&mut self, path: &[usize]) -> Result<(), PivotIssue> {
        let level = path.len() - 1;
        if level + 1 == self.levels.len() || !self.subtotal_on[level] {
            return Ok(());
        }
        let g = path[level];
        let x = self.position[level][g].expect("visible group");
        let label_column = if self.form == Form::Compact { 0 } else { level as u32 };
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
/// A column axis level. The Values level contains the data fields in their declared order.
struct ColumnLevel {
    field: Option<usize>,
    groups: Vec<Group>,
    positions: Vec<Option<u32>>,
}
struct ColumnEntry {
    path: Vec<usize>,
    measure: usize,
    kind: &'static str,
}

/// Column visitor for the same depth-first walk used by rows.
struct ColumnWalk<'a> {
    paths: &'a std::collections::BTreeSet<Vec<usize>>,
    levels: &'a [ColumnLevel],
    measures: usize,
    output: &'a mut Vec<ColumnEntry>,
}
impl AxisVisitor for ColumnWalk<'_> {
    fn children(&self, prefix: &[usize]) -> Vec<usize> {
        let depth = prefix.len();
        self.paths.range(prefix.to_vec()..)
            .take_while(|path| path.starts_with(prefix))
            .map(|path| path[depth])
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter().collect()
    }
    fn enter(&mut self, path: &[usize]) -> Result<bool, PivotIssue> {
        if path.len() != self.levels.len() { return Ok(true); }
        let measure = self.levels.iter().position(|l| l.field.is_none()).map_or(0, |i| path[i]);
        self.output.push(ColumnEntry { path: path.to_vec(), measure, kind: "data" });
        Ok(false)
    }
    fn leave(&mut self, path: &[usize]) -> Result<(), PivotIssue> {
        let depth = path.len() - 1;
        if self.levels[depth].field.is_some() && self.levels[depth + 1..].iter().any(|l| l.field.is_some()) {
            let measure = self.levels.iter().position(|l| l.field.is_none())
                .filter(|&i| i <= depth).map(|i| path[i]);
            match measure {
                Some(measure) => self.output.push(ColumnEntry { path: path.to_vec(), measure, kind: "default" }),
                None => for measure in 0..self.measures {
                    self.output.push(ColumnEntry { path: path.to_vec(), measure, kind: "default" });
                },
            }
        }
        Ok(())
    }
}

/// Column combinations over a single row level. Subtotals and the Values level use the same
/// depth-first walk as the row layouts.
pub(super) fn layout_columns(
    table: &PivotTable,
    cache: &PivotCache,
    snapshot: &PivotSnapshot,
) -> Result<PivotLayout, PivotIssue> {
    use std::collections::{BTreeMap, BTreeSet};
    let bad = |why: &str| refusal(&table.part, why);
    let row = usize::try_from(table.row_fields[0]).map_err(|_| bad("Invalid row field"))?;
    let value_level = table.column_fields.iter().filter(|&&f| f == -2).count();
    let values_only = table.column_fields == [-2];
    let show = table.data_fields.first().and_then(|d| d.attributes.get("showDataAs"))
        .map(String::as_str).unwrap_or("normal");
    let percent = show != "normal";
    if percent && (table.column_fields.len() != 1 || values_only || table.data_fields.len() != 1
        || !matches!(show, "percentOfCol" | "percentOfRow" | "percentOfTotal")) {
        return Err(bad("This percentage layout is not modelled"));
    }
    if row >= cache.fields.len()
        || table.fields.len() != cache.fields.len()
        || table.data_fields.is_empty()
        || value_level > 1
        || (table.data_fields.len() > 1) != (value_level == 1)
        || table.data_fields.iter().any(|d| {
            d.field as usize >= cache.fields.len()
                || !matches!(
                    d.subtotal.as_str(),
                    "sum" | "count" | "countNums" | "min" | "max" | "average"
                )
                || d.attributes
                    .get("showDataAs")
                    .is_some_and(|v| v != "normal" && !percent)
        })
    {
        return Err(bad("This pivot axis or value aggregation is not modelled"));
    }
    let custom_subtotal = |field: usize| table.fields[field].attributes.iter().any(|(name, value)| {
        name.ends_with("Subtotal") && name != "defaultSubtotal" && truthy(Some(value), false)
    });
    if custom_subtotal(row) || !truthy(table.fields[row].attributes.get("defaultSubtotal"), true) {
        return Err(bad(
            "Row fields without an automatic subtotal are not modelled",
        ));
    }
    let measure = table.data_fields[0].field as usize;
    let rows = field_groups(table, cache, snapshot, row, measure)?;
    let positions = |groups: &[Group]| {
        let mut next = 0;
        groups
            .iter()
            .map(|g| {
                if g.hidden {
                    None
                } else {
                    let at = next;
                    next += 1;
                    Some(at)
                }
            })
            .collect::<Vec<_>>()
    };
    let row_positions = positions(&rows);
    let mut levels = Vec::new();
    let mut fields = vec![row];
    for &field in &table.column_fields {
        if field == -2 {
            levels.push(ColumnLevel {
                field: None,
                groups: Vec::new(),
                positions: Vec::new(),
            });
        } else {
            let field = usize::try_from(field).map_err(|_| bad("Invalid column field"))?;
            if field >= cache.fields.len() || fields.contains(&field) {
                return Err(bad("Invalid or repeated column field"));
            }
            fields.push(field);
            if custom_subtotal(field) || !truthy(table.fields[field].attributes.get("defaultSubtotal"), true) {
                return Err(bad(
                    "Column fields without automatic subtotals are not modelled",
                ));
            }
            let groups = field_groups(table, cache, snapshot, field, measure)?;
            let positions = positions(&groups);
            levels.push(ColumnLevel {
                field: Some(field),
                groups,
                positions,
            });
        }
    }
    if levels.len() > 64 {
        return Err(bad("More than 64 column levels are not modelled"));
    }
    let sorted = |groups: &[Group]| {
        let mut indexes: Vec<usize> = (0..groups.len()).collect();
        indexes.sort_by(|&a, &b| compare(&groups[a].key, &groups[b].key).unwrap_or(Ordering::Equal));
        indexes
    };
    let row_sorted = sorted(&rows);
    let level_sorted: Vec<_> = levels.iter().map(|level| sorted(&level.groups)).collect();
    let find = |groups: &[Group], sorted: &[usize], key: &PivotItem| {
        sorted.binary_search_by(|&i| compare(&groups[i].key, key).unwrap_or(Ordering::Equal))
            .map(|at| sorted[at]).map_err(|_| bad("A source label is missing from its field"))
    };
    let mut paths = BTreeSet::new();
    let mut totals = BTreeMap::<(usize, Vec<usize>, usize), Total>::new();
    for record in &snapshot.records {
        if !page_allows(table, cache, record)? { continue; }
        let r = find(&rows, &row_sorted, resolve(cache, record, row)?)?;
        if rows[r].hidden {
            continue;
        }
        let mut indexes = Vec::with_capacity(levels.len());
        let mut hidden = false;
        for (depth, level) in levels.iter().enumerate() {
            if let Some(field) = level.field {
                let index = find(&level.groups, &level_sorted[depth], resolve(cache, record, field)?)?;
                hidden |= level.groups[index].hidden;
                indexes.push(index);
            } else {
                indexes.push(0);
            }
        }
        if hidden {
            continue;
        }
        for (m, data) in table.data_fields.iter().enumerate() {
            if let Some(i) = levels.iter().position(|level| level.field.is_none()) {
                indexes[i] = m;
            }
            paths.insert(indexes.clone());
            if paths.len() > 16_384 {
                return Err(bad("The pivot has too many column combinations"));
            }
            let value = resolve(cache, record, data.field as usize)?;
            for depth in 0..=indexes.len() {
                let path = indexes[..depth].to_vec();
                totals
                    .entry((r, path.clone(), m))
                    .or_default()
                    .push(value)?;
                totals
                    .entry((usize::MAX, path, m))
                    .or_default()
                    .push(value)?;
            }
        }
    }
    let mut columns = Vec::new();
    walk_axis(&mut ColumnWalk {
        paths: &paths,
        levels: &levels,
        measures: table.data_fields.len(),
        output: &mut columns,
    }, &mut Vec::new())?;
    let row_grand = truthy(table.attributes.get("colGrandTotals"), true);
    if !values_only && truthy(table.attributes.get("rowGrandTotals"), true) {
        for measure in 0..table.data_fields.len() {
            columns.push(ColumnEntry {
                path: Vec::new(),
                measure,
                kind: "grand",
            });
        }
    }
    let first = table.area.first();
    let header = if values_only { 1 } else { levels.len() as u32 + 1 };
    let mut cells = BTreeMap::<(u32, u32), CalcValue>::new();
    let caption = |m: usize| {
        table.data_fields[m].name.clone().unwrap_or_else(|| {
            cache.fields[table.data_fields[m].field as usize]
                .name
                .clone()
        })
    };
    let compact = truthy(table.attributes.get("compact"), true);
    let row_caption = if compact {
        table
            .attributes
            .get("rowHeaderCaption")
            .map(String::as_str)
            .unwrap_or("Row Labels")
    } else {
        &cache.fields[row].name
    };
    let grand_caption = table
        .attributes
        .get("grandTotalCaption")
        .map(String::as_str)
        .unwrap_or("Grand Total");
    if values_only {
        cells.insert((0, 0), plain_text(row_caption));
    } else {
        if table.data_fields.len() == 1 { cells.insert((0, 0), plain_text(&caption(0))); }
        let column_caption = table.attributes.get("colHeaderCaption").map(String::as_str)
            .unwrap_or_else(|| if compact { "Column Labels" } else { &cache.fields[levels[0].field.unwrap()].name });
        cells.insert((0, 1), plain_text(column_caption));
        cells.insert((header - 1, 0), plain_text(row_caption));
    }
    let mut col_items = Vec::new();
    let mut previous = Vec::new();
    for (i, entry) in columns.iter().enumerate() {
        let column = i as u32 + 1;
        let repeat = if entry.kind == "data" {
            previous
                .iter()
                .zip(&entry.path)
                .take_while(|(a, b)| a == b)
                .count()
        } else {
            0
        };
        if values_only {
            cells.insert((0, column), plain_text(&caption(entry.measure)));
        } else if entry.kind == "grand" {
            let label = if table.data_fields.len() == 1 {
                grand_caption.to_owned()
            } else {
                format!("Total {}", caption(entry.measure))
            };
            cells.insert((1, column), plain_text(&label));
        } else if entry.kind == "default" {
            let depth = entry.path.len() - 1;
            let level = &levels[depth];
            cells.insert(
                (1 + depth as u32, column),
                plain_text(&format!(
                    "{} Total",
                    label_text(&level.groups[entry.path[depth]])?
                )),
            );
        } else {
            for depth in repeat..entry.path.len() {
                let level = &levels[depth];
                let value = match level.field {
                    Some(_) => group_label(&level.groups[entry.path[depth]])?,
                    None => plain_text(&caption(entry.path[depth])),
                };
                cells.insert((1 + depth as u32, column), value);
            }
        }
        let indexes = if entry.kind == "grand" {
            vec![0]
        } else {
            entry
                .path
                .iter()
                .enumerate()
                .skip(repeat)
                .map(|(depth, &g)| {
                    levels[depth]
                        .field
                        .map_or(g as u32, |_| levels[depth].positions[g].unwrap())
                })
                .collect()
        };
        col_items.push(PivotAxisItem {
            kind: entry.kind.into(),
            repeat: repeat as u32,
            data_index: entry.measure as u32,
            indexes,
        });
        previous = if entry.kind == "data" {
            entry.path.clone()
        } else {
            Vec::new()
        };
    }
    if percent {
        let valid = |total: &Total| matches!(total.result(&table.data_fields[0].subtotal).core_ref(),
            calc_value::CoreRef::Number(n) if n != 0.0);
        let grand = totals.get(&(usize::MAX, Vec::new(), 0)).ok_or_else(|| bad("Missing grand total"))?;
        if ((show != "percentOfCol" || columns.iter().any(|entry| entry.kind == "grand")) && !valid(grand))
            || (show == "percentOfCol" && columns.iter().filter(|entry| entry.kind != "grand").any(|entry|
                totals.get(&(usize::MAX, entry.path.clone(), 0)).is_none_or(|total| !valid(total))))
            || (show == "percentOfRow" && rows.iter().enumerate().filter(|(_, group)| !group.hidden).any(|(i, _)|
                totals.get(&(i, Vec::new(), 0)).is_some_and(|total| !valid(total))))
            || totals.values().any(|total| matches!(total.result(&table.data_fields[0].subtotal).core_ref(), calc_value::CoreRef::Error(_))) {
            return Err(bad("Percentage with zero or error totals is not modelled"));
        }
    }
    let displayed = |total: &Total, row: usize, entry: &ColumnEntry| -> CalcValue {
        let value = total.result(&table.data_fields[entry.measure].subtotal);
        if !percent { return value; }
        let denominator = match show {
            "percentOfRow" => (row, Vec::new(), 0),
            "percentOfTotal" => (usize::MAX, Vec::new(), 0),
            _ => (usize::MAX, entry.path.clone(), 0),
        };
        match (value.core_ref(), totals[&denominator].result(&table.data_fields[0].subtotal).core_ref()) {
            (calc_value::CoreRef::Number(n), calc_value::CoreRef::Number(d)) => {
                let ratio = n / d;
                if ratio.is_finite() { CalcValue::number(ratio) } else { CalcValue::error(WorksheetErrorCode::Num) }
            }
            (calc_value::CoreRef::Error(e), _) => CalcValue::error(e),
            _ => CalcValue::empty(),
        }
    };
    let mut axis = Vec::new();
    let mut r = header;
    for (i, group) in rows.iter().enumerate().filter(|(_, g)| !g.hidden) {
        if !totals.contains_key(&(i, Vec::new(), 0)) {
            continue;
        }
        cells.insert((r, 0), group_label(group)?);
        for (col, entry) in columns.iter().enumerate() {
            if let Some(total) = totals.get(&(i, entry.path.clone(), entry.measure)) {
                cells.insert(
                    (r, col as u32 + 1),
                    displayed(total, i, entry),
                );
            } else if percent {
                cells.insert((r, col as u32 + 1), CalcValue::number(0.));
            }
        }
        axis.push(axis_item("data", 0, vec![row_positions[i].unwrap()]));
        r += 1;
    }
    if row_grand {
        cells.insert((r, 0), plain_text(grand_caption));
        for (col, entry) in columns.iter().enumerate() {
            if let Some(total) = totals.get(&(usize::MAX, entry.path.clone(), entry.measure)) {
                cells.insert(
                    (r, col as u32 + 1),
                    displayed(total, usize::MAX, entry),
                );
            }
        }
        axis.push(axis_item("grand", 0, vec![0]));
        r += 1;
    }
    let last = CellKey::new(
        first.sheet(),
        first
            .row()
            .checked_add(r - 1)
            .ok_or_else(|| bad("Pivot output exceeds worksheet"))?,
        first
            .column()
            .checked_add(columns.len() as u32)
            .ok_or_else(|| bad("Pivot output exceeds worksheet"))?,
    )
    .map_err(|_| bad("Pivot output exceeds worksheet"))?;
    let mut next = table.clone();
    next.area = GridArea::new(first, last).map_err(|_| bad("Invalid pivot output area"))?;
    next.location.insert("firstHeaderRow".into(), if values_only { "0" } else { "1" }.into());
    next.location
        .insert("firstDataRow".into(), header.to_string());
    next.location.insert("firstDataCol".into(), "1".into());
    let rebuild = |field: usize, groups: &[Group], positions: &[Option<u32>]| {
        let mut out = groups
            .iter()
            .enumerate()
            .filter(|(i, _)| positions[*i].is_some())
            .map(|(_, g)| PivotFieldItem {
                index: Some(g.shared),
                kind: "data".into(),
                hidden: false,
            })
            .collect::<Vec<_>>();
        out.extend(
            table.fields[field]
                .items
                .iter()
                .filter(|i| i.hidden)
                .cloned(),
        );
        out.push(PivotFieldItem {
            index: None,
            kind: "default".into(),
            hidden: false,
        });
        out
    };
    next.fields[row].items = rebuild(row, &rows, &row_positions);
    for level in &levels {
        if let Some(field) = level.field {
            next.fields[field].items = rebuild(field, &level.groups, &level.positions);
        }
    }
    next.row_items = axis;
    next.col_items = col_items;
    for row in 0..r {
        for col in 0..=columns.len() as u32 {
            cells.entry((row, col)).or_insert_with(CalcValue::empty);
        }
    }
    let cells = cells
        .into_iter()
        .map(|((row, col), value)| {
            (
                CellKey::new(first.sheet(), first.row() + row, first.column() + col)
                    .expect("validated bounds"),
                value,
            )
        })
        .collect();
    page_field_items(table, cache, snapshot, measure, &mut next)?;
    Ok(PivotLayout { table: next, cells })
}
