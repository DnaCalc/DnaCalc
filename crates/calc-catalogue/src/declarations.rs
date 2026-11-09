//! Data types used by function declarations.

use std::fmt;

/// The stable identity of a catalogue entry: a family code and a member number within that
/// family, both written in the family's fragment and never derived from an entry's position.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionId(u32);

impl FunctionId {
    pub const fn new(family: u16, member: u16) -> Self {
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

    pub const fn family(self) -> u16 {
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


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingForm { Let, Lambda }

/// A deferred implementation category and its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deferral { ExternalService, Python, ListsInCells }

impl Deferral {
    pub const fn owner(self) -> &'static str {
        match self {
            Self::ExternalService | Self::Python => "dc-fetch-f0tp",
            Self::ListsInCells => "dc-nk93",
        }
    }
}

/// Availability supported by the current observations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    /// The first supported build or compatibility level has not been recorded here.
    Unspecified,
    /// Insider Beta with workbook compatibility version 3.
    ListsInCellsBeta,
    /// The name is in the catalogue and Excel on Windows (16.0.20527) does not hold it: every
    /// call answers #NAME?, the answer a missing name gets.
    AbsentOnWindows,
    /// Excel runs the name only on a macro sheet, in a defined name or through
    /// `ExecuteExcel4Macro`: a worksheet formula holding it is refused at entry (`Range.Formula`
    /// and `Formula2` throw 0x800A03EC) and a file holding it does not open (CALL and
    /// REGISTER.ID, Excel 16.0.20527).
    MacroSheetOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Implementation {
    Held,
    Binding,
    NotHeld { owner: &'static str },
    Deferred { class: Deferral },
}

/// Positions of result operands whose computed blank origin is retained.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PairedResult {
    #[default]
    None,
    Odd,
    EvenAfterSelector,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WholeResultPolicy { #[default] None, Conditions, Selector }

/// The measured treatment of a 3-D reference at an argument position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreeDPlace { Unknown, Take, Spread, ValueError, ReferenceError }

/// dc-10y: Excel 16.0.20430, 2026-10-25, `Range.Formula`, three sheets with 1, 2 and 3 in A1.
/// `SUM(Ja:Ma!A1)` takes the three sheet values; `INDEX(Ja:Ma!A1,1)` gives #VALUE!;
/// arithmetic functions at measured scalar places give #REF!. The catalogue oracle's ThreeD
/// sheet records each measured position and its #REF! control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreeD {
    take: Option<&'static [usize]>,
    value: Option<&'static [usize]>,
    reference: Option<&'static [usize]>,
}

impl Default for ThreeD {
    fn default() -> Self { Self::NONE }
}

impl ThreeD {
    const NONE: Self = Self { take: None, value: None, reference: None };
    pub const fn new(take: Option<&'static [usize]>, value: Option<&'static [usize]>, reference: Option<&'static [usize]>) -> Self {
        Self { take, value, reference }
    }
    fn has(positions: Option<&[usize]>, index: usize) -> bool {
        match positions {
            None => false,
            Some([]) => true,
            Some([.., start, usize::MAX]) => index >= *start || positions.unwrap().contains(&index),
            Some(positions) => positions.contains(&index),
        }
    }
    fn is_tail(positions: Option<&[usize]>, index: usize) -> bool {
        match positions {
            Some([]) => true,
            Some([.., start, usize::MAX]) => index >= *start,
            _ => false,
        }
    }
    pub fn at(self, index: usize) -> ThreeDPlace {
        if Self::has(self.value, index) { ThreeDPlace::ValueError }
        else if Self::has(self.reference, index) { ThreeDPlace::ReferenceError }
        else if Self::has(self.take, index) {
            if Self::is_tail(self.take, index) { ThreeDPlace::Spread } else { ThreeDPlace::Take }
        } else { ThreeDPlace::Unknown }
    }
}

/// The provider keys of the host routes.
pub const FETCH_PROVIDER: &str = "dnacalc:fetch";
pub const IMPORT_PROVIDER: &str = "dnacalc:import";

/// The kind of fetch performed for a worksheet call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FetchKind {
    /// WEBSERVICE: the body as worksheet text.
    Text,
    /// IMAGE: the body as a picture.
    Picture,
}

/// The kind of file import performed for a worksheet call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImportKind {
    /// IMPORTCSV: comma-delimited, UTF-8.
    Csv,
    /// IMPORTTEXT: any delimiter and encoding.
    Text,
}

/// The owner and syntax of a catalogue entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Worksheet,
    Operator { file_spelling: Option<&'static str> },
    Binding(BindingForm),
    AddIn { library: &'static str, export: &'static str },
    HostRoute { provider: &'static str, route: HostRouteKind },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostRouteKind { Fetch(FetchKind), Import(ImportKind), Rtd }

/// Measured behavior of a catalogue entry that is independent of its parameter classes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntryTraits {
    /// Intersect the result in a legacy formula outside an array place.
    pub intersect_legacy_result: bool,
    /// Refuse a multi-cell reference at a scalar parameter.
    pub reject_multi_cell_reference: bool,
    /// Produce missing-coordinate errors before invoking the kernel.
    pub padding_precedes_call: bool,
    /// The function can exclude nested subtotal cells; its arguments choose the policy.
    pub can_skip_nested_subtotals: bool,
    /// A call to this entry is itself a nested subtotal for another call.
    pub is_subtotal: bool,
    /// In a dynamic formula, run once for each reference in an array of references.
    pub lift_dynamic_references: bool,
    /// Read an array of references as a whole.
    pub take_reference_array: bool,
    /// Lift a computed array even when this entry takes a reference whole.
    pub lift_computed_array: bool,
    pub paired_result: PairedResult,
    pub whole_result_policy: WholeResultPolicy,
    pub zero_array_blanks: bool,
    pub zero_lifted_blank: bool,
    pub zero_dynamic_lifted_blank: bool,
    pub volatile: bool,
    pub flush_subnormal_inputs: bool,
    pub retain_overflow_number: bool,
    pub retain_subnormal_output: bool,
    pub strict_number_arguments: bool,
    pub three_d: ThreeD,
}

impl EntryTraits {
    pub const NONE: Self = Self {
        intersect_legacy_result: false,
        reject_multi_cell_reference: false,
        padding_precedes_call: false,
        can_skip_nested_subtotals: false,
        is_subtotal: false,
        lift_dynamic_references: false,
        take_reference_array: false,
        lift_computed_array: false,
        paired_result: PairedResult::None,
        whole_result_policy: WholeResultPolicy::None,
        zero_array_blanks: false,
        zero_lifted_blank: false,
        zero_dynamic_lifted_blank: false,
        volatile: false,
        flush_subnormal_inputs: false,
        retain_overflow_number: false,
        retain_subnormal_output: false,
        strict_number_arguments: false,
        three_d: ThreeD::NONE,
    };

    pub const fn intersect_legacy_result(mut self) -> Self { self.intersect_legacy_result = true; self }
    pub const fn reject_multi_cell_reference(mut self) -> Self { self.reject_multi_cell_reference = true; self }
    pub const fn padding_precedes_call(mut self) -> Self { self.padding_precedes_call = true; self }
    pub const fn skip_nested_subtotals(mut self) -> Self { self.can_skip_nested_subtotals = true; self.is_subtotal = true; self }
    pub const fn lift_dynamic_references(mut self) -> Self { self.lift_dynamic_references = true; self }
    pub const fn take_reference_array(mut self) -> Self { self.take_reference_array = true; self }
    pub const fn lift_computed_array(mut self) -> Self { self.lift_computed_array = true; self }
    pub const fn if_blanks(mut self) -> Self { self.zero_array_blanks = true; self.zero_lifted_blank = true; self }
    pub const fn choose_blanks(mut self) -> Self { self.zero_dynamic_lifted_blank = true; self }
    pub const fn paired_result(mut self, paired: PairedResult) -> Self { self.paired_result = paired; self }
    pub const fn whole_result(mut self, policy: WholeResultPolicy) -> Self { self.whole_result_policy = policy; self }
    pub const fn volatile(mut self) -> Self { self.volatile = true; self }
    pub const fn flush_subnormal_inputs(mut self) -> Self { self.flush_subnormal_inputs = true; self }
    pub const fn retain_overflow_number(mut self) -> Self { self.retain_overflow_number = true; self }
    pub const fn retain_subnormal_output(mut self) -> Self { self.retain_subnormal_output = true; self }
    pub const fn strict_number_arguments(mut self) -> Self { self.strict_number_arguments = true; self }
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

/// One declared parameter: its name, as signature help shows it, its class (how the evaluator
/// passes it) and its entry class (what Excel's formula entry accepts there).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: &'static str,
    pub class: ParameterClass,
    pub entry: EntryClass,
    pub legacy: LegacyPlace,
}

/// Measured treatment of a reference supplied to a parameter in a legacy formula.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyPlace {
    /// No special reference intersection is declared.
    Default,
    /// Intersect a reference except below an array place.
    IntersectReference,
    /// Intersect a reference even below an array place.
    IntersectReferenceAlways,
    /// Intersect a value or reference even below an array place.
    SingleValue,
    /// Lift a range or array even below an array place.
    ArrayAlways,
    /// As `ArrayAlways`, including repeated arguments.
    ArrayAlwaysRepeated,
    /// Lift below this reference parameter and run once per reference.
    ArrayAlwaysLiftReferences,
    /// As `ArrayAlwaysLiftReferences`, including repeated arguments.
    ArrayAlwaysLiftReferencesRepeated,
    /// Lift below the parameter at the root or an aggregate place.
    ArrayAtRoot,
    /// As `ArrayAtRoot`, including repeated arguments.
    ArrayAtRootRepeated,
    /// Intersect ranges while lifting arrays at the root or an aggregate place.
    AggregateAtRoot,
    /// As `AggregateAtRoot`, including repeated arguments.
    AggregateAtRootRepeated,
    /// As `AggregateAtRoot`, and lift an operator below an array place.
    AggregateUnderArray,
    /// As `AggregateUnderArray`, including repeated arguments.
    AggregateUnderArrayRepeated,
    /// An aggregate place for this parameter and the next argument only.
    AggregateAtRootTwice,
    /// Aggregate at the root, and lift below an array place.
    AggregateRootArrayNested,
    /// As `AggregateRootArrayNested`, also running once per reference.
    AggregateRootArrayNestedLiftReferences,
    /// The default value place, running once per reference.
    LiftReferences,
    /// Aggregate at the root and intersect a reference outside an array place.
    AggregateIntersectReference,
    /// Aggregate and intersect a reference even below an array place.
    AggregateAlwaysIntersectReference,
}

impl Parameter {
    /// A parameter of class `class` whose entry takes every operand.
    pub const fn of(name: &'static str, class: ParameterClass) -> Self {
        Parameter {
            name,
            class,
            entry: EntryClass::Any,
            legacy: LegacyPlace::Default,
        }
    }

    pub const fn value(name: &'static str) -> Self {
        Self::of(name, ParameterClass::Value)
    }

    pub const fn value_or_reference(name: &'static str) -> Self {
        Self::of(name, ParameterClass::ValueOrReference)
    }

    pub const fn reference(name: &'static str) -> Self {
        Self::of(name, ParameterClass::Reference)
    }

    /// This parameter with the entry class `entry`.
    pub const fn entry(mut self, entry: EntryClass) -> Self {
        self.entry = entry;
        self
    }

    pub const fn legacy(mut self, legacy: LegacyPlace) -> Self {
        self.legacy = legacy;
        self
    }
}


/// What Excel's formula entry accepts in a parameter. The default is [`EntryClass::Any`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryClass {
    /// Every operand form.
    Any,
    /// Only an operand that may give a reference ([`OperandForm::fills_reference_place`]).
    Reference,
}

/// What a call of an entry may give, as Excel's formula entry reads it. The default is
/// [`ResultForm::Value`]. Entry treats both other forms alike: such a call fills a reference
/// place and may be called.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResultForm {
    /// No measured result form is held for this entry.
    Unknown,
    /// A value: never a reference, never a LAMBDA.
    Value,
    /// A value or a reference (INDEX, OFFSET, INDIRECT, XLOOKUP).
    MayBeReference,
    /// A value, a reference or a LAMBDA: one of its arguments, as IF and CHOOSE return.
    MayBeEither,
}

impl ResultForm {
    /// Whether the result may be a reference or a LAMBDA.
    pub const fn may_be_reference_or_callable(self) -> bool {
        !matches!(self, ResultForm::Value)
    }
}

/// An entry's repeating groups of arguments: from argument `from` on, the arguments come in groups
/// of `size`, each argument taking the declared parameter at `from + (i - from) % size`, and at
/// most `tail` arguments may follow the last complete group (SWITCH's default). The first group's
/// completeness is the arity's: SORTBY takes `(array, by_array1)` without its sort order, but
/// not a later group without one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArgumentGroups {
    pub from: usize,
    pub size: usize,
    pub tail: usize,
}

impl ArgumentGroups {
    pub const fn new(from: usize, size: usize, tail: usize) -> Self {
        ArgumentGroups { from, size, tail }
    }

    /// Whether `count` arguments end in an incomplete group.
    pub const fn incomplete(self, count: usize) -> bool {
        let rest = count.saturating_sub(self.from);
        rest > self.size && rest % self.size > self.tail
    }

    /// The declared parameter argument `index` takes, when it lies in a group.
    pub const fn parameter(self, index: usize) -> Option<usize> {
        if index < self.from {
            None
        } else {
            Some(self.from + (index - self.from) % self.size)
        }
    }
}

/// A translated display name with its locale identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalizedDisplayName {
    pub locale: &'static str,
    pub spelling: &'static str,
}

/// A service a kernel may use when it evaluates a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextFacility { NumericMode, Cells, Random, ReferenceText, Environment, Host, Callables }

/// Data for a worksheet function whose executable kernel is attached by the caller.
#[derive(Clone, Copy, Debug)]
pub struct WorksheetDeclaration {
    /// `None` means translated names have not been recorded for this entry.
    pub localized_names: Option<&'static [LocalizedDisplayName]>,
    /// `None` means the complete set of facilities has not been classified.
    pub context_facilities: Option<&'static [ContextFacility]>,
    pub id: FunctionId,
    pub kind: EntryKind,
    pub spelling: &'static str,
    pub notation: Notation,
    pub availability: Availability,
    pub arity: calc_value::CallableArityShape,
    pub groups: Option<ArgumentGroups>,
    pub parameters: &'static [Parameter],
    pub result: ResultForm,
    pub demand: ArgumentDemand,
    pub traits: EntryTraits,
}

impl WorksheetDeclaration {
    pub const fn with_result(mut self, result: ResultForm) -> Self {
        self.result = result;
        self
    }
    pub const fn with_groups(mut self, groups: ArgumentGroups) -> Self {
        self.groups = Some(groups);
        self
    }
    pub const fn with_traits(mut self, traits: EntryTraits) -> Self {
        let three_d = self.traits.three_d;
        self.traits = traits;
        self.traits.three_d = three_d;
        self
    }
    pub const fn with_three_d(mut self, three_d: ThreeD) -> Self {
        self.traits.three_d = three_d;
        self
    }
}
