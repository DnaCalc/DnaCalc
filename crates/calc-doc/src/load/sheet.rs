//! Worksheet cells and selected view, geometry and rule metadata, read in document order.
//! Other elements remain in the package for the save path to preserve.

use super::error::{CellError, LoadError};
use super::metadata::{CellMeta, Metadata, ValueMeta};
use super::strings::{decode, string_item};
use super::{
    Events, FORMULA_MAX_UTF16_CODE_UNITS, FormulaRecord, MAIN_NAMESPACE, SharedFormula, address,
    read_xml, shift,
};
use crate::opc::Package;
use crate::xml::{XmlElement, XmlEvent};
use calc_doc_model::value::{
    CellKey, ExcelText, GridArea, MAX_COLUMNS, MAX_ROWS, SheetId, WorksheetErrorCode,
};
use calc_doc_model::{
    ArrayFormula, ArrayKind, CachedValue, CellInput, CellText, FileCached, FiniteNumber,
    FormulaBlock, FormulaBlockId, FormulaForm, FormulaTemplateSource, FormulaText,
    SemanticTemplateKey, SourcePart, TemplatePlacement, UnloadedCell,
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// What every sheet's cells are read against: the workbook's shared strings, the number of its
/// cell formats, and its cell and value metadata.
pub(super) struct Context<'a> {
    pub(super) shared_strings: &'a [ExcelText],
    pub(super) cell_formats: u32,
    pub(super) metadata: &'a Metadata,
}

/// One sheet's cells as read: the inputs for the model and the records kept beside it.
pub(super) struct SheetLoad {
    pub(super) conditional_sources: Vec<calc_doc_model::ConditionalRuleSource>,
    pub(super) review: super::SheetReview,
    pub(super) protected: bool,
    pub(super) merged_areas: Vec<GridArea>,
    pub(super) part: Arc<SourcePart>,
    pub(super) inputs: Vec<(CellKey, CellInput)>,
    pub(super) styles: BTreeMap<(u32, u32), u32>,
    pub(super) row_styles: BTreeMap<u32, u32>,
    pub(super) column_styles: Vec<(u32, u32, u32)>,
    pub(super) formulas: BTreeMap<(u32, u32), FormulaRecord>,
    /// The sheet's array formulas.
    pub(super) arrays: Vec<ArrayFormula>,
    /// The sheet's what-if data tables (L3, dc-m3u).
    pub(super) data_tables: Vec<calc_doc_model::DataTable>,
    /// The cells the model holds no input for, with why.
    pub(super) unloaded: Vec<(CellKey, UnloadedCell)>,
    /// What the file cached for cells the model holds no input for: the cells an array formula
    /// fills beside its anchor, and unloaded cells.
    pub(super) value_caches: BTreeMap<(u32, u32), FileCached>,
    /// The rows `<row hidden="1">` marks hidden, in order.
    pub(super) hidden_rows: Vec<u32>,
    /// The sheet's `<autoFilter ref>` range, (top, left, bottom, right).
    pub(super) auto_filter: Option<(u32, u32, u32, u32)>,
    pub(super) filter: Option<calc_doc_model::AutoFilter>,
    pub(super) filter_manual: Option<std::collections::BTreeSet<u32>>,
    /// The rows' and columns' sizes and hidden flags (dc-09v).
    pub(super) geometry: super::geometry::SheetGeometry,
    /// The first window's top left and active cells (dc-uar).
    pub(super) view: super::geometry::SheetView,
    /// The sheet's conditional formats (dc-6yw).
    pub(super) conditional_formats: Vec<super::conditional::ConditionalFormat>,
    /// The sheet's data validations (dc-6yw).
    pub(super) validations: Vec<super::validation::Validation>,
}

/// An area some of whose cells are still to be read, whose cells are not inputs: what an array
/// formula fills beside its anchor, or a data table's results.
struct Filled {
    /// (top, left, bottom, right)
    area: (u32, u32, u32, u32),
    data_table: bool,
}

/// The spelling of each worksheet error in a cell's `<v>`, matched exactly.
pub(crate) const ERRORS: [(&str, WorksheetErrorCode); 14] = [
    ("#NULL!", WorksheetErrorCode::Null),
    ("#DIV/0!", WorksheetErrorCode::Div0),
    ("#VALUE!", WorksheetErrorCode::Value),
    ("#REF!", WorksheetErrorCode::Ref),
    ("#NAME?", WorksheetErrorCode::Name),
    ("#NUM!", WorksheetErrorCode::Num),
    ("#N/A", WorksheetErrorCode::NA),
    ("#GETTING_DATA", WorksheetErrorCode::GettingData),
    ("#SPILL!", WorksheetErrorCode::Spill),
    ("#CALC!", WorksheetErrorCode::Calc),
    ("#FIELD!", WorksheetErrorCode::Field),
    ("#BLOCKED!", WorksheetErrorCode::Blocked),
    ("#CONNECT!", WorksheetErrorCode::Connect),
    ("#BUSY!", WorksheetErrorCode::Busy),
];

/// `text` without the XML whitespace around it, as an XML Schema number or boolean reads it.
pub(crate) fn collapsed(text: &str) -> &str {
    text.trim_matches(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
}

/// A shared formula group's master, once read.
struct Group {
    master: (u32, u32),
    /// The master's formula text with its `_xHHHH_` escapes decoded, which its members' texts are
    /// moved from; `None` when it holds an unpaired surrogate, which no member's text is moved
    /// from.
    text: Option<String>,
    /// (top, left, bottom, right)
    area: (u32, u32, u32, u32),
}

/// Validated once by the existing shared-reference occurrence mover.
#[derive(Debug)]
pub(crate) struct SharedSource {
    anchor: CellKey,
    source: ExcelText,
    moves: Option<shift::Moves>,
}
impl SharedSource {
    pub(crate) fn from_source(anchor: CellKey, source: ExcelText) -> Option<Self> {
        let spelling = String::from_utf16(source.utf16_code_units()).ok()?;
        let moves = shift::Moves::new(&spelling).ok()?;
        Some(Self {
            anchor,
            source,
            moves: Some(moves),
        })
    }
    pub(crate) fn try_render(&self, at: CellKey) -> Option<ExcelText> {
        if at.sheet() != self.anchor.sheet() {
            return None;
        }
        if at == self.anchor {
            return Some(self.source.clone());
        }
        let text = self
            .moves
            .as_ref()?
            .apply(
                i64::from(at.row()) - i64::from(self.anchor.row()),
                i64::from(at.column()) - i64::from(self.anchor.column()),
            )
            .ok()?;
        Some(ExcelText::from_utf16_code_units(
            text.encode_utf16().collect(),
        ))
    }
}
impl FormulaTemplateSource for SharedSource {
    fn anchor(&self) -> CellKey {
        self.anchor
    }
    fn source(&self) -> &ExcelText {
        &self.source
    }
    fn render(&self, at: CellKey) -> ExcelText {
        self.try_render(at)
            .expect("every admitted member was validated against the grid")
    }
}

/// A member of a shared formula group, waiting for every master of the sheet to be read.
struct Member {
    row: u32,
    column: u32,
    index: u32,
    cache: Option<CachedValue>,
}

/// A cell's `<f>` element.
struct Formula {
    kind: Option<String>,
    index: Option<String>,
    area: Option<String>,
    /// A data table's `dt2D`, `dtr`, `r1` and `r2`.
    data_table: [Option<String>; 4],
    text: String,
}

struct Sheet<'a> {
    context: &'a Context<'a>,
    /// An Excel 4.0 macro sheet: its formula cells are unloaded (dc-xuw).
    macro_sheet: bool,
    id: SheetId,
    part: Arc<SourcePart>,
    load: SheetLoad,
    groups: BTreeMap<u32, Group>,
    members: Vec<Member>,
    /// The areas whose cells, beside the anchor, are filled rather than authored.
    filled: Vec<Filled>,
    /// The formula records and cell styles in cell order, as read; [`SheetLoad::formulas`] and
    /// [`SheetLoad::styles`] are built from them in one pass at the end (dc-dxm), rather than by
    /// inserting each cell into a map.
    formulas: Vec<((u32, u32), FormulaRecord)>,
    styles: Vec<((u32, u32), u32)>,
}

impl Sheet<'_> {
    /// Whether the cell at `row` and `column` lies in a filled area other than at its anchor,
    /// and whether that area is a data table's.
    fn filled_at(&mut self, row: u32, column: u32) -> Option<bool> {
        self.filled.retain(|filled| filled.area.2 >= row);
        self.filled
            .iter()
            .find(|filled| {
                let (top, left, bottom, right) = filled.area;
                (top..=bottom).contains(&row)
                    && (left..=right).contains(&column)
                    && (row, column) != (top, left)
            })
            .map(|filled| filled.data_table)
    }

    /// A cell's cached value as its `<v>` and `vm` give it: a `vm` naming a rich error gives
    /// that error (the `<v>` holds only Excel's `#VALUE!` fallback); one naming any other rich
    /// value, or one that does not resolve, gives the inner `Err` with why, since the `<v>` is
    /// not the cell's value.
    fn cached(
        &self,
        kind: Option<&str>,
        text: &str,
        vm: Option<u32>,
    ) -> Result<Result<CachedValue, UnloadedCell>, CellError> {
        let Some(vm) = vm else {
            return self.value(kind, text).map(Ok);
        };
        Ok(match self.context.metadata.value(vm) {
            ValueMeta::Error(code) => Ok(CachedValue::Error(code)),
            ValueMeta::UnknownErrorType { error_type } => Err(UnloadedCell::RichValue {
                structure: format!("_error with errorType {error_type}"),
            }),
            ValueMeta::OtherRichValue { structure } => Err(UnloadedCell::RichValue { structure }),
            ValueMeta::Unresolved { reason } => {
                Err(UnloadedCell::UnresolvedValueMetadata { reason })
            }
        })
    }
    fn key(&self, row: u32, column: u32) -> CellKey {
        CellKey::new(self.id, row, column).expect("a cell the loader checked lies inside the grid")
    }

    fn cell_error(&self, row: u32, column: u32, error: CellError) -> LoadError {
        LoadError::Cell {
            part: self.part.name().to_owned(),
            row,
            column,
            error,
        }
    }

    fn text(&self, units: Vec<u16>) -> Result<CellText, CellError> {
        let count = units.len();
        CellText::new(ExcelText::from_utf16_code_units(units))
            .ok_or(CellError::TextTooLong { units: count })
    }

    /// The value of `<v>` read as type `kind` (`n` when absent).
    fn value(&self, kind: Option<&str>, text: &str) -> Result<CachedValue, CellError> {
        match kind {
            None | Some("n") => {
                let number = collapsed(text);
                number
                    .parse::<f64>()
                    .ok()
                    .and_then(FiniteNumber::new)
                    .map(CachedValue::Number)
                    .ok_or_else(|| CellError::InvalidNumber {
                        text: text.to_owned(),
                    })
            }
            Some("s") => {
                let count = self.context.shared_strings.len();
                let index = collapsed(text)
                    .parse::<usize>()
                    .ok()
                    .filter(|&index| index < count)
                    .ok_or_else(|| CellError::SharedStringIndex {
                        text: text.to_owned(),
                        count,
                    })?;
                let units = self.context.shared_strings[index]
                    .utf16_code_units()
                    .to_vec();
                self.text(units).map(CachedValue::Text)
            }
            Some("str") => self.text(decode(text)).map(CachedValue::Text),
            Some("b") => match collapsed(text) {
                "0" | "false" => Ok(CachedValue::Logical(false)),
                "1" | "true" => Ok(CachedValue::Logical(true)),
                _ => Err(CellError::InvalidBoolean {
                    text: text.to_owned(),
                }),
            },
            Some("e") => ERRORS
                .iter()
                .find(|(spelling, _)| *spelling == collapsed(text))
                .map(|(_, code)| CachedValue::Error(*code))
                .ok_or_else(|| CellError::UnknownError {
                    text: text.to_owned(),
                }),
            Some(other) => Err(CellError::UnsupportedType {
                kind: other.to_owned(),
            }),
        }
    }

    /// The input of a formula whose `<f>` element holds `text`, checked against the length limit.
    /// ECMA-376 derives ST_Formula from ST_Xstring, so its `_xHHHH_` escapes are decoded first.
    fn formula_input(text: &str) -> Result<CellInput, CellError> {
        Self::formula_units(decode(text))
    }

    /// The input of a formula whose text is `units`, checked against the length limit.
    fn formula_units(units: Vec<u16>) -> Result<CellInput, CellError> {
        if units.len() > FORMULA_MAX_UTF16_CODE_UNITS {
            return Err(CellError::FormulaTooLong { units: units.len() });
        }
        FormulaText::with_form(ExcelText::from_utf16_code_units(units), FormulaForm::Legacy)
            .map(CellInput::Formula)
            .map_err(|_| CellError::EmptyFormula)
    }

    fn record(
        &mut self,
        row: u32,
        column: u32,
        shared: Option<SharedFormula>,
        cache: Option<CachedValue>,
    ) {
        let cache = cache.map(|value| FileCached::new(value, Arc::clone(&self.part)));
        self.formulas
            .push(((row, column), FormulaRecord { shared, cache }));
    }

    fn shared(&self, index: u32, group: &Group) -> SharedFormula {
        let (top, left, bottom, right) = group.area;
        SharedFormula {
            index,
            master: self.key(group.master.0, group.master.1),
            area: GridArea::new(self.key(top, left), self.key(bottom, right))
                .expect("two corners on one sheet"),
        }
    }

    /// Reads one `<c>` whose start tag is `element`, after the cell at `previous` of row `row`.
    /// Returns the cell's column.
    fn cell(
        &mut self,
        events: &mut Events<'_>,
        element: &XmlElement,
        row: u32,
        previous: u32,
    ) -> Result<u32, LoadError> {
        let (cell_row, column) = match element.attribute("r") {
            Some(text) => address::cell(text).ok_or_else(|| LoadError::InvalidCellAddress {
                part: events.part().to_owned(),
                text: text.to_owned(),
            })?,
            None if previous < MAX_COLUMNS => (row, previous + 1),
            None => {
                return Err(LoadError::InvalidCellAddress {
                    part: events.part().to_owned(),
                    text: String::new(),
                });
            }
        };
        let part = events.part();
        let fail = |error| LoadError::Cell {
            part: part.to_owned(),
            row: cell_row,
            column,
            error,
        };
        if cell_row != row || column <= previous {
            return Err(fail(CellError::OutOfOrder));
        }
        let index = |name: &str| match element.attribute(name) {
            None => Ok(None),
            Some(text) => collapsed(text).parse::<u32>().map(Some).map_err(|_| {
                fail(CellError::Malformed {
                    detail: "a cell whose cm or vm is not a count",
                })
            }),
        };
        let (cm, vm) = (index("cm")?, index("vm")?);
        let style = match element.attribute("s") {
            None => 0,
            Some(text) => collapsed(text)
                .parse::<u32>()
                .ok()
                .filter(|&style| style == 0 || style < self.context.cell_formats)
                .ok_or_else(|| {
                    fail(CellError::StyleIndex {
                        text: text.to_owned(),
                        cell_formats: self.context.cell_formats,
                    })
                })?,
        };
        let kind = element.attribute("t").map(str::to_owned);
        if let Some(kind) = kind
            .as_deref()
            .filter(|kind| !matches!(*kind, "n" | "s" | "str" | "b" | "e" | "inlineStr"))
        {
            return Err(fail(CellError::UnsupportedType {
                kind: kind.to_owned(),
            }));
        }
        let (mut formula, mut value, mut inline) = (None, None, None);
        loop {
            let child = match events.next()? {
                Some(XmlEvent::Start(child)) => child,
                Some(XmlEvent::Text(text)) if text.trim().is_empty() => continue,
                Some(XmlEvent::Text(_)) => {
                    return Err(fail(CellError::Malformed {
                        detail: "a cell holds text outside its elements",
                    }));
                }
                Some(XmlEvent::End(_)) => break,
                None => return Err(events.invalid("the part ends inside a cell")),
            };
            let twice = |detail| fail(CellError::Malformed { detail });
            if child.name.is(MAIN_NAMESPACE, "f") {
                let text = events.text()?;
                let attribute = |local: &str| child.attribute(local).map(str::to_owned);
                let previous = formula.replace(Formula {
                    kind: attribute("t"),
                    index: attribute("si"),
                    area: attribute("ref"),
                    data_table: [
                        attribute("dt2D"),
                        attribute("dtr"),
                        attribute("r1"),
                        attribute("r2"),
                    ],
                    text,
                });
                if previous.is_some() {
                    return Err(twice("a cell with two formulas"));
                }
            } else if child.name.is(MAIN_NAMESPACE, "v") {
                if value.replace(events.text()?).is_some() {
                    return Err(twice("a cell with two values"));
                }
            } else if child.name.is(MAIN_NAMESPACE, "is") {
                if inline.replace(string_item(events)?).is_some() {
                    return Err(twice("a cell with two inline strings"));
                }
            } else if child.name.is(MAIN_NAMESPACE, "extLst") {
                events.skip()?;
            } else {
                return Err(twice("an element a cell does not hold"));
            }
        }
        if element.attribute("s").is_some() {
            self.styles.push(((row, column), style));
        }
        let kind = kind.as_deref();
        if self.macro_sheet && formula.is_some() {
            // A macro sheet's formula is not read: the cell is unloaded, with what the file
            // cached for it.
            if let Some(text) = &value
                && !(matches!(kind, None | Some("n")) && collapsed(text).is_empty())
                && let Ok(cached) = self.cached(kind, text, vm).map_err(fail)?
            {
                let cache = FileCached::new(cached, Arc::clone(&self.part));
                self.load.value_caches.insert((row, column), cache);
            }
            let key = self.key(row, column);
            self.load.unloaded.push((key, UnloadedCell::MacroSheet));
            return Ok(column);
        }
        if let Some(data_table) = self.filled_at(row, column) {
            if formula.is_some() {
                return Err(fail(CellError::Malformed {
                    detail: "a formula inside the area an array formula or data table fills",
                }));
            }
            let cache = match (value, inline) {
                (_, Some(units)) => Some(Ok(CachedValue::Text(self.text(units).map_err(fail)?))),
                (Some(text), None)
                    if matches!(kind, None | Some("n")) && collapsed(&text).is_empty() =>
                {
                    None
                }
                (Some(text), None) => Some(self.cached(kind, &text, vm).map_err(fail)?),
                (None, None) => None,
            };
            let key = self.key(row, column);
            match cache {
                Some(Ok(value)) => {
                    let cache = FileCached::new(value, Arc::clone(&self.part));
                    self.load.value_caches.insert((row, column), cache);
                    if data_table {
                        self.load.unloaded.push((key, UnloadedCell::DataTable));
                    }
                }
                Some(Err(why)) => self.load.unloaded.push((key, why)),
                None if data_table => self.load.unloaded.push((key, UnloadedCell::DataTable)),
                None => {}
            }
            return Ok(column);
        }
        if let Some(formula) = formula {
            if matches!(kind, Some("inlineStr")) || inline.is_some() {
                return Err(fail(CellError::Malformed {
                    detail: "a formula cell with an inline string",
                }));
            }
            let cache = match &value {
                // An empty `<v>` of a number-typed formula cell (`<v/>`, as some writers emit
                // for a formula they did not calculate) stores no result. An empty one of type
                // `str` is the empty text, and of any other type it is refused as before.
                Some(text) if matches!(kind, None | Some("n")) && collapsed(text).is_empty() => {
                    None
                }
                // A rich value other than an error is not a result the model holds, so the
                // formula then has no cache.
                Some(text) => self.cached(kind, text, vm).map_err(fail)?.ok(),
                None => None,
            };
            self.formula(row, column, formula, cache, cm)
                .map_err(fail)?;
            return Ok(column);
        }
        let input = match (kind, value, inline) {
            (Some("inlineStr"), None, Some(units)) => {
                Some(CellInput::Text(self.text(units).map_err(fail)?))
            }
            (Some("inlineStr"), _, _) => {
                return Err(fail(CellError::Malformed {
                    detail: "an inline string cell without exactly its is element",
                }));
            }
            (_, _, Some(_)) => {
                return Err(fail(CellError::Malformed {
                    detail: "an is element in a cell that is not an inline string",
                }));
            }
            (Some("str"), _, None) => {
                return Err(fail(CellError::Malformed {
                    detail: "a formula string cell without a formula",
                }));
            }
            (None | Some("n"), None, None) => None,
            (_, None, None) => {
                return Err(fail(CellError::Malformed {
                    detail: "a shared string, logical or error cell without a value",
                }));
            }
            (kind, Some(text), None) => match self.cached(kind, &text, vm).map_err(fail)? {
                Ok(CachedValue::Number(number)) => Some(CellInput::Number(number)),
                Ok(CachedValue::Text(text)) => Some(CellInput::Text(text)),
                Ok(CachedValue::Logical(logical)) => Some(CellInput::Logical(logical)),
                Ok(CachedValue::Error(code)) => Some(CellInput::Error(code)),
                Err(why) => {
                    // A rich value the model does not represent: the cell is unloaded, and its
                    // fallback `<v>` is kept as what the file cached for it.
                    if let Ok(fallback) = self.value(kind, &text) {
                        let cache = FileCached::new(fallback, Arc::clone(&self.part));
                        self.load.value_caches.insert((row, column), cache);
                    }
                    let key = self.key(row, column);
                    self.load.unloaded.push((key, why));
                    None
                }
            },
        };
        if let Some(input) = input {
            let key = self.key(row, column);
            self.load.inputs.push((key, input));
        }
        Ok(column)
    }

    /// Records a formula cell: its input now, or, for a shared formula's member, once the
    /// sheet's masters are known.
    /// A data table's definition from its `<f t="dataTable">` attributes over `area`; `None`
    /// when they do not name its input cells (the table's cells are then not calculated).
    fn data_table(
        &self,
        formula: &Formula,
        area: (u32, u32, u32, u32),
    ) -> Option<calc_doc_model::DataTable> {
        let flag = |value: &Option<String>| matches!(value.as_deref(), Some("1" | "true"));
        let [two, row, r1, r2] = &formula.data_table;
        let cell = |text: &Option<String>| {
            let (row, column) = address::cell(text.as_deref()?.trim())?;
            Some(self.key(row, column))
        };
        let inputs = if flag(two) {
            calc_doc_model::DataTableInputs::Both {
                row: cell(r1)?,
                column: cell(r2)?,
            }
        } else if flag(row) {
            calc_doc_model::DataTableInputs::Row(cell(r1)?)
        } else {
            calc_doc_model::DataTableInputs::Column(cell(r1)?)
        };
        let (top, left, bottom, right) = area;
        let area = GridArea::new(self.key(top, left), self.key(bottom, right)).ok()?;
        calc_doc_model::DataTable::new(area, inputs)
    }

    fn formula(
        &mut self,
        row: u32,
        column: u32,
        formula: Formula,
        cache: Option<CachedValue>,
        cm: Option<u32>,
    ) -> Result<(), CellError> {
        match formula.kind.as_deref() {
            Some(kind @ ("array" | "dataTable")) => {
                let detail = |detail| CellError::ArrayFormula { detail };
                let area = formula
                    .area
                    .as_deref()
                    .ok_or(detail("an array or data table formula without a ref"))?;
                let area = address::area(area).ok_or(detail("a ref that is not an area"))?;
                if (area.0, area.1) != (row, column) {
                    return Err(detail("a ref that does not start at the formula's cell"));
                }
                let (top, left, bottom, right) = area;
                let key = self.key(row, column);
                if kind == "dataTable" {
                    // L3 (dc-m3u): the table's definition, which the engine calculates; its
                    // cells stay unloaded inputs, whose bytes a save keeps.
                    if let Some(table) = self.data_table(&formula, area) {
                        self.load.data_tables.push(table);
                    }
                    self.load.unloaded.push((key, UnloadedCell::DataTable));
                    if let Some(cache) = cache {
                        let cache = FileCached::new(cache, Arc::clone(&self.part));
                        self.load.value_caches.insert((row, column), cache);
                    }
                    self.filled.push(Filled {
                        area,
                        data_table: true,
                    });
                    return Ok(());
                }
                let kind = match cm.map(|cm| self.context.metadata.cell(cm)) {
                    None | Some(CellMeta::DynamicArray { dynamic: false, .. }) => ArrayKind::Legacy,
                    Some(CellMeta::DynamicArray {
                        dynamic: true,
                        collapsed,
                    }) => ArrayKind::Dynamic { collapsed },
                    Some(CellMeta::Other { .. } | CellMeta::Unresolved { .. }) => {
                        return Err(CellError::UnsupportedMetadata);
                    }
                };
                let CellInput::Formula(text) = Self::formula_input(&formula.text)? else {
                    unreachable!()
                };
                let input = CellInput::Formula(
                    FormulaText::with_form(text.text().clone(), FormulaForm::Dynamic)
                        .expect("a validated nonempty array source"),
                );
                self.load.inputs.push((key, input));
                self.load.arrays.push(ArrayFormula::new(
                    GridArea::new(key, self.key(bottom, right)).expect("two corners on one sheet"),
                    kind,
                ));
                if (top, left) != (bottom, right) {
                    self.filled.push(Filled {
                        area,
                        data_table: false,
                    });
                }
                self.record(row, column, None, cache);
                Ok(())
            }
            None | Some("normal") => {
                let input = Self::formula_input(&formula.text)?;
                let key = self.key(row, column);
                self.load.inputs.push((key, input));
                self.record(row, column, None, cache);
                Ok(())
            }
            Some("shared") => {
                let detail = |detail| CellError::SharedFormula { detail };
                let index: u32 = formula
                    .index
                    .as_deref()
                    .and_then(|text| collapsed(text).parse().ok())
                    .ok_or(detail("a shared formula without an si that is a number"))?;
                if formula.area.is_none() {
                    // A dependent may carry text; its si/master define the formula.
                    self.members.push(Member {
                        row,
                        column,
                        index,
                        cache,
                    });
                    return Ok(());
                }
                let area = formula
                    .area
                    .as_deref()
                    .ok_or(detail("a shared formula's master without a ref"))?;
                let area = address::area(area).ok_or(detail("a ref that is not an area"))?;
                let (top, left, bottom, right) = area;
                if !(top..=bottom).contains(&row) || !(left..=right).contains(&column) {
                    return Err(detail("a master outside its ref"));
                }
                if self.groups.contains_key(&index) {
                    return Err(detail("two masters for one si"));
                }
                let units = decode(&formula.text);
                let text = String::from_utf16(&units).ok();
                let input = Self::formula_units(units)?;
                let key = self.key(row, column);
                self.load.inputs.push((key, input));
                let group = Group {
                    master: (row, column),
                    text,
                    area,
                };
                let shared = self.shared(index, &group);
                self.groups.insert(index, group);
                self.record(row, column, Some(shared), cache);
                Ok(())
            }
            Some(kind) => Err(CellError::UnsupportedFormula {
                kind: kind.to_owned(),
            }),
        }
    }

    /// Validates member spellings, then retains one source and explicit actual membership.
    fn finish_members(&mut self) -> Result<(), LoadError> {
        let mut moves: BTreeMap<u32, Result<shift::Moves, &'static str>> = BTreeMap::new();
        let members = std::mem::take(&mut self.members);
        let mut positions: BTreeMap<u32, Vec<CellKey>> = BTreeMap::new();
        for member in &members {
            let (row, column) = (member.row, member.column);
            let error = |error| self.cell_error(row, column, error);
            let detail = |detail| error(CellError::SharedFormula { detail });
            let group = self
                .groups
                .get(&member.index)
                .ok_or_else(|| detail("a member whose si has no master"))?;
            let (top, left, bottom, right) = group.area;
            if !(top..=bottom).contains(&row) || !(left..=right).contains(&column) {
                return Err(detail("a member outside its group's ref"));
            }
            let group_moves = moves
                .entry(member.index)
                .or_insert_with(|| match &group.text {
                    Some(text) => shift::Moves::new(text).map_err(|refused| refused.0),
                    None => Err("a member of a group whose text holds an unpaired surrogate"),
                })
                .as_ref()
                .map_err(|refused| detail(refused))?;
            let text = group_moves
                .apply(
                    i64::from(row) - i64::from(group.master.0),
                    i64::from(column) - i64::from(group.master.1),
                )
                .map_err(|shift::PastGrid| error(CellError::SharedFormulaPastGrid))?;
            // Preserve all existing load refusals, including post-shift formula length. The
            // temporary validation spelling is discarded; members retain no expanded source.
            Self::formula_units(text.encode_utf16().collect()).map_err(error)?;
            positions
                .entry(member.index)
                .or_default()
                .push(self.key(row, column));
        }
        let masters: BTreeMap<CellKey, u32> = self
            .groups
            .iter()
            .map(|(&index, group)| (self.key(group.master.0, group.master.1), index))
            .collect();
        let sources: BTreeMap<u32, ExcelText> = self
            .load
            .inputs
            .iter()
            .filter_map(|(at, input)| match (masters.get(at), input) {
                (Some(&index), CellInput::Formula(text)) => Some((index, text.text().clone())),
                _ => None,
            })
            .collect();
        let mut blocks = BTreeMap::new();
        for (&index, group) in &self.groups {
            let master = self.key(group.master.0, group.master.1);
            let source = sources[&index].clone();
            let key = group
                .text
                .as_ref()
                .and_then(|text| {
                    calc_fml::reference::template_key(
                        text,
                        calc_fml::reference::FormulaAnchor::new(master),
                    )
                })
                .map(|normalized_ir| SemanticTemplateKey {
                    normalized_ir: normalized_ir.into(),
                    placement: TemplatePlacement::Cell,
                    evaluation: FormulaForm::Legacy,
                });
            let source = Arc::new(SharedSource {
                anchor: master,
                source,
                moves: moves.remove(&index).and_then(Result::ok),
            });
            let area = self.shared(index, group).area;
            let cells = positions
                .remove(&index)
                .unwrap_or_default()
                .into_iter()
                .chain([master]);
            let block = FormulaBlock::new(
                FormulaBlockId {
                    sheet: master.sheet(),
                    local: index,
                },
                area,
                cells,
                key,
                FormulaForm::Legacy,
                source,
            )
            .expect("validated shared group and actual membership");
            blocks.insert(index, block);
        }
        for (at, input) in &mut self.load.inputs {
            if let Some(index) = masters.get(at) {
                *input = CellInput::Formula(
                    FormulaText::member(Arc::clone(&blocks[index]), *at)
                        .expect("the master is a member"),
                );
            }
        }
        for member in members {
            let at = self.key(member.row, member.column);
            let block = &blocks[&member.index];
            self.load.inputs.push((
                at,
                CellInput::Formula(
                    FormulaText::member(Arc::clone(block), at).expect("validated member"),
                ),
            ));
            let shared = self.shared(member.index, &self.groups[&member.index]);
            self.record(member.row, member.column, Some(shared), member.cache);
        }
        Ok(())
    }

    fn sheet_data(&mut self, events: &mut Events<'_>) -> Result<(), LoadError> {
        let mut previous_row = 0u32;
        loop {
            let row = match events.next()? {
                Some(XmlEvent::Start(row)) if row.name.is(MAIN_NAMESPACE, "row") => row,
                Some(XmlEvent::Start(_)) => {
                    return Err(events.invalid("an element other than row in sheetData"));
                }
                Some(XmlEvent::Text(text)) if text.trim().is_empty() => continue,
                Some(XmlEvent::Text(_)) => return Err(events.invalid("sheetData holds text")),
                Some(XmlEvent::End(_)) => return Ok(()),
                None => return Err(events.invalid("the part ends inside sheetData")),
            };
            let number = match row.attribute("r") {
                Some(text) => address::row_number(collapsed(text)).ok_or_else(|| {
                    LoadError::InvalidRowNumber {
                        part: events.part().to_owned(),
                        text: text.to_owned(),
                    }
                })?,
                None if previous_row < MAX_ROWS => previous_row + 1,
                None => {
                    return Err(LoadError::InvalidRowNumber {
                        part: events.part().to_owned(),
                        text: String::new(),
                    });
                }
            };
            if number <= previous_row {
                return Err(events.invalid("a row that is not after the row before it"));
            }
            previous_row = number;
            if super::manifest::is_true(row.attribute("hidden")) {
                self.load.hidden_rows.push(number);
            }
            self.load.geometry.read_row(number, &row);
            if matches!(row.attribute("customFormat"), Some("1" | "true"))
                && let Some(style) = row.attribute("s").and_then(|s| collapsed(s).parse().ok())
            {
                self.load.row_styles.insert(number, style);
            }
            let mut previous_column = 0u32;
            loop {
                match events.next()? {
                    Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "c") => {
                        previous_column = self.cell(events, &element, number, previous_column)?;
                    }
                    Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "extLst") => {
                        events.skip()?;
                    }
                    Some(XmlEvent::Start(_)) => {
                        return Err(events.invalid("an element other than c in a row"));
                    }
                    Some(XmlEvent::Text(text)) if text.trim().is_empty() => {}
                    Some(XmlEvent::Text(_)) => return Err(events.invalid("a row holds text")),
                    Some(XmlEvent::End(_)) => break,
                    None => return Err(events.invalid("the part ends inside a row")),
                }
            }
        }
    }
}

/// Reads the cells of the worksheet part `part` for the sheet `id`.
/// The namespace of an Excel 4.0 macro sheet's document element (`xm:macrosheet`).
const MACRO_SHEET_NAMESPACE: &str = "http://schemas.microsoft.com/office/excel/2006/main";

pub(super) fn load_sheet(
    package: &Package,
    context: &Context<'_>,
    id: SheetId,
    part: &str,
    macro_sheet: bool,
) -> Result<SheetLoad, LoadError> {
    let found = package.part(part).ok_or_else(|| LoadError::MissingPart {
        part: part.to_owned(),
    })?;
    let source = Arc::new(SourcePart::new(
        found.name().to_owned(),
        found.crc32(),
        found.size(),
    ));
    let xml = read_xml(package, part)?;
    let mut events = Events::new(&xml);
    if macro_sheet {
        // A macro sheet's document element is in Excel's own namespace; what it holds is in
        // SpreadsheetML's, as a worksheet's.
        events.root(
            MACRO_SHEET_NAMESPACE,
            "macrosheet",
            "the document element is not an Excel macro sheet",
        )?;
    } else {
        events.root(
            MAIN_NAMESPACE,
            "worksheet",
            "the document element is not a SpreadsheetML worksheet",
        )?;
    }
    let mut sheet = Sheet {
        context,
        macro_sheet,
        id,
        part: Arc::clone(&source),
        load: SheetLoad {
            conditional_sources: Vec::new(),
            review: super::review::read(package, part, id),
            protected: false,
            merged_areas: Vec::new(),
            part: source,
            inputs: Vec::new(),
            styles: BTreeMap::new(),
            row_styles: BTreeMap::new(),
            column_styles: Vec::new(),
            formulas: BTreeMap::new(),
            arrays: Vec::new(),
            data_tables: Vec::new(),
            unloaded: Vec::new(),
            value_caches: BTreeMap::new(),
            hidden_rows: Vec::new(),
            auto_filter: None,
            filter: None,
            filter_manual: None,
            geometry: super::geometry::SheetGeometry::default(),
            view: super::geometry::SheetView::default(),
            conditional_formats: Vec::new(),
            validations: Vec::new(),
        },
        groups: BTreeMap::new(),
        members: Vec::new(),
        filled: Vec::new(),
        formulas: Vec::new(),
        styles: Vec::new(),
    };
    let mut sheet_data = false;
    let mut bars = BTreeMap::new();
    loop {
        match events.next()? {
            Some(XmlEvent::Start(element))
                if element.name.is(MAIN_NAMESPACE, "sheetProtection") =>
            {
                sheet.load.protected = matches!(element.attribute("sheet"), Some("1" | "true"));
                events.skip()?;
            }
            Some(XmlEvent::Start(element))
                if element.name.is(MAIN_NAMESPACE, "conditionalFormatting") =>
            {
                let block = super::conditional::read_block(&mut events, &element)?;
                sheet.load.conditional_formats.push(block);
            }
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "extLst") => {
                let unread = super::conditional::read_extensions(
                    &mut events,
                    &mut bars,
                    &mut sheet.load.validations,
                )?;
                if unread > 0 {
                    sheet.load.review.unsupported.push(format!("{unread} extension-only conditional rules are preserved but cannot be displayed or edited (dc-971p.8)"));
                }
            }
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "hyperlinks") => {
                sheet.load.review.links = super::review::links(&mut events, package, part, id)?;
            }
            Some(XmlEvent::Start(element))
                if element.name.is(MAIN_NAMESPACE, "dataValidations") =>
            {
                let read = super::validation::read_validations(&mut events)?;
                sheet.load.validations.extend(read);
            }
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "sheetData") => {
                if sheet_data {
                    return Err(events.invalid("a worksheet with two sheetData elements"));
                }
                sheet_data = true;
                sheet.sheet_data(&mut events)?;
            }
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "autoFilter") => {
                sheet.load.auto_filter = element.attribute("ref").and_then(address::area);
                (sheet.load.filter,sheet.load.filter_manual)=super::filter::read(&mut events,&element,id)?;
            }
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "sheetViews") => {
                sheet.load.view = read_view(&mut events)?;
            }
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "sheetFormatPr") => {
                sheet.load.geometry.read_format(&element);
                events.skip()?;
            }
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "mergeCells") => {
                loop {
                    match events.next()? {
                        Some(XmlEvent::Start(child))
                            if child.name.is(MAIN_NAMESPACE, "mergeCell") =>
                        {
                            if let Some((top, left, bottom, right)) =
                                child.attribute("ref").and_then(address::area)
                            {
                                let area = GridArea::new(
                                    CellKey::new(id, top, left).unwrap(),
                                    CellKey::new(id, bottom, right).unwrap(),
                                )
                                .unwrap();
                                sheet.load.merged_areas.push(area);
                            }
                            events.skip()?;
                        }
                        Some(XmlEvent::Start(_)) => events.skip()?,
                        Some(XmlEvent::Text(_)) => {}
                        Some(XmlEvent::End(_)) => break,
                        None => return Err(events.invalid("the part ends inside mergeCells")),
                    }
                }
            }
            Some(XmlEvent::Start(element)) if element.name.is(MAIN_NAMESPACE, "cols") => loop {
                match events.next()? {
                    Some(XmlEvent::Start(col)) if col.name.is(MAIN_NAMESPACE, "col") => {
                        sheet.load.geometry.read_column(&col);
                        if let (Some(first), Some(last), Some(style)) = (
                            col.attribute("min")
                                .and_then(|s| collapsed(s).parse::<u32>().ok()),
                            col.attribute("max")
                                .and_then(|s| collapsed(s).parse::<u32>().ok()),
                            col.attribute("style")
                                .and_then(|s| collapsed(s).parse().ok()),
                        ) && first >= 1
                            && first <= last
                            && last <= MAX_COLUMNS
                        {
                            sheet.load.column_styles.push((first, last, style));
                        }
                        events.skip()?;
                    }
                    Some(XmlEvent::Start(_)) => events.skip()?,
                    Some(XmlEvent::Text(_)) => {}
                    Some(XmlEvent::End(_)) => break,
                    None => return Err(events.invalid("the part ends inside cols")),
                }
            },
            Some(XmlEvent::Start(_)) => events.skip()?,
            Some(XmlEvent::Text(text)) if text.trim().is_empty() => {}
            Some(XmlEvent::Text(_)) => return Err(events.invalid("worksheet holds text")),
            Some(XmlEvent::End(_)) => break,
            None => return Err(events.invalid("the part ends inside worksheet")),
        }
    }
    events.end()?;
    if !sheet_data {
        return Err(events.invalid("a worksheet without sheetData"));
    }
    sheet.finish_members()?;
    // The cells were read in order, each once, so each map is built from a sorted list.
    sheet.load.formulas = std::mem::take(&mut sheet.formulas).into_iter().collect();
    sheet.load.styles = std::mem::take(&mut sheet.styles).into_iter().collect();
    let mut load = sheet.load;
    super::outline::read(&xml, &mut load.geometry)?;
    if load.geometry.outline_unsupported {load.review.unsupported.push("Imported outline metadata cannot be edited here (dc-971p.39); its source is retained".into());}
    let unread = bars
        .keys()
        .filter(|id| {
            !load
                .conditional_formats
                .iter()
                .flat_map(|b| &b.rules)
                .any(|r| r.extension_id.as_ref() == Some(*id))
        })
        .count();
    if unread > 0 {
        load.review.unsupported.push(format!("{unread} extension-only data bars are preserved but cannot be displayed or edited (dc-971p.8)"));
    }
    load.conditional_formats =
        super::conditional::with_extensions(std::mem::take(&mut load.conditional_formats), &bars);
    if !load.conditional_formats.is_empty() {
        load.conditional_sources =
            crate::save::conditional::sources(&xml, id).map_err(|_| LoadError::InvalidPart {
                part: part.to_owned(),
                detail: "conditional source could not be retained",
            })?;
    }
    Ok(load)
}

/// The top left and active cells of the first `<sheetView>` of the `<sheetViews>` element whose
/// start tag was just read (dc-uar). The active cell is the `activeCell` of the `<selection>`
/// of the pane `<pane activePane>` names (the top left pane when it names none); a value that is
/// not a cell reference is passed over.
pub(crate) fn view_from_xml(xml:&crate::xml::XmlPart)->Result<super::geometry::SheetView,LoadError> {
    let mut events=Events::new(xml);
    while let Some(event)=events.next()? {if let XmlEvent::Start(e)=event {if e.name.is(MAIN_NAMESPACE,"sheetViews") {return read_view(&mut events);}}}
    Ok(Default::default())
}

fn read_view(events: &mut Events<'_>) -> Result<super::geometry::SheetView, LoadError> {
    let mut view = super::geometry::SheetView::default();
    let mut first = true;
    loop {
        match events.next()? {
            Some(XmlEvent::Start(element))
                if first && element.name.is(MAIN_NAMESPACE, "sheetView") =>
            {
                first = false;
                view.zoom=element.attribute("zoomScale").and_then(|s|s.parse::<u16>().ok()).filter(|n|(10..=400).contains(n));
                view.top_left = element.attribute("topLeftCell").and_then(address::cell);
                let mut active_pane = "topLeft".to_owned();
                let mut selections: Vec<(String, Option<(u32, u32)>)> = Vec::new();
                loop {
                    match events.next()? {
                        Some(XmlEvent::Start(child)) if child.name.is(MAIN_NAMESPACE, "pane") => {
                            view.pane_top_left =
                                child.attribute("topLeftCell").and_then(address::cell);
                            let position = |name| {
                                child
                                    .attribute(name)
                                    .unwrap_or("0")
                                    .trim()
                                    .parse::<f64>()
                                    .ok()
                                    .filter(|n| n.is_finite() && *n >= 0.)
                            };
                            let count = |name| pane_count(child.attribute(name).unwrap_or("0"));
                            let frozen =
                                matches!(child.attribute("state"), Some("frozen" | "frozenSplit"));
                            view.frozen = if frozen {
                                count("ySplit")
                                    .zip(count("xSplit"))
                                    .and_then(|(r, c)| calc_doc_model::FrozenPanes::new(r, c))
                            } else {
                                None
                            };
                            view.split_window = (!frozen || view.frozen.is_none())
                                && (count("xSplit") != Some(0) || count("ySplit") != Some(0));
                            if !frozen {
                                view.split = position("xSplit")
                                    .zip(position("ySplit"))
                                    .filter(|(x, y)| *x > 0. || *y > 0.)
                                    .map(|(x, y)| (x.to_bits(), y.to_bits()));
                            }
                            if let Some(pane) = child.attribute("activePane") {
                                active_pane = pane.trim().to_owned();
                            }
                            events.skip()?;
                        }
                        Some(XmlEvent::Start(child))
                            if child.name.is(MAIN_NAMESPACE, "selection") =>
                        {
                            let pane = child
                                .attribute("pane")
                                .unwrap_or("topLeft")
                                .trim()
                                .to_owned();
                            selections.push((
                                pane,
                                child.attribute("activeCell").and_then(address::cell),
                            ));
                            events.skip()?;
                        }
                        Some(XmlEvent::Start(_)) => events.skip()?,
                        Some(XmlEvent::Text(_)) => {}
                        Some(XmlEvent::End(_)) => break,
                        None => return Err(events.invalid("the part ends inside sheetView")),
                    }
                }
                view.active_pane = match active_pane.as_str() {
                    "topRight" => 1,
                    "bottomLeft" => 2,
                    "bottomRight" => 3,
                    _ => 0,
                };
                view.active_cell = selections
                    .iter()
                    .find(|(pane, _)| *pane == active_pane)
                    .and_then(|(_, cell)| *cell);
            }
            Some(XmlEvent::Start(_)) => events.skip()?,
            Some(XmlEvent::Text(_)) => {}
            Some(XmlEvent::End(_)) => return Ok(view),
            None => return Err(events.invalid("the part ends inside sheetViews")),
        }
    }
}

// Frozen split attributes have XML Schema double syntax, while their values count bands.
fn pane_count(text: &str) -> Option<u32> {
    collapsed(text)
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.0 && n.fract() == 0.0 && *n <= u32::MAX as f64)
        .map(|n| n as u32)
}

#[cfg(test)]
mod pane_tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn frozen_counts_accept_integral_double_syntax_only() {
        for text in ["1", "1.0", "1E0", " 1.0 "] {
            assert_eq!(pane_count(text), Some(1), "{text}");
        }
        for text in ["-1", "0.5", "NaN", "INF", "inf", "4294967296", ""] {
            assert_eq!(pane_count(text), None, "{text}");
        }
        assert_eq!(pane_count("-0.0"), Some(0));
    }
}

#[cfg(test)]
mod origin_tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn first_window_origins_and_fractional_twip_splits_remain_distinct() {
        let xml=crate::xml::XmlPart::new("views.xml".into(),br#"<sheetViews xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetView workbookViewId="0" topLeftCell="C5"><pane xSplit="2400.5" ySplit="1800" topLeftCell="H100" activePane="bottomRight" state="split"/><selection pane="topLeft" activeCell="C5"/><selection pane="bottomRight" activeCell="J101"/></sheetView></sheetViews>"#.to_vec(),Default::default());
        let mut events = Events::new(&xml);
        events.root(MAIN_NAMESPACE, "sheetViews", "views").unwrap();
        let view = read_view(&mut events).unwrap();
        assert_eq!(view.top_left, Some((5, 3)));
        assert_eq!(view.pane_top_left, Some((100, 8)));
        assert_eq!(view.active_cell, Some((101, 10)));
        assert_eq!(view.active_pane, 3);
        assert_eq!(view.frozen, None);
        assert!(view.split_window);
        assert_eq!(view.split, Some((2400.5f64.to_bits(), 1800f64.to_bits())));
    }
}
