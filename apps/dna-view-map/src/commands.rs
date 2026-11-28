use super::*;

use calc_skin::{Batch, Change, ModelChange, OperandSpan, Stamp};
use dna_kit::{Invocation, Seam};
use serde::de::DeserializeOwned;

#[derive(Serialize, Deserialize)]
pub(super) struct Rewire {
    pub at: Key,
    pub expected_old: String,
    pub occurrence: OperandSpan,
    pub with: String,
    pub stamp: Stamp,
}

fn args<T: DeserializeOwned>(invocation: &Invocation) -> Result<T, String> {
    serde_json::from_value(invocation.args.clone()).map_err(|_| "Choose a map item first.".into())
}

fn depth(session: Session, state: MapState) -> Result<Scope, String> {
    let active = session.active().ok_or("Select a worksheet cell first.")?;
    Ok(match state.scope(session) {
        Scope::Book => Scope::Sheet(Sheet(active.0, active.1)),
        Scope::Sheet(_) => match session.selection().map(|selection| selection.target) {
            Some(SourceRef::Cells { areas, .. })
                if areas.len() == 1 && areas[0].0 != areas[0].1 =>
            {
                Scope::Block(areas[0])
            }
            _ => Scope::Excerpt(excerpt_at(active)),
        },
        Scope::Block(area) => Scope::Excerpt(excerpt_at(if contains(area, active) {
            active
        } else {
            area.0
        })),
        scope @ Scope::Excerpt(_) => scope,
    })
}

pub(super) fn excerpt_at(at: Key) -> Area {
    let Key(slot, generation, row, column) = at;
    let top = row.saturating_sub(3).clamp(1, 1_048_570);
    let left = column.saturating_sub(5).clamp(1, 16_373);
    Area(
        Key(slot, generation, top, left),
        Key(slot, generation, top + 6, left + 11),
    )
}

pub(super) fn back(session: Session, state: MapState) -> bool {
    if session.view().0 != ID {
        return false;
    }
    if state.rewire.get_untracked().is_some() {
        state.rewire.set(None);
        return true;
    }
    let scope = match state.scope(session) {
        Scope::Book => return false,
        Scope::Sheet(_) => Scope::Book,
        Scope::Block(_) | Scope::Excerpt(_) => Scope::Sheet(state.scope(session).sheet().unwrap()),
    };
    state.set_scope(session, scope);
    true
}

pub(super) fn install(session: Session, state: MapState) -> Vec<ViewCommand> {
    let id = ViewId(ID.into());
    vec![
        ViewCommand::new(&id, "map.open", "Open map scope", &[], move |invocation| {
            let scope: Scope = args(invocation)?;
            if let Some(sheet) = scope.sheet() {
                let entry = session
                    .summary()
                    .and_then(|summary| {
                        summary
                            .sheets
                            .into_iter()
                            .find(|entry| entry.sheet == sheet && entry.worksheet)
                    })
                    .ok_or("The worksheet is unavailable.")?;
                match scope {
                    Scope::Sheet(sheet)
                        if entry.visible
                            && session.active().is_none_or(|at| Sheet(at.0, at.1) != sheet) =>
                    {
                        session.navigate(Target::Source(SourceRef::Sheet(sheet)))
                    }
                    Scope::Block(area) if entry.visible => {
                        if area.area().is_none() {
                            return Err("The block is outside the worksheet.".into());
                        }
                        session.navigate(Target::Source(area_source(area)));
                    }
                    Scope::Excerpt(area) if !bounded_excerpt(area) => {
                        return Err("A sheet excerpt can show up to 7 rows and 12 columns.".into());
                    }
                    _ => {}
                }
            }
            state.set_scope(session, scope);
            Ok(())
        }),
        ViewCommand::new(
            &id,
            "map.select",
            "Select worksheet cell",
            &[],
            move |invocation| {
                session.select_cell(args(invocation)?);
                Ok(())
            },
        ),
        ViewCommand::new(
            &id,
            "map.drill",
            "Drill into map",
            &["Enter", "+"],
            move |_| {
                state.set_scope(session, depth(session, state)?);
                Ok(())
            },
        )
        .when("mode:ready"),
        ViewCommand::new(&id, "map.back", "Zoom out of map", &["-"], move |_| {
            back(session, state);
            Ok(())
        })
        .when("mode:ready"),
        ViewCommand::new(
            &id,
            "map.expand-all",
            "Expand all formulas",
            &[],
            move |_| {
                state.expanded.set(true);
                Ok(())
            },
        ),
        ViewCommand::new(
            &id,
            "map.inventory",
            "Show sheet inventory",
            &[],
            move |_| {
                state.inventory.update(|visible| *visible = !*visible);
                Ok(())
            },
        ),
        ViewCommand::new(
            &id,
            "map.connections",
            "Show connections list",
            &[],
            move |_| {
                state.listing.update(|visible| *visible = !*visible);
                Ok(())
            },
        ),
        ViewCommand::new(
            &id,
            "map.companion",
            "Classic companion",
            &["Ctrl+Alt+0", "Alt+Shift+0"],
            move |_| {
                state.companion.update(|visible| *visible = !*visible);
                Ok(())
            },
        ),
        ViewCommand::new(
            &id,
            "map.node-page",
            "Page through map items",
            &[],
            move |invocation| {
                state.node_page.set(args(invocation)?);
                Ok(())
            },
        ),
        ViewCommand::new(
            &id,
            "map.wire-page",
            "Page through connections",
            &[],
            move |invocation| {
                state.wire_page.set(args(invocation)?);
                Ok(())
            },
        ),
        ViewCommand::new(
            &id,
            "map.rewire",
            "Choose formula operand",
            &["W"],
            move |_| {
                let stamp = session.stamp().ok_or("Open a workbook first.")?;
                let at = session.active().ok_or("Select a formula cell first.")?;
                let single = session.selection().is_some_and(|selection| {
                    matches!(selection.target, SourceRef::Cells { areas, .. } if areas.as_slice() == [Area(at, at)])
                });
                if !single {
                    return Err("Select one formula cell to rewire.".into());
                }
                if session.active_entry().is_none_or(|entry| !entry.formula) {
                    return Err("Select one formula cell to rewire.".into());
                }
                state.rewire.update(|current| {
                    let next = Some((stamp.workbook, at));
                    *current = if *current == next { None } else { next };
                });
                Ok(())
            },
        )
        .when("mode:ready")
        .when("writable"),
        ViewCommand::new(
            &id,
            "map.replace",
            "Preview port rewire",
            &[],
            move |invocation| {
                let edit: Rewire = args(invocation)?;
                if !matches!(session.seam(), Seam::Idle) {
                    return Err("Finish the change in progress first.".into());
                }
                if session.active() != Some(edit.at) || !state.editing_wire(session, edit.at) {
                    return Err("Select the formula and choose a reference first.".into());
                }
                session.submit_for_review(Batch {
                    id: String::new(),
                    workbook: Some(edit.stamp.workbook),
                    base: Some(edit.stamp.model),
                    expects: vec![cell_source(edit.at)],
                    selection_seq: None,
                    label: Some("Rewire one formula reference".into()),
                    changes: vec![Change::Model(ModelChange::ReplaceReference {
                        at: edit.at,
                        expected_old: edit.expected_old,
                        occurrence: edit.occurrence,
                        with: edit.with,
                    })],
                });
                Ok(())
            },
        )
        .when("mode:ready")
        .when("writable"),
    ]
}

fn bounded_excerpt(area: Area) -> bool {
    area.area()
        .is_some_and(|area| area.row_count() <= 7 && area.column_count() <= 12)
}
