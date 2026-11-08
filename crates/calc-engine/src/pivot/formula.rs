//! Calculated pivot fields use CalcFml's evaluator, with source-field totals supplied as reads.
use super::*;
use calc_fml::reference::{
    CellRun, ExternalBookRef, FormulaAnchor, NameDefinition, NameScope, ReferenceCatalog,
    ReferenceNotation, ReferenceReader, SpillExtent, TableLookup,
};
use calc_fml::{EvalFrame, EvalScratch, FactRead, FormulaEvaluationContext, PlanCache, ReadDenial, ReadRequest};
use std::ops::ControlFlow;

struct FieldCatalog {
    names: BTreeMap<String, String>,
}
impl ReferenceCatalog for FieldCatalog {
    fn sheet_named(&self, _: &str) -> Option<calc_value::SheetId> { None }
    fn defined_name(&self, name: &str, _: NameScope) -> NameDefinition {
        self.names.get(&calc_value::fold_workbook_name(name)).cloned()
            .map_or(NameDefinition::NotModelled, NameDefinition::Formula)
    }
    fn table(&self, _: &str) -> TableLookup { TableLookup::NotModelled }
    fn table_containing(&self, _: CellKey) -> TableLookup { TableLookup::NotModelled }
    fn sheet_span(&self, _: calc_value::SheetId, _: calc_value::SheetId) -> Option<Vec<calc_value::SheetId>> { None }
    fn unknown_calls_are_names(&self) -> bool { false }
    fn workbook_name(&self) -> Option<&str> { None }
    fn external_sheet(&self, _: &ExternalBookRef, _: &str) -> Option<calc_value::SheetId> { None }
    fn external_function_target(&self, _: &ExternalBookRef) -> Option<String> { None }
}
struct TotalsReader<'a> {
    sheet: calc_value::SheetId,
    values: &'a [CalcValue],
}
impl ReferenceReader for TotalsReader<'_> {
    fn read_denial(&self) -> Option<ReadDenial> { None }
    fn read_fact(&mut self, _: FactRead) {}
    fn read_area(&mut self, _: ReadRequest, area: GridArea, sink: &mut dyn FnMut(CellRun<'_>) -> ControlFlow<()>) {
        if area.cell_count() != 1 || area.first().sheet() != self.sheet || area.first().row() != 1 {
            let _ = sink(CellRun::Missing(area.cell_count()));
            return;
        }
        match area.first().column().checked_sub(1).and_then(|i| self.values.get(i as usize)) {
            Some(value) => { let _ = sink(CellRun::Values(std::slice::from_ref(value))); }
            None => { let _ = sink(CellRun::Missing(1)); }
        }
    }
    fn spill_extent(&mut self, _: CellKey) -> SpillExtent { SpillExtent::NotModelled }
    fn subtotal_exclusions(&mut self, _: GridArea) -> Option<calc_fml::SubtotalExclusions> { None }
}

fn column_name(mut number: usize) -> String {
    let mut name = String::new();
    while number > 0 {
        number -= 1;
        name.push((b'A' + (number % 26) as u8) as char);
        number /= 26;
    }
    name.chars().rev().collect()
}

fn supported_syntax(tree: &calc_fml::syntax::SyntaxTree) -> bool {
    use calc_fml::syntax::{SyntaxKind, TokenKind};
    let mut nodes = vec![tree.root()];
    while let Some(node) = nodes.pop() {
        if !matches!(node.kind(), SyntaxKind::FormulaRoot | SyntaxKind::NumberLiteralExpr
            | SyntaxKind::IdentifierExpr | SyntaxKind::CallExpr | SyntaxKind::ArgumentList
            | SyntaxKind::BinaryExpr | SyntaxKind::PrefixExpr | SyntaxKind::GroupingExpr) {
            return false;
        }
        if node.kind() == SyntaxKind::CallExpr
            && node.tokens().first().is_none_or(|token| !token.text().eq_ignore_ascii_case("IF")) {
            return false;
        }
        nodes.extend(node.child_nodes());
    }
    tree.root().tokens().iter().all(|token| matches!(token.kind(), TokenKind::Equals | TokenKind::NotEqual
        | TokenKind::Less | TokenKind::LessEqual | TokenKind::Greater | TokenKind::GreaterEqual
        | TokenKind::Number | TokenKind::Identifier | TokenKind::LParen | TokenKind::RParen
        | TokenKind::Comma | TokenKind::Plus | TokenKind::Minus | TokenKind::Star | TokenKind::Slash
        | TokenKind::Whitespace | TokenKind::Eof))
}

/// Evaluate a field formula against actual source totals; unknown names and references refuse.
pub(super) fn evaluate_field(
    cache: &PivotCache,
    field: usize,
    source_count: usize,
    values: &[CalcValue],
    anchor: CellKey,
    numeric: &calc_value::NumericMode,
) -> Result<CalcValue, PivotIssue> {
    let bad = |why: &str| refusal(&cache.part, why);
    if values.len() != source_count || source_count > 16_384 { return Err(bad("Invalid calculated-field inputs")); }
    let mut names = BTreeMap::new();
    for (i, field) in cache.fields.iter().enumerate() {
        let definition = if i < source_count { format!("${}$1", column_name(i + 1)) }
            else { field.attributes.get("formula").cloned().ok_or_else(|| bad("A calculated field has no formula"))? };
        if names.insert(calc_value::fold_workbook_name(&field.name), definition).is_some() {
            return Err(bad("Ambiguous calculated-field names"));
        }
    }
    let catalog = FieldCatalog { names };
    let origin = FormulaAnchor::new(anchor);
    for virtual_field in &cache.fields[source_count..] {
        let formula = virtual_field.attributes.get("formula").ok_or_else(|| bad("A calculated field has no formula"))?;
        let tree = calc_fml::parse_formula(&format!("={formula}"));
        if !tree.diagnostics().is_empty() || !supported_syntax(&tree) || !calc_fml::bind_references(&tree, ReferenceNotation::A1, origin, &catalog).is_empty() {
            return Err(bad("References in calculated pivot fields are not modelled"));
        }
    }
    let formula = cache.fields.get(field).and_then(|field| field.attributes.get("formula"))
        .ok_or_else(|| bad("Invalid calculated field"))?;
    let tree = calc_fml::parse_formula(&format!("={formula}"));
    let plan = calc_fml::compile_loaded(&tree, ReferenceNotation::A1, origin, &catalog, &mut PlanCache::new())
        .map_err(|_| bad("This calculated-field formula is not modelled"))?;
    let instance = calc_fml::instantiate(&plan, origin);
    let mut reader = TotalsReader { sheet: anchor.sheet(), values };
    let mut scratch = EvalScratch::new();
    let mut services = FormulaEvaluationContext::new(numeric, &mut reader, None, None, ReadRequest::DEPENDENCY).with_catalog(&catalog);
    let value = calc_fml::evaluate(EvalFrame::new(&instance, &mut scratch), &mut services)
        .map_err(|_| bad("This calculated-field evaluation is not modelled"))?;
    match value.core_ref() {
        calc_value::CoreRef::Number(_) | calc_value::CoreRef::Error(_) => Ok(value),
        _ => Err(bad("This calculated-field result is not modelled")),
    }
}
