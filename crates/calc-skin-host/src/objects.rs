use crate::query::{QueryWork, checked_sheet, raw, render_chart_image};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use calc_doc_model::{Shape, ShapeKind, ShapePicture};
use calc_host::{ChartCorner, ChartKind, ChartPlace, HostState, LookColor};
use calc_skin::grid::{Key, Sheet};
use calc_skin::{
    ChartData, ChartObject, ChartPlotData, ChartPlotKind, ChartSeriesData, DrawingKind,
    DrawingObject, FormControlObject, ObjectColor, ObjectCorner, ObjectFrame, PictureObject,
    Refusal, SourceRef, SparklineObject, WorksheetObjects,
};
use std::collections::BTreeMap;

pub(crate) fn worksheet_objects(
    host: &HostState,
    sheet: Sheet,
    meter: &mut QueryWork,
) -> Result<WorksheetObjects, Refusal> {
    let id = checked_sheet(host, sheet)?;
    let loaded = host.loaded().sheet(id);
    let unread_drawings = loaded.map_or(0, |sheet| sheet.shapes().unread);
    let definitions = host.model().definitions();
    let chart_count = definitions.charts(id).len();
    let sparkline_count = definitions
        .sparklines(id)
        .iter()
        .map(|group| group.sparklines.len())
        .sum::<usize>();
    let control_count = loaded.map_or(0, |sheet| sheet.shapes().controls.len());
    meter.work(0, (chart_count + sparkline_count + control_count) as u64, 0)?;

    let (views, unread_charts) = host.charts(id);
    let charts = views
        .into_iter()
        .map(|chart| {
            let image = render_chart_image(&chart, None, meter)?;
            let plots = chart
                .plots
                .into_iter()
                .map(|plot| ChartPlotData {
                    kind: plot_kind(plot.kind),
                    series: plot
                        .series
                        .into_iter()
                        .map(|series| ChartSeriesData {
                            name: series.name,
                            categories: series
                                .categories
                                .iter()
                                .map(|point| point.as_ref().map(raw))
                                .collect(),
                            outer_levels: series.outer_levels,
                            values: bits(series.values),
                            sizes: bits(series.sizes),
                            uses_saved_values: series.cached,
                        })
                        .collect(),
                })
                .collect();
            Ok(ChartObject {
                source: SourceRef::Object {
                    sheet,
                    name: chart.name,
                },
                frame: frame(chart.place),
                image,
                data: ChartData { plots },
            })
        })
        .collect::<Result<_, Refusal>>()?;

    let drawings = host
        .shapes(id)
        .iter()
        .map(|shape| drawing(host, sheet, shape, meter))
        .collect::<Result<_, _>>()?;

    let mut sparklines = Vec::new();
    for (group, views) in host.sparklines(id) {
        let kind = group.kind.as_deref().unwrap_or("line");
        let color = group
            .colour("colorSeries")
            .and_then(|color| host.sparkline_colour(color))
            .map(|color| match color {
                LookColor::Rgb(rgb) => ObjectColor::Rgb(rgb),
                LookColor::Indexed(index) => ObjectColor::Indexed(index),
            });
        for line in views {
            meter.work((line.values.len() + line.dates.len()) as u64, 0, 0)?;
            sparklines.push(SparklineObject {
                at: Key::of(line.cell),
                kind: kind.into(),
                marker_points: group.markers,
                color: color.clone(),
                values: bits(line.values),
                dates: bits(line.dates),
            });
        }
    }

    let links: BTreeMap<_, _> = host
        .form_control_links(id)
        .map(|(control, at)| (control.id, Key::of(at)))
        .collect();
    let controls = host
        .form_controls(id)
        .into_iter()
        .map(|view| {
            let control = view.control;
            let linked_cell = links.get(&control.id).copied();
            meter.work(
                view.items.len() as u64 + u64::from(linked_cell.is_some()),
                0,
                0,
            )?;
            Ok(FormControlObject {
                id: control.id,
                name: control.name,
                kind: control.kind,
                frame: host.frame_place(id, &control.anchor, None).map(frame),
                caption: control.caption,
                linked_cell,
                linked_value: linked_cell.and(view.linked.as_ref().map(raw)),
                items: view.items,
                min: control.min,
                max: control.max,
                increment: control.increment,
                has_macro: control.macro_name.is_some(),
            })
        })
        .collect::<Result<_, Refusal>>()?;
    Ok(WorksheetObjects {
        charts,
        drawings,
        sparklines,
        controls,
        unread_charts,
        unread_drawings,
    })
}

fn drawing(
    host: &HostState,
    sheet: Sheet,
    shape: &Shape,
    meter: &mut QueryWork,
) -> Result<DrawingObject, Refusal> {
    meter.work(0, 1, 0)?;
    let kind = match shape.kind {
        ShapeKind::Shape => DrawingKind::Shape,
        ShapeKind::TextBox => DrawingKind::TextBox,
        ShapeKind::Picture => DrawingKind::Picture,
        ShapeKind::Connector => DrawingKind::Connector,
        ShapeKind::Group => DrawingKind::Group,
    };
    let picture = shape
        .media
        .as_ref()
        .map(|media| picture(media, meter))
        .transpose()?;
    let children = shape
        .children
        .iter()
        .map(|child| drawing(host, sheet, child, meter))
        .collect::<Result<_, _>>()?;
    Ok(DrawingObject {
        id: shape.id,
        name: shape.name.clone(),
        kind,
        frame: host
            .frame_place(sheet.id(), &shape.anchor, shape.edit_as.as_deref())
            .map(frame),
        hidden: shape.hidden,
        geometry: shape.geometry.clone(),
        text: shape.text.clone(),
        picture,
        children,
    })
}

fn picture(picture: &ShapePicture, meter: &mut QueryWork) -> Result<PictureObject, Refusal> {
    let media = picture.image.as_ref().or(picture.svg.as_ref());
    let mut data_url = None;
    if let Some(image) = media
        && !image.bytes.is_empty()
        && matches!(
            image.content_type.as_str(),
            "image/png" | "image/jpeg" | "image/gif" | "image/bmp" | "image/webp"
        )
    {
        let prefix = format!("data:{};base64,", image.content_type);
        meter.charge_payload(prefix.len() as u64 + 4 * (image.bytes.len().div_ceil(3) as u64))?;
        data_url = Some(format!("{prefix}{}", STANDARD.encode(&image.bytes)));
    }
    Ok(PictureObject {
        description: picture.description.clone(),
        crop: picture.crop,
        content_type: media.map(|image| image.content_type.clone()),
        stored_bytes: media.map(|image| image.size),
        data_url,
    })
}

fn bits(values: Vec<Option<f64>>) -> Vec<Option<u64>> {
    values
        .into_iter()
        .map(|point| point.map(f64::to_bits))
        .collect()
}

fn frame(place: ChartPlace) -> ObjectFrame {
    let corner = |corner: ChartCorner| ObjectCorner {
        cell: Key::of(corner.cell),
        dx: corner.dx,
        dy: corner.dy,
    };
    match place {
        ChartPlace::Cells { from, to } => ObjectFrame::Cells {
            from: corner(from),
            to: corner(to),
        },
        ChartPlace::Sized {
            from,
            width,
            height,
        } => ObjectFrame::Sized {
            from: corner(from),
            width,
            height,
        },
        ChartPlace::Absolute {
            x,
            y,
            width,
            height,
        } => ObjectFrame::Absolute {
            x,
            y,
            width,
            height,
        },
    }
}

fn plot_kind(kind: ChartKind) -> ChartPlotKind {
    match kind {
        ChartKind::Column => ChartPlotKind::Column,
        ChartKind::Bar => ChartPlotKind::Bar,
        ChartKind::Line => ChartPlotKind::Line,
        ChartKind::Area => ChartPlotKind::Area,
        ChartKind::Pie => ChartPlotKind::Pie,
        ChartKind::Doughnut => ChartPlotKind::Doughnut,
        ChartKind::Scatter => ChartPlotKind::Scatter,
        ChartKind::Radar => ChartPlotKind::Radar,
        ChartKind::Bubble => ChartPlotKind::Bubble,
        ChartKind::Stock => ChartPlotKind::Stock,
        ChartKind::Waterfall => ChartPlotKind::Waterfall,
        ChartKind::Funnel => ChartPlotKind::Funnel,
        ChartKind::BoxWhisker => ChartPlotKind::BoxWhisker,
        ChartKind::Histogram => ChartPlotKind::Histogram,
        ChartKind::Surface => ChartPlotKind::Surface,
        ChartKind::Treemap => ChartPlotKind::Treemap,
        ChartKind::Sunburst => ChartPlotKind::Sunburst,
        ChartKind::Other(name) => ChartPlotKind::Other(name),
    }
}

/// Describe the literal editor for an unambiguous, linked worksheet control.
pub(crate) fn input_editor(
    host: &HostState,
    at: calc_host::value::CellKey,
    meter: &mut QueryWork,
) -> Result<calc_skin::query::InputEditor, Refusal> {
    use calc_skin::query::{InputChoice, InputEditor};

    let mut linked = None;
    for sheet in host.sheets() {
        for (control, cell) in host.form_control_links(sheet.id()) {
            meter.work(0, 1, 0)?;
            if cell == at {
                if linked.is_some() || control.macro_name.is_some() {
                    return Ok(InputEditor::Field);
                }
                linked = Some((sheet.id(), control));
            }
        }
    }
    let Some((sheet, control)) = linked else { return Ok(InputEditor::Field) };
    Ok(match control.kind.as_str() {
        "CheckBox" if control.checked.as_deref() != Some("Mixed") => InputEditor::Checkbox,
        "Scroll" | "Spin" => {
            // MS-XLSX §2.6.65 supplies defaults for min and inc.
            let min = control.min.unwrap_or(0);
            let step = control.increment.unwrap_or(1);
            let Some(max) = control.max.filter(|max| min < *max && *max <= 30_000) else {
                return Ok(InputEditor::Field);
            };
            if min < 0 || !(1..=30_000).contains(&step) {
                return Ok(InputEditor::Field);
            }
            if control.kind == "Scroll" {
                InputEditor::Slider { min, max, step }
            } else {
                InputEditor::Stepper { min, max, step }
            }
        }
        "Drop" => {
            let Some(view) = host.form_controls(sheet).into_iter().find(|view| view.control.id == control.id) else {
                return Ok(InputEditor::Field);
            };
            meter.work(view.items.len() as u64, 0, 0)?;
            if view.items.is_empty() {
                return Ok(InputEditor::Field);
            }
            InputEditor::Choices(view.items.into_iter().enumerate().map(|(index, label)| {
                InputChoice { label, entry: (index + 1).to_string() }
            }).collect())
        }
        _ => InputEditor::Field,
    })
}
