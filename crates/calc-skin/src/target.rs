use crate::grid::{Area, Key, Sheet};
use calc_syntax::{AnchorId, DocId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ViewId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NameScope {
    Workbook,
    Sheet(Sheet),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TablePart {
    All,
    Data,
    Headers,
    Totals,
    Columns { first: String, last: String },
}

/// An Excel workbook entity. Session sheet identities can be checked against a host's summary.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SourceRef {
    Cells { sheet: Sheet, areas: Vec<Area> },
    Spill { anchor: Key },
    Name { scope: NameScope, name: String },
    Table { name: String, part: TablePart },
    Object { sheet: Sheet, name: String },
    Sheet(Sheet),
    Workbook,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ViewItemRef {
    pub view: ViewId,
    pub doc: DocId,
    pub item: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Target {
    Source(SourceRef),
    Anchor(AnchorId),
    Item(ViewItemRef),
}

/// Both identities must resolve to the same source before a binding can write.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bound {
    pub anchor: AnchorId,
    pub address: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub seq: u64,
    pub target: SourceRef,
    pub active: Option<Key>,
    pub grouped: Vec<Sheet>,
    pub item: Option<ViewItemRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectionCause {
    ClientEcho { client_seq: u64 },
    Macro { procedure: String },
    Agent { agent: String },
    History,
    Open,
}
