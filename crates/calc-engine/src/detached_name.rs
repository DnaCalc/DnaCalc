//! Bind a selected name by identity for explicit inspection, including a workbook name
//! shadowed on the caller sheet. Its body still uses the ordinary name compiler and reader.
use super::DetachedFormula;
use crate::{NotModeled, read::SnapshotCatalog};
use calc_doc_model::ModelRoot;
use calc_fml::reference::{
    AxisRef, BoundName, ExternalBookRef, FormulaAnchor, NameBindingProof, NameDefinition, NameScope,
    ReferenceCatalog, ReferenceNotation, ReferenceShape, SheetTarget, TableLookup, bind_references,
};
use calc_fml::syntax::{SyntaxChild, SyntaxKind, TokenKind};
use calc_value::{CellKey, GridArea, NameId, SheetId};
use std::cell::Cell;

struct SelectedName<'a> {
    base: SnapshotCatalog<'a>,
    selected: &'a calc_doc_model::NameNode,
    root: Cell<bool>,
}
const INSPECTION: &str = "__DnaCalcNameInspection";
fn selected_catalog(inputs: &ModelRoot, id: NameId) -> Result<SelectedName<'_>, NotModeled> {
    let selected = inputs
        .definitions()
        .name_table()
        .get(id)
        .ok_or(NotModeled::Evaluation(calc_fml::EvalFault::NotModelled {
            construct: "an explicit evaluation of a deleted name",
            owner: "dc-971p",
        }))?;
    Ok(SelectedName {
        base: SnapshotCatalog::new(inputs),
        selected,
        root: Cell::new(false),
    })
}
impl ReferenceCatalog for SelectedName<'_> {
    fn defined_name(&self, name: &str, scope: NameScope) -> NameDefinition {
        if self.root.get() && name == INSPECTION {
            let d = self.selected.definition();
            if d.function || d.vb_procedure || d.xlm {
                NameDefinition::NotModelled
            } else {
                NameDefinition::Formula(d.formula.clone())
            }
        } else {
            self.base.defined_name(name, scope)
        }
    }
    fn name_binding(
        &self,
        name: &str,
        scope: NameScope,
        parent: Option<NameId>,
    ) -> Option<NameBindingProof> {
        let root = name == INSPECTION && parent.is_none();
        self.root.set(root);
        if root {
            Some(NameBindingProof {
                resolved: Some(BoundName {
                    id: self.selected.id(),
                    version: self.selected.binding_version(),
                    scope: self.selected.definition().scope,
                }),
                probes: Vec::new(),
                parent: None,
            })
        } else {
            self.base.name_binding(name, scope, parent)
        }
    }
    fn sheet_named(&self, name: &str) -> Option<SheetId> {
        self.base.sheet_named(name)
    }
    fn table(&self, name: &str) -> TableLookup {
        self.base.table(name)
    }
    fn table_containing(&self, at: CellKey) -> TableLookup {
        self.base.table_containing(at)
    }
    fn sheet_span(&self, first: SheetId, last: SheetId) -> Option<Vec<SheetId>> {
        self.base.sheet_span(first, last)
    }
    fn unknown_calls_are_names(&self) -> bool {
        self.base.unknown_calls_are_names()
    }
    fn workbook_name(&self) -> Option<&str> {
        self.base.workbook_name()
    }
    fn external_sheet(&self, book: &ExternalBookRef, sheet: &str) -> Option<SheetId> {
        self.base.external_sheet(book, sheet)
    }
    fn external_function_target(&self, book: &ExternalBookRef) -> Option<String> {
        self.base.external_function_target(book)
    }
    fn external_name(
        &self,
        book: &ExternalBookRef,
        sheet: Option<&str>,
        name: &str,
    ) -> Option<String> {
        self.base.external_name(book, sheet, name)
    }
}
impl DetachedFormula {
    /// The literal range, or spill anchor, named by a stable workbook definition.
    /// The second field identifies a spill. Relative or calculated targets have no stable area.
    pub fn stable_name_source(inputs: &ModelRoot, id: NameId) -> Option<(GridArea, bool)> {
        let catalog = selected_catalog(inputs, id).ok()?;
        let definition = catalog.selected.definition();
        if definition.function || definition.vb_procedure || definition.xlm {
            return None;
        }
        let local = match definition.scope {
            calc_doc_model::NameScope::Workbook => None,
            calc_doc_model::NameScope::Sheet(sheet) => Some(sheet),
        };
        let sheet = local.or_else(|| inputs.sheets().first().map(|entry| entry.id()))?;
        let anchor = FormulaAnchor::new(CellKey::new(sheet, 1, 1).ok()?);
        let body = definition.formula.strip_prefix('=').unwrap_or(&definition.formula);
        let tree = calc_fml::parse_formula(&format!("={body}"));
        if !tree.diagnostics().is_empty() {
            return None;
        }
        let mut reference = tree.expression()?;
        while reference.kind() == SyntaxKind::GroupingExpr {
            reference = reference.child_nodes().next()?;
        }
        let spill = match reference.children() {
            [SyntaxChild::Node(operand), SyntaxChild::Token(operator)]
                if reference.kind() == SyntaxKind::PostfixExpr && operator.kind() == TokenKind::Hash =>
            {
                reference = operand;
                true
            }
            [SyntaxChild::Token(callee), SyntaxChild::Node(arguments)]
                if reference.kind() == SyntaxKind::CallExpr && callee.text().eq_ignore_ascii_case("_xlfn.ANCHORARRAY") =>
            {
                let mut operands = arguments.child_nodes();
                reference = operands.next()?;
                if operands.next().is_some() { return None; }
                true
            }
            _ => false,
        };
        let sites = bind_references(&tree, ReferenceNotation::A1, anchor, &catalog);
        let [site] = sites.as_slice() else { return None };
        if site.span != reference.span() || site.this_row {
            return None;
        }
        let operand = site.binding.as_ref().ok()?;
        if operand.sheet == SheetTarget::Anchor && local.is_none() {
            return None;
        }
        let absolute = |axis| matches!(axis, AxisRef::Absolute(_));
        let fixed_cell = |at: calc_fml::reference::CellRef| absolute(at.row) && absolute(at.column);
        let fixed = match operand.shape {
            ReferenceShape::Cell(at) => fixed_cell(at),
            ReferenceShape::Area { first, last } => fixed_cell(first) && fixed_cell(last),
            ReferenceShape::Rows { first, last } | ReferenceShape::Columns { first, last } =>
                absolute(first) && absolute(last),
            ReferenceShape::Deleted => false,
        };
        if !fixed || (spill && !matches!(operand.shape, ReferenceShape::Cell(_))) {
            return None;
        }
        operand.instantiate(anchor).ok().map(|area| (area, spill))
    }

    /// Definition/caller-qualified source evidence from the ordinary name compiler.
    /// This compiles only; it runs no user code and creates no evaluation or shared value.
    pub fn analyze_name(
        inputs: &ModelRoot,
        id: NameId,
        anchor: CellKey,
        functions: &calc_fml::CallableBindings,
        cache: &mut calc_fml::PlanCache,
    ) -> Result<calc_fml::NameAnalysis, NotModeled> {
        let catalog = selected_catalog(inputs, id)?;
        calc_fml::analyze_loaded_names(
            &calc_fml::parse_formula(&format!("={INSPECTION}")),
            ReferenceNotation::A1,
            FormulaAnchor::new(anchor),
            &catalog,
            cache,
            calc_fml::FormulaForm::Dynamic,
            functions,
        )
        .map_err(NotModeled::Compile)
    }
    /// A selected non-cell node compiled at its actual caller, through the canonical
    /// name binding path (relative name references wrap; cell formula references do not).
    pub fn compile_name(
        inputs: &ModelRoot,
        id: NameId,
        anchor: CellKey,
    ) -> Result<Self, NotModeled> {
        Self::compile_name_with_bindings(inputs, id, anchor, &Default::default())
    }

    /// Compile with registration metadata, without giving diagnostics executable owners.
    pub fn compile_name_with_bindings(
        inputs: &ModelRoot,
        id: NameId,
        anchor: CellKey,
        functions: &calc_fml::CallableBindings,
    ) -> Result<Self, NotModeled> {
        let catalog = selected_catalog(inputs, id)?;
        let mut cache = calc_fml::PlanCache::new();
        let plan = calc_fml::compile_loaded_with_bindings(
            &calc_fml::parse_formula(&format!("={INSPECTION}")),
            ReferenceNotation::A1,
            FormulaAnchor::new(anchor),
            &catalog,
            &mut cache,
            calc_fml::FormulaForm::Dynamic,
            functions,
        )
        .map_err(NotModeled::Compile)?;
        Ok(Self {
            plan,
            reference_result: true,
            compiled_inputs: inputs.pin(),
            caller_sheet: anchor.sheet(),
            selected_name: Some(BoundName {
                id: catalog.selected.id(),
                version: catalog.selected.binding_version(),
                scope: catalog.selected.definition().scope,
            }),
        })
    }
}
