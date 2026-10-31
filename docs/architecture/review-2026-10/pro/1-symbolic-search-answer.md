## Recommendation

**Build a small, exact program-identification system around your existing evaluator. Its core should be bounded enumeration, exact rounding preimages, and experiments that distinguish surviving programs.** Add SMT for narrowly defined arithmetic questions. Use machine learning later to improve search order and probe selection.

Appendix A is a useful first experiment, but I would change it before running it. The most consequential issues are:

1. **The frozen helper values are hypotheses.** A combine search can fail because `expm1` is wrong, because its retained precision is wrong, or because the combine is wrong.
2. **The 39,366 programs cover six selected skeletons, not the plausible association space.** Some ordinary, inexpensive alternatives are missing.
3. **The quotient-store hypothesis retained in Appendix D needs reconciliation with the reachability result in Appendix A.**
4. **Appendix C still contains instructions that recreate the rejected methodology:** ranking complete candidates by match count, retaining beams by numerical closeness, and constructing branches where different approximations win.
5. **The strongest immediate addition is a small independent checker for exact inverse-rounding and floating-point lattice certificates.** This can reject whole families and help distinguish prefix defects from suffix defects.

I treat the corpus measurements and historical results in your [prompt and appendices](1-symbolic-search-prompt.md) as supplied evidence. I have not replayed that bank or run Excel. I independently checked the arithmetic counts and the small numerical counterexamples below, and checked the relevant public specifications and tool documentation.

## 1. State precisely what the search identifies

For a grammar \(G\), execution profile \(E\), observable domain \(D\), and trusted witness set \(W\), maintain:

\[
V(G,W,E)=
\left\{
P\in G:
\forall(x,y)\in W,\ \operatorname{Obs}_E(P,x)=y
\right\}.
\]

A complete candidate either belongs to this set or has a contradiction. There is no numerical closeness term.

Separate three activities:

| Activity | Permitted evidence |
|---|---|
| **Deciding which programs to try first** | Published provenance, historical plausibility, size, coherent precision blocks, search cost |
| **Diagnosing why a family failed** | Residual patterns, exact preimage constraints, counterexamples, branch experiments |
| **Accepting a complete candidate** | Exact agreement with every applicable trusted witness |

Optimizing witness order by how many candidates it kills is useful scheduling. Selecting a candidate because it survives more witnesses is the failure mode you have rejected.

There is also a necessary distinction between recovering an observable program and recovering Excel’s historical source. Several source programs can have identical observable behavior. Finite black-box observations cannot establish that no unseen branch exists.

The useful claim is therefore:

> This is the unique surviving executable model within grammar \(G\), under profile \(E\) and scope \(D\); it also passes the specified independent validation.

That is considerably stronger than “best fit,” while remaining honest about what was established.

This formulation closely matches **oracle-guided component-based synthesis**: candidate components, exact constraints, and oracle questions chosen to separate alternatives. The research literature supplies that architecture, although its demonstrations do not establish a ready-made solution for Excel’s floating-point kernels. [1]

## 2. Represent algorithm sketches, then their executable realizations

### 2.1 Use a small control-flow language with expression DAGs inside it

A DAG alone is insufficient for branches, changing numeric state, and iterative algorithms. I would use a typed SSA-style representation with:

- Expression DAGs within basic blocks.
- Explicit branches with lazy evaluation.
- Explicit precision and rounding-state regions.
- Explicit loads, conversions, stores, and publication.
- Small state machines for recurrences and solvers.

The main representation choices should be:

| Component | What must be represented |
|---|---|
| Inputs | Exact bits, types, omissions, coercions, array element identity |
| Values | Binary64, canonical extended values, integers, booleans, worksheet errors |
| Operations | Opcode, operand types, result semantics, rounding mode |
| Storage | Binary64 conversion, extended spill, reload, retained resident value |
| Constants | Exact payload, source, conversion and loading path |
| Sharing | Reused value versus recomputation, including each use’s conversion |
| Calls | Helper identity, argument staging, return format, profile |
| Guards | Predicate expression, evaluation precision, comparison strictness |
| Loops | State variables, update order, stores, termination, returned state |
| Publication | Numeric conversion, zero normalization, error mapping, shape |

For example, these are distinct instructions:

```text
add_b64(a: B64, b: B64) -> B64
add_x87_p64(a: Ext80, b: Ext80) -> Ext80
store_b64(a: Ext80) -> B64
load_b64(a: B64) -> Ext80
spill_ext80(a: Ext80) -> Ext80
```

A `store` flag attached to a generic arithmetic operation can be convenient syntax, but lower it to explicit typed operations before evaluating or proving anything.

This avoids ambiguity when one consumer uses the extended value and another uses its binary64 publication.

### 2.2 Search within plausible algorithm sketches

For the difficult functions, unrestricted arithmetic-tree enumeration is too broad. Start with a library of **algorithm sketches**:

- Rational approximation with a specified coefficient family.
- Horner, grouped Horner, or Clenshaw evaluation.
- Argument reduction followed by approximation and reconstruction.
- Forward or backward recurrence.
- Series or continued-fraction evaluation.
- Direct-product, logarithmic, or saddle-point probability mass.
- A particular inverse iteration family.

Then search discrete holes within a sketch: association, constants’ loading rules, stores, reduction details, guards, accumulation direction, and stopping schedules.

For PMT, a small arithmetic grammar is appropriate. For incomplete gamma, a grammar that freely invents a hundred-operation tree is not.

Order sketches by a fixed plausibility prior:

1. Published family and provenance.
2. Number of operations.
3. Number of numeric-mode regions.
4. Number of stores and helper boundaries.
5. Number and complexity of branches.
6. Number of unexplained constants.
7. Unusual implementation features.

This prior controls **when** a program is examined. It must not let a refuted program survive.

### 2.3 Search coherent precision blocks before arbitrary node assignments

Human-written legacy code is more likely to have a few coherent arithmetic regions, local-variable stores, and call boundaries than an independent precision decision at every operator.

Search in this order:

1. One coherent arithmetic model.
2. Stores at named temporaries and helper boundaries.
3. Different models for a few blocks.
4. Arbitrary node-level annotations within the declared bound.

Do not permanently exclude the later layers. Report them as unsearched until reached.

The physical x87 schedule can eventually matter: evaluation order, stack pressure, and spill format may distinguish realizations of the same source expression. Initially, explicit conversions and stores capture much of the observable effect. Add scheduling as a separate search axis when a concrete ambiguity demands it.

### 2.4 Constants need loading histories

A numerical value and its provenance are separate fields. Distinguish:

- A binary64 constant loaded exactly into an extended register.
- A published decimal literal converted directly to an extended constant.
- An x87 constant-load instruction.
- A constant derived by a sequence of rounded operations.
- A reciprocal computed at runtime.

For later constant recovery, an unknown constant may be a shared symbolic parameter constrained by every applicable witness. That is different from minimizing residuals over coefficient bits. Keep the number of such parameters small, preserve the load hypothesis, and require fresh predictions that distinguish the surviving representations.

The first PMT round needs none of this machinery beyond exact 0 and 1.

### 2.5 Deduplicate without losing completeness

A prediction signature for an intermediate must contain its **full typed value**, including extended bits and relevant state. A binary64 publication signature is insufficient.

Two expressions equal on today’s witnesses may differ on tomorrow’s question. Therefore either:

- Retain all members of a signature class in a compressed representation; or
- Regenerate the affected enumeration whenever the witness set changes.

Keeping one representative forever silently changes a complete search into an incomplete one.

## 3. Make the numeric semantics exact before searching them

### 3.1 Three models that must remain distinct

Let \(R_B\) round to binary64 and \(R_E\) round to the extended format with a 64-bit significand.

For ordinary finite arithmetic:

\[
\text{strict binary64:}\quad R_B(a\circ b)
\]

when \(a,b\) are already binary64;

\[
\text{resident PC64:}\quad R_E(a\circ b);
\]

\[
\text{stored PC64:}\quad R_B(R_E(a\circ b)).
\]

If a strict binary64 operation receives extended children, its operands must first be converted:

\[
R_B\bigl(R_B(a)\circ R_B(b)\bigr).
\]

For instance, with resident \(a=1+2^{-53}\) and \(b=-1\), converting the operands first yields zero. Rounding only the exact final difference yields \(2^{-53}\).

Two further distinctions matter:

- **PC53 retains the extended exponent range**, so it is not binary64 arithmetic.
- An **extended-format spill preserves the value**, whereas a binary64 spill rounds it.

The x87 precision-control field affects particular arithmetic instructions, including addition, multiplication, division, and square root; it does not impose one generic precision rule on every x87 instruction. [2]

### 3.2 Include constructed substrate tests

Here is a small double-rounding discriminator I checked using exact rational arithmetic:

\[
a=1,\qquad
b=2^{-53}+2^{-65}=\mathtt{0x1.001p{-53}}.
\]

Both inputs are exactly binary64-representable. Nevertheless:

\[
R_B(a+b)=1+2^{-52},
\]

while:

\[
R_B(R_E(a+b))=1.
\]

Other useful tests include:

- Resident PC53 evaluation of \((2^{1023}+2^{1023})/2\), contrasted with binary64 overflow.
- Producing \(1+2^{-60}\), spilling to extended or binary64, then subtracting 1.
- Changing precision while an operand still contains additional significand bits.
- Underflow, ties, cancellation, zero signs, and publication boundaries.

These are tests of your evaluator, not Excel observations.

Change Appendix A’s promise to “prove … against the current native/software operations” to **validate both implementations against a deliberate independent reference suite**. Finite agreement between two backends is not a proof, particularly if they share implementation assumptions.

### 3.3 Treat transcendental instructions separately

There is no single x87 instruction implementing natural logarithm or general exponential. `FYL2X`, `FYL2XP1`, `F2XM1`, constants, reduction, scaling, and stores form an instruction sequence. Intel documents approximate transcendental behavior and changes across processor generations. [2]

Consequently:

- A correctly rounded mathematical `exp` or `log` is not an emulator of the target instruction sequence.
- Agreement with hardware strongly supports a behavioral model; it does not establish a unique internal implementation.
- Worksheet `EXP` agreement does not establish PMT’s private helper or its extended return value.
- The Linux host’s native results must be associated with its CPU profile, separately from the Windows target.

Use three explicit classes of transcendental leaf:

1. **Concrete leaf:** exact bits supplied for this witness; conclusions are conditional on them.
2. **CPU operation:** an explicitly defined native instruction sequence and machine profile.
3. **Abstract leaf:** a sound enclosure or relation used for exclusion.

An abstract leaf that admits extra behavior can support an exclusion when the overapproximated family is impossible. Its satisfiability does not establish an implementation.

## 4. Exact elimination: start with inverse rounding and lattice intersection

### 4.1 An observed result constrains a set of internal values

For an ordinary finite nonzero binary64 result \(y\), define its rounding cell:

\[
C_B(y)=\{z:R_B(z)=y\}.
\]

The endpoints are the exact rational midpoints:

\[
\ell=\frac{\operatorname{prev}(y)+y}{2},
\qquad
u=\frac{y+\operatorname{next}(y)}{2}.
\]

Under nearest-even rounding, midpoint inclusion depends on the parity of \(y\)’s significand.

Use predecessor and successor separately. A symmetric “\(y\pm\frac12\operatorname{ULP}(y)\)” shortcut mishandles binade boundaries. Zeros, overflow, exceptional values, and Excel’s publication rules need separate cases.

An observed answer gives an interval constraint—not the internal value \(y\), and not a rounded inverse of \(y\).

### 4.2 Double rounding has a different preimage

For:

\[
DR=R_B\circ R_E,
\]

the correct preimage is:

\[
D(y)=
\bigcup_{z\in F_E\cap C_B(y)}C_E(z).
\]

Let \(z_{\min}\) and \(z_{\max}\) be the first and last extended lattice points that publish to \(y\). Their rounding cells join into a contiguous interval with exact endpoints and endpoint-inclusion flags.

For example:

\[
D(1)=
\left[
1-2^{-54}-2^{-65},
\quad
1+2^{-53}+2^{-64}
\right].
\]

This is wider than the ordinary binary64 cell, and asymmetric.

The implementation needs integers, powers of two, rational comparisons, and explicit open/closed endpoints. It does not need a tolerance parameter.

### 4.3 The quotient certificate should quantify over every possible quotient

Suppose the final shell is precisely:

\[
y=DR(qr),
\]

with known nonzero \(r\), and the hypothesis requires \(q\in F_B\), the binary64 lattice.

Then:

\[
q\in D(y)/r,
\]

so the family is impossible when:

\[
F_B\cap\bigl(D(y)/r\bigr)=\varnothing.
\]

Reverse interval ordering when \(r<0\).

Within each binade, lattice intersection reduces to integer ceiling/floor bounds on a grid \(k2^s\), adjusted for open endpoints.

This certificate is unusually strong: **if no arbitrary binary64 \(q\) works, no numerator or helper repair can rescue that exact final shell.**

But its conclusion is limited to that shell. It does not reject a final division, a different multiplier, or a different publication rule.

The reported 54-bit result means that a finer lattice contains a feasible value. It establishes neither a 54-bit Excel precision mode nor a quotient-producing program.

### 4.4 Propagate constraints backwards, preserving correlation

Generalize the same mechanism:

1. Start with the observed output’s publication preimage.
2. Pull the set backwards through the final rounded operation.
3. Intersect with the producer’s value lattice.
4. Continue through known operands and stores.
5. Split domains where inversion changes monotonicity or crosses zero.

Intervals and affine arithmetic are useful **sound filters**, especially for eliminating broad families. They become weak when repeated quantities are treated independently.

For PMT, `em` appears in both numerator and denominator. Those occurrences must denote the same value. An interval enclosure that forgets that relationship can remain sound but admit many impossible combinations.

An enclosure excluding the target rejects a family. An enclosure containing the target is inconclusive.

### 4.5 Use shared latent helpers to separate defects

For a fixed \((r,n)\), vary the financial amounts while retaining the hypothesis that the same helper result \(e\) is used.

For combine graph \(G\), define:

\[
E_i(G)=
\left\{
e:
\operatorname{eval}(G,r,n,PV_i,FV_i,type_i,e)=y_i
\right\}.
\]

Then require:

\[
\bigcap_i E_i(G)\ne\varnothing.
\]

This produces useful distinctions:

- **Empty intersection:** the asserted common-helper-plus-combine model is impossible.
- **Nonempty intersection excluding the frozen helper:** the frozen helper value or representation is inconsistent with that combine.
- **Multiple feasible helper values:** more discriminating amount probes are needed.
- **A uniquely constrained helper value:** a new target for helper-program search, still conditional on the combine.

Only enforce sharing where the model asserts it. A stored and resident variant may require a relationship such as \(e_B=R_B(e_E)\), not unconditional identity.

This is substantially stronger than back-solving an independent `em` from each published answer.

### 4.6 First kills prove rejection, not causation

Persist a first killing witness for every rejected candidate. Also keep its full-width trace.

However, a histogram of first kills is affected by witness ordering. It does not identify the defective subexpression.

For localization, use **relaxed subgraphs**: replace a suspected intermediate with all values allowed by its type or justified bounds. If the suffix still cannot reach the target, changing that prefix cannot repair the suffix.

Conversely, finding some arbitrary intermediate that works does not validate the suffix. It only establishes reachability.

### 4.7 Keep measurement failures distinct from contradictions

An unparseable expectation, failed COM call, missing row, or unsupported capture is a failed verification obligation. It is not a semantic counterexample.

Do not silently skip these rows, but do not turn them into mathematical evidence against every candidate either. Report separate counts for:

- Exact successes.
- Trusted contradictions.
- Unsupported observations.
- Harness failures.
- Quarantined evidence.
- Unchecked rows.

## 5. Specific corrections to the PMT experiment

### 5.1 The count is correct, but describes a narrow grammar

Each proposed skeleton has:

- Three numerator operations.
- Two operations for `tf`.
- Three combine operations.

Thus:

\[
2\times3\times3^8=39{,}366.
\]

The count is correct once the three modes are precisely defined.

There are also immediate redundancies. At the root, resident PC64 followed by mandatory binary64 publication has the same result as stored PC64 followed by publication. Removing that duplicate alone gives an upper bound of 26,244 distinct annotated-root behaviors. Further reductions are possible on restricted input types.

This is optional optimization. The larger issue is omitted structure.

### 5.2 Ordinary associations are missing

Examples include:

```text
((N*r)/em)/tf
((N/em)*r)/tf
(N/em)*(r/tf)
N*(r/(em*tf))
(N/(em*tf))*r
```

The numerator alternatives omit:

```text
PV + (FV + FV*em)
(PV + FV*em) + FV
```

A plausible rearrangement of the whole expression is:

```text
(FV + (PV + FV)/em) * (r/tf)
```

That last form can use fewer operations than the proposed decomposition and therefore deserves an early place under the human-program prior.

Here is a concrete demonstration of the numerator omission. Take:

\[
PV=-(2^{53}-1),\quad FV=2^{53},\quad em=-2^{-54}.
\]

Under strict binary64 nearest-even arithmetic:

| Numerator | Result |
|---|---:|
| `PV + FV*(1+em)` | \(1\) |
| `(PV+FV) + FV*em` | \(1/2\) |
| `(PV+FV*em) + FV` | \(+0\) |

The balances are not exactly opposing.

This proves that an omitted association has distinct behavior. It does **not** establish Excel’s answer or prove failure of every precision annotation in the original grammar.

### 5.3 Define the branch and sign convention executablely

If:

\[
em=(1+r)^{-n}-1,\qquad tf=1+r\,type,
\]

then the mathematical form is:

\[
PMT=
\frac{r[PV+FV(1+em)]}{em\,tf}.
\]

There is no additional leading minus under that convention.

“Finite discount arm” needs an executable definition addressing:

- `em == 0`.
- `tf == 0`.
- Exponential underflow giving `em == -1`.
- Numerator cancellation despite non-opposing balances.
- Intermediate overflow with finite leaves.
- Nonstandard `type` values and coercion.
- Resident versus published helper values.
- Output normalization and error mapping.

Membership must follow predeclared input and execution predicates. It must not be chosen after observing which rows a graph matches.

### 5.4 Reconcile the retained quotient store

Appendix D retains the quotient-store change associated with `17e42ce`. Appendix A reports witnesses unreachable by any binary64 quotient through the existing final product.

Those statements require an explicit conflict record.

Possible resolutions include different branches, profiles, final shells, or an earlier store hypothesis that was overgeneralized. Do not silently retain the store as settled while using the certificate to require its absence.

Recheck the certificate independently, then state exactly which store hypothesis it refutes.

### 5.5 Keep two helper lanes

Run:

- A **conditional combine search**, with exact helper values and formats frozen.
- A **shared-helper feasibility search**, using the intersections above.

Freezing only binary64 helper outputs is particularly dangerous if the target retains extended low bits. The first lane remains useful, but zero survivors means that its conjunction of assumptions failed.

### 5.6 Budget for streaming, not a full prediction matrix

The original family against all reported discount rows gives:

\[
39{,}366\times250{,}730
=
9{,}870{,}237{,}180
\]

candidate-row evaluations before early exits.

Storing one eight-byte result per pair would require about 79 GB, already outside the proposed memory budget.

Use:

1. A compact challenge basis.
2. Streaming exact first-kill elimination.
3. Shared evaluation of genuinely identical sub-DAGs.
4. Full-bank replay only for survivors.
5. Checkpointed enumeration IDs and explicit coverage accounting.

Thirty-two discovery rows can be an effective first filter. Their purpose is to eliminate cheaply, not to establish identification.

## 6. Choose experiments that distinguish explanations

### 6.1 Optimize the partition, not the number of different outputs

For each proposed input \(x\), partition survivors by complete predicted outcome. If the bucket masses are \(w_z\), useful objectives include:

\[
\min_x\max_z w_z
\]

for worst-case elimination, or:

\[
\max_x\left(1-\sum_z w_z^2\right)
\]

for pair separation.

A split of 500/500 is generally more useful than 998/1/1, despite having fewer distinct answers.

Do not let thousands of redundant store variants outweigh a different algorithm family. Use proven-equivalence classes where available, and otherwise family-balanced weights or diversity quotas. These are experiment-design choices, not probabilities that Excel contains particular source code.

### 6.2 Select batches jointly

Choosing the 64 individually highest-ranked probes often asks the same question 64 times.

Instead, choose greedily by **additional separation after the already-selected questions**. Maintain groups still indistinguishable under the batch prefix and select the next input that splits those groups most usefully.

For an unresolved group \(B\), partitioned into \(B_z\), the number of newly separated unordered pairs is:

\[
\frac{|B|^2-\sum_z|B_z|^2}{2}.
\]

No quadratic candidate-pair table is required.

### 6.3 A reasonable reading budget

I would start with **64–128 discovery readings per round**, expanding toward a few hundred for boundary maps.

A possible 128-row batch is:

- 80 jointly selected discriminators.
- 24 adjacent-value or boundary companions.
- 16 substrate doubt controls.
- 8 transport and repeatability controls.

For PMT, select some questions as fixed-\((r,n)\) groups with different `PV`, `FV`, and `type`, because their shared-helper constraints are valuable collectively.

Pure scaling can be redundant. If scaling both amounts by a power of two merely scales every intermediate, it may add almost no information. Change relative amounts, cancellation, and exponents deliberately.

Freeze questions and predictions before reading the answers. Validation must be a separately generated batch after the candidate is frozen.

### 6.4 Verify the transport itself

Appendix C treats `Range.Value2` as sufficient exact-input transport. Appendix E records a counterexample: subnormal inputs were flushed during injection.

Record:

```text
requested bits
submitted representation
read-back bits
formula and referenced cells
captured result representation
```

Where effective kernel inputs cannot be observed directly, record that limitation.

Microsoft’s public documentation describes exceptional-value and subnormal behavior that differs from ordinary IEEE arithmetic, which reinforces the need to separate transport, kernel, and publication semantics. [3]

Also retain workbook calculation settings, especially precision-as-displayed, along with the build and Compatibility Version.

### 6.5 Use fingerprints as hypotheses

| Signature | Possible explanation | Discriminating experiment |
|---|---|---|
| Rare midpoint-related one-bit differences | Double rounding or a store | Generate inputs where explicit precision models differ while other operations are controlled |
| Abrupt residual change | Branch, reduction interval, helper switch | Locate adjacent inputs and vary another parameter to distinguish raw-input from transformed-coordinate guards |
| Cancellation amplifies a discrepancy | Association, store placement, inaccurate helper | Change one cancellation term independently; compare explicit graphs |
| Exponent or mantissa periodicity | Reduction table, constant precision, reconstruction | Repeat significand patterns across binades and test load variants |
| Plateaus in published answers | Intermediate lattice or iteration publication | Analyze exact reachable sets; vary a controlled multiplier or target |
| Permutation dependence | Fold order, grouping, compensation, pivoting | Reverse and interleave exact-control sequences |
| Root changes under exact cash-flow scaling | Absolute residual stop, derivative approximation, underflow | Compare candidate stopping predicates under powers-of-two scaling |
| Direct function differs from worksheet decomposition | Private helper or additional stores | Compare resident and explicitly stored compositions |

Compute bit distances with integer operations. Appendix E’s integer-bit-pattern-to-double error is exactly the kind of harness defect that can manufacture identities.

For branch recovery, a residual discontinuity only proposes an experiment. Determine the predicate, its coordinate, comparison strictness, and distinguishable boundary. A first mismatch at 20.5 does not establish that Excel branches at 20.5.

## 7. Synthesis, e-graphs, SMT, and symbolic learning

### 7.1 Use CEGIS as the search architecture

The practical loop is:

1. Generate programs from a bounded grammar.
2. Eliminate them on cached trusted observations.
3. Ask for inputs distinguishing survivors.
4. Extend the evidence.
5. Refine the grammar only when required.
6. Freeze surviving models and validate independently.

SMT is useful for specific tasks:

- Selecting among a bounded set of store or constant choices.
- Finding a distinguishing input for two arithmetic programs.
- Establishing local reachability.
- Extracting a small inconsistent subset of witness constraints.

For two candidates, the distinguishing query is:

\[
\exists x\in D:
\operatorname{Obs}(P,x)\ne\operatorname{Obs}(Q,x).
\]

A satisfying assignment proposes an Excel question. An unsatisfiable result establishes equivalence only under the encoded semantics and domain.

Excel itself is not an SMT equivalence oracle.

### 7.2 SMT floating point is not a complete x87 model

SMT-LIB’s `(_ FloatingPoint 15 64)` represents the canonical extended numerical lattice. Its interchange encoding has 79 bits because the leading significand bit is implicit; x87 storage has an explicit bit.

The theory does not supply x87 transcendental instructions, processor status, or NaN payload behavior. Also, `fp.eq` identifies positive and negative zero, so it is unsuitable as the sole exact-output predicate when zero sign is observable. [4]

A further trap is PC53 arithmetic consuming wider operands. Converting operands to a 53-bit format first can be wrong; performing a 64-bit operation and then converting can double-round. Such mixed cases need a justified bit-vector or exact-arithmetic encoding.

Current tooling also needs qualification: cvc5 marks formats beyond Float32/Float64 experimental, and Bitwuzla documents nonstandard formats as experimental because of SymFPU issues. Neither should become the sole ext80 proof authority in the first round. [5] [6]

A saved `unsat` result is solver-backed evidence. It becomes an independently checked proof only when the relevant encoding and proof steps are checked. Timeout and `unknown` exclude nothing.

### 7.3 Use e-graphs to generate alternatives

Keep two layers:

1. A real-algebra layer that generates mathematically equivalent forms.
2. An executable floating-point layer that preserves their different rounding behavior.

For example, binary64 evaluation distinguishes:

\[
(2^{53}+1)-2^{53}=0
\]

from:

\[
2^{53}+(1-2^{53})=1.
\]

A real-algebra e-class may contain both. It cannot license replacing one by the other during exact evaluation.

Within the executable layer, merge only with a proved floating-point identity under explicit preconditions. Even `x+0`, `x/x`, commutative canonicalization, and common-subexpression elimination need attention to exceptional values, stores, and effects.

### 7.4 A practical tool selection

| Tool | Role | Licence and qualification |
|---|---|---|
| **Existing cleared DnaCalc Fpu interface** | Candidate execution | Adapt the current interface; preserve its explicit semantic limits |
| **`num-bigint` / `num-rational`** | Independent rational and lattice checker | MIT or Apache-2.0; natural Rust components. [7] [8] |
| **Berkeley SoftFloat** | Independent basic arithmetic/ext80 reference | Berkeley three-clause licence; C FFI or separate executable. It does not emulate transcendental instructions. [9] |
| **MPFR** | Mathematical references and directed enclosures | LGPL-3.0-or-later; exponent and subnormal semantics require configuration. [10] |
| **Z3** | Small FP/BV constraints and distinguishing queries | MIT; an SMT-LIB subprocess avoids coupling the research design to a Rust binding. [11] |
| **cvc5** | SMT and exploratory syntax-guided synthesis | BSD-3-Clause; dependency licences vary by build, and extended formats need qualification. [12] |
| **Bitwuzla** | BV and standard-FP solving; qualified ext80 cross-check | MIT; nonstandard-format caveat above. [13] |
| **egg / egglog** | Optional structure generation and relational analyses | MIT; suitable Rust-based infrastructure, unnecessary for the initial small enumeration. [14] [15] |
| **Rosette** | External solver-aided DSL prototype | Two-clause BSD; Racket-based, not a supplied Rust/x87 solution. [16] |

One easy SoftFloat mistake deserves a pin: its `extF80_roundingPrecision` value **80** selects the full 64-bit significand; **64** selects reduced 53-bit precision. Its naming uses format widths. Also check whether the chosen port’s mode variables are thread-local before parallel evaluation. [9]

### 7.5 The useful machine-learning target is search policy

I would not start by training symbolic regression against Excel’s numerical outputs. A more credible research track is:

1. Generate known programs from the permitted sketches.
2. Generate observations that distinguish those programs.
3. Train a model to prioritize sketches, stores, guard templates, or probe families.
4. Test on held-out **program families**, not just unseen inputs.
5. Retain exact acceptance and a fair fallback enumeration.
6. Measure time to an admissible program, evaluator calls, and useful information per oracle round.

BUSTLE provides a concrete precedent for learning to prioritize bottom-up compositions from intermediate execution information. PROBE provides a precedent for learning a probabilistic search policy during enumeration. These are useful architectural ideas, not demonstrated solutions to this x87 identification problem. [17] [18]

A learned policy may suggest where to search. It must not preserve a contradicted complete program or permanently discard alternatives without marking the search incomplete.

For iterative functions, retain the same discipline. Search a small state machine containing the seed, coordinate, update, bracket, stop predicate, stores, cap, and returned iterate. Candidate traces are model traces; they are not observations of Excel’s internals. An inaccurate forward kernel must remain an explicit dependency rather than being compensated by a fitted stopping tolerance.

## 8. What I would do in the first three four-hour rounds

These are deliverable-driven investigation rounds, not a promise that PMT will be identified in twelve hours.

### Round 1: establish a trustworthy bounded experiment

**First hour**

- Freeze the exact scope, profile, observation revision, and helper representations.
- Adapt the existing witness loader and Fpu interface.
- Add the constructed mixed-precision and double-rounding tests.
- Check requested-versus-admitted input semantics.

**Second hour**

- Implement the final-product inverse-rounding/lattice checker.
- Reproduce the reported quotient gaps.
- Record the conflict with the retained quotient-store hypothesis.
- Define and hash the original six-skeleton grammar.

**Remaining two hours**

- Run exact first-kill elimination.
- Replay survivors progressively against the full scoped bank.
- Export a jointly selected batch of up to 64 questions.
- Checkpoint any unfinished coverage.

**Deliverable:** a checked local certificate, complete or explicitly incomplete enumeration coverage, survivor/kill records, and frozen questions.

If semantic validation or certificate reproduction fails, that is the round’s finding. Do not spend the remaining time racing an unreliable evaluator.

### Round 2: distinguish missing structure from wrong helpers

- Ingest returned observations with transport controls.
- Add the ordinary missing associations and simpler rearranged formula.
- Run shared-helper intersections for fixed-\((r,n)\) groups.
- Distinguish resident and stored helper representations.
- Select 64–128 questions targeting the unresolved model axes.

**Deliverable:** a narrower explanation of the failure: impossible suffix, inconsistent frozen helper, missing arithmetic structure, or multiple surviving models.

Widen one justified axis at a time. Avoid another large store-mask campaign around a family already excluded by a small exact certificate.

### Round 3: independent discrimination and a scoped conclusion

If several candidates remain, prioritize distinguishing probes or small arithmetic miters.

If one observational class remains:

- Freeze it.
- Generate validation through independent mechanisms and parameter regions.
- Replay all scoped cached observations.
- Check relevant excluded-arm controls.
- Validate interpreter/backend agreement on full-width traces.
- Prepare the exact production mapping and its dependencies.

If no candidate remains, publish the bounded exclusion and unresolved assumptions.

**Deliverable:** either a scoped empirically supported model with remaining obligations, or a useful negative result that prevents repeating the same search.

The first twelve hours should also leave reusable infrastructure. The interval checker, full-width evaluator interface, evidence accounting, and discriminator scheduler will be more valuable than a PMT-specific percentage improvement.

## 9. Organisation and methodological changes

### 9.1 Keep observations, semantics, search, and product separate

Use the existing architecture with four clearly owned responsibilities:

| Area | Contents |
|---|---|
| **Observation bank** | Immutable Excel observations, profile metadata, transport details, provenance, exposure history |
| **Numeric semantics** | Versioned Fpu contracts, backend adapters, conformance vectors |
| **Research tooling** | Grammars, enumeration, exact preimages, solver queries, candidate traces, first kills, probe selection |
| **Product library** | The selected executable graph, compact regression witnesses, ordinary published fallbacks with honest status |

Research may depend on the cleared product interface. Product should not depend on search machinery, candidate rankings, large answer tables, or learned policies.

Use `xtask` as the command surface and reuse the current loader and verdict encodings. A narrow research feature or adapter for private Fpu operations is preferable to copying the substrate.

Record dependencies as hypotheses with versions. If a helper model changes, its dependent identification claims become stale automatically; the Excel observations themselves remain valid.

### 9.2 The per-function status should describe a claim

At minimum, record:

- Function, surface, aliases, selected elements, and exact domain predicate.
- Excel/build/architecture/CPU profile and Compatibility Version.
- Transport, coercion, and publication contract.
- Claim strength: published fallback, conditional hypothesis, unique grammar survivor, independently validated model, or proved equivalence under a formal specification.
- Program, grammar, numeric-model, and evidence hashes.
- Enumeration bounds and actual coverage.
- Helper dependencies and unresolved assumptions.
- Constants and algorithm provenance.
- Surviving alternatives and equivalence status.
- First-kill records and family certificates.
- Discovery/validation exposure history.
- Frozen questions and predictions.
- Validation results and unresolved failures.
- Product mapping and replay results.
- The next experiment that would resolve each material uncertainty.

In particular, **“retained after rollback” must not become synonymous with “identified Excel program.”** Appendix D correctly allows retained published methods and partial program elements.

### 9.3 Amend the conflicting methodology explicitly

| Existing instruction | Replacement |
|---|---|
| Appendix B: rank by fit to witness banks | Order by declared plausibility and cost; eliminate complete programs on exact contradictions |
| Appendix C §10: exact-count/ULP candidate ranking | Separate admissibility, proposal priority, and residual diagnosis |
| §11.2: keep a beam of numerically best candidates | Prefer complete bounded enumeration; resource-pruned alternatives remain unsearched |
| §6.5: different models win in different regions | Regional patterns propose guard experiments; they do not establish dispatch |
| §9.7: zero survivors means a missing axis | Audit grammar, substrate, observation, profile, surface, and evaluator assumptions |
| §9.7: no discriminator found implies equivalence | Distinguish equal-on-witnesses, unresolved-within-budget, and proved equivalent |
| §4.1: Value2 preserves exact input | Record transport and read-back behavior explicitly |
| §11.1: prediction-vector deduplication | Preserve alternatives or regenerate them as witnesses expand |
| §17: one identified graph | State uniqueness within the declared grammar and independent validation separately |

The historical wall ledger should retain its chronology but attach current status to every claim: active hypothesis, refuted, superseded, or established within a stated scope. In particular, old high-percentage “closed” claims and later counterexamples must not remain equally authoritative.

**The first concrete milestone I would set is this:** reproduce and independently check the 734 quotient-gap certificates, reconcile their scope with the retained quotient store, and use grouped PMT observations to decide whether the frozen helper can coexist with any surviving combine. That resolves a real architectural uncertainty and gives the subsequent symbolic search a trustworthy foundation.

## References

1. [Jha et al., *Oracle-Guided Component-Based Program Synthesis*](https://www2.eecs.berkeley.edu/Pubs/TechRpts/2010/EECS-2010-15.html).
2. [Intel, *64 and IA-32 Architectures Software Developer’s Manual*, Volume 1, March 2025](https://cdrdv2-public.intel.com/851056/253665-087-sdm-vol-1.pdf), especially §§8.1.5.2, 8.3.9–8.3.10.
3. [Microsoft, *Floating-point arithmetic may give inaccurate results in Excel*](https://learn.microsoft.com/en-us/troubleshoot/microsoft-365-apps/excel/floating-point-arithmetic-inaccurate-result).
4. [SMT-LIB, *FloatingPoint theory*](https://smt-lib.org/theories-FloatingPoint.shtml).
5. [cvc5 documentation, *Options*](https://cvc5.github.io/docs/latest/options.html), `fp-exp`.
6. [Bitwuzla, *NEWS.md*](https://github.com/bitwuzla/bitwuzla/blob/main/NEWS.md), experimental floating-point formats.
7. [rust-num, *num-bigint*](https://github.com/rust-num/num-bigint).
8. [rust-num, *num-rational*](https://github.com/rust-num/num-rational).
9. [John R. Hauser, *Berkeley SoftFloat Release 3e: Library Interface*](https://www.jhauser.us/arithmetic/SoftFloat-3/doc/SoftFloat.html), including licence and rounding-precision documentation.
10. [GNU MPFR manual](https://www.mpfr.org/mpfr-current/mpfr.html).
11. [Z3 official FAQ](https://github.com/Z3Prover/z3/wiki/FAQ), licence.
12. [cvc5, *COPYING*](https://raw.githubusercontent.com/cvc5/cvc5/main/COPYING).
13. [Bitwuzla, *COPYING*](https://raw.githubusercontent.com/bitwuzla/bitwuzla/main/COPYING).
14. [egg, *LICENSE*](https://github.com/egraphs-good/egg/blob/main/LICENSE).
15. [egglog, *LICENSE*](https://github.com/egraphs-good/egglog/blob/main/LICENSE).
16. [Rosette, *LICENSE*](https://github.com/emina/rosette/blob/master/LICENSE).
17. [Odena et al., *BUSTLE: Bottom-Up Program Synthesis Through Learning-Guided Exploration*](https://research.google/pubs/bustle-bottom-up-program-synthesis-through-learning-guided-exploration/).
18. [Barke, Peleg, and Polikarpova, *Just-in-Time Learning for Bottom-Up Enumerative Synthesis*](https://cseweb.ucsd.edu/~hpeleg/probe-oopsla20.pdf).

[1]: https://www2.eecs.berkeley.edu/Pubs/TechRpts/2010/EECS-2010-15.html
[2]: https://cdrdv2-public.intel.com/851056/253665-087-sdm-vol-1.pdf
[3]: https://learn.microsoft.com/en-us/troubleshoot/microsoft-365-apps/excel/floating-point-arithmetic-inaccurate-result
[4]: https://smt-lib.org/theories-FloatingPoint.shtml
[5]: https://cvc5.github.io/docs/latest/options.html
[6]: https://github.com/bitwuzla/bitwuzla/blob/main/NEWS.md
[7]: https://github.com/rust-num/num-bigint
[8]: https://github.com/rust-num/num-rational
[9]: https://www.jhauser.us/arithmetic/SoftFloat-3/doc/SoftFloat.html
[10]: https://www.mpfr.org/mpfr-current/mpfr.html
[11]: https://github.com/Z3Prover/z3/wiki/FAQ
[12]: https://raw.githubusercontent.com/cvc5/cvc5/main/COPYING
[13]: https://raw.githubusercontent.com/bitwuzla/bitwuzla/main/COPYING
[14]: https://github.com/egraphs-good/egg/blob/main/LICENSE
[15]: https://github.com/egraphs-good/egglog/blob/main/LICENSE
[16]: https://github.com/emina/rosette/blob/master/LICENSE
[17]: https://research.google/pubs/bustle-bottom-up-program-synthesis-through-learning-guided-exploration/
[18]: https://cseweb.ucsd.edu/~hpeleg/probe-oopsla20.pdf

