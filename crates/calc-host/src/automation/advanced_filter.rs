//! `Range.AdvancedFilter` (dc-oxh8w.71): `xlFilterCopy` copies the list's header row and the records that meet
//! the criteria range to `CopyToRange`; `Unique` leaves out a record equal to an earlier one.
//!
//! The criteria are the database functions' (Excel documents them as the same): the criteria range's first row
//! names list columns, each row below it is one alternative, a record matching when it meets every condition of
//! any row. A record is judged by calling DCOUNTA over the header and that record, through the same catalogue
//! call a worksheet function takes, so there is one criterion evaluator. `xlFilterInPlace` (hiding rows with no
//! AutoFilter) and a `CopyToRange` of several cells (a chosen set of columns) are typed refusals owned by
//! dc-hduqd.
use super::*;

const XL_FILTER_IN_PLACE: i64 = 1;
const XL_FILTER_COPY: i64 = 2;
/// Owner of the in-place filter and the column-selecting copy.
const REMAINDER: &str = "dc-hduqd";

fn error_1004() -> AutoError {
    AutoError::excel(1004, "Application-defined or object-defined error")
}

impl WorkbookAutomation {
    pub(super) fn register_advanced_filter(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "AdvancedFilter", InvokeKind::Method, Self::range_advanced_filter);
    }

    fn range_advanced_filter(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.is_empty() || call.args.len() > 4 {
            return Err(geometry::bad_count());
        }
        let action = geometry::integer(call.arg(0))?;
        if action != XL_FILTER_IN_PLACE && action != XL_FILTER_COPY {
            return Err(error_1004());
        }
        let unique = match call.arg(3) {
            AutoValue::Missing | AutoValue::Empty => false,
            AutoValue::Bool(value) => *value,
            _ => return Err(geometry::type_mismatch()),
        };
        let held_of = |index: usize| -> Result<Option<calc_func::StoredReference>, AutoError> {
            match call.arg(index) {
                AutoValue::Missing | AutoValue::Empty => Ok(None),
                AutoValue::Object(ObjectRef {
                    key: ObjectKey::Range(held) | ObjectKey::RangeView { reference: held, .. },
                    ..
                }) => Ok(Some(held.clone())),
                _ => Err(geometry::type_mismatch()),
            }
        };
        let criteria = held_of(1)?;
        let copy_to = held_of(2)?;
        if action == XL_FILTER_IN_PLACE {
            return Err(AutoError::not_modelled("Range.AdvancedFilter xlFilterInPlace", REMAINDER));
        }
        let Some(copy_to) = copy_to else {
            return Err(error_1004());
        };
        // The list is only read; the output block is what is written.
        self.mutate(target, call, |session, list| {
            let list = list.ok_or_else(geometry::type_mismatch)?;
            if list.area_count() != 1 {
                return Err(AutoError::not_modelled(format!("{} multi-area write", call.label()), "dc-1nbp.6"));
            }
            let mut access = Access::Session(session, false);
            let list_area = list.areas()[0];
            let criteria = match &criteria {
                Some(held) => {
                    let resolved = self.resolve(&mut access, held)?;
                    if resolved.area_count() != 1 {
                        return Err(error_1004());
                    }
                    Some(resolved.areas()[0])
                }
                None => None,
            };
            let destination = self.resolve(&mut access, &copy_to)?;
            if destination.area_count() != 1 {
                return Err(error_1004());
            }
            let destination = destination.areas()[0];
            if destination.cell_count() != 1 {
                return Err(AutoError::not_modelled("Range.AdvancedFilter CopyToRange of several cells", REMAINDER));
            }
            if list_area.row_count() < 1 {
                return Err(error_1004());
            }
            let rows = list_area.row_count();
            let columns = list_area.column_count();
            let host = session.host();
            // Copying formulas is not modelled: a record holding one refuses before anything is written.
            for row in list_area.first().row()..=list_area.last().row() {
                for column in list_area.first().column()..=list_area.last().column() {
                    let key = CellKey::new(list_area.sheet(), row, column).map_err(|_| error_1004())?;
                    if matches!(host.model().cell(key), Some(calc_doc_model::CellInput::Formula(_))) {
                        return Err(AutoError::not_modelled(
                            "Range.AdvancedFilter over a list holding formulas",
                            REMAINDER,
                        ));
                    }
                }
            }
            let cells = |area: GridArea| -> Result<Vec<CalcValue>, AutoError> {
                let value = read_session(session, area, false)?;
                Ok(match value.as_array() {
                    Some(array) => array.iter_row_major().cloned().collect(),
                    None => vec![value],
                })
            };
            let all = cells(list_area)?;
            let header: Vec<CalcValue> = all[..columns as usize].to_vec();
            let mut kept: Vec<usize> = Vec::new();
            let environment = host.entry_environment();
            let numeric = host.recalc_context().numeric;
            // A criteria range of labels only sets no condition; DCOUNTA needs a row below the labels.
            let criteria_values = criteria
                .filter(|area| area.row_count() >= 2)
                .map(|area| CalcValue::reference(ReferenceLike::area(area)));
            for record in 1..rows as usize {
                let row = &all[record * columns as usize..(record + 1) * columns as usize];
                let matches = match &criteria_values {
                    None => true,
                    Some(criteria) => {
                        let database: Vec<CalcValue> = header.iter().chain(row.iter()).cloned().collect();
                        let database = calc_value::CalcArray::new(
                            calc_value::ArrayShape { rows: 2, cols: columns as usize },
                            database,
                        )
                        .map(CalcValue::array)
                        .ok_or_else(error_1004)?;
                        let mut reader = SessionCells(session, false);
                        let mut services = calc_func::EvalServices::with_cells(&numeric, &mut reader)
                            .with_environment(&environment);
                        let counted = worksheet_function::catalogue_call(
                            "DCOUNTA",
                            &[database, CalcValue::missing(), criteria.clone()],
                            &mut services,
                        )?;
                        match counted.core_ref() {
                            CoreRef::Number(n) => n >= 1.0,
                            // A criteria label the list lacks, or an error in the criteria: not guessed.
                            _ => {
                                return Err(AutoError::not_modelled(
                                    format!("Range.AdvancedFilter criteria the list cannot meet: {:?}", counted.core_ref()),
                                    REMAINDER,
                                ));
                            }
                        }
                    }
                };
                if matches {
                    kept.push(record);
                }
            }
            if unique {
                let key = |record: usize| -> Vec<String> {
                    all[record * columns as usize..(record + 1) * columns as usize]
                        .iter()
                        .map(|v| match v.core_ref() {
                            CoreRef::Text(t) => format!("t{}", t.to_string_lossy().to_lowercase()),
                            CoreRef::Number(n) => format!("n{}", n.to_bits()),
                            CoreRef::Logical(b) => format!("b{b}"),
                            CoreRef::Empty | CoreRef::Missing => "e".into(),
                            other => format!("{other:?}"),
                        })
                        .collect()
                };
                let mut seen = std::collections::HashSet::new();
                kept.retain(|record| seen.insert(key(*record)));
            }
            let out_rows = kept.len() as u32 + 1;
            let end_row = destination.first().row() + out_rows - 1;
            let end_column = destination.first().column() + columns - 1;
            if end_row > calc_value::MAX_ROWS || end_column > calc_value::MAX_COLUMNS {
                return Err(error_1004());
            }
            let output = GridArea::new(
                destination.first(),
                CellKey::new(destination.sheet(), end_row, end_column).map_err(|_| error_1004())?,
            )
            .map_err(|_| error_1004())?;
            self.guard_area(session, output)?;
            let mut values = Vec::with_capacity((out_rows * columns) as usize);
            for value in header.iter() {
                values.push(auto_value(value)?);
            }
            for record in &kept {
                for value in &all[record * columns as usize..(record + 1) * columns as usize] {
                    values.push(auto_value(value)?);
                }
            }
            let sheet = destination.sheet();
            let corner = |row, column| CellKey::new(sheet, row, column).map_err(|_| error_1004());
            let area = GridArea::new(corner(destination.first().row(), destination.first().column())?, corner(end_row, end_column)?)
                .map_err(|_| error_1004())?;
            let array = AutoValue::Array(Box::new(calc_object_model::AutoArray {
                rows: out_rows,
                columns,
                lower: (1, 1),
                values,
            }));
            self.write_range(session, ReferenceLike::area(area), Some(&array))
        })?;
        Ok(AutoValue::Bool(true))
    }
}
