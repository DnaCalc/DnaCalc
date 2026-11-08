//! The seven CUBE functions, evaluated against a workbook's data model.
//!
//! A host gives the model through [`crate::WorkbookFacts::cube_model`]: the rows of each model
//! table and the measures. [`CubeModel`] answers the member and set expressions a formula names.
//!
//! Measured in Excel 16.0.20527 (64-bit) on 2026-11-07, with a data model over a worksheet table.
//! Cube functions reach the model only with `Application.ScreenUpdating` on; with it off a cell
//! stays #GETTING_DATA. A file stores `CUBEVALUE` and the others bare, each cell with a `vm` into
//! `mdxMetadata`, and `<v>` the shown value, so the value is kept on opening.
//!
//! - A connection that is not the workbook's, in any form, gives #N/A from every function.
//! - CUBEMEMBER is the member's caption, or the caption argument read as text (`""` shows empty),
//!   and holds the member. An unknown member, a number, empty text or an error is #N/A. An array
//!   or range is a tuple, shown as its last member; two members of one hierarchy are #N/A.
//! - CUBEVALUE filters by its members; each is a cell that holds a member or set, or text naming
//!   one: `[T].[C].&[key]`, `[T].[C].[key]`, `[T].[C].[All]`, `[Measures].[name]` or a caption alone.
//!   A set sums its members. Two members of one hierarchy are #VALUE!. An unknown member, a number
//!   and empty text are #N/A; a blank, a logical and an error are passed over. With no rows the
//!   result is empty text, as it is with no member at all. With members and no measure it is 1.
//! - CUBESET shows its caption, or empty text, and holds its set: `.Children`, `.Members` (with
//!   the All member), `.AllMembers`, `[T].[C].[C].Members`, `{a,b}`, TOPCOUNT and BOTTOMCOUNT over
//!   a measure, FILTER by a measure against a number, and CROSSJOIN, whose pairs are those with
//!   rows. sort_order 0 keeps natural order, 1 and 2 sort by sort_by ascending and descending
//!   (no sort_by is #VALUE!), 3 and 4 by caption, 5 and 6 naturally; anything else is #N/A.
//! - CUBESETCOUNT counts a set. A cell holding a member is #VALUE!; anything else is #N/A.
//! - CUBERANKEDMEMBER takes a set cell or set text and a rank read as a truncated number; a rank
//!   outside the set is #N/A.
//! - CUBEMEMBERPROPERTY is #N/A for every property tried on a data model member.
//! - CUBEKPIMEMBER: a text property is #VALUE!, and a number is #N/A for a model with no KPI.
//! - A SUM over a column whose every value has at most 6 decimal places is exact, then the nearest
//!   binary64 (42.3, where binary64 addition gives 42.300000000000004); with one value of 7 or more
//!   places it adds binary64 in row order. So the model holds such a column as scaled integers.
//!   Values near 1E+10 with 6 places fit neither rule; they are read by the second here, which
//!   can differ in the last places. AVERAGE is that sum over the count in binary64.
//! - Keys: a blank is a member, shown empty and first (Excel showed one such member as `(blank)`,
//!   for reasons not established); keys equal in any letter case are one, shown as the first row
//!   spells it; numbers sort as numbers; logicals show `True` and `False`; the All member shows
//!   `All`. A caption alone must be a text key (`"2024"` is #N/A).
//! - Measures read: SUM, AVERAGE, COUNT (every kind of cell), COUNTA, COUNTBLANK, MIN, MAX (of text
//!   too), DISTINCTCOUNT (a blank counts), COUNTROWS. With no rows, or a count of none, a
//!   value is empty text.
//! - Editing a source cell changes nothing until the model refreshes; the host supplies the
//!   retained model snapshot, and no dependency on the table's cells is kept.
//!
//! Other set expressions, and DAX beyond one aggregate of one column, are refused as not
//! modelled, as is a table column holding a formula.

use crate::value::{CalcValue, CubeValue, ExcelText, WorksheetErrorCode};
use std::cmp::Ordering;

/// One column of a model table, its cells in row order.
#[derive(Clone, Debug, PartialEq)]
pub struct CubeColumn {
    pub name: String,
    pub cells: Vec<CalcValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CubeTable {
    pub name: String,
    pub columns: Vec<CubeColumn>,
}

/// A measure as the model defines it: `SUM(Table1[Amount])`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CubeMeasure {
    pub name: String,
    pub expression: String,
}

/// A workbook's data model, read when a CUBE function asks for it.
#[derive(Clone, Debug, PartialEq)]
pub struct CubeModel {
    pub connection: String,
    pub tables: Vec<CubeTable>,
    pub measures: Vec<CubeMeasure>,
}

/// What the host knows of a connection name.
#[derive(Clone, Debug, PartialEq)]
pub enum CubeLookup {
    /// Not a connection of the workbook: #N/A.
    NoConnection,
    /// A connection whose data this host cannot read: refused.
    NotReadable,
    Model(std::sync::Arc<CubeModel>),
}

/// Why a cube expression has no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CubeFailure {
    Error(WorksheetErrorCode),
    /// Beyond what is modelled; the caller refuses.
    NotModelled,
    /// Cross-table model relationships have not been decoded (dc-8wp.19).
    CrossTableNotModelled,
}

const NA: CubeFailure = CubeFailure::Error(WorksheetErrorCode::NA);
const VALUE: CubeFailure = CubeFailure::Error(WorksheetErrorCode::Value);

/// A member of the model: a measure, or a key of a column, or a column's All member.
#[derive(Clone, Debug, PartialEq)]
pub enum CubeMember {
    Measure(usize),
    All { table: usize, column: usize },
    Key {
        table: usize,
        column: usize,
        key: CalcValue,
    },
}

fn folded_eq(a: &str, b: &str) -> bool {
    crate::value::fold_workbook_name(a) == crate::value::fold_workbook_name(b)
}

fn key_text(key: &CalcValue) -> String {
    use crate::value::CoreRef;
    match key.core_ref() {
        CoreRef::Text(text) => text.to_string_lossy(),
        CoreRef::Number(number) => format!("{number}"),
        CoreRef::Logical(logical) => if logical { "True" } else { "False" }.to_owned(),
        _ => String::new(),
    }
}

fn key_order(a: &CalcValue, b: &CalcValue) -> Ordering {
    use crate::value::CoreRef;
    match (a.core_ref(), b.core_ref()) {
        (CoreRef::Empty, CoreRef::Empty) => Ordering::Equal,
        (CoreRef::Empty, _) => Ordering::Less,
        (_, CoreRef::Empty) => Ordering::Greater,
        (CoreRef::Number(a), CoreRef::Number(b)) => a.total_cmp(&b),
        (CoreRef::Number(_), _) => Ordering::Less,
        (_, CoreRef::Number(_)) => Ordering::Greater,
        _ => crate::value::fold_workbook_name(&key_text(a))
            .cmp(&crate::value::fold_workbook_name(&key_text(b))),
    }
}

/// The bracketed parts of a name such as `[T].[C].&[K]`, each with whether `&` marks it as a key.
fn name_parts(text: &str) -> Option<Vec<(String, bool)>> {
    let mut parts = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let key = rest.starts_with('&');
        rest = rest.strip_prefix('&').unwrap_or(rest);
        rest = rest.strip_prefix('[')?;
        let mut name = String::new();
        let mut chars = rest.char_indices();
        let end = loop {
            let (at, c) = chars.next()?;
            if c == ']' {
                if rest[at + 1..].starts_with(']') {
                    name.push(']');
                    chars.next();
                } else {
                    break at + 1;
                }
            } else {
                name.push(c);
            }
        };
        parts.push((name, key));
        rest = &rest[end..];
        if rest.is_empty() {
            break;
        }
        rest = rest.strip_prefix('.')?;
    }
    (!parts.is_empty()).then_some(parts)
}

impl CubeModel {
    fn column(&self, table: &str, column: &str) -> Option<(usize, usize)> {
        let at = self.tables.iter().position(|t| folded_eq(&t.name, table))?;
        let col = self.tables[at]
            .columns
            .iter()
            .position(|c| folded_eq(&c.name, column))?;
        Some((at, col))
    }

    fn measure(&self, name: &str) -> Option<usize> {
        self.measures.iter().position(|m| folded_eq(&m.name, name))
    }

    /// The column's distinct keys in natural order: a blank first, keys equal in any letter case
    /// as one, spelled as the first row spells it.
    fn keys(&self, table: usize, column: usize) -> Vec<CalcValue> {
        let mut keys: Vec<CalcValue> = self.tables[table].columns[column].cells.clone();
        keys.sort_by(key_order);
        keys.dedup_by(|a, b| key_order(a, b) == Ordering::Equal);
        keys
    }

    fn key(&self, table: usize, column: usize, name: &str) -> Option<CalcValue> {
        self.keys(table, column)
            .into_iter()
            .find(|key| folded_eq(&key_text(key), name))
    }

    /// The member a text names, or the failure Excel gives.
    pub fn member(&self, text: &str) -> Result<CubeMember, CubeFailure> {
        if text.trim().is_empty() {
            return Err(NA);
        }
        let Some(parts) = name_parts(text) else {
            // A caption alone names a text key of the first column that holds one.
            for (table, held) in self.tables.iter().enumerate() {
                for column in 0..held.columns.len() {
                    if let Some(key) = self.key(table, column, text.trim())
                        && key.as_text().is_some()
                    {
                        return Ok(CubeMember::Key { table, column, key });
                    }
                }
            }
            return self.measure(text.trim()).map(CubeMember::Measure).ok_or(NA);
        };
        let names: Vec<&str> = parts.iter().map(|(name, _)| name.as_str()).collect();
        match names.as_slice() {
            ["Measures", name] => self.measure(name).map(CubeMember::Measure).ok_or(NA),
            [table, column] => {
                let (table, column) = self.column(table, column).ok_or(NA)?;
                Ok(CubeMember::All { table, column })
            }
            [table, column, member] => {
                let (table, column) = self.column(table, column).ok_or(NA)?;
                if !parts[2].1 && folded_eq(member, "All") {
                    return Ok(CubeMember::All { table, column });
                }
                let key = self.key(table, column, member).ok_or(NA)?;
                Ok(CubeMember::Key { table, column, key })
            }
            _ => Err(NA),
        }
    }

    /// Resolve an MDX measure spelling for a model-pivot query.
    pub fn pivot_measure(&self, text: &str) -> Result<CubeMember, CubeFailure> {
        let missing = CubeFailure::Error(WorksheetErrorCode::Ref);
        if text.trim_end().ends_with('.') { return Err(missing); }
        let parts = name_parts(text).ok_or(missing)?;
        if parts.len() != 2 || parts.iter().any(|(_, key)| *key) || !folded_eq(&parts[0].0, "Measures") {
            return Err(missing);
        }
        self.measure(&parts[1].0).map(CubeMember::Measure).ok_or(missing)
    }

    /// Resolve a hierarchy and its unique key with the same member parser as CUBE formulas.
    pub fn pivot_key(&self, hierarchy: &str, text: &str) -> Result<CubeMember, CubeFailure> {
        let missing = CubeFailure::Error(WorksheetErrorCode::Ref);
        if hierarchy.trim_end().ends_with('.') || text.trim_end().ends_with('.') { return Err(missing); }
        let field = name_parts(hierarchy).ok_or(missing)?;
        let key = name_parts(text).ok_or(missing)?;
        if field.len() != 2 || field.iter().any(|(_, marked)| *marked)
            || key.len() != 3 || key[0].1 || key[1].1 || !key[2].1
            || !folded_eq(&field[0].0, &key[0].0) || !folded_eq(&field[1].0, &key[1].0) {
            return Err(missing);
        }
        match self.member(text) {
            Ok(member @ CubeMember::Key { .. }) => Ok(member),
            _ => Err(missing),
        }
    }

    /// The member's unique name, as a [`CubeValue`] holds it.
    pub fn unique_name(&self, member: &CubeMember) -> String {
        match member {
            CubeMember::Measure(at) => format!("[Measures].[{}]", self.measures[*at].name),
            CubeMember::All { table, column } => format!(
                "[{}].[{}].[All]",
                self.tables[*table].name, self.tables[*table].columns[*column].name
            ),
            CubeMember::Key { table, column, key } => format!(
                "[{}].[{}].&[{}]",
                self.tables[*table].name,
                self.tables[*table].columns[*column].name,
                key_text(key)
            ),
        }
    }

    /// What a member shows.
    pub fn caption(&self, member: &CubeMember) -> String {
        match member {
            CubeMember::Measure(at) => self.measures[*at].name.clone(),
            CubeMember::All { .. } => "All".to_owned(),
            CubeMember::Key { key, .. } => key_text(key),
        }
    }

    fn hierarchy(member: &CubeMember) -> Option<(usize, usize)> {
        match member {
            CubeMember::Measure(_) => None,
            CubeMember::All { table, column } | CubeMember::Key { table, column, .. } => {
                Some((*table, *column))
            }
        }
    }

    /// Whether two members lie on one hierarchy, the measures being one.
    pub fn same_hierarchy(a: &CubeMember, b: &CubeMember) -> bool {
        Self::hierarchy(a) == Self::hierarchy(b)
    }

    /// The tuples a set expression names, in natural order.
    pub fn set(&self, text: &str) -> Result<Vec<Vec<CubeMember>>, CubeFailure> {
        let text = text.trim();
        if text.is_empty() {
            return Err(NA);
        }
        if let Some(inner) = text.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
            return split_top(inner)
                .into_iter()
                .map(|part| self.member(part).map(|member| vec![member]))
                .collect();
        }
        let upper = text.to_ascii_uppercase();
        for (name, top) in [("TOPCOUNT(", true), ("BOTTOMCOUNT(", false)] {
            if upper.starts_with(name) && text.ends_with(')') {
                let inner = &text[name.len()..text.len() - 1];
                return self.top_count(&split_top(inner), top);
            }
        }
        if upper.starts_with("CROSSJOIN(") && text.ends_with(')') {
            return self.crossjoin(&split_top(&text["CROSSJOIN(".len()..text.len() - 1]));
        }
        if upper.starts_with("FILTER(") && text.ends_with(')') {
            return self.filtered(&split_top(&text["FILTER(".len()..text.len() - 1]));
        }
        if text.contains('(') {
            return Err(CubeFailure::NotModelled);
        }
        for (suffix, whole) in [(".Children", false), (".AllMembers", true), (".Members", true)] {
            if upper.ends_with(&suffix.to_ascii_uppercase()) {
                return self.hierarchy_set(&text[..text.len() - suffix.len()], whole);
            }
        }
        self.member(text).map(|member| vec![vec![member]])
    }

    fn hierarchy_set(&self, name: &str, whole: bool) -> Result<Vec<Vec<CubeMember>>, CubeFailure> {
        let parts = name_parts(name).ok_or(NA)?;
        let names: Vec<&str> = parts.iter().map(|(name, _)| name.as_str()).collect();
        if names == ["Measures"] {
            return Ok(Vec::new());
        }
        let (table, column, whole) = match names.as_slice() {
            [table, column] => {
                let (table, column) = self.column(table, column).ok_or(NA)?;
                (table, column, whole)
            }
            // `[T].[C].[C].Members` is the level and `[T].[C].[All].Children` its members.
            [table, column, _] => {
                let (table, column) = self.column(table, column).ok_or(NA)?;
                (table, column, false)
            }
            _ => return Err(NA),
        };
        let mut tuples = Vec::new();
        if whole {
            tuples.push(vec![CubeMember::All { table, column }]);
        }
        tuples.extend(self.keys(table, column).into_iter().map(|key| {
            vec![CubeMember::Key {
                table,
                column,
                key,
            }]
        }));
        Ok(tuples)
    }

    fn top_count(&self, parts: &[&str], top: bool) -> Result<Vec<Vec<CubeMember>>, CubeFailure> {
        let [set, count, measure] = parts else {
            return Err(CubeFailure::NotModelled);
        };
        let tuples = self.set(set)?;
        let count: usize = count.trim().parse().map_err(|_| CubeFailure::NotModelled)?;
        let measure = self.member(measure)?;
        let mut sorted = self.sorted(tuples, &measure, top)?;
        sorted.truncate(count);
        Ok(sorted)
    }

    /// The pairs of `parts` that have rows, as the model's auto-exist gives them.
    fn crossjoin(&self, parts: &[&str]) -> Result<Vec<Vec<CubeMember>>, CubeFailure> {
        let [left, right] = parts else {
            return Err(CubeFailure::NotModelled);
        };
        let (left, right) = (self.set(left)?, self.set(right)?);
        let mut joined = Vec::new();
        for a in &left {
            for b in &right {
                let tuple: Vec<CubeMember> = a.iter().chain(b).cloned().collect();
                if self.value(std::slice::from_ref(&tuple))?.as_number().is_some() {
                    joined.push(tuple);
                }
            }
        }
        Ok(joined)
    }

    /// `FILTER(set,[Measures].[m]>n)`, the one condition modelled: a measure against a number.
    fn filtered(&self, parts: &[&str]) -> Result<Vec<Vec<CubeMember>>, CubeFailure> {
        let [set, condition] = parts else {
            return Err(CubeFailure::NotModelled);
        };
        let at = condition
            .find(['<', '>', '='])
            .ok_or(CubeFailure::NotModelled)?;
        let (measure, rest) = condition.split_at(at);
        let operator_length = rest.chars().take_while(|c| "<>=".contains(*c)).count();
        let (operator, limit) = rest.split_at(operator_length);
        let limit: f64 = limit.trim().parse().map_err(|_| CubeFailure::NotModelled)?;
        let measure = self.member(measure)?;
        let holds = |value: f64| match operator {
            ">" => value > limit,
            ">=" => value >= limit,
            "<" => value < limit,
            "<=" => value <= limit,
            "=" => value == limit,
            "<>" => value != limit,
            _ => false,
        };
        let mut kept = Vec::new();
        for tuple in self.set(set)? {
            let mut filter = tuple.clone();
            filter.push(measure.clone());
            if self.value(&[filter])?.as_number().is_some_and(holds) {
                kept.push(tuple);
            }
        }
        Ok(kept)
    }

    fn sorted(
        &self,
        tuples: Vec<Vec<CubeMember>>,
        measure: &CubeMember,
        descending: bool,
    ) -> Result<Vec<Vec<CubeMember>>, CubeFailure> {
        let mut keyed = Vec::with_capacity(tuples.len());
        for tuple in tuples {
            let mut filter = tuple.clone();
            filter.push(measure.clone());
            let value = self.value(&[filter])?;
            keyed.push((value.as_number().unwrap_or(f64::NEG_INFINITY), tuple));
        }
        keyed.sort_by(|a, b| {
            let order = a.0.total_cmp(&b.0);
            if descending { order.reverse() } else { order }
        });
        Ok(keyed.into_iter().map(|(_, tuple)| tuple).collect())
    }

    /// A set ordered as CUBESET's sort_order says.
    pub fn ordered(
        &self,
        tuples: Vec<Vec<CubeMember>>,
        sort_order: i64,
        sort_by: Option<&str>,
    ) -> Result<Vec<Vec<CubeMember>>, CubeFailure> {
        let caption = |tuple: &Vec<CubeMember>| {
            crate::value::fold_workbook_name(&tuple.last().map(|m| self.caption(m)).unwrap_or_default())
        };
        match sort_order {
            0 | 5 => Ok(tuples),
            6 => Ok(tuples.into_iter().rev().collect()),
            1 | 2 => {
                let measure = self.member(sort_by.ok_or(VALUE)?)?;
                self.sorted(tuples, &measure, sort_order == 2)
            }
            3 | 4 => {
                let mut tuples = tuples;
                tuples.sort_by_key(caption);
                if sort_order == 4 {
                    tuples.reverse();
                }
                Ok(tuples)
            }
            _ => Err(NA),
        }
    }

    /// The tuple or set of tuples a [`CubeValue`] holds, named again against this model.
    pub fn held(&self, value: &CubeValue) -> Result<Vec<Vec<CubeMember>>, CubeFailure> {
        value
            .tuples
            .iter()
            .map(|tuple| tuple.iter().map(|name| self.member(name)).collect())
            .collect()
    }

    /// The value over `filters`, each a list of members that together select the rows, one list
    /// of alternatives per argument. The first measure among them is the value.
    pub fn value(&self, filters: &[Vec<CubeMember>]) -> Result<CalcValue, CubeFailure> {
        let alternatives: Vec<Vec<Vec<CubeMember>>> =
            filters.iter().map(|tuple| vec![tuple.clone()]).collect();
        self.value_over(&alternatives)
    }

    /// As [`CubeModel::value`], where each argument offers alternative tuples (a set).
    pub fn value_over(&self, arguments: &[Vec<Vec<CubeMember>>]) -> Result<CalcValue, CubeFailure> {
        let mut seen: Vec<CubeMember> = Vec::new();
        let mut measure = None;
        for argument in arguments {
            if let [tuple] = argument.as_slice() {
                for member in tuple {
                    if seen.iter().any(|held| Self::same_hierarchy(held, member)) {
                        return Err(VALUE);
                    }
                    seen.push(member.clone());
                }
            }
            for tuple in argument {
                for member in tuple {
                    if let CubeMember::Measure(at) = member {
                        measure = Some(*at);
                    }
                }
            }
        }
        if arguments.is_empty() {
            return Ok(empty_text());
        }
        let table = self.fact_table(arguments, measure)?;
        // Relationships between model tables are not decoded yet. Silently ignoring a member
        // from another table would publish an unfiltered measure as if it had been filtered.
        // This check also covers every alternative in a set and All members.
        if arguments.iter().flatten().flatten().any(|member|
            Self::hierarchy(member).is_some_and(|(member_table, _)| member_table != table)) {
            return Err(CubeFailure::CrossTableNotModelled);
        }
        let rows = self.tables[table]
            .columns
            .first()
            .map_or(0, |column| column.cells.len());
        let selected: Vec<usize> = (0..rows)
            .filter(|row| arguments.iter().all(|argument| self.row_in(table, *row, argument)))
            .collect();
        let Some(measure) = measure else {
            return Ok(if selected.is_empty() {
                empty_text()
            } else {
                CalcValue::number(1.0)
            });
        };
        if selected.is_empty() {
            return Ok(empty_text());
        }
        self.aggregate(&self.measures[measure].expression, table, &selected)
    }

    fn fact_table(
        &self,
        arguments: &[Vec<Vec<CubeMember>>],
        measure: Option<usize>,
    ) -> Result<usize, CubeFailure> {
        if let Some(measure) = measure {
            let (_, table, _) = parse_measure(&self.measures[measure].expression)?;
            return self
                .tables
                .iter()
                .position(|t| folded_eq(&t.name, &table))
                .ok_or(CubeFailure::NotModelled);
        }
        for member in arguments.iter().flatten().flatten() {
            if let Some((table, _)) = Self::hierarchy(member) {
                return Ok(table);
            }
        }
        if self.tables.is_empty() { Err(CubeFailure::NotModelled) } else { Ok(0) }
    }

    fn row_in(&self, table: usize, row: usize, argument: &[Vec<CubeMember>]) -> bool {
        argument.iter().any(|tuple| {
            tuple.iter().all(|member| match member {
                CubeMember::Key {
                    table: held,
                    column,
                    key,
                } if *held == table => self.tables[table].columns[*column]
                    .cells
                    .get(row)
                    .is_some_and(|cell| key_order(cell, key) == Ordering::Equal),
                _ => true,
            })
        })
    }

    fn aggregate(
        &self,
        expression: &str,
        table: usize,
        rows: &[usize],
    ) -> Result<CalcValue, CubeFailure> {
        let (function, _, column) = parse_measure(expression)?;
        if function == "COUNTROWS" {
            return Ok(CalcValue::number(rows.len() as f64));
        }
        // Blank rows make no member of a measure; COUNT counts every kind of cell.
        let column = column.ok_or(CubeFailure::NotModelled)?;
        let held = self.tables[table]
            .columns
            .iter()
            .find(|c| folded_eq(&c.name, &column))
            .ok_or(CubeFailure::NotModelled)?;
        let cells: Vec<&CalcValue> = rows.iter().filter_map(|row| held.cells.get(*row)).collect();
        if function == "DISTINCTCOUNT" {
            let mut distinct: Vec<CalcValue> = cells.iter().map(|cell| (*cell).clone()).collect();
            distinct.sort_by(key_order);
            distinct.dedup_by(|a, b| key_order(a, b) == Ordering::Equal);
            return Ok(CalcValue::number(distinct.len() as f64));
        }
        let filled = cells.iter().filter(|cell| !cell.is_empty()).count();
        match function.as_str() {
            "COUNT" | "COUNTA" => {
                return Ok(if filled == 0 {
                    empty_text()
                } else {
                    CalcValue::number(filled as f64)
                });
            }
            "COUNTBLANK" => {
                let blanks = cells.len() - filled;
                return Ok(if blanks == 0 {
                    empty_text()
                } else {
                    CalcValue::number(blanks as f64)
                });
            }
            _ => {}
        }
        let numbers: Vec<f64> = cells.iter().filter_map(|cell| cell.as_number()).collect();
        // MIN and MAX of a text column are its first and last key.
        if numbers.is_empty() && matches!(function.as_str(), "MIN" | "MAX") {
            let mut texts: Vec<CalcValue> = cells
                .iter()
                .filter(|cell| !cell.is_empty())
                .map(|cell| (*cell).clone())
                .collect();
            texts.sort_by(key_order);
            let chosen = if function == "MIN" { texts.first() } else { texts.last() };
            return Ok(chosen.cloned().unwrap_or_else(empty_text));
        }
        if numbers.is_empty() {
            return Ok(empty_text());
        }
        // The measured column-wide rule implies one representation for every slice: a value
        // outside the selected rows can determine whether a SUM uses scaled integers or binary64
        // addition. A sliced column with seven decimal places has not yet been captured.
        let sum = || {
            let all_numbers: Vec<f64> = held.cells.iter().filter_map(CalcValue::as_number).collect();
            if scaled_sum(&all_numbers).is_some() {
                scaled_sum(&numbers).unwrap_or_else(|| numbers.iter().sum())
            } else {
                numbers.iter().sum()
            }
        };
        Ok(CalcValue::number(match function.as_str() {
            "SUM" => sum(),
            "AVERAGE" => sum() / numbers.len() as f64,
            "MIN" => numbers.iter().copied().fold(f64::INFINITY, f64::min),
            "MAX" => numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            _ => return Err(CubeFailure::NotModelled),
        }))
    }
}

/// The most decimal places, and the largest magnitude, of a column held as scaled integers.
const SCALED_PLACES: usize = 6;
const SCALED_LIMIT: f64 = 1e9;

/// The exact sum of `numbers` when every one has at most [`SCALED_PLACES`] decimal places, as the
/// nearest binary64.
fn scaled_sum(numbers: &[f64]) -> Option<f64> {
    let mut sum = 0_i128;
    for number in numbers {
        if number.abs() >= SCALED_LIMIT {
            return None;
        }
        let text = format!("{}", number.abs());
        let (whole, places) = text.split_once('.').unwrap_or((&text, ""));
        if places.len() > SCALED_PLACES {
            return None;
        }
        let digits = format!("{whole}{places:0<width$}", width = SCALED_PLACES);
        let scaled: i128 = digits.parse().ok()?;
        sum += if *number < 0.0 { -scaled } else { scaled };
    }
    format!("{sum}e-{SCALED_PLACES}").parse().ok()
}

fn empty_text() -> CalcValue {
    CalcValue::text(ExcelText::from_utf16_code_units(Vec::new()))
}

/// The parts of a comma-separated list, outside brackets, braces and parentheses.
fn split_top(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut brackets, mut from) = (0_i32, false, 0);
    for (at, c) in text.char_indices() {
        match c {
            '[' => brackets = true,
            ']' => brackets = false,
            '(' | '{' if !brackets => depth += 1,
            ')' | '}' if !brackets => depth -= 1,
            ',' if !brackets && depth == 0 => {
                parts.push(&text[from..at]);
                from = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[from..]);
    parts
}

/// A measure of one aggregate over one column, or COUNTROWS of a table:
/// (function, table, column).
fn parse_measure(expression: &str) -> Result<(String, String, Option<String>), CubeFailure> {
    let text = expression.trim();
    let open = text.find('(').ok_or(CubeFailure::NotModelled)?;
    let inner = text[open + 1..]
        .strip_suffix(')')
        .ok_or(CubeFailure::NotModelled)?
        .trim();
    let function = text[..open].trim().to_ascii_uppercase();
    if !matches!(
        function.as_str(),
        "SUM" | "COUNT" | "COUNTA" | "COUNTBLANK" | "AVERAGE" | "MIN" | "MAX" | "DISTINCTCOUNT"
            | "COUNTROWS"
    ) || inner.contains('(')
    {
        return Err(CubeFailure::NotModelled);
    }
    let unquoted = |name: &str| name.trim().trim_matches('\'').to_owned();
    match inner.split_once('[') {
        Some((table, column)) => {
            let column = column.strip_suffix(']').ok_or(CubeFailure::NotModelled)?;
            Ok((function, unquoted(table), Some(column.to_owned())))
        }
        None if function == "COUNTROWS" => Ok((function, unquoted(inner), None)),
        None => Err(CubeFailure::NotModelled),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> CalcValue {
        CalcValue::text(ExcelText::from_utf16_code_units(value.encode_utf16().collect()))
    }

    /// The table of the measured workbook.
    fn model() -> CubeModel {
        let rows: [(&str, &str, f64); 6] = [
            ("East", "A", 10.0),
            ("West", "A", 20.0),
            ("East", "B", 5.0),
            ("North", "B", 7.0),
            ("West", "C", 0.1),
            ("East", "A", 0.2),
        ];
        let column = |name: &str, cells: Vec<CalcValue>| CubeColumn {
            name: name.to_owned(),
            cells,
        };
        CubeModel {
            connection: "ThisWorkbookDataModel".to_owned(),
            tables: vec![CubeTable {
                name: "Table1".to_owned(),
                columns: vec![
                    column("Region", rows.iter().map(|r| text(r.0)).collect()),
                    column("Product", rows.iter().map(|r| text(r.1)).collect()),
                    column("Amount", rows.iter().map(|r| CalcValue::number(r.2)).collect()),
                ],
            }],
            measures: [
                ("TotalAmount", "SUM(Table1[Amount])"),
                ("CountAmount", "COUNT(Table1[Amount])"),
                ("AvgAmount", "AVERAGE(Table1[Amount])"),
                ("MinAmount", "MIN(Table1[Amount])"),
                ("MaxAmount", "MAX(Table1[Amount])"),
                ("DistinctRegion", "DISTINCTCOUNT(Table1[Region])"),
                ("RowCount", "COUNTROWS(Table1)"),
            ]
            .map(|(name, expression)| CubeMeasure {
                name: name.to_owned(),
                expression: expression.to_owned(),
            })
            .to_vec(),
        }
    }

    fn value(model: &CubeModel, members: &[&str]) -> Result<CalcValue, CubeFailure> {
        let arguments: Result<Vec<Vec<Vec<CubeMember>>>, CubeFailure> = members
            .iter()
            .map(|member| model.member(member).map(|member| vec![vec![member]]))
            .collect();
        model.value_over(&arguments?)
    }

    /// The M4 model-pivot capture's MDX arguments, parsed without accepting captions as names.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_8wp_19__model_pivot_mdx_members_use_cube_member_identity() {
        let model = model();
        let total = model.pivot_measure("[Measures].[TotalAmount]").unwrap();
        assert_eq!(total, model.member("[Measures].[TotalAmount]").unwrap());
        let east = model.pivot_key("[Table1].[Region]", "[Table1].[Region].&[East]").unwrap();
        let product = model.pivot_key("[Table1].[Product]", "[Table1].[Product].&[A]").unwrap();
        assert_eq!(east, model.member("[Table1].[Region].&[East]").unwrap());
        assert_eq!(product, model.member("[Table1].[Product].&[A]").unwrap());
        let missing = CubeFailure::Error(WorksheetErrorCode::Ref);
        assert_eq!(model.pivot_measure("TotalAmount"), Err(missing));
        assert_eq!(model.pivot_measure("[Measures].[TotalAmount]."), Err(missing));
        assert_eq!(model.pivot_measure("[Measures].[Missing]"), Err(missing));
        assert_eq!(model.pivot_key("[Table1].[Region]", "East"), Err(missing));
        assert_eq!(model.pivot_key("[Table1].[Region].", "[Table1].[Region].&[East]"), Err(missing));
        assert_eq!(model.pivot_key("[Table1].[Region]", "[Table1].[Region].&[East]."), Err(missing));
        assert_eq!(model.pivot_key("[Table1].[Region]", "[Table1].[Region].&[Nowhere]"), Err(missing));
        assert_eq!(model.pivot_key("[Table1].[Region]", "[Table1].[Product].&[A]"), Err(missing));
        assert_eq!(model.pivot_key("[Table1].[Region].[Region]", "[Table1].[Region].[Region].&[East]"), Err(missing));
    }

    /// Unmodelled relationships cannot silently turn a cross-table slice into an unfiltered sum.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_8wp_19__cross_table_members_refuse_without_relationships() {
        let mut model = model();
        model.tables.push(CubeTable { name: "Other".into(), columns: vec![CubeColumn {
            name: "Region".into(), cells: vec![text("East"), text("West")],
        }] });
        let total = model.member("[Measures].[TotalAmount]").unwrap();
        let east = model.member("[Table1].[Region].&[East]").unwrap();
        let other_east = model.member("[Other].[Region].&[East]").unwrap();
        let other_all = model.member("[Other].[Region].[All]").unwrap();
        assert_eq!(model.value(&[vec![total.clone()], vec![east.clone()]]), Ok(CalcValue::number(15.2)));
        assert_eq!(model.value(&[vec![total.clone()], vec![other_east.clone()]]), Err(CubeFailure::CrossTableNotModelled));
        assert_eq!(model.value(&[vec![total.clone()], vec![other_all]]), Err(CubeFailure::CrossTableNotModelled));
        assert_eq!(model.value_over(&[vec![vec![total]], vec![vec![east], vec![other_east]]]), Err(CubeFailure::CrossTableNotModelled));
        model.tables.clear();
        assert_eq!(model.value(&[vec![]]), Err(CubeFailure::NotModelled));
    }

    /// A column with a blank, keys differing in case, numbers and logicals (the p44 measurement).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn keys_are_excels() {
        let blank = CalcValue::empty;
        let n = CalcValue::number;
        let l = CalcValue::logical;
        let column = |name: &str, cells: Vec<CalcValue>| CubeColumn {
            name: name.to_owned(),
            cells,
        };
        let model = CubeModel {
            connection: "M".to_owned(),
            tables: vec![CubeTable {
                name: "Table1".to_owned(),
                columns: vec![
                    column("Region", vec![text("East"), text("West"), text("east"), blank(), text("North")]),
                    column("Year", vec![n(2024.0), n(2023.0), n(2024.0), n(2022.0), n(10.0)]),
                    column("Flag", vec![l(true), l(false), l(true), l(false), blank()]),
                    column("Amount", vec![n(10.0), n(20.0), n(5.0), n(7.0), blank()]),
                    column("Note", vec![text("a"), blank(), text("b"), text("a"), text("c")]),
                ],
            }],
            measures: [
                ("S", "SUM(Table1[Amount])"),
                ("C", "COUNT(Table1[Amount])"),
                ("CN", "COUNT(Table1[Note])"),
                ("A", "AVERAGE(Table1[Amount])"),
                ("D", "DISTINCTCOUNT(Table1[Note])"),
                ("D2", "DISTINCTCOUNT(Table1[Region])"),
                ("R", "COUNTROWS(Table1)"),
                ("MINN", "MIN(Table1[Note])"),
                ("MAXA", "MAX(Table1[Amount])"),
                ("CA", "COUNTA(Table1[Note])"),
                ("CB", "COUNTBLANK(Table1[Amount])"),
            ]
            .map(|(name, expression)| CubeMeasure {
                name: name.to_owned(),
                expression: expression.to_owned(),
            })
            .to_vec(),
        };
        let caption = |text: &str| model.caption(&model.member(text).unwrap());
        assert_eq!(caption("[Table1].[Year].&[2024]"), "2024");
        assert_eq!(caption("[Table1].[Flag].&[TRUE]"), "True");
        assert_eq!(caption("[Table1].[Region].&[EAST]"), "East");
        assert_eq!(caption("[Table1].[Region].[All]"), "All");
        assert_eq!(caption("[Table1].[Region]"), "All");
        let shown = |expression: &str| -> Vec<String> {
            model
                .set(expression)
                .unwrap()
                .iter()
                .map(|tuple| model.caption(&tuple[0]))
                .collect()
        };
        assert_eq!(shown("[Table1].[Year].Children"), ["10", "2022", "2023", "2024"]);
        assert_eq!(shown("[Table1].[Flag].Children"), ["", "False", "True"]);
        assert_eq!(shown("[Table1].[Region].Children"), ["", "East", "North", "West"]);
        assert_eq!(shown("[Table1].[Amount].Children"), ["", "5", "7", "10", "20"]);
        let east = "[Table1].[Region].&[East]";
        for (measure, all, in_east) in [
            ("[Measures].[S]", n(42.0), n(15.0)),
            ("[Measures].[C]", n(4.0), n(2.0)),
            ("[Measures].[CN]", n(4.0), n(2.0)),
            ("[Measures].[A]", n(10.5), n(7.5)),
            ("[Measures].[D]", n(4.0), n(2.0)),
            ("[Measures].[D2]", n(4.0), n(1.0)),
            ("[Measures].[R]", n(5.0), n(2.0)),
            ("[Measures].[MINN]", text("a"), text("a")),
            ("[Measures].[MAXA]", n(20.0), n(10.0)),
            ("[Measures].[CA]", n(4.0), n(2.0)),
            ("[Measures].[CB]", n(1.0), empty_text()),
        ] {
            assert_eq!(value(&model, &[measure]), Ok(all), "{measure}");
            assert_eq!(value(&model, &[east, measure]), Ok(in_east), "{measure} East");
        }
        assert_eq!(
            value(&model, &["[Table1].[Year].&[2024]", "[Measures].[S]"]),
            Ok(n(15.0))
        );
        assert_eq!(
            value(&model, &["[Table1].[Flag].&[True]", "[Measures].[S]"]),
            Ok(n(15.0))
        );
        assert_eq!(model.member("2024"), Err(NA));
    }

    const EAST: &str = "[Table1].[Region].&[East]";
    const NORTH: &str = "[Table1].[Region].&[North]";
    const A: &str = "[Table1].[Product].&[A]";

    /// Excel 16.0.20527's readings of this table (the p40 measurement).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn values_are_excels() {
        let model = model();
        let n = CalcValue::number;
        for (members, expected) in [
            (vec![EAST, "[Measures].[TotalAmount]"], n(15.2)),
            (vec!["East", "[Measures].[TotalAmount]"], n(15.2)),
            (vec!["[Table1].[Region].[East]", "[Measures].[TotalAmount]"], n(15.2)),
            (vec![EAST, A, "[Measures].[TotalAmount]"], n(10.2)),
            (vec![EAST], n(1.0)),
            (vec!["[Table1].[Region].[All]"], n(1.0)),
            (vec!["[Measures].[CountAmount]"], n(6.0)),
            (vec![EAST, "[Measures].[CountAmount]"], n(3.0)),
            (vec!["[Measures].[AvgAmount]"], n(7.05)),
            (vec![EAST, "[Measures].[AvgAmount]"], n(5.066666666666666)),
            (vec!["[Measures].[MinAmount]"], n(0.1)),
            (vec![EAST, "[Measures].[MinAmount]"], n(0.2)),
            (vec!["[Measures].[MaxAmount]"], n(20.0)),
            (vec![EAST, "[Measures].[MaxAmount]"], n(10.0)),
            (vec!["[Measures].[DistinctRegion]"], n(3.0)),
            (vec![EAST, "[Measures].[DistinctRegion]"], n(1.0)),
            (vec!["[Measures].[RowCount]"], n(6.0)),
            (vec![EAST, "[Measures].[RowCount]"], n(3.0)),
            (vec![NORTH, A, "[Measures].[TotalAmount]"], empty_text()),
            (vec![NORTH, A, "[Measures].[DistinctRegion]"], empty_text()),
            (vec![NORTH, A, "[Measures].[RowCount]"], empty_text()),
        ] {
            assert_eq!(value(&model, &members), Ok(expected), "{members:?}");
        }
        assert_eq!(model.value_over(&[]), Ok(empty_text()));
        assert_eq!(
            value(&model, &["[Measures].[TotalAmount]", EAST, "[Table1].[Region].&[West]"]),
            Err(VALUE)
        );
        assert_eq!(value(&model, &[EAST, EAST, "[Measures].[TotalAmount]"]), Err(VALUE));
        assert_eq!(model.member("[NoSuch].[X]"), Err(NA));
        assert_eq!(model.member("[Table1].[Region].&[Nope]"), Err(NA));
        assert_eq!(model.member("[Measures].[Sum of Amount]"), Err(NA));
        assert_eq!(model.member(""), Err(NA));
    }

    /// Excel's sums of the p41 and p43 columns: six places are exact, seven add in row order.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sums_are_scaled_up_to_six_places() {
        assert_eq!(scaled_sum(&[10.0, 20.0, 5.0, 7.0, 0.1, 0.2]), Some(42.3));
        assert_eq!(scaled_sum(&[-0.000001, 0.000002]), Some(0.000001));
        assert_eq!(scaled_sum(&[1.5, 0.1234567]), None);
        assert_eq!(scaled_sum(&[1e10]), None);
    }

    /// Exercise the inferred whole-column representation for a filtered slice.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_8wp_19__slice_uses_whole_column_numeric_representation() {
        let mut model = model();
        model.tables[0].columns[0].cells = vec![text("East"), text("East"), text("West")];
        model.tables[0].columns[1].cells = vec![text("A"), text("A"), text("B")];
        model.tables[0].columns[2].cells = vec![CalcValue::number(0.1), CalcValue::number(0.2), CalcValue::number(0.0000001)];
        let members = ["[Measures].[TotalAmount]", "[Table1].[Region].&[East]"];
        assert_eq!(value(&model, &members), Ok(CalcValue::number(0.1 + 0.2)));
        model.tables[0].columns[2].cells[2] = CalcValue::number(0.000001);
        assert_eq!(value(&model, &members), Ok(CalcValue::number(0.3)));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sets_are_excels() {
        let model = model();
        let captions = |tuples: Vec<Vec<CubeMember>>| -> Vec<String> {
            tuples.iter().map(|tuple| model.caption(&tuple[0])).collect()
        };
        let children = "[Table1].[Region].Children";
        let by = Some("[Measures].[TotalAmount]");
        for (order, expected) in [
            (0, ["East", "North", "West"]),
            (1, ["North", "East", "West"]),
            (2, ["West", "East", "North"]),
            (3, ["East", "North", "West"]),
            (4, ["West", "North", "East"]),
            (5, ["East", "North", "West"]),
            (6, ["West", "North", "East"]),
        ] {
            let set = model.ordered(model.set(children).unwrap(), order, by).unwrap();
            assert_eq!(captions(set), expected, "sort {order}");
        }
        assert_eq!(model.ordered(model.set(children).unwrap(), 7, by), Err(NA));
        assert_eq!(model.ordered(model.set(children).unwrap(), -1, by), Err(NA));
        assert_eq!(model.ordered(model.set(children).unwrap(), 1, None), Err(VALUE));
        for (expression, count) in [
            ("[Table1].[Region].Members", 4),
            ("[Table1].[Region].[Region].Members", 3),
            ("[Table1].[Region].AllMembers", 4),
            ("{[Table1].[Region].&[East]}", 1),
            ("[Table1].[Product].Children", 3),
            ("TOPCOUNT([Table1].[Region].Children,1,[Measures].[TotalAmount])", 1),
            ("[Measures].Members", 0),
            ("FILTER([Table1].[Region].Children,[Measures].[TotalAmount]>10)", 2),
            ("CROSSJOIN([Table1].[Region].Children,[Table1].[Product].Children)", 5),
            ("[Table1].[Region].[All].Children", 3),
        ] {
            assert_eq!(model.set(expression).map(|set| set.len()), Ok(count), "{expression}");
        }
        assert_eq!(
            model.set("HEAD([Table1].[Region].Children,1)"),
            Err(CubeFailure::NotModelled)
        );
        let members = model.set("{[Table1].[Region].&[East],[Table1].[Region].&[West]}").unwrap();
        let measure = vec![vec![model.member("[Measures].[TotalAmount]").unwrap()]];
        assert_eq!(
            model.value_over(&[members, measure.clone()]),
            Ok(CalcValue::number(35.3))
        );
        let all = model.set("[Table1].[Region].Members").unwrap();
        assert_eq!(model.value_over(&[all, measure]), Ok(CalcValue::number(42.3)));
    }
}
