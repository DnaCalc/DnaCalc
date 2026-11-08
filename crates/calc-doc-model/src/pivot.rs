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
    pub group: Option<PivotCacheGroup>,
    pub items: Vec<PivotItem>,
    /// Shared-item indexes retained without any current source record.
    pub unused_items: Vec<u32>,
    pub attributes: BTreeMap<String, String>,
}
/// Grouping metadata retained from a cache field. Virtual date fields use their group items
/// as labels; source fields can carry a parent reference without a range.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PivotCacheGroup {
    pub attributes: BTreeMap<String, String>,
    pub range: Option<BTreeMap<String, String>>,
    /// For a discrete group, the group-item index for each source shared item.
    pub discrete: Option<Vec<u32>>,
    /// Labels for numeric groups on a source field, distinct from its original shared items.
    pub numeric_items: Option<Vec<PivotItem>>,
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
    pub page_fields: Vec<PivotPageField>,
    pub filters: Vec<PivotFilter>,
    pub data_fields: Vec<PivotDataField>,
    pub attributes: BTreeMap<String, String>,
    pub location: BTreeMap<String, String>,
    pub fields: Vec<PivotField>,
    pub row_items: Vec<PivotAxisItem>,
    pub col_items: Vec<PivotAxisItem>,
    pub unsupported: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PivotFilter {
    pub field: u32,
    pub measure: u32,
    pub kind: PivotFilterKind,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PivotFilterKind {
    TopCount(u32),
    ValueGreaterThan(String),
    Unsupported,
}
/// A report-filter field and its selected position in that field's item list.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PivotPageField {
    pub field: u32,
    pub selected_item: Option<u32>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PivotField {
    pub attributes: BTreeMap<String, String>,
    pub items: Vec<PivotFieldItem>,
    /// Data-field index selected by the supported value-sort scope.
    pub sort_measure: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PivotFieldItem {
    pub index: Option<u32>,
    pub kind: String,
    pub hidden: bool,
    /// A retained item that no longer occurs in the source records.
    pub missing: bool,
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
    /// A recognized extended display calculation preserved in the original table part.
    pub extended_show: Option<String>,
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

impl PivotItem {
    /// The Excel serial number of a cache date item (ISO 8601 date and optional time, 1900 date
    /// system, from 1900-03-01), so dates order and compare with the numbers a source holds.
    pub fn date_serial(&self) -> Option<f64> {
        let PivotItem::Date(text) = self else {
            return None;
        };
        let (date, time) = text.split_once('T').unwrap_or((text.as_str(), "00:00:00"));
        let mut d = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
        let (y, m, day) = (d.next()??, d.next()??, d.next()??);
        if !(1..=12).contains(&m) || !(1..=31).contains(&day) || y < 1900 {
            return None;
        }
        let mut t = time.trim_end_matches('Z').splitn(3, ':');
        let hours = t.next()?.parse::<f64>().ok()?;
        let minutes = t.next().map_or(Some(0.), |p| p.parse::<f64>().ok())?;
        let seconds = t.next().map_or(Some(0.), |p| p.parse::<f64>().ok())?;
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + day - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468 + 25_569;
        if days < 61 {
            return None;
        }
        Some(days as f64 + (hours * 3600. + minutes * 60. + seconds) / 86_400.)
    }
}
