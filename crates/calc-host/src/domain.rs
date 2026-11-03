//! Owner coordination of connected workbook engines. Files and provider lifetimes belong
//! to the caller; this portable owner admits calculation and exchanges immutable input cuts.
use crate::{
    HostProgress, HostSession, HostState, ReadObserver, RecalcFailed, Unobserved, WorkBudget,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DomainWorkbookId(u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomainSaveId(u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomainError {
    Busy,
    UnknownWorkbook,
    UnknownLink,
    /// Cross-workbook circular/iterative sessions are not implemented by this owner.
    LinkedCycle {
        owner: &'static str,
    },
}
impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for DomainError {}

#[derive(Debug)]
pub enum DomainProgress {
    Advanced {
        workbook: DomainWorkbookId,
        progress: HostProgress,
    },
    Complete,
    Waiting {
        workbook: DomainWorkbookId,
    },
    Failed {
        workbook: DomainWorkbookId,
        error: RecalcFailed,
    },
}

struct DomainWork {
    order: Vec<DomainWorkbookId>,
    cursor: usize,
    started: bool,
    full: bool,
    scopes: BTreeMap<DomainWorkbookId, crate::CalcScope>,
    remainder: Option<crate::CalcScope>,
    pins: BTreeMap<DomainWorkbookId, std::sync::Arc<crate::LinkedWorkbookSnapshot>>,
}

struct DomainSave {
    id: DomainSaveId,
    workbook: DomainWorkbookId,
    demand: crate::save::SaveDemand,
    policy: crate::SavePolicy,
    terminal: Option<Result<crate::PreparedSave, crate::SaveFailed>>,
}

#[derive(Default)]
pub struct CalculationDomain {
    workbooks: BTreeMap<DomainWorkbookId, HostSession>,
    links: BTreeMap<(DomainWorkbookId, u32), DomainWorkbookId>,
    work: Option<DomainWork>,
    save: Option<DomainSave>,
}
impl CalculationDomain {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, host: HostState) -> Result<DomainWorkbookId, DomainError> {
        self.insert_session(HostSession::new(host))
    }
    pub fn insert_session(
        &mut self,
        session: HostSession,
    ) -> Result<DomainWorkbookId, DomainError> {
        if self.work.is_some() {
            return Err(DomainError::Busy);
        }
        let id = DomainWorkbookId(crate::state::next_command_scope());
        self.workbooks.insert(id, session);
        Ok(id)
    }
    pub fn host(&self, id: DomainWorkbookId) -> Option<&HostState> {
        self.session(id).map(HostSession::host)
    }
    pub fn session(&self, id: DomainWorkbookId) -> Option<&HostSession> {
        self.workbooks.get(&id)
    }
    /// Before allowing an edit, invalidate destination contexts. No result from the old
    /// cut can remain current merely because a source later calculates an equal value.
    pub fn host_mut(&mut self, id: DomainWorkbookId) -> Result<&mut HostState, DomainError> {
        Ok(&mut self.session_mut(id)?.host)
    }
    pub fn session_mut(&mut self, id: DomainWorkbookId) -> Result<&mut HostSession, DomainError> {
        if self.work.is_some() {
            return Err(DomainError::Busy);
        }
        if !self.workbooks.contains_key(&id) {
            return Err(DomainError::UnknownWorkbook);
        }
        self.invalidate_consumers(id);
        Ok(self.workbooks.get_mut(&id).expect("checked workbook"))
    }
    fn invalidate_consumers(&mut self, id: DomainWorkbookId) {
        let mut changed = BTreeSet::from([id]);
        loop {
            let before = changed.len();
            for (&(destination, number), source) in &self.links {
                if changed.contains(source) {
                    let _ = self
                        .workbooks
                        .get_mut(&destination)
                        .expect("connected workbook")
                        .host
                        .set_linked_workbook(number, None);
                    changed.insert(destination);
                }
            }
            if changed.len() == before {
                break;
            }
        }
    }
    pub fn connect(
        &mut self,
        destination: DomainWorkbookId,
        number: u32,
        source: DomainWorkbookId,
    ) -> Result<(), DomainError> {
        if self.work.is_some() {
            return Err(DomainError::Busy);
        }
        if !self.workbooks.contains_key(&source) {
            return Err(DomainError::UnknownWorkbook);
        }
        let host = self
            .workbooks
            .get_mut(&destination)
            .ok_or(DomainError::UnknownWorkbook)?;
        host.host
            .set_linked_workbook(number, None)
            .map_err(|_| DomainError::UnknownLink)?;
        self.links.insert((destination, number), source);
        self.invalidate_consumers(destination);
        Ok(())
    }
    pub fn disconnect(
        &mut self,
        destination: DomainWorkbookId,
        number: u32,
    ) -> Result<(), DomainError> {
        if self.work.is_some() {
            return Err(DomainError::Busy);
        }
        let host = self
            .workbooks
            .get_mut(&destination)
            .ok_or(DomainError::UnknownWorkbook)?;
        host.host
            .set_linked_workbook(number, None)
            .map_err(|_| DomainError::UnknownLink)?;
        self.links.remove(&(destination, number));
        self.invalidate_consumers(destination);
        Ok(())
    }
    pub fn workbook_ids(&self) -> impl Iterator<Item = DomainWorkbookId> + '_ {
        self.workbooks.keys().copied()
    }
    pub fn request_calculation(
        &mut self,
        targets: &[DomainWorkbookId],
        full: bool,
    ) -> Result<(), DomainError> {
        if self.work.is_some() {
            return Err(DomainError::Busy);
        }
        fn visit(
            domain: &CalculationDomain,
            id: DomainWorkbookId,
            visiting: &mut BTreeSet<DomainWorkbookId>,
            done: &mut BTreeSet<DomainWorkbookId>,
            order: &mut Vec<DomainWorkbookId>,
        ) -> Result<(), DomainError> {
            if !domain.workbooks.contains_key(&id) {
                return Err(DomainError::UnknownWorkbook);
            }
            if done.contains(&id) {
                return Ok(());
            }
            if !visiting.insert(id) {
                return Err(DomainError::LinkedCycle { owner: "dc-ylr" });
            }
            for (&(destination, _), &source) in &domain.links {
                if destination == id {
                    visit(domain, source, visiting, done, order)?;
                }
            }
            visiting.remove(&id);
            done.insert(id);
            order.push(id);
            Ok(())
        }
        let mut order = Vec::new();
        let (mut visiting, mut done) = (BTreeSet::new(), BTreeSet::new());
        for &id in targets {
            visit(self, id, &mut visiting, &mut done, &mut order)?;
        }
        self.work = Some(DomainWork {
            order,
            cursor: 0,
            started: false,
            full,
            scopes: BTreeMap::new(),
            remainder: None,
            pins: BTreeMap::new(),
        });
        Ok(())
    }
    /// Rebuild the selected domain workbooks and their linked sources, then Full.
    /// Validate the whole request before discarding any derived bindings.
    pub fn request_rebuild(&mut self, targets: &[DomainWorkbookId]) -> Result<(), DomainError> {
        if self.work.is_some() || self.workbooks.values().any(|session| session.host.work.is_some()) {
            return Err(DomainError::Busy);
        }
        self.request_calculation(targets, true)?;
        for id in &self.work.as_ref().expect("admitted rebuild").order {
            self.workbooks.get_mut(id).expect("admitted workbook").host.engine.rebuild_bindings();
        }
        Ok(())
    }
    pub fn advance(&mut self, budget: WorkBudget) -> DomainProgress {
        self.advance_observed(budget, None, &mut Unobserved)
    }
    pub fn advance_observed(
        &mut self,
        budget: WorkBudget,
        observed: Option<DomainWorkbookId>,
        observer: &mut dyn ReadObserver,
    ) -> DomainProgress {
        let Some(work) = self.work.as_mut() else {
            return DomainProgress::Complete;
        };
        let Some(&id) = work.order.get(work.cursor) else {
            self.work = None;
            self.prepare_domain_image();
            return DomainProgress::Complete;
        };
        if !work.started {
            let pins: Vec<_> = self
                .links
                .iter()
                .filter(|((destination, _), _)| *destination == id)
                .map(|((_, number), source)| (*number, work.pins[source].clone()))
                .collect();
            let host = &mut self.workbooks.get_mut(&id).expect("admitted workbook").host;
            for (number, pin) in pins {
                host.set_linked_workbook(number, Some(pin))
                    .expect("connected link");
            }
            // A save may wait for an existing invocation, but cannot discharge
            // it by driving a successor after refreshing one of its source cuts.
            if let Some(save) = &mut self.save {
                if host.work.as_ref().is_some_and(|active|
                    !active.live_inputs_current(&host.engine) || !active.callables_current(&host.engine)) {
                    save.terminal = Some(Err(crate::save::operation_failed("The source cut of an existing domain invocation changed")));
                    self.work = None;
                    return DomainProgress::Failed { workbook:id, error:RecalcFailed::Failed(crate::RecalcFailure::ExternalInputMismatch) };
                }
            }
            if work.full {
                crate::calculate_full(host);
            } else {
                let scope = work
                    .scopes
                    .get(&id)
                    .cloned()
                    .unwrap_or(crate::CalcScope::Workbook);
                if let Some(active) = &host.work {
                    if host.engine.nodes().any(|n| {
                        scope.contains(n.anchor()) && !active.admission().scope.contains(n.anchor())
                    }) || active.admission().range_post_state
                        == crate::RangePostState::StillRequired
                    {
                        work.remainder = Some(scope);
                    }
                } else {
                    host.requested_calculation =
                        Some((scope, crate::EvalSelection::DirtyAndVolatile));
                    crate::calculate(host);
                }
            }
            work.started = true;
        }
        let session = self.workbooks.get_mut(&id).expect("admitted workbook");
        let progress = if observed == Some(id) {
            session.advance_observed(budget, observer).0
        } else {
            session.advance_observed(budget, &mut Unobserved).0
        };
        let host = &session.host;
        match progress {
            HostProgress::Waiting { .. } => DomainProgress::Waiting { workbook: id },
            HostProgress::Failed(error) => {
                if let Some(save) = &mut self.save {
                    save.terminal = Some(Err(crate::save::operation_failed(error.to_string())));
                }
                self.work = None;
                DomainProgress::Failed {
                    workbook: id,
                    error,
                }
            }
            HostProgress::Cancelled => {
                if let Some(save) = &mut self.save {
                    save.terminal = Some(Err(crate::SaveFailed::Cancelled));
                }
                self.work = None;
                DomainProgress::Advanced {
                    workbook: id,
                    progress,
                }
            }
            _ => {
                if host.work.is_none() {
                    if let Some(save) = &mut self.save {
                        if host
                            .last_calculation_completion
                            .as_ref()
                            .is_none_or(|receipt| {
                                receipt.admitted.model != host.revision()
                                    || receipt.results != host.engine.result_view()
                                    || !crate::save::save_completion_valid(receipt)
                            })
                        {
                            save.terminal = Some(Err(crate::save::operation_failed(
                                "A domain peer did not complete the save calculation",
                            )));
                            self.work = None;
                            return DomainProgress::Advanced {
                                workbook: id,
                                progress,
                            };
                        }
                    }
                    if let Some(scope) = work.remainder.take() {
                        let host =
                            &mut self.workbooks.get_mut(&id).expect("admitted workbook").host;
                        host.requested_calculation =
                            Some((scope, crate::EvalSelection::DirtyAndVolatile));
                        crate::calculate(host);
                        return DomainProgress::Advanced {
                            workbook: id,
                            progress,
                        };
                    }
                    if self.links.values().any(|source| *source == id) {
                        work.pins.insert(id, host.linked_snapshot());
                    }
                    work.cursor += 1;
                    work.started = false;
                }
                DomainProgress::Advanced {
                    workbook: id,
                    progress,
                }
            }
        }
    }

    /// P06: Save(true) admits dirty/volatile demand in every domain workbook,
    /// including unrelated peers. BeforeSave edits commit through the saved session.
    pub fn begin_save(
        &mut self,
        workbook: DomainWorkbookId,
        policy: &crate::SavePolicy,
        context: crate::ExcelSaveContext<'_>,
    ) -> Result<DomainSaveId, crate::SaveFailed> {
        if self.work.is_some() || self.save.is_some() {
            return Err(crate::save::operation_failed(
                "The domain already has pending work or a save image",
            ));
        }
        if !self.workbooks.contains_key(&workbook) {
            return Err(crate::save::operation_failed(
                "The saved workbook is unavailable",
            ));
        }
        let before = self.host(workbook).expect("checked workbook").revision();
        let normalized = self
            .workbooks
            .get_mut(&workbook)
            .expect("checked workbook")
            .normalize_save_context(context);
        if self.host(workbook).expect("checked workbook").revision() != before {
            self.invalidate_consumers(workbook);
        }
        let demand = normalized?;
        if let Some(scope) = &demand.scope {
            let targets: Vec<_> = if demand.calculate_before_save {
                self.workbook_ids().collect()
            } else {
                vec![workbook]
            };
            for &id in &targets {
                let required = if id == workbook {
                    scope
                } else {
                    &crate::CalcScope::Workbook
                };
                if !crate::save::save_can_wait_for(
                    self.host(id).expect("domain workbook"),
                    required,
                ) {
                    return Err(crate::save::operation_failed(
                        "An active domain calculation does not cover this save's requirements",
                    ));
                }
            }
            self.request_calculation(&targets, false)
                .map_err(|e| crate::save::operation_failed(e.to_string()))?;
            self.work
                .as_mut()
                .expect("admitted domain calculation")
                .scopes
                .insert(workbook, scope.clone());
        }
        let id = DomainSaveId(crate::state::next_command_scope());
        self.save = Some(DomainSave {
            id,
            workbook,
            demand,
            policy: policy.clone(),
            terminal: None,
        });
        if self.work.is_none() {
            self.prepare_domain_image();
        }
        Ok(id)
    }
    fn prepare_domain_image(&mut self) {
        let Some(save) = &mut self.save else {
            return;
        };
        if save.terminal.is_some() {
            return;
        }
        let host = &self.workbooks[&save.workbook].host;
        if let Some(workbook_scope) = &save.demand.scope {
            let valid = self.workbooks.iter().all(|(id, session)| {
                if *id != save.workbook && !save.demand.calculate_before_save {
                    return true;
                }
                let scope = if *id == save.workbook {
                    workbook_scope
                } else {
                    &crate::CalcScope::Workbook
                };
                crate::save::save_scope_satisfied(&session.host, scope)
            });
            if !valid {
                save.terminal = Some(Err(crate::save::operation_failed(
                    "The domain save still has unmet calculation requirements",
                )));
                return;
            }
        }
        let completion = save
            .demand
            .scope
            .as_ref()
            .and(host.last_calculation_completion.clone());
        save.terminal = Some(crate::save::prepare_save_operation(
            host,
            &save.policy,
            save.demand.caches,
            save.demand.calculate_before_save,
            completion,
        ));
    }
    pub fn take_save(
        &mut self,
        id: DomainSaveId,
    ) -> Result<crate::PreparedSave, crate::SaveFailed> {
        let Some(save) = self.save.as_ref().filter(|save| save.id == id) else {
            return Err(crate::save::operation_failed(
                "The save identity is unavailable",
            ));
        };
        if save.terminal.is_none() {
            return Err(crate::save::operation_failed(
                "The domain save is still pending",
            ));
        }
        self.save
            .take()
            .expect("checked save")
            .terminal
            .expect("checked terminal image")
    }
    pub fn cancel_save(&mut self, id: DomainSaveId) -> Result<(), DomainError> {
        let Some(save) = &mut self.save else {
            return Err(DomainError::UnknownWorkbook);
        };
        if save.id != id {
            return Err(DomainError::UnknownWorkbook);
        }
        if save.terminal.is_none() {
            if let Some(work) = self.work.take() {
                for id in work.order {
                    self.workbooks
                        .get_mut(&id)
                        .expect("admitted workbook")
                        .cancel_calculation();
                }
            }
            save.terminal = Some(Err(crate::SaveFailed::Cancelled));
        }
        Ok(())
    }
    pub fn complete_async(
        &mut self,
        id: DomainWorkbookId,
        token: &crate::AsyncToken,
        value: crate::value::CalcValue,
    ) -> Result<crate::AsyncDisposition, DomainError> {
        Ok(self
            .workbooks
            .get_mut(&id)
            .ok_or(DomainError::UnknownWorkbook)?
            .complete_async(token, value))
    }
}
