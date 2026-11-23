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
pub enum BindingForm {
    Let,
    Lambda,
}

/// A deferred implementation category and its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deferral {
    ExternalService,
    Python,
    ListsInCells,
}

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
pub enum WholeResultPolicy {
    #[default]
    None,
    Conditions,
    Selector,
}

/// The measured treatment of a 3-D reference at an argument position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreeDPlace {
    Unknown,
    Take,
    Spread,
    /// The call is #VALUE!.
    ValueError,
    /// The argument is #REF!.
    ReferenceError,
    /// The argument is #VALUE!, read only where the call reads it (a branch, a fallback, a
    /// selected value).
    ArgumentValueError,
}

/// dc-10y: Excel 16.0.20430, 2026-10-25, `Range.Formula`, three sheets with 1, 2 and 3 in A1.
/// `SUM(Ja:Ma!A1)` takes the three sheet values; `INDEX(Ja:Ma!A1,1)` gives #VALUE!;
/// arithmetic functions at measured scalar places give #REF!. The catalogue oracle's ThreeD
/// sheet records each measured position and its #REF! control.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ThreeD {
    take: Option<&'static [usize]>,
    value: Option<&'static [usize]>,
    reference: Option<&'static [usize]>,
    value_argument: Option<&'static [usize]>,
    selector_pairs: bool,
}

/// `N` argument positions: `lead`, then every other position from `start` (dc-b1l: the criteria
/// and criteria ranges of COUNTIFS and SUMIFS, the conditions and values of IFS).
pub const fn alternate<const N: usize>(lead: &[usize], start: usize) -> [usize; N] {
    let mut out = [0; N];
    let mut i = 0;
    while i < lead.len() {
        out[i] = lead[i];
        i += 1;
    }
    let mut next = start;
    while i < N {
        out[i] = next;
        next += 2;
        i += 1;
    }
    out
}

// The declaration snapshots (calc-catalogue/tests/fixtures) print every entry's traits; SWITCH's
// argument-count roles show only where they are set.
impl std::fmt::Debug for ThreeD {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = f.debug_struct("ThreeD");
        out.field("take", &self.take)
            .field("value", &self.value)
            .field("reference", &self.reference)
            .field("value_argument", &self.value_argument);
        if self.selector_pairs {
            out.field("selector_pairs", &true);
        }
        out.finish()
    }
}

impl Default for ThreeD {
    fn default() -> Self {
        Self::NONE
    }
}

impl ThreeD {
    const NONE: Self = Self {
        take: None,
        value: None,
        reference: None,
        value_argument: None,
        selector_pairs: false,
    };
    pub const fn new(
        take: Option<&'static [usize]>,
        value: Option<&'static [usize]>,
        reference: Option<&'static [usize]>,
    ) -> Self {
        Self {
            take,
            value,
            reference,
            value_argument: None,
            selector_pairs: false,
        }
    }
    /// SWITCH's roles, which depend on the argument count: the expression and each compared
    /// value is a #REF! argument, each result and the default (the last of an even count) a
    /// #VALUE! argument read only when selected. dc-b1l, Excel 16.0.20527 through Touchstone
    /// sheet cases (2026-11-17, family three-d): `=SWITCH(Ja:Ma!A1,1,2)` and
    /// `=SWITCH(1,2,3,Ja:Ma!A1,4,5)` #REF!, `=SWITCH(1,1,2,Ja:Ma!A1,3)` 2, `=SWITCH(1,1,Ja:Ma!A1)`
    /// and `=SWITCH(1,2,3,Ja:Ma!A1)` #VALUE!, `=SWITCH(1,2,Ja:Ma!A1,3)` 3, `=SWITCH(1,2,Ja:Ma!A1)`
    /// #N/A.
    pub const fn selector_pairs() -> Self {
        let mut out = Self::NONE;
        out.selector_pairs = true;
        out
    }
    /// The positions where a 3-D reference is a #VALUE! argument
    /// ([`ThreeDPlace::ArgumentValueError`]).
    pub const fn value_arguments(mut self, positions: &'static [usize]) -> Self {
        self.value_argument = Some(positions);
        self
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
    /// The treatment at argument `index` of a call of `argc` arguments.
    pub fn at(self, index: usize, argc: usize) -> ThreeDPlace {
        if self.selector_pairs {
            let default = argc % 2 == 0 && index + 1 == argc;
            return if index == 0 || (index % 2 == 1 && !default) {
                ThreeDPlace::ReferenceError
            } else {
                ThreeDPlace::ArgumentValueError
            };
        }
        if Self::has(self.value, index) {
            ThreeDPlace::ValueError
        } else if Self::has(self.reference, index) {
            ThreeDPlace::ReferenceError
        } else if Self::has(self.value_argument, index) {
            ThreeDPlace::ArgumentValueError
        } else if Self::has(self.take, index) {
            if Self::is_tail(self.take, index) {
                ThreeDPlace::Spread
            } else {
                ThreeDPlace::Take
            }
        } else {
            ThreeDPlace::Unknown
        }
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
    Operator {
        file_spelling: Option<&'static str>,
    },
    Binding(BindingForm),
    AddIn {
        library: &'static str,
        export: &'static str,
    },
    HostRoute {
        provider: &'static str,
        route: HostRouteKind,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostRouteKind {
    Fetch(FetchKind),
    Import(ImportKind),
    Rtd,
}

/// How a dynamic-array call reads each reference an array of references holds (OFFSET or
/// INDIRECT lifted over an array) at a parameter that takes references, when it neither runs once
/// per reference ([`EntryTraits::lift_dynamic_references`]) nor reads the array whole
/// ([`EntryTraits::take_reference_array`]). dc-b1l, Excel 16.0.20527 through Touchstone sheet
/// cases (2026-11-16, cell-context family reference-arrays, five arrays of references over 1..6
/// in A1:A6 and 10..60 in C1:C6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeldReferences {
    /// Not measured: the call is refused.
    #[default]
    Refuse,
    /// Each reference is the value of its first cell (`=MIN(OFFSET(A1,{0;1;2},0,2))` is 1,
    /// `=STDEV(...)` 1, `=NPV(0.1,...)` 4.8159..., `=FREQUENCY(...,{2})` 2|1).
    FirstCell,
    /// Each reference is no number (`=SUMPRODUCT(OFFSET(A1,{0;1;2},0,2))` is 0).
    NoNumber,
    /// Each reference is #VALUE! (`=MEDIAN(...)` and `=LARGE(...,1)` are #VALUE!, `=SORT(...)`
    /// spills #VALUE! in each cell, `=MATCH(2,...,0)` is #N/A, `=COLUMNS(...)` 1, `=TYPE(...)` 64).
    ValueError,
    /// Each reference is #NUM! (`=CORREL(OFFSET(A1,{0;1;2},0,2),{1;2;3})` is #NUM!, and #N/A
    /// beside an array of another size; text arrays there are #DIV/0!).
    NumError,
    /// Each reference is #N/A (`=AND(OFFSET(A1,{0;1;2},0,2),TRUE)` is #N/A; a text array there
    /// is #VALUE!).
    NaError,
}

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
    /// The first argument position [`Self::lift_dynamic_references`] lifts over (0: every one);
    /// from a position above 0 a dynamic formula also lifts over any array there.
    pub lift_references_from: u8,
    /// Read an array of references as a whole.
    pub take_reference_array: bool,
    /// How a held reference of an array of references is read otherwise.
    pub held_references: HeldReferences,
    /// Lift a computed array even when this entry takes a reference whole (COUNTBLANK,
    /// W111 count-family readings and w089-axis-reference-vs-array-variant).
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
    /// Whether an admitted ordinary array drives a call. A value slot may lift an array of
    /// references even if its reference elements will be supplied as worksheet errors.
    /// The closures delay inspecting array cells and legacy parameter declarations.
    pub fn lifts_admitted_array(
        self,
        id: FunctionId,
        index: usize,
        dynamic: bool,
        value_slot: bool,
        contains_references: impl FnOnce() -> bool,
        lifts_over_references: impl FnOnce() -> bool,
    ) -> bool {
        if value_slot || self.lift_computed_array {
            return true;
        }
        let from = usize::from(self.lift_references_from);
        // AGGREGATE's ref2 and later lift ordinary arrays too.
        if dynamic && from > 0 && index >= from {
            return true;
        }
        // Excel 20527 runs a database call for each value in a dynamic criteria array.
        // The kernel determines whether each element supplies a criteria reference.
        let database_criteria =
            dynamic && index == 2 && id.family() == crate::ported::database::CODE;
        if (database_criteria || index >= from) && contains_references() {
            index >= from && lifts_over_references()
        } else {
            database_criteria
        }
    }

    pub const NONE: Self = Self {
        intersect_legacy_result: false,
        reject_multi_cell_reference: false,
        padding_precedes_call: false,
        can_skip_nested_subtotals: false,
        is_subtotal: false,
        lift_dynamic_references: false,
        lift_references_from: 0,
        take_reference_array: false,
        held_references: HeldReferences::Refuse,
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

    pub const fn intersect_legacy_result(mut self) -> Self {
        self.intersect_legacy_result = true;
        self
    }
    pub const fn reject_multi_cell_reference(mut self) -> Self {
        self.reject_multi_cell_reference = true;
        self
    }
    pub const fn padding_precedes_call(mut self) -> Self {
        self.padding_precedes_call = true;
        self
    }
    pub const fn skip_nested_subtotals(mut self) -> Self {
        self.can_skip_nested_subtotals = true;
        self.is_subtotal = true;
        self
    }
    pub const fn lift_dynamic_references(mut self) -> Self {
        self.lift_dynamic_references = true;
        self
    }
    /// AGGREGATE: in a dynamic formula, run once for each element of an array given as ref2 or
    /// later (each reference of an array of references, each value of any other array), while
    /// ref1's references are read as [`Self::held_references`] says.
    /// dc-b1l, Excel 16.0.20527 through Touchstone sheet cases (2026-11-17, family
    /// reference-arrays): `=AGGREGATE(9,0,A1:A2,OFFSET(A1,{0;1;2},0,2))` spills 6|8|10,
    /// `=AGGREGATE(9,0,A1:A2,OFFSET(A1,{0;1},0),OFFSET(C1,{0,1},0))` a 2 by 2 array,
    /// `=AGGREGATE(9,0,OFFSET(A1,{0;1},0),OFFSET(A1,{0;1},0))` #VALUE!|#VALUE! and
    /// `=AGGREGATE(9,0,OFFSET(A1,{0;1;2},0))` #VALUE!.
    pub const fn lift_references_after_ref1(mut self) -> Self {
        self.lift_dynamic_references = true;
        self.lift_references_from = 3;
        self
    }
    pub const fn take_reference_array(mut self) -> Self {
        self.take_reference_array = true;
        self
    }
    pub const fn first_cell_references(mut self) -> Self {
        self.held_references = HeldReferences::FirstCell;
        self
    }
    pub const fn no_number_references(mut self) -> Self {
        self.held_references = HeldReferences::NoNumber;
        self
    }
    pub const fn value_error_references(mut self) -> Self {
        self.held_references = HeldReferences::ValueError;
        self
    }
    pub const fn num_error_references(mut self) -> Self {
        self.held_references = HeldReferences::NumError;
        self
    }
    pub const fn na_error_references(mut self) -> Self {
        self.held_references = HeldReferences::NaError;
        self
    }
    pub const fn lift_computed_array(mut self) -> Self {
        self.lift_computed_array = true;
        self
    }
    pub const fn if_blanks(mut self) -> Self {
        self.zero_array_blanks = true;
        self.zero_lifted_blank = true;
        self
    }
    pub const fn choose_blanks(mut self) -> Self {
        self.zero_dynamic_lifted_blank = true;
        self
    }
    pub const fn paired_result(mut self, paired: PairedResult) -> Self {
        self.paired_result = paired;
        self
    }
    pub const fn whole_result(mut self, policy: WholeResultPolicy) -> Self {
        self.whole_result_policy = policy;
        self
    }
    pub const fn volatile(mut self) -> Self {
        self.volatile = true;
        self
    }
    pub const fn flush_subnormal_inputs(mut self) -> Self {
        self.flush_subnormal_inputs = true;
        self
    }
    pub const fn retain_overflow_number(mut self) -> Self {
        self.retain_overflow_number = true;
        self
    }
    pub const fn retain_subnormal_output(mut self) -> Self {
        self.retain_subnormal_output = true;
        self
    }
    pub const fn strict_number_arguments(mut self) -> Self {
        self.strict_number_arguments = true;
        self
    }
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

impl LegacyPlace {
    /// The legacy placement of a call position, using the final declaration for a repeated tail.
    pub fn for_position(parameters: &[Parameter], index: usize) -> Self {
        let last = parameters.len().saturating_sub(1);
        let Some(parameter) = parameters.get(index.min(last)) else {
            return Self::Default;
        };
        let place = parameter.legacy;
        if index <= last
            || matches!(
                place,
                Self::ArrayAlwaysRepeated
                    | Self::ArrayAlwaysLiftReferencesRepeated
                    | Self::ArrayAtRootRepeated
                    | Self::AggregateAtRootRepeated
                    | Self::AggregateUnderArrayRepeated
            )
            || (index == last + 1 && place == Self::AggregateAtRootTwice)
        {
            place
        } else {
            Self::Default
        }
    }

    /// Whether the compiler inserts implicit intersection on a reference at this position.
    pub fn intersects_reference(self, below_array: bool) -> bool {
        match self {
            Self::IntersectReferenceAlways | Self::AggregateAlwaysIntersectReference => true,
            Self::IntersectReference | Self::AggregateIntersectReference => !below_array,
            _ => false,
        }
    }

    /// A declaration which enables per-reference lifting for the legacy call.
    pub fn lifts_references(self) -> bool {
        matches!(
            self,
            Self::ArrayAlwaysLiftReferences
                | Self::ArrayAlwaysLiftReferencesRepeated
                | Self::AggregateRootArrayNestedLiftReferences
                | Self::LiftReferences
        )
    }
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
pub enum ContextFacility {
    NumericMode,
    Cells,
    Random,
    ReferenceText,
    Environment,
    NumberSeparators,
    Host,
    Callables,
    WeekState,
}

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
    pub const fn with_context(mut self, facilities: &'static [ContextFacility]) -> Self {
        self.context_facilities = Some(facilities);
        self
    }

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
