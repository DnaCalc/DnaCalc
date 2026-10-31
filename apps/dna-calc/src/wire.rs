//! The messages between the page and the workbook's server (dc-p96).
//!
//! The workbook's session lives with a [`crate::server::Server`]: in the calculation worker
//! ([`crate::worker`]), or, until every skin reads through these messages, on the page itself
//! ([`crate::port`]'s in-process transport, which passes the same messages). The page sends a
//! [`Request`] and gets one [`Reply`]; the server also pushes [`Event`]s (the cells of the page's
//! subscriptions as they change). Every message is one JSON text (serde_json) through
//! `postMessage`; a workbook's bytes (to open, or saved) travel beside it. Both sides are the same
//! wasm build, so the messages are not versioned; they are tested to read back as they were
//! written.
//!
//! What crosses is what the page shows, worked out by the server: a cell travels as its
//! [`CellShown`] (its text in its number format, its provenance, its conditional look), its input
//! as text, and the words that say where its value came from; never the engine's values.

use crate::cells::{CellShown, ConditionalShown, FormatShown, Provenance};
use calc_host::value::{CellKey, GridArea, SheetId};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Area(pub Key, pub Key);

impl Area {
    pub fn of(area: GridArea) -> Area {
        Area(Key::of(area.first()), Key::of(area.last()))
    }

    pub fn area(self) -> Option<GridArea> {
        GridArea::new(self.0.cell()?, self.1.cell()?).ok()
    }
}

/// What a subscription follows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interest {
    Area(Area),
    Areas(Vec<Area>),
    Sheet(Sheet),
    /// The session's selection.
    Selection,
}

/// What the page asks the session to do: calc-host's skin intents as the page sends them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Intent {
    ShiftCells {
        area: Area,
        down: bool,
        insert: bool,
    },
    EditSheet {
        sheet: Option<Sheet>,
        change: SheetChange,
    },
    FitContents {
        sheet: Sheet,
        rows: bool,
        revision: u64,
        sizes: Vec<(u32, u64)>,
    },
    AxisFormat {
        sheet: Sheet,
        rows: bool,
        first: u32,
        count: u32,
        size_bits: Option<u64>,
        hidden: Option<bool>,
    },
    FreezePanes {
        sheet: Sheet,
        rows: u32,
        columns: u32,
    },
    ApplyPaste {
        id: u64,
    },
    EnterCell {
        at: Key,
        text: String,
    },
    EnterSelection {
        text: String,
    },
    Clear {
        areas: Vec<Area>,
    },
    Navigate {
        nav: crate::grid::selection::Navigation,
        extend: bool,
        page_height: u32,
    },
    /// Inserts (`insert`) or deletes `count` rows (`rows`) or columns from `start`.
    EditAxis {
        sheet: Sheet,
        rows: bool,
        insert: bool,
        start: u32,
        count: u32,
    },
    Copy {
        from: Area,
        to: Area,
        cut: bool,
    },
    Undo,
    Redo,
    Recalculate,
    SetCalcMode {
        manual: bool,
    },
    /// Sets the workbook's iterative-calculation settings (dc-y2o.2), one undo step: the maximum
    /// change as its binary64 bits.
    SetIteration {
        enabled: bool,
        max_iterations: u32,
        max_change_bits: u64,
    },
    Select {
        active: Key,
        areas: Vec<Area>,
    },
}

/// Worksheet commands. Positions are one-based in the full workbook order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SheetChange {
    Add { name: String },
    Rename { name: String },
    Move { position: u32 },
    Remove,
    Hide,
    Show,
}

/// The page to the server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToServer {
    pub id: u64,
    pub request: Request,
}

/// What the page asks of the server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    FitContents {
        area: Area,
    },
    Capture {
        area: Area,
        keep: Option<u64>,
    },
    PreparePaste {
        source: PasteSource,
        to: Area,
        mode: PasteMode,
    },
    /// An answer, to show the server is there and to time a round trip.
    Ping,
    /// A new, empty workbook.
    New,
    /// Opens the bytes that travel beside the message, under `name`.
    Open {
        name: String,
    },
    /// Saves the workbook: its bytes travel back beside the reply.
    Save,
    Intent(Intent),
    /// Registers subscription `id` (the page's number for it); its cells come as events.
    Subscribe {
        id: u64,
        interest: Interest,
    },
    SetInterest {
        id: u64,
        interest: Interest,
    },
    Unsubscribe {
        id: u64,
    },
    /// A sheet's sizes, as the grid lays it out.
    Geometry(Sheet),
    UsedArea(Sheet),
    ResolveReference {
        anchor: Key,
        text: String,
    },
    /// The sheet's charts, drawn.
    Charts(Sheet),
    /// The cell's data validation: its rule and, for a list, its items.
    Validation(Key),
    /// The atlas: the census and the session's state.
    Atlas,
    /// The tree's outline: each sheet's first rows (`shown`, per sheet; [`crate::tree::ROW_PAGE`]
    /// for a sheet not named) and the cells of the rows of the `open` sheets.
    Outline {
        shown: Vec<(Sheet, u64)>,
        open: Vec<Sheet>,
    },
    /// The formula cells that read the cell.
    Dependents(Key),
    /// The canvas around `center`: `back` steps of what it reads, `forward` of what reads it.
    Canvas {
        center: Key,
        back: u8,
        forward: u8,
    },
    /// One page of the ledger: the formula cells of the workbook (or of `sheet`), kept by the
    /// filter's choice and its text, in the sort's order (`active` for "reading the active cell").
    Ledger {
        sheet: Option<Sheet>,
        filter: String,
        text: String,
        sort: String,
        page: u64,
        active: Option<Key>,
    },
}

/// The workbook as the page's shell shows it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub name: String,
    pub changed: bool,
    pub undo: usize,
    pub redo: usize,
    /// The sheets in workbook order, with their names.
    pub sheets: Vec<(Sheet, String)>,
    pub hidden_sheets: Vec<(Sheet, String, bool)>,
    pub sheet_order: Vec<Sheet>,
    pub manual: bool,
    /// The session's authored revision, which advances with every committed change.
    pub revision: u64,
    /// The workbook's iterative-calculation settings (dc-y2o.2): on or off, the most iterations
    /// and the maximum change.
    pub iteration: bool,
    pub max_iterations: u32,
    pub max_change: f64,
    /// The formula cells the last recalculation found on a circular reference (dc-y2o.1), the
    /// first of them and how many, which the status line warns of.
    pub circular: Option<Key>,
    pub circular_count: usize,
}

impl dna_formula_ui::display::SheetNames for Summary {
    fn sheet_name_of(&self, sheet: SheetId) -> Option<String> {
        self.sheets
            .iter()
            .find(|(s, _)| s.id() == sheet)
            .map(|(_, name)| name.clone())
            .or_else(|| {
                self.hidden_sheets
                    .iter()
                    .find(|(s, _, _)| s.id() == sheet)
                    .map(|(_, name, _)| name.clone())
            })
    }
}

/// What an intent (or an open, a new workbook, a save) did.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub accepted: bool,
    pub committed: bool,
    pub recalculated: bool,
    /// Why the intent was refused.
    pub error: Option<String>,
    /// The outcome of the recalculation the intent ran, in words (`recalculated`, ...).
    pub recalc: Option<String>,
    /// How long the server took, in milliseconds.
    pub elapsed_ms: f64,
    /// The session's selection after the intent: its active cell and first area.
    pub selection: Option<(Key, Vec<Area>)>,
    /// For an entry the session took: what a warning or information rule of the cell said about
    /// it (the rule's kind, and its title and message), as Excel would ask or tell.
    pub note: Option<(String, String)>,
    pub summary: Summary,
}

/// A sheet's sizes as the file states them (calc-host's sheet geometry).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    pub frozen_rows: u32,
    pub frozen_columns: u32,
    pub split_window: bool,
    pub default_row_height: Option<f64>,
    pub default_column_width: Option<f64>,
    pub base_column_width: Option<f64>,
    /// Column spans: first, last, width in characters, hidden.
    pub columns: Vec<(u32, u32, Option<f64>, bool)>,
    /// Rows: row, height in points, hidden.
    pub rows: Vec<(u32, Option<f64>, bool)>,
}

impl Geometry {
    pub fn of(geometry: &calc_host::SheetGeometry) -> Geometry {
        Geometry {
            frozen_rows: 0,
            frozen_columns: 0,
            split_window: false,
            default_row_height: geometry.default_row_height,
            default_column_width: geometry.default_column_width,
            base_column_width: geometry.base_column_width,
            columns: geometry
                .columns
                .iter()
                .map(|span| (span.first, span.last, span.width, span.hidden))
                .collect(),
            rows: geometry
                .rows
                .iter()
                .map(|(&row, format)| (row, format.height, format.hidden))
                .collect(),
        }
    }

    pub fn geometry(&self) -> calc_host::SheetGeometry {
        calc_host::SheetGeometry {
            default_row_height: self.default_row_height,
            default_column_width: self.default_column_width,
            base_column_width: self.base_column_width,
            columns: self
                .columns
                .iter()
                .map(|&(first, last, width, hidden)| calc_host::ColumnSpan {
                    first,
                    last,
                    width,
                    hidden,
                    // The wire carries no customWidth flag; a skin lays out by the width alone.
                    custom_width: width.is_some(),
                })
                .collect(),
            rows: self
                .rows
                .iter()
                .map(|&(row, height, hidden)| (row, calc_host::RowFormat { height, hidden }))
                .collect(),
        }
    }
}

/// A chart as the grid places it: its frame's corners (cells and offsets in pixels at 100%), or
/// its position and size, and its drawing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chart {
    pub name: String,
    pub kind: String,
    pub place: ChartPlace,
    /// The chart's SVG, for a frame of the size `place` gives at 100%.
    pub svg: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ChartPlace {
    Cells {
        from: (Key, f64, f64),
        to: (Key, f64, f64),
    },
    Sized {
        from: (Key, f64, f64),
        width: f64,
        height: f64,
    },
    Absolute {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
}

/// A cell's data validation as a skin shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Validation {
    pub dropdown: bool,
    /// The input message: title and text.
    pub prompt: Option<(String, String)>,
    /// A list's items as text to enter.
    pub items: Vec<String>,
}

/// What the server answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteMode {
    All,
    Values,
    Formulas,
    Formats,
}

impl PasteMode {
    pub fn host(self) -> calc_host::PasteMode {
        match self {
            Self::All => calc_host::PasteMode::All,
            Self::Values => calc_host::PasteMode::Values,
            Self::Formulas => calc_host::PasteMode::Formulas,
            Self::Formats => calc_host::PasteMode::Formats,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteSource {
    Internal {
        token: u64,
        cut: bool,
    },
    External(Vec<Vec<String>>),
    Fill {
        from: Area,
        down: bool,
        series: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clipboard {
    pub token: u64,
    pub session: String,
    pub area: Area,
    pub revision: u64,
    pub text: String,
    pub html: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PastePreview {
    pub id: u64,
    pub area: Area,
    pub replaces: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    Clipboard(Clipboard),
    PastePreview(PastePreview),
    /// The answer to [`Request::Ping`]: the package version the server runs.
    Pong {
        version: String,
    },
    /// A new workbook, an open, a save or an intent: what it did. A refused open leaves the
    /// workbook as it was and says why in `error`.
    Done {
        outcome: Outcome,
        /// For an open: the time it took to read and to calculate.
        opened: Option<(f64, Option<f64>)>,
        /// For a save: the name to offer the bytes under.
        saved: Option<String>,
    },
    FitContents {
        revision: u64,
        cells: Vec<Cell>,
    },
    Subscribed,
    Geometry(Geometry),
    UsedArea(Option<Area>),
    ResolvedReference(Result<Area, String>),
    Charts(Vec<Chart>),
    Validation(Option<Validation>),
    Atlas(Atlas),
    Outline(Outline),
    Keys(Vec<Key>),
    Canvas(Canvas),
    Ledger(Ledger),
}

/// One page of the ledger: how many formula cells its scope holds, how many the filter keeps,
/// and the page's rows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    pub keys: u64,
    pub rows: u64,
    pub page: Vec<LedgerRow>,
}

/// One formula cell as the ledger's row shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LedgerRow {
    pub at: Key,
    pub label: String,
    pub formula: String,
    pub value: String,
    pub provenance: Provenance,
    pub format: String,
    pub reads: u64,
    pub read_by: u64,
}

/// The canvas as the page draws it: the chips at their places, the counts left out, the wires,
/// the plane's size, and the inspector of the centre cell.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Canvas {
    pub chips: Vec<Chip>,
    /// Columns that leave chips out: the column, where the count goes, how many.
    pub hidden: Vec<(i32, f64, f64, usize)>,
    pub wires_svg: String,
    pub width: f64,
    pub height: f64,
    pub inspector: Inspector,
}

/// One chip: a cell (with what was entered, its value and provenance) or a range.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chip {
    pub column: i32,
    pub x: f64,
    pub y: f64,
    pub label: String,
    /// The cell, for a cell's chip; `None` for a range's.
    pub cell: Option<Key>,
    pub input: String,
    pub value: String,
    pub provenance: Provenance,
    /// A range's cell count.
    pub count: u64,
}

/// The centre cell as the canvas's panel shows it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Inspector {
    pub label: String,
    pub input: String,
    pub value: String,
    pub why: String,
    pub format: String,
    pub reads: Vec<String>,
    pub read_by: Vec<String>,
}

/// The tree's outline: the workbook's name, its defined names (`Name = formula`), its sheets
/// (each with its cell count, its first rows' cells and how many rows follow) and the cells of
/// the open sheets' rows, as the page shows them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Outline {
    pub workbook: String,
    pub names: Vec<String>,
    pub sheets: Vec<OutlineSheet>,
    pub cells: Vec<Cell>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutlineSheet {
    pub sheet: Sheet,
    pub name: String,
    pub cells: u64,
    pub rows: Vec<(u32, Vec<Key>)>,
    pub more_rows: u64,
}

/// What the atlas shows: the formula cells by provenance and the session's calculation state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Atlas {
    pub census: crate::workbook::Census,
    /// The calculation mode, in words.
    pub mode: String,
    pub revision: u64,
    pub undo: usize,
    pub redo: usize,
    pub intents: usize,
    pub name: String,
    pub changed: bool,
    /// The server's wasm linear memory, in bytes (the calculation worker's, in a browser).
    pub memory: Option<f64>,
}

/// One cell of a subscription, as the page shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub at: Key,
    pub text: String,
    pub kind: String,
    pub provenance: Provenance,
    pub formula: bool,
    pub format: FormatShown,
    pub color: Option<String>,
    pub conditional: Option<ConditionalShown>,
    /// The cell's input as it would be edited (`=A1+1`, `12`), empty for none.
    pub input: String,
    /// Where the value came from, in words.
    pub why: String,
    pub owner: Option<String>,
    pub error_meaning: Option<String>,
}

impl Cell {
    pub fn of(
        shown: &CellShown,
        input: String,
        why: String,
        projected: &calc_host::ProjectedCell,
    ) -> Cell {
        Cell {
            at: Key::of(shown.at),
            text: shown.text.clone(),
            kind: shown.kind.to_owned(),
            provenance: shown.provenance,
            formula: shown.formula,
            format: shown.format,
            color: shown.color.clone(),
            conditional: shown.conditional.clone(),
            input,
            why,
            owner: crate::cells::refusal_owner(projected),
            error_meaning: crate::cells::error_meaning(projected),
        }
    }

    /// The cell as the grid draws it.
    pub fn shown(&self) -> Option<CellShown> {
        Some(CellShown {
            at: self.at.cell()?,
            text: self.text.clone(),
            kind: match self.kind.as_str() {
                "number" => "number",
                "text" => "text",
                "logical" => "logical",
                "error" => "error",
                "array" => "array",
                _ => "",
            },
            provenance: self.provenance,
            formula: self.formula,
            format: self.format,
            color: self.color.clone(),
            conditional: self.conditional.clone(),
        })
    }
}

/// Changed cells of the page's subscription `id`, as calc-host's deltas give them: `reset` means
/// these are all of its cells.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cells {
    pub id: u64,
    pub seq: u64,
    pub reset: bool,
    pub cells: Vec<Cell>,
    /// Cells that became empty.
    pub emptied: Vec<Key>,
}

/// What the server tells the page without being asked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    Cells(Cells),
}

/// The server to the page. A message lives only until it is encoded or applied, so its
/// variants' sizes are left as they are.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum FromServer {
    /// The worker is loaded and listening.
    Started,
    /// The answer to the request `id`, and whether bytes travel beside it.
    Reply {
        id: u64,
        reply: Reply,
    },
    /// The request `id` (or, with no id, a message that was not a request) could not be served.
    Failed {
        id: Option<u64>,
        message: String,
    },
    Event(Event),
}

/// A message as the JSON text `postMessage` carries.
pub fn encode<T: Serialize>(message: &T) -> String {
    serde_json::to_string(message).expect("a wire message always encodes")
}

/// A message from its JSON text.
pub fn decode<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    serde_json::from_str(text).map_err(|e| format!("not a message: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn messages_read_back_as_written() {
        let at = Key(3, 1, 7, 2);
        let request = ToServer {
            id: 7,
            request: Request::Intent(Intent::EnterCell {
                at,
                text: "=A1+0.1".to_owned(),
            }),
        };
        assert_eq!(decode::<ToServer>(&encode(&request)), Ok(request));
        let cell = Cell {
            at,
            text: "0.30000000000000004".to_owned(),
            kind: "number".to_owned(),
            provenance: Provenance::Calculated,
            formula: true,
            format: FormatShown::General,
            color: None,
            conditional: Some(ConditionalShown {
                bar: Some((0.25, "#638ec6".to_owned(), true)),
                ..ConditionalShown::default()
            }),
            input: "=A1+0.1".to_owned(),
            why: "calculated".to_owned(),
            owner: None,
            error_meaning: None,
        };
        for message in [
            FromServer::Started,
            FromServer::Reply {
                id: 7,
                reply: Reply::Pong {
                    version: "0.1.0".to_owned(),
                },
            },
            FromServer::Failed {
                id: None,
                message: "no".to_owned(),
            },
            FromServer::Event(Event::Cells(Cells {
                id: 1,
                seq: 2,
                reset: false,
                cells: vec![cell.clone()],
                emptied: vec![at],
            })),
        ] {
            assert_eq!(decode::<FromServer>(&encode(&message)), Ok(message));
        }
        assert_eq!(cell.shown().map(|shown| shown.kind), Some("number"));
        assert!(decode::<ToServer>("{\"id\":1}").is_err());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn numbers_cross_exactly() {
        let geometry = Geometry {
            frozen_rows: 1,
            frozen_columns: 2,
            split_window: false,
            default_row_height: Some(0.1 + 0.2),
            default_column_width: Some(8.43),
            base_column_width: None,
            columns: vec![(1, 3, Some(1.0 / 3.0), false)],
            rows: vec![(2, Some(f64::MIN_POSITIVE), true)],
        };
        let back: Reply = decode(&encode(&Reply::Geometry(geometry.clone()))).unwrap();
        assert_eq!(back, Reply::Geometry(geometry));
    }
}
