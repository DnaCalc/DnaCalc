use crate::grid::{Area, Key, Sheet};
use crate::{AnchorId, Bound, DocId, RawValue, Selection, SelectionCause, SourceRef, Target, ViewId};
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
    CircularHold,
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
    Contributions,
    Structure,
    FileInventory,
    ExpressionStructure,
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
    RequiresWorkbook {
        reason: String,
        route: crate::FormulaLink,
    },
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
        /// The inputs at review and at refusal, when both revisions are retained.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        diffs: Vec<ModelWriteDiff>,
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
    CloseSummary,
    Census {
        scope: SourceRef,
    },
    Inventory {
        kinds: Vec<crate::InventoryKind>,
    },
    Structure {
        scope: SourceRef,
    },
    ReferenceOperands {
        at: Key,
    },
    InputCandidates {
        scope: SourceRef,
    },
    OutputCandidates {
        scope: SourceRef,
    },
    Objects { sheet: Sheet },
    ChartImage {
        object: SourceRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comparison: Option<Bound>,
    },
    NumericComparison { bound: Bound },
    Sources,
    PivotFreshness { at: Key },
    Deps {
        of: SourceRef,
        dir: crate::DependencyDirection,
        depth: u16,
        group: crate::DependencyGrouping,
    },
    DependencyAtlas,
    CompareCells { left: Key, right: Key },
    Issues {
        scope: SourceRef,
    },
    Contributions {
        at: Key,
    },
    Cells {
        interest: Interest,
    },
    Rows {
        source: SourceRef,
        filter: Vec<RowFilter>,
        sort: Vec<RowSort>,
        window: RowWindow,
    },
    Placement {
        sheet: Sheet,
        kind: PlacementKind,
        shape: Option<PlacementShape>,
        /// Previously declared lanes on this sheet, without their spacers.
        #[serde(default)]
        reserved: Vec<Area>,
    },
    NewSheetPlacement {
        name: String,
        kind: PlacementKind,
        shape: Option<PlacementShape>,
    },
    SpillBlockers { anchor: Key },
    DefinedName { scope: crate::NameScope, name: String },
    FormulaNames { cells: Vec<Key> },
    Geometry {
        sheet: Sheet,
    },
    Formula {
        at: Key,
    },
    TrialFor {
        at: Key,
    },
    EvaluateDraftPart {
        at: Key,
        formula: String,
        start: usize,
        len: usize,
    },
    ExpressionTree {
        at: Key,
    },
    Rules {
        at: Key,
    },
    ValidationSettings {
        selected: Area,
    },
    CalcState,
    CalcChanges,
    /// Retained evaluator entries from the latest publication of an observed command.
    CalcExecution,
    CalcIteration,
    ViewPart,
    Notes {
        anchor: Option<AnchorId>,
    },
    History,
    Proposals,
    Resolve {
        address: String,
    },
    ResolveBound {
        bound: Bound,
    },
    ConsistentBindings {
        reads: Vec<crate::BindingRead>,
    },
    BoundInput {
        bound: Bound,
    },
    PrintPlan {
        source: SourceRef,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetSummary {
    pub sheet: Sheet,
    pub name: String,
    pub visible: bool,
    pub worksheet: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbookSummary {
    pub name: String,
    pub sheets: Vec<SheetSummary>,
    pub edited: bool,
    pub layout_edited: bool,
    pub date1904: bool,
    pub macros: bool,
    #[serde(default)]
    pub pending_proposals: u64,
    pub capabilities: Capabilities,
}

/// Authored changes compared with the open workbook.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseSummary {
    pub has_changes: bool,
    pub changed_inputs: u64,
    pub added_scenarios: u64,
    pub layout_edited: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorksheetGeometry {
    pub rows: Vec<(u32, Option<f64>, bool)>,
    pub columns: Vec<(u32, u32, Option<f64>, bool)>,
    pub default_row_height: f64,
    pub default_column_width: f64,
    pub default_digit_px: f64,
    pub merges: Vec<Area>,
    pub frozen_rows: u32,
    pub frozen_columns: u32,
    pub zoom: u16,
    pub show_grid_lines: Option<bool>,
    #[serde(default)]
    pub auto_filter_areas: Vec<Area>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpillExtent {
    Filled(Area),
    Blocked(Area),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpillBlockers {
    pub desired: Area,
    pub blockers: Vec<SpillBlocker>,
    pub more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpillBlocker {
    pub area: Area,
    pub cell_to_move: Option<Key>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkbookCell {
    pub at: Key,
    pub input: String,
    pub shown: Shown<crate::RawValue>,
    pub text: String,
    pub spill: Option<SpillExtent>,
    pub style: u32,
    #[serde(default, skip_serializing_if = "CellBorders::is_empty")]
    pub borders: CellBorders,
}

/// The visible thin, continuous edges of a cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellBorders {
    pub top: Option<ThinBorder>,
    pub right: Option<ThinBorder>,
    pub bottom: Option<ThinBorder>,
    pub left: Option<ThinBorder>,
}

impl CellBorders {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThinBorder {
    Automatic,
    Rgb([u8; 3]),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CellWindow {
    pub areas: Vec<Area>,
    pub cells: Vec<WorkbookCell>,
}

/// An exact match on the raw value in a zero-based source column. Filters are combined with AND.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RowFilter {
    pub column: u32,
    pub equals: crate::RawValue,
}

/// One sort key in priority order, using a zero-based source column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowSort {
    pub column: u32,
    pub descending: bool,
}

/// Zero-based offset in the filtered, sorted result and number of rows requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowWindow {
    pub offset: u32,
    pub limit: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorksheetRow {
    /// The original, one-based worksheet row, retained across view sort and filter.
    pub row: u32,
    pub cells: Vec<WorkbookCell>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RowPage {
    /// The source bounds, including table metadata rows where applicable.
    pub bounds: Area,
    pub columns: u32,
    pub headers: Option<Vec<String>>,
    pub totals: Option<Vec<WorkbookCell>>,
    pub source_rows: u32,
    pub matched_rows: u32,
    pub offset: u32,
    pub rows: Vec<WorksheetRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlacementKind {
    Scalar,
    Array,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacementShape {
    pub rows: u32,
    pub columns: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub at: Key,
    pub label: Option<Key>,
    /// Width reserved for this array, from its anchor to the bottom of the sheet.
    pub lane: Option<Area>,
    /// The read dependency for rechecking a proposed write, including lane spacing.
    pub footprint: Area,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    pub id: String,
    #[serde(default)]
    pub workbook: Option<WorkbookId>,
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
    /// Move a current spill anchor or an authored cell blocking its intended footprint.
    MoveSpillCell { anchor: Key, from: Key, to: Key, expected: Stamp },
    Name { scope: crate::NameScope, change: NameChange },
    /// Create a worksheet at the identity returned by NewSheetPlacement at this model revision.
    InsertWorksheet {
        name: String,
        sheet: Sheet,
    },
    Enter {
        at: Key,
        text: String,
    },
    /// An App input may change a scalar literal only while both binding identities agree.
    EnterBound {
        bound: Bound,
        text: String,
    },
    /// Advance a bound nonnegative integer control by one, with worksheet validation.
    IncrementBound { bound: Bound },
    /// Insert the changing values of an existing worksheet scenario in one workbook edit.
    ShowScenario { scenario: crate::ScenarioRef },
    /// Capture the current authored literal values in the worksheet's native Scenario Manager.
    SaveScenario {
        scenario: crate::ScenarioRef,
        inputs: Vec<Bound>,
    },
    /// Capture the current literal values in the selected worksheet areas.
    SaveScenarioCells {
        scenario: crate::ScenarioRef,
        areas: Vec<Area>,
    },
    /// Rename or remove a native Scenario Manager record.
    EditScenario { scenario: crate::ScenarioRef, change: ScenarioChange },
    /// Restore the exact authored literals from the open workbook to the chosen bound inputs.
    ResetInputs { inputs: Vec<Bound> },
    ClearContents {
        areas: Vec<Area>,
    },
    PasteCopied {
        copy: u64,
        to: Area,
        contents: PasteContents,
    },
    FillRange {
        selected: Area,
        direction: FillDirection,
    },
    FlashFill {
        selected: Area,
    },
    SetColumnsHidden {
        sheet: Sheet,
        /// Inclusive first and last column in each band.
        bands: Vec<(u32, u32)>,
        hidden: bool,
    },
    ToggleAutoFilter {
        selected: Area,
    },
    SetValidation {
        selected: Area,
        settings: ValidationSettings,
    },
    SetAllBorders {
        selected: Area,
    },
    SetOutlineBorder {
        selected: Area,
    },
    /// Refresh the workbook cache used by the PivotTable at this cell.
    RefreshPivot { at: Key, expected: Stamp },
    /// Sort a selected rectangle by its relative columns, using workbook values.
    SortRows {
        selected: Area,
        keys: Vec<RowSort>,
        header: bool,
        case_sensitive: bool,
    },
    RewriteFormula {
        at: Key,
        expected_old: String,
        new_text: String,
    },
    /// Replace one explicitly chosen scalar reference. Its bounds refer to `expected_old`.
    ReplaceReference {
        at: Key,
        expected_old: String,
        occurrence: crate::OperandSpan,
        with: String,
    },
    /// Replace matching members of one inferred formula run, using `at` as the copy origin.
    RewriteFormulaRun {
        at: Key,
        expected_old: String,
        new_text: String,
        extent: Area,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationSettings {
    AnyValue,
    List { source: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NameChange {
    Define { name: String, formula: String },
    DefineFunction { name: String, formula: String },
    Rename { name: String, to: String },
    SetFormula { name: String, formula: String },
    Remove { name: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteContents {
    Values,
    Formulas,
    AllTransposed,
    Link,
    Special {
        part: PastePart,
        transpose: bool,
        #[serde(default)]
        skip_blanks: bool,
        #[serde(default)]
        arithmetic: Option<PasteArithmetic>,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PastePart {
    #[default]
    All,
    Formulas,
    Values,
    Formats,
    Comments,
    Validation,
    AllUsingSourceTheme,
    AllExceptBorders,
    ColumnWidths,
    FormulasAndNumberFormats,
    ValuesAndNumberFormats,
    AllMergingConditionalFormats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteArithmetic {
    Add,
    Subtract,
    Multiply,
    Divide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillDirection {
    Down,
    Right,
    Up,
    Left,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PresentationChange {
    CreateDoc {
        doc: DocId,
        #[serde(default)]
        expect_view_rev: Option<Revision>,
        view: ViewId,
        sheet: Option<Sheet>,
        schema: u32,
        body: Value,
    },
    PatchDoc {
        doc: DocId,
        expect_view_rev: Revision,
        body: Value,
    },
    RemoveDoc {
        doc: DocId,
    },
    Trail {
        enabled: bool,
    },
    PreferView {
        view: Option<ViewId>,
        expect_view_rev: Revision,
    },
    /// Save the skin declaration in the workbook's authored presentation.
    KeepSkin {
        skin: Box<crate::SkinManifest>,
        expect_view_rev: Revision,
    },
    /// Keep a reviewed App page whose bindings are resolved against this workbook.
    KeepAppPage {
        doc: DocId,
        page: Box<crate::Page>,
        expect_view_rev: Revision,
    },
    Bind {
        anchor: AnchorId,
        source: SourceRef,
    },
    RemoveAnchor {
        anchor: AnchorId,
    },
    PutNote {
        id: crate::NoteId,
        anchor: AnchorId,
        text: String,
    },
    RemoveNote {
        id: crate::NoteId,
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

/// A changed output from two isolated calculations. Its provenance is local to the preview.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ValueEffect {
    pub at: Key,
    pub before: Shown<RawValue>,
    pub after: Shown<RawValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelProperty {
    StyleTable,
    ArrayFormulas,
    MergedCells,
    ConditionalFormatting,
    DataValidation,
    Notes,
    Hyperlinks,
    RichText,
    ColumnVisibility,
    ColumnWidths,
    AutoFilter,
    PivotCache,
    Charts,
    Shapes,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelPropertyDiff {
    CellMoved { from: Key, to: Key },
    DefinedName {
        scope: crate::NameScope,
        before: Option<DefinedNameText>,
        after: Option<DefinedNameText>,
    },
    AddedScenario {
        scenario: crate::ScenarioRef,
        values: Vec<(Key, String)>,
    },
    EditedScenario { scenario: crate::ScenarioRef, change: ScenarioChange },
    WorksheetInserted { sheet: Sheet, name: String },
    CellFormat {
        at: Key,
        before: Option<u32>,
        after: Option<u32>,
    },
    Scope { source: SourceRef, property: ModelProperty },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinedNameText {
    pub name: String,
    pub formula: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScenarioChange {
    Rename { to: String },
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefinedNameFacts {
    pub definition: DefinedNameText,
    pub parameters: Option<Vec<String>>,
    /// The direct range or spill anchor when the workbook name has a stable address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preview {
    pub base: Stamp,
    pub writes: Vec<ModelWriteDiff>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<ModelPropertyDiff>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<ValueEffect>,
    pub presentation: Vec<PresentationChange>,
    pub warnings: Vec<String>,
    pub refusal: Option<Refusal>,
}

impl Preview {
    pub fn kind(&self) -> Option<ChangeKind> {
        let model_empty = self.writes.is_empty() && self.properties.is_empty();
        match (model_empty, self.presentation.is_empty()) {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_unit: Option<UnitId>,
    pub unreviewed: bool,
}

/// The agent's proposal identity within an open workbook.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProposalId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnitId(pub String);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub at: Stamp,
    pub intent: String,
    pub units: Vec<ProposalUnit>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProposalUnit {
    pub id: UnitId,
    pub needs: Vec<UnitId>,
    pub batch: Batch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalStatus {
    Pending,
    Accepted,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProposalReview {
    pub id: ProposalId,
    pub at: Stamp,
    pub agent: String,
    pub intent: String,
    pub units: Vec<ProposalUnitReview>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProposalUnitReview {
    pub id: UnitId,
    pub needs: Vec<UnitId>,
    pub label: String,
    pub status: ProposalStatus,
    pub writes: Vec<ModelWriteDiff>,
    pub properties: Vec<ModelPropertyDiff>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrailOutcome {
    Accepted,
    Rejected,
    Undone,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrailEvent {
    pub timestamp_millis: u64,
    pub agent: String,
    pub proposal: ProposalId,
    pub unit: UnitId,
    pub intent: String,
    pub targets: Vec<String>,
    pub outcome: TrailOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrailFacts {
    pub enabled: bool,
    pub dropped: u64,
    pub events: Vec<TrailEvent>,
}

impl Default for TrailFacts {
    fn default() -> Self {
        Self {
            enabled: true,
            dropped: 0,
            events: Vec::new(),
        }
    }
}

/// Code-free presentation that an agent can offer for temporary preview.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TryOnPayload {
    Skin(Box<crate::SkinManifest>),
    Page(Box<crate::Page>),
}

/// A look delivered to the workbook operator with its author and source revision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LookOffer {
    pub at: Stamp,
    pub agent: String,
    pub proposal: String,
    pub look: TryOnPayload,
}

/// One leading worksheet band to freeze.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FreezeBand {
    TopRow,
    FirstColumn,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Operation {
    Undo,
    Redo,
    Revert(EntryId),
    SetMacroSecurity(crate::MacroSecurity),
    RunMacro { approval: u64 },
    /// Stage model changes from an agent without changing the workbook.
    Propose(Proposal),
    /// Resolve exactly these pending units as an operator.
    Decide { proposal: ProposalId, accept: Vec<UnitId>, reject: Vec<UnitId> },
    /// Offer a declarative appearance to the workbook operator for temporary preview.
    TryOn { expected: Stamp, look: TryOnPayload },
    Calculate { scope: SourceRef },
    /// Recalculate every workbook formula, retaining the existing dependency bindings.
    CalculateFull,
    /// Rebuild the workbook's dependency bindings and then recalculate every formula.
    CalculateRebuild,
    /// Recalculate the workbook with execution observation on for this command.
    CalculateObserved,
    CancelCalculation,
    ToggleGridlines { workbook: WorkbookId, sheet: Sheet },
    /// Freeze the leading bands before the confirmed active cell, or unfreeze the sheet.
    ToggleFreezePanes {
        workbook: WorkbookId,
        at: Key,
        /// The worksheet viewport has moved away from its leading row or column.
        #[serde(default)]
        off_origin: bool,
    },
    /// Freeze one worksheet band from the leading row or column.
    FreezeLeadingBand {
        workbook: WorkbookId,
        at: Key,
        band: FreezeBand,
        off_origin: bool,
    },
    CopyCells { workbook: WorkbookId, copy: u64, from: Area },
    /// Pin the current publication for these page bindings without editing the workbook.
    PinBaseline { expected: Stamp, bounds: Vec<Bound>, budget: Budget },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    Selection {
        selection: Selection,
        cause: SelectionCause,
    },
    History(HistoryEntry),
    TryOn(LookOffer),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Update {
    Ready {
        watch: WatchId,
        coverage: Coverage,
        body: Value,
    },
    Tiles {
        watch: WatchId,
        coverage: Coverage,
        barrier: crate::tiles::TileBarrier,
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
