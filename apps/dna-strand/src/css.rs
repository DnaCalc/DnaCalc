//! CSS custom-property emitter — STRAND_DESIGN_LANGUAGE.md §6 (Theming) /
//! SHELL_SPEC.md §8: "Token layer `--dna-*` ... Implementation: token
//! definitions in one Strand crate consumed by every skin crate; no
//! per-skin stylesheets beyond component-scoped rules resolving through
//! tokens."
//!
//! [`css_custom_properties`] is the one place that turns a
//! [`crate::theme::Theme`] + [`crate::theme::Density`] pair into that
//! `--dna-*` block. Property order is fixed by [`PROPERTY_ORDER`] and never
//! depends on hash-map iteration, so the same input always emits
//! byte-identical output (see `tests::css_custom_properties_is_deterministic`).

use crate::geometry::{
    GAP_SCALE, MOTION_DURATION_MAX_MS, MOTION_DURATION_MIN_MS, RADIUS_CARD, RADIUS_CHIP,
    RADIUS_PANEL,
};
use crate::palette::AMBER;
use crate::theme::{Density, Theme};

/// The stable emission order of every `--dna-*` custom property.
/// `css_custom_properties` and `tests::property_order_is_exactly_expected`
/// both consult this list, so emitter and test can never silently diverge.
const PROPERTY_ORDER: &[&str] = &[
    "theme",
    "density",
    "bg",
    "paper",
    "paper-2",
    "ink",
    "ink-2",
    "ink-3",
    "line",
    "line-soft",
    "chrome",
    "chrome-2",
    "chrome-ink",
    "chrome-ink-2",
    "accent",
    "accent-soft",
    "amber",
    "amber-soft",
    "green-soft",
    "signal-soft",
    "red-soft",
    "accent-ink",
    "value-ink",
    "prov-const",
    "prov-ext",
    "red-ink",
    "radius-chip",
    "radius-card",
    "radius-panel",
    "gap-1",
    "gap-2",
    "gap-3",
    "gap-4",
    "gap-5",
    "gap-6",
    "motion-min",
    "motion-max",
    "line-height",
    "measure",
];

fn property_value(
    name: &str,
    theme: Theme,
    t: &crate::theme::ThemeTokens,
    density: Density,
) -> String {
    match name {
        "theme" => theme.id().to_string(),
        "density" => density.id().to_string(),
        "bg" => t.bg.to_hex(),
        "paper" => t.paper.to_hex(),
        "paper-2" => t.paper_2.to_hex(),
        "ink" => t.ink.to_hex(),
        "ink-2" => t.ink_2.to_hex(),
        "ink-3" => t.ink_3.to_hex(),
        "line" => t.line.to_hex(),
        "line-soft" => t.line_soft.to_hex(),
        "chrome" => t.chrome.to_hex(),
        "chrome-2" => t.chrome_2.to_hex(),
        "chrome-ink" => t.chrome_ink.to_hex(),
        "chrome-ink-2" => t.chrome_ink_2.to_hex(),
        "accent" => t.accent.to_hex(),
        "accent-soft" => t.accent_soft.to_hex(),
        // The one non-themed color (STRAND §2: amber is the single saturated
        // attention channel, unchanged by theme). `ThemeTokens::resolve("amber")`
        // already answers it; emitting it here is what lets a stylesheet resolve
        // `var(--dna-amber)` — the mast dirty dot and the stale liveness dot
        // referenced it and painted transparent until then (dtc-j7n8.26).
        "amber" => AMBER.to_hex(),
        "amber-soft" => t.amber_soft.to_hex(),
        "green-soft" => t.green_soft.to_hex(),
        "signal-soft" => t.signal_soft.to_hex(),
        "red-soft" => t.red_soft.to_hex(),
        "accent-ink" => t.accent_ink.to_hex(),
        "value-ink" => t.value_ink.to_hex(),
        "prov-const" => t.prov_const.to_hex(),
        "prov-ext" => t.prov_ext.to_hex(),
        "red-ink" => t.red_ink.to_hex(),
        "radius-chip" => format!("{RADIUS_CHIP}px"),
        "radius-card" => format!("{RADIUS_CARD}px"),
        "radius-panel" => format!("{RADIUS_PANEL}px"),
        "gap-1" => format!("{}px", GAP_SCALE[0]),
        "gap-2" => format!("{}px", GAP_SCALE[1]),
        "gap-3" => format!("{}px", GAP_SCALE[2]),
        "gap-4" => format!("{}px", GAP_SCALE[3]),
        "gap-5" => format!("{}px", GAP_SCALE[4]),
        "gap-6" => format!("{}px", GAP_SCALE[5]),
        "motion-min" => format!("{MOTION_DURATION_MIN_MS}ms"),
        "motion-max" => format!("{MOTION_DURATION_MAX_MS}ms"),
        "line-height" => format!("{}", density.line_height()),
        "measure" => density.measure().unwrap_or("none").to_string(),
        _ => unreachable!("PROPERTY_ORDER lists {name:?} with no value mapping in property_value"),
    }
}

/// Emit a stable-ordered `:root { --dna-*: ...; }` block for `theme` +
/// `density`. Colors come from `theme.tokens()`; geometry/motion are
/// theme- and density-invariant constants; `line-height`/`measure` vary
/// with `density` only (STRAND §5: density affects spacing/type-scale
/// tokens, never colors).
pub fn css_custom_properties(theme: Theme, density: Density) -> String {
    custom_properties(":root", theme, density, false)
}

/// The `--dna-*` block for `theme` and `density` under `selector`; with `scheme`, also the
/// theme's `color-scheme` (so the browser's own controls and scroll bars follow it).
fn custom_properties(selector: &str, theme: Theme, density: Density, scheme: bool) -> String {
    let tokens = theme.tokens();
    let mut out = format!("{selector} {{\n");
    if scheme {
        let dark = matches!(theme, Theme::CockpitDark);
        out.push_str(&format!(
            "  color-scheme: {};\n",
            if dark { "dark" } else { "light" }
        ));
    }
    for name in PROPERTY_ORDER {
        let value = property_value(name, theme, &tokens, density);
        out.push_str(&format!("  --dna-{name}: {value};\n"));
    }
    out.push_str("}\n");
    out
}

/// The token stylesheet both apps load before their own (`tokens.css`, written by `cargo xtask
/// app bundle`): `cockpit-light` by default; `cockpit-dark` when the system prefers a dark scheme
/// and the page names no theme; and each theme when `<html data-dna-theme="...">` names it (the
/// apps' theme switch sets it). Working density throughout.
pub fn stylesheet() -> String {
    let density = Density::Working;
    let mut out = String::from(
        "/* Strand design tokens (dna-strand, dc-xib). Generated by `cargo xtask app bundle`. */\n",
    );
    out.push_str(&custom_properties(
        ":root",
        Theme::CockpitLight,
        density,
        true,
    ));
    out.push_str("@media (prefers-color-scheme: dark) {\n");
    out.push_str(&custom_properties(
        ":root:not([data-dna-theme])",
        Theme::CockpitDark,
        density,
        true,
    ));
    out.push_str("}\n");
    for theme in Theme::ALL {
        out.push_str(&custom_properties(
            &format!(":root[data-dna-theme=\"{}\"]", theme.id()),
            theme,
            density,
            true,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden snapshot for the default combination (`cockpit-light` +
    /// `working`). Kept as a plain string constant, not `insta`, per the
    /// bead's ask — readable, and a diff on failure shows exactly which
    /// token moved.
    const EXPECTED_COCKPIT_LIGHT_WORKING: &str = ":root {
  --dna-theme: cockpit-light;
  --dna-density: working;
  --dna-bg: #F4F7F8;
  --dna-paper: #FFFFFF;
  --dna-paper-2: #EEF2F4;
  --dna-ink: #17282E;
  --dna-ink-2: #48606A;
  --dna-ink-3: #5B717B;
  --dna-line: #D5DFE3;
  --dna-line-soft: #E4EBEE;
  --dna-chrome: #193F49;
  --dna-chrome-2: #12303A;
  --dna-chrome-ink: #DFEEF2;
  --dna-chrome-ink-2: #8FB3BC;
  --dna-accent: #318995;
  --dna-accent-soft: #E2EEF0;
  --dna-amber: #FFB301;
  --dna-amber-soft: #FFF4DB;
  --dna-green-soft: #E4EFEC;
  --dna-signal-soft: #FFEDDB;
  --dna-red-soft: #F8E1E0;
  --dna-accent-ink: #1B5F6B;
  --dna-value-ink: #2E6E5B;
  --dna-prov-const: #2D6A8A;
  --dna-prov-ext: #A35400;
  --dna-red-ink: #D02A23;
  --dna-radius-chip: 5px;
  --dna-radius-card: 9px;
  --dna-radius-panel: 14px;
  --dna-gap-1: 2px;
  --dna-gap-2: 4px;
  --dna-gap-3: 6px;
  --dna-gap-4: 10px;
  --dna-gap-5: 14px;
  --dna-gap-6: 22px;
  --dna-motion-min: 120ms;
  --dna-motion-max: 160ms;
  --dna-line-height: 1.35;
  --dna-measure: none;
}
";

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn css_custom_properties_cockpit_light_working_matches_golden() {
        assert_eq!(
            css_custom_properties(Theme::CockpitLight, Density::Working),
            EXPECTED_COCKPIT_LIGHT_WORKING
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_stylesheet_defaults_light_follows_the_system_and_names_every_theme() {
        let css = stylesheet();
        assert!(css.contains(":root {\n  color-scheme: light;\n  --dna-theme: cockpit-light;"));
        assert!(css.contains(
            "@media (prefers-color-scheme: dark) {\n:root:not([data-dna-theme]) {\n  color-scheme: dark;\n  --dna-theme: cockpit-dark;"
        ));
        for theme in Theme::ALL {
            assert!(css.contains(&format!(":root[data-dna-theme=\"{}\"] {{", theme.id())));
        }
        assert_eq!(css.matches('{').count(), css.matches('}').count());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn css_custom_properties_is_deterministic() {
        for theme in Theme::ALL {
            for density in Density::ALL {
                let a = css_custom_properties(theme, density);
                let b = css_custom_properties(theme, density);
                assert_eq!(a, b);
            }
        }
    }

    fn property_names_in_order(css: &str) -> Vec<String> {
        css.lines()
            .filter_map(|line| {
                let line = line.trim();
                let rest = line.strip_prefix("--dna-")?;
                let (name, _) = rest.split_once(':')?;
                Some(name.to_string())
            })
            .collect()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn property_order_is_exactly_expected_for_every_theme_and_density() {
        for theme in Theme::ALL {
            for density in Density::ALL {
                let css = css_custom_properties(theme, density);
                assert_eq!(
                    property_names_in_order(&css),
                    PROPERTY_ORDER.to_vec(),
                    "property order drifted for {theme:?}/{density:?}"
                );
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn every_combination_starts_and_ends_correctly_and_has_no_duplicate_keys() {
        for theme in Theme::ALL {
            for density in Density::ALL {
                let css = css_custom_properties(theme, density);
                assert!(css.starts_with(":root {\n"));
                assert!(css.ends_with("}\n"));
                let names = property_names_in_order(&css);
                let unique: std::collections::HashSet<_> = names.iter().collect();
                assert_eq!(unique.len(), names.len(), "duplicate --dna-* key emitted");
            }
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn density_changes_leading_and_measure_only_not_colors() {
        let working = css_custom_properties(Theme::CockpitDark, Density::Working);
        let reading = css_custom_properties(Theme::CockpitDark, Density::Reading);
        // Strip the density-owned lines, then the rest must be identical.
        let strip = |css: &str| {
            css.lines()
                .filter(|l| {
                    let t = l.trim();
                    !t.starts_with("--dna-density:")
                        && !t.starts_with("--dna-line-height:")
                        && !t.starts_with("--dna-measure:")
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(strip(&working), strip(&reading));
        assert_ne!(working, reading);
    }
}
