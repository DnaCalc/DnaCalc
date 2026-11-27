//! Workbook view registration and the shared workbook stage data.

use calc_skin::{CellWindow, WorksheetGeometry, grid::Key};
use leptos::prelude::*;

#[derive(Clone, Copy)]
pub struct WorkbookContext {
    pub window: RwSignal<Option<CellWindow>>,
    pub geometry: RwSignal<Option<WorksheetGeometry>>,
    pub active: RwSignal<Option<Key>>,
    pub select: Callback<Key>,
    pub edit: Callback<(Key, Option<String>)>,
}

pub trait WorkbookView {
    fn mount(context: WorkbookContext) -> AnyView;
}

impl super::Registry {
    pub fn workbook<V: WorkbookView>(&mut self) {
        self.workbook = Some(V::mount);
    }

    pub fn mount_workbook(&self, context: WorkbookContext) -> Option<AnyView> {
        self.workbook.map(|mount| mount(context))
    }
}
