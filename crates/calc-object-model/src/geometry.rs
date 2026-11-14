//! Portable Range geometry, shared by all projections. Evidence: the X19 Excel traces in
//! `calc-xll-reference/traces/2026-11-11-x19-com` and the measured `projection.cpp` they replaced.
//! Relative R1C1, RelativeTo, defaults and Union normalization follow the three identical
//! E6/E11 runs in `calc-xll-reference/traces/2026-11-14-om-probes/`.

use crate::{AutoError, AutoValue, Call};
use calc_value::{CellKey, GridArea, ReferenceLike};

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
    /// Parse scalar arguments. The host binds RelativeTo's Range to its top-left CellKey
    /// when relative R1C1 needs it; A1 and absolute R1C1 ignore RelativeTo (E11).
    pub fn from_call(call: &Call<'_>) -> Result<Self, AutoError> {
        if call.args.len() > 5 {
            return Err(bad_count());
        }
        let mut options = Self::default();
        if !address_default(call.arg(0)) {
            options.row_absolute = boolean(call.arg(0))?;
        }
        if !address_default(call.arg(1)) {
            options.column_absolute = boolean(call.arg(1))?;
        }
        if !address_default(call.arg(2)) {
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
        if !address_default(call.arg(3)) {
            options.external = boolean(call.arg(3))?;
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
    fn axis(tag: char, coordinate: u32, origin: u32, absolute: bool) -> String {
        if absolute {
            return format!("{tag}{coordinate}");
        }
        let offset = i64::from(coordinate) - i64::from(origin);
        if offset == 0 {
            tag.to_string()
        } else {
            format!("{tag}[{offset}]")
        }
    }
    let cell = |at: CellKey| {
        if options.style == ReferenceStyle::R1C1 {
            return format!(
                "{}{}",
                axis(
                    'R',
                    at.row(),
                    options.relative_to.map_or(1, |base| base.row()),
                    options.row_absolute
                ),
                axis(
                    'C',
                    at.column(),
                    options.relative_to.map_or(1, |base| base.column()),
                    options.column_absolute
                )
            );
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
        if area.column_count() == 16_384 || area.row_count() == 1_048_576 {
            let rows = area.column_count() == 16_384;
            let coordinate = |at: CellKey| {
                if options.style == ReferenceStyle::R1C1 {
                    if rows {
                        axis(
                            'R',
                            at.row(),
                            options.relative_to.map_or(1, |b| b.row()),
                            options.row_absolute,
                        )
                    } else {
                        axis(
                            'C',
                            at.column(),
                            options.relative_to.map_or(1, |b| b.column()),
                            options.column_absolute,
                        )
                    }
                } else if rows {
                    format!(
                        "{}{}",
                        if options.row_absolute { "$" } else { "" },
                        at.row()
                    )
                } else {
                    // Reuse the cell formatter's column spelling; its numeric row is suffix-only.
                    let label = cell(at);
                    label
                        .trim_end_matches(|c: char| c.is_ascii_digit())
                        .trim_end_matches('$')
                        .to_owned()
                }
            };
            text.push_str(&coordinate(area.first()));
            text.push(':');
            text.push_str(&coordinate(area.last()));
            continue;
        }
        text.push_str(&cell(area.first()));
        if area.cell_count() != 1 {
            text.push(':');
            text.push_str(&cell(area.last()));
        }
    }
    Ok(text)
}

/// Address's omitted/Null/Error arguments use their defaults (E6).
pub fn address_default(value: &AutoValue) -> bool {
    matches!(
        value,
        AutoValue::Missing | AutoValue::Null | AutoValue::Error(_)
    )
}

/// E11: keep argument order and distinct overlap multiplicity; drop identical areas and
/// merge vertically adjacent areas of the same width. No bounding-box union of overlaps.
pub fn union(references: &[ReferenceLike]) -> Result<ReferenceLike, AutoError> {
    if references.len() < 2 || references.len() > 30 {
        return Err(bad_count());
    }
    let reference = ReferenceLike::from_areas(
        references
            .iter()
            .flat_map(|r| r.areas().iter().copied())
            .collect(),
    )
    .map_err(|_| AutoError::excel(1004, "Union method of Application class failed"))?;
    let mut areas: Vec<GridArea> = Vec::new();
    for area in reference.areas().iter().copied() {
        if areas.contains(&area) {
            continue;
        }
        let adjacent = areas.iter_mut().find(|previous| {
            previous.first().column() == area.first().column()
                && previous.last().column() == area.last().column()
                && (previous.last().row().checked_add(1) == Some(area.first().row())
                    || area.last().row().checked_add(1) == Some(previous.first().row()))
        });
        if let Some(previous) = adjacent {
            let first = if previous.first().row() < area.first().row() {
                previous.first()
            } else {
                area.first()
            };
            let last = if previous.last().row() > area.last().row() {
                previous.last()
            } else {
                area.last()
            };
            *previous = GridArea::new(first, last).expect("valid adjacent areas on one sheet");
        } else {
            areas.push(area);
        }
    }
    Ok(ReferenceLike::from_areas(areas).expect("nonempty areas from one sheet"))
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

/// A bounded grid integer. Fractional coercion awaits the optional-argument measurements.
pub fn integer(value: &AutoValue) -> Result<i64, AutoError> {
    let n = match value {
        AutoValue::I2(v) => f64::from(*v),
        AutoValue::I4(v) => f64::from(*v),
        AutoValue::R4(v) => f64::from(*v),
        AutoValue::R8(v) => *v,
        AutoValue::Empty => 0.,
        AutoValue::Text(v) => v.to_string_lossy().parse().map_err(|_| type_mismatch())?,
        _ => return Err(type_mismatch()),
    };
    if !n.is_finite() || n.abs() > i32::MAX as f64 {
        return Err(AutoError::excel(1004, ""));
    }
    if n.fract() != 0. {
        return Err(AutoError::not_modelled(
            "Range fractional index coercion",
            "dc-1nbp.6",
        ));
    }
    Ok(n as i64)
}

fn rectangle(
    sheet: calc_value::SheetId,
    row: i64,
    column: i64,
    rows: i64,
    columns: i64,
) -> Result<GridArea, AutoError> {
    let cell = |r, c| {
        CellKey::new(
            sheet,
            u32::try_from(r).unwrap_or(0),
            u32::try_from(c).unwrap_or(0),
        )
        .map_err(|_| AutoError::excel(1004, ""))
    };
    if rows <= 0 || columns <= 0 {
        return Err(AutoError::excel(1004, ""));
    }
    GridArea::new(
        cell(row, column)?,
        cell(row + rows - 1, column + columns - 1)?,
    )
    .map_err(|_| AutoError::excel(1004, ""))
}

/// Range.Item indexes relative to the first area's top-left; Cells resets Rows/Columns views.
pub fn item(
    reference: &ReferenceLike,
    axis: RangeAxis,
    row: &AutoValue,
    column: &AutoValue,
) -> Result<ReferenceLike, AutoError> {
    let a = reference.areas()[0];
    let index = integer(row)?;
    let (r, c, rows, columns) = if !column.is_missing() {
        if axis != RangeAxis::Cells {
            return Err(AutoError::excel(1004, ""));
        }
        let col = if let AutoValue::Text(text) = column {
            let s = text.to_string_lossy();
            if s.bytes().all(|b| b.is_ascii_alphabetic()) && !s.is_empty() {
                s.bytes()
                    .try_fold(0i64, |n, b| {
                        n.checked_mul(26)?
                            .checked_add(i64::from(b.to_ascii_uppercase() - b'A' + 1))
                    })
                    .ok_or_else(|| AutoError::excel(1004, ""))?
            } else {
                integer(column)?
            }
        } else {
            integer(column)?
        };
        (index - 1, col - 1, 1, 1)
    } else {
        match axis {
            RangeAxis::Cells => {
                let width = i64::from(a.column_count());
                (
                    (index - 1).div_euclid(width),
                    (index - 1).rem_euclid(width),
                    1,
                    1,
                )
            }
            RangeAxis::Rows => (index - 1, 0, 1, i64::from(a.column_count())),
            RangeAxis::Columns => (0, index - 1, i64::from(a.row_count()), 1),
        }
    };
    rectangle(
        a.sheet(),
        i64::from(a.first().row()) + r,
        i64::from(a.first().column()) + c,
        rows,
        columns,
    )
    .map(ReferenceLike::area)
}

pub fn transform(
    reference: &ReferenceLike,
    name: &str,
    row: &AutoValue,
    column: &AutoValue,
) -> Result<ReferenceLike, AutoError> {
    // Multi-area Offset/Resize are intentionally refused until E11 measures their shapes.
    if reference.area_count() != 1 && matches!(name, "Offset" | "Resize") {
        return Err(AutoError::not_modelled(
            format!("Range.{name} multi-area"),
            "dc-1nbp.6",
        ));
    }
    let areas = reference
        .areas()
        .iter()
        .map(|a| {
            let (mut r, mut c, mut h, mut w) = (
                i64::from(a.first().row()),
                i64::from(a.first().column()),
                i64::from(a.row_count()),
                i64::from(a.column_count()),
            );
            match name {
                "Offset" => {
                    if !row.is_missing() {
                        r += integer(row)?;
                    }
                    if !column.is_missing() {
                        c += integer(column)?;
                    }
                }
                "Resize" => {
                    if !row.is_missing() {
                        h = integer(row)?;
                    }
                    if !column.is_missing() {
                        w = integer(column)?;
                    }
                }
                "EntireRow" => {
                    c = 1;
                    w = 16_384;
                }
                "EntireColumn" => {
                    r = 1;
                    h = 1_048_576;
                }
                _ => {
                    return Err(AutoError::not_modelled(
                        format!("Range.{name}"),
                        "dc-1nbp.5",
                    ));
                }
            }
            rectangle(a.sheet(), r, c, h, w)
        })
        .collect::<Result<Vec<_>, _>>()?;
    ReferenceLike::from_areas(areas).map_err(|_| AutoError::excel(1004, ""))
}
