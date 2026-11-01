//! Caller-owned, engine-neutral user functions. Formula compilation resolves names here;
//! evaluation uses the current registration, so replacing a module cannot retain old code.

use crate::value::{CalcValue, CellKey, ReferenceLike, WorksheetErrorCode};
use std::{collections::BTreeMap, fmt, rc::Rc};

/// A user function. The default host entry reads references eagerly; adapters can instead
/// use live reference/caller services for one invocation. An adapter
/// returns worksheet errors as values and execution failures as diagnostics (mapped to #VALUE!).
/// No Send/Sync promise is made: a desktop add-in may belong to its owning thread.
pub trait UserFunction: fmt::Debug {
    fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String>;

    /// Reference-aware host entry. Eager adapters keep the original value-call behavior.
    fn call_with_context(
        &self,
        arguments: &[CalcValue],
        context: &mut dyn UserFunctionContext,
    ) -> Result<CalcValue, String> {
        let arguments = arguments
            .iter()
            .map(|value| match value.as_reference() {
                Some(reference) => context.values_of(reference),
                None => Ok(value.clone()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.call(&arguments)
    }
}

/// Services borrowed only for one synchronous call. The evaluator owns the reader and caller;
/// adapters can retain reference identities, and must read their values through this context.
pub trait UserFunctionContext {
    fn caller(&self) -> Option<CellKey>;
    /// Set this formula cell's volatility for the current evaluation. The last executed
    /// request wins; a host that only performs explicit calls may ignore scheduling effects.
    fn set_volatile(&mut self, _volatile: bool) {}
    /// Record a host capability refusal. It must survive VBA error handlers and formula IFERROR.
    fn refuse(&mut self, reason: &'static str) -> String;
    /// Record the adapter that owns an absent capability. Older adapters keep their default
    /// owner through `refuse`; native adapters can identify their own work package.
    fn refuse_by(&mut self, reason: &'static str, _owner: &'static str) -> String {
        self.refuse(reason)
    }
    fn values_of(&mut self, reference: &ReferenceLike) -> Result<CalcValue, String>;
    /// Invoke the ordinary CalcFunc catalogue through the evaluator's existing call route.
    fn worksheet_function(
        &mut self,
        name: &str,
        arguments: &[CalcValue],
    ) -> Result<CalcValue, String>;
    /// The same worksheet callback, retaining the native adapter that owns an absent service.
    fn worksheet_function_for(
        &mut self,
        name: &str,
        arguments: &[CalcValue],
        _owner: &'static str,
    ) -> Result<CalcValue, String> {
        self.worksheet_function(name, arguments)
    }
}

/// Per-host registrations, matched by the shared workbook-name comparison. Built-ins and local formula names keep their
/// existing precedence. Hosts replace this registry atomically when loading a new module.
#[derive(Clone, Debug, Default)]
pub struct UserFunctions {
    entries: BTreeMap<(Option<String>, String), Registration>,
}

#[derive(Clone, Debug)]
struct Registration {
    /// Retain the inventory spelling separately from its shared comparison key.
    name: String,
    function: Rc<dyn UserFunction>,
}

impl UserFunctions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reject duplicates; callers can build a replacement registry before publishing it.
    pub fn register(&mut self, name: &str, function: Rc<dyn UserFunction>) -> Result<(), String> {
        self.register_for(None, name, function)
    }

    /// Register a procedure at its exact provider target, as retained by the source link.
    /// `None` is this workbook. Module qualification stays in `name`; provider paths are
    /// never reduced to a file name or searched for on disk.
    pub fn register_for(
        &mut self,
        provider: Option<&str>,
        name: &str,
        function: Rc<dyn UserFunction>,
    ) -> Result<(), String> {
        let key = Self::key(provider, name);
        if key.1.is_empty() || provider == Some("") || self.entries.contains_key(&key) {
            return Err(format!("duplicate or empty user function: {name}"));
        }
        self.entries.insert(
            key,
            Registration {
                name: name.to_ascii_uppercase(),
                function,
            },
        );
        Ok(())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.contains_for(None, name)
    }

    pub fn contains_for(&self, provider: Option<&str>, name: &str) -> bool {
        self.entries.contains_key(&Self::key(provider, name))
    }

    fn key(provider: Option<&str>, name: &str) -> (Option<String>, String) {
        (
            provider.map(str::to_owned),
            crate::value::fold_workbook_name(name),
        )
    }

    /// Names registered in this workbook, including explicit module qualifications.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries
            .iter()
            .filter_map(|((provider, _), entry)| provider.is_none().then_some(entry.name.as_str()))
    }

    /// All registrations, retaining external provider identity for host inventories.
    pub fn registrations(&self) -> impl Iterator<Item = (Option<&str>, &str)> {
        self.entries
            .iter()
            .map(|((provider, _), entry)| (provider.as_deref(), entry.name.as_str()))
    }

    /// Invoke with live reference/caller services. A missing function remains #NAME?.
    pub fn call_with_context(
        &self,
        name: &str,
        arguments: &[CalcValue],
        context: &mut dyn UserFunctionContext,
    ) -> Result<CalcValue, String> {
        self.call_for_with_context(None, name, arguments, context)
    }

    /// Invoke through the same adapter route, retaining the provider selected at binding.
    pub fn call_for_with_context(
        &self,
        provider: Option<&str>,
        name: &str,
        arguments: &[CalcValue],
        context: &mut dyn UserFunctionContext,
    ) -> Result<CalcValue, String> {
        let value = match self.entries.get(&Self::key(provider, name)) {
            Some(entry) => entry.function.call_with_context(arguments, context)?,
            None => CalcValue::error(WorksheetErrorCode::Name),
        };
        if value.is_missing() {
            Err("a user function returned a missing argument".into())
        } else {
            Ok(value)
        }
    }

    /// Missing registrations yield #NAME?, including a plan retained after module removal.
    pub fn call(&self, name: &str, arguments: &[CalcValue]) -> Result<CalcValue, String> {
        match self.entries.get(&Self::key(None, name)) {
            Some(entry) => {
                let value = entry.function.call(arguments)?;
                if value.is_missing() {
                    Err("a user function returned a missing argument".to_owned())
                } else {
                    Ok(value)
                }
            }
            None => Ok(CalcValue::error(WorksheetErrorCode::Name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Twice;
    impl UserFunction for Twice {
        fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String> {
            match arguments {
                [a] => a
                    .as_number()
                    .map(|n| CalcValue::number(n * 2.0))
                    .ok_or("number required".into()),
                _ => Err("one argument required".into()),
            }
        }
    }

    #[derive(Debug)]
    struct ReturnsMissing;
    impl UserFunction for ReturnsMissing {
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            Ok(CalcValue::missing())
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_generic_adapter_cannot_publish_missing_as_a_value() {
        let mut functions = UserFunctions::new();
        functions
            .register("MissingResult", Rc::new(ReturnsMissing))
            .unwrap();
        assert!(
            functions
                .call("MissingResult", &[])
                .unwrap_err()
                .contains("missing argument")
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn registration_is_local_case_insensitive_and_rejects_duplicates() {
        let mut functions = UserFunctions::new();
        functions.register("Twice", Rc::new(Twice)).unwrap();
        assert!(functions.register("tWICE", Rc::new(Twice)).is_err());
        assert!(functions.register("", Rc::new(Twice)).is_err());
        assert_eq!(
            functions.call("twice", &[CalcValue::number(21.0)]).unwrap(),
            CalcValue::number(42.0)
        );
        assert!(functions.call("TWICE", &[]).is_err());
        assert_eq!(
            UserFunctions::new().call("Twice", &[]).unwrap(),
            CalcValue::error(WorksheetErrorCode::Name)
        );
    }
}
