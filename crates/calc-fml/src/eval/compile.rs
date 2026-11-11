//! Lowering a parsed formula into a [`CompiledPlan`].
//!
//! Literals and array constants, bound references and the reference operators, the arithmetic,
//! comparison and concatenation operators, implicit intersection (`@`) and spill references
//! (`#`), calls of catalogue functions, LET and LAMBDA, calls of LAMBDA values, and defined names
//! compile; everything else is refused with a typed [`CompileError`] that names the construct and
//! the work package that owns it, never compiled into an approximation.
//!
//! A number literal becomes its nearest binary64 here, once, under no numeric mode (ties to even,
//! at most 15 significant digits); that Excel converts every such literal to the same binary64 is
//! not measured by this package. A plan holds no folded constant: every operation runs at
//! evaluation, under the caller's mode.
//!
//! Lowering walks the tree on a stack of its own [`Task`]s, not on the native stack: a node's
//! operands are lowered before the instruction that consumes them, in source order, so a formula
//! nested as deep as its 8,192 characters allow (`=1+1+...+1`, `=((...(1)...))`, `=--...-1`)
//! lowers without recursion. A defined name's formula is lowered, once per plan, into a block of
//! its own when the formula first uses the name; that lowering nests at most
//! [`MAX_NAME_NESTING`] names deep.
//!
//! Names are resolved as Excel resolves them (ported from OxFml's binder and evaluator,
//! `binding/mod.rs` and `eval/mod.rs`): a name written as a value is a LET name or LAMBDA
//! parameter in scope (the innermost, regardless of case), else a defined name; a name written as
//! a call is a built-in function first (Excel 16.0, 2026-10-23:
//! `=LET(gcd,LAMBDA(a,b,a+b),gcd(48,36))` is GCD's 12), else a LET name or parameter holding a
//! LAMBDA, else a defined name. The file forms Excel writes are read too: `_xlfn.` before a
//! function, `_xlpm.` before a LET name or LAMBDA parameter, `_xlop.` before an optional LAMBDA
//! parameter, `_xlfn.SINGLE(x)` for `@x` and `_xlfn.ANCHORARRAY(x)` for `x#` (OxFml's
//! `binding/excel_prefix.rs`, measured on Excel 16.0 on 2026-09-15).

use super::entry::{GrammarRule, Rejection, check_entry};
use super::legacy::Place;
use super::{
    Admission, CompiledPlan, FormulaForm, Instruction, Literal, NameTarget, PlanCache, Target,
    UserFunctionSource,
};
use crate::parse_formula;
use crate::reference::{
    BindError, BoundName, ExternalBookRef, ExternalOperand, FormulaAnchor, NameBindingProof,
    NameDefinition, NameScope, ReferenceCatalog, ReferenceNotation, ReferenceOperand, SheetTarget,
    TableLookup, bind_references, external_name_parts, external_operand, qualified_call,
    qualified_name,
    qualified_sheet_call,
};
use crate::syntax::{SyntaxChild, SyntaxKind, SyntaxNode, SyntaxTree, TextSpan, Token, TokenKind};
use calc_catalogue::{FIELD_ACCESS_FILE_NAME as FIELD_ACCESS, FilePrefix};
use calc_func::value::{CellKey, ExcelText, SheetId, WorksheetErrorCode};
use calc_func::{Notation, ParameterClass, catalogue};
use std::collections::HashMap;
use std::sync::Arc;

/// Why a formula did not compile: a claim about Excel ([`CompileError::Rejected`]), a gap of
/// this compiler ([`CompileError::NotModelled`]), or a reference binding refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    /// Excel's formula entry refuses this formula too ([`check_entry`]): a claim, scored against
    /// Excel like a value.
    Rejected { span: TextSpan, rule: Rejection },
    /// A construct this compiler does not model: a typed refusal.
    NotModelled { span: TextSpan, gap: Gap },
    /// A reference that binding refused.
    Reference { span: TextSpan, error: BindError },
}

/// What [`CompileError::NotModelled`] does not model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gap {
    /// Formula syntax this compiler does not lower yet: `construct` says what it is and `owner`
    /// names the work package that owns it.
    Construct {
        construct: &'static str,
        owner: &'static str,
    },
    /// No catalogue entry is written this way: a function the catalogue does not hold yet, or a
    /// name Excel does not know (whose `#NAME?` result is not modelled either).
    UnknownFunction { spelling: String },
    /// A number literal whose Excel value is not modelled: more than 15 significant digits
    /// (which Excel rounds to 15 at entry), beyond binary64's range, or in or below its subnormal
    /// range. Owned by bead dc-m3u (numeric text policy).
    NumberLiteral,
}

impl CompileError {
    /// The span the error names.
    pub fn span(&self) -> TextSpan {
        match self {
            CompileError::Rejected { span, .. }
            | CompileError::NotModelled { span, .. }
            | CompileError::Reference { span, .. } => *span,
        }
    }

    /// The rule Excel's entry refuses the formula by, for a rejection.
    pub fn rejection(&self) -> Option<Rejection> {
        match self {
            CompileError::Rejected { rule, .. } => Some(*rule),
            _ => None,
        }
    }

    /// The work package that owns a construct this compiler does not lower.
    pub fn owner(&self) -> Option<&'static str> {
        match self {
            CompileError::NotModelled {
                gap: Gap::Construct { owner, .. },
                ..
            } => Some(owner),
            _ => None,
        }
    }
}

/// A rejection by the grammar's syntax rule at `span`.
fn syntax(span: TextSpan) -> CompileError {
    CompileError::Rejected {
        span,
        rule: Rejection::Grammar(GrammarRule::Syntax),
    }
}

/// A rejection by the grammar rule `rule` at `span`.
fn grammar(span: TextSpan, rule: GrammarRule) -> CompileError {
    CompileError::Rejected {
        span,
        rule: Rejection::Grammar(rule),
    }
}

/// A name no catalogue entry is written as.
fn unknown(span: TextSpan, spelling: String) -> CompileError {
    CompileError::NotModelled {
        span,
        gap: Gap::UnknownFunction { spelling },
    }
}

/// The work package that owns the refusals of the constructs this module lowers and does not
/// model: L1 (dc-mzk) lowered them, and L3 (dc-m3u) took over its open refusals.
const OWNER: &str = "dc-10y";

/// The work package that owns the defined names a catalogue does not model.
const NAMES_OWNER: &str = "dc-10y";

/// Work package L3, which owns tables, 3-D references, names with relative references and the
/// calls of names no defined name holds.
const L3: &str = "dc-10y";

/// How many defined names deep one name's formula may use another's while it is lowered.
pub const MAX_NAME_NESTING: usize = 32;

/// Compiles the formula `tree` in its legacy form ([`FormulaForm::Legacy`]), as a user enters it:
/// [`compile_with`] under that form.
///
/// It reads the formula as a user enters it: a function is written by its plain name, and also
/// through the prefixes a file stores before it ([`file_unprefixed`]). [`compile_loaded`] reads
/// the formula as a file stores it.
pub fn compile(
    tree: &SyntaxTree,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    cache: &mut PlanCache,
) -> Result<Arc<CompiledPlan>, CompileError> {
    compile_as(
        tree,
        false,
        FormulaForm::Legacy,
        notation,
        anchor,
        catalog,
        cache,
        None,
    )
}

/// [`compile`] for a formula whose text is as a file stores it (the text of a worksheet's `<f>`
/// element, as the document model holds every formula), in its legacy form, the form of every
/// formula a file holds without dynamic-array cell metadata: a call that writes, without the
/// prefix `_xlfn.`, a function a file stores with that prefix ([`calc_func::FilePrefix::Xlfn`]) is
/// an unknown name to Excel's load, and gives `#NAME?` where it is called, whatever its arguments,
/// which are not compiled. Every other formula compiles as [`compile`] compiles it.
pub fn compile_loaded(
    tree: &SyntaxTree,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    cache: &mut PlanCache,
) -> Result<Arc<CompiledPlan>, CompileError> {
    compile_as(
        tree,
        true,
        FormulaForm::Legacy,
        notation,
        anchor,
        catalog,
        cache,
        None,
    )
}

/// [`compile_loaded`] in the form `form`: an array formula a file holds is compiled in the
/// dynamic-array form, which lifts as an array formula does.
pub fn compile_loaded_with(
    tree: &SyntaxTree,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    cache: &mut PlanCache,
    form: FormulaForm,
) -> Result<Arc<CompiledPlan>, CompileError> {
    compile_as(tree, true, form, notation, anchor, catalog, cache, None)
}

/// Compiles the formula `tree`, as a user enters it, in the form `form`, its references read in
/// `notation` and bound for a formula anchored at `anchor` through `catalog`, and returns the plan
/// `cache` holds for its normal form.
///
/// The plan does not depend on the anchor: binding gives each relative coordinate as an offset
/// from it, so the same formula copied to another anchor compiles to the same plan. Each call
/// lowers the tree and binds its references; the cache only shares the resulting plan. A defined
/// name the formula uses is compiled into the plan from the definition `catalog` gives, so a plan
/// holds the names' formulas as they were when it was compiled.
pub fn compile_with(
    tree: &SyntaxTree,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    cache: &mut PlanCache,
    form: FormulaForm,
) -> Result<Arc<CompiledPlan>, CompileError> {
    compile_as(tree, false, form, notation, anchor, catalog, cache, None)
}

/// Compiles with caller-owned user functions, without depending on their execution engines.
/// Plans hold checked slots and generations; evaluation receives the callable table through its services.
#[allow(clippy::too_many_arguments)]
pub fn compile_with_functions(
    tree: &SyntaxTree,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    cache: &mut PlanCache,
    form: FormulaForm,
    functions: &crate::CallableTable,
) -> Result<Arc<CompiledPlan>, CompileError> {
    compile_as(
        tree,
        false,
        form,
        notation,
        anchor,
        catalog,
        cache,
        Some((&functions.bindings(), true)),
    )
}

/// [`compile_loaded_with`] with the caller's current user-function registry.
#[allow(clippy::too_many_arguments)]
pub fn compile_loaded_with_functions(
    tree: &SyntaxTree,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    cache: &mut PlanCache,
    form: FormulaForm,
    functions: &crate::CallableTable,
) -> Result<Arc<CompiledPlan>, CompileError> {
    compile_as(
        tree,
        true,
        form,
        notation,
        anchor,
        catalog,
        cache,
        Some((&functions.bindings(), true)),
    )
}

/// Compile against the executable-free projection of the current callable table.
/// The catalog decides unresolved-call behavior: a lookup projection does not prove
/// that every workbook macro or external provider has been represented.
#[allow(clippy::too_many_arguments)]
pub fn compile_loaded_with_bindings(
    tree: &SyntaxTree,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    cache: &mut PlanCache,
    form: FormulaForm,
    functions: &crate::CallableBindings,
) -> Result<Arc<CompiledPlan>, CompileError> {
    compile_as(
        tree,
        true,
        form,
        notation,
        anchor,
        catalog,
        cache,
        Some((functions, false)),
    )
}

/// Name-source evidence from the same lowering that creates the executable plan.
/// Spans and caller context never participate in semantic interning.
#[derive(Debug)]
pub struct NameAnalysis {
    pub plan: Arc<CompiledPlan>,
    pub uses: Vec<crate::reference::NameUse>,
}

#[allow(clippy::too_many_arguments)]
pub fn analyze_loaded_names(
    tree: &SyntaxTree,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    cache: &mut PlanCache,
    form: FormulaForm,
    functions: &crate::CallableBindings,
) -> Result<NameAnalysis, CompileError> {
    compile_analyzed_as(
        tree,
        true,
        form,
        notation,
        anchor,
        catalog,
        cache,
        Some((functions, false)),
        true,
    )
}

fn compile_as(
    tree: &SyntaxTree,
    loaded: bool,
    form: FormulaForm,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    cache: &mut PlanCache,
    functions: Option<(&crate::CallableBindings, bool)>,
) -> Result<Arc<CompiledPlan>, CompileError> {
    compile_analyzed_as(
        tree, loaded, form, notation, anchor, catalog, cache, functions, false,
    )
    .map(|analysis| analysis.plan)
}

/// [`compile_with`], or with `loaded` the text as a file stores it ([`compile_loaded`]).
fn compile_analyzed_as(
    tree: &SyntaxTree,
    loaded: bool,
    form: FormulaForm,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
    cache: &mut PlanCache,
    functions: Option<(&crate::CallableBindings, bool)>,
    collect_name_uses: bool,
) -> Result<NameAnalysis, CompileError> {
    if let Err(rejection) = check_entry(tree, loaded) {
        return Err(CompileError::Rejected {
            span: rejection.span,
            rule: rejection.rule,
        });
    }
    let Some(expression) = tree.expression() else {
        return Err(syntax(tree.root().span()));
    };
    let mut lowering = Lowering {
        form,
        anchor,
        catalog,
        functions: functions.map(|(bindings, _)| bindings),
        complete_registry: functions.is_some_and(|(_, complete)| complete),
        sites: HashMap::new(),
        blocks: vec![Vec::new()],
        references: Vec::new(),
        wrapped: Vec::new(),
        externals: Vec::new(),
        notation,
        in_name: false,
        scope: Vec::new(),
        floor: 0,
        names: HashMap::new(),
        name_bindings: Vec::new(),
        callable_bindings: Vec::new(),
        name_context: crate::reference::NameEvaluationContext::for_formula(anchor),
        name_uses: collect_name_uses.then(Vec::new),
        last_name: None,
        last_name_sheet: None,
        nesting: 0,
        loaded,
    };
    let sites = sites(tree, notation, anchor, catalog);
    lowering.lower_root(expression, sites, 0, true, Place::Value)?;
    let plan = CompiledPlan {
        blocks: lowering
            .blocks
            .into_iter()
            .map(Vec::into_boxed_slice)
            .collect(),
        references: lowering.references.into_boxed_slice(),
        wrapped: lowering.wrapped.into_boxed_slice(),
        externals: lowering.externals.into_boxed_slice(),
        name_bindings: lowering.name_bindings.into_boxed_slice(),
        callable_bindings: lowering.callable_bindings.into_boxed_slice(),
        form,
    };
    Ok(NameAnalysis {
        plan: cache.share(plan),
        uses: lowering.name_uses.unwrap_or_default(),
    })
}

/// A site's binding and whether it takes the formula's row by implicit intersection.
type Site = (Result<ReferenceOperand, BindError>, bool);

type Sites = HashMap<TextSpan, Site>;

#[derive(Clone, Copy)]
enum Position {
    Call,
    Value,
}

enum Resolved {
    BuiltIn(calc_func::FunctionCallTarget<'static>),
    Local(u32),
    Defined(NameDefinition),
    Table(TableLookup),
    Registered(crate::CallableHandle),
    Reference,
    Unknown,
}

fn sites(
    tree: &SyntaxTree,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
) -> Sites {
    bind_references(tree, notation, anchor, catalog)
        .into_iter()
        .map(|site| (site.span, (site.binding, site.this_row)))
        .collect()
}

struct Lowering<'c> {
    form: FormulaForm,
    anchor: FormulaAnchor,
    catalog: &'c dyn ReferenceCatalog,
    functions: Option<&'c crate::CallableBindings>,
    complete_registry: bool,
    /// The binder's sites of the tree being lowered, by span; each is taken once, by the node it
    /// spans.
    sites: Sites,
    blocks: Vec<Vec<Instruction>>,
    references: Vec<ReferenceOperand>,
    /// Whether each reference is placed with its relative offsets wrapping around the grid's
    /// edge: the references of a defined name's formula.
    wrapped: Vec<bool>,
    /// The external references, in the order compile met them (dc-ylr).
    externals: Vec<ExternalOperand>,
    /// The notation the formula's references are read in.
    notation: ReferenceNotation,
    /// Whether the references being bound now belong to a defined name's formula.
    in_name: bool,
    /// The LET names and LAMBDA parameters in scope, innermost last, each with its local slot.
    scope: Vec<(String, u32)>,
    /// The first entry of `scope` the tree being lowered sees: a defined name's formula sees none
    /// of the formula that uses it.
    floor: usize,
    /// The block of each defined name compiled into the plan, by its upper-case spelling and
    /// definition.
    names: HashMap<
        (
            String,
            String,
            Option<calc_func::value::NameId>,
            Option<calc_func::value::SheetId>,
        ),
        NameTarget,
    >,
    name_bindings: Vec<NameBindingProof>,
    callable_bindings: Vec<crate::CallableBindingProof>,
    name_context: crate::reference::NameEvaluationContext,
    name_uses: Option<Vec<crate::reference::NameUse>>,
    last_name: Option<BoundName>,
    last_name_sheet: Option<calc_func::value::SheetId>,
    /// How many defined names' formulas are being lowered, one inside another.
    nesting: usize,
    /// Whether the formula's text is as a file stores it ([`compile_loaded`]).
    loaded: bool,
}

/// One step of the lowering, on the lowering's own stack: the steps a node pushes run in the
/// reverse order of their pushing, so it pushes the instruction that consumes its operands first
/// and its first operand last.
enum Task<'t> {
    /// Append to block `block` the instructions that push the value of `node`, which holds the
    /// place `place` of a legacy formula. `tail` says whether that value is the formula's result
    /// for the check of a final `+` or `-`: the root expression only. Parentheses, a prefix sign,
    /// the arguments of every call (IF's, IFS's, CHOOSE's and IFERROR's included) and LET's result
    /// end it. Excel adjusts only the formula's root operation (dc-b1l, measured in Excel
    /// 16.0.20430 through COM on 2026-10-25, `Range.Formula2`): `=IF(TRUE,(1+7*POWER(2,-52))-1)`,
    /// `=IFS(TRUE,...)` and `=CHOOSE(1,...)` are 7*2^-52, as `=IFERROR(0.5-0.4-0.1,1)` is
    /// -2.7755575615628914e-17 (bead dc-ibp2's capture).
    Lower {
        node: &'t SyntaxNode,
        tail: bool,
        block: usize,
        place: Place,
    },
    /// Append `instruction` to block `block`, after its operands' instructions.
    Emit {
        block: usize,
        instruction: Instruction,
    },
    /// Append to block `block` the check of the formula's final `+` or `-`, written `operator`.
    FinalSumCheck { block: usize, operator: &'t Token },
    /// Append to block `block` the store of the value on top of the stack into the next local,
    /// and bring `name` into scope as that local: a LET name after its value.
    Declare { block: usize, name: String },
    /// Bring `name` into scope as the next local without storing anything: a LAMBDA parameter,
    /// which the call fills.
    Bind { name: String },
    /// Take the last `.0` names out of scope.
    Unbind(usize),
}

impl<'c> Lowering<'c> {
    /// Resolve one name through the catalogue and the caller-owned layers in Excel's order.
    fn resolve(&mut self, text: &str, position: Position, span: TextSpan) -> Resolved {
        let function = catalogue()
            .resolve_surface(Notation::Call, file_function(text))
            .ok();
        if matches!(position, Position::Call) {
            if let Some(target) = function {
                return Resolved::BuiltIn(target);
            }
            if file_unprefixed(text).len() != text.len() {
                return Resolved::Unknown;
            }
            if self.sites.contains_key(&span) {
                return Resolved::Reference;
            }
        }
        if let Some(slot) = self.local(&parameter_key(text)) {
            return Resolved::Local(slot);
        }
        let scope = NameScope::Formula(self.name_context.lookup_sheet());
        let definition = self.lookup_name(text, scope, span);
        if !matches!(definition, NameDefinition::Unknown) {
            return Resolved::Defined(definition);
        }
        if matches!(position, Position::Value) {
            let table = self.catalog.table(text);
            if matches!(table, TableLookup::Found(_)) {
                return Resolved::Table(table);
            }
        }
        if matches!(position, Position::Call) {
            self.watch_callable(None, text);
        }
        if let Some(handle) = self
            .functions
            .and_then(|functions| functions.bind(None, text))
        {
            return Resolved::Registered(handle);
        }
        if matches!(position, Position::Value) && !self.loaded {
            if let Some(target) = function {
                return Resolved::BuiltIn(target);
            }
        }
        Resolved::Unknown
    }

    /// The local slot the next name brought into scope takes.
    fn depth(&self) -> u32 {
        u32::try_from(self.scope.len() - self.floor).expect("fewer than 2^32 locals")
    }

    /// The slot of the innermost LET name or LAMBDA parameter `key` in scope.
    fn local(&self, key: &str) -> Option<u32> {
        self.scope[self.floor..]
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map(|(_, slot)| *slot)
    }

    /// How a call at `place` admits a range or an array given to a single-value parameter.
    fn admission(&self, place: Place) -> Admission {
        match self.form {
            FormulaForm::Dynamic => Admission::Lift,
            FormulaForm::Legacy => place.admission(),
        }
    }

    /// Lowers `expression`, whose reference sites are `sites`, into block `block`, and each
    /// argument of a lazy call or LAMBDA body inside it into the block it is given, in source
    /// order.
    fn lower_root(
        &mut self,
        expression: &SyntaxNode,
        sites: Sites,
        block: usize,
        tail: bool,
        place: Place,
    ) -> Result<(), CompileError> {
        let outer = std::mem::replace(&mut self.sites, sites);
        let mut tasks = vec![Task::Lower {
            node: expression,
            tail,
            block,
            place,
        }];
        let mut outcome = Ok(());
        while let Some(task) = tasks.pop() {
            let step = match task {
                Task::Lower {
                    node,
                    tail,
                    block,
                    place,
                } => self.expression(node, tail, block, place, &mut tasks),
                Task::Emit { block, instruction } => {
                    self.blocks[block].push(instruction);
                    Ok(())
                }
                Task::FinalSumCheck { block, operator } => {
                    final_sum_check(operator, &mut self.blocks[block])
                }
                Task::Declare { block, name } => {
                    let slot = self.depth();
                    self.blocks[block].push(Instruction::Store(slot));
                    self.scope.push((name, slot));
                    Ok(())
                }
                Task::Bind { name } => {
                    let slot = self.depth();
                    self.scope.push((name, slot));
                    Ok(())
                }
                Task::Unbind(count) => {
                    let keep = self.scope.len() - count;
                    self.scope.truncate(keep);
                    Ok(())
                }
            };
            if let Err(error) = step {
                outcome = Err(error);
                break;
            }
        }
        self.sites = outer;
        outcome
    }

    /// Lowers `node` into block `block`: a leaf at once, and an operation as the tasks that lower
    /// its operands and then emit it, pushed onto `tasks`. `tail` and `place` are as
    /// [`Task::Lower`] says.
    fn expression<'t>(
        &mut self,
        node: &'t SyntaxNode,
        tail: bool,
        block: usize,
        place: Place,
        tasks: &mut Vec<Task<'t>>,
    ) -> Result<(), CompileError> {
        let span = node.span();
        match node.kind() {
            SyntaxKind::NumberLiteralExpr => {
                if !self.reference(node, block)? {
                    self.blocks[block].push(Instruction::Literal(number(node)?));
                }
            }
            SyntaxKind::IdentifierExpr => {
                if !self.reference(node, block)? {
                    let text = only_token(node).map(Token::text).unwrap_or_default();
                    if text.contains('[') {
                        // A bracket group binding left alone: `[y]` outside a LAMBDA's
                        // parameters and outside every table. Excel 16.0.20430 (2026-10-25,
                        // dc-10y) reads each such identifier of a file's formula in a cell
                        // outside every table as #REF!: `SUM([Qty])` as `SUM(#REF!)`, and
                        // `[@Qty]`, `SUM([#All])`, `[[Qty]]`, `[Qty]+1`, `ISREF([Qty])` (FALSE),
                        // `IFERROR([Qty],5)` (5) and `SUM([Qty],1)` likewise. (`x[1]`, also
                        // #REF! there, binds as a table x the workbook does not hold, which
                        // stays refused.)
                        self.blocks[block].push(Instruction::Literal(Literal::Error(
                            WorksheetErrorCode::Ref,
                        )));
                        return Ok(());
                    }
                    if text.eq_ignore_ascii_case("TRUE") {
                        self.blocks[block].push(Instruction::Literal(Literal::Logical(true)));
                    } else if text.eq_ignore_ascii_case("FALSE") {
                        self.blocks[block].push(Instruction::Literal(Literal::Logical(false)));
                    } else if let Some(slot) = self.local(&parameter_key(text)) {
                        self.blocks[block].push(Instruction::Local(slot));
                    } else if let Some(function) = without_prefix(text, "_xleta.") {
                        // dc-h9u: a function named where a LAMBDA value is taken, as a file
                        // stores it.
                        let target = catalogue()
                            .resolve_surface(Notation::Call, file_function(function))
                            .map(Target)
                            .map_err(|_| unknown(span, text.to_owned()))?;
                        self.blocks[block].push(Instruction::Function(target));
                    } else {
                        let scope = NameScope::Formula(self.name_context.lookup_sheet());
                        // A function typed bare where a LAMBDA value is taken (Excel 16.0.20527,
                        // UI30: `=MAP({1,2,3},SUM)`, `=BYROW(A1:B2,SUM)`) is the function's value
                        // when no defined name has the text; a VBA or add-in function of that
                        // name, or of any bare name, is #VALUE! there, and one named like a
                        // built-in shadows it. A file stores the value as `_xleta.SUM`, so a
                        // bare identifier of a loaded formula stays a name.
                        if !self.loaded {
                            match self.resolve(text, Position::Value, span) {
                                Resolved::Registered(_) => {
                                    self.blocks[block].push(Instruction::Literal(Literal::Error(
                                        WorksheetErrorCode::Value,
                                    )));
                                    return Ok(());
                                }
                                Resolved::BuiltIn(target) => {
                                    self.blocks[block].push(Instruction::Function(Target(target)));
                                    return Ok(());
                                }
                                Resolved::Table(table) => {
                                    debug_assert!(matches!(table, TableLookup::Found(_)));
                                }
                                _ => {}
                            }
                        }
                        self.defined_name(text, scope, span, block)?;
                    }
                }
            }
            SyntaxKind::QualifiedReferenceExpr | SyntaxKind::SheetSpan3DReferenceExpr => {
                if let Some((book, call)) = qualified_call(node) {
                    let sheet = qualified_sheet_call(node)
                        .and_then(|(sheet, _)| self.catalog.sheet_named(&sheet));
                    return self.qualified_user_call(book, call, sheet, span, block, tasks);
                }
                if !self.reference(node, block)? {
                    // dc-g9q: `[0]!Name`, a defined name qualified by the formula's own workbook,
                    // as a file stores one: the name as the formula's sheet sees it.
                    if let Some(name) = own_workbook_name(node) {
                        let scope = NameScope::Formula(self.name_context.lookup_sheet());
                        self.defined_name(name.text(), scope, span, block)?;
                        return Ok(());
                    }
                    if let Some((book, sheet, name)) = external_name_parts(node) {
                        return self.external_name(&book, sheet.as_deref(), &name, span, block);
                    }
                    let Some((sheet, name)) = qualified_name(node) else {
                        return Err(unsupported(span, "a sheet-qualified name", OWNER));
                    };
                    if name.text().contains('[') {
                        return Err(unsupported(
                            span,
                            "a sheet-qualified structured reference",
                            L3,
                        ));
                    }
                    let Some(sheet) = self.catalog.sheet_named(&sheet) else {
                        return Err(CompileError::Reference {
                            span,
                            error: BindError::UnknownSheet { name: sheet },
                        });
                    };
                    self.defined_name(name.text(), NameScope::Sheet(sheet), span, block)?;
                }
            }
            SyntaxKind::RangeExpr => {
                if !self.reference(node, block)? {
                    let mut operands = node.child_nodes();
                    let (Some(left), Some(right)) = (operands.next(), operands.next()) else {
                        return Err(syntax(span));
                    };
                    tasks.push(Task::Emit {
                        block,
                        instruction: Instruction::Range,
                    });
                    for operand in [right, left] {
                        self.lower_reference_operand(operand, block, place, tasks);
                    }
                }
            }
            SyntaxKind::TrimRangeExpr => {
                // dc-h9u: TRIMRANGE of the range, with both trim arguments 1 (`.:`, leading),
                // 2 (`:.`, trailing) or 3 (`.:.`, both).
                let operator: String = node
                    .children()
                    .iter()
                    .filter_map(|child| match child {
                        SyntaxChild::Token(token) => Some(token.text()),
                        SyntaxChild::Node(_) => None,
                    })
                    .collect();
                let mode = match operator.as_str() {
                    ".:" => 1.0,
                    ":." => 2.0,
                    ".:." => 3.0,
                    _ => return Err(syntax(span)),
                };
                let target = catalogue()
                    .resolve_surface(Notation::Call, "TRIMRANGE")
                    .map(Target)
                    .map_err(|_| syntax(span))?;
                tasks.push(Task::Emit {
                    block,
                    instruction: Instruction::Call {
                        target,
                        argc: 3,
                        retain: false,
                        synthetic: false,
                        admission: self.admission(place),
                    },
                });
                for _ in 0..2 {
                    tasks.push(Task::Emit {
                        block,
                        instruction: Instruction::Literal(Literal::Number(f64::to_bits(mode))),
                    });
                }
                if !self.reference(node, block)? {
                    let mut operands = node.child_nodes();
                    let (Some(left), Some(right)) = (operands.next(), operands.next()) else {
                        return Err(syntax(span));
                    };
                    tasks.push(Task::Emit {
                        block,
                        instruction: Instruction::Range,
                    });
                    for operand in [right, left] {
                        self.lower_reference_operand(operand, block, place, tasks);
                    }
                }
            }
            SyntaxKind::IntersectionExpr => {
                let mut operands = node.child_nodes();
                let (Some(left), Some(right)) = (operands.next(), operands.next()) else {
                    return Err(syntax(span));
                };
                tasks.push(Task::Emit {
                    block,
                    instruction: Instruction::Intersect,
                });
                for operand in [right, left] {
                    self.lower_reference_operand(operand, block, place, tasks);
                }
            }
            SyntaxKind::UnionExpr => {
                let operands: Vec<&SyntaxNode> = node.child_nodes().collect();
                tasks.push(Task::Emit {
                    block,
                    instruction: Instruction::Union(operands.len()),
                });
                for operand in operands.into_iter().rev() {
                    self.lower_reference_operand(operand, block, place, tasks);
                }
            }
            SyntaxKind::QuotedIdentifierExpr => {
                return Err(unsupported(span, "a quoted name", OWNER));
            }
            SyntaxKind::StringLiteralExpr => {
                self.blocks[block].push(Instruction::Literal(text(node)?))
            }
            SyntaxKind::ErrorLiteralExpr => {
                let code = only_token(node)
                    .and_then(Token::error_code)
                    .ok_or(syntax(span))?;
                self.blocks[block].push(Instruction::Literal(Literal::Error(code)));
            }
            SyntaxKind::ArrayLiteralExpr => {
                self.blocks[block].push(Instruction::Literal(array(node)?));
            }
            SyntaxKind::OmittedArgExpr => self.blocks[block].push(Instruction::Omitted),
            SyntaxKind::GroupingExpr => {
                let inner = node.child_nodes().next().ok_or(syntax(span))?;
                tasks.push(Task::Lower {
                    node: inner,
                    tail: false,
                    block,
                    place,
                });
            }
            SyntaxKind::BinaryExpr => {
                let (left, operator, right) = match node.children() {
                    [
                        SyntaxChild::Node(left),
                        SyntaxChild::Token(operator),
                        SyntaxChild::Node(right),
                    ] => (left, operator, right),
                    _ => return Err(syntax(span)),
                };
                let target = target(Notation::Infix, operator)?;
                let final_sum =
                    tail && matches!(operator.kind(), TokenKind::Plus | TokenKind::Minus);
                if final_sum {
                    tasks.push(Task::FinalSumCheck { block, operator });
                }
                tasks.push(Task::Emit {
                    block,
                    instruction: Instruction::Call {
                        target,
                        argc: 2,
                        retain: final_sum,
                        synthetic: false,
                        admission: self.admission(place),
                    },
                });
                for operand in [right, left] {
                    tasks.push(Task::Lower {
                        node: operand,
                        tail: false,
                        block,
                        place: place.operand(),
                    });
                }
            }
            SyntaxKind::PrefixExpr => {
                let (operator, operand) = match node.children() {
                    [SyntaxChild::Token(operator), SyntaxChild::Node(operand)] => {
                        (operator, operand)
                    }
                    _ => return Err(syntax(span)),
                };
                if operator.kind() == TokenKind::At {
                    tasks.push(Task::Emit {
                        block,
                        instruction: Instruction::Implicit,
                    });
                    tasks.push(Task::Lower {
                        node: operand,
                        tail: false,
                        block,
                        place: Place::Unknown,
                    });
                    return Ok(());
                }
                let target = target(Notation::Prefix, operator)?;
                tasks.push(Task::Emit {
                    block,
                    instruction: Instruction::Call {
                        target,
                        argc: 1,
                        retain: false,
                        synthetic: false,
                        admission: self.admission(place),
                    },
                });
                // Excel does not adjust a sum under a prefix sign: `=-((1+7*POWER(2,-52))-1)` is
                // -7*2^-52 (dc-b1l).
                tasks.push(Task::Lower {
                    node: operand,
                    tail: false,
                    block,
                    place: place.operand(),
                });
            }
            SyntaxKind::PostfixExpr => {
                let (operand, operator) = match node.children() {
                    [SyntaxChild::Node(operand), SyntaxChild::Token(operator)] => {
                        (operand, operator)
                    }
                    _ => return Err(syntax(span)),
                };
                if operator.kind() == TokenKind::Hash {
                    tasks.push(Task::Emit {
                        block,
                        instruction: Instruction::Spill,
                    });
                    tasks.push(Task::Lower {
                        node: operand,
                        tail: false,
                        block,
                        place: Place::Unknown,
                    });
                    return Ok(());
                }
                let target = target(Notation::Postfix, operator)?;
                tasks.push(Task::Emit {
                    block,
                    instruction: Instruction::Call {
                        target,
                        argc: 1,
                        retain: false,
                        synthetic: false,
                        admission: self.admission(place),
                    },
                });
                tasks.push(Task::Lower {
                    node: operand,
                    tail: false,
                    block,
                    place: place.operand(),
                });
            }
            SyntaxKind::CallExpr => self.call(node, tail, block, place, tasks)?,
            SyntaxKind::InvokeExpr => {
                let (callee, list) = match node.children() {
                    [SyntaxChild::Node(callee), SyntaxChild::Node(list)] => (callee, list),
                    _ => return Err(syntax(span)),
                };
                let arguments: Vec<&SyntaxNode> = list.child_nodes().collect();
                tasks.push(Task::Emit {
                    block,
                    instruction: Instruction::Invoke {
                        argc: arguments.len(),
                    },
                });
                for argument in arguments.into_iter().rev() {
                    tasks.push(Task::Lower {
                        node: argument,
                        tail: false,
                        block,
                        place: Place::Unknown,
                    });
                }
                tasks.push(Task::Lower {
                    node: callee,
                    tail: false,
                    block,
                    place: Place::Unknown,
                });
            }
            // The parser gives each of these a diagnostic, refused above, or never places them
            // under an expression.
            SyntaxKind::MissingExpr
            | SyntaxKind::UnsupportedExpr
            | SyntaxKind::FormulaRoot
            | SyntaxKind::ArgumentList => return Err(syntax(span)),
        }
        Ok(())
    }

    /// A call by name, lowered into block `block`: LET, LAMBDA and the file forms of `@` and `#`;
    /// a catalogue function, whose eager arguments are pushed in order before the call and whose
    /// lazy arguments each become a block of their own, run only when the function asks for it;
    /// or a call of the LAMBDA value a LET name, LAMBDA parameter or defined name holds.
    fn call<'t>(
        &mut self,
        node: &'t SyntaxNode,
        tail: bool,
        block: usize,
        place: Place,
        tasks: &mut Vec<Task<'t>>,
    ) -> Result<(), CompileError> {
        let span = node.span();
        let (name, list) = match node.children() {
            [SyntaxChild::Token(name), SyntaxChild::Node(list)] => (name, list),
            _ => return Err(syntax(span)),
        };
        let arguments: Vec<&SyntaxNode> = list.child_nodes().collect();
        let argc = arguments.len();
        let spelling = file_unprefixed(name.text());
        let prefixed = spelling.len() != name.text().len();
        match binding_call(spelling) {
            Some(BindingCall::Let) => {
                return self.let_call(span, &arguments, tail, block, place, tasks);
            }
            Some(BindingCall::Lambda) => {
                return self.lambda(span, &arguments, block, place, tasks);
            }
            None => {}
        }
        if let Some(operator) = catalogue().resolve_operator_alias(spelling) {
            let [operand] = arguments[..] else {
                return Err(syntax(span));
            };
            let instruction = match operator.notation() {
                Notation::Prefix => Instruction::Implicit,
                Notation::Postfix => Instruction::Spill,
                _ => return Err(syntax(span)),
            };
            tasks.push(Task::Emit { block, instruction });
            tasks.push(Task::Lower {
                node: operand,
                tail: false,
                block,
                place: Place::Unknown,
            });
            return Ok(());
        }
        // dc-h9u: a trim reference as a file stores it (Excel 16.0.20527 saves `C1:.C20` as
        // `_xlfn._TRO_TRAILING(C1:C20)`, `C1.:C20` as `_TRO_LEADING` and `C1.:.C20` as
        // `_TRO_ALL`): TRIMRANGE of the range with both trim arguments 2, 1 or 3.
        let trim = [
            ("_TRO_LEADING", 1.0),
            ("_TRO_TRAILING", 2.0),
            ("_TRO_ALL", 3.0),
        ]
        .into_iter()
        .find(|(name, _)| spelling.eq_ignore_ascii_case(name))
        .map(|(_, mode)| mode);
        if let (true, Some(mode)) = (prefixed, trim) {
            let [operand] = arguments[..] else {
                return Err(syntax(span));
            };
            let target = catalogue()
                .resolve_surface(Notation::Call, "TRIMRANGE")
                .map(Target)
                .map_err(|_| syntax(span))?;
            tasks.push(Task::Emit {
                block,
                instruction: Instruction::Call {
                    target,
                    argc: 3,
                    retain: false,
                    synthetic: false,
                    admission: self.admission(place),
                },
            });
            for _ in 0..2 {
                tasks.push(Task::Emit {
                    block,
                    instruction: Instruction::Literal(Literal::Number(f64::to_bits(mode))),
                });
            }
            tasks.push(Task::Lower {
                node: operand,
                tail: false,
                block,
                place: place.argument("TRIMRANGE", 0, Some(ParameterClass::Reference), false),
            });
            return Ok(());
        }
        let resolved = self.resolve(name.text(), Position::Call, name.span());
        let target = match resolved {
            Resolved::BuiltIn(target) => Target(target),
            other => {
                return self.named_call(
                    name,
                    unknown(name.span(), name.text().to_owned()),
                    other,
                    &arguments,
                    block,
                    tasks,
                );
            }
        };
        if self.loaded && unknown_on_load(name.text(), target) {
            self.blocks[block].push(Instruction::Literal(Literal::Error(
                WorksheetErrorCode::Name,
            )));
            return Ok(());
        }
        // Excel's entry check, which [`check_entry`] made on the whole formula before lowering
        // began; asked again here for a defined name's formula, which it does not read.
        let forms: Vec<calc_func::OperandForm> = arguments
            .iter()
            .map(|argument| super::entry::operand_form(argument, self.loaded))
            .collect();
        if let Err(rule) = target.0.check_entry(&forms) {
            return Err(CompileError::Rejected {
                span,
                rule: Rejection::Function {
                    function: target.0.id(),
                    rule,
                },
            });
        }
        let spelled = target.0.spelling();
        if let calc_func::EntryKind::HostRoute { provider, route } = target.0.kind()
            && (route != calc_func::HostRouteKind::Rtd || !prefixed)
        {
            // A change of the host route rebinds this plan.
            self.watch_callable(Some(provider), spelled);
            if self
                .functions
                .is_some_and(|f| f.contains_for(Some(provider), spelled))
            {
                self.user_call(
                    spelled,
                    Some(UserFunctionSource {
                        book: ExternalBookRef::Number(0),
                        target: Some(provider.to_owned()),
                    }),
                    &arguments,
                    block,
                    tasks,
                );
                return Ok(());
            }
        }
        // A host route without its provider, or a deferred built-in, is a typed refusal.
        let target = call_target(name)?;
        let demand = target.0.argument_demand();
        let lazy = demand.is_lazy();
        let value_place = place.argument(spelled, 0, Some(ParameterClass::Value), lazy);
        let admission = self.admission(value_place);
        let mut three_d: Vec<Option<Vec<Instruction>>> = Vec::with_capacity(argc);
        for (index, argument) in arguments.iter().enumerate() {
            match self
                .three_d_argument(argument, target, index, !lazy)
                .transpose()?
            {
                Some(ThreeD::CallIs(code)) => {
                    self.blocks[block].push(Instruction::Literal(Literal::Error(code)));
                    return Ok(());
                }
                Some(ThreeD::Arguments(instructions)) => three_d.push(Some(instructions)),
                None => three_d.push(None),
            }
        }
        if !lazy {
            // Excel writes `@` before ROW and COLUMN in a legacy formula, except where an array is
            // taken (dc-b1l, measured in Excel 16.0.20430 on 2026-10-25: `Range.Formula`
            // `=SUM(ROW(B3:B5))` reads back as `=SUM(@ROW(B3:B5))`, 3, and
            // `=MATCH(4,ROW(B3:B5),0)` as `=MATCH(4,@ROW(B3:B5),0)`, #N/A; `=SUMPRODUCT(ROW(B3:B5))`
            // and `=ROWS(ROW(B3:B5))` keep no `@`, 12 and 3). FILTERXML behaves
            // likewise: catalogue dcuar_text!D1293 captures SUM(FILTERXML(...))
            // as the first matching node, rather than the sum of all nodes.
            if self.form == FormulaForm::Legacy
                && place != Place::Array
                && target.0.traits().intersect_legacy_result
            {
                tasks.push(Task::Emit {
                    block,
                    instruction: Instruction::Implicit,
                });
            }
            // A 3-D reference spread over one argument per sheet adds arguments.
            let spread: usize = three_d
                .iter()
                .flatten()
                .map(|instructions| instructions.len() - 1)
                .sum();
            tasks.push(Task::Emit {
                block,
                instruction: Instruction::Call {
                    target,
                    argc: argc + spread,
                    retain: false,
                    synthetic: false,
                    admission,
                },
            });
            for (index, argument) in arguments.into_iter().enumerate().rev() {
                if let Some(instructions) = three_d[index].take() {
                    for instruction in instructions.into_iter().rev() {
                        tasks.push(Task::Emit { block, instruction });
                    }
                    continue;
                }
                if self.form == FormulaForm::Legacy
                    && match target.0.legacy_place(index) {
                        Some(
                            calc_func::LegacyPlace::IntersectReferenceAlways
                            | calc_func::LegacyPlace::AggregateAlwaysIntersectReference,
                        ) => true,
                        Some(
                            calc_func::LegacyPlace::IntersectReference
                            | calc_func::LegacyPlace::AggregateIntersectReference,
                        ) => place != Place::Array,
                        _ => false,
                    }
                {
                    tasks.push(Task::Emit {
                        block,
                        instruction: Instruction::ImplicitReference,
                    });
                }
                tasks.push(Task::Lower {
                    node: argument,
                    tail: false,
                    block,
                    place: place.argument(spelled, index, target.0.parameter_class(index), false),
                });
            }
            return Ok(());
        }
        let first_block = self.blocks.len();
        self.blocks.resize_with(first_block + argc, Vec::new);
        tasks.push(Task::Emit {
            block,
            instruction: Instruction::LazyCall {
                target,
                first_block,
                argc,
                admission,
                lifted_admission: self.admission(place),
            },
        });
        for (index, argument) in arguments.into_iter().enumerate().rev() {
            if let Some(instructions) = three_d[index].take() {
                for instruction in instructions.into_iter().rev() {
                    tasks.push(Task::Emit {
                        block: first_block + index,
                        instruction,
                    });
                }
                continue;
            }
            tasks.push(Task::Lower {
                node: argument,
                tail: false,
                block: first_block + index,
                place: place.argument(spelled, index, target.0.parameter_class(index), true),
            });
        }
        Ok(())
    }

    /// A call written with a name that is no catalogue function: the LAMBDA value of a LET name,
    /// LAMBDA parameter or defined name, called with `arguments`. A built-in function the
    /// catalogue does not hold yet is refused as `unknown`, and so is a call of a name that no
    /// defined name holds.
    fn watch_callable(&mut self, provider: Option<&str>, name: &str) {
        let proof = crate::CallableBindingProof {
            provider: provider.map(str::to_owned),
            name: calc_func::value::fold_workbook_name(name),
            bound: self.functions.and_then(|f| f.bind(provider, name)),
        };
        if !self.callable_bindings.contains(&proof) {
            self.callable_bindings.push(proof);
        }
    }
    fn named_call<'t>(
        &mut self,
        name: &Token,
        unknown: CompileError,
        resolved: Resolved,
        arguments: &[&'t SyntaxNode],
        block: usize,
        tasks: &mut Vec<Task<'t>>,
    ) -> Result<(), CompileError> {
        let spelling = file_unprefixed(name.text());
        if spelling.len() != name.text().len() {
            return Err(unknown);
        }
        if let Resolved::Reference = resolved {
            let (operand, this_row) = self
                .sites
                .remove(&name.span())
                .expect("the resolved reference");
            // Use the existing reference invocation route: it produces #REF!
            // without looking up a same-spelled VBA procedure or dereferencing
            // a cell to seek a closure. Keep the ordinary argument lowering.
            let operand = operand.map_err(|error| CompileError::Reference {
                span: name.span(),
                error,
            })?;
            self.push_reference(operand, this_row, block);
        } else if let Resolved::Local(slot) = resolved {
            self.blocks[block].push(Instruction::Local(slot));
        } else {
            match resolved {
                Resolved::Defined(NameDefinition::Formula(definition)) => {
                    let name_block = self.name_block(name.text(), &definition, name.span())?;
                    self.blocks[block].push(Instruction::Name(name_block));
                }
                // No defined name: Excel's #NAME? when the workbook can hold no function of its
                // own (Excel 16.0, 2026-10-23: `=FOO(1)` is #NAME? and `=IFERROR(FOO(1),3)` is
                // 3); else a VBA or add-in function might have the name, and the call is refused.
                // The arguments are not evaluated: Excel's #NAME? comes from the name alone.
                Resolved::Registered(handle) => {
                    debug_assert_eq!(
                        self.functions
                            .and_then(|functions| functions.bind(None, spelling)),
                        Some(handle)
                    );
                    self.user_call(spelling, None, arguments, block, tasks);
                    return Ok(());
                }
                Resolved::Unknown
                    if self.complete_registry || self.catalog.unknown_calls_are_names() =>
                {
                    self.blocks[block].push(Instruction::Literal(Literal::Error(
                        WorksheetErrorCode::Name,
                    )));
                    return Ok(());
                }
                _ => return Err(unknown),
            }
        }
        tasks.push(Task::Emit {
            block,
            instruction: Instruction::Invoke {
                argc: arguments.len(),
            },
        });
        for argument in arguments.iter().rev() {
            tasks.push(Task::Lower {
                node: argument,
                tail: false,
                block,
                place: Place::Unknown,
            });
        }
        Ok(())
    }

    fn user_call<'t>(
        &mut self,
        name: &str,
        source: Option<UserFunctionSource>,
        arguments: &[&'t SyntaxNode],
        block: usize,
        tasks: &mut Vec<Task<'t>>,
    ) {
        tasks.push(Task::Emit {
            block,
            instruction: Instruction::UserCall {
                binding: self.functions.and_then(|functions| {
                    functions.bind(source.as_ref().and_then(|s| s.target.as_deref()), name)
                }),
                name: calc_func::value::fold_workbook_name(name),
                source,
                argc: arguments.len(),
            },
        });
        for argument in arguments.iter().rev() {
            tasks.push(Task::Lower {
                node: argument,
                tail: false,
                block,
                place: Place::Unknown,
            });
        }
    }

    fn qualified_user_call<'t>(
        &mut self,
        book: ExternalBookRef,
        call: &'t SyntaxNode,
        sheet: Option<SheetId>,
        qualified_span: TextSpan,
        block: usize,
        tasks: &mut Vec<Task<'t>>,
    ) -> Result<(), CompileError> {
        let span = call.span();
        let name = call.tokens().into_iter().next().ok_or(syntax(span))?;
        // A name edit replaces the complete qualifier and callee, leaving its
        // argument list in place. Recording only the token would duplicate the
        // sheet qualifier when the existing rewrite supplies a qualified name.
        let name_span = TextSpan::covering(qualified_span, name.span());
        let list = call.child_nodes().next().ok_or(syntax(span))?;
        let arguments: Vec<_> = list.child_nodes().collect();
        let own = book == ExternalBookRef::Number(0)
            || match &book {
                ExternalBookRef::File(name) => self
                    .catalog
                    .workbook_name()
                    .is_some_and(|held| calc_func::value::workbook_names_equal(held, name)),
                _ => false,
            };
        if sheet.is_some() && (own || self.catalog.external_function_target(&book).is_some()) {
            return Err(unsupported(
                span,
                "a call qualifier naming both a sheet and a workbook provider",
                "dc-hsa5.6",
            ));
        }
        let target = if own || sheet.is_some() {
            None
        } else {
            Some(
                self.catalog
                    .external_function_target(&book)
                    .ok_or_else(|| {
                        unsupported(
                            span,
                            "a user function whose provider link is unresolved",
                            "dc-snz.9",
                        )
                    })?,
            )
        };
        if own || sheet.is_some() {
            let scope = sheet
                .map(NameScope::Sheet)
                .unwrap_or_else(|| NameScope::Formula(self.name_context.lookup_sheet()));
            match self.lookup_name(name.text(), scope, name_span) {
                NameDefinition::Formula(definition) => {
                    let name_block = self.name_block(name.text(), &definition, name_span)?;
                    self.blocks[block].push(Instruction::Name(name_block));
                    tasks.push(Task::Emit {
                        block,
                        instruction: Instruction::Invoke {
                            argc: arguments.len(),
                        },
                    });
                    for argument in arguments.iter().rev() {
                        tasks.push(Task::Lower {
                            node: argument,
                            tail: false,
                            block,
                            place: Place::Unknown,
                        });
                    }
                    return Ok(());
                }
                NameDefinition::NotModelled => {
                    return Err(unsupported(
                        span,
                        "a qualified function name",
                        if sheet.is_some() {
                            "dc-hsa5.6"
                        } else {
                            "dc-snz.9"
                        },
                    ));
                }
                NameDefinition::Unknown if sheet.is_some() => {
                    return Err(unsupported(
                        span,
                        "a sheet-qualified callee without a defined name",
                        "dc-hsa5.6",
                    ));
                }
                NameDefinition::Unknown => {}
            }
        }
        if let Some(addin) = target
            .as_deref()
            .and_then(|library| catalogue().resolve_addin(library, name.text()))
        {
            tasks.push(Task::Emit {
                block,
                instruction: Instruction::Call {
                    target: Target(addin),
                    argc: arguments.len(),
                    retain: false,
                    synthetic: false,
                    admission: self.admission(Place::Unknown),
                },
            });
            for argument in arguments.iter().rev() {
                tasks.push(Task::Lower {
                    node: argument,
                    tail: false,
                    block,
                    place: Place::Unknown,
                });
            }
            return Ok(());
        }
        self.watch_callable(target.as_deref(), name.text());
        if self
            .functions
            .is_some_and(|f| f.contains_for(target.as_deref(), name.text()))
        {
            self.user_call(
                name.text(),
                Some(UserFunctionSource { book, target }),
                &arguments,
                block,
                tasks,
            );
        } else if self.complete_registry {
            self.blocks[block].push(Instruction::Literal(Literal::Error(
                WorksheetErrorCode::Name,
            )));
        } else if self.functions.is_some() {
            // A failed qualified binding must remain eligible for registration changes.
            return Err(unknown(name.span(), name.text().to_owned()));
        } else {
            return Err(unsupported(
                span,
                "a workbook-qualified user function without a registry",
                "dc-snz.6",
            ));
        }
        Ok(())
    }

    /// `LET(name1, value1, [name2, value2, ...], calculation)`: each value is stored in a local
    /// of its own, in order, its name in scope for the values after it and for the calculation.
    /// Every value runs, whether or not a later expression uses it.
    fn let_call<'t>(
        &mut self,
        span: TextSpan,
        arguments: &[&'t SyntaxNode],
        tail: bool,
        block: usize,
        place: Place,
        tasks: &mut Vec<Task<'t>>,
    ) -> Result<(), CompileError> {
        let argc = arguments.len();
        if argc < 3 || argc.is_multiple_of(2) {
            return Err(grammar(span, GrammarRule::BindingArguments));
        }
        // Where a legacy formula's LET intersects is not measured: refused there.
        let _ = place;
        let inner = Place::Unknown;
        let pairs = argc / 2;
        tasks.push(Task::Unbind(pairs));
        // LET's result is not adjusted: `=LET(a,1,(1+7*POWER(2,-52))-a)` is 7*2^-52 (dc-b1l).
        let _ = tail;
        tasks.push(Task::Lower {
            node: arguments[argc - 1],
            tail: false,
            block,
            place: inner,
        });
        for pair in (0..pairs).rev() {
            let name = self.declared_name(arguments[2 * pair])?;
            if name.1 {
                return Err(syntax(arguments[2 * pair].span()));
            }
            tasks.push(Task::Declare {
                block,
                name: name.0,
            });
            tasks.push(Task::Lower {
                node: arguments[2 * pair + 1],
                tail: false,
                block,
                place: inner,
            });
        }
        Ok(())
    }

    /// `LAMBDA([parameter1, ...], calculation)`: the LAMBDA value, its body lowered into a block
    /// of its own with its parameters in scope after the names it captures.
    fn lambda<'t>(
        &mut self,
        span: TextSpan,
        arguments: &[&'t SyntaxNode],
        block: usize,
        place: Place,
        tasks: &mut Vec<Task<'t>>,
    ) -> Result<(), CompileError> {
        let Some((body, parameters)) = arguments.split_last() else {
            return Err(grammar(span, GrammarRule::BindingArguments));
        };
        if body.kind() == SyntaxKind::OmittedArgExpr {
            return Err(syntax(body.span()));
        }
        let mut names = Vec::with_capacity(parameters.len());
        let mut required = 0;
        for parameter in parameters {
            let (name, optional) = self.declared_name(parameter)?;
            if names.contains(&name) {
                return Err(grammar(
                    parameter.span(),
                    GrammarRule::DuplicateLambdaParameter,
                ));
            }
            if optional {
                // A parameter after an optional one is optional too.
            } else if required == names.len() {
                required += 1;
            } else {
                return Err(syntax(parameter.span()));
            }
            names.push(name);
        }
        let _ = place;
        let body_block = self.blocks.len();
        self.blocks.push(Vec::new());
        let captured = self.depth();
        self.blocks[block].push(Instruction::Lambda {
            block: body_block,
            params: u32::try_from(names.len()).expect("at most 253 parameters"),
            required: u32::try_from(required).expect("at most 253 parameters"),
            captured,
        });
        tasks.push(Task::Unbind(names.len()));
        tasks.push(Task::Lower {
            node: body,
            tail: false,
            block: body_block,
            place: Place::Unknown,
        });
        for name in names.into_iter().rev() {
            tasks.push(Task::Bind { name });
        }
        Ok(())
    }

    /// The name a LET name or LAMBDA parameter node declares, upper case and without its file
    /// prefix, and whether it is optional (`[y]`, or `_xlop.y` in a file). A node that is no
    /// name, or that binding read as a reference, is refused as syntax Excel does not accept.
    fn declared_name(&mut self, node: &SyntaxNode) -> Result<(String, bool), CompileError> {
        let span = node.span();
        let refused = syntax(span);
        if node.kind() != SyntaxKind::IdentifierExpr {
            return Err(refused);
        }
        let text = only_token(node).map(Token::text).ok_or(refused.clone())?;
        if text.starts_with('[') {
            // An optional parameter, which a formula inside a table may also read as a
            // structured reference: as a parameter it is a name.
            self.sites.remove(&span);
        } else if self.sites.contains_key(&span) {
            return Err(refused);
        }
        if text.eq_ignore_ascii_case("TRUE") || text.eq_ignore_ascii_case("FALSE") {
            return Err(refused);
        }
        let (text, optional) = match text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
            Some(inner) => (inner, true),
            None => match without_prefix(text, "_xlop.") {
                Some(inner) => (inner, true),
                None => (text, false),
            },
        };
        let valid = text
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '\\')
            && text
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.');
        if !valid {
            return Err(refused);
        }
        Ok((parameter_key(text), optional))
    }

    fn lookup_name(&mut self, name: &str, scope: NameScope, span: TextSpan) -> NameDefinition {
        let proof = self
            .catalog
            .name_binding(name, scope, self.name_context.definition);
        self.last_name = proof.as_ref().and_then(|proof| proof.resolved);
        if let Some(uses) = &mut self.name_uses {
            uses.push(crate::reference::NameUse {
                span,
                context: self.name_context,
                resolved: self.last_name,
            });
        }
        self.last_name_sheet = self
            .last_name
            .and_then(|name| match name.scope {
                calc_func::value::NameScope::Sheet(sheet) => Some(sheet),
                calc_func::value::NameScope::Workbook => self.name_context.definition_sheet,
            })
            .or_else(|| {
                if proof.is_none() {
                    match scope {
                        NameScope::Sheet(sheet) => Some(sheet),
                        _ => None,
                    }
                } else {
                    None
                }
            });
        if let Some(mut proof) = proof {
            proof.parent = self.name_context.definition;
            if !self.name_bindings.contains(&proof) {
                self.name_bindings.push(proof);
            }
        }
        self.catalog.defined_name(name, scope)
    }

    /// Pushes onto block `block` the value of the defined name `name` seen from `scope`: its
    /// formula's block, #NAME? when the workbook holds no such name, or the refusal of a name the
    /// catalogue cannot say.
    /// A defined name of another workbook (`[2]!Rate`, `'[2]Sheet'!Rate`, dc-9cyc, dc-x8v4): the
    /// reference the workbook's link to that book lists for it, read from the link's cache like
    /// any other external reference. A name the link does not list, or one that refers to
    /// anything but one reference, stays refused.
    fn external_name(
        &mut self,
        book: &ExternalBookRef,
        sheet: Option<&str>,
        name: &str,
        span: TextSpan,
        block: usize,
    ) -> Result<(), CompileError> {
        let refused = || unsupported(span, "a defined name of another workbook", OWNER);
        let Some(refers) = self.catalog.external_name(book, sheet, name) else {
            return Err(refused());
        };
        let refers = refers.strip_prefix('=').unwrap_or(&refers);
        if refers.eq_ignore_ascii_case("#REF!") {
            self.blocks[block].push(Instruction::Literal(Literal::Error(WorksheetErrorCode::Ref)));
            return Ok(());
        }
        let spelled = match book {
            ExternalBookRef::Number(number) => number.to_string(),
            ExternalBookRef::File(file) => file.clone(),
        };
        let text = match refers.strip_prefix('\'') {
            Some(rest) => format!("='[{spelled}]{rest}"),
            None => format!("=[{spelled}]{refers}"),
        };
        let tree = parse_formula(&text);
        if !tree.diagnostics().is_empty() {
            return Err(refused());
        }
        let Some(expression) = tree.expression() else {
            return Err(refused());
        };
        let a1 = CellKey::new(self.name_context.lookup_sheet(), 1, 1)
            .expect("A1 lies on the grid");
        match external_operand(expression, ReferenceNotation::A1, FormulaAnchor::new(a1)) {
            Some(operand) => {
                self.blocks[block].push(Instruction::External(self.externals.len()));
                self.externals.push(operand);
                Ok(())
            }
            None => Err(refused()),
        }
    }

    fn defined_name(
        &mut self,
        name: &str,
        scope: NameScope,
        span: TextSpan,
        block: usize,
    ) -> Result<(), CompileError> {
        match self.lookup_name(name, scope, span) {
            NameDefinition::Formula(definition) => {
                let name_block = self.name_block(name, &definition, span)?;
                self.blocks[block].push(Instruction::Name(name_block));
            }
            NameDefinition::Unknown => {
                // A table's name alone names its data rows (Excel 16.0, 2026-10-23:
                // `=ROWS(Sales)` is 3 and `=SUM(Sales)` sums the data rows; a file stores it as
                // `Sales[]`). Tables and defined names share one namespace.
                let table = match scope {
                    NameScope::Formula(_) => self.catalog.table(name),
                    NameScope::Sheet(_) => TableLookup::Unknown,
                };
                match table {
                    TableLookup::Found(table) => {
                        self.push_reference(table.data_body(), false, block);
                    }
                    TableLookup::Unknown | TableLookup::NotModelled => {
                        self.blocks[block].push(Instruction::Literal(Literal::Error(
                            WorksheetErrorCode::Name,
                        )));
                    }
                }
            }
            NameDefinition::NotModelled => {
                return Err(unsupported(span, "a defined name", NAMES_OWNER));
            }
        }
        Ok(())
    }

    /// The block of the defined name `name` whose formula is `definition`, compiled into the plan
    /// the first time the formula uses it. Its references are bound as a file stores them,
    /// relative to cell A1 of the formula's sheet, and placed at the formula's cell with each
    /// relative offset wrapping around the grid's edge
    /// ([`ReferenceOperand::instantiate_wrapped`]): Excel 16.0 (2026-10-23) stores `RC[-1]` as
    /// `Nm!XFD1` and reads `=LeftCell` in C3 as B3 and in A1 as XFD1.
    /// It sees no LET name or LAMBDA parameter of the formula that uses it. A name that uses
    /// itself is given its own block, so a LAMBDA a name holds may call that name.
    fn name_block(
        &mut self,
        name: &str,
        definition: &str,
        span: TextSpan,
    ) -> Result<NameTarget, CompileError> {
        let key = (
            calc_func::value::fold_workbook_name(name),
            definition.to_owned(),
            self.last_name.map(|name| name.id),
            self.last_name_sheet,
        );
        if let Some(block) = self.names.get(&key) {
            return Ok(*block);
        }
        if self.nesting >= MAX_NAME_NESTING {
            return Err(unsupported(
                span,
                "defined names nested more deeply than this package lowers",
                OWNER,
            ));
        }
        let bound = self.last_name;
        let definition_sheet = self.last_name_sheet;
        let text = if definition.starts_with('=') {
            definition.to_owned()
        } else {
            format!("={definition}")
        };
        let tree = parse_formula(&text);
        let refused = || unsupported(span, "a defined name whose formula does not compile", OWNER);
        if !tree.diagnostics().is_empty() {
            return Err(refused());
        }
        let Some(expression) = tree.expression() else {
            return Err(refused());
        };
        let a1 = CellKey::new(definition_sheet.unwrap_or(self.anchor.sheet()), 1, 1)
            .expect("A1 lies on the grid");
        let sites = sites(
            &tree,
            ReferenceNotation::A1,
            FormulaAnchor::new(a1),
            self.catalog,
        );
        let block = self.blocks.len();
        self.blocks.push(Vec::new());
        let target = NameTarget {
            block,
            binding: bound,
            definition_sheet,
        };
        self.names.insert(key, target);
        let child_context = self
            .name_context
            .child(target.binding.map(|name| name.id), definition_sheet);
        let name_context = std::mem::replace(&mut self.name_context, child_context);
        let floor = std::mem::replace(&mut self.floor, self.scope.len());
        let in_name = std::mem::replace(&mut self.in_name, true);
        self.nesting += 1;
        let lowered = self.lower_root(expression, sites, block, false, Place::Unknown);
        self.nesting -= 1;
        self.in_name = in_name;
        self.floor = floor;
        self.name_context = name_context;
        lowered.map_err(|error| match error {
            CompileError::NotModelled { .. } => error,
            _ => refused(),
        })?;
        Ok(target)
    }

    /// Pushes onto block `block` the reference `node` binds to, when binding gave it a site;
    /// `false` when it is not a reference. A structured reference's `#This Row` is followed by
    /// implicit intersection. A 3-D reference here, where no function takes it, is #REF!: Excel
    /// 16.0 (2026-10-23) gives `=Ja:Ma!A1`, `=Ja:Ma!A1+1`, `=ABS(Ja:Ma!A1)` and
    /// `=IFERROR(Ja:Ma!A1,"e")` #REF!, #REF!, #REF! and "e".
    fn reference(&mut self, node: &SyntaxNode, block: usize) -> Result<bool, CompileError> {
        let span = node.span();
        match self.sites.remove(&span) {
            None => Ok(false),
            // dc-ylr: a reference to another workbook is a reference to the sheet the workbook's
            // link to that book caches, placed when the formula runs. A defined name's is bound
            // as a file stores it, relative to A1 of the formula's sheet, and wraps when placed.
            Some((Err(BindError::ExternalWorkbook), _)) => {
                let (notation, anchor) = if self.in_name {
                    let a1 = CellKey::new(self.name_context.lookup_sheet(), 1, 1)
                        .expect("A1 lies on the grid");
                    (ReferenceNotation::A1, FormulaAnchor::new(a1))
                } else {
                    (self.notation, self.anchor)
                };
                match external_operand(node, notation, anchor) {
                    Some(mut operand) => {
                        operand.wrapped = self.in_name;
                        self.blocks[block].push(Instruction::External(self.externals.len()));
                        self.externals.push(operand);
                        Ok(true)
                    }
                    None => Err(CompileError::Reference {
                        span,
                        error: BindError::ExternalWorkbook,
                    }),
                }
            }
            Some((Err(error), _)) => Err(CompileError::Reference { span, error }),
            Some((Ok(operand), _)) if matches!(operand.sheet, SheetTarget::Span { .. }) => {
                self.blocks[block].push(Instruction::Literal(Literal::Error(
                    WorksheetErrorCode::Ref,
                )));
                Ok(true)
            }
            Some((Ok(operand), this_row)) => {
                self.push_reference(operand, this_row, block);
                Ok(true)
            }
        }
    }

    /// Pushes the task that lowers `operand`, an operand of the range, intersection or union
    /// operator, into block `block`. A 3-D reference there is #VALUE! (Excel 16.0, 2026-10-23:
    /// `=SUM(Ja:Ma!A1:A2 Ja:Ma!A1)` is #VALUE!), where elsewhere it is #REF!.
    fn lower_reference_operand<'t>(
        &mut self,
        operand: &'t SyntaxNode,
        block: usize,
        place: Place,
        tasks: &mut Vec<Task<'t>>,
    ) {
        let span = operand.span();
        if let Some((Ok(bound), _)) = self.sites.get(&span)
            && matches!(bound.sheet, SheetTarget::Span { .. })
        {
            self.sites.remove(&span);
            tasks.push(Task::Emit {
                block,
                instruction: Instruction::Literal(Literal::Error(WorksheetErrorCode::Value)),
            });
            return;
        }
        tasks.push(Task::Lower {
            node: operand,
            tail: false,
            block,
            place,
        });
    }

    /// Pushes `operand` onto block `block`, followed by implicit intersection with `this_row`.
    fn push_reference(&mut self, mut operand: ReferenceOperand, this_row: bool, block: usize) {
        if self.in_name
            && operand.sheet == SheetTarget::Anchor
            && let Some(sheet) = self.name_context.definition_sheet
        {
            operand.sheet = SheetTarget::Sheet(sheet);
        }
        self.blocks[block].push(Instruction::Reference(self.references.len()));
        self.references.push(operand);
        self.wrapped.push(self.in_name);
        if this_row {
            self.blocks[block].push(Instruction::Implicit);
        }
    }

    /// When argument `index` of a call of `spelling` is a 3-D reference, what it becomes there:
    /// the reference across its sheets for a function that takes one, the error a function gives
    /// one it does not take, or the refusal of a function whose handling of it is not measured.
    /// `None` when the argument is no 3-D reference.
    fn three_d_argument(
        &mut self,
        node: &SyntaxNode,
        target: Target,
        index: usize,
        may_spread: bool,
    ) -> Option<Result<ThreeD, CompileError>> {
        let span = node.span();
        let (Ok(operand), _) = self.sites.get(&span)? else {
            return None;
        };
        let SheetTarget::Span { first, last } = operand.sheet else {
            return None;
        };
        let shape = operand.shape;
        self.sites.remove(&span);
        let place = target.0.three_d_place(index);
        if place == calc_func::ThreeDPlace::ValueError {
            return Some(Ok(ThreeD::CallIs(WorksheetErrorCode::Value)));
        }
        if place == calc_func::ThreeDPlace::ReferenceError {
            return Some(Ok(ThreeD::Arguments(vec![Instruction::Literal(
                Literal::Error(WorksheetErrorCode::Ref),
            )])));
        }
        if place == calc_func::ThreeDPlace::Unknown {
            return Some(Err(unsupported(
                span,
                "a 3-D reference given to a function whose handling of it is not measured",
                L3,
            )));
        }
        let Some(sheets) = self.catalog.sheet_span(first, last) else {
            return Some(Err(CompileError::Reference {
                span,
                error: BindError::SheetSpan,
            }));
        };
        let start = self.references.len();
        for sheet in &sheets {
            self.references.push(ReferenceOperand {
                sheet: SheetTarget::Sheet(*sheet),
                shape,
            });
            self.wrapped.push(self.in_name);
        }
        // A function that reads every argument alike (SUM's numbers, CONCAT's texts, NPV's values
        // after its rate) reads a 3-D reference there as one argument per sheet, in sheet order;
        // any other is given the reference across its sheets.
        let spreads = may_spread && place == calc_func::ThreeDPlace::Spread;
        Some(Ok(ThreeD::Arguments(if spreads {
            (start..start + sheets.len())
                .map(Instruction::Reference)
                .collect()
        } else {
            vec![Instruction::Reference3D {
                first: start,
                count: sheets.len(),
            }]
        })))
    }
}

/// What a 3-D reference given as an argument becomes.
enum ThreeD {
    /// The instructions that push the argument, or the arguments it spreads into.
    Arguments(Vec<Instruction>),
    /// The call's result is this error, whatever its other arguments.
    CallIs(WorksheetErrorCode),
}

/// The key a LET name or LAMBDA parameter is matched by: without a leading `_xlpm.` (any ASCII
/// case), under the simple Unicode case fold Excel matches parameters by (dc-cpr:
/// `=LET(Größe,21,GRÖßE*2)` finds the parameter, `GRÖSSE` does not and is read as a defined
/// name; calc-value's `parameter_names_equal`). A defined name's block is keyed by the full fold,
/// as defined names compare.
pub(super) fn parameter_key(text: &str) -> String {
    calc_func::value::fold_parameter_name(without_prefix(text, "_xlpm.").unwrap_or(text))
}

/// An array constant: rows of number, text, logical and error constants (a number may carry a
/// sign), separated by `,` within a row and `;` between rows, every row as long as the first.
/// Anything else inside the braces is syntax Excel does not accept.
fn array(node: &SyntaxNode) -> Result<Literal, CompileError> {
    let span = node.span();
    let mut rows: Vec<Vec<Literal>> = vec![Vec::new()];
    for child in node.children() {
        match child {
            SyntaxChild::Token(token) => match token.kind() {
                TokenKind::Semicolon => rows.push(Vec::new()),
                TokenKind::Comma | TokenKind::LBrace | TokenKind::RBrace => {}
                _ => return Err(syntax(token.span())),
            },
            SyntaxChild::Node(element) => {
                let literal = array_element(element)?;
                rows.last_mut().expect("a row").push(literal);
            }
        }
    }
    let cols = rows[0].len();
    if cols == 0 || rows.iter().any(|row| row.len() != cols) {
        return Err(syntax(span));
    }
    let count = rows.len();
    Ok(Literal::Array {
        rows: count,
        cols,
        cells: rows.into_iter().flatten().collect(),
    })
}

fn array_element(node: &SyntaxNode) -> Result<Literal, CompileError> {
    let span = node.span();
    match node.kind() {
        SyntaxKind::NumberLiteralExpr => number(node),
        SyntaxKind::StringLiteralExpr => text(node),
        SyntaxKind::ErrorLiteralExpr => only_token(node)
            .and_then(Token::error_code)
            .map(Literal::Error)
            .ok_or(syntax(span)),
        SyntaxKind::IdentifierExpr => {
            let text = only_token(node).map(Token::text).unwrap_or_default();
            if text.eq_ignore_ascii_case("TRUE") {
                Ok(Literal::Logical(true))
            } else if text.eq_ignore_ascii_case("FALSE") {
                Ok(Literal::Logical(false))
            } else {
                Err(syntax(span))
            }
        }
        SyntaxKind::PrefixExpr => match node.children() {
            [SyntaxChild::Token(sign), SyntaxChild::Node(operand)]
                if operand.kind() == SyntaxKind::NumberLiteralExpr
                    && matches!(sign.kind(), TokenKind::Minus | TokenKind::Plus) =>
            {
                let Literal::Number(bits) = number(operand)? else {
                    unreachable!("number gives a number literal")
                };
                let value = f64::from_bits(bits);
                let signed = if sign.kind() == TokenKind::Minus && value != 0.0 {
                    -value
                } else {
                    value
                };
                Ok(Literal::Number(signed.to_bits()))
            }
            _ => Err(syntax(span)),
        },
        _ => Err(syntax(span)),
    }
}

fn unsupported(span: TextSpan, construct: &'static str, owner: &'static str) -> CompileError {
    CompileError::NotModelled {
        span,
        gap: Gap::Construct { construct, owner },
    }
}

/// The name of `[0]!Name`: a qualified operand whose qualifier is the workbook index 0, the
/// formula's own workbook, and whose operand is a name.
fn own_workbook_name(node: &SyntaxNode) -> Option<&Token> {
    if node.kind() != SyntaxKind::QualifiedReferenceExpr {
        return None;
    }
    let [SyntaxChild::Token(qualifier), ..] = node.children() else {
        return None;
    };
    if qualifier.text() != "[0]" && qualifier.text() != "[0]!" {
        return None;
    }
    let target = node.child_nodes().next()?;
    match target.children() {
        [SyntaxChild::Token(token)] if target.kind() == SyntaxKind::IdentifierExpr => Some(token),
        _ => None,
    }
}

/// The catalogue entry written as `token` in `notation`.
fn target(notation: Notation, token: &Token) -> Result<Target, CompileError> {
    catalogue()
        .resolve_surface(notation, token.text())
        .map(Target)
        .map_err(|_| unknown(token.span(), token.text().to_owned()))
}

/// The catalogue entry a named call written `name` calls: the entry of its spelling after the
/// prefixes a file stores before a function's name ([`file_unprefixed`]). A refusal keeps the
/// spelling as written.
fn call_target(name: &Token) -> Result<Target, CompileError> {
    let target = catalogue()
        .resolve_surface(Notation::Call, file_function(name.text()))
        .map_err(|_| unknown(name.span(), name.text().to_owned()))?;
    let gap = match target.implementation() {
        calc_func::Implementation::NotHeld { owner } => Some(Gap::Construct {
            construct: "an unheld built-in function",
            owner,
        }),
        calc_func::Implementation::Deferred { class } => Some(Gap::Construct {
            construct: "a deferred built-in function",
            owner: class.owner(),
        }),
        _ => None,
    };
    match gap {
        Some(gap) => Err(CompileError::NotModelled {
            span: name.span(),
            gap,
        }),
        None => Ok(Target(target)),
    }
}

/// Whether a file's call written `spelling`, of the catalogue entry `target`, names a function a
/// file stores with a prefix without that prefix: Excel's load reads such a name as an unknown
/// one. A spelling with `_xlfn.` names the function whatever prefix a file stores it with
/// ([`file_unprefixed`]). An entry whose file prefix is not known is read by its plain name.
pub(super) fn unknown_on_load(spelling: &str, target: Target) -> bool {
    !spelling.eq_ignore_ascii_case(FIELD_ACCESS)
        && without_prefix(spelling, calc_catalogue::FUTURE_FUNCTION_PREFIX).is_none()
        && match target.0.file_prefix() {
            Some(FilePrefix::Xlfn | FilePrefix::XlfnXlws) => true,
            Some(FilePrefix::Bare) | None => false,
        }
}

/// The catalogue spelling a file's call written `spelling` names: FIELDVALUE for a field access
/// ([`FIELD_ACCESS`]), otherwise the spelling without its file prefixes ([`file_unprefixed`]).
pub(super) fn file_function(spelling: &str) -> &str {
    if spelling.eq_ignore_ascii_case(FIELD_ACCESS) {
        calc_catalogue::field_access_name()
    } else {
        file_unprefixed(spelling)
    }
}

pub(crate) enum BindingCall {
    Let,
    Lambda,
}

/// Calls which introduce local bindings rather than dispatching through the catalogue.
pub(crate) fn binding_call(spelling: &str) -> Option<BindingCall> {
    match catalogue()
        .resolve_surface(Notation::Call, spelling)
        .ok()?
        .kind()
    {
        calc_func::EntryKind::Binding(calc_func::BindingForm::Let) => Some(BindingCall::Let),
        calc_func::EntryKind::Binding(calc_func::BindingForm::Lambda) => Some(BindingCall::Lambda),
        _ => None,
    }
}

/// `spelling` without a leading `_xlfn.`, and without a `_xlws.` right after that, each matched
/// in any ASCII case; any other spelling unchanged. Excel reads a function it knows after
/// `_xlfn.` or `_xlfn._xlws.`, in any case, as that function, also one it stores without a prefix
/// (`_xlfn.SUM`), and reads `_xlws.` alone as an unknown name (#NAME?); in a file it loads, so are
/// `_xlfn._xlfn.` and `_xlws._xlfn.`. A formula a user enters may spell a function a file stores
/// with a prefix without it (`MAXIFS`); a file that does is read by [`unknown_on_load`].
pub(crate) fn file_unprefixed(spelling: &str) -> &str {
    calc_catalogue::file_unprefixed(spelling)
}

/// The rest of `text` when its first bytes are the ASCII `prefix` in any ASCII case. Bytes equal
/// to ASCII bytes are ASCII themselves, so the rest starts on a character boundary.
pub(crate) fn without_prefix<'t>(text: &'t str, prefix: &str) -> Option<&'t str> {
    calc_catalogue::strip_ascii_prefix(text, prefix)
}

/// Appends the check of the formula's final `+` or `-`, written `operator`, after the call that
/// computed it and retained its two operands. When the result is a non-zero number, each operand
/// is copied and negated by a call of the catalogue's prefix `-`, which reads it as a number by
/// the same worksheet coercion as the operation, and [`Instruction::NearZeroCheck`] compares the
/// result with the larger negation. The negations are marked synthetic: the formula did not write
/// them, and an observer of the evaluation's calls must not count them as its own.
fn final_sum_check(operator: &Token, out: &mut Vec<Instruction>) -> Result<(), CompileError> {
    let negate = catalogue()
        .resolve_surface(Notation::Prefix, "-")
        .map(Target)
        .map_err(|_| unknown(operator.span(), "-".to_owned()))?;
    let negation = Instruction::Call {
        target: negate,
        argc: 1,
        retain: false,
        synthetic: true,
        admission: Admission::Lift,
    };
    // Stack: left, right, result. Two below the top is the left operand; after its negation is
    // pushed, two below the top is the right operand.
    let check = [
        Instruction::Copy(2),
        negation.clone(),
        Instruction::Copy(2),
        negation,
        Instruction::NearZeroCheck,
    ];
    out.push(Instruction::NearZeroGuard { skip: check.len() });
    out.extend(check);
    Ok(())
}

/// The token of a node that holds exactly one.
pub(super) fn only_token(node: &SyntaxNode) -> Option<&Token> {
    match node.children() {
        [SyntaxChild::Token(token)] => Some(token),
        _ => None,
    }
}

/// Significant digits Excel keeps of a number it reads.
const SIGNIFICANT_DIGITS: usize = 15;

/// A number literal's value: the binary64 nearest to its decimal (ties to even), a zero as +0. A
/// literal above 9.99999999999999E+307, the largest number Excel lets a cell be given, is admitted
/// up to binary64's maximum (`=1E308`); what Excel does with it in a formula is not measured.
/// Digits after the 15th significant one are read as 0, as Excel keeps 15 (dc-uar: Excel
/// 16.0.20527 gives `7200547821627.375&""` as `7200547821627.37` and `1234567890123455&""` as
/// `1234567890123450`, 384 of 384 such literals in the catalogue oracle's dcuar_text rows).
fn number(node: &SyntaxNode) -> Result<Literal, CompileError> {
    let span = node.span();
    let refused = CompileError::NotModelled {
        span,
        gap: Gap::NumberLiteral,
    };
    let text = only_token(node).map(Token::text).ok_or(refused.clone())?;
    let (digits, nonzero) = significant_digits(text);
    let kept;
    let text = if digits > SIGNIFICANT_DIGITS {
        kept = fifteen_digits(text);
        kept.as_str()
    } else {
        text
    };
    let parsed: f64 = text.parse().map_err(|_| refused.clone())?;
    let value = if !parsed.is_finite() || (parsed == 0.0 && nonzero) {
        return Err(refused);
    } else if parsed.is_subnormal() {
        // Excel holds no subnormal: a literal below the normal range that it accepts is 0
        // (`=1E-308+1E-308` is 0, and `=1E-308` is stored as `=0`); one it does not accept
        // (`=1E-310`) is refused.
        if !calc_func::number_literal_scale_is_readable(text) {
            return Err(refused);
        }
        0.0
    } else {
        parsed
    };
    Ok(Literal::Number(
        if value == 0.0 { 0.0f64 } else { value }.to_bits(),
    ))
}

/// A number literal with the digits of its mantissa after the 15th significant one written as 0.
fn fifteen_digits(text: &str) -> String {
    let mut seen = 0;
    let mut exponent = false;
    text.chars()
        .map(|c| {
            if c == 'e' || c == 'E' {
                exponent = true;
            }
            if exponent || !c.is_ascii_digit() || (seen == 0 && c == '0') {
                return c;
            }
            seen += 1;
            if seen > SIGNIFICANT_DIGITS { '0' } else { c }
        })
        .collect()
}

/// The significant digits of a number literal's mantissa (from its first to its last non-zero
/// digit), and whether it has a non-zero digit at all.
fn significant_digits(text: &str) -> (usize, bool) {
    let mantissa = text.split(['e', 'E']).next().unwrap_or_default();
    let digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    let first = digits.iter().position(|&d| d != b'0');
    let last = digits.iter().rposition(|&d| d != b'0');
    match (first, last) {
        (Some(first), Some(last)) => (last - first + 1, true),
        _ => (0, false),
    }
}

/// A string literal's text: inside its quotes, each doubled quote one quote.
fn text(node: &SyntaxNode) -> Result<Literal, CompileError> {
    let span = node.span();
    let quoted = only_token(node).map(Token::text).ok_or(syntax(span))?;
    let inner = quoted
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or(syntax(span))?;
    let units = inner.replace("\"\"", "\"").encode_utf16().collect();
    Ok(Literal::Text(ExcelText::from_utf16_code_units(units)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_formula;
    use calc_func::value::{CellKey, IdAllocator, SheetId};

    struct Sheets(SheetId);

    impl ReferenceCatalog for Sheets {
        fn defined_name(
            &self,
            _: &str,
            _: crate::reference::NameScope,
        ) -> crate::reference::NameDefinition {
            crate::reference::NameDefinition::NotModelled
        }

        fn table(&self, name: &str) -> crate::reference::TableLookup {
            let _ = name;
            crate::reference::TableLookup::NotModelled
        }

        fn table_containing(
            &self,
            cell: calc_func::value::CellKey,
        ) -> crate::reference::TableLookup {
            let _ = cell;
            crate::reference::TableLookup::NotModelled
        }

        fn sheet_span(
            &self,
            first: calc_func::value::SheetId,
            last: calc_func::value::SheetId,
        ) -> Option<Vec<calc_func::value::SheetId>> {
            let _ = (first, last);
            None
        }

        fn unknown_calls_are_names(&self) -> bool {
            false
        }

        fn workbook_name(&self) -> Option<&str> {
            None
        }

        fn external_sheet(
            &self,
            book: &crate::reference::ExternalBookRef,
            sheet: &str,
        ) -> Option<calc_func::value::SheetId> {
            let _ = (book, sheet);
            None
        }

        fn external_function_target(
            &self,
            book: &crate::reference::ExternalBookRef,
        ) -> Option<String> {
            let _ = book;
            None
        }

        fn sheet_named(&self, name: &str) -> Option<SheetId> {
            name.eq_ignore_ascii_case("Sheet2").then_some(self.0)
        }
    }

    fn compiled(formula: &str) -> Result<Arc<CompiledPlan>, CompileError> {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let one = ids.issue().unwrap();
        let two = ids.issue().unwrap();
        let anchor = FormulaAnchor::new(CellKey::new(one, 5, 3).unwrap());
        compile(
            &parse_formula(formula),
            ReferenceNotation::A1,
            anchor,
            &Sheets(two),
            &mut PlanCache::new(),
        )
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn plan_cache_does_not_own_unused_plans_even_when_cloned() {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = ids.issue().unwrap();
        let anchor = FormulaAnchor::new(CellKey::new(sheet, 1, 1).unwrap());
        let mut cache = PlanCache::new();
        let plan = compile(
            &parse_formula("=1+2"),
            ReferenceNotation::A1,
            anchor,
            &Sheets(sheet),
            &mut cache,
        )
        .unwrap();
        let same = compile(
            &parse_formula("=1+2"),
            ReferenceNotation::A1,
            anchor,
            &Sheets(sheet),
            &mut cache,
        )
        .unwrap();
        assert!(Arc::ptr_eq(&plan, &same));
        let weak = Arc::downgrade(&plan);
        let copied_cache = cache.clone();
        drop(plan);
        assert!(weak.upgrade().is_some(), "a live caller retains its plan");
        drop(same);
        assert!(
            weak.upgrade().is_none(),
            "neither interner owns the bytecode"
        );
        drop(copied_cache);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn plan_cache_collects_expired_metadata_while_preserving_live_identity() {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let sheet = ids.issue().unwrap();
        let anchor = FormulaAnchor::new(CellKey::new(sheet, 1, 1).unwrap());
        let mut cache = PlanCache::new();
        let held = compile(
            &parse_formula("=1+2"),
            ReferenceNotation::A1,
            anchor,
            &Sheets(sheet),
            &mut cache,
        )
        .unwrap();
        for n in 0..1024 {
            let _ = compile(
                &parse_formula(&format!("={n}+3")),
                ReferenceNotation::A1,
                anchor,
                &Sheets(sheet),
                &mut cache,
            )
            .unwrap();
        }
        assert_eq!(cache.stats().live, 1);
        assert!(cache.stats().entries <= 5, "{:?}", cache.stats());
        cache.evict_expired();
        assert_eq!(cache.stats().entries, 1);
        let same = compile(
            &parse_formula("=1+2"),
            ReferenceNotation::A1,
            anchor,
            &Sheets(sheet),
            &mut cache,
        )
        .unwrap();
        assert!(Arc::ptr_eq(&held, &same));
        drop(same);
        drop(held);
        assert_eq!(cache.evict_expired(), 1);
        assert_eq!(cache.stats().entries, 0);
    }

    fn refusal(formula: &str) -> CompileError {
        match compiled(formula) {
            Ok(plan) => panic!("{formula} compiled: {plan:?}"),
            Err(error) => error,
        }
    }

    fn owner(formula: &str) -> &'static str {
        match refusal(formula) {
            CompileError::NotModelled {
                gap: Gap::Construct { owner, .. },
                ..
            } => owner,
            other => panic!("{formula}: {other:?}"),
        }
    }

    /// A defined name the catalogue cannot supply is refused with the work package that owns
    /// such names; the constructs L1 lowers compile.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn unsupported_constructs_name_their_owner() {
        assert_eq!(owner("=TaxRate*2"), "dc-10y");
        assert_eq!(owner("=Sheet2!TaxRate"), "dc-10y");
        for compiles in [
            "=(A1):B2",
            "=SUM(A1 B2)",
            "=SUM((A1,B2))",
            "=@A1:A3",
            "=A1#",
            "=SUM({1,2})",
            "=(A1)(2)",
            "=LET(x,1,x)",
            "=LAMBDA(x,x)(1)",
        ] {
            assert!(compiled(compiles).is_ok(), "{compiles}");
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn refusals_are_typed() {
        assert_eq!(
            refusal("=1+").rejection(),
            Some(Rejection::Grammar(GrammarRule::Syntax))
        );
        assert!(matches!(
            refusal("=Nope!A1"),
            CompileError::Reference {
                error: BindError::UnknownSheet { .. },
                ..
            }
        ));
        assert!(matches!(
            refusal("=$XFE$1"),
            CompileError::Reference {
                error: BindError::OutsideGrid,
                ..
            }
        ));
        assert_eq!(
            refusal("=NOSUCH(1)"),
            unknown(TextSpan::new(1, 6), "NOSUCH".to_owned())
        );
        let function = |spelling: &str, rule| {
            Some(Rejection::Function {
                function: catalogue()
                    .resolve_surface(Notation::Call, spelling)
                    .unwrap()
                    .id(),
                rule,
            })
        };
        assert_eq!(
            refusal("=IF(1)").rejection(),
            function("IF", calc_func::EntryRule::TooFewArguments)
        );
        assert_eq!(
            refusal("=SUM()").rejection(),
            function("SUM", calc_func::EntryRule::TooFewArguments)
        );
        assert_eq!(
            refusal("=COUNTBLANK(2)").rejection(),
            function(
                "COUNTBLANK",
                calc_func::EntryRule::ReferenceRequired { index: 0 }
            )
        );
        for literal in ["=1E400", "=1E-310", "=1E-400"] {
            assert!(
                matches!(
                    refusal(literal),
                    CompileError::NotModelled {
                        gap: Gap::NumberLiteral,
                        ..
                    }
                ),
                "{literal}"
            );
        }
    }

    /// A literal below the normal range that Excel accepts is 0 (read through COM on Excel
    /// 16.0.20527: `=1E-308`, `=1E-309`, `=0.1E-307` and `=2.2250738585072014E-308` are stored as
    /// `=0`; the jnt rows `=1E-308+1E-308`, `=+1E-309` and `=2E-308-1E-308` read 0), while the
    /// normal range's lower end (`=2.3E-308`) is kept and a literal Excel does not accept
    /// (`=1E-310`) stays refused.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn a_literal_below_the_normal_range_is_zero() {
        for literal in [
            "=1E-308",
            "=1E-309",
            "=0.1E-307",
            "=2.2250738585072014E-308",
            "=1E-308+1E-308",
            "=-1E-309",
            "=2E-308-1E-308",
        ] {
            assert!(compiled(literal).is_ok(), "{literal}");
        }
        assert!(compiled("=2.3E-308").is_ok());
        assert!(compiled("=1E-310").is_err());
        assert_eq!(compiled("=1E-308").unwrap(), compiled("=0").unwrap());
    }

    /// A leading `_xlfn.`, and a `_xlws.` right after it, come off a spelling in any ASCII case;
    /// no other text does, and a spelling cut inside a character is left whole.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn file_prefixes() {
        for (spelling, name) in [
            ("_xlfn.MAXIFS", "MAXIFS"),
            ("_XlFn.MAXIFS", "MAXIFS"),
            ("_xlfn._xlws.FILTER", "FILTER"),
            ("_xLfN._xLwS.FILTER", "FILTER"),
            ("_xlfn.STDEV.P", "STDEV.P"),
            ("_xlfn.", ""),
            ("_xlfn._xlws.", ""),
            ("_xlfn._xlfn.MAXIFS", "_xlfn.MAXIFS"),
            ("_xlfn._xlws._xlws.FILTER", "_xlws.FILTER"),
            ("_xlws.FILTER", "_xlws.FILTER"),
            ("_xlws._xlfn.MAXIFS", "_xlws._xlfn.MAXIFS"),
            ("MAXIFS", "MAXIFS"),
            ("_xlfnMAXIFS", "_xlfnMAXIFS"),
            ("_xlf", "_xlf"),
            ("_xlf\u{20AC}.SUM", "_xlf\u{20AC}.SUM"),
            ("_xlfn._xl\u{20AC}.SUM", "_xl\u{20AC}.SUM"),
        ] {
            assert_eq!(file_unprefixed(spelling), name, "{spelling}");
        }
        assert_eq!(
            refusal("=_xlfn.NOSUCH(1)"),
            unknown(TextSpan::new(1, 12), "_xlfn.NOSUCH".to_owned())
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn field_access_file_alias_uses_the_shared_id() {
        assert_eq!(file_function("_fV"), "FIELDVALUE");
        let target = catalogue()
            .resolve_surface(Notation::Call, file_function("_FV"))
            .unwrap();
        assert_eq!(target.id(), calc_catalogue::FIELD_ACCESS_ID);
    }

    /// Read as a file stores it, a call that writes without `_xlfn.` a function a file stores with
    /// it becomes the `#NAME?` literal, whatever its arguments and in any case of its name; with
    /// `_xlfn.`, and for a function a file writes bare, the plan is the one [`compile`] gives.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn loaded_unprefixed_calls() {
        let loaded = |formula: &str| {
            let mut ids: IdAllocator<SheetId> = IdAllocator::new();
            let one = ids.issue().unwrap();
            let two = ids.issue().unwrap();
            compile_loaded(
                &parse_formula(formula),
                ReferenceNotation::A1,
                FormulaAnchor::new(CellKey::new(one, 5, 3).unwrap()),
                &Sheets(two),
                &mut PlanCache::new(),
            )
        };
        let name = [Instruction::Literal(Literal::Error(
            WorksheetErrorCode::Name,
        ))];
        for formula in [
            "=MAXIFS(A1:A3,B1:B3,\"x\")",
            "=maxifs(A1:A3,B1:B3,\"x\")",
            "=IFNA(1,2)",
            "=XMATCH(1,A1:A3)",
            "=MAXIFS(A1)",
            "=MAXIFS({1,2},A1:A2,1)",
        ] {
            let plan = loaded(formula).unwrap_or_else(|e| panic!("{formula}: {e:?}"));
            assert_eq!(&plan.blocks[0][..], &name, "{formula}");
            assert!(plan.references.is_empty(), "{formula}");
            assert_eq!(plan.blocks.len(), 1, "{formula}");
        }
        for (file, entered) in [
            (
                "=_xlfn.MAXIFS(A1:A3,B1:B3,\"x\")",
                "=MAXIFS(A1:A3,B1:B3,\"x\")",
            ),
            ("=_xlfn._xlws.IFNA(1,2)", "=IFNA(1,2)"),
            ("=SUM(A1:A3)+1", "=SUM(A1:A3)+1"),
            ("=_xlfn.SUM(A1:A3)", "=SUM(A1:A3)"),
        ] {
            assert_eq!(loaded(file).unwrap(), compiled(entered).unwrap(), "{file}");
        }
        assert_eq!(
            compiled("=MAXIFS(A1:A3,B1:B3,\"x\")").unwrap().blocks[0].len(),
            4
        );
        assert_eq!(
            loaded("=_xlfn.MAXIFS(A1)"),
            Err(CompileError::Rejected {
                span: TextSpan::new(1, 16),
                rule: Rejection::Function {
                    function: catalogue()
                        .resolve_surface(Notation::Call, "MAXIFS")
                        .unwrap()
                        .id(),
                    rule: calc_func::EntryRule::TooFewArguments,
                },
            })
        );
        assert_eq!(
            loaded("=NOSUCH(1)"),
            Err(unknown(TextSpan::new(1, 6), "NOSUCH".to_owned()))
        );
    }

    /// Number literals of at most 15 significant digits read as their nearest binary64; leading
    /// and trailing zeros are not significant.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn number_literals() {
        let pushed = |formula: &str| match compiled(formula).unwrap().blocks[0][..] {
            [Instruction::Literal(Literal::Number(bits))] => f64::from_bits(bits),
            ref other => panic!("{formula}: {other:?}"),
        };
        assert_eq!(pushed("=0.1"), 0.1);
        assert_eq!(pushed("=.5"), 0.5);
        assert_eq!(pushed("=5."), 5.0);
        assert_eq!(pushed("=1E3"), 1000.0);
        assert_eq!(pushed("=123456789012345"), 123_456_789_012_345.0);
        assert_eq!(pushed("=1234567890123450000"), 1.23456789012345e18);
        assert_eq!(pushed("=0.000000000000000000001"), 1e-21);
        assert_eq!(pushed("=0e5").to_bits(), 0.0f64.to_bits());
        // Above 9.99999999999999E+307, the largest number Excel lets a cell be given: admitted, and
        // what Excel does with it is not measured.
        assert_eq!(pushed("=1E308"), 1e308);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn string_and_logical_literals() {
        let plan = compiled("=\"a\"\"b\"&TRUE&false").unwrap();
        let literals: Vec<&Instruction> = plan.blocks[0]
            .iter()
            .filter(|instruction| matches!(instruction, Instruction::Literal(_)))
            .collect();
        assert_eq!(
            literals,
            [
                &Instruction::Literal(Literal::Text(ExcelText::from_interop_assignment("a\"b"))),
                &Instruction::Literal(Literal::Logical(true)),
                &Instruction::Literal(Literal::Logical(false)),
            ]
        );
    }

    /// The formula's root operation is checked, and nothing below it: not through parentheses, a
    /// prefix sign, IF's branches, LET's result or IFERROR's and IFNA's arguments.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn final_sums_are_marked() {
        let marked = |formula: &str| -> usize {
            compiled(formula)
                .unwrap()
                .blocks
                .iter()
                .flat_map(|block| block.iter())
                .filter(|instruction| matches!(instruction, Instruction::NearZeroCheck))
                .count()
        };
        assert_eq!(marked("=0.5-0.4-0.1"), 1);
        assert_eq!(marked("=(0.5-0.4-0.1)"), 0);
        assert_eq!(marked("=+(1+2)"), 0);
        assert_eq!(marked("=+(1+2)-1"), 1);
        assert_eq!(marked("=IF(1+1,2-1,3+4)"), 0);
        assert_eq!(marked("=LET(a,1,2-a)"), 0);
        assert_eq!(marked("=IFERROR(0.5-0.4-0.1,1)"), 0);
        assert_eq!(marked("=IFNA(#N/A,2-1)"), 0);
        assert_eq!(marked("=IFERROR(IF(TRUE,2-1),1)"), 0);
        assert_eq!(marked("=IF(TRUE,IFERROR(1,2-1))"), 0);
        assert_eq!(marked("=1*(0.5-0.4-0.1)"), 0);
        assert_eq!(marked("=-(1+2)"), 0);
        assert_eq!(marked("=2*-(1+2)"), 0);
        assert_eq!(marked("=SUM(1+2)"), 0);
        assert_eq!(marked("=(1+2)%"), 0);
        assert_eq!(marked("=1+2=3"), 0);
    }

    /// The check's negations are calls of the catalogue's prefix `-` in the plan, after the final
    /// operation's retaining call, and are the only synthetic calls; no other call retains its
    /// arguments, and a negation the formula writes is not synthetic.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn final_sum_check_instructions() {
        let resolved = |notation, spelling| {
            Target(
                catalogue()
                    .resolve_surface(notation, spelling)
                    .expect("a catalogue entry"),
            )
        };
        let call = |target, argc, retain| Instruction::Call {
            target,
            argc,
            retain,
            synthetic: false,
            admission: Admission::IntersectAll,
        };
        let synthetic = |target| Instruction::Call {
            target,
            argc: 1,
            retain: false,
            synthetic: true,
            admission: Admission::Lift,
        };
        let minus = resolved(Notation::Infix, "-");
        let negate = resolved(Notation::Prefix, "-");
        let number = |n: f64| Instruction::Literal(Literal::Number(n.to_bits()));
        assert_eq!(
            compiled("=0.5-0.4-0.1").unwrap().blocks[..],
            [vec![
                number(0.5),
                number(0.4),
                call(minus, 2, false),
                number(0.1),
                call(minus, 2, true),
                Instruction::NearZeroGuard { skip: 5 },
                Instruction::Copy(2),
                synthetic(negate),
                Instruction::Copy(2),
                synthetic(negate),
                Instruction::NearZeroCheck,
            ]
            .into_boxed_slice()]
        );
        assert_eq!(
            compiled("=-0.5+1").unwrap().blocks[..],
            [vec![
                number(0.5),
                call(negate, 1, false),
                number(1.0),
                call(resolved(Notation::Infix, "+"), 2, true),
                Instruction::NearZeroGuard { skip: 5 },
                Instruction::Copy(2),
                synthetic(negate),
                Instruction::Copy(2),
                synthetic(negate),
                Instruction::NearZeroCheck,
            ]
            .into_boxed_slice()]
        );
        assert_eq!(
            compiled("=1*(0.5-0.4)").unwrap().blocks[..],
            [vec![
                number(1.0),
                number(0.5),
                number(0.4),
                call(minus, 2, false),
                call(resolved(Notation::Infix, "*"), 2, false),
            ]
            .into_boxed_slice()]
        );
    }
}
