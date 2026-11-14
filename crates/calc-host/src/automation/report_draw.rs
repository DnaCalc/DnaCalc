//! Charts and drawings of the report objects (dc-piw0.2, dc-piw0.3): `ChartObjects`, `Chart`,
//! `Shapes`, `Shape` and the Office members reached from a shape. See `report.rs` for the rules
//! (descriptor-registered handlers, command context writes, uniform refusals).

use super::report::{report, text_arg};
use super::*;
use crate::{Points, ShapeChange, WorkbookCommand};
use calc_doc_model::{ChartAnchor, Shape, ShapeColour, ShapeFill, ShapeKind, ShapeLine};
use calc_object_model::ReportItem;

const CHARTS: &str = "dc-piw0.2";
const SHAPES: &str = "dc-piw0.3";
const EMU_PER_POINT: f64 = 12_700.0;

fn single(value: f64) -> AutoValue {
    AutoValue::R4(value as f32)
}

fn number(value: &AutoValue) -> Result<f64, AutoError> {
    match value {
        AutoValue::I2(v) => Ok(f64::from(*v)),
        AutoValue::I4(v) => Ok(f64::from(*v)),
        AutoValue::R4(v) => Ok(f64::from(*v)),
        AutoValue::R8(v) => Ok(*v),
        _ => Err(geometry::type_mismatch()),
    }
}

/// Which side of a rectangle a Left/Top/Width/Height member is.
#[derive(Clone, Copy)]
enum Side {
    Left,
    Top,
    Width,
    Height,
}

fn rectangle_with(side: Side, value: f64) -> [Option<Points>; 4] {
    let mut out = [None; 4];
    out[side as usize] = Some(Points::new(value));
    out
}

fn rgb_to_colour(value: i64) -> Result<ShapeColour, AutoError> {
    if !(0..=0xFF_FFFF).contains(&value) {
        return Err(geometry::type_mismatch());
    }
    let (r, g, b) = (value & 0xFF, (value >> 8) & 0xFF, (value >> 16) & 0xFF);
    Ok(ShapeColour::Rgb(((r << 16) | (g << 8) | b) as u32))
}

fn colour_to_rgb(hex: u32) -> i64 {
    let v = i64::from(hex);
    ((v >> 16) & 0xFF) | (v & 0xFF00) | ((v & 0xFF) << 16)
}

impl WorkbookAutomation {
    // --- charts ---------------------------------------------------------------------------------

    /// The chart part a chart-valued report object denotes.
    fn chart_token(target: &ObjectRef) -> Result<String, AutoError> {
        match &target.key {
            ObjectKey::Report {
                item:
                    ReportItem::ChartObject(part)
                    | ReportItem::Chart(part)
                    | ReportItem::ChartTitle(part)
                    | ReportItem::ChartShape(part)
                    | ReportItem::SeriesCollection(part)
                    | ReportItem::Series(part, _),
                ..
            } => Ok(part.clone()),
            _ => Err(geometry::type_mismatch()),
        }
    }

    fn charts_of(session: &HostSession, sheet: SheetId) -> Vec<calc_doc_model::Chart> {
        session.host().model().definitions().charts(sheet).to_vec()
    }

    /// A read of the chart the object denotes; a chart that is gone is Excel's "Object required".
    fn real_chart<T>(
        &self,
        target: &ObjectRef,
        read: impl Fn(&calc_doc_model::Chart) -> T,
    ) -> Result<T, AutoError> {
        let part = Self::chart_token(target)?;
        self.read_host(target, |s, sheet, _| {
            Self::charts_of(s, sheet)
                .iter()
                .find(|c| c.part == part)
                .map(&read)
                .ok_or_else(|| AutoError::excel(424, "Object required"))
        })
    }

    pub(super) fn chart_objects_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.read_host(target, |s, sheet, _| {
            Ok(AutoValue::I4(Self::charts_of(s, sheet).len() as i32))
        })
    }
    /// `ChartObjects(Index)` by place or name. Excel's errors (dc-piw0 Q3): an index out of range
    /// is 1004, a name that is not there -2147024809.
    pub(super) fn chart_objects_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let (workbook, _, _) = Self::report_key(target)?;
        self.read_host(target, |s, sheet, _| {
            let charts = Self::charts_of(s, sheet);
            let found = match call.arg(0) {
                AutoValue::Text(name) => charts
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(&name.to_string_lossy()))
                    .ok_or_else(|| {
                        AutoError::excel(
                            -2147024809,
                            "The item with the specified name wasn't found.",
                        )
                    })?,
                other => usize::try_from(geometry::integer(other)? - 1)
                    .ok()
                    .and_then(|n| charts.get(n))
                    .ok_or_else(|| {
                        AutoError::excel(1004, "Application-defined or object-defined error")
                    })?,
            };
            Ok(report(
                workbook,
                sheet,
                "ChartObject",
                ReportItem::ChartObject(found.part.clone()),
            ))
        })
    }
    /// The chart made last on the sheet: the one a creating command just added.
    fn newest_chart(&self, target: &ObjectRef) -> Result<String, AutoError> {
        self.read_host(target, |s, sheet, _| {
            Self::charts_of(s, sheet)
                .last()
                .map(|c| c.part.clone())
                .ok_or_else(|| AutoError::excel(1004, ""))
        })
    }
    /// `ChartObjects.Add(Left, Top, Width, Height)`: a chart without data, saved as Excel saves an
    /// empty chart (a column chart with its legend, no title).
    pub(super) fn chart_objects_add(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 4 {
            return Err(geometry::bad_count());
        }
        let (workbook, sheet, _) = Self::report_key(target)?;
        let mut place = [0.0; 4];
        for (i, slot) in place.iter_mut().enumerate() {
            *slot = number(call.arg(i))?;
        }
        self.command(call, |s| {
            Ok(WorkbookCommand::ChartCreateEmpty {
                sheet,
                revision: s.host().revision().get(),
                kind: "column".into(),
                place: place.map(Points::new),
                title: None,
                legend: true,
            })
        })?;
        let part = self.newest_chart(target)?;
        Ok(report(workbook, sheet, "ChartObject", ReportItem::ChartObject(part)))
    }
    pub(super) fn chart_object_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(AutoValue::text(&self.real_chart(target, |c| c.name.clone())?))
    }
    pub(super) fn chart_object_put_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let name = text_arg(call.args.last().ok_or_else(geometry::bad_count)?)?;
        self.chart_frame(target, call, Some(name), None)
    }
    fn chart_frame(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        name: Option<String>,
        side: Option<(Side, f64)>,
    ) -> Reply {
        let part = Self::chart_token(target)?;
        let (_, sheet, _) = Self::report_key(target)?;
        self.command(call, |s| {
            let mut rect = Self::chart_rectangle(s, sheet, &part)?;
            if let Some((side, value)) = side {
                rect[side as usize] = value;
            }
            Ok(WorkbookCommand::ChartFrame {
                sheet,
                revision: s.host().revision().get(),
                part: part.clone(),
                name: name.clone(),
                place: side.map(|_| rect.map(Points::new)),
            })
        })
    }
    /// A chart's rectangle in points (left, top, width, height) from its anchor.
    fn chart_rectangle(
        session: &HostSession,
        sheet: SheetId,
        part: &str,
    ) -> Result<[f64; 4], AutoError> {
        let host = session.host();
        let chart = host
            .model()
            .definitions()
            .charts(sheet)
            .iter()
            .find(|c| c.part == part)
            .ok_or_else(|| AutoError::excel(424, "Object required"))?;
        let corner = |p: &calc_doc_model::DrawingPoint| {
            (
                host.column_left(sheet, p.column) + p.column_offset as f64 / EMU_PER_POINT,
                host.row_top(sheet, p.row) + p.row_offset as f64 / EMU_PER_POINT,
            )
        };
        Ok(match chart.anchor {
            ChartAnchor::TwoCell { from, to } => {
                let (l, t) = corner(&from);
                let (r, b) = corner(&to);
                [l, t, r - l, b - t]
            }
            ChartAnchor::OneCell {
                from,
                width,
                height,
            } => {
                let (l, t) = corner(&from);
                [
                    l,
                    t,
                    width as f64 / EMU_PER_POINT,
                    height as f64 / EMU_PER_POINT,
                ]
            }
            ChartAnchor::Absolute {
                x,
                y,
                width,
                height,
            } => [
                x as f64 / EMU_PER_POINT,
                y as f64 / EMU_PER_POINT,
                width as f64 / EMU_PER_POINT,
                height as f64 / EMU_PER_POINT,
            ],
        })
    }
    /// `ChartObject.Left` and the others are Doubles holding a Single (dc-piw0 Q2: 50.7 reads
    /// back 50.7000007629395).
    fn chart_side(&self, target: &ObjectRef, call: &Call<'_>, side: Side) -> Reply {
        no_args(call)?;
        let part = Self::chart_token(target)?;
        self.read_host(target, |s, sheet, _| {
            Ok(AutoValue::R8(f64::from(
                Self::chart_rectangle(s, sheet, &part)?[side as usize] as f32,
            )))
        })
    }
    fn chart_put_side(&self, target: &ObjectRef, call: &Call<'_>, side: Side) -> Reply {
        let value = number(call.args.last().ok_or_else(geometry::bad_count)?)?;
        self.chart_frame(target, call, None, Some((side, value)))
    }
    pub(super) fn chart_object_left(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_side(t, c, Side::Left)
    }
    pub(super) fn chart_object_top(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_side(t, c, Side::Top)
    }
    pub(super) fn chart_object_width(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_side(t, c, Side::Width)
    }
    pub(super) fn chart_object_height(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_side(t, c, Side::Height)
    }
    pub(super) fn chart_object_put_left(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_put_side(t, c, Side::Left)
    }
    pub(super) fn chart_object_put_top(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_put_side(t, c, Side::Top)
    }
    pub(super) fn chart_object_put_width(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_put_side(t, c, Side::Width)
    }
    pub(super) fn chart_object_put_height(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.chart_put_side(t, c, Side::Height)
    }
    pub(super) fn chart_object_chart(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(workbook, sheet, "Chart", ReportItem::Chart(part)))
    }

    pub(super) fn chart_type(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let kind = self
            .real_chart(target, |c| {
                let plot = c.plots.first()?;
                let stacked =
                    matches!(plot.grouping.as_deref(), Some("stacked" | "percentStacked"));
                Some(
                    match (plot.kind.as_str(), plot.bar_direction.as_deref(), stacked) {
                        ("barChart", Some("bar"), false) => 57,
                        ("barChart", Some("bar"), true) => 58,
                        ("barChart", _, false) => 51,
                        ("barChart", _, true) => 52,
                        ("lineChart", _, _) => 4,
                        ("areaChart", _, false) => 1,
                        ("areaChart", _, true) => 76,
                        ("pieChart", _, _) => 5,
                        ("doughnutChart", _, _) => -4120,
                        ("scatterChart", _, _) => -4169,
                        _ => 0,
                    },
                )
            })?
            .filter(|k| *k != 0)
            .ok_or_else(|| AutoError::not_modelled("Chart.ChartType of this chart", CHARTS))?;
        Ok(AutoValue::I4(kind))
    }
    pub(super) fn chart_put_type(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let xl = geometry::integer(call.args.last().ok_or_else(geometry::bad_count)?)?;
        // Excel: a value that is no XlChartType fails the member and leaves the type (Q3).
        if !is_xl_chart_type(xl) {
            return Err(AutoError::excel(
                -2147467259,
                "Method 'ChartType' of object '_Chart' failed",
            ));
        }
        let (kind, stacked) = chart_kind(xl)?;
        let (_, sheet, _) = Self::report_key(target)?;
        let part = Self::chart_token(target)?;
        self.command(call, |s| {
            Ok(WorkbookCommand::ChartType {
                sheet,
                revision: s.host().revision().get(),
                part: part.clone(),
                plot: 0,
                kind: kind.into(),
                stacked,
            })
        })
    }
    /// Whether the chart shows a title: one it holds, or the automatic title of a chart with a
    /// single series that has not had its title removed.
    fn shows_title(c: &calc_doc_model::Chart) -> bool {
        c.title.is_some()
            || c.title_formula.is_some()
            || !c.auto_title_deleted && c.plots.iter().map(|p| p.series.len()).sum::<usize>() == 1
    }
    pub(super) fn chart_has_title(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(AutoValue::Bool(self.real_chart(target, Self::shows_title)?))
    }
    pub(super) fn chart_put_has_title(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let on = geometry::boolean(call.args.last().ok_or_else(geometry::bad_count)?)?;
        if on == self.real_chart(target, Self::shows_title)? {
            return Ok(AutoValue::Empty);
        }
        // Switching a title on shows "Chart Title" until ChartTitle.Text sets another.
        let title = on.then(|| "Chart Title".to_owned());
        self.chart_properties(target, call, Some(title), None)
    }
    fn chart_properties(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        title: Option<Option<String>>,
        legend: Option<bool>,
    ) -> Reply {
        let (_, sheet, _) = Self::report_key(target)?;
        let part = Self::chart_token(target)?;
        self.command(call, |s| {
            Ok(WorkbookCommand::ChartProperties {
                sheet,
                revision: s.host().revision().get(),
                part: part.clone(),
                set_title: title.is_some(),
                title: title.clone().flatten(),
                legend,
            })
        })
    }
    pub(super) fn chart_title(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(
            workbook,
            sheet,
            "ChartTitle",
            ReportItem::ChartTitle(part),
        ))
    }
    pub(super) fn chart_has_legend(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(AutoValue::Bool(self.real_chart(target, |c| c.legend.is_some())?))
    }
    pub(super) fn chart_put_has_legend(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let on = geometry::boolean(call.args.last().ok_or_else(geometry::bad_count)?)?;
        self.chart_properties(target, call, None, Some(on))
    }
    /// Excel: a chart without a title has no `ChartTitle` to read or write (dc-piw0 Q3).
    fn no_title() -> AutoError {
        AutoError::excel(-2147024809, "This object has no title.")
    }
    pub(super) fn chart_title_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (_, sheet, _) = Self::report_key(target)?;
        let chart = self.real_chart(target, |c| c.clone())?;
        // The automatic title of a one-series chart is the series' name.
        let text = match chart.title.clone() {
            Some(title) => Some(title),
            None if Self::shows_title(&chart) => {
                let series = chart.plots.iter().flat_map(|p| p.series.first()).next();
                Some(match series {
                    Some(series) => self.read_host(target, |s, _, _| {
                        Ok(s.host().series_display_name(sheet, series, 0))
                    })?,
                    None => "Chart Title".to_owned(),
                })
            }
            None => None,
        };
        text.map(|t| AutoValue::text(&t)).ok_or_else(Self::no_title)
    }
    pub(super) fn chart_title_put_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let text = text_arg(call.args.last().ok_or_else(geometry::bad_count)?)?;
        if !self.real_chart(target, Self::shows_title)? {
            return Err(Self::no_title());
        }
        if text.chars().count() > 255 {
            return Err(AutoError::excel(
                -2147467259,
                "Method 'Text' of object 'ChartTitle' failed",
            ));
        }
        self.chart_properties(target, call, Some(Some(text)), None)
    }
    /// `Chart.SetSourceData(Source, PlotBy)`: the chart's series become those of the range, in
    /// rows when PlotBy is xlRows (1) and in columns when xlColumns (2). Without PlotBy Excel
    /// plots along the shorter side of the data (rows when equal), a table's range by columns
    /// (dc-piw0 Q6). The source's text first row and first column are series names and
    /// categories (`source_layout`). A source with no data leaves the chart without series.
    pub(super) fn chart_set_source(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let part = Self::chart_token(target)?;
        let (_, sheet, _) = Self::report_key(target)?;
        let failed = || {
            AutoError::excel(
                -2147417848,
                "Method 'SetSourceData' of object '_Chart' failed",
            )
        };
        if !matches!(call.arg(0), AutoValue::Object(_)) {
            return Err(failed());
        }
        let range = self.source_range(target, call.arg(0), sheet)?;
        if range.area_count() != 1 {
            return Err(AutoError::excel(1004, ""));
        }
        let area = range.areas()[0];
        let explicit = match call.arg(1) {
            AutoValue::Missing => None,
            other => Some(geometry::integer(other)? == 1),
        };
        self.command(call, |s| {
            let layout = Self::source_layout(s, area, explicit);
            Ok(WorkbookCommand::ChartSource {
                sheet,
                revision: s.host().revision().get(),
                part: part.clone(),
                area,
                rows: layout.rows,
                headers: layout.headers,
                categories: layout.categories,
            })
        })
    }
    /// How Excel reads a source range (dc-piw0 Q6). The first row is a label row when its cells
    /// (less a first cell the label column covers) are all text, the first column a label column
    /// when its cells (less the corner) are all text; the data is what is left. Series run along
    /// rows when the data has no more rows than columns; the label row then holds the categories
    /// and the label column the series names, the other way round for series in columns. A range
    /// that is exactly a table is plotted in columns.
    fn source_layout(session: &HostSession, area: GridArea, explicit: Option<bool>) -> SourceLayout {
        let model = session.host().model();
        let text_at = |row: u32, column: u32| {
            CellKey::new(area.sheet(), area.first().row() + row, area.first().column() + column)
                .ok()
                .and_then(|at| model.cell(at))
                .is_some_and(|c| matches!(c, crate::CellInput::Text(_)))
        };
        let (rows, columns) = (area.row_count(), area.column_count());
        let first_row_text = |skip: u32| columns > skip && (skip..columns).all(|c| text_at(0, c));
        let first_column_text = |skip: u32| rows > skip && (skip..rows).all(|r| text_at(r, 0));
        // The corner counts with the row or column that is not a label of the other kind.
        let label_row = rows > 1 && first_row_text(1);
        let label_column = columns > 1 && first_column_text(1);
        let (label_row, label_column) = match (label_row, label_column) {
            // Both candidates need the other to leave the corner out; a label row whose corner is
            // text but whose column is numeric is still a label row when the corner is text.
            (true, true) => (true, true),
            (true, false) => (rows > 1 && first_row_text(0), false),
            (false, true) => (false, columns > 1 && first_column_text(0)),
            (false, false) => (false, false),
        };
        let data_rows = rows - u32::from(label_row);
        let data_columns = columns - u32::from(label_column);
        let is_table = session
            .host()
            .model()
            .definitions()
            .tables()
            .iter()
            .any(|t| t.sheet == area.sheet() && t.area == area);
        let by_rows = explicit.unwrap_or(!is_table && data_rows <= data_columns);
        // Series in rows: names in the label column, categories in the label row.
        let (headers, categories) = if by_rows {
            (label_column, label_row)
        } else {
            (label_row, label_column)
        };
        SourceLayout {
            rows: by_rows,
            headers,
            categories,
        }
    }

    // --- series ---------------------------------------------------------------------------------

    pub(super) fn chart_series_collection(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        self.real_chart(target, |_| ())?;
        let collection = report(
            workbook,
            sheet,
            "SeriesCollection",
            ReportItem::SeriesCollection(part),
        );
        if call.arg(0).is_missing() {
            return Ok(collection);
        }
        let AutoValue::Object(collection) = collection else {
            return Err(geometry::type_mismatch());
        };
        self.series_item(&collection, call)
    }
    fn series_of(
        &self,
        target: &ObjectRef,
    ) -> Result<Vec<calc_doc_model::ChartSeries>, AutoError> {
        self.real_chart(target, |c| {
            c.plots.iter().flat_map(|p| p.series.clone()).collect()
        })
    }
    pub(super) fn series_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        Ok(AutoValue::I4(self.series_of(target)?.len() as i32))
    }
    /// `SeriesCollection(Index or Name)`: out of range is 1004 "Parameter not valid" (Q3).
    pub(super) fn series_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let part = Self::chart_token(target)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        let series = self.series_of(target)?;
        let place = match call.arg(0) {
            AutoValue::Text(name) => {
                let name = name.to_string_lossy();
                self.read_host(target, |s, _, _| {
                    Ok(series
                        .iter()
                        .enumerate()
                        .position(|(i, x)| s.host().series_display_name(sheet, x, i) == name))
                })?
            }
            other => usize::try_from(geometry::integer(other)? - 1).ok().filter(|n| *n < series.len()),
        }
        .ok_or_else(|| AutoError::excel(1004, "Parameter not valid"))?;
        Ok(report(
            workbook,
            sheet,
            "Series",
            ReportItem::Series(part, place as u32),
        ))
    }
    /// A sheet name as Excel writes it in a formula: quoted only when it has to be.
    fn excel_reference(canonical: &str) -> String {
        let Some(rest) = canonical.strip_prefix('\'') else {
            return canonical.to_owned();
        };
        let Some(end) = rest.find("'!") else {
            return canonical.to_owned();
        };
        let (name, tail) = rest.split_at(end);
        let word = name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
            && name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_');
        let letters = name.chars().take_while(char::is_ascii_alphabetic).count();
        let cell_like = (1..=3).contains(&letters)
            && letters < name.len()
            && name[letters..].chars().all(|c| c.is_ascii_digit());
        let r1c1 = name
            .strip_prefix(['R', 'r'])
            .is_some_and(|r| r.chars().all(|c| c.is_ascii_digit() || matches!(c, 'C' | 'c')));
        if word && !cell_like && !r1c1 && !name.contains("''") {
            format!("{name}{}", &tail[1..])
        } else {
            canonical.to_owned()
        }
    }
    fn series_at(&self, target: &ObjectRef) -> Result<(usize, calc_doc_model::ChartSeries), AutoError> {
        let ObjectKey::Report {
            item: ReportItem::Series(_, place),
            ..
        } = &target.key
        else {
            return Err(geometry::type_mismatch());
        };
        let place = *place as usize;
        self.series_of(target)?
            .into_iter()
            .nth(place)
            .map(|s| (place, s))
            .ok_or_else(|| AutoError::excel(424, "Object required"))
    }
    pub(super) fn series_name_get(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (place, series) = self.series_at(target)?;
        let (_, sheet, _) = Self::report_key(target)?;
        let name = self.read_host(target, |s, _, _| {
            Ok(s.host().series_display_name(sheet, &series, place))
        })?;
        Ok(AutoValue::text(&name))
    }
    /// `=SERIES(name, categories, values, order)` over the references the series holds.
    pub(super) fn series_formula(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (place, series) = self.series_at(target)?;
        let part = |d: &Option<calc_doc_model::ChartData>| {
            d.as_ref()
                .and_then(|d| d.formula.as_deref())
                .map(Self::excel_reference)
                .unwrap_or_default()
        };
        Ok(AutoValue::text(&format!(
            "=SERIES({},{},{},{})",
            part(&series.name),
            part(&series.categories),
            part(&series.values),
            place + 1
        )))
    }
    pub(super) fn series_plot_order(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let (place, _) = self.series_at(target)?;
        Ok(AutoValue::I4(place as i32 + 1))
    }

    // --- shapes ---------------------------------------------------------------------------------

    fn shape_id_of(target: &ObjectRef) -> Result<u32, AutoError> {
        match &target.key {
            ObjectKey::Report {
                item:
                    ReportItem::Shape(id)
                    | ReportItem::TextFrame2(id)
                    | ReportItem::TextRange2(id)
                    | ReportItem::FillFormat(id)
                    | ReportItem::LineFormat(id)
                    | ReportItem::ConnectorFormat(id)
                    | ReportItem::ColorFormat(id, _),
                ..
            } => Ok(*id),
            _ => Err(geometry::type_mismatch()),
        }
    }
    fn shape_of(session: &HostSession, sheet: SheetId, id: u32) -> Result<Shape, AutoError> {
        session
            .host()
            .shapes(sheet)
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| AutoError::excel(424, "Object required"))
    }
    fn shape_command(&self, target: &ObjectRef, call: &Call<'_>, change: ShapeChange) -> Reply {
        let (_, sheet, _) = Self::report_key(target)?;
        self.command(call, |s| {
            Ok(WorkbookCommand::Shape {
                sheet,
                revision: s.host().revision().get(),
                change: change.clone(),
            })
        })
    }
    /// Excel's Shapes collection holds charts too, so Count counts them.
    pub(super) fn shapes_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        self.read_host(target, |s, sheet, _| {
            Ok(AutoValue::I4(
                (s.host().shapes(sheet).len() + s.host().model().definitions().charts(sheet).len())
                    as i32,
            ))
        })
    }
    /// `Shapes(Index or Name)`: the sheet's charts, then its other objects. Excel's errors
    /// (dc-piw0 Q3): both a name and an index that are not there are -2147024809, with different
    /// descriptions.
    pub(super) fn shapes_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() != 1 {
            return Err(geometry::bad_count());
        }
        let (workbook, _, _) = Self::report_key(target)?;
        self.read_host(target, |s, sheet, _| {
            let charts = s.host().model().definitions().charts(sheet);
            let shapes = s.host().shapes(sheet);
            let item = match call.arg(0) {
                AutoValue::Text(name) => {
                    let name = name.to_string_lossy();
                    charts
                        .iter()
                        .find(|c| c.name.eq_ignore_ascii_case(&name))
                        .map(|c| ReportItem::ChartShape(c.part.clone()))
                        .or_else(|| {
                            shapes
                                .iter()
                                .find(|x| x.name.eq_ignore_ascii_case(&name))
                                .map(|x| ReportItem::Shape(x.id))
                        })
                        .ok_or_else(|| {
                            AutoError::excel(
                                -2147024809,
                                "The item with the specified name wasn't found.",
                            )
                        })?
                }
                other => {
                    let n = usize::try_from(geometry::integer(other)? - 1).ok();
                    n.and_then(|n| match charts.get(n) {
                        Some(c) => Some(ReportItem::ChartShape(c.part.clone())),
                        None => shapes.get(n - charts.len()).map(|x| ReportItem::Shape(x.id)),
                    })
                    .ok_or_else(|| {
                        AutoError::excel(
                            -2147024809,
                            "The index into the specified collection is out of bounds.",
                        )
                    })?
                }
            };
            Ok(report(workbook, sheet, "Shape", item))
        })
    }
    /// `Shapes.AddChart2(Style, Type, Left, Top, Width, Height, NewLayout)`: a chart with a title
    /// and no legend, over the data region of the selection when it holds data (Q6). Without a
    /// place Excel uses the visible window's: 520.8749, 169.125, 360 by 216 on the measured
    /// machine (Q2; window-dependent, so only a default here).
    pub(super) fn shapes_add_chart2(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let (workbook, sheet, _) = Self::report_key(target)?;
        let xl = match call.arg(1) {
            AutoValue::Missing => -1,
            other => geometry::integer(other)?,
        };
        let (kind, _) = chart_kind(if xl == -1 { 51 } else { xl })?;
        let defaults = [520.8749, 169.125, 360.0, 216.0];
        let mut place = defaults;
        for (i, slot) in place.iter_mut().enumerate() {
            if !call.arg(2 + i).is_missing() {
                *slot = number(call.arg(2 + i))?;
            }
        }
        let mut data = None;
        self.command(call, |s| {
            data = Self::selection_data(s, sheet);
            Ok(WorkbookCommand::ChartCreateEmpty {
                sheet,
                revision: s.host().revision().get(),
                kind: kind.into(),
                place: place.map(Points::new),
                title: Some("Chart Title".into()),
                legend: data.is_some(),
            })
        })?;
        let part = self.newest_chart(target)?;
        if let Some(area) = data {
            self.command(call, |s| {
                let layout = Self::source_layout(s, area, None);
                Ok(WorkbookCommand::ChartSource {
                    sheet,
                    revision: s.host().revision().get(),
                    part: part.clone(),
                    area,
                    rows: layout.rows,
                    headers: layout.headers,
                    categories: layout.categories,
                })
            })?;
        }
        Ok(report(workbook, sheet, "Shape", ReportItem::ChartShape(part)))
    }
    /// The data a chart made from the selection plots: a single selected cell with a value stands
    /// for its current region; a selected range stands for itself when it holds a value.
    fn selection_data(session: &HostSession, sheet: SheetId) -> Option<GridArea> {
        let selection = session.selection();
        // A workbook nobody selected in has its first cell selected, as Excel opens one.
        let area = match selection.areas.first() {
            Some(area) => *area,
            None => GridArea::cell(
                selection
                    .active
                    .unwrap_or(CellKey::new(sheet, 1, 1).ok()?),
            ),
        };
        if area.sheet() != sheet {
            return None;
        }
        let model = session.host().model();
        let filled = |row: u32, column: u32| {
            CellKey::new(sheet, row, column)
                .ok()
                .and_then(|at| model.cell(at))
                .is_some()
        };
        let any = |top: u32, left: u32, bottom: u32, right: u32| {
            (top..=bottom).any(|r| (left..=right).any(|c| filled(r, c)))
        };
        let (mut top, mut left) = (area.first().row(), area.first().column());
        let (mut bottom, mut right) = (area.last().row(), area.last().column());
        if area.cell_count() > 1 {
            return any(top, left, bottom, right).then_some(area);
        }
        if !filled(top, left) {
            return None;
        }
        // The current region: grow the rectangle while a ring of cells around it holds a value.
        loop {
            let (t, l) = (top.saturating_sub(1).max(1), left.saturating_sub(1).max(1));
            let (b, r) = ((bottom + 1).min(1_048_576), (right + 1).min(16_384));
            let grown = (t, l, b, r) != (top, left, bottom, right) && {
                // Only the new ring needs a look.
                any(t, l, t, r) && t != top
                    || any(b, l, b, r) && b != bottom
                    || any(t, l, b, l) && l != left
                    || any(t, r, b, r) && r != right
            };
            if !grown {
                break;
            }
            let (nt, nl, nb, nr) = (
                if t != top && any(t, l, t, r) { t } else { top },
                if l != left && any(t, l, b, l) { l } else { left },
                if b != bottom && any(b, l, b, r) { b } else { bottom },
                if r != right && any(t, r, b, r) { r } else { right },
            );
            (top, left, bottom, right) = (nt, nl, nb, nr);
        }
        let first = CellKey::new(sheet, top, left).ok()?;
        let last = CellKey::new(sheet, bottom, right).ok()?;
        GridArea::new(first, last).ok()
    }
    fn add_shape(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        kind: ShapeKind,
        geometry_name: &str,
        rect: [f64; 4],
        flips: (bool, bool),
    ) -> Reply {
        let (workbook, sheet, _) = Self::report_key(target)?;
        let [left, top, width, height] = rect;
        // Excel: a negative size is 1004 and makes no shape; zero is accepted (dc-piw0 Q2).
        if width < 0.0 || height < 0.0 {
            return Err(AutoError::excel(1004, "The specified value is out of range."));
        }
        let mut id = None;
        self.command(call, |s| {
            id = Some(s.host().next_drawing_id(sheet));
            Ok(WorkbookCommand::Shape {
                sheet,
                revision: s.host().revision().get(),
                change: ShapeChange::Add {
                    kind,
                    geometry: geometry_name.into(),
                    left: Points::new(left),
                    top: Points::new(top),
                    width: Points::new(width),
                    height: Points::new(height),
                    flips,
                    name: None,
                },
            })
        })?;
        let id = id.ok_or_else(busy)?;
        Ok(report(workbook, sheet, "Shape", ReportItem::Shape(id)))
    }
    fn rect_args(call: &Call<'_>, first: usize) -> Result<[f64; 4], AutoError> {
        if call.args.len() != first + 4 {
            return Err(geometry::bad_count());
        }
        let mut rect = [0.0; 4];
        for (i, slot) in rect.iter_mut().enumerate() {
            *slot = number(call.arg(first + i))?;
        }
        Ok(rect)
    }
    /// `Shapes.AddShape(Type, Left, Top, Width, Height)`: rectangle, rounded rectangle and oval
    /// (the presets whose connection sites are modelled); other `MsoAutoShapeType`s refuse.
    pub(super) fn shapes_add_shape(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let rect = Self::rect_args(call, 1)?;
        let geometry_name = match geometry::integer(call.arg(0))? {
            1 => "rect",
            5 => "roundRect",
            9 => "ellipse",
            other => {
                return Err(AutoError::not_modelled(
                    format!("Shapes.AddShape MsoAutoShapeType {other}"),
                    SHAPES,
                ));
            }
        };
        self.add_shape(
            target,
            call,
            ShapeKind::Shape,
            geometry_name,
            rect,
            (false, false),
        )
    }
    /// `Shapes.AddTextbox(Orientation, Left, Top, Width, Height)`: horizontal text boxes.
    pub(super) fn shapes_add_textbox(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let rect = Self::rect_args(call, 1)?;
        if geometry::integer(call.arg(0))? != 1 {
            return Err(AutoError::not_modelled(
                "Shapes.AddTextbox vertical orientation",
                SHAPES,
            ));
        }
        self.add_shape(
            target,
            call,
            ShapeKind::TextBox,
            "rect",
            rect,
            (false, false),
        )
    }
    /// A line from (x1, y1) to (x2, y2): its rectangle is the corners', flipped as drawn.
    fn line_rect(a: [f64; 4]) -> ([f64; 4], (bool, bool)) {
        let [x1, y1, x2, y2] = a;
        (
            [x1.min(x2), y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs()],
            (x1 > x2, y1 > y2),
        )
    }
    /// `Shapes.AddLine(BeginX, BeginY, EndX, EndY)`.
    pub(super) fn shapes_add_line(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let (rect, flips) = Self::line_rect(Self::rect_args(call, 0)?);
        self.add_shape(target, call, ShapeKind::Connector, "line", rect, flips)
    }
    /// `Shapes.AddConnector(Type, BeginX, BeginY, EndX, EndY)`: straight connectors; elbow and
    /// curved ones need their adjust handles (dc-zjsg.5).
    pub(super) fn shapes_add_connector(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let (rect, flips) = Self::line_rect(Self::rect_args(call, 1)?);
        if geometry::integer(call.arg(0))? != 1 {
            return Err(AutoError::not_modelled(
                "Shapes.AddConnector elbow and curved connectors",
                "dc-zjsg.5",
            ));
        }
        // Excel names it "Straight Arrow Connector N" and saves `straightConnector1` (Q0, Q1).
        self.add_shape(
            target,
            call,
            ShapeKind::Connector,
            "straightConnector1",
            rect,
            flips,
        )
    }
    /// The chart part when the shape object is a chart in `Shapes`.
    fn chart_shape_part(target: &ObjectRef) -> Option<String> {
        match &target.key {
            ObjectKey::Report {
                item: ReportItem::ChartShape(part),
                ..
            } => Some(part.clone()),
            _ => None,
        }
    }
    pub(super) fn shape_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if Self::chart_shape_part(target).is_some() {
            return self.chart_object_name(target, call);
        }
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            Ok(AutoValue::text(&Self::shape_of(s, sheet, id)?.name))
        })
    }
    pub(super) fn shape_put_name(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if Self::chart_shape_part(target).is_some() {
            return self.chart_object_put_name(target, call);
        }
        let id = Self::shape_id_of(target)?;
        let name = text_arg(call.args.last().ok_or_else(geometry::bad_count)?)?;
        // Excel: an empty name is out of range; a name another shape has is accepted (Q3).
        if name.is_empty() {
            return Err(AutoError::excel(
                -2147024809,
                "The specified value is out of range.",
            ));
        }
        self.shape_command(target, call, ShapeChange::Name { id, name })
    }
    pub(super) fn shape_id(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if let Some(part) = Self::chart_shape_part(target) {
            return self
                .real_chart(target, |c| c.drawing_id)?
                .map(|id| AutoValue::I4(id as i32))
                .ok_or_else(|| {
                    AutoError::not_modelled(
                        format!("Shape.ID of the chart {part} the file holds"),
                        CHARTS,
                    )
                });
        }
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            Ok(AutoValue::I4(Self::shape_of(s, sheet, id)?.id as i32))
        })
    }
    pub(super) fn shape_delete(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if Self::chart_shape_part(target).is_some() {
            return Err(AutoError::not_modelled("Shape.Delete of a chart", CHARTS));
        }
        let id = Self::shape_id_of(target)?;
        // A shape that is gone is Excel's "Object required" (424), also when deleted twice (Q3).
        self.read_host(target, |s, sheet, _| {
            Self::shape_of(s, sheet, id).map(|_| ())
        })?;
        self.shape_command(target, call, ShapeChange::Delete { id })
    }
    fn shape_side(&self, target: &ObjectRef, call: &Call<'_>, side: Side) -> Reply {
        no_args(call)?;
        if let Some(part) = Self::chart_shape_part(target) {
            return self.read_host(target, |s, sheet, _| {
                Ok(single(Self::chart_rectangle(s, sheet, &part)?[side as usize]))
            });
        }
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            let shape = Self::shape_of(s, sheet, id)?;
            let (l, t, w, h) = s.host().shape_rectangle(sheet, &shape);
            Ok(single([l, t, w, h][side as usize]))
        })
    }
    fn shape_put_side(&self, target: &ObjectRef, call: &Call<'_>, side: Side) -> Reply {
        if Self::chart_shape_part(target).is_some() {
            return self.chart_put_side(target, call, side);
        }
        let id = Self::shape_id_of(target)?;
        let value = number(call.args.last().ok_or_else(geometry::bad_count)?)?;
        let [left, top, width, height] = rectangle_with(side, value);
        self.shape_command(
            target,
            call,
            ShapeChange::Place {
                id,
                left,
                top,
                width,
                height,
            },
        )
    }
    pub(super) fn shape_left(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_side(t, c, Side::Left)
    }
    pub(super) fn shape_top(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_side(t, c, Side::Top)
    }
    pub(super) fn shape_width(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_side(t, c, Side::Width)
    }
    pub(super) fn shape_height(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_side(t, c, Side::Height)
    }
    pub(super) fn shape_put_left(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_put_side(t, c, Side::Left)
    }
    pub(super) fn shape_put_top(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_put_side(t, c, Side::Top)
    }
    pub(super) fn shape_put_width(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_put_side(t, c, Side::Width)
    }
    pub(super) fn shape_put_height(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_put_side(t, c, Side::Height)
    }
    fn shape_child(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        interface: &str,
        make: fn(u32) -> ReportItem,
    ) -> Reply {
        no_args(call)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            Self::shape_of(s, sheet, id).map(|_| ())
        })?;
        Ok(report(workbook, sheet, interface, make(id)))
    }
    pub(super) fn shape_text_frame(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_child(t, c, "TextFrame2", ReportItem::TextFrame2)
    }
    pub(super) fn shape_fill(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_child(t, c, "FillFormat", ReportItem::FillFormat)
    }
    pub(super) fn shape_line(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_child(t, c, "LineFormat", ReportItem::LineFormat)
    }
    pub(super) fn shape_connector_format(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_child(t, c, "ConnectorFormat", ReportItem::ConnectorFormat)
    }
    pub(super) fn frame_text_range(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.shape_child(t, c, "TextRange2", ReportItem::TextRange2)
    }
    /// The shape's text as Excel gives it: paragraphs joined by a line feed (dc-piw0 Q8: "a" and
    /// "b" typed with vbCr read back as "a" LF "b").
    pub(super) fn range_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            let shape = Self::shape_of(s, sheet, id)?;
            Ok(AutoValue::text(&shape.text.unwrap_or_default().join("\n")))
        })
    }
    pub(super) fn range_put_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let id = Self::shape_id_of(target)?;
        let text = text_arg(call.args.last().ok_or_else(geometry::bad_count)?)?;
        self.shape_command(target, call, ShapeChange::Text { id, text })
    }
    fn colour_child(&self, target: &ObjectRef, call: &Call<'_>, line: bool) -> Reply {
        no_args(call)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        let id = Self::shape_id_of(target)?;
        Ok(report(
            workbook,
            sheet,
            "ColorFormat",
            ReportItem::ColorFormat(id, line),
        ))
    }
    pub(super) fn fill_colour(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.colour_child(t, c, false)
    }
    pub(super) fn line_colour(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.colour_child(t, c, true)
    }
    pub(super) fn colour_rgb(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let ObjectKey::Report {
            item: ReportItem::ColorFormat(id, line),
            ..
        } = &target.key
        else {
            return Err(geometry::type_mismatch());
        };
        let (id, line) = (*id, *line);
        self.read_host(target, |s, sheet, _| {
            let shape = Self::shape_of(s, sheet, id)?;
            let colour = if line {
                shape.line.colour.clone()
            } else {
                match shape.fill {
                    ShapeFill::Solid(c) => Some(c),
                    _ => None,
                }
            };
            match colour {
                Some(ShapeColour::Rgb(v)) => Ok(AutoValue::I4(colour_to_rgb(v) as i32)),
                // A colour the style or a theme decides: Excel's RGB for it is not recorded.
                _ => Err(AutoError::not_modelled(
                    "ColorFormat.RGB of a style or theme colour",
                    SHAPES,
                )),
            }
        })
    }
    pub(super) fn colour_put_rgb(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let ObjectKey::Report {
            item: ReportItem::ColorFormat(id, line),
            ..
        } = &target.key
        else {
            return Err(geometry::type_mismatch());
        };
        let (id, line) = (*id, *line);
        let colour = rgb_to_colour(geometry::integer(
            call.args.last().ok_or_else(geometry::bad_count)?,
        )?)?;
        let (_, sheet, _) = Self::report_key(target)?;
        self.command(call, |s| {
            let shape = Self::shape_of(s, sheet, id)?;
            let change = if line {
                let mut next: ShapeLine = shape.line.clone();
                next.colour = Some(colour.clone());
                next.hidden = false;
                ShapeChange::Line { id, line: next }
            } else {
                ShapeChange::Fill {
                    id,
                    fill: ShapeFill::Solid(colour.clone()),
                }
            };
            Ok(WorkbookCommand::Shape {
                sheet,
                revision: s.host().revision().get(),
                change,
            })
        })
    }
    pub(super) fn line_weight(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            match Self::shape_of(s, sheet, id)?.line.weight {
                Some(w) => Ok(single(w as f64 / EMU_PER_POINT)),
                None => Err(AutoError::not_modelled(
                    "LineFormat.Weight of a style-driven line",
                    SHAPES,
                )),
            }
        })
    }
    pub(super) fn line_put_weight(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        let id = Self::shape_id_of(target)?;
        let weight = number(call.args.last().ok_or_else(geometry::bad_count)?)?;
        if !(0.0..=1584.0).contains(&weight) {
            return Err(AutoError::excel(1004, ""));
        }
        let (_, sheet, _) = Self::report_key(target)?;
        self.command(call, |s| {
            let mut line = Self::shape_of(s, sheet, id)?.line;
            line.weight = Some((weight * EMU_PER_POINT).round() as i64);
            Ok(WorkbookCommand::Shape {
                sheet,
                revision: s.host().revision().get(),
                change: ShapeChange::Line { id, line },
            })
        })
    }
    /// `Shape.Type`: msoChart 3, msoAutoShape 1, msoTextBox 17, msoLine 9 (a line; a connector
    /// is an auto shape, Q0).
    pub(super) fn shape_type(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if Self::chart_shape_part(target).is_some() {
            return Ok(AutoValue::I4(3));
        }
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            let shape = Self::shape_of(s, sheet, id)?;
            Ok(AutoValue::I4(match (shape.kind, shape.geometry.as_deref()) {
                (ShapeKind::TextBox, _) => 17,
                (ShapeKind::Connector, Some("line")) => 9,
                _ => 1,
            }))
        })
    }
    /// `Shape.AutoShapeType`: msoShapeRectangle 1, rounded rectangle 5, oval 9; -2 (mixed) for
    /// connectors, lines and charts.
    pub(super) fn shape_auto_shape_type(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if Self::chart_shape_part(target).is_some() {
            return Ok(AutoValue::I4(-2));
        }
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            let shape = Self::shape_of(s, sheet, id)?;
            Ok(AutoValue::I4(match (shape.kind, shape.geometry.as_deref()) {
                (ShapeKind::Connector, _) => -2,
                (_, Some("roundRect")) => 5,
                (_, Some("ellipse")) => 9,
                (_, Some("rect") | None) => 1,
                (_, Some(other)) => {
                    return Err(AutoError::not_modelled(
                        format!("Shape.AutoShapeType of the preset {other}"),
                        SHAPES,
                    ));
                }
            }))
        })
    }
    /// `Shape.Connector`: msoTrue for a line or connector.
    pub(super) fn shape_connector(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        if Self::chart_shape_part(target).is_some() {
            return Ok(AutoValue::I4(0));
        }
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            let shape = Self::shape_of(s, sheet, id)?;
            Ok(AutoValue::I4(if shape.kind == ShapeKind::Connector { -1 } else { 0 }))
        })
    }
    pub(super) fn shape_horizontal_flip(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            let flipped = Self::shape_of(s, sheet, id)?.flip_horizontal;
            Ok(AutoValue::I4(if flipped { -1 } else { 0 }))
        })
    }
    pub(super) fn shape_vertical_flip(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            let flipped = Self::shape_of(s, sheet, id)?.flip_vertical;
            Ok(AutoValue::I4(if flipped { -1 } else { 0 }))
        })
    }
    /// `Shape.ConnectionSiteCount`: a rectangle 4, an oval 8, a line 2 (Q7).
    pub(super) fn shape_site_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            let shape = Self::shape_of(s, sheet, id)?;
            let geometry = shape.geometry.as_deref().unwrap_or("rect");
            crate::shapes::connection_site_count(geometry)
                .map(|n| AutoValue::I4(n as i32))
                .ok_or_else(|| {
                    AutoError::not_modelled(
                        format!("Shape.ConnectionSiteCount of the preset {geometry}"),
                        SHAPES,
                    )
                })
        })
    }
    /// `Shape.Chart` of a chart in `Shapes`.
    pub(super) fn shape_chart(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        no_args(call)?;
        let Some(part) = Self::chart_shape_part(target) else {
            return Err(AutoError::excel(1004, "This shape does not hold a chart."));
        };
        let (workbook, sheet, _) = Self::report_key(target)?;
        Ok(report(workbook, sheet, "Chart", ReportItem::Chart(part)))
    }
    fn connector_end_state(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        begin: bool,
        read: fn(Option<calc_doc_model::ShapeConnection>) -> Result<AutoValue, AutoError>,
    ) -> Reply {
        no_args(call)?;
        let id = Self::shape_id_of(target)?;
        self.read_host(target, |s, sheet, _| {
            let shape = Self::shape_of(s, sheet, id)?;
            read(if begin { shape.begin } else { shape.end })
        })
    }
    pub(super) fn connector_begin_connected(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.connector_end_state(t, c, true, |e| Ok(AutoValue::I4(if e.is_some() { -1 } else { 0 })))
    }
    pub(super) fn connector_end_connected(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.connector_end_state(t, c, false, |e| Ok(AutoValue::I4(if e.is_some() { -1 } else { 0 })))
    }
    pub(super) fn connector_begin_site(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.connector_end_state(t, c, true, Self::site_of)
    }
    pub(super) fn connector_end_site(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.connector_end_state(t, c, false, Self::site_of)
    }
    /// The 1-based connection site an end is glued to; an unglued end has none.
    fn site_of(end: Option<calc_doc_model::ShapeConnection>) -> Result<AutoValue, AutoError> {
        end.map(|e| AutoValue::I4(e.site as i32 + 1))
            .ok_or_else(|| AutoError::excel(1004, "Application-defined or object-defined error"))
    }
    fn connector_end_shape(&self, target: &ObjectRef, call: &Call<'_>, begin: bool) -> Reply {
        no_args(call)?;
        let (workbook, sheet, _) = Self::report_key(target)?;
        let id = Self::shape_id_of(target)?;
        let glued = self.read_host(target, |s, sheet, _| {
            let shape = Self::shape_of(s, sheet, id)?;
            Ok(if begin { shape.begin } else { shape.end })
        })?;
        let glued = glued
            .ok_or_else(|| AutoError::excel(1004, "Application-defined or object-defined error"))?;
        Ok(report(workbook, sheet, "Shape", ReportItem::Shape(glued.shape)))
    }
    pub(super) fn connector_begin_shape(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.connector_end_shape(t, c, true)
    }
    pub(super) fn connector_end_shape_get(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.connector_end_shape(t, c, false)
    }
    fn connect(&self, target: &ObjectRef, call: &Call<'_>, begin: bool) -> Reply {
        if call.args.len() != 2 {
            return Err(geometry::bad_count());
        }
        let id = Self::shape_id_of(target)?;
        let AutoValue::Object(ObjectRef {
            key:
                ObjectKey::Report {
                    item: ReportItem::Shape(shape),
                    ..
                },
            ..
        }) = call.arg(0)
        else {
            return Err(geometry::type_mismatch());
        };
        let invalid = || {
            AutoError::excel(
                -2147024809,
                "The specified parameter has an invalid value.",
            )
        };
        let site = u32::try_from(geometry::integer(call.arg(1))?).map_err(|_| invalid())?;
        // Excel (dc-piw0 Q3): a site the shape does not have, and the connector's own shape, are
        // an invalid parameter; a chart is a type mismatch (it is not a `ReportItem::Shape`).
        let sites = self.read_host(target, |s, sheet, _| {
            let glued = Self::shape_of(s, sheet, *shape)?;
            let geometry = glued.geometry.as_deref().unwrap_or("rect");
            crate::shapes::connection_site_count(geometry).ok_or_else(|| {
                AutoError::not_modelled(
                    format!("Connection sites of the preset {geometry}"),
                    SHAPES,
                )
            })
        })?;
        if *shape == id || site == 0 || site as usize > sites {
            return Err(invalid());
        }
        self.shape_command(
            target,
            call,
            ShapeChange::Connect {
                id,
                begin,
                shape: *shape,
                site,
            },
        )
    }
    pub(super) fn connector_begin(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.connect(t, c, true)
    }
    pub(super) fn connector_end(&self, t: &ObjectRef, c: &Call<'_>) -> Reply {
        self.connect(t, c, false)
    }
}

/// How `Chart.SetSourceData` reads a range: along rows or columns, with series names and
/// categories.
struct SourceLayout {
    rows: bool,
    headers: bool,
    categories: bool,
}

/// Whether `value` is a member of `XlChartType`.
fn is_xl_chart_type(value: i64) -> bool {
    matches!(
        value,
        1 | 4 | 5 | 15 | 51..=123 | 140
    ) || matches!(
        value,
        -4098 | -4100 | -4101 | -4102 | -4111 | -4120 | -4151 | -4152 | -4169
    )
}

/// `XlChartType` to the chart commands' kind (and whether stacked). Kinds the chart writer does not
/// save refuse naming dc-zjsg.6.
fn chart_kind(xl_type: i64) -> Result<(&'static str, bool), AutoError> {
    Ok(match xl_type {
        51 => ("column", false),
        52 => ("column", true),
        57 => ("bar", false),
        58 => ("bar", true),
        4 => ("line", false),
        1 => ("area", false),
        76 => ("area", true),
        5 => ("pie", false),
        -4120 => ("doughnut", false),
        -4169 => ("scatter", false),
        other => {
            return Err(AutoError::not_modelled(
                format!("XlChartType {other}"),
                "dc-zjsg.6",
            ));
        }
    })
}
