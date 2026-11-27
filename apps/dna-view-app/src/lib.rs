//! The App operator and Forms maker author workbooks through the same session and page schema.

use calc_skin::grid::{Key, Sheet};
use calc_skin::query::{self, ViewDocumentFacts};
use calc_skin::{
    AnchorId, Batch, Bound, Change, Coverage, DocId, InputPolicy, ModelChange, Page, PageNode,
    PageSchema, PresentationChange, Refusal, SourceRef, Stamp, ViewId, address,
};
use dna_kit::commands::CommandId;
use dna_kit::page::{PageInput, PageRenderer};
use dna_kit::{
    Invocation, Live, Located, MountedView, Registry, Seam, Session, View, ViewCommand, ViewSpec,
    Watch,
};
use leptos::prelude::*;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::num::NonZeroU128;

const APP: &str = "app";
const FORMS: &str = "forms";

pub fn install(registry: &mut Registry) {
    registry.view(Surface::new(APP, "App"));
    registry.view(Surface::new(FORMS, "Forms maker"));
}

struct Surface {
    id: &'static str,
    title: &'static str,
    state: Cell<Option<State>>,
}

impl Surface {
    fn new(id: &'static str, title: &'static str) -> Self {
        Self {
            id,
            title,
            state: Cell::new(None),
        }
    }
}

impl View for Surface {
    fn spec(&self) -> ViewSpec {
        ViewSpec {
            id: ViewId(self.id.to_owned()),
            title: self.title,
            profile: "excel-365-win-en-us",
        }
    }

    fn cell_editor(&self) -> bool {
        false
    }

    fn commands(&self, session: Session) -> Vec<ViewCommand> {
        let state = State::new(session);
        self.state.set(Some(state));
        let id = self.spec().id;
        if self.id == APP {
            // Drafts belong to the session owner, even when App fields are rebuilt.
            let _ = inputs(session);
            vec![
                ViewCommand::new(
                    &id,
                    "app.draft-input",
                    "Draft an input",
                    &[],
                    move |invocation| {
                        let (bound, text): (Bound, String) =
                            serde_json::from_value(invocation.args.clone())
                                .map_err(|_| "Choose a page input and enter a value".to_owned())?;
                        let (doc, _) = state.document()?;
                        if !permits(&doc, &bound) {
                            return Err("The source is not a page input".into());
                        }
                        inputs(session).update(|inputs| {
                            if inputs.current(session).is_none() || inputs.values.is_empty() {
                                inputs.values.clear();
                                inputs.stamp = session.stamp();
                            }
                            inputs.values.insert(bound.anchor, (bound, text));
                        });
                        Ok(())
                    },
                )
                .when("writable"),
                ViewCommand::new(
                    &id,
                    "app.commit-input",
                    "Apply this input",
                    &[],
                    move |invocation| {
                        let bound = serde_json::from_value(invocation.args.clone())
                            .map_err(|_| "Choose a page input".to_owned())?;
                        let (doc, _) = state.document()?;
                        if doc.input == InputPolicy::Live {
                            state.apply(Some(&bound))?;
                        }
                        Ok(())
                    },
                )
                .when("writable"),
                ViewCommand::new(&id, "app.apply-inputs", "Apply inputs", &[], move |_| {
                    let (doc, _) = state.document()?;
                    if doc.input != InputPolicy::Batched {
                        return Err("This page applies inputs live".into());
                    }
                    state.apply(None)
                })
                .when("writable"),
                ViewCommand::new(
                    &id,
                    "app.discard-inputs",
                    "Discard input drafts",
                    &[],
                    move |_| {
                        inputs(session).set(Inputs::default());
                        Ok(())
                    },
                ),
            ]
        } else {
            vec![
                ViewCommand::new(&id, "forms.create", "Create App page", &[], move |_| {
                    state.create()
                })
                .when("writable"),
                ViewCommand::new(
                    &id,
                    "forms.title",
                    "Set page title",
                    &[],
                    move |invocation| {
                        let title = invocation
                            .args
                            .as_str()
                            .map(str::trim)
                            .filter(|title| !title.is_empty())
                            .ok_or_else(|| "Give the page a title".to_owned())?;
                        state.patch("Set page title", |document| {
                            document.title = title.to_owned()
                        })
                    },
                )
                .when("writable"),
                ViewCommand::new(
                    &id,
                    "forms.toggle-input",
                    "Change input behaviour",
                    &[],
                    move |_| {
                        if !inputs(session).with(|inputs| {
                            inputs
                                .current(session)
                                .is_some_and(|inputs| !inputs.values.is_empty())
                        }) {
                            return state.patch("Change input behaviour", |document| {
                                document.input = match document.input {
                                    InputPolicy::Live => InputPolicy::Batched,
                                    InputPolicy::Batched => InputPolicy::Live,
                                };
                            });
                        }
                        Err("Apply or discard the input drafts in App first".into())
                    },
                )
                .when("writable"),
                ViewCommand::new(
                    &id,
                    "forms.add-cell",
                    "Add selected cell to page",
                    &[],
                    move |invocation| {
                        let label = invocation
                            .args
                            .as_str()
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                            .ok_or_else(|| "Give the page control a label".to_owned())?;
                        state.add_cell(label)
                    },
                )
                .when("writable"),
            ]
        }
    }

    fn cancel_interaction(&self, session: Session) -> bool {
        if self.id != APP || session.view().0 != APP {
            return false;
        }
        let current = inputs(session);
        let focused = current.with_untracked(|inputs| inputs.focused.clone());
        if let Some(bound) = focused {
            let changed =
                current.with_untracked(|inputs| inputs.values.contains_key(&bound.anchor));
            if changed {
                current.update(|inputs| {
                    inputs.values.remove(&bound.anchor);
                });
                return true;
            }
        }
        false
    }

    fn mount(&self, session: Session) -> MountedView {
        let state = self.state.get().expect("commands installed before mount");
        MountedView {
            body: if self.id == APP {
                operator(session, state)
            } else {
                forms(session, state)
            },
            locate: Box::new(|_| Located::Absent),
            reveal: Box::new(|_| ()),
        }
    }
}

#[derive(Clone, Copy)]
struct State {
    session: Session,
    part: Watch<query::ViewPartOf>,
}

impl State {
    fn new(session: Session) -> Self {
        Self {
            session,
            part: session.watch(query::ViewPartOf),
        }
    }

    fn snapshot(self) -> Result<(Stamp, Option<ViewDocumentFacts>), String> {
        if let Live::Ready(answer) = self.session.read(self.part) {
            if !matches!(answer.coverage, Coverage::Complete) || answer.body.opaque {
                return Err("The workbook's view declarations are unavailable".into());
            }
            if self.session.stamp().is_none_or(|now| {
                now.workbook != answer.stamp.workbook || now.view != answer.stamp.view
            }) {
                return Err("Reading the current workbook pages…".into());
            }
            let document = answer
                .body
                .documents
                .into_iter()
                .find(|doc| doc.sheet.is_none() && doc.view.0 == APP);
            Ok((answer.stamp, document))
        } else {
            Err("Reading workbook pages…".into())
        }
    }

    fn document(self) -> Result<(Page, DocId), String> {
        let (_, doc) = self.snapshot()?;
        let doc = doc.ok_or_else(|| "No App is defined yet".to_owned())?;
        Ok((decode(&doc)?, doc.id))
    }

    fn create(self) -> Result<(), String> {
        let (stamp, current) = self.snapshot()?;
        if current.is_some() {
            return Err("This workbook already has an App page".into());
        }
        let document = Page {
            schema: PageSchema::V1,
            title: "App".into(),
            input: InputPolicy::Live,
            root: PageNode::Stack {
                children: Vec::new(),
            },
        };
        // The workbook-level App has a reserved document ID.
        let doc = DocId(NonZeroU128::new(0x4150_5001).unwrap());
        self.submit(
            stamp,
            "Create App page",
            vec![PresentationChange::CreateDoc {
                doc,
                view: ViewId(APP.into()),
                sheet: None,
                schema: 1,
                body: encode(&document)?,
            }],
        )
    }

    fn patch(self, label: &str, edit: impl FnOnce(&mut Page)) -> Result<(), String> {
        let (stamp, current) = self.snapshot()?;
        let current = current.ok_or_else(|| "Create an App page first".to_owned())?;
        let mut page = decode(&current)?;
        edit(&mut page);
        self.submit(
            stamp,
            label,
            vec![PresentationChange::PatchDoc {
                doc: current.id,
                expect_view_rev: stamp.view,
                body: encode(&page)?,
            }],
        )
    }

    fn add_cell(self, label: &str) -> Result<(), String> {
        let (stamp, current) = self.snapshot()?;
        let current = current.ok_or_else(|| "Create an App page first".to_owned())?;
        let mut page = decode(&current)?;
        let at = self
            .session
            .active()
            .ok_or_else(|| "Select a worksheet cell".to_owned())?;
        let address = qualified(self.session, at)
            .ok_or_else(|| "The source worksheet is unavailable".to_owned())?;
        let formula = self
            .session
            .active_entry()
            .ok_or_else(|| "Reading the selected cell…".to_owned())?
            .formula;
        let source = SourceRef::Cells {
            sheet: Sheet(at.0, at.1),
            areas: vec![calc_skin::grid::Area(at, at)],
        };
        let Live::Ready(part) = self.session.read(self.part) else {
            return Err("Reading source anchors…".into());
        };
        let mut part_ids = Vec::new();
        page.root
            .for_each_part(&mut |id, _, _| part_ids.push(id.to_owned()));
        let anchor = (1u128..)
            .map(|n| AnchorId(NonZeroU128::new(n).unwrap()))
            .find(|id| part.body.anchors.iter().all(|anchor| anchor.id != *id))
            .expect("fewer anchors than u128 identifiers");
        let id = (part_ids.len()..)
            .map(|n| format!("cell-{n}"))
            .find(|id| !part_ids.contains(id))
            .expect("fewer controls than usize identifiers");
        let part = PageNode::Part {
            id,
            component: if formula { "value" } else { "input" }.to_owned(),
            bind: BTreeMap::from([("value".into(), Bound { anchor, address })]),
            props: BTreeMap::from([("label".into(), serde_json::Value::String(label.to_owned()))]),
        };
        match &mut page.root {
            PageNode::Stack { children } => children.push(part),
            root => {
                let old = root.clone();
                *root = PageNode::Stack {
                    children: vec![old, part],
                };
            }
        }
        self.submit(
            stamp,
            "Add cell to App page",
            vec![
                PresentationChange::Bind { anchor, source },
                PresentationChange::PatchDoc {
                    doc: current.id,
                    expect_view_rev: stamp.view,
                    body: encode(&page)?,
                },
            ],
        )
    }

    fn submit(
        self,
        stamp: Stamp,
        label: &str,
        changes: Vec<PresentationChange>,
    ) -> Result<(), String> {
        if !matches!(self.session.seam(), Seam::Idle) {
            return Err("Finish the change in progress first".into());
        }
        self.session.submit(batch(
            stamp,
            label,
            changes.into_iter().map(Change::Presentation).collect(),
        ));
        Ok(())
    }

    fn apply(self, one: Option<&Bound>) -> Result<(), String> {
        let (document, _) = self.document()?;
        let state = inputs(self.session);
        let Some((stamp, entries)) = state.with_untracked(|inputs| {
            let inputs = inputs.current(self.session)?;
            let entries = inputs
                .values
                .values()
                .filter(|(bound, _)| one.is_none_or(|pick| pick == bound))
                .cloned()
                .collect::<Vec<_>>();
            Some((inputs.stamp?, entries))
        }) else {
            return Ok(());
        };
        if entries.is_empty() {
            return Ok(());
        }
        if entries.iter().any(|(bound, _)| !permits(&document, bound)) {
            return Err("The page inputs changed. Discard the draft and try again".into());
        }
        if !matches!(self.session.seam(), Seam::Idle) {
            return Err("Finish the change in progress first".into());
        }
        let changes = entries
            .iter()
            .map(|(bound, text)| {
                Change::Model(ModelChange::EnterBound {
                    bound: bound.clone(),
                    text: text.clone(),
                })
            })
            .collect();
        self.session
            .submit_then(batch(stamp, "Apply App inputs", changes), move || {
                state.update(|inputs| {
                    for (bound, text) in entries {
                        if inputs
                            .values
                            .get(&bound.anchor)
                            .is_some_and(|(current, value)| current == &bound && value == &text)
                        {
                            inputs.values.remove(&bound.anchor);
                        }
                    }
                    if inputs.values.is_empty() {
                        inputs.stamp = None;
                    }
                });
            });
        Ok(())
    }
}

fn decode(doc: &ViewDocumentFacts) -> Result<Page, String> {
    if doc.schema != 1 {
        return Err("This App page uses a newer schema".into());
    }
    serde_json::from_value(doc.body.clone())
        .map_err(|err| format!("Cannot read the App page ({err})"))
}

fn encode(page: &Page) -> Result<serde_json::Value, String> {
    serde_json::to_value(page).map_err(|err| err.to_string())
}

fn permits(page: &Page, bound: &Bound) -> bool {
    let mut permitted = false;
    page.root.for_each_part(&mut |_, component, bind| {
        permitted |= component == "input" && bind.get("value") == Some(bound);
    });
    permitted
}

fn batch(stamp: Stamp, label: &str, changes: Vec<Change>) -> Batch {
    Batch {
        id: String::new(),
        workbook: Some(stamp.workbook),
        base: Some(stamp.model),
        expects: Vec::new(),
        selection_seq: None,
        label: Some(label.to_owned()),
        changes,
    }
}

fn qualified(session: Session, at: Key) -> Option<String> {
    session
        .summary()?
        .sheets
        .into_iter()
        .find(|sheet| sheet.sheet == Sheet(at.0, at.1))
        .map(|sheet| {
            format!(
                "{}!{}",
                address::quoted_sheet(&sheet.name),
                address::address(at.2, at.3)
            )
        })
}

#[derive(Clone, Default)]
struct Inputs {
    stamp: Option<Stamp>,
    focused: Option<Bound>,
    values: BTreeMap<AnchorId, (Bound, String)>,
}

impl Inputs {
    fn current(&self, session: Session) -> Option<&Self> {
        let stamp = self.stamp?;
        (session.stamp()?.workbook == stamp.workbook).then_some(self)
    }
}

#[derive(Clone, Copy)]
struct InputState(RwSignal<Inputs>);

impl Default for InputState {
    fn default() -> Self {
        Self(RwSignal::new(Inputs::default()))
    }
}

fn inputs(session: Session) -> RwSignal<Inputs> {
    session.local::<InputState>().0
}

const INPUT: PageInput = PageInput {
    command: "app.commit-input",
    draft_command: "app.draft-input",
    draft: |session, bound| {
        inputs(session).with(|inputs| {
            inputs
                .current(session)?
                .values
                .get(&bound.anchor)
                .filter(|(current, _)| current == bound)
                .map(|(_, text)| text.clone())
        })
    },
    focus: |session, bound| inputs(session).update(|inputs| inputs.focused = bound),
    error: |session, bound, at| {
        let Refusal::Validation { target, reason } = session.last_refusal()? else {
            return None;
        };
        let SourceRef::Cells { areas, .. } = target else {
            return None;
        };
        let [area] = areas.as_slice() else {
            return None;
        };
        if area.0 != at || area.1 != at {
            return None;
        }
        (INPUT.draft)(session, bound).map(|_| reason)
    },
};

fn button(session: Session, command: &'static str, label: &'static str) -> impl IntoView {
    view! { <button type="button" data-command=command on:click=move |_| {
        let _ = session.command(command);
    }>{label}</button> }
}

fn operator(session: Session, state: State) -> AnyView {
    let page = Memo::new(move |_| state.document());
    view! {
        <div class="dna-page-surface" data-dnacalc="app">
            {move || match page.get() {
                Ok((document, _)) => {
                    let batched = document.input == InputPolicy::Batched;
                    view! {
                        <header class="dna-page-tools">
                            {button(session, "view.forms", "Edit page…")}
                            {button(session, "view.classic", "See the cells")}
                        </header>
                        <PageRenderer session document input=INPUT />
                        {batched.then(|| view! {
                            <footer class="dna-page-tools">
                                {button(session, "app.apply-inputs", "Apply inputs")}
                                {button(session, "app.discard-inputs", "Discard")}
                            </footer>
                        })}
                        <Show when=move || inputs(session).with(|drafts| drafts.current(session).is_some_and(|drafts| !drafts.values.is_empty()))>
                            <p role="status">"Previous valid inputs"</p>
                        </Show>
                    }.into_any()
                }
                Err(message) => view! {
                    <section class="dna-page-card">
                        <h1>{message}</h1>
                        {button(session, "view.forms", "Open Forms maker")}
                        {button(session, "view.classic", "Open Classic")}
                    </section>
                }.into_any(),
            }}
        </div>
    }.into_any()
}

fn forms(session: Session, state: State) -> AnyView {
    let page = Memo::new(move |_| state.document());
    let title = NodeRef::<leptos::html::Input>::new();
    let label = NodeRef::<leptos::html::Input>::new();
    view! {
        <div class="dna-page-surface" data-dnacalc="forms">
            <header class="dna-page-tools">
                <h1>"Forms maker"</h1>
                {button(session, "view.app", "Run App")}
            </header>
            {move || match page.get() {
                Ok((document, _)) => {
                    let input_mode = match document.input { InputPolicy::Live => "Live", InputPolicy::Batched => "Batched" };
                    view! {
                        <div class="dna-page-tools">
                            <form on:submit=move |event: leptos::ev::SubmitEvent| {
                                event.prevent_default();
                                if let Some(input) = title.get() { argument(session, "forms.title", input.value()); }
                            }>
                                <label>"Page title " <input node_ref=title required prop:value=document.title.clone()/></label>
                                <button type="submit" data-command="forms.title">"Save title"</button>
                            </form>
                            <p>{input_mode}</p>
                            {button(session, "forms.toggle-input", "Switch input mode")}
                        </div>
                        <aside class="dna-page-card">
                            <h2>"Add a worksheet cell"</h2>
                            <p>{move || session.active().and_then(|at| qualified(session, at))}</p>
                            {button(session, "view.classic", "Choose in Classic")}
                            <form on:submit=move |event: leptos::ev::SubmitEvent| {
                                event.prevent_default();
                                if let Some(input) = label.get() { argument(session, "forms.add-cell", input.value()); }
                            }>
                                <label>"Control label " <input node_ref=label required/></label>
                                <button type="submit" data-command="forms.add-cell">"Add selected cell"</button>
                            </form>
                        </aside>
                        <PageRenderer session document />
                    }.into_any()
                }
                Err(message) => view! {
                    <p>{message}</p>
                    {button(session, "forms.create", "Create App page")}
                }.into_any(),
            }}
        </div>
    }.into_any()
}

fn argument(session: Session, command: &'static str, text: String) {
    let _ = session.run(&Invocation {
        command: CommandId::new(command),
        args: serde_json::Value::String(text),
    });
}
