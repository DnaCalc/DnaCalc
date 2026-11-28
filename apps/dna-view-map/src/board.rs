use super::*;
use crate::{commands::excerpt_at, detail};
use calc_skin::query::{self, SheetCensus};
use calc_skin::{
    Dependencies, DependencyAtlas, DependencyAtlasLink, DependencyDirection, DependencyGrouping,
    DependencyKind, DependencyTarget, FormulaRun, Interest, Structure, WorkbookCell,
};
use std::collections::BTreeMap;

const BOARD_COLUMNS: usize = 4;
const NODE_WIDTH: usize = 194;
const NODE_HEIGHT: usize = 80;
const X_STEP: usize = 226;
const Y_STEP: usize = 112;
const INSET: usize = 20;
const INVENTORY_PAGE: usize = 30;

#[derive(Clone)]
struct Node {
    label: String,
    detail: String,
    kind: &'static str,
    action: Action,
    selected: bool,
    area: Option<Area>,
    source: Option<SourceRef>,
}

#[derive(Clone, Copy)]
enum Action {
    Scope(Scope),
    Cell(Key),
    Inventory,
}

#[derive(Clone, Copy)]
struct Wire {
    source: usize,
    reader: usize,
    dynamic: bool,
}

pub(super) fn mount(session: Session, state: MapState) -> impl IntoView {
    let scope = Memo::new(move |_| state.scope(session));
    let census = session.following_with_budget::<query::CensusOf>(SURVEY);
    let atlas = session.following_with_budget::<query::DependencyAtlasOf>(SURVEY);
    let structure = session.following_with_budget::<query::StructureOf>(SURVEY);
    let port_values = session.following::<query::Cells>();
    let dependencies = session.following::<query::Deps>();
    let graph = Memo::new(move |_| {
        let active = session
            .active()
            .filter(|_| session.active_entry().is_some_and(|entry| entry.formula));
        let selected = active
            .filter(|at| state.listing.get() || scope.get().sheet() == Some(Sheet(at.0, at.1)));
        dependencies.at(selected.map(|at| query::Deps {
            of: cell_source(at),
            dir: DependencyDirection::Precedents,
            depth: 1,
            group: DependencyGrouping::Reference,
        }))
    });

    view! {
        <section class="dna-map" data-dnacalc="model-map" aria-label="Model map">
            <header class="dna-map-toolbar">
                <nav aria-label="Map scope" class="dna-map-crumbs">
                    {open_button(session, "Workbook", Scope::Book)}
                    {move || {
                        let current = scope.get();
                        let sheet = current.sheet()?;
                        Some(view! {
                            <span aria-hidden="true">"›"</span>
                            {open_button(session, sheet_name(session, sheet), Scope::Sheet(sheet))}
                            {match current {
                                Scope::Block(area) | Scope::Excerpt(area) => Some(view! {
                                    <span aria-hidden="true">"›"</span>
                                    <strong>{extent(area)}</strong>
                                }),
                                _ => None,
                            }}
                        })
                    }}
                </nav>
                <div class="dna-map-tools">
                    {button(session, "map.back", "Zoom out")}
                    {button(session, "map.drill", "Zoom in")}
                    {button(session, "map.inventory", "Sheets")}
                    {button(session, "map.connections", "Connections")}
                    {button(session, "map.companion", "Classic companion")}
                </div>
            </header>
            <div class="dna-map-body">
                <div class="dna-map-workbench" data-map-scope=move || match scope.get() {
                    Scope::Book => "workbook", Scope::Sheet(_) => "sheet",
                    Scope::Block(_) => "block", Scope::Excerpt(_) => "excerpt",
                }>
                    {move || {
                        let current = scope.get();
                        if state.inventory.get() {
                            return inventory(session, state).into_any();
                        }
                        match current {
                            Scope::Book => {
                                let atlas = atlas.at(Some(query::DependencyAtlasOf));
                                structure.at(None);
                                port_values.at(None);
                                let stats = census.at(Some(query::CensusOf { scope: SourceRef::Workbook }));
                                overview(session, state, atlas, stats).into_any()
                            }
                            Scope::Sheet(_) | Scope::Block(_) => {
                                let sheet = current.sheet().expect("worksheet scope");
                                atlas.at(None);
                                let source = match current {
                                    Scope::Block(area) => area_source(area),
                                    _ => SourceRef::Sheet(sheet),
                                };
                                let read = structure.at(Some(query::StructureOf { scope: source.clone() }));
                                let stats = census.at(Some(query::CensusOf { scope: source }));
                                read_error(read, "Reading worksheet structure…", move |answer| {
                                    evidence(answer.body).map(|body| {
                                        let areas: Vec<_> = body.ports.iter().filter_map(|port|
                                            (port.area.0 == port.area.1).then_some(port.area)
                                        ).collect();
                                        let values = match port_values.at((!areas.is_empty()).then_some(query::Cells {
                                            interest: Interest::Areas(areas),
                                        })) {
                                            Live::Ready(values) if values.stamp.workbook == answer.stamp.workbook
                                                && values.stamp.model == answer.stamp.model => values.body.cells,
                                            _ => Vec::new(),
                                        };
                                        sheet_board(session, state, current, body, stats, graph.get(), values)
                                    })
                                })
                            }
                            Scope::Excerpt(area) => {
                                atlas.at(None);
                                structure.at(None);
                                census.at(None);
                                port_values.at(None);
                                detail::excerpt(session, area).into_any()
                            }
                        }
                    }}
                </div>
                <Show when=move || state.companion.get()>
                    <aside class="dna-map-companion" aria-label="Classic companion">
                        <h2>"Classic companion"</h2>
                        {dna_grid::picker(session)}
                    </aside>
                </Show>
                <aside class="dna-map-inspector" aria-label="Map selection">
                    {detail::selection(session, state, graph)}
                    {dna_kit::page::clock::authored_controls(session)}
                </aside>
            </div>
        </section>
    }
}

fn overview(
    session: Session,
    state: MapState,
    atlas: Live<calc_skin::Answer<Evidence<DependencyAtlas>>>,
    census: Live<calc_skin::Answer<Census>>,
) -> impl IntoView {
    let active = session.active().map(|at| Sheet(at.0, at.1));
    let default = session
        .summary()
        .map(|summary| summary.sheets)
        .unwrap_or_default();
    let worksheets: Vec<_> = default.iter().filter(|sheet| sheet.worksheet).collect();
    let total_sheets = worksheets.len();
    let direct = if total_sheets > OVERVIEW_NODES {
        OVERVIEW_NODES - 1
    } else {
        total_sheets
    };
    let sheets = &worksheets[..direct];
    let stats: BTreeMap<_, _> = match census {
        Live::Ready(ref answer) => answer
            .body
            .sheets
            .iter()
            .map(|row| (row.sheet, row))
            .collect(),
        _ => BTreeMap::new(),
    };
    let (atlas, state_message) = match atlas {
        Live::Ready(answer) => (evidence(answer.body), None),
        Live::Refused(reason) => (None, Some(dna_kit::refusal_text(reason))),
        _ => (None, Some("Reading workbook connections…".into())),
    };
    let index: BTreeMap<_, _> = worksheets
        .iter()
        .enumerate()
        .map(|(index, sheet)| (sheet.sheet, index.min(direct)))
        .collect();
    let names: BTreeMap<_, _> = worksheets
        .iter()
        .map(|sheet| (sheet.sheet, sheet.name.clone()))
        .collect();
    let counts: BTreeMap<_, _> = atlas
        .iter()
        .flat_map(|atlas| &atlas.sheets)
        .map(|sheet| (sheet.sheet, sheet.formulas))
        .collect();
    let mut nodes = sheets
        .iter()
        .map(|sheet| {
            let count = stats
                .get(&sheet.sheet)
                .map(|row| row.formulas)
                .or_else(|| counts.get(&sheet.sheet).copied());
            let count = count
                .map(|count| format!("{count} formulas"))
                .unwrap_or_default();
            let errors = error_text(stats.get(&sheet.sheet).copied());
            Node {
                label: sheet.name.clone(),
                detail: [
                    count,
                    if sheet.visible {
                        String::new()
                    } else {
                        "Hidden worksheet".into()
                    },
                    errors,
                ]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
                kind: "sheet",
                action: Action::Scope(Scope::Sheet(sheet.sheet)),
                selected: active == Some(sheet.sheet),
                area: None,
                source: Some(SourceRef::Sheet(sheet.sheet)),
            }
        })
        .collect::<Vec<_>>();
    if total_sheets > direct {
        let members = &worksheets[direct..];
        let errors: u64 = members
            .iter()
            .filter_map(|sheet| stats.get(&sheet.sheet))
            .flat_map(|row| row.errors.values())
            .sum();
        let mut detail = format!("{} worksheets in inventory", members.len());
        if errors > 0 {
            detail.push_str(&format!(" · {errors} errors"));
        }
        nodes.push(Node {
            label: "‹More worksheets›".into(),
            detail,
            kind: "group",
            action: Action::Inventory,
            selected: members.iter().any(|sheet| active == Some(sheet.sheet)),
            area: None,
            source: None,
        });
    }
    let links = atlas
        .as_ref()
        .map(|atlas| atlas.links.as_slice())
        .unwrap_or_default();
    let mut drawn_references = 0;
    let mut grouped_references = 0;
    let mut grouped = BTreeMap::<_, (u64, u64)>::new();
    for link in links {
        let Some((&source, &reader)) = index.get(&link.precedent).zip(index.get(&link.consumer))
        else {
            continue;
        };
        if source == reader {
            grouped_references += link.placed + link.dynamic;
        } else {
            let (placed, dynamic) = grouped.entry((source, reader)).or_default();
            *placed += link.placed;
            *dynamic += link.dynamic;
        }
    }
    let wires = grouped
        .into_iter()
        .take(MAX_EDGES)
        .map(|((source, reader), (placed, dynamic))| {
            drawn_references += placed + dynamic;
            Wire {
                source,
                reader,
                dynamic: placed == 0,
            }
        })
        .collect::<Vec<_>>();
    let formulas = atlas
        .as_ref()
        .map(|atlas| atlas.sheets.iter().map(|sheet| sheet.formulas).sum())
        .or_else(|| match census {
            Live::Ready(ref answer) => Some(answer.body.formulas),
            _ => None,
        });
    let refs = atlas.as_ref().map(|atlas| {
        let unbound: u64 = atlas.sheets.iter().map(|sheet| sheet.unbound).sum();
        let site_records = atlas.total_references;
        let accounted = drawn_references + grouped_references + atlas.local_references + atlas.unattributed_names;
        let summary = format!(
            "{accounted} of {site_records} references accounted for · {drawn_references} drawn · {} within sheets · {grouped_references} within groups · {} unattributed · {} cross-sheet records off canvas",
            atlas.local_references, atlas.unattributed_names, site_records.saturating_sub(accounted),
        );
        view! {
            <p data-map-coverage="references" data-map-reference-total=site_records>{summary}</p>
            {(unbound > 0).then(|| view! { <p>{format!("{unbound} formula anchors await binding.")}</p> })}
        }
    });
    let pages = links.len().div_ceil(INVENTORY_PAGE);
    let page = state.wire_page.get().min(pages.saturating_sub(1));
    let link_list = state
        .listing
        .get()
        .then(|| connection_list(session, page, links, &names));
    let canvases = diagram(session, nodes, wires);
    view! {
        <div class="dna-map-caption">
            <h2>"Workbook structure"</h2>
            <p data-map-coverage="sheets">{format!("{} of {total_sheets} worksheets drawn", sheets.len())}</p>
            {(total_sheets > direct).then(|| view! { <p>{format!("{} worksheets in the inventory group", total_sheets - direct)}</p> })}
            {formulas.map(|count| view! { <p data-map-formulas=count>{format!("{count} formula anchors")}</p> })}
            {formulas.filter(|count| *count > MAX_NODES as u64)
                .map(|count| expand_all(session, state, count))}
        </div>
        {canvases}
        <footer class="dna-map-footer">
            {state_message.map(|message| view! { <p class="dna-map-status" role="status">{message}</p> })}
            {refs}
        </footer>
        {link_list}
    }
}

fn sheet_board(
    session: Session,
    state: MapState,
    scope: Scope,
    structure: Structure,
    census: Live<calc_skin::Answer<Census>>,
    graph: Live<calc_skin::Answer<Evidence<Dependencies>>>,
    ports: Vec<WorkbookCell>,
) -> impl IntoView {
    let sheet = scope.sheet().expect("sheet scope");
    let active = session.active();
    let folded: u64 = structure
        .runs
        .iter()
        .flat_map(|run| &run.members)
        .filter_map(|part| part.area())
        .map(|part| part.cell_count())
        .sum();
    let exceptions: usize = structure.runs.iter().map(|run| run.exceptions.len()).sum();
    let formulas = structure.formulas;
    let standalone = formulas.saturating_sub(folded);
    let mut nodes: Vec<_> = structure
        .runs
        .iter()
        .map(|run| run_node(run, active, scope))
        .collect();
    nodes.extend(structure.tables.iter().map(|table| Node {
        label: table.name.clone(),
        detail: format!("Table · {}", extent(table.area)),
        kind: "table",
        action: Action::Scope(Scope::Block(table.area)),
        selected: active.is_some_and(|at| contains(table.area, at)),
        area: Some(table.area),
        source: None,
    }));
    let readings: BTreeMap<_, _> = ports.into_iter().map(|cell| (cell.at, cell)).collect();
    nodes.extend(structure.ports.iter().map(|port| Node {
        label: port.name.clone(),
        detail: match readings.get(&port.area.0) {
            Some(cell) if port.area.0 == port.area.1 => format!(
                "{} · {} · {}",
                extent(port.area),
                cell.text,
                detail::provenance(&cell.shown.provenance),
            ),
            _ => format!("Defined Name · {}", extent(port.area)),
        },
        kind: "port",
        action: Action::Cell(port.area.0),
        selected: active == Some(port.area.0),
        area: Some(port.area),
        source: Some(SourceRef::Name {
            scope: port.scope.clone(),
            name: port.name.clone(),
        }),
    }));
    nodes.extend(structure.standalone.iter().map(|at| Node {
        label: a1(*at),
        detail: "Formula".into(),
        kind: "formula",
        action: Action::Cell(*at),
        selected: active == Some(*at),
        area: Some(Area(*at, *at)),
        source: None,
    }));
    let returned = nodes.len();
    let total = u64::try_from(structure.runs.len() + structure.tables.len()).unwrap_or_default()
        + standalone
        + structure.port_count;
    let page = state
        .node_page
        .get()
        .min(returned.div_ceil(MAX_NODES).saturating_sub(1));
    let mut shown = nodes
        .into_iter()
        .skip(page * MAX_NODES)
        .take(MAX_NODES)
        .collect::<Vec<_>>();
    let shown_count = shown.len();
    let no_nodes = shown.is_empty();
    let status = match census {
        Live::Ready(answer) => error_text(answer.body.sheets.iter().find(|row| row.sheet == sheet)),
        Live::Refused(reason) => dna_kit::refusal_text(reason),
        _ => String::new(),
    };
    let excerpt = active
        .filter(|at| Sheet(at.0, at.1) == sheet)
        .map(excerpt_at)
        .or_else(|| structure.runs.first().map(|run| excerpt_at(run.extent.0)));
    let heading = match scope {
        Scope::Block(area) => format!("{} · {}", sheet_name(session, sheet), extent(area)),
        _ => sheet_name(session, sheet),
    };
    let mut wire_count = None;
    let wires = match (active, graph) {
        (Some(at), Live::Ready(answer)) => evidence(answer.body).map_or_else(Vec::new, |graph| {
            let wires = local_wires(session, &mut shown, at, &graph);
            wire_count = Some((wires.len(), graph.edges.len()));
            wires
        }),
        _ => Vec::new(),
    };
    let diagram = diagram(session, shown, wires);
    let runs = structure.runs.len();
    let run_label = if runs == 1 { "run" } else { "runs" };
    view! {
        <div class="dna-map-caption">
            <h2>{heading}</h2>
            <p data-map-coverage="formulas" data-map-formulas=formulas>
                {format!("{folded} of {formulas} formulas in {runs} {run_label} · {standalone} outside runs · {exceptions} occupied exceptions")}
            </p>
            {(!status.is_empty()).then(|| view! { <p class="dna-map-anomaly">{status}</p> })}
            <div class="dna-map-tools">
                {excerpt.map(|area| open_button(session, "Sheet excerpt", Scope::Excerpt(area)))}
                {button(session, "view.inspector", "Inspector")}
            </div>
        </div>
        {no_nodes.then(|| view! { <p class="dna-map-status">"No tables, stable names or formulas in this scope."</p> })}
        {diagram}
        <footer class="dna-map-footer">
            <p data-map-coverage="nodes">{format!("{shown_count} of {total} map items drawn")}</p>
            {wire_count.map(|(drawn, count)| view! {
                <p data-map-coverage="direct">{format!("{drawn} of {count} direct connection records drawn for the selected formula.")}</p>
            })}
            {(total > returned as u64).then(|| view! {
                <p>{format!("{} individual items are outside the returned inventory. Use the sheet excerpt or Classic.", total - returned as u64)}</p>
            })}
            {pagination(session, "map.node-page", page, returned.div_ceil(MAX_NODES))}
        </footer>
    }
}

fn run_node(run: &FormulaRun, active: Option<Key>, scope: Scope) -> Node {
    let count: u64 = run
        .members
        .iter()
        .filter_map(|area| area.area())
        .map(|area| area.cell_count())
        .sum();
    Node {
        label: format!("‹{}›", extent(run.extent)),
        detail: format!(
            "{count} formulas · {} exceptions · {}",
            run.exceptions.len(),
            run.r1c1
        ),
        kind: "run",
        action: Action::Scope(match scope {
            Scope::Block(_) => Scope::Excerpt(excerpt_at(run.extent.0)),
            _ => Scope::Block(run.extent),
        }),
        selected: active.is_some_and(|at| contains(run.extent, at)),
        area: Some(run.extent),
        source: None,
    }
}

fn local_wires(
    session: Session,
    nodes: &mut Vec<Node>,
    at: Key,
    graph: &Dependencies,
) -> Vec<Wire> {
    let Some(reader) = nodes
        .iter()
        .position(|node| node.area.is_some_and(|area| contains(area, at)))
    else {
        return Vec::new();
    };
    graph
        .edges
        .iter()
        .filter_map(|edge| {
            let target = graph.nodes.get(edge.to)?;
            let DependencyTarget::Source(source) = target else {
                return None;
            };
            let area = match source {
                SourceRef::Cells { areas, .. } if areas.len() == 1 => Some(areas[0]),
                SourceRef::Spill { anchor } => Some(Area(*anchor, *anchor)),
                _ => None,
            };
            let existing = nodes.iter().position(|node| {
                node.source.as_ref() == Some(source)
                    || area.zip(node.area).is_some_and(|(source, extent)| {
                        contains(extent, source.0) && contains(extent, source.1)
                    })
            });
            let source = existing.or_else(|| {
                let area = area?;
                if nodes.len() == MAX_NODES {
                    return None;
                }
                let label = format!(
                    "{}!{}",
                    sheet_name(session, Sheet(area.0.0, area.0.1)),
                    extent(area)
                );
                let index = nodes.len();
                nodes.push(Node {
                    label,
                    detail: "Referenced range".into(),
                    kind: "boundary",
                    action: Action::Cell(area.0),
                    selected: false,
                    area: Some(area),
                    source: Some(source.clone()),
                });
                Some(index)
            })?;
            // An internal read in a folded run does not establish feedback.
            if source == reader && area != Some(Area(at, at)) {
                return None;
            }
            Some(Wire {
                source,
                reader,
                dynamic: edge.kind == DependencyKind::Dynamic,
            })
        })
        .take(MAX_EDGES)
        .collect()
}

fn error_text(census: Option<&SheetCensus>) -> String {
    census
        .into_iter()
        .flat_map(|row| &row.errors)
        .map(|(name, count)| format!("{count} × {name}"))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn expand_all(session: Session, state: MapState, formulas: u64) -> impl IntoView {
    view! {
        {button(session, "map.expand-all", "Expand all formulas")}
        {move || state.expanded.get().then(|| view! {
            <div class="dna-map-decline" data-map-decline role="status">
                <p>{format!("{formulas} formulas exceed the map limit of {MAX_NODES} nodes and {MAX_EDGES} wires.")}</p>
                <div class="dna-map-tools">
                    {button(session, "map.inventory", "Choose a sheet submap")}
                    {button(session, "map.connections", "Open connections list")}
                    {button(session, "view.classic", "Open Classic")}
                </div>
            </div>
        })}
    }
}

fn inventory(session: Session, state: MapState) -> impl IntoView {
    let sheets = session
        .summary()
        .map_or_else(Vec::new, |summary| summary.sheets);
    let pages = sheets.len().div_ceil(INVENTORY_PAGE);
    let page = state.node_page.get().min(pages.saturating_sub(1));
    let count = sheets.len();
    let entries = sheets
        .into_iter()
        .skip(page * INVENTORY_PAGE)
        .take(INVENTORY_PAGE)
        .map(|sheet| {
            view! {
                <li>
                    {if sheet.worksheet {
                        open_button(session, sheet.name, Scope::Sheet(sheet.sheet)).into_any()
                    } else {
                        view! { <span>{sheet.name}" · non-worksheet"</span> }.into_any()
                    }}
                    {(!sheet.visible).then_some(" · hidden")}
                </li>
            }
        })
        .collect_view();
    view! {
        <div class="dna-map-list" aria-label="Sheet inventory">
            <h2>"Sheet inventory"</h2>
            <p>{format!("{}–{} of {count} sheets", usize::from(count > 0) + page * INVENTORY_PAGE, count.min((page + 1) * INVENTORY_PAGE))}</p>
            <ul>{entries}</ul>
            {pagination(session, "map.node-page", page, pages)}
        </div>
    }
}

fn connection_list(
    session: Session,
    page: usize,
    links: &[DependencyAtlasLink],
    names: &BTreeMap<Sheet, String>,
) -> impl IntoView + use<> {
    let total = links.len();
    let entries = links
        .iter()
        .skip(page * INVENTORY_PAGE)
        .take(INVENTORY_PAGE)
        .map(|link| {
            let source = names.get(&link.precedent).cloned().unwrap_or_default();
            let reader = names.get(&link.consumer).cloned().unwrap_or_default();
            let count = link.placed + link.dynamic;
            view! {
                <li>
                    {open_button(session, source, Scope::Sheet(link.precedent))}
                    <span aria-label="feeds">" → "</span>
                    {open_button(session, reader, Scope::Sheet(link.consumer))}
                    <span>{format!(" · {count} reference records")}</span>
                </li>
            }
        })
        .collect_view();
    view! {
        <section class="dna-map-list" aria-label="Cross-sheet connections">
            <h2>{format!("{total} cross-sheet connections")}</h2>
            <ul>{entries}</ul>
            {pagination(session, "map.wire-page", page, total.div_ceil(INVENTORY_PAGE))}
        </section>
    }
}

fn pagination(session: Session, command: &'static str, page: usize, pages: usize) -> impl IntoView {
    (pages > 1).then(|| view! {
        <nav class="dna-map-tools" aria-label="Pages">
            <button type="button" disabled=page == 0 data-command=command
                on:click=move |_| invoke(session, command, page.saturating_sub(1))>"Previous"</button>
            <span>{format!("Page {} of {pages}", page + 1)}</span>
            <button type="button" disabled=page + 1 == pages data-command=command
                on:click=move |_| invoke(session, command, page + 1)>"Next"</button>
        </nav>
    })
}

pub(super) fn button(session: Session, id: &'static str, title: &'static str) -> impl IntoView {
    view! { <button type="button" data-command=id on:click=move |_| { let _ = session.command(id); }>{title}</button> }
}

pub(super) fn open_button(
    session: Session,
    title: impl Into<String>,
    scope: Scope,
) -> impl IntoView {
    let title = title.into();
    let target = match scope {
        Scope::Book => "workbook",
        Scope::Sheet(_) => "sheet",
        Scope::Block(_) => "block",
        Scope::Excerpt(_) => "excerpt",
    };
    view! {
        <button type="button" data-command="map.open" data-map-open=target
            on:click=move |_| invoke(session, "map.open", scope)>{title}</button>
    }
}

fn diagram(session: Session, nodes: Vec<Node>, wires: Vec<Wire>) -> impl IntoView {
    let node_count = nodes.len();
    let edge_count = wires.len();
    let height =
        INSET * 2 + node_count.div_ceil(BOARD_COLUMNS).saturating_sub(1) * Y_STEP + NODE_HEIGHT;
    let width = INSET * 2 + (BOARD_COLUMNS - 1) * X_STEP + NODE_WIDTH;
    let canvas = NodeRef::<leptos::html::Canvas>::new();
    paint(canvas, wires);
    let items = nodes.into_iter().map(|node| {
        let command = match node.action {
            Action::Scope(_) => "map.open",
            Action::Cell(_) => "map.select",
        Action::Inventory => "map.inventory",
        };
        let label = node.label;
        let detail = node.detail;
        let name = label.clone();
        let node_title = label.clone();
        let detail_title = detail.clone();
        view! {
            <li class="dna-map-node">
                <button type="button" data-command=command data-map-node=node.kind data-map-name=name aria-pressed=node.selected
                    on:click=move |_| match node.action {
                        Action::Scope(scope) => invoke(session, command, scope),
                        Action::Cell(at) => invoke(session, command, at),
                        Action::Inventory => { let _ = session.command(command); },
                    }>
                    <strong title=node_title>{label}</strong>
                    <span title=detail_title>{detail}</span>
                </button>
            </li>
        }
    }).collect_view();
    view! {
        <div class="dna-map-viewport" tabindex="0" aria-label="Map schematic"
            on:wheel=move |event| {
                if event.ctrl_key() && event.delta_y() != 0.0 {
                    event.prevent_default();
                    let _ = session.command(if event.delta_y() < 0.0 { "map.drill" } else { "map.back" });
                }
            }>
            <div class="dna-map-diagram" data-map-node-count=node_count data-map-edge-count=edge_count>
                <canvas aria-hidden="true" width=width as u32 height=height as u32 node_ref=canvas></canvas>
                <ul aria-label="Map nodes">{items}</ul>
            </div>
        </div>
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn paint(_: NodeRef<leptos::html::Canvas>, _: Vec<Wire>) {}

#[cfg(target_arch = "wasm32")]
fn paint(canvas: NodeRef<leptos::html::Canvas>, wires: Vec<Wire>) {
    use wasm_bindgen::JsCast;
    use web_sys::CanvasRenderingContext2d;

    fn origin(index: usize) -> (f64, f64) {
        (
            (INSET + index % BOARD_COLUMNS * X_STEP) as f64,
            (INSET + index / BOARD_COLUMNS * Y_STEP) as f64,
        )
    }

    Effect::new(move |_| {
        let Some(canvas) = canvas.get() else {
            return;
        };
        let Ok(Some(context)) = canvas.get_context("2d") else {
            return;
        };
        let Ok(context) = context.dyn_into::<CanvasRenderingContext2d>() else {
            return;
        };
        context.clear_rect(
            0.0,
            0.0,
            f64::from(canvas.width()),
            f64::from(canvas.height()),
        );
        context.set_line_width(1.75);
        context.set_stroke_style_str("#4b9191");
        context.set_fill_style_str("#4b9191");
        for wire in &wires {
            let (sx, sy) = origin(wire.source);
            let (rx, ry) = origin(wire.reader);
            let dash = js_dash(wire.dynamic);
            let _ = context.set_line_dash(&dash);
            if wire.source == wire.reader {
                let edge = sx + NODE_WIDTH as f64;
                context.begin_path();
                context.move_to(edge, sy + 57.0);
                context.bezier_curve_to(
                    edge + 28.0,
                    sy + 65.0,
                    edge + 28.0,
                    sy + 14.0,
                    edge,
                    sy + 23.0,
                );
                context.stroke();
                context.begin_path();
                context.move_to(edge, sy + 23.0);
                context.line_to(edge + 7.0, sy + 18.0);
                context.line_to(edge + 7.0, sy + 25.0);
                context.fill();
                continue;
            }
            let from = (sx + NODE_WIDTH as f64 / 2.0, sy + NODE_HEIGHT as f64);
            let to = (rx + NODE_WIDTH as f64 / 2.0, ry);
            let bend = (from.1 + to.1) / 2.0;
            context.begin_path();
            context.move_to(from.0, from.1);
            context.bezier_curve_to(from.0, bend + 16.0, to.0, bend - 16.0, to.0, to.1 - 4.0);
            context.stroke();
            context.begin_path();
            context.move_to(to.0, to.1);
            context.line_to(to.0 - 4.0, to.1 - 7.0);
            context.line_to(to.0 + 4.0, to.1 - 7.0);
            context.fill();
        }
    });

    fn js_dash(dynamic: bool) -> wasm_bindgen::JsValue {
        let values = leptos::web_sys::js_sys::Array::new();
        if dynamic {
            values.push(&4.into());
            values.push(&3.into());
        }
        values.into()
    }
}
