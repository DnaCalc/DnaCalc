//! Row Height and Column Width: Excel's two one-box dialogs on the Home tab's Format menu.
//!
//! Each opens on the size the selected rows or columns share, empty when they differ, and
//! commits one size for every one of them. A size of 0 hides them, as it does in Excel.

use crate::dialog::{Commit, Family, Field, FieldId, Kind, Spec, State, Tab, Target};
use crate::{Live, Session};
use calc_skin::{ModelChange, WorksheetGeometry, query};
use leptos::prelude::*;

pub const ROW_HEIGHT: &str = "classic.row-height";
pub const COLUMN_WIDTH: &str = "classic.column-width";

pub static ROWS: Family = Family {
    id: "row-height",
    command: ROW_HEIGHT,
    label: "Row Height",
    chords: &[],
    when: "mode:ready && writable",
    target: Target::selection,
    spec: |_| spec("row-height", "Row Height", "&Row height:"),
    begin,
    poll: |session, target, state| poll(session, target, state, true),
    end,
    enabled: Family::always,
    shown: Family::always,
    changed: Family::unlinked,
    press: Family::unlinked,
    custom: Family::no_custom,
    commit: |state, target| commit(state, target, true),
};

pub static COLUMNS: Family = Family {
    id: "column-width",
    command: COLUMN_WIDTH,
    label: "Column Width",
    chords: &[],
    when: "mode:ready && writable",
    target: Target::selection,
    spec: |_| spec("column-width", "Column Width", "&Column width:"),
    begin,
    poll: |session, target, state| poll(session, target, state, false),
    end,
    enabled: Family::always,
    shown: Family::always,
    changed: Family::unlinked,
    press: Family::unlinked,
    custom: Family::no_custom,
    commit: |state, target| commit(state, target, false),
};

const SIZE: FieldId = "size";

fn spec(id: &'static str, title: &'static str, label: &'static str) -> Spec {
    Spec {
        id,
        title,
        accept: "OK",
        tabs: vec![Tab {
            id: "size",
            label: title,
            fields: vec![Field::new(SIZE, label, Kind::Edit { max_length: 12 })],
        }],
    }
}

#[derive(Clone, Copy, Default)]
struct Reading {
    watch: RwSignal<Option<dna_skin_runtime::client::Watch<query::Geometry>>>,
}

fn begin(session: Session, target: &Target) {
    end(session);
    let watch = session.watch(query::Geometry { sheet: target.sheet });
    session.local::<Reading>().watch.set(Some(watch));
}

fn end(session: Session) {
    let reading = session.local::<Reading>();
    if let Some(watch) = reading.watch.get_untracked() {
        session.unwatch(watch);
    }
    reading.watch.set(None);
}

fn poll(session: Session, target: &Target, state: &mut State, rows: bool) -> Live<()> {
    let Some(watch) = session.local::<Reading>().watch.get() else {
        return Live::Idle;
    };
    match session.read(watch) {
        Live::Ready(answer) if answer.stamp.workbook == target.workbook && answer.stamp.model == target.revision => {
            let size = shared_size(&answer.body, &bands(target, rows), rows);
            state.set_text(SIZE, size.map(shown).unwrap_or_default());
            Live::Ready(())
        }
        Live::Refused(reason) => Live::Refused(reason),
        _ => Live::Loading,
    }
}

/// The selected rows or columns as inclusive bands in ascending order, joined where they meet.
pub fn bands(target: &Target, rows: bool) -> Vec<(u32, u32)> {
    let mut bands: Vec<(u32, u32)> = target
        .areas
        .iter()
        .map(|area| if rows { (area.0.2, area.1.2) } else { (area.0.3, area.1.3) })
        .collect();
    bands.sort_unstable();
    let mut joined: Vec<(u32, u32)> = Vec::new();
    for (first, last) in bands {
        match joined.last_mut() {
            Some((_, end)) if first <= end.saturating_add(1) => *end = (*end).max(last),
            _ => joined.push((first, last)),
        }
    }
    joined
}

/// The size every row or column of the bands has: its own, the sheet's default where it has
/// none, and 0 where it is hidden. `None` when they differ.
pub fn shared_size(geometry: &WorksheetGeometry, bands: &[(u32, u32)], rows: bool) -> Option<f64> {
    let default = if rows { geometry.default_row_height } else { geometry.default_column_width };
    // Each stored span as (first, last, size).
    let stored: Vec<(u32, u32, f64)> = if rows {
        geometry
            .rows
            .iter()
            .map(|&(row, height, hidden)| (row, row, if hidden { 0.0 } else { height.unwrap_or(default) }))
            .collect()
    } else {
        geometry
            .columns
            .iter()
            .map(|&(first, last, width, hidden)| (first, last, if hidden { 0.0 } else { width.unwrap_or(default) }))
            .collect()
    };
    let mut shared: Option<f64> = None;
    let mut see = |size: f64| match shared {
        None => {
            shared = Some(size);
            true
        }
        Some(seen) => seen == size,
    };
    for &(first, last) in bands {
        let mut covered = 0u64;
        for &(from, to, size) in &stored {
            let (from, to) = (from.max(first), to.min(last));
            if from <= to {
                covered += u64::from(to - from) + 1;
                if !see(size) {
                    return None;
                }
            }
        }
        if covered < u64::from(last - first) + 1 && !see(default) {
            return None;
        }
    }
    shared
}

/// A size as the box shows it: at most two decimal places and no trailing zeros.
fn shown(size: f64) -> String {
    let text = format!("{:.2}", size);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

fn commit(state: &State, target: &Target, rows: bool) -> Result<Commit, String> {
    if !state.changed(SIZE) {
        return Ok(Commit::Nothing);
    }
    let (most, message, label) = if rows {
        (409.0, "Row height must be between 0 and 409.", "Row Height")
    } else {
        (255.0, "Column width must be between 0 and 255.", "Column Width")
    };
    let size = state.text(SIZE).trim().parse::<f64>().ok().filter(|size| (0.0..=most).contains(size)).ok_or(message)?;
    Ok(Commit::Edit(target.batch(label, ModelChange::SetAxisSize {
        sheet: target.sheet,
        rows,
        bands: bands(target, rows),
        size,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_skin::grid::{Area, Key, Sheet};
    use calc_skin::{Revision, WorkbookId};

    fn geometry() -> WorksheetGeometry {
        WorksheetGeometry {
            rows: vec![(2, Some(30.0), false), (3, Some(30.0), false), (5, None, true)],
            columns: vec![(2, 3, Some(12.5), false), (6, 6, Some(20.0), true)],
            default_row_height: 15.0,
            default_column_width: 8.43,
            default_digit_px: 7.0,
            merges: Vec::new(),
            frozen_rows: 0,
            frozen_columns: 0,
            zoom: 100,
            show_grid_lines: None,
            auto_filter_areas: Vec::new(),
        }
    }

    fn target(areas: &[((u32, u32), (u32, u32))]) -> Target {
        Target {
            workbook: WorkbookId(1),
            revision: Revision(0),
            sheet: Sheet(0, 1),
            areas: areas.iter().map(|(first, last)| Area(Key(0, 1, first.0, first.1), Key(0, 1, last.0, last.1))).collect(),
            active: Key(0, 1, areas[0].0.0, areas[0].0.1),
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_box_opens_on_the_shared_size_and_is_empty_where_sizes_differ() {
        let geometry = geometry();
        assert_eq!(shared_size(&geometry, &[(2, 3)], true), Some(30.0));
        assert_eq!(shared_size(&geometry, &[(1, 1)], true), Some(15.0), "a row with no height has the default");
        assert_eq!(shared_size(&geometry, &[(1, 2)], true), None);
        assert_eq!(shared_size(&geometry, &[(5, 5)], true), Some(0.0), "a hidden row shows 0");
        assert_eq!(shared_size(&geometry, &[(1, 1_048_576)], true), None);
        assert_eq!(shared_size(&geometry, &[(7, 1_048_576)], true), Some(15.0));
        assert_eq!(shared_size(&geometry, &[(2, 3)], false), Some(12.5));
        assert_eq!(shared_size(&geometry, &[(3, 4)], false), None);
        assert_eq!(shared_size(&geometry, &[(2, 2), (3, 3)], false), Some(12.5));
        assert_eq!(shared_size(&geometry, &[(1, 1), (4, 5)], false), Some(8.43));
        assert_eq!((shown(15.0), shown(8.43), shown(12.5), shown(0.0)), ("15".into(), "8.43".into(), "12.5".into(), "0".into()));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn bands_join_touching_areas_and_ok_commits_one_size_within_excels_limits() {
        let target = target(&[((4, 2), (5, 3)), ((2, 5), (3, 5)), ((9, 1), (9, 1))]);
        assert_eq!(bands(&target, true), [(2, 5), (9, 9)]);
        assert_eq!(bands(&target, false), [(1, 3), (5, 5)]);
        let mut state = State::new(spec("row-height", "Row Height", "&Row height:"), []);
        state.set_text(SIZE, "15");
        state.settle();
        assert_eq!(commit(&state, &target, true), Ok(Commit::Nothing));
        state.set_text(SIZE, "22.5");
        let Ok(Commit::Edit(batch)) = commit(&state, &target, true) else { panic!("an edit") };
        assert_eq!(batch.label.as_deref(), Some("Row Height"));
        assert_eq!(batch.changes, vec![calc_skin::Change::Model(ModelChange::SetAxisSize {
            sheet: Sheet(0, 1),
            rows: true,
            bands: vec![(2, 5), (9, 9)],
            size: 22.5,
        })]);
        for refused in ["410", "-1", "tall", ""] {
            state.set_text(SIZE, refused);
            assert_eq!(commit(&state, &target, true), Err("Row height must be between 0 and 409.".into()), "{refused}");
        }
        state.set_text(SIZE, "256");
        assert_eq!(commit(&state, &target, false), Err("Column width must be between 0 and 255.".into()));
        state.set_text(SIZE, "0");
        assert!(matches!(commit(&state, &target, false), Ok(Commit::Edit(_))), "0 hides the columns");
    }
}
