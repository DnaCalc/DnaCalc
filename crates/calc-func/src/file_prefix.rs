//! The file prefix associated with a catalogue entry.
use crate::catalogue::{FunctionCallTarget, Notation};
pub use calc_catalogue::FilePrefix;

impl FunctionCallTarget<'_> {
    /// The file prefix for a named function, when measured.
    pub fn file_prefix(self) -> Option<FilePrefix> {
        if self.notation() != Notation::Call { return None; }
        calc_catalogue::declaration(self.id()).and_then(|entry| entry.file_prefix())
    }
}
