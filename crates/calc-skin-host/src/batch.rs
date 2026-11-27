use crate::{State, cell, grid_area, invalid, unavailable};
use calc_doc_model::ViewAnchor;
use calc_host::value::{CellKey, GridArea};
use calc_host::{CellEdit, CommandError, HostSession, IntentError, search::entry_text};
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
    if calc_host::can_edit(host, area.first()).is_err() {
        return Err(Refusal::Protected(source_area(area)));
    }
    if entry {
        let at = area.first();
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

pub(crate) fn prepare(state: &mut State, request: &Batch) -> Result<Prepared, Refusal> {
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
    let mut presentation = Vec::new();
    for change in &request.changes {
        match change {
            Change::Model(change) => match change {
                ModelChange::Enter { at, text }
                | ModelChange::RewriteFormula {
                    at, new_text: text, ..
                } => {
                    let at = cell(*at)?;
                    rules(&state.session, at)?;
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
                    rules(&state.session, at)?;
                    if matches!(before.cell(at), Some(calc_doc_model::CellInput::Formula(_))) {
                        return Err(Refusal::Protected(source_area(GridArea::cell(at))));
                    }
                    let entries = prepare_entry(&state.session, at, text)?;
                    if entries.iter().any(|entry| match entry {
                        CellEdit::Clear { .. } => false,
                        CellEdit::Set { input, .. } => matches!(input, calc_doc_model::CellInput::Formula(_)),
                        _ => true,
                    }) {
                        return Err(invalid("A page input accepts literal values"));
                    }
                    edits.extend(entries);
                    areas.push(GridArea::cell(at));
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
                        edits.push(CellEdit::Clear { area });
                        areas.push(area);
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
                            _ => return Err(unavailable("dc-aje2v", "This anchor source")),
                        };
                        if binding
                            .area()
                            .is_none_or(|area| before.sheet_by_id(area.sheet()).is_none())
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
                        view,
                        sheet,
                        schema,
                        body,
                    } => {
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
    if !edits.is_empty() {
        let after = state.session.preview_edits(&edits).map_err(refusal)?;
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
    let preview = Preview {
        base,
        writes,
        presentation,
        warnings: Vec::new(),
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

fn encode_document(body: &serde_json::Value) -> Result<String, Refusal> {
    serde_json::to_string(body).map_err(|error| invalid(error.to_string()))
}
