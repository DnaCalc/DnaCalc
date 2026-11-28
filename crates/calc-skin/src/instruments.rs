//! Workbook inspection records shared by the host and its views.

use crate::grid::{Area, Key, Sheet};
use crate::{NameScope, RawValue, SourceRef};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InventoryKind {
    Sheets,
    Names,
    Tables,
    Charts,
    Shapes,
    Scenarios,
    PivotTables,
    ExternalLinks,
    VbaProject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SheetVisibility {
    Visible,
    Hidden,
    VeryHidden,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableInfo {
    pub name: String,
    pub area: Area,
    pub headers: u32,
    pub totals: u32,
    pub columns: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotFreshness {
    Current,
    Stale,
    Unassessed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PivotCacheFacts {
    pub name: String,
    pub source: Option<Area>,
    pub saved_refresh: Option<String>,
    pub observed_refresh_unix_ms: Option<u64>,
    pub freshness: PivotFreshness,
    pub unavailable: Option<String>,
}

/// A worksheet chart drawn by the workbook's chart renderer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartImage {
    pub label: String,
    pub data_url: String,
    pub uses_saved_values: bool,
    #[serde(default)]
    pub compares_baseline: bool,
}

/// A named Scenario Manager record on one Excel worksheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioRef {
    pub sheet: Sheet,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalLink {
    pub index: u32,
    pub file_name: Option<String>,
    pub cached_sheets: u64,
    pub refresh_error_sheets: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InventoryItem {
    Sheet {
        sheet: Sheet,
        name: String,
        visibility: SheetVisibility,
        worksheet: bool,
    },
    Name {
        scope: NameScope,
        name: String,
        formula: String,
        hidden: bool,
    },
    Table(TableInfo),
    Chart {
        object: SourceRef,
    },
    Shape {
        object: SourceRef,
    },
    Scenario(ScenarioRef),
    PivotTable {
        name: String,
        area: Area,
    },
    ExternalLink(ExternalLink),
    VbaProject,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Structure {
    pub tables: Vec<TableInfo>,
    pub runs: Vec<FormulaRun>,
}

/// Authored Excel evidence that a worksheet cell is intended for entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputCandidate {
    pub at: Key,
    pub label: Option<String>,
    pub signal: InputSignal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputSignal {
    ValidationList,
    UnlockedOnProtectedSheet,
    LinkedFormControl,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormulaRun {
    pub extent: Area,
    pub r1c1: String,
    /// Exact membership, in worksheet order, omitting exceptions.
    pub members: Vec<Area>,
    pub exceptions: Vec<Key>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sources {
    pub links: Vec<ExternalLink>,
    pub topics: Vec<LiveTopic>,
    pub volatile_cells: Vec<Key>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LiveTopic {
    pub progid: String,
    pub server: String,
    pub strings: Vec<String>,
    pub lease: u64,
    /// Latest sequence admitted by the owner for this lease.
    pub sequence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<LiveTopicReceipt>,
    pub last: crate::Shown<crate::RawValue>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveTopicReceipt {
    pub received: u64,
    pub timed_received: u64,
    pub first_unix_ms: Option<u64>,
    pub last_unix_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyDirection {
    Precedents,
    Dependents,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyGrouping {
    Reference,
    Sheet,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DependencyTarget {
    Source(SourceRef),
    SheetSpan { area: Area, last: Sheet },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DependencyKind {
    Placed,
    Dynamic,
    Name,
    Member,
    Reader,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DependencyEdge {
    /// Index in `Dependencies::nodes`.
    pub from: usize,
    /// Index in `Dependencies::nodes`.
    pub to: usize,
    pub kind: DependencyKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencySheetGroup {
    pub sheet: Sheet,
    /// Indexes of the graph nodes that belong to this sheet.
    pub members: Vec<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependencies {
    pub roots: Vec<usize>,
    pub nodes: Vec<DependencyTarget>,
    pub edges: Vec<DependencyEdge>,
    /// Authored table and spill operands behind bound graph edges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<DependencyReference>,
    pub sheets: Vec<DependencySheetGroup>,
    /// Formula cells for which the host has no bound edges yet.
    pub unbound: Vec<Key>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyReference {
    pub edge: DependencyEdge,
    pub spelling: String,
    /// The cells of a structured reference or the anchor of a spill.
    pub source: SourceRef,
    /// A spill operand's last published extent. The graph can also contain its dynamic read.
    pub published_extent: Option<Area>,
}

/// A sheet-tier summary of the workbook's bound dependency graph.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyAtlas {
    pub sheets: Vec<DependencyAtlasSheet>,
    pub links: Vec<DependencyAtlasLink>,
    /// Name edges with no fixed worksheet in the retained binding.
    pub unattributed_names: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyAtlasSheet {
    pub sheet: Sheet,
    pub formulas: u64,
    pub unbound: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyAtlasLink {
    pub consumer: Sheet,
    pub precedent: Sheet,
    /// Distinct formulas in the consumer that read the precedent.
    pub readers: u64,
    pub placed: u64,
    pub dynamic: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CellComparison {
    pub left: crate::Shown<crate::RawValue>,
    pub right: crate::Shown<crate::RawValue>,
    /// Binary64 left minus right when both values are current numbers.
    pub difference: Option<u64>,
    pub common: usize,
    pub left_only: Vec<ComparisonRead>,
    pub right_only: Vec<ComparisonRead>,
    pub unbound: Vec<Key>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ComparisonRead {
    pub target: DependencyTarget,
    pub kind: DependencyKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issues {
    /// Current errors in the scope.
    pub errors: Vec<ErrorIssue>,
    /// Counted by inferred root, with one count per other affected cell in the scope.
    pub roots: Vec<ErrorRoot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorIssue {
    pub at: Key,
    pub code: String,
    pub origin: ErrorOrigin,
    /// Endpoint when the producing calculation proves a direct error chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact_root: Option<Key>,
    /// Definition bodies that actually returned this error in the producing evaluation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub named_errors: Vec<NamedErrorEvidence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedErrorEvidence {
    pub name: String,
    pub scope: NameScope,
    pub formula: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorOrigin {
    Literal,
    /// No same-error read within the scope was found in the retained certificate.
    RootCandidate,
    /// A same-error read can explain this error. The certificate does not prove causality.
    CandidatePropagation {
        roots: Vec<Key>,
    },
    /// A complete chain could not be derived within the scope.
    Unresolved,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorRoot {
    pub at: Key,
    pub affected: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IterationStop {
    MaximumIterations,
    Converged,
    NoNumericSample,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IterationFacts {
    pub members: Vec<Key>,
    pub omitted_members: usize,
    pub passes: u32,
    pub residual_bits: Option<u64>,
    pub max_change_bits: u64,
    pub stop: IterationStop,
    pub reads: Vec<IterationRead>,
    pub reads_limited: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IterationRead {
    pub reader: Key,
    pub source: Key,
    pub current_pass: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Contributions {
    pub formula: Key,
    pub function: String,
    /// Where each inspected position is located. Aggregates with a target use that
    /// range. Counts use their first criteria range.
    pub source: Area,
    pub inspected: u64,
    pub matching_count: u64,
    /// Disjoint row or column spans of positions that pass every criterion.
    pub matching: Vec<Area>,
    /// Position chosen by a lookup when the calculation returned a match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<Area>,
    pub undetermined: u64,
    pub criteria: Vec<ContributionCriterion>,
    /// The result from the same detached inspection, over the host's current values.
    pub result: RawValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionCriterion {
    pub range: Area,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub matching: u64,
    /// Disjoint spans of positions whose only failed criterion was this one.
    pub failing_only_this: Vec<Area>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchoredNote {
    pub id: crate::NoteId,
    pub anchor: crate::AnchorId,
    pub text: String,
    /// The currently bound source. A deleted or orphaned anchor has no source.
    pub source: Option<SourceRef>,
}
