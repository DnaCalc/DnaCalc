//! `Range.Interior`: the fill of worksheet ranges, written through Format Cells' Fill tab.
use super::font::reverse_rgb;
use super::*;
use crate::{FillColor, InteriorPatch};
use calc_doc_model::{Fill, StyleColor};

const AUTOMATIC_COLOR: i32 = -4105;
const NO_COLOR: i32 = -4142;
const WHITE: i32 = 0x00FF_FFFF;
// Excel's XlPattern values paired with their OOXML pattern types.
const PATTERNS: &[(i32, &str)] = &[
    (NO_COLOR, "none"),
    (1, "solid"),
    (-4125, "mediumGray"),
    (-4126, "darkGray"),
    (-4124, "lightGray"),
    (-4128, "darkHorizontal"),
    (-4166, "darkVertical"),
    (-4121, "darkDown"),
    (-4162, "darkUp"),
    (9, "darkGrid"),
    (10, "darkTrellis"),
    (11, "lightHorizontal"),
    (12, "lightVertical"),
    (13, "lightDown"),
    (14, "lightUp"),
    (15, "lightGrid"),
    (16, "lightTrellis"),
    (17, "gray125"),
    (18, "gray0625"),
];

impl WorkbookAutomation {
    pub(super) fn register_range_interior(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Interior", InvokeKind::Get, Self::range_interior);
        // Series.Interior (chart_format.rs) registers Color; its handlers hand range interiors back here.
        for member in ["ColorIndex", "Pattern", "PatternColor"] {
            handlers.on("Interior", member, InvokeKind::Get, Self::range_interior_get);
            handlers.on("Interior", member, InvokeKind::Put, Self::range_interior_put);
        }
    }

    fn range_interior(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        self.with_range(target, |access, _, range, _| {
            let reference = self.retain_range(access, range)?;
            Ok(AutoValue::Object(object("Interior", ObjectKey::RangeFormat(reference))))
        })
    }

    /// Interior members of a range; the series interior shares the class name and keeps its own handlers.
    pub(in crate::automation) fn range_interior_get(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if super::conditions::is_condition_part(target) {
            return self.condition_interior_get(target, call);
        }
        if let ObjectKey::Style { index, .. } = target.key {
            return self.style_interior_get(target, call, index);
        }
        let member = call.member.name.as_str();
        self.read_format(target, call, AutoValue::I4, |host, style| {
            read_interior_member(host, cell_fill(host, style)?, member)
        })
    }

    pub(in crate::automation) fn range_interior_put(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if super::conditions::is_condition_part(target) {
            return self.condition_interior_put(target, call);
        }
        if let ObjectKey::Style { index, .. } = target.key {
            return self.style_interior_put(target, call, index);
        }
        self.put_format(target, call, || {
            Ok(FormatPatch {
                interior: Some(parse_interior_patch(call)?),
                ..Default::default()
            })
        })
    }
}

pub(super) fn read_interior_member(host: &HostState, fill: Option<&Fill>, member: &str) -> Result<i32, AutoError> {
    let kind = fill.and_then(|f| f.pattern_type.as_deref()).unwrap_or("none");
    let cell = match kind {
        "none" => None,
        "solid" => fill.and_then(|f| f.fg_color.as_ref()),
        _ => fill.and_then(|f| f.bg_color.as_ref()),
    };
    match member {
        "Pattern" => PATTERNS
            .iter()
            .find(|(_, name)| *name == kind)
            .map(|(value, _)| *value)
            .ok_or_else(|| AutoError::not_modelled(format!("Interior.Pattern stored as {kind}"), OWNER)),
        "Color" if kind == "none" => Ok(WHITE),
        "Color" => color_bgr(host, cell, WHITE),
        "ColorIndex" if kind == "none" => Ok(NO_COLOR),
        "ColorIndex" => color_index(host, cell),
        "PatternColor" if matches!(kind, "none" | "solid") => Err(AutoError::not_modelled(
            "Interior.PatternColor of a fill without a pattern",
            OWNER,
        )),
        "PatternColor" => color_bgr(host, fill.and_then(|f| f.fg_color.as_ref()), 0),
        _ => unreachable!("registered interior property"),
    }
}

pub(super) fn parse_interior_patch(call: &Call<'_>) -> Result<InteriorPatch, AutoError> {
    let mut patch = InteriorPatch::default();
    match call.member.name.as_str() {
        "Color" => patch.color = Some(FillColor::Rgb(rgb_arg(call)?)),
        "PatternColor" => patch.pattern_color = Some(FillColor::Rgb(rgb_arg(call)?)),
        "Pattern" => match integral_format_arg(call)? {
            // ww23: xlAutomatic reads back xlSolid with the automatic (white) colour.
            AUTOMATIC_COLOR => {
                patch.pattern = Some("solid".into());
                patch.color = Some(FillColor::Automatic);
            }
            4000 | 4001 => {
                return Err(AutoError::not_modelled("Interior.Pattern gradient values", OWNER));
            }
            value => {
                let name = PATTERNS.iter().find(|(v, _)| *v == value).ok_or_else(format_error)?.1;
                patch.pattern = Some(name.into());
            }
        },
        "ColorIndex" => match integral_format_arg(call)? {
            NO_COLOR => patch.pattern = Some("none".into()),
            AUTOMATIC_COLOR => patch.pattern = Some("none".into()),
            index @ 1..=56 => patch.color = Some(FillColor::Indexed(index as u8 + 7)),
            _ => return Err(format_error()),
        },
        _ => unreachable!("registered interior property"),
    }
    Ok(patch)
}

fn rgb_arg(call: &Call<'_>) -> Result<String, AutoError> {
    let rgb = integral_format_arg(call)?;
    if !(0..=WHITE).contains(&rgb) {
        return Err(format_error());
    }
    Ok(format!("{:06X}", reverse_rgb(rgb as u32)))
}

fn cell_fill(host: &HostState, style: u32) -> Result<Option<&Fill>, AutoError> {
    let styles = host.styles();
    let format = styles.cell_formats.get(style as usize);
    if format.is_none() && style != 0 {
        return Err(AutoError::not_modelled(format!("Cell format {style} is unavailable"), OWNER));
    }
    let id = format.and_then(|xf| {
        xf.fill_id.or_else(|| styles.cell_style_formats.get(xf.xf_id? as usize)?.fill_id)
    });
    match id {
        Some(id) => styles
            .fills
            .get(id as usize)
            .map(Some)
            .ok_or_else(|| AutoError::not_modelled(format!("Fill record {id} is unavailable"), OWNER)),
        None => Ok(styles.fills.first()),
    }
}

pub(super) fn color_bgr(host: &HostState, color: Option<&StyleColor>, automatic: i32) -> Result<i32, AutoError> {
    let Some(color) = color.filter(|color| !color.auto) else {
        return Ok(automatic);
    };
    host.resolved_style_color(color)
        .and_then(|rgb| u32::from_str_radix(rgb.strip_prefix('#')?, 16).ok())
        .map(|rgb| reverse_rgb(rgb) as i32)
        .ok_or_else(|| AutoError::not_modelled("Color from an unresolved style color", OWNER))
}

pub(super) fn color_index(host: &HostState, color: Option<&StyleColor>) -> Result<i32, AutoError> {
    let Some(color) = color.filter(|color| !color.auto) else {
        return Ok(AUTOMATIC_COLOR);
    };
    match color.indexed.as_deref().and_then(|index| index.parse::<i32>().ok()) {
        Some(index @ 8..=63) => return Ok(index - 7),
        Some(64) => return Ok(AUTOMATIC_COLOR),
        _ => {}
    }
    if let Some(rgb) = host.resolved_style_color(color) {
        for index in 8..=63 {
            if host.indexed_color(index).is_some_and(|entry| entry.eq_ignore_ascii_case(&rgb)) {
                return Ok(index as i32 - 7);
            }
        }
    }
    // Outside the palette Excel reads the nearest entry (ww23: 0x123456 -> 52). One measured point: the metric is the
    // squared RGB distance, the first of equally near entries.
    let rgb = host.resolved_style_color(color).and_then(|rgb| u32::from_str_radix(rgb.strip_prefix('#')?, 16).ok());
    let Some(rgb) = rgb else {
        return Err(AutoError::not_modelled("ColorIndex for an unresolved style color", OWNER));
    };
    let channels = |v: u32| [(v >> 16) as i32 & 0xFF, (v >> 8) as i32 & 0xFF, v as i32 & 0xFF];
    let want = channels(rgb);
    (8..=63)
        .filter_map(|index| {
            let text = host.indexed_color(index)?;
            let entry = u32::from_str_radix(text.strip_prefix('#')?, 16).ok()?;
            let have = channels(entry);
            Some((want.iter().zip(have).map(|(a, b)| (a - b) * (a - b)).sum::<i32>(), index))
        })
        .min()
        .map(|(_, index)| index as i32 - 7)
        .ok_or_else(|| AutoError::not_modelled("ColorIndex for a workbook without a palette", OWNER))
}
