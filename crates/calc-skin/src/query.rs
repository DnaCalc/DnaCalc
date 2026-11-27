//! Typed workbook queries.
//!
//! [`WorkbookQuery`] is what crosses the wire and [`Answer<Value>`] is what comes back. Neither is
//! what a view writes against. A view names a query type from this module and receives that
//! query's answer type: `session.watch(query::History)` yields `HistoryList`, never JSON to index
//! by field name. The host encodes each answer from the same type, so the two sides cannot drift.

use crate::grid::{Area, Key, Sheet};
use crate::{
    Answer, CellWindow, Contributions, Dependencies, DependencyDirection, DependencyGrouping,
    Evidence, HistoryEntry, Interest, InventoryItem, InventoryKind, Issues,
    Placement as PlacementAnswer, PlacementKind, PlacementShape, Refusal, RowFilter, RowPage,
    RowSort, RowWindow, SourceRef, Sources, Structure, WorkbookQuery, WorkbookSummary,
    WorksheetGeometry,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

pub trait Query: Clone + PartialEq + 'static {
    type Answer: Clone + PartialEq + Serialize + DeserializeOwned + 'static;

    fn request(&self) -> WorkbookQuery;

    /// Encodes an answer body for the wire. The host calls this and nothing else to build a body.
    fn encode(answer: &Self::Answer) -> Result<Value, Refusal> {
        serde_json::to_value(answer).map_err(|error| Refusal::Transport(error.to_string()))
    }

    fn decode(answer: Answer<Value>) -> Result<Answer<Self::Answer>, Refusal> {
        let body = serde_json::from_value(answer.body)
            .map_err(|error| Refusal::Transport(format!("unexpected answer: {error}")))?;
        Ok(Answer {
            stamp: answer.stamp,
            coverage: answer.coverage,
            body,
        })
    }
}

macro_rules! query {
    ($(#[$doc:meta])* $name:ident => $answer:ty, $variant:expr) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name;
        query!(@imp $name, $answer, $variant);
    };
    ($(#[$doc:meta])* $name:ident { $($field:ident : $ty:ty),* } => $answer:ty, $variant:expr) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name { $(pub $field: $ty),* }
        query!(@imp $name, $answer, $variant);
    };
    (@imp $name:ident, $answer:ty, $variant:expr) => {
        impl Query for $name {
            type Answer = $answer;

            fn request(&self) -> WorkbookQuery {
                let build: fn(&$name) -> WorkbookQuery = $variant;
                build(self)
            }
        }
    };
}

query!(
    /// The workbook's name, state, sheets and what this host can do.
    Summary => WorkbookSummary, |_: &Summary| WorkbookQuery::Summary);
query!(
    /// Counts taken without calculating; what `View::fit` decides from.
    CensusOf { scope: SourceRef } => Census, |q: &CensusOf| WorkbookQuery::Census { scope: q.scope.clone() });
query!(
    /// Authored workbook records. An empty kind list includes every supported kind.
    Inventory { kinds: Vec<InventoryKind> } => Vec<InventoryItem>, |q: &Inventory| WorkbookQuery::Inventory { kinds: q.kinds.clone() });
query!(
    /// Tables and inferred formula runs in the current model.
    StructureOf { scope: SourceRef } => Evidence<Structure>, |q: &StructureOf| WorkbookQuery::Structure { scope: q.scope.clone() });
query!(
    /// Certain signals of worksheet inputs, including workbook validation and protection.
    InputCandidates { scope: SourceRef } => Vec<crate::InputCandidate>,
    |q: &InputCandidates| WorkbookQuery::InputCandidates { scope: q.scope.clone() }
);
query!(
    /// Bound formulas with no readers in the current dependency graph.
    OutputCandidates { scope: SourceRef } => Vec<Key>,
    |q: &OutputCandidates| WorkbookQuery::OutputCandidates { scope: q.scope.clone() }
);
query!(
    /// Workbook link caches, admitted RTD topics and bound volatile cells.
    SourceInventory => Sources, |_: &SourceInventory| WorkbookQuery::Sources);
query!(
/// Edges of the bound workbook graph in the requested direction.
Deps { of: SourceRef, dir: DependencyDirection, depth: u16, group: DependencyGrouping }
    => Evidence<Dependencies>, |q: &Deps| WorkbookQuery::Deps {
        of: q.of.clone(), dir: q.dir, depth: q.depth, group: q.group,
    });
query!(
    /// Current worksheet errors and possible roots in the requested scope.
    IssuesIn { scope: SourceRef } => Evidence<Issues>, |q: &IssuesIn| WorkbookQuery::Issues { scope: q.scope.clone() });
query!(
    /// Conditional-aggregate criteria over the whole source, from the current calculation kernel.
    ContributionsAt { at: Key } => Evidence<Contributions>, |q: &ContributionsAt| WorkbookQuery::Contributions { at: q.at });
query!(
    /// The cells of an interest. Watched, it arrives as tiles.
    Cells { interest: Interest } => CellWindow, |q: &Cells| WorkbookQuery::Cells { interest: q.interest.clone() });
query!(
/// A view window over a rectangular worksheet source and its exact matched row count.
Rows { source: SourceRef, filter: Vec<RowFilter>, sort: Vec<RowSort>, window: RowWindow }
    => RowPage, |q: &Rows| WorkbookQuery::Rows {
        source: q.source.clone(),
        filter: q.filter.clone(),
        sort: q.sort.clone(),
        window: q.window,
    });
query!(
/// A proposed backing cell and its read footprint.
Placement { sheet: Sheet, kind: PlacementKind, shape: Option<PlacementShape>, reserved: Vec<Area> }
    => PlacementAnswer, |q: &Placement| WorkbookQuery::Placement {
        sheet: q.sheet,
        kind: q.kind,
        shape: q.shape,
        reserved: q.reserved.clone(),
    });
query!(
    Geometry { sheet: Sheet } => WorksheetGeometry, |q: &Geometry| WorkbookQuery::Geometry { sheet: q.sheet });
query!(
    Formula { at: Key } => FormulaFacts, |q: &Formula| WorkbookQuery::Formula { at: q.at });
query!(
    /// A detached Formula Explorer trial captured at one published workbook generation.
    TrialFor { at: Key } => crate::Trial, |q: &TrialFor| WorkbookQuery::TrialFor { at: q.at });
query!(
    /// A structural view of the authored formula's expressions, in its workbook context.
    ExpressionTree { at: Key } => Evidence<crate::parts::Trace>, |q: &ExpressionTree| WorkbookQuery::ExpressionTree { at: q.at });
query!(
    Rules { at: Key } => CellRules, |q: &Rules| WorkbookQuery::Rules { at: q.at });
query!(
    CalcState => CalcStatus, |_: &CalcState| WorkbookQuery::CalcState);
query!(
    /// Observed formula-result differences in the latest publication.
    CalcChanges => Evidence<Vec<FormulaResultChange>>, |_: &CalcChanges| WorkbookQuery::CalcChanges);
query!(
    /// Authored notes, optionally limited to a single anchor.
    NotesOf { anchor: Option<crate::AnchorId> } => Vec<crate::AnchoredNote>, |q: &NotesOf| WorkbookQuery::Notes { anchor: q.anchor });
query!(
    /// The workbook's view part: whether this build can read it, and where each anchor is now.
    ViewPartOf => ViewPartFacts, |_: &ViewPartOf| WorkbookQuery::ViewPart);
query!(
    /// The pages a source prints on.
    PrintPlanOf { source: SourceRef } => crate::print::PrintPlan, |q: &PrintPlanOf| WorkbookQuery::PrintPlan { source: q.source.clone() });
query!(
    /// The one history: what Ctrl+Z and Ctrl+Y will do next, newest first.
    History => HistoryList, |_: &History| WorkbookQuery::History);
query!(
    Resolve { address: String } => SourceRef, |q: &Resolve| WorkbookQuery::Resolve { address: q.address.clone() });
query!(
    /// Resolve both saved identities of a binding against the current workbook.
    ResolveBound { bound: crate::Bound } => SourceRef, |q: &ResolveBound| WorkbookQuery::ResolveBound { bound: q.bound.clone() });

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Census {
    pub formulas: u64,
    pub literals: u64,
    pub sheets: Vec<SheetCensus>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormulaResultChange {
    pub at: Key,
    pub before: FormulaObservation,
    pub after: FormulaObservation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormulaObservation {
    pub value: Option<crate::RawValue>,
    pub origin: ResultOrigin,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultOrigin {
    None,
    AsSaved,
    Calculated { tick: u64 },
    ExternalCache { tick: u64 },
    CircularHold { tick: u64 },
    Refused { tick: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SheetCensus {
    pub sheet: Sheet,
    pub formulas: u64,
    pub literals: u64,
    /// Bounds of the counted cells inside the requested scope.
    pub used: Option<Area>,
    /// Current errors by Excel spelling. Saved, stale and refused results are excluded.
    pub errors: std::collections::BTreeMap<String, u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewPartFacts {
    /// The part was written by a later version: it is kept byte for byte and not read.
    pub opaque: bool,
    /// The author's request, subject to the named view's opening criteria.
    #[serde(default)]
    pub preferred: Option<crate::ViewId>,
    pub anchors: Vec<AnchorFacts>,
    #[serde(default)]
    pub documents: Vec<ViewDocumentFacts>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewDocumentFacts {
    pub id: crate::DocId,
    pub view: crate::ViewId,
    pub sheet: Option<Sheet>,
    pub schema: u32,
    pub body: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorFacts {
    pub id: crate::AnchorId,
    /// What the anchor is bound to now; `None` when its source no longer exists.
    pub source: Option<SourceRef>,
    /// The first area of that source, when it is on a sheet.
    pub area: Option<Area>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormulaFacts {
    /// The cell's entry as the formula bar shows it.
    pub input: String,
    pub formula: bool,
    pub precedents: Vec<Area>,
    /// The exact entry used as the precondition for a formula rewrite.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authored: Option<String>,
    /// A historical observation from the opened file, independent of the current result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_saved: Option<AsSavedFormula>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsSavedFormula {
    pub entry: String,
    pub value: crate::RawValue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellRules {
    /// Why the cell cannot be edited now, if it cannot.
    pub refusal: Option<Refusal>,
    pub validation: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalcMode {
    Automatic,
    AutomaticExceptTables,
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalcActivity {
    /// Every formula is current.
    Done,
    /// Formulas are waiting: in Manual mode until the person calculates, otherwise until the
    /// running pass ends.
    Pending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalcStatus {
    pub mode: CalcMode,
    pub activity: CalcActivity,
    /// The cell Excel names for a circular reference, when one is reported.
    pub circular: Option<Key>,
}

impl CalcStatus {
    /// The word the Strip shows, as Excel's status bar does; empty when there is nothing to say.
    pub fn word(&self) -> &'static str {
        match (self.activity, self.mode, self.circular) {
            (CalcActivity::Pending, CalcMode::Manual, _) => "Calculate",
            (CalcActivity::Pending, _, _) => "Calculating",
            (CalcActivity::Done, _, Some(_)) => "Circular reference",
            (CalcActivity::Done, _, None) => "",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryList {
    /// Steps Ctrl+Z takes back, the next one first.
    pub undo: Vec<HistoryEntry>,
    /// Steps Ctrl+Y commits again, the next one first.
    pub redo: Vec<HistoryEntry>,
    /// The depth of each stack. It can exceed the entries listed: a step made outside the skin
    /// interface is undone in order and carries no entry.
    pub undo_depth: u64,
    pub redo_depth: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CalcGeneration, Coverage, Revision, Stamp, WorkbookId};
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test;

    fn stamp() -> Stamp {
        Stamp {
            workbook: WorkbookId(1),
            model: Revision(2),
            view: Revision(3),
            calc: CalcGeneration(4),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn an_answer_decodes_to_its_query_type_or_refuses() {
        let status = CalcStatus {
            mode: CalcMode::Manual,
            activity: CalcActivity::Pending,
            circular: None,
        };
        let wire = Answer {
            stamp: stamp(),
            coverage: Coverage::Complete,
            body: CalcState::encode(&status).unwrap(),
        };
        let typed = CalcState::decode(wire.clone()).unwrap();
        assert_eq!((typed.stamp, typed.body), (stamp(), status));
        assert_eq!(typed.body.word(), "Calculate");
        // The same bytes are not a history.
        assert!(matches!(History::decode(wire), Err(Refusal::Transport(_))));
        assert_eq!(CalcState.request(), WorkbookQuery::CalcState);
        assert_eq!(
            Resolve {
                address: "Sheet1!A1".into()
            }
            .request(),
            WorkbookQuery::Resolve {
                address: "Sheet1!A1".into()
            }
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test)]
    fn the_status_word_follows_excel() {
        let status = |mode, activity, circular| CalcStatus {
            mode,
            activity,
            circular,
        };
        let cell = Some(Key(0, 0, 1, 1));
        assert_eq!(
            status(CalcMode::Automatic, CalcActivity::Done, None).word(),
            ""
        );
        assert_eq!(
            status(CalcMode::Automatic, CalcActivity::Pending, cell).word(),
            "Calculating"
        );
        assert_eq!(
            status(CalcMode::Automatic, CalcActivity::Done, cell).word(),
            "Circular reference"
        );
    }
}
