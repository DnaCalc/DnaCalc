//! Name Manager uses live name identities and the existing scoped name history.
use crate::{CellEdit, HostState};
use calc_doc_model::{NameNode, NameScope};
use calc_value::{CellKey, NameId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameEdit {
    pub id: Option<NameId>,
    pub version: Option<u64>,
    pub scope: NameScope,
    pub name: String,
    pub formula: Option<String>,
}
use calc_engine::valid_defined_name as valid_name;
pub(crate) fn prepare(host: &HostState, edit: NameEdit) -> Result<Vec<CellEdit>, String> {
    let table = host.model().definitions().name_table();
    if let NameScope::Sheet(sheet) = edit.scope
        && host.model().sheet_by_id(sheet).is_none()
    {
        return Err("The name's worksheet is unavailable".into());
    }
    if !valid_name(&edit.name) {
        return Err("Use an Excel name: start with a letter, underscore or backslash; use no spaces or cell-reference spelling (dc-971p.34)".into());
    }
    let formula = edit
        .formula
        .map(|f| f.strip_prefix('=').unwrap_or(&f).to_owned());
    if let Some(formula) = &formula {
        let parsed = calc_fml::parse_formula(&format!("={formula}"));
        if formula.encode_utf16().count() > 8192
            || !parsed.diagnostics().is_empty()
            || parsed.expression().is_none()
        {
            return Err(
                "The name's formula is incomplete or has unsupported syntax (dc-971p.34)".into(),
            );
        }
    }
    let Some(id) = edit.id else {
        if formula.is_none() {
            return Err("Select a name to delete".into());
        }
        if table.lookup(edit.scope, &edit.name).is_some() {
            return Err("That name already exists in this scope".into());
        }
        if host
            .model()
            .definitions()
            .tables()
            .iter()
            .any(|t| calc_value::workbook_names_equal(&t.name, &edit.name))
        {
            return Err("That name belongs to a table".into());
        }
        return Ok(vec![CellEdit::SetName {
            name: edit.name,
            scope: edit.scope,
            formula,
        }]);
    };
    let node = host
        .name_node(id)
        .ok_or("The selected name was deleted; refresh the list")?;
    if edit.version != Some(node.binding_version()) {
        return Err("The name changed; refresh before editing it".into());
    }
    if node.definition().scope != edit.scope || node.definition().name != edit.name {
        if formula.as_deref() != Some(node.definition().formula.as_str()) {
            return Err(
                "Rename or move the name first, then edit or delete its definition (dc-hsa5.6)"
                    .into(),
            );
        }
        // These retained codec sources have no authored rewrite operation yet. Do not
        // leave a dangling validation/chart name behind a successful Name Manager edit.
        let mentions = |text: &str| {
            calc_engine::mentions_name(text, &node.definition().name)
                || calc_engine::mentions_name(text, &edit.name)
        };
        for sheet in host.loaded().sheets() {
            if sheet
                .validations()
                .iter()
                .flat_map(|v| [v.formula1.as_deref(), v.formula2.as_deref()])
                .flatten()
                .any(mentions)
            {
                return Err("A data-validation formula uses this name; its name rewrite is not modelled (dc-hsa5.6)".into());
            }
            let charts = sheet.charts();
            if charts.unread > 0
                || charts.charts.iter().any(|chart| {
                    chart.title_formula.as_deref().is_some_and(mentions)
                        || chart
                            .plots
                            .iter()
                            .flat_map(|plot| &plot.series)
                            .flat_map(|series| {
                                [
                                    series.name.as_ref(),
                                    series.categories.as_ref(),
                                    series.values.as_ref(),
                                ]
                            })
                            .flatten()
                            .filter_map(|data| data.formula.as_deref())
                            .any(mentions)
                })
            {
                return Err(
                    "A chart source cannot be checked for this name change (dc-hsa5.6)".into(),
                );
            }
        }
        return host
            .engine
            .prepare_name_change(
                host.model(),
                &calc_engine::NameChangeRequest {
                    id,
                    version: node.binding_version(),
                    scope: edit.scope,
                    name: edit.name,
                },
            )
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|operation| match operation {
                calc_doc_model::WorkbookOp::RestoreNames(names) => {
                    Ok(CellEdit::RestoreNames(names))
                }
                calc_doc_model::WorkbookOp::SetCell { at, input } => {
                    Ok(CellEdit::Set { at, input })
                }
                calc_doc_model::WorkbookOp::SetArray {
                    array,
                    input: calc_doc_model::CellInput::Formula(formula),
                } => Ok(CellEdit::SetArray { array, formula }),
                calc_doc_model::WorkbookOp::SetConditionalRules { sheet, rules } => {
                    Ok(CellEdit::ConditionalRules { sheet, rules })
                }
                _ => Err(
                    "The name change returned an unsupported authored operation (dc-hsa5.6)".into(),
                ),
            })
            .collect();
    }
    if formula.as_ref() == Some(&node.definition().formula) {
        return Ok(Vec::new());
    }
    let mut snapshots = table.snapshot_keys(&[node.key().clone()], formula.is_none());
    for snapshot in &mut snapshots {
        if let Some(formula) = &formula {
            let record = snapshot
                .records
                .iter_mut()
                .find(|r| r.origin == node.origin())
                .ok_or("The selected source record is unavailable")?;
            record.definition.formula = formula.clone();
        } else {
            snapshot.records.retain(|r| r.origin != node.origin());
        }
    }
    Ok(vec![CellEdit::RestoreNames(snapshots)])
}
impl HostState {
    pub fn name_node(&self, id: NameId) -> Option<&NameNode> {
        self.engine.name_node(&self.model, id)
    }
    /// Explicit diagnostic evaluation, with a real name identity and caller anchor. No
    /// authored cell or shared name value is created, and no file cache is accepted as current.
    pub fn evaluate_name(
        &self,
        id: NameId,
        version: u64,
        anchor: CellKey,
    ) -> Result<calc_value::CalcValue, String> {
        let node = self
            .name_node(id)
            .ok_or("The selected name is unavailable")?;
        if node.binding_version() != version {
            return Err("The name changed; refresh before evaluating it".into());
        }
        if self.model.sheet_by_id(anchor.sheet()).is_none() {
            return Err("The evaluation worksheet is unavailable".into());
        }
        let formula = calc_engine::DetachedFormula::compile_name(&self.model, id, anchor)
            .map_err(|e| e.to_string())?;
        formula
            .evaluate(&self.engine, &self.model, anchor)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HostSession, ProjectedValue, SkinIntent, WorkbookCommand};
    use calc_value::{CalcValue, CoreRef, GridArea};
    fn book() -> HostSession {
        HostSession::open(
            std::sync::Arc::new(crate::MemorySource::new(crate::blank_package())),
            &Default::default(),
            Default::default(),
        )
        .unwrap()
    }
    fn add(book: &mut HostSession, scope: NameScope, name: &str, formula: &str) -> NameId {
        let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::Name(NameEdit {
            id: None,
            version: None,
            scope,
            name: name.into(),
            formula: Some(formula.into()),
        })));
        assert!(receipt.accepted(), "{:?}", receipt.error);
        book.host()
            .model()
            .definitions()
            .name_table()
            .lookup(scope, name)
            .unwrap()
            .id()
    }
    fn value(book: &HostSession, id: NameId, at: CellKey) -> CalcValue {
        let node = book.host().name_node(id).unwrap();
        book.host()
            .evaluate_name(id, node.binding_version(), at)
            .unwrap()
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn selected_name_identity_overrides_only_the_root_lookup_and_preserves_name_wrapping() {
        let mut book = book();
        let sheet = book.host().sheets()[0].id();
        let a1 = CellKey::new(sheet, 1, 1).unwrap();
        let global = add(&mut book, NameScope::Workbook, "Choice", "=40+2");
        let local = add(&mut book, NameScope::Sheet(sheet), "Choice", "=99");
        assert_eq!(value(&book, global, a1), CalcValue::number(42.));
        assert_eq!(value(&book, local, a1), CalcValue::number(99.));
        let alias = add(&mut book, NameScope::Workbook, "Alias", "=Choice+1");
        assert_eq!(value(&book, alias, a1), CalcValue::number(100.));
        let relative = add(&mut book, NameScope::Workbook, "Relative", "=B1");
        let edge = CellKey::new(sheet, calc_value::MAX_ROWS, calc_value::MAX_COLUMNS).unwrap();
        let evaluated = value(&book, relative, edge);
        let CoreRef::Reference(reference) = evaluated.core_ref() else {
            panic!("a name reference remains a reference: {evaluated:?}")
        };
        assert_eq!(
            reference.areas(),
            &[GridArea::cell(
                CellKey::new(sheet, calc_value::MAX_ROWS, 1).unwrap()
            )]
        );
        assert!(valid_name("Growth_2026"));
        assert!(valid_name("税率"));
        assert!(!valid_name("A1"));
        assert!(!valid_name("R1C1"));
        assert!(!valid_name("two names"));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn name_manager_edits_rebind_users_and_use_existing_undo_redo_and_save() {
        let mut book = book();
        let sheet = book.host().sheets()[0].id();
        let a1 = CellKey::new(sheet, 1, 1).unwrap();
        let id = add(&mut book, NameScope::Workbook, "Rate", "=2");
        assert!(
            book.dispatch(SkinIntent::EnterCell {
                at: a1,
                text: "=Rate*3".into()
            })
            .accepted()
        );
        let number = |book: &HostSession| match crate::project(
            book.host(),
            &crate::ProjectionRequest { cells: vec![a1] },
        )
        .unwrap()
        .cells()[0]
            .value()
        {
            ProjectedValue::Calculated { value, .. } => value.clone(),
            other => panic!("calculated user: {other:?}"),
        };
        assert_eq!(number(&book), CalcValue::number(6.));
        let version = book.host().name_node(id).unwrap().binding_version();
        let edit = NameEdit {
            id: Some(id),
            version: Some(version),
            scope: NameScope::Workbook,
            name: "Rate".into(),
            formula: Some("=4".into()),
        };
        assert!(
            book.dispatch(SkinIntent::Command(WorkbookCommand::Name(edit.clone())))
                .accepted()
        );
        assert_eq!(number(&book), CalcValue::number(12.));
        assert!(book.host().evaluate_name(id, version, a1).is_err());
        let revision = book.host().revision();
        assert!(
            !book
                .dispatch(SkinIntent::Command(WorkbookCommand::Name(edit)))
                .accepted()
        );
        assert_eq!(book.host().revision(), revision);
        assert!(book.dispatch(SkinIntent::Undo).accepted());
        assert_eq!(number(&book), CalcValue::number(6.));
        assert!(book.dispatch(SkinIntent::Redo).accepted());
        assert_eq!(number(&book), CalcValue::number(12.));
        let node = book.host().name_node(id).unwrap();
        let version = node.binding_version();
        let definition = node.definition().clone();
        assert!(
            book.dispatch(SkinIntent::Command(WorkbookCommand::Name(NameEdit {
                id: Some(id),
                version: Some(version),
                scope: NameScope::Workbook,
                name: "Rate".into(),
                formula: None
            })))
            .accepted()
        );
        assert!(book.host().name_node(id).is_none());
        assert!(book.dispatch(SkinIntent::Undo).accepted());
        let restored = book
            .host()
            .model()
            .definitions()
            .name_table()
            .lookup(NameScope::Workbook, "Rate")
            .unwrap();
        assert_ne!(restored.id(), id);
        assert_eq!(restored.definition(), &definition);
        assert_eq!(number(&book), CalcValue::number(12.));
        let mut bytes = Vec::new();
        book.save(&Default::default(), &mut bytes).unwrap();
        let reopened = HostSession::open(
            std::sync::Arc::new(crate::MemorySource::new(bytes)),
            &Default::default(),
            Default::default(),
        )
        .unwrap();
        assert_eq!(
            reopened
                .host()
                .model()
                .definitions()
                .name_table()
                .lookup(NameScope::Workbook, "Rate")
                .unwrap()
                .definition(),
            &definition
        );
    }
    fn extra_sheet(book: &mut HostSession, name: &str) -> calc_value::SheetId {
        assert!(
            book.dispatch(SkinIntent::Command(WorkbookCommand::Sheet {
                sheet: None,
                change: crate::SheetChange::Add { name: name.into() }
            }))
            .accepted()
        );
        book.host()
            .model()
            .sheets()
            .iter()
            .find(|sheet| sheet.name().text().to_string_lossy() == name)
            .unwrap()
            .id()
    }
    fn rename(
        book: &mut HostSession,
        id: NameId,
        scope: NameScope,
        name: &str,
    ) -> crate::IntentReceipt {
        let node = book.host().name_node(id).unwrap();
        let edit = NameEdit {
            id: Some(id),
            version: Some(node.binding_version()),
            scope,
            name: name.into(),
            formula: Some(node.definition().formula.clone()),
        };
        book.dispatch(SkinIntent::Command(WorkbookCommand::Name(edit)))
    }
    fn entered(book: &mut HostSession, at: CellKey, text: &str) {
        let result = book.dispatch(SkinIntent::EnterCell {
            at,
            text: text.into(),
        });
        assert!(result.accepted(), "{:?}", result.error);
    }
    fn cell_formula(book: &HostSession, at: CellKey) -> String {
        match book.host().model().cell(at).unwrap() {
            calc_doc_model::CellInput::Formula(text) => text.text().to_string_lossy(),
            other => panic!("formula {other:?}"),
        }
    }
    fn current(book: &HostSession, at: CellKey) -> CalcValue {
        match crate::project(book.host(), &crate::ProjectionRequest { cells: vec![at] })
            .unwrap()
            .cells()[0]
            .value()
        {
            ProjectedValue::Calculated { value, .. } => value.clone(),
            other => panic!("calculated {other:?}"),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn explicit_nested_name_contexts_keep_caller_and_definition_scope_distinct() {
        let mut book = book();
        let a = book.host().sheets()[0].id();
        let b = extra_sheet(&mut book, "Other");
        let global = add(&mut book, NameScope::Workbook, "Inner", "=1");
        let local_a = add(&mut book, NameScope::Sheet(a), "Inner", "=10");
        let local_b = add(&mut book, NameScope::Sheet(b), "Inner", "=20");
        let alias = add(&mut book, NameScope::Workbook, "Alias", "=Inner");
        let outer = add(&mut book, NameScope::Sheet(a), "Outer", "=Alias+1");
        let at_a = CellKey::new(a, 1, 1).unwrap();
        let at_b = CellKey::new(b, 1, 1).unwrap();
        assert_eq!(value(&book, alias, at_a), CalcValue::number(10.));
        assert_eq!(value(&book, alias, at_b), CalcValue::number(20.));
        assert_eq!(value(&book, outer, at_b), CalcValue::number(11.));
        assert_eq!(value(&book, global, at_a), CalcValue::number(1.));
        let mut cache = calc_fml::PlanCache::new();
        let first = calc_engine::DetachedFormula::analyze_name(
            book.host().model(),
            alias,
            at_a,
            &calc_fml::CallableBindings::default(),
            &mut cache,
        )
        .unwrap();
        let another = CellKey::new(a, 2, 1).unwrap();
        let second = calc_engine::DetachedFormula::analyze_name(
            book.host().model(),
            alias,
            another,
            &calc_fml::CallableBindings::default(),
            &mut cache,
        )
        .unwrap();
        assert!(std::sync::Arc::ptr_eq(&first.plan, &second.plan));
        assert!(
            second
                .uses
                .iter()
                .all(|usage| usage.context.caller.cell() == another)
        );
        for (caller, expected) in [(at_a, local_a), (at_b, local_b)] {
            let analysis = calc_engine::DetachedFormula::analyze_name(
                book.host().model(),
                alias,
                caller,
                &calc_fml::CallableBindings::default(),
                &mut calc_fml::PlanCache::new(),
            )
            .unwrap();
            let usage = analysis
                .uses
                .iter()
                .find(|usage| usage.context.definition == Some(alias))
                .unwrap();
            assert_eq!(usage.context.caller.cell(), caller);
            assert_eq!(usage.context.lookup_sheet(), caller.sheet());
            assert_eq!(usage.resolved.unwrap().id, expected);
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn rename_rewrites_only_bound_uses_and_keeps_ids_history_and_save() {
        let mut book = book();
        let a = book.host().sheets()[0].id();
        let b = extra_sheet(&mut book, "Other");
        let id = add(&mut book, NameScope::Workbook, "Rate", "=2");
        let shadow = add(&mut book, NameScope::Sheet(a), "Rate", "=10");
        let alias = add(&mut book, NameScope::Sheet(b), "Alias", "=Rate+1");
        let a1 = CellKey::new(a, 1, 1).unwrap();
        let b1 = CellKey::new(b, 1, 1).unwrap();
        let b2 = CellKey::new(b, 2, 1).unwrap();
        let b3 = CellKey::new(b, 3, 1).unwrap();
        let b4 = CellKey::new(b, 4, 1).unwrap();
        entered(&mut book, a1, "=Rate");
        entered(&mut book, b1, "=Rate*3");
        entered(&mut book, b2, "=LET(Rate,5,Rate+1)");
        entered(&mut book, b3, "=\"Rate\"");
        entered(&mut book, b4, "=\"😀\"&Rate");
        let let_source = cell_formula(&book, b2);
        let version = book.host().name_node(id).unwrap().binding_version();
        let result = rename(&mut book, id, NameScope::Workbook, "YieldRate");
        assert!(result.accepted(), "{:?}", result.error);
        assert_eq!(
            book.host().name_node(id).unwrap().definition().name,
            "YieldRate"
        );
        assert_eq!(
            book.host().name_node(shadow).unwrap().definition().name,
            "Rate"
        );
        assert_eq!(cell_formula(&book, a1), "Rate");
        assert_eq!(cell_formula(&book, b1), "YieldRate*3");
        assert_eq!(cell_formula(&book, b2), let_source);
        assert_eq!(cell_formula(&book, b3), "\"Rate\"");
        assert_eq!(cell_formula(&book, b4), "\"😀\"&YieldRate");
        assert_eq!(
            book.host().name_node(alias).unwrap().definition().formula,
            "YieldRate+1"
        );
        assert_eq!(current(&book, a1), CalcValue::number(10.));
        assert_eq!(current(&book, b1), CalcValue::number(6.));
        assert!(book.host().evaluate_name(id, version, b1).is_err());
        assert!(book.dispatch(SkinIntent::Undo).accepted());
        assert_eq!(book.host().name_node(id).unwrap().definition().name, "Rate");
        assert_eq!(cell_formula(&book, b1), "Rate*3");
        assert!(book.dispatch(SkinIntent::Redo).accepted());
        assert_eq!(
            book.host().name_node(id).unwrap().definition().name,
            "YieldRate"
        );
        let mut bytes = Vec::new();
        book.save(&Default::default(), &mut bytes).unwrap();
        let reopened = HostSession::open(
            std::sync::Arc::new(crate::MemorySource::new(bytes)),
            &Default::default(),
            Default::default(),
        )
        .unwrap();
        assert!(
            reopened
                .host()
                .model()
                .definitions()
                .name_table()
                .lookup(NameScope::Workbook, "YieldRate")
                .is_some()
        );
        assert_eq!(cell_formula(&reopened, b1), "YieldRate*3");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn scope_moves_qualify_consumers_and_keep_array_extent_and_live_identity() {
        let mut book = book();
        let a = book.host().sheets()[0].id();
        let b = extra_sheet(&mut book, "Other");
        let id = add(&mut book, NameScope::Workbook, "Rate", "=2");
        let a1 = CellKey::new(a, 1, 1).unwrap();
        let b1 = CellKey::new(b, 1, 1).unwrap();
        entered(&mut book, a1, "=Rate+1");
        entered(&mut book, b1, "=Rate+2");
        let area = GridArea::new(
            CellKey::new(b, 3, 1).unwrap(),
            CellKey::new(b, 4, 1).unwrap(),
        )
        .unwrap();
        let array = calc_doc_model::ArrayFormula::new(area, calc_doc_model::ArrayKind::Legacy);
        assert!(
            book.dispatch(SkinIntent::Edit {
                edits: vec![crate::CellEdit::SetArray {
                    array,
                    formula: calc_doc_model::FormulaText::with_form(
                        calc_value::ExcelText::from_utf16_code_units(
                            "Rate*{1;2}".encode_utf16().collect()
                        ),
                        calc_fml::FormulaForm::Legacy
                    )
                    .unwrap()
                }]
            })
            .accepted()
        );
        let result = rename(&mut book, id, NameScope::Sheet(a), "Rate");
        assert!(result.accepted(), "{:?}", result.error);
        assert!(cell_formula(&book, b1).contains("!Rate"));
        assert_eq!(current(&book, a1), CalcValue::number(3.));
        assert_eq!(current(&book, b1), CalcValue::number(4.));
        assert_eq!(
            book.host().model().definitions().array(area.first()),
            Some(&array)
        );
        let result = rename(&mut book, id, NameScope::Workbook, "Rate");
        assert!(result.accepted(), "{:?}", result.error);
        assert_eq!(cell_formula(&book, b1), "Rate+2");
        assert_eq!(current(&book, b1), CalcValue::number(4.));
        assert!(book.dispatch(SkinIntent::Undo).accepted());
        assert_eq!(
            book.host().name_node(id).unwrap().definition().scope,
            NameScope::Sheet(a)
        );
        assert!(book.dispatch(SkinIntent::Redo).accepted());
        assert_eq!(
            book.host().name_node(id).unwrap().definition().scope,
            NameScope::Workbook
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn nested_capture_collision_and_reference_changing_moves_refuse_before_history() {
        let mut book = book();
        let a = book.host().sheets()[0].id();
        let b = extra_sheet(&mut book, "Other");
        let id = add(&mut book, NameScope::Workbook, "Rate", "=2");
        add(&mut book, NameScope::Sheet(a), "Rate", "=10");
        add(&mut book, NameScope::Workbook, "Alias", "=Rate+1");
        let revision = book.host().revision();
        let result = rename(&mut book, id, NameScope::Workbook, "YieldRate");
        assert!(!result.accepted());
        assert_eq!(book.host().revision(), revision);
        assert!(
            result
                .error
                .unwrap()
                .to_string()
                .contains("different identities")
        );
        let local = add(&mut book, NameScope::Sheet(b), "MoveMe", "=A1");
        entered(&mut book, CellKey::new(a, 2, 1).unwrap(), "='Other'!MoveMe");
        let revision = book.host().revision();
        assert!(!rename(&mut book, local, NameScope::Workbook, "MoveMe").accepted());
        assert_eq!(book.host().revision(), revision);
        let another = add(&mut book, NameScope::Workbook, "Taken", "=8");
        let revision = book.host().revision();
        assert!(!rename(&mut book, another, NameScope::Workbook, "Rate").accepted());
        assert_eq!(book.host().revision(), revision);
    }
}
