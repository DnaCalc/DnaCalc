//! Author bounded dependency slices from mismatching cells. The source Excel
//! answer is an inherited observation, never an input or a new capture. A DNA
//! reproduction check cannot establish that a reduced case still fails in Excel.
use super::compare_triage::{Retained, RunEntry, value_json};
use calc_host::value::{CalcValue, CellKey, CoreValue, GridArea, NumericMode, SheetId};
use calc_host::{
    ArrayFormula, CellEdit, CellInput, CellText, FiniteNumber, FormulaText, HostOptions,
    HostProgress, HostSession, HostState, MemorySource, ModelRoot, OpenPolicy, ProjectedValue,
    ProjectionRequest, SavePolicy, SheetChange, SkinIntent, WorkbookCommand,
};
use calc_verdict::{CaptureRecord, CellAddress, Verdict, VerdictReport, compare, sha256_hex};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::Path,
    sync::Arc,
};

const MAX_INPUTS: usize = 256;
const MAX_FORMULAS: usize = 32;

#[derive(Default)]
struct Recipe {
    inputs: BTreeMap<CellKey, CellInput>,
    arrays: BTreeMap<CellKey, ArrayFormula>,
    sheets: BTreeSet<SheetId>,
    frozen: BTreeSet<CellKey>,
    issues: BTreeSet<String>,
}
fn literal(value: &CalcValue) -> Option<CellInput> {
    match value.core() {
        CoreValue::Number(v) => FiniteNumber::new(*v).map(CellInput::Number),
        CoreValue::Text(v) => CellText::new(v.clone()).map(CellInput::Text),
        CoreValue::Logical(v) => Some(CellInput::Logical(*v)),
        CoreValue::Error(v) => Some(CellInput::Error(*v)),
        _ => None,
    }
}
fn key(model: &ModelRoot, address: &CellAddress) -> Option<CellKey> {
    let sheet = model
        .sheets()
        .iter()
        .find(|s| s.name().text().to_string_lossy() == address.sheet())?;
    CellKey::new(sheet.id(), address.row(), address.column()).ok()
}
fn address(model: &ModelRoot, key: CellKey) -> Option<CellAddress> {
    CellAddress::new(
        &model
            .sheet_by_id(key.sheet())?
            .name()
            .text()
            .to_string_lossy(),
        key.row(),
        key.column(),
    )
}
fn recipe(
    model: &ModelRoot,
    target: CellKey,
    differing: &BTreeSet<CellAddress>,
    reads: &HashMap<CellAddress, Retained>,
) -> Recipe {
    let mut recipe = Recipe::default();
    let mut pending = vec![target];
    let mut visited = BTreeSet::new();
    while let Some(mut at) = pending.pop() {
        if let Some(array) = model.definitions().array_containing(at) {
            if array.area().cell_count() > MAX_INPUTS as u64 {
                recipe
                    .issues
                    .insert("source array exceeds reduction cell bound".into());
                continue;
            }
            at = array.anchor();
            recipe.arrays.insert(at, *array);
        }
        if !visited.insert(at) {
            continue;
        }
        if visited.len() > MAX_FORMULAS || recipe.inputs.len() >= MAX_INPUTS {
            recipe
                .issues
                .insert("dependency slice exceeds reduction bound".into());
            break;
        }
        let Some(CellInput::Formula(text)) = model.cell(at) else {
            recipe
                .issues
                .insert("target or mismatching producer has no authored formula".into());
            continue;
        };
        let formula =
            FormulaText::with_form(text.rendered(), text.form()).expect("existing formula text");
        recipe.inputs.insert(at, CellInput::Formula(formula));
        let Some(kept) = address(model, at).and_then(|a| reads.get(&a)) else {
            recipe
                .issues
                .insert("evaluation read trace unavailable".into());
            continue;
        };
        for area in &kept.precedents {
            recipe.sheets.insert(area.sheet());
            let sheet = model
                .sheet_by_id(area.sheet())
                .map(|s| s.name().text().to_string_lossy());
            let covered = kept.reads.iter().any(|read| {
                sheet.as_deref() == Some(read.sheet.as_str())
                    && read.top <= area.first().row()
                    && read.left <= area.first().column()
                    && read.bottom >= area.last().row()
                    && read.right >= area.last().column()
            });
            // A spill's extent depends on its anchor even when ROWS/COLUMNS
            // reads no values. Retain that producer instead of freezing its
            // anchor to a scalar and losing the shape. Recorded value reads
            // continue to use the existing bounded backward slice below.
            let first = area.first();
            if !covered && matches!(model.cell(first), Some(CellInput::Formula(_))) {
                pending.push(first);
            }
        }
        for read in &kept.reads {
            if read.omitted != 0 {
                recipe
                    .issues
                    .insert("evaluation read trace was truncated".into());
            }
            let Some(sheet) = model
                .sheets()
                .iter()
                .find(|s| s.name().text().to_string_lossy() == read.sheet)
            else {
                recipe.issues.insert("read sheet unavailable".into());
                continue;
            };
            let width = u64::from(read.right - read.left + 1);
            let mut offset = 0u64;
            for run in &read.runs {
                match run {
                    RunEntry::Blank(n) => offset = offset.saturating_add(*n),
                    RunEntry::Missing(n) => {
                        offset = offset.saturating_add(*n);
                        recipe.issues.insert("unsupplied evaluation read".into());
                    }
                    RunEntry::Value(value) => {
                        let row = u64::from(read.top) + offset / width;
                        let column = u64::from(read.left) + offset % width;
                        offset += 1;
                        let other = u32::try_from(row)
                            .ok()
                            .zip(u32::try_from(column).ok())
                            .and_then(|(r, c)| CellKey::new(sheet.id(), r, c).ok());
                        let Some(other) = other else {
                            recipe.issues.insert("read coordinate unavailable".into());
                            continue;
                        };
                        if address(model, other).is_some_and(|a| differing.contains(&a)) {
                            pending.push(other);
                            continue;
                        }
                        if recipe.inputs.len() >= MAX_INPUTS {
                            recipe
                                .issues
                                .insert("dependency slice exceeds reduction bound".into());
                            break;
                        }
                        let input = match model.cell(other) {
                            Some(input) if !matches!(input, CellInput::Formula(_)) => {
                                Some(input.clone())
                            }
                            _ => {
                                recipe.frozen.insert(other);
                                literal(value)
                            }
                        };
                        if let Some(input) = input {
                            if let Some(previous) = recipe.inputs.insert(other, input.clone())
                                && previous != input
                            {
                                recipe.issues.insert(
                                    "one invocation observed differing values at one address"
                                        .into(),
                                );
                            }
                        } else if !matches!(value.core(), CoreValue::Empty) {
                            recipe
                                .issues
                                .insert("nonscalar read cannot be authored as a cell".into());
                        }
                    }
                }
            }
        }
    }
    recipe
}

fn execute(
    model: &ModelRoot,
    recipe: &Recipe,
    target: CellKey,
    capture: &CaptureRecord,
    numeric: &NumericMode,
) -> Result<(Vec<u8>, Option<CalcValue>), String> {
    let mut needed: BTreeSet<_> = recipe.inputs.keys().map(|at| at.sheet()).collect();
    needed.extend(&recipe.sheets);
    needed.insert(target.sheet());
    let mut host = HostState::open(
        Arc::new(MemorySource::new(calc_host::blank_package())),
        &OpenPolicy::default(),
        HostOptions {
            numeric: numeric.clone(),
            calc_mode: Some(calc_host::CalcMode::Manual),
            locale: super::compare::capture_locale(capture),
            formula_cell_budget: None,
        },
    )
    .map_err(|e| e.to_string())?;
    host.set_clock(super::compare::capture_clock(capture));
    host.set_file(super::compare::capture_file(capture));
    host.set_application(super::compare::capture_application(capture));
    let mut book = HostSession::new(host);
    let mut sheets = BTreeMap::<SheetId, SheetId>::new();
    for source in model
        .sheets()
        .iter()
        .filter(|sheet| needed.contains(&sheet.id()))
    {
        let change = if sheets.is_empty() {
            SheetChange::Rename {
                name: source.name().text().to_string_lossy(),
            }
        } else {
            SheetChange::Add {
                name: source.name().text().to_string_lossy(),
            }
        };
        let sheet = if sheets.is_empty() {
            Some(book.host().sheets()[0].id())
        } else {
            None
        };
        let receipt = book.dispatch(SkinIntent::Command(WorkbookCommand::Sheet {
            sheet,
            change,
        }));
        if !receipt.accepted() {
            return Err(format!("case sheet: {:?}", receipt.error));
        }
        let id = book
            .host()
            .sheets()
            .iter()
            .find(|s| s.name() == source.name())
            .ok_or("authored sheet unavailable")?
            .id();
        sheets.insert(source.id(), id);
    }
    let placed = |at: CellKey| {
        CellKey::new(
            *sheets.get(&at.sheet()).expect("included sheet"),
            at.row(),
            at.column(),
        )
        .expect("source coordinate")
    };
    let mut edits = vec![CellEdit::SetIteration(model.definitions().iteration())];
    for (at, input) in &recipe.inputs {
        let at_new = placed(*at);
        if let Some(array) = recipe.arrays.get(at)
            && let CellInput::Formula(formula) = input
        {
            let area =
                GridArea::new(at_new, placed(array.area().last())).map_err(|e| e.to_string())?;
            edits.push(CellEdit::SetArray {
                array: ArrayFormula::new(area, array.kind()),
                formula: formula.clone(),
            });
        } else {
            edits.push(CellEdit::Set {
                at: at_new,
                input: input.clone(),
            });
        }
    }
    let receipt = book.dispatch(SkinIntent::Edit { edits });
    if !receipt.accepted() {
        return Err(format!("case inputs: {:?}", receipt.error));
    }
    let receipt = book.dispatch(SkinIntent::Rebuild);
    if !receipt.accepted() {
        return Err(format!("case calculation: {:?}", receipt.error));
    }
    let mut steps = 0;
    while book.has_pending_calculation() && steps < 1024 {
        if matches!(
            book.advance_calculation(),
            HostProgress::Waiting { .. } | HostProgress::Failed(_)
        ) {
            break;
        }
        steps += 1;
    }
    let projected = calc_host::project(
        book.host(),
        &ProjectionRequest {
            cells: vec![placed(target)],
        },
    )
    .map_err(|e| e.to_string())?;
    let actual = match projected.cells()[0].value() {
        ProjectedValue::Calculated { value, calculation }
            if !calculation.held_by_circular_rule() && !calculation.reads_external_links() =>
        {
            let legacy = model
                .definitions()
                .array_containing(target)
                .is_some_and(|array| array.kind() == calc_host::ArrayKind::Legacy);
            Some(if legacy {
                value.clone()
            } else {
                super::compare::spilled_array(book.host(), placed(target))
                    .unwrap_or_else(|| value.clone())
            })
        }
        _ => None,
    };
    let mut bytes = Vec::new();
    calc_host::save(book.host(), &SavePolicy::default(), &mut bytes).map_err(|e| e.to_string())?;
    Ok((bytes, actual))
}

pub(crate) fn write_cases(
    out: &Path,
    capture: &CaptureRecord,
    report: &VerdictReport,
    retained: &HashMap<CellAddress, Retained>,
    model: Option<&ModelRoot>,
    numeric: &NumericMode,
) -> Result<(), String> {
    let dir = out.join("cases");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let differing: BTreeSet<_> = report
        .cells
        .iter()
        .filter(|c| matches!(c.verdict, Verdict::Mismatched { .. }))
        .map(|c| c.formula.cell.clone())
        .collect();
    let mut cases = Vec::new();
    for (i, cell) in report
        .cells
        .iter()
        .filter(|c| matches!(c.verdict, Verdict::Mismatched { .. }))
        .enumerate()
    {
        let Verdict::Mismatched {
            expected, actual, ..
        } = &cell.verdict
        else {
            unreachable!()
        };
        let address = &cell.formula.cell;
        let mut metadata = json!({"sheet":address.sheet(),"row":address.row(),"column":address.column(),
            "source_reference_observation":value_json(expected),"source_dna_value":value_json(actual),
            "reference_is_live_excel":matches!(cell.observation,Some(calc_verdict::Observation::ExcelRaw(_))),
            "excel_confirmation":"pending; inherited source observation is not a capture of this case"});
        if let Some(model) = model
            && let Some(target) = key(model, address)
        {
            let recipe = recipe(model, target, &differing, retained);
            metadata["input_cells"] = json!(recipe.inputs.len());
            metadata["frozen_matching_or_noninventory_reads"] = json!(recipe.frozen.len());
            metadata["limitations"] = json!(recipe.issues);
            match execute(model, &recipe, target, capture, numeric) {
                Ok((bytes, replayed)) => {
                    let name = format!("cell-{i:06}.xlsx");
                    fs::write(dir.join(&name), &bytes).map_err(|e| e.to_string())?;
                    metadata["artifact"] = json!(name);
                    metadata["sha256"] = json!(sha256_hex(&bytes));
                    metadata["dna_reproduced"] = json!(
                        replayed
                            .as_ref()
                            .is_some_and(|v| compare(actual, v).is_none())
                            && recipe.issues.is_empty()
                    );
                    metadata["reduced_dna_value"] =
                        replayed.as_ref().map(value_json).unwrap_or(Value::Null);
                }
                Err(reason) => {
                    metadata["dna_reproduced"] = json!(false);
                    metadata["failure"] = json!(reason);
                }
            }
        } else {
            metadata["failure"] = json!("source model or target unavailable");
            metadata["dna_reproduced"] = json!(false);
        }
        cases.push(metadata);
    }
    let manifest = json!({"format":"dnacalc-reduced-cases","version":1,"source":capture.source,
        "method":"bounded backward slices; retain mismatching formula producers, freeze matching/noninventory observed inputs; preserve coordinates and formula form",
        "rule":"DNA reproduction is checked through HostSession; every authored case needs independent Excel recalculation before its mismatch is established; source Excel answers are never authored inputs",
        "cases":cases});
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::super::compare_triage::Read;
    use super::*;
    #[test]
    fn reductions_retain_full_spills_and_shape_only_producers() {
        let mut book = HostSession::open(
            Arc::new(MemorySource::new(calc_host::blank_package())),
            &OpenPolicy::default(),
            HostOptions {
                calc_mode: Some(calc_host::CalcMode::Manual),
                ..Default::default()
            },
        )
        .unwrap();
        let sheet = book.host().sheets()[0].id();
        let anchor = CellKey::new(sheet, 1, 1).unwrap();
        let observer = CellKey::new(sheet, 4, 2).unwrap();
        for (at, text) in [(anchor, "=HSTACK(1,2)"), (observer, "=COLUMNS(A1#)")] {
            assert!(
                book.dispatch(SkinIntent::EnterCell {
                    at,
                    text: text.into()
                })
                .accepted()
            );
        }
        assert!(book.dispatch(SkinIntent::Rebuild).accepted());
        while book.has_pending_calculation() {
            book.advance_calculation();
        }
        assert_eq!(book.host().spill_area(anchor).unwrap().cell_count(), 2);
        let mut bytes = Vec::new();
        calc_host::save(book.host(), &SavePolicy::default(), &mut bytes).unwrap();
        let capture =
            crate::compare_inventory::source_record(&bytes, "spill-reduction-control").unwrap();
        let model = book.host().model();
        let mut reads = HashMap::from([
            (address(model, anchor).unwrap(), Retained::default()),
            (address(model, observer).unwrap(), Retained::default()),
        ]);
        reads
            .get_mut(&address(model, observer).unwrap())
            .unwrap()
            .precedents = book.host().precedents(observer).unwrap();
        let differing = BTreeSet::from([
            address(model, anchor).unwrap(),
            address(model, observer).unwrap(),
        ]);
        let recipe = recipe(model, anchor, &differing, &reads);
        let (_, value) = execute(model, &recipe, anchor, &capture, &NumericMode::Fast).unwrap();
        let expected = CalcValue::array(
            calc_host::value::CalcArray::new(
                calc_host::value::ArrayShape { rows: 1, cols: 2 },
                vec![CalcValue::number(1.), CalcValue::number(2.)],
            )
            .unwrap(),
        );
        assert_eq!(
            value,
            Some(expected),
            "compare the whole spill, including every element"
        );
        let recipe = super::recipe(model, observer, &differing, &reads);
        assert!(
            recipe.inputs.contains_key(&anchor),
            "a shape-only read still needs its spill producer"
        );
        let (_, value) = execute(model, &recipe, observer, &capture, &NumericMode::Fast).unwrap();
        assert_eq!(value, Some(CalcValue::number(2.)));
    }

    #[test]
    fn reductions_keep_differing_producers_and_reopen_through_the_product_route() {
        let mut book = HostSession::open(
            Arc::new(MemorySource::new(calc_host::blank_package())),
            &OpenPolicy::default(),
            HostOptions {
                calc_mode: Some(calc_host::CalcMode::Manual),
                ..Default::default()
            },
        )
        .unwrap();
        let sheet = book.host().sheets()[0].id();
        let at = |c| CellKey::new(sheet, 1, c).unwrap();
        for (c, text) in [(1, "1"), (2, "=A1+1"), (3, "=B1+1")] {
            assert!(
                book.dispatch(SkinIntent::EnterCell {
                    at: at(c),
                    text: text.into()
                })
                .accepted()
            );
        }
        assert!(book.dispatch(SkinIntent::Rebuild).accepted());
        let mut original = Vec::new();
        calc_host::save(book.host(), &SavePolicy::default(), &mut original).unwrap();
        let capture =
            crate::compare_inventory::source_record(&original, "authored-reducer-control").unwrap();
        let model = book.host().model();
        let b = address(model, at(2)).unwrap();
        let c = address(model, at(3)).unwrap();
        let mut reads = HashMap::new();
        for (target, column, value) in [(b.clone(), 1, 1.0), (c.clone(), 2, 2.0)] {
            reads.insert(
                target,
                Retained {
                    calls: Vec::new(),
                    precedents: Vec::new(),
                    reads: vec![Read {
                        sheet: "Sheet1".into(),
                        top: 1,
                        left: column,
                        bottom: 1,
                        right: column,
                        cells: 1,
                        runs: vec![RunEntry::Value(CalcValue::number(value))],
                        omitted: 0,
                    }],
                },
            );
        }
        for both_differ in [true, false] {
            let differing = if both_differ {
                BTreeSet::from([b.clone(), c.clone()])
            } else {
                BTreeSet::from([c.clone()])
            };
            let recipe = recipe(model, at(3), &differing, &reads);
            assert!(recipe.issues.is_empty());
            if both_differ {
                assert!(matches!(recipe.inputs[&at(2)], CellInput::Formula(_)));
                assert_eq!(recipe.inputs.len(), 3);
            } else {
                assert!(matches!(recipe.inputs[&at(2)], CellInput::Number(_)));
                assert!(!recipe.inputs.contains_key(&at(1)));
            }
            let (bytes, value) =
                execute(model, &recipe, at(3), &capture, &NumericMode::Fast).unwrap();
            assert_eq!(value.unwrap(), CalcValue::number(3.0));
            let opened = HostState::open(
                Arc::new(MemorySource::new(bytes)),
                &OpenPolicy::default(),
                HostOptions::default(),
            )
            .unwrap();
            let target = CellKey::new(opened.sheets()[0].id(), 1, 3).unwrap();
            assert!(matches!(
                opened.model().cell(target),
                Some(CellInput::Formula(_))
            ));
            let view = calc_host::project(
                &opened,
                &ProjectionRequest {
                    cells: vec![target],
                },
            )
            .unwrap();
            assert!(
                matches!(view.cells()[0].value(), ProjectedValue::FileCached(_)),
                "opening the authored artifact cannot score its saved answer"
            );
        }
    }
}
