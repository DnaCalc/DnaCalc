//! The workbook shell's temporary skin preview and source editor.

use super::command_button;
use calc_skin::{
    Batch, Change, Coverage, Level, Page, PresentationChange, SkinManifest, TryOnPayload, WorkbookId,
};
use dna_kit::{Invocation, Live, Seam, Session, ViewCommand, commands::CommandId};
use dna_skin_runtime::skin::{self, Document};
use leptos::prelude::*;

const OPEN: &str = "studio.open";
const TRY: &str = "studio.try-on";
const TRY_FILE: &str = "studio.try-file";
const TRY_AGENT: &str = "studio.try-agent";
const DISCARD_AGENT: &str = "studio.discard-agent";
const KEEP_FILE: &str = "studio.keep-file";
const KEEP_ME: &str = "studio.keep-me";
const CLEAR_PROFILE: &str = "studio.clear-personal-skin";
#[cfg(target_arch = "wasm32")]
const PROFILE_STORAGE: &str = "dna.workbook-skin/1";
const DISCARD: &str = "studio.discard";
const CLOSE: &str = "studio.close";

#[derive(Clone, PartialEq)]
struct Appearance {
    manifest: Box<SkinManifest>,
    style: String,
}

impl Appearance {
    fn parse(source: &str) -> Result<Self, String> {
        Self::from_skin(skin::validate(source)?)
    }

    fn from_skin(validated: skin::Validated) -> Result<Self, String> {
        let style = validated
            .inline_style()
            .ok_or("Choose a dna.skin/1 declaration")?;
        let Document::Skin(manifest) = validated.document else {
            return Err("Choose a dna.skin/1 declaration".into());
        };
        if manifest.level != Level::Workbook {
            return Err("Choose a workbook skin in this Studio".into());
        }
        Ok(Self { manifest, style })
    }
}

#[derive(Clone, PartialEq)]
struct Preview {
    workbook: WorkbookId,
    appearance: Appearance,
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
    kept_file: RwSignal<Option<Preview>>,
    personal: RwSignal<Option<Appearance>>,
}

impl Studio {
    pub(super) fn new() -> Self {
        Self {
            editor: RwSignal::new(false),
            source: RwSignal::new(String::new()),
            error: RwSignal::new(None),
            offer: RwSignal::new(None),
            preview: RwSignal::new(None),
            kept_file: RwSignal::new(None),
            personal: RwSignal::new(None),
        }
    }

    pub(super) fn follow_workbook(self, session: Session) {
        let workbook = Memo::new(move |_| session.stamp().map(|stamp| stamp.workbook));
        Effect::new(move |previous: Option<Option<WorkbookId>>| {
            let workbook = workbook.get();
            if previous.is_none() {
                self.personal.set(read_profile().ok().flatten());
            }
            if previous.flatten() != workbook {
                self.preview.set(None);
                self.kept_file.set(None);
                self.offer.update(|offer| {
                    if offer
                        .as_ref()
                        .is_some_and(|offer| Some(offer.workbook) != workbook)
                    {
                        *offer = None;
                    }
                });
                self.source.set(String::new());
                self.editor.set(false);
                self.error.set(None);
                if workbook.is_some()
                    && let Err(error) = self.restore_chords(session)
                {
                    self.personal.set(None);
                    self.error.set(Some(error));
                }
            }
            workbook
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
                let kept = self.kept_file.get_untracked();
                let active = kept.as_ref().is_some_and(|kept| {
                    kept.workbook == workbook
                        && file.declaration.as_ref() == Some(&*kept.appearance.manifest)
                });
                if !active && kept.is_some() {
                    self.kept_file.set(None);
                    if self.preview.with_untracked(Option::is_none) {
                        let _ = self.restore_chords(session);
                    }
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
            ViewCommand::global(TRY_AGENT, "Try on the agent proposal", &[], move |_| {
                let offer = session
                    .take_look_offer()
                    .ok_or("There is no agent proposal to try")?;
                match offer.look {
                    TryOnPayload::Skin(skin) => {
                        let source = serde_json::to_string_pretty(&skin)
                            .map_err(|error| error.to_string())?;
                        self.preview_source(session, source)
                    }
                    TryOnPayload::Page(page) => self.preview_app_page(session, &page),
                }
            })
            .when("!editing"),
            ViewCommand::global(
                DISCARD_AGENT,
                "Discard the agent proposal",
                &[],
                move |_| {
                    session.take_look_offer();
                    Ok(())
                },
            ),
            ViewCommand::global(KEEP_FILE, "Keep skin for file", &[], move |_| {
                self.keep_file(session)
            })
            .when("!editing && writable"),
            ViewCommand::global(KEEP_ME, "Keep skin for me", &[], move |_| {
                self.keep_personal(session)
            })
            .when("!editing"),
            ViewCommand::global(CLEAR_PROFILE, "Clear my saved skin", &[], move |_| {
                write_profile(None)?;
                self.personal.set(None);
                if self.preview.with_untracked(Option::is_none) {
                    self.restore_chords(session)?;
                }
                Ok(())
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
        if let Document::Page(page) = &validated.document {
            return self.preview_app_page(session, page);
        }
        let appearance = Appearance::from_skin(validated)?;
        let departures = session.set_skin_layer(Some(&appearance.manifest))?;
        self.preview.set(Some(Preview {
            workbook,
            appearance,
            departures,
        }));
        self.error.set(None);
        self.editor.set(false);
        Ok(())
    }

    fn preview_app_page(self, session: Session, page: &Page) -> Result<(), String> {
        let args = serde_json::to_value(page).map_err(|error| error.to_string())?;
        session
            .run(&Invocation {
                command: CommandId::new(dna_kit::page::TRY_APP_PAGE),
                args,
            })
            .map_err(|_| session.notice())?;
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
            .filter(|preview| preview.workbook == stamp.workbook)
            .ok_or("Try on a skin before keeping it")?;
        let skin = preview.appearance.manifest.clone();
        if matches!(session.view_part(), Live::Ready(answer)
            if answer.stamp.workbook == stamp.workbook
                && answer.stamp.view == stamp.view
                && answer.coverage == Coverage::Complete
                && answer.body.skin.as_ref() == Some(&*skin))
        {
            self.accept_file(preview);
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
        session.submit_then(change, move || self.accept_file(preview));
        Ok(())
    }

    fn accept_file(self, preview: Preview) {
        self.kept_file.set(Some(preview.clone()));
        if self
            .preview
            .with_untracked(|current| current.as_ref() == Some(&preview))
        {
            self.preview.set(None);
        }
        self.offer.set(None);
    }

    fn keep_personal(self, session: Session) -> Result<(), String> {
        let workbook = session
            .stamp()
            .ok_or("The workbook is unavailable")?
            .workbook;
        let preview = self
            .preview
            .get_untracked()
            .filter(|preview| preview.workbook == workbook)
            .ok_or("Try on a skin before keeping it")?;
        if !preview.appearance.manifest.pages.is_empty() {
            return Err("Keep pages with the workbook that owns their bindings".into());
        }
        write_profile(Some(&preview.appearance.manifest))?;
        self.personal.set(Some(preview.appearance));
        self.kept_file.set(None);
        self.preview.set(None);
        self.offer.set(None);
        Ok(())
    }

    /// Reinstall the appearance underneath the temporary trial.
    fn restore_chords(self, session: Session) -> Result<(), String> {
        let file = self.kept_file.get_untracked();
        let personal = self.personal.get_untracked();
        let baseline = file
            .as_ref()
            .map(|skin| &skin.appearance)
            .or(personal.as_ref());
        session
            .set_skin_layer(baseline.map(|skin| &*skin.manifest))
            .map(|_| ())
    }

    fn discard(self, session: Session) -> Result<(), String> {
        if self.preview.with_untracked(Option::is_some) {
            self.restore_chords(session)?;
            self.preview.set(None);
            self.error.set(None);
        }
        self.offer.set(None);
        Ok(())
    }

    pub(super) fn escape(self, session: Session) -> bool {
        if self.editor.get_untracked() {
            self.editor.set(false);
            return true;
        }
        if self.preview.with_untracked(Option::is_none)
            && untrack(|| session.look_offer().is_some())
        {
            let _ = session.command(DISCARD_AGENT);
            return true;
        }
        if self.offer.with_untracked(Option::is_some)
            || self.preview.with_untracked(Option::is_some)
        {
            let _ = session.command(DISCARD);
            return true;
        }
        false
    }

    fn appearance<T>(self, read: impl Fn(&Appearance) -> T) -> Option<T> {
        self.preview
            .with(|preview| preview.as_ref().map(|trial| read(&trial.appearance)))
            .or_else(|| {
                self.kept_file
                    .with(|file| file.as_ref().map(|kept| read(&kept.appearance)))
            })
            .or_else(|| self.personal.with(|personal| personal.as_ref().map(&read)))
    }

    pub(super) fn style(self) -> Option<String> {
        self.appearance(|skin| skin.style.clone())
    }

    pub(super) fn theme(self) -> Option<String> {
        self.appearance(|skin| skin.manifest.theme.base.clone())
    }

    pub(super) fn density(self) -> Option<String> {
        self.appearance(|skin| skin.manifest.theme.density.clone())
    }

    pub(super) fn compact_deck(self) -> bool {
        self.appearance(|skin| skin.manifest.shell.deck.as_deref() == Some("compact"))
            .unwrap_or(false)
    }

    pub(super) fn editing(self) -> bool {
        self.editor.get()
    }

    pub(super) fn trying(self) -> bool {
        self.preview.with(Option::is_some)
    }

    pub(super) fn banner(self, session: Session) -> impl IntoView {
        move || {
            if let Some(preview) = self.preview.get() {
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
                        <strong>{format!("Trying {}", preview.appearance.manifest.name)}</strong>
                        <span>{departures}</span>
                        {command_button(session, KEEP_FILE, "Keep for file")}
                        {command_button(session, KEEP_ME, "Keep for me")}
                        {command_button(session, OPEN, "Edit")}
                        {command_button(session, DISCARD, "Discard")}
                    </aside>
                }.into_any()
            } else if let Some(offer) = session.look_offer() {
                let title = match offer.look {
                    TryOnPayload::Skin(manifest) => manifest.name.clone(),
                    TryOnPayload::Page(page) => page.title.clone(),
                };
                view! {
                    <aside class="dna-skin-banner" data-dnacalc="agent-look-offer" aria-label="Agent proposal">
                        <strong>{title}</strong>
                        <span>{offer.agent}</span>
                        {command_button(session, TRY_AGENT, "Try on")}
                        {command_button(session, DISCARD_AGENT, "Discard")}
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
                    {move || {
                        let source = self.source.get();
                        skin::review_theme(&source).ok().map(contrast_review)
                    }}
                    {command_button(session, TRY, "Try on")}
                    {command_button(session, "proposal.open", "Agent trail")}
                    {move || self.personal.with(Option::is_some).then(|| {
                        command_button(session, CLEAR_PROFILE, "Clear my saved skin")
                    })}
                    {command_button(session, CLOSE, "Close")}
                </aside>
            </Show>
        }
    }
}

fn contrast_review(report: skin::ThemeReview) -> impl IntoView {
    let count = report.contrast.len();
    let failing = report
        .contrast
        .iter()
        .filter(|reading| !reading.passes())
        .count();
    let rows = report
        .contrast
        .into_iter()
        .filter(|reading| {
            !reading.passes()
                || (reading.theme == report.theme
                    && matches!(
                        (reading.foreground, reading.background),
                        ("ink" | "ink-2" | "ink-3", "paper" | "paper-2")
                            | ("accent-ink", "paper")
                            | ("chrome-ink" | "amber", "chrome")
                    ))
        })
        .map(|reading| {
            let pair = format!("{} on {}", reading.foreground, reading.background);
            let label = pair.clone();
            view! {
                <tr data-skin-contrast=pair data-contrast-pass=reading.passes().to_string()>
                    <th scope="row">{label}</th>
                    <td>{format!("{:.4}:1", reading.ratio)}</td>
                    <td>{format!("{:.1}:1", reading.minimum)}</td>
                    <td>{if reading.passes() { "Pass" } else { "Fail" }}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <details class="dna-skin-contrast" open={failing > 0}>
            <summary>{format!("Contrast · {} of {count} pass", count - failing)}</summary>
            <table>
                <thead><tr><th>"Pair"</th><th>"Ratio"</th><th>"Minimum"</th><th>"Result"</th></tr></thead>
                <tbody>{rows}</tbody>
            </table>
        </details>
    }
}

fn read_profile() -> Result<Option<Appearance>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        let source = profile_storage()?
            .get_item(PROFILE_STORAGE)
            .map_err(|_| "Cannot read your saved skin")?;
        return source.as_deref().map(Appearance::parse).transpose();
    }
    #[cfg(not(target_arch = "wasm32"))]
    Ok(None)
}

fn write_profile(manifest: Option<&SkinManifest>) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = profile_storage()?;
        return match manifest {
            Some(manifest) => {
                let source = serde_json::to_string(manifest).map_err(|error| error.to_string())?;
                storage
                    .set_item(PROFILE_STORAGE, &source)
                    .map_err(|_| "Cannot save your skin".into())
            }
            None => storage
                .remove_item(PROFILE_STORAGE)
                .map_err(|_| "Cannot clear your saved skin".into()),
        };
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = manifest;
        Err("This carrier has no personal skin storage".into())
    }
}

#[cfg(target_arch = "wasm32")]
fn profile_storage() -> Result<web_sys::Storage, String> {
    window()
        .local_storage()
        .ok()
        .flatten()
        .ok_or_else(|| "Personal skin storage is unavailable".into())
}
