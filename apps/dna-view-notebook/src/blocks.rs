use super::*;
use calc_skin::{AddressBook, Batch, Bound, Change, ModelChange, Notation};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(super) enum InputControl {
    Choice { options: Vec<String> },
    Integer { min: i32, max: i32 },
}

impl InputControl {
    fn is_configured(&self) -> bool {
        match self {
            Self::Choice { options } => {
                !options.is_empty() && options.iter().all(|entry| !entry.trim().is_empty())
            }
            Self::Integer { min, max } => min < max,
        }
    }

    fn accepts(&self, entry: &str) -> bool {
        match self {
            Self::Choice { options } => options.iter().any(|option| option == entry),
            Self::Integer { min, max } => entry
                .parse::<i32>()
                .is_ok_and(|value| (*min..=*max).contains(&value)),
        }
    }
}

fn source(session: Session, address: &str) -> Result<SourceRef, String> {
    let book = address_book(session).ok_or("Loading workbook sources…")?;
    SourceRef::parse(address, Notation::A1, &book)
        .map_err(|_| "Enter a qualified workbook source.".into())
}

fn address_book(session: Session) -> Option<AddressBook> {
    Some(AddressBook {
        sheets: session
            .summary()?
            .sheets
            .into_iter()
            .map(|sheet| (sheet.sheet, sheet.name))
            .collect(),
        ..AddressBook::default()
    })
}

pub(super) fn add_control(
    state: State,
    label: String,
    address: String,
    control: InputControl,
) -> Result<(), String> {
    let bound = source(state.session, &address)?;
    if !matches!(&bound, SourceRef::Cells { areas, .. } if matches!(areas.as_slice(), [Area(first,last)] if first == last))
    {
        return Err("Select one worksheet input.".into());
    }
    if label.trim().is_empty() || !control.is_configured() {
        return Err("Enter a label and valid options for the input.".into());
    }
    let snapshot = state.snapshot()?;
    state.add(
        &snapshot,
        Kind::Control {
            label,
            address,
            control,
        },
        Some(bound),
        true,
    )
}

pub(super) fn add_chart(state: State, address: String) -> Result<(), String> {
    let bound = source(state.session, &address)?;
    if !matches!(bound, SourceRef::Object { .. }) {
        return Err("Choose a workbook chart.".into());
    }
    let snapshot = state.snapshot()?;
    state.add(
        &snapshot,
        Kind::Chart {
            address,
            requires: None,
        },
        Some(bound),
        true,
    )
}

pub(super) fn inserter(state: State) -> impl IntoView {
    let session = state.session;
    view! {
        <Show when=move || state.inserter.get().is_some_and(|book|
            session.stamp().is_some_and(|stamp| stamp.workbook == book)
        )>
            <section aria-label="Insert Notebook block" class="dna-notebook-tools">
                {stage::button(session, "notebook.new-formula", "Formula")}
                {stage::button(session, "notebook.prose-below", "Markdown")}
                {stage::button(session, "notebook.new-function", "Function")}
                {control_form(session, false)}
                {control_form(session, true)}
                {chart_form(state)}
                {stage::button(session, "notebook.inserter", "Close inserter")}
            </section>
        </Show>
    }
}

fn control_form(session: Session, numeric: bool) -> impl IntoView {
    let label = NodeRef::<leptos::html::Input>::new();
    let address = NodeRef::<leptos::html::Input>::new();
    let options = NodeRef::<leptos::html::Input>::new();
    let min = NodeRef::<leptos::html::Input>::new();
    let max = NodeRef::<leptos::html::Input>::new();
    let add = if numeric {
        "Add slider"
    } else {
        "Add dropdown"
    };
    view! {
        <form aria-label=add class="dna-notebook-tools" on:submit=move |event| {
            event.prevent_default();
            let control = if numeric {
                let number = |field: NodeRef<leptos::html::Input>| field.get_untracked()?.value().parse::<i32>().ok();
                number(min).zip(number(max)).map(|(min, max)| InputControl::Integer { min, max })
            } else {
                options.get_untracked().map(|field| InputControl::Choice {
                    options: field.value().split(',').map(str::trim).map(str::to_owned).collect(),
                })
            };
            if let (Some(label), Some(address), Some(control)) = (label.get_untracked(), address.get_untracked(), control) {
                commands::invoke(session, "notebook.add-control", (label.value(), address.value(), control));
            } else {
                session.refuse(calc_skin::Refusal::Invalid { reason: "Enter all input settings.".into() });
            }
        }>
            <label>"Label" <input required node_ref=label /></label>
            <label>"Cell" <input required node_ref=address placeholder="Analysis!B2" /></label>
            {if numeric {
                view! {
                    <label>"Minimum" <input type="number" required node_ref=min /></label>
                    <label>"Maximum" <input type="number" required node_ref=max /></label>
                }.into_any()
            } else {
                view! { <label>"Comma-separated options" <input required node_ref=options /></label> }.into_any()
            }}
            <button type="submit" data-command="notebook.add-control">{add}</button>
        </form>
    }
}

pub(super) fn choose_chart_sheet(state: State, name: String) -> Result<(), String> {
    let workbook = state.session.stamp().map(|stamp| stamp.workbook)
        .filter(|workbook| state.inserter.get() == Some(*workbook))
        .ok_or("Open the block inserter first.")?;
    let sheet = state.session.summary()
        .and_then(|summary| summary.sheets.into_iter().find(|sheet| sheet.name == name))
        .ok_or("Choose a workbook worksheet.")?;
    state.chart_sheet.set(Some((workbook, sheet.sheet)));
    Ok(())
}

fn chart_form(state: State) -> impl IntoView {
    let session = state.session;
    let address = NodeRef::<leptos::html::Input>::new();
    let book = Memo::new(move |_| address_book(session));
    let worksheet = Memo::new(move |_| {
        let book = book.get()?;
        let active = state.chart_sheet.get()
            .filter(|(owner, _)| session.stamp().is_some_and(|stamp| stamp.workbook == *owner))
            .map(|(_, sheet)| sheet)
            .or_else(|| session.active().map(|at| Sheet(at.0, at.1)));
        book.sheets
            .iter()
            .find(|(sheet, _)| Some(*sheet) == active)
            .or_else(|| book.sheets.first())
            .map(|(sheet, _)| *sheet)
    });
    let objects = session.following::<query::Objects>();
    let charts = Memo::new(move |_| {
        let result = objects.at(worksheet.get().map(|sheet| query::Objects { sheet }));
        let (Some(book), Live::Ready(answer)) = (book.get(), result) else {
            return Vec::new();
        };
        answer
            .body
            .charts
            .into_iter()
            .filter_map(|chart| {
                let address = chart.source.display(&book)?;
                Some((address, chart.image.label))
            })
            .collect::<Vec<_>>()
    });
    view! {
        <form aria-label="Add chart" class="dna-notebook-tools" on:submit=move |event| {
            event.prevent_default();
            if let Some(field) = address.get_untracked() {
                commands::invoke(session, "notebook.add-chart", field.value());
            }
        }>
            <label>"Chart worksheet" <select data-command="notebook.chart-sheet" on:change=move |event| {
                commands::invoke(session, "notebook.chart-sheet", event_target_value(&event));
            }>
                {move || {
                    let active = worksheet.get();
                    book.get().into_iter().flat_map(|book| book.sheets).map(|(sheet, name)| {
                        let value = name.clone();
                        let selected = active == Some(sheet);
                        view! { <option value=value selected=selected>{name}</option> }
                    }).collect_view()
                }}
            </select></label>
            <label>"Workbook chart" <input required node_ref=address
                list="notebook-charts" placeholder="Analysis!\"SalesByRegion\"" /></label>
            <datalist id="notebook-charts">
                {move || charts.get().into_iter().map(|(address, label)| {
                    view! { <option value=address label=label /> }
                }).collect_view()}
            </datalist>
            <button type="submit" data-command="notebook.add-chart">"Add chart"</button>
        </form>
    }
}

pub(super) fn enter(state: State, id: AnchorId, text: String) -> Result<(), String> {
    let snapshot = state.snapshot()?;
    let block = snapshot
        .chain
        .cells
        .iter()
        .find(|item| item.id == id)
        .ok_or("This Notebook input was removed.")?;
    let Kind::Control {
        address, control, ..
    } = &block.kind
    else {
        return Err("Choose a Notebook input.".into());
    };
    if !control.accepts(&text) {
        return Err("Choose a value within the input's settings.".into());
    }
    let mut batch = Batch {
        id: String::new(),
        workbook: Some(snapshot.part.stamp.workbook),
        base: None,
        expects: Vec::new(),
        selection_seq: None,
        label: Some("Change Notebook input".into()),
        changes: vec![Change::Model(ModelChange::EnterBound {
            bound: Bound {
                anchor: id,
                address: address.clone(),
            },
            text,
        })],
    };
    names::pin_workbook_revision(state.session, &snapshot, &mut batch)?;
    state.session.submit(batch);
    Ok(())
}

pub(super) fn input(
    session: Session,
    id: AnchorId,
    label: String,
    address: String,
    control: InputControl,
) -> impl IntoView {
    dna_kit::page::bound_input(
        session,
        Bound {
            anchor: id,
            address,
        },
        label,
        move |value, writable| {
            let change = move |event: leptos::ev::Event| {
                commands::invoke(
                    session,
                    "notebook.set-control",
                    (id, event_target_value(&event)),
                );
            };
            match &control {
            InputControl::Choice { options } => view! {
                <select disabled=move || !writable.get() prop:value=move || value.get() on:change=change>
                    {options.iter().cloned().map(|entry| {
                        let option = entry.clone();
                        view! { <option value=option>{entry}</option> }
                    }).collect_view()}
                </select>
            }.into_any(),
            InputControl::Integer { min, max } => view! {
                <input type="range" min=*min max=*max step="1"
                    disabled=move || !writable.get() prop:value=move || value.get() on:change=change />
                <output>{move || value.get()}</output>
            }.into_any(),
        }
        },
    )
}

pub(super) fn chart(
    state: State,
    id: AnchorId,
    address: String,
    requires: Option<AnchorId>,
    cells: Memo<Option<BTreeMap<Key, calc_skin::WorkbookCell>>>,
) -> impl IntoView {
    let session = state.session;
    let bound = Bound {
        anchor: id,
        address,
    };
    move || {
        if let Some(requires) = requires {
            let Some(at) = state
                .snapshot()
                .ok()
                .and_then(|snapshot| snapshot.source(requires).and_then(first_cell))
            else {
                return view! { <p role="status">"Chart data unavailable"</p> }.into_any();
            };
            let Some(value) = cells.with(|cells| cells.as_ref().map(|cells| cells.get(&at).cloned())) else {
                return view! { <p role="status">"Reading chart data…"</p> }.into_any();
            };
            if !value.is_some_and(|cell| {
                    matches!(
                        cell.shown.provenance,
                        calc_skin::Provenance::Calculated { .. } | calc_skin::Provenance::Literal
                    ) && !matches!(cell.shown.value, calc_skin::RawValue::Error(_))
                })
            {
                return view! { <p role="status">"Chart data unavailable"</p> }.into_any();
            }
        }
        dna_kit::page::bound_chart(session, bound.clone()).into_any()
    }
}
