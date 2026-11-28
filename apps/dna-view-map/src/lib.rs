//! A bounded schematic over the same Excel addresses as the Classic worksheet.

mod board;
mod commands;
mod detail;

use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::query::Census;
use calc_skin::{Budget, Evidence, Selection, SourceRef, Target, ViewId, WorkbookId};
use dna_kit::{Fit, Live, Located, MountedView, Registry, Session, View, ViewCommand, ViewSpec};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

pub const ID: &str = "map";
const OVERVIEW_NODES: usize = 60;
const MAX_NODES: usize = 200;
const MAX_EDGES: usize = 400;

const SURVEY: Budget = Budget {
    cells: 2_000_000,
    nodes: 32_768,
    edges: 2_000_000,
    bytes: 4 * 1024 * 1024,
    millis: 5_000,
};

pub fn install(registry: &mut Registry) {
    registry.view(ModelMap);
}

struct ModelMap;

impl View for ModelMap {
    fn spec(&self) -> ViewSpec {
        ViewSpec {
            id: ViewId(ID.into()),
            title: "Model map",
            profile: "excel-365-win-en-us",
        }
    }

    fn fit(&self, census: &Census) -> Fit {
        // A workbook can always be read at the sheet level.
        if census.sheets.len() > OVERVIEW_NODES {
            Fit::Simplified {
                shown: OVERVIEW_NODES as u64,
                total: census.sheets.len() as u64,
                explanation: "Use the sheet inventory to open any omitted worksheet.".into(),
            }
        } else {
            Fit::Whole
        }
    }

    fn commands(&self, session: Session) -> Vec<ViewCommand> {
        commands::install(session, session.local::<MapState>())
    }

    fn cancel_interaction(&self, session: Session) -> bool {
        commands::back(session, session.local())
    }

    fn mount(&self, session: Session) -> MountedView {
        let state = session.local::<MapState>();
        MountedView {
            body: board::mount(session, state).into_any(),
            locate: Box::new(move |selection| locate(session, state, selection)),
            reveal: Box::new(move |target| {
                if let Target::Source(source) = target
                    && let Some(sheet) = sheet_of(source)
                {
                    state.set_scope(session, Scope::Sheet(sheet));
                }
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
enum Scope {
    #[default]
    Book,
    Sheet(Sheet),
    Block(Area),
    Excerpt(Area),
}

impl Scope {
    fn sheet(self) -> Option<Sheet> {
        match self {
            Self::Book => None,
            Self::Sheet(sheet) => Some(sheet),
            Self::Block(Area(first, _)) | Self::Excerpt(Area(first, _)) => {
                Some(Sheet(first.0, first.1))
            }
        }
    }
}

#[derive(Clone, Copy)]
struct MapState {
    scope: RwSignal<Option<(WorkbookId, Scope)>>,
    rewire: RwSignal<Option<(WorkbookId, Key)>>,
    listing: RwSignal<bool>,
    expanded: RwSignal<bool>,
    inventory: RwSignal<bool>,
    companion: RwSignal<bool>,
    node_page: RwSignal<usize>,
    wire_page: RwSignal<usize>,
}

impl Default for MapState {
    fn default() -> Self {
        Self {
            scope: RwSignal::new(None),
            rewire: RwSignal::new(None),
            listing: RwSignal::new(false),
            expanded: RwSignal::new(false),
            inventory: RwSignal::new(false),
            companion: RwSignal::new(false),
            node_page: RwSignal::new(0),
            wire_page: RwSignal::new(0),
        }
    }
}

impl MapState {
    fn scope(self, session: Session) -> Scope {
        let book = session.stamp().map(|stamp| stamp.workbook);
        self.scope
            .get()
            .filter(|(owner, _)| Some(*owner) == book)
            .map_or(Scope::Book, |(_, scope)| scope)
    }

    fn set_scope(self, session: Session, scope: Scope) {
        self.rewire.set(None);
        self.expanded.set(false);
        self.node_page.set(0);
        self.wire_page.set(0);
        self.inventory.set(false);
        self.scope
            .set(session.stamp().map(|stamp| (stamp.workbook, scope)));
    }

    fn editing_wire(self, session: Session, at: Key) -> bool {
        session
            .stamp()
            .is_some_and(|stamp| self.rewire.get() == Some((stamp.workbook, at)))
    }
}

fn sheet_of(source: &SourceRef) -> Option<Sheet> {
    match source {
        SourceRef::Sheet(sheet) | SourceRef::Cells { sheet, .. } => Some(*sheet),
        SourceRef::Spill {
            anchor: Key(slot, generation, ..),
        } => Some(Sheet(*slot, *generation)),
        _ => None,
    }
}

fn cell_source(at: Key) -> SourceRef {
    area_source(Area(at, at))
}

fn area_source(area: Area) -> SourceRef {
    SourceRef::Cells {
        sheet: Sheet(area.0.0, area.0.1),
        areas: vec![area],
    }
}

fn sheet_name(session: Session, sheet: Sheet) -> String {
    session
        .summary()
        .and_then(|summary| {
            summary
                .sheets
                .into_iter()
                .find(|entry| entry.sheet == sheet)
        })
        .map_or_else(|| "Worksheet".into(), |entry| entry.name)
}

fn a1(Key(_, _, row, col): Key) -> String {
    format!("{}{row}", calc_skin::address::column_letters(col))
}

fn extent(Area(first, last): Area) -> String {
    if first == last {
        a1(first)
    } else {
        format!("{}:{}", a1(first), a1(last))
    }
}

fn locate(session: Session, state: MapState, selection: &Selection) -> Located {
    let Some(sheet) = sheet_of(&selection.target) else {
        return Located::Absent;
    };
    match state.scope(session) {
        Scope::Book => Located::Within {
            label: sheet_name(session, sheet),
        },
        Scope::Sheet(active) if active == sheet => Located::Within {
            label: sheet_name(session, sheet),
        },
        Scope::Block(area) | Scope::Excerpt(area)
            if selection.active.is_some_and(|at| contains(area, at)) =>
        {
            Located::Exact
        }
        _ => Located::Absent,
    }
}

fn contains(area: Area, at: Key) -> bool {
    (area.0.0, area.0.1) == (at.0, at.1)
        && (area.0.2..=area.1.2).contains(&at.2)
        && (area.0.3..=area.1.3).contains(&at.3)
}

fn evidence<T>(evidence: Evidence<T>) -> Option<T> {
    match evidence {
        Evidence::Derived { body, .. } | Evidence::Observed { body, .. } => Some(body),
        Evidence::Unavailable { .. } => None,
    }
}

fn invoke<T: Serialize>(session: Session, command: &'static str, args: T) {
    let _ = session.run(&dna_kit::Invocation {
        command: dna_kit::commands::CommandId::new(command),
        args: serde_json::to_value(args).expect("map command arguments serialize"),
    });
}

fn read_error<T, V: IntoView>(
    read: Live<T>,
    loading: &'static str,
    ready: impl FnOnce(T) -> V,
) -> AnyView {
    match read {
        Live::Ready(answer) => ready(answer).into_any(),
        Live::Refused(reason) => {
            view! { <p class="dna-map-status" role="status">{dna_kit::refusal_text(reason)}</p> }
                .into_any()
        }
        _ => view! { <p class="dna-map-status" role="status" aria-busy="true">{loading}</p> }
            .into_any(),
    }
}
