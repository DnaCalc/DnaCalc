//! Merge and unmerge cells (Excel's Merge Cells command and `Range.Merge`, `UnMerge`,
//! `MergeCells`): the authored merged-area set of a worksheet, replaced as a whole (dc-oc78o).
use crate::{CellEdit, HostState};
use calc_value::{CellKey, GridArea};

fn meets(a: GridArea, b: GridArea) -> bool {
    a.first().row() <= b.last().row()
        && a.last().row() >= b.first().row()
        && a.first().column() <= b.last().column()
        && a.last().column() >= b.first().column()
}

fn area(sheet: calc_value::SheetId, top: u32, left: u32, bottom: u32, right: u32) -> GridArea {
    GridArea::new(
        CellKey::new(sheet, top, left).expect("a cell"),
        CellKey::new(sheet, bottom, right).expect("a cell"),
    )
    .expect("an area")
}

fn one_sheet(host: &HostState, areas: &[GridArea]) -> Result<calc_value::SheetId, String> {
    let [first, rest @ ..] = areas else {
        return Err("Select cells to merge".into());
    };
    let sheet = first.sheet();
    if rest.iter().any(|a| a.sheet() != sheet) {
        return Err("Merge cells on one worksheet".into());
    }
    crate::can_edit(host, first.first())?;
    Ok(sheet)
}

fn finish(sheet: calc_value::SheetId, mut merges: Vec<GridArea>) -> CellEdit {
    merges.sort_by_key(|a| (a.first().row(), a.first().column()));
    CellEdit::Merges {
        sheet,
        areas: merges,
    }
}

/// Merge each area into one cell holding the upper-left value; with `across`, each row of an
/// area separately. A merged area meeting a new one is unmerged first.
pub(crate) fn prepare_merge(
    host: &HostState,
    areas: &[GridArea],
    across: bool,
) -> Result<Vec<CellEdit>, String> {
    let sheet = one_sheet(host, areas)?;
    let mut rects = Vec::new();
    for a in areas {
        let (t, l, b, r) = (a.first().row(), a.first().column(), a.last().row(), a.last().column());
        if across {
            rects.extend((t..=b).map(|row| area(sheet, row, l, row, r)));
        } else {
            rects.push(*a);
        }
    }
    rects.retain(|r| r.first() != r.last());
    if rects.is_empty() {
        return Ok(Vec::new());
    }
    let definitions = host.model().definitions();
    for rect in &rects {
        if definitions.arrays_meeting(*rect).next().is_some() {
            return Err("Cannot merge part of an array formula".into());
        }
        if definitions
            .tables()
            .iter()
            .any(|t| t.sheet == sheet && meets(t.area, *rect))
        {
            return Err("Cannot merge cells of a table".into());
        }
    }
    let mut merges = host.merged_areas(sheet);
    merges.retain(|m| !rects.iter().any(|r| meets(*m, *r)));
    let mut edits = Vec::new();
    for rect in &rects {
        let (t, l, b, r) = (rect.first().row(), rect.first().column(), rect.last().row(), rect.last().column());
        if r > l {
            edits.push(CellEdit::Clear { area: area(sheet, t, l + 1, t, r) });
        }
        if b > t {
            edits.push(CellEdit::Clear { area: area(sheet, t + 1, l, b, r) });
        }
    }
    merges.extend(rects);
    edits.push(finish(sheet, merges));
    Ok(edits)
}

/// Unmerge every merged area that meets an area.
pub(crate) fn prepare_unmerge(host: &HostState, areas: &[GridArea]) -> Result<Vec<CellEdit>, String> {
    let sheet = one_sheet(host, areas)?;
    let mut merges = host.merged_areas(sheet);
    let before = merges.len();
    merges.retain(|m| !areas.iter().any(|a| meets(*m, *a)));
    if merges.len() == before {
        return Ok(Vec::new());
    }
    Ok(vec![finish(sheet, merges)])
}
