# Publishing DNA OneCalc and DNA Calc: the owner's steps (dc-ysl.5)

The plan is `docs/plan/PUBLISHING_PLAN.md`; the tools are `cargo xtask site` (`tools/xtask/src/site.rs`),
`deploy/host/dnacalc-site-receive` and `deploy/caddy/`. Every publish is run or approved by the owner.

| Site | Host | Root on the host |
|---|---|---|
| `calc` (DNA Calc) | dnacalc.com (www.dnacalc.com redirects) | `/srv/dnacalc-site/calc/current` |
| `onecalc` (DNA OneCalc) | dnaonecalc.dnacalc.com now; onecalc.dnacalc.com once its DNS record exists | `/srv/dnacalc-site/onecalc/current` |

**Route:** build on this machine, upload the packed tar over ssh, switch on the host. The host's
Rust (1.95.0) is not the workspace's (1.94.1), it has no wasm-bindgen 0.2.117 CLI and no browsers
for the smoke, so building there would add toolchains to the host and lose the two-browser smoke
before the switch. The tar's `build.json` names the commit either way.

## 0. Before the first public publish

The licence review (`docs/plan/THIRD_PARTY_REVIEW.md`, section 3) lists code in the wasm that a
notice cannot cover (rows 1 to 7). The owner rules on each before the first public binary.

## 1. One-time setup on the host

Run from a clone of this repository at the commit being published, as the ssh user `koderbot`
(`ssh dna-koderbot`), which has sudo. Nothing here touches users, sudoers, ssh, keys or the
firewall.

```text
# The release root, writable by the ssh user, readable by Caddy.
ssh dna-koderbot 'sudo install -d -o koderbot -g koderbot -m 755 /srv/dnacalc-site'

# The receiver.
scp deploy/host/dnacalc-site-receive dna-koderbot:bin/dnacalc-site-receive
ssh dna-koderbot 'chmod 755 ~/bin/dnacalc-site-receive'
```

Then publish both sites once (section 2, up to `site publish`), so each `current` exists, and only
then point Caddy at them:

```text
scp deploy/caddy/headers.caddy deploy/caddy/sites.caddy dna-koderbot:tmp/
ssh dna-koderbot
  stamp=$(date -u +%Y%m%dT%H%M%SZ)
  sudo cp -p /etc/caddy/Caddyfile /etc/caddy/Caddyfile.bak.$stamp
  sudo install -d -m 755 /etc/caddy/dnacalc
  sudo install -m 644 ~/tmp/headers.caddy ~/tmp/sites.caddy /etc/caddy/dnacalc/
  # In /etc/caddy/Caddyfile, replace the dnacalc.com, www.dnacalc.com and dnaonecalc.dnacalc.com
  # blocks with the one line:   import /etc/caddy/dnacalc/*.caddy
  # (koderbot.dnakode.com and dnatreecalc.dnacalc.com stay as they are.)
  sudo caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile   # must say Valid
  sudo systemctl reload caddy
```

When onecalc.dnacalc.com has an A record (92.222.230.210), add `onecalc.dnacalc.com` to the
OneCalc block's address in `/etc/caddy/dnacalc/sites.caddy` (and the repository's copy), validate
and reload.

## 2. Each publish (one site at a time)

```text
git pull --no-rebase                       # a clean tree, HEAD pushed to origin/master
cargo xtask site pack calc                 # target/site/calc/<release>/, .tar and .pack.json
cargo xtask site smoke calc                # the smoke suite in Chrome and Firefox against the
                                           # packed release, under the site's own headers
cargo xtask site publish calc --release target/site/calc/<release> --dry-run --live https://dnacalc.com
cargo xtask site publish calc --release target/site/calc/<release> --to dna-koderbot
cargo xtask site verify calc --url https://dnacalc.com/ --release target/site/calc/<release>
cargo xtask app smoke calc --url https://dnacalc.com/
git add deploy/site/published.json && git commit -m "dc-ysl: publish calc <release>" && git push
```

For DNA OneCalc, the same with `onecalc` and `https://dnaonecalc.dnacalc.com/`.

`site publish` refuses a release packed from a tree with changes, one whose smoke has not passed,
and runs only at a terminal. The receiver refuses a tar whose files are not its build.json's, a
release already on the host, and any entry tarfile's data filter refuses. Caddy needs no reload.

## 3. Rollback

- **One step back:** `cargo xtask site status calc --to dna-koderbot` shows `current` and
  `previous`; `cargo xtask site rollback calc --to dna-koderbot --release <previous>` switches back
  (it passes the current release it read, so a concurrent switch is refused). Immediate, no
  reload; no release or asset is deleted.
- **Without the tool:** `ssh dna-koderbot 'cd /srv/dnacalc-site/calc && ln -sfn releases/<r>
  current.tmp && mv -T current.tmp current'`.
- **The Caddy change itself:** `sudo cp -p /etc/caddy/Caddyfile.bak.<stamp> /etc/caddy/Caddyfile`,
  `sudo caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile`, `sudo systemctl reload
  caddy`. The old content under `/srv/dnacalc` is left in place, so the old blocks work again.
