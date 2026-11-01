//! Immutable formula groups with plain grid coordinates and explicit membership.

use crate::{FormulaForm, SemanticTemplateKey, TemplatePlacement};
use calc_value::{CellKey, ExcelText, GridArea, SheetId};
use std::ops::RangeInclusive;
use std::sync::Arc;

/// A formula group within one sheet/revision, distinct from the notebook's `BlockId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FormulaBlockId {
    pub sheet: SheetId,
    pub local: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormulaSaveMode {
    SharedGroup,
    Individual,
}

/// Row-major offsets within the extent. Runs are inclusive, disjoint and sorted.
/// Even a full-sheet extent with two members allocates only two runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormulaMembership {
    Dense,
    Runs(Arc<[RangeInclusive<u64>]>),
}

/// The codec's validated source and existing reference-occurrence renderer. It owns no result,
/// bound plan, document or evaluator. A renderer must accept every admitted block member.
pub trait FormulaTemplateSource: std::fmt::Debug + Send + Sync {
    fn anchor(&self) -> CellKey;
    fn source(&self) -> &ExcelText;
    fn render(&self, at: CellKey) -> ExcelText;
}

#[derive(Debug)]
pub struct FormulaBlock {
    id: FormulaBlockId,
    extent: GridArea,
    membership: FormulaMembership,
    member_count: u64,
    key: Option<SemanticTemplateKey>,
    form: FormulaForm,
    source: Arc<dyn FormulaTemplateSource>,
}

impl FormulaBlock {
    /// Builds membership from actual cells, never from the bounding rectangle. A source with
    /// unsupported normalization may still be represented, but its `key` is absent.
    pub fn new(
        id: FormulaBlockId,
        extent: GridArea,
        members: impl IntoIterator<Item = CellKey>,
        key: Option<SemanticTemplateKey>,
        form: FormulaForm,
        source: Arc<dyn FormulaTemplateSource>,
    ) -> Option<Arc<Self>> {
        if id.sheet != extent.sheet()
            || !extent.contains(source.anchor())
            || source.source().utf16_code_units().is_empty()
            || key.as_ref().is_some_and(|key| {
                key.evaluation != form || key.placement != TemplatePlacement::Cell
            })
        {
            return None;
        }
        let mut offsets = Vec::new();
        for at in members {
            offsets.push(Self::offset(extent, at)?);
        }
        offsets.sort_unstable();
        offsets.dedup();
        if offsets
            .binary_search(&Self::offset(extent, source.anchor())?)
            .is_err()
        {
            return None;
        }
        let member_count = u64::try_from(offsets.len()).ok()?;
        let membership = if member_count == extent.cell_count() {
            FormulaMembership::Dense
        } else {
            let mut runs: Vec<RangeInclusive<u64>> = Vec::new();
            for offset in offsets {
                if let Some(last) = runs.last_mut()
                    && last.end().checked_add(1) == Some(offset)
                {
                    *last = *last.start()..=offset;
                } else {
                    runs.push(offset..=offset);
                }
            }
            FormulaMembership::Runs(runs.into())
        };
        Some(Arc::new(Self {
            id,
            extent,
            membership,
            member_count,
            key,
            form,
            source,
        }))
    }

    fn offset(extent: GridArea, at: CellKey) -> Option<u64> {
        if !extent.contains(at) {
            return None;
        }
        u64::from(at.row() - extent.first().row())
            .checked_mul(u64::from(extent.column_count()))?
            .checked_add(u64::from(at.column() - extent.first().column()))
    }
    pub fn contains(&self, at: CellKey) -> bool {
        let Some(offset) = Self::offset(self.extent, at) else {
            return false;
        };
        match &self.membership {
            FormulaMembership::Dense => true,
            FormulaMembership::Runs(runs) => runs
                .binary_search_by(|run| {
                    if offset < *run.start() {
                        std::cmp::Ordering::Greater
                    } else if offset > *run.end() {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .is_ok(),
        }
    }
    pub fn id(&self) -> FormulaBlockId {
        self.id
    }
    pub fn extent(&self) -> GridArea {
        self.extent
    }
    pub fn membership(&self) -> &FormulaMembership {
        &self.membership
    }
    pub fn member_count(&self) -> u64 {
        self.member_count
    }
    pub fn key(&self) -> Option<&SemanticTemplateKey> {
        self.key.as_ref()
    }
    pub fn form(&self) -> FormulaForm {
        self.form
    }
    pub fn save_mode(&self) -> FormulaSaveMode {
        match self.form {
            FormulaForm::Legacy => FormulaSaveMode::SharedGroup,
            FormulaForm::Dynamic => FormulaSaveMode::Individual,
        }
    }
    pub fn source(&self) -> &dyn FormulaTemplateSource {
        &*self.source
    }
}
