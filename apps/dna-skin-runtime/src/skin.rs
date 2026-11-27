//! Portable validation for declarative skins and pages.

use crate::commands::{Keymap, KeymapError};
use calc_skin::{Bound, Page, SkinManifest};
use dna_strand::theme::{Density, Theme};
use dna_strand::{ContrastReading, ResolvedTheme, SANCTIONED_PAIRS, ThemeError};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub enum Document {
    Skin(Box<SkinManifest>),
    Page(Page),
}

#[derive(Debug)]
pub struct Validated {
    pub document: Document,
    pub theme: Option<Theme>,
    pub density: Option<Density>,
    /// Colours are keyed by their canonical Strand token.
    pub colours: BTreeMap<String, String>,
    pub formula_size_px: Option<f64>,
    pub contrast_pairs: usize,
}

impl Validated {
    /// The validated theme colours and density as Strand declarations for one surface.
    pub fn inline_style(&self) -> Option<String> {
        let mut style = dna_strand::css::inline_style(self.theme?, self.density?);
        for (token, colour) in &self.colours {
            style.push_str(&format!("--dna-{token}:{colour};"));
        }
        if let Some(size) = self.formula_size_px {
            style.push_str(&format!("--dna-formula-font-size:{size}px;"));
        }
        Some(style)
    }
}

pub fn validate(source: &str) -> Result<Validated, String> {
    let value: serde_json::Value =
        serde_json::from_str(source).map_err(|error| error.to_string())?;
    match value.get("schema").and_then(serde_json::Value::as_str) {
        Some("dna.skin/1") => {
            validate_skin(serde_json::from_str(source).map_err(|error| error.to_string())?)
        }
        Some("dna.page/1") => {
            let page = serde_json::from_str(source).map_err(|error| error.to_string())?;
            validate_page(&page)?;
            Ok(Validated {
                document: Document::Page(page),
                theme: None,
                density: None,
                colours: BTreeMap::new(),
                formula_size_px: None,
                contrast_pairs: 0,
            })
        }
        _ => Err("The schema must be dna.skin/1 or dna.page/1".into()),
    }
}

fn validate_skin(manifest: SkinManifest) -> Result<Validated, String> {
    if manifest.id.trim().is_empty() || manifest.name.trim().is_empty() {
        return Err("A skin needs an ID and a name".into());
    }
    if let Some(base) = &manifest.extends
        && !matches!(base.as_str(), "dna:default" | "dna:explorer")
    {
        return Err(format!("Unknown base skin {base}"));
    }
    let StyleDraft {
        theme,
        density,
        colours,
        formula_size_px,
        palette,
    } = skin_palette(&manifest)?;
    palette.check_contrast().map_err(theme_error)?;
    if let Some(keys) = &manifest.keymap {
        if keys.profile != "excel-365-win-en-us" {
            return Err(format!("Unknown keymap profile {}", keys.profile));
        }
        Keymap::validate_skin_bindings(&keys.bindings).map_err(|errors| {
            errors
                .into_iter()
                .map(keymap_error)
                .collect::<Vec<_>>()
                .join("\n")
        })?;
    }
    let mut commands = BTreeSet::new();
    for command in &manifest.commands {
        if !command.id.starts_with("skin.")
            || command.id.len() == "skin.".len()
            || command.label.trim().is_empty()
            || command.steps.is_empty()
            || !commands.insert(&command.id)
        {
            return Err(format!("Invalid or duplicate skin command {}", command.id));
        }
        if command
            .steps
            .iter()
            .any(|(id, _)| id.trim().is_empty() || id.starts_with("skin."))
        {
            return Err(format!(
                "Skin command {} must name built-in steps",
                command.id
            ));
        }
    }
    for (id, page) in &manifest.pages {
        validate_page(page).map_err(|reason| format!("Page {id} · {reason}"))?;
    }
    Ok(Validated {
        document: Document::Skin(Box::new(manifest)),
        theme: Some(theme),
        density: Some(density),
        colours,
        formula_size_px,
        contrast_pairs: SANCTIONED_PAIRS
            .iter()
            .filter(|pair| pair.theme == theme)
            .count(),
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeReview {
    pub theme: Theme,
    pub contrast: Vec<ContrastReading>,
}

/// The Studio's live contrast results, including failing pairs from the draft.
pub fn review_theme(source: &str) -> Result<ThemeReview, String> {
    let manifest = serde_json::from_str(source).map_err(|error| error.to_string())?;
    let draft = skin_palette(&manifest)?;
    Ok(ThemeReview {
        theme: draft.theme,
        contrast: draft.palette.contrast_readings().collect(),
    })
}

struct StyleDraft {
    theme: Theme,
    density: Density,
    colours: BTreeMap<String, String>,
    formula_size_px: Option<f64>,
    palette: ResolvedTheme,
}

fn skin_palette(manifest: &SkinManifest) -> Result<StyleDraft, String> {
    let theme = Theme::ALL
        .into_iter()
        .find(|theme| theme.id() == manifest.theme.base)
        .ok_or_else(|| format!("Unknown theme {}", manifest.theme.base))?;
    let density = Density::ALL
        .into_iter()
        .find(|density| density.id() == manifest.theme.density)
        .ok_or_else(|| format!("Unknown density {}", manifest.theme.density))?;
    let mut colours = BTreeMap::new();
    let mut formula_size_px = None;
    for (key, value) in &manifest.theme.overrides {
        let token = match key.as_str() {
            "ink.secondary" => "ink-2",
            "surface.panel" => "paper-2",
            "formula.font.size" => {
                let size: f64 = value
                    .strip_suffix("px")
                    .and_then(|value| value.parse().ok())
                    .filter(|size| (10.0..=32.0).contains(size))
                    .ok_or_else(|| "Formula font size must be 10–32 px".to_owned())?;
                formula_size_px = Some(size);
                continue;
            }
            "bg" | "paper" | "paper-2" | "ink" | "ink-2" | "ink-3" | "line" | "line-soft"
            | "chrome" | "chrome-2" | "chrome-ink" | "chrome-ink-2" | "accent" | "accent-ink"
            | "accent-soft" => key,
            _ => return Err(format!("The token {key} cannot be changed by a skin")),
        };
        if colours.insert(token.to_owned(), value.clone()).is_some() {
            return Err(format!("Two overrides name the Strand token {token}"));
        }
    }
    let palette = dna_strand::resolve_draft(
        theme,
        colours
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    )
    .map_err(theme_error)?;
    for (key, value) in &mut colours {
        *value = palette
            .colour(key)
            .expect("a resolved override has a colour")
            .to_hex();
    }
    Ok(StyleDraft {
        theme,
        density,
        colours,
        formula_size_px,
        palette,
    })
}

fn theme_error(error: ThemeError) -> String {
    match error {
        ThemeError::Contrast {
            foreground,
            background,
            ratio,
            minimum,
        } => {
            format!(
                "{foreground} on {background} has {ratio:.4}:1 contrast, minimum {minimum:.1}:1"
            )
        }
        ThemeError::InvalidColour { token, value } => {
            format!("{token} needs an sRGB hex colour, received {value}")
        }
        ThemeError::UnknownToken(token) | ThemeError::FixedToken(token) => {
            format!("The token {token} cannot be changed")
        }
    }
}

/// Describe a keymap validation diagnostic for the CLI and Studio.
pub fn keymap_error(error: KeymapError) -> String {
    match error {
        KeymapError::Protected(chord) => format!("{chord} is reserved"),
        KeymapError::Collision(chord) => format!("Overlapping skin bindings for {chord}"),
        KeymapError::Chord { chord, reason } => format!("{chord} · {reason}"),
        KeymapError::When { when, reason } => format!("{when} · {reason}"),
        KeymapError::UnknownCommand(command) => format!("Unknown command {command}"),
    }
}

fn validate_page(page: &Page) -> Result<(), String> {
    if page.title.trim().is_empty() {
        return Err("A page needs a title".into());
    }
    let mut seen = BTreeSet::new();
    page.root.try_for_each_part(&mut |id, component, slots| {
        if id.trim().is_empty() || !seen.insert(id.to_owned()) {
            return Err(format!("Invalid or duplicate page part {id}"));
        }
        if component.trim().is_empty() {
            return Err(format!("Page part {id} needs a component"));
        }
        if slots.values().any(|bound| bound.address.trim().is_empty()) {
            return Err(format!("Page part {id} has an empty source address"));
        }
        let required = match component {
            "input" | "value" => Some("value"),
            "data-frame" | "notebook-excerpt" => Some("rows"),
            "chart" => Some("object"),
            _ => None,
        };
        if let Some(slot) = required
            && !slots.contains_key(slot)
        {
            return Err(format!("Page part {id} needs the {slot} binding"));
        }
        if component == "clock" {
            for slot in ["tick", "rate", "running", "step", "reset", "stopped"] {
                if !slots.contains_key(slot) {
                    return Err(format!("Page part {id} needs the {slot} binding"));
                }
            }
        }
        Ok(())
    })
}

pub fn bindings(document: &Document) -> Vec<&Bound> {
    let mut bindings = Vec::new();
    match document {
        Document::Skin(manifest) => {
            for page in manifest.pages.values() {
                collect_bindings(page, &mut bindings);
            }
        }
        Document::Page(page) => collect_bindings(page, &mut bindings),
    }
    bindings
}

fn collect_bindings<'a>(page: &'a Page, out: &mut Vec<&'a Bound>) {
    let _: Result<(), std::convert::Infallible> =
        page.root.try_for_each_part(&mut |_, _, slots| {
            out.extend(slots.values());
            Ok(())
        });
}

#[cfg(test)]
mod tests {
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn theme_review_reports_every_pair_and_retains_the_blocking_measurement() {
        let manifest = serde_json::json!({
            "schema": "dna.skin/1",
            "id": "audit",
            "name": "Calm audit",
            "theme": {
                "base": "cockpit-dark",
                "density": "working",
                "overrides": { "ink.secondary": "#7D97A0", "surface.panel": "#22343C" }
            }
        });
        let review = super::review_theme(&manifest.to_string()).unwrap();
        assert_eq!(review.contrast.len(), 61);
        let failed: Vec<_> = review
            .contrast
            .iter()
            .filter(|pair| !pair.passes())
            .collect();
        assert_eq!(failed.len(), 2);
        for pair in failed {
            assert_eq!(pair.minimum, 4.5);
            assert_eq!(pair.background, "paper-2");
            assert!((4.1898..4.1900).contains(&pair.ratio), "{pair:?}");
        }
        let mut corrected = manifest;
        corrected["theme"]["overrides"]["ink.secondary"] = serde_json::json!("#A4B8C2");
        corrected["theme"]["overrides"]["ink-3"] = serde_json::json!("#A4B8C2");
        let source = corrected.to_string();
        assert!(
            super::review_theme(&source)
                .unwrap()
                .contrast
                .into_iter()
                .all(|pair| pair.passes())
        );
        assert!(super::validate(&source).is_ok());
    }
}
