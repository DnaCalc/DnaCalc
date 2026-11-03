//! Workbook event positions. The runtime owns handler selection and project state;
//! the host supplies committed authored inputs and completed calculation views.
use crate::{
    BeforeSaveOutcome, CommandReceipt, HostProgress, HostSession, HostState, SaveOperation,
};
use calc_value::{GridArea, SheetId};
use std::{
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
    WorksheetCalculate {
        sheet: SheetId,
    },
    BeforeSave {
        operation: SaveOperation,
    },
}

/// One registered owner. It supplies runtime-wired document sources, not handler names.
/// Change and Calculate run synchronously with a read-only host view. BeforeSave can
/// return the existing authored batch, which the host commits before the save pin.
pub trait WorkbookEventHandler {
    fn workbook_open(&self, _host: &HostState) -> Result<(), String> { Ok(()) }
    fn worksheet_change(
        &self,
        host: &HostState,
        sheet: SheetId,
        target: &[GridArea],
    ) -> Result<(), String>;
    fn worksheet_calculate(&self, host: &HostState, sheet: SheetId) -> Result<(), String>;
    fn before_save(
        &self,
        host: &HostState,
        operation: SaveOperation,
    ) -> Result<BeforeSaveOutcome, String>;
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
    enabled: bool,
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
            enabled: true,
            next: 1,
            failures: VecDeque::new(),
            omitted: 0,
            open_delivered: false,
            calculation: None,
        }
    }
}
impl HostSession {
    /// The application calls this once after admitting a file's owning project,
    /// before its first automatic calculation. Source replacement is not an open.
    pub fn notify_workbook_open(&mut self) {
        if std::mem::replace(&mut self.workbook_events.open_delivered, true) { return; }
        if let Some(owner) = self.event_owner()
            && let Err(reason) = owner.workbook_open(&self.host)
        {
            self.record_event_failure(WorkbookEvent::Open, reason);
        }
    }
    pub fn set_workbook_event_handler(&mut self, owner: Option<Rc<dyn WorkbookEventHandler>>) {
        self.workbook_events.owner = owner;
    }
    pub fn set_workbook_events_enabled(&mut self, enabled: bool) {
        self.workbook_events.enabled = enabled;
    }
    pub fn workbook_events_enabled(&self) -> bool {
        self.workbook_events.enabled
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
            .then(|| self.workbook_events.owner.clone())
            .flatten()
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
        let Some(owner) = self.event_owner() else {
            return;
        };
        let mut targets: BTreeMap<SheetId, Vec<GridArea>> = BTreeMap::new();
        for area in receipt.input_writes().iter().copied() {
            targets.entry(area.sheet()).or_default().push(area);
        }
        for (sheet, mut target) in targets {
            if self.host.model().sheet_by_id(sheet).is_none() {
                continue;
            }
            target.sort_by_key(|area| (area.first(), area.last()));
            target.dedup();
            if let Err(reason) = owner.worksheet_change(&self.host, sheet, &target) {
                self.record_event_failure(WorkbookEvent::WorksheetChange { sheet, target }, reason);
            }
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
        let Some(owner) = self.event_owner() else {
            return;
        };
        for sheet in sheets {
            if let Err(reason) = owner.worksheet_calculate(&self.host, sheet) {
                self.record_event_failure(WorkbookEvent::WorksheetCalculate { sheet }, reason);
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
        let outcome = owner.before_save(&self.host, operation);
        if let Err(reason) = &outcome {
            self.record_event_failure(WorkbookEvent::BeforeSave { operation }, reason.clone());
        }
        outcome
    }
}
