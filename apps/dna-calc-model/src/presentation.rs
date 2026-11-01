//! Layout inputs belong to presentation. Updating them neither edits a workbook nor issues a
//! calculation request. A plan carries the stamp so consumers can invalidate measured text.
use crate::geometry::{GridMetrics, Viewport};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PresentationStamp {
    pub epoch: u64,
    pub fonts: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Presentation {
    pub stamp: PresentationStamp,
    pub metrics: GridMetrics,
    pub viewport: Viewport,
}
impl Presentation {
    /// Equal inputs preserve the stamp, independently of how often a reactive reader runs.
    pub fn update(
        previous: Option<&Self>,
        metrics: GridMetrics,
        viewport: Viewport,
        fonts: u64,
    ) -> Self {
        let changed = previous.is_none_or(|p| {
            p.metrics != metrics || p.viewport != viewport || p.stamp.fonts != fonts
        });
        let epoch = previous.map_or(0, |p| p.stamp.epoch);
        Self {
            stamp: PresentationStamp {
                epoch: if changed {
                    epoch
                        .checked_add(1)
                        .expect("Presentation identities exhausted")
                } else {
                    epoch
                },
                fonts,
            },
            metrics,
            viewport,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn resize_zoom_and_font_completion_invalidate_only_presentation() {
        let metrics = GridMetrics::default();
        let view = Viewport {
            width: 800.,
            height: 500.,
            ..Default::default()
        };
        let original = Presentation::update(None, metrics.clone(), view, 0);
        assert_eq!(
            Presentation::update(Some(&original), metrics.clone(), view, 0),
            original
        );
        let resized = Presentation::update(
            Some(&original),
            metrics.clone(),
            Viewport {
                width: 600.,
                ..view
            },
            0,
        );
        assert_eq!(resized.stamp.epoch, original.stamp.epoch + 1);
        let zoomed = Presentation::update(Some(&resized), metrics.scaled(2.), resized.viewport, 0);
        assert_eq!(zoomed.stamp.epoch, resized.stamp.epoch + 1);
        let fonts = Presentation::update(Some(&zoomed), zoomed.metrics.clone(), zoomed.viewport, 1);
        assert_eq!(fonts.stamp.epoch, zoomed.stamp.epoch + 1);
        assert_eq!(fonts.stamp.fonts, 1);
        assert_eq!(
            Presentation::update(Some(&fonts), fonts.metrics.clone(), fonts.viewport, 1),
            fonts
        );
    }
}
