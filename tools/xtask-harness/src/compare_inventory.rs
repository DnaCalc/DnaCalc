//! Source inventories and saved-cache classifications. None of these reads is
//! a calculated result or a substitute for a live Excel observation.
use calc_host::{ArrayKind, CellInput, HostOptions, HostState, MemorySource, OpenPolicy};
use calc_verdict::{
    Acquisition, CaptureRecord, CellAddress, FormulaCell, FormulaKind, Observation, SheetEntry,
    SheetVisibility, compare, sha256_hex,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug)]
pub(crate) struct InventoryFailure {
    pub(crate) key: String,
    pub(crate) message: String,
}
impl std::fmt::Display for InventoryFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl From<String> for InventoryFailure {
    fn from(message: String) -> Self {
        Self {
            key: "SourceInventory".into(),
            message,
        }
    }
}
impl From<&str> for InventoryFailure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

pub(crate) fn source_record(bytes: &[u8], name: &str) -> Result<CaptureRecord, InventoryFailure> {
    let host = HostState::open(
        Arc::new(MemorySource::new(bytes.to_vec()).named(name)),
        &OpenPolicy::default(),
        HostOptions {
            formula_cell_budget: None,
            ..HostOptions::default()
        },
    )
    .map_err(|e| {
        let mut key = super::compare::open_key(&e);
        if let calc_host::OpenFailure::Workbook(calc_host::LoadError::Cell {
            error: calc_host::CellError::Malformed { detail },
            ..
        }) = &e
        {
            key.push(':');
            key.push_str(detail);
        }
        InventoryFailure {
            key,
            message: e.to_string(),
        }
    })?;
    let mut formulas = BTreeMap::new();
    let mut sheets = Vec::new();
    for sheet in host.sheets() {
        let name = sheet.name().text().to_string_lossy();
        sheets.push(SheetEntry {
            name: name.clone(),
            visibility: match sheet.visibility() {
                calc_host::SheetVisibility::Visible => SheetVisibility::Visible,
                calc_host::SheetVisibility::Hidden => SheetVisibility::Hidden,
                calc_host::SheetVisibility::VeryHidden => SheetVisibility::VeryHidden,
            },
        });
        let loaded = host
            .loaded()
            .sheets()
            .iter()
            .find(|s| s.id() == sheet.id())
            .ok_or("missing loaded sheet")?;
        for (row, column, _) in loaded.formulas() {
            let cell =
                CellAddress::new(&name, row, column).ok_or("invalid source formula coordinate")?;
            let at = calc_host::value::CellKey::new(sheet.id(), row, column)
                .map_err(|e| e.to_string())?;
            let formula = match host.model().cell(at) {
                Some(CellInput::Formula(text)) => Some(text.text().to_string_lossy()),
                _ => None,
            };
            formulas.insert(
                cell.clone(),
                FormulaCell {
                    cell,
                    kind: FormulaKind::Ordinary,
                    formula,
                    dynamic_array: false,
                },
            );
        }
        // Unsupported macro/table formula cells remain in the denominator even
        // though their authored inputs deliberately contain no executable formula.
        for (at, unloaded) in host
            .model()
            .definitions()
            .unloaded_cells()
            .filter(|(at, _)| at.sheet() == sheet.id())
        {
            if matches!(
                unloaded,
                calc_host::UnloadedCell::MacroSheet | calc_host::UnloadedCell::DataTable
            ) {
                let cell = CellAddress::new(&name, at.row(), at.column())
                    .ok_or("invalid unloaded formula coordinate")?;
                formulas.entry(cell.clone()).or_insert(FormulaCell {
                    cell,
                    kind: FormulaKind::Ordinary,
                    formula: None,
                    dynamic_array: false,
                });
            }
        }
        for array in host.model().definitions().arrays().filter(|array| {
            array.anchor().sheet() == sheet.id() && array.kind() == ArrayKind::Legacy
        }) {
            let first = array.anchor();
            let last = array.area().last();
            for row in first.row()..=last.row() {
                for column in first.column()..=last.column() {
                    let cell =
                        CellAddress::new(&name, row, column).ok_or("invalid array member")?;
                    let kind = if row == first.row() && column == first.column() {
                        FormulaKind::LegacyArrayAnchor {
                            rows: last.row() - row + 1,
                            columns: last.column() - column + 1,
                        }
                    } else {
                        FormulaKind::LegacyArrayMember {
                            anchor_row: first.row(),
                            anchor_column: first.column(),
                        }
                    };
                    formulas
                        .entry(cell.clone())
                        .or_insert_with(|| FormulaCell {
                            cell,
                            kind,
                            formula: None,
                            dynamic_array: false,
                        })
                        .kind = kind;
                }
            }
        }
        for table in host
            .model()
            .definitions()
            .data_tables()
            .iter()
            .filter(|t| t.area().sheet() == sheet.id())
        {
            let first = table.area().first();
            let last = table.area().last();
            for row in first.row()..=last.row() {
                for column in first.column()..=last.column() {
                    let cell =
                        CellAddress::new(&name, row, column).ok_or("invalid table member")?;
                    let kind = if row == first.row() && column == first.column() {
                        FormulaKind::DataTableAnchor {
                            rows: last.row() - row + 1,
                            columns: last.column() - column + 1,
                        }
                    } else {
                        FormulaKind::DataTableMember {
                            anchor_row: first.row(),
                            anchor_column: first.column(),
                        }
                    };
                    formulas
                        .entry(cell.clone())
                        .or_insert_with(|| FormulaCell {
                            cell,
                            kind,
                            formula: None,
                            dynamic_array: false,
                        })
                        .kind = kind;
                }
            }
        }
    }
    let source = format!("sha256:{}", sha256_hex(bytes));
    let session = calc_verdict::CaptureSession {
        revision: 0,
        operations: Vec::new(),
        context: super::compare::run_context(&host.recalc_context()),
        tool: "dnacalc-source-inventory/1; no Excel execution or observations".into(),
        input_package: source.clone(),
        output_package: None,
        failures: Vec::new(),
        shared_ranges: Vec::new(),
        saved: Vec::new(),
        reopened: Vec::new(),
    };
    Ok(CaptureRecord {
        case: name.into(),
        source,
        acquisition: Acquisition::FileCacheReplay,
        // This is a DNA-only comparison policy, not a statement about an Excel
        // session. File projects and their open/event handlers are never run.
        environment: Some(calc_verdict::ExcelEnvironment {
            target: calc_host::value::NumericTargetId::new(
                "unrecorded", "unrecorded", "unrecorded", "unrecorded",
            ).expect("nonempty inventory labels"),
            settings: vec![
                ("automation_security".into(), "3".into()),
                ("enable_events".into(), "false".into()),
                ("comparison_policy".into(), "opened-with-macros-disabled".into()),
            ],
        }),
        sheets,
        formulas: formulas.into_values().collect(),
        observations: Vec::new(),
        session: Some(session),
    })
}

/// Compare the input package's cache with Excel's own live recalculation, never
/// with DNA's answer. Drift stays classified alongside the ordinary verdict.
pub(crate) fn saved_caches(host: &HostState, capture: &CaptureRecord) -> Value {
    let observations: BTreeMap<_, _> = capture
        .observations
        .iter()
        .map(|o| (&o.cell, &o.observation))
        .collect();
    let mut counts = BTreeMap::<String, u64>::new();
    let mut cells = Vec::new();
    for formula in &capture.formulas {
        let address = &formula.cell;
        let sheet = host
            .sheets()
            .iter()
            .find(|s| s.name().text().to_string_lossy() == address.sheet())
            .and_then(|sheet| host.loaded().sheets().iter().find(|s| s.id() == sheet.id()));
        let saved = sheet
            .and_then(|s| {
                s.formula(address.row(), address.column())
                    .and_then(|r| r.cache())
                    .or_else(|| s.value_cache(address.row(), address.column()))
            })
            .map(|v| v.value().to_value());
        let raw = observations.get(address).and_then(|o| match o {
            Observation::ExcelRaw(value) => Some(value),
            _ => None,
        });
        let class = match (&saved, raw) {
            (_, None) => "no-live-excel-observation",
            (None, Some(_)) => "source-cache-missing",
            (Some(_), Some(value)) if value.as_array().is_some() => {
                "array-cache-shape-unclassified"
            }
            (Some(saved), Some(raw)) if compare(raw, saved).is_none() => "same",
            _ => "different",
        };
        *counts.entry(class.into()).or_default() += 1;
        cells.push(
            json!({"sheet":address.sheet(),"row":address.row(),"column":address.column(),
            "class":class,"source_cached":saved.as_ref().map(super::compare_triage::value_json),
            "excel_recalculated":raw.map(super::compare_triage::value_json)}),
        );
    }
    json!({"format":"dnacalc-source-cache-comparison","version":1,
        "rule":"input package cache versus live Excel recalculation; differences remain in all verdict denominators; volatile/version/cache causes are not inferred",
        "source":capture.source,"counts":counts,"cells":cells})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_macro_formulas_stay_in_xml_and_binary_source_inventories() {
        for bytes in [
            include_bytes!("../../../crates/calc-doc/tests/fixtures/dc_xuw/macro-sheet.xlsm")
                .as_slice(),
            include_bytes!("../../../crates/calc-doc/tests/fixtures/dc_xuw/macro-sheet.xlsb")
                .as_slice(),
        ] {
            let host = HostState::open(
                Arc::new(MemorySource::new(bytes.to_vec())),
                &OpenPolicy::default(),
                HostOptions::default(),
            )
            .unwrap();
            let names: Vec<_> = host
                .loaded()
                .sheets()
                .iter()
                .filter(|s| s.is_macro_sheet())
                .filter_map(|s| host.model().sheet_by_id(s.id()))
                .map(|s| s.name().text().to_string_lossy())
                .collect();
            assert!(!names.is_empty());
            let inventory = source_record(bytes, "macro-source-control").unwrap();
            for name in names {
                assert!(
                    inventory.formulas.iter().any(|f| f.cell.sheet() == name),
                    "{name} formulas disappeared from the denominator"
                );
            }
            assert!(inventory.observations.is_empty());
        }
    }
    #[test]
    fn source_inventory_has_no_answers_and_includes_array_members() {
        let mut book = calc_host::HostSession::open(
            Arc::new(MemorySource::new(calc_host::blank_package())),
            &OpenPolicy::default(),
            HostOptions {
                calc_mode: Some(calc_host::CalcMode::Manual),
                ..Default::default()
            },
        )
        .unwrap();
        let sheet = book.host().sheets()[0].id();
        let cell = |r, c| calc_host::value::CellKey::new(sheet, r, c).unwrap();
        let area = calc_host::value::GridArea::new(cell(1, 1), cell(2, 1)).unwrap();
        let formula = calc_host::FormulaText::new(
            calc_host::value::ExcelText::from_utf16_code_units("{1;2}".encode_utf16().collect()),
        )
        .unwrap();
        assert!(
            book.dispatch(calc_host::SkinIntent::Edit {
                edits: vec![calc_host::CellEdit::SetArray {
                    array: calc_host::ArrayFormula::new(area, ArrayKind::Legacy),
                    formula,
                }]
            })
            .accepted()
        );
        assert!(
            book.dispatch(calc_host::SkinIntent::EnterCell {
                at: cell(1, 4),
                text: "=5".into()
            })
            .accepted()
        );
        let mut bytes = Vec::new();
        calc_host::save(book.host(), &calc_host::SavePolicy::default(), &mut bytes).unwrap();
        let record = source_record(&bytes, "array-source").unwrap();
        assert_eq!(record.acquisition, Acquisition::FileCacheReplay);
        assert!(record.observations.is_empty());
        assert_eq!(record.formulas.len(), 3);
        assert!(
            record
                .formulas
                .iter()
                .any(|f| matches!(f.kind, FormulaKind::LegacyArrayMember { .. }))
        );
        let roundtrip = CaptureRecord::decode(record.encode().unwrap().as_bytes()).unwrap();
        assert_eq!(record, roundtrip);
    }
}
