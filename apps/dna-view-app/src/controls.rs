//! Properties of an authored page control. All edits use the workbook presentation history.

use super::{State, argument, button, cell_source, encode, qualified, unused_anchors};
use calc_skin::{Bound, DocId, Page, PageNode, PresentationChange, ViewId, WorkbookId};
use dna_kit::{Live, Session, ViewCommand};
use leptos::prelude::*;

pub(super) const SELECT: &str = "forms.select-part";

#[derive(Clone)]
struct Selection {
    workbook: WorkbookId,
    document: DocId,
    part: String,
}

#[derive(Clone, Copy)]
struct Editor(RwSignal<Option<Selection>>);

impl Default for Editor {
    fn default() -> Self {
        Self(RwSignal::new(None))
    }
}

fn selection(session: Session) -> RwSignal<Option<Selection>> {
    session.local::<Editor>().0
}

fn selected(session: Session, document: DocId) -> Option<Selection> {
    let workbook = session.stamp()?.workbook;
    selection(session)
        .get()
        .filter(|selection| selection.workbook == workbook && selection.document == document)
}

pub(super) fn is_selected(session: Session, document: DocId, part: &str) -> bool {
    selected(session, document).is_some_and(|selection| selection.part == part)
}

pub(super) fn commands(state: State, view: &ViewId) -> Vec<ViewCommand> {
    let session = state.session;
    let _ = selection(session);
    let choose = ViewCommand::new(view, SELECT, "Select a control", &[], move |invocation| {
        let id = invocation
            .args
            .as_str()
            .ok_or_else(|| "Choose a control.".to_owned())?;
        let (page, doc) = state.document()?;
        let workbook = session
            .stamp()
            .ok_or_else(|| "Open a workbook first.".to_owned())?
            .workbook;
        path_of(&page, id)?;
        selection(session).set(Some(Selection {
            workbook,
            document: doc,
            part: id.into(),
        }));
        Ok(())
    });
    let label = ViewCommand::new(
        view,
        "forms.label-part",
        "Set control label",
        &[],
        move |invocation| {
            let label = invocation
                .args
                .as_str()
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .ok_or_else(|| "Give the control a label.".to_owned())?;
            edit(state, "Set control label", |page, path| {
                let PageNode::Part { props, .. } = at_path(&mut page.root, path) else {
                    unreachable!()
                };
                props.insert("label".into(), serde_json::Value::String(label.into()));
                Ok(Vec::new())
            })
        },
    )
    .when("writable");
    let shift = |id, label, offset| {
        ViewCommand::new(view, id, label, &[], move |_| {
            edit(state, label, |page, path| {
                let Some((Step::Child(index), parent)) = path.split_last() else {
                    return Err("This control has no sibling in its layout.".into());
                };
                let children =
                    child_nodes(at_path(&mut page.root, parent)).expect("known child path");
                let to = index
                    .checked_add_signed(offset)
                    .filter(|&to| to < children.len())
                    .ok_or_else(|| {
                        "The control is already at that edge of its layout.".to_owned()
                    })?;
                children.swap(*index, to);
                Ok(Vec::new())
            })
        })
        .when("writable")
    };
    let remove = ViewCommand::new(
        view,
        "forms.remove-part",
        "Remove control",
        &[],
        move |_| {
            edit(state, "Remove control", |page, path| {
                if let Some((Step::Child(index), parent)) = path.split_last() {
                    child_nodes(at_path(&mut page.root, parent))
                        .expect("known child path")
                        .remove(*index);
                } else {
                    *at_path(&mut page.root, path) = PageNode::Stack {
                        children: Vec::new(),
                    };
                }
                Ok(Vec::new())
            })
        },
    )
    .when("writable");
    let rebind = ViewCommand::new(
        view,
        "forms.rebind-part",
        "Bind to selected cell",
        &[],
        move |_| {
            edit(state, "Bind control to selected cell", |page, path| {
                let PageNode::Part {
                    component, bind, ..
                } = at_path(&mut page.root, path)
                else {
                    unreachable!()
                };
                if component != "value" && component != "input" {
                    return Err("Choose an input or value control to bind one cell.".into());
                }
                let at = session
                    .active()
                    .ok_or_else(|| "Select a worksheet cell.".to_owned())?;
                let address = qualified(session, at)
                    .ok_or_else(|| "The source worksheet is unavailable.".to_owned())?;
                if component == "input"
                    && session
                        .active_entry()
                        .ok_or_else(|| "Reading the selected cell…".to_owned())?
                        .formula
                {
                    return Err("Choose a literal cell for an input control.".into());
                }
                let Live::Ready(part) = session.view_part() else {
                    return Err("Reading source anchors…".into());
                };
                let anchor = unused_anchors(&part.body)
                    .next()
                    .expect("fewer anchors than identifiers");
                bind.insert("value".into(), Bound { anchor, address });
                Ok(vec![PresentationChange::Bind {
                    anchor,
                    source: cell_source(at),
                }])
            })
        },
    )
    .when("writable");
    vec![
        choose,
        label,
        shift("forms.move-part-up", "Move control earlier", -1),
        shift("forms.move-part-down", "Move control later", 1),
        remove,
        rebind,
    ]
}

fn edit(
    state: State,
    label: &str,
    apply: impl FnOnce(&mut Page, &[Step]) -> Result<Vec<PresentationChange>, String>,
) -> Result<(), String> {
    let (stamp, doc) = state.snapshot()?;
    let doc = doc.ok_or_else(|| "Create an App page first.".to_owned())?;
    let selection =
        selected(state.session, doc.id).ok_or_else(|| "Select a page control first.".to_owned())?;
    let mut page = super::decode(&doc)?;
    let path = path_of(&page, &selection.part)?;
    let mut changes = apply(&mut page, &path)?;
    changes.push(PresentationChange::PatchDoc {
        doc: doc.id,
        expect_view_rev: stamp.view,
        body: encode(&page)?,
    });
    state.submit(stamp, label, changes)
}

#[derive(Clone, Copy)]
enum Step {
    Child(usize),
    Tab(usize),
}

fn path_of(page: &Page, id: &str) -> Result<Vec<Step>, String> {
    let mut occurrences = 0;
    page.root
        .for_each_part(&mut |part, _, _| occurrences += usize::from(part == id));
    if occurrences != 1 {
        return Err("Choose a control with a unique ID in this page.".into());
    }
    let mut path = Vec::new();
    find_path(&page.root, id, &mut path);
    Ok(path)
}

fn find_path(node: &PageNode, id: &str, path: &mut Vec<Step>) -> bool {
    match node {
        PageNode::Part { id: part, .. } => part == id,
        PageNode::Stack { children }
        | PageNode::Row { children }
        | PageNode::Grid { children, .. }
        | PageNode::Section { children, .. } => children.iter().enumerate().any(|(i, child)| {
            path.push(Step::Child(i));
            if find_path(child, id, path) {
                return true;
            }
            path.pop();
            false
        }),
        PageNode::Tabs { tabs } => tabs.iter().enumerate().any(|(i, tab)| {
            path.push(Step::Tab(i));
            if find_path(&tab.body, id, path) {
                return true;
            }
            path.pop();
            false
        }),
    }
}

fn child_nodes(node: &mut PageNode) -> Option<&mut Vec<PageNode>> {
    match node {
        PageNode::Stack { children }
        | PageNode::Row { children }
        | PageNode::Grid { children, .. }
        | PageNode::Section { children, .. } => Some(children),
        _ => None,
    }
}

fn at_path<'a>(node: &'a mut PageNode, path: &[Step]) -> &'a mut PageNode {
    let Some((step, rest)) = path.split_first() else {
        return node;
    };
    let child = match step {
        Step::Child(i) => &mut child_nodes(node).expect("known child path")[*i],
        Step::Tab(i) => {
            let PageNode::Tabs { tabs } = node else {
                unreachable!()
            };
            &mut tabs[*i].body
        }
    };
    at_path(child, rest)
}

pub(super) fn inspector(session: Session, document: DocId, page: Page) -> impl IntoView {
    let label_field = NodeRef::<leptos::html::Input>::new();
    move || {
        let picked = selected(session, document)?;
        let mut current = page.clone();
        let path = path_of(&current, &picked.part).ok()?;
        let PageNode::Part {
            component,
            bind,
            props,
            ..
        } = at_path(&mut current.root, &path)
        else {
            return None;
        };
        let label = props
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or(&picked.part)
            .to_owned();
        let address = bind.get("value").map(|bound| bound.address.clone());
        let can_rebind = component == "input" || component == "value";
        Some(view! {
            <aside class="dna-page-card" aria-label="Control properties" data-dnacalc="control-properties">
                <form on:submit=move |event: leptos::ev::SubmitEvent| {
                    event.prevent_default();
                    if let Some(field) = label_field.get() { argument(session, "forms.label-part", field.value()); }
                }>
                    <label>"Control label " <input node_ref=label_field required prop:value=label/></label>
                    <button type="submit" data-command="forms.label-part">"Save label"</button>
                </form>
                <p>{address}</p>
                {can_rebind.then(|| button(session, "forms.rebind-part", "Bind to selected cell"))}
                {button(session, "forms.move-part-up", "Move earlier")}
                {button(session, "forms.move-part-down", "Move later")}
                {button(session, "forms.remove-part", "Remove control")}
            </aside>
        })
    }
}
