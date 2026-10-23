//! The call interface: [`invoke`] runs a resolved catalogue entry on the arguments its caller
//! supplies through [`PreparedArguments`], with services ([`EvalServices`]) and scratch storage
//! ([`FunctionCallScratch`]) that the caller owns and reuses.
//!
//! The entry's declarations decide the order of work: the arity is checked before any argument
//! is asked for; an eager entry's arguments are asked for once each, in order, and admitted
//! against their parameter classes before its kernel runs; a lazy entry's kernel asks for the
//! arguments it selects itself, each admitted when it is asked for, so an argument it does not
//! select is never evaluated.

use crate::catalogue::{CatalogueEntry, FunctionCallTarget, FunctionId, Kernel, ParameterClass};
use crate::value::{CalcValue, CellKey, CoreValue, GridArea, NumericMode};
use std::fmt;
use std::ops::ControlFlow;

/// The arguments of one call. The caller produces each value when the call interface asks for
/// it, so a lazy entry's held-back arguments are never evaluated by the interface.
pub trait PreparedArguments {
    /// The number of arguments at the call site, omitted ones included.
    fn count(&self) -> usize;

    /// Argument `index`, below [`PreparedArguments::count`]. An omitted argument is
    /// [`CalcValue::missing`].
    fn argument(&mut self, index: usize) -> CalcValue;
}

/// Typed read access to the cells that a reference argument names, supplied by the caller with
/// the services. A kernel reads a reference only through it and never reaches workbook state.
pub trait CellReader {
    /// Calls `visit` for the cells of `area` that hold a value, in row-major order (each row left
    /// to right, rows top to bottom), until `visit` returns [`ControlFlow::Break`]; it then
    /// returns `Ok`. A cell it does not visit is blank, and it may also visit a blank cell as
    /// [`CoreValue::Empty`]. [`Unreadable`] when it cannot supply the values of `area`.
    fn read_area(
        &mut self,
        area: GridArea,
        visit: &mut dyn FnMut(CellKey, &CalcValue) -> ControlFlow<()>,
    ) -> Result<(), Unreadable>;

    /// What SUBTOTAL and AGGREGATE leave out of `area`: the cells holding a SUBTOTAL or
    /// AGGREGATE formula, and the rows that are hidden or filtered out. `None`, the default, when
    /// the reader cannot say; those functions then refuse a reference argument as not modelled
    /// rather than count cells Excel would leave out.
    fn subtotal_exclusions(&mut self, area: GridArea) -> Option<SubtotalExclusions> {
        let _ = area;
        None
    }
}

/// The cells of an area that SUBTOTAL and AGGREGATE leave out, as a [`CellReader`] reports them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubtotalExclusions {
    /// The cells whose formula is a SUBTOTAL or AGGREGATE call.
    pub nested_subtotals: Vec<CellKey>,
    /// The rows hidden by hand (SUBTOTAL's function numbers 101 to 111 leave them out).
    pub hidden_rows: Vec<u32>,
    /// The rows a filter hides (every function number leaves them out).
    pub filtered_rows: Vec<u32>,
}

/// A source of uniform random numbers for RAND, RANDBETWEEN and RANDARRAY, which the caller
/// supplies with the services. Without one those functions are refused as not modelled.
pub trait RandomSource {
    /// A number in [0, 1).
    fn next_unit(&mut self) -> f64;
}

/// A [`CellReader`]'s refusal to supply the values of an area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unreadable;

/// Resolves reference text, as INDIRECT reads it, in the calling formula's workbook context: its
/// sheets, and its cell, from which relative R1C1 text is offset. calc-func holds neither, and
/// no reference grammar, so the caller supplies the resolution with the services
/// ([`EvalServices::with_reference_text`]); calc-fml implements it with its reference binder.
pub trait ReferenceText {
    /// The reference `text` (UTF-16 code units) names, read in A1 notation when `a1` is true and
    /// in R1C1 notation otherwise.
    fn resolve(&mut self, text: &[u16], a1: bool) -> TextReference;
}

/// What a [`ReferenceText`] resolution gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextReference {
    /// The text names this area.
    Area(GridArea),
    /// The text names no reference: the worksheet error #REF!.
    Invalid,
    /// What the text names is not modelled (a defined name, another workbook), and the call is
    /// refused.
    NotModelled,
}

/// The services a call runs with, owned by the caller: the numeric policy, which every
/// arithmetic site reads instead of assuming one, the typed reader of reference arguments, and
/// the resolution of reference text.
pub struct EvalServices<'s> {
    numeric: &'s NumericMode,
    cells: Option<&'s mut dyn CellReader>,
    random: Option<&'s mut dyn RandomSource>,
    reference_text: Option<&'s mut dyn ReferenceText>,
}

impl<'s> EvalServices<'s> {
    /// Services without a cell reader: a reference argument that a kernel must read is refused
    /// with [`FunctionFault::ReferenceUnreadable`].
    pub fn new(numeric: &'s NumericMode) -> Self {
        EvalServices {
            numeric,
            cells: None,
            random: None,
            reference_text: None,
        }
    }

    /// Services whose kernels read reference arguments through `cells`.
    pub fn with_cells(numeric: &'s NumericMode, cells: &'s mut dyn CellReader) -> Self {
        EvalServices {
            numeric,
            cells: Some(cells),
            random: None,
            reference_text: None,
        }
    }

    /// These services, with reference text resolved by `resolution`. Without one, a call that
    /// must resolve reference text (INDIRECT's) is refused as not modelled.
    pub fn with_reference_text(self, resolution: &'s mut dyn ReferenceText) -> Self {
        EvalServices {
            reference_text: Some(resolution),
            ..self
        }
    }

    /// These services with `random` as the source of RAND, RANDBETWEEN and RANDARRAY.
    pub fn with_random(mut self, random: &'s mut dyn RandomSource) -> Self {
        self.random = Some(random);
        self
    }

    pub fn numeric_mode(&self) -> &'s NumericMode {
        self.numeric
    }

    pub(crate) fn cells(&mut self) -> Option<&mut (dyn CellReader + 's)> {
        self.cells.as_deref_mut()
    }

    pub(crate) fn random(&mut self) -> Option<&mut (dyn RandomSource + 's)> {
        self.random.as_deref_mut()
    }

    pub(crate) fn reference_text(&mut self) -> Option<&mut (dyn ReferenceText + 's)> {
        self.reference_text.as_deref_mut()
    }
}

impl fmt::Debug for EvalServices<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvalServices")
            .field("numeric", self.numeric)
            .field("cells", &self.cells.is_some())
            .field("random", &self.random.is_some())
            .field("reference_text", &self.reference_text.is_some())
            .finish()
    }
}

/// Argument storage the caller owns and reuses across calls, so a call on warm scratch does not
/// allocate for its arguments.
#[derive(Debug, Default)]
pub struct FunctionCallScratch {
    arguments: Vec<CalcValue>,
}

impl FunctionCallScratch {
    pub fn new() -> Self {
        Self::default()
    }
}

/// An infrastructure or admission failure of a call, distinct from a worksheet error value that
/// a kernel computes. A caller reports a call that ends in a fault as not calculated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FunctionFault {
    /// The call supplies a number of arguments that the entry's arity does not admit.
    ArityMismatch { id: FunctionId, supplied: usize },
    /// A value parameter received a reference; the caller dereferences before the call.
    ReferenceArgument { id: FunctionId, index: usize },
    /// A reference-only parameter received something other than a reference.
    ReferenceRequired { id: FunctionId, index: usize },
    /// The kernel must read the reference at argument `index`, and the services have no
    /// [`CellReader`] or the reader refused an area of it.
    ReferenceUnreadable { id: FunctionId, index: usize },
    /// The kernel does not model Excel's result for this call; `argument` names the argument
    /// responsible when one is.
    NotImplemented {
        id: FunctionId,
        argument: Option<usize>,
    },
    /// The call reaches an arithmetic site under [`NumericMode::Exact`], for which no
    /// implementation exists.
    ExactUnavailable { id: FunctionId },
    /// The kernel returned [`CoreValue::Missing`], which marks an omitted argument and is never a
    /// function's result ([`CoreValue::Empty`] is).
    MissingReturned { id: FunctionId },
}

/// The kernel of an eager entry: its arguments, all evaluated and admitted, and the services.
pub(crate) type EagerKernel =
    fn(&[CalcValue], &mut EvalServices<'_>) -> Result<CalcValue, FunctionFault>;

/// The kernel of a lazy entry: the arguments it selects from, and the services.
pub(crate) type LazyKernel =
    fn(&mut LazyArguments<'_>, &mut EvalServices<'_>) -> Result<CalcValue, FunctionFault>;

/// The arguments of a lazy entry's call. The caller evaluates each one only when the kernel asks
/// for it, and it is admitted against its parameter class before the kernel sees it.
pub(crate) struct LazyArguments<'a> {
    entry: &'a CatalogueEntry,
    source: &'a mut dyn PreparedArguments,
    count: usize,
}

impl LazyArguments<'_> {
    /// The number of arguments at the call site, omitted ones included.
    pub(crate) fn count(&self) -> usize {
        self.count
    }

    /// Argument `index`, below [`LazyArguments::count`], evaluated now.
    pub(crate) fn argument(&mut self, index: usize) -> Result<CalcValue, FunctionFault> {
        let value = self.source.argument(index);
        admit_argument(self.entry, index, &value)?;
        Ok(value)
    }
}

/// Runs the entry `target` resolved to. See the module documentation for the order of work.
pub fn invoke(
    target: FunctionCallTarget<'_>,
    arguments: &mut dyn PreparedArguments,
    services: &mut EvalServices<'_>,
    scratch: &mut FunctionCallScratch,
) -> Result<CalcValue, FunctionFault> {
    let entry = target.entry;
    let supplied = arguments.count();
    if !entry.arity.accepts(supplied) {
        return Err(FunctionFault::ArityMismatch {
            id: entry.id,
            supplied,
        });
    }
    match entry.kernel {
        Kernel::Lazy(kernel) => {
            let mut selectable = LazyArguments {
                entry,
                source: arguments,
                count: supplied,
            };
            admit_return(entry.id, kernel(&mut selectable, services)?)
        }
        Kernel::Eager(kernel) => {
            scratch.arguments.clear();
            let result = run_eager(entry, kernel, arguments, services, &mut scratch.arguments);
            scratch.arguments.clear();
            result
        }
    }
}

fn run_eager(
    entry: &CatalogueEntry,
    kernel: EagerKernel,
    arguments: &mut dyn PreparedArguments,
    services: &mut EvalServices<'_>,
    evaluated: &mut Vec<CalcValue>,
) -> Result<CalcValue, FunctionFault> {
    for index in 0..arguments.count() {
        let value = arguments.argument(index);
        admit_argument(entry, index, &value)?;
        evaluated.push(value);
    }
    admit_return(entry.id, kernel(evaluated, services)?)
}

fn admit_argument(
    entry: &CatalogueEntry,
    index: usize,
    value: &CalcValue,
) -> Result<(), FunctionFault> {
    let is_reference = matches!(value.core(), CoreValue::Reference(_));
    match entry.parameter_class(index) {
        ParameterClass::Value if is_reference => Err(FunctionFault::ReferenceArgument {
            id: entry.id,
            index,
        }),
        ParameterClass::Reference if !is_reference => Err(FunctionFault::ReferenceRequired {
            id: entry.id,
            index,
        }),
        ParameterClass::Value | ParameterClass::ValueOrReference | ParameterClass::Reference => {
            Ok(())
        }
    }
}

fn admit_return(id: FunctionId, value: CalcValue) -> Result<CalcValue, FunctionFault> {
    if value.is_missing() {
        Err(FunctionFault::MissingReturned { id })
    } else {
        Ok(value)
    }
}

impl FunctionCallTarget<'_> {
    /// Runs this entry through [`invoke`].
    pub fn invoke(
        self,
        arguments: &mut dyn PreparedArguments,
        services: &mut EvalServices<'_>,
        scratch: &mut FunctionCallScratch,
    ) -> Result<CalcValue, FunctionFault> {
        invoke(self, arguments, services, scratch)
    }
}

/// Two boundary rules, on test entries that exist only here: no admitted kernel returns Missing,
/// and a reference-only parameter, like COUNTBLANK's, admits a reference and refuses every other
/// kind of value. Each test runs natively and, as a wasm-bindgen test, in Chrome and Firefox.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::{ArgumentDemand, Notation, Parameter};
    use crate::value::{CallableArityShape, CellKey, IdAllocator, ReferenceLike, SheetId};

    struct Given(Vec<CalcValue>);

    impl PreparedArguments for Given {
        fn count(&self) -> usize {
            self.0.len()
        }

        fn argument(&mut self, index: usize) -> CalcValue {
            self.0[index].clone()
        }
    }

    fn returns_missing(
        _: &[CalcValue],
        _: &mut EvalServices<'_>,
    ) -> Result<CalcValue, FunctionFault> {
        Ok(CalcValue::missing())
    }

    fn returns_empty(
        _: &[CalcValue],
        _: &mut EvalServices<'_>,
    ) -> Result<CalcValue, FunctionFault> {
        Ok(CalcValue::empty())
    }

    const TEST_ID: FunctionId = FunctionId::new(0xFFFF, 1);

    const fn entry(class: ParameterClass, kernel: crate::call::EagerKernel) -> CatalogueEntry {
        CatalogueEntry {
            id: TEST_ID,
            spelling: "TEST.BOUNDARY",
            notation: Notation::Call,
            arity: CallableArityShape::exact(1),
            parameters: match class {
                ParameterClass::Reference => &[Parameter {
                    name: "reference",
                    class: ParameterClass::Reference,
                }],
                _ => &[Parameter {
                    name: "value",
                    class: ParameterClass::ValueOrReference,
                }],
            },
            demand: ArgumentDemand::Eager,
            kernel: Kernel::Eager(kernel),
        }
    }

    fn call(entry: &CatalogueEntry, argument: CalcValue) -> Result<CalcValue, FunctionFault> {
        let mode = NumericMode::Fast;
        invoke(
            FunctionCallTarget { entry },
            &mut Given(vec![argument]),
            &mut EvalServices::new(&mode),
            &mut FunctionCallScratch::new(),
        )
    }

    fn a_reference() -> CalcValue {
        let mut sheets: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = sheets.issue().unwrap();
        CalcValue::reference(ReferenceLike::cell(CellKey::new(sheet, 1, 1).unwrap()))
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_raw_return_may_be_empty_but_never_missing() {
        let missing = entry(ParameterClass::ValueOrReference, returns_missing);
        let empty = entry(ParameterClass::ValueOrReference, returns_empty);
        assert_eq!(
            call(&missing, CalcValue::number(1.0)),
            Err(FunctionFault::MissingReturned { id: TEST_ID })
        );
        assert_eq!(call(&empty, CalcValue::number(1.0)), Ok(CalcValue::empty()));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_reference_only_parameter_admits_only_references() {
        let reference_only = entry(ParameterClass::Reference, returns_empty);
        for refused in [
            CalcValue::number(1.0),
            CalcValue::empty(),
            CalcValue::missing(),
            CalcValue::error(crate::value::WorksheetErrorCode::Ref),
        ] {
            assert_eq!(
                call(&reference_only, refused),
                Err(FunctionFault::ReferenceRequired {
                    id: TEST_ID,
                    index: 0
                })
            );
        }
        assert_eq!(call(&reference_only, a_reference()), Ok(CalcValue::empty()));
    }
}
