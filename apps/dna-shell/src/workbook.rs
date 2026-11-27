//! The level-one frame: chrome over the one session.
//!
//! The shell holds no workbook state. The session has the selection, the draft, the seam, the
//! watches, the commands and the keys; a view has its own body. The shell draws the Mast, Deck,
//! Bridge, Stage, Sheet Tabs and Strip from the session, registers the commands that belong to
//! no view, and hands every key press to the session's keymap.

use calc_skin::grid::{Key, Sheet};
use calc_skin::query::{self, Census};
use calc_skin::{Operation, SourceRef, ViewId, address};
use dna_kit::commands::{CommandId, CommandSpec, Layer, When};
use dna_kit::{
    DockSide, Fit, Focus, Invocation, KeyInput, Live, Located, Registry, Seam, Session, View,
};
use leptos::{leptos_dom::helpers::request_animation_frame, prelude::*, wasm_bindgen::JsCast};
use std::rc::Rc;

/// The digit of each view's switching chord (decision D12). App has only the Alt+Shift alias:
/// Excel uses Ctrl+Alt+5.
const VIEW_DIGITS: [(&str, &str, bool); 5] = [
    ("classic", "1", true),
    ("inspector", "2", true),
    ("notebook", "3", true),
    ("map", "4", true),
    ("app", "5", false),
];

/// What the chrome itself keeps: which of its own parts are open.
#[derive(Clone, Copy)]
pub struct Chrome {
    views: StoredValue<Vec<Rc<dyn View>>, LocalStorage>,
    search: RwSignal<bool>,
    details: RwSignal<bool>,
}

fn a1(at: Key) -> String {
    format!("{}{}", address::column_letters(at.3), at.2)
}

fn spec(id: &'static str, label: &'static str, when: &str) -> CommandSpec {
    CommandSpec {
        id: CommandId::new(id),
        label: label.into(),
        enabled: When::parse(when).expect("a built-in condition parses"),
    }
}

/// Registers every command and key of the page with the session: each view's own, then the
/// shell's. Called once per session, before [`workbook`] draws.
pub fn install(registry: &Registry, session: Session) -> Chrome {
    let views: Vec<Rc<dyn View>> = registry
        .views()
        .iter()
        .filter_map(|spec| registry.find(&spec.id))
        .collect();
    registry.install_commands(session);
    let chrome = Chrome {
        views: StoredValue::new_local(views.clone()),
        search: RwSignal::new(false),
        details: RwSignal::new(false),
    };
    register(session, chrome, &views);
    chrome
}

fn register(session: Session, chrome: Chrome, views: &[Rc<dyn View>]) {
    let history = session.watch(query::History);
    let step = move |operation: Operation| {
        let entry = match session.read(history) {
            Live::Ready(history) => match operation {
                Operation::Undo => history.body.undo.into_iter().next().map(|e| ("Undid", e)),
                _ => history.body.redo.into_iter().next().map(|e| ("Redid", e)),
            },
            _ => None,
        };
        let notice = entry.map(|(verb, entry)| format!("{verb} · {}", entry.label));
        session.operate(operation, notice);
    };
    // While a draft is open Ctrl+Z and Ctrl+Y are the editor's, as in Excel.
    session.register(spec("edit.undo", "Undo", "!editing"), move |_| {
        step(Operation::Undo);
        Ok(())
    });
    session.register(spec("edit.redo", "Redo", "!editing"), move |_| {
        step(Operation::Redo);
        Ok(())
    });
    session.register(spec("calc.calculate", "Calculate", ""), move |_| {
        session.operate(
            Operation::Calculate {
                scope: SourceRef::Workbook,
            },
            None,
        );
        Ok(())
    });
    session.register(spec("edit.commit", "Enter", "editing"), move |_| {
        session.commit_edit();
        Ok(())
    });
    session.register(spec("edit.apply-review", "Apply the change", ""), move |_| {
        session.apply_review();
        Ok(())
    });
    session.register(spec("edit.cancel", "Cancel", ""), move |_| {
        if chrome.search.get_untracked() {
            chrome.search.set(false);
        } else if session.mode().editing() || !matches!(session.seam(), Seam::Idle)
            || !chrome.views.with_value(|views| views.iter().any(|view| view.cancel_interaction(session))) {
            session.cancel();
        }
        Ok(())
    });
    session.register(
        spec("shell.search", "Search commands and cells", ""),
        move |_| {
            chrome.search.update(|open| *open = !*open);
            Ok(())
        },
    );
    session.register(spec("shell.details", "Details", ""), move |_| {
        chrome.details.update(|open| *open = !*open);
        Ok(())
    });
    let ready = When::parse("!editing").expect("a built-in condition parses");
    let editing = When::parse("editing").expect("a built-in condition parses");
    for (when, chord, command) in [
        (ready.clone(), "Ctrl+Z", "edit.undo"),
        (ready, "Ctrl+Y", "edit.redo"),
        (When::ALWAYS, "Escape", "edit.cancel"),
        (When::ALWAYS, "Alt+Q", "shell.search"),
    ] {
        session.bind(Layer::Safety, when, chord, CommandId::new(command));
    }
    session.bind(Layer::Editor, editing, "Enter", CommandId::new("edit.commit"));
    session.bind(Layer::Shell, When::ALWAYS, "F9", CommandId::new("calc.calculate"));
    for view in views {
        let ViewId(name) = view.spec().id;
        let command = CommandId::from(format!("view.{name}").as_str());
        let id = ViewId(name.clone());
        session.register(
            CommandSpec {
                id: command.clone(),
                label: format!("Show {}", view.spec().title).into(),
                enabled: When::ALWAYS,
            },
            move |_| {
                session.show(id.clone());
                Ok(())
            },
        );
        if let Some((_, digit, ctrl_alt)) = VIEW_DIGITS.iter().find(|(view, ..)| *view == name) {
            if *ctrl_alt {
                session.bind(Layer::Shell, When::ALWAYS, &format!("Ctrl+Alt+{digit}"), command.clone());
            }
            session.bind(Layer::Shell, When::ALWAYS, &format!("Alt+Shift+{digit}"), command);
        }
    }
}

/// What the page's test probe reports.
pub fn probe(session: Session) -> serde_json::Value {
    let edit = session.edit();
    serde_json::json!({
        "stamp": session.stamp(),
        "active": session.active(),
        "view": session.view().0,
        "mode": session.mode().label(),
        "formula": session.entry(),
        "draft": edit.map(|edit| edit.text),
        "seam": match session.seam() {
            Seam::Idle => "idle",
            Seam::Queued => "queued",
            Seam::Previewing => "previewing",
            Seam::Review(_) => "review",
            Seam::Committing => "committing",
        },
        "notice": session.notice(),
    })
}

fn key_input(event: &leptos::ev::KeyboardEvent) -> KeyInput {
    KeyInput {
        key: event.key(),
        ctrl: event.ctrl_key(),
        alt: event.alt_key(),
        shift: event.shift_key(),
        meta: event.meta_key(),
        alt_graph: event.get_modifier_state("AltGraph"),
        composing: event.is_composing(),
    }
}

fn focus_of(event: &leptos::ev::KeyboardEvent) -> Focus {
    let Some(element) = event
        .target()
        .and_then(|target| target.dyn_into::<leptos::web_sys::Element>().ok())
    else {
        return Focus::Stage;
    };
    if element.get_attribute("data-dnacalc").as_deref() == Some("formula-bar") {
        return Focus::Editor;
    }
    match element.tag_name().as_str() {
        "INPUT" | "TEXTAREA" | "SELECT" => Focus::Field,
        _ if element.get_attribute("contenteditable").is_some() => Focus::Field,
        _ => Focus::Stage,
    }
}

/// A Deck button: its label, its state and its key come from the command.
fn command_button(session: Session, id: &'static str, label: &'static str) -> impl IntoView {
    let command = CommandId::new(id);
    let hint = {
        let command = command.clone();
        move || {
            session.mode();
            session
                .chords_for(&command)
                .first()
                .map(ToString::to_string)
                .unwrap_or_default()
        }
    };
    view! {
        <button type="button" data-command=id title=hint
            on:click=move |_| { let _ = session.command(id); }>{label}</button>
    }
}

pub fn workbook(
    registry: &Registry,
    session: Session,
    chrome: Chrome,
    file_control: AnyView,
) -> impl IntoView + use<> {
    let specs = registry.views();
    let views = chrome.views;
    let docks = views.with_value(|views| views.iter().filter_map(|view| view.dock()).collect::<Vec<_>>());
    let dock_buttons = docks.iter().map(|dock| command_button(session, dock.command, dock.label)).collect_view();
    let right_docks = docks.iter().map(|dock| (dock.mount)(session, DockSide::Right)).collect_view();
    let bottom_docks = docks.iter().map(|dock| (dock.mount)(session, DockSide::Bottom)).collect_view();
    let on_stage = move || {
        let id = session.view();
        views.with_value(|views| views.iter().find(|view| view.spec().id == id).cloned())
    };

    let calc = session.watch(query::CalcState);
    let census = session.watch(query::CensusOf {
        scope: SourceRef::Workbook,
    });
    // The view decides from counts whether it can show this workbook. The stage mounts again
    // only when that decision changes, never because a count moved.
    let fit = Memo::new(move |_| {
        let view = on_stage()?;
        match session.read(census) {
            Live::Ready(answer) => Some(view.fit(&answer.body)),
            Live::Refused(_) => Some(view.fit(&Census::default())),
            _ => None,
        }
    });
    let locate = StoredValue::new_local(None::<Box<dyn Fn(&calc_skin::Selection) -> Located>>);
    let mounted = RwSignal::new(0u64);
    let stage = move || {
        let body = match (on_stage(), fit.get()) {
            (None, _) => {
                locate.set_value(None);
                view! { <p class="dna-book-declined">"This view is not installed."</p> }.into_any()
            }
            (Some(_), None) => {
                locate.set_value(None);
                view! { <p class="dna-sheet-loading">"Loading workbook…"</p> }.into_any()
            }
            (Some(_), Some(Fit::Declined { reason, alternatives })) => {
                locate.set_value(None);
                let ways = alternatives.into_iter().map(|way| {
                    let command = way.command;
                    view! { <button type="button" on:click=move |_| { let _ = session.run(&command); }>{way.label}</button> }
                }).collect_view();
                view! { <div class="dna-book-declined" role="alert"><p>{reason}</p>{ways}</div> }.into_any()
            }
            (Some(view), Some(fit)) => {
                let view = view.mount(session);
                locate.set_value(Some(view.locate));
                let simplified = match fit {
                    Fit::Simplified { shown, total, explanation } => Some(view! {
                        <p class="dna-book-simplified" role="note">{format!("Showing {shown} of {total}. {explanation}")}</p>
                    }),
                    _ => None,
                };
                view! { {simplified}{view.body} }.into_any()
            }
        };
        mounted.update(|count| *count += 1);
        body
    };
    let location = move || {
        mounted.track();
        let selection = session.selection()?;
        locate.with_value(|locate| match locate.as_ref()?(&selection) {
            Located::Exact => None,
            Located::Within { label } => Some(format!("In {label}")),
            Located::Absent => Some("The selection is not in this view".to_owned()),
        })
    };

    let search = NodeRef::<leptos::html::Input>::new();
    let search_text = RwSignal::new(String::new());
    let formula_input = NodeRef::<leptos::html::Input>::new();
    let stage_node = NodeRef::<leptos::html::Main>::new();
    // A draft that starts on the stage moves the caret into the editor; when it ends the keys
    // go back to the stage.
    Effect::new(move |was: Option<bool>| {
        let editing = session.mode().editing();
        if was.is_some_and(|was| was != editing) {
            request_animation_frame(move || {
                if editing {
                    if let Some(input) = formula_input.get_untracked() {
                        let _ = input.focus();
                        let end = input.value().encode_utf16().count() as u32;
                        let _ = input.set_selection_range(end, end);
                    }
                } else if let Some(stage) = stage_node.get_untracked() {
                    let _ = stage.focus();
                }
            });
        }
        editing
    });
    Effect::new(move |_| {
        if chrome.search.get() {
            search_text.set(String::new());
            request_animation_frame(move || {
                if let Some(search) = search.get_untracked() {
                    search.set_value("");
                    let _ = search.focus();
                }
            });
        }
    });
    let sheet = move || session.active().map(|at| Sheet(at.0, at.1));
    let switcher = specs.iter().map(|spec| {
        let id = spec.id.clone();
        let command = Invocation::of(CommandId::from(format!("view.{}", id.0).as_str()));
        let data = format!("view.{}", id.0);
        view! {
            <button type="button" class="dna-book-view" data-command=data
                aria-pressed=move || (session.view() == id).to_string()
                on:click=move |_| { let _ = session.run(&command); }>{spec.title}</button>
        }
    }).collect_view();

    view! {
        <div class="dna-workbook" data-dnacalc="frame" on:keydown=move |event: leptos::ev::KeyboardEvent| {
            if session.key(&key_input(&event), focus_of(&event)) {
                event.prevent_default();
            }
        }>
            <header class="dna-book-mast">
                <span class="dna-fe-mark" aria-hidden="true"><i></i><i></i></span>
                <strong>"DNA Calc"</strong>
                <span class="dna-book-title">{move || session.summary().map(|book| book.name).unwrap_or_default()}</span>
                <span>{move || session.summary().map(|book| if book.capabilities.read_only { "Read-only" } else if book.edited { "Edited" } else if book.layout_edited { "Layout edited" } else { "Unchanged" }).unwrap_or("")}</span>
                {switcher}
                <button type="button" data-command="shell.search" on:click=move |_| { let _ = session.command("shell.search"); }>"Go to" <kbd>"Alt+Q"</kbd></button>
            </header>
            <nav class="dna-book-deck" aria-label="Workbook commands">
                {file_control}
                {command_button(session, "edit.undo", "Undo")}
                {command_button(session, "edit.redo", "Redo")}
                {command_button(session, "calc.calculate", "Calculate")}
                {command_button(session, "shell.details", "Details")}
                {dock_buttons}
            </nav>
            <div class="dna-book-bridge">
                <input class="dna-book-name" aria-label="Cell address" data-dnacalc="name-box"
                    prop:value=move || session.active().map(a1).unwrap_or_default()
                    on:keydown=move |event: leptos::ev::KeyboardEvent| {
                        if event.key() == "Enter" { session.go_to(event_target_value(&event)); }
                    }/>
                <span class="dna-book-fx" aria-hidden="true">"ƒx"</span>
                <input class="dna-book-formula" aria-label="Formula bar" data-dnacalc="formula-bar" node_ref=formula_input
                    prop:value=move || session.entry()
                    on:input=move |event| session.edit_text(event_target_value(&event))/>
                {move || session.edit().map(|draft| {
                    view! { <span class="dna-book-seam">{format!("Model change · {}", a1(draft.at))}</span> }
                })}
                {move || match session.seam() {
                    Seam::Review(preview) => Some(view! {
                        <span class="dna-book-seam" role="alert" data-dnacalc="seam-review">
                            <span>{format!("{} {} changed", preview.writes.len(),
                                if preview.writes.len() == 1 { "cell entry" } else { "cell entries" })}</span>
                            {preview.writes.into_iter().map(|write| view! {
                                <span class="dna-book-review-write">
                                    <strong>{a1(write.at)}</strong>
                                    <code>{write.before}</code>" → "<code>{write.after}</code>
                                </span>
                            }).collect_view()}
                            {preview.warnings.join(" · ")}
                            {command_button(session, "edit.apply-review", "Apply")}
                            {command_button(session, "edit.cancel", "Cancel")}
                        </span>
                    }),
                    _ => None,
                }}
            </div>
            <form class="dna-book-search" hidden=move || !chrome.search.get() on:submit=move |event| {
                event.prevent_default();
                let text = search_text.get_untracked();
                let wanted = text.trim();
                let command = session.search(wanted).into_iter()
                    .find(|spec| spec.label.eq_ignore_ascii_case(wanted) || spec.id.as_str().eq_ignore_ascii_case(wanted));
                chrome.search.set(false);
                match command {
                    Some(spec) => { let _ = session.run(&Invocation::of(spec.id)); }
                    None if !wanted.is_empty() => session.go_to(wanted.to_owned()),
                    None => (),
                }
            }>
                <input node_ref=search aria-label="Find a command or cell" placeholder="Command or cell address" type="search"
                    on:input=move |event| search_text.set(event_target_value(&event))/>
                <button>"Go"</button>
                <ul class="dna-book-found" role="listbox" aria-label="Commands">
                    {move || {
                        let text = search_text.get();
                        if !chrome.search.get() || text.trim().is_empty() { return ().into_any(); }
                        session.search(&text).into_iter().take(8).map(|spec| {
                            let id = spec.id.clone();
                            let data = spec.id.to_string();
                            view! {
                                <li role="option"><button type="button" data-command=data on:click=move |_| {
                                    chrome.search.set(false);
                                    let _ = session.run(&Invocation::of(id.clone()));
                                }>{spec.label.to_string()}</button></li>
                            }
                        }).collect_view().into_any()
                    }}
                </ul>
            </form>
            <main class="dna-book-stage" tabindex="-1" node_ref=stage_node>
                <div class="dna-book-work">
                    {stage}
                    <Show when=move || chrome.details.get()>
                    <aside class="dna-book-details">
                        <h2>"Details"</h2>
                        <dl><dt>"Selected cell"</dt><dd>{move || session.active().map(a1).unwrap_or_default()}</dd>
                            <dt>"Entry"</dt><dd>{move || session.entry()}</dd></dl>
                    </aside>
                    </Show>
                </div>
                <div class="dna-book-dock-right">{right_docks}</div>
                <div class="dna-book-dock-bottom">{bottom_docks}</div>
            </main>
            <nav class="dna-book-sheets" aria-label="Worksheets">
                {move || session.summary().into_iter().flat_map(|book| book.sheets)
                    .filter(|sheet| sheet.visible && sheet.worksheet).map(|entry| {
                        let id = entry.sheet;
                        view! {
                            <button type="button" aria-current=move || (sheet() == Some(id)).to_string()
                                on:click=move |_| session.select_cell(Key(id.0, id.1, 1, 1))>{entry.name}</button>
                        }
                    }).collect_view()}
            </nav>
            <footer class="dna-book-strip" role="status" data-dnacalc="strip">
                <span data-dnacalc="mode">{move || session.mode().label()}</span>
                <span>{move || match session.read(calc) { Live::Ready(answer) => answer.body.word(), _ => "" }}</span>
                <span>{location}</span>
                <span>{move || session.notice()}</span>
            </footer>
        </div>
    }
}
