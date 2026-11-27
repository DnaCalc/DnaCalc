use crate::{area_label, button, coverage, evidence, result, reveal, sheet_label, status};
use calc_skin::{
    Dependencies, DependencyDirection, DependencyGrouping, DependencyTarget, ErrorOrigin,
    InventoryItem, InventoryKind, SheetVisibility, SourceRef, query,
};
use dna_kit::Session;
use leptos::prelude::*;
use std::collections::BTreeMap;

pub(crate) fn overview(session: Session) -> impl IntoView {
    let census = session.following_with_budget::<query::CensusOf>(crate::INSTRUMENT_BUDGET);
    let inventory = session.following::<query::Inventory>();
    let structure = session.following_with_budget::<query::StructureOf>(crate::INSTRUMENT_BUDGET);
    view! {
        <div class="dna-inspector-card">
            {move || match result(census.at(Some(query::CensusOf { scope: SourceRef::Workbook })), "") {
                Ok(answer) => view! {
                    <div class="dna-inspector-counts" aria-label="Workbook counts">
                        <div><strong>{answer.body.sheets.len()}</strong><span>"Worksheets"</span></div>
                        <div><strong>{answer.body.formulas}</strong><span>"Formulas"</span></div>
                        <div><strong>{answer.body.literals}</strong><span>"Literals"</span></div>
                    </div>
                    {coverage(answer.coverage).map(status)}
                }.into_any(),
                Err(message) => status(message),
            }}
        </div>
        {issues(session)}
        <div class="dna-inspector-card">
            <h2>"Workbook inventory"</h2>
            {move || {
                let inventory = inventory.at(Some(query::Inventory { kinds: vec![
                    InventoryKind::Sheets, InventoryKind::Names, InventoryKind::ExternalLinks,
                    InventoryKind::VbaProject, InventoryKind::PivotTables,
                ] }));
                match result(inventory, "") {
                    Ok(answer) => inventory_view(answer.body, coverage(answer.coverage)),
                    Err(message) => status(message),
                }
            }}
        </div>
        <div class="dna-inspector-card">
            <h2>"Structure"</h2>
            {move || {
                let data = result(structure.at(Some(query::StructureOf { scope: SourceRef::Workbook })), "")
                    .and_then(|answer| Ok((evidence(answer.body)?, coverage(answer.coverage))));
                match data {
                    Ok((structure, limit)) => view! {
                        <p>{format!("{} tables · {} formula runs", structure.tables.len(), structure.runs.len())}</p>
                        {limit.map(status)}
                        {structure.tables.into_iter().take(40).map(|table| view! {
                            <p><strong>{table.name}</strong>" · " {area_label(session, table.area)}</p>
                        }).collect_view()}
                        {structure.runs.into_iter().filter(|run| !run.exceptions.is_empty()).take(40).map(|run| view! {
                            <p>{format!("{} overrides in {}", run.exceptions.len(), area_label(session, run.extent))}</p>
                        }).collect_view()}
                    }.into_any(),
                    Err(message) => status(message),
                }
            }}
        </div>
    }
}

fn inventory_view(items: Vec<InventoryItem>, limit: Option<String>) -> AnyView {
    let mut hidden = Vec::new();
    let mut broken = Vec::new();
    let mut links = Vec::new();
    let mut pivots = Vec::new();
    let mut vba = false;
    for item in items {
        match item {
            InventoryItem::Sheet {
                name, visibility, ..
            } if visibility != SheetVisibility::Visible => {
                let label = if visibility == SheetVisibility::VeryHidden {
                    "VeryHidden"
                } else {
                    "Hidden"
                };
                hidden.push(view! { <li>{name}<span>{label}</span></li> });
            }
            InventoryItem::Name { name, formula, .. }
                if formula.to_ascii_uppercase().contains("#REF!") =>
            {
                broken.push(view! { <li>{name}<code>{formula}</code></li> });
            }
            InventoryItem::PivotTable { name, .. } => pivots.push(view! { <li>{name}</li> }),
            InventoryItem::ExternalLink(link) => links.push(format!(
                "{} · {} cached sheets · {} refresh errors",
                link.file_name
                    .unwrap_or_else(|| format!("Link {}", link.index)),
                link.cached_sheets,
                link.refresh_error_sheets,
            )),
            InventoryItem::VbaProject => vba = true,
            _ => (),
        }
    }
    view! {
        {limit.map(status)}
        <div class="dna-inspector-columns">
            <section><h3>{format!("{} hidden worksheets", hidden.len())}</h3><ul>{hidden}</ul></section>
            <section><h3>{format!("{} names containing #REF!", broken.len())}</h3><ul>{broken}</ul></section>
            <section><h3>{format!("{} external workbook links", links.len())}</h3><ul>{links.into_iter().map(|link| view! { <li>{link}</li> }).collect_view()}</ul></section>
            <section><h3>{format!("{} PivotTables", pivots.len())}</h3><ul>{pivots}</ul></section>
        </div>
        <p>{if vba { "VBA project present" } else { "No VBA project in inventory" }}</p>
    }.into_any()
}

fn issues(session: Session) -> impl IntoView {
    let issues = session.following_with_budget::<query::IssuesIn>(crate::INSTRUMENT_BUDGET);
    view! {
        <div class="dna-inspector-card">
            <h2>"Current worksheet errors"</h2>
            {move || {
                let data = result(issues.at(Some(query::IssuesIn { scope: SourceRef::Workbook })), "")
                    .and_then(|answer| Ok((evidence(answer.body)?, coverage(answer.coverage))));
                let (issues, limit) = match data {
                    Ok(value) => value,
                    Err(message) => return status(message),
                };
                let count = issues.errors.len();
                let unresolved = issues.errors.iter().filter(|item| matches!(item.origin, ErrorOrigin::Unresolved)).count();
                let errors: BTreeMap<_,_> = issues.errors.into_iter().map(|item| (item.at, item)).collect();
                let rows = issues.roots.iter().take(100).map(|root| {
                    let item = &errors[&root.at];
                    let origin = if item.origin == ErrorOrigin::Literal { "Literal" } else { "Root candidate" };
                    view! { <tr><th scope="row">{reveal(session, root.at)}</th><td>{item.code.clone()}</td><td>{origin}</td><td>{root.affected}</td></tr> }.into_any()
                }).chain(errors.values().filter(|item| item.origin == ErrorOrigin::Unresolved).take(100).map(|item| {
                    view! { <tr><th scope="row">{reveal(session, item.at)}</th><td>{item.code.clone()}</td><td>"Unresolved"</td><td>"—"</td></tr> }.into_any()
                })).collect_view();
                view! {
                    <p data-inspector="issues-count">{format!("{count} {} · {} root candidates and literals · {unresolved} unresolved",
                        if count == 1 { "error" } else { "errors" }, issues.roots.len())}</p>
                    {limit.map(status)}
                    {(issues.roots.len() > 100 || unresolved > 100).then(|| view! { <p>"Showing up to 100 per origin group."</p> })}
                    {(count > 0).then(|| view! {
                        <table><thead><tr><th scope="col">"Cell"</th><th scope="col">"Error"</th><th scope="col">"Origin"</th><th scope="col">"Other affected cells"</th></tr></thead>
                        <tbody>{rows}</tbody></table>
                    })}
                }.into_any()
            }}
        </div>
    }
}

pub(crate) fn dependencies(session: Session, direction: DependencyDirection) -> impl IntoView {
    let deps = session.following::<query::Deps>();
    let names = session.following::<query::Inventory>();
    view! {
        <div class="dna-inspector-card">
            <h2>{match direction {
                DependencyDirection::Precedents => "Upstream dependencies",
                DependencyDirection::Dependents => "Downstream dependencies",
            }}</h2>
            {move || {
                let scope = session.selection().map(|selection| selection.target);
                let data = result(deps.at(scope.map(|of| query::Deps {
                    of, dir: direction, depth: 5, group: DependencyGrouping::Sheet,
                })), "Select a cell to inspect.")
                    .and_then(|answer| Ok((evidence(answer.body)?, coverage(answer.coverage))));
                match data {
                    Ok((graph, limit)) => {
                        let has_names = graph.nodes.iter().any(|node| matches!(node, DependencyTarget::Source(SourceRef::Name { .. })));
                        let query = has_names.then(|| query::Inventory { kinds: vec![InventoryKind::Names] });
                        let names = result(names.at(query), "").map(|answer| answer.body).unwrap_or_default();
                        graph_view(session, graph, names, limit)
                    }
                    Err(message) => {
                        names.at(None);
                        status(message)
                    }
                }
            }}
        </div>
    }
}

fn target_label(session: Session, target: &DependencyTarget) -> String {
    match target {
        DependencyTarget::Source(SourceRef::Cells { areas, .. }) => areas
            .iter()
            .copied()
            .map(|area| area_label(session, area))
            .collect::<Vec<_>>()
            .join(", "),
        DependencyTarget::Source(SourceRef::Name { name, scope }) => match scope {
            calc_skin::NameScope::Sheet(sheet) => {
                format!("{}!{name}", sheet_label(session, *sheet))
            }
            _ => name.clone(),
        },
        DependencyTarget::Source(SourceRef::Sheet(sheet)) => sheet_label(session, *sheet),
        DependencyTarget::SheetSpan { area, last } => format!(
            "{} through {}",
            area_label(session, *area),
            sheet_label(session, *last)
        ),
        DependencyTarget::Source(source) => format!("{source:?}"),
    }
}

fn graph_view(
    session: Session,
    graph: Dependencies,
    names: Vec<InventoryItem>,
    limit: Option<String>,
) -> AnyView {
    const NODES: usize = 200;
    const EDGES: usize = 400;
    let labels: Vec<_> = graph
        .nodes
        .iter()
        .take(NODES)
        .map(|target| target_label(session, target))
        .collect();
    let named_formulas = graph.nodes.iter().take(NODES).filter_map(|target| {
        let DependencyTarget::Source(SourceRef::Name { scope, name }) = target else { return None; };
        names.iter().find_map(|item| match item {
            InventoryItem::Name { scope: owner, name: label, formula, .. } if owner == scope && label == name =>
                Some(view! { <li data-inspector="name-evidence"><strong>{name.clone()}</strong>" " <code>{formula.clone()}</code></li> }),
            _ => None,
        })
    }).collect_view();
    let edges: Vec<_> = graph
        .edges
        .iter()
        .filter(|edge| edge.from < labels.len() && edge.to < labels.len())
        .take(EDGES)
        .collect();
    let grouped = graph
        .sheets
        .into_iter()
        .map(|group| {
            let title = format!(
                "{} · {} references",
                sheet_label(session, group.sheet),
                group.members.len()
            );
            let members = group
                .members
                .into_iter()
                .filter_map(|i| labels.get(i))
                .map(|name| view! { <li>{name.clone()}</li> })
                .collect_view();
            view! { <details><summary>{title}</summary><ul>{members}</ul></details> }
        })
        .collect_view();
    view! {
        <p>"Dependency explanation; execution not recorded"</p>
        <p>{format!("{} references · {} edges", graph.nodes.len(), graph.edges.len())}</p>
        {limit.map(status)}
        {(labels.len() < graph.nodes.len() || edges.len() < graph.edges.len()).then(|| view! {
            <p>{format!("Showing {} references and {} edges at this depth.", labels.len(), edges.len())}</p>
        })}
        {(!graph.unbound.is_empty()).then(|| view! { <p>{format!("{} cells await binding.", graph.unbound.len())}</p> })}
        <ul class="dna-inspector-names">{named_formulas}</ul>
        {grouped}
        <ol class="dna-inspector-edges">
            {edges.into_iter().map(|edge| {
                let from = labels[edge.from].clone();
                let to = labels[edge.to].clone();
                let target = match &graph.nodes[edge.to] {
                    DependencyTarget::Source(SourceRef::Cells { areas, .. }) => areas.first().map(|area| area.0),
                    _ => None,
                };
                view! { <li><span>{from}</span><span aria-label=format!("{:?}", edge.kind)>"→"</span>
                    {match target {
                        Some(at) => reveal(session, at).into_any(),
                        None => to.into_any(),
                    }}
                    <small>{format!("{:?}", edge.kind)}</small></li> }
            }).collect_view()}
        </ol>
        {graph.edges.is_empty().then(|| view! { <p>"No bound edges at this depth."</p> })}
        {button(session, "inspector.back", "Previous cell")}
    }.into_any()
}

pub(crate) fn calculation(session: Session) -> impl IntoView {
    let calc = session.following::<query::CalcState>();
    view! {
        <div class="dna-inspector-card">
            <h2>"Calculation state"</h2>
            {move || match result(calc.at(Some(query::CalcState)), "") {
                Ok(answer) => view! {
                    <p>{match answer.body.mode {
                        query::CalcMode::Automatic => "Automatic",
                        query::CalcMode::AutomaticExceptTables => "Automatic except data tables",
                        query::CalcMode::Manual => "Manual",
                    }} " · " {match answer.body.activity {
                        query::CalcActivity::Done => "Current",
                        query::CalcActivity::Pending => "Pending",
                    }}</p>
                    {answer.body.circular.map(|at| view! { <p>"Circular reference · " {reveal(session, at)}</p> })}
                }.into_any(),
                Err(message) => status(message),
            }}
            {button(session, "calc.calculate", "Calculate workbook")}
        </div>
        {dependency_waves(session)}
        <div class="dna-inspector-card"><h2>"Value changes"</h2><p>"Value differences are unavailable."</p></div>
        <div class="dna-inspector-card"><h2>"DNA Calc's calculation order"</h2><p>"Not observed"</p></div>
    }
}

pub(crate) fn sources(session: Session) -> impl IntoView {
    let sources = session.following::<query::SourceInventory>();
    view! {
        {crate::live::recording(session)}
        <div class="dna-inspector-card"><h2>"Live sources"</h2>
            {move || match result(sources.at(Some(query::SourceInventory)), "") {
                Ok(answer) => {
                    let sources = answer.body;
                    view! {
                        <p>{format!("{} RTD topics · {} linked workbooks · {} volatile cells", sources.topics.len(), sources.links.len(), sources.volatile_cells.len())}</p>
                        {coverage(answer.coverage).map(status)}
                        <ul>{sources.topics.into_iter().map(|topic| {
                            let watch = crate::live::topic_button(session, &topic);
                            view! {
                                <li><strong>{topic.progid}</strong>" " {topic.strings.join(" / ")}" · " {topic.last.value.display()}
                                    <small>{format!("Lease {} · sequence {}", topic.lease, topic.sequence)}</small>
                                    {watch}
                                </li>
                            }
                        }).collect_view()}</ul>
                        <ul>{sources.links.into_iter().map(|link| view! {
                            <li>{link.file_name.unwrap_or_else(|| format!("External link {}", link.index))}
                                <small>{format!("{} cached sheets · {} refresh errors", link.cached_sheets, link.refresh_error_sheets)}</small></li>
                        }).collect_view()}</ul>
                        <ul>{sources.volatile_cells.into_iter().map(|at| view! { <li>{reveal(session, at)}</li> }).collect_view()}</ul>
                    }.into_any()
                }
                Err(message) => status(message),
            }}
        </div>
    }
}

fn dependency_waves(session: Session) -> impl IntoView {
    let watch = session.following::<query::Deps>();
    view! {
        <div class="dna-inspector-card">
            <h2>"Dependency waves"</h2>
            {move || {
                let source = session.active().map(crate::cell_source);
                let graph = result(watch.at(source.map(|of| query::Deps {
                    of, dir: DependencyDirection::Precedents, depth: 5, group: DependencyGrouping::Reference,
                })), "Select a cell to inspect.")
                    .and_then(|answer| Ok((evidence(answer.body)?, coverage(answer.coverage))));
                let (graph, limit) = match graph {
                    Ok(graph) => graph,
                    Err(message) => return status(message),
                };
                if graph.nodes.len() > 200 || graph.edges.len() > 400 {
                    return status("The selected cell's graph exceeds 200 references or 400 edges.".into());
                }
                let (waves, held) = frontiers(&graph);
                view! {
                    <p>"Derived from up to five levels of precedents"</p>
                    {limit.map(status)}
                    <ol>{waves.into_iter().map(|wave| view! {
                        <li>
                            <details>
                                <summary>{format!("{} references", wave.len())}</summary>
                                <ul>{wave.into_iter().map(|node| view! {
                                    <li>{target_label(session, &graph.nodes[node])}</li>
                                }).collect_view()}</ul>
                            </details>
                        </li>
                    }).collect_view()}</ol>
                    {(held > 0).then(|| view! { <p>{format!("{held} references in or downstream of a cycle in this graph.")}</p> })}
                }.into_any()
            }}
        </div>
    }
}

fn frontiers(graph: &Dependencies) -> (Vec<Vec<usize>>, usize) {
    let count = graph.nodes.len();
    let mut reads = vec![0; count];
    let mut readers = vec![Vec::new(); count];
    for edge in &graph.edges {
        reads[edge.from] += 1;
        readers[edge.to].push(edge.from);
    }
    let mut ready: Vec<_> = (0..count).filter(|&node| reads[node] == 0).collect();
    let mut waves = Vec::new();
    let mut reached = 0;
    while !ready.is_empty() {
        let mut next = Vec::new();
        ready.sort_unstable();
        for &predecessor in &ready {
            for &reader in &readers[predecessor] {
                reads[reader] -= 1;
                if reads[reader] == 0 {
                    next.push(reader);
                }
            }
        }
        reached += ready.len();
        waves.push(ready);
        ready = next;
    }
    (waves, count - reached)
}
