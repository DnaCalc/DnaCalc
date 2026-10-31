//! Authored physical row and column dimensions, independent of any application viewport.
use crate::value::SheetId;
use crate::{Axis, AxisChange, AxisEdit, FiniteNumber, moved_position};
use std::collections::BTreeMap;

/// A band's stated size and hidden flag. Row sizes are points; column sizes use the file's
/// padded character units. `custom` is a column's customWidth flag; it is unused for rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AxisFormat {
    size: Option<FiniteNumber>,
    hidden: bool,
    custom: bool,
}
impl AxisFormat {
    pub fn new(size: Option<f64>, hidden: bool, custom: bool) -> Option<Self> {
        let size = match size {
            Some(n) if n >= 0.0 => Some(FiniteNumber::new(n)?),
            Some(_) => return None,
            None => None,
        };
        Some(Self {
            size,
            hidden,
            custom,
        })
    }
    pub fn size(self) -> Option<f64> {
        self.size.map(FiniteNumber::get)
    }
    pub fn hidden(self) -> bool {
        self.hidden
    }
    pub fn custom(self) -> bool {
        self.custom
    }
    pub fn with_hidden(self, hidden: bool) -> Self {
        Self { hidden, ..self }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AxisLayouts {
    rows: BTreeMap<(SheetId, u32), AxisFormat>,
    columns: BTreeMap<(SheetId, u32), AxisFormat>,
}
impl AxisLayouts {
    fn map(&self, axis: Axis) -> &BTreeMap<(SheetId, u32), AxisFormat> {
        match axis {
            Axis::Rows => &self.rows,
            Axis::Columns => &self.columns,
        }
    }
    fn map_mut(&mut self, axis: Axis) -> &mut BTreeMap<(SheetId, u32), AxisFormat> {
        match axis {
            Axis::Rows => &mut self.rows,
            Axis::Columns => &mut self.columns,
        }
    }
    pub fn get(&self, sheet: SheetId, axis: Axis, index: u32) -> AxisFormat {
        self.map(axis)
            .get(&(sheet, index))
            .copied()
            .unwrap_or_default()
    }
    pub fn bands(
        &self,
        sheet: SheetId,
        axis: Axis,
    ) -> impl Iterator<Item = (u32, AxisFormat)> + '_ {
        self.map(axis)
            .range((sheet, 1)..=(sheet, axis.size()))
            .map(|((_, i), f)| (*i, *f))
    }
    pub fn set(&mut self, edit: AxisFormatEdit) {
        let (sheet, axis, index, format) = (edit.sheet, edit.axis, edit.index, edit.format);
        if format == AxisFormat::default() {
            self.map_mut(axis).remove(&(sheet, index));
        } else {
            self.map_mut(axis).insert((sheet, index), format);
        }
    }
    pub fn sheets(&self) -> impl Iterator<Item = SheetId> + '_ {
        self.rows.keys().chain(self.columns.keys()).map(|(s, _)| *s)
    }
    pub fn without_sheet(&mut self, sheet: SheetId) {
        self.rows.retain(|(s, _), _| *s != sheet);
        self.columns.retain(|(s, _), _| *s != sheet);
    }
    pub fn moved(&self, edit: AxisEdit) -> Self {
        let mut next = self.clone();
        let target = next.map_mut(edit.axis());
        target.retain(|(s, _), _| *s != edit.sheet());
        for (index, format) in self.bands(edit.sheet(), edit.axis()) {
            if let Some(index) = moved_position(edit, index) {
                target.insert((edit.sheet(), index), format);
            }
        }
        if edit.change() == AxisChange::Insert && edit.first() > 1 {
            let format = self
                .get(edit.sheet(), edit.axis(), edit.first() - 1)
                .with_hidden(false);
            if format != AxisFormat::default() {
                for index in edit.first()..=edit.last() {
                    target.insert((edit.sheet(), index), format);
                }
            }
        }
        next
    }
}

/// One validated physical-band change; one host batch can contain several.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AxisFormatEdit {
    sheet: SheetId,
    axis: Axis,
    index: u32,
    format: AxisFormat,
}
impl AxisFormatEdit {
    pub fn new(sheet: SheetId, axis: Axis, index: u32, format: AxisFormat) -> Option<Self> {
        (index >= 1 && index <= axis.size()).then_some(Self {
            sheet,
            axis,
            index,
            format,
        })
    }
    pub fn sheet(self) -> SheetId {
        self.sheet
    }
    pub fn axis(self) -> Axis {
        self.axis
    }
    pub fn index(self) -> u32 {
        self.index
    }
    pub fn format(self) -> AxisFormat {
        self.format
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn inserted_bands_inherit_size_but_not_hiding_and_deleted_bands_disappear() {
        let mut ids = crate::value::IdAllocator::<SheetId>::new();
        let sheet = ids.issue().unwrap();
        let format = AxisFormat::new(Some(20.7109375), true, true).unwrap();
        let mut layouts = AxisLayouts::default();
        layouts.set(AxisFormatEdit::new(sheet, Axis::Columns, 2, format).unwrap());
        assert!(AxisFormatEdit::new(sheet, Axis::Columns, 0, format).is_none());
        assert!(AxisFormatEdit::new(sheet, Axis::Columns, 16_385, format).is_none());
        let inserted =
            layouts.moved(AxisEdit::new(sheet, Axis::Columns, AxisChange::Insert, 3, 2).unwrap());
        assert_eq!(inserted.get(sheet, Axis::Columns, 2), format);
        for index in [3, 4] {
            assert_eq!(
                inserted.get(sheet, Axis::Columns, index),
                format.with_hidden(false)
            );
        }
        let deleted =
            inserted.moved(AxisEdit::new(sheet, Axis::Columns, AxisChange::Delete, 2, 2).unwrap());
        assert_eq!(
            deleted.get(sheet, Axis::Columns, 2),
            format.with_hidden(false)
        );
        assert_eq!(deleted.bands(sheet, Axis::Columns).count(), 1);
    }
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn physical_visibility_and_calculation_hidden_rows_stay_consistent() {
        let mut ids = crate::value::IdAllocator::<SheetId>::new();
        let sheet = ids.issue().unwrap();
        let hidden = AxisFormat::new(None, true, false).unwrap();
        let mut layouts = AxisLayouts::default();
        layouts.set(AxisFormatEdit::new(sheet, Axis::Rows, 2, hidden).unwrap());
        let definitions = crate::Definitions::default()
            .with_axis_layouts(layouts)
            .with_hidden_rows([(sheet, 2, crate::RowHidden::Filtered)]);
        let visible = definitions.with_axis_format(
            AxisFormatEdit::new(sheet, Axis::Rows, 2, hidden.with_hidden(false)).unwrap(),
        );
        assert!(visible.hidden_rows(sheet, 2, 2).next().is_none());
        assert!(!visible.axis_layouts().get(sheet, Axis::Rows, 2).hidden());
        let hidden_again =
            visible.with_axis_format(AxisFormatEdit::new(sheet, Axis::Rows, 2, hidden).unwrap());
        assert_eq!(
            hidden_again.hidden_rows(sheet, 2, 2).collect::<Vec<_>>(),
            vec![(2, crate::RowHidden::Manual)]
        );
    }
}
