//! The App operator and Forms maker author workbooks through the same session and page schema.

use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::query::{self, ViewDocumentFacts, ViewPartFacts};
use calc_skin::{
    AnchorId, Batch, Bound, Change, Coverage, DocId, InputCandidate, InputPolicy, InputSignal,
    InventoryItem, InventoryKind, ModelChange, Page, PageNode, PageSchema, PresentationChange,
    Refusal, ScenarioRef, SourceRef, Stamp, TableInfo, ViewId,
};
use dna_kit::commands::CommandId;
use dna_kit::page::{PageAuthor, PageInput, PageRenderer, PageScenario};
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
const REVIEW_SCENARIO: &str = "app.review-scenario";
const SAVE_SCENARIO: &str = "app.save-scenario";

mod controls;
mod printing;
mod rail;

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

    fn can_open_preferred(&self, part: &ViewPartFacts) -> bool {
        self.id == APP && app_document(part).is_some_and(|doc| decode(doc).is_ok())
    }

    fn commands(&self, session: Session) -> Vec<ViewCommand> {
        let state = State::new(session, self.id == FORMS);
        self.state.set(Some(state));
        let id = self.spec().id;
        if self.id == APP {
            // Drafts belong to the session owner, even when App fields are rebuilt.
            let _ = inputs(session);
            let _ = draft(session);
            let candidates = session.watch(query::InputCandidates { scope: SourceRef::Workbook });
            let mut commands = vec![
                ViewCommand::new(&id, REVIEW_SCENARIO, "Review saved scenario", &[], move |invocation| {
                    let scenario = serde_json::from_value(invocation.args.clone())
                        .map_err(|_| "Choose a saved worksheet scenario".to_owned())?;
                    state.review_scenario(scenario)
                }).when("writable"),
                ViewCommand::new(&id, SAVE_SCENARIO, "Save as scenario", &[], move |invocation| {
                    let name = invocation.args.as_str().map(str::trim)
                        .filter(|name| !name.is_empty())
                        .ok_or_else(|| "Enter a scenario name first".to_owned())?;
                    state.save_scenario(name)
                }).when("writable"),
                ViewCommand::new(&id, "app.reset-inputs", "Reset to opening inputs", &[], move |_| {
                    state.reset_inputs()
                }).when("writable"),
                ViewCommand::new(&id, "app.pin-baseline", "Pin current as baseline", &[], move |_| {
                    state.pin_baseline()
                }),
                ViewCommand::new(
                    &id,
                    "app.draft-from-file",
                    "Draft App from file",
                    &[],
                    move |_| state.draft(candidates),
                )
                .when("writable"),
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
            ];
            commands.extend(printing::commands(state, &id));
            commands
        } else {
            let mut commands = vec![
                ViewCommand::new(&id, "forms.add-scenario-picker", "Add scenario picker", &[], move |_| {
                    state.append_part(
                        "Add scenario picker",
                        PageNode::Part {
                            id: String::new(),
                            component: "scenario-picker".into(),
                            bind: BTreeMap::new(),
                            props: BTreeMap::from([("label".into(), "Scenarios".into())]),
                        },
                        None,
                    )
                }).when("writable"),
                ViewCommand::new(&id, "forms.create", "Create App page", &[], move |_| {
                    state.create()
                })
                .when("writable"),
                ViewCommand::new(&id, "forms.keep-draft", "Keep App draft", &[], move |_| {
                    state.keep_draft()
                })
                .when("writable"),
                ViewCommand::new(&id, "forms.discard-draft", "Discard App draft", &[], move |_| {
                    draft(session).set(None);
                    Ok(())
                }),
                ViewCommand::new(&id, "forms.prefer-app", "Open this workbook in App", &[], move |_| {
                    state.prefer_app(true)
                })
                .when("writable"),
                ViewCommand::new(&id, "forms.prefer-classic", "Open this workbook in Classic", &[], move |_| {
                    state.prefer_app(false)
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
                ViewCommand::new(&id, "forms.add-prose", "Add report prose", &[], move |invocation| {
                    let text = invocation.args.get("text").and_then(serde_json::Value::as_str)
                        .filter(|text| !text.trim().is_empty())
                        .ok_or_else(|| "Enter some report prose.".to_owned())?;
                    let link = invocation.args.get("bind_selected").and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    state.add_prose(text, link)
                }).when("writable"),
                ViewCommand::new(&id, "forms.add-excerpt", "Add notebook excerpt", &[], move |_| {
                    state.add_excerpt()
                }).when("writable"),
            ];
            commands.extend(controls::commands(state, &id));
            commands.extend(rail::commands(session, &id));
            commands.push(
                ViewCommand::new(&id, "forms.add-table", "Add table to App page", &[], move |invocation| {
                    let table = serde_json::from_value(invocation.args.clone())
                        .map_err(|_| "Choose a worksheet table.".to_owned())?;
                    state.add_table(table)
                }).when("writable"),
            );
            commands.push(
                ViewCommand::new(&id, "forms.add-chart", "Add chart to App page", &[], move |invocation| {
                    let chart = serde_json::from_value(invocation.args.clone())
                        .map_err(|_| "Choose a worksheet chart.".to_owned())?;
                    state.add_chart(chart)
                }).when("writable"),
            );
            commands
        }
    }

    fn cancel_interaction(&self, session: Session) -> bool {
        if self.id != APP || session.view().0 != APP {
            return false;
        }
        if printing::cancel(session) {
            return true;
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
    inventory: Option<Watch<query::Inventory>>,
}

impl State {
    fn new(session: Session, author: bool) -> Self {
        Self {
            session,
            inventory: author.then(|| session.watch(query::Inventory {
                kinds: vec![InventoryKind::Tables, InventoryKind::Charts],
            })),
        }
    }

    fn snapshot(self) -> Result<(Stamp, Option<ViewDocumentFacts>), String> {
        if let Live::Ready(answer) = self.session.view_part() {
            if !matches!(answer.coverage, Coverage::Complete) || answer.body.opaque {
                return Err("The workbook's view declarations are unavailable".into());
            }
            let Some(stamp) = self.session.stamp().filter(|now| {
                now.workbook == answer.stamp.workbook && now.view == answer.stamp.view
            }) else {
                return Err("Reading the current workbook pages…".into());
            };
            let document = app_document(&answer.body).cloned();
            Ok((stamp, document))
        } else {
            Err("Reading workbook pages…".into())
        }
    }

    fn document(self) -> Result<(Page, DocId), String> {
        let (_, doc) = self.snapshot()?;
        let doc = doc.ok_or_else(|| "No App is defined yet".to_owned())?;
        Ok((decode(&doc)?, doc.id))
    }

    fn prefer_app(self, preferred: bool) -> Result<(), String> {
        let (stamp, doc) = self.snapshot()?;
        if preferred {
            decode(&doc.ok_or_else(|| "Create an App page first".to_owned())?)?;
        }
        self.submit(
            stamp,
            if preferred { "Open in App" } else { "Open in Classic" },
            vec![PresentationChange::PreferView {
                view: preferred.then(|| ViewId(APP.into())),
                expect_view_rev: stamp.view,
            }],
        )
    }

    fn create(self) -> Result<(), String> {
        let stamp = self.unauthored()?;
        self.create_page(stamp, "Create App page", Vec::new(), Vec::new())
    }

    fn unauthored(self) -> Result<Stamp, String> {
        let (stamp, page) = self.snapshot()?;
        if page.is_some() {
            Err("This workbook already has an App page".into())
        } else {
            Ok(stamp)
        }
    }

    fn create_page(
        self,
        stamp: Stamp,
        label: &str,
        children: Vec<PageNode>,
        mut changes: Vec<PresentationChange>,
    ) -> Result<(), String> {
        let document = Page {
            schema: PageSchema::V1,
            title: "App".into(),
            input: InputPolicy::Live,
            root: PageNode::Stack { children },
        };
        // The workbook-level App has a reserved document ID.
        let doc = DocId(NonZeroU128::new(0x4150_5001).unwrap());
        changes.push(PresentationChange::CreateDoc {
            doc,
            expect_view_rev: Some(stamp.view),
            view: ViewId(APP.into()),
            sheet: None,
            schema: 1,
            body: encode(&document)?,
        });
        self.submit(stamp, label, changes)
    }

    fn draft(self, watch: Watch<query::InputCandidates>) -> Result<(), String> {
        let stamp = self.unauthored()?;
        let answer = match self.session.read(watch) {
            Live::Ready(answer) if answer.stamp.workbook == stamp.workbook
                && answer.coverage == Coverage::Complete => answer,
            Live::Refused(Refusal::OverBudget { .. }) => {
                return Err("This workbook is too large to draft automatically. Choose cells in Forms maker.".into());
            }
            Live::Refused(_) => return Err("Worksheet input signals are unavailable.".into()),
            _ => return Err("Reading worksheet input signals…".into()),
        };
        if answer.body.is_empty() {
            return Err("No certain App inputs were found in this workbook.".into());
        }
        draft(self.session).set(Some(Draft {
            stamp,
            candidates: answer.body,
        }));
        self.session
            .command("view.forms")
            .map_err(|_| "Forms maker is unavailable.".into())
    }

    fn keep_draft(self) -> Result<(), String> {
        let stamp = self.unauthored()?;
        let draft = current_draft(self.session)
            .ok_or_else(|| "Draft again from the current workbook first".to_owned())?;
        let Live::Ready(part) = self.session.view_part() else {
            return Err("Reading source anchors…".into());
        };
        let mut changes = Vec::new();
        let mut children = Vec::new();
        for (i, (candidate, anchor)) in draft
            .candidates
            .into_iter()
            .zip(unused_anchors(&part.body))
            .enumerate()
        {
            let at = candidate.at;
            let address = qualified(self.session, at)
                .ok_or_else(|| "A draft source is unavailable".to_owned())?;
            let label = candidate
                .label
                .filter(|label| !label.trim().is_empty())
                .unwrap_or_else(|| address.clone());
            children.push(bound_part(
                format!("cell-{i}"),
                "input",
                "value",
                Bound { anchor, address },
                label,
            ));
            changes.push(PresentationChange::Bind {
                anchor,
                source: cell_source(at),
            });
        }
        self.create_page(stamp, "Keep App draft", children, changes)
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
        let at = self
            .session
            .active()
            .ok_or_else(|| "Select a worksheet cell".to_owned())?;
        let formula = self
            .session
            .active_entry()
            .ok_or_else(|| "Reading the selected cell…".to_owned())?
            .formula;
        self.append_bound(
            "Add cell to App page",
            label,
            if formula { "value" } else { "input" },
            "value",
            Area(at, at),
        )
    }

    fn inventory(self) -> Result<Vec<InventoryItem>, String> {
        let inventory = self.inventory.expect("Forms reports the workbook's inventory");
        let Live::Ready(answer) = self.session.read(inventory) else {
            return Err("Reading worksheet objects…".into());
        };
        if self.session.stamp().is_none_or(|stamp| stamp.workbook != answer.stamp.workbook)
            || answer.coverage != Coverage::Complete
        {
            return Err("Reading the current workbook’s objects…".into());
        }
        Ok(answer.body)
    }

    fn add_table(self, table: TableInfo) -> Result<(), String> {
        if !self.inventory()?.iter().any(|item| matches!(item, InventoryItem::Table(known) if known == &table)) {
            return Err("Choose a current workbook table.".into());
        }
        self.append_bound("Add table to App page", &table.name, "data-frame", "rows", table.area)
    }

    fn add_chart(self, object: SourceRef) -> Result<(), String> {
        let SourceRef::Object { name, .. } = &object else {
            return Err("Choose a worksheet chart.".into());
        };
        if !self.inventory()?.iter().any(|item| matches!(item, InventoryItem::Chart { object: chart } if chart == &object)) {
            return Err("Choose a current workbook chart.".into());
        }
        let label = name.clone();
        let (bound, binding) = self.bind_source(object)?;
        self.append_part(
            "Add chart to App page",
            bound_part(String::new(), "chart", "object", bound, label),
            Some(binding),
        )
    }

    fn append_bound(
        self,
        action: &str,
        label: &str,
        component: &str,
        slot: &str,
        area: Area,
    ) -> Result<(), String> {
        let (bound, binding) = self.make_binding(area)?;
        self.append_part(
            action,
            bound_part(String::new(), component, slot, bound, label.to_owned()),
            Some(binding),
        )
    }

    fn add_prose(self, text: &str, link: bool) -> Result<(), String> {
        let mut bind = BTreeMap::new();
        let mut change = None;
        if link {
            if !text.contains("{=value}") {
                return Err("Put {=value} where the selected cell's result should appear.".into());
            }
            let at = self.session.active().ok_or_else(|| "Select a worksheet cell.".to_owned())?;
            let (bound, binding) = self.make_binding(Area(at, at))?;
            bind.insert("value".into(), bound);
            change = Some(binding);
        }
        self.append_part(
            "Add report prose",
            PageNode::Part {
                id: String::new(),
                component: "rich-prose".into(),
                bind,
                props: BTreeMap::from([
                    ("label".into(), "Report prose".into()),
                    ("text".into(), text.into()),
                ]),
            },
            change,
        )
    }

    fn add_excerpt(self) -> Result<(), String> {
        let selection = self.session.selection().ok_or_else(|| "Select notebook output cells.".to_owned())?;
        if let SourceRef::Cells { areas, .. } = selection.target
            && let [area] = areas.as_slice()
        {
            return self.append_bound("Add notebook excerpt", "Notebook excerpt", "notebook-excerpt", "rows", *area);
        }
        Err("Select one range of notebook output cells.".into())
    }

    fn make_binding(self, area: Area) -> Result<(Bound, PresentationChange), String> {
        self.bind_source(SourceRef::Cells {
            sheet: Sheet(area.0.0, area.0.1),
            areas: vec![area],
        })
    }

    fn bind_source(self, source: SourceRef) -> Result<(Bound, PresentationChange), String> {
        let address = source_address(self.session, &source)
            .ok_or_else(|| "The source worksheet is unavailable".to_owned())?;
        let Live::Ready(part) = self.session.view_part() else {
            return Err("Reading source anchors…".into());
        };
        let anchor = unused_anchors(&part.body).next()
            .expect("fewer anchors than u128 identifiers");
        Ok((Bound { anchor, address }, PresentationChange::Bind { anchor, source }))
    }

    fn append_part(
        self,
        action: &str,
        mut part: PageNode,
        binding: Option<PresentationChange>,
    ) -> Result<(), String> {
        let (stamp, current) = self.snapshot()?;
        let current = current.ok_or_else(|| "Create an App page first".to_owned())?;
        let mut page = decode(&current)?;
        let mut part_ids = Vec::new();
        page.root.for_each_part(&mut |id, _, _| part_ids.push(id.to_owned()));
        let PageNode::Part { id, .. } = &mut part else { unreachable!("appending a page control") };
        *id = (part_ids.len()..)
            .map(|n| format!("cell-{n}"))
            .find(|id| !part_ids.contains(id))
            .expect("fewer controls than usize identifiers");
        match &mut page.root {
            PageNode::Stack { children } => children.push(part),
            root => {
                let old = root.clone();
                *root = PageNode::Stack {
                    children: vec![old, part],
                };
            }
        }
        let mut changes: Vec<_> = binding.into_iter().collect();
        changes.push(PresentationChange::PatchDoc {
            doc: current.id,
            expect_view_rev: stamp.view,
            body: encode(&page)?,
        });
        self.submit(stamp, action, changes)
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
        let expects = changes
            .iter()
            .filter_map(|change| match change {
                PresentationChange::Bind { source, .. } => Some(source.clone()),
                _ => None,
            })
            .collect();
        self.session.submit(Batch {
            expects,
            ..batch(stamp, label, changes.into_iter().map(Change::Presentation).collect())
        });
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

    fn review_scenario(self, scenario: ScenarioRef) -> Result<(), String> {
        self.document()?;
        let label = format!("Show scenario {}", scenario.name);
        self.scenario_batch(&label, ModelChange::ShowScenario { scenario })
    }

    fn save_scenario(self, name: &str) -> Result<(), String> {
        let (page, _) = self.document()?;
        let at = self.session.active().ok_or_else(|| "Choose a worksheet for the scenario".to_owned())?;
        let scenario = ScenarioRef { sheet: Sheet(at.0, at.1), name: name.into() };
        let inputs = page_inputs(&page);
        self.scenario_batch(&format!("Save scenario {name}"), ModelChange::SaveScenario { scenario, inputs })
    }

    fn reset_inputs(self) -> Result<(), String> {
        let (page, _) = self.document()?;
        let stamp = self.session.stamp().ok_or_else(|| "Open a workbook first".to_owned())?;
        if !matches!(self.session.seam(), Seam::Idle) {
            return Err("Finish the change in progress first".into());
        }
        let bindings = page_inputs(&page);
        if bindings.is_empty() {
            return Err("This page has no worksheet inputs".into());
        }
        let pending = inputs(self.session);
        self.session.submit_then(batch(
            stamp,
            "Reset to opening inputs",
            vec![Change::Model(ModelChange::ResetInputs { inputs: bindings })],
        ), move || pending.set(Inputs::default()));
        Ok(())
    }

    fn pin_baseline(self) -> Result<(), String> {
        let (page, _) = self.document()?;
        let stamp = self.session.stamp().ok_or_else(|| "Open a workbook first".to_owned())?;
        let bounds = comparison_bindings(&page);
        if bounds.is_empty() {
            return Err("This page has no output values or charts".into());
        }
        self.session.operate(calc_skin::Operation::PinBaseline {
            expected: stamp,
            bounds,
            budget: dna_kit::PAGE_BUDGET,
        }, Some("Baseline pinned".into()));
        Ok(())
    }

    fn scenario_batch(self, label: &str, change: ModelChange) -> Result<(), String> {
        let stamp = self.session.stamp().ok_or_else(|| "Open a workbook first".to_owned())?;
        if !matches!(self.session.seam(), Seam::Idle) {
            return Err("Finish the change in progress first".into());
        }
        if inputs(self.session).with_untracked(|values| {
            values.current(self.session).is_some_and(|current| !current.values.is_empty())
        }) {
            return Err("Apply or discard the drafted inputs first".into());
        }
        self.session.submit_for_review(batch(
            stamp,
            label,
            vec![Change::Model(change)],
        ));
        Ok(())
    }
}

fn app_document(part: &ViewPartFacts) -> Option<&ViewDocumentFacts> {
    part.documents.iter().find(|doc| doc.sheet.is_none() && doc.view.0 == APP)
}

fn unused_anchors(part: &ViewPartFacts) -> impl Iterator<Item = AnchorId> + '_ {
    (1u128..)
        .map(|n| AnchorId(NonZeroU128::new(n).unwrap()))
        .filter(|id| part.anchors.iter().all(|anchor| anchor.id != *id))
}

fn cell_source(at: Key) -> SourceRef {
    SourceRef::Cells {
        sheet: Sheet(at.0, at.1),
        areas: vec![Area(at, at)],
    }
}

fn bound_part(id: String, component: &str, slot: &str, bound: Bound, label: String) -> PageNode {
    PageNode::Part {
        id,
        component: component.to_owned(),
        bind: BTreeMap::from([(slot.into(), bound)]),
        props: BTreeMap::from([("label".into(), serde_json::Value::String(label))]),
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

fn page_inputs(page: &Page) -> Vec<Bound> {
    let mut inputs = Vec::new();
    page.root.for_each_part(&mut |_, component, bindings| {
        if component == "input" && let Some(bound) = bindings.get("value") {
            inputs.push(bound.clone());
        }
    });
    inputs
}

fn comparison_bindings(page: &Page) -> Vec<Bound> {
    let mut bounds = Vec::new();
    page.root.for_each_part(&mut |_, component, bindings| {
        let slot = match component {
            "value" => "value",
            "chart" => "object",
            _ => return,
        };
        if let Some(bound) = bindings.get(slot) {
            bounds.push(bound.clone());
        }
    });
    bounds
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
    source_address(session, &cell_source(at))
}

fn source_address(session: Session, source: &SourceRef) -> Option<String> {
    let sheets = session
        .summary()?
        .sheets
        .into_iter()
        .map(|sheet| (sheet.sheet, sheet.name))
        .collect();
    source.display(&calc_skin::AddressBook { sheets, ..Default::default() })
}

#[derive(Clone)]
struct Draft {
    stamp: Stamp,
    candidates: Vec<InputCandidate>,
}

#[derive(Clone, Copy)]
struct DraftState(RwSignal<Option<Draft>>);

impl Default for DraftState {
    fn default() -> Self {
        Self(RwSignal::new(None))
    }
}

fn draft(session: Session) -> RwSignal<Option<Draft>> {
    session.local::<DraftState>().0
}

fn current_draft(session: Session) -> Option<Draft> {
    let stamp = session.stamp()?;
    draft(session).get().filter(|draft| {
        stamp.workbook == draft.stamp.workbook
            && stamp.model == draft.stamp.model
            && stamp.view == draft.stamp.view
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
    let printing = printing::showing(session);
    view! {
        <div class="dna-page-surface" data-dnacalc="app">
            {move || match page.get() {
                Ok((document, _)) => {
                    let batched = document.input == InputPolicy::Batched;
                    let has_inputs = !page_inputs(&document).is_empty();
                    let has_comparison = !comparison_bindings(&document).is_empty();
                    view! {
                        <header class="dna-page-tools">
                            {button(session, "view.forms", "Edit page…")}
                            {button(session, "view.classic", "See the cells")}
                            {has_inputs.then(|| button(session, "app.reset-inputs", "Reset to opening inputs"))}
                            {has_comparison.then(|| button(session, "app.pin-baseline", "Pin current as baseline"))}
                            {printing::open_button(session)}
                        </header>
                        {move || {
                            let document = document.clone();
                            if printing.get() {
                                printing::stage(session, document).into_any()
                            } else {
                                view! {
                                    <PageRenderer session document input=INPUT compare=true scenario=PageScenario {
                                        review: REVIEW_SCENARIO,
                                        save: SAVE_SCENARIO,
                                    } />
                                }.into_any()
                            }
                        }}
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
                        <h1>{message.clone()}</h1>
                        {if message == "No App is defined yet" {
                            button(session, "app.draft-from-file", "Draft from file · Edit page…").into_any()
                        } else {
                            ().into_any()
                        }}
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
    let draft = move || current_draft(session);
    let title = NodeRef::<leptos::html::Input>::new();
    let label = NodeRef::<leptos::html::Input>::new();
    view! {
        <div class="dna-page-surface" data-dnacalc="forms">
            <header class="dna-page-tools">
                <h1>"Forms maker"</h1>
                {button(session, "view.app", "Run App")}
                {button(session, "forms.toggle-picker", "Classic Companion")}
            </header>
            {move || match page.get() {
                Ok((document, id)) => {
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
                            {move || {
                                let prefer_app = matches!(session.view_part(), Live::Ready(answer)
                                    if answer.body.preferred.as_ref().is_some_and(|view| view.0 == APP));
                                if prefer_app {
                                    button(session, "forms.prefer-classic", "Open this workbook in Classic")
                                } else {
                                    button(session, "forms.prefer-app", "Open this workbook in App")
                                }
                            }}
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
                        {reader_tools(session)}
                        {button(session, "forms.add-scenario-picker", "Add scenario picker")}
                        <div class="dna-page-author-layout">
                            {rail::rail(session, state.inventory.expect("Forms has an inventory watch"))}
                            {controls::inspector(session, id, document.clone())}
                            <PageRenderer session document author=PageAuthor {
                                document: id,
                                select_command: controls::SELECT,
                                selected: controls::is_selected,
                            } />
                        </div>
                    }.into_any()
                }
                Err(message) => match draft() {
                    Some(draft) => draft_review(session, draft),
                    None => view! {
                        <p>{message}</p>
                        {button(session, "forms.create", "Create App page")}
                    }.into_any(),
                },
            }}
            {rail::companion(session)}
        </div>
    }.into_any()
}

fn reader_tools(session: Session) -> impl IntoView {
    let prose = NodeRef::<leptos::html::Textarea>::new();
    let link = NodeRef::<leptos::html::Input>::new();
    view! {
        <details class="dna-page-card">
            <summary>"Reader content"</summary>
            <form on:submit=move |event: leptos::ev::SubmitEvent| {
                event.prevent_default();
                if let Some(text) = prose.get() {
                    let _ = session.run(&Invocation {
                        command: CommandId::new("forms.add-prose"),
                        args: serde_json::json!({
                            "text": text.value(),
                            "bind_selected": link.get().is_some_and(|input| input.checked()),
                        }),
                    });
                }
            }>
                <label>"Report prose" <textarea node_ref=prose required/></label>
                <label>
                    <input type="checkbox" node_ref=link/>
                    "Show the selected cell’s value at {=value}"
                </label>
                <button type="submit" data-command="forms.add-prose">"Add report prose"</button>
            </form>
            {button(session, "forms.add-excerpt", "Add selected notebook outputs")}
        </details>
    }
}

fn draft_review(session: Session, draft: Draft) -> AnyView {
    view! {
        <section class="dna-page-card" data-dnacalc="app-draft">
            <h2>"Draft from file"</h2>
            <ul>
                {draft.candidates.into_iter().map(|candidate| {
                    let address = qualified(session, candidate.at);
                    let reason = match candidate.signal {
                        InputSignal::ValidationList => "Validation list",
                        InputSignal::UnlockedOnProtectedSheet => "Unlocked on protected sheet",
                        InputSignal::LinkedFormControl => "Linked Form Control",
                    };
                    view! { <li><strong>{address}</strong>" · "{reason}</li> }
                }).collect_view()}
            </ul>
            {button(session, "forms.keep-draft", "Keep draft")}
            {button(session, "forms.discard-draft", "Discard draft")}
        </section>
    }.into_any()
}

fn argument(session: Session, command: &'static str, text: String) {
    let _ = session.run(&Invocation {
        command: CommandId::new(command),
        args: serde_json::Value::String(text),
    });
}
