//! The call interface: [`invoke`] runs a resolved catalogue entry on the arguments its caller
//! supplies through [`PreparedArguments`], with services ([`EvalServices`]) and scratch storage
//! ([`FunctionCallScratch`]) that the caller owns and reuses.
//!
//! The entry's declarations decide the order of work: the arity is checked before any argument
//! is asked for; an eager entry's arguments are asked for once each, in order, and admitted
//! against their parameter classes before its kernel runs; a lazy entry's arguments are not asked
//! for here at all, because only the entry may select which of them is evaluated.

use crate::catalogue::{CatalogueEntry, FunctionCallTarget, FunctionId, ParameterClass};
use crate::value::{CalcValue, CoreValue, NumericMode};

/// The arguments of one call. The caller produces each value when the call interface asks for
/// it, so a lazy entry's held-back arguments are never evaluated by the interface.
pub trait PreparedArguments {
    /// The number of arguments at the call site, omitted ones included.
    fn count(&self) -> usize;

    /// Argument `index`, below [`PreparedArguments::count`]. An omitted argument is
    /// [`CalcValue::missing`].
    fn argument(&mut self, index: usize) -> CalcValue;
}

/// The services a call runs with, owned by the caller: for now the numeric policy, which every
/// arithmetic site reads instead of assuming one.
#[derive(Debug)]
pub struct EvalServices<'s> {
    numeric: &'s NumericMode,
}

impl<'s> EvalServices<'s> {
    pub fn new(numeric: &'s NumericMode) -> Self {
        EvalServices { numeric }
    }

    pub fn numeric_mode(&self) -> &'s NumericMode {
        self.numeric
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
    /// The entry has no kernel, or its kernel does not model this call; `argument` names the
    /// first argument it does not model when one is responsible.
    NotImplemented {
        id: FunctionId,
        argument: Option<usize>,
    },
    /// The call runs under [`NumericMode::Exact`], for which the entry has no implementation.
    ExactUnavailable { id: FunctionId },
    /// The kernel returned [`CoreValue::Missing`], which marks an omitted argument and is never a
    /// function's result ([`CoreValue::Empty`] is).
    MissingReturned { id: FunctionId },
}

/// The kernel of an eager entry: its arguments, all evaluated and admitted, and the services.
pub(crate) type EagerKernel =
    fn(&[CalcValue], &mut EvalServices<'_>) -> Result<CalcValue, FunctionFault>;

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
    if entry.demand.is_lazy() {
        // No lazy kernel exists yet; whichever arrives receives `arguments` itself.
        return Err(FunctionFault::NotImplemented {
            id: entry.id,
            argument: None,
        });
    }
    scratch.arguments.clear();
    let result = run_eager(entry, supplied, arguments, services, &mut scratch.arguments);
    scratch.arguments.clear();
    result
}

fn run_eager(
    entry: &CatalogueEntry,
    supplied: usize,
    arguments: &mut dyn PreparedArguments,
    services: &mut EvalServices<'_>,
    evaluated: &mut Vec<CalcValue>,
) -> Result<CalcValue, FunctionFault> {
    for index in 0..supplied {
        let value = arguments.argument(index);
        admit_argument(entry, index, &value)?;
        evaluated.push(value);
    }
    let kernel = entry.kernel.ok_or(FunctionFault::NotImplemented {
        id: entry.id,
        argument: None,
    })?;
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

/// The two boundary rules no admitted entry can reach yet: no entry declares a reference-only
/// parameter and no kernel returns Missing, so these entries exist only here. Each test runs
/// natively and, as a wasm-bindgen test, in Chrome and Firefox.
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
            kernel: Some(kernel),
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
