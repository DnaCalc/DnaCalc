//! Workbook operations issued by the page. This module translates typed targets into the
//! existing wire intent before the controller admits it to the command queue.

use crate::wire::{
    Area, ChartTextStyle, ConditionalChange, FilterChange, FindOptions, FindScope, Intent, Key,
    OutlineChange, Request, ReviewFormat, Sheet, SheetChange, SpecialCellKind, TableChange,
};
use calc_value::{CellKey, GridArea};
use dna_calc_model::Target;

// These declarations give each operation its page payload and target conversion together.
macro_rules! workbook_commands {
    ($command:ident => $wire:ident {
        $($variant:ident $({$($field:ident: $ty:ty $(=> $convert:path)?,)*})?,)*
    }) => {
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum $command {
            $($variant $({$($field: $ty,)*})?,)*
        }

        impl $command {
            fn wire(self) -> Result<$wire, &'static str> {
                Ok(match self {
                    $(Self::$variant $({$($field,)*})? => $wire::$variant $({
                        $($field: workbook_commands!(@convert $field $(=> $convert)?),)*
                    })?,)*
                })
            }
        }
    };
    (@convert $field:ident) => { $field };
    (@convert $field:ident => $convert:path) => { $convert($field)? };
}

workbook_commands! { WorkbookCommand => Intent {
    RefreshAll {
        revision: u64,
    },
    PivotCreate {
        revision: u64,
        source: Target => wire_area,
        destination: String,
        row: u32,
        value: u32,
        subtotal: String,
    },
    PivotCreateLayout {
        revision: u64,
        source: Target => wire_area,
        destination: String,
        rows: Vec<u32>,
        columns: Vec<u32>,
        values: Vec<(u32, String)>,
    },
    PivotSetLayout {
        sheet: Target => wire_sheet,
        revision: u64,
        part: String,
        rows: Vec<i32>,
        columns: Vec<i32>,
    },
    PivotRefresh {
        sheet: Target => wire_sheet,
        revision: u64,
        part: String,
    },
    PageSetup {
        sheet: Target => wire_sheet,
        revision: u64,
        change: dna_calc_model::printing::PageSetupChange,
    },
    ChartCreate {
        revision: u64,
        area: Target => wire_area,
        kind: String,
        rows: bool,
        headers: bool,
        categories: bool,
    },
    ChartSeriesStyle {
        sheet: Target => wire_sheet,
        revision: u64,
        part: String,
        plot: u32,
        series: u32,
        fill: Option<String>,
    },
    ChartElementTextStyle {
        sheet: Target => wire_sheet,
        revision: u64,
        part: String,
        target: String,
        style: ChartTextStyle,
    },
    ChartTextStyle {
        sheet: Target => wire_sheet,
        revision: u64,
        part: String,
        style: ChartTextStyle,
    },
    ChartType {
        sheet: Target => wire_sheet,
        revision: u64,
        part: String,
        plot: u32,
        kind: String,
        stacked: bool,
    },
    ChartLegend {
        sheet: Target => wire_sheet,
        revision: u64,
        part: String,
        position: String,
    },
    ShapePlace {
        sheet: Target => wire_sheet,
        revision: u64,
        id: u32,
        left: i64,
        top: i64,
        width: i64,
        height: i64,
    },
    ShapeDelete {
        sheet: Target => wire_sheet,
        revision: u64,
        id: u32,
    },
    ShapeAdd {
        sheet: Target => wire_sheet,
        revision: u64,
        at: Target => wire_key,
        kind: String,
        geometry: String,
        width: i64,
        height: i64,
    },
    SparklineAdd {
        sheet: Target => wire_sheet,
        revision: u64,
        at: Target => wire_key,
        data: String,
        kind: String,
    },
    SparklineKind {
        sheet: Target => wire_sheet,
        revision: u64,
        at: Target => wire_key,
        kind: String,
    },
    SparklineShow {
        sheet: Target => wire_sheet,
        revision: u64,
        at: Target => wire_key,
        point: String,
        on: bool,
    },
    SparklineDelete {
        sheet: Target => wire_sheet,
        revision: u64,
        at: Target => wire_key,
    },
    ShapePicture {
        sheet: Target => wire_sheet,
        revision: u64,
        at: Target => wire_key,
        name: String,
    },
    ShapeText {
        sheet: Target => wire_sheet,
        revision: u64,
        id: u32,
        text: String,
    },
    ShapeFill {
        sheet: Target => wire_sheet,
        revision: u64,
        id: u32,
        fill: String,
    },
    ShapeLine {
        sheet: Target => wire_sheet,
        revision: u64,
        id: u32,
        colour: String,
        weight: Option<i64>,
    },
    ShapeRotate {
        sheet: Target => wire_sheet,
        revision: u64,
        id: u32,
        degrees: i64,
    },
    ShapeDuplicate {
        sheet: Target => wire_sheet,
        revision: u64,
        id: u32,
        dx: i64,
        dy: i64,
    },
    ShapeNudge {
        sheet: Target => wire_sheet,
        revision: u64,
        id: u32,
        dx: i64,
        dy: i64,
    },
    ShapeFlip {
        sheet: Target => wire_sheet,
        revision: u64,
        id: u32,
        horizontal: bool,
    },
    ShapeOrder {
        sheet: Target => wire_sheet,
        revision: u64,
        id: u32,
        order: String,
    },
    ShapeGroup {
        sheet: Target => wire_sheet,
        revision: u64,
        ids: Vec<u32>,
    },
    ShapeUngroup {
        sheet: Target => wire_sheet,
        revision: u64,
        id: u32,
    },
    ChartLogAxis {
        sheet: Target => wire_sheet,
        revision: u64,
        part: String,
        axis: u32,
        base: Option<String>,
    },
    ChartAxis {
        sheet: Target => wire_sheet,
        revision: u64,
        part: String,
        axis: u32,
        minimum: Option<String>,
        maximum: Option<String>,
        major_unit: Option<String>,
        reversed: bool,
    },
    ChartSeries {
        sheet: Target => wire_sheet,
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
        sheet: Target => wire_sheet,
        revision: u64,
        part: String,
        title: Option<String>,
        set_title: bool,
        legend: Option<bool>,
    },
    CreateReview {
        at: Target => wire_key,
        revision: u64,
        parent: Option<u32>,
        threaded: bool,
        author: String,
        text: String,
        timestamp: String,
    },
    FormatReview {
        at: Target => wire_key,
        revision: u64,
        index: u32,
        text: String,
        start: u32,
        end: u32,
        patch: ReviewFormat,
    },
    ResolveReview {
        at: Target => wire_key,
        revision: u64,
        index: u32,
        resolved: bool,
    },
    EditReview {
        at: Target => wire_key,
        revision: u64,
        index: u32,
        text: String,
    },
    EditName {
        id: Option<Target> => optional_name,
        version: Option<u64>,
        name: String,
        scope: Option<Target> => optional_sheet,
        formula: Option<String>,
    },
    FormatCells {
        areas: Target => wire_areas,
        revision: u64,
        patch: dna_calc_model::format::FormatPatch,
    },
    NotebookLink {
        anchor: Target => wire_area,
        prose: bool,
        text: Option<String>,
    },
    NotebookOrder {
        placement: Target => wire_placement,
        direction: i8,
    },
    Replace {
        options: FindCommandOptions => FindCommandOptions::wire,
        replacement: String,
        revision: u64,
        only: Option<Target> => optional_key,
    },
    EditConditional {
        sheet: Target => wire_sheet,
        revision: u64,
        index: Option<u32>,
        change: ConditionalChange,
    },
    AcceptValidation {
        token: u64,
    },
    ShiftCells {
        area: Target => wire_area,
        down: bool,
        insert: bool,
    },
    EditSheet {
        sheet: Option<Target> => optional_sheet,
        change: SheetChange,
    },
    FitContents {
        sheet: Target => wire_sheet,
        rows: bool,
        revision: u64,
        sizes: Vec<(u32, u64)>,
    },
    Outline {
        sheet: Target => wire_sheet,
        rows: bool,
        revision: u64,
        change: OutlineChange,
    },
    AxisFormat {
        sheet: Target => wire_sheet,
        rows: bool,
        first: u32,
        count: u32,
        size_bits: Option<u64>,
        hidden: Option<bool>,
    },
    SplitPanes {
        sheet: Target => wire_sheet,
        revision: u64,
        x: u64,
        y: u64,
        row: u32,
        column: u32,
    },
    FreezePanes {
        sheet: Target => wire_sheet,
        rows: u32,
        columns: u32,
    },
    Table {
        revision: u64,
        change: TableCommand => TableCommand::wire,
    },
    AutoFilter {
        sheet: Target => wire_sheet,
        revision: u64,
        table: Option<String>,
        change: FilterChange,
    },
    SortRowsWithOptions {
        area: Target => wire_area,
        keys: Vec<(u32, bool)>,
        header: bool,
        revision: u64,
        case_sensitive: bool,
        custom_order: Vec<String>,
    },
    SortRows {
        area: Target => wire_area,
        keys: Vec<(u32, bool)>,
        header: bool,
        revision: u64,
    },
    ApplyPaste {
        id: u64,
        select_target: bool,
    },
    EnterCell {
        at: Target => wire_key,
        text: String,
    },
    EnterSelection {
        text: String,
    },
    Clear {
        areas: Target => wire_areas,
    },
    Navigate {
        nav: dna_calc_model::selection::Navigation,
        extend: bool,
        page_height: u32,
    },
    EditAxis {
        sheet: Target => wire_sheet,
        rows: bool,
        insert: bool,
        start: u32,
        count: u32,
    },
    Copy {
        from: Target => wire_area,
        to: Target => wire_area,
        cut: bool,
    },
    Undo,
    Redo,
    Recalculate,
    SetCalcMode {
        manual: bool,
    },
    SetIteration {
        enabled: bool,
        max_iterations: u32,
        max_change_bits: u64,
    },
    Select {
        active: Target => wire_key,
        areas: Target => wire_areas,
    },
} }

workbook_commands! { TableCommand => TableChange {
    Create {
        area: Target => wire_area,
        name: String,
    },
    Resize {
        part: String,
        area: Target => wire_area,
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
} }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindCommandOptions {
    pub sheet: Target,
    pub areas: Target,
    pub text: String,
    pub case_sensitive: bool,
    pub whole: bool,
    pub wildcards: bool,
    pub formulas: bool,
    pub scope: FindScope,
}

impl FindCommandOptions {
    fn wire(self) -> Result<FindOptions, &'static str> {
        Ok(FindOptions {
            sheet: wire_sheet(self.sheet)?,
            areas: wire_areas(self.areas)?,
            text: self.text,
            case_sensitive: self.case_sensitive,
            whole: self.whole,
            wildcards: self.wildcards,
            formulas: self.formulas,
            scope: self.scope,
        })
    }

    pub(crate) fn find(self) -> Result<Request, &'static str> {
        self.wire().map(Request::Find)
    }
}

pub(crate) fn go_to_special(
    sheet: Target,
    areas: Target,
    kind: SpecialCellKind,
) -> Result<Request, &'static str> {
    Ok(Request::GoToSpecial {
        sheet: wire_sheet(sheet)?,
        areas: wire_areas(areas)?,
        kind,
    })
}

impl WorkbookCommand {
    pub(crate) fn request(self) -> Result<Request, &'static str> {
        self.wire().map(Request::Intent)
    }

    pub(crate) fn moves_selection(&self) -> bool {
        matches!(
            self,
            Self::Select { .. } | Self::Navigate { .. } | Self::Undo | Self::Redo
        )
    }

    pub(crate) fn edits_workbook(&self) -> bool {
        !matches!(self, Self::Select { .. } | Self::Navigate { .. })
    }
}

pub(crate) fn cell_target(at: CellKey) -> Target {
    area_target(GridArea::cell(at))
}
pub(crate) fn area_target(area: GridArea) -> Target {
    Target::Cells(vec![area])
}
pub(crate) fn projected_cell(at: Key) -> Target {
    Target::Cells(at.cell().map(GridArea::cell).into_iter().collect())
}
pub(crate) fn projected_area(area: Area) -> Target {
    Target::Cells(area.area().into_iter().collect())
}

pub(crate) fn target_cell(target: &Target) -> Option<CellKey> {
    let area = target_area(target)?;
    (area.first() == area.last()).then(|| area.first())
}

pub(crate) fn target_area(target: &Target) -> Option<GridArea> {
    match target {
        Target::Cells(areas) if areas.len() == 1 => Some(areas[0]),
        _ => None,
    }
}

fn wire_key(target: Target) -> Result<Key, &'static str> {
    target_cell(&target)
        .map(Key::of)
        .ok_or("This command needs one cell")
}

fn wire_area(target: Target) -> Result<Area, &'static str> {
    target_area(&target)
        .map(Area::of)
        .ok_or("This command needs one range")
}

fn wire_areas(target: Target) -> Result<Vec<Area>, &'static str> {
    match target {
        Target::Cells(areas) => Ok(areas.into_iter().map(Area::of).collect()),
        _ => Err("This command needs cell ranges"),
    }
}
fn wire_sheet(target: Target) -> Result<Sheet, &'static str> {
    match target {
        Target::Sheet(sheet) => Ok(Sheet::of(sheet)),
        _ => Err("This command needs a sheet"),
    }
}
fn wire_name(target: Target) -> Result<(u32, u32), &'static str> {
    match target {
        Target::Name(id) => Ok((id.slot(), id.generation())),
        _ => Err("This command needs a defined name"),
    }
}
fn wire_placement(target: Target) -> Result<String, &'static str> {
    match target {
        Target::LayoutItem(id) => Ok(id),
        _ => Err("This command needs a notebook placement"),
    }
}

fn optional_sheet(target: Option<Target>) -> Result<Option<Sheet>, &'static str> {
    target.map(wire_sheet).transpose()
}

fn optional_key(target: Option<Target>) -> Result<Option<Key>, &'static str> {
    target.map(wire_key).transpose()
}

fn optional_name(target: Option<Target>) -> Result<Option<(u32, u32)>, &'static str> {
    target.map(wire_name).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_value::{NameId, SheetId};

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn commands_keep_target_identities_in_wire_order() {
        let sheet = SheetId::from_parts(9, 3);
        let active = CellKey::new(sheet, 8, 12).unwrap();
        let other = CellKey::new(sheet, 2, 4).unwrap();
        let request = WorkbookCommand::Select {
            active: cell_target(active),
            areas: Target::Cells(vec![GridArea::cell(active), GridArea::cell(other)]),
        };
        assert_eq!(
            crate::wire::encode(&request.request().unwrap()),
            r#"{"Intent":{"Select":{"active":[9,3,8,12],"areas":[[[9,3,8,12],[9,3,8,12]],[[9,3,2,4],[9,3,2,4]]]}}}"#
        );
        let bad_target = WorkbookCommand::EnterCell {
            at: Target::Name(NameId::from_parts(12, 4)),
            text: "x".to_owned(),
        };
        assert_eq!(bad_target.request(), Err("This command needs one cell"));
    }
}
