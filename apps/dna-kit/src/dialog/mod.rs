//! Classic's dialog boxes: one model, one frame, and one family per Excel dialog.

mod frame;
pub mod model;

pub use frame::{
    ACCEPT, CANCEL, Commit, Family, STANDARD_COLORS, Target, accept, change, close, commands, is_ready, key,
    mount, open, poll, press, probe, read, show, state, target,
};
pub use model::{Choice, Field, FieldId, Focus, KeyAction, Kind, Spec, State, Tab, Value, caption};

use leptos::prelude::*;

/// Open a mounted workbook dialog using native focus handling.
pub(crate) fn show_modal(element: NodeRef<leptos::html::Dialog>) {
    Effect::new(move |_| {
        if let Some(dialog) = element.get()
            && !dialog.open()
        {
            let _ = dialog.show_modal();
        }
    });
}
