//! The workbook shell's temporary skin preview and source editor.

use super::command_button;
use calc_skin::{Batch, Change, Coverage, Level, PresentationChange, SkinManifest, WorkbookId};
use dna_kit::{Live, Seam, Session, ViewCommand};
use dna_skin_runtime::skin::{self, Document};
use leptos::prelude::*;

const OPEN: &str = "studio.open";
const TRY: &str = "studio.try-on";
const TRY_FILE: &str = "studio.try-file";
const KEEP_FILE: &str = "studio.keep-file";
const DISCARD: &str = "studio.discard";
const CLOSE: &str = "studio.close";

#[derive(Clone, PartialEq)]
struct Preview {
    workbook: WorkbookId,
    kept: bool,
    manifest: Box<SkinManifest>,
    style: String,
    departures: usize,
}

#[derive(Clone, PartialEq)]
struct FileSkin {
    workbook: WorkbookId,
    declaration: Option<SkinManifest>,
    issue: Option<String>,
}

#[derive(Clone, Copy)]
pub(super) struct Studio {
    editor: RwSignal<bool>,
    source: RwSignal<String>,
    error: RwSignal<Option<String>>,
    offer: RwSignal<Option<FileSkin>>,
    preview: RwSignal<Option<Preview>>,
}

impl Studio {
    pub(super) fn new() -> Self {
        Self {
            editor: RwSignal::new(false),
            source: RwSignal::new(String::new()),
            error: RwSignal::new(None),
            offer: RwSignal::new(None),
            preview: RwSignal::new(None),
        }
    }

    pub(super) fn follow_workbook(self, session: Session) {
        let workbook = Memo::new(move |_| session.stamp().map(|stamp| stamp.workbook));
        Effect::new(move |_| {
            let workbook = workbook.get();
            if self.preview.with_untracked(|preview| {
                preview
                    .as_ref()
                    .is_some_and(|preview| Some(preview.workbook) != workbook)
            }) {
                self.clear_preview(session);
                self.source.set(String::new());
                self.editor.set(false);
            }
        });
    }

    pub(super) fn follow_file_skin(self, session: Session) {
        Effect::new(move |previous: Option<Option<FileSkin>>| {
            let previous = previous.flatten();
            let Some(workbook) = session.stamp().map(|stamp| stamp.workbook) else {
                return previous;
            };
            let Live::Ready(answer) = session.view_part() else {
                return previous;
            };
            if answer.stamp.workbook != workbook || answer.coverage != Coverage::Complete {
                return previous;
            }
            let file = FileSkin {
                workbook,
                declaration: answer.body.skin,
                issue: answer.body.skin_issue,
            };
            if previous.as_ref() != Some(&file) {
                let kept = self.preview.with_untracked(|preview| {
                    preview.as_ref().filter(|preview| preview.kept).cloned()
                });
                let active = kept.as_ref().is_some_and(|kept| {
                    kept.workbook == workbook && file.declaration.as_ref() == Some(&*kept.manifest)
                });
                if !active && kept.is_some() {
                    self.clear_preview(session);
                }
                self.offer.set(
                    (!active && (file.declaration.is_some() || file.issue.is_some()))
                        .then(|| file.clone()),
                );
            }
            Some(file)
        });
    }

    pub(super) fn install(self, session: Session) {
        session.install(vec![
            ViewCommand::global(OPEN, "Customisation Studio", &[], move |_| {
                self.editor.set(true);
                Ok(())
            }),
            ViewCommand::global(TRY, "Try on a skin", &[], move |invocation| {
                let source = match &invocation.args {
                    serde_json::Value::Null => self.source.get_untracked(),
                    serde_json::Value::String(source) => source.clone(),
                    source => {
                        serde_json::to_string_pretty(source).map_err(|error| error.to_string())?
                    }
                };
                self.preview_source(session, source)
            })
            .when("!editing"),
            ViewCommand::global(TRY_FILE, "Try on the workbook skin", &[], move |_| {
                let workbook = session
                    .stamp()
                    .ok_or("The workbook is unavailable")?
                    .workbook;
                let skin = self
                    .offer
                    .get_untracked()
                    .filter(|offer| offer.workbook == workbook)
                    .and_then(|offer| offer.declaration)
                    .ok_or("There is no workbook skin to try")?;
                let source =
                    serde_json::to_string_pretty(&skin).map_err(|error| error.to_string())?;
                self.preview_source(session, source)?;
                self.offer.set(None);
                Ok(())
            })
            .when("!editing"),
            ViewCommand::global(KEEP_FILE, "Keep skin for file", &[], move |_| {
                self.keep_file(session)
            })
            .when("!editing && writable"),
            ViewCommand::global(DISCARD, "Discard the skin preview", &[], move |_| {
                self.discard(session)
            }),
            ViewCommand::global(CLOSE, "Close Customisation Studio", &[], move |_| {
                self.editor.set(false);
                Ok(())
            }),
        ]);
    }

    fn preview_source(self, session: Session, source: String) -> Result<(), String> {
        self.try_on(session, source).inspect_err(|reason| {
            self.error.set(Some(reason.clone()));
            self.editor.set(true);
        })
    }

    fn try_on(self, session: Session, source: String) -> Result<(), String> {
        let workbook = session
            .stamp()
            .ok_or("Open a workbook to try on a skin")?
            .workbook;
        self.source.set(source.clone());
        let validated = skin::validate(&source)?;
        let style = validated
            .inline_style()
            .ok_or("Choose a dna.skin/1 declaration")?;
        let Document::Skin(manifest) = validated.document else {
            return Err("Choose a dna.skin/1 declaration".into());
        };
        if manifest.level != Level::Workbook {
            return Err("Choose a workbook skin in this Studio".into());
        }
        let departures = session.set_skin_layer(Some(&manifest))?;
        self.preview.set(Some(Preview {
            workbook,
            kept: false,
            manifest,
            style,
            departures,
        }));
        self.error.set(None);
        self.editor.set(false);
        Ok(())
    }

    fn keep_file(self, session: Session) -> Result<(), String> {
        if !matches!(session.seam(), Seam::Idle) {
            return Err("Finish the current change before keeping the skin".into());
        }
        let stamp = session.stamp().ok_or("The workbook is unavailable")?;
        let preview = self
            .preview
            .get_untracked()
            .filter(|preview| !preview.kept && preview.workbook == stamp.workbook)
            .ok_or("Try on a skin before keeping it")?;
        let skin = preview.manifest.clone();
        if matches!(session.view_part(), Live::Ready(answer)
            if answer.stamp.workbook == stamp.workbook
                && answer.stamp.view == stamp.view
                && answer.coverage == Coverage::Complete
                && answer.body.skin.as_ref() == Some(&*skin))
        {
            self.preview.update(|current| {
                if let Some(active) = current.as_mut() {
                    active.kept = true;
                }
            });
            self.offer.set(None);
            return Ok(());
        }
        let change = Batch {
            id: String::new(),
            workbook: Some(stamp.workbook),
            base: Some(stamp.model),
            expects: Vec::new(),
            selection_seq: None,
            label: Some(format!("Kept skin {} for file", skin.name)),
            changes: vec![Change::Presentation(PresentationChange::KeepSkin {
                skin,
                expect_view_rev: stamp.view,
            })],
        };
        session.submit_then(change, move || {
            self.preview.update(|current| {
                if let Some(active) = current.as_mut()
                    && active == &preview
                {
                    active.kept = true;
                }
            });
            self.offer.set(None);
        });
        Ok(())
    }

    fn clear_preview(self, session: Session) {
        session
            .set_skin_layer(None)
            .expect("the built-in keymap is valid");
        self.preview.set(None);
        self.error.set(None);
    }

    fn discard(self, session: Session) -> Result<(), String> {
        if self
            .preview
            .with_untracked(|preview| preview.as_ref().is_some_and(|active| !active.kept))
        {
            self.clear_preview(session);
        }
        self.offer.set(None);
        Ok(())
    }

    pub(super) fn escape(self, session: Session) -> bool {
        if self.editor.get_untracked() {
            self.editor.set(false);
            return true;
        }
        if self.offer.with_untracked(Option::is_some)
            || self
                .preview
                .with_untracked(|preview| preview.as_ref().is_some_and(|active| !active.kept))
        {
            let _ = session.command(DISCARD);
            return true;
        }
        false
    }

    pub(super) fn style(self) -> Option<String> {
        self.preview
            .with(|preview| preview.as_ref().map(|preview| preview.style.clone()))
    }

    pub(super) fn theme(self) -> Option<String> {
        self.preview.with(|preview| {
            preview
                .as_ref()
                .map(|preview| preview.manifest.theme.base.clone())
        })
    }

    pub(super) fn density(self) -> Option<String> {
        self.preview.with(|preview| {
            preview
                .as_ref()
                .map(|preview| preview.manifest.theme.density.clone())
        })
    }

    pub(super) fn compact_deck(self) -> bool {
        self.preview.with(|preview| {
            preview
                .as_ref()
                .is_some_and(|preview| preview.manifest.shell.deck.as_deref() == Some("compact"))
        })
    }

    pub(super) fn editing(self) -> bool {
        self.editor.get()
    }

    pub(super) fn trying(self) -> bool {
        self.preview
            .with(|preview| preview.as_ref().is_some_and(|active| !active.kept))
    }

    pub(super) fn banner(self, session: Session) -> impl IntoView {
        move || {
            if let Some(preview) = self.preview.get().filter(|preview| !preview.kept) {
                let departures = format!(
                    "Departs from Excel in {} {}",
                    preview.departures,
                    if preview.departures == 1 {
                        "key"
                    } else {
                        "keys"
                    },
                );
                view! {
                    <aside class="dna-skin-banner" data-dnacalc="skin-try-on" aria-label="Skin preview">
                        <strong>{format!("Trying {}", preview.manifest.name)}</strong>
                        <span>{departures}</span>
                        {command_button(session, KEEP_FILE, "Keep for file")}
                        {command_button(session, OPEN, "Edit")}
                        {command_button(session, DISCARD, "Discard")}
                    </aside>
                }.into_any()
            } else if let Some(offer) = self.offer.get() {
                let name = offer.declaration.as_ref().map(|skin| skin.name.clone());
                view! {
                    <aside class="dna-skin-banner" data-dnacalc="skin-offer" aria-label="Workbook skin">
                        <strong>{name.clone().unwrap_or_else(|| "Workbook skin".into())}</strong>
                        {offer.issue.map(|issue| view! { <span role="alert">{issue}</span> })}
                        {name.map(|_| command_button(session, TRY_FILE, "Try on"))}
                        {command_button(session, DISCARD, "Discard")}
                    </aside>
                }.into_any()
            } else {
                ().into_any()
            }
        }
    }

    pub(super) fn editor(self, session: Session) -> impl IntoView {
        let source_field = NodeRef::<leptos::html::Textarea>::new();
        Effect::new(move |_| {
            if let Some(field) = source_field.get() {
                let _ = field.focus();
            }
        });
        view! {
            <Show when=move || self.editor.get()>
                <aside class="dna-book-details dna-skin-studio" data-dnacalc="skin-studio">
                    <h2>"Customisation Studio"</h2>
                    <label>"Skin JSON"
                        <textarea data-dnacalc="skin-source" data-dna-action-field
                            node_ref=source_field spellcheck="false" rows="16"
                            prop:value=move || self.source.get()
                            on:input=move |event| self.source.set(event_target_value(&event))>
                        </textarea>
                    </label>
                    <p role="alert">{move || self.error.get()}</p>
                    {command_button(session, TRY, "Try on")}
                    {command_button(session, CLOSE, "Close")}
                </aside>
            </Show>
        }
    }
}
