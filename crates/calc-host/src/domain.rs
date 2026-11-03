//! Owner coordination of connected workbook engines. Files and provider lifetimes belong
//! to the caller; this portable owner admits calculation and exchanges immutable input cuts.
use crate::{HostProgress, HostState, ReadObserver, RecalcFailed, Unobserved, WorkBudget};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DomainWorkbookId(u64);

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
    pins: BTreeMap<DomainWorkbookId, std::sync::Arc<crate::LinkedWorkbookSnapshot>>,
}

#[derive(Default)]
pub struct CalculationDomain {
    next: u64,
    workbooks: BTreeMap<DomainWorkbookId, HostState>,
    links: BTreeMap<(DomainWorkbookId, u32), DomainWorkbookId>,
    work: Option<DomainWork>,
}
impl CalculationDomain {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, host: HostState) -> Result<DomainWorkbookId, DomainError> {
        if self.work.is_some() {
            return Err(DomainError::Busy);
        }
        self.next = self
            .next
            .checked_add(1)
            .expect("domain workbook identity exhausted");
        let id = DomainWorkbookId(self.next);
        self.workbooks.insert(id, host);
        Ok(id)
    }
    pub fn host(&self, id: DomainWorkbookId) -> Option<&HostState> {
        self.workbooks.get(&id)
    }
    /// Before allowing an edit, invalidate destination contexts. No result from the old
    /// cut can remain current merely because a source later calculates an equal value.
    pub fn host_mut(&mut self, id: DomainWorkbookId) -> Result<&mut HostState, DomainError> {
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
        host.set_linked_workbook(number, None)
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
        host.set_linked_workbook(number, None)
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
            pins: BTreeMap::new(),
        });
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
            return DomainProgress::Complete;
        };
        if !work.started {
            let pins: Vec<_> = self
                .links
                .iter()
                .filter(|((destination, _), _)| *destination == id)
                .map(|((_, number), source)| (*number, work.pins[source].clone()))
                .collect();
            let host = self.workbooks.get_mut(&id).expect("admitted workbook");
            for (number, pin) in pins {
                host.set_linked_workbook(number, Some(pin))
                    .expect("connected link");
            }
            if work.full {
                crate::calculate_full(host);
            } else if host.work.is_none() {
                crate::calculate(host);
            }
            work.started = true;
        }
        let host = self.workbooks.get_mut(&id).expect("admitted workbook");
        let progress = if observed == Some(id) {
            crate::step(host, budget, observer)
        } else {
            crate::step(host, budget, &mut Unobserved)
        };
        match progress {
            HostProgress::Waiting { .. } => DomainProgress::Waiting { workbook: id },
            HostProgress::Failed(error) => {
                self.work = None;
                DomainProgress::Failed {
                    workbook: id,
                    error,
                }
            }
            HostProgress::Cancelled => {
                self.work = None;
                DomainProgress::Advanced {
                    workbook: id,
                    progress,
                }
            }
            _ => {
                if host.work.is_none() {
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
