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

pub trait ViewState: Default + Send + Sync + 'static {
    const SCHEMA: u32 = 1;
    fn pending_reason(&self) -> Option<&'static str> {
        None
    }
}
