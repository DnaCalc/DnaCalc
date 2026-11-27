//! Native Excel Scenario Manager records.

use calc_value::{CellKey, ExcelText};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ScenarioInput {
    pub at: CellKey,
    /// The original ST_Xstring value, decoded without interpreting it as cell content.
    pub value: ExcelText,
    pub number_format: u32,
    pub deleted: bool,
    pub undone: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WorksheetScenario {
    pub name: String,
    pub hidden: bool,
    pub locked: bool,
    pub inputs: Vec<ScenarioInput>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct WorksheetScenarios {
    pub entries: Vec<WorksheetScenario>,
    /// Records preserved by the package but unavailable for applying.
    pub unread: usize,
}
