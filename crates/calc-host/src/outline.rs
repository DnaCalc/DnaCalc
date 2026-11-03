//! Outline commands use the existing axis edit, history and visibility path.
use crate::{CellEdit, HostState};
use calc_doc_model::{Axis, AxisFormatEdit};
use calc_value::{CellKey, SheetId};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutlineChange {
    Group {
        first: u32,
        last: u32,
    },
    Ungroup {
        first: u32,
        last: u32,
    },
    Detail {
        first: u32,
        last: u32,
        level: Option<u8>,
        show: bool,
    },
    ShowLevel(u8),
    SummaryAfter(bool),
}
pub(crate) fn prepare(
    host: &HostState,
    sheet: SheetId,
    rows: bool,
    revision: u64,
    change: OutlineChange,
) -> Result<Vec<CellEdit>, String> {
    if host.revision().get() != revision {
        return Err("The workbook changed before outlining (dc-971p.39)".into());
    }
    crate::can_edit(
        host,
        CellKey::new(sheet, 1, 1).map_err(|_| "Invalid worksheet")?,
    )?;
    let axis = if rows { Axis::Rows } else { Axis::Columns };
    let old = host.model().definitions().axis_layouts();
    let mut next = old.clone();
    let groups = old.outline_groups(sheet, axis);
    let range = |first: u32, last: u32| -> Result<(), String> {
        if first < 1 || first > last || last > axis.size() || last - first >= 16384 {
            Err("Outline at most 16384 valid rows or columns at once (dc-971p.39)".into())
        } else {
            Ok(())
        }
    };
    let put = |next: &mut calc_doc_model::AxisLayouts, index: u32, level: u8, mask: u8| {
        let f = next
            .get(sheet, axis, index)
            .with_outline(level, mask)
            .expect("checked outline level");
        next.set(AxisFormatEdit::new(sheet, axis, index, f).expect("outline coordinate"));
    };
    match change {
        OutlineChange::Group { first, last } | OutlineChange::Ungroup { first, last } => {
            range(first, last)?;
            let add = matches!(change, OutlineChange::Group { .. });
            if add && (first..=last).any(|i| old.get(sheet, axis, i).outline_level() == 7) {
                return Err("Excel outlines have at most seven grouped levels (dc-971p.39)".into());
            }
            for index in first..=last {
                let level = old.get(sheet, axis, index).outline_level();
                put(
                    &mut next,
                    index,
                    if add {
                        level + 1
                    } else {
                        level.saturating_sub(1)
                    },
                    0,
                );
            }
            // Rebuild control bits from surviving groups, keeping independently collapsed children.
            for (index, f) in old.bands(sheet, axis) {
                if f.collapsed() != 0 {
                    let level = next.get(sheet, axis, index).outline_level();
                    put(&mut next, index, level, 0);
                }
            }
            for group in next.outline_groups(sheet, axis) {
                if group.summary < 1 || group.summary > axis.size() {
                    return Err("An outline needs an adjacent summary row or column inside the worksheet (dc-971p.39)".into());
                }
                if groups.iter().any(|g| {
                    let level = if first <= g.first && last >= g.last {
                        if add {
                            g.level + 1
                        } else {
                            g.level.saturating_sub(1)
                        }
                    } else {
                        g.level
                    };
                    g.first == group.first
                        && g.last == group.last
                        && g.collapsed
                        && level == group.level
                }) {
                    let f = next.get(sheet, axis, group.summary);
                    put(
                        &mut next,
                        group.summary,
                        f.outline_level(),
                        f.collapsed() | (1 << (group.level - 1)),
                    );
                }
            }
            if !add {
                // Documented Excel behaviour: removing outline structure keeps hidden detail.
                // https://support.microsoft.com/en-au/excel/outline-group-data-in-a-worksheet
                let before = old.outlined();
                let after = next.outlined();
                let hidden: Vec<_> = before
                    .bands(sheet, axis)
                    .filter(|(i, f)| f.hidden() && !after.get(sheet, axis, *i).hidden())
                    .map(|(i, _)| i)
                    .collect();
                for index in hidden {
                    let f = next.get(sheet, axis, index).with_hidden(true);
                    next.set(AxisFormatEdit::new(sheet, axis, index, f).unwrap());
                }
            }
        }
        OutlineChange::Detail {
            first,
            last,
            level,
            show,
        } => {
            range(first, last)?;
            let mut candidates: Vec<_> = groups
                .iter()
                .filter(|g| {
                    level.is_none_or(|l| l == g.level)
                        && ((first >= g.first && last <= g.last)
                            || (first == last && first == g.summary))
                })
                .collect();
            candidates.sort_by_key(|g| (g.last - g.first, std::cmp::Reverse(g.level)));
            let group = candidates
                .first()
                .ok_or("Select grouped detail or its summary band (dc-971p.39)")?;
            if group.summary < 1 || group.summary > axis.size() {
                return Err(
                    "The outline has no summary band inside the worksheet (dc-971p.39)".into(),
                );
            }
            let f = next.get(sheet, axis, group.summary);
            let bit = 1 << (group.level - 1);
            put(
                &mut next,
                group.summary,
                f.outline_level(),
                if show {
                    f.collapsed() & !bit
                } else {
                    f.collapsed() | bit
                },
            );
        }
        OutlineChange::ShowLevel(level) => {
            if !(1..=8).contains(&level) {
                return Err("Choose an outline level from 1 to 8 (dc-971p.39)".into());
            }
            for group in &groups {
                if group.summary < 1 || group.summary > axis.size() {
                    return Err(
                        "The outline has no summary band inside the worksheet (dc-971p.39)".into(),
                    );
                }
                let f = next.get(sheet, axis, group.summary);
                let bit = 1 << (group.level - 1);
                put(
                    &mut next,
                    group.summary,
                    f.outline_level(),
                    if group.level >= level {
                        f.collapsed() | bit
                    } else {
                        f.collapsed() & !bit
                    },
                );
            }
        }
        OutlineChange::SummaryAfter(after) => {
            let f = next.get(sheet, axis, 1);
            next.set(
                AxisFormatEdit::new(sheet, axis, 1, f)
                    .unwrap()
                    .with_summary_after(after),
            );
            for (index, f) in old.bands(sheet, axis) {
                if f.collapsed() != 0 {
                    let level = f.outline_level();
                    put(&mut next, index, level, 0);
                }
            }
            for group in next.outline_groups(sheet, axis) {
                if group.summary < 1 || group.summary > axis.size() {
                    return Err("This summary direction would put a control outside the worksheet (dc-971p.39)".into());
                }
                if groups.iter().any(|g| {
                    g.first == group.first
                        && g.last == group.last
                        && g.level == group.level
                        && g.collapsed
                }) {
                    let f = next.get(sheet, axis, group.summary);
                    put(
                        &mut next,
                        group.summary,
                        f.outline_level(),
                        f.collapsed() | (1 << (group.level - 1)),
                    );
                }
            }
        }
    }
    let indices: std::collections::BTreeSet<_> = old
        .bands(sheet, axis)
        .chain(next.bands(sheet, axis))
        .map(|(i, _)| i)
        .chain([1])
        .collect();
    let mut edits = Vec::new();
    for index in indices {
        let (a, b) = (old.get(sheet, axis, index), next.get(sheet, axis, index));
        let summary =
            index == 1 && old.summary_after(sheet, axis) != next.summary_after(sheet, axis);
        if a != b || summary {
            let edit = AxisFormatEdit::new(sheet, axis, index, b).unwrap();
            edits.push(CellEdit::AxisFormat(if summary {
                edit.with_summary_after(next.summary_after(sheet, axis))
            } else {
                edit
            }));
        }
    }
    // Reuse visibility preflight so opaque imported filters cannot lose their hidden state.
    for edit in &edits {
        if let CellEdit::AxisFormat(e) = edit {
            crate::actions::axis_format_edits(
                host,
                sheet,
                rows,
                e.index(),
                1,
                None,
                Some(e.format().hidden()),
            )?;
        }
    }
    Ok(edits)
}
