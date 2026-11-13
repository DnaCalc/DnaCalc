//! Workbook event positions. The runtime owns handler selection and project state;
//! the host supplies committed authored inputs and completed calculation views.
use crate::{BeforeSaveOutcome, CommandReceipt, HostProgress, HostSession, SaveOperation};
use calc_value::{GridArea, SheetId};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkbookEvent {
    Open,
    WorksheetChange {
        sheet: SheetId,
        target: Vec<GridArea>,
    },
    AfterCalculate,
    WorksheetCalculate {
        sheet: SheetId,
    },
    BeforeSave {
        operation: SaveOperation,
    },
}

/// One registered owner. It supplies runtime-wired document sources, not handler names.
/// The session lends exclusive authority during each synchronous callback. A nested
/// command completes its transaction, calculation and events before the caller resumes.
/// BeforeSave may also return an authored batch for the existing pre-pin route.
pub trait WorkbookEventHandler {
    fn file_identity_changed(&self, _name: &str) {}
    fn workbook_open(&self, _session: &mut HostSession) -> Result<(), String> {
        Ok(())
    }
    fn worksheet_change(
        &self,
        session: &mut HostSession,
        sheet: SheetId,
        target: &[GridArea],
    ) -> Result<(), String>;
    fn worksheet_calculate(&self, session: &mut HostSession, sheet: SheetId) -> Result<(), String>;
    fn before_save(
        &self,
        session: &mut HostSession,
        operation: SaveOperation,
    ) -> Result<BeforeSaveOutcome, String>;
}

/// A native projection observes the same committed events after the owning VBA handler.
/// Callbacks borrow the session only for their duration; they must not retain its address.
pub trait WorkbookEventObserver {
    fn context_changed(&self, _session: &HostSession) {}
    fn worksheet_change(
        &self,
        _session: &mut HostSession,
        _sheet: SheetId,
        _target: &[GridArea],
    ) -> Result<(), String> {
        Ok(())
    }
    fn after_calculate(&self, _session: &mut HostSession) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkbookEventFailure {
    pub sequence: u64,
    pub revision: calc_doc_model::AuthoredRevision,
    pub event: WorkbookEvent,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct WorkbookEventFailures {
    pub failures: Vec<WorkbookEventFailure>,
    /// Older undrained failures omitted from the bounded queue, never counted as successes.
    pub omitted: u64,
}

pub(crate) struct WorkbookEventState {
    owner: Option<Rc<dyn WorkbookEventHandler>>,
    observer: Option<Rc<dyn WorkbookEventObserver>>,
    enabled: Rc<Cell<bool>>,
    next: u64,
    failures: VecDeque<WorkbookEventFailure>,
    omitted: u64,
    open_delivered: bool,
    calculation: Option<(crate::CalcCommandId, BTreeSet<SheetId>)>,
}
impl Default for WorkbookEventState {
    fn default() -> Self {
        Self {
            owner: None,
            observer: None,
            enabled: Rc::new(Cell::new(true)),
            next: 1,
            failures: VecDeque::new(),
            omitted: 0,
            open_delivered: false,
            calculation: None,
        }
    }
}
impl HostSession {
    pub(crate) fn notify_file_identity(&self, name: &str) {
        // Identity is state, so it must also update while event delivery is disabled.
        if let Some(owner) = &self.workbook_events.owner {
            owner.file_identity_changed(name);
        }
    }
    /// The application calls this once after admitting a file's owning project,
    /// before its first automatic calculation. Source replacement is not an open.
    pub fn notify_workbook_open(&mut self) {
        if std::mem::replace(&mut self.workbook_events.open_delivered, true) {
            return;
        }
        if let Some(owner) = self.event_owner()
            && let Err(reason) = owner.workbook_open(self)
        {
            self.record_event_failure(WorkbookEvent::Open, reason);
        }
    }
    pub fn set_workbook_event_handler(&mut self, owner: Option<Rc<dyn WorkbookEventHandler>>) {
        self.workbook_events.owner = owner;
    }
    pub fn set_workbook_events_enabled(&mut self, enabled: bool) {
        self.workbook_events.enabled.set(enabled);
    }
    pub fn workbook_events_enabled(&self) -> bool {
        self.workbook_events.enabled.get()
    }
    /// The projection and VBA use this same switch, including during synchronous callbacks.
    pub fn workbook_events_switch(&self) -> Rc<Cell<bool>> {
        self.workbook_events.enabled.clone()
    }
    pub fn set_workbook_event_observer(&mut self, observer: Option<Rc<dyn WorkbookEventObserver>>) {
        self.workbook_events.observer = observer;
        self.notify_workbook_context();
    }
    pub(crate) fn notify_workbook_context(&self) {
        if let Some(observer) = &self.workbook_events.observer {
            observer.context_changed(self);
        }
    }
    /// Inspect the bounded event report without editing or invalidating the workbook.
    pub fn workbook_event_failures(&self) -> WorkbookEventFailures {
        WorkbookEventFailures {
            failures: self.workbook_events.failures.iter().cloned().collect(),
            omitted: self.workbook_events.omitted,
        }
    }
    pub fn take_workbook_event_failures(&mut self) -> WorkbookEventFailures {
        WorkbookEventFailures {
            failures: self.workbook_events.failures.drain(..).collect(),
            omitted: std::mem::take(&mut self.workbook_events.omitted),
        }
    }
    fn event_owner(&self) -> Option<Rc<dyn WorkbookEventHandler>> {
        self.workbook_events
            .enabled
            .get()
            .then(|| self.workbook_events.owner.clone())
            .flatten()
    }
    fn event_owner_still_active(&self, owner: &Rc<dyn WorkbookEventHandler>) -> bool {
        self.workbook_events.enabled.get()
            && self
                .workbook_events
                .owner
                .as_ref()
                .is_some_and(|current| Rc::ptr_eq(current, owner))
    }
    fn record_event_failure(&mut self, event: WorkbookEvent, reason: String) {
        let state = &mut self.workbook_events;
        let sequence = state.next;
        state.next = state.next.checked_add(1).expect("event sequence exhausted");
        if state.failures.len() == 128 {
            state.failures.pop_front();
            state.omitted = state.omitted.saturating_add(1);
        }
        state.failures.push_back(WorkbookEventFailure {
            sequence,
            revision: self.host.revision(),
            event,
            reason,
        });
    }
    pub(crate) fn notify_workbook_change(&mut self, receipt: &CommandReceipt) {
        if !self.workbook_events_enabled() {
            return;
        }
        let owner = self.event_owner();
        let mut targets: BTreeMap<SheetId, Vec<GridArea>> = BTreeMap::new();
        for area in receipt.input_writes().iter().copied() {
            targets.entry(area.sheet()).or_default().push(area);
        }
        for (sheet, target) in targets {
            if !self.workbook_events_enabled()
                || owner
                    .as_ref()
                    .is_some_and(|owner| !self.event_owner_still_active(owner))
            {
                break;
            }
            if self.host.model().sheet_by_id(sheet).is_none() {
                continue;
            }
            let target = crate::copy_metadata::merge(target);
            if let Some(owner) = &owner {
                if let Err(reason) = owner.worksheet_change(self, sheet, &target) {
                    self.record_event_failure(
                        WorkbookEvent::WorksheetChange {
                            sheet,
                            target: target.clone(),
                        },
                        reason,
                    );
                }
            }
            // Nested handler writes finish their own VBA and native events before this target.
            if self.workbook_events_enabled() && self.host.model().sheet_by_id(sheet).is_some() {
                if let Some(observer) = self.workbook_events.observer.clone() {
                    if let Err(reason) = observer.worksheet_change(self, sheet, &target) {
                        self.record_event_failure(
                            WorkbookEvent::WorksheetChange { sheet, target },
                            reason,
                        );
                    }
                }
            }
        }
    }
    pub(crate) fn refuse_automation_change(&mut self, receipt: &CommandReceipt) {
        if !self.workbook_events_enabled()
            || (self.workbook_events.owner.is_none() && self.workbook_events.observer.is_none())
        {
            return;
        }
        let mut targets: BTreeMap<SheetId, Vec<GridArea>> = BTreeMap::new();
        for area in receipt.input_writes().iter().copied() {
            targets.entry(area.sheet()).or_default().push(area);
        }
        for (sheet, target) in targets {
            let target = crate::copy_metadata::merge(target);
            self.record_event_failure(
                WorkbookEvent::WorksheetChange { sheet, target },
                "Automation Change before terminal successful calculation is not modelled (dc-hsa5.12)".into(),
            );
        }
    }
    pub(crate) fn notify_workbook_calculation(&mut self, progress: &HostProgress) {
        let HostProgress::Published(delta) = progress else {
            if matches!(progress, HostProgress::Failed(_) | HostProgress::Cancelled) {
                self.workbook_events.calculation = None;
            }
            return;
        };
        let command = delta.publication().expect("published unit").command;
        let state = &mut self.workbook_events;
        if state
            .calculation
            .as_ref()
            .is_none_or(|(previous, _)| *previous != command)
        {
            state.calculation = Some((command, BTreeSet::new()));
        }
        let (_, sheets) = state.calculation.as_mut().expect("current command");
        sheets.extend(delta.changes().iter().filter_map(|change| match change {
            calc_engine::ValueChange::Published { at, .. } => Some(at.sheet()),
            calc_engine::ValueChange::Removed { .. } => None,
        }));
        if delta.completion().is_none() {
            return;
        }
        let (_, sheets) = state.calculation.take().expect("completed command");
        let owner = self.event_owner();
        for sheet in sheets {
            let Some(owner) = &owner else {
                break;
            };
            if !self.event_owner_still_active(owner) {
                break;
            }
            if self.host.model().sheet_by_id(sheet).is_none() {
                continue;
            }
            if let Err(reason) = owner.worksheet_calculate(self, sheet) {
                self.record_event_failure(WorkbookEvent::WorksheetCalculate { sheet }, reason);
            }
        }
        if delta
            .completion()
            .is_some_and(|receipt| receipt.outcome == crate::CompletionOutcome::Complete)
            && self.workbook_events_enabled()
        {
            if let Some(observer) = self.workbook_events.observer.clone() {
                if let Err(reason) = observer.after_calculate(self) {
                    self.record_event_failure(WorkbookEvent::AfterCalculate, reason);
                }
            }
        }
    }
    pub(crate) fn notify_workbook_before_save(
        &mut self,
        operation: SaveOperation,
    ) -> Result<BeforeSaveOutcome, String> {
        let Some(owner) = self.event_owner() else {
            return Ok(BeforeSaveOutcome::default());
        };
        let outcome = owner.before_save(self, operation);
        if let Err(reason) = &outcome {
            self.record_event_failure(WorkbookEvent::BeforeSave { operation }, reason.clone());
        }
        outcome
    }
}
