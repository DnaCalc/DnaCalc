# Publishing DNA OneCalc and DNA Calc on dnacalc.com (dc-ysl)

The plan for epic dc-ysl. It was prepared on 2026-10-27 as read-only research
(`parked/assess/publishing.md`, outside the repository, read at `7aa7e11`) and copied here under
dc-ysl.1, updated with the owner's ruling below. Nothing in this plan is published without the
owner: every publish is run or approved by him.

## 0. Owner ruling, 2026-10-27, and the working defaults

**Ruling.**
- The current state of dnacalc.com and of the old subdomains does not matter.
- The owner has full control of the host.
- There are no legacy compatibility constraints: no redirect, retention period or legacy page has
  to be kept for compatibility. Section 4's decision 4 is therefore the owner's choice at the
  cutover, not a condition of it.
- Every actual publish is still run or approved by the owner.

**Working defaults** (section 4's recommended answers, taken until the owner says otherwise):

| Decision | Working default |
|---|---|
| 1. First publish | DNA OneCalc first, at onecalc.dnacalc.com, labelled a development build |
| 2. Apex | DNA Calc at the apex next, with a link to OneCalc; no separate home release |
| 5. CSP for styles | `style-src 'self' 'unsafe-inline'` for now; scripts strict. dc-xib moves the inline styles into the token sheet, after which `'unsafe-inline'` can go |
| 6. `/build.json` | Public, with the commit SHA |
| 7. Pipeline form | A: owner-run `cargo xtask site` commands; no GitHub Actions route, `ci` stays disabled |
| 9. Notices | LICENSE and a generated THIRD-PARTY-NOTICES.txt in every release, after the licence check (dc-ysl.2) |

Still the owner's, and not needed by any agent work: host access (decision 3), the legacy hosts at
the cutover (decision 4), the publish transport (decision 8) and dc-oy6l (decision 10).

**Agent work, as beads under dc-ysl** (local tooling only; no agent connects to the host):

| Bead | Package |
|---|---|
| dc-ysl.1 | This plan in the repository |
| dc-ysl.2 | Licence review of ported code; THIRD-PARTY-NOTICES.txt (P4) |
| dc-ysl.3 | `cargo xtask site pack` (P1, the bundle half) |
| dc-ysl.4 | `deploy/caddy/headers.caddy`, `cargo xtask site serve`, both smokes against the packed bundle under those headers (P1, the serve half) |
| dc-ysl.5 | `site publish --dry-run`, `site verify`, the Caddy files, the receiver and `OWNER_STEPS.md` (P2, P3) |
| dc-xib | Design tokens, dark theme, narrow screens and touch; inline styles into the token sheet (P7) |
| dc-p96 | Calculation in a web worker |

Sections 1 to 4 below are the research as written on 2026-10-27; where the ruling changes them,
a note says so.

## 0a. The host as surveyed on 2026-10-29, and the route chosen

Owner update, 2026-10-29: the work prepares **and** publishes to the host reached by `ssh
dna-koderbot`; the existing DNA Calc endpoint may be overwritten; the owner approved publishing DNA
OneCalc and DNA Calc once the checks pass. Only the dnacalc.com, www.dnacalc.com and
onecalc/dnaonecalc site blocks change; no other site, user, sudoers entry, ssh setting, key or
firewall rule.

**Read-only survey** (`ssh dna-koderbot`, 2026-10-29):

| What | Found |
|---|---|
| OS | Ubuntu 25.10, kernel 6.17.0-40 |
| ssh user | `koderbot` (sudo without a password, lxd) |
| Caddy | 2.6.2, active; `/etc/caddy/Caddyfile` (102 lines) with backups beside it |
| Site blocks | `koderbot.dnakode.com` (reverse proxies, not touched); `dnacalc.com` (root `/srv/dnacalc/landing`, `/dnatreecalc/*` from `/srv/dnacalc/dnatreecalc`); `www.dnacalc.com` (redirect to the apex); `dnaonecalc.dnacalc.com` (root `/srv/dnacalc/dnaonecalc`, the 2026-09-11 OneCalc preview, an 8.2 MB debug wasm); `dnatreecalc.dnacalc.com` |
| Headers now | nosniff, no-referrer, a Permissions-Policy; COOP/CORP on the subdomains; no CSP |
| Content | `/srv/dnacalc/{landing,dnaonecalc,dnatreecalc}`, owned by `caddy` |
| DNS (as the host resolves it) | dnacalc.com, www.dnacalc.com, dnaonecalc.dnacalc.com → 92.222.230.210; **onecalc.dnacalc.com has no record** |
| Org clones folder | `~/projects` (`~/projects/DnaCalc` among others) |
| Toolchains | rustup in `~/.cargo` (not on the non-interactive PATH), rustc 1.95.0 with wasm32-unknown-unknown; no wasm-bindgen CLI, no trunk; git, python3, curl |
| Disk | 339 GB, 274 GB free |

**Route: build here, upload the tar, switch on the host.** The workspace pins Rust 1.94.1 and
wasm-bindgen 0.2.117; the host has 1.95.0 and no wasm-bindgen CLI, and no browsers. Building
on the host would mean installing toolchains there and losing the two-browser smoke of the exact
bytes before the switch. `cargo xtask site pack` builds here; `site smoke` runs both apps' smoke
suites against the packed release under the site's own headers; `site publish` pipes the tar to
the receiver (`deploy/host/dnacalc-site-receive`, installed as `~/bin/dnacalc-site-receive`), which
checks every file against build.json and switches `current` atomically, keeping `previous`.
`build.json` carries the commit either way.

**Host names.** DNA Calc at dnacalc.com (www redirects to it). DNA OneCalc at
dnaonecalc.dnacalc.com until onecalc.dnacalc.com has a DNS record (an owner step; it cannot be set
from the host), then at onecalc.dnacalc.com too.

**Caddy change.** The three blocks are replaced by `import /etc/caddy/dnacalc/*.caddy`
(`deploy/caddy/headers.caddy` and `sites.caddy`). The candidate Caddyfile (the live one with that
replacement) was checked on the host with `caddy validate` on 2026-10-29: valid; its sites are
dnatreecalc.dnacalc.com, dnaonecalc.dnacalc.com, koderbot.dnakode.com, www.dnacalc.com and
dnacalc.com, as before. The exact steps, the backup and the rollback are in `deploy/OWNER_STEPS.md`.

## 1. What exists now

### 1.1 Hosting, as far as the files show

No repository holds deployment material for dnacalc.com. There is no `deploy/` folder, Caddy
file, `expect.json`, `published.json`, publish script or publish receipt in DnaCalc, DnaTreeCalc,
DnaOneCalc, Foundation or DnaCalc-infra (a full-text search of each found none; DnaCalc-infra is a
lane clone of DnaCalc). The only record of the live setup is the host survey written into the
deployment epic dc-gh6q on 2026-10-18 (commit `6cea24c`). The epic and its 14 children were deleted
from the live tracker in the bulk-port rework (`11cc4f2`, 2026-10-23). They survive only in git
history: `git show 11cc4f2~1:.beads/issues.jsonl`.

What that survey recorded (not re-checked; no server was contacted):

- **dna-koderbot**, 92.222.230.210 (the `dna-koderbot` and `dna-koderbot-ubuntu` ssh aliases in
  `~/.ssh/config` point there). Ubuntu 25.10 KVM, 16 vCPU, 61 GiB RAM, no swap.
- **Caddy 2.6.2** serves:
  - the apex dnacalc.com and www (a legacy landing page);
  - the dnatreecalc and dnaonecalc hosts (dnaonecalc serves a legacy OneCalc wasm preview);
  - koderbot.dnakode.com, which must stay untouched.

  Content lives under `/srv/dnacalc`, with one block per host in `/etc/caddy/Caddyfile`. The Caddy
  admin API listens on 127.0.0.1:2019.
- **Access.** The agents' ssh user `koderbot` has NOPASSWD sudo and lxd membership, so agent
  sessions currently have root on the site host. Other services on the host: 8765, 18789 and
  18790; cups on 631. One proxied service has no basic auth.
- **DNS.** The apex resolves to dna-koderbot. onecalc.dnacalc.com and calc.dnacalc.com have no
  records.

### 1.2 Previous publish routes

None is recorded anywhere. The legacy apps were built locally, then placed on the server by
hand:

- DnaTreeCalc used trunk; `dist/` holds `index.html`, `dnatreecalc-web-<hash>.js` and `_bg.wasm`.
- OneCalc used `DnaTreeCalc/scripts/onecalc/run-onecalc-preview.ps1`, a debug wasm-bindgen build
  with a local PowerShell server.

There are no receipts, no record of which commit is live on any host, and no rollback procedure.
(Under the 2026-10-27 ruling, how the legacy sites got there no longer matters.)

### 1.3 What the repository has now

- **Both apps exist and pass their browser smoke tests locally.**
  - D3 (dc-q4y, closed): DNA OneCalc. The wasm is 1.78 MB, about 624 KB gzipped. The smoke passes
    23/23 in headless Chrome and Firefox.
  - D4 (dc-rif, closed): DNA Calc with the grid, notebook, tree, canvas, ledger and atlas skins.
    The smoke passes 37/37.
- **`cargo xtask app bundle onecalc|calc [--out]`** (`tools/xtask/src/app.rs`):
  - builds `apps/<pkg>/src/bin/<pkg>-web.rs` for wasm32 in `[profile.web]` (opt-level 3, LTO, one
    codegen unit);
  - runs wasm-bindgen 0.2.117 (`--target web`, version checked exactly);
  - copies `web/index.html`.

  The output is three files with fixed names: `index.html`, `<entry>.js` and `<entry>_bg.wasm`.
  There are no hashed names, no `/build.json`, no LICENSE or notices, and no record of the commit.
- **`cargo xtask app smoke`** serves the bundle on 127.0.0.1 from its own small server and drives
  headless Chrome and Firefox through WebDriver. It fails on any panic or page error. The server:
  - serves only names without a `/`, so it cannot serve an `assets/<id>/` subfolder;
  - sends `Cache-Control: no-store` and no security headers.

  The smoke has therefore never run under a CSP.
- **Facts that set the headers:**
  - Neither app uses a Worker, SharedArrayBuffer, wasm threads or `fetch`. The parallel
    recalculation of dc-2m0 is native-only. **No cross-origin isolation (COOP/COEP) is needed.**
    The DNA Calc worker that old C1 planned was never built; HostState runs on the main thread.
  - Both `index.html` files carry an inline `<script>` (the fault recorder), an inline module
    `<script>` (the bootstrap) and an inline `<style>`.
  - At run time, Leptos renders `<style>{CSS}</style>` elements (once in OneCalc, seven times in
    DNA Calc). DNA Calc also sets about 25 `style=` attributes and two `inner_html` (SVG, icons).
  - So the strict CSP drafted in dc-gh6q (`script-src 'self' 'wasm-unsafe-eval'; style-src
    'self'`) **would break both apps as they are**.
- **GitHub.** The `ci` workflow is disabled by the owner (Actions minutes; AGENTS.md line 125) and
  stays disabled. The repository is private on GitHub Free, which has no environments, required
  reviewers or branch protection on private repositories.
- **Licence.** The workspace is MIT, Copyright (c) 2026 DNA Kode, Inc. (dc-cyd7). dc-cyd7's note on
  S1 and S3 requires every published release to carry LICENSE plus a third-party notices file. The
  bundle includes ported numeric code with its own notices:
  - `crates/calc-func/src/ported/excel_numeric/crt_libm.rs`: AMD libm, BSD-3, whose clause 2
    requires the notice in binary redistributions;
  - `crt_pow.rs`: Arm optimized-routines, MIT or Apache-2.0 WITH LLVM-exception;
  - `bessel_convert_family.rs`: names the Numerical Recipes algorithm.

  The licence-review bead dc-4546 was deleted in the rework. `cargo xtask ci licenses` only checks
  crate licences against `tools/xtask/licenses.json`; it writes no notices file.
- **No `C:\Work\DnaCalc\DnaCalc-owner` clone exists yet.** dc-gh6q planned it as the owner's
  publishing clone.

### 1.4 The deferred rulings in dc-oy6l, and what each blocks today

dc-oy6l (deferred, P1) is still in the tracker. Every bead it held back (W1, K1a–K4, R1, S1–S3,
O1, O2, C1, C2, and dc-b7w4) was deleted on 2026-10-23, so several rulings no longer block
anything.

| Ruling in dc-oy6l | Status | What it blocks now |
|---|---|---|
| Hostnames: DNA Calc at the apex, OneCalc at onecalc.dnacalc.com, home page at the apex until C2 | **Decided 2026-10-18** | Nothing. Whether the interim home page is still wanted is open (decision 2 below) |
| Retirement of apex+www, dnatreecalc, dnaonecalc, each approved separately; backup and DNS retention | Open | The apex cutover and the legacy redirects. **Not** a OneCalc-only first publish on a new subdomain |
| Host access (K1a): remove or touch-protect the agents' koderbot key; drop koderbot's NOPASSWD sudo and lxd; ubuntu as touch-required admin | Open | Any publish route where "the owner's yes" must mean more than a promise: today any agent session can rewrite /srv as root |
| Agent credential: fine-grained token without Workflows, org 2FA | Open | Only a route that adds workflows. Not needed for the recommended route |
| AGENTS.md amendments (held category, owner-pushed workflows, no ssh to koderbot, runner rules) | Open | The "no ssh to dna-koderbot" rule matters for publishing; the runner rules only for a runner |
| Container limits, egress ACL, firewall (runner) | Open | Only option B of section 3 |
| Legacy root on koderbot (K4) | Open | Only the test-machine half; not publishing |
| GitHub Free: artifact retention; no CI publish route until a plan with required reviewers | Open | Confirms there is no Actions publish route now |
| Graph: onecalc-m8.1 and dnacalc-m8.1 depend on O2 and C2 | **Moot**: those beads were deleted | Nothing |
| Public /build.json fields (site, commit, built UTC, file hashes; exposes private SHAs) | Open | The site format (decision 6) |
| Browsers in the container; tracker status page | "Not filed" | Nothing |
| HSTS (one year, no preload) only after a clean cutover | Open | Nothing now; set later |
| Standing approval: not available | Open | Keeps every publish owner-run |
| Hardening: Caddy admin to a unix socket, cups 631, LXD API off the network, review the unauthenticated proxied service | Open | Nothing in the code; good to do at the same session as host access |
| Priorities P1/P2/P3 | Moot | The bulk-port plan puts dnacalc.com under "Later" |

## 2. Smallest safe route to a first public publish

**Order:**
1. DNA OneCalc at **onecalc.dnacalc.com**. This is a new host: it replaces nothing and needs none
   of the retirement rulings.
2. DNA Calc at the **apex**, together with the legacy cutover, each host approved separately.

Both are labelled "development build". The word "shipped" is not used.

### 2.1 Site bundle layout

Built by a new `cargo xtask site pack onecalc|calc`, which reuses `app::bundle`. It refuses
unless the tree is clean and HEAD is on origin/master after a fetch.

```text
target/site/<site>/<release>/            release = <UTC yyyymmddThhmmssZ>-<sha12>
  index.html                             rewritten to point at assets/<id>/
  build.json                             public build record (2.3)
  LICENSE                                workspace MIT
  THIRD-PARTY-NOTICES.txt                cargo metadata licence texts + ported-code notices
  assets/<id>/boot.js                    fault recorder + module bootstrap, moved out of index.html
  assets/<id>/<entry>.js                 wasm-bindgen glue
  assets/<id>/<entry>_bg.wasm
target/site/<site>/<release>.tar         uncompressed, the publish payload
target/site/<site>/<release>.pack.json   private record: tar sha256, smoke result, tool versions
```

- `<id>` is the first 16 hex digits of the sha256 over the asset files.
- On the server, `assets/` is shared and append-only per site:
  - `/srv/dnacalc-site/<site>/assets/<id>/`;
  - each release under `releases/<release>/`;
  - a `current` symlink, plus `previous`.

  A page loaded before a switch still finds its old assets, because they are never removed with
  the switch. This replaces dc-gh6q's `current`-then-`previous` try_files rule, and is simpler.
  Old asset folders are pruned by hand after 30 days.
- Moving the two inline scripts into `boot.js` is a small app change: both `web/index.html` files,
  plus `app.rs` copying `web/boot.js`. The smoke's fault list (`window.__onecalcFaults`) stays as
  it is.

### 2.2 Headers (one Caddy snippet, used by every site and by the local smoke)

`deploy/caddy/headers.caddy`:

```text
Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval';
  style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self';
  worker-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none';
  frame-ancestors 'none'
X-Content-Type-Options: nosniff
Referrer-Policy: no-referrer
Cross-Origin-Resource-Policy: same-origin
/index.html, /build.json:  Cache-Control: no-cache
/assets/*:                 Cache-Control: public, max-age=31536000, immutable
encode zstd gzip
```

- **`'unsafe-inline'` for styles** is the cheapest way to keep the Leptos `<style>` elements and
  `style=` attributes working. Scripts stay strict. The alternative is to move all CSS into a
  stylesheet and the positions into CSSOM calls (`style:` directives), which is about a day of
  DNA Calc work. Owner decision 5.
- **`'wasm-unsafe-eval'`** is required for `WebAssembly.instantiateStreaming` under a script-src
  CSP.
- `.wasm` must arrive as `application/wasm` (Go's MIME table gives this). The smoke and `site
  verify` check it.
- **No COOP/COEP.** Nothing needs cross-origin isolation (1.3). If wasm threads or
  SharedArrayBuffer ever arrive, add `Cross-Origin-Opener-Policy: same-origin` and
  `Cross-Origin-Embedder-Policy: require-corp`. Every asset is same-origin, so that would be a
  header change only.
- `connect-src 'self'` also states a privacy fact: DNA Calc opens workbooks from a local file
  input, and the page cannot send them anywhere.
- No HSTS until 30 days after a clean apex cutover (dc-oy6l). It is sticky.

### 2.3 Versioning and /build.json

- **Release ID:** `<UTC>-<sha12>`. The app version is the workspace `0.1.0` until the owner wants
  semver. The page footer or about box shows the commit by reading `build.json`. That is optional
  for the first publish.
- **`/build.json`**, which is public:

  ```json
  {"format":"dnacalc-site-build/1","site":"onecalc","release":"20261028T101500Z-7aa7e1128523",
   "commit":"7aa7e1128523e2b72ae4a62a999a329f951f38e0","version":"0.1.0",
   "built_utc":"2026-10-28T10:15:00Z","rustc":"1.94.1","wasm_bindgen":"0.2.117",
   "files":{"index.html":{"bytes":1234,"sha256":"..."}, "...":{}}}
  ```

  It exposes private commit SHAs, which the owner rules on (decision 6). Receipts and smoke results
  stay out of it, in `pack.json` and `deploy/site/published.json` in the private repository.

### 2.4 Local static-serve smoke (before every publish)

1. `cargo xtask site serve <site> [--release <dir>]`. This extends app.rs's server to serve
   subpaths and to send the headers parsed from `deploy/caddy/headers.caddy`, so the local smoke
   and the live site share one header source.
2. `cargo xtask app smoke <app> --url http://127.0.0.1:<port>/`. Add `--url` to the existing smoke,
   plus a `securitypolicyviolation` listener in `boot.js` that writes into the same fault list. All
   23 or 37 cases must pass in Chrome and Firefox with zero faults.
3. `site verify --dir <release> --url <local>` checks that every served byte hash-matches
   `build.json`, the MIME types (wasm, js) and that the headers are present.
4. Optional, needing the owner's yes to download Caddy 2.6.2 for Windows: run the real Caddy
   snippet locally against the release. This is the only way to check the Caddy file itself
   before the server does.

### 2.5 The owner-run publish step

**One-time owner setup:**
1. Do the host-access steps (decision 3).
2. Create `dnacalc-deploy`, a system user whose only access is a forced command
   (`deploy/host/dnacalc-site-receive`). It is a Python 3.12+ standard-library script that:
   - reads an uncompressed tar from stdin with `tarfile`'s `filter="data"`, which refuses absolute
     paths, links outside the tree and device files;
   - checks every file against `build.json`;
   - writes to `releases/<release>.partial`, renames it, then flips `current` and `previous`
     atomically under a flock.

   It also has `status <site>`, `list <site>` and `rollback <site> <release> --if-current <r>`.
   This is dc-gh6q's R1, slimmed.
3. Install the sshd drop-in that confines the user (ForceCommand, no tty or forwarding, `Match all`
   at the end).
4. Create the publish key `ssh-keygen -t ed25519-sk -O verify-required`, held only by the owner.
5. Add the DNS A record `onecalc.dnacalc.com → 92.222.230.210`. Only once it resolves, install
   `deploy/caddy/onecalc.caddy` (root `/srv/dnacalc-site/onecalc/current`, the shared snippet),
   then `caddy validate` and reload.
6. Create the `C:\Work\DnaCalc\DnaCalc-owner` clone.

**Each publish (owner, in DnaCalc-owner):**

```text
git pull --no-rebase
cargo xtask site pack onecalc                  # builds, writes release + tar + pack.json
cargo xtask site serve onecalc & cargo xtask app smoke onecalc --url ...   # 2.4
cargo xtask site publish onecalc --dry-run     # diff against the live /build.json, no key needed
cargo xtask site publish onecalc               # ssh dnacalc-deploy@dna-koderbot publish onecalc < tar
                                               #   (the hardware-key touch is the approval)
cargo xtask site verify onecalc --url https://onecalc.dnacalc.com/
git commit deploy/site/published.json          # release, commit, build.json sha256, smoke result
```

- Caddy needs no reload per publish, because its root is the `current` symlink.
- `site publish` and `site rollback` refuse to run without a terminal, so no agent or script can
  publish unattended.

### 2.6 Rollback

- **Normal:** `cargo xtask site rollback onecalc --release <previous>`. It reads `current` from
  `status` and passes `--if-current`. It is an atomic symlink flip, takes effect immediately, and
  needs no Caddy reload. Releases and assets are never deleted by a switch.
- **Host level** (apex cutover only): restore the Caddyfile from the 0600 backup in
  `/var/backups`, `caddy validate`, reload. This works until `/srv/dnacalc` (the legacy content) is
  removed, which should wait for decision 4's retention period.
- **Emergency without the tool:** as the admin user, `ln -sfn releases/<r> current.tmp && mv -T
  current.tmp current`.

## 3. The web update pipeline (push to master → published site, owner approves each time)

CI is disabled and stays disabled, so the pipeline cannot rely on GitHub-hosted jobs firing on
push. Four forms were weighed.

| | A. Owner-run xtask (local) | B. Self-hosted runner on dna-koderbot | C. Owner-signed tag, pulled by koderbot | D. Dispatch-only GitHub job builds, owner publishes |
|---|---|---|---|---|
| How approval happens | The owner's key touch on `site publish` | The owner dispatches `site.yml`. On Free private there are no required reviewers, so any account with Actions write can dispatch | The owner pushes a tag `publish/<site>/<n>` signed with his ssh signing key. The host checks it with `git verify-tag` against an allowed-signers file | The owner downloads the artifact and runs A's publish |
| Actions minutes | 0 | 0 (self-hosted jobs are not billed) | 0 | ~10–15 Linux min per build (cold LTO wasm build) |
| Standing secrets | None. The key is a hardware key in the owner's hand | Runner registration on the site host; anyone who can push runs code there | A read-only deploy key on the host | None, if it only uploads an artifact. A repository secret to push would be readable by any workflow, so no |
| Owner work to set up | Section 2.5 (about 1–2 h) | All of K1a: LXD container, egress ACL, runner, hook, monthly upkeep, re-registration (about 1–2 days, then hours each month) | 2.5, plus a build container with Rust 1.94.1 and wasm-bindgen 0.2.117 (about half a day) | 2.5, plus pushing one workflow file |
| Owner work per publish | About 10–20 min, mostly build and smoke time on his PC | One dispatch | One `git tag -s` and push | Download, then A |
| Smoke in two browsers before the switch | Yes (owner's PC) | Only with browsers in the container ("not filed") | Same as B | No |
| Risk | Lowest | Highest: a runner beside the site and the private Caddy | Medium: the build runs on the site host | Low, but spends the minutes that caused CI to be disabled |

**Recommendation: A now.** It is the only form that adds no standing credential, no workflow, no
Actions minutes and no new service on the site host. It also keeps the two-browser smoke in front
of every switch.

The split of work under A:
- Agents do everything up to the publish:
  - land on master;
  - run `site pack`, `site serve` and smoke, and `site publish --dry-run` in their own clone;
  - post the release ID, the diff and the smoke result for the owner.
- The owner repeats pack in DnaCalc-owner. The tar is reproducible from the commit, so its hashes
  must equal the agent's `pack.json`. Then he publishes with the key touch.
- Revisit **C** if publishes become weekly or more often. It reuses the same receiver and pack
  format, and the signed tag is an approval no agent can forge.
- B and D are not recommended.

**Drift check.** dc-gh6q planned a daily scheduled `site.yml`. That costs about 30 Linux min a
month. Instead, run `cargo xtask site verify --drift` at each publish and weekly by hand: it
compares each live `/build.json` with `published.json` at origin/master. If an unattended daily
check is wanted, a systemd timer on dna-koderbot (as an unprivileged user, reading only
`https://…/build.json`) costs no minutes.

## 4. Owner decisions (with recommended answers)

1. **First publish.** Publish DNA OneCalc alone at onecalc.dnacalc.com as a development build,
   before touching the apex?
   *Recommended: yes.* It needs only a DNS record and a new Caddy file. No legacy host changes.
2. **Apex.** dc-oy6l says the apex serves a home page until DNA Calc's first publish. DNA Calc now
   exists (D4).
   *Recommended: skip the separate home release.* At the cutover, publish DNA Calc at the apex,
   labelled a development build, with a link to OneCalc.
3. **Host access before the first publish.**
   *Recommended: yes, the minimum:*
   - remove or touch-protect the agents' `koderbot` key;
   - remove koderbot's NOPASSWD sudo and lxd membership;
   - do admin work as `ubuntu` with a touch-required key;
   - add "agents do not ssh to dna-koderbot" to AGENTS.md.

   First check what other projects' koderbot automation needs sudo. Do the hardening items in the
   same session: Caddy admin to a unix socket, cups, the LXD API, the open proxied service.
4. **Legacy hosts at the apex cutover** (each approved separately). Under the 2026-10-27 ruling
   there are no compatibility constraints; these are conveniences the owner may keep or drop.
   *Recommended:*
   - www → apex (308);
   - dnatreecalc → apex (301);
   - dnaonecalc → onecalc.dnacalc.com (301) at the OneCalc publish, retiring the old preview;
   - keep the backups and the legacy `/srv/dnacalc` for 90 days;
   - keep the DNS records for as long as the redirects should work (at least 12 months).
5. **CSP for styles.**
   *Recommended: `style-src 'self' 'unsafe-inline'` now, scripts strict.* Moving DNA Calc's inline
   styles out can follow as its own bead.
6. **Public `/build.json` with the commit SHA.** *Recommended: yes.* It lets anyone, the drift
   check included, tie the live bytes to a commit. The repository stays private.
7. **Pipeline form.** *Recommended: A (owner-run `cargo xtask site`)*, with C as the later upgrade.
   No GitHub Actions publish route, and `ci` stays disabled.
8. **Publish transport.** *Recommended:* the `dnacalc-deploy` forced-command user with an
   `ed25519-sk verify-required` key. Fallback for the very first publish only: the owner copies
   the tar as `ubuntu` and runs the receiver script with sudo by hand.
9. **Licence notices.** *Recommended:* every release carries LICENSE plus a generated
   THIRD-PARTY-NOTICES.txt:
   - crate licences from cargo metadata;
   - the AMD libm BSD-3 and Arm optimized-routines notices.

   Also confirm that the Bessel code (Numerical Recipes algorithm) was re-implemented from the
   algorithm and not copied, before the first binary goes public.
10. **dc-oy6l itself.** *Recommended:*
    - record rulings 1–9 there and close it;
    - park the test-machine half (runner, legacy root, issue raising) under dc-8abf or a new
      deferred bead, since WSL covers Linux evidence;
    - HSTS 30 days after a clean apex cutover.

### Work packages to file (in the bulk-port style: light, one package each)

| Key | Package | Needs |
|---|---|---|
| P1 | **site pack + serve + smoke-by-URL**: `cargo xtask site pack onecalc\|calc`, `site serve`, `app smoke --url`, `site verify --dir/--url`. Covers: inline scripts moved to `boot.js` in both apps; hashed `assets/<id>/`; `build.json`; LICENSE and notices; a reproducible tar; `pack.json`; headers read from `deploy/caddy/headers.caddy`; a `securitypolicyviolation` listener | Decisions 5, 6, 9 |
| P2 | **deploy/ files**: `headers.caddy`, `onecalc.caddy`, `apex.caddy`, `www.caddy` and the redirect files; the receiver (Python, `tarfile` data filter, flock, atomic switch, rollback, status, list); the sshd drop-in; `OWNER_STEPS.md` and `CUTOVER.md`. The receiver is tested in the WSL clone against a temp root | Decisions 3, 8 |
| P3 | **site publish / rollback / status / verify --drift** client commands, which refuse to run without a terminal; `deploy/site/published.json` format | P1, P2 |
| P4 | **Third-party notices and licence review** of ported numeric code (replaces the deleted dc-4546) | Decision 9 |
| P5 | **Owner-held: first OneCalc publish**: host access, deploy user and key, DNS, Caddy file, publish, verify, published.json | P1–P4, decisions 1, 3, 8 |
| P6 | **Owner-held: apex cutover**: backups, DNA Calc at the apex, www/dnatreecalc/dnaonecalc redirects, one approval each; legacy removed after the retention period | P5, decisions 2, 4 |
| P7 | Optional: **DNA Calc styles to stylesheet/CSSOM**, then drop `'unsafe-inline'` | P6 |
| P8 | Later: **form C** (signed-tag pull build on dna-koderbot) | Only if publishes become frequent |

Estimated agent effort for P1–P4: about 2–3 days in one clone. P5 and P6 need the owner for about
2–3 hours in total, plus DNS propagation.
