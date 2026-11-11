//! Skin-neutral workbook commands and projections. Only values, identities and presentation
//! data cross this boundary; no host, evaluator, document or reactive implementation is exposed.
use calc_macro_security::{MacroPolicy, MacroSecurity};
use calc_value::{CellKey, GridArea, SheetId};
use serde::{Deserialize, Serialize};

/// Where a shown value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Provenance {
    /// The cell holds nothing.
    Empty,
    /// The cell's input is a literal, which is its value.
    Literal,
    /// The engine calculated the value, and it still stands.
    Calculated,
    /// The value the file stored for the formula; nothing here calculated it.
    FileCache,
    /// The engine refused the formula: it has no value.
    Refused,
    /// The formula has no value that stands at the current revision.
    Stale,
}

impl Provenance {
    /// The name the page carries in `data-provenance` and the class suffix.
    pub const fn name(self) -> &'static str {
        match self {
            Provenance::Empty => "empty",
            Provenance::Literal => "literal",
            Provenance::Calculated => "calculated",
            Provenance::FileCache => "file-cache",
            Provenance::Refused => "refused",
            Provenance::Stale => "stale",
        }
    }
}

/// One cell as a skin shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct CellShown {
    pub style: TextStyle,
    pub at: CellKey,
    /// The text in the cell: the value as a cell of standard width shows it, `refused` for a
    /// refusal, a stale cell's last value, or nothing.
    pub text: String,
    /// The value's type: `number`, `text`, `logical`, `error`, `array`, or empty.
    pub kind: &'static str,
    pub provenance: Provenance,
    /// Whether the cell's input is a formula.
    pub formula: bool,
    /// How the cell's number format applied.
    pub format: FormatShown,
    /// The color the number format gives the text (`[Red]`, `[Color10]`), as `#rrggbb`.
    pub color: Option<String>,
    /// What the sheet's conditional formats give the cell (dc-6yw), its colours resolved.
    pub conditional: Option<ConditionalShown>,
}

/// A cell's look under its conditional formats, as the grid draws it: colours as `#rrggbb`.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConditionalShown {
    pub fill: Option<String>,
    pub font_color: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// A data bar: the share of the cell's width (0 to 1), its colour, whether it is a gradient.
    pub bar: Option<(f64, String, bool)>,
    /// An icon: its set's name and its index.
    pub icon: Option<(String, usize)>,
    /// The value is hidden (a data bar or icon set that shows no value).
    pub hide_value: bool,
}

/// How a cell's number format applied to what it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FormatShown {
    /// The General format (or no value to format).
    General,
    /// The cell's format code, rendered by calc-func's TEXT renderer.
    Applied,
    /// The renderer does not model the code: the value is shown in General instead.
    NotModelled,
    /// The value cannot be shown under the code (a negative date): `#` as Excel's cell shows it.
    Unshowable,
}

impl FormatShown {
    /// The name the page carries in `data-format`.
    pub const fn name(self) -> &'static str {
        match self {
            FormatShown::General => "general",
            FormatShown::Applied => "applied",
            FormatShown::NotModelled => "not-modelled",
            FormatShown::Unshowable => "unshowable",
        }
    }
}

/// The text geometry the Grid actually renders. The workbook's base font maps to the app's
/// system font; other families must be available in the browser before AutoFit can use them.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextStyle {
    /// A supported pivot DisplayFormat number-code override. When absent the cell code remains
    /// in use; `unsupported` identifies appearance that has not been fully modelled. Text keeps
    /// its usual cell and conditional-format rendering path, which can differ on pivot totals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_number_format: Option<String>,
    pub font_color: Option<String>,
    pub fill: Option<String>,
    #[serde(default)]
    pub brush: Option<FillShown>,
    #[serde(default)]
    pub diagonal: [Option<BorderShown>; 2],
    pub underline: bool,
    #[serde(default)]
    pub underline_style: Option<String>,
    pub strike: bool,
    /// Left, right, top and bottom at the physical cell boundary.
    pub borders: [Option<BorderShown>; 4],
    pub family: Option<String>,
    pub pixels: f64,
    pub bold: bool,
    pub italic: bool,
    pub wrap: bool,
    #[serde(default)]
    pub shrink: bool,
    pub rotation: i16,
    pub indent: u16,
    pub horizontal: Option<String>,
    /// Consecutive blank cells sharing fill/centerContinuous, including the source column.
    pub alignment_columns: u32,
    pub vertical: Option<String>,
    pub width: f64,
    pub unsupported: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BorderShown {
    pub style: String,
    pub color: Option<String>,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self {
            display_number_format: None,
            font_color: None,
            fill: None,
            brush: None,
            diagonal: Default::default(),
            underline: false,
            underline_style: None,
            strike: false,
            borders: Default::default(),
            family: None,
            pixels: 13.,
            bold: false,
            italic: false,
            wrap: false,
            shrink: false,
            rotation: 0,
            indent: 0,
            horizontal: None,
            alignment_columns: 1,
            vertical: None,
            width: 64.,
            unsupported: None,
        }
    }
}
/// One sheet's formula cells by provenance.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SheetCensus {
    pub name: String,
    pub identity: Option<Sheet>,
    /// The used area's label, when the sheet holds anything.
    pub used: Option<String>,
    pub inputs: usize,
    pub formulas: usize,
    pub calculated: usize,
    pub refused: usize,
    pub file_cached: usize,
    pub stale: usize,
}

impl SheetCensus {
    /// Formula cells whose host projection has no calculated, refused, cached or stale value.
    pub fn pending(&self) -> usize {
        self.formulas
            .saturating_sub(self.calculated + self.refused + self.file_cached + self.stale)
    }
}

/// The workbook's formula cells by provenance, per sheet, and the refusals by reason.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Census {
    pub sheets: Vec<SheetCensus>,
    pub refusals: std::collections::BTreeMap<String, usize>,
}

impl Census {
    pub fn pending(&self) -> usize {
        self.sheets.iter().map(SheetCensus::pending).sum()
    }
    pub fn formulas(&self) -> usize {
        self.sheets.iter().map(|sheet| sheet.formulas).sum()
    }

    pub fn calculated(&self) -> usize {
        self.sheets.iter().map(|sheet| sheet.calculated).sum()
    }

    pub fn refused(&self) -> usize {
        self.sheets.iter().map(|sheet| sheet.refused).sum()
    }

    pub fn file_cached(&self) -> usize {
        self.sheets.iter().map(|sheet| sheet.file_cached).sum()
    }

    pub fn stale(&self) -> usize {
        self.sheets.iter().map(|sheet| sheet.stale).sum()
    }
}
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
    RefreshAll {
        revision: u64,
    },
    PivotCreate {
        revision: u64,
        source: Area,
        destination: String,
        row: u32,
        value: u32,
        subtotal: String,
    },
    PivotRefresh {
        sheet: Sheet,
        revision: u64,
        part: String,
    },
    PageSetup {
        sheet: Sheet,
        revision: u64,
        change: crate::printing::PageSetupChange,
    },
    ChartCreate {
        revision: u64,
        area: Area,
        kind: String,
        rows: bool,
        headers: bool,
        categories: bool,
    },
    ChartSeriesStyle {
        sheet: Sheet,
        revision: u64,
        part: String,
        plot: u32,
        series: u32,
        fill: Option<String>,
    },
    ChartElementTextStyle {
        sheet: Sheet,
        revision: u64,
        part: String,
        target: String,
        style: ChartTextStyle,
    },
    ChartTextStyle {
        sheet: Sheet,
        revision: u64,
        part: String,
        style: ChartTextStyle,
    },
    ChartType {
        sheet: Sheet,
        revision: u64,
        part: String,
        plot: u32,
        kind: String,
        stacked: bool,
    },
    ChartLegend {
        sheet: Sheet,
        revision: u64,
        part: String,
        position: String,
    },
    ChartLogAxis {
        sheet: Sheet,
        revision: u64,
        part: String,
        axis: u32,
        base: Option<String>,
    },
    ChartAxis {
        sheet: Sheet,
        revision: u64,
        part: String,
        axis: u32,
        minimum: Option<String>,
        maximum: Option<String>,
        major_unit: Option<String>,
        reversed: bool,
    },
    ChartSeries {
        sheet: Sheet,
        revision: u64,
        part: String,
        plot: u32,
        series: Option<u32>,
        name: Option<String>,
        categories: Option<String>,
        values: Option<String>,
        remove: bool,
    },
    ChartProperties {
        sheet: Sheet,
        revision: u64,
        part: String,
        title: Option<String>,
        set_title: bool,
        legend: Option<bool>,
    },
    CreateReview {
        at: Key,
        revision: u64,
        parent: Option<u32>,
        threaded: bool,
        author: String,
        text: String,
        timestamp: String,
    },
    FormatReview {
        at: Key,
        revision: u64,
        index: u32,
        text: String,
        start: u32,
        end: u32,
        patch: ReviewFormat,
    },
    ResolveReview {
        at: Key,
        revision: u64,
        index: u32,
        resolved: bool,
    },
    EditReview {
        at: Key,
        revision: u64,
        index: u32,
        text: String,
    },
    EditName {
        id: Option<(u32, u32)>,
        version: Option<u64>,
        name: String,
        scope: Option<Sheet>,
        formula: Option<String>,
    },
    FormatCells {
        areas: Vec<Area>,
        revision: u64,
        patch: crate::format::FormatPatch,
    },
    NotebookLink {
        anchor: Area,
        prose: bool,
        text: Option<String>,
    },
    NotebookOrder {
        placement: String,
        direction: i8,
    },

    Replace {
        options: FindOptions,
        replacement: String,
        revision: u64,
        only: Option<Key>,
    },
    EditConditional {
        sheet: Sheet,
        revision: u64,
        index: Option<u32>,
        change: ConditionalChange,
    },
    AcceptValidation {
        token: u64,
    },
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
    Outline {
        sheet: Sheet,
        rows: bool,
        revision: u64,
        change: OutlineChange,
    },
    AxisFormat {
        sheet: Sheet,
        rows: bool,
        first: u32,
        count: u32,
        size_bits: Option<u64>,
        hidden: Option<bool>,
    },
    SplitPanes {
        sheet: Sheet,
        revision: u64,
        x: u64,
        y: u64,
        row: u32,
        column: u32,
    },
    FreezePanes {
        sheet: Sheet,
        rows: u32,
        columns: u32,
    },
    Table {
        revision: u64,
        change: TableChange,
    },
    AutoFilter {
        sheet: Sheet,
        revision: u64,
        table: Option<String>,
        change: FilterChange,
    },
    SortRowsWithOptions {
        area: Area,
        keys: Vec<(u32, bool)>,
        header: bool,
        revision: u64,
        case_sensitive: bool,
        custom_order: Vec<String>,
    },
    SortRows {
        area: Area,
        keys: Vec<(u32, bool)>,
        header: bool,
        revision: u64,
    },
    ApplyPaste {
        id: u64,
        #[serde(default)]
        select_target: bool,
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
        nav: crate::selection::Navigation,
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
    Rebuild,
    CalculateRange {
        areas: Vec<Area>,
    },
    Calculate {
        scope: CalculationScope,
        selection: CalculationSelection,
    },
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConditionalChange {
    /// Add a cell, formula, statistical or duplicate condition with a solid fill.
    Condition {
        kind: String,
        applies: String,
        color: String,
        formulas: Vec<String>,
        operator: String,
        rank: u32,
        percent: bool,
        stop: bool,
    },
    Add {
        kind: String,
        applies: String,
        color: String,
    },
    Update {
        applies: String,
        formulas: Vec<String>,
        stop: bool,
    },
    Move {
        up: bool,
    },
    Remove,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub next_index: u32,
    pub at: Option<Key>,
    pub revision: u64,
    pub rules: Vec<Rule>,
    pub notes: Vec<ReviewItem>,
    pub links: Vec<ReviewLink>,
    pub unsupported: Vec<String>,
    pub validation: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewItem {
    pub runs: Vec<ReviewRun>,
    pub index: u32,
    pub author: String,
    pub text: String,
    pub threaded: bool,
    pub reply: bool,
    pub resolved: bool,
    pub editable: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub index: u32,
    pub kind: String,
    pub priority: i64,
    pub applies: String,
    pub formulas: Vec<String>,
    pub stop: bool,
    pub state: String,
    pub editable: bool,
    pub formula_editable: bool,
    pub visual: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewLink {
    pub target: Option<String>,
    pub location: Option<String>,
    pub display: String,
    pub tooltip: String,
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

/// An identity is issued once by the model and retained for retransmission. The target is
/// resolved at submission, so a selection-relative command cannot follow a later selection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandEnvelope {
    pub session: String,
    pub id: u64,
    pub workbook: u64,
    pub base_revision: u64,
    pub source: String,
    pub undo_group: Option<u64>,
    pub resolved_selection: Option<(Key, Vec<Area>)>,
    pub request: Box<Request>,
}

/// The page to the server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToServer {
    pub id: u64,
    pub request: Request,
}

/// Commands carry neutral criteria; the host validates and owns their authored representation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterChange {
    Enable(Area),
    Values {
        column: u32,
        values: Vec<String>,
        blank: bool,
    },
    Custom {
        column: u32,
        and: bool,
        predicates: Vec<(String, String)>,
    },
    Dates {
        column: u32,
        groups: Vec<Vec<u16>>,
        values: Vec<String>,
        blank: bool,
    },
    Top {
        column: u32,
        top: bool,
        percent: bool,
        value: String,
    },
    Color {
        column: u32,
        cell_color: bool,
        color: String,
    },
    ColorReference {
        column: u32,
        dxf_id: u32,
        cell_color: bool,
    },
    ClearColumn(u32),
    Clear,
    Remove,
    Reapply,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterView {
    #[serde(default)]
    pub table: Option<String>,
    pub area: Area,
    pub columns: Vec<u32>,
    pub filtered: u32,
    pub unsupported: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableView {
    pub part: String,
    pub name: String,
    pub area: Area,
    pub columns: Vec<String>,
    #[serde(default)]
    pub column_formulas: Vec<Option<String>>,
    pub headers: bool,
    pub totals: bool,
    pub style: String,
    pub row_stripes: bool,
    pub column_stripes: bool,
    pub first_column: bool,
    pub last_column: bool,
    pub opaque: bool,
    pub filter: Option<FilterView>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableChange {
    Create {
        area: Area,
        name: String,
    },
    Resize {
        part: String,
        area: Area,
    },
    Options {
        part: String,
        headers: bool,
        totals: bool,
        style: String,
        row_stripes: bool,
        column_stripes: bool,
        first_column: bool,
        last_column: bool,
    },
    Total {
        part: String,
        column: u32,
        function: String,
    },
    RenameColumn {
        part: String,
        column: u32,
        name: String,
    },
    CalculatedColumn {
        part: String,
        column: u32,
        formula: Option<String>,
    },
}

/// What a painter needs of a picture in a cell, beside its bytes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CellImage {
    pub source: String,
    pub alt_text: Option<String>,
    /// 0 fit, 1 fill, 2 original size, 3 custom size.
    pub sizing: u8,
    pub height: Option<f64>,
    pub width: Option<f64>,
    pub content_type: String,
}

/// What the page asks of the server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// The picture a cell holds: [`Reply::CellImage`], with its bytes beside the reply.
    CellImage {
        at: Key,
    },
    PageSetup(Sheet),
    PrintPreview {
        sheet: Sheet,
        revision: u64,
    },
    InspectTemplate,
    OpenTemplate {
        name: String,
    },
    InsertTemplate {
        name: Option<String>,
        revision: u64,
    },
    FilterValues {
        sheet: Sheet,
        table: Option<String>,
        column: u32,
    },
    Names {
        query: String,
        page: u64,
    },
    EvaluateName {
        id: (u32, u32),
        version: u64,
        anchor: Key,
    },
    /// F9 on a part of the formula being edited at `anchor` (dc-0f1k.7): the value of the part of
    /// `formula` from `start` for `len` UTF-16 code units, as that cell would read it.
    EvaluatePart {
        formula: String,
        anchor: Key,
        start: usize,
        len: usize,
    },
    /// The evaluation trace of the formula being edited at `anchor`: every part with its value.
    TraceFormula {
        formula: String,
        anchor: Key,
    },
    /// The registered functions the workbook can call (add-in registrations and VBA), for the
    /// formula field's completion.
    UserFunctions,
    CalculateName {
        id: (u32, u32),
        version: u64,
        anchor: Key,
    },
    PollNameCalculation {
        token: u64,
    },
    CancelNameCalculation {
        token: u64,
    },
    FormatSelection(Vec<Area>),
    PreviewFormat {
        at: Key,
        revision: u64,
        patch: crate::format::FormatPatch,
    },
    Summary,
    Command(CommandEnvelope),
    Find(FindOptions),
    SelectionStats,
    CanEdit(Key),
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
        #[serde(default)]
        transpose: bool,
    },
    /// An answer, to show the server is there and to time a round trip.
    Ping,
    /// A new, empty workbook.
    New,
    /// Opens the bytes that travel beside the message, under `name`.
    Open {
        name: String,
    },
    /// Opens the template file itself instead of creating an unsaved workbook.
    OpenForEditing {
        name: String,
    },
    /// Prepare and replace the workbook's live VBA procedures. Source is not persisted here.
    LoadVbaModule {
        name: String,
        source: String,
        max_steps: u64,
    },
    /// Explicit source module kinds and document identities for automatic VBA events.
    LoadVbaProject {
        modules: Vec<VbaModule>,
        max_steps: u64,
    },
    SetVbaEventsEnabled {
        enabled: bool,
    },
    /// The person enabled macros for the open workbook (the page's "Macros are disabled" bar): its
    /// own VBA project is compiled, its functions published and `Workbook_Open` raised. Refused
    /// for a file whose Mark of the Web or network origin blocks its project (dc-0f1k.22).
    EnableMacros,
    /// The page's "Unblock and enable" button, behind its warning: enables the macros of a file
    /// whose Mark of the Web or network origin blocks them (dc-15dk).
    UnblockMacros,
    /// The person chose a macro security level (dc-15dk); refused while the command line or the
    /// environment pins it. The native host stores the level per user.
    SetMacroSecurity { level: MacroSecurity },
    /// Saves the workbook: its bytes travel back beside the reply.
    Save,
    /// Ordinary live save with Excel's CalculateBeforeSave setting.
    SaveWithContext {
        calculate_before_save: bool,
        #[serde(default)]
        views: Option<SaveWindow>,
    },
    /// Uses the ordinary pinned save, with an explicit format and file name.
    SaveAs {
        name: String,
        format: FileFormat,
        views: Option<SaveWindow>,
    },
    Intent(Intent),
    /// Registers subscription `id` (the page's number for it); its cells come as events.
    Subscribe {
        workbook: u64,
        id: u64,
        interest: Interest,
    },
    SetInterest {
        workbook: u64,
        id: u64,
        interest: Interest,
    },
    Unsubscribe {
        workbook: u64,
        id: u64,
    },
    /// Release the one in-flight publication after the mirror applies its completion barrier.
    TileAck(crate::tiles::TileBarrier),
    /// Explicit recovery; the server coalesces all missed deltas into one atomic reset.
    ResetTiles {
        workbook: u64,
        id: u64,
    },
    Review(Key),
    ReviewMarkers(Sheet),
    /// A sheet's sizes, as the grid lays it out.
    Geometry(Sheet),
    UsedArea(Sheet),
    ResolveReference {
        anchor: Key,
        text: String,
    },
    /// The sheet's charts, drawn.
    Charts(Sheet),
    PivotTables(Sheet),
    PivotSourceFields {
        area: Area,
        revision: u64,
    },
    /// The cell's data validation: its rule and, for a list, its items.
    Validation(Key),
    ValidationInvalid(Sheet),
    /// The atlas: the census and the session's state.
    Atlas,
    /// A bounded page of cells with this exact projected refusal reason.
    AtlasRefusals {
        reason: String,
        page: u64,
    },
    /// The tree's outline: each sheet's first rows (`shown`, per sheet; [`crate::tree::ROW_PAGE`]
    /// for a sheet not named) and the cells of the rows of the `open` sheets.
    Outline {
        shown: Vec<(Sheet, u64)>,
        open: Vec<Sheet>,
        reveal: Option<Key>,
        query: String,
    },
    Notebook {
        sheet: Sheet,
        page: u64,
        reveal: Option<Key>,
        placement: Option<String>,
    },
    BlockDetail(Area),
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
    /// Calculate a pinned workbook image before saving; poll its token for the bytes.
    SaveCalculated,
    PollSave {
        token: u64,
    },
    CancelSave {
        token: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VbaModuleTarget {
    Standard,
    Worksheet(Sheet),
    Workbook,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VbaModule {
    pub name: String,
    pub source: String,
    pub target: VbaModuleTarget,
}

impl Request {
    pub fn is_command(&self) -> bool {
        matches!(
            self,
            Self::OpenTemplate { .. }
                | Self::InsertTemplate { .. }
                | Self::EvaluateName { .. }
                | Self::CalculateName { .. }
                | Self::PollNameCalculation { .. }
                | Self::CancelNameCalculation { .. }
                | Self::New
                | Self::Open { .. }
                | Self::OpenForEditing { .. }
                | Self::SaveAs { .. }
                | Self::Save
                | Self::SaveWithContext { .. }
                | Self::SaveCalculated
                | Self::PollSave { .. }
                | Self::CancelSave { .. }
                | Self::Intent(_)
                | Self::LoadVbaModule { .. }
                | Self::LoadVbaProject { .. }
                | Self::SetVbaEventsEnabled { .. }
                | Self::EnableMacros
                | Self::UnblockMacros
                | Self::SetMacroSecurity { .. }
        )
    }
}

/// Whether an open workbook's own VBA project runs, as Excel's default has it: a macro workbook
/// opens with its project held and saved back but not run until the person enables macros for it;
/// a file with the Mark of the Web (zone 3 or 4) or from the network cannot be enabled.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MacroStatus {
    /// The workbook has no VBA project.
    #[default]
    None,
    /// Held, not compiled, not run; [`Request::EnableMacros`] lets it run.
    Disabled,
    /// Held, not run, and not to be enabled; `reason` says why, in the bar's words.
    Blocked { reason: String },
    /// The person enabled macros: the project compiled and its events run.
    Enabled,
}

/// The workbook as the page's shell shows it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub session: String,
    #[serde(default)]
    pub initial_cell: Option<Key>,
    pub workbook: u64,
    pub name: String,
    pub changed: bool,
    #[serde(default)]
    pub unsaved: bool,
    #[serde(default)]
    pub macro_enabled: bool,
    /// Whether the workbook's own VBA project runs (dc-0f1k.22).
    #[serde(default)]
    pub macros: MacroStatus,
    /// The macro security level in force, and what pins it, always shown on the page (dc-15dk).
    #[serde(default)]
    pub macro_policy: MacroPolicy,
    /// An information notice about the project's macros (a marked or network file that ran
    /// because the level is Unguarded, or was unblocked); empty for none.
    #[serde(default)]
    pub macro_note: String,
    #[serde(default)]
    pub template: bool,
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

impl Summary {
    /// Every known worksheet in workbook order, including Hidden and VeryHidden scopes.
    pub fn all_sheets(&self) -> Vec<SheetTab> {
        let mut seen = std::collections::BTreeSet::new();
        let mut rows = Vec::new();
        for id in self
            .sheet_order
            .iter()
            .map(|s| s.id())
            .chain(self.sheets.iter().map(|(s, _)| s.id()))
            .chain(self.hidden_sheets.iter().map(|(s, _, _)| s.id()))
        {
            if seen.insert(id) {
                if let Some(name) = crate::address::SheetNames::sheet_name_of(self, id) {
                    rows.push(SheetTab { id, name });
                }
            }
        }
        rows
    }
}

/// Supported authored SpreadsheetML file kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileFormat {
    Xlsx,
    Xlsm,
    Xltx,
    Xltm,
}
impl FileFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Xlsx => "xlsx",
            Self::Xlsm => "xlsm",
            Self::Xltx => "xltx",
            Self::Xltm => "xltm",
        }
    }
}

/// What an intent (or an open, a new workbook, a save) did.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notebook_placement: Option<String>,
    pub validation: Option<ValidationDecision>,
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

/// Immutable UI viewport positions captured by one save request, in zoom-scaled pixels before browser scroll-surface compression.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SaveWindow {
    pub revision: u64,
    pub active_sheet: Sheet,
    pub sheets: Vec<SheetWindow>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SheetWindow {
    pub geometry: Geometry,
    pub sheet: Sheet,
    pub active: (u32, u32),
    pub active_pane: u8,
    pub panes: Vec<PaneWindow>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaneWindow {
    pub index: u8,
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

/// A sheet's sizes as the file states them (calc-host's sheet geometry).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutlineChange {
    Group {
        first: u32,
        last: u32,
    },
    Ungroup {
        first: u32,
        last: u32,
    },
    Detail {
        first: u32,
        last: u32,
        level: Option<u8>,
        show: bool,
    },
    ShowLevel(u8),
    SummaryAfter(bool),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineGroup {
    pub rows: bool,
    pub first: u32,
    pub last: u32,
    pub level: u8,
    pub summary: u32,
    pub collapsed: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    #[serde(default)]
    pub outline: Vec<OutlineGroup>,
    #[serde(default = "outline_after")]
    pub summary_below: bool,
    #[serde(default = "outline_after")]
    pub summary_right: bool,
    #[serde(default)]
    pub auto_filter: Option<FilterView>,
    #[serde(default)]
    pub tables: Vec<TableView>,
    /// Areas whose absent cells are known empty in this committed subscription root.
    #[serde(default)]
    pub coverage: Vec<Area>,
    /// Imported merges, transformed by authored structural edits, in this publication.
    #[serde(default)]
    pub merges: Vec<Area>,
    /// The same sheet preflight refusal returned by CanEdit. Commit validation remains separate.
    #[serde(default)]
    pub edit_refusal: Option<String>,
    #[serde(default)]
    pub zoom: Option<u16>,
    pub top_left: Option<(u32, u32)>,
    pub pane_top_left: Option<(u32, u32)>,
    pub active_cell: Option<(u32, u32)>,
    pub active_pane: u8,
    pub split: Option<(u64, u64)>,
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

fn outline_after() -> bool {
    true
}

impl Geometry {
    pub fn merge_at(&self, at: CellKey) -> Option<GridArea> {
        self.merges
            .iter()
            .filter_map(|a| a.area())
            .find(|a| a.contains(at))
    }
    pub fn anchor(&self, at: CellKey) -> CellKey {
        self.merge_at(at).map_or(at, |a| a.first())
    }

    /// A geometry snapshot is already neutral and can be used for physical layout directly.
    pub fn geometry(&self) -> Self {
        self.clone()
    }
}

/// A chart as the grid places it: its frame's corners (cells and offsets in pixels at 100%), or
/// its position and size, and its drawing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chart {
    #[serde(default)]
    pub text_style: ChartTextStyle,
    #[serde(default)]
    pub text_elements: Vec<ChartTextElement>,
    #[serde(default)]
    pub plots: Vec<ChartPlotSource>,
    #[serde(default)]
    pub axes: Vec<ChartAxisSource>,
    #[serde(default)]
    pub part: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub legend: Option<String>,
    #[serde(default)]
    pub title_linked: bool,
    pub name: String,
    pub kind: String,
    pub place: ChartPlace,
    /// The chart's SVG, for a frame of the size `place` gives at 100%.
    pub svg: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartAxisSource {
    pub id: u32,
    pub position: String,
    pub minimum: Option<String>,
    pub maximum: Option<String>,
    pub major_unit: Option<String>,
    pub reversed: bool,
    pub logarithmic: bool,
    #[serde(default)]
    pub log_base: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartPlotSource {
    pub kind: String,
    #[serde(default)]
    pub bar_direction: Option<String>,
    #[serde(default)]
    pub grouping: Option<String>,
    pub series: Vec<ChartSeriesSource>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartSeriesSource {
    #[serde(default)]
    pub fill: Option<String>,
    pub index: u32,
    pub label: String,
    /// A literal name, or a cell reference prefixed by '='.
    pub name: String,
    pub categories: Option<String>,
    pub values: Option<String>,
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

/// Current validation audit, including unavailable criteria and an explicit scan limit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationScan {
    pub sheet: Sheet,
    pub revision: u64,
    pub invalid: Vec<Key>,
    pub checked: u32,
    pub valid: u32,
    pub unavailable: u32,
    pub truncated: bool,
    pub reasons: Vec<(String, u32)>,
}

/// A rejected entry awaiting an explicit user decision; the worker holds the draft command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationDecision {
    pub token: u64,
    pub style: String,
    pub title: String,
    pub message: String,
}

/// What the server answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteMode {
    All,
    Values,
    Formulas,
    Formats,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteSource {
    Internal {
        token: u64,
        cut: bool,
    },
    External(Vec<Vec<String>>),
    FlashFill,
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
    #[serde(default)]
    pub samples: Vec<String>,
    pub id: u64,
    pub area: Area,
    pub replaces: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    CellImage(Option<CellImage>),
    PageSetup(Result<crate::printing::PageSetupState, String>),
    PrintPreview(Result<crate::printing::PrintDocument, String>),
    TemplateFormat(FileFormat),
    FilterValues {
        revision: u64,
        values: Result<(Vec<(String, bool)>, bool), String>,
        criteria: Option<FilterChange>,
    },
    Names(crate::names::NamePage),
    NameEvaluation(crate::names::NameEvaluation),
    Part(Result<crate::parts::PartValue, String>),
    Trace(crate::parts::Trace),
    UserFunctions(Vec<String>),
    NameCalculation(crate::names::NameCalculationReply),
    FormatSelection(Result<crate::format::FormatSelection, String>),
    FormatPreview(Result<Cell, String>),
    Summary(Summary),
    Found(Result<Found, String>),
    SelectionStats(Result<SelectionStats, String>),
    CanEdit(Result<(), String>),
    Review(Review),
    ReviewMarkers(Vec<(Key, String)>),
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
    PivotTables {
        tables: Vec<SavedPivot>,
        issues: Vec<String>,
    },
    PivotSourceFields(Result<Vec<String>, String>),
    Validation(Option<Validation>),
    ValidationInvalid(ValidationScan),
    Atlas(Atlas),
    AtlasRefusals(RefusalCells),
    Outline(Outline),
    Notebook(Notebook),
    BlockDetail(BlockDetail),
    Keys(Vec<Key>),
    Canvas(Canvas),
    Ledger(Ledger),
    SavePending {
        token: u64,
        waiting: bool,
    },
    SaveImage {
        outcome: Outcome,
        saved: String,
        model_revision: u64,
    },
}

/// One page of the ledger: how many formula cells its scope holds, how many the filter keeps,
/// and the page's rows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    pub whole: u64,
    pub page_index: u64,
    pub active_page: Option<u64>,
    pub keys: u64,
    pub rows: u64,
    pub page: Vec<LedgerRow>,
}

/// One formula cell as the ledger's row shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LedgerRow {
    pub reason: String,
    pub error: bool,
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
    pub reference: String,
    pub sheet: Sheet,
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
    /// Successful calculation metadata belongs in the optional details.
    pub reason_is_detail: bool,
    pub format: String,
    pub reads: Vec<String>,
    pub read_by: Vec<String>,
}

/// The tree's outline: the workbook's name, its defined names (`Name = formula`), its sheets
/// (each with its cell count, its first rows' cells and how many rows follow) and the cells of
/// the open sheets' rows, as the page shows them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Outline {
    pub omitted: u64,
    pub workbook: String,
    pub names: Vec<String>,
    /// The registered functions (add-ins) with what their providers documented of them: the
    /// formula field completes them and shows their signature and help.
    #[serde(default)]
    pub functions: Vec<FunctionDoc>,
    pub sheets: Vec<OutlineSheet>,
    pub cells: Vec<Cell>,
}

/// A registered function as its provider documented it (`xlfRegister`'s names and help text).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FunctionDoc {
    pub name: String,
    /// Where it comes from, for people (`xll Name.xll`).
    pub origin: String,
    pub category: String,
    pub description: String,
    /// Each parameter's name and its help text.
    pub parameters: Vec<(String, String)>,
    pub volatile: bool,
    pub thread_safe: bool,
    pub asynchronous: bool,
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
    pub census: Census,
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

/// One page of actual refused formula cells, in workbook address order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RefusalCells {
    pub total: usize,
    pub page: usize,
    pub cells: Vec<(Key, String)>,
}

/// One cell of a subscription, as the page shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    #[serde(default)]
    pub style: TextStyle,
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
    /// The cell as the grid draws it.
    pub fn shown(&self) -> Option<CellShown> {
        Some(CellShown {
            style: self.style.clone(),
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

/// What the server tells the page without being asked. Controls remain JSON; tile batches
/// use the binary codec on worker transports.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    WorkbookEventFailed {
        workbook: u64,
        sequence: u64,
        event: String,
        reason: String,
        omitted: u64,
    },
    SaveReady {
        workbook: u64,
        token: u64,
    },
    TileAdmission(crate::tiles::TileStamp),
    Tiles(crate::tiles::TileBatch),
    TileCompletion(crate::tiles::TileBarrier),
    ProjectionFailed {
        id: u64,
        reason: String,
    },
    Calculation {
        workbook: u64,
        receipt: CalculationEvent,
        summary: Option<Summary>,
        #[serde(default)]
        failed_projections: Vec<u64>,
    },
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindScope {
    Sheet,
    Selection,
    Workbook,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindOptions {
    pub sheet: Sheet,
    pub areas: Vec<Area>,
    pub text: String,
    pub case_sensitive: bool,
    pub whole: bool,
    pub wildcards: bool,
    pub formulas: bool,
    pub scope: FindScope,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    pub revision: u64,
    pub rows: Vec<FoundRow>,
    pub scanned: u64,
    pub partial: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundRow {
    pub at: Key,
    pub label: String,
    pub source: String,
    pub display: String,
    pub provenance: Provenance,
    pub hidden: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionStats {
    pub selected: u64,
    pub count: u64,
    pub numeric: u64,
    pub literal: u64,
    pub calculated: u64,
    pub blank: u64,
    pub empty_text: u64,
    pub errors: u64,
    pub cached: u64,
    pub stale: u64,
    pub refused: u64,
    pub sum: String,
    pub average: String,
    pub min: String,
    pub max: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Notebook {
    pub total: u64,
    pub page: u64,
    pub omitted: u64,
    pub blocks: Vec<NotebookBlock>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NotebookBlock {
    pub placement: Option<String>,
    pub anchor: Option<Area>,
    pub prose: bool,
    pub cell: Option<Cell>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlockDetail {
    pub summary: String,
    pub anchor: Option<Key>,
    pub cells: Vec<Cell>,
    pub blockers: Vec<Area>,
}

/// One sheet as the skins list it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetTab {
    pub id: SheetId,
    pub name: String,
}

/// Initial notebook projection page size.
pub const NOTEBOOK_PAGE: usize = 40;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Publication {
    pub domain: u64,
    pub command: u64,
    pub sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalculationScope {
    Workbook,
    Sheet(Sheet),
    Range(Vec<Area>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalculationSelection {
    DirtyAndVolatile,
    ForceAll,
    FormulaEntry,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalculationOutcome {
    Complete,
    Incomplete,
    Superseded,
    Cancelled,
    Failed,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalculationRangePostState {
    #[default]
    Satisfied,
    StillRequired,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalculationEvent {
    Reevaluated {
        domain: u64,
        command: u64,
        pass: u32,
        at: Key,
        evaluation: u64,
        reason: String,
        previous_calls: Vec<u64>,
    },
    AwaitingAsync {
        domain: u64,
        command: u64,
        pass: u32,
        token: u64,
        at: Key,
        request: u64,
    },
    AsyncReturned {
        token: u64,
        disposition: AsyncReturnDisposition,
    },
    Committed {
        revision: u64,
    },
    Admitted {
        domain: u64,
        command: u64,
        pass: u32,
        revision: u64,
        scope: CalculationScope,
        selection: CalculationSelection,
        #[serde(default)]
        range_post_state: CalculationRangePostState,
        predecessor: Option<u64>,
    },
    TargetsRefused {
        domain: u64,
        command: u64,
        targets: Vec<Key>,
        construct: String,
        owner: String,
    },
    Yielded {
        domain: u64,
        command: u64,
        pass: u32,
        remaining: usize,
    },
    Superseded {
        domain: u64,
        command: u64,
        by: u64,
        revision: u64,
    },
    Reused {
        at: Key,
        computed_by: u64,
        certified_for: u64,
    },
    Rejected {
        at: Key,
    },
    Published(Publication),
    Completed {
        domain: u64,
        command: u64,
        revision: u64,
        outcome: CalculationOutcome,
        required: usize,
        calculated: usize,
        refused: usize,
        held: usize,
        uncertified: usize,
        #[serde(default)]
        boundary_cached: usize,
        #[serde(default)]
        imported_cached: usize,
    },
}

/// Transport projection of a checked provider return, without exposing publication authority.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AsyncReturnDisposition {
    Accepted,
    Duplicate,
    Rejected(String),
}

/// Resolved colours and geometry of a physical fill. Both painters consume this projection.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FillShown {
    Pattern {
        name: String,
        foreground: String,
        background: String,
    },
    Gradient {
        kind: String,
        degree: f64,
        rectangle: [f64; 4],
        stops: Vec<(f64, String)>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewFormat {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub color: Option<String>,
    pub clear: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Option<String>,
    pub size: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartTextStyle {
    pub family: Option<String>,
    pub size: Option<u32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub color: Option<String>,
}

/// Metadata for a saved pivot; its cells are presented through the normal worksheet route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedPivot {
    pub refreshed_revision: Option<u64>,
    pub name: String,
    pub part: String,
    pub area: Area,
    pub location: String,
    pub source: String,
    pub fields: String,
    pub cache: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartTextElement {
    pub target: String,
    pub label: String,
    pub style: ChartTextStyle,
}
