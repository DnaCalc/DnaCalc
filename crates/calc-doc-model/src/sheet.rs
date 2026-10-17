//! A sheet's document-stable key, its name, and its document-only metadata.

use calc_value::ExcelText;
use std::fmt;

/// A sheet's document-stable key: unique among a document's sheets and never reused within one
/// document, the role the xlsx `sheetId` attribute plays. Operations on the wire carry keys, never
/// [`calc_value::SheetId`]s, because an ID is scoped to the allocator of one session; the
/// receiving [`crate::ModelRoot`] maps each key to its sheet.
///
/// This package checks only that a key is unique among one root's sheets and the batch's
/// inserts. A [`crate::ModelRoot`] keeps no record of the keys removed sheets held, so "never
/// reused within one document" is not checked here: whoever applies a [`crate::SheetEdit`] checks
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SheetKey(u32);

impl SheetKey {
    pub const fn new(value: u32) -> Self {
        SheetKey(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// The longest sheet name, in UTF-16 code units.
pub const SHEET_NAME_MAX_UTF16_CODE_UNITS: usize = 31;

/// The code units a sheet name may not contain: `:` `\` `/` `?` `*` `[` `]`.
const FORBIDDEN_NAME_UNITS: [u16; 7] = [0x3A, 0x5C, 0x2F, 0x3F, 0x2A, 0x5B, 0x5D];

const APOSTROPHE: u16 = 0x27;

/// Why [`SheetName::new`] refused a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SheetNameError {
    Empty,
    /// The name has more than [`SHEET_NAME_MAX_UTF16_CODE_UNITS`] code units.
    TooLong {
        units: usize,
    },
    /// The name holds one of `:` `\` `/` `?` `*` `[` `]`.
    ForbiddenCharacter {
        unit: u16,
    },
    /// The name begins or ends with an apostrophe.
    BoundaryApostrophe,
}

impl fmt::Display for SheetNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SheetNameError::Empty => f.write_str("the sheet name is empty"),
            SheetNameError::TooLong { units } => write!(
                f,
                "a sheet name has at most {SHEET_NAME_MAX_UTF16_CODE_UNITS} UTF-16 code units, not {units}"
            ),
            SheetNameError::ForbiddenCharacter { unit } => {
                write!(
                    f,
                    "the sheet name contains U+{unit:04X}, which a sheet name may not contain"
                )
            }
            SheetNameError::BoundaryApostrophe => {
                f.write_str("the sheet name begins or ends with an apostrophe")
            }
        }
    }
}

impl std::error::Error for SheetNameError {}

/// A sheet name: 1 to [`SHEET_NAME_MAX_UTF16_CODE_UNITS`] UTF-16 code units, none of them
/// `:` `\` `/` `?` `*` `[` `]`, and no apostrophe first or last. Those are the only rules checked
/// here; a reserved name and uniqueness among a document's sheets (compared without case) are not.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SheetName(ExcelText);

impl SheetName {
    pub fn new(text: ExcelText) -> Result<Self, SheetNameError> {
        let units = text.utf16_code_units();
        let (Some(first), Some(last)) = (units.first(), units.last()) else {
            return Err(SheetNameError::Empty);
        };
        if units.len() > SHEET_NAME_MAX_UTF16_CODE_UNITS {
            return Err(SheetNameError::TooLong { units: units.len() });
        }
        if let Some(unit) = units
            .iter()
            .find(|unit| FORBIDDEN_NAME_UNITS.contains(unit))
        {
            return Err(SheetNameError::ForbiddenCharacter { unit: *unit });
        }
        if *first == APOSTROPHE || *last == APOSTROPHE {
            return Err(SheetNameError::BoundaryApostrophe);
        }
        Ok(SheetName(text))
    }

    pub fn text(&self) -> &ExcelText {
        &self.0
    }
}

/// Document-only metadata: whether a sheet is shown. It is not a calculation input, so nothing
/// built for CalcEngine reads it; a [`crate::SheetEdit::SetVisibility`] sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SheetVisibility {
    Visible,
    /// Hidden, and listed for unhiding.
    Hidden,
    /// Hidden, and not listed for unhiding (xlsx `veryHidden`).
    VeryHidden,
}
