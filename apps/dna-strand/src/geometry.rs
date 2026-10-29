//! Block geometry and motion constants — STRAND_DESIGN_LANGUAGE.md §4
//! (Block geometry) and §5 (Motion & density).
//!
//! "Everything is a block" (STRAND tenet 2): formula tokens, name chips,
//! node cards and map districts are the same component family at different
//! sizes, sharing one radius scale and one gap scale.

/// Corner radius, in CSS pixels, for chip/token-sized blocks (formula
/// tokens, name chips). STRAND §4.
pub const RADIUS_CHIP: u32 = 5;

/// Corner radius, in CSS pixels, for card-sized blocks (node cards, map
/// districts). STRAND §4.
pub const RADIUS_CARD: u32 = 9;

/// Corner radius, in CSS pixels, for panel-sized blocks (panels, stage
/// corners). STRAND §4.
pub const RADIUS_PANEL: u32 = 14;

/// The one gap scale used throughout Strand layouts, in CSS pixels.
/// STRAND §4.
pub const GAP_SCALE: [u32; 6] = [2, 4, 6, 10, 14, 22];

/// Minimum sanctioned motion duration, in milliseconds. STRAND §5.
pub const MOTION_DURATION_MIN_MS: u32 = 120;

/// Maximum sanctioned motion duration, in milliseconds. STRAND §5.
pub const MOTION_DURATION_MAX_MS: u32 = 160;

/// Convention name for the reduced-motion flag consuming shells set (e.g. as
/// a `data-*` attribute or class) once `prefers-reduced-motion: reduce` is
/// detected, so component code across skins branches on one name instead of
/// re-querying the media feature everywhere. Strand does not itself read
/// `prefers-reduced-motion` — it only names the convention. STRAND §5.
pub const REDUCED_MOTION_FLAG: &str = "dna-reduced-motion";

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn radius_scale_tracks_block_size() {
        const { assert!(RADIUS_CHIP < RADIUS_CARD) };
        const { assert!(RADIUS_CARD < RADIUS_PANEL) };
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn gap_scale_is_strictly_increasing() {
        assert!(GAP_SCALE.windows(2).all(|w| w[0] < w[1]));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn motion_bounds_are_ordered() {
        const { assert!(MOTION_DURATION_MIN_MS < MOTION_DURATION_MAX_MS) };
    }
}
