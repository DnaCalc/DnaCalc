//! Commands: translating a cell-editing command into one authored batch, preparing the
//! document's and the engine's share of it against one base revision, and committing both as one
//! transaction (plan 3.3).

use crate::state::HostState;
use calc_doc::EditError;
use calc_doc_model::value::{CellKey, GridArea};
use calc_doc_model::{
    ArrayFormula, AuthoredRevision, AxisEdit, CellInput, FormulaText, ModelRoot, OpBatch,
    WorkbookOp,
};
use calc_engine::{CalcTick, FormulaNode, InputSeed, NodeChange, PrepareError};
use std::fmt;
use std::sync::Arc;

/// One edit of a cell-editing command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellEdit {
    /// Authored style table, in the same transaction as its cell assignments.
    Tables(Vec<calc_doc_model::Table>),
    Styles(Arc<calc_doc_model::StyleTable>),
    PageSetup {sheet:calc_value::SheetId,setup:calc_doc_model::PageSetup},
    Charts {sheet:calc_value::SheetId,charts:Vec<calc_doc_model::Chart>},
    AutoFilter { sheet: calc_doc_model::value::SheetId, filter: Option<calc_doc_model::AutoFilter> },
    /// Document layout metadata, independent of cell contents.
    Layout(calc_doc_model::SkinLayoutEdit),
    Validations { sheet: calc_value::SheetId, rules: Vec<calc_doc_model::Validation> },
    ReviewLinks { sheet:calc_value::SheetId, links:Vec<calc_doc_model::ReviewLink> },
    ReviewNotes {
        sheet: calc_doc_model::value::SheetId,
        notes: Vec<calc_doc_model::ReviewNote>,
    },
    /// Replaces authored conditional rules, retaining original XML fields.
    ConditionalRules {
        sheet: calc_doc_model::value::SheetId,
        rules: Vec<calc_doc_model::ConditionalRuleSource>,
    },
    /// A worksheet lifecycle operation, with reference rewriting and snapshot history.
    Sheet(calc_doc_model::SheetEdit),
    /// Changes a physical row height, column width or hidden flag.
    AxisFormat(calc_doc_model::AxisFormatEdit),
    /// Changes a worksheet window's frozen leading bands without changing cells.
    PaneLayout { sheet: calc_value::SheetId, layout: Option<calc_doc_model::PaneLayout> },
    FrozenPanes {
        sheet: calc_doc_model::value::SheetId,
        panes: Option<calc_doc_model::FrozenPanes>,
    },
    /// Sets a cell of an existing sheet to a value or a formula.
    Set { at: CellKey, input: CellInput },
    /// Removes the contents of every cell of an area of an existing sheet.
    Clear { area: GridArea },
    /// Authors an array formula: the rest of the array's area is emptied and its anchor holds
    /// the formula ([`calc_doc_model::WorkbookOp::SetArray`]). A dynamic array over its anchor
    /// alone is what Excel stores for a formula entered into one cell that can return an array;
    /// a legacy array is a Ctrl+Shift+Enter formula over its area.
    SetArray {
        array: ArrayFormula,
        formula: FormulaText,
    },
    /// Inserts or deletes rows or columns of a sheet ([`calc_doc::prepare_structural_edit`]):
    /// the cells and what moves with them shift, and every formula's and defined name's
    /// references adjust as Excel's do. A command holding one holds only axis edits.
    Axis(AxisEdit),
    /// Gives a cell a cell format: an index into the workbook's cell formats (a cell's `s`), or
    /// `None` to return it to the format the file gave it ([`calc_doc_model::WorkbookOp::SetFormat`],
    /// dc-09v). It changes no input and no value.
    SetFormat { at: CellKey, format: Option<u32> },
    /// Gives the defined name `name` of `scope` the formula `formula`
    /// ([`calc_doc_model::WorkbookOp::SetNameFormula`], dc-gf0). Scoped graph watches rebind
    /// its users without changing the authored formulas or their generations.
    SetName {
        name: String,
        scope: calc_doc_model::NameScope,
        /// `None` removes the name; a name the workbook does not hold is defined.
        formula: Option<String>,
    },
    /// The complete scoped records retained by name undo.
    RestoreNames(Vec<calc_doc_model::NameRestoration>),
    /// Sets the workbook's iterative-calculation settings
    /// ([`calc_doc_model::WorkbookOp::SetIteration`], dc-y2o.2): Excel's "Enable iterative
    /// calculation", "Maximum Iterations" and "Maximum Change", which the workbook saves in its
    /// `calcPr`. The formulas on a circular reference are calculated again under them.
    SetIteration(calc_doc_model::IterationSettings),
}

impl CellEdit {
    fn operation(self) -> WorkbookOp {
        match self {
            CellEdit::Tables(tables)=>WorkbookOp::SetTables(tables),
            CellEdit::PageSetup {sheet,setup}=>WorkbookOp::SetPageSetup {sheet,setup},
            CellEdit::Charts {sheet,charts}=>WorkbookOp::SetCharts {sheet,charts},
            CellEdit::Styles(styles) => WorkbookOp::SetStyles(styles),
            CellEdit::AutoFilter { sheet, filter } => WorkbookOp::SetAutoFilter { sheet, filter },
            CellEdit::Layout(edit) => WorkbookOp::EditSkinLayout(edit),
            CellEdit::Validations { sheet, rules } => WorkbookOp::SetValidations { sheet, rules },
            CellEdit::ReviewLinks { sheet, links } => WorkbookOp::SetReviewLinks { sheet, links },
            CellEdit::ReviewNotes { sheet, notes } => WorkbookOp::SetReviewNotes { sheet, notes },
            CellEdit::ConditionalRules { sheet, rules } => {
                WorkbookOp::SetConditionalRules { sheet, rules }
            }
            CellEdit::Sheet(edit) => WorkbookOp::EditSheet(edit),
            CellEdit::AxisFormat(edit) => WorkbookOp::SetAxisFormat(edit),
            CellEdit::PaneLayout {sheet,layout}=>WorkbookOp::SetPaneLayout {sheet,layout},
            CellEdit::FrozenPanes { sheet, panes } => WorkbookOp::SetFrozenPanes { sheet, panes },
            CellEdit::Set { at, input } => WorkbookOp::SetCell { at, input },
            CellEdit::Clear { area } => WorkbookOp::Clear { area },
            CellEdit::SetArray { array, formula } => WorkbookOp::SetArray {
                array,
                input: CellInput::Formula(formula),
            },
            CellEdit::Axis(edit) => WorkbookOp::EditAxis(edit),
            CellEdit::SetFormat { at, format } => WorkbookOp::SetFormat { at, format },
            CellEdit::SetName {
                name,
                scope,
                formula,
            } => WorkbookOp::SetNameFormula {
                name,
                scope,
                formula,
            },
            CellEdit::RestoreNames(names) => WorkbookOp::RestoreNames(names),
            CellEdit::SetIteration(settings) => WorkbookOp::SetIteration(settings),
        }
    }
}

/// A cell-editing command: its edits, applied in order as one transaction, and the authored
/// revision the caller prepared them against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostCommand {
    pub base: AuthoredRevision,
    pub edits: Vec<CellEdit>,
}

/// What a committed command published: the one authored revision it made and its dirty set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandReceipt {
    base: AuthoredRevision,
    revision: AuthoredRevision,
    changed_cells: Vec<CellKey>,
    cleared_areas: Vec<GridArea>,
    input_writes: Vec<GridArea>,
    pending: Vec<CellKey>,
    superseded_recalc: Option<CalcTick>,
}

impl CommandReceipt {
    /// Explicit authored cell writes, including same-value rewrites and clears of blanks.
    /// Formats, calculation results and workbook settings are not worksheet Change targets.
    pub fn input_writes(&self) -> &[GridArea] {
        &self.input_writes
    }
    /// The revision the command was based on.
    pub fn base(&self) -> AuthoredRevision {
        self.base
    }

    /// The authored revision the command made, which the engine state is also at.
    pub fn revision(&self) -> AuthoredRevision {
        self.revision
    }

    /// The cells whose input a set command changed, in the order of the cells (sheet ID, then row,
    /// then column). A cell set to the input it already held is not among them, and the cells a
    /// clear emptied are in [`CommandReceipt::cleared_areas`], not here.
    pub fn changed_cells(&self) -> &[CellKey] {
        &self.changed_cells
    }

    /// The cleared areas that emptied at least one cell holding an input, in command order.
    pub fn cleared_areas(&self) -> &[GridArea] {
        &self.cleared_areas
    }

    /// The formula cells whose results the next recalculation calculates from (the anchors of
    /// the engine's pending nodes after the commit), in the order of the cells: each formula the
    /// command added or replaced, each formula reading a changed cell, and each formula already
    /// pending before it. The formulas reading these are stale too; [`crate::project`] labels
    /// every stale result.
    pub fn pending(&self) -> &[CellKey] {
        &self.pending
    }

    /// The predecessor calculation. Its pure candidates are checked by the successor; only
    /// candidates whose read proofs still hold may be reused.
    pub fn superseded_recalc(&self) -> Option<CalcTick> {
        self.superseded_recalc
    }
}

/// Why [`dispatch`] committed nothing. The authored revision, the engine state and every cell are
/// as they were before the command.
#[derive(Debug, PartialEq, Eq)]
pub enum CommandError {
    /// The command holds no edit, so it would make no change to commit.
    NoEdits,
    /// The command was prepared against another revision than the current one.
    StaleBase {
        command: AuthoredRevision,
        current: AuthoredRevision,
    },
    /// The document refused the batch (calc-doc's [`calc_doc::prepare_edit`]): a sheet the
    /// workbook does not hold, or a formula a save could not write.
    Document(EditError),
    /// The layout cannot be saved without replacing unread metadata or a user name.
    LayoutSave(calc_doc::SaveRefusal),
    /// The engine refused the batch after the document prepared it.
    Engine(PrepareError),
    /// The batch adds formula cells and would leave `formula_cells` of them, more than the host's
    /// [`crate::HostOptions::formula_cell_budget`]; the node IDs the engine reserved for them are
    /// released.
    FormulaBudget { formula_cells: usize, budget: usize },
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::NoEdits => f.write_str("the command holds no edit"),
            CommandError::StaleBase { command, current } => write!(
                f,
                "the command was prepared against revision {}, the workbook is at {}",
                command.get(),
                current.get()
            ),
            CommandError::LayoutSave(error) => write!(f, "the layout cannot be saved: {error}"),
            CommandError::Document(error) => write!(f, "the document refused the edit: {error}"),
            CommandError::Engine(error) => write!(f, "the engine refused the edit: {error}"),
            CommandError::FormulaBudget {
                formula_cells,
                budget,
            } => write!(
                f,
                "the edit would leave {formula_cells} formula cells, more than the budget of {budget}"
            ),
        }
    }
}

impl std::error::Error for CommandError {}

/// Commits `command` to the workbook as one transaction, or refuses it with nothing changed.
///
/// 1. A command with no edit is refused, and so is a command based on another revision than the
///    current one.
/// 2. Its edits become one batch, in order, based on the current revision.
/// 3. The document's share is prepared ([`calc_doc::prepare_edit`]): the next revision, made
///    beside the current one, which stays as it is.
/// 4. The engine's share is prepared against the same base ([`calc_engine::prepare_edit`]): its
///    node changes and input seeds, with the IDs of the nodes it adds reserved.
/// 5. The reserved nodes are admitted against the host's formula-cell budget, and the receipt's
///    dirty set is reserved.
/// 6. Both are committed: the engine's change ([`calc_engine::commit_edit`], which returns no
///    error) and the adoption of the prepared revision. From here on nothing returns an error or
///    grows a host buffer past what step 5 reserved. A recalculation in progress, which was
///    calculating the revision before, is cancelled. When the batch changed a cell's input or
///    formula (the engine staged a seed or a node change), the host records that the inputs
///    changed since the workbook was opened, which [`crate::project`] reads for file caches.
///
/// A refusal at steps 1 to 5 drops whatever was prepared; dropping the engine's change releases
/// its reserved node IDs.
pub fn dispatch(
    host: &mut HostState,
    command: HostCommand,
) -> Result<CommandReceipt, CommandError> {
    if command.edits.is_empty() {
        return Err(CommandError::NoEdits);
    }
    let current = host.model.revision();
    if command.base != current {
        return Err(CommandError::StaleBase {
            command: command.base,
            current,
        });
    }
    let batch = OpBatch {
        base: command.base,
        operations: command.edits.into_iter().map(CellEdit::operation).collect(),
    };

    let transformed_scope = if batch
        .operations
        .iter()
        .any(|op| matches!(op, WorkbookOp::EditAxis(_) | WorkbookOp::EditSheet(_)))
    {
        active_scope(host).map(|mut scope| {
            for op in &batch.operations {
                match op {
                    WorkbookOp::EditAxis(edit) => scope = axis_scope(scope, *edit),
                    WorkbookOp::EditSheet(calc_doc_model::SheetEdit::Remove { sheet }) => {
                        scope =
                            scope.transformed(&calc_fml::reference::StructuralEdit::RemoveSheet {
                                sheet: *sheet,
                                order: Vec::new(),
                            })
                    }
                    _ => {}
                }
            }
            scope
        })
    } else {
        None
    };

    host.loaded
        .check_layout_save(&batch.operations)
        .map_err(CommandError::LayoutSave)?;
    let axis_edits: Vec<AxisEdit> = batch
        .operations
        .iter()
        .filter_map(|op| match op {
            WorkbookOp::EditAxis(edit) => Some(*edit),
            _ => None,
        })
        .collect();
    let mut outline_layouts=host.model.definitions().axis_layouts().clone();
    let mut filter_areas=host.model.definitions().auto_filters().map(|(s,f)|(s,f.area)).collect::<std::collections::BTreeMap<_,_>>();
    for edit in &axis_edits {
        if crate::review::has_sorted_imported_notes(host,edit.sheet()) {
            return Err(CommandError::Document(EditError::SheetOperation("Row or column edits with sorted notes are not modelled (dc-971p.38); save and reopen first; nothing changed".into())));
        }
    }
    for edit in &axis_edits {
        if host.loaded.sheet(edit.sheet()).is_some_and(|s|s.geometry().outline_unsupported) {return Err(CommandError::Document(EditError::SheetOperation("Structural edits of imported outline metadata are not modelled (dc-971p.39); nothing changed".into())));}
        let next_outlines=outline_layouts.moved(*edit);
        let next_groups=next_outlines.outline_groups(edit.sheet(),edit.axis());
        if next_groups.iter().any(|g|g.summary<1 || g.summary>edit.axis().size()) {return Err(CommandError::Document(EditError::SheetOperation("This edit moves an outline summary outside the worksheet (dc-971p.39); nothing changed".into())));}
        for group in outline_layouts.outline_groups(edit.sheet(),edit.axis()).into_iter().filter(|g|g.collapsed) {
            if let Some((first,mut last))=calc_doc_model::moved_run(*edit,group.first,group.last) {
                if edit.change()==calc_doc_model::AxisChange::Insert && edit.first()==group.last+1 {last=last.saturating_add(edit.count()).min(edit.axis().size());}
                if !next_groups.iter().any(|g|g.first==first && g.last==last && g.level==group.level && g.collapsed) {return Err(CommandError::Document(EditError::SheetOperation("This edit merges or clips collapsed outline groups (dc-971p.39); expand them first; nothing changed".into())));}
            }
        }
        outline_layouts=next_outlines;
        if edit.axis()==calc_doc_model::Axis::Rows && host.model.definitions().auto_filter(edit.sheet()).is_some_and(|f|f.unsupported && host.model.definitions().axis_layouts().bands(edit.sheet(),calc_doc_model::Axis::Rows).any(|(r,v)|r>f.area.first().row() && r<=f.area.last().row() && v.hidden())) {
            return Err(CommandError::Document(EditError::SheetOperation("Row edits with independent manual hiding in an imported filter are not modelled (dc-971p.37); nothing changed".into())));
        }
        if let Some(area)=filter_areas.get(&edit.sheet()).copied() {
            if edit.axis()==calc_doc_model::Axis::Rows && edit.change()==calc_doc_model::AxisChange::Delete && edit.first()<=area.first().row() && edit.last()>=area.first().row() {
                return Err(CommandError::Document(EditError::SheetOperation("Remove AutoFilter before deleting its header (dc-971p.37); nothing changed".into())));
            }
            let next=calc_doc_model::moved_area(*edit,area).ok_or_else(||CommandError::Document(EditError::SheetOperation("Remove AutoFilter before deleting or moving its entire range outside the grid (dc-971p.37); nothing changed".into())))?;
            filter_areas.insert(edit.sheet(),next);
        }
    }
    for edit in &axis_edits {
        if host
            .model
            .definitions()
            .review_notes(edit.sheet())
            .iter()
            .any(|n| {
                n.source.as_ref().is_some_and(|source| {
                    host.loaded
                        .sheets()
                        .iter()
                        .filter(|s| {
                            s.review().notes.iter().any(|n| {
                                n.source
                                    .as_ref()
                                    .is_some_and(|other| other.part() == source.part())
                            })
                        })
                        .count()
                        > 1
                })
            })
        {
            return Err(CommandError::Document(EditError::SheetOperation("Structural edits with shared review parts are unavailable (dc-971p.40); nothing changed".into())));
        }
    }
    for edit in &axis_edits {
        if let Some(view) = host.sheet_view(edit.sheet())
            && let Some(p) = view.frozen
        {
            let origin = if edit.axis() == calc_doc_model::Axis::Rows {
                view.top_left.unwrap_or((1, 1)).0
            } else {
                view.top_left.unwrap_or((1, 1)).1
            };
            let count = if edit.axis() == calc_doc_model::Axis::Rows {
                p.rows()
            } else {
                p.columns()
            };
            if origin > 1
                && count > 0
                && edit.first() <= origin.saturating_add(count).saturating_sub(1)
            {
                return Err(CommandError::Document(EditError::SheetOperation("Unfreeze this imported view before inserting or deleting these bands (dc-971p.16)".into())));
            }
        }
    }
    if batch.operations.iter().filter_map(|op|if let WorkbookOp::EditAxis(e)=op{Some(*e)}else{None}).any(|edit|host.model.definitions().tables().iter().any(|t|!crate::tables::axis_compatible(host,t,edit))){return Err(CommandError::Document(EditError::SheetOperation("Axis edits with unsupported imported table metadata are not modelled (dc-971p.41); nothing changed".into())));}
    for op in &batch.operations {if let WorkbookOp::SetAxisFormat(edit)=op {if edit.axis()==calc_doc_model::Axis::Rows&&host.model.definitions().tables().iter().any(|t|t.sheet==edit.sheet()&&t.filter.as_ref().is_some_and(|f|f.unsupported&&edit.index()>f.area.first().row()&&edit.index()<=f.area.last().row())){return Err(CommandError::Document(EditError::SheetOperation("Row visibility with this imported table filter is not modelled (dc-971p.41); nothing changed".into())));}}}
    crate::tables::validate_cells(host,&batch).map_err(|s|CommandError::Document(EditError::SheetOperation(s)))?;
    for op in &batch.operations {
        if let WorkbookOp::SetTables(tables)=op {crate::tables::validate(host,tables).map_err(|s|CommandError::Document(EditError::SheetOperation(s)))?;}
        if let WorkbookOp::SetValidations { sheet, rules } = op {
            let invalid = host.model.sheet_by_id(*sheet).is_none()
                || host.loaded.sheet(*sheet).is_some_and(|s| s.protected() || s.is_macro_sheet())
                || rules.len() > 4096
                || rules.iter().any(|rule| {
                    rule.areas.is_empty() || rule.areas.iter().any(|&(t,l,b,r)| {
                        let (Ok(first),Ok(last)) = (CellKey::new(*sheet,t,l),CellKey::new(*sheet,b,r)) else {return true;};
                        calc_value::GridArea::new(first,last).is_err() || t>b || l>r
                    }) || rule.source.as_ref().is_some_and(|source| {
                        // Range/formula transforms are authored, while source-backed alert
                        // attributes retain their imported contract until an editor owns them.
                        !host.loaded.sheets().iter().flat_map(|s|s.validations()).any(|old| {
                            if old.source.as_ref()!=Some(source) || old.formula1.is_some()!=rule.formula1.is_some() || old.formula2.is_some()!=rule.formula2.is_some() {return false;}
                            let mut expected=old.clone();expected.authored_copy=rule.authored_copy;expected.areas=rule.areas.clone();
                            expected.formula1=rule.formula1.clone();expected.formula2=rule.formula2.clone();
                            &expected==rule
                        })
                    })
                });
            if invalid {return Err(CommandError::Document(EditError::SheetOperation("Validation edits need valid ranges on an editable sheet and must retain imported alert attributes (dc-971p.40)".into())));}
        }
    }
    for op in &batch.operations {
        if let WorkbookOp::SetPageSetup {sheet,setup}=op {
            if host.model.sheet_by_id(*sheet).is_none() || host.loaded.sheet(*sheet).is_some_and(|s|s.protected()||s.is_macro_sheet()) {return Err(CommandError::Document(EditError::SheetOperation("Page setup needs an editable worksheet (dc-971p.56)".into())));}
            host.loaded.check_page_setup(*sheet,&host.model.definitions().page_setup(*sheet),setup).map_err(|e|CommandError::Document(EditError::SheetOperation(e.to_string())))?;
        }
        if let WorkbookOp::SetCharts {sheet,charts}=op {
            if host.loaded.sheet(*sheet).is_none_or(|s|s.protected()||s.is_macro_sheet()) {
                return Err(CommandError::Document(EditError::SheetOperation("Chart editing requires an editable worksheet (dc-971p.49)".into())));
            }
            let mut parts=std::collections::BTreeSet::new();
            for chart in charts.iter().chain(host.model.definitions().all_charts().filter(|(s,_)|s!=sheet).flat_map(|(_,cs)|cs)) {
                if !parts.insert(chart.part.as_str()){return Err(CommandError::Document(EditError::SheetOperation("Chart frames need distinct part identities (dc-971p.49)".into())));}
            }
            host.loaded.check_chart_set(*sheet,charts).map_err(|e|CommandError::Document(EditError::SheetOperation(e.to_string())))?;
        }
        if let WorkbookOp::SetReviewLinks{sheet,links}=op {
            if host.loaded.sheet(*sheet).is_some_and(|s|s.protected()||s.is_macro_sheet()){
                return Err(CommandError::Document(EditError::SheetOperation("Hyperlinks require an editable worksheet (dc-971p.38)".into())));
            }
            host.loaded.check_sorted_links(*sheet,links).map_err(|e|CommandError::Document(EditError::SheetOperation(e.to_string())))?;
        }
    }
    if batch.operations.iter().any(|op|matches!(op,WorkbookOp::SetReviewNotes{..})) {
        crate::review::validate_batch(host,&batch).map_err(|message|CommandError::Document(EditError::SheetOperation(message)))?;
    }
    for op in &batch.operations {
        if let WorkbookOp::SetConditionalRules { sheet, rules } = op {
            let invalid = host
                .loaded
                .sheet(*sheet)
                .is_some_and(|s| s.conditional_formats().iter().any(|b| b.unread_container))
                || host
                    .loaded
                    .sheet(*sheet)
                    .is_some_and(|s| s.protected() || s.is_macro_sheet())
                || rules.len() > 1024
                || rules.iter().any(|r| {
                    r.areas.is_empty()
                        || r.areas.iter().any(|a| a.sheet() != *sheet)
                        || r.priority < 1
                        || calc_doc::conditional_rule(r).is_err()
                });
            if invalid {
                return Err(CommandError::Document(EditError::SheetOperation("Conditional rules require an editable worksheet, valid source, ranges and priorities (dc-971p.8)".into())));
            }
        }
    }
    if batch.operations.iter().any(|op|matches!(op,WorkbookOp::EditAxis(edit) if host.model.definitions().all_conditional_rules().flat_map(|(_,r)|r).any(|r|r.origin().is_some_and(|at|calc_doc_model::moved_cell(*edit,at).is_none())))) {
        return Err(CommandError::Document(EditError::SheetOperation("Deleting a conditional rule's formula origin is unavailable (dc-971p.8); edit its range first".into())));
    }
    if batch
        .operations
        .iter()
        .any(|op| matches!(op, WorkbookOp::EditAxis(_)))
        && (host
            .loaded
            .sheets()
            .iter()
            .any(|s| s.conditional_formats().iter().any(|b| b.unread_container))
            || host
                .model
                .definitions()
                .all_conditional_rules()
                .flat_map(|(_, r)| r)
                .any(|source| {
                    calc_doc::conditional_rule(source).is_ok_and(|r| {
                        r.extension_id.is_some()
                    })
                }))
    {
        return Err(CommandError::Document(EditError::SheetOperation("Axis edits with extension-backed conditional rules are unavailable (dc-971p.8); nothing changed".into())));
    }
    let sheet_edit = batch.operations.iter().find_map(|op| match op {
        WorkbookOp::EditSheet(edit) => Some(edit),
        _ => None,
    });
    let document = if let Some(edit) = sheet_edit {
        if batch.operations.len() != 1 {
            return Err(CommandError::Document(EditError::SheetOperation("Submit one worksheet operation per command".to_owned())));
        }
        if host.loaded.structure_protected() {
            return Err(CommandError::Document(EditError::SheetOperation("Workbook structure is protected".to_owned())));
        }
        if let calc_doc_model::SheetEdit::Rename { sheet, .. } | calc_doc_model::SheetEdit::Remove { sheet } = edit {
            if host.model.definitions().all_conditional_rules().any(|(_,r)|!r.is_empty())
                || host.loaded.defined_names_not_loaded()!=0
                || host.model.definitions().unloaded_cells().next().is_some()
                || !host.loaded.sheet_reference_metadata_supported()
                || host.model.layout().blocks().iter().any(|b| b.anchor().is_some_and(|a| a.sheet() == *sheet))
                || !host.model.definitions().tables().is_empty()
                || host.model.definitions().all_charts().any(|(_,charts)|!charts.is_empty())
                || host.loaded.sheets().iter().any(|s| !host.loaded.cell_shift_metadata_supported(s.id())
                    || !s.conditional_formats().is_empty()
                    || !s.charts().charts.is_empty() || s.charts().unread != 0 || s.is_macro_sheet())
                || (matches!(edit, calc_doc_model::SheetEdit::Remove { .. }) && host.model.definitions().macro_enabled())
            {
                return Err(CommandError::Document(EditError::SheetOperation("Renaming or deleting sheets with layout, table, rule, chart or other package references is not modelled (dc-971p.7); nothing changed".to_owned())));
            }
        }
        let identity = if let calc_doc_model::SheetEdit::Insert { key, .. } = edit {
            Some(*host.sheet_identities.entry(*key).or_insert_with(|| host.sheet_ids.issue().expect("sheet identity space")))
        } else { None };
        calc_doc::prepare_sheet_edit(&host.model, &batch, &calc_engine::ReferenceRewriter,
            &mut || identity.expect("only an insertion issues a sheet"))
    } else if axis_edits.is_empty() {
        calc_doc::prepare_edit(&host.model, &batch)
    } else {
        calc_doc::prepare_structural_edit(&host.model, &batch, &calc_engine::ReferenceRewriter)
    }
    .map_err(CommandError::Document)?;

    let formula_cells_before = host.engine.node_count();
    let pending_before = host.engine.pending().count();
    let engine = calc_engine::prepare_edit(&mut host.engine, &host.model, &batch)
        .map_err(CommandError::Engine)?;

    let tables_changed=batch.operations.iter().any(|op|match op{WorkbookOp::SetTables(next)=>{let old=host.model.definitions().tables();old.len()!=next.len()||old.iter().zip(next).any(|(a,b)|a.name!=b.name||a.display_name!=b.display_name||a.sheet!=b.sheet||a.area!=b.area||a.header_rows!=b.header_rows||a.totals_rows!=b.totals_rows||a.columns!=b.columns||a.filter!=b.filter)},_=>false});
    let facts_changed = batch
        .operations
        .iter()
        .any(|op| matches!(op, WorkbookOp::SetAxisFormat(_) | WorkbookOp::SetAutoFilter { .. }));
    let changes_inputs =
        tables_changed || facts_changed || !engine.seeds().is_empty() || !engine.node_changes().is_empty();
    let (mut added, mut replaced, mut removed) = (0usize, 0usize, 0usize);
    for change in engine.node_changes() {
        match change {
            NodeChange::Added { .. } => added += 1,
            NodeChange::Replaced { .. } => replaced += 1,
            NodeChange::Removed { .. } => removed += 1,
        }
    }
    let formula_cells = formula_cells_before - removed + added;
    if let Some(budget) = host.options.formula_cell_budget
        && added > removed
        && formula_cells > budget
    {
        return Err(CommandError::FormulaBudget {
            formula_cells,
            budget,
        });
    }

    let mut changed_cells = Vec::new();
    let mut cleared_areas = Vec::new();
    let mut seeded = 0usize;
    for seed in engine.seeds() {
        match seed {
            InputSeed::Cell { at, dependents, .. } => {
                changed_cells.push(*at);
                seeded += dependents.len();
            }
            InputSeed::Area { area, dependents } => {
                cleared_areas.push(*area);
                seeded += dependents.len();
            }
            // The cells a removed array formula filled change though no input of theirs did;
            // the anchor's own cell is among the changed or cleared cells already.
            InputSeed::Spill { dependents, .. } | InputSeed::Name { dependents, .. } => {
                seeded += dependents.len()
            }
        }
    }
    // After the commit the pending nodes are those pending before, the added and replaced nodes
    // and the seeds' dependents, less the removed ones: at most this many.
    // A structural edit leaves every node pending.
    let mut pending = Vec::with_capacity(
        if axis_edits.is_empty() && sheet_edit.is_none() && !facts_changed && !tables_changed {
            pending_before + added + replaced + seeded
        } else {
            formula_cells
        },
    );

    let committed = calc_engine::commit_edit(engine);
    host.model = document.into_model();
    if tables_changed {host.engine=host.engine.rebuilt(&host.model);}
    for edit in &axis_edits {
        host.references.apply(&axis_transform(*edit));
    }
    if let Some(calc_doc_model::SheetEdit::Remove { sheet }) = sheet_edit {
        host.references
            .apply(&calc_fml::reference::StructuralEdit::RemoveSheet {
                sheet: *sheet,
                order: Vec::new(),
            });
    }
    if transformed_scope.is_some() {
        host.transformed_calculation_scope = transformed_scope;
    }
    host.calculation_cancelled = false;
    host.calc_events.push(calc_engine::CalcEvent::Committed {
        revision: host.model.revision(),
    });
    let superseded_recalc = host.work.as_ref().map(|work| work.tick());
    if changes_inputs
        || !axis_edits.is_empty()
        || sheet_edit
            .is_some_and(|edit| !matches!(edit, calc_doc_model::SheetEdit::SetVisibility { .. }))
    {
        host.inputs_changed_since_open = true;
    }
    host.axis_edits.extend(axis_edits);

    for node in host.engine.pending() {
        if let Some(record) = host.engine.node(node) {
            pending.push(FormulaNode::anchor(record));
        }
    }
    pending.sort_unstable();
    Ok(CommandReceipt {
        base: command.base,
        revision: committed.revision(),
        changed_cells,
        cleared_areas,
        input_writes: batch.operations.iter().filter_map(|op| match op {
            WorkbookOp::SetCell { at, .. } => Some(GridArea::cell(*at)),
            WorkbookOp::Clear { area } => Some(*area),
            WorkbookOp::SetArray { array, .. } => Some(array.area()),
            _ => None,
        }).collect(),
        pending,
        superseded_recalc,
    })
}

/// Brings back the revision `earlier` (of this host's lineage) as the next revision
/// ([`calc_doc_model::ModelRoot::restored`]), with the engine state built afresh for it
/// ([`calc_engine::EngineState::rebuilt`]: every formula pending, nothing published) and
/// `axis_edits` as the axis edits since the workbook was opened that lead to it. This undoes an
/// edit no batch inverts, a structural one. A recalculation in progress is superseded on its next step.
pub(crate) fn restore(
    host: &mut HostState,
    earlier: &Arc<ModelRoot>,
    axis_edits: Vec<AxisEdit>,
) -> Result<CommandReceipt, CommandError> {
    restore_inner(host,earlier,axis_edits,None)
}

/// Source rebasing leaves live coordinates unchanged and retains honest held values.
pub(crate) fn restore_package_source(host:&mut HostState,earlier:&Arc<ModelRoot>,source:&calc_doc::LoadedWorkbook)->Result<CommandReceipt,CommandError>{
    let caches=source.sheets().iter().filter(|s|host.model.sheet_by_id(s.id()).is_none()).flat_map(|sheet|{
        sheet.formulas().filter_map(move |(r,c,record)|record.cache().map(|cache|(CellKey::new(sheet.id(),r,c).unwrap(),cache.clone())))
            .chain(sheet.value_caches().filter_map(|(r,c,cache)|{let at=CellKey::new(sheet.id(),r,c).unwrap();(earlier.definitions().array_containing(at).is_some()&&earlier.definitions().unloaded(at).is_none()).then(||(at,cache.clone()))}))
    }).collect();
    restore_inner(host,earlier,host.axis_edits.clone(),Some(caches))
}
fn restore_inner(host:&mut HostState,earlier:&Arc<ModelRoot>,axis_edits:Vec<AxisEdit>,caches:Option<Vec<(CellKey,calc_doc_model::FileCached)>>)->Result<CommandReceipt,CommandError>{
    let transformed_scope = if host.scoped_calculation_active() {
        let mut before: Vec<_> = host.model.sheets().iter().map(|sheet| sheet.id()).collect();
        let mut after: Vec<_> = earlier.sheets().iter().map(|sheet| sheet.id()).collect();
        before.sort();
        after.sort();
        // Restoring authored sheets never revives locations removed from an active command.
        // Only sheets present on both sides keep their scope identity. A fresh explicit
        // calculation may target restored sheets after this successor completes.
        let removed: Vec<_> = before
            .iter()
            .chain(&after)
            .copied()
            .filter(|sheet| {
                before.binary_search(sheet).is_err() || after.binary_search(sheet).is_err()
            })
            .collect();
        let common = host
            .axis_edits
            .iter()
            .zip(&axis_edits)
            .take_while(|(a, b)| a == b)
            .count();
        active_scope(host).map(|mut scope| {
            for sheet in &removed {
                scope = scope.transformed(&calc_fml::reference::StructuralEdit::RemoveSheet {
                    sheet: *sheet,
                    order: Vec::new(),
                });
            }
            for edit in host.axis_edits[common..].iter().rev() {
                let change = match edit.change() {
                    calc_doc_model::AxisChange::Insert => calc_doc_model::AxisChange::Delete,
                    calc_doc_model::AxisChange::Delete => calc_doc_model::AxisChange::Insert,
                };
                let inverse = AxisEdit::new(
                    edit.sheet(),
                    edit.axis(),
                    change,
                    edit.first(),
                    edit.count(),
                )
                .expect("an inverse of a checked axis edit");
                scope = axis_scope(scope, inverse);
            }
            for &edit in &axis_edits[common..] {
                scope = axis_scope(scope, edit);
            }
            scope
        })
    } else {
        None
    };
    let mut reference_edits = Vec::new();
    for sheet in host.model.sheets() {
        if earlier.sheet_by_id(sheet.id()).is_none() {
            reference_edits.push(calc_fml::reference::StructuralEdit::RemoveSheet {
                sheet: sheet.id(),
                order: Vec::new(),
            });
        }
    }
    let common = host
        .axis_edits
        .iter()
        .zip(&axis_edits)
        .take_while(|(a, b)| a == b)
        .count();
    for edit in host.axis_edits[common..].iter().rev() {
        let change = match edit.change() {
            calc_doc_model::AxisChange::Insert => calc_doc_model::AxisChange::Delete,
            calc_doc_model::AxisChange::Delete => calc_doc_model::AxisChange::Insert,
        };
        let inverse = AxisEdit::new(
            edit.sheet(),
            edit.axis(),
            change,
            edit.first(),
            edit.count(),
        )
        .expect("checked inverse axis edit");
        reference_edits.push(axis_transform(inverse));
    }
    reference_edits.extend(
        axis_edits[common..]
            .iter()
            .map(|edit| axis_transform(*edit)),
    );
    let base = host.model.revision();
    let model = host
        .model
        .restored(earlier)
        .map_err(|error| CommandError::Document(EditError::Model(error)))?;
    let engine = if let Some(caches)=caches {
        host.engine.rebuilt_with_held_caches(&host.model,&model,caches).map_err(|e|CommandError::Document(EditError::SheetOperation(e.to_string())))?
    }else{host.engine.rebuilt(&model)};
    host.model = model;
    for edit in reference_edits {
        host.references.apply(&edit);
    }
    if transformed_scope.is_some() {
        host.transformed_calculation_scope = transformed_scope;
    }
    host.engine = engine;
    host.calculation_cancelled = false;
    host.calc_events.push(calc_engine::CalcEvent::Committed {
        revision: host.model.revision(),
    });
    let superseded_recalc = host.work.as_ref().map(|work| work.tick());
    host.inputs_changed_since_open = true;
    host.axis_edits = axis_edits;
    let mut pending: Vec<CellKey> = host
        .engine
        .pending()
        .filter_map(|node| host.engine.node(node).map(FormulaNode::anchor))
        .collect();
    pending.sort_unstable();
    Ok(CommandReceipt {
        base,
        revision: host.model.revision(),
        changed_cells: Vec::new(),
        cleared_areas: Vec::new(),
        input_writes: Vec::new(),
        pending,
        superseded_recalc,
    })
}

pub(crate) fn active_scope(host: &HostState) -> Option<calc_engine::CalcScope> {
    host.transformed_calculation_scope
        .clone()
        .or_else(|| {
            host.work
                .as_ref()
                .map(|work| work.admission().scope.clone())
        })
        .or_else(|| {
            host.requested_calculation
                .as_ref()
                .map(|(scope, _)| scope.clone())
        })
        .filter(|scope| !matches!(scope, calc_engine::CalcScope::Workbook))
}

fn axis_scope(scope: calc_engine::CalcScope, edit: AxisEdit) -> calc_engine::CalcScope {
    scope.transformed(&axis_transform(edit))
}
fn axis_transform(edit: AxisEdit) -> calc_fml::reference::StructuralEdit {
    use calc_fml::reference::{EditAxis, StructuralEdit};
    let axis = match edit.axis() {
        calc_doc_model::Axis::Rows => EditAxis::Rows,
        calc_doc_model::Axis::Columns => EditAxis::Columns,
    };
    match edit.change() {
        calc_doc_model::AxisChange::Insert => StructuralEdit::Insert {
            sheet: edit.sheet(),
            axis,
            first: edit.first(),
            count: edit.count(),
        },
        calc_doc_model::AxisChange::Delete => StructuralEdit::Delete {
            sheet: edit.sheet(),
            axis,
            first: edit.first(),
            count: edit.count(),
        },
    }
}
