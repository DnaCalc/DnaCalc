<!-- Copied for dc-xib from DnaTreeCalc bf6dfd7, docs/ux/redesign/STRAND_DESIGN_LANGUAGE.md. The tokens
     live in apps/dna-strand (where it and this text disagree, the crate wins). Typefaces: the apps
     use the system UI and monospace stacks for now; Recursive and Source Serif are not bundled. -->

# Strand — the DNA Calc design language

Status: v0.1 · 2026-07-12 · companion to [REDESIGN_PROGRAM.md](REDESIGN_PROGRAM.md).

Strand takes its material from the DNA Kode mark: rounded blocks in petrol, teal and amber —
base pairs, code tokens, cell runs. The temperament is flight-deck, not showroom: dark, calm
chrome holds the controls; the work sits on bright paper; color appears only when it means
something. Understated, dense where the hands are, generous where the eyes rest.

Sources: `C:\Work\DNAKodeInc\Design\DNA Kode logo dev - v6 copy.pdf`,
`C:\Work\DNAKodeInc\Design\LogosAndIcons\` (palette PNG + logo SVGs).

## 1. Eight tenets

1. **Cockpit chrome, daylight work.** Petrol chrome holds instruments; the model sits on paper.
   Chrome never competes with data.
2. **Everything is a block.** The base-pair rounded rect is the atomic unit: cells, nodes,
   formula tokens, name chips, map districts. One radius scale, one gap scale.
3. **Color is a signal, not paint.** Amber is *now*. Teal is structure. Green is a fresh value.
   Signal orange is external/volatile. Red is error — and nothing else, ever.
4. **One material, many states.** One variable typeface morphs sans↔mono; UI, values and
   formulas share a skeleton. Notebook prose gets a quiet serif.
5. **Modeless truth.** One bit separates SELECTED from EDITING. Ghost previews before commit;
   Esc is always an exact revert; legality shows before you pay (preview seam).
6. **Instruments, not ornaments.** Persistent honest readouts; motion only where it carries
   meaning (120–160 ms settle; recalc ripple along real evaluation order).
7. **Every object has an address.** Node path, A1, name, table column — one grammar for goto,
   peek, and automation. What a human types is what an agent sends.
8. **Zoom is semantic.** Far views trade detail for structure (names over blocks, summaries over
   cells). Text never shrinks below legibility; level-of-detail swaps instead.

Inherited spine laws (re-expressed, not re-invented — see `docs/ux/skin-suite/SPINE.md`):
calc-state is the only saturated channel (it is amber now) · provenance is structural tint,
never decoration · authoring is modeless 1-bit.

## 2. Palette — every color has a job

| Token | Hex | Role |
|---|---|---|
| `petrol-800` | `#193F49` | Chrome ground (Mast, Bridge, Strip), instrument panels |
| `petrol-900/950` | `#12303A` / `#0C1C22` | Chrome depth ramp, dark-theme chrome |
| `teal-600` | `#318995` | Structure, references, interactive accents; Excel-strict identity color |
| `teal-400` | `#3DB6CF` | Links, secondary structure; OneCalc identity color |
| `amber` | `#FFB301` | **Focus. The only saturated attention channel**: cursor, selection, running calc, X-Ray target |
| `green-500` | `#3F8B76` | Fresh/verified values (not an identity color) |
| `sage-400` / `forest-700` | `#71A08A` / `#235949` | Rich-Tree identity pair; soft value tints |
| `signal` | `#FF8001` | External & volatile only: RTD, links, add-in-fed cells |
| `red` | `#D02A23` | Errors only. Never emphasis, never branding |
| `charcoal` | `#353938` | Dark-theme paper base, body ink |
| `mist` | `#EDF1F2` | Light paper base, chrome ink |

Rules:
- **Identity is never a lone hue.** Profiles are recognized by a two-block *base-pair badge*
  (the logo's own device): distinct block-length pattern + color pair, so recognition survives
  small sizes and color-vision differences. Assignments:
  OneCalc = cyan dot + petrol long (`dot-long`) · Rich Tree = sage long + forest dot
  (`long-dot`) · Excel-strict = teal + petrol equal pair (`twin`). Only Tree vs Excel-strict
  ever co-occur in one shell (OneCalc is its own window), so the hard requirement is that
  *those two* are effortless: sage-light-green vs teal-dark-blue, plus opposite block patterns.
- Identity badges appear **only in chrome identity spots** (profile badge, stage underline) —
  never on data surfaces.
- Neutrals are petrol-biased greys derived from `petrol`/`mist`, never pure grey.
- Every color signal pairs with a non-color cue (shape, weight, badge) — color is never the
  sole indicator (a11y doctrine).

### 2.1 Contrast law

The hues above are **surface tones** (fills, borders, blocks). Wherever a hue becomes text or a
small glyph, it must use a per-theme **`-ink` variant** meeting WCAG AA on its ground —
never the raw surface tone. Established variants (light-paper / dark-paper):

| Variant | On light `#FFFFFF` | On dark `#1A262B` | Used for |
|---|---|---|---|
| `teal-ink` | `#1B5F6B` (≈7.3:1) | `#4FC0D8` | accent text, eyebrows, command ids |
| `green-ink` | `#2E6E5B` (≈6.0:1) | `#74C7AB` | value text, "fresh/match" readouts |
| `ink-3` (meta) | `#5B717B` (≥4.5 on paper-2) | `#7D97A0` | kickers, captions, mono footnotes |
| `prov-const` | `#2D6A8A` | `#7DB8D8` | constant-cell text |
| `prov-ext` | `#A35400` | `#F0A35C` | external/volatile-cell text |
| `red-ink` | `#D02A23` | `#E4625C` | error text (raw red fails AA on dark paper — the law applies to red too) |

Canonical source: `apps/dna-strand` (the crate's `SANCTIONED_PAIRS` tests enforce this table;
`theme.rs` comments record every derivation). This table mirrors the crate — on divergence, the
crate wins and this table gets updated.

Thresholds: ≥ 4.5:1 for text and small glyphs; ≥ 3:1 for large text, borders and meaningful
non-text marks; chrome ink ≥ 4.5:1 on petrol. **Every sanctioned foreground/background pair is
a token pair, and the Strand crate asserts the ratios in unit tests** — a palette change that
breaks a pair fails the build, not the reader. (Found the hard way: a global `strong { color:
ink }` rule painted near-black product names onto the petrol masthead — scoped inks on dark
chrome are part of the law.)

## 3. Type system

| Role | Face | Notes |
|---|---|---|
| UI & data | **Recursive** (sans-linear axis) | Free variable family (MONO·CASL·wght·slnt). Tabular figures (`tnum`) wherever numbers stack. |
| Formulas & addresses | **Recursive Mono** | Same skeleton as UI — formulas read as the same material, not foreign code. Casual axis reserved for ghost text/hints. |
| Notebook prose | **Source Serif 4** | The reading voice; ~68ch measure, wider leading. Prose is annotation-layer only. |
| Fallbacks | Segoe UI Variable / system-ui · Cascadia Code / Consolas · Charter / Georgia | Windows-first fallback chain. |

Scale: a single modular ramp (11 · 12.5 · 13.5 · 15.5 · 18 · 22 · 27); weights 480/560/650.
Uppercase labels get +0.12em tracking. Headings `text-wrap: balance`.

## 4. Block geometry

Radius tracks block size (from the mark's proportions): chips/tokens 5px · cells 0 (grid) ·
cards 9px · panels 14px · stage corners 14px. Gap scale: 2 · 4 · 6 · 10 · 14 · 22.
Formula tokens, name chips, node cards and map districts are all the same component family at
different sizes — one border treatment, one hover/focus grammar (amber outline, 1px offset).

## 5. Motion & density

- Durations 120–160 ms, standard decel curve; `prefers-reduced-motion` honored everywhere.
- Only three sanctioned animations: state settle (focus/selection), recalc ripple (follows real
  `evaluation_order`), cross-stage continuity halo (mechanism 09). Nothing else moves.
- Two density modes: **Working** (default, hands-density) and **Reading** (Notebook/published,
  looser leading, wider measure). No third mode.

## 6. Theming

Token layer `--dna-*` (custom properties), three themes: `cockpit-light` (default: dark petrol
chrome around light paper), `cockpit-dark` (petrol-black chrome, charcoal paper), `high-contrast`.
Themes are the *only* per-user visual variance — they replace the 16-skin concept entirely.
Implementation: token definitions in one Strand crate consumed by every skin crate; no
per-skin stylesheets beyond component-scoped rules resolving through tokens
(carries forward the `--dtc-*` discipline under the new prefix).

## 7. Do / don't

- Do put every readout in the Strip or Inspector; don't scatter status into the stage.
- Do use provenance tints structurally (constants/formulas/external); don't color-code sheets or
  nodes decoratively — structure comes from type, weight and blocks.
- Don't introduce a new saturated color for a new feature; if it needs attention it is amber,
  if it is external it is signal, if it is broken it is red.
- Don't animate to delight. If a motion carries no state information, cut it.
- Parity/evidence marks (future; PARITY_TRUST_UX.md) reuse the standard semantic colors under
  a distinct twin-block glyph family — parity never gets a hue of its own, and never amber.
- Never describe any of this as final; the language versions like the protocol does.
