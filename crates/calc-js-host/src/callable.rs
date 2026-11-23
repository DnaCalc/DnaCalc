use crate::{
    DataType, Dimensionality, FunctionSpec, FunctionsMetadata, JsArgument, JsEngine, JsHostError,
    JsInvocation, Parameter,
};
use calc_fml::{CallableDescriptor, CallableParameter, CallableProvider, CallableTable};
use calc_func::{UserFunction, UserFunctionContext};
use calc_value::{CalcArray, CalcValue, CellKey, CoreRef, GridArea, WorksheetErrorCode as W};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

type Engine = RefCell<Box<dyn JsEngine>>;

/// A loaded add-in. Dropping the owner revokes its callables, including previously published tables.
#[derive(Debug)]
pub struct JsAddin {
    _engine: Rc<Engine>,
    table: CallableTable,
}

impl JsAddin {
    /// Load source and publish every association atomically, after the owning host permits execution.
    pub fn load(
        mut engine: Box<dyn JsEngine>,
        module_id: &str,
        source: &str,
        metadata: &FunctionsMetadata,
    ) -> Result<Self, JsHostError> {
        metadata.validate()?;
        engine.load_module(module_id, source)?;
        engine.register_functions(metadata)?;
        let engine = Rc::new(RefCell::new(engine));
        let mut table = CallableTable::new();
        for spec in &metadata.functions {
            let name = metadata.worksheet_name(spec);
            let descriptor = CallableDescriptor {
                name: name.clone(),
                origin: format!("Office.js {}", metadata.namespace),
                description: spec.description.clone(),
                volatile: spec.options.volatile,
                asynchronous: spec.options.stream || spec.options.cancelable,
                parameters: spec
                    .parameters
                    .iter()
                    .map(|parameter| CallableParameter {
                        name: parameter.name.clone(),
                        description: parameter.description.clone(),
                    })
                    .collect(),
                ..CallableDescriptor::default()
            };
            table
                .register_provider(
                    CallableProvider::JavaScript,
                    None,
                    &name,
                    Rc::new(JsCallable {
                        engine: Rc::downgrade(&engine),
                        name: name.clone(),
                        spec: spec.clone(),
                        allow_any_errors: metadata.allow_error_for_data_type_any,
                    }),
                    Some(descriptor),
                )
                .map_err(JsHostError::Registration)?;
        }
        Ok(Self {
            _engine: engine,
            table,
        })
    }

    /// Merge these registrations into the application's ordinary `CallableTable`.
    pub fn callables(&self) -> &CallableTable {
        &self.table
    }
}

#[derive(Debug)]
struct JsCallable {
    engine: Weak<Engine>,
    name: String,
    spec: FunctionSpec,
    allow_any_errors: bool,
}

impl JsCallable {
    fn invoke(
        &self,
        args: &[CalcValue],
        invocation: &JsInvocation,
    ) -> Result<CalcValue, JsHostError> {
        for (flag, reason, owner) in [
            (
                self.spec.options.stream,
                "streaming JavaScript custom functions",
                "dc-26wtr.7",
            ),
            (
                self.spec.options.cancelable,
                "cancelable JavaScript custom functions",
                "dc-26wtr.6",
            ),
        ] {
            if flag {
                return Err(JsHostError::Deferred { reason, owner });
            }
        }
        let engine = self
            .engine
            .upgrade()
            .ok_or_else(|| JsHostError::Execution("the add-in was unloaded".into()))?;
        let mut input = args.iter();
        let mut prepared = Vec::with_capacity(self.spec.parameters.len());
        for parameter in &self.spec.parameters {
            let packed = if parameter.repeating {
                input
                    .by_ref()
                    .map(|arg| self.prepare(parameter, Some(arg)))
                    .collect::<Result<Vec<_>, _>>()
                    .map(JsArgument::Repeating)
            } else {
                self.prepare(parameter, input.next()).map(JsArgument::Value)
            };
            match packed {
                Ok(argument) => prepared.push(argument),
                Err(code) => return Ok(CalcValue::error(code)),
            }
        }
        if input.next().is_some() {
            return Ok(CalcValue::error(W::Value));
        }
        engine
            .try_borrow_mut()
            .map_err(|_| JsHostError::Execution("reentrant custom function call".into()))?
            .invoke_sync(&self.spec, &prepared, invocation)
    }

    fn prepare(&self, parameter: &Parameter, argument: Option<&CalcValue>) -> Result<CalcValue, W> {
        let Some(argument) = argument.filter(|value| !value.is_missing()) else {
            return if parameter.optional || parameter.repeating {
                Ok(CalcValue::missing())
            } else {
                Err(W::Value)
            };
        };
        if parameter.dimensionality == Dimensionality::Scalar {
            self.validate_cell(parameter, argument)?;
            return Ok(argument.clone());
        }
        match argument.as_array() {
            Some(array) => {
                for cell in array.iter_row_major() {
                    self.validate_cell(parameter, cell)?;
                }
                Ok(argument.clone())
            }
            None => {
                self.validate_cell(parameter, argument)?;
                Ok(CalcValue::array(CalcArray::from_scalar(argument.clone())))
            }
        }
    }

    fn validate_cell(&self, param: &Parameter, value: &CalcValue) -> Result<(), W> {
        let data_type = match value.core_ref() {
            CoreRef::Empty | CoreRef::Missing => return Ok(()),
            CoreRef::Error(code) => {
                return if param.data_type == DataType::Any && self.allow_any_errors {
                    Ok(())
                } else {
                    Err(code)
                };
            }
            CoreRef::Number(_) => DataType::Number,
            CoreRef::Logical(_) => DataType::Boolean,
            CoreRef::Text(_) => DataType::String,
            CoreRef::Reference(_) | CoreRef::Array(_) => return Err(W::Value),
        };
        if param.data_type == DataType::Any || param.data_type == data_type {
            Ok(())
        } else {
            Err(W::Value)
        }
    }

    fn invocation(&self) -> JsInvocation {
        JsInvocation {
            function_name: self.name.clone(),
            ..JsInvocation::default()
        }
    }
}

impl UserFunction for JsCallable {
    fn call(&self, args: &[CalcValue]) -> Result<CalcValue, String> {
        if self.spec.options.requires_address || self.spec.options.requires_parameter_addresses {
            return Err("custom function needs a worksheet address context".into());
        }
        self.invoke(args, &self.invocation())
            .map_err(|e| e.to_string())
    }

    fn call_with_context(
        &self,
        args: &[CalcValue],
        context: &mut dyn UserFunctionContext,
    ) -> Result<CalcValue, String> {
        let mut invocation = self.invocation();
        if self.spec.options.requires_address {
            let cell = context
                .caller()
                .ok_or("custom function needs a worksheet caller")?;
            invocation.address = Some(area_address(context, GridArea::cell(cell))?);
        }
        if self.spec.options.requires_parameter_addresses {
            invocation.parameter_addresses = Some(
                args.iter()
                    .map(|arg| {
                        let Some(reference) = arg.as_reference() else {
                            return Ok(None);
                        };
                        if let [area] = reference.areas() {
                            area_address(context, *area).map(Some)
                        } else {
                            Err(context.refuse_by(
                                "Office.js addresses for discontiguous or 3-D references",
                                "dc-17spk.22",
                            ))
                        }
                    })
                    .collect::<Result<_, _>>()?,
            );
        }
        if self.spec.options.volatile {
            context.set_volatile(true);
        }
        let args = args
            .iter()
            .map(|argument| match argument.as_reference() {
                Some(reference) => context.argument_values_of(reference),
                None => Ok(argument.clone()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.invoke(&args, &invocation)
            .map_err(|error| match error {
                JsHostError::Deferred { reason, owner } => context.refuse_by(reason, owner),
                error => error.to_string(),
            })
    }
}

fn area_address(context: &mut dyn UserFunctionContext, area: GridArea) -> Result<String, String> {
    let sheet = context.sheet_name(area.sheet())?;
    let sheet = if sheet.chars().all(char::is_alphanumeric) {
        sheet
    } else {
        format!("'{}'", sheet.replace('\'', "''"))
    };
    let a1 = |cell: CellKey| {
        format!(
            "{}{}",
            calc_fml::reference::column_letters(cell.column()),
            cell.row()
        )
    };
    let first = a1(area.first());
    Ok(if area.first() == area.last() {
        format!("{sheet}!{first}")
    } else {
        format!("{sheet}!{first}:{}", a1(area.last()))
    })
}
