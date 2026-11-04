//! Worksheet templates enter the same authored history and evaluator as other sheet commands.
use crate::{
    HostOptions, HostSession, HostState, MemorySource, OpenPolicy, SavePolicy, SkinIntent,
    WorkbookCommand,
};
use std::{collections::BTreeSet, sync::Arc};

pub(crate) fn prepare(
    host: &mut HostState,
    keys: &BTreeSet<crate::SheetKey>,
    bytes: Arc<[u8]>,
    name: Option<String>,
    revision: u64,
) -> Result<calc_doc::LoadedWorkbook, String> {
    let fail = |text: &str| format!("{text} (dc-bctq); nothing changed");
    if revision != host.revision().get() {
        return Err(fail("The workbook changed before inserting the template"));
    }
    if host.loaded.structure_protected() {
        return Err(fail("The workbook structure is protected"));
    }
    let mut source = HostSession::open(
        Arc::new(MemorySource::new(bytes)),
        &OpenPolicy::default(),
        HostOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    if !source.host().loaded().spreadsheet_format().is_template() {
        return Err(fail("Choose an .xltx or .xltm template package"));
    }
    if source.host().loaded().has_vba_project() || host.loaded.has_vba_project() {
        return Err(fail(
            "Template VBA project merging is not modelled; create a workbook from this template instead",
        ));
    }
    if let Some(budget) = host.options.formula_cell_budget {
        let added = source.host().engine.node_count();
        if added > 0 && host.engine.node_count().saturating_add(added) > budget {
            return Err(fail(
                "The template would exceed the workbook formula-cell budget",
            ));
        }
    }

    let mut used: Vec<String> = host
        .model
        .sheets()
        .iter()
        .map(|s| s.name().text().to_string_lossy())
        .collect();
    let sheets: Vec<_> = source
        .host()
        .sheets()
        .iter()
        .map(|s| (s.id(), s.name().text().to_string_lossy()))
        .collect();
    for (index, (_, old)) in sheets.iter().enumerate() {
        let requested = if index == 0 {
            name.as_deref().unwrap_or(old)
        } else {
            old
        };
        let mut next = requested.to_owned();
        if used
            .iter()
            .any(|n| calc_doc_model::value::workbook_names_equal(n, &next))
        {
            next = (1u32..)
                .map(|n| format!("Sheet{n}"))
                .find(|n| {
                    !used
                        .iter()
                        .any(|a| calc_doc_model::value::workbook_names_equal(a, n))
                        && !sheets
                            .iter()
                            .any(|(_, a)| calc_doc_model::value::workbook_names_equal(a, n))
                })
                .ok_or_else(|| fail("No worksheet names remain"))?;
        }
        if next.eq_ignore_ascii_case("History") {
            return Err(fail(
                "History is reserved by Excel; choose another worksheet name",
            ));
        }
        if &next != old || index == 0 {
            let id = source
                .host()
                .sheets()
                .iter()
                .find(|s| s.name().text().to_string_lossy() == *old)
                .ok_or_else(|| fail("The template worksheet changed"))?
                .id();
            let name = crate::SheetName::new(crate::value::ExcelText::from_utf16_code_units(
                next.encode_utf16().collect(),
            ))
            .map_err(|e| e.to_string())?;
            let bytes = calc_doc::rename_template_sheet(
                source.host().loaded(),
                id,
                name,
                &calc_engine::ReferenceRewriter,
            )
            .map_err(|e| e.to_string())?;
            source = HostSession::open(
                Arc::new(MemorySource::new(bytes)),
                &OpenPolicy::default(),
                HostOptions::default(),
            )
            .map_err(|e| e.to_string())?;
        }
        used.push(next);
    }
    let mut current_bytes = Vec::new();
    crate::save(host, &SavePolicy::default(), &mut current_bytes).map_err(|e| e.to_string())?;
    let source = source.host().loaded().clone();
    let open = |bytes: Vec<u8>| {
        calc_doc::open(Arc::new(MemorySource::new(bytes)), &OpenPolicy::default())
            .map_err(|e| e.to_string())
    };
    let current_doc = open(current_bytes)?;
    let mut ids = crate::value::IdAllocator::new();
    let target = calc_doc::load(&current_doc, &mut ids).map_err(|e| e.to_string())?;
    let first = keys
        .iter()
        .map(|k| k.get())
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| fail("No worksheet keys remain"))?;
    let bytes =
        calc_doc::merge_template_sheets(&target, &source, first).map_err(|e| e.to_string())?;
    let document = open(bytes)?;
    let mut loaded =
        calc_doc::load_preserving_sheet_ids(&document, &mut host.sheet_ids, &host.sheet_identities)
            .map_err(|e| e.to_string())?;
    loaded
        .adopt_template_lineage(&host.model)
        .map_err(|e| e.to_string())?;
    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Axis, AxisChange, AxisEdit,
        value::{CellKey, GridArea, ReferenceLike},
    };
    use calc_func::ReferenceHandles;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_bctq__source_rebasing_never_moves_registered_ranges() {
        let mut source = HostSession::open(
            Arc::new(MemorySource::new(calc_doc::blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let mut bytes = Vec::new();
        source
            .save(
                &SavePolicy {
                    format: calc_doc::SaveFormat::Spreadsheet(calc_doc::SpreadsheetFormat::Xltx),
                    ..Default::default()
                },
                &mut bytes,
            )
            .unwrap();
        let mut book = HostSession::open(
            Arc::new(MemorySource::new(calc_doc::blank_package())),
            &OpenPolicy::default(),
            HostOptions::default(),
        )
        .unwrap();
        let sheet = book.host().sheets()[0].id();
        let at = CellKey::new(sheet, 1, 1).unwrap();
        let live = book
            .host()
            .references
            .pin()
            .retain(&ReferenceLike::from_areas(vec![GridArea::cell(at)]).unwrap())
            .unwrap();
        assert!(
            book.dispatch(SkinIntent::EnterCell {
                at,
                text: "42".into()
            })
            .accepted()
        );
        assert!(
            book.dispatch(SkinIntent::EditAxis(
                AxisEdit::new(sheet, Axis::Rows, AxisChange::Insert, 1, 1).unwrap()
            ))
            .accepted()
        );
        let check = |book: &HostSession| {
            assert_eq!(
                book.host().references.pin().resolve(&live).unwrap().areas(),
                &[GridArea::cell(CellKey::new(sheet, 2, 1).unwrap())]
            )
        };
        check(&book);
        let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::InsertTemplate {
            bytes: bytes.into(),
            name: None,
            revision: book.host().revision().get(),
        }));
        assert!(receipt.accepted(), "{receipt:?}");
        check(&book);
        assert!(book.dispatch(SkinIntent::Undo).accepted());
        check(&book);
        assert!(book.dispatch(SkinIntent::Redo).accepted());
        check(&book);
    }
}
