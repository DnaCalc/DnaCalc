use crate::{CopiedCells, State, cell, grid_area, invalid, unavailable};
use calc_doc_model::{CellInput, ModelRoot, ScenarioInput, ViewAnchor, WorksheetScenario, WorksheetScenarios};
use calc_host::value::{CellKey, GridArea, SheetId};
use calc_host::{
    Axis, CellEdit, CommandError, HostSession, IntentError, PasteArithmetic as HostPasteArithmetic,
    PasteCellOptions, PasteMode, PasteOptionsRefusal, PasteSource, SortKey, SortOptions,
    prepare_column_visibility, prepare_filter_toggle, prepare_paste, prepare_paste_column_widths,
    prepare_paste_oriented, prepare_row_visibility, prepare_sort_rows,
    search::entry_text,
};
use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::*;
use std::{collections::{BTreeMap, BTreeSet}, sync::Arc};

pub(crate) fn source_area(area: GridArea) -> SourceRef {
    SourceRef::Cells {
        sheet: Sheet::of(area.sheet()),
        areas: vec![Area::of(area)],
    }
}

pub(crate) fn refusal(error: IntentError) -> Refusal {
    match error {
        IntentError::Irreversible(error) => Refusal::Irreversible {
            reason: error.to_string(),
        },
        error @ IntentError::MacroCheckpointRequired(_) => Refusal::Irreversible {
            reason: error.to_string(),
        },
        IntentError::Command(CommandError::NotModelled { owner, what }) => Refusal::NotModelled {
            owner: owner.into(),
            what: what.into(),
        },
        error => Refusal::Excel {
            reason: error.to_string(),
        },
    }
}

fn editability(session: &HostSession, area: GridArea, entry: bool) -> Result<(), Refusal> {
    let host = session.host();
    if host.model().sheet_by_id(area.sheet()).is_none() {
        return Err(invalid("The worksheet is unavailable"));
    }
    let at = area.first();
    let protected = if entry {
        host.loaded().sheet(at.sheet()).is_some_and(|sheet| sheet.is_macro_sheet())
            || (host.sheet_contents_protected(at.sheet()) && host.style_is_locked(host.cell_style(at)))
    } else {
        calc_host::can_edit(host, at).is_err()
    };
    if protected {
        return Err(Refusal::Protected(source_area(area)));
    }
    if entry {
        if host.spill_owner(at).is_some_and(|anchor| anchor != at) {
            return Err(Refusal::Excel {
                reason: "This cell belongs to a spilled array".into(),
            });
        }
    }
    Ok(())
}

pub(crate) fn rules(session: &HostSession, at: CellKey) -> Result<(), Refusal> {
    editability(session, GridArea::cell(at), true)?;
    let definitions = session.host().model().definitions();
    if definitions.pivots().contains(at) {
        return Err(Refusal::Excel {
            reason: "PivotTable output is read-only".into(),
        });
    }
    if definitions.array_containing(at).is_some_and(|array| {
        array.area().cell_count() > 1
            && !matches!(array.kind(), calc_doc_model::ArrayKind::Dynamic { .. })
    }) {
        return Err(Refusal::Excel {
            reason: "This cell belongs to an array formula".into(),
        });
    }
    Ok(())
}

pub(crate) struct Prepared {
    pub change: PreparedChange,
    pub preview: Preview,
    pub after: Arc<ModelRoot>,
}

pub(crate) enum PreparedChange {
    Edits(Vec<CellEdit>),
    Paste(calc_host::PastePlan),
}

fn range_properties(
    before: &ModelRoot,
    after: &ModelRoot,
    areas: &[GridArea],
    edits: &[CellEdit],
) -> Vec<ModelPropertyDiff> {
    let mut properties = Vec::new();
    let before = before.definitions();
    let after = after.definitions();
    let formats: BTreeSet<_> = edits
        .iter()
        .filter_map(|edit| match edit {
            CellEdit::SetFormat { at, .. } => Some(*at),
            _ => None,
        })
        .collect();
    for at in formats {
        let old = before.cell_format(at);
        let new = after.cell_format(at);
        if old != new {
            properties.push(ModelPropertyDiff::CellFormat {
                at: Key::of(at),
                before: old,
                after: new,
            });
        }
    }
    if before.styles() != after.styles() {
        properties.push(ModelPropertyDiff::Scope {
            source: SourceRef::Workbook,
            property: ModelProperty::StyleTable,
        });
    }
    let sheets: BTreeSet<_> = areas.iter().map(|area| area.sheet()).collect();
    for sheet in sheets {
        let changes = [
            (ModelProperty::MergedCells, before.merges(sheet) != after.merges(sheet)),
            (
                ModelProperty::ConditionalFormatting,
                before.conditional_rules(sheet) != after.conditional_rules(sheet),
            ),
            (ModelProperty::DataValidation, before.validations(sheet) != after.validations(sheet)),
            (ModelProperty::Notes, before.review_notes(sheet) != after.review_notes(sheet)),
            (ModelProperty::Hyperlinks, before.review_links(sheet) != after.review_links(sheet)),
        ];
        for (property, changed) in changes {
            if changed {
                properties.push(ModelPropertyDiff::Scope {
                    source: SourceRef::Sheet(Sheet::of(sheet)),
                    property,
                });
            }
        }
    }
    for &area in areas {
        if before.arrays_meeting(area).ne(after.arrays_meeting(area)) {
            properties.push(ModelPropertyDiff::Scope {
                source: source_area(area),
                property: ModelProperty::ArrayFormulas,
            });
        }
        let old_text = before.text_sources()
            .range(area.first()..=area.last())
            .filter(|(at, _)| area.contains(**at));
        let new_text = after.text_sources()
            .range(area.first()..=area.last())
            .filter(|(at, _)| area.contains(**at));
        if old_text.ne(new_text) {
            properties.push(ModelPropertyDiff::Scope {
                source: source_area(area),
                property: ModelProperty::RichText,
            });
        }
    }
    properties
}

fn axis_properties(
    before: &ModelRoot,
    after: &ModelRoot,
    edits: &[CellEdit],
) -> Vec<ModelPropertyDiff> {
    let before = before.definitions();
    let after = after.definitions();
    let mut sheets = BTreeMap::<_, ((bool, bool), (bool, bool))>::new();
    for edit in edits {
        let CellEdit::AxisFormat(edit) = edit else { continue };
        let layout = |root: &calc_doc_model::Definitions| {
            root.axis_layouts().get(edit.sheet(), edit.axis(), edit.index())
        };
        let old = layout(before);
        let new = layout(after);
        let visibility = old.hidden() != new.hidden();
        let size = old.size() != new.size() || old.custom() != new.custom();
        if visibility || size {
            let (rows, columns) = sheets.entry(edit.sheet()).or_default();
            let (changed_visibility, changed_size) = match edit.axis() {
                Axis::Rows => rows,
                Axis::Columns => columns,
            };
            *changed_visibility |= visibility;
            *changed_size |= size;
        }
    }
    let mut properties = Vec::new();
    for (sheet, ((row_visibility, row_height), (column_visibility, column_width))) in sheets {
        let source = SourceRef::Sheet(Sheet::of(sheet));
        for (property, changed) in [
            (ModelProperty::RowVisibility, row_visibility),
            (ModelProperty::RowHeights, row_height),
            (ModelProperty::ColumnVisibility, column_visibility),
            (ModelProperty::ColumnWidths, column_width),
            (ModelProperty::Charts, before.charts(sheet) != after.charts(sheet)),
            (ModelProperty::Shapes, before.shapes(sheet) != after.shapes(sheet)),
        ] {
            if changed {
                properties.push(ModelPropertyDiff::Scope {
                    source: source.clone(),
                    property,
                });
            }
        }
    }
    properties
}

fn require_current_model(request: &Batch, current: Revision) -> Result<(), Refusal> {
    let expected = request
        .base
        .ok_or_else(|| invalid("This change requires a workbook revision."))?;
    if expected != current {
        return Err(Refusal::Conflict {
            base: expected,
            conflicts: vec![Target::Source(SourceRef::Workbook)],
            diffs: Vec::new(),
        });
    }
    Ok(())
}

fn require_formula(
    root: &calc_doc_model::ModelRoot,
    at: CellKey,
    expected_old: &str,
    text: &str,
    base: Revision,
) -> Result<(), Refusal> {
    let input = root.cell(at);
    if !matches!(input, Some(calc_doc_model::CellInput::Formula(_)))
        || entry_text(input) != expected_old
    {
        return Err(Refusal::Conflict {
            base,
            conflicts: vec![Target::Source(source_area(GridArea::cell(at)))],
            diffs: Vec::new(),
        });
    }
    if !text.starts_with('=') {
        return Err(invalid("The replacement must be a formula"));
    }
    Ok(())
}

fn rewrite_run(
    state: &State,
    at: CellKey,
    extent: Area,
    text: &str,
    base: Revision,
) -> Result<(Vec<GridArea>, Vec<CellEdit>), Refusal> {
    let area = grid_area(extent)?;
    if area.column_count() != 1 || !area.contains(at) {
        return Err(invalid("Choose the column run that contains the inspected cell"));
    }
    let mut work = crate::query::QueryWork::new(
        Budget {
            cells: 10_000,
            nodes: 10_000,
            edges: 0,
            bytes: 4 * 1024 * 1024,
            millis: 2_000,
        },
        "Preview the single-cell repair",
    );
    if area.cell_count() > 10_000 {
        work.work(area.cell_count(), 0, 0)?;
    }
    let source = source_area(area);
    let structure = crate::structure::structure(state, &source, &mut work)?;
    let targets = structure
        .runs
        .iter()
        .find(|run| run.extent == extent)
        .map(|run| {
            run.members
                .iter()
                .copied()
                .map(grid_area)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .filter(|members| members.iter().any(|area| area.contains(at)))
        .ok_or_else(|| Refusal::Conflict {
            base,
            conflicts: vec![Target::Source(source.clone())],
            diffs: Vec::new(),
        })?;
    for target in &targets {
        for row in target.first().row()..=target.last().row() {
            let member = CellKey::new(at.sheet(), row, at.column()).expect("member of the same run");
            rules(&state.session, member)?;
            if state.session.host().number_format(member) == Some("@") {
                return Err(Refusal::Excel {
                    reason: "A cell in the run is formatted as text".into(),
                });
            }
        }
    }
    let edits = state
        .session
        .prepare_entries(at, text, &targets)
        .map_err(|error| match error {
            IntentError::Validation(failure) => Refusal::Validation {
                target: source,
                reason: failure.to_string(),
            },
            other => refusal(other),
        })?;
    Ok((targets, edits))
}

fn plan_spill_move(
    state: &State,
    anchor: Key,
    from: Key,
    to: Key,
) -> Result<(GridArea, calc_host::PastePlan), Refusal> {
    let anchor = cell(anchor)?;
    let source = cell(from)?;
    let destination = cell(to)?;
    rules(&state.session, source)?;
    rules(&state.session, destination)?;
    let host = state.session.host();
    if host.model().cell(source).is_none() {
        return Err(invalid("Choose an occupied cell to move"));
    }
    let actual = host.spill_area(anchor).filter(|_| host.spill_owner(anchor) == Some(anchor));
    let blocked = host.blocked_spill_area(anchor);
    let (moving, desired) = if source == anchor {
        let intended = actual.or(blocked).ok_or_else(|| invalid("Choose a current dynamic spill"))?;
        (actual.unwrap_or_else(|| GridArea::cell(source)), intended)
    } else {
        let spill = blocked.filter(|area| area.contains(source))
            .ok_or_else(|| invalid("That cell is outside the current blocked spill"))?;
        if spill.contains(destination) {
            return Err(invalid("Move the obstruction outside the intended spill"));
        }
        let area = GridArea::cell(source);
        (area, area)
    };
    let corner = CellKey::new(
        destination.sheet(),
        destination.row() + desired.row_count() - 1,
        destination.column() + desired.column_count() - 1,
    ).map_err(|_| invalid("The destination reaches outside the worksheet"))?;
    let footprint = GridArea::new(destination, corner).expect("same sheet");
    if host.model().cells(footprint).into_iter().flatten().next().is_some()
        || host.model().definitions().unloaded_in(footprint).next().is_some()
    {
        return Err(invalid("The destination is occupied"));
    }
    let excel = |reason| Refusal::Excel { reason };
    calc_host::check_destination(host, footprint, PasteMode::All)
        .map_err(|error| excel(error.to_string()))?;
    let snapshot = calc_host::CopySnapshot::capture(host, moving)
        .map_err(|error| excel(error.to_string()))?;
    let paste = calc_host::prepare_paste(
        host, PasteSource::Snapshot { snapshot: &snapshot, cut: true },
        GridArea::cell(destination), PasteMode::All,
    ).map_err(excel)?;
    let paste = if source == anchor { paste.with_target_selection() } else { paste };
    Ok((moving, paste))
}

pub(crate) fn prepare(
    state: &mut State,
    request: &Batch,
    copied: Option<&CopiedCells>,
) -> Result<Prepared, Refusal> {
    let base = state.stamp();
    state.check_base(request)?;
    if request
        .selection_seq
        .is_some_and(|seq| seq != state.selection_seq)
    {
        return Err(Refusal::StaleSelection);
    }
    let before = state.session.host().model().clone();
    let mut part = (**before.view_part()).clone();
    let mut edits = Vec::new();
    let mut areas = Vec::new();
    let mut ranges_with_metadata = Vec::new();
    let mut filter_sheets = BTreeSet::new();
    let mut pivot_cache = None;
    let mut inserted = None;
    let mut presentation = Vec::new();
    let mut warnings = Vec::new();
    let mut cut = None;
    let mut names = Vec::new();
    let mut scenario_changes = Vec::new();
    let removes_anchor = request.changes.iter().any(|change| {
        matches!(change, Change::Presentation(PresentationChange::RemoveAnchor { .. }))
    });
    for change in &request.changes {
        match change {
            Change::Model(change) => match change {
                ModelChange::MoveSpillCell { anchor, from, to, expected } => {
                    if *expected != base {
                        return Err(invalid("The spill changed. Review the move again."));
                    }
                    require_current_model(request, base.model)?;
                    if request.changes.len() != 1 {
                        return Err(invalid("Submit the cell move as a single workbook change"));
                    }
                    let (moving, paste) = plan_spill_move(state, *anchor, *from, *to)?;
                    let host = state.session.host();
                    let target = paste.target();
                    let prepared = paste
                        .clone()
                        .into_edits(host)
                        .map_err(|reason| Refusal::Excel { reason })?;
                    areas.extend([moving, target]);
                    areas.extend(prepared.iter().filter_map(|edit| match edit {
                        CellEdit::Set { at, .. } => Some(GridArea::cell(*at)),
                        CellEdit::SetArray { array, .. } => Some(array.area()),
                        _ => None,
                    }));
                    ranges_with_metadata.extend([moving, target]);
                    edits.extend(prepared);
                    cut = Some((paste, *from, *to));
                }
                ModelChange::Name { scope, change } => {
                    require_current_model(request, base.model)?;
                    let (plan, prepared, used) = crate::names::prepare(state, scope.clone(), change)?;
                    if used {
                        warnings.push("Other formulas read the name being removed.".into());
                    }
                    areas.extend(prepared.iter().filter_map(|edit| match edit {
                        CellEdit::Set { at, .. } => Some(GridArea::cell(*at)),
                        CellEdit::SetArray { array, .. } => Some(array.area()),
                        _ => None,
                    }));
                    edits.extend(prepared);
                    names.push(plan);
                }
                ModelChange::InsertWorksheet { name, sheet } => {
                    if !edits.is_empty() || inserted.is_some() {
                        return Err(invalid("Insert the worksheet before entering its cells"));
                    }
                    if request.base != Some(base.model) {
                        return Err(Refusal::Conflict {
                            base: request.base.unwrap_or(base.model),
                            conflicts: vec![Target::Source(SourceRef::Workbook)],
                            diffs: Vec::new(),
                        });
                    }
                    let (id, edit) = state.session.prepare_worksheet_insert(name.clone()).map_err(refusal)?;
                    if Sheet::of(id) != *sheet {
                        return Err(invalid("The worksheet location changed. Request a fresh placement."));
                    }
                    inserted = Some((*sheet, name.clone()));
                    edits.push(edit);
                }
                ModelChange::Enter { at, text }
                | ModelChange::RewriteFormula {
                    at, new_text: text, ..
                } => {
                    let at = cell(*at)?;
                    if !inserted.as_ref().is_some_and(|(sheet, _)| sheet.id() == at.sheet()) {
                        rules(&state.session, at)?;
                    }
                    if let ModelChange::RewriteFormula { expected_old, .. } = change {
                        require_formula(
                            &before,
                            at,
                            expected_old,
                            text,
                            request.base.unwrap_or(base.model),
                        )?;
                    }
                    edits.extend(prepare_entry(&state.session, at, text)?);
                    areas.push(GridArea::cell(at));
                }
                ModelChange::EnterBound { bound, text } => {
                    let at = crate::query::bound_cell(state, bound)?;
                    edits.extend(prepare_literal(&state.session, at, text)?);
                    areas.push(GridArea::cell(at));
                }
                ModelChange::ReplaceReference { at, expected_old, occurrence, with } => {
                    require_current_model(request, base.model)?;
                    let at = cell(*at)?;
                    rules(&state.session, at)?;
                    require_formula(&before, at, expected_old, expected_old, request.base.unwrap_or(base.model))?;
                    let (text, target) = crate::references::replace(
                        state.session.host(), at, expected_old, *occurrence, with,
                    )?;
                    if target == at {
                        warnings.push("This edit would refer to its own formula cell.".into());
                    }
                    edits.extend(prepare_entry(&state.session, at, &text)?);
                    areas.push(GridArea::cell(at));
                }
                ModelChange::IncrementBound { bound } => {
                    require_current_model(request, base.model)?;
                    let at = crate::query::bound_cell(state, bound)?;
                    literal_target(&state.session, at)?;
                    let Some(CellInput::Number(count)) = before.cell(at) else {
                        return Err(invalid("The control needs a nonnegative integer"));
                    };
                    let count = count.get();
                    if count.fract() != 0.0 || !(0.0..999_999_999_999_999.0).contains(&count) {
                        return Err(invalid("The control needs an incrementable nonnegative integer"));
                    }
                    let next = (count as u64 + 1).to_string();
                    edits.extend(prepare_literal(&state.session, at, &next)?);
                    areas.push(GridArea::cell(at));
                }
                ModelChange::ShowScenario { scenario } => {
                    require_current_model(request, base.model)?;
                    let scenario = crate::inventory::worksheet_scenarios(
                        state.session.host(), scenario.sheet,
                    ).find(|entry| entry.name == scenario.name)
                        .ok_or_else(|| invalid("The worksheet scenario is unavailable"))?;
                    for input in &scenario.inputs {
                        let text = String::from_utf16(input.value.utf16_code_units())
                            .map_err(|_| unavailable("dc-zh096", "This scenario’s input text"))?;
                        edits.extend(prepare_literal(&state.session, input.at, &text)?);
                        areas.push(GridArea::cell(input.at));
                    }
                }
                ModelChange::SaveScenario { scenario, inputs } => {
                    require_current_model(request, base.model)?;
                    let change = save_scenario(state, scenario, ScenarioCells::Bindings(inputs), &mut edits)?;
                    scenario_changes.push(change);
                }
                ModelChange::SaveScenarioCells { scenario, areas } => {
                    require_current_model(request, base.model)?;
                    let change = save_scenario(state, scenario, ScenarioCells::Areas(areas), &mut edits)?;
                    scenario_changes.push(change);
                }
                ModelChange::EditScenario { scenario, change } => {
                    require_current_model(request, base.model)?;
                    scenario_changes.extend(edit_scenario(state, scenario, change, &mut edits)?);
                }
                ModelChange::ResetInputs { inputs } => {
                    require_current_model(request, base.model)?;
                    let host = state.session.host();
                    let opening = host.loaded().model();
                    let mut seen = BTreeSet::new();
                    for bound in inputs {
                        let at = crate::query::bound_cell(state, bound)?;
                        if !seen.insert(at) {
                            continue;
                        }
                        literal_target(&state.session, at)?;
                        let original = host.source_at_open(at);
                        let input = original.and_then(|at| opening.cell(at));
                        if matches!(input, Some(CellInput::Formula(_)))
                            || original.is_some_and(|at| opening.definitions().not_available(at, input.is_some()))
                        {
                            return Err(invalid("This input's opening value was not a worksheet literal"));
                        }
                        let area = GridArea::cell(at);
                        if let Err(failure) = host.check_entry(at, input)
                            && failure.style == calc_host::ErrorStyle::Stop
                        {
                            return Err(Refusal::Validation {
                                target: source_area(area),
                                reason: failure.to_string(),
                            });
                        }
                        edits.push(match input {
                            Some(input) => CellEdit::Set { at, input: input.clone() },
                            None => CellEdit::Clear { area },
                        });
                        areas.push(area);
                    }
                }
                ModelChange::RewriteFormulaRun {
                    at,
                    expected_old,
                    new_text,
                    extent,
                } => {
                    let at = cell(*at)?;
                    let base = request.base.unwrap_or(base.model);
                    require_formula(&before, at, expected_old, new_text, base)?;
                    let (targets, replacements) = rewrite_run(state, at, *extent, new_text, base)?;
                    areas.extend(targets);
                    edits.extend(replacements);
                }
                ModelChange::ClearContents { areas: targets } => {
                    for target in targets {
                        let area = grid_area(*target)?;
                        editability(&state.session, area, false)?;
                        if removes_anchor && warnings.is_empty() {
                            let host = state.session.host();
                            let reads = host.spill_area(area.first()).unwrap_or(area);
                            if host.dependency_readers(reads).any(|at| !area.contains(at)) {
                                warnings.push("Other formulas read the cells being cleared.".into());
                            }
                        }
                        edits.push(CellEdit::Clear { area });
                        areas.push(area);
                    }
                }
                ModelChange::PasteCopied { copy, to, contents } => {
                    // A single-cell destination can expand. Keep the preview's whole model
                    // revision so a concurrent edit outside the selected cell cannot be lost.
                    require_current_model(request, base.model)?;
                    let copied = copied
                        .filter(|copied| (copied.workbook, copied.id) == (base.workbook, *copy))
                        .ok_or_else(|| invalid("Copy worksheet cells again before pasting."))?;
                    let plain = PasteCellOptions::default();
                    let (mode, transpose, options) = match contents {
                        PasteContents::Values => (Some(PasteMode::Values), false, plain),
                        PasteContents::Formulas => (Some(PasteMode::Formulas), false, plain),
                        PasteContents::AllTransposed => (Some(PasteMode::All), true, plain),
                        PasteContents::Link => (Some(PasteMode::Formulas), false, plain),
                        PasteContents::Special { part, transpose, skip_blanks, arithmetic } => {
                            let mode = match part {
                                PastePart::All | PastePart::AllUsingSourceTheme => Some(PasteMode::All),
                                PastePart::Formulas => Some(PasteMode::Formulas),
                                PastePart::Values => Some(PasteMode::Values),
                                PastePart::Formats => Some(PasteMode::Formats),
                                PastePart::Comments => Some(PasteMode::Comments),
                                PastePart::Validation => Some(PasteMode::Validation),
                                PastePart::AllExceptBorders => Some(PasteMode::AllExceptBorders),
                                PastePart::FormulasAndNumberFormats => Some(PasteMode::FormulasAndNumberFormats),
                                PastePart::ValuesAndNumberFormats => Some(PasteMode::ValuesAndNumberFormats),
                                PastePart::AllMergingConditionalFormats => Some(PasteMode::AllMergingConditionalFormats),
                                PastePart::ColumnWidths => None,
                            };
                            let arithmetic = arithmetic.map(|operation| match operation {
                                PasteArithmetic::Add => HostPasteArithmetic::Add,
                                PasteArithmetic::Subtract => HostPasteArithmetic::Subtract,
                                PasteArithmetic::Multiply => HostPasteArithmetic::Multiply,
                                PasteArithmetic::Divide => HostPasteArithmetic::Divide,
                            });
                            (mode, *transpose, PasteCellOptions { skip_blanks: *skip_blanks, arithmetic })
                        }
                    };
                    let host = state.session.host();
                    let to = grid_area(*to)?;
                    let Some(mode) = mode else {
                        // Excel's Column Widths uses axis edits independently of cell options.
                        if host.sheet_contents_protected(to.sheet()) {
                            return Err(Refusal::Protected(SourceRef::Sheet(Sheet::of(to.sheet()))));
                        }
                        let (target, widths) = prepare_paste_column_widths(host, &copied.snapshot, to)
                            .map_err(|reason| Refusal::Excel { reason: reason.to_string() })?;
                        areas.push(target);
                        edits.extend(widths);
                        continue;
                    };
                    let excel = |reason| Refusal::Excel { reason };
                    let source = match contents {
                        PasteContents::Link => PasteSource::Link(&copied.snapshot),
                        _ => PasteSource::Snapshot {
                            snapshot: &copied.snapshot,
                            cut: false,
                        },
                    };
                    let plan = prepare_paste_oriented(host, source, to, mode, transpose)
                    .map_err(excel)?
                    .with_cell_options(host, options)
                    .map_err(|reason| match reason {
                        PasteOptionsRefusal::WorkbookChanged => invalid("The workbook changed after the preview; paste again."),
                        PasteOptionsRefusal::ProjectionUnavailable => invalid("The destination values are unavailable."),
                        PasteOptionsRefusal::NotModelled(what) => Refusal::NotModelled {
                            owner: "dc-oxh8w.43".into(),
                            what: format!("Paste Special operation {what}"),
                        },
                    })?;
                    let target = plan.target();
                    areas.push(target);
                    if !ranges_with_metadata.contains(&target) {
                        ranges_with_metadata.push(target);
                    }
                    edits.extend(plan.into_edits(host).map_err(excel)?);
                }
                ModelChange::FillRange { selected, direction } => {
                    require_current_model(request, base.model)?;
                    let selected = grid_area(*selected)?;
                    let first = selected.first();
                    let last = selected.last();
                    let key = |row, column| {
                        CellKey::new(selected.sheet(), row, column).expect("inside the selection")
                    };
                    let (seed, destination) = match direction {
                        FillDirection::Down if first.row() < last.row() => (
                            (first, key(first.row(), last.column())),
                            (key(first.row() + 1, first.column()), last),
                        ),
                        FillDirection::Right if first.column() < last.column() => (
                            (first, key(last.row(), first.column())),
                            (key(first.row(), first.column() + 1), last),
                        ),
                        FillDirection::Right if selected.cell_count() == 1 && first.column() > 1 => {
                            let left = key(first.row(), first.column() - 1);
                            ((left, left), (first, last))
                        }
                        FillDirection::Up if first.row() < last.row() => (
                            (key(last.row(), first.column()), last),
                            (first, key(last.row() - 1, last.column())),
                        ),
                        FillDirection::Left if first.column() < last.column() => (
                            (key(first.row(), last.column()), last),
                            (first, key(last.row(), last.column() - 1)),
                        ),
                        FillDirection::Down => return Err(invalid("Select at least two rows to fill down.")),
                        FillDirection::Right => return Err(invalid("Select at least two columns to fill right.")),
                        FillDirection::Up => return Err(invalid("Select at least two rows to fill up.")),
                        FillDirection::Left => return Err(invalid("Select at least two columns to fill left.")),
                    };
                    let from = GridArea::new(seed.0, seed.1).expect("an ordered seed range");
                    let target = GridArea::new(destination.0, destination.1).expect("an ordered fill range");
                    let host = state.session.host();
                    let excel = |reason| Refusal::Excel { reason };
                    let plan = prepare_paste_oriented(
                        host,
                        PasteSource::Fill {
                            from,
                            down: matches!(direction, FillDirection::Down | FillDirection::Up),
                            series: false,
                        },
                        target,
                        PasteMode::All,
                        false,
                    ).map_err(excel)?;
                    areas.push(target);
                    if !ranges_with_metadata.contains(&target) {
                        ranges_with_metadata.push(target);
                    }
                    edits.extend(plan.into_edits(host).map_err(excel)?);
                }
                ModelChange::FlashFill { selected } => {
                    require_current_model(request, base.model)?;
                    let host = state.session.host();
                    let excel = |reason| Refusal::Excel { reason };
                    let plan = prepare_paste(host, PasteSource::FlashFill, grid_area(*selected)?, PasteMode::Values)
                        .map_err(excel)?;
                    areas.push(plan.target());
                    edits.extend(plan.into_edits(host).map_err(excel)?);
                    warnings.push("Flash Fill writes literal text.".into());
                }
                ModelChange::SetColumnsHidden { sheet, bands, hidden }
                | ModelChange::SetRowsHidden { sheet, bands, hidden } => {
                    require_current_model(request, base.model)?;
                    let host = state.session.host();
                    let visibility = if matches!(change, ModelChange::SetRowsHidden { .. }) {
                        prepare_row_visibility(host, sheet.id(), bands, *hidden)
                    } else {
                        prepare_column_visibility(host, sheet.id(), bands, *hidden)
                    };
                    edits.extend(visibility.map_err(|reason| Refusal::Excel { reason })?);
                }
                ModelChange::ToggleAutoFilter { selected } => {
                    require_current_model(request, base.model)?;
                    let selected = grid_area(*selected)?;
                    edits.extend(prepare_filter_toggle(state.session.host(), selected)
                        .map_err(|reason| Refusal::Excel { reason })?);
                    filter_sheets.insert(selected.sheet());
                }
                ModelChange::SetValidation { selected, settings } => {
                    require_current_model(request, base.model)?;
                    if request.changes.len() != 1 {
                        return Err(invalid("Apply Data Validation as one workbook change"));
                    }
                    let selected = grid_area(*selected)?;
                    if let Some(edit) = crate::validation::prepare(state.session.host(), selected, settings)? {
                        edits.push(edit);
                        areas.push(selected);
                        ranges_with_metadata.push(selected);
                    }
                }
                ModelChange::SetAllBorders { selected }
                | ModelChange::SetOutlineBorder { selected }
                | ModelChange::SetThickOutlineBorder { selected }
                | ModelChange::ClearBorders { selected }
                | ModelChange::SetSideBorder { selected, .. } => {
                    require_current_model(request, base.model)?;
                    if request.changes.len() != 1 {
                        return Err(invalid("Apply borders as one workbook change"));
                    }
                    let selected = grid_area(*selected)?;
                    let host = state.session.host();
                    let side = match change {
                        ModelChange::SetSideBorder { side, .. } => Some(*side),
                        _ => None,
                    };
                    let affected = calc_host::border_affected_areas(selected, side);
                    if host.merged_areas(selected.sheet()).iter().any(|merge| {
                        affected.iter().any(|area| merge.intersection(*area).is_some())
                    }) {
                        return Err(unavailable("dc-s2xup", "Applying borders at merged cells"));
                    }
                    let excel = |reason| Refusal::Excel { reason };
                    let border_edits = match change {
                        ModelChange::SetAllBorders { .. } => calc_host::prepare_all_borders(host, selected),
                        ModelChange::SetOutlineBorder { .. } => calc_host::prepare_outline_border(host, selected),
                        ModelChange::SetThickOutlineBorder { .. } => {
                            calc_host::prepare_thick_outline_border(host, selected)
                        }
                        ModelChange::SetSideBorder { side, .. } => {
                            calc_host::prepare_side_border(host, selected, *side)
                        }
                        ModelChange::ClearBorders { .. } => {
                            for at in calc_host::format_samples(host, &affected).map_err(excel)? {
                                let border = calc_host::border_for_style(host.styles(), host.cell_style(at))
                                    .map_err(excel)?;
                                if border.is_some_and(|border| !border.other_children.is_empty()) {
                                    return Err(unavailable("dc-s2xup", "Removing unmodelled imported border fields"));
                                }
                            }
                            calc_host::prepare_no_borders(host, selected)
                        }
                        _ => unreachable!("only worksheet border commands enter this arm"),
                    };
                    edits.extend(border_edits.map_err(excel)?);
                    areas.extend(affected.iter().copied());
                    ranges_with_metadata.extend(affected);
                }
                ModelChange::RefreshPivot { at, expected } => {
                    require_current_model(request, base.model)?;
                    if expected.workbook != base.workbook || expected.calc != base.calc || expected.model != base.model {
                        return Err(invalid("The workbook or calculation changed. Review the PivotTable again."));
                    }
                    if request.changes.len() != 1 {
                        return Err(invalid("Refresh the PivotTable cache in a single workbook change"));
                    }
                    let at = cell(*at)?;
                    let table = before.definitions().pivots().tables().find(|table| table.area.contains(at))
                        .ok_or_else(|| invalid("Choose a cell inside the PivotTable"))?;
                    edits.extend(state.session.prepare_pivot_refresh(at.sheet(), &table.part).map_err(refusal)?);
                    pivot_cache = Some(table.cache_id);
                }
                ModelChange::SortRows { selected, keys, header, case_sensitive } => {
                    require_current_model(request, base.model)?;
                    let selected = grid_area(*selected)?;
                    let host = state.session.host();
                    if host.sheet_contents_protected(selected.sheet()) {
                        return Err(unavailable("dc-s2xup", "Sorting a protected worksheet"));
                    }
                    let keys = keys.iter().map(|key| {
                        if key.column >= selected.column_count() {
                            return Err(invalid("Every sort key must lie inside the selected range"));
                        }
                        Ok(SortKey {
                            column: selected.first().column() + key.column,
                            descending: key.descending,
                        })
                    }).collect::<Result<Vec<_>, _>>()?;
                    let options = SortOptions { case_sensitive: *case_sensitive, ..Default::default() };
                    edits.extend(prepare_sort_rows(host, selected, &keys, *header, base.model.0, &options)
                        .map_err(|reason| Refusal::Excel { reason })?);
                    areas.push(selected);
                    if !ranges_with_metadata.contains(&selected) {
                        ranges_with_metadata.push(selected);
                    }
                }
            },
            Change::Presentation(change) => {
                if part.is_opaque() {
                    return Err(invalid("This workbook has an unreadable view part"));
                }
                let next = match change {
                    PresentationChange::Bind { anchor, source } => {
                        let binding = match source {
                            SourceRef::Cells { sheet, areas } if areas.len() == 1 => {
                                let area = grid_area(areas[0])?;
                                if Sheet::of(area.sheet()) != *sheet {
                                    return Err(invalid(
                                        "The anchor's address is on another sheet",
                                    ));
                                }
                                ViewAnchor::cells(*anchor, area)
                            }
                            SourceRef::Spill { anchor: at } => {
                                ViewAnchor::spill(*anchor, cell(*at)?)
                            }
                            SourceRef::Object { sheet, name } => {
                                let key = before.sheet_by_id(sheet.id())
                                    .filter(|_| state.session.host().has_named_chart(sheet.id(), name))
                                    .map(|entry| entry.key())
                                    .ok_or_else(|| invalid("The chart is unavailable"))?;
                                ViewAnchor::object(*anchor, key, name.clone())
                            }
                            _ => return Err(unavailable("dc-aje2v", "This anchor source")),
                        };
                        if binding.object_target().is_none()
                            && binding.area().is_none_or(|area| {
                                before.sheet_by_id(area.sheet()).is_none()
                                    && !inserted.as_ref().is_some_and(|(sheet, _)| sheet.id() == area.sheet())
                            })
                        {
                            return Err(invalid("The anchor's sheet is unavailable"));
                        }
                        part.clone().with_anchor(binding)
                    }
                    PresentationChange::RemoveAnchor { anchor } => {
                        part.clone().without_anchor(*anchor)
                    }
                    PresentationChange::PutNote { id, anchor, text } => {
                        if part.anchor(*anchor).is_none()
                            && !part.note(*id).is_some_and(|note| note.anchor == *anchor)
                        {
                            return Err(invalid("Choose an existing anchor for this note"));
                        }
                        part.clone().with_note(calc_doc_model::ViewNote {
                            id: *id,
                            anchor: *anchor,
                            text: text.clone(),
                        })
                    }
                    PresentationChange::RemoveNote { id } => part.clone().without_note(*id),
                    PresentationChange::CreateDoc {
                        doc,
                        expect_view_rev,
                        view,
                        sheet,
                        schema,
                        body,
                    } => {
                        if expect_view_rev.is_some_and(|expected| expected != base.view) {
                            return Err(invalid(
                                "The workbook presentation changed. Review the page again.",
                            ));
                        }
                        if part.document(*doc).is_some() {
                            return Err(invalid("This view document already exists"));
                        }
                        let sheet = sheet
                            .map(|sheet| {
                                before
                                    .sheet_by_id(sheet.id())
                                    .map(|sheet| sheet.key())
                                    .ok_or_else(|| {
                                        invalid("The view document's worksheet is unavailable")
                                    })
                            })
                            .transpose()?;
                        part.clone().with_document(calc_doc_model::ViewDocument {
                            id: *doc,
                            view: view.0.clone(),
                            sheet,
                            schema: *schema,
                            body: encode_document(body)?,
                        })
                    }
                    PresentationChange::PatchDoc {
                        doc,
                        expect_view_rev,
                        body,
                    } => {
                        if *expect_view_rev != base.view {
                            return Err(invalid(
                                "The view document has changed. Reopen or retry the edit.",
                            ));
                        }
                        let mut document = part
                            .document(*doc)
                            .cloned()
                            .ok_or_else(|| invalid("This view document no longer exists"))?;
                        document.body = encode_document(body)?;
                        part.clone().with_document(document)
                    }
                    PresentationChange::RemoveDoc { doc } => part.clone().without_document(*doc),
                    PresentationChange::KeepAppPage { doc, page, expect_view_rev } => {
                        if *expect_view_rev != base.view {
                            return Err(invalid("The workbook presentation changed. Review the App again."));
                        }
                        if part.document(*doc).is_some_and(|existing| existing.view != "app") {
                            return Err(invalid("The document ID belongs to another view"));
                        }
                        crate::query::verify_page_bindings(state, page)?;
                        part.clone().with_document(calc_doc_model::ViewDocument {
                            id: *doc,
                            view: "app".into(),
                            sheet: None,
                            schema: 1,
                            body: serde_json::to_string(page)
                                .map_err(|error| invalid(error.to_string()))?,
                        })
                    }
                    PresentationChange::KeepSkin { skin, expect_view_rev } => {
                        if *expect_view_rev != base.view {
                            return Err(invalid("The workbook presentation changed. Try on the skin again."));
                        }
                        if skin.level != Level::Workbook || skin.id.trim().is_empty() || skin.name.trim().is_empty() {
                            return Err(invalid("A workbook skin needs an ID and name"));
                        }
                        for page in skin.pages.values() {
                            crate::query::verify_page_bindings(state, page)?;
                        }
                        let body = serde_json::to_string(skin)
                            .map_err(|error| invalid(error.to_string()))?;
                        part.clone().with_skin(Some(calc_doc_model::ViewSkin {
                            id: skin.id.clone(),
                            body,
                        }))
                    }
                    PresentationChange::Trail { enabled } => {
                        let mut trail = part.trail().cloned().unwrap_or_default();
                        if trail.enabled == *enabled {
                            part.clone()
                        } else {
                            trail.enabled = *enabled;
                            part.clone().with_trail(Some(trail))
                        }
                    }
                    PresentationChange::PreferView { view, expect_view_rev } => {
                        if *expect_view_rev != base.view {
                            return Err(invalid("The view preferences have changed. Retry the edit."));
                        }
                        part.clone().with_preferred(view.as_ref().map(|view| view.0.clone()))
                    }
                };
                if next != part {
                    part = next;
                    presentation.push(change.clone());
                }
            }
        }
    }
    if before.view_part().as_ref() == &part {
        presentation.clear();
    } else {
        edits.push(CellEdit::View(Arc::new(part)));
    }
    let mut writes = Vec::new();
    let mut properties = scenario_changes;
    let after = if edits.is_empty() {
        before.clone()
    } else {
        let after = match &cut {
            Some((paste, _, _)) => state.session.preview_skin_paste(paste),
            None => state.session.preview_edits(&edits),
        }.map_err(refusal)?;
        if !ranges_with_metadata.is_empty() {
            properties.extend(range_properties(&before, &after, &ranges_with_metadata, &edits));
        }
        properties.extend(names.into_iter().filter_map(|name| name.diff(&after)));
        properties.extend(axis_properties(&before, &after, &edits));
        if let Some(cache_id) = pivot_cache {
            let affected: BTreeMap<_, _> = [&before, &after].into_iter()
                .flat_map(|root| root.definitions().pivots().tables())
                .filter(|table| table.cache_id == cache_id)
                .map(|table| ((table.area.first(), table.area.last()), table.area))
                .collect();
            properties.extend(affected.into_values().map(|area| ModelPropertyDiff::Scope {
                source: source_area(area),
                property: ModelProperty::PivotCache,
            }));
        }
        for sheet in filter_sheets {
            let old = before.definitions();
            let new = after.definitions();
            if old.auto_filter(sheet) != new.auto_filter(sheet)
                || !old.tables().iter().filter(|table| table.sheet == sheet)
                    .eq(new.tables().iter().filter(|table| table.sheet == sheet))
            {
                properties.push(ModelPropertyDiff::Scope {
                    source: SourceRef::Sheet(Sheet::of(sheet)),
                    property: ModelProperty::AutoFilter,
                });
            }
        }
        if let Some((_, from, to)) = &cut {
            properties.push(ModelPropertyDiff::CellMoved { from: *from, to: *to });
            properties.extend(crate::names::moved(&before, &after, &edits));
            for anchor in after.view_part().anchors() {
                if before.view_part().anchor(anchor.id) == Some(anchor) { continue; }
                if let Some(area) = anchor.area() {
                    let source = if anchor.spill {
                        SourceRef::Spill { anchor: Key::of(area.first()) }
                    } else {
                        source_area(area)
                    };
                    presentation.push(PresentationChange::Bind { anchor: anchor.id, source });
                }
            }
        }
        let mut changed = BTreeSet::new();
        for area in areas {
            for root in [&before, &after] {
                if let Some(cells) = root.cells(area) {
                    changed.extend(cells.map(|(at, _)| at));
                }
            }
        }
        for at in changed {
            let old = before.cell(at);
            let new = after.cell(at);
            if old != new {
                writes.push(ModelWriteDiff {
                    at: Key::of(at),
                    before: old.map(|input| entry_text(Some(input))),
                    after: new.map(|input| entry_text(Some(input))),
                });
            }
        }
        after
    };
    if let Some((sheet, name)) = inserted {
        properties.push(ModelPropertyDiff::WorksheetInserted { sheet, name });
    }
    let preview = Preview {
        base,
        writes,
        properties,
        effects: Vec::new(),
        presentation,
        warnings,
        refusal: None,
    };
    let change = match cut {
        Some((paste, _, _)) => PreparedChange::Paste(paste),
        None => PreparedChange::Edits(edits),
    };
    Ok(Prepared { change, preview, after })
}

fn prepare_entry(session: &HostSession, at: CellKey, text: &str) -> Result<Vec<CellEdit>, Refusal> {
    session.prepare_cell_entry(at, text).map_err(|error| match error {
        IntentError::Validation(failure) => Refusal::Validation {
            target: source_area(GridArea::cell(at)),
            reason: failure.to_string(),
        },
        other => refusal(other),
    })
}

fn prepare_literal(session: &HostSession, at: CellKey, text: &str) -> Result<Vec<CellEdit>, Refusal> {
    literal_target(session, at)?;
    let entries = prepare_entry(session, at, text)?;
    if entries.iter().any(|entry| match entry {
        CellEdit::Clear { .. } => false,
        CellEdit::Set { input, .. } => matches!(input, CellInput::Formula(_)),
        _ => true,
    }) {
        return Err(invalid("A page input accepts literal values"));
    }
    Ok(entries)
}

fn literal_target(session: &HostSession, at: CellKey) -> Result<(), Refusal> {
    rules(session, at)?;
    if matches!(session.host().model().cell(at), Some(CellInput::Formula(_))) {
        return Err(Refusal::Protected(source_area(GridArea::cell(at))));
    }
    Ok(())
}

enum ScenarioCells<'a> {
    Bindings(&'a [Bound]),
    Areas(&'a [Area]),
}

fn scenario_records(state: &State, sheet: Sheet, edits: &[CellEdit]) -> Result<(SheetId, WorksheetScenarios), Refusal> {
    let host = state.session.host();
    let id = crate::query::checked_sheet(host, sheet)?;
    if host.sheet_contents_protected(id) {
        return Err(Refusal::Protected(SourceRef::Sheet(sheet)));
    }
    let records = edits.iter().rev().find_map(|edit| match edit {
        CellEdit::Scenarios { sheet, records } if *sheet == id => Some(records.as_ref()),
        _ => None,
    }).unwrap_or_else(|| host.model().definitions().scenarios(id)).cloned().unwrap_or_default();
    Ok((id, records))
}

fn scenario_name(name: &str) -> Result<String, Refusal> {
    if name.is_empty() || name.encode_utf16().count() > 255 {
        return Err(invalid("Enter a scenario name of 1–255 characters"));
    }
    Ok(calc_doc_model::value::fold_workbook_name(name))
}

fn edit_scenario(state: &State, scenario: &ScenarioRef, change: &ScenarioChange, edits: &mut Vec<CellEdit>) -> Result<Option<ModelPropertyDiff>, Refusal> {
    let (id, mut records) = scenario_records(state, scenario.sheet, edits)?;
    if records.unread != 0 {
        return Err(unavailable("dc-s2xup", "Editing unread Scenario Manager records"));
    }
    let i = records.entries.iter().position(|record| record.name == scenario.name)
        .ok_or_else(|| invalid("The worksheet scenario is unavailable"))?;
    match change {
        ScenarioChange::Rename { to } => {
            if to == &scenario.name {
                return Ok(None);
            }
            let folded = scenario_name(to)?;
            if records.entries.iter().enumerate().any(|(index, known)| {
                index != i && calc_doc_model::value::fold_workbook_name(&known.name) == folded
            }) {
                return Err(invalid("A scenario with that name already exists on the worksheet"));
            }
            records.entries[i].name = to.clone();
        }
        ScenarioChange::Remove => {
            records.entries.remove(i);
        }
    }
    let records = (!records.entries.is_empty()).then_some(records);
    edits.push(CellEdit::Scenarios { sheet: id, records });
    Ok(Some(ModelPropertyDiff::EditedScenario { scenario: scenario.clone(), change: change.clone() }))
}

fn save_scenario(
    state: &State,
    scenario: &ScenarioRef,
    source: ScenarioCells<'_>,
    edits: &mut Vec<CellEdit>,
) -> Result<ModelPropertyDiff, Refusal> {
    let host = state.session.host();
    let (id, mut records) = scenario_records(state, scenario.sheet, edits)?;
    let folded = scenario_name(&scenario.name)?;
    if records.unread != 0 {
        return Err(unavailable("dc-zh096", "Adding a scenario to unread Scenario Manager records"));
    }
    if records.entries.iter().any(|known| calc_doc_model::value::fold_workbook_name(&known.name) == folded) {
        return Err(invalid("A scenario with that name already exists on the worksheet"));
    }
    let mut cells = BTreeSet::new();
    let invalid_count = || invalid("Choose 1–32 changing cells on the worksheet");
    match source {
        ScenarioCells::Bindings(inputs) => {
            for bound in inputs {
                let at = crate::query::bound_cell(state, bound)?;
                if at.sheet() == id {
                    cells.insert(at);
                }
            }
        }
        ScenarioCells::Areas(areas) => {
            for &area in areas {
                let area = grid_area(area)?;
                if area.sheet() != id || area.cell_count() > 32 {
                    return Err(invalid_count());
                }
                let (first, last) = (area.first(), area.last());
                for row in first.row()..=last.row() {
                    for column in first.column()..=last.column() {
                        cells.insert(CellKey::new(id, row, column).expect("inside the selected area"));
                        if cells.len() > 32 {
                            return Err(invalid_count());
                        }
                    }
                }
            }
        }
    }
    if !(1..=32).contains(&cells.len()) {
        return Err(invalid_count());
    }
    let mut changing = Vec::with_capacity(cells.len());
    let mut values = Vec::with_capacity(cells.len());
    for at in cells {
        let current = host.model().cell(at);
        let display = entry_text(current);
        let text = match current {
            Some(CellInput::Text(text)) => {
                let text = String::from_utf16(text.text().utf16_code_units())
                    .map_err(|_| unavailable("dc-zh096", "This scenario’s input text"))?;
                format!("'{text}")
            }
            _ => display.clone(),
        };
        let replay = prepare_literal(&state.session, at, &text)?;
        let same = match replay.as_slice() {
            [CellEdit::Set { input, .. }] => Some(input) == current,
            [CellEdit::Clear { .. }] => current.is_none(),
            _ => false,
        };
        let value: Vec<u16> = text.encode_utf16().collect();
        if value.len() > 255 || !same {
            return Err(unavailable("dc-zh096", "Storing this changing cell exactly in Excel Scenario Manager"));
        }
        let number_format = host.styles().cell_formats.get(host.cell_style(at) as usize)
            .and_then(|format| format.number_format_id).unwrap_or(0);
        changing.push(ScenarioInput {
            at, value: calc_doc_model::value::ExcelText::from_utf16_code_units(value),
            number_format, deleted: false, undone: false,
        });
        values.push((Key::of(at), display));
    }
    records.entries.push(WorksheetScenario {
        source_index: None,
        name: scenario.name.clone(),
        hidden: false,
        locked: false,
        inputs: changing,
    });
    edits.push(CellEdit::Scenarios { sheet: id, records: Some(records) });
    Ok(ModelPropertyDiff::AddedScenario { scenario: scenario.clone(), values })
}

fn encode_document(body: &serde_json::Value) -> Result<String, Refusal> {
    serde_json::to_string(body).map_err(|error| invalid(error.to_string()))
}
