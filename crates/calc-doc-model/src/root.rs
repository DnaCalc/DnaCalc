//! The immutable model root: one authored revision of a document's sheets.

use crate::revision::AuthoredRevision;
use crate::sheet::{SheetKey, SheetName, SheetVisibility};
use calc_value::SheetId;
use std::fmt;
use std::sync::Arc;

/// One sheet of a [`ModelRoot`]: its session ID, its document-stable key, its name and its
/// visibility.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetEntry {
    id: SheetId,
    key: SheetKey,
    name: SheetName,
    visibility: SheetVisibility,
}

impl SheetEntry {
    pub fn new(id: SheetId, key: SheetKey, name: SheetName, visibility: SheetVisibility) -> Self {
        SheetEntry {
            id,
            key,
            name,
            visibility,
        }
    }

    pub fn id(&self) -> SheetId {
        self.id
    }

    pub fn key(&self) -> SheetKey {
        self.key
    }

    pub fn name(&self) -> &SheetName {
        &self.name
    }

    pub fn visibility(&self) -> SheetVisibility {
        self.visibility
    }
}

/// Why [`ModelRoot::new`] refused its sheets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModelRootError {
    /// Two sheets have this key.
    DuplicateSheetKey(SheetKey),
    /// Two sheets have this ID.
    DuplicateSheetId(SheetId),
}

impl fmt::Display for ModelRootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelRootError::DuplicateSheetKey(key) => {
                write!(f, "two sheets have the key {}", key.get())
            }
            ModelRootError::DuplicateSheetId(id) => write!(f, "two sheets have the ID {id:?}"),
        }
    }
}

impl std::error::Error for ModelRootError {}

/// One authored revision of a document: the revision and the sheets in order. A root is
/// immutable and shared behind an `Arc`; a change makes a new root. It maps each sheet's
/// [`SheetKey`] to its [`SheetId`] and back, which is how wire operations, which carry keys, are
/// resolved. It holds no cell storage yet.
///
/// It keeps no record of the keys that removed sheets held, so it cannot tell a reused key from a
/// new one; whoever applies a [`crate::SheetEdit`] checks that a key is never reused within one
/// document.
#[derive(Debug, PartialEq, Eq)]
pub struct ModelRoot {
    revision: AuthoredRevision,
    sheets: Vec<SheetEntry>,
}

impl ModelRoot {
    /// The root of these sheets, in this order, at `revision`; refused when two sheets share a
    /// key or an ID.
    pub fn new(
        revision: AuthoredRevision,
        sheets: Vec<SheetEntry>,
    ) -> Result<Arc<ModelRoot>, ModelRootError> {
        for (index, sheet) in sheets.iter().enumerate() {
            let earlier = &sheets[..index];
            if earlier.iter().any(|other| other.key == sheet.key) {
                return Err(ModelRootError::DuplicateSheetKey(sheet.key));
            }
            if earlier.iter().any(|other| other.id == sheet.id) {
                return Err(ModelRootError::DuplicateSheetId(sheet.id));
            }
        }
        Ok(Arc::new(ModelRoot { revision, sheets }))
    }

    pub fn revision(&self) -> AuthoredRevision {
        self.revision
    }

    /// The sheets in workbook order.
    pub fn sheets(&self) -> &[SheetEntry] {
        &self.sheets
    }

    pub fn sheet_by_key(&self, key: SheetKey) -> Option<&SheetEntry> {
        self.sheets.iter().find(|sheet| sheet.key == key)
    }

    pub fn sheet_by_id(&self, id: SheetId) -> Option<&SheetEntry> {
        self.sheets.iter().find(|sheet| sheet.id == id)
    }
}
