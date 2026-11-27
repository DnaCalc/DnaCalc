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

/// A worksheet chart drawn by the workbook's chart renderer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartImage {
    pub label: String,
    pub data_url: String,
    pub uses_saved_values: bool,
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
    pub last: crate::Shown<crate::RawValue>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
    pub sheets: Vec<DependencySheetGroup>,
    /// Formula cells for which the host has no bound edges yet.
    pub unbound: Vec<Key>,
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
    pub undetermined: u64,
    pub criteria: Vec<ContributionCriterion>,
    /// The result from the same detached inspection, over the host's current values.
    pub result: RawValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionCriterion {
    pub range: Area,
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
