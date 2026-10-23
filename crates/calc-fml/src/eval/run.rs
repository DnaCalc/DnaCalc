//! Running a [`FormulaInstance`] in an [`EvalFrame`].
//!
//! A block's instructions run in order on the frame's operand stack. An eager call takes its
//! arguments from the stack, each already evaluated; a lazy call hands calc-func its arguments
//! as blocks that run only when the kernel asks for them. Before a call, an argument whose
//! parameter takes a value and holds a reference is replaced by its cell's value; a reference
//! passed to a parameter that admits one reaches the kernel as a reference, which the kernel
//! reads through the shared reader.
//!
//! Operator chains and eager calls, however long or deep, run in one block's loop over the
//! operand stack. Evaluation recurses on the native stack in one place: a lazy call's arguments
//! run from inside calc-func's kernel (`Run::block`, `Run::lazy_call`, `calc_func::invoke`, the
//! kernel, `Blocks::argument`, then `Run::block` for the argument's block), once for each lazy
//! call nested inside another's argument. The parser gives a diagnostic to a call nested more
//! than 64 levels deep (Excel's documented limit of nested levels of functions), and compile refuses every
//! formula with a diagnostic, so this recursion is at most 64 lazy calls deep. The explicit evaluation stack that removes the recursion
//! is bead cfml-m2.3's (plan section 4.3).

use super::cells::{ReaderCells, SharedReader};
use super::{
    EvalFrame, EvaluationObserver, EvaluationReport, EvaluationServices, ExecutedCall,
    FormulaInstance, Instruction, ResolvedReference, Target,
};
use crate::reference::{
    FormulaAnchor, ReadContractError, ReferenceCatalog, ReferenceNotation, bind_text,
};
use calc_func::value::{CalcValue, CellKey, CoreValue, GridArea, NumericMode, WorksheetErrorCode};
use calc_func::{
    EvalServices, FunctionCallScratch, FunctionFault, ParameterClass, PreparedArguments,
    ReferenceText, TextReference, invoke,
};

/// Why an evaluation produced no value. Each is a refusal to calculate, never a worksheet value:
/// a caller reports the formula as not calculated.
#[derive(Clone, Debug, PartialEq)]
pub enum EvalFault {
    /// A call ended in calc-func's fault (an argument it does not admit, a result it does not
    /// model, an arithmetic site under Exact).
    Function(FunctionFault),
    /// The formula reads `cell`, which the reader reported as missing: an input it cannot supply.
    MissingInput { cell: CellKey },
    /// The reader's runs did not cover an area exactly.
    ReadContract(ReadContractError),
    /// A reference to more than one cell where one value is needed (an operator's operand, a
    /// value parameter, the formula's result). Excel gives it one value by implicit intersection
    /// or spills it as an array, which beads cfml-m3.3 and cfml-m3.4 own.
    AreaAsValue { area: GridArea },
    /// The formula's final operation is an infix `+` or `-` (seen through parentheses, a prefix
    /// `+` or `-` and the arguments IF may return) whose result is a non-zero number of at most
    /// 1e-13 times the magnitude of its larger operand; `result` is the binary64 result.
    ///
    /// Excel adjusts the result of a final addition or subtraction near zero (Microsoft KB 78113),
    /// by a rule that is not documented precisely enough to implement, so a result in this band
    /// is refused rather than published. The band is chosen, not measured: it is not Excel's, and
    /// it also refuses results that are exact and non-zero (`=1E15+1-1E15` is exactly 1). Bead
    /// dc-r4ci measures Excel's rule against cfunc-m1.2's oracle rows and replaces this refusal
    /// with it.
    NearZeroFinalSum { result: f64 },
}

/// The band within which the result of a final `+` or `-` is refused as
/// [`EvalFault::NearZeroFinalSum`]: at most this many times its larger operand's magnitude. The
/// value is chosen (the relative 1e-13 within which calc-func refuses to order two nearly equal
/// numbers), not measured; bead dc-r4ci replaces the band with Excel's measured rule.
const NEAR_ZERO_BAND: f64 = 1e-13;

/// The argument a lazy kernel receives after one of its arguments faulted. The call's result is
/// discarded and the fault reported, so this value is never published. An error makes IF stop
/// without asking for another argument; IFERROR asks for its fallback, which receives this value
/// too without its block running.
const ABANDONED: WorksheetErrorCode = WorksheetErrorCode::Calc;

/// Runs `frame` to its value: the formula's result, with a reference to one cell replaced by the
/// cell's value, or the fault that stopped it. A result that is the empty value (a reference to a
/// blank cell, or a blank value a function returned, as IFERROR returns a blank `value`) is the
/// number 0, the value Excel shows for `=A1` when A1 is blank (not measured by this package). The
/// rule applies to every formula, not only to IFERROR and IFNA. Microsoft's IFERROR and IFNA pages
/// say an empty cell is treated as an empty string, yet Excel 16.0.20430.20182 shows 0 for each of
/// bead dc-ibp2's oracle rows whose result is a blank or omitted value or fallback, as this rule
/// gives (its capture of `tests/fixtures/dc_ibp2/oracle.xlsx`).
///
/// When the services carry an [`EvaluationObserver`], it is shown each call the evaluation runs
/// and then this outcome, before it is returned.
pub fn evaluate(
    frame: EvalFrame<'_>,
    services: &mut EvaluationServices<'_>,
) -> Result<CalcValue, EvalFault> {
    let EvalFrame { instance, scratch } = frame;
    let reader = SharedReader::new(&mut *services.reader);
    let mut run = Run {
        instance,
        stack: &mut scratch.stack,
        calls: &mut scratch.calls,
        numeric: services.numeric,
        reader: &reader,
        catalog: services.catalog,
        // Reborrowed for this evaluation only: the coercion shortens the observer's lifetime to
        // the run's, which `Option::as_deref_mut` cannot.
        observer: match services.observer.as_mut() {
            Some(observer) => Some(&mut **observer),
            None => None,
        },
    };
    let outcome = run.block(0).and_then(|value| {
        let value = match value.core() {
            CoreValue::Reference(reference) => reader.value_of(reference)?,
            _ => value,
        };
        Ok(if value.is_empty() {
            CalcValue::number(0.0)
        } else {
            value
        })
    });
    if let Some(observer) = run.observer {
        observer.returned(EvaluationReport {
            anchor: instance.anchor,
            numeric: services.numeric,
            outcome: outcome.as_ref(),
        });
    }
    outcome
}

struct Run<'a, 'r> {
    instance: &'a FormulaInstance,
    stack: &'a mut Vec<CalcValue>,
    /// Call scratch not in use; a lazy call's arguments take their own while it holds one.
    calls: &'a mut Vec<FunctionCallScratch>,
    numeric: &'a NumericMode,
    reader: &'a SharedReader<'r>,
    /// The workbook context INDIRECT's text is bound in, when the caller gave one.
    catalog: Option<&'a dyn ReferenceCatalog>,
    observer: Option<&'a mut dyn EvaluationObserver>,
}

impl Run<'_, '_> {
    /// Runs block `index` and gives the one value it leaves.
    fn block(&mut self, index: usize) -> Result<CalcValue, EvalFault> {
        let base = self.stack.len();
        let instance = self.instance;
        let block = &instance.plan.blocks[index];
        let mut at = 0;
        while let Some(instruction) = block.get(at) {
            at += 1;
            let step = match instruction {
                Instruction::Literal(literal) => {
                    self.stack.push(literal.value());
                    Ok(())
                }
                Instruction::Omitted => {
                    self.stack.push(CalcValue::missing());
                    Ok(())
                }
                Instruction::Reference(reference) => {
                    self.stack.push(instance.reference(*reference));
                    Ok(())
                }
                Instruction::Call {
                    target,
                    argc,
                    retain,
                    synthetic,
                } => self.call(*target, *argc, *retain, *synthetic),
                Instruction::LazyCall {
                    target,
                    first_block,
                    argc,
                } => self.lazy_call(*target, *first_block, *argc),
                Instruction::Copy(depth) => {
                    let value = self.stack[self.stack.len() - 1 - depth].clone();
                    self.stack.push(value);
                    Ok(())
                }
                Instruction::NearZeroGuard { skip } => {
                    // Stack: left, right, result.
                    let top = self.stack.len() - 1;
                    if !matches!(self.stack[top].as_number(), Some(n) if n != 0.0) {
                        let result = self.stack.pop().expect("the operation's result");
                        self.stack.truncate(top - 2);
                        self.stack.push(result);
                        at += skip;
                    }
                    Ok(())
                }
                Instruction::NearZeroCheck => self.near_zero_check(),
            };
            if let Err(fault) = step {
                self.stack.truncate(base);
                return Err(fault);
            }
        }
        // Compile lowers every expression to instructions that leave exactly one value.
        let value = self.stack.pop().expect("a block leaves one value");
        debug_assert_eq!(self.stack.len(), base);
        Ok(value)
    }

    /// The argument `value` as parameter `index` of `target` receives it: a reference passed to
    /// a value parameter is replaced by its cell's value.
    fn admit(
        &self,
        target: Target,
        index: usize,
        value: CalcValue,
    ) -> Result<CalcValue, EvalFault> {
        match (target.0.parameter_class(index), value.core()) {
            (Some(ParameterClass::Value), CoreValue::Reference(reference)) => {
                self.reader.value_of(reference)
            }
            _ => Ok(value),
        }
    }

    fn call(
        &mut self,
        target: Target,
        argc: usize,
        retain: bool,
        synthetic: bool,
    ) -> Result<(), EvalFault> {
        let start = self.stack.len() - argc;
        for index in 0..argc {
            let value = std::mem::replace(&mut self.stack[start + index], CalcValue::missing());
            self.stack[start + index] = self.admit(target, index, value)?;
        }
        let mut scratch = self.calls.pop().unwrap_or_default();
        self.reader.clear_refusal();
        let mut text = self.catalog.map(|catalog| TextBinding {
            catalog,
            anchor: self.instance.anchor,
            resolved: Vec::new(),
        });
        let result = {
            let mut cells = ReaderCells::new(self.reader);
            let mut services = EvalServices::with_cells(self.numeric, &mut cells);
            if let Some(text) = text.as_mut() {
                services = services.with_reference_text(text);
            }
            invoke(
                target.0,
                &mut Evaluated(&self.stack[start..]),
                &mut services,
                &mut scratch,
            )
        };
        self.calls.push(scratch);
        for area in text.map(|text| text.resolved).unwrap_or_default() {
            self.show_resolved(target, area);
        }
        self.show_call(target, synthetic);
        let value = result.map_err(|fault| self.function_fault(fault))?;
        if !retain {
            self.stack.truncate(start);
        }
        self.stack.push(value);
        Ok(())
    }

    fn lazy_call(
        &mut self,
        target: Target,
        first_block: usize,
        argc: usize,
    ) -> Result<(), EvalFault> {
        let mut scratch = self.calls.pop().unwrap_or_default();
        let reader = self.reader;
        let numeric = self.numeric;
        reader.clear_refusal();
        let mut arguments = Blocks {
            run: &mut *self,
            target,
            first_block,
            argc,
            fault: None,
        };
        let result = {
            let mut cells = ReaderCells::new(reader);
            let mut services = EvalServices::with_cells(numeric, &mut cells);
            invoke(target.0, &mut arguments, &mut services, &mut scratch)
        };
        let fault = arguments.fault.take();
        self.calls.push(scratch);
        self.show_call(target, false);
        if let Some(fault) = fault {
            return Err(fault);
        }
        let value = result.map_err(|fault| self.function_fault(fault))?;
        self.stack.push(value);
        Ok(())
    }

    /// Shows the observer, when there is one, that `target` ran.
    fn show_call(&mut self, target: Target, synthetic: bool) {
        if let Some(observer) = self.observer.as_deref_mut() {
            observer.called(ExecutedCall {
                target: target.0,
                synthetic,
            });
        }
    }

    /// Shows the observer, when there is one, that `target` resolved reference text to `area`.
    fn show_resolved(&mut self, target: Target, area: GridArea) {
        if let Some(observer) = self.observer.as_deref_mut() {
            observer.resolved(ResolvedReference {
                target: target.0,
                area,
            });
        }
    }

    /// A call's fault, with the reader's refusal when the kernel could not read a reference.
    fn function_fault(&self, fault: FunctionFault) -> EvalFault {
        let refusal = self.reader.take_refusal();
        match (&fault, refusal) {
            (FunctionFault::ReferenceUnreadable { .. }, Some(refusal)) => refusal.fault(),
            _ => EvalFault::Function(fault),
        }
    }

    /// The check of a final `+` or `-`: pops the negations of its right and left operands, its
    /// result and its operands, and pushes the result unless it lies in the near-zero band of the
    /// larger operand. A negation that is not a number leaves that operand's magnitude unknown,
    /// and a non-zero number result is then refused rather than assumed to lie outside the band.
    fn near_zero_check(&mut self) -> Result<(), EvalFault> {
        let base = self.stack.len() - 5;
        let negated = (
            self.stack[base + 4].as_number(),
            self.stack[base + 3].as_number(),
        );
        let value = self.stack[base + 2].clone();
        self.stack.truncate(base);
        if let Some(result) = value.as_number().filter(|result| *result != 0.0) {
            let in_band = match negated {
                (Some(right), Some(left)) => {
                    result.abs() <= left.abs().max(right.abs()) * NEAR_ZERO_BAND
                }
                _ => true,
            };
            if in_band {
                return Err(EvalFault::NearZeroFinalSum { result });
            }
        }
        self.stack.push(value);
        Ok(())
    }
}

/// Reference text bound for one eager call, INDIRECT's, in the evaluated formula's workbook
/// context, with every area it resolved kept for the observer. A lazy call is given none: no lazy
/// entry resolves reference text, and its arguments' own calls are given their own.
struct TextBinding<'c> {
    catalog: &'c dyn ReferenceCatalog,
    anchor: FormulaAnchor,
    resolved: Vec<GridArea>,
}

impl ReferenceText for TextBinding<'_> {
    fn resolve(&mut self, text: &[u16], a1: bool) -> TextReference {
        let notation = if a1 {
            ReferenceNotation::A1
        } else {
            ReferenceNotation::R1C1
        };
        let bound = bind_text(text, notation, self.anchor, self.catalog);
        if let TextReference::Area(area) = bound {
            self.resolved.push(area);
        }
        bound
    }
}

/// The evaluated arguments of an eager call.
struct Evaluated<'v>(&'v [CalcValue]);

impl PreparedArguments for Evaluated<'_> {
    fn count(&self) -> usize {
        self.0.len()
    }

    fn argument(&mut self, index: usize) -> CalcValue {
        self.0[index].clone()
    }
}

/// The arguments of a lazy call: argument `i` runs block `first_block + i` when the kernel asks
/// for it. The first fault stops every further argument; the call's result is then discarded.
struct Blocks<'b, 'a, 'r> {
    run: &'b mut Run<'a, 'r>,
    target: Target,
    first_block: usize,
    argc: usize,
    fault: Option<EvalFault>,
}

impl PreparedArguments for Blocks<'_, '_, '_> {
    fn count(&self) -> usize {
        self.argc
    }

    fn argument(&mut self, index: usize) -> CalcValue {
        if self.fault.is_some() {
            return CalcValue::error(ABANDONED);
        }
        let evaluated = self
            .run
            .block(self.first_block + index)
            .and_then(|value| self.run.admit(self.target, index, value));
        match evaluated {
            Ok(value) => value,
            Err(fault) => {
                self.fault = Some(fault);
                CalcValue::error(ABANDONED)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{
        EvalScratch, EvaluationObserver, EvaluationReport, ExecutedCall, PlanCache, compile,
        instantiate,
    };
    use crate::parse_formula;
    use crate::reference::{
        CellRun, FormulaAnchor, ReferenceCatalog, ReferenceNotation, ReferenceReader,
    };
    use calc_func::value::{ExcelText, IdAllocator, SheetId};
    use std::ops::ControlFlow;

    struct NoSheets;

    impl ReferenceCatalog for NoSheets {
        fn sheet_named(&self, _: &str) -> Option<SheetId> {
            None
        }
    }

    /// Every cell holds the same value.
    struct Everywhere(CalcValue);

    impl ReferenceReader for Everywhere {
        fn read_area(
            &mut self,
            area: GridArea,
            sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>,
        ) {
            for _ in 0..area.cell_count() {
                if sink(CellRun::Values(std::slice::from_ref(&self.0))).is_break() {
                    return;
                }
            }
        }
    }

    fn evaluated(formula: &str, cells: CalcValue) -> Result<CalcValue, EvalFault> {
        evaluated_under(&NumericMode::Fast, formula, cells)
    }

    fn evaluated_under(
        mode: &NumericMode,
        formula: &str,
        cells: CalcValue,
    ) -> Result<CalcValue, EvalFault> {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let anchor = FormulaAnchor::new(CellKey::new(ids.issue().unwrap(), 5, 5).unwrap());
        let plan = compile(
            &parse_formula(formula),
            ReferenceNotation::A1,
            anchor,
            &NoSheets,
            &mut PlanCache::new(),
        )
        .unwrap();
        let instance = instantiate(&plan, anchor);
        let mut reader = Everywhere(cells);
        let mut scratch = EvalScratch::new();
        evaluate(
            EvalFrame::new(&instance, &mut scratch),
            &mut EvaluationServices::new(mode, &mut reader),
        )
    }

    fn text(s: &str) -> CalcValue {
        CalcValue::text(ExcelText::from_interop_assignment(s))
    }

    /// What an observer is shown of one evaluation: each call, in order, as its spelling and
    /// notation with `*` appended when it is synthetic, and each report: its anchor, numeric mode
    /// and outcome.
    #[derive(Default)]
    struct Shown {
        calls: Vec<String>,
        reports: Vec<(FormulaAnchor, NumericMode)>,
        outcomes: Vec<Result<CalcValue, EvalFault>>,
    }

    impl EvaluationObserver for Shown {
        fn called(&mut self, call: ExecutedCall) {
            let target = call.target();
            self.calls.push(format!(
                "{:?} {}{}",
                target.notation(),
                target.spelling(),
                if call.synthetic() { " *" } else { "" }
            ));
        }

        fn returned(&mut self, report: EvaluationReport<'_>) {
            self.reports
                .push((report.anchor(), report.numeric_mode().clone()));
            self.outcomes
                .push(report.outcome().cloned().map_err(Clone::clone));
        }
    }

    /// The anchor [`observed`] evaluates at.
    fn observed_anchor() -> FormulaAnchor {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        FormulaAnchor::new(CellKey::new(ids.issue().unwrap(), 5, 5).unwrap())
    }

    /// Evaluates `formula` under Fast, every cell holding `cells`, with an observer attached.
    fn observed(formula: &str, cells: CalcValue) -> (Result<CalcValue, EvalFault>, Shown) {
        let anchor = observed_anchor();
        let plan = compile(
            &parse_formula(formula),
            ReferenceNotation::A1,
            anchor,
            &NoSheets,
            &mut PlanCache::new(),
        )
        .unwrap();
        let instance = instantiate(&plan, anchor);
        let mut reader = Everywhere(cells);
        let mut scratch = EvalScratch::new();
        let mut shown = Shown::default();
        let outcome = evaluate(
            EvalFrame::new(&instance, &mut scratch),
            &mut EvaluationServices::new(&NumericMode::Fast, &mut reader).observed_by(&mut shown),
        );
        (outcome, shown)
    }

    /// The observer is shown each call the evaluation ran, as it returned, with the near-zero
    /// check's two negations marked synthetic and a negation the formula wrote not, and then the
    /// outcome the evaluation returned, once, the same as `evaluate` returns, with the instance's
    /// anchor and the numeric mode. A formula whose result is a reference to one cell is shown the
    /// cell's value, as `evaluate` returns it, never the reference.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_observer_is_shown_calls_and_the_outcome() {
        let three = CalcValue::number(3.0);
        // Each formula, the value every cell holds, the calls shown and the outcome.
        type Case<'c> = (
            &'c str,
            CalcValue,
            &'c [&'c str],
            Result<CalcValue, EvalFault>,
        );
        let cases: [Case<'_>; 10] = [
            // A bare reference: the cell's value, not the reference.
            ("=A1", three.clone(), &[], Ok(CalcValue::number(3.0))),
            // A blank cell's value is the number 0.
            ("=A1", CalcValue::empty(), &[], Ok(CalcValue::number(0.0))),
            // A lazy call that returns the reference its argument wrote: the cell's value.
            (
                "=IF(TRUE,A1,0)",
                three.clone(),
                &["Call IF"],
                Ok(CalcValue::number(3.0)),
            ),
            // -3 + 10 = 7: the written negation, the final sum, and the check's two negations.
            (
                "=-A1+10",
                three.clone(),
                &["Prefix -", "Infix +", "Prefix - *", "Prefix - *"],
                Ok(CalcValue::number(7.0)),
            ),
            // A product is not checked.
            (
                "=A1*4",
                three.clone(),
                &["Infix *"],
                Ok(CalcValue::number(12.0)),
            ),
            // A result in the band: the check runs and refuses it.
            (
                "=0.5-0.4-0.1",
                CalcValue::empty(),
                &["Infix -", "Infix -", "Prefix - *", "Prefix - *"],
                Err(EvalFault::NearZeroFinalSum {
                    result: -2.7755575615628914e-17,
                }),
            ),
            // A result that is not a non-zero number skips the check, so no negation runs.
            (
                "=\"a\"-1",
                CalcValue::empty(),
                &["Infix -"],
                Ok(CalcValue::error(WorksheetErrorCode::Value)),
            ),
            // A lazy call is shown after the calls of the argument it asked for, which hold the
            // checked final sum of its branch.
            (
                "=IF(TRUE,A1+1,A1*2)",
                three.clone(),
                &["Infix +", "Prefix - *", "Prefix - *", "Call IF"],
                Ok(CalcValue::number(4.0)),
            ),
            // An argument the evaluation cannot supply stops the call before its function runs.
            (
                "=A1:A2+1",
                three.clone(),
                &[],
                evaluated("=A1:A2+1", three.clone()),
            ),
            // A function that refuses still ran (a product below the normal range, which
            // calc-func refuses as not modelled).
            (
                "=1E-300*1E-10",
                CalcValue::empty(),
                &["Infix *"],
                evaluated("=1E-300*1E-10", CalcValue::empty()),
            ),
        ];
        assert!(matches!(cases[8].3, Err(EvalFault::AreaAsValue { .. })));
        assert!(matches!(
            cases[9].3,
            Err(EvalFault::Function(FunctionFault::NotImplemented { .. }))
        ));
        for (formula, cells, calls, expected) in cases {
            let (outcome, shown) = observed(formula, cells.clone());
            assert_eq!(outcome, expected, "{formula}");
            assert_eq!(shown.calls, calls, "{formula}");
            assert_eq!(shown.outcomes, [expected], "{formula}");
            assert_eq!(
                shown.reports,
                [(observed_anchor(), NumericMode::Fast)],
                "{formula}"
            );
            // The observer changes nothing the evaluation returns.
            assert_eq!(evaluated(formula, cells), outcome, "{formula}");
        }
    }

    /// The three near-zero rows of cfunc-m1.2's oracle workbook, and how the final operation
    /// decides: the binary64 result of a final `+` or `-` in the band is refused, and the same
    /// difference under a final `*` is published (KB 78113's own example).
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_final_sum_near_zero_is_refused() {
        let blank = CalcValue::empty();
        assert_eq!(
            evaluated("=0.5-0.4-0.1", blank.clone()),
            Err(EvalFault::NearZeroFinalSum {
                result: -2.7755575615628914e-17
            })
        );
        assert_eq!(
            evaluated("=1.333+1.225-1.333-1.225", blank.clone()),
            Err(EvalFault::NearZeroFinalSum {
                result: -2.220446049250313e-16
            })
        );
        assert_eq!(
            evaluated("=1*(0.5-0.4-0.1)", blank.clone()),
            Ok(CalcValue::number(-2.7755575615628914e-17))
        );
        assert_eq!(
            evaluated("=(0.5-0.4-0.1)", blank.clone()),
            Err(EvalFault::NearZeroFinalSum {
                result: -2.7755575615628914e-17
            })
        );
        assert_eq!(
            evaluated("=-(0.5-0.4-0.1)", blank.clone()),
            Err(EvalFault::NearZeroFinalSum {
                result: -2.7755575615628914e-17
            })
        );
        assert_eq!(
            evaluated("=IF(TRUE,0.5-0.4-0.1)", blank.clone()),
            Err(EvalFault::NearZeroFinalSum {
                result: -2.7755575615628914e-17
            })
        );
        // Outside the band, and exactly zero, the result is published.
        assert_eq!(
            evaluated("=1-0.9999999", blank.clone()),
            Ok(CalcValue::number(1.0 - 0.9999999))
        );
        assert_eq!(
            evaluated("=0.5-0.5", blank.clone()),
            Ok(CalcValue::number(0.0))
        );
        // A text operand is read as calc-func reads it.
        assert_eq!(
            evaluated("=0.1+0.2-\"0.3\"", blank.clone()),
            Err(EvalFault::NearZeroFinalSum {
                result: 5.551115123125783e-17
            })
        );
        assert_eq!(
            evaluated("=\"3\"-\"1\"", blank.clone()),
            Ok(CalcValue::number(2.0))
        );
        // A result that is not a non-zero number is not checked: the check's negations do not
        // run. Under Exact, negating the text "1" would be refused (reading text as a number
        // rounds), so the #N/A result shows that they did not.
        assert_eq!(
            evaluated("=\"a\"-1", blank.clone()),
            Ok(CalcValue::error(WorksheetErrorCode::Value))
        );
        let exact = NumericMode::Exact(
            calc_func::value::NumericTargetId::new("test", "test", "test", "test")
                .expect("a target"),
        );
        assert_eq!(
            evaluated_under(&exact, "=#N/A+\"1\"", blank.clone()),
            Ok(CalcValue::error(WorksheetErrorCode::NA))
        );
        assert!(matches!(
            evaluated_under(&exact, "=-\"1\"", blank),
            Err(EvalFault::Function(FunctionFault::ExactUnavailable { .. }))
        ));
    }

    /// A reference to one cell is its value where a value is needed and as the result; a blank
    /// cell is 0 as the result. A reference to more cells is refused where a value is needed.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn references_where_values_are_needed() {
        assert_eq!(evaluated("=A1", text("a")), Ok(text("a")));
        assert_eq!(
            evaluated("=A1", CalcValue::empty()),
            Ok(CalcValue::number(0.0))
        );
        assert_eq!(evaluated("=A1&\"x\"", CalcValue::empty()), Ok(text("x")));
        assert_eq!(
            evaluated("=IF(TRUE,A1)", CalcValue::empty()),
            Ok(CalcValue::number(0.0))
        );
        assert!(matches!(
            evaluated("=A1:A2+1", CalcValue::number(1.0)),
            Err(EvalFault::AreaAsValue { .. })
        ));
        assert!(matches!(
            evaluated("=A1:A2", CalcValue::number(1.0)),
            Err(EvalFault::AreaAsValue { .. })
        ));
        assert_eq!(
            evaluated("=SUM(A1:B2)", CalcValue::number(1.5)),
            Ok(CalcValue::number(6.0))
        );
    }

    /// IFERROR's fallback runs only when its value is an error; a value whose evaluation faults
    /// stops the call without running the fallback; a blank value is the formula's 0.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn iferror_runs_its_fallback_only_on_an_error() {
        let blank = CalcValue::empty();
        let cases: [(&str, &[&str]); 4] = [
            ("=IFERROR(1/0,5)", &["Infix /", "Call IFERROR"]),
            ("=IFERROR(2,1/0)", &["Call IFERROR"]),
            ("=IFERROR(1E-300*1E-10,1/0)", &["Infix *", "Call IFERROR"]),
            ("=IFNA(1/0,1/0)", &["Infix /", "Call IFNA"]),
        ];
        let outcomes = [
            Ok(CalcValue::number(5.0)),
            Ok(CalcValue::number(2.0)),
            evaluated("=1E-300*1E-10", blank.clone()),
            Ok(CalcValue::error(WorksheetErrorCode::Div0)),
        ];
        for ((formula, calls), expected) in cases.into_iter().zip(outcomes) {
            let (outcome, shown) = observed(formula, blank.clone());
            assert_eq!(outcome, expected, "{formula}");
            assert_eq!(shown.calls, calls, "{formula}");
        }
        assert_eq!(
            evaluated("=IFERROR(A1,\"x\")", blank.clone()),
            Ok(CalcValue::number(0.0))
        );
        assert_eq!(
            evaluated("=IFERROR(A1,1)&\"|\"", blank.clone()),
            Ok(text("|"))
        );
        assert!(matches!(
            evaluated("=IFERROR(A1:A2,1)", blank),
            Err(EvalFault::AreaAsValue { .. })
        ));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn function_faults_pass_through() {
        assert!(matches!(
            evaluated("=1E-300*1E-10", CalcValue::empty()),
            Err(EvalFault::Function(FunctionFault::NotImplemented { .. }))
        ));
    }
}
