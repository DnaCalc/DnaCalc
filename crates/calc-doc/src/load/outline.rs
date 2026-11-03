//! Outline source fields and the source-checked independent visibility extension.
use super::{Events, LoadError, MAIN_NAMESPACE, geometry::SheetGeometry};
use crate::xml::{XmlEvent, XmlPart};
use calc_doc_model::value::SheetId;
use calc_doc_model::{Axis, AxisFormatEdit, AxisLayouts};
use std::collections::BTreeSet;
pub(crate) const NAMESPACE: &str = "urn:dnacalc:outline:v1";
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct OutlineState {
    rows: Vec<(u32, u8)>,
    columns: Vec<(u32, u8)>,
    manual_rows: BTreeSet<u32>,
    manual_columns: BTreeSet<u32>,
}
impl OutlineState {
    fn valid(&self, geometry: &SheetGeometry) -> bool {
        let mut ids = calc_doc_model::value::IdAllocator::<SheetId>::new();
        let sheet = ids.issue().expect("first sheet");
        let mut layouts = AxisLayouts::default();
        let mut plain = geometry.clone();
        plain.outline_state = None;
        plain.add_to_model(sheet, &mut layouts);
        for (axis, masks, manual) in [
            (Axis::Rows, &self.rows, &self.manual_rows),
            (Axis::Columns, &self.columns, &self.manual_columns),
        ] {
            let physical = |i: u32| {
                match axis {
                    Axis::Rows => geometry.rows.get(&i).map(|f| (f.hidden, f.collapsed)),
                    Axis::Columns => geometry.column(i).map(|f| (f.hidden, f.collapsed)),
                }
                .unwrap_or((false, false))
            };
            let groups = layouts.outline_groups(sheet, axis);
            let mut seen = BTreeSet::new();
            for &(index, mask) in masks {
                if !seen.insert(index) || !physical(index).1 {
                    return false;
                }
                for level in 1..=7 {
                    if mask & (1 << (level - 1)) != 0
                        && !groups.iter().any(|g| {
                            g.summary == index
                                && g.level == level
                                && (g.first..=g.last).all(|i| physical(i).0)
                        })
                    {
                        return false;
                    }
                }
            }
            if manual.iter().any(|i| !physical(*i).0) {
                return false;
            }
            if layouts
                .bands(sheet, axis)
                .any(|(i, _)| physical(i).1 && !seen.contains(&i))
            {
                return false;
            }
        }
        true
    }
    pub fn restore(&self, sheet: SheetId, layouts: &mut AxisLayouts) {
        for (axis, masks, manual) in [
            (Axis::Rows, &self.rows, &self.manual_rows),
            (Axis::Columns, &self.columns, &self.manual_columns),
        ] {
            let indices: BTreeSet<_> = layouts
                .bands(sheet, axis)
                .map(|(i, _)| i)
                .chain(manual.iter().copied())
                .chain(masks.iter().map(|(i, _)| *i))
                .collect();
            for i in indices {
                let f = layouts.get(sheet, axis, i);
                let mask = masks.iter().find(|(j, _)| *j == i).map_or(0, |(_, m)| *m);
                layouts.set(
                    AxisFormatEdit::new(
                        sheet,
                        axis,
                        i,
                        f.with_hidden(manual.contains(&i))
                            .with_outline(f.outline_level(), mask)
                            .unwrap(),
                    )
                    .unwrap(),
                );
            }
        }
    }
}
fn indices(s: Option<&str>, max: u32) -> Option<BTreeSet<u32>> {
    s?.split_whitespace()
        .map(|x| x.parse::<u32>().ok().filter(|n| *n >= 1 && *n <= max))
        .collect()
}
fn masks(s: Option<&str>, max: u32) -> Option<Vec<(u32, u8)>> {
    s?.split_whitespace()
        .map(|x| {
            let (i, m) = x.split_once(':')?;
            Some((
                i.parse::<u32>().ok().filter(|n| *n >= 1 && *n <= max)?,
                m.parse::<u8>().ok().filter(|n| *n > 0 && *n < 128)?,
            ))
        })
        .collect()
}
pub(crate) fn state_checksum(
    source: u32,
    rows: &str,
    columns: &str,
    manual_rows: &str,
    manual_columns: &str,
) -> u32 {
    crate::zip::crc32::crc32(
        format!("{source};{rows};{columns};{manual_rows};{manual_columns}").as_bytes(),
    )
}
pub(crate) fn signature(geometry: &SheetGeometry) -> u32 {
    let mut bytes = Vec::new();
    bytes.extend([
        u8::from(geometry.summary_below.unwrap_or(true)),
        u8::from(geometry.summary_right.unwrap_or(true)),
    ]);
    for (i, f) in &geometry.rows {
        if f.outline_level > 0 || f.collapsed || f.hidden {
            bytes.push(b'r');
            bytes.extend(i.to_le_bytes());
            bytes.extend([f.outline_level, u8::from(f.collapsed), u8::from(f.hidden)]);
        }
    }
    for i in 1..=16384 {
        if let Some(f) = geometry.column(i) {
            if f.outline_level > 0 || f.collapsed || f.hidden {
                bytes.push(b'c');
                bytes.extend(i.to_le_bytes());
                bytes.extend([f.outline_level, u8::from(f.collapsed), u8::from(f.hidden)]);
            }
        }
    }
    crate::zip::crc32::crc32(&bytes)
}
/// Read only physical outline fields. The checksum excludes extensions and dimensions, so
/// unrelated retained metadata has no effect; external outline/visibility edits invalidate it.
pub(crate) fn read(xml: &XmlPart, geometry: &mut SheetGeometry) -> Result<(), LoadError> {
    let mut events = Events::new(xml);
    let mut state = None;
    let mut crc = None;
    loop {
        match events.next()? {
            Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "outlinePr") => {
                geometry.summary_below = e
                    .attribute("summaryBelow")
                    .map(|s| matches!(s, "1" | "true"));
                geometry.summary_right = e
                    .attribute("summaryRight")
                    .map(|s| matches!(s, "1" | "true"));
            }
            Some(XmlEvent::Start(e)) if e.name.is(NAMESPACE, "state") => {
                let parsed = (|| {
                    Some(OutlineState {
                        rows: masks(e.attribute("rows"), 1048576)?,
                        columns: masks(e.attribute("columns"), 16384)?,
                        manual_rows: indices(e.attribute("manualRows"), 1048576)?,
                        manual_columns: indices(e.attribute("manualColumns"), 16384)?,
                    })
                })();
                crc = e.attribute("source").and_then(|s| s.parse::<u32>().ok());
                let integrity = e.attribute("integrity").and_then(|s| s.parse::<u32>().ok());
                let expected = (|| {
                    Some(state_checksum(
                        crc?,
                        e.attribute("rows")?,
                        e.attribute("columns")?,
                        e.attribute("manualRows")?,
                        e.attribute("manualColumns")?,
                    ))
                })();
                state = parsed.filter(|_| integrity.is_some() && integrity == expected);
            }
            None => break,
            _ => {}
        }
    }
    if crc == Some(signature(geometry)) {
        geometry.outline_state = state.filter(|s| s.valid(geometry));
    }
    let mut ids = calc_doc_model::value::IdAllocator::<SheetId>::new();
    let sheet = ids.issue().expect("first sheet");
    let mut layouts = AxisLayouts::default();
    geometry.add_to_model(sheet, &mut layouts);
    for axis in [Axis::Rows, Axis::Columns] {
        let markers: Vec<_> = match axis {
            Axis::Rows => geometry
                .rows
                .iter()
                .filter(|(_, f)| f.collapsed)
                .map(|(i, _)| *i)
                .collect(),
            Axis::Columns => geometry
                .columns
                .iter()
                .filter(|f| f.collapsed)
                .flat_map(|f| f.first..=f.last)
                .collect(),
        };
        geometry.outline_unsupported |= markers
            .into_iter()
            .any(|i| layouts.get(sheet, axis, i).collapsed() == 0)
            || layouts
                .outline_groups(sheet, axis)
                .iter()
                .any(|g| g.summary < 1 || g.summary > axis.size());
    }
    Ok(())
}
pub(crate) fn physical(xml: &XmlPart) -> Result<SheetGeometry, LoadError> {
    let mut geometry = SheetGeometry::default();
    let mut events = Events::new(xml);
    loop {
        match events.next()? {
            Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "row") => {
                if let Some(i) = e.attribute("r").and_then(|s| s.parse::<u32>().ok()) {
                    geometry.read_row(i, &e);
                }
            }
            Some(XmlEvent::Start(e)) if e.name.is(MAIN_NAMESPACE, "col") => {
                geometry.read_column(&e)
            }
            None => break,
            _ => {}
        }
    }
    read(xml, &mut geometry)?;
    Ok(geometry)
}
