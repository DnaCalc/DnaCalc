//! Data types used by function declarations.

use std::fmt;

/// The stable identity of a catalogue entry: a family code and a member number within that
/// family, both written in the family's fragment and never derived from an entry's position.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionId(u32);

impl FunctionId {
    /// Assemble an ID from its stable family and member codes.
    pub const fn new(family: u16, member: u16) -> Self {
        FunctionId(((family as u32) << 16) | member as u32)
    }

    /// The ID whose raw value is `raw`, as [`FunctionId::raw`] gave it (an ID kept outside the
    /// process). [`crate::declaration`] looks up its declaration.
    pub const fn from_raw(raw: u32) -> Self {
        FunctionId(raw)
    }

    /// Encode the family in the high 16 bits and the member in the low 16 bits.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Return the ID's family code.
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
/// Binding forms whose operands introduce names in a formula.
pub enum BindingForm {
    /// Bind names for use in subsequent expressions and return the final expression.
    Let,
    /// Define a callable with named parameters and a body.
    Lambda,
}

/// A deferred implementation category and its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deferral {
    /// Obtain data from an external provider.
    ExternalService,
    /// Evaluate through Excel's Python service.
    Python,
    /// Evaluate with compatibility-version-3 list values.
    ListsInCells,
}

impl Deferral {
    /// Return the tracker ID for this deferred capability.
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
/// How a declared entry is implemented.
pub enum Implementation {
    /// The entry has an execution route.
    Held,
    /// The formula binder handles the entry.
    Binding,
    /// The declaration is held for admission or file handling while execution is unimplemented.
    NotHeld {
        /// Tracker ID for the missing execution work.
        owner: &'static str,
    },
    /// Execution is deferred with a classified reason.
    Deferred {
        /// Category responsible for the deferral.
        class: Deferral,
    },
}

/// Positions of result operands whose computed blank origin is retained.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PairedResult {
    #[default]
    /// Preserve no special blank origin.
    None,
    /// Preserve blank origin at odd zero-based positions.
    Odd,
    /// Preserve blank origin at even zero-based positions after the selector.
    EvenAfterSelector,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
/// How IFS and SWITCH can preserve a selected result whole.
pub enum WholeResultPolicy {
    #[default]
    /// Use ordinary result publication.
    None,
    /// Select a whole result through IFS's conditions.
    Conditions,
    /// Select a whole result by comparing SWITCH values with its selector.
    Selector,
}

/// The measured treatment of a 3-D reference at an argument position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreeDPlace {
    /// No behavior has been recorded at this position.
    Unknown,
    /// Supply the reference through the measured 3-D argument route.
    Take,
    /// Spread the referenced sheets into successive arguments.
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
    /// Set zero-based positions for accepted references, call-level `#VALUE!` and argument `#REF!`.
    ///
    /// An empty slice covers all positions. A trailing `usize::MAX` marks a repeating tail.
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
/// Provider key for worksheet calls that import file contents.
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
    /// A regular named worksheet function.
    Worksheet,
    /// An operator recognized by the formula language.
    Operator {
        /// Optional named spelling when Excel writes the operator as a call.
        file_spelling: Option<&'static str>,
    },
    /// A binder-owned form that introduces names.
    Binding(BindingForm),
    /// A function exported by an add-in.
    AddIn {
        /// The registered add-in library name.
        library: &'static str,
        /// The exported function name.
        export: &'static str,
    },
    /// An entry evaluated through a host service.
    HostRoute {
        /// Key used to look up the host provider.
        provider: &'static str,
        /// Operation requested from the provider.
        route: HostRouteKind,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// The operation a host-routed function requests.
pub enum HostRouteKind {
    /// Fetch a resource for a worksheet call.
    Fetch(FetchKind),
    /// Import the contents of a file.
    Import(ImportKind),
    /// Read from a real-time data server.
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
    /// Positions whose selected results retain their blank origin.
    pub paired_result: PairedResult,
    /// Selection rule for evaluating a result whole.
    pub whole_result_policy: WholeResultPolicy,
    /// Publish array blanks selected by IF as zero.
    pub zero_array_blanks: bool,
    /// Publish blanks in lifted IF results as zero.
    pub zero_lifted_blank: bool,
    /// Publish blanks in dynamic lifted CHOOSE results as zero.
    pub zero_dynamic_lifted_blank: bool,
    /// Recalculate the call according to the volatile-function policy.
    pub volatile: bool,
    /// Flush subnormal numeric operands on entry.
    pub flush_subnormal_inputs: bool,
    /// Preserve numeric overflow at the entry's output boundary.
    pub retain_overflow_number: bool,
    /// Preserve a subnormal result at the entry's output boundary.
    pub retain_subnormal_output: bool,
    /// Require numeric arguments under the entry's strict number policy.
    pub strict_number_arguments: bool,
    /// Admission and error rules for 3-D references.
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

    /// Baseline traits with no special entry behavior enabled.
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

    /// Enable intersection of the result at a legacy scalar position.
    pub const fn intersect_legacy_result(mut self) -> Self {
        self.intersect_legacy_result = true;
        self
    }
    /// Reject multi-cell references at scalar positions.
    pub const fn reject_multi_cell_reference(mut self) -> Self {
        self.reject_multi_cell_reference = true;
        self
    }
    /// Publish missing-coordinate errors before invoking the kernel.
    pub const fn padding_precedes_call(mut self) -> Self {
        self.padding_precedes_call = true;
        self
    }
    /// Mark calls as subtotals and permit them to skip nested subtotals.
    pub const fn skip_nested_subtotals(mut self) -> Self {
        self.can_skip_nested_subtotals = true;
        self.is_subtotal = true;
        self
    }
    /// Enable dynamic lifting once per reference in a reference array.
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
    /// Pass a reference array as a whole operand.
    pub const fn take_reference_array(mut self) -> Self {
        self.take_reference_array = true;
        self
    }
    /// Have each held reference supply its first cell.
    pub const fn first_cell_references(mut self) -> Self {
        self.held_references = HeldReferences::FirstCell;
        self
    }
    /// Have each held reference supply no number.
    pub const fn no_number_references(mut self) -> Self {
        self.held_references = HeldReferences::NoNumber;
        self
    }
    /// Have each held reference supply `#VALUE!`.
    pub const fn value_error_references(mut self) -> Self {
        self.held_references = HeldReferences::ValueError;
        self
    }
    /// Have each held reference supply `#NUM!`.
    pub const fn num_error_references(mut self) -> Self {
        self.held_references = HeldReferences::NumError;
        self
    }
    /// Have each held reference supply `#N/A`.
    pub const fn na_error_references(mut self) -> Self {
        self.held_references = HeldReferences::NaError;
        self
    }
    /// Lift computed arrays for an entry that can take a reference whole.
    pub const fn lift_computed_array(mut self) -> Self {
        self.lift_computed_array = true;
        self
    }
    /// Enable IF's zero publication for selected array blanks and lifted blanks.
    pub const fn if_blanks(mut self) -> Self {
        self.zero_array_blanks = true;
        self.zero_lifted_blank = true;
        self
    }
    /// Enable CHOOSE's zero publication for blanks in dynamic lifted results.
    pub const fn choose_blanks(mut self) -> Self {
        self.zero_dynamic_lifted_blank = true;
        self
    }
    /// Set the positions whose selected results preserve their blank origin.
    pub const fn paired_result(mut self, paired: PairedResult) -> Self {
        self.paired_result = paired;
        self
    }
    /// Set the rule for consuming the selected result whole.
    pub const fn whole_result(mut self, policy: WholeResultPolicy) -> Self {
        self.whole_result_policy = policy;
        self
    }
    /// Declare that the entry is volatile.
    pub const fn volatile(mut self) -> Self {
        self.volatile = true;
        self
    }
    /// Enable flushing of subnormal numeric arguments on entry.
    pub const fn flush_subnormal_inputs(mut self) -> Self {
        self.flush_subnormal_inputs = true;
        self
    }
    /// Enable preservation of numeric overflow at the output boundary.
    pub const fn retain_overflow_number(mut self) -> Self {
        self.retain_overflow_number = true;
        self
    }
    /// Enable preservation of subnormal outputs.
    pub const fn retain_subnormal_output(mut self) -> Self {
        self.retain_subnormal_output = true;
        self
    }
    /// Enable the strict number policy for arguments.
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
    /// Label displayed in signature help.
    pub name: &'static str,
    /// Operand class at the execution interface.
    pub class: ParameterClass,
    /// Operand form required at formula entry.
    pub entry: EntryClass,
    /// Implicit-intersection and lifting placement in legacy formulas.
    pub legacy: LegacyPlace,
    /// Whether the kernel itself lifts elementwise over an ordinary array or multi-cell
    /// single-area range when pre-kernel admission passes the operand whole (for example,
    /// criteria of COUNTIF/SUMIF/AVERAGEIF and the *IFS family).
    pub kernel_lifts: bool,
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
    /// The legacy placement of a call position, resolving repeating [`ArgumentGroups`] before
    /// falling back to [`Self::for_position`] for ungrouped repeated tails.
    pub fn for_grouped_position(
        parameters: &[Parameter],
        groups: Option<ArgumentGroups>,
        index: usize,
    ) -> Self {
        if index >= parameters.len()
            && let Some(groups) = groups
            && let Some(named) = groups.parameter(index)
            && let Some(parameter) = parameters.get(named)
        {
            return parameter.legacy;
        }
        Self::for_position(parameters, index)
    }

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
            kernel_lifts: false,
        }
    }

    /// Declare a value parameter whose references are read before execution.
    pub const fn value(name: &'static str) -> Self {
        Self::of(name, ParameterClass::Value)
    }

    /// Declare a parameter that receives values or intact references.
    pub const fn value_or_reference(name: &'static str) -> Self {
        Self::of(name, ParameterClass::ValueOrReference)
    }

    /// Declare a parameter that receives only references.
    pub const fn reference(name: &'static str) -> Self {
        Self::of(name, ParameterClass::Reference)
    }

    /// This parameter with the entry class `entry`.
    pub const fn entry(mut self, entry: EntryClass) -> Self {
        self.entry = entry;
        self
    }

    /// Set the placement used when lowering a legacy formula.
    pub const fn legacy(mut self, legacy: LegacyPlace) -> Self {
        self.legacy = legacy;
        self
    }

    /// Mark a non-`Value` parameter whose kernel lifts elementwise over ordinary arrays and
    /// multi-cell single-area ranges.
    pub const fn kernel_lifts(mut self) -> Self {
        self.kernel_lifts = true;
        self
    }
}

/// What Excel's formula entry accepts in a parameter. The default is [`EntryClass::Any`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntryClass {
    /// Every operand form.
    Any,
    /// Only an operand that formula admission classifies as possibly producing a reference.
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
    /// Zero-based index where the first group starts.
    pub from: usize,
    /// Number of parameters in each repeated group.
    pub size: usize,
    /// Maximum number of arguments permitted after the last complete group.
    pub tail: usize,
}

impl ArgumentGroups {
    /// Describe repeating groups with a positive size and an optional trailing suffix.
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
    /// Locale identifier for the translation.
    pub locale: &'static str,
    /// Spelling displayed in that locale.
    pub spelling: &'static str,
}

/// A service a kernel may use when it evaluates a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextFacility {
    /// Selection of the calculation's numeric mode.
    NumericMode,
    /// Access to cell and reference values.
    Cells,
    /// Access to the calculation's random stream.
    Random,
    /// Conversion between reference text and references.
    ReferenceText,
    /// Workbook, worksheet or application environment facts.
    Environment,
    /// Decimal and grouping separators.
    NumberSeparators,
    /// Dispatch to a host service.
    Host,
    /// Invocation of a callable operand.
    Callables,
    /// Date and week-number state.
    WeekState,
}

/// Data for a worksheet function whose executable kernel is attached by the caller.
#[derive(Clone, Copy, Debug)]
pub struct WorksheetDeclaration {
    /// `None` means translated names have not been recorded for this entry.
    pub localized_names: Option<&'static [LocalizedDisplayName]>,
    /// `None` means the complete set of facilities has not been classified.
    pub context_facilities: Option<&'static [ContextFacility]>,
    /// The entry's stable catalogue identity.
    pub id: FunctionId,
    /// Call form and owner of execution.
    pub kind: EntryKind,
    /// Invariant spelling used in formulas.
    pub spelling: &'static str,
    /// Named call or operator syntax.
    pub notation: Notation,
    /// Observed build or compatibility availability.
    pub availability: Availability,
    /// Supported range of argument counts.
    pub arity: calc_value::CallableArityShape,
    /// Repeating parameter groups, when declared.
    pub groups: Option<ArgumentGroups>,
    /// Parameter declarations in call order.
    pub parameters: &'static [Parameter],
    /// Result form seen by formula entry.
    pub result: ResultForm,
    /// When arguments are evaluated.
    pub demand: ArgumentDemand,
    /// Additional evaluation and publication behavior.
    pub traits: EntryTraits,
}

impl WorksheetDeclaration {
    /// Declare the complete set of facilities the kernel may use.
    pub const fn with_context(mut self, facilities: &'static [ContextFacility]) -> Self {
        self.context_facilities = Some(facilities);
        self
    }

    /// Set the result form visible to formula admission.
    pub const fn with_result(mut self, result: ResultForm) -> Self {
        self.result = result;
        self
    }
    /// Describe repeated groups in the argument list.
    pub const fn with_groups(mut self, groups: ArgumentGroups) -> Self {
        self.groups = Some(groups);
        self
    }
    /// Replace evaluation traits while preserving the existing 3-D reference policy.
    pub const fn with_traits(mut self, traits: EntryTraits) -> Self {
        let three_d = self.traits.three_d;
        self.traits = traits;
        self.traits.three_d = three_d;
        self
    }
    /// Set the 3-D reference policy while preserving the other evaluation traits.
    pub const fn with_three_d(mut self, three_d: ThreeD) -> Self {
        self.traits.three_d = three_d;
        self
    }
}
