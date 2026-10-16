//! The revision of a document's authored state.

/// The revision of a document's authored state: a counter that advances by one with each
/// committed batch of authored operations. It is a document-scoped number, not an allocator-issued
/// identity, so it can be rebuilt from a journal or a wire message with [`AuthoredRevision::new`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AuthoredRevision(u64);

impl AuthoredRevision {
    /// The revision of a document before its first authored change.
    pub const INITIAL: AuthoredRevision = AuthoredRevision(0);

    pub const fn new(value: u64) -> Self {
        AuthoredRevision(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// The next revision, or `None` when this one is `u64::MAX`.
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(next) => Some(AuthoredRevision(next)),
            None => None,
        }
    }
}
