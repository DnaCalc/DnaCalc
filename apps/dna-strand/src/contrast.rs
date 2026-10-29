//! The contrast law — STRAND_DESIGN_LANGUAGE.md §2.1.
//!
//! "Every sanctioned foreground/background pair is a token pair, and the
//! Strand crate asserts the ratios in unit tests — a palette change that
//! breaks a pair fails the build, not the reader." This module is that
//! assertion: WCAG 2.x relative luminance and contrast ratio, plus
//! [`SANCTIONED_PAIRS`], the table the tests below iterate.

use crate::palette::Rgb;
use crate::theme::Theme;

/// WCAG 2.x relative luminance of an sRGB color: linearize each channel,
/// then combine with the standard coefficients (0.2126 / 0.7152 / 0.0722).
pub fn relative_luminance(c: Rgb) -> f64 {
    fn linearize(channel: u8) -> f64 {
        let c = channel as f64 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linearize(c.r) + 0.7152 * linearize(c.g) + 0.0722 * linearize(c.b)
}

/// WCAG 2.x contrast ratio between two colors: `(L1 + 0.05) / (L2 + 0.05)`,
/// where `L1` is the greater of the two relative luminances. Symmetric in
/// `a`/`b`; always >= 1.0.
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// One sanctioned foreground/background pair. `fg`/`bg` are Strand token ids
/// (see [`crate::theme::ThemeTokens::resolve`]); `theme` picks which
/// theme's tokens they resolve against; `min_ratio` is the WCAG ratio the
/// pair must reach.
#[derive(Debug, Clone, Copy)]
pub struct SanctionedPair {
    pub theme: Theme,
    pub fg: &'static str,
    pub bg: &'static str,
    pub min_ratio: f64,
}

const fn pair(theme: Theme, fg: &'static str, bg: &'static str, min_ratio: f64) -> SanctionedPair {
    SanctionedPair {
        theme,
        fg,
        bg,
        min_ratio,
    }
}

/// The foreground/background pairs the Strand contrast law sanctions,
/// covering STRAND §2.1 at minimum:
/// - `ink` / `ink-2` / `ink-3` vs `paper` and `paper-2`: >= 4.5:1 (>= 7.0:1
///   under `high-contrast`).
/// - `accent-ink`, `value-ink`, `prov-const`, `prov-ext`, `red` vs `paper`
///   (every theme) and `paper-2` (`cockpit-light` and `high-contrast`, where
///   `paper-2` is the tighter binding ground; `cockpit-dark`'s `paper-2` is
///   darker than its `paper`, so it never binds and is not asserted here):
///   >= 4.5:1 (>= 7.0:1 under `high-contrast`).
/// - `chrome-ink` vs `chrome` and `chrome-2`: >= 4.5:1 (>= 7.0:1 under
///   `high-contrast`).
/// - `chrome-ink-2` vs `chrome` and `chrome-2`: >= 4.0:1 (>= 7.0:1 under
///   `high-contrast`).
/// - `amber` vs `chrome` and `chrome-2` (large-glyph class, not body text):
///   >= 3.0:1 in every theme, `high-contrast` included.
///
/// `Theme::ALL` all appear here (asserted by
/// `tests::every_theme_has_sanctioned_coverage`); `tests::
/// all_sanctioned_pairs_meet_contrast_law` is the build-breaking assertion
/// itself.
pub const SANCTIONED_PAIRS: &[SanctionedPair] = &[
    // --- cockpit-light ---
    pair(Theme::CockpitLight, "ink", "paper", 4.5),
    pair(Theme::CockpitLight, "ink", "paper-2", 4.5),
    pair(Theme::CockpitLight, "ink-2", "paper", 4.5),
    pair(Theme::CockpitLight, "ink-2", "paper-2", 4.5),
    pair(Theme::CockpitLight, "ink-3", "paper", 4.5),
    pair(Theme::CockpitLight, "ink-3", "paper-2", 4.5),
    pair(Theme::CockpitLight, "accent-ink", "paper", 4.5),
    pair(Theme::CockpitLight, "accent-ink", "paper-2", 4.5),
    pair(Theme::CockpitLight, "value-ink", "paper", 4.5),
    pair(Theme::CockpitLight, "value-ink", "paper-2", 4.5),
    pair(Theme::CockpitLight, "prov-const", "paper", 4.5),
    pair(Theme::CockpitLight, "prov-const", "paper-2", 4.5),
    pair(Theme::CockpitLight, "prov-ext", "paper", 4.5),
    pair(Theme::CockpitLight, "prov-ext", "paper-2", 4.5),
    pair(Theme::CockpitLight, "red", "paper", 4.5),
    pair(Theme::CockpitLight, "red", "paper-2", 4.5),
    pair(Theme::CockpitLight, "chrome-ink", "chrome", 4.5),
    pair(Theme::CockpitLight, "chrome-ink", "chrome-2", 4.5),
    pair(Theme::CockpitLight, "chrome-ink-2", "chrome", 4.0),
    pair(Theme::CockpitLight, "chrome-ink-2", "chrome-2", 4.0),
    pair(Theme::CockpitLight, "amber", "chrome", 3.0),
    pair(Theme::CockpitLight, "amber", "chrome-2", 3.0),
    // --- cockpit-dark ---
    pair(Theme::CockpitDark, "ink", "paper", 4.5),
    pair(Theme::CockpitDark, "ink", "paper-2", 4.5),
    pair(Theme::CockpitDark, "ink-2", "paper", 4.5),
    pair(Theme::CockpitDark, "ink-2", "paper-2", 4.5),
    pair(Theme::CockpitDark, "ink-3", "paper", 4.5),
    pair(Theme::CockpitDark, "ink-3", "paper-2", 4.5),
    pair(Theme::CockpitDark, "accent-ink", "paper", 4.5),
    pair(Theme::CockpitDark, "value-ink", "paper", 4.5),
    pair(Theme::CockpitDark, "prov-const", "paper", 4.5),
    pair(Theme::CockpitDark, "prov-ext", "paper", 4.5),
    pair(Theme::CockpitDark, "red", "paper", 4.5),
    pair(Theme::CockpitDark, "chrome-ink", "chrome", 4.5),
    pair(Theme::CockpitDark, "chrome-ink", "chrome-2", 4.5),
    pair(Theme::CockpitDark, "chrome-ink-2", "chrome", 4.0),
    pair(Theme::CockpitDark, "chrome-ink-2", "chrome-2", 4.0),
    pair(Theme::CockpitDark, "amber", "chrome", 3.0),
    pair(Theme::CockpitDark, "amber", "chrome-2", 3.0),
    // --- high-contrast: every text pair >= 7.0:1; amber keeps its 3.0:1
    // large-glyph minimum (STRAND's large-text/graphical class) ---
    pair(Theme::HighContrast, "ink", "paper", 7.0),
    pair(Theme::HighContrast, "ink", "paper-2", 7.0),
    pair(Theme::HighContrast, "ink-2", "paper", 7.0),
    pair(Theme::HighContrast, "ink-2", "paper-2", 7.0),
    pair(Theme::HighContrast, "ink-3", "paper", 7.0),
    pair(Theme::HighContrast, "ink-3", "paper-2", 7.0),
    pair(Theme::HighContrast, "accent-ink", "paper", 7.0),
    pair(Theme::HighContrast, "accent-ink", "paper-2", 7.0),
    pair(Theme::HighContrast, "value-ink", "paper", 7.0),
    pair(Theme::HighContrast, "value-ink", "paper-2", 7.0),
    pair(Theme::HighContrast, "prov-const", "paper", 7.0),
    pair(Theme::HighContrast, "prov-const", "paper-2", 7.0),
    pair(Theme::HighContrast, "prov-ext", "paper", 7.0),
    pair(Theme::HighContrast, "prov-ext", "paper-2", 7.0),
    pair(Theme::HighContrast, "red", "paper", 7.0),
    pair(Theme::HighContrast, "red", "paper-2", 7.0),
    pair(Theme::HighContrast, "chrome-ink", "chrome", 7.0),
    pair(Theme::HighContrast, "chrome-ink", "chrome-2", 7.0),
    pair(Theme::HighContrast, "chrome-ink-2", "chrome", 7.0),
    pair(Theme::HighContrast, "chrome-ink-2", "chrome-2", 7.0),
    pair(Theme::HighContrast, "amber", "chrome", 3.0),
    pair(Theme::HighContrast, "amber", "chrome-2", 3.0),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn relative_luminance_of_black_is_zero_and_white_is_one() {
        assert_eq!(relative_luminance(Rgb { r: 0, g: 0, b: 0 }), 0.0);
        assert!(
            (relative_luminance(Rgb {
                r: 255,
                g: 255,
                b: 255
            }) - 1.0)
                .abs()
                < 1e-9
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contrast_ratio_black_on_white_is_21_to_1() {
        let black = Rgb { r: 0, g: 0, b: 0 };
        let white = Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        assert!((contrast_ratio(black, white) - 21.0).abs() < 1e-9);
        // Symmetric regardless of argument order.
        assert_eq!(contrast_ratio(black, white), contrast_ratio(white, black));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn contrast_ratio_of_a_color_with_itself_is_one() {
        let c = Rgb {
            r: 100,
            g: 150,
            b: 200,
        };
        assert!((contrast_ratio(c, c) - 1.0).abs() < 1e-9);
    }

    /// Every `Theme` variant must appear in `SANCTIONED_PAIRS` — the test
    /// below iterates "all themes x pairs" (STRAND §2.1); a theme with zero
    /// rows would silently escape the contrast law.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn every_theme_has_sanctioned_coverage() {
        for theme in Theme::ALL {
            assert!(
                SANCTIONED_PAIRS.iter().any(|p| p.theme == theme),
                "no SANCTIONED_PAIRS rows for {theme:?}"
            );
        }
    }

    /// The heart of the crate: every sanctioned pair, in every theme it is
    /// declared for, must meet its minimum ratio. On failure this prints
    /// token names, hexes, computed ratio and required ratio for every
    /// failing row before asserting — never just the first failure.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn all_sanctioned_pairs_meet_contrast_law() {
        let mut failures = Vec::new();
        for pair in SANCTIONED_PAIRS {
            let tokens = pair.theme.tokens();
            let fg = tokens
                .resolve(pair.fg)
                .unwrap_or_else(|| panic!("SANCTIONED_PAIRS: unknown fg token id {:?}", pair.fg));
            let bg = tokens
                .resolve(pair.bg)
                .unwrap_or_else(|| panic!("SANCTIONED_PAIRS: unknown bg token id {:?}", pair.bg));
            let ratio = contrast_ratio(fg, bg);
            if ratio < pair.min_ratio {
                failures.push(format!(
                    "{:?}: fg {} ({}) on bg {} ({}) = {ratio:.3}:1, need >= {:.2}:1",
                    pair.theme, pair.fg, fg, pair.bg, bg, pair.min_ratio
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "contrast-law failures (STRAND_DESIGN_LANGUAGE.md §2.1):\n{}",
            failures.join("\n")
        );
    }
}
