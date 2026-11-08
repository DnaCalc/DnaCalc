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
mod worksheet;
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
    worksheet_refusals: Vec<worksheet::Refusal>,
    spreadsheet: Option<Arc<spreadsheet::SpreadsheetHost>>,
    event_project: bool,
    /// Whether the project reaches platform COM objects through its references.
    com: bool,
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
        Self::load_modules(
            vec![sym::ModuleUnit {
                module_name: module_name.into(),
                module_kind: sym::ModuleKind::Procedural,
                attributes: sym::ModuleAttributes::named(module_name),
                source: source.into(),
            }],
            &crate::events::ProjectReferences::none(),
            spreadsheet_objects,
            false,
            false,
            true,
            false,
        )
    }
    pub(crate) fn load_event_project(
        modules: &[crate::events::ProjectModule],
        references: &crate::events::ProjectReferences,
        private_worksheet_lookup: bool,
        events: crate::events::DocumentEvents,
    ) -> Result<Self, Error> {
        let automatic_events = events == crate::events::DocumentEvents::All;
        let workbook_events = events == crate::events::DocumentEvents::Workbook;
        let modules = modules
            .iter()
            .map(|module| {
                let host = match module.kind {
                    crate::events::ModuleKind::Standard => None,
                    crate::events::ModuleKind::Class => None,
                    crate::events::ModuleKind::Worksheet => Some("Excel.Worksheet"),
                    crate::events::ModuleKind::Workbook => Some("Excel.Workbook"),
                };
                let mut attributes = sym::ModuleAttributes::named(&module.name);
                attributes.host_document_type = host.map(str::to_owned);
                attributes.vb_predeclared_id = host.is_some();
                sym::ModuleUnit {
                    module_name: module.name.clone(),
                    module_kind: if module.kind == crate::events::ModuleKind::Class {
                        sym::ModuleKind::Class
                    } else if host.is_some() {
                        sym::ModuleKind::Document
                    } else {
                        sym::ModuleKind::Procedural
                    },
                    attributes,
                    source: exported_source(&module.source),
                }
            })
            .collect();
        Self::load_modules(
            modules,
            references,
            true,
            true,
            private_worksheet_lookup,
            automatic_events,
            workbook_events,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn load_modules(
        modules: Vec<sym::ModuleUnit>,
        references: &crate::events::ProjectReferences,
        spreadsheet_objects: bool,
        events: bool,
        private_worksheet_lookup: bool,
        automatic_events: bool,
        workbook_events: bool,
    ) -> Result<Self, Error> {
        let procedural: std::collections::HashSet<_> = modules
            .iter()
            .filter(|m| m.module_kind == sym::ModuleKind::Procedural)
            .map(|m| m.module_name.to_ascii_lowercase())
            .collect();
        let mut manifest = sym::SymbolProjectManifest {
            project_name: "UdfProject".into(),
            project_kind: sym::ProjectKind::Library,
            modules,
            references: if spreadsheet_objects {
                vec![
                    sym::ProjectReference::HostInjected {
                        referenced_project_name: "DnaCalc.Application".into(),
                    },
                    sym::ProjectReference::HostInjected {
                        referenced_project_name: "DnaCalc.ActiveSheet".into(),
                    },
                    sym::ProjectReference::HostInjected {
                        referenced_project_name: "DnaCalc.ActiveWorkbook".into(),
                    },
                    sym::ProjectReference::HostInjected {
                        referenced_project_name: "DnaCalc.ActiveWindow".into(),
                    },
                    sym::ProjectReference::HostInjected {
                        referenced_project_name: "DnaCalc.Selection".into(),
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
                .into_iter()
                .chain(references.compiler.iter().cloned())
                .collect()
            } else {
                references.compiler.clone()
            },
            reference_projects: vec![],
            conditional_constants: Default::default(),
            conditional_compilation_target: Default::default(),
        };
        let event_types = crate::events::types::EventTypes {
            automatic_events,
            workbook_events,
        };
        let own: &dyn oxvba_symbol::TypeLibResolver = if events {
            &event_types
        } else if spreadsheet_objects {
            &spreadsheet::SpreadsheetTypes
        } else {
            &oxvba_symbol::CatalogTypeLibResolver
        };
        let chained = crate::events::types::WithReferences {
            own,
            libraries: &references.libraries,
        };
        let resolver: &dyn oxvba_symbol::TypeLibResolver = &chained;
        if events {
            validate_project_modules(&manifest, resolver, automatic_events)?;
        }
        let mut engine =
            oxvba_embed::Module::load_project(&manifest, resolver).map_err(engine_error)?;
        let entries = if private_worksheet_lookup {
            let entries = worksheet::private_entries(&mut manifest, &engine)?;
            if !entries.bridges.is_empty() {
                engine = oxvba_embed::Module::load_project(&manifest, resolver).map_err(engine_error)?;
            }
            entries
        } else {
            Default::default()
        };
        if events
            && automatic_events
            && let Some(route) = engine
                .event_routes()
                .iter()
                .filter_map(|route| route.native.as_ref())
                .find(|native| {
                    native.parameter_types.iter().any(|ty| {
                        !matches!(
                            ty,
                            oxvba_com::TypeLibParamType::Boolean
                                | oxvba_com::TypeLibParamType::Object
                                | oxvba_com::TypeLibParamType::ByRefBoolean
                        )
                    })
                })
        {
            return Err(Error::UnsupportedSignature(format!(
                "automatic {} needs an event argument carrier unavailable in this runtime (dc-hsa5.12)",
                route.name
            )));
        }
        let functions: Vec<_> = engine
            .procedures()
            .filter(|p| {
                p.visibility == oxvba_embed::Visibility::Public
                    && p.kind == oxvba_embed::ProcedureKind::Function
                    && procedural.contains(&p.module.to_ascii_lowercase())
            })
            .map(|p| {
                let original = entries
                    .bridges
                    .get(&(p.module.clone(), p.name.clone()))
                    .unwrap_or(p);
                let mut entry = p.clone();
                // Calls supply every owning argument. Keep the original defaults
                // and optional markers in the adapter's metadata, while binding
                // the generated procedure's real session-owned handle.
                entry.parameters = original.parameters.clone();
                entry.return_type = original.return_type.clone();
                (
                    Function {
                        name: original.name.clone(),
                        qualified_name: format!("{}.{}", original.module, original.name),
                        parameters: original
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
                        return_type: original.return_type.as_ref().map(type_name).unwrap_or_default(),
                    },
                    entry,
                )
            })
            .collect();
        if events
            && automatic_events
            && functions.iter().any(|(_, procedure)| {
                procedure
                    .parameters
                    .iter()
                    .any(|p| matches!(p.ty, OxTy::Object(_)))
                    || procedure
                        .return_type
                        .as_ref()
                        .is_some_and(|ty| matches!(ty, OxTy::Object(_)))
            })
        {
            return Err(Error::UnsupportedSignature("object-valued worksheet UDF signatures in document projects require the shared Range projection (dc-hsa5.12)".into()));
        }
        if events
            && automatic_events
            && functions.iter().any(|(_, procedure)| {
                procedure.parameters.iter().any(|p| {
                    !matches!(
                        p.ty,
                        OxTy::Long | OxTy::Double | OxTy::Bool | OxTy::Str | OxTy::Variant
                    )
                }) || !procedure.return_type.as_ref().is_some_and(|ty| {
                    matches!(
                        ty,
                        OxTy::Long | OxTy::Double | OxTy::Bool | OxTy::Str | OxTy::Variant
                    )
                })
            })
        {
            return Err(Error::UnsupportedSignature("worksheet UDF signature outside the document project's owning value carriers (dc-hsa5.12)".into()));
        }
        let spreadsheet =
            spreadsheet_objects.then(|| Arc::new(spreadsheet::SpreadsheetHost::new(events && automatic_events, references.com)));
        let engine = if let Some(host) = &spreadsheet {
            engine.with_host(host.clone())
        } else {
            engine
        };
        Ok(Self {
            engine,
            functions,
            worksheet_refusals: entries.refusals,
            spreadsheet,
            event_project: events && automatic_events,
            com: references.com,
        })
    }
    pub fn functions(&self) -> impl ExactSizeIterator<Item = &Function> {
        self.functions.iter().map(|(function, _)| function)
    }
    pub(crate) fn worksheet_refusals(&self) -> impl Iterator<Item = (&str, &str, &'static str)> {
        self.worksheet_refusals.iter().map(|entry| {
            (entry.name.as_str(), entry.qualified_name.as_str(), entry.reason)
        })
    }
}

fn validate_project_modules(
    manifest: &sym::SymbolProjectManifest,
    resolver: &dyn oxvba_symbol::TypeLibResolver,
    automatic_events: bool,
) -> Result<(), Error> {
    use oxvba_symbol::model::SymbolKind;
    let env = oxvba_symbol::build_resolution_environment(manifest, resolver)
        .map_err(|error| Error::Compile(error.to_string()))?;
    let compiled = env.modules().collect::<Vec<_>>();
    if compiled.len() != manifest.modules.len() {
        return Err(Error::UnsupportedSignature(
            "project module inventory changed its directory identity (dc-snz.15)".into(),
        ));
    }
    // The compiler preserves active manifest order and honors exported VB_Name.
    // Compare each source unit, so even a permutation of existing names refuses.
    for (module, unit) in compiled.into_iter().zip(&manifest.modules) {
        // ubs:ignore[rust.security.constant-time-compare] These are public source module kinds and names, not secrets.
        if module.module_kind != unit.module_kind
            || oxvba_symbol::model::fold_identifier(&unit.module_name)
                != oxvba_symbol::model::fold_identifier(module.module_name)
        {
            return Err(Error::UnsupportedSignature(format!(
                "project module {} changed its directory identity (dc-snz.15)",
                unit.module_name
            )));
        }
        // ubs:ignore[rust.security.constant-time-compare] A public module-kind discriminator controls event validation.
        if !automatic_events || module.module_kind != sym::ModuleKind::Document {
            continue;
        }
        let source = unit
            .attributes
            .host_document_type
            .as_deref()
            .ok_or_else(|| {
                Error::UnsupportedSignature("document host type is unavailable (dc-hsa5.12)".into())
            })?;
        let prefix = format!("{}_", source.rsplit('.').next().expect("host type name"));
        let events = env.com_source_event_metadata(source).unwrap_or_default();
        for id in env
            .symbols
            .symbols_in_scope(module.module_scope)
            .map_err(|error| Error::Compile(error.to_string()))?
        {
            let symbol = env.symbols.symbol(id).expect("declared symbol");
            if !matches!(symbol.kind, SymbolKind::Procedure | SymbolKind::Function) {
                continue;
            }
            let name = &env
                .symbols
                .name(symbol.name)
                .expect("declared name")
                .first_spelling;
            if name.len() >= prefix.len()
                && name.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
                && !events
                    .iter()
                    .any(|event| name[prefix.len()..].eq_ignore_ascii_case(&event.name))
            {
                return Err(Error::UnsupportedSignature(format!(
                    "document procedure {name} has an unsupported automatic-event prefix (dc-hsa5.12)"
                )));
            }
        }
    }
    Ok(())
}
/// Excel exports these descriptive member attributes inside the procedure. The
/// pinned frontend accepts them at module level. Relocate only complete, parsed
/// metadata statements; preserve all executable text and every other attribute.
fn exported_source(source: &str) -> String {
    use oxvba_syntax::{SyntaxKind as K, lexer::tokenize};
    let tokens = tokenize(source);
    let mut body = String::with_capacity(source.len());
    let mut attributes = String::new();
    let mut start = 0;
    for end in (0..tokens.len()).filter(|&i| tokens[i].0 == K::Newline)
        .map(|i| i + 1).chain(std::iter::once(tokens.len()))
    {
        let statement = &tokens[start..end];
        start = end;
        let significant = statement.iter().filter(|(kind, _)| !matches!(
            kind, K::Whitespace | K::Comment | K::Newline | K::LineContinuation,
        )).collect::<Vec<_>>();
        let description = significant.len() == 6
            && significant[3].1.eq_ignore_ascii_case("VB_Description");
        let invoke = significant.len() == 8
            && significant[3].1.eq_ignore_ascii_case("VB_ProcData")
            && significant[4].0 == K::Dot
            && significant[5].1.eq_ignore_ascii_case("VB_Invoke_Func");
        let metadata = (description || invoke)
            && significant[0].0 == K::KwAttribute
            && significant[1].0 == K::Ident
            && significant[2].0 == K::Dot
            && significant[significant.len() - 2].0 == K::Eq
            && significant[significant.len() - 1].0 == K::StringLiteral
            // The frontend's lossless lexer also labels an unterminated string
            // StringLiteral. After the opening quote, escaped quotes occur in
            // pairs and only an odd final run includes a closing delimiter.
            && significant[significant.len() - 1].1.strip_prefix('"').is_some_and(|body| {
                body.bytes().rev().take_while(|byte| *byte == b'"').count() % 2 == 1
            });
        let text: String = statement.iter().map(|(_, text)| *text).collect();
        if metadata && oxvba_syntax::parse(&text).errors().is_empty() {
            attributes.push_str(&text);
            attributes.push('\n');
            for ch in text.chars() {
                body.push(if matches!(ch, '\r' | '\n') { ch } else { ' ' });
            }
        } else {
            body.push_str(&text);
        }
    }
    if !attributes.is_empty() {
        body.push('\n');
        body.push_str(&attributes);
    }
    body
}

#[cfg(test)]
mod exported_tests {
    use super::exported_source;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn exported_metadata_never_removes_executable_or_unknown_source() {
        for source in [
            "' Attribute F.VB_Description = \"comment\"\n",
            "F = \"Attribute F.VB_Description = x\"\n",
            "Attribute F.VB_Unknown = \"unknown\"\n",
            "Attribute F.VB_Description = \"bad\" : F = 99\n",
            "F = 1 + _\nAttribute F.VB_Description = \"continued\"\n",
            "Attribute F.VB_Description = \"unterminated\n",
            "Attribute F.VB_Description = \"unterminated\"\"\n",
        ] {
            assert_eq!(exported_source(source), source);
        }
        let source = "Public Function F() As Long\r\nAttribute F.VB_Description = \"text\"\r\n F = 7\r\nEnd Function\r\n";
        let adjusted = exported_source(source);
        assert!(adjusted.contains(" F = 7\r\nEnd Function\r\n"));
        assert!(adjusted.ends_with("Attribute F.VB_Description = \"text\"\r\n\n"));
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

pub(crate) fn to_variant(value: &Value) -> Result<Variant, Error> {
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

pub(crate) fn from_variant(value: &Variant, inside_array: bool) -> Result<Value, Error> {
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
