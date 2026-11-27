use crate::{State, cell_source, coverage, result, status};
use calc_skin::grid::Key;
use calc_skin::{
    AnchorId, Batch, Change, Interest, NoteId, PresentationChange, Provenance, Stamp, query,
};
use dna_kit::{Session, ViewCommand};
use leptos::prelude::*;
use std::num::NonZeroU128;

pub(crate) fn commands(session: Session) -> Vec<ViewCommand> {
    let state = session.local::<State>();
    vec![
        ViewCommand::global("inspector.details", "Cell details", &[], move |_| {
            state.details.update(|open| *open = !*open);
            Ok(())
        }),
        ViewCommand::global("inspector.notes", "Inspector notes", &[], move |_| {
            state.notes.update(|open| *open = !*open);
            Ok(())
        }),
        ViewCommand::global(
            "inspector.note-put",
            "Save finding",
            &[],
            move |invocation| {
                let (at, stamp, id, anchor, text): (
                    Key,
                    Stamp,
                    Option<NoteId>,
                    Option<AnchorId>,
                    String,
                ) = serde_json::from_value(invocation.args.clone())
                    .map_err(|_| "Select a cell and enter finding text.")?;
                if session.active() != Some(at)
                    || session.stamp().map(|s| (s.workbook, s.model, s.view))
                        != Some((stamp.workbook, stamp.model, stamp.view))
                {
                    return Err(
                        "The note target changed. Refresh the finding before saving.".into(),
                    );
                }
                let text = text.trim().to_owned();
                if text.is_empty() {
                    return Err("Enter finding text.".into());
                }
                let source = cell_source(at);
                let mut changes = Vec::new();
                let anchor = anchor.unwrap_or_else(|| {
                    let anchor = AnchorId(new_id());
                    changes.push(Change::Presentation(PresentationChange::Bind {
                        anchor,
                        source: source.clone(),
                    }));
                    anchor
                });
                let id = id.unwrap_or_else(|| NoteId(new_id()));
                changes.push(Change::Presentation(PresentationChange::PutNote {
                    id,
                    anchor,
                    text,
                }));
                session.submit(Batch {
                    id: String::new(),
                    workbook: Some(stamp.workbook),
                    base: Some(stamp.model),
                    expects: vec![source],
                    selection_seq: None,
                    label: Some("Saved Inspector finding".into()),
                    changes,
                });
                Ok(())
            },
        )
        .when("!editing && writable"),
    ]
}

fn new_id() -> NonZeroU128 {
    NonZeroU128::new(uuid::Uuid::new_v4().as_u128()).expect("a version-4 UUID is nonzero")
}

pub(crate) fn details(session: Session) -> impl IntoView {
    let cells = session.following::<query::Cells>();
    let history = session.following::<query::History>();
    let formula_watch = session.following::<query::Formula>();
    view! {
        <section class="dna-inspector-card">
            <h2>"Cell details"</h2>
            {move || {
                let at = session.active();
                let data = result(cells.at(at.map(|at| query::Cells { interest: Interest::Area(calc_skin::grid::Area(at, at)) })),
                    "Select a cell to inspect.");
                let cell = match data {
                    Ok(answer) => answer.body.cells.into_iter().find(|cell| Some(cell.at) == at),
                    Err(message) => return status(message),
                };
                cell.map(|cell| {
                    let label = match cell.shown.provenance {
                        Provenance::Empty => "Empty",
                        Provenance::Literal => "Literal",
                        Provenance::Calculated { .. } => "Calculated",
                        Provenance::AsSaved => "As saved",
                        Provenance::External { .. } => "External",
                        Provenance::Stale { .. } => "Stale",
                        Provenance::Refused { .. } => "Refused",
                    };
                    view! {
                        <dl data-inspector="cell-details">
                            <dt>"Entry"</dt><dd><code>{cell.input}</code></dd>
                            <dt>"Value"</dt><dd>{cell.text}</dd>
                            <dt>"Raw value"</dt><dd>{cell.shown.value.display()}</dd>
                            <dt>"Provenance"</dt><dd>{label}</dd>
                        </dl>
                    }.into_any()
                }).unwrap_or_else(|| status("This cell is empty.".into()))
            }}
            {move || {
                let formula = session.active().map(|at| query::Formula { at });
                result(formula_watch.at(formula), "").ok().and_then(|answer| answer.body.as_saved).map(|saved| view! {
                    <dl data-inspector="saved-formula">
                        <dt>"As saved in the opened file"</dt><dd>{saved.value.display()}</dd>
                        <dt>"Saved formula"</dt><dd><code>{saved.entry}</code></dd>
                    </dl>
                })
            }}
            {move || match result(history.at(Some(query::History)), "") {
                Ok(answer) => view! {
                    <details><summary>"Workbook history"</summary>
                        <ul>{answer.body.undo.into_iter().take(8).map(|entry| view! { <li>{entry.label}</li> }).collect_view()}</ul>
                    </details>
                }.into_any(),
                Err(message) => status(message),
            }}
        </section>
    }
}

pub(crate) fn mount(session: Session) -> impl IntoView {
    let notes = session.following_with_budget::<query::NotesOf>(crate::INSTRUMENT_BUDGET);
    let part = session.following::<query::ViewPartOf>();
    view! {
        <section class="dna-inspector-card">
            <h2>"Findings"</h2>
            {move || {
                let data = result(notes.at(Some(query::NotesOf { anchor: None })), "");
                let answer = match data {
                    Ok(answer) => answer,
                    Err(message) => return status(message),
                };
                let selected = session.active().map(cell_source);
                let mut found = answer.body.into_iter().filter(|note| note.source == selected && note.source.is_some()).peekable();
                let has_notes = found.peek().is_some();
                let forms = found.map(|note| {
                    note_entry(session, note.text, Some((note.id, note.anchor)))
                }).collect_view();
                let selected_anchor = result(part.at(Some(query::ViewPartOf)), "").ok().and_then(|part| {
                    part.body.anchors.into_iter().find(|anchor| anchor.source == selected && anchor.source.is_some()).map(|anchor| anchor.id)
                });
                view! {
                    {coverage(answer.coverage).map(status)}
                    {(!has_notes).then(|| view! { <p>"No findings on this cell."</p> })}
                    {forms}
                    {session.active().map(|_| note_entry_with_anchor(session, String::new(), None, selected_anchor))}
                }.into_any()
            }}
        </section>
    }
}

fn note_entry(
    session: Session,
    text: String,
    identity: Option<(NoteId, AnchorId)>,
) -> impl IntoView {
    note_entry_with_anchor(
        session,
        text,
        identity.map(|id| id.0),
        identity.map(|id| id.1),
    )
}

fn note_entry_with_anchor(
    session: Session,
    text: String,
    id: Option<NoteId>,
    anchor: Option<AnchorId>,
) -> impl IntoView {
    let input = NodeRef::<leptos::html::Textarea>::new();
    let at = session.active();
    // Capture the target used by the form. A selection move before submission cannot retarget it.
    let stamp = session.stamp();
    let label = if id.is_some() {
        "Edit finding"
    } else {
        "New finding"
    };
    let read_only = session.context().read_only;
    view! {
        <form class="dna-inspector-form" on:submit=move |event| {
            event.prevent_default();
            if let (Some(at), Some(stamp), Some(input)) = (at, stamp, input.get_untracked()) {
                crate::run(session, "inspector.note-put", serde_json::json!([at, stamp, id, anchor, input.value()]));
            }
        }>
            <label>{label}
                <textarea aria-label=label node_ref=input prop:value=text readonly=read_only></textarea>
            </label>
            <button type="submit" data-command="inspector.note-put" disabled=read_only>"Save finding"</button>
        </form>
    }
}
