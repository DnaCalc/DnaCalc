use calc_value::{CellKey, GridArea, SheetId};
use serde::{Deserialize, Serialize};

/// A cell: its sheet's slot and generation, its row and its column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Key(pub u32, pub u32, pub u32, pub u32);

impl Key {
    pub fn of(at: CellKey) -> Key {
        Key(
            at.sheet().slot(),
            at.sheet().generation(),
            at.row(),
            at.column(),
        )
    }

    pub fn cell(self) -> Option<CellKey> {
        CellKey::new(SheetId::from_parts(self.0, self.1), self.2, self.3).ok()
    }
}

/// A sheet: its slot and generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Sheet(pub u32, pub u32);

impl Sheet {
    pub fn of(sheet: SheetId) -> Sheet {
        Sheet(sheet.slot(), sheet.generation())
    }

    pub fn id(self) -> SheetId {
        SheetId::from_parts(self.0, self.1)
    }
}

/// An area: its first and last cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Area(pub Key, pub Key);

impl Area {
    pub fn of(area: GridArea) -> Area {
        Area(Key::of(area.first()), Key::of(area.last()))
    }

    pub fn area(self) -> Option<GridArea> {
        GridArea::new(self.0.cell()?, self.1.cell()?).ok()
    }
}
