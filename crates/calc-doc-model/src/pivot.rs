//! Pivot definitions and source-cache snapshots. Computed output belongs to CalcEngine.
use calc_value::{GridArea, NumericMode, SheetId};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PivotIssue {
    pub part: String,
    pub detail: String,
    pub owner: &'static str,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PivotCaches {
    pub caches: BTreeMap<u32, Arc<PivotCache>>,
    pub issues: Vec<PivotIssue>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PivotState {
    pub caches: PivotCaches,
    pub sheets: BTreeMap<SheetId, SheetPivots>,
}
impl PivotState {
    pub fn tables(&self) -> impl Iterator<Item = &PivotTable> {
        self.sheets.values().flat_map(|s| s.tables.iter())
    }
    pub fn sheet(&self, sheet: SheetId) -> Option<&SheetPivots> {
        self.sheets.get(&sheet)
    }
    pub fn contains(&self, at: calc_value::CellKey) -> bool {
        self.tables().any(|t| t.area.contains(at))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PivotCache {
    pub id: u32,
    pub part: String,
    pub records_part: Option<String>,
    pub record_count: Option<u64>,
    pub source_type: String,
    pub source_sheet: Option<String>,
    pub source_range: Option<String>,
    pub source_name: Option<String>,
    pub fields: Vec<PivotCacheField>,
    pub attributes: BTreeMap<String, String>,
    pub unsupported: Vec<String>,
    /// Source readings taken by an explicit refresh, never aggregated answers.
    pub snapshot: Option<Arc<PivotSnapshot>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PivotSnapshot {
    pub records: Vec<Vec<PivotItem>>,
    pub source_revision: u64,
    pub numeric: NumericMode,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PivotCacheField {
    pub name: String,
    pub shared_items: Option<u32>,
    pub items: Vec<PivotItem>,
    pub attributes: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SheetPivots {
    pub tables: Vec<PivotTable>,
    pub issues: Vec<PivotIssue>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PivotTable {
    pub name: String,
    pub part: String,
    pub cache_id: u32,
    pub area: GridArea,
    pub row_fields: Vec<i32>,
    pub column_fields: Vec<i32>,
    pub page_fields: Vec<u32>,
    pub data_fields: Vec<PivotDataField>,
    pub attributes: BTreeMap<String, String>,
    pub location: BTreeMap<String, String>,
    pub fields: Vec<PivotField>,
    pub row_items: Vec<PivotAxisItem>,
    pub unsupported: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PivotField {
    pub attributes: BTreeMap<String, String>,
    pub items: Vec<PivotFieldItem>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PivotFieldItem {
    pub index: Option<u32>,
    pub kind: String,
    pub hidden: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PivotAxisItem {
    pub kind: String,
    pub repeat: u32,
    pub data_index: u32,
    pub indexes: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PivotDataField {
    pub name: Option<String>,
    pub field: u32,
    pub subtotal: String,
    pub attributes: BTreeMap<String, String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PivotItem {
    Missing,
    Number(String),
    Text(String),
    Boolean(bool),
    Date(String),
    Error(String),
    Index(u32),
    Other {
        namespace: Option<String>,
        element: String,
        attributes: BTreeMap<String, String>,
    },
}
