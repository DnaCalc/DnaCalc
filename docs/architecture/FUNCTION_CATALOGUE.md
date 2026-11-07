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

Facts about the file format rather than a function's behaviour are kept as one generated table each, keyed by
`FunctionId`:
- the prefix a file stores (`_xlfn.`, `_xlws.`);
- the xlsb function number;
- the localised names.

Every built-in Excel knows has a catalogue entry, implemented or not. So "is this a built-in?" and "what does it
do?" are answered by the same table.

## What an entry declares

```rust
pub(crate) struct CatalogueEntry {
    pub(crate) id: FunctionId,
    pub(crate) spelling: &'static str,          // English, upper case; the token for an operator
    pub(crate) notation: Notation,              // Call | Prefix | Infix | Postfix
    pub(crate) kind: EntryKind,
    pub(crate) availability: Availability,      // which Excel builds and compatibility versions know it
    pub(crate) arity: CallableArityShape,       // {min, max}; LAMBDA values share this type
    pub(crate) groups: Option<ArgumentGroups>,  // repeating groups: IFS pairs, SWITCH pairs + default, LET pairs
    pub(crate) parameters: &'static [Parameter],
    pub(crate) result: ResultForm,
    pub(crate) demand: ArgumentDemand,          // Eager | BranchOnCondition | FallbackOnError
    pub(crate) traits: EntryTraits,             // per-function behaviours with a measured name
    pub(crate) kernel: Kernel,
}

pub(crate) enum EntryKind {
    Worksheet,                                  // an ordinary built-in
    Operator { file_spelling: Option<&'static str> },   // `@` stored as _xlfn.SINGLE, `#` as _xlfn.ANCHORARRAY
    Binding(BindingForm),                       // LET, LAMBDA: arguments are names and bodies, not values
    AddIn { library: &'static str },            // EUROCONVERT through EUROTOOL.XLAM
    HostRoute { provider: &'static str },       // WEBSERVICE, IMAGE through the host's fetch route
}

pub(crate) enum Kernel {
    Eager(EagerKernel),
    Resumable { run: EagerKernel, start: StartKernel },
    Lazy(LazyKernel),
    Binding,                                    // CalcFml's scope machinery runs it
    NotHeld { owner: &'static str },            // Excel knows it; not implemented yet: refused, owner named
    Deferred { class: Deferral },               // by owner policy: services, PY, lists in cells (dc-nk93)
}

pub(crate) struct Parameter {
    pub(crate) name: &'static str,              // as signature help shows it
    pub(crate) class: ParameterClass,           // how the evaluator passes it: Value | ValueOrReference | Reference
    pub(crate) entry: EntryClass,               // what Excel's formula entry accepts here
    pub(crate) legacy: LegacyPlace,             // how a legacy formula treats a range or array here
}

pub enum EntryClass { Any, Reference, Callable }          // COUNTBLANK's range: Reference
pub enum ResultForm { Value, MayBeReference, MayBeCallable, MayBeEither }
pub(crate) struct ArgumentGroups { pub(crate) from: usize, pub(crate) size: usize, pub(crate) tail: usize }
```

`LegacyPlace` holds the measured place rules of the legacy (pre-dynamic-array) form: whether a range is
intersected, lifted, aggregated or kept as a reference, at the root and below an array place. `EntryTraits` holds
the remaining measured behaviours, each under its own name, for example:
- implicit intersection of the result in a legacy formula (ROW, COLUMN, FILTERXML);
- multi-cell references refused (MROUND, QUOTIENT);
- nested subtotals skipped (SUBTOTAL, AGGREGATE);
- missing coordinates padded before the call (ROUNDUP, ROUNDDOWN).

Family fragments declare entries through `const fn` builders, so a declaration stays one readable block:

```rust
const RANGE: &[Parameter] = &[Parameter::value_or_reference("range").entry(EntryClass::Reference)];
```

Every fact that is not the default cites its Excel evidence where it is declared, as a doc comment or a named
test case. The evidence includes cases Excel refuses and near neighbours it accepts.

## Names

A formula's names are resolved in one name space, built from layers that their owners supply:

```rust
pub enum Resolved<'c> {
    BuiltIn(FunctionCallTarget<'c>),            // from the catalogue: held, not held or deferred
    Local(LocalBinding),                        // a LET name or LAMBDA parameter in scope
    Defined(NameDefinition),                    // a workbook or sheet name
    Table(TableLookup),
    Registered(RegisteredFunction),             // a UDF, with its source: Xll | Vba | AddIn | Js
    Unknown,
}

pub enum Position { Call, Value }               // `F(...)` versus a bare `F`

fn resolve(&self, text: &str, position: Position) -> Resolved<'_>;
```

The catalogue layer is immutable and generated. The local layer belongs to the compiler's scope. The defined-name,
table and registration layers belong to the caller: the workbook's catalogue and the host's registries, as
SYNTHESIS requires ("mutable UDF registries belong to the caller, not a global singleton"). Excel's precedence rules
are written once, in `resolve`, with their measurements:
- **Call position:** a built-in wins over a local or defined name of the same spelling.
  `=LET(gcd,LAMBDA(a,b,a+b),gcd(48,36))` is 12, GCD's value.
- **Value position:** a defined name or table comes first. A bare built-in is the function's value:
  `=MAP({1,2,3},ABS)` works. A registered function is not a value: `=MAP({1,2,3},MyFunc)` is #VALUE! even when
  MyFunc's arity fits. Only built-ins are function values.
- **Loaded formulas:** a file stores a function value as `_xleta.NAME`, so in a loaded formula a bare identifier
  stays a name.

Because every Excel built-in has an entry (`NotHeld` and `Deferred` included), "built-in or not" never needs a
second list.

## Entry checking: formulas Excel will not take

Excel checks a formula when it is entered and refuses some outright. COM reports that as `0x800A03EC`, the
"There's a problem with this formula" dialog. DNA Calc makes the same check in one place, used by:
- cell entry in the grid (refuse, store nothing);
- the file loader;
- the corpus verdict.

```rust
pub enum OperandForm { Missing, Constant, ArrayConstant, Reference, Call(FunctionId), Lambda, Name, Expression }

pub enum EntryRule {
    TooFewArguments, TooManyArguments, IncompleteGroup,
    ReferenceRequired { index: usize },         // COUNTBLANK(2), COUNTBLANK({"",1})
    CallableRequired { index: usize },
    ResultNotCallable,                          // IFERROR(#N/A,LAMBDA(x,x))(3)
}

impl FunctionCallTarget<'_> {
    pub fn check_entry(self, operands: &[OperandForm]) -> Result<(), EntryRule>;
}
```

The grammar's own rules (duplicate LET names, duplicate LAMBDA parameters, both case-insensitive within one scope)
stay in CalcFml. A compile result separates a claim from a gap:

```rust
pub enum CompileError {
    Rejected { span: TextSpan, rule: Rejection },        // a claim: Excel refuses this entry
    NotModelled { span: TextSpan, construct: &'static str, owner: &'static str },   // a gap: a refusal
    Reference { span: TextSpan, error: BindError },
}
pub enum Rejection { Grammar(GrammarRule), Function { function: FunctionId, rule: EntryRule } }
```

A rejection is scored like a value:
- the oracle records Excel's refusal as an observation (`EntryRejected { form }`);
- the engine reports `Rejected { rule }` as its provenance.

| Excel | DNA Calc | Verdict |
|---|---|---|
| refuses | refuses | match (`AgreedRejection`), counted in the headline and listed on its own line |
| refuses | accepts | mismatch, class entry accepted |
| accepts | refuses | mismatch, class entry rejected wrongly |

A refusal still means only "not modelled".

## Localised names

Files store formulas in English, so the calculation never sees a localised name. Localised names are a property of
editing and display: CalcFml's parser and printer read the user's formula through them and through the locale
profile's list separator (`;` in many locales). They come from one table per locale, keyed by `FunctionId`. Its
seed is the handbook's 40-locale harvest of Microsoft Support's function index. Excel, through `Range.FormulaLocal`
with the language installed, decides where the two differ. The handbook reads the same table, so it keeps no copy
of its own. The grid's localised formula entry and display follow from this table.

## Getting there

These steps lead to milestones, each landing green:
1. Entry metadata (`EntryClass`, `ResultForm`, `groups`), `check_entry`, the split compile result and the rejection
   verdict. This step changes scores: agreed rejections become matches, and DNA Calc's acceptances of formulas Excel
   refuses become named mismatches.
2. `LegacyPlace` on every parameter, carrying the measured place rules CalcFml holds today. This changes no
   behaviour: the corpus and the function bank give identical counts before and after.
3. `EntryKind`, `EntryTraits` and the full entry list (`NotHeld`, `Deferred`, `Availability`), and the name space
   with `resolve`. This changes no behaviour, by the same test.
4. The localised-name table, CalcFml's localised parse and print, and the grid's localised entry and display.
