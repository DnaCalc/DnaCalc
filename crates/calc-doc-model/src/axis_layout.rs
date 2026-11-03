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
    outline_level: u8,
    collapsed: u8,
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
            outline_level: 0,
            collapsed: 0,
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
    pub fn outline_level(self) -> u8 {
        self.outline_level
    }
    /// One bit for each collapsed child level at this summary band.
    pub fn collapsed(self) -> u8 {
        self.collapsed
    }
    pub fn with_outline(self, level: u8, collapsed: u8) -> Option<Self> {
        (level <= 7 && collapsed < 128).then_some(Self {
            outline_level: level,
            collapsed,
            ..self
        })
    }
    pub fn with_size(self, size: Option<f64>, custom: bool) -> Option<Self> {
        let sized = Self::new(size, self.hidden, custom)?;
        Some(Self {
            size: sized.size,
            custom,
            ..self
        })
    }
    pub fn with_hidden(self, hidden: bool) -> Self {
        Self { hidden, ..self }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AxisLayouts {
    rows: BTreeMap<(SheetId, u32), AxisFormat>,
    columns: BTreeMap<(SheetId, u32), AxisFormat>,
    summary_before: std::collections::BTreeSet<(SheetId, bool)>,
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
        if let Some(after) = edit.summary_after {
            let key = (sheet, axis == Axis::Rows);
            if after {
                self.summary_before.remove(&key);
            } else {
                self.summary_before.insert(key);
            }
        }
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
        self.summary_before.retain(|(s, _)| *s != sheet);
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
            let format = format
                .with_outline(format.outline_level(), 0)
                .expect("inherited outline level");
            if format != AxisFormat::default() {
                for index in edit.first()..=edit.last() {
                    target.insert((edit.sheet(), index), format);
                }
            }
        }
        // Reattach each surviving collapsed group to its derived summary coordinate.
        // The summary can change identity when insertion precedes the first detail band.
        let groups = next.outline_groups(edit.sheet(), edit.axis());
        let markers: Vec<_> = next
            .bands(edit.sheet(), edit.axis())
            .filter(|(_, f)| f.collapsed() != 0)
            .collect();
        for (index, f) in markers {
            next.set(
                AxisFormatEdit::new(
                    edit.sheet(),
                    edit.axis(),
                    index,
                    f.with_outline(f.outline_level(), 0).unwrap(),
                )
                .unwrap(),
            );
        }
        for old in self
            .outline_groups(edit.sheet(), edit.axis())
            .into_iter()
            .filter(|g| g.collapsed)
        {
            let Some((first, mut last)) = crate::moved_run(edit, old.first, old.last) else {
                continue;
            };
            if edit.change() == AxisChange::Insert && edit.first() == old.last + 1 {
                last = last.saturating_add(edit.count()).min(edit.axis().size());
            }
            if let Some(group) = groups.iter().find(|g| {
                g.first == first
                    && g.last == last
                    && g.level == old.level
                    && g.summary >= 1
                    && g.summary <= edit.axis().size()
            }) {
                let f = next.get(edit.sheet(), edit.axis(), group.summary);
                next.set(
                    AxisFormatEdit::new(
                        edit.sheet(),
                        edit.axis(),
                        group.summary,
                        f.with_outline(f.outline_level(), f.collapsed() | (1 << (group.level - 1)))
                            .unwrap(),
                    )
                    .unwrap(),
                );
            }
        }
        next
    }
}

/// A contiguous run at one outline level; its control is on the adjacent summary band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutlineGroup {
    pub first: u32,
    pub last: u32,
    pub level: u8,
    pub summary: u32,
    pub collapsed: bool,
}
impl AxisLayouts {
    pub fn summary_after(&self, sheet: SheetId, axis: Axis) -> bool {
        !self.summary_before.contains(&(sheet, axis == Axis::Rows))
    }
    pub fn outline_groups(&self, sheet: SheetId, axis: Axis) -> Vec<OutlineGroup> {
        let bands: Vec<_> = self
            .bands(sheet, axis)
            .filter(|(_, f)| f.outline_level > 0)
            .collect();
        let mut groups = Vec::new();
        for level in 1..=7 {
            let mut run: Option<(u32, u32)> = None;
            let mut emit = |first: u32, last: u32| {
                let summary = if self.summary_after(sheet, axis) {
                    last.checked_add(1)
                } else {
                    first.checked_sub(1)
                }
                .unwrap_or(0);
                groups.push(OutlineGroup {
                    first,
                    last,
                    level,
                    summary,
                    collapsed: self.get(sheet, axis, summary).collapsed & (1 << (level - 1)) != 0,
                });
            };
            for &(index, format) in &bands {
                if format.outline_level < level {
                    continue;
                }
                match run {
                    Some((first, last)) if last + 1 == index => run = Some((first, index)),
                    Some((first, last)) => {
                        emit(first, last);
                        run = Some((index, index));
                    }
                    None => run = Some((index, index)),
                }
            }
            if let Some((first, last)) = run {
                emit(first, last);
            }
        }
        groups
    }
    /// Add outline hiding to a disposable physical projection, retaining authored manual flags.
    pub fn outlined(&self) -> Self {
        let mut visible = self.clone();
        let sheets: std::collections::BTreeSet<_> = self.sheets().collect();
        for sheet in sheets {
            for axis in [Axis::Rows, Axis::Columns] {
                for group in self
                    .outline_groups(sheet, axis)
                    .into_iter()
                    .filter(|g| g.collapsed)
                {
                    for index in group.first..=group.last {
                        let format = visible.get(sheet, axis, index).with_hidden(true);
                        visible.set(
                            AxisFormatEdit::new(sheet, axis, index, format).expect("outline band"),
                        );
                    }
                }
            }
        }
        visible
    }
}

/// One validated physical-band change; one host batch can contain several.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AxisFormatEdit {
    sheet: SheetId,
    axis: Axis,
    index: u32,
    format: AxisFormat,
    summary_after: Option<bool>,
}
impl AxisFormatEdit {
    pub fn new(sheet: SheetId, axis: Axis, index: u32, format: AxisFormat) -> Option<Self> {
        (index >= 1 && index <= axis.size()).then_some(Self {
            sheet,
            axis,
            index,
            format,
            summary_after: None,
        })
    }
    pub fn with_summary_after(mut self, after: bool) -> Self {
        self.summary_after = Some(after);
        self
    }
    pub fn summary_after(self) -> Option<bool> {
        self.summary_after
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
