//! The colour modifiers of a drawing colour (dc-zjsg.2): `lumMod`, `lumOff`, `lum`, `satMod`, `satOff`,
//! `sat`, `hueMod`, `hueOff`, `hue`, `tint`, `shade`, `alpha`, `alphaMod` and `alphaOff`, applied in file
//! order to a base `RRGGBB`.
//!
//! The luminance, saturation and hue modifiers work in HSL. `tint` and `shade` work on the linear-light
//! (sRGB-decoded) channels: `shade s` multiplies each by `s`, `tint t` moves each toward white,
//! `c' = 1 - (1 - c) * t`. Evidence: accent 1 (`4472C4`) with `shade 50%` comes out `2F528F`, the outline
//! Excel's own shape style draws, which only the linear form gives; `lumMod 60% lumOff 40%` on accent 1
//! gives `8FAADC`, Excel's "Accent 1, Lighter 40%". `tint` is by the same linear form and not yet
//! measured against Excel (assumed); modifiers not listed here (`comp`, `inv`, `gray`, `gamma`, ...) are
//! ignored (assumed).

/// A resolved colour: `0xRRGGBB` and its opacity 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resolved {
    pub rgb: u32,
    pub alpha: f64,
}

fn to_hsl(rgb: u32) -> (f64, f64, f64) {
    let (r, g, b) = (
        f64::from((rgb >> 16) & 255) / 255.0,
        f64::from((rgb >> 8) & 255) / 255.0,
        f64::from(rgb & 255) / 255.0,
    );
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    (h, s, l)
}

fn from_hsl(h: f64, s: f64, l: f64) -> u32 {
    let (s, l) = (s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h6 = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (h6.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h6 as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let byte = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u32;
    (byte(r) << 16) | (byte(g) << 8) | byte(b)
}

fn decode(byte: u32) -> f64 {
    let c = f64::from(byte) / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn encode(linear: f64) -> u32 {
    let c = linear.clamp(0.0, 1.0);
    let v = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    (v * 255.0).round().clamp(0.0, 255.0) as u32
}

fn linear(rgb: u32, f: impl Fn(f64) -> f64) -> u32 {
    let channel = |shift: u32| encode(f(decode((rgb >> shift) & 255)));
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

/// `rgb` after `modifiers` (name, `val`), in order. Consecutive HSL modifiers (`lumMod` then `lumOff`)
/// share one HSL value and are rounded to 8-bit channels once.
pub fn modify(rgb: u32, modifiers: &[(String, i64)]) -> Resolved {
    let (mut rgb, mut alpha) = (rgb, 1.0f64);
    let mut hsl: Option<(f64, f64, f64)> = None;
    for (name, value) in modifiers {
        let fraction = *value as f64 / 100_000.0;
        let hsl_modifier = matches!(
            name.as_str(),
            "lumMod" | "lumOff" | "lum" | "satMod" | "satOff" | "sat" | "hueMod" | "hueOff" | "hue"
        );
        if !hsl_modifier {
            if let Some((h, s, l)) = hsl.take() {
                rgb = from_hsl(h, s, l);
            }
        }
        match name.as_str() {
            "lumMod" | "lumOff" | "lum" | "satMod" | "satOff" | "sat" | "hueMod" | "hueOff" | "hue" => {
                let (mut h, mut s, mut l) = hsl.unwrap_or_else(|| to_hsl(rgb));
                match name.as_str() {
                    "lumMod" => l *= fraction,
                    "lumOff" => l += fraction,
                    "lum" => l = fraction,
                    "satMod" => s *= fraction,
                    "satOff" => s += fraction,
                    "sat" => s = fraction,
                    "hueMod" => h *= fraction,
                    "hueOff" => h += *value as f64 / 60_000.0,
                    _ => h = *value as f64 / 60_000.0,
                }
                hsl = Some((h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0)));
            }
            "tint" => rgb = linear(rgb, |c| 1.0 - (1.0 - c) * fraction),
            "shade" => rgb = linear(rgb, |c| c * fraction),
            "alpha" => alpha = fraction,
            "alphaMod" => alpha *= fraction,
            "alphaOff" => alpha += fraction,
            _ => {}
        }
    }
    if let Some((h, s, l)) = hsl {
        rgb = from_hsl(h, s, l);
    }
    Resolved { rgb, alpha: alpha.clamp(0.0, 1.0) }
}

/// The CSS of a resolved colour: `#RRGGBB`, or `rgba(...)` when it is not opaque.
pub fn css(colour: Resolved) -> String {
    if colour.alpha >= 1.0 {
        format!("#{:06X}", colour.rgb)
    } else {
        let alpha = (colour.alpha * 1000.0).round() / 1000.0;
        format!("rgba({},{},{},{})", (colour.rgb >> 16) & 255, (colour.rgb >> 8) & 255, colour.rgb & 255, alpha)
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    fn m(list: &[(&str, i64)]) -> Vec<(String, i64)> {
        list.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__shade_and_luminance_match_the_colours_excel_writes() {
        // Excel's own shape style: accent 1 shaded 50% is the 2F528F outline.
        assert_eq!(modify(0x4472C4, &m(&[("shade", 50_000)])).rgb, 0x2F528F);
        // Excel's "Accent 1, Lighter 40%" is lumMod 60% lumOff 40%.
        assert_eq!(modify(0x4472C4, &m(&[("lumMod", 60_000), ("lumOff", 40_000)])).rgb, 0x8FAADC);
        // "Accent 1, Darker 25%" is lumMod 75%.
        assert_eq!(modify(0x4472C4, &m(&[("lumMod", 75_000)])).rgb, 0x2F5597);
        // "Accent 2, Lighter 80%" is lumMod 20% lumOff 80%.
        assert_eq!(modify(0xED7D31, &m(&[("lumMod", 20_000), ("lumOff", 80_000)])).rgb, 0xFBE5D6);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn dc_zjsg_2__alpha_tint_and_saturation_modifiers() {
        let half = modify(0xFF0000, &m(&[("alpha", 50_000)]));
        assert_eq!((half.rgb, half.alpha), (0xFF0000, 0.5));
        assert_eq!(css(half), "rgba(255,0,0,0.5)");
        assert_eq!(css(modify(0xFF0000, &[])), "#FF0000");
        // tint 100% and shade 100% change nothing; tint 0% is white, shade 0% black.
        assert_eq!(modify(0x4472C4, &m(&[("tint", 100_000)])).rgb, 0x4472C4);
        assert_eq!(modify(0x4472C4, &m(&[("shade", 100_000)])).rgb, 0x4472C4);
        assert_eq!(modify(0x4472C4, &m(&[("tint", 0)])).rgb, 0xFFFFFF);
        assert_eq!(modify(0x4472C4, &m(&[("shade", 0)])).rgb, 0x000000);
        // Zero saturation is the grey of the same luminance; modifiers run in order.
        assert_eq!(modify(0xFF0000, &m(&[("satMod", 0)])).rgb, 0x808080);
        assert_eq!(modify(0x808080, &m(&[("alphaMod", 50_000), ("alphaMod", 50_000)])).alpha, 0.25);
        // A modifier this module does not know changes nothing.
        assert_eq!(modify(0x123456, &m(&[("comp", 0)])).rgb, 0x123456);
    }
}
