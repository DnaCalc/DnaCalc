//! Portable Range geometry, shared by all projections. Evidence: the X19 Excel traces in
//! `calc-xll-reference/traces/2026-11-11-x19-com` and the measured `projection.cpp` they replaced.
//! Relative R1C1 and RelativeTo await dc-1nbp.6; the old projection did not model them either.

use crate::{AutoError, AutoValue, Call};
use calc_value::{CellKey, ReferenceLike};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RangeAxis {
    #[default]
    Cells,
    Rows,
    Columns,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceStyle {
    A1,
    R1C1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddressOptions {
    pub row_absolute: bool,
    pub column_absolute: bool,
    pub style: ReferenceStyle,
    pub external: bool,
    pub relative_to: Option<CellKey>,
}
impl Default for AddressOptions {
    fn default() -> Self {
        Self {
            row_absolute: true,
            column_absolute: true,
            style: ReferenceStyle::A1,
            external: false,
            relative_to: None,
        }
    }
}

pub fn boolean(value: &AutoValue) -> Result<bool, AutoError> {
    match value {
        AutoValue::Bool(v) => Ok(*v),
        AutoValue::Empty => Ok(false),
        AutoValue::I2(v) => Ok(*v != 0),
        AutoValue::I4(v) => Ok(*v != 0),
        AutoValue::R4(v) => Ok(*v != 0.),
        AutoValue::R8(v) => Ok(*v != 0.),
        AutoValue::Text(_) => match value.as_text().unwrap().to_ascii_lowercase().as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(type_mismatch()),
        },
        _ => Err(type_mismatch()),
    }
}
pub fn type_mismatch() -> AutoError {
    AutoError::Status(0x8002_0005u32 as i32)
}
pub fn bad_count() -> AutoError {
    AutoError::Status(0x8002_000Eu32 as i32)
}

impl AddressOptions {
    pub fn from_call(call: &Call<'_>) -> Result<Self, AutoError> {
        if call.args.len() > 5 {
            return Err(bad_count());
        }
        let mut options = Self::default();
        if !call.arg(0).is_missing() {
            options.row_absolute = boolean(call.arg(0))?;
        }
        if !call.arg(1).is_missing() {
            options.column_absolute = boolean(call.arg(1))?;
        }
        if !call.arg(2).is_missing() {
            let number = match call.arg(2) {
                AutoValue::I2(v) => f64::from(*v),
                AutoValue::I4(v) => f64::from(*v),
                AutoValue::R8(v) => *v,
                AutoValue::R4(v) => f64::from(*v),
                _ => return Err(type_mismatch()),
            };
            options.style = match number {
                1. => ReferenceStyle::A1,
                -4150. => ReferenceStyle::R1C1,
                _ => {
                    return Err(AutoError::not_modelled(
                        "Range.Address ReferenceStyle",
                        "dc-1nbp.6",
                    ));
                }
            };
        }
        if !call.arg(3).is_missing() {
            options.external = boolean(call.arg(3))?;
        }
        if !call.arg(4).is_missing() {
            return Err(AutoError::not_modelled(
                "Range.Address RelativeTo",
                "dc-1nbp.6",
            ));
        }
        Ok(options)
    }
}

/// `qualifier` is the workbook binder's escaped `[Book]Sheet` prefix, without `!`.
pub fn address(
    reference: &ReferenceLike,
    options: AddressOptions,
    qualifier: Option<&str>,
) -> Result<String, AutoError> {
    if options.relative_to.is_some() {
        return Err(AutoError::not_modelled(
            "Range.Address RelativeTo",
            "dc-1nbp.6",
        ));
    }
    if options.style == ReferenceStyle::R1C1 && (!options.row_absolute || !options.column_absolute)
    {
        return Err(AutoError::not_modelled(
            "Range.Address relative R1C1",
            "dc-1nbp.6",
        ));
    }
    let cell = |at: CellKey| {
        if options.style == ReferenceStyle::R1C1 {
            return format!("R{}C{}", at.row(), at.column());
        }
        let mut column = at.column();
        let mut name = Vec::new();
        while column > 0 {
            column -= 1;
            name.push(b'A' + (column % 26) as u8);
            column /= 26;
        }
        name.reverse();
        format!(
            "{}{}{}{}",
            if options.column_absolute { "$" } else { "" },
            String::from_utf8(name).expect("ASCII column"),
            if options.row_absolute { "$" } else { "" },
            at.row()
        )
    };
    let mut text = String::new();
    if options.external {
        text.push_str(
            qualifier
                .ok_or_else(|| AutoError::not_modelled("Range.Address External", "dc-1nbp.5"))?,
        );
        text.push('!');
    }
    for (index, area) in reference.areas().iter().enumerate() {
        if index != 0 {
            text.push(',');
        }
        text.push_str(&cell(area.first()));
        if area.cell_count() != 1 {
            text.push(':');
            text.push_str(&cell(area.last()));
        }
    }
    Ok(text)
}

/// Keep argument order and area multiplicity, including overlapping and identical areas.
pub fn union(references: &[ReferenceLike]) -> Result<ReferenceLike, AutoError> {
    if references.len() < 2 || references.len() > 30 {
        return Err(bad_count());
    }
    ReferenceLike::from_areas(
        references
            .iter()
            .flat_map(|r| r.areas().iter().copied())
            .collect(),
    )
    .map_err(|_| AutoError::excel(1004, "Union ranges must belong to one worksheet"))
}
pub fn row(reference: &ReferenceLike) -> u32 {
    reference.areas()[0].first().row()
}
pub fn column(reference: &ReferenceLike) -> u32 {
    reference.areas()[0].first().column()
}
pub fn count(reference: &ReferenceLike, axis: RangeAxis) -> u64 {
    match axis {
        RangeAxis::Cells => reference.areas().iter().map(|a| a.cell_count()).sum(),
        RangeAxis::Rows => u64::from(reference.areas()[0].row_count()),
        RangeAxis::Columns => u64::from(reference.areas()[0].column_count()),
    }
}
pub fn count_value(count: u64) -> AutoValue {
    match i32::try_from(count) {
        Ok(n) => AutoValue::I4(n),
        Err(_) => AutoValue::R8(count as f64),
    }
}
