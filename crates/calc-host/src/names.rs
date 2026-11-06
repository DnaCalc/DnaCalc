//! Name Manager uses live name identities and the existing scoped name history.
use crate::{CellEdit, HostState};
use calc_doc_model::{NameNode, NameScope};
use calc_value::{CellKey, NameId};

/// Existing execution evidence with the ordinary cell projection's current/stale
/// qualifications. This query neither binds a definition nor evaluates its body.
#[derive(Clone, Debug, PartialEq)]
pub struct NameInvocationView {
    pub revision: calc_doc_model::AuthoredRevision,
    pub results: calc_engine::contract::ReadViewId,
    pub result: crate::ProjectedCell,
    pub root: calc_engine::contract::InvocationId,
    pub invocations: Vec<calc_fml::NameInvocation>,
    pub omitted: u64,
}

/// A requested name diagnostic against one published result view. Its external command
/// identity belongs to the caller; the evaluation retains its own typed name pass.
/// It can consume supplied clock/random services without installing a cell or shared value.
#[derive(Clone, Debug, PartialEq)]
pub struct NameDiagnosticEvaluation {
    pub revision: calc_doc_model::AuthoredRevision,
    pub results: calc_engine::contract::ReadViewId,
    pub workbook: calc_engine::contract::WorkbookInstanceId,
    pub evaluation: calc_engine::DetachedNameEvaluation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameEdit {
    pub id: Option<NameId>,
    pub version: Option<u64>,
    pub scope: NameScope,
    pub name: String,
    pub formula: Option<String>,
}
use calc_engine::valid_defined_name as valid_name;

/// Authored name bodies use the existing file-form converter. Name results are
/// reference-or-value, so retain explicit intersection rather than choosing a
/// cell's inferred legacy form. Imported/restored definitions keep their source.
fn authored_name_formula(host: &HostState, text: &str, scope: NameScope) -> Result<String, String> {
    let formula = calc_doc_model::FormulaText::with_form(
        calc_value::ExcelText::from_utf16_code_units(text.encode_utf16().collect()),
        calc_doc_model::FormulaForm::Dynamic,
    ).map_err(|_| "The name's formula is empty")?;
    let table = host.model().definitions().name_table();
    let lookup = |qualifier: Option<&str>, spelling: &str| {
        let local = match qualifier {
            Some(name) => Some(host.model().sheets().iter().find(|sheet|
                calc_value::workbook_names_equal(&sheet.name().text().to_string_lossy(), name))?.id()),
            None => match scope { NameScope::Sheet(sheet) => Some(sheet), NameScope::Workbook => None },
        };
        if let Some(sheet) = local
            && let Some(node) = table.lookup(NameScope::Sheet(sheet), spelling)
        {
            return Some(node.definition().formula.as_str());
        }
        qualifier.is_none().then(|| table.lookup(NameScope::Workbook, spelling))
            .flatten().map(|node| node.definition().formula.as_str())
    };
    let links = host.model().definitions().external_file_names();
    let form = calc_doc::file_form_with_lookup(&formula, &lookup, &links)
        .map_err(|_| "The name's formula cannot be written in Excel form (dc-hsa5.6)")?;
    String::from_utf16(&form.text).map_err(|_| "The name's formula is unreadable (dc-hsa5.6)".into())
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
            formula: formula.map(|f| authored_name_formula(host, &f, edit.scope)).transpose()?,
        }]);
    };
    let node = host
        .name_node(id)
        .ok_or("The selected name was deleted; refresh the list")?;
    if edit.version != Some(node.binding_version()) {
        return Err("The name changed; refresh before editing it".into());
    }
    if node.definition().scope != edit.scope || node.definition().name != edit.name {
        let original = calc_fml::assist::display_formula(&format!("={}", node.definition().formula));
        if formula.as_ref().map(|f| calc_fml::assist::display_formula(&format!("={f}"))).as_ref() != Some(&original) {
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
    let formula = formula.map(|f| authored_name_formula(host, &f, edit.scope)).transpose()?;
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
    pub fn name_invocation_view(&self, caller: CellKey) -> Option<NameInvocationView> {
        let page = crate::project(
            self,
            &crate::ProjectionRequest {
                cells: vec![caller],
            },
        )
        .ok()?;
        let result = page.cells().first()?;
        let calculation = match result.value() {
            crate::ProjectedValue::Calculated { calculation, .. }
            | crate::ProjectedValue::NotModeled { calculation, .. } => calculation,
            crate::ProjectedValue::Stale(stale) => stale.last_calculation()?,
            _ => return None,
        };
        let stamp = calculation.stamp()?;
        let calc_engine::contract::ComputationOrigin::Evaluated { root } = &stamp.computed_by
        else {
            return None;
        };
        Some(NameInvocationView {
            revision: self.revision(),
            results: self.engine.result_view(),
            result: result.clone(),
            root: root.clone(),
            invocations: stamp.reads.name_invocations.clone(),
            omitted: stamp.reads.omitted_name_invocations,
        })
    }

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
        self.evaluate_name_with_evidence(id, version, anchor)?.evaluation.outcome
            .map_err(|e| e.to_string())
    }

    pub fn evaluate_name_with_evidence(
        &self,
        id: NameId,
        version: u64,
        anchor: CellKey,
    ) -> Result<NameDiagnosticEvaluation, String> {
        let node = self
            .name_node(id)
            .ok_or("The selected name is unavailable")?;
        if node.binding_version() != version {
            return Err("The name changed; refresh before evaluating it".into());
        }
        if self.model.sheet_by_id(anchor.sheet()).is_none() {
            return Err("The evaluation worksheet is unavailable".into());
        }
        let formula = calc_engine::DetachedFormula::compile_name_with_bindings(
            &self.model, id, anchor, &self.user_function_bindings())
            .map_err(|e| e.to_string())?;
        Ok(NameDiagnosticEvaluation {
            revision: self.revision(),
            results: self.engine.result_view(),
            workbook: self.engine.workbook_instance(),
            evaluation: formula.evaluate_with_name_evidence(&self.engine, &self.model, anchor, &self.options.numeric),
        })
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
    fn dc_hsa5_6__current_numeric_context_refuses_old_formula_values_but_keeps_lazy_blocks() {
        use calc_engine::{DetachedEvaluationContext as Context, DetachedReadPolicy as Policy};
        let mut book = book();
        let anchor = CellKey::new(book.host().sheets()[0].id(), 1, 1).unwrap();
        assert!(book.dispatch(SkinIntent::EnterCell { at: anchor, text: "=2".into() }).accepted());
        let host = book.host();
        let fast = Context::new(&host.engine, host.model(), Policy::CurrentValues).unwrap();
        assert_eq!(fast.read_cell(anchor), Ok(CalcValue::number(2.)));
        let view = host.engine.result_view();
        let revision = host.revision();
        let history = book.history_depth();
        let mode = calc_value::NumericMode::Exact(calc_value::NumericTargetId::new(
            "16.0.20527.20150", "Current", "Excel", "x86-64").unwrap());
        let current = Context::new_with_numeric_mode(&host.engine, host.model(), Policy::CurrentValues, &mode).unwrap();
        assert!(current.stale_cells().unwrap().contains(&anchor));
        assert_eq!(current.all_formula_values_current(), Some(false));
        assert!(current.read_cell(anchor).is_err());
        let lazy = current.compile("IF(FALSE,$A$1,42)", anchor, calc_fml::FormulaForm::Legacy).unwrap();
        assert_eq!(lazy.evaluate_in(&current, anchor), Ok(CalcValue::number(42.)));
        let arithmetic = current.compile("1+1", anchor, calc_fml::FormulaForm::Legacy).unwrap();
        assert!(arithmetic.evaluate_in(&current, anchor).is_err());
        let historical = Context::new_with_numeric_mode(&host.engine, host.model(), Policy::PublishedValues, &mode).unwrap();
        assert_eq!(historical.read_cell(anchor), Ok(CalcValue::number(2.)));
        assert!(historical.all_formula_values_current().is_none());
        assert_eq!((host.engine.result_view(), host.revision(), book.history_depth()), (view, revision, history));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__current_context_rejects_expired_callable_bindings_without_entry() {
        use calc_engine::{DetachedEvaluationContext as Context, DetachedReadPolicy as Policy};
        #[derive(Debug)]
        struct Counted(std::rc::Rc<std::cell::Cell<usize>>);
        impl calc_fml::UserFunction for Counted {
            fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
                self.0.set(self.0.get() + 1);
                Ok(CalcValue::number(42.))
            }
        }
        let mut book = book();
        let anchor = CellKey::new(book.host().sheets()[0].id(), 1, 1).unwrap();
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let functions = || {
            let mut table = calc_fml::CallableTable::new();
            table.register("KnownUdf", std::rc::Rc::new(Counted(calls.clone()))).unwrap();
            table
        };
        book.set_user_functions(functions());
        let old = {
            let host = book.host();
            let context = Context::new(&host.engine, host.model(), Policy::CurrentValues).unwrap();
            let formula = context.compile("IF(FALSE,KnownUdf(),42)", anchor, calc_fml::FormulaForm::Legacy).unwrap();
            assert_eq!(formula.evaluate_in(&context, anchor), Ok(CalcValue::number(42.)));
            formula
        };
        book.set_user_functions(functions());
        let host = book.host();
        let context = Context::new(&host.engine, host.model(), Policy::CurrentValues).unwrap();
        assert!(old.evaluate_in(&context, anchor).is_err(), "registration replacement expires the old plan");
        let fresh = context.compile("IF(FALSE,KnownUdf(),42)", anchor, calc_fml::FormulaForm::Legacy).unwrap();
        assert_eq!(fresh.evaluate_in(&context, anchor), Ok(CalcValue::number(42.)));
        assert_eq!(calls.get(), 0, "neither binding nor a lazy branch enters user code");
        assert_eq!(host.engine.nodes().count(), 0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__explicit_name_passes_keep_draw_occurrences_and_let_reuse() {
        let mut book = book();
        let sheet = book.host().sheets()[0].id();
        let anchor = CellKey::new(sheet, 4, 5).unwrap();
        let inner = add(&mut book, NameScope::Workbook, "PassRandom", "=RAND()");
        let repeated = add(&mut book, NameScope::Workbook, "RepeatedRandom", "=PassRandom+PassRandom");
        let once = add(&mut book, NameScope::Workbook, "LocalRandom", "=LET(x,PassRandom,x+x)");
        let evaluate = |id| book.host().evaluate_name_with_evidence(id,
            book.host().name_node(id).unwrap().binding_version(), anchor).unwrap();
        let revision = book.host().revision();
        let results = book.host().engine.result_view();
        let history = book.history_depth();
        let first = evaluate(repeated);
        let root = first.evaluation.root.as_ref().unwrap();
        assert_eq!(root.workbook, first.workbook);
        assert_eq!(root.locus.definition.id, repeated);
        assert_eq!(root.locus.caller, anchor);
        assert_eq!(root.pass.ordinal, 0);
        assert_eq!(root.reason, calc_engine::contract::ReevaluationReason::NewCommandDemand);
        assert_eq!(first.evaluation.inputs, book.host().engine.input_version());
        let draws = &first.evaluation.random_draws;
        assert_eq!((draws.samples.len(), draws.omitted), (2, 0));
        assert_eq!(first.evaluation.outcome, Ok(CalcValue::number(
            f64::from_bits(draws.samples[0]) + f64::from_bits(draws.samples[1]))));
        assert_eq!(first.evaluation.invocations.iter().filter(|body| body.definition.id == inner).count(), 2);
        let local = evaluate(once);
        let local_root = local.evaluation.root.as_ref().unwrap();
        assert_ne!(root.pass.command, local_root.pass.command, "new command has a new draw key");
        let draws = &local.evaluation.random_draws;
        assert_eq!((draws.samples.len(), draws.omitted), (1, 0));
        let x = f64::from_bits(draws.samples[0]);
        assert_eq!(local.evaluation.outcome, Ok(CalcValue::number(x + x)));
        assert_eq!(local.evaluation.invocations.iter().filter(|body| body.definition.id == inner).count(), 1);
        for _ in 0..4 {
            assert!(book.host().name_node(inner).is_some());
            assert!(book.host().name_invocation_view(anchor).is_none());
        }
        let next = evaluate(once);
        assert_eq!(next.evaluation.root.as_ref().unwrap().pass.command.sequence,
            local_root.pass.command.sequence + 1, "passive views allocate no name pass");
        assert_eq!(book.host().engine.nodes().count(), 0, "no synthetic name cell");
        assert_eq!((book.host().revision(), book.host().engine.result_view(), book.history_depth()),
            (revision, results, history));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__name_passes_bound_draw_evidence_and_refuse_stale_inputs() {
        let mut book = book();
        let sheet = book.host().sheets()[0].id();
        let at = |row, column| CellKey::new(sheet, row, column).unwrap();
        let anchor = at(4,5);
        let array = add(&mut book, NameScope::Workbook, "RandomArray", "=RANDARRAY(1,1026)");
        let evaluate = |book: &HostSession, id| book.host().evaluate_name_with_evidence(id,
            book.host().name_node(id).unwrap().binding_version(), anchor).unwrap();
        let sampled = evaluate(&book, array);
        assert!(sampled.evaluation.outcome.is_ok(), "{:?}", sampled.evaluation.outcome);
        let draws = &sampled.evaluation.random_draws;
        assert_eq!((draws.samples.len(), draws.omitted), (calc_engine::MAX_RECORDED_RANDOM_DRAWS, 2),
            "outcome {:?}; bodies {:?}", sampled.evaluation.outcome, sampled.evaluation.invocations);
        let value = sampled.evaluation.outcome.as_ref().unwrap().as_array().unwrap();
        assert_eq!((value.shape().rows, value.shape().cols), (1, 1026));
        for (value, bits) in value.iter_row_major().zip(&draws.samples) {
            assert_eq!(value.as_number().unwrap().to_bits(), *bits);
        }
        assert_eq!(book.host().engine.nodes().count(), 0);
        assert_eq!(book.host().name_node(array).unwrap().definition().formula, "_xlfn.RANDARRAY(1,1026)");
        let mut bytes = Vec::new();
        book.save(&Default::default(), &mut bytes).unwrap();
        let reopened = HostSession::open(std::sync::Arc::new(crate::MemorySource::new(bytes)),
            &Default::default(), crate::HostOptions { calc_mode: Some(crate::CalcMode::Manual), ..Default::default() }).unwrap();
        let restored = reopened.host().model().definitions().name_table().lookup(NameScope::Workbook, "RandomArray").unwrap();
        assert_eq!(restored.definition().formula, "_xlfn.RANDARRAY(1,1026)");
        let restored_evaluation = reopened.host().evaluate_name_with_evidence(restored.id(), restored.binding_version(),
            CellKey::new(reopened.host().sheets()[0].id(), 4, 5).unwrap()).unwrap();
        assert_eq!((restored_evaluation.evaluation.random_draws.samples.len(), restored_evaluation.evaluation.random_draws.omitted),
            (calc_engine::MAX_RECORDED_RANDOM_DRAWS, 2));
        assert_ne!(restored_evaluation.workbook, sampled.workbook, "reopen has a new owner namespace");
        for (at, text) in [(at(1,1), "1"), (at(1,2), "=$A$1+1")] {
            assert!(book.dispatch(SkinIntent::EnterCell { at, text: text.into() }).accepted());
        }
        let stale = add(&mut book, NameScope::Workbook, "RandomThenStale", "=RAND()+$B$1");
        assert!(book.dispatch(SkinIntent::SetCalcMode(crate::CalcMode::Manual)).accepted());
        assert!(book.dispatch(SkinIntent::EnterCell { at: at(1,1), text: "5".into() }).accepted());
        let revision = book.host().revision();
        let results = book.host().engine.result_view();
        let history = book.history_depth();
        let refused = evaluate(&book, stale);
        assert!(refused.evaluation.outcome.is_err());
        assert_eq!(refused.evaluation.random_draws.samples.len(), 1, "retain the executed prefix");
        assert_eq!(refused.evaluation.root.as_ref().unwrap().locus.definition.id, stale);
        assert!(refused.evaluation.invocations.iter().any(|body| body.result == calc_fml::NameResultKind::Refused));
        let page = crate::project(book.host(), &crate::ProjectionRequest { cells: vec![at(1,2)] }).unwrap();
        assert!(matches!(page.cells()[0].value(), ProjectedValue::Stale(_)), "diagnostic admits no producer");
        assert_eq!((book.host().revision(), book.host().engine.result_view(), book.history_depth()),
            (revision, results, history));
        let clock = add(&mut book, NameScope::Workbook, "OwnerClock", "=NOW()");
        book.set_clock(Some(45200.25));
        let timed = evaluate(&book, clock);
        assert_eq!(timed.evaluation.clock, Some(45200.25));
        assert_eq!(timed.evaluation.outcome, Ok(CalcValue::number(45200.25)));
        assert!(timed.evaluation.random_draws.samples.is_empty());
        book.set_clock(None);
        let absent = evaluate(&book, clock);
        assert_eq!(absent.evaluation.clock, None);
        assert!(absent.evaluation.outcome.is_err(), "no implicit clock source");
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__name_pass_uses_owner_numeric_mode_before_any_cell_publication() {
        let target = calc_value::NumericTargetId::new("16.0.20527.20150", "Current", "Excel", "x86-64").unwrap();
        let mode = calc_value::NumericMode::Exact(target);
        let mut book = HostSession::open(
            std::sync::Arc::new(crate::MemorySource::new(crate::blank_package())),
            &Default::default(),
            crate::HostOptions { numeric: mode.clone(), calc_mode: Some(crate::CalcMode::Manual), ..Default::default() },
        ).unwrap();
        let id = add(&mut book, NameScope::Workbook, "SelectedMode", "=42");
        let anchor = CellKey::new(book.host().sheets()[0].id(), 1, 1).unwrap();
        let diagnostic = book.host().evaluate_name_with_evidence(id,
            book.host().name_node(id).unwrap().binding_version(), anchor).unwrap();
        assert_eq!(diagnostic.evaluation.outcome, Ok(CalcValue::number(42.)));
        assert_eq!(diagnostic.evaluation.numeric, mode,
            "the requested owner mode is independent of the last cell publication");
        assert_eq!(book.host().engine.nodes().count(), 0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__name_diagnostic_binds_known_udfs_but_never_enters_them() {
        #[derive(Debug)]
        struct Counted(std::rc::Rc<std::cell::Cell<usize>>);
        impl calc_fml::UserFunction for Counted {
            fn call(&self, _: &[CalcValue]) -> Result<CalcValue, String> {
                self.0.set(self.0.get() + 1);
                Ok(CalcValue::number(42.))
            }
        }
        let mut book = book();
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut functions = calc_fml::CallableTable::new();
        functions.register("KnownUdf", std::rc::Rc::new(Counted(calls.clone()))).unwrap();
        book.set_user_functions(functions);
        let anchor = CellKey::new(book.host().sheets()[0].id(), 1, 1).unwrap();
        for (name, formula, refused) in [
            ("DirectCall", "=KnownUdf()", true),
            ("CaughtCall", "=IFERROR(KnownUdf(),99)", true),
            ("UntakenCall", "=IF(FALSE,KnownUdf(),RAND())", false),
        ] {
            let id = add(&mut book, NameScope::Workbook, name, formula);
            let result = book.host().evaluate_name_with_evidence(id,
                book.host().name_node(id).unwrap().binding_version(), anchor).unwrap();
            assert_eq!(result.evaluation.outcome.is_err(), refused, "{formula}: {:?}", result.evaluation.outcome);
            assert!(result.evaluation.root.is_some());
            assert_eq!(result.evaluation.random_draws.samples.len(), usize::from(!refused));
            if !refused {
                assert_eq!(result.evaluation.outcome, Ok(CalcValue::number(
                    f64::from_bits(result.evaluation.random_draws.samples[0]))));
            }
        }
        assert_eq!(calls.get(), 0, "registration metadata grants no executable capability");
        assert_eq!(book.host().engine.nodes().count(), 0);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__detached_context_keeps_read_policy_and_reference_consumption_explicit() {
        use calc_engine::{DetachedEvaluationContext as Context, DetachedFormula, DetachedReadPolicy as Policy};
        let mut book = book();
        let sheet = book.host().sheets()[0].id();
        let at = |row, col| CellKey::new(sheet, row, col).unwrap();
        for (at, text) in [(at(1,1), "1"), (at(1,2), "=A1+1"), (at(3,1), "=SEQUENCE(A1)")] {
            assert!(book.dispatch(SkinIntent::EnterCell { at, text: text.into() }).accepted());
        }
        let name = add(&mut book, NameScope::Workbook, "SelectedReference", "=$B$1");
        let scalar = DetachedFormula::compile(book.host().model(), "$B$1+1", at(8,5)).unwrap();
        let old = book.host().model().pin();
        assert!(book.dispatch(SkinIntent::SetCalcMode(crate::CalcMode::Manual)).accepted());
        assert!(book.dispatch(SkinIntent::EnterCell { at: at(1,1), text: "2".into() }).accepted());
        let revision = book.host().revision();
        let history = book.history_depth();
        let view = book.host().engine.result_view();
        let host = book.host();
        let reference = DetachedFormula::compile_name(host.model(), name, at(8,5)).unwrap();
        let blank = DetachedFormula::compile(host.model(), "$A$4+1", at(8,5)).unwrap();
        let display = Context::new(&host.engine, host.model(), Policy::PublishedValues).unwrap();
        assert!(display.stale_cells().is_none());
        assert_eq!(scalar.evaluate_in(&display, at(8,5)), Ok(CalcValue::number(3.)));
        let current = Context::new(&host.engine, host.model(), Policy::CurrentValues).unwrap();
        for row in 8..40 {
            assert!(scalar.evaluate_in(&current, at(row,5)).is_err());
            assert!(blank.evaluate_in(&current, at(row,5)).is_err());
            assert!(current.read_cell(at(1,2)).is_err());
            assert!(current.read_cell(at(4,1)).is_err());
            assert_eq!(current.read_cell(at(1,1)), Ok(CalcValue::number(2.)));
        }
        let returned = reference.evaluate_in(&current, at(8,5)).unwrap();
        assert!(matches!(returned.core_ref(), CoreRef::Reference(_)), "identity does not consume values");
        assert!(Context::new(&host.engine, &old, Policy::CurrentValues).is_err());
        assert_eq!(host.revision(), revision);
        assert_eq!(host.engine.result_view(), view);
        assert_eq!(book.history_depth(), history);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__detached_plans_refuse_expired_name_bindings() {
        use calc_engine::{DetachedEvaluationContext, DetachedFormula, DetachedReadPolicy};
        let mut book = book();
        let at = CellKey::new(book.host().sheets()[0].id(), 2, 3).unwrap();
        let name = add(&mut book, NameScope::Workbook, "Choice", "=42");
        let version = book.host().name_node(name).unwrap().binding_version();
        let plan = DetachedFormula::compile_name(book.host().model(), name, at).unwrap();
        let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::Name(NameEdit {
            id: Some(name), version: Some(version), scope: NameScope::Workbook,
            name: "Choice".into(), formula: Some("=99".into()),
        })));
        assert!(receipt.accepted(), "{:?}", receipt.error);
        let host = book.host();
        let context = DetachedEvaluationContext::new(&host.engine, host.model(), DetachedReadPolicy::CurrentValues).unwrap();
        assert!(plan.evaluate_in(&context, at).is_err());
        assert_eq!(value(&book, name, at), CalcValue::number(99.));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__detached_context_refuses_changed_file_identity_and_caller_scope() {
        use calc_engine::{DetachedEvaluationContext as Context, DetachedFormula, DetachedReadPolicy as Policy};
        let mut book = book();
        let one = book.host().sheets()[0].id();
        let other = extra_sheet(&mut book, "Other");
        add(&mut book, NameScope::Sheet(one), "Choice", "=42");
        add(&mut book, NameScope::Sheet(other), "Choice", "=99");
        let a = CellKey::new(one, 3, 4).unwrap();
        let b = CellKey::new(other, 3, 4).unwrap();
        let plan = DetachedFormula::compile(book.host().model(), "Choice", a).unwrap();
        {
            let host = book.host();
            let context = Context::new(&host.engine, host.model(), Policy::CurrentValues).unwrap();
            assert_eq!(plan.evaluate_in(&context, a), Ok(CalcValue::number(42.)));
            assert!(plan.evaluate_in(&context, b).is_err());
            let rebound = DetachedFormula::compile(host.model(), "Choice", b).unwrap();
            assert_eq!(rebound.evaluate_in(&context, b), Ok(CalcValue::number(99.)));
        }
        let revision = book.host().revision();
        let history = book.history_depth();
        book.set_file_identity("Renamed.xlsx", false);
        assert_eq!(book.host().revision(), revision);
        assert_eq!(book.history_depth(), history);
        let host = book.host();
        let context = Context::new(&host.engine, host.model(), Policy::CurrentValues).unwrap();
        assert!(plan.evaluate_in(&context, a).is_err(), "same revision does not preserve file bindings");
        let rebound = DetachedFormula::compile(host.model(), "Choice", a).unwrap();
        assert_eq!(rebound.evaluate_in(&context, a), Ok(CalcValue::number(42.)));
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__diagnostics_keep_selected_identity_and_refused_snapshot_evidence() {
        let mut book = book();
        let sheet = book.host().sheets()[0].id();
        let anchor = CellKey::new(sheet, 4, 5).unwrap();
        let global = add(&mut book, NameScope::Workbook, "Choice", "=42");
        let local = add(&mut book, NameScope::Sheet(sheet), "Choice", "=99");
        for (id, expected) in [(global, 42.), (local, 99.)] {
            let version = book.host().name_node(id).unwrap().binding_version();
            let revision = book.host().revision();
            let results = book.host().engine.result_view();
            let history = book.history_depth();
            let diagnostic = book.host().evaluate_name_with_evidence(id, version, anchor).unwrap();
            let root = diagnostic.evaluation.root.as_ref().unwrap();
            assert_eq!((root.locus.definition.id, root.locus.definition.version), (id, version));
            assert_eq!(root.locus.caller, anchor);
            assert_eq!(diagnostic.revision, revision);
            assert_eq!(diagnostic.results, results);
            assert_eq!(diagnostic.evaluation.outcome, Ok(CalcValue::number(expected)));
            assert_eq!(diagnostic.evaluation.omitted, 0);
            let body = &diagnostic.evaluation.invocations[0];
            assert_eq!(diagnostic.evaluation.invocations.len(), 1);
            assert_eq!((body.definition.id, body.definition.version), (id, version));
            assert_eq!(body.context.caller.cell(), anchor);
            assert_eq!((body.ordinal, body.parent), (0, None));
            assert_eq!(body.result, calc_fml::NameResultKind::Number);
            assert_eq!(book.host().engine.result_view(), results);
            assert_eq!(book.history_depth(), history);
            assert!(book.host().name_invocation_view(anchor).is_none());
        }
        assert!(book.dispatch(SkinIntent::SetCalcMode(crate::CalcMode::Manual)).accepted());
        let b1 = CellKey::new(sheet, 1, 2).unwrap();
        assert!(book.dispatch(SkinIntent::EnterCell { at: b1, text: "=2".into() }).accepted());
        let needs_current = add(&mut book, NameScope::Workbook, "NeedsCurrent", "=$B$1+1");
        let version = book.host().name_node(needs_current).unwrap().binding_version();
        let results = book.host().engine.result_view();
        let refused = book.host().evaluate_name_with_evidence(needs_current, version, anchor).unwrap();
        assert!(matches!(&refused.evaluation.outcome,
            Err(calc_engine::NotModeled::Evaluation(calc_fml::EvalFault::NotModelled {
                construct: "a detached formula that reads a formula cell with no published value",
                owner: "dc-6yw",
            }))), "{:?}", refused.evaluation.outcome);
        assert_eq!(refused.evaluation.invocations.len(), 1);
        assert_eq!(refused.evaluation.invocations[0].result, calc_fml::NameResultKind::Refused);
        assert_eq!(book.host().engine.result_view(), results);
        assert!(book.host().name_invocation_view(anchor).is_none());
        assert!(book.dispatch(SkinIntent::SetCalcMode(crate::CalcMode::Automatic)).accepted());
        let a1 = CellKey::new(sheet, 1, 1).unwrap();
        for (at, text) in [(a1, "1"), (b1, "=$A$1+1")] {
            assert!(book.dispatch(SkinIntent::EnterCell { at, text: text.into() }).accepted());
        }
        assert!(book.dispatch(SkinIntent::SetCalcMode(crate::CalcMode::Manual)).accepted());
        assert!(book.dispatch(SkinIntent::EnterCell { at: a1, text: "5".into() }).accepted());
        let page = crate::project(book.host(), &crate::ProjectionRequest { cells: vec![b1] }).unwrap();
        assert!(matches!(page.cells()[0].value(), ProjectedValue::Stale(_)));
        let results = book.host().engine.result_view();
        let stale = book.host().evaluate_name_with_evidence(needs_current, version, anchor).unwrap();
        assert!(stale.evaluation.outcome.is_err(), "a dirty formula cache cannot supply a diagnostic calculation");
        assert_eq!(stale.evaluation.invocations.len(), 1);
        assert_eq!(stale.evaluation.invocations[0].result, calc_fml::NameResultKind::Refused);
        assert_eq!(book.host().engine.result_view(), results);
        let error = add(&mut book, NameScope::Workbook, "WorksheetFault", "=1/0");
        let version = book.host().name_node(error).unwrap().binding_version();
        let fault = book.host().evaluate_name_with_evidence(error, version, anchor).unwrap();
        assert_eq!(fault.evaluation.outcome, Ok(CalcValue::error(calc_value::WorksheetErrorCode::Div0)));
        assert_eq!(fault.evaluation.invocations[0].result, calc_fml::NameResultKind::Error(calc_value::WorksheetErrorCode::Div0));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_hsa5_6__diagnostics_refuse_unresolved_spill_expansion_blanks() {
        for replace_formula in [false, true] {
            let mut book = book();
            let sheet = book.host().sheets()[0].id();
            let at = |row, column| CellKey::new(sheet, row, column).unwrap();
            for (at, text) in [(at(1,2), "1"), (at(1,1), "=SEQUENCE(B1)")] {
                assert!(book.dispatch(SkinIntent::EnterCell { at, text: text.into() }).accepted());
            }
            let scalar = add(&mut book, NameScope::Workbook, "ExpansionScalar", "=$A$2+1");
            let area = add(&mut book, NameScope::Workbook, "ExpansionArea", "=SUM($A$2:$A$3)+1");
            let safe = add(&mut book, NameScope::Workbook, "BlockedCorridor", "=$C$1+1");
            assert!(book.dispatch(SkinIntent::SetCalcMode(crate::CalcMode::Manual)).accepted());
            let (target, text) = if replace_formula { (at(1,1), "=SEQUENCE(2)") } else { (at(1,2), "2") };
            assert!(book.dispatch(SkinIntent::EnterCell { at: target, text: text.into() }).accepted());
            let results = book.host().engine.result_view();
            for id in [scalar, area] {
                let version = book.host().name_node(id).unwrap().binding_version();
                let diagnostic = book.host().evaluate_name_with_evidence(id, version, at(4,5)).unwrap();
                assert!(diagnostic.evaluation.outcome.is_err(), "pending expansion, replace={replace_formula}: {:?}", diagnostic.evaluation.outcome);
                assert_eq!(diagnostic.evaluation.invocations.len(), 1);
                assert_eq!(diagnostic.evaluation.invocations[0].result, calc_fml::NameResultKind::Refused);
                assert_eq!(book.host().engine.result_view(), results);
            }
            assert_eq!(value(&book, safe, at(4,5)), CalcValue::number(1.));
            assert!(book.dispatch(SkinIntent::SetCalcMode(crate::CalcMode::Automatic)).accepted());
            assert_eq!(value(&book, scalar, at(4,5)), CalcValue::number(3.));
            assert_eq!(value(&book, area, at(4,5)), CalcValue::number(3.));
        }
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
