//! Brand color constants — STRAND_DESIGN_LANGUAGE.md §2 ("Palette — every
//! color has a job").
//!
//! Every color is written down once, as a hex string; the parsed [`Rgb`]
//! constant is derived from that string at compile time via [`hex_to_rgb`],
//! a `const fn`. The hex literal is therefore the single source of truth —
//! the hex string and the parsed value can never drift apart.
//!
//! These are **surface tones** (fills, borders, blocks). Per STRAND §2.1,
//! wherever one of these hues would become text or a small glyph, use the
//! matching `-ink` variant on [`crate::theme::ThemeTokens`] instead — never
//! a raw surface tone. See `crate::contrast` for the law that enforces this.

use std::fmt;

/// An 8-bit sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// Render as an uppercase `#RRGGBB` string.
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

const fn hex_digit(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => panic!("Rgb hex literal must contain only [0-9a-fA-F] digits"),
    }
}

const fn hex_byte(hi: u8, lo: u8) -> u8 {
    hex_digit(hi) * 16 + hex_digit(lo)
}

/// Parse a `"#RRGGBB"` (or bare `"RRGGBB"`) literal into an [`Rgb`] at
/// compile time. Malformed input is a compile error (a `const fn` panic).
pub const fn hex_to_rgb(hex: &str) -> Rgb {
    let bytes = hex.as_bytes();
    let offset = match bytes.len() {
        7 => 1, // "#RRGGBB"
        6 => 0, // "RRGGBB"
        _ => panic!("Rgb hex literal must be 6 or 7 characters (\"RRGGBB\" or \"#RRGGBB\")"),
    };
    Rgb {
        r: hex_byte(bytes[offset], bytes[offset + 1]),
        g: hex_byte(bytes[offset + 2], bytes[offset + 3]),
        b: hex_byte(bytes[offset + 4], bytes[offset + 5]),
    }
}

/// Chrome ground (Mast, Bridge, Strip), instrument panels. STRAND §2.
pub const PETROL_800_HEX: &str = "#193F49";
pub const PETROL_800: Rgb = hex_to_rgb(PETROL_800_HEX);

/// Chrome depth ramp. STRAND §2.
pub const PETROL_900_HEX: &str = "#12303A";
pub const PETROL_900: Rgb = hex_to_rgb(PETROL_900_HEX);

/// Chrome depth ramp, dark-theme chrome. STRAND §2.
pub const PETROL_950_HEX: &str = "#0C1C22";
pub const PETROL_950: Rgb = hex_to_rgb(PETROL_950_HEX);

/// Structure, references, interactive accents; Excel-strict identity color.
/// STRAND §2.
pub const TEAL_600_HEX: &str = "#318995";
pub const TEAL_600: Rgb = hex_to_rgb(TEAL_600_HEX);

/// Links, secondary structure; OneCalc identity color. STRAND §2.
pub const TEAL_400_HEX: &str = "#3DB6CF";
pub const TEAL_400: Rgb = hex_to_rgb(TEAL_400_HEX);

/// Focus. The only saturated attention channel: cursor, selection, running
/// calc, X-Ray target. STRAND §2.
pub const AMBER_HEX: &str = "#FFB301";
pub const AMBER: Rgb = hex_to_rgb(AMBER_HEX);

/// Fresh/verified values (not an identity color). STRAND §2.
pub const GREEN_500_HEX: &str = "#3F8B76";
pub const GREEN_500: Rgb = hex_to_rgb(GREEN_500_HEX);

/// Rich-Tree identity pair — long block. STRAND §2.
pub const SAGE_HEX: &str = "#71A08A";
pub const SAGE: Rgb = hex_to_rgb(SAGE_HEX);

/// Rich-Tree identity pair — dot block. STRAND §2.
pub const FOREST_HEX: &str = "#235949";
pub const FOREST: Rgb = hex_to_rgb(FOREST_HEX);

/// External & volatile only: RTD, links, add-in-fed cells. STRAND §2.
pub const SIGNAL_HEX: &str = "#FF8001";
pub const SIGNAL: Rgb = hex_to_rgb(SIGNAL_HEX);

/// Errors only. Never emphasis, never branding. STRAND §2.
pub const RED_HEX: &str = "#D02A23";
pub const RED: Rgb = hex_to_rgb(RED_HEX);

/// Dark-theme paper base, body ink. STRAND §2.
pub const CHARCOAL_HEX: &str = "#353938";
pub const CHARCOAL: Rgb = hex_to_rgb(CHARCOAL_HEX);

/// Light paper base, chrome ink. STRAND §2.
pub const MIST_HEX: &str = "#EDF1F2";
pub const MIST: Rgb = hex_to_rgb(MIST_HEX);

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn hex_to_rgb_parses_hash_prefixed_and_bare() {
        assert_eq!(hex_to_rgb("#FFB301"), hex_to_rgb("FFB301"));
        assert_eq!(
            hex_to_rgb("#FFB301"),
            Rgb {
                r: 0xFF,
                g: 0xB3,
                b: 0x01
            }
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn hex_to_rgb_is_case_insensitive() {
        assert_eq!(hex_to_rgb("#abcdef"), hex_to_rgb("#ABCDEF"));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn to_hex_roundtrips_through_hex_to_rgb() {
        for hex in [
            PETROL_800_HEX,
            PETROL_900_HEX,
            PETROL_950_HEX,
            TEAL_600_HEX,
            TEAL_400_HEX,
            AMBER_HEX,
            GREEN_500_HEX,
            SAGE_HEX,
            FOREST_HEX,
            SIGNAL_HEX,
            RED_HEX,
            CHARCOAL_HEX,
            MIST_HEX,
        ] {
            assert_eq!(hex_to_rgb(hex).to_hex(), hex, "round-trip failed for {hex}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn display_matches_to_hex() {
        assert_eq!(TEAL_600.to_string(), TEAL_600.to_hex());
    }
}
