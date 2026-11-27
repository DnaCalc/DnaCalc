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
