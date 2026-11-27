//! Pointing in the workbook formula editor.

use calc_skin::editor::replace_text;

/// The shared formula parser's candidate slot, in UTF-16. The model decides whether the
/// current edit permits reference replacement, or continues an existing pointed range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointSyntax {
    Insert { start: usize, end: usize },
    ReplaceReference { start: usize, end: usize },
}

/// An accepted local edit for the browser adapter to reflect into its mounted text field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointEdit {
    pub text: String,
    pub caret: usize,
}

impl crate::Model {
    pub fn can_point(&self) -> bool {
        self.edit.surface.is_some()
            && self.edit.target.is_some()
            && !self.editor.composing
            && !self.editor.readonly
    }

    /// The picked reference still at the caret in the current draft.
    pub fn point_at_caret(&self, start: usize, end: usize) -> Option<crate::PointPick> {
        self.edit.pointed.filter(|pick| {
            self.edit.pointed_text == self.edit.draft && start == end && end == pick.end
        })
    }

    /// Points the live edit at a cell or range without changing its authored target or selection.
    /// Syntax discovery and DOM caret reads are inputs; draft, range and journal updates are one
    /// model mutation. This is the route the application's controller uses.
    pub fn point_at(
        &mut self,
        at: calc_value::CellKey,
        extend: bool,
        caret: crate::TextSelection,
        syntax: Option<PointSyntax>,
    ) -> Option<PointEdit> {
        if !self.can_point() {
            return None;
        }
        let target = self.edit.target?;
        let previous = self.point_at_caret(caret.start, caret.end);
        let (start, end) = previous
            .map(|p| (p.start, p.end))
            .or_else(|| match syntax? {
                PointSyntax::Insert { start, end } => Some((start, end)),
                PointSyntax::ReplaceReference { start, end } if self.edit.point_mode => {
                    Some((start, end))
                }
                PointSyntax::ReplaceReference { .. } => None,
            })?;
        let end = end.min(self.edit.draft.encode_utf16().count());
        let start = start.min(end);
        let corner = (at.row(), at.column());
        let anchor = if extend {
            previous
                .filter(|p| p.sheet == at.sheet())
                .map_or(corner, |p| p.anchor)
        } else {
            corner
        };
        let near = (anchor.0.min(corner.0), anchor.1.min(corner.1));
        let far = (anchor.0.max(corner.0), anchor.1.max(corner.1));
        let first = if target.sheet() != at.sheet() {
            let first = calc_value::CellKey::new(at.sheet(), near.0, near.1).ok()?;
            crate::address::cell(&*self.runtime.summary, first)
        } else {
            crate::address::address(near.0, near.1)
        };
        let reference = if near == far {
            first
        } else {
            format!("{}:{}", first, crate::address::address(far.0, far.1))
        };
        let (text, end) = replace_text(&self.edit.draft, start, end, &reference);
        self.edit.draft = text.clone();
        self.edit.pointed_text = text.clone();
        self.edit.point_mode = true;
        self.edit.pointed = Some(crate::PointPick {
            sheet: at.sheet(),
            start,
            end,
            anchor,
            corner,
        });
        self.edit.caret = Some(crate::TextSelection {
            start: end,
            end,
            backward: false,
        });
        self.editor.caret = end;
        self.editor.dismissed = None;
        self.editor.history.record(text.clone(), end);
        Some(PointEdit { text, caret: end })
    }
}

#[cfg(test)]
mod pointing_tests {
    use super::*;
    use crate::{CellKey, GridArea, Model, SheetId, SurfaceId, TextSelection};

    fn at(sheet: u32, row: u32, column: u32) -> CellKey {
        CellKey::new(SheetId::from_parts(sheet, 1), row, column).unwrap()
    }
    fn caret(end: usize) -> TextSelection {
        TextSelection {
            start: end,
            end,
            backward: false,
        }
    }
    fn editing(text: &str) -> Model {
        let mut model = Model::default();
        let target = at(1, 1, 1);
        let areas = vec![GridArea::new(target, target).unwrap()];
        model.selection.select(target, areas.clone());
        model
            .edit
            .begin(SurfaceId::Bar, target, text.into(), None, areas);
        model
            .editor
            .history
            .start(text.into(), text.into(), text.encode_utf16().count());
        model.runtime.summary.sheets = vec![
            (
                crate::projection::Sheet::of(target.sheet()),
                "Sheet1".into(),
            ),
            (
                crate::projection::Sheet::of(at(2, 1, 1).sheet()),
                "Q2 Data".into(),
            ),
        ];
        model
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn pointing_extends_replaces_and_qualifies_without_moving_the_edit_target() {
        let mut model = editing("=");
        let selection = model.selection.clone();
        let original = model.edit.target;
        let first = model
            .point_at(
                at(1, 3, 2),
                false,
                caret(1),
                Some(PointSyntax::Insert { start: 1, end: 1 }),
            )
            .unwrap();
        assert_eq!(first.text, "=B3");
        let range = model
            .point_at(at(1, 5, 4), true, caret(first.caret), None)
            .unwrap();
        assert_eq!(range.text, "=B3:D5");
        let other = model
            .point_at(at(2, 4, 3), true, caret(range.caret), None)
            .unwrap();
        assert_eq!(other.text, "='Q2 Data'!C4");
        assert_eq!(model.edit.pointed.unwrap().anchor, (4, 3));
        assert_eq!(model.edit.target, original);
        assert_eq!(model.selection, selection);
        assert_eq!(model.edit.draft, other.text);
        assert_eq!(model.editor.history.step(false).unwrap().0, "=B3:D5");
        assert_eq!(model.editor.history.step(true).unwrap().0, other.text);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn pointing_uses_utf16_and_retains_the_journal_caret() {
        let mut model = editing("=\"😀\"&");
        let next = model
            .point_at(
                at(1, 1, 1),
                false,
                caret(6),
                Some(PointSyntax::Insert { start: 6, end: 6 }),
            )
            .unwrap();
        assert_eq!(
            next,
            PointEdit {
                text: "=\"😀\"&A1".into(),
                caret: 8
            }
        );
        assert_eq!(model.edit.caret, Some(caret(8)));
        assert_eq!(model.editor.caret, 8);
        assert_eq!(
            model.editor.history.step(false),
            Some(("=\"😀\"&".into(), 6))
        );
        assert_eq!(model.editor.history.step(true), Some((next.text, 8)));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn pointing_refuses_busy_or_closed_edits_and_does_not_reuse_a_changed_draft() {
        let mut model = editing("=A1");
        let syntax = Some(PointSyntax::ReplaceReference { start: 1, end: 3 });
        let before = model.edit.clone();
        assert!(
            model
                .point_at(at(1, 2, 2), false, caret(3), syntax)
                .is_none()
        );
        assert_eq!(model.edit, before);
        model.edit.point_mode = true;
        model.editor.readonly = true;
        let before = model.edit.clone();
        assert!(
            model
                .point_at(at(1, 2, 2), false, caret(3), syntax)
                .is_none()
        );
        assert_eq!(model.edit, before);
        model.editor.readonly = false;
        model.editor.composing = true;
        assert!(
            model
                .point_at(at(1, 2, 2), false, caret(3), syntax)
                .is_none()
        );
        model.editor.composing = false;
        let next = model
            .point_at(at(1, 2, 2), false, caret(3), syntax)
            .unwrap();
        assert_eq!(next.text, "=B2");
        model.edit.draft = "text".into();
        assert!(
            model
                .point_at(at(1, 3, 3), true, caret(next.caret), None)
                .is_none()
        );
        assert_eq!(model.edit.draft, "text");
        model.edit.finish();
        assert!(
            model
                .point_at(
                    at(1, 3, 3),
                    false,
                    caret(4),
                    Some(PointSyntax::Insert { start: 4, end: 4 })
                )
                .is_none()
        );
    }
}
