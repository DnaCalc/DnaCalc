//! The browser surface over a session, reached through a [`Link`]: the formula editor (syntax
//! highlighting, bracket pairs, completion of calc-func's function names, signature help and
//! function help), the cell the formula is entered in, a scratch grid of cells, and the result (its
//! type, its value or array grid, a worksheet error and what it means, where it came from), the
//! diagnostics and the reads, recalculated on every edit.
//!
//! The surface keeps only what the person typed ([`Authored`]) and what the session last said to
//! show ([`Shown`]). On each edit it sends the authored state through its link
//! ([`Request::Evaluate`]) and draws the answer; no value is computed here, and the session is
//! never in this module. The link is one of two: in the browser the session runs in the page
//! ([`LocalLink`]); in the desktop exe the page is in WebView2 and the session runs on the native
//! thread that owns add-in calls, behind [`crate::bridge`]. Both are the same message set
//! ([`crate::port`]), so one surface and one smoke suite serve both.
//!
//! The file is a constrained Excel workbook ([`crate::file`]): New, Open and Save, a file dropped
//! on the page, and, on the desktop, the native dialogs. Ctrl+Shift+Enter in the formula field
//! enters the formula as Excel's legacy array formula (`{=...}`); every other entry is the
//! dynamic form, and the surface has no selector for it.
//!
//! The formula editor is [`dna_formula_ui::formula_field`], which DNA Calc's formula bar shares; its
//! assistance is calc-fml's ([`calc_fml::assist`]) and the function help text is calc-func's
//! ([`calc_func::help`]). Every element a person uses carries a `data-onecalc` attribute naming its
//! role.
//!
//! Ported in part from DnaTreeCalc's Leptos formula skin (`src/dnacalc-formula-skin-leptos/src/lib.rs`
//! at DnaTreeCalc c68ecaa: the array result grid and its CSS; the editor's part moved to
//! [`dna_formula_ui::formula_field`]).
//!
//! The evaluation effect writes the answer into a signal when the link replies; the popup is
//! reconciled in the event handlers that change the text or the caret, so a panic can only come
//! from a memo, an effect or a view, where the browser smoke test (`cargo xtask app smoke
//! onecalc`) sees it.

use crate::grid;
use crate::port::{Link, LoadedAddin, LocalLink, LocalPort, Opened, Reply, Request};
use crate::session::{AddinRef, Session, SessionOptions};
use crate::shown::{Authored, Shown, ViewWire};
use calc_fml::syntax::TextSpan;
use calc_fml::FormatProfile;
use calc_func::help;
use dna_formula_ui::editor::browse;
use dna_formula_ui::formula_field::{FormulaAssist, TextBinding};
use dna_formula_ui::theme;
use leptos::mount::mount_to;
use leptos::prelude::*;
use std::any::Any;
use std::rc::Rc;
use web_sys::HtmlElement;

/// The sheet a new surface's session starts with; the scratch grid's cells are on it.
const FIRST_SHEET: &str = "Sheet1";

/// The most functions the function list shows at once.
const MAX_LISTED_FUNCTIONS: usize = 60;

/// The largest file a person can open: a session is a few cells and one formula.
const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;

/// The media type of the file Save downloads.
const XLSX_TYPE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// The surface, mounted; dropping it unmounts the surface.
pub struct Mounted {
    _handle: Box<dyn Any>,
}

/// Mounts a new surface as the last child of `parent`, over the desktop bridge when the page runs
/// in the desktop exe and over a session in the page otherwise.
pub fn mount(parent: HtmlElement) -> Mounted {
    let link: Rc<dyn Link> = match crate::bridge::connect() {
        Some(bridge) => Rc::new(bridge),
        None => Rc::new(LocalLink::new(LocalPort::new(
            Session::new(FIRST_SHEET).expect("Sheet1 is a sheet name"),
        ))),
    };
    mount_over(parent, link)
}

/// Mounts a new surface over `link`.
pub fn mount_over(parent: HtmlElement, link: Rc<dyn Link>) -> Mounted {
    Mounted {
        _handle: Box::new(mount_to(parent, move || surface(link))),
    }
}

/// What a file action waits for while unsaved changes ask whether to replace them.
#[derive(Clone, Debug, PartialEq)]
enum Pending {
    New,
    Open { name: String, bytes: Vec<u8> },
    OpenNative,
    /// An example's authored state.
    Example(Box<Authored>),
}

fn surface(link: Rc<dyn Link>) -> impl IntoView {
    let native = link.native_files();
    let link = StoredValue::new_local(link);
    let call = move |request: Request, done: Box<dyn FnOnce(Reply)>| {
        link.with_value(|link| link.call(request, done));
    };

    let initial = Authored::default();
    let formula = RwSignal::new(initial.formula.clone());
    // Ctrl+Shift+Enter entered the formula as a legacy array formula (the text it was entered
    // with); any edit of the text leaves that form, as editing a formula in Excel does.
    let array_entry = RwSignal::new(None::<String>);
    // A plain formula read from a file: the legacy form without braces (implicit intersection).
    let plain_entry = RwSignal::new(false);
    let assist = FormulaAssist::new(TextBinding::new(formula.into(), move |text| {
        array_entry.set(None);
        plain_entry.set(false);
        formula.set(text);
    }));
    assist.start_edit(formula.get_untracked());
    let formula_cell = RwSignal::new(initial.formula_cell.clone());
    let entries: RwSignal<Vec<String>> = RwSignal::new(initial.grid.clone());
    let locale = RwSignal::new(initial.locale.clone());
    let number_format = RwSignal::new(initial.number_format.clone());
    let date_1904 = RwSignal::new(initial.date_1904);
    let shown = RwSignal::new(Shown::default());

    let braces = move || array_entry.with(Option::is_some);
    let legacy = move || braces() || plain_entry.get();
    let authored = Memo::new(move |_| Authored {
        formula: formula.get(),
        formula_cell: formula_cell.get(),
        legacy: legacy(),
        plain: plain_entry.get() && !braces(),
        grid: entries.get(),
        locale: locale.get(),
        number_format: number_format.get(),
        date_1904: date_1904.get(),
        now: None,
    });

    // F9 on a selected part asks the session for that part's value over the authored state;
    // arrow keys and clicks on the scratch grid insert references at the caret (point mode).
    assist.enable_part_evaluation(move |start, len, done| {
        call(
            Request::EvaluatePart {
                authored: authored.get_untracked(),
                start,
                len,
            },
            Box::new(move |reply| {
                done(match reply {
                    Reply::Part(part) => Ok(part.literal),
                    Reply::Refused { message } => Err(message),
                    other => Err(format!("Unexpected answer: {other:?}")),
                })
            }),
        );
    });
    assist.enable_point_mode(grid::ROWS, grid::COLUMNS, move || {
        formula_cell
            .with_untracked(|text| dna_formula_ui::display::parse_address(text))
            .unwrap_or((1, grid::COLUMNS))
    });

    // The evaluation trace: the steps the session listed for the authored state it was asked
    // about, and the step shown. It is stale once the authored state differs from that one.
    let trace = RwSignal::new(None::<(Authored, crate::trace::Trace)>);
    let trace_at = RwSignal::new(0usize);
    let show_step = move |index: usize| {
        trace.with_untracked(|trace| {
            if let Some((_, trace)) = trace
                && let Some(step) = trace.steps.get(index)
            {
                assist.focus().set(Some(TextSpan::new(step.start, step.len)));
                trace_at.set(index);
            }
        });
    };
    let start_trace = move |_| {
        let state = authored.get_untracked();
        call(
            Request::Trace {
                authored: state.clone(),
            },
            Box::new(move |reply| {
                if let Reply::Trace(found) = reply {
                    let first = found.steps.first().map(|s| TextSpan::new(s.start, s.len));
                    trace.set(Some((state, found)));
                    trace_at.set(0);
                    assist.focus().set(first);
                }
            }),
        );
    };
    let trace_fresh = move || {
        trace.with(|trace| trace.as_ref().is_some_and(|(state, _)| *state == authored.get()))
    };

    // The VBA module: its source, and what the session said of loading it.
    let vba_source = RwSignal::new(calc_vba::EXAMPLE_MODULE.to_owned());
    let vba_status = RwSignal::new("No VBA module loaded.".to_owned());
    let vba_generation = RwSignal::new(0u64);
    let load_vba = move |_| {
        let source = vba_source.get_untracked();
        call(
            Request::LoadVba {
                name: "Module1".to_owned(),
                source,
            },
            Box::new(move |reply| {
                vba_status.set(match reply {
                    Reply::Functions { names } => format!("Loaded: {}", names.join(", ")),
                    Reply::Refused { message } => {
                        format!("Module not loaded: {message}. Previous functions remain active.")
                    }
                    other => format!("Unexpected answer: {other:?}"),
                });
                vba_generation.update(|generation| *generation += 1);
            }),
        );
    };
    let clear_vba = move |_| {
        call(
            Request::UnloadVba,
            Box::new(move |_| {
                vba_status.set("No VBA module loaded.".to_owned());
                vba_generation.update(|generation| *generation += 1);
            }),
        );
    };

    // The registered functions the formula field completes beside the catalogue. An answer names
    // the version of the table it was calculated with; the functions are asked for once per version.
    let known_functions = RwSignal::new(0u64);
    let complete_with = Callback::new(move |docs: crate::port::FunctionDocs| {
        known_functions.set(docs.version);
        assist.scope().update(|scope| {
            if scope.functions != docs.names {
                scope.functions = docs.names;
            }
            if scope.described != docs.docs {
                scope.described = docs.docs;
            }
        });
    });
    let sync_functions = Callback::new(move |version: u64| {
        if version != known_functions.get_untracked() {
            // Claim the version at once so that the next answer does not ask again.
            known_functions.set(version);
            call(
                Request::FunctionDocs,
                Box::new(move |reply| {
                    if let Reply::FunctionDocs(docs) = reply {
                        complete_with.run(docs);
                    }
                }),
            );
        }
    });
    // Each edit sends the authored state and draws the answer. The clock is the page's.
    Effect::new(move |_| {
        let mut state = authored.get();
        vba_generation.track();
        state.now = dna_formula_ui::web::serial_clock();
        call(
            Request::Evaluate(state),
            Box::new(move |reply| {
                if let Reply::Shown(next) = reply {
                    sync_functions.run(next.functions_version);
                    shown.set(*next);
                }
            }),
        );
    });

    // The file: its name, what the last action said, what it lists, and what it was when saved.
    let file_name = RwSignal::new(String::new());
    let file_status = RwSignal::new("Start with a formula or try an example.".to_owned());
    let addins = RwSignal::new(Vec::<AddinRef>::new());
    let loaded = RwSignal::new(Vec::<String>::new());
    let loaded_addins = RwSignal::new(Vec::<LoadedAddin>::new());
    let addin_status = RwSignal::new(String::new());
    let addin_path = RwSignal::new(String::new());
    let addins_open = RwSignal::new(false);
    let baseline = RwSignal::new(Some(initial.clone()));
    let pending = RwSignal::new(None::<Pending>);
    let file_dialog = NodeRef::<leptos::html::Dialog>::new();
    let file_input = NodeRef::<leptos::html::Input>::new();
    let dirty = move || baseline.get().as_ref() != Some(&authored.get());

    // Gives the surface's fields an authored state, as an opened file or an example does.
    let apply = move |state: Authored| {
        batch(move || {
            array_entry.set((state.legacy && !state.plain).then(|| state.formula.clone()));
            plain_entry.set(state.legacy && state.plain);
            formula.set(state.formula.clone());
            assist.start_edit(state.formula.clone());
            formula_cell.set(state.formula_cell);
            entries.set(state.grid);
            locale.set(state.locale);
            number_format.set(state.number_format);
            date_1904.set(state.date_1904);
        });
    };
    let apply_opened = move |opened: Opened| {
        let Opened {
            name,
            authored: state,
            shown: answer,
            addins: listed,
            loaded: running,
            project,
        } = opened;
        batch(move || {
            baseline.set(Some(state.clone()));
            apply(state);
            sync_functions.run(answer.functions_version);
            shown.set(answer);
            let offered = listed.iter().any(|addin| !running.contains(&addin.path));
            addins.set(listed);
            loaded.set(running);
            if native && offered {
                addins_open.set(true);
                addin_status.set(
                    "This session lists add-ins that are not loaded. Review them below and load the ones you trust."
                        .to_owned(),
                );
            }
            // A session file's VBA project (an .xlsm) is compiled when it opens; its modules are
            // edited through the VBA panel, and Save writes the project back.
            vba_status.set(match &project {
                None => "No VBA module loaded.".to_owned(),
                Some(view) if view.functions.is_empty() => {
                    format!("The file's VBA project did not load: {}", view.notes.join("; "))
                }
                Some(view) => {
                    let mut status = format!(
                        "The file's VBA project is loaded: {} function(s): {}.",
                        view.functions.len(),
                        view.functions.join(", ")
                    );
                    for note in &view.notes {
                        status.push(' ');
                        status.push_str(note);
                        status.push('.');
                    }
                    status
                }
            });
            vba_generation.update(|generation| *generation += 1);
            file_status.set(if name.is_empty() {
                "New session.".to_owned()
            } else {
                format!("Opened {name}.")
            });
            file_name.set(name);
        });
    };
    // The add-ins: the ones the host has loaded, what the last action said, and the path the
    // person typed. Loading and unloading changes the functions, so the formula is evaluated again.
    let on_addins = Callback::new(move |(reply, what): (Reply, String)| match reply {
        Reply::Addins(list) => {
            let count = list.len();
            loaded_addins.set(list);
            addin_status.set(if what.is_empty() {
                format!("{count} add-in(s) loaded.")
            } else {
                format!("{what} {count} add-in(s) loaded.")
            });
            vba_generation.update(|generation| *generation += 1);
        }
        Reply::Cancelled => addin_status.set("No add-in chosen; nothing changed.".to_owned()),
        Reply::Refused { message } => addin_status.set(message),
        other => addin_status.set(format!("Unexpected answer: {other:?}")),
    });
    let load_addin_native = move |_| {
        call(
            Request::LoadAddinNative,
            Box::new(move |reply| on_addins.run((reply, "Loaded.".to_owned()))),
        );
    };
    let load_addin_path = move |_| {
        let path = addin_path.get_untracked();
        call(
            Request::LoadAddin {
                path: path.clone(),
                expected_hash: None,
            },
            Box::new(move |reply| on_addins.run((reply, format!("Loaded {path}.")))),
        );
    };
    let load_listed = Callback::new(move |listed: AddinRef| {
        let path = listed.path.clone();
        call(
            Request::LoadAddin {
                path: listed.path,
                expected_hash: Some(listed.hash),
            },
            Box::new(move |reply| on_addins.run((reply, format!("Loaded {path}.")))),
        );
    });
    // Every listed add-in that is not loaded, in the order the session lists them; each answer
    // comes back in that order.
    let load_all_listed = move |_| {
        let running = loaded_addins.get_untracked();
        let offered: Vec<AddinRef> = addins.with_untracked(|listed| {
            listed
                .iter()
                .filter(|addin| !running.iter().any(|r| r.path == addin.path))
                .cloned()
                .collect()
        });
        for listed in offered {
            load_listed.run(listed);
        }
    };
    let unload_addin = Callback::new(move |path: String| {
        call(
            Request::UnloadAddin { path: path.clone() },
            Box::new(move |reply| on_addins.run((reply, format!("Unloaded {path}.")))),
        );
    });
    Effect::new(move |_| {
        call(
            Request::Addins,
            Box::new(move |reply| {
                if let Reply::Addins(list) = reply {
                    loaded_addins.set(list);
                }
            }),
        );
    });
    let on_reply = Callback::new(move |reply: Reply| match reply {
        Reply::Opened(opened) => apply_opened(*opened),
        Reply::NotASession { reason, offer } => file_status.set(format!(
            "This file is outside what a OneCalc session holds: {reason}. {offer}."
        )),
        Reply::Cancelled => {}
        Reply::Refused { message } => file_status.set(message),
        other => file_status.set(format!("Unexpected answer: {other:?}")),
    });
    let run_pending = move |what: Pending| match what {
        Pending::New => call(Request::New, Box::new(move |reply| on_reply.run(reply))),
        Pending::Open { name, bytes } => call(
            Request::Open { name, bytes },
            Box::new(move |reply| on_reply.run(reply)),
        ),
        Pending::OpenNative => call(Request::OpenNative, Box::new(move |reply| on_reply.run(reply))),
        Pending::Example(state) => apply(*state),
    };
    // Unsaved changes are never replaced without asking.
    let request = Callback::new(move |what: Pending| {
        if dirty() {
            pending.set(Some(what));
        } else {
            run_pending(what);
        }
    });
    Effect::new(move |_| {
        let waiting = pending.get().is_some();
        if let Some(dialog) = file_dialog.get() {
            if waiting && !dialog.open() {
                let _ = dialog.show_modal();
            } else if !waiting && dialog.open() {
                dialog.close();
            }
        }
    });
    let keep = move || {
        pending.set(None);
        file_status.set("Current session kept; nothing was replaced.".to_owned());
        if let Some(dialog) = file_dialog.get_untracked() {
            dialog.close();
        }
        if let Some(field) = assist.field.get_untracked() {
            let _ = field.focus();
        }
    };
    let replace = move || {
        if let Some(what) = pending.get_untracked() {
            pending.set(None);
            if let Some(dialog) = file_dialog.get_untracked() {
                dialog.close();
            }
            run_pending(what);
            if let Some(field) = assist.field.get_untracked() {
                let _ = field.focus();
            }
        }
    };
    let open_file = move |_| {
        if native {
            request.run(Pending::OpenNative);
        } else if let Some(input) = file_input.get_untracked() {
            input.click();
        }
    };
    // A file the page has: chosen in the file input or dropped on the page.
    let read_file = move |file: web_sys::File| {
        dna_formula_ui::web::read_file(file, Some(MAX_FILE_BYTES), move |read| match read {
            Ok((name, bytes)) => request.run(Pending::Open { name, bytes }),
            Err(message) => file_status.set(message),
        });
    };
    let save_file = move |_| {
        let saved = authored.get_untracked();
        let request = if native {
            Request::SaveNative
        } else {
            Request::Save
        };
        call(
            request,
            Box::new(move |reply| match reply {
                Reply::File { name, bytes } => {
                    match dna_formula_ui::web::download_bytes(&name, XLSX_TYPE, &bytes) {
                        Ok(()) => {
                            baseline.set(Some(saved));
                            file_status.set(format!(
                                "Download started: {name}. Keep the file before leaving."
                            ));
                        }
                        Err(message) => file_status.set(message),
                    }
                }
                Reply::Saved { path } => {
                    baseline.set(Some(saved));
                    file_status.set(format!("Saved {path}."));
                }
                Reply::Cancelled => {}
                Reply::Refused { message } => file_status.set(message),
                other => file_status.set(format!("Unexpected answer: {other:?}")),
            }),
        );
    };
    let on_key = Callback::new(move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Enter" && (ev.ctrl_key() || ev.meta_key()) && ev.shift_key() {
            ev.prevent_default();
            array_entry.set(Some(formula.get_untracked()));
        }
    });

    let filter = RwSignal::new(String::new());
    let functions = Memo::new(move |_| filter.with(|filter| browse(filter)));
    let function_list = move || {
        functions.with(|found| {
            let total = found.len();
            let items = found
                .iter()
                .take(MAX_LISTED_FUNCTIONS)
                .map(|target| {
                    let name = target.spelling();
                    let signature = target.signature_help().display;
                    // Microsoft's official description, marked as Microsoft's text in the tooltip.
                    let description = match help::help(*target) {
                        Some(h) if h.microsoft.is_some() => {
                            format!("{} (Source: Microsoft)", h.description())
                        }
                        Some(h) => h.description().to_owned(),
                        None => String::new(),
                    };
                    view! {
                        <li>
                            <button
                                type="button"
                                class="oc-fn"
                                data-onecalc="function-entry"
                                data-name=name
                                title=description.clone()
                                on:mousedown=move |ev| ev.prevent_default()
                                on:click=move |_| assist.insert_function(name)
                            >
                                <code>{signature}</code>
                            </button>
                            <small>{description}</small>
                        </li>
                    }
                })
                .collect_view();
            let count = if total > MAX_LISTED_FUNCTIONS {
                format!("{total} functions; the first {MAX_LISTED_FUNCTIONS} listed")
            } else {
                format!("{total} functions")
            };
            view! {
                <p class="oc-prov" data-onecalc="function-count">{count}</p>
                <ul class="oc-fns" data-onecalc="functions">{items}</ul>
            }
        })
    };

    let result_view = move || {
        shown.with(|shown| match &shown.view {
            ViewWire::NoFormula => view! {
                <div class="oc-empty"><p>"Start with one Excel formula and supply the inputs it reads."</p><button data-onecalc="try-example" on:click=move |_| request.run(Pending::Example(Box::new(Authored::example_sum())))>"Try SUM with two inputs"</button></div>
            }
            .into_any(),
            ViewWire::Value { kind, text, exact } => {
                let (kind, text) = (kind.clone(), text.clone());
                let exact = exact.clone().map(|exact| {
                    view! {
                        <details class="oc-exact"><summary>"Exact value"</summary><span class="oc-prov" data-onecalc="result-exact">
                            {format!(" exactly {exact}")}
                        </span></details>
                    }
                });
                view! {
                    <div>
                        <span class="oc-kind" data-onecalc="result-type">{kind}</span>
                        <span class="oc-value" data-onecalc="result-value">{text}</span>
                        {exact}
                    </div>
                }
                .into_any()
            }
            ViewWire::Error { code, meaning } => {
                let (code, meaning) = (code.clone(), meaning.clone());
                view! {
                    <div>
                        <span class="oc-kind" data-onecalc="result-type">"error"</span>
                        <span class="oc-value oc-err" data-onecalc="result-value">{code}</span>
                        " "
                        <span class="oc-err" data-onecalc="result-error">{meaning}</span>
                    </div>
                }
                .into_any()
            }
            ViewWire::Array {
                total_rows,
                total_cols,
                truncated,
                cells,
            } => {
                let caption = format!(
                    "{total_rows} x {total_cols} array{}",
                    if *truncated {
                        format!(
                            ", first {} rows by {} columns shown",
                            cells.len(),
                            cells.first().map_or(0, Vec::len)
                        )
                    } else {
                        String::new()
                    }
                );
                let body = cells
                    .iter()
                    .map(|row| {
                        let cells = row
                            .iter()
                            .map(|cell| {
                                let (kind, text) = (cell.kind.clone(), cell.text.clone());
                                view! { <td data-kind=kind.clone() title=kind>{text}</td> }
                            })
                            .collect_view();
                        view! { <tr>{cells}</tr> }
                    })
                    .collect_view();
                view! {
                    <div>
                        <span class="oc-kind" data-onecalc="result-type">"array"</span>
                        <span data-onecalc="result-shape">{caption}</span>
                        <table class="oc-array" data-onecalc="result-array" role="grid">
                            <tbody>{body}</tbody>
                        </table>
                    </div>
                }
                .into_any()
            }
            ViewWire::NotCalculated => view! {
                <div>
                    <span class="oc-kind" data-onecalc="result-type">"no value"</span>
                    <span class="oc-err">"The formula was not calculated; see below."</span>
                </div>
            }
            .into_any(),
        })
    };

    let grid_header = (1..=grid::COLUMNS)
        .map(|column| view! { <th>{grid::column_letter(column).to_string()}</th> })
        .collect_view();
    let grid_body = (1..=grid::ROWS)
        .map(|row| {
            let cells = (1..=grid::COLUMNS)
                .map(|column| {
                    let index = ((row - 1) * grid::COLUMNS + (column - 1)) as usize;
                    let address = grid::address(row, column);
                    let cell = move || shown.with(|shown| shown.cell(index));
                    let class = move || {
                        let pointed = assist.pointed_cells().is_some_and(|(r1, c1, r2, c2)| {
                            (r1..=r2).contains(&row) && (c1..=c2).contains(&column)
                        });
                        let base = match cell().kind.as_str() {
                            "formula" => "oc-formula",
                            "refused" => "oc-refused",
                            _ => "",
                        };
                        if pointed { format!("{base} oc-pointed") } else { base.to_owned() }
                    };
                    let value = move || {
                        let shown = cell();
                        if shown.kind == "formula" {
                            shown.value
                        } else {
                            entries.with(|entries| entries.get(index).cloned().unwrap_or_default())
                        }
                    };
                    view! {
                        <td class=class>
                            <input
                                type="text"
                                data-onecalc="grid-cell"
                                on:mousedown=move |ev: leptos::ev::MouseEvent| {
                                    if assist.point_click(row, column, ev.shift_key()) {
                                        ev.prevent_default();
                                    }
                                }
                                data-cell=address.clone()
                                aria-label=address
                                data-kind=move || cell().kind
                                title=move || cell().note
                                readonly=move || cell().kind == "formula"
                                prop:value=value
                                on:input=move |ev| {
                                    let text = event_target_value(&ev);
                                    entries.update(|entries| {
                                        if let Some(entry) = entries.get_mut(index) {
                                            *entry = text;
                                        }
                                    });
                                }
                            />
                        </td>
                    }
                })
                .collect_view();
            view! { <tr><th>{row}</th>{cells}</tr> }
        })
        .collect_view();

    let addin_list = move || {
        addins.with(|listed| {
            listed
                .iter()
                .map(|addin| {
                    let running = loaded.with(|loaded| loaded.contains(&addin.path));
                    view! {
                        <li data-onecalc="file-addin">
                            {format!(
                                "{}: {} ({}) - {}",
                                addin.kind,
                                addin.path,
                                addin.hash,
                                if running { "loaded" } else { "listed, not loaded" }
                            )}
                        </li>
                    }
                })
                .collect_view()
        })
    };

    view! {
        <section
            class="oc"
            data-onecalc="surface"
            on:dragover=move |ev: leptos::ev::DragEvent| ev.prevent_default()
            on:drop=move |ev: leptos::ev::DragEvent| {
                ev.prevent_default();
                if let Some(file) = ev.data_transfer().and_then(|t| t.files()).and_then(|f| f.get(0)) {
                    read_file(file);
                }
            }
        >
            <header class="oc-head">
                <h1>"DNA OneCalc"</h1>
                <select
                    class="oc-theme"
                    data-onecalc="theme"
                    aria-label="Theme"
                    on:change=move |ev| theme::set(&event_target_value(&ev))
                    prop:value=theme::current()
                >
                    {theme::CHOICES
                        .iter()
                        .map(|(id, label)| view! { <option value=*id>{*label}</option> })
                        .collect_view()}
                </select>
            </header>
            <p class="oc-sub">
                "One Excel formula, with the inputs it needs."
            </p>
            <details class="oc-fixtures" data-onecalc="addin-controls" prop:open=move || addins_open.get() on:toggle=move |ev| addins_open.set(event_target::<web_sys::HtmlElement>(&ev).has_attribute("open"))><summary data-onecalc="addin-tools">"Add-ins · "<span data-onecalc="addin-count">{move || loaded_addins.with(|list| format!("{} loaded", list.len()))}</span></summary>
                <p>"Excel add-ins (.xll, and VBA add-ins in .xlam files) register worksheet functions the formula can call, with the argument names and help they carry. Loading an .xlam installs it, as Excel's Add-ins dialog does: its Workbook_Open and Workbook_AddinInstall run. A saved session lists the add-ins loaded, with the hash of each file; reopening it offers to load them again (an .xlam is then opened, not installed), and loads nothing by itself."</p>
                <p data-onecalc="addin-browser-note" hidden=native>"Add-ins load in the desktop app; this page runs without a host that can load them."</p>
                <div class="oc-session-actions" hidden=!native>
                    <button data-onecalc="addin-load" on:click=load_addin_native>"Load add-in…"</button>
                    <input data-onecalc="addin-path" type="text" size="40" placeholder="Path of an .xll or .xlam file" aria-label="Path of an add-in" prop:value=move || addin_path.get() on:input=move |ev| addin_path.set(event_target_value(&ev))/>
                    <button data-onecalc="addin-load-path" on:click=load_addin_path>"Load path"</button>
                </div>
                <p role="status" data-onecalc="addin-status">{move || addin_status.get()}</p>
                <ul data-onecalc="addins-loaded">{move || loaded_addins.with(|list| list.iter().map(|addin| {
                    let path = addin.path.clone();
                    let functions = if addin.functions.is_empty() { "no worksheet functions".to_owned() } else { addin.functions.join(", ") };
                    let notes = if addin.notes.is_empty() { String::new() } else { format!(" ({})", addin.notes.join("; ")) };
                    view! {
                        <li data-onecalc="addin-loaded" data-path=addin.path.clone()>
                            <strong>{addin.kind.clone()}</strong>" "{addin.path.clone()}" ("{addin.hash.clone()}") - "{functions}{notes}" "
                            <button data-onecalc="addin-unload" on:click=move |_| unload_addin.run(path.clone())>"Unload"</button>
                        </li>
                    }
                }).collect_view())}</ul>
                <p hidden=move || !native || addins.with(|listed| listed.is_empty())><button data-onecalc="addin-load-all" on:click=load_all_listed>"Load all the add-ins the session lists"</button></p>
                <ul data-onecalc="addins-offered">{move || {
                    let running = loaded_addins.get();
                    addins.with(|listed| listed.iter().filter(|addin| !running.iter().any(|r| r.path == addin.path)).map(|addin| {
                        let offered = addin.clone();
                        view! {
                            <li data-onecalc="addin-offered" data-path=addin.path.clone()>
                                {format!("{}: {} ({}) - listed by the session, not loaded ", addin.kind, addin.path, addin.hash)}
                                <button data-onecalc="addin-load-listed" hidden=!native on:click=move |_| load_listed.run(offered.clone())>"Load"</button>
                            </li>
                        }
                    }).collect_view())
                }}</ul>
            </details>
            <details class="oc-fixtures" data-onecalc="file-controls"><summary data-onecalc="file-tools">"Session file · "<span data-onecalc="file-state">{move ||if dirty(){"Unsaved changes"}else{"Unchanged since opened or saved"}}</span></summary>
                <p>"A session is an Excel workbook (.xlsx) with one sheet, the formula in its cell and the grid's constants; with a VBA project it is an .xlsm, whose project loads when it opens and is saved back with it. Open one, drop one on this page, or save the session as one; Excel opens it as it is."</p>
                <div class="oc-session-actions">
                    <button data-onecalc="file-new" on:click=move |_| request.run(Pending::New)>"New session"</button>
                    <button data-onecalc="file-open" on:click=open_file>"Open…"</button>
                    <button data-onecalc="file-save" on:click=save_file>"Save…"</button>
                </div>
                <input node_ref=file_input data-onecalc="file-input" type="file" accept=".xlsx,.xlsm,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/vnd.ms-excel.sheet.macroEnabled.12" hidden aria-label="Open a session workbook" on:change=move |ev| {
                    let input = event_target::<web_sys::HtmlInputElement>(&ev);
                    if let Some(file) = input.files().and_then(|files| files.get(0)) {
                        read_file(file);
                    }
                    input.set_value("");
                }/>
                <p role="status" data-onecalc="file-status">{move || file_status.get()}</p>
                <p data-onecalc="file-name">{move || file_name.get()}</p>
                <ul data-onecalc="file-addins">{addin_list}</ul>
                <div class="oc-examples"><strong>"Examples"</strong><button data-onecalc="example-sum" on:click=move |_| request.run(Pending::Example(Box::new(Authored::example_sum())))>"SUM & blank input"</button><button data-onecalc="example-array" on:click=move |_| request.run(Pending::Example(Box::new(Authored { formula: "=SEQUENCE(3,2)".to_owned(), ..Authored::default() })))>"Array"</button><button data-onecalc="example-lambda" on:click=move |_| request.run(Pending::Example(Box::new(Authored { formula: "=LAMBDA(x,x*2)(21)".to_owned(), ..Authored::default() })))>"LAMBDA"</button><button data-onecalc="example-missing" on:click=move |_| request.run(Pending::Example(Box::new(Authored { formula: "=Z99+1".to_owned(), ..Authored::default() })))>"Missing input"</button></div>
            </details>
            <dialog class="oc-session-dialog" node_ref=file_dialog data-onecalc="file-dialog" aria-labelledby="oc-replace-title" on:cancel=move |ev:leptos::ev::Event|{ev.prevent_default();keep();}>
                <h2 id="oc-replace-title">"Replace unsaved changes?"</h2><p>"Save this session first to keep its formula and inputs."</p><button data-onecalc="file-keep" autofocus on:click=move |_|keep()>"Keep editing"</button><button data-onecalc="file-replace" on:click=move |_|replace()>"Replace session"</button>
            </dialog>

            <div class="oc-row">
                <div class="oc-col">
                    {assist
                        .field(
                            Signal::derive(move || {
                                shown.with(|shown| {
                                    shown
                                        .diagnostic_spans
                                        .iter()
                                        .map(|&(start, len)| TextSpan::new(start, len))
                                        .collect()
                                })
                            }),
                            Some(on_key),
                            2,
                            "=SUM(A1:A3)",
                        )}
                    {assist.popup()}
                    {assist.signature()}
                    <p class="oc-prov" role="status" data-onecalc="part-status">{move || assist.part_status().get()}</p>
                    <p>
                        <span class="oc-prov" data-onecalc="array-formula" hidden=move || !braces()>
                            {move || format!("{{{}}}", formula.get())}
                        </span>
                        <span class="oc-prov" data-onecalc="plain-formula" hidden=move || !plain_entry.get()>
                            "A plain formula from a file: the legacy form, with implicit intersection. Editing it enters it in the dynamic form."
                        </span>
                        " entered in "
                        <input
                            data-onecalc="formula-cell"
                            type="text"
                            size="10"
                            aria-label="Formula cell" prop:value=move ||formula_cell.get()
                            on:input=move |ev| formula_cell.set(event_target_value(&ev))
                        />
                        " "
                        <span data-onecalc="formula-cell-status">
                            {move || shown.with(|shown| shown.formula_cell.clone())}
                        </span>
                    </p>
                    <section class="oc-result" data-onecalc="result-panel" aria-labelledby="oc-result-title">
                        <header><h2 id="oc-result-title">"Result"</h2><span class="oc-result-state" data-onecalc="result-state">{move ||shown.with(|s|s.state.clone())}</span></header>
                        <div class="oc-primary-result" data-onecalc="primary-result" aria-live="polite">{result_view}</div>
                        <p class="oc-format-note" data-onecalc="format-note">{move ||shown.with(|s|s.format_note.clone())}</p>
                        <ul class="oc-diags" data-onecalc="diagnostics">{move ||shown.with(|s|s.diagnostics.clone()).into_iter().map(|line|view!{<li>{line}</li>}).collect_view()}</ul>
                        <details class="oc-calculation-details" data-onecalc="result-details"><summary>"Calculation details"</summary>
                            <p>"Raw result: "<output data-onecalc="result">{move ||shown.with(|s|s.result.clone())}</output></p>
                            <p class="oc-prov" data-onecalc="provenance">{move ||shown.with(|s|s.provenance.clone())}</p>
                            <p class="oc-prov" data-onecalc="environment">{move ||format!("Dates use the {} system; {} numbers in {} format. NOW and TODAY read the page's clock. Evaluation uses calc-fml and calc-func.",if date_1904.get(){"1904"}else{"1900"},locale.get(),number_format.get())}</p>
                            <ol class="oc-reads" data-onecalc="reads">{move ||shown.with(|s|s.reads.clone()).into_iter().map(|line|view!{<li>{line}</li>}).collect_view()}</ol>
                        </details>
                    </section>
                    <section class="oc-trace" data-onecalc="trace-panel" aria-labelledby="oc-trace-title">
                        <header><h2 id="oc-trace-title">"Evaluation trace"</h2>
                            <button type="button" data-onecalc="trace-start" on:click=start_trace>"Trace the formula"</button>
                            <button type="button" data-onecalc="trace-previous" on:click=move |_| show_step(trace_at.get_untracked().saturating_sub(1))>"Previous step"</button>
                            <button type="button" data-onecalc="trace-next" on:click=move |_| show_step(trace_at.get_untracked() + 1)>"Next step"</button>
                        </header>
                        <p class="oc-prov" role="status" data-onecalc="trace-status">{move || if trace.with(Option::is_none) {
                            "Select part of the formula and press F9 to see its value; Escape restores the formula. Or trace the whole formula, part by part.".to_owned()
                        } else if !trace_fresh() {
                            "The formula or its inputs changed since this trace. Trace it again.".to_owned()
                        } else {
                            trace.with(|trace| trace.as_ref().map(|(_, t)| format!("Step {} of {}", trace_at.get() + 1, t.steps.len())).unwrap_or_default())
                        }}</p>
                        <ol class="oc-trace-steps" data-onecalc="trace-steps">{move || {
                            if !trace_fresh() { return Vec::new(); }
                            let current = trace_at.get();
                            trace.with(|trace| trace.as_ref().map(|(_, t)| t.steps.iter().enumerate().map(|(index, step)| {
                                let class = format!(
                                    "oc-depth-{}{}",
                                    step.depth.min(8),
                                    if index == current { " oc-trace-current" } else { "" }
                                );
                                let (text, kind, value) = (step.text.clone(), step.kind.clone(), step.value.clone());
                                view! { <li class=class data-onecalc="trace-step" on:click=move |_| show_step(index)>
                                    <code data-onecalc="trace-part">{text}</code>" = "<span data-onecalc="trace-value" data-kind=kind>{value}</span>
                                </li> }
                            }).collect::<Vec<_>>()).unwrap_or_default())
                        }}</ol>
                        <p class="oc-prov" data-onecalc="trace-note">{move || trace.with(|trace| trace.as_ref().map(|(_, t)| t.note.clone()).unwrap_or_default())}</p>
                    </section>
                    <details class="oc-format-controls" data-onecalc="format-controls"><summary data-onecalc="format-tools">"Format & locale"</summary>
                        <label>"Locale"<select data-onecalc="locale" prop:value=move ||locale.get() on:change=move |ev|locale.set(event_target_value(&ev))>{SessionOptions::LOCALES.into_iter().map(|name|view!{<option value=name>{name}</option>}).collect_view()}</select></label>
                        <label>"Format"<select data-onecalc="format-picker" prop:value=move || {
                            let current = number_format.get();
                            let profile = FormatProfile::for_culture(&locale.get()).expect("listed locale");
                            if dna_formula_ui::result::format_choices(&profile).iter().any(|choice| choice.code == current) { current } else { "custom".to_owned() }
                        } on:change=move |ev| {
                            let code = event_target_value(&ev);
                            if code != "custom" { number_format.set(code); }
                        }>
                            <option value="custom" disabled=true>"Custom (type it below)"</option>
                            {move || {
                                let profile = FormatProfile::for_culture(&locale.get()).expect("listed locale");
                                dna_formula_ui::result::format_choices(&profile).into_iter().map(|choice| {
                                    let text = format!("{}: {} ({})", choice.group, choice.label, choice.code);
                                    view! { <option value=choice.code>{text}</option> }
                                }).collect_view()
                            }}
                        </select></label>
                        <label>"Date system"<select data-onecalc="date-system" prop:value=move || if date_1904.get() { "1904" } else { "1900" } on:change=move |ev| date_1904.set(event_target_value(&ev) == "1904")>
                            <option value="1900">"1900 (Excel for Windows, and the default)"</option>
                            <option value="1904">"1904 (older Excel for Mac)"</option>
                        </select></label>
                        <label>"Number format (selected locale)"<input data-onecalc="number-format" list="oc-number-formats" prop:value=move ||number_format.get() on:input=move |ev|number_format.set(event_target_value(&ev))/></label>
                        <datalist id="oc-number-formats">{move || {
                            let p=FormatProfile::for_culture(&locale.get()).expect("listed locale");
                            ["General".to_owned(),format!("0{}00",p.decimal_separator),format!("#{}##0{}00",p.thousands_separator,p.decimal_separator),format!("0{}00%",p.decimal_separator),"yyyy-mm-dd".to_owned()].into_iter().map(|code|view!{<option value=code/>}).collect_view()
                        }}</datalist>
                        <p class="oc-prov">{move || {
                            let p=FormatProfile::for_culture(&locale.get()).expect("listed locale");
                            format!("Number formats use the selected locale's separators, for example 0{}00. Changing locale keeps your format text; adjust its separators when needed.",p.decimal_separator)
                        }}</p>
                        <p class="oc-prov">"Formulas use English function names, commas between arguments and a dot for decimals. Numeric inputs also use a dot. Ctrl+Shift+Enter enters the formula as a legacy array formula, {=...}, as in Excel."</p>
                    </details>
                    {assist.help()}
                </div>
                <div class="oc-col">
                    <div class="oc-panel">
                        <h2>{format!("Scratch grid: {FIRST_SHEET}!A1:{}", grid::address(grid::ROWS, grid::COLUMNS))}</h2>
                        <div class="oc-gridwrap">
                            <table class="oc-grid" data-onecalc="grid">
                                <thead><tr><th></th>{grid_header}</tr></thead>
                                <tbody>{grid_body}</tbody>
                            </table>
                        </div>
                    </div>
                    <details class="oc-panel oc-vba-panel"><summary data-onecalc="vba-tools">"VBA functions"</summary>
                        <textarea
                            data-onecalc="vba-source"
                            aria-label="VBA standard module"
                            rows="8"
                            class="oc-vba-source"
                            prop:value=move || vba_source.get()
                            on:input=move |ev| vba_source.set(event_target_value(&ev))
                        />
                        <button data-onecalc="vba-load" type="button" on:click=load_vba>"Load module"</button>
                        " "
                        <button data-onecalc="vba-clear" type="button" on:click=clear_vba>"Unload module"</button>
                        <p data-onecalc="vba-status">{move || vba_status.get()}</p>
                        <p class="oc-prov">"Load the example, enter 40 in A1, then try =Module1.Add2(A1,2). Module state lasts until reload or unload. Each call stops at its execution budget. References can supply values, arrays or read-only Excel Range arguments; a write to a cell raises VBA error 1004, as in Excel, and calculation commands remain unsupported. In an .xlsm session the module goes into the file's VBA project and is saved with it; a module typed into a session without a project cannot be saved (dc-0f1k.4)."</p>
                    </details>
                    <div class="oc-panel oc-catalogue" data-onecalc="catalogue-panel">
                        <h2>"Functions"</h2>
                        <input
                            type="search"
                            data-onecalc="function-filter"
                            placeholder="Find a function by name or description"
                            aria-label="Find a function"
                            size="34"
                            on:input=move |ev| filter.set(event_target_value(&ev))
                        />
                        {function_list}
                    </div>
                </div>
            </div>
        </section>
    }
}
