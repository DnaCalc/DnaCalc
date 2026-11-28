//! The Classic Data Validation dialog over the workbook's authored rules.

use crate::{Live, Seam, Session, ViewCommand, refusal_text};
use calc_skin::grid::Area;
use calc_skin::{
    Batch, Change, ModelChange, Revision, SourceRef, ValidationSettings, ViewId, WorkbookId, query,
};
use leptos::prelude::*;

pub const OPEN: &str = "classic.data-validation";
const APPLY: &str = "classic.apply-validation";

#[derive(Clone, Copy, PartialEq, Eq)]
struct Target {
    workbook: WorkbookId,
    revision: Revision,
    selected: Area,
}

#[derive(Clone, Copy, Default)]
struct ValidationDialog {
    target: RwSignal<Option<Target>>,
    settings: RwSignal<Option<ValidationSettings>>,
    element: NodeRef<leptos::html::Dialog>,
}

pub fn commands(view: &ViewId, session: Session) -> [ViewCommand; 2] {
    let dialog = session.local::<ValidationDialog>();
    [
        ViewCommand::new(view, OPEN, "Data Validation", &[], move |_| {
            let selection = session.selection().ok_or("Select cells to validate.")?;
            let SourceRef::Cells { areas, .. } = selection.target else {
                return Err("Select worksheet cells to validate.".into());
            };
            let [selected] = areas.as_slice() else {
                return Err("Select one rectangular range to validate.".into());
            };
            let stamp = session.stamp().ok_or("The workbook is unavailable.")?;
            dialog.settings.set(None);
            dialog.target.set(Some(Target {
                workbook: stamp.workbook,
                revision: stamp.model,
                selected: *selected,
            }));
            Ok(())
        })
        .when("mode:ready && writable"),
        ViewCommand::new(view, APPLY, "Apply data validation", &[], move |_| {
            let target = dialog
                .target
                .get_untracked()
                .ok_or("Open Data Validation first.")?;
            let settings = dialog
                .settings
                .get_untracked()
                .ok_or("Loading data validation settings.")?;
            if !matches!(session.seam(), Seam::Idle) {
                return Err("Finish reviewing the current edit first.".into());
            }
            if !session.stamp().is_some_and(|stamp| {
                stamp.workbook == target.workbook && stamp.model == target.revision
            }) {
                return Err("The workbook changed. Reopen Data Validation.".into());
            }
            close(session);
            session.submit(Batch {
                id: String::new(),
                workbook: Some(target.workbook),
                base: Some(target.revision),
                expects: Vec::new(),
                selection_seq: None,
                label: Some("Data Validation".into()),
                changes: vec![Change::Model(ModelChange::SetValidation {
                    selected: target.selected,
                    settings,
                })],
            });
            Ok(())
        })
        .when("mode:ready && writable"),
    ]
}

pub fn close(session: Session) -> bool {
    let dialog = session.local::<ValidationDialog>();
    if dialog.target.with_untracked(Option::is_none) {
        return false;
    }
    if let Some(element) = dialog.element.get_untracked() {
        element.close();
    }
    dialog.target.set(None);
    true
}

pub fn mount(session: Session) -> impl IntoView {
    let dialog = session.local::<ValidationDialog>();
    let target = Memo::new(move |_| {
        let stamp = session.stamp()?;
        dialog
            .target
            .get()
            .filter(|target| target.workbook == stamp.workbook)
    });
    move || target.get().map(|target| dialog.show(session, target))
}

impl ValidationDialog {
    fn show(self, session: Session, target: Target) -> impl IntoView {
        crate::dialog::show_modal(self.element);
        let watch = session.watch(query::ValidationFor {
            selected: target.selected,
        });
        on_cleanup(move || session.unwatch(watch));
        let current = move || {
            session.stamp().is_some_and(|stamp| {
                stamp.workbook == target.workbook && stamp.model == target.revision
            })
        };
        Effect::new(move |_| {
            if let Live::Ready(answer) = session.read(watch)
                && answer.stamp.workbook == target.workbook
                && answer.stamp.model == target.revision
                && self.settings.with_untracked(Option::is_none)
            {
                self.settings.set(Some(answer.body));
            }
        });
        let ready = move || current() && self.settings.with(Option::is_some);
        view! {
            <dialog class="dna-workbook-dialog dna-validation-dialog" node_ref=self.element data-dnacalc="data-validation"
                data-dna-action-field aria-labelledby="dna-validation-title"
                on:cancel=move |event: leptos::web_sys::Event| { event.prevent_default(); close(session); }>
                <h2 id="dna-validation-title">"Data Validation"</h2>
                <form on:submit=move |event| { event.prevent_default(); let _ = session.command(APPLY); }>
                    <Show when=ready fallback=move || view! {
                        <p role="status">{move || {
                            if !current() {
                                return "The workbook changed. Reopen Data Validation.".into();
                            }
                            match session.read(watch) {
                                Live::Refused(reason) => refusal_text(reason),
                                _ => "Loading data validation settings…".into(),
                            }
                        }}</p>
                    }>
                        {self.fields()}
                    </Show>
                    <footer>
                        <button type="button" on:click=move |_| { close(session); }>"Cancel"</button>
                        <button type="submit" data-dnacalc="validation-apply" data-command=APPLY
                            disabled=move || !ready()>"OK"</button>
                    </footer>
                </form>
            </dialog>
        }
    }

    fn fields(self) -> impl IntoView {
        let list = move || {
            self.settings
                .with(|settings| matches!(settings, Some(ValidationSettings::List { .. })))
        };
        view! {
            <label>"Allow"
                <select data-dnacalc="validation-kind"
                    prop:value=move || if list() { "list" } else { "any" }
                    on:change=move |event| self.settings.set(Some(match event_target_value(&event).as_str() {
                        "list" => ValidationSettings::List { source: String::new() },
                        _ => ValidationSettings::AnyValue,
                    }))>
                    <option value="any" selected=move || !list()>"Any value"</option>
                    <option value="list" selected=list>"List"</option>
                </select>
            </label>
            <Show when=list>
                <label>"Source"
                    <input type="text" data-dnacalc="validation-source" maxlength=255 required
                        prop:value=move || self.settings.with(|settings| match settings {
                            Some(ValidationSettings::List { source }) => source.clone(),
                            _ => String::new(),
                        })
                        on:input=move |event| self.settings.set(Some(ValidationSettings::List {
                            source: event_target_value(&event),
                        }))/>
                </label>
            </Show>
        }
    }
}
