//! Calculated saves wait for an existing invocation or calculate one isolated model pin.
use crate::{HostState, PreparedSave, SaveFailed, SavePin};
use calc_doc::{LoadedWorkbook, SavePolicy};
use calc_doc_model::{AuthoredRevision, AxisEdit, ModelRoot};
use calc_engine::{
    PinnedCalculation, RecalcFailure, RecalcProgress, StartError, WorkBudget, contract::*,
};
use calc_value::CalcValue;
use std::{cell::Cell, fmt, rc::Rc, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalculatedSaveProgress {
    Calculating,
    Waiting,
    WaitingForLive,
    Ready,
    Failed,
}

#[derive(Debug)]
pub enum CalculatedSaveError {
    Busy,
    Pending,
    Superseded,
    ProviderChanged,
    Cancelled,
    Incomplete(CompletionReceipt),
    MissingCompletion,
    ReturnIdentityConflict(AsyncRejection),
    Start(StartError),
    Calculation(RecalcFailure),
    Save(SaveFailed),
}
impl fmt::Display for CalculatedSaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => f.write_str("another pinned save owns this calculation lane (dc-hsa5.4)"),
            Self::Pending => f.write_str("the save calculation has not completed"),
            Self::Superseded => {
                f.write_str("the save's workbook or required live calculation was replaced")
            }
            Self::ProviderChanged => f.write_str("the save's callable provider was replaced"),
            Self::Cancelled => f.write_str("the save calculation was cancelled"),
            Self::Incomplete(receipt) => write!(
                f,
                "the save calculation is incomplete: {:?}",
                receipt.outcome
            ),
            Self::MissingCompletion => {
                f.write_str("the required live result view is no longer available")
            }
            Self::ReturnIdentityConflict(reason) => write!(
                f,
                "the save return history conflicts with its owner: {reason:?}"
            ),
            Self::Start(error) => write!(f, "the save calculation could not start: {error}"),
            Self::Calculation(error) => write!(f, "the save calculation failed: {error}"),
            Self::Save(error) => write!(f, "{error}"),
        }
    }
}
impl std::error::Error for CalculatedSaveError {}

struct SaveLane(Rc<Cell<usize>>);
impl Drop for SaveLane {
    fn drop(&mut self) {
        self.0.set(0);
    }
}

/// Owns an immutable model, callable/input leases and a separate result destination.
/// A live invocation is waited for instead of being copied or invoked a second time.
pub struct CalculatingSave {
    id: CommandId,
    pin: SavePin,
    loaded: LoadedWorkbook,
    model: Arc<ModelRoot>,
    axis_edits: Vec<AxisEdit>,
    mode: calc_doc::CalcMode,
    policy: SavePolicy,
    functions: calc_fml::CallableBindings,
    waiting_live: Option<CalcCommandId>,
    calculation: Option<Box<PinnedCalculation>>,
    lane: Option<SaveLane>,
    terminal: Option<Result<PreparedSave, CalculatedSaveError>>,
    events: Vec<calc_engine::CalcEvent>,
}

pub fn begin_calculated_save(
    host: &mut HostState,
    policy: &SavePolicy,
) -> Result<CalculatingSave, CalculatedSaveError> {
    if host.pinned_calculations.get() != 0 {
        return Err(CalculatedSaveError::Busy);
    }
    // An internal save owns a namespace from the same allocator as host sessions;
    // it cannot alias an earlier UI command's sequence in the live session.
    let id = CommandId {
        client_session: crate::state::next_command_scope(),
        sequence: 1,
    };
    let waiting_live = host.work.as_ref().map(|work| work.admission().command);
    let mut save = CalculatingSave {
        id,
        pin: SavePin {
            workbook: host.engine.workbook_instance(),
            model: host.revision(),
            results: host.engine.result_view(),
            active_calculation: waiting_live,
            cache_policy: crate::SaveCachePolicy::CurrentCalculatedOnly,
            destination: ResultDestination::PinnedSave { save: id },
        },
        loaded: host.loaded.clone(),
        model: host.model.clone(),
        axis_edits: host.axis_edits.clone(),
        mode: host.calc_mode(),
        policy: *policy,
        functions: host.functions.bindings(),
        waiting_live,
        calculation: None,
        lane: None,
        terminal: None,
        events: Vec::new(),
    };
    if waiting_live.is_none() {
        let mut calculation = PinnedCalculation::begin(
            &host.engine,
            host.model.clone(),
            host.options.numeric.clone(),
            id,
            &host.functions,
            host.rtd_inputs.clone(),
            host.references.pin(),
            host.workbook_facts(),
        )
        .map_err(CalculatedSaveError::Start)?;
        save.events.extend(calculation.take_events());
        save.calculation = Some(Box::new(calculation));
        host.pinned_calculations.set(1);
        save.lane = Some(SaveLane(host.pinned_calculations.clone()));
    }
    Ok(save)
}

impl CalculatingSave {
    pub fn progress(&self) -> CalculatedSaveProgress {
        if let Some(result) = &self.terminal {
            if result.is_ok() {
                CalculatedSaveProgress::Ready
            } else {
                CalculatedSaveProgress::Failed
            }
        } else if self.waiting_live.is_some() {
            CalculatedSaveProgress::WaitingForLive
        } else if self
            .calculation
            .as_ref()
            .is_some_and(|calculation| calculation.is_waiting())
        {
            CalculatedSaveProgress::Waiting
        } else {
            CalculatedSaveProgress::Calculating
        }
    }
    pub fn remaining(&self) -> usize {
        self.calculation
            .as_ref()
            .map_or(0, |calculation| calculation.remaining())
    }
    pub fn needs_advance(&self, live: &HostState) -> bool {
        self.terminal.is_none()
            && (self.authority(live).is_err()
                || self.waiting_live.is_some_and(|command| {
                    live.revision() != self.pin.model
                        || live
                            .work
                            .as_ref()
                            .is_none_or(|work| work.admission().command != command)
                })
                || self.has_pending_calculation())
    }
    pub fn id(&self) -> CommandId {
        self.id
    }
    pub fn model_revision(&self) -> AuthoredRevision {
        self.pin.model
    }
    pub fn take_events(&mut self) -> Vec<calc_engine::CalcEvent> {
        std::mem::take(&mut self.events)
    }
    pub fn pending_async(&self) -> Vec<AsyncToken> {
        self.calculation
            .as_ref()
            .map(|calculation| calculation.pending_async())
            .unwrap_or_default()
    }
    pub fn has_pending_calculation(&self) -> bool {
        self.terminal.is_none()
            && self.calculation.as_ref().is_some_and(|calculation| {
                calculation.completion().is_none() && !calculation.is_waiting()
            })
    }
    pub fn error(&self) -> Option<&CalculatedSaveError> {
        self.terminal
            .as_ref()
            .and_then(|result| result.as_ref().err())
    }
    fn authority(&self, live: &HostState) -> Result<(), CalculatedSaveError> {
        if live.engine.workbook_instance() != self.pin.workbook {
            Err(CalculatedSaveError::Superseded)
        } else if live.functions.bindings() != self.functions {
            Err(CalculatedSaveError::ProviderChanged)
        } else {
            Ok(())
        }
    }
    fn fail(&mut self, error: CalculatedSaveError) -> CalculatedSaveProgress {
        if let Some(calculation) = &mut self.calculation {
            calculation.cancel();
            self.events.extend(calculation.take_events());
        }
        self.lane = None;
        self.terminal = Some(Err(error));
        CalculatedSaveProgress::Failed
    }
    pub fn complete_async(
        &mut self,
        live: &HostState,
        token: &AsyncToken,
        value: CalcValue,
    ) -> AsyncDisposition {
        if self.terminal.is_none()
            && let Err(error) = self.authority(live)
        {
            let reason = if matches!(error, CalculatedSaveError::ProviderChanged) {
                AsyncRejection::ProviderChanged
            } else {
                AsyncRejection::TargetChanged
            };
            self.fail(error);
            let disposition = AsyncDisposition::Rejected(reason);
            self.events.push(calc_engine::CalcEvent::AsyncReturned {
                token: token.id,
                disposition,
            });
            return disposition;
        }
        let Some(calculation) = &mut self.calculation else {
            return AsyncDisposition::Rejected(AsyncRejection::UnknownToken);
        };
        let disposition = calculation.complete_async(token, value);
        self.events.extend(calculation.take_events());
        disposition
    }
    pub fn advance(&mut self, live: &HostState, budget: WorkBudget) -> CalculatedSaveProgress {
        if let Some(result) = &self.terminal {
            return if result.is_ok() {
                CalculatedSaveProgress::Ready
            } else {
                CalculatedSaveProgress::Failed
            };
        }
        if let Err(error) = self.authority(live) {
            return self.fail(error);
        }
        if let Some(command) = self.waiting_live {
            if live.revision() != self.pin.model {
                return self.fail(CalculatedSaveError::Superseded);
            }
            if live
                .work
                .as_ref()
                .is_some_and(|work| work.admission().command == command)
            {
                return CalculatedSaveProgress::WaitingForLive;
            }
            let Some(receipt) = live.last_calculation_completion.as_ref().filter(|receipt| {
                receipt.command == command && receipt.results == live.engine.result_view()
            }) else {
                return self.fail(CalculatedSaveError::MissingCompletion);
            };
            let result = self.prepare(&live.engine, receipt);
            self.terminal = Some(result);
        } else {
            let calculation = self.calculation.as_mut().expect("pinned calculation");
            let progress = calculation.advance(budget, &mut calc_engine::Unobserved);
            self.events.extend(calculation.take_events());
            match progress {
                RecalcProgress::Pending { .. } => return CalculatedSaveProgress::Calculating,
                RecalcProgress::Waiting { .. } => return CalculatedSaveProgress::Waiting,
                RecalcProgress::Failed => {
                    let error = calculation
                        .failure()
                        .cloned()
                        .map(CalculatedSaveError::Calculation)
                        .unwrap_or(CalculatedSaveError::Cancelled);
                    return self.fail(error);
                }
                RecalcProgress::Complete => {}
            }
            let calculation = self.calculation.as_ref().expect("pinned calculation");
            let receipt = calculation
                .completion()
                .expect("terminal calculation receipt");
            let result = self.prepare(calculation.results(), receipt);
            self.terminal = Some(result);
        }
        self.lane = None;
        if self.terminal.as_ref().expect("prepared outcome").is_ok() {
            CalculatedSaveProgress::Ready
        } else {
            CalculatedSaveProgress::Failed
        }
    }
    fn prepare(
        &self,
        engine: &calc_engine::EngineState,
        receipt: &CompletionReceipt,
    ) -> Result<PreparedSave, CalculatedSaveError> {
        if receipt.outcome != CompletionOutcome::Complete
            || receipt.coverage.calculated != receipt.coverage.required
            || !engine
                .stale(&self.model)
                .expect("pinned input revision")
                .is_empty()
        {
            return Err(CalculatedSaveError::Incomplete(receipt.clone()));
        }
        let mut pin = self.pin.clone();
        pin.results = engine.result_view();
        pin.active_calculation = None;
        pin.destination = receipt.destination;
        crate::save::prepare_image(
            &self.loaded,
            &self.model,
            engine,
            &self.axis_edits,
            self.mode,
            pin,
            &self.policy,
            Some(receipt.clone()),
        )
        .map_err(CalculatedSaveError::Save)
    }
    pub fn cancel(&mut self) {
        if self.terminal.is_none() {
            self.fail(CalculatedSaveError::Cancelled);
        }
    }
    pub(crate) fn finish(
        mut self,
        live: &mut HostState,
    ) -> Result<PreparedSave, CalculatedSaveError> {
        if let Some(calculation) = &mut self.calculation {
            let returns = calculation.retire_returns();
            if live.engine.workbook_instance() == self.pin.workbook {
                live.async_records
                    .absorb(returns)
                    .map_err(CalculatedSaveError::ReturnIdentityConflict)?;
            }
        }
        self.terminal
            .take()
            .unwrap_or(Err(CalculatedSaveError::Pending))
    }
}
impl Drop for CalculatingSave {
    fn drop(&mut self) {
        if let Some(calculation) = &mut self.calculation {
            calculation.cancel();
        }
    }
}
