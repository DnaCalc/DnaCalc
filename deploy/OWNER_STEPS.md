# Publishing DNA OneCalc and DNA Calc: the owner's steps (dc-ysl.5)

The plan is `docs/plan/PUBLISHING_PLAN.md`; the tools are `cargo xtask site`
(`tools/xtask-harness/src/site.rs`), `deploy/host/dnacalc-site-receive` and `deploy/caddy/`. Every
publish is run or approved by the owner.

| Site | Host | Root on the host |
|---|---|---|
| `calc` (DNA Calc) | dnacalc.com (www.dnacalc.com redirects) | `/srv/dnacalc-site/calc/current` |
| `onecalc` (DNA OneCalc) | onecalc.dnacalc.com | `/srv/dnacalc-site/onecalc/current` |

**Route** (owner, 2026-11-08): everything runs on the site host dna-koderbot, as `koderbot`, in a
run folder under `~/codex-runs/` holding a fresh clone of the pushed master. The release is
packed, smoked in both browsers and switched there; nothing is built on Windows or in WSL, and
the `~/projects` clones other sessions use are not touched. The host has the workspace's Rust
(1.94.1) with the wasm32 target, the wasm-bindgen 0.2.117 CLI, Node 24 and python3. The smoke
browsers are the `chromium` and `firefox` snaps (`/snap/bin/chromium.chromedriver`,
`/snap/bin/geckodriver`), kept apart from the browsers the UI lanes keep in `~/.local/bin`.

## 0. Before the first public publish

The licence review (`docs/plan/THIRD_PARTY_REVIEW.md`, section 3) lists code in the wasm that a
notice cannot cover (rows 1 to 7). The owner rules on each before the first public binary.

## 1. One-time setup on the host

On dna-koderbot as `koderbot`, which has sudo. Nothing here touches users, sudoers, ssh, keys or
the firewall. Done on 2026-11-08 except where a step says otherwise; repeat a step only if its
check fails.

```text
ssh -t dna-koderbot
  # The release root, writable by koderbot, readable by Caddy.
  sudo install -d -o koderbot -g koderbot -m 755 /srv/dnacalc-site

  # The smoke browsers and their drivers (each snap ships the driver for its own browser).
  sudo snap install chromium firefox
  /snap/bin/chromium.chromedriver --version; /snap/bin/geckodriver --version

  # A run folder with a fresh clone of the pushed master (section 2 makes one per publish).
  R=~/codex-runs/site-publish/$(date -u +%Y%m%dT%H%M%SZ); mkdir -p $R
  git clone https://github.com/DnaCalc/DnaCalc.git $R/repo && cd $R/repo
  git config user.name '<your name>'; git config user.email '<your email>'   # for published.json

  # The receiver. `site publish` and `site rollback` refuse one that is not the clone's copy.
  install -m 755 deploy/host/dnacalc-site-receive ~/bin/dnacalc-site-receive
```

Then publish both sites once (section 2, up to `site publish`), so each `current` exists, and only
then point Caddy at them (done on 2026-10-30; the site files were last updated 2026-11-07):

```text
  stamp=$(date -u +%Y%m%dT%H%M%SZ)
  sudo cp -p /etc/caddy/Caddyfile /etc/caddy/Caddyfile.bak.$stamp
  sudo install -d -m 755 /etc/caddy/dnacalc
  sudo install -m 644 deploy/caddy/headers.caddy deploy/caddy/sites.caddy /etc/caddy/dnacalc/
  # In /etc/caddy/Caddyfile, use one import for the DNA Calc and OneCalc site blocks:
  # import /etc/caddy/dnacalc/*.caddy
  sudo caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile   # must say Valid
  sudo systemctl reload caddy
```

Confirm that `onecalc.dnacalc.com` resolves to the host (`92.222.230.210`) and serves OneCalc over HTTPS.

## 2. Each publish (one site at a time)

All on the host, at a terminal (`ssh -t`), in a new run folder:

```text
ssh -t dna-koderbot
  export CHROMEDRIVER=/snap/bin/chromium.chromedriver GECKODRIVER=/snap/bin/geckodriver
  R=~/codex-runs/site-publish/$(date -u +%Y%m%dT%H%M%SZ); mkdir -p $R
  git clone https://github.com/DnaCalc/DnaCalc.git $R/repo && cd $R/repo   # origin/master's tip
  git config user.name '<your name>'; git config user.email '<your email>'
  cmp deploy/host/dnacalc-site-receive ~/bin/dnacalc-site-receive \
    || install -m 755 deploy/host/dnacalc-site-receive ~/bin/dnacalc-site-receive
  cargo xtask site pack calc               # target/site/calc/<release>/, .tar and .pack.json
  cargo xtask site smoke calc              # the smoke suite in headless Chromium and Firefox
                                           # against the packed release, under the site's headers
  cargo xtask site publish calc --release target/site/calc/<release> --dry-run --live https://dnacalc.com
  cargo xtask site publish calc --release target/site/calc/<release> --local
  cargo xtask site verify calc --url https://dnacalc.com/ --release target/site/calc/<release>
  cargo xtask app smoke calc --url https://dnacalc.com/
  git add deploy/site/published.json && git commit -m "dc-ysl: publish calc <release>"
  git pull --no-rebase && git push
```

For DNA OneCalc, use `onecalc` and `https://onecalc.dnacalc.com/`.

`site publish --local` refuses a release packed from a tree with changes, one whose smoke has not
passed, an installed receiver that is not the clone's `deploy/host/dnacalc-site-receive`, and runs
only at a terminal. The receiver refuses a tar whose files are not its build.json's, a release
already on the host, and any entry tarfile's data filter refuses; it switches `current` and
`previous` atomically under a lock. Caddy needs no reload.

## 3. Rollback

On the host, in the latest run folder's clone (`cd ~/codex-runs/site-publish/<stamp>/repo`):

- **One step back:** `cargo xtask site status calc` shows `current` and `previous`; `cargo xtask
  site rollback calc --local --release <previous>` switches back (it passes the current release it
  read, so a concurrent switch is refused). Immediate, no reload; no release or asset is deleted.
- **Without the tool:** `cd /srv/dnacalc-site/calc && ln -sfn releases/<r> current.tmp && mv -T
  current.tmp current`.
- **The Caddy change itself:** `sudo cp -p /etc/caddy/Caddyfile.bak.<stamp> /etc/caddy/Caddyfile`,
  `sudo caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile`, `sudo systemctl reload
  caddy`. The old content under `/srv/dnacalc` is left in place, so the old blocks work again.

## 4. The Handbook (handbook.dnacalc.com)

The Handbook (repository DnaCalc/ExcelFunctionsHandbook) is a static folder, not one of the
receiver's sites. Its site block is `deploy/caddy/handbook.caddy`; its root is
`/srv/handbook-site/current`, a symlink to one folder under `/srv/handbook-site/releases/`.
The evaluation and contribution server (`tools/efh-api`) is not run on the host, so the site block
answers `/api/eval` and `/api/contributions` with 503 and the contribution form says so.

It is built on dna-koderbot itself, in a run folder, never in the `~/projects` clones other
sessions use. The host has Rust 1.94.1 with the wasm target, wasm-bindgen 0.2.117 and Node 24.

```text
ssh dna-koderbot
  R=~/codex-runs/handbook-publish/<handbook commit 7>; mkdir -p $R
  git clone https://github.com/DnaCalc/ExcelFunctionsHandbook.git $R/repo && cd $R/repo
  git checkout <handbook commit>
  git -C ~/projects/DnaCalc/DnaCalc archive <revision in tools/efh-engine/source.json> --output=$R/source.tar
  python3 tools/efh-engine/build.py --archive $R/source.tar --work $R/build
  M=tools/efh-site/Cargo.toml
  cargo test --locked --manifest-path $M
  cargo run --release --locked --manifest-path $M -- build      # then check, check-links, check-claims
  node tools/efh-engine/check-examples.mjs $R/examples.json     # "failures": []
```

`site/build/api/build.json` must name the commit with `"source_tree_state": "clean"`. To look at
the pages before switching, copy `site/build` to this machine and serve it locally in a browser.

Switch (release = `<UTC stamp>-<first 12 of the Handbook commit>`), on the host:

```text
d=/srv/handbook-site/releases/<release>; test ! -e $d && cp -r $R/repo/site/build $d && chmod -R a+rX $d
cd /srv/handbook-site && { [ -L current ] && ln -sfn "$(readlink current)" previous || true; } && ln -sfn releases/<release> current.tmp && mv -T current.tmp current
curl -sS https://handbook.dnacalc.com/api/build.json      # source_revision is the published commit
```

First time only: `sudo install -d -o koderbot -g koderbot -m 755 /srv/handbook-site/releases`, then
back up the Caddyfile and `/etc/caddy/dnacalc`, `sudo install -m 644 handbook.caddy
/etc/caddy/dnacalc/`, validate and reload as in section 1. Rollback: point `current` at `previous`
the same way; no reload.
