//! The function catalogue: one logical, declarative catalogue assembled from per-family
//! fragments, and the views generated from it.
//!
//! Each family fragment (the child modules declared below, one file per family) declares its
//! entries with explicit stable [`FunctionId`]s: an entry's spelling and [`Notation`], arity,
//! parameters, [`ArgumentDemand`] and kernel, and holds the kernels themselves. At compile time
//! the generator below combines the fragments into an ID-ordered dispatch table and a
//! surface-ordered name table, and the build fails when two entries share an ID or a surface, or
//! when an entry's declaration contradicts itself. Signature help and the oracle formula are
//! rendered from the same declarations. The catalogue is immutable data built by the compiler:
//! nothing is assembled at run time and no table is mutable.

use crate::call::{EagerKernel, LazyKernel};
use crate::value::CallableArityShape;
use std::cmp::Ordering;
use std::fmt;

#[macro_use]
mod kit;

mod aggregate;
mod arithmetic;
mod arrays;
mod bonds;
mod comparison;
mod conditional;
mod database;
mod dates;
mod distributions;
mod engineering;
mod error_fallback;
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
mod reference_text;
mod random;
mod rounding;
mod rounding_more;
mod statistics;
mod subtotal;
mod text;
mod text_format;
mod text_functions;

/// The family fragments that make up the catalogue. A new family adds its module above and one
/// line here; its entries stay in its own file.
const FAMILIES: [&Family; 31] = [
    &arithmetic::FAMILY,
    &comparison::FAMILY,
    &text::FAMILY,
    &aggregate::FAMILY,
    &logical::FAMILY,
    &rounding::FAMILY,
    &conditional::FAMILY,
    &error_fallback::FAMILY,
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

/// The stable identity of a catalogue entry: a family code and a member number within that
/// family, both written in the family's fragment and never derived from an entry's position.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionId(u32);

impl FunctionId {
    pub(crate) const fn new(family: u16, member: u16) -> Self {
        FunctionId(((family as u32) << 16) | member as u32)
    }

    /// The ID whose raw value is `raw`, as [`FunctionId::raw`] gave it (an ID kept outside the
    /// process). [`Catalogue::resolve`] says whether the catalogue admits it.
    pub const fn from_raw(raw: u32) -> Self {
        FunctionId(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    const fn family(self) -> u16 {
        (self.0 >> 16) as u16
    }
}

impl fmt::Debug for FunctionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FunctionId({}.{})", self.0 >> 16, self.0 & 0xFFFF)
    }
}

/// How an entry is written in a formula.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Notation {
    /// A named call, as in `SUM(1,2)`.
    Call,
    /// A prefix operator, as in `-1`.
    Prefix,
    /// An infix operator, as in `1+2`.
    Infix,
    /// A postfix operator, as in `50%`.
    Postfix,
}

/// What a parameter admits at the call interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParameterClass {
    /// A value: the caller dereferences a reference before the call, so the entry never sees one.
    Value,
    /// A value or a reference: a reference reaches the entry as a reference.
    ValueOrReference,
    /// A reference and nothing else.
    Reference,
}

/// When an entry's arguments are evaluated. The caller schedules the evaluation; which held-back
/// argument is taken is the entry's own decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArgumentDemand {
    /// Every argument is evaluated before the call.
    Eager,
    /// IF's and CHOOSE's shape: argument 0 decides, and then at most one of the other arguments is
    /// evaluated.
    BranchOnCondition,
    /// IFERROR's and IFNA's shape: argument 0 is always evaluated, and argument 1 only when
    /// argument 0 is an error the entry catches; which errors it catches is the entry's own rule.
    FallbackOnError,
}

impl ArgumentDemand {
    /// Whether any argument is held back from evaluation before the call.
    pub const fn is_lazy(self) -> bool {
        match self {
            Self::Eager => false,
            Self::BranchOnCondition | Self::FallbackOnError => true,
        }
    }

    /// The argument the caller evaluates first because it selects what else is evaluated, or
    /// `None` when every argument is evaluated before the call.
    pub const fn discriminator(self) -> Option<usize> {
        match self {
            Self::Eager => None,
            Self::BranchOnCondition | Self::FallbackOnError => Some(0),
        }
    }

    /// Whether argument `index` is evaluated only when the entry selects it.
    pub const fn holds_back(self, index: usize) -> bool {
        match self {
            Self::Eager => false,
            Self::BranchOnCondition | Self::FallbackOnError => index > 0,
        }
    }
}

/// One declared parameter: its name, as signature help shows it, and its class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Parameter {
    pub(crate) name: &'static str,
    pub(crate) class: ParameterClass,
}

impl Parameter {
    pub(crate) const fn value(name: &'static str) -> Self {
        Parameter {
            name,
            class: ParameterClass::Value,
        }
    }

    pub(crate) const fn value_or_reference(name: &'static str) -> Self {
        Parameter {
            name,
            class: ParameterClass::ValueOrReference,
        }
    }

    pub(crate) const fn reference(name: &'static str) -> Self {
        Parameter {
            name,
            class: ParameterClass::Reference,
        }
    }
}

/// One entry as its family fragment declares it.
#[derive(Debug)]
pub(crate) struct CatalogueEntry {
    pub(crate) id: FunctionId,
    /// The spelling in an English formula: upper case for a named call, the token for an
    /// operator.
    pub(crate) spelling: &'static str,
    pub(crate) notation: Notation,
    pub(crate) arity: CallableArityShape,
    /// The named parameters. When the arity admits more arguments than there are names, the last
    /// parameter repeats.
    pub(crate) parameters: &'static [Parameter],
    pub(crate) demand: ArgumentDemand,
    /// The kernel: eager for an eager entry and lazy for a lazy one.
    pub(crate) kernel: Kernel,
}

/// How an entry computes.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Kernel {
    /// Receives every argument evaluated and admitted.
    Eager(EagerKernel),
    /// Asks for the arguments it selects, so an argument it does not select is never evaluated.
    Lazy(LazyKernel),
}

impl CatalogueEntry {
    /// An eager operator, whose arity is its parameter count.
    pub(crate) const fn operator(
        id: FunctionId,
        spelling: &'static str,
        notation: Notation,
        parameters: &'static [Parameter],
        kernel: EagerKernel,
    ) -> Self {
        CatalogueEntry {
            id,
            spelling,
            notation,
            arity: CallableArityShape::exact(parameters.len()),
            parameters,
            demand: ArgumentDemand::Eager,
            kernel: Kernel::Eager(kernel),
        }
    }

    /// The class of argument `index`, which the arity admits.
    pub(crate) fn parameter_class(&self, index: usize) -> ParameterClass {
        let last = self.parameters.len().saturating_sub(1);
        self.parameters[index.min(last)].class
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
    if let Some(first) = entry.demand.discriminator()
        && first >= arity.min
    {
        panic!("a lazy entry's discriminator is not a required argument");
    }
    match entry.kernel {
        Kernel::Eager(_) if entry.demand.is_lazy() => panic!(
            "a lazy entry has an eager kernel, which would receive its held-back arguments evaluated"
        ),
        Kernel::Lazy(_) if !entry.demand.is_lazy() => {
            panic!("an eager entry has a lazy kernel, against its declared argument demand")
        }
        Kernel::Eager(_) | Kernel::Lazy(_) => {}
    }
}

/// The order of the generated tables: -1, 0 or 1.
const fn compare(a: &CatalogueEntry, b: &CatalogueEntry, by_surface: bool) -> i32 {
    if !by_surface {
        return order_of(a.id.0 as u64, b.id.0 as u64);
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

    /// Every entry of the dispatch table, in ID order.
    pub fn targets(&self) -> impl Iterator<Item = FunctionCallTarget<'_>> + '_ {
        self.by_id.iter().map(|entry| FunctionCallTarget { entry })
    }

    /// Every entry of the name table, ordered by notation and then by spelling.
    pub fn surfaces(&self) -> impl Iterator<Item = FunctionCallTarget<'_>> + '_ {
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
    pub fn id(self) -> FunctionId {
        self.entry.id
    }

    pub fn spelling(self) -> &'c str {
        self.entry.spelling
    }

    pub fn notation(self) -> Notation {
        self.entry.notation
    }

    pub fn arity(self) -> CallableArityShape {
        self.entry.arity
    }

    /// The class of argument `index`, or `None` when the arity admits no argument there.
    pub fn parameter_class(self, index: usize) -> Option<ParameterClass> {
        (index < self.entry.arity.max).then(|| self.entry.parameter_class(index))
    }

    pub fn argument_demand(self) -> ArgumentDemand {
        self.entry.demand
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
