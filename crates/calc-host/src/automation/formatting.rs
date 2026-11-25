//! Worksheet formatting uses the authored Format Cells command and style table.
use super::*;
use crate::{
    Axis, AxisFormat, FormatPatch, HostState, SkinIntent, WorkbookCommand, format_samples,
    style_alignment,
};

mod font;
mod interior;
mod borders;
mod styles;

const OWNER: &str = "dc-oxh8w.2";

// Excel's XlHAlign and XlVAlign values paired with their OOXML spellings.
const HORIZONTAL: &[(i32, &str)] = &[
    (1, "general"),
    (-4131, "left"),
    (-4108, "center"),
    (-4152, "right"),
    (5, "fill"),
    (-4130, "justify"),
    (7, "centerContinuous"),
    (-4117, "distributed"),
];
const VERTICAL: &[(i32, &str)] = &[
    (-4107, "bottom"),
    (-4160, "top"),
    (-4108, "center"),
    (-4130, "justify"),
    (-4117, "distributed"),
];
const XL_VERTICAL_TEXT: i32 = -4166;

impl WorkbookAutomation {
    pub(super) fn register_formatting(handlers: &mut Handlers<Self>) {
        Self::register_range_font(handlers);
        Self::register_range_interior(handlers);
        Self::register_range_borders(handlers);
        Self::register_range_styles(handlers);
        handlers.on(
            "Range",
            "NumberFormat",
            InvokeKind::Get,
            Self::number_format,
        );
        handlers.on(
            "Range",
            "NumberFormat",
            InvokeKind::Put,
            Self::put_number_format,
        );
        handlers.on(
            "Range",
            "HorizontalAlignment",
            InvokeKind::Get,
            Self::horizontal_alignment,
        );
        handlers.on(
            "Range",
            "HorizontalAlignment",
            InvokeKind::Put,
            Self::put_horizontal_alignment,
        );
        handlers.on(
            "Range",
            "VerticalAlignment",
            InvokeKind::Get,
            Self::vertical_alignment,
        );
        handlers.on(
            "Range",
            "VerticalAlignment",
            InvokeKind::Put,
            Self::put_vertical_alignment,
        );
        handlers.on("Range", "WrapText", InvokeKind::Get, Self::wrap_text);
        handlers.on("Range", "WrapText", InvokeKind::Put, Self::put_wrap_text);
        handlers.on("Range", "IndentLevel", InvokeKind::Get, Self::indent_level);
        handlers.on(
            "Range",
            "IndentLevel",
            InvokeKind::Put,
            Self::put_indent_level,
        );
        handlers.on("Range", "Orientation", InvokeKind::Get, Self::orientation);
        handlers.on(
            "Range",
            "Orientation",
            InvokeKind::Put,
            Self::put_orientation,
        );
        handlers.on(
            "Range",
            "ColumnWidth",
            InvokeKind::Get,
            |host, target, call| host.dimension(target, call, Axis::Columns),
        );
        handlers.on(
            "Range",
            "ColumnWidth",
            InvokeKind::Put,
            |host, target, call| host.put_dimension(target, call, Axis::Columns),
        );
        handlers.on(
            "Range",
            "RowHeight",
            InvokeKind::Get,
            |host, target, call| host.dimension(target, call, Axis::Rows),
        );
        handlers.on(
            "Range",
            "RowHeight",
            InvokeKind::Put,
            |host, target, call| host.put_dimension(target, call, Axis::Rows),
        );
    }

    fn horizontal_alignment(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.read_alignment(target, call, AutoValue::I4, |alignment| {
            format_enum_value(
                HORIZONTAL,
                alignment.and_then(|a| a.horizontal.as_deref()),
                call,
            )
        })
    }

    fn vertical_alignment(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.read_alignment(target, call, AutoValue::I4, |alignment| {
            format_enum_value(
                VERTICAL,
                alignment.and_then(|a| a.vertical.as_deref()),
                call,
            )
        })
    }

    fn wrap_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.read_alignment(target, call, AutoValue::Bool, |alignment| {
            Ok(alignment.is_some_and(|a| a.wrap_text))
        })
    }

    fn indent_level(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.read_alignment(target, call, AutoValue::I4, |alignment| {
            let indent = alignment.and_then(|a| a.indent.as_deref()).unwrap_or("0");
            indent.parse().map_err(|_| {
                AutoError::not_modelled(format!("Range.IndentLevel stored as {indent}"), OWNER)
            })
        })
    }

    fn put_indent_level(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.put_format(target, call, || {
            let level = integral_format_arg(call)?;
            if !(0..=15).contains(&level) {
                return Err(format_error());
            }
            Ok(FormatPatch {
                indent: Some(level as u16),
                ..Default::default()
            })
        })
    }

    fn orientation(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.read_alignment(target, call, AutoValue::I4, |alignment| {
            let rotation = alignment
                .and_then(|a| a.text_rotation.as_deref())
                .unwrap_or("0");
            match rotation.parse::<i32>() {
                Ok(n @ 0..=90) => Ok(n),
                Ok(n @ 91..=180) => Ok(90 - n),
                Ok(255) => Ok(XL_VERTICAL_TEXT),
                _ => Err(AutoError::not_modelled(
                    format!("Range.Orientation stored as {rotation}"),
                    OWNER,
                )),
            }
        })
    }

    fn put_orientation(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.put_format(target, call, || {
            let rotation = match integral_format_arg(call)? {
                n @ -90..=90 => n as i16,
                -4128 => 0,   // xlHorizontal
                -4171 => 90,  // xlUpward
                -4170 => -90, // xlDownward
                XL_VERTICAL_TEXT => 255,
                _ => return Err(format_error()),
            };
            Ok(FormatPatch {
                rotation: Some(rotation),
                ..Default::default()
            })
        })
    }

    fn dimension(&self, target: &ObjectRef, call: &Call<'_>, axis: Axis) -> Reply {
        format_arity(call, 0)?;
        self.with_range(target, |access, _, range, _| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled(
                    format!("{} inside a worksheet invocation", call.label()),
                    OWNER,
                ));
            };
            let host = session.host();
            let sheet = range.sheet();
            check_dimension_font(host, axis, call)?;
            let size = |format| match axis {
                Axis::Rows => host.row_points_of(sheet, format),
                Axis::Columns => host.column_characters_of(sheet, format),
            };
            let layouts = host.model().definitions().axis_layouts();
            let mut uniform = None;
            for area in range.areas() {
                let (first, last) = axis_span(*area, axis);
                let mut explicit = 0;
                for (_, format) in layouts
                    .bands(sheet, axis)
                    .skip_while(|(index, _)| *index < first)
                    .take_while(|(index, _)| *index <= last)
                {
                    let current = size(format);
                    if *uniform.get_or_insert(current) != current {
                        return Ok(AutoValue::Null);
                    }
                    explicit += 1;
                }
                if explicit < last - first + 1 {
                    let default = size(AxisFormat::default());
                    if *uniform.get_or_insert(default) != default {
                        return Ok(AutoValue::Null);
                    }
                }
            }
            Ok(AutoValue::R8(
                uniform.unwrap_or_else(|| size(AxisFormat::default())),
            ))
        })
    }

    fn put_dimension(&self, target: &ObjectRef, call: &Call<'_>, axis: Axis) -> Reply {
        format_arity(call, 1)?;
        let size = super::report_draw::number(call.arg(0)).map_err(|_| format_error())?;
        let rows = axis == Axis::Rows;
        let limit = if rows { 409.0 } else { 255.0 };
        if !(0.0..=limit).contains(&size) {
            return Err(format_error());
        }
        self.mutate_areas_with_owner(target, call, OWNER, |session, range| {
            let [area] = range.areas() else {
                return Err(AutoError::not_modelled(
                    format!("{} with multiple areas", call.label()),
                    OWNER,
                ));
            };
            if !rows && size > 0.0 {
                check_dimension_font(session.host(), axis, call)?;
                if size < 1.0 {
                    return Err(AutoError::not_modelled(
                        "ColumnWidth between zero and one character",
                        OWNER,
                    ));
                }
            }
            let (first, last) = axis_span(*area, axis);
            Ok(SkinIntent::Command(WorkbookCommand::AxisFormat {
                sheet: area.sheet(),
                rows,
                first,
                count: last - first + 1,
                size_bits: Some(size.to_bits()),
                hidden: None,
            }))
        })
    }

    fn read_alignment<T: PartialEq>(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        value: impl Fn(T) -> AutoValue,
        property: impl Fn(Option<&calc_doc_model::Alignment>) -> Result<T, AutoError>,
    ) -> Reply {
        self.read_format(target, call, value, |host, style| {
            property(style_alignment(host.styles(), style))
        })
    }

    fn read_format<T: PartialEq>(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        value: impl Fn(T) -> AutoValue,
        property: impl Fn(&HostState, u32) -> Result<T, AutoError>,
    ) -> Reply {
        format_arity(call, 0)?;
        self.with_range(target, |access, _, range, _| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled(
                    format!("{} inside a worksheet invocation", call.label()),
                    OWNER,
                ));
            };
            let host = session.host();
            let samples = format_samples(host, range.areas())
                .map_err(|reason| AutoError::not_modelled(reason, OWNER))?;
            let mut values = samples
                .into_iter()
                .map(|at| property(host, host.cell_style(at)));
            let first = values.next().ok_or_else(format_error)??;
            for current in values {
                if current? != first {
                    return Ok(AutoValue::Null);
                }
            }
            Ok(value(first))
        })
    }

    fn put_horizontal_alignment(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.put_format(target, call, || {
            Ok(FormatPatch {
                horizontal: Some(format_enum_keyword(HORIZONTAL, call)?),
                ..Default::default()
            })
        })
    }

    fn put_vertical_alignment(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.put_format(target, call, || {
            Ok(FormatPatch {
                vertical: Some(format_enum_keyword(VERTICAL, call)?),
                ..Default::default()
            })
        })
    }

    fn put_wrap_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.put_format(target, call, || {
            let AutoValue::Bool(wrap) = call.arg(0) else {
                return Err(AutoError::not_modelled("Range.WrapText coercion", OWNER));
            };
            Ok(FormatPatch {
                wrap: Some(*wrap),
                ..Default::default()
            })
        })
    }

    fn number_format(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        self.with_range(target, |access, _, range, _| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled(
                    "Range.NumberFormat inside a worksheet invocation",
                    OWNER,
                ));
            };
            let host = session.host();
            let samples = format_samples(host, range.areas())
                .map_err(|reason| AutoError::not_modelled(reason, OWNER))?;
            let mut uniform = None;
            for at in samples {
                let code = host
                    .styles()
                    .number_format_code(host.cell_style(at))
                    .ok_or_else(|| {
                        AutoError::not_modelled(
                            "Range.NumberFormat for an unknown format ID",
                            OWNER,
                        )
                    })?;
                if uniform.is_some_and(|prior| prior != code) {
                    return Ok(AutoValue::Null);
                }
                uniform = Some(code);
            }
            Ok(AutoValue::text(uniform.unwrap_or("General")))
        })
    }

    fn put_number_format(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.put_format(target, call, || {
            let AutoValue::Text(text) = call.arg(0) else {
                return Err(AutoError::not_modelled(
                    "Range.NumberFormat coercion to a format code",
                    OWNER,
                ));
            };
            let code = String::from_utf16(text.utf16_code_units()).map_err(|_| {
                AutoError::not_modelled("Range.NumberFormat with unpaired UTF-16", OWNER)
            })?;
            Ok(FormatPatch {
                number_format: Some(code),
                ..Default::default()
            })
        })
    }

    fn put_format(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        patch: impl Fn() -> Result<FormatPatch, AutoError>,
    ) -> Reply {
        format_arity(call, 1)?;
        self.mutate_areas_with_owner(target, call, OWNER, |_, range| {
            let patch = patch()?;
            patch.validate().map_err(|_| format_error())?;
            Ok(SkinIntent::Command(WorkbookCommand::FormatCells {
                areas: range.areas().to_vec(),
                patch,
            }))
        })
    }
}

fn check_dimension_font(host: &HostState, axis: Axis, call: &Call<'_>) -> Result<(), AutoError> {
    if axis == Axis::Columns && crate::facts::maximum_digit_width_at(host.styles(), 96.0).is_none()
    {
        return Err(AutoError::not_modelled(
            format!("{} with an unmeasured Normal font", call.label()),
            OWNER,
        ));
    }
    Ok(())
}

fn axis_span(area: GridArea, axis: Axis) -> (u32, u32) {
    match axis {
        Axis::Rows => (area.first().row(), area.last().row()),
        Axis::Columns => (area.first().column(), area.last().column()),
    }
}

fn format_arity(call: &Call<'_>, expected: usize) -> Result<(), AutoError> {
    if call.args.len() != expected {
        return Err(format_error());
    }
    Ok(())
}

fn format_error() -> AutoError {
    AutoError::excel(1004, "Application-defined or object-defined error")
}

fn integral_format_arg(call: &Call<'_>) -> Result<i32, AutoError> {
    let value = super::report_draw::number(call.arg(0)).map_err(|_| format_error())?;
    let integer = value as i32;
    if f64::from(integer) != value {
        return Err(format_error());
    }
    Ok(integer)
}

fn format_enum_value(
    values: &[(i32, &str)],
    keyword: Option<&str>,
    call: &Call<'_>,
) -> Result<i32, AutoError> {
    let Some(keyword) = keyword else {
        return Ok(values[0].0);
    };
    values
        .iter()
        .find(|(_, name)| *name == keyword)
        .map(|(value, _)| *value)
        .ok_or_else(|| {
            AutoError::not_modelled(format!("{} stored as {keyword}", call.label()), OWNER)
        })
}

fn format_enum_keyword(values: &[(i32, &str)], call: &Call<'_>) -> Result<String, AutoError> {
    let value = integral_format_arg(call)?;
    values
        .iter()
        .find(|(constant, _)| *constant == value)
        .map(|(_, name)| (*name).to_owned())
        .ok_or_else(format_error)
}
