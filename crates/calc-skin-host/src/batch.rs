use crate::{CopiedCells, State, cell, grid_area, invalid, unavailable};
use calc_doc_model::{CellInput, ModelRoot, ScenarioInput, ViewAnchor, WorksheetScenario};
use calc_host::value::{CellKey, GridArea};
use calc_host::{
    Axis, CellEdit, CommandError, HostSession, IntentError, PasteMode, PasteSource,
    prepare_column_visibility, prepare_paste_oriented,
    search::entry_text,
};
use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::*;
use std::{collections::BTreeSet, sync::Arc};

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
    pub edits: Vec<CellEdit>,
    pub preview: Preview,
}

fn paste_properties(
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

fn column_properties(
    before: &ModelRoot,
    after: &ModelRoot,
    edits: &[CellEdit],
) -> Vec<ModelPropertyDiff> {
    let before = before.definitions();
    let after = after.definitions();
    let sheets: BTreeSet<_> = edits
        .iter()
        .filter_map(|edit| {
            let CellEdit::AxisFormat(edit) = edit else {
                return None;
            };
            let sheet = edit.sheet();
            let hidden = |root: &calc_doc_model::Definitions| {
                root.axis_layouts().get(sheet, Axis::Columns, edit.index()).hidden()
            };
            (edit.axis() == Axis::Columns && hidden(before) != hidden(after)).then_some(sheet)
        })
        .collect();
    let mut properties = Vec::new();
    for sheet in sheets {
        let source = SourceRef::Sheet(Sheet::of(sheet));
        for (property, changed) in [
            (ModelProperty::ColumnVisibility, true),
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
    let mut paste_areas = Vec::new();
    let mut inserted = None;
    let mut presentation = Vec::new();
    let mut warnings = Vec::new();
    let mut names = Vec::new();
    let mut scenario_changes = Vec::new();
    let removes_anchor = request.changes.iter().any(|change| {
        matches!(change, Change::Presentation(PresentationChange::RemoveAnchor { .. }))
    });
    for change in &request.changes {
        match change {
            Change::Model(change) => match change {
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
                    let change = save_scenario(state, scenario, inputs, &mut edits)?;
                    scenario_changes.push(change);
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
                    let (mode, transpose) = match contents {
                        PasteContents::Values => (PasteMode::Values, false),
                        PasteContents::Formulas => (PasteMode::Formulas, false),
                        PasteContents::AllTransposed => (PasteMode::All, true),
                    };
                    let host = state.session.host();
                    let excel = |reason| Refusal::Excel { reason };
                    let plan = prepare_paste_oriented(
                        host,
                        PasteSource::Snapshot {
                            snapshot: &copied.snapshot,
                            cut: false,
                        },
                        grid_area(*to)?,
                        mode,
                        transpose,
                    )
                    .map_err(excel)?;
                    let target = plan.target();
                    areas.push(target);
                    if !paste_areas.contains(&target) {
                        paste_areas.push(target);
                    }
                    edits.extend(plan.into_edits(host).map_err(excel)?);
                }
                ModelChange::SetColumnsHidden { sheet, bands, hidden } => {
                    require_current_model(request, base.model)?;
                    edits.extend(
                        prepare_column_visibility(state.session.host(), sheet.id(), bands, *hidden)
                            .map_err(|reason| Refusal::Excel { reason })?,
                    );
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
    if !edits.is_empty() {
        let after = state.session.preview_edits(&edits).map_err(refusal)?;
        if !paste_areas.is_empty() {
            properties.extend(paste_properties(&before, &after, &paste_areas, &edits));
        }
        properties.extend(names.into_iter().filter_map(|name| name.diff(&after)));
        properties.extend(column_properties(&before, &after, &edits));
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
    }
    if let Some((sheet, name)) = inserted {
        properties.push(ModelPropertyDiff::WorksheetInserted { sheet, name });
    }
    let preview = Preview {
        base,
        writes,
        properties,
        presentation,
        warnings,
        refusal: None,
    };
    Ok(Prepared { edits, preview })
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
    rules(session, at)?;
    if matches!(session.host().model().cell(at), Some(CellInput::Formula(_))) {
        return Err(Refusal::Protected(source_area(GridArea::cell(at))));
    }
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

fn save_scenario(
    state: &State,
    scenario: &ScenarioRef,
    inputs: &[Bound],
    edits: &mut Vec<CellEdit>,
) -> Result<ModelPropertyDiff, Refusal> {
    let host = state.session.host();
    let id = crate::query::checked_sheet(host, scenario.sheet)?;
    if host.sheet_contents_protected(id) {
        return Err(Refusal::Protected(SourceRef::Sheet(scenario.sheet)));
    }
    if scenario.name.is_empty() || scenario.name.encode_utf16().count() > 255 {
        return Err(invalid("Enter a scenario name of 1–255 characters"));
    }
    let mut records = edits.iter().rev().find_map(|edit| match edit {
        CellEdit::Scenarios { sheet, records } if *sheet == id => records.as_ref(),
        _ => None,
    }).or_else(|| host.model().definitions().scenarios(id)).cloned().unwrap_or_default();
    if records.unread != 0 {
        return Err(unavailable("dc-zh096", "Adding a scenario to unread Scenario Manager records"));
    }
    let folded = calc_doc_model::value::fold_workbook_name(&scenario.name);
    if records.entries.iter().any(|known| calc_doc_model::value::fold_workbook_name(&known.name) == folded) {
        return Err(invalid("A scenario with that name already exists on the worksheet"));
    }
    let mut cells = BTreeSet::new();
    for bound in inputs {
        let at = crate::query::bound_cell(state, bound)?;
        if at.sheet() == id {
            cells.insert(at);
        }
    }
    if !(1..=32).contains(&cells.len()) {
        return Err(invalid("Choose a worksheet with 1–32 changing cells on this App page"));
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
