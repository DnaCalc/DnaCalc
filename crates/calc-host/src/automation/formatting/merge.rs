//! `Range.Merge`, `UnMerge`, `MergeCells` and `MergeArea` over the authored merge set (dc-oc78o).
use super::*;
use calc_value::GridArea;

const OWNER: &str = "dc-oxh8w.39";

fn cells(area: &GridArea) -> u64 {
    u64::from(area.last().row() - area.first().row() + 1)
        * u64::from(area.last().column() - area.first().column() + 1)
}

fn covered(merge: &GridArea, area: &GridArea) -> u64 {
    let top = merge.first().row().max(area.first().row());
    let bottom = merge.last().row().min(area.last().row());
    let left = merge.first().column().max(area.first().column());
    let right = merge.last().column().min(area.last().column());
    if top > bottom || left > right {
        return 0;
    }
    u64::from(bottom - top + 1) * u64::from(right - left + 1)
}

/// A Boolean argument: a Boolean, or a number (non-zero is true); anything else is error 1004.
fn flag(value: &AutoValue) -> Result<bool, AutoError> {
    match value {
        AutoValue::Bool(b) => Ok(*b),
        AutoValue::I2(n) => Ok(*n != 0),
        AutoValue::I4(n) => Ok(*n != 0),
        AutoValue::R8(n) => Ok(*n != 0.0),
        _ => Err(format_error()),
    }
}

impl WorkbookAutomation {
    pub(super) fn register_range_merge(handlers: &mut Handlers<Self>) {
        handlers.on("Range", "Merge", InvokeKind::Method, Self::range_merge);
        handlers.on("Range", "UnMerge", InvokeKind::Method, Self::range_unmerge);
        handlers.on("Range", "MergeCells", InvokeKind::Get, Self::range_merge_cells);
        handlers.on("Range", "MergeCells", InvokeKind::Put, Self::range_put_merge_cells);
        handlers.on("Range", "MergeArea", InvokeKind::Get, Self::range_merge_area);
    }

    fn merge_through(
        &self,
        target: &ObjectRef,
        call: &Call<'_>,
        merge: Option<bool>,
    ) -> Reply {
        self.mutate_areas_with_owner(target, call, OWNER, |session, range| {
            let areas = range.areas().to_vec();
            let command = match merge {
                Some(across) => WorkbookCommand::MergeCells { areas, across },
                None => WorkbookCommand::UnmergeCells { areas },
            };
            // Worksheet-state refusals are Excel's 1004, as in the Format Cells route.
            if let Err(reason) = match &command {
                WorkbookCommand::MergeCells { areas, across } => {
                    crate::merge::prepare_merge(session.host(), areas, *across)
                }
                WorkbookCommand::UnmergeCells { areas } => {
                    crate::merge::prepare_unmerge(session.host(), areas)
                }
                _ => unreachable!("a merge command"),
            } {
                return Err(AutoError::excel(1004, reason));
            }
            Ok(SkinIntent::Command(command))
        })
    }

    fn range_merge(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        if call.args.len() > 1 {
            return Err(format_error());
        }
        let across = match call.arg(0) {
            AutoValue::Missing | AutoValue::Empty => false,
            value => flag(value)?,
        };
        self.merge_through(target, call, Some(across))
    }

    fn range_unmerge(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        self.merge_through(target, call, None)
    }

    fn range_put_merge_cells(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 1)?;
        let merge = flag(call.arg(0))?;
        self.merge_through(target, call, merge.then_some(false))
    }

    fn merged_sets(&self, target: &ObjectRef, call: &Call<'_>) -> Result<(Vec<GridArea>, Vec<GridArea>), AutoError> {
        let mut out = None;
        self.with_range(target, |access, _, range, _| {
            let Access::Session(session, _) = access else {
                return Err(AutoError::not_modelled(
                    format!("{} inside a worksheet invocation", call.label()),
                    OWNER,
                ));
            };
            let areas = range.areas().to_vec();
            let sheet = areas.first().ok_or_else(format_error)?.sheet();
            out = Some((areas, session.host().merged_areas(sheet)));
            Ok(AutoValue::Empty)
        })?;
        out.ok_or_else(format_error)
    }

    fn range_merge_cells(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let (areas, merges) = self.merged_sets(target, call)?;
        let total: u64 = areas.iter().map(cells).sum();
        let merged: u64 = areas
            .iter()
            .map(|a| merges.iter().map(|m| covered(m, a)).sum::<u64>())
            .sum();
        Ok(match merged {
            0 => AutoValue::Bool(false),
            n if n == total => AutoValue::Bool(true),
            _ => AutoValue::Null,
        })
    }

    fn range_merge_area(&self, target: &ObjectRef, call: &Call<'_>) -> Reply {
        format_arity(call, 0)?;
        let (areas, merges) = self.merged_sets(target, call)?;
        let [only] = &areas[..] else {
            return Err(AutoError::not_modelled("MergeArea of a multi-area range", "dc-oxh8w.39"));
        };
        if only.first() != only.last() {
            return Err(AutoError::not_modelled("MergeArea of a multi-cell range", "dc-oxh8w.39"));
        }
        let at = *only;
        let found = merges.iter().find(|m| covered(m, &at) == 1).copied().unwrap_or(at);
        self.with_range(target, |access, _, _, _| {
            self.acquire(access, &ReferenceLike::area(found), RangeAxis::Cells)
        })
    }
}
