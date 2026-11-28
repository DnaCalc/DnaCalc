use calc_skin::{Refusal, Selection, ViewId, ViewRequest, WorkbookId, grid::Key};
use dna_kit::commands::{CommandId, Layer, When};
use dna_kit::{Invocation, Registry, Session, view::PanelSpec};
use leptos::prelude::*;

enum Navigation {
    View {
        view: ViewId,
        selection: Option<Selection>,
    },
    Panel {
        panel: PanelSpec,
        shown: bool,
    },
}

const BACK: &str = "view.macro-back";

pub(super) fn install(registry: &Registry, session: Session) {
    let panels: Vec<_> = registry.docks().filter_map(|dock| dock.panel).collect();
    let history = StoredValue::new_local(Vec::<(WorkbookId, Navigation)>::new());
    session.on_macro_view(move |request| {
        let Some(stamp) = session.stamp() else {
            return;
        };
        let previous = match request {
            ViewRequest::Switch { view, sheet } => switch_view(session, view, sheet),
            ViewRequest::ShowPanel { panel, shown } => {
                match panels.iter().copied().find(|spec| spec.id == panel) {
                    Some(panel) => {
                        let previous = (panel.visible)(session);
                        if previous == shown || set_panel(session, panel, shown).is_err() {
                            None
                        } else {
                            Some(Navigation::Panel {
                                panel,
                                shown: previous,
                            })
                        }
                    }
                    None => {
                        session.refuse(Refusal::Invalid {
                            reason: format!("Panel {panel} is unavailable."),
                        });
                        None
                    }
                }
            }
        };
        if let Some(previous) = previous {
            history.update_value(|entries| {
                entries.retain(|(workbook, _)| *workbook == stamp.workbook);
                if entries.len() == 64 {
                    entries.remove(0);
                }
                entries.push((stamp.workbook, previous));
            });
        }
    });
    session.register(
        super::spec(BACK, "Previous view or cell", "!editing"),
        move |_| {
            let workbook = session.stamp().map(|stamp| stamp.workbook);
            let previous = history
                .try_update_value(|entries| {
                    entries.retain(|(key, _)| Some(*key) == workbook);
                    entries.pop().map(|(_, previous)| previous)
                })
                .flatten();
            match previous {
                Some(Navigation::View { view, selection }) => {
                    show_view(session, &view)?;
                    if let Some(selection) = selection {
                        session.select(selection);
                    }
                }
                Some(Navigation::Panel { panel, shown }) => set_panel(session, panel, shown)?,
                None => {
                    let _ = session.command("inspector.back");
                }
            }
            Ok(())
        },
    );
    session.bind(
        Layer::Component,
        When::parse("!editing").expect("a built-in condition parses"),
        "Alt+ArrowLeft",
        CommandId::new(BACK),
    );
}

fn switch_view(session: Session, view: ViewId, sheet: Option<String>) -> Option<Navigation> {
    let sheet = match sheet {
        None => None,
        Some(name) => {
            let found = session.summary().and_then(|book| {
                book.sheets.into_iter().find(|sheet| {
                    sheet.visible && sheet.worksheet && sheet.name.eq_ignore_ascii_case(&name)
                })
            });
            let Some(sheet) = found else {
                session.refuse(Refusal::Invalid {
                    reason: format!("Worksheet {name} is unavailable."),
                });
                return None;
            };
            Some(sheet.sheet)
        }
    };
    let previous = session.view();
    let selection = session.selection();
    if previous == view && sheet.is_none() {
        return None;
    }
    show_view(session, &view).ok()?;
    if let Some(sheet) = sheet
        && !session.active().is_some_and(|at| (at.0, at.1) == (sheet.0, sheet.1))
    {
        session.select_cell(Key(sheet.0, sheet.1, 1, 1));
    }
    Some(Navigation::View {
        view: previous,
        selection,
    })
}

fn show_view(session: Session, view: &ViewId) -> Result<(), String> {
    session
        .run(&Invocation::of(CommandId::from(
            format!("view.{}", view.0).as_str(),
        )))
        .map_err(|error| format!("{error:?}"))
}

fn set_panel(session: Session, panel: PanelSpec, shown: bool) -> Result<(), String> {
    session
        .run(&Invocation {
            command: CommandId::new(panel.visibility_command),
            args: shown.into(),
        })
        .map_err(|error| format!("{error:?}"))
}
