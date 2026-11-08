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
    match &group.key {
        PivotItem::Missing => Ok(plain_text("(blank)")),
        PivotItem::Text(value) if value.is_empty() => Ok(CalcValue::empty()),
        PivotItem::Boolean(value) => Ok(plain_text(if *value { "TRUE" } else { "FALSE" })),
        PivotItem::Error(value) => Ok(plain_text(value)),
        _ => item_value(&group.key),
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
    pub(super) blank_after: &'a [bool],
    pub(super) fill_down: bool,
    pub(super) kind: &'a str,
    pub(super) value_column: u32,
    pub(super) r: u32,
    /// Tabular: how many leading levels of the current path already show their labels.
    pub(super) printed: usize,
    pub(super) cells: BTreeMap<(u32, u32), CalcValue>,
    pub(super) axis: Vec<PivotAxisItem>,
    /// Output rows that carry a value and the field-item path defining their total.
    pub(super) row_paths: Vec<(u32, Vec<usize>, Option<usize>)>,
    pub(super) collect_paths: bool,
    pub(super) values_level: Option<usize>,
    pub(super) value_captions: &'a [String],
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
        let data_index = self.values_level.and_then(|index| path.get(index)).copied().unwrap_or(0) as u32;
        if level + 1 == depth {
            match self.form {
                Form::Compact | Form::Outline => {
                    self.cells.insert((self.r, label_column), group_label(group)?);
                    self.cells.insert((self.r, self.value_column), total);
                    let mut item = axis_item("data", level as u32, vec![x]);
                    item.data_index = data_index;
                    self.axis.push(item);
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
                    let mut item = axis_item("data", from as u32, indexes);
                    item.data_index = data_index;
                    self.axis.push(item);
                    self.printed = depth;
                }
            }
            if self.collect_paths { self.row_paths.push((self.r, path.to_vec(), None)); }
            self.r += 1;
            return Ok(false);
        }
        match self.form {
            Form::Compact | Form::Outline => {
                self.cells.insert((self.r, label_column), group_label(group)?);
                if self.subtotal_top && self.subtotal_on[level] {
                    self.cells.insert((self.r, self.value_column), total);
                    if self.collect_paths { self.row_paths.push((self.r, path.to_vec(), None)); }
                }
                let mut item = axis_item("data", level as u32, vec![x]);
                item.data_index = data_index;
                self.axis.push(item);
                self.r += 1;
            }
            Form::Tabular => self.printed = self.printed.min(level),
        }
        Ok(true)
    }
    fn leave(&mut self, path: &[usize]) -> Result<(), PivotIssue> {
        let level = path.len() - 1;
        if level + 1 == self.levels.len() {
            return Ok(());
        }
        let g = path[level];
        let x = self.position[level][g].expect("visible group");
        if self.subtotal_on[level] && self.values_level.is_some_and(|v| level + 1 < v) {
            let group = &self.levels[level][g];
            for (measure, caption) in self.value_captions.iter().enumerate() {
                self.cells.insert((self.r, level as u32), plain_text(&format!("{} {}", label_text(group)?, caption)));
                let mut item = axis_item("default", level as u32, vec![x]);
                item.data_index = measure as u32;
                self.axis.push(item);
                if self.collect_paths { self.row_paths.push((self.r, path.to_vec(), Some(measure))); }
                self.r += 1;
            }
            self.printed = level;
        } else if self.subtotal_on[level] {
            let label_column = if self.form == Form::Compact { 0 } else { level as u32 };
            let group = &self.levels[level][g];
            let total = self.totals[level][&*path].result(self.kind);
            let total_label = plain_text(&format!("{} Total", label_text(group)?));
            match self.form {
                Form::Compact | Form::Outline if !self.subtotal_top => {
                    self.cells.insert((self.r, label_column), total_label);
                    self.cells.insert((self.r, self.value_column), total);
                    self.axis.push(axis_item("default", level as u32, vec![x]));
                    if self.collect_paths { self.row_paths.push((self.r, path.to_vec(), None)); }
                    self.r += 1;
                }
                Form::Tabular => {
                    self.cells.insert((self.r, level as u32), total_label);
                    self.cells.insert((self.r, self.value_column), total);
                    self.axis.push(axis_item("default", level as u32, vec![x]));
                    if self.collect_paths { self.row_paths.push((self.r, path.to_vec(), None)); }
                    self.r += 1;
                    self.printed = level;
                }
                _ => {}
            }
        }
        if self.blank_after[level] {
            self.axis.push(axis_item("blank", 0, vec![x]));
            self.r += 1;
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

/// Column combinations and their row paths use the shared depth-first axis walk.
pub(super) fn render_axes(
    table: &PivotTable,
    cache: &PivotCache,
    snapshot: &PivotSnapshot,
) -> Result<PivotLayout, PivotIssue> {
    use std::collections::{BTreeMap, BTreeSet};
    let bad = |why: &str| refusal(&table.part, why);
    // usize::MAX denotes the Values pseudo-field; real cache fields cannot use that index.
    let row_fields = table.row_fields.iter().map(|&field| if field == -2 { Ok(usize::MAX) }
        else { usize::try_from(field).map_err(|_| bad("Invalid row field")) })
        .collect::<Result<Vec<_>, _>>()?;
    let row_values = row_fields.iter().position(|&field| field == usize::MAX);
    let row = row_fields.iter().copied().find(|&field| field != usize::MAX)
        .ok_or_else(|| bad("A source row field is required"))?;
    let multiple_rows = row_fields.len() > 1;
    if row_fields.len() > 64 || row_fields.iter().any(|&field| field != usize::MAX && field >= cache.fields.len())
        || row_fields.iter().enumerate().any(|(i, field)| row_fields[..i].contains(field)) {
        return Err(bad("Invalid or repeated row field"));
    }
    let value_level = table.column_fields.iter().filter(|&&f| f == -2).count();
    let values_only = table.column_fields == [-2];
    let rows_only = table.column_fields.is_empty();
    let show = table.data_fields.first().and_then(|d| d.attributes.get("showDataAs"))
        .map(String::as_str).unwrap_or("normal");
    let percent = matches!(show, "percentOfCol" | "percentOfRow" | "percentOfTotal");
    let indexed = show == "index";
    let relative = matches!(show, "difference" | "percent" | "percentDiff" | "runTotal");
    if show != "normal" && (!(percent || indexed || relative) || table.column_fields.len() != 1
        || values_only || table.data_fields.len() != 1) {
        return Err(bad("This value calculation layout is not modelled"));
    }
    if (multiple_rows && show != "normal")
        || (multiple_rows && row_values.is_none() && table.data_fields.len() != 1)
        || (row_values.is_some() && (row_fields.len() > 3 || table.column_fields.len() > 1
            || table.data_fields.iter().any(|d| d.subtotal != "sum")))
        || row >= cache.fields.len()
        || table.fields.len() != cache.fields.len()
        || table.data_fields.is_empty()
        || value_level + usize::from(row_values.is_some()) > 1
        || (table.data_fields.len() > 1) != (value_level + usize::from(row_values.is_some()) == 1)
        || table.data_fields.iter().any(|d| {
            d.field as usize >= cache.fields.len()
                || !matches!(d.subtotal.as_str(),
                    "sum" | "count" | "countNums" | "min" | "max" | "average" | "product" | "stdDev" | "stdDevp" | "var" | "varp")
                || d.attributes.get("showDataAs").is_some_and(|v| v != "normal" && !(percent || indexed || relative))
        })
    {
        return Err(bad("This pivot axis or value aggregation is not modelled"));
    }
    let value_sorts: Vec<_> = table.fields.iter().enumerate().filter_map(|(field, value)|
        value.sort_measure.map(|measure| (field, measure as usize))).collect();
    if !value_sorts.is_empty() && (value_sorts.len() != 1 || row_fields.len() != 1
        || row_values.is_some() || table.column_fields.iter().filter(|&&field| field != -2).count() > 1
        || !table.page_fields.is_empty() || !table.filters.is_empty() || show != "normal"
        || value_sorts.iter().any(|&(field, measure)| {
            !(row_fields.contains(&field) || table.column_fields.contains(&(field as i32)))
                || table.data_fields.get(measure).is_none_or(|data| data.subtotal != "sum")
        })) {
        return Err(bad("This value sort layout is not modelled"));
    }
    let custom_subtotal = |field: usize| table.fields[field].attributes.iter().any(|(name, value)| {
        name.ends_with("Subtotal") && name != "defaultSubtotal" && truthy(Some(value), false)
    });
    if row_fields.iter().filter(|&&field| field != usize::MAX).any(|&field| custom_subtotal(field)
        || (!rows_only && !truthy(table.fields[field].attributes.get("defaultSubtotal"), true))) {
        return Err(bad("Row fields without an automatic subtotal are not modelled in this layout"));
    }
    let compact_rows = truthy(table.fields[row].attributes.get("compact"), true);
    let outline_rows = truthy(table.fields[row].attributes.get("outline"), true);
    let tabular_values = row_values.is_some() && row_fields.len() == 3 && rows_only
        && !compact_rows && !outline_rows && row_values == Some(2);
    if row_values.is_some() && row_fields.len() == 3 && !tabular_values {
        return Err(bad("Values with two row fields are only modelled in tabular form"));
    }
    if multiple_rows && ((!compact_rows && !tabular_values && !rows_only)
        || row_fields.iter().filter(|&&field| field != usize::MAX).any(|&field|
            truthy(table.fields[field].attributes.get("compact"), true) != compact_rows
            || ((rows_only || tabular_values) && truthy(table.fields[field].attributes.get("outline"), true) != outline_rows))) {
        return Err(bad("This multi-field row form is not modelled"));
    }
    let form = if compact_rows { Form::Compact } else if outline_rows { Form::Outline } else { Form::Tabular };
    let raw_subtotal_top = truthy(table.fields[row].attributes.get("subtotalTop"), true);
    let subtotal_top = raw_subtotal_top && form != Form::Tabular;
    if multiple_rows && row_fields.iter().filter(|&&field| field != usize::MAX).any(|&field|
        truthy(table.fields[field].attributes.get("subtotalTop"), true) != raw_subtotal_top) {
        return Err(bad("Row fields with different subtotal positions are not modelled"));
    }
    let mut subtotal_on: Vec<_> = row_fields.iter().map(|&field| field == usize::MAX
        || truthy(table.fields[field].attributes.get("defaultSubtotal"), true)).collect();
    if rows_only && row_values.is_none() && row_fields.len() == 2 && subtotal_on.iter().any(|on| !on)
        && !(form == Form::Outline && !subtotal_on[0] && subtotal_on[1]) {
        return Err(bad("Row fields without a subtotal remain unmeasured in this layout"));
    }
    if let Some(level) = row_values {
        if level > 0 { subtotal_on[level - 1] = false; } else { subtotal_on[0] = false; }
    }
    let fill_down = truthy(table.fields[row].attributes.get("fillDownLabels"), false);
    let blank_after: Vec<_> = row_fields.iter().map(|&field| field != usize::MAX &&
        truthy(table.fields[field].attributes.get("insertBlankRow"), false)).collect();
    if (row_values.is_some() && blank_after.iter().any(|&blank| blank))
        || (!multiple_rows && blank_after[0]) || (1..row_fields.len().saturating_sub(1)).any(|i| blank_after[i]) {
        return Err(bad("Blank rows at this field depth are not modelled"));
    }
    let measure = table.data_fields[0].field as usize;
    let value_captions: Vec<String> = table.data_fields.iter().map(|data|
        data.name.clone().unwrap_or_else(|| cache.fields[data.field as usize].name.clone())).collect();
    let row_levels = row_fields.iter().map(|&field| {
        if field == usize::MAX {
            Ok(table.data_fields.iter().enumerate().map(|(i, data)| Group {
                key: PivotItem::Text(data.name.clone().unwrap_or_else(|| cache.fields[data.field as usize].name.clone())),
                shared: i as u32, first: i, total: Total::default(), old: i, hidden: false, excluded: false,
            }).collect())
        } else { field_groups(table, cache, snapshot, field, measure) }
    }).collect::<Result<Vec<_>, _>>()?;
    let rows = &row_levels[0];
    let positions = |groups: &[Group], preserve_items: bool| {
        let mut order: Vec<_> = (0..groups.len()).filter(|&i| !groups[i].hidden).collect();
        if preserve_items { order.sort_by_key(|&i| (groups[i].old, groups[i].first)); }
        let mut result = vec![None; groups.len()];
        for (position, i) in order.into_iter().enumerate() { result[i] = Some(position as u32); }
        result
    };
    let row_positions: Vec<_> = row_levels.iter().enumerate().map(|(level, groups)| {
        let field = row_fields[level];
        positions(groups, field != usize::MAX && table.fields[field].sort_measure.is_some())
    }).collect();
    let mut levels = Vec::new();
    let mut fields = row_fields.clone();
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
            let positions = positions(&groups, table.fields[field].sort_measure.is_some());
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
    let row_sorted: Vec<_> = row_levels.iter().map(|groups| sorted(groups)).collect();
    let level_sorted: Vec<_> = levels.iter().map(|level| sorted(&level.groups)).collect();
    let find = |groups: &[Group], sorted: &[usize], key: &PivotItem| {
        sorted.binary_search_by(|&i| compare(&groups[i].key, key).unwrap_or(Ordering::Equal))
            .map(|at| sorted[at]).map_err(|_| bad("A source label is missing from its field"))
    };
    let mut paths = BTreeSet::new();
    let mut totals = BTreeMap::<(usize, Vec<usize>, usize), Total>::new();
    let mut row_ids = BTreeMap::<Vec<usize>, usize>::new();
    let mut row_totals: Vec<BTreeMap<Vec<usize>, Total>> =
        (0..row_fields.len()).map(|_| BTreeMap::new()).collect();
    for record in &snapshot.records {
        if !page_allows(table, cache, record)? { continue; }
        let mut row_path = Vec::with_capacity(row_fields.len());
        for (level, &field) in row_fields.iter().enumerate() {
            row_path.push(if field == usize::MAX { 0 } else {
                find(&row_levels[level], &row_sorted[level], resolve(cache, record, field)?)?
            });
        }
        if row_path.iter().enumerate().any(|(level, &i)|
            row_levels[level][i].hidden || row_levels[level][i].excluded) {
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
            if let Some(i) = row_values { row_path[i] = m; }
            if let Some(i) = levels.iter().position(|level| level.field.is_none()) { indexes[i] = m; }
            paths.insert(indexes.clone());
            if paths.len() > 16_384 { return Err(bad("The pivot has too many column combinations")); }
            let value = resolve(cache, record, data.field as usize)?;
            let mut row_keys = Vec::with_capacity(row_path.len());
            if multiple_rows {
                for (depth, totals) in row_totals.iter_mut().enumerate() {
                    let path = row_path[..=depth].to_vec();
                    // Above Values, the node is a label only; its total is not displayed.
                    if row_values.is_none_or(|index| depth >= index) || m == 0 {
                        totals.entry(path.clone()).or_default().push(value)?;
                    }
                    let next = row_ids.len();
                    row_keys.push(*row_ids.entry(path).or_insert(next));
                }
            } else { row_keys.push(row_path[0]); }
            for depth in 0..=indexes.len() {
                let path = indexes[..depth].to_vec();
                for &key in &row_keys { totals.entry((key, path.clone(), m)).or_default().push(value)?; }
                totals.entry((usize::MAX, path, m)).or_default().push(value)?;
            }
        }
        if row_ids.len() > 1_000_000 || totals.len() > 1_000_000 {
            return Err(bad("The pivot has too many row and column combinations"));
        }
    }
    if row_values.is_some() && totals.values().any(|total| total.source_error().is_some()) {
        return Err(bad("Source errors with Values on rows are not modelled"));
    }
    if totals.iter().any(|((_, _, measure), total)| {
        let kind = &table.data_fields[*measure].subtotal;
        !total.supports(kind) || ((multiple_rows || levels.iter().any(|level| level.field.is_some()))
            && !total.supports_grouped_product(kind))
    }) {
        return Err(bad("This numeric input for the value function is not modelled"));
    }
    let mut columns = Vec::new();
    if rows_only {
        columns.push(ColumnEntry { path: Vec::new(), measure: 0, kind: "data" });
    } else {
        walk_axis(&mut ColumnWalk {
            paths: &paths,
            levels: &levels,
            measures: table.data_fields.len(),
            output: &mut columns,
        }, &mut Vec::new())?;
    }
    let row_grand = truthy(table.attributes.get("colGrandTotals"), true);
    if !values_only && !rows_only && truthy(table.attributes.get("rowGrandTotals"), true) {
        let column_measures = if row_values.is_some() { 1 } else { table.data_fields.len() };
        for measure in 0..column_measures {
            columns.push(ColumnEntry {
                path: Vec::new(),
                measure,
                kind: "grand",
            });
        }
    }
    let first = table.area.first();
    let header = if values_only || rows_only { 1 } else { levels.len() as u32 + 1 };
    let label_width = if (rows_only && form != Form::Compact) || tabular_values { row_fields.len() as u32 } else { 1 };
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
    if rows_only {
        if form == Form::Compact {
            cells.insert((0, 0), plain_text(table.attributes.get("rowHeaderCaption").map(String::as_str).unwrap_or("Row Labels")));
        } else {
            for (level, &field) in row_fields.iter().enumerate() {
                let label = if field == usize::MAX { table.attributes.get("dataCaption").map(String::as_str).unwrap_or("Values") }
                    else { &cache.fields[field].name };
                cells.insert((0, level as u32), plain_text(label));
            }
        }
        if row_values.is_none() { cells.insert((0, label_width), plain_text(&caption(0))); }
    } else if values_only {
        cells.insert((0, 0), plain_text(row_caption));
    } else {
        if table.data_fields.len() == 1 { cells.insert((0, 0), plain_text(&caption(0))); }
        let column_caption = table.attributes.get("colHeaderCaption").map(String::as_str)
            .unwrap_or_else(|| if compact { "Column Labels" } else { &cache.fields[levels[0].field.unwrap()].name });
        cells.insert((0, 1), plain_text(column_caption));
        cells.insert((header - 1, 0), plain_text(row_caption));
    }
    let mut col_items = Vec::new();
    if rows_only { col_items.push(axis_item("data", 0, Vec::new())); }
    let mut previous = Vec::new();
    for (i, entry) in columns.iter().enumerate() {
        if rows_only { continue; }
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
            let label = if table.data_fields.len() == 1 || row_values.is_some() {
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
    // baseItem names a position in the pivot field's saved item list, or the reserved
    // previous/next item identities. Restrict this calculation to the measured two-axis case.
    let relative_base = if relative {
        let attrs = &table.data_fields[0].attributes;
        let base = attrs.get("baseField").and_then(|v| v.parse::<usize>().ok())
            .ok_or_else(|| bad("Missing value calculation base field"))?;
        let item = attrs.get("baseItem").and_then(|v| v.parse::<u32>().ok())
            .ok_or_else(|| bad("Missing value calculation base item"))?;
        if item == 1_048_829 && matches!(show, "percent" | "percentDiff") {
            return Err(bad("Percentage of the next item is not modelled"));
        }
        let is_row = base == row;
        if !is_row && levels[0].field != Some(base) { return Err(bad("Unsupported value calculation base field")); }
        let groups = if is_row { &rows } else { &levels[0].groups };
        if show == "runTotal" && table.data_fields[0].subtotal != "sum" {
            return Err(bad("Running totals for this value function are not modelled"));
        }
        let base_index = match item {
            _ if show == "runTotal" => None,
            1_048_828 | 1_048_829 => None,
            _ => {
                let shared = table.fields[base].items.get(item as usize).and_then(|item| item.index)
                    .ok_or_else(|| bad("Invalid value calculation base item"))?;
                Some(groups.iter().position(|group| group.shared == shared && !group.hidden)
                    .ok_or_else(|| bad("Hidden or missing value calculation base item"))?)
            }
        };
        if totals.values().any(|total| matches!(total.result(&table.data_fields[0].subtotal).core_ref(), calc_value::CoreRef::Error(_))) {
            return Err(bad("Value calculation with errors is not modelled"));
        }
        if matches!(show, "percent" | "percentDiff") && totals.values().any(|total|
            matches!(total.result(&table.data_fields[0].subtotal).core_ref(), calc_value::CoreRef::Number(0.0))) {
            return Err(bad("Relative percentages with zero totals are not modelled"));
        }
        Some((is_row, item, base_index))
    } else { None };
    let visible_rows: Vec<_> = rows.iter().enumerate().filter(|(i, group)| !group.hidden
        && totals.contains_key(&(*i, Vec::new(), 0))).map(|(i, _)| i).collect();
    let visible_columns: Vec<_> = levels.first().into_iter().flat_map(|level| level.groups.iter().enumerate())
        .filter(|(i, group)| !group.hidden && totals.contains_key(&(usize::MAX, vec![*i], 0)))
        .map(|(i, _)| i).collect();
    let row_order: BTreeMap<_, _> = visible_rows.iter().enumerate().map(|(n, &i)| (i, n)).collect();
    let column_order: BTreeMap<_, _> = visible_columns.iter().enumerate().map(|(n, &i)| (i, n)).collect();
    let mut running = BTreeMap::<(usize, Vec<usize>), CalcValue>::new();
    if let Some((base_is_row, _, _)) = relative_base.filter(|_| show == "runTotal") {
        let numeric = |row, path: Vec<usize>| totals.get(&(row, path, 0))
            .map(|total| total.result("sum")).and_then(|value| match value.core_ref() {
                calc_value::CoreRef::Number(n) => Some(n), _ => None,
            }).unwrap_or(0.0);
        if base_is_row {
            for entry in &columns {
                let mut sum = 0.0;
                for &row in &visible_rows {
                    sum += numeric(row, entry.path.clone());
                    running.insert((row, entry.path.clone()), if sum.is_finite() { CalcValue::number(sum) }
                        else { CalcValue::error(WorksheetErrorCode::Num) });
                }
            }
        } else {
            for row in visible_rows.iter().copied().chain(std::iter::once(usize::MAX)) {
                let mut sum = 0.0;
                for &col in &visible_columns {
                    let path = vec![col];
                    sum += numeric(row, path.clone());
                    running.insert((row, path), if sum.is_finite() { CalcValue::number(sum) }
                        else { CalcValue::error(WorksheetErrorCode::Num) });
                }
            }
        }
    }
    if percent || indexed {
        let valid = |total: &Total| matches!(total.result(&table.data_fields[0].subtotal).core_ref(),
            calc_value::CoreRef::Number(n) if n != 0.0);
        let grand = totals.get(&(usize::MAX, Vec::new(), 0)).ok_or_else(|| bad("Missing grand total"))?;
        if ((show != "percentOfCol" || columns.iter().any(|entry| entry.kind == "grand")) && !valid(grand))
            || ((show == "percentOfCol" || indexed) && columns.iter().filter(|entry| entry.kind != "grand").any(|entry|
                totals.get(&(usize::MAX, entry.path.clone(), 0)).is_none_or(|total| !valid(total))))
            || ((show == "percentOfRow" || indexed) && rows.iter().enumerate().filter(|(_, group)| !group.hidden).any(|(i, _)|
                totals.get(&(i, Vec::new(), 0)).is_some_and(|total| !valid(total))))
            || totals.values().any(|total| matches!(total.result(&table.data_fields[0].subtotal).core_ref(), calc_value::CoreRef::Error(_))) {
            return Err(bad("Percentage with zero or error totals is not modelled"));
        }
    }
    let displayed = |total: &Total, row: usize, entry: &ColumnEntry| -> CalcValue {
        let value = total.result(&table.data_fields[entry.measure].subtotal);
        if let Some((base_is_row, base_item, fixed)) = relative_base {
            if (base_is_row && row == usize::MAX) || (!base_is_row && entry.kind == "grand") {
                return CalcValue::empty();
            }
            if show == "runTotal" {
                return running.get(&(row, entry.path.clone())).cloned().unwrap_or_else(CalcValue::empty);
            }
            let current = if base_is_row { row } else { entry.path[0] };
            let visible = if base_is_row { &visible_rows } else { &visible_columns };
            let reference = fixed.or_else(|| {
                let position = *if base_is_row { row_order.get(&current) } else { column_order.get(&current) }?;
                match base_item {
                    1_048_828 => position.checked_sub(1).map(|i| visible[i]),
                    1_048_829 => visible.get(position + 1).copied(),
                    _ => None,
                }
            });
            let reference = match reference {
                Some(reference) => reference,
                None if show == "percent" && base_item == 1_048_828 => current,
                None => return CalcValue::empty(),
            };
            if matches!(show, "difference" | "percentDiff") && reference == current {
                return CalcValue::empty();
            }
            let key = if base_is_row { (reference, entry.path.clone(), 0) }
                else { (row, vec![reference], 0) };
            let base = totals.get(&key).map(|total| total.result(&table.data_fields[0].subtotal));
            let current_number = match value.core_ref() {
                calc_value::CoreRef::Number(n) => Some(n),
                calc_value::CoreRef::Empty => None,
                _ => return CalcValue::empty(),
            };
            let base_number = base.as_ref().and_then(|v| match v.core_ref() {
                calc_value::CoreRef::Number(n) => Some(n), _ => None,
            });
            if show != "difference" && (current_number.is_none() || base_number.is_none()) {
                return CalcValue::error(WorksheetErrorCode::Null);
            }
            let n = current_number.unwrap_or(0.0);
            let b = base_number.unwrap_or(0.0);
            let result = match show {
                "difference" => n - b,
                "percent" if b != 0.0 => n / b,
                "percentDiff" if b != 0.0 => (n - b) / b,
                _ => return CalcValue::error(WorksheetErrorCode::Div0),
            };
            return if result.is_finite() { CalcValue::number(result) } else { CalcValue::error(WorksheetErrorCode::Num) };
        }
        if indexed {
            let kind = &table.data_fields[0].subtotal;
            let row_value = totals[&(row, Vec::new(), 0)].result(kind);
            let col_value = totals[&(usize::MAX, entry.path.clone(), 0)].result(kind);
            let grand_value = totals[&(usize::MAX, Vec::new(), 0)].result(kind);
            if let (calc_value::CoreRef::Number(n), calc_value::CoreRef::Number(r),
                calc_value::CoreRef::Number(c), calc_value::CoreRef::Number(g)) =
                (value.core_ref(), row_value.core_ref(), col_value.core_ref(), grand_value.core_ref()) {
                let result = ((n * g) / r) / c;
                return if result.is_finite() { CalcValue::number(result) } else { CalcValue::error(WorksheetErrorCode::Num) };
            }
            return CalcValue::empty();
        }
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
    let empty_total = Total::default();
    let mut axis = Vec::new();
    let mut r = header;
    if multiple_rows {
        let mut walk = Walk {
            levels: &row_levels,
            totals: &row_totals,
            position: &row_positions,
            form,
            subtotal_top,
            subtotal_on: &subtotal_on,
            blank_after: &blank_after,
            fill_down,
            kind: &table.data_fields[0].subtotal,
            value_column: label_width,
            r: header,
            printed: 0,
            cells: BTreeMap::new(),
            axis: Vec::new(),
            row_paths: Vec::new(),
            collect_paths: true,
            values_level: row_values,
            value_captions: &value_captions,
        };
        walk.visit(&mut Vec::new())?;
        for ((row, column), value) in walk.cells {
            if column < label_width { cells.insert((row, column), value); }
        }
        for (output_row, path, subtotal_measure) in walk.row_paths {
            let key = row_ids[&path];
            for (col, entry) in columns.iter().enumerate() {
                let measure = subtotal_measure.or_else(|| row_values.and_then(|level| path.get(level)).copied()).unwrap_or(entry.measure);
                if let Some(total) = totals.get(&(key, entry.path.clone(), measure)) {
                    cells.insert((output_row, col as u32 + label_width), total.result(&table.data_fields[measure].subtotal));
                }
            }
        }
        axis = walk.axis;
        r = walk.r;
    } else {
        for (i, group) in rows.iter().enumerate().filter(|(_, g)| !g.hidden) {
            if !totals.contains_key(&(i, Vec::new(), 0)) {
                continue;
            }
            cells.insert((r, 0), group_label(group)?);
            for (col, entry) in columns.iter().enumerate() {
                if let Some(total) = totals.get(&(i, entry.path.clone(), entry.measure)) {
                    cells.insert((r, col as u32 + label_width), displayed(total, i, entry));
                } else if relative {
                    cells.insert((r, col as u32 + label_width), displayed(&empty_total, i, entry));
                } else if percent || indexed {
                    cells.insert((r, col as u32 + label_width), CalcValue::number(0.));
                }
            }
            axis.push(axis_item("data", 0, vec![row_positions[0][i].unwrap()]));
            r += 1;
        }
    }
    if row_grand {
        // With several value functions on the same source field, Excel's captured grand row
        // carries the source error from Sum into its Count and Count Numbers cells as well.
        let common_error = if values_only && table.data_fields[0].subtotal == "sum"
            && table.data_fields.iter().all(|d| d.field == table.data_fields[0].field) {
            totals.get(&(usize::MAX, Vec::new(), 0)).and_then(Total::source_error)
        } else { None };
        let grand_measures = if row_values.is_some() { table.data_fields.len() } else { 1 };
        for measure in 0..grand_measures {
            let label = if row_values.is_some() { format!("Total {}", caption(measure)) }
                else { grand_caption.to_owned() };
            cells.insert((r, 0), plain_text(&label));
            for (col, entry) in columns.iter().enumerate() {
                let m = if row_values.is_some() { measure } else { entry.measure };
                if let Some(total) = totals.get(&(usize::MAX, entry.path.clone(), m)) {
                    let value = common_error.map(CalcValue::error).unwrap_or_else(|| {
                        if row_values.is_some() { total.result(&table.data_fields[m].subtotal) }
                        else { displayed(total, usize::MAX, entry) }
                    });
                    cells.insert((r, col as u32 + label_width), value);
                }
            }
            let mut item = axis_item("grand", 0, vec![0]);
            item.data_index = measure as u32;
            axis.push(item);
            r += 1;
        }
    }
    if u64::from(r) * (columns.len() as u64 + u64::from(label_width)) > 1_000_000 {
        return Err(bad("The pivot output exceeds one million cells"));
    }
    let last = CellKey::new(
        first.sheet(),
        first
            .row()
            .checked_add(r - 1)
            .ok_or_else(|| bad("Pivot output exceeds worksheet"))?,
        first
            .column()
            .checked_add(label_width - 1 + columns.len() as u32)
            .ok_or_else(|| bad("Pivot output exceeds worksheet"))?,
    )
    .map_err(|_| bad("Pivot output exceeds worksheet"))?;
    let mut next = table.clone();
    next.area = GridArea::new(first, last).map_err(|_| bad("Invalid pivot output area"))?;
    next.location.insert("firstHeaderRow".into(), if values_only { "0" } else { "1" }.into());
    next.location
        .insert("firstDataRow".into(), header.to_string());
    next.location.insert("firstDataCol".into(), label_width.to_string());
    let rebuild = |field: usize, groups: &[Group], positions: &[Option<u32>], add_default: bool| {
        let mut order: Vec<_> = (0..groups.len()).filter(|&i| positions[i].is_some()).collect();
        order.sort_by_key(|&i| positions[i]);
        let mut out = order.into_iter().map(|i| PivotFieldItem {
            index: Some(groups[i].shared),
            kind: "data".into(),
            hidden: false,
        }).collect::<Vec<_>>();
        out.extend(
            table.fields[field]
                .items
                .iter()
                .filter(|i| i.hidden)
                .cloned(),
        );
        if add_default {
            out.push(PivotFieldItem { index: None, kind: "default".into(), hidden: false });
        }
        out
    };
    for (level, &field) in row_fields.iter().enumerate() {
        if field != usize::MAX {
            let add_default = if row_values.is_some() || !rows_only { true }
                else if row_fields.len() == 1 { row_grand } else { subtotal_on[level] };
            next.fields[field].items = rebuild(field, &row_levels[level], &row_positions[level], add_default);
        }
    }
    for level in &levels {
        if let Some(field) = level.field {
            next.fields[field].items = rebuild(field, &level.groups, &level.positions, true);
        }
    }
    next.row_items = axis;
    if !rows_only { next.col_items = col_items; }
    for row in 0..r {
        for col in 0..(label_width + columns.len() as u32) {
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
