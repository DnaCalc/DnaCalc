//! Worksheet AutoFilter criteria and the rows excluded by the last explicit application.
use crate::value::GridArea;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FilterCriteria {
    /// SpreadsheetML filter values, before display clipping; an empty value is distinct from blank.
    Values { values: Vec<String>, blank: bool },
    Custom {
        and: bool,
        predicates: Vec<FilterPredicate>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FilterPredicate {
    pub operator: FilterOperator,
    pub value: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterOperator {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}
impl FilterOperator {
    pub fn xml(self) -> &'static str {
        match self {
            Self::Equal => "equal",
            Self::NotEqual => "notEqual",
            Self::Less => "lessThan",
            Self::LessEqual => "lessThanOrEqual",
            Self::Greater => "greaterThan",
            Self::GreaterEqual => "greaterThanOrEqual",
        }
    }
    pub fn from_xml(s: &str) -> Option<Self> {
        Some(match s {
            "equal" => Self::Equal,
            "notEqual" => Self::NotEqual,
            "lessThan" => Self::Less,
            "lessThanOrEqual" => Self::LessEqual,
            "greaterThan" => Self::Greater,
            "greaterThanOrEqual" => Self::GreaterEqual,
            _ => return None,
        })
    }
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::NotEqual => "<>",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AutoFilter {
    pub area: GridArea,
    /// Absolute worksheet column numbers, independent of the selected cell.
    pub columns: BTreeMap<u32, FilterCriteria>,
    pub filtered_rows: BTreeSet<u32>,
    /// Unknown criteria/source attributes stay in the package and prevent destructive editing.
    pub unsupported: bool,
}
impl AutoFilter {
    pub fn new(area: GridArea) -> Self {
        Self {
            area,
            columns: BTreeMap::new(),
            filtered_rows: BTreeSet::new(),
            unsupported: false,
        }
    }
    pub fn valid(&self) -> bool {
        self.columns.iter().all(|(col, criteria)| {
            (self.area.first().column()..=self.area.last().column()).contains(col)
                && match criteria {
                    FilterCriteria::Values { values, .. } => {
                        values.len() <= 10000
                            && values.iter().all(|v| v.encode_utf16().count() <= 32767)
                    }
                    FilterCriteria::Custom { predicates, .. } => {
                        !predicates.is_empty()
                            && predicates.len() <= 2
                            && predicates
                                .iter()
                                .all(|p| p.value.encode_utf16().count() <= 32767)
                    }
                }
        }) && self
            .filtered_rows
            .iter()
            .all(|r| *r > self.area.first().row() && *r <= self.area.last().row())
    }
}
