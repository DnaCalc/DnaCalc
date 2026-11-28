use crate::{State, invalid, inventory::name_scope, query::QueryWork, survey};
use calc_host::value::{CellKey, WorksheetErrorCode};
use calc_host::{NameResultKind, ProjectedValue, ProjectionRequest, ResultStamp};
use calc_skin::grid::Key;
use calc_skin::{
    ErrorIssue, ErrorOrigin, ErrorRoot, Issues, NamedErrorEvidence, Refusal, SourceRef,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

struct Error<'a> {
    code: WorksheetErrorCode,
    literal: bool,
    certificate: Option<&'a ResultStamp>,
}

pub(crate) fn issues(
    state: &State,
    scope: &SourceRef,
    meter: &mut QueryWork,
) -> Result<Issues, Refusal> {
    let host = state.session.host();
    let by_sheet = survey::scoped_cells(state, scope, meter)?;
    let cells = by_sheet.values().flatten().copied().collect();
    let page = calc_host::project(host, &ProjectionRequest { cells })
        .map_err(|error| invalid(error.to_string()))?;
    let mut errors = BTreeMap::new();
    for item in page.cells() {
        meter.work(0, 0, 0)?;
        let Some(code) = survey::current_error(item.value()) else {
            continue;
        };
        meter.work(0, 1, 0)?;
        let (literal, certificate) = match item.value() {
            ProjectedValue::Literal(_) => (true, None),
            ProjectedValue::Calculated { calculation, .. } => (
                false,
                calculation.stamp().filter(|stamp| {
                    !stamp.reads.uses_boundary_cache
                        && !stamp.reads.uses_imported_cache
                        && !stamp.reads.legacy_boundary
                }),
            ),
            _ => unreachable!("a current error is a literal or a calculated value"),
        };
        errors.insert(
            item.at(),
            Error {
                code,
                literal,
                certificate,
            },
        );
    }

    let mut pending = BTreeMap::new();
    let mut children: BTreeMap<CellKey, Vec<CellKey>> = BTreeMap::new();
    let mut queue = VecDeque::new();
    let mut roots_by_cell: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
    let mut direct_sources = BTreeMap::new();
    let mut exact_roots = BTreeMap::new();
    for (&at, error) in &errors {
        let mut inputs = BTreeSet::new();
        if let Some(certificate) = error.certificate {
            for read in &certificate.reads.dependencies {
                meter.work(0, 0, 0)?;
                let region = read.region;
                for (&candidate, input) in errors.range(region.first()..=region.last()) {
                    meter.work(0, 0, 0)?;
                    if region.contains(candidate)
                        && input.code == error.code
                        && inputs.insert(candidate)
                    {
                        meter.work(0, 0, 1)?;
                        children.entry(candidate).or_default().push(at);
                    }
                }
            }
            if inputs.len() == 1
                && let Some(source) = host.direct_reference(at)
                && inputs.contains(&source)
            {
                direct_sources.insert(at, source);
            }
        }
        if inputs.is_empty() && (error.literal || error.certificate.is_some()) {
            roots_by_cell.insert(at, BTreeSet::from([at]));
            queue.push_back(at);
            if error.literal
                || error.certificate.is_some_and(|proof| {
                    proof.reads.dependencies.is_empty()
                        && proof.reads.name_invocations.is_empty()
                        && proof.reads.omitted_name_invocations == 0
                        && proof.reads.external_inputs.is_empty()
                        && proof.reads.linked_inputs.is_none()
                        && proof.reads.invocations.is_empty()
                })
            {
                exact_roots.insert(at, at);
            }
        }
        pending.insert(at, inputs.len());
    }

    let mut affected = BTreeMap::new();
    let mut origins = BTreeMap::new();
    while let Some(at) = queue.pop_front() {
        meter.work(0, 0, 0)?;
        let roots = roots_by_cell.remove(&at).unwrap_or_default();
        let origin = if roots.contains(&at) {
            affected.entry(at).or_insert(0);
            if errors[&at].literal {
                ErrorOrigin::Literal
            } else {
                ErrorOrigin::RootCandidate
            }
        } else {
            if let Some(root) = direct_sources
                .get(&at)
                .and_then(|source| exact_roots.get(source))
                .copied()
            {
                exact_roots.insert(at, root);
            }
            for root in &roots {
                *affected.entry(*root).or_insert(0) += 1;
            }
            ErrorOrigin::CandidatePropagation {
                roots: roots.iter().copied().map(Key::of).collect(),
            }
        };
        origins.insert(at, origin);
        for child in children.get(&at).into_iter().flatten() {
            let remaining = pending.get_mut(child).expect("child is an error in scope");
            let parent_roots = roots_by_cell.entry(*child).or_default();
            for &root in &roots {
                if parent_roots.insert(root) {
                    meter.work(0, 0, 1)?;
                }
            }
            *remaining -= 1;
            if *remaining == 0 {
                queue.push_back(*child);
            }
        }
    }

    let errors = errors
        .into_iter()
        .map(|(at, error)| {
            let mut names = BTreeSet::new();
            let mut named_errors = Vec::new();
            if let Some(certificate) = error.certificate {
                for invocation in &certificate.reads.name_invocations {
                    meter.work(0, 0, 0)?;
                    if invocation.result == NameResultKind::Error(error.code)
                        && names.insert(invocation.definition.id)
                        && let Some(node) = host.name_node(invocation.definition.id)
                        && node.binding_version() == invocation.definition.version
                    {
                        meter.work(0, 0, 1)?;
                        let definition = node.definition();
                        named_errors.push(NamedErrorEvidence {
                            name: definition.name.clone(),
                            scope: name_scope(definition.scope),
                            formula: definition.formula.clone(),
                        });
                    }
                }
            }
            Ok(ErrorIssue {
                at: Key::of(at),
                code: error.code.excel_spelling().into(),
                origin: origins.remove(&at).unwrap_or(ErrorOrigin::Unresolved),
                exact_root: exact_roots.get(&at).copied().map(Key::of),
                named_errors,
            })
        })
        .collect::<Result<_, Refusal>>()?;
    Ok(Issues {
        errors,
        roots: affected
            .into_iter()
            .map(|(at, affected)| ErrorRoot {
                at: Key::of(at),
                affected,
            })
            .collect(),
    })
}
