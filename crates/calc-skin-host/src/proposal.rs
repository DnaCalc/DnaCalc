use crate::{State, batch, effects, invalid, query};
use calc_doc_model::ModelRoot;
use calc_host::{SkinEditUnit, search::entry_text, value::CellKey};
use calc_skin::{grid::Key, *};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone)]
pub(crate) struct StagedProposal {
    declaration: Proposal,
    root: Arc<ModelRoot>,
    pub review: ProposalReview,
}

pub(crate) fn offer(
    state: &mut State,
    origin: &Origin,
    policy: WritePolicy,
    mut declaration: Proposal,
) -> Result<Stamp, Refusal> {
    let Origin::Agent { agent, proposal } = origin else {
        return Err(Refusal::PolicyRefused);
    };
    if policy != WritePolicy::Propose {
        return Err(Refusal::PolicyRefused);
    }
    if state.read_only {
        return Err(Refusal::ReadOnly);
    }
    let id = ProposalId(proposal.clone());
    if id.0.trim().is_empty() || declaration.intent.trim().is_empty() {
        return Err(invalid(
            "Give the proposal an identity and explain its intent",
        ));
    }
    order_units(&mut declaration.units)?;
    for unit in &mut declaration.units {
        let request = &mut unit.batch;
        if request.changes.is_empty()
            || request
                .changes
                .iter()
                .any(|change| !matches!(change, Change::Model(_)))
        {
            return Err(invalid("Each proposal unit must declare a model change"));
        }
        if request
            .workbook
            .is_some_and(|workbook| workbook != declaration.at.workbook)
            || request
                .base
                .is_some_and(|base| base != declaration.at.model)
        {
            return Err(invalid(
                "Each unit must use the proposal's workbook and model revision",
            ));
        }
        request.workbook = Some(declaration.at.workbook);
        request.base = Some(declaration.at.model);
    }
    if let Some(staged) = state.proposals.get(&id) {
        if staged.review.agent == *agent && staged.declaration == declaration {
            return Ok(state.stamp());
        }
        return Err(invalid("This proposal identity is already in use"));
    }
    let current = state.stamp();
    if (
        declaration.at.workbook,
        declaration.at.model,
        declaration.at.view,
    ) != (current.workbook, current.model, current.view)
    {
        return Err(invalid("The workbook changed. Submit the proposal again."));
    }
    let root = state.session.host().model().clone();
    let units = prepare_units(state, declaration.units.iter())?;
    let review = ProposalReview {
        id: id.clone(),
        at: current,
        agent: agent.clone(),
        intent: declaration.intent.clone(),
        units: declaration
            .units
            .iter()
            .zip(units)
            .map(|(unit, staged)| ProposalUnitReview {
                id: unit.id.clone(),
                needs: unit.needs.clone(),
                label: unit
                    .batch
                    .label
                    .clone()
                    .unwrap_or_else(|| unit.id.0.clone()),
                status: ProposalStatus::Pending,
                writes: staged.preview.writes,
                properties: staged.preview.properties,
            })
            .collect(),
    };
    state.proposals.insert(
        id,
        StagedProposal {
            declaration,
            root,
            review,
        },
    );
    state.proposal_revision += 1;
    Ok(current)
}

fn order_units(units: &mut Vec<ProposalUnit>) -> Result<(), Refusal> {
    if units.is_empty() || units.len() > calc_host::UNDO_LIMIT {
        return Err(invalid(
            "Choose a number of units that fits the workbook's undo history",
        ));
    }
    let unique: BTreeSet<_> = units.iter().map(|unit| &unit.id).collect();
    if unique.len() != units.len() || unique.iter().any(|id| id.0.trim().is_empty()) {
        return Err(invalid("Assign every proposal unit a distinct identity"));
    }
    let mut ordered = Vec::new();
    let mut ready = BTreeSet::new();
    while !units.is_empty() {
        let next = units
            .iter()
            .position(|unit| unit.needs.iter().all(|need| ready.contains(need)))
            .ok_or_else(|| invalid("A proposal dependency is missing or circular"))?;
        let unit = units.remove(next);
        ready.insert(unit.id.clone());
        ordered.push(unit);
    }
    *units = ordered;
    Ok(())
}

fn selected<'a>(
    staged: &'a StagedProposal,
    accept: &[UnitId],
    reject: &[UnitId],
) -> Result<Vec<&'a ProposalUnit>, Refusal> {
    let wanted: BTreeSet<_> = accept.iter().collect();
    let all: BTreeSet<_> = accept.iter().chain(reject).collect();
    if all.len() != accept.len() + reject.len() {
        return Err(invalid("Choose each proposal unit once"));
    }
    let known = |id: &UnitId| staged.review.units.iter().find(|unit| unit.id == *id);
    for id in all {
        if known(id).is_none_or(|unit| unit.status != ProposalStatus::Pending) {
            return Err(invalid(format!("Unit {} is unavailable for review", id.0)));
        }
    }
    let units: Vec<_> = staged
        .declaration
        .units
        .iter()
        .filter(|unit| wanted.contains(&unit.id))
        .collect();
    for unit in &units {
        if let Some(need) = unit.needs.iter().find(|need| {
            !wanted.contains(*need)
                && known(need).is_none_or(|review| review.status != ProposalStatus::Accepted)
        }) {
            return Err(invalid(format!("Unit {} needs {}", unit.id.0, need.0)));
        }
    }
    Ok(units)
}

fn prepare_units<'a>(
    state: &mut State,
    units: impl IntoIterator<Item = &'a ProposalUnit>,
) -> Result<Vec<batch::Prepared>, Refusal> {
    let mut prepared: Vec<batch::Prepared> = Vec::new();
    for unit in units {
        let next = if let Some(previous) = prepared.last() {
            let owner =
                crate::SkinHost::new(state.session.for_staged_edits(previous.after.clone()));
            let mut staging = owner.state.borrow_mut();
            staging.selection_seq = state.selection_seq;
            staging.selection = state.selection.clone();
            staging.opened_at = state.opened_at;
            let mut request = unit.batch.clone();
            request.workbook = Some(staging.stamp().workbook);
            request.base = Some(staging.stamp().model);
            batch::prepare(&mut staging, &request, None)?
        } else {
            batch::prepare(state, &unit.batch, None)?
        };
        if next.preview.kind() != Some(ChangeKind::Model) {
            return Err(invalid(format!(
                "Unit {} needs a reversible model change",
                unit.id.0
            )));
        }
        prepared.push(next);
    }
    if prepared.len() > 1
        && prepared
            .iter()
            .any(|unit| matches!(unit.change, batch::PreparedChange::Paste(_)))
    {
        return Err(crate::unavailable(
            "dc-zh096",
            "A coordinate move in a multi-unit proposal",
        ));
    }
    Ok(prepared)
}

pub(crate) fn preview(
    state: &mut State,
    id: &ProposalId,
    units: &[UnitId],
    depth: PreviewDepth,
) -> Result<Preview, Refusal> {
    let staged = proposal(state, id)?;
    let selected = selected(&staged, units, &[])?;
    let prepared = checked_units(state, &staged, &selected)?;
    let base = state.stamp();
    let before = state.session.host().model();
    let Some(last) = prepared.last() else {
        return Ok(empty_preview(base));
    };
    let mut preview = empty_preview(base);
    let keys: BTreeSet<_> = prepared
        .iter()
        .flat_map(|unit| unit.preview.writes.iter().map(|write| write.at))
        .collect();
    preview.writes = write_diffs(before, &last.after, keys.into_iter().filter_map(Key::cell));
    for unit in &prepared {
        preview
            .properties
            .extend(unit.preview.properties.iter().cloned());
        preview
            .warnings
            .extend(unit.preview.warnings.iter().cloned());
    }
    if let PreviewDepth::WithEffects { budget } = depth {
        effects::append(
            state.session.host(),
            &mut preview,
            last.after.clone(),
            budget,
        );
    }
    Ok(preview)
}

fn empty_preview(base: Stamp) -> Preview {
    Preview {
        base,
        writes: Vec::new(),
        properties: Vec::new(),
        effects: Vec::new(),
        presentation: Vec::new(),
        warnings: Vec::new(),
        refusal: None,
    }
}

fn proposal(state: &State, id: &ProposalId) -> Result<StagedProposal, Refusal> {
    state
        .proposals
        .get(id)
        .cloned()
        .ok_or_else(|| invalid("The proposal is unavailable in this workbook"))
}

fn checked_units(
    state: &mut State,
    proposal: &StagedProposal,
    units: &[&ProposalUnit],
) -> Result<Vec<batch::Prepared>, Refusal> {
    let mut conflicts = Vec::new();
    for unit in units {
        if let Err(error) = state.check_base(&unit.batch) {
            match error {
                Refusal::Conflict {
                    conflicts: affected,
                    ..
                } => {
                    for target in affected {
                        if !conflicts.contains(&target) {
                            conflicts.push(target);
                        }
                    }
                }
                other => return Err(other),
            }
        }
    }
    if !conflicts.is_empty() {
        return Err(conflict(state, proposal, units, conflicts));
    }
    prepare_units(state, units.iter().copied()).map_err(|error| match error {
        Refusal::Conflict { conflicts, .. } => conflict(state, proposal, units, conflicts),
        other => other,
    })
}

fn conflict(
    state: &State,
    proposal: &StagedProposal,
    units: &[&ProposalUnit],
    conflicts: Vec<Target>,
) -> Refusal {
    let now = state.session.host().model();
    let mut cells = BTreeSet::new();
    let mut unresolved = false;
    for target in &conflicts {
        let areas = match target {
            Target::Source(SourceRef::Workbook) => None,
            Target::Source(source) => query::source_areas(state, source).ok(),
            _ => None,
        };
        if let Some(areas) = areas {
            for area in areas {
                for root in [&proposal.root, now] {
                    cells.extend(root.cells(area).into_iter().flatten().map(|(at, _)| at));
                }
            }
        } else {
            unresolved = true;
        }
    }
    if unresolved {
        for review in &proposal.review.units {
            if units.iter().any(|unit| unit.id == review.id) {
                cells.extend(review.writes.iter().filter_map(|write| write.at.cell()));
            }
        }
    }
    let diffs = write_diffs(&proposal.root, now, cells);
    Refusal::Conflict {
        base: proposal.review.at.model,
        conflicts,
        diffs,
    }
}

fn write_diffs(
    before: &ModelRoot,
    after: &ModelRoot,
    cells: impl IntoIterator<Item = CellKey>,
) -> Vec<ModelWriteDiff> {
    cells
        .into_iter()
        .filter_map(|at| {
            let old = before.cell(at);
            let new = after.cell(at);
            (old != new).then(|| ModelWriteDiff {
                at: Key::of(at),
                before: old.map(|input| entry_text(Some(input))),
                after: new.map(|input| entry_text(Some(input))),
            })
        })
        .collect()
}

pub(crate) fn decide(
    state: &mut State,
    id: &ProposalId,
    accept: &[UnitId],
    reject: &[UnitId],
) -> Result<Stamp, Refusal> {
    let staged = proposal(state, id)?;
    let selected = selected(&staged, accept, reject)?;
    let prepared = checked_units(state, &staged, &selected)?;
    let mut events = Vec::new();
    if trail_enabled(state) {
        use calc_doc_model::ViewTrailOutcome::{Accepted, Rejected};
        for (chosen, outcome) in [(accept, Accepted), (reject, Rejected)] {
            for unit in &staged.review.units {
                if chosen.contains(&unit.id) {
                    events.push(trail_event(state, &staged, unit, outcome));
                }
            }
        }
    }
    if prepared.is_empty() {
        state.session.record_trail(events).map_err(batch::refusal)?;
    } else {
        apply(state, &staged, &selected, prepared, events)?;
    }
    let proposal = state
        .proposals
        .get_mut(id)
        .expect("the workbook still holds its proposal");
    for unit in &mut proposal.review.units {
        if accept.contains(&unit.id) {
            unit.status = ProposalStatus::Accepted;
        } else if reject.contains(&unit.id) {
            unit.status = ProposalStatus::Rejected;
        }
    }
    if !accept.is_empty() || !reject.is_empty() {
        state.proposal_revision += 1;
    }
    if &state.selection != state.session.selection() {
        state.update_selection(SelectionCause::History);
    }
    Ok(state.stamp())
}

fn apply(
    state: &mut State,
    staged: &StagedProposal,
    units: &[&ProposalUnit],
    prepared: Vec<batch::Prepared>,
    events: Vec<calc_doc_model::ViewTrailEvent>,
) -> Result<(), Refusal> {
    let before = state.stamp();
    let mut ordinary = Vec::new();
    let mut paste = None;
    let mut scope_changed = false;
    for (unit, prepared) in units.iter().zip(prepared) {
        scope_changed |= !prepared.preview.properties.is_empty();
        let mut targets: Vec<_> = unit
            .batch
            .expects
            .iter()
            .cloned()
            .map(Target::Source)
            .collect();
        for write in &prepared.preview.writes {
            let target = Target::Source(dna_cell_source(write.at));
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
        let history = HistoryEntry {
            id: EntryId(0),
            at: before,
            label: unit
                .batch
                .label
                .clone()
                .unwrap_or_else(|| unit.id.0.clone()),
            origin: Origin::Agent {
                agent: staged.review.agent.clone(),
                proposal: staged.review.id.0.clone(),
            },
            kind: ChangeKind::Model,
            targets,
            cells_touched: prepared.preview.writes.len() as u64,
            reversibility: Reversibility::Reversible,
            group: Some(staged.review.id.0.clone()),
            proposal_unit: Some(unit.id.clone()),
            unreviewed: false,
        };
        match prepared.change {
            batch::PreparedChange::Edits(edits) => ordinary.push(SkinEditUnit { edits, history }),
            batch::PreparedChange::Paste(plan) => paste = Some((plan, history)),
        }
    }
    let (receipt, events) = if let Some((plan, entry)) = paste {
        (state.session.dispatch_skin_paste(plan, entry), events)
    } else {
        (
            state.session.dispatch_skin_group(ordinary, events),
            Vec::new(),
        )
    };
    if let Some(error) = receipt.error {
        return Err(batch::refusal(error));
    }
    let Some(commit) = receipt.committed else {
        return Err(invalid("The proposal did not commit a model change"));
    };
    state.session.record_trail(events).map_err(batch::refusal)?;
    let touched = (!scope_changed).then(|| commit.input_writes().to_vec());
    state.record_touch(before, touched, commit.pending().to_vec());
    state.record_calculation(before, receipt.recalc.as_ref());
    Ok(())
}

fn dna_cell_source(at: Key) -> SourceRef {
    batch::source_area(calc_host::value::GridArea::cell(
        at.cell().expect("a written cell"),
    ))
}

fn trail_enabled(state: &State) -> bool {
    let part = state.session.host().model().view_part();
    !part.is_opaque() && part.trail().is_none_or(|trail| trail.enabled)
}

fn trail_event(
    state: &State,
    staged: &StagedProposal,
    unit: &ProposalUnitReview,
    outcome: calc_doc_model::ViewTrailOutcome,
) -> calc_doc_model::ViewTrailEvent {
    let names = query::address_book(state);
    let mut targets: BTreeSet<String> = unit
        .writes
        .iter()
        .filter_map(|write| write.at.cell())
        .map(|at| calc_skin::address::cell(&names, at))
        .collect();
    for property in &unit.properties {
        match property {
            ModelPropertyDiff::DefinedName { after, before, .. } => {
                if let Some(name) = after.as_ref().or(before.as_ref()) {
                    targets.insert(name.name.clone());
                }
            }
            ModelPropertyDiff::AddedScenario { scenario, .. }
            | ModelPropertyDiff::EditedScenario { scenario, .. } => {
                targets.insert(scenario.name.clone());
            }
            ModelPropertyDiff::WorksheetInserted { name, .. } => {
                targets.insert(name.clone());
            }
            ModelPropertyDiff::CellMoved { from, to } => {
                targets.extend(
                    [*from, *to]
                        .into_iter()
                        .filter_map(Key::cell)
                        .map(|at| calc_skin::address::cell(&names, at)),
                );
            }
            ModelPropertyDiff::CellFormat { at, .. } => {
                if let Some(at) = at.cell() {
                    targets.insert(calc_skin::address::cell(&names, at));
                }
            }
            ModelPropertyDiff::Scope { source, .. } => {
                if let Ok(areas) = query::source_areas(state, source) {
                    targets.extend(
                        areas
                            .into_iter()
                            .map(|area| calc_skin::address::area(&names, area)),
                    );
                }
            }
        }
    }
    calc_doc_model::ViewTrailEvent {
        timestamp_millis: timestamp_millis(),
        agent: staged.review.agent.clone(),
        proposal: staged.review.id.0.clone(),
        unit: unit.id.0.clone(),
        intent: staged.review.intent.clone(),
        targets: targets.into_iter().collect(),
        outcome,
    }
}

fn timestamp_millis() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| {
                u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
            })
    }
}

pub(crate) fn history_trail_event(
    state: &State,
    redo: bool,
) -> Option<calc_doc_model::ViewTrailEvent> {
    if !trail_enabled(state) {
        return None;
    }
    let entry = state.session.skin_entry_for_restore(redo)?;
    let Origin::Agent { proposal, .. } = &entry.origin else {
        return None;
    };
    let id = entry.proposal_unit.as_ref()?;
    let staged = state.proposals.get(&ProposalId(proposal.clone()))?;
    let unit = staged.review.units.iter().find(|unit| unit.id == *id)?;
    let outcome = if redo {
        calc_doc_model::ViewTrailOutcome::Accepted
    } else {
        calc_doc_model::ViewTrailOutcome::Undone
    };
    Some(trail_event(state, staged, unit, outcome))
}
