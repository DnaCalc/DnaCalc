use super::*;
use calc_skin::{Batch, Change, ModelChange, NameChange, NameScope};

pub(super) fn change(change: NameChange) -> Change {
    Change::Model(ModelChange::Name {
        scope: NameScope::Workbook,
        change,
    })
}

pub(super) fn pin_workbook_revision(
    session: Session,
    snapshot: &Snapshot,
    batch: &mut Batch,
) -> Result<(), String> {
    let saved = snapshot.part.stamp;
    let current = session
        .stamp()
        .filter(|stamp| stamp.workbook == saved.workbook && stamp.view == saved.view)
        .ok_or("The Notebook changed. Review the cell again.")?;
    batch.base = Some(current.model);
    batch.expects.push(SourceRef::Workbook);
    Ok(())
}

pub(super) fn name_output(state: State, name: String) -> Result<(), String> {
    let snapshot = state.snapshot()?;
    let index = state
        .selected(&snapshot)
        .ok_or("Choose a Notebook formula first.")?;
    let mut chain = snapshot.chain.clone();
    let item = &mut chain.cells[index];
    if !item.owns_cell() || !matches!(item.kind, Kind::Formula) {
        return Err("Choose a formula owned by this Notebook.".into());
    }
    let name = name.trim().to_owned();
    let action = if let Some(previous) = &item.output_name {
        if *previous == name {
            return Ok(());
        }
        NameChange::Rename {
            name: previous.clone(),
            to: name.clone(),
        }
    } else {
        let source = snapshot
            .source(item.id)
            .ok_or("The worksheet source is unavailable.")?;
        let at = first_cell(source).ok_or("Choose a worksheet formula.")?;
        let summary = state.session.summary().ok_or("Loading worksheets…")?;
        let worksheet = summary
            .sheets
            .iter()
            .find(|s| s.sheet == Sheet(at.0, at.1))
            .ok_or("The worksheet is unavailable.")?;
        let sheet = calc_skin::address::quoted_sheet(&worksheet.name);
        let column = calc_skin::address::column_letters(at.3);
        let spill = if matches!(source, SourceRef::Spill { .. }) {
            "#"
        } else {
            ""
        };
        let formula = format!("={sheet}!${column}${}{spill}", at.2);
        NameChange::Define {
            name: name.clone(),
            formula,
        }
    };
    item.output_name = Some(name);
    let mut batch = state.prepare_batch(
        &snapshot,
        &chain,
        vec![change(action)],
        "Name Notebook output",
    )?;
    pin_workbook_revision(state.session, &snapshot, &mut batch)?;
    state.session.submit(batch);
    Ok(())
}

pub(super) fn define_lambda(state: State, name: String, formula: String) -> Result<(), String> {
    let snapshot = state.snapshot()?;
    let mut chain = snapshot.chain.clone();
    let id = AnchorId(snapshot.next_id()?);
    let index = state.selected(&snapshot).map_or(0, |index| index + 1);
    let name = name.trim().to_owned();
    chain.cells.insert(
        index,
        Item::new(
            id,
            Kind::Function {
                name: name.clone(),
                owns_name: true,
            },
        ),
    );
    let definition = change(NameChange::DefineFunction { name, formula });
    let mut batch = state.prepare_batch(
        &snapshot,
        &chain,
        vec![definition],
        "Define Notebook function",
    )?;
    pin_workbook_revision(state.session, &snapshot, &mut batch)?;
    state.session.submit_then(batch, move || {
        state.function_form.set(None);
        if state.session.view().0 == ID {
            state.select(snapshot.part.stamp, id, None);
        }
    });
    Ok(())
}

pub(super) fn live_function(session: Session, name: String) -> impl IntoView {
    let observed = session.following::<query::DefinedName>();
    let read = Memo::new(move |_| {
        observed.at(Some(query::DefinedName {
            scope: NameScope::Workbook,
            name: name.clone(),
        }))
    });
    view! {
        <strong>{move || match read.get() {
            Live::Ready(answer) => answer.body.parameters
                .map(|parameters| format!("{}({})", answer.body.definition.name, parameters.join(", ")))
                .unwrap_or_else(|| "Name no longer defines a function".into()),
            Live::Refused(_) => "Workbook function is unavailable".into(),
            _ => "Loading workbook function…".into(),
        }}</strong>
        <pre data-notebook-function><code>{move || {
            if let Live::Ready(answer) = read.get() {
                answer.body.definition.formula
            } else {
                String::new()
            }
        }}</code></pre>
    }
}

pub(super) fn output_name_form(
    state: State,
    id: AnchorId,
    current: Option<String>,
) -> impl IntoView {
    let name = NodeRef::<leptos::html::Input>::new();
    view! {
        <form class="dna-notebook-tools" on:submit=move |event| {
            event.prevent_default();
            if let Some(name) = name.get_untracked() {
                super::commands::invoke(state.session, "notebook.focus", id);
                super::commands::invoke(state.session, "notebook.name-output", name.value());
            }
        }>
            <label>"Output name" <input type="text" value=current.unwrap_or_default() node_ref=name /></label>
            <button type="submit" data-command="notebook.name-output">"Save name"</button>
        </form>
    }
}

pub(super) fn lambda_form(state: State) -> impl IntoView {
    let name = NodeRef::<leptos::html::Input>::new();
    let formula = NodeRef::<leptos::html::Textarea>::new();
    view! {
        <Show when=move || state.function_form.get().is_some_and(|book|
            state.session.stamp().is_some_and(|stamp| stamp.workbook == book)
        )>
            <form class="dna-notebook-tools" aria-label="Define function" on:submit=move |event| {
                event.prevent_default();
                if let (Some(name), Some(formula)) = (name.get_untracked(), formula.get_untracked()) {
                    super::commands::invoke(state.session, "notebook.author-lambda", (name.value(), formula.value()));
                }
            }>
                <label>"Workbook name" <input type="text" node_ref=name /></label>
                <label>"LAMBDA formula" <textarea node_ref=formula></textarea></label>
                <button type="submit" data-command="notebook.author-lambda">"Define function"</button>
                {stage::button(state.session, "notebook.cancel-function", "Cancel")}
            </form>
        </Show>
    }
}
