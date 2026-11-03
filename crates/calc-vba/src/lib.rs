//! VBA adapter outside calc-func and calc-fml. A loaded module retains one project session.
//! Native builds prefer JIT; wasm and explicitly declined native modules use the interpreter.

use calc_fml::CallableTable as UserFunctions;
use calc_func::value::{ArrayShape, CalcArray, CalcValue, CoreRef, ExcelText, WorksheetErrorCode};
use calc_func::value::{CellKey, ReferenceLike};
use calc_func::{UserFunction, UserFunctionContext};
pub use oxvba_embed::Backend;
mod runtime;
pub mod events;
use runtime::{CodeCache, Module, OwnedSession, Range, SpreadsheetContext, Value};
use std::{cell::RefCell, fmt, rc::Rc};

thread_local! {
    static CODE_CACHE: RefCell<CodeCache> = RefCell::new(CodeCache::new(16,8*1024*1024));
}

/// Process-local diagnostics for the current thread's bounded immutable-code cache.
pub fn code_cache_stats() -> oxvba_embed::CacheStats {
    CODE_CACHE.with(|cache| cache.borrow().stats())
}
/// Release cached code owners. Live registrations retain code and project state safely.
pub fn clear_code_cache() {
    CODE_CACHE.with(|cache| cache.borrow_mut().clear());
}
fn prepare_module(module: Module, backend: Backend) -> Result<OwnedSession, runtime::Error> {
    CODE_CACHE.with(|cache| {
        cache
            .try_borrow_mut()
            .map_err(|_| {
                runtime::Error::Backend("code cache is already preparing a module".into())
            })?
            .prepare(module, backend)
    })
}

pub const DEFAULT_STEP_BUDGET: u64 = 100_000;
pub const EXAMPLE_MODULE: &str =
    "Public Function Add2(a As Double, b As Double) As Double\n    Add2 = a + b\nEnd Function\n";

/// Compile a module and construct replacement registrations before the host publishes them.
/// Both bare and module-qualified public Function names are registered; Subs remain private
/// to VBA. Single rectangular references remain live Range objects during each host call.
pub fn load_module(name: &str, source: &str, max_steps: u64) -> Result<UserFunctions, String> {
    load_module_selected(name, source, max_steps, None)
}

/// Select an exact backend for parity tests and hosts that require one. An explicit JIT
/// decline is returned to the caller; execution never switches backend after it starts.
pub fn load_module_with_backend(
    name: &str,
    source: &str,
    max_steps: u64,
    backend: Backend,
) -> Result<UserFunctions, String> {
    load_module_selected(name, source, max_steps, Some(backend))
}

fn load_module_selected(
    name: &str,
    source: &str,
    max_steps: u64,
    backend: Option<Backend>,
) -> Result<UserFunctions, String> {
    let module = Module::load_spreadsheet(name, source).map_err(|e| e.to_string())?;
    Ok(prepare_project(module, max_steps, backend)?.functions)
}

fn prepare_project(module: Module, max_steps: u64, backend: Option<Backend>) -> Result<events::LoadedProject, String> {
    let metadata = module.functions().cloned().collect::<Vec<_>>();
    let session = if let Some(backend) = backend {
        prepare_module(module, backend).map_err(|e| e.to_string())?
    } else {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Admission happens before any VBA code executes. A declined image is compiled
            // afresh for the interpreter; runtime errors and exhausted budgets never retry.
            match prepare_module(module.clone(), Backend::Jit) {
                Ok(session) => session,
                Err(runtime::Error::UnsupportedBackend(_) | runtime::Error::BackendUnavailable) => {
                    prepare_module(module, Backend::Interpreter).map_err(|e| e.to_string())?
                }
                Err(error) => return Err(error.to_string()),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            prepare_module(module, Backend::Interpreter).map_err(|e| e.to_string())?
        }
    };
    let session = Rc::new(session);
    let sheets = Rc::new(RefCell::new(Sheets::default()));
    let mut functions = UserFunctions::new();
    for (index, metadata) in metadata.into_iter().enumerate() {
        let function = Rc::new(VbaFunction {
            session: session.clone(),
            sheets: sheets.clone(),
            name: metadata.qualified_name.clone(),
            index,
            max_steps,
        });
        functions.register(&metadata.name, function.clone())?;
        functions.register(&metadata.qualified_name, function)?;
    }
    Ok(events::LoadedProject { session, functions })
}

struct VbaFunction {
    session: Rc<OwnedSession>,
    sheets: Rc<RefCell<Sheets>>,
    name: String,
    index: usize,
    max_steps: u64,
}

impl fmt::Debug for VbaFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VbaFunction")
            .field("name", &self.name)
            .field("max_steps", &self.max_steps)
            .field("backend", &self.session.backend())
            .finish()
    }
}

struct VbaActivation {
    activation: Box<dyn calcvba_host_api::Invocation>,
    session: Rc<OwnedSession>,
    sheets: Rc<RefCell<Sheets>>,
}
impl fmt::Debug for VbaActivation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VbaActivation")
            .field("id", &self.activation.frame().id)
            .finish_non_exhaustive()
    }
}
impl calc_func::UserFunctionInvocation for VbaActivation {
    fn resume(
        &mut self,
        context: &mut dyn UserFunctionContext,
        quantum: u64,
    ) -> Result<calc_func::UserFunctionStep, String> {
        let bridge = ContextBridge {
            context: RefCell::new(context),
            sheets: self.sheets.clone(),
        };
        self.session
            .in_context(&bridge, || {
                self.activation
                    .resume(quantum)
                    .map_err(|e| runtime::Error::Backend(e.to_string()))
            })
            .map_err(|e| e.to_string())
            .and_then(|progress| match progress {
                calcvba_host_api::InvocationProgress::Suspended => {
                    Ok(calc_func::UserFunctionStep::Suspended)
                }
                calcvba_host_api::InvocationProgress::Returned(value) => {
                    let value = match value {
                        calcvba_host_api::OwnedValue::Long(n) => CalcValue::number(f64::from(n)),
                        calcvba_host_api::OwnedValue::Double(n) if n.is_finite() => {
                            CalcValue::number(n)
                        }
                        _ => return Err("unsupported typed VBA result".into()),
                    };
                    Ok(calc_func::UserFunctionStep::Complete(value))
                }
            })
    }
}
impl UserFunction for VbaFunction {
    fn state_lane(&self) -> Option<u64> {
        Some(Rc::as_ptr(&self.session) as usize as u64)
    }
    fn reevaluation_policy(&self) -> Option<calc_func::UserReevaluationReason> {
        Some(calc_func::UserReevaluationReason::ProvisionalArguments)
    }
    fn cached_reads(&self) -> bool {
        true
    }
    fn begin_call(
        &self,
        arguments: &[CalcValue],
        frame: calc_func::UserInvocationFrame,
        context: &mut dyn UserFunctionContext,
    ) -> Result<Option<Box<dyn calc_func::UserFunctionInvocation>>, String> {
        let bridge = ContextBridge {
            context: RefCell::new(context),
            sheets: self.sheets.clone(),
        };
        let arguments = arguments
            .iter()
            .map(|value| bridge.argument(value))
            .collect::<Result<Vec<_>, _>>()?;
        let frame = calcvba_host_api::InvocationFrame::new(frame.id, frame.parent, frame.read_view);
        self.session
            .in_context(&bridge, || {
                self.session
                    .begin_typed(self.index, &arguments, frame, self.max_steps)
            })
            .map_err(|e| e.to_string())
            .map(|activation| {
                activation.map(|activation| {
                    Box::new(VbaActivation {
                        activation,
                        session: self.session.clone(),
                        sheets: self.sheets.clone(),
                    }) as Box<dyn calc_func::UserFunctionInvocation>
                })
            })
    }

    fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String> {
        let args = arguments
            .iter()
            .map(to_vba)
            .collect::<Result<Vec<_>, _>>()?;
        from_vba(
            self.session
                .call(self.index, &args, self.max_steps)
                .map_err(|e| e.to_string())?,
        )
    }
    fn call_with_context(
        &self,
        arguments: &[CalcValue],
        context: &mut dyn UserFunctionContext,
    ) -> Result<CalcValue, String> {
        let mut bridge = ContextBridge {
            context: RefCell::new(context),
            sheets: self.sheets.clone(),
        };
        let arguments = arguments
            .iter()
            .map(|value| bridge.argument(value))
            .collect::<Result<Vec<_>, _>>()?;
        from_vba(
            self.session
                .call_with_context(self.index, &arguments, self.max_steps, &mut bridge)
                .map_err(|e| e.to_string())?,
        )
    }
}

fn to_vba(value: &CalcValue) -> Result<Value, String> {
    if value.callable_value().is_some() {
        return Err("a VBA argument cannot be a LAMBDA".into());
    }
    Ok(match value.core_ref() {
        CoreRef::Empty => Value::Empty,
        CoreRef::Missing => Value::Missing,
        CoreRef::Number(n) if n.is_finite() => Value::Number(n),
        CoreRef::Logical(b) => Value::Boolean(b),
        CoreRef::Text(t) => Value::Text(
            String::from_utf16(t.utf16_code_units()).map_err(|_| "invalid UTF-16 argument")?,
        ),
        CoreRef::Error(e) => {
            Value::Error(error_number(e).ok_or("unsupported worksheet error argument")?)
        }
        CoreRef::Array(a) => {
            let shape = a.shape();
            let mut cells = Vec::with_capacity(a.cell_count());
            for row in 0..shape.rows {
                for col in 0..shape.cols {
                    let cell = a.get(row, col).expect("inside array");
                    if cell.as_array().is_some() {
                        return Err("nested VBA arrays are not supported".into());
                    }
                    cells.push(to_vba(cell)?);
                }
            }
            Value::Array {
                rows: u32::try_from(shape.rows).map_err(|_| "array too tall")?,
                columns: u32::try_from(shape.cols).map_err(|_| "array too wide")?,
                cells,
            }
        }
        _ => {
            return Err("unsupported VBA argument (missing, reference or nonfinite number)".into());
        }
    })
}

fn from_vba(value: Value) -> Result<CalcValue, String> {
    Ok(match value {
        Value::Empty => CalcValue::empty(),
        Value::Missing => return Err("a VBA function returned a missing argument".into()),
        Value::Range(_) => {
            return Err("a VBA Range result was not materialized by the spreadsheet host".into());
        }
        Value::Number(n) if n.is_finite() => CalcValue::number(n),
        Value::Number(_) => return Err("nonfinite VBA result".into()),
        Value::Boolean(b) => CalcValue::logical(b),
        Value::Text(t) => {
            if t.encode_utf16().count() > 32767 {
                return Err("VBA result exceeds the cell text limit".into());
            }
            CalcValue::text(ExcelText::from_utf16_code_units(t.encode_utf16().collect()))
        }
        Value::Error(n) => CalcValue::error(
            WorksheetErrorCode::all()
                .into_iter()
                .find(|e| error_number(*e) == Some(n))
                .ok_or_else(|| format!("unknown VBA worksheet error {n}"))?,
        ),
        Value::Array {
            rows,
            columns,
            cells,
        } => {
            let cells = cells
                .into_iter()
                .map(from_vba)
                .collect::<Result<Vec<_>, _>>()?;
            CalcValue::array(
                CalcArray::new(
                    ArrayShape {
                        rows: rows as usize,
                        cols: columns as usize,
                    },
                    cells,
                )
                .ok_or("malformed VBA array result")?,
            )
        }
    })
}

// Same Automation error numbers as calc-oracle-excel's value conversion. Busy has no admitted
// Automation code in that contract and is refused rather than assigned an invented number.
fn error_number(error: WorksheetErrorCode) -> Option<i32> {
    use WorksheetErrorCode::*;
    Some(match error {
        Null => 2000,
        Div0 => 2007,
        Value => 2015,
        Ref => 2023,
        Name => 2029,
        Num => 2036,
        NA => 2042,
        GettingData => 2043,
        Spill => 2045,
        Connect => 2046,
        Blocked => 2047,
        Field => 2049,
        Calc => 2050,
        Busy => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn typed_activations_resume_once_and_keep_old_code_and_state_across_replacement() {
        struct Context;
        impl UserFunctionContext for Context {
            fn caller(&self) -> Option<CellKey> {
                None
            }
            fn refuse(&mut self, reason: &'static str) -> String {
                reason.into()
            }
            fn values_of(&mut self, _: &ReferenceLike) -> Result<CalcValue, String> {
                Err("unexpected read".into())
            }
            fn worksheet_function(
                &mut self,
                _: &str,
                _: &[CalcValue],
            ) -> Result<CalcValue, String> {
                Err("unexpected callback".into())
            }
        }
        let source = "Private entries As Long\nPublic Function Counted(ByVal count As Long) As Double\n Dim total As Double\n entries=entries+1\n Do While count > 0\n total=total+1#\n count=count-1\n Loop\n Counted=total+entries\nEnd Function\nPublic Function EntryCount() As Long\n EntryCount=entries\nEnd Function\n";
        let mut backends = vec![Backend::Interpreter];
        #[cfg(not(target_arch = "wasm32"))]
        backends.push(Backend::Jit);
        for backend in backends {
            let many_globals = (0..130)
                .map(|n| format!("Private unused{n} As Long\n"))
                .collect::<String>()
                + "Public Function Scalar() As Double\n Scalar=42#\nEnd Function\n";
            let fallback = load_module_with_backend("Wide", &many_globals, 1000, backend).unwrap();
            let function = fallback
                .resolve(fallback.bind(None, "Scalar").unwrap())
                .unwrap();
            assert!(
                function
                    .begin_call(
                        &[],
                        calc_func::UserInvocationFrame::new(None, Rc::new(())),
                        &mut Context
                    )
                    .unwrap()
                    .is_none(),
                "pure layout is insufficient for an owning activation"
            );
            assert_eq!(
                function.call_with_context(&[], &mut Context).unwrap(),
                CalcValue::number(42.0)
            );
            let mut table = load_module_with_backend("Resume", source, 10_000, backend).unwrap();
            let handle = table.bind(None, "Counted").unwrap();
            let old_count = table
                .resolve(table.bind(None, "EntryCount").unwrap())
                .unwrap()
                .clone();
            let function = table.resolve(handle).unwrap().clone();
            let frame = calc_func::UserInvocationFrame {
                id: 17,
                parent: Some(9),
                read_view: Rc::new(40u64),
                formula_entry: false,
            };
            let weak = Rc::downgrade(&frame.read_view);
            let mut activation = function
                .begin_call(&[CalcValue::number(20.0)], frame, &mut Context)
                .unwrap()
                .expect("typed signature");
            assert!(matches!(
                activation.resume(&mut Context, 0).unwrap(),
                calc_func::UserFunctionStep::Suspended
            ));
            for _ in 0..5 {
                assert!(matches!(
                    activation.resume(&mut Context, 1).unwrap(),
                    calc_func::UserFunctionStep::Suspended
                ));
            }
            let replacement=load_module_with_backend("Resume","Public Function Counted(ByVal text As String) As String\n Counted=text\nEnd Function\n",10_000,backend).unwrap();
            table.replace(replacement);
            assert!(table.resolve(handle).is_none());
            assert_eq!(
                table.bind(None, "Counted").unwrap().generation,
                handle.generation + 1
            );
            let mut yields = 0;
            let value = loop {
                match activation.resume(&mut Context, 1).unwrap() {
                    calc_func::UserFunctionStep::Awaiting { .. } => {
                        panic!("VBA fixture has no external provider")
                    }
                    calc_func::UserFunctionStep::Suspended => {
                        yields += 1;
                        assert!(yields < 10_000);
                    }
                    calc_func::UserFunctionStep::Complete(value) => break value,
                }
            };
            assert_eq!(value, CalcValue::number(21.0));
            assert!(yields > 20);
            assert_eq!(old_count.call(&[]).unwrap(), CalcValue::number(1.0));
            assert!(activation.resume(&mut Context, 1).is_err());
            assert!(weak.upgrade().is_some());
            drop(activation);
            assert!(weak.upgrade().is_none());
            assert_eq!(
                table
                    .call(
                        "Counted",
                        &[CalcValue::text(ExcelText::from_utf16_code_units(
                            "new".encode_utf16().collect()
                        ))]
                    )
                    .unwrap(),
                CalcValue::text(ExcelText::from_utf16_code_units(
                    "new".encode_utf16().collect()
                ))
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_adapter_preserves_supported_error_codes_and_rejects_invalid_values() {
        for error in WorksheetErrorCode::all() {
            let value = CalcValue::error(error);
            if error == WorksheetErrorCode::Busy {
                assert!(to_vba(&value).is_err());
            } else {
                assert_eq!(from_vba(to_vba(&value).unwrap()).unwrap(), value);
            }
        }
        let bad_text = CalcValue::text(ExcelText::from_utf16_code_units(vec![0xd800]));
        assert!(to_vba(&bad_text).is_err());
        assert_eq!(to_vba(&CalcValue::missing()).unwrap(), Value::Missing);
        assert!(from_vba(Value::Error(12345)).is_err());
        assert!(from_vba(Value::Number(f64::INFINITY)).is_err());
        assert!(from_vba(Value::Text("x".repeat(32768))).is_err());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn only_public_functions_are_registered_and_no_main_runs() {
        let source = format!(
            "{}\nPrivate Function Hidden()\n Hidden = 1\nEnd Function\nPublic Sub Main()\n Do\n Loop\nEnd Sub\n",
            EXAMPLE_MODULE
        );
        let functions = load_module("Math", &source, 1000).unwrap();
        assert_eq!(functions.names().collect::<Vec<_>>(), ["ADD2", "MATH.ADD2"]);
        assert_eq!(
            functions
                .call("aDd2", &[CalcValue::number(40.0), CalcValue::number(2.0)])
                .unwrap(),
            CalcValue::number(42.0)
        );
        assert_eq!(
            functions.call("Hidden", &[]).unwrap(),
            CalcValue::error(WorksheetErrorCode::Name)
        );
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn explicit_backends_preserve_optional_calls_and_isolated_module_state() {
        let mut backends = vec![Backend::Interpreter];
        #[cfg(not(target_arch = "wasm32"))]
        backends.push(Backend::Jit);
        let source = "Private n As Long\nPublic Function Counter() As Long\n n = n + 1\n Counter = n\nEnd Function\nPublic Function Defaults(Optional x As Long = 7, Optional y As Long = 2) As Long\n Defaults = x + y\nEnd Function\nPublic Function Pack(ParamArray x() As Variant) As Long\n Pack = UBound(x) - LBound(x) + 1\nEnd Function\n";
        for backend in backends {
            let functions = load_module_with_backend("State", source, 1000, backend).unwrap();
            let other = load_module_with_backend("State", source, 1000, backend).unwrap();
            assert_eq!(
                functions.call("Counter", &[]).unwrap(),
                CalcValue::number(1.0)
            );
            assert_eq!(
                functions.call("State.Counter", &[]).unwrap(),
                CalcValue::number(2.0)
            );
            assert_eq!(other.call("Counter", &[]).unwrap(), CalcValue::number(1.0));
            assert_eq!(
                functions.call("Defaults", &[]).unwrap(),
                CalcValue::number(9.0)
            );
            assert_eq!(
                functions
                    .call("Defaults", &[CalcValue::missing(), CalcValue::number(3.0)])
                    .unwrap(),
                CalcValue::number(10.0)
            );
            assert_eq!(functions.call("Pack", &[]).unwrap(), CalcValue::number(0.0));
            assert_eq!(
                functions
                    .call("Pack", &[CalcValue::number(1.0), CalcValue::number(2.0)])
                    .unwrap(),
                CalcValue::number(2.0)
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn volatility_requests_belong_to_the_invocation_context() {
        #[derive(Default)]
        struct Context(Vec<bool>);
        impl UserFunctionContext for Context {
            fn caller(&self) -> Option<CellKey> {
                None
            }
            fn set_volatile(&mut self, value: bool) {
                self.0.push(value);
            }
            fn refuse(&mut self, reason: &'static str) -> String {
                reason.into()
            }
            fn values_of(&mut self, _: &ReferenceLike) -> Result<CalcValue, String> {
                Err("no sheet".into())
            }
            fn worksheet_function(
                &mut self,
                _: &str,
                _: &[CalcValue],
            ) -> Result<CalcValue, String> {
                Err("no callback".into())
            }
        }
        let source = "Private n As Long\nPublic Function Toggle(ByVal flag As Boolean) As Long\n Application.Volatile flag\n n = n + 1\n Toggle = n\nEnd Function\nPublic Function Conditional(ByVal flag As Boolean) As Long\n If flag Then Application.Volatile\n Conditional = n\nEnd Function\nPublic Function Nested() As Long\n Mark\n Nested = n\nEnd Function\nPrivate Sub Mark()\n Application.Volatile True\n Application.Volatile False\nEnd Sub\n";
        let mut backends = vec![Backend::Interpreter];
        #[cfg(not(target_arch = "wasm32"))]
        backends.push(Backend::Jit);
        for backend in backends {
            let functions = load_module_with_backend("Effects", source, 1000, backend).unwrap();
            let mut first = Context::default();
            let mut second = Context::default();
            assert_eq!(
                functions
                    .call_with_context("Toggle", &[CalcValue::logical(true)], &mut first)
                    .unwrap(),
                CalcValue::number(1.0)
            );
            assert_eq!(
                functions
                    .call_with_context("Toggle", &[CalcValue::logical(false)], &mut second)
                    .unwrap(),
                CalcValue::number(2.0)
            );
            assert_eq!(first.0, [true]);
            assert_eq!(second.0, [false]);
            let mut conditional = Context::default();
            functions
                .call_with_context(
                    "Conditional",
                    &[CalcValue::logical(false)],
                    &mut conditional,
                )
                .unwrap();
            assert!(conditional.0.is_empty());
            let mut nested = Context::default();
            functions
                .call_with_context("Nested", &[], &mut nested)
                .unwrap();
            assert_eq!(nested.0, [true, false]);
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn selected_backends_start_once_and_preserve_rich_errors() {
        let source = "Private n As Long\nPublic Function RichError()\n n = n + 1\n On Error GoTo Failed\n Err.Raise 5, \"probe\", \"detail\"\n Exit Function\nFailed:\n RichError = CStr(n) & \":\" & Err.Source & \":\" & Err.Description\nEnd Function\n";
        let selected = load_module("Fallback", source, 1000).unwrap();
        let interpreter =
            load_module_with_backend("Fallback", source, 1000, Backend::Interpreter).unwrap();
        #[cfg(not(target_arch = "wasm32"))]
        let jit = load_module_with_backend("Fallback", source, 1000, Backend::Jit).unwrap();
        #[cfg(not(target_arch = "wasm32"))]
        assert!(format!("{selected:?}").contains("Jit"));
        #[cfg(target_arch = "wasm32")]
        assert!(format!("{selected:?}").contains("Interpreter"));
        for n in 1..=2 {
            let expected = CalcValue::text(ExcelText::from_utf16_code_units(
                format!("{n}:probe:detail").encode_utf16().collect(),
            ));
            assert_eq!(selected.call("RichError", &[]).unwrap(), expected);
            assert_eq!(interpreter.call("RichError", &[]).unwrap(), expected);
            #[cfg(not(target_arch = "wasm32"))]
            assert_eq!(jit.call("RichError", &[]).unwrap(), expected);
        }
    }
}

#[derive(Default)]
struct Sheets(Vec<calc_func::StoredReference>);
impl Sheets {
    fn insert(&mut self, reference: calc_func::StoredReference) -> Range {
        let id = self
            .0
            .iter()
            .position(|held| *held == reference)
            .unwrap_or_else(|| {
                self.0.push(reference);
                self.0.len() - 1
            });
        Range { id: id as u64 }
    }
    fn get(&self, range: Range) -> Result<calc_func::StoredReference, runtime::ContextError> {
        self.0
            .get(
                usize::try_from(range.id).map_err(|_| {
                    runtime::ContextError::Vba(424, "invalid Range identity".into())
                })?,
            )
            .cloned()
            .ok_or_else(|| runtime::ContextError::Vba(424, "unknown Range identity".into()))
    }
}
fn range_metadata(reference: &ReferenceLike) -> runtime::RangeInfo {
    let first = reference.areas()[0];
    fn cell(at: CellKey) -> String {
        let mut col = at.column();
        let mut name = Vec::new();
        while col > 0 {
            col -= 1;
            name.push(b'A' + (col % 26) as u8);
            col /= 26;
        }
        name.reverse();
        format!(
            "${}${}",
            String::from_utf8(name).expect("ASCII column"),
            at.row()
        )
    }
    let address = reference
        .areas()
        .iter()
        .map(|area| {
            if area.cell_count() == 1 {
                cell(area.first())
            } else {
                format!("{}:{}", cell(area.first()), cell(area.last()))
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    runtime::RangeInfo {
        row: first.first().row(),
        column: first.first().column(),
        rows: first.row_count(),
        columns: first.column_count(),
        cells: reference.areas().iter().map(|area| area.cell_count()).sum(),
        areas: reference.area_count() as u32,
        address,
    }
}
struct ContextBridge<'a> {
    context: RefCell<&'a mut dyn UserFunctionContext>,
    sheets: Rc<RefCell<Sheets>>,
}
impl ContextBridge<'_> {
    fn refuse(&self, reason: &'static str) -> String {
        self.context
            .try_borrow_mut()
            .map(|mut context| context.refuse(reason))
            .unwrap_or_else(|_| "reentrant worksheet capability refusal".into())
    }
    fn range(&self, reference: &ReferenceLike) -> Result<Range, runtime::ContextError> {
        if reference.spans_sheets() {
            return Err(self.refuse("a VBA Range spanning worksheets").into());
        }
        let retained = {
            self.context
                .try_borrow_mut()
                .map_err(|_| "reentrant reference admission")?
                .retain_reference(reference)
        };
        let handle = retained.map_err(|error| self.reference_error(error))?;
        Ok(self.sheets.borrow_mut().insert(handle))
    }
    fn reference_error(&self, error: calc_func::ReferenceHandleError) -> runtime::ContextError {
        match error {
            calc_func::ReferenceHandleError::NotModelledEdit => self
                .context
                .try_borrow_mut()
                .map(|mut context| {
                    context.refuse_by(
                        "a stored Range split across worksheets by an edit",
                        "dc-hsa5.4",
                    )
                })
                .unwrap_or_else(|_| "reentrant reference resolution".into())
                .into(),
            calc_func::ReferenceHandleError::ExpiredView => self
                .context
                .try_borrow_mut()
                .map(|mut context| {
                    context.refuse_by(
                        "creating a stored Range after its invocation view expired",
                        "dc-hsa5.4",
                    )
                })
                .unwrap_or_else(|_| "reentrant reference admission".into())
                .into(),
            _ => runtime::ContextError::Vba(424, format!("stored Range is unavailable: {error}")),
        }
    }
    fn reference(&self, range: Range) -> Result<ReferenceLike, runtime::ContextError> {
        let handle = self.sheets.borrow().get(range)?;
        let resolved = {
            self.context
                .try_borrow_mut()
                .map_err(|_| "reentrant Range resolution")?
                .resolve_reference(&handle)
        };
        resolved.map_err(|error| self.reference_error(error))
    }
    fn argument(&self, value: &CalcValue) -> Result<Value, String> {
        if let Some(reference) = value.as_reference() {
            self.range(reference)
                .map(Value::Range)
                .map_err(|error| error.to_string())
        } else {
            to_vba(value)
                .map_err(|_| self.refuse("a VBA argument outside the supported value surface"))
        }
    }
    fn value(&self, value: &Value) -> Result<CalcValue, runtime::ContextError> {
        match value {
            Value::Range(range) => Ok(CalcValue::reference(self.reference(*range)?)),
            value => from_vba(value.clone()).map_err(Into::into),
        }
    }
}
impl SpreadsheetContext for ContextBridge<'_> {
    fn refuse(&self, reason: &'static str) -> String {
        ContextBridge::refuse(self, reason)
    }
    fn caller(&self) -> Option<Range> {
        let caller = self.context.try_borrow().ok()?.caller()?;
        self.range(&ReferenceLike::cell(caller)).ok()
    }
    fn metadata(&self, range: Range) -> Result<runtime::RangeInfo, runtime::ContextError> {
        Ok(range_metadata(&self.reference(range)?))
    }
    fn cell(&self, range: Range, row: u32, column: u32) -> Result<Range, runtime::ContextError> {
        let reference = self.reference(range)?;
        let first = reference.areas()[0].first();
        let row = first
            .row()
            .checked_add(row - 1)
            .ok_or_else(|| runtime::ContextError::Vba(1004, "Range row overflow".into()))?;
        let col = first
            .column()
            .checked_add(column - 1)
            .ok_or_else(|| runtime::ContextError::Vba(1004, "Range column overflow".into()))?;
        let cell = CellKey::new(first.sheet(), row, col)
            .map_err(|error| runtime::ContextError::Vba(1004, error.to_string()))?;
        self.range(&ReferenceLike::cell(cell))
    }
    fn area(&self, range: Range, index: u32) -> Result<Range, runtime::ContextError> {
        let reference = self.reference(range)?;
        let area = *reference
            .areas()
            .get(index.saturating_sub(1) as usize)
            .filter(|_| index > 0)
            .ok_or_else(|| {
                runtime::ContextError::Vba(1004, "Range area index out of bounds".into())
            })?;
        self.range(&ReferenceLike::area(area))
    }
    fn read_argument_range(&self, range: Range) -> Result<Value, runtime::ContextError> {
        let reference = self.reference(range)?;
        let value = self
            .context
            .try_borrow_mut()
            .map_err(|_| "reentrant argument read")?
            .argument_values_of(&reference)?;
        to_vba(&value).map_err(|_| {
            self.refuse("a VBA scalar argument outside the supported value surface")
                .into()
        })
    }
    fn read_range(&self, range: Range) -> Result<Value, runtime::ContextError> {
        let reference = self.reference(range)?;
        if reference.area_count() != 1 {
            return Err(self
                .refuse("Value2 of a multi-area Range (dc-hsa5.4)")
                .into());
        }
        let value = self
            .context
            .try_borrow_mut()
            .map_err(|_| "reentrant worksheet read")?
            .values_of(&reference)?;
        to_vba(&value).map_err(|_| {
            self.refuse("a Range.Value2 outside the supported value surface")
                .into()
        })
    }
    fn worksheet_function(
        &self,
        name: &str,
        args: &[Value],
    ) -> Result<Value, runtime::ContextError> {
        let name = name.to_ascii_uppercase();
        let valid = match name.as_str() {
            "SUM" | "MIN" | "MAX" | "AVERAGE" => (1..=30).contains(&args.len()),
            "MATCH" => (2..=3).contains(&args.len()),
            _ => {
                return Err(self
                    .refuse("a worksheet callback outside the measured VBA host surface")
                    .into());
            }
        };
        if !valid {
            return Err(self
                .refuse("a worksheet callback argument count outside the supported host surface")
                .into());
        }
        let arguments = args
            .iter()
            .map(|value| self.value(value))
            .collect::<Result<Vec<_>, _>>()?;
        if arguments.iter().any(|value| {
            value
                .as_reference()
                .is_some_and(|reference| reference.area_count() != 1)
        }) {
            return Err(self
                .refuse("a worksheet callback over multiple Range areas")
                .into());
        }
        let value = self
            .context
            .try_borrow_mut()
            .map_err(|_| "reentrant worksheet callback")?
            .worksheet_function(&name, &arguments)?;
        if value.as_reference().is_some() {
            return Err(self.refuse("a reference-valued worksheet callback").into());
        }
        to_vba(&value).map_err(|_| {
            self.refuse("a worksheet callback result outside the supported value surface")
                .into()
        })
    }
    fn set_volatile(&self, value: bool) -> Result<(), runtime::ContextError> {
        self.context
            .try_borrow_mut()
            .map_err(|_| "reentrant volatility request")?
            .set_volatile(value);
        Ok(())
    }
}
