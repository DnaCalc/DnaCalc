//! Classic: the workbook as Excel shows it, on the shared worksheet grid.

use calc_skin::ViewId;
use dna_kit::{MountedView, Registry, Session, View, ViewCommand, ViewSpec};

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
        dna_grid::commands(&self.spec().id, session)
    }

    fn mount(&self, session: Session) -> MountedView {
        MountedView::exact(dna_grid::mount(session))
    }
}

pub fn install(registry: &mut Registry) {
    registry.view(Classic);
}
