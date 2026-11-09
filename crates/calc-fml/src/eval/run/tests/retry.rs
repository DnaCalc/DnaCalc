//! Replay safety of the retry path (dc-hsa5.19, Pro A3 and its risk table, row 1).
//!
//! An instruction of [`waits_for_inputs`] that faults with `MissingInput` for a cell whose
//! producer is pending is run again from its start once the producer is done. That is safe only
//! if running its start twice does nothing the evaluation could observe twice. The user-function
//! counter alone does not show it, so every case here is driven twice, with the producer ready
//! from the start and with the producer pending until the evaluation has waited, and the two runs
//! must show the same events: the value, the draws of the random source, the user-function
//! calls, the calls and resolved text the observer is shown, the defined-name records and the
//! fact reads. A formula whose wait is not taken in place (the evaluation ends with
//! `MissingInput` and the engine evaluates it again from its start, which is the model's
//! documented fallback) is reported as such and compared on nothing but its value.

use super::*;
use std::cell::RefCell;
use std::rc::Rc;

/// What one evaluation did that something outside it could see, in order.
#[derive(Clone, Debug, Default, PartialEq)]
struct Events {
    /// The unit values the random source gave.
    draws: Vec<f64>,
    /// The arguments of each user-function call.
    user: Vec<String>,
    /// The calls the observer was shown.
    calls: Vec<String>,
    /// The areas of reference text the observer was shown resolved.
    resolved: Vec<String>,
    /// The defined-name invocations the effects recorded: ordinal, parent and result kind.
    names: Vec<String>,
    /// The distinct fact reads the reader saw (a fact read is a read, and its repetition is not an
    /// effect).
    facts: Vec<String>,
}

#[derive(Debug)]
struct Tick(Rc<RefCell<Vec<String>>>);

impl calc_func::UserFunction for Tick {
    fn call(&self, arguments: &[CalcValue]) -> Result<CalcValue, String> {
        self.0.borrow_mut().push(format!(
            "{:?}",
            arguments.iter().map(CalcValue::as_number).collect::<Vec<_>>()
        ));
        Ok(CalcValue::number(42.0))
    }
}

struct Draws(Rc<RefCell<Vec<f64>>>);

impl RandomSource for Draws {
    fn next_unit(&mut self) -> f64 {
        let mut draws = self.0.borrow_mut();
        let unit = (draws.len() + 1) as f64 * 0.0625;
        draws.push(unit);
        unit
    }
}

#[derive(Default)]
struct Log {
    calls: Vec<String>,
    resolved: Vec<String>,
}

impl EvaluationObserver for Log {
    fn called(&mut self, call: ExecutedCall) {
        let target = call.target();
        self.calls.push(format!("{} {}", target.spelling(), if call.synthetic() { "*" } else { "" }));
    }

    fn resolved(&mut self, reference: ResolvedReference) {
        self.resolved.push(format!("{:?}", reference.area()));
    }

    fn returned(&mut self, _: EvaluationReport<'_>) {}
}

/// Every cell holds 5 except the `pending` ones, which read as missing, and name their producer
/// as pending, until `ready` is set.
struct Gated {
    pending: Vec<CellKey>,
    ready: bool,
    facts: Vec<String>,
}

impl ReferenceReader for Gated {
    fn pending_producer(&self, at: CellKey) -> bool {
        !self.ready && self.pending.contains(&at)
    }

    fn read_denial(&self) -> Option<crate::ReadDenial> {
        None
    }

    fn read_fact(&mut self, fact: crate::FactRead) {
        self.facts.push(format!("{fact:?}"));
    }

    fn spill_extent(&mut self, _: CellKey) -> crate::reference::SpillExtent {
        crate::reference::SpillExtent::NotModelled
    }

    fn subtotal_exclusions(&mut self, _: GridArea) -> Option<calc_func::SubtotalExclusions> {
        None
    }

    fn read_area(
        &mut self,
        _request: crate::ReadRequest,
        area: GridArea,
        sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>,
    ) {
        // Row-major, a pending cell as a missing run of its own, as the engine's reader gives it.
        let five = CalcValue::number(5.0);
        let (first, last) = (area.first(), area.last());
        for row in first.row()..=last.row() {
            for column in first.column()..=last.column() {
                let at = CellKey::new(first.sheet(), row, column).expect("a cell of the area");
                let run = if !self.ready && self.pending.contains(&at) {
                    CellRun::Missing(1)
                } else {
                    CellRun::Values(std::slice::from_ref(&five))
                };
                if sink(run).is_break() {
                    return;
                }
            }
        }
    }
}

/// A catalogue with one defined name, `Drawn`, whose formula is given.
struct Named {
    formula: &'static str,
    id: calc_func::value::NameId,
}

impl Named {
    fn new(formula: &'static str) -> Self {
        let mut ids: IdAllocator<calc_func::value::NameId> = IdAllocator::new();
        Named {
            formula,
            id: ids.issue().expect("a name id"),
        }
    }
}

impl ReferenceCatalog for Named {
    fn defined_name(&self, name: &str, _: crate::reference::NameScope) -> crate::reference::NameDefinition {
        if name.eq_ignore_ascii_case("Drawn") {
            crate::reference::NameDefinition::Formula(self.formula.into())
        } else {
            crate::reference::NameDefinition::Unknown
        }
    }

    fn name_binding(
        &self,
        name: &str,
        _: crate::reference::NameScope,
        _: Option<calc_func::value::NameId>,
    ) -> Option<crate::reference::NameBindingProof> {
        name.eq_ignore_ascii_case("Drawn").then(|| crate::reference::NameBindingProof {
            resolved: Some(crate::reference::BoundName {
                id: self.id,
                version: 1,
                scope: calc_func::value::NameScope::Workbook,
            }),
            probes: Vec::new(),
            parent: None,
        })
    }

    fn sheet_named(&self, name: &str) -> Option<SheetId> {
        NoSheets.sheet_named(name)
    }

    fn table(&self, name: &str) -> crate::reference::TableLookup {
        NoSheets.table(name)
    }

    fn table_containing(&self, cell: CellKey) -> crate::reference::TableLookup {
        NoSheets.table_containing(cell)
    }

    fn sheet_span(&self, first: SheetId, last: SheetId) -> Option<Vec<SheetId>> {
        NoSheets.sheet_span(first, last)
    }

    fn unknown_calls_are_names(&self) -> bool {
        false
    }

    fn workbook_name(&self) -> Option<&str> {
        None
    }

    fn external_sheet(&self, book: &crate::reference::ExternalBookRef, sheet: &str) -> Option<SheetId> {
        NoSheets.external_sheet(book, sheet)
    }

    fn external_function_target(&self, book: &crate::reference::ExternalBookRef) -> Option<String> {
        NoSheets.external_function_target(book)
    }
}

/// How a pending run ended.
#[derive(Clone, Debug, PartialEq)]
enum Ending {
    /// The producer was ready from the start.
    Ready,
    /// The evaluation waited this many times and then completed.
    Waited(usize),
    /// The evaluation ended with `MissingInput`: the engine would evaluate it again from its start.
    FellBack,
}

struct Run2 {
    ending: Ending,
    events: Events,
    value: Result<CalcValue, EvalFault>,
}

fn drive(formula: &str, pending: bool, quantum: usize) -> Run2 {
    let anchor = observed_anchor();
    // B3, E3 and B8 stand for cells whose producers are still to be calculated.
    let sheet = anchor.cell().sheet();
    let cells = vec![
        CellKey::new(sheet, 3, 2).unwrap(),
        CellKey::new(sheet, 3, 5).unwrap(),
    ];
    let user = Rc::new(RefCell::new(Vec::new()));
    let draws = Rc::new(RefCell::new(Vec::new()));
    let mut functions = crate::CallableTable::new();
    functions.register("Tick", Rc::new(Tick(user.clone()))).unwrap();
    let catalog = Named::new("RAND()+$B$3");
    let plan = crate::compile_with_functions(
        &parse_formula(formula),
        ReferenceNotation::A1,
        anchor,
        &catalog,
        &mut PlanCache::new(),
        FormulaForm::Dynamic,
        &functions,
    )
    .unwrap_or_else(|error| panic!("{formula}: {error:?}"));
    let instance = instantiate(&plan, anchor);
    let mut activation = EvalContinuation::new(&instance, EvalScratch::new());
    let mut reader = Gated {
        pending: cells,
        ready: !pending,
        facts: Vec::new(),
    };
    let mut effects = crate::UserFunctionEffects::default();
    let mut random = Draws(draws.clone());
    let mut log = Log::default();
    let mut waits = 0;
    let value = loop {
        let step = activation.advance(
            &mut FormulaEvaluationContext::new(
                &NumericMode::Fast,
                &mut reader,
                None,
                None,
                crate::ReadRequest::DEPENDENCY,
            )
            .with_catalog(&catalog)
            .with_functions(&functions)
            .with_user_function_effects(&mut effects)
            .with_random(&mut random)
            .observed_by(&mut log),
            quantum,
        );
        match step {
            EvalStep::Suspended(_) => {
                if activation.waiting_input().is_some() {
                    waits += 1;
                    assert!(waits < 20, "{formula}: waits without progress");
                    reader.ready = true;
                }
            }
            EvalStep::Complete(outcome) => break outcome,
        }
    };
    let fell_back = matches!(&value, Err(EvalFault::MissingInput { .. })) && pending && waits == 0;
    let names = effects
        .name_invocations()
        .iter()
        .map(|name| format!("{} {:?} {:?}", name.ordinal, name.parent, name.result))
        .collect();
    let events = Events {
        draws: draws.borrow().clone(),
        user: user.borrow().clone(),
        calls: log.calls,
        resolved: log.resolved,
        names,
        facts: {
            let mut facts = std::mem::take(&mut reader.facts);
            facts.sort();
            facts.dedup();
            facts
        },
    };
    Run2 {
        ending: if !pending {
            Ending::Ready
        } else if fell_back {
            Ending::FellBack
        } else {
            Ending::Waited(waits)
        },
        events,
        value,
    }
}

/// One case: its formula, and whether it is expected to wait in place (false: it reads no value).
const CASES: &[(&str, bool)] = &[
    // An eager call: its operand is read at admission, or by its kernel.
    ("=RAND()+B3+RAND()", true),
    ("=SUM(RAND(),B3)", true),
    ("=SUM(RAND(),B1:B4,RAND())", true),
    ("=SUM(B1:B4)+RAND()", true),
    ("=SUM(Tick(1),B1:B4,Tick(2))", true),
    // An eager call run once per element of an array of references: element 2 reads B3.
    ("=SUM(N(OFFSET(B1,{0;1;2},0)))", true),
    ("=SUM(N(OFFSET(B1,{0;1;2},0)))+RAND()", true),
    // A call run per element whose kernel draws.
    ("=SUM(RANDBETWEEN({1;2;3},10))+B3", true),
    // A lazy call: its selector or a selected argument is a reference read at admission.
    ("=IF(B3,RAND(),0)", true),
    ("=IF(RAND()<0.5,B3,1)", true),
    ("=IFERROR(B3,RAND())", true),
    ("=IFS(RAND()>2,1,B3>0,2)", true),
    ("=CHOOSE(2,RAND(),B3)", true),
    ("=IF({TRUE;FALSE;TRUE},B3,RAND())", true),
    // A call of a function value or a LAMBDA.
    ("=LAMBDA(x,x+B3)(RAND())", true),
    ("=LET(f,SUM,f(RAND(),B3))", true),
    ("=LET(f,SUM,f(B3,RAND()))", true),
    ("=LET(f,SUM,f(RAND(),B1:B4))", true),
    // A kernel that calls a LAMBDA through the retained helper path.
    ("=MAP({1;2},LAMBDA(x,x+RAND()+B3))", true),
    ("=SCAN(0,{1;2;3},LAMBDA(a,v,a+v+RAND()+B3))", true),
    ("=REDUCE(0,{1;2;3},LAMBDA(a,v,a+v+RAND()+B3))", true),
    ("=BYROW({1,2;3,4},LAMBDA(r,SUM(r)+RAND()+B3))", true),
    ("=MAKEARRAY(2,2,LAMBDA(r,c,r+c+RAND()+B3))", true),
    // GROUPBY and PIVOTBY call their LAMBDA through the retained helper path too (dc-hsa5.19, D2).
    ("=GROUPBY({\"a\";\"b\";\"a\"},{1;2;3},LAMBDA(x,SUM(x)+RAND()+B3))", true),
    ("=GROUPBY({\"a\";\"b\";\"a\"},{1;2;3},LAMBDA(x,B3+SUM(x)),0,-1)", true),
    ("=PIVOTBY({\"a\";\"b\";\"a\"},{\"x\";\"x\";\"y\"},{1;2;3},LAMBDA(x,SUM(x)+RAND()+B3))", true),
    // A kernel whose LAMBDA reads, through a name.
    ("=SUM(MAP({1;2},LAMBDA(x,x+B3)),RAND())", true),
    // A defined name whose body draws and reads.
    ("=Drawn+RAND()", true),
    ("=SUM(Drawn,RAND())", true),
    ("=LET(n,Drawn,n+RAND())", true),
    // Facts read before a cell read.
    ("=CELL(\"contents\",B3)&\"\"", true),
    ("=ISFORMULA(B3)", false),
    // Reference text resolved before its read (an INDIRECT with no catalogue is refused alike).
    ("=SUM(INDIRECT(\"B1:B4\"))+RAND()", true),
];

fn compare(formula: &str, quantum: usize) -> (Ending, Vec<String>) {
    let ready = drive(formula, false, quantum);
    let pending = drive(formula, true, quantum);
    let mut differences = Vec::new();
    if pending.ending == Ending::FellBack {
        if pending.value.is_ok() || ready.value.is_err() {
            differences.push(format!("{formula}: fell back, ready {:?}, pending {:?}", ready.value, pending.value));
        }
        return (pending.ending, differences);
    }
    if ready.value != pending.value {
        differences.push(format!("{formula}: value {:?} against {:?}", ready.value, pending.value));
    }
    if ready.events != pending.events {
        differences.push(format!(
            "{formula}: events differ\n    ready   {:?}\n    pending {:?}",
            ready.events, pending.events
        ));
    }
    (pending.ending, differences)
}

/// Every waiting instruction, driven with and without a pending producer, does what it does once.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn a_wait_replays_nothing_the_evaluation_could_observe() {
    let mut differences = Vec::new();
    let mut report = Vec::new();
    for quantum in [usize::MAX, 1] {
        for (formula, in_place) in CASES {
            let (ending, found) = compare(formula, quantum);
            report.push(format!("quantum {quantum}: {formula} -> {ending:?}"));
            differences.extend(found);
            if *in_place && !matches!(ending, Ending::Waited(waits) if waits > 0) {
                differences.push(format!("{formula}: expected a wait in place, got {ending:?}"));
            }
            if !*in_place && !matches!(ending, Ending::Waited(0)) {
                differences.push(format!("{formula}: expected no wait, got {ending:?}"));
            }
        }
    }
    assert!(differences.is_empty(), "{}\n--\n{}", differences.join("\n"), report.join("\n"));
}
