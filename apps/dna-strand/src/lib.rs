//! Strand: DNA Calc's design tokens (dc-xib), ported from DnaTreeCalc's `dnacalc-strand` crate
//! (DnaTreeCalc bf6dfd7, `src/dnacalc-strand/src`), whose design authority is DnaTreeCalc's
//! `docs/ux/redesign/STRAND_DESIGN_LANGUAGE.md` (summarised in this repository's
//! `docs/plan/STRAND.md`).
//!
//! Framework-free and dependency-free: the palette ([`palette`]), the contrast law and its tests
//! ([`contrast`]: every sanctioned foreground/background pair is checked in the tests, so a
//! palette change that breaks a pair fails the build), block geometry and motion ([`geometry`]),
//! the three themes and two densities ([`theme`]), the `--dna-*` custom properties ([`css`]) and
//! the stylesheet both apps load ([`css::stylesheet`]), and the identity badges ([`identity`]).
//!
//! Nothing here is final: the hex values are the current best answer, held to the contrast law,
//! not frozen constants.

pub mod contrast;
pub mod css;
pub mod geometry;
pub mod identity;
pub mod palette;
pub mod theme;

pub use contrast::{SANCTIONED_PAIRS, SanctionedPair, contrast_ratio, relative_luminance};
pub use css::{css_custom_properties, stylesheet};
pub use geometry::{
    GAP_SCALE, MOTION_DURATION_MAX_MS, MOTION_DURATION_MIN_MS, RADIUS_CARD, RADIUS_CHIP,
    RADIUS_PANEL, REDUCED_MOTION_FLAG,
};
pub use identity::{
    ALL_IDENTITIES, EXCELSTRICT_IDENTITY, IdentityBadge, IdentityBlock, ONECALC_IDENTITY,
    RICHTREE_IDENTITY,
};
pub use palette::Rgb;
pub use theme::{COCKPIT_DARK, COCKPIT_LIGHT, Density, HIGH_CONTRAST, Theme, ThemeTokens};
