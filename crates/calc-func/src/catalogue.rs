//! The function catalogue: one logical, declarative catalogue assembled from per-family
//! fragments, and the views generated from it.
//!
//! Shared declarations live in `calc-catalogue`; family fragments attach kernels to stable
//! [`FunctionId`]s. Entry metadata
//! includes spelling, notation, arity, groups, parameters, result and argument demand, with
//! Excel evidence beside the declarations. At compile time
//! the generator below combines the fragments into an ID-ordered dispatch table and a
//! surface-ordered name table, and the build fails when two entries share an ID or a surface, or
//! when an entry's declaration contradicts itself. Signature help and the oracle formula are
//! rendered from the same declarations. The catalogue is immutable data built by the compiler:
//! nothing is assembled at run time and no table is mutable.

use crate::call::{EagerKernel, LazyKernel, StartKernel};
use crate::value::{CalcValue, CallableArityShape, NumericMode};
pub(crate) use calc_catalogue::Parameter;
pub use calc_catalogue::{
    ArgumentDemand, Availability, BindingForm, Deferral, EntryTraits, FunctionId, HeldReferences,
    Implementation, LegacyPlace, Notation, PairedResult, ParameterClass, ThreeDPlace, WholeResultPolicy,
};
use std::cmp::Ordering;

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
mod financial;
mod group_pivot;
mod grouping;
mod host_facts;
mod importing;
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
    Resumable(StartKernel),
    /// Asks for the arguments it selects, so an argument it does not select is never evaluated.
    Lazy(LazyKernel),
    Binding,
    NotHeld {
        owner: &'static str,
    },
    Deferred {
        class: Deferral,
    },
}

// Every data declaration needs one attached kernel, binding, or explicit refusal. This
// runs during constant evaluation, before the generated dispatch table can be built.
const fn attachment_matches(entry: &CatalogueEntry) -> bool {
    let Some(data) = calc_catalogue::declaration(entry.id) else {
        return false;
    };
    matches!(
        (data.implementation(), entry.kernel),
        (
            Implementation::Held,
            Kernel::Eager(_) | Kernel::Resumable(_) | Kernel::Lazy(_)
        ) | (Implementation::Binding, Kernel::Binding)
            | (Implementation::NotHeld { .. }, Kernel::NotHeld { .. })
            | (Implementation::Deferred { .. }, Kernel::Deferred { .. })
    )
}

const fn check_attachments(families: &[&Family]) {
    assert!(
        entry_count(families) == calc_catalogue::DECLARATION_COUNT,
        "a catalogue declaration has no attachment"
    );
    let mut f = 0;
    while f < families.len() {
        let mut i = 0;
        while i < families[f].entries.len() {
            assert!(
                attachment_matches(&families[f].entries[i]),
                "a catalogue attachment has no matching declaration or implementation"
            );
            i += 1;
        }
        f += 1;
    }
}
const _: () = check_attachments(&FAMILIES);

impl CatalogueEntry {
    /// The declared parameter slot for argument `index`: its group's parameter when it lies in a
    /// repeating group beyond the named parameters, else the last named one.
    fn parameter_index(&self, index: usize) -> usize {
        let named = self.parameters.len();
        match self.groups.and_then(|groups| groups.parameter(index)) {
            Some(at) if index >= named => at,
            _ => index.min(named.saturating_sub(1)),
        }
    }

    /// The class of argument `index`, which the arity admits.
    pub(crate) fn parameter_class(&self, index: usize) -> ParameterClass {
        self.parameters[self.parameter_index(index)].class
    }

    /// The entry class of argument `index`, which the arity admits: its group's parameter's when
    /// it lies in a repeating group beyond the named parameters, else the last named one's.
    pub(crate) fn entry_class(&self, index: usize) -> EntryClass {
        self.parameters[self.parameter_index(index)].entry
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
        Kernel::Eager(_) | Kernel::Resumable(_) if entry.demand.is_lazy() => panic!(
            "a lazy entry has an eager kernel, which would receive its held-back arguments evaluated"
        ),
        Kernel::Lazy(_) if !entry.demand.is_lazy() => {
            panic!("an eager entry has a lazy kernel, against its declared argument demand")
        }
        Kernel::Eager(_)
        | Kernel::Resumable(_)
        | Kernel::Lazy(_)
        | Kernel::Binding
        | Kernel::NotHeld { .. }
        | Kernel::Deferred { .. } => {}
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
        /// Notation of the conflicting registration.
        notation: Notation,
        /// Spelling of the conflicting registration.
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

    /// The library and export of the add-in entry spelled `name` in the catalogue
    /// (`_EUROTOOL.EUROCONVERT`: EUROTOOL.XLAM's EUROCONVERT), which no bare call reaches.
    pub fn addin_surface(&self, name: &str) -> Option<(&'static str, &'static str)> {
        self.by_id.iter().find_map(|entry| match entry.kind {
            EntryKind::AddIn { library, export } if entry.spelling.eq_ignore_ascii_case(name) => {
                Some((library, export))
            }
            _ => None,
        })
    }

    /// The library of the add-in whose export is `name` (EUROCONVERT: EUROTOOL.XLAM).
    pub fn addin_library_of_export(&self, name: &str) -> Option<&'static str> {
        self.by_id.iter().find_map(|entry| match entry.kind {
            EntryKind::AddIn { library, export } if export.eq_ignore_ascii_case(name) => Some(library),
            _ => None,
        })
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
        self.by_id
            .iter()
            .find(|entry| match entry.kind {
                EntryKind::Operator {
                    file_spelling: Some(file),
                } => file
                    .rsplit('.')
                    .next()
                    .is_some_and(|alias| alias.eq_ignore_ascii_case(name)),
                _ => false,
            })
            .map(|entry| FunctionCallTarget { entry })
    }

    /// Every worksheet entry of the dispatch table, in ID order: all but the add-in's and the
    /// names Excel on Windows does not hold ([`Availability::AbsentOnWindows`]).
    pub fn targets(&self) -> impl Iterator<Item = FunctionCallTarget<'_>> + '_ {
        self.by_id
            .iter()
            .filter(|entry| {
                entry.spelling != euro_tools::SURFACE
                    && entry.availability != Availability::AbsentOnWindows
                    && matches!(
                        entry.kernel,
                        Kernel::Eager(_) | Kernel::Resumable(_) | Kernel::Lazy(_)
                    )
            })
            .map(|entry| FunctionCallTarget { entry })
    }

    /// Every entry of the name table, ordered by notation and then by spelling: the names Excel
    /// holds, so neither the add-in's nor an [`Availability::AbsentOnWindows`] name (JIS) is in it,
    /// though `resolve_surface` still finds the latter and it answers #NAME?.
    pub fn surfaces(&self) -> impl Iterator<Item = FunctionCallTarget<'_>> + '_ {
        self.by_surface
            .iter()
            .filter(|entry| {
                entry.spelling != euro_tools::SURFACE
                    && entry.availability != Availability::AbsentOnWindows
                    && matches!(
                        entry.kernel,
                        Kernel::Eager(_) | Kernel::Resumable(_) | Kernel::Lazy(_)
                    )
            })
            .map(|entry| FunctionCallTarget { entry })
    }

    /// All catalogue entries, including binding forms and entries without a held kernel.
    pub fn entries(&self) -> impl Iterator<Item = FunctionCallTarget<'_>> + '_ {
        self.by_surface
            .iter()
            .map(|entry| FunctionCallTarget { entry })
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

pub use calc_catalogue::{HelpParameter, SignatureHelp};

impl<'c> FunctionCallTarget<'c> {
    /// Whether this target has a cooperative kernel entry point.
    pub fn is_resumable(self) -> bool {
        matches!(self.entry.kernel, Kernel::Resumable(_))
    }
    /// Stable catalogue identity of the callable.
    pub fn id(self) -> FunctionId {
        self.entry.id
    }

    /// Spelling associated with the resolved target.
    pub fn spelling(self) -> &'c str {
        self.entry.spelling
    }

    /// Notation associated with the resolved target.
    pub fn notation(self) -> Notation {
        self.entry.notation
    }

    /// How the target participates in formula evaluation.
    pub fn kind(self) -> EntryKind {
        self.entry.kind
    }

    /// Availability of this entry in the active catalogue.
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

    /// Argument counts accepted by the target.
    pub fn arity(self) -> CallableArityShape {
        self.entry.arity
    }

    /// The class of argument `index`, or `None` when it is unmeasured or not admitted.
    pub fn parameter_class(self, index: usize) -> Option<ParameterClass> {
        if matches!(
            self.entry.kernel,
            Kernel::NotHeld { .. } | Kernel::Deferred { .. }
        ) {
            return None;
        }
        (index < self.entry.arity.max).then(|| self.entry.parameter_class(index))
    }

    /// The measured legacy rule for a parameter, including repeated parameters.
    pub fn legacy_place(self, index: usize) -> Option<LegacyPlace> {
        if matches!(
            self.entry.kernel,
            Kernel::NotHeld { .. } | Kernel::Deferred { .. }
        ) {
            return None;
        }
        if index >= self.entry.arity.max || self.entry.parameters.is_empty() {
            return None;
        }
        Some(LegacyPlace::for_grouped_position(
            self.entry.parameters,
            self.entry.groups,
            index,
        ))
    }

    /// Whether a legacy call runs once for each reference in an array of references.
    pub fn legacy_lifts_over_references(self) -> bool {
        self.entry
            .parameters
            .iter()
            .any(|parameter| parameter.legacy.lifts_references())
    }

    /// How the target selects the arguments it evaluates.
    pub fn argument_demand(self) -> ArgumentDemand {
        self.entry.demand
    }

    /// The entry's measured behavior.
    pub fn traits(self) -> EntryTraits {
        self.entry.traits
    }

    /// Whether the catalogue declares that this function uses the specified evaluation facility.
    pub fn uses_context(self, facility: calc_catalogue::ContextFacility) -> bool {
        calc_catalogue::declaration(self.id()).is_some_and(|entry| entry.uses_context(facility))
    }

    /// How a 3-D reference is handled at `index` of a call of `argc` arguments.
    pub fn three_d_place(self, index: usize, argc: usize) -> ThreeDPlace {
        self.entry.traits.three_d.at(index, argc)
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
            && logical_more::selects_whole_result(
                self.entry.traits.whole_result_policy,
                arguments,
                numeric,
            )
    }

    /// Signature and parameter descriptions for callers and editors.
    pub fn signature_help(self) -> SignatureHelp {
        calc_catalogue::declaration(self.id())
            .expect("attached function has a declaration")
            .signature_help()
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
fn original_declaration_snapshot(fixture: &str) -> std::borrow::Cow<'_, str> {
    if usize::BITS == u64::BITS {
        return std::borrow::Cow::Borrowed(fixture);
    }
    // The x64 snapshots print usize::MAX at the end of a repeated argument group.
    std::borrow::Cow::Owned(
        fixture.replace(&format!(", {}]", u64::MAX), &format!(", {}]", usize::MAX)),
    )
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
            actual.push_str(&format!(
                "{}\t{}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\t{:?}\n",
                entry.id().raw(),
                entry.spelling(),
                entry.notation(),
                entry.kind(),
                entry.availability(),
                entry.implementation(),
                entry.arity(),
                entry.result_form(),
                entry.argument_demand(),
                entry.traits(),
                entry.signature_help()
            ));
        }
        let expected = super::original_declaration_snapshot(include_str!(
            "../../calc-catalogue/tests/fixtures/catalogue-before-ported.tsv"
        ));
        assert_eq!(actual, expected.as_ref());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn shared_lookup_matches_every_attached_entry() {
        let entries: Vec<_> = calc_catalogue::declarations().collect();
        let mut ids = std::collections::BTreeSet::new();
        for data in &entries {
            assert!(ids.insert(data.id()), "duplicate ID {:?}", data.id());
            let target = super::catalogue().resolve(data.id()).unwrap();
            assert_eq!(data.spelling(), target.spelling());
            assert_eq!(data.notation(), target.notation());
            assert_eq!(data.kind(), target.kind());
            assert_eq!(data.availability(), target.availability());
            assert_eq!(data.implementation(), target.implementation());
            assert_eq!(data.arity(), target.arity());
            assert_eq!(data.groups(), target.entry.groups);
            assert_eq!(data.parameters(), target.entry.parameters);
            assert_eq!(data.result(), target.result_form());
            assert_eq!(data.demand(), target.argument_demand());
            assert_eq!(data.traits(), target.traits());
            assert_eq!(
                calc_catalogue::declaration(data.id()).unwrap().spelling(),
                target.spelling()
            );
        }
        assert_eq!(entries.len(), super::catalogue().entries().count());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn missing_or_wrong_attachments_are_rejected() {
        let mut entry = super::FAMILIES[0].entries[0];
        assert!(super::attachment_matches(&entry));
        let id = entry.id;
        entry.id = super::FunctionId::from_raw(u32::MAX);
        assert!(!super::attachment_matches(&entry));
        entry.id = id;
        entry.kernel = super::Kernel::Binding;
        assert!(!super::attachment_matches(&entry));
    }

    // Parameter behavior captured from the earlier compiled catalogue; the source declarations
    // for these families did not change before their move into the data crate.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn original_parameter_rules_equal_shared_declarations() {
        let expected =
            include_str!("../../calc-catalogue/tests/fixtures/ported-parameters-before.tsv");
        let mut actual = String::new();
        for line in expected.lines() {
            let id: u32 = line.split('\t').next().unwrap().parse().unwrap();
            let entry = super::catalogue()
                .resolve(super::FunctionId::from_raw(id))
                .unwrap();
            let count = entry.signature_help().parameters.len() + 4;
            let positions: Vec<_> = (0..count)
                .chain(std::iter::once(254))
                .map(|i| {
                    let mut class = entry.parameter_class(i);
                    // The migration capture predates the measured SORTBY order lifting.
                    if entry.id() == calc_catalogue::ported::arrays::SORTBY && i >= 2 && i % 2 == 0 {
                        assert_eq!(
                            class,
                            Some(super::ParameterClass::Value),
                            "SORTBY argument {i}"
                        );
                        class = Some(super::ParameterClass::ValueOrReference);
                    }
                    (i, class, entry.entry_class(i), entry.legacy_place(i))
                })
                .collect();
            actual.push_str(&format!(
                "{}\t{:?}\t{:?}\n",
                id,
                entry.argument_groups(),
                positions
            ));
        }
        assert_eq!(actual, expected);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn legacy_place_resolves_repeating_argument_groups() {
        static GROUPED_PARAMETERS: [super::Parameter; 3] = [
            super::Parameter {
                name: "head",
                class: super::ParameterClass::Value,
                entry: super::EntryClass::Any,
                legacy: super::LegacyPlace::Default,
                kernel_lifts: false,
            },
            super::Parameter {
                name: "pair_a",
                class: super::ParameterClass::Value,
                entry: super::EntryClass::Any,
                legacy: super::LegacyPlace::IntersectReference,
                kernel_lifts: false,
            },
            super::Parameter {
                name: "pair_b",
                class: super::ParameterClass::Value,
                entry: super::EntryClass::Any,
                legacy: super::LegacyPlace::ArrayAlways,
                kernel_lifts: false,
            },
        ];
        let mut entry = super::FAMILIES[0].entries[0];
        entry.arity = super::CallableArityShape { min: 1, max: 255 };
        entry.groups = Some(super::ArgumentGroups::new(1, 2, 0));
        entry.parameters = &GROUPED_PARAMETERS;
        let target = super::FunctionCallTarget { entry: &entry };
        assert_eq!(target.legacy_place(0), Some(super::LegacyPlace::Default));
        assert_eq!(
            target.legacy_place(1),
            Some(super::LegacyPlace::IntersectReference)
        );
        assert_eq!(
            target.legacy_place(2),
            Some(super::LegacyPlace::ArrayAlways)
        );
        assert_eq!(
            target.legacy_place(3),
            Some(super::LegacyPlace::IntersectReference)
        );
        assert_eq!(
            target.legacy_place(4),
            Some(super::LegacyPlace::ArrayAlways)
        );
        for item in super::catalogue().entries() {
            for index in [0, 1, 2, 3, 4, 5, 6, 254] {
                if let Some(got) = item.legacy_place(index) {
                    let want = super::LegacyPlace::for_grouped_position(
                        item.entry.parameters,
                        item.entry.groups,
                        index,
                    );
                    assert_eq!(got, want, "{} arg {index}", item.spelling());
                }
            }
        }
    }
}
