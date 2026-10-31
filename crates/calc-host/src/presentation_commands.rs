//! Authored notebook layout and AutoFit preflight shared by every front end.
//! Extracted from DNA Calc server preparation; notebook layout retains the DnaTreeCalc
//! bf6dfd7 `src/dnacalc-stage-notebook/src/{lib.rs,model.rs}` port recorded there.
use crate::{CellEdit, HostState, PlacementId, SkinIntent};
use calc_value::{CellKey, GridArea};
pub const AUTOFIT_CELLS: usize = 10_000;
pub fn can_edit(host: &HostState, at: CellKey) -> Result<(), String> {
    if host.model().sheet_by_id(at.sheet()).is_none() {
        return Err("The worksheet is unavailable".into());
    }
    if host
        .loaded()
        .sheet(at.sheet())
        .is_some_and(|s| s.protected() || s.is_macro_sheet())
    {
        return Err("Editing protected or macro worksheets is unavailable".into());
    }
    Ok(())
}

pub fn prepare_notebook_link(
    host: &HostState,
    target: GridArea,
    prose: bool,
    text: Option<String>,
) -> Result<SkinIntent, String> {
    use crate::{BlockId, BlockKind, PlacementId, SkinLayoutEdit, Spot, ViewKind};
    if text.is_some() {
        can_edit(host, target.first())?;
    }
    let layout = host.model().layout();
    if text.is_some()
        && (target.first() != target.last()
            || host
                .occupied_areas(target.sheet())
                .iter()
                .any(|area| area.contains(target.first())))
    {
        return Err(
            "Insert into one empty cell, or link existing cells without replacing their contents"
                .into(),
        );
    }
    let kind = if prose {
        BlockKind::Prose
    } else {
        BlockKind::Calculation
    };
    if prose
        && host
            .model()
            .cells(target)
            .into_iter()
            .flatten()
            .any(|(_, c)| matches!(c, crate::CellInput::Formula(_)))
    {
        return Err("Prose uses visible worksheet text; a formula is a calculation block".into());
    }
    if prose
        && host.occupied_areas(target.sheet()).iter().any(|a| {
            a.first() != a.last()
                && a.first().row() <= target.last().row()
                && a.last().row() >= target.first().row()
                && a.first().column() <= target.last().column()
                && a.last().column() >= target.first().column()
        })
    {
        return Err("Prose links use worksheet text, not array values".into());
    }
    let entry = text.map(|text| if prose { format!("'{text}") } else { text });
    let mut edits = Vec::new();
    let block = if let Some(block) = layout
        .blocks()
        .iter()
        .find(|b| b.anchor() == Some(target) && b.kind() == kind)
    {
        block.id()
    } else {
        let id = BlockId::new(
            layout
                .blocks()
                .iter()
                .map(|b| b.id().get())
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("Block identity space exhausted")?,
        )
        .ok_or("Block identity space exhausted")?;
        edits.push(CellEdit::Layout(SkinLayoutEdit::CreateBlock {
            block: id,
            kind,
            anchor: target,
        }));
        id
    };
    let id = PlacementId::new(
        ViewKind::ALL
            .iter()
            .flat_map(|v| layout.view(*v))
            .map(|p| p.id().get())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("Placement identity space exhausted")?,
    )
    .ok_or("Placement identity space exhausted")?;
    let position = std::num::NonZeroU32::new(
        u32::try_from(layout.view(ViewKind::Notebook).len() + 1)
            .map_err(|_| "Too many notebook links")?,
    )
    .ok_or("Notebook position is zero")?;
    edits.push(CellEdit::Layout(SkinLayoutEdit::AddPlacement {
        placement: id,
        block,
        spot: Spot::Notebook { position },
    }));
    if let Some(text) = entry {
        let layout = edits
            .into_iter()
            .filter_map(|e| {
                if let CellEdit::Layout(layout) = e {
                    Some(layout)
                } else {
                    None
                }
            })
            .collect();
        Ok(SkinIntent::EnterCellAndLayout {
            at: target.first(),
            text,
            layout,
        })
    } else {
        Ok(SkinIntent::Edit { edits })
    }
}

pub fn prepare_notebook_order(
    host: &HostState,
    id: PlacementId,
    direction: i8,
) -> Result<Option<SkinIntent>, String> {
    use crate::{SkinLayoutEdit, Spot, ViewKind};
    let layout = host.model().layout();
    let list = layout.view(ViewKind::Notebook);
    let index = list
        .iter()
        .position(|p| p.id() == id)
        .ok_or("Notebook link no longer exists")?;
    let mut edits = Vec::new();
    if direction == 0 {
        let block = list[index].block();
        edits.push(CellEdit::Layout(SkinLayoutEdit::RemovePlacement {
            placement: id,
        }));
        if ViewKind::ALL
            .iter()
            .flat_map(|v| layout.view(*v))
            .filter(|p| p.block() == block)
            .count()
            == 1
        {
            edits.push(CellEdit::Layout(SkinLayoutEdit::RemoveBlock { block }));
        }
    } else {
        let sheet = layout
            .block(list[index].block())
            .and_then(|b| b.anchor())
            .map(|a| a.sheet());
        let peer = if direction < 0 {
            (0..index).rev().find(|i| {
                layout
                    .block(list[*i].block())
                    .and_then(|b| b.anchor())
                    .map(|a| a.sheet())
                    == sheet
            })
        } else {
            (index + 1..list.len()).find(|i| {
                layout
                    .block(list[*i].block())
                    .and_then(|b| b.anchor())
                    .map(|a| a.sheet())
                    == sheet
            })
        };
        let Some(peer) = peer else {
            return Ok(None);
        };
        edits.push(CellEdit::Layout(SkinLayoutEdit::MovePlacement {
            placement: id,
            spot: Spot::Notebook {
                position: std::num::NonZeroU32::new(peer as u32 + 1).unwrap(),
            },
        }));
    }
    Ok(Some(SkinIntent::Edit { edits }))
}
pub fn autofit_cells(
    host: &HostState,
    target: GridArea,
    visible: impl Fn(CellKey) -> bool,
) -> Result<Vec<CellKey>, String> {
    if host.model().sheet_by_id(target.sheet()).is_none() {
        return Err("The sheet is unavailable".to_owned());
    }
    let sheet = host
        .loaded()
        .sheet(target.sheet())
        .ok_or("The sheet is unavailable")?;
    if sheet.protected() {
        return Err("Fitting a protected sheet is unavailable here".to_owned());
    }
    if host.merged_areas(target.sheet()).iter().any(|merge| {
        merge.first().row() <= target.last().row()
            && merge.last().row() >= target.first().row()
            && merge.first().column() <= target.last().column()
            && merge.last().column() >= target.first().column()
    }) {
        return Err(
            "AutoFit with merged cells is unavailable; choose an unmerged range".to_owned(),
        );
    }
    let mut keys = std::collections::BTreeSet::new();
    for occupied in host.occupied_areas(target.sheet()) {
        let top = target.first().row().max(occupied.first().row());
        let left = target.first().column().max(occupied.first().column());
        let bottom = target.last().row().min(occupied.last().row());
        let right = target.last().column().min(occupied.last().column());
        if top > bottom || left > right {
            continue;
        }
        if u64::from(bottom - top + 1) * u64::from(right - left + 1) > AUTOFIT_CELLS as u64 {
            return Err("Fit at most 10000 occupied cells at once".to_owned());
        }
        for row in top..=bottom {
            for column in left..=right {
                if !visible(CellKey::new(target.sheet(), row, column).unwrap()) {
                    continue;
                }
                keys.insert(CellKey::new(target.sheet(), row, column).unwrap());
                if keys.len() > AUTOFIT_CELLS {
                    return Err("Fit at most 10000 occupied cells at once".to_owned());
                }
            }
        }
    }
    Ok(keys.into_iter().collect())
}
