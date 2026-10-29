//! Identity badge data — STRAND_DESIGN_LANGUAGE.md §2 ("Identity is never a
//! lone hue").
//!
//! Profiles are recognized by a two-block *base-pair badge* (the logo's own
//! device): a distinct block-length pattern plus a color pair, so
//! recognition survives small sizes and color-vision differences. Identity
//! badges appear only in chrome identity spots (profile badge, stage
//! underline) — never on data surfaces.

use crate::palette::{FOREST, PETROL_800, Rgb, SAGE, TEAL_400, TEAL_600};

/// One block of a two-block identity badge: a color and a size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityBlock {
    pub color: Rgb,
    pub size_px: u32,
}

/// A profile's identity badge: a named block-length pattern plus its two
/// blocks, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdentityBadge {
    /// Stable profile id (e.g. `"onecalc"`).
    pub profile_id: &'static str,
    /// Block-length pattern name (e.g. `"dot-long"`).
    pub pattern: &'static str,
    pub blocks: [IdentityBlock; 2],
}

/// OneCalc: cyan dot + petrol long (`dot-long`). STRAND §2.
pub const ONECALC_IDENTITY: IdentityBadge = IdentityBadge {
    profile_id: "onecalc",
    pattern: "dot-long",
    blocks: [
        IdentityBlock {
            color: TEAL_400,
            size_px: 10,
        },
        IdentityBlock {
            color: PETROL_800,
            size_px: 22,
        },
    ],
};

/// Rich Tree: sage long + forest dot (`long-dot`). STRAND §2.
pub const RICHTREE_IDENTITY: IdentityBadge = IdentityBadge {
    profile_id: "richtree",
    pattern: "long-dot",
    blocks: [
        IdentityBlock {
            color: SAGE,
            size_px: 22,
        },
        IdentityBlock {
            color: FOREST,
            size_px: 10,
        },
    ],
};

/// Excel-strict: teal + petrol equal pair (`twin`). STRAND §2. Only Tree vs
/// Excel-strict ever co-occur in one shell (OneCalc is its own window), so
/// the hard requirement is that *those two* are effortless to tell apart:
/// sage-light-green vs teal-dark-blue, plus opposite block patterns
/// (`long-dot` vs `twin`).
pub const EXCELSTRICT_IDENTITY: IdentityBadge = IdentityBadge {
    profile_id: "excelstrict",
    pattern: "twin",
    blocks: [
        IdentityBlock {
            color: TEAL_600,
            size_px: 15,
        },
        IdentityBlock {
            color: PETROL_800,
            size_px: 15,
        },
    ],
};

/// All three identity badges, for iteration (e.g. rendering a legend).
pub const ALL_IDENTITIES: [IdentityBadge; 3] =
    [ONECALC_IDENTITY, RICHTREE_IDENTITY, EXCELSTRICT_IDENTITY];

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn each_identity_has_a_distinct_pattern() {
        let patterns: std::collections::HashSet<_> =
            ALL_IDENTITIES.iter().map(|b| b.pattern).collect();
        assert_eq!(patterns.len(), ALL_IDENTITIES.len());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn tree_and_excelstrict_are_never_a_lone_hue_apart() {
        // The hard requirement (STRAND §2): Rich Tree vs Excel-strict must be
        // effortless to tell apart since they can co-occur in one shell —
        // both a different color pair AND a different block pattern.
        assert_ne!(RICHTREE_IDENTITY.pattern, EXCELSTRICT_IDENTITY.pattern);
        assert_ne!(
            RICHTREE_IDENTITY.blocks[0].color,
            EXCELSTRICT_IDENTITY.blocks[0].color
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn onecalc_is_its_own_window_but_still_a_distinct_pair() {
        assert_ne!(ONECALC_IDENTITY.pattern, EXCELSTRICT_IDENTITY.pattern);
        assert_ne!(ONECALC_IDENTITY.pattern, RICHTREE_IDENTITY.pattern);
    }
}
