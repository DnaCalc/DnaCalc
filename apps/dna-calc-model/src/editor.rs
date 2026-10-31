//! Local formula editing state, independent of the DOM and workbook undo.
/// The current text edit's undo journal. It never contains workbook operations.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextHistory {
    entries: Vec<(String, usize)>,
    current: usize,
}

impl TextHistory {
    pub fn available(&self, redo: bool) -> bool {
        if redo {
            self.current + 1 < self.entries.len()
        } else {
            self.current > 0
        }
    }
    pub fn start(&mut self, original: String, text: String, end: usize) {
        let caret = original.encode_utf16().count();
        self.reset(original, caret);
        self.record(text, end);
    }

    pub fn reset(&mut self, text: String, caret: usize) {
        self.entries = vec![(text, caret)];
        self.current = 0;
    }

    pub fn record(&mut self, text: String, caret: usize) {
        if self
            .entries
            .get(self.current)
            .is_some_and(|(old, _)| *old == text)
        {
            return;
        }
        self.entries.truncate(self.current + 1);
        self.entries.push((text, caret));
        if self.entries.len() > 256 {
            self.entries.remove(0);
        }
        self.current = self.entries.len() - 1;
    }

    pub fn step(&mut self, redo: bool) -> Option<(String, usize)> {
        let next = if redo {
            self.current.checked_add(1)?
        } else {
            self.current.checked_sub(1)?
        };
        let entry = self.entries.get(next)?.clone();
        self.current = next;
        Some(entry)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditorState {
    pub caret: usize,
    pub dismissed: Option<String>,
    pub composing: bool,
    pub readonly: bool,
    pub history: TextHistory,
}

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
        let previous = self.edit.pointed.filter(|p| {
            self.edit.pointed_text == self.edit.draft
                && caret.start == caret.end
                && caret.end == p.end
        });
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

/// Replace a browser selection, counted in UTF-16, and return the caret after the insertion.
pub fn replace_text(text: &str, start: usize, end: usize, insert: &str) -> (String, usize) {
    let units: Vec<u16> = text.encode_utf16().collect();
    let end = end.min(units.len());
    let start = start.min(end);
    let mut out = units[..start].to_vec();
    out.extend(insert.encode_utf16());
    let caret = out.len();
    out.extend_from_slice(&units[end..]);
    (String::from_utf16_lossy(&out), caret)
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
