//! Keyboard navigation through the bounded dependency query.

use crate::audit::target_label;
use calc_skin::{
    Dependencies, DependencyDirection, DependencyTarget, SourceRef, WorkbookId, grid::Key,
};
use dna_kit::{Invocation, Session, ViewCommand, commands::CommandId};
use leptos::prelude::*;
use std::collections::VecDeque;
use std::sync::Arc;

pub(crate) const COMPONENT: &str = "inspector-dependencies";

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Scope {
    pub workbook: WorkbookId,
    pub source: SourceRef,
    pub direction: DependencyDirection,
}

struct Row {
    target: DependencyTarget,
    parent: Option<usize>,
    children: Vec<usize>,
    level: usize,
    operands: Vec<String>,
}

struct Tree {
    scope: Scope,
    roots: Vec<usize>,
    rows: Vec<Row>,
    order: Vec<usize>,
}

impl Tree {
    fn new(scope: Scope, graph: &Dependencies, shown: usize) -> Self {
        let mut rows = graph
            .nodes
            .iter()
            .take(shown)
            .map(|target| Row {
                target: target.clone(),
                parent: None,
                children: Vec::new(),
                level: 0,
                operands: Vec::new(),
            })
            .collect::<Vec<_>>();
        let mut edges = vec![Vec::new(); rows.len()];
        for edge in &graph.edges {
            if edge.from < rows.len() && edge.to < rows.len() {
                edges[edge.from].push(edge.to);
            }
        }
        let roots = graph
            .roots
            .iter()
            .copied()
            .filter(|node| *node < rows.len())
            .collect::<Vec<_>>();
        let mut queue = VecDeque::from(roots.clone());
        for &root in &roots {
            rows[root].level = 1;
        }
        while let Some(parent) = queue.pop_front() {
            for &child in &edges[parent] {
                // The first path supplies a tree parent. The graph below still shows every edge.
                if rows[child].level == 0 {
                    rows[child].level = rows[parent].level + 1;
                    rows[child].parent = Some(parent);
                    rows[child].operands.extend(graph.references.iter().filter(|reference| {
                        reference.edge.from == parent && reference.edge.to == child
                    }).map(|reference| reference.spelling.clone()));
                    rows[parent].children.push(child);
                    queue.push_back(child);
                }
            }
        }
        let mut order = Vec::new();
        let mut stack = roots.iter().rev().copied().collect::<Vec<_>>();
        while let Some(node) = stack.pop() {
            order.push(node);
            stack.extend(rows[node].children.iter().rev().copied());
        }
        Self {
            scope,
            roots,
            rows,
            order,
        }
    }

    fn index(&self, target: &DependencyTarget) -> Option<usize> {
        self.order
            .iter()
            .copied()
            .find(|&node| self.rows[node].target == *target)
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Position {
    scope: Option<Scope>,
    cursor: Option<DependencyTarget>,
    expanded: Vec<DependencyTarget>,
}

impl Position {
    fn at(&self, tree: &Tree) -> Option<usize> {
        if self.scope.as_ref() == Some(&tree.scope)
            && let Some(at) = self.cursor.as_ref().and_then(|target| tree.index(target))
            && self.visible(tree, at)
        {
            return Some(at);
        }
        tree.roots.first().copied()
    }

    fn open(&self, tree: &Tree, node: usize) -> bool {
        self.scope.as_ref() == Some(&tree.scope) && self.expanded.contains(&tree.rows[node].target)
    }

    fn visible(&self, tree: &Tree, mut node: usize) -> bool {
        while let Some(parent) = tree.rows[node].parent {
            if !self.open(tree, parent) {
                return false;
            }
            node = parent;
        }
        true
    }

    fn select_scope(&mut self, scope: &Scope) {
        if self.scope.as_ref() != Some(scope) {
            *self = Self {
                scope: Some(scope.clone()),
                ..Default::default()
            };
        }
    }
}

#[derive(Clone, Copy, Default)]
struct State {
    tree: StoredValue<Option<Arc<Tree>>>,
    position: RwSignal<Position>,
}

#[derive(Clone, Copy)]
enum Move {
    Open,
    Close,
    Previous,
    Next,
    Reveal,
    Toggle,
}

pub(crate) fn commands(session: Session) -> Vec<ViewCommand> {
    let state = session.local::<State>();
    [
        (
            "inspector.dependency-open",
            "Expand dependency",
            &["ArrowRight"][..],
            Move::Open,
        ),
        (
            "inspector.dependency-close",
            "Collapse dependency",
            &["ArrowLeft"][..],
            Move::Close,
        ),
        (
            "inspector.dependency-previous",
            "Previous dependency sibling",
            &["ArrowUp"][..],
            Move::Previous,
        ),
        (
            "inspector.dependency-next",
            "Next dependency sibling",
            &["ArrowDown"][..],
            Move::Next,
        ),
        (
            "inspector.dependency-reveal",
            "Reveal dependency source",
            &["Enter"][..],
            Move::Reveal,
        ),
        (
            "inspector.dependency-toggle",
            "Toggle dependency branch",
            &[][..],
            Move::Toggle,
        ),
    ]
    .into_iter()
    .map(|(id, label, chords, action)| {
        ViewCommand::global(id, label, chords, move |invocation| {
            step(session, state, action, invocation)
        })
        .when("!editing && !mode:key-tips")
        .in_component(COMPONENT)
    })
    .collect()
}

fn step(
    session: Session,
    state: State,
    action: Move,
    invocation: &Invocation,
) -> Result<(), String> {
    let tree = state
        .tree
        .get_value()
        .ok_or("Inspect a cell's dependencies first.")?;
    if !session
        .stamp()
        .is_some_and(|stamp| stamp.workbook == tree.scope.workbook)
        || !session
            .selection()
            .is_some_and(|selection| selection.target == tree.scope.source)
    {
        return Err("The inspected source changed.".into());
    }
    let mut position = state.position.get_untracked();
    position.select_scope(&tree.scope);
    let current = if invocation.args.is_null() {
        position.at(&tree).ok_or("Select a dependency.")?
    } else {
        let target =
            serde_json::from_value(invocation.args.clone()).map_err(|_| "Select a dependency.")?;
        tree.index(&target).ok_or("The dependency changed.")?
    };
    let row = tree
        .rows
        .get(current)
        .filter(|row| row.level > 0)
        .ok_or("Select a dependency.")?;
    if !position.visible(&tree, current) {
        return Err("Expand the containing dependency first.".into());
    }
    let mut to = current;
    let open = position.open(&tree, current);
    match action {
        Move::Open if open => to = row.children.first().copied().unwrap_or(current),
        Move::Open | Move::Toggle if !open && !row.children.is_empty() => {
            position.expanded.push(row.target.clone())
        }
        Move::Close if !open => to = row.parent.unwrap_or(current),
        Move::Close | Move::Toggle => position.expanded.retain(|target| target != &row.target),
        Move::Previous | Move::Next => {
            let siblings = row
                .parent
                .map_or(&tree.roots, |parent| &tree.rows[parent].children);
            let index = siblings
                .iter()
                .position(|node| *node == current)
                .unwrap_or(0);
            let next = match action {
                Move::Previous => index.checked_sub(1),
                _ => index.checked_add(1),
            };
            to = next
                .and_then(|index| siblings.get(index))
                .copied()
                .unwrap_or(current);
        }
        Move::Reveal => {
            position.cursor = Some(row.target.clone());
            state.position.set(position);
            if let Some(at) = source_cell(&row.target) {
                return session
                    .run(&Invocation {
                        command: CommandId::new("inspector.reveal"),
                        args: serde_json::to_value(at).expect("a cell key serializes"),
                    })
                    .map_err(|error| format!("{error:?}"));
            }
            return session
                .command("explorer.dock")
                .map_err(|error| format!("{error:?}"));
        }
        _ => {}
    }
    position.cursor = Some(tree.rows[to].target.clone());
    state.position.set(position);
    focus(to);
    Ok(())
}

fn source_cell(target: &DependencyTarget) -> Option<Key> {
    match target {
        DependencyTarget::Source(SourceRef::Cells { areas, .. }) => {
            areas.first().map(|area| area.0)
        }
        DependencyTarget::Source(SourceRef::Spill { anchor }) => Some(*anchor),
        DependencyTarget::SheetSpan { area, .. } => Some(area.0),
        _ => None,
    }
}

fn focus(index: usize) {
    #[cfg(target_arch = "wasm32")]
    {
        use leptos::wasm_bindgen::JsCast;
        let selector = format!("[data-inspector-dependency='{index}']");
        if let Ok(Some(row)) = leptos::prelude::document().query_selector(&selector)
            && let Ok(row) = row.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = row.focus();
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = index;
}

pub(crate) fn mount(session: Session, scope: Scope, graph: &Dependencies, shown: usize) -> AnyView {
    let state = session.local::<State>();
    let tree = Arc::new(Tree::new(scope, graph, shown));
    state.tree.set_value(Some(tree.clone()));
    let mounted = tree.clone();
    on_cleanup(move || {
        state.tree.try_update_value(|current| {
            if current
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &mounted))
            {
                *current = None;
            }
        });
    });
    let rows = tree.order.iter().copied().map(|node| {
        let row = &tree.rows[node];
        let target = target_label(session, &row.target);
        let label = if row.operands.is_empty() {
            target
        } else {
            format!("{} · {target}", row.operands.join(", "))
        };
        let level = row.level;
        let branch = !row.children.is_empty();
        // A row can render while its dock's query owner is being replaced by the bench.
        let exposed = {
            let tree = tree.clone();
            move || state.position.with(|position| position.visible(&tree, node))
        };
        let open = {
            let tree = tree.clone();
            move || state.position.with(|position| position.open(&tree, node))
        };
        let icon_open = open.clone();
        let label_open = open.clone();
        let current = {
            let tree = tree.clone();
            move || state.position.with(|position| position.at(&tree) == Some(node))
        };
        let selected = current.clone();
        let args = serde_json::to_value(&row.target).expect("a dependency target serializes");
        let invoke = move |id| { let _ = session.run(&Invocation {
            command: CommandId::new(id),
            args: args.clone(),
        }); };
        let toggle = invoke.clone();
        view! {
            <li role="treeitem" data-inspector-dependency=node
                aria-selected=move || selected().to_string()
                aria-expanded=move || branch.then(|| open().to_string())
                tabindex=move || if current() { 0 } else { -1 }
                hidden=move || !exposed()>
                {branch.then(|| view! {
                    <button type="button" tabindex="-1" data-command="inspector.dependency-toggle"
                        aria-label=move || if label_open() { "Collapse dependencies" } else { "Expand dependencies" }
                        on:click=move |_| toggle("inspector.dependency-toggle")>
                        {move || if icon_open() { "▾" } else { "▸" }}
                    </button>
                })}
                <button type="button" tabindex="-1" data-command="inspector.dependency-reveal"
                    on:click=move |_| invoke("inspector.dependency-reveal")>{label}</button>
            </li>
        }.attr("aria-level", level.to_string())
    }).collect_view();
    view! {
        <ul role="tree" aria-label="Dependency tree" class="dna-inspector-dependency-tree"
            data-dna-key-component=COMPONENT>{rows}</ul>
    }
    .into_any()
}
