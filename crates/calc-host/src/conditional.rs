//! What a sheet's conditional formats show (dc-6yw): for a cell, the fill, the font colour and
//! weight, the number format, the data bar or the icon its rules give it, as Excel applies them.
//!
//! [`HostState::conditional_formats`] prepares a sheet's rules once for the current revision: each
//! rule's formulas compiled for detached evaluation ([`calc_engine::DetachedFormula`]: CalcFml's
//! one evaluator, reading the workbook as the engine published it, never a second route), and the
//! statistics a rule reads over its whole areas (the numbers for a colour scale, a data bar, an
//! icon set, a top or bottom rank and an average; the values for duplicates). [`ConditionalFormats::look`]
//! then gives one cell's look.
//!
//! The rules apply in priority order (`priority`, lowest first, across every block that holds
//! the cell). A rule that holds sets the parts of the look a rule before it left unset, and one
//! with stop-if-true ends the cell's rules. A `cellIs` rule compares the cell with its formulas
//! through a formula the host writes (`A1>(5)` at the block's top-left cell), so the comparison is
//! the evaluator's, as Excel's comparison operators compare; an `expression` rule and the text,
//! blank, error and date rules, for which Excel writes the formula, hold when their formula gives
//! TRUE or a non-zero number. A formula that is refused, or that reads a cell whose value nothing
//! calculated, does not hold, and is counted ([`ConditionalFormats::refused`]).
//!
//! Colours: an `rgb` colour as it is; a theme colour from the workbook's theme with its tint
//! applied as Windows' HLS arithmetic does (luminance in 0 to 240, measured on Excel 16.0.20430:
//! accent1 156082 at tint 0.6 shows as 83CCEB); an indexed colour by its index, for the caller's
//! palette ([`LookColor::Indexed`]).
//!
//! Not modelled yet, each leaving the cell as it is: rules that only
//! Excel 2010's extension holds, a colour scale's or data bar's `formula` thresholds that give no
//! number, and after a row or column is inserted or deleted in the session, a rule's formulas are
//! not rewritten (its areas move, its formulas keep their text; the save rewrites them).

use crate::state::HostState;
use crate::{ProjectedValue, ProjectionRequest, project};
use calc_doc::look::{self, Applied, Appearance, RuleStats};
use calc_doc::{ConditionalRule, Threshold};
use calc_doc_model::StyleColor;
use calc_doc_model::value::{CalcValue, CellKey, GridArea, SheetId};
use calc_engine::DetachedFormula;

// The look types and the arithmetic that works out a rule's look from a range's values are
// calc-doc's `look` module, which DNA OneCalc runs too (it depends on neither calc-host nor
// calc-engine). This module prepares a sheet's rules over the engine's published values.
pub use calc_doc::look::{BarLook, CellLook, IconLook, LookColor};

/// A differential format's look (a rule's `dxfId`).
type DxfLook = Appearance;

/// What a rule reads over its whole areas.
type Stats = RuleStats;

/// One rule, prepared.
struct Prepared {
    source_index: usize,
    rule: ConditionalRule,
    areas: Vec<GridArea>,
    /// The formula whose truth the rule tests, compiled at the block's origin.
    condition: Option<Result<DetachedFormula, ()>>,
    /// Each threshold's formula, for colour scales, data bars and icon sets.
    threshold_formulas: Vec<Option<DetachedFormula>>,
    stats: Option<Stats>,
    /// Aggregate threshold evaluated once through CalcFml, not once for every painted cell.
    average: Option<Result<f64, ()>>,
    dxf: DxfLook,
}

/// A sheet's conditional formats, prepared for one revision ([`HostState::conditional_formats`]).
pub struct ConditionalFormats {
    sheet: SheetId,
    rules: Vec<Prepared>,
    refused: std::cell::Cell<usize>,
}

impl ConditionalFormats {
    /// The sheet the formats are of.
    pub fn sheet(&self) -> SheetId {
        self.sheet
    }

    /// How many rules the sheet has.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// How many rule evaluations so far did not hold because their formula was refused.
    pub fn refused(&self) -> usize {
        self.refused.get()
    }

    /// What the conditional formats give the cell `at`, reading `host` (the revision they were
    /// prepared for); `None` when no rule changes its look.
    pub fn look(&self, host: &HostState, at: CellKey) -> Option<CellLook> {
        self.look_with_context(host, at, None)
    }

    /// An authoring decision reuses its current-value context, without changing painting.
    pub(crate) fn look_in(
        &self,
        host: &HostState,
        at: CellKey,
        context: &calc_engine::DetachedEvaluationContext<'_>,
    ) -> Option<CellLook> {
        self.look_with_context(host, at, Some(context))
    }

    fn look_with_context(
        &self,
        host: &HostState,
        at: CellKey,
        context: Option<&calc_engine::DetachedEvaluationContext<'_>>,
    ) -> Option<CellLook> {
        if at.sheet() != self.sheet {
            return None;
        }
        let mut look = CellLook::default();
        let mut value = None;
        for prepared in &self.rules {
            if !prepared.areas.iter().any(|area| area.contains(at)) {
                continue;
            }
            let value = value.get_or_insert_with(|| match context {
                Some(context) => context.read_cell(at).map(Some),
                None => Ok(cell_value(host, at)),
            });
            let Ok(value) = value else {
                self.refused.set(self.refused.get() + 1);
                return None;
            };
            let held = self.apply(host, prepared, at, value.as_ref(), &mut look, context);
            if held && prepared.rule.stop_if_true {
                break;
            }
        }
        (!look.is_empty()).then_some(look)
    }

    /// Explanations use the same prepared rules and evaluator as rendering, without changing counters.
    pub fn explain(&self, host: &HostState, at: CellKey) -> Vec<(usize, String)> {
        let saved = self.refused.get();
        let value = cell_value(host, at);
        let mut stopped = false;
        let mut look = CellLook::default();
        let result = self
            .rules
            .iter()
            .map(|p| {
                let state = if !p.areas.iter().any(|a| a.contains(at)) {
                    "Outside this cell".to_owned()
                } else if stopped {
                    "Skipped after Stop if true".to_owned()
                } else if p.condition.is_none()
                        && !matches!(
                            p.rule.kind.as_str(),
                            "colorScale"
                                | "dataBar"
                                | "iconSet"
                                | "top10"
                                | "aboveAverage"
                                | "duplicateValues"
                                | "uniqueValues"
                        )
                {
                    "Rule evaluation is unavailable (dc-971p.8)".to_owned()
                } else {
                    let old = look.clone();
                    let before = self.refused.get();
                    let held = self.apply(host, p, at, value.as_ref(), &mut look, None);
                    if self.refused.get() != before {
                        "Rule formula unavailable; no appearance supplied".to_owned()
                    } else if held {
                        stopped = p.rule.stop_if_true;
                        if old == look {
                            "Condition true; no additional supported appearance".into()
                        } else {
                            "Applies appearance to this cell".into()
                        }
                    } else if matches!(p.rule.kind.as_str(), "colorScale" | "dataBar" | "iconSet") {
                        "No appearance: value or threshold is unavailable".into()
                    } else {
                        "Condition false for this cell".into()
                    }
                };
                (p.source_index, state)
            })
            .collect();
        self.refused.set(saved);
        result
    }

    /// Applies one rule to the cell, setting what the look leaves unset; whether the rule held.
    fn apply(
        &self,
        host: &HostState,
        prepared: &Prepared,
        at: CellKey,
        value: Option<&CalcValue>,
        look: &mut CellLook,
        context: Option<&calc_engine::DetachedEvaluationContext<'_>>,
    ) -> bool {
        let rule = &prepared.rule;
        let theme = host.loaded.theme_colors();
        let formula_threshold = |index: usize| {
            let formula = prepared.threshold_formulas.get(index)?.as_ref()?;
            look::number_of(&evaluate_formula(host, formula, at, context).ok()?)
        };
        if let Some(applied) = look::apply_aggregate(
            rule,
            &prepared.dxf,
            prepared.stats.as_ref(),
            prepared.average,
            value,
            theme,
            &formula_threshold,
            look,
        ) {
            if applied == Applied::Refused {
                self.refused.set(self.refused.get() + 1);
            }
            return applied == Applied::Held;
        }
        let Some(condition) = &prepared.condition else {
            return false;
        };
        let Ok(formula) = condition else {
            self.refused.set(self.refused.get() + 1);
            return false;
        };
        match evaluate_formula(host, formula, at, context) {
            Ok(result) if look::truth(&result) => look::dress(look, &prepared.dxf),
            Ok(_) => false,
            Err(_) => {
                self.refused.set(self.refused.get() + 1);
                false
            }
        }
    }
}

fn evaluate_formula(
    host: &HostState,
    formula: &DetachedFormula,
    at: CellKey,
    context: Option<&calc_engine::DetachedEvaluationContext<'_>>,
) -> Result<CalcValue, calc_engine::NotModeled> {
    match context {
        Some(context) => formula.evaluate_in(context, at),
        None => formula.evaluate(&host.engine, &host.model, at),
    }
}

/// A style colour as a look's colour, against the workbook's theme.
pub(crate) fn resolve(host: &HostState, color: &StyleColor) -> Option<LookColor> {
    look::resolve_color(color, host.loaded.theme_colors())
}

/// The value a rule reads of a cell: its literal, or its formula's calculated or file-cached
/// value; `None` for a blank cell, and for a formula with no value (refused or stale).
pub(crate) fn cell_value(host: &HostState, at: CellKey) -> Option<CalcValue> {
    let page = project(host, &ProjectionRequest { cells: vec![at] }).ok()?;
    projected_value(page.cells().first()?.value())
}

pub(crate) fn projected_value(value: &ProjectedValue) -> Option<CalcValue> {
    match value {
        ProjectedValue::Literal(value) => Some(value.clone()),
        ProjectedValue::Calculated { value, .. } => Some(value.clone()),
        ProjectedValue::FileCached(cache) => Some(cache.value().to_value()),
        _ => None,
    }
}

/// The address of a cell as a relative A1 reference (`B3`).
fn a1(at: CellKey) -> String {
    look::a1(at.row(), at.column())
}

/// The formula a `cellIs` rule tests at the block's origin ([`look::condition_formula`]).
fn cell_is_formula(rule: &ConditionalRule, origin: CellKey) -> Option<String> {
    look::condition_formula(rule, &a1(origin))
}

impl HostState {
    /// The conditional formats of `sheet`, prepared for the current revision (see
    /// [`crate::conditional`]); empty for a sheet the file gave none.
    pub fn conditional_formats(&self, sheet: SheetId) -> ConditionalFormats {
        self.prepare_conditional_formats(sheet, None, None)
    }

    /// Prepare an authoring range through the same evaluator as painting, in its current context.
    pub(crate) fn conditional_formats_for(
        &self,
        area: GridArea,
        context: &calc_engine::DetachedEvaluationContext<'_>,
    ) -> ConditionalFormats {
        self.prepare_conditional_formats(area.sheet(), Some(area), Some(context))
    }

    fn prepare_conditional_formats(
        &self,
        sheet: SheetId,
        area: Option<GridArea>,
        context: Option<&calc_engine::DetachedEvaluationContext<'_>>,
    ) -> ConditionalFormats {
        let mut rules = Vec::new();
        for (index, source) in self
            .model
            .definitions()
            .conditional_rules(sheet)
            .iter()
            .enumerate()
            .filter(|(_, source)| area.is_none_or(|a| source.areas.iter().any(|b| a.intersection(*b).is_some())))
        {
            if let (Ok(rule), Some(origin)) = (calc_doc::conditional_rule(source), source.origin())
            {
                let mut prepared = self.prepare(&rule, &source.areas, origin, context);
                prepared.source_index = index;
                rules.push(prepared);
            }
        }
        rules.sort_by_key(|prepared| prepared.rule.priority);
        ConditionalFormats {
            sheet,
            rules,
            refused: std::cell::Cell::new(0),
        }
    }

    /// Areas the file states for `sheet`, as (top, left, bottom, right), where the session's row
    /// and column insertions and deletions have moved them; an area all of whose cells were
    /// deleted is left out.
    pub(crate) fn current_areas(
        &self,
        sheet: SheetId,
        areas: &[(u32, u32, u32, u32)],
    ) -> Vec<GridArea> {
        use calc_fml::reference::{EditAxis, StructuralEdit};
        let moves: Vec<StructuralEdit> = self
            .axis_edits
            .iter()
            .filter(|edit| edit.sheet() == sheet)
            .map(|edit| {
                let axis = match edit.axis() {
                    calc_doc_model::Axis::Rows => EditAxis::Rows,
                    calc_doc_model::Axis::Columns => EditAxis::Columns,
                };
                let (first, count) = (edit.first(), edit.count());
                match edit.change() {
                    calc_doc_model::AxisChange::Insert => StructuralEdit::Insert {
                        sheet,
                        axis,
                        first,
                        count,
                    },
                    calc_doc_model::AxisChange::Delete => StructuralEdit::Delete {
                        sheet,
                        axis,
                        first,
                        count,
                    },
                }
            })
            .collect();
        areas
            .iter()
            .filter_map(|&(top, left, bottom, right)| {
                let mut area = GridArea::new(
                    CellKey::new(sheet, top, left).ok()?,
                    CellKey::new(sheet, bottom, right).ok()?,
                )
                .ok()?;
                for edit in &moves {
                    area = edit.area(area)?;
                }
                Some(area)
            })
            .collect()
    }

    fn prepare(
        &self,
        rule: &ConditionalRule,
        areas: &[GridArea],
        origin: CellKey,
        context: Option<&calc_engine::DetachedEvaluationContext<'_>>,
    ) -> Prepared {
        let compile = |text: &str| match context {
            Some(context) => context.compile(text, origin, calc_fml::FormulaForm::Legacy),
            None => DetachedFormula::compile(&self.model, text, origin),
        }.map_err(|_| ());
        let condition = match rule.kind.as_str() {
            "cellIs" => Some(match cell_is_formula(rule, origin) {
                Some(text) => compile(&text),
                None => Err(()),
            }),
            "colorScale" | "dataBar" | "iconSet" | "top10" | "aboveAverage" | "duplicateValues"
            | "uniqueValues" => None,
            _ => Some(match rule.formulas.first() {
                Some(text) => compile(text),
                None => Err(()),
            }),
        };
        let thresholds: Vec<&Threshold> = rule
            .color_scale
            .iter()
            .flat_map(|s| s.thresholds.iter())
            .chain(rule.data_bar.iter().flat_map(|b| b.thresholds.iter()))
            .chain(rule.icon_set.iter().flat_map(|s| s.thresholds.iter()))
            .collect();
        let threshold_formulas = thresholds
            .iter()
            .map(|t| match (t.kind.as_str(), t.value.as_deref()) {
                ("formula" | "num", Some(text)) if text.trim().parse::<f64>().is_err() => {
                    compile(text.trim_start_matches('=')).ok()
                }
                _ => None,
            })
            .collect();
        let average = (rule.kind == "aboveAverage").then(|| {
            let absolute = |at| {let text=a1(at);let split=text.find(|c:char|c.is_ascii_digit()).unwrap();format!("${}${}",&text[..split],&text[split..])};
            let ranges=areas.iter().map(|a|format!("{}:{}",absolute(a.first()),absolute(a.last()))).collect::<Vec<_>>().join(",");
            let bound=look::average_formula(rule,&ranges);
            let formula=compile(&bound)?;
            let value=evaluate_formula(self,&formula,origin,context).map_err(|_|())?;
            look::number_of(&value).ok_or(())
        });
        let stats = (condition.is_none() && average.is_none()).then(|| self.stats(areas));
        let dxf = rule
            .dxf
            .and_then(|i| self.styles().differential_formats.get(i as usize))
            .map(|d| DxfLook {
                fill: d.fill.as_ref().and_then(|fill| {
                    let color = match fill.pattern_type.as_deref() {
                        Some("solid") => fill.fg_color.as_ref().or(fill.bg_color.as_ref()),
                        _ => fill.bg_color.as_ref().or(fill.fg_color.as_ref()),
                    }?;
                    resolve(self, color)
                }),
                font_color: d
                    .font
                    .as_ref()
                    .and_then(|font| font.color.as_ref())
                    .and_then(|color| resolve(self, color)),
                bold: d.font.as_ref().and_then(|f| f.bold.then_some(true)),
                italic: d.font.as_ref().and_then(|f| f.italic.then_some(true)),
                underline: d
                    .font
                    .as_ref()
                    .and_then(|f| f.underline.as_ref().map(|u| u != "none")),
                strike: d.font.as_ref().and_then(|f| f.strike.then_some(true)),
                number_format: d.number_format.as_ref().map(|n| n.code.clone()),
            })
            .unwrap_or_default();
        Prepared {
            source_index: 0,
            rule: rule.clone(),
            areas: areas.to_vec(),
            condition,
            threshold_formulas,
            stats,
            average,
            dxf,
        }
    }

    /// The numbers and the value counts over `areas`' occupied cells.
    fn stats(&self, areas: &[GridArea]) -> Stats {
        let mut cells: Vec<CellKey> = Vec::new();
        for area in areas {
            cells.extend(
                self.model
                    .cells(*area)
                    .into_iter()
                    .flatten()
                    .map(|(at, _)| at),
            );
        }
        // Live spill members need not have authored cell inputs. Read their published values
        // through the same projection as occupied cells, and count overlaps only once.
        for spill in self.engine.spills() {
            for area in areas.iter().filter_map(|area| area.intersection(spill.area())) {
                for row in area.first().row()..=area.last().row() {
                    for column in area.first().column()..=area.last().column() {
                        cells.push(CellKey::new(area.sheet(), row, column).expect("valid spill area"));
                    }
                }
            }
        }
        cells.sort();
        cells.dedup();
        let mut stats = Stats::default();
        if let Ok(page) = project(self, &ProjectionRequest { cells }) {
            for cell in page.cells() {
                let Some(value) = projected_value(cell.value()) else {
                    continue;
                };
                stats.add(&value);
            }
        }
        stats.finish();
        stats
    }
}
