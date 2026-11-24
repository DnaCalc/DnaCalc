//! Worksheet formatting uses the authored Format Cells command and style table.
use super::*;
use crate::{FormatPatch, SkinIntent, WorkbookCommand, format_samples, style_alignment};

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

impl WorkbookAutomation {
    pub(super) fn register_formatting(handlers: &mut Handlers<Self>) {
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
    }

    fn horizontal_alignment(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.read_alignment(target, call, AutoValue::I4, |alignment| {
            alignment_value(
                HORIZONTAL,
                alignment.and_then(|a| a.horizontal.as_deref()),
                call,
            )
        })
    }

    fn vertical_alignment(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.read_alignment(target, call, AutoValue::I4, |alignment| {
            alignment_value(
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

    fn read_alignment<T: PartialEq>(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        value: impl Fn(T) -> AutoValue,
        property: impl Fn(Option<&calc_doc_model::Alignment>) -> Result<T, AutoError>,
    ) -> Reply {
        no_args(call)?;
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
            let mut uniform = None;
            for at in samples {
                let current = property(style_alignment(host.styles(), host.cell_style(at)))?;
                if uniform.as_ref().is_some_and(|prior| *prior != current) {
                    return Ok(AutoValue::Null);
                }
                uniform = Some(current);
            }
            Ok(value(uniform.unwrap_or(property(None)?)))
        })
    }

    fn put_horizontal_alignment(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.put_format(target, call, || {
            Ok(FormatPatch {
                horizontal: Some(alignment_keyword(HORIZONTAL, call)?),
                ..Default::default()
            })
        })
    }

    fn put_vertical_alignment(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.put_format(target, call, || {
            Ok(FormatPatch {
                vertical: Some(alignment_keyword(VERTICAL, call)?),
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
        no_args(call)?;
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
        self.mutate_areas(target, call, |_, range| {
            if call.args.len() != 1 {
                return Err(geometry::bad_count());
            }
            let patch = patch()?;
            Ok(SkinIntent::Command(WorkbookCommand::FormatCells {
                areas: range.areas().to_vec(),
                patch,
            }))
        })
    }
}

fn alignment_value(
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

fn alignment_keyword(values: &[(i32, &str)], call: &Call<'_>) -> Result<String, AutoError> {
    let value = super::report_draw::number(call.arg(0)).map_err(|_| {
        AutoError::not_modelled(
            format!("{} coercion to an alignment constant", call.label()),
            OWNER,
        )
    })?;
    values
        .iter()
        .find(|(constant, _)| f64::from(*constant) == value)
        .map(|(_, name)| (*name).to_owned())
        .ok_or_else(|| AutoError::excel(1004, ""))
}
