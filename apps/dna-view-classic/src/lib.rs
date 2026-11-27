//! Classic mounts the shared worksheet grid.

use dna_kit::{Registry, WorkbookContext, WorkbookView};
use leptos::prelude::*;

struct Classic;

impl WorkbookView for Classic {
    fn mount(context: WorkbookContext) -> AnyView {
        dna_grid::mount(context).into_any()
    }
}

pub fn install(registry: &mut Registry) {
    registry.workbook::<Classic>();
}
