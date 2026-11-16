//! The links a legacy workbook's formulas name (EXTERNSHEET, SUPBOOK, EXTERNNAME, NAME), gathered
//! into the context the xlsb token printer prints BIFF8 tokens against.

use super::workbook::{Globals, SupBookRecord};
use crate::xlsb::formula::{Context, Layout, NameRef, SupBook, TableRef, Xti};
use std::collections::BTreeMap;

pub(crate) struct Links {
    sheets: Vec<String>,
    xti: Vec<Xti>,
    supbooks: Vec<SupBook>,
    names: Vec<NameRef>,
    tables: BTreeMap<u32, TableRef>,
}

impl Links {
    /// The links of `globals`; `sheets` are the workbook's sheet names in tab order.
    pub(crate) fn new(globals: &Globals, sheets: Vec<String>) -> Links {
        let mut external = 0u32;
        let supbooks = globals
            .supbooks
            .iter()
            .map(|book| match book {
                SupBookRecord::This => SupBook::This,
                SupBookRecord::AddIn { names } => SupBook::AddIn {
                    names: names.clone(),
                },
                SupBookRecord::External { sheets, names, .. } => {
                    external += 1;
                    SupBook::External {
                        number: external,
                        sheets: sheets.clone(),
                        names: names.clone(),
                    }
                }
            })
            .collect();
        Links {
            sheets,
            xti: globals.xti.clone(),
            supbooks,
            names: globals
                .names
                .iter()
                .map(|name| NameRef {
                    name: name.name.clone(),
                    sheet: name.scope,
                })
                .collect(),
            tables: BTreeMap::new(),
        }
    }

    /// The context for a formula on the sheet at tab index `tab`, or in a workbook-level name.
    pub(crate) fn context(&self, tab: Option<u32>) -> Context<'_> {
        Context {
            layout: Layout::Biff8,
            sheets: &self.sheets,
            xti: &self.xti,
            supbooks: &self.supbooks,
            names: &self.names,
            tables: &self.tables,
            sheet: tab,
        }
    }
}
