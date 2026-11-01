//! DNA-owned value adapter and runtime composition. Ported from OxVba 325257ea
//! adapters/spreadsheet-compat/src/lib.rs; compiler, code and state stay in OxVba.
use oxvba_bundle::coreir::CoreConst;
use oxvba_oxir::OxTy;
use oxvba_runtime::{
    VarType, Variant,
    safe_array::{SafeArray, SafeArrayBound},
};
use oxvba_symbol::manifest as sym;
use std::sync::Arc;
mod cache;
pub(crate) mod spreadsheet;
pub use cache::{CodeCache, OwnedSession};
pub use oxvba_embed::{Backend, CacheStats};
pub(crate) use spreadsheet::{ContextError, Range, RangeInfo, SpreadsheetContext};

/// Spreadsheet values; array cells are row-major and contain scalar values.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Empty,
    /// An omitted positional argument, distinct from a blank cell.
    Missing,
    /// A live read-only reference in the host's sheet namespace.
    Range(Range),
    Number(f64),
    Text(String),
    Boolean(bool),
    /// VBA CVErr / Excel error code, e.g. 2042 for #N/A.
    Error(i32),
    Array {
        rows: u32,
        columns: u32,
        cells: Vec<Value>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub type_name: String,
    pub by_ref: bool,
    pub optional: bool,
    pub param_array: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub qualified_name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: String,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("compile: {0}")]
    Compile(String),
    #[error("unknown public function: {0}")]
    UnknownFunction(String),
    #[error("expected {expected} arguments, received {received}")]
    ArgumentCount { expected: usize, received: usize },
    #[error("unsupported signature: {0}")]
    UnsupportedSignature(String),
    #[error("value conversion: {0}")]
    ValueConversion(String),
    #[error("spreadsheet host refused the call: {0}")]
    HostRefused(String),
    #[error("execution budget exhausted")]
    BudgetExceeded,
    #[error("JIT backend is not enabled in this build")]
    BackendUnavailable,
    #[error("backend: {0}")]
    Backend(String),
    #[error("backend does not support this module: {0}")]
    UnsupportedBackend(String),
    #[error("VBA error {number}: {description}")]
    Runtime {
        number: i32,
        description: String,
        vba_source: String,
    },
}

#[derive(Clone)]
pub struct Module {
    engine: oxvba_embed::Module,
    functions: Vec<(Function, oxvba_embed::Procedure)>,
    spreadsheet: Option<Arc<spreadsheet::SpreadsheetHost>>,
}
impl Module {
    pub fn load_spreadsheet(module_name: &str, source: &str) -> Result<Self, Error> {
        Self::load_inner(module_name, source, true)
    }
    fn load_inner(
        module_name: &str,
        source: &str,
        spreadsheet_objects: bool,
    ) -> Result<Self, Error> {
        let manifest = sym::SymbolProjectManifest {
            project_name: "UdfProject".into(),
            project_kind: sym::ProjectKind::Library,
            modules: vec![sym::ModuleUnit {
                module_name: module_name.into(),
                module_kind: sym::ModuleKind::Procedural,
                attributes: sym::ModuleAttributes::named(module_name),
                source: source.into(),
            }],
            references: if spreadsheet_objects {
                vec![
                    sym::ProjectReference::HostInjected {
                        referenced_project_name: "DnaCalc.Application".into(),
                    },
                    sym::ProjectReference::TypeLibrary {
                        // VBA resolves unqualified types in reference order. The source-compatible
                        // Range must retain Variant Value2, including Empty; the numeric-only
                        // host interface remains available as DnaCalcObjects.Range.
                        name: "Excel".into(),
                        guid: None,
                        version_major: None,
                        version_minor: None,
                        lcid: None,
                        import_lib: None,
                    },
                    sym::ProjectReference::TypeLibrary {
                        name: "DnaCalcObjects".into(),
                        guid: None,
                        version_major: None,
                        version_minor: None,
                        lcid: None,
                        import_lib: None,
                    },
                ]
            } else {
                vec![]
            },
            reference_projects: vec![],
            conditional_constants: Default::default(),
            conditional_compilation_target: Default::default(),
        };
        let resolver: &dyn oxvba_symbol::TypeLibResolver = if spreadsheet_objects {
            &spreadsheet::SpreadsheetTypes
        } else {
            &oxvba_symbol::CatalogTypeLibResolver
        };
        let engine =
            oxvba_embed::Module::load_project(&manifest, resolver).map_err(engine_error)?;
        let functions = engine
            .procedures()
            .filter(|p| {
                p.visibility == oxvba_embed::Visibility::Public
                    && p.kind == oxvba_embed::ProcedureKind::Function
            })
            .map(|p| {
                (
                    Function {
                        name: p.name.clone(),
                        qualified_name: format!("{}.{}", p.module, p.name),
                        parameters: p
                            .parameters
                            .iter()
                            .map(|p| Parameter {
                                name: p.name.clone(),
                                type_name: type_name(&p.ty),
                                by_ref: p.by_ref,
                                optional: p.optional,
                                param_array: p.param_array,
                            })
                            .collect(),
                        return_type: p.return_type.as_ref().map(type_name).unwrap_or_default(),
                    },
                    p.clone(),
                )
            })
            .collect();
        let spreadsheet =
            spreadsheet_objects.then(|| Arc::new(spreadsheet::SpreadsheetHost::new()));
        let engine = if let Some(host) = &spreadsheet {
            engine.with_host(host.clone())
        } else {
            engine
        };
        Ok(Self {
            engine,
            functions,
            spreadsheet,
        })
    }
    pub fn functions(&self) -> impl ExactSizeIterator<Item = &Function> {
        self.functions.iter().map(|(function, _)| function)
    }
}

fn bind_functions(
    functions: &[(Function, oxvba_embed::Procedure)],
    mut bind: impl FnMut(
        &oxvba_embed::Procedure,
    ) -> Result<oxvba_embed::BoundProcedure, oxvba_embed::Error>,
) -> Result<Vec<oxvba_embed::BoundProcedure>, Error> {
    functions
        .iter()
        .map(|(_, p)| bind(p).map_err(engine_error))
        .collect()
}
fn call_function(
    functions: &[(Function, oxvba_embed::Procedure)],
    spreadsheet: Option<&spreadsheet::SpreadsheetHost>,
    index: usize,
    args: &[Value],
    call: impl FnOnce(usize, &[Variant]) -> Result<Variant, Error>,
) -> Result<Value, Error> {
    let (metadata, procedure) = functions
        .get(index)
        .ok_or_else(|| Error::UnknownFunction(index.to_string()))?;
    let variadic = metadata.parameters.last().is_some_and(|p| p.param_array);
    let fixed = procedure.parameters.len() - usize::from(variadic);
    if (!variadic && args.len() > fixed)
        || metadata
            .parameters
            .iter()
            .take(fixed)
            .enumerate()
            .any(|(i, p)| !p.optional && args.get(i).is_none_or(|a| matches!(a, Value::Missing)))
    {
        return Err(Error::ArgumentCount {
            expected: fixed,
            received: args.len(),
        });
    }
    let argument = |value: &Value| match value {
        Value::Range(range) => spreadsheet
            .ok_or_else(|| {
                Error::UnsupportedSignature("Range requires spreadsheet host objects".into())
            })?
            .range_variant(*range),
        value => to_variant(value),
    };
    let mut values = Vec::with_capacity(procedure.parameters.len());
    for (i, parameter) in procedure.parameters.iter().take(fixed).enumerate() {
        let value = match args.get(i) {
            Some(Value::Range(range))
                if !matches!(parameter.ty, OxTy::Variant | OxTy::Object(_)) =>
            {
                to_variant(
                    &spreadsheet
                        .ok_or_else(|| {
                            Error::UnsupportedSignature(
                                "Range requires spreadsheet host objects".into(),
                            )
                        })?
                        .read_argument_range(*range)?,
                )?
            }
            Some(value) if !matches!(value, Value::Missing) => argument(value)?,
            _ => match &parameter.default {
                Some(value) => default_variant(value),
                None if parameter.ty == OxTy::Variant => Variant::from_error_code(-2147352572),
                None if matches!(parameter.ty, OxTy::Object(_)) => Variant::nothing(),
                None => Variant::empty(),
            },
        };
        values.push(coerce_argument(value, &parameter.ty)?);
    }
    if variadic {
        values.push(Variant::from_safearray(SafeArray::from_variants(
            args.iter()
                .skip(fixed)
                .map(argument)
                .collect::<Result<Vec<_>, _>>()?,
        )));
    }
    let result = call(index, &values)?;
    match spreadsheet {
        Some(host) => host.result(&result),
        None => from_variant(&result, false),
    }
}
fn engine_error(error: oxvba_embed::Error) -> Error {
    match error {
        oxvba_embed::Error::Compile(s) => Error::Compile(s),
        oxvba_embed::Error::UnsupportedSignature(s) => Error::UnsupportedSignature(s),
        oxvba_embed::Error::BudgetExceeded => Error::BudgetExceeded,
        oxvba_embed::Error::BackendUnavailable => Error::BackendUnavailable,
        oxvba_embed::Error::UnsupportedBackend(reason) => Error::UnsupportedBackend(reason),
        oxvba_embed::Error::Runtime {
            number,
            description,
            vba_source,
        } => Error::Runtime {
            number,
            description,
            vba_source,
        },
        other => Error::Backend(other.to_string()),
    }
}

fn coerce_argument(value: Variant, ty: &OxTy) -> Result<Variant, Error> {
    use oxvba_bundle::NumericCoerceTarget as Target;
    let target = match ty {
        OxTy::Variant => return Ok(value),
        OxTy::Object(_) if value.vtype() == VarType::Object => return Ok(value),
        OxTy::Array(element, _)
            if **element == OxTy::Variant && value.vtype() == VarType::ArrayVariant =>
        {
            return Ok(value);
        }
        OxTy::Str => return oxvba_eval::arith::coerce_string(&value).map_err(argument_error),
        OxTy::Bool => Target::Boolean,
        OxTy::Byte => Target::Byte,
        OxTy::Integer => Target::Integer,
        OxTy::Long => Target::Long,
        OxTy::LongLong => Target::LongLong,
        OxTy::Single => Target::Single,
        OxTy::Double => Target::Double,
        OxTy::Currency => Target::Currency,
        OxTy::Date => Target::Date,
        other => {
            return Err(Error::UnsupportedSignature(format!(
                "parameter type {other:?}"
            )));
        }
    };
    oxvba_eval::arith::coerce_numeric(&value, target).map_err(argument_error)
}

fn argument_error(error: oxvba_eval::arith::ArithError) -> Error {
    Error::Runtime {
        number: error.code,
        description: error.message,
        vba_source: "UdfProject".into(),
    }
}

fn type_name(ty: &OxTy) -> String {
    match ty {
        OxTy::Bool => "Boolean",
        OxTy::Byte => "Byte",
        OxTy::Integer => "Integer",
        OxTy::Long => "Long",
        OxTy::LongLong => "LongLong",
        OxTy::Single => "Single",
        OxTy::Double => "Double",
        OxTy::Currency => "Currency",
        OxTy::Date => "Date",
        OxTy::Str => "String",
        OxTy::Variant => "Variant",
        OxTy::Object(_) => "Object",
        OxTy::Array(element, _) => return format!("{}()", type_name(element)),
        other => return format!("Unsupported({other:?})"),
    }
    .into()
}

fn to_variant(value: &Value) -> Result<Variant, Error> {
    Ok(match value {
        Value::Range(_) => {
            return Err(Error::ValueConversion(
                "Range requires a live spreadsheet invocation".into(),
            ));
        }
        Value::Empty => Variant::empty(),
        Value::Missing => Variant::from_error_code(-2147352572),
        Value::Number(value) => {
            if !value.is_finite() {
                return Err(Error::ValueConversion(
                    "spreadsheet numbers must be finite".into(),
                ));
            }
            Variant::from_f64(*value)
        }
        Value::Text(value) => Variant::from_string(value.as_str()),
        Value::Boolean(value) => Variant::from_bool(*value),
        Value::Error(code) => Variant::from_error_code(*code),
        Value::Array {
            rows,
            columns,
            cells,
        } => {
            let count = (*rows as usize).checked_mul(*columns as usize);
            if *rows == 0
                || *columns == 0
                || count != Some(cells.len())
                || cells.iter().any(|cell| matches!(cell, Value::Array { .. }))
            {
                return Err(Error::ValueConversion(
                    "array must be a nonempty rectangle of scalar cells".into(),
                ));
            }
            let values = cells
                .iter()
                .map(to_variant)
                .collect::<Result<Vec<_>, _>>()?;
            let array = SafeArray::from_shape_and_variants(
                vec![
                    SafeArrayBound {
                        count: *rows,
                        lower: 1,
                    },
                    SafeArrayBound {
                        count: *columns,
                        lower: 1,
                    },
                ],
                values,
            )
            .map_err(Error::ValueConversion)?;
            Variant::from_safearray(array)
        }
    })
}

fn from_variant(value: &Variant, inside_array: bool) -> Result<Value, Error> {
    Ok(match value.vtype() {
        VarType::Empty => Value::Empty,
        VarType::Boolean => Value::Boolean(value.as_bool().expect("Boolean carrier")),
        VarType::Error => Value::Error(value.as_error_code().expect("Error carrier")),
        VarType::String => {
            let text = value.as_bstr().expect("String carrier");
            if !text.byte_len().is_multiple_of(2) {
                return Err(Error::ValueConversion(
                    "result text has an odd BSTR byte length".into(),
                ));
            }
            Value::Text(
                String::from_utf16(&text.to_utf16_units()).map_err(|_| {
                    Error::ValueConversion("result text is not valid UTF-16".into())
                })?,
            )
        }
        VarType::ArrayVariant if !inside_array => {
            let array = value
                .as_safearray()
                .ok_or_else(|| Error::ValueConversion("unallocated result array".into()))?;
            let bounds = array
                .bounds()
                .ok_or_else(|| Error::ValueConversion("array has no bounds".into()))?;
            let (rows, columns) = match bounds.as_slice() {
                [columns] => (1, columns.count),
                [rows, columns] => (rows.count, columns.count),
                _ => {
                    return Err(Error::ValueConversion(
                        "only one or two dimensional result arrays are supported".into(),
                    ));
                }
            };
            if rows == 0 || columns == 0 {
                return Err(Error::ValueConversion(
                    "result array must be a nonempty rectangle".into(),
                ));
            }
            let cells = (0..array.len())
                .map(|index| {
                    array
                        .variant_element(index)
                        .map_err(Error::ValueConversion)
                        .and_then(|value| from_variant(&value, true))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Value::Array {
                rows,
                columns,
                cells,
            }
        }
        VarType::Byte
        | VarType::Integer
        | VarType::Long
        | VarType::LongLong
        | VarType::Single
        | VarType::Double
        | VarType::Date
        | VarType::Currency
        | VarType::Decimal => {
            let number = oxvba_runtime::coerce::coerce_to(value, VarType::Double)
                .map_err(Error::ValueConversion)?;
            Value::Number(number.as_f64().expect("Double coercion"))
        }
        other => {
            return Err(Error::ValueConversion(format!(
                "unsupported result carrier {other:?}"
            )));
        }
    })
}

fn default_variant(value: &CoreConst) -> Variant {
    match value {
        CoreConst::Empty => Variant::empty(),
        CoreConst::Null => Variant::null(),
        CoreConst::Nothing => Variant::nothing(),
        CoreConst::Bool(v) => Variant::from_bool(*v),
        CoreConst::I16(v) => Variant::from_i16(*v),
        CoreConst::I32(v) => Variant::from_i32(*v),
        CoreConst::I64(v) => Variant::from_i64(*v),
        CoreConst::F64(v) => Variant::from_f64(f64::from_bits(*v)),
        CoreConst::F32(v) => Variant::from_f32(f32::from_bits(*v)),
        CoreConst::Currency(v) => Variant::from_currency_scaled_i64(*v),
        CoreConst::Date(v) => Variant::from_date_f64(f64::from_bits(*v)),
        CoreConst::Str(v) => Variant::from_string(v.as_str()),
    }
}
