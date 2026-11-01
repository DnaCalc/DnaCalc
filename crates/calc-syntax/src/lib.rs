//! Neutral semantic identity shared by authored formula blocks and the formula evaluator.
//! The existing parser, binder and reference transforms remain in CalcFml in this first slice.

use std::sync::Arc;

/// Excel's implicit-intersection and lifting/spilling evaluation dialects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormulaForm {
    Legacy,
    Dynamic,
}

/// The reference placement rules of a template. Cell references fail at the grid edge;
/// relative defined-name references wrap. These identities must never coalesce.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TemplatePlacement {
    Cell,
    Name,
    Rule,
}

/// Binding-independent reference-normalized syntax, placement and evaluation dialect.
/// Per-cell metadata and cached answers are held outside this identity. The conservative
/// normalizer may retain spelling distinctions; broader canonicalization remains separate work.
/// The first slice uses CalcFml's existing conservative reference-template normalizer; syntax
/// it cannot normalize has no key and cannot enter the shared plan cache.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SemanticTemplateKey {
    pub normalized_ir: Arc<str>,
    pub placement: TemplatePlacement,
    pub evaluation: FormulaForm,
}
