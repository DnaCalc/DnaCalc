//! Driving the engine's recalculation of the current revision. A recalculation is not part of an
//! edit's transaction (plan 3.3): a committed edit stays whatever the recalculation does.

use crate::state::HostState;
use calc_doc::CalcMode;
use calc_engine::{
    ReadObserver, RecalcFailure, RecalcProgress, StartError, ValueDelta, WorkBudget,
};
use std::fmt;

/// Why a recalculation stopped before completion. Committed edits and previously installed
/// publication units stay, and [`crate::project`] labels every calculated result or refusal the recalculation
/// would have replaced as stale ([`crate::StaleCause::RecalcFailed`] until an edit commits, then
/// [`crate::StaleCause::AwaitingRecalc`]) until one publishes. A failure does not make a file
/// cache stale: one stays a file cache while no edit since the workbook was opened has changed
/// an input or a formula.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecalcFailed {
    /// The engine refused to begin it ([`calc_engine::begin_recalc`]), such as for a circular
    /// reference among the formulas it would calculate.
    Refused(StartError),
    /// It began and failed before completing ([`calc_engine::step`] and
    /// [`calc_engine::finish`]), such as when the caller's read observer stopped it.
    Failed(RecalcFailure),
}

impl fmt::Display for RecalcFailed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecalcFailed::Refused(error) => write!(f, "the recalculation was refused: {error}"),
            RecalcFailed::Failed(failure) => write!(f, "the recalculation failed: {failure}"),
        }
    }
}

impl std::error::Error for RecalcFailed {}

/// Where the workbook's calculation stands after a [`step`].
#[derive(Clone, Debug, PartialEq)]
pub enum HostProgress {
    /// The authorized calculation was cancelled; automatic demand waits for a new edit or request.
    Cancelled,
    /// The published generation calculated the current revision and no formula is pending:
    /// nothing was done.
    Current,
    /// The workbook calculates manually ([`crate::CalcMode::Manual`]) and no recalculation was
    /// asked for ([`calculate`]): nothing was done, and the formulas an edit reached project as
    /// stale.
    Manual,
    /// A recalculation of the current revision is in progress with `remaining` formulas to
    /// evaluate; the next step continues it.
    Calculating { remaining: usize },
    /// Provider input is required; schedule no continuation task until a return or edit.
    Waiting { remaining: usize },
    /// A recalculation installed a publication unit: what changed. More work may remain.
    Published(ValueDelta),
    /// A recalculation failed. Another request or authored change permits a new attempt.
    Failed(RecalcFailed),
}

/// Recalculates the current revision, evaluating at most `budget` formulas in this call, under
/// the host's numeric mode, with `observer` shown every run each evaluation reads.
///
/// A step with no recalculation in progress begins one ([`calc_engine::begin_recalc`]) unless the
/// published generation is of the current revision and no formula is pending. It then evaluates
/// ([`calc_engine::step`]) and, once nothing remains or the recalculation failed, publishes or
/// collects the failure ([`calc_engine::finish`]). A product caller that observes nothing passes
/// [`calc_engine::Unobserved`]; an observer can only watch reads or stop the recalculation, which
/// then fails ([`calc_engine::RecalcFailure::ReadFailed`]).
pub fn step(
    host: &mut HostState,
    budget: WorkBudget,
    observer: &mut dyn ReadObserver,
) -> HostProgress {
    if host.pinned_calculations.get() != 0 {
        return HostProgress::Waiting {
            remaining: host.engine.pending().count(),
        };
    }
    if host.calculation_cancelled && !host.calculate_requested && host.work.is_none() {
        return HostProgress::Cancelled;
    }
    let force = std::mem::take(&mut host.full_calculate_requested);
    let mut work = match host.work.take() {
        Some(work)
            if work.revision() != host.model.revision()
                || force
                || !work.live_inputs_current(&host.engine)
                || !work.callables_current(&host.engine) =>
        {
            let reloaded = !work.callables_current(&host.engine);
            host.renew_workbook_facts();
            let transformed = host.transformed_calculation_scope.take();
            let selection = work.admission().selection;
            let started = if force {
                host.calculate_requested = false;
                calc_engine::supersede_recalc_scoped(
                    &mut host.engine,
                    &host.model,
                    work,
                    host.options.numeric.clone(),
                    calc_engine::CalcScope::Workbook,
                    calc_engine::EvalSelection::ForceAll,
                )
            } else if let Some(scope) = transformed {
                calc_engine::supersede_recalc_scoped(
                    &mut host.engine,
                    &host.model,
                    work,
                    host.options.numeric.clone(),
                    scope,
                    selection,
                )
            } else {
                calc_engine::supersede_recalc(
                    &mut host.engine,
                    &host.model,
                    work,
                    host.options.numeric.clone(),
                )
            };
            match started {
                Ok(work) => {
                    let work = host.attach_rtd_connector(
                        work.with_external_inputs(host.rtd_inputs.clone())
                            .with_functions(&host.functions)
                            .with_reference_tracker(host.references.clone()),
                    );
                    if reloaded {
                        work.with_reevaluation_reason(
                            calc_engine::contract::ReevaluationReason::CallableReload,
                        )
                    } else {
                        work.with_reevaluation_reason(if force {
                            calc_engine::contract::ReevaluationReason::NewCommandDemand
                        } else {
                            calc_engine::contract::ReevaluationReason::AuthoredOrInputChange
                        })
                    }
                }
                Err(error) => return failed(host, RecalcFailed::Refused(error)),
            }
        }
        Some(work) => work,
        None => {
            let current = host.model.revision();
            let requested = std::mem::take(&mut host.calculate_requested);
            // L3 (dc-m3u): automatic except tables leaves data tables until a recalculation is
            // asked for.
            let deferred = host.calc_mode() == CalcMode::AutomaticExceptTables && !requested;
            host.engine.set_tables_deferred(deferred);
            if !requested {
                if host.calc_mode() == CalcMode::Manual {
                    return HostProgress::Manual;
                }
                if host.published == Some(current) && !host.engine.needs_recalc(&host.model) {
                    return HostProgress::Current;
                }
            }
            // dc-uar: the facts CELL and INFO read, as of this revision.
            host.renew_workbook_facts();
            let started = if let Some((scope, selection)) = host.requested_calculation.take() {
                let scope = host.transformed_calculation_scope.take().unwrap_or(scope);
                let poststate = if host.calc_mode() == CalcMode::Manual
                    && matches!(scope, calc_engine::CalcScope::Range(_))
                    && selection == calc_engine::EvalSelection::ForceAll
                {
                    calc_engine::contract::RangePostState::StillRequired
                } else {
                    calc_engine::contract::RangePostState::Satisfied
                };
                calc_engine::begin_recalc_scoped_with_poststate(
                    &mut host.engine,
                    &host.model,
                    host.options.numeric.clone(),
                    scope,
                    selection,
                    poststate,
                )
            } else {
                calc_engine::begin_recalc(
                    &mut host.engine,
                    &host.model,
                    host.options.numeric.clone(),
                )
            };
            match started {
                Ok(work) => host.attach_rtd_connector(
                    work.with_external_inputs(host.rtd_inputs.clone())
                        .with_functions(&host.functions)
                        .with_reference_tracker(host.references.clone()),
                ),
                Err(error) => return failed(host, RecalcFailed::Refused(error)),
            }
        }
    };
    if work.admission_pending()
        && work.admission().predecessor.is_none()
        && host.calc_mode() == CalcMode::Manual
        && matches!(work.admission().scope, calc_engine::CalcScope::Range(_))
        && work.admission().selection.forces_all()
    {
        work = work.with_manual_range_poststate();
    }
    if work.admission_pending()
        && let Some(source) = host.calculation_source.take()
    {
        work = work.with_source(source);
    }
    work.enable_publication_fences();
    work.set_interest(&host.engine, &host.model, &host.calculation_interest);
    host.record_calc_events(work.take_events());
    // Interest can promote candidates already computed by age escapes. Publish those before
    // spending another quantum on an unrelated suspended activation.
    if let Some(delta) = calc_engine::publish_ready(&mut host.engine, &host.model, &mut work) {
        host.record_calc_events(delta.events().to_vec());
        host.work = Some(work);
        return HostProgress::Published(delta);
    }
    let progress = calc_engine::step(&host.engine, &host.model, &mut work, budget, observer);
    // An RTD connection made during this step returned a value ConnectData supplied. Admit it
    // as its sequence before anything is published, so the result that showed it is certified.
    let admitted = host.admit_connected_rtd();
    match progress {
        RecalcProgress::Complete if admitted => {
            // The admission moved the inputs under this work: the next step supersedes it,
            // keeping every staged result whose certificate still holds.
            host.work = Some(work);
            HostProgress::Calculating {
                remaining: host.engine.pending().count().max(1),
            }
        }
        RecalcProgress::Pending { remaining } | RecalcProgress::Waiting { remaining } => {
            let waiting = work.is_waiting_external();
            host.record_calc_events(work.take_events());
            let published = calc_engine::publish_ready(&mut host.engine, &host.model, &mut work);
            host.work = Some(work);
            if let Some(delta) = published {
                host.record_calc_events(delta.events().to_vec());
                HostProgress::Published(delta)
            } else if waiting {
                HostProgress::Waiting { remaining }
            } else {
                HostProgress::Calculating { remaining }
            }
        }
        RecalcProgress::Complete | RecalcProgress::Failed => {
            let failure_receipt = work.completion(
                calc_engine::contract::CompletionOutcome::Failed,
                host.engine.result_view(),
                None,
            );
            match calc_engine::finish(&mut host.engine, &host.model, work) {
                Ok(delta) => {
                    host.record_calc_events(delta.events().to_vec());
                    host.published = Some(delta.revision());
                    host.last_failure = None;
                    HostProgress::Published(delta)
                }
                Err(failure) => {
                    host.record_calc_events(vec![calc_engine::CalcEvent::Completed(
                        failure_receipt,
                    )]);
                    failed(host, RecalcFailed::Failed(failure))
                }
            }
        }
    }
}

/// Asks for a recalculation, as Excel's Calculate Now (F9) does: the next [`step`] begins one
/// whatever the calculation mode, and even when nothing is pending, so that it calculates the
/// pending formulas, the formulas that call a volatile function and every formula that reads
/// them. A recalculation already in progress continues; the one asked for begins after it.
pub fn calculate(host: &mut HostState) {
    host.calculation_cancelled = false;
    host.calculate_requested = true;
}

/// Asks for a full recalculation, as Excel's `CalculateFull` (Ctrl+Alt+F9) does: every formula
/// is made pending and the next [`step`] begins a recalculation that calculates every formula,
/// whatever the calculation mode. The compiled plans and the dependency graph are kept, so
/// nothing is compiled again. A recalculation in progress is cancelled, since it would clear the
/// nodes it calculates from pending when it published; the full one replaces it.
pub fn calculate_full(host: &mut HostState) {
    host.calculation_cancelled = false;
    host.full_calculate_requested = true;
    host.engine.mark_all_pending();
    host.calculate_requested = true;
}

fn failed(host: &mut HostState, failure: RecalcFailed) -> HostProgress {
    // A failed command keeps its terminal receipt. New work requires an edit or a request.
    host.last_failure = Some((host.model.revision(), failure.clone()));
    HostProgress::Failed(failure)
}

impl HostState {
    pub(crate) fn record_calc_events(&mut self, events: Vec<calc_engine::CalcEvent>) {
        self.async_records.observe(&events);
        for event in &events {
            if let calc_engine::CalcEvent::Completed(receipt) = event {
                self.last_calculation_completion = Some(receipt.clone());
            }
        }
        self.calc_events.extend(events);
    }
    pub fn pending_async(&self) -> Vec<calc_engine::contract::AsyncToken> {
        self.work
            .as_ref()
            .map(|work| work.pending_async().cloned().collect())
            .unwrap_or_default()
    }
    /// The provider's adapter resolves its opaque handle to the complete stored token.
    /// A rejected return changes no model, cache or activation, and admits no calculation.
    pub fn complete_async(
        &mut self,
        token: &calc_engine::contract::AsyncToken,
        value: calc_doc_model::value::CalcValue,
    ) -> calc_engine::contract::AsyncDisposition {
        let disposition =
            self.async_records
                .deliver(&self.engine, &self.model, self.work.as_mut(), token, value);
        self.calc_events
            .push(calc_engine::CalcEvent::AsyncReturned {
                token: token.id,
                disposition,
            });
        disposition
    }
}

/// Provisional RTD values the connector returned during evaluation, awaiting admission.
pub(crate) type ConnectedQueue = std::rc::Rc<
    std::cell::RefCell<
        Vec<(
            calc_fml::TopicKey,
            calc_fml::RtdLease,
            u64,
            calc_doc_model::value::CalcValue,
        )>,
    >,
>;
/// The owner's connector with a record of every provisional value it returned. CalcHost admits
/// those values (as their sequence) before it publishes the results that showed them.
struct RecordingConnector {
    inner: std::rc::Rc<dyn calc_fml::RtdConnector>,
    queue: ConnectedQueue,
}
impl std::fmt::Debug for RecordingConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordingConnector").finish_non_exhaustive()
    }
}
impl calc_fml::RtdConnector for RecordingConnector {
    fn connect(
        &self,
        topic: &calc_fml::TopicKey,
        caller: calc_doc_model::value::CellKey,
        declared: bool,
    ) -> Result<calc_fml::RtdConnection, String> {
        let connection = self.inner.connect(topic, caller, declared)?;
        if let Some((sequence, value)) = &connection.provisional {
            self.queue.borrow_mut().push((
                topic.clone(),
                connection.lease,
                *sequence,
                value.clone(),
            ));
        }
        Ok(connection)
    }
}
impl HostState {
    /// The RTD values admitted since the last call, in admission order: those the owner
    /// delivered and those the host admitted for a connection made during evaluation.
    pub fn take_rtd_admissions(&mut self) -> Vec<RtdAdmission> {
        std::mem::take(&mut self.rtd_admissions)
    }
    /// The owner's capability to join or start RTD topics while this host evaluates.
    /// It also registers the built-in `RTD`, which reads through the same connector.
    pub fn set_rtd_connector(&mut self, connector: std::rc::Rc<dyn calc_fml::RtdConnector>) {
        self.rtd_connector = Some(connector);
        let functions = self.functions.clone();
        self.set_user_functions(functions);
    }
    pub(crate) fn attach_rtd_connector(
        &self,
        work: calc_engine::RecalcWork,
    ) -> calc_engine::RecalcWork {
        match &self.rtd_connector {
            Some(inner) => work.with_rtd_connector(std::rc::Rc::new(RecordingConnector {
                inner: inner.clone(),
                queue: self.rtd_connected.clone(),
            })),
            None => work,
        }
    }
    /// Admit each provisional value the connector returned since the last call, as its
    /// sequence. Returns whether any admission changed the input cut.
    pub(crate) fn admit_connected_rtd(&mut self) -> bool {
        let connected = std::mem::take(&mut *self.rtd_connected.borrow_mut());
        let mut changed = false;
        for (topic, lease, sequence, value) in connected {
            changed |= self.update_rtd(topic, lease, sequence, value) == RtdDisposition::Accepted;
        }
        changed
    }
}
pub use calc_fml::rtd::RtdDisposition;
/// One RTD value the host admitted: its topic, lease and sequence, and the value.
#[derive(Clone, Debug, PartialEq)]
pub struct RtdAdmission {
    pub topic: calc_fml::TopicKey,
    pub lease: calc_fml::RtdLease,
    pub sequence: u64,
    pub value: calc_doc_model::value::CalcValue,
}
impl HostState {
    /// RTD advances a topic's input. Manual mode waits for explicit demand; a current command is
    /// superseded at its next owner step, never completed by this event. The admission rules are
    /// `calc_fml::rtd::admit`; this wrapper calls the engine's per-topic invalidation.
    pub fn update_rtd(
        &mut self,
        topic: calc_fml::TopicKey,
        lease: calc_fml::RtdLease,
        sequence: u64,
        value: calc_doc_model::value::CalcValue,
    ) -> RtdDisposition {
        let admitted_value = value.clone();
        let (next, disposition) = calc_fml::rtd::admit(
            &self.rtd_inputs,
            calc_fml::rtd::RtdEvent::Update {
                topic: topic.clone(),
                lease,
                sequence,
                value,
            },
        );
        if disposition == RtdDisposition::Accepted {
            self.rtd_inputs = std::rc::Rc::new(next);
            self.engine
                .invalidate_rtd_topic(&self.model, &topic, lease, sequence);
            self.published = None;
            self.rtd_admissions.push(RtdAdmission {
                topic,
                lease,
                sequence,
                value: admitted_value,
            });
        }
        disposition
    }

    /// Leases are never recycled. Disconnect removes the current input of that incarnation and
    /// keeps a tombstone; old snapshots stay leased to old work.
    pub fn retire_rtd(
        &mut self,
        topic: calc_fml::TopicKey,
        lease: calc_fml::RtdLease,
    ) -> RtdDisposition {
        let (next, disposition) = calc_fml::rtd::admit(
            &self.rtd_inputs,
            calc_fml::rtd::RtdEvent::Retire {
                topic: topic.clone(),
                lease,
            },
        );
        if disposition == RtdDisposition::Accepted {
            self.rtd_inputs = std::rc::Rc::new(next);
            self.engine
                .invalidate_rtd_topic(&self.model, &topic, lease, 0);
            self.published = None;
        }
        disposition
    }
}
