# DNA Calc developer APIs and llms.txt

Design for owner review, 2026-11-22. Give developers three entry points on dnacalc.com: product
pages to choose an app, `/developers/` to choose an integration, and `/handbook/` to find the
behaviour of an Excel function. Present the API status and supported targets beside each example.
The [site plan](SITE_PLAN.md) describes the audiences and the
[embedding plan](../plan/ENGINE_EMBEDDING.md) defines the proposed workbook facade.

## Developer entry points

| Reader and route | Surface to document | First example and status |
|---|---|---|
| Embed the engine, `/developers/engine/` | Use `calc_host::HostSession` for workbooks, `calc_fml::parse_formula` for syntax and diagnostics, and `calc_func::invoke` with `calc_value` and host-supplied services for function calls. Describe `calc-doc` and `calc-engine` as lower-level contracts. | Link the existing Rust workbook round-trip, parser and function doctests. Show where cached values, recalculated values and typed refusals appear. The crates are unpublished. |
| Check workbook files, `/developers/engine/` | Document the `dnacalc` Linux x86-64 runner, its command and exit status contract, and the shape of `--json` output before treating it as a scripting interface. | Run `dnacalc recalc book.xlsx --out checked.xlsx` and `dnacalc diff checked.xlsx` on the existing sample. Explain that `diff` checks against values saved in its input file. |
| Host VBA, `/developers/vba/` | Call the standalone runtime **CalcVBA** and link its own host reference for project lifetime, capabilities and backend availability. In DNA Calc, describe the `calc-vba` adapter's workbook and values-only profiles, and document the file-based XLSM and XLAM routes separately. | Use a worksheet UDF from the existing simple example. Show the result in the supported route and the typed refusal when a values-only call needs a workbook object. Treat the Rust adapter as internal until a host contract is approved. |
| Test an add-in, `/developers/addins/` | Document the Excel-facing XLL callbacks and XLAM capabilities demonstrated by the Windows x64 previews, by app and registration or call path. Label the coverage with the measured build and fixture set. | Load an existing test add-in in Formula Explorer, invoke its exported function and compare the result type and value with Excel. Link the tested callback table and explain refusal reporting. |
| Embed Formula Explorer, linked from `/onecalc/` and `/developers/` | Use **DNA Calc - Formula Explorer** as the product name. The site already mounts it through the `dna-onecalc` browser surface. Start from that mount when defining a component contract for the host element, initial formula and inputs, result or diagnostic events, and disposal. | Mount the same component the site uses and run a single formula. Label the component contract experimental when offered to integrators. Keep the desktop bridge, worker messages and hashed asset URLs internal. |

Use existing crate names, Rust module paths and Excel spellings in the reference. Give every
surface one status. **Internal** means callers have no compatibility commitment. **Experimental**
means pin the documented version or commit and expect breaking changes in previews. **Supported**
means the owner has approved a documented compatibility contract and changes follow its versioning
rules. No surface moves to Supported solely because a symbol is public in Rust.

Follow [dc-ysl.11 and the release plan](../plan/RELEASES.md) for product versions, preview and
stable streams, and build commit metadata. Start the published crate set at the product version
and allow crate-level SemVer thereafter. Pin rustdoc and examples to the crate version and site
build they describe. Version a machine-readable schema independently once consumers can depend on
it. Record deprecations and migration steps in release notes. Track claims about Excel separately
by tested Excel build, platform and CPU where recorded.

Generate signature and type reference from versioned rustdoc and CLI reference from the runner's
help and schema once defined. Write guides for choice of entry point, prerequisites, examples,
limitations and failure handling. Compile Rust examples as doctests and exercise the CLI, add-in
and component examples through their existing application paths. Link the reference from the
guides and carry links back to the guides in crate-level rustdoc.

## Handbook of Excel Functions

Give the [Handbook catalogue](https://handbook.dnacalc.com/catalogue/index.html) a direct link
from Developers, the engine reference and the product site's Handbook page. From a documented
function, link to its canonical [function page](https://handbook.dnacalc.com/functions/ROUND.html)
for behaviour, examples, quirks, implementation and versions. Link to the matching
[function JSON](https://handbook.dnacalc.com/api/functions/FUNC.ROUND.json) for structured sources,
variants and evidence. Resolve aliases and operators through the Handbook's
[ID index](https://handbook.dnacalc.com/api/index.json) rather than deriving paths from display
names. Use release-pinned DNA Calc references when explaining behaviour for a particular build.
The Handbook has its own revision and CC BY 4.0 content licence. Attribute any reused Handbook
prose and link to the source. [Handbook repository](https://github.com/DnaCalc/ExcelFunctionsHandbook)

The Handbook's `tools/efh-site` already writes a root
[llms.txt](https://handbook.dnacalc.com/llms.txt). Revise that generator output in place. Use the
catalogue and emitted reference records to link the human index, JSON index and schema, and a few
examples such as ROUND. The JSON index gives agents the complete per-entry route. Describe
evidence by build and environment where present, label empty evidence as unmeasured, and identify
OneCalc examples as DNA Calc results. The hosted Handbook is static and currently returns HTTP 503
for `/api/eval` and `/api/contributions`. Leave those calls out of the hosted task list and link
back to DNA Calc's index once both documents are published. [Handbook build tool](https://github.com/DnaCalc/ExcelFunctionsHandbook/tree/main/tools/efh-site)

## The product site's index

Generate `/llms.txt` as UTF-8 Markdown for the content site. Start with `# DNA Calc` and a short
blockquote that identifies the spreadsheet, Formula Explorer and developer tools. Use these H2
sections with bullets linking absolute canonical URLs and brief descriptions, following the
[llms.txt proposal](https://llmstxt.org/#format).

| Section | Include |
|---|---|
| Products | Windows desktop, browser app overview, Formula Explorer and Downloads. Explain where the browser and desktop feature sets differ. |
| Developer guides | Developers, engine and runner, VBA and add-ins. Link component instructions and versioned reference once their routes exist in the release. |
| Handbook | Product Handbook entry point, Handbook `/llms.txt`, catalogue and representative function pages and JSON. |
| Evidence and articles | Status and `/build.json` for build provenance, the [Function Bank preview](../fun/function-bank.html) for measured answers, and the last-bit article index and published articles for numerical investigations. |

For the first release, keep the index small. Use the existing Markdown sources to emit
`index.md` next to directory-based articles and guides, with the same editorial visibility and
resolved links as HTML. Link HTML for HTML-authored pages until shared content can supply both
formats. Advertise actual Markdown editions from HTML with `rel="alternate" type="text/markdown"`
and the index with `rel="describedby"`. Defer `/llms-full.txt` until there is a use for an
aggregate. If added, concatenate selected public Markdown from the same build with canonical
links, deduplicating pages and keeping evidence tables in their source documents.

## Build and verification

In `site pack content`, generate the index before collecting files for `build.json`. Extend
`marketing.rs`'s existing route declarations with descriptions and index inclusion, and use
`build_guides` and the blog's publish filter for Markdown-backed routes. Use the configured
canonical content origin for apex links, independently of `browser_app_url` in `layout.json`.
Reuse the data behind the existing status and evidence pages, and link to figures with their
denominators and dates. Keep draft-only routes out of the index even in a draft preview.

Extend `site verify content` to require the index in the manifest. Check its heading order, link
syntax, UTF-8 and response type, and resolve every same-origin link against the served release
including anchors. Check HTML alternates and Markdown links against the same route inventory.
The existing manifest verification then checks served hashes and response headers. The Handbook
generator should check its own index through its existing build, link and claim checks, using its
emitted route and ID maps. At owner-approved release review, check the cross-site links against
their intended hosts. Keep local builds reproducible by checking same-release paths without
network access and testing external route expectations with fixtures. Verify agent discovery
from each index by following the links for a workbook round trip, an XLL bench example and a
function quirk with an evidence record.

## Follow-up work

- Developer guides and preview API contracts (`dc-6q2k.10`).
- Product site index, Markdown editions and verification (`dc-6q2k.11`).
- Handbook index and cross-site links (`dc-6q2k.12`).
- Crate reference coverage remains with `dc-z94tt`. Release and versioning decisions remain with
  `dc-ysl.11`. Publication remains with `dc-6q2k`.
