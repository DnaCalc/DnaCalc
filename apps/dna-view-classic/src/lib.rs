//! Classic: the workbook as Excel shows it, on the shared worksheet grid.

use calc_skin::ViewId;
use dna_kit::{MountedView, Registry, Session, View, ViewCommand, ViewSpec};

mod excel_key_tips;

pub const ID: &str = "classic";

struct Classic;

impl View for Classic {
    fn spec(&self) -> ViewSpec {
        ViewSpec {
            id: ViewId(ID.into()),
            title: "Classic",
            profile: "excel-365-win-en-us",
        }
    }

    fn commands(&self, session: Session) -> Vec<ViewCommand> {
        let view = self.spec().id;
        session.set_key_tips(view.clone(), excel_key_tips::EXCEL_KEY_TIPS);
        let mut commands = dna_grid::commands(&view, session);
        commands.extend(dna_kit::paste_special::commands(&view, session));
        commands.extend(dna_kit::sort_dialog::commands(&view, session));
        commands.extend(dna_kit::scenario_manager::commands(&view, session));
        commands.push(
            ViewCommand::new(&view, "classic.key-tips", "Key tips", &["F10"], move |_| {
                session.toggle_key_tips();
                Ok(())
            })
            .when("!editing"),
        );
        commands
    }

    fn mount(&self, session: Session) -> MountedView {
        MountedView::exact(leptos::view! {
            {dna_grid::mount_with_tints(session, move || {
                session.view_setting(ID, "provenance_tints")
                    .and_then(|setting| setting.as_bool())
                    .unwrap_or(false)
            })}
            {dna_kit::paste_special::mount(session)}
            {dna_kit::sort_dialog::mount(session)}
            {dna_kit::scenario_manager::mount(session)}
        })
    }

    fn cancel_interaction(&self, session: Session) -> bool {
        dna_kit::paste_special::close(session)
            || dna_kit::sort_dialog::close(session)
            || dna_kit::scenario_manager::close(session)
    }
}

pub fn install(registry: &mut Registry) {
    registry.view(Classic);
}
