//! An isolated calculation of one immutable model for a save. It uses the ordinary
//! engine evaluator, graph and publication code, and exposes no authored mutation API.
use crate::{
    AsyncReturns, EngineState, ReadObserver, RecalcFailure, RecalcProgress, RecalcWork, StartError,
    WorkBudget, contract::*,
};
use calc_doc_model::ModelRoot;
use calc_fml::{CallableTable, RtdCut, reference::ReferenceView};
use calc_value::{CalcValue, NumericMode};
use std::{rc::Rc, sync::Arc};

pub struct PinnedCalculation {
    state: EngineState,
    inputs: Arc<ModelRoot>,
    work: Option<RecalcWork>,
    returns: AsyncReturns,
    events: Vec<CalcEvent>,
    completion: Option<CompletionReceipt>,
    failure: Option<RecalcFailure>,
}

impl PinnedCalculation {
    pub fn begin(
        source: &EngineState,
        inputs: Arc<ModelRoot>,
        numeric: NumericMode,
        save: CommandId,
        functions: &CallableTable,
        external_inputs: Rc<dyn RtdCut>,
        references: Rc<ReferenceView>,
        facts: Arc<dyn calc_fml::WorkbookFacts>,
    ) -> Result<Self, StartError> {
        let mut state = source.snapshot_for_save(save);
        state.set_workbook_facts(Some(facts));
        state.set_tables_deferred(false);
        let work = crate::begin_recalc_scoped(
            &mut state,
            &inputs,
            numeric,
            CalcScope::Workbook,
            EvalSelection::DirtyAndVolatile,
        )?
        .with_source(save)
        .with_functions(functions)
        .with_external_inputs(external_inputs)
        .with_reference_view(references);
        let mut calculation = Self {
            state,
            inputs,
            work: Some(work),
            returns: AsyncReturns::default(),
            events: Vec::new(),
            completion: None,
            failure: None,
        };
        calculation.collect();
        Ok(calculation)
    }
    fn record(&mut self, events: Vec<CalcEvent>) {
        self.returns.observe(&events);
        for event in &events {
            if let CalcEvent::Completed(receipt) = event {
                self.completion = Some(receipt.clone());
            }
        }
        self.events.extend(events);
    }
    fn collect(&mut self) {
        let events = self
            .work
            .as_mut()
            .map(RecalcWork::take_events)
            .unwrap_or_default();
        self.record(events);
    }
    pub fn results(&self) -> &EngineState {
        &self.state
    }
    pub fn completion(&self) -> Option<&CompletionReceipt> {
        self.completion.as_ref()
    }
    pub fn failure(&self) -> Option<&RecalcFailure> {
        self.failure.as_ref()
    }
    pub fn take_events(&mut self) -> Vec<CalcEvent> {
        std::mem::take(&mut self.events)
    }
    pub fn pending_async(&self) -> Vec<AsyncToken> {
        self.work
            .as_ref()
            .map(|work| work.pending_async().cloned().collect())
            .unwrap_or_default()
    }
    pub fn is_waiting(&self) -> bool {
        self.work
            .as_ref()
            .is_some_and(RecalcWork::is_waiting_external)
    }
    pub fn remaining(&self) -> usize {
        self.work.as_ref().map_or(0, RecalcWork::remaining)
    }
    pub fn retire_returns(&mut self) -> AsyncReturns {
        self.cancel();
        std::mem::take(&mut self.returns)
    }
    pub fn advance(
        &mut self,
        budget: WorkBudget,
        observer: &mut dyn ReadObserver,
    ) -> RecalcProgress {
        let Some(work) = self.work.as_mut() else {
            return if self.failure.is_some() {
                RecalcProgress::Failed
            } else {
                RecalcProgress::Complete
            };
        };
        let progress = crate::step(&self.state, &self.inputs, work, budget, observer);
        self.collect();
        if matches!(progress, RecalcProgress::Complete | RecalcProgress::Failed) {
            let work = self.work.take().expect("terminal work");
            let failed = work.completion(CompletionOutcome::Failed, self.state.result_view(), None);
            match crate::finish(&mut self.state, &self.inputs, work) {
                Ok(delta) => self.record(delta.events().to_vec()),
                Err(failure) => {
                    self.failure = Some(failure);
                    self.record(vec![CalcEvent::Completed(failed)]);
                    return RecalcProgress::Failed;
                }
            }
        }
        progress
    }
    pub fn complete_async(&mut self, token: &AsyncToken, value: CalcValue) -> AsyncDisposition {
        let disposition =
            self.returns
                .deliver(&self.state, &self.inputs, self.work.as_mut(), token, value);
        self.events.push(CalcEvent::AsyncReturned {
            token: token.id,
            disposition,
        });
        disposition
    }
    pub fn cancel(&mut self) {
        if let Some(work) = self.work.take() {
            let receipt =
                work.completion(CompletionOutcome::Cancelled, self.state.result_view(), None);
            self.returns.revoke();
            self.record(vec![CalcEvent::Completed(receipt)]);
        }
    }
}
