//! The level-one frame: chrome over the one session.
//!
//! The shell holds no workbook state. The session has the selection, the draft, the seam, the
//! watches, the commands and the keys; a view has its own body. The shell draws the Mast, Deck,
//! Bridge, Stage, Sheet Tabs and Strip from the session, registers the commands that belong to
//! no view, and hands every key press to the session's keymap.

use calc_skin::grid::{Key, Sheet};
use calc_skin::query::{self, Census};
use calc_skin::{ModelProperty, ModelPropertyDiff, Operation, SourceRef, ViewId, address};
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidePane {
    Details,
    Help,
}

/// What the chrome itself keeps: which of its own parts are open.
#[derive(Clone, Copy)]
pub struct Chrome {
    views: StoredValue<Vec<Rc<dyn View>>, LocalStorage>,
    search: RwSignal<bool>,
    side_pane: RwSignal<Option<SidePane>>,
    deck_folded: RwSignal<bool>,
    tips: NodeRef<leptos::html::Div>,
}

fn a1(at: Key) -> String {
    format!("{}{}", address::column_letters(at.3), at.2)
}

fn model_property_text(diff: ModelPropertyDiff) -> String {
    match diff {
        ModelPropertyDiff::AddedScenario { scenario, values } => {
            let values = values.into_iter().map(|(at, value)| {
                let value = if value.is_empty() { "Blank".into() } else { value };
                format!("{} = {value}", a1(at))
            }).collect::<Vec<_>>().join(", ");
            format!("Save scenario {} · {values}", scenario.name)
        }
        ModelPropertyDiff::WorksheetInserted { name, .. } => format!("Add worksheet {name}"),
        ModelPropertyDiff::CellFormat { at, before, after } => {
            let style =
                |id: Option<u32>| id.map_or_else(|| "File format".into(), |id| format!("Format {id}"));
            format!("{} · {} → {}", a1(at), style(before), style(after))
        }
        ModelPropertyDiff::Scope { property, .. } => {
            let label = match property {
                ModelProperty::StyleTable => "Workbook formats",
                ModelProperty::ArrayFormulas => "Array formulas",
                ModelProperty::MergedCells => "Merged cells",
                ModelProperty::ConditionalFormatting => "Conditional formatting",
                ModelProperty::DataValidation => "Data validation",
                ModelProperty::Notes => "Notes",
                ModelProperty::Hyperlinks => "Hyperlinks",
                ModelProperty::RichText => "Rich text",
                ModelProperty::ColumnVisibility => "Column visibility",
                ModelProperty::AutoFilter => "AutoFilter",
                ModelProperty::Charts => "Charts",
                ModelProperty::Shapes => "Shapes",
            };
            format!("{label} changed")
        }
    }
}

fn spec(id: &'static str, label: &'static str, when: &str) -> CommandSpec {
    CommandSpec {
        id: CommandId::new(id),
        label: label.into(),
        enabled: When::parse(when).expect("a built-in condition parses"),
    }
}

/// Registers the shell's defaults and then the view commands. Called before [`workbook`] draws.
pub fn install(registry: &Registry, session: Session) -> Chrome {
    let views: Vec<Rc<dyn View>> = registry
        .views()
        .iter()
        .filter_map(|spec| registry.find(&spec.id))
        .collect();
    let chrome = Chrome {
        views: StoredValue::new_local(views.clone()),
        search: RwSignal::new(false),
        side_pane: RwSignal::new(None),
        deck_folded: RwSignal::new(false),
        tips: NodeRef::new(),
    };
    register(session, chrome, &views);
    registry.install_commands(session);
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
        if session.step_back_key_tips() {
            return Ok(());
        }
        if chrome.search.get_untracked() {
            chrome.search.set(false);
        } else if session.mode().editing() || !matches!(session.seam(), Seam::Idle) {
            session.cancel();
        } else if chrome.side_pane.get_untracked() == Some(SidePane::Help) {
            chrome.side_pane.set(None);
        } else if !chrome.views.with_value(|views| views.iter().any(|view| view.cancel_interaction(session))) {
            session.cancel();
        }
        Ok(())
    });
    session.register(
        spec("shell.search", "Search commands and cells", ""),
        move |_| {
            session.dismiss_key_tips();
            chrome.search.update(|open| *open = !*open);
            Ok(())
        },
    );
    session.register(spec("shell.details", "Details", ""), move |_| {
        chrome.side_pane.update(|pane| {
            *pane = (*pane != Some(SidePane::Details)).then_some(SidePane::Details);
        });
        Ok(())
    });
    session.register(spec("shell.help", "Keyboard shortcuts", ""), move |_| {
        session.dismiss_key_tips();
        chrome.side_pane.set(Some(SidePane::Help));
        Ok(())
    });
    session.register(spec("shell.close-help", "Close Help", ""), move |_| {
        if chrome.side_pane.get_untracked() == Some(SidePane::Help) {
            chrome.side_pane.set(None);
        }
        Ok(())
    });
    session.register(spec("shell.toggle-deck", "Toggle Command Deck", "!editing"), move |_| {
        chrome.deck_folded.update(|folded| *folded = !*folded);
        Ok(())
    });
    let ready = When::parse("!editing").expect("a built-in condition parses");
    let editing = When::parse("editing").expect("a built-in condition parses");
    session.bind(Layer::Shell, ready.clone(), "Ctrl+F1", CommandId::new("shell.toggle-deck"));
    for (when, chord, command) in [
        (ready.clone(), "Ctrl+Z", "edit.undo"),
        (ready, "Ctrl+Y", "edit.redo"),
        (When::ALWAYS, "Escape", "edit.cancel"),
        (When::ALWAYS, "Alt+Q", "shell.search"),
    ] {
        session.bind(Layer::Safety, when, chord, CommandId::new(command));
    }
    session.bind(Layer::Shell, When::ALWAYS, "F9", CommandId::new("calc.calculate"));
    session.bind(Layer::Shell, When::ALWAYS, "F1", CommandId::new("shell.help"));
    for (id, label, forward, chords) in [
        ("deck.next-tip", "Next key tip", true, ["Tab", "ArrowRight", "ArrowDown"]),
        ("deck.previous-tip", "Previous key tip", false, ["Shift+Tab", "ArrowLeft", "ArrowUp"]),
    ] {
        let when = "mode:key-tips";
        session.register(spec(id, label, when), move |_| {
            let Some(path) = session.key_tip_path() else {
                return Ok(());
            };
            if !move_key_tip_focus(chrome.tips, path, forward) {
                // F10 and Tab can arrive before the newly opened group's buttons are mounted.
                #[cfg(target_arch = "wasm32")]
                {
                    request_animation_frame(move || {
                        if session.key_tip_path() == Some(path) {
                            move_key_tip_focus(chrome.tips, path, forward);
                        }
                    });
                }
            }
            Ok(())
        });
        for chord in chords {
            session.bind(Layer::Component, When::parse(when).unwrap(), chord, CommandId::new(id));
        }
    }
    for view in views {
        let spec = view.spec();
        let ViewId(name) = spec.id;
        if spec.profile != "jupyter" {
            session.bind(
                Layer::Editor,
                editing.clone().and(When::view(&name)),
                "Enter",
                CommandId::new("edit.commit"),
            );
        }
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

fn focus_tip(button: Option<leptos::web_sys::Element>) {
    if let Some(button) = button.and_then(|button| button.dyn_into::<leptos::web_sys::HtmlElement>().ok()) {
        let _ = button.focus();
    }
}

fn move_key_tip_focus(tips: NodeRef<leptos::html::Div>, path: &str, forward: bool) -> bool {
    let Some(tips) = tips.get_untracked() else {
        return false;
    };
    if !tips.is_connected() || tips.get_attribute("data-key-tip-path").as_deref() != Some(path) {
        return false;
    }
    let active = leptos::web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.active_element())
        .filter(|button| button.has_attribute("data-key-tip"));
    let next = if forward {
        active.and_then(|button| button.next_element_sibling()).or_else(|| tips.first_element_child())
    } else {
        active.and_then(|button| button.previous_element_sibling()).or_else(|| tips.last_element_child())
    };
    let mounted = next.is_some();
    focus_tip(next);
    mounted
}

fn tip_path_changed(
    session: Session,
    tips: NodeRef<leptos::html::Div>,
    stage: NodeRef<leptos::html::Main>,
    previous: &'static str,
) {
    let current = session.key_tip_path();
    if current == Some(previous) {
        return;
    }
    // The previously focused button may already have disappeared. Keep the next key in the
    // workbook immediately, even when someone types the rest of a path before the next frame.
    let stage = stage.get_untracked();
    if let Some(stage) = &stage {
        let _ = stage.focus();
    }
    let Some(path) = current else {
        return;
    };
    request_animation_frame(move || {
        if session.key_tip_path() != Some(path)
            || !stage.as_ref().is_some_and(|stage| stage.matches(":focus").unwrap_or(false))
        {
            return;
        }
        if let Some(tips) = tips.get_untracked() {
            let previous_button = previous.strip_prefix(path)
                .map(str::trim)
                .filter(|key| !key.is_empty())
                .and_then(|key| tips.query_selector(&format!("[data-key-tip='{key}']")).ok().flatten());
            focus_tip(previous_button.or_else(|| tips.first_element_child()));
        }
    });
}

fn key_tip_deck(session: Session, chrome: Chrome, stage: NodeRef<leptos::html::Main>) -> impl IntoView {
    move || {
        session.key_tip_path().map(|path| {
            view! {
                <div class="dna-book-key-tips" role="group" aria-label="Excel key tips"
                    data-dnacalc="key-tips" data-key-tip-path=path node_ref=chrome.tips>
                    {session.key_tip_choices().into_iter().map(|tip| {
                        let key = tip.key();
                        view! {
                            <button type="button" data-key-tip=key
                                on:click=move |_| {
                                    session.choose_key_tip(key);
                                    tip_path_changed(session, chrome.tips, stage, path);
                                }>
                                <kbd>{key}</kbd>{tip.label}
                            </button>
                        }
                    }).collect_view()}
                </div>
            }
        })
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
    if element.get_attribute("data-dnacalc").as_deref() == Some("formula-bar")
        || element.closest("[data-dna-editor]").ok().flatten().is_some()
    {
        return Focus::Editor;
    }
    if element.has_attribute("data-dna-action-field") {
        return Focus::ActionField;
    }
    if element.has_attribute("data-key-tip") {
        return Focus::KeyTip;
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

fn shortcut_help(session: Session) -> impl IntoView {
    view! {
        <aside class="dna-book-details" data-dnacalc="shortcut-help" aria-label="Keyboard shortcut help">
            <h2>"Keyboard shortcuts"</h2>
            {command_button(session, "shell.close-help", "Close Help")}
            <dl>
                {move || {
                    session.view();
                    session.mode();
                    session.search("").into_iter().filter_map(|spec| {
                        let keys = session.chords_for(&spec.id).iter().map(ToString::to_string)
                            .collect::<Vec<_>>().join(" / ");
                        (!keys.is_empty()).then(|| view! {
                            <dt>{spec.label.to_string()}</dt>
                            <dd><kbd>{keys}</kbd></dd>
                        })
                    }).collect_view()
                }}
            </dl>
        </aside>
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
    let docks: Vec<_> = registry.docks().collect();
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
                    let inline = stage_node.get_untracked().and_then(|stage| {
                        stage.query_selector("[data-dna-editor] textarea, textarea[data-dna-editor]").ok().flatten()
                    }).and_then(|input| input.dyn_into::<leptos::web_sys::HtmlTextAreaElement>().ok());
                    if let Some(input) = inline {
                        let _ = input.focus();
                        let end = input.value().encode_utf16().count() as u32;
                        let _ = input.set_selection_range(end, end);
                    } else if let Some(input) = formula_input.get_untracked() {
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
            let focus = focus_of(&event);
            let path = (focus == Focus::KeyTip).then(|| session.key_tip_path()).flatten();
            if session.key(&key_input(&event), focus) {
                event.prevent_default();
                if let Some(path) = path {
                    tip_path_changed(session, chrome.tips, stage_node, path);
                }
            }
        } on:keyup=move |event: leptos::ev::KeyboardEvent| {
            let path = (focus_of(&event) == Focus::KeyTip).then(|| session.key_tip_path()).flatten();
            session.key_released(&key_input(&event));
            if let Some(path) = path {
                tip_path_changed(session, chrome.tips, stage_node, path);
            }
        }>
            <header class="dna-book-mast">
                <span class="dna-fe-mark" aria-hidden="true"><i></i><i></i></span>
                <strong>"DNA Calc"</strong>
                <span class="dna-book-title">{move || session.summary().map(|book| book.name).unwrap_or_default()}</span>
                <span>{move || session.summary().map(|book| if book.capabilities.read_only { "Read-only" } else if book.edited { "Edited" } else if book.layout_edited { "Layout edited" } else { "Unchanged" }).unwrap_or("")}</span>
                {switcher}
                <button type="button" data-command="shell.search" on:click=move |_| { let _ = session.command("shell.search"); }>"Go to" <kbd>"Alt+Q"</kbd></button>
                {command_button(session, "shell.help", "Help")}
            </header>
            <nav class="dna-book-deck" aria-label="Workbook commands">
                <div style:display=move || if chrome.deck_folded.get() { "none" } else { "contents" }>
                    {file_control}
                    {command_button(session, "edit.undo", "Undo")}
                    {command_button(session, "edit.redo", "Redo")}
                    {command_button(session, "calc.calculate", "Calculate")}
                    {command_button(session, "shell.details", "Details")}
                    {dock_buttons}
                </div>
                <button type="button" data-command="shell.toggle-deck" aria-keyshortcuts="Control+F1"
                    aria-expanded=move || (!chrome.deck_folded.get()).to_string()
                    disabled=move || session.mode().editing()
                    on:click=move |_| { let _ = session.command("shell.toggle-deck"); }>
                    {move || if chrome.deck_folded.get() { "Expand Deck" } else { "Collapse Deck" }}
                </button>
                {key_tip_deck(session, chrome, stage_node)}
            </nav>
            <div class="dna-book-bridge" class:dna-book-bridge-passive=move || {
                let id = session.view();
                chrome.views.with_value(|views| views.iter().any(|view| view.spec().id == id && !view.cell_editor()))
            }>
                <input class="dna-book-name" aria-label="Cell address" data-dnacalc="name-box"
                    prop:value=move || session.active().map(a1).unwrap_or_default()
                    on:keydown=move |event: leptos::ev::KeyboardEvent| {
                        if event.key() == "Enter" { session.go_to(event_target_value(&event)); }
                    }/>
                <span class="dna-book-fx" aria-hidden="true">"ƒx"</span>
                <input class="dna-book-formula" aria-label="Formula bar" data-dnacalc="formula-bar" node_ref=formula_input
                    prop:value=move || session.entry()
                    prop:readOnly=move || session.summary().is_some_and(|book| book.capabilities.read_only)
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
                            {preview.properties.into_iter().map(|property| view! {
                                <span class="dna-book-review-write">{model_property_text(property)}</span>
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
                    <Show when=move || chrome.side_pane.get() == Some(SidePane::Details)>
                    <aside class="dna-book-details">
                        <h2>"Details"</h2>
                        <dl><dt>"Selected cell"</dt><dd>{move || session.active().map(a1).unwrap_or_default()}</dd>
                            <dt>"Entry"</dt><dd>{move || session.entry()}</dd></dl>
                    </aside>
                    </Show>
                    <Show when=move || chrome.side_pane.get() == Some(SidePane::Help)>
                        {shortcut_help(session)}
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
                <span>{move || match session.held_edits() {
                    0 => String::new(),
                    1 => "1 Notebook cell edited, not run".into(),
                    count => format!("{count} Notebook cells edited, not run"),
                }}</span>
                <span>{move || session.notice()}</span>
            </footer>
        </div>
    }
}
