//! Authored note and threaded-comment text with the source identity retained for lossless saves.
use crate::value::CellKey;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ReviewSource {
    Note {
        part: String,
        origin: CellKey,
    },
    Thread {
        part: String,
        id: String,
        parent: Option<String>,
        person: String,
        timestamp: Option<String>,
        resolved: bool,
    },
}
impl ReviewSource {
    pub fn part(&self) -> &str {
        match self {
            Self::Note { part, .. } | Self::Thread { part, .. } => part,
        }
    }
    pub fn threaded(&self) -> bool {
        matches!(self, Self::Thread { .. })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReviewNote {
    pub at: CellKey,
    pub author: String,
    pub text: String,
    pub source: Option<ReviewSource>,
    /// Text carrying mentions or unsupported annotations is preserved until their offsets can move.
    pub editable: bool,
    /// Legacy compatibility text for a threaded comment, retained but not a second annotation.
    pub compatibility: bool,
}

/// Plain compatibility text for readers that do not understand threaded comments.
pub fn review_thread_text(notes: &[ReviewNote], at: CellKey) -> String {
    notes
        .iter()
        .filter(|n| n.at == at && n.source.as_ref().is_some_and(ReviewSource::threaded))
        .map(|n| format!("{}: {}", n.author, n.text))
        .collect::<Vec<_>>()
        .join("\n\n")
}
