//! Structured references to tables (L3, dc-m3u): `Sales[Qty]`, `Sales[[#Headers],[Qty]]`,
//! `Sales[[#This Row],[Qty]]` (`Sales[@Qty]`), `Sales[[Qty]:[Price]]`, `Sales[#All]`, `Sales[]`,
//! and, in a formula inside a table, the unqualified `[@Qty]` and `[Qty]`.
//!
//! The text of a reference is split into its table name and its selectors by the bracket grammar
//! ported from OxFml 15daac5 (`crates/oxfml_core/src/binding/mod.rs`:
//! `parse_structured_reference_text`, `split_top_level_segments`, `split_top_level_once`,
//! `strip_structured_brackets_raw`, `matching_outer_bracket_end`,
//! `unescape_structured_reference_text`, `parse_section_qualifier`), where `'` escapes the next
//! `#`, `[`, `]`, `@` or `'`. OxFml's string-keyed table descriptors and bind records are left
//! behind: a table here is a [`TableInfo`] the catalogue gives, and a reference binds to a typed
//! [`ReferenceOperand`] with absolute coordinates.
//!
//! Each selection was measured on Excel 16.0.20430 through COM on 2026-10-23, over a table
//! `Sales` at A1:C5 of its sheet (a header row, three data rows, a totals row) whose columns are
//! Name, Qty and `Unit [Price]`:
//!
//! | formula | value |
//! |---|---|
//! | `=SUM(Sales[Qty])` | 9 (the data rows) |
//! | `=Sales[[#Headers],[Qty]]` | Qty |
//! | `=Sales[[#Totals],[Qty]]` | 9 |
//! | `=ROWS(Sales[#All])`, `=ROWS(Sales[#Data])`, `=ROWS(Sales)`, `=ROWS(Sales[])` | 5, 3, 3, 3 |
//! | `=ROWS(Sales[[#Headers],[#Data]])`, `=ROWS(Sales[[#Data],[#Totals]])` | 4, 4 |
//! | `=SUM(Sales[[Qty]:[Unit '[Price']]])`, `=SUM(Sales[Unit '[Price']])` | 32, 23 |
//! | `=COUNTA(Sales[[#Headers],[Name]:[Qty]])`, `=SUM(sales[qty])` | 2, 9 |
//! | `=Sales[@Qty]` in the table's second data row, beside the table | 3 |
//! | `=Sales[@Qty]` in a row outside the table | #VALUE! |
//!
//! A file stores `Sales[@Qty]` as `Sales[[#This Row],[Qty]]` and `Sales` as `Sales[]`; a legacy
//! formula's `=Sales[Qty]` is stored as `Sales[[#This Row],[Qty]]` too (Excel rewrites it).
//! `#This Row` selects the data rows and takes the formula's row from them by implicit
//! intersection, which is #VALUE! in a row outside them.

use super::{AxisRef, BindError, CellRef, FormulaAnchor, ReferenceOperand, ReferenceShape};
use super::{ReferenceCatalog, SheetTarget};
use calc_func::value::{GridArea, SheetId};

/// A table (ListObject) as a [`ReferenceCatalog`] gives it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TableInfo {
    /// The name structured references use (the table part's `displayName`).
    pub name: String,
    pub sheet: SheetId,
    /// The whole table: its header row, data rows and totals row.
    pub area: GridArea,
    /// 1 when the table shows a header row, else 0.
    pub header_rows: u32,
    /// 1 when the table shows a totals row, else 0.
    pub totals_rows: u32,
    /// The columns' names, left to right.
    pub columns: Vec<String>,
}

/// What a catalogue knows of a table.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TableLookup {
    Found(TableInfo),
    /// The workbook has no such table.
    Unknown,
    /// The catalogue does not carry tables.
    NotModelled,
}

impl TableInfo {
    /// The data rows: below the header row and above the totals row. `None` when there are none.
    fn data_rows(&self) -> Option<(u32, u32)> {
        let top = self.area.first().row() + self.header_rows;
        let bottom = self.area.last().row().checked_sub(self.totals_rows)?;
        (top <= bottom).then_some((top, bottom))
    }

    /// The reference the table's name alone names (`Sales`, stored `Sales[]`): its data rows.
    pub(crate) fn data_body(&self) -> ReferenceOperand {
        match self.data_rows() {
            Some((top, bottom)) => operand(
                self.sheet,
                top,
                self.area.first().column(),
                bottom,
                self.area.last().column(),
            ),
            None => deleted(self.sheet),
        }
    }

    fn column_index(&self, name: &str) -> Option<u32> {
        column_index(&self.columns, name).map(|index| index as u32)
    }
}

fn column_index(columns: &[String], name: &str) -> Option<usize> {
    columns
        .iter()
        .position(|column| same_name(column, name))
        .or_else(|| {
            columns
                .iter()
                .position(|column| same_name(column, name.trim()))
        })
}

/// Excel matches table and column names without case, as workbook names: under the full Unicode
/// case fold (dc-cpr, calc-value's `workbook_names_equal`).
pub(crate) fn same_name(a: &str, b: &str) -> bool {
    a == b || calc_func::value::workbook_names_equal(a, b)
}

/// The parts of a table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    All,
    Data,
    Headers,
    Totals,
    ThisRow,
}

/// A structured reference's text split into its parts.
#[derive(Debug, PartialEq, Eq)]
struct Parsed {
    table: Option<String>,
    sections: Vec<Section>,
    /// No column, one, or the two ends of a column range, unescaped.
    columns: Vec<String>,
}

/// The table an unqualified selector can name at this authored formula's locus.
#[derive(Clone, Copy)]
pub enum TableColumnContext<'a> {
    Cell(Option<&'a str>),
    /// A name can be evaluated from different tables; its unqualified selectors are ambiguous.
    Name,
}

/// Escape a column name inside a structured selector, using the binder's grammar.
pub fn quote_table_column(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if "#[]@'".contains(ch) {
            out.push('\'');
        }
        out.push(ch);
    }
    out
}

fn spelling(parsed: &Parsed) -> String {
    let table = parsed.table.as_deref().unwrap_or("");
    if parsed.sections.is_empty() && parsed.columns.len() == 1 {
        return format!("{table}[{}]", quote_table_column(&parsed.columns[0]));
    }
    let mut parts = parsed
        .sections
        .iter()
        .map(|section| {
            format!(
                "[{}]",
                match section {
                    Section::All => "#All",
                    Section::Data => "#Data",
                    Section::Headers => "#Headers",
                    Section::Totals => "#Totals",
                    Section::ThisRow => "#This Row",
                }
            )
        })
        .collect::<Vec<_>>();
    if !parsed.columns.is_empty() {
        parts.push(
            parsed
                .columns
                .iter()
                .map(|c| format!("[{}]", quote_table_column(c)))
                .collect::<Vec<_>>()
                .join(":"),
        );
    }
    format!("{table}[{}]", parts.join(","))
}

/// Rename actual selectors, preserving literals, unrelated tables and all surrounding text.
/// Unsupported qualifiers and context-dependent names refuse before the caller mutates a model.
pub fn rename_table_column(
    source: &str,
    table: &str,
    columns: &[String],
    old: &str,
    new: &str,
    context: TableColumnContext<'_>,
    sheet_exists: &dyn Fn(&str) -> bool,
) -> Result<String, &'static str> {
    use crate::syntax::{SyntaxKind, Token};
    let tree = crate::parse_formula(source);
    let lambda = tree.tokens().iter().any(|t| {
        t.text().eq_ignore_ascii_case("LAMBDA")
            || crate::eval::without_prefix(t.text(), calc_catalogue::FUTURE_FUNCTION_PREFIX)
                .is_some_and(|name| name.eq_ignore_ascii_case("LAMBDA"))
    });
    let old_index = columns
        .iter()
        .position(|c| same_name(c, old))
        .ok_or("The original table column is unavailable")?;
    let mut next_columns = columns.to_vec();
    next_columns[old_index] = new.into();
    let rewrite = |token: &Token| -> Result<Option<String>, &'static str> {
        let Some(mut parsed) = parse(token.text()) else {
            return Ok(None);
        };
        let replacements = parsed
            .columns
            .iter()
            .map(|c| {
                let index = column_index(columns, c)?;
                if index == old_index {
                    Some(new.to_owned())
                } else if column_index(&next_columns, c) != Some(index) {
                    Some(columns[index].clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if replacements.iter().all(Option::is_none) {
            return Ok(None);
        }
        if let Some(name) = &parsed.table {
            if !same_name(name, table) {
                return Ok(None);
            }
        } else {
            match context {
                TableColumnContext::Name => {
                    return Err("A named formula has a context-dependent table selector");
                }
                TableColumnContext::Cell(Some(name)) if same_name(name, table) => {
                    if lambda {
                        return Err(
                            "Unqualified table selectors inside LAMBDA need a binding-aware rename",
                        );
                    }
                }
                _ => return Ok(None),
            }
        }
        for (column, replacement) in parsed.columns.iter_mut().zip(replacements) {
            if let Some(replacement) = replacement {
                *column = replacement;
            }
        }
        Ok(Some(spelling(&parsed)))
    };
    let mut changes = Vec::new();
    let mut pending = vec![tree.root()];
    while let Some(node) = pending.pop() {
        match node.kind() {
            SyntaxKind::QualifiedReferenceExpr | SyntaxKind::SheetSpan3DReferenceExpr => {
                if let Some((sheet, token)) = super::bind::qualified_name(node) {
                    if let Some(text) = rewrite(token)? {
                        if !sheet_exists(&sheet) {
                            return Err("A table selector has an unavailable sheet qualifier");
                        }
                        changes.push((token.span(), text));
                    }
                } else if node
                    .tokens()
                    .iter()
                    .any(|t| rewrite(t).is_err() || rewrite(t).ok().flatten().is_some())
                {
                    return Err("A table selector has an unsupported workbook or sheet qualifier");
                }
            }
            SyntaxKind::UnsupportedExpr | SyntaxKind::ArrayLiteralExpr => {
                for token in node.tokens() {
                    if rewrite(token)?.is_some() {
                        return Err("A table selector occurs in unsupported formula syntax");
                    }
                }
            }
            SyntaxKind::IdentifierExpr => {
                for token in node.tokens() {
                    if let Some(text) = rewrite(token)? {
                        changes.push((token.span(), text));
                    }
                }
            }
            _ => pending.extend(node.child_nodes()),
        }
    }
    if changes.is_empty() {
        return Ok(source.into());
    }
    if !tree.diagnostics().is_empty() {
        return Err("A table selector occurs in an incomplete formula");
    }
    changes.sort_by_key(|(span, _)| span.start);
    let mut units = source.encode_utf16().collect::<Vec<_>>();
    for (span, text) in changes.into_iter().rev() {
        units.splice(span.start..span.end(), text.encode_utf16());
    }
    String::from_utf16(&units).map_err(|_| "The renamed formula is not valid Unicode")
}

/// A structured reference bound: its operand, and whether it takes the formula's row by implicit
/// intersection (`#This Row`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bound {
    pub(crate) operand: ReferenceOperand,
    pub(crate) this_row: bool,
}

/// Binds `text`, an identifier holding a bracket group (`Sales[Qty]`) or a bracketed one
/// (`[@Qty]`), for a formula anchored at `anchor`. `None` when the text is no structured
/// reference: an unqualified bracket group in a formula outside every table (a LAMBDA's optional
/// parameter `[y]` is one), or text the grammar does not read.
pub(crate) fn bind_structured(
    text: &str,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
) -> Option<Result<Bound, BindError>> {
    let parsed = parse(text)?;
    let table = match &parsed.table {
        Some(name) => match catalog.table(name) {
            TableLookup::Found(table) => table,
            TableLookup::Unknown => {
                return Some(Err(BindError::UnknownTable { name: name.clone() }));
            }
            TableLookup::NotModelled => return Some(Err(BindError::TablesNotModelled)),
        },
        None => match catalog.table_containing(anchor.cell()) {
            TableLookup::Found(table) => table,
            TableLookup::Unknown => return None,
            TableLookup::NotModelled => return Some(Err(BindError::TablesNotModelled)),
        },
    };
    Some(resolve(&parsed, &table))
}

fn resolve(parsed: &Parsed, table: &TableInfo) -> Result<Bound, BindError> {
    let refused = || BindError::StructuredReference;
    let (left, right) = match parsed.columns.as_slice() {
        [] => (table.area.first().column(), table.area.last().column()),
        [one] => {
            let column = table.area.first().column()
                + table
                    .column_index(one)
                    .ok_or_else(|| BindError::UnknownColumn { name: one.clone() })?;
            (column, column)
        }
        [first, last] => {
            let index = |name: &String| {
                table
                    .column_index(name)
                    .ok_or_else(|| BindError::UnknownColumn { name: name.clone() })
            };
            let (a, b) = (index(first)?, index(last)?);
            let start = table.area.first().column();
            (start + a.min(b), start + a.max(b))
        }
        _ => return Err(refused()),
    };
    let this_row = parsed.sections.contains(&Section::ThisRow);
    if this_row && parsed.sections.len() > 1 {
        return Err(refused());
    }
    let mut sections = parsed.sections.clone();
    if sections.is_empty() || this_row {
        sections = vec![Section::Data];
    }
    sections.sort_by_key(|section| match section {
        Section::Headers | Section::All => 0,
        Section::Data | Section::ThisRow => 1,
        Section::Totals => 2,
    });
    sections.dedup();
    let header = table.area.first().row();
    let totals = table.area.last().row();
    let data = table.data_rows();
    // The rows each section covers, `None` for a section the table does not show (#REF!).
    let rows = |section: Section| -> Option<(u32, u32)> {
        match section {
            Section::All => Some((header, totals)),
            Section::Data | Section::ThisRow => data,
            Section::Headers => (table.header_rows > 0).then_some((header, header)),
            Section::Totals => (table.totals_rows > 0).then_some((totals, totals)),
        }
    };
    let (top, bottom) = match sections.as_slice() {
        [one] => match rows(*one) {
            Some(rows) => rows,
            None => {
                return Ok(Bound {
                    operand: deleted(table.sheet),
                    this_row: false,
                });
            }
        },
        // Two sections that meet: the header row and the data, the data and the totals row.
        [Section::Headers, Section::Data] | [Section::Data, Section::Totals] => {
            match (rows(sections[0]), rows(sections[1])) {
                (Some((top, _)), Some((_, bottom))) => (top, bottom),
                _ => {
                    return Ok(Bound {
                        operand: deleted(table.sheet),
                        this_row: false,
                    });
                }
            }
        }
        // `#All` with another section is `#All`.
        [Section::All, ..] => (header, totals),
        _ => return Err(refused()),
    };
    Ok(Bound {
        operand: operand(table.sheet, top, left, bottom, right),
        this_row,
    })
}

/// The operand of the rectangle from (`top`, `left`) to (`bottom`, `right`) on `sheet`, every
/// coordinate absolute.
fn operand(sheet: SheetId, top: u32, left: u32, bottom: u32, right: u32) -> ReferenceOperand {
    let cell = |row, column| CellRef {
        row: AxisRef::Absolute(row),
        column: AxisRef::Absolute(column),
    };
    let shape = if top == bottom && left == right {
        ReferenceShape::Cell(cell(top, left))
    } else {
        ReferenceShape::Area {
            first: cell(top, left),
            last: cell(bottom, right),
        }
    };
    ReferenceOperand {
        sheet: SheetTarget::Sheet(sheet),
        shape,
    }
}

/// `#REF!` on `sheet`: a section the table does not show.
fn deleted(sheet: SheetId) -> ReferenceOperand {
    ReferenceOperand {
        sheet: SheetTarget::Sheet(sheet),
        shape: ReferenceShape::Deleted,
    }
}

/// Splits a structured reference's text (ported from OxFml's `parse_structured_reference_text`).
fn parse(text: &str) -> Option<Parsed> {
    let (table, selector) = if text.starts_with('[') {
        (None, text)
    } else {
        let bracket = text.find('[')?;
        (Some(text[..bracket].to_owned()), &text[bracket..])
    };
    if table.as_deref().is_some_and(str::is_empty) {
        return None;
    }
    if !selector.ends_with(']') || matching_outer_bracket_end(selector)? != selector.len() - 1 {
        return None;
    }
    let inner = &selector[1..selector.len() - 1];
    let mut sections = Vec::new();
    let mut columns = Vec::new();
    if inner.trim().is_empty() {
        // `Sales[]`: the data rows of every column.
    } else if inner.starts_with('[') {
        for segment in split_top_level(inner, ',') {
            let raw = strip_brackets_raw(segment)?;
            if !contains_escape(raw)
                && let Some(section) = section(raw)
            {
                sections.push(section);
                continue;
            }
            if !columns.is_empty() {
                return None;
            }
            columns = column_segment(segment)?;
        }
    } else if let Some(rest) = inner.strip_prefix('@') {
        sections.push(Section::ThisRow);
        let rest = rest.trim();
        if !rest.is_empty() {
            columns = column_segment(rest)?;
        }
    } else if let Some(section) = section(inner) {
        sections.push(section);
    } else {
        columns.push(unescape(inner));
    }
    Some(Parsed {
        table,
        sections,
        columns,
    })
}

pub(super) fn is_selector(text: &str) -> bool {
    parse(text).is_some()
}

/// A column segment: `[Qty]`, `Qty`, or `[Qty]:[Price]`.
fn column_segment(segment: &str) -> Option<Vec<String>> {
    if let Some((left, right)) = split_top_level_once(segment, ':') {
        return Some(vec![strip_brackets(left)?, strip_brackets(right)?]);
    }
    Some(vec![strip_brackets(segment)?])
}

fn split_top_level(text: &str, delimiter: char) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut skip_escaped = false;
    for (index, ch) in text.char_indices() {
        if skip_escaped {
            skip_escaped = false;
            continue;
        }
        if ch == '\'' && escape_at(text, index) {
            skip_escaped = true;
            continue;
        }
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            _ if ch == delimiter && depth == 0 => {
                result.push(text[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    result.push(text[start..].trim());
    result
}

fn split_top_level_once(text: &str, delimiter: char) -> Option<(&str, &str)> {
    let mut depth = 0usize;
    let mut skip_escaped = false;
    for (index, ch) in text.char_indices() {
        if skip_escaped {
            skip_escaped = false;
            continue;
        }
        if ch == '\'' && escape_at(text, index) {
            skip_escaped = true;
            continue;
        }
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            _ if ch == delimiter && depth == 0 => {
                return Some((text[..index].trim(), text[index + ch.len_utf8()..].trim()));
            }
            _ => {}
        }
    }
    None
}

fn strip_brackets(text: &str) -> Option<String> {
    strip_brackets_raw(text).map(unescape)
}

fn strip_brackets_raw(text: &str) -> Option<&str> {
    if text.starts_with('[') && text.ends_with(']') && text.len() >= 2 {
        Some(&text[1..text.len() - 1])
    } else if !text.is_empty() {
        Some(text)
    } else {
        None
    }
}

fn matching_outer_bracket_end(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut skip_escaped = false;
    for (index, ch) in text.char_indices() {
        if skip_escaped {
            skip_escaped = false;
            continue;
        }
        if ch == '\'' && escape_at(text, index) {
            skip_escaped = true;
            continue;
        }
        match ch {
            '[' => depth += 1,
            ']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn unescape(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\'' && matches!(chars.peek(), Some('#' | '[' | ']' | '@' | '\'')) {
            output.push(chars.next().expect("the escaped character"));
        } else {
            output.push(ch);
        }
    }
    output
}

fn escape_at(text: &str, index: usize) -> bool {
    text[index..]
        .chars()
        .nth(1)
        .is_some_and(|ch| matches!(ch, '#' | '[' | ']' | '@' | '\''))
}

fn contains_escape(text: &str) -> bool {
    text.char_indices()
        .any(|(index, ch)| ch == '\'' && escape_at(text, index))
}

fn section(text: &str) -> Option<Section> {
    match text.to_ascii_uppercase().replace(' ', "").as_str() {
        "#ALL" => Some(Section::All),
        "#DATA" => Some(Section::Data),
        "#HEADERS" => Some(Section::Headers),
        "#TOTALS" | "#TOTALROW" => Some(Section::Totals),
        "#THISROW" => Some(Section::ThisRow),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// dc-cpr: table and column names compare as workbook names do (Excel 16.0, 2026-09-15:
    /// `UMSÄTZE[GRÖSSE]`, `UMSÄTZE[GRÖßE]` and `umsätze[größe]` name `Umsätze[Größe]`, and
    /// `[ΜΈΓΕΘΟΣ]` and `[μέγεθοσ]` name `[Μέγεθος]`).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[allow(non_snake_case)]
    fn dc_cpr__table_and_column_names_compare_as_workbook_names() {
        for (held, written) in [
            ("Umsätze", "UMSÄTZE"),
            ("Umsätze", "umsätze"),
            ("Größe", "GRÖSSE"),
            ("Größe", "GRÖßE"),
            ("Größe", "größe"),
            ("Μέγεθος", "ΜΈΓΕΘΟΣ"),
            ("Μέγεθος", "μέγεθοσ"),
        ] {
            assert!(same_name(held, written), "{held} / {written}");
        }
        assert!(!same_name("Μέγεθος", "ΜΕΓΕΘΟΣ"), "accents stay significant");
        assert!(!same_name("Isik", "\u{0130}sik"), "İ is not I");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_bracket_grammar_splits_tables_sections_and_columns() {
        let p = |text: &str| parse(text).unwrap();
        assert_eq!(p("Sales[Qty]").columns, ["Qty"]);
        assert_eq!(p("Sales[Qty]").table.as_deref(), Some("Sales"));
        assert_eq!(p("Sales[]").columns, Vec::<String>::new());
        assert_eq!(p("Sales[#All]").sections, [Section::All]);
        assert_eq!(p("Sales[#This Row]").sections, [Section::ThisRow]);
        let both = p("Sales[[#Headers],[#Data]]");
        assert_eq!(both.sections, [Section::Headers, Section::Data]);
        let row = p("Sales[[#This Row],[Unit '[Price']]]");
        assert_eq!(row.sections, [Section::ThisRow]);
        assert_eq!(row.columns, ["Unit [Price]"]);
        assert_eq!(p("Sales[@[Unit '[Price']]]").columns, ["Unit [Price]"]);
        assert_eq!(p("Sales[@Qty]").columns, ["Qty"]);
        assert_eq!(p("[@Qty]").table, None);
        assert_eq!(
            p("Sales[[Qty]:[Unit '[Price']]]").columns,
            ["Qty", "Unit [Price]"]
        );
        assert_eq!(p("Sales[[#Headers],[Name]:[Qty]]").columns, ["Name", "Qty"]);
        assert_eq!(p("Sales[It''s]").columns, ["It's"]);
        assert!(parse("Sales[Qty").is_none());
        assert!(parse("Sales[Qty]x").is_none());
        assert!(parse("[Qty]]").is_none());
    }
}
