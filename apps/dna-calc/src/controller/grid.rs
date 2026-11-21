//! Grid gestures and pane state, using the controller's workbook session and command queue.
use super::*;
use dna_calc_model::{GridState, viewport::GridView};
use std::ops::Deref;

#[derive(Clone, Copy)]
pub(crate) struct GridController {
    core: Controller,
    grid_views: ModelField<BTreeMap<(SheetId, u8), GridView>>,
    grid_active: ModelField<BTreeMap<SheetId, u8>>,
    grid_geometry: ModelField<Snapshot<BTreeMap<SheetId, wire::Geometry>>>,
}

impl Deref for GridController {
    type Target = Controller;

    fn deref(&self) -> &Controller {
        &self.core
    }
}

impl GridController {
    pub(crate) fn new(core: Controller) -> Self {
        // A save from any skin includes the workbook windows remembered by the grid.
        core.sheet_windows.set_value(Some(Self::sheet_windows));
        let model = core.model;
        Self {
            core,
            grid_views: model_view_field::<GridState, _>(
                model,
                SkinId::Grid,
                |s| &s.views,
                |s| &mut s.views,
            ),
            grid_active: model_view_field::<GridState, _>(
                model,
                SkinId::Grid,
                |s| &s.active,
                |s| &mut s.active,
            ),
            grid_geometry: Field::new(
                model,
                |m| &m.runtime.grid_geometry,
                |m| &mut m.runtime.grid_geometry,
            ),
        }
    }

    pub(crate) fn core(self) -> Controller {
        self.core
    }

    fn sheet_windows(controller: Controller, active_sheet: SheetId) -> Vec<wire::SheetWindow> {
        controller.model.with_untracked(|model| {
            let Some(grid) = model.views.get::<GridState>(SkinId::Grid) else {
                return Vec::new();
            };
            let mut grouped: BTreeMap<SheetId, Vec<wire::PaneWindow>> = BTreeMap::new();
            for (&(sheet, index), v) in &grid.views {
                grouped.entry(sheet).or_default().push(wire::PaneWindow {
                    index,
                    x: v.viewport.scroll_x,
                    y: v.viewport.scroll_y,
                    zoom: v.zoom,
                })
            }
            grouped
                .into_iter()
                .filter(|(sheet, _)| {
                    controller
                        .summary
                        .with_untracked(|all| all.sheets.iter().any(|(s, _)| s.id() == *sheet))
                })
                .filter_map(|(sheet, panes)| {
                    let (active, areas) = if sheet == active_sheet {
                        (
                            controller.active.get_untracked(),
                            controller.areas.get_untracked(),
                        )
                    } else {
                        controller
                            .sheet_selections
                            .with_value(|a| {
                                a.get(&sheet)
                                    .map(|(at, areas)| ((at.row(), at.column()), areas.clone()))
                            })
                            .unwrap_or(((1, 1), Vec::new()))
                    };
                    // One area larger than a cell with the active cell at a corner is stored
                    // as the sheet's selection; several areas keep only the active cell.
                    let selection = match areas.as_slice() {
                        [area] if area.row_count() * area.column_count() > 1 => {
                            let (r, c) = (area.first().row(), area.first().column());
                            let (b, d) = (area.last().row(), area.last().column());
                            ((active.0 == r || active.0 == b) && (active.1 == c || active.1 == d))
                                .then_some((r, c, b, d))
                        }
                        _ => None,
                    };
                    Some(wire::SheetWindow {
                        geometry: model.runtime.grid_geometry.get(&sheet)?.clone(),
                        sheet: wire::Sheet::of(sheet),
                        active,
                        selection,
                        active_pane: grid.active.get(&sheet).copied().unwrap_or(0),
                        panes,
                    })
                })
                .collect()
        })
    }

    pub fn grid_view_for(&self, sheet: SheetId, index: u8) -> Option<GridView> {
        self.grid_views
            .with_value(|all| all.get(&(sheet, index)).copied())
    }
    pub fn active_grid_pane(&self, sheet: SheetId) -> Option<u8> {
        self.grid_active.with_value(|all| all.get(&sheet).copied())
    }
    pub fn remember_active_grid_pane(&self, sheet: SheetId, index: u8) {
        self.grid_active.update_value(|all| {
            all.insert(sheet, index);
        });
    }
    pub fn outline_geometry(&self) -> Option<wire::Geometry> {
        self.grid_geometry.track();
        self.sheet.get().and_then(|sheet| self.grid_geometry(sheet))
    }
    pub fn grid_geometry(&self, sheet: SheetId) -> Option<wire::Geometry> {
        self.grid_geometry
            .with_value(|all| all.get(&sheet).cloned())
    }
    pub fn remember_grid_geometry(&self, sheet: SheetId, geometry: wire::Geometry) {
        self.grid_geometry.update_value(|all| {
            all.insert(sheet, geometry);
        });
    }
    pub fn remember_grid_view_for(&self, sheet: SheetId, index: u8, mut view: GridView) {
        if self.sheet.get_untracked() != Some(sheet) {
            if let Some(active) = self
                .sheet_selections
                .with_value(|all| all.get(&sheet).map(|(at, _)| (at.row(), at.column())))
            {
                view.active = active;
            }
        }
        self.grid_views.update_value(|all| {
            all.insert((sheet, index), view);
        });
    }
    pub fn outline(&self, rows: bool, change: wire::OutlineChange) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                controller.say("Finish or cancel the current entry before outlining", true);
                return;
            }
            let Some(sheet) = controller.sheet.get_untracked() else {
                return;
            };
            controller.dispatch(
                WorkbookCommand::Outline {
                    sheet: Target::Sheet(sheet),
                    rows,
                    revision: controller.summary.get_untracked().revision,
                    change,
                },
                move |outcome| {
                    if outcome.accepted {
                        controller.say("Outline updated", false);
                    }
                },
            );
        });
    }
    pub fn outline_selection(&self, rows: bool, action: u8) {
        let controller = *self;
        self.after_input(move || {
            let areas = controller.areas.get_untracked();
            if areas.len() != 1 {
                controller.say("Choose one rectangle before outlining", true);
                return;
            }
            let area = areas[0];
            let (first, last) = if rows {
                (area.first().row(), area.last().row())
            } else {
                (area.first().column(), area.last().column())
            };
            let change = match action {
                0 => wire::OutlineChange::Group { first, last },
                1 => wire::OutlineChange::Ungroup { first, last },
                _ => wire::OutlineChange::Detail {
                    first,
                    last,
                    level: None,
                    show: action == 3,
                },
            };
            controller.outline(rows, change);
        });
    }
    pub fn axis_format(&self, rows: bool, size: Option<String>, hidden: Option<bool>) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                controller.say(
                    "Finish or cancel the current entry before changing dimensions",
                    true,
                );
                return;
            }
            let areas = controller.areas.get_untracked();
            if areas.len() != 1 {
                controller.say("Choose one rectangle before changing dimensions", true);
                return;
            }
            let area = areas[0];
            let (first, last) = if rows {
                (area.first().row(), area.last().row())
            } else {
                (area.first().column(), area.last().column())
            };
            let size_bits = match size {
                Some(text) => match text.trim().parse::<f64>() {
                    Ok(n) => Some(n.to_bits()),
                    Err(_) => {
                        controller.say("Enter a numeric size", true);
                        return;
                    }
                },
                None => None,
            };
            controller.send_axis_format(
                area.sheet(),
                rows,
                first,
                last - first + 1,
                size_bits,
                hidden,
            );
        });
    }

    fn send_axis_format(
        &self,
        sheet: SheetId,
        rows: bool,
        first: u32,
        count: u32,
        size_bits: Option<u64>,
        hidden: Option<bool>,
    ) {
        let controller = *self;
        self.dispatch(
            WorkbookCommand::AxisFormat {
                sheet: Target::Sheet(sheet),
                rows,
                first,
                count,
                size_bits,
                hidden,
            },
            move |outcome| {
                if outcome.accepted {
                    controller.say(
                        if hidden == Some(true)
                            || size_bits.is_some_and(|b| f64::from_bits(b) == 0.0)
                        {
                            if rows {
                                "Rows hidden"
                            } else {
                                "Columns hidden"
                            }
                        } else if hidden == Some(false) {
                            if rows { "Rows shown" } else { "Columns shown" }
                        } else {
                            if rows {
                                "Row heights updated"
                            } else {
                                "Column widths updated"
                            }
                        },
                        false,
                    );
                }
            },
        );
    }

    pub fn resize_axis(&self, drag: crate::grid::axis::ResizeDrag) {
        let controller = *self;
        self.after_input(move || {
            if controller.generation.get_untracked() != drag.generation
                || controller.sheet.get_untracked() != Some(drag.sheet)
                || controller.editing.get_untracked().is_some()
            {
                return;
            }
            let areas = controller.areas.get_untracked();
            if areas.len() != 1 {
                controller.say("Choose one rectangle before resizing", true);
                return;
            }
            let (first, count) =
                crate::grid::axis::selected_bands(&areas, drag.band.rows, drag.band.index);
            controller.send_axis_format(
                drag.sheet,
                drag.band.rows,
                first,
                count,
                Some(drag.input().to_bits()),
                None,
            );
        });
    }

    pub fn autofit(&self, rows: bool) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                controller.say("Finish or cancel the entry before fitting contents", true);
                return;
            }
            let areas = controller.areas.get_untracked();
            if areas.len() != 1 {
                controller.say("Choose one rectangle before fitting contents", true);
                return;
            }
            controller.fit_contents(rows, areas[0]);
        });
    }
    pub fn autofit_header(&self, band: crate::grid::axis::BandEdge) {
        let controller = *self;
        self.after_input(move || {
            let Some(sheet) = controller.sheet.get_untracked() else {
                return;
            };
            if controller.editing.get_untracked().is_some() {
                return;
            }
            let (first, count) = crate::grid::axis::selected_bands(
                &controller.areas.get_untracked(),
                band.rows,
                band.index,
            );
            let last = first + count - 1;
            let (top, left, bottom, right) = if band.rows {
                (first, 1, last, 16384)
            } else {
                (1, first, 1048576, last)
            };
            let area = GridArea::new(
                CellKey::new(sheet, top, left).unwrap(),
                CellKey::new(sheet, bottom, right).unwrap(),
            )
            .unwrap();
            controller.fit_contents(band.rows, area);
        });
    }
    fn fit_contents(&self, rows: bool, area: GridArea) {
        let controller = *self;
        let generation = self.generation.get_untracked();
        self.pending_actions.update(|n| *n += 1);
        self.call(
            Request::FitContents {
                area: Area::of(area),
            },
            None,
            move |reply, _| {
                controller
                    .pending_actions
                    .update(|n| *n = n.saturating_sub(1));
                if controller.generation.get_untracked() != generation
                    || controller.sheet.get_untracked() != Some(area.sheet())
                {
                    return;
                }
                match reply {
                    Ok(Reply::FitContents { revision, cells }) => {
                        match crate::grid::autofit::measure(rows, &cells) {
                            Ok(sizes) if sizes.is_empty() => {
                                controller.say("No visible nonempty contents to fit", false)
                            }
                            Ok(sizes) => controller.dispatch(
                                WorkbookCommand::FitContents {
                                    sheet: Target::Sheet(area.sheet()),
                                    rows,
                                    revision,
                                    sizes,
                                },
                                move |outcome| {
                                    if outcome.accepted {
                                        controller.say("Fitted contents within size limits", false);
                                    }
                                },
                            ),
                            Err(error) => controller.say(error, true),
                        }
                    }
                    Err(error) => controller.say(error, true),
                    _ => controller.say("The workbook did not return contents to fit", true),
                }
            },
        );
    }

    pub fn table_view(&self) -> Option<wire::TableView> {
        let at = self.active_key()?;
        self.grid_geometry.with(|all| {
            all.get(&at.sheet()).and_then(|g| {
                g.tables
                    .iter()
                    .find(|t| t.area.area().is_some_and(|a| a.contains(at)))
                    .cloned()
            })
        })
    }
    pub fn filter_view(&self) -> Option<wire::FilterView> {
        if let Some(table) = self.table_view() {
            return table.filter;
        }
        let sheet = self.sheet.get()?;
        self.grid_geometry
            .with(|all| all.get(&sheet).and_then(|g| g.auto_filter.clone()))
    }
    pub fn filter_command(&self, change: wire::FilterChange) {
        self.filter_command_revision(change, None);
    }
    fn filter_command_revision(&self, change: wire::FilterChange, expected: Option<u64>) {
        let controller = *self;
        let generation = self.generation.get_untracked();
        let target = self.sheet.get_untracked();
        let table = self.table_view().map(|t| t.part);
        self.after_input(move || {
            if controller.generation.get_untracked() != generation
                || controller.sheet.get_untracked() != target
                || controller.table_view().map(|t| t.part) != table
            {
                controller.say("The filter target changed; reopen it", true);
                return;
            }
            if controller.editing.get_untracked().is_some() {
                controller.say("Finish the entry before filtering", true);
                return;
            }
            let Some(sheet) = controller.sheet.get_untracked() else {
                return;
            };
            let revision = controller.summary.get_untracked().revision;
            if expected.is_some_and(|r| r != revision) {
                controller.say(
                    "The filter draft is out of date; load its values again",
                    true,
                );
                return;
            }
            controller.dispatch(
                WorkbookCommand::AutoFilter {
                    sheet: Target::Sheet(sheet),
                    revision,
                    table,
                    change,
                },
                move |outcome| {
                    if outcome.accepted {
                        controller.model.update(|m| {
                            m.runtime.tools.filter.loaded = false;
                            m.runtime.tools.filter.epoch += 1;
                        });
                        controller.say("AutoFilter updated", false);
                    }
                },
            );
        });
    }
    pub fn enable_filter(&self) {
        let areas = self.areas.get_untracked();
        let [area] = areas.as_slice() else {
            self.say("Select one rectangle including its header", true);
            return;
        };
        self.filter_command(wire::FilterChange::Enable(Area::of(*area)));
    }
    pub fn open_filter(&self, column: u32) {
        let Some(sheet) = self.sheet.get_untracked() else {
            return;
        };
        let Some(filter) = self.filter_view() else {
            return;
        };
        let Some(area) = filter.area.area() else {
            return;
        };
        self.model.update(|m| {
            let f = &mut m.runtime.tools.filter;
            f.column = (column - area.first().column() + 1).to_string();
            f.open = true;
        });
        self.load_filter_values();
    }
    pub fn load_filter_values(&self) {
        let controller = *self;
        self.after_input(move || {
            let Some(sheet)=controller.sheet.get_untracked()else{return;};
            let Some(filter)=controller.filter_view()else{return;};
            let Some(area)=filter.area.area()else{return;};
            let relative=controller.model.with_untracked(|m|m.runtime.tools.filter.column.parse::<u32>());
            let Ok(relative)=relative else{controller.say("Enter a column number within the filtered range",true);return;};
            if relative==0 || relative>area.column_count(){controller.say("The filter column must be within its range",true);return;}
            let column=area.first().column()+relative-1;
            let generation=controller.generation.get_untracked();
            let epoch = controller
                .model
                .try_update(|m| {
                    let f = &mut m.runtime.tools.filter;
                    f.epoch += 1;
                    f.loaded = false;
                    f.message = "Loading values…".into();
                    f.epoch
                })
                .unwrap();
            let table = filter.table.clone();
            controller.call(
                Request::FilterValues { sheet: Sheet::of(sheet), table: table.clone(), column },
                None,
                move |reply, _| {
                    if controller.generation.get_untracked() != generation
                        || controller.sheet.get_untracked() != Some(sheet)
                        || controller.filter_view().and_then(|f| f.table) != table
                        || controller.model.with_untracked(|m| {
                            m.runtime.tools.filter.epoch != epoch
                                || m.runtime.tools.filter.column.parse::<u32>().ok() != Some(relative)
                        })
                    {
                        return;
                    }
                    controller.model.update(|m| {
                        let f = &mut m.runtime.tools.filter;
                        match reply {
                            Ok(Reply::FilterValues { revision, values: Ok((values, blank)), criteria })
                                if revision == m.runtime.summary.revision =>
                            {
                                f.table = table.clone();
                                f.sheet = Some(Sheet::of(sheet));
                                f.revision = revision;
                                f.values = values;
                                f.blank = blank;
                                f.operator = "equal".into();
                                f.value.clear();
                                f.operator2.clear();
                                f.value2.clear();
                                f.and = true;
                                if let Some(wire::FilterChange::Custom { and, predicates, .. }) = criteria {
                                    f.and = and;
                                    if let Some((op, v)) = predicates.first() {
                                        f.operator = op.clone();
                                        f.value = v.clone();
                                    }
                                    if let Some((op, v)) = predicates.get(1) {
                                        f.operator2 = op.clone();
                                        f.value2 = v.clone();
                                    }
                                }
                                f.loaded = true;
                                f.search.clear();
                                f.message = "Choose values, then Apply. All values in the range are listed.".into();
                            }
                            Ok(Reply::FilterValues { values: Err(e), .. }) | Err(e) => f.message = e,
                            _ => f.message = "The workbook changed; load its values again".into(),
                        }
                    });
                },
            );
        });
    }
    pub fn apply_filter(&self, custom: bool) {
        let Some(sheet) = self.sheet.get_untracked() else {
            return;
        };
        let Some(filter) = self.filter_view() else {
            return;
        };
        let Some(area) = filter.area.area() else {
            return;
        };
        let f = self
            .model
            .with_untracked(|m| m.runtime.tools.filter.clone());
        let Some(column) = f
            .column
            .parse::<u32>()
            .ok()
            .filter(|c| *c > 0 && *c <= area.column_count())
            .map(|c| area.first().column() + c - 1)
        else {
            self.say("Choose a column within the filter range", true);
            return;
        };
        let change = if custom {
            let mut predicates = vec![(f.operator, f.value)];
            if !f.operator2.is_empty() {
                predicates.push((f.operator2, f.value2));
            }
            wire::FilterChange::Custom {
                column,
                and: f.and,
                predicates,
            }
        } else {
            if !f.loaded
                || f.table != filter.table
                || f.sheet != Some(Sheet::of(sheet))
                || f.revision != self.summary.get_untracked().revision
            {
                self.say(
                    "The filter list is out of date; load its values again",
                    true,
                );
                return;
            }
            wire::FilterChange::Values {
                column,
                values: f
                    .values
                    .into_iter()
                    .filter(|(_, checked)| *checked)
                    .map(|(v, _)| v)
                    .collect(),
                blank: f.blank,
            }
        };
        self.filter_command_revision(change, Some(self.summary.get_untracked().revision));
    }
    pub fn color_filter(&self, cell_color: bool, color: String) {
        let Some(sheet) = self.sheet.get_untracked() else {
            return;
        };
        let Some(area) = self.filter_view().and_then(|f| f.area.area()) else {
            return;
        };
        if let Some(column) = self
            .model
            .with_untracked(|m| m.runtime.tools.filter.column.parse::<u32>().ok())
            .filter(|c| *c > 0 && *c <= area.column_count())
            .map(|c| area.first().column() + c - 1)
        {
            self.filter_command_revision(
                wire::FilterChange::Color {
                    column,
                    cell_color,
                    color,
                },
                Some(self.summary.get_untracked().revision),
            );
        } else {
            self.say("Choose a column within the filter range", true);
        }
    }
    /// Above or below the average of the chosen filter column, through the same command.
    pub fn average_filter(&self, above: bool) {
        let Some(area) = self.filter_view().and_then(|f| f.area.area()) else {
            return;
        };
        let Some(column) = self
            .model
            .with_untracked(|m| m.runtime.tools.filter.column.parse::<u32>().ok())
            .filter(|c| *c > 0 && *c <= area.column_count())
            .map(|c| area.first().column() + c - 1)
        else {
            self.say("Choose a column within the filter range", true);
            return;
        };
        self.filter_command_revision(
            wire::FilterChange::Average { column, above },
            Some(self.summary.get_untracked().revision),
        );
    }
    /// A date period relative to today (or a quarter or month of any year) on the chosen column.
    pub fn calendar_filter(&self, period: String) {
        let Some(area) = self.filter_view().and_then(|f| f.area.area()) else {
            return;
        };
        let Some(column) = self
            .model
            .with_untracked(|m| m.runtime.tools.filter.column.parse::<u32>().ok())
            .filter(|c| *c > 0 && *c <= area.column_count())
            .map(|c| area.first().column() + c - 1)
        else {
            self.say("Choose a column within the filter range", true);
            return;
        };
        self.filter_command_revision(
            wire::FilterChange::Calendar { column, period },
            Some(self.summary.get_untracked().revision),
        );
    }
    pub fn special_filter(
        &self,
        date: Option<String>,
        top: bool,
        percent: bool,
        value: String,
        blank: bool,
    ) {
        let Some(sheet) = self.sheet.get_untracked() else {
            return;
        };
        let Some(area) = self.filter_view().and_then(|f| f.area.area()) else {
            return;
        };
        let Some(column) = self
            .model
            .with_untracked(|m| m.runtime.tools.filter.column.parse::<u32>().ok())
            .filter(|c| *c > 0 && *c <= area.column_count())
            .map(|c| area.first().column() + c - 1)
        else {
            self.say("Choose a column within the filter range", true);
            return;
        };
        let change = if let Some(date) = date {
            let groups = date
                .lines()
                .filter(|s| !s.trim().is_empty())
                .map(|s| {
                    s.trim()
                        .split(['-', 'T', ':'])
                        .map(str::parse::<u16>)
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>();
            let Ok(groups) = groups else {
                self.say("Enter years such as 2026, months such as 2026-11, or dates such as 2026-11-03T14:30",true);
                return;
            };
            if groups.is_empty() {
                self.say("Enter at least one date group", true);
                return;
            }
            wire::FilterChange::Dates {
                column,
                groups,
                values: Vec::new(),
                blank,
            }
        } else {
            wire::FilterChange::Top {
                column,
                top,
                percent,
                value,
            }
        };
        self.filter_command_revision(change, Some(self.summary.get_untracked().revision));
    }
    pub fn clear_filter_column(&self) {
        let Some(sheet) = self.sheet.get_untracked() else {
            return;
        };
        let Some(filter) = self.filter_view() else {
            return;
        };
        let Some(area) = filter.area.area() else {
            return;
        };
        if let Some(c) = self
            .model
            .with_untracked(|m| m.runtime.tools.filter.column.parse::<u32>().ok())
            .filter(|c| *c > 0 && *c <= area.column_count())
        {
            self.filter_command(wire::FilterChange::ClearColumn(
                area.first().column() + c - 1,
            ));
        }
    }

    pub fn sort_rows(&self, descending: bool) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                return;
            }
            let areas = controller.areas.get_untracked();
            let [area] = areas.as_slice() else {
                controller.say("Select one rectangular range to sort", true);
                return;
            };
            let area = *area;
            let column = controller
                .model
                .with_untracked(|m| m.runtime.tools.sort_column.parse::<u32>());
            let Ok(column) = column else {
                controller.say("Enter a column number within the selection", true);
                return;
            };
            if column == 0 || column > area.column_count() {
                controller.say(
                    "The sort column must be within the selected range (1 is its first column)",
                    true,
                );
                return;
            }
            let (header, levels) = controller.model.with_untracked(|m| {
                (
                    m.runtime.tools.sort_header,
                    m.runtime.tools.sort_levels.clone(),
                )
            });
            let mut keys = vec![(area.first().column() + column - 1, descending)];
            for (text, reverse) in levels {
                let Some(column) = text
                    .parse::<u32>()
                    .ok()
                    .filter(|c| *c > 0 && *c <= area.column_count())
                else {
                    controller.say("Every sort key must be a column within the selection", true);
                    return;
                };
                keys.push((area.first().column() + column - 1, reverse));
            }
            let (case_sensitive, custom_order) = controller.model.with_untracked(|m| {
                (
                    m.runtime.tools.sort_case_sensitive,
                    m.runtime.tools.sort_custom_order.clone(),
                )
            });
            let custom_order: Vec<String> = if custom_order.is_empty() {
                Vec::new()
            } else {
                custom_order.lines().map(str::to_owned).collect()
            };
            let revision = controller.summary.get_untracked().revision;
            let custom_sort = !custom_order.is_empty();
            let intent = if case_sensitive || !custom_order.is_empty() {
                WorkbookCommand::SortRowsWithOptions {
                    area: area_target(area),
                    keys,
                    header,
                    revision,
                    case_sensitive,
                    custom_order,
                }
            } else {
                WorkbookCommand::SortRows {
                    area: area_target(area),
                    keys,
                    header,
                    revision,
                }
            };
            controller.dispatch(intent, move |outcome| {
                if outcome.accepted {
                    controller.say(
                        format!(
                            "Sorted {} {}",
                            controller.area_label(area),
                            if custom_sort {
                                "in custom list order"
                            } else if descending {
                                "largest to smallest"
                            } else {
                                "smallest to largest"
                            }
                        ),
                        false,
                    );
                }
            });
        });
    }

    pub fn split_panes(&self, x: String, y: String) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                return;
            }
            let (Ok(x), Ok(y)) = (x.trim().parse::<f64>(), y.trim().parse::<f64>()) else {
                controller.say("Enter nonnegative split positions in points", true);
                return;
            };
            if !x.is_finite() || !y.is_finite() || x < 0. || y < 0. || x > 10000. || y > 10000. {
                controller.say("Split positions must be from 0 to 10000 points", true);
                return;
            }
            let Some(sheet) = controller.sheet.get_untracked() else {
                return;
            };
            let (row, column) = controller.active.get_untracked();
            controller.dispatch(
                WorkbookCommand::SplitPanes {
                    sheet: Target::Sheet(sheet),
                    revision: controller.summary.get_untracked().revision,
                    x: (x * 20.).to_bits(),
                    y: (y * 20.).to_bits(),
                    row,
                    column,
                },
                move |o| {
                    if o.accepted {
                        controller.say(
                            if x == 0. && y == 0. {
                                "Split removed"
                            } else {
                                "Worksheet split"
                            },
                            false,
                        );
                    }
                },
            );
        });
    }
    pub fn freeze_panes(&self, choice: &'static str) {
        let controller = *self;
        self.after_input(move || {
            if controller.editing.get_untracked().is_some() {
                controller.say(
                    "Finish or cancel the current entry before changing panes",
                    true,
                );
                return;
            }
            let Some(sheet) = controller.sheet.get_untracked() else {
                return;
            };
            if choice != "none" {
                let offset = controller.grid_geometry(sheet).is_some_and(|g| {
                    (g.frozen_rows > 0 || g.frozen_columns > 0)
                        && g.top_left.is_some_and(|at| at != (1, 1))
                });
                if offset {
                    controller.say(
                        "Unfreeze this imported view before choosing new frozen bands",
                        true,
                    );
                    return;
                }
                if choice == "active"
                    && controller
                        .grid_view_for(sheet, controller.active_grid_pane(sheet).unwrap_or(0))
                        .is_some_and(|v| v.viewport.scroll_x > 0.5 || v.viewport.scroll_y > 0.5)
                {
                    controller.say(
                        "Return to the top-left of the sheet before freezing at the active cell",
                        true,
                    );
                    return;
                }
            }
            let (row, col) = controller.active.get_untracked();
            let (rows, columns) = match choice {
                "first-row" => (1, 0),
                "first-column" => (0, 1),
                "active" => (row - 1, col - 1),
                _ => (0, 0),
            };
            controller.dispatch(
                WorkbookCommand::FreezePanes {
                    sheet: Target::Sheet(sheet),
                    rows,
                    columns,
                },
                move |outcome| {
                    if outcome.accepted {
                        controller.say(
                            if rows == 0 && columns == 0 {
                                "Panes unfrozen".to_owned()
                            } else {
                                format!("Frozen: {rows} rows, {columns} columns")
                            },
                            false,
                        );
                    }
                },
            );
        });
    }

    /// Makes `at` the active cell, showing its sheet (a skin's jump to a cell: the tree's, the
    /// canvas's, the ledger's).

    pub fn toggle_cell(&self, row: u32, col: u32) {
        if self.has_deferred() && !self.replaying.get_value() {
            let controller = *self;
            self.enqueue(move || controller.toggle_cell(row, col));
            return;
        }
        let Some(sheet) = self.sheet.get_untracked() else {
            return;
        };
        let Ok(at) = CellKey::new(sheet, row, col) else {
            return;
        };
        let areas = self.areas.get_untracked();
        let removing = areas.iter().any(|area| area.contains(at));
        let mut areas: Vec<_> = if removing {
            areas
                .into_iter()
                .flat_map(|area| dna_calc_model::selection::subtract(area, GridArea::cell(at)))
                .collect()
        } else {
            let mut areas = areas;
            areas.push(GridArea::cell(at));
            areas
        };
        if areas.is_empty() {
            areas.push(GridArea::cell(at));
        }
        let active = if areas.iter().any(|area| area.contains(at)) {
            at
        } else {
            areas[0].first()
        };
        self.select_areas(active, areas);
    }

    pub fn drag_range(
        &self,
        from: GridArea,
        to: GridArea,
        copy: bool,
        generation: u64,
        revision: u64,
    ) {
        let controller = *self;
        self.after_input(move || {
            if generation != controller.generation.get_untracked()
                || revision != controller.summary.get_untracked().revision
                || controller.selected_area_untracked() != Some(from)
                || controller.editing.get_untracked().is_some()
            {
                controller.say(
                    "The workbook or selection changed during the drag; drag again",
                    true,
                );
                return;
            }
            if from == to {
                return;
            }
            controller.pending_actions.update(|n| *n += 1);
            controller.pending_changes.update(|n| *n += 1);
            let keep = controller.copied.get_untracked().map(|c| c.token);
            let epoch = controller.selection_epoch.get_value();
            controller.call(
                Request::Capture {
                    area: Area::of(from),
                    keep,
                },
                None,
                move |reply, _| {
                    controller
                        .pending_actions
                        .update(|n| *n = n.saturating_sub(1));
                    controller
                        .pending_changes
                        .update(|n| *n = n.saturating_sub(1));
                    if generation != controller.generation.get_untracked() {
                        return;
                    }
                    if epoch != controller.selection_epoch.get_value()
                        || controller.editing.get_untracked().is_some()
                    {
                        controller.say("The selection changed during the drag; drag again", true);
                        return;
                    }
                    if revision != controller.summary.get_untracked().revision {
                        controller.say("The workbook changed during the drag; drag again", true);
                        return;
                    }
                    match reply {
                        Ok(Reply::Clipboard(payload)) if payload.revision == revision => {
                            controller.prepare_paste_at_mode(
                                wire::PasteSource::Internal {
                                    token: payload.token,
                                    cut: !copy,
                                },
                                to,
                                false,
                                wire::PasteMode::All,
                                false,
                                true,
                            );
                        }
                        Ok(Reply::Clipboard(_)) => {
                            controller.say("The drag source changed; drag again", true)
                        }
                        Err(reason) => controller.say(reason, true),
                        _ => controller.say("The drag snapshot did not answer", true),
                    }
                },
            );
        });
    }

    /// Paste the captured workbook snapshot explicitly; system paste reads the native event instead.

    pub fn structure_command(&self, insert: bool, mode: &str) {
        if self.editing.get_untracked().is_some() {
            self.say(
                "Enter or cancel the current edit before inserting or deleting cells",
                true,
            );
            return;
        }
        let Some(area) = self.selected_area_untracked() else {
            return;
        };
        if self.areas.with_untracked(|a| a.len()) != 1 {
            self.say("Select one rectangle", true);
            return;
        }
        let intent = match mode {
            "down" | "right" => WorkbookCommand::ShiftCells {
                area: area_target(area),
                down: mode == "down",
                insert,
            },
            "rows" | "columns" => WorkbookCommand::EditAxis {
                sheet: Target::Sheet(area.sheet()),
                rows: mode == "rows",
                insert,
                start: if mode == "rows" {
                    area.first().row()
                } else {
                    area.first().column()
                },
                count: if mode == "rows" {
                    area.row_count()
                } else {
                    area.column_count()
                },
            },
            _ => return,
        };
        let controller = *self;
        self.dispatch_result(intent, move |result| {
            if result.is_ok_and(|outcome| outcome.accepted) {
                controller.structural_choice.set(None);
                controller.focus_active_skin();
            }
        });
    }
}
