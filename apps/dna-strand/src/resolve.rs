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
}

pub fn resolve<'a>(
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
    for pair in SANCTIONED_PAIRS.iter().filter(|pair| pair.theme == theme) {
        let ratio = contrast_ratio(
            resolved.colour(pair.fg).expect("sanctioned foreground"),
            resolved.colour(pair.bg).expect("sanctioned background"),
        );
        if ratio < pair.min_ratio {
            return Err(ThemeError::Contrast {
                foreground: pair.fg,
                background: pair.bg,
                ratio,
                minimum: pair.min_ratio,
            });
        }
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
