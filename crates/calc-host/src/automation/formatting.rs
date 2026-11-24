//! Worksheet formatting uses the authored Format Cells command and style table.
use super::*;
use crate::{FormatPatch, SkinIntent, WorkbookCommand, format_samples};

const OWNER: &str = "dc-oxh8w.2";

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
        self.mutate_areas(target, call, |_, range| {
            if call.args.len() != 1 {
                return Err(geometry::bad_count());
            }
            let AutoValue::Text(text) = call.arg(0) else {
                return Err(AutoError::not_modelled(
                    "Range.NumberFormat coercion to a format code",
                    OWNER,
                ));
            };
            let code = String::from_utf16(text.utf16_code_units()).map_err(|_| {
                AutoError::not_modelled("Range.NumberFormat with unpaired UTF-16", OWNER)
            })?;
            let patch = FormatPatch {
                number_format: Some(code),
                ..Default::default()
            };
            Ok(SkinIntent::Command(WorkbookCommand::FormatCells {
                areas: range.areas().to_vec(),
                patch,
            }))
        })
    }
}
