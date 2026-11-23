# dnacalc.com: site plan revision draft (dc-6q2k.1)

Draft note for the owner, 2026-11-23. Proposes updates to [SITE_PLAN.md](SITE_PLAN.md),
navigation, and page copy now that `dnacalc.com` is published from `master` (2026-11-23).
No files under `docs/site/pages/` or `tools/xtask-harness/src/site/` were edited in this note.

## 1. Why a revision is needed now

`docs/site/SITE_PLAN.md` was written on 2026-11-13 (`0798d060`) when `dnacalc.com` still served
only the 2026-11-09 `calc` web app (`deploy/site/published.json`). Between 2026-11-21 and
2026-11-23, `dc-6q2k.2`–`.12` built the `content` site (`marketing.rs`, `llms.rs`, `blog.rs`,
`downloads.rs`), 14 page templates in `docs/site/pages/`, 9 technical articles in `docs/articles/`,
the Classic Grid guide in `docs/user/classic-grid.md`, 3 draft blog posts in `docs/site/blog/`,
video drafts in `docs/site/video/`, and `/llms.txt` with Markdown editions.

Now that `dnacalc.com` is published from `master`, four gaps stand out between the live site,
`SITE_PLAN.md`, and the current repository state (`64004902`):

1. **Preview framing on a live apex site:**
   - The primary hero button on `pages/home.html` is **Preview downloads** (`/download/`), where
     every row in `downloads.rs` currently reads `"Not yet published"`.
   - `pages/web.html` labels the `/app/` button **Open local preview**, even though `/app/` is live
     inside the `content` site pack.
   - `pages/status.html` and `marketing.rs:builds_table` label the served site
     `"Local site preview, including both apps"` and compare against `deploy/site/published.json`,
     which in git still lists the 2026-11-09 `calc` and `onecalc` builds.
2. **Browser app URL mismatch (`layout.json` vs `sites.caddy` vs `/app/`):**
   - `docs/site/layout.json` sets `"browser_app_url": "https://app.dnacalc.com/"`, while
     `marketing.rs` packs the full web app at `/app/index.html` and `deploy/caddy/sites.caddy` has
     server blocks only for `dnacalc.com`, `www.dnacalc.com`, and `onecalc.dnacalc.com`.
   - On `/web/`, `{{app_address}}` renders a second link to `https://app.dnacalc.com/` beside the
     `/app/` button. Either `app.dnacalc.com` needs a DNS record and a Caddy block in `sites.caddy`,
     or `layout.json` should point to `/app/` until `app.dnacalc.com` exists.
3. **Empty `/blog/` vs unlinked `/articles/`:**
   - All three posts in `docs/site/blog/` have `Status: draft`, and `docs/site/video/` is built only
     under `--include-drafts` (`marketing.rs:539`). On a published site (`!include_drafts`), `/blog/`
     in the top bar lands on `"Posts appear here after editorial review."` and an empty Atom feed.
   - Meanwhile, the 9 finished technical articles (`docs/articles/last-bit-*.md` and
     `calcvba-variant.md`) render in HTML and Markdown at `/articles/`, yet `/articles/` is absent
     from the top navigation and from `pages/home.html`.
4. **Out-of-date status claims in `SITE_PLAN.md` and `pages/`:**
   Several figures and bead statuses in `SITE_PLAN.md` §2 and `pages/` have moved forward on
   `master`.

## 2. Current facts from the repository (2026-11-23)

All counts below come from checked-in files and tracker state at `64004902`:

| Area | Current status on `master` | Source |
|---|---|---|
| Function Bank (`full`) | `4,768,604` of `4,794,724` matched (`99.46%`), `506` functions, `436` exact (0 ULP) | `crates/calc-func/witness-status.tsv` |
| Function Bank (`pinned`) | `40,713` of `40,836` matched (`99.70%`), `281` functions, `264` exact | `crates/calc-func/witness-status.tsv` |
| Oracle workbook corpus | `RELEASES.md:99` lists `78,162` of `78,296` (`99.82%`, `bb2f3d1b`, `63` workbooks; rendered on `/` and `/status/`); `docs/site/release-notes/next.md:24` lists `78,167` of `78,296` (`99.84%`, `129` failures, `0` refusals) | `docs/plan/RELEASES.md`, `docs/site/release-notes/next.md` |
| CalcVBA corpus (`baseline.json`) | `common`: `232` entries (`dna-simple` `109/109` on module/JIT/`.xlsm`; `dna-examples` `24/26` project, `22/26` `.xlsm`; `vba-dictionary` `15/15`, `vba-json` `21/21`, `vba-regex` `69/69`, `vba-arraytools` `51/51`, `vba-stringbuilder` `35/35`, `vba-web` `45/45`, `vba-test` `31/31`, `vba-keyedcollection` `23/24` on project/JIT). `windows`: `2` entries (`wasabi` project/interpreter and JIT, `41/53` each) | `crates/calc-vba/tests/corpus/baseline.json` |
| Desktop workbook events & AutoFilter / sort | `dc-1nbp.3` (`--engine` session workbook, `SheetChange`, `SheetCalculate`, `AfterCalculate`, `EnableEvents`), `dc-971p.37` (AutoFilter), and `dc-971p.38` (row sorting) are **closed**. Open: WebView2 page-route add-in event timing (`dc-1nbp.3.1`), Page Setup (`dc-971p.57`), drawing epic (`dc-zjsg`, print-preview CSP `dc-zjsg.12`), Enron coercion (`dc-3k5n`), charts/dialogs (`dc-hc85`) | tracker (`br show`) |
| Windows installers | Unsigned MSI build exists (`dc-fk66.2` open); signing and clean-machine install/upgrade/uninstall (`dc-fk66.3`) remain open | tracker (`br show`) |
| Linux runner & desktop | `dnacalc` tarball (`11 MB`) and `.deb` (`7.8 MB`) accepted on `ubuntu:26.04` (`dc-icq4.2` closed). WebKitGTK 4.1 workbook probe measured (`dc-icq4.5` closed 2026-11-23) | tracker (`br show`) |
| Engine crates & exposed APIs | `publish = false` (`Cargo.toml:46`); API prep (`dc-qm0h`) closed; rustdoc (`dc-z94tt`) open. Exposed COM / C API `xlc` / Office.js faces active under `dc-17spk` (`dc-17spk.2`, `.6`, `.7`) | `Cargo.toml`, tracker |
| Published record | `deploy/site/published.json` still holds 2026-11-09 (`779e136e9c4f`, `c27d1c615516`) | `deploy/site/published.json` |

Note: `RELEASES.md:99` (`78,162`) and `release-notes/next.md:24` (`78,167`) should be reconciled to one
pinned commit before `0.2.0` release notes publish.

## 3. Pages per use case (audit and proposed changes)

Checking every use case listed in `dc-6q2k` and `SITE_PLAN.md`:

| Use case (`dc-6q2k` / `SITE_PLAN.md`) | Current route | Gap today | Proposed change |
|---|---|---|---|
| **1. Windows Excel alternative** (VBA, XLL, COM; Linux next, macOS one day) | `/desktop/` | No mention of the six skins over one workbook, `.xls`/`.xlsb`, `Off`/`Guarded`/`Unguarded` macro security, or "Linux next, macOS one day"; `dc-1nbp.3` and AutoFilter/sort closures are missing; no screenshots/clips | Add a "Works / In progress / Not yet" table synced to 2026-11-23, a short "Six skins, one workbook" section, the Linux/macOS roadmap line, and version-tagged screenshots once captured |
| **2. Formula Explorer as teaser & add-in bench** | `/onecalc/` (+ live box on `/`, `/play/`) | Doesn't separate browser capabilities (formulas, `F9`, trace, no XLL) from Windows desktop add-in bench (`.xll`/`.xlam`, Stop/Restart, values-only refusal of `As Range`/`Caller`/`ThisCell`); `#embedding` sits directly on product page | Add a short two-column split ("In the browser / on `onecalc.dnacalc.com`" vs "On Windows desktop"), link `/developers/addins/`, and keep `#embedding` linked from `/developers/` |
| **3. Browser app that opens `.xlsx` faithfully** | `/web/` + `/app/` | Button says `"Open local preview"`; omits the core privacy claim ("nothing is uploaded; runs in WebAssembly under `connect-src 'self'`"); `{{app_address}}` points to unconfigured `app.dnacalc.com` | Rename CTA to `"Open DNA Calc in the browser"`; state the local-only CSP boundary and link `/app/` (or `app.dnacalc.com` once wired in Caddy/DNS) |
| **4. Checking workbooks without Excel** (auditors, model reviewers, CI) | Buried in `/developers/engine/` | Non-Rust users (CI/audit audience #3 in `SITE_PLAN.md`) have to find CLI docs under `/developers/engine/` alongside unpublished Rust crates | Give `dnacalc` CLI its own card on `/` (kept) and a clear top section or dedicated `/cli/` route (or `/developers/engine/#cli` linked cleanly from `/` and `/download/`), separate from Rust crates |
| **5. Engine as library & CLI** | `/developers/`, `/developers/engine/` | Pinned links cite `539c4234`; `dc-17spk` (COM / `xlc` / Office.js faces) is not mentioned; `dc-6q2k.10` remains open | Finish `dc-6q2k.10` verification; keep crate status explicitly `Experimental / unpublished (publish = false, dc-z94tt)`; add a brief "In progress: exposed COM, C API, and Office.js faces (`dc-17spk`)" note |
| **6. CalcVBA outside Excel** | `/developers/vba/` | `{{vba}}` dumps all 234 rows from `baseline.json` with no summary table; `wasabi` (`windows`) rows appear unexplained | Add a 10-row suite summary above a `<details>` block (or filtered view) for the full 234-row baseline; update `SITE_PLAN.md` §2 to reflect that third-party projects now have Excel answers |
| **7. Handbook of Excel Functions** | `/handbook/` → `handbook.dnacalc.com` | Does not state CC BY 4.0 attribution or link to `/articles/` | Add one line noting CC BY 4.0 content licence and a link to `/articles/` |
| **8. Playground** | `/play/` → `/fun/*.html` | `docs/fun/index.html` footer says `Made 2026-11-12` and `dna-calc-brochure.html` has 2026-11-12 figures | Refresh `docs/fun/index.html` footer and `dna-calc-brochure.html` figures in the next content pass |
| **9. Articles & Blog** | `/articles/`, `/blog/` | `/articles/` has 9 complete articles (missing from main nav); `/blog/` is in main nav but empty without `--include-drafts` | Either publish the 3 ready draft posts (starting with `2026-11-21-what-is-dna-calc.md`) or link `/articles/` directly in main nav until blog posts are approved |
| **10. Download & Status** | `/download/`, `/status/` | Primary `/` CTA points to `/download/` where all 6 files say `"Not yet published"`; `/status/` calls the live build `"Local site preview"` | Make `/web/` or `/app/` the primary hero CTA until signed `0.2.0` installers ship; add "What you can run today" (`/app/`, `/onecalc/`, source build) to `/download/`; update `builds_table` and `published.json` on publish |

## 4. Navigation proposal

Current header (`marketing.rs:96-106`) has **9** items plus brand (`Home`, `DNA Calc`, `Formula Explorer`,
`In the browser`, `Developers`, `Handbook`, `Playground`, `Blog`, `Download`), and the footer has
`About`, `Licence`, `Status`, `Build record`.

Two options for owner decision:

- **Option A (keep current 9 slots, fix dead ends):**
  Change `Blog` to `Articles` (`/articles/`, which links `/blog/` once posts are published) or publish
  the first blog post before keeping `Blog` in the bar; link `/help/classic-grid/` from `/desktop/`,
  `/web/`, and `/onecalc/`.
- **Option B (recommended: 7 concise top-level items + brand):**
  1. `DNA Calc` (`/desktop/`) — desktop spreadsheet (Windows preview; Linux/macOS roadmap)
  2. `Browser app` (`/web/`) — run `.xlsx` locally in WebAssembly + launch `/app/`
  3. `Formula Explorer` (`/onecalc/`) — live formula workbench + Windows add-in bench
  4. `Developers` (`/developers/`) — `dnacalc` CLI, Rust crates, CalcVBA, Add-ins, Embedding
  5. `Handbook` (`/handbook/`) — function search + `handbook.dnacalc.com`
  6. `Articles` (`/articles/`, or `Blog` once posts are `Status: published`) — technical notes + posts
  7. `Download` (`/download/`) — planned `0.2.0` packages + source/CLI instructions

  Footer: `Playground` (`/play/`), `About` (`/about/`), `Licence` (`/licence/`), `Status` (`/status/`),
  `Build record` (`/build.json`), `llms.txt` (`/llms.txt`). (Moving `Playground` to a prominent home
  section + footer keeps the top bar single-row on desktop; or keep `Playground` in Option A if
  visibility in the header is preferred.)

## 5. Proposed copy changes (for owner approval before editing `pages/`)

1. **`pages/home.html` (hero):**
   - *Now:* kicker `"Windows desktop preview"`, primary button `"Preview downloads"` (`/download/`),
     secondary link `"Explore DNA Calc for Windows →"` (`/desktop/`).
   - *Proposed:* kicker `"Open-source Excel calculation engine · MIT"`, headline `"A spreadsheet for
     your Excel workbooks."`, lede `"Open and recalculate Excel workbooks in your browser or on
     Windows—built in Rust to match Excel down to the last bit, with tested VBA and native add-in
     support."`, primary button `"Try in your browser"` (`/web/` or `/app/`), secondary link
     `"Windows preview & downloads →"` (`/desktop/`).
   - Under `Compared with Excel`: add `"Read the technical investigations →"` (`/articles/`).
2. **`pages/web.html`:**
   - *Now:* `"Open local preview"` button; no explicit `"nothing is uploaded"` statement; `{{app_address}}`
     links `https://app.dnacalc.com/`.
   - *Proposed:* Button `"Open DNA Calc in your browser"` (`/app/`); add `"Nothing is uploaded: the
     workbook opens and recalculates locally in your browser in WebAssembly (`connect-src 'self'`)."`;
     remove or align `{{app_address}}` with the actual live app host.
3. **`pages/desktop.html`:**
   - Add a `"Six skins, one workbook"` paragraph (Grid, Notebook, Atlas, Tree, Canvas, Ledger) and
     `"Platform status: Windows x64 preview today; Linux desktop in exploration (`dc-icq4.5`);
     macOS planned."`
   - Update the coverage table: mark AutoFilter (`dc-971p.37`), row sorting (`dc-971p.38`), and
     `--engine` workbook events (`dc-1nbp.3`) as implemented; keep Page Setup (`dc-971p.57`),
     drawings (`dc-zjsg`), and WebView2 page-route event timing (`dc-1nbp.3.1`) listed honestly as
     in progress.
4. **`pages/onecalc.html`:**
   - Add a concise split right below the lede: `"In the browser: test any standard Excel formula
     with inputs, parts (`F9`), and step traces. On Windows desktop: also load native `.xll` and
     `.xlam` add-ins in an isolated surrogate host (values-only ceiling: `As Range`, `Caller`, and
     `ThisCell` return typed refusals)."`
5. **`pages/download.html`:**
   - Keep the `"Planned preview · 0.2.0"` table and honest `"Not yet published"` status, and add a
     `"Run it today"` block above the table linking `/app/` (browser spreadsheet), `/onecalc/`
     (Formula Explorer), and building `dnacalc` / desktop from source (`github.com/DnaCalc/DnaCalc`).
6. **`pages/status.html` & `marketing.rs:builds_table`:**
   - Replace `"Compare the local preview with the recorded public web builds"` and
     `"Local site preview, including both apps"` so a published build from `master` labels itself
     `"Current site build"` (or `"Published site build"` when `deploy/site/published.json` matches)
     rather than `"Local site preview"`.

## 6. Owner decisions needed

1. **Browser app URL (`layout.json` and `deploy/caddy/sites.caddy`):** Keep the full browser app at
   `https://dnacalc.com/app/` (same origin, works with existing Caddy config), or add DNS + a Caddy
   block for `https://app.dnacalc.com/`?
2. **Primary Home CTA before signed `0.2.0` installers:** Switch `/` hero primary button from
   `/download/` (`"Not yet published"`) to `/web/` or `/app/` (`"Try in your browser"`), keeping
   `/desktop/` and `/download/` as secondary links?
3. **Navigation bar (`NAV` in `marketing.rs`) and `/blog/` vs `/articles/`:** Keep 9 items or
   tighten to 7 (Option B)? Publish the 3 draft blog posts in `docs/site/blog/` (or promote
   `/articles/` into the nav bar until blog posts are `Status: published`)?
4. **Oracle corpus figure reconciliation:** Reconcile `docs/plan/RELEASES.md:99` (`78,162 / 78,296`
   at `bb2f3d1b`) with `docs/site/release-notes/next.md:24` (`78,167 / 78,296`) so `/` and
   `/status/` agree with the next release notes.
5. **Published build provenance (`deploy/site/published.json`):** Record the 2026-11-23 `dnacalc.com`
   publish in `deploy/site/published.json` and update `builds_table` wording in `marketing.rs`.
6. **Videos & screenshots (`dc-6q2k.4`):** Record Episodes 1–2 from the Windows cross-build preview
   now or after signed `0.2.0` MSIs (`dc-fk66.3`), and confirm project-only attribution on
   `pages/about.html`.
