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
    SelectionChanged {
        sheet: SheetId,
        target: Vec<GridArea>,
    },
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
    fn context_changed(&self, _session: &HostSession) {}
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
    fn selection_changed(
        &self,
        _session: &mut HostSession,
        _sheet: SheetId,
        _target: &[GridArea],
    ) -> Result<(), String> {
        Ok(())
    }
    fn before_save(
        &self,
        session: &mut HostSession,
        operation: SaveOperation,
    ) -> Result<BeforeSaveOutcome, String>;
}

/// A projection observes the same committed events after the owning VBA handler.
/// Callbacks borrow the session only for their duration; they must not retain its address.
pub trait WorkbookEventObserver {
    fn context_changed(&self, _session: &HostSession) {}
    fn workbook_open(&self, _session: &mut HostSession) -> Result<(), String> {
        Ok(())
    }
    fn selection_changed(
        &self,
        _session: &mut HostSession,
        _sheet: SheetId,
        _target: &[GridArea],
    ) -> Result<(), String> {
        Ok(())
    }
    fn before_save(
        &self,
        _session: &mut HostSession,
        _operation: SaveOperation,
        _cancel: &mut bool,
    ) -> Result<(), String> {
        Ok(())
    }
    fn worksheet_change(
        &self,
        _session: &mut HostSession,
        _sheet: SheetId,
        _target: &[GridArea],
    ) -> Result<(), String> {
        Ok(())
    }
    /// A sheet that calculated in a completed calculation command, after the owning VBA
    /// handler and before `after_calculate` (Excel's AppEvents SheetCalculate).
    fn worksheet_calculate(
        &self,
        _session: &mut HostSession,
        _sheet: SheetId,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkbookEventObserverId(u64);

pub(crate) struct WorkbookEventState {
    owner: Option<Rc<dyn WorkbookEventHandler>>,
    observers: BTreeMap<WorkbookEventObserverId, Rc<dyn WorkbookEventObserver>>,
    application_observer: Option<WorkbookEventObserverId>,
    next_observer: u64,
    enabled: Rc<Cell<bool>>,
    next: u64,
    failures: VecDeque<WorkbookEventFailure>,
    omitted: u64,
    owner_open_delivered: bool,
    observer_open_delivered: bool,
    calculation: Option<(crate::CalcCommandId, BTreeSet<SheetId>)>,
}
impl Default for WorkbookEventState {
    fn default() -> Self {
        Self {
            owner: None,
            observers: BTreeMap::new(),
            application_observer: None,
            next_observer: 1,
            enabled: Rc::new(Cell::new(true)),
            next: 1,
            failures: VecDeque::new(),
            omitted: 0,
            owner_open_delivered: false,
            observer_open_delivered: false,
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
    /// The application announces an open after project admission. A project admitted later
    /// can receive its own Open without replaying the already delivered projection event.
    pub fn notify_workbook_open(&mut self) {
        let notify_owner = self.workbook_events.owner.is_some()
            && !std::mem::replace(&mut self.workbook_events.owner_open_delivered, true);
        let notify_observers =
            !std::mem::replace(&mut self.workbook_events.observer_open_delivered, true);
        if notify_owner
            && let Some(owner) = self.event_owner()
            && let Err(reason) = owner.workbook_open(self)
        {
            self.record_event_failure(WorkbookEvent::Open, reason);
        }
        if notify_observers {
            let _ = self.notify_observers(WorkbookEvent::Open, |observer, session| {
                observer.workbook_open(session)
            });
        }
    }
    pub fn set_workbook_event_handler(&mut self, owner: Option<Rc<dyn WorkbookEventHandler>>) {
        self.workbook_events.owner = owner;
        self.notify_workbook_context();
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
    /// Replace the application observer, preserving independently registered observers.
    pub fn set_workbook_event_observer(&mut self, observer: Option<Rc<dyn WorkbookEventObserver>>) {
        if let Some(previous) = self.workbook_events.application_observer.take() {
            self.remove_workbook_event_observer(previous);
        }
        self.workbook_events.application_observer =
            observer.map(|observer| self.add_workbook_event_observer(observer));
    }
    pub fn add_workbook_event_observer(
        &mut self,
        observer: Rc<dyn WorkbookEventObserver>,
    ) -> WorkbookEventObserverId {
        let id = WorkbookEventObserverId(self.workbook_events.next_observer);
        self.workbook_events.next_observer =
            id.0.checked_add(1)
                .expect("workbook event observer IDs exhausted");
        observer.context_changed(self);
        self.workbook_events.observers.insert(id, observer);
        id
    }
    pub fn remove_workbook_event_observer(&mut self, id: WorkbookEventObserverId) -> bool {
        self.workbook_events.observers.remove(&id).is_some()
    }
    pub(crate) fn notify_workbook_context(&self) {
        if let Some(owner) = &self.workbook_events.owner {
            owner.context_changed(self);
        }
        for observer in self.workbook_events.observers.values() {
            observer.context_changed(self);
        }
    }

    fn notify_observers(
        &mut self,
        event: WorkbookEvent,
        mut deliver: impl FnMut(&dyn WorkbookEventObserver, &mut HostSession) -> Result<(), String>,
    ) -> Result<(), String> {
        if !self.workbook_events_enabled() {
            return Ok(());
        }
        // Callbacks can remove observers, start nested commands, or disable events.
        let recipients: Vec<_> = self.workbook_events.observers.keys().copied().collect();
        let mut first_failure = None;
        for id in recipients {
            if !self.workbook_events_enabled() {
                break;
            }
            if let Some(observer) = self.workbook_events.observers.get(&id).cloned()
                && let Err(reason) = deliver(&*observer, self)
            {
                self.record_event_failure(event.clone(), reason.clone());
                first_failure.get_or_insert(reason);
            }
        }
        first_failure.map_or(Ok(()), Err)
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
            if self.host.model().sheet_by_id(sheet).is_some() {
                let _ = self.notify_observers(
                    WorkbookEvent::WorksheetChange {
                        sheet,
                        target: target.clone(),
                    },
                    |observer, session| observer.worksheet_change(session, sheet, &target),
                );
            }
        }
    }
    pub(crate) fn notify_workbook_selection_changed(
        &mut self,
        sheet: SheetId,
        target: &[GridArea],
    ) {
        if let Some(owner) = self.event_owner()
            && let Err(reason) = owner.selection_changed(self, sheet, target)
        {
            self.record_event_failure(
                WorkbookEvent::SelectionChanged {
                    sheet,
                    target: target.to_vec(),
                },
                reason,
            );
        }
        if self.host.model().sheet_by_id(sheet).is_some() {
            let _ = self.notify_observers(
                WorkbookEvent::SelectionChanged {
                    sheet,
                    target: target.to_vec(),
                },
                |observer, session| observer.selection_changed(session, sheet, target),
            );
        }
    }
    pub(crate) fn refuse_automation_change(&mut self, receipt: &CommandReceipt) {
        if !self.workbook_events_enabled()
            || (self.workbook_events.owner.is_none() && self.workbook_events.observers.is_empty())
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
            if let Some(owner) = &owner
                && let Err(reason) = owner.worksheet_calculate(self, sheet)
            {
                self.record_event_failure(WorkbookEvent::WorksheetCalculate { sheet }, reason);
            }
            // The native projection's SheetCalculate follows the owning VBA handler's, as its
            // SheetChange does.
            if self.host.model().sheet_by_id(sheet).is_some() {
                let _ = self.notify_observers(
                    WorkbookEvent::WorksheetCalculate { sheet },
                    |observer, session| observer.worksheet_calculate(session, sheet),
                );
            }
        }
        // AfterCalculate follows a calculation that ran: one that calculated a formula or that was
        // asked for. A completed pass nobody asked for that calculated nothing (a tick with
        // nothing dirty) is not one (dc-1nbp.3.1).
        let demanded = std::mem::take(&mut self.host.calculation_demanded);
        if delta.completion().is_some_and(|receipt| {
            receipt.outcome == crate::CompletionOutcome::Complete
                && (receipt.coverage.calculated > 0 || demanded)
        }) {
            let _ = self.notify_observers(WorkbookEvent::AfterCalculate, |observer, session| {
                observer.after_calculate(session)
            });
        }
    }
    pub(crate) fn notify_observers_before_save(
        &mut self,
        operation: SaveOperation,
        cancel: &mut bool,
    ) -> Result<(), String> {
        self.notify_observers(
            WorkbookEvent::BeforeSave { operation },
            |observer, session| observer.before_save(session, operation, cancel),
        )
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
