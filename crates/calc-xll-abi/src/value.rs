use super::*;
use calc_value::{
    ArrayShape, CalcArray, CalcValue, CellKey, CoreRef, ExcelText, GridArea, ReferenceLike,
    SheetId, WorksheetErrorCode,
};
use std::collections::BTreeMap;

/// Opaque nonzero C API sheet identities. Generational DNA identities never become pointers.
#[derive(Clone, Default, Debug)]
pub struct SheetMap {
    sheets: BTreeMap<SheetId, usize>,
}
impl SheetMap {
    pub fn encode(&mut self, sheet: SheetId) -> usize {
        let next = self.sheets.len() + 1;
        *self.sheets.entry(sheet).or_insert(next)
    }
    pub fn decode(&self, handle: usize) -> Option<SheetId> {
        self.sheets
            .iter()
            .find_map(|(sheet, id)| (*id == handle).then_some(*sheet))
    }
}

/// All allocations survive moves of the arena and subsequent allocations. The host owns one
/// arena per invocation and one per live callback result, released with xlFree.
#[derive(Default)]
pub struct Arena {
    opers: Vec<Box<[XlOper12]>>,
    strings: Vec<Box<[u16]>>,
    references: Vec<Box<[u32]>>,
    doubles: Vec<Box<[f64]>>,
    shorts: Vec<Box<i16>>,
    integers: Vec<Box<i32>>,
}

impl Arena {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn store(&mut self, oper: XlOper12) -> *mut XlOper12 {
        let mut owned = vec![oper].into_boxed_slice();
        let ptr = owned.as_mut_ptr();
        self.opers.push(owned);
        ptr
    }
    pub fn short(&mut self, value: i16) -> *mut i16 {
        let mut value = Box::new(value);
        let ptr = &mut *value as *mut i16;
        self.shorts.push(value);
        ptr
    }
    pub fn int(&mut self, value: i32) -> *mut i32 {
        let mut value = Box::new(value);
        let ptr = &mut *value as *mut i32;
        self.integers.push(value);
        ptr
    }
    pub fn wide(
        &mut self,
        units: &[u16],
        counted: bool,
        in_place: bool,
    ) -> Result<*mut u16, String> {
        if units.len() > 32767 {
            return Err("XLL string exceeds 32767 UTF-16 units".into());
        }
        let mut buffer =
            vec![0u16; if in_place { 32768 } else { units.len() + 1 }].into_boxed_slice();
        if counted {
            buffer[0] = units.len() as u16;
            buffer[1..units.len() + 1].copy_from_slice(units);
        } else {
            buffer[..units.len()].copy_from_slice(units);
        }
        let ptr = buffer.as_mut_ptr();
        self.strings.push(buffer);
        Ok(ptr)
    }
    pub fn text(&mut self, text: &str) -> Result<XlOper12, String> {
        let pointer = self.wide(&text.encode_utf16().collect::<Vec<_>>(), true, false)?;
        Ok(XlOper12 {
            value: XlValue12 { string: pointer },
            xltype: XL_STR,
        })
    }
    pub fn fp12(&mut self, value: &CalcValue) -> Result<*mut Fp12, String> {
        let (rows, columns, values) = if let Some(array) = value.as_array() {
            let shape = array.shape();
            let values = (0..shape.rows)
                .flat_map(|r| (0..shape.cols).map(move |c| array.get(r, c).unwrap()))
                .map(|v| v.as_number().ok_or("FP12 requires numbers".to_owned()))
                .collect::<Result<Vec<_>, _>>()?;
            (shape.rows, shape.cols, values)
        } else {
            (
                1,
                1,
                vec![value.as_number().ok_or("FP12 requires numbers")?],
            )
        };
        let rows = i32::try_from(rows).map_err(|_| "FP12 too tall")?;
        let columns = i32::try_from(columns).map_err(|_| "FP12 too wide")?;
        let header = (columns as u64) << 32 | rows as u64;
        let mut buffer = std::iter::once(f64::from_bits(header))
            .chain(values)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let ptr = buffer.as_mut_ptr().cast::<Fp12>();
        self.doubles.push(buffer);
        Ok(ptr)
    }
    pub fn oper(&mut self, value: &CalcValue, sheets: &mut SheetMap) -> Result<XlOper12, String> {
        if value.callable_value().is_some() {
            return Err("dc-snz.2: an XLL argument cannot be a LAMBDA".into());
        }
        Ok(match value.core_ref() {
            CoreRef::Number(n) if n.is_finite() => XlOper12::number(n),
            CoreRef::Number(_) => return Err("nonfinite XLL argument".into()),
            CoreRef::Logical(b) => XlOper12::boolean(b),
            CoreRef::Error(e) => {
                XlOper12::error(error_number(e).ok_or("dc-snz.2: unsupported XLL worksheet error")?)
            }
            CoreRef::Empty => XlOper12::default(),
            CoreRef::Missing => XlOper12 {
                xltype: XL_MISSING,
                ..XlOper12::default()
            },
            CoreRef::Text(text) => XlOper12 {
                value: XlValue12 {
                    string: self.wide(text.utf16_code_units(), true, false)?,
                },
                xltype: XL_STR,
            },
            CoreRef::Array(array) => {
                let shape = array.shape();
                let mut cells = Vec::with_capacity(array.cell_count());
                for row in 0..shape.rows {
                    for col in 0..shape.cols {
                        let cell = array.get(row, col).unwrap();
                        if cell.as_array().is_some() || cell.as_reference().is_some() {
                            return Err("nested array/reference in XLL array".into());
                        }
                        cells.push(self.oper(cell, sheets)?);
                    }
                }
                let mut cells = cells.into_boxed_slice();
                let values = cells.as_mut_ptr();
                self.opers.push(cells);
                XlOper12 {
                    value: XlValue12 {
                        array: XlArray12 {
                            values,
                            rows: i32::try_from(shape.rows).map_err(|_| "array too tall")?,
                            columns: i32::try_from(shape.cols).map_err(|_| "array too wide")?,
                        },
                    },
                    xltype: XL_MULTI,
                }
            }
            CoreRef::Reference(reference) => {
                let areas = reference.areas();
                let sheet = areas.first().ok_or("empty XLL reference")?.sheet();
                if areas.iter().any(|a| a.sheet() != sheet) {
                    return Err("dc-snz.2: XLOPER12 cannot hold a 3D reference".into());
                }
                let count = u16::try_from(areas.len()).map_err(|_| "too many reference areas")?;
                let mut words = vec![0u32; 1 + 4 * areas.len()].into_boxed_slice();
                words[0] = u32::from(count);
                for (i, area) in areas.iter().enumerate() {
                    words[1 + 4 * i..5 + 4 * i].copy_from_slice(&[
                        area.first().row() - 1,
                        area.last().row() - 1,
                        area.first().column() - 1,
                        area.last().column() - 1,
                    ]);
                }
                let table = words.as_mut_ptr().cast::<XlMRef12>();
                self.references.push(words);
                XlOper12 {
                    value: XlValue12 {
                        reference: XlReference12 {
                            table,
                            sheet: sheets.encode(sheet),
                        },
                    },
                    xltype: XL_REF,
                }
            }
        })
    }
}

pub fn error_number(error: WorksheetErrorCode) -> Option<i32> {
    Some(match error {
        WorksheetErrorCode::Null => 0,
        WorksheetErrorCode::Div0 => 7,
        WorksheetErrorCode::Value => 15,
        WorksheetErrorCode::Ref => 23,
        WorksheetErrorCode::Name => 29,
        WorksheetErrorCode::Num => 36,
        WorksheetErrorCode::NA => 42,
        WorksheetErrorCode::GettingData => 43,
        _ => return None,
    })
}

fn rectangle(rect: XlRef12, sheet: SheetId) -> Result<GridArea, String> {
    if rect.row_first > rect.row_last || rect.col_first > rect.col_last {
        return Err("inverted XLL rectangle".into());
    }
    let cell = |row: i32, col: i32| {
        let row = u32::try_from(row)
            .map_err(|_| "negative XLL row")?
            .checked_add(1)
            .ok_or("XLL row overflow")?;
        let col = u32::try_from(col)
            .map_err(|_| "negative XLL column")?
            .checked_add(1)
            .ok_or("XLL column overflow")?;
        CellKey::new(sheet, row, col).map_err(|e| e.to_string())
    };
    GridArea::new(
        cell(rect.row_first, rect.col_first)?,
        cell(rect.row_last, rect.col_last)?,
    )
    .map_err(|e| e.to_string())
}

/// Copy an XLOPER12 recursively, ignoring ownership flags (the host releases the original).
/// # Safety
/// All nonnull pointers and their declared lengths must be valid for reading until return.
/// This validates dimensions and supported kinds; it cannot sandbox native pointer corruption.
pub unsafe fn from_oper(
    oper: *const XlOper12,
    sheets: &SheetMap,
    current_sheet: Option<SheetId>,
) -> Result<CalcValue, String> {
    unsafe { read(oper, sheets, current_sheet, 0, &mut 1_048_576usize) }
}

unsafe fn read(
    oper: *const XlOper12,
    sheets: &SheetMap,
    current_sheet: Option<SheetId>,
    depth: usize,
    budget: &mut usize,
) -> Result<CalcValue, String> {
    if oper.is_null() {
        return Ok(CalcValue::error(WorksheetErrorCode::Num));
    }
    if depth > 16 || *budget == 0 {
        return Err("XLL result exceeds conversion limit".into());
    }
    *budget -= 1;
    let oper = unsafe { &*oper };
    Ok(match oper.xltype & TYPE_MASK {
        XL_NUM => {
            let n = unsafe { oper.value.number };
            if n.is_finite() {
                CalcValue::number(n)
            } else {
                CalcValue::error(WorksheetErrorCode::Num)
            }
        }
        XL_INT => CalcValue::number(f64::from(unsafe { oper.value.integer })),
        XL_BOOL => CalcValue::logical(unsafe { oper.value.integer } != 0),
        XL_NIL => CalcValue::empty(),
        XL_MISSING => CalcValue::missing(),
        XL_STR => {
            let ptr = unsafe { oper.value.string };
            if ptr.is_null() {
                return Err("null XLL string".into());
            }
            let len = usize::from(unsafe { *ptr });
            if len > 32767 {
                return Err("XLL string exceeds 32767 UTF-16 units".into());
            }
            let units = unsafe { std::slice::from_raw_parts(ptr.add(1), len) };
            CalcValue::text(ExcelText::from_utf16_code_units(units.to_vec()))
        }
        XL_ERR => {
            let n = unsafe { oper.value.integer };
            let error = WorksheetErrorCode::all()
                .into_iter()
                .find(|e| error_number(*e) == Some(n))
                .ok_or_else(|| format!("dc-snz.2: unknown XLL error {n}"))?;
            CalcValue::error(error)
        }
        XL_MULTI => {
            let array = unsafe { oper.value.array };
            let rows = usize::try_from(array.rows).map_err(|_| "negative XLL array rows")?;
            let cols = usize::try_from(array.columns).map_err(|_| "negative XLL array columns")?;
            let count = rows
                .checked_mul(cols)
                .filter(|n| *n > 0 && *n <= *budget)
                .ok_or("invalid or oversized XLL array")?;
            if array.values.is_null() {
                return Err("null XLL array".into());
            }
            let mut cells = Vec::with_capacity(count);
            for i in 0..count {
                let value = unsafe {
                    read(
                        array.values.add(i),
                        sheets,
                        current_sheet,
                        depth + 1,
                        budget,
                    )?
                };
                if value.as_array().is_some() || value.as_reference().is_some() {
                    return Err("nested XLL array/reference".into());
                }
                cells.push(value);
            }
            CalcValue::array(
                CalcArray::new(ArrayShape { rows, cols }, cells)
                    .ok_or("invalid XLL array shape")?,
            )
        }
        XL_REF => {
            let reference = unsafe { oper.value.reference };
            let sheet = sheets
                .decode(reference.sheet)
                .ok_or("unknown XLL sheet identity")?;
            if reference.table.is_null() {
                return Err("null XLL reference table".into());
            }
            let count = usize::from(unsafe { (*reference.table).count });
            if count == 0 || count > *budget {
                return Err("invalid XLL reference count".into());
            }
            let ptr = unsafe { reference.table.cast::<u8>().add(4).cast::<XlRef12>() };
            let areas = unsafe { std::slice::from_raw_parts(ptr, count) }
                .iter()
                .map(|r| rectangle(*r, sheet))
                .collect::<Result<Vec<_>, _>>()?;
            CalcValue::reference(ReferenceLike::from_areas(areas).map_err(|e| e.to_string())?)
        }
        XL_SREF => {
            let reference = unsafe { oper.value.single_reference };
            if reference.count != 1 {
                return Err("SRef count must be one".into());
            }
            CalcValue::reference(ReferenceLike::area(rectangle(
                reference.rectangle,
                current_sheet.ok_or("SRef without current sheet")?,
            )?))
        }
        kind => {
            return Err(format!(
                "dc-snz.2: XLL type {kind:#x} has no worksheet value conversion"
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Native-only: traverses native ABI pointers and checks allocation lifetime.
    #[test]
    fn values_copy_before_the_arena_is_freed() {
        let mut sheets = SheetMap::default();
        let sheet = calc_value::IdAllocator::<SheetId>::new().issue().unwrap();
        let reference = CalcValue::reference(
            ReferenceLike::from_areas(vec![
                GridArea::cell(CellKey::new(sheet, 1, 1).unwrap()),
                GridArea::cell(CellKey::new(sheet, 1048576, 16384).unwrap()),
            ])
            .unwrap(),
        );
        let values = vec![
            CalcValue::number(3.25),
            CalcValue::logical(true),
            CalcValue::empty(),
            CalcValue::missing(),
            CalcValue::text(ExcelText::from_utf16_code_units(vec![0, 0xd800, 65])),
            CalcValue::error(WorksheetErrorCode::NA),
            reference,
            CalcValue::array(
                CalcArray::from_rows(vec![vec![
                    CalcValue::number(7.),
                    CalcValue::text(ExcelText::from_interop_assignment("x")),
                ]])
                .unwrap(),
            ),
        ];
        for value in values {
            let copy = {
                let mut arena = Arena::new();
                let wire = arena.oper(&value, &mut sheets).unwrap();
                unsafe { from_oper(&wire, &sheets, Some(sheet)) }.unwrap()
            };
            assert_eq!(copy, value);
        }
        let bad = XlOper12 {
            value: XlValue12 {
                array: XlArray12 {
                    values: std::ptr::null_mut(),
                    rows: -1,
                    columns: 2,
                },
            },
            xltype: XL_MULTI,
        };
        assert!(unsafe { from_oper(&bad, &sheets, None) }.is_err());
        let null = unsafe { from_oper(std::ptr::null(), &sheets, None) }.unwrap();
        assert_eq!(null, CalcValue::error(WorksheetErrorCode::Num));
    }
}
