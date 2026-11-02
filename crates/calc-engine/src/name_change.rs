//! Identity-bound name changes, prepared through the canonical formula compiler.
use crate::{DetachedFormula, EngineState, read::SnapshotCatalog};
use calc_doc_model::{CellInput, FormulaText, ModelRoot, OpBatch, WorkbookOp};
use calc_fml::reference::{FormulaAnchor, ReferenceNotation};
use calc_fml::{NameAnalysis, PlanCache};
use calc_value::{CellKey, ExcelText, NameId, NameKey, NameScope};
use std::collections::{BTreeMap, BTreeSet};

pub fn valid_defined_name(name: &str) -> bool {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameChangeRequest {
    pub id: NameId,
    pub version: u64,
    pub scope: NameScope,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameChangeFailure {
    StaleIdentity,
    InvalidName,
    UnavailableSheet,
    Conflict,
    NotModelled(&'static str),
}
impl std::fmt::Display for NameChangeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reason = match self {
            Self::InvalidName => "Use a valid Excel name with no spaces or cell-reference spelling",
            Self::StaleIdentity => "The selected name changed; refresh before editing it",
            Self::UnavailableSheet => "The destination worksheet is unavailable",
            Self::Conflict => "The destination name already exists in that scope",
            Self::NotModelled(reason) => reason,
        };
        write!(f, "{reason} (dc-hsa5.6)")
    }
}
impl std::error::Error for NameChangeFailure {}

/// Candidate text only, never computed INDIRECT strings or cell addresses.
pub fn mentions_name(text: &str, spelling: &str) -> bool {
    calc_fml::syntax::lex(text).iter().any(|token| {
        token.kind() == calc_fml::syntax::TokenKind::Identifier
            && calc_value::workbook_names_equal(token.text(), spelling)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Cell(CellKey),
    Name(NameId),
    Rule(calc_value::SheetId, usize, usize),
}
struct Checked {
    source: Source,
    anchor: CellKey,
    before: NameAnalysis,
}
fn push_source(
    sources: &mut Vec<(Source, CellKey)>,
    source: (Source, CellKey),
) -> Result<(), NameChangeFailure> {
    if sources.len() == 100_000 {
        return Err(NameChangeFailure::NotModelled(
            "This name change exceeds the bounded source analysis",
        ));
    }
    sources.push(source);
    Ok(())
}
fn owner(source: Source) -> Option<NameId> {
    match source {
        Source::Name(id) => Some(id),
        _ => None,
    }
}
fn text_at(model: &ModelRoot, source: Source) -> Option<String> {
    match source {
        Source::Cell(at) => match model.cell(at)? {
            CellInput::Formula(text) => String::from_utf16(text.text().utf16_code_units()).ok(),
            _ => None,
        },
        Source::Name(id) => Some(
            model
                .definitions()
                .name_table()
                .get(id)?
                .definition()
                .formula
                .clone(),
        ),
        Source::Rule(sheet, rule, formula) => Some(
            model
                .definitions()
                .conditional_rules(sheet)
                .get(rule)?
                .formulas
                .get(formula)?
                .clone(),
        ),
    }
}
impl EngineState {
    /// Prepare one authored batch. Only compiler-resolved occurrences of this identity
    /// change. A context-dependent spelling that cannot preserve every binding refuses.
    pub fn prepare_name_change(
        &self,
        model: &ModelRoot,
        request: &NameChangeRequest,
    ) -> Result<Vec<WorkbookOp>, NameChangeFailure> {
        use NameChangeFailure as Error;
        if self.revision() != model.revision() {
            return Err(Error::StaleIdentity);
        }
        if !valid_defined_name(&request.name) {
            return Err(Error::InvalidName);
        }
        let table = model.definitions().name_table();
        let selected = table
            .get(request.id)
            .filter(|node| node.binding_version() == request.version)
            .ok_or(Error::StaleIdentity)?;
        if let NameScope::Sheet(sheet) = request.scope {
            if model.sheet_by_id(sheet).is_none() {
                return Err(Error::UnavailableSheet);
            }
        }
        let new_key = NameKey::new(request.scope, &request.name);
        if table
            .lookup_key(&new_key)
            .is_some_and(|node| node.id() != request.id)
            || model
                .definitions()
                .tables()
                .iter()
                .any(|t| calc_value::workbook_names_equal(&t.name, &request.name))
        {
            return Err(Error::Conflict);
        }
        if !model.definitions().tables().is_empty() {
            return Err(Error::NotModelled(
                "Table formula metadata is not modelled for name rewriting",
            ));
        }
        if selected.definition().function
            || selected.definition().vb_procedure
            || selected.definition().xlm
        {
            return Err(Error::NotModelled(
                "A macro name cannot be renamed as an ordinary formula name",
            ));
        }
        let old_key = selected.key().clone();
        let old_name = &selected.definition().name;
        let relevant =
            |text: &str| mentions_name(text, old_name) || mentions_name(text, &request.name);
        let mut sources = Vec::new();
        for sheet in model.sheets() {
            for (at, input) in model.sheet_cells(sheet.id()).into_iter().flatten() {
                if let CellInput::Formula(text) = input {
                    let text =
                        String::from_utf16(text.text().utf16_code_units()).map_err(|_| {
                            Error::NotModelled("A formula source has unpaired UTF-16 units")
                        })?;
                    if relevant(&text) {
                        push_source(&mut sources, (Source::Cell(at), at))?;
                    }
                }
            }
            for (r, rule) in model
                .definitions()
                .conditional_rules(sheet.id())
                .iter()
                .enumerate()
            {
                for (f, text) in rule.formulas.iter().enumerate() {
                    if relevant(text) {
                        push_source(
                            &mut sources,
                            (
                                Source::Rule(sheet.id(), r, f),
                                rule.origin().ok_or(Error::NotModelled(
                                    "A conditional rule has no evaluation anchor",
                                ))?,
                            ),
                        )?;
                    }
                }
            }
        }
        for node in table.nodes() {
            if node.id() != request.id && !relevant(&node.definition().formula) {
                continue;
            }
            for sheet in model.sheets() {
                if matches!(node.definition().scope, NameScope::Sheet(local) if local != sheet.id())
                {
                    continue;
                }
                push_source(
                    &mut sources,
                    (
                        Source::Name(node.id()),
                        CellKey::new(sheet.id(), 1, 1).expect("A1 is in the grid"),
                    ),
                )?;
            }
        }
        let mut cache = PlanCache::new();
        let analyze = |model: &ModelRoot,
                       source: Source,
                       anchor: CellKey,
                       cache: &mut PlanCache|
         -> Result<NameAnalysis, Error> {
            if let Source::Name(id) = source {
                return DetachedFormula::analyze_name(model, id, anchor, &self.callables, cache)
                    .map_err(|_| {
                        Error::NotModelled("A name body cannot be checked in every nested scope")
                    });
            }
            let text = text_at(model, source)
                .ok_or(Error::NotModelled("A formula source is unavailable"))?;
            let form = match source {
                Source::Cell(at) => match model.cell(at) {
                    Some(CellInput::Formula(text)) => text.form(),
                    _ => unreachable!(),
                },
                _ => calc_fml::FormulaForm::Legacy,
            };
            calc_fml::analyze_loaded_names(
                &calc_fml::parse_formula(&format!("={}", text.strip_prefix('=').unwrap_or(&text))),
                ReferenceNotation::A1,
                FormulaAnchor::new(anchor),
                &SnapshotCatalog::new(model),
                cache,
                form,
                &self.callables,
            )
            .map_err(|_| {
                Error::NotModelled("A referring formula cannot be checked for name capture")
            })
        };
        let mut checked = Vec::new();
        let mut occurrences: BTreeMap<Source, BTreeMap<(usize, usize), BTreeSet<Option<NameId>>>> =
            BTreeMap::new();
        for (source, anchor) in sources {
            let before = analyze(model, source, anchor, &mut cache)?;
            for usage in &before.uses {
                if usage.context.definition == owner(source) {
                    occurrences
                        .entry(source)
                        .or_default()
                        .entry((usage.span.start, usage.span.end()))
                        .or_default()
                        .insert(usage.resolved.map(|name| name.id));
                }
            }
            checked.push(Checked {
                source,
                anchor,
                before,
            });
        }
        let replacement = match request.scope {
            NameScope::Workbook => request.name.clone(),
            NameScope::Sheet(sheet) => format!(
                "'{}'!{}",
                model
                    .sheet_by_id(sheet)
                    .ok_or(Error::UnavailableSheet)?
                    .name()
                    .text()
                    .to_string_lossy()
                    .replace('\'', "''"),
                request.name
            ),
        };
        let mut rewrites = BTreeMap::new();
        for (source, sites) in occurrences {
            let mut text: Vec<u16> = format!(
                "={}",
                text_at(model, source)
                    .ok_or(Error::NotModelled("A formula source is unavailable"))?
                    .trim_start_matches('=')
            )
            .encode_utf16()
            .collect();
            let mut changed = false;
            for ((start, end), bindings) in sites.into_iter().rev() {
                if !bindings.contains(&Some(request.id)) {
                    continue;
                }
                if bindings.len() != 1 {
                    return Err(Error::NotModelled(
                        "A nested name occurrence binds different identities in different caller scopes; qualify it before renaming",
                    ));
                }
                text.splice(start..end, replacement.encode_utf16());
                changed = true;
            }
            if changed {
                rewrites.insert(
                    source,
                    String::from_utf16(&text[1..])
                        .map_err(|_| Error::NotModelled("A rewritten name source is not UTF-16"))?,
                );
            }
        }
        let mut keys: Vec<_> = rewrites
            .keys()
            .filter_map(|source| match source {
                Source::Name(id) => table.get(*id).map(|node| node.key().clone()),
                _ => None,
            })
            .collect();
        keys.push(old_key.clone());
        keys.push(new_key.clone());
        keys.sort();
        keys.dedup();
        let mut names = table.snapshot_keys(&keys, true);
        let by_origin: std::collections::HashMap<_, _> = keys
            .iter()
            .flat_map(|key| table.matching(key))
            .map(|node| (node.origin(), node.id()))
            .collect();
        let mut moved = None;
        for snapshot in &mut names {
            for record in &mut snapshot.records {
                if let Some(id) = by_origin.get(&record.origin) {
                    if let Some(text) = rewrites.get(&Source::Name(*id)) {
                        record.definition.formula = text.clone();
                    }
                }
            }
            if snapshot.key == old_key {
                let position = snapshot
                    .records
                    .iter()
                    .position(|record| record.origin == selected.origin())
                    .ok_or(Error::StaleIdentity)?;
                let mut record = snapshot.records.remove(position);
                record.definition.name = request.name.clone();
                record.definition.scope = request.scope;
                moved = Some(record);
            }
        }
        names
            .iter_mut()
            .find(|snapshot| snapshot.key == new_key)
            .ok_or(Error::StaleIdentity)?
            .records
            .push(moved.ok_or(Error::StaleIdentity)?);
        for snapshot in &mut names {
            snapshot.records.sort_by_key(|record| record.position);
        }
        let mut operations = vec![WorkbookOp::RestoreNames(names)];
        let mut rules = BTreeMap::new();
        for (source, text) in rewrites {
            match source {
                Source::Cell(at) => {
                    let Some(CellInput::Formula(held)) = model.cell(at) else {
                        return Err(Error::NotModelled("A referring cell lost its formula"));
                    };
                    let input = CellInput::Formula(
                        FormulaText::with_form(
                            ExcelText::from_utf16_code_units(text.encode_utf16().collect()),
                            held.form(),
                        )
                        .map_err(|_| Error::NotModelled("The rewritten formula is empty"))?,
                    );
                    operations.push(if let Some(array) = model.definitions().array(at) {
                        WorkbookOp::SetArray {
                            array: *array,
                            input,
                        }
                    } else {
                        WorkbookOp::SetCell { at, input }
                    });
                }
                Source::Rule(sheet, r, f) => {
                    let rules = rules
                        .entry(sheet)
                        .or_insert_with(|| model.definitions().conditional_rules(sheet).to_vec());
                    rules[r].formulas[f] = text;
                    rules[r].authored = true;
                }
                Source::Name(_) => {}
            }
        }
        operations.extend(
            rules
                .into_iter()
                .map(|(sheet, rules)| WorkbookOp::SetConditionalRules { sheet, rules }),
        );
        let candidate = model
            .apply(&OpBatch {
                base: model.revision(),
                operations: operations.clone(),
            })
            .map_err(|_| Error::NotModelled("The name change is not one valid authored batch"))?;
        for check in checked {
            let after = analyze(&candidate, check.source, check.anchor, &mut cache)?;
            let ids = |analysis: &NameAnalysis| {
                analysis
                    .plan
                    .name_bindings()
                    .iter()
                    .map(|proof| proof.resolved.map(|name| name.id))
                    .collect::<Vec<_>>()
            };
            if ids(&check.before) != ids(&after) {
                return Err(Error::NotModelled(
                    "The name change would capture a different name in a nested scope",
                ));
            }
            let areas = |analysis: &NameAnalysis| {
                analysis
                    .plan
                    .references()
                    .iter()
                    .map(|reference| {
                        reference.instantiate_wrapped(FormulaAnchor::new(check.anchor))
                    })
                    .collect::<Vec<_>>()
            };
            if areas(&check.before) != areas(&after) {
                return Err(Error::NotModelled(
                    "The scope move would change cell references; qualify the definition before moving it",
                ));
            }
        }
        Ok(operations)
    }
}
