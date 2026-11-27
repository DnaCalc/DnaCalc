//! The workbook shell's temporary skin preview and source editor.

use super::command_button;
use calc_skin::{SkinManifest, WorkbookId};
use dna_kit::{Session, ViewCommand};
use dna_skin_runtime::skin::{self, Document};
use leptos::prelude::*;

const OPEN: &str = "studio.open";
const TRY: &str = "studio.try-on";
const DISCARD: &str = "studio.discard";
const CLOSE: &str = "studio.close";

#[derive(Clone, PartialEq)]
struct Preview {
    workbook: WorkbookId,
    manifest: Box<SkinManifest>,
    style: String,
    departures: usize,
}

#[derive(Clone, Copy)]
pub(super) struct Studio {
    editor: RwSignal<bool>,
    source: RwSignal<String>,
    error: RwSignal<Option<String>>,
    preview: RwSignal<Option<Preview>>,
}

impl Studio {
    pub(super) fn new() -> Self {
        Self {
            editor: RwSignal::new(false),
            source: RwSignal::new(String::new()),
            error: RwSignal::new(None),
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
                let _ = self.discard(session);
                self.source.set(String::new());
                self.editor.set(false);
            }
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
                match self.try_on(session, source) {
                    Ok(()) => Ok(()),
                    Err(reason) => {
                        self.error.set(Some(reason.clone()));
                        self.editor.set(true);
                        Err(reason)
                    }
                }
            })
            .when("!editing"),
            ViewCommand::global(DISCARD, "Discard the skin preview", &[], move |_| {
                self.discard(session)
            }),
            ViewCommand::global(CLOSE, "Close Customisation Studio", &[], move |_| {
                self.editor.set(false);
                Ok(())
            }),
        ]);
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
        let departures = session.set_skin_layer(Some(&manifest))?;
        self.preview.set(Some(Preview {
            workbook,
            manifest,
            style,
            departures,
        }));
        self.error.set(None);
        self.editor.set(false);
        Ok(())
    }

    fn discard(self, session: Session) -> Result<(), String> {
        session.set_skin_layer(None)?;
        self.preview.set(None);
        self.error.set(None);
        Ok(())
    }

    pub(super) fn escape(self, session: Session) -> bool {
        if self.editor.get_untracked() {
            self.editor.set(false);
            return true;
        }
        if self.preview.with_untracked(Option::is_some) {
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

    pub(super) fn banner(self, session: Session) -> impl IntoView {
        move || {
            self.preview.get().map(|preview| {
                let departures = format!(
                    "Departs from Excel in {} {}",
                    preview.departures,
                    if preview.departures == 1 { "key" } else { "keys" },
                );
                view! {
                    <aside class="dna-skin-banner" data-dnacalc="skin-try-on" aria-label="Skin preview">
                        <strong>{format!("Trying {}", preview.manifest.name)}</strong>
                        <span>{departures}</span>
                        {command_button(session, OPEN, "Edit")}
                        {command_button(session, DISCARD, "Discard")}
                    </aside>
                }
            })
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
