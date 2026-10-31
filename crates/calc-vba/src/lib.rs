//! VBA adapter outside calc-func and calc-fml. A loaded module retains one project session.
//! Native builds prefer JIT; wasm and explicitly declined native modules use the interpreter.

use calc_func::value::{ArrayShape, CalcArray, CalcValue, CoreRef, ExcelText, WorksheetErrorCode};
use calc_func::value::{CellKey, GridArea, ReferenceLike, SheetId};
use calc_func::{UserFunction, UserFunctionContext, UserFunctions};
pub use oxvba_embed::Backend;
use oxvba_embed::{CodeCache, Module, OwnedSession, Range, SpreadsheetContext, Value};
use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
};

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
fn prepare_module(module: Module, backend: Backend) -> Result<OwnedSession, oxvba_embed::Error> {
    CODE_CACHE.with(|cache| {
        cache
            .try_borrow_mut()
            .map_err(|_| {
                oxvba_embed::Error::Backend("code cache is already preparing a module".into())
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
                Err(
                    oxvba_embed::Error::UnsupportedBackend(_)
                    | oxvba_embed::Error::BackendUnavailable,
                ) => prepare_module(module, Backend::Interpreter).map_err(|e| e.to_string())?,
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
    for metadata in metadata {
        let function = Rc::new(VbaFunction {
            session: session.clone(),
            sheets: sheets.clone(),
            volatile: Cell::new(false),
            name: metadata.qualified_name.clone(),
            max_steps,
        });
        functions.register(&metadata.name, function.clone())?;
        functions.register(&metadata.qualified_name, function)?;
    }
    Ok(functions)
}

struct VbaFunction {
    session: Rc<OwnedSession>,
    sheets: Rc<RefCell<Sheets>>,
    volatile: Cell<bool>,
    name: String,
    max_steps: u64,
}

impl fmt::Debug for VbaFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VbaFunction")
            .field("name", &self.name)
            .field("max_steps", &self.max_steps)
            .field("backend", &self.session.backend())
            .field("volatile_requested", &self.volatile.get())
            .finish()
    }
}

impl UserFunction for VbaFunction {
    fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String> {
        let args = arguments
            .iter()
            .map(to_vba)
            .collect::<Result<Vec<_>, _>>()?;
        from_vba(
            self.session
                .call(&self.name, &args, self.max_steps)
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
            volatile: &self.volatile,
        };
        let arguments = arguments
            .iter()
            .map(|value| bridge.argument(value))
            .collect::<Result<Vec<_>, _>>()?;
        from_vba(
            self.session
                .call_with_context(&self.name, &arguments, self.max_steps, &mut bridge)
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
    fn admission_fallback_starts_once_and_preserves_rich_errors_in_the_interpreter() {
        let source = "Private n As Long\nPublic Function RichError()\n n = n + 1\n On Error GoTo Failed\n Err.Raise 5, \"probe\", \"detail\"\n Exit Function\nFailed:\n RichError = CStr(n) & \":\" & Err.Source & \":\" & Err.Description\nEnd Function\n";
        let selected = load_module("Fallback", source, 1000).unwrap();
        let interpreter =
            load_module_with_backend("Fallback", source, 1000, Backend::Interpreter).unwrap();
        #[cfg(not(target_arch = "wasm32"))]
        assert!(load_module_with_backend("Fallback", source, 1000, Backend::Jit).is_err());
        assert!(format!("{selected:?}").contains("Interpreter"));
        for n in 1..=2 {
            let expected = CalcValue::text(ExcelText::from_utf16_code_units(
                format!("{n}:probe:detail").encode_utf16().collect(),
            ));
            assert_eq!(selected.call("RichError", &[]).unwrap(), expected);
            assert_eq!(interpreter.call("RichError", &[]).unwrap(), expected);
        }
    }
}

#[derive(Default)]
struct Sheets(Vec<SheetId>);
impl Sheets {
    fn range(&mut self, area: GridArea) -> Range {
        let sheet = self
            .0
            .iter()
            .position(|id| *id == area.sheet())
            .unwrap_or_else(|| {
                self.0.push(area.sheet());
                self.0.len() - 1
            });
        Range {
            sheet: sheet as u64,
            row: area.first().row(),
            column: area.first().column(),
            rows: area.row_count(),
            columns: area.column_count(),
        }
    }
    fn reference(&self, range: Range) -> Result<ReferenceLike, String> {
        range.validate()?;
        let sheet = *self
            .0
            .get(usize::try_from(range.sheet).map_err(|_| "invalid host sheet")?)
            .ok_or("unknown host sheet")?;
        let first = CellKey::new(sheet, range.row, range.column).map_err(|e| e.to_string())?;
        let last = CellKey::new(
            sheet,
            range.row + range.rows - 1,
            range.column + range.columns - 1,
        )
        .map_err(|e| e.to_string())?;
        Ok(ReferenceLike::area(
            GridArea::new(first, last).map_err(|e| e.to_string())?,
        ))
    }
}
struct ContextBridge<'a> {
    context: RefCell<&'a mut dyn UserFunctionContext>,
    sheets: Rc<RefCell<Sheets>>,
    volatile: &'a Cell<bool>,
}
impl ContextBridge<'_> {
    fn refuse(&self, reason: &'static str) -> String {
        match self.context.try_borrow_mut() {
            Ok(mut context) => context.refuse(reason),
            Err(_) => "reentrant worksheet capability refusal".into(),
        }
    }
    fn argument(&self, value: &CalcValue) -> Result<Value, String> {
        if let Some(reference) = value.as_reference() {
            let [area] = reference.areas() else {
                return Err(self.refuse("a VBA Range with more than one rectangular area"));
            };
            Ok(Value::Range(self.sheets.borrow_mut().range(*area)))
        } else {
            to_vba(value)
                .map_err(|_| self.refuse("a VBA argument outside the supported value surface"))
        }
    }
    fn value(&self, value: &Value) -> Result<CalcValue, String> {
        match value {
            Value::Range(range) => Ok(CalcValue::reference(
                self.sheets.borrow().reference(*range)?,
            )),
            value => from_vba(value.clone()),
        }
    }
}
impl SpreadsheetContext for ContextBridge<'_> {
    fn refuse(&self, reason: &'static str) -> String {
        ContextBridge::refuse(self, reason)
    }
    fn caller(&self) -> Option<Range> {
        let caller = self.context.try_borrow().ok()?.caller()?;
        Some(self.sheets.borrow_mut().range(GridArea::cell(caller)))
    }
    fn read_range(&self, range: Range) -> Result<Value, String> {
        let reference = self.sheets.borrow().reference(range)?;
        let value = self
            .context
            .try_borrow_mut()
            .map_err(|_| "reentrant worksheet read")?
            .values_of(&reference)?;
        to_vba(&value)
            .map_err(|_| self.refuse("a Range.Value2 outside the supported value surface"))
    }
    fn worksheet_function(&self, name: &str, args: &[Value]) -> Result<Value, String> {
        let name = name.to_ascii_uppercase();
        let valid_arity = match name.as_str() {
            "SUM" | "MIN" | "MAX" | "AVERAGE" => (1..=30).contains(&args.len()),
            "MATCH" => (2..=3).contains(&args.len()),
            _ => {
                return Err(
                    self.refuse("a worksheet callback outside the measured VBA host surface")
                );
            }
        };
        if !valid_arity {
            return Err(self
                .refuse("a worksheet callback argument count outside the supported host surface"));
        }
        let arguments = args
            .iter()
            .map(|value| self.value(value))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| {
                self.refuse("a worksheet callback argument outside the supported value surface")
            })?;
        let value = self
            .context
            .try_borrow_mut()
            .map_err(|_| "reentrant worksheet callback")?
            .worksheet_function(&name, &arguments)?;
        if value.as_reference().is_some() {
            return Err(self.refuse("a reference-valued worksheet callback"));
        }
        to_vba(&value).map_err(|_| {
            self.refuse("a worksheet callback result outside the supported value surface")
        })
    }
    fn set_volatile(&self, value: bool) -> Result<(), String> {
        self.volatile.set(value);
        Ok(())
    }
}
