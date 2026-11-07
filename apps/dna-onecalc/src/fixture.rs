//! The session's sheets and inputs as calc-fml sees them: a [`ReferenceCatalog`] over the named
//! sheets and a [`ReferenceReader`] over the typed inputs that records every read. Known
//! input and sheet metadata reaches functions through the existing [`calc_func::HostFacts`].

use crate::session::InputValue;
use calc_fml::reference::CellRun;
use calc_fml::{ReferenceCatalog, ReferenceReader};
use calc_func::value::{CalcValue, CellKey, GridArea, IdAllocator, SheetId};
use std::collections::BTreeMap;
use std::ops::ControlFlow;

/// Named sheets and the inputs on them.
#[derive(Debug, Default)]
pub(crate) struct Fixture {
    ids: IdAllocator<SheetId>,
    /// Sheet names in the order they were added.
    sheets: Vec<(String, SheetId)>,
    /// Inputs in row-major order: keys order by sheet, then row, then column.
    inputs: BTreeMap<CellKey, InputValue>,
}

impl Fixture {
    /// Adds a sheet; the caller has checked the name.
    pub(crate) fn add_sheet(&mut self, name: &str) -> SheetId {
        let id = self
            .ids
            .issue()
            .expect("a session adds far fewer sheets than an allocator has slots");
        self.sheets.push((name.to_owned(), id));
        id
    }

    pub(crate) fn sheets(&self) -> &[(String, SheetId)] {
        &self.sheets
    }

    pub(crate) fn sheet_name(&self, id: SheetId) -> Option<&str> {
        self.sheets
            .iter()
            .find(|(_, sheet)| *sheet == id)
            .map(|(name, _)| name.as_str())
    }

    pub(crate) fn inputs(&self) -> &BTreeMap<CellKey, InputValue> {
        &self.inputs
    }

    pub(crate) fn inputs_mut(&mut self) -> &mut BTreeMap<CellKey, InputValue> {
        &mut self.inputs
    }
}

/// Facts the session's typed input view actually holds. Its current formula is evaluated
/// separately from the inputs, so self metadata remains unknown. Workbook/file/view facts
/// are absent; input cells cannot carry formulas, filters or hidden-row state.
pub(crate) struct SessionFacts<'f> {
    fixture: &'f Fixture,
    caller: CellKey,
}

impl<'f> SessionFacts<'f> {
    pub(crate) fn new(fixture: &'f Fixture, caller: CellKey) -> Self {
        Self { fixture, caller }
    }
}

impl calc_func::HostFacts for SessionFacts<'_> {
    fn pivot_data_cell(&mut self,_:&str,_:&[calc_func::value::GridArea],_:&[(String,calc_func::value::CalcValue)])->calc_func::PivotLookup{calc_func::PivotLookup::NotModelled}

    fn is_external_sheet(&mut self, _: SheetId) -> bool {
        false
    }

    fn caller(&mut self) -> Option<CellKey> {
        Some(self.caller)
    }

    fn is_formula(&mut self, cell: CellKey) -> Option<bool> {
        (cell != self.caller && self.fixture.inputs.contains_key(&cell)).then_some(false)
    }

    fn formula_text(&mut self, cell: CellKey) -> Option<Option<String>> {
        (cell != self.caller && self.fixture.inputs.contains_key(&cell)).then_some(None)
    }

    fn sheet_index(&mut self, sheet: SheetId) -> Option<u32> {
        self.fixture.sheets.iter().position(|(_, id)| *id == sheet)
            .and_then(|index| u32::try_from(index + 1).ok())
    }

    fn sheet_named(&mut self, name: &str) -> Option<SheetId> {
        ReferenceCatalog::sheet_named(self.fixture, name)
    }

    fn sheet_name(&mut self, sheet: SheetId) -> Option<String> {
        self.fixture.sheet_name(sheet).map(str::to_owned)
    }

    fn sheet_count(&mut self) -> Option<u32> {
        u32::try_from(self.fixture.sheets.len()).ok()
    }

    fn workbook_facts(&mut self) -> Option<std::sync::Arc<dyn calc_func::WorkbookFacts>> {
        None
    }
}

impl ReferenceCatalog for Fixture {
    fn table(&self, name: &str) -> calc_fml::reference::TableLookup {
        let _ = name;
        calc_fml::reference::TableLookup::NotModelled
    }

    fn table_containing(
        &self,
        cell: calc_func::value::CellKey,
    ) -> calc_fml::reference::TableLookup {
        let _ = cell;
        calc_fml::reference::TableLookup::NotModelled
    }

    fn sheet_span(
        &self,
        first: calc_func::value::SheetId,
        last: calc_func::value::SheetId,
    ) -> Option<Vec<calc_func::value::SheetId>> {
        let _ = (first, last);
        None
    }

    fn unknown_calls_are_names(&self) -> bool {
        false
    }

    fn workbook_name(&self) -> Option<&str> {
        None
    }

    fn external_sheet(
        &self,
        book: &calc_fml::reference::ExternalBookRef,
        sheet: &str,
    ) -> Option<calc_func::value::SheetId> {
        let _ = (book, sheet);
        None
    }

    fn external_function_target(
        &self,
        book: &calc_fml::reference::ExternalBookRef,
    ) -> Option<String> {
        let _ = book;
        None
    }

    // OneCalc carries inputs and user functions, and has no workbook defined names.
    fn defined_name(
        &self,
        _: &str,
        _: calc_fml::reference::NameScope,
    ) -> calc_fml::reference::NameDefinition {
        calc_fml::reference::NameDefinition::Unknown
    }

    /// Sheet names match regardless of case, compared by Unicode lowercase; Excel's exact case
    /// folding for non-ASCII names is not measured here.
    fn sheet_named(&self, name: &str) -> Option<SheetId> {
        let wanted = name.to_lowercase();
        self.sheets
            .iter()
            .find(|(held, _)| held.to_lowercase() == wanted)
            .map(|(_, id)| *id)
    }
}

/// One area an evaluation asked for, and the runs of cells the reader returned for it, in the
/// order returned.
#[derive(Clone, Debug, PartialEq)]
pub struct Read {
    pub area: GridArea,
    pub runs: Vec<ReadRun>,
}

/// Consecutive cells of a read, in row-major order, starting at `start`.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadRun {
    pub start: CellKey,
    pub cells: RunCells,
}

/// What a run of a read held.
#[derive(Clone, Debug, PartialEq)]
pub enum RunCells {
    /// Cells holding these input values, one per cell.
    Values(Vec<CalcValue>),
    /// This many cells whose input is blank.
    Blank(u64),
    /// This many cells the session holds no input for: the reader reports them missing, and the
    /// evaluation that needs one stops with a missing-input fault.
    Missing(u64),
}

/// The reader one evaluation runs with: it reads the fixture's inputs and records each request
/// and each run it returns.
pub(crate) struct TracingReader<'f> {
    fixture: &'f Fixture,
    caller: CellKey,
    reads: Vec<Read>,
}

impl<'f> TracingReader<'f> {
    /// A reader over `fixture`, keeping the separately evaluated caller out of inferred
    /// input-only subtotal facts.
    pub(crate) fn new(fixture: &'f Fixture, caller: CellKey) -> Self {
        TracingReader {
            fixture,
            caller,
            reads: Vec::new(),
        }
    }

    pub(crate) fn into_reads(self) -> Vec<Read> {
        self.reads
    }
}

/// The class of a stored input within a run: a value or a blank.
enum Stored<'a> {
    Value(&'a CalcValue),
    Blank,
}

impl ReferenceReader for TracingReader<'_> {
    fn read_denial(&self) -> Option<calc_fml::ReadDenial> {
        None
    }

    fn read_fact(&mut self, _fact: calc_fml::FactRead) {}

    fn spill_extent(&mut self, anchor: calc_func::value::CellKey) -> calc_fml::reference::SpillExtent {
        // An explicitly stored literal cannot be a formula's spill anchor. Missing inputs
        // and the separately evaluated formula's own anchor still have no spill knowledge.
        if anchor != self.caller && self.fixture.inputs.contains_key(&anchor) {
            calc_fml::reference::SpillExtent::None
        } else {
            calc_fml::reference::SpillExtent::NotModelled
        }
    }

    fn subtotal_exclusions(
        &mut self,
        area: calc_func::value::GridArea,
    ) -> Option<calc_func::SubtotalExclusions> {
        // Only literal inputs are stored, and the session cannot hide or filter rows.
        // The separately evaluated formula's own cell is deliberately not inferred.
        (self.fixture.sheet_name(area.sheet()).is_some() && !area.contains(self.caller))
            .then(calc_func::SubtotalExclusions::default)
    }

    fn read_area(
        &mut self,
        _request: calc_fml::ReadRequest,
        area: GridArea,
        sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>,
    ) {
        let first = area.first();
        let last = area.last();
        let width = u64::from(area.column_count());
        let total = area.cell_count();
        let at = |offset: u64| {
            let row = first.row() + u32::try_from(offset / width).expect("a row of the area");
            let column =
                first.column() + u32::try_from(offset % width).expect("a column of the area");
            CellKey::new(first.sheet(), row, column).expect("a cell of the area")
        };
        let mut runs = Vec::new();
        let mut emit = |run: CellRun<'_>, start: u64| {
            runs.push(ReadRun {
                start: at(start),
                cells: match run {
                    CellRun::Values(values) => RunCells::Values(values.to_vec()),
                    CellRun::Blank(count) => RunCells::Blank(count),
                    CellRun::Missing(count) => RunCells::Missing(count),
                },
            });
            sink(run)
        };
        // Stored inputs between the area's first and last cells come in row-major order; those
        // outside its columns are skipped. Every cell between two stored inputs of the area is
        // one the session holds nothing for.
        let fixture: &Fixture = self.fixture;
        let stored = fixture
            .inputs
            .range(first..=last)
            .filter(|(key, _)| area.contains(**key))
            .map(|(key, input)| {
                let offset = u64::from(key.row() - first.row()) * width
                    + u64::from(key.column() - first.column());
                let value = input.value();
                let class = if value.is_empty() {
                    Stored::Blank
                } else {
                    Stored::Value(value)
                };
                (offset, class)
            });
        let mut next = 0u64;
        let mut values: Vec<CalcValue> = Vec::new();
        let mut values_start = 0u64;
        let mut blanks = 0u64;
        let mut blanks_start = 0u64;
        let mut stopped = false;
        for (offset, class) in stored {
            // A run ends where the next stored cell does not continue it.
            let follows = offset == next;
            let continues_values = follows && matches!(class, Stored::Value(_));
            let continues_blanks = follows && matches!(class, Stored::Blank);
            if !values.is_empty() && !continues_values {
                if emit(CellRun::Values(&values), values_start).is_break() {
                    stopped = true;
                    break;
                }
                values.clear();
            }
            if blanks > 0 && !continues_blanks {
                if emit(CellRun::Blank(blanks), blanks_start).is_break() {
                    stopped = true;
                    break;
                }
                blanks = 0;
            }
            if offset > next && emit(CellRun::Missing(offset - next), next).is_break() {
                stopped = true;
                break;
            }
            match class {
                Stored::Value(value) => {
                    if values.is_empty() {
                        values_start = offset;
                    }
                    values.push(value.clone());
                }
                Stored::Blank => {
                    if blanks == 0 {
                        blanks_start = offset;
                    }
                    blanks += 1;
                }
            }
            next = offset + 1;
        }
        if !stopped {
            let tail = if !values.is_empty() {
                emit(CellRun::Values(&values), values_start)
            } else if blanks > 0 {
                emit(CellRun::Blank(blanks), blanks_start)
            } else {
                ControlFlow::Continue(())
            };
            if tail.is_continue() && next < total {
                let _ = emit(CellRun::Missing(total - next), next);
            }
        }
        self.reads.push(Read { area, runs });
    }
}
/// Authored formula-session state. Calculated values are deliberately absent.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormulaFixture {
    pub version: u32,
    pub formula: String,
    pub formula_cell: String,
    /// Effective qualified anchor, retained even while the address field has an invalid draft.
    pub effective_anchor: String,
    pub sheets: Vec<String>,
    pub grid: Vec<String>,
    pub inputs: Vec<FormulaInput>,
    pub legacy: bool,
    pub locale: String,
    pub number_format: String,
    pub vba_draft: String,
    pub vba_loaded: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormulaInput {
    pub address: String,
    pub kind: String,
    pub entry: String,
}
impl Default for FormulaFixture {
    fn default() -> Self {
        Self {
            version: 1,
            formula: String::new(),
            formula_cell: "H1".into(),
            effective_anchor: "Sheet1!H1".into(),
            sheets: vec!["Sheet1".into()],
            grid: vec![String::new(); (crate::grid::ROWS * crate::grid::COLUMNS) as usize],
            inputs: Vec::new(),
            legacy: false,
            locale: "en-US".into(),
            number_format: "General".into(),
            vba_draft: calc_vba::EXAMPLE_MODULE.into(),
            vba_loaded: None,
        }
    }
}
impl FormulaFixture {
    pub const MAX_BYTES: usize = 1_048_576;
    pub const LOCALES: [&'static str; 5] = ["en-US", "en-GB", "en-ZA", "de-DE", "fr-FR"];
    pub fn from_json(text: &str) -> Result<Self, String> {
        if text.len() > Self::MAX_BYTES {
            return Err("Session file is limited to 1 MiB".into());
        }
        let fixture: Self =
            serde_json::from_str(text).map_err(|e| format!("Invalid formula-session JSON: {e}"))?;
        fixture.session()?;
        Ok(fixture)
    }
    pub fn to_json(&self) -> Result<String, String> {
        self.session()?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        if text.len() > Self::MAX_BYTES {
            return Err("Session file is limited to 1 MiB".into());
        }
        Ok(text)
    }
    /// Validate before replacing a current session; draft formula/input errors remain reproducible.
    pub fn session(&self) -> Result<crate::Session, String> {
        if self.version != 1 {
            return Err(format!(
                "Formula-session version {} is unavailable",
                self.version
            ));
        }
        if self.sheets.first().map(String::as_str) != Some("Sheet1") || self.sheets.len() > 256 {
            return Err("A session starts with Sheet1 and supports at most 256 sheets".into());
        }
        if self.grid.len() != (crate::grid::ROWS * crate::grid::COLUMNS) as usize
            || self.inputs.len() > 1024
        {
            return Err(
                "Session grid dimensions or input-row count are outside supported limits".into(),
            );
        }
        if !Self::LOCALES.contains(&self.locale.as_str()) {
            return Err("This session locale is unavailable".into());
        }
        if self.formula.len() > 262_144
            || self.formula_cell.len() > 1024
            || self.effective_anchor.len() > 1024
            || self.number_format.len() > 4096
            || self.grid.iter().any(|v| v.len() > 131_072)
            || self.inputs.iter().any(|r| {
                r.address.len() > 1024
                    || r.entry.len() > 131_072
                    || !["number", "text", "logical", "error", "blank"].contains(&r.kind.as_str())
            })
        {
            return Err("A session field exceeds the supported size or input kind".into());
        }
        if self.vba_draft.len() > 262_144
            || self.vba_loaded.as_ref().is_some_and(|s| s.len() > 262_144)
        {
            return Err("The VBA module exceeds the session size limit".into());
        }
        let mut session =
            crate::Session::new("Sheet1").map_err(|e| crate::display::sheet_error(&e))?;
        for sheet in self.sheets.iter().skip(1) {
            session
                .add_sheet(sheet)
                .map_err(|e| crate::display::sheet_error(&e))?;
        }
        session
            .set_formula_cell(&self.effective_anchor)
            .map_err(|e| crate::display::input_error(&session, &e))?;
        if crate::display::cell(&session, session.formula_cell()) != self.effective_anchor {
            return Err("The effective formula anchor must be a qualified cell address".into());
        }
        if let Some(source) = &self.vba_loaded {
            session
                .load_vba_module("Module1", source, calc_vba::DEFAULT_STEP_BUDGET)
                .map_err(|e| format!("Fixture VBA module: {e}"))?;
        }
        Ok(session)
    }
    pub fn example(which: &str) -> Self {
        let mut fixture = Self::default();
        match which {
            "sum" => {
                fixture.formula = "=SUM(A1:A3)".into();
                fixture.grid[0] = "1.5".into();
                fixture.grid[crate::grid::COLUMNS as usize] = "2".into();
            }
            "array" => fixture.formula = "=SEQUENCE(3,2)".into(),
            "lambda" => fixture.formula = "=LAMBDA(x,x*2)(21)".into(),
            "missing" => fixture.formula = "=Z99+1".into(),
            _ => {}
        }
        fixture
    }
}

#[cfg(test)]
mod formula_files {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn authored_fixture_roundtrip_and_rejection_before_replacement() {
        let mut f = FormulaFixture::example("sum");
        f.sheets.push("Inputs".into());
        f.inputs.push(FormulaInput {
            address: "Inputs!A1".into(),
            kind: "text".into(),
            entry: "é & < >".into(),
        });
        f.locale = "de-DE".into();
        f.number_format = "0.00".into();
        assert_eq!(FormulaFixture::from_json(&f.to_json().unwrap()).unwrap(), f);
        let mut future = f.clone();
        future.version = 2;
        assert!(future.to_json().is_err());
        let mut wrong = f.clone();
        wrong.sheets.push("inputs".into());
        assert!(wrong.session().is_err());
        let mut draft = f;
        draft.formula = "=SUM(".into();
        draft.inputs[0].entry = "not a number".into();
        assert!(draft.to_json().is_ok());
        assert!(FormulaFixture::from_json("{\"version\":1}").is_err());
    }
}
