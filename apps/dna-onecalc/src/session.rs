//! The OneCalc application controller: a session of named sheets, typed inputs and one formula,
//! calculated through calc-fml and calc-func alone.

use crate::fixture::{Fixture, Read, SessionFacts, TracingReader};
use calc_fml::reference::{BindError, FormulaAnchor, ReferenceNotation, ReferenceShape};
use calc_fml::syntax::SyntaxDiagnostic;
use calc_fml::{
    CalcEnvironment, CompileError, EvalContinuation, EvalFault, EvalFrame, EvalScratch, EvalStep,
    FormulaEvaluationContext, FormulaForm, PlanCache, ReferenceCatalog, bind_references,
    compile_with_functions, evaluate, instantiate, parse_formula,
};
use calc_func::value::{
    CalcValue, CellKey, EXCEL_TEXT_MAX_UTF16_CODE_UNITS, ExcelText, NumericMode, SheetId,
    WorksheetErrorCode,
};

/// A typed input: the value one cell holds. Only values a worksheet cell can hold are
/// constructed: a finite number, text within Excel's length limit, a logical, a worksheet error,
/// or a blank.
#[derive(Clone, Debug, PartialEq)]
pub struct InputValue {
    value: CalcValue,
}

impl InputValue {
    /// A number input. A worksheet number carries no negative zero (calc-func publishes every zero
    /// as +0), so -0 is held as +0. NaN, the infinities and subnormal numbers are refused: no
    /// cell holds the first three, and Excel's handling of a subnormal is not modelled (calc-fml
    /// refuses such literals for the same reason).
    pub fn number(value: f64) -> Result<InputValue, InputError> {
        if !value.is_finite() || value.is_subnormal() {
            return Err(InputError::NumberNotHeld { value });
        }
        let value = if value == 0.0 { 0.0 } else { value };
        Ok(InputValue {
            value: CalcValue::number(value),
        })
    }

    /// A text input, refused when it is longer than Excel's limit of 32,767 UTF-16 code units
    /// rather than cut to it.
    pub fn text(text: &str) -> Result<InputValue, InputError> {
        let units: Vec<u16> = text.encode_utf16().collect();
        if units.len() > EXCEL_TEXT_MAX_UTF16_CODE_UNITS {
            return Err(InputError::TextTooLong { units: units.len() });
        }
        Ok(InputValue {
            value: CalcValue::text(ExcelText::from_utf16_code_units(units)),
        })
    }

    pub fn logical(value: bool) -> InputValue {
        InputValue {
            value: CalcValue::logical(value),
        }
    }

    pub fn error(code: WorksheetErrorCode) -> InputValue {
        InputValue {
            value: CalcValue::error(code),
        }
    }

    /// A blank cell: one that exists and holds nothing, unlike a cell with no input, which the
    /// session cannot supply.
    pub fn blank() -> InputValue {
        InputValue {
            value: CalcValue::empty(),
        }
    }

    /// The value the cell holds.
    pub fn value(&self) -> &CalcValue {
        &self.value
    }
}

/// Why a sheet was not added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SheetError {
    /// The session already has a sheet of this name; names match regardless of case.
    Duplicate { name: String },
    /// Excel does not accept this sheet name: it is blank, longer than 31 characters, holds one
    /// of `: \ / ? * [ ]`, begins or ends with an apostrophe, or is `History` (Microsoft's
    /// documented rules for renaming a worksheet; counted in UTF-16 code units).
    InvalidName { name: String },
}

/// Why an input or the formula cell was refused.
#[derive(Clone, Debug, PartialEq)]
pub enum InputError {
    /// The address is not one cell reference: a range, a name, a number, malformed text or a
    /// reference whose sheet was deleted (`#REF!A1`).
    NotACell { address: String },
    /// calc-fml's binder refused the address: a sheet the session does not have, or a cell
    /// outside the grid.
    Address { address: String, error: BindError },
    /// The cell is the formula's own cell, which holds the formula and so no input.
    FormulaCell { cell: CellKey },
    /// A number no worksheet cell holds: NaN, an infinity, or a subnormal number.
    NumberNotHeld { value: f64 },
    /// Text longer than Excel's 32,767 UTF-16 code units.
    TextTooLong { units: usize },
}

/// Why a calculation produced no value, or the first thing that stopped it.
#[derive(Clone, Debug, PartialEq)]
pub enum Diagnostic {
    /// A user function returned #VALUE! with this execution diagnostic.
    UserFunction(String),
    /// The parser refused the formula text; each diagnostic has its span in UTF-16 code units.
    Syntax(SyntaxDiagnostic),
    /// The formula parsed but did not compile: a reference binding refused (an unknown sheet, a
    /// cell outside the grid), syntax not compiled yet, an unknown function, an arity the entry
    /// does not admit, or a number literal whose value is not modelled. Each carries its span.
    Compile(CompileError),
    /// The evaluation stopped without a value: a cell with no input, a calc-func fault, or another
    /// typed refusal of calc-fml's evaluator.
    Eval(EvalFault),
}

/// What one calculation of the session's formula gave.
#[derive(Clone, Debug, PartialEq)]
pub struct Calculation {
    /// The formula's value, which may be a worksheet error such as `#DIV/0!`; `None` when the
    /// diagnostics stopped the calculation.
    pub result: Option<CalcValue>,
    /// Every syntax diagnostic of a formula the parser refused, or the one compile error or
    /// evaluation fault that stopped it; empty when `result` holds the value.
    pub diagnostics: Vec<Diagnostic>,
    /// The areas the evaluation asked the session's reader for, in order, with what it returned.
    pub reads: Vec<Read>,
}

/// The options a session keeps beside its formula: what the file format has no place for.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionOptions {
    /// The locale profile's culture name (`en-US`).
    pub locale: String,
    /// The number format text the result is shown with, in the locale's separators.
    pub number_format: String,
    /// Whether serial dates count from 1904 rather than 1900 (the workbook's `date1904`).
    #[serde(default)]
    pub date_1904: bool,
}

impl Default for SessionOptions {
    fn default() -> Self {
        SessionOptions {
            locale: "en-US".to_owned(),
            number_format: "General".to_owned(),
            date_1904: false,
        }
    }
}

impl SessionOptions {
    /// The locales a session offers.
    pub const LOCALES: [&'static str; 5] = ["en-US", "en-GB", "en-ZA", "de-DE", "fr-FR"];
}

/// An add-in a session lists: its kind (`xll`, `xlam`, `bas`), its path as the person chose it and
/// the SHA-256 of its file when it was listed. A session file lists add-ins; it never loads one.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AddinRef {
    pub kind: String,
    pub path: String,
    pub hash: String,
}

/// Where an add-in's asynchronous returns arrive from, for a host that gives the session
/// continuations: a registered function that answers later (a native async call, a thread-safe call
/// on the host's pool) suspends the evaluation, and the owner of the session waits for the return.
/// The session keeps the evaluation as it was and resumes it with the value. A session with no such
/// owner (the browsers') calls every function to its end, as before.
pub trait ExternalReturns: std::fmt::Debug {
    /// A calculation starts: waits are counted from here.
    fn begin(&mut self) {}

    /// Waits for the return of `request`, giving the platform its turns meanwhile, or says why it
    /// did not come (the add-in never returned, its return failed).
    fn await_return(&mut self, request: u64) -> Result<CalcValue, String>;

    /// A call that runs on the add-ins' pool is not done: gives the platform a turn, then the
    /// evaluation tries it again. An error says the call took too long.
    fn idle(&mut self) -> Result<(), String>;
}

/// The OneCalc application controller: named sheets, typed inputs, one formula and the cell it
/// is entered in, and the caller-owned state calc-fml compiles and evaluates with (a plan cache
/// and evaluation scratch). It holds no dependency graph and no recalculation engine.
#[derive(Debug)]
pub struct Session {
    fixture: Fixture,
    functions: calc_fml::CallableTable,
    references: std::rc::Rc<calc_fml::reference::ReferenceTracker>,
    formula: String,
    formula_cell: CellKey,
    plans: PlanCache,
    scratch: EvalScratch,
    /// The calculation environment: the date system, the locale profile and the clock the date,
    /// time and number-format functions read. None until set, which refuses those functions.
    environment: Option<CalcEnvironment>,
    /// Every calculation runs under Fast, the default numeric mode (plan section 5); calc-func
    /// has no Exact implementation, so a mode choice belongs with it.
    numeric: NumericMode,
    /// The form the formula is entered in: dynamic-array, as Excel 365 enters a formula, unless
    /// set otherwise.
    form: FormulaForm,
    /// Whether the legacy form is a plain formula read from a file (no `{=...}` array braces).
    plain: bool,
    options: SessionOptions,
    addins: Vec<AddinRef>,
    /// Whether a VBA standard module is loaded (its source is not kept: the session file holds no
    /// VBA project until dc-0f1k.4).
    vba_loaded: bool,
    /// The owner that delivers asynchronous returns; with one, evaluation runs as a continuation.
    returns: Option<Box<dyn ExternalReturns>>,
    /// Changes whenever the table of registered functions is replaced (a counter shared by all
    /// sessions, so a replacement session never repeats a version a surface has seen).
    functions_version: u64,
}

/// The next version of a table of registered functions.
fn next_functions_version() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Sheet names Excel reserves; matched regardless of case.
const RESERVED_SHEET_NAMES: [&str; 1] = ["History"];

/// The longest sheet name Excel accepts, in UTF-16 code units.
const MAX_SHEET_NAME_UNITS: usize = 31;

/// Characters Excel does not accept in a sheet name.
const SHEET_NAME_FORBIDDEN: [char; 7] = [':', '\\', '/', '?', '*', '[', ']'];

impl Session {
    /// A session with one sheet, `first_sheet`, an empty formula entered in its cell A1, and no
    /// inputs.
    pub fn new(first_sheet: &str) -> Result<Session, SheetError> {
        let mut fixture = Fixture::default();
        check_sheet_name(&fixture, first_sheet)?;
        let sheet = fixture.add_sheet(first_sheet);
        Ok(Session {
            fixture,
            functions: calc_fml::CallableTable::new(),
            references: calc_fml::reference::ReferenceTracker::new(),
            formula: String::new(),
            formula_cell: CellKey::new(sheet, 1, 1).expect("A1 is on the grid"),
            plans: PlanCache::new(),
            scratch: EvalScratch::new(),
            environment: None,
            numeric: NumericMode::Fast,
            form: FormulaForm::Dynamic,
            plain: false,
            options: SessionOptions::default(),
            addins: Vec::new(),
            vba_loaded: false,
            returns: None,
            functions_version: next_functions_version(),
        })
    }

    /// Gives the session an owner of asynchronous returns, which makes every evaluation a
    /// continuation that waits on it (or takes it away again).
    pub fn set_external_returns(&mut self, returns: Option<Box<dyn ExternalReturns>>) {
        self.returns = returns;
    }

    /// Takes the owner of asynchronous returns out of the session, for the one that replaces it.
    pub fn take_external_returns(&mut self) -> Option<Box<dyn ExternalReturns>> {
        self.returns.take()
    }

    /// The options kept beside the formula.
    pub fn options(&self) -> &SessionOptions {
        &self.options
    }

    pub fn set_options(&mut self, options: SessionOptions) {
        self.options = options;
    }

    /// The add-ins the session lists.
    pub fn addins(&self) -> &[AddinRef] {
        &self.addins
    }

    pub fn set_addins(&mut self, addins: Vec<AddinRef>) {
        self.addins = addins;
    }

    /// Records whether a VBA module is loaded, for a host that keeps the module beside its
    /// add-ins and publishes the one table of functions itself.
    pub fn set_vba_loaded(&mut self, loaded: bool) {
        self.vba_loaded = loaded;
    }

    /// Whether a VBA module is loaded.
    pub fn has_vba_module(&self) -> bool {
        self.vba_loaded
    }

    /// Adds a sheet named `name`.
    pub fn add_sheet(&mut self, name: &str) -> Result<SheetId, SheetError> {
        check_sheet_name(&self.fixture, name)?;
        Ok(self.fixture.add_sheet(name))
    }

    /// The sheets, in the order they were added.
    pub fn sheets(&self) -> impl Iterator<Item = (&str, SheetId)> + '_ {
        self.fixture
            .sheets()
            .iter()
            .map(|(name, id)| (name.as_str(), *id))
    }

    pub fn sheet_name(&self, sheet: SheetId) -> Option<&str> {
        self.fixture.sheet_name(sheet)
    }

    /// The cell `address` names: one A1 cell reference, read by calc-fml's parser and binder as a
    /// reference in a formula entered in the formula cell would be, so an unqualified address
    /// names a cell of the formula cell's sheet and a qualified one (`Sheet2!B3`,
    /// `'Q2 Data'!B3`) a cell of the sheet it names.
    pub fn cell(&self, address: &str) -> Result<CellKey, InputError> {
        let not_a_cell = || InputError::NotACell {
            address: address.to_owned(),
        };
        let text = address.trim();
        if text.starts_with('=') {
            return Err(not_a_cell());
        }
        let tree = parse_formula(text);
        if !tree.diagnostics().is_empty() {
            return Err(not_a_cell());
        }
        let expression = tree.expression().ok_or_else(not_a_cell)?;
        let anchor = FormulaAnchor::new(self.formula_cell);
        let sites = bind_references(&tree, ReferenceNotation::A1, anchor, &self.fixture);
        let [site] = sites.as_slice() else {
            return Err(not_a_cell());
        };
        if site.span != expression.span() {
            return Err(not_a_cell());
        }
        let operand = site.binding.clone().map_err(|error| InputError::Address {
            address: address.to_owned(),
            error,
        })?;
        if !matches!(operand.shape, ReferenceShape::Cell(_)) {
            return Err(not_a_cell());
        }
        let area = operand.instantiate(anchor).map_err(|_| not_a_cell())?;
        Ok(area.first())
    }

    /// The cell the formula is entered in: its sheet is the one unqualified references name, and
    /// relative references are offsets from it.
    pub fn formula_cell(&self) -> CellKey {
        self.formula_cell
    }

    /// Enters the formula in the cell `address` names, refused when that cell holds an input.
    pub fn set_formula_cell(&mut self, address: &str) -> Result<CellKey, InputError> {
        let cell = self.cell(address)?;
        if self.fixture.inputs().contains_key(&cell) {
            return Err(InputError::FormulaCell { cell });
        }
        self.formula_cell = cell;
        Ok(cell)
    }

    /// The calculation environment, when one is set.
    pub fn environment(&self) -> Option<CalcEnvironment> {
        self.environment
    }

    /// Sets the calculation environment the date, time and number-format functions read: the
    /// workbook's date system, the locale profile (separators, currency, date order) and the clock
    /// (`now`, a serial date number) NOW and TODAY return.
    pub fn set_environment(&mut self, environment: Option<CalcEnvironment>) {
        self.environment = environment;
    }

    /// The form the formula is entered in.
    pub fn form(&self) -> FormulaForm {
        self.form
    }

    /// Enters the formula in `form`: [`FormulaForm::Dynamic`] as Excel 365 enters a formula (a
    /// function of single values lifts over an array, and an array result is the result), or
    /// [`FormulaForm::Legacy`] as an older Excel does (implicit intersection, one value).
    pub fn set_form(&mut self, form: FormulaForm) {
        self.form = form;
        self.plain = false;
    }

    /// Whether the formula is a plain formula of a file: the legacy form (implicit intersection)
    /// without array braces. A plain `<f>` in a file is this, not the dynamic form.
    pub fn is_plain(&self) -> bool {
        self.plain && self.form == FormulaForm::Legacy
    }

    /// Enters the formula as a plain legacy formula, as a file's plain `<f>` is read.
    pub fn set_plain_legacy(&mut self) {
        self.form = FormulaForm::Legacy;
        self.plain = true;
    }

    pub fn formula(&self) -> &str {
        &self.formula
    }

    /// Sets the formula text, with or without its leading `=`.
    pub fn set_formula(&mut self, text: &str) {
        self.formula = text.to_owned();
    }

    /// Sets the input of the cell `address` names, replacing any it held, and returns the cell.
    /// The formula's own cell holds no input.
    pub fn set_input(&mut self, address: &str, value: InputValue) -> Result<CellKey, InputError> {
        let cell = self.cell(address)?;
        if cell == self.formula_cell {
            return Err(InputError::FormulaCell { cell });
        }
        self.fixture.inputs_mut().insert(cell, value);
        Ok(cell)
    }

    /// Sets the input of `cell`, replacing any it held. The formula's own cell holds no input.
    pub fn set_input_at(&mut self, cell: CellKey, value: InputValue) -> Result<(), InputError> {
        if cell == self.formula_cell {
            return Err(InputError::FormulaCell { cell });
        }
        self.fixture.inputs_mut().insert(cell, value);
        Ok(())
    }

    /// Removes the input of the cell `address` names and returns it; the cell then has no input.
    pub fn remove_input(&mut self, address: &str) -> Result<Option<InputValue>, InputError> {
        let cell = self.cell(address)?;
        Ok(self.fixture.inputs_mut().remove(&cell))
    }

    /// Removes every input.
    pub fn clear_inputs(&mut self) {
        self.fixture.inputs_mut().clear();
    }

    pub fn input(&self, cell: CellKey) -> Option<&InputValue> {
        self.fixture.inputs().get(&cell)
    }

    /// The inputs, in row-major order by sheet.
    pub fn inputs(&self) -> impl Iterator<Item = (CellKey, &InputValue)> + '_ {
        self.fixture
            .inputs()
            .iter()
            .map(|(cell, value)| (*cell, value))
    }

    /// Publish a complete replacement. Plans retain no engine objects or old registrations.
    pub fn set_user_functions(&mut self, functions: calc_fml::CallableTable) {
        self.functions.replace(functions);
        self.functions_version = next_functions_version();
    }

    /// The version of the table of registered functions, which changes when it is replaced.
    pub fn functions_version(&self) -> u64 {
        self.functions_version
    }

    /// Load a VBA standard module. Failed compilation preserves the previous registrations.
    pub fn load_vba_module(
        &mut self,
        name: &str,
        source: &str,
        max_steps: u64,
    ) -> Result<Vec<String>, String> {
        let functions = calc_vba::load_module(name, source, max_steps)?;
        let names = functions.names().map(str::to_owned).collect();
        self.set_user_functions(functions);
        self.vba_loaded = true;
        Ok(names)
    }

    /// The callable user functions, which a new session over the same add-ins takes over.
    pub fn user_functions(&self) -> &calc_fml::CallableTable {
        &self.functions
    }

    /// The names of the callable user functions (add-in registrations and VBA), in table order.
    pub fn user_function_names(&self) -> Vec<String> {
        self.functions.names().map(str::to_owned).collect()
    }

    /// What the providers documented of the callable user functions, in name order.
    pub fn user_function_descriptors(&self) -> Vec<calc_fml::CallableDescriptor> {
        self.functions.descriptors().cloned().collect()
    }

    pub fn clear_user_functions(&mut self) {
        self.vba_loaded = false;
        self.set_user_functions(calc_fml::CallableTable::new());
    }

    /// Calculates the formula: parse, compile (the session's plan cache shares one plan per
    /// normal form), place at the formula cell, and evaluate over the session's inputs, recording
    /// every read.
    pub fn calculate(&mut self) -> Calculation {
        let formula = self.formula.clone();
        self.calculate_text(&formula)
    }

    /// [`Session::calculate`] for `formula` in place of the session's own: the same inputs, form,
    /// environment and functions. A part of a formula is evaluated this way (F9, the trace).
    pub fn calculate_text(&mut self, formula: &str) -> Calculation {
        let tree = parse_formula(formula);
        let syntax: Vec<Diagnostic> = tree
            .diagnostics()
            .iter()
            .cloned()
            .map(Diagnostic::Syntax)
            .collect();
        if !syntax.is_empty() {
            return Calculation {
                result: None,
                diagnostics: syntax,
                reads: Vec::new(),
            };
        }
        let anchor = FormulaAnchor::new(self.formula_cell);
        let plan = match compile_with_functions(
            &tree,
            ReferenceNotation::A1,
            anchor,
            &self.fixture,
            &mut self.plans,
            self.form,
            &self.functions,
        ) {
            Ok(plan) => plan,
            Err(error) => {
                return Calculation {
                    result: None,
                    diagnostics: vec![Diagnostic::Compile(error)],
                    reads: Vec::new(),
                };
            }
        };
        let instance = instantiate(&plan, anchor);
        let mut reader = TracingReader::new(&self.fixture, self.formula_cell);
        let mut facts = SessionFacts::new(&self.fixture, self.formula_cell);
        // The session's sheets are the workbook context INDIRECT's text is bound in.
        let mut function_diagnostics = Vec::new();
        let references = self.references.pin();
        let mut services = FormulaEvaluationContext::new(
            &self.numeric,
            &mut reader,
            self.environment,
            Some(&mut facts),
            calc_fml::ReadRequest::DEPENDENCY,
        )
        .with_catalog(&self.fixture)
        .with_functions(&self.functions)
        .with_reference_handles(&*references)
        .with_function_diagnostics(&mut function_diagnostics);
        let mut failure = None;
        let outcome = match self.returns.as_mut() {
            None => evaluate(EvalFrame::new(&instance, &mut self.scratch), &mut services),
            Some(returns) => {
                // A continuation: a call that answers later suspends the evaluation, the owner
                // waits for the return, and the evaluation resumes with it.
                returns.begin();
                let mut run =
                    EvalContinuation::new(&instance, std::mem::replace(&mut self.scratch, EvalScratch::new()));
                let outcome = loop {
                    match run.advance(&mut services, usize::MAX) {
                        EvalStep::Complete(outcome) => break outcome,
                        EvalStep::Suspended(_) => {
                            let Some((invocation, request)) = run.pending_external() else {
                                // A call on the add-ins' pool, which is polled until it is done.
                                if let Err(why) = returns.idle() {
                                    failure = Some(why);
                                    break Err(EvalFault::NotModelled {
                                        construct: "an add-in call that did not finish",
                                        owner: "dc-snz.23",
                                    });
                                }
                                continue;
                            };
                            match returns.await_return(request) {
                                Ok(value) => {
                                    if !run.accept_external(invocation, request, value) {
                                        break Err(EvalFault::NotModelled {
                                            construct: "an add-in return the evaluation did not wait for",
                                            owner: "dc-snz.23",
                                        });
                                    }
                                }
                                Err(why) => {
                                    failure = Some(why);
                                    break Err(EvalFault::NotModelled {
                                        construct: "an add-in return that failed or never came",
                                        owner: "dc-snz.23",
                                    });
                                }
                            }
                        }
                    }
                };
                self.scratch = run.into_scratch();
                outcome
            }
        };
        drop(services);
        function_diagnostics.extend(failure);
        let reads = reader.into_reads();
        match outcome {
            Ok(value) => Calculation {
                result: Some(value),
                diagnostics: function_diagnostics
                    .into_iter()
                    .map(Diagnostic::UserFunction)
                    .collect(),
                reads,
            },
            Err(fault) => {
                let mut diagnostics: Vec<_> = function_diagnostics
                    .into_iter()
                    .map(Diagnostic::UserFunction)
                    .collect();
                diagnostics.push(Diagnostic::Eval(fault));
                Calculation {
                    result: None,
                    diagnostics,
                    reads,
                }
            }
        }
    }
}

/// Refuses a name Excel does not accept for a sheet, or one the session already has.
fn check_sheet_name(fixture: &Fixture, name: &str) -> Result<(), SheetError> {
    let invalid = name.trim().is_empty()
        || name.encode_utf16().count() > MAX_SHEET_NAME_UNITS
        || name.contains(SHEET_NAME_FORBIDDEN)
        || name.starts_with('\'')
        || name.ends_with('\'')
        || RESERVED_SHEET_NAMES
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(name));
    if invalid {
        return Err(SheetError::InvalidName {
            name: name.to_owned(),
        });
    }
    if fixture.sheet_named(name).is_some() {
        return Err(SheetError::Duplicate {
            name: name.to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod continuation_tests {
    use super::*;
    use calc_fml::{UserFunction, UserFunctionContext, UserFunctionInvocation, UserFunctionStep};
    use std::cell::Cell;
    use std::rc::Rc;

    /// A function that answers later: its invocation parks on request 7 until the owner delivers
    /// the return, then completes with the delivered value plus one.
    #[derive(Debug, Default)]
    struct Later;
    #[derive(Debug, Default)]
    struct LaterCall {
        started: bool,
    }
    impl UserFunctionInvocation for LaterCall {
        fn resume(
            &mut self,
            context: &mut dyn UserFunctionContext,
            _quantum: u64,
        ) -> Result<UserFunctionStep, String> {
            if !self.started {
                self.started = true;
                return Ok(UserFunctionStep::Awaiting { request: 7 });
            }
            match context.take_async_result(7) {
                Some(value) => Ok(UserFunctionStep::Complete(CalcValue::number(
                    value.as_number().unwrap_or(f64::NAN) + 1.,
                ))),
                None => Ok(UserFunctionStep::Awaiting { request: 7 }),
            }
        }
    }
    impl UserFunction for Later {
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            Err("called to its end".into())
        }
        fn begin_call(
            &self,
            _arguments: &[CalcValue],
            _frame: calc_fml::UserInvocationFrame,
            _context: &mut dyn UserFunctionContext,
        ) -> Result<Option<Box<dyn UserFunctionInvocation>>, String> {
            Ok(Some(Box::new(LaterCall::default())))
        }
    }

    /// A function that runs on a pool: its invocation is not done for the first few resumptions.
    #[derive(Debug)]
    struct Pooled;
    #[derive(Debug, Default)]
    struct PooledCall {
        polls: u32,
    }
    impl UserFunctionInvocation for PooledCall {
        fn resume(&mut self, _: &mut dyn UserFunctionContext, _: u64) -> Result<UserFunctionStep, String> {
            self.polls += 1;
            Ok(if self.polls < 4 {
                UserFunctionStep::Suspended
            } else {
                UserFunctionStep::Complete(CalcValue::number(f64::from(self.polls)))
            })
        }
    }
    impl UserFunction for Pooled {
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            Err("called to its end".into())
        }
        fn begin_call(
            &self,
            _: &[CalcValue],
            _: calc_fml::UserInvocationFrame,
            _: &mut dyn UserFunctionContext,
        ) -> Result<Option<Box<dyn UserFunctionInvocation>>, String> {
            Ok(Some(Box::new(PooledCall::default())))
        }
    }

    /// The owner the test gives the session: the return of request 7 is 41, and it counts its turns.
    #[derive(Debug)]
    struct Owner {
        turns: Rc<Cell<u32>>,
        returns: Rc<Cell<u32>>,
        fail_after: Option<u32>,
    }
    impl ExternalReturns for Owner {
        fn await_return(&mut self, request: u64) -> Result<CalcValue, String> {
            assert_eq!(request, 7);
            self.returns.set(self.returns.get() + 1);
            Ok(CalcValue::number(41.))
        }
        fn idle(&mut self) -> Result<(), String> {
            self.turns.set(self.turns.get() + 1);
            match self.fail_after {
                Some(limit) if self.turns.get() > limit => Err("took too long".into()),
                _ => Ok(()),
            }
        }
    }

    fn session(fail_after: Option<u32>) -> (Session, Rc<Cell<u32>>, Rc<Cell<u32>>) {
        let mut session = Session::new("Sheet1").unwrap();
        let mut table = calc_fml::CallableTable::new();
        table.register("Later", Rc::new(Later)).unwrap();
        table.register("Pooled", Rc::new(Pooled)).unwrap();
        session.set_user_functions(table);
        let (turns, returns) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        session.set_external_returns(Some(Box::new(Owner {
            turns: turns.clone(),
            returns: returns.clone(),
            fail_after,
        })));
        (session, turns, returns)
    }

    /// A call that answers later suspends the evaluation, the owner delivers the return, and the
    /// evaluation resumes with it; a call on the pool is polled until done. A session with no owner
    /// calls every function to its end, as the browsers' sessions do.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_session_with_an_owner_of_returns_runs_late_calls_as_continuations() {
        let (mut session, turns, returns) = session(None);
        session.set_formula("=Later()*2");
        let calculation = session.calculate();
        assert_eq!(calculation.result, Some(CalcValue::number(84.)));
        assert_eq!((returns.get(), turns.get()), (1, 0));
        session.set_formula("=Pooled()+Later()");
        let calculation = session.calculate();
        assert_eq!(calculation.result, Some(CalcValue::number(46.)));
        assert_eq!((returns.get(), turns.get()), (2, 3));
        // The owner goes with the session's replacement only when the host moves it.
        session.set_external_returns(None);
        session.set_formula("=Later()");
        let calculation = session.calculate();
        assert!(calculation.result.is_none() && !calculation.diagnostics.is_empty());
    }

    /// A pool call that never finishes ends the calculation with the owner's reason.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_call_that_does_not_finish_fails_the_calculation_with_the_owners_reason() {
        let (mut session, _, _) = session(Some(1));
        session.set_formula("=Pooled()");
        let calculation = session.calculate();
        assert!(calculation.result.is_none());
        let text: Vec<String> = calculation
            .diagnostics
            .iter()
            .map(|d| format!("{d:?}"))
            .collect();
        assert!(text.iter().any(|t| t.contains("took too long")), "{text:?}");
        // The session is usable afterwards.
        session.set_formula("=1+1");
        assert_eq!(session.calculate().result, Some(CalcValue::number(2.)));
    }
}
