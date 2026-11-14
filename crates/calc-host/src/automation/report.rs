//! The report objects a macro builds (dc-piw0): tables (`ListObjects`), charts (`ChartObjects`,
//! `Chart`) and drawings (`Shapes`, `Shape`, `TextFrame2`, `FillFormat`, `LineFormat`,
//! `ConnectorFormat`), as members of the one object model. Reads use the snapshot of the session;
//! writes go through the session's commands in the command and event contexts only (the one
//! `with_write` ingress), so a worksheet invocation gets Excel's 1004. Each member is one handler
//! registered from the descriptor; a member whose interface the descriptor does not hold is not
//! registered and answers the uniform refusal: see [`WorkbookAutomation::report_members_pending`].
//!
//! Defaults and errors are Excel's recorded ones (`~/codex-runs/eng1/excel-answers.md`); the tests
//! `dc_piw0__excel_answers_*` pin them and name the question of each.

use super::*;
use crate::{CommandIngress, SkinIntent, TableChange, WorkbookCommand};
use calc_object_model::{Descriptor, Handler, ReportItem};

const TABLES: &str = "dc-piw0.1";

const XL_SRC_RANGE: i64 = 1;
const XL_YES: i64 = 1;
const XL_NO: i64 = 2;
const XL_GUESS: i64 = 0;

pub(super) fn report(
    workbook: u64,
    sheet: SheetId,
    interface: &str,
    item: ReportItem,
) -> AutoValue {
    AutoValue::Object(object(
        interface,
        ObjectKey::Report {
            workbook,
            sheet,
            item,
        },
    ))
}

pub(super) fn text_arg(value: &AutoValue) -> Result<String, AutoError> {
    value.as_text().ok_or_else(geometry::type_mismatch)
}

impl WorkbookAutomation {
    pub(super) fn register_report(handlers: &mut Handlers<Self>) -> Vec<String> {
        let mut pending = Vec::new();
        let mut on = |interface: &str, member: &str, kind: InvokeKind, handler: Handler<Self>| {
            let descriptor = Descriptor::get();
            let present = descriptor.id(interface).is_some_and(|id| {
                let id = descriptor.default_interface(id);
                descriptor.member(id, member, kind).is_some()
            });
            if present {
                handlers.on(interface, member, kind, handler);
            } else {
                pending.push(format!("{interface}.{member} ({kind:?})"));
            }
        };
        use InvokeKind::{Get, Method, Put};
        // The worksheet's collections.
        on("Worksheet", "ListObjects", Get, Self::sheet_list_objects);
        on(
            "Worksheet",
            "ChartObjects",
            Method,
            Self::sheet_chart_objects,
        );
        on("Worksheet", "Shapes", Get, Self::sheet_shapes);
        // Tables.
        on("ListObjects", "Count", Get, Self::tables_count);
        on("ListObjects", "Item", Get, Self::tables_item);
        on("ListObjects", "_Default", Get, Self::tables_item);
        on("ListObjects", "Add", Method, Self::tables_add);
        on("ListObject", "Name", Get, Self::table_name);
        on("ListObject", "Name", Put, Self::table_put_name);
        on("ListObject", "Range", Get, Self::table_range);
        on("ListObject", "DataBodyRange", Get, Self::table_body);
        on("ListObject", "HeaderRowRange", Get, Self::table_header);
        on("ListObject", "ShowTotals", Get, Self::table_show_totals);
        on("ListObject", "ShowTotals", Put, Self::table_put_show_totals);
        on("ListObject", "TableStyle", Get, Self::table_style);
        on("ListObject", "TableStyle", Put, Self::table_put_style);
        on("ListObject", "ListColumns", Get, Self::table_columns);
        on("ListObject", "ListRows", Get, Self::table_rows);
        on("ListColumns", "Count", Get, Self::columns_count);
        on("ListColumns", "Item", Get, Self::columns_item);
        on("ListColumns", "_Default", Get, Self::columns_item);
        on("ListColumn", "Name", Get, Self::column_name);
        on("ListColumn", "Name", Put, Self::column_put_name);
        on("ListRows", "Count", Get, Self::rows_count);
        on("ListRows", "Add", Method, Self::rows_add);
        // Charts.
        on("ChartObjects", "Count", Get, Self::chart_objects_count);
        on("ChartObjects", "Item", Method, Self::chart_objects_item);
        on("ChartObjects", "_Default", Method, Self::chart_objects_item);
        on("ChartObjects", "Add", Method, Self::chart_objects_add);
        on("ChartObject", "Name", Get, Self::chart_object_name);
        on("ChartObject", "Name", Put, Self::chart_object_put_name);
        on("ChartObject", "Chart", Get, Self::chart_object_chart);
        for (member, index) in [("Left", 0), ("Top", 1), ("Width", 2), ("Height", 3)] {
            let (get, put): (Handler<Self>, Handler<Self>) = match index {
                0 => (Self::chart_object_left, Self::chart_object_put_left),
                1 => (Self::chart_object_top, Self::chart_object_put_top),
                2 => (Self::chart_object_width, Self::chart_object_put_width),
                _ => (Self::chart_object_height, Self::chart_object_put_height),
            };
            on("ChartObject", member, Get, get);
            on("ChartObject", member, Put, put);
        }
        on("Chart", "ChartType", Get, Self::chart_type);
        on("Chart", "ChartType", Put, Self::chart_put_type);
        on("Chart", "HasTitle", Get, Self::chart_has_title);
        on("Chart", "HasTitle", Put, Self::chart_put_has_title);
        on("Chart", "ChartTitle", Get, Self::chart_title);
        on("Chart", "HasLegend", Get, Self::chart_has_legend);
        on("Chart", "HasLegend", Put, Self::chart_put_has_legend);
        on("Chart", "SetSourceData", Method, Self::chart_set_source);
        on("Chart", "SeriesCollection", Method, Self::chart_series_collection);
        on("SeriesCollection", "Count", Get, Self::series_count);
        on("SeriesCollection", "Item", Method, Self::series_item);
        on("SeriesCollection", "_Default", Method, Self::series_item);
        on("Series", "Name", Get, Self::series_name_get);
        on("Series", "Formula", Get, Self::series_formula);
        on("Series", "PlotOrder", Get, Self::series_plot_order);
        on("Series", "Values", Get, Self::series_values_get);
        on("Series", "Values", Put, Self::series_values_put);
        on("Series", "XValues", Get, Self::series_xvalues_get);
        on("Series", "XValues", Put, Self::series_xvalues_put);
        on("ChartTitle", "Text", Get, Self::chart_title_text);
        on("ChartTitle", "Text", Put, Self::chart_title_put_text);
        // Drawings.
        on("Shapes", "Count", Get, Self::shapes_count);
        on("Shapes", "Item", Method, Self::shapes_item);
        on("Shapes", "_Default", Method, Self::shapes_item);
        on("Shapes", "AddShape", Method, Self::shapes_add_shape);
        on("Shapes", "AddChart2", Method, Self::shapes_add_chart2);
        on("Shape", "Type", Get, Self::shape_type);
        on("Shape", "AutoShapeType", Get, Self::shape_auto_shape_type);
        on("Shape", "Connector", Get, Self::shape_connector);
        on("Shape", "HorizontalFlip", Get, Self::shape_horizontal_flip);
        on("Shape", "VerticalFlip", Get, Self::shape_vertical_flip);
        on("Shape", "ConnectionSiteCount", Get, Self::shape_site_count);
        on("Shape", "Chart", Get, Self::shape_chart);
        on("Shapes", "AddTextbox", Method, Self::shapes_add_textbox);
        on("Shapes", "AddLine", Method, Self::shapes_add_line);
        on("Shapes", "AddConnector", Method, Self::shapes_add_connector);
        on("Shape", "Name", Get, Self::shape_name);
        on("Shape", "Name", Put, Self::shape_put_name);
        on("Shape", "ID", Get, Self::shape_id);
        on("Shape", "Delete", Method, Self::shape_delete);
        on("Shape", "TextFrame2", Get, Self::shape_text_frame);
        on("Shape", "Fill", Get, Self::shape_fill);
        on("Shape", "Line", Get, Self::shape_line);
        on(
            "Shape",
            "ConnectorFormat",
            Get,
            Self::shape_connector_format,
        );
        for (member, index) in [("Left", 0), ("Top", 1), ("Width", 2), ("Height", 3)] {
            let (get, put): (Handler<Self>, Handler<Self>) = match index {
                0 => (Self::shape_left, Self::shape_put_left),
                1 => (Self::shape_top, Self::shape_put_top),
                2 => (Self::shape_width, Self::shape_put_width),
                _ => (Self::shape_height, Self::shape_put_height),
            };
            on("Shape", member, Get, get);
            on("Shape", member, Put, put);
        }
        on("TextFrame2", "TextRange", Get, Self::frame_text_range);
        on("TextRange2", "Text", Get, Self::range_text);
        on("TextRange2", "Text", Put, Self::range_put_text);
        on("FillFormat", "ForeColor", Get, Self::fill_colour);
        on("LineFormat", "ForeColor", Get, Self::line_colour);
        on("LineFormat", "Weight", Get, Self::line_weight);
        on("LineFormat", "Weight", Put, Self::line_put_weight);
        on("ColorFormat", "RGB", Get, Self::colour_rgb);
        on("ColorFormat", "RGB", Put, Self::colour_put_rgb);
        on(
            "ConnectorFormat",
            "BeginConnect",
            Method,
            Self::connector_begin,
        );
        on("ConnectorFormat", "EndConnect", Method, Self::connector_end);
        on("ConnectorFormat", "BeginConnected", Get, Self::connector_begin_connected);
        on("ConnectorFormat", "EndConnected", Get, Self::connector_end_connected);
        on("ConnectorFormat", "BeginConnectionSite", Get, Self::connector_begin_site);
        on("ConnectorFormat", "EndConnectionSite", Get, Self::connector_end_site);
        on("ConnectorFormat", "BeginConnectedShape", Get, Self::connector_begin_shape);
        on("ConnectorFormat", "EndConnectedShape", Get, Self::connector_end_shape_get);
        pending
    }

    /// The report members the committed descriptor does not describe yet, so they refuse with the
    /// uniform rule. Empty once the descriptor is regenerated with the report interfaces.
    pub fn report_members_pending(&self) -> Vec<String> {
        Self::register_report(&mut Handlers::new())
    }

    /// The range a Source argument names, resolved against the session.
    pub(super) fn source_range(
        &self,
        target: &ObjectRef,
        value: &AutoValue,
        sheet: SheetId,
    ) -> Result<ReferenceLike, AutoError> {
        self.with_access(|mut access| {
            let meta = self.metadata()?;
            self.validate(target, &meta)?;
            self.range_argument(&mut access, &meta, value, Some(sheet))
        })
    }

    pub(super) fn report_key(target: &ObjectRef) -> Result<(u64, SheetId, ReportItem), AutoError> {
        match &target.key {
            ObjectKey::Worksheet { workbook, sheet } => {
                Ok((*workbook, *sheet, ReportItem::ListObjects))
            }
            ObjectKey::Report {
                workbook,
                sheet,
                item,
            } => Ok((*workbook, *sheet, item.clone())),
            _ => Err(geometry::type_mismatch()),
        }
    }

    /// A read of the session's current state: the snapshot model is the session's last published
    /// model, so a table made a moment ago is seen.
    pub(super) fn read_host<T>(
        &self,
        target: &ObjectRef,
        mut read: impl FnMut(&HostSession, SheetId, &ReportItem) -> Result<T, AutoError>,
    ) -> Result<T, AutoError> {
        let (_, sheet, item) = Self::report_key(target)?;
        let mut out = None;
        let reply = self.host.with_session(&mut |session| {
            out = Some(read(session, sheet, &item)?);
            Ok(AutoValue::Empty)
        });
        match (reply, out) {
            (Ok(_), Some(value)) => Ok(value),
            (Err(error), _) => Err(error),
            _ => Err(busy()),
        }
    }

    pub(super) fn command(
        &self,
        call: &Call<'_>,
        mut make: impl FnMut(&HostSession) -> Result<WorkbookCommand, AutoError>,
    ) -> Reply {
        self.with_write(|session| {
            let command = make(session)?;
            let receipt =
                session.dispatch_from(CommandIngress::Automation, SkinIntent::Command(command));
            if let Some(error) = receipt.error {
                return Err(AutoError::not_modelled(
                    format!("{}: {error:?}", call.label()),
                    OWNER,
                ));
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    // --- the worksheet's collections ---------------------------------------------------------

    pub(super) fn collection(
        &self,
        target: &ObjectRef,
        interface: &str,
        item: ReportItem,
    ) -> Reply {
        let (workbook, sheet, _) = Self::report_key(target)?;
        self.with_target(target, |_, _| {
            Ok(report(workbook, sheet, interface, item.clone()))
        })
    }
    fn sheet_list_objects(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.collection(target, "ListObjects", ReportItem::ListObjects)
    }
    fn sheet_shapes(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.collection(target, "Shapes", ReportItem::Shapes)
    }
    /// `Worksheet.ChartObjects(Index)`: the collection without an index, a chart with one.
    fn sheet_chart_objects(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let collection = self.collection(target, "ChartObjects", ReportItem::ChartObjects)?;
        if call.arg(0).is_missing() {
            return Ok(collection);
        }
        let AutoValue::Object(collection) = collection else {
            return Err(geometry::type_mismatch());
        };
        self.chart_objects_item(&collection, call)
    }

    // --- tables --------------------------------------------------------------------------------

    fn tables_of(session: &HostSession, sheet: SheetId) -> Vec<calc_doc_model::Table> {
        session
            .host()
            .model()
            .definitions()
            .tables()
            .iter()
            .filter(|t| t.sheet == sheet)
            .cloned()
            .collect()
    }
    fn table_of(
        session: &HostSession,
        sheet: SheetId,
        part: &str,
    ) -> Result<calc_doc_model::Table, AutoError> {
        Self::tables_of(session, sheet)
            .into_iter()
            .find(|t| t.part == part)
            .ok_or_else(|| AutoError::excel(424, "Object required"))
    }
    fn table_part(target: &ObjectRef) -> Result<String, AutoError> {
        match Self::report_key(target)?.2 {
            ReportItem::ListObject(part)
            | ReportItem::ListColumns(part)
            | ReportItem::ListColumn(part, _)
            | ReportItem::ListRows(part) => Ok(part),
            _ => Err(geometry::type_mismatch()),
        }
    }
    fn tables_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.read_host(target, |s, sheet, _| {
            Ok(AutoValue::I4(Self::tables_of(s, sheet).len() as i32))
        })
    }
    fn tables_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let (workbook, _, _) = Self::report_key(target)?;
        self.read_host(target, |s, sheet, _| {
            let tables = Self::tables_of(s, sheet);
            let table = match call.arg(0) {
                AutoValue::Text(name) => tables.iter().find(|t| {
                    calc_value::workbook_names_equal(&t.display_name, &name.to_string_lossy())
                }),
                other => usize::try_from(geometry::integer(other)? - 1)
                    .ok()
                    .and_then(|n| tables.get(n)),
            }
            .ok_or_else(|| AutoError::excel(9, "Subscript out of range"))?;
            Ok(report(
                workbook,
                sheet,
                "ListObject",
                ReportItem::ListObject(table.part.clone()),
            ))
        })
    }
    /// `ListObjects.Add(SourceType, Source, LinkSource, XlListObjectHasHeaders, Destination,
    /// TableStyleName)`: a range source with headers (xlYes). The first table gets Excel's default
    /// name `Table1`, the next free one after it (excel-answers.md Q1: table names are workbook-wide).
    fn tables_add(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let (workbook, sheet, _) = Self::report_key(target)?;
        if !matches!(call.arg(0), AutoValue::Missing)
            && geometry::integer(call.arg(0))? != XL_SRC_RANGE
        {
            return Err(AutoError::not_modelled(
                "ListObjects.Add SourceType other than xlSrcRange",
                TABLES,
            ));
        }
        if !call.arg(2).is_missing() || !call.arg(4).is_missing() {
            return Err(AutoError::not_modelled(
                "ListObjects.Add LinkSource and Destination",
                TABLES,
            ));
        }
        let headers = match call.arg(3) {
            AutoValue::Missing => XL_GUESS,
            other => geometry::integer(other)?,
        };
        if headers == XL_NO {
            return Err(AutoError::not_modelled(
                "ListObjects.Add with xlNo headers (Excel inserts a header row above the data)",
                TABLES,
            ));
        }
        let range = self.source_range(target, call.arg(1), sheet)?;
        if range.area_count() != 1 {
            return Err(AutoError::excel(1004, ""));
        }
        let area = range.areas()[0];
        // Excel (dc-piw0 Q3, Q4): a source that overlaps a table is 1004; without xlYes a first
        // row of text is taken as the headers, any other first row makes Excel insert a header
        // row, which is not modelled.
        self.read_host(target, |s, sheet, _| {
            if Self::tables_of(s, sheet)
                .iter()
                .any(|t| t.area.intersection(area).is_some())
            {
                return Err(AutoError::excel(
                    1004,
                    "A table can't overlap another table. ",
                ));
            }
            if headers != XL_YES {
                let model = s.host().model();
                let text_first_row = (0..area.column_count()).all(|c| {
                    CellKey::new(sheet, area.first().row(), area.first().column() + c)
                        .ok()
                        .and_then(|at| model.cell(at))
                        .is_some_and(|cell| matches!(cell, crate::CellInput::Text(_)))
                });
                if !text_first_row {
                    return Err(AutoError::not_modelled(
                        "ListObjects.Add without xlYes over a first row that is not text \
                         (Excel inserts a header row above the data)",
                        TABLES,
                    ));
                }
            }
            Ok(())
        })?;
        let style = match call.arg(5) {
            AutoValue::Missing => None,
            other => Some(text_arg(other)?),
        };
        let mut created = None;
        let reply = self.command(call, |s| {
            let tables = s.host().model().definitions().tables();
            let name = (1u32..)
                .map(|i| format!("Table{i}"))
                .find(|n| {
                    !tables
                        .iter()
                        .any(|t| calc_value::workbook_names_equal(&t.display_name, n))
                })
                .expect("a free table name");
            created = Some(name.clone());
            Ok(WorkbookCommand::Table {
                revision: s.host().revision().get(),
                change: TableChange::Create { area, name },
            })
        })?;
        let _ = reply;
        let name = created.ok_or_else(busy)?;
        if let Some(style) = style {
            let part = self.table_by_name(target, &name)?;
            self.table_options(call, &part, |o| o.style = style.clone())?;
        }
        let part = self.table_by_name(target, &name)?;
        Ok(report(
            workbook,
            sheet,
            "ListObject",
            ReportItem::ListObject(part),
        ))
    }
    fn table_by_name(&self, target: &ObjectRef, name: &str) -> Result<String, AutoError> {
        self.read_host(target, |s, sheet, _| {
            Self::tables_of(s, sheet)
                .into_iter()
                .find(|t| t.display_name == name)
                .map(|t| t.part)
                .ok_or_else(|| AutoError::excel(424, "Object required"))
        })
    }
    fn table_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let part = Self::table_part(target)?;
        self.read_host(target, |s, sheet, _| {
            Ok(AutoValue::text(
                &Self::table_of(s, sheet, &part)?.display_name,
            ))
        })
    }
    fn table_put_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let part = Self::table_part(target)?;
        let name = text_arg(call.args.last().ok_or_else(geometry::bad_count)?)?;
        // Excel (dc-piw0 Q3): an empty name is 1004, over 255 characters is error 5, another
        // table's name (any case) is 1004; a case-only change is accepted.
        if name.is_empty() {
            return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
        }
        if name.chars().count() > 255 {
            return Err(AutoError::excel(5, "Invalid procedure call or argument"));
        }
        self.read_host(target, |s, _, _| {
            let clash = s.host().model().definitions().tables().iter().any(|t| {
                t.part != part && calc_value::workbook_names_equal(&t.display_name, &name)
            });
            if clash {
                return Err(AutoError::excel(
                    1004,
                    "Application-defined or object-defined error",
                ));
            }
            Ok(())
        })?;
        self.command(call, |s| {
            Ok(WorkbookCommand::Table {
                revision: s.host().revision().get(),
                change: TableChange::Rename {
                    part: part.clone(),
                    name: name.clone(),
                },
            })
        })
    }
    fn table_area_reply(
        &self,
        target: &ObjectRef,
        pick: impl Fn(&calc_doc_model::Table) -> Option<GridArea>,
    ) -> Reply {
        let part = Self::table_part(target)?;
        let mut area = None;
        self.read_host(target, |s, sheet, _| {
            area = pick(&Self::table_of(s, sheet, &part)?);
            Ok(())
        })?;
        match area {
            Some(area) => self.with_target(target, |access, _| {
                self.acquire(access, &ReferenceLike::area(area), RangeAxis::Cells)
            }),
            // Excel: DataBodyRange and the like are Nothing when the part is absent.
            None => Ok(AutoValue::Empty),
        }
    }
    fn table_range(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.table_area_reply(target, |t| Some(t.area))
    }
    fn table_header(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.table_area_reply(target, |t| {
            (t.header_rows > 0).then(|| {
                GridArea::new(
                    t.area.first(),
                    CellKey::new(t.sheet, t.area.first().row(), t.area.last().column()).unwrap(),
                )
                .unwrap()
            })
        })
    }
    fn table_body(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.table_area_reply(target, |t| {
            let (first, last) = (
                t.area.first().row() + t.header_rows,
                t.area.last().row() - t.totals_rows,
            );
            (first <= last).then(|| {
                GridArea::new(
                    CellKey::new(t.sheet, first, t.area.first().column()).unwrap(),
                    CellKey::new(t.sheet, last, t.area.last().column()).unwrap(),
                )
                .unwrap()
            })
        })
    }
    fn table_show_totals(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let part = Self::table_part(target)?;
        self.read_host(target, |s, sheet, _| {
            Ok(AutoValue::Bool(
                Self::table_of(s, sheet, &part)?.totals_rows > 0,
            ))
        })
    }
    fn table_style(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let part = Self::table_part(target)?;
        self.read_host(target, |s, sheet, _| {
            let table = Self::table_of(s, sheet, &part)?;
            Ok(table
                .style
                .name
                .as_deref()
                .map_or(AutoValue::Empty, AutoValue::text))
        })
    }
    fn table_options(
        &self,
        call: &Call<'_>,
        part: &str,
        change: impl Fn(&mut TableOptions),
    ) -> Reply {
        let part = part.to_owned();
        self.command(call, |s| {
            let table = s
                .host()
                .model()
                .definitions()
                .tables()
                .iter()
                .find(|t| t.part == part)
                .ok_or_else(|| AutoError::excel(424, "Object required"))?;
            let mut options = TableOptions {
                headers: table.header_rows > 0,
                totals: table.totals_rows > 0,
                style: table.style.name.clone().unwrap_or_default(),
            };
            change(&mut options);
            Ok(WorkbookCommand::Table {
                revision: s.host().revision().get(),
                change: TableChange::Options {
                    part: part.clone(),
                    headers: options.headers,
                    totals: options.totals,
                    style: options.style,
                    row_stripes: table.style.row_stripes,
                    column_stripes: table.style.column_stripes,
                    first_column: table.style.first_column,
                    last_column: table.style.last_column,
                },
            })
        })
    }
    fn table_put_show_totals(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let part = Self::table_part(target)?;
        let on = geometry::boolean(call.args.last().ok_or_else(geometry::bad_count)?)?;
        self.table_options(call, &part, |o| o.totals = on)
    }
    fn table_put_style(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let part = Self::table_part(target)?;
        let style = text_arg(call.args.last().ok_or_else(geometry::bad_count)?)?;
        self.table_options(call, &part, |o| o.style = style.clone())
    }
    fn table_child(
        &self,
        target: &ObjectRef,
        interface: &str,
        make: fn(String) -> ReportItem,
    ) -> Reply {
        let (workbook, sheet, _) = Self::report_key(target)?;
        let part = Self::table_part(target)?;
        self.read_host(target, |s, sheet, _| {
            Self::table_of(s, sheet, &part).map(|_| ())
        })?;
        Ok(report(workbook, sheet, interface, make(part)))
    }
    fn table_columns(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.table_child(target, "ListColumns", ReportItem::ListColumns)
    }
    fn table_rows(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.table_child(target, "ListRows", ReportItem::ListRows)
    }
    fn columns_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let part = Self::table_part(target)?;
        self.read_host(target, |s, sheet, _| {
            Ok(AutoValue::I4(
                Self::table_of(s, sheet, &part)?.columns.len() as i32,
            ))
        })
    }
    fn columns_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let (workbook, _, _) = Self::report_key(target)?;
        let part = Self::table_part(target)?;
        self.read_host(target, |s, sheet, _| {
            let table = Self::table_of(s, sheet, &part)?;
            let index = match call.arg(0) {
                AutoValue::Text(name) => table
                    .columns
                    .iter()
                    .position(|c| calc_value::workbook_names_equal(c, &name.to_string_lossy())),
                other => usize::try_from(geometry::integer(other)? - 1)
                    .ok()
                    .filter(|n| *n < table.columns.len()),
            }
            .ok_or_else(|| AutoError::excel(9, "Subscript out of range"))?;
            Ok(report(
                workbook,
                sheet,
                "ListColumn",
                ReportItem::ListColumn(part.clone(), index as u32),
            ))
        })
    }
    fn column_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let ReportItem::ListColumn(part, index) = Self::report_key(target)?.2 else {
            return Err(geometry::type_mismatch());
        };
        self.read_host(target, |s, sheet, _| {
            let table = Self::table_of(s, sheet, &part)?;
            table
                .columns
                .get(index as usize)
                .map(|c| AutoValue::text(c))
                .ok_or_else(|| AutoError::excel(424, "Object required"))
        })
    }
    fn column_put_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let ReportItem::ListColumn(part, index) = Self::report_key(target)?.2 else {
            return Err(geometry::type_mismatch());
        };
        // A number is stored as its text; an empty name becomes `Column<n>`; a name another column
        // has is silently ignored, the name unchanged (dc-piw0 Q3).
        let name = match call.args.last().ok_or_else(geometry::bad_count)? {
            AutoValue::I2(v) => v.to_string(),
            AutoValue::I4(v) => v.to_string(),
            other => text_arg(other)?,
        };
        let mut ignored = false;
        let table = self.read_host(target, |s, sheet, _| Self::table_of(s, sheet, &part))?;
        let name = if name.is_empty() {
            let taken: Vec<String> = table.columns.clone();
            calc_doc_model::free_column_name(&taken)
        } else {
            name
        };
        if table.columns.iter().enumerate().any(|(j, c)| {
            j != index as usize && calc_value::workbook_names_equal(c, &name)
        }) {
            ignored = true;
        }
        if ignored {
            return Ok(AutoValue::Empty);
        }
        self.command(call, |s| {
            let table = s
                .host()
                .model()
                .definitions()
                .tables()
                .iter()
                .find(|t| t.part == part)
                .ok_or_else(|| AutoError::excel(424, "Object required"))?;
            Ok(WorkbookCommand::Table {
                revision: s.host().revision().get(),
                change: TableChange::RenameColumn {
                    part: part.clone(),
                    column: table.area.first().column() + index,
                    name: name.clone(),
                },
            })
        })
    }
    fn rows_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let part = Self::table_part(target)?;
        self.read_host(target, |s, sheet, _| {
            let t = Self::table_of(s, sheet, &part)?;
            Ok(AutoValue::I4(
                (t.area.row_count() - t.header_rows - t.totals_rows) as i32,
            ))
        })
    }
    /// `ListRows.Add(Position, AlwaysInsert)`: a new last row (a position inside the table needs
    /// Excel's row shift, dc-piw0.1).
    fn rows_add(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if !call.arg(0).is_missing() || !call.arg(1).is_missing() {
            return Err(AutoError::not_modelled(
                "ListRows.Add Position and AlwaysInsert",
                TABLES,
            ));
        }
        let part = Self::table_part(target)?;
        self.command(call, |s| {
            let table = s
                .host()
                .model()
                .definitions()
                .tables()
                .iter()
                .find(|t| t.part == part)
                .ok_or_else(|| AutoError::excel(424, "Object required"))?;
            let last = table.area.last();
            let row = last
                .row()
                .checked_add(1)
                .ok_or_else(|| AutoError::excel(1004, ""))?;
            let area = GridArea::new(
                table.area.first(),
                CellKey::new(table.sheet, row, last.column())
                    .map_err(|_| AutoError::excel(1004, ""))?,
            )
            .unwrap();
            Ok(WorkbookCommand::Table {
                revision: s.host().revision().get(),
                change: TableChange::Resize {
                    part: part.clone(),
                    area,
                },
            })
        })
    }
}

/// The table options a member changes; the rest of the table's options are kept.
struct TableOptions {
    headers: bool,
    totals: bool,
    style: String,
}
