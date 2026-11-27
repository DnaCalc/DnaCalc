//! The ordered computation chain over the workbook session.

mod commands;
mod stage;

use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::query::{self, ViewDocumentFacts};
use calc_skin::{AnchorId, Answer, DocId, Selection, SourceRef, Stamp, Target, ViewId};
use dna_kit::{
    Following, Live, Located, MountedView, Registry, Session, View, ViewCommand, ViewSpec, Watch,
};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::num::NonZeroU128;

pub const ID: &str = "notebook";
const SCHEMA: u32 = 1;

#[derive(Default)]
struct Notebook {
    state: Cell<Option<State>>,
}

impl View for Notebook {
    fn spec(&self) -> ViewSpec {
        ViewSpec {
            id: ViewId(ID.into()),
            title: "Notebook",
            profile: "jupyter",
        }
    }

    fn commands(&self, session: Session) -> Vec<ViewCommand> {
        session.enable_notebook_keys(self.spec().id);
        let state = State::new(session);
        self.state.set(Some(state));
        commands::install(state)
    }

    fn mount(&self, _session: Session) -> MountedView {
        let state = self.state.get().expect("Notebook commands installed");
        MountedView {
            body: stage::mount(state).into_any(),
            locate: Box::new(move |selection| state.locate(selection)),
            reveal: Box::new(move |target| {
                if let Target::Anchor(id) = target {
                    stage::reveal(*id);
                }
                if let Target::Source(source) = target {
                    if let Ok(snapshot) = state.snapshot() {
                        if let Some(anchor) = snapshot
                            .part
                            .body
                            .anchors
                            .iter()
                            .find(|anchor| anchor.source.as_ref() == Some(source))
                        {
                            stage::reveal(anchor.id);
                        }
                    }
                }
            }),
        }
    }
}

pub fn install(registry: &mut Registry) {
    registry.view(Notebook::default());
}

#[derive(Clone, Copy)]
struct State {
    session: Session,
    part: Watch<query::ViewPartOf>,
    item: RwSignal<Option<(calc_skin::WorkbookId, AnchorId)>>,
    workspace: StoredValue<Workspace, LocalStorage>,
    draft: RwSignal<Option<calc_skin::WorkbookId>>,
    structure: Following<query::StructureOf>,
    prose_editor: RwSignal<Option<(calc_skin::WorkbookId, AnchorId)>>,
    tree: RwSignal<Option<(calc_skin::WorkbookId, AnchorId)>>,
    rows: RwSignal<BTreeMap<(calc_skin::WorkbookId, AnchorId), RowControls>>,
}

#[derive(Default)]
struct Workspace {
    owner: Option<calc_skin::WorkbookId>,
    copied: Option<CopiedCell>,
    delete_armed: Option<(calc_skin::WorkbookId, AnchorId)>,
}

#[derive(Clone)]
struct CopiedCell {
    kind: Kind,
    binding: Option<SourceRef>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Chain {
    #[serde(default)]
    layout: Layout,
    cells: Vec<Item>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Layout {
    #[default]
    Vertical,
    Pipeline,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Item {
    id: AnchorId,
    #[serde(flatten)]
    kind: Kind,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    folded: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    line_numbers: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum Kind {
    Formula,
    Table { name: Option<String> },
    Prose { markdown: String },
}

#[derive(Clone, Copy)]
struct RowControls {
    offset: u32,
    limit: u32,
    sort: Option<calc_skin::RowSort>,
}

impl Default for RowControls {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 20,
            sort: None,
        }
    }
}

impl Item {
    fn new(id: AnchorId, kind: Kind) -> Self {
        Self {
            id,
            kind,
            folded: false,
            line_numbers: false,
        }
    }
}

struct Snapshot {
    part: Answer<query::ViewPartFacts>,
    doc: Option<DocId>,
    chain: Chain,
}

impl State {
    fn new(session: Session) -> Self {
        Self {
            session,
            part: session.watch(query::ViewPartOf),
            item: RwSignal::new(None),
            workspace: StoredValue::new_local(Workspace::default()),
            draft: RwSignal::new(None),
            structure: session.following(),
            prose_editor: RwSignal::new(None),
            tree: RwSignal::new(None),
            rows: RwSignal::new(BTreeMap::new()),
        }
    }

    fn snapshot(self) -> Result<Snapshot, String> {
        let part = match self.session.read(self.part) {
            Live::Ready(part) => part,
            Live::Refused(reason) => {
                return Err(format!("Could not read the Notebook: {reason:?}"));
            }
            _ => return Err("Loading Notebook…".into()),
        };
        if part.body.opaque {
            return Err("This version of DNA Calc cannot read the workbook’s views.".into());
        }
        let document = part
            .body
            .documents
            .iter()
            .find(|doc| doc.view.0 == ID && doc.sheet.is_none());
        let (doc, chain) = match document {
            Some(ViewDocumentFacts {
                id,
                schema: SCHEMA,
                body,
                ..
            }) => {
                let chain: Chain = serde_json::from_value(body.clone())
                    .map_err(|error| format!("Could not read the Notebook: {error}"))?;
                (Some(*id), chain)
            }
            Some(_) => return Err("This Notebook was written in a different format.".into()),
            None => (None, Chain::default()),
        };
        Ok(Snapshot { part, doc, chain })
    }

    fn structure(self) -> Option<Vec<(Kind, SourceRef)>> {
        let Live::Ready(answer) = self.structure.at(Some(query::StructureOf {
            scope: SourceRef::Workbook,
        })) else {
            return None;
        };
        let (calc_skin::Evidence::Derived { body, .. }
        | calc_skin::Evidence::Observed { body, .. }) = answer.body
        else {
            return None;
        };
        let tables = body.tables.into_iter().map(|table| {
            (
                Kind::Table {
                    name: Some(table.name),
                },
                source_area(table.area),
            )
        });
        let runs = body
            .runs
            .into_iter()
            .flat_map(|run| run.members)
            .map(|member| {
                let kind = if member.0 == member.1 {
                    Kind::Formula
                } else {
                    Kind::Table { name: None }
                };
                (kind, source_area(member))
            });
        Some(tables.chain(runs).take(100).collect())
    }

    fn row_controls(self, id: AnchorId) -> RowControls {
        self.session
            .stamp()
            .and_then(|stamp| {
                self.rows
                    .with(|rows| rows.get(&(stamp.workbook, id)).copied())
            })
            .unwrap_or_default()
    }

    fn change_rows(
        self,
        id: AnchorId,
        change: impl FnOnce(&mut RowControls),
    ) -> Result<(), String> {
        let stamp = self.session.stamp().ok_or("Open a workbook first.")?;
        self.rows.update(|rows| {
            rows.retain(|(book, _), _| *book == stamp.workbook);
            change(rows.entry((stamp.workbook, id)).or_default());
        });
        Ok(())
    }

    fn locate(self, selection: &Selection) -> Located {
        let Ok(snapshot) = self.snapshot() else {
            return Located::Absent;
        };
        if selection.active.is_some_and(|at| {
            snapshot.chain.cells.iter().any(|item| {
                snapshot
                    .anchor(item.id)
                    .and_then(|anchor| anchor.area)
                    .and_then(|area| area.area())
                    .is_some_and(|area| at.cell().is_some_and(|at| area.contains(at)))
            })
        }) {
            Located::Exact
        } else {
            Located::Absent
        }
    }

    fn select(self, stamp: Stamp, id: AnchorId, source: Option<&SourceRef>) {
        self.item.set(Some((stamp.workbook, id)));
        self.workspace
            .update_value(|state| state.delete_armed = None);
        if let Some(at) = source.and_then(first_cell) {
            if self.session.active() != Some(at) {
                self.session.select_cell(at);
            }
        }
    }

    fn selected(self, snapshot: &Snapshot) -> Option<usize> {
        self.item
            .get()
            .and_then(|(workbook, id)| {
                (workbook == snapshot.part.stamp.workbook)
                    .then(|| snapshot.chain.cells.iter().position(|item| item.id == id))
                    .flatten()
            })
            .or_else(|| {
                let active = self.session.active();
                snapshot.chain.cells.iter().position(|item| {
                    active.is_some() && snapshot.source(item.id).and_then(first_cell) == active
                })
            })
            .or_else(|| (!snapshot.chain.cells.is_empty()).then_some(0))
    }
}

impl Snapshot {
    fn anchor(&self, id: AnchorId) -> Option<&query::AnchorFacts> {
        self.part.body.anchors.iter().find(|anchor| anchor.id == id)
    }

    fn source(&self, id: AnchorId) -> Option<&SourceRef> {
        self.anchor(id)?.source.as_ref()
    }

    fn next_id(&self) -> Result<NonZeroU128, String> {
        self.chain
            .cells
            .iter()
            .map(|item| item.id.0.get())
            .chain(
                self.part
                    .body
                    .anchors
                    .iter()
                    .map(|anchor| anchor.id.0.get()),
            )
            .chain(self.part.body.documents.iter().map(|doc| doc.id.0.get()))
            .max()
            .unwrap_or_default()
            .checked_add(1)
            .and_then(NonZeroU128::new)
            .ok_or_else(|| "The workbook has no available Notebook item ID.".into())
    }
}

fn first_cell(source: &SourceRef) -> Option<Key> {
    match source {
        SourceRef::Cells { areas, .. } => areas.first().map(|area| area.0),
        SourceRef::Spill { anchor } => Some(*anchor),
        _ => None,
    }
}

fn source_area(area: Area) -> SourceRef {
    SourceRef::Cells {
        sheet: Sheet(area.0.0, area.0.1),
        areas: vec![area],
    }
}
