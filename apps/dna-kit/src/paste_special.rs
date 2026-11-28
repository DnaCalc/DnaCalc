//! Classic's workbook clipboard commands and Paste Special dialog.

use crate::{Seam, Session, ViewCommand};
use calc_skin::grid::Area;
use calc_skin::{
    Batch, Change, ModelChange, Operation, PasteArithmetic, PasteContents, PastePart, Revision,
    SourceRef, ViewId, WorkbookId,
};
use leptos::prelude::*;

pub const OPEN: &str = "classic.paste-special";
const APPLY: &str = "classic.apply-paste-special";

#[derive(Clone, Copy, PartialEq, Eq)]
struct Destination {
    workbook: WorkbookId,
    revision: Revision,
    copy: u64,
    to: Area,
}

impl Destination {
    fn batch(self, label: &str, contents: PasteContents) -> Batch {
        Batch {
            id: String::new(),
            workbook: Some(self.workbook),
            base: Some(self.revision),
            expects: Vec::new(),
            selection_seq: None,
            label: Some(label.into()),
            changes: vec![Change::Model(ModelChange::PasteCopied {
                copy: self.copy,
                to: self.to,
                contents,
            })],
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Clipboard {
    copied: RwSignal<Option<(WorkbookId, u64)>>,
    destination: RwSignal<Option<Destination>>,
    part: RwSignal<PastePart>,
    transpose: RwSignal<bool>,
    skip_blanks: RwSignal<bool>,
    arithmetic: RwSignal<Option<PasteArithmetic>>,
    dialog: NodeRef<leptos::html::Dialog>,
}

impl Clipboard {
    fn destination(self, session: Session) -> Result<Destination, String> {
        let stamp = session.stamp().ok_or("The workbook is unavailable.")?;
        let (_, copy) = self
            .copied
            .get_untracked()
            .filter(|(workbook, _)| *workbook == stamp.workbook)
            .ok_or("Copy worksheet cells before pasting.")?;
        Ok(Destination {
            workbook: stamp.workbook,
            revision: stamp.model,
            copy,
            to: single_range(session)?,
        })
    }
}

fn single_range(session: Session) -> Result<Area, String> {
    if let Some(selection) = session.selection()
        && let SourceRef::Cells { areas, .. } = selection.target
        && let [area] = areas.as_slice()
    {
        return Ok(*area);
    }
    Err("Select one rectangular range of worksheet cells.".into())
}

pub fn commands(view: &ViewId, session: Session) -> [ViewCommand; 6] {
    let clipboard = session.local::<Clipboard>();
    let paste = |id, label, chords: &[&'static str], contents| {
        ViewCommand::new(view, id, label, chords, move |_| {
            let to = clipboard.destination(session)?;
            session.submit(to.batch(label, contents));
            Ok(())
        })
        .when("mode:ready && writable")
    };
    [
        ViewCommand::new(
            view,
            "classic.copy-cells",
            "Copy cells",
            &["Ctrl+C"],
            move |_| {
                let workbook = session
                    .stamp()
                    .ok_or("The workbook is unavailable.")?
                    .workbook;
                let from = single_range(session)?;
                let copy = clipboard
                    .copied
                    .get_untracked()
                    .map_or(1, |(_, previous)| previous + 1);
                clipboard.copied.set(Some((workbook, copy)));
                session.operate(
                    Operation::CopyCells {
                        workbook,
                        copy,
                        from,
                    },
                    Some("Copied worksheet cells.".into()),
                );
                Ok(())
            },
        )
        .when("mode:ready"),
        paste(
            "classic.paste-values",
            "Paste Values",
            &["Ctrl+Shift+V"],
            PasteContents::Values,
        ),
        paste(
            "classic.paste-formulas",
            "Paste Formulas",
            &[],
            PasteContents::Formulas,
        ),
        paste(
            "classic.paste-transpose",
            "Paste Transpose",
            &[],
            PasteContents::AllTransposed,
        ),
        ViewCommand::new(view, OPEN, "Paste Special", &["Ctrl+Alt+V"], move |_| {
            if !matches!(session.seam(), Seam::Idle) {
                return Err("Finish reviewing the current edit first.".into());
            }
            let to = clipboard.destination(session)?;
            clipboard.part.set(PastePart::All);
            clipboard.transpose.set(false);
            clipboard.skip_blanks.set(false);
            clipboard.arithmetic.set(None);
            clipboard.destination.set(Some(to));
            Ok(())
        })
        .when("mode:ready && writable"),
        ViewCommand::new(view, APPLY, "Apply Paste Special", &[], move |_| {
            let to = clipboard
                .destination
                .get_untracked()
                .filter(|target| {
                    session
                        .stamp()
                        .is_some_and(|stamp| stamp.workbook == target.workbook)
                })
                .ok_or("Open Paste Special for the current workbook.")?;
            let contents = PasteContents::Special {
                part: clipboard.part.get_untracked(),
                transpose: clipboard.transpose.get_untracked(),
                skip_blanks: clipboard.skip_blanks.get_untracked(),
                arithmetic: clipboard.arithmetic.get_untracked(),
            };
            close(session);
            session.submit(to.batch("Paste Special", contents));
            Ok(())
        })
        .when("mode:ready && writable"),
    ]
}

const CHOICES: &[(PastePart, &str, &str, &str)] = &[
    (PastePart::All, "A", "All", "paste-special-all"),
    (
        PastePart::Formulas,
        "F",
        "Formulas",
        "paste-special-formulas",
    ),
    (PastePart::Values, "V", "Values", "paste-special-values"),
    (PastePart::Formats, "T", "Formats", "paste-special-formats"),
    (
        PastePart::Comments,
        "C",
        "Comments and notes",
        "paste-special-comments",
    ),
    (
        PastePart::Validation,
        "N",
        "Validation",
        "paste-special-validation",
    ),
    (
        PastePart::AllUsingSourceTheme,
        "H",
        "All using source theme",
        "paste-special-source-theme",
    ),
    (
        PastePart::AllExceptBorders,
        "X",
        "All except borders",
        "paste-special-no-borders",
    ),
    (
        PastePart::FormulasAndNumberFormats,
        "R",
        "Formulas and number formats",
        "paste-special-formula-formats",
    ),
    (
        PastePart::ValuesAndNumberFormats,
        "U",
        "Values and number formats",
        "paste-special-value-formats",
    ),
    (
        PastePart::AllMergingConditionalFormats,
        "",
        "All merging conditional formats",
        "paste-special-merge-formats",
    ),
];

const OPERATIONS: &[(Option<PasteArithmetic>, &str, &str, &str)] = &[
    (None, "", "None", "paste-special-operation-none"),
    (
        Some(PasteArithmetic::Add),
        "D",
        "Add",
        "paste-special-operation-add",
    ),
    (
        Some(PasteArithmetic::Subtract),
        "S",
        "Subtract",
        "paste-special-operation-subtract",
    ),
    (
        Some(PasteArithmetic::Multiply),
        "M",
        "Multiply",
        "paste-special-operation-multiply",
    ),
    (
        Some(PasteArithmetic::Divide),
        "I",
        "Divide",
        "paste-special-operation-divide",
    ),
];

pub fn close(session: Session) -> bool {
    let clipboard = session.local::<Clipboard>();
    if clipboard.destination.get_untracked().is_none() {
        return false;
    }
    if let Some(dialog) = clipboard.dialog.get_untracked() {
        dialog.close();
    }
    clipboard.destination.set(None);
    true
}

pub fn mount(session: Session) -> impl IntoView {
    let clipboard = session.local::<Clipboard>();
    let shown = Memo::new(move |_| {
        clipboard.destination.get().is_some_and(|to| {
            session
                .stamp()
                .is_some_and(|stamp| stamp.workbook == to.workbook)
        })
    });
    move || shown.get().then(|| dialog(session, clipboard))
}

fn dialog(session: Session, clipboard: Clipboard) -> impl IntoView {
    crate::dialog::show_modal(clipboard.dialog);
    let mnemonic_key = |event: &leptos::ev::KeyboardEvent| {
        (!event.ctrl_key()
            && !event.meta_key()
            && !event.get_modifier_state("AltGraph")
            && !event.is_composing())
        .then(|| event.key())
    };
    let mnemonic = move |event: leptos::ev::KeyboardEvent| {
        let Some(key) = mnemonic_key(&event) else {
            return;
        };
        if key.eq_ignore_ascii_case("e") {
            clipboard.transpose.update(|value| *value = !*value);
        } else if key.eq_ignore_ascii_case("s") {
            clipboard.skip_blanks.update(|value| *value = !*value);
        } else if let Some((part, _, _, _)) = CHOICES
            .iter()
            .find(|(_, letter, _, _)| letter.eq_ignore_ascii_case(&key))
        {
            clipboard.part.set(*part);
        } else if let Some((operation, _, _, _)) = OPERATIONS
            .iter()
            .find(|(_, letter, _, _)| letter.eq_ignore_ascii_case(&key))
        {
            clipboard.arithmetic.set(*operation);
        } else {
            return;
        }
        event.prevent_default();
    };
    let subtract = move |event: leptos::ev::KeyboardEvent| {
        if mnemonic_key(&event).is_some_and(|key| key.eq_ignore_ascii_case("s")) {
            clipboard.arithmetic.set(Some(PasteArithmetic::Subtract));
            event.stop_propagation();
            event.prevent_default();
        }
    };
    view! {
        <dialog class="dna-workbook-dialog dna-paste-special" node_ref=clipboard.dialog
            data-dnacalc="paste-special" data-dna-action-field aria-labelledby="dna-paste-special-title"
            on:cancel=move |event: leptos::ev::Event| { event.prevent_default(); close(session); }
            on:keydown=mnemonic>
            <h2 id="dna-paste-special-title">"Paste Special"</h2>
            <form on:submit=move |event: leptos::ev::SubmitEvent| {
                event.prevent_default();
                let _ = session.command(APPLY);
            }>
                <fieldset>
                    <legend>"Paste"</legend>
                    <div class="dna-paste-special-options">
                        {CHOICES.iter().copied().map(|(part, key, label, role)| view! {
                            <label>
                                <input type="radio" name="paste-part" data-dnacalc=role
                                    prop:checked=move || clipboard.part.get() == part
                                    on:change=move |_| clipboard.part.set(part)/>
                                {label} {(!key.is_empty()).then(|| view! { <kbd>{key}</kbd> })}
                            </label>
                        }).collect_view()}
                    </div>
                </fieldset>
                <fieldset on:keydown=subtract>
                    <legend>"Operation"</legend>
                    <div class="dna-paste-special-options">
                        {OPERATIONS.iter().copied().map(|(operation, key, label, role)| view! {
                            <label>
                                <input type="radio" name="paste-operation" data-dnacalc=role
                                    prop:checked=move || clipboard.arithmetic.get() == operation
                                    on:change=move |_| clipboard.arithmetic.set(operation)/>
                                {label} {(!key.is_empty()).then(|| view! { <kbd>{key}</kbd> })}
                            </label>
                        }).collect_view()}
                    </div>
                </fieldset>
                <label>
                    <input type="checkbox" data-dnacalc="paste-special-skip-blanks"
                        prop:checked=move || clipboard.skip_blanks.get()
                        on:change=move |event| clipboard.skip_blanks.set(event_target_checked(&event))/>
                    "Skip blanks" <kbd>"S"</kbd>
                </label>
                <label>
                    <input type="checkbox" prop:checked=move || clipboard.transpose.get()
                        on:change=move |event| clipboard.transpose.set(event_target_checked(&event))/>
                    "Transpose" <kbd>"E"</kbd>
                </label>
                <footer>
                    <button type="submit" data-dnacalc="paste-special-apply" data-command=APPLY>"OK"</button>
                    <button type="button" data-dnacalc="paste-special-cancel"
                        on:click=move |_| { close(session); }>"Cancel"</button>
                </footer>
            </form>
        </dialog>
    }
}
