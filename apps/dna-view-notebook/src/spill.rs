use super::*;
use calc_skin::{AddressBook, Batch, Change, ModelChange, Notation, PlacementKind, PlacementShape};

pub(super) fn review_move(state: State, anchor: Key, from: Key, to: Key) -> Result<(), String> {
    let snapshot = state.snapshot()?;
    let expected = state.session.stamp().ok_or("Open a workbook first.")?;
    let mut batch = Batch {
        id: String::new(),
        workbook: Some(snapshot.part.stamp.workbook),
        base: None,
        expects: Vec::new(),
        selection_seq: None,
        label: Some(
            if from == anchor {
                "Move spill lane"
            } else {
                "Move spill obstruction"
            }
            .into(),
        ),
        changes: vec![Change::Model(ModelChange::MoveSpillCell {
            anchor,
            from,
            to,
            expected,
        })],
    };
    super::names::pin_workbook_revision(state.session, &snapshot, &mut batch)?;
    state.session.submit_for_review(batch);
    Ok(())
}

fn destination(session: Session, anchor: Key, text: &str) -> Option<Key> {
    let book = AddressBook {
        sheets: session
            .summary()?
            .sheets
            .into_iter()
            .filter(|sheet| sheet.visible && sheet.worksheet)
            .map(|sheet| (sheet.sheet, sheet.name))
            .collect(),
        active: Some(anchor),
        ..AddressBook::default()
    };
    match SourceRef::parse(text, Notation::A1, &book).ok()? {
        SourceRef::Cells { areas, .. } if matches!(areas.as_slice(), [Area(first,last)] if first == last) => {
            Some(areas[0].0)
        }
        _ => None,
    }
}

pub(super) fn controls(state: State, anchor: Key) -> impl IntoView {
    let session = state.session;
    let blockers = session.following::<query::SpillBlockers>();
    let proposal = session.following::<query::Placement>();
    view! {
        <section class="dna-notebook-tools" aria-label="Blocked spill">
            {move || {
                let answer = match blockers.at(Some(query::SpillBlockers { anchor })) {
                    Live::Ready(answer) => answer,
                    Live::Refused(_) => return view! { <p role="status">"The spill extent is unavailable."</p> }.into_any(),
                    _ => return view! { <p role="status">"Reading the intended spill range…"</p> }.into_any(),
                };
                let Ok(snapshot) = state.snapshot() else {
                    return view! { <p role="status">"Reading Notebook layout…"</p> }.into_any();
                };
                let desired = answer.body.desired;
                let sheet = Sheet(anchor.0, anchor.1);
                let declared = snapshot.chain.cells.iter().find(|item| {
                    snapshot.source(item.id).and_then(first_cell) == Some(anchor)
                }).and_then(|item| item.lane_columns).unwrap_or(1);
                let destination = proposal.at(Some(query::Placement {
                    sheet,
                    kind: PlacementKind::Array,
                    shape: Some(PlacementShape {
                        rows: desired.1.2 - desired.0.2 + 1,
                        columns: declared.max(desired.1.3 - desired.0.3 + 1),
                    }),
                    reserved: snapshot.reserved_lanes(sheet),
                }));
                view! {
                    <p>{format!("Spill wants {} to {}", stage::address(session, Some(desired.0)),
                        stage::address(session, Some(desired.1)))}</p>
                    {answer.body.blockers.into_iter().map(|blocker| match blocker.cell_to_move {
                        Some(from) => obstruction_form(session, anchor, from).into_any(),
                        None => view! { <p>{format!("Blocked at {}", stage::address(session, Some(blocker.area.0)))}</p> }.into_any(),
                    }).collect_view()}
                    {answer.body.more.then(|| view! { <p>"More cells obstruct this spill."</p> })}
                    {if let Live::Ready(answer) = destination {
                        let to = answer.body.at;
                        view! {
                            <button type="button" data-command="notebook.move-lane"
                                on:click=move |_| super::commands::invoke(session, "notebook.move-lane", (anchor, to))>
                                {format!("Review lane move to {}", stage::address(session, Some(to)))}
                            </button>
                        }.into_any()
                    } else { ().into_any() }}
                }.into_any()
            }}
        </section>
    }
}

fn obstruction_form(session: Session, anchor: Key, from: Key) -> impl IntoView {
    let target = NodeRef::<leptos::html::Input>::new();
    view! {
        <form class="dna-notebook-tools" on:submit=move |event| {
            event.prevent_default();
            if let Some(to) = target.get_untracked().and_then(|field| destination(session, anchor, &field.value())) {
                super::commands::invoke(session, "notebook.move-obstruction", (anchor, from, to));
            } else {
                session.refuse(calc_skin::Refusal::Invalid { reason: "Enter one destination cell".into() });
            }
        }>
            <label>{format!("Move {} to", stage::address(session, Some(from)))}
                <input type="text" node_ref=target />
            </label>
            <button type="submit" data-command="notebook.move-obstruction">"Review move"</button>
        </form>
    }
}
