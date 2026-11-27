//! Typed workbook queries.
//!
//! [`WorkbookQuery`] is what crosses the wire and [`Answer<Value>`] is what comes back. Neither is
//! what a view writes against. A view names a query type from this module and receives that
//! query's answer type: `session.watch(query::History)` yields `HistoryList`, never JSON to index
//! by field name. The host encodes each answer from the same type, so the two sides cannot drift.

use crate::grid::{Area, Key, Sheet};
use crate::{
    Answer, CellWindow, HistoryEntry, Interest, Refusal, SourceRef, WorkbookQuery, WorkbookSummary,
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
    /// The cells of an interest. Watched, it arrives as tiles.
    Cells { interest: Interest } => CellWindow, |q: &Cells| WorkbookQuery::Cells { interest: q.interest.clone() });
query!(
    Geometry { sheet: Sheet } => WorksheetGeometry, |q: &Geometry| WorkbookQuery::Geometry { sheet: q.sheet });
query!(
    Formula { at: Key } => FormulaFacts, |q: &Formula| WorkbookQuery::Formula { at: q.at });
query!(
    Rules { at: Key } => CellRules, |q: &Rules| WorkbookQuery::Rules { at: q.at });
query!(
    CalcState => CalcStatus, |_: &CalcState| WorkbookQuery::CalcState);
query!(
    /// The one history: what Ctrl+Z and Ctrl+Y will do next, newest first.
    History => HistoryList, |_: &History| WorkbookQuery::History);
query!(
    Resolve { address: String } => SourceRef, |q: &Resolve| WorkbookQuery::Resolve { address: q.address.clone() });

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Census {
    pub formulas: u64,
    pub literals: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormulaFacts {
    /// The cell's entry as the formula bar shows it.
    pub input: String,
    pub formula: bool,
    pub precedents: Vec<Area>,
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
        assert_eq!(status(CalcMode::Automatic, CalcActivity::Done, None).word(), "");
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
