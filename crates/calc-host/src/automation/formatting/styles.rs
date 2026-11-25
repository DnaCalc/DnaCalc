//! `Range.Style` and the members of one `Style`: the Cell Styles gallery's apply command over named cell styles.
use super::*;

impl WorkbookAutomation {
    pub(super) fn register_range_styles(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Style", InvokeKind::Get, Self::range_style);
        handlers.on("Range", "Style", InvokeKind::Put, Self::put_range_style);
        for member in ["Name", "NameLocal", "BuiltIn", "NumberFormat"] {
            handlers.on("Style", member, InvokeKind::Get, Self::style_member);
        }
    }

    fn range_style(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        self.with_range(target, |access, meta, range, _| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled("Range.Style inside a worksheet invocation", OWNER));
            };
            let host = session.host();
            let samples = format_samples(host, range.areas()).map_err(|r| AutoError::not_modelled(r, OWNER))?;
            let styles = host.styles();
            let mut uniform = None;
            for at in samples {
                let parent = styles
                    .cell_formats
                    .get(host.cell_style(at) as usize)
                    .and_then(|xf| xf.xf_id)
                    .unwrap_or(0);
                let index = styles
                    .cell_styles
                    .iter()
                    .position(|s| s.xf_id == parent)
                    .ok_or_else(|| AutoError::not_modelled("Range.Style of a format without a named style", OWNER))?;
                if uniform.is_some_and(|prior| prior != index) {
                    return Ok(AutoValue::Null);
                }
                uniform = Some(index);
            }
            let index = uniform.ok_or_else(format_error)? as u32;
            Ok(AutoValue::Object(object(
                "Style",
                ObjectKey::Style {
                    workbook: meta.workbook,
                    index,
                },
            )))
        })
    }

    fn put_range_style(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let named = match call.arg(0) {
            AutoValue::Text(text) => Some(text.to_string_lossy()),
            AutoValue::Object(style) if matches!(style.key, ObjectKey::Style { .. }) => None,
            _ => return Err(format_error()),
        };
        let given = match call.arg(0) {
            AutoValue::Object(ObjectRef {
                key: ObjectKey::Style { index, workbook },
                ..
            }) => Some((*index, *workbook)),
            _ => None,
        };
        let workbook = self.metadata()?.workbook;
        self.mutate_areas_with_owner(target, call, OWNER, |session, range| {
            let styles = session.host().styles();
            let index = match (&named, given) {
                (Some(name), _) => styles
                    .cell_styles
                    .iter()
                    .position(|s| s.name.to_lowercase() == name.to_lowercase())
                    .ok_or_else(format_error)?,
                (None, Some((index, from))) if from == workbook => index as usize,
                _ => return Err(format_error()),
            };
            let xf_id = styles.cell_styles.get(index).ok_or_else(format_error)?.xf_id;
            Ok(SkinIntent::Command(WorkbookCommand::FormatCells {
                areas: range.areas().to_vec(),
                patch: FormatPatch {
                    style: Some(xf_id),
                    ..Default::default()
                },
            }))
        })
    }

    fn style_member(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let ObjectKey::Style { index, .. } = target.key else {
            return Err(geometry::type_mismatch());
        };
        self.with_target(target, |access, _| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled(format!("{} inside a worksheet invocation", call.label()), OWNER));
            };
            let host = session.host();
            let styles = host.styles();
            let style = styles.cell_styles.get(index as usize).ok_or_else(|| AutoError::excel(424, "Object required"))?;
            Ok(match call.member.name.as_str() {
                "Name" | "NameLocal" => AutoValue::text(&style.name),
                "BuiltIn" => AutoValue::Bool(style.builtin_id.is_some() && !style.custom_builtin),
                _ => {
                    let base = styles
                        .cell_style_formats
                        .get(style.xf_id as usize)
                        .ok_or_else(|| AutoError::not_modelled("Style.NumberFormat of a missing style format", OWNER))?;
                    let id = base.number_format_id.unwrap_or(0);
                    let code = styles
                        .number_formats
                        .iter()
                        .find(|n| n.id == id)
                        .map(|n| n.code.as_str())
                        .or_else(|| calc_doc_model::builtin_number_format(id))
                        .ok_or_else(|| AutoError::not_modelled("Style.NumberFormat for an unknown format ID", OWNER))?;
                    AutoValue::text(code)
                }
            })
        })
    }
}
