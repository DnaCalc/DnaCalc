//! The Strand theme of the page (dc-xib): the system's choice by default, or one the user names,
//! kept in the browser's local storage under [`STORAGE_KEY`] for both apps.
//!
//! The token stylesheet (`dna_strand::stylesheet`, the first part of each bundle's `app.css`)
//! gives `cockpit-light` by default and `cockpit-dark` when the system prefers a dark scheme;
//! `<html data-dna-theme="...">` names a theme outright. [`set`] sets or removes that attribute
//! and remembers the choice; the page's start module (`web/boot.js`) applies a remembered choice
//! before the app loads.

/// The local-storage key both apps keep the chosen theme under.
pub const STORAGE_KEY: &str = "dna-theme";

/// The choices a theme switch offers: the attribute value (empty for the system's choice) and
/// its label.
pub const CHOICES: [(&str, &str); 4] = [
    ("", "System theme"),
    ("cockpit-light", "Light"),
    ("cockpit-dark", "Dark"),
    ("high-contrast", "High contrast"),
];

/// The theme the page names now (empty: the system's choice).
pub fn current() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        leptos::prelude::document()
            .document_element()
            .and_then(|root| root.get_attribute("data-dna-theme"))
            .unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        String::new()
    }
}

/// Names `theme` on the page (empty: the system's choice) and remembers it. A value that is not
/// one of [`CHOICES`] is taken as the system's choice.
pub fn set(theme: &str) {
    let theme = if CHOICES.iter().any(|(id, _)| *id == theme) {
        theme
    } else {
        ""
    };
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(root) = leptos::prelude::document().document_element() {
            let _ = if theme.is_empty() {
                root.remove_attribute("data-dna-theme")
            } else {
                root.set_attribute("data-dna-theme", theme)
            };
        }
        if let Ok(Some(storage)) = leptos::prelude::window().local_storage() {
            let _ = if theme.is_empty() {
                storage.remove_item(STORAGE_KEY)
            } else {
                storage.set_item(STORAGE_KEY, theme)
            };
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = theme;
    }
}

#[cfg(test)]
mod tests {
    use super::CHOICES;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn every_named_choice_is_a_strand_theme_and_every_theme_is_offered() {
        let named: Vec<&str> = CHOICES
            .iter()
            .map(|(id, _)| *id)
            .filter(|id| !id.is_empty())
            .collect();
        let themes: Vec<&str> = dna_strand::Theme::ALL.iter().map(|t| t.id()).collect();
        assert_eq!(named, themes);
    }
}
