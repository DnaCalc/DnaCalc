//! The host callable table: names bind once to checked slots; execution retains leases.
use calc_func::value::{CalcValue, WorksheetErrorCode};
use calc_func::{UserFunction, UserFunctionContext};
use std::{collections::BTreeMap, rc::Rc};

/// Per-host registrations, matched by the shared workbook-name comparison. Built-ins and local formula names keep their
/// existing precedence. Hosts replace this registry atomically when loading a new module.
pub use calc_func::UserCallableIdentity as CallableHandle;

/// Binding-only projection; no session or thread-bound executable owners.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallableBindings {
    entries: BTreeMap<(Option<String>, String), CallableHandle>,
    provisional: Vec<CallableHandle>,
    cached_readers: Vec<CallableHandle>,
    early: Vec<CallableHandle>,
    parallel: BTreeMap<CallableHandle, usize>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CallableBindingProof {
    pub provider: Option<String>,
    pub name: String,
    pub bound: Option<CallableHandle>,
}
impl CallableBindings {
    pub fn permits_early_execution(&self, handle: CallableHandle) -> bool {
        self.early.contains(&handle)
    }
    pub fn max_parallel_calls(&self, handle: CallableHandle) -> usize {
        self.parallel.get(&handle).copied().unwrap_or(1)
    }
    pub fn uses_cached_reads(&self, handle: CallableHandle) -> bool {
        self.cached_readers.contains(&handle)
    }
    pub fn permits_provisional(&self, handle: CallableHandle) -> bool {
        self.provisional.contains(&handle)
    }
    pub fn validates(&self, proof: &CallableBindingProof) -> bool {
        self.bind(proof.provider.as_deref(), &proof.name) == proof.bound
    }

    pub fn bind(&self, provider: Option<&str>, name: &str) -> Option<CallableHandle> {
        self.entries
            .get(&CallableTable::key(provider, name))
            .copied()
    }
    pub fn contains_for(&self, provider: Option<&str>, name: &str) -> bool {
        self.bind(provider, name).is_some()
    }
    pub fn contains(&self, name: &str) -> bool {
        self.contains_for(None, name)
    }
}

/// One parameter of a registered function, as its provider documented it.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CallableParameter {
    pub name: String,
    /// What the argument is for; empty when the provider gave no text.
    pub description: String,
}

/// What a person sees of a registered function: the descriptor a provider (an .xll add-in's
/// `xlfRegister` call, a VBA module) gives its callable, so completion, signature help and function
/// help can show it beside the catalogue's own functions. Nothing here affects a call.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CallableDescriptor {
    /// The name as the provider spelled it.
    pub name: String,
    /// Where it comes from, for people: `xll Name.xll`, `VBA module Module1`.
    pub origin: String,
    pub category: String,
    pub description: String,
    pub parameters: Vec<CallableParameter>,
    pub volatile: bool,
    pub thread_safe: bool,
    pub asynchronous: bool,
}

impl CallableDescriptor {
    /// `Name(first, second)`: the signature line signature help and the popup show.
    pub fn signature(&self) -> String {
        let names: Vec<&str> = self.parameters.iter().map(|p| p.name.as_str()).collect();
        format!("{}({})", self.name, names.join(", "))
    }

    /// The signature split around the parameter at `active`: the text before it, the parameter and
    /// the rest. An `active` past the last parameter marks none (the last parameter does not repeat).
    pub fn signature_pieces(&self, active: usize) -> (String, String, String) {
        let mut before = format!("{}(", self.name);
        let (mut current, mut after) = (String::new(), String::new());
        for (index, parameter) in self.parameters.iter().enumerate() {
            let separator = if index > 0 { ", " } else { "" };
            if index < active {
                before.push_str(separator);
                before.push_str(&parameter.name);
            } else if index == active {
                before.push_str(separator);
                current = parameter.name.clone();
            } else {
                after.push_str(separator);
                after.push_str(&parameter.name);
            }
        }
        after.push(')');
        (before, current, after)
    }
}

#[derive(Clone, Debug)]
pub struct CallableTable {
    identity: u64,
    entries: BTreeMap<(Option<String>, String), Registration>,
    slots: Vec<Option<Rc<dyn UserFunction>>>,
    generations: Vec<u64>,
    lineage: Rc<std::cell::RefCell<Vec<u64>>>,
    descriptors: BTreeMap<(Option<String>, String), CallableDescriptor>,
}
impl Default for CallableTable {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let identity = NEXT
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .expect("callable identity exhausted");
        Self {
            identity,
            entries: BTreeMap::new(),
            slots: Vec::new(),
            generations: Vec::new(),
            lineage: Rc::default(),
            descriptors: BTreeMap::new(),
        }
    }
}

/// Whether a reevaluation policy leaves pool parallelism and early execution as registered.
/// Status 64 (`XllUncalcedUnwind`) only ends the attempt that met a pending producer, and a
/// function that never gets 64 runs as before: Excel ran `$` reads concurrently (F.3, F.5) and
/// gave 64 on worker threads (P03). Provisional arguments observe values that a later entry
/// replaces, so those callables stay serial and late.
fn keeps_measured_concurrency(policy: Option<calc_func::UserReevaluationReason>) -> bool {
    !matches!(
        policy,
        Some(calc_func::UserReevaluationReason::ProvisionalArguments)
    )
}

#[derive(Clone, Debug)]
struct Registration {
    /// Retain the inventory spelling separately from its shared comparison key.
    name: String,
    handle: CallableHandle,
    parallel: usize,
}

impl CallableTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bindings(&self) -> CallableBindings {
        CallableBindings {
            early: self
                .entries
                .values()
                .filter_map(|entry| {
                    let function = self.resolve(entry.handle)?;
                    (function.permits_early_execution()
                        && !function.cached_reads()
                        && function.state_lane().is_none()
                        && keeps_measured_concurrency(function.reevaluation_policy())
                        && !function.permits_reentry())
                    .then_some(entry.handle)
                })
                .collect(),
            parallel: self
                .entries
                .values()
                .filter(|entry| entry.parallel > 1)
                .map(|entry| (entry.handle, entry.parallel))
                .collect(),
            cached_readers: self
                .entries
                .values()
                .filter_map(|entry| {
                    self.resolve(entry.handle)?
                        .cached_reads()
                        .then_some(entry.handle)
                })
                .collect(),
            entries: self
                .entries
                .iter()
                .map(|(key, r)| (key.clone(), r.handle))
                .collect(),
            // A cached reader (an XLL `#` registration) may also be entered before a producer it
            // names directly: Excel's `Range.Calculate` entered `QU#` once, first, and it read the
            // producer's previous value without status 64 (P03 `BU#`, x17 MyUcReadMacro).
            provisional: self
                .entries
                .values()
                .filter_map(|entry| {
                    let function = self.resolve(entry.handle)?;
                    (function.reevaluation_policy().is_some() || function.cached_reads())
                        .then_some(entry.handle)
                })
                .collect(),
        }
    }
    pub fn bind(&self, provider: Option<&str>, name: &str) -> Option<CallableHandle> {
        self.entries
            .get(&Self::key(provider, name))
            .map(|r| r.handle)
    }
    pub fn resolve(&self, handle: CallableHandle) -> Option<&Rc<dyn UserFunction>> {
        (handle.table == self.identity
            && self.generations.get(handle.slot as usize) == Some(&handle.generation))
        .then(|| self.slots.get(handle.slot as usize)?.as_ref())
        .flatten()
    }
    /// Publish replacement adapters at a boundary. Retained activations own their previous leases;
    /// a retained binding cannot call either old code or an incompatible new signature.
    pub fn replace(&mut self, replacement: Self) {
        let mut entries = BTreeMap::new();
        for (key, registration) in replacement.entries {
            let function = replacement.slots[registration.handle.slot as usize]
                .clone()
                .expect("live replacement");
            let slot = self
                .entries
                .get(&key)
                .map(|r| r.handle.slot as usize)
                .unwrap_or_else(|| {
                    self.slots.push(None);
                    self.generations.push(0);
                    self.slots.len() - 1
                });
            self.generations[slot] = self.next_generation(slot);
            self.slots[slot] = Some(function);
            entries.insert(
                key,
                Registration {
                    name: registration.name,
                    parallel: registration.parallel,
                    handle: CallableHandle {
                        table: self.identity,
                        slot: slot as u32,
                        generation: self.generations[slot],
                    },
                },
            );
        }
        for (key, old) in &self.entries {
            if !entries.contains_key(key) {
                self.slots[old.handle.slot as usize] = None;
            }
        }
        self.entries = entries;
        self.descriptors = replacement.descriptors;
    }

    fn next_generation(&self, slot: usize) -> u64 {
        let mut lineage = self.lineage.borrow_mut();
        if lineage.len() <= slot {
            lineage.resize(slot + 1, 0);
        }
        lineage[slot] = lineage[slot]
            .checked_add(1)
            .expect("callable generation exhausted");
        lineage[slot]
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
        let handle = CallableHandle {
            table: self.identity,
            slot: u32::try_from(self.slots.len()).map_err(|_| "callable slots exhausted")?,
            generation: self.next_generation(self.slots.len()),
        };
        let parallel = if function.state_lane().is_none()
            && !function.cached_reads()
            && keeps_measured_concurrency(function.reevaluation_policy())
            && !function.permits_reentry()
        {
            function.max_parallel_calls().max(1)
        } else {
            1
        };
        self.slots.push(Some(function));
        self.generations.push(handle.generation);
        self.entries.insert(
            key,
            Registration {
                name: name.to_ascii_uppercase(),
                handle,
                parallel,
            },
        );
        Ok(())
    }

    /// Registers every function of `other` here at its own provider target, as one table of a
    /// host's native add-ins and a VBA project's functions is composed. A name both tables
    /// register is refused ([`Self::register_for`]); nothing of `other` is registered then.
    pub fn add_from(&mut self, other: &CallableTable) -> Result<(), String> {
        let mut functions = Vec::new();
        for (provider, name) in other.registrations() {
            let function = other
                .bind(provider, name)
                .and_then(|handle| other.resolve(handle))
                .ok_or("a registration lost its function")?;
            let key = Self::key(provider, name);
            if self.entries.contains_key(&key) {
                return Err(format!("duplicate or empty user function: {name}"));
            }
            functions.push((provider.map(str::to_owned), name.to_owned(), Rc::clone(function)));
        }
        for (provider, name, function) in functions {
            self.register_for(provider.as_deref(), &name, function)?;
        }
        Ok(())
    }

    /// [`Self::register_for`] with the descriptor people see (completion, signature help, help).
    pub fn register_described(
        &mut self,
        provider: Option<&str>,
        name: &str,
        function: Rc<dyn UserFunction>,
        descriptor: CallableDescriptor,
    ) -> Result<(), String> {
        self.register_for(provider, name, function)?;
        self.descriptors
            .insert(Self::key(provider, name), descriptor);
        Ok(())
    }

    /// Gives a registered callable its descriptor (replacing one it had). `false` when nothing is
    /// registered there.
    pub fn describe(
        &mut self,
        provider: Option<&str>,
        name: &str,
        descriptor: CallableDescriptor,
    ) -> bool {
        let key = Self::key(provider, name);
        if !self.entries.contains_key(&key) {
            return false;
        }
        self.descriptors.insert(key, descriptor);
        true
    }

    /// The descriptor of a registered callable, when its provider gave one.
    pub fn descriptor(&self, provider: Option<&str>, name: &str) -> Option<&CallableDescriptor> {
        self.descriptors.get(&Self::key(provider, name))
    }

    /// Every descriptor of the workbook's own registrations (no external provider), in name order.
    pub fn descriptors(&self) -> impl Iterator<Item = &CallableDescriptor> {
        self.descriptors
            .iter()
            .filter(|((provider, _), _)| provider.is_none())
            .map(|(_, descriptor)| descriptor)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.contains_for(None, name)
    }

    pub fn contains_for(&self, provider: Option<&str>, name: &str) -> bool {
        self.entries.contains_key(&Self::key(provider, name))
    }

    /// Register the built-in `RTD`, which reads through the evaluation's RTD connector.
    pub fn register_rtd(&mut self) {
        let provider = crate::rtd::RTD_PROVIDER;
        self.entries
            .retain(|(held, _), _| held.as_deref() != Some(provider));
        self.register_for(Some(provider), "RTD", Rc::new(crate::rtd::RtdFunction))
            .expect("a provider cleared above");
    }

    /// Bind the fetching functions to `route`, replacing any earlier route's.
    pub fn register_fetch_route(&mut self, route: &Rc<dyn calc_func::FetchRoute>) {
        let provider = calc_func::FETCH_PROVIDER;
        self.entries
            .retain(|(held, _), _| held.as_deref() != Some(provider));
        for name in calc_func::FETCH_FUNCTIONS {
            let function = calc_func::FetchFunction::new(name, route.clone())
                .expect("a fetching function's spelling");
            self.register_for(Some(provider), name, Rc::new(function))
                .expect("a provider cleared above");
        }
    }

    /// Bind IMPORTCSV and IMPORTTEXT to `route`, replacing any earlier route's.
    pub fn register_import_route(&mut self, route: &Rc<dyn calc_func::ImportRoute>) {
        let provider = calc_func::IMPORT_PROVIDER;
        self.entries
            .retain(|(held, _), _| held.as_deref() != Some(provider));
        for name in calc_func::IMPORT_FUNCTIONS {
            let function = calc_func::ImportFunction::new(name, route.clone())
                .expect("an import function's spelling");
            self.register_for(Some(provider), name, Rc::new(function))
                .expect("a provider cleared above");
        }
    }

    fn key(provider: Option<&str>, name: &str) -> (Option<String>, String) {
        (
            provider.map(str::to_owned),
            calc_func::value::fold_workbook_name(name),
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
            Some(entry) => self
                .resolve(entry.handle)
                .expect("live registration")
                .call_with_context(arguments, context)?,
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
                let value = self
                    .resolve(entry.handle)
                    .expect("live registration")
                    .call(arguments)?;
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
    struct Permission {
        early: bool,
        cached: bool,
        lane: bool,
    }
    impl UserFunction for Permission {
        fn permits_early_execution(&self) -> bool {
            self.early
        }
        fn max_parallel_calls(&self) -> usize {
            8
        }
        fn cached_reads(&self) -> bool {
            self.cached
        }
        fn state_lane(&self) -> Option<u64> {
            self.lane.then_some(1)
        }
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            Ok(CalcValue::number(42.0))
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn early_execution_is_separate_from_parallelism_and_is_revoked_on_replacement() {
        for (early, cached, lane, allowed) in [
            (false, false, false, false),
            (true, false, false, true),
            (true, true, false, false),
            (true, false, true, false),
        ] {
            let mut table = CallableTable::new();
            table
                .register(
                    "F",
                    Rc::new(Permission {
                        early,
                        cached,
                        lane,
                    }),
                )
                .unwrap();
            let handle = table.bind(None, "F").unwrap();
            assert_eq!(table.bindings().permits_early_execution(handle), allowed);
            if !cached && !lane {
                assert_eq!(table.bindings().max_parallel_calls(handle), 8);
            }
            let lease = table.resolve(handle).unwrap().clone();
            let mut replacement = CallableTable::new();
            replacement
                .register(
                    "F",
                    Rc::new(Permission {
                        early: false,
                        cached: false,
                        lane: false,
                    }),
                )
                .unwrap();
            table.replace(replacement);
            assert!(table.resolve(handle).is_none());
            let current = table.bind(None, "F").unwrap();
            assert!(!table.bindings().permits_early_execution(current));
            assert_eq!(lease.permits_early_execution(), early);
        }
    }

    #[derive(Debug)]
    struct Unwinding(Option<calc_func::UserReevaluationReason>);
    impl UserFunction for Unwinding {
        fn permits_early_execution(&self) -> bool {
            true
        }
        fn max_parallel_calls(&self) -> usize {
            8
        }
        fn reevaluation_policy(&self) -> Option<calc_func::UserReevaluationReason> {
            self.0
        }
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            Ok(CalcValue::number(1.0))
        }
    }
    /// Answer 13 harvest trap: the status 64 policy must not switch off pool parallelism or
    /// early execution (a `$` XLL that never meets a pending producer keeps both); provisional
    /// arguments still do.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_status_64_policy_keeps_parallelism_and_early_execution() {
        for (policy, parallel, early) in [
            (None, 8, true),
            (
                Some(calc_func::UserReevaluationReason::XllUncalcedUnwind),
                8,
                true,
            ),
            (
                Some(calc_func::UserReevaluationReason::ProvisionalArguments),
                1,
                false,
            ),
        ] {
            let mut table = CallableTable::new();
            table.register("F", Rc::new(Unwinding(policy))).unwrap();
            let handle = table.bind(None, "F").unwrap();
            let bindings = table.bindings();
            assert_eq!(bindings.max_parallel_calls(handle), parallel, "{policy:?}");
            assert_eq!(bindings.permits_early_execution(handle), early, "{policy:?}");
            assert_eq!(bindings.permits_provisional(handle), policy.is_some());
        }
    }

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

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn reload_preserves_slots_and_revokes_old_handles_without_retargeting_leases() {
        let mut table = CallableTable::new();
        table.register("Twice", Rc::new(Twice)).unwrap();
        let first = table.bind(None, "Twice").unwrap();
        let lease = table.resolve(first).unwrap().clone();
        let mut next = CallableTable::new();
        next.register("Twice", Rc::new(ReturnsMissing)).unwrap();
        table.replace(next);
        let second = table.bind(None, "Twice").unwrap();
        assert_eq!(first.slot, second.slot);
        assert_eq!(second.generation, first.generation + 1);
        assert!(table.resolve(first).is_none());
        assert!(table.resolve(second).is_some());
        assert_eq!(
            lease.call(&[CalcValue::number(21.0)]).unwrap(),
            CalcValue::number(42.0)
        );
        let mut foreign = CallableTable::new();
        foreign.register("Twice", Rc::new(Twice)).unwrap();
        assert!(foreign.resolve(second).is_none());
        table.replace(CallableTable::new());
        assert!(table.resolve(second).is_none());
        let mut restored = CallableTable::new();
        restored.register("Twice", Rc::new(Twice)).unwrap();
        table.replace(restored);
        assert_ne!(table.bind(None, "Twice").unwrap(), second);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn divergent_table_clones_cannot_authorize_each_others_replacements_or_new_slots() {
        let mut first = CallableTable::new();
        first.register("Twice", Rc::new(Twice)).unwrap();
        let mut second = first.clone();
        first.register("New", Rc::new(Twice)).unwrap();
        second.register("Other", Rc::new(ReturnsMissing)).unwrap();
        assert!(second.resolve(first.bind(None, "New").unwrap()).is_none());
        let mut next = CallableTable::new();
        next.register("Twice", Rc::new(ReturnsMissing)).unwrap();
        first.replace(next);
        let mut next = CallableTable::new();
        next.register("Twice", Rc::new(Twice)).unwrap();
        second.replace(next);
        assert!(second.resolve(first.bind(None, "Twice").unwrap()).is_none());
    }

    /// Native-only: exercises distinct OS thread identity namespaces.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn callable_ids_cannot_cross_thread_owners() {
        let first = std::thread::spawn(|| {
            let mut table = CallableTable::new();
            table.register("Twice", Rc::new(Twice)).unwrap();
            table.bind(None, "Twice").unwrap()
        })
        .join()
        .unwrap();
        assert!(
            std::thread::spawn(move || {
                let mut table = CallableTable::new();
                table.register("Twice", Rc::new(ReturnsMissing)).unwrap();
                table.resolve(first).is_none()
            })
            .join()
            .unwrap()
        );
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
        let mut functions = CallableTable::new();
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
        let mut functions = CallableTable::new();
        functions.register("Twice", Rc::new(Twice)).unwrap();
        assert!(functions.register("tWICE", Rc::new(Twice)).is_err());
        assert!(functions.register("", Rc::new(Twice)).is_err());
        assert_eq!(
            functions.call("twice", &[CalcValue::number(21.0)]).unwrap(),
            CalcValue::number(42.0)
        );
        assert!(functions.call("TWICE", &[]).is_err());
        assert_eq!(
            CallableTable::new().call("Twice", &[]).unwrap(),
            CalcValue::error(WorksheetErrorCode::Name)
        );
    }
}
