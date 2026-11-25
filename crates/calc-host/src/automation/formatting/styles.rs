//! `Range.Style` and the members of one `Style`: the Cell Styles gallery's apply command over named cell styles.
use super::*;

const STYLE_OWNER: &str = "dc-lj4j5";

impl WorkbookAutomation {
    pub(super) fn register_range_styles(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Style", InvokeKind::Get, Self::range_style);
        handlers.on("Range", "Style", InvokeKind::Put, Self::put_range_style);
        handlers.on("Workbook", "Styles", InvokeKind::Get, Self::workbook_styles);
        handlers.on("Styles", "Count", InvokeKind::Get, Self::styles_count);
        handlers.on("Styles", "Item", InvokeKind::Get, Self::styles_item);
        handlers.on("Styles", "_Default", InvokeKind::Get, Self::styles_item);
        handlers.on("Styles", "Add", InvokeKind::Method, Self::styles_add);
        handlers.on("Style", "Delete", InvokeKind::Method, Self::style_delete);
        handlers.on("Style", "Font", InvokeKind::Get, Self::style_font);
        handlers.on("Style", "Interior", InvokeKind::Get, Self::style_interior);
        handlers.on("Style", "NumberFormat", InvokeKind::Put, Self::style_put_number_format);
        for member in ["WrapText", "ShrinkToFit", "Locked", "FormulaHidden"] {
            handlers.on("Style", member, InvokeKind::Put, Self::style_put_flag);
        }
        for member in ["HorizontalAlignment", "VerticalAlignment", "IndentLevel", "Orientation"] {
            handlers.on("Style", member, InvokeKind::Put, Self::style_put_alignment);
        }
        for member in [
            "Name",
            "NameLocal",
            "BuiltIn",
            "NumberFormat",
            "Value",
            "_Default",
            "HorizontalAlignment",
            "VerticalAlignment",
            "WrapText",
            "ShrinkToFit",
            "IndentLevel",
            "Orientation",
            "Locked",
            "FormulaHidden",
        ] {
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
                "Name" | "NameLocal" | "Value" | "_Default" => AutoValue::text(&style.name),
                "HorizontalAlignment" | "VerticalAlignment" | "WrapText" | "ShrinkToFit" | "IndentLevel" | "Orientation"
                | "Locked" | "FormulaHidden" => {
                    let base = styles.cell_style_formats.get(style.xf_id as usize).ok_or_else(|| {
                        AutoError::not_modelled(format!("{} of a missing style format", call.label()), STYLE_OWNER)
                    })?;
                    let alignment = base.alignment.as_ref();
                    match call.member.name.as_str() {
                        "HorizontalAlignment" => {
                            AutoValue::I4(format_enum_value(HORIZONTAL, alignment.and_then(|a| a.horizontal.as_deref()), call)?)
                        }
                        "VerticalAlignment" => {
                            AutoValue::I4(format_enum_value(VERTICAL, alignment.and_then(|a| a.vertical.as_deref()), call)?)
                        }
                        "WrapText" => AutoValue::Bool(alignment.is_some_and(|a| a.wrap_text)),
                        "ShrinkToFit" => AutoValue::Bool(alignment.is_some_and(|a| a.shrink_to_fit)),
                        "IndentLevel" => {
                            let indent = alignment.and_then(|a| a.indent.as_deref()).unwrap_or("0");
                            AutoValue::I4(indent.parse().map_err(|_| {
                                AutoError::not_modelled(format!("Style.IndentLevel stored as {indent}"), STYLE_OWNER)
                            })?)
                        }
                        "Orientation" => {
                            let rotation = alignment.and_then(|a| a.text_rotation.as_deref()).unwrap_or("0");
                            AutoValue::I4(match rotation.parse::<i32>() {
                                Ok(n @ 0..=90) => n,
                                Ok(n @ 91..=180) => 90 - n,
                                Ok(255) => XL_VERTICAL_TEXT,
                                _ => {
                                    return Err(AutoError::not_modelled(
                                        format!("Style.Orientation stored as {rotation}"),
                                        STYLE_OWNER,
                                    ));
                                }
                            })
                        }
                        "Locked" => AutoValue::Bool(base.protection.as_ref().and_then(|p| p.locked).unwrap_or(true)),
                        _ => AutoValue::Bool(base.protection.as_ref().and_then(|p| p.hidden).unwrap_or(false)),
                    }
                }
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

    fn workbook_styles(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        self.with_target(target, |_, meta| Ok(AutoValue::Object(object("Styles", ObjectKey::Styles(meta.workbook)))))
    }

    fn styles_count(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        self.with_target(target, |access, _| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled("Styles.Count inside a worksheet invocation", STYLE_OWNER));
            };
            Ok(AutoValue::I4(session.host().styles().cell_styles.len() as i32))
        })
    }

    /// `Styles(Index)`: a one-based position or a name (compared without regard to case).
    fn styles_item(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        self.with_target(target, |access, meta| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled("Styles.Item inside a worksheet invocation", STYLE_OWNER));
            };
            let styles = session.host().styles();
            let found = match call.arg(0) {
                AutoValue::Text(name) => {
                    let name = name.to_string_lossy().to_lowercase();
                    styles.cell_styles.iter().position(|s| s.name.to_lowercase() == name)
                }
                other => {
                    let n = geometry::integer(other).map_err(|_| AutoError::excel(1004, "Application-defined or object-defined error"))?;
                    usize::try_from(n - 1).ok().filter(|n| *n < styles.cell_styles.len())
                }
            };
            let index = found.ok_or_else(|| AutoError::excel(1004, "Application-defined or object-defined error"))?;
            Ok(AutoValue::Object(object("Style", ObjectKey::Style { workbook: meta.workbook, index: index as u32 })))
        })
    }

    /// `Styles.Add(Name, BasedOn)`: a new style with Normal's format, or `BasedOn`'s first cell's format.
    fn styles_add(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.is_empty() || call.args.len() > 2 {
            return Err(geometry::bad_count());
        }
        let AutoValue::Text(name) = call.arg(0) else {
            return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
        };
        let name = name.to_string_lossy();
        let based_on = match call.arg(1) {
            AutoValue::Missing | AutoValue::Empty => None,
            AutoValue::Object(ObjectRef {
                key: ObjectKey::Range(held) | ObjectKey::RangeView { reference: held, .. },
                ..
            }) => {
                let mut cell_style = None;
                self.with_access(|mut access| {
                    let resolved = self.resolve(&mut access, held)?;
                    let Access::Session(session, _) = access else {
                        return Err(AutoError::not_modelled("Styles.Add inside a worksheet invocation", STYLE_OWNER));
                    };
                    cell_style = Some(session.host().cell_style(resolved.areas()[0].first()));
                    Ok(AutoValue::Empty)
                })?;
                cell_style
            }
            _ => return Err(format_error()),
        };
        let workbook = self.metadata()?.workbook;
        let _ = target;
        let index = self.with_write(|session| {
            let receipt = session.dispatch_from(
                crate::CommandIngress::Automation,
                SkinIntent::Command(WorkbookCommand::StyleTable {
                    change: crate::StyleChange::Add { name: name.clone(), based_on },
                }),
            );
            if receipt.error.is_some() {
                return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
            }
            self.context_changed(session);
            Ok(AutoValue::I4(session.host().styles().cell_styles.len() as i32 - 1))
        })?;
        let AutoValue::I4(index) = index else { return Err(format_error()) };
        Ok(AutoValue::Object(object("Style", ObjectKey::Style { workbook, index: index as u32 })))
    }

    fn style_font(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let ObjectKey::Style { workbook, index } = target.key else {
            return Err(geometry::type_mismatch());
        };
        self.with_target(target, |access, _| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled("Style.Font inside a worksheet invocation", STYLE_OWNER));
            };
            session
                .host()
                .styles()
                .cell_styles
                .get(index as usize)
                .ok_or_else(|| AutoError::excel(424, "Object required"))?;
            Ok(AutoValue::Object(object("Font", ObjectKey::Style { workbook, index })))
        })
    }

    fn style_interior(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let ObjectKey::Style { workbook, index } = target.key else {
            return Err(geometry::type_mismatch());
        };
        self.with_target(target, |access, _| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled("Style.Interior inside a worksheet invocation", STYLE_OWNER));
            };
            session
                .host()
                .styles()
                .cell_styles
                .get(index as usize)
                .ok_or_else(|| AutoError::excel(424, "Object required"))?;
            Ok(AutoValue::Object(object("Interior", ObjectKey::Style { workbook, index })))
        })
    }

    pub(super) fn style_font_get(&self, target: &ObjectRef, call: &Call<'_>, index: u32) -> Reply {
        format_arity(call, 0)?;
        self.with_target(target, |access, _| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled(format!("{} inside a worksheet invocation", call.label()), STYLE_OWNER));
            };
            let host = session.host();
            let styles = host.styles();
            let style = styles.cell_styles.get(index as usize).ok_or_else(|| AutoError::excel(424, "Object required"))?;
            let base = styles.cell_style_formats.get(style.xf_id as usize).ok_or_else(|| {
                AutoError::not_modelled("Style.Font of a missing style format", STYLE_OWNER)
            })?;
            let font = match base.font_id {
                Some(id) => styles.fonts.get(id as usize),
                None => styles.fonts.first(),
            };
            super::font::read_font_member(host, font, call)
        })
    }

    pub(super) fn style_font_put(&self, target: &ObjectRef, call: &Call<'_>, index: u32) -> Reply {
        format_arity(call, 1)?;
        let patch = super::font::parse_font_patch(call)?;
        patch.validate().map_err(|_| format_error())?;
        let _ = target;
        let member = call.member.name.as_str();
        self.with_write(|session| {
            let receipt = session.dispatch_from(
                crate::CommandIngress::Automation,
                SkinIntent::Command(WorkbookCommand::StyleTable {
                    change: crate::StyleChange::Font { index, patch: Box::new(patch.clone()) },
                }),
            );
            if let Some(error) = receipt.error {
                return Err(AutoError::not_modelled(format!("Style.Font.{member}: {error:?}"), STYLE_OWNER));
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    pub(super) fn style_interior_get(&self, target: &ObjectRef, call: &Call<'_>, index: u32) -> Reply {
        format_arity(call, 0)?;
        let member = call.member.name.as_str();
        self.with_target(target, |access, _| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled(format!("{} inside a worksheet invocation", call.label()), STYLE_OWNER));
            };
            let host = session.host();
            let styles = host.styles();
            let style = styles.cell_styles.get(index as usize).ok_or_else(|| AutoError::excel(424, "Object required"))?;
            let base = styles.cell_style_formats.get(style.xf_id as usize).ok_or_else(|| {
                AutoError::not_modelled("Style.Interior of a missing style format", STYLE_OWNER)
            })?;
            let fill = match base.fill_id {
                Some(id) => styles.fills.get(id as usize),
                None => styles.fills.first(),
            };
            Ok(AutoValue::I4(super::interior::read_interior_member(host, fill, member)?))
        })
    }

    pub(super) fn style_interior_put(&self, target: &ObjectRef, call: &Call<'_>, index: u32) -> Reply {
        format_arity(call, 1)?;
        let patch = super::interior::parse_interior_patch(call)?;
        let _ = target;
        let member = call.member.name.as_str();
        self.with_write(|session| {
            let receipt = session.dispatch_from(
                crate::CommandIngress::Automation,
                SkinIntent::Command(WorkbookCommand::StyleTable {
                    change: crate::StyleChange::Interior { index, patch: patch.clone() },
                }),
            );
            if let Some(error) = receipt.error {
                return Err(AutoError::not_modelled(format!("Style.Interior.{member}: {error:?}"), STYLE_OWNER));
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    /// `Style.HorizontalAlignment/VerticalAlignment/IndentLevel/Orientation = value` for an unused style (dc-lj4j5).
    fn style_put_alignment(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let ObjectKey::Style { index, .. } = target.key else {
            return Err(geometry::type_mismatch());
        };
        let member = call.member.name.as_str();
        let (mut horizontal, mut vertical, mut indent, mut rotation) = (None, None, None, None);
        match member {
            "HorizontalAlignment" => horizontal = Some(format_enum_keyword(HORIZONTAL, call)?),
            "VerticalAlignment" => vertical = Some(format_enum_keyword(VERTICAL, call)?),
            "IndentLevel" => {
                let level = integral_format_arg(call)?;
                if !(0..=15).contains(&level) {
                    return Err(format_error());
                }
                indent = Some(level as u16);
            }
            "Orientation" => {
                rotation = Some(match integral_format_arg(call)? {
                    n @ -90..=90 => n as i16,
                    -4128 => 0,
                    -4171 => 90,
                    -4170 => -90,
                    XL_VERTICAL_TEXT => 255,
                    _ => return Err(format_error()),
                });
            }
            _ => unreachable!("registered style alignment property"),
        }
        self.with_write(|session| {
            let receipt = session.dispatch_from(
                crate::CommandIngress::Automation,
                SkinIntent::Command(WorkbookCommand::StyleTable {
                    change: crate::StyleChange::Alignment {
                        index,
                        horizontal: horizontal.clone(),
                        vertical: vertical.clone(),
                        indent,
                        rotation,
                    },
                }),
            );
            if let Some(error) = receipt.error {
                return Err(AutoError::not_modelled(format!("Style.{member}: {error:?}"), STYLE_OWNER));
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    /// `Style.NumberFormat = code` for a style no cell format uses; one in use is refused (dc-lj4j5).
    fn style_put_number_format(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let ObjectKey::Style { index, .. } = target.key else {
            return Err(geometry::type_mismatch());
        };
        let AutoValue::Text(code) = call.arg(0) else {
            return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
        };
        let code = code.to_string_lossy();
        self.with_write(|session| {
            let receipt = session.dispatch_from(
                crate::CommandIngress::Automation,
                SkinIntent::Command(WorkbookCommand::StyleTable { change: crate::StyleChange::NumberFormat { index, code: code.clone() } }),
            );
            if let Some(error) = receipt.error {
                return Err(AutoError::not_modelled(format!("Style.NumberFormat: {error:?}"), "dc-lj4j5"));
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    /// `Style.WrapText/ShrinkToFit/Locked/FormulaHidden = bool` for a style no cell format uses (dc-lj4j5).
    fn style_put_flag(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let ObjectKey::Style { index, .. } = target.key else {
            return Err(geometry::type_mismatch());
        };
        let AutoValue::Bool(value) = call.arg(0) else {
            return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
        };
        let member = call.member.name.clone();
        self.with_write(|session| {
            let receipt = session.dispatch_from(
                crate::CommandIngress::Automation,
                SkinIntent::Command(WorkbookCommand::StyleTable {
                    change: crate::StyleChange::Flag { index, member: member.clone(), value: *value },
                }),
            );
            if let Some(error) = receipt.error {
                return Err(AutoError::not_modelled(format!("Style.{member}: {error:?}"), "dc-lj4j5"));
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }

    fn style_delete(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let ObjectKey::Style { index, .. } = target.key else {
            return Err(geometry::type_mismatch());
        };
        self.with_write(|session| {
            let styles = session.host().styles();
            let style = styles.cell_styles.get(index as usize).ok_or_else(|| AutoError::excel(424, "Object required"))?;
            if style.builtin_id.is_some() || style.name == "Normal" {
                // Excel does not delete Normal; built-in styles it resets (dc-oxh8w.63).
                return Err(AutoError::excel(1004, "Application-defined or object-defined error"));
            }
            let receipt = session.dispatch_from(
                crate::CommandIngress::Automation,
                SkinIntent::Command(WorkbookCommand::StyleTable { change: crate::StyleChange::Remove { index } }),
            );
            if let Some(error) = receipt.error {
                return Err(AutoError::not_modelled(format!("Style.Delete: {error:?}"), STYLE_OWNER));
            }
            self.context_changed(session);
            Ok(AutoValue::Empty)
        })
    }
}
