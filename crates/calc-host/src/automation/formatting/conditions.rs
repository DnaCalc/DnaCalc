//! `Range.FormatConditions` and `FormatCondition` (dc-tqcvu): rules of the cell-value and expression types, their
//! `Interior` and `Font`, Count, Item and Delete. Each write is the conditional-rule command the Conditional Formatting
//! dialog uses (`WorkbookCommand::Conditional`); a rule a macro adds owns one differential format that `Interior` and
//! `Font` then change (`ConditionalChange::Format`).
//!
//! `FormatConditions(i)` counts the rules that touch the range, in priority order. A `FormatCondition` names its rule
//! by position in the worksheet's rule list, so deleting a rule makes later objects answer 1004 or name a neighbour.
//! Formulas are stored and read relative to the first cell of the range given to `Add`; Excel reads them relative to
//! the active cell (RUN-ON-WINDOWS.txt, cl5 dc-tqcvu).
use super::super::report::report;
use super::borders::{
    BOTTOM, DIAGONAL_DOWN, DIAGONAL_UP, LEFT, LINES, RIGHT, TOP, WEIGHTS, edge_property,
    indexed_color, pick, rgb_color, side,
};
use super::interior::color_bgr;
use super::*;
use crate::{CommandIngress, ConditionalChange, EdgeChange, EdgesPatch, SkinIntent, WorkbookCommand};
use calc_doc_model::{DifferentialFormat, Fill, NumberFormat, StyleColor};
use calc_object_model::ReportItem;

/// Owner of the rule types, members and operations this port refuses.
const REMAINDER: &str = "dc-jupjv";

// XlFormatConditionType (common types) and XlFormatConditionOperator, with the OOXML spellings.
const TYPES: &[(i32, &str)] = &[
    (1, "cellIs"),
    (2, "expression"),
    (3, "colorScale"),
    (4, "dataBar"),
    (5, "top10"),
    (6, "iconSet"),
    (8, "duplicateValues"),
    (8, "uniqueValues"),
    (9, "containsText"),
    (10, "containsBlanks"),
    (11, "timePeriod"),
    (12, "aboveAverage"),
    (13, "notContainsBlanks"),
    (16, "containsErrors"),
    (17, "notContainsErrors"),
];
const OPERATORS: &[(i32, &str)] = &[
    (1, "between"),
    (2, "notBetween"),
    (3, "equal"),
    (4, "notEqual"),
    (5, "greaterThan"),
    (6, "lessThan"),
    (7, "greaterThanOrEqual"),
    (8, "lessThanOrEqual"),
];

fn refused() -> AutoError {
    AutoError::excel(1004, "Application-defined or object-defined error")
}

fn text_of(value: &AutoValue) -> Result<Option<String>, AutoError> {
    Ok(match value {
        AutoValue::Missing | AutoValue::Empty => None,
        AutoValue::Text(_) => Some(value.as_text().unwrap_or_default()),
        AutoValue::Bool(b) => Some(if *b { "TRUE" } else { "FALSE" }.into()),
        AutoValue::I2(v) => Some(v.to_string()),
        AutoValue::I4(v) => Some(v.to_string()),
        AutoValue::R4(v) => Some(f64::from(*v).to_string()),
        AutoValue::R8(v) => Some(v.to_string()),
        _ => return Err(refused()),
    })
}

fn applies_text(areas: &[GridArea]) -> String {
    areas
        .iter()
        .map(|a| {
            let first = calc_doc::look::a1(a.first().row(), a.first().column());
            if a.cell_count() == 1 {
                first
            } else {
                format!("{first}:{}", calc_doc::look::a1(a.last().row(), a.last().column()))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The areas as rule text, refused (1004) unless every one is on `sheet`.
fn applies_text_checked(areas: &[GridArea], sheet: SheetId) -> Result<String, AutoError> {
    if areas.iter().any(|a| a.first().sheet() != sheet) {
        return Err(refused());
    }
    Ok(applies_text(areas))
}

/// The positions (in the worksheet's rule list) of the rules touching `areas`, in priority order.
fn touching(session: &HostSession, sheet: SheetId, areas: &[GridArea]) -> Vec<usize> {
    let rules = session.host().model().definitions().conditional_rules(sheet);
    let mut order: Vec<usize> = (0..rules.len())
        .filter(|&n| rules[n].areas.iter().any(|a| areas.iter().any(|b| a.intersection(*b).is_some())))
        .collect();
    order.sort_by_key(|&n| rules[n].priority);
    order
}

fn rule_at(
    session: &HostSession,
    sheet: SheetId,
    index: u32,
) -> Result<(calc_doc_model::ConditionalRuleSource, calc_doc::ConditionalRule), AutoError> {
    let rules = session.host().model().definitions().conditional_rules(sheet);
    let source = rules.get(index as usize).ok_or_else(refused)?.clone();
    let parsed = crate::conditional_rule(&source).map_err(|_| refused())?;
    Ok((source, parsed))
}

fn dxf_of(session: &HostSession, rule: &calc_doc::ConditionalRule) -> Result<DifferentialFormat, AutoError> {
    let Some(id) = rule.dxf else {
        return Ok(DifferentialFormat::default());
    };
    session
        .host()
        .styles()
        .differential_formats
        .get(id as usize)
        .cloned()
        .ok_or_else(|| AutoError::not_modelled("a rule whose format is missing from the styles part", REMAINDER))
}

fn condition_index(target: &ObjectRef) -> Result<u32, AutoError> {
    match &target.key {
        ObjectKey::Report {
            item:
                ReportItem::FormatCondition(n)
                | ReportItem::ConditionInterior(n)
                | ReportItem::ConditionFont(n)
                | ReportItem::ConditionBorders(n)
                | ReportItem::ConditionBorder(n, _),
            ..
        } => Ok(*n),
        _ => Err(geometry::type_mismatch()),
    }
}

/// True when `target` is the `Interior`, `Font`, `Borders` or `Border` of a `FormatCondition`.
pub(in crate::automation) fn is_condition_part(target: &ObjectRef) -> bool {
    matches!(
        &target.key,
        ObjectKey::Report {
            item:
                ReportItem::ConditionInterior(_)
                | ReportItem::ConditionFont(_)
                | ReportItem::ConditionBorders(_)
                | ReportItem::ConditionBorder(..),
            ..
        }
    )
}

/// Subtract `cut` from `area`, yielding up to four non-overlapping rectangular strips.
fn subtract_area(area: GridArea, cut: GridArea) -> Vec<GridArea> {
    let Some(inter) = area.intersection(cut) else {
        return vec![area];
    };
    let sheet = area.sheet();
    let (ar1, ac1, ar2, ac2) = (
        area.first().row(),
        area.first().column(),
        area.last().row(),
        area.last().column(),
    );
    let (ir1, ic1, ir2, ic2) = (
        inter.first().row(),
        inter.first().column(),
        inter.last().row(),
        inter.last().column(),
    );
    let rect = |r1, c1, r2, c2| {
        GridArea::new(
            CellKey::new(sheet, r1, c1).expect("strip inside grid"),
            CellKey::new(sheet, r2, c2).expect("strip inside grid"),
        )
        .expect("same sheet")
    };
    let mut out = Vec::new();
    if ar1 < ir1 {
        out.push(rect(ar1, ac1, ir1 - 1, ac2));
    }
    if ir2 < ar2 {
        out.push(rect(ir2 + 1, ac1, ar2, ac2));
    }
    if ac1 < ic1 {
        out.push(rect(ir1, ac1, ir2, ic1 - 1));
    }
    if ic2 < ac2 {
        out.push(rect(ir1, ic2 + 1, ir2, ac2));
    }
    out
}

fn subtract_areas(existing: &[GridArea], cuts: &[GridArea]) -> Vec<GridArea> {
    let mut current = existing.to_vec();
    for &cut in cuts {
        current = current
            .into_iter()
            .flat_map(|area| subtract_area(area, cut))
            .collect();
    }
    current
}

impl WorkbookAutomation {
    pub(super) fn register_conditions(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "FormatConditions", InvokeKind::Get, Self::range_conditions);
        handlers.on("FormatConditions", "Count", InvokeKind::Get, Self::conditions_count);
        handlers.on("FormatConditions", "Item", InvokeKind::Method, Self::conditions_item);
        handlers.on("FormatConditions", "_Default", InvokeKind::Get, Self::conditions_item);
        handlers.on("FormatConditions", "Add", InvokeKind::Method, Self::conditions_add);
        handlers.on("FormatConditions", "Delete", InvokeKind::Method, Self::conditions_delete);
        for member in ["Type", "Operator", "Formula1", "Formula2", "Priority"] {
            handlers.on("FormatCondition", member, InvokeKind::Get, Self::condition_get);
        }
        handlers.on("FormatCondition", "StopIfTrue", InvokeKind::Get, Self::condition_stop);
        handlers.on("FormatCondition", "StopIfTrue", InvokeKind::Put, Self::condition_put_stop);
        handlers.on("FormatCondition", "Interior", InvokeKind::Get, Self::condition_interior);
        handlers.on("FormatCondition", "Font", InvokeKind::Get, Self::condition_font);
        handlers.on("FormatCondition", "Borders", InvokeKind::Get, Self::condition_borders);
        handlers.on("FormatCondition", "NumberFormat", InvokeKind::Get, Self::condition_number_format);
        handlers.on("FormatCondition", "NumberFormat", InvokeKind::Put, Self::condition_put_number_format);
        handlers.on("FormatCondition", "AppliesTo", InvokeKind::Get, Self::condition_applies_to);
        handlers.on("FormatCondition", "ModifyAppliesToRange", InvokeKind::Method, Self::condition_modify_applies);
        handlers.on("FormatCondition", "Priority", InvokeKind::Put, Self::condition_put_priority);
        handlers.on("FormatCondition", "SetFirstPriority", InvokeKind::Method, Self::condition_set_priority);
        handlers.on("FormatCondition", "SetLastPriority", InvokeKind::Method, Self::condition_set_priority);
        handlers.on("FormatCondition", "Delete", InvokeKind::Method, Self::condition_delete);
    }

    fn range_conditions(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        self.with_range(target, |access, _, range, _| {
            let reference = self.retain_range(access, range)?;
            Ok(AutoValue::Object(object("FormatConditions", ObjectKey::RangeFormat(reference))))
        })
    }

    fn conditions_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        self.with_range(target, |access, _, range, _| {
            let Access::Session(session, _) = access else { return Err(refused()) };
            let sheet = range.areas()[0].first().sheet();
            Ok(AutoValue::I4(touching(session, sheet, range.areas()).len() as i32))
        })
    }

    fn conditions_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let place = geometry::integer(call.arg(0)).map_err(|_| refused())?;
        self.with_range(target, |access, meta, range, _| {
            let Access::Session(session, _) = access else { return Err(refused()) };
            let sheet = range.areas()[0].first().sheet();
            let order = touching(session, sheet, range.areas());
            let n = usize::try_from(place - 1).ok().and_then(|n| order.get(n)).ok_or_else(refused)?;
            Ok(report(meta.workbook, sheet, "FormatCondition", ReportItem::FormatCondition(*n as u32)))
        })
    }

    /// `FormatConditions.Add(Type, Operator, Formula1, Formula2)` for `xlCellValue` and `xlExpression`.
    fn conditions_add(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.is_empty() || call.args.len() > 4 {
            return Err(geometry::bad_count());
        }
        let kind = geometry::integer(call.arg(0)).map_err(|_| refused())? as i32;
        let (kind, operator, formulas) = match kind {
            1 => {
                let op = geometry::integer(call.arg(1)).map_err(|_| refused())? as i32;
                let name = OPERATORS.iter().find(|(v, _)| *v == op).ok_or_else(refused)?.1;
                let first = text_of(call.arg(2))?.ok_or_else(refused)?;
                let mut formulas = vec![first];
                if op <= 2 {
                    formulas.push(text_of(call.arg(3))?.ok_or_else(refused)?);
                }
                ("cellIs", name, formulas)
            }
            2 => ("expression", "", vec![text_of(call.arg(2))?.ok_or_else(refused)?]),
            // Excel's defaults: top 10 items, above the average, duplicate values.
            5 => ("top10", "", Vec::new()),
            8 => ("duplicateValues", "", Vec::new()),
            12 => ("aboveAverage", "", Vec::new()),
            3..=17 if TYPES.iter().any(|(v, _)| *v == kind) => {
                return Err(AutoError::not_modelled(
                    format!("FormatConditions.Add of type {kind}"),
                    REMAINDER,
                ));
            }
            _ => return Err(refused()),
        };
        let mut found = None;
        self.with_range(target, |access, meta, range, _| {
            let Access::Session(session, _) = access else { return Err(refused()) };
            let sheet = range.areas()[0].first().sheet();
            // The new rule is appended to the worksheet's list: its position is the current length.
            let next = session.host().model().definitions().conditional_rules(sheet).len();
            found = Some((meta.workbook, sheet, next, applies_text(range.areas())));
            Ok(AutoValue::Empty)
        })?;
        let (workbook, sheet, before, areas) = found.ok_or_else(refused)?;
        self.with_write(|session| {
            if session.host().loaded().sheet(sheet).is_some_and(|s| s.protected()) {
                return Err(refused());
            }
            self.guard_sheet_contents(session, sheet)?;
            let revision = session.host().revision().get();
            let receipt = session.dispatch_from(
                CommandIngress::Automation,
                SkinIntent::Command(WorkbookCommand::Conditional {
                    sheet,
                    revision,
                    index: None,
                    change: ConditionalChange::Condition {
                        kind: kind.into(),
                        applies: areas.clone(),
                        color: String::new(),
                        formulas: formulas.clone(),
                        operator: operator.into(),
                        rank: 10,
                        percent: false,
                        stop: false,
                    },
                }),
            );
            if receipt.error.is_some() {
                return Err(refused());
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })?;
        Ok(report(workbook, sheet, "FormatCondition", ReportItem::FormatCondition(before as u32)))
    }

    fn conditions_delete(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let mut plan = None;
        self.with_range(target, |access, _, range, _| {
            let Access::Session(session, _) = access else { return Err(refused()) };
            let sheet = range.areas()[0].first().sheet();
            let rules = session.host().model().definitions().conditional_rules(sheet);
            let mut order = touching(session, sheet, range.areas());
            // Apply from the highest position down so earlier positions stay valid after removals.
            order.sort_unstable_by(|a, b| b.cmp(a));
            let mut edits = Vec::new();
            for n in order {
                let source = &rules[n];
                let remaining = subtract_areas(&source.areas, range.areas());
                if remaining.is_empty() {
                    edits.push((n as u32, ConditionalChange::Remove));
                } else if remaining.len() > 128 {
                    return Err(AutoError::not_modelled(
                        "FormatConditions.Delete splitting a rule into more than 128 ranges",
                        REMAINDER,
                    ));
                } else if remaining != source.areas {
                    edits.push((
                        n as u32,
                        ConditionalChange::Update {
                            applies: applies_text(&remaining),
                            formulas: source.formulas.clone(),
                            stop: source.stop,
                        },
                    ));
                }
            }
            plan = Some((sheet, edits));
            Ok(AutoValue::Empty)
        })?;
        let (sheet, edits) = plan.ok_or_else(refused)?;
        for (index, change) in edits {
            self.rule_change(sheet, index, change)?;
        }
        Ok(AutoValue::Empty)
    }

    /// Dispatch one change to the rule at `index` as the Automation ingress.
    fn rule_change(&self, sheet: SheetId, index: u32, change: ConditionalChange) -> Reply {
        self.with_write(|session| {
            if session.host().loaded().sheet(sheet).is_some_and(|s| s.protected()) {
                return Err(refused());
            }
            self.guard_sheet_contents(session, sheet)?;
            let revision = session.host().revision().get();
            let receipt = session.dispatch_from(
                CommandIngress::Automation,
                SkinIntent::Command(WorkbookCommand::Conditional {
                    sheet,
                    revision,
                    index: Some(index),
                    change: change.clone(),
                }),
            );
            if let Some(error) = receipt.error {
                return Err(AutoError::not_modelled(format!("FormatCondition: {error:?}"), REMAINDER));
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    fn condition_get(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let index = condition_index(target)?;
        let member = call.member.name.as_str();
        self.read_host(target, |session, sheet, _| {
            let (source, rule) = rule_at(session, sheet, index)?;
            match member {
                "Type" => TYPES
                    .iter()
                    .find(|(_, name)| *name == rule.kind)
                    .map(|(v, _)| AutoValue::I4(*v))
                    .ok_or_else(|| AutoError::not_modelled(format!("FormatCondition.Type of a {} rule", rule.kind), REMAINDER)),
                "Operator" if rule.kind == "cellIs" => rule
                    .operator
                    .as_deref()
                    .and_then(|op| OPERATORS.iter().find(|(_, name)| *name == op))
                    .map(|(v, _)| AutoValue::I4(*v))
                    .ok_or_else(|| AutoError::not_modelled("FormatCondition.Operator stored as another comparison", REMAINDER)),
                // ww21 Condition.Expression.Operator: expression rules answer Empty.
                "Operator" if rule.kind == "expression" => Ok(AutoValue::Empty),
                "Formula1" | "Formula2" if matches!(rule.kind.as_str(), "cellIs" | "expression") => {
                    let place = usize::from(member == "Formula2");
                    match source.formulas.get(place) {
                        Some(f) => Ok(AutoValue::text(&format!("={f}"))),
                        // ww21 Condition.Formula2: an absent second formula answers Empty.
                        None if place == 1 => Ok(AutoValue::Empty),
                        None => Err(AutoError::not_modelled("FormatCondition.Formula1 of a rule without a formula", REMAINDER)),
                    }
                }
                "Priority" => {
                    let rules = session.host().model().definitions().conditional_rules(sheet);
                    let rank = rules.iter().filter(|r| r.priority <= source.priority).count();
                    Ok(AutoValue::I4(rank as i32))
                }
                _ => Err(AutoError::not_modelled(format!("FormatCondition.{member} of a {} rule", rule.kind), REMAINDER)),
            }
        })
    }

    fn condition_stop(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let index = condition_index(target)?;
        self.read_host(target, |session, sheet, _| {
            Ok(AutoValue::Bool(rule_at(session, sheet, index)?.0.stop))
        })
    }

    fn condition_put_stop(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let index = condition_index(target)?;
        let stop = geometry::boolean(call.arg(0)).map_err(|_| refused())?;
        let (_, sheet, _) = Self::report_key(target)?;
        let mut keep = None;
        self.read_host(target, |session, sheet, _| {
            let (source, rule) = rule_at(session, sheet, index)?;
            if !matches!(rule.kind.as_str(), "cellIs" | "expression") {
                return Err(AutoError::not_modelled("StopIfTrue of this rule type", REMAINDER));
            }
            keep = Some(source);
            Ok(AutoValue::Empty)
        })?;
        let source = keep.ok_or_else(refused)?;
        let areas = applies_text(&source.areas);
        self.rule_change(
            sheet,
            index,
            ConditionalChange::Update { applies: areas, formulas: source.formulas.clone(), stop },
        )
    }

    fn condition_applies_to(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let index = condition_index(target)?;
        let mut areas = Vec::new();
        self.read_host(target, |session, sheet, _| {
            areas = rule_at(session, sheet, index)?.0.areas;
            Ok(AutoValue::Empty)
        })?;
        let reference = ReferenceLike::from_areas(areas).map_err(|_| refused())?;
        self.with_access(|mut access| self.acquire(&mut access, &reference, RangeAxis::Cells))
    }

    /// `ModifyAppliesToRange(Range)`: the rule now applies to `Range`, which must be on the rule's worksheet.
    fn condition_modify_applies(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let index = condition_index(target)?;
        let (_, sheet, _) = Self::report_key(target)?;
        let AutoValue::Object(ObjectRef { key: ObjectKey::Range(held) | ObjectKey::RangeView { reference: held, .. }, .. }) =
            call.arg(0)
        else {
            return Err(refused());
        };
        let areas = self.with_access(|mut access| {
            let resolved = self.resolve(&mut access, held)?;
            Ok(AutoValue::text(&applies_text_checked(resolved.areas(), sheet)?))
        })?;
        let applies = areas.as_text().ok_or_else(refused)?;
        let mut keep = None;
        self.read_host(target, |session, sheet, _| {
            keep = Some(rule_at(session, sheet, index)?.0);
            Ok(AutoValue::Empty)
        })?;
        let source = keep.ok_or_else(refused)?;
        self.rule_change(sheet, index, ConditionalChange::Update { applies, formulas: source.formulas.clone(), stop: source.stop })
    }

    fn condition_put_priority(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let rank = geometry::integer(call.arg(0)).map_err(|_| refused())?;
        self.condition_move(target, i64::from(rank))
    }

    fn condition_set_priority(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        self.condition_move(target, if call.member.name == "SetFirstPriority" { 1 } else { i64::MAX })
    }

    /// Move the rule to `rank` among the worksheet's rules; `i64::MAX` means last.
    fn condition_move(&self, target: &ObjectRef, rank: i64) -> Reply {
        let index = condition_index(target)?;
        let (_, sheet, _) = Self::report_key(target)?;
        let mut total = 0;
        self.read_host(target, |session, sheet, _| {
            rule_at(session, sheet, index)?;
            total = session.host().model().definitions().conditional_rules(sheet).len() as i64;
            Ok(AutoValue::Empty)
        })?;
        let rank = if rank == i64::MAX { total } else { rank };
        if !(1..=total).contains(&rank) {
            return Err(refused());
        }
        self.rule_change(sheet, index, ConditionalChange::Priority { rank: rank as u32 })
    }

    fn condition_interior(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.condition_part(target, call, true)
    }

    fn condition_font(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.condition_part(target, call, false)
    }

    fn condition_part(&self, target: &ObjectRef, call: &Call<'_>, interior: bool) -> Reply {
        format_arity(call, 0)?;
        let index = condition_index(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        self.read_host(target, |session, sheet, _| rule_at(session, sheet, index).map(|_| ()))
            .map(|_| ())?;
        let item = if interior { ReportItem::ConditionInterior(index) } else { ReportItem::ConditionFont(index) };
        Ok(report(workbook, sheet, if interior { "Interior" } else { "Font" }, item))
    }

    fn condition_delete(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let index = condition_index(target)?;
        let (_, sheet, _) = Self::report_key(target)?;
        self.read_host(target, |session, sheet, _| rule_at(session, sheet, index).map(|_| ()))?;
        self.rule_change(sheet, index, ConditionalChange::Remove)
    }

    fn condition_borders(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let index = condition_index(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        self.read_host(target, |session, sheet, _| rule_at(session, sheet, index).map(|_| ()))?;
        Ok(report(workbook, sheet, "Borders", ReportItem::ConditionBorders(index)))
    }

    fn condition_number_format(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let index = condition_index(target)?;
        self.read_host(target, |session, sheet, _| {
            let (_, rule) = rule_at(session, sheet, index)?;
            let dxf = dxf_of(session, &rule)?;
            Ok(match dxf.number_format {
                Some(nf) => AutoValue::text(&nf.code),
                None => AutoValue::Empty,
            })
        })
    }

    fn condition_put_number_format(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let AutoValue::Text(text) = call.arg(0) else {
            return Err(refused());
        };
        let code = String::from_utf16(text.utf16_code_units()).map_err(|_| refused())?;
        let mut id = 164u32;
        self.read_host(target, |session, _, _| {
            let styles = session.host().styles();
            id = (0..=163)
                .find(|n| {
                    calc_doc_model::builtin_number_format(*n).is_some_and(|c| {
                        c == code || (c.eq_ignore_ascii_case("general") && code.eq_ignore_ascii_case("general"))
                    })
                })
                .or_else(|| {
                    styles
                        .differential_formats
                        .iter()
                        .filter_map(|d| d.number_format.as_ref())
                        .find(|n| n.code == code)
                        .map(|n| n.id)
                })
                .or_else(|| styles.number_formats.iter().find(|n| n.code == code).map(|n| n.id))
                .unwrap_or_else(|| {
                    styles
                        .differential_formats
                        .iter()
                        .filter_map(|d| d.number_format.as_ref().map(|n| n.id))
                        .chain(styles.number_formats.iter().map(|n| n.id))
                        .max()
                        .unwrap_or(163)
                        .max(163)
                        .saturating_add(1)
                });
            Ok(AutoValue::Empty)
        })?;
        self.condition_edit(target, move |dxf| {
            dxf.number_format = Some(NumberFormat { id, code: code.clone() });
            Ok(())
        })
    }

    pub(in crate::automation) fn condition_borders_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let ObjectKey::Report { workbook, sheet, item: ReportItem::ConditionBorders(index) } = target.key else {
            return Err(geometry::type_mismatch());
        };
        let edge = integral_format_arg(call)?;
        match edge {
            DIAGONAL_DOWN..=RIGHT => {
                self.read_host(target, |session, sheet, _| rule_at(session, sheet, index).map(|_| ()))?;
                Ok(report(workbook, sheet, "Border", ReportItem::ConditionBorder(index, edge)))
            }
            11 | 12 => Err(AutoError::not_modelled("FormatCondition.Borders inside edges", REMAINDER)),
            _ => Err(refused()),
        }
    }

    pub(in crate::automation) fn condition_border_get(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let ObjectKey::Report { ref item, .. } = target.key else {
            return Err(geometry::type_mismatch());
        };
        let (index, edges): (u32, &[i32]) = match *item {
            ReportItem::ConditionBorders(n) => (n, &[LEFT, TOP, BOTTOM, RIGHT]),
            ReportItem::ConditionBorder(n, e) => (n, std::slice::from_ref(match e {
                DIAGONAL_DOWN => &DIAGONAL_DOWN,
                DIAGONAL_UP => &DIAGONAL_UP,
                LEFT => &LEFT,
                TOP => &TOP,
                BOTTOM => &BOTTOM,
                _ => &RIGHT,
            })),
            _ => return Err(geometry::type_mismatch()),
        };
        let member = call.member.name.as_str();
        self.read_host(target, |session, sheet, _| {
            let (_, rule) = rule_at(session, sheet, index)?;
            let dxf = dxf_of(session, &rule)?;
            let mut uniform = None;
            for &edge in edges {
                let value = edge_property(session.host(), side(dxf.border.as_ref(), edge), member)?;
                if uniform.is_some_and(|prior| prior != value) {
                    return Ok(AutoValue::Null);
                }
                uniform = Some(value);
            }
            Ok(AutoValue::I4(uniform.ok_or_else(refused)?))
        })
    }

    pub(in crate::automation) fn condition_border_put(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let ObjectKey::Report { ref item, .. } = target.key else {
            return Err(geometry::type_mismatch());
        };
        let mut change = EdgeChange::default();
        match call.member.name.as_str() {
            "LineStyle" => change.line = Some(pick(LINES, integral_format_arg(call)?)?),
            "Weight" => change.weight = Some(pick(WEIGHTS, integral_format_arg(call)?)?),
            "Color" => change.color = Some(rgb_color(integral_format_arg(call)?)?),
            _ => change.color = Some(indexed_color(integral_format_arg(call)?)?),
        }
        let mut patch = EdgesPatch::default();
        match *item {
            ReportItem::ConditionBorders(_) => {
                patch.left = Some(change.clone());
                patch.top = Some(change.clone());
                patch.bottom = Some(change.clone());
                patch.right = Some(change);
            }
            ReportItem::ConditionBorder(_, edge) => match edge {
                LEFT => patch.left = Some(change),
                RIGHT => patch.right = Some(change),
                TOP => patch.top = Some(change),
                BOTTOM => patch.bottom = Some(change),
                DIAGONAL_UP => patch.diagonal_up = Some(change),
                DIAGONAL_DOWN => patch.diagonal_down = Some(change),
                _ => return Err(refused()),
            },
            _ => return Err(geometry::type_mismatch()),
        }
        self.condition_edit(target, move |dxf| {
            let border = dxf.border.get_or_insert_with(Default::default);
            crate::format_edges::apply(border, &patch).map_err(|_| refused())
        })
    }

    /// `Interior.Color` and `.ColorIndex` of a rule's format.
    pub(in crate::automation) fn condition_interior_get(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let index = condition_index(target)?;
        let member = call.member.name.clone();
        self.read_host(target, |session, sheet, _| {
            let (_, rule) = rule_at(session, sheet, index)?;
            let dxf = dxf_of(session, &rule)?;
            match member.as_str() {
                // ww21 Condition.Interior.Color: unset fill answers 0.
                "Color" => {
                    let bg = dxf.fill.as_ref().and_then(|f| f.bg_color.as_ref());
                    Ok(AutoValue::I4(color_bgr(session.host(), bg, 0)?))
                }
                _ => Err(AutoError::not_modelled(format!("FormatCondition.Interior.{member}"), REMAINDER)),
            }
        })
    }

    pub(in crate::automation) fn condition_interior_put(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let member = call.member.name.as_str();
        if member != "Color" {
            return Err(AutoError::not_modelled(format!("FormatCondition.Interior.{member}"), REMAINDER));
        }
        let rgb = integral_format_arg(call)?;
        if !(0..=0x00FF_FFFF).contains(&rgb) {
            return Err(refused());
        }
        let color = StyleColor {
            rgb: Some(format!("FF{:06X}", super::font::reverse_rgb(rgb as u32))),
            ..Default::default()
        };
        self.condition_edit(target, move |dxf| {
            dxf.fill = Some(Fill {
                pattern_type: Some("solid".into()),
                // Excel takes a differential format's solid fill from bgColor.
                bg_color: Some(color.clone()),
                ..Default::default()
            });
            Ok(())
        })
    }

    /// `Font.Bold`, `.Italic`, `.Underline` and `.Color` of a rule's format.
    pub(in crate::automation) fn condition_font_get(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let index = condition_index(target)?;
        let member = call.member.name.clone();
        self.read_host(target, |session, sheet, _| {
            let (_, rule) = rule_at(session, sheet, index)?;
            let dxf = dxf_of(session, &rule)?;
            match member.as_str() {
                "Bold" => Ok(AutoValue::Bool(dxf.font.as_ref().is_some_and(|f| f.bold))),
                "Italic" => Ok(AutoValue::Bool(dxf.font.as_ref().is_some_and(|f| f.italic))),
                "Underline" => Ok(AutoValue::I4(format_enum_value(
                    super::font::UNDERLINES,
                    dxf.font.as_ref().and_then(|f| f.underline.as_deref()),
                    call,
                )?)),
                "Color" => Ok(AutoValue::I4(color_bgr(
                    session.host(),
                    dxf.font.as_ref().and_then(|f| f.color.as_ref()),
                    0,
                )?)),
                _ => Err(AutoError::not_modelled(format!("FormatCondition.Font.{member}"), REMAINDER)),
            }
        })
    }

    pub(in crate::automation) fn condition_font_put(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let member = call.member.name.clone();
        match member.as_str() {
            "Bold" | "Italic" => {
                let on = geometry::boolean(call.arg(0)).map_err(|_| refused())?;
                self.condition_edit(target, move |dxf| {
                    let font = dxf.font.get_or_insert_with(Default::default);
                    if member == "Bold" {
                        font.bold = on;
                        font.bold_off = !on;
                    } else {
                        font.italic = on;
                        font.italic_off = !on;
                    }
                    Ok(())
                })
            }
            "Underline" => {
                let keyword = format_enum_keyword(super::font::UNDERLINES, call)?;
                self.condition_edit(target, move |dxf| {
                    dxf.font.get_or_insert_with(Default::default).underline = Some(keyword.clone());
                    Ok(())
                })
            }
            "Color" => {
                let rgb = integral_format_arg(call)?;
                if !(0..=0x00FF_FFFF).contains(&rgb) {
                    return Err(refused());
                }
                let color = StyleColor {
                    rgb: Some(format!("FF{:06X}", super::font::reverse_rgb(rgb as u32))),
                    ..Default::default()
                };
                self.condition_edit(target, move |dxf| {
                    dxf.font.get_or_insert_with(Default::default).color = Some(color.clone());
                    Ok(())
                })
            }
            _ => Err(AutoError::not_modelled(format!("FormatCondition.Font.{member}"), REMAINDER)),
        }
    }

    /// Change the rule's own differential format and write it back.
    fn condition_edit(
        &self,
        target: &ObjectRef,
        edit: impl Fn(&mut DifferentialFormat) -> Result<(), AutoError>,
    ) -> Reply {
        let index = condition_index(target)?;
        let (_, sheet, _) = Self::report_key(target)?;
        let mut current = None;
        self.read_host(target, |session, sheet, _| {
            let (_, rule) = rule_at(session, sheet, index)?;
            current = Some(dxf_of(session, &rule)?);
            Ok(AutoValue::Empty)
        })?;
        let mut dxf = current.ok_or_else(refused)?;
        edit(&mut dxf)?;
        self.rule_change(sheet, index, ConditionalChange::Format { dxf: Box::new(dxf) })
    }
}
