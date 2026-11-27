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
        MountedView::exact(dna_grid::mount(session))
    }
}

pub fn install(registry: &mut Registry) {
    registry.view(Classic);
}
