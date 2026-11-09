//! The host owns the callable lane for one explicit non-cell calculation.
use crate::HostSession;
pub use calc_engine::{NameCalculationProgress, NameCalculationResult};
use calc_value::{CellKey, NameId};

/// Dropping or cancelling this handle retires its activation. Effects that already
/// ran remain; no calculation result or authored/history entry is installed.
pub struct NameCalculation {
    work: Option<calc_engine::NameCalculationWork>,
    result: Option<NameCalculationResult>,
    references: Option<std::rc::Rc<calc_fml::reference::ReferenceView>>,
    facts: Option<std::sync::Arc<crate::facts::HostFacts>>,
    authority: u64,
    options: crate::HostOptions,
    lane: Option<std::rc::Rc<std::cell::Cell<usize>>>,
}
impl Drop for NameCalculation {
    fn drop(&mut self) {
        if let Some(work) = &mut self.work {
            work.cancel();
        }
        self.work = None;
        self.release();
    }
}
impl NameCalculation {
    fn release(&mut self) {
        if let Some(lane) = self.lane.take() {
            lane.set(
                lane.get()
                    .checked_sub(1)
                    .expect("an owned name calculation lane"),
            );
        }
    }
    fn collect_terminal(&mut self) {
        if self
            .work
            .as_ref()
            .is_some_and(|work| work.progress() == NameCalculationProgress::Ready)
        {
            self.result = Some(
                self.work
                    .take()
                    .expect("a terminal work handle")
                    .into_result()
                    .ok()
                    .expect("one terminal result"),
            );
            self.references = None;
            self.facts = None;
            self.release();
        }
    }
    pub fn result(&self) -> Option<&NameCalculationResult> {
        self.result.as_ref()
    }
    pub fn take_result(&mut self) -> Option<NameCalculationResult> {
        self.result.take()
    }
    pub fn progress(&self) -> NameCalculationProgress {
        if self.work.is_none() {
            NameCalculationProgress::Ready
        } else {
            NameCalculationProgress::Calculating
        }
    }
    pub fn cancel(&mut self) {
        if let Some(work) = &mut self.work {
            work.cancel();
        }
        self.collect_terminal();
    }
    pub fn advance(
        &mut self,
        session: &mut HostSession,
        quantum: usize,
    ) -> NameCalculationProgress {
        let host = &mut session.host;
        let Some(work) = &mut self.work else {
            return NameCalculationProgress::Ready;
        };
        if self.authority != host.calculation_authority
            || self.options != host.options
            || !self.facts.as_ref().is_some_and(|facts| facts.matches(host))
        {
            work.expire();
        } else {
            work.advance(
                &host.engine,
                &host.model,
                &host.options.numeric,
                &host.functions,
                host.rtd_inputs.as_ref(),
                self.references
                    .as_deref()
                    .expect("an active reference view"),
                quantum,
            );
        }
        self.collect_terminal();
        self.progress()
    }
}
impl HostSession {
    /// Explicit authorization to enter UDFs. Admission runs no user code; advance
    /// owns its ordinary continuation. Passive name diagnostics keep their policy.
    pub fn begin_name_calculation(
        &mut self,
        id: NameId,
        version: u64,
        caller: CellKey,
    ) -> Result<NameCalculation, String> {
        let host = &mut self.host;
        if host.pinned_calculations.get() != 0 || host.work.is_some() {
            return Err("Another calculation owns the callable lane".into());
        }
        let node = host
            .name_node(id)
            .ok_or("The selected name is unavailable")?;
        if node.binding_version() != version {
            return Err("The name changed; refresh before calculating it".into());
        }
        if host.model.sheet_by_id(caller.sheet()).is_none() {
            return Err("The evaluation worksheet is unavailable".into());
        }
        let formula = calc_engine::DetachedFormula::compile_name_with_bindings(
            &host.model,
            id,
            caller,
            &host.user_function_bindings(),
        )
        .map_err(|e| e.to_string())?;
        let facts = host.refresh_workbook_facts();
        let work = formula
            .begin_name_calculation(&host.engine, &host.model, caller, &host.options.numeric)
            .map_err(|e| e.to_string())?;
        host.pinned_calculations.set(1);
        Ok(NameCalculation {
            work: Some(work),
            result: None,
            references: Some(host.references.pin()),
            facts: Some(facts),
            authority: host.calculation_authority,
            options: host.options.clone(),
            lane: Some(host.pinned_calculations.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NameEdit, NameScope, SkinIntent, WorkbookCommand};
    use calc_value::{CalcValue, NumericMode};
    use std::{cell::RefCell, rc::Rc, sync::Arc};
    fn book() -> HostSession {
        HostSession::open(
            Arc::new(crate::MemorySource::new(crate::blank_package())),
            &Default::default(),
            Default::default(),
        )
        .unwrap()
    }
    fn add(book: &mut HostSession, name: &str, formula: &str) -> NameId {
        let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::Name(NameEdit {
            id: None,
            version: None,
            scope: NameScope::Workbook,
            name: name.into(),
            formula: Some(formula.into()),
        })));
        assert!(receipt.accepted(), "{:?}", receipt.error);
        book.host()
            .model()
            .definitions()
            .name_table()
            .lookup(NameScope::Workbook, name)
            .unwrap()
            .id()
    }
    fn begin(book: &mut HostSession, id: NameId) -> NameCalculation {
        let caller = CellKey::new(book.host().sheets()[0].id(), 1, 1).unwrap();
        let version = book.host().name_node(id).unwrap().binding_version();
        book.begin_name_calculation(id, version, caller).unwrap()
    }
    #[derive(Debug, Default)]
    struct Trace {
        entries: usize,
        resumes: usize,
        drops: usize,
        release: bool,
        callers: Vec<CellKey>,
        lane: Option<Rc<std::cell::Cell<usize>>>,
    }
    #[derive(Debug)]
    struct Paused(Rc<RefCell<Trace>>);
    #[derive(Debug)]
    struct Activation(Rc<RefCell<Trace>>);
    impl Drop for Activation {
        fn drop(&mut self) {
            let mut trace = self.0.borrow_mut();
            if let Some(lane) = &trace.lane {
                assert_eq!(lane.get(), 1, "cleanup still owns the callable lane");
            }
            trace.drops += 1;
        }
    }
    impl calc_fml::UserFunction for Paused {
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            panic!("cooperative entry required")
        }
        fn begin_call(
            &self,
            _: &[CalcValue],
            frame: calc_fml::UserInvocationFrame,
            context: &mut dyn calc_fml::UserFunctionContext,
        ) -> Result<Option<Box<dyn calc_fml::UserFunctionInvocation>>, String> {
            let view = frame
                .read_view
                .downcast_ref::<calc_engine::contract::InvocationReadView<
                    calc_engine::contract::NameEvaluationLocus,
                >>()
                .expect("the name root stays a typed name locus");
            assert_eq!(Some(view.root.locus.caller), context.caller());
            let mut t = self.0.borrow_mut();
            t.entries += 1;
            t.callers.push(context.caller().unwrap());
            Ok(Some(Box::new(Activation(self.0.clone()))))
        }
    }
    impl calc_fml::UserFunctionInvocation for Activation {
        fn resume(
            &mut self,
            _: &mut dyn calc_fml::UserFunctionContext,
            _: u64,
        ) -> Result<calc_fml::UserFunctionStep, String> {
            let mut t = self.0.borrow_mut();
            t.resumes += 1;
            Ok(if t.release {
                calc_fml::UserFunctionStep::Complete(CalcValue::number(7.))
            } else {
                calc_fml::UserFunctionStep::Suspended
            })
        }
    }
    fn install(book: &mut HostSession, t: Rc<RefCell<Trace>>) {
        t.borrow_mut().lane = Some(book.host.pinned_calculations.clone());
        let mut functions = calc_fml::CallableTable::new();
        functions.register("Paused", Rc::new(Paused(t))).unwrap();
        book.set_user_functions(functions);
    }
    fn enter(work: &mut NameCalculation, book: &mut HostSession, t: &Rc<RefCell<Trace>>) {
        for _ in 0..1000 {
            work.advance(book, 1);
            if t.borrow().entries != 0 {
                return;
            }
        }
        panic!("the selected name must reach its user call");
    }
    fn finish(work: &mut NameCalculation, book: &mut HostSession, quantum: usize) {
        for _ in 0..10000 {
            if work.advance(book, quantum) == NameCalculationProgress::Ready {
                return;
            }
        }
        panic!("bounded name calculation must finish");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__name_calculation_retains_effects_nested_blocks_arrays_and_draws() {
        for quantum in [1, usize::MAX] {
            let mut book = book();
            let t = Rc::new(RefCell::new(Trace::default()));
            install(&mut book, t.clone());
            let id = add(
                &mut book,
                "Effectful",
                "=LET(draw,RAND(),LAMBDA(x,IF(TRUE,Paused()+x,99))(SEQUENCE(3))+draw)",
            );
            let (revision, results, history) = (
                book.host().revision(),
                book.host().engine.result_view(),
                book.history_depth(),
            );
            let caller = CellKey::new(book.host().sheets()[0].id(), 1, 1).unwrap();
            let version = book.host().name_node(id).unwrap().binding_version();
            assert!(book.host().evaluate_name(id, version, caller).is_err());
            let mut work = begin(&mut book, id);
            assert_eq!(t.borrow().entries, 0, "admission runs no code");
            enter(&mut work, &mut book, &t);
            for _ in 0..3 {
                assert_eq!(
                    work.advance(&mut book, quantum),
                    NameCalculationProgress::Calculating
                );
            }
            assert_eq!(t.borrow().entries, 1, "a yield never repeats the prefix");
            assert!(book.begin_name_calculation(id, version, caller).is_err());
            assert!(matches!(
                book.advance_calculation(),
                crate::HostProgress::Waiting { .. }
            ));
            t.borrow_mut().release = true;
            finish(&mut work, &mut book, quantum);
            let result = work.result().unwrap();
            let evaluation = &result.evaluation;
            assert_eq!(evaluation.random_draws.samples.len(), 1);
            let draw = f64::from_bits(evaluation.random_draws.samples[0]);
            let value = evaluation.outcome.as_ref().unwrap().as_array().unwrap();
            assert_eq!(
                value
                    .iter_row_major()
                    .map(|v| v.as_number().unwrap())
                    .collect::<Vec<_>>(),
                vec![8. + draw, 9. + draw, 10. + draw]
            );
            assert_eq!(evaluation.root.as_ref().unwrap().locus.definition.id, id);
            assert_eq!(result.calls.len(), 1);
            assert!(result.calls[0].completed);
            assert_eq!(t.borrow().entries, 1);
            assert_eq!(t.borrow().drops, 1);
            assert_eq!(
                (
                    book.host().revision(),
                    book.host().engine.result_view(),
                    book.history_depth()
                ),
                (revision, results, history)
            );
            assert_eq!(
                book.host().engine.nodes().count(),
                0,
                "no synthetic formula cell or spill"
            );
            let resumes = t.borrow().resumes;
            work.advance(&mut book, 1);
            assert_eq!(t.borrow().resumes, resumes);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__name_calculation_revokes_before_more_effects_and_releases_owner() {
        for cause in [
            "cancel",
            "session-cancel",
            "drop",
            "edit",
            "clock",
            "reload",
            "other-book",
            "numeric",
            "mode",
            "file",
            "application",
            "mode-aba",
            "file-aba",
            "application-aba",
            "rebuild",
        ] {
            let mut book = book();
            let t = Rc::new(RefCell::new(Trace::default()));
            install(&mut book, t.clone());
            let id = add(&mut book, "Effectful", "=Paused()+Paused()");
            if cause == "mode-aba" {
                assert!(
                    book.dispatch(SkinIntent::SetCalcMode(crate::CalcMode::Automatic))
                        .accepted()
                );
            }
            let mut work = begin(&mut book, id);
            enter(&mut work, &mut book, &t);
            let resumes = t.borrow().resumes;
            match cause {
                "cancel" => work.cancel(),
                "session-cancel" => book.cancel_calculation(),
                "rebuild" => {
                    assert!(book.dispatch(SkinIntent::Rebuild).accepted());
                }
                "drop" => {
                    drop(work);
                    assert_eq!(t.borrow().drops, 1);
                    let _fresh = begin(&mut book, id);
                    continue;
                }
                "edit" => {
                    let at = CellKey::new(book.host().sheets()[0].id(), 2, 1).unwrap();
                    assert!(
                        book.dispatch(SkinIntent::EnterCell {
                            at,
                            text: "1".into()
                        })
                        .accepted()
                    );
                }
                "clock" => book.set_clock(Some(123.)),
                "mode" => {
                    assert!(
                        book.dispatch(SkinIntent::SetCalcMode(crate::CalcMode::Manual))
                            .accepted()
                    );
                }
                "file" => book.host.set_file(Some(calc_fml::WorkbookFile {
                    folder: Some("/changed/".into()),
                    name: "changed.xlsx".into(),
                })),
                "application" => book.host.set_application(Some(calc_fml::ApplicationFacts {
                    release: Some("changed".into()),
                    ..Default::default()
                })),
                "mode-aba" => {
                    assert!(
                        book.dispatch(SkinIntent::SetCalcMode(crate::CalcMode::Manual))
                            .accepted()
                    );
                    assert!(
                        book.dispatch(SkinIntent::SetCalcMode(crate::CalcMode::Automatic))
                            .accepted()
                    );
                }
                "file-aba" => {
                    let original = book.host.file.clone();
                    book.host.set_file(Some(calc_fml::WorkbookFile {
                        folder: None,
                        name: "temporary.xlsx".into(),
                    }));
                    book.host.set_file(original);
                }
                "application-aba" => {
                    let original = book.host.application.clone();
                    book.host.set_application(Some(calc_fml::ApplicationFacts {
                        release: Some("temporary".into()),
                        ..Default::default()
                    }));
                    book.host.set_application(original);
                }
                "reload" => install(&mut book, t.clone()),
                "other-book" => {
                    let mut other = self::book();
                    finish(&mut work, &mut other, 1);
                }
                "numeric" => {
                    book.host.options.numeric = NumericMode::Exact(
                        calc_value::NumericTargetId::new(
                            "16.0.20527.20150",
                            "Current",
                            "Excel",
                            "x86-64",
                        )
                        .unwrap(),
                    )
                }
                _ => unreachable!(),
            }
            t.borrow_mut().release = true;
            finish(&mut work, &mut book, 1);
            assert!(
                work.result().unwrap().evaluation.outcome.is_err(),
                "{cause}"
            );
            assert_eq!(t.borrow().entries, 1, "{cause}");
            assert_eq!(t.borrow().resumes, resumes, "{cause}");
            assert_eq!(t.borrow().drops, 1, "{cause}");
            assert_eq!(book.host.pinned_calculations.get(), 0, "{cause}");
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__name_calculation_refreshes_first_facts_and_keeps_current_fact_reads() {
        let mut book = HostSession::open(
            Arc::new(crate::MemorySource::new(crate::blank_package())),
            &Default::default(),
            crate::HostOptions {
                calc_mode: Some(crate::CalcMode::Manual),
                ..Default::default()
            },
        )
        .unwrap();
        book.host.set_file(Some(calc_fml::WorkbookFile {
            folder: Some("/facts/".into()),
            name: "facts.xlsx".into(),
        }));
        book.host.set_application(Some(calc_fml::ApplicationFacts {
            release: Some("engine22-test".into()),
            ..Default::default()
        }));
        let first = add(
            &mut book,
            "FirstFacts",
            "=INFO(\"recalc\")&\"|\"&INFO(\"release\")&\"|\"&CELL(\"filename\",A1)",
        );
        let mut work = begin(&mut book, first);
        finish(&mut work, &mut book, 1);
        assert_eq!(
            work.result().unwrap().evaluation.outcome,
            Ok(CalcValue::text(
                calc_value::ExcelText::from_interop_assignment(
                    "Manual|engine22-test|/facts/[facts.xlsx]Sheet1"
                )
            ))
        );
        let consumes = add(&mut book, "ConsumesFact", "=$B$1&\"\"");
        let at = CellKey::new(book.host().sheets()[0].id(), 1, 2).unwrap();
        assert!(
            book.dispatch(SkinIntent::EnterCell {
                at,
                text: "=INFO(\"release\")".into()
            })
            .accepted()
        );
        assert!(book.dispatch(SkinIntent::Recalculate).accepted());
        let before = book.host().engine.result(at).unwrap().clone();
        assert!(before.calculated().is_some());
        for quantum in [1, usize::MAX] {
            let mut work = begin(&mut book, consumes);
            finish(&mut work, &mut book, quantum);
            assert_eq!(
                work.result().unwrap().evaluation.outcome,
                Ok(CalcValue::text(
                    calc_value::ExcelText::from_interop_assignment("engine22-test")
                ))
            );
            assert_eq!(
                book.host().engine.result(at),
                Some(&before),
                "facts reuse does not calculate the precedent again"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__completed_scalar_name_keeps_evidence_without_retaining_workbook() {
        let mut book = book();
        let id = add(&mut book, "ScalarOnly", "=40+2");
        let weak = Arc::downgrade(book.host.model.definitions());
        let mut work = begin(&mut book, id);
        finish(&mut work, &mut book, 1);
        assert!(work.result().unwrap().runtime_root.is_none());
        assert!(work.result().unwrap().observations.is_empty());
        drop(book);
        assert!(
            weak.upgrade().is_none(),
            "shared workbook definitions retained by model pins retire at completion"
        );
        assert_eq!(
            work.result().unwrap().evaluation.outcome,
            Ok(CalcValue::number(42.))
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__active_name_owns_its_input_pin_until_retirement() {
        let mut book = book();
        let trace = Rc::new(RefCell::new(Trace::default()));
        install(&mut book, trace.clone());
        let id = add(&mut book, "ActivePin", "=Paused()");
        let weak = Arc::downgrade(book.host.model.definitions());
        let mut work = begin(&mut book, id);
        enter(&mut work, &mut book, &trace);
        drop(book);
        assert!(
            weak.upgrade().is_some(),
            "active work retains its required model data"
        );
        drop(work);
        assert_eq!(trace.borrow().drops, 1);
        assert!(
            weak.upgrade().is_none(),
            "retirement releases unused model data"
        );
    }

    #[derive(Debug)]
    struct CacheRead {
        at: CellKey,
        entries: Rc<std::cell::Cell<usize>>,
    }
    impl calc_fml::UserFunction for CacheRead {
        fn cached_reads(&self) -> bool {
            true
        }
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            panic!("pinned context required")
        }
        fn call_with_context(
            &self,
            _: &[CalcValue],
            context: &mut dyn calc_fml::UserFunctionContext,
        ) -> Result<CalcValue, String> {
            self.entries.set(self.entries.get() + 1);
            context.values_of(&calc_value::ReferenceLike::cell(self.at))
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__name_calculation_retains_cache_qualifications_and_refuses_producer_demand() {
        for quantum in [1, usize::MAX] {
            let mut book = HostSession::open(
                Arc::new(crate::MemorySource::new(
                    include_bytes!("../../calc-engine/tests/fixtures/dc_86f/oracle.xlsx").to_vec(),
                )),
                &Default::default(),
                crate::HostOptions {
                    calc_mode: Some(crate::CalcMode::Manual),
                    ..Default::default()
                },
            )
            .unwrap();
            let source = CellKey::new(book.host().sheets()[0].id(), 1, 1).unwrap();
            let cache = book.host().engine.result(source).unwrap().clone();
            assert!(matches!(cache, calc_engine::FormulaResult::FileCached(_)));
            let entries = Rc::new(std::cell::Cell::new(0));
            let mut functions = calc_fml::CallableTable::new();
            functions
                .register(
                    "ObserveCache",
                    Rc::new(CacheRead {
                        at: source,
                        entries: entries.clone(),
                    }),
                )
                .unwrap();
            book.set_user_functions(functions);
            let id = add(&mut book, "Observed", "=ObserveCache()");
            let mut work = begin(&mut book, id);
            finish(&mut work, &mut book, quantum);
            let result = work.result().unwrap();
            assert!(result.evaluation.outcome.is_ok());
            assert!(
                result
                    .observations
                    .iter()
                    .any(|o| o.lineage.as_ref().is_some_and(|s| s.imported > 0)),
                "an actual UDF observation keeps imported provenance"
            );
            assert_eq!(book.host().engine.result(source), Some(&cache));
            assert_eq!(entries.get(), 1);
            let id = add(&mut book, "NeedsProducer", "=IFERROR(ObserveCache()+A1,99)");
            let mut work = begin(&mut book, id);
            finish(&mut work, &mut book, quantum);
            assert!(
                work.result().unwrap().evaluation.outcome.is_err(),
                "no producer, cached fallback or prefix replay is admitted"
            );
            assert_eq!(entries.get(), 2);
            assert_eq!(book.host().engine.result(source), Some(&cache));
        }
    }

    #[derive(Debug)]
    struct Awaiter(Rc<RefCell<Trace>>);
    #[derive(Debug)]
    struct AwaitingActivation(Rc<RefCell<Trace>>);
    impl Drop for AwaitingActivation {
        fn drop(&mut self) {
            self.0.borrow_mut().drops += 1;
        }
    }
    impl calc_fml::UserFunction for Awaiter {
        fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
            panic!("cooperative entry required")
        }
        fn begin_call(
            &self,
            _: &[CalcValue],
            _: calc_fml::UserInvocationFrame,
            _: &mut dyn calc_fml::UserFunctionContext,
        ) -> Result<Option<Box<dyn calc_fml::UserFunctionInvocation>>, String> {
            self.0.borrow_mut().entries += 1;
            Ok(Some(Box::new(AwaitingActivation(self.0.clone()))))
        }
    }
    impl calc_fml::UserFunctionInvocation for AwaitingActivation {
        fn resume(
            &mut self,
            _: &mut dyn calc_fml::UserFunctionContext,
            _: u64,
        ) -> Result<calc_fml::UserFunctionStep, String> {
            self.0.borrow_mut().resumes += 1;
            Ok(calc_fml::UserFunctionStep::Awaiting { request: 7 })
        }
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__name_calculation_wait_is_typed_and_never_creates_a_cell_return_token() {
        let mut book = book();
        let trace = Rc::new(RefCell::new(Trace::default()));
        let mut functions = calc_fml::CallableTable::new();
        functions
            .register("Awaiter", Rc::new(Awaiter(trace.clone())))
            .unwrap();
        book.set_user_functions(functions);
        let id = add(&mut book, "WaitingName", "=IFERROR(Awaiter(),99)");
        let mut work = begin(&mut book, id);
        finish(&mut work, &mut book, 1);
        let result = work.result().unwrap();
        assert!(result.evaluation.outcome.is_err());
        assert_eq!(result.calls.len(), 1);
        assert!(!result.calls[0].completed);
        assert_eq!(trace.borrow().entries, 1);
        assert_eq!(trace.borrow().resumes, 1);
        assert_eq!(trace.borrow().drops, 1);
        assert!(book.host().pending_async().is_empty());
        assert_eq!(book.host().engine.nodes().count(), 0);
        work.advance(&mut book, usize::MAX);
        assert_eq!(trace.borrow().entries, 1);
        assert_eq!(book.host.pinned_calculations.get(), 0);
    }
}
