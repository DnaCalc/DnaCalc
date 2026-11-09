//! The device advances Excel's grid uses for Western text. Excel places these glyphs with
//! GDI's hinted, whole-pixel advances. A browser reports the outline's fractional advance,
//! which is not the same number after rounding: Calibri at 15 ppem advances `l` by 4 where
//! its outline gives 3.44. `gdi-advances.bin` holds GDI's own answers for printable ASCII,
//! written by `tools/xtask-harness/src/paint/gdi-advances.ps1` from the installed fonts.
const TABLE: &[u8] = include_bytes!("gdi-advances.bin");
const FAMILIES: [&str; 5] = ["Calibri", "Arial", "Cambria", "Consolas", "Times New Roman"];
const STYLES: usize = 4;
const PPEMS: std::ops::RangeInclusive<usize> = 6..=48;
const GLYPHS: usize = 95;

/// GDI's advance of one printable ASCII glyph, or `None` where the table holds no answer.
pub(super) fn advance(
    family: Option<&str>,
    bold: bool,
    italic: bool,
    pixels: f64,
    glyph: &str,
) -> Option<f64> {
    let [byte] = glyph.as_bytes() else {
        return None;
    };
    let code = usize::from(*byte)
        .checked_sub(32)
        .filter(|code| *code < GLYPHS)?;
    let family = FAMILIES
        .iter()
        .position(|name| family.is_some_and(|f| f.eq_ignore_ascii_case(name)))?;
    let ppem = pixels.round() as usize;
    if !PPEMS.contains(&ppem) {
        return None;
    }
    let style = usize::from(bold) + 2 * usize::from(italic);
    let row = (family * STYLES + style) * PPEMS.count() + (ppem - PPEMS.start());
    TABLE
        .get(row * GLYPHS + code)
        .map(|advance| f64::from(*advance))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// `Align Ag` at Calibri 11 and 100% measures 51 pixels in Excel's own raster.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn calibri_keeps_the_hinted_advances_excel_paints() {
        let line = |text: &str| -> f64 {
            text.chars()
                .map(|c| advance(Some("Calibri"), false, false, 44. / 3., &c.to_string()).unwrap())
                .sum()
        };
        assert_eq!(line("Align Ag"), 51.);
        assert_eq!(line("Ag 0123"), 47.);
        assert_eq!(
            advance(Some("Calibri"), false, false, 44. / 3., "l"),
            Some(4.)
        );
        assert_eq!(advance(Some("Wingdings"), false, false, 15., "l"), None);
        assert_eq!(advance(Some("Calibri"), false, false, 15., "é"), None);
    }
}
