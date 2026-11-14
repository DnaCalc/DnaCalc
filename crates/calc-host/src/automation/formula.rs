//! `Range.Formula` (get and put) and `Range.Text` (dc-piw0.6). The put enters the text through
//! the session's typed-entry path (`SkinIntent::EnterEntries`: `interpret_entry_in`, the formula
//! parser, relocation across the range, one recalculation), the get reads what the session holds.
use super::*;
use crate::{CellInput, SkinIntent};

/// Why a Range.Formula answer is the simplest Excel-like one: ~/codex-runs/eng4/excel-questions.md.
impl WorkbookAutomation {
    pub(super) fn register_formula(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Formula", InvokeKind::Get, Self::range_formula);
        handlers.on("Range", "Formula", InvokeKind::Put, Self::put_formula);
        handlers.on("Range", "Text", InvokeKind::Get, Self::cells_text);
    }

    /// The projected cells of the first area, row-major (session reads only).
    fn session_cells(
        access: &mut Access<'_>,
        area: GridArea,
        what: &str,
    ) -> Result<Vec<crate::ProjectedCell>, AutoError> {
        let Access::Session(session) = access else {
            return Err(AutoError::not_modelled(
                format!("{what} inside a worksheet invocation"),
                OWNER,
            ));
        };
        if area.cell_count() > 1_048_576 {
            return Err(AutoError::not_modelled(
                format!("{what} materialization limit"),
                OWNER,
            ));
        }
        let mut cells = Vec::with_capacity(area.cell_count() as usize);
        for row in area.first().row()..=area.last().row() {
            for col in area.first().column()..=area.last().column() {
                cells.push(CellKey::new(area.sheet(), row, col).expect("cell in valid area"));
            }
        }
        crate::project(session.host(), &ProjectionRequest { cells })
            .map(|page| page.cells().to_vec())
            .map_err(|e| AutoError::not_modelled(e.to_string(), OWNER))
    }

    fn shaped(area: GridArea, mut values: Vec<AutoValue>) -> Reply {
        if values.len() == 1 {
            return Ok(values.remove(0));
        }
        Ok(AutoValue::Array(Box::new(AutoArray {
            rows: area.row_count(),
            columns: area.column_count(),
            lower: (1, 1),
            values,
        })))
    }

    /// A formula cell answers `=` and its text as typed; any other cell answers its value, an
    /// empty cell the empty string.
    fn range_formula(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, _, range, _| {
            no_args(call)?;
            let area = range.areas()[0];
            let cells = Self::session_cells(access, area, "Range.Formula")?;
            let values = cells
                .iter()
                .map(|cell| match cell.input() {
                    Some(CellInput::Formula(_)) => Ok(AutoValue::text(
                        &crate::search::entry_text(cell.input()),
                    )),
                    None => Ok(AutoValue::text("")),
                    Some(CellInput::Number(n)) => Ok(AutoValue::R8(n.get())),
                    Some(CellInput::Text(t)) => Ok(AutoValue::Text(t.text().clone())),
                    Some(CellInput::Logical(b)) => Ok(AutoValue::Bool(*b)),
                    Some(CellInput::Error(_)) => match cell.value() {
                        ProjectedValue::Literal(v) => auto_value(v),
                        _ => Err(AutoError::not_modelled("Range.Formula error constant", OWNER)),
                    },
                })
                .collect::<Result<Vec<_>, _>>()?;
            Self::shaped(area, values)
        })
    }

    /// The text the cell shows under its number format, the way the grid draws it. A range of
    /// several cells answers Null.
    fn cells_text(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.with_range(target, |access, _, range, _| {
            no_args(call)?;
            let area = range.areas()[0];
            if area.cell_count() != 1 {
                return Ok(AutoValue::Null);
            }
            let cells = Self::session_cells(access, area, "Range.Text")?;
            let Access::Session(session) = access else {
                unreachable!("session_cells refused");
            };
            let host = session.host();
            let cell = &cells[0];
            let value = match cell.value() {
                ProjectedValue::Empty => return Ok(AutoValue::text("")),
                ProjectedValue::Literal(v) | ProjectedValue::Calculated { value: v, .. } => {
                    v.clone()
                }
                ProjectedValue::FileCached(cache) => cache.value().to_value(),
                ProjectedValue::Stale(stale) => stale
                    .last_value()
                    .cloned()
                    .or_else(|| stale.file_cache().map(|c| c.value().to_value()))
                    .unwrap_or_else(CalcValue::empty),
                ProjectedValue::NotModeled { .. } => {
                    return Err(AutoError::not_modelled(
                        "Range.Text of an unmodelled formula",
                        OWNER,
                    ));
                }
            };
            let environment = host.entry_environment();
            // A session without a regional profile shows numbers as en-US does.
            let fallback;
            let profile = match environment.profile.as_ref() {
                Some(profile) => profile,
                None => {
                    fallback = calc_func::locale::format_profile(calc_func::locale::LocaleId::EnUs);
                    &fallback
                }
            };
            let general = |value: &CalcValue| -> Result<String, AutoError> {
                Ok(match value.core_ref() {
                    CoreRef::Number(n) => calc_func::general_format(profile, n),
                    CoreRef::Text(t) => t.to_string_lossy(),
                    CoreRef::Logical(true) => "TRUE".to_owned(),
                    CoreRef::Logical(false) => "FALSE".to_owned(),
                    CoreRef::Error(_) => return Self::error_text(&value),
                    CoreRef::Empty => String::new(),
                    _ => return Err(AutoError::not_modelled("Range.Text cell kind", OWNER)),
                })
            };
            let code = host.number_format(area.first());
            let text = match code {
                Some(code)
                    if !code.eq_ignore_ascii_case("general")
                        && matches!(value.core_ref(), CoreRef::Number(_) | CoreRef::Text(_)) =>
                {
                    let system = if host.declared_date1904() {
                        calc_func::locale::DateSystem::System1904
                    } else {
                        calc_func::locale::DateSystem::System1900
                    };
                    match calc_func::cell_text(profile, system, &value, code) {
                        Ok(text) => text,
                        Err(calc_func::CellFormatError::Unshowable) => "#####".to_owned(),
                        Err(calc_func::CellFormatError::NotModelled) => general(&value)?,
                    }
                }
                _ => general(&value)?,
            };
            Ok(AutoValue::text(&text))
        })
    }

    fn error_text(value: &CalcValue) -> Result<String, AutoError> {
        match auto_value(value)? {
            AutoValue::Error(code) => Ok(match code {
                2000 => "#NULL!",
                2007 => "#DIV/0!",
                2015 => "#VALUE!",
                2023 => "#REF!",
                2029 => "#NAME?",
                2036 => "#NUM!",
                _ => "#N/A",
            }
            .to_owned()),
            _ => Err(AutoError::not_modelled("Range.Text error", OWNER)),
        }
    }

    /// An argument as the text a user would type: text as it is, numbers by their shortest
    /// round-trip decimal, logicals as TRUE/FALSE, Empty as nothing (a clear).
    fn entry_of(value: &AutoValue) -> Result<String, AutoError> {
        Ok(match value {
            AutoValue::Empty | AutoValue::Missing => String::new(),
            AutoValue::Text(t) => t.to_string_lossy(),
            AutoValue::R8(n) => n.to_string(),
            AutoValue::I4(n) => n.to_string(),
            AutoValue::Bool(true) => "TRUE".to_owned(),
            AutoValue::Bool(false) => "FALSE".to_owned(),
            _ => {
                return Err(AutoError::not_modelled(
                    "Range.Formula put of this value type",
                    OWNER,
                ));
            }
        })
    }

    fn put_formula(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        self.mutate_range(target, call, |_, range| {
            if call.args.len() != 1 {
                return Err(geometry::bad_count());
            }
            let area = range.areas()[0];
            let entries = match call.arg(0) {
                AutoValue::Array(a) => {
                    if a.rows != area.row_count()
                        || a.columns != area.column_count()
                        || u64::from(a.rows) * u64::from(a.columns) != a.values.len() as u64
                    {
                        return Err(AutoError::not_modelled(
                            "Range.Formula array/target shape difference",
                            OWNER,
                        ));
                    }
                    let mut entries = Vec::with_capacity(a.values.len());
                    for row in 0..area.row_count() {
                        for column in 0..area.column_count() {
                            let at = CellKey::new(
                                area.sheet(),
                                area.first().row() + row,
                                area.first().column() + column,
                            )
                            .expect("cell in valid area");
                            let value = &a.values[(row * area.column_count() + column) as usize];
                            entries.push((GridArea::cell(at), Self::entry_of(value)?));
                        }
                    }
                    entries
                }
                other => vec![(area, Self::entry_of(other)?)],
            };
            Ok(SkinIntent::EnterEntries { entries })
        })
    }
}
