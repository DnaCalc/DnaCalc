use crate::{State, batch::refusal, invalid, query::checked_sheet};
use calc_doc_model::{DefinedName, ModelRoot, NameScope as ModelScope};
use calc_host::{CellEdit, NameEdit};
use calc_skin::{DefinedNameText, ModelPropertyDiff, NameChange, NameScope, Refusal};

pub(crate) struct NamePlan {
    scope: NameScope,
    model_scope: ModelScope,
    before: Option<DefinedNameText>,
    name: String,
}

impl NamePlan {
    pub(crate) fn diff(self, after: &ModelRoot) -> Option<ModelPropertyDiff> {
        let after = after
            .definitions()
            .name_table()
            .lookup(self.model_scope, &self.name)
            .map(|node| name_text(node.definition()));
        (self.before != after).then_some(ModelPropertyDiff::DefinedName {
            scope: self.scope,
            before: self.before,
            after,
        })
    }
}

fn name_text(definition: &DefinedName) -> DefinedNameText {
    DefinedNameText {
        name: definition.name.clone(),
        formula: calc_formula_kernel::display::formula(&format!("={}", definition.formula)),
    }
}

fn model_scope(host: &calc_host::HostState, scope: &NameScope) -> Result<ModelScope, Refusal> {
    match scope {
        NameScope::Workbook => Ok(ModelScope::Workbook),
        NameScope::Sheet(sheet) => Ok(ModelScope::Sheet(checked_sheet(host, *sheet)?)),
    }
}

pub(crate) fn facts(
    host: &calc_host::HostState,
    scope: &NameScope,
    name: &str,
) -> Result<calc_skin::DefinedNameFacts, Refusal> {
    let scope = model_scope(host, scope)?;
    let node = host
        .model()
        .definitions()
        .name_table()
        .lookup(scope, name)
        .ok_or_else(|| invalid("The defined name is unavailable"))?;
    let definition = name_text(node.definition());
    let parameters = calc_host::lambda_parameters(&definition.formula);
    Ok(calc_skin::DefinedNameFacts {
        definition,
        parameters,
    })
}

pub(crate) fn moved(
    before: &ModelRoot,
    after: &ModelRoot,
    edits: &[CellEdit],
) -> Vec<ModelPropertyDiff> {
    edits.iter().filter_map(|edit| {
        let CellEdit::SetName { name, scope, .. } = edit else { return None };
        let prior = before.definitions().name_table().lookup(*scope, name)
            .map(|node| name_text(node.definition()));
        NamePlan {
            scope: crate::inventory::name_scope(*scope),
            model_scope: *scope,
            before: prior,
            name: name.clone(),
        }.diff(after)
    }).collect()
}

pub(crate) fn prepare(
    state: &State,
    scope: NameScope,
    change: &NameChange,
) -> Result<(NamePlan, Vec<CellEdit>, bool), Refusal> {
    let host = state.session.host();
    let model_scope = model_scope(host, &scope)?;
    let table = host.model().definitions().name_table();
    let require = |name| {
        table
            .lookup(model_scope, name)
            .ok_or_else(|| invalid("The defined name is unavailable"))
    };
    let (prior, name, formula) = match change {
        NameChange::Define { name, formula } => (None, name, Some(formula.clone())),
        NameChange::DefineFunction { name, formula } => {
            if calc_host::lambda_parameters(formula).is_none() {
                return Err(invalid("Enter a workbook LAMBDA definition."));
            }
            (None, name, Some(formula.clone()))
        }
        NameChange::Rename { name, to } => {
            let node = require(name.as_str())?;
            (Some(node), to, Some(node.definition().formula.clone()))
        }
        NameChange::Remove { name } => (Some(require(name.as_str())?), name, None),
    };
    let used = matches!(change, NameChange::Remove { .. })
        && prior.is_some_and(|node| host.dependency_name_readers(node.id()).next().is_some());
    let edits = state
        .session
        .prepare_name_edit(NameEdit {
            id: prior.map(|node| node.id()),
            version: prior.map(|node| node.binding_version()),
            scope: model_scope,
            name: name.clone(),
            formula,
        })
        .map_err(refusal)?;
    let plan = NamePlan {
        scope,
        model_scope,
        before: prior.map(|node| name_text(node.definition())),
        name: name.clone(),
    };
    Ok((plan, edits, used))
}
