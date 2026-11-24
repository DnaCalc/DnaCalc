use crate::{
    Dimensionality, ExecLimits, FunctionSpec, FunctionsMetadata, JsArgument, JsEngine, JsHostError,
    JsInvocation,
};
use calc_value::{
    ArrayShape, CalcArray, CalcValue, CoreRef, EXCEL_TEXT_MAX_UTF16_CODE_UNITS, ExcelText,
    MAX_COLUMNS, MAX_ROWS, WorksheetErrorCode as W,
};
use rquickjs::{
    Array, Context, Ctx, Error, Function, IntoJs, Module, Object, Persistent, Runtime, TypedArray,
    Value, context::EvalOptions, function::Args, object::Property,
};
use std::{cell::Cell, rc::Rc, time::Instant};

// The host retains only the returned helpers. The add-in sees the Office association and error APIs.
const OFFICE_API: &str = r##"
(() => {
    const associations = new Map();
    const lookup = associations.get.bind(associations);
    const put = associations.set.bind(associations);
    const has = associations.has.bind(associations);
    const upper = Function.call.bind(String.prototype.toUpperCase);
    let sealed = false;
    const errors = new WeakMap();
    const errorCode = errors.get.bind(errors);
    const rememberError = errors.set.bind(errors);
    const ErrorCode = Object.freeze({
        divisionByZero: "#DIV/0!", invalidName: "#NAME?", invalidNumber: "#NUM!",
        invalidReference: "#REF!", invalidValue: "#VALUE!", notAvailable: "#N/A",
        nullReference: "#NULL!"
    });
    class FunctionError extends Error {
        constructor(code, message) {
            super(message);
            this.code = code;
            rememberError(this, code);
        }
    }
    globalThis.CustomFunctions = Object.freeze({
        Error: FunctionError, ErrorCode,
        associate(id, implementation) {
            if (sealed || typeof id !== "string" || typeof implementation !== "function")
                throw new TypeError("invalid custom function association");
            id = upper(id);
            if (has(id)) throw new TypeError("duplicate custom function association");
            put(id, implementation);
        }
    });
    const fromCharCode = String.fromCharCode;
    const charCodeAt = Function.call.bind(String.prototype.charCodeAt);
    const Units = Uint16Array;
    return {
        lookup, has, errorCode, makeError: code => new FunctionError(code),
        seal: () => { sealed = true; },
        fromUnits(units) {
            let text = "";
            for (let i = 0; i < units.length; ++i) text += fromCharCode(units[i]);
            return text;
        },
        toUnits(text, max) {
            if (text.length > max) throw new RangeError("custom function text exceeds Excel's cell limit");
            const units = new Units(text.length);
            for (let i = 0; i < units.length; ++i) units[i] = charCodeAt(text, i);
            return units;
        }
    };
})()
"##;

/// QuickJS runs one add-in in a native, bounded runtime with no host I/O or module loader.
pub struct QuickJs {
    limits: ExecLimits,
    realm: Option<Realm>,
}

struct Realm {
    // Persistent values must be dropped before their runtime.
    helpers: Persistent<Object<'static>>,
    context: Context,
    _runtime: Runtime,
    deadline: Rc<Cell<Option<Instant>>>,
}

impl Realm {
    fn with_invocation_deadline(
        &self,
        end: Instant,
        invoke: impl FnOnce() -> Result<CalcValue, JsHostError>,
    ) -> Result<CalcValue, JsHostError> {
        self.deadline.set(Some(end));
        let result = invoke();
        self.deadline.set(None);
        result
    }
}

impl std::fmt::Debug for QuickJs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuickJs")
            .field("limits", &self.limits)
            .field("loaded", &self.realm.is_some())
            .finish()
    }
}

impl QuickJs {
    /// Create an unloaded runtime configuration. Loading allocates the isolate.
    pub fn new(limits: ExecLimits) -> Self {
        Self {
            limits,
            realm: None,
        }
    }

    fn realm(&self) -> Result<&Realm, JsHostError> {
        self.realm
            .as_ref()
            .ok_or_else(|| JsHostError::Registration("load the script first".into()))
    }
}

impl JsEngine for QuickJs {
    fn load_module(&mut self, id: &str, source: &str) -> Result<(), JsHostError> {
        if self.realm.is_some() {
            return Err(JsHostError::Registration(
                "one script per add-in runtime".into(),
            ));
        }
        if self.limits.heap_bytes == 0 || source.len() > self.limits.heap_bytes {
            return Err(JsHostError::Registration(
                "script exceeds the JavaScript heap limit".into(),
            ));
        }
        let end = end_time(self.limits)?;
        let runtime = Runtime::new().map_err(registration_error)?;
        runtime.set_memory_limit(self.limits.heap_bytes);
        let deadline = Rc::new(Cell::new(Some(end)));
        let interrupt = Rc::clone(&deadline);
        runtime.set_interrupt_handler(Some(Box::new(move || {
            interrupt.get().is_some_and(|end| Instant::now() >= end)
        })));
        let context = Context::full(&runtime).map_err(registration_error)?;
        let result = context.with(|ctx| {
            let helpers: Object = ctx.eval(OFFICE_API)?;
            if id.ends_with(".mjs") {
                let loaded = Module::evaluate(ctx.clone(), id, source)?;
                match loaded.result::<()>() {
                    Some(result) => result?,
                    None => {
                        return Ok(Err(JsHostError::Deferred {
                            reason: "asynchronous JavaScript module initialization",
                            owner: "dc-26wtr.6",
                        }));
                    }
                }
            } else {
                let mut options = EvalOptions::default();
                options.strict = false;
                options.filename = Some(id.to_owned());
                ctx.eval_with_options::<(), _>(source, options)?;
            }
            helpers.get::<_, Function>("seal")?.call::<_, ()>(())?;
            Ok(Ok(Persistent::save(&ctx, helpers)))
        });
        check_time(end)?;
        let helpers = result.map_err(registration_error)??;
        deadline.set(None);
        self.realm = Some(Realm {
            helpers,
            context,
            _runtime: runtime,
            deadline,
        });
        Ok(())
    }

    fn register_functions(&mut self, metadata: &FunctionsMetadata) -> Result<(), JsHostError> {
        let realm = self.realm()?;
        realm.context.with(|ctx| {
            let helpers = realm
                .helpers
                .clone()
                .restore(&ctx)
                .map_err(registration_error)?;
            let has: Function = helpers.get("has").map_err(registration_error)?;
            for spec in &metadata.functions {
                if !has
                    .call::<_, bool>((spec.id.to_ascii_uppercase(),))
                    .map_err(registration_error)?
                {
                    return Err(JsHostError::Registration(format!(
                        "{} has no CustomFunctions.associate entry",
                        spec.id
                    )));
                }
            }
            Ok(())
        })
    }

    fn invoke_sync(
        &mut self,
        spec: &FunctionSpec,
        arguments: &[JsArgument],
        invocation: &JsInvocation,
    ) -> Result<CalcValue, JsHostError> {
        let realm = self.realm()?;
        let end = end_time(self.limits)?;
        realm.with_invocation_deadline(end, || {
            realm.context.with(|ctx| {
                let result = (|| {
                    let helpers = realm.helpers.clone().restore(&ctx)?;
                    let mut bridge = Bridge {
                        ctx: &ctx,
                        helpers,
                        end,
                        output_bytes: self.limits.heap_bytes,
                    };
                    bridge.invoke(spec, arguments, invocation)
                })();
                result.unwrap_or_else(|error| {
                    if matches!(error, Error::Exception) {
                        ctx.catch();
                    }
                    check_time(end)?;
                    Err(JsHostError::Execution(error.to_string()))
                })
            })
        })
    }
}

fn end_time(limits: ExecLimits) -> Result<Instant, JsHostError> {
    Instant::now()
        .checked_add(limits.time)
        .ok_or(JsHostError::Timeout)
}

fn check_time(end: Instant) -> Result<(), JsHostError> {
    if Instant::now() >= end {
        Err(JsHostError::Timeout)
    } else {
        Ok(())
    }
}

fn registration_error(error: Error) -> JsHostError {
    JsHostError::Registration(error.to_string())
}

struct Bridge<'a, 'js> {
    ctx: &'a Ctx<'js>,
    helpers: Object<'js>,
    end: Instant,
    output_bytes: usize,
}

impl<'js> Bridge<'_, 'js> {
    fn invoke(
        &mut self,
        spec: &FunctionSpec,
        arguments: &[JsArgument],
        invocation: &JsInvocation,
    ) -> rquickjs::Result<Result<CalcValue, JsHostError>> {
        let lookup: Function = self.helpers.get("lookup")?;
        let function: Function = lookup.call((spec.id.to_ascii_uppercase(),))?;
        let mut args = Args::new(self.ctx.clone(), arguments.len() + 1);
        for arg in arguments {
            let value = match arg {
                JsArgument::Value(value) => self.input(value, true)?,
                JsArgument::Repeating(values) => {
                    let array = Array::new(self.ctx.clone())?;
                    for (i, value) in values.iter().enumerate() {
                        define_element(&array, i, self.input(value, true)?)?;
                    }
                    array.into_value()
                }
            };
            args.push_arg(value)?;
        }
        args.push_arg(self.invocation(invocation)?)?;
        let called = function.call_arg(args);
        if let Err(timeout) = check_time(self.end) {
            if matches!(called, Err(Error::Exception)) {
                self.ctx.catch();
            }
            return Ok(Err(timeout));
        }
        let value: Value = match called {
            Err(Error::Exception) => {
                let thrown = self.ctx.catch();
                return Ok(match self.returned_error(&thrown)? {
                    Some(code) => Ok(CalcValue::error(code)),
                    None => Err(JsHostError::Execution(
                        "JavaScript threw an exception".into(),
                    )),
                });
            }
            result => result?,
        };
        if value.is_promise() {
            return Ok(Err(JsHostError::Deferred {
                reason: "JavaScript custom functions that return a Promise",
                owner: "dc-26wtr.6",
            }));
        }
        Ok(Ok(self.output(value, spec.result.dimensionality)?))
    }

    fn invocation(&self, data: &JsInvocation) -> rquickjs::Result<Object<'js>> {
        let invocation = Object::new_proto(self.ctx.clone(), None)?;
        invocation.set("functionName", data.function_name.as_str())?;
        invocation.set("isInValuePreview", false)?;
        if let Some(address) = &data.address {
            invocation.set("address", address.as_str())?;
        }
        if let Some(addresses) = &data.parameter_addresses {
            let array = Array::new(self.ctx.clone())?;
            for (index, address) in addresses.iter().enumerate() {
                define_element(&array, index, address.as_deref())?;
            }
            invocation.set("parameterAddresses", array)?;
        }
        Ok(invocation)
    }

    fn input(&self, value: &CalcValue, may_be_matrix: bool) -> rquickjs::Result<Value<'js>> {
        if Instant::now() >= self.end {
            return Err(conversion("argument conversion exceeded the time limit"));
        }
        let ctx = self.ctx.clone();
        Ok(match value.core_ref() {
            CoreRef::Number(n) => Value::new_float(ctx, n),
            CoreRef::Logical(b) => Value::new_bool(ctx, b),
            CoreRef::Empty | CoreRef::Missing => Value::new_null(ctx),
            CoreRef::Text(text) => {
                let from_units: Function = self.helpers.get("fromUnits")?;
                from_units.call((TypedArray::<u16>::new_copy(ctx, text.utf16_code_units())?,))?
            }
            CoreRef::Error(code) => {
                let make_error: Function = self.helpers.get("makeError")?;
                make_error.call((code.excel_spelling(),))?
            }
            CoreRef::Array(array) if may_be_matrix => {
                let outer = Array::new(ctx.clone())?;
                for i in 0..array.shape().rows {
                    let row = array.row_slice(i).expect("row in bounds");
                    let inner = Array::new(ctx.clone())?;
                    for (j, cell) in row.iter().enumerate() {
                        define_element(&inner, j, self.input(cell, false)?)?;
                    }
                    define_element(&outer, i, inner)?;
                }
                outer.into_value()
            }
            _ => return Err(conversion("expected a scalar worksheet value")),
        })
    }

    fn output(&mut self, value: Value<'js>, shape: Dimensionality) -> rquickjs::Result<CalcValue> {
        if value.is_array() && shape == Dimensionality::Matrix {
            return self.matrix(value.into_array().expect("checked array"));
        }
        self.scalar(value)
    }

    fn matrix(&mut self, array: Array<'js>) -> rquickjs::Result<CalcValue> {
        // Array::len assumes a small integer. A sparse result may declare a much larger length.
        let rows: u32 = array.as_object().get("length")?;
        if rows == 0 {
            return Ok(CalcValue::error(W::Calc));
        }
        if rows > MAX_ROWS {
            return Err(conversion("matrix exceeds Excel's row limit"));
        }
        let mut cells = Vec::new();
        let mut cols = 0;
        for i in 0..rows {
            let row: Array = array.get(i as usize)?;
            let width: u32 = row.as_object().get("length")?;
            if i == 0 {
                cols = width;
                if cols == 0 || cols > MAX_COLUMNS {
                    return Err(conversion("invalid matrix width"));
                }
            }
            if cols != width {
                return Err(conversion("matrix rows differ in length"));
            }
            for j in 0..cols {
                cells.push(self.scalar(row.get(j as usize)?)?);
            }
        }
        let shape = ArrayShape {
            rows: rows as usize,
            cols: cols as usize,
        };
        CalcArray::new(shape, cells)
            .map(CalcValue::array)
            .ok_or_else(|| conversion("invalid matrix"))
    }

    fn scalar(&mut self, value: Value<'js>) -> rquickjs::Result<CalcValue> {
        if Instant::now() >= self.end {
            return Err(conversion("result conversion exceeded the time limit"));
        }
        self.charge(size_of::<CalcValue>())?;
        if let Some(n) = value.as_number() {
            return Ok(if n.is_finite() {
                CalcValue::number(n)
            } else {
                CalcValue::error(W::Num)
            });
        }
        if let Some(logical) = value.as_bool() {
            return Ok(CalcValue::logical(logical));
        }
        if value.is_string() {
            let to_units: Function = self.helpers.get("toUnits")?;
            let units: TypedArray<u16> = to_units.call((value, EXCEL_TEXT_MAX_UTF16_CODE_UNITS))?;
            let units: &[u16] = units.as_ref();
            self.charge(std::mem::size_of_val(units))?;
            return Ok(CalcValue::text(ExcelText::from_utf16_code_units(
                units.to_vec(),
            )));
        }
        if value.is_null() || value.is_undefined() {
            return Ok(CalcValue::empty());
        }
        if let Some(code) = self.returned_error(&value)? {
            return Ok(CalcValue::error(code));
        }
        Err(conversion("expected a scalar worksheet result"))
    }

    fn returned_error(&self, value: &Value<'js>) -> rquickjs::Result<Option<W>> {
        let error_code: Function = self.helpers.get("errorCode")?;
        let code: Value = error_code.call((value.clone(),))?;
        let Some(code) = code.as_string() else {
            return Ok(None);
        };
        Ok(match code.to_string()?.as_str() {
            "#NULL!" => Some(W::Null),
            "#DIV/0!" => Some(W::Div0),
            "#VALUE!" => Some(W::Value),
            "#REF!" => Some(W::Ref),
            "#NAME?" => Some(W::Name),
            "#NUM!" => Some(W::Num),
            "#N/A" => Some(W::NA),
            _ => None,
        })
    }

    fn charge(&mut self, bytes: usize) -> rquickjs::Result<()> {
        self.output_bytes = self
            .output_bytes
            .checked_sub(bytes)
            .ok_or_else(|| conversion("result exceeds the custom function memory limit"))?;
        Ok(())
    }
}

// Define ordinary elements directly so a guest's Array.prototype setters cannot
// intercept host argument construction.
fn define_element<'js>(
    array: &Array<'js>,
    index: usize,
    value: impl IntoJs<'js>,
) -> rquickjs::Result<()> {
    array.as_object().prop(
        index as u32,
        Property::from(value).configurable().enumerable().writable(),
    )
}

fn conversion(message: &'static str) -> Error {
    Error::new_from_js_message("JavaScript", "worksheet value", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: QuickJS runs in a native runtime.
    #[test]
    fn deadline_cleanup_preserves_a_completed_invocation() {
        let mut engine = QuickJs::new(ExecLimits::default());
        engine.load_module("test", "").unwrap();
        let realm = engine.realm().unwrap();
        let value = CalcValue::number(42.);
        let result = realm.with_invocation_deadline(Instant::now(), || Ok(value.clone()));
        assert_eq!(result, Ok(value));
        assert_eq!(realm.deadline.get(), None);
    }
}
