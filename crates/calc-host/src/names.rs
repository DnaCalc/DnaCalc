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
fn valid_name(name: &str) -> bool {
    let first = name.chars().next();
    if name.encode_utf16().count() > 255
        || !first.is_some_and(|c| c.is_alphabetic() || c == '_' || c == '\\')
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\'))
    {
        return false;
    }
    if matches!(
        name.to_ascii_uppercase().as_str(),
        "R" | "C" | "TRUE" | "FALSE"
    ) {
        return false;
    }
    let split = name
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(name.len());
    if split > 0 && split < name.len() {
        let column = name[..split].bytes().try_fold(0u32, |n, c| {
            n.checked_mul(26)?
                .checked_add(u32::from(c.to_ascii_uppercase() - b'A' + 1))
        });
        if column.is_some_and(|c| c <= calc_value::MAX_COLUMNS)
            && name[split..]
                .parse::<u32>()
                .is_ok_and(|r| (1..=calc_value::MAX_ROWS).contains(&r))
        {
            return false;
        }
    }
    let upper = name.to_ascii_uppercase();
    if let Some(rest) = upper.strip_prefix('R')
        && let Some((row, column)) = rest.split_once('C')
        && row.bytes().all(|c| c.is_ascii_digit())
        && column.bytes().all(|c| c.is_ascii_digit())
    {
        return false;
    }
    let parsed = calc_fml::parse_formula(&format!("={name}"));
    parsed.diagnostics().is_empty()
        && parsed
            .expression()
            .is_some_and(|n| n.kind() == calc_fml::syntax::SyntaxKind::IdentifierExpr)
}
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
        return Err("Renaming or moving an existing name is not modelled (dc-971p.34); its name and scope are fixed here".into());
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
}
