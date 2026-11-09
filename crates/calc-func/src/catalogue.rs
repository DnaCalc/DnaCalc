//! The function catalogue: one logical, declarative catalogue assembled from per-family
//! fragments, and the views generated from it.
//!
//! Family fragments attach kernels to stable [`FunctionId`]s. Shared declarations live in
//! calc-catalogue; the families not yet moved retain their declarations here. Entry metadata
//! includes spelling, notation, arity, groups, parameters, result and argument demand, with
//! Excel evidence beside the declarations. At compile time
//! the generator below combines the fragments into an ID-ordered dispatch table and a
//! surface-ordered name table, and the build fails when two entries share an ID or a surface, or
//! when an entry's declaration contradicts itself. Signature help and the oracle formula are
//! rendered from the same declarations. The catalogue is immutable data built by the compiler:
//! nothing is assembled at run time and no table is mutable.

use crate::call::{EagerKernel, LazyKernel, StartKernel};
use crate::value::{CalcValue, CallableArityShape, NumericMode};
use std::cmp::Ordering;
pub use calc_catalogue::{ArgumentDemand, Availability, BindingForm, Deferral, EntryTraits,
    FunctionId, Implementation, LegacyPlace, Notation, PairedResult, ParameterClass,
    ThreeDPlace, WholeResultPolicy};
pub(crate) use calc_catalogue::{Parameter, ThreeD};

#[macro_use]
mod kit;

mod aggregate;
mod arithmetic;
mod arrays;
mod bonds;
mod comparison;
pub(crate) use comparison::text_order;
mod conditional;
mod cube_functions;
mod database;
mod dates;
mod distributions;
mod engineering;
mod entry;
pub use entry::{ArgumentGroups, EntryClass, EntryRule, OperandForm, ResultForm};
mod error_fallback;
mod euro_tools;
mod fetching;
mod importing;
mod financial;
mod group_pivot;
mod grouping;
mod host_facts;
mod information;
mod lambda_helpers;
mod logical;
mod logical_more;
mod lookup;
mod math;
mod offset;
mod random;
mod reference_text;
mod rounding;
mod rounding_more;
mod statistics;
mod subtotal;
mod text;
mod text_format;
mod text_functions;
mod unheld;

/// The family fragments that make up the catalogue. A new family adds its module above and one
/// line here; its entries stay in its own file.
const FAMILIES: [&Family; 36] = [
    &arithmetic::FAMILY,
    &comparison::FAMILY,
    &text::FAMILY,
    &aggregate::FAMILY,
    &logical::FAMILY,
    &rounding::FAMILY,
    &conditional::FAMILY,
    &error_fallback::FAMILY,
    &euro_tools::FAMILY,
    &cube_functions::FAMILY,
    &fetching::FAMILY,
    &importing::FAMILY,
    &lookup::FAMILY,
    &information::FAMILY,
    &reference_text::FAMILY,
    &math::FAMILY,
    &statistics::FAMILY,
    &distributions::FAMILY,
    &engineering::FAMILY,
    &rounding_more::FAMILY,
    &subtotal::FAMILY,
    &random::FAMILY,
    &offset::FAMILY,
    &text_functions::FAMILY,
    &logical_more::FAMILY,
    &text_format::FAMILY,
    &dates::FAMILY,
    &host_facts::FAMILY,
    &database::FAMILY,
    &financial::FAMILY,
    &bonds::FAMILY,
    &arrays::FAMILY,
    &lambda_helpers::FAMILY,
    &group_pivot::FAMILY,
    &grouping::FAMILY,
    &unheld::FAMILY,
];

const ENTRY_COUNT: usize = entry_count(&FAMILIES);

/// The dispatch table: every entry, in [`FunctionId`] order.
static BY_ID: [&CatalogueEntry; ENTRY_COUNT] = generate(&FAMILIES, false);

/// The name table: every entry, ordered by notation and then by spelling.
static BY_SURFACE: [&CatalogueEntry; ENTRY_COUNT] = generate(&FAMILIES, true);

static CATALOGUE: Catalogue = Catalogue {
    by_id: &BY_ID,
    by_surface: &BY_SURFACE,
};

/// The built-in catalogue.
pub fn catalogue() -> &'static Catalogue {
    &CATALOGUE
}

pub use calc_catalogue::{EntryKind, HostRouteKind};

/// One entry as its family fragment declares it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CatalogueEntry {
    pub(crate) id: FunctionId,
    /// The spelling in an English formula: upper case for a named call, the token for an
    /// operator.
    pub(crate) spelling: &'static str,
    pub(crate) notation: Notation,
    pub(crate) kind: EntryKind,
    pub(crate) availability: Availability,
    pub(crate) arity: CallableArityShape,
    /// The repeating groups of the arguments, when they come in groups (IFS's pairs).
    pub(crate) groups: Option<ArgumentGroups>,
    /// The named parameters. When the arity admits more arguments than there are names, the last
    /// parameter repeats, or the group's parameters repeat where `groups` declares groups.
    pub(crate) parameters: &'static [Parameter],
    /// What a call may give, as formula entry reads it.
    pub(crate) result: ResultForm,
    pub(crate) demand: ArgumentDemand,
    pub(crate) traits: EntryTraits,
    /// The kernel: eager for an eager entry and lazy for a lazy one.
    pub(crate) kernel: Kernel,
}

/// How an entry computes.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Kernel {
    /// Receives every argument evaluated and admitted.
    Eager(EagerKernel),
    Resumable {
        run: EagerKernel,
        start: StartKernel,
    },
    /// Asks for the arguments it selects, so an argument it does not select is never evaluated.
    Lazy(LazyKernel),
    Binding,
    NotHeld { owner: &'static str },
    Deferred { class: Deferral },
}

impl CatalogueEntry {
    pub(crate) const fn with_traits(mut self, traits: EntryTraits) -> Self {
        let three_d = self.traits.three_d;
        self.traits = traits;
        self.traits.three_d = three_d;
        self
    }

    pub(crate) const fn with_three_d(mut self, three_d: ThreeD) -> Self {
        self.traits.three_d = three_d;
        self
    }

    pub(crate) const fn with_continuation(mut self, start: Option<StartKernel>) -> Self {
        if let Some(start) = start {
            if let Kernel::Eager(run) = self.kernel {
                self.kernel = Kernel::Resumable { run, start };
            } else {
                panic!("only eager entries carry kernel continuations");
            }
        }
        self
    }

    /// This entry with the repeating groups `groups`.
    pub(crate) const fn with_groups(mut self, groups: ArgumentGroups) -> Self {
        self.groups = Some(groups);
        self
    }

    /// This entry with the result form `result`.
    pub(crate) const fn with_result(mut self, result: ResultForm) -> Self {
        self.result = result;
        self
    }

    /// The class of argument `index`, which the arity admits.
    pub(crate) fn parameter_class(&self, index: usize) -> ParameterClass {
        let last = self.parameters.len().saturating_sub(1);
        self.parameters[index.min(last)].class
    }

    /// The entry class of argument `index`, which the arity admits: its group's parameter's when
    /// it lies in a repeating group beyond the named parameters, else the last named one's.
    pub(crate) fn entry_class(&self, index: usize) -> EntryClass {
        let named = self.parameters.len();
        let at = match self.groups.and_then(|groups| groups.parameter(index)) {
            Some(at) if index >= named => at,
            _ => index.min(named.saturating_sub(1)),
        };
        self.parameters[at].entry
    }
}

/// One family fragment: its code, which every entry's [`FunctionId`] carries, and its entries.
pub(crate) struct Family {
    pub(crate) code: u16,
    pub(crate) entries: &'static [CatalogueEntry],
}

const fn entry_count(families: &[&Family]) -> usize {
    let mut count = 0;
    let mut f = 0;
    while f < families.len() {
        count += families[f].entries.len();
        f += 1;
    }
    count
}

/// Every entry of `families`, each checked against its declaration rules, sorted by ID or by
/// surface. Evaluated by the compiler: a broken rule or a duplicate stops the build.
const fn generate<const N: usize>(
    families: &'static [&'static Family],
    by_surface: bool,
) -> [&'static CatalogueEntry; N] {
    let mut table = [&families[0].entries[0]; N];
    let mut filled = 0;
    let mut f = 0;
    while f < families.len() {
        let family = families[f];
        let mut earlier = 0;
        while earlier < f {
            if families[earlier].code == family.code {
                panic!("two catalogue families declare the same family code");
            }
            earlier += 1;
        }
        let mut e = 0;
        while e < family.entries.len() {
            let entry = &family.entries[e];
            check_declaration(family.code, entry);
            table[filled] = entry;
            filled += 1;
            e += 1;
        }
        f += 1;
    }
    if filled != N {
        panic!("the catalogue table length differs from its entry count");
    }
    let mut i = 1;
    while i < N {
        let mut j = i;
        while j > 0 && compare(table[j - 1], table[j], by_surface) > 0 {
            let moved = table[j];
            table[j] = table[j - 1];
            table[j - 1] = moved;
            j -= 1;
        }
        i += 1;
    }
    let mut k = 1;
    while k < N {
        if compare(table[k - 1], table[k], by_surface) == 0 {
            if by_surface {
                panic!("two catalogue entries declare the same notation and spelling");
            }
            panic!("two catalogue entries declare the same FunctionId");
        }
        k += 1;
    }
    table
}

/// The declaration rules every entry must keep.
const fn check_declaration(family: u16, entry: &CatalogueEntry) {
    if entry.id.family() != family {
        panic!("a catalogue entry's FunctionId lies outside its family's code");
    }
    let spelling = entry.spelling.as_bytes();
    if spelling.is_empty() {
        panic!("a catalogue entry has an empty spelling");
    }
    let arity = entry.arity;
    match entry.notation {
        Notation::Call => {
            let mut b = 0;
            while b < spelling.len() {
                let c = spelling[b];
                if !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'.' || c == b'_') {
                    panic!(
                        "a named call's spelling is not upper-case ASCII letters, digits, '.' and '_'"
                    );
                }
                b += 1;
            }
        }
        Notation::Infix => {
            if arity.min != 2 || arity.max != 2 {
                panic!("an infix operator does not take exactly two arguments");
            }
        }
        Notation::Prefix | Notation::Postfix => {
            if arity.min != 1 || arity.max != 1 {
                panic!("a prefix or postfix operator does not take exactly one argument");
            }
        }
    }
    let named = entry.parameters.len();
    if arity.min > arity.max || named < arity.min || named > arity.max {
        panic!("a catalogue entry's parameters do not cover its arity");
    }
    if arity.max > 0 && named == 0 {
        panic!("a catalogue entry that takes arguments names no parameter");
    }
    if let Some(groups) = entry.groups
        && (groups.size == 0 || groups.from + groups.size > named || groups.tail >= groups.size)
    {
        panic!("a catalogue entry's repeating groups do not lie in its named parameters");
    }
    let mut p = 0;
    while p < named {
        let parameter = entry.parameters[p];
        if matches!(parameter.entry, EntryClass::Reference)
            && matches!(parameter.class, ParameterClass::Value)
        {
            panic!("a reference-only entry place has a value parameter, which a call dereferences");
        }
        p += 1;
    }
    if let Some(first) = entry.demand.discriminator()
        && first >= arity.min
    {
        panic!("a lazy entry's discriminator is not a required argument");
    }
    match entry.kernel {
        Kernel::Eager(_) | Kernel::Resumable { .. } if entry.demand.is_lazy() => panic!(
            "a lazy entry has an eager kernel, which would receive its held-back arguments evaluated"
        ),
        Kernel::Lazy(_) if !entry.demand.is_lazy() => {
            panic!("an eager entry has a lazy kernel, against its declared argument demand")
        }
        Kernel::Eager(_) | Kernel::Resumable { .. } | Kernel::Lazy(_) |
        Kernel::Binding | Kernel::NotHeld { .. } | Kernel::Deferred { .. } => {}
    }
}

/// The order of the generated tables: -1, 0 or 1.
const fn compare(a: &CatalogueEntry, b: &CatalogueEntry, by_surface: bool) -> i32 {
    if !by_surface {
        return order_of(a.id.raw() as u64, b.id.raw() as u64);
    }
    let (left, right) = (a.notation as u64, b.notation as u64);
    if left != right {
        return order_of(left, right);
    }
    let (left, right) = (a.spelling.as_bytes(), b.spelling.as_bytes());
    let mut i = 0;
    while i < left.len() && i < right.len() {
        if left[i] != right[i] {
            return order_of(left[i] as u64, right[i] as u64);
        }
        i += 1;
    }
    order_of(left.len() as u64, right.len() as u64)
}

const fn order_of(left: u64, right: u64) -> i32 {
    if left < right {
        -1
    } else if left > right {
        1
    } else {
        0
    }
}

/// The run-time counterpart of the surface order: a named call matches its spelling in any ASCII
/// case.
fn surface_order(entry: &CatalogueEntry, notation: Notation, spelling: &str) -> Ordering {
    entry.notation.cmp(&notation).then_with(|| {
        entry
            .spelling
            .bytes()
            .cmp(spelling.bytes().map(|b| b.to_ascii_uppercase()))
    })
}

/// Why the catalogue refused to resolve an identity or a surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistrationError {
    /// No entry has this ID.
    UnknownId(FunctionId),
    /// No entry has this notation and spelling.
    UnknownSurface {
        notation: Notation,
        spelling: String,
    },
}

/// The generated catalogue: an ID-ordered dispatch table and a surface-ordered name table over
/// the same entries.
#[derive(Debug)]
pub struct Catalogue {
    by_id: &'static [&'static CatalogueEntry],
    by_surface: &'static [&'static CatalogueEntry],
}

impl Catalogue {
    /// The entry with this ID.
    pub fn resolve(&self, id: FunctionId) -> Result<FunctionCallTarget<'_>, RegistrationError> {
        self.by_id
            .binary_search_by(|entry| entry.id.cmp(&id))
            .map(|at| FunctionCallTarget {
                entry: self.by_id[at],
            })
            .map_err(|_| RegistrationError::UnknownId(id))
    }

    /// The entry written with this notation and spelling; a named call's spelling matches in any
    /// ASCII case.
    pub fn resolve_surface(
        &self,
        notation: Notation,
        spelling: &str,
    ) -> Result<FunctionCallTarget<'_>, RegistrationError> {
        if spelling.eq_ignore_ascii_case(euro_tools::SURFACE) {
            return Err(RegistrationError::UnknownSurface {
                notation,
                spelling: spelling.to_owned(),
            });
        }
        self.by_surface
            .binary_search_by(|entry| surface_order(entry, notation, spelling))
            .map(|at| FunctionCallTarget {
                entry: self.by_surface[at],
            })
            .map_err(|_| RegistrationError::UnknownSurface {
                notation,
                spelling: spelling.to_owned(),
            })
    }

    /// EUROCONVERT, which only a call qualified by a library link to the Euro Currency Tools
    /// add-in reaches. No bare spelling resolves to it and [`Catalogue::surfaces`] omits it.
    pub fn euro_tool_call(&self) -> FunctionCallTarget<'_> {
        let at = self
            .by_surface
            .binary_search_by(|entry| surface_order(entry, Notation::Call, euro_tools::SURFACE))
            .expect("the add-in's entry");
        FunctionCallTarget {
            entry: self.by_surface[at],
        }
    }

    /// Resolve a library-qualified add-in call using the entry's declared library.
    pub fn resolve_addin(&self, library: &str, name: &str) -> Option<FunctionCallTarget<'_>> {
        let file = library.rsplit(['/', '\\']).next()?;
        self.by_id.iter().find(|entry| {
            matches!(entry.kind, EntryKind::AddIn { library, export } if library.eq_ignore_ascii_case(file) && export.eq_ignore_ascii_case(name))
        }).map(|entry| FunctionCallTarget { entry })
    }

    /// Whether `name` is the export of a declared add-in entry (EUROCONVERT of EUROTOOL.XLAM). A
    /// bare call of such a name reaches no entry: Excel answers #NAME? while the add-in is not
    /// loaded, which is the state of every bare-name reading.
    pub fn is_unloaded_addin_export(&self, name: &str) -> bool {
        self.by_id.iter().any(|entry| {
            matches!(entry.kind, EntryKind::AddIn { export, .. } if export.eq_ignore_ascii_case(name))
        })
    }

    /// Resolve a file-form operator name without its file prefix.
    pub fn resolve_operator_alias(&self, name: &str) -> Option<FunctionCallTarget<'_>> {
        self.by_id.iter().find(|entry| {
            match entry.kind {
                EntryKind::Operator { file_spelling: Some(file) } => file.rsplit('.').next().is_some_and(|alias| alias.eq_ignore_ascii_case(name)),
                _ => false,
            }
        }).map(|entry| FunctionCallTarget { entry })
    }

    /// Every worksheet entry of the dispatch table, in ID order: all but the add-in's and the
    /// names Excel on Windows does not hold ([`Availability::AbsentOnWindows`]).
    pub fn targets(&self) -> impl Iterator<Item = FunctionCallTarget<'_>> + '_ {
        self.by_id
            .iter()
            .filter(|entry| entry.spelling != euro_tools::SURFACE && entry.availability != Availability::AbsentOnWindows && matches!(entry.kernel, Kernel::Eager(_) | Kernel::Resumable { .. } | Kernel::Lazy(_)))
            .map(|entry| FunctionCallTarget { entry })
    }

    /// Every entry of the name table, ordered by notation and then by spelling: the names Excel
    /// holds, so neither the add-in's nor an [`Availability::AbsentOnWindows`] name (JIS) is in it,
    /// though `resolve_surface` still finds the latter and it answers #NAME?.
    pub fn surfaces(&self) -> impl Iterator<Item = FunctionCallTarget<'_>> + '_ {
        self.by_surface
            .iter()
            .filter(|entry| entry.spelling != euro_tools::SURFACE && entry.availability != Availability::AbsentOnWindows && matches!(entry.kernel, Kernel::Eager(_) | Kernel::Resumable { .. } | Kernel::Lazy(_)))
            .map(|entry| FunctionCallTarget { entry })
    }

    /// All catalogue entries, including binding forms and entries without a held kernel.
    pub fn entries(&self) -> impl Iterator<Item = FunctionCallTarget<'_>> + '_ {
        self.by_surface.iter().map(|entry| FunctionCallTarget { entry })
    }
}

/// A resolved catalogue entry: its metadata, its signature help, its oracle formula and its
/// invocation ([`FunctionCallTarget::invoke`]).
#[derive(Clone, Copy, Debug)]
pub struct FunctionCallTarget<'c> {
    pub(crate) entry: &'c CatalogueEntry,
}

impl PartialEq for FunctionCallTarget<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.entry.id == other.entry.id
    }
}

impl Eq for FunctionCallTarget<'_> {}

/// The signature help of an entry, rendered from its declared parameters and arity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureHelp {
    /// The signature as a tooltip shows it, such as `SUM(number1, [number2], ...)`.
    pub display: String,
    pub parameters: Vec<HelpParameter>,
}

/// One parameter of a [`SignatureHelp`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HelpParameter {
    pub name: &'static str,
    /// Whether a call may leave it out: its position is at or after the arity's minimum.
    pub optional: bool,
    /// Whether it repeats: it is the last named parameter and the arity admits more arguments
    /// than there are names.
    pub repeats: bool,
}

impl<'c> FunctionCallTarget<'c> {
    pub fn is_resumable(self) -> bool {
        matches!(self.entry.kernel, Kernel::Resumable { .. })
    }
    pub fn id(self) -> FunctionId {
        self.entry.id
    }

    pub fn spelling(self) -> &'c str {
        self.entry.spelling
    }

    pub fn notation(self) -> Notation {
        self.entry.notation
    }

    pub fn kind(self) -> EntryKind {
        self.entry.kind
    }

    pub fn availability(self) -> Availability {
        self.entry.availability
    }

    /// The implementation state, including the owner of an unheld entry.
    pub fn implementation(self) -> Implementation {
        match self.entry.kernel {
            Kernel::Binding => Implementation::Binding,
            Kernel::NotHeld { owner } => Implementation::NotHeld { owner },
            Kernel::Deferred { class } => Implementation::Deferred { class },
            _ => Implementation::Held,
        }
    }

    pub fn arity(self) -> CallableArityShape {
        self.entry.arity
    }

    /// The class of argument `index`, or `None` when it is unmeasured or not admitted.
    pub fn parameter_class(self, index: usize) -> Option<ParameterClass> {
        if matches!(self.entry.kernel, Kernel::NotHeld { .. } | Kernel::Deferred { .. }) {
            return None;
        }
        (index < self.entry.arity.max).then(|| self.entry.parameter_class(index))
    }

    /// The measured legacy rule for a parameter, including repeated parameters.
    pub fn legacy_place(self, index: usize) -> Option<LegacyPlace> {
        if matches!(self.entry.kernel, Kernel::NotHeld { .. } | Kernel::Deferred { .. }) {
            return None;
        }
        if index >= self.entry.arity.max || self.entry.parameters.is_empty() {
            return None;
        }
        let last = self.entry.parameters.len() - 1;
        let legacy = self.entry.parameters[index.min(last)].legacy;
        if index > last && !matches!(legacy, LegacyPlace::ArrayAlwaysRepeated | LegacyPlace::ArrayAlwaysLiftReferencesRepeated | LegacyPlace::ArrayAtRootRepeated | LegacyPlace::AggregateAtRootRepeated | LegacyPlace::AggregateUnderArrayRepeated)
            && !(index == last + 1 && legacy == LegacyPlace::AggregateAtRootTwice) {
            return Some(LegacyPlace::Default);
        }
        Some(legacy)
    }

    /// Whether a legacy call runs once for each reference in an array of references.
    pub fn legacy_lifts_over_references(self) -> bool {
        self.entry.parameters.iter().any(|parameter| matches!(parameter.legacy,
            LegacyPlace::ArrayAlwaysLiftReferences | LegacyPlace::ArrayAlwaysLiftReferencesRepeated |
            LegacyPlace::AggregateRootArrayNestedLiftReferences | LegacyPlace::LiftReferences))
    }

    pub fn argument_demand(self) -> ArgumentDemand {
        self.entry.demand
    }

    /// The entry's measured behavior.
    pub fn traits(self) -> EntryTraits {
        self.entry.traits
    }

    /// How a 3-D reference is handled at `index`.
    pub fn three_d_place(self, index: usize) -> ThreeDPlace {
        self.entry.traits.three_d.at(index)
    }

    /// Whether an absent lifted coordinate produces #N/A before the eager kernel runs.
    /// W111's independent padding controls retain this policy for the two directed
    /// rounding functions, whose full validation order differs from argument order. DB,
    /// DDB and VDB read their required arguments before optional ones; their scalar adapters select
    /// earlier coercion errors before later padding.
    /// A formed earlier argument error still precedes padding in those named calls.
    /// Operators retain their existing policy until their error/padding order is measured.
    pub fn padding_precedes_call(self) -> bool {
        self.entry.notation != Notation::Call || self.entry.traits.padding_precedes_call
    }

    /// Whether a scalar IFS/SWITCH selector returns its chosen result whole instead of
    /// lifting over result arrays. Invalid IFS conditions retain the array error shape.
    /// This inspects values only; it neither invokes a kernel nor evaluates an argument.
    pub fn selects_whole_result(self, arguments: &[CalcValue], numeric: &NumericMode) -> bool {
        self.entry.arity.accepts(arguments.len())
            && logical_more::selects_whole_result(self.entry.traits.whole_result_policy, arguments, numeric)
    }

    pub fn signature_help(self) -> SignatureHelp {
        let entry = self.entry;
        let open_tail = entry.arity.max > entry.parameters.len();
        let last = entry.parameters.len().saturating_sub(1);
        let parameters: Vec<HelpParameter> = entry
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| HelpParameter {
                name: parameter.name,
                optional: index >= entry.arity.min,
                repeats: open_tail && index == last,
            })
            .collect();
        let shown: Vec<String> = parameters
            .iter()
            .map(|parameter| {
                if parameter.optional {
                    format!("[{}]", parameter.name)
                } else {
                    parameter.name.to_owned()
                }
            })
            .collect();
        let display = match entry.notation {
            Notation::Call => {
                let mut list = shown.join(", ");
                if open_tail {
                    list.push_str(", ...");
                }
                format!("{}({list})", entry.spelling)
            }
            Notation::Prefix => format!("{}{}", entry.spelling, shown[0]),
            Notation::Infix => format!("{} {} {}", shown[0], entry.spelling, shown[1]),
            Notation::Postfix => format!("{}{}", shown[0], entry.spelling),
        };
        SignatureHelp {
            display,
            parameters,
        }
    }

    /// The oracle selection for this entry: the Excel formula that asks Excel for this entry
    /// applied to `operands`, or `None` when the arity does not admit that many. Each operand is
    /// written as given, so a caller parenthesizes an operand that is not a single literal or
    /// reference.
    pub fn oracle_formula(self, operands: &[&str]) -> Option<String> {
        let entry = self.entry;
        if !entry.arity.accepts(operands.len()) {
            return None;
        }
        let mut formula = String::new();
        formula.push('=');
        match entry.notation {
            Notation::Call => {
                formula.push_str(entry.spelling);
                formula.push('(');
                formula.push_str(&operands.join(","));
                formula.push(')');
            }
            Notation::Prefix => {
                formula.push_str(entry.spelling);
                formula.push_str(operands[0]);
            }
            Notation::Infix => {
                formula.push_str(operands[0]);
                formula.push_str(entry.spelling);
                formula.push_str(operands[1]);
            }
            Notation::Postfix => {
                formula.push_str(operands[0]);
                formula.push_str(entry.spelling);
            }
        }
        Some(formula)
    }
}

#[cfg(test)]
mod shared_declaration_equivalence {
    // Snapshot of every entry at 9edc9030, before moving the ported family declarations.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn original_catalogue_equals_shared_declarations() {
        let mut entries: Vec<_> = super::catalogue().entries().collect();
        entries.sort_by_key(|entry| entry.id().raw());
        let mut actual = String::new();
        for entry in entries {
            actual.push_str(&format!("{}\t{}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\n",
                entry.id().raw(), entry.spelling(), entry.notation(), entry.kind(), entry.availability(),
                entry.implementation(), entry.arity(), entry.result_form(), entry.argument_demand(),
                entry.traits(), entry.signature_help()));
        }
        assert_eq!(actual, include_str!("../../calc-catalogue/tests/fixtures/catalogue-before-ported.tsv"));
    }

    // Parameter behavior captured from the earlier compiled catalogue; the source declarations
    // for these families did not change before their move into the data crate.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn original_parameter_rules_equal_shared_declarations() {
        let expected = include_str!("../../calc-catalogue/tests/fixtures/ported-parameters-before.tsv");
        let mut actual = String::new();
        for line in expected.lines() {
            let id: u32 = line.split('\t').next().unwrap().parse().unwrap();
            let entry = super::catalogue().resolve(super::FunctionId::from_raw(id)).unwrap();
            let count = entry.signature_help().parameters.len() + 4;
            let positions: Vec<_> = (0..count).chain(std::iter::once(254))
                .map(|i| (i, entry.parameter_class(i), entry.entry_class(i), entry.legacy_place(i)))
                .collect();
            actual.push_str(&format!("{}\t{:?}\t{:?}\n", id, entry.argument_groups(), positions));
        }
        assert_eq!(actual, expected);
    }

}
