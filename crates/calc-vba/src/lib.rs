//! VBA adapter outside calc-func and calc-fml. The interpreter runs on desktop and wasm;
//! every call has fresh VBA state, private ByRef inputs and no external host services.

use calc_func::value::{ArrayShape, CalcArray, CalcValue, CoreRef, ExcelText, WorksheetErrorCode};
use calc_func::{UserFunction, UserFunctions};
use oxvba_embed::{Backend, Module, Value};
use std::{fmt, rc::Rc};

pub const DEFAULT_STEP_BUDGET: u64 = 100_000;
pub const EXAMPLE_MODULE: &str =
    "Public Function Add2(a As Double, b As Double) As Double\n    Add2 = a + b\nEnd Function\n";

/// Compile a module and construct replacement registrations before the host publishes them.
/// Both bare and module-qualified public Function names are registered; Subs remain private
/// to VBA. References have already become scalar values or rectangular value arrays in calc-fml.
pub fn load_module(name: &str, source: &str, max_steps: u64) -> Result<UserFunctions, String> {
    let module = Rc::new(Module::load(name, source).map_err(|e| e.to_string())?);
    let mut functions = UserFunctions::new();
    for metadata in module.functions() {
        if metadata.parameters.iter().any(|p| p.param_array) {
            return Err(format!(
                "{}: ParamArray calls are not supported yet",
                metadata.name
            ));
        }
        let function = Rc::new(VbaFunction {
            module: module.clone(),
            name: metadata.qualified_name.clone(),
            max_steps,
        });
        functions.register(&metadata.name, function.clone())?;
        functions.register(&metadata.qualified_name, function)?;
    }
    Ok(functions)
}

struct VbaFunction {
    module: Rc<Module>,
    name: String,
    max_steps: u64,
}

impl fmt::Debug for VbaFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VbaFunction")
            .field("name", &self.name)
            .field("max_steps", &self.max_steps)
            .finish()
    }
}

impl UserFunction for VbaFunction {
    fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String> {
        let args = arguments
            .iter()
            .map(to_vba)
            .collect::<Result<Vec<_>, _>>()?;
        let session = self
            .module
            .prepare(Backend::Interpreter)
            .map_err(|e| e.to_string())?;
        from_vba(
            session
                .call(&self.name, &args, self.max_steps)
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
        assert!(to_vba(&CalcValue::missing()).is_err());
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
}
