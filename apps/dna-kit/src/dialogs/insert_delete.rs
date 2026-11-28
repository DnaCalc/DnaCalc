//! Insert and Delete: Excel's two four-option dialogs for shifting cells, rows and columns.
//!
//! `Ctrl+Shift+=` and `Ctrl+-` act at once on whole rows or whole columns and open the dialog
//! on any other range, as Excel's do.

use crate::dialog::{Choice, Commit, Family, Field, FieldId, Kind, Spec, State, Tab, Target, Value};
use crate::{Session, ViewCommand};
use calc_skin::grid::Area;
use calc_skin::{DeleteShift, InsertShift, ModelChange, ViewId};

pub const INSERT: &str = "classic.insert";
pub const DELETE: &str = "classic.delete";
pub const INSERT_CELLS: &str = "classic.insert-cells";
pub const DELETE_CELLS: &str = "classic.delete-cells";

const MAX_ROWS: u32 = 1_048_576;
const MAX_COLUMNS: u32 = 16_384;
const SHIFT: FieldId = "shift";

pub static INSERT_FAMILY: Family = Family {
    id: "insert",
    command: INSERT_CELLS,
    label: "Insert Cells",
    chords: &[],
    when: "mode:ready && writable",
    target: one_area,
    spec: |target| spec("insert", "Insert", "Insert", &[
        ("right", "Shift cells r&ight"),
        ("down", "Shift cells &down"),
        ("row", "Entire &row"),
        ("column", "Entire &column"),
    ], if taller(target) { "right" } else { "down" }),
    begin: Family::no_read,
    poll: |_, target, state| ready(state, if taller(target) { "right" } else { "down" }),
    end: Family::no_end,
    enabled: Family::always,
    shown: Family::always,
    changed: Family::unlinked,
    press: Family::unlinked,
    custom: Family::no_custom,
    commit: |state, target| {
        let shift = match state.choice(SHIFT) {
            Some("right") => InsertShift::Right,
            Some("down") => InsertShift::Down,
            Some("row") => InsertShift::EntireRow,
            Some("column") => InsertShift::EntireColumn,
            _ => return Err("Choose how to insert.".into()),
        };
        Ok(Commit::Edit(insert(target, shift)))
    },
};

pub static DELETE_FAMILY: Family = Family {
    id: "delete",
    command: DELETE_CELLS,
    label: "Delete Cells",
    chords: &[],
    when: "mode:ready && writable",
    target: one_area,
    spec: |target| spec("delete", "Delete", "Delete", &[
        ("left", "Shift cells &left"),
        ("up", "Shift cells &up"),
        ("row", "Entire &row"),
        ("column", "Entire &column"),
    ], if taller(target) { "left" } else { "up" }),
    begin: Family::no_read,
    poll: |_, target, state| ready(state, if taller(target) { "left" } else { "up" }),
    end: Family::no_end,
    enabled: Family::always,
    shown: Family::always,
    changed: Family::unlinked,
    press: Family::unlinked,
    custom: Family::no_custom,
    commit: |state, target| {
        let shift = match state.choice(SHIFT) {
            Some("left") => DeleteShift::Left,
            Some("up") => DeleteShift::Up,
            Some("row") => DeleteShift::EntireRow,
            Some("column") => DeleteShift::EntireColumn,
            _ => return Err("Choose how to delete.".into()),
        };
        Ok(Commit::Edit(delete(target, shift)))
    },
};

fn spec(id: &'static str, title: &'static str, group: &'static str, options: &[(&str, &str)], _: &str) -> Spec {
    Spec {
        id,
        title,
        accept: "OK",
        tabs: vec![Tab {
            id: "shift",
            label: title,
            fields: vec![Field::new(
                SHIFT,
                group,
                Kind::Options(options.iter().map(|(value, label)| Choice::new(*value, *label)).collect()),
            )],
        }],
    }
}

fn ready(state: &mut State, default: &str) -> crate::Live<()> {
    state.set(SHIFT, Value::Choice(Some(default.into())));
    crate::Live::Ready(())
}

/// Excel acts on one rectangle: several areas get its "multiple selections" refusal.
fn one_area(session: Session) -> Result<Target, String> {
    let target = Target::selection(session)?;
    if target.areas.len() != 1 {
        return Err("This action won't work on multiple selections.".into());
    }
    Ok(target)
}

/// A range with more rows than columns shifts sideways by default; any other shifts vertically.
fn taller(target: &Target) -> bool {
    target.areas.first().is_some_and(|Area(first, last)| last.2 - first.2 > last.3 - first.3)
}

fn whole_rows(area: &Area) -> bool {
    area.0.3 == 1 && area.1.3 == MAX_COLUMNS
}

fn whole_columns(area: &Area) -> bool {
    area.0.2 == 1 && area.1.2 == MAX_ROWS
}

fn insert(target: &Target, shift: InsertShift) -> calc_skin::Batch {
    target.batch("Insert", ModelChange::InsertCells { selected: target.areas[0], shift })
}

fn delete(target: &Target, shift: DeleteShift) -> calc_skin::Batch {
    target.batch("Delete", ModelChange::DeleteCells { selected: target.areas[0], shift })
}

/// `Ctrl+Shift+=` and `Ctrl+-`: at once on whole rows or columns, the dialog otherwise.
pub fn commands(view: &ViewId, session: Session) -> Vec<ViewCommand> {
    vec![
        ViewCommand::new(view, INSERT, "Insert", &["Ctrl+Shift+=", "Ctrl+Shift++", "Ctrl++"], move |_| {
            let target = one_area(session)?;
            let area = target.areas[0];
            if whole_rows(&area) && !whole_columns(&area) {
                session.submit(insert(&target, InsertShift::EntireRow));
            } else if whole_columns(&area) && !whole_rows(&area) {
                session.submit(insert(&target, InsertShift::EntireColumn));
            } else {
                session.command(INSERT_CELLS).map_err(|error| format!("{error:?}"))?;
            }
            Ok(())
        })
        .when("mode:ready && writable"),
        ViewCommand::new(view, DELETE, "Delete", &["Ctrl+-"], move |_| {
            let target = one_area(session)?;
            let area = target.areas[0];
            if whole_rows(&area) && !whole_columns(&area) {
                session.submit(delete(&target, DeleteShift::EntireRow));
            } else if whole_columns(&area) && !whole_rows(&area) {
                session.submit(delete(&target, DeleteShift::EntireColumn));
            } else {
                session.command(DELETE_CELLS).map_err(|error| format!("{error:?}"))?;
            }
            Ok(())
        })
        .when("mode:ready && writable"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use calc_skin::grid::{Key, Sheet};
    use calc_skin::{Change, Revision, WorkbookId};

    fn target(first: (u32, u32), last: (u32, u32)) -> Target {
        Target {
            workbook: WorkbookId(1),
            revision: Revision(3),
            sheet: Sheet(0, 1),
            areas: vec![Area(Key(0, 1, first.0, first.1), Key(0, 1, last.0, last.1))],
            active: Key(0, 1, first.0, first.1),
        }
    }

    fn opened(family: &Family, target: &Target) -> State {
        let mut state = State::new((family.spec)(target), []);
        // The families read nothing: their poll only sets the default option.
        match family.id {
            "insert" => ready(&mut state, if taller(target) { "right" } else { "down" }),
            _ => ready(&mut state, if taller(target) { "left" } else { "up" }),
        };
        state.settle();
        state
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn insert_has_excels_four_options_and_defaults_by_the_shape_of_the_selection() {
        let wide = target((2, 2), (2, 4));
        let state = opened(&INSERT_FAMILY, &wide);
        let Kind::Options(options) = &state.field(SHIFT).unwrap().kind else { panic!("option buttons") };
        let labels: Vec<_> = options.iter().map(|option| crate::dialog::caption(&option.label)).collect();
        assert_eq!(labels, [
            ("Shift cells right".into(), Some('i')),
            ("Shift cells down".into(), Some('d')),
            ("Entire row".into(), Some('r')),
            ("Entire column".into(), Some('c')),
        ]);
        assert_eq!(state.choice(SHIFT), Some("down"));
        assert_eq!(opened(&INSERT_FAMILY, &target((2, 2), (5, 2))).choice(SHIFT), Some("right"));
        assert_eq!(opened(&INSERT_FAMILY, &target((2, 2), (2, 2))).choice(SHIFT), Some("down"));
        let Ok(Commit::Edit(batch)) = (INSERT_FAMILY.commit)(&state, &wide) else { panic!("an edit") };
        assert_eq!(batch.changes, vec![Change::Model(ModelChange::InsertCells {
            selected: wide.areas[0],
            shift: InsertShift::Down,
        })]);
        assert_eq!((batch.base, batch.label.as_deref()), (Some(Revision(3)), Some("Insert")));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn delete_has_excels_four_options_and_commits_the_chosen_shift() {
        let tall = target((2, 2), (6, 3));
        let mut state = opened(&DELETE_FAMILY, &tall);
        assert_eq!(state.choice(SHIFT), Some("left"));
        assert_eq!(opened(&DELETE_FAMILY, &target((2, 2), (2, 2))).choice(SHIFT), Some("up"));
        state.set(SHIFT, Value::Choice(Some("row".into())));
        let Ok(Commit::Edit(batch)) = (DELETE_FAMILY.commit)(&state, &tall) else { panic!("an edit") };
        assert_eq!(batch.changes, vec![Change::Model(ModelChange::DeleteCells {
            selected: tall.areas[0],
            shift: DeleteShift::EntireRow,
        })]);
        assert!(whole_rows(&Area(Key(0, 1, 3, 1), Key(0, 1, 4, MAX_COLUMNS))));
        assert!(whole_columns(&Area(Key(0, 1, 1, 2), Key(0, 1, MAX_ROWS, 2))));
        assert!(!whole_rows(&tall.areas[0]) && !whole_columns(&tall.areas[0]));
    }
}
