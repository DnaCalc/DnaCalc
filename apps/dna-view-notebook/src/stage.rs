use super::*;
use calc_skin::{Interest, Provenance, RowWindow, SpillExtent, TablePart, WorkbookCell};
use commands::invoke;
use dna_formula_ui::formula_field::{FormulaAssist, TextBinding};

pub(super) fn mount(state: State) -> impl IntoView {
    let declaration = Memo::new(move |_| state.snapshot().map(|snapshot| snapshot.doc));
    let chain = Memo::new(move |_| state.snapshot().ok().map(|snapshot| snapshot.chain));
    let items = move || {
        chain
            .get()
            .into_iter()
            .flat_map(|chain| chain.cells.into_iter().map(|item| item.id))
            .collect::<Vec<_>>()
    };
    view! {
        <section class="dna-notebook" data-dnacalc="notebook" aria-label="Notebook" tabindex="0">
            {move || match declaration.get() {
                Err(message) => view! { <p role="status">{message}</p> }.into_any(),
                Ok(None) => first_visit(state).into_any(),
                Ok(Some(_)) => view! {
                    <nav class="dna-notebook-tools" aria-label="Notebook commands">
                        {button(state.session, "notebook.link", "Link selection")}
                        {button(state.session, "notebook.prose-below", "Add Markdown")}
                        {button(state.session, "notebook.layout", "Vertical / Pipeline")}
                        <Show when=move || { state.session.held_edits() > 0 }>
                            {button(state.session, "notebook.run-all", "Run all held drafts")}
                        </Show>
                    </nav>
                    <div role="list" class="dna-notebook-chain"
                        class:dna-notebook-pipeline=move || chain.get().is_some_and(|chain| chain.layout == Layout::Pipeline)>
                        <For each=items
                            key=|id| *id
                            children=move |id| item_card(state, id) />
                    </div>
                    <Show when=move || chain.get().is_some_and(|chain| chain.cells.is_empty())>
                        <p>"Select a worksheet cell in Classic, then link it here."</p>
                    </Show>
                }.into_any(),
            }}
        </section>
    }
}

fn first_visit(state: State) -> impl IntoView {
    view! {
        <div class="dna-notebook-start">
            <h1>"Notebook"</h1>
            <p>"Put workbook formulas, results and Markdown in the order you want to read them."</p>
            <div class="dna-notebook-tools">
                {button(state.session, "notebook.link", "Start with selected cells")}
                {button(state.session, "notebook.draft", "Draft from workbook structure")}
                {button(state.session, "notebook.blank", "Blank notebook")}
            </div>
            {move || {
                let requested = state.draft.get().is_some_and(|book| state.session.stamp().is_some_and(|stamp| stamp.workbook == book));
                if !requested { return ().into_any(); }
                let Some(items) = state.structure() else {
                    return view! { <p role="status">"Reading workbook structure…"</p> }.into_any();
                };
                view! {
                    <h2>"Drafted from workbook structure"</h2>
                    <p>"Keep to save the Notebook layout."</p>
                    <ul>{items.into_iter().map(|(kind, source)| {
                        let name = match kind {
                            Kind::Table { name: Some(name) } => name,
                            _ => address(state.session, first_cell(&source)),
                        };
                        view! { <li>{name}</li> }
                    }).collect_view()}</ul>
                    {button(state.session, "notebook.keep", "Keep")}
                }.into_any()
            }}
        </div>
    }
}

fn item_card(state: State, id: AnchorId) -> impl IntoView {
    let item = Memo::new(move |_| {
        state
            .snapshot()
            .ok()
            .and_then(|snapshot| snapshot.chain.cells.into_iter().find(|item| item.id == id))
    });
    let source = Memo::new(move |_| {
        state
            .snapshot()
            .ok()
            .and_then(|snapshot| snapshot.source(id).cloned())
    });
    let selected = move || {
        state.snapshot().ok().is_some_and(|snapshot| {
            state
                .selected(&snapshot)
                .is_some_and(|index| snapshot.chain.cells[index].id == id)
        })
    };
    let serial = id.to_string();
    view! {
        <article class="dna-notebook-cell" class:dna-notebook-selected=selected
            role="listitem" aria-current=move || selected().to_string() data-notebook-cell=serial>
            <header>
                <button type="button" data-command="notebook.focus"
                    on:click=move |_| invoke(state.session, "notebook.focus", id)>
                    {move || match item.get().map(|item| item.kind) {
                        Some(Kind::Prose { .. }) => "Markdown", Some(Kind::Table { .. }) => "Table", _ => "Formula",
                    }}
                </button>
                <Show when=move || source.get().is_some()>
                    <button type="button" data-command="notebook.reveal"
                        on:click=move |_| invoke(state.session, "notebook.reveal", id)>
                        {move || address(state.session, source.get().as_ref().and_then(first_cell))}
                    </button>
                </Show>
            </header>
            {move || match item.get() {
                Some(Item { kind: Kind::Prose { markdown }, .. }) => prose(state, id, markdown).into_any(),
                Some(Item { kind: Kind::Formula, folded, line_numbers, .. }) => match source.get() {
                    Some(source) => formula(state, id, source, folded, line_numbers).into_any(),
                    None => view! { <p role="status">"The worksheet source is unavailable."</p> }.into_any(),
                },
                Some(Item { kind: Kind::Table { name }, folded, .. }) => {
                    if folded { return view! { <p>"Output folded"</p> }.into_any(); }
                    let table = source.get().map(|source| name.map_or(source, |name| SourceRef::Table { name, part: TablePart::All }));
                    table.map(|source| rows(state, id, source)).into_any()
                }
                None => ().into_any(),
            }}
        </article>
    }
}

fn formula(
    state: State,
    id: AnchorId,
    source: SourceRef,
    folded: bool,
    line_numbers: bool,
) -> impl IntoView {
    let session = state.session;
    let at = first_cell(&source);
    let cells = session.following::<query::Cells>();
    let cell = Memo::new(move |_| {
        let at = at?;
        match cells.at(Some(query::Cells {
            interest: Interest::Area(Area(at, at)),
        })) {
            Live::Ready(answer) => answer.body.cells.into_iter().find(|cell| cell.at == at),
            _ => None,
        }
    });
    let input = move || {
        let snapshot = state.snapshot().ok()?;
        let selected = state.selected(&snapshot)?;
        session
            .edit()
            .filter(|edit| Some(edit.at) == at && snapshot.chain.cells[selected].id == id)
    };
    let editing = Memo::new(move |_| input().is_some());
    let held = move || at.and_then(|at| session.held_edit(at));
    view! {
        <div class="dna-notebook-input">
            <Show when=move || editing.get() fallback=move || view! {
                <pre data-notebook-formula><code>{move || {
                    let entry = held().map(|edit| edit.text).or_else(|| cell.get().map(|cell| cell.input)).unwrap_or_default();
                    if line_numbers {
                        entry.lines().enumerate().map(|(i, line)| format!("{}  {line}", i + 1)).collect::<Vec<_>>().join("\n")
                    } else {
                        entry
                    }
                }}</code></pre>
            }>
                {move || formula_editor(session)}
            </Show>
            <Show when=move || held().is_some()>
                <p class="dna-notebook-draft">"Edited · not run"
                    <button type="button" data-command="notebook.revert"
                        on:click=move |_| invoke(session, "notebook.revert", id)>
                        "Revert cell"
                    </button>
                </p>
            </Show>
        </div>
        <div class="dna-notebook-output" aria-label="Output">
            {move || {
                if folded { return view! { <p>"Output folded"</p> }.into_any(); }
                let Some(cell) = cell.get() else {
                    return view! { <p role="status">"Loading worksheet value…"</p> }.into_any();
                };
                let prior = held().is_some();
                let at = cell.at;
                let output = match cell.spill {
                    Some(SpillExtent::Filled(area)) if area.0 != area.1 =>
                        rows(state, id, SourceRef::Spill { anchor: at }).into_any(),
                    _ => value(cell).into_any(),
                };
                view! { {prior.then(|| view! { <p>"Previous result"</p> })}{output} }.into_any()
            }}
        </div>
        <Show when=move || cell.get().is_some_and(|cell| cell.input.starts_with('='))>
            <button type="button" data-command="notebook.tree"
                on:click=move |_| invoke(session, "notebook.tree", id)>
                "Expression structure"
            </button>
        </Show>
        <Show when=move || {
            state.tree.get().is_some_and(|(workbook, item)| item == id &&
                session.stamp().is_some_and(|stamp| stamp.workbook == workbook))
        }>
            {at.map(|at| dna_kit::live_tree::live_tree(session, at))}
        </Show>
    }
}

fn formula_editor(session: Session) -> impl IntoView {
    let text = TextBinding::new(Signal::derive(move || session.entry()), move |text| {
        session.edit_text(text)
    });
    let assist = FormulaAssist::new(text);
    let diagnostics = Signal::derive(Vec::new);
    view! {
        <div class="dna-notebook-formula-editor" data-dna-editor="true">
            {assist.anchored_field(diagnostics, None, 2, "Enter an Excel formula")}
        </div>
    }
}

fn value(cell: WorkbookCell) -> impl IntoView {
    let provenance = match &cell.shown.provenance {
        Provenance::Calculated { .. } | Provenance::Literal | Provenance::Empty => None,
        Provenance::AsSaved => Some("Saved value"),
        Provenance::Stale { .. } => Some("Previous result"),
        Provenance::External { .. } => Some("External value"),
        Provenance::Refused { .. } => Some("Unavailable"),
    };
    let is_error = matches!(cell.shown.value, calc_skin::RawValue::Error(_));
    view! {
        <div class="dna-notebook-value" class:dna-notebook-error=is_error data-notebook-output>
            {cell.text}
            {provenance.map(|label| view! { <small>{label}</small> })}
        </div>
    }
}

fn rows(state: State, id: AnchorId, source: SourceRef) -> impl IntoView {
    let watch = state.session.following::<query::Rows>();
    move || {
        let options = state.row_controls(id);
        let query = query::Rows {
            source: source.clone(),
            sort: options.sort.into_iter().collect(),
            filter: Vec::new(),
            window: RowWindow {
                offset: options.offset,
                limit: options.limit,
            },
        };
        let page = match watch.at(Some(query)) {
            Live::Ready(answer) => answer.body,
            Live::Refused(reason) => {
                return view! { <p role="status">{format!("Rows unavailable: {reason:?}")}</p> }
                    .into_any();
            }
            _ => return view! { <p role="status">"Loading rows…"</p> }.into_any(),
        };
        let end = page.offset.saturating_add(page.rows.len() as u32);
        let status = if page.rows.is_empty() {
            format!("{} rows", page.matched_rows)
        } else {
            format!(
                "Showing {}–{end} of {} rows",
                page.offset + 1,
                page.matched_rows
            )
        };
        let headers = page.headers.unwrap_or_else(|| {
            (0..page.columns)
                .map(|column| calc_skin::address::column_letters(page.bounds.0.3 + column))
                .collect()
        });
        let session = state.session;
        view! {
            <div class="dna-notebook-frame" data-notebook-rows=page.matched_rows>
                <div class="dna-notebook-tools">
                    <span>{status}</span>
                    <span>"View only"</span>
                    <button type="button" data-command="notebook.rows-page" disabled=page.offset == 0
                        on:click=move |_| invoke(session, "notebook.rows-page", (id, page.offset.saturating_sub(options.limit)))>
                        "Previous"
                    </button>
                    <button type="button" data-command="notebook.rows-page" disabled=end >= page.matched_rows
                        on:click=move |_| invoke(session, "notebook.rows-page", (id, end))>
                        "Next"
                    </button>
                    <button type="button" data-command="notebook.rows-limit"
                        on:click=move |_| invoke(session, "notebook.rows-limit", id)>
                        {if options.limit == 20 { "200 per page" } else { "20 per page" }}
                    </button>
                </div>
                <div class="dna-notebook-table-scroll">
                    <table aria-label="Workbook results" aria-rowcount=page.matched_rows + 1>
                        <thead><tr>
                            <th scope="col">"Row"</th>
                            {headers.into_iter().enumerate().map(|(col, label)| view! {
                                <th scope="col"><button type="button" data-command="notebook.rows-sort"
                                    on:click=move |_| invoke(session, "notebook.rows-sort", (id, col as u32))>
                                    {label}
                                </button></th>
                            }).collect_view()}
                        </tr></thead>
                        <tbody>{page.rows.into_iter().enumerate().map(|(index, row)| view! {
                            <tr aria-rowindex=page.offset as usize + index + 2>
                                <th scope="row">{row.row}</th>
                                {row.cells.into_iter().map(|cell| view! { <td>{value(cell)}</td> }).collect_view()}
                            </tr>
                        }).collect_view()}</tbody>
                        {page.totals.map(|totals| view! {
                            <tfoot><tr><th scope="row">"Totals"</th>
                                {totals.into_iter().map(|cell| view! { <td>{value(cell)}</td> }).collect_view()}
                            </tr></tfoot>
                        })}
                    </table>
                </div>
            </div>
        }.into_any()
    }
}

fn prose(state: State, id: AnchorId, markdown: String) -> impl IntoView {
    let editor = RwSignal::new(markdown.clone());
    let editing = move || {
        state.prose_editor.get().is_some_and(|(book, item)| {
            item == id
                && state
                    .session
                    .stamp()
                    .is_some_and(|stamp| stamp.workbook == book)
        })
    };
    let rendered = markdown
        .lines()
        .map(|line| {
            let parts = |text: &str| chips(state.session, text);
            if let Some(title) = line.strip_prefix("#### ") {
                view! { <h4>{parts(title)}</h4> }.into_any()
            } else if let Some(title) = line.strip_prefix("### ") {
                view! { <h3>{parts(title)}</h3> }.into_any()
            } else if let Some(title) = line.strip_prefix("## ") {
                view! { <h2>{parts(title)}</h2> }.into_any()
            } else if let Some(title) = line.strip_prefix("# ") {
                view! { <h1>{parts(title)}</h1> }.into_any()
            } else if line.is_empty() {
                ().into_any()
            } else {
                view! { <p>{parts(line)}</p> }.into_any()
            }
        })
        .collect_view();
    view! {
        <section class="dna-notebook-prose">
            <div hidden=editing>{rendered}</div>
            <form hidden=move || !editing() on:submit=move |event| {
                event.prevent_default();
                invoke(state.session, "notebook.save-prose", (id, editor.get_untracked()));
            }>
                <textarea aria-label="Markdown" prop:value=move || editor.get()
                    on:input=move |event| editor.set(event_target_value(&event))/>
                <button type="submit" data-command="notebook.save-prose">"Save Markdown"</button>
                {button(state.session, "notebook.cancel-prose", "Cancel")}
            </form>
        </section>
    }
}

fn chips(session: Session, text: &str) -> Vec<AnyView> {
    let mut remaining = text;
    let mut parts = Vec::new();
    while let Some((prefix, candidate)) = remaining.split_once("{=") {
        let Some((reference, rest)) = candidate.split_once('}') else {
            break;
        };
        parts.push(prefix.to_owned().into_any());
        parts.push(chip(session, reference.trim().to_owned()).into_any());
        remaining = rest;
    }
    parts.push(remaining.to_owned().into_any());
    parts
}

fn chip(session: Session, address: String) -> impl IntoView {
    let resolving = session.following::<query::Resolve>();
    let cells = session.following::<query::Cells>();
    let address_text = address.clone();
    let destination = address.clone();
    view! {
        <button type="button" class="dna-notebook-chip" title=address_text
            data-command="notebook.jump" on:click=move |_| invoke(session, "notebook.jump", destination.clone())>
            {move || {
                let Live::Ready(source) = resolving.at(Some(query::Resolve { address: address.clone() })) else { return address.clone(); };
                let Some(at) = first_cell(&source.body) else { return address.clone(); };
                match cells.at(Some(query::Cells { interest: Interest::Area(Area(at, at)) })) {
                    Live::Ready(answer) => answer.body.cells.into_iter().find(|cell| cell.at == at)
                        .map(|cell| cell.text).unwrap_or_else(|| address.clone()),
                    _ => address.clone(),
                }
            }}
        </button>
    }
}

fn button(session: Session, id: &'static str, label: &'static str) -> impl IntoView {
    view! {
        <button type="button" data-command=id on:click=move |_| { let _ = session.command(id); }>
            {label}
        </button>
    }
}

fn address(session: Session, at: Option<Key>) -> String {
    let Some(at) = at else {
        return String::new();
    };
    let cell = format!("{}{}", calc_skin::address::column_letters(at.3), at.2);
    session
        .summary()
        .and_then(|workbook| {
            workbook
                .sheets
                .into_iter()
                .find(|sheet| sheet.sheet == Sheet(at.0, at.1))
        })
        .map_or(cell.clone(), |sheet| format!("{}!{cell}", sheet.name))
}

pub(super) fn reveal(id: AnchorId) {
    #[cfg(target_arch = "wasm32")]
    if let Ok(Some(element)) =
        leptos::prelude::document().query_selector(&format!("[data-notebook-cell='{id}']"))
    {
        element.scroll_into_view();
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = id;
}
