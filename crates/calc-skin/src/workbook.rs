use crate::grid::{Area, Key, Sheet};
use crate::{AnchorId, DocId, Selection, SelectionCause, SourceRef, Target, ViewId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

macro_rules! counter {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self(value)
            }
        }
    };
}

counter!(WorkbookId);
counter!(Revision);
counter!(CalcGeneration);
counter!(WatchId);
counter!(EntryId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    pub workbook: WorkbookId,
    pub model: Revision,
    pub view: Revision,
    pub calc: CalcGeneration,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provenance {
    Empty,
    Literal,
    Calculated {
        calc: CalcGeneration,
    },
    AsSaved,
    External {
        source: String,
        observed_at: Option<u64>,
    },
    Stale {
        since: Revision,
        cause: StaleCause,
    },
    Refused {
        owner: String,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaleCause {
    ManualMode,
    Calculating,
    CalcFailed,
    Cancelled,
    SourceUnavailable,
    UnrefreshedPivotCache { source_revision: Revision },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shown<T> {
    pub value: T,
    pub provenance: Provenance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartialWhy {
    Budget,
    Window,
    Pending,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Coverage {
    Complete,
    Partial {
        done: u64,
        total: Option<u64>,
        why: PartialWhy,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Answer<T> {
    pub stamp: Stamp,
    pub coverage: Coverage,
    pub body: T,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Evidence<T> {
    Observed {
        observer: String,
        at: Stamp,
        body: T,
    },
    Derived {
        from: Derivation,
        at: Stamp,
        body: T,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Derivation {
    Dependencies,
    Structure,
    FileInventory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub cells: u64,
    pub nodes: u64,
    pub edges: u64,
    pub bytes: u64,
    pub millis: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Capability {
    Workbook,
    Vba,
    NativeAddins,
    ComRtd,
    NativeFileDialogs,
    ExcelClipboardFormats,
    Printing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    NotModelled {
        owner: String,
        what: String,
    },
    NotAvailableHere {
        capability: Capability,
        route: Option<String>,
    },
    Conflict {
        base: Revision,
        conflicts: Vec<Target>,
    },
    Invalid {
        reason: String,
    },
    StaleSelection,
    ReadOnly,
    Protected(SourceRef),
    Validation {
        target: SourceRef,
        reason: String,
    },
    Excel {
        reason: String,
    },
    OverBudget {
        counts: Budget,
        alternatives: Vec<String>,
    },
    PolicyRefused,
    Irreversible {
        reason: String,
    },
    SchemaMismatch {
        expected: u32,
        received: u32,
    },
    Transport(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub read_only: bool,
    pub supported: Vec<Capability>,
    pub platform_reserved_chords: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interest {
    Area(Area),
    Areas(Vec<Area>),
    Sheet(Sheet),
    Selection,
    Anchors(Vec<AnchorId>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum WorkbookQuery {
    Summary,
    Census { scope: SourceRef },
    Cells { interest: Interest },
    Geometry { sheet: Sheet },
    Formula { at: Key },
    ViewPart,
    History,
    Resolve { address: String },
    PrintPlan { source: SourceRef },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    pub id: String,
    pub base: Option<Revision>,
    pub expects: Vec<SourceRef>,
    pub selection_seq: Option<u64>,
    pub label: Option<String>,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Change {
    Model(ModelChange),
    Presentation(PresentationChange),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ModelChange {
    Enter {
        at: Key,
        text: String,
    },
    ClearContents {
        areas: Vec<Area>,
    },
    RewriteFormula {
        at: Key,
        expected_old: String,
        new_text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PresentationChange {
    CreateDoc {
        doc: DocId,
        view: ViewId,
        sheet: Option<Sheet>,
        schema: u32,
        body: Value,
    },
    RemoveDoc {
        doc: DocId,
    },
    Bind {
        anchor: AnchorId,
        source: SourceRef,
    },
    RemoveAnchor {
        anchor: AnchorId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeKind {
    Model,
    Presentation,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewDepth {
    WritesOnly,
    WithEffects { budget: Budget },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelWriteDiff {
    pub at: Key,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preview {
    pub base: Stamp,
    pub writes: Vec<ModelWriteDiff>,
    pub presentation: Vec<PresentationChange>,
    pub warnings: Vec<String>,
    pub refusal: Option<Refusal>,
}

impl Preview {
    pub fn kind(&self) -> Option<ChangeKind> {
        match (self.writes.is_empty(), self.presentation.is_empty()) {
            (false, false) => Some(ChangeKind::Both),
            (false, true) => Some(ChangeKind::Model),
            (true, false) => Some(ChangeKind::Presentation),
            (true, true) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    User { view: ViewId },
    Agent { agent: String, proposal: String },
    Macro { run: String, procedure: String },
    Automation,
    Extension { id: String, view: ViewId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WritePolicy {
    Commit,
    ApplyThenReview,
    Propose,
    Look,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reversibility {
    Reversible,
    DocumentOnly { external_effects: Vec<String> },
    Irreversible { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: EntryId,
    pub at: Stamp,
    pub label: String,
    pub origin: Origin,
    pub kind: ChangeKind,
    pub targets: Vec<Target>,
    pub cells_touched: u64,
    pub reversibility: Reversibility,
    pub group: Option<String>,
    pub unreviewed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Operation {
    Undo,
    Redo,
    Revert(EntryId),
    Calculate { scope: SourceRef },
    CancelCalculation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    Selection {
        selection: Selection,
        cause: SelectionCause,
    },
    History(HistoryEntry),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Update {
    Ready {
        watch: WatchId,
        coverage: Coverage,
        body: Value,
    },
    Outdated {
        watch: WatchId,
        since: Stamp,
    },
    Refused {
        watch: WatchId,
        reason: Refusal,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub stamp: Stamp,
    pub events: Vec<Event>,
    pub updates: Vec<Update>,
}

pub trait WorkbookPort {
    fn capabilities(&self) -> Capabilities;
    fn ask(&self, query: WorkbookQuery, budget: Budget) -> Result<Answer<Value>, Refusal>;
    fn watch(&self, query: WorkbookQuery, budget: Budget) -> Result<WatchId, Refusal>;
    fn unwatch(&self, watch: WatchId);
    fn select(&self, selection: Selection, client_seq: u64) -> Result<(), Refusal>;
    fn navigate(&self, target: Target) -> Result<(), Refusal>;
    fn preview(&self, batch: &Batch, depth: PreviewDepth) -> Result<Preview, Refusal>;
    fn commit(&self, batch: Batch) -> Result<Stamp, Refusal>;
    fn run(&self, operation: Operation) -> Result<Stamp, Refusal>;
    fn next_frame(&self) -> Result<Option<Frame>, Refusal>;
}
