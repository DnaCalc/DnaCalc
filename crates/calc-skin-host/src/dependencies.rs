use crate::{
    State, batch::source_area, invalid, inventory::name_scope, query::QueryWork,
    query::source_areas,
};
use calc_host::value::{CellKey, GridArea, NameId};
use calc_host::{HostState, Prerequisite};
use calc_skin::grid::{Area, Key, Sheet};
use calc_skin::{
    Dependencies, DependencyDirection, DependencyEdge, DependencyGrouping, DependencyKind,
    DependencySheetGroup, DependencyTarget, NameScope, Refusal, SourceRef,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Node {
    Area(GridArea),
    Name(NameId),
    Span(calc_host::SheetSpan),
}

pub(crate) fn deps(
    state: &State,
    of: &SourceRef,
    dir: DependencyDirection,
    depth: u16,
    group: DependencyGrouping,
    meter: &mut QueryWork,
) -> Result<Dependencies, Refusal> {
    let host = state.session.host();
    let roots = match of {
        SourceRef::Name { scope, name } => {
            vec![Node::Name(lookup_name(host, scope, name)?)]
        }
        scope => source_areas(state, scope)?
            .into_iter()
            .map(Node::Area)
            .collect(),
    };
    let mut graph = Graph {
        host,
        meter,
        output: Dependencies::default(),
        ids: HashMap::new(),
        queue: VecDeque::new(),
        edges: BTreeSet::new(),
    };
    for root in roots {
        if !graph.ids.contains_key(&root) {
            let id = graph.add(root, depth)?;
            graph.output.roots.push(id);
        }
    }
    while let Some((subject, id, left)) = graph.queue.pop_front() {
        graph.meter.work(0, 0, 0)?;
        if left == 0 || left < graph.ids[&subject].1 {
            continue;
        }
        match dir {
            DependencyDirection::Precedents => graph.precedents(subject, id, left - 1)?,
            DependencyDirection::Dependents => graph.dependents(subject, id, left - 1)?,
        }
    }
    graph.output.unbound.sort_unstable();
    graph.output.unbound.dedup();
    if group == DependencyGrouping::Sheet {
        let mut groups: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (id, target) in graph.output.nodes.iter().enumerate() {
            if let Some(sheet) = sheet_of(target) {
                groups.entry(sheet).or_default().push(id);
            }
        }
        graph.output.sheets = groups
            .into_iter()
            .map(|(sheet, members)| DependencySheetGroup { sheet, members })
            .collect();
    }
    Ok(graph.output)
}

struct Graph<'a> {
    host: &'a HostState,
    meter: &'a mut QueryWork,
    output: Dependencies,
    ids: HashMap<Node, (usize, u16)>,
    queue: VecDeque<(Node, usize, u16)>,
    edges: BTreeSet<DependencyEdge>,
}

impl Graph<'_> {
    fn add(&mut self, node: Node, left: u16) -> Result<usize, Refusal> {
        if let Some((id, expanded)) = self.ids.get_mut(&node) {
            if left > *expanded {
                *expanded = left;
                self.queue.push_back((node, *id, left));
            }
            return Ok(*id);
        }
        self.meter.work(0, 1, 0)?;
        let target = match node {
            Node::Area(area) => DependencyTarget::Source(source_area(area)),
            Node::Name(id) => {
                let definition = self
                    .host
                    .name_node(id)
                    .ok_or_else(|| invalid("The defined name is unavailable"))?
                    .definition();
                DependencyTarget::Source(SourceRef::Name {
                    scope: name_scope(definition.scope),
                    name: definition.name.clone(),
                })
            }
            Node::Span(span) => DependencyTarget::SheetSpan {
                area: Area::of(span.target()),
                last: Sheet::of(span.last()),
            },
        };
        let id = self.output.nodes.len();
        self.output.nodes.push(target);
        self.ids.insert(node, (id, left));
        self.queue.push_back((node, id, left));
        Ok(id)
    }

    fn connect(
        &mut self,
        from: usize,
        target: Node,
        kind: DependencyKind,
        left: u16,
    ) -> Result<(), Refusal> {
        let to = self.add(target, left)?;
        let edge = DependencyEdge { from, to, kind };
        if self.edges.insert(edge) {
            self.meter.work(0, 0, 1)?;
            self.output.edges.push(edge);
        }
        Ok(())
    }

    fn precedents(&mut self, subject: Node, id: usize, left: u16) -> Result<(), Refusal> {
        match subject {
            Node::Area(area) if area.cell_count() == 1 => {
                let at = area.first();
                self.meter.work(1, 0, 0)?;
                let Some((placed, dynamic)) = self.host.dependency_inputs(at) else {
                    if self.host.holds_formula(at) {
                        self.output.unbound.push(Key::of(at));
                    }
                    return Ok(());
                };
                for (prerequisite, kind) in placed
                    .iter()
                    .map(|p| (p, DependencyKind::Placed))
                    .chain(dynamic.iter().map(|p| (p, DependencyKind::Dynamic)))
                {
                    let (related, kind) = match *prerequisite {
                        Prerequisite::Cell(at) => (Node::Area(GridArea::cell(at)), kind),
                        Prerequisite::Area(area) => (Node::Area(area), kind),
                        Prerequisite::AxisValue(axis) => (Node::Area(axis.area()), kind),
                        Prerequisite::SheetSpan(span) => (Node::Span(span), kind),
                        Prerequisite::Name(name) => (Node::Name(name), DependencyKind::Name),
                    };
                    self.connect(id, related, kind, left)?;
                }
            }
            Node::Area(area) => {
                for at in self.host.dependency_anchors(area) {
                    self.connect(
                        id,
                        Node::Area(GridArea::cell(at)),
                        DependencyKind::Member,
                        left + 1,
                    )?;
                }
            }
            Node::Span(span) => {
                for sheet in span.sheets(self.host.model()).into_iter().flatten() {
                    let area = on_sheet(span.target(), sheet.id());
                    self.connect(id, Node::Area(area), DependencyKind::Member, left + 1)?;
                }
            }
            Node::Name(name) => {
                for child in self.host.dependency_name_children(name) {
                    self.connect(id, Node::Name(child), DependencyKind::Name, left)?;
                }
            }
        }
        Ok(())
    }

    fn dependents(&mut self, subject: Node, id: usize, left: u16) -> Result<(), Refusal> {
        match subject {
            Node::Area(area) => {
                for at in self.host.dependency_readers(area) {
                    self.meter.work(0, 0, 0)?;
                    self.connect(
                        id,
                        Node::Area(GridArea::cell(at)),
                        DependencyKind::Reader,
                        left,
                    )?;
                }
            }
            Node::Name(name) => {
                for at in self.host.dependency_name_readers(name) {
                    self.connect(
                        id,
                        Node::Area(GridArea::cell(at)),
                        DependencyKind::Reader,
                        left,
                    )?;
                }
            }
            Node::Span(_) => {
                unreachable!("the reverse graph starts from worksheet areas and names")
            }
        }
        Ok(())
    }
}

fn lookup_name(host: &HostState, scope: &NameScope, name: &str) -> Result<NameId, Refusal> {
    let scope = match scope {
        NameScope::Workbook => calc_host::NameScope::Workbook,
        NameScope::Sheet(sheet) => calc_host::NameScope::Sheet(sheet.id()),
    };
    host.model()
        .definitions()
        .name_table()
        .lookup(scope, name)
        .map(|node| node.id())
        .ok_or_else(|| invalid("The defined name is unavailable"))
}

fn on_sheet(area: GridArea, sheet: calc_host::value::SheetId) -> GridArea {
    let first = area.first();
    let last = area.last();
    GridArea::new(
        CellKey::new(sheet, first.row(), first.column()).unwrap(),
        CellKey::new(sheet, last.row(), last.column()).unwrap(),
    )
    .unwrap()
}

fn sheet_of(target: &DependencyTarget) -> Option<Sheet> {
    match target {
        DependencyTarget::Source(SourceRef::Cells { sheet, .. })
        | DependencyTarget::Source(SourceRef::Name {
            scope: NameScope::Sheet(sheet),
            ..
        }) => Some(*sheet),
        _ => None,
    }
}
