//! Font properties of worksheet ranges.
use super::*;
use calc_doc_model::{Font, StyleColor};

const AUTOMATIC_COLOR: i32 = -4105;
const NO_COLOR: i32 = -4142;
pub(super) const UNDERLINES: &[(i32, &str)] = &[
    (-4142, "none"),
    (2, "single"),
    (-4119, "double"),
    (4, "singleAccounting"),
    (5, "doubleAccounting"),
];

impl WorkbookAutomation {
    pub(super) fn register_range_font(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Font", InvokeKind::Get, Self::range_font);
        for member in [
            "Bold",
            "Italic",
            "Underline",
            "Size",
            "Name",
            "Color",
            "ColorIndex",
        ] {
            handlers.on("Font", member, InvokeKind::Get, Self::get_font_property);
            handlers.on("Font", member, InvokeKind::Put, Self::put_font_property);
        }
    }

    fn range_font(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        self.with_range(target, |access, _, range, _| {
            let reference = self.retain_range(access, range)?;
            Ok(AutoValue::Object(object(
                "Font",
                ObjectKey::RangeFormat(reference),
            )))
        })
    }

    fn get_font_property(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if super::conditions::is_condition_part(target) {
            return self.condition_font_get(target, call);
        }
        if let ObjectKey::Style { index, .. } = target.key {
            return self.style_font_get(target, call, index);
        }
        let member = call.member.name.as_str();
        match member {
            "Bold" | "Italic" => self.read_format(target, call, AutoValue::Bool, |host, style| {
                let font = cell_font(host, style)?;
                Ok(font.is_some_and(|f| if member == "Bold" { f.bold } else { f.italic }))
            }),
            "Underline" => self.read_format(target, call, AutoValue::I4, |host, style| {
                let keyword = cell_font(host, style)?.and_then(|f| f.underline.as_deref());
                format_enum_value(UNDERLINES, keyword, call)
            }),
            "Name" => self.read_format(
                target,
                call,
                |name: String| AutoValue::text(&name),
                |host, style| {
                    cell_font(host, style)?
                        .and_then(|f| f.name.clone())
                        .or_else(|| host.default_font_name().map(str::to_owned))
                        .ok_or_else(|| {
                            AutoError::not_modelled(
                                "Font.Name from an unavailable font name",
                                OWNER,
                            )
                        })
                },
            ),
            "Size" => self.read_format(target, call, AutoValue::R8, |host, style| {
                let size = cell_font(host, style)?
                    .and_then(|f| f.size.as_deref())
                    .unwrap_or("11");
                size.parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .ok_or_else(|| {
                        AutoError::not_modelled(format!("Font.Size stored as {size}"), OWNER)
                    })
            }),
            "Color" | "ColorIndex" => {
                self.read_format(target, call, AutoValue::I4, |host, style| {
                    let color = cell_font(host, style)?.and_then(|f| f.color.as_ref());
                    if member == "Color" {
                        font_color(host, color)
                    } else {
                        font_color_index(host, color)
                    }
                })
            }
            _ => unreachable!("registered font property"),
        }
    }

    fn put_font_property(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if super::conditions::is_condition_part(target) {
            return self.condition_font_put(target, call);
        }
        if let ObjectKey::Style { index, .. } = target.key {
            return self.style_font_put(target, call, index);
        }
        self.put_format(target, call, || parse_font_patch(call))
    }
}

pub(super) fn parse_font_patch(call: &Call<'_>) -> Result<FormatPatch, AutoError> {
    let mut patch = FormatPatch::default();
    match call.member.name.as_str() {
        "Bold" => patch.bold = Some(geometry::boolean(call.arg(0)).map_err(|_| format_error())?),
        "Italic" => patch.italic = Some(geometry::boolean(call.arg(0)).map_err(|_| format_error())?),
        "Underline" => {
            let keyword = format_enum_keyword(UNDERLINES, call)?;
            if keyword == "singleAccounting" {
                return Err(AutoError::not_modelled(
                    "Font.Underline single accounting style",
                    OWNER,
                ));
            }
            patch.underline_style = Some(keyword);
        }
        "Size" => {
            let size = super::super::report_draw::number(call.arg(0)).map_err(|_| format_error())?;
            patch.font_size = Some(size.to_string());
        }
        "Name" => {
            let AutoValue::Text(text) = call.arg(0) else {
                return Err(format_error());
            };
            patch.font_name = Some(String::from_utf16(text.utf16_code_units()).map_err(|_| format_error())?);
        }
        "Color" => {
            let rgb = integral_format_arg(call)?;
            if !(0..=0x00FF_FFFF).contains(&rgb) {
                return Err(format_error());
            }
            patch.font_color = Some(format!("{:06X}", reverse_rgb(rgb as u32)));
        }
        "ColorIndex" => match integral_format_arg(call)? {
            AUTOMATIC_COLOR => patch.font_color = Some(String::new()),
            NO_COLOR => {
                return Err(AutoError::not_modelled(
                    "Font.ColorIndex xlColorIndexNone",
                    OWNER,
                ));
            }
            index @ 1..=56 => patch.font_color_indexed = Some(index as u8 + 7),
            _ => return Err(format_error()),
        },
        _ => unreachable!("registered font property"),
    }
    Ok(patch)
}

pub(super) fn read_font_member(
    host: &HostState,
    font: Option<&Font>,
    call: &Call<'_>,
) -> Reply {
    match call.member.name.as_str() {
        "Bold" => Ok(AutoValue::Bool(font.is_some_and(|f| f.bold))),
        "Italic" => Ok(AutoValue::Bool(font.is_some_and(|f| f.italic))),
        "Underline" => {
            let keyword = font.and_then(|f| f.underline.as_deref());
            Ok(AutoValue::I4(format_enum_value(UNDERLINES, keyword, call)?))
        }
        "Name" => {
            let name = font
                .and_then(|f| f.name.clone())
                .or_else(|| host.default_font_name().map(str::to_owned))
                .ok_or_else(|| {
                    AutoError::not_modelled("Font.Name from an unavailable font name", OWNER)
                })?;
            Ok(AutoValue::text(&name))
        }
        "Size" => {
            let size = font.and_then(|f| f.size.as_deref()).unwrap_or("11");
            let v = size
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v > 0.0)
                .ok_or_else(|| AutoError::not_modelled(format!("Font.Size stored as {size}"), OWNER))?;
            Ok(AutoValue::R8(v))
        }
        "Color" => Ok(AutoValue::I4(font_color(host, font.and_then(|f| f.color.as_ref()))?)),
        "ColorIndex" => Ok(AutoValue::I4(font_color_index(host, font.and_then(|f| f.color.as_ref()))?)),
        _ => unreachable!("registered font property"),
    }
}

fn cell_font(host: &HostState, style: u32) -> Result<Option<&Font>, AutoError> {
    let styles = host.styles();
    let format = styles.cell_formats.get(style as usize);
    if format.is_none() && style != 0 {
        return Err(AutoError::not_modelled(
            format!("Cell format {style} is unavailable"),
            OWNER,
        ));
    }
    let id = format.and_then(|xf| {
        xf.font_id.or_else(|| {
            let parent = styles.cell_style_formats.get(xf.xf_id? as usize)?;
            parent.font_id
        })
    });
    match id {
        Some(id) => styles.fonts.get(id as usize).map(Some).ok_or_else(|| {
            AutoError::not_modelled(format!("Font record {id} is unavailable"), OWNER)
        }),
        None => Ok(styles.fonts.first()),
    }
}

// VBA's RGB packs the red component in the low byte. Styles store RRGGBB.
pub(super) fn reverse_rgb(rgb: u32) -> u32 {
    (rgb & 0xFF) << 16 | rgb & 0xFF00 | rgb >> 16 & 0xFF
}

fn font_color(host: &HostState, color: Option<&StyleColor>) -> Result<i32, AutoError> {
    let Some(color) = color.filter(|color| !color.auto) else {
        return Ok(0);
    };
    host.resolved_style_color(color)
        .and_then(|rgb| u32::from_str_radix(rgb.strip_prefix('#')?, 16).ok())
        .map(|rgb| reverse_rgb(rgb) as i32)
        .ok_or_else(|| AutoError::not_modelled("Font.Color from an unresolved style color", OWNER))
}

fn font_color_index(host: &HostState, color: Option<&StyleColor>) -> Result<i32, AutoError> {
    let Some(color) = color.filter(|color| !color.auto) else {
        return Ok(AUTOMATIC_COLOR);
    };
    match color
        .indexed
        .as_deref()
        .and_then(|index| index.parse::<i32>().ok())
    {
        Some(index @ 8..=63) => return Ok(index - 7),
        Some(64) => return Ok(AUTOMATIC_COLOR),
        _ => {}
    }
    let resolved = host.resolved_style_color(color);
    if let Some(rgb) = resolved {
        for index in 8..=63 {
            if host
                .indexed_color(index)
                .is_some_and(|entry| entry.eq_ignore_ascii_case(&rgb))
            {
                return Ok(index as i32 - 7);
            }
        }
    }
    Err(AutoError::not_modelled(
        "Font.ColorIndex for a color outside the workbook palette",
        OWNER,
    ))
}
