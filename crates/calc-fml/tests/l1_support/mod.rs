//! A small workbook for the L1 tests (work package dc-mzk): sheets with cells, defined names and
//! spills, implementing calc-fml's catalogue and reader, and one call that compiles and evaluates a
//! formula through `calc_fml::evaluate`, the route the engine takes.

#![allow(dead_code)]

use calc_fml::reference::{
    CellRun, FormulaAnchor, NameDefinition, NameScope, ReferenceCatalog, ReferenceNotation,
    ReferenceReader, SpillExtent,
};
use calc_fml::{
    CompileError, EvalFault, EvalFrame, EvalScratch, EvaluationServices, FormulaForm, PlanCache,
    compile_with, evaluate, instantiate, parse_formula,
};
use calc_func::value::{
    CalcArray, CalcValue, CellKey, ExcelText, GridArea, IdAllocator, NumericMode, SheetId,
    WorksheetErrorCode,
};
use std::collections::BTreeMap;
use std::ops::ControlFlow;

pub struct Book {
    pub sheets: Vec<(String, SheetId)>,
    pub cells: BTreeMap<CellKey, CalcValue>,
    /// Workbook names by upper-case spelling.
    pub names: BTreeMap<String, String>,
    /// Sheet names by sheet and upper-case spelling.
    pub local_names: BTreeMap<(SheetId, String), String>,
    pub spills: BTreeMap<CellKey, GridArea>,
    /// Whether the catalogue carries names; without them every name is not modelled.
    pub knows_names: bool,
    /// Whether the reader knows spills.
    pub knows_spills: bool,
}

impl Default for Book {
    fn default() -> Self {
        Self::new()
    }
}

impl Book {
    /// Two sheets, Sheet1 and Sheet2, empty, names and spills known.
    pub fn new() -> Self {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let sheets = vec![
            ("Sheet1".to_owned(), ids.issue().unwrap()),
            ("Sheet2".to_owned(), ids.issue().unwrap()),
        ];
        Book {
            sheets,
            cells: BTreeMap::new(),
            names: BTreeMap::new(),
            local_names: BTreeMap::new(),
            spills: BTreeMap::new(),
            knows_names: true,
            knows_spills: true,
        }
    }

    pub fn sheet(&self, name: &str) -> SheetId {
        self.sheets
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, id)| *id)
            .expect("a sheet of the book")
    }

    /// The cell an A1 address names, `Sheet!A1` or `A1` on Sheet1.
    pub fn at(&self, address: &str) -> CellKey {
        let (sheet, cell) = match address.split_once('!') {
            Some((sheet, cell)) => (self.sheet(sheet), cell),
            None => (self.sheet("Sheet1"), address),
        };
        let letters: String = cell
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        let digits = &cell[letters.len()..];
        let column = letters.bytes().fold(0u32, |n, b| {
            n * 26 + u32::from(b.to_ascii_uppercase() - b'A' + 1)
        });
        CellKey::new(sheet, digits.parse().unwrap(), column).unwrap()
    }

    pub fn area(&self, a: &str, b: &str) -> GridArea {
        GridArea::new(self.at(a), self.at(b)).unwrap()
    }

    pub fn set(&mut self, address: &str, value: CalcValue) -> &mut Self {
        let at = self.at(address);
        self.cells.insert(at, value);
        self
    }

    pub fn number(&mut self, address: &str, n: f64) -> &mut Self {
        self.set(address, CalcValue::number(n))
    }

    pub fn name(&mut self, name: &str, definition: &str) -> &mut Self {
        self.names
            .insert(name.to_uppercase(), definition.to_owned());
        self
    }

    pub fn local_name(&mut self, sheet: &str, name: &str, definition: &str) -> &mut Self {
        let sheet = self.sheet(sheet);
        self.local_names
            .insert((sheet, name.to_uppercase()), definition.to_owned());
        self
    }

    /// Compiles `formula` at `at` in `form` and evaluates it.
    pub fn eval_in(
        &mut self,
        form: FormulaForm,
        formula: &str,
        at: &str,
    ) -> Result<Result<CalcValue, EvalFault>, CompileError> {
        let anchor = FormulaAnchor::new(self.at(at));
        let plan = compile_with(
            &parse_formula(formula),
            ReferenceNotation::A1,
            anchor,
            &Catalog(self),
            &mut PlanCache::new(),
            form,
        )?;
        let instance = instantiate(&plan, anchor);
        let mut scratch = EvalScratch::new();
        let catalog = Catalog(self);
        let mut reader = Reader(self);
        let mut services =
            EvaluationServices::new(&NumericMode::Fast, &mut reader).with_catalog(&catalog);
        Ok(evaluate(
            EvalFrame::new(&instance, &mut scratch),
            &mut services,
        ))
    }

    /// A dynamic-array formula's value at Sheet1!Z100, which every compile and evaluation must
    /// give.
    pub fn dynamic(&mut self, formula: &str) -> CalcValue {
        self.dynamic_at(formula, "Z100")
    }

    pub fn dynamic_at(&mut self, formula: &str, at: &str) -> CalcValue {
        match self.eval_in(FormulaForm::Dynamic, formula, at) {
            Ok(Ok(value)) => value,
            other => panic!("{formula}: {other:?}"),
        }
    }

    /// A legacy formula's value at `at`.
    pub fn legacy_at(&mut self, formula: &str, at: &str) -> CalcValue {
        match self.eval_in(FormulaForm::Legacy, formula, at) {
            Ok(Ok(value)) => value,
            other => panic!("{formula}: {other:?}"),
        }
    }
}

struct Catalog<'b>(&'b Book);

impl ReferenceCatalog for Catalog<'_> {
    fn sheet_named(&self, name: &str) -> Option<SheetId> {
        self.0
            .sheets
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, id)| *id)
    }

    fn defined_name(&self, name: &str, scope: NameScope) -> NameDefinition {
        if !self.0.knows_names {
            return NameDefinition::NotModelled;
        }
        let key = name.to_uppercase();
        let local = |sheet: SheetId| self.0.local_names.get(&(sheet, key.clone()));
        let found = match scope {
            NameScope::Formula(sheet) => local(sheet).or_else(|| self.0.names.get(&key)),
            NameScope::Sheet(sheet) => local(sheet),
        };
        match found {
            Some(definition) => NameDefinition::Formula(definition.clone()),
            None => NameDefinition::Unknown,
        }
    }
}

struct Reader<'b>(&'b Book);

impl ReferenceReader for Reader<'_> {
    fn read_area(&mut self, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        let first = area.first();
        let last = area.last();
        for row in first.row()..=last.row() {
            for column in first.column()..=last.column() {
                let key = CellKey::new(area.sheet(), row, column).unwrap();
                let flow = match self.0.cells.get(&key) {
                    Some(value) => sink(CellRun::Values(std::slice::from_ref(value))),
                    None => sink(CellRun::Blank(1)),
                };
                if flow.is_break() {
                    return;
                }
            }
        }
    }

    fn spill_extent(&mut self, anchor: CellKey) -> SpillExtent {
        if !self.0.knows_spills {
            return SpillExtent::NotModelled;
        }
        match self.0.spills.get(&anchor) {
            Some(area) => SpillExtent::Area(*area),
            None => SpillExtent::None,
        }
    }
}

pub fn n(value: f64) -> CalcValue {
    CalcValue::number(value)
}

pub fn t(text: &str) -> CalcValue {
    CalcValue::text(ExcelText::from_interop_assignment(text))
}

pub fn b(value: bool) -> CalcValue {
    CalcValue::logical(value)
}

pub fn e(code: WorksheetErrorCode) -> CalcValue {
    CalcValue::error(code)
}

/// The array of these rows.
pub fn rows(rows: &[&[CalcValue]]) -> CalcValue {
    CalcValue::array(CalcArray::from_rows(rows.iter().map(|row| row.to_vec()).collect()).unwrap())
}

/// The array of these numbers, in rows.
pub fn numbers(values: &[&[f64]]) -> CalcValue {
    CalcValue::array(
        CalcArray::from_rows(
            values
                .iter()
                .map(|row| row.iter().map(|v| CalcValue::number(*v)).collect())
                .collect(),
        )
        .unwrap(),
    )
}
