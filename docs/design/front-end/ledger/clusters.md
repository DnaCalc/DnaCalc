# DNA Calc Front-End Design Ledger — Stage 2 Clusters

Generated from all six Stage 1 card files (`cards-framing.jsonl`, `cards-pro19-carbon.jsonl`, `cards-pro19-fable.jsonl`, `cards-pro19-gpt.jsonl`, `cards-pro19-neon.jsonl`, `cards-pro21.jsonl`). **Nothing is deleted**: every one of the 1,814 extracted idea cards belongs to exactly one cluster below, with symmetric `same-as` and `conflicts-with` links recorded in [ideas.jsonl](ideas.jsonl) and every conflict resolved in [decisions.md](decisions.md).

## Ledger Summary

- **Total cards:** 1814 (100% assigned, 0 deleted)
- **Total clusters:** 207 across 16 Design Book parts
- **`same-as` links:** 1107 symmetric pairs (29 cross-topic) across 399 equivalence components (1092 cards linked)
- **`conflicts-with` links:** 1117 symmetric pairs (197 cross-topic; 698 cards linked), 100% resolved by 93 canonical decisions in [decisions.md](decisions.md)
- **Decisions:** 93 total (14 owner/coordinator rulings from `DECISIONS_20261126.md` + 79 decider rulings; 0 left open to the owner)

| Part | Cards | Clusters | Cards with `same-as` | Cards with `conflicts-with` | Governing Decisions |
|---|---:|---:|---:|---:|---|
| I. Principles | 96 | 13 | 49 | 40 | D-01, D-03, D-08, D-15, D-16, D-17, D-18, D-19, D-20, D-21, D-88 |
| II. Conceptual model | 195 | 21 | 85 | 70 | D-04, D-12, D-14, D-18, D-22, D-23, D-24, D-25, D-26, D-27, D-28, D-29, D-30, D-31 |
| III. Shell | 94 | 12 | 47 | 25 | D-08, D-12, D-32, D-33, D-34, D-35, D-36, D-37, D-86 |
| IV. Classic | 466 | 31 | 337 | 92 | D-04, D-11, D-12, D-17, D-20, D-25, D-32, D-34, D-38, D-39, D-40, D-41, D-42, D-43, D-44, D-45, D-46, D-47, D-48, D-49 |
| V. Inspector | 104 | 12 | 79 | 37 | D-05, D-12, D-17, D-23, D-50, D-51, D-52, D-53, D-54 |
| VI. Notebook | 91 | 12 | 75 | 42 | D-06, D-12, D-23, D-28, D-55, D-56, D-57, D-58 |
| VII. Model map | 85 | 12 | 44 | 45 | D-03, D-12, D-59, D-60, D-61, D-62, D-63, D-64, D-70 |
| VIII. App | 49 | 8 | 36 | 31 | D-12, D-65, D-66, D-67, D-68, D-69 |
| IX. Formula Explorer | 89 | 12 | 66 | 33 | D-02, D-23, D-34, D-70, D-71, D-72, D-73, D-81 |
| X. Patterns | 132 | 18 | 98 | 76 | D-03, D-04, D-06, D-68, D-74, D-75, D-76, D-77, D-78, D-88 |
| XI. Big workbooks | 99 | 12 | 49 | 55 | D-12, D-48, D-65, D-79, D-80 |
| XII. Customisation and the skin API | 91 | 12 | 42 | 39 | D-07, D-09, D-10, D-12, D-16, D-81, D-82, D-83, D-84, D-85 |
| XIII. Persistence | 59 | 9 | 26 | 26 | D-05, D-06, D-13, D-14, D-83, D-86, D-87 |
| XIV. Automation | 44 | 7 | 26 | 26 | D-04, D-88, D-89 |
| XV. Testing the experience | 77 | 10 | 18 | 39 | D-02, D-03, D-14, D-90, D-91, D-92 |
| Appendices | 43 | 6 | 15 | 22 | D-01, D-02, D-03, D-04, D-06, D-09, D-14, D-91, D-93 |

## I. Principles

### CL-01-01: Core Product Thesis: Excel Under the Hood, Modern on the Surface

- **Summary:** DNA Calc keeps Excel's calculation semantics and plain .xlsx/.xlsm persistence intact while projecting one live workbook into five persona-driven views (Classic, Inspector, Notebook, Model map, App) plus Formula Explorer, making structure and state continuously visible without ever altering calculation.
- **Cards (10):** `FRAMING-0001`, `FRAMING-0048`, `P19-CARBON-0002`, `P19-CARBON-0003`, `P19-FABLE-0013`, `P19-FABLE-0014`, `P19-FABLE-0015`, `P19-FABLE-0017`, `P19-GPT-0001`, `P19-NEON-0003`
- **Sources:** FRAMING (2), P19-CARBON (2), P19-FABLE (4), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core-with-decision` — Open Chapter 1 of the Design Book with this unified product thesis, defining the five views + Formula Explorer and explicitly stripping z^-1 delay claims from the Model map overview.
- **Decisions:** [D-03](decisions.md#d-03)
- **`same-as` groups (2):** `FRAMING-0001` = `FRAMING-0048` = `P19-CARBON-0003`; `P19-FABLE-0017` = `P19-GPT-0001`
- **Agreements:**
  - Excel's calculation engine, formula semantics, and .xlsx/.xlsm file format remain the single uncompromised bedrock so every workbook opens and calculates identically in Excel.
  - All five built-in views (Classic, Inspector, Notebook, Model map, App) plus Formula Explorer (docked tool and standalone skin-0 build) project one shared live workbook under one shell with Classic always one click or key away.
  - 'Modern on the surface' means surfacing structure (blocks, runs, names, tables, flows) and live state (calc status, root errors, uncalculated values, external feeds, change provenance) as standing instruments rather than decorative paint or modal dialogs.
- **Conflicts:**
  - **Characterising Model map cycles as z^-1 discrete-time delay loops** (`P19-NEON-0003` vs `FRAMING-0001`, `FRAMING-0048`, `P19-GPT-0001`) → Resolved by **[D-03](decisions.md#d-03)**
  - *Cross-cluster `conflicts-with` links (resolved by D-03):* `P19-NEON-0003` vs `FRAMING-0053` (CL-07-07), `FRAMING-0068` (CL-01-03), `FRAMING-0141` (CL-07-08), `FRAMING-0168` (CL-07-08), `P19-GPT-0088` (CL-07-07), `P21-0064` (CL-07-07) (+2 more)

### CL-01-02: Standing Constraints, Excel-Only Capability Ceiling & Rejected Anti-Patterns

- **Summary:** Views may use only capabilities Excel itself exposes so every view stays Excel-compatible (and could run as an Excel add-in), shipping zero Microsoft artwork or bundled Microsoft fonts, running on WebView2/Leptos, enforcing Off/Guarded/Unguarded macro security, and rejecting simplified SaaS dialects and experimental mode sprawl.
- **Cards (8):** `FRAMING-0057`, `FRAMING-0110`, `FRAMING-0132`, `P19-CARBON-0004`, `P19-CARBON-0177`, `P19-GPT-0093`, `P19-NEON-0002`, `P19-NEON-0194`
- **Sources:** FRAMING (3), P19-CARBON (2), P19-GPT (1), P19-NEON (2)
- **Disposition:** `core-with-decision` — State these hard product boundaries, IP constraints (zero Microsoft artwork or bundled fonts; use generic `system-ui` rather than hardcoded `Segoe`), and rejected alternatives (P19-GPT-0093) prominently in Chapter 1 so later chapters never drift into non-Excel features or Microsoft styling.
- **Decisions:** [D-88](decisions.md#d-88)
- **`same-as` groups (3):** `FRAMING-0057` = `FRAMING-0110`; `P19-CARBON-0004` = `P19-NEON-0002`; `FRAMING-0132` = `P19-CARBON-0177` = `P19-NEON-0194`
- **Agreements:**
  - The engine does no more than Excel and views rely strictly on what Excel exposes (names, formulas, precedents/dependents, pre/post-calc values, RTD, data tables, iterative-calc settings); calculation events may drive visuals but never alter values.
  - Zero Microsoft icons, ribbon artwork, Office visual styling, or bundled Microsoft proprietary fonts (Calibri, Aptos, Segoe) may ship in DNA Calc.
  - Simplified SaaS formula dialects, un-Excel card walls, separate visual-programming runtimes, run-once script notebook cells, automatic App publication from inference, per-view undo stacks, hiding Classic, and keeping six experimental views as top-level modes are explicitly rejected.
  - Desktop uses WebView2 (FerroUI deferred per DECISIONS_20261126 #7), macro security levels are Off / Guarded / Unguarded, and planning uses milestones rather than calendar dates.
- **Conflicts:** None

### CL-01-03: Absolute Honesty, Excel Truth Over Polish & Accepted Push-Backs

- **Summary:** Honesty is an unyielding hard constraint: every view must reflect Excel-faithful calculation and state, disclose simplifications and omissions in place with counts, decline rather than fabricate, preserve Excel quirks honestly, and uphold the four owner-accepted push-backs (circular ref != delay, dependency != execution recording, tested save-route matrix, intact file != runs everywhere).
- **Cards (10):** `FRAMING-0068`, `P19-CARBON-0007`, `P19-FABLE-0018`, `P19-FABLE-0028`, `P19-GPT-0002`, `P19-GPT-0004`, `P19-GPT-0094`, `P19-NEON-0008`, `P19-NEON-0021`, `P21-0058`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-FABLE (2), P19-GPT (3), P19-NEON (2), P21 (1)
- **Disposition:** `core-with-decision` — Make this the non-negotiable Honesty Contract in Chapter 1, enumerating all four owner-accepted push-backs and the three top acceptance risks.
- **Decisions:** [D-03](decisions.md#d-03), [D-20](decisions.md#d-20)
- **`same-as` groups (3):** `P19-CARBON-0007` = `P19-FABLE-0018` = `P19-NEON-0008`; `P19-FABLE-0028` = `P19-NEON-0021`; `P19-GPT-0002` = `P19-GPT-0004`
- **Agreements:**
  - Honesty never yields to visual tidiness or polish: no view may silently drop rows, hide errors behind collapsed blocks or formatting, invent non-Excel numbers, silently rewrite formulas, or present heuristic inference as certainty.
  - Simplifications must disclose their coverage, counts, and exceptions in place (e.g., '11 alike, 1 differs'), missing observations must say 'not observed', and views that cannot represent a workbook honestly must decline with a concrete path forward.
  - Dependency graphs, highlights, and wire animations are strictly dependency explanations and are never presented as recordings of Excel's execution order; any optional recorded order is explicitly labelled 'DNA Calc's calculation order'.
  - The top three design risks—overstating what the UI knows, disrupting practiced Excel editing, and coupling views so heavy analysis stalls work—must serve as primary acceptance criteria.
- **Conflicts:**
  - **Drawing a delay glyph on circular references vs banning circular-ref-as-delay** (`P19-FABLE-0028` vs `FRAMING-0068`) → Resolved by **[D-03](decisions.md#d-03)**
  - *Cross-cluster `conflicts-with` links (resolved by D-03, D-20):* `FRAMING-0068` vs `P19-CARBON-0037` (CL-07-06), `P19-CARBON-0106` (CL-15-05), `P19-CARBON-0127` (CL-16-04), `P19-CARBON-0194` (CL-07-06), `P19-CARBON-0237` (CL-10-12), `P19-FABLE-0080` (CL-07-09) (+6 more); `P19-FABLE-0028` vs `FRAMING-0053` (CL-07-07), `FRAMING-0141` (CL-07-08), `FRAMING-0168` (CL-07-08), `P19-GPT-0088` (CL-07-07), `P21-0064` (CL-07-07), `P21-0069` (CL-07-08) (+1 more)

### CL-01-04: Quality Trade-Off Hierarchy, Practiced Excel Hands & Context Continuity

- **Summary:** Under Honesty as a hard constraint, quality trade-offs between familiarity, clarity, and density are calibrated per view—preserving Windows Excel muscle memory in Classic and Excel's formula/key vocabulary outside Classic, while carrying selection, draft, and history seamlessly across view switches.
- **Cards (7):** `P19-CARBON-0006`, `P19-FABLE-0019`, `P19-FABLE-0020`, `P19-FABLE-0052`, `P19-GPT-0003`, `P19-GPT-0005`, `P19-NEON-0007`
- **Sources:** P19-CARBON (1), P19-FABLE (3), P19-GPT (2), P19-NEON (1)
- **Disposition:** `core-with-decision` — Include Fable's per-view priority table right after the Honesty Contract in Chapter 1, paired with the practiced-hands and context-continuity laws.
- **Decisions:** [D-15](decisions.md#d-15)
- **`same-as` groups:** None within cluster
- **Agreements:**
  - Honesty sits strictly above all other qualities and is never traded away.
  - Classic protects practiced Windows Excel keyboard and mouse gestures; new commands live behind Alt+Q command search rather than stealing established Excel shortcuts, and browser shortcut collisions are disclosed with alternatives.
  - Outside Classic, familiarity means preserving Excel's formula language, editing keys, and terminology rather than imitating spreadsheet grid chrome.
  - Selection, active sheet, formula drafts, and navigation history survive view switches while each view retains its own camera/scroll position.
  - First-open value for Excel users requires zero startup modal dialogs, explicit 'as saved' labelling, root-grouped error counts, read-only hidden-sheet peeking, and true 'unchanged' state after undo.
- **Conflicts:**
  - **Single global quality ranking vs per-view quality priority matrix** (`P19-CARBON-0006`, `P19-NEON-0007` vs `P19-FABLE-0019`) → Resolved by **[D-15](decisions.md#d-15)**

### CL-01-05: Strand Flight-Deck Philosophy, Token Authority & Chrome/Paper Palette

- **Summary:** DNA Calc's visual language is Strand—a calm flight-deck aesthetic pairing petrol instrument chrome (#193F49) with bright working paper (#FFFFFF) and mist surround (#EDF1F2), teal structural accents (#318995 / #3DB6CF), and authoritative shipped tokens owned by the `apps/dna-strand` crate.
- **Cards (8):** `FRAMING-0070`, `FRAMING-0081`, `FRAMING-0082`, `FRAMING-0083`, `FRAMING-0092`, `FRAMING-0093`, `FRAMING-0112`, `P21-0001`
- **Sources:** FRAMING (7), P21 (1)
- **Disposition:** `core-with-decision` — Update the Strand token table in Chapter 1 (and sync `apps/dna-strand` in implementation) to separate `#FFFFFF` paper from `#EDF1F2` mist surround and drop legacy Rich-Tree references.
- **Decisions:** [D-16](decisions.md#d-16)
- **`same-as` groups (2):** `FRAMING-0070` = `FRAMING-0081`; `FRAMING-0082` = `FRAMING-0083` = `P21-0001`
- **Agreements:**
  - The `apps/dna-strand` crate is the canonical source of shipped design tokens; when prose docs and the crate diverge, the crate wins (until deliberately updated together).
  - Flight-deck temperament ('instruments, not ornaments'): petrol chrome (#193F49, depth ramp #12303A / #0C1C22) holds instruments so chrome never competes with data sitting on bright paper.
  - Teal (#318995 primary structure/references; #3DB6CF links/secondary structure and Formula Explorer accent) carries structural relationships, while sage/forest (#71A08A / #235949) serve only as soft value tints.
  - Strand v0.1 is the baseline visual language, with justified v0.2 refinements folded cleanly into the Design Book.
- **Conflicts:**
  - **Light-mode working paper ground (#EDF1F2 mist vs #FFFFFF crisp paper) and legacy Rich-Tree identity** (`FRAMING-0092`, `FRAMING-0093` vs `P21-0001`) → Resolved by **[D-16](decisions.md#d-16)**

### CL-01-06: Sacred Semantic Color Channels, Amber Focus Contrast & Signal Glyph Vocabulary

- **Summary:** Each semantic color has exactly one meaning—amber for active focus/now, teal for structure, green for freshly recalculated values, signal orange for external/volatile feeds, red strictly for evaluation errors—paired with WCAG-compliant dual-stroke focus rings, dark-safe ink tokens, and a single unified non-color glyph vocabulary that preserves user workbook formatting.
- **Cards (18):** `FRAMING-0085`, `FRAMING-0091`, `FRAMING-0094`, `FRAMING-0103`, `FRAMING-0133`, `P19-CARBON-0009`, `P19-CARBON-0174`, `P19-FABLE-0002`, `P19-FABLE-0006`, `P19-FABLE-0007`, `P19-FABLE-0008`, `P19-FABLE-0009`, `P19-FABLE-0012`, `P19-FABLE-0024`, `P19-FABLE-0036`, `P19-GPT-0008`, `P19-NEON-0015`, `P19-NEON-0193`
- **Sources:** FRAMING (5), P19-CARBON (2), P19-FABLE (8), P19-GPT (1), P19-NEON (2)
- **Disposition:** `core-with-decision` — Publish the resolved Strand v0.2 Semantic Signal & Glyph Table in Chapter 1 so Classic, Inspector, Notebook, Model map, App, and Formula Explorer reuse identical cues.
- **Decisions:** [D-17](decisions.md#d-17)
- **`same-as` groups (3):** `FRAMING-0085` = `FRAMING-0094` = `P19-FABLE-0024`; `FRAMING-0133` = `P19-GPT-0008`; `P19-FABLE-0009` = `P19-FABLE-0036`
- **Agreements:**
  - Five hues carry fixed, non-overlapping meanings: amber (#FFB301) for focus/selection/active calc, teal (#318995) for structure, green (#3F8B76) for fresh recalc, signal orange (#FF8001) for external/RTD/UDF/volatile, and red (#D02A23) strictly for evaluation errors—never introducing a new saturated hue for a new feature.
  - Structural provenance tints distinguish constants, formulas, and external/volatile feeds without decorative sheet/node color-coding.
  - Every semantic color signal pairs with a non-color cue (glyph, border geometry, or text label) and never overwrites the workbook's own cell formatting (such as red negative numbers or conditional formats).
  - Parity/evidence marks reuse standard semantic colors under a distinct twin-block glyph family (never amber, never a new hue), and Strand is treated as a versioned design language rather than 'final'.
- **Conflicts:**
  - **Amber focus-ring stroke geometry on light paper (raw amber vs amber-inner/dark-outer vs amber-outer/petrol-inner)** (`FRAMING-0094`, `P19-FABLE-0002`, `P19-NEON-0015` vs `FRAMING-0133`, `P19-GPT-0008`) → Resolved by **[D-17](decisions.md#d-17)**
  - **Conflicting non-color signal glyph tables (`[!]` error vs warning; `◈` vs `◊` vs `▸` external; `⊗/⊘` vs left-edge red tick)** (`P19-CARBON-0174`, `P19-NEON-0193` vs `P19-FABLE-0006`, `P19-FABLE-0007`, `P19-FABLE-0008`, `P19-FABLE-0009`, `P19-FABLE-0012`, `P19-FABLE-0036`) → Resolved by **[D-17](decisions.md#d-17)**
  - **Meaning of green ('fresh/verified' vs 'calculated for this revision, not audited')** (`FRAMING-0094` vs `P19-GPT-0008`, `P19-FABLE-0008`) → Resolved by **[D-17](decisions.md#d-17)**

### CL-01-07: Truth-Channel Line Styles: Stated vs Inferred vs Ghost

- **Summary:** Strand v0.2 adds line style as an orthogonal, color-free truth channel across all chips, boxes, and wires: solid lines with plain names denote facts stated in the Excel file, dashed teal lines with guillemets ‹label› denote program-inferred structure, and dotted teal lines with casual/slanted type denote uncommitted ghost previews and proposals.
- **Cards (4):** `P19-FABLE-0003`, `P19-FABLE-0004`, `P19-FABLE-0005`, `P19-FABLE-0035`
- **Sources:** P19-FABLE (4)
- **Disposition:** `core` — Present the three-row Truth-Channel Line-Style table alongside the Semantic Color Table in Chapter 1 (explicitly noting this Strand v0.2 refinement over Strand v0.1's single border treatment in FRAMING-0100) so Model map, Inspector, Notebook, and Agent proposals all inherit one consistent grammar.
- **`same-as` groups:** None within cluster
- **Agreements:**
  - Stated file truth, heuristic program inference, and uncommitted previews/proposals must never look identical.
  - Encoding epistemic status via line style (solid = file fact, dashed + ‹label› = inferred, dotted + casual/slanted type = ghost/uncommitted proposal) communicates honesty without introducing a single new hue, refining Strand v0.1's single border treatment (see FRAMING-0100 in CL-01-09).
- **Conflicts:** None

### CL-01-08: Quiet Surface, Honest Instruments & Abnormal-Only Status Signaling

- **Summary:** Following Pro 20 Change 1, a healthy current workbook stays visually quiet—displaying only a small steady mark in the Strip—while abnormal states (pending, stale, as-saved, previous, error, missing source, or long calculation) surface immediately with explicit words and symbols, keeping revision numbers in History and print footers.
- **Cards (4):** `FRAMING-0088`, `FRAMING-0136`, `P19-GPT-0006`, `P21-0005`
- **Sources:** FRAMING (2), P19-GPT (1), P21 (1)
- **Disposition:** `core-with-decision` — Make 'Quiet surface with honesty underneath' a headline principle in Chapter 1 and enforce it across all Shell/Strip specs.
- **Decisions:** [D-18](decisions.md#d-18)
- **`same-as` groups (1):** `FRAMING-0136` = `P21-0005`
- **Cross-cluster `same-as`:** `FRAMING-0136` = `P19-GPT-0039` (CL-02-12); `P21-0005` = `P19-GPT-0039` (CL-02-12)
- **Agreements:**
  - Status readouts live in dedicated instruments (Strip and Inspector) rather than cluttering the working paper.
  - Attention is spent deliberately: dense controls serve hands while quiet paper serves reading, and abnormal states always carry distinct words and non-color symbols.
- **Conflicts:**
  - **Always-on verbose status readouts vs quiet steady-state mark (Pro 20 Change 1)** (`FRAMING-0088` vs `FRAMING-0136`, `P21-0005`) → Resolved by **[D-18](decisions.md#d-18)**

### CL-01-09: Typography, Block Geometry & Two-Density System

- **Summary:** Every view shares one variable typeface family (Recursive sans-linear with tabular figures + Recursive Mono on the same skeleton, plus Source Serif 4 strictly for Notebook prose), one base-pair rounded-block geometry (0/5/9/14 px radii and 2–22 px gap scale), and exactly two density modes: Working (22 px hands-density) and Reading (~68 ch generous leading).
- **Cards (7):** `FRAMING-0084`, `FRAMING-0086`, `FRAMING-0099`, `FRAMING-0100`, `P19-CARBON-0010`, `P19-NEON-0009`, `P19-NEON-0011`
- **Sources:** FRAMING (4), P19-CARBON (1), P19-NEON (2)
- **Disposition:** `core-with-decision` — Include the Typography, Block Geometry, and Two-Density specification in Chapter 1, noting that `system-ui` replaces hardcoded `Segoe UI Variable` (per CL-01-02), border line styles encode truth (per CL-01-07), and focus uses the dual-stroke ring (per CL-01-06).
- **Decisions:** [D-19](decisions.md#d-19)
- **`same-as` groups (1):** `P19-CARBON-0010` = `P19-NEON-0011`
- **Agreements:**
  - UI and tabular numeric data use Recursive (sans-linear, `tnum`); formulas and addresses use Recursive Mono sharing the exact same skeleton (casual axis reserved for ghost text and hints); prose annotation uses Source Serif 4 (~68 ch measure).
  - Modular type scale (11, 12.5, 13.5, 15.5, 18, 22, 27 px), weights 480/560/650, +0.12em uppercase tracking, and `text-wrap: balance` on headings apply consistently.
  - Base-pair block geometry unifies corner radii (grid cells 0 px, chips/tokens 5 px, cards 9 px, panels/stage corners 14 px) and the gap scale (2, 4, 6, 10, 14, 22 px).
  - Exactly two density modes exist—Working and Reading—with no third medium density mode.
- **Conflicts:**
  - **Which views use Reading density (Notebook/published only vs Notebook and App)** (`P19-CARBON-0010`, `P19-NEON-0011` vs `FRAMING-0086`, `FRAMING-0099`) → Resolved by **[D-19](decisions.md#d-19)**

### CL-01-10: Motion Discipline & Recalculation Ripple Bounds

- **Summary:** Only three meaningful transitions may animate—state settle, dependency recalculation ripple, and cross-view continuity halo—each 120–160 ms with a 400 ms total ripple cap (block-stepped beyond 200 items), instant 0 ms under `prefers-reduced-motion`, and no marching-ants copy animation.
- **Cards (3):** `FRAMING-0101`, `P19-CARBON-0175`, `P19-FABLE-0038`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-FABLE (1)
- **Disposition:** `core-with-decision` — State the 3-animation law, 400 ms ripple cap, topological-dependency framing, reduced-motion zeroing, and static copy border in Chapter 1.
- **Decisions:** [D-20](decisions.md#d-20)
- **`same-as` groups:** None within cluster
- **Cross-cluster `same-as`:** `P19-FABLE-0038` = `P19-FABLE-0395` (CL-04-20)
- **Agreements:**
  - Only three animations are sanctioned in the entire application: focus/selection state settle, recalculation ripple, and cross-view continuity halo.
  - Individual transition durations are 120–160 ms on a standard deceleration curve, collapsing to instant 0 ms state transitions whenever `prefers-reduced-motion: reduce` is active.
- **Conflicts:**
  - **Ripple semantics ('real evaluation_order' vs dependency propagation) and duration cap / marching-ants ban** (`FRAMING-0101` vs `P19-CARBON-0175`, `P19-FABLE-0038`) → Resolved by **[D-20](decisions.md#d-20)**

### CL-01-11: Contrast Law & Non-Color Accessibility Check

- **Summary:** Every foreground/background token pair across all three built-in themes (`cockpit-light`, `cockpit-dark`, `high-contrast`) and custom skins must pass automated WCAG 2.2 AA contrast checks (>=4.5:1 text/small glyphs and chrome ink on petrol, >=3.0:1 large text, borders, focus rings, and graphical badges) using scoped `-ink` tokens, with zero information carried by color or motion alone.
- **Cards (7):** `FRAMING-0096`, `FRAMING-0097`, `P19-CARBON-0173`, `P19-FABLE-0001`, `P19-FABLE-0519`, `P19-FABLE-0521`, `P19-NEON-0192`
- **Sources:** FRAMING (2), P19-CARBON (1), P19-FABLE (3), P19-NEON (1)
- **Disposition:** `core-with-decision` — Make the automated WCAG 2.2 AA contrast check and no-color-alone/no-motion-alone doctrine normative acceptance rules in Chapter 1.
- **Decisions:** [D-21](decisions.md#d-21)
- **`same-as` groups (2):** `FRAMING-0096` = `P19-FABLE-0001` = `P19-FABLE-0521`; `FRAMING-0097` = `P19-FABLE-0519` = `P19-NEON-0192`
- **Agreements:**
  - Surface tones are never used directly as text or small glyphs; per-theme `-ink` variants (including dark-paper red `#E4625C` and amber ink) and scoped chrome inks enforce contrast across `cockpit-light`, `cockpit-dark`, and `high-contrast`.
  - Automated build-time contrast assertions verify >=4.5:1 for normal text/small glyphs/chrome ink and >=3.0:1 for large text, borders, focus rings, and non-text marks on all built-in themes and user/agent skins.
  - No information is ever carried by color alone or motion alone; every cue remains fully legible in grayscale and with motion disabled.
- **Conflicts:**
  - **WCAG 2.1 AA vs WCAG 2.2 AA baseline** (`P19-CARBON-0173` vs `P19-NEON-0192`, `FRAMING-0097`) → Resolved by **[D-21](decisions.md#d-21)**

### CL-01-12: Keyboard Operability, Screen Readers, Zoom Reflow & Touch Scope

- **Summary:** 100% of features across all five views, Formula Explorer, Quarantine Bar, and Agent review must be operable by keyboard alone with WAI-ARIA semantics and polite live announcements, clean reflow at 200% zoom on 1280x720 (with independent 10%–400% stage zoom), and click/touch-safe hit targets (>=36 px in Reading density, no hover-only primary actions) while deferring dedicated touch/pen authoring gestures.
- **Cards (7):** `P19-CARBON-0178`, `P19-FABLE-0157`, `P19-FABLE-0517`, `P19-FABLE-0518`, `P19-FABLE-0520`, `P19-NEON-0234`, `P19-NEON-0238`
- **Sources:** P19-CARBON (1), P19-FABLE (4), P19-NEON (2)
- **Disposition:** `core-with-decision` — Place the keyboard, screen-reader, zoom/reflow, and touch-ergonomics contract in Chapter 1's Accessibility section.
- **Decisions:** [D-21](decisions.md#d-21)
- **`same-as` groups (2):** `P19-CARBON-0178` = `P19-FABLE-0517`; `P19-FABLE-0518` = `P19-NEON-0234`
- **Agreements:**
  - Every feature and storyboard across all five views, Formula Explorer, and Agent review passes 100% by keyboard alone.
  - Screen readers receive WAI-ARIA `grid`, `tree`, `region`, `tablist`, and document semantics (cell address/value/formula/state in Classic; reads/read-by relationships in other views) plus polite, non-interrupting live announcements for recalculation and error-count changes.
  - Stage zoom spans 10%–400% independently of UI scale; at 200% zoom on a 1280x720 viewport no functionality is lost and every non-Classic view reflows cleanly.
- **Conflicts:**
  - **Touch and pen support: fully deferred vs baseline Reading-density ergonomics now** (`P19-FABLE-0157` vs `P19-NEON-0238`) → Resolved by **[D-21](decisions.md#d-21)**

### CL-01-13: Design Book Governance, Coherence & Positioning Hook

- **Summary:** Owner and decider rulings in DECISIONS_20261126 override all prior drafts; UX specifications stay free of internal implementation jargon, converge on one crisp coherent design with every rejected option documented in Appendix Z, and replace 'spreadsheet for your workbooks' with a stronger product hook.
- **Cards (3):** `FRAMING-0052`, `FRAMING-0164`, `FRAMING-0173`
- **Sources:** FRAMING (3)
- **Disposition:** `core-with-decision` — Use as the editorial charter in the Design Book preface and resolve the replacement website hook in the Website & Positioning chapter (per DECISIONS_20261126 #8).
- **Decisions:** [D-01](decisions.md#d-01), [D-08](decisions.md#d-08)
- **`same-as` groups:** None within cluster
- **Cross-cluster `same-as`:** `FRAMING-0173` = `FRAMING-0165` (CL-03-06)
- **Agreements:**
  - DECISIONS_20261126 rulings take absolute precedence over every earlier brief, Pro 19 answer, and Pro 21 mockup.
  - The Design Book defines one tight, coherent UX (never a grab-bag of overlapping features) and parks every rejected alternative in Appendix Z with an explicit rationale.
  - UX narrative focuses on user experience, design philosophy, and concrete flows rather than internal engine implementation trivia ('inside baseball').
  - The old website tagline 'spreadsheet for your workbooks' is retired in favour of a sharper hook foregrounding modern views, Excel parity, automation, cross-platform reach, and agentic customisation.
- **Conflicts:** None

## II. Conceptual model

### CL-02-01: Five-View Projection Architecture & Model/Presentation Split

- **Summary:** One open workbook is split along a single clean seam—the standard OOXML Excel Workbook Model versus the CustomXML Presentation Layer—and projected into five persona-driven views (Classic, Inspector, Notebook, Model map, App) plus Formula Explorer and user/agent skins, sharing one live state, one selection, one clock (when present), and one undo history.
- **Cards (11):** `FRAMING-0004`, `FRAMING-0006`, `FRAMING-0047`, `FRAMING-0061`, `FRAMING-0114`, `FRAMING-0123`, `FRAMING-0134`, `P19-FABLE-0160`, `P19-GPT-0231`, `P19-NEON-0041`, `P21-0010`
- **Sources:** FRAMING (7), P19-FABLE (1), P19-GPT (1), P19-NEON (1), P21 (1)
- **Disposition:** `core` — Anchor Chapter 2 (Conceptual Model) with the Model vs Presentation seam diagram, the five-view consolidation map (P19-FABLE-0160), and the shared-state invariant.
- **`same-as` groups (2):** `FRAMING-0061` = `P21-0010`; `FRAMING-0006` = `FRAMING-0114`
- **Agreements:**
  - Every view inside an open workbook shares one Excel model state, one selection, one clock (when present), and one undo history; switching views changes presentation only and leaves workbook calculation content byte-for-byte identical (R-1).
  - A view may perform only operations a user could perform in Excel plus arranging its own presentation layout, stored in an Excel-preserved CustomXML part.
  - The six earlier experimental views consolidate cleanly into five built-in views plus shared panels: Grid -> Classic, Notebook -> Notebook, Canvas -> Model map, Atlas -> Model map at sheet zoom + Overview, Tree -> Outline panel, and Ledger -> Notebook frames + Inspector Calc list.
  - Layer 4 comprises the five built-in persona views plus declarative skins authored by users or AI agents, alongside Formula Explorer (skin 0) docked inside DNA Calc and shipped standalone.
- **Conflicts:** None

### CL-02-02: Universal Target Ontology: Native Excel Objects, Blocks & View Items

- **Summary:** All five views operate over a single explicit ontology of workbook and view objects—Cells/Ranges (including spills), Names (workbook/sheet-scoped; dots are plain characters, not namespaces), Tables/Arrays, Workbook Objects (charts, shapes, controls), Inferred Blocks (explicit member sets creating no formula or scope), and Presentation-Only View Items—where every bound item reveals its exact source in one action.
- **Cards (8):** `P19-GPT-0027`, `P19-GPT-0028`, `P19-GPT-0029`, `P19-GPT-0030`, `P19-GPT-0031`, `P19-GPT-0032`, `P19-GPT-0232`, `P19-GPT-0300`
- **Sources:** P19-GPT (8)
- **Disposition:** `core` — Include the Target Ontology Table in Chapter 2 before the Address Grammar so every later view chapter uses identical terms for Cells, Spills, Names, Tables/Arrays, Objects, Blocks, and View Items.
- **`same-as` groups:** None within cluster
- **Agreements:**
  - Native Excel entities (Cell/Range, Name, Table/Array, Workbook Object) preserve exact Excel identity, multi-area active-cell rules, scope, and anchor/shape semantics across every view.
  - A Block is strictly a view grouping over an explicit (possibly discontiguous) member set; expanding it lists all members and exceptions, and grouping alone creates no Excel formula, defined name, or scope.
  - Dotted text inside a Defined Name (e.g., `Stock.ReorderPoint`) is an ordinary Excel identifier character rather than a new language namespace; real workbook vs sheet scope remains visible.
  - A View Item (card, paragraph, group, port, binding, panel layout) has its own presentation identity—never masquerading as a cell address—and every bound value/table/port/output reveals its exact backing source in one action, while source-less prose is explicitly marked as annotation.
  - The system tracks object identity across edits and reopens so views know honestly when a formerly bound source no longer exists rather than guessing nearby replacements (C-1).
- **Conflicts:** None

### CL-02-03: Canonical Address Grammar, Unified Address Box & Human-Agent Parity

- **Summary:** Humans (via Ctrl+G/F5/Name Box/Alt+Q), skins, macros, and AI agents share one canonical Target Address grammar covering all seven target kinds, keeping native Excel syntax untouched while giving non-Excel objects (sheet objects, inferred blocks, view items) unambiguous, collision-free addressing.
- **Cards (12):** `FRAMING-0089`, `P19-CARBON-0047`, `P19-CARBON-0048`, `P19-FABLE-0119`, `P19-FABLE-0120`, `P19-FABLE-0121`, `P19-FABLE-0122`, `P19-GPT-0033`, `P19-NEON-0006`, `P19-NEON-0042`, `P19-NEON-0195`, `P19-NEON-0265`
- **Sources:** FRAMING (1), P19-CARBON (2), P19-FABLE (4), P19-GPT (1), P19-NEON (4)
- **Disposition:** `core-with-decision` — Publish the 7-row Canonical Address Grammar table in Chapter 2 and use it verbatim in every mockup, storyboard, and agent tool contract.
- **Decisions:** [D-22](decisions.md#d-22)
- **`same-as` groups (1):** `FRAMING-0089` = `P19-CARBON-0047` = `P19-FABLE-0119` = `P19-NEON-0006`
- **Agreements:**
  - What a human types into the Address Box (`Ctrl+G` / `F5`) is the exact canonical address an AI agent, skin, or script sends (Strand Tenet 7).
  - Native Excel targets use standard Excel syntax: Cell (`Monthly!H13`), Range (`Inputs!C5:D16`), Spill (`Monthly!C21#`), Defined Name (`ServicesShare`, `Store.Stock`), and Table/Structured Ref (`Orders`, `Orders[Amount]`, `Orders[@Date]`).
  - Whenever a view item stands for a model object, the Address Box displays the model address; for pure presentation items it displays the view path in muted ink with a `[View]` tag and leaves the formula bar empty.
  - Commands accepting Excel addresses continue accepting pure Excel syntax without ever injecting UI scope annotations into workbook formulas.
- **Conflicts:**
  - **Non-Excel address syntax for Inferred Blocks, Sheet Objects, and Presentation View Items** (`P19-CARBON-0048`, `P19-NEON-0042` vs `P19-FABLE-0120`, `P19-FABLE-0121`, `P19-GPT-0033`) → Resolved by **[D-22](decisions.md#d-22)**

### CL-02-04: Read-Only Formula Lens vs Bidirectional AST Scoped-Name Interpreter

- **Summary:** Two distinct, strictly AST-level transforms over `calc-fml` improve readability without ever breaking Excel semantics: (1) the optional Formula Lens is strictly read-only display formatting (with 'Rewrite as LET' an explicit previewed model edit), while (2) the Scoped-Name Presenter/Interpreter translates local shorthand inside named boxes/frames back to exact Excel defined names under OxFml W074 precedence rules before commit.
- **Cards (4):** `FRAMING-0012`, `FRAMING-0063`, `FRAMING-0080`, `FRAMING-0107`
- **Sources:** FRAMING (4)
- **Disposition:** `core-with-decision` — Document the two AST transforms separately in Chapter 2 so neither users nor implementers confuse display-only Formula Lens with box-local Scoped-Name expansion.
- **Decisions:** [D-23](decisions.md#d-23)
- **`same-as` groups:** None within cluster
- **Agreements:**
  - All formula presentation and name interpretation operate strictly on `calc-fml`'s parsed AST—never raw string replacement—so string literals, cell ranges, and function names are never corrupted.
  - Name and call resolution obeys the frozen OxFml W074 oracle matrix (distinguishing call vs bare value position, sheet vs workbook scope, built-ins winning call collisions, visible defined names winning non-built-in UDF collisions, and LET/LAMBDA local scoping).
  - The Formula Lens never silently mutates workbook formulas; refactoring a formula via 'Rewrite as LET' is always an explicit, previewed `[Model]` edit.
- **Conflicts:**
  - **Strictly read-only Formula Lens vs bidirectional Presenter/Interpreter rewriting** (`FRAMING-0063` vs `FRAMING-0012`, `FRAMING-0107`) → Resolved by **[D-23](decisions.md#d-23)**
  - *Cross-cluster `conflicts-with` links (resolved by D-23):* `FRAMING-0063` vs `P19-NEON-0031` (CL-05-06)

### CL-02-05: Cross-View Selection Continuity, Camera Preservation & Draft Survival

- **Summary:** Selection belongs to the workbook in model terms and translates across all five views with a 150 ms amber continuity halo (instant under reduced motion), independent per-view scroll/zoom cameras, explicit Reveal/Add handling when a source is absent from a destination layout, and lossless survival of in-progress formula drafts.
- **Cards (13):** `FRAMING-0072`, `P19-CARBON-0024`, `P19-CARBON-0179`, `P19-FABLE-0022`, `P19-FABLE-0029`, `P19-FABLE-0123`, `P19-FABLE-0124`, `P19-FABLE-0429`, `P19-GPT-0035`, `P19-GPT-0233`, `P19-GPT-0234`, `P19-NEON-0023`, `P19-NEON-0043`
- **Sources:** FRAMING (1), P19-CARBON (2), P19-FABLE (5), P19-GPT (3), P19-NEON (2)
- **Disposition:** `core-with-decision` — Specify the exact selection-translation state machine (exact match, coarser container, absent source, source-less prose, active draft) in Chapter 2.
- **Decisions:** [D-24](decisions.md#d-24)
- **`same-as` groups (3):** `P19-CARBON-0024` = `P19-CARBON-0179`; `P19-FABLE-0022` = `P19-FABLE-0029`; `P19-GPT-0035` = `P19-GPT-0233`
- **Agreements:**
  - One shared workbook selection, address field, and formula bar (`F2` edits the selected item's formula with Classic formula keys) persist across every view.
  - When the destination view renders the selected model target at a coarser granularity (e.g., a single cell inside a Model map block), it outlines the finest containing item, states the mapping in the Strip, and restores the exact cells upon returning to a finer view.
  - Switching views never silently commits or drops an active formula draft: it remains in an anchored editor or defers the switch with explicit Commit / Cancel choices.
  - Visual continuity uses a 150 ms amber Continuity Halo (`0 ms` instant outline under `prefers-reduced-motion`).
- **Conflicts:**
  - **Behavior when the selected model source is absent from the destination view's layout** (`P19-NEON-0043` vs `P19-GPT-0035`, `P19-GPT-0233`) → Resolved by **[D-24](decisions.md#d-24)**
  - **Viewport scrolling on view switch (always center target vs minimal scroll preserving camera) and latency target (100 ms vs 150 ms)** (`P19-NEON-0023`, `P19-NEON-0043`, `P19-CARBON-0179` vs `P19-FABLE-0124`, `P19-FABLE-0429`) → Resolved by **[D-24](decisions.md#d-24)**

### CL-02-06: View-Switching Shortcuts, Global Navigation, Preferred Views & Companion Grid

- **Summary:** Views switch via `Ctrl+Alt+1..5` (per DECISIONS_20261126 #12, with `Alt+Q` fallback), a companion Classic grid toggles alongside any view via `Ctrl+Alt+0` (max two panes), global navigation (`Ctrl+G`/`F5`, `Ctrl+[`/`Ctrl+]`, `Alt+←`/`Alt+→`, `F6`) works identically everywhere, and sheet preferred views show as tab badges without surprise auto-switching.
- **Cards (7):** `P19-CARBON-0049`, `P19-FABLE-0125`, `P19-FABLE-0428`, `P19-FABLE-0430`, `P19-FABLE-0431`, `P19-FABLE-0432`, `P19-NEON-0196`
- **Sources:** P19-CARBON (1), P19-FABLE (5), P19-NEON (1)
- **Disposition:** `core-with-decision` — Publish the global shortcut map (`Ctrl+Alt+1..5`, `Ctrl+Alt+0`, `Alt+Q`, `Ctrl+G`/`F5`, `Ctrl+[`/`]`, `Alt+←`/`→`, `F6`) in Chapter 2 and Appendix A.
- **Decisions:** [D-12](decisions.md#d-12)
- **`same-as` groups:** None within cluster
- **Agreements:**
  - From every view—App included—the backing cells in Classic are always one keystroke away (R-20).
  - Global navigation commands work identically across all five views: `F5`/`Ctrl+G`/Name box/`Alt+Q` jump to any address, `Ctrl+[`/`Ctrl+]` trace upstream/downstream, `Alt+←`/`Alt+→` walk navigation history across sheets and views, and `F6` cycles workspace regions.
  - A file's preferred view applies on first open, while a sheet's preferred view displays as a tab badge and never hijacks the user's current view on a sheet-tab click (R-19).
  - A side-by-side companion Classic grid shares selection bidirectionally with the active view, capped at two panes maximum.
- **Conflicts:**
  - **View-switching and companion-grid keyboard shortcuts (`Ctrl+1..5` vs `Alt+Shift+1..5` vs `Ctrl+Alt+1..5`)** (`P19-CARBON-0049`, `P19-FABLE-0428`, `P19-FABLE-0430` vs `P19-NEON-0196`) → Resolved by **[D-12](decisions.md#d-12)**
  - *Cross-cluster `conflicts-with` links (resolved by D-12):* `P19-CARBON-0049` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28); `P19-FABLE-0428` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28); `P19-FABLE-0430` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28); `P19-NEON-0196` vs `P19-GPT-0218` (CL-04-28)

### CL-02-07: Modeless 1-Bit Editing, Unified Edit Grammar & Preview Seam

- **Summary:** Across every view, exactly one bit separates SELECTED from EDITING (reflected simultaneously on the item, formula bar, and Strip), edits share one keyboard grammar (`type` to replace, `F2` edit, `Enter` commit, `Tab` commit-and-move, `Esc` lossless one-layer unwind), and structural or non-grid mutations preview their exact consequences across the Preview Seam before commit.
- **Cards (7):** `FRAMING-0087`, `P19-CARBON-0023`, `P19-CARBON-0180`, `P19-FABLE-0016`, `P19-FABLE-0023`, `P19-NEON-0005`, `P19-NEON-0198`
- **Sources:** FRAMING (1), P19-CARBON (2), P19-FABLE (2), P19-NEON (2)
- **Disposition:** `core-with-decision` — Define the 1-bit `SELECTED`/`EDITING` contract, `Esc` layer unwind order, and Preview Seam trigger table cleanly in Chapter 2.
- **Decisions:** [D-25](decisions.md#d-25)
- **`same-as` groups (2):** `FRAMING-0087` = `P19-CARBON-0180` = `P19-NEON-0198`; `P19-CARBON-0023` = `P19-NEON-0005`
- **Agreements:**
  - Modeless 1-bit editing (`SELECTED` vs `EDITING`) eliminates modal editing traps and shows the active state simultaneously on the target item, formula bar, and Strip.
  - One universal keyboard editing grammar (`type` to replace, `F2` edit, `Enter` commit, `Esc` revert, `Tab` commit and advance) governs every editable target across all views.
  - Pressing `Esc` unwinds strictly one layer (`Editing -> Selected -> Preview Seam Dismissed -> Open Drawer Closed`) and always restores exact prior workbook and view state.
- **Conflicts:**
  - **Preview Seam trigger scope: every edit everywhere vs structural/non-grid/high-blast-radius edits** (`P19-FABLE-0016` vs `P19-CARBON-0023`, `P19-NEON-0005`) → Resolved by **[D-25](decisions.md#d-25)**

### CL-02-08: Non-Grid Edit Semantics: Model vs Presentation Classification & Refusal Rules

- **Summary:** Every edit outside Classic is explicitly labelled `[Model]` or `[Presentation]` before commitment (showing exact Excel cells/names/settings or explicitly stating zero model changes), obeys Excel sheet protection, validation rules, and macro security, keeps view sorts/filters and prose strictly non-mutating, and refuses any gesture Excel cannot reproduce.
- **Cards (13):** `FRAMING-0062`, `P19-CARBON-0021`, `P19-FABLE-0030`, `P19-FABLE-0130`, `P19-FABLE-0131`, `P19-FABLE-0447`, `P19-FABLE-0448`, `P19-FABLE-0449`, `P19-FABLE-0453`, `P19-FABLE-0454`, `P19-GPT-0036`, `P19-NEON-0044`, `P21-0008`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-FABLE (8), P19-GPT (1), P19-NEON (1), P21 (1)
- **Disposition:** `core` — Make this four-category edit contract (Backing-Sheet Primacy, Model vs Presentation classification, View-Only guarantees, Refusal rule) a central pillar of Chapter 2.
- **`same-as` groups (3):** `FRAMING-0062` = `P19-GPT-0036` = `P21-0008`; `P19-CARBON-0021` = `P19-FABLE-0030` = `P19-FABLE-0448` = `P19-NEON-0044`; `P19-FABLE-0130` = `P19-FABLE-0449`
- **Agreements:**
  - Every review surface separates Model changes from Presentation changes under distinct headings with concrete before/after diffs (explicitly stating when zero cells, formulas, or names change).
  - Every `[Model]` change made in any view must be reproducible by a human in Excel, previewing the exact Excel cells, names, or settings it will write before commit (R-35, R-36); any gesture outside legal Excel mutations or pure presentation is refused with a plain reason and the nearest Excel equivalent.
  - Pure `[Presentation]` edits (layout, card order, folding, prose, view sort, view filter) live in CustomXML and leave the workbook's model hash bit-identical (R-37); view sorts/filters never alter Excel filter state or row order and always display their label and counts (R-41).
  - Prose may quote cells live, but typing a formula directly into prose is refused with an offer to add a real backed cell/formula to the workbook (R-42).
  - Every non-grid editor respects Excel sheet protection, data-validation rules/alert styles, macro security, and grouped-sheet disclosure, and never hides Classic.
- **Conflicts:** None

### CL-02-09: Non-Grid Structural Mutations: Formula Runs, Placement Rule & Deletion Safety

- **Summary:** Non-grid structural edits map deterministically to Excel operations: new backed items follow a transparent visible-sheet placement rule (label + value + name, editable in the Preview Seam), run edits propagate R1C1-identical formulas without silently clobbering exceptions, and `Delete` on a view item safely removes presentation by default while `Shift+Delete` deletes backing model cells through a blast-radius seam.
- **Cards (7):** `P19-CARBON-0050`, `P19-FABLE-0127`, `P19-FABLE-0128`, `P19-FABLE-0129`, `P19-FABLE-0450`, `P19-FABLE-0451`, `P19-FABLE-0452`
- **Sources:** P19-CARBON (1), P19-FABLE (6)
- **Disposition:** `core-with-decision` — Include the Non-Grid Structural Edit Matrix (create, edit run, rewire, rename, delete) in Chapter 2.
- **Decisions:** [D-26](decisions.md#d-26)
- **`same-as` groups (1):** `P19-FABLE-0128` = `P19-FABLE-0452`
- **Agreements:**
  - Creating model content outside the grid follows the visible placement rule: write a label cell, a value/formula cell, and (where the view works by names) a defined name in the next free row of its block/frame on a visible sheet—always shown and editable in the Preview Seam before commit (R-40).
  - Structural Model map gestures map strictly to Excel operations: rewiring replaces one formula reference with another, and renaming a box renames its defined name across references.
  - Formula runs with exceptions are always surfaced honestly as `'n alike, m differ'` across every view and print output, with the exception one step away (R-39).
- **Conflicts:**
  - **Pressing `Delete` on a Model map box: clear backing cell & name immediately vs safe presentation removal with explicit model-delete choice** (`P19-FABLE-0129` vs `P19-CARBON-0050`) → Resolved by **[D-26](decisions.md#d-26)**
  - **Editing a formula run that contains an exception cell: overwrite whole run vs preserve exceptions by default** (`P19-FABLE-0127`, `P19-FABLE-0450` vs `P19-CARBON-0050`, `P19-FABLE-0451`) → Resolved by **[D-26](decisions.md#d-26)**

### CL-02-10: Cross-View Undo/Redo Architecture, Macro Undo & Recovery Semantics

- **Summary:** Each workbook maintains one shared chronological `[Model]` undo history plus scoped per-view `[View]` presentation histories (all inspectable together via `Ctrl+Shift+Z`), so `Ctrl+Z` undoes whichever is newer between the last model edit and the focused view's last presentation edit without yanking the viewport or undoing invisible layouts; macro undo is designed for but not promised in v1 (per DECISIONS_20261126 #4).
- **Cards (12):** `P19-CARBON-0051`, `P19-CARBON-0182`, `P19-FABLE-0049`, `P19-FABLE-0082`, `P19-FABLE-0132`, `P19-FABLE-0500`, `P19-FABLE-0501`, `P19-GPT-0038`, `P19-GPT-0251`, `P19-GPT-0252`, `P19-NEON-0045`, `P19-NEON-0197`
- **Sources:** P19-CARBON (2), P19-FABLE (5), P19-GPT (3), P19-NEON (2)
- **Disposition:** `core-with-decision` — Specify the hybrid `[Model]` + per-view `[View]` undo model, `Ctrl+Shift+Z` unified inspector, crash-recovery boundaries, and Owner Ruling #4 on macro undo in Chapter 2.
- **Decisions:** [D-04](decisions.md#d-04), [D-27](decisions.md#d-27)
- **`same-as` groups (3):** `P19-CARBON-0051` = `P19-CARBON-0182` = `P19-NEON-0045` = `P19-NEON-0197`; `P19-FABLE-0082` = `P19-FABLE-0501`; `P19-GPT-0038` = `P19-GPT-0251`
- **Agreements:**
  - Undo is scoped per workbook, uses named steps carrying `[Model]` vs `[View]` kind and actor attribution, survives saving, and restores the Mast state to `'unchanged'` when undone back to opening content (P19-FABLE-0049).
  - Pure navigation, camera/zoom movement, tree folding, and Formula Explorer sandbox tests never pollute Undo (navigation uses `Alt+←`/`Alt+→` Back/Forward).
  - `Ctrl+Z` never unexpectedly yanks the user to another view; the Strip names what was undone and offers `[Jump]` if off-screen.
  - Local draft undo operates inside an active text/formula editor prior to commit, and crash recovery cleanly separates committed edits, unapplied drafts, and unaccepted proposals with zero double replay or auto-apply on reopen (R-22).
- **Conflicts:**
  - **Single global `[Model]+[View]` undo stack vs Shared `[Model]` history + per-view `[View]` presentation history** (`P19-CARBON-0051`, `P19-CARBON-0182`, `P19-GPT-0038`, `P19-GPT-0251`, `P19-NEON-0045`, `P19-NEON-0197` vs `P19-FABLE-0082`, `P19-FABLE-0132`, `P19-FABLE-0501`) → Resolved by **[D-27](decisions.md#d-27)**
  - **Promising single-step undo across a macro run in v1 vs designing for it without promising v1 delivery** (`P19-FABLE-0132`, `P19-FABLE-0500` vs `P19-GPT-0038`) → Resolved by **[D-04](decisions.md#d-04)**
  - *Cross-cluster `conflicts-with` links (resolved by D-04, D-27):* `P19-FABLE-0132` vs `FRAMING-0169` (CL-14-04), `P19-GPT-0079` (CL-14-04), `P19-GPT-0205` (CL-04-30), `P19-GPT-0207` (CL-04-30), `P19-GPT-0296` (CL-14-04), `P19-GPT-0312` (CL-02-20); `P19-FABLE-0500` vs `FRAMING-0169` (CL-14-04), `P19-GPT-0079` (CL-14-04), `P19-GPT-0205` (CL-04-30), `P19-GPT-0207` (CL-04-30), `P19-GPT-0296` (CL-14-04), `P19-GPT-0312` (CL-02-20); `P19-GPT-0038` vs `P19-CARBON-0120` (CL-14-04), `P19-CARBON-0128` (CL-16-04), `P19-CARBON-0206` (CL-14-04), `P19-CARBON-0223` (CL-02-20), `P19-CARBON-0241` (CL-10-16), `P19-FABLE-0207` (CL-14-04) (+2 more)

### CL-02-11: Semantic Clipboard & Cross-View Copy/Paste Matrix

- **Summary:** The clipboard carries the underlying semantic object (cells with source-origin relative refs, value frames, map boxes, or diagnostic rows) alongside standard Excel/TSV/HTML/picture formats; paste adapts cleanly to the destination view, keeps Formula lens text from silently becoming executable syntax, and previews any model-creating or cross-workbook paste in the Preview Seam.
- **Cards (11):** `P19-FABLE-0143`, `P19-FABLE-0502`, `P19-FABLE-0503`, `P19-FABLE-0504`, `P19-FABLE-0505`, `P19-FABLE-0506`, `P19-FABLE-0507`, `P19-FABLE-0570`, `P19-GPT-0037`, `P19-GPT-0249`, `P19-GPT-0250`
- **Sources:** P19-FABLE (8), P19-GPT (3)
- **Disposition:** `core-with-decision` — Publish the unified 5x4 Cross-View Clipboard Matrix (Fable R-90 + GPT R-19/R-20 safety rules) in Chapter 2.
- **Decisions:** [D-28](decisions.md#d-28)
- **`same-as` groups (2):** `P19-FABLE-0143` = `P19-FABLE-0502`; `P19-GPT-0037` = `P19-GPT-0249`
- **Agreements:**
  - Copy exports semantic workbook metadata alongside standard Excel cell/object formats, tab-separated text, HTML tables, and images (C-46, R-90).
  - Ordinary cell copy retains source-origin relative reference semantics; Copy formula text copies exact formula syntax with origin disclosed; Formula lens display text never silently becomes executable clipboard syntax (R-19).
  - Any paste that creates model content (cells, names, formulas) or crosses workbooks previews the target type, name clashes, and any new external links in the Preview Seam before committing, refusing ambiguous external bindings (R-20, R-90).
  - Pasting cells into Notebook creates a live-quoting frame (or text); pasting Trace/Issues rows into Notebook creates text with live quotes; pasting external tables into Notebook asks 'text or cells?'.
- **Conflicts:**
  - **Pasting a Model map box or view frame within the same workbook: duplicate presentation vs duplicate backing cells & names** (`P19-FABLE-0505` vs `P19-GPT-0250`) → Resolved by **[D-28](decisions.md#d-28)**

### CL-02-12: Orthogonal Cell/Workbook State Taxonomy, Error Purity & Live-Stream Honesty

- **Summary:** Workbook and cell states track strictly orthogonal axes—loading/ready, clean/dirty/calculating/cancelled, current/saved/previous/pending, source connected/stale/missing, and Excel formula error vs non-error—never inventing fake Excel error codes, keeping steady state quiet while surfacing root-grouped errors and pure, non-consuming preview/print snapshots for live RTD streams.
- **Cards (12):** `P19-CARBON-0052`, `P19-FABLE-0134`, `P19-FABLE-0439`, `P19-FABLE-0440`, `P19-FABLE-0444`, `P19-GPT-0039`, `P19-GPT-0238`, `P19-GPT-0239`, `P19-GPT-0241`, `P19-GPT-0242`, `P19-NEON-0046`, `P21-0028`
- **Sources:** P19-CARBON (1), P19-FABLE (4), P19-GPT (5), P19-NEON (1), P21 (1)
- **Disposition:** `core-with-decision` — Publish the Orthogonal State Matrix (Loading, Calculation, Freshness, External Source, Excel Error) and the live-stream purity rule (P21-0028) in Chapter 2.
- **Decisions:** [D-18](decisions.md#d-18)
- **`same-as` groups (1):** `P19-GPT-0039` = `P19-GPT-0238`
- **Cross-cluster `same-as`:** `P19-GPT-0039` = `FRAMING-0136` (CL-01-08), `P21-0005` (CL-01-08)
- **Agreements:**
  - Excel formula errors (`#REF!`, `#SPILL!`, `#NUM!`, `#DIV/0!`, etc.) remain real cell values intact; missing data providers, iteration limits, and loading statuses are strictly UI/host states and never invented Excel cell error codes (R-9).
  - Under Manual calculation, cells with modified inputs are visibly hatched as stale until recalculated (`F9`), and recalculations exceeding 300 ms display Strip progress, keep typing responsive, and abort cleanly on `Esc` (R-27, R-28).
  - Errors group by root origin with dependent counts so `Enter` jumps directly to the root cause (R-32).
  - Manual/automatic/iterative calculation settings and host scope stay visible and unchanged across view switches; live RTD/external panels display source time (if known), receipt time, connection status, and retained-history coverage, with pausing history observation strictly separate from stopping the feed (R-11, R-12).
  - Generating gallery previews or print snapshots must be strictly pure and never consume or advance a live RTD price observation or clock tick (P21-0028).
- **Conflicts:**
  - **Strip steady-state verbosity and revision display vs Pro 20 Change 1 quiet surface** (`P19-NEON-0046`, `P19-FABLE-0134`, `P19-GPT-0242` vs `P19-GPT-0039`, `P19-CARBON-0052`) → Resolved by **[D-18](decisions.md#d-18)**
  - **Shared diagonal hatch for `pending`/`stale`/`as saved` vs distinct accessible labels for `saved`/`previous`/`pending`/`current`** (`P19-FABLE-0134` vs `P19-GPT-0039`, `P19-GPT-0238`) → Resolved by **[D-18](decisions.md#d-18)**

### CL-02-13: Honest First-Open Behaviour, As-Saved Labelling & Non-Modal Notices

- **Summary:** Opening any workbook never raises blocking startup dialogs about links, macros, circular loops, or formats: saved cached values render immediately under an explicit `'as saved <date>'` mark until recalculated, post-recalc deltas against saved values are counted and linked, missing links/macros/manual-calc/hidden sheets summarise into one calm notice line and Overview, hidden/very-hidden sheets are peekable read-only, and Mast state words accurately reflect file identity.
- **Cards (7):** `P19-FABLE-0043`, `P19-FABLE-0438`, `P19-FABLE-0441`, `P19-FABLE-0442`, `P19-FABLE-0443`, `P19-FABLE-0445`, `P19-FABLE-0446`
- **Sources:** P19-FABLE (7)
- **Disposition:** `core` — Present the First-Open Honesty Contract (zero dialogs, as-saved band, post-recalc diff count, read-only hidden peek, Mast state machine) as a signature DNA Calc story in Chapter 2.
- **`same-as` groups (1):** `P19-FABLE-0043` = `P19-FABLE-0438`
- **Agreements:**
  - Opening a workbook never interrupts the user with modal pop-ups about external links, macros, circular references, or file formats (R-30); all findings consolidate into one non-modal notice line and Overview (R-31).
  - Saved cached values display within budget under an explicit `'as saved <date>'` hatch/mark until recalculated, and under manual calc nothing recalculates behind the user's back (R-26).
  - After the first recalculation of an opened file, DNA Calc reports the exact count of values that changed compared to the saved cache and links straight to the diff list (R-29).
  - Hidden and very-hidden sheets are listed and can be inspected read-only without mutating the workbook (R-33).
  - The Mast state word is strictly one of `new`, `unchanged`, `saved`, `edited`, `'saved, layout edited'`, or `read-only` (and undoing back to opening content returns to `unchanged`, R-34).
- **Conflicts:** None

### CL-02-14: AI Agent Invocation, Permission Boundaries & Visual Proposal Grammar

- **Summary:** The AI agent operates strictly through the same canonical addresses and commands as a human (zero back doors), is invoked cleanly via `Alt+Q` fall-through or `Tools > Agent` (no Excel shortcut collisions), defaults to read-only `Propose` mode with explicit consent required for trust-boundary actions, and renders uncommitted proposals using Strand's dotted-teal ghost border with a light hairline hatch and `[Agent]` badge—never amber.
- **Cards (15):** `P19-CARBON-0018`, `P19-CARBON-0122`, `P19-FABLE-0039`, `P19-FABLE-0108`, `P19-FABLE-0115`, `P19-FABLE-0137`, `P19-FABLE-0138`, `P19-FABLE-0139`, `P19-FABLE-0142`, `P19-FABLE-0493`, `P19-FABLE-0498`, `P19-FABLE-0499`, `P19-GPT-0099`, `P19-GPT-0280`, `P19-NEON-0016`
- **Sources:** P19-CARBON (2), P19-FABLE (10), P19-GPT (2), P19-NEON (1)
- **Disposition:** `core-with-decision` — Define the Agent Trust & Visual Contract (invocation via `Alt+Q` / `Tools > Agent`, zero back doors, dotted-teal ghost grammar, 3 permission modes, hard trust boundaries) in Chapter 2.
- **Decisions:** [D-29](decisions.md#d-29), [D-30](decisions.md#d-30)
- **`same-as` groups (3):** `P19-FABLE-0039` = `P19-FABLE-0498`; `P19-FABLE-0115` = `P19-FABLE-0142` = `P19-FABLE-0499`; `P19-FABLE-0139` = `P19-FABLE-0493`
- **Agreements:**
  - The agent uses only the same unified addresses and workbook commands as a human user with zero private back doors (P19-FABLE-0137).
  - Default agent permission is `Propose`: nothing is written to the workbook model until explicitly accepted by the user (R-81).
  - Agent activity shows calmly in a Mast status chip (idle, reading, working, proposals waiting) without stealing focus or cluttering the stage, and `Esc` stops an active run cleanly.
  - High-impact trust actions—changing calculation settings, enabling macros, loading add-ins, saving, or sending workbook data to an external provider—always require explicit user permission and can never be granted by a workbook, skin, or agent customisation (R-50, R-87).
  - Visual styling/layout requests travel as reversible try-on skins that cannot mutate formulas or escalate permissions.
- **Conflicts:**
  - **Uncommitted agent proposal visual grammar (dotted teal vs dashed teal-600 vs sage-400 hatch)** (`P19-CARBON-0018`, `P19-NEON-0016` vs `P19-FABLE-0039`, `P19-FABLE-0498`) → Resolved by **[D-29](decisions.md#d-29)**
  - **Agent autonomy ceiling: always stage interactively vs optional per-workbook 'Apply, then review' & delegated scopes** (`P19-CARBON-0122` vs `P19-FABLE-0139`, `P19-GPT-0099`) → Resolved by **[D-30](decisions.md#d-30)**

### CL-02-15: Agent Staged Proposals, Hunk Review & Subset Impact Preview

- **Summary:** Interactive agent mutations arrive in a non-blocking copy-on-write Staged Proposal grouped into discrete, labelled semantic units (separating Model changes, Presentation changes, and flagged unsupported suggestions), with before/after diffs that reserve red strictly for errors, inert checkboxes, and `'Review selected'` (`P`) computing exact combined downstream arithmetic and ghost overlays for the chosen subset before commitment.
- **Cards (14):** `P19-CARBON-0044`, `P19-CARBON-0053`, `P19-CARBON-0204`, `P19-FABLE-0109`, `P19-FABLE-0110`, `P19-FABLE-0112`, `P19-FABLE-0116`, `P19-FABLE-0117`, `P19-GPT-0025`, `P19-GPT-0273`, `P19-GPT-0274`, `P19-NEON-0040`, `P21-0110`, `P21-0111`
- **Sources:** P19-CARBON (3), P19-FABLE (5), P19-GPT (3), P19-NEON (1), P21 (2)
- **Disposition:** `core-with-decision` — Detail the unified Changes / Agent Proposal Review flow (M7.1 + S14) in Chapter 2, highlighting no-red diffs, inert ticking, and exact subset preview.
- **Decisions:** [D-30](decisions.md#d-30), [D-31](decisions.md#d-31)
- **`same-as` groups (3):** `P19-CARBON-0044` = `P19-CARBON-0053` = `P19-CARBON-0204`; `P19-GPT-0025` = `P21-0110`; `P19-GPT-0274` = `P21-0111`
- **Agreements:**
  - Agent proposals never write directly to cells: they land in a staged copy-on-write proposal batch collapsed into named semantic/R1C1-run hunks, separating Model changes, Presentation changes, and explicitly flagged unsupported suggestions (plus 'Not proposed' notes).
  - Proposal diffs reserve red strictly for evaluation errors—using `−`/`+`, Before/After labels, and strike/underline rather than red/green git diff colors.
  - Users review proposals non-blockingly by keyboard (`J`/`K` or arrows navigate, `Space` toggles unit selection, `P` previews ghosts on stage, `R` rejects with optional note/noted-exception, `Ctrl+Enter` applies ticked units, `Esc` exits preview without losing ticks, `Ctrl+C` copies plain-text before/after summary), with full ARIA checkbox-list semantics.
  - Every proposal discloses author, base revision, source scope, before/after changes, model-vs-presentation classification, and computed calculation impact before any write (R-43).
- **Conflicts:**
  - **Agent invocation shortcut (`Ctrl+I` vs `Ctrl+J` vs `Alt+Q` / `Tools > Agent`)** (`P19-NEON-0040`, `P19-CARBON-0044` vs `P19-GPT-0025`) → Resolved by **[D-30](decisions.md#d-30)**
  - **Per-unit upfront preview vs inert ticking with explicit `'Review selected'` subset evaluation** (`P19-CARBON-0044`, `P19-FABLE-0109` vs `P19-GPT-0025`, `P19-GPT-0274`, `P21-0110`, `P21-0111`) → Resolved by **[D-31](decisions.md#d-31)**

### CL-02-16: Agent Safety Checks, Structural Coupling, Stop Semantics & Stale-Base Conflicts

- **Summary:** Every agent proposal enforces hard automated safety invariants: computed (never asserted) value-change counts and three automated checks, never pre-ticking value-changing parts, auto-quarantining hunks that introduce errors or touch protected cells, coupling structurally dependent edits so half-units cannot be accepted, preserving incomplete proposals cleanly on Stop, and atomically blocking stale-base overwrites with a three-value conflict display and Recompute path.
- **Cards (9):** `P19-FABLE-0140`, `P19-FABLE-0227`, `P19-FABLE-0494`, `P19-GPT-0275`, `P19-GPT-0277`, `P19-GPT-0279`, `P19-NEON-0047`, `P19-NEON-0233`, `P21-0114`
- **Sources:** P19-FABLE (3), P19-GPT (3), P19-NEON (2), P21 (1)
- **Disposition:** `core` — Make these six automated safety invariants normative acceptance criteria in Chapter 2's Agent Safety section.
- **`same-as` groups (3):** `P19-FABLE-0140` = `P19-FABLE-0494`; `P19-GPT-0277` = `P21-0114`; `P19-NEON-0047` = `P19-NEON-0233`
- **Agreements:**
  - Every proposal part displays computed result-change counts with examples ('No value changes' is always computed, never asserted) plus three automated checks: opens in Excel, adds no new errors, breaks no formula run (R-82).
  - To prevent proposal fatigue, computed value effects lead the UI and no part that changes calculated values is ever pre-ticked (P19-FABLE-0227).
  - An automated Safety Check blocks blind batch-accept whenever a hunk introduces evaluation errors, touches protected cells, or toggles iteration/calculation settings (R-43).
  - Structurally dependent edits (e.g., creating a defined name and referencing it) are coupled into indivisible review/undo units so ordinary review can never leave a dangling half-unit (R-45).
  - Clicking `Stop` leaves a visibly incomplete, unapplied proposal; continuing or reopening revalidates its base revision and a recovered accepted proposal can never execute twice (R-49).
  - If any target cell changed after the proposal was drafted (e.g., expected 120, current 123, proposed 125), applying any selection containing the stale unit is blocked atomically with all three values displayed, offering `Recompute` or deselecting the stale unit to apply independent units cleanly (R-47, S16).
- **Conflicts:** None

### CL-02-17: Granular Agent Undo, Out-of-Order Reversal & Persistent Change Trail

- **Summary:** Applying selected agent units validates all of them atomically against the base revision yet records separate ordered per-unit Undo steps (`Ctrl+Z` undoes one unit at a time and returns it to selectable state in Changes while preserving immutable attribution in History); out-of-order rollback creates a conflict-checked `'Create reversal'` forward step, and per DECISIONS_20261126 #14 the agent change trail is persisted in the workbook's CustomXML part (with an opt-out toggle).
- **Cards (11):** `P19-CARBON-0045`, `P19-FABLE-0111`, `P19-FABLE-0113`, `P19-FABLE-0495`, `P19-FABLE-0496`, `P19-FABLE-0497`, `P19-GPT-0026`, `P19-GPT-0276`, `P19-GPT-0278`, `P21-0112`, `P21-0113`
- **Sources:** P19-CARBON (1), P19-FABLE (5), P19-GPT (3), P21 (2)
- **Disposition:** `core-with-decision` — Specify granular per-unit Undo, `'Create reversal'`, and CustomXML change-trail persistence (DECISIONS_20261126 #14) in Chapter 2.
- **Decisions:** [D-14](decisions.md#d-14), [D-31](decisions.md#d-31)
- **`same-as` groups (3):** `P19-GPT-0276` = `P21-0112`; `P19-FABLE-0113` = `P19-FABLE-0496`; `P19-FABLE-0497` = `P21-0113`
- **Agreements:**
  - Accepting multiple selected agent units validates the whole selection atomically before writing, yet pushes separate named per-unit Undo steps in order (e.g., A then B) grouped under the proposal so one `Ctrl+Z` reverts B while keeping A (R-46, R-83, S15).
  - Cell and workbook History retain full immutable attribution—actor, timestamp, proposal/part ID, before/after content, and accepted/rejected/undone/reverted status—never erasing audit history on `Ctrl+Z` or Reject, with a copyable change record (R-48, R-85).
  - Rejecting an unsupported suggestion updates only the proposal record and leaves the workbook untouched.
- **Conflicts:**
  - **Persistence of the agent's change trail (optional/session-only export vs persisted in file's customXml part)** (`P19-CARBON-0045` vs `P19-FABLE-0497`, `P21-0113`) → Resolved by **[D-14](decisions.md#d-14)**
  - **Post-undo state of an accepted agent part (`Returned to pending in Changes` vs `Undone in History`)** (`P19-FABLE-0111`, `P19-FABLE-0495` vs `P19-GPT-0026`, `P21-0113`) → Resolved by **[D-31](decisions.md#d-31)**
  - **Out-of-order reversal UX (`Revert` with `'Revert anyway'` vs `'Create reversal'` checked proposal)** (`P19-FABLE-0113`, `P19-FABLE-0496` vs `P19-GPT-0026`, `P19-GPT-0278`) → Resolved by **[D-31](decisions.md#d-31)**
  - *Cross-cluster `conflicts-with` links (resolved by D-14, D-31):* `P19-CARBON-0045` vs `FRAMING-0179` (CL-13-03), `P19-GPT-0098` (CL-13-03); `P19-FABLE-0497` vs `P19-FABLE-0141` (CL-13-03), `P19-FABLE-0233` (CL-13-03), `P19-NEON-0100` (CL-15-05), `P19-NEON-0123` (CL-16-04); `P21-0113` vs `P19-FABLE-0141` (CL-13-03), `P19-FABLE-0233` (CL-13-03), `P19-NEON-0100` (CL-15-05), `P19-NEON-0123` (CL-16-04)

### CL-02-18: System Needs: Model Observation, Delta Stream & Editability Taxonomy

- **Summary:** To power all five views cleanly over one Skin IR, the engine/host must expose exact reads of formulas/values/formats/errors/settings (C-2), per-cell freshness and completed document revision state including dirty/stale tracking under Manual calc (C-3, C-11), incremental delta notifications covering values/formatting/spill shapes (C-4), and an explicit editability taxonomy across cells, spills, legacy arrays, tables, pivots, protected ranges, and objects (C-10).
- **Cards (5):** `P19-CARBON-0217`, `P19-GPT-0301`, `P19-GPT-0302`, `P19-GPT-0303`, `P19-GPT-0309`
- **Sources:** P19-CARBON (1), P19-GPT (4)
- **Disposition:** `core` — Feed these five capabilities directly into the Skin Architecture / Appendix C Engine-Needs contract (per DECISIONS_20261126 #9).
- **`same-as` groups (1):** `P19-CARBON-0217` = `P19-GPT-0302`
- **Agreements:**
  - All views read exact workbook truth (formulas, stored values, formatted text, number formats, styles, errors, annotations, settings) through one observation boundary without re-parsing OOXML.
  - The host exposes exact per-cell state (`saved`, `current`, `dirty/stale`, `calculating`, `cancelled`, `unavailable`) tied to a completed document revision so no view ever displays mixed-revision numbers as current.
  - Incremental delta notifications report changed cells, objects, settings, formatting, and changed spill shapes so all views stay live without full-workbook re-reads.
  - One shared editability query distinguishes ordinary cells, spill anchors vs spill children, legacy CSE array regions, tables, pivot outputs, protected cells, and workbook objects so every view obeys identical Excel edit rules.
- **Conflicts:** None

### CL-02-19: System Needs: Speculative Preview Calculation, Structural Edit Primitives & Agent Staging API

- **Summary:** The Preview Seam, non-grid refactors, and AI agent review all rest on one shared transactional engine capability: isolated copy-on-write speculative recalculation returning downstream diffs and error checks (C-6, C-16, C-30), Excel-exact structural edit primitives with reference adjustment (C-11, C-20), atomic stale-base-checked commits (C-12), and non-blocking read-only snapshot access for agents (C-29).
- **Cards (7):** `P19-CARBON-0222`, `P19-GPT-0310`, `P19-GPT-0311`, `P19-NEON-0244`, `P19-NEON-0258`, `P19-NEON-0267`, `P19-NEON-0268`
- **Sources:** P19-CARBON (1), P19-GPT (2), P19-NEON (4)
- **Disposition:** `core` — Consolidate these seven cards into the Transactional Preview & Structural Mutation section of Appendix C / Skin Architecture.
- **`same-as` groups (2):** `P19-CARBON-0222` = `P19-NEON-0244`; `P19-GPT-0310` = `P19-NEON-0258`
- **Agreements:**
  - One copy-on-write speculative evaluation overlay powers every preview in the product (Formula Explorer apply, Inspector fix, Map rewire, bulk run edit, and AI Agent Proposal Batches), returning exact downstream deltas, KPI impacts, and new-error checks with zero residual state on discard.
  - All views invoke the same Excel-exact structural edit primitives (fill series/extent, cut/move reference relocation, row/column insert/delete shift, relative copy, batch commit) so non-grid edits obey identical reference-adjustment and validation rules.
  - Atomic multi-edit commits reject stale base expectations and report conflicts at their exact source cell.
  - Agents read workbook structure, inferred patterns, formulas, values, and diagnostics via non-blocking read-only snapshots over the Unified Address Grammar (C-29).
- **Conflicts:** None

### CL-02-20: System Needs: Transactional Checkpoints, Exact Hashing, Reversal & Crash Recovery

- **Summary:** To back cross-view Undo, `'unchanged'` detection, attributed history, and safe restarts, the platform needs O(changed-cells) named compound checkpoints tagging reversible vs non-reversible effects (C-13, C-17, C-40), exact model-content hashing to prove `'unchanged'` and `'presentation only'` (C-42), conflict-checked reversal computation (C-33, C-41), and idempotent recovery separating committed work from unapplied drafts (C-14).
- **Cards (7):** `P19-CARBON-0223`, `P19-FABLE-0564`, `P19-FABLE-0565`, `P19-FABLE-0566`, `P19-GPT-0312`, `P19-GPT-0313`, `P19-GPT-0332`
- **Sources:** P19-CARBON (1), P19-FABLE (3), P19-GPT (3)
- **Disposition:** `core-with-decision` — Include in Appendix C (History, Checkpoints & Recovery capabilities), noting Owner Ruling #4 on macro undo.
- **Decisions:** [D-04](decisions.md#d-04)
- **`same-as` groups (2):** `P19-CARBON-0223` = `P19-FABLE-0564`; `P19-FABLE-0565` = `P19-GPT-0332`
- **Agreements:**
  - Named compound checkpoints restore exact values, formulas, formatting, and calculation state in `O(changed cells)` time, record actor attribution (user, macro, agent), and explicitly tag non-reversible external effects.
  - Exact model-content comparison/hashing mechanically proves `'unchanged'` (undo back to open) and `'presentation only'` (zero model mutation) rather than relying on fragile dirty booleans (C-42).
  - Revision comparison and conflict-checked reversal computation identify overlapping later edits at exact target cells before any out-of-order reversal commits (C-33, C-41).
  - Crash/restart recovery separates committed work, unapplied drafts, and unaccepted proposals with stable identities so accepted edits can never replay twice (C-14).
- **Conflicts:**
  - **Macro-run checkpoint requirement timing (v1 mandatory in C-17 vs design-for-later per DECISIONS_20261126 #4)** (`P19-CARBON-0223` vs `P19-GPT-0312`) → Resolved by **[D-04](decisions.md#d-04)**
  - *Cross-cluster `conflicts-with` links (resolved by D-04):* `P19-CARBON-0223` vs `FRAMING-0169` (CL-14-04), `P19-GPT-0038` (CL-02-10), `P19-GPT-0079` (CL-14-04), `P19-GPT-0205` (CL-04-30), `P19-GPT-0207` (CL-04-30), `P19-GPT-0296` (CL-14-04); `P19-GPT-0312` vs `P19-CARBON-0120` (CL-14-04), `P19-CARBON-0128` (CL-16-04), `P19-CARBON-0206` (CL-14-04), `P19-CARBON-0241` (CL-10-16), `P19-FABLE-0132` (CL-02-10), `P19-FABLE-0207` (CL-14-04) (+3 more)

### CL-02-21: System Needs: CustomXML View Anchoring & Pre-Save Excel Compatibility

- **Summary:** View metadata lives strictly in an Excel-preserved CustomXML part anchored to cells and the user's own defined names—never injecting hidden Excel names (per DECISIONS_20261126 #13)—stays synchronized across local structural edits with label/shape/size fingerprints for re-matching after external Excel edits (C-43, C-44), and runs a pre-save compatibility audit warning of anything Excel would refuse, repair, or render differently (C-45).
- **Cards (3):** `P19-FABLE-0567`, `P19-FABLE-0568`, `P19-FABLE-0569`
- **Sources:** P19-FABLE (3)
- **Disposition:** `core` — Place C-43/C-44/C-45 in Appendix C and cross-link tightly to Persistence (Topic XI) and Decider Ruling #13 (no hidden names as anchors).
- **`same-as` groups:** None within cluster
- **Cross-cluster `same-as`:** `P19-FABLE-0567` = `FRAMING-0178` (CL-13-02)
- **Agreements:**
  - View-setting references automatically adjust in step with inserts, deletes, moves, renames, and sorts performed inside DNA Calc, and carry structural fingerprints (labels, formula shape, size) to relocate anchors cleanly after edits made externally in Excel (C-43).
  - Anchors live strictly in our CustomXML part keyed to cells and the user's own defined names—DNA Calc never pollutes a workbook with hidden defined names (DECISIONS_20261126 #13).
  - The host reads and writes an Excel-preserved CustomXML part and explicitly detects when it is missing or older than the workbook model (C-44).
  - Before saving, the system reports anything in the workbook that Excel would refuse, repair, or display differently (C-45).
- **Conflicts:** None

## III. Shell

### CL-03-01: Unified Shell Architecture, Invariance & Never-Trapped Guarantee

- **Summary:** All five workbook views mount inside one shared Strand shell across desktop and browser (retiring OneCalc's separate shell), communicating strictly through commands rather than protocol internals. Chrome regions stay at fixed coordinates on view switch with zero layout jump, at most two panes are ever shown, and no view—including App—may trap the user away from Classic, Inspector, or required status.
- **Cards (13):** `FRAMING-0005`, `FRAMING-0035`, `P19-CARBON-0011`, `P19-CARBON-0172`, `P19-FABLE-0126`, `P19-FABLE-0230`, `P19-FABLE-0413`, `P19-GPT-0009`, `P19-GPT-0243`, `P19-NEON-0004`, `P19-NEON-0024`, `P19-NEON-0191`, `P19-NEON-0200`
- **Sources:** FRAMING (2), P19-CARBON (2), P19-FABLE (3), P19-GPT (2), P19-NEON (4)
- **Disposition:** `core` — Foundation of Chapter III (Shared Shell). Note that P19-CARBON-0172 incidentally cites Ctrl+1..Ctrl+5; the Book uses the resolved view-switching chords from CL-03-04 (Ctrl+Alt+1..5 + Alt+Shift+1..5 alias).
- **`same-as` groups (4):** `FRAMING-0005` = `P19-GPT-0009`; `P19-CARBON-0011` = `P19-NEON-0004`; `P19-CARBON-0172` = `P19-FABLE-0413` = `P19-NEON-0191`; `P19-GPT-0243` = `P19-NEON-0024` = `P19-NEON-0200`
- **Agreements:**
  - One shared shell wraps every view identically in desktop and browser builds, retiring OneCalc's separate shell and enforcing a command-only boundary (FRAMING-0005, FRAMING-0035).
  - Calm petrol-800 (#193F49) flight-deck chrome frames a daylight paper stage (#FFFFFF / #EDF1F2 in light mode), eliminating modal clutter while keeping the model unobstructed (P19-CARBON-0011, P19-NEON-0004).
  - Switching between the five views keeps shell chrome at fixed screen coordinates with zero layout jump and preserves window/navigation state (P19-CARBON-0172, P19-FABLE-0413, P19-NEON-0191).
  - No view—including App—may remove Classic, suppress required workbook status, or trap the user without an explicit keyboard-reachable path back to Classic or Inspector (subject only to standard Excel sheet protection) (P19-GPT-0243, P19-NEON-0024, P19-NEON-0200).
  - At most two panes may be shown at once (active view plus one companion such as Classic grid sharing selection); free multi-pane tiling and separate tabs per view are rejected so the five views always feel like one workbook (P19-FABLE-0126, P19-FABLE-0230).
  - A once-dismissible, Help-restorable first-use hint clarifies the shared-workbook mental model ('Same workbook. Choose a view for the task. Alt+Q finds any command.') (P19-GPT-0009).
- **Conflicts:** None

### CL-03-02: Chrome Regions, Command Deck & Instrument Strip

- **Summary:** Defines the concrete spatial anatomy of the shared shell—Mast, compact single-row Command Deck, Bridge, paper Stage, docked Panels (with Strand's 'Inspector' panel renamed to 'Details'), bottom-of-Stage Sheet Tabs, and permanently visible Instrument Strip—while resolving the Ribbon/Deck, Sheet-Tab placement, and Mast shortcut disagreements.
- **Cards (12):** `P19-CARBON-0005`, `P19-CARBON-0012`, `P19-CARBON-0014`, `P19-CARBON-0015`, `P19-FABLE-0021`, `P19-FABLE-0027`, `P19-FABLE-0033`, `P19-FABLE-0414`, `P19-FABLE-0422`, `P19-NEON-0012`, `P19-NEON-0014`, `P21-0009`
- **Sources:** P19-CARBON (4), P19-FABLE (5), P19-NEON (2), P21 (1)
- **Disposition:** `core-with-decision` — Use the unified Mast (36 px) + single-row Command Deck (collapsible outside Classic) + Bridge (32 px) + Stage + bottom Sheet-Tab bar + Instrument Strip (26 px) layout, and use 'Details' (never 'Inspector') for the pinned peek panel. Note Neon's Alt+Hover peek in P19-NEON-0014 pairs with Fable's 500 ms rest hover in CL-03-03 as an optional instant-peek accelerator.
- **Decisions:** [D-32](decisions.md#d-32), [D-33](decisions.md#d-33), [D-34](decisions.md#d-34)
- **`same-as` groups (3):** `P19-CARBON-0005` = `P19-NEON-0012`; `P19-FABLE-0021` = `P19-FABLE-0027` = `P21-0009`; `P19-CARBON-0015` = `P19-FABLE-0414`
- **Agreements:**
  - All sources reject cloning Microsoft Excel's tall multi-row light-grey Ribbon (280 px icon warehouse) and prohibit Microsoft artwork (P19-CARBON-0005, P19-NEON-0012).
  - The bottom Instrument Strip (26 px) is permanently visible in every view and skin as the sole home for persistent telemetry: mode word, calculation state/progress, error count (with root count), outside-data/macro/agent status, selection aggregates, and zoom (P19-CARBON-0015, P19-FABLE-0414).
  - Strand v0.1's docked 'Inspector' panel is renamed 'Details' to avoid colliding with the top-level Inspector view (P19-FABLE-0033).
  - The Mast (~34–36 px petrol-800) holds the Strand profile badge, document title, view switcher, non-modal trust/quarantine pills, and command search trigger (P19-CARBON-0012, P19-FABLE-0027, P19-NEON-0014).
- **Conflicts:**
  - **Compact single-row Command Deck with Excel tabs vs No-Ribbon Bridge-only chrome** (`P19-FABLE-0021`, `P19-FABLE-0027`, `P19-FABLE-0422`, `P21-0009` vs `P19-CARBON-0005`, `P19-CARBON-0012`, `P19-NEON-0012`, `P19-NEON-0014`) → Resolved by **[D-32](decisions.md#d-32)**
  - **Sheet-tab bar placement: dedicated Stage-bottom bar vs embedded inside the 26 px Instrument Strip** (`P19-CARBON-0014`, `P19-FABLE-0027`, `P21-0009` vs `P19-NEON-0014`) → Resolved by **[D-33](decisions.md#d-33)**
  - **Sheet-tab visibility in non-Classic views** (`P19-CARBON-0014`, `P19-NEON-0014` vs `P19-FABLE-0027`) → Resolved by **[D-33](decisions.md#d-33)**
  - **Mast quick-action shortcut bindings (Excel-safe Alt+Q/Alt+Shift vs colliding Ctrl keys)** (`P19-FABLE-0027`, `P21-0009` vs `P19-CARBON-0012`, `P19-NEON-0014`) → Resolved by **[D-34](decisions.md#d-34)**

### CL-03-03: Bridge, Formula Bar, Selection State, Peek Card & Esc Ladder

- **Summary:** Every view shares one Bridge (Unified Address & Target Box plus expandable multi-line Formula Bar, folded by default only in App), one SELECTED-vs-EDITING visual contract, one five-rung Esc ladder that always restores the pre-edit model hash, one set of docked panels, and one consistent Peek Card.
- **Cards (8):** `P19-CARBON-0013`, `P19-FABLE-0025`, `P19-FABLE-0415`, `P19-FABLE-0416`, `P19-FABLE-0417`, `P19-FABLE-0418`, `P19-FABLE-0419`, `P19-FABLE-0423`
- **Sources:** P19-CARBON (1), P19-FABLE (7)
- **Disposition:** `core` — Adopt all eight contracts verbatim as core Shell interaction invariants. Pair Fable's 500 ms rest hover (P19-FABLE-0423) with Neon's Alt+Hover instant-peek accelerator (from P19-NEON-0014 in CL-03-02) so keyboard-assisted mouse users can summon the Peek Card immediately without delay.
- **`same-as` groups:** None within cluster
- **Agreements:**
  - The Bridge provides the Unified Address & Target Box and an expandable multi-line Formula Bar (Recursive Mono, multi-line LET) across views, folding by default only in App (P19-CARBON-0013, P19-FABLE-0415, P19-FABLE-0416).
  - Typing an address in the Bridge address box, Go To, or Command Palette selects that target in the current view (P19-FABLE-0415).
  - SELECTED and EDITING always differ simultaneously in three places—the selected item, the formula bar, and the Strip mode word—and Esc restores the exact pre-edit model hash (P19-FABLE-0417, P19-FABLE-0418).
  - One shared Esc ladder steps back exactly one level in strict order: edit -> preview -> copy outline -> popover -> look-inside (P19-FABLE-0025, P19-FABLE-0419).
  - One Peek Card shows identical content for a given object in every view after 500 ms hover rest without covering its target, with identical content pinned in the Details panel (Alt+Shift+D) (P19-FABLE-0025, P19-FABLE-0423).
- **Conflicts:** None

### CL-03-04: Keyboard Architecture, View-Switching Keys & Command Palette

- **Summary:** Establishes the two-layer keyboard contract that preserves 100% of Excel's shortcuts while giving DNA Calc fast, collision-free keys: Ctrl+Alt+1..5 for views (with Alt+Shift+1..5 AltGr-safe alias), Alt+Q for the sub-50 ms Command Palette, and Alt+Shift+<key> for DNA Calc tools.
- **Cards (7):** `FRAMING-0071`, `FRAMING-0177`, `P19-CARBON-0054`, `P19-FABLE-0026`, `P19-FABLE-0420`, `P19-FABLE-0421`, `P19-NEON-0113`
- **Sources:** FRAMING (2), P19-CARBON (1), P19-FABLE (3), P19-NEON (1)
- **Disposition:** `core-with-decision` — Publish one authoritative keyboard table in Chapter III / Appendix A: Excel keys untouched; Ctrl+Alt+1..5 (+ Alt+Shift+1..5 alias) for views; Alt+Q for Command Palette; Alt+Shift+{0,L,E,T,D} for DNA Calc shell tools.
- **Decisions:** [D-12](decisions.md#d-12), [D-34](decisions.md#d-34)
- **`same-as` groups (2):** `FRAMING-0177` = `P19-NEON-0113`; `P19-FABLE-0026` = `P19-FABLE-0420`
- **Cross-cluster `same-as`:** `P19-FABLE-0420` = `P19-GPT-0227` (CL-04-05)
- **Agreements:**
  - The shell follows a unified keyboard atlas governing chrome, command surfaces, and focus movement (FRAMING-0071).
  - Switching views must complete in under 100 ms; touch and pen remain deferred for v1 with explicit hook points noted (P19-CARBON-0054).
  - Command Palette opens within 50 ms, searches commands, addresses, names, sheets, blocks, and views, displays each command's shortcut and Excel key-tip path, and offers 'Ask the agent' as its last row (P19-FABLE-0421).
  - Opportunistic Keyboard Lock in installed PWA mode with dynamic shortcut tooltips helps browser builds retain parity with desktop (P19-NEON-0113).
- **Conflicts:**
  - **View-switching keyboard shortcuts (Ctrl+Alt+1..5 vs Alt+Shift+1..5 vs Ctrl+1..5)** (`FRAMING-0177`, `P19-NEON-0113` vs `P19-CARBON-0054`, `P19-FABLE-0026`, `P19-FABLE-0420`) → Resolved by **[D-12](decisions.md#d-12)**
  - **Command Palette and Formula Lens shortcuts (Alt+Q & Alt+Shift+L vs Ctrl+K & Alt+L)** (`P19-FABLE-0026`, `P19-FABLE-0420`, `P19-FABLE-0421` vs `P19-CARBON-0054`) → Resolved by **[D-34](decisions.md#d-34)**
  - *Cross-cluster `conflicts-with` links (resolved by D-12, D-34):* `FRAMING-0177` vs `P19-CARBON-0049` (CL-02-06), `P19-CARBON-0055` (CL-04-01), `P19-CARBON-0057` (CL-05-01), `P19-CARBON-0059` (CL-06-02), `P19-CARBON-0061` (CL-07-09), `P19-CARBON-0063` (CL-08-01) (+11 more); `P19-CARBON-0054` vs `P19-GPT-0218` (CL-04-28); `P19-FABLE-0026` vs `P19-GPT-0218` (CL-04-28); `P19-FABLE-0420` vs `P19-GPT-0218` (CL-04-28); `P19-NEON-0113` vs `P19-GPT-0218` (CL-04-28)

### CL-03-05: Strand Identity, Semantic Tokens, Focus Ring & Motion Restraint

- **Summary:** Enforces Strand's visual discipline across the shell: petrol-biased neutrals, two-block base-pair profile badges only in chrome, high-visibility amber focus rings, strict one-meaning semantic colors paired with shapes, consistent solid/dashed/dotted line semantics, restrained reduced-motion-safe animation, and 100% self-contained original SVG assets.
- **Cards (6):** `FRAMING-0095`, `P19-FABLE-0424`, `P19-FABLE-0425`, `P19-FABLE-0426`, `P19-FABLE-0427`, `P21-0011`
- **Sources:** FRAMING (1), P19-FABLE (4), P21 (1)
- **Disposition:** `core` — Cross-reference directly with Chapter II (Strand Design System) so Shell chrome and every view obey the exact same token, focus, line-style, and asset rules.
- **`same-as` groups:** None within cluster
- **Agreements:**
  - Profile identity uses the two-block base-pair badge (OneCalc dot-long, Excel-strict twin, legacy Rich Tree long-dot) strictly in chrome identity spots and never on data surfaces; neutrals are petrol-biased greys, never pure grey (FRAMING-0095).
  - Keyboard focus is always visible as an amber ring (>=2 px wide, 1 px offset), and stage selection turns hollow when focus moves to chrome or panels (P19-FABLE-0424).
  - Semantic color tokens carry single meanings paired with shapes: amber = selection/focus/'now', red = errors, signal orange = outside/volatile data, green = fresh values (P19-FABLE-0425).
  - Line styles are invariant across every view: solid = stated in file, dashed = inferred, dotted = ghost/preview (P19-FABLE-0426).
  - Motion is strictly limited to selection/focus settle, <=400 ms recalc ripple, and view-switch halo, all disabled under prefers-reduced-motion (P19-FABLE-0427).
  - All iconography and UI assets are self-contained original SVGs with zero CDN calls, external web-font downloads, or Microsoft artwork (P21-0011).
- **Conflicts:** None

### CL-03-06: Product Baseline, Twelve UX Moments & Website-App Synergy

- **Summary:** Frames the overarching product scope the shell serves: exact Excel calculation and round-trip fidelity on desktop and browser (keyboard and mouse first), twelve mandatory make-or-break UX moments every workflow must specify, and two-way narrative and link integration between the DNA Calc app and website.
- **Cards (3):** `FRAMING-0113`, `FRAMING-0122`, `FRAMING-0165`
- **Sources:** FRAMING (3)
- **Disposition:** `core-with-decision` — Use FRAMING-0122 as a checklist across every Design Book chapter, and include the website-app two-way links story in the Shell's Help / Start Stage / About surfaces.
- **Decisions:** [D-08](decisions.md#d-08)
- **`same-as` groups:** None within cluster
- **Cross-cluster `same-as`:** `FRAMING-0165` = `FRAMING-0173` (CL-01-13)
- **Agreements:**
  - DNA Calc targets exact Excel calculation and file round-trip across desktop and browser, designing keyboard and mouse first while deferring touch/pen (FRAMING-0113).
  - Every workflow in the Design Book must explicitly specify the twelve make-or-break UX moments from first-run/empty states through accessibility and Excel-user wins (FRAMING-0122).
  - The website and app must link both ways and tell the same stories, foregrounding automation, engine use, cross-platform support, and agentic tweaking alongside the UI (FRAMING-0165).
- **Conflicts:** None

### CL-03-07: Start Stage, File Open, Model Patterns & Loss-Safe Save

- **Summary:** Cold start opens a fast, calm Start Stage with Open, New, Formula Explorer, Recent workbooks (showing preferred view and Classic fallback), and 'New from a pattern' built from transparent ordinary Excel sheets. File Save discloses that view settings travel in the workbook, offers separate view-settings JSON export, preserves the last good file on error, and prevents silent macro or metadata loss.
- **Cards (6):** `FRAMING-0157`, `P19-CARBON-0026`, `P19-FABLE-0041`, `P19-GPT-0297`, `P21-0095`, `P21-0096`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-FABLE (1), P19-GPT (1), P21 (2)
- **Disposition:** `core-with-decision` — Harmonize the Start Stage Formula Explorer shortcut hint to Alt+Shift+E (P19-FABLE-0041) rather than Ctrl+Shift+E (P19-CARBON-0026), consistent with CL-03-04.
- **Decisions:** [D-35](decisions.md#d-35)
- **`same-as` groups (2):** `P19-CARBON-0026` = `P19-FABLE-0041`; `FRAMING-0157` = `P21-0096`
- **Agreements:**
  - Cold start (<65 ms) lands on a calm Start Stage with zero tour popups or account walls, presenting Open Workbook (Ctrl+O / drag-and-drop), New Workbook (Ctrl+N), Formula Explorer (Alt+Shift+E), Recent list, and a concise Excel-compatibility promise (P19-CARBON-0026, P19-FABLE-0041).
  - File Open lists recent workbooks with preferred view and Classic fallback, offers 'New from a pattern' disclosing ordinary sheets/cells/linked formulas with scoped Undo, and handles unavailable file paths without silently substituting an empty workbook (FRAMING-0157, P21-0096).
  - File Save displays a quiet notice that view settings travel in the workbook alongside a three-part breakdown (Excel model, DNA Calc views, Ordinary-reader fallback) and offers separate JSON export of view settings (FRAMING-0157, P21-0095).
  - Save failures always preserve the last good workbook on disk and never discard stored macro code or unknown supported OOXML parts (P19-GPT-0297).
- **Conflicts:**
  - **Handling format conversions that drop macros or custom-XML views (hard refusal vs explicit confirmation)** (`P21-0095` vs `P19-GPT-0297`) → Resolved by **[D-35](decisions.md#d-35)**

### CL-03-08: Staged Non-Blocking Open & Startup Quarantine

- **Summary:** Large or untrusted workbooks open in stages—snapping shell chrome immediately, painting cached values of the active sheet without waiting for full indexing, streaming dependency graphs in the background, and replacing Excel's modal startup walls with a single non-blocking Trust & Notice bar plus hidden-sheet transparency.
- **Cards (8):** `P19-CARBON-0025`, `P19-CARBON-0027`, `P19-CARBON-0176`, `P19-CARBON-0207`, `P19-FABLE-0042`, `P19-FABLE-0525`, `P19-NEON-0025`, `P19-NEON-0239`
- **Sources:** P19-CARBON (4), P19-FABLE (2), P19-NEON (2)
- **Disposition:** `core-with-decision` — Set performance budgets clearly: shell frame <100 ms, typical active-sheet cached paint <250 ms (<2 s for 300k-formula Inherited-scale files), background indexing non-blocking with live Strip progress.
- **Decisions:** [D-36](decisions.md#d-36)
- **`same-as` groups (2):** `P19-CARBON-0025` = `P19-CARBON-0176`; `P19-CARBON-0207` = `P19-FABLE-0525` = `P19-NEON-0239`
- **Cross-cluster `same-as`:** `P19-NEON-0025` = `P19-FABLE-0555` (CL-13-09)
- **Agreements:**
  - Opening a workbook with untrusted macros, missing external links, or manual calculation never blocks behind modal startup dialogs; macros default to quarantined and missing links hold cached values behind clear non-blocking notices (P19-CARBON-0025, P19-CARBON-0176, P19-FABLE-0042, P19-NEON-0025).
  - File opening is staged: sheet metadata (including Visible, Hidden, and VeryHidden states), formatting, and last-saved cached values of the active viewport paint immediately while dependency indexing streams in the background with Strip progress (P19-CARBON-0207, P19-FABLE-0525, P19-NEON-0239).
  - Hidden and VeryHidden sheets are disclosed cleanly in the Sheet Drawer without altering their visibility flags, and a polite screen-reader announcement summarizes file state on open (P19-CARBON-0027, P19-NEON-0025).
- **Conflicts:**
  - **Startup triage surface & Quarantine Bar shortcut (Alt+Q Quarantine Bar vs single Trust & Triage Banner vs auto-opened Overview panel)** (`P19-CARBON-0027`, `P19-CARBON-0176` vs `P19-FABLE-0042`, `P19-NEON-0025`) → Resolved by **[D-36](decisions.md#d-36)**

### CL-03-09: Mixed Per-Sheet Views & File View-Settings UI

- **Summary:** One workbook may assign different preferred views to different sheets (e.g., Classic inputs, Model Map monthly logic, Notebook summary), surfaced via visible sheet-tab view badges and a File View-Settings UI that inspects CustomXML declarations and previews what an Excel-only reader sees.
- **Cards (3):** `FRAMING-0015`, `FRAMING-0064`, `FRAMING-0155`
- **Sources:** FRAMING (3)
- **Disposition:** `core-with-decision` — Shell exposes the File View-Settings inspector and sheet-tab view badges; persistence schema and tab-click pinning vs switching conflict resolution live in CL-13-01 and CL-13-04.
- **Decisions:** [D-86](decisions.md#d-86)
- **`same-as` groups:** None within cluster
- **Cross-cluster `same-as`:** `FRAMING-0064` = `P19-FABLE-0221` (CL-13-04), `P19-NEON-0103` (CL-13-04)
- **Agreements:**
  - Sheets inside a single workbook can carry distinct preferred views (FRAMING-0015).
  - Per-sheet view preferences are surfaced via visible tab badges and never trigger disorienting automatic view jumps during authoring (FRAMING-0064).
  - A dedicated File View-Settings UI lets authors configure preferred and supported views, inspect what is stored in CustomXML, and see what an ordinary Excel reader without DNA Calc sees (FRAMING-0155).
- **Conflicts:** None

### CL-03-10: Shared Print Engine, Frozen Snapshots & Classic Excel Print Parity

- **Summary:** Every view prints paginated vector output forced onto daylight paper via Ctrl+P using a consistent frozen calculation snapshot with explicit freshness/filter/error disclosure, while Classic matches Microsoft Excel PageSetup and pagination breaks exactly.
- **Cards (10):** `P19-CARBON-0093`, `P19-CARBON-0205`, `P19-CARBON-0227`, `P19-FABLE-0512`, `P19-GPT-0077`, `P19-GPT-0328`, `P19-NEON-0085`, `P19-NEON-0263`, `P21-0097`, `P21-0129`
- **Sources:** P19-CARBON (3), P19-FABLE (1), P19-GPT (2), P19-NEON (2), P21 (2)
- **Disposition:** `core-with-decision` — Note also that P19-GPT-0077's macro-free export confirmation aligns with CL-03-07's Save-a-Copy decision, and P21-0129 belongs in the verification plan for native desktop/browser print certification.
- **Decisions:** [D-37](decisions.md#d-37)
- **`same-as` groups (3):** `P19-CARBON-0093` = `P19-CARBON-0205`; `P19-CARBON-0227` = `P19-NEON-0263`; `P19-GPT-0328` = `P21-0097`
- **Cross-cluster `same-as`:** `P19-GPT-0328` = `P19-FABLE-0557` (CL-13-08)
- **Agreements:**
  - All views print crisp paginated vector output forced to cockpit-light daylight paper regardless of active UI dark theme, stripping dark chrome and clipped scrollbars (P19-CARBON-0093, P19-CARBON-0205).
  - Classic matches Excel PageSetup (PrintArea, PrintTitles, manual/auto page breaks, margins, orientation, scaling, headers/footers) and produces identical page counts and breaks (P19-CARBON-0205, P19-CARBON-0227, P19-FABLE-0512, P19-NEON-0263).
  - PDF output is a frozen report; native OS print dialogs, browser pagination, physical/PDF output, clipboard permissions, and download prompts require dedicated native/browser verification (P19-GPT-0077, P21-0129).
- **Conflicts:**
  - **Frozen Print Preview snapshot & blocked-calculation honesty vs live-updating preview** (`P19-GPT-0077`, `P19-GPT-0328`, `P21-0097` vs `P19-NEON-0085`) → Resolved by **[D-37](decisions.md#d-37)**

### CL-03-11: View-Specific Honest Print Layouts

- **Summary:** Specifies tailored, scope-disclosed print layouts for every non-Classic view—portrait Notebook write-ups with optional formula appendix, landscape Model Map selected regions refusing illegible shrink-to-fit, Inspector evidence with omitted-branch disclosures, App summary reports with opt-in schedules, and Formula Explorer evaluation reports—plus 'Print as Excel would' everywhere.
- **Cards (8):** `P19-FABLE-0203`, `P19-FABLE-0513`, `P19-GPT-0076`, `P19-GPT-0293`, `P19-GPT-0294`, `P19-NEON-0235`, `P21-0098`, `P21-0099`
- **Sources:** P19-FABLE (2), P19-GPT (3), P19-NEON (1), P21 (2)
- **Disposition:** `core` — Include a clean per-view Print Matrix table in Chapter III summarizing orientation, default scope, mandatory disclosures, and appendix options for all five views plus Formula Explorer.
- **`same-as` groups (2):** `P19-FABLE-0203` = `P19-FABLE-0513`; `P19-GPT-0076` = `P19-GPT-0294`
- **Agreements:**
  - Every non-Classic print carries a footer recording file name, active scenario, and value timestamp, and every view offers a direct 'Print as Excel would' option (P19-FABLE-0203, P19-FABLE-0513).
  - No printout may silently treat an excerpt, sampled table, or partial graph as complete; summarized rows, hidden branches, and boundary connectors are always disclosed (P19-GPT-0293, P19-GPT-0294, P19-NEON-0235).
  - Notebook prints A4 portrait in computation/reading order (explicitly marking live/RTD values as captured/non-updating) with an optional backing-formula appendix that visibly updates page count (P19-GPT-0076, P21-0098).
  - Model Map prints A4 landscape for a readable selected region with boundary connectors, period-delay vs iteration distinctions, and optional backing-formula appendix, refusing unreadable shrink-to-fit auto-expansion (P19-GPT-0076, P21-0099).
  - App prints its authored summary report by default (e.g. Loan prints one summary page) unless supporting schedule pages are explicitly selected, and Formula Explorer prints expression, inputs, caller, provider/version, tests, and formatting rules (P19-GPT-0076, P19-GPT-0293).
- **Conflicts:** None

### CL-03-12: Shell-to-Engine Calculation State, Clipboard & Host Object Capabilities

- **Summary:** Collects the underlying engine and platform-host contracts required by the shared shell: distinguishing saved vs recalculated values, cell staleness, recalc diffs and triggers, evaluation order, interruptible non-blocking calculation, workbook calc settings, rich clipboard transfer, vector chart/PivotTable bindings, and separating unsupported host capabilities from invalid workbook content.
- **Cards (10):** `P19-FABLE-0526`, `P19-FABLE-0527`, `P19-FABLE-0528`, `P19-FABLE-0529`, `P19-FABLE-0530`, `P19-FABLE-0531`, `P19-GPT-0323`, `P19-GPT-0327`, `P19-GPT-0333`, `P19-NEON-0264`
- **Sources:** P19-FABLE (6), P19-GPT (3), P19-NEON (1)
- **Disposition:** `core` — Feed directly into the Design Book's System-Needs / Skin-Host Contract appendix so calc-engine and calc-host expose every signal the Shell and views need.
- **`same-as` groups (1):** `P19-GPT-0323` = `P19-NEON-0264`
- **Cross-cluster `same-as`:** `P19-GPT-0327` = `P19-GPT-0169` (CL-04-20); `P19-GPT-0333` = `P19-FABLE-0235` (CL-14-07)
- **Agreements:**
  - Engine exposes saved-file values separately from recalculated values (with save date and saving program), per-cell staleness, post-recalc change diffs with trigger attribution (edit/F9/live/macro), and last evaluation order (P19-FABLE-0526, P19-FABLE-0527, P19-FABLE-0528, P19-FABLE-0529).
  - Long recalculations report progress, stay interruptible, and allow reading values and typing meanwhile; workbook calculation settings (mode, iteration, max iterations/change, precision as displayed, date system) are readable and writable (P19-FABLE-0530, P19-FABLE-0531).
  - Host exposes live native Charts, PivotTables, form controls, scenarios, validation rules, and print settings synchronized with recalculation (P19-GPT-0323, P19-NEON-0264).
  - Clipboard transfers preserve source-origin and formula/value/text/format distinctions while declaring loss at unsupported destinations, and hosts cleanly distinguish unsupported capability from invalid workbook content without discarding original content (P19-GPT-0327, P19-GPT-0333).
- **Conflicts:** None

## IV. Classic

### CL-04-01: Classic Role, Universal Default & Visual IP Boundaries

- **Summary:** Classic is the default landing view for plain or unannotated .xlsx workbooks and the universal precision grid that never declines or summarises any file. It preserves Excel's interaction structure and muscle memory under DNA Calc's own Strand visual language with zero Microsoft icons, fonts, or Office chrome.
- **Cards (11):** `FRAMING-0007`, `FRAMING-0049`, `FRAMING-0059`, `P19-CARBON-0008`, `P19-CARBON-0055`, `P19-FABLE-0032`, `P19-FABLE-0151`, `P19-GPT-0040`, `P19-NEON-0010`, `P19-NEON-0013`, `P19-NEON-0048`
- **Sources:** FRAMING (3), P19-CARBON (2), P19-FABLE (2), P19-GPT (1), P19-NEON (3)
- **Disposition:** `core-with-decision` — Open Chapter IV (Classic) with this mandate: universal non-declining grid default, visual IP independence under Strand, and the Ruling #12 view-switch chord table.
- **Decisions:** [D-12](decisions.md#d-12)
- **`same-as` groups (2):** `FRAMING-0007` = `FRAMING-0049` = `P19-CARBON-0008` = `P19-FABLE-0032` = `P19-GPT-0040` = `P19-NEON-0010`; `P19-FABLE-0151` = `P19-NEON-0013`
- **Agreements:**
  - Classic is the default landing view for plain/unannotated .xlsx files and never declines or summarises any workbook.
  - Familiarity lives in keyboard and mouse muscle memory, never in copied Microsoft icons, Ribbon artwork, or Office-blue styling.
  - Classic sits on the shared skin/command architecture (already cleaned up to 0 raw intents in the 2026-11-22 baseline).
- **Conflicts:**
  - **View-switching shortcut to enter Classic (Ctrl+1 vs Alt+Shift+1 vs Ctrl+Alt+1)** (`P19-CARBON-0055` vs `P19-FABLE-0151`, `P19-NEON-0010`, `P19-NEON-0048`) → Resolved by **[D-12](decisions.md#d-12)**
  - *Cross-cluster `conflicts-with` links (resolved by D-12):* `P19-CARBON-0055` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28); `P19-FABLE-0151` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28)

### CL-04-02: Conformance Contract, Oracle Profile, Input Routing & Deliberate Differences

- **Summary:** Classic parity is governed by an executable Appendix A conformance suite tested against a pinned Microsoft 365 Windows English/US desktop Excel oracle, tiered into T1 (v1 blocking check) and T2 (follow-on). Strict input-routing precedence (focused editor/menu/dialog -> focused view -> shell) prevents shortcut leaks, every deliberate divergence is enumerated transparently, and unimplemented Excel commands report honestly rather than acting as dead keys.
- **Cards (16):** `FRAMING-0129`, `FRAMING-0143`, `P19-CARBON-0129`, `P19-CARBON-0183`, `P19-FABLE-0224`, `P19-FABLE-0246`, `P19-FABLE-0405`, `P19-FABLE-0433`, `P19-GPT-0095`, `P19-GPT-0107`, `P19-GPT-0213`, `P19-GPT-0245`, `P19-GPT-0246`, `P19-NEON-0108`, `P19-NEON-0125`, `P19-NEON-0201`
- **Sources:** FRAMING (2), P19-CARBON (2), P19-FABLE (4), P19-GPT (5), P19-NEON (3)
- **Disposition:** `core-with-decision` — Include the oracle definition, T1/T2 check, input-routing precedence rule, and deliberate-differences policy as the normative preamble to Appendix A.
- **Decisions:** [D-38](decisions.md#d-38)
- **`same-as` groups (4):** `FRAMING-0129` = `P19-CARBON-0129` = `P19-FABLE-0224` = `P19-NEON-0108` = `P19-NEON-0125`; `P19-CARBON-0183` = `P19-NEON-0201`; `P19-FABLE-0246` = `P19-FABLE-0433`; `P19-GPT-0095` = `P19-GPT-0107` = `P19-GPT-0245`
- **Agreements:**
  - Appendix A is an executable, automated conformance contract — not informal guidance — where every unlisted divergence from desktop Excel is treated as a bug.
  - Input routing strictly follows focused editor/menu/dialog -> focused view -> shell so shell or agent panels can never hijack active grid shortcuts like Ctrl+Enter.
  - No mode may trap the user without a visible Strip mode indicator and an Esc exit; unsupported Excel features (Office Clipboard pane, Thesaurus, ink) announce 'Not in DNA Calc' rather than silently failing.
  - Shortcut customisation surfaces conflicts and baseline defaults, and imported skins cannot override core Escape/recovery keys or hide secret actions behind familiar shortcuts.
- **Conflicts:**
  - **Flat 100% Appendix A check vs. T1/T2 tiered release with pinned M365 Windows oracle** (`P19-CARBON-0183`, `P19-NEON-0201` vs `P19-FABLE-0246`, `P19-FABLE-0433`, `P19-GPT-0095`, `P19-GPT-0107`, `P19-GPT-0245`) → Resolved by **[D-38](decisions.md#d-38)**

### CL-04-03: Browser Host Shortcut Fallbacks & Platform Parity

- **Summary:** Native desktop and installed PWA windows capture 100% of Excel shortcuts, whereas browser-tab hosts cannot intercept OS/browser-reserved chords (Ctrl+N/T/W/Tab, F11, Alt, Ctrl+PgUp/PgDn). Browser mode publishes an explicit per-browser/OS shortcut differences sheet, supplies deterministic fallbacks, handles clipboard permissions transparently, and discloses desktop-only capabilities (native add-ins, local links, desktop macros).
- **Cards (9):** `P19-FABLE-0229`, `P19-FABLE-0406`, `P19-FABLE-0407`, `P19-FABLE-0408`, `P19-FABLE-0409`, `P19-FABLE-0410`, `P19-FABLE-0411`, `P19-GPT-0229`, `P19-NEON-0109`
- **Sources:** P19-FABLE (7), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core-with-decision` — Place this as the Browser Parity & Fallbacks subsection of Chapter IV / Appendix A Part 12.
- **Decisions:** [D-39](decisions.md#d-39)
- **`same-as` groups (1):** `P19-FABLE-0229` = `P19-GPT-0229`
- **Agreements:**
  - Never promise universal interception of browser-reserved shortcuts in an ordinary tab; full capture applies to native desktop and installed app/PWA windows (using Keyboard Lock API where supported).
  - No shortcut may differ silently: browser hosts surface a clear one-time Strip notice and a reachable 'Browser shortcut differences' reference sheet classifying contested keys as same, explicit alternate, browser-owned, or unavailable.
  - In browsers, clipboard permission is requested on first paste with explicit format-fallback reporting; right-click opens DNA Calc's menu while Shift+right-click exposes the browser menu; browser Back maps to place history (Alt+Left).
  - Unavailable native capabilities (XLL/COM add-ins, machine-touching macros, local file links) show saved values with the cached-value marker and an 'Open in desktop app' action.
- **Conflicts:**
  - **Browser-tab fallback bindings for Alt key tips, sheet switching, and command palette** (`P19-FABLE-0406`, `P19-FABLE-0408` vs `P19-NEON-0109`) → Resolved by **[D-39](decisions.md#d-39)**

### CL-04-04: Command Strip / Deck Layout, Key Tips & Legacy Menu Paths

- **Summary:** Classic replaces Excel's tall Ribbon with a compact, foldable Strand Command Strip / Deck (~32 px default, expandable to ~68 px, folded with Ctrl+F1) while preserving 100% of Excel's exact Alt / F10 key-tip letters and zero-delay type-ahead command paths, plus T2 legacy 2003 menu aliases (Alt, E, S, V) and Quick Access slots (Alt+1..9).
- **Cards (24):** `FRAMING-0176`, `P19-CARBON-0056`, `P19-CARBON-0130`, `P19-CARBON-0131`, `P19-FABLE-0044`, `P19-FABLE-0236`, `P19-FABLE-0383`, `P19-FABLE-0384`, `P19-FABLE-0385`, `P19-FABLE-0386`, `P19-FABLE-0387`, `P19-FABLE-0391`, `P19-FABLE-0392`, `P19-FABLE-0435`, `P19-GPT-0108`, `P19-GPT-0109`, `P19-GPT-0110`, `P19-GPT-0111`, `P19-GPT-0113`, `P19-GPT-0115`, `P19-NEON-0049`, `P19-NEON-0126`, `P19-NEON-0127`, `P21-0044`
- **Sources:** FRAMING (1), P19-CARBON (3), P19-FABLE (10), P19-GPT (6), P19-NEON (3), P21 (1)
- **Disposition:** `core-with-decision` — Specify the 32/68 px foldable Deck chrome and include Fable's complete A-138..A-141 key-tip path tables (T1 Ribbon paths + T2 legacy 2003 aliases) as normative tables.
- **Decisions:** [D-11](decisions.md#d-11), [D-32](decisions.md#d-32)
- **`same-as` groups (4):** `FRAMING-0176` = `P19-FABLE-0044` = `P19-FABLE-0236` = `P19-FABLE-0384` = `P19-GPT-0109` = `P19-NEON-0126`; `P19-FABLE-0383` = `P19-GPT-0108` = `P19-NEON-0127` = `P21-0044`; `P19-CARBON-0131` = `P19-FABLE-0385` = `P19-FABLE-0386` = `P19-GPT-0110`; `P19-FABLE-0387` = `P19-GPT-0111`
- **Agreements:**
  - Alt (tap and release) and F10 display high-contrast Strand key-tip badges without altering cell selection; Esc steps back one level and restores focus.
  - Sequential key-tip paths typed from memory (e.g. Alt, H, V, V; Alt, H, O, I; Alt, A, T; Alt, W, F, F) execute immediately at typing speed (0 ms animation wait) regardless of Deck fold state or skin layout.
  - Legacy Excel 2003 menu paths (Alt, E, S, V, Enter etc.) are supported as T2 aliases in the key-tip table; unknown paths never guess a command.
  - Tab/Shift+Tab traverse controls inside the Deck and dialogs in reading order without moving the active worksheet cell; dialogs honour Excel's Alt-underlined mnemonics, Enter default, and Esc cancel.
  - F1 opens contextual help, Ctrl+F1 folds/unfolds the Deck without losing an active edit, and Alt+1..9 provide nine user-configurable Quick Access slots (T2).
- **Conflicts:**
  - **Key-tip first-letter parity with Excel (Carbon's I/V/L remapping vs Excel's exact N/W/L/Q letters)** (`P19-CARBON-0130` vs `FRAMING-0176`, `P19-FABLE-0044`, `P19-FABLE-0236`, `P19-FABLE-0384`, `P19-GPT-0109`, `P19-NEON-0126`) → Resolved by **[D-11](decisions.md#d-11)**
  - **Command Strip / Deck height and tab visibility (30 px fixed vs 32/68 px expandable vs full tabbed Deck)** (`P19-CARBON-0056` vs `P19-NEON-0049`, `P19-FABLE-0383`, `P21-0044`) → Resolved by **[D-32](decisions.md#d-32)**

### CL-04-05: Command Search (Alt+Q), Global File/Window Shortcuts & Format-Cell Collision Guard

- **Summary:** Alt+Q opens Classic's keyboard-navigable Command Search / Palette (matching Excel's Tell Me box) without stealing Ctrl+K (which stays Insert/Edit Hyperlink) or Ctrl+1 (which stays Format Cells). Standard desktop Excel workbook/window/macro/spelling shortcuts (Ctrl+N/O/S/P/W, F12, Ctrl+F4, Alt+F4, Ctrl+Tab/Ctrl+F6, F7, Alt+F8/Alt+F11) work faithfully.
- **Cards (12):** `P19-CARBON-0132`, `P19-CARBON-0159`, `P19-FABLE-0317`, `P19-FABLE-0388`, `P19-GPT-0208`, `P19-GPT-0209`, `P19-GPT-0211`, `P19-GPT-0212`, `P19-GPT-0219`, `P19-GPT-0227`, `P19-NEON-0128`, `P21-0049`
- **Sources:** P19-CARBON (2), P19-FABLE (2), P19-GPT (6), P19-NEON (1), P21 (1)
- **Disposition:** `core-with-decision` — Standardise Alt+Q across the whole app, protect Ctrl+K (Hyperlink) and Ctrl+1 (Format Cells) in Classic, and document macro/file/window keys in Appendix A.
- **Decisions:** [D-12](decisions.md#d-12), [D-34](decisions.md#d-34)
- **`same-as` groups (3):** `P19-GPT-0227` = `P21-0049`; `P19-FABLE-0317` = `P19-GPT-0212`; `P19-FABLE-0388` = `P19-GPT-0209` = `P19-GPT-0211`
- **Cross-cluster `same-as`:** `P19-GPT-0227` = `P19-FABLE-0420` (CL-03-04)
- **Agreements:**
  - Alt+Q opens fuzzy Command Search (filter by typing, Up/Down choose, Enter invokes, Esc restores exact prior focus and cell selection), showing shortcuts alongside hits.
  - Standard workbook and window commands match Excel: Ctrl+N (New), Ctrl+O (Open), Ctrl+S (Save), F12 (Save As), Ctrl+P (Print), Ctrl+W / Ctrl+F4 (Close workbook), Alt+F4 (Quit), Ctrl+Tab / Ctrl+F6 (+Shift) switch desktop workbook windows.
  - F7 runs spelling check in the selection/sheet (undoable in workbook history), and Alt+F8 / Alt+F11 open macros and the macro editor (disclosing Guarded/Off security or unavailable browser host while preserving workbook modules).
- **Conflicts:**
  - **Command Palette shortcut: Alt+Q vs Ctrl+K / Ctrl+Shift+P collision with Excel Hyperlink & Font Size** (`P19-CARBON-0132`, `P19-NEON-0128` vs `P19-FABLE-0388`, `P19-GPT-0227`, `P21-0049`) → Resolved by **[D-34](decisions.md#d-34)**
  - **Ctrl+1 Format Cells protection vs Carbon's Ctrl+1..5 view switch + Ctrl+Shift+F workaround** (`P19-CARBON-0159` vs `P19-FABLE-0388`) → Resolved by **[D-12](decisions.md#d-12)**
  - *Cross-cluster `conflicts-with` links (resolved by D-12, D-34):* `P19-CARBON-0159` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28)

### CL-04-06: Visual Cell States, Author Formatting Preservation & Provenance Overlay

- **Summary:** The Classic sheet canvas belongs to the workbook author: all stored number formats (including red parenthesised negatives), fills, borders, fonts, and conditional formats render faithfully with zero diagnostic overlays enabled by default. DNA Calc distinguishes 14 crisp visual cell/control states, eliminates blocking modal popups on open, renders errors with a red left-edge tick and notes with a teal fold, and offers an optional toggleable Provenance Overlay.
- **Cards (12):** `P19-CARBON-0184`, `P19-FABLE-0034`, `P19-FABLE-0397`, `P19-FABLE-0398`, `P19-FABLE-0434`, `P19-GPT-0247`, `P19-NEON-0020`, `P19-NEON-0189`, `P19-NEON-0203`, `P19-NEON-0204`, `P21-0006`, `P21-0007`
- **Sources:** P19-CARBON (1), P19-FABLE (4), P19-GPT (1), P19-NEON (4), P21 (2)
- **Disposition:** `core-with-decision` — Include the 14-state cell visual matrix (P21-0006), the 'paper belongs to the author' rule, non-modal open triage, and the Provenance Overlay spec.
- **Decisions:** [D-17](decisions.md#d-17), [D-40](decisions.md#d-40)
- **`same-as` groups (3):** `P19-FABLE-0034` = `P19-FABLE-0434` = `P19-GPT-0247` = `P21-0007`; `P19-FABLE-0397` = `P19-NEON-0204`; `P19-CARBON-0184` = `P19-NEON-0020` = `P19-NEON-0189` = `P19-NEON-0203`
- **Agreements:**
  - Author cell formatting (including red negative numbers, borders, fills, conditional formatting, and original font metadata) renders faithfully on paper; open-font substitutions are disclosed and never saved back as silent format mutations.
  - Opening a workbook with errors, broken links, unsigned macros, hidden sheets, or loops never throws blocking modal popups — it lands immediately in Classic showing cached values plus one non-modal triage/security notice line.
  - Cells and controls visually distinguish Normal, Selected, Active, Editing, Point, Error, Spill anchor, Spill child, Stale, Pending, Negative, Conditional format, Hover, and Keyboard Focus without ambiguity.
  - An optional non-destructive Provenance Overlay distinguishes literal constants (including hardcoded overrides inside uniform formula runs), formulas, iterative cells, external/volatile/UDF cells, and inferred block frames.
- **Conflicts:**
  - **Provenance Overlay default state in Classic & toggle shortcut (Ctrl+Shift+P collision)** (`P19-CARBON-0184`, `P19-NEON-0020`, `P19-NEON-0189`, `P19-NEON-0203` vs `P19-FABLE-0034`, `P19-FABLE-0434`) → Resolved by **[D-40](decisions.md#d-40)**
  - **Cell error vs. note indicator geometry (left-edge red tick + teal note fold vs. corner error triangles)** (`P19-FABLE-0398` vs `P19-NEON-0020`, `P19-NEON-0203`) → Resolved by **[D-17](decisions.md#d-17)**

### CL-04-07: Cell Modes, Direct Entry, Editing & In-Cell Text Keys

- **Summary:** Classic implements Excel's exact four-state mode machine (Ready, Enter, Edit, Point) displayed in the Status Strip, with direct printable-key replacement, F2 Edit/Enter/Point cycling, double-click editing, full in-editor text navigation, Alt+Enter line breaks, Ctrl+Shift+U formula bar expansion, locale-exact entry parsing, text AutoComplete, and Esc universal cancel.
- **Cards (31):** `P19-CARBON-0144`, `P19-CARBON-0150`, `P19-CARBON-0153`, `P19-FABLE-0247`, `P19-FABLE-0248`, `P19-FABLE-0249`, `P19-FABLE-0250`, `P19-FABLE-0251`, `P19-FABLE-0252`, `P19-FABLE-0253`, `P19-FABLE-0254`, `P19-FABLE-0255`, `P19-FABLE-0256`, `P19-FABLE-0257`, `P19-FABLE-0258`, `P19-FABLE-0263`, `P19-FABLE-0266`, `P19-FABLE-0267`, `P19-FABLE-0268`, `P19-FABLE-0284`, `P19-GPT-0136`, `P19-GPT-0137`, `P19-GPT-0138`, `P19-GPT-0139`, `P19-GPT-0141`, `P19-GPT-0143`, `P19-GPT-0144`, `P19-GPT-0145`, `P19-NEON-0148`, `P19-NEON-0149`, `P19-NEON-0157`
- **Sources:** P19-CARBON (3), P19-FABLE (17), P19-GPT (8), P19-NEON (3)
- **Disposition:** `core-with-decision` — Forms Appendix A Part 1 (Modes & Entry) and Part 2 entry parsing rules.
- **Decisions:** [D-41](decisions.md#d-41)
- **`same-as` groups (6):** `P19-FABLE-0247` = `P19-GPT-0136`; `P19-CARBON-0144` = `P19-FABLE-0248` = `P19-GPT-0138` = `P19-NEON-0148` = `P19-NEON-0149`; `P19-FABLE-0252` = `P19-GPT-0145`; `P19-FABLE-0253` = `P19-FABLE-0254` = `P19-GPT-0144`; `P19-CARBON-0153` = `P19-FABLE-0256` = `P19-GPT-0143`; `P19-CARBON-0150` = `P19-FABLE-0263` = `P19-GPT-0141` = `P19-NEON-0157`
- **Agreements:**
  - Status Strip displays Ready, Enter, Edit, or Point exactly when Excel does (plus Extend Selection, Add Selection, End Mode, Scroll Lock beside it).
  - Typing any printable character in Ready starts Enter mode replacing cell content (leading apostrophe forces text; =, +, -, @ start formulas); arrow keys in Enter commit-and-move unless following a formula operator/comma/open-paren where they enter Point mode.
  - F2 from Ready enters Edit with caret at end (and opens Formula Bar if active cell is off-screen); F2 while editing toggles cleanly between Edit (arrows move text caret) and Enter/Point (arrows pick sheet ranges).
  - Inside an active text/formula editor, arrows, Home/End, Ctrl+Arrows, Shift-selection, Ctrl+A, Backspace, Delete, Ctrl+Delete (T2), and Insert overtype (T2) act strictly on editor text without leaking to the grid.
  - Alt+Enter inserts a literal LF (CHAR(10)) line break inside text or multi-line formulas (enabling Wrap Text on text cells); Ctrl+Shift+U expands/collapses the Formula Bar (and its lower edge is draggable).
  - Delete in Ready clears contents of the whole selection while preserving formats/notes/validation; Backspace in Ready clears the active cell only and starts entry.
  - Esc cancels uncommitted edits restoring exact prior content/formatting (Esc is never historical Undo); invalid formula syntax retains the draft in Edit with Excel's message/proposed correction.
  - Locale-exact Excel entry parsing (5%, 1/2, 0 1/2, 1e3, $1,000, (100), 15-digit precision) and column text AutoComplete + Alt+Down pick-list (T2) match Excel.
- **Conflicts:**
  - **Double-click cell behavior when 'Allow editing directly in cells' option is disabled** (`P19-FABLE-0249` vs `P19-GPT-0137`) → Resolved by **[D-41](decisions.md#d-41)**
  - **Ctrl+F2 shortcut: Formula Bar edit focus vs. Desktop Excel Print Preview** (`P19-GPT-0139` vs `P19-FABLE-0250`) → Resolved by **[D-41](decisions.md#d-41)**

### CL-04-08: Commit Workflows, Enter/Tab Walks, Ctrl+Enter & Quick Entry Shortcuts

- **Summary:** Enter/Shift+Enter and Tab/Shift+Tab commit and advance (including carriage-return after a horizontal Tab run and wrapping inside multi-cell selections), Ctrl+Enter batch-fills rectangular or disjoint selections with relative-reference translation in one Undo step, quick shortcuts cover AutoSum (Alt+=), above-cell copy (Ctrl+' / Ctrl+Shift+"), and static date/time (Ctrl+; / Ctrl+Shift+:), and optional non-blocking preview-seam warnings catch silent formula-run overwrites.
- **Cards (28):** `P19-CARBON-0140`, `P19-CARBON-0151`, `P19-CARBON-0154`, `P19-CARBON-0156`, `P19-CARBON-0157`, `P19-FABLE-0069`, `P19-FABLE-0259`, `P19-FABLE-0260`, `P19-FABLE-0261`, `P19-FABLE-0264`, `P19-FABLE-0265`, `P19-FABLE-0285`, `P19-FABLE-0286`, `P19-FABLE-0287`, `P19-FABLE-0400`, `P19-FABLE-0437`, `P19-GPT-0130`, `P19-GPT-0140`, `P19-GPT-0142`, `P19-GPT-0156`, `P19-GPT-0161`, `P19-NEON-0137`, `P19-NEON-0158`, `P19-NEON-0159`, `P19-NEON-0161`, `P19-NEON-0162`, `P19-NEON-0186`, `P21-0047`
- **Sources:** P19-CARBON (5), P19-FABLE (11), P19-GPT (5), P19-NEON (6), P21 (1)
- **Disposition:** `core-with-decision` — Forms Appendix A Part 2 (Commit & Quick Entry); highlight the non-blocking Preview Seam safety badge as a signature Strand improvement.
- **Decisions:** [D-25](decisions.md#d-25)
- **`same-as` groups (6):** `P19-CARBON-0140` = `P19-FABLE-0259` = `P19-FABLE-0260` = `P19-FABLE-0265` = `P19-GPT-0130` = `P19-NEON-0137` = `P19-NEON-0159`; `P19-CARBON-0151` = `P19-FABLE-0261` = `P19-GPT-0142` = `P19-NEON-0158` = `P21-0047`; `P19-CARBON-0154` = `P19-FABLE-0285` = `P19-GPT-0156` = `P19-NEON-0186`; `P19-CARBON-0156` = `P19-FABLE-0286` = `P19-GPT-0161` = `P19-NEON-0162`; `P19-CARBON-0157` = `P19-FABLE-0287` = `P19-NEON-0161`; `P19-FABLE-0069` = `P19-FABLE-0400` = `P19-FABLE-0437`
- **Agreements:**
  - Single-cell Enter/Shift+Enter follow Excel's 'After pressing Enter, move selection' setting; after a horizontal Tab run, Enter carriage-returns to the starting column one row down; inside a multi-cell selection, Enter/Tab wrap strictly inside the selection without collapsing it.
  - Ctrl+Enter commits the typed entry across every cell in a rectangular or disjoint multi-area selection as one Undoable edit, shifting relative references from the active cell, keeping selection intact, and never intercepted by shell or agent panels (with a visible button alternative).
  - Clicking another cell while entering a literal commits and selects the clicked cell; clicking while in a formula reference position enters Point mode.
  - Alt+= AutoSum inserts =SUM(...) (or =SUBTOTAL(9,...) below a filtered Table) proposing the contiguous numeric block above or left, or fills totals across a block + empty total row/col in one stroke.
  - Ctrl+' copies the exact unshifted formula from the cell above into Edit mode; Ctrl+Shift+" copies the evaluated value from above; Ctrl+; and Ctrl+Shift+: insert static current local date and time.
- **Conflicts:**
  - **Pre-commit Preview Seam warnings on breaking a formula run, pasting hidden rows, or editing grouped sheets** (`P19-FABLE-0069`, `P19-FABLE-0400`, `P19-FABLE-0437` vs `P19-GPT-0140`, `P19-NEON-0159`) → Resolved by **[D-25](decisions.md#d-25)**

### CL-04-09: Formula Range Picking (Point Mode), Reference Highlighting & Cross-Sheet Picking

- **Summary:** Typing a formula after an operator/comma/open-paren enables full Excel Point mode across cells, ranges, headings, structured tables, dynamic spills (A1#), named ranges, 3-D sheet ranges (Jan:Dec!B5), and external workbooks — plus draggable/resizable on-grid range finders. Referenced ranges use an accessible numbered non-red palette paired with border patterns, amber highlight on the active caret reference, and off-sheet precedent badges on worksheet tabs.
- **Cards (28):** `P19-CARBON-0016`, `P19-CARBON-0043`, `P19-CARBON-0170`, `P19-CARBON-0185`, `P19-FABLE-0144`, `P19-FABLE-0282`, `P19-FABLE-0292`, `P19-FABLE-0293`, `P19-FABLE-0294`, `P19-FABLE-0295`, `P19-FABLE-0296`, `P19-FABLE-0297`, `P19-FABLE-0298`, `P19-FABLE-0299`, `P19-FABLE-0300`, `P19-FABLE-0301`, `P19-FABLE-0302`, `P19-FABLE-0304`, `P19-FABLE-0305`, `P19-FABLE-0396`, `P19-FABLE-0436`, `P19-GPT-0148`, `P19-GPT-0149`, `P19-GPT-0150`, `P19-NEON-0017`, `P19-NEON-0155`, `P19-NEON-0202`, `P21-0043`
- **Sources:** P19-CARBON (4), P19-FABLE (17), P19-GPT (3), P19-NEON (3), P21 (1)
- **Disposition:** `core-with-decision` — Forms Appendix A Part 4 (Range Picking) plus the normative Formula Reference Highlighting spec (palette + notches + active-caret amber + off-sheet tab badges).
- **Decisions:** [D-42](decisions.md#d-42)
- **`same-as` groups (3):** `P19-CARBON-0170` = `P19-FABLE-0292` = `P19-FABLE-0293` = `P19-FABLE-0295` = `P19-GPT-0148` = `P19-NEON-0155`; `P19-FABLE-0297` = `P19-GPT-0149` = `P21-0043`; `P19-FABLE-0282` = `P19-GPT-0150`
- **Agreements:**
  - In Point mode, clicking/dragging, Shift+click, Ctrl+click (adds comma + ref), arrows/Shift+arrows/Ctrl+Shift+arrows, clicking row/column/corner headings, clicking another sheet tab (quoting names as Excel does; Shift+tab for 3-D Jan:Dec!B5; Ctrl+PgUp/PgDn without committing), or another open workbook (T2, absolute [Book.xlsx]Sheet!$A$1) insert live references at the caret.
  - Smart reference syntax matches Excel: structured references inside Tables (Orders[Amount], [@Amount], Orders[[#Totals],[Amount]]), spill reference A1# when selecting an entire spill, defined name when selecting an exact named range (T2), and GETPIVOTDATA when enabled (T2).
  - Typing an operator/comma/paren ends pointing and keeps the reference; scrolling with wheel/scrollbars/Scroll Lock pans without altering the pointed reference; dragging a highlighted reference's border moves it and dragging its corner resizes it live while preserving $ flags.
  - Formula reference highlighting NEVER uses error red (#D02A23) and ALWAYS pairs hue with numbered badges (①②③④ / superscripts) and distinct border patterns so colour-vision deficiency never loses pairing.
  - During F2 edit/point mode, worksheet tabs containing referenced precedents display matching ①②③④ badges (plus scalar value previews when scalar, e.g. Inputs [② $420]), and add-in UDF mid-edit previews show ◈ provenance and Excel-safe cache portability notes.
- **Conflicts:**
  - **Formula Reference Palette slots and whether amber highlights the active caret reference** (`P19-CARBON-0016`, `P19-CARBON-0043` vs `P19-FABLE-0144`, `P19-FABLE-0396`, `P19-FABLE-0436`, `P19-NEON-0017`, `P19-NEON-0202`) → Resolved by **[D-42](decisions.md#d-42)**

### CL-04-10: F4 Reference Anchoring, Repeat Command & A1/R1C1 Style

- **Summary:** F4 in Edit or Point mode cycles absolute/relative $ anchoring (A1 -> $A$1 -> A$1 -> $A1 -> A1) across single references, both endpoints of ranges, or all references inside a text selection (with four accessible anchor buttons mirroring it); F4 in Ready repeats the last repeatable formatting or structural command. Switching between A1 and R1C1 reference styles changes syntax display/input without mutating formula semantics.
- **Cards (11):** `P19-CARBON-0146`, `P19-CARBON-0147`, `P19-FABLE-0274`, `P19-FABLE-0275`, `P19-FABLE-0303`, `P19-GPT-0151`, `P19-GPT-0152`, `P19-GPT-0228`, `P19-NEON-0150`, `P19-NEON-0151`, `P21-0045`
- **Sources:** P19-CARBON (2), P19-FABLE (3), P19-GPT (3), P19-NEON (2), P21 (1)
- **Disposition:** `core` — Full cross-source unanimity; test F4 on single cells, ranges, cross-sheet refs, multi-selection, structured refs (baseline-verified), and R1C1.
- **`same-as` groups (2):** `P19-CARBON-0146` = `P19-FABLE-0274` = `P19-FABLE-0303` = `P19-GPT-0151` = `P19-NEON-0150` = `P21-0045`; `P19-CARBON-0147` = `P19-FABLE-0275` = `P19-GPT-0152` = `P19-NEON-0151`
- **Agreements:**
  - F4 in Edit or immediately after pointing cycles A1 -> $A$1 -> A$1 -> $A1 -> A1, cycling both endpoints of a range synchronously and every reference intersecting a text selection, without changing the cell's current evaluated value.
  - Four optional/visible anchor buttons in the formula toolbar mirror F4 for discoverability and pointer/touch users (P21-0045).
  - F4 in Ready repeats the last repeatable formatting or structural command (never enters formula edit merely because a formula cell is selected; labels clearly if the last action cannot be repeated).
  - A1 / R1C1 reference style switching updates display, reference picking, F4 cycling, copy, and fill cleanly in both notations without altering formula meaning.
- **Conflicts:** None

### CL-04-11: Formula Autocomplete, Function Tooltips, Insert Function & Name Manager

- **Summary:** Formula editing provides Excel-faithful AutoComplete across functions, defined names, tables (including [ / @ / # structured-reference triggers), and loaded add-ins; interactive function signature tooltips bold the active argument; Shift+F3 opens Insert Function (routed to Formula Explorer's Functions facet), F3 pastes a defined name, and Ctrl+F3 / Ctrl+Shift+F3 manage and create defined names.
- **Cards (8):** `P19-FABLE-0278`, `P19-FABLE-0279`, `P19-FABLE-0280`, `P19-FABLE-0281`, `P19-GPT-0146`, `P19-GPT-0147`, `P19-GPT-0189`, `P19-NEON-0156`
- **Sources:** P19-FABLE (4), P19-GPT (3), P19-NEON (1)
- **Disposition:** `core` — Full agreement across sources; integrate Shift+F3 directly with Formula Explorer (Chapter IX) while keeping Excel's exact keyboard loop.
- **`same-as` groups (3):** `P19-FABLE-0280` = `P19-GPT-0146` = `P19-NEON-0156`; `P19-FABLE-0278` = `P19-GPT-0189`; `P19-FABLE-0279` = `P19-GPT-0147`
- **Cross-cluster `same-as`:** `P19-FABLE-0279` = `P19-FABLE-0156` (CL-09-04)
- **Agreements:**
  - Formula AutoComplete triggers after '=' and a letter (listing Excel functions, defined names, tables, and loaded add-in UDFs); Up/Down choose, Tab inserts (opening '(' for functions), Esc dismisses without cancelling the cell edit; inside a table ref, '[' lists columns, '@' means current row, '#' lists special items.
  - Function tooltip displays the signature with the active argument bolded; clicking an argument selects it in the formula, and clicking the function name opens its help.
  - Ctrl+A after a function name opens argument fields; Ctrl+Shift+A inserts parameter placeholders.
  - Shift+F3 opens Insert Function (Formula Explorer's Functions view with search, categories, and argument fields); F3 opens Paste Name (explaining and offering Define Name when empty); Ctrl+F3 opens Name Manager (showing scope, definition, value/error, refusing duplicates/invalid names without silent scope changes); Ctrl+Shift+F3 creates names from selection labels.
- **Conflicts:** None

### CL-04-12: Calculation (F9), In-Formula Sub-Expression Evaluation & Iterative/Circular Models

- **Summary:** Outside editing, F9, Shift+F9, Ctrl+Alt+F9, and Ctrl+Alt+Shift+F9 execute Excel's exact four calculation scopes (without refreshing external data sources) and step iterative/ticked models. Inside formula editing, F9 evaluates the highlighted sub-expression in-place with an explicit draft warning and reversible Ctrl+Z/Esc recovery. Circular references with iteration off return 0 with the loop named; iterative models display exact iteration settings honestly without fabricating convergence counts.
- **Cards (14):** `P19-CARBON-0039`, `P19-CARBON-0148`, `P19-CARBON-0149`, `P19-FABLE-0272`, `P19-FABLE-0276`, `P19-FABLE-0277`, `P19-GPT-0153`, `P19-GPT-0154`, `P19-GPT-0155`, `P19-NEON-0035`, `P19-NEON-0152`, `P19-NEON-0153`, `P19-NEON-0154`, `P21-0051`
- **Sources:** P19-CARBON (3), P19-FABLE (3), P19-GPT (3), P19-NEON (4), P21 (1)
- **Disposition:** `core-with-decision` — Forms Appendix A Part 3 calculation keys plus iterative/circular honesty rules.
- **Decisions:** [D-43](decisions.md#d-43), [D-44](decisions.md#d-44)
- **`same-as` groups (2):** `P19-CARBON-0148` = `P19-FABLE-0277` = `P19-GPT-0154` = `P19-GPT-0155` = `P19-NEON-0152` = `P19-NEON-0153`; `P19-FABLE-0276` = `P19-GPT-0153` = `P19-NEON-0154`
- **Agreements:**
  - Outside editing: F9 calculates dirty formulas across open workbooks (and advances +1 tick under Ticks/Iteration mode); Shift+F9 calculates the active sheet; Ctrl+Alt+F9 forces full recalculation; Ctrl+Alt+Shift+F9 rebuilds the dependency graph and recalculates all. F9 never refreshes external connections (Alt+F5 does).
  - Stale results remain explicitly labelled while calculation runs; cancellation state and requested scope are shown honestly.
  - Circular reference with iteration disabled matches Excel (evaluates to 0 and names the circular cell in the Strip without a blocking modal).
  - Iterative models (such as Stock.xlsx) show explicit iteration settings (max iterations, tolerance), subtle iterative [↻] markers, and never invent a fake convergence iteration count on non-converging loops.
- **Conflicts:**
  - **F9 in-formula sub-expression evaluation UX (direct literal replacement vs reversible preview pill + Ctrl+Z un-bake)** (`P19-CARBON-0149` vs `P19-FABLE-0276`, `P19-GPT-0153`, `P19-NEON-0154`) → Resolved by **[D-43](decisions.md#d-43)**
  - **Live tick/slider recalculation animation & continuous 120 fps Form Control syncing vs honest bounded recalculation** (`P19-CARBON-0039`, `P19-NEON-0035` vs `P21-0051`) → Resolved by **[D-44](decisions.md#d-44)**

### CL-04-13: Formula Auditing, Show Formulas & Precedent/Dependent Tracing

- **Summary:** Classic supports Excel's full auditing keyboard repertoire: Ctrl+` toggles Show Formulas (auto-widening columns temporarily), Ctrl+[/] and Ctrl+Shift+{/} select direct and transitive precedents/dependents across sheets, Alt, M, P/D/A,A draw and clear navigable tracer arrows, Alt, M, V routes Evaluate Formula into Formula Explorer's Steps facet, and Excel's audit error flags respond to Alt+Shift+F10.
- **Cards (11):** `P19-CARBON-0164`, `P19-FABLE-0283`, `P19-FABLE-0288`, `P19-FABLE-0289`, `P19-FABLE-0290`, `P19-FABLE-0291`, `P19-FABLE-0404`, `P19-GPT-0216`, `P19-GPT-0217`, `P19-NEON-0184`, `P19-NEON-0185`
- **Sources:** P19-CARBON (1), P19-FABLE (6), P19-GPT (2), P19-NEON (2)
- **Disposition:** `core` — Complete harmony across all sources; note explicitly in D-11 that Alt, M, V and Shift+F3 route to Formula Explorer without breaking muscle memory.
- **`same-as` groups (3):** `P19-FABLE-0283` = `P19-NEON-0184`; `P19-CARBON-0164` = `P19-FABLE-0288` = `P19-GPT-0217` = `P19-NEON-0185`; `P19-FABLE-0289` = `P19-GPT-0216`
- **Agreements:**
  - Ctrl+` (backtick) toggles Show Formulas in cells in place of values, temporarily widening columns for legibility and restoring prior widths when toggled off (distinct from Formula Explorer / Lens).
  - Ctrl+[ selects direct precedents, Ctrl+] selects direct dependents, Ctrl+Shift+{ selects all transitive precedents, and Ctrl+Shift+} selects all transitive dependents (navigating across sheets cleanly and observing baseline fixtures for closed-file links).
  - Alt, M, P (Trace Precedents), Alt, M, D (Trace Dependents), and Alt, M, A, A (Remove Arrows) draw real dependency levels with dashed off-sheet indicators and honest endpoints for missing external workbooks; double-clicking an arrow follows it.
  - Alt, M, V (Evaluate Formula) opens Formula Explorer at its Steps facet (offering Evaluate, Step In, Step Out, and full expression tree) — preserving the Excel shortcut while upgrading the destination tool (D-11).
  - Excel's background audit flags (number stored as text, inconsistent formula in run, omitted adjacent cells, unlocked formula, two-digit year) are flagged and expose their Excel choices (including Ignore) via Alt+Shift+F10 (T2).
- **Conflicts:** None

### CL-04-14: Dynamic Arrays, Spills & Legacy CSE Array Formulas

- **Summary:** Dynamic array formulas spill natively on plain Enter with a crisp teal spill outline, greyed read-only formula projections on spill children, and actionable inline #SPILL! obstruction diagnostics. Legacy Ctrl+Shift+Enter (CSE) array formulas remain fully supported without silent auto-conversion, refusing partial interior edits with a one-click Select Array action.
- **Cards (11):** `P19-CARBON-0152`, `P19-CARBON-0218`, `P19-FABLE-0102`, `P19-FABLE-0262`, `P19-FABLE-0271`, `P19-FABLE-0399`, `P19-GPT-0157`, `P19-GPT-0158`, `P19-GPT-0159`, `P19-GPT-0160`, `P19-NEON-0163`
- **Sources:** P19-CARBON (2), P19-FABLE (4), P19-GPT (4), P19-NEON (1)
- **Disposition:** `core-with-decision` — Specify spill rendering, spill-child protection, #SPILL! inline card with explicit undoable actions, and CSE refusal rules; record C-12 in system needs.
- **Decisions:** [D-45](decisions.md#d-45)
- **`same-as` groups (2):** `P19-CARBON-0152` = `P19-FABLE-0262` = `P19-GPT-0157` = `P19-NEON-0163`; `P19-FABLE-0271` = `P19-GPT-0159`
- **Agreements:**
  - Dynamic array formulas (FILTER, SORT, SEQUENCE, UNIQUE, TRANSPOSE, etc.) spill natively on plain Enter; implicit intersection '@' renders exactly where Excel shows it.
  - Selecting a spill child shows the anchor address, teal spill boundary outline, and greyed read-only formula projection; spill children cannot be edited independently, and Delete/paste never silently redirects to the whole array (offer 'Edit source' to jump to the anchor).
  - Legacy Ctrl+Shift+Enter (CSE) commits multi-cell braced array formulas for Excel compatibility and never auto-converts between CSE and dynamic arrays; editing or deleting an interior CSE cell refuses partial mutation, displays the full array address, and offers 'Select array'.
  - The engine exposes exact spill extents and exact blocking cell coordinates for every #SPILL! anchor (system need C-12).
- **Conflicts:**
  - **#SPILL! remediation actions (inline 3-action card vs strictly manual blocker removal)** (`P19-FABLE-0102`, `P19-FABLE-0399` vs `P19-GPT-0160`) → Resolved by **[D-45](decisions.md#d-45)**

### CL-04-15: Grid Navigation, End Mode, Viewport Paging & Navigation History

- **Summary:** Classic delivers exact Excel grid navigation across virtualised sheets: single-cell arrows (skipping hidden rows/columns and stepping merged cells as one unit), Ctrl+Arrow and sticky End-Mode edge jumps, Home/Ctrl+Home/Ctrl+End, vertical and horizontal viewport paging, Scroll Lock panning, Ctrl+Backspace active-cell reveal, F6 pane cycling, double-click border jumps, scrollbar row/col tooltips, and Alt+Left/Right cross-sheet/cross-view navigation history.
- **Cards (32):** `P19-CARBON-0029`, `P19-CARBON-0134`, `P19-CARBON-0135`, `P19-CARBON-0136`, `P19-CARBON-0137`, `P19-CARBON-0138`, `P19-CARBON-0142`, `P19-FABLE-0306`, `P19-FABLE-0307`, `P19-FABLE-0308`, `P19-FABLE-0309`, `P19-FABLE-0311`, `P19-FABLE-0312`, `P19-FABLE-0318`, `P19-FABLE-0321`, `P19-FABLE-0325`, `P19-GPT-0112`, `P19-GPT-0119`, `P19-GPT-0120`, `P19-GPT-0121`, `P19-GPT-0122`, `P19-GPT-0123`, `P19-GPT-0134`, `P19-GPT-0220`, `P19-NEON-0130`, `P19-NEON-0131`, `P19-NEON-0132`, `P19-NEON-0133`, `P19-NEON-0134`, `P19-NEON-0136`, `P19-NEON-0138`, `P19-NEON-0140`
- **Sources:** P19-CARBON (7), P19-FABLE (9), P19-GPT (8), P19-NEON (8)
- **Disposition:** `core` — Forms Appendix A Part 5 (Navigation); emphasize GPT A-13's rule that semantic block detectors never alter Ctrl+Arrow behavior.
- **`same-as` groups (8):** `P19-CARBON-0134` = `P19-FABLE-0306` = `P19-GPT-0119` = `P19-NEON-0130`; `P19-CARBON-0135` = `P19-FABLE-0307` = `P19-GPT-0120` = `P19-NEON-0131`; `P19-CARBON-0136` = `P19-GPT-0122` = `P19-NEON-0132`; `P19-CARBON-0137` = `P19-FABLE-0308` = `P19-GPT-0121` = `P19-NEON-0133`; `P19-CARBON-0138` = `P19-FABLE-0309` = `P19-GPT-0123` = `P19-NEON-0134`; `P19-CARBON-0142` = `P19-FABLE-0311` = `P19-NEON-0136`; `P19-FABLE-0312` = `P19-GPT-0134` = `P19-NEON-0138`; `P19-FABLE-0318` = `P19-GPT-0112`
- **Agreements:**
  - Arrow keys move 1 cell skipping hidden rows/columns and stepping across merged cells as one unit (exposing extent and top-left storage cell; never inventing hidden values in covered merged cells).
  - Ctrl+Arrow jumps to Excel's exact contiguous data boundary (verified against fixtures for blank islands, empty-string formulas, hidden rows, and sheet edge XFD/1,048,576 — never redefined by any semantic block detector).
  - End (tap/release) enters visible single-shot End Mode in the Strip; next Arrow behaves like Ctrl+Arrow (End, Home -> used-range bottom-right; End, Enter -> last occupied col in row; End again cancels).
  - Home -> Col A (or first unfrozen col); Ctrl+Home -> A1 (or top-left unfrozen cell); Ctrl+End -> bottom-right cell of active used range (honestly reflecting used-range formatting).
  - PageUp/PageDown and Alt+PageUp/Alt+PageDown scroll and move selection by one viewport height/width respecting frozen panes; Ctrl+Backspace scrolls back to the active cell without collapsing selection or leaving F2 edit.
  - Scroll Lock turns on a visible/announced Strip indicator so arrows pan the viewport while holding the active cell; scrollbar thumb dragging previews target row/col (T2); double-clicking a cell's border jumps to the data edge on that side (with Shift, selects to it; T2).
  - F6 / Shift+F6 cycle landmarks (stage, split panes, companion/open panels, Deck, Mast, Strip) preserving focus inside each; Alt+Left / Alt+Right traverse cross-sheet/cross-view navigation history.
  - Virtualised grid rendering supports instant (~11 ms) cross-view jumps into large workbooks (e.g. Gas_Daily_2001!AT142).
- **Conflicts:** None

### CL-04-16: Selection, Multi-Area Ranges, Visible Cells Only & Selection Modes

- **Summary:** Classic supports full Excel rectangular, header, corner, table-staged, and disjoint multi-area selection (including Ctrl+click deselection), sticky Extend (F8) and Add (Shift+F8) modes, Ctrl+A staged expansion, Ctrl+Space/Shift+Space table-aware row/column selection, Ctrl+./Ctrl+Alt+Arrows/Shift+Backspace area navigation, Alt+; Visible Cells Only, and audit-difference selectors (Ctrl+\, Ctrl+Shift+|, Ctrl+Shift+O).
- **Cards (33):** `P19-CARBON-0141`, `P19-CARBON-0160`, `P19-CARBON-0161`, `P19-CARBON-0162`, `P19-CARBON-0163`, `P19-CARBON-0165`, `P19-FABLE-0324`, `P19-FABLE-0326`, `P19-FABLE-0327`, `P19-FABLE-0328`, `P19-FABLE-0329`, `P19-FABLE-0330`, `P19-FABLE-0331`, `P19-FABLE-0332`, `P19-FABLE-0333`, `P19-FABLE-0334`, `P19-FABLE-0338`, `P19-GPT-0116`, `P19-GPT-0117`, `P19-GPT-0118`, `P19-GPT-0126`, `P19-GPT-0127`, `P19-GPT-0128`, `P19-GPT-0129`, `P19-GPT-0131`, `P19-GPT-0248`, `P19-NEON-0141`, `P19-NEON-0142`, `P19-NEON-0143`, `P19-NEON-0144`, `P19-NEON-0145`, `P19-NEON-0146`, `P19-NEON-0147`
- **Sources:** P19-CARBON (6), P19-FABLE (11), P19-GPT (9), P19-NEON (7)
- **Disposition:** `core` — Forms Appendix A Part 6 (Selection); strong unanimity across all sources including accessibility exposure.
- **`same-as` groups (8):** `P19-CARBON-0165` = `P19-FABLE-0332` = `P19-GPT-0116` = `P19-GPT-0117` = `P19-GPT-0118`; `P19-FABLE-0326` = `P19-NEON-0141`; `P19-CARBON-0160` = `P19-FABLE-0327` = `P19-GPT-0126` = `P19-NEON-0142`; `P19-CARBON-0161` = `P19-FABLE-0328` = `P19-GPT-0127` = `P19-NEON-0143`; `P19-FABLE-0329` = `P19-GPT-0128`; `P19-CARBON-0162` = `P19-FABLE-0331` = `P19-GPT-0129` = `P19-NEON-0144` = `P19-NEON-0145`; `P19-CARBON-0141` = `P19-GPT-0131` = `P19-NEON-0146`; `P19-CARBON-0163` = `P19-NEON-0147`
- **Agreements:**
  - Click/drag, Shift+click, and Ctrl+click/drag support rectangular, extended, and noncontiguous multi-area selection (Ctrl+click on an already-selected cell removes it from selection); dragging shows live dimensions and exposes address, dimensions, active cell, and error state to assistive technology.
  - Row/column headings select rows/columns (with Shift/Ctrl) and the top-left corner selects the entire sheet; active cell stays untinted inside a tinted selection, selected headings are marked, and separate areas are each outlined.
  - Shift+Space / Ctrl+Space select rows/columns with Excel's staged Table progression (Table data -> Table with header -> full sheet row/col); Ctrl+A expands progressively (Table body -> Table+headers/totals -> Sheet; or Current Region -> Sheet; all objects when an object is selected; editor text when editing).
  - Ctrl+Shift+* selects current region (bounded strictly by blanks, never guessed model blocks); Ctrl+/ selects current array; Ctrl+\ (row differences), Ctrl+Shift+| (column differences), Ctrl+Shift+O (cells with notes), and Alt+; (Visible Cells Only) match Excel.
  - F8 toggles sticky Extend mode and Shift+F8 toggles Add-to-Selection mode (both named in Strip, announced once, exited with Esc); Ctrl+. cycles corners clockwise; Ctrl+Alt+Right/Left switch between disjoint areas (T2); Shift+Backspace collapses selection to active cell (T2).
  - Multi-area, filtered, grouped-sheet, merged-cell, and protected selections always keep their honest real extent; any incompatible downstream operation explains the exact refusal without partial mutation (R-18).
- **Conflicts:** None

### CL-04-17: Name Box, Go To (F5 / Ctrl+G), Go To Special & Find/Replace (Ctrl+F / Ctrl+H)

- **Summary:** The Name Box navigates to addresses/names and defines names on the fly; F5 / Ctrl+G open Go To with a return jump stack (F5, Enter jumps back) and Go To Special (Alt+S / Alt, H, FD, S); Ctrl+F and Ctrl+H open a non-modal docked Find/Replace bar exposing Excel's full option set, Find All navigable results, single-Undo Replace All, and Shift+F4 / Ctrl+Shift+F4 next/previous stepping.
- **Cards (11):** `P19-CARBON-0143`, `P19-FABLE-0313`, `P19-FABLE-0314`, `P19-FABLE-0315`, `P19-FABLE-0316`, `P19-GPT-0132`, `P19-GPT-0133`, `P19-GPT-0190`, `P19-GPT-0191`, `P19-NEON-0139`, `P19-NEON-0188`
- **Sources:** P19-CARBON (1), P19-FABLE (4), P19-GPT (4), P19-NEON (2)
- **Disposition:** `core-with-decision` — Unifies Name Box, Go To/Special, and docked full-parity Find/Replace in Chapter IV and Appendix A.
- **Decisions:** [D-46](decisions.md#d-46)
- **`same-as` groups (3):** `P19-CARBON-0143` = `P19-FABLE-0313` = `P19-GPT-0132` = `P19-NEON-0139`; `P19-FABLE-0314` = `P19-GPT-0133`; `P19-FABLE-0316` = `P19-GPT-0190`
- **Agreements:**
  - The Name Box accepts A1/R1C1 addresses, sheet-qualified targets (Monthly!B4), spill refs (Report!H4#), multi-area ranges, and defined names (listing existing names in its dropdown); typing a fresh name with cells selected defines that name immediately.
  - F5 / Ctrl+G open Go To listing Defined Names, Tables, and recent jumps so F5, Enter returns to the previous location; invalid, missing, and protected targets give distinct explanations.
  - Go To Special (Alt+S from Go To, or Alt, H, FD, S) supports Excel's categories: Constants/Formulas (by Num/Text/Logical/Error), Blanks, Current Region, Current Array, Objects, Row/Column Differences, Direct/All Precedents & Dependents, Last Cell, Visible Cells Only (Alt+;), Conditional Formats, and Data Validation.
  - Ctrl+F and Ctrl+H support Sheet/Workbook/Selection scope, Rows/Columns search order, Formulas/Values/Notes/Comments look-in, Match Case, Whole Cell, Formats, Excel wildcards (?, *, ~), navigable Find All list, Replace All with count and one Undo group, and Shift+F4 / Ctrl+Shift+F4 Find Next/Previous.
- **Conflicts:**
  - **Find & Replace chrome presentation (non-modal docked Bridge bar vs. full Excel Find/Replace option dialog)** (`P19-NEON-0188` vs `P19-FABLE-0316`, `P19-GPT-0190`) → Resolved by **[D-46](decisions.md#d-46)**

### CL-04-18: Fill Handle, AutoFill, Double-Click Fill, Ctrl+D/Ctrl+R & Flash Fill

- **Summary:** Classic implements Excel's complete fill system: dragging the 5x5 px bottom-right fill handle with live ghost-value/formula previews, Ctrl+drag copy/series inversion, post-drop AutoFill Options chip, right-button drag fill menu, double-click auto-fill verified against observed Excel fixtures, inward drag-to-clear, Shift+drag insert/remove, Ctrl+D/Ctrl+R, Ctrl+E Flash Fill (disclosing literal outputs), Fill menu (Alt, H, F, I), visible-only filtered fills, and single-step Undo.
- **Cards (26):** `P19-CARBON-0155`, `P19-CARBON-0167`, `P19-CARBON-0168`, `P19-FABLE-0339`, `P19-FABLE-0340`, `P19-FABLE-0341`, `P19-FABLE-0342`, `P19-FABLE-0343`, `P19-FABLE-0344`, `P19-FABLE-0345`, `P19-FABLE-0346`, `P19-FABLE-0347`, `P19-FABLE-0348`, `P19-FABLE-0349`, `P19-FABLE-0350`, `P19-GPT-0170`, `P19-GPT-0171`, `P19-GPT-0172`, `P19-GPT-0215`, `P19-NEON-0160`, `P19-NEON-0164`, `P19-NEON-0165`, `P19-NEON-0166`, `P19-NEON-0167`, `P19-NEON-0168`, `P21-0046`
- **Sources:** P19-CARBON (3), P19-FABLE (12), P19-GPT (4), P19-NEON (6), P21 (1)
- **Disposition:** `core-with-decision` — Forms Appendix A Part 7 (Fill Handle & Filling); tie double-click fill directly to captured Excel baseline fixtures.
- **Decisions:** [D-47](decisions.md#d-47)
- **`same-as` groups (6):** `P19-CARBON-0167` = `P19-FABLE-0340` = `P19-GPT-0170` = `P19-NEON-0164` = `P21-0046`; `P19-FABLE-0341` = `P19-NEON-0165`; `P19-FABLE-0343` = `P19-NEON-0166`; `P19-FABLE-0345` = `P19-NEON-0168`; `P19-CARBON-0155` = `P19-FABLE-0347` = `P19-GPT-0172` = `P19-NEON-0160`; `P19-CARBON-0168` = `P19-FABLE-0344` = `P19-NEON-0167`
- **Agreements:**
  - Dragging the bottom-right fill handle (honouring Excel's toggle option) shows a destination hatch and live tooltip preview of the ending value/adjusted formula, translates relative formula references, copies single numbers/text, increments recognized sequences (two-cell step, text+number, dates, weekdays, months, quarters, custom lists), and drops a non-blocking AutoFill Options chip (Copy, Series, Formatting Only, Without Formatting, Date Units, Flash Fill).
  - Holding Ctrl while dragging the fill handle inverts copy <-> series with a '+' badge; right-button dragging opens the full Fill menu including Linear/Growth Trend and Series (T2); dragging inward over the selection hatches and clears contents (T2); Shift+dragging inserts/removes cells (T2).
  - Ctrl+D / Ctrl+R fill down/right from the selection's top row / leftmost column (or from the adjacent upper/left cell for a single-cell selection) as one Undoable edit; Alt, H, F, I exposes Up/Left/Across Worksheets/Series/Justify/Flash Fill (T2); visible Fill button aids pointer/keyboard accessibility.
  - Ctrl+E runs Flash Fill from examples (disclosing that outputs are literal values rather than formulas, or stating clearly if unavailable, in one Undo step); filling filtered rows fills visible cells only; every fill reverses in a single Undo step.
- **Conflicts:**
  - **Double-click Fill Handle stopping rule (fixed left-then-right heuristic vs Excel-observed edge-case fixtures)** (`P19-CARBON-0168`, `P19-FABLE-0344`, `P19-NEON-0167` vs `P19-GPT-0171`) → Resolved by **[D-47](decisions.md#d-47)**

### CL-04-19: Border Drag-Move, Drag-Copy & Insert-Shift

- **Summary:** Dragging the selection border moves cells (updating dependent formulas across the workbook identical to Cut+Paste), Ctrl+drag copies with a '+' badge, Shift+drag and Ctrl+Shift+drag show a crisp insertion bar to move/copy-and-insert rows/columns/cells, Alt+drag over a sheet tab transfers across sheets (T2), and right-button border drag opens the full move/copy/link/shift menu (T2), all honouring edge auto-scroll, Esc cancel, and Excel's drag-and-drop preference.
- **Cards (18):** `P19-CARBON-0169`, `P19-FABLE-0351`, `P19-FABLE-0352`, `P19-FABLE-0353`, `P19-FABLE-0354`, `P19-FABLE-0355`, `P19-FABLE-0356`, `P19-FABLE-0357`, `P19-GPT-0173`, `P19-GPT-0174`, `P19-GPT-0175`, `P19-GPT-0176`, `P19-NEON-0169`, `P19-NEON-0170`, `P19-NEON-0171`, `P19-NEON-0172`, `P19-NEON-0173`, `P21-0048`
- **Sources:** P19-CARBON (1), P19-FABLE (7), P19-GPT (4), P19-NEON (5), P21 (1)
- **Disposition:** `core-with-decision` — Forms Appendix A Part 8 (Moving and Copying by Dragging).
- **Decisions:** [D-47](decisions.md#d-47)
- **`same-as` groups (6):** `P19-GPT-0173` = `P21-0048`; `P19-CARBON-0169` = `P19-FABLE-0351`; `P19-FABLE-0352` = `P19-GPT-0174` = `P19-NEON-0170`; `P19-FABLE-0353` = `P19-GPT-0175` = `P19-NEON-0171` = `P19-NEON-0172`; `P19-FABLE-0354` = `P19-NEON-0173`; `P19-GPT-0176` = `P19-NEON-0169`
- **Agreements:**
  - Dragging any edge of the selection border (distinct from interior selection drag and fill handle) previews the destination outline and target address, moves values/formulas/formats/notes intact, rewrites workbook references pointing at the moved cells, and reverses cleanly in one Undo step (e.g. moving Summary!A21 -> D21 to unblock #SPILL!).
  - Holding Ctrl switches live to Drag-Copy ('+' pointer badge + 'Copy cells' preview, shifting relative references); holding Shift (or Ctrl+Shift) displays a crisp insertion I-beam to move/copy and shift displaced rows/columns/cells cleanly without overwriting (refusing noncontiguous row drags rather than approximating).
  - Alt+drag border onto a sheet tab switches onto that sheet after a ~400 ms dwell to drop across sheets (T2); right-button border drag opens Move/Copy/Values/Formats/Link/Hyperlink/Shift menu on release (T2).
  - Dragging near viewport edges auto-scrolls smoothly; Esc cancels any drag with zero mutation; Excel's 'Enable fill handle and cell drag-and-drop' setting is honoured (T2).
- **Conflicts:**
  - **Occupied-target overwrite confirmation during border drag-move** (`P19-CARBON-0169`, `P19-FABLE-0351` vs `P19-GPT-0176`, `P19-NEON-0169`) → Resolved by **[D-47](decisions.md#d-47)**

### CL-04-20: Clipboard, Paste Special, Format Painter & External/Cross-View Interop

- **Summary:** Classic delivers Excel clipboard fidelity: Ctrl+C/Ctrl+X (live cut rewriting dependents), Ctrl+V/Enter (with single-cell broadcast and whole-multiple tiling), Ctrl+Alt+V Paste Special with Excel's full option set and keyboard letters, Ctrl+Shift+V Paste Values + post-paste Ctrl options chip, Esc cancel, multi-area and filtered-list paste parity, safe TSV/HTML/image external interop preserving leading zeros, cross-view DNA Calc link-vs-snapshot paste, and Format Painter.
- **Cards (24):** `P19-FABLE-0358`, `P19-FABLE-0359`, `P19-FABLE-0360`, `P19-FABLE-0361`, `P19-FABLE-0362`, `P19-FABLE-0363`, `P19-FABLE-0365`, `P19-FABLE-0366`, `P19-FABLE-0367`, `P19-FABLE-0368`, `P19-FABLE-0369`, `P19-FABLE-0395`, `P19-GPT-0162`, `P19-GPT-0163`, `P19-GPT-0164`, `P19-GPT-0165`, `P19-GPT-0166`, `P19-GPT-0167`, `P19-GPT-0168`, `P19-GPT-0169`, `P19-NEON-0175`, `P19-NEON-0176`, `P19-NEON-0177`, `P19-NEON-0178`
- **Sources:** P19-FABLE (12), P19-GPT (8), P19-NEON (4)
- **Disposition:** `core-with-decision` — Forms Appendix A Part 9 (Clipboard & Interop); record static copy outline in Part 12 D-2.
- **Decisions:** [D-20](decisions.md#d-20)
- **`same-as` groups (5):** `P19-FABLE-0358` = `P19-FABLE-0359` = `P19-GPT-0162`; `P19-FABLE-0360` = `P19-GPT-0163` = `P19-NEON-0176`; `P19-FABLE-0361` = `P19-GPT-0165` = `P19-NEON-0177`; `P19-FABLE-0362` = `P19-GPT-0166`; `P19-FABLE-0367` = `P19-FABLE-0368` = `P19-GPT-0167` = `P19-GPT-0168` = `P19-NEON-0178`
- **Cross-cluster `same-as`:** `P19-FABLE-0395` = `P19-FABLE-0038` (CL-01-10); `P19-GPT-0169` = `P19-GPT-0327` (CL-03-12)
- **Agreements:**
  - Ctrl+C marks the source until Esc, a new entry, or another copy; Ctrl+X enters Live Cut (clearing source only on paste and rewriting dependent formulas); Ctrl+V pastes repeatedly (broadcasting 1 cell into many and tiling whole-multiple blocks) while Enter pastes once and clears the clipboard state; Esc cancels without erasing source.
  - Ctrl+Alt+V opens Paste Special with Excel's full matrix (All, Formulas, Values, Formats, Comments/Notes, Validation, Source Theme, All Except Borders, Column Widths, Formulas/Values & Number Formats, Add/Subtract/Multiply/Divide, Skip Blanks, Transpose, Paste Link) and keyboard letters; Ctrl+Shift+V pastes values; Ctrl after paste opens the Paste Options chip.
  - Multi-area copy is allowed where areas share rows or columns (showing Excel's message otherwise, T2); pasting 1 cell into a filtered selection fills visible cells only while pasting a block follows Excel's consecutive-row rule (with Alt+; copy-visible and non-blocking hidden-row seam warning).
  - External interop writes TSV, styled HTML table, image, and internal spreadsheet metadata (disclosing when formulas cannot travel), and pastes TSV/CSV/HTML/images safely (preserving leading-zero identifiers via Paste as text, offering Match Destination Formatting, never executing imported HTML).
  - Cross-view DNA Calc paste: pasting Classic cells into Notebook defaults to 'Link existing cells' (or explicit value snapshot), while pasting into Classic writes cells; Format Painter paints once on click and locks until Esc on double-click (T2).
- **Conflicts:**
  - **Copy source outline animation (static dashed outline vs animated marching ants)** (`P19-FABLE-0395` vs `P19-GPT-0164`, `P19-NEON-0175`) → Resolved by **[D-20](decisions.md#d-20)**

### CL-04-21: Rows, Columns, Insert/Delete, Resize/AutoFit, Hide/Unhide & Outlining

- **Summary:** Classic supports Excel's complete row and column manipulation gestures: Ctrl+Shift+= / Ctrl+- for insert/delete (including inserting waiting cut/copied cells), header-divider drag resizing across multi-selected headings, double-click AutoFit, drag-to-zero hiding, Ctrl+9/0 and Shift chords for hide/unhide (honestly handling Windows' Ctrl+Shift+0 reservation), and Alt+Shift+Arrows / Ctrl+8 outline grouping.
- **Cards (11):** `P19-CARBON-0166`, `P19-FABLE-0364`, `P19-FABLE-0371`, `P19-FABLE-0373`, `P19-FABLE-0374`, `P19-GPT-0177`, `P19-GPT-0178`, `P19-GPT-0179`, `P19-NEON-0174`, `P19-NEON-0182`, `P19-NEON-0183`
- **Sources:** P19-CARBON (1), P19-FABLE (4), P19-GPT (3), P19-NEON (3)
- **Disposition:** `core` — Full consensus across all sources; note Windows Ctrl+Shift+0 OS reservation honestly in Appendix A.
- **`same-as` groups (3):** `P19-CARBON-0166` = `P19-FABLE-0371` = `P19-GPT-0178` = `P19-NEON-0174`; `P19-FABLE-0364` = `P19-GPT-0177` = `P19-NEON-0182`; `P19-FABLE-0373` = `P19-GPT-0179` = `P19-NEON-0183`
- **Agreements:**
  - Dragging a row/column heading divider shows a live size tooltip and resizes all selected rows/columns simultaneously; double-clicking the divider AutoFits width/height to content + header; dragging to zero hides and dragging out reveals; divider drags can never accidentally move cell data.
  - Ctrl+Shift+= (Ctrl++) and Ctrl+- insert/delete selected whole rows/columns immediately, or open a compact shift-direction prompt for partial cell ranges (inserting waiting cut/copied cells when clipboard is active; explaining refusals on multi-area, table, protected, or array conflicts).
  - Ctrl+9 / Ctrl+0 hide selected rows/columns and Ctrl+Shift+9 / Ctrl+Shift+0 unhide them, with clear hidden indicators/counts on headers; where Windows OS IME reserves Ctrl+Shift+0, expose the Unhide Columns command prominently rather than falsely promising OS interception.
  - Alt+Shift+Right / Alt+Shift+Left group and ungroup rows/columns, Ctrl+8 toggles outline symbols, and clicking outline level buttons and +/- controls expands/collapses cleanly (T2).
- **Conflicts:** None

### CL-04-22: Sheet Tabs, Sheet Management & Grouped Sheets

- **Summary:** Worksheet tabs support Ctrl+PageUp/PageDown (skipping hidden sheets and restoring per-sheet scroll/selection), tab scroll arrows with right-click all-sheets picker, double-click rename, drag reorder, Ctrl+drag copy, Shift+F11 insert, full Excel tab context menu, and multi-sheet grouping (Ctrl/Shift+click or Ctrl+Shift+PageUp/PageDown) with a prominent amber Grouped Sheets indicator in the Strip and on every grouped tab.
- **Cards (12):** `P19-CARBON-0139`, `P19-FABLE-0310`, `P19-FABLE-0322`, `P19-FABLE-0335`, `P19-FABLE-0375`, `P19-FABLE-0401`, `P19-GPT-0124`, `P19-GPT-0125`, `P19-GPT-0180`, `P19-GPT-0181`, `P19-GPT-0221`, `P19-NEON-0135`
- **Sources:** P19-CARBON (1), P19-FABLE (5), P19-GPT (5), P19-NEON (1)
- **Disposition:** `core` — Strong unanimity; highlight D-8 (prominent amber grouped-sheet indicator on Strip and tabs) as a key silent-accident safeguard.
- **`same-as` groups (3):** `P19-CARBON-0139` = `P19-FABLE-0310` = `P19-GPT-0124` = `P19-NEON-0135`; `P19-FABLE-0375` = `P19-GPT-0180` = `P19-GPT-0181`; `P19-FABLE-0335` = `P19-FABLE-0401` = `P19-GPT-0125` = `P19-GPT-0221`
- **Agreements:**
  - Ctrl+PageUp / Ctrl+PageDown switch previous/next visible worksheet (skipping hidden sheets) and restore each sheet's saved active cell and viewport scroll.
  - Tab scroll arrows scroll long tab strips, Ctrl+click jumps to first/last tab, and right-clicking the arrows lists all sheets for direct jump.
  - Double-clicking a tab renames inline; dragging reorders with a target marker; Ctrl+drag copies; Shift+F11 inserts a new worksheet (cleanly Undoable); right-clicking a tab offers Insert, Delete, Rename, Move or Copy, View Code, Protect Sheet, Tab Color, Hide, Unhide, and Select All Sheets.
  - Ctrl+click, Shift+click, or Ctrl+Shift+PageUp/PageDown group sheets; clicking an ungrouped tab or choosing Ungroup ends grouping. Grouped state displays a prominent amber 'Grouped sheets: N sheets' badge in the Strip AND on every grouped tab (deliberate improvement D-8 over Excel's subtle title-bar label), shows affected sheets in edit feedback, and undoes grouped edits atomically.
- **Conflicts:** None

### CL-04-23: Panes, Freeze/Split, Sheet Views & Semantic Zoom

- **Summary:** Classic supports Excel's Freeze Panes, draggable/double-click-removable Split Panes, wheel/Shift+wheel scrolling, Normal/Page Layout/Page Break Preview sheet views (Alt, W, L/P/I), and Excel's full 10%-400% worksheet zoom kept strictly separate from shell/UI zoom, switching cleanly to legible block/range-name labels below 40%.
- **Cards (8):** `P19-CARBON-0171`, `P19-FABLE-0320`, `P19-FABLE-0323`, `P19-FABLE-0382`, `P19-FABLE-0402`, `P19-GPT-0135`, `P19-GPT-0196`, `P19-GPT-0226`
- **Sources:** P19-CARBON (1), P19-FABLE (4), P19-GPT (3)
- **Disposition:** `core-with-decision` — Record the 10%-400% zoom range, UI-zoom separation, and <40% block-label LOD in Chapter IV and Part 12 D-9.
- **Decisions:** [D-48](decisions.md#d-48)
- **`same-as` groups (2):** `P19-FABLE-0323` = `P19-GPT-0196`; `P19-FABLE-0320` = `P19-GPT-0135` = `P19-GPT-0226`
- **Cross-cluster `same-as`:** `P19-FABLE-0402` = `P19-FABLE-0166` (CL-11-07)
- **Agreements:**
  - Freeze Panes freezes rows above and columns left of the active cell (Ctrl+Home targets top-left unfrozen cell); Split creates independent scroll panes with draggable split bars (double-click removes).
  - Mouse wheel scrolls vertically (~3 rows/notch), Shift+wheel or tilt-wheel scrolls horizontally, and middle-click pan works cleanly.
  - Normal, Page Layout, and Page Break Preview (Alt, W, L / P / I) are supported with draggable page breaks in Page Break Preview (T2).
  - Worksheet zoom (Ctrl+wheel / zoom control) scales sheet geometry about focus without changing active cell and is labeled and controlled separately from shell/UI/browser page zoom (tested up to 200%/400% with frozen panes and active editing).
- **Conflicts:**
  - **Worksheet zoom bounds & semantic LOD threshold (25%-250% / 70% vs Excel's 10%-400% / 40%)** (`P19-CARBON-0171` vs `P19-FABLE-0320`, `P19-FABLE-0402`, `P19-GPT-0226`) → Resolved by **[D-48](decisions.md#d-48)**

### CL-04-24: Cell Formatting, Number Formats, Format Cells & Conditional Formatting

- **Summary:** Classic preserves all Excel formatting shortcuts — Ctrl+1 Format Cells (all six Excel tabs with live preview), Ctrl+B/I/U/5 font styles, Ctrl+Shift+~/$ /% /# /! /^ /@ instant number formats, and Ctrl+Shift+& / Ctrl+Shift+_ outline borders — plus a full Conditional Formatting Manage Rules dialog backed by shared Number Format (C-17) and CF evaluation (C-18) engine services.
- **Cards (13):** `P19-CARBON-0158`, `P19-FABLE-0389`, `P19-FABLE-0390`, `P19-GPT-0192`, `P19-GPT-0193`, `P19-GPT-0194`, `P19-GPT-0195`, `P19-GPT-0224`, `P19-NEON-0179`, `P19-NEON-0180`, `P19-NEON-0181`, `P19-NEON-0255`, `P19-NEON-0256`
- **Sources:** P19-CARBON (1), P19-FABLE (2), P19-GPT (5), P19-NEON (5)
- **Disposition:** `core` — Full consensus; keep Ctrl+1 strictly protected (cross-ref CL-04-05) and include C-17 / C-18 in system needs.
- **`same-as` groups (4):** `P19-FABLE-0390` = `P19-GPT-0192` = `P19-NEON-0179`; `P19-GPT-0193` = `P19-NEON-0181`; `P19-CARBON-0158` = `P19-FABLE-0389` = `P19-GPT-0194` = `P19-NEON-0180`; `P19-FABLE-0389` = `P19-GPT-0195`
- **Agreements:**
  - Ctrl+1 opens Format Cells with Excel's six tabs (Number with custom codes, Alignment, Font, Border, Fill, Protection), Ctrl+Tab tab-switching, and live preview using actual selected cell values (separate from view/skin appearance settings).
  - Ctrl+B / I / U / 5 toggle Bold, Italic, Underline, and Strikethrough on selected cells or selected text during editing.
  - Ctrl+Shift+~ (General), ! (Number #,##0.00), @ (Time), # (Date), $ (Currency), % (Percent), ^ (Scientific) apply locale-accurate formats immediately without altering underlying values; Ctrl+Shift+& applies an outline border and Ctrl+Shift+_ removes borders (distinguished clearly from selection/dependency chrome).
  - Conditional Formatting > Manage Rules exposes scope, rule order, formula anchor, Applies To range, format, and Stop If True with live preview on actual selected values and single-Undo Apply; workbook rule colours survive theme changes.
  - Shared engine services power Excel number-format code rendering (C-17) and CF rule evaluation / Stop-If-True resolution (C-18) for both Classic and Formula Explorer.
- **Conflicts:** None

### CL-04-25: Excel Tables (ListObject), AutoFilter, Sorting & PivotTables

- **Summary:** Classic supports native Excel Tables (Ctrl+T / Ctrl+L, Tab auto-extension, calculated-column auto-fill with two-step Undo), AutoFilter (Ctrl+Shift+L, Alt+Down keyboard-navigable filter/sort menus, Ctrl+Alt+L reapply), and PivotTables (Insert PivotTable with field controls, Alt+F5 / Ctrl+Alt+F5 refresh distinct from F9, drill-through, and expand/collapse).
- **Cards (12):** `P19-FABLE-0270`, `P19-FABLE-0378`, `P19-GPT-0183`, `P19-GPT-0184`, `P19-GPT-0185`, `P19-GPT-0186`, `P19-GPT-0187`, `P19-GPT-0188`, `P19-GPT-0197`, `P19-GPT-0198`, `P19-GPT-0199`, `P19-NEON-0187`
- **Sources:** P19-FABLE (2), P19-GPT (9), P19-NEON (1)
- **Disposition:** `core` — Full consensus across sources; keep PivotTable authoring/drill-through clearly tiered (basic display/refresh T1, full drill-through/authoring T2).
- **`same-as` groups (3):** `P19-GPT-0183` = `P19-NEON-0187`; `P19-FABLE-0270` = `P19-GPT-0184` = `P19-GPT-0185`; `P19-FABLE-0378` = `P19-GPT-0186` = `P19-GPT-0187`
- **Agreements:**
  - Ctrl+T / Ctrl+L create an Excel Table (ListObject) from a confirmed extent and header choice (never nesting tables); typing below/beside or Tabbing from the last data cell extends the table; editing a calculated column auto-fills the column with structured references and reveals exception rows (first Ctrl+Z undoes the auto-fill, second undoes the entry).
  - Ctrl+Shift+L toggles AutoFilter buttons; Alt+Down on a filter header opens a full keyboard-navigable filter/sort menu (search, value list, number/date/text filters, exact visible row count); sorting moves full records unless user explicitly chooses partial range; Ctrl+Alt+L reapplies sort/filter cleanly.
  - Insert PivotTable confirms source, destination, and aggregation and presents existing PivotTables with field controls rather than ordinary editable cells; Alt+F5 refreshes selected Pivot/connection and Ctrl+Alt+F5 refreshes all (showing last refresh time and retained-cache state if credentials/files are missing; F9 stays calculation only); drill-through creates a real detail sheet (refusing unavailable external details honestly).
- **Conflicts:** None

### CL-04-26: Data Validation & Sheet Protection

- **Summary:** Classic enforces Excel Data Validation rules — honouring Stop, Warning, and Information alert styles with each rule's own title/message, Alt+Down validation dropdown lists, observed Excel paste/fill behavior, and a shared validation query service (C-19) — and honours Sheet Protection by refusing locked-cell edits/partial pastes while letting Tab cycle exclusively through unlocked cells.
- **Cards (6):** `P19-FABLE-0269`, `P19-FABLE-0273`, `P19-GPT-0182`, `P19-GPT-0214`, `P19-GPT-0225`, `P19-NEON-0257`
- **Sources:** P19-FABLE (2), P19-GPT (3), P19-NEON (1)
- **Disposition:** `core` — Unanimous across sources; include C-19 in system needs and anchor validation refusals inline at the cell while preserving Stop's strict commit block.
- **`same-as` groups (2):** `P19-FABLE-0269` = `P19-GPT-0225`; `P19-FABLE-0273` = `P19-GPT-0182`
- **Agreements:**
  - Invalid direct entry honours the saved Data Validation alert style: Stop refuses commit (retry/cancel), Warning offers explicit accept/revise/cancel, Information offers accept/cancel — using the rule's own title/message and retaining the draft, anchored cleanly at the cell (D-6).
  - Paste and fill over validated cells follow observed Microsoft 365 Excel behavior rather than inventing an un-Excel universal rejection.
  - Alt+Down on a list-validated cell opens its permitted-choice dropdown (arrows choose, Enter commits, Esc restores); inside a formula editor, Alt+Down toggles autocomplete instead.
  - On protected sheets, editing or pasting over a locked target preserves selection, explains which locked cell blocked the edit, and refuses partial rectangular pastes; Tab moves exclusively among unlocked cells.
  - Shared Data Validation inspection/check engine service (C-19) serves Classic, Notebook, and App consistently.
- **Conflicts:** None

### CL-04-27: Notes, Threaded Comments & Hyperlinks

- **Summary:** Classic distinguishes Excel Notes (Shift+F2, hover preview on teal-folded cell) from threaded Comments (Ctrl+Shift+F2, never silently converting plain notes into conversations), and preserves Ctrl+K for Insert/Edit Hyperlink with click-to-follow, press-and-hold-to-select, and safe external navigation that never executes workbook macros.
- **Cards (5):** `P19-CARBON-0145`, `P19-FABLE-0319`, `P19-FABLE-0377`, `P19-GPT-0203`, `P19-GPT-0204`
- **Sources:** P19-CARBON (1), P19-FABLE (2), P19-GPT (2)
- **Disposition:** `core-with-decision` — Full consensus; reinforce that Ctrl+K belongs strictly to Hyperlinks in Classic (cross-ref CL-04-05) and notes use a teal fold (cross-ref CL-04-06).
- **Decisions:** [D-34](decisions.md#d-34)
- **`same-as` groups (2):** `P19-CARBON-0145` = `P19-FABLE-0377` = `P19-GPT-0203`; `P19-FABLE-0319` = `P19-GPT-0204`
- **Agreements:**
  - Shift+F2 adds or edits a cell Note (Esc closes and returns focus to cell; hovering a marked cell previews the note); Ctrl+Shift+F2 starts/edits a threaded Comment without auto-converting plain notes into threads.
  - Ctrl+K inserts or edits a Hyperlink preserving cell content; clicking a hyperlink follows it safely (never running a workbook macro on external navigation), while pressing and holding selects the cell without following (T2).
- **Conflicts:**
  - *Cross-cluster `conflicts-with` links (resolved by D-34):* `P19-GPT-0204` vs `P19-CARBON-0030` (CL-09-11), `P19-NEON-0039` (CL-09-05)

### CL-04-28: Floating Objects, Charts & Form Controls

- **Summary:** Classic provides full keyboard and mouse parity for floating shapes, charts, and OOXML form controls: distinct object selection (Ctrl+Alt+5 then Tab, Esc back to grid), drag/resize/nudge/snap/duplicate/hide (Ctrl+6), chart creation (Alt+F1 embedded, F11 chart sheet) and element navigation, and cell-bound Form Controls (click/keyboard operates, Ctrl+click / Right-click / Selection panel selects for editing, ghost value while dragging with commit on release).
- **Cards (11):** `P19-FABLE-0337`, `P19-FABLE-0370`, `P19-FABLE-0379`, `P19-FABLE-0380`, `P19-FABLE-0381`, `P19-FABLE-0403`, `P19-GPT-0200`, `P19-GPT-0201`, `P19-GPT-0202`, `P19-GPT-0218`, `P19-GPT-0222`
- **Sources:** P19-FABLE (6), P19-GPT (5)
- **Disposition:** `core-with-decision` — Note explicitly that Ctrl+Alt+5 (P19-GPT-0218) stays bound to floating-object navigation in Classic per DECISIONS_20261126 #12 collision check (cross-ref CL-04-01).
- **Decisions:** [D-12](decisions.md#d-12), [D-49](decisions.md#d-49)
- **`same-as` groups (3):** `P19-FABLE-0379` = `P19-GPT-0201`; `P19-FABLE-0381` = `P19-GPT-0200`; `P19-FABLE-0380` = `P19-GPT-0202`
- **Agreements:**
  - Floating objects (shapes, charts, controls) select separately from cells, cycle via Tab after entering object navigation (Ctrl+Alt+5 or clicking an object), and return to grid selection via Esc (shape text editing captures text keys first).
  - Dragging moves objects, handles resize (Shift locks aspect ratio, Alt snaps to cell gridlines), arrows nudge, Ctrl+drag duplicates, Copy/Paste work on objects, Delete removes, Ctrl+6 hides/shows all objects, and right-click offers Assign Macro and Format Control (T2).
  - Alt+F1 inserts an embedded chart and F11 creates a chart sheet from selected data (entering a concrete range-picking state if selection is empty/unsuitable); clicking a chart then an element selects sub-elements navigable by arrows (T2).
  - Form Controls operate on normal click and keyboard (arrows step small change, track/PgUp/PgDn step page change, checkbox writes TRUE/FALSE, list writes index), disclose executable macro actions, keep move/resize undo separate from linked-value undo, and show a live ghost value during thumb drag committing on release to prevent recalc storms (D-10).
- **Conflicts:**
  - **How to select a Form Control for editing without operating it (Ctrl+click vs Right-click / Selection panel)** (`P19-FABLE-0337` vs `P19-GPT-0222`) → Resolved by **[D-49](decisions.md#d-49)**
  - *Cross-cluster `conflicts-with` links (resolved by D-12, D-49):* `P19-GPT-0218` vs `P19-CARBON-0049` (CL-02-06), `P19-CARBON-0054` (CL-03-04), `P19-CARBON-0055` (CL-04-01), `P19-CARBON-0057` (CL-05-01), `P19-CARBON-0059` (CL-06-02), `P19-CARBON-0061` (CL-07-09) (+16 more)

### CL-04-29: Context Menus & Status Strip (Aggregates & Date System)

- **Summary:** Right-click and Shift+F10 / Menu key open compact, on-screen context menus anchored to the active cell, row/column heading, object, or sheet tab — preserving Excel's exact item ordering first with DNA Calc actions (Trace, Explore formula, Show in) cleanly separated below a rule. The Status Strip renders live selection aggregates (<8 ms, click-to-copy, right-click configurable) alongside mode words, calculation state, and the workbook's 1900/1904 date system.
- **Cards (7):** `P19-CARBON-0133`, `P19-FABLE-0336`, `P19-FABLE-0372`, `P19-FABLE-0376`, `P19-GPT-0114`, `P19-GPT-0223`, `P19-NEON-0129`
- **Sources:** P19-CARBON (1), P19-FABLE (3), P19-GPT (2), P19-NEON (1)
- **Disposition:** `core` — Complete alignment across all sources; include exact context-menu ordering rule in Chapter IV.
- **`same-as` groups (2):** `P19-CARBON-0133` = `P19-FABLE-0376` = `P19-GPT-0114` = `P19-NEON-0129`; `P19-FABLE-0336` = `P19-GPT-0223`
- **Agreements:**
  - Right-click, Shift+F10, and the Context Menu key open identical target-aware menus anchored neatly on-screen to the active cell, block, header, object, or tab; Esc returns focus to that exact target.
  - Cell context menu preserves Excel's familiar order from Cut through Link, placing DNA Calc's native actions (Trace, Explore formula, Show in) after a divider rule at the bottom; row/column heading menus expose Cut, Copy, Paste, Paste Special, Insert, Delete, Clear Contents, Format Cells, Row Height/Column Width, Hide, Unhide.
  - The Status Strip computes Sum, Average, Count, Numerical Count, Minimum, and Maximum across selections in <8 ms (right-click chooses visible aggregates; clicking any figure copies it to clipboard) and exposes accessible status details including edit/calc state and the workbook's 1900/1904 date system (editable only as an explicit previewed workbook setting, never inferred).
- **Conflicts:** None

### CL-04-30: Undo / Redo & Cross-View History Semantics

- **Summary:** Classic participates in DNA Calc's per-workbook, named, cross-view Undo/Redo stack (Ctrl+Z, Alt+Backspace, Ctrl+Y, plus multi-step history dropdown) that survives Save and view switches, gives active text editors first priority on Ctrl+Z, and keeps Ctrl+Y (Redo-then-Repeat) distinct from F4 (Repeat outside editing). Per DECISIONS_20261126 #4, Undo across macro runs is designed for architecturally but NOT promised in v1.
- **Cards (7):** `P19-FABLE-0393`, `P19-FABLE-0394`, `P19-GPT-0205`, `P19-GPT-0206`, `P19-GPT-0207`, `P19-NEON-0190`, `P21-0052`
- **Sources:** P19-FABLE (2), P19-GPT (3), P19-NEON (1), P21 (1)
- **Disposition:** `core-with-decision` — State the v1 Undo guarantee (per-workbook, named, cross-view, survives Save) and the exact Ruling #4 framing for macro runs in Chapter IV and Part 12 D-1.
- **Decisions:** [D-04](decisions.md#d-04)
- **`same-as` groups (1):** `P19-FABLE-0393` = `P19-GPT-0205` = `P19-NEON-0190` = `P21-0052`
- **Agreements:**
  - Ctrl+Z (and Alt+Backspace) undoes the last committed model or presentation transaction for the owning workbook across all views; uncommitted text edits consume editor Undo first; navigation and recalc notifications never pollute edit history.
  - Undo history is per-workbook (unlike Excel's global cross-workbook stack), names each step cleanly in a multi-step dropdown, survives Save, and is unaffected by switching views (which never commits an invalid draft).
  - Ctrl+Y redoes when a redo branch exists and otherwise repeats a repeatable command; F4 outside editing is strictly Repeat (never Redo), and toolbar labels name the exact action.
- **Conflicts:**
  - **Undo survival across macro runs (guaranteed vs architectural goal not promised in v1)** (`P19-FABLE-0394` vs `P19-GPT-0205`, `P19-GPT-0207`) → Resolved by **[D-04](decisions.md#d-04)**
  - *Cross-cluster `conflicts-with` links (resolved by D-04):* `P19-FABLE-0394` vs `FRAMING-0169` (CL-14-04), `P19-GPT-0038` (CL-02-10), `P19-GPT-0079` (CL-14-04), `P19-GPT-0296` (CL-14-04), `P19-GPT-0312` (CL-02-20); `P19-GPT-0205` vs `P19-CARBON-0120` (CL-14-04), `P19-CARBON-0128` (CL-16-04), `P19-CARBON-0206` (CL-14-04), `P19-CARBON-0223` (CL-02-20), `P19-CARBON-0241` (CL-10-16), `P19-FABLE-0132` (CL-02-10) (+3 more); `P19-GPT-0207` vs `P19-CARBON-0120` (CL-14-04), `P19-CARBON-0128` (CL-16-04), `P19-CARBON-0206` (CL-14-04), `P19-CARBON-0223` (CL-02-20), `P19-CARBON-0241` (CL-10-16), `P19-FABLE-0132` (CL-02-10) (+3 more)

### CL-04-31: Classic Printing & Page Setup Fidelity

- **Summary:** Classic printing honours Excel's PageSetup 100% faithfully — PrintArea, PrintTitles, manual and automatic page breaks, FitToPages scaling, margins, headers/footers, and page order — previewed via Ctrl+P and Ctrl+F2, disclosing calculation freshness/scope where material, with Provenance Overlay defaulting OFF on print (opt-in checkbox).
- **Cards (4):** `P19-CARBON-0094`, `P19-FABLE-0197`, `P19-GPT-0210`, `P19-NEON-0086`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core-with-decision` — Include in Chapter IV / Chapter VIII (Printing) with strict Excel PageSetup fidelity and overlay-OFF print default.
- **Decisions:** [D-41](decisions.md#d-41)
- **`same-as` groups (1):** `P19-CARBON-0094` = `P19-NEON-0086`
- **Agreements:**
  - Classic printing reproduces Excel's printed page faithfully: PrintArea, PrintTitles (repeating header rows/columns), manual and automatic page breaks, scaling (Fit to N pages), headers/footers, margins, page order, and author formatting.
  - Provenance Overlay / diagnostic pips default strictly OFF on print, with an explicit opt-in checkbox ('Include diagnostic/provenance overlay') when printing an audit copy.
  - Ctrl+P opens Print Preview with 'Worksheet print' as a direct choice and honest freshness/scenario/scope metadata where material.
- **Conflicts:**
  - **Ctrl+F2 Print Preview shortcut parity (Fable vs GPT)** (`P19-FABLE-0197` vs `P19-GPT-0210`) → Resolved by **[D-41](decisions.md#d-41)**

## V. Inspector

### CL-05-01: Inspector Role, Dockable Panels Everywhere & View Architecture

- **Summary:** Inspector is a Phyphox-inspired read-mostly diagnostic bench whose instruments dock as panels inside every view and enlarge seamlessly into the full-stage Inspector view without losing state. Navigation preserves active selection, provides keyboard traversal across probes, and jumps directly to backing cells in Classic.
- **Cards (13):** `FRAMING-0008`, `FRAMING-0066`, `P19-CARBON-0057`, `P19-CARBON-0058`, `P19-CARBON-0111`, `P19-FABLE-0068`, `P19-FABLE-0152`, `P19-FABLE-0218`, `P19-FABLE-0477`, `P19-GPT-0041`, `P19-NEON-0050`, `P19-NEON-0051`, `P21-0063`
- **Sources:** FRAMING (2), P19-CARBON (3), P19-FABLE (4), P19-GPT (1), P19-NEON (2), P21 (1)
- **Disposition:** `core-with-decision` — Design Book opens Inspector chapter with the 5-tab model (Overview, Precedents, Dependents, Calculation, Live), dockable-everywhere rule, `Ctrl+Alt+2` view shortcut, and keyboard map.
- **Decisions:** [D-12](decisions.md#d-12), [D-50](decisions.md#d-50)
- **`same-as` groups (3):** `FRAMING-0066` = `P19-FABLE-0068` = `P19-FABLE-0218` = `P19-FABLE-0477`; `P19-GPT-0041` = `P21-0063`; `P19-CARBON-0058` = `P19-NEON-0051`
- **Agreements:**
  - Inspector is read-mostly and inspired by Phyphox: systematic diagnostic instruments over precedents/dependents, calculation state, errors, volatiles, names, external links, and live RTD feeds (FRAMING-0008).
  - Diagnostic instruments must be usable beside the data inside every view rather than forcing users to leave their active sheet or notebook (FRAMING-0066, P19-FABLE-0218, P19-CARBON-0111).
  - Switching between a docked instrument panel and full-stage Inspector preserves the active probe, selected node, and scroll/cell return path (P19-FABLE-0068, P19-FABLE-0477, P21-0063).
  - Full keyboard traversal (arrow keys walk hierarchy/siblings, Enter reveals source in Classic, Back restores trace) and empty-selection prompts prevent dead ends (P19-GPT-0041, P19-FABLE-0152, P19-NEON-0050).
- **Conflicts:**
  - **In-view diagnostic surface model: dockable instrument panels vs toggleable X-Ray overlay** (`FRAMING-0066`, `P19-FABLE-0068`, `P19-FABLE-0218`, `P19-FABLE-0477` vs `P19-CARBON-0111`) → Resolved by **[D-50](decisions.md#d-50)**
  - **Inspector top-level tab taxonomy: 4 focused cell probes vs 7 instruments vs multi-pane dashboard** (`P19-GPT-0041`, `P21-0063` vs `P19-CARBON-0058`, `P19-NEON-0051`) → Resolved by **[D-50](decisions.md#d-50)**
  - **Keyboard shortcut to switch to Inspector view** (`P19-NEON-0050` vs `P19-CARBON-0057`, `P19-FABLE-0068`, `P19-FABLE-0152`) → Resolved by **[D-12](decisions.md#d-12)**
  - *Cross-cluster `conflicts-with` links (resolved by D-12, D-50):* `P19-CARBON-0057` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28); `P19-FABLE-0068` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28); `P19-FABLE-0152` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28)

### CL-05-02: Workbook-Wide Triage, Sheet Atlas & Structural Inventory

- **Summary:** Whole-workbook inspection starts with bounded structural counts and block-level triage rather than expanding hundreds of thousands of cells, disclosing hidden and veryHidden sheets, external links, VBA modules, dead backup sheets, and volatile hotspots safely without bypassing workbook protection.
- **Cards (5):** `P19-CARBON-0028`, `P19-CARBON-0188`, `P19-GPT-0014`, `P19-NEON-0026`, `P19-NEON-0205`
- **Sources:** P19-CARBON (2), P19-GPT (1), P19-NEON (2)
- **Disposition:** `core-with-decision` — Specify Inspector Overview's count-first structural inventory, hidden/veryHidden disclosure rules, reversible unhide contract, and bounded sheet-tier Atlas sub-mode.
- **Decisions:** [D-51](decisions.md#d-51)
- **`same-as` groups (2):** `P19-CARBON-0028` = `P19-CARBON-0188`; `P19-NEON-0026` = `P19-NEON-0205`
- **Agreements:**
  - All `xlSheetHidden` and `xlSheetVeryHidden` worksheets are explicitly disclosed and tagged in Inspector inventory without bypassing Excel workbook protection (P19-CARBON-0028, P19-CARBON-0188, P19-GPT-0014, P19-NEON-0026, P19-NEON-0205).
  - Large workbooks (40+ sheets, 250k-300k formulas) are summarised at sheet and block granularity; cell-level graph expansion is refused with bounded sheet/block alternatives (P19-CARBON-0028, P19-GPT-0014, P19-NEON-0026).
  - Unhiding a normally hidden sheet is explicitly declared as a reversible model change (`Ctrl+Z`), while protected/veryHidden restrictions are disclosed honestly (P19-GPT-0014).
  - External links (with cached timestamps/status), VBA modules, dead zero-dependent sheets, and volatile `OFFSET`/`INDIRECT` fan-out counts appear in one structural inventory (P19-CARBON-0028, P19-CARBON-0188, P19-NEON-0026).
- **Conflicts:**
  - **Whole-workbook entry view: count-first structured triage vs graphical L0 Sheet Atlas** (`P19-GPT-0014`, `P19-CARBON-0028` vs `P19-NEON-0026`, `P19-NEON-0205`) → Resolved by **[D-51](decisions.md#d-51)**

### CL-05-03: Root-Cause Error Attribution, Folding & Broken-Name Isolation

- **Summary:** All `#` error cells across a workbook fold deterministically into their upstream root-origin cells with exact downstream blast counts and shortest propagation paths. Local formula/name errors (such as `#REF!` inside a defined name) are isolated strictly from unrelated missing external links and stale cached values, with zero guessed repairs.
- **Cards (9):** `P19-CARBON-0186`, `P19-CARBON-0211`, `P19-FABLE-0045`, `P19-FABLE-0133`, `P19-FABLE-0540`, `P19-GPT-0013`, `P19-NEON-0206`, `P19-NEON-0251`, `P21-0056`
- **Sources:** P19-CARBON (2), P19-FABLE (3), P19-GPT (1), P19-NEON (2), P21 (1)
- **Disposition:** `core` — Unanimous four-answer + Pro 21 consensus. Document root-error folding algorithm contract, cross-view red-tick inheritance, and strict separation of local `#REF!` from missing external links and cached values.
- **`same-as` groups (2):** `P19-CARBON-0186` = `P19-CARBON-0211` = `P19-FABLE-0045` = `P19-FABLE-0540` = `P19-NEON-0206` = `P19-NEON-0251`; `P19-GPT-0013` = `P21-0056`
- **Agreements:**
  - Every `#` error cell is classified as either a Root Origin (producing an error from non-error inputs) or Propagated from specific root cell(s), collapsing thousands of cascading errors into a small actionable root list with exact downstream counts (P19-CARBON-0186, P19-CARBON-0211, P19-FABLE-0045, P19-NEON-0206, P19-NEON-0251).
  - Error indicators use a consistent red left tick on cells and roll up with counts onto containing boxes, notebook items, App tiles, and sheet tabs (P19-FABLE-0133).
  - Local `#REF!` errors inside broken defined names (e.g. `Revenue.Window =OFFSET(Revenue!#REF!,...)`) are traced and shown separately from missing external workbooks and saved cached values (P19-GPT-0013, P21-0056).
  - DNA Calc never substitutes zero for missing external data, never displays a stale file cache as the current formula result, and never guesses a replacement reference for `#REF!` (P19-GPT-0013, P21-0056).
- **Conflicts:** None

### CL-05-04: Precedents/Dependents Graph, Dynamic References & Formula Structure Queries

- **Summary:** Inspector queries direct and transitive precedents and dependents at cell, range, defined-name, block, and sheet granularity—preserving name-to-name edges, structured table references, and runtime-resolved dynamic references—while supporting formula AST/span inspection, relative formula-run tracking, step-by-step sub-expression evaluation, and Excel's `Ctrl+[` workflow.
- **Cards (12):** `P19-CARBON-0209`, `P19-FABLE-0064`, `P19-FABLE-0532`, `P19-FABLE-0533`, `P19-FABLE-0534`, `P19-FABLE-0535`, `P19-FABLE-0536`, `P19-FABLE-0537`, `P19-FABLE-0538`, `P19-GPT-0306`, `P19-GPT-0307`, `P19-NEON-0249`
- **Sources:** P19-CARBON (1), P19-FABLE (8), P19-GPT (2), P19-NEON (1)
- **Disposition:** `core` — Forms the Skin/Engine dependency query contract (Appendix C synthesis) and Precedents/Dependents tab specification.
- **`same-as` groups (2):** `P19-CARBON-0209` = `P19-FABLE-0532` = `P19-FABLE-0533` = `P19-FABLE-0534` = `P19-GPT-0306` = `P19-NEON-0249`; `P19-FABLE-0535` = `P19-GPT-0307`
- **Cross-cluster `same-as`:** `P19-FABLE-0536` = `FRAMING-0152` (CL-06-09)
- **Agreements:**
  - Precedent and dependent queries must preserve defined-name edges (name-to-name and name-to-cell) and structured table references rather than flattening everything to raw A1 coordinates (P19-CARBON-0209, P19-FABLE-0534, P19-NEON-0249).
  - Dynamic, conditional, and unresolved references (`OFFSET`, `INDIRECT`, `INDEX`, `XLOOKUP`, `FILTER`, spills) are resolved from the latest calculation and explicitly marked as dynamic/runtime-selected (P19-CARBON-0209, P19-FABLE-0533, P19-GPT-0306, P19-NEON-0249).
  - Block- and sheet-level summaries must allow drilling into exact membership and exceptions behind a relative formula run (P19-FABLE-0535, P19-GPT-0307).
  - Formula AST spans, exact Excel text round-tripping, safe global name rename, and step-by-step sub-expression evaluation underpin Trace and Formula Explorer (P19-FABLE-0536, P19-FABLE-0537, P19-FABLE-0538).
  - Excel's `Ctrl+[` keeps its exact jump-to-precedents behaviour in Classic, with `Alt+←` restoring selection and a gentle once-per-session hint introducing `Alt+Shift+T` in-place Trace (P19-FABLE-0064).
- **Conflicts:** None

### CL-05-05: Conditional-Row Contribution Trace, Near-Miss Ladder & Non-Mutating View Filter

- **Summary:** For row-testing functions (`SUMIFS`, `COUNTIFS`, `AVERAGEIFS`, `FILTER`, lookups), Trace evaluates 100% of rows (never silent samples) to render a criteria ladder showing remaining and fail-only-this counts, isolates outlier rows with both Table data row and worksheet row addresses, lifts number-format disguises, and opens a non-mutating View filter in the source table without altering Excel's AutoFilter.
- **Cards (14):** `P19-CARBON-0226`, `P19-FABLE-0065`, `P19-FABLE-0066`, `P19-FABLE-0072`, `P19-FABLE-0073`, `P19-FABLE-0473`, `P19-FABLE-0474`, `P19-FABLE-0475`, `P19-FABLE-0539`, `P19-GPT-0020`, `P19-GPT-0254`, `P19-GPT-0322`, `P19-NEON-0270`, `P21-0053`
- **Sources:** P19-CARBON (1), P19-FABLE (8), P19-GPT (3), P19-NEON (1), P21 (1)
- **Disposition:** `core` — Signature Inspector capability (Sales reconciliation hero story). Combine Fable's criteria ladder & non-mutating view filter with GPT/P21's full-scope 25k-row completeness and dual Table/worksheet row addressing.
- **`same-as` groups (3):** `P19-FABLE-0065` = `P19-FABLE-0473` = `P19-FABLE-0539`; `P19-FABLE-0066` = `P19-FABLE-0474` = `P19-NEON-0270`; `P19-GPT-0020` = `P19-GPT-0254` = `P19-GPT-0322` = `P21-0053`
- **Agreements:**
  - Contribution traces cover 100% of the target scope (e.g., all 25,000 Orders rows) or explicitly disclose incompleteness; large tables are shown in paged/virtual row windows rather than exploding into 25,000 graph nodes (P19-GPT-0020, P19-GPT-0254, P19-GPT-0322, P21-0053).
  - Row-testing functions render a criteria ladder (<200 ms on cast workbooks) showing rows remaining after each test and rows that 'fail only this' test, alongside near-miss diagnostics for hidden time-of-day decimals, trailing whitespace, or text-stored numbers (P19-FABLE-0065, P19-FABLE-0473, P19-FABLE-0539, P19-CARBON-0226).
  - Drilling into ladder/outlier rows opens a labelled non-mutating 'View filter from Trace' in the source table leaving Excel's AutoFilter and `SUBTOTAL` untouched, displaying both Table data row and worksheet row addresses plus raw stored values beside formatted display, clearing cleanly on `Esc` (P19-FABLE-0066, P19-FABLE-0474, P19-NEON-0270, P21-0053).
  - Diagnostic hints carry an explicit confidence grade, frame semantic mismatches as suggestions to confirm (never inferring business rules from prose), and never mutate workbook data (P19-FABLE-0475, P19-GPT-0020).
  - The conditional ladder is exposed to screen readers as an accessible tree announcing test index, criterion, remaining count, and fail-only-this count (P19-FABLE-0072).
- **Conflicts:** None

### CL-05-06: Dual-Target Discrepancy Comparison, Strip Delta & Structural Anomaly Probes

- **Summary:** Selecting two numeric cells immediately shows their difference in the status strip and launches Inspector's Dual-Target Discrepancy Waterfall to reconcile 100% of silent gaps (clipped ranges, dirty criteria keys). Alongside it, Structural Anomaly Probes flag valid-Excel modeling hazards—partial table coverage, hardcoded overrides inside formula runs, numbers stored as text, and volatile fan-out—as non-red typed warnings.
- **Cards (8):** `P19-CARBON-0034`, `P19-CARBON-0187`, `P19-FABLE-0063`, `P19-FABLE-0136`, `P19-FABLE-0478`, `P19-NEON-0031`, `P19-NEON-0207`, `P19-NEON-0253`
- **Sources:** P19-CARBON (2), P19-FABLE (3), P19-NEON (3)
- **Disposition:** `core-with-decision` — Include Strip two-cell Difference, Dual-Target Discrepancy Waterfall, and the canonical catalogue of achromatic `△` structural anomaly probes (per D-17).
- **Decisions:** [D-17](decisions.md#d-17), [D-23](decisions.md#d-23)
- **`same-as` groups (3):** `P19-CARBON-0034` = `P19-CARBON-0187`; `P19-FABLE-0063` = `P19-FABLE-0478`; `P19-NEON-0031` = `P19-NEON-0207` = `P19-NEON-0253`
- **Agreements:**
  - Selecting exactly two numeric cells (`Ctrl+Click`) automatically shows their Difference in the status strip alongside Sum and Count, with a direct link to open discrepancy comparison in Inspector (P19-CARBON-0034, P19-FABLE-0063, P19-FABLE-0478).
  - Discrepancy comparison accounts quantitatively for structural coverage gaps (e.g. truncated `SUMIFS` range vs full Excel Table) and dirty criteria mismatches (trailing whitespace, text-stored numbers) until residual is explained (P19-CARBON-0034, P19-CARBON-0187, P19-NEON-0031).
  - Structural Anomaly Probes surface valid-Excel modeling hazards—partial table coverage, hardcoded overrides inside uniform formula runs, numbers stored as text, stale pivots, missing external links, and volatile `OFFSET`/`INDIRECT` fan-out—distinctly from `#` errors (P19-FABLE-0136, P19-NEON-0207, P19-NEON-0253).
- **Conflicts:**
  - **Visual colour grammar for structural anomalies and typed warnings** (`P19-FABLE-0136` vs `P19-CARBON-0034`, `P19-NEON-0031`) → Resolved by **[D-17](decisions.md#d-17)**
  - *Cross-cluster `conflicts-with` links (resolved by D-17, D-23):* `P19-NEON-0031` vs `FRAMING-0063` (CL-02-04), `P19-GPT-0090` (CL-06-09)

### CL-05-07: Speculative Repair Previews, Sibling-Run Apply & Spill/Cycle Pre-Commit Checks

- **Summary:** Before mutating any cell, Inspector evaluates proposed repairs speculatively to preview exact before/after formula text, result delta, changed-formula count, and any loop closure, spill obstruction, or validation break. Accepted edits commit as one atomic Undo step while Cancel discards only the pending preview.
- **Cards (5):** `P19-FABLE-0541`, `P19-FABLE-0542`, `P19-FABLE-0544`, `P19-NEON-0032`, `P21-0055`
- **Sources:** P19-FABLE (3), P19-NEON (1), P21 (1)
- **Disposition:** `core-with-decision` — Define the standard Inspector Preview Seam contract (speculative diff, pre-commit spill/loop/validation checks, single-cell default with opt-in run-batch toggle, exact Cancel/Apply/Close/Undo lifecycle).
- **Decisions:** [D-52](decisions.md#d-52)
- **`same-as` groups (1):** `P19-FABLE-0541` = `P21-0055`
- **Cross-cluster `same-as`:** `P19-FABLE-0544` = `P21-0032` (CL-06-08)
- **Agreements:**
  - Engine supports speculative evaluation of pending formula edits without committing them, reporting exact value deltas and pre-commit hazards (new circular loop, blocked spill, broken validation) (P19-FABLE-0541, P19-FABLE-0542, P19-FABLE-0544).
  - Proposed repairs display exact before/after Excel formula text, resulting value delta, and changed-formula count visually separated from the current committed result until explicit Apply (P21-0055).
  - Cancel discards only an uncommitted preview, Close retains an accepted change, and Undo restores the exact prior formula state in one step (P21-0055, P19-NEON-0032).
- **Conflicts:**
  - **Repair preview scope: strict single-cell fix vs sibling-run batch fix** (`P21-0055` vs `P19-NEON-0032`) → Resolved by **[D-52](decisions.md#d-52)**

### CL-05-08: Calculation Order Honesty: Dependency Explanation vs Observed Execution

- **Summary:** Inspector strictly separates static dependency explanations (topological waves and cycles), post-recalc value deltas (changed cells before→after), and optional observed execution order. When execution order is recorded, it is strictly labelled 'DNA Calc's calculation order' (preserving concurrent waves) and never presented as Excel's internal calculation chain.
- **Cards (11):** `FRAMING-0135`, `P19-CARBON-0210`, `P19-FABLE-0145`, `P19-FABLE-0476`, `P19-GPT-0089`, `P19-GPT-0097`, `P19-GPT-0240`, `P19-GPT-0319`, `P19-NEON-0208`, `P19-NEON-0243`, `P19-NEON-0250`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-FABLE (2), P19-GPT (4), P19-NEON (3)
- **Disposition:** `core-with-decision` — Make the 3-way distinction (Dependency explanation / Value delta / Optional 'DNA Calc's calculation order') a headline honesty invariant in the Inspector chapter.
- **Decisions:** [D-53](decisions.md#d-53)
- **`same-as` groups (3):** `P19-GPT-0089` = `P19-GPT-0097` = `P19-GPT-0240` = `P19-GPT-0319`; `P19-FABLE-0145` = `P19-FABLE-0476`; `P19-CARBON-0210` = `P19-NEON-0208` = `P19-NEON-0250`
- **Agreements:**
  - After every recalculation, the engine reports exact changed cells and defined names with previous and new values plus trigger provenance (P19-CARBON-0210, P19-NEON-0243, P19-GPT-0240).
  - Recording recalculation traces writes nothing into the workbook file (P19-FABLE-0145, P19-FABLE-0476).
  - Strongly connected components (circular cycles and self-loops) are explicitly identified in dependency analysis (P19-NEON-0250, P19-FABLE-0145).
- **Conflicts:**
  - **Presenting calculation order: serial evaluation log vs topological dependency waves vs strict separation with 'DNA Calc's calculation order'** (`FRAMING-0135`, `P19-GPT-0089`, `P19-GPT-0097`, `P19-GPT-0240`, `P19-GPT-0319` vs `P19-FABLE-0145`, `P19-FABLE-0476`, `P19-CARBON-0210`, `P19-NEON-0208`, `P19-NEON-0250`) → Resolved by **[D-53](decisions.md#d-53)**

### CL-05-09: Recalculation Controls, Timing Profiles, Iterative Loops & Convergence Telemetry

- **Summary:** Inspector exposes Excel's exact calculation mode, scoped recalculation shortcuts (`F9` through `Ctrl+Alt+Shift+F9`), interruptible long-recalc feedback with saved-cache drift counts, per-block timing hotspots, and full iterative-calculation telemetry (cycle members, pass-read markers, pass count, convergence vs iteration-cap stop reason, and residual delta).
- **Cards (7):** `P19-FABLE-0050`, `P19-FABLE-0543`, `P19-FABLE-0545`, `P19-FABLE-0546`, `P19-GPT-0318`, `P19-NEON-0245`, `P19-NEON-0246`
- **Sources:** P19-FABLE (4), P19-GPT (1), P19-NEON (2)
- **Disposition:** `core` — Full consensus on Calculation tab controls, interruptible F9 progress UX, per-block profiling, and iterative convergence diagnostics.
- **`same-as` groups (1):** `P19-FABLE-0543` = `P19-FABLE-0546` = `P19-GPT-0318` = `P19-NEON-0245`
- **Agreements:**
  - All four standard Excel recalculation scopes are supported and exposed (`F9` dirty all, `Shift+F9` active sheet, `Ctrl+Alt+F9` full, `Ctrl+Alt+Shift+F9` rebuild dependency graph + full) (P19-NEON-0246).
  - Long manual recalculations keep the grid interactive with hatched pending cells, an amber Strip progress indicator with `Esc stops`, and a completion report showing duration, count of values differing from saved file cache, and external links using saved values (P19-FABLE-0050).
  - Calculation telemetry exposes mode (`Automatic`, `Automatic Except Data Tables`, `Manual`), dirty state, last recalc duration, per-block timings, iterative settings (`Iterative`, `MaxIterations`, `MaxChange`), cycle members, pass-read provenance (this pass vs prior pass), and honest convergence vs cap-hit stop reason with final residual delta (P19-FABLE-0543, P19-FABLE-0545, P19-FABLE-0546, P19-GPT-0318, P19-NEON-0245).
- **Conflicts:** None

### CL-05-10: Phyphox Live Instruments, Observation Recording, Side-Effect-Free Replay & RTD Coalescing

- **Summary:** Inspector's Live tab realizes the Phyphox instrument bench: it lists external data sources (`◊` RTD, volatile, add-in, linked files) as first-class objects, plots chosen signals over time, separates observation capture controls (Record/Pause capture/Clear) from source controls (Run/Pause/Reset), replays recordings without mutating workbook inputs, and honestly reports RTD source vs sampling rates, coalesced counts, and stale disconnection ages.
- **Cards (8):** `FRAMING-0138`, `P19-FABLE-0135`, `P19-GPT-0320`, `P19-GPT-0321`, `P21-0059`, `P21-0060`, `P21-0061`, `P21-0062`
- **Sources:** FRAMING (1), P19-FABLE (1), P19-GPT (2), P21 (4)
- **Disposition:** `core` — Strong Pro 20/Pro 21 refinement. Specify Live tab instrument controls, RTD coalescing metrics table, side-effect-free Replay, and `Unknown` cycle fallback.
- **`same-as` groups (3):** `FRAMING-0138` = `P21-0059` = `P21-0060`; `P19-GPT-0321` = `P21-0061`; `P19-FABLE-0135` = `P19-GPT-0320`
- **Cross-cluster `same-as`:** `P21-0061` = `P21-0027` (CL-06-07)
- **Agreements:**
  - External data sources (linked workbooks, RTD topics, volatile functions, add-ins) are first-class inspectable objects marking downstream cells with `◊` and reporting provider identity, connection state, and last supplied value/time without silent refresh (P19-FABLE-0135, P19-GPT-0320).
  - Observation capture controls (`Record`, `Pause capture`, `Clear`) are strictly separated from source controls (`Run`, `Pause`, `Reset`); pausing capture leaves live sources running and labels the plot as a recorded observation, and `Reset` restores simulation tick 0 without wiping an existing recording (P21-0059, P21-0062).
  - Statistics (`min`, `max`, `last`) derive strictly from the displayed recording, and `Replay` scrubs a cursor against the dependency explanation without writing to workbook inputs or re-injecting RTD events (FRAMING-0138, P21-0060).
  - RTD telemetry discloses source update rate, observation sampling rate, received/observed/coalesced counts, and last-received age; disconnected feeds retain last value explicitly labelled `stale` with age (FRAMING-0138, P19-GPT-0321, P21-0061).
  - In ticked/cyclic models, missing clock/guard evidence classifies self-referencing cells honestly as `Unknown` rather than assuming a tick delay (P21-0062).
- **Conflicts:** None

### CL-05-11: PivotTable Cache Freshness & CalcVerdict Parity/Trust Readouts

- **Summary:** PivotTable cache staleness is tracked as its own state independent of formula calculation—displaying last refresh time, flagging source changes as 'Stale cache', and refusing to treat unrefreshed caches as current confirmation—while cell, function, and workbook verification badges ground trust readouts directly in CalcVerdict oracle evidence.
- **Cards (5):** `FRAMING-0073`, `P19-FABLE-0071`, `P19-FABLE-0479`, `P19-GPT-0255`, `P21-0054`
- **Sources:** FRAMING (1), P19-FABLE (2), P19-GPT (1), P21 (1)
- **Disposition:** `core` — Keep PivotTable staleness invariant and CalcVerdict trust-state granularity as explicit Inspector verification rules.
- **`same-as` groups (1):** `P19-FABLE-0071` = `P19-FABLE-0479` = `P19-GPT-0255` = `P21-0054`
- **Agreements:**
  - Every PivotTable surface displays its last refresh timestamp and whether its source data has changed since refresh (P19-FABLE-0071, P19-FABLE-0479).
  - PivotTable cache freshness is tracked independently of formula recalculation; an unrefreshed cache is labelled 'Stale cache' and refused as confirmation of a current total until refreshed (P19-GPT-0255, P21-0054).
  - UI trust and Excel-parity verification readouts reflect real CalcVerdict provenance and captured Excel oracle evidence rather than vague status claims (FRAMING-0073).
- **Conflicts:** None

### CL-05-12: Audit Evidence Export, Findings & History Storage, and Printable Dossier

- **Summary:** Inspector turns diagnostic sessions into defensible audit artifacts: Copy evidence exports formulas, name scopes, error origins, and stale-vs-current state as structured text/Markdown/CSV, Findings and cell/agent history persist in DNA Calc's `customXml` part (with optional visible-sheet mirror), and Print outputs either the active instrument or a full Workbook Audit Dossier stamped with file hash and calculation state.
- **Cards (7):** `P19-CARBON-0095`, `P19-FABLE-0070`, `P19-FABLE-0114`, `P19-FABLE-0198`, `P19-NEON-0087`, `P19-NEON-0209`, `P21-0057`
- **Sources:** P19-CARBON (1), P19-FABLE (3), P19-NEON (2), P21 (1)
- **Disposition:** `core-with-decision` — Ground findings/notes/history storage explicitly in `DECISIONS_20261126 #5 & #14` (`customXml` + optional visible-sheet mirror) and unify Copy evidence + Print dossier contracts.
- **Decisions:** [D-05](decisions.md#d-05), [D-54](decisions.md#d-54)
- **`same-as` groups (2):** `P19-CARBON-0095` = `P19-NEON-0087` = `P19-NEON-0209`; `P19-FABLE-0070` = `P21-0057`
- **Cross-cluster `same-as`:** `P19-FABLE-0114` = `FRAMING-0170` (CL-13-03)
- **Agreements:**
  - Copy evidence / Ctrl+C inside Inspector exports clean, paste-ready structured text, Markdown, or CSV preserving cell coordinates, formulas, name scope, error origins, and `Clean / Cached / Dirty` state (P19-FABLE-0070, P19-NEON-0209, P21-0057).
  - Protected worksheets remain fully inspectable for auditing, while requiring an authorised editable copy before any repair can be applied (P21-0057).
  - Long-term cell provenance (`Details › History`) records who edited a cell, when, via which proposal, before/after formulas, and links to findings/notes (P19-FABLE-0114).
  - Audit printouts stamp file hash, timestamp, calculation state (`Clean` vs `Cached/Dirty`), and exact counts so exported PDFs are defensible evidence (P19-NEON-0087).
- **Conflicts:**
  - **Storage location for Inspector findings, notes, and cell/agent edit history** (`P19-FABLE-0114` vs `P19-FABLE-0070`) → Resolved by **[D-05](decisions.md#d-05)**
  - **Default Inspector print scope: active instrument report vs full Workbook Audit Dossier** (`P19-FABLE-0198` vs `P19-CARBON-0095`, `P19-NEON-0087`, `P19-NEON-0209`) → Resolved by **[D-54](decisions.md#d-54)**
  - *Cross-cluster `conflicts-with` links (resolved by D-05, D-54):* `P19-FABLE-0070` vs `FRAMING-0170` (CL-13-03); `P19-FABLE-0114` vs `P19-FABLE-0219` (CL-13-03)

## VI. Notebook

### CL-06-01: Jupyter-Style Computation Chain & Single-Mode Rule (No Separate Read Mode)

- **Summary:** Notebook works exactly like Jupyter with Pluto/Excel live reactivity: every written Excel formula sits directly above its live evaluated output in a visible computation chain, with Markdown prose between links. There is no separate Read presentation mode or formula-hiding Reading density; read-only is strictly a file-open lock, while executive reader-style presentations belong to the App (Easy Forms Maker) view.
- **Cards (14):** `FRAMING-0009`, `FRAMING-0054`, `FRAMING-0056`, `FRAMING-0145`, `FRAMING-0153`, `FRAMING-0171`, `P19-CARBON-0032`, `P19-FABLE-0059`, `P19-FABLE-0458`, `P19-GPT-0017`, `P19-GPT-0042`, `P21-0014`, `P21-0015`, `P21-0037`
- **Sources:** FRAMING (6), P19-CARBON (1), P19-FABLE (2), P19-GPT (2), P21 (3)
- **Disposition:** `core-with-decision` — Foundational chapter opening for Topic VI. State the single Jupyter computation-chain rule upfront, simplify Author chrome (no Author/Read mode switch), and cross-link reader-style executive presentations cleanly to App / Forms Maker.
- **Decisions:** [D-06](decisions.md#d-06)
- **`same-as` groups (5):** `FRAMING-0009` = `FRAMING-0054` = `FRAMING-0145` = `P21-0014`; `FRAMING-0056` = `FRAMING-0171`; `P19-FABLE-0059` = `P19-FABLE-0458`; `P19-CARBON-0032` = `P19-GPT-0017` = `P19-GPT-0042` = `P21-0037`; `FRAMING-0153` = `P21-0015`
- **Agreements:**
  - Notebook combines Jupyter's visible input-formula → live-output vertical chain with Pluto/Excel reactive recalculation (no manual Run Cell button, no stale out-of-order cells) (FRAMING-0009, FRAMING-0054, FRAMING-0145, P21-0014).
  - Prose blocks, live bound numbers, linked tables, workbook-owned charts, and cell-bound controls coexist cleanly over the exact same underlying Excel cells without duplicating values or creating hidden scenarios (P19-CARBON-0032, P19-GPT-0017, P21-0015, P21-0037).
  - Read-only is solely a file-open lock preventing edits, not a separate presentation mode (FRAMING-0171).
- **Conflicts:**
  - **Notebook core paradigm & mode split: single Jupyter computation chain vs narrative-first Reading density vs Author/Read mode toggle** (`FRAMING-0009`, `FRAMING-0054`, `FRAMING-0056`, `FRAMING-0145`, `FRAMING-0171`, `P21-0014` vs `FRAMING-0153`, `P19-CARBON-0032`, `P19-FABLE-0059`, `P19-FABLE-0458`, `P19-GPT-0017`, `P19-GPT-0042`, `P21-0015`, `P21-0037`) → Resolved by **[D-06](decisions.md#d-06)**
  - *Cross-cluster `conflicts-with` links (resolved by D-06):* `FRAMING-0054` vs `P19-CARBON-0242` (CL-10-17), `P19-FABLE-0060` (CL-13-06), `P19-FABLE-0240` (CL-16-04), `P19-NEON-0284` (CL-10-17); `FRAMING-0056` vs `P19-CARBON-0242` (CL-10-17), `P19-FABLE-0060` (CL-13-06), `P19-FABLE-0240` (CL-16-04), `P19-NEON-0284` (CL-10-17); `FRAMING-0145` vs `P19-CARBON-0242` (CL-10-17), `P19-FABLE-0060` (CL-13-06), `P19-FABLE-0240` (CL-16-04), `P19-NEON-0284` (CL-10-17); `FRAMING-0171` vs `P19-CARBON-0242` (CL-10-17), `P19-FABLE-0060` (CL-13-06), `P19-FABLE-0240` (CL-16-04), `P19-NEON-0284` (CL-10-17); `P21-0014` vs `P19-CARBON-0242` (CL-10-17), `P19-FABLE-0060` (CL-13-06), `P19-FABLE-0240` (CL-16-04), `P19-NEON-0284` (CL-10-17)

### CL-06-02: First-Visit Onboarding, Non-Mutating Scaffolding & Large-Workbook Guardrails

- **Summary:** Opening Notebook on a workbook without a saved notebook layout presents a fast, non-mutating First-Run Card anchored to the user's current selection, an optional drafted chain from workbook structure, or a blank canvas—while refusing to dump sprawling 40-sheet / 285k-cell legacy workbooks inline and offering a focused subset instead.
- **Cards (11):** `FRAMING-0074`, `P19-CARBON-0031`, `P19-CARBON-0059`, `P19-FABLE-0053`, `P19-FABLE-0153`, `P19-FABLE-0455`, `P19-GPT-0015`, `P19-NEON-0028`, `P19-NEON-0052`, `P19-NEON-0210`, `P21-0036`
- **Sources:** FRAMING (1), P19-CARBON (2), P19-FABLE (3), P19-GPT (1), P19-NEON (3), P21 (1)
- **Disposition:** `core-with-decision` — Specify First-Run Card options, 400 ms draft budget, large-workbook subset guardrail, `Ctrl+Alt+3` shortcut, and `FLOW_DESIGN` keyboard map.
- **Decisions:** [D-12](decisions.md#d-12), [D-55](decisions.md#d-55)
- **`same-as` groups (3):** `P19-CARBON-0031` = `P19-CARBON-0059`; `P19-FABLE-0053` = `P19-FABLE-0455` = `P19-NEON-0028` = `P19-NEON-0210`; `P19-GPT-0015` = `P21-0036`
- **Agreements:**
  - Drafting or opening a Notebook layout leaves the workbook model hash completely unchanged (zero worksheet edits until the user creates or edits a backing cell) (P19-CARBON-0031, P19-FABLE-0053, P19-FABLE-0455, P19-NEON-0028).
  - Notebook declines auto-expanding huge legacy workbooks (e.g. Inherited's 40 sheets / 285k cells) and instead offers selecting a focused subset or starting from the current selection (P19-CARBON-0059, P19-FABLE-0153, P19-NEON-0052).
  - Keyboard navigation (`↑/↓` between cells, `Enter`/`F2` edit, `Esc` exit, `Alt+↑/↓` reorder, `/` block-insert menu) builds on carried-forward `FLOW_DESIGN` ergonomics (FRAMING-0074, P19-FABLE-0153, P19-NEON-0052).
- **Conflicts:**
  - **First-visit onboarding: automatic outline synthesis vs explicit First-Run choice card** (`P19-FABLE-0053`, `P19-FABLE-0455`, `P19-GPT-0015`, `P19-NEON-0028`, `P19-NEON-0210`, `P21-0036` vs `P19-CARBON-0031`, `P19-CARBON-0059`) → Resolved by **[D-55](decisions.md#d-55)**
  - **Notebook view-switching shortcut and inline live-value quote trigger** (`P19-NEON-0028`, `P19-NEON-0052` vs `P19-CARBON-0059`, `P19-FABLE-0153`) → Resolved by **[D-12](decisions.md#d-12)**
  - *Cross-cluster `conflicts-with` links (resolved by D-12, D-55):* `P19-CARBON-0059` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28); `P19-FABLE-0153` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28)

### CL-06-03: Excel-Backed Cells, Column-Lane Backing Sheet & Named Outputs

- **Summary:** Zero floating non-Excel formulas exist in Notebook: every computation cell allocates a real Excel cell on a visible backing sheet arranged in non-colliding column lanes for downward dynamic-array spills, binds to a readable workbook defined name (`=Analysis!$A$10#`) with safe global rename and click-to-definition navigation, and opens directly in Classic with outlined worksheet anchors.
- **Cards (7):** `FRAMING-0147`, `P19-NEON-0212`, `P21-0017`, `P21-0018`, `P21-0019`, `P21-0033`, `P21-0035`
- **Sources:** FRAMING (1), P19-NEON (1), P21 (5)
- **Disposition:** `core` — Backbone of the Notebook-to-Excel compatibility story. Make sure visible workbook names are explicitly distinguished from hidden anchor names (forbidden by `DECISIONS_20261126 #13`).
- **`same-as` groups (1):** `FRAMING-0147` = `P19-NEON-0212` = `P21-0018` = `P21-0019`
- **Agreements:**
  - Creating a notebook formula cell allocates a real Excel cell and visible workbook defined name (displaying the backing address explicitly); no non-Excel floating math exists (P19-NEON-0212, P21-0018, P21-0035).
  - Visible backing sheets organize inputs, scalars, notes, and dynamic-array spills into separate column lanes starting at row 10 (e.g. `A:G`, `J:K`, `M:N`, `Q:R`, `U:V`) so vertical notebook reading order works without downward spill collisions in Excel (P21-0017).
  - Clicking a notebook output name allows renaming with Excel name-syntax validation, conflict rejection, and exact updates to all downstream formulas while keeping the cell anchor fixed (FRAMING-0147, P21-0018).
  - Clicking any workbook name inside a downstream formula or prose quote navigates directly to its producing cell, and 'Open in Classic / Backing cells' reveals the exact live calculation state with outlined cell anchors (P21-0019, P21-0033).
  - Concrete Excel syntax (`FILTER`, `GROUPBY`, `PIVOTBY`, `SORTBY`, `LET`, `RTD`, structured Table references, `IF(ISERROR(...))`) is preserved verbatim as written (P21-0035).
- **Conflicts:** None

### CL-06-04: Inline Formula Editing, Pluto/Excel Reactivity, Formula Runs & Draft/Error Isolation

- **Summary:** Notebook cells provide full Excel formula editing (`Ctrl+Space` completion, signature help) with immediate Pluto/Excel live recalculation and no Run button (while respecting Manual calc mode honestly). Incomplete drafts isolate downstream outputs into an explicit draft state, empty `FILTER` results propagate `#CALC!` cleanly through guards, and legacy multi-cell formula runs edit atomically via a preview seam.
- **Cards (9):** `FRAMING-0146`, `FRAMING-0148`, `P19-CARBON-0189`, `P19-FABLE-0057`, `P19-FABLE-0062`, `P19-GPT-0256`, `P21-0022`, `P21-0023`, `P21-0025`
- **Sources:** FRAMING (2), P19-CARBON (1), P19-FABLE (2), P19-GPT (1), P21 (3)
- **Disposition:** `core-with-decision` — Detail inline editor, Pluto reactivity (respecting Manual calc mode honestly), draft-isolation state, `#CALC!` error guards, and unified run-card preview seam for legacy runs.
- **Decisions:** [D-56](decisions.md#d-56)
- **`same-as` groups (2):** `FRAMING-0146` = `FRAMING-0148` = `P19-CARBON-0189` = `P21-0022`; `P19-FABLE-0057` = `P19-FABLE-0062`
- **Agreements:**
  - Notebook cell editors supply full Excel completion (`Ctrl+Space`) and signature help, recalculating all downstream tables, charts, and prose quotes automatically in dependency order with brief change marks and no manual Run Cell button (FRAMING-0146, FRAMING-0148, P19-CARBON-0189, P21-0022).
  - Notebook visual order (vertical or horizontal) has zero effect on Excel calculation order; moving a prose paragraph creates no cell or name (P19-GPT-0256).
  - An incomplete or unsupported formula draft replaces all dependent displays with an explicit draft state so stale outputs never masquerade as current results; `Esc` restores the last committed expression (P21-0023).
  - Omitting `if_empty` on `FILTER` produces exact Excel `#CALC!` on zero matches, forwarded cleanly via `IF(ISERROR(...))` guards with zero stale tables or charts left visible (P21-0025).
- **Conflicts:**
  - **Authoring unit for computed blocks: single-anchor dynamic-array cells vs multi-cell relative formula runs** (`FRAMING-0146`, `P21-0022`, `P21-0025` vs `P19-FABLE-0057`, `P19-FABLE-0062`) → Resolved by **[D-56](decisions.md#d-56)**

### CL-06-05: Reusable Function Cells (`=LAMBDA(...)`)

- **Summary:** A Notebook function cell defines a reusable workbook-scoped `=LAMBDA(...)` stored as a standard Excel defined name, visually distinct from value cells, displaying its callable signature rather than a fabricated scalar, and callable by ordinary formulas across any worksheet.
- **Cards (2):** `FRAMING-0149`, `P21-0030`
- **Sources:** FRAMING (1), P21 (1)
- **Disposition:** `core` — Clean, self-contained Pro 21 specification. Highlight exact Excel Name Manager round-trip parity.
- **`same-as` groups (1):** `FRAMING-0149` = `P21-0030`
- **Agreements:**
  - Function cells bind directly to workbook-scoped defined names holding `=LAMBDA(...)` definitions, making them 100% native Excel reusable functions callable from any sheet or downstream notebook cell (FRAMING-0149, P21-0030).
  - Function cells are styled distinctly from value cells and display their callable signature (parameter list) rather than fabricating a fake scalar output from an uncalled worksheet cell (FRAMING-0149, P21-0030).
- **Conflicts:** None

### CL-06-06: Typed Data Frames, Windowed Previews, PIVOTBY & Safe Sort/Filter

- **Summary:** Excel Tables and dynamic arrays render as typed Data Frames with column types, sticky headers/totals, explicit row-window dimensions (`n of N`), and full-scope evaluation over all rows (never sampled math). `PIVOTBY` stays an honest dynamic-array spill (not a PivotTable cache), and sorting/filtering defaults to non-mutating View-only presentation with an explicit reviewed Apply path for Table mutations.
- **Cards (9):** `FRAMING-0139`, `P19-CARBON-0191`, `P19-FABLE-0058`, `P19-FABLE-0456`, `P19-GPT-0258`, `P21-0016`, `P21-0020`, `P21-0039`, `P21-0040`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-FABLE (2), P19-GPT (1), P21 (4)
- **Disposition:** `core-with-decision` — Specify typed Data Frame anatomy, `n of N` full-scope honesty rule, `PIVOTBY` spill contract, and the two-tier View-only vs reviewed 'Changes the Table' sort/filter flow.
- **Decisions:** [D-57](decisions.md#d-57)
- **`same-as` groups (2):** `P19-CARBON-0191` = `P19-FABLE-0456` = `P21-0016`; `P19-FABLE-0058` = `P19-GPT-0258` = `P21-0039`
- **Agreements:**
  - Large tables and spills (>200 rows) render as virtualised, windowed Data Frames displaying exact full dimensions (`n of N`, folded column/section counts with errors/exceptions) while evaluating all formulas and aggregates across 100% of source rows (FRAMING-0139, P19-CARBON-0191, P19-FABLE-0456, P21-0016).
  - `PIVOTBY` outputs render as dynamic-array formula spills with exact header/total rows rather than PivotTable caches and never invent unpopulated months (P21-0020).
  - Editing a row inside a displayed/filtered Notebook Data Frame always addresses the true underlying worksheet row, and Find / Copy all matches operate on the full matching scope (P19-GPT-0258, P21-0040).
- **Conflicts:**
  - **Sorting and filtering Notebook Data Frames: strictly view-only vs optional reviewed 'Changes the Table' mutation** (`FRAMING-0139`, `P21-0040` vs `P19-FABLE-0058`, `P19-GPT-0258`) → Resolved by **[D-57](decisions.md#d-57)**

### CL-06-07: Prose Cells, Live Value Quotes, Cell-Bound Controls & Shared RTD Streams

- **Summary:** Markdown prose cells sit between computation steps and embed live cell quotations that take their source cell's number format and recalculate automatically (refusing floating unbacked prose math). Interactive parameter controls (sliders, dropdowns, date pickers, checkboxes) bind directly to ordinary cells, and live RTD cells share one observer stream with Inspector while isolating provider outages and stale prices from historical totals.
- **Cards (11):** `FRAMING-0150`, `P19-FABLE-0055`, `P19-FABLE-0457`, `P19-GPT-0016`, `P19-GPT-0257`, `P19-NEON-0029`, `P19-NEON-0211`, `P21-0024`, `P21-0027`, `P21-0029`, `P21-0041`
- **Sources:** FRAMING (1), P19-FABLE (2), P19-GPT (2), P19-NEON (2), P21 (4)
- **Disposition:** `core` — Strong cross-source consensus. Document prose live-quote chip interaction (`=` trigger, link mark on focus, Escape draft cancel), cell-bound control bindings, and single-stream RTD outage isolation.
- **`same-as` groups (3):** `P19-FABLE-0055` = `P19-FABLE-0457` = `P19-GPT-0016` = `P19-GPT-0257` = `P19-NEON-0211`; `FRAMING-0150` = `P19-NEON-0029` = `P21-0024`; `P21-0027` = `P21-0029` = `P21-0041`
- **Cross-cluster `same-as`:** `P21-0027` = `P21-0061` (CL-05-10)
- **Agreements:**
  - Live values quoted inside Markdown prose bind directly to real workbook cells, inherit their cell's number format, show source-cell provenance on focus, and update live on recalculation (P19-FABLE-0055, P19-FABLE-0457, P19-GPT-0016, P19-GPT-0257, P19-NEON-0211).
  - No calculation hides inside prose: typing an unbacked formula expression in prose is refused with an inline offer to quote an existing cell or create a real Excel-backed computation cell (P19-FABLE-0055, P19-NEON-0029).
  - Interactive controls (dropdowns, sliders, clamped date handles, checkboxes) bind directly to ordinary worksheet input cells (e.g. `Analysis!B2:B5`, `Inputs!C2`) and drive downstream outputs immediately (FRAMING-0150, P19-NEON-0029, P21-0024).
  - Live RTD feeds in Notebook share a single observer stream with Inspector and Data frames, clean up observers on navigation, label disconnected values `stale` without asserting current valuations, and keep historical totals completely independent (P21-0027, P21-0029, P21-0041).
- **Conflicts:** None

### CL-06-08: Dynamic-Array `#SPILL!` Honesty & Safe Blocker Relocation

- **Summary:** Dynamic-array spill collisions are treated as real Excel model events: creating a formula previews its backing destination and spill footprint, growing into an occupied cell returns genuine `#SPILL!` forwarded cleanly to dependents with zero stale charts or prose left visible, spill children are never independently editable, and safe reviewed blocker relocation moves only the obstructing cell to an empty destination with full Undo.
- **Cards (4):** `P19-GPT-0018`, `P19-GPT-0259`, `P21-0031`, `P21-0032`
- **Sources:** P19-GPT (2), P21 (2)
- **Disposition:** `core` — Exact Excel `#SPILL!` honesty contract in Notebook. Pair with `P21-0017` column-lane layout (CL-06-03) that prevents routine spills from colliding in the first place.
- **`same-as` groups (1):** `P19-GPT-0018` = `P19-GPT-0259` = `P21-0031` = `P21-0032`
- **Cross-cluster `same-as`:** `P21-0032` = `P19-FABLE-0544` (CL-05-07)
- **Agreements:**
  - Adding a notebook formula previews its backing cell, spill footprint, and any occupied blockers before commit; spill child cells are never independently editable (P19-GPT-0018, P19-GPT-0259).
  - When a growing spill hits an obstructing cell, `#SPILL!` stays real and explicit error guards forward `#SPILL!` to every dependent card with zero partial output, prior chart, or successful-looking prose remaining (P19-GPT-0259, P21-0031).
  - Spill repair ('Reveal blocker / Move blocker...') moves only the obstructing cell/formula and its defined name to an explicitly chosen empty destination without altering the spilling formula; Apply disables if the target is occupied, Cancel is inert, and Undo restores the exact blockage (P19-GPT-0018, P21-0032).
- **Conflicts:** None

### CL-06-09: Read-Only Formula Lens & Inline Live Expression Tree

- **Summary:** Formula Lens displays human-readable aliases alongside exact Excel formula text without mutating formulas, adding hidden names, or refactoring evaluation multiplicity (such as collapsing `RAND()+RAND()`). Complementary to it, any `LET` or nested formula cell expands inline into a live expression tree showing each binding's current evaluated value, explicitly labelled as expression structure rather than a runtime execution trace.
- **Cards (8):** `FRAMING-0152`, `P19-GPT-0090`, `P19-GPT-0235`, `P19-GPT-0236`, `P19-GPT-0237`, `P19-GPT-0308`, `P21-0026`, `P21-0038`
- **Sources:** FRAMING (1), P19-GPT (5), P21 (2)
- **Disposition:** `core-with-decision` — Strong GPT + Pro 21 consensus. Highlight the non-refactoring invariant (`RAND()+RAND()`) and share the inline Live Tree component with Formula Explorer.
- **Decisions:** [D-23](decisions.md#d-23)
- **`same-as` groups (2):** `P19-GPT-0090` = `P19-GPT-0235` = `P19-GPT-0236` = `P19-GPT-0237` = `P21-0038`; `FRAMING-0152` = `P21-0026`
- **Cross-cluster `same-as`:** `FRAMING-0152` = `P19-FABLE-0536` (CL-05-04)
- **Agreements:**
  - Formula Lens is strictly read-only presentation: toggling it never edits the source formula or adds workbook names, and preserves exact meaning of scoped/relative/structured references, `@` implicit intersection, `#` spill operator, errors, and literals (P19-GPT-0090, P19-GPT-0235, P21-0038).
  - Formula Lens never performs silent algebraic or `LET` refactoring that could alter evaluation multiplicity (e.g. `RAND()+RAND()`), coercion, or error semantics; ambiguous shortened aliases are refused or open exact source editing (P19-GPT-0090, P19-GPT-0236, P19-GPT-0237).
  - Any `LET` or nested formula expands inline into a live binding/argument tree displaying each node's current evaluated value directly under the cell, explicitly labelled as expression structure rather than an execution firing order (FRAMING-0152, P21-0026, P19-GPT-0308).
- **Conflicts:**
  - *Cross-cluster `conflicts-with` links (resolved by D-23):* `P19-GPT-0090` vs `P19-NEON-0031` (CL-05-06)

### CL-06-10: Horizontal Cross-Sheet Pipeline Layout (`Pipeline` / `Across`)

- **Summary:** Any Notebook chain can toggle between vertical flow and a horizontal left-to-right Pipeline layout (`Inputs → Monthly → Summary` or `Orders → Report → PivotTable1`) connected by live precedent rails. Pipeline arrangement is pure reading order that never alters calculation order, explicitly labels windowed excerpts and PivotTable staleness, marks prior results while recalculating, and reflows vertically at 200% zoom.
- **Cards (6):** `P19-CARBON-0060`, `P19-CARBON-0190`, `P19-FABLE-0146`, `P19-NEON-0053`, `P19-NEON-0213`, `P21-0042`
- **Sources:** P19-CARBON (2), P19-FABLE (1), P19-NEON (2), P21 (1)
- **Disposition:** `core-with-decision` — Present Pipeline as the sole secondary flow layout of the Jupyter chain (`Alt+P`), preserving every honesty rule (excerpts, pending states, Pivot staleness, 200% vertical reflow).
- **Decisions:** [D-58](decisions.md#d-58)
- **`same-as` groups (1):** `P19-CARBON-0060` = `P19-CARBON-0190` = `P19-NEON-0053` = `P19-NEON-0213` = `P21-0042`
- **Agreements:**
  - Horizontal Pipeline lays out multi-sheet or multi-stage chains left-to-right with live precedent connectors where hovering highlights downstream dependencies (P19-CARBON-0060, P19-NEON-0053, P19-NEON-0213).
  - Pipeline arrangement is strictly reading order stored in `customXml`, never altering Excel calculation order (P19-CARBON-0190, P21-0042).
  - Pipeline cards honestly disclose windowed excerpts (e.g. 3 of 12 months while annual total sums all 12), hatched/labelled 'Previous result' states during pending recalculation, and PivotTable cache staleness (`[!] Source changed after last refresh`, `Alt+F5`) (P19-FABLE-0146, P21-0042).
- **Conflicts:**
  - **Naming of horizontal flow layout: 'Pipeline' vs 'Across'** (`P19-CARBON-0060`, `P19-CARBON-0190`, `P19-NEON-0053`, `P19-NEON-0213`, `P21-0042` vs `P19-FABLE-0146`) → Resolved by **[D-58](decisions.md#d-58)**

### CL-06-11: Layout Persistence, Presentation-vs-Model Undo & Accessibility

- **Summary:** Notebook prose and block arrangement persist exclusively in `customXml`, visually separating presentation edits ('saved, layout edited') from workbook model edits while keeping cross-view Undo exact with origin toasts. Accessibility guarantees semantic headings, descriptive ARIA labels for formulas and live chips, and zero sideways scrolling at 200% zoom.
- **Cards (4):** `P19-FABLE-0054`, `P19-FABLE-0061`, `P19-FABLE-0459`, `P19-NEON-0030`
- **Sources:** P19-FABLE (3), P19-NEON (1)
- **Disposition:** `core` — Anchor Notebook persistence in `customXml`, cross-view Undo toast semantics, and WCAG 200%-zoom/ARIA acceptance requirements.
- **`same-as` groups (1):** `P19-FABLE-0061` = `P19-FABLE-0459` = `P19-NEON-0030`
- **Agreements:**
  - Reordering notebook blocks (Outline panel `F6`, `Alt+↑/↓`, or drag handle) changes only presentation persisted in `customXml` and shows 'saved, layout edited' in the Mast so the user knows the Excel model is untouched (P19-FABLE-0054, P19-NEON-0030).
  - Cross-view `Ctrl+Z` reverts workbook edits made in Notebook from Classic (and vice versa) with a Strip toast naming the origin view (P19-NEON-0030).
  - At 200% zoom, Notebook prose reflows cleanly with zero horizontal page scroll while Data Frames scroll internally (P19-FABLE-0061, P19-FABLE-0459, P19-NEON-0030).
  - Screen-reader semantics expose real document headings, spoken formula/name/count/value-range announcements on computation cells, and descriptive ARIA labels on live prose chips (P19-FABLE-0061, P19-NEON-0030).
- **Conflicts:** None

### CL-06-12: Cross-View Clipboard, External Export & Honest Publication Printing

- **Summary:** Copy/paste respects destination semantics: copying inside Notebook duplicates live linked cards, pasting Classic ranges defaults to Linked Live Data Frames (never silently creating cross-workbook external links), and copying to external apps exports clean rich HTML, Markdown, resolved formatted numbers, and vector SVG charts. Printing waits for pending calculation (or stamps a snapshot), repeats Data Frame headers, and discloses any sampled row window.
- **Cards (6):** `P19-CARBON-0033`, `P19-CARBON-0096`, `P19-FABLE-0199`, `P19-GPT-0019`, `P19-NEON-0088`, `P19-NEON-0214`
- **Sources:** P19-CARBON (2), P19-FABLE (1), P19-GPT (1), P19-NEON (2)
- **Disposition:** `core-with-decision` — Specify exact clipboard matrix (same-workbook, cross-workbook, external Word/email, Excel cells) and honest print pagination/sampling disclosure rules.
- **Decisions:** [D-28](decisions.md#d-28)
- **`same-as` groups (2):** `P19-CARBON-0033` = `P19-NEON-0214`; `P19-CARBON-0096` = `P19-FABLE-0199` = `P19-NEON-0088`
- **Agreements:**
  - Copying Notebook content to external programs (Word, email) writes clean rich HTML + plain Markdown with evaluated formatted values, vector SVG charts, and revision/timestamp metadata, while copying cell values for Excel exports ordinary cells (P19-CARBON-0033, P19-GPT-0019, P19-NEON-0214).
  - Cross-workbook paste of a linked item explicitly asks whether to bind to an existing target or copy static cells, never silently creating external workbook links (P19-GPT-0019).
  - Notebook printing outputs clean paginated chain order with vector charts, repeating headers across page breaks on Data Frames, waiting for pending calculation or stamping a saved snapshot, and disclosing any truncated row window (`First 200 of 25,000 rows printed; aggregates exact over all 25,000`) with an opt-in `[ ] Expand full table` checkbox (P19-CARBON-0096, P19-FABLE-0199, P19-GPT-0019, P19-NEON-0088).
- **Conflicts:**
  - **Default paste behaviour when pasting a Classic range into Notebook** (`P19-NEON-0214` vs `P19-GPT-0019`) → Resolved by **[D-28](decisions.md#d-28)**

## VII. Model map

### CL-07-01: Sheet-Truth Projection & Bidirectional Grid Interplay

- **Summary:** Model map is an architectural projection and alternate formula editor over the authoritative backing workbook, never a parallel calculation graph. Every box and wire derives from real sheets, defined names, inferred blocks, and engine precedents/dependents, staying live-synchronized with Classic grid in split view so every saved file opens unchanged in Excel.
- **Cards (7):** `FRAMING-0010`, `FRAMING-0106`, `FRAMING-0108`, `FRAMING-0109`, `P19-CARBON-0192`, `P19-FABLE-0081`, `P19-FABLE-0085`
- **Sources:** FRAMING (4), P19-CARBON (1), P19-FABLE (2)
- **Disposition:** `core-with-decision` — Anchor the Model map chapter with this invariant: sheet-truth projection first, zero proprietary calculation semantics, and instant split-view verification against Classic (`Ctrl+\` or split toggle).
- **Decisions:** [D-70](decisions.md#d-70)
- **`same-as` groups (1):** `FRAMING-0109` = `P19-CARBON-0192`
- **Agreements:**
  - The backing worksheet and standard Excel formulas remain the single source of truth; the workbook opens and calculates in Excel with zero engine extensions.
  - First-class defined names (including Excel's native dot separator) and host precedent/dependent graphs provide all semantics Model map needs.
  - Classic grid and Model map share one selection and formula bar in split view so edits in either pane update the other immediately.
- **Conflicts:**
  - *Cross-cluster `conflicts-with` links (resolved by D-70):* `FRAMING-0108` vs `P19-FABLE-0150` (CL-09-03)

### CL-07-02: Cell-Backed Authoring Default & Name Allocation

- **Summary:** Boxes created directly on the Model map are always backed by real, labelled cells on a visible worksheet bound to non-colliding Excel defined names—never invisible Name-Manager-only formulas that produce blank 'ghost workbooks' in desktop Excel. Empty-map sketching teaches a single `name = formula` grammar with an inline Preview Seam showing exact cell placement before commit.
- **Cards (10):** `P19-CARBON-0036`, `P19-CARBON-0112`, `P19-CARBON-0118`, `P19-CARBON-0193`, `P19-FABLE-0074`, `P19-FABLE-0075`, `P19-FABLE-0234`, `P19-FABLE-0463`, `P19-NEON-0033`, `P19-NEON-0266`
- **Sources:** P19-CARBON (4), P19-FABLE (4), P19-NEON (2)
- **Disposition:** `core-with-decision` — Specify the `name = formula` blank-paper grammar, two-column cell allocation, workbook-scoped default with sheet-scope option, and non-colliding allocator (`C-28`). Explicitly ban hidden names (`DECISIONS_20261126 #13`).
- **Decisions:** [D-59](decisions.md#d-59)
- **`same-as` groups (2):** `P19-CARBON-0112` = `P19-CARBON-0193` = `P19-FABLE-0234`; `P19-FABLE-0074` = `P19-FABLE-0463`
- **Agreements:**
  - Creating a new box on the map allocates a visible label cell and value cell on a real worksheet and binds a non-colliding Excel defined name so Excel recipients see a clean, readable model.
  - Existing workbooks containing pure Name-Manager formula names are still rendered faithfully on the map even though map authoring defaults to cell-backed names.
  - Empty-paper sketching shows a Preview Seam detailing the exact sheet, label cell, value cell, defined name, and inferred prefix group before writing.
  - Top-down sketching supports referencing not-yet-defined names with an explicit choice to create empty boxes or commit as typed (`#NAME?`), and pasting vectors previews standard dynamic array spills.
- **Conflicts:**
  - **Workbook-scoped vs sheet-scoped defined names for map-created boxes** (`P19-CARBON-0036`, `P19-CARBON-0118` vs `P19-FABLE-0074`, `P19-FABLE-0234`) → Resolved by **[D-59](decisions.md#d-59)**

### CL-07-03: Dotted-Name Ports, Source Toggle & Inferred vs Real Visual Distinction

- **Summary:** Boxes expose live values on source-bound ports, strip active dotted group prefixes inside composite frames via an AST-safe presenter, and provide a Source toggle plus a Connections list for exact cell verification. Visual framing strictly distinguishes real workbook objects (solid frames for sheets, names, and tables) from inferred blocks (dashed frames with guillemet labels), with an exact accounting line guaranteeing every wire matches real formula references.
- **Cards (6):** `FRAMING-0104`, `P19-FABLE-0147`, `P19-FABLE-0460`, `P19-FABLE-0461`, `P19-NEON-0215`, `P21-0065`
- **Sources:** FRAMING (1), P19-FABLE (3), P19-NEON (1), P21 (1)
- **Disposition:** `core-with-decision` — Adopt Fable's solid-vs-dashed visual grammar, exact wire-accounting rule (`R-48`), P21's live source-bound ports + Source toggle + Connections list, and AST prefix stripping without non-typeable superscript glyphs.
- **Decisions:** [D-60](decisions.md#d-60)
- **`same-as` groups (1):** `P19-FABLE-0147` = `P19-FABLE-0461`
- **Agreements:**
  - Every drawn wire represents at least one real formula reference, and every reference between drawn items is represented by a wire or counted bundle, verified by an exact 'x of y accounted for' line.
  - Real workbook structures (sheets, defined names, Excel Tables) use solid frames; inferred blocks/groups use dashed frames and guillemet labels (`‹Revenue›`) so inference is never mistaken for file structure.
  - Nodes show live values on source-bound ports, a Source toggle for compact inline Excel formulas, a direct jump to backing cells, and a keyboard/reduced-motion Connections list alternative.
  - Dotted name prefix stripping runs over the parsed formula AST so strings, ranges, and function names are never corrupted.
- **Conflicts:**
  - **Self-reference notation inside AST Formula Presenter (`OnHand⁻` superscript vs plain speakable label)** (`P19-NEON-0215` vs `P21-0065`) → Resolved by **[D-60](decisions.md#d-60)**

### CL-07-04: Hierarchy, Drill-Down, Breadcrumbs & Optional Sheet Pages

- **Summary:** Model map organises complex workbooks into drillable hierarchical groups (Enter to open, breadcrumbs/Esc to climb out) without inventing non-Excel scoping rules, keeping selection and parameter controls intact across levels. Recurring schedules collapse to expandable period-recurrence blocks, duplicate source appearances share live state, and optional sheet-page tabs align the canvas with workbook tabs.
- **Cards (4):** `P19-GPT-0043`, `P19-GPT-0263`, `P19-NEON-0216`, `P21-0066`
- **Sources:** P19-GPT (2), P19-NEON (1), P21 (1)
- **Disposition:** `core-with-decision` — Standardize breadcrumb drill-down, shared-state duplicate appearances, expandable recurrence blocks, optional sheet-page tabs, and non-scroll-hijacking `Ctrl+Wheel` semantic zoom.
- **Decisions:** [D-61](decisions.md#d-61)
- **`same-as` groups (1):** `P19-GPT-0043` = `P21-0066`
- **Agreements:**
  - Hierarchical groups open into member nodes via Enter/click and climb back via breadcrumbs (`Stock › Replenishment › Day 5`) or Esc/Back without losing selection or shared parameter controls.
  - Groups reflect workbook/prefix/sheet membership strictly without inventing proprietary Excel scoping; each group exposes full membership and exceptions, and duplicate node appearances share state.
  - Optional per-sheet page tabs synchronize map navigation with workbook sheets, and long recurring schedules (such as Loan's 360 monthly rows) appear as compact expandable recurrence blocks.
  - Dynamic INDEX ranges distinguish the full possible lookup range from the currently selected precedent.
- **Conflicts:**
  - **Mouse wheel default on map canvas: scroll vs zoom** (`P19-GPT-0043` vs `P19-NEON-0216`) → Resolved by **[D-61](decisions.md#d-61)**

### CL-07-05: Staged Authoring Rollout, Formula-Exact Wiring & Safe Deletion

- **Summary:** Model map ships as a rock-solid architectural reader before wiring edits and blank-paper authoring. Every visual wire change is strictly a previewed, cell-specific Excel formula edit showing exact before/after formulas and live preview values, refusing ambiguous multi-operand targets in favour of explicit operand selection, and never silently substituting zero on wire deletion.
- **Cards (9):** `FRAMING-0065`, `P19-FABLE-0078`, `P19-FABLE-0220`, `P19-FABLE-0466`, `P19-GPT-0021`, `P19-GPT-0260`, `P19-GPT-0261`, `P19-NEON-0104`, `P21-0067`
- **Sources:** FRAMING (1), P19-FABLE (3), P19-GPT (3), P19-NEON (1), P21 (1)
- **Disposition:** `core-with-decision` — Define the 3-stage rollout (Reader -> Safe Rewiring -> Blank-Paper Building), exact Preview Seam rules, operand disambiguation, presentation-only node deletion, and zero-free wire deletion.
- **Decisions:** [D-62](decisions.md#d-62)
- **`same-as` groups (2):** `P19-GPT-0021` = `P19-GPT-0260` = `P21-0067`; `FRAMING-0065` = `P19-FABLE-0220`
- **Agreements:**
  - Every wire drag or keyboard 'Connect to…' draws a dotted ghost wire and opens a Preview Seam displaying the exact source formula before/after and resulting value change; Esc cancels with zero mutation.
  - Clicking a box while editing a formula inserts its reference cleanly.
  - Protected targets allow non-mutating preview inspection while blocking Apply.
  - Deleting a node on the map removes only its visual presentation by default (clearly labelled), never deleting underlying worksheet cells without explicit intent.
- **Conflicts:**
  - **Scope of visual wiring edits: named ports only vs operand-targeted cell rewiring** (`FRAMING-0065`, `P19-FABLE-0220`, `P19-NEON-0104` vs `P19-GPT-0021`, `P19-GPT-0260`, `P21-0067`) → Resolved by **[D-62](decisions.md#d-62)**
  - **Wire deletion semantics: explicit replacement required vs restoring literal/empty** (`P19-GPT-0021`, `P19-GPT-0261` vs `P19-FABLE-0078`) → Resolved by **[D-62](decisions.md#d-62)**

### CL-07-06: Circular-Reference Preview Seam, Iteration Settings & Honest Convergence

- **Summary:** Closing a circular loop while Excel Iterative Calculation is OFF never triggers an opaque modal popup or silently mutates workbook settings; it pauses in an inline Preview Seam offering explicit Excel iteration settings, keeping iteration off (with circular warning), or cancelling. Active circular groups display real Excel iteration parameters and test non-converging edits honestly in temporary state.
- **Cards (11):** `FRAMING-0079`, `P19-CARBON-0037`, `P19-CARBON-0194`, `P19-CARBON-0216`, `P19-FABLE-0076`, `P19-FABLE-0077`, `P19-FABLE-0462`, `P19-FABLE-0464`, `P19-GPT-0022`, `P19-NEON-0034`, `P19-NEON-0218`
- **Sources:** FRAMING (1), P19-CARBON (3), P19-FABLE (4), P19-GPT (1), P19-NEON (2)
- **Disposition:** `core-with-decision` — Keep the inline Preview Seam for cycle closure (`M4.2`/`R-52`), explicit scope review for Excel's iteration settings, honest non-converging preview testing, and strict ban on conflating circularity with `[z⁻¹]`.
- **Decisions:** [D-03](decisions.md#d-03)
- **`same-as` groups (3):** `P19-FABLE-0076` = `P19-FABLE-0464`; `P19-FABLE-0077` = `P19-FABLE-0462`; `P19-CARBON-0037` = `P19-CARBON-0194` = `P19-NEON-0034` = `P19-NEON-0218`
- **Agreements:**
  - Closing a circular wire when Iterative Calculation is OFF pauses at an inline Preview Seam (no modal popup) presenting explicit workbook iteration choices or `Esc` cancel.
  - The host exposes workbook calculation mode, Iterative Calculation flag, Maximum Iterations, Maximum Change, and single-step recalculation (`C-10`), respecting Excel's single-thread vs multithread circular-reference observations (`FRAMING-0079`).
  - Non-converging formula previews run in temporary workbook state and report the real stop reason or 'Convergence status unavailable'—never a green verified badge.
- **Conflicts:**
  - **Whether closing a circular loop automatically tags the arc as a `[z⁻¹]` delay / Ticks Director** (`P19-CARBON-0037`, `P19-CARBON-0194`, `P19-NEON-0034`, `P19-NEON-0218` vs `P19-FABLE-0077`, `P19-FABLE-0462`, `P19-GPT-0022`) → Resolved by **[D-03](decisions.md#d-03)**
  - *Cross-cluster `conflicts-with` links (resolved by D-03):* `P19-CARBON-0037` vs `FRAMING-0053` (CL-07-07), `FRAMING-0068` (CL-01-03), `FRAMING-0141` (CL-07-08), `FRAMING-0168` (CL-07-08), `P19-GPT-0088` (CL-07-07), `P21-0064` (CL-07-07) (+2 more); `P19-CARBON-0194` vs `FRAMING-0053` (CL-07-07), `FRAMING-0068` (CL-01-03), `FRAMING-0141` (CL-07-08), `FRAMING-0168` (CL-07-08), `P19-GPT-0088` (CL-07-07), `P21-0064` (CL-07-07) (+2 more); `P19-NEON-0034` vs `FRAMING-0053` (CL-07-07), `FRAMING-0068` (CL-01-03), `FRAMING-0141` (CL-07-08), `FRAMING-0168` (CL-07-08), `P19-GPT-0088` (CL-07-07), `P21-0064` (CL-07-07) (+2 more); `P19-NEON-0218` vs `FRAMING-0053` (CL-07-07), `FRAMING-0068` (CL-01-03), `FRAMING-0141` (CL-07-08), `FRAMING-0168` (CL-07-08), `P19-GPT-0088` (CL-07-07), `P21-0064` (CL-07-07) (+2 more)

### CL-07-07: Semantic Distinction: Period Lags, Solved Iteration & Clocked Delays

- **Summary:** Model map draws three fundamentally different visual structures for three different Excel mechanisms: period-row lags (e.g., Day 2 `H15` → Day 5 `C18`), within-calculation iterative circular enclosures (e.g., Reserve `B8`), and verified per-tick state delays (`z⁻¹`). Whenever file evidence cannot prove the mechanism, the map labels the cycle 'Unknown' and refuses to invent delay semantics.
- **Cards (7):** `FRAMING-0053`, `FRAMING-0140`, `P19-GPT-0088`, `P19-GPT-0096`, `P19-GPT-0262`, `P21-0064`, `P21-0071`
- **Sources:** FRAMING (2), P19-GPT (3), P21 (2)
- **Disposition:** `core-with-decision` — Make this three-way visual and semantic taxonomy (Period Lag vs Iterative Enclosure vs Verified Per-Tick State Delay + 'Unknown' fallback) a headline rule of the Model map chapter.
- **Decisions:** [D-03](decisions.md#d-03)
- **`same-as` groups (1):** `P19-GPT-0088` = `P19-GPT-0096` = `P19-GPT-0262` = `P21-0064`
- **Agreements:**
  - A circular reference is NEVER automatically a time delay.
  - Period-row lags (explicit prior-period references such as `H15→C18`) carry a period-labelled delay badge (`3-day lag` / `[t-1]`), visually distinct from circular iteration enclosures.
  - Within-calculation iterative solving (such as Stock's Reserve `B8`) is drawn inside an iteration enclosure exposing exact cells and settings.
  - When file evidence is insufficient to confirm a clocked delay or valid iteration setup, the UI labels the cycle 'Unknown', refuses to start an assumed clock contract, and offers source inspection.
- **Conflicts:**
  - *Cross-cluster `conflicts-with` links (resolved by D-03):* `FRAMING-0053` vs `P19-CARBON-0037` (CL-07-06), `P19-CARBON-0106` (CL-15-05), `P19-CARBON-0127` (CL-16-04), `P19-CARBON-0194` (CL-07-06), `P19-CARBON-0237` (CL-10-12), `P19-FABLE-0028` (CL-01-03) (+7 more); `P19-GPT-0088` vs `P19-CARBON-0037` (CL-07-06), `P19-CARBON-0106` (CL-15-05), `P19-CARBON-0127` (CL-16-04), `P19-CARBON-0194` (CL-07-06), `P19-CARBON-0237` (CL-10-12), `P19-FABLE-0028` (CL-01-03) (+7 more); `P21-0064` vs `P19-CARBON-0037` (CL-07-06), `P19-CARBON-0106` (CL-15-05), `P19-CARBON-0127` (CL-16-04), `P19-CARBON-0194` (CL-07-06), `P19-CARBON-0237` (CL-10-12), `P19-FABLE-0028` (CL-01-03) (+7 more)

### CL-07-08: Ticked Simulation Contract: RTD Clock, Macro Ticking & Excel Runtime Limits

- **Summary:** DNA Calc's example ticked models use an Excel-loadable RTD clock (`DnaCalc.Clock`) advancing immutable previous-tick snapshots (`Prev.*`) once per tick, while real Excel models ticked via a macro engine (value changes, volatile inputs, Calculate calls) are equally valid. Model map honestly respects Excel's runtime realities—including Application-scoped `MaxIterations` and `RTD.ThrottleInterval`—without pretending file formulas alone prove a guarded tick contract.
- **Cards (7):** `FRAMING-0111`, `FRAMING-0141`, `FRAMING-0142`, `FRAMING-0168`, `P21-0068`, `P21-0069`, `P21-0070`
- **Sources:** FRAMING (4), P21 (3)
- **Disposition:** `core-with-decision` — Document both supported ticking mechanisms per `DECISIONS_20261126 #3` (RTD clock for DNA Calc examples; macro/value-change/Calculate ticking for real Excel models), the immutable `Prev.*` snapshot pattern, and the Application-scoped `MaxIterations` / RTD throttle disclosures.
- **Decisions:** [D-03](decisions.md#d-03)
- **`same-as` groups (1):** `FRAMING-0141` = `FRAMING-0142`
- **Cross-cluster `same-as`:** `FRAMING-0142` = `P21-0080` (CL-08-07)
- **Agreements:**
  - Example ticked models use an Excel-loadable RTD clock (`=RTD("DnaCalc.Clock",,"Stock","tick")`) exposed as a source block with Run, Pause, Step, Rate, and Reset controls synchronized across Model map, Inspector Live, and App.
  - Real-world Excel models ticked by a macro engine (value changes, volatile inputs, `Calculate` calls) are first-class valid ticked models too (`DECISIONS_20261126 #3`).
  - The ticked Stock demonstration advances day, stock, and 3-stage pipeline exactly once per accepted tick from one immutable previous-tick snapshot (`Prev.*`), keeping pre-order position distinct from post-order stock+pipeline.
  - `Application.MaxIterations` is Application-scoped in Excel (not per-sheet), so 100-iteration period mode and 1-iteration ticked mode are alternative calculation profiles rather than simultaneous per-sheet limits.
- **Conflicts:**
  - **Can file evidence alone (RTD topic + MaxIterations=1 + self-reference) prove a guarded per-tick contract?** (`FRAMING-0141` vs `P21-0069`, `P21-0070`) → Resolved by **[D-03](decisions.md#d-03)**
  - *Cross-cluster `conflicts-with` links (resolved by D-03):* `FRAMING-0141` vs `P19-CARBON-0037` (CL-07-06), `P19-CARBON-0106` (CL-15-05), `P19-CARBON-0127` (CL-16-04), `P19-CARBON-0194` (CL-07-06), `P19-CARBON-0237` (CL-10-12), `P19-FABLE-0028` (CL-01-03) (+7 more); `FRAMING-0168` vs `P19-CARBON-0037` (CL-07-06), `P19-CARBON-0106` (CL-15-05), `P19-CARBON-0127` (CL-16-04), `P19-CARBON-0194` (CL-07-06), `P19-CARBON-0237` (CL-10-12), `P19-FABLE-0028` (CL-01-03) (+7 more); `P21-0069` vs `P19-CARBON-0037` (CL-07-06), `P19-CARBON-0106` (CL-15-05), `P19-CARBON-0127` (CL-16-04), `P19-CARBON-0194` (CL-07-06), `P19-CARBON-0237` (CL-10-12), `P19-FABLE-0028` (CL-01-03) (+7 more); `P21-0070` vs `P19-CARBON-0037` (CL-07-06), `P19-CARBON-0106` (CL-15-05), `P19-CARBON-0127` (CL-16-04), `P19-CARBON-0194` (CL-07-06), `P19-CARBON-0237` (CL-10-12), `P19-FABLE-0028` (CL-01-03) (+7 more)

### CL-07-09: Calculation Chips vs Directors, Preview Harmonisation & History Plotting

- **Summary:** Harmonises the `names-in-motion` and `watershed` HTML previews into Strand's honest token system: keeping hierarchical drill-down, live gauges/sparklines, evaluation-order ripples, `IFS`/`SWITCH` state badges, and parameter rails, while replacing proprietary 'Director' jargon with plain Excel-terms calculation chips and routing tick-history plotting cleanly to Inspector/Watch rather than writing synthetic history into the workbook.
- **Cards (11):** `P19-CARBON-0017`, `P19-CARBON-0038`, `P19-CARBON-0061`, `P19-CARBON-0062`, `P19-FABLE-0080`, `P19-FABLE-0159`, `P19-NEON-0019`, `P19-NEON-0054`, `P19-NEON-0055`, `P19-NEON-0116`, `P19-NEON-0217`
- **Sources:** P19-CARBON (4), P19-FABLE (2), P19-NEON (5)
- **Disposition:** `core-with-decision` — Keep Strand palette harmonisation, `IFS`/`SWITCH` state badges, live gauges, and parameter rail; replace 'Director' with Excel-terms calculation chips and unify plotting in Inspector › Live.
- **Decisions:** [D-03](decisions.md#d-03), [D-12](decisions.md#d-12), [D-63](decisions.md#d-63)
- **`same-as` groups (3):** `P19-CARBON-0017` = `P19-NEON-0019`; `P19-CARBON-0038` = `P19-CARBON-0062` = `P19-NEON-0055`; `P19-CARBON-0061` = `P19-NEON-0054` = `P19-NEON-0217`
- **Agreements:**
  - Retain the strongest elements from `names-in-motion.html` and `watershed.html`: hierarchical boxes with breadcrumbs, live port values, Presented/Excel toggle, evaluation-order ripple, `IFS`/`SWITCH` compact state badges, and value-bearing sparklines/gauges.
  - Harmonise all colours into Strand's palette (amber recalc pulses, signal orange RTD/volatile markers, teal iteration enclosures, red only for illegal cycles with Iteration OFF) and cut purely decorative motion (spinning turbines/waves).
  - Transient tick-history plots live strictly in view/session state and NEVER write synthetic history arrays into the `.xlsx` file.
- **Conflicts:**
  - **'Director' terminology & playback bar vs Excel-terms calculation chips** (`P19-CARBON-0038`, `P19-CARBON-0061`, `P19-CARBON-0062`, `P19-NEON-0054`, `P19-NEON-0055`, `P19-NEON-0217` vs `P19-FABLE-0159`) → Resolved by **[D-63](decisions.md#d-63)**
  - **Front-end auto-F9 ticking default vs RTD clock / macro-ticked playback** (`P19-FABLE-0080`, `P19-NEON-0116` vs `P19-CARBON-0017`, `P19-FABLE-0159`) → Resolved by **[D-03](decisions.md#d-03)**
  - **Where live tick history plots live: inline map History Strip vs Inspector Live / Watch panel** (`P19-CARBON-0062`, `P19-NEON-0055` vs `P19-FABLE-0080`, `P19-FABLE-0159`) → Resolved by **[D-63](decisions.md#d-63)**
  - *Cross-cluster `conflicts-with` links (resolved by D-03, D-12, D-63):* `P19-CARBON-0061` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28); `P19-FABLE-0080` vs `FRAMING-0053` (CL-07-07), `FRAMING-0068` (CL-01-03), `FRAMING-0141` (CL-07-08), `FRAMING-0168` (CL-07-08), `P19-GPT-0088` (CL-07-07), `P21-0064` (CL-07-07) (+2 more); `P19-NEON-0116` vs `FRAMING-0053` (CL-07-07), `FRAMING-0068` (CL-01-03), `FRAMING-0141` (CL-07-08), `FRAMING-0168` (CL-07-08), `P19-GPT-0088` (CL-07-07), `P21-0064` (CL-07-07) (+2 more)

### CL-07-10: Scale Boundaries, Semantic Zoom, Formula-Run Folding & Bounded Sheet Excerpts

- **Summary:** Model map stays fast and honest on massive workbooks by opening at a bounded zoom (≤60 boxes), maintaining constant readable text size across sheet/block/cell semantic zoom, collapsing identical sheets and 100k+ row R1C1 formula-pattern runs with 100% coverage accounting, and offering optional bounded geometry-preserving sheet excerpts inside the map when exact cell coordinates matter.
- **Cards (5):** `P19-FABLE-0046`, `P19-FABLE-0047`, `P19-FABLE-0154`, `P19-NEON-0027`, `P21-0075`
- **Sources:** P19-FABLE (3), P19-NEON (1), P21 (1)
- **Disposition:** `core-with-decision` — Combine Fable's sheet-zoom accounting/stacks/hidden-sheet peek (`M1.4`), Neon's L2 R1C1 formula-run Scale Notice Banner (`M-03`), and P21's opt-in bounded sheet excerpt (`M05`) with changed-binding fingerprint protection.
- **Decisions:** [D-12](decisions.md#d-12), [D-64](decisions.md#d-64)
- **`same-as` groups:** None within cluster
- **Agreements:**
  - Never drop sheets, hidden/very-hidden sheets, unread sheets, or cells silently; every view zoom displays an exact accounting footer (`40 of 40 sheets accounted for`, `12/12 runs · 0 cells omitted`).
  - Semantic zoom keeps text legible at constant size (never shrinking node text into unreadable pixels), starting at the coarsest level with ≤60 boxes.
  - Missing external links display in signal orange with dashed wires noting 'values as saved'; hidden sheets can be peeked non-destructively ('Hidden in the file. You are looking, not unhiding').
  - Surfaced interior formula errors (such as `#DIV/0!` inside a folded 8,760-row run) jump directly to Classic, and scale summaries export as Markdown.
- **Conflicts:**
  - **Showing raw cells on large sheets inside Model map: strict refusal/L2 run folding vs bounded geometry-preserving sheet excerpt** (`P19-FABLE-0047`, `P19-FABLE-0154`, `P19-NEON-0027` vs `P21-0075`) → Resolved by **[D-64](decisions.md#d-64)**
  - *Cross-cluster `conflicts-with` links (resolved by D-12, D-64):* `P19-FABLE-0154` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28)

### CL-07-11: Controls, Dependency-Explanation Ripple, Keyboard & Accessibility

- **Summary:** Model map is fully operable without a mouse (spatial arrows, `Ctrl+[`/`]` along wires, Enter/Esc drill, `W` to wire, full ARIA/text equivalents) and supports standard Excel Form Control sliders that commit once on release. Post-edit value motion along wires is brief (140 ms amber ripple), strictly labelled as 'dependency explanation', and degrades to instant border highlights under `prefers-reduced-motion`.
- **Cards (5):** `P19-FABLE-0079`, `P19-FABLE-0084`, `P19-FABLE-0465`, `P19-NEON-0219`, `P19-NEON-0220`
- **Sources:** P19-FABLE (3), P19-NEON (2)
- **Disposition:** `core` — Standardize the commit-on-release Form Control slider rule, full keyboard/ARIA graph contract, dual image+text clipboard copy, and reduced-motion-safe 140 ms dependency-explanation ripple.
- **`same-as` groups (1):** `P19-FABLE-0084` = `P19-FABLE-0465` = `P19-NEON-0220`
- **Agreements:**
  - Adding a slider on a box inserts a standard Excel Form Control ScrollBar linked to the box's cell; dragging shows a ghost value and commits once on release so iterative/ticked models do not fire dozens of unintended ticks.
  - Every box, composite, port, and wire action is keyboard-operable (spatial arrows, `Ctrl+[` / `Ctrl+]` along wires, Enter/Esc drill, `W` to wire) with screen-reader announcements listing name, role, value, inputs (including last-pass reads), and dependents.
  - Copying a frame (`Ctrl+C`) places both a presentation image and plain `name = formula` text on the clipboard.
  - Topological recalc ripple lasts 140 ms in Strand amber, is strictly labelled 'dependency explanation' (never implying asynchronous signal travel time), and replaces motion with instant border highlights under `prefers-reduced-motion`.
- **Conflicts:** None

### CL-07-12: Engineering Schematic Printing & Export

- **Summary:** Model map prints as a clean vector Engineering Schematic Sheet with a Strand title block (workbook, active scope, calculation/iteration settings, timestamp, legend, and exact 'x of y accounted for' coverage stamp), supporting single-page fit or poster-tiled output with cross-page wire tags and an optional box/formula schedule appendix.
- **Cards (3):** `P19-CARBON-0097`, `P19-FABLE-0200`, `P19-NEON-0089`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-NEON (1)
- **Disposition:** `core` — Consolidate all three proposals into one unified Engineering Schematic Print specification sharing the shell's print pipeline.
- **`same-as` groups (1):** `P19-CARBON-0097` = `P19-FABLE-0200` = `P19-NEON-0089`
- **Agreements:**
  - Prints the active map scope as a vector schematic on one page or tiled across multiple pages with cross-page wire tags (`──► [B3]`).
  - Includes a title block and footer disclosing workbook, calculation/iteration settings, timestamp, legend, and exact coverage accounting (including L2 Formula Runs summary on folded sheets).
  - Offers an optional appendix listing defined names, composite breakdowns, and backing Excel formulas.
- **Conflicts:** None

## VIII. App

### CL-08-01: App View Purpose, Forms-Maker Scope & Entry/Exit Ergonomics

- **Summary:** App is the calm, consumer-facing forms-maker view (also housing reader-style presentation per DECISIONS_20261126 #6): it presents an Excel workbook as a clean form of inputs, results, charts, and scenario controls bound 1:1 to real cells via `customXml`, hides spreadsheet mechanics by default, and always keeps a one-click escape hatch (`View as ▾`) back to Classic and the other views.
- **Cards (6):** `FRAMING-0011`, `P19-CARBON-0063`, `P19-FABLE-0086`, `P19-FABLE-0095`, `P19-FABLE-0155`, `P19-NEON-0221`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-FABLE (3), P19-NEON (1)
- **Disposition:** `core-with-decision` — Frame App as both the operational calculator form and the easy forms-maker / reader-style presentation surface (`DECISIONS_20261126 #6`), anchored on `customXml` 1:1 cell bindings and reached via `Alt+Q` / `Alt+Shift+5` (preserving `Ctrl+Alt+5` for Excel floating shapes per D-12).
- **Decisions:** [D-12](decisions.md#d-12)
- **`same-as` groups (2):** `FRAMING-0011` = `P19-NEON-0221`; `P19-FABLE-0086` = `P19-FABLE-0095`
- **Agreements:**
  - A workbook declaring `preferredView='app'` opens directly in App from saved values (<1 s), shows plain-language recalculation status ('Checking the numbers' -> 'Up to date'), and hides the grid and formula bar by default.
  - Every App control, KPI tile, chart, and schedule binds 1:1 to real workbook addresses via declarative `customXml` metadata—never hidden calculation logic.
  - An explicit `View as ▾` escape hatch (plus keyboard access) keeps Classic, Notebook, Inspector, and Model map one step away; builders enter layout editing explicitly (`Edit App Layout`).
  - Per DECISIONS_20261126 #6, reader-style presentation belongs here in App (the easy forms-maker view), not as a separate read mode in Notebook.
- **Conflicts:**
  - **View-switching keyboard shortcut for App (`Ctrl+5` vs `Alt+Shift+5` vs `Ctrl+Alt+5`)** (`P19-CARBON-0063` vs `P19-FABLE-0155`) → Resolved by **[D-12](decisions.md#d-12)**
  - *Cross-cluster `conflicts-with` links (resolved by D-12):* `P19-CARBON-0063` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28); `P19-FABLE-0155` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28)

### CL-08-02: Auto-Drafted vs Author-Confirmed App Publication & Binding Integrity

- **Summary:** Unstructured or messy workbooks like `Inherited` are never auto-published as an App; an author must confirm the writable inputs, outputs, and runtime contract first. On clean, well-structured workbooks with unambiguous Excel signals (unlocked cells, validation rules, control links), App may offer an explicitly badged, editable draft ('Drafted from the file · Edit page…') while broken bindings or locked inputs always surface plain explanations matching Classic.
- **Cards (6):** `P19-FABLE-0148`, `P19-FABLE-0467`, `P19-GPT-0044`, `P19-GPT-0266`, `P19-GPT-0298`, `P19-NEON-0056`
- **Sources:** P19-FABLE (2), P19-GPT (3), P19-NEON (1)
- **Disposition:** `core-with-decision` — Codify the two-tier publication rule: authored `customXml` required for `preferredView='app'`, certain-evidence-only `'Drafted from file'` allowed on explicit user switch, and honest decline on `Inherited`.
- **Decisions:** [D-65](decisions.md#d-65)
- **`same-as` groups (2):** `P19-FABLE-0148` = `P19-FABLE-0467`; `P19-GPT-0044` = `P19-GPT-0298`
- **Cross-cluster `same-as`:** `P19-GPT-0044` = `P19-GPT-0061` (CL-11-11)
- **Agreements:**
  - Messy, unannotated workbooks (such as `Inherited`) are honestly declined for automatic App generation until an author confirms an explicit input/output contract.
  - App strictly mirrors Classic's protection and runtime rules: a locked input, unavailable macro, or broken cell binding shows a clear explanation and route forward, never a silent substitute.
  - When an App page is drafted rather than author-published, that provenance is visibly disclosed alongside an 'Edit page…' action.
- **Conflicts:**
  - **Auto-drafting an App page from Excel signals vs requiring an authored layout before rendering App** (`P19-FABLE-0148`, `P19-FABLE-0467`, `P19-NEON-0056` vs `P19-GPT-0044`, `P19-GPT-0298`) → Resolved by **[D-65](decisions.md#d-65)**

### CL-08-03: Live Recalculation Default vs Author-Chosen Batched 'Apply Inputs'

- **Summary:** App inputs recalculate outputs, charts, and schedules live by default—matching spreadsheet immediacy, revealing bound source cells on focus, and undoing per gesture. Batched input ('Apply inputs') is supported as an explicit, named author option (for slow models or multi-parameter review) with its reason visibly stated beside the controls.
- **Cards (6):** `FRAMING-0137`, `P19-FABLE-0087`, `P19-GPT-0023`, `P19-GPT-0264`, `P21-0076`, `P21-0078`
- **Sources:** FRAMING (1), P19-FABLE (1), P19-GPT (2), P21 (2)
- **Disposition:** `core-with-decision` — Specify Live as default and Batched as an author-declared option with visible reason, atomic Apply Undo, Discard/Esc semantics, and unapplied-draft recovery.
- **Decisions:** [D-66](decisions.md#d-66)
- **`same-as` groups (1):** `FRAMING-0137` = `P21-0076` = `P21-0078`
- **Agreements:**
  - Focusing an input reveals 'Model change' and its bound source cell before typing; outputs keep full internal precision while displaying formatted values (with unrounded residuals inspectable).
  - In batched mode, pending edits are marked visually and textually as drafts without relabelling applied outputs ('Previous valid inputs'), Apply commits all valid edits atomically as one Undo step, Discard clears the pending set, Esc reverts the focused field, and recovered drafts remain unapplied after a restart.
- **Conflicts:**
  - **Default App input commit mode: live recalculation vs batched 'Apply inputs'** (`FRAMING-0137`, `P19-FABLE-0087`, `P21-0076`, `P21-0078` vs `P19-GPT-0023`, `P19-GPT-0264`) → Resolved by **[D-66](decisions.md#d-66)**

### CL-08-04: Inline Validation Refusal, Error Honesty & Invalid-Draft Handling

- **Summary:** App enforces the workbook's real Excel Data Validation rules inline at the field without jarring modal popups: invalid entries stay visible as inline drafts, write nothing to the workbook, label the remaining outputs 'Previous valid inputs', and offer one-key revert (`Esc`) or likely-fix suggestions. Genuine formula errors (`#NUM!`, `#DIV/0!`) are displayed immediately on affected tiles with red ticks, plain-language trace causes, and one-click Undo—never blanks or stale numbers.
- **Cards (6):** `P19-FABLE-0088`, `P19-FABLE-0090`, `P19-FABLE-0468`, `P19-FABLE-0470`, `P19-NEON-0222`, `P21-0077`
- **Sources:** P19-FABLE (4), P19-NEON (1), P21 (1)
- **Disposition:** `core` — Full cross-source consensus. Keep inline Stop validation, 'Previous valid inputs' badge for invalid drafts, and trace-backed error honesty on output tiles.
- **`same-as` groups (2):** `P19-FABLE-0088` = `P19-FABLE-0468` = `P19-NEON-0222` = `P21-0077`; `P19-FABLE-0090` = `P19-FABLE-0470`
- **Agreements:**
  - Tab visits editable inputs only; out-of-range edits are refused inline using the workbook's exact Excel Data Validation rule and message (no modal alert popup).
  - Invalid entries never mutate the bound cell; they remain visible as an inline draft with `Esc` revert (and optional smart fix suggestion like `6.5%` for `65`), while displayed outputs explicitly state 'Previous valid inputs'.
  - When a valid input produces an Excel calculation error (e.g., `#NUM!`), every affected output tile displays the error code, red tick, plain-language root cause from the dependency trace, and 'Undo my last change'—never a blank or previous number.
- **Conflicts:** None

### CL-08-05: Formula Card Protection, Goal Seek & Source Provenance Peek

- **Summary:** Calculated output tiles in App cannot be accidentally overwritten with literal values: typing on a formula outcome refuses overwrite cleanly and offers a safe single-input Goal Seek action or switching to Classic. Resting on or focusing an output tile reveals its provenance summary, exact cell address, and one-key jump to the underlying cells, while Excel Worksheet Protection remains an explicit opt-in export option.
- **Cards (5):** `P19-CARBON-0040`, `P19-CARBON-0195`, `P19-CARBON-0228`, `P19-FABLE-0091`, `P19-NEON-0115`
- **Sources:** P19-CARBON (3), P19-FABLE (1), P19-NEON (1)
- **Disposition:** `core-with-decision` — Include provenance peek + UI-level formula protection in V1, opt-in Excel sheet protection on export, and explicit single-input Goal Seek (`C-22`) as an additive capability.
- **Decisions:** [D-67](decisions.md#d-67)
- **`same-as` groups (1):** `P19-CARBON-0040` = `P19-CARBON-0195`
- **Agreements:**
  - Calculated outcome tiles refuse direct literal overwrite so consumer interactions never destroy formulas.
  - Hovering/focusing an output tile reveals its provenance summary, cell address (`Loan!C9`), and a direct 'See the cells' jump to Classic/split view.
  - App uses UI-level read-only guards on formula tiles by default rather than silently applying Excel Worksheet Protection to the `.xlsx`, while offering an opt-in 'Also protect formula cells in Excel' export toggle.
- **Conflicts:**
  - **Whether typing on a read-only output tile offers inline Goal Seek or purely refuses/routes to Classic** (`P19-CARBON-0040`, `P19-CARBON-0195`, `P19-CARBON-0228` vs `P19-FABLE-0091`) → Resolved by **[D-67](decisions.md#d-67)**

### CL-08-06: Scenarios, Baseline Pinning, Comparison Deltas & Reset

- **Summary:** App supports two cleanly separated comparison layers: (1) lightweight in-session baseline comparison ('Compare with opening / last applied / pinned baseline') showing directional deltas (`▼`/`▲` + semantic text, never colour alone) and dual-curve chart overlays without mutating the workbook, and (2) reusable named scenarios persisted directly to Excel's native Scenario Manager via an explicit, undoable 'Save as scenario…' preview.
- **Cards (9):** `P19-CARBON-0041`, `P19-CARBON-0064`, `P19-CARBON-0196`, `P19-FABLE-0089`, `P19-FABLE-0469`, `P19-FABLE-0471`, `P19-GPT-0265`, `P19-NEON-0036`, `P19-NEON-0223`
- **Sources:** P19-CARBON (3), P19-FABLE (3), P19-GPT (1), P19-NEON (2)
- **Disposition:** `core-with-decision` — Standardize native Excel Scenario Manager for saved scenarios, transient Pin/Last-Applied Baseline for ad-hoc deltas, dual-curve textured chart overlays, and single-step Undo Reset.
- **Decisions:** [D-68](decisions.md#d-68)
- **`same-as` groups (3):** `P19-CARBON-0041` = `P19-CARBON-0064` = `P19-CARBON-0196`; `P19-FABLE-0089` = `P19-FABLE-0469`; `P19-NEON-0036` = `P19-NEON-0223`
- **Agreements:**
  - Directional deltas across KPI cards and charts always pair symbols (`▼`/`▲`) and text with colour (never colour alone) and announce updates to screen readers.
  - Live workbook charts overlay baseline vs active scenario curves cleanly (using distinct line styles/textures as well as tones).
  - Reset restores all inputs to their opening/base state as a single undoable step (`R-59`).
  - Recorded comparisons are explicitly distinguished from saved Excel scenarios (`R-35`).
- **Conflicts:**
  - **Scenario storage: native Excel Scenario Manager vs in-memory Pin Baseline / customXml presets** (`P19-FABLE-0089`, `P19-FABLE-0469`, `P19-GPT-0265` vs `P19-CARBON-0041`, `P19-CARBON-0196`, `P19-NEON-0036`) → Resolved by **[D-68](decisions.md#d-68)**

### CL-08-07: Long Schedules, Ticked Operator Panel, Accessibility & Save-Close UX

- **Summary:** App handles long schedules gracefully without flooding the screen, hosts a synchronized operator console for ticked simulation workbooks (Run/Pause/Step/Rate/Stop/Reset), meets strong accessibility and future-touch ergonomics (labelled form, live regions, 200% reflow, generous control targets), and summarises modifications in plain words on close.
- **Cards (5):** `P19-FABLE-0093`, `P19-FABLE-0094`, `P19-FABLE-0472`, `P19-NEON-0224`, `P21-0080`
- **Sources:** P19-FABLE (3), P19-NEON (1), P21 (1)
- **Disposition:** `core-with-decision` — Include plain-language close summary, accessibility/200%-reflow/36px targets, windowed schedule + optional badged rollup toggle, and the synchronized ticked-simulation operator panel (`A02`).
- **Decisions:** [D-69](decisions.md#d-69)
- **`same-as` groups (1):** `P19-FABLE-0093` = `P19-FABLE-0472`
- **Cross-cluster `same-as`:** `P21-0080` = `FRAMING-0142` (CL-07-08)
- **Agreements:**
  - Closing a modified App workbook summarises changes in plain consumer language ('You added 1 scenario and changed 2 inputs') with Save, Save a copy, and Don't save.
  - App renders as a labelled semantic form with `aria-live` output announcements and clean two-to-one-column reflow at 200% zoom.
  - For ticked simulation workbooks (`Stock (ticked)`), App exposes Run, Pause, Step, Rate, Reorder point, Stop condition, and Reset synchronized with Model map and Inspector, plotting only observed ticks and pausing at boundaries until explicit Reset.
- **Conflicts:**
  - **Long schedule presentation in App: auto-grouped annual rollup vs windowed row frame** (`P19-NEON-0224` vs `P19-FABLE-0094`) → Resolved by **[D-69](decisions.md#d-69)**

### CL-08-08: Executive Decision Brief Printing & Frozen Print Snapshot

- **Summary:** Printing from App (`Ctrl+P`) captures a frozen print snapshot of the applied case and formats a clean, daylight-paper 1-page Executive Decision Brief (inputs, KPI deltas, comparison column/table, vector chart, and provenance footer with file, scenario, snapshot ID, and timestamp), letting users opt in to append annual or full schedules with live page-count disclosure.
- **Cards (6):** `P19-CARBON-0098`, `P19-FABLE-0092`, `P19-FABLE-0201`, `P19-NEON-0037`, `P19-NEON-0090`, `P21-0079`
- **Sources:** P19-CARBON (1), P19-FABLE (2), P19-NEON (2), P21 (1)
- **Disposition:** `core-with-decision` — Standardize the frozen-snapshot 1-page default brief, explicit schedule page-cost toggles, daylight paper styling, and provenance footer.
- **Decisions:** [D-69](decisions.md#d-69)
- **`same-as` groups (2):** `P19-FABLE-0092` = `P19-FABLE-0201` = `P21-0079`; `P19-CARBON-0098` = `P19-NEON-0037` = `P19-NEON-0090`
- **Cross-cluster `same-as`:** `P19-CARBON-0098` = `P19-NEON-0091` (CL-09-12)
- **Agreements:**
  - Print preview renders in light daylight paper regardless of screen theme and never drops long schedule rows silently.
  - Appending schedule details (first year / annual rollup / full 360-row schedule) is an explicit opt-in showing exact page cost before printing.
  - Every printed page carries a provenance footer tying output to file name, active scenario, timestamp, and snapshot ID (shown in the print footer only, never cluttering the working App).
- **Conflicts:**
  - **Default App print length: 1-page summary vs 2-page brief including annual schedule** (`P19-FABLE-0092`, `P19-FABLE-0201`, `P21-0079` vs `P19-CARBON-0098`, `P19-NEON-0037`, `P19-NEON-0090`) → Resolved by **[D-69](decisions.md#d-69)**

## IX. Formula Explorer

### CL-09-01: Two-Level Host Layering, Skin 0 Identity & Early Standalone Teaser

- **Summary:** Formula Explorer (casually `OneCalc` internally, publicly `DNA Calc - Formula Explorer` standalone) is Skin 0 built strictly on the Level-0 formula host (`CalcFml`, `CalcFunc`, `calc-value`, formatting, conditional formatting, add-in UDFs—NEVER `calc-engine`). It runs both embedded inside DNA Calc (receiving precedent snapshots from Level 1) and as a stripped standalone build carrying Strand's dot-long `[•━]` identity mark, shipping early as a public teaser per Owner Ruling #2.
- **Cards (9):** `FRAMING-0025`, `FRAMING-0026`, `FRAMING-0040`, `FRAMING-0042`, `FRAMING-0043`, `FRAMING-0051`, `FRAMING-0167`, `P19-CARBON-0020`, `P19-NEON-0106`
- **Sources:** FRAMING (7), P19-CARBON (1), P19-NEON (1)
- **Disposition:** `core-with-decision` — Lead Chapter IX with the Level-0 / Level-1 boundary, early standalone teaser milestone (`DECISIONS_20261126 #2`), dot-long `[•━]` mark, and mechanical zero-engine dependency ceiling.
- **Decisions:** [D-02](decisions.md#d-02), [D-81](decisions.md#d-81)
- **`same-as` groups (2):** `FRAMING-0025` = `FRAMING-0026` = `FRAMING-0051`; `FRAMING-0040` = `FRAMING-0042` = `FRAMING-0043`
- **Cross-cluster `same-as`:** `FRAMING-0167` = `P19-CARBON-0123` (CL-16-03)
- **Agreements:**
  - Level-0 formula host encapsulates formula parsing/evaluation, function catalogue/help, number formatting, conditional formatting, and add-in UDF registration with a mechanical dependency ceiling that NEVER includes `calc-engine`.
  - Level-1 workbook host builds on Level 0 without duplicating it; DNA Calc's formula bar, help, and formatting reuse the exact same Level-0 services.
  - The legacy separate OneCalc shell and custom Link messages are retired in favour of stripping DNA Calc to Skin 0 + Level 0, keeping `OneCalc` only as an internal casual name.
  - Standalone `DNA Calc - Formula Explorer` wears Strand's dot-long `[•━]` identity mark so workbookless sessions are immediately distinct from file-backed workbooks.
- **Conflicts:**
  - **Release timing of standalone `DNA Calc - Formula Explorer`: early teaser vs late packaging milestone** (`FRAMING-0167` vs `P19-NEON-0106`) → Resolved by **[D-02](decisions.md#d-02)**
  - *Cross-cluster `conflicts-with` links (resolved by D-02, D-81):* `FRAMING-0051` vs `P19-GPT-0101` (CL-15-10), `P19-NEON-0124` (CL-16-03); `FRAMING-0167` vs `P19-GPT-0101` (CL-15-10), `P19-NEON-0124` (CL-16-03); `P19-NEON-0106` vs `P19-CARBON-0123` (CL-16-03)

### CL-09-02: Dual-Form Workbench Parity, Core Editor & AST Step Evaluation

- **Summary:** Standalone and embedded Formula Explorer share one identical workbench—editor, live AST sub-expression tree, function catalogue, number/conditional format labs, and add-in library—built on carried-forward native browser input/caret discipline and bounded typed previews for large spilled arrays. Every AST node highlights its exact formula span and reports intermediate value, type, coercion, timing, and first error origin.
- **Cards (11):** `FRAMING-0075`, `FRAMING-0076`, `P19-CARBON-0065`, `P19-CARBON-0213`, `P19-CARBON-0214`, `P19-CARBON-0215`, `P19-FABLE-0481`, `P19-NEON-0057`, `P19-NEON-0225`, `P19-NEON-0247`, `P19-NEON-0248`
- **Sources:** FRAMING (2), P19-CARBON (4), P19-FABLE (1), P19-NEON (4)
- **Disposition:** `core` — Standardize the 5 shared facets/tabs (Steps/Eval Tree, Functions, Number Format, Conditional Format, Add-ins & Types), AST span highlighting, and legacy OneCalc editor/large-array guardrails.
- **`same-as` groups (3):** `P19-CARBON-0213` = `P19-NEON-0248`; `P19-CARBON-0214` = `P19-CARBON-0215` = `P19-NEON-0247`; `P19-CARBON-0065` = `P19-NEON-0057` = `P19-NEON-0225`
- **Agreements:**
  - One shared UI workbench serves both standalone and embedded Formula Explorer verbatim.
  - Formula editing preserves native browser IME/caret/revision fidelity (`FRAMING-0075`) and bounded typed previews with visible truncation notices and predictable copy semantics for huge spilled arrays (`FRAMING-0076`).
  - Selecting or arrowing through any AST node highlights its exact character span in the formula and shows its intermediate value, Excel type/coercions, timing, and first error source (`R-69`).
  - Side-effect-free sub-expression evaluation (`F9` partial eval, scratchpad `LET`/`LAMBDA`, trial inputs) powers both Formula Explorer and the shared formula bar.
- **Conflicts:** None

### CL-09-03: Standalone Sandbox Scope, Caller-Context Honesty & Local Test Inputs

- **Summary:** Standalone `DNA Calc - Formula Explorer` launches fast (<500 ms target, ideal ≤50 ms) with zero workbook chrome, no sheet tabs, and no hidden temporary workbook. It supports explicit local named test inputs (scalars, arrays, errors) and multi-line `LET`/`LAMBDA` formatting for practical standalone testing, while strictly refusing caller- or workbook-dependent functions (`CELL`, `ROW`, `DNA.CALLERADDRESS()`) as an honest Level-0 tool state with a direct route to open in DNA Calc.
- **Cards (9):** `P19-CARBON-0042`, `P19-CARBON-0067`, `P19-CARBON-0198`, `P19-FABLE-0096`, `P19-FABLE-0150`, `P19-GPT-0024`, `P19-GPT-0269`, `P19-NEON-0038`, `P21-0082`
- **Sources:** P19-CARBON (3), P19-FABLE (2), P19-GPT (2), P19-NEON (1), P21 (1)
- **Disposition:** `core-with-decision` — Enforce strict zero-workbook / no-fabricated-caller refusal (`P21-0082`), keep single-formula + `LET`/`LAMBDA` + badged Local Test Inputs shelf, and route multi-step chained computation to Notebook.
- **Decisions:** [D-70](decisions.md#d-70)
- **`same-as` groups (2):** `P19-GPT-0024` = `P19-GPT-0269` = `P21-0082`; `P19-CARBON-0042` = `P19-CARBON-0067` = `P19-CARBON-0198` = `P19-NEON-0038`
- **Agreements:**
  - Standalone Formula Explorer has no workbook, no sheet tabs, and no secret background workbook.
  - Supports multi-line `LET`/`LAMBDA` formatting (`Shift+Alt+F`), dual raw IEEE-754 + formatted readouts, and instant launch.
  - Unavailable native add-ins or workbook-only capabilities are refused plainly while keeping the user's draft intact.
- **Conflicts:**
  - **Handling caller/sheet-context functions (`ROW()`, `CELL()`, `DNA.CALLERADDRESS()`) in standalone Skin 0** (`P19-GPT-0024`, `P19-GPT-0269`, `P21-0082` vs `P19-FABLE-0150`) → Resolved by **[D-70](decisions.md#d-70)**
  - **Standalone local named test inputs & multi-line scratchpad vs literal-arguments-only evaluation** (`P19-CARBON-0042`, `P19-CARBON-0067`, `P19-FABLE-0150`, `P19-NEON-0038` vs `P19-GPT-0024`, `P19-GPT-0269`, `P21-0082`) → Resolved by **[D-70](decisions.md#d-70)**
  - *Cross-cluster `conflicts-with` links (resolved by D-70):* `P19-FABLE-0150` vs `FRAMING-0108` (CL-07-01)

### CL-09-04: Docked In-Workbook Explorer: Precedent Snapshot, Sandbox Isolation & Apply

- **Summary:** Inside DNA Calc, Formula Explorer opens docked beside the active cell, seeded with a snapshot of that cell's live precedents (showing both formatted and raw stored values) and real caller context. Users edit trial formulas and override precedent values freely in an isolated sandbox ('Trying. Workbook unchanged.') until explicitly committing via `Apply to cell (Ctrl+Enter)` as one shared Undo step.
- **Cards (9):** `P19-CARBON-0035`, `P19-CARBON-0066`, `P19-CARBON-0197`, `P19-FABLE-0067`, `P19-FABLE-0103`, `P19-FABLE-0149`, `P19-FABLE-0156`, `P19-FABLE-0480`, `P21-0081`
- **Sources:** P19-CARBON (3), P19-FABLE (5), P21 (1)
- **Disposition:** `core-with-decision` — Lock in the docked precedent-snapshot sandbox, 'Trying. Workbook unchanged.' badge, `Alt+Shift+E` (+ `Shift+F3`/`Alt,M,V`) launch (per D-34), `Ctrl+Enter` trial evaluate, and explicit `[Apply to cell]` commit.
- **Decisions:** [D-34](decisions.md#d-34)
- **`same-as` groups (1):** `P19-CARBON-0066` = `P19-CARBON-0197` = `P19-FABLE-0067` = `P19-FABLE-0103` = `P19-FABLE-0149` = `P19-FABLE-0480`
- **Cross-cluster `same-as`:** `P19-FABLE-0156` = `P19-FABLE-0279` (CL-04-11)
- **Agreements:**
  - Opening Formula Explorer on a workbook cell seeds every precedent with its shown and stored value plus its real caller context.
  - Editing the formula or overriding precedent values in the 'Try' column evaluates in strict isolation ('Trying. Workbook unchanged.') and never mutates the workbook.
  - `Apply to cell` is the sole mutating action, writing one formula edit with one shared Undo step; `Esc` discards the trial cleanly.
  - Docked Explorer keeps the active cell (and spilled array, if any) visible and supports `F6` region cycling, `F9` partial eval, `F4` reference cycling, and `Shift+F3` / `Alt,M,V` Excel entry points.
- **Conflicts:**
  - **Primary keyboard shortcut for opening embedded Formula Explorer (`Ctrl+Shift+E` vs `Alt+Shift+E` vs `Ctrl+E`) and `Ctrl+Enter` meaning** (`P19-CARBON-0035`, `P19-CARBON-0197`, `P19-FABLE-0149`, `P19-FABLE-0156` vs `P21-0081`) → Resolved by **[D-34](decisions.md#d-34)**

### CL-09-05: Stale-Revision Guards, Downstream Preview Seam & Level-0 vs Level-1 Trial Scope

- **Summary:** Docked Formula Explorer records the source cell's revision on open and checks it before Apply—requiring refresh and review if the cell changed underneath. It cleanly separates fast Level-0 single-expression trial evaluation from optional Level-1 speculative downstream impact preview across the workbook Preview Seam, and never runs side-effecting providers silently during preview.
- **Cards (6):** `P19-GPT-0045`, `P19-GPT-0267`, `P19-GPT-0268`, `P19-GPT-0314`, `P19-NEON-0039`, `P19-NEON-0226`
- **Sources:** P19-GPT (4), P19-NEON (2)
- **Disposition:** `core-with-decision` — Mandate stale-revision checking before Apply (`R-38`), strict separation of Level-0 expression trial vs opt-in Level-1 downstream Preview Seam (`C-15`), and no silent execution of side-effecting providers.
- **Decisions:** [D-34](decisions.md#d-34), [D-71](decisions.md#d-71)
- **`same-as` groups (2):** `P19-GPT-0045` = `P19-GPT-0267` = `P19-GPT-0268`; `P19-NEON-0039` = `P19-NEON-0226`
- **Agreements:**
  - Docked Explorer captures source revision, real caller context, inputs, exact formula, and formatting on open (`R-37`).
  - Applying a trial checks the current source revision; if stale, Apply pauses and requires explicit refresh/review rather than silently clobbering concurrent edits (`R-38`).
  - Side-effecting macros/UDFs never execute silently during trial preview; Cancel leaves formula, format, and provider state untouched.
  - The docked drawer exposes WAI-ARIA tree semantics for the Eval Tree and supports popping out (`⤢`).
- **Conflicts:**
  - **Downstream workbook impact preview inside docked Formula Explorer vs strict Level-0 single-cell isolation** (`P19-NEON-0039`, `P19-NEON-0226` vs `P19-GPT-0045`, `P19-GPT-0314`) → Resolved by **[D-71](decisions.md#d-71)**
  - *Cross-cluster `conflicts-with` links (resolved by D-34, D-71):* `P19-NEON-0039` vs `P19-GPT-0204` (CL-04-27)

### CL-09-06: Function Browser & Honest Oracle Compatibility Evidence

- **Summary:** Formula Explorer includes a searchable function browser organised by family with signatures, argument help, runnable examples, host-availability tags, inline live `LET` inspection, and transparent per-function Excel-match and worst-ULP status (plus plain refusal reasons for unimplemented functions)—wired to real measured corpus/oracle evidence rather than illustrative placeholder numbers.
- **Cards (3):** `FRAMING-0160`, `P21-0084`, `P21-0127`
- **Sources:** FRAMING (1), P21 (2)
- **Disposition:** `core` — Make the Function Browser + real build-time oracle parity badges a signature trust feature of the early standalone Formula Explorer teaser (`DECISIONS_20261126 #2`).
- **`same-as` groups (1):** `FRAMING-0160` = `P21-0084`
- **Agreements:**
  - Functions are browsable by family and search, showing syntax, argument help, runnable examples, host-availability badges, and plain refusal reasons for unimplemented functions.
  - Each function displays its real DNA Calc Excel-parity record (matched calls / total calls and worst ULP) generated from real oracle runs.
  - Illustrative figures used in design mockups (e.g., `GAMMA 4,252/4,311 · 63 ulp`) are explicitly sample archive records (`P21-0127`); the shipping product must bind strictly to live build-time oracle receipts and never display unmeasured claims.
- **Conflicts:** None

### CL-09-07: Number Formatting & Conditional Formatting Playground

- **Summary:** The Formatting playground lets users test custom Excel number format codes, locales (`en-US`, `en-ZA`, `fr-FR`, `de-DE`), and conditional-format rules against scalar or spilled array results (e.g., `=SEQUENCE(6,3,-1250,250)`), keeping raw underlying values visibly separate from formatted display text, evaluating CF rules against underlying numbers before text formatting, and rendering CF cues with hatch + text labels as well as colour.
- **Cards (6):** `FRAMING-0161`, `P19-FABLE-0101`, `P19-FABLE-0482`, `P19-FABLE-0483`, `P19-GPT-0317`, `P21-0085`
- **Sources:** FRAMING (1), P19-FABLE (3), P19-GPT (1), P21 (1)
- **Disposition:** `core` — Full consensus across all rounds. Ship both Number Format and Conditional Format labs in the standalone teaser to exercise DNA Calc's formatting kernels directly.
- **`same-as` groups (2):** `P19-FABLE-0101` = `P19-FABLE-0482` = `P19-FABLE-0483`; `FRAMING-0161` = `P19-GPT-0317` = `P21-0085`
- **Agreements:**
  - Raw underlying values and formatted display strings are always shown visibly apart (`C-18`), preventing display formatting from being confused with stored numeric values.
  - Number-format testing previews custom format codes across live results plus standard positive, negative, zero, and text 4-section samples across selectable locales.
  - Conditional-format testing lists applicable rules in evaluation order, evaluates them against underlying numeric values BEFORE text formatting, shows exactly which rule fired and why, and uses hatch + text labels alongside colour.
  - Invalid or unsupported format drafts are rejected inline without replacing the applied format, and formatting edits participate in Undo.
- **Conflicts:** None

### CL-09-08: Expected-Value Test Cases, Error Assertions & Automated Boundary Probes

- **Summary:** Formula Explorer acts as a lightweight formula and UDF testbench: users can create, edit (with Undo), save, and run expected-value test cases (where expected Excel errors like `#NUM!` register as a passing 'Match' while still displaying `#NUM!` visibly) and run one-click automated boundary probes (zero, negative, empty, wrong type, error propagation, out-of-range) with per-call timings.
- **Cards (3):** `P19-FABLE-0099`, `P19-GPT-0271`, `P21-0083`
- **Sources:** P19-FABLE (1), P19-GPT (1), P21 (1)
- **Disposition:** `core` — Unify P21's saved test-case runner with Fable's/Carbon's one-click Boundary Probe Matrix and crash-resilient add-in probing.
- **`same-as` groups (1):** `P19-GPT-0271` = `P21-0083`
- **Agreements:**
  - Test cases store expected and observed values separately from number/conditional formatting (`108` and `R 108.00` share the same numeric result).
  - An expected Excel error (`#NUM!`) evaluates as a passing test ('Match') while keeping the error code visibly displayed—never hiding errors.
  - One-click 'Edge cases' / boundary probes test awkward inputs with per-call timings, survive crashing add-ins cleanly (`[!] add-in stopped`), and offer 'Copy as a test case'.
- **Conflicts:** None

### CL-09-09: Add-In Library, Developer Hot-Reload & Crash Isolation

- **Summary:** Formula Explorer provides a fast workbook-free add-in developer workbench: loading `.xll` or `.xlam` add-ins behind a clear trust card, listing registered functions, categories, argument metadata, and authoring remarks (e.g., missing argument descriptions), marking add-in formulas with `◊`, isolating add-in crashes without killing the host, and hot-reloading rebuilt add-ins without restarting.
- **Cards (7):** `FRAMING-0162`, `P19-FABLE-0097`, `P19-FABLE-0098`, `P19-FABLE-0100`, `P19-FABLE-0107`, `P19-FABLE-0484`, `P19-FABLE-0486`
- **Sources:** FRAMING (1), P19-FABLE (6)
- **Disposition:** `core` — Keep the developer-favourite workflow: trust card, `◊` marker, surrogate crash isolation, shadow-copy hot-reload (`Ctrl+Shift+R`), and honest browser-host refusal.
- **`same-as` groups (2):** `P19-FABLE-0097` = `FRAMING-0162`; `P19-FABLE-0100` = `P19-FABLE-0107` = `P19-FABLE-0484`
- **Agreements:**
  - Loading an add-in shows a single clear trust card (file path, build time, signature status, permission warning) with 'Load once', 'Always trust this folder', or 'Cancel'.
  - The Add-ins library lists loaded `.xll` and `.xlam` providers with registered functions, categories, argument descriptions, authoring remarks, and explicit Level-0 capability indicators.
  - Add-in functions appear in autocomplete and formula readouts marked `◊`, and saving notes what Excel will display if the add-in is absent.
  - Shadow-copied loading / file watching enables one-key in-place Reload without DLL locks or app restarts, automatically re-evaluating and marking changed results ('was: stopped'), while process isolation keeps the host intact if an add-in crashes.
  - In browser/WASM builds, the UI honestly states that native `.xll` add-ins cannot load.
- **Conflicts:** None

### CL-09-10: Provider Execution Policy, Side-Effect Guards & Native XLL Security Boundary

- **Summary:** Add-in execution obeys strict honesty and security boundaries: unknown or side-effecting providers never execute silently on every keystroke, missing or unloaded desktop providers never silently substitute another implementation or zero, and native `.xll` binaries are explicitly disclosed as NOT contained by the VBA Guarded policy—requiring an independently chosen desktop execution host and trust decision.
- **Cards (8):** `P19-GPT-0092`, `P19-GPT-0270`, `P19-GPT-0272`, `P19-GPT-0315`, `P19-GPT-0316`, `P19-NEON-0227`, `P21-0086`, `P21-0087`
- **Sources:** P19-GPT (5), P19-NEON (1), P21 (2)
- **Disposition:** `core-with-decision` — Enforce explicit Evaluate for unknown/side-effecting providers, zero silent fallback on missing hosts, and honest native-code trust language (never calling native `.xll` a 'sandbox').
- **Decisions:** [D-72](decisions.md#d-72)
- **`same-as` groups (2):** `P19-GPT-0092` = `P19-GPT-0270` = `P21-0086`; `P19-GPT-0272` = `P19-GPT-0315`
- **Agreements:**
  - Every add-in provider exposes its actual version, host requirement, permission state, and capability metadata (`C-16`, `C-17`).
  - Unloading a provider or lacking its required desktop host refuses execution cleanly while preserving the formula draft and any cached value—never substituting another implementation or returning zero.
  - Unknown or side-effecting providers never run on every editor keystroke; they require explicit Evaluate, a safe preview capability, or plain refusal, and cancellation never falsely claims to undo external side effects.
  - Native `.xll` code is explicitly NOT contained by the VBA Guarded policy (`P21-0087`); running a native `.xll` always requires an independent host/trust authorization.
- **Conflicts:**
  - **Unsigned local add-in consent UX: lightweight Dev Sandbox banner vs explicit native-code trust policy** (`P19-NEON-0227` vs `P21-0087`, `P19-GPT-0270`) → Resolved by **[D-72](decisions.md#d-72)**

### CL-09-11: Formula Lens: Readable Display Presentation & Explicit Refactor-to-LET

- **Summary:** Formula Lens (`Alt+L` in every view) is a strictly non-mutating readability overlay over real Excel formulas: it substitutes defined names and visibly distinct sheet-inferred labels (`‹…›`), applies structural line breaks, and optionally expands repeated sub-expressions as labelled read-only steps—never altering stored formulas unless the user explicitly invokes a previewed 'Refactor cell to real LET' or 'Make it a name…' edit.
- **Cards (11):** `P19-CARBON-0030`, `P19-CARBON-0068`, `P19-CARBON-0113`, `P19-CARBON-0181`, `P19-FABLE-0056`, `P19-FABLE-0158`, `P19-FABLE-0222`, `P19-FABLE-0485`, `P19-NEON-0058`, `P19-NEON-0105`, `P19-NEON-0228`
- **Sources:** P19-CARBON (4), P19-FABLE (4), P19-NEON (3)
- **Disposition:** `core-with-decision` — Standardize `Alt+L` (checking Excel key-tip collisions; fallback `Alt+Shift+L`), Names+Layout default, opt-in labelled Steps mode, explicit Refactor-to-LET, and Classic raw-formula `F2` editing.
- **Decisions:** [D-23](decisions.md#d-23), [D-34](decisions.md#d-34)
- **`same-as` groups (3):** `P19-CARBON-0030` = `P19-CARBON-0068` = `P19-CARBON-0181`; `P19-FABLE-0158` = `P19-FABLE-0222` = `P19-FABLE-0485`; `P19-NEON-0058` = `P19-NEON-0228`
- **Agreements:**
  - Formula Lens changes display ONLY—never stored formulas—and clearly shows a `[Readable view]` badge with raw Excel syntax on hover or beneath.
  - Real Excel defined names are rendered visibly distinct from inferred row/column header labels (guillemets `‹…›`), and right-clicking an inferred label offers a previewed `'Make it a name…'` command.
  - All sources agree that synthetic virtual `LET` folding MUST NEVER be editable directly (to prevent silent AST churn and pre-365 Excel breakage); converting to real `LET` is always an explicit, previewed `'Refactor cell to LET'` command.
  - Formula Lens defaults to OFF in Classic and ON in Notebook and Model map.
- **Conflicts:**
  - **Default Formula Lens layers: Names + Layout only vs automatic virtual-LET decomposition** (`P19-FABLE-0158`, `P19-FABLE-0222` vs `P19-CARBON-0068`, `P19-NEON-0058`, `P19-NEON-0228`) → Resolved by **[D-23](decisions.md#d-23)**
  - **Editing behaviour while Formula Lens is active: bidirectional name-alias editing vs editing raw formula with inline alias badges** (`P19-CARBON-0068`, `P19-CARBON-0113`, `P19-FABLE-0158` vs `P19-NEON-0105`) → Resolved by **[D-23](decisions.md#d-23)**
  - *Cross-cluster `conflicts-with` links (resolved by D-23, D-34):* `P19-CARBON-0030` vs `P19-GPT-0204` (CL-04-27)

### CL-09-12: Standalone Persistence, Clipboard Interop, Accessibility & Verification Card Printing

- **Summary:** Standalone Formula Explorer auto-persists recent local scratchpads, exports cleanly to plain `.xlsx` for Excel handoff AND saves explicit JSON test-suite files (recording formula, inputs, expected values, and provider requirements) for add-in developers. Clipboard interop turns pasted Excel formulas into prompted inputs, keyboard/ARIA navigation covers every region shape-first, and printing produces a 1-page Formula Verification Card.
- **Cards (7):** `P19-CARBON-0121`, `P19-FABLE-0105`, `P19-FABLE-0106`, `P19-FABLE-0202`, `P19-FABLE-0223`, `P19-GPT-0046`, `P19-NEON-0091`
- **Sources:** P19-CARBON (1), P19-FABLE (4), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core-with-decision` — Standardize local auto-restore, `.xlsx` graduation export, `.dnafe.json` developer test suites, smart Excel clipboard interop, `F6`/shape-first SR navigation, and the 1-page Verification Card print.
- **Decisions:** [D-73](decisions.md#d-73)
- **`same-as` groups (2):** `P19-FABLE-0202` = `P19-NEON-0091`; `P19-CARBON-0121` = `P19-GPT-0046`
- **Cross-cluster `same-as`:** `P19-NEON-0091` = `P19-CARBON-0098` (CL-08-08)
- **Agreements:**
  - Clipboard interop copies formula lines as Excel formulas and array results as values, and pasting an Excel formula into standalone Explorer turns unresolved cell references into prompted inputs (`O14 = ?`).
  - Keyboard and screen-reader fluency uses `F6` to cycle regions (formula line, inputs, result table, steps tree), `Tab` within regions, and announces array outputs shape-first with min-max bounds.
  - Printing produces a single-page Formula Verification Card showing formula text, inputs, evaluated AST steps, format preview, result, and add-in version.
- **Conflicts:**
  - **Standalone Formula Explorer file persistence: plain `.xlsx` export only vs dedicated JSON test-suite files with provider metadata** (`P19-CARBON-0121`, `P19-GPT-0046` vs `P19-FABLE-0223`) → Resolved by **[D-73](decisions.md#d-73)**

## X. Patterns

### CL-10-01: Pattern Inference Philosophy, Evidence Grades & Provenance States

- **Summary:** DNA Calc infers recurring spreadsheet structures automatically from verifiable Excel signals (OOXML objects, R1C1 formula equivalence, dependency topology, formats) so every view works on ordinary .xlsx files without upfront declarations, while keeping workbook facts strictly separate from inferred business meaning via four provenance states and qualitative evidence grades.
- **Cards (14):** `FRAMING-0125`, `FRAMING-0154`, `P19-CARBON-0069`, `P19-CARBON-0070`, `P19-FABLE-0031`, `P19-FABLE-0161`, `P19-FABLE-0162`, `P19-FABLE-0164`, `P19-GPT-0047`, `P19-GPT-0334`, `P19-NEON-0022`, `P19-NEON-0059`, `P19-NEON-0060`, `P21-0088`
- **Sources:** FRAMING (2), P19-CARBON (2), P19-FABLE (4), P19-GPT (2), P19-NEON (3), P21 (1)
- **Disposition:** `core-with-decision` — Make the four provenance states and three qualitative evidence tiers the unified spine of the Patterns chapter; never print confidence percentages in UI mockups or specs.
- **Decisions:** [D-74](decisions.md#d-74)
- **`same-as` groups (5):** `FRAMING-0125` = `P19-FABLE-0031` = `P19-NEON-0022`; `P19-CARBON-0069` = `P19-NEON-0059`; `P19-CARBON-0070` = `P19-NEON-0060`; `FRAMING-0154` = `P19-GPT-0047` = `P21-0088`; `P19-FABLE-0161` = `P19-GPT-0334`
- **Cross-cluster `same-as`:** `P19-FABLE-0162` = `P19-FABLE-0191` (CL-12-05)
- **Agreements:**
  - Every view works immediately on ordinary legacy .xlsx files without requiring upfront user schemas or annotations (infer first, confirm/correct second).
  - Inference runs non-blocking over verifiable Excel signals (OOXML objects, R1C1 formula runs, dependency graph, spatial layout, formats) rather than brittle label regexes.
  - Overlapping patterns can coexist on the same range (e.g., time series + running balance + formula run); the highest-certainty reading governs default display and exceptions are never absorbed.
  - Pattern readings are non-destructive presentation metadata and never silently mutate Excel formulas, cells, or calculation semantics.
- **Conflicts:**
  - **Numeric confidence percentages vs qualitative evidence grades and four lifecycle states** (`P19-CARBON-0070`, `P19-NEON-0060` vs `FRAMING-0154`, `P19-FABLE-0161`, `P19-FABLE-0162`, `P19-GPT-0047`, `P19-GPT-0334`, `P21-0088`) → Resolved by **[D-74](decisions.md#d-74)**

### CL-10-02: Pattern Confirmation, Correction, Relinking & Workbook Structure Overview

- **Summary:** Users and agents inspect evidence ('Why suggested'), confirm or correct inferred blocks in one or two gestures persisted strictly in customXml, review stale readings after external Excel edits ('Needs review after change' / 're-located' / 'orphan'), and orient via the Workbook Structure overview without mutating the workbook or altering Classic cell rendering.
- **Cards (13):** `P19-CARBON-0071`, `P19-CARBON-0115`, `P19-FABLE-0163`, `P19-FABLE-0225`, `P19-FABLE-0487`, `P19-FABLE-0488`, `P19-FABLE-0489`, `P19-FABLE-0575`, `P19-GPT-0054`, `P19-NEON-0061`, `P19-NEON-0110`, `P21-0089`, `P21-0090`
- **Sources:** P19-CARBON (2), P19-FABLE (6), P19-GPT (1), P19-NEON (2), P21 (2)
- **Disposition:** `core-with-decision` — Specify the unified Reading Card (combining Fable M9.1's evidence + ruled-out alternative with Pro 21 W02's fingerprint & Needs-review invalidation), Workbook Structure overview (W01), and Classic non-interference (P19-FABLE-0575).
- **Decisions:** [D-75](decisions.md#d-75)
- **`same-as` groups (3):** `P19-CARBON-0071` = `P19-NEON-0061` = `P19-FABLE-0487`; `P19-CARBON-0115` = `P19-FABLE-0225` = `P19-NEON-0110`; `P19-FABLE-0489` = `P21-0090`
- **Agreements:**
  - Opening a reading card shows kind, label, provenance state, concrete line-by-line evidence ('Why suggested'), and nearest ruled-out alternative.
  - Corrections (Confirm, Rename, Reclassify, Extend/Shrink, Split, Merge, Dismiss / Not a pattern, Mark Override as Noted) work identically for humans and agents and persist in xl/customXml/.
  - External or subsequent edits that change headers, ranges, or formula runs invalidate only the presentation reading ('Needs review after change', 're-located', or 'orphan'), never the Excel workbook.
  - Workbook Structure overview summarizes sheet counts, formula counts, hidden-sheet counts (without revealing hidden cells or changing visibility), and recognized patterns even while dependency resolution is partial.
  - Unclassified irregular islands remain labelled ranges with a Classic preview rather than forced classifications.
- **Conflicts:**
  - **Model-mutating shortcuts ('Pin with a name') inside pattern correction vs strict metadata-only correction** (`P19-FABLE-0163`, `P19-FABLE-0488` vs `P19-CARBON-0071`, `P19-GPT-0054`) → Resolved by **[D-75](decisions.md#d-75)**

### CL-10-03: Pattern Taxonomy Consolidation, Sampling & Engine Detection Needs

- **Summary:** Establishes one clean, consolidated catalogue of recurring spreadsheet model patterns (avoiding overlapping proliferations), enforces strict sampling-vs-exact-calculation boundaries, and specifies the inner-layer engine queries required for R1C1 equivalence clustering, anomaly detection, and structural metadata.
- **Cards (11):** `FRAMING-0124`, `P19-CARBON-0072`, `P19-CARBON-0212`, `P19-FABLE-0571`, `P19-FABLE-0572`, `P19-FABLE-0573`, `P19-FABLE-0574`, `P19-GPT-0055`, `P19-NEON-0062`, `P19-NEON-0252`, `P19-NEON-0254`
- **Sources:** FRAMING (1), P19-CARBON (2), P19-FABLE (4), P19-GPT (1), P19-NEON (3)
- **Disposition:** `core-with-decision` — Present the consolidated 20-pattern / 5-family table cleanly at the top of the Patterns chapter, plus the C-list engine detection primitives.
- **Decisions:** [D-76](decisions.md#d-76)
- **`same-as` groups (2):** `P19-CARBON-0212` = `P19-NEON-0252`; `P19-FABLE-0571` = `P19-NEON-0254`
- **Agreements:**
  - The engine/host layer must expose R1C1 formula-run clustering with interior anomaly/override/partial-range detection (C-6 / C-14), OOXML structural metadata feeds (C-16 / C-47), and quick workbook counts without full calculation (C-48).
  - Every pattern command and address is equally accessible to the command palette and agents, with non-blocking agent reads and proposal-checked writes (C-49).
  - All structural queries must meet big-workbook latency budgets or explicitly report partiality (C-50).
  - Sampling is strictly an explicit display/inspection tool (always stating sample size and method) and never supports exact totals or soundness claims; irregular Enron-class workbooks motivate testing without replacing modern Excel features.
- **Conflicts:**
  - **Scope and granularity of the canonical pattern catalogue (15 vs 18 vs 19 vs 23 vs 27 patterns)** (`FRAMING-0124`, `P19-NEON-0062`, `P19-CARBON-0072` vs `P19-FABLE-0571`) → Resolved by **[D-76](decisions.md#d-76)**

### CL-10-04: Sheet-as-Database & Reference/Lookup Tables

- **Summary:** Covers tabular record sheets (Sales!Orders) and small lookup/reference tables (QuotaTable, Ref_Hubs), grounding detection in Excel ListObjects vs rectangular ranges, surfacing key-uniqueness and partial-range audit warnings, and projecting them cleanly across all five views without ever turning 25,000 rows into 25,000 diagram boxes.
- **Cards (10):** `P19-GPT-0048`, `P19-CARBON-0229`, `P19-FABLE-0576`, `P19-GPT-0335`, `P19-NEON-0271`, `P21-0091`, `P19-CARBON-0234`, `P19-FABLE-0582`, `P19-GPT-0340`, `P19-NEON-0276`
- **Sources:** P19-CARBON (2), P19-FABLE (2), P19-GPT (3), P19-NEON (2), P21 (1)
- **Disposition:** `core-with-decision` — Document Sheet-as-database and Reference/lookup tables as the first two patterns of Family 1 (Data & Reference), featuring Sales!Orders and Ref_Hubs as anchor examples.
- **Decisions:** [D-77](decisions.md#d-77)
- **`same-as` groups (3):** `P19-CARBON-0234` = `P19-FABLE-0582` = `P19-GPT-0340` = `P19-NEON-0276`; `P19-CARBON-0229` = `P19-NEON-0271`; `P19-GPT-0335` = `P21-0091`
- **Agreements:**
  - Excel ListObject Tables are deterministic ('From workbook') database structures; Model Map always collapses an entire database table into a single compact source node listing read columns/ports.
  - Inspector audits column types, blanks, numbers-stored-as-text, key uniqueness, duplicate/missing lookup keys, match modes, and partial-range reference clips (e.g., referencing 22,000 of 25,000 rows).
  - Reference/lookup tables appear as folded/appendix frames in Notebook, compact lookup nodes with key-labelled join wires in Model Map, and validated dropdown sources in App (never silently deduplicated or sorted).
  - Classic keeps the full virtualised grid with sticky headers and AutoFilter.
- **Conflicts:**
  - **Heuristic database promotion threshold, Notebook row-window size, and Table conversion affordance** (`P19-CARBON-0229`, `P19-NEON-0271` vs `P19-FABLE-0576`, `P19-GPT-0335`, `P21-0091`) → Resolved by **[D-77](decisions.md#d-77)**

### CL-10-05: Inputs, Assumptions, Control Panels & Switches

- **Summary:** Covers labelled constant assumption blocks, native Excel Form Controls / Data Validation dropdowns, and mode switches, defining how editable parameters flow into Inspector fan-out counts, Notebook parameter blocks, Model Map left-edge ports, and App input controls.
- **Cards (9):** `P19-CARBON-0230`, `P19-FABLE-0577`, `P19-GPT-0336`, `P19-NEON-0272`, `P19-CARBON-0238`, `P19-FABLE-0587`, `P19-GPT-0345`, `P19-NEON-0280`, `P19-FABLE-0597`
- **Sources:** P19-CARBON (2), P19-FABLE (3), P19-GPT (2), P19-NEON (2)
- **Disposition:** `core-with-decision` — Cover Inputs & assumptions and Control panels (including boolean/enum switches) in Family 2, stressing the boundary between 'From workbook' controls and 'Suggested' constant blocks.
- **Decisions:** [D-77](decisions.md#d-77)
- **`same-as` groups (3):** `P19-CARBON-0238` = `P19-FABLE-0587` = `P19-GPT-0345` = `P19-NEON-0280`; `P19-CARBON-0230` = `P19-NEON-0272`; `P19-FABLE-0577` = `P19-GPT-0336`
- **Agreements:**
  - Native OOXML Form Controls, ActiveX controls with linked cells, Data Validation rules, and unlocked cells on protected sheets are 'From workbook' control/input facts.
  - Inspector exposes every input and control with its linked cell, validation/protection state, macro action (if any), and downstream fan-out count.
  - Model Map places inputs and control ports at the left edge / Parameter Rail synchronized live with on-sheet controls.
  - Control bindings preserve the exact min/max/step and linked-cell semantics of Excel.
- **Conflicts:**
  - **Auto-promoting inferred constant blocks to App inputs vs requiring explicit confirmation (and standalone Switch pattern)** (`P19-CARBON-0230`, `P19-NEON-0272` vs `P19-FABLE-0577`, `P19-GPT-0336`, `P19-FABLE-0597`) → Resolved by **[D-77](decisions.md#d-77)**

### CL-10-06: Calculation Blocks, Ragged Rollups & Two-Way Grids

- **Summary:** Covers contiguous rectangular formula regions sharing R1C1 formula equivalence and subtotal footers, along with two specialized calculation-grid variants: ragged multi-level SUBTOTAL/AGGREGATE hierarchies (catching double-counted SUM bugs) and two-way mixed-anchor formula grids.
- **Cards (6):** `P19-CARBON-0231`, `P19-FABLE-0578`, `P19-GPT-0337`, `P19-NEON-0273`, `P19-CARBON-0245`, `P19-FABLE-0602`
- **Sources:** P19-CARBON (2), P19-FABLE (2), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core-with-decision` — Describe Calculation Blocks in Family 3 with dedicated callouts for the Ragged Rollup (SUBTOTAL double-count audit) and Two-Way Grid sub-variants.
- **Decisions:** [D-76](decisions.md#d-76)
- **`same-as` groups (1):** `P19-CARBON-0231` = `P19-FABLE-0578` = `P19-GPT-0337` = `P19-NEON-0273`
- **Agreements:**
  - Calculation blocks are detected from contiguous R1C1-identical formula runs bounded by blank rows, headers, or subtotal lines, always exposing interior exceptions rather than hiding them.
  - Users and agents can rename, split, or merge visual block groupings in customXml without altering Excel cells.
  - Inspector shows the block as one formula family with its inputs, outputs, timing cost, and exception list; Model Map renders a single drillable composite block; App keeps intermediate blocks hidden unless an output row is pinned.
  - Overlapping SUM ranges inside nested indented hierarchies (double-counting subtotals) are a critical audit hazard that Inspector must flag.
- **Conflicts:**
  - **Whether Ragged Rollups and Two-Way Grids are standalone top-level patterns or variants of Calculation Blocks** (`P19-CARBON-0245`, `P19-FABLE-0602` vs `P19-GPT-0337`, `P19-NEON-0273`) → Resolved by **[D-76](decisions.md#d-76)**

### CL-10-07: Time Series with Periods Across

- **Summary:** Covers horizontal period-based projections (Jan-Dec, Q1-Q4, FY26-FY30) with R1C1-identical formulas across columns, carry-forward recurrences, actuals-to-forecast switchovers, and right-hand summary totals.
- **Cards (5):** `P19-GPT-0050`, `P19-CARBON-0232`, `P19-FABLE-0579`, `P19-GPT-0338`, `P19-NEON-0274`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-GPT (2), P19-NEON (1)
- **Disposition:** `core-with-decision` — Use Budget!Monthly as the flagship Time Series example, explicitly demonstrating actuals-to-forecast switchover and mixed text/date caption handling.
- **Decisions:** [D-77](decisions.md#d-77)
- **`same-as` groups (2):** `P19-CARBON-0232` = `P19-NEON-0274`; `P19-FABLE-0579` = `P19-GPT-0338`
- **Agreements:**
  - Recognised from monotonic date/period headers across columns above rows sharing identical horizontal R1C1 formulas and optional right-hand SUM/AVERAGE totals.
  - Inspector highlights the exact period where a row's formula changes (e.g., actuals-to-forecast switchover or accidental break) rather than hiding it.
  - Notebook displays a compact time-series Data Frame (with period lens notation like Customers[prev] and optional sparkline/FY total); Model Map shows one period-folded block with [t-1] markers instead of duplicating 12+ columns; App renders a bound chart/period summary.
  - Users can confirm the period axis, mark the actuals/forecast boundary, and exclude non-period columns.
- **Conflicts:**
  - **Treatment of text period captions ('Jan', 'Jul actuals') vs true Excel serial dates** (`P19-CARBON-0232`, `P19-NEON-0274` vs `P19-FABLE-0579`, `P19-GPT-0338`) → Resolved by **[D-77](decisions.md#d-77)**

### CL-10-08: Scenarios, Sensitivity Data Tables & Saved Solver Set-Ups

- **Summary:** Covers Excel What-If Data Tables ({=TABLE(r,c)}), saved Excel Scenario Manager entries, selector-switched parallel assumption columns (INDEX/CHOOSE/IF), and saved Solver settings, distinguishing read-only scenario viewing from workbook-editing scenario creation.
- **Cards (7):** `P19-GPT-0049`, `P19-CARBON-0233`, `P19-FABLE-0580`, `P19-FABLE-0581`, `P19-GPT-0339`, `P19-NEON-0275`, `P19-FABLE-0598`
- **Sources:** P19-CARBON (1), P19-FABLE (3), P19-GPT (2), P19-NEON (1)
- **Disposition:** `core-with-decision` — Group Scenarios and Sensitivity Data Tables together in Family 2 (with saved Solver parameters noted as a secondary 'From workbook' optimization metadata surface).
- **Decisions:** [D-68](decisions.md#d-68)
- **`same-as` groups (1):** `P19-CARBON-0233` = `P19-NEON-0275`
- **Agreements:**
  - Native Excel {=TABLE(r,c)} Data Tables and saved Excel Scenarios are 'From workbook' facts; parallel constant columns switched by INDEX/CHOOSE/IF/XLOOKUP on one selector cell are 'Suggested' scenarios.
  - Inspector exposes Data Table row/column input cells and recalculation sweep cost ('recalculates model 25 times'), plus which inputs differ and which outputs move across scenarios.
  - Model Map renders a switch/sweep node showing parallel inputs or sweep fan-out without duplicating the downstream model N times.
  - App exposes scenario selectors with Compare / Try / Apply semantics and renders sensitivity Data Tables as shaded read-only grids.
- **Conflicts:**
  - **CustomXML scenario preset snapshots & heuristic Goal Seek guessing vs strict workbook-backed scenarios and explicit Solver settings** (`P19-NEON-0275`, `P19-FABLE-0598` vs `P19-FABLE-0580`, `P19-FABLE-0581`, `P19-GPT-0339`) → Resolved by **[D-68](decisions.md#d-68)**

### CL-10-09: Pipelines Across Sheets & Repeated-Sheet Families

- **Summary:** Covers multi-sheet directed calculation pipelines (Inputs -> Monthly -> Summary) and structurally identical sheet families consolidated via 3-D sums or rollups (Jan..Dec, regional tabs), keeping back-edges, hidden tabs, dynamic references, and per-sheet deviations explicitly visible.
- **Cards (6):** `P19-CARBON-0235`, `P19-FABLE-0583`, `P19-GPT-0341`, `P19-NEON-0277`, `P19-FABLE-0593`, `P19-GPT-0354`
- **Sources:** P19-CARBON (1), P19-FABLE (2), P19-GPT (2), P19-NEON (1)
- **Disposition:** `core` — Include both Pipelines Across Sheets and Repeated-Sheet Families in Family 3; highlight how Repeated-Sheet Families also directly power Model Map compression on 40-sheet workbooks like Inherited.
- **`same-as` groups (2):** `P19-CARBON-0235` = `P19-FABLE-0583` = `P19-GPT-0341` = `P19-NEON-0277`; `P19-FABLE-0593` = `P19-GPT-0354`
- **Agreements:**
  - Cross-sheet pipelines are inferred from the inter-sheet dependency graph: Inspector surfaces stage costs, reverse back-edges, and unresolved dynamic edges; Notebook offers horizontal multi-lane Pipeline layout; Model Map lays top-level sheet groups left-to-right; App maps entry stage to inputs and sink stage to outputs.
  - Repeated-sheet families (tabs matching cell-for-cell and/or targeted by 3-D references) collapse cleanly into a stack ('12 alike') in Model Map and a template + picker in Notebook/App.
  - Both patterns MUST disclose hidden-sheet membership (without unhiding), unresolved dynamic/external edges, and every cell where one sheet in a repeated family deviates from the others.
- **Conflicts:** None

### CL-10-10: Running Balances & Deliberate Circular References

- **Summary:** Distinguishes strictly between acyclic temporal running balances (corkscrews where each row/column reads its neighbour, e.g. Loan amortization) and true algebraic circular references (strongly connected components evaluated under Excel's Iterative Calculation setting).
- **Cards (6):** `P19-CARBON-0236`, `P19-NEON-0278`, `P19-FABLE-0584`, `P19-FABLE-0585`, `P19-GPT-0342`, `P19-GPT-0343`
- **Sources:** P19-CARBON (1), P19-FABLE (2), P19-GPT (2), P19-NEON (1)
- **Disposition:** `core-with-decision` — Document Running Balances and Deliberate Circular References as consecutive but explicitly distinct patterns in Family 4, contrasting Loan's 360-row schedule with Stock/debt iterative loops.
- **Decisions:** [D-76](decisions.md#d-76)
- **`same-as` groups (3):** `P19-CARBON-0236` = `P19-NEON-0278`; `P19-FABLE-0584` = `P19-GPT-0342`; `P19-FABLE-0585` = `P19-GPT-0343`
- **Agreements:**
  - Running balances always expose their opening seed, recurrence identity (End = Beg - Principal), broken/overwritten links in Inspector, and balance trajectory in Notebook/App.
  - Deliberate circular references are detected deterministically from dependency-graph SCCs plus workbook Iterate=ON; Inspector reports exact participating cells, MaxIterations/MaxChange settings, iteration count, and convergence residual (never hiding non-convergence).
  - When Iterate=OFF, a cycle is reported as an Excel circular-reference error state; enabling Iterative Calculation is an explicit workbook-setting edit, whereas annotating an existing loop as Intended vs Accidental is presentation metadata.
- **Conflicts:**
  - **Bundling Running Balances and Deliberate Circular References into one pattern vs keeping them strictly distinct** (`P19-CARBON-0236`, `P19-NEON-0278` vs `P19-FABLE-0584`, `P19-FABLE-0585`, `P19-GPT-0342`, `P19-GPT-0343`) → Resolved by **[D-76](decisions.md#d-76)**

### CL-10-11: Stocks, Flows, Feedback & Delays

- **Summary:** Covers systems-dynamics models built in Excel (state accumulation Stock[t] = Stock[t-1] + Inflow - Outflow, shift-register delay pipelines, and IFS/SWITCH policy guards), projecting them natively in Model Map while enforcing the owner rule that circular references are not delays and supporting both RTD-clock and macro/volatile-Calculate ticked models.
- **Cards (6):** `P19-GPT-0051`, `P19-CARBON-0243`, `P19-FABLE-0592`, `P19-GPT-0350`, `P19-GPT-0351`, `P19-NEON-0285`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-GPT (3), P19-NEON (1)
- **Disposition:** `core-with-decision` — Make Stocks, Flows, Feedback & Delays a showcase pattern in Family 4, explicitly citing DECISIONS_20261126 #3 on RTD vs macro/Calculate ticking and SKIN_DISCUSSION #6 on circular != delay.
- **Decisions:** [D-78](decisions.md#d-78)
- **`same-as` groups (2):** `P19-CARBON-0243` = `P19-NEON-0285`; `P19-FABLE-0592` = `P19-GPT-0350` = `P19-GPT-0351`
- **Agreements:**
  - Model Map is the signature native surface for stocks, flows, delays, modal policy switches, and transient history plotting, while Classic remains the faithful backing grid.
  - Every diagram arrow and sign convention follows actual Excel formulas and can be traced cell-for-cell in Inspector.
  - A circular reference is NEVER itself a time delay (SKIN_DISCUSSION #6): delay labels (e.g., '3 days' / z^-1) require explicit formula, row-lag, or shift-array evidence.
- **Conflicts:**
  - **Auto-classifying Stock/Flow/Delay semantics as High confidence vs treating systems roles as 'Suggested' pending confirmation (and tick mechanism)** (`P19-CARBON-0243`, `P19-NEON-0285` vs `P19-FABLE-0592`, `P19-GPT-0350`, `P19-GPT-0351`) → Resolved by **[D-78](decisions.md#d-78)**

### CL-10-12: Simulations & Stochastic Trials

- **Summary:** Covers Monte Carlo and repeated-trial models using RAND/RANDBETWEEN/RANDARRAY, Data Table sweeps, or stepped recalculation, exposing volatility cost, trial counts, and outcome distributions without ever inventing a view-side random engine.
- **Cards (4):** `P19-CARBON-0237`, `P19-FABLE-0586`, `P19-GPT-0344`, `P19-NEON-0279`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core-with-decision` — Keep Simulation controls tightly bound to Excel's real recalculation, Data Table, RTD-clock, or macro mechanisms.
- **Decisions:** [D-03](decisions.md#d-03)
- **`same-as` groups (1):** `P19-FABLE-0586` = `P19-GPT-0344` = `P19-NEON-0279`
- **Agreements:**
  - Detected from RAND/RANDBETWEEN/RANDARRAY AST nodes feeding trial rows, percentile aggregations, or {=TABLE} sweeps.
  - Classic shows volatile markers and F9 recalculation; Inspector reports volatile recalculation fan-out and timing cost; Notebook and App display bound histograms / P10-P50-P90 fan charts with explicit trial counts and freshness.
- **Conflicts:**
  - **View-side simulation runner / reseeding controls vs strict Excel-native execution triggers** (`P19-CARBON-0237` vs `P19-FABLE-0586`, `P19-GPT-0344`, `P19-NEON-0279`) → Resolved by **[D-03](decisions.md#d-03)**
  - *Cross-cluster `conflicts-with` links (resolved by D-03):* `P19-CARBON-0237` vs `FRAMING-0053` (CL-07-07), `FRAMING-0068` (CL-01-03), `FRAMING-0141` (CL-07-08), `FRAMING-0168` (CL-07-08), `P19-GPT-0088` (CL-07-07), `P21-0064` (CL-07-07) (+2 more)

### CL-10-13: PivotTables & PivotCaches

- **Summary:** Covers native OOXML PivotTables and their PivotCaches as deterministic ('From workbook') aggregation structures, exposing cache freshness, source row coverage, and real Excel Pivot refresh/filter actions across all five views.
- **Cards (4):** `P19-CARBON-0239`, `P19-FABLE-0588`, `P19-GPT-0346`, `P19-NEON-0281`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core` — Unanimous across all four Pro 19 sources; use Sales!Report PivotTable as the canonical example.
- **`same-as` groups (1):** `P19-CARBON-0239` = `P19-FABLE-0588` = `P19-GPT-0346` = `P19-NEON-0281`
- **Agreements:**
  - Detected deterministically ('From workbook') from OOXML pivotTableDefinition and pivotCacheDefinition.
  - Inspector exposes source binding, last refresh timestamp/status, whether source data changed since refresh (stale-cache badge), source row coverage, and GETPIVOTDATA dependents.
  - Classic renders the native tabular Pivot layout with field filters/slicers; Notebook renders a bound Pivot Data Frame with freshness line; Model Map renders one '[Pivot: ...]' aggregation node between source and report; App exposes read-only summary cards with approved slicers and Refresh.
  - Filtering and refreshing always execute real Excel Pivot operations, never synthetic view-only re-aggregation.
- **Conflicts:** None

### CL-10-14: Dynamic Array Spill Pipelines & LAMBDA Libraries

- **Summary:** Covers modern Excel dynamic-array spill anchors (FILTER/SORT/UNIQUE/SEQUENCE/HSTACK/SCAN, # spill operators, #SPILL! obstructions) and workbook-scoped LAMBDA Defined Names as deterministic ('From workbook') atomic blocks and reusable function definitions.
- **Cards (4):** `P19-CARBON-0246`, `P19-FABLE-0601`, `P19-GPT-0356`, `P19-NEON-0287`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core` — Unanimous high-value modern Excel pattern; include in Family 1 (Data & Reference) and cross-link to Formula Explorer.
- **`same-as` groups (1):** `P19-CARBON-0246` = `P19-FABLE-0601` = `P19-GPT-0356` = `P19-NEON-0287`
- **Agreements:**
  - Detected deterministically ('From workbook') from engine dynamic-array spill metadata and Defined Name LAMBDA/formula ASTs.
  - Spill footprints always preserve single-anchor ownership: Classic draws the spill frame and highlights exact #SPILL! blocking cells; attempting to edit a spill child refuses with a jump to the anchor or obstruction.
  - Inspector treats the entire spill as one atomic node (never 500 individual cell nodes), tracks # resize cascades, and lists LAMBDA definitions with all call sites.
  - Notebook renders spills as auto-sizing reactive Data Frames and lists LAMBDA libraries in a definitions section; Model Map shows array-valued ports with live [R x C] dimensions (or 'shape unavailable') and reusable LAMBDA definition boxes; App binds to auto-sizing result tables.
  - Formula Explorer lists workbook LAMBDAs in the function list and supports calling and stepping into them.
- **Conflicts:** None

### CL-10-15: Live Data & External-Link Consolidation

- **Summary:** Covers real-time streaming formulas (RTD, async UDFs, linked data types, NOW/TODAY) and external workbook references ([OtherBook.xlsx]Sheet!A1), keeping source availability, freshness timestamps, missing-link caches, and UI observation pauses strictly honest across all views.
- **Cards (8):** `P19-GPT-0052`, `P19-CARBON-0240`, `P19-FABLE-0589`, `P19-GPT-0347`, `P19-NEON-0282`, `P19-FABLE-0596`, `P19-GPT-0355`, `P19-NEON-0288`
- **Sources:** P19-CARBON (1), P19-FABLE (2), P19-GPT (3), P19-NEON (2)
- **Disposition:** `core-with-decision` — Present Live Data and External-Link Consolidation as adjacent patterns in Family 4, sharing the signal-orange external boundary visual language.
- **Decisions:** [D-78](decisions.md#d-78)
- **`same-as` groups (2):** `P19-FABLE-0589` = `P19-GPT-0347` = `P19-NEON-0282`; `P19-FABLE-0596` = `P19-GPT-0355`
- **Agreements:**
  - RTD, async UDFs, web/volatile clock functions, and external workbook references are detected deterministically ('From workbook') from formula ASTs and OOXML external-link records.
  - Classic marks external/live cells with the external/signal glyph and hover freshness/path details; Inspector provides a Live & External Links instrument showing per-topic update rates, event strip, missing-file status, last update, and blast radius.
  - Model Map places live feeds and external workbooks as edge boundary source nodes in signal orange with unresolved-edge markers when links are missing.
  - Notebook and App always surface timestamps and cached/stale/missing-source notices and never display an optimistic 'Ready' status over missing external links.
  - Remapping a missing link target is an explicit workbook edit (Excel's Edit Links).
- **Conflicts:**
  - **UI pause/throttle semantics on live feeds and heuristic detection of paste-special staging tabs** (`P19-CARBON-0240`, `P19-NEON-0288` vs `P19-GPT-0347`, `P19-NEON-0282`, `P19-FABLE-0596`, `P19-GPT-0355`) → Resolved by **[D-78](decisions.md#d-78)**

### CL-10-16: Macro-Driven Models

- **Summary:** Covers .xlsm/.xlsb workbooks containing vbaProject.bin, macro-assigned buttons, or VBA UDFs, enforcing quarantine-by-default security (Off / Guarded / Unguarded), read-only code inspection, and observed-only post-run effect tracking without promising v1 macro undo.
- **Cards (4):** `P19-CARBON-0241`, `P19-FABLE-0590`, `P19-GPT-0348`, `P19-NEON-0283`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core-with-decision` — Cite DECISIONS_20261126 #4 explicitly; align quarantine terminology with standing macro security levels (Off / Guarded / Unguarded).
- **Decisions:** [D-04](decisions.md#d-04), [D-88](decisions.md#d-88)
- **`same-as` groups (1):** `P19-GPT-0348` = `P19-NEON-0283`
- **Agreements:**
  - Presence of vbaProject.bin, macro-assigned controls/shapes (onAction), and VBA UDF calls are detected deterministically ('From workbook').
  - Macros are quarantined by default on open with a non-modal security bar and read-only VBA code inspection; Notebook, Model Map, and App never execute macros silently and surface whitelisted macro buttons only after explicit trust (or show a plain desktop route when unavailable in browser).
  - Inspector inventories all modules, procedures, callers, signature/permission state, and observed post-run changes.
- **Conflicts:**
  - **Promising reversible Macro Undo checkpoints & static VBA range inference vs DECISIONS_20261126 #4 and observed-only effects** (`P19-CARBON-0241`, `P19-FABLE-0590` vs `P19-GPT-0348`, `P19-NEON-0283`) → Resolved by **[D-88](decisions.md#d-88)**
  - *Cross-cluster `conflicts-with` links (resolved by D-04, D-88):* `P19-CARBON-0241` vs `FRAMING-0169` (CL-14-04), `P19-GPT-0038` (CL-02-10), `P19-GPT-0079` (CL-14-04), `P19-GPT-0205` (CL-04-30), `P19-GPT-0207` (CL-04-30), `P19-GPT-0296` (CL-14-04) (+1 more)

### CL-10-17: Reports, Dashboards, Irregular Forms & Unread Scratch Areas

- **Summary:** Covers terminal presentation sheets (charts, KPI summaries, print areas), printed invoice/statement forms (merged cells, borders, sparse formulas), and unread scratch/dead workings, reconciling Notebook's Jupyter-chain model with App's reader/form surface.
- **Cards (8):** `P19-GPT-0053`, `P19-CARBON-0242`, `P19-FABLE-0591`, `P19-GPT-0349`, `P19-NEON-0284`, `P19-FABLE-0599`, `P19-GPT-0357`, `P19-FABLE-0600`
- **Sources:** P19-CARBON (1), P19-FABLE (3), P19-GPT (3), P19-NEON (1)
- **Disposition:** `core-with-decision` — Cover Reports & Dashboards alongside Irregular Forms and Unread Scratch Areas as the three terminal-sheet roles in Family 5, explicitly respecting DECISIONS_20261126 #6.
- **Decisions:** [D-06](decisions.md#d-06)
- **`same-as` groups (3):** `P19-CARBON-0242` = `P19-NEON-0284`; `P19-FABLE-0591` = `P19-GPT-0349`; `P19-FABLE-0599` = `P19-GPT-0357`
- **Agreements:**
  - Charts, print areas, and 'zero downstream readers' are verifiable workbook facts; 'this sheet is a report / form / scratch area' is a semantic suggestion that users can confirm or reclassify ('This is an output', 'This is a form', 'Keep in view').
  - Classic remains the faithful ground for print-ready reports and heavily merged printed forms; Inspector traces KPI lineage back to assumptions and inventories unclassified islands and unread sheets with sizes.
  - Irregular forms and sparse legacy layouts decline automatic App generation ('Choose inputs and outputs first') and display 'Unclassified content' rather than forced semantic labels.
  - Unread scratch areas are never silently deleted — they remain inventoried in Inspector and folded under a collapsed 'Not used by anything' group in Notebook/Map.
- **Conflicts:**
  - **Auto-scaffolding Notebook executive prose & App dashboards from sink sheets vs DECISIONS_20261126 #6 (Jupyter Notebook) and explicit confirmation** (`P19-CARBON-0242`, `P19-NEON-0284` vs `P19-FABLE-0591`, `P19-GPT-0349`, `P19-GPT-0053`) → Resolved by **[D-06](decisions.md#d-06)**
  - *Cross-cluster `conflicts-with` links (resolved by D-06):* `P19-CARBON-0242` vs `FRAMING-0054` (CL-06-01), `FRAMING-0056` (CL-06-01), `FRAMING-0145` (CL-06-01), `FRAMING-0171` (CL-06-01), `P21-0014` (CL-06-01); `P19-NEON-0284` vs `FRAMING-0054` (CL-06-01), `FRAMING-0056` (CL-06-01), `FRAMING-0145` (CL-06-01), `FRAMING-0171` (CL-06-01), `P21-0014` (CL-06-01)

### CL-10-18: Reconciliations, Checks & Manual Overrides

- **Summary:** Covers accounting tie-out/check formulas (ABS(A-B)<eps, OK/ERR flags) and manual hardcoded overrides/plugs breaking an otherwise uniform formula run, guaranteeing that failed checks and unnoted overrides are never hidden inside collapsed summaries while keeping structural exceptions distinct from formula errors.
- **Cards (7):** `P19-CARBON-0244`, `P19-FABLE-0594`, `P19-GPT-0352`, `P19-NEON-0286`, `P19-CARBON-0247`, `P19-FABLE-0595`, `P19-GPT-0353`
- **Sources:** P19-CARBON (2), P19-FABLE (2), P19-GPT (2), P19-NEON (1)
- **Disposition:** `core-with-decision` — Include Reconciliations/Checks and Manual Overrides as the audit capstone of Family 5; persist noted overrides and confirmed check predicates in customXml per DECISIONS_20261126 #5.
- **Decisions:** [D-78](decisions.md#d-78)
- **`same-as` groups (3):** `P19-CARBON-0244` = `P19-NEON-0286`; `P19-FABLE-0594` = `P19-GPT-0352`; `P19-FABLE-0595` = `P19-GPT-0353`
- **Agreements:**
  - Reconciliation/check formulas and formula-run breaks (hardcoded literal or '+ plug' inside a uniform >=5-cell R1C1 run) are high-value audit patterns essential for real corporate spreadsheets.
  - Inspector surfaces a dedicated Checks view (failures first, both sides traced) and an Exceptions/Overrides list (noted vs unnoted, comparing the cell against its neighbours).
  - Collapsed blocks in Notebook and Model Map ALWAYS surface override counts ('11 alike, 1 differs') and failed-check badges; App surfaces failed checks and unnoted-override warnings so readiness labels never look falsely green.
  - Users can annotate an intentional override ('Noted override' with reason, persisted in customXml) or repair it via Preview Seam showing original vs restored formula and downstream impact.
- **Conflicts:**
  - **Auto-enforcing inferred check tolerances & alarming on overrides vs requiring pass-predicate confirmation and keeping Exception != Error** (`P19-CARBON-0244`, `P19-CARBON-0247`, `P19-NEON-0286` vs `P19-FABLE-0594`, `P19-FABLE-0595`, `P19-GPT-0352`, `P19-GPT-0353`) → Resolved by **[D-78](decisions.md#d-78)**

## XI. Big workbooks

### CL-11-01: Big-Workbook Stress Benchmark (Inherited) & Corpus Measurement Methodology

- **Summary:** Anchors scale testing in an Enron-class 40-sheet / ~300k-formula benchmark workbook (Inherited) alongside the real Excel corpus and carried-forward OxCalc grid perf register, requiring repeatable timed benchmarks across reference and constrained hardware profiles rather than manifest-only simulations.
- **Cards (11):** `FRAMING-0050`, `FRAMING-0078`, `FRAMING-0119`, `P19-CARBON-0073`, `P19-FABLE-0165`, `P19-FABLE-0228`, `P19-FABLE-0490`, `P19-GPT-0062`, `P19-GPT-0281`, `P19-NEON-0063`, `P21-0128`
- **Sources:** FRAMING (3), P19-CARBON (1), P19-FABLE (3), P19-GPT (2), P19-NEON (1), P21 (1)
- **Disposition:** `core` — Standardize one checksummed real Inherited.xlsm benchmark fixture (resolving the minor Pro 19 narrative variations: 301k vs 304k formulas, 6 vs 9 hidden sheets, 47 vs 417 errors) and require real timed measurements on WebView2/Leptos and WASM browser builds.
- **`same-as` groups (2):** `FRAMING-0119` = `P19-CARBON-0073` = `P19-FABLE-0165` = `P19-NEON-0063`; `P19-FABLE-0490` = `P19-GPT-0281`
- **Agreements:**
  - All non-grid views must be tested against real large corpus workbooks and the Inherited stress fixture (~40 sheets including hidden sheets, ~300,000 formulas, one edge-to-edge packed sheet, missing external links, VBA module, volatile OFFSET names, and unfixed errors).
  - Scale budgets are enforced as measurable release criteria (not aspirations): crossing a budget is reported as a performance failure, never excused by silently dropping data.
  - Benchmarking must test both a reference desktop/browser profile (Win11, 4 cores, 16 GB RAM, SSD, 1440x900) and a constrained profile (8 GB RAM, 200-400% zoom, browser WASM build), recording p50/p95/p99 over 10 cold opens and >=1,000 actions with hardware, cache, and provider state.
  - Accessibility at scale (coalesced screen-reader announcements preserving real virtualised coordinates, non-corrupting cancellable save/export) is part of the benchmark harness.
  - Pro 21's manifest fixture (P21-0128) proves UX layout structure only and must be superseded by timed native benchmarks against a real checksummed Inherited workbook and the corpus perf register.
- **Conflicts:** None

### CL-11-02: Unbreakable Scale Invariants, Anomaly Bubbling & Honest Refusal

- **Summary:** Defines the non-negotiable honesty and liveness laws under any workbook size: never freeze the UI or ignore Esc, never drop content without an exact stated count, never conceal interior errors/overrides/broken links inside collapsed summaries, and always accompany every declining view state with size, reason, and at least two concrete ways forward.
- **Cards (11):** `FRAMING-0126`, `P19-CARBON-0022`, `P19-CARBON-0085`, `P19-CARBON-0200`, `P19-CARBON-0201`, `P19-FABLE-0183`, `P19-FABLE-0491`, `P19-FABLE-0492`, `P19-GPT-0283`, `P19-NEON-0077`, `P19-NEON-0231`
- **Sources:** FRAMING (1), P19-CARBON (4), P19-FABLE (3), P19-GPT (1), P19-NEON (2)
- **Disposition:** `core` — Make these four invariants and the 'shown + not shown = total' test the opening contract of the Big Workbooks chapter.
- **`same-as` groups (4):** `P19-CARBON-0022` = `P19-CARBON-0200`; `P19-CARBON-0085` = `P19-FABLE-0183` = `P19-NEON-0077`; `P19-CARBON-0201` = `P19-FABLE-0492` = `P19-GPT-0283`; `P19-FABLE-0491` = `P19-NEON-0231`
- **Agreements:**
  - Three absolute release-blocking failure modes are forbidden across every view: (1) UI hangs or ignoring Esc, (2) silently dropping or truncating content without an exact stated count where shown + hidden = total, and (3) misstating the model.
  - Four concrete misstatement prohibitions apply everywhere: never draw an estimate as fact, never show a formula run as uniform when it has an interior error or manual override, never show a saved/stale/missing-link value as fresh, and never draw a wire absent from formulas.
  - Collapsing formula runs, grouping sheets, or zooming out must ALWAYS bubble interior errors, broken links, and hardcoded overrides onto the parent summary badge.
  - Every declining state states the exact workbook/sheet size, the plain reason the view cannot synthesize cleanly, and at least two immediate 1-click ways forward (e.g., narrow to one sheet/range, open in Inspector, or open in Classic).
- **Conflicts:** None

### CL-11-03: Honest Simplification Techniques: Semantic Zoom & Sampling Discipline

- **Summary:** Specifies the five legal simplification mechanisms shared across views (named blocks with A1 bounds, R1C1 formula-run folding with bubbled exceptions, semantic zoom that never shrinks text below legibility, local expand-on-demand, and strictly display-only row sampling).
- **Cards (4):** `FRAMING-0090`, `P19-FABLE-0172`, `P19-GPT-0282`, `P19-NEON-0070`
- **Sources:** FRAMING (1), P19-FABLE (1), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core` — Pair directly with CL-11-02 as the constructive toolkit every view draws from before refusing.
- **`same-as` groups (1):** `P19-FABLE-0172` = `P19-GPT-0282`
- **Agreements:**
  - Zoom is semantic (Strand Tenet 8): zooming out swaps fine cell detail for legible structural block headers/summaries (>=11 px) and never shrinks text below legibility.
  - Every folded run, aggregate, map group, or sampled preview reports exact coverage, member count, and exception count.
  - Sampling is permitted SOLELY for display previews (windowed tables, Formula Explorer array previews, optional value-distribution peeks), must always state 'Showing n of N' and selection method, and may NEVER feed any calculated value or exact whole-data conclusion.
- **Conflicts:** None

### CL-11-04: Progressive Opening, Non-Blocking Analysis & Safe Large-Workbook Orientation

- **Summary:** Ensures huge workbooks like Inherited open immediately to the last saved viewport with stale values visibly labelled, while sheet inventory, dependency inspection, and volatile recalculation stream asynchronously with separate progress/cancellation controls and zero unsafe auto-execution.
- **Cards (6):** `P19-CARBON-0116`, `P19-GPT-0011`, `P19-GPT-0012`, `P19-GPT-0056`, `P19-GPT-0253`, `P21-0050`
- **Sources:** P19-CARBON (1), P19-GPT (4), P21 (1)
- **Disposition:** `core` — Make Storyboard S01 (opening Inherited safely and progressively) the narrative walk-through for this section.
- **`same-as` groups (1):** `P19-GPT-0011` = `P19-GPT-0253` = `P21-0050`
- **Agreements:**
  - Opening, navigating, analysing, and calculating maintain independent progress and cancellation pipelines so an expensive step never stalls the user.
  - Inherited opens straight to its last saved viewport with saved/not-current values explicitly labelled, without running quarantined macros or refreshing missing external links (missing links offer non-modal Locate file / Keep saved values / Cancel).
  - The sheet list and Workbook Overview appear immediately with separate visible and hidden sheet counts (listing hidden names never unhides them or reveals hidden cells) and incremental progress ('18/40 sheets inspected; 14 errors found so far'), collapsing to a drawer at 200% zoom.
  - Keyboard navigation (Ctrl+G, Back, Inspect name preview without moving selection) works during background indexing, and screen-reader completion announcements coalesce politely once.
  - Volatile legacy patterns (e.g., OFFSET/INDIRECT dirtying 184k cells) recalculate off-thread with a Strip cost meter and Inspector's Preview-Seam refactor suggestion (e.g., OFFSET -> INDEX).
- **Conflicts:** None

### CL-11-05: Startup, File-Open, Indexing & Input Latency Budgets

- **Summary:** Reconciles the widely differing numeric latency budgets across sources into one realistic, enforceable budget table grounded in the WebView2/Leptos desktop + WASM browser baseline (DECISIONS_20261126 #7).
- **Cards (22):** `P19-CARBON-0079`, `P19-CARBON-0080`, `P19-CARBON-0081`, `P19-CARBON-0082`, `P19-CARBON-0083`, `P19-CARBON-0199`, `P19-FABLE-0173`, `P19-FABLE-0174`, `P19-FABLE-0175`, `P19-FABLE-0176`, `P19-FABLE-0177`, `P19-FABLE-0178`, `P19-FABLE-0181`, `P19-GPT-0063`, `P19-GPT-0064`, `P19-GPT-0065`, `P19-GPT-0066`, `P19-GPT-0069`, `P19-NEON-0071`, `P19-NEON-0072`, `P19-NEON-0073`, `P19-NEON-0229`
- **Sources:** P19-CARBON (6), P19-FABLE (7), P19-GPT (5), P19-NEON (4)
- **Disposition:** `core-with-decision` — Publish ONE unified Latency Budget Table in the Big Workbooks chapter with Desktop p95, Browser p95, and Target vs Hard Ceiling columns, citing DECISIONS_20261126 #7.
- **Decisions:** [D-79](decisions.md#d-79)
- **`same-as` groups (5):** `P19-CARBON-0080` = `P19-NEON-0071`; `P19-CARBON-0082` = `P19-NEON-0072` = `P19-NEON-0073`; `P19-CARBON-0199` = `P19-NEON-0229`; `P19-FABLE-0175` = `P19-GPT-0064`; `P19-FABLE-0177` = `P19-GPT-0066`
- **Agreements:**
  - Cached values paint first so users interact before full 300k-formula dependency indexing finishes off-thread.
  - Typing, caret motion, and scrolling stay decoupled from background indexing and recalculation via viewport virtualisation.
  - Live incoming value revisions coalesce onto the frame cadence (aim 60 Hz; applied within <=100 ms p95) rather than rendering every intermediate event.
  - Budgets are defined for a mid-range laptop reference environment, allowing a bounded 2x multiplier on WASM browser startup and initial open only.
- **Conflicts:**
  - **Cold shell startup and Inherited cached first-paint budgets under WebView2/Leptos vs native-style aggressive targets** (`P19-CARBON-0079`, `P19-CARBON-0080`, `P19-CARBON-0199`, `P19-NEON-0071`, `P19-NEON-0229` vs `P19-FABLE-0173`, `P19-FABLE-0174`, `P19-FABLE-0175`, `P19-GPT-0063`, `P19-GPT-0064`) → Resolved by **[D-79](decisions.md#d-79)**
  - **Background 300k-formula indexing budget (1.5 s vs 5 s / 8 s) and interactive keystroke ceiling (12-16 ms vs 50 ms p95)** (`P19-CARBON-0081`, `P19-CARBON-0082`, `P19-CARBON-0083`, `P19-NEON-0072`, `P19-NEON-0073` vs `P19-FABLE-0176`, `P19-FABLE-0177`, `P19-FABLE-0178`, `P19-FABLE-0181`, `P19-GPT-0065`, `P19-GPT-0066`, `P19-GPT-0069`) → Resolved by **[D-79](decisions.md#d-79)**

### CL-11-06: View-Switch, Recalculation Progress & Cancellation Budgets

- **Summary:** Unifies view-switching latency and keyboard shortcuts (Ctrl+Alt+1..5 per DECISIONS_20261126 #12), edit-to-recalculation feedback, progress-indicator onset thresholds, and Esc cancellation guarantees including honest reporting of uninterruptible providers.
- **Cards (10):** `P19-CARBON-0084`, `P19-FABLE-0179`, `P19-FABLE-0180`, `P19-FABLE-0182`, `P19-GPT-0067`, `P19-GPT-0068`, `P19-NEON-0074`, `P19-NEON-0075`, `P19-NEON-0076`, `P19-NEON-0230`
- **Sources:** P19-CARBON (1), P19-FABLE (3), P19-GPT (2), P19-NEON (4)
- **Disposition:** `core-with-decision` — Include directly inside the unified Latency & Responsiveness Budget section; cite DECISIONS_20261126 #12 for Ctrl+Alt+1..5.
- **Decisions:** [D-12](decisions.md#d-12), [D-79](decisions.md#d-79)
- **`same-as` groups (2):** `P19-FABLE-0180` = `P19-GPT-0067`; `P19-NEON-0076` = `P19-NEON-0230`
- **Agreements:**
  - Short work never flashes a spinner; long work keeps the UI scrollable and interactive while dimming or hatching stale dependents.
  - Every long operation supports Esc cancellation with fast acknowledgement, and uninterruptible external providers (e.g., COM/add-ins) are identified honestly instead of falsely claiming cancellation succeeded.
  - Operations exceeding 10 seconds can be sent to the background without blocking unrelated sheet navigation.
- **Conflicts:**
  - **View-switch shortcut (Ctrl+1..5 vs Ctrl+Alt+1..5) and view-switch latency budget (60-100 ms vs 150 ms / 400-500 ms)** (`P19-CARBON-0084`, `P19-NEON-0074` vs `P19-FABLE-0180`, `P19-GPT-0067`) → Resolved by **[D-79](decisions.md#d-79)**
  - **Progress indicator onset threshold (150 ms vs 300 ms vs 1 s) and small-workbook recalc ceiling** (`P19-NEON-0075`, `P19-NEON-0076`, `P19-NEON-0230` vs `P19-FABLE-0179`, `P19-FABLE-0182`, `P19-GPT-0068`) → Resolved by **[D-79](decisions.md#d-79)**
  - *Cross-cluster `conflicts-with` links (resolved by D-12, D-79):* `P19-CARBON-0084` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28)

### CL-11-07: Classic View at Scale (Virtualisation, Semantic Zoom & Sheet Overflow)

- **Summary:** Classic is the universal bedrock that never declines any legal Excel sheet (up to 1,048,576 x 16,384), staying smooth through strict viewport virtualisation, explicit Loading cells, scroll-gutter error/formula density maps, searchable sheet-tab overflow, and Semantic Zoom block outlines.
- **Cards (5):** `P19-CARBON-0074`, `P19-FABLE-0051`, `P19-FABLE-0166`, `P19-GPT-0057`, `P19-NEON-0064`
- **Sources:** P19-CARBON (1), P19-FABLE (2), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core-with-decision` — Fix the Classic Semantic Zoom threshold at <40% (and mention the searchable sheet drawer for >12 sheets + scroll-gutter density strip).
- **Decisions:** [D-48](decisions.md#d-48)
- **`same-as` groups (1):** `P19-FABLE-0051` = `P19-FABLE-0166`
- **Cross-cluster `same-as`:** `P19-FABLE-0166` = `P19-FABLE-0402` (CL-04-23)
- **Agreements:**
  - Classic NEVER declines any legal Excel sheet and keeps every cell and object addressable through strict viewport virtualisation (zero off-screen DOM nodes; fast enough for WebGPU canvas/grid rendering).
  - Hidden rows/columns, filters, and frozen panes preserve exact Excel semantics; unloaded regions say 'Loading' rather than appearing falsely blank; dense formatting alone never forces dense data loading; a region load failure names the region and leaves the file intact.
  - Scroll-gutter density indicators mark formula and error clusters, high-contrast themes preserve shape-based error/external/hatch cues, and workbooks with >12 tabs provide a searchable sheet drawer/switcher.
- **Conflicts:**
  - **Classic Semantic Zoom switchover threshold (<40% vs <55% vs <70% zoom)** (`P19-FABLE-0051`, `P19-FABLE-0166` vs `P19-NEON-0064`, `P19-CARBON-0074`) → Resolved by **[D-48](decisions.md#d-48)**

### CL-11-08: Inspector View at Scale (Root-Cause Folding & Bounded Dependency Lists)

- **Summary:** Keeps Inspector always usable on 300k-formula workbooks by starting at one dependency frontier, folding cascading errors to root causes and identical formulas into R1C1 runs, and refusing flat cell-level hairball diagrams in favour of bounded range nodes, block lists, export, or sheet narrowing.
- **Cards (4):** `P19-CARBON-0075`, `P19-FABLE-0167`, `P19-GPT-0058`, `P19-NEON-0065`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core-with-decision` — Document Inspector's dual thresholds cleanly (200 nodes / 400 edges for graph traces; 2,000 rows for virtualised lists; 500 precedents for range-node folding).
- **Decisions:** [D-80](decisions.md#d-80)
- **`same-as` groups:** None within cluster
- **Agreements:**
  - Inspector always remains available on Inherited-scale workbooks (never refuses the workbook as a whole): all 40 sheets (including hidden/very-hidden), error root causes, missing links, and volatile names remain inspectable.
  - Errors fold to root causes (expandable to all affected cells), repetitive formulas collapse into R1C1 runs with explicit member and exception counts, and large precedent ranges (e.g., SUM over 8,760 rows) render as a single bounded range node with expand-on-demand.
- **Conflicts:**
  - **Inspector graph vs list display caps (200 nodes/400 edges vs 500 precedents vs 2,000 rows vs 2,500 edges) and refusal wording** (`P19-GPT-0058`, `P19-NEON-0065` vs `P19-CARBON-0075`, `P19-FABLE-0167`) → Resolved by **[D-80](decisions.md#d-80)**

### CL-11-09: Notebook View at Scale (Windowed Data Frames & Declining Whole-Book Scaffolding)

- **Summary:** Keeps Notebook fast and legible on large workbooks by displaying virtualised windowed Data Frames with exact full-table aggregates, collapsing formula runs while surfacing exceptions, and refusing whole-book auto-generation in favour of a sheet/output picker or curated Jupyter chain.
- **Cards (4):** `P19-CARBON-0076`, `P19-FABLE-0168`, `P19-GPT-0059`, `P19-NEON-0067`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core-with-decision` — Align Notebook-at-scale tightly with DECISIONS_20261126 #6 (Jupyter cell chain, no reader-mode prose auto-generation).
- **Decisions:** [D-80](decisions.md#d-80)
- **`same-as` groups:** None within cluster
- **Agreements:**
  - Notebook NEVER auto-generates a cell-by-cell or 40-sheet dump for Inherited; it declines whole-workbook auto-scaffolding and offers concrete ways forward (pick specific sheet(s), pick chosen outputs + feeders, select a range, or open Inspector/Classic).
  - Large tables (e.g., Sales!Orders 25,000 rows) render as windowed Data Frames with an explicit count footer ('Showing n of 25,000 rows · Aggregates exact over all rows') and access to the full underlying table.
  - Folded formula runs always keep interior exceptions visible, and unplaced workbook content stays accessible in the workbook inventory.
- **Conflicts:**
  - **Notebook inline block cap (15 vs 150), table window size (15 vs 20 vs 200 rows), and auto-scaffolding vs Jupyter chain** (`P19-CARBON-0076`, `P19-NEON-0067` vs `P19-FABLE-0168`, `P19-GPT-0059`) → Resolved by **[D-80](decisions.md#d-80)**

### CL-11-10: Model Map at Scale (Hierarchical Districts, Display Budgets & Drill-Down)

- **Summary:** Scales Model Map to 40-sheet / 300k-formula workbooks via hierarchical sheet groups and contiguous block decomposition, enforcing a strict viewport display budget, preserving hidden-sheet visibility and formula exceptions, refusing 'Expand all cells', and supporting optional sheet pages inside the map.
- **Cards (8):** `FRAMING-0158`, `P19-CARBON-0077`, `P19-FABLE-0169`, `P19-GPT-0060`, `P19-NEON-0066`, `P21-0072`, `P21-0073`, `P21-0074`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-FABLE (1), P19-GPT (1), P19-NEON (1), P21 (3)
- **Disposition:** `core-with-decision` — Use Pro 21 M03/M04 (Inherited 8 sheet groups + 32 collapsed + Revenue 4x12,000 block drill-down with exceptions D63/H219/F2058) as the canonical Model Map scale walkthrough.
- **Decisions:** [D-80](decisions.md#d-80)
- **`same-as` groups (1):** `FRAMING-0158` = `P19-GPT-0060` = `P21-0072`
- **Agreements:**
  - Large workbooks open at the sheet-group / district level with counted inter-group edges, bundled parallel wires, and stacked identical sheets, never drawing hundreds of thousands of cell boxes.
  - Clicking 'Expand all cells' is plainly refused without losing selection or deleting data, offering drill-down into one sheet group's blocks, a focused submap with breadcrumb, a counted reference list, or Classic.
  - Drilling into a large sheet group decomposes it into explicit contiguous calculation blocks / R1C1 formula runs while keeping every declared formula exception (e.g., D63, H219, F2058) and missing external link explicitly visible.
  - All grouping and expansion is strictly presentation-only (0 formulas, names, or calculation scopes created), preserves hidden-sheet visibility, displays unresolved partial-analysis boundaries honestly (e.g., 18/40 resolved), and supports an optional embedded sheet page inside the map.
- **Conflicts:**
  - **Model Map direct-display node cap (60 vs 80 vs 120 vs 200 nodes / 400 edges) and post-clustering refusal cap (120 vs 500 vs 2,000)** (`P19-FABLE-0169`, `P19-CARBON-0077`, `P19-NEON-0066` vs `P19-GPT-0060`, `P21-0072`) → Resolved by **[D-80](decisions.md#d-80)**

### CL-11-11: App & Formula Explorer at Scale (Contract Enforcement & Large-Array Previews)

- **Summary:** Governs scaling behaviour for the two specialized endpoints: App refuses unstructured massive workbooks unless an explicit/confirmed Input-Output contract exists, while Formula Explorer never declines a legal Excel formula and summarizes large array arguments as virtualised previews while always computing exact full-array results.
- **Cards (6):** `P19-CARBON-0078`, `P19-FABLE-0170`, `P19-GPT-0061`, `P19-NEON-0068`, `P19-FABLE-0171`, `P19-NEON-0069`
- **Sources:** P19-CARBON (1), P19-FABLE (2), P19-GPT (1), P19-NEON (2)
- **Disposition:** `core-with-decision` — Cover both App contract enforcement and Formula Explorer virtualised array previews concisely in the per-view scaling matrix.
- **Decisions:** [D-65](decisions.md#d-65), [D-80](decisions.md#d-80)
- **`same-as` groups (1):** `P19-CARBON-0078` = `P19-NEON-0068`
- **Cross-cluster `same-as`:** `P19-GPT-0061` = `P19-GPT-0044` (CL-08-02)
- **Agreements:**
  - App declines automatic generation on messy, error-ridden, unmarked workbooks like Inherited (pointing clearly to Inspector triage, output-driven input/output selection, or Classic), collapses long output schedules (>50 rows) into grouped/expandable rollups with explicit counts, and never silently substitutes a nearby cell for a broken binding ('Source missing').
  - Formula Explorer NEVER declines any valid Excel formula (refusing only formulas beyond Excel's own limits in Excel's wording) and ALWAYS evaluates over 100% of array elements while displaying virtualised previews for large array arguments.
- **Conflicts:**
  - **App initial behavior when no authored App spec exists (strict 'No App is defined' vs certain-evidence draft vs heuristic auto-App)** (`P19-CARBON-0078`, `P19-NEON-0068` vs `P19-FABLE-0170`, `P19-GPT-0061`) → Resolved by **[D-65](decisions.md#d-65)**
  - **Formula Explorer array-argument preview threshold (50 elements vs 200 cells)** (`P19-NEON-0069` vs `P19-FABLE-0171`) → Resolved by **[D-80](decisions.md#d-80)**

### CL-11-12: Inner-Layer Scale Contracts (Virtualised Reads, Paged Projections & Cancellation)

- **Summary:** Specifies the host/engine C-list query and event contracts required to uphold every big-workbook budget: constant-time viewport window reads, incremental cell loading states, complete-vs-partial structural inventories, resource estimates before expansion, and viewport-prioritized cancellable background work.
- **Cards (8):** `FRAMING-0036`, `P19-CARBON-0225`, `P19-GPT-0304`, `P19-GPT-0305`, `P19-GPT-0330`, `P19-GPT-0331`, `P19-NEON-0240`, `P19-NEON-0269`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-GPT (4), P19-NEON (2)
- **Disposition:** `core` — Feed these C-list contracts directly into the Skin Architecture / Host API design (DECISIONS_20261126 #9) so every view consumes one shared paged, cancellable projection layer.
- **`same-as` groups (2):** `P19-CARBON-0225` = `P19-GPT-0330` = `P19-NEON-0240`; `P19-GPT-0304` = `P19-NEON-0269`
- **Agreements:**
  - Viewport reads must run in O(visible viewport) constant time with zero full-sheet UI materialisation and distinguish three explicit cell states: empty, not yet loaded, and failed to load.
  - Structural inventories expose sheets, tables, objects, hidden/protection state, used extents, region/name dependency queries, calc order, and complete-vs-partial counts with explicit latency and memory budgets.
  - Before expanding a large graph, table, or history, the inner layer supplies resource/coverage estimates and exact totals so views can simplify or decline BEFORE hanging.
  - Long indexing and recalculation runs execute off-thread with incremental progress counts, prioritize visible viewport and Strip cells, support safe Esc cancellation, and report uninterruptible external operations honestly.
- **Conflicts:** None

## XII. Customisation and the skin API

### CL-12-01: View Contract v2, Layering & Neutral Engine Boundary

- **Summary:** All views and custom skins sit behind Layer 2 (the view contract / Skin IR) and never hold a handle to Layer 1 (`calc-host`, `calc-doc`, or `calc-engine`), guaranteeing 'no more than Excel' by construction. Contract v2 routes every action through neutral `Target` values, typed Excel-plus-layout commands, multi-view `ViewId` subscriptions, a view-neutral controller core, and page-side projections/SVG rendering; the skin architecture is central and must also settle the level-0 Formula Explorer crate boundary so `dna-onecalc` never reaches `calc-engine`.
- **Cards (10):** `FRAMING-0002`, `FRAMING-0003`, `FRAMING-0020`, `FRAMING-0032`, `FRAMING-0033`, `FRAMING-0034`, `FRAMING-0041`, `FRAMING-0046`, `FRAMING-0174`, `P19-FABLE-0524`
- **Sources:** FRAMING (9), P19-FABLE (1)
- **Disposition:** `core-with-decision` — Treat as the architectural spine of Chapter XII and the companion Skin Architecture specification (DECISIONS_20261126 #9). On `FRAMING-0041`'s level-0 boundary question, decide decisively in favour of placing the engine-free formula-session tier in a dedicated crate beside/below `calc-host` (moving RTD admission out of `calc-host/src/recalc.rs`) so Cargo crate boundaries—rather than workspace-unified feature flags—mechanically enforce that `dna-onecalc` and `dna-formula-ui` never reach `calc-engine`.
- **Decisions:** [D-09](decisions.md#d-09), [D-81](decisions.md#d-81)
- **`same-as` groups (2):** `FRAMING-0002` = `FRAMING-0020`; `FRAMING-0003` = `FRAMING-0032`
- **Agreements:**
  - Front-end views and skins never reach directly into Layer 1 (`calc-host`, `calc-doc`, `calc-engine`); holding only the Layer 2 view contract enforces 'no more than Excel' mechanically.
  - Contract v2 rests on four pillars: neutral `Target` addressing, one typed command vocabulary (Excel operations + view layout commands), page-side projections/SVG built from neutral worker data, and multi-view `ViewId` subscriptions separating view state from workbook state.
  - The controller core (session, commands, subscriptions, undo) stays strictly view-neutral and isolated from `GridController` so non-grid skins never inherit grid assumptions.
  - Front-end requirements (`C-1..C-50`) specify what the UI needs in plain terms without dictating internal backend structure; per DECISIONS_20261126 #9, Fable and Opus independently design the merged skin architecture before fresh UI work begins.
- **Conflicts:**
  - *Cross-cluster `conflicts-with` links (resolved by D-09, D-81):* `FRAMING-0174` vs `FRAMING-0024` (CL-16-03)

### CL-12-02: Baseline Code State, Fresh UI Rewrite & Runtime Platform

- **Summary:** Records the verified 2026-11-20/22 code baseline (the controller split `dc-xfrhg.3`, typed commands `dc-xfrhg.2`, unified cell assembly `dc-xfrhg.5`, remaining worker-side projections/SVG, single-skin state, and dead-code cleanup) and resolves two major delivery forks via owner rulings: keeping the WebView2/Leptos baseline over FerroUI, and writing the new UI fresh against the merged skin interface rather than executing a micro-step migration that preserves the six experimental skins.
- **Cards (10):** `FRAMING-0021`, `FRAMING-0022`, `FRAMING-0023`, `FRAMING-0028`, `FRAMING-0029`, `FRAMING-0030`, `FRAMING-0031`, `FRAMING-0044`, `FRAMING-0172`, `FRAMING-0175`
- **Sources:** FRAMING (10)
- **Disposition:** `core-with-decision` — Summarise baseline status concisely in the architecture bridge section (not inside-baseball in the UX chapters), state Owner Rulings #7 and #10 plainly, and record both FerroUI and micro-step migration in Appendix Z.
- **Decisions:** [D-07](decisions.md#d-07), [D-10](decisions.md#d-10)
- **`same-as` groups:** None within cluster
- **Agreements:**
  - Verified baseline cleanups already landed (`dc-xfrhg.2`, `dc-xfrhg.3`, `dc-xfrhg.5`, desktop shell wire-decoder removal, single `calc-vba` reader) are kept as the factual starting point.
  - Dead protocol intents, worker-side per-skin projections/SVG builders, duplicated message handling, and retired readers are deleted once Contract v2 is live (`FRAMING-0044`).
  - Build-checked layering ('page code never reaches engine or document code') and 'engine does no more than Excel' remain non-negotiable invariants across the rewrite.
- **Conflicts:**
  - **Fresh UI rewrite against the clean skin interface vs micro-step migration preserving the six legacy skins** (`FRAMING-0175` vs `FRAMING-0028`) → Resolved by **[D-10](decisions.md#d-10)**
  - **WebView2/Leptos desktop baseline vs adopting FerroUI native toolkit** (`FRAMING-0172` vs `FRAMING-0031`) → Resolved by **[D-07](decisions.md#d-07)**

### CL-12-03: Strand Design System Tokens, Themes, Typography & Geometry (v0.1 -> v0.2)

- **Summary:** Consolidates `dna-strand` into Strand v0.2: one `--dna-*` CSS token crate powering three canonical WCAG AA themes (`cockpit-light`, `cockpit-dark`, `high-contrast`), petrol-800 chrome with dedicated dark-chrome ink tokens, contrast-hardened semantic inks on light paper (including WCAG AA `--amber-ink`), dual-stroke focus, a 6-tone Formula Reference Palette, 45-degree Preview Seam hatching, three offline open font voices (Recursive, Recursive Mono, Source Serif 4 with Noto/DejaVu/system fallbacks), and fixed geometry/motion scales.
- **Cards (8):** `FRAMING-0102`, `P19-CARBON-0001`, `P19-FABLE-0011`, `P19-GPT-0007`, `P19-NEON-0001`, `P21-0002`, `P21-0003`, `P21-0004`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-FABLE (1), P19-GPT (1), P19-NEON (1), P21 (3)
- **Disposition:** `core-with-decision` — Publish the unified Strand v0.2 token table in Chapter XII (and cross-reference `dna-strand` `SANCTIONED_PAIRS` tests in Chapter XV).
- **Decisions:** [D-16](decisions.md#d-16)
- **`same-as` groups (1):** `P19-GPT-0007` = `P21-0003`
- **Agreements:**
  - All visual tokens come from one `dna-strand` crate using `--dna-*` CSS custom properties across three canonical themes (`cockpit-light` default, `cockpit-dark`, `high-contrast`), replacing the legacy 16-skin theme concept.
  - Typography uses three distinct offline open voices—Recursive for UI (`27/22/18/15.5/13.5/12.5/11 px`), Source Serif 4 for reading prose, and Recursive Mono for formulas/source—with open Noto/DejaVu/system fallbacks and zero external font downloads or Microsoft font dependencies.
  - Geometry and motion are fixed across views: radii `Cells 0 / Chips 5 / Cards 9 / Panels 14 px`, spacing steps `2, 4, 6, 10, 14, 22 px`, two densities, and `120–160 ms` settle motion respecting `prefers-reduced-motion`.
- **Conflicts:**
  - **Strand v0.1 base token set & raw `#FFB301` amber ink vs Strand v0.2 contrast-hardened chrome and amber text inks** (`P19-FABLE-0011`, `P19-NEON-0001`, `P19-CARBON-0001` vs `FRAMING-0102`, `P21-0002`) → Resolved by **[D-16](decisions.md#d-16)**

### CL-12-04: Themes vs Declarative Skins & Automated Strand Contrast Check

- **Summary:** Reconciles Strand's three-theme discipline with declarative skin theming by allowing skins to override non-semantic token values (surfaces, accents, borders, density, type scale) on top of a base Strand theme, mechanically checked at preview and apply time by unrounded sRGB contrast checks across nine declared semantic pairs while forbidding any rebinding of semantic signal roles.
- **Cards (5):** `P19-CARBON-0019`, `P19-FABLE-0037`, `P19-NEON-0018`, `P19-NEON-0112`, `P21-0102`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-NEON (2), P21 (1)
- **Disposition:** `core-with-decision` — Specify the exact 9 declared sRGB contrast pairs and unrounded threshold rules (`≥ 4.5:1` text, `≥ 3:1` control boundaries and dual-stroke focus) as part of the Skin Validator contract.
- **Decisions:** [D-82](decisions.md#d-82)
- **`same-as` groups (1):** `P19-FABLE-0037` = `P19-NEON-0018`
- **Cross-cluster `same-as`:** `P21-0102` = `FRAMING-0098` (CL-15-07), `P19-GPT-0287` (CL-15-06)
- **Agreements:**
  - Visual drift in custom or agent-authored skins must be caught mechanically by an automated Strand Contrast & Safety Check at preview/load/apply time—never left to manual inspection.
  - Semantic signal roles (error red, verified/changed green, external/volatile signal orange, focus amber) cannot be repurposed or collapsed into one another.
  - Themes govern presentation tokens only and can never alter workbook cell formats, calculation behaviour, or macro permissions.
- **Conflicts:**
  - **Locking themes strictly to Strand's three built-in palettes vs allowing contrast-checked custom non-semantic token overrides** (`P19-FABLE-0037`, `P19-NEON-0018`, `P21-0102` vs `P19-CARBON-0019`) → Resolved by **[D-82](decisions.md#d-82)**

### CL-12-05: Declarative Skin Manifest Scope, Schema & Interchange Format

- **Summary:** Defines a skin as a code-free, schema-validated declaration (`dna.view/1` canonical JSON, exported as `.dnaskin.json` and serialized to standard OOXML `/customXml/itemN.xml` (`urn:dnacalc:views:v1`)) whose application never changes the workbook model hash. Covers five declarative sections: theme tokens/density, per-workbook/per-sheet view settings and reading corrections, panel/layout/App/print composition bound to workbook addresses, keymaps/Deck/quick-slots, and user-invoked macro-free UI command sequences.
- **Cards (14):** `FRAMING-0018`, `P19-CARBON-0086`, `P19-CARBON-0087`, `P19-FABLE-0184`, `P19-FABLE-0185`, `P19-FABLE-0186`, `P19-FABLE-0188`, `P19-FABLE-0189`, `P19-FABLE-0190`, `P19-FABLE-0191`, `P19-FABLE-0508`, `P19-GPT-0070`, `P19-NEON-0078`, `P19-NEON-0079`
- **Sources:** FRAMING (1), P19-CARBON (2), P19-FABLE (8), P19-GPT (1), P19-NEON (2)
- **Disposition:** `core-with-decision` — Publish the `dna.view/1` manifest schema with all five sections (theme, views/readings, layout/bindings/print, keymaps/Deck/slots, user-invoked UI command sequences) and document the standard OOXML `/customXml/itemN.xml` (`urn:dnacalc:views:v1`) persistence envelope in Chapter XIII.
- **Decisions:** [D-83](decisions.md#d-83), [D-84](decisions.md#d-84)
- **`same-as` groups (2):** `FRAMING-0018` = `P19-FABLE-0184` = `P19-FABLE-0508`; `P19-CARBON-0087` = `P19-GPT-0070`
- **Cross-cluster `same-as`:** `P19-FABLE-0191` = `P19-FABLE-0162` (CL-10-01)
- **Agreements:**
  - Applying any declarative skin leaves the Excel model hash strictly unchanged (`R-91`); skins describe presentation, arrangement, bindings, and ergonomics—never calculation.
  - The same declarative format is authored by humans in the studio and emitted by AI agents, covering theme tokens/density/type sizes, per-workbook and per-sheet view preferences (with honest auto-drafting for unconfigured views), layout regions/panels/standard declarative parts (`value`, `input`, `frame`, `chart`, `trace`, `text`), print presets, user corrections to inferred readings, and keymaps.
  - Every command exposed by a skin retains a named shell route so custom layouts never make functionality unreachable.
- **Conflicts:**
  - **Declarative skin file format: canonical JSON (`dna.view/1` / `.dnaskin.json`) vs YAML (`.dnaskin`)** (`P19-CARBON-0086` vs `P19-NEON-0078`) → Resolved by **[D-83](decisions.md#d-83)**
  - **Named macro-free UI command sequences inside skins vs purely passive layout/keymap declarations** (`P19-FABLE-0188`, `P19-NEON-0079` vs `P19-CARBON-0087`, `P19-GPT-0070`) → Resolved by **[D-84](decisions.md#d-84)**
  - *Cross-cluster `conflicts-with` links (resolved by D-83, D-84):* `P19-CARBON-0086` vs `FRAMING-0013` (CL-13-01), `P19-CARBON-0220` (CL-13-01), `P19-NEON-0241` (CL-13-01)

### CL-12-06: Security Boundary: Declarative Workbook Skins vs Signed Coded Extensions

- **Summary:** Enforces unanimous security hygiene across all four Pro 19 sources: skins travelling inside `.xlsx` `customXml` are 100% declarative data with zero executable scripts so macro-free workbooks never become a second macro attack vector, while genuinely new coded UI widgets ship exclusively as signed, host-installed extensions behind the add-in trust card with declarative fallback cards when absent.
- **Cards (8):** `FRAMING-0067`, `P19-CARBON-0088`, `P19-CARBON-0114`, `P19-CARBON-0202`, `P19-FABLE-0192`, `P19-NEON-0080`, `P19-NEON-0107`, `P19-NEON-0117`
- **Sources:** FRAMING (1), P19-CARBON (3), P19-FABLE (1), P19-NEON (3)
- **Disposition:** `core-with-decision` — Make this a highlighted security rule (`R-31`) in Chapter XII. Because DECISIONS_20261126 #7 retains WebView2/Leptos, implement host-installed coded widgets through a sandboxed Web Component / Leptos extension boundary behind the add-in trust card.
- **Decisions:** [D-84](decisions.md#d-84)
- **`same-as` groups (2):** `FRAMING-0067` = `P19-CARBON-0114` = `P19-NEON-0107` = `P19-NEON-0117`; `P19-CARBON-0088` = `P19-FABLE-0192` = `P19-NEON-0080`
- **Agreements:**
  - Workbook-embedded `customXml` skins contain zero executable code—preserving corporate gateway trust in macro-free `.xlsx` files and avoiding macro-style security prompts on open.
  - Coded UI components are permitted only for genuinely new visualisations or instruments (never for theming, layout, keymaps, or standard Apps), must be explicitly installed and signed through the same host trust card as add-ins, receive read-only cell values, emit only standard Layer 2 edit intents, and can never alter calculation.
  - Every manifest referencing an optional coded component must declare a pure-declarative fallback card so the workbook remains usable when the extension is not installed.
- **Conflicts:** None

### CL-12-07: Immutable Core Invariants Across All Skins

- **Summary:** Lists the non-negotiable guarantees no skin or custom component may ever override or obscure: single-evaluator Excel calculation parity, exact address/source identity, reserved semantic signals and shapes, fixed Strip truthfulness readouts, WCAG AA contrast and reduced-motion respect, Escape/Undo/Classic-return recovery, App formula-overwrite protection, clipboard semantics, and macro/link security banners.
- **Cards (6):** `P19-CARBON-0090`, `P19-FABLE-0194`, `P19-FABLE-0510`, `P19-GPT-0071`, `P19-GPT-0291`, `P19-NEON-0082`
- **Sources:** P19-CARBON (1), P19-FABLE (2), P19-GPT (2), P19-NEON (1)
- **Disposition:** `core-with-decision` — Publish as the formal 'Inviolable Core' checklist (`R-61`, `R-93`) enforced by the Skin Validator. Note that `P19-FABLE-0510`'s specific protected Classic-return chord (`Alt+Shift+1` / `Alt+Shift+0`) is superseded by DECISIONS_20261126 #12 (`Ctrl+Alt+1…5` collision-checked, with command search as protected fallback)—protect `Ctrl+Alt+1` (or its collision-free equivalent) and command search instead.
- **Decisions:** [D-12](decisions.md#d-12)
- **`same-as` groups (1):** `P19-CARBON-0090` = `P19-FABLE-0194` = `P19-GPT-0071` = `P19-NEON-0082`
- **Agreements:**
  - No skin may alter Excel calculation parity, formula meaning, workbook content/formats, clipboard semantics, or embed hidden input transforms / secondary calculators.
  - No skin may hide fixed Strip status readouts, conceal stale/cached-vs-live state, rebind the reserved semantic signals (red error, amber focus/pending, signal external/volatile, green verified) and their non-colour shapes, or suppress macro/link quarantine banners and trust cards.
  - No skin may break WCAG AA contrast thresholds, disable reduced-motion, block `Esc` revert / `Ctrl+Z` Undo / 1-click theme reset, or remove the guaranteed return path to Classic view.
- **Conflicts:**
  - *Cross-cluster `conflicts-with` links (resolved by D-12):* `P19-FABLE-0510` vs `FRAMING-0177` (CL-03-04), `P19-GPT-0218` (CL-04-28)

### CL-12-08: Keymap Customisation, Classic Conformance Flagging & Trial Bench

- **Summary:** Allows skins and users to add or remap non-protected chords while keeping Classic Excel conformance transparent and recoverable: core recovery and formula-editing keys are protected from reassignment, collisions block Apply, every departure from the Classic profile is explicitly counted and labelled, an isolated single-cell trial bench tests draft chords with zero workbook writes, and one-step 'Restore Classic' reverts cleanly.
- **Cards (5):** `P19-FABLE-0187`, `P19-FABLE-0511`, `P21-0103`, `P21-0104`, `P21-0105`
- **Sources:** P19-FABLE (2), P21 (3)
- **Disposition:** `core` — Include the Keymap Editor, protected-key table, Classic-departure badge, and isolated trial bench (`U03`) directly in Chapter XII.
- **`same-as` groups (1):** `P19-FABLE-0187` = `P19-FABLE-0511`
- **Agreements:**
  - Adding custom key bindings is free, while overriding any Appendix A / Classic key explicitly flags departure from the Classic conformance profile with the exact count of modified keys (`R-94`) and provides one-step 'Restore Classic' plus presentation Undo.
  - Core editing and recovery commands (`Escape`, `Ctrl+Z`/`Ctrl+Y`, `F4`, `Ctrl+Enter`, `Alt+Q`, plus Classic view return) are protected from reassignment; intra-keymap collisions (e.g., `Ctrl+E` Flash Fill) block Apply, and host/browser shortcut conflicts are checked on install.
  - Remapping a Ready-grid trigger (e.g., `F2 -> Ctrl+Shift+E`) preserves in-edit `F2`/`F4` contexts and is safely tested on an isolated single-cell trial bench where `Esc` cancels with zero workbook writes.
- **Conflicts:** None

### CL-12-09: Customisation Studio, Reversible Preview Seam & Skin Sharing/Import

- **Summary:** Specifies the unified human-and-agent Customisation Studio ('View > Edit this layout', `Alt, W, U`, `Ctrl+K -> Customize Skin`) showing the inspectable `dna.view/1` manifest beside a live preview, with strict binding/contrast validation blocking Apply on errors, non-persistent 'Try in this window' (`Esc` reverts with 0 model writes), single-Undo presentation commits, value-free skin export, and full disclosure on import.
- **Cards (9):** `FRAMING-0156`, `P19-CARBON-0089`, `P19-FABLE-0193`, `P19-FABLE-0509`, `P19-GPT-0072`, `P19-NEON-0081`, `P19-NEON-0232`, `P21-0100`, `P21-0101`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-FABLE (2), P19-GPT (1), P19-NEON (2), P21 (2)
- **Disposition:** `core-with-decision` — Unify all three entry points (`View > Edit this layout`, `Alt, W, U`, and `Ctrl+K -> Customize Skin`) onto the single Customisation Studio (`U01`–`U03`), using the Preview Seam band and strict binding/contrast blocking.
- **Decisions:** [D-82](decisions.md#d-82)
- **`same-as` groups (1):** `P19-CARBON-0089` = `P19-FABLE-0193` = `P19-FABLE-0509` = `P21-0100`
- **Agreements:**
  - All skin edits—whether made in the visual/text Customisation Studio or proposed by an AI agent—enter through a non-persistent live Preview Seam (`Try in this window` / `Esc` to discard with zero model writes) before Review and Apply.
  - Applying a skin commits one reversible presentation transaction (with Undo) saved either for the user or into the workbook's `customXml` view part; 200% zoom reflows the studio editor and live preview vertically.
  - Exporting a skin writes a versioned `.dnaskin.json` package with zero workbook values by default; importing discloses all commands, required coded extensions, and unavailable bindings.
  - Source bindings require both stable identity and exact cell/object address to match; broken bindings stay visibly broken and block Apply with zero silent fallback to a nearby cell (`P21-0101`, `P19-GPT-0072`).
- **Conflicts:**
  - **Contrast-check failure behaviour: reject/block Apply with fix hints vs silently auto-adjust failing tokens** (`P19-CARBON-0089`, `P19-FABLE-0193`, `P19-FABLE-0509`, `P21-0100`, `P21-0101` vs `P19-NEON-0081`) → Resolved by **[D-82](decisions.md#d-82)**

### CL-12-10: Published Skin SDK & Automated Agent Verification Harness

- **Summary:** Packages the skinning surface into a four-part SDK—view contract, component library, manifest schema/loader, and an automated verification harness that loads any workbook, screenshots every view, and runs the keyboard/gesture conformance list—enabling humans and AI agents to author, verify, and ship skins objectively.
- **Cards (3):** `FRAMING-0019`, `FRAMING-0038`, `FRAMING-0127`
- **Sources:** FRAMING (3)
- **Disposition:** `core` — Document the four-part SDK contract in Chapter XII and tie its automated harness directly to the corpus/conformance harness in Chapter XV (`FRAMING-0039`).
- **`same-as` groups (1):** `FRAMING-0019` = `FRAMING-0038`
- **Agreements:**
  - The Skin SDK exposes four concrete deliverables: the Layer 2 view contract, the Strand component library, the `dna.view/1` manifest schema and loader (plus extension boundary), and an automated verification harness.
  - The harness closes the autonomous agent customisation loop by loading real workbooks, rendering view screenshots, running contrast/binding validation, and verifying keyboard/gesture conformance—validated against the two canonical worked examples (Classic restyle and Loan App).
- **Conflicts:** None

### CL-12-11: Worked Example 1 — Agent Restyling Classic View

- **Summary:** Validates declarative restyling via an agent-authored Classic skin (e.g., dark audit/review variant): computes its real diff (`Changes: presentation only; 0 cells, formulas or names`), passes all nine Strand sRGB contrast checks, preserves Classic keymap conformance and editing gestures, refuses to mutate workbook row heights or cell formatting, and commits/reverts as one presentation Undo step.
- **Cards (6):** `P19-CARBON-0091`, `P19-FABLE-0195`, `P19-GPT-0073`, `P19-GPT-0290`, `P19-NEON-0083`, `P21-0106`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-GPT (2), P19-NEON (1), P21 (1)
- **Disposition:** `core-with-decision` — Present Example 1 (`U04` dark Classic restyle + explicit row-height refusal from Fable) as the canonical acceptance test for `R-60` and the Skin SDK.
- **Decisions:** [D-85](decisions.md#d-85)
- **`same-as` groups (1):** `P19-GPT-0073` = `P19-GPT-0290` = `P21-0106`
- **Agreements:**
  - Every Classic restyle proposal previews side-by-side through the same Review/Apply/Undo workflow as human studio edits, explicitly reporting `0 model writes` computed from the real diff (`R-60`).
  - Drafts that clip cell addresses, fail any of the 9 Strand sRGB contrast pairs (e.g., dark secondary ink `#7D97A0` at `4.19:1` rejected in favour of `#A4B8C2` at `6.29:1`), or break Classic editing semantics (`F2`/`F4`/`Ctrl+Enter`/fill-handle) are blocked before Apply.
- **Conflicts:**
  - **Strict presentation-only Classic restyle preserving Classic keymap & workbook row heights vs setting pixel row heights and adding formatting keybindings in the restyle example** (`P19-FABLE-0195`, `P19-GPT-0073`, `P19-GPT-0290`, `P21-0106` vs `P19-NEON-0083`, `P19-CARBON-0091`) → Resolved by **[D-85](decisions.md#d-85)**

### CL-12-12: Worked Example 2 — Agent Building Loan App from Three-Line Brief

- **Summary:** Validates zero-code declarative App authoring: from a three-line prompt, an agent generates a `dna.view/1` App layout disclosing exact Read/Write workbook bindings, tab order, and reused Excel Stop validation (never a second `PMT` calculation), tests trial inputs in strict isolation so accepting stores presentation only without leaking trial values, blocks Apply on any invalid address without approximate fallback, and routes any suggested Define Name / Validation / Scenario additions to separate model Changes proposals.
- **Cards (7):** `P19-CARBON-0092`, `P19-FABLE-0196`, `P19-GPT-0074`, `P19-NEON-0084`, `P21-0107`, `P21-0108`, `P21-0109`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-GPT (1), P19-NEON (1), P21 (3)
- **Disposition:** `core-with-decision` — Present Example 2 (`U05`) as the second canonical Skin SDK acceptance test, highlighting isolated trial inputs, broken-binding Apply blocking, and separation of suggested workbook names/scenarios into model proposals.
- **Decisions:** [D-85](decisions.md#d-85)
- **`same-as` groups (2):** `P19-CARBON-0092` = `P19-NEON-0084`; `P19-FABLE-0196` = `P19-GPT-0074` = `P21-0107`
- **Agreements:**
  - A three-line brief produces a zero-code declarative App layout stored in workbook `customXml` (preferred view `app`, sliders/inputs, KPIs, baseline-overlaid existing chart, expandable/rolled-up schedule, and print preset) without writing any secondary formula logic.
  - The proposal explicitly discloses every Read vs Write workbook address (`Loan!B2:B4` Write, `Loan!B6:B7` Read, `loan.balanceChart`, `A12:F371`), reuses workbook Stop validation, runs trial inputs in complete isolation (never writing trial numbers on layout acceptance), and blocks Apply with withheld previews if any binding address is invalid (`P21-0109`).
  - Any suggested workbook additions (such as Define Name, new Excel Data Validation rules, or Excel Scenario records) are strictly separated into model Changes proposals so presentation and model edits never mix (`P19-FABLE-0196`, `P19-GPT-0074`).
- **Conflicts:**
  - **Loan App worked example manifest format & binding discipline: canonical JSON with explicit Read/Write disclosure and trial isolation vs YAML shorthand** (`P19-GPT-0074`, `P19-FABLE-0196`, `P21-0107`, `P21-0108`, `P21-0109` vs `P19-NEON-0084`) → Resolved by **[D-85](decisions.md#d-85)**

## XIII. Persistence

### CL-13-01: CustomXML View Manifest, Schema Versioning & Excel Round-Trip

- **Summary:** All DNA Calc view preferences, per-sheet layouts, prose, and presentation metadata travel inside standard .xlsx/.xlsm workbooks via a single versioned OOXML customXml part (`urn:dnacalc:views:v1`) implemented by one unified reader/writer in calc-doc, preserving unknown parts and future schema versions opaquely with zero Excel repair warnings.
- **Cards (12):** `FRAMING-0013`, `FRAMING-0037`, `FRAMING-0077`, `FRAMING-0105`, `P19-CARBON-0220`, `P19-FABLE-0522`, `P19-GPT-0325`, `P19-GPT-0326`, `P19-NEON-0199`, `P19-NEON-0241`, `P21-0093`, `P21-0094`
- **Sources:** FRAMING (4), P19-CARBON (1), P19-FABLE (1), P19-GPT (2), P19-NEON (2), P21 (2)
- **Disposition:** `core-with-decision` — Note that FRAMING-0013 and FRAMING-0037's early mention of hidden defined-name anchors is explicitly superseded by DECISIONS_20261126 #13 (handled in CL-13-02).
- **Decisions:** [D-13](decisions.md#d-13), [D-83](decisions.md#d-83)
- **`same-as` groups (3):** `P19-CARBON-0220` = `P19-NEON-0241`; `P19-FABLE-0522` = `P19-NEON-0199`; `P19-GPT-0326` = `P21-0094`
- **Agreements:**
  - One unified view-manifest reader and writer in calc-doc serves every view, grounded in verified OOXML format references (FRAMING-0037, FRAMING-0077).
  - Spatial/view layouts (Notebook, Model Map, Linked Boxes, App, block overrides, declarative skins) are strictly presentation metadata in an optional customXml part, never polluting the Excel calculation model (FRAMING-0105, P19-GPT-0325, P19-NEON-0199).
  - Saved workbooks open in Microsoft Excel with zero repair prompts and identical calculated values while preserving unrelated workbook parts untouched (P19-FABLE-0522, P19-GPT-0325, P19-NEON-0199).
  - Unsupported future view-part versions (e.g., v2/v3) are retained opaquely and never overwritten or silently discarded by older DNA Calc builds, with Classic always available as the ordinary-reader fallback (P19-GPT-0326, P21-0094).
- **Conflicts:**
  - **CustomXML part packaging, namespace & payload encoding (OOXML /customXml/itemN.xml + urn:dnacalc:views:v1 vs fixed path / arbitrary JSON blobs)** (`P21-0093`, `P21-0094` vs `FRAMING-0013`, `P19-CARBON-0220`, `P19-NEON-0241`) → Resolved by **[D-83](decisions.md#d-83)**
  - *Cross-cluster `conflicts-with` links (resolved by D-13, D-83):* `FRAMING-0013` vs `FRAMING-0178` (CL-13-02), `P19-CARBON-0086` (CL-12-05), `P19-FABLE-0232` (CL-13-02); `FRAMING-0037` vs `FRAMING-0178` (CL-13-02), `P19-FABLE-0232` (CL-13-02); `P19-CARBON-0220` vs `P19-CARBON-0086` (CL-12-05); `P19-NEON-0241` vs `P19-CARBON-0086` (CL-12-05)

### CL-13-02: Anchor Strategy & Structural Reconciliation After External Edits

- **Summary:** DNA Calc never adds hidden or synthetic defined names to a workbook. All view anchors live exclusively in the customXml view part, referencing existing user-created visible names/tables where present and pairing addresses with lightweight header/label and R1C1 formula fingerprints to reconcile moved blocks after desktop Excel edits and flag uncertain bindings as visible orphans.
- **Cards (8):** `FRAMING-0178`, `P19-CARBON-0117`, `P19-CARBON-0119`, `P19-CARBON-0203`, `P19-CARBON-0221`, `P19-FABLE-0226`, `P19-FABLE-0232`, `P19-GPT-0075`
- **Sources:** FRAMING (1), P19-CARBON (4), P19-FABLE (2), P19-GPT (1)
- **Disposition:** `core-with-decision` — Specify the exact anchor tuple and reconciliation states (exact match, auto-reconciled with notice, uncertain/orphan requiring rebind) in Chapter XIII.
- **Decisions:** [D-13](decisions.md#d-13)
- **`same-as` groups (2):** `FRAMING-0178` = `P19-FABLE-0232`; `P19-CARBON-0117` = `P19-CARBON-0221` = `P19-FABLE-0226`
- **Cross-cluster `same-as`:** `FRAMING-0178` = `P19-FABLE-0567` (CL-02-21)
- **Agreements:**
  - Every view anchor pairs human-readable coordinates with stable binding identity (existing user Defined Name or Table reference when present, plus header/label and R1C1 formula fingerprints) because a surviving A1 address after external Excel edits does not prove the same logical target survived (P19-CARBON-0117, P19-CARBON-0221, P19-FABLE-0226, P19-GPT-0075).
  - When rows or columns are inserted/deleted in desktop Excel, DNA Calc re-matches unambiguously shifted blocks on reopen with a quiet notice, and holds ambiguous or broken bindings as visible orphans prompting user review/rebinding rather than silently attaching to wrong cells (P19-FABLE-0226, P19-GPT-0075).
- **Conflicts:**
  - **Hidden / synthetic defined names vs CustomXML-only anchors** (`FRAMING-0178`, `P19-FABLE-0232` vs `P19-CARBON-0119`, `P19-CARBON-0203`) → Resolved by **[D-13](decisions.md#d-13)**
  - *Cross-cluster `conflicts-with` links (resolved by D-13):* `FRAMING-0178` vs `FRAMING-0013` (CL-13-01), `FRAMING-0037` (CL-13-01); `P19-FABLE-0232` vs `FRAMING-0013` (CL-13-01), `FRAMING-0037` (CL-13-01)

### CL-13-03: Notes, Findings & Agent Change-Trail Persistence

- **Summary:** User notes, review findings, Notebook prose, and the structured agent change trail are persisted inside the same workbook customXml part (`urn:dnacalc:views:v1`), with an opt-out toggle for the agent change trail, local-only transient undo/recovery, and an optional user-invoked Excel-visible mirror for findings and prose.
- **Cards (6):** `FRAMING-0170`, `FRAMING-0179`, `P19-FABLE-0141`, `P19-FABLE-0219`, `P19-FABLE-0233`, `P19-GPT-0098`
- **Sources:** FRAMING (2), P19-FABLE (3), P19-GPT (1)
- **Disposition:** `core-with-decision` — Both notes/findings and the structured agent change trail share the same customXml part and carry the owner's 'ship now, improve with experience' guidance.
- **Decisions:** [D-05](decisions.md#d-05), [D-14](decisions.md#d-14)
- **`same-as` groups (1):** `P19-FABLE-0141` = `P19-FABLE-0233`
- **Cross-cluster `same-as`:** `FRAMING-0170` = `P19-FABLE-0114` (CL-05-12)
- **Agreements:**
  - Accepted agent edits become ordinary named undo steps in session history, while transient undo/recovery stays local rather than bloating the saved workbook (P19-FABLE-0141, P19-GPT-0098).
  - Any persisted agent provenance stores only a compact structured change summary (who, when, proposal summary, affected cells/diffs) rather than raw chat transcripts, and includes a privacy/opt-out control (P19-FABLE-0233, P19-GPT-0098).
- **Conflicts:**
  - **Agent change trail persistence default (persisted in customXml with opt-out vs OFF by default opt-in)** (`FRAMING-0179`, `P19-GPT-0098` vs `P19-FABLE-0141`, `P19-FABLE-0233`) → Resolved by **[D-14](decisions.md#d-14)**
  - **Notes, findings & prose storage (customXml default vs Excel-visible mirror)** (`FRAMING-0170` vs `P19-FABLE-0219`) → Resolved by **[D-05](decisions.md#d-05)**
  - *Cross-cluster `conflicts-with` links (resolved by D-05, D-14):* `FRAMING-0170` vs `P19-FABLE-0070` (CL-05-12); `FRAMING-0179` vs `P19-CARBON-0045` (CL-02-17), `P19-NEON-0100` (CL-15-05), `P19-NEON-0123` (CL-16-04); `P19-FABLE-0141` vs `P19-FABLE-0497` (CL-02-17), `P21-0113` (CL-02-17); `P19-FABLE-0219` vs `P19-FABLE-0114` (CL-05-12); `P19-FABLE-0233` vs `P19-FABLE-0497` (CL-02-17), `P21-0113` (CL-02-17); `P19-GPT-0098` vs `P19-CARBON-0045` (CL-02-17), `P19-NEON-0100` (CL-15-05), `P19-NEON-0123` (CL-16-04)

### CL-13-04: Per-Sheet View Preferences vs Tab-Click Switching Behaviour

- **Summary:** Workbooks declare preferred and supported views at workbook and sheet granularity, with Classic universally guaranteed and user session overrides outranking file defaults. During authoring, clicking a sheet tab keeps the current view pinned and shows the sheet's preferred view as a tab badge and one-click suggestion pill rather than auto-switching unexpectedly.
- **Cards (7):** `FRAMING-0014`, `P19-FABLE-0221`, `P19-GPT-0034`, `P19-GPT-0244`, `P19-NEON-0103`, `P19-NEON-0114`, `P21-0092`
- **Sources:** FRAMING (1), P19-FABLE (1), P19-GPT (2), P19-NEON (2), P21 (1)
- **Disposition:** `core-with-decision` — Align Chapter III (CL-03-09) and Chapter XIII on this exact tab-pinning rule so both chapters describe identical tab-click and Bridge-pill behaviour.
- **Decisions:** [D-86](decisions.md#d-86)
- **`same-as` groups (2):** `P19-FABLE-0221` = `P19-NEON-0103` = `P19-NEON-0114`; `P19-GPT-0034` = `P19-GPT-0244` = `P21-0092`
- **Cross-cluster `same-as`:** `P19-FABLE-0221` = `FRAMING-0064` (CL-03-09); `P19-NEON-0103` = `FRAMING-0064` (CL-03-09)
- **Agreements:**
  - A workbook may declare a preferred view, per-sheet preferred views, and supported views, yet every view must still open every workbook (undesigned views appearing plain) and Classic is always available (FRAMING-0014, P19-GPT-0034, P21-0092).
  - Explicit local user view choices always override saved file defaults, and a missing per-sheet layout falls back to Classic at the exact selected source while preserving the unresolved preference without substituting a neighbouring sheet's layout (P19-GPT-0244, P21-0092).
- **Conflicts:**
  - **Sheet-tab click behaviour: pinned active view with badge/pill vs automatic per-sheet view switch** (`P19-FABLE-0221`, `P19-NEON-0103`, `P19-NEON-0114` vs `P19-GPT-0034`, `P19-GPT-0244`, `P21-0092`) → Resolved by **[D-86](decisions.md#d-86)**

### CL-13-05: Real Excel Backing Cells, Native Charts & Date Serial Fidelity

- **Summary:** Every computation authored in non-Classic views (especially Notebook) compiles into ordinary Excel cells, spill-safe backing ranges, dynamic workbook Defined Names, native Excel chart objects, and real Excel date serial numbers so Excel evaluates identical results without DNA Calc.
- **Cards (3):** `FRAMING-0151`, `P21-0021`, `P21-0034`
- **Sources:** FRAMING (1), P21 (2)
- **Disposition:** `core` — Highlight this as the cornerstone of 'Super Excel-compatible in the engine, modern in the presentation'—zero proprietary formulas or pseudo-objects ever enter the worksheet XML.
- **`same-as` groups:** None within cluster
- **Agreements:**
  - Every Notebook computing cell maps to a real cell or defined name on a visible backing sheet (e.g., Analysis) in reading order with explicit spill-headroom management, outlined in Classic, while prose stays in the view part (FRAMING-0151).
  - Notebook charts persist as ordinary native Excel chart objects referencing dynamic workbook Defined Names (e.g., INDEX+ROWS over ranked spills), never as invented worksheet chart functions (P21-0021).
  - Dates persist as genuine Excel date serial numbers in native .xlsx/.xlsm files rather than ISO date strings (P21-0034).
- **Conflicts:** None

### CL-13-06: Save Disclosures, Stripped-Part Resilience & Save-Route Certification

- **Summary:** Saving communicates transparently what stays standard Excel versus what travels in customXml (and notes add-in `#NAME?` portability caveats), guarantees full workbook self-sufficiency if an external tool strips customXml, and grounds round-trip promises in an explicit tested save-route matrix rather than untested assumptions.
- **Cards (8):** `P19-FABLE-0060`, `P19-FABLE-0083`, `P19-FABLE-0104`, `P19-FABLE-0523`, `P19-GPT-0091`, `P19-GPT-0292`, `P19-NEON-0111`, `P21-0125`
- **Sources:** P19-FABLE (4), P19-GPT (2), P19-NEON (1), P21 (1)
- **Disposition:** `core-with-decision` — Important Ruling #6 cleanup: P19-FABLE-0060 mentions setting 'Notebook (Reading density)' as preferred view on save; under DECISIONS_20261126 #6 Notebook works like Jupyter with no separate Read-mode presentation, so saving a Notebook workbook sets preferred view to `Notebook`.
- **Decisions:** [D-06](decisions.md#d-06), [D-87](decisions.md#d-87)
- **`same-as` groups (2):** `P19-FABLE-0523` = `P19-NEON-0111`; `P19-GPT-0091` = `P21-0125`
- **Agreements:**
  - First-save notices transparently reassure users that Notebook, Model Map, and standard models save as ordinary Excel workbooks whose F9 recalculation matches identically in Excel, while flagging add-in portability caveats (`#NAME?` where an XLL is not installed) without blocking save (P19-FABLE-0060, P19-FABLE-0083, P19-FABLE-0104).
  - Every workbook remains 100% self-sufficient without its customXml view part: if Document Inspector, LibreOffice, Google Sheets, or an ETL script strips the part, the model stays intact and auto-inference can reconstruct every view cleanly (P19-FABLE-0523, P19-NEON-0111).
  - Round-trip preservation of customXml, unknown parts, and VBA projects must be backed by a certified native Excel save-route matrix (.xlsx/.xlsm across named desktop/web Excel versions) rather than assumed from prototype JSON tests (P19-GPT-0091, P21-0125).
- **Conflicts:**
  - **Fallback landing view when customXml is missing or stripped (open in Classic vs auto-inferred view)** (`P19-GPT-0292` vs `P19-FABLE-0523`, `P19-NEON-0111`) → Resolved by **[D-87](decisions.md#d-87)**
  - *Cross-cluster `conflicts-with` links (resolved by D-06, D-87):* `P19-FABLE-0060` vs `FRAMING-0054` (CL-06-01), `FRAMING-0056` (CL-06-01), `FRAMING-0145` (CL-06-01), `FRAMING-0171` (CL-06-01), `P21-0014` (CL-06-01)

### CL-13-07: Local Crash-Recovery Journal

- **Summary:** Uncommitted model edits and customXml view edits journal incrementally to local storage so relaunching after an abnormal exit presents a non-modal [Restore] [Discard] recovery bar without corrupting the last saved workbook.
- **Cards (2):** `P19-NEON-0237`, `P19-NEON-0242`
- **Sources:** P19-NEON (2)
- **Disposition:** `core` — Keep recovery journals strictly local (never written inside the .xlsx file), matching CL-13-03's local-only transient history policy.
- **`same-as` groups:** None within cluster
- **Agreements:**
  - Both workbook model edits and customXml view edits stream to an incremental local recovery journal (P19-NEON-0242).
  - On relaunch after a crash or unexpected termination, recovery is surfaced via a non-modal [Restore] [Discard] bar rather than a blocking modal dialog (P19-NEON-0237).
- **Conflicts:** None

### CL-13-08: Native Workbook Structured Objects Metadata

- **Summary:** Defines the calc-doc / calc-host read-write contracts for structured Excel workbook objects—ListObject Tables, PivotTables, What-If Data Tables, Scenarios (including non-mutating scenario summaries), Form Controls, Charts/Shapes/Pictures, and Excel PageSetup/pagination—so every view operates on genuine Excel objects.
- **Cards (7):** `P19-CARBON-0219`, `P19-FABLE-0547`, `P19-FABLE-0548`, `P19-FABLE-0549`, `P19-FABLE-0550`, `P19-FABLE-0556`, `P19-FABLE-0557`
- **Sources:** P19-CARBON (1), P19-FABLE (6)
- **Disposition:** `core` — Map directly into the calc-doc / Skin Host capability contract table so views reuse real Excel objects rather than parallel mockups.
- **`same-as` groups:** None within cluster
- **Cross-cluster `same-as`:** `P19-FABLE-0557` = `P19-GPT-0328` (CL-03-10)
- **Agreements:**
  - Tables expose name, range, typed columns, totals row, filter/sort state, and growth notifications (P19-CARBON-0219, P19-FABLE-0547).
  - PivotTables expose source ranges, fields/axes, cached values, last refresh timestamp, and source-changed staleness (P19-CARBON-0219, P19-FABLE-0548).
  - What-If `{=TABLE}` data tables and Scenarios expose changing cells/values with apply, create, and sheet-free summary generation (P19-CARBON-0219, P19-FABLE-0549).
  - Form controls expose linked cells, limits, and steps for direct user-equivalent writes; charts, shapes, and pictures expose names, source ranges, and rendering; PageSetup exposes exact Excel pagination (P19-CARBON-0219, P19-FABLE-0550, P19-FABLE-0556, P19-FABLE-0557).
- **Conflicts:** None

### CL-13-09: Cell-Level Formats, Validation, Protection, Visibility & Comments Metadata

- **Summary:** Defines the calc-doc / calc-host contracts for cell- and sheet-level Excel semantics: validation rules and candidate testing, conditional-format firing order, exact Excel number formatting (stored vs shown), cell/sheet/workbook protection permissions, non-mutating reads of hidden/VeryHidden sheets, and native comments/notes.
- **Cards (6):** `P19-FABLE-0551`, `P19-FABLE-0552`, `P19-FABLE-0553`, `P19-FABLE-0554`, `P19-FABLE-0555`, `P19-FABLE-0558`
- **Sources:** P19-FABLE (6)
- **Disposition:** `core` — Pair with CL-13-08 in the System-Needs appendix; emphasize non-mutating reads of VeryHidden sheets and exact stored-vs-displayed formatting fidelity.
- **`same-as` groups:** None within cluster
- **Cross-cluster `same-as`:** `P19-FABLE-0555` = `P19-NEON-0025` (CL-03-08)
- **Agreements:**
  - Validation rules expose messages and allow testing candidate values before write; conditional formats expose applicable rules, evaluation order, and which rules fired (P19-FABLE-0551, P19-FABLE-0552).
  - Number formatting renders any value under any Excel format code (colours and locale included) and distinguishes stored value from displayed text (P19-FABLE-0553).
  - Protection flags (locked/hidden, sheet and workbook protection permissions), sheet visibility (including non-mutating reads of Hidden and VeryHidden sheets), and native comments/notes are fully exposed through the host contract (P19-FABLE-0554, P19-FABLE-0555, P19-FABLE-0558).
- **Conflicts:** None

## XIV. Automation

### CL-14-01: Macro Security Levels (Off / Guarded / Unguarded) & Quarantine Inspection

- **Summary:** Macro security is strictly host-controlled across exactly three levels—Off, Guarded, and Unguarded—with zero auto-execution on open, read-only source inspection, inline refusal cards naming the blocked procedure and its actions, and honest boundaries around static preflight for arbitrary VBA.
- **Cards (10):** `FRAMING-0159`, `P19-CARBON-0099`, `P19-CARBON-0208`, `P19-FABLE-0048`, `P19-FABLE-0514`, `P19-NEON-0262`, `P21-0115`, `P21-0116`, `P21-0118`, `P21-0119`
- **Sources:** FRAMING (1), P19-CARBON (2), P19-FABLE (2), P19-NEON (1), P21 (4)
- **Disposition:** `core-with-decision` — Make Off / Guarded / Unguarded the canonical terminology throughout the entire Design Book, and pair it with Fable's inline refusal card and Pro 21's honest preflight disclaimer.
- **Decisions:** [D-88](decisions.md#d-88)
- **`same-as` groups (3):** `FRAMING-0159` = `P21-0115`; `P19-CARBON-0099` = `P19-NEON-0262`; `P19-FABLE-0048` = `P19-FABLE-0514`
- **Agreements:**
  - On workbook open, the host inventories VBA modules, external links (and reachability), RTD topics, and add-in UDF calls up front while holding macros and broken links inactive (P19-CARBON-0208, P19-NEON-0262).
  - Security authority belongs strictly to the host—never to a theme, App view, or workbook layout—and protected cells block writes before execution (P21-0115).
  - Clicking a blocked button never fails silently: it opens a clear inline card naming the target procedure, showing a static summary/source link, and explaining why execution was refused (P19-FABLE-0048, P19-FABLE-0514).
  - Unguarded never fabricates a missing VBA runtime or external capability on hosts that lack them (P21-0115).
- **Conflicts:**
  - **Three-tier host security (Off / Guarded / Unguarded) vs binary Blocked / Session-Trusted toggle** (`FRAMING-0159`, `P21-0115`, `P21-0116` vs `P19-CARBON-0099`, `P19-FABLE-0048`, `P19-FABLE-0514`, `P19-NEON-0262`) → Resolved by **[D-88](decisions.md#d-88)**
  - **Static preflight & all-or-nothing batch rollback for arbitrary VBA** (`P21-0119` vs `P21-0118`) → Resolved by **[D-88](decisions.md#d-88)**

### CL-14-02: Excel VBA Object Model Compatibility Across Views

- **Summary:** Legacy Excel VBA macros and add-ins operate through the preserved Excel object model across all views—with Classic exposing Selection, ActiveCell, ActiveWindow, and StatusBar, selection verbs moving the shared selection, dialogs appearing over any view, and ScreenUpdating=False batching repaints without hiding running state.
- **Cards (4):** `FRAMING-0016`, `P19-CARBON-0100`, `P19-FABLE-0204`, `P19-FABLE-0562`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-FABLE (2)
- **Disposition:** `core` — Note that P19-CARBON-0100's reference to pushing a single compound Macro Undo checkpoint is governed by Owner Ruling #4 in CL-14-04 (architect for it, do not promise universally in v1).
- **`same-as` groups (1):** `P19-CARBON-0100` = `P19-FABLE-0204`
- **Agreements:**
  - VBA and add-ins preserve Excel's object model, with Classic exposing UI members (Selection, ActiveCell, ActiveWindow, StatusBar) cleanly (FRAMING-0016).
  - Existing VBA verbs keep their meaning in every view: Select/Activate/Application.Goto move the shared selection, MsgBox/InputBox/UserForms render over any view, Application.StatusBar shows labelled in the Strip, and ScreenUpdating=False batches repaints while keeping 'Macro running · Esc interrupts' visible (P19-CARBON-0100, P19-FABLE-0204).
  - The host exposes read-only module/procedure/control-binding/OM-call inspection, interruptible execution, and observed post-run change summaries (P19-FABLE-0562).
- **Conflicts:** None

### CL-14-03: DNA Calc Views Automation Library & Guardrails

- **Summary:** Provides a tiny, optional, Excel-safe presentation automation surface for VBA macros—three core verbs (SwitchView, Select, ShowPanel) reachable via late-bound COM or Application.Run—subject to strict UI safety guardrails and guaranteed zero impact on Excel calculation parity.
- **Cards (12):** `FRAMING-0017`, `FRAMING-0128`, `P19-CARBON-0101`, `P19-FABLE-0205`, `P19-FABLE-0206`, `P19-FABLE-0516`, `P19-FABLE-0563`, `P19-GPT-0078`, `P19-GPT-0295`, `P19-NEON-0092`, `P19-NEON-0236`, `P21-0117`
- **Sources:** FRAMING (2), P19-CARBON (1), P19-FABLE (4), P19-GPT (2), P19-NEON (2), P21 (1)
- **Disposition:** `core-with-decision` — Publish the exact 3-verb specification, its late-bound VBA examples (`Application.Run` and `CreateObject`), and the hard security guardrail checklist in Chapter XIV; list `SetFormulaLens` and `ShowNoticeBanner` in Appendix Z.
- **Decisions:** [D-89](decisions.md#d-89)
- **`same-as` groups (3):** `P19-CARBON-0101` = `P19-FABLE-0205` = `P19-FABLE-0516`; `P19-GPT-0078` = `P19-GPT-0295`; `P19-NEON-0092` = `P19-NEON-0236`
- **Agreements:**
  - Macro security applies uniformly across all five views, and calculation macros must never depend on DNA-only presentation calls to calculate identically in Excel (P19-GPT-0078, P19-GPT-0295).
  - View/selection automation requests record as navigation (not workbook Undo steps), never silently alter calculation mode, fall back cleanly in Excel, and defer view switches requested during an active user cell edit until commit (P19-FABLE-0206, P19-GPT-0078, P21-0117).
  - Strict UI safety guardrails forbid macros from dismissing trust cards, spoofing security/error banners, hiding fixed Strip readouts or the View Switcher, changing skins, violating contrast/safety tokens, bypassing App formula protection, or bypassing the Agent Preview Seam (P19-CARBON-0101, P19-FABLE-0206, P19-NEON-0092, P19-NEON-0236).
  - Macro code is preserved intact even on hosts that cannot execute it (P19-GPT-0078).
- **Conflicts:**
  - **Invocation mechanism & Excel portability (late-bound Application.Run vs dedicated COM Views library)** (`P19-CARBON-0101`, `P19-FABLE-0205`, `P19-FABLE-0516` vs `FRAMING-0017`, `P21-0117`) → Resolved by **[D-89](decisions.md#d-89)**
  - **Views API surface scope (minimal 3 verbs vs expanded 6 verbs)** (`FRAMING-0017`, `FRAMING-0128`, `P19-CARBON-0101`, `P19-FABLE-0205`, `P19-FABLE-0516`, `P19-GPT-0078`, `P19-GPT-0295`, `P21-0117` vs `P19-NEON-0092`, `P19-NEON-0236`) → Resolved by **[D-89](decisions.md#d-89)**

### CL-14-04: Undo Across Macro Runs, Checkpoints & Observed-Effect History

- **Summary:** Architects macro-run boundaries and recoverable copy-on-write workbook checkpoints so pure workbook-mutating runs can become single-step undoable over time, without promising universal macro Ctrl+Z in v1, while honestly disclosing non-undoable runs and keeping external side effects strictly outside document undo.
- **Cards (8):** `FRAMING-0169`, `P19-CARBON-0120`, `P19-CARBON-0206`, `P19-FABLE-0207`, `P19-FABLE-0515`, `P19-GPT-0079`, `P19-GPT-0296`, `P19-GPT-0324`
- **Sources:** FRAMING (1), P19-CARBON (2), P19-FABLE (2), P19-GPT (3)
- **Disposition:** `core-with-decision` — Keep Fable's 'Macro running · Esc interrupts' requirement (P19-FABLE-0515) as a hard v1 invariant while guarding full Ctrl+Z macro undo behind the post-v1 checkpoint roadmap per Owner Ruling #4.
- **Decisions:** [D-04](decisions.md#d-04)
- **`same-as` groups (3):** `P19-CARBON-0120` = `P19-CARBON-0206`; `P19-FABLE-0207` = `P19-FABLE-0515`; `P19-GPT-0079` = `P19-GPT-0296`
- **Agreements:**
  - The host tracks macro run boundaries, observed workbook effects, and navigation/focus changes (announcing macro-driven focus moves and recording them in navigation history) (P19-GPT-0079, P19-GPT-0324).
  - 'Macro running · Esc interrupts' remains visibly displayed in the Strip for the full duration of a macro run even when `ScreenUpdating = False` (P19-FABLE-0515).
  - External side effects (file writes, network calls, COM automation outside the workbook) are strictly distinguished from document edits and never portrayed as reversible via workbook Undo (P19-GPT-0079, P19-GPT-0296).
- **Conflicts:**
  - **v1 promise of single-step Ctrl+Z undo across VBA macro runs vs design-for-it checkpoint architecture** (`FRAMING-0169`, `P19-GPT-0079`, `P19-GPT-0296` vs `P19-CARBON-0120`, `P19-CARBON-0206`, `P19-FABLE-0207`, `P19-FABLE-0515`) → Resolved by **[D-04](decisions.md#d-04)**
  - *Cross-cluster `conflicts-with` links (resolved by D-04):* `FRAMING-0169` vs `P19-CARBON-0128` (CL-16-04), `P19-CARBON-0223` (CL-02-20), `P19-CARBON-0241` (CL-10-16), `P19-FABLE-0132` (CL-02-10), `P19-FABLE-0394` (CL-04-30), `P19-FABLE-0500` (CL-02-10); `P19-CARBON-0120` vs `P19-GPT-0038` (CL-02-10), `P19-GPT-0205` (CL-04-30), `P19-GPT-0207` (CL-04-30), `P19-GPT-0312` (CL-02-20); `P19-CARBON-0206` vs `P19-GPT-0038` (CL-02-10), `P19-GPT-0205` (CL-04-30), `P19-GPT-0207` (CL-04-30), `P19-GPT-0312` (CL-02-20); `P19-FABLE-0207` vs `P19-GPT-0038` (CL-02-10), `P19-GPT-0205` (CL-04-30), `P19-GPT-0207` (CL-04-30), `P19-GPT-0312` (CL-02-20); `P19-FABLE-0515` vs `P19-GPT-0038` (CL-02-10), `P19-GPT-0205` (CL-04-30), `P19-GPT-0207` (CL-04-30), `P19-GPT-0312` (CL-02-20); `P19-GPT-0079` vs `P19-CARBON-0128` (CL-16-04), `P19-CARBON-0223` (CL-02-20), `P19-CARBON-0241` (CL-10-16), `P19-FABLE-0132` (CL-02-10), `P19-FABLE-0394` (CL-04-30), `P19-FABLE-0500` (CL-02-10) (+1 more cards)

### CL-14-05: Add-In Host, UDF Provenance & Non-Locking Diagnostics

- **Summary:** Defines the host contract for loading, unloading, and reloading add-ins (.xll, JS/WASM) without locking binaries on disk or crashing the app, enumerating UDF signatures/help/volatility/thread-safety metadata, preserving portable `_xll.` prefixes and cached values, tagging dependent cells with provenance, and surfacing per-call timing and error diagnostics.
- **Cards (3):** `P19-CARBON-0224`, `P19-FABLE-0561`, `P19-NEON-0261`
- **Sources:** P19-CARBON (1), P19-FABLE (1), P19-NEON (1)
- **Disposition:** `core` — Connects directly to calc-addin-host and Formula Explorer's add-in UDF testing workflow.
- **`same-as` groups (1):** `P19-CARBON-0224` = `P19-FABLE-0561` = `P19-NEON-0261`
- **Agreements:**
  - Add-ins load, unload, and hot-reload without locking their files on disk or bringing down the application if an add-in fails (P19-CARBON-0224, P19-FABLE-0561, P19-NEON-0261).
  - The host exposes add-in trust/signature metadata, registered UDF names, argument descriptions, return types, thread-safety and volatility flags, cell-level provenance, call timing, and rich per-argument error diagnostics (P19-CARBON-0224, P19-FABLE-0561, P19-NEON-0261).
  - Saved `.xlsx` files preserve portable `_xll.` prefixes alongside last-calculated cached results so Excel compatibility stays intact (P19-CARBON-0224).
- **Conflicts:** None

### CL-14-06: External Links, Volatile Functions & Live RTD Feed Telemetry

- **Summary:** Exposes complete outside-data observability for the shell and views: external workbook link targets/reachability/cached values/dependents, volatile and add-in function inventories, and live RTD/async feed topics with timestamps, update frequencies, event streams, and a non-mutating UI-side visual pause/throttle hook.
- **Cards (4):** `P19-FABLE-0559`, `P19-FABLE-0560`, `P19-NEON-0259`, `P19-NEON-0260`
- **Sources:** P19-FABLE (2), P19-NEON (2)
- **Disposition:** `core` — Connects directly to Owner Ruling #3 (DECISIONS_20261126 #3) on RTD-clocked simulation models and macro-ticked models, feeding Inspector and Strip outside-data readouts.
- **`same-as` groups:** None within cluster
- **Agreements:**
  - External workbook links report target path, resolved vs missing status, last-saved cached values, last update time, and all dependent cells (P19-FABLE-0559, P19-NEON-0259).
  - Engine exposes which active functions in the workbook are volatile and which originate from add-ins (P19-FABLE-0560).
  - Active RTD topics and async add-in cells report server, topic, last value, timestamps, update frequencies, and per-update events, plus a UI-side visual pause/throttle hook that never modifies formula text (P19-FABLE-0559, P19-NEON-0260).
- **Conflicts:** None

### CL-14-07: Cross-Platform Host Boundaries & Prototype Runtime Limits

- **Summary:** Keeps cross-platform automation honest: native VBA macros and XLL add-ins run on desktop first, browser builds clearly show marked saved values (`◊`), a one-line refusal reason, and an 'Open in desktop app' handoff, and prototype JS simulations are explicitly separated from native runtime verification.
- **Cards (3):** `P19-FABLE-0231`, `P19-FABLE-0235`, `P21-0126`
- **Sources:** P19-FABLE (2), P21 (1)
- **Disposition:** `core` — Enforces the Honesty Rules across desktop vs browser platforms and frames the post-book verification plan for real VBA, XLL, and COM RTD hosts.
- **`same-as` groups (1):** `P19-FABLE-0231` = `P19-FABLE-0235`
- **Cross-cluster `same-as`:** `P19-FABLE-0235` = `P19-GPT-0333` (CL-03-12)
- **Agreements:**
  - Native VBA macros and native `.xll` add-ins are desktop-only initially; on browser/WASM builds that cannot execute them, DNA Calc preserves the file intact, marks cached values with `◊`, states a concise one-line refusal reason, and offers 'Open in the desktop app' (P19-FABLE-0231, P19-FABLE-0235).
  - No real VBA engine, native XLL/XLAM binary, COM RTD provider, or external network feed was executed in the Pro 21 HTML prototype; native automation requires dedicated Windows/desktop runtime verification (P21-0126).
- **Conflicts:** None

## XV. Testing the experience

### CL-15-01: Shared Cast Fixtures & Personas

- **Summary:** Anchors all storyboards, mockups, usability studies, and acceptance tests in a fixed shared cast of six personas (Maria, Sam, Lee, Jo, Ade, and an AI agent) working across realistic domain workbooks (Budget, Sales, Loan, Stock, plus Inherited) so every view is evaluated against concrete, end-to-end spreadsheet tasks.
- **Cards (5):** `FRAMING-0115`, `FRAMING-0116`, `FRAMING-0117`, `FRAMING-0118`, `FRAMING-0120`
- **Sources:** FRAMING (5)
- **Disposition:** `core` — Introduce the shared cast at the start of Chapter XV and link directly to the pinned numeric fixture contracts in Appendix D (`CL-16-05`).
- **`same-as` groups:** None within cluster
- **Cross-cluster `same-as`:** `FRAMING-0115` = `P19-GPT-0359` (CL-16-05); `FRAMING-0116` = `P19-GPT-0360` (CL-16-05); `FRAMING-0117` = `P19-GPT-0361` (CL-16-05); `FRAMING-0118` = `P19-GPT-0362` (CL-16-05)
- **Agreements:**
  - Four domain workbooks—Budget (3-sheet financial plan), Sales (25,000-row Orders table + Report + PivotTable), Loan (inputs + 360-row amortisation schedule + chart), and Stock (inventory model with delay, iterative reference, and slider)—together with the Inherited stress workbook form the canonical fixture cast.
  - Six personas—Maria (finance analyst), Sam (reviewer/auditor), Lee (systems engineer), Jo (model consumer), Ade (add-in author), and an AI agent working alongside Maria—ensure every view serves a distinct, testable user role.
- **Conflicts:** None

### CL-15-02: Seven End-to-End Storyboards & Make-or-Break UX Coverage Matrix

- **Summary:** Ties the Design Book directly to executable QA: seven mandatory end-to-end storyboards are written with exact keys and time budgets so they double as usability scripts, mapped against an eleven-category make-or-break UX coverage matrix and Pro 21's round-three requirement coverage matrix.
- **Cards (5):** `FRAMING-0121`, `P19-CARBON-0046`, `P19-FABLE-0040`, `P19-FABLE-0118`, `P21-0132`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-FABLE (2), P21 (1)
- **Disposition:** `core` — Include the unified 7-storyboard × 11-category make-or-break coverage table in Chapter XV as the top-level traceability index.
- **`same-as` groups (1):** `P19-CARBON-0046` = `P19-FABLE-0118`
- **Agreements:**
  - The seven canonical journeys (Maria/Inherited, Maria/Budget Notebook, Sam/Sales Inspector, Lee/Stock Model Map, Jo/Loan App, Ade/Formula Explorer + Budget, Maria+Agent review/undo) anchor UX acceptance.
  - Every make-or-break interaction class—first run/empty states, discovery, loading/calc feedback, view switching, errors/refusals, undo/recovery, copy/paste, hover/focus/selection, perceived speed, accessibility, and Excel-user wins—must map to at least one concrete storyboard step.
  - Requirement coverage matrices track every brief item to normal and exception scenes while keeping prototype coverage strictly separate from production certification.
- **Conflicts:** None

### CL-15-03: Moderated Usability Protocol, Comprehension Quiz & Evidence Bundle

- **Summary:** Establishes how human usability testing is run: goal-based plain-language tasks with zero hints or UI jargon across role-matched cohorts (desktop and browser, keyboard-only, real screen-reader users, high contrast, reduced motion, 100%/200%/400% zoom), followed by a five-statement model comprehension quiz (≥90% pass bar) to catch misleading views, and deterministic scripted-key runs recording interaction logs, transcripts, and file diffs.
- **Cards (5):** `P19-CARBON-0102`, `P19-FABLE-0208`, `P19-FABLE-0216`, `P19-GPT-0080`, `P19-NEON-0093`
- **Sources:** P19-CARBON (1), P19-FABLE (2), P19-GPT (1), P19-NEON (1)
- **Disposition:** `core-with-decision` — Specify the unified two-phase usability + automated-conformance protocol and the 5-statement comprehension quiz as the standard release-check methodology in Chapter XV.
- **Decisions:** [D-90](decisions.md#d-90)
- **`same-as` groups (1):** `P19-FABLE-0208` = `P19-GPT-0080`
- **Agreements:**
  - Usability runs present the starting workbook and a plain-language goal without coaching or prescribed clicks, requiring zero unnoticed model edits, silent stale claims, lost drafts, focus traps, or hidden content loss.
  - Every script is tested across desktop and browser, keyboard-only, high contrast, reduced motion, and high zoom (200%/400%), with real screen-reader participants exercising key auditing/consuming/authoring flows.
  - Static screenshots never suffice on their own: every acceptance run captures interaction logs, screen-reader transcripts, file/model-hash comparisons, and a post-task comprehension check.
- **Conflicts:**
  - **Usability cohort sizing & session timing: 8 mixed participants (`≤15 min` uniform) vs 10-per-role cohorts with task-specific time budgets (5–20 min) and mandatory 5-statement comprehension quiz** (`P19-FABLE-0216`, `P19-GPT-0080` vs `P19-CARBON-0102`, `P19-NEON-0093`) → Resolved by **[D-90](decisions.md#d-90)**

### CL-15-04: Usability Task Scripts — Storyboards 1–3 (Inherited, Budget Notebook, Sales Audit)

- **Summary:** Unifies the concrete usability task scripts and pass bars for the first three storyboards—triaging `Inherited.xlsm` safely without enabling macros, authoring a live Notebook write-up over `Budget` with scenario updates and spill recovery, and diagnosing a reporting discrepancy in the 25,000-row `Sales` workbook—resolving conflicting workbook faults invented independently by the four Pro 19 sources.
- **Cards (12):** `P19-CARBON-0103`, `P19-CARBON-0104`, `P19-CARBON-0105`, `P19-FABLE-0209`, `P19-FABLE-0210`, `P19-FABLE-0211`, `P19-GPT-0081`, `P19-GPT-0082`, `P19-GPT-0083`, `P19-NEON-0094`, `P19-NEON-0095`, `P19-NEON-0096`
- **Sources:** P19-CARBON (3), P19-FABLE (3), P19-GPT (3), P19-NEON (3)
- **Disposition:** `core-with-decision` — Publish the unified SB1–SB3 usability scripts in Chapter XV with exact task wording, time ceilings, and pass bars aligned with the pinned cast fixtures in Appendix D.
- **Decisions:** [D-91](decisions.md#d-91)
- **`same-as` groups:** None within cluster
- **Agreements:**
  - SB1 (Inherited): participants must open the large `.xlsm` without modal blocks or macro execution, identify the root error(s) separately from missing external links, and state accurately that displayed values are as-saved/cached rather than recalculated.
  - SB2 (Budget): participants must author a live Notebook note quoting calculated cells, verify live recalculation upon input/scenario edits, copy cleanly, and know what Excel will and will not show when reopening the file.
  - SB3 (Sales): participants must diagnose a discrepancy between `Report` and the `PivotTable` across the 25,000-row `Orders` table, trace the exact offending rows, repair cleanly, and verify exact `Ctrl+Z` undo with no stray edits.
- **Conflicts:**
  - **Divergent planted faults and numeric pass bars across the four Pro 19 sources for Inherited (SB1), Budget (SB2), and Sales (SB3)** (`P19-GPT-0081`, `P19-GPT-0082`, `P19-GPT-0083` vs `P19-CARBON-0103`, `P19-CARBON-0104`, `P19-CARBON-0105`, `P19-FABLE-0209`, `P19-FABLE-0210`, `P19-FABLE-0211`, `P19-NEON-0094`, `P19-NEON-0095`, `P19-NEON-0096`) → Resolved by **[D-91](decisions.md#d-91)**

### CL-15-05: Usability Task Scripts — Storyboards 4–7 & Keyboard/SR Audit (Stock, Loan, Explorer, Agent)

- **Summary:** Unifies usability scripts SB4–SB7 plus Neon's cross-cast keyboard/screen-reader audit (UT-8): building and stepping `Stock` on Model Map, running and printing `Loan` in App view, testing and hot-reloading an add-in in standalone Formula Explorer before committing in `Budget`, and selectively accepting/undoing an AI agent proposal—resolving Stock's ticking mechanism via DECISIONS_20261126 #3 and pinning one fixture per script.
- **Cards (17):** `P19-CARBON-0106`, `P19-CARBON-0107`, `P19-CARBON-0108`, `P19-CARBON-0109`, `P19-FABLE-0212`, `P19-FABLE-0213`, `P19-FABLE-0214`, `P19-FABLE-0215`, `P19-GPT-0084`, `P19-GPT-0085`, `P19-GPT-0086`, `P19-GPT-0087`, `P19-NEON-0097`, `P19-NEON-0098`, `P19-NEON-0099`, `P19-NEON-0100`, `P19-NEON-0101`
- **Sources:** P19-CARBON (4), P19-FABLE (4), P19-GPT (4), P19-NEON (5)
- **Disposition:** `core-with-decision` — Publish SB4–SB7 and UT-8 in Chapter XV alongside SB1–SB3, explicitly citing Ruling #3 for Stock and Ruling #14 for the Agent change-trail check.
- **Decisions:** [D-03](decisions.md#d-03), [D-14](decisions.md#d-14), [D-91](decisions.md#d-91)
- **`same-as` groups:** None within cluster
- **Agreements:**
  - SB5 (Loan): a non-builder manager must run scenarios, trigger and recover unaided from an Excel Stop validation refusal with zero overwritten formulas, restore baseline, and produce a clean labelled print/PDF.
  - SB6 (Formula Explorer): an add-in developer must load a local add-in with no workbook open, discover a boundary/domain error (plus standalone caller-context refusal), hot-reload a fix without restarting, and preview/commit the function in `Budget`.
  - SB7 (Agent Review): an analyst must inspect a multi-part agent proposal without premature commit, accept selected parts while rejecting an unintended/out-of-scope part, revert one accepted hunk cleanly with single `Ctrl+Z`, and handle stale-cell conflicts truthfully.
  - UT-8 (`P19-NEON-0101`): executing core scripts (SB1, SB3, SB4, SB5) using keyboard + screen reader only at 200% zoom and `high-contrast` is a mandatory release bar with zero traps or unlabeled controls.
- **Conflicts:**
  - **Stock simulation stepping & delay semantics in SB4: circular-ref iteration / F9 stepping vs RTD clock + explicit separation of time-stepped delay from same-period iteration** (`P19-GPT-0084` vs `P19-CARBON-0106`, `P19-FABLE-0212`, `P19-NEON-0097`) → Resolved by **[D-03](decisions.md#d-03)**
  - **Divergent add-in binaries, Loan numbers, and Agent proposal hunks across SB5–SB7** (`P19-GPT-0085`, `P19-GPT-0086`, `P19-GPT-0087` vs `P19-CARBON-0107`, `P19-CARBON-0108`, `P19-CARBON-0109`, `P19-FABLE-0213`, `P19-FABLE-0214`, `P19-FABLE-0215`, `P19-NEON-0098`, `P19-NEON-0099`, `P19-NEON-0100`) → Resolved by **[D-91](decisions.md#d-91)**
  - *Cross-cluster `conflicts-with` links (resolved by D-03, D-14, D-91):* `P19-CARBON-0106` vs `FRAMING-0053` (CL-07-07), `FRAMING-0068` (CL-01-03), `FRAMING-0141` (CL-07-08), `FRAMING-0168` (CL-07-08), `P19-GPT-0088` (CL-07-07), `P21-0064` (CL-07-07) (+2 more); `P19-FABLE-0212` vs `FRAMING-0053` (CL-07-07), `FRAMING-0068` (CL-01-03), `FRAMING-0141` (CL-07-08), `FRAMING-0168` (CL-07-08), `P19-GPT-0088` (CL-07-07), `P21-0064` (CL-07-07) (+2 more); `P19-NEON-0097` vs `FRAMING-0053` (CL-07-07), `FRAMING-0068` (CL-01-03), `FRAMING-0141` (CL-07-08), `FRAMING-0168` (CL-07-08), `P19-GPT-0088` (CL-07-07), `P21-0064` (CL-07-07) (+2 more); `P19-NEON-0100` vs `FRAMING-0179` (CL-13-03), `P19-FABLE-0497` (CL-02-17), `P19-GPT-0098` (CL-13-03), `P21-0113` (CL-02-17)

### CL-15-06: Accessibility, Keyboard-Only, Screen-Reader & High-Zoom Conformance

- **Summary:** Defines the full behavioural accessibility acceptance bar (`R-54`..`R-59`, `C-30`, `A.5`): complete keyboard-only operation and click-only alternatives to dragging, truthful screen-reader exposure of real virtualised row/column coordinates and states without flooding announcements, 200%/400% zoom reflow with linear alternatives for 2D surfaces, non-colour cues and dual-stroke focus, focus-accessible hover info, reduced motion, and 19 required spreadsheet edge-case fixture families.
- **Cards (9):** `P19-GPT-0230`, `P19-GPT-0284`, `P19-GPT-0285`, `P19-GPT-0286`, `P19-GPT-0287`, `P19-GPT-0288`, `P19-GPT-0289`, `P19-GPT-0329`, `P21-0012`
- **Sources:** P19-GPT (8), P21 (1)
- **Disposition:** `core` — Adopt `R-54`..`R-59`, `C-30`, and the 19-family spreadsheet edge-case matrix verbatim as the Accessibility & Behavioural Conformance section of Chapter XV.
- **`same-as` groups (1):** `P19-GPT-0286` = `P21-0012`
- **Cross-cluster `same-as`:** `P19-GPT-0287` = `FRAMING-0098` (CL-15-07), `P21-0102` (CL-12-04)
- **Agreements:**
  - Every storyboard operation works via keyboard alone and provides click-only alternatives to pointer dragging (`R-54`).
  - Screen readers expose real row/column coordinates (virtualisation never renumbers row 10483 as row 1), headers, selection, source, editability, error, and freshness (`R-55`, `C-30`), announcing completion/progress without stealing focus or reading every recalculated cell (`R-59`).
  - 200% and 400% zoom keep every control reachable: prose/forms reflow at 320 CSS px and split panes stack vertically, while 2D grids and maps keep scrolling plus a linear/list alternative (`R-56`, `P21-0012`).
  - Contrast thresholds hold across light/dark/high-contrast with visible two-stroke focus and non-colour cues (`R-57`); hover information is equally reachable via focus and dismissible; reduced motion strips dependency ripples while retaining full state information (`R-58`).
  - Conformance testing spans 19 required spreadsheet edge-case fixture families (data boundaries, blank islands, formatted empty ranges, merges, filters/hidden rows, noncontiguous selections, tables, spills, legacy arrays, protection, scoped names, cross-sheet picking, empty-text formulas, clipboard round-trips, drag/insert, interrupts, stale results, macros, unavailable add-ins) recording selection, values, formatting, structure, and Undo (`P19-GPT-0230`).
- **Conflicts:** None

### CL-15-07: Automated Corpus, Model-Hash, Cross-Module & Contrast Test Harnesses

- **Summary:** Specifies the mechanical test harnesses that prove front-end invariants continuously: opening corpus workbooks (including an Enron 20.7M-formula stress tier) in every view without crash, hang, budget breach, or altered Excel values; checking `model hash` immutability across presentation edits and `Esc` reverts; enforcing `SANCTIONED_PAIRS` WCAG contrast in `dna-strand` unit tests; and running cross-module integration checks across clock ticks, File re-entry, Undo, and Explorer.
- **Cards (4):** `FRAMING-0039`, `FRAMING-0098`, `P19-FABLE-0412`, `P21-0123`
- **Sources:** FRAMING (2), P19-FABLE (1), P21 (1)
- **Disposition:** `core` — Make these four mechanical harnesses the CI/local acceptance backbone in Chapter XV, linking directly to `cargo xtask` and `apps/dna-strand`.
- **`same-as` groups:** None within cluster
- **Cross-cluster `same-as`:** `FRAMING-0098` = `P19-GPT-0287` (CL-15-06), `P21-0102` (CL-12-04)
- **Agreements:**
  - The automated view harness opens corpus workbooks across every view—including an Enron-scale stress tier—to prove zero crashes/hangs, latency/memory budget compliance, zero calculated values altered, intact Excel file round-trip, and Classic gesture conformance (`FRAMING-0039`).
  - `model hash` (exact comparison of everything Excel reads from the file) provides the deterministic oracle for presentation-only skins, view switching, and `Esc`-revert invariants (`P19-FABLE-0412`).
  - `SANCTIONED_PAIRS` unit tests in `apps/dna-strand` mechanically fail the build on any contrast regression; on doc/code divergence, the crate wins and `STRAND.md` is updated (`FRAMING-0098`).
  - Cross-module integration tests verify synchronised clock tick counters/breadcrumbs across Model Map and Inspector, File re-entry preserving workbook/customisation, scoped New-from-pattern Undo, Explorer metadata/tree/result/Undo alignment, and 1440×900 first-viewport fit (`P21-0123`).
- **Conflicts:** None

### CL-15-08: Mandatory Visual-State & Screenshot Review Matrix

- **Summary:** Sets the visual completeness and realism standard for design artifacts and automated golden screenshots: real fixture data (never placeholders), paired Normal and Exception variants at 1440×900 plus 200% zoom, high-contrast, reduced-motion, and all three Strand themes, covering every workbook×view combination, the Strand component sheet (10 cell states, 4 drawer types), docked/standalone Formula Explorer, and active storyboard lifecycle states.
- **Cards (7):** `FRAMING-0131`, `FRAMING-0144`, `P19-CARBON-0110`, `P19-FABLE-0217`, `P19-GPT-0299`, `P19-NEON-0102`, `P21-0013`
- **Sources:** FRAMING (2), P19-CARBON (1), P19-FABLE (1), P19-GPT (1), P19-NEON (1), P21 (1)
- **Disposition:** `core-with-decision` — Publish the Tier 1 / Tier 2 visual-state review matrix and Strand component-sheet checklist in Chapter XV.
- **Decisions:** [D-92](decisions.md#d-92)
- **`same-as` groups (1):** `P19-CARBON-0110` = `P19-NEON-0102`
- **Agreements:**
  - Every visual mockup and golden capture uses real cast workbook labels, numbers, formulas, and errors—never dummy text—and explicitly exercises active/critical/exception states (mid-edit, mid-calc, error+root, refusal, preview seam, undo) rather than resting states alone (`FRAMING-0131`, `P21-0013`).
  - The Strand component sheet verifies tokens, type scale, 10 cell states (`normal`, `selected`, `active`, `editing`, `point-mode`, `error`, `spill-anchor`, `spill-child`, `stale`, `pending`), chips/cards, shell chrome, 4 drawer types (`model-change`, `presentation-change`, `agent-proposal`, `conflict`), ports/wires, and focus/hover/selection (`FRAMING-0144`).
  - Any cell in the review matrix that cannot be produced is treated as a design or implementation gap, and screenshots always complement—never replace—interaction logs, accessibility transcripts, and file comparisons.
- **Conflicts:**
  - **Golden screenshot matrix scope: 24–27 curated states vs 25 workbook×view + lifecycle set vs full 5×5×8 state cube** (`P19-GPT-0299`, `P19-FABLE-0217`, `FRAMING-0144`, `P21-0013` vs `P19-CARBON-0110`, `P19-NEON-0102`) → Resolved by **[D-92](decisions.md#d-92)**

### CL-15-09: Independent Arithmetic Verification & Evidence Honesty Boundaries

- **Summary:** Codifies strict measurement honesty in design and QA reporting: derived UI readouts stay synchronised with simulation state, fixture aggregates (including 25,000-row Sales `GROUPBY`/`PIVOTBY`) are verified by an independent reconstruction loop rather than production code, heterogeneous test metrics are never summed into one inflated total, and design/prototype checks (`jsdom`, offline PDF proofs, target budgets) are explicitly distinguished from native-browser, screen-reader, and Excel runtime certification.
- **Cards (6):** `FRAMING-0163`, `P19-GPT-0010`, `P19-GPT-0365`, `P21-0120`, `P21-0121`, `P21-0124`
- **Sources:** FRAMING (1), P19-GPT (2), P21 (3)
- **Disposition:** `core` — Include as the 'Verification Boundaries & Reporting Honesty' subsection of Chapter XV—directly aligning UX verification with the repository's Honesty Rules.
- **`same-as` groups (1):** `P19-GPT-0010` = `P19-GPT-0365`
- **Agreements:**
  - Every shown computed output—including `GROUPBY`/`PIVOTBY` aggregates over the 25,000-row Sales fixture—is independently reconstructed from the row specification in a separate loop rather than calling production code to generate expectations (`FRAMING-0163`, `P21-0120`).
  - Interactive views keep all derived readouts (tick counters, breadcrumbs, captured states) strictly synchronised with underlying simulation state.
  - QA reports keep heterogeneous counts separate (assertions, scenario groups, focused checks, thumbnails, static proofs) rather than summing them into one misleading number (`P21-0121`).
  - Design targets and prototype checks (`jsdom`, WeasyPrint proofs, fixture arithmetic) never masquerade as native-browser rendering, font-metric, OS-shortcut, screen-reader, WCAG, Excel runtime, or `customXml` round-trip certification (`P19-GPT-0010`, `P19-GPT-0365`, `P21-0124`).
- **Conflicts:** None

### CL-15-10: Milestone Acceptance Checks & Cross-View Coherence Exit Criteria

- **Summary:** Structures staged release verification so every milestone proves source honesty before adding presentation layers (Classic shell -> Inspector & Formula Explorer -> Notebook -> Model Map -> App -> Customisation & Agent) and checks completion on running all five cast workbooks plus screen-reader, zoom, and Enron stress checks across every view.
- **Cards (7):** `P19-GPT-0100`, `P19-GPT-0101`, `P19-GPT-0102`, `P19-GPT-0103`, `P19-GPT-0104`, `P19-GPT-0105`, `P19-GPT-0106`
- **Sources:** P19-GPT (7)
- **Disposition:** `core-with-decision` — Retain `P19-GPT-0106`'s cross-view cast/SR/Enron acceptance criteria verbatim in Chapter XV, and synchronise the milestone sequence with Appendix C (`CL-16-03`, `CL-16-04`): prepend M0 Skin Architecture foundation (DECISIONS_20261126 #9/#10) and promote Standalone Formula Explorer to M1 as an early teaser (DECISIONS_20261126 #2) while keeping GPT's exit criteria per surface.
- **Decisions:** [D-02](decisions.md#d-02)
- **`same-as` groups:** None within cluster
- **Agreements:**
  - Truth-telling surfaces (Classic editing shell, Inspector, Formula Explorer) must pass their source-honesty and round-trip exit bars before summarising views (Notebook, Model Map, App) and customisation build on top of them.
  - No view is judged on an isolated demo: every milestone runs all five cast fixtures plus screen-reader, zoom, and Enron-derived stress checks across all completed views, verifying that reviewers can trace every non-prose item to its backing cell/object, predict every edit and undo, distinguish live vs stored observations, and return to Classic without losing their place (`P19-GPT-0106`).
- **Conflicts:**
  - *Cross-cluster `conflicts-with` links (resolved by D-02):* `P19-GPT-0101` vs `FRAMING-0051` (CL-09-01), `FRAMING-0167` (CL-09-01), `P19-CARBON-0123` (CL-16-03)

## Appendices

### CL-16-01: Front-End Consolidation Governance, Source Corpus & Owner-Ruled Disagreements

- **Summary:** Establishes the ledger-first synthesis methodology (`DECISIONS_20261126 #1`), the exact closed source corpus for epic `dc-etcxb` (framing docs, Pro 19/20/21 briefs and answers, Strand, Linked Boxes, previews), the five consolidation deliverables, and the master index of eight cross-answer disagreements settled by Owner & Decider Rulings #2–#14.
- **Cards (5):** `FRAMING-0055`, `FRAMING-0058`, `FRAMING-0060`, `FRAMING-0069`, `FRAMING-0166`
- **Sources:** FRAMING (5)
- **Disposition:** `core-with-decision` — Place the source-corpus ledger index and the Decision Log (mapping `FRAMING-0069`'s 8 disagreements + Rulings #1–#14 to their Design Book chapters) in the Design Book's Governance / Appendix Z front-matter. Note that `FRAMING-0058` deliverable #5 (FerroUI evaluation) is closed as not now by DECISIONS_20261126 #7.
- **Decisions:** [D-01](decisions.md#d-01)
- **`same-as` groups (1):** `FRAMING-0055` = `FRAMING-0058`
- **Agreements:**
  - Synthesis uses the lossless ledger-first method: Neon extracts every claim into ledgers, then Fable and Opus cluster, decide, and author one coherent ~100-page Design Book with all rejected alternatives documented in Appendix Z (`DECISIONS_20261126 #1`).
  - The source corpus is closed and explicit (`FRAMING-0060`), with Pro 21's 73 scenes superseding Pro 20's 43 scenes.
  - All eight cross-answer disagreements catalogued in `FRAMING-0069` are now resolved by `DECISIONS_20261126` (#2 early standalone Explorer, #3 RTD clock / macro tick, #4 try macro undo without first-release promise, #5 `customXml` notes/findings with optional sheet mirror, #11 exact Excel key-tip letters in Classic, #12 `Ctrl+Alt+1…5` view keys + command search fallback, #13 no hidden names as anchors, #14 file-persisted toggleable agent change trail in `customXml`).
- **Conflicts:** None

### CL-16-02: UX-to-Architecture Handoff Format (R-List, C-List, Beads & Milestone Rules)

- **Summary:** Defines the structural contract between the Design Book and the Skin Architecture implementation plan: testable front-end requirements (`R-1…`), plain-language system needs (`C-1…` without dictating backend code), explicit push-backs, and dependency-ordered implementation beads with test-named acceptance criteria, milestone order (never calendar dates), deletion records, and risk-retirement pairs.
- **Cards (3):** `FRAMING-0027`, `FRAMING-0045`, `FRAMING-0130`
- **Sources:** FRAMING (3)
- **Disposition:** `core` — Use this exact structure for Appendices B (Unified R-list), C (Unified C-list & Milestone Plan), and Z (Not-Taken & Push-Back Ledger).
- **`same-as` groups:** None within cluster
- **Agreements:**
  - The Design Book owns the unified `R-` (front-end acceptance requirements) and `C-` (plain-language system needs) appendices plus explicit push-backs, keeping Rust/protocol inside-baseball out of the UX specification.
  - The downstream Skin Architecture implementation plan maps every `C-` item to crates and layers under one epic of dependency-ordered beads with test-named acceptance criteria, explicit deletions/compatibility/proof tests, risks paired with retiring steps, and milestones rather than calendar dates.
- **Conflicts:** None

### CL-16-03: Early Delivery Sequence — Skin Architecture, Classic Core & Standalone Formula Explorer

- **Summary:** Reconciles the opening milestones across `FRONT_END_SCOPE`, Carbon, Fable, and Neon under Owner Rulings #2, #9, and #10: Skin Architecture foundation comes first (M0), followed immediately by One Shell, full-fidelity Classic grid, and Standalone `DNA Calc - Formula Explorer` as an early teaser (M1)—explicitly overruling Neon's placement of Standalone Formula Explorer last in M6 while adopting Fable's principles that every milestone serves at least one persona and truth-telling precedes summarising views.
- **Cards (7):** `FRAMING-0024`, `P19-CARBON-0123`, `P19-FABLE-0237`, `P19-FABLE-0245`, `P19-NEON-0118`, `P19-NEON-0119`, `P19-NEON-0124`
- **Sources:** FRAMING (1), P19-CARBON (1), P19-FABLE (2), P19-NEON (3)
- **Disposition:** `core-with-decision` — Publish M0 and M1 as the opening phase of the unified Delivery Roadmap in Appendix C, explicitly citing Rulings #2, #9, #10, and #11.
- **Decisions:** [D-02](decisions.md#d-02), [D-09](decisions.md#d-09)
- **`same-as` groups (1):** `P19-FABLE-0237` = `P19-NEON-0119`
- **Cross-cluster `same-as`:** `P19-CARBON-0123` = `FRAMING-0167` (CL-09-01)
- **Agreements:**
  - Every milestone must be independently useful to at least one persona, and truth-telling instruments (Shell, Classic, scale loading, unified Undo, key-tip parity) must land before summarising views (`P19-FABLE-0245`).
  - Strand v0.2 tokens, non-blocking off-thread opening for large workbooks, unified address grammar, dual-tagged Undo, Command Palette, non-modal security/triage banners, and full Appendix A Classic parity form the opening user-facing foundation (`P19-CARBON-0123`, `P19-FABLE-0237`, `P19-NEON-0118`, `P19-NEON-0119`).
- **Conflicts:**
  - **Timing of Standalone Formula Explorer (`DNA Calc - Formula Explorer`): early M1 teaser vs final M6 release** (`P19-CARBON-0123` vs `P19-NEON-0124`) → Resolved by **[D-02](decisions.md#d-02)**
  - **Opening architecture sequence: legacy contract refactor keeping old skins (`FRONT_END_SCOPE`) vs M0 merged Skin Architecture followed by fresh UI** (`P19-NEON-0118`, `P19-CARBON-0123` vs `FRAMING-0024`) → Resolved by **[D-09](decisions.md#d-09)**
  - *Cross-cluster `conflicts-with` links (resolved by D-02, D-09):* `FRAMING-0024` vs `FRAMING-0174` (CL-12-01); `P19-CARBON-0123` vs `P19-GPT-0101` (CL-15-10), `P19-NEON-0106` (CL-09-01); `P19-NEON-0124` vs `FRAMING-0051` (CL-09-01), `FRAMING-0167` (CL-09-01)

### CL-16-04: Mid-to-Late Delivery Sequence — Inspector, Notebook, Map, App, Skins & Agent Review

- **Summary:** Reconciles the mid-to-late milestone plans of Carbon, Fable, and Neon into one coherent sequence after M0–M1: M2 Inspector + Docked Formula Explorer + Formula Lens; M3 CustomXML Persistence + Declarative Skin Engine + Jupyter-style Notebook; M4 Model Map (reader first, then wiring & RTD/macro-ticked playback controls); M5 App View + Print Engine; and M6 Agent Staged Proposals + Persisted Change Trail + Guarded Macro/Undo & Coded-Extension Hardening.
- **Cards (16):** `P19-CARBON-0124`, `P19-CARBON-0125`, `P19-CARBON-0126`, `P19-CARBON-0127`, `P19-CARBON-0128`, `P19-FABLE-0238`, `P19-FABLE-0239`, `P19-FABLE-0240`, `P19-FABLE-0241`, `P19-FABLE-0242`, `P19-FABLE-0243`, `P19-FABLE-0244`, `P19-NEON-0120`, `P19-NEON-0121`, `P19-NEON-0122`, `P19-NEON-0123`
- **Sources:** P19-CARBON (5), P19-FABLE (7), P19-NEON (4)
- **Disposition:** `core-with-decision` — Publish the reconciled M0–M6 Delivery Roadmap in Appendix C. Explicitly adjust three inherited details to match `DECISIONS_20261126`: (1) Notebook in M3 is Jupyter-style with no separate read-mode presentation (`P19-FABLE-0240` Reading density -> App view, Ruling #6); (2) Model Map ticks in M4 use RTD clock / valid macro ticks rather than circular-ref delays (`P19-CARBON-0127`/`P19-NEON-0122`, Ruling #3); (3) Agent Change Trail in M6 is persisted in workbook `customXml` (toggleable) rather than session-only (`P19-NEON-0123`, Ruling #14), and macro undo (`P19-CARBON-0128`) is designed for without blocking first release (Ruling #4).
- **Decisions:** [D-03](decisions.md#d-03), [D-04](decisions.md#d-04), [D-06](decisions.md#d-06), [D-14](decisions.md#d-14), [D-93](decisions.md#d-93)
- **`same-as` groups (2):** `P19-CARBON-0124` = `P19-NEON-0120`; `P19-CARBON-0127` = `P19-NEON-0122`
- **Agreements:**
  - All proposals place **Inspector + Docked Formula Explorer + Formula Lens** immediately after Classic (M2), delivering root-cause error folding, anomaly probes, causal trace, and block/run pattern reading so every later view links back to exact audit instruments.
  - All proposals sequence **Model Map** as read-only grouping/zoom first before interactive operand wiring and simulation Directors.
  - All proposals treat **Agent Staged Proposals** (selective hunk acceptance, impact preview, per-hunk Undo, conflict handling) and **macro/coded-extension hardening** as the final milestone once every view surface is in place.
- **Conflicts:**
  - **Sequencing of CustomXML/Declarative Skins and App View relative to Notebook and Model Map** (`P19-FABLE-0240`, `P19-FABLE-0241`, `P19-FABLE-0242`, `P19-FABLE-0243`, `P19-NEON-0121`, `P19-NEON-0123` vs `P19-CARBON-0125`, `P19-CARBON-0126`) → Resolved by **[D-93](decisions.md#d-93)**
  - *Cross-cluster `conflicts-with` links (resolved by D-03, D-04, D-06, D-14, D-93):* `P19-CARBON-0127` vs `FRAMING-0053` (CL-07-07), `FRAMING-0068` (CL-01-03), `FRAMING-0141` (CL-07-08), `FRAMING-0168` (CL-07-08), `P19-GPT-0088` (CL-07-07), `P21-0064` (CL-07-07) (+2 more); `P19-CARBON-0128` vs `FRAMING-0169` (CL-14-04), `P19-GPT-0038` (CL-02-10), `P19-GPT-0079` (CL-14-04), `P19-GPT-0205` (CL-04-30), `P19-GPT-0207` (CL-04-30), `P19-GPT-0296` (CL-14-04) (+1 more); `P19-FABLE-0240` vs `FRAMING-0054` (CL-06-01), `FRAMING-0056` (CL-06-01), `FRAMING-0145` (CL-06-01), `FRAMING-0171` (CL-06-01), `P21-0014` (CL-06-01); `P19-NEON-0123` vs `FRAMING-0179` (CL-13-03), `P19-FABLE-0497` (CL-02-17), `P19-GPT-0098` (CL-13-03), `P21-0113` (CL-02-17)

### CL-16-05: Canonical Cast Fixture Specifications & Numeric Contracts

- **Summary:** Pins one authoritative, arithmetically verified specification for all five shared cast workbooks (Budget, Sales, Loan, Stock, Inherited) plus Ade's add-in test contract (`DiscountFunctions.xll`), enforcing English formula syntax, real date values, full internal calculation precision distinct from display rounding, independent baselines, and explicit separation of Stock's time-stepped `INDEX` delay from iterative convergence.
- **Cards (9):** `P19-FABLE-0010`, `P19-GPT-0358`, `P19-GPT-0359`, `P19-GPT-0360`, `P19-GPT-0361`, `P19-GPT-0362`, `P19-GPT-0363`, `P19-GPT-0364`, `P21-0122`
- **Sources:** P19-FABLE (1), P19-GPT (7), P21 (1)
- **Disposition:** `core-with-decision` — Publish the six canonical fixture contracts (Budget, Sales, Loan, Stock, Inherited, and `DiscountFunctions.xll`) in Appendix D as the single numerical ground truth for the entire Design Book.
- **Decisions:** [D-91](decisions.md#d-91)
- **`same-as` groups (1):** `P19-GPT-0358` = `P21-0122`
- **Cross-cluster `same-as`:** `P19-GPT-0359` = `FRAMING-0115` (CL-15-01); `P19-GPT-0360` = `FRAMING-0116` (CL-15-01); `P19-GPT-0361` = `FRAMING-0117` (CL-15-01); `P19-GPT-0362` = `FRAMING-0118` (CL-15-01)
- **Agreements:**
  - All cast workbooks and workbook-free Formula Explorer examples use pinned, arithmetically checkable numbers rather than placeholder text.
  - Full internal floating-point precision is strictly preserved separate from display rounding (no precision-as-displayed), with real date serials and locale-independent formula semantics.
  - Add-in testing covers normal evaluation, domain errors (`#NUM!`), and honest caller-context refusal (`DNA.CALLERADDRESS()`) in standalone mode without falsely claiming native XLL execution inside a browser.
- **Conflicts:**
  - **Two incompatible numeric baselines for the five cast fixtures: Fable's figures vs GPT/Pro 21's verified fixture contracts** (`P19-GPT-0358`, `P19-GPT-0359`, `P19-GPT-0360`, `P19-GPT-0361`, `P19-GPT-0362`, `P19-GPT-0363`, `P21-0122` vs `P19-FABLE-0010`) → Resolved by **[D-91](decisions.md#d-91)**

### CL-16-06: Primary Technical Provenance, Microsoft References & Reference Scene Inventory

- **Summary:** Documents the authoritative technical provenance behind the Design Book—5 supplied repository inputs, 21 external primary design references (`S1–S21` covering WCAG 2.2, Excel docs, Pluto, phyphox, Enron), 33 Microsoft Classic interaction references (`KA1–KA33`), 16 Microsoft engine/RTD/function references (`T1–T16`), the pinned-Excel empirical-check rule whenever docs are ambiguous—and indexes Pro 21's complete 73-scene interactive reference package superseding Pro 20.
- **Cards (3):** `P19-GPT-0366`, `P21-0130`, `P21-0131`
- **Sources:** P19-GPT (1), P21 (2)
- **Disposition:** `core` — Include the consolidated provenance bibliography (`S1–S21`, `KA1–KA33`, `T1–T16`) and the 73-scene Pro 21 cross-reference index in Appendix E.
- **`same-as` groups:** None within cluster
- **Agreements:**
  - Normative Excel behaviour and Classic interaction rules are anchored in primary Microsoft documentation (`S2–S4`, `S7–S14`, `S16`, `KA1–KA33`, `T1–T16`), with observed pinned-Excel comparison required whenever documentation is ambiguous.
  - Accessibility and systems/notebook inspirations are grounded in primary specifications (WCAG 2.2 `S1`/`S17–S21`, Pluto `S5`, phyphox `S6`, Enron corpus `S15`) without claiming native execution certification from design prototypes.
  - Pro 21's 73-scene interactive package (retaining all 43 Pro 20 scenes plus 30 round-three scenes across 13 HTML files, `verification.html`, and `README.md`) serves as the authoritative visual reference companion to the Design Book.
- **Conflicts:** None
