//! The operator's review before replacing an edited workbook.
use crate::{Live, Seam, Session, ViewCommand};
use calc_skin::{Budget, CloseSummary, Coverage, WorkbookId, query};
use leptos::prelude::*;

const KEEP: &str = "file.keep-open";
const REPLACE: &str = "file.replace-workbook";

struct File {
    name: String,
    bytes: Vec<u8>,
}

#[derive(Clone, Copy, Default)]
struct Review {
    current: RwSignal<Option<WorkbookId>>,
    incoming: StoredValue<Option<File>, LocalStorage>,
    element: NodeRef<leptos::html::Dialog>,
}

impl Review {
    fn clear(self) -> Option<File> {
        self.current.set(None);
        self.incoming.try_update_value(Option::take).flatten()
    }

    fn matches(self, session: Session) -> bool {
        self.current.get()
            .is_some_and(|book| session.stamp().is_some_and(|stamp| stamp.workbook == book))
    }
}

pub fn request(session: Session, name: String, bytes: Vec<u8>) {
    let Some(stamp) = session.stamp().filter(|_| {
        session.edit().is_none() && matches!(session.seam(), Seam::Idle)
    }) else {
        session.open(name, bytes);
        return;
    };
    let review = session.local::<Review>();
    review.incoming.set_value(Some(File { name, bytes }));
    review.current.set(Some(stamp.workbook));
}

pub fn commands(session: Session) -> Vec<ViewCommand> {
    let review = session.local::<Review>();
    vec![
        ViewCommand::global(KEEP, "Keep the current workbook open", &[], move |_| {
            review.clear();
            Ok(())
        }),
        ViewCommand::global(REPLACE, "Discard changes and open the selected workbook", &[], move |_| {
            if !untrack(|| review.matches(session)) {
                return Err("The reviewed workbook changed".into());
            }
            let file = review.clear().ok_or("Choose a workbook to open")?;
            session.open(file.name, file.bytes);
            Ok(())
        }).when("!editing"),
    ]
}

pub fn mount(session: Session) -> impl IntoView {
    let review = session.local::<Review>();
    let active = Memo::new(move |_| review.matches(session));
    let facts = session.following_with_budget::<query::CloseSummaryOf>(Budget {
        cells: 65_536,
        nodes: 65_536,
        edges: 0,
        bytes: 4096,
        millis: 1000,
    });
    let reading = Memo::new(move |_| facts.at(active.get().then_some(query::CloseSummaryOf)));
    let summary = Memo::new(move |_| match reading.get() {
        Live::Ready(answer) if answer.coverage == Coverage::Complete => {
            answer.body.has_changes.then(|| close_text(answer.body))
        }
        Live::Refused(_) | Live::Ready(_) => Some("This workbook may have changes".into()),
        _ => None,
    });
    Effect::new(move |_| {
        if review.current.get().is_some() && !active.get() {
            review.clear();
        }
        if active.get()
            && matches!(reading.get(), Live::Ready(answer)
                if answer.coverage == Coverage::Complete && !answer.body.has_changes)
            && let Some(file) = review.clear()
        {
            session.open(file.name, file.bytes);
        }
    });
    crate::dialog::show_modal(review.element);
    view! {
        <Show when=move || summary.get().is_some()>
            <dialog class="dna-workbook-dialog" data-dnacalc="close-workbook" node_ref=review.element
                data-dna-action-field aria-labelledby="dna-close-title"
                on:cancel=move |event: leptos::ev::Event| {
                    event.prevent_default();
                    let _ = session.command(KEEP);
                }>
                <h2 id="dna-close-title">{move || session.summary().map(|book| format!("Close {}?", book.name))}</h2>
                <p aria-live="polite">{move || summary.get()}</p>
                <div class="dna-page-tools">
                    <button type="button" data-command=KEEP autofocus=true
                        on:click=move |_| { let _ = session.command(KEEP); }>"Keep open"</button>
                    <button type="button" data-command=REPLACE
                        on:click=move |_| { let _ = session.command(REPLACE); }>"Discard changes and open"</button>
                </div>
            </dialog>
        </Show>
    }
}

fn close_text(changes: CloseSummary) -> String {
    let mut actions = Vec::new();
    if changes.added_scenarios > 0 {
        let noun = if changes.added_scenarios == 1 { "scenario" } else { "scenarios" };
        actions.push(format!("added {} {noun}", changes.added_scenarios));
    }
    if changes.changed_inputs > 0 {
        let noun = if changes.changed_inputs == 1 { "input" } else { "inputs" };
        actions.push(format!("changed {} {noun}", changes.changed_inputs));
    }
    if changes.layout_edited {
        actions.push("changed the workbook layout".into());
    }
    if actions.is_empty() {
        "This workbook has changes".into()
    } else {
        format!("You {}", actions.join(" and "))
    }
}
