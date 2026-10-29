//! Theme tokens — STRAND_DESIGN_LANGUAGE.md §6 (Theming) and SHELL_SPEC.md
//! §8 (Theming & density).
//!
//! Three themes: `cockpit-light` (default: dark petrol chrome around light
//! paper), `cockpit-dark` (petrol-black chrome, charcoal paper) and
//! `high-contrast`. Themes are the *only* per-user visual variance. Two
//! densities: `working` (default, hands-density) and `reading`
//! (Notebook/published, looser leading, wider measure) — density affects
//! spacing/type-scale tokens, never colors, so [`ThemeTokens`] itself has no
//! density axis.
//!
//! Chrome (Mast/Bridge/Strip) stays dark petrol regardless of theme — STRAND
//! tenet 1, "Cockpit chrome, daylight work" — which is why `chrome_ink` is
//! identical across `cockpit-light` and `cockpit-dark` below (the chrome
//! ground barely changes; only the paper the model sits on does).

use crate::palette::{AMBER, GREEN_500, RED, Rgb, SIGNAL, TEAL_400, TEAL_600, hex_to_rgb};

/// One of the three Strand themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Theme {
    /// Default: dark petrol chrome around light paper.
    CockpitLight,
    /// Petrol-black chrome, charcoal paper.
    CockpitDark,
    /// Stricter derived theme: every text ink reaches >= 7.0:1 on its
    /// ground. See the `HIGH_CONTRAST` const doc comment for how each ink
    /// was derived.
    HighContrast,
}

impl Theme {
    /// All themes, for iteration (used by the contrast-law test and by
    /// anything that needs to render a theme picker).
    pub const ALL: [Theme; 3] = [Theme::CockpitLight, Theme::CockpitDark, Theme::HighContrast];

    /// The `--dna-theme` / `data-dna-theme` wire id.
    pub const fn id(self) -> &'static str {
        match self {
            Theme::CockpitLight => "cockpit-light",
            Theme::CockpitDark => "cockpit-dark",
            Theme::HighContrast => "high-contrast",
        }
    }

    /// This theme's resolved color tokens.
    pub const fn tokens(self) -> ThemeTokens {
        match self {
            Theme::CockpitLight => COCKPIT_LIGHT,
            Theme::CockpitDark => COCKPIT_DARK,
            Theme::HighContrast => HIGH_CONTRAST,
        }
    }
}

/// Working (hands-density, default) vs Reading (Notebook/published: looser
/// leading, wider measure). STRAND §5. No third mode. Density never touches
/// color — only spacing/type-scale-adjacent tokens in
/// [`crate::css::css_custom_properties`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Density {
    Working,
    Reading,
}

impl Density {
    pub const ALL: [Density; 2] = [Density::Working, Density::Reading];

    /// The `--dna-density` / `data-dna-density` wire id.
    pub const fn id(self) -> &'static str {
        match self {
            Density::Working => "working",
            Density::Reading => "reading",
        }
    }

    /// Body line-height. STRAND §5: Reading gets looser leading.
    pub const fn line_height(self) -> f64 {
        match self {
            Density::Working => 1.35,
            Density::Reading => 1.6,
        }
    }

    /// Max content measure. STRAND §5: Reading gets a wider measure (~68ch,
    /// matching the Notebook prose measure in STRAND §3). `None` in Working
    /// density, where chrome/grids are not measure-constrained.
    pub const fn measure(self) -> Option<&'static str> {
        match self {
            Density::Working => None,
            Density::Reading => Some("68ch"),
        }
    }
}

/// A theme's full set of resolved color tokens.
///
/// Field groups follow STRAND §2/§2.1: surfaces, neutral inks, lines,
/// chrome, accent + soft tints, and the dedicated `-ink` variants that must
/// be used wherever a hue becomes text (STRAND §2.1's contrast law; see
/// `crate::contrast`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeTokens {
    // --- surfaces ---
    pub bg: Rgb,
    pub paper: Rgb,
    pub paper_2: Rgb,

    // --- neutral inks ---
    pub ink: Rgb,
    pub ink_2: Rgb,
    pub ink_3: Rgb,

    // --- lines ---
    pub line: Rgb,
    pub line_soft: Rgb,

    // --- chrome ---
    pub chrome: Rgb,
    pub chrome_2: Rgb,
    pub chrome_ink: Rgb,
    pub chrome_ink_2: Rgb,

    // --- accent + soft tints ---
    pub accent: Rgb,
    pub accent_soft: Rgb,
    pub amber_soft: Rgb,
    pub green_soft: Rgb,
    pub signal_soft: Rgb,
    pub red_soft: Rgb,

    // --- ink variants (STRAND §2.1) ---
    pub accent_ink: Rgb,
    pub value_ink: Rgb,
    pub prov_const: Rgb,
    pub prov_ext: Rgb,
    /// Text-safe red. STRAND §2.1 names `teal-ink`/`green-ink`/`ink-3`/
    /// `prov-const`/`prov-ext` as "established variants" (plural,
    /// non-exhaustive) of the same law: "wherever a hue becomes text or a
    /// small glyph, it must use a per-theme `-ink` variant... never the raw
    /// surface tone." Red-as-error-text needs the same treatment — and the
    /// raw brand `RED` fails the law outright on dark paper (2.98:1, see
    /// below) — so this crate adds `red_ink` as a fifth variant, symmetrical
    /// with the other four.
    pub red_ink: Rgb,
}

impl ThemeTokens {
    /// Resolve a Strand token id — the vocabulary used by
    /// [`crate::contrast::SANCTIONED_PAIRS`] and by
    /// [`crate::css::css_custom_properties`] — to this theme's color.
    ///
    /// `"amber"` is the one id that does not name a `ThemeTokens` field: it
    /// is the single global, non-themed signal color
    /// ([`crate::palette::AMBER`]), used directly (STRAND §2: amber is the
    /// only saturated attention channel, unchanged by theme).
    pub fn resolve(&self, token: &str) -> Option<Rgb> {
        Some(match token {
            "bg" => self.bg,
            "paper" => self.paper,
            "paper-2" => self.paper_2,
            "ink" => self.ink,
            "ink-2" => self.ink_2,
            "ink-3" => self.ink_3,
            "line" => self.line,
            "line-soft" => self.line_soft,
            "chrome" => self.chrome,
            "chrome-2" => self.chrome_2,
            "chrome-ink" => self.chrome_ink,
            "chrome-ink-2" => self.chrome_ink_2,
            "accent" => self.accent,
            "accent-soft" => self.accent_soft,
            "amber-soft" => self.amber_soft,
            "green-soft" => self.green_soft,
            "signal-soft" => self.signal_soft,
            "red-soft" => self.red_soft,
            "accent-ink" => self.accent_ink,
            "value-ink" => self.value_ink,
            "prov-const" => self.prov_const,
            "prov-ext" => self.prov_ext,
            "red" | "red-ink" => self.red_ink,
            "amber" => AMBER,
            _ => return None,
        })
    }
}

/// Blend `hue` into `base` at `hue_pct` percent (0..=100), per channel,
/// rounded to the nearest integer. Used only to derive the `*_soft` tint
/// fields below — plain integer math so the whole token table stays a
/// compile-time constant (no floating-point `const fn` dependency).
const fn mix_channel(base: u8, hue: u8, hue_pct: u32) -> u8 {
    let base = base as u32;
    let hue = hue as u32;
    ((base * (100 - hue_pct) + hue * hue_pct + 50) / 100) as u8
}

const fn mix(base: Rgb, hue: Rgb, hue_pct: u32) -> Rgb {
    Rgb {
        r: mix_channel(base.r, hue.r, hue_pct),
        g: mix_channel(base.g, hue.g, hue_pct),
        b: mix_channel(base.b, hue.b, hue_pct),
    }
}

/// Soft-tint mix strength (percent of brand hue blended into `paper`) for
/// light-ground themes (`cockpit-light`, `high-contrast`). STRAND does not
/// specify soft-tint hex values; this crate derives them as a paper/hue
/// blend so they read as pastel badge fills. They are never sanctioned as
/// text — any text drawn over a soft tint uses the matching `-ink` variant
/// (STRAND §2.1) or the neutral inks, both of which are checked against
/// `paper`/`paper-2` directly in `crate::contrast`, so the exact soft-tint
/// shade cannot affect the contrast law.
const SOFT_TINT_LIGHT_PCT: u32 = 14;

/// Soft-tint mix strength for the dark-ground theme (`cockpit-dark`). Higher
/// than the light mix: blending into a dark paper needs more hue percentage
/// to stay visibly tinted rather than reading as plain paper.
const SOFT_TINT_DARK_PCT: u32 = 24;

const LIGHT_PAPER: Rgb = hex_to_rgb("#FFFFFF");
const DARK_PAPER: Rgb = hex_to_rgb("#1A262B");
const HC_PAPER: Rgb = hex_to_rgb("#FFFFFF");

/// `cockpit-light` — STRAND §2.1 ink-variant table (light column) plus the
/// vision-board surface values.
pub const COCKPIT_LIGHT: ThemeTokens = ThemeTokens {
    bg: hex_to_rgb("#F4F7F8"),
    paper: LIGHT_PAPER,
    paper_2: hex_to_rgb("#EEF2F4"),
    ink: hex_to_rgb("#17282E"),
    ink_2: hex_to_rgb("#48606A"),
    // Spec value #5F7680 only reaches 4.25:1 on paper-2 (< 4.5 required;
    // 4.78:1 on paper alone was fine). Darkened minimally, hue and
    // saturation held (198.2° / 14.8% -> 198.8° / 15.0%), to #5B717B:
    // 5.13:1 on paper, 4.56:1 on paper-2.
    // adjusted from spec #5F7680 for AA: see STRAND 2.1
    ink_3: hex_to_rgb("#5B717B"),
    line: hex_to_rgb("#D5DFE3"),
    line_soft: hex_to_rgb("#E4EBEE"),
    chrome: hex_to_rgb("#193F49"),
    chrome_2: hex_to_rgb("#12303A"),
    chrome_ink: hex_to_rgb("#DFEEF2"),
    chrome_ink_2: hex_to_rgb("#8FB3BC"),
    accent: TEAL_600,
    accent_soft: mix(LIGHT_PAPER, TEAL_600, SOFT_TINT_LIGHT_PCT),
    amber_soft: mix(LIGHT_PAPER, AMBER, SOFT_TINT_LIGHT_PCT),
    green_soft: mix(LIGHT_PAPER, GREEN_500, SOFT_TINT_LIGHT_PCT),
    signal_soft: mix(LIGHT_PAPER, SIGNAL, SOFT_TINT_LIGHT_PCT),
    red_soft: mix(LIGHT_PAPER, RED, SOFT_TINT_LIGHT_PCT),
    accent_ink: hex_to_rgb("#1B5F6B"),
    value_ink: hex_to_rgb("#2E6E5B"),
    prov_const: hex_to_rgb("#2D6A8A"),
    prov_ext: hex_to_rgb("#A35400"),
    // Raw RED already reaches 5.20:1 on paper — no adjustment needed here
    // (contrast cockpit-dark below, where it fails outright).
    red_ink: RED,
};

/// `cockpit-dark` — STRAND §2.1 ink-variant table (dark column) plus the
/// vision-board surface values.
pub const COCKPIT_DARK: ThemeTokens = ThemeTokens {
    bg: hex_to_rgb("#0E1A1F"),
    paper: DARK_PAPER,
    paper_2: hex_to_rgb("#152126"),
    ink: hex_to_rgb("#E2ECEF"),
    ink_2: hex_to_rgb("#9FB4BB"),
    ink_3: hex_to_rgb("#7D97A0"),
    line: hex_to_rgb("#2D3F46"),
    line_soft: hex_to_rgb("#24343B"),
    chrome: hex_to_rgb("#0F2229"),
    chrome_2: hex_to_rgb("#0C1C22"),
    chrome_ink: hex_to_rgb("#DFEEF2"),
    chrome_ink_2: hex_to_rgb("#7FA2AC"),
    accent: TEAL_400,
    accent_soft: mix(DARK_PAPER, TEAL_400, SOFT_TINT_DARK_PCT),
    amber_soft: mix(DARK_PAPER, AMBER, SOFT_TINT_DARK_PCT),
    green_soft: mix(DARK_PAPER, GREEN_500, SOFT_TINT_DARK_PCT),
    signal_soft: mix(DARK_PAPER, SIGNAL, SOFT_TINT_DARK_PCT),
    red_soft: mix(DARK_PAPER, RED, SOFT_TINT_DARK_PCT),
    accent_ink: hex_to_rgb("#4FC0D8"),
    value_ink: hex_to_rgb("#74C7AB"),
    prov_const: hex_to_rgb("#7DB8D8"),
    prov_ext: hex_to_rgb("#F0A35C"),
    // Raw RED (#D02A23) reaches only 2.98:1 on dark paper #1A262B (< 4.5
    // required). Lightened minimally, hue and saturation held (2.4° / 71%),
    // to #E4625C: 4.57:1 on paper, 4.85:1 on paper-2.
    // adjusted from spec #D02A23 for AA: see STRAND 2.1
    red_ink: hex_to_rgb("#E4625C"),
};

/// `high-contrast` — not directly specified by STRAND (which leaves it to
/// be "derived": "every text ink reaches >= 7.0:1 on its grounds"). Built
/// from `cockpit-light`'s structure (white paper, dark chrome — chrome
/// stays dark in every theme per STRAND tenet 1) with every ink darkened —
/// hue and saturation held, lightness reduced — until it clears 7.0:1 on
/// *both* `paper` and `paper_2`. `chrome_ink`/`chrome_ink_2` already clear
/// 7.0:1 unchanged once chrome itself is pushed darker; `amber` keeps its
/// large-glyph minimum of 3.0:1 (STRAND's large-text/graphical class, not
/// body text, so high-contrast's 7.0:1 text bar does not apply to it).
pub const HIGH_CONTRAST: ThemeTokens = ThemeTokens {
    bg: hex_to_rgb("#F2F5F6"),
    paper: HC_PAPER,
    paper_2: hex_to_rgb("#E7ECEE"),
    // Already 15.2:1 / 12.8:1 on paper/paper-2 unchanged from cockpit-light.
    ink: hex_to_rgb("#17282E"),
    // Darkened from cockpit-light's #48606A (6.65:1 on paper, < 7.0) to
    // #3C5059: 8.46:1 on paper, 7.10:1 on paper-2.
    ink_2: hex_to_rgb("#3C5059"),
    // Darkened from the STRAND-spec value #5F7680 (same starting point as
    // cockpit-light's ink_3 above, before *its* AA adjustment; 4.78:1 on
    // paper, well under 7.0) to #405056: 8.40:1 on paper, 7.05:1 on paper-2.
    ink_3: hex_to_rgb("#405056"),
    line: hex_to_rgb("#B9C4C8"),
    line_soft: hex_to_rgb("#D6DEE1"),
    // Darker than petrol-950, for extra headroom under the chrome-ink 7:1
    // bar below.
    chrome: hex_to_rgb("#0A171C"),
    chrome_2: hex_to_rgb("#050D10"),
    // Unchanged from cockpit-light: already 15.3:1 / 16.5:1 against the
    // darker high-contrast chrome tones above.
    chrome_ink: hex_to_rgb("#DFEEF2"),
    // Unchanged from cockpit-light: already 8.1:1 / 8.7:1 against the
    // darker high-contrast chrome tones above (chrome-ink-2 only needed
    // >= 4.0:1 in the base themes; high-contrast raises that bar to 7.0:1,
    // which this shade already clears without adjustment).
    chrome_ink_2: hex_to_rgb("#8FB3BC"),
    accent: TEAL_600,
    accent_soft: mix(HC_PAPER, TEAL_600, SOFT_TINT_LIGHT_PCT),
    amber_soft: mix(HC_PAPER, AMBER, SOFT_TINT_LIGHT_PCT),
    green_soft: mix(HC_PAPER, GREEN_500, SOFT_TINT_LIGHT_PCT),
    signal_soft: mix(HC_PAPER, SIGNAL, SOFT_TINT_LIGHT_PCT),
    red_soft: mix(HC_PAPER, RED, SOFT_TINT_LIGHT_PCT),
    // Darkened from cockpit-light's #1B5F6B (7.25:1 on paper but only
    // 6.44:1 on paper-2, < 7.0) to #18545F: 8.49:1 / 7.13:1.
    accent_ink: hex_to_rgb("#18545F"),
    // Darkened from cockpit-light's #2E6E5B (6.00:1 on paper, < 7.0) to
    // #245647: 8.41:1 / 7.06:1.
    value_ink: hex_to_rgb("#245647"),
    // Darkened from cockpit-light's #2D6A8A (5.93:1 on paper, < 7.0) to
    // #23526B: 8.43:1 / 7.08:1.
    prov_const: hex_to_rgb("#23526B"),
    // Darkened from cockpit-light's #A35400 (5.49:1 on paper, < 7.0) to
    // #783E00: 8.44:1 / 7.09:1.
    prov_ext: hex_to_rgb("#783E00"),
    // Darkened from the raw brand RED #D02A23 (5.20:1 on paper, < 7.0) to
    // #961E19: 8.41:1 / 7.06:1.
    red_ink: hex_to_rgb("#961E19"),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn every_theme_id_is_distinct() {
        let ids: std::collections::HashSet<_> = Theme::ALL.iter().map(|t| t.id()).collect();
        assert_eq!(ids.len(), Theme::ALL.len());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn resolve_covers_every_themetokens_field_name() {
        let t = COCKPIT_LIGHT;
        for token in [
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
            "amber-soft",
            "green-soft",
            "signal-soft",
            "red-soft",
            "accent-ink",
            "value-ink",
            "prov-const",
            "prov-ext",
            "red",
            "amber",
        ] {
            assert!(
                t.resolve(token).is_some(),
                "resolve({token:?}) returned None"
            );
        }
        assert_eq!(t.resolve("nonexistent-token"), None);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn resolve_red_is_the_text_safe_ink_not_the_raw_surface_tone() {
        // On cockpit-dark, raw brand RED fails the contrast law on paper
        // (2.98:1) — resolve("red") must return the adjusted red_ink, never
        // crate::palette::RED directly.
        let dark = COCKPIT_DARK;
        assert_eq!(dark.resolve("red"), Some(dark.red_ink));
        assert_ne!(dark.resolve("red"), Some(RED));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn high_contrast_inks_are_darker_than_cockpit_light() {
        // Spot-check the derivation direction: high-contrast darkens
        // cockpit-light's paper-going inks (lower relative luminance).
        use crate::contrast::relative_luminance;
        for (light, hc) in [
            (COCKPIT_LIGHT.ink_2, HIGH_CONTRAST.ink_2),
            (COCKPIT_LIGHT.ink_3, HIGH_CONTRAST.ink_3),
            (COCKPIT_LIGHT.accent_ink, HIGH_CONTRAST.accent_ink),
            (COCKPIT_LIGHT.value_ink, HIGH_CONTRAST.value_ink),
            (COCKPIT_LIGHT.prov_const, HIGH_CONTRAST.prov_const),
            (COCKPIT_LIGHT.prov_ext, HIGH_CONTRAST.prov_ext),
        ] {
            assert!(
                relative_luminance(hc) < relative_luminance(light),
                "expected high-contrast ink {hc:?} to be darker than cockpit-light {light:?}"
            );
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn density_ids_are_distinct_and_reading_is_looser() {
        assert_ne!(Density::Working.id(), Density::Reading.id());
        assert!(Density::Working.line_height() < Density::Reading.line_height());
        assert_eq!(Density::Working.measure(), None);
        assert_eq!(Density::Reading.measure(), Some("68ch"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn mix_at_zero_percent_is_base_and_at_hundred_is_hue() {
        let base = hex_to_rgb("#102030");
        let hue = hex_to_rgb("#A0B0C0");
        assert_eq!(mix(base, hue, 0), base);
        assert_eq!(mix(base, hue, 100), hue);
    }
}
