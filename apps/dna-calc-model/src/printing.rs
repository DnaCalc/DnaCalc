//! Print projections and header/footer rendering for the existing browser preview.

pub use calc_skin::print::*;
use crate::projection::{Cell, Chart, Geometry, Sheet};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrintDocument {
    pub revision: u64,
    pub sheet: Sheet,
    pub sheet_name: String,
    pub book_name: String,
    pub settings: PageSettings,
    pub geometry: Geometry,
    pub plan: PrintPlan,
    pub cells: Vec<Cell>,
    pub charts: Vec<Chart>,
    /// The sheet's shapes, text boxes and pictures as the grid draws them (dc-zjsg.2).
    #[serde(default)]
    pub shapes: Vec<crate::drawn_shapes::DrawnShape>,
    /// The images of its pictures by their identity, as `data:` URLs (dc-zjsg.3).
    #[serde(default)]
    pub pictures: Vec<(u32, String)>,
    /// The sheet's sparklines (dc-zjsg.7).
    #[serde(default)]
    pub sparklines: Vec<crate::sparklines::DrawnSparkline>,
    /// The sheet's legacy form controls printed with it (dc-zjsg.8).
    #[serde(default)]
    pub controls: Vec<crate::controls::DrawnControl>,
    pub notices: Vec<String>,
}
