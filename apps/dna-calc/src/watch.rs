//! Every sheet's cells, kept current by one calc-host subscription per sheet
//! ([`calc_host::Interest::Sheet`]) while a skin that lists the whole workbook is shown: the tree
//! and the ledger. Each mirror is given its sheet's cells when it subscribes and every change
//! after, as the notebook's one mirror is for the shown sheet.

use crate::controller::Controller;
use calc_host::value::SheetId;
use calc_host::{DeltaMirror, Interest, SubscriptionId};
use leptos::prelude::*;

/// One sheet's cells: its identity, its name and the mirror its subscription keeps.
#[derive(Clone, Copy, Debug)]
pub struct SheetMirror {
    pub id: SheetId,
    pub name: StoredValue<String>,
    pub mirror: RwSignal<DeltaMirror>,
}

/// Subscribes to every sheet of the open workbook, again whenever another workbook is opened, and
/// unsubscribes when the calling skin goes. The sheets are in the workbook's order.
pub fn watch_sheets(controller: Controller) -> RwSignal<Vec<SheetMirror>> {
    let sheets: RwSignal<Vec<SheetMirror>> = RwSignal::new(Vec::new());
    let held: StoredValue<(u64, Vec<SubscriptionId>)> = StoredValue::new((0, Vec::new()));
    Effect::new(move |_| {
        let generation = controller.generation.get();
        // A workbook opened since: its session holds none of the old subscriptions.
        let (was, ids) = held.get_value();
        if was == generation {
            for id in ids {
                controller.unsubscribe(id);
            }
        }
        let mut mirrors = Vec::new();
        let mut ids = Vec::new();
        for tab in controller.with_untracked(|book| book.sheets()) {
            let mirror = RwSignal::new(DeltaMirror::default());
            if let Some(id) = controller.subscribe(Interest::Sheet(tab.id), mirror) {
                ids.push(id);
            }
            mirrors.push(SheetMirror {
                id: tab.id,
                name: StoredValue::new(tab.name),
                mirror,
            });
        }
        held.set_value((generation, ids));
        sheets.set(mirrors);
    });
    on_cleanup(move || {
        let (generation, ids) = held.get_value();
        if generation == controller.generation.get_untracked() {
            for id in ids {
                controller.unsubscribe(id);
            }
        }
    });
    sheets
}
