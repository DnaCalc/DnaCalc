//! Resolves a skin's colour overrides and checks every sanctioned contrast pair.

use crate::{Rgb, SANCTIONED_PAIRS, Theme, contrast_ratio};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum ThemeError {
    UnknownToken(String),
    FixedToken(String),
    InvalidColour {
        token: String,
        value: String,
    },
    Contrast {
        foreground: &'static str,
        background: &'static str,
        ratio: f64,
        minimum: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContrastReading {
    pub theme: Theme,
    pub foreground: &'static str,
    pub background: &'static str,
    pub ratio: f64,
    pub minimum: f64,
}

impl ContrastReading {
    pub fn passes(self) -> bool {
        self.ratio >= self.minimum
    }
}

pub struct ResolvedTheme {
    theme: Theme,
    colours: BTreeMap<String, Rgb>,
}

impl ResolvedTheme {
    pub fn colour(&self, token: &str) -> Option<Rgb> {
        let key = if token == "red" { "red-ink" } else { token };
        self.colours
            .get(key)
            .copied()
            .or_else(|| self.theme.tokens().resolve(key))
    }

    /// Check the selected palette and the default palettes of the other themes.
    pub fn contrast_readings(&self) -> impl Iterator<Item = ContrastReading> + '_ {
        SANCTIONED_PAIRS.iter().map(|pair| {
            let colour = |token| {
                if pair.theme == self.theme {
                    self.colour(token)
                } else {
                    let key = if token == "red" { "red-ink" } else { token };
                    pair.theme.tokens().resolve(key)
                }
                .expect("sanctioned Strand token")
            };
            ContrastReading {
                theme: pair.theme,
                foreground: pair.fg,
                background: pair.bg,
                ratio: contrast_ratio(colour(pair.fg), colour(pair.bg)),
                minimum: pair.min_ratio,
            }
        })
    }

    pub fn check_contrast(&self) -> Result<(), ThemeError> {
        if let Some(reading) = self.contrast_readings().find(|reading| !reading.passes()) {
            return Err(ThemeError::Contrast {
                foreground: reading.foreground,
                background: reading.background,
                ratio: reading.ratio,
                minimum: reading.minimum,
            });
        }
        Ok(())
    }
}

pub fn resolve<'a>(
    theme: Theme,
    overrides: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<ResolvedTheme, ThemeError> {
    let resolved = resolve_draft(theme, overrides)?;
    resolved.check_contrast()?;
    Ok(resolved)
}

/// Resolve a draft for inspection, including pairs that fail contrast.
pub fn resolve_draft<'a>(
    theme: Theme,
    overrides: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<ResolvedTheme, ThemeError> {
    let mut resolved = ResolvedTheme {
        theme,
        colours: BTreeMap::new(),
    };
    for (token, value) in overrides {
        if token == "amber" {
            return Err(ThemeError::FixedToken(token.into()));
        }
        if token == "red" || theme.tokens().resolve(token).is_none() {
            return Err(ThemeError::UnknownToken(token.into()));
        }
        let hex = value.strip_prefix('#').unwrap_or(value);
        let parsed = (hex.len() == 6 && hex.bytes().all(|ch| ch.is_ascii_hexdigit()))
            .then(|| u32::from_str_radix(hex, 16).expect("six ASCII hexadecimal digits"));
        let Some(rgb) = parsed else {
            return Err(ThemeError::InvalidColour {
                token: token.into(),
                value: value.into(),
            });
        };
        resolved.colours.insert(
            token.into(),
            Rgb {
                r: (rgb >> 16) as u8,
                g: (rgb >> 8) as u8,
                b: rgb as u8,
            },
        );
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn resolves_theme_overrides_only_when_the_pairs_retain_contrast() {
        for theme in Theme::ALL {
            assert!(resolve(theme, []).is_ok(), "{theme:?}");
        }
        let theme = Theme::CockpitLight;
        assert!(matches!(
            resolve(theme, [("ink", "#FFFFFF")]),
            Err(ThemeError::Contrast {
                foreground: "ink",
                ..
            })
        ));
        assert!(matches!(
            resolve(theme, [("amber", "#000000")]),
            Err(ThemeError::FixedToken(_))
        ));
        assert!(matches!(
            resolve(theme, [("paper", "rgb(0,0,0)")]),
            Err(ThemeError::InvalidColour { .. })
        ));
        let allowed = resolve(theme, [("ink", "#102020")]).unwrap();
        assert_eq!(allowed.colour("ink").unwrap().to_hex(), "#102020");
        assert_eq!(allowed.colour("paper"), Some(theme.tokens().paper));
    }
}
