//! The crash-recovery journal: what was committed since the workbook was opened, and the drafts
//! that were not.
//!
//! The journal is local to the person's machine and never part of a workbook file. It is an
//! append-only text of one JSON record per line, so that a record is durable the moment its
//! line is written and a torn last line costs that record alone. After an abnormal exit the
//! journal is read back as a [`Recovered`] and replayed once: each committed step is sent again
//! in its original order with its original command identity, and each reply must report the
//! revision the step produced the first time. Text that was never committed comes back as held
//! drafts and is not applied.

use calc_skin::grid::Key;
use calc_skin::{Batch, Change, ModelChange, Operation, Selection, Stamp, WorkbookId};
use serde::{Deserialize, Serialize};

/// The record schema. A journal written by another schema is not replayed.
pub const SCHEMA: u32 = 1;

/// The workbook the steps were made on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Base {
    /// The blank workbook a session starts with.
    Blank,
    /// A workbook file. Its bytes are kept beside the journal by the store.
    File { name: String, bytes: u64 },
}

/// One change the workbook accepted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Step {
    Commit(Batch),
    Run(Operation),
    /// An accepted change that cannot be sent again, such as a macro run. Recovery stops here.
    Unreplayable { what: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub step: Step,
    /// The stamp the workbook reported for the step.
    pub after: Stamp,
}

/// Text typed into a cell and not committed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    pub at: Key,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Record {
    Opened { schema: u32, base: Base, at: Stamp },
    Entry(Entry),
    /// Every draft at this moment. The last one read is the one restored.
    Drafts(Vec<Draft>),
    Selection(Box<Selection>),
}

/// Where a journal is kept: `OPFS` and local storage in a browser, the profile folder on a
/// desktop. Every call is best effort; a store that cannot write says so once and the session
/// carries on without recovery.
pub trait JournalStore {
    /// The journal left by the session before this one, if it left one.
    fn read(&self) -> Option<String>;
    /// Starts a new journal with this text, replacing the current one.
    fn begin(&self, text: &str) -> Result<(), String>;
    /// Adds one line to the current journal.
    fn append(&self, line: &str) -> Result<(), String>;
    /// Forgets the journal and the workbook bytes kept with it.
    fn clear(&self);
    /// Keeps the bytes of the workbook that was opened.
    fn keep_base(&self, bytes: &[u8]) -> Result<(), String>;
    /// The kept workbook bytes, given to `then` when they have been read.
    fn base(&self, then: Box<dyn FnOnce(Option<Vec<u8>>)>);
}

/// Which accepted operations a recovery sends again.
fn replays(operation: &Operation) -> Option<bool> {
    Some(match operation {
        Operation::Undo
        | Operation::Redo
        | Operation::Revert(_)
        | Operation::Calculate { .. }
        | Operation::CalculateFull
        | Operation::CalculateRebuild
        | Operation::CalculateObserved
        | Operation::ToggleGridlines { .. }
        | Operation::ToggleFreezePanes { .. }
        | Operation::FreezeLeadingBand { .. }
        | Operation::CopyCells { .. } => true,
        // These change the workbook in ways a second run would not repeat exactly.
        Operation::RunMacro { .. } | Operation::RestoreCheckpoint { .. } | Operation::Decide { .. } => false,
        // These leave the workbook as it is.
        Operation::SetMacroSecurity(_)
        | Operation::Propose(_)
        | Operation::TryOn { .. }
        | Operation::CancelCalculation
        | Operation::PinBaseline { .. } => return None,
    })
}

/// Writes the journal of the running session.
pub struct Writer {
    store: std::rc::Rc<dyn JournalStore>,
    /// Why the journal stopped, said once.
    failed: Option<String>,
    drafts: Vec<Draft>,
    lines: usize,
}

impl Writer {
    pub fn new(store: std::rc::Rc<dyn JournalStore>) -> Self {
        Self {
            store,
            failed: None,
            drafts: Vec::new(),
            lines: 0,
        }
    }

    fn line(record: &Record) -> Option<String> {
        serde_json::to_string(record).ok()
    }

    fn fail(&mut self, reason: String) {
        if self.failed.is_none() {
            self.failed = Some(reason);
        }
    }

    /// Why recovery is off for this session, when the store refused a write.
    pub fn failure(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    /// A workbook was opened: a new journal starts on it.
    pub fn opened(&mut self, base: Base, bytes: Option<&[u8]>, at: Stamp) {
        self.failed = None;
        self.drafts.clear();
        self.lines = 1;
        let Some(line) = Self::line(&Record::Opened { schema: SCHEMA, base, at }) else { return };
        let kept = match bytes {
            Some(bytes) => self.store.keep_base(bytes),
            None => Ok(()),
        };
        if let Err(reason) = kept.and_then(|()| self.store.begin(&format!("{line}\n"))) {
            self.fail(reason);
        }
    }

    fn append(&mut self, record: &Record) {
        if self.failed.is_some() || self.lines == 0 {
            return;
        }
        let Some(line) = Self::line(record) else { return };
        match self.store.append(&format!("{line}\n")) {
            Ok(()) => self.lines += 1,
            Err(reason) => self.fail(reason),
        }
    }

    /// The workbook accepted a batch.
    pub fn committed(&mut self, batch: &Batch, after: Stamp) {
        self.append(&Record::Entry(Entry { step: Step::Commit(batch.clone()), after }));
    }

    /// The workbook accepted an operation.
    pub fn ran(&mut self, operation: &Operation, after: Stamp) {
        let step = match replays(operation) {
            Some(true) => Step::Run(operation.clone()),
            Some(false) => Step::Unreplayable { what: operation_label(operation).into() },
            None => return,
        };
        self.append(&Record::Entry(Entry { step, after }));
    }

    /// The drafts changed: the active one and the held ones.
    pub fn drafts(&mut self, drafts: Vec<Draft>) {
        if drafts != self.drafts {
            self.drafts = drafts.clone();
            self.append(&Record::Drafts(drafts));
        }
    }

    pub fn selected(&mut self, selection: &Selection) {
        self.append(&Record::Selection(Box::new(selection.clone())));
    }

    /// The person discarded the session's work or saved it elsewhere.
    pub fn clear(&mut self) {
        self.store.clear();
        self.lines = 0;
        self.drafts.clear();
    }
}

fn a1(at: Key) -> String {
    format!("{}{}", calc_skin::address::column_letters(at.3), at.2)
}

fn operation_label(operation: &Operation) -> &'static str {
    match operation {
        Operation::Undo => "Undo",
        Operation::Redo => "Redo",
        Operation::Revert(_) => "Revert",
        Operation::SetMacroSecurity(_) => "Macro security",
        Operation::RunMacro { .. } => "Macro run",
        Operation::RestoreCheckpoint { .. } => "Restore workbook",
        Operation::Propose(_) => "Proposal",
        Operation::Decide { .. } => "Proposal decision",
        Operation::TryOn { .. } => "Try on",
        Operation::Calculate { .. }
        | Operation::CalculateFull
        | Operation::CalculateRebuild
        | Operation::CalculateObserved => "Calculate",
        Operation::CancelCalculation => "Cancel calculation",
        Operation::ToggleGridlines { .. } => "Gridlines",
        Operation::ToggleFreezePanes { .. } | Operation::FreezeLeadingBand { .. } => "Freeze Panes",
        Operation::CopyCells { .. } => "Copy",
        Operation::PinBaseline { .. } => "Baseline",
    }
}

/// What a journal holds, read back after an abnormal exit.
#[derive(Clone, Debug, PartialEq)]
pub struct Recovered {
    pub base: Base,
    /// The stamp of the workbook when the journal began.
    pub opened: Stamp,
    pub entries: Vec<Entry>,
    pub drafts: Vec<Draft>,
    pub selection: Option<Selection>,
}

impl Recovered {
    /// Reads a journal. `None` when it holds nothing to recover: no journal, another schema,
    /// or a workbook that was opened and never changed.
    pub fn read(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        let Record::Opened { schema: SCHEMA, base, at } = serde_json::from_str(lines.next()?).ok()? else {
            return None;
        };
        let mut recovered = Self {
            base,
            opened: at,
            entries: Vec::new(),
            drafts: Vec::new(),
            selection: None,
        };
        for line in lines {
            // A line torn by the exit ends the journal: what follows it was never durable.
            let Ok(record) = serde_json::from_str::<Record>(line) else { break };
            match record {
                Record::Opened { .. } => break,
                Record::Entry(entry) => recovered.entries.push(entry),
                Record::Drafts(drafts) => recovered.drafts = drafts,
                Record::Selection(selection) => recovered.selection = Some(*selection),
            }
        }
        recovered.drafts.retain(|draft| !draft.text.is_empty());
        (!recovered.entries.is_empty() || !recovered.drafts.is_empty()).then_some(recovered)
    }

    /// The committed changes that count for the person: every entry that changed the workbook.
    pub fn committed(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| !matches!(entry.step, Step::Run(Operation::CopyCells { .. } | Operation::Calculate { .. }
                | Operation::CalculateFull | Operation::CalculateRebuild | Operation::CalculateObserved)))
            .count()
    }

    /// The recovery bar's text, as the design book words it.
    pub fn summary(&self) -> String {
        let plural = |count: usize, one: &str, many: &str| format!("{count} {}", if count == 1 { one } else { many });
        format!(
            "Unsaved edits recovered ({}, {})",
            plural(self.committed(), "committed change", "committed changes"),
            plural(self.drafts.len(), "held draft", "held drafts"),
        )
    }

    /// One line per entry and draft, for Inspect diff.
    pub fn lines(&self) -> Vec<String> {
        let mut lines: Vec<String> = self.entries.iter().map(|entry| match &entry.step {
            Step::Commit(batch) => batch.label.clone().unwrap_or_else(|| match batch.changes.as_slice() {
                [Change::Model(ModelChange::Enter { at, text })] => {
                    format!("Enter {} at {}", text, a1(*at))
                }
                changes => format!("{} workbook changes", changes.len()),
            }),
            Step::Run(operation) => operation_label(operation).into(),
            Step::Unreplayable { what } => format!("{what} (cannot be replayed)"),
        }).collect();
        lines.extend(self.drafts.iter().map(|draft| {
            format!("Draft at {}: {}", a1(draft.at), draft.text)
        }));
        lines
    }

    /// The steps to send to a workbook freshly opened as `now`, in order. The workbook's new
    /// identity replaces the one the steps were recorded against, and a step's check against
    /// the selection sequence is dropped: the selection is not replayed.
    pub fn plan(&self, now: Stamp) -> Vec<Planned> {
        let rebase = |workbook: WorkbookId| if workbook == self.opened.workbook { now.workbook } else { workbook };
        let mut plan = Vec::new();
        for entry in &self.entries {
            let step = match entry.step.clone() {
                Step::Commit(mut batch) => {
                    batch.workbook = batch.workbook.map(rebase);
                    batch.selection_seq = None;
                    for change in &mut batch.changes {
                        if let Change::Model(
                            ModelChange::MoveSpillCell { expected, .. } | ModelChange::RefreshPivot { expected, .. },
                        ) = change
                        {
                            expected.workbook = rebase(expected.workbook);
                        }
                    }
                    Step::Commit(batch)
                }
                Step::Run(mut operation) => {
                    match &mut operation {
                        Operation::ToggleGridlines { workbook, .. }
                        | Operation::ToggleFreezePanes { workbook, .. }
                        | Operation::FreezeLeadingBand { workbook, .. }
                        | Operation::CopyCells { workbook, .. } => *workbook = rebase(*workbook),
                        _ => {}
                    }
                    Step::Run(operation)
                }
                unreplayable @ Step::Unreplayable { .. } => unreplayable,
            };
            plan.push(Planned {
                step,
                after: Stamp { workbook: rebase(entry.after.workbook), ..entry.after },
            });
        }
        plan
    }

    /// The highest command serial the journal used, so new commands continue after it.
    pub fn last_command(&self) -> u64 {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.step {
                Step::Commit(batch) => batch.id.parse().ok(),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    }
}

/// One step of a recovery, with the stamp its reply must report.
#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    pub step: Step,
    pub after: Stamp,
}

/// How a recovery ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Every step was replayed and every reply matched.
    Exact { committed: usize, drafts: usize },
    /// The replay stopped: the workbook holds the first `replayed` steps and no more.
    Stopped { replayed: usize, of: usize, why: String },
}

impl Outcome {
    pub fn text(&self) -> String {
        match self {
            Outcome::Exact { committed, drafts } => {
                format!("Restored {committed} committed changes and {drafts} held drafts.")
            }
            Outcome::Stopped { replayed, of, why } => {
                format!("Restored {replayed} of {of} recorded steps, then stopped: {why}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_skin::grid::{Area, Sheet};
    use calc_skin::{CalcGeneration, Revision, SourceRef};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct Memory {
        text: RefCell<Option<String>>,
        base: RefCell<Option<Vec<u8>>>,
        full: std::cell::Cell<bool>,
    }

    impl JournalStore for Memory {
        fn read(&self) -> Option<String> {
            self.text.borrow().clone()
        }
        fn begin(&self, text: &str) -> Result<(), String> {
            *self.text.borrow_mut() = Some(text.into());
            Ok(())
        }
        fn append(&self, line: &str) -> Result<(), String> {
            if self.full.get() {
                return Err("The browser's storage is full".into());
            }
            self.text.borrow_mut().get_or_insert_default().push_str(line);
            Ok(())
        }
        fn clear(&self) {
            *self.text.borrow_mut() = None;
            *self.base.borrow_mut() = None;
        }
        fn keep_base(&self, bytes: &[u8]) -> Result<(), String> {
            *self.base.borrow_mut() = Some(bytes.to_vec());
            Ok(())
        }
        fn base(&self, then: Box<dyn FnOnce(Option<Vec<u8>>)>) {
            then(self.base.borrow().clone());
        }
    }

    fn stamp(workbook: u64, model: u64) -> Stamp {
        Stamp {
            workbook: WorkbookId(workbook),
            model: Revision(model),
            view: Revision(0),
            calc: CalcGeneration(model),
        }
    }

    fn enter(id: u64, workbook: u64, base: u64, at: Key, text: &str) -> Batch {
        Batch {
            id: id.to_string(),
            workbook: Some(WorkbookId(workbook)),
            base: Some(Revision(base)),
            expects: Vec::new(),
            selection_seq: Some(9),
            label: None,
            changes: vec![Change::Model(ModelChange::Enter { at, text: text.into() })],
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_journal_reads_back_its_steps_its_last_drafts_and_its_selection() {
        let store = Rc::new(Memory::default());
        let mut writer = Writer::new(store.clone());
        let at = Key(0, 1, 2, 2);
        writer.opened(Base::Blank, None, stamp(1, 0));
        assert_eq!(Recovered::read(&store.read().unwrap()), None, "an unchanged workbook has nothing to recover");
        writer.committed(&enter(1, 1, 0, at, "5"), stamp(1, 1));
        writer.ran(&Operation::CopyCells { workbook: WorkbookId(1), copy: 1, from: Area(at, at) }, stamp(1, 1));
        writer.ran(&Operation::SetMacroSecurity(calc_skin::MacroSecurity::Off), stamp(1, 1));
        writer.committed(&enter(2, 1, 1, Key(0, 1, 3, 2), "=B2*2"), stamp(1, 2));
        writer.ran(&Operation::Undo, stamp(1, 3));
        writer.drafts(vec![Draft { at, text: "=SU".into() }]);
        writer.drafts(vec![Draft { at, text: "=SUM(".into() }]);
        writer.drafts(vec![Draft { at, text: "=SUM(".into() }]);
        let selection = Selection {
            seq: 4,
            target: SourceRef::Cells { sheet: Sheet(0, 1), areas: vec![Area(at, at)] },
            active: Some(at),
            grouped: Vec::new(),
            item: None,
        };
        writer.selected(&selection);
        let text = store.read().unwrap();
        assert_eq!(text.lines().count(), 8, "the unchanged drafts and the session setting add no line");
        let recovered = Recovered::read(&text).unwrap();
        assert_eq!(recovered.base, Base::Blank);
        assert_eq!(recovered.entries.len(), 4);
        assert_eq!(recovered.committed(), 3, "Copy is replayed and is not a change to count");
        assert_eq!(recovered.drafts, vec![Draft { at, text: "=SUM(".into() }]);
        assert_eq!(recovered.selection, Some(selection));
        assert_eq!(recovered.summary(), "Unsaved edits recovered (3 committed changes, 1 held draft)");
        assert_eq!(recovered.lines(), ["Enter 5 at B2", "Copy", "Enter =B2*2 at B3", "Undo", "Draft at B2: =SUM("]);
        assert_eq!(recovered.last_command(), 2);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_plan_moves_the_steps_to_the_reopened_workbook_and_keeps_their_revisions() {
        let store = Rc::new(Memory::default());
        let mut writer = Writer::new(store.clone());
        let at = Key(0, 1, 1, 1);
        writer.opened(Base::File { name: "Budget.xlsx".into(), bytes: 3 }, Some(&[1, 2, 3]), stamp(4, 0));
        writer.committed(&enter(1, 4, 0, at, "5"), stamp(4, 1));
        writer.ran(&Operation::ToggleGridlines { workbook: WorkbookId(4), sheet: Sheet(0, 1) }, stamp(4, 1));
        writer.ran(&Operation::RunMacro { approval: 7 }, stamp(4, 2));
        let recovered = Recovered::read(&store.read().unwrap()).unwrap();
        let kept = Rc::new(RefCell::new(None));
        store.base(Box::new({
            let kept = kept.clone();
            move |bytes| *kept.borrow_mut() = bytes
        }));
        assert_eq!(*kept.borrow(), Some(vec![1, 2, 3]));
        let plan = recovered.plan(stamp(2, 0));
        let Step::Commit(batch) = &plan[0].step else { panic!("a commit") };
        assert_eq!((batch.workbook, batch.base, batch.selection_seq), (Some(WorkbookId(2)), Some(Revision(0)), None));
        assert_eq!(batch.id, "1", "the command keeps its identity, so it is applied once");
        assert_eq!(plan[0].after, stamp(2, 1));
        assert_eq!(plan[1].step, Step::Run(Operation::ToggleGridlines { workbook: WorkbookId(2), sheet: Sheet(0, 1) }));
        assert_eq!(plan[2].step, Step::Unreplayable { what: "Macro run".into() });
        assert_eq!(recovered.lines()[2], "Macro run (cannot be replayed)");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_torn_line_ends_the_journal_and_a_full_store_stops_it_once() {
        let store = Rc::new(Memory::default());
        let mut writer = Writer::new(store.clone());
        let at = Key(0, 1, 1, 1);
        writer.opened(Base::Blank, None, stamp(1, 0));
        writer.committed(&enter(1, 1, 0, at, "5"), stamp(1, 1));
        writer.committed(&enter(2, 1, 1, at, "6"), stamp(1, 2));
        let text = store.read().unwrap();
        let torn = &text[..text.len() - 20];
        assert_eq!(Recovered::read(torn).unwrap().entries.len(), 1, "the torn record alone is lost");
        assert_eq!(Recovered::read("{\"Opened\":{\"schema\":99"), None);
        assert_eq!(Recovered::read(""), None);
        store.full.set(true);
        writer.committed(&enter(3, 1, 2, at, "7"), stamp(1, 3));
        writer.committed(&enter(4, 1, 3, at, "8"), stamp(1, 4));
        assert_eq!(writer.failure(), Some("The browser's storage is full"));
        assert_eq!(Recovered::read(&store.read().unwrap()).unwrap().entries.len(), 2);
        writer.clear();
        assert_eq!(store.read(), None);
        assert_eq!(Outcome::Exact { committed: 2, drafts: 1 }.text(), "Restored 2 committed changes and 1 held drafts.");
    }
}
