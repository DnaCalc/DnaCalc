//! Optional source regrouping through the existing shared-reference occurrence renderer.
use crate::load::sheet::SharedSource;
use calc_doc_model::value::{CellKey, ExcelText, GridArea, SheetId};
use calc_doc_model::{
    CellInput, FormulaBlock, FormulaBlockId, FormulaForm, FormulaSourceStats, FormulaText,
    ModelRoot, SemanticTemplateKey, TemplatePlacement,
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug)]
pub struct FormulaSourceBudget {
    /// Occupied cells inspected, including constants and ineligible formulas.
    pub max_cells: usize,
    /// UTF-16 source units normalized, parsed or checked before the storage copy.
    pub max_utf16_units: usize,
}
impl Default for FormulaSourceBudget {
    fn default() -> Self {
        Self {
            max_cells: 100_000,
            max_utf16_units: 8 * 1024 * 1024,
        }
    }
}
#[derive(Debug)]
pub struct RegroupedFormulaSources {
    pub model: Arc<ModelRoot>,
    pub before: FormulaSourceStats,
    pub after: FormulaSourceStats,
    pub regrouped_cells: usize,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SourceIdentity {
    Block(usize),
    Cell(CellKey),
}
struct Part {
    block: Option<Arc<FormulaBlock>>,
    origin: CellKey,
    source: ExcelText,
    members: Vec<CellKey>,
}
fn charge(units: &mut usize, added: usize, budget: FormulaSourceBudget) -> Option<()> {
    *units = units.checked_add(added)?;
    (*units <= budget.max_utf16_units).then_some(())
}
fn formula(model: &ModelRoot, at: CellKey) -> Option<&FormulaText> {
    match model.cell(at)? {
        CellInput::Formula(text) => Some(text),
        _ => None,
    }
}

/// Decline atomically when the budget is insufficient. Only exact rendered spelling,
/// cell placement and explicit legacy form can share. Arrays and tables stay separate.
/// A retained source is moved as a whole, so its membership never acquires phantom cells.
pub fn regroup_formula_sources(
    model: &ModelRoot,
    budget: FormulaSourceBudget,
) -> Option<RegroupedFormulaSources> {
    let mut cells = 0usize;
    let mut parts = BTreeMap::<(SheetId, SourceIdentity), Part>::new();
    let mut next_ids = BTreeMap::<SheetId, Option<u32>>::new();
    for sheet in model.sheets() {
        for (at, input) in model.sheet_cells(sheet.id()).into_iter().flatten() {
            if cells == budget.max_cells {
                return None;
            }
            cells += 1;
            let CellInput::Formula(text) = input else {
                continue;
            };
            if let Some(block) = text.block() {
                let next = block.id().local.checked_add(1);
                next_ids
                    .entry(sheet.id())
                    .and_modify(|held| {
                        *held = match (*held, next) {
                            (Some(a), Some(b)) => Some(a.max(b)),
                            _ => None,
                        };
                    })
                    .or_insert(next);
            }
            if text.form() != FormulaForm::Legacy
                || !text.is_form_explicit()
                || model.array_containing(at).is_some()
                || model.definitions().data_table_containing(at).is_some()
                || model
                    .definitions()
                    .tables()
                    .iter()
                    .any(|table| table.area.contains(at))
            {
                continue;
            }
            if let Some(block) = text.block() {
                parts
                    .entry((
                        sheet.id(),
                        SourceIdentity::Block(Arc::as_ptr(block) as usize),
                    ))
                    .or_insert_with(|| Part {
                        block: Some(block.clone()),
                        origin: block.source().anchor(),
                        source: block.source().source().clone(),
                        members: Vec::new(),
                    })
                    .members
                    .push(at);
            } else {
                parts.insert(
                    (sheet.id(), SourceIdentity::Cell(at)),
                    Part {
                        block: None,
                        origin: at,
                        source: text.rendered(),
                        members: vec![at],
                    },
                );
            }
        }
    }
    let before = model.formula_source_stats();
    let mut units = 0usize;
    let mut groups = BTreeMap::<(SheetId, Arc<str>), Vec<Part>>::new();
    for ((sheet, _), mut part) in parts {
        part.members.sort_unstable();
        let key = if let Some(block) = &part.block {
            if block.member_count() != part.members.len() as u64 {
                continue;
            }
            let Some(key) = block.key() else { continue };
            charge(&mut units, part.source.utf16_code_units().len(), budget)?;
            key.normalized_ir.clone()
        } else {
            charge(&mut units, part.source.utf16_code_units().len(), budget)?;
            let Ok(spelling) = String::from_utf16(part.source.utf16_code_units()) else {
                continue;
            };
            let Some(key) = calc_fml::reference::template_key(
                &spelling,
                calc_fml::reference::FormulaAnchor::new(part.origin),
            ) else {
                continue;
            };
            key.into()
        };
        groups.entry((sheet, key)).or_default().push(part);
    }
    let mut replacements = Vec::new();
    for ((sheet, normalized), mut group) in groups {
        // A single retained source already represents every member without expansion.
        if group.len() < 2 {
            continue;
        }
        group.sort_by_key(|part| part.members[0]);
        let canonical = group[0].members[0];
        let mut classes = BTreeMap::<Vec<u16>, Vec<Part>>::new();
        for part in group {
            charge(&mut units, part.source.utf16_code_units().len(), budget)?;
            let Some(renderer) = SharedSource::from_source(part.origin, part.source.clone()) else {
                continue;
            };
            let Some(spelling) = renderer.try_render(canonical) else {
                continue;
            };
            classes
                .entry(spelling.utf16_code_units().to_vec())
                .or_default()
                .push(part);
        }
        for (_, class) in classes {
            if class.len() < 2 {
                continue;
            }
            let master = class[0].members[0];
            let Some(source) =
                SharedSource::from_source(master, formula(model, master)?.rendered())
            else {
                continue;
            };
            let mut members = Vec::new();
            let mut master_admitted = true;
            for (index, part) in class.iter().enumerate() {
                let mut exact = true;
                let mut part_members = Vec::new();
                for &at in &part.members {
                    let actual = formula(model, at)?.rendered();
                    charge(&mut units, actual.utf16_code_units().len(), budget)?;
                    if source.try_render(at).as_ref() != Some(&actual) {
                        exact = false;
                        break;
                    }
                    part_members.push(at);
                }
                if exact {
                    members.extend(part_members);
                } else if index == 0 {
                    master_admitted = false;
                    break;
                }
            }
            // Never peel only some members off an existing source or search every master.
            if !master_admitted || members.len() < 2 {
                continue;
            }
            let top = members.iter().map(|at| at.row()).min()?;
            let bottom = members.iter().map(|at| at.row()).max()?;
            let left = members.iter().map(|at| at.column()).min()?;
            let right = members.iter().map(|at| at.column()).max()?;
            let extent = GridArea::new(
                CellKey::new(sheet, top, left).ok()?,
                CellKey::new(sheet, bottom, right).ok()?,
            )
            .ok()?;
            let next = next_ids.entry(sheet).or_insert(Some(0));
            let Some(local) = *next else { continue };
            *next = local.checked_add(1);
            let key = SemanticTemplateKey {
                normalized_ir: normalized.clone(),
                placement: TemplatePlacement::Cell,
                evaluation: FormulaForm::Legacy,
            };
            let block = FormulaBlock::new(
                FormulaBlockId { sheet, local },
                extent,
                members.iter().copied(),
                Some(key),
                FormulaForm::Legacy,
                Arc::new(source),
            )?;
            for at in members {
                replacements.push((at, FormulaText::member(block.clone(), at)?));
            }
        }
    }
    let regrouped_cells = replacements.len();
    let model = model.with_formula_storage(replacements)?;
    let after = model.formula_source_stats();
    Some(RegroupedFormulaSources {
        model,
        before,
        after,
        regrouped_cells,
    })
}
