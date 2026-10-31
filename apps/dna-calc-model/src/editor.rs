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
