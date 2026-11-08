# Functions and names: the catalogue design

Owner-agreed design, 2026-11-07. It refines `SYNTHESIS.md` section 4: "the catalogue declares argument demand,
reference preservation, coercion, array lifting, callable slots, volatility and context dependencies once.
CalcFml implements the execution machinery for those declarations."

## The principle

CalcFunc knows the functions. CalcFml knows the formula language. What is true of one function is declared once,
in that function's family fragment (`crates/calc-func/src/catalogue/<family>.rs`), next to its kernel and the Excel
evidence for it. CalcFml asks the catalogue and never tests a function's spelling. CalcFml's own vocabulary is the
grammar: operators, literals (`TRUE`, `FALSE`, the error literals), references, names, and the two binding forms
LET and LAMBDA as forms, not as spellings.

File-format mappings are kept apart from function behavior: the stored-prefix table is keyed by
English spelling, and the xlsb function-number table maps file numbers to names. Localised names are
an editing and display concern; the shared localised-name table is not yet implemented.

The catalogue includes the built-in names in the existing Excel census, implemented or not, and
the separately recorded deferred names. "Is this a built-in?" and "what does it do?" are answered
by the same table.

## What an entry declares

A `CatalogueEntry` holds a stable `FunctionId`, spelling and notation, `EntryKind`,
`Availability`, arity, optional repeating `ArgumentGroups`, parameters, `ResultForm`,
`ArgumentDemand`, `EntryTraits` and a kernel. Family fragments declare these with `const fn`
builders next to the implementation. The generated catalogue provides an ID table and a surface
table over the same entries. `entries()` includes bindings and unheld or deferred entries;
`targets()` and `surfaces()` enumerate entries with executable kernels and omit the add-in's
private surface.

`EntryKind` distinguishes worksheet functions, operators with their file spelling, binding forms,
add-ins with their library and exported name, and host routes with their provider and route kind.
A kernel is eager, resumable or lazy, or marks a binding, a `NotHeld` entry with its owner, or a
`Deferred` entry with its deferral class. Availability is `ListsInCellsBeta` for the four list
functions known to require Insider Beta and workbook compatibility version 3; `Unspecified` says
the supported builds or versions have not been established here. It is not a claim that an entry
works in every build.

RTD has a host route when its connector is registered; without that provider the catalogue's
unheld fallback reports the owning work package. The route and the fallback share one entry.

Each parameter has its name, `ParameterClass`, `EntryClass` and `LegacyPlace`. The legacy
declaration records how the place treats a range, an array and an array of references at the root
and below an array place. Repeated-place variants retain the original measured position rules,
including positions that do not repeat with the signature's last parameter. CalcFml interprets
these declarations and the formula's place; it holds no table of function names for those rules.

`EntryTraits` declares the other measured call behavior: legacy result intersection, multi-cell
reference refusal, missing-coordinate handling, nested subtotals, reference-array lifting and
admission, selection and blank handling, volatility, and numerical admission and publication.
Each entry's 3-D reference declaration records the measured positions that are taken, spread,
returned as #VALUE! or #REF!, or not measured. CalcFml asks the entry for that result. The 3-D
positions come from dc-10y's Excel 16.0.20430 measurements and the catalogue oracle's ThreeD
sheet; the legacy place and entry declarations cite their measurements in the family fragments.

Excel's entry takes any operand in a LAMBDA place (`=MAP({1},1)` enters and gives #VALUE!), so no
entry class requires a callable. Entry treats both measured result forms other than `Value` alike:
such a call fills a reference place and may be called. `Unknown` records an unmeasured result
form, and an unheld parameter's class and legacy place are not reported as measured.
`ArgumentGroups` says that from argument `from`
on the arguments come in groups of `size`, each taking its group's declared parameter, with at
most `tail` arguments after the last complete group. The first group's completeness follows the
arity. Signatures for unheld and deferred entries are handbook seeds; the entry checker does not
claim their arity or parameter rules were measured.

## Names

The compiler's `resolve` uses the catalogue and caller-owned local bindings, defined names,
tables and registered functions. In call position a built-in wins over a local or defined name of
the same spelling; `=LET(gcd,LAMBDA(a,b,a+b),gcd(48,36))` is 12. In value position a local or
defined name and a table take precedence over a bare built-in; a registered function is not a
function value. A file stores a function value as `_xleta.NAME`, so a bare identifier in a loaded
formula stays a name. The resolver also retains a reference callee and lets the existing reference
route report its result.

The catalogue layer is immutable. Scope belongs to the compiler, while defined names, tables and
registrations belong to the caller. An unheld built-in is a typed refusal naming its owner; a
deferred built-in names its deferral's owner. Binding forms are handled by the formula language.
`is_builtin` asks the catalogue.

## Entry checking: formulas Excel will not take

Excel checks a formula when it is entered and refuses some outright. COM reports that as `0x800A03EC`, the
"There's a problem with this formula" dialog. DNA Calc makes the same check in one place, used by:
- cell entry in the grid (refuse, store nothing);
- the file loader;
- the corpus verdict.

```rust
pub enum OperandForm {
    Missing, Constant, ArrayConstant,
    Reference,                                  // a reference, a reference operation, `@x`, `x#`, `#REF!`
    Call(FunctionId), Lambda, Let, Name,
    Invocation,                                 // `f(1)`, `LAMBDA(x,x)(1)`: a call of a value or of a name
    Expression,                                 // an operation of values: `-A1`, `A1+0`, `A1&""`
}

pub enum EntryRule {
    TooFewArguments, TooManyArguments, IncompleteGroup,
    ReferenceRequired { index: usize },         // COUNTBLANK(2), COUNTBLANK({"",1})
    ResultNotCallable,                          // IFERROR(#N/A,LAMBDA(x,x))(3)
}

impl FunctionCallTarget<'_> {
    pub fn check_entry(self, operands: &[OperandForm]) -> Result<(), EntryRule>;
    pub fn check_called(self) -> Result<(), EntryRule>;   // a call postfix after this call
}
```

A reference place takes a reference, a name (defined or not, whatever it holds), a LAMBDA, a LET, an invocation or
a call whose result form is not `Value`; it refuses a constant, an array constant, an omitted argument, an
operation of values and a call that gives a value, also in parentheses. A call postfix takes the same heads. These
rules hold on every one of the 3,822 formulas measured on Excel 16.0 build 20527 (dc-mx3x, `Range.Formula2` and
`Range.Formula` agreed on each): CalcFml's test `entry_matches_excel` replays them from
`crates/calc-fml/tests/fixtures/entry/excel-entry.tsv`. The 34 entries with reference places and the 12 whose calls
may give a reference (CHOOSE, DROP, IF, IFS, INDEX, INDIRECT, OFFSET, REDUCE, SWITCH, TAKE, TRIMRANGE, XLOOKUP)
came from a census of every catalogue entry; each declaration cites its refused cases and accepted neighbours.

`calc_fml::check_entry(tree, loaded)` makes the whole check on a parsed formula, without a workbook: it walks the
tree, builds each call's operand forms from syntax and asks the call's entry once, and holds the grammar's own
rules (`GrammarRule`):
- `Syntax`: a parser diagnostic;
- `BindingArguments`: `LET()`, `LET(x,1)`, `LET(x,1,y,2)`, `LAMBDA()`;
- `DuplicateLetName`, `DuplicateLambdaParameter`: a name declared twice by one LET or one LAMBDA, by the fold
  Excel matches parameters by (`xx` and `XX`, `x1` and `X1`, `é` and `É` clash); another LET or LAMBDA is another
  scope;
- `NotCallable`: a call postfix after a constant, an array constant or an operation (`=(1)(3)`);
- `OperatorCall`: `ANCHORARRAY` and the trim references `_TRO_*` typed as calls (only a file stores them) and
  `SINGLE` with other than one argument.

A compile result separates a claim from a gap:

```rust
pub enum CompileError {
    Rejected { span: TextSpan, rule: Rejection },        // a claim: Excel refuses this entry
    NotModelled { span: TextSpan, gap: Gap },            // a gap: a refusal
    Reference { span: TextSpan, error: BindError },
}
pub enum Gap { Construct { construct: &'static str, owner: &'static str }, UnknownFunction { spelling: String }, NumberLiteral }
pub enum Rejection { Grammar(GrammarRule), Function { function: FunctionId, rule: EntryRule } }
```

The three users ask the same check:
- **The grid** (calc-host's `interpret_entry`) refuses a formula Excel's entry refuses with
  `EntryRefusal::Rejected { span, rule }` and stores nothing.
- **The file loader** (calc-doc) records a stored formula Excel's entry refuses as
  `UnloadedCell::RejectedFormula { rule, cached }` and gives the cell no input. Excel does not open such a file
  as it is (`Workbooks.Open` fails with 0x800A03EC); its repair removes the formula and keeps the cached value as a
  constant. A calculation reads such a cell as blank when the file cached nothing, as Excel's repaired cell is,
  and does not read a cached value yet.
- **The corpus verdict** scores the cell from that record, or from the compile's rejection.

The grid and the loader act only on the measured rules (`Rejection::is_measured`): a parser diagnostic may be
syntax this parser does not hold, so it is left to the compiler, which reports it as a rejection.

A rejection is scored like a value:
- the oracle records Excel's refusal as an observation (`Observation::EntryRejected { form }`): reading a capture
  record turns each capture failure at stage `entry` with COM `0x800A03EC` into one, and every other exception stays
  a capture failure;
- the engine's rejection reaches the verdict as the provenance `Rejected { rule }`.

| Excel | DNA Calc | Verdict |
|---|---|---|
| refuses | refuses | `agreed_rejection`: a pass, counted in the headline and listed on its own line |
| refuses | accepts | `entry_accepted`: a mismatch |
| accepts | refuses | `entry_rejected_wrongly`: a mismatch |
| not observed | refuses | `rejection_not_run`: a failure |

A refusal still means only "not modelled".

## Localised names

Files store formulas in English, so localised names belong to editing and display. The intended
table is keyed by `FunctionId` and locale, seeded from the handbook's 40-locale harvest and checked
against Excel's `Range.FormulaLocal` with each language installed. CalcFml's localised parse and
print and the grid's localised entry and display still need that table.
