use super::*;
use calc_skin::{Interest, Provenance, RowWindow, SpillExtent, TablePart, WorkbookCell};
use commands::invoke;
use dna_formula_ui::formula_field::{FormulaAssist, TextBinding};

pub(super) fn mount(state: State) -> impl IntoView {
    let readings = FormulaReadings::new(state);
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
            {super::author::mount(state)}
            {super::names::lambda_form(state)}
            {super::blocks::inserter(state)}
            {move || match declaration.get() {
                Err(message) => view! { <p role="status">{message}</p> }.into_any(),
                Ok(None) => first_visit(state).into_any(),
                Ok(Some(_)) => view! {
                    <nav class="dna-notebook-tools" aria-label="Notebook commands">
                        {button(state.session, "notebook.link", "Link selection")}
                        {button(state.session, "notebook.new-formula", "New formula")}
                        {button(state.session, "notebook.new-function", "New function")}
                        {button(state.session, "notebook.prose-below", "Add Markdown")}
                        {button(state.session, "notebook.inserter", "Insert block")}
                        {button(state.session, "notebook.layout", "Vertical / Pipeline")}
                        <Show when=move || { state.session.held_edits() > 0 }>
                            {button(state.session, "notebook.run-all", "Run all held drafts")}
                        </Show>
                    </nav>
                    <div role="list" class="dna-notebook-chain"
                        class:dna-notebook-pipeline=move || chain.get().is_some_and(|chain| chain.layout == Layout::Pipeline)>
                        <For each=items
                            key=|id| *id
                            children=move |id| item_card(state, readings, id) />
                    </div>
                    <Show when=move || chain.get().is_some_and(|chain| chain.cells.is_empty())>
                        <p>"Add a formula or link a worksheet cell."</p>
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
                {button(state.session, "notebook.new-formula", "New formula")}
                {button(state.session, "notebook.new-function", "New function")}
                {button(state.session, "notebook.inserter", "Insert block")}
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

#[derive(Clone, Copy)]
struct FormulaReadings {
    cells: Memo<Option<BTreeMap<Key, WorkbookCell>>>,
    names: Memo<BTreeMap<Key, query::FormulaNamesOf>>,
}

impl FormulaReadings {
    fn new(state: State) -> Self {
        let keys = Memo::new(move |_| {
            let Ok(snapshot) = state.snapshot() else { return Vec::new() };
            let mut keys = std::collections::BTreeSet::new();
            for item in &snapshot.chain.cells {
                let id = match item.kind {
                    Kind::Formula => item.id,
                    Kind::Chart { requires: Some(id), .. } => id,
                    _ => continue,
                };
                if let Some(at) = snapshot.source(id).and_then(first_cell) {
                    keys.insert(at);
                }
            }
            keys.into_iter().collect::<Vec<_>>()
        });
        let cell_watch = state.session.following::<query::Cells>();
        let name_watch = state.session.following::<query::FormulaNames>();
        let cells = Memo::new(move |_| {
            let request = keys.with(|keys| (!keys.is_empty()).then(|| query::Cells {
                interest: Interest::Areas(keys.iter().map(|at| Area(*at, *at)).collect()),
            }));
            match cell_watch.at(request) {
                Live::Ready(answer) => Some(answer.body.cells.into_iter().map(|cell| (cell.at, cell)).collect()),
                _ => None,
            }
        });
        let names = Memo::new(move |_| {
            let cells = keys.get();
            match name_watch.at((!cells.is_empty()).then_some(query::FormulaNames { cells })) {
                Live::Ready(answer) => answer.body.into_iter().map(|formula| (formula.at, formula)).collect(),
                _ => BTreeMap::new(),
            }
        });
        Self { cells, names }
    }
}

fn item_card(state: State, readings: FormulaReadings, id: AnchorId) -> impl IntoView {
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
            role="listitem" aria-current=move || selected().to_string() data-notebook-cell=serial
            data-notebook-output-name=move || item.get().and_then(|item| item.output_name)>
            <header>
                <button type="button" data-command="notebook.focus"
                    on:click=move |_| invoke(state.session, "notebook.focus", id)>
                    {move || match item.get().map(|item| item.kind) {
                        Some(Kind::Prose { .. }) => "Markdown",
                        Some(Kind::Control { .. }) => "Input",
                        Some(Kind::Chart { .. }) => "Chart",
                        Some(Kind::Table { .. }) => "Table",
                        Some(Kind::Function { .. }) => "Function",
                        _ => "Formula",
                    }}
                </button>
                <Show when=move || source.get().is_some()>
                    <span>{move || {
                        if item.get().is_some_and(|item| item.owns_cell()) { "Owned" } else { "Referenced" }
                    }}</span>
                    <button type="button" data-command="notebook.reveal"
                        on:click=move |_| invoke(state.session, "notebook.reveal", id)>
                        {move || address(state.session, source.get().as_ref().and_then(first_cell))}
                    </button>
                </Show>
            </header>
            {move || {
                item.get().filter(|item| item.owns_cell() && matches!(item.kind, Kind::Formula))
                    .map(|item| super::names::output_name_form(state, id, item.output_name))
            }}
            {move || match item.get() {
                Some(Item { kind: Kind::Prose { markdown }, .. }) => prose(state, id, markdown).into_any(),
                Some(Item { kind: Kind::Function { name, .. }, .. }) =>
                    super::names::live_function(state.session, name).into_any(),
                Some(Item { kind: Kind::Control { label, address, control }, .. }) =>
                    super::blocks::input(state.session, id, label, address, control).into_any(),
                Some(Item { kind: Kind::Chart { address, requires }, .. }) =>
                    super::blocks::chart(state, id, address, requires, readings.cells).into_any(),
                Some(Item { kind: Kind::CodeSlot, .. }) => view! {
                    <button type="button" data-command="notebook.edit" on:click=move |_| {
                        invoke(state.session, "notebook.focus", id);
                        let _ = state.session.command("notebook.edit");
                    }>"Enter formula"</button>
                }.into_any(),
                Some(Item { kind: Kind::Formula, folded, line_numbers, .. }) => match source.get() {
                    Some(source) => formula(state, readings, id, source, folded, line_numbers).into_any(),
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
    readings: FormulaReadings,
    id: AnchorId,
    source: SourceRef,
    folded: bool,
    line_numbers: bool,
) -> impl IntoView {
    let session = state.session;
    let at = first_cell(&source);
    let cell = Memo::new(move |_| {
        let at = at?;
        readings.cells.with(|cells| cells.as_ref()?.get(&at).cloned())
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
                    if let Some(edit) = held() {
                        return formula_text(session, &edit.text, &[], line_numbers);
                    }
                    let entry = cell.get().map(|cell| cell.input).unwrap_or_default();
                    readings.names.with(|names| {
                        let links = at.and_then(|at| names.get(&at)).filter(|formula| formula.text == entry)
                            .map_or(&[][..], |formula| formula.links.as_slice());
                        formula_text(session, &entry, links, line_numbers)
                    })
                }}</code></pre>
            }>
                {move || formula_editor(state)}
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
                let blocked = matches!(cell.spill, Some(SpillExtent::Blocked(_)));
                let output = match cell.spill {
                    Some(SpillExtent::Filled(area)) if area.0 != area.1 =>
                        rows(state, id, SourceRef::Spill { anchor: at }).into_any(),
                    _ => value(cell).into_any(),
                };
                view! {
                    {prior.then(|| view! { <p>"Previous result"</p> })}
                    {output}
                    {blocked.then(|| super::spill::controls(state, at))}
                }.into_any()
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

fn formula_text(
    session: Session, text: &str, links: &[query::FormulaNameLink], line_numbers: bool,
) -> Vec<AnyView> {
    let mut offset = 0;
    let mut content = Vec::new();
    let mut links = links.iter().peekable();
    for (number, line) in text.split_inclusive('\n').enumerate() {
        if line_numbers {
            content.push(format!("{}  ", number + 1).into_any());
        }
        let units = line.encode_utf16().collect::<Vec<_>>();
        let mut cursor = 0;
        while let Some(link) = links.next_if(|link| link.start < offset + units.len()) {
            if link.start < offset + cursor || link.end <= link.start || link.end > offset + units.len() {
                continue;
            }
            let (start, end) = (link.start - offset, link.end - offset);
            content.push(String::from_utf16_lossy(&units[cursor..start]).into_any());
            let label = String::from_utf16_lossy(&units[start..end]);
            let source = link.source.clone();
            content.push(view! {
                <button type="button" class="dna-notebook-chip" data-command="notebook.follow-source"
                    on:click=move |_| invoke(session, "notebook.follow-source", source.clone())>{label}</button>
            }.into_any());
            cursor = end;
        }
        content.push(String::from_utf16_lossy(&units[cursor..]).into_any());
        offset += units.len();
    }
    content
}

fn formula_editor(state: State) -> impl IntoView {
    let session = state.session;
    let text = TextBinding::new(Signal::derive(move || session.entry()), move |text| {
        session.edit_text(text)
    });
    let assist = FormulaAssist::new(text);
    assist.enable_worksheet_pointing(move || session.edit().map_or((1, 1), |edit| (edit.at.2, edit.at.3)));
    evaluate_draft_part(session, assist);
    state.formula_editor.set_value(Some(assist));
    on_cleanup(move || {
        state.formula_editor.try_set_value(None);
    });
    let diagnostics = Signal::derive(Vec::new);
    view! {
        <div class="dna-notebook-formula-editor" data-dna-editor="true">
            {assist.anchored_field(diagnostics, None, 2, "Enter an Excel formula")}
            <Show when=move || !assist.part_status().get().is_empty()>
                <p role="status">{move || assist.part_status().get()}</p>
            </Show>
        </div>
    }
}

type PartReply = Box<dyn FnOnce(Result<String, String>)>;

fn evaluate_draft_part(session: Session, assist: FormulaAssist) {
    let request = RwSignal::new(None::<query::EvaluateDraftPart>);
    let pending = StoredValue::new_local(None::<PartReply>);
    let work = session.following_with_budget::<query::EvaluateDraftPart>(dna_kit::live_tree::EXPRESSION_BUDGET);
    assist.use_part_keymap();
    assist.enable_part_evaluation(move |start, len, reply| {
        let Some(edit) = session.edit() else {
            reply(Err("Open a Notebook formula.".into()));
            return;
        };
        pending.set_value(Some(reply));
        request.set(Some(query::EvaluateDraftPart {
            at: edit.at, formula: edit.text, start, len,
        }));
    });
    Effect::new(move |_| {
        let draft = request.get();
        let result = match work.at(draft.clone()) {
            Live::Ready(answer) => Ok(answer.body.literal),
            Live::Refused(reason) => Err(dna_kit::refusal_text(reason)),
            _ => return,
        };
        let Some(draft) = draft else { return };
        request.set(None);
        let reply = pending.try_update_value(Option::take).flatten();
        if session.edit().is_some_and(|edit| edit.at == draft.at && edit.text == draft.formula)
            && let Some(reply) = reply
        {
            reply(result);
        }
    });
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
    dna_kit::page::live_values(text, |address| chip(session, address).into_any())
}

fn chip(session: Session, address: String) -> impl IntoView {
    let resolving = session.following::<query::Resolve>();
    let names = session.following::<query::DefinedName>();
    let cells = session.following::<query::Cells>();
    let address_text = address.clone();
    let destination = address.clone();
    let source = Memo::new(move |_| {
        let Live::Ready(answer) = resolving.at(Some(query::Resolve { address: address.clone() })) else {
            return None;
        };
        match answer.body {
            SourceRef::Name { scope, name } => match names.at(Some(query::DefinedName { scope, name })) {
                Live::Ready(answer) => answer.body.source,
                _ => None,
            },
            source => Some(source),
        }
    });
    let label = destination.clone();
    view! {
        <button type="button" class="dna-notebook-chip" title=address_text
            data-command="notebook.follow-source" on:click=move |_| {
                if let Some(source) = source.get_untracked() {
                    invoke(session, "notebook.follow-source", source);
                } else {
                    invoke(session, "notebook.jump", destination.clone());
                }
            }>
            {move || {
                let Some(at) = source.get().as_ref().and_then(first_cell) else { return label.clone(); };
                match cells.at(Some(query::Cells { interest: Interest::Area(Area(at, at)) })) {
                    Live::Ready(answer) => answer.body.cells.into_iter().find(|cell| cell.at == at)
                        .map(|cell| match cell.shown.provenance {
                            Provenance::Refused { .. } => "Unavailable".into(),
                            Provenance::Stale { .. } => format!("{} · previous result", cell.text),
                            Provenance::AsSaved => format!("{} · saved value", cell.text),
                            _ => cell.text,
                        }).unwrap_or_else(|| label.clone()),
                    _ => label.clone(),
                }
            }}
        </button>
    }
}

pub(super) fn button(session: Session, id: &'static str, label: &'static str) -> impl IntoView {
    view! {
        <button type="button" data-command=id on:click=move |_| { let _ = session.command(id); }>
            {label}
        </button>
    }
}

pub(super) fn address(session: Session, at: Option<Key>) -> String {
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
