# Releases: one version, three streams (dc-ysl.11)

Draft for owner review, updated 2026-11-21. Recommend a common `0.MINOR.PATCH` version and source
commit for both web apps, both Windows installers and the Linux runner. Use that version for the
first published crates, then allow crate-level SemVer at their public API boundaries.

Keep dev web builds local by default. Cut a preview from one checked master commit and archive its
manifests and artefacts together. Promote the very same checked artefacts to stable once the owner
approves them. The release-notes generator is available now and the coordinator command remains a
follow-up. The owner signs and publishes releases.

## 1. What ships

| Product | What a user gets | Built by today |
|---|---|---|
| DNA Calc web | dnacalc.com | `cargo xtask site pack calc` (tools/xtask-harness/src/site.rs) |
| DNA Calc - Formula Explorer web | onecalc.dnacalc.com | `cargo xtask site pack onecalc` |
| DNA Calc - Formula Explorer desktop | Windows zip and per-user MSI candidate | `cargo xtask package onecalc` and `cargo xtask package installer` |
| DNA Calc desktop | Windows zip and per-user MSI candidate | The same commands, `DNA-Calc/` folder |
| Linux runner `dnacalc` | tarball and .deb | `deploy/linux/package.sh` (dc-icq4.2, commit f7d95fa3) |
| Engine crates | not published (`publish = false`, Cargo.toml:46) | dc-qm0h |
| Handbook | handbook.dnacalc.com | its own repository; versioned by its commit |

## 2. Where versions live today

Checked in the code on 2026-11-13:

| Place | What it holds |
|---|---|
| `Cargo.toml` `[workspace.package] version` | `0.1.0`. Every member uses `version.workspace = true`. |
| `--version` | `DNA Calc - Formula Explorer 0.1.0` (apps/dna-onecalc-desktop/src/main.rs:46), `DNA Calc 0.1.0` (apps/dna-calc-desktop/src/main.rs:67), `dnacalc 0.1.0` (apps/dnacalc/src/lib.rs:122). All three come from `CARGO_PKG_VERSION`. None prints the commit. |
| Worker `Ping` | `Pong { version }` from `CARGO_PKG_VERSION` (apps/dna-calc-worker/src/server.rs:811). |
| Windows version resource | `FILEVERSION 16,0,0,1` and `ProductVersion "16.0.0.1"` in both `apps/*-desktop/version.rc`. The value came in with the first XLL host (d433cacb). That commit does not say why. |
| Site `build.json` | `version` (the workspace version, site.rs:567), `release` = `<UTC>-<commit12>`, `commit`, tool versions, file hashes. Public at `/build.json`. |
| `deploy/site/published.json` | What is live: calc `20261109T105341Z-779e136e9c4f`, onecalc `20261109T052737Z-c27d1c615516`. |
| `package-manifest.json` | `version`, `commit`, file sizes and SHA-256 (tools/xtask-harness/src/package.rs:252). Zip names `DNA-OneCalc-<version>-win-x64.zip`. |
| Linux packages | Version from cargo metadata (package.sh:17), `BUILD` file with the commit, names `dnacalc-<version>-linux-x64.tar.gz` and `dnacalc_<version>_amd64.deb`. |
| Git tags | None. |

So today every product says `0.1.0` and only the site and the zip manifest record the commit. Master
is 578 commits past the live calc site and 618 past the live onecalc site (`git rev-list --count`).

## 3. The version scheme

- **One number for the product:** `0.MINOR.PATCH` while before 1.0. Web apps, both desktop apps,
  the installers and the runner carry the same number. The source is the workspace version; nothing
  else holds its own copy.
- **Master carries the next patch with `-dev`.** After 0.2.0 is cut, master moves to `0.2.1-dev`.
  A dev build then says what it is: `DNA Calc 0.2.1-dev+01e14c4d1234`.
- **A preview is a MINOR step** (`0.2.0`, `0.3.0`). A quick fix to a published preview is a PATCH
  (`0.2.1`). There are no release branches (AGENTS.md: master only), so a patch is cut from master
  like any release, carries everything on master since the preview, and passes all of section 5.
  The release commit sets whichever number applies.
- **Build metadata = the commit.** Every product prints and records `<version>+<commit12>`
  somewhere a user can find it: `--version`, the About box, `/build.json`, the MSI's
  `Comments` property, the runner's `BUILD` file.
- **Windows numeric forms.** MSI ProductVersion is `0.MINOR.PATCH` (MSI compares the first three
  fields only). MSIX, if ever used, needs four fields: `0.MINOR.PATCH.0`.
- **The `16.0.0.1` file version stays until measured.** Add-ins can read the host's version
  through the C API; `xlfGetWorkspace(2)` answers `"16.0"` (crates/calc-xll-host/src/callbacks.rs:777).
  Whether any add-in reads the exe's file resource is not measured. Proposal: keep `FILEVERSION
  16,0,0,1`, set `ProductVersion` to the product version, and run the m4 and m5 add-in sets with
  the changed resource before the first preview. If a set changes, `ProductVersion` stays 16.0.0.1
  too and the product version lives in the MSI and `--version` only.
- **The deb** maps `-dev` to `~dev` (`0.2.1~dev`), so a dev package sorts before its release.
- **Crates:** follow the product number until the owner publishes them (dc-qm0h). After the first
  publish each crate keeps its own SemVer, starting at the product number of that day.
- **The first preview is 0.2.0.** The 2026-11-09 sites are recorded as 0.1.0 dev builds; no tag is
  made for them after the fact.

## 4. Streams

| Stream | What | When | Where | Approval |
|---|---|---|---|---|
| dev | web apps only, labelled "dev build" with the commit | any master build used for review | local smoke and preview | owner approves any future public dev host |
| preview | both web apps, both desktop MSIs and zips, the Linux tarball and deb, release notes | per milestone, at most weekly, once checks and the clean-machine install test pass | dnacalc.com, onecalc.dnacalc.com and the downloads page | owner reviews the evidence, signs and publishes |
| stable | the same bytes and artifact hashes as the promoted preview | later, after at least a week without new P0 or P1 defects | the same | owner approves promotion |

Recommend serving approved downloads under versioned paths on the content site. Each filename
belongs to one release record with its source commit and final SHA-256. Retain versioned downloads
so the owner can roll back the current link and explain a withdrawn release.

Owner decisions remain on a public dev hostname, an attended or signed-tag publish route for
that host, the downloads origin, and whether to test the Windows version-resource change from
section 3 in the first preview. Keep the proposed dev workflow local until those decisions.

## 5. What a release must pass

Every number is from a run at the release commit, by the release's own builds. The notes print each
one beside the previous release's.

1. **Builds:** `cargo xtask check --all` on Linux. The Windows binaries are cross-built with
   cargo-xwin (docs/plan/WINDOWS_CROSS_BUILD.md), hash-checked on Windows, and their test executables
   run there. `cargo xtask sweep` on Windows in the night window (00:00-07:00) when it can run;
   otherwise the cross-built test set, and the notes say which.
2. **Corpus:** `cargo xtask corpus --oracle`, debug and `--profile perf` counts equal. Pass count not
   lower than the previous release unless each lost cell is listed with its bead. Today:
   77,036 of 78,264 formula cells (commit 6847cc02).
3. **Function bank:** `cargo xtask witness judge --bank full`, debug and perf equal; no row that
   matched in the previous release fails now unless listed. Today: 4,607,949 of 4,729,476 rows
   (commit 5ed48847).
4. **VBA:** `crates/calc-vba/tests/corpus/baseline.json` rows not lower. Today dna-simple 109 of 109 on
   module, JIT and .xlsm routes; dna-examples project 24 of 26, .xlsm 22 of 26. Its `windows` section
   is empty; the first preview needs the Windows rows (dc-tey4.1 ran them on 2026-11-13).
5. **Add-ins (Windows):** the Formula Explorer page acceptance m4 and m5 sets with the cross-built release
   binaries. Last recorded: m4 350 of 350, xloil-utils-sql 12 of 12, simple-xlam and simple-xlsm 109
   of 109 each (WINDOWS_CROSS_BUILD.md, second run).
6. **Web:** `cargo xtask site smoke` for both sites before publish, `site verify` and `app smoke --url`
   after (deploy/OWNER_STEPS.md section 2).
7. **Installers:** the clean-machine install, upgrade from the previous preview, and uninstall test
   (dc-fk66.3) on the MSIs the owner will sign.
8. **Linux:** the tarball and deb on a clean ubuntu:26.04 container, as dc-icq4.2 did it
   (`dnacalc --version`, `dnacalc diff` on dna-simple.xlsm = 109 of 109).
9. **Notices:** THIRD-PARTY-NOTICES.txt holds full licence texts for every linked crate (dc-ysl.2,
   open). Until dc-ysl.2 closes, the notes say what is missing.
10. **No open P0** in the tracker. Open P1 defects are listed in the notes.

A lower figure is allowed only when the notes list it with a cause and a bead. Nothing is weakened
to make a figure hold.

## 6. Release notes

The installer release-notes generator takes the installer manifest and a Markdown draft. The
draft's user changes and known gaps cite their tracker beads. Run `cargo xtask release notes
--manifest <installer-manifest.json> --draft <notes.md> --out <dir>` to build a local download
page and its release notes. The owner supplies the final digests and signer after signing.

A future coordinator can collect candidate changes from tracker entries closed since the previous
tag and commits carrying those bead IDs. The owner edits and approves the draft. Sections:

1. What you can do now (plain words, one line per user-visible change, bead id at the end).
2. The figures of section 5, this release and the previous.
3. Known problems: open P0 and P1 defects, and what is refused (typed not-modelled refusals).
4. Downloads: file names, sizes, SHA-256, and the signer.
5. Upgrade notes (settings paths, file format changes).

The notes are committed as `docs/releases/<version>.md` and shown on the site's downloads page.

## 7. How a release goes out

Agents (dna-firehorse for Windows and Linux binaries, dna-koderbot for the web packs):
1. Pull master at the chosen commit. Set the workspace version to `0.N.0` in one commit
   (`<bead>: release 0.N.0`), run every check of section 5, and write
   `target/release-candidates/0.N.0/` with the unsigned files, `SHA256SUMS`, the figures and the
   draft notes.
2. Push; tell the owner the commit, the folder and the hashes.

The owner:
3. Reads the notes and says yes or no.
4. Signs the Windows executables and MSIs (WINDOWS_INSTALLERS.md section 6) and checks the signatures.
5. Makes the signed tag: `git tag -s v0.N.0 <commit> -m "DNA Calc 0.N.0"` and `git push origin v0.N.0`.
6. Publishes the web apps (`site publish --local`, OWNER_STEPS.md section 2) and copies the signed
   downloads to their place.

Agents again:
7. Bump master to `0.N.1-dev`, commit the notes and `published.json`, and verify the live sites.

## 8. Rollback

- **Web:** `cargo xtask site rollback <site> --local --release <previous>` (OWNER_STEPS.md section 3).
  Immediate; nothing is deleted.
- **Downloads:** every version stays in its own folder. Rollback is pointing the downloads page back at
  the previous version and marking the bad one withdrawn in its notes. The MSI of the previous version
  installs over a newer one only after the newer one is uninstalled (MSI refuses a downgrade by design;
  see WINDOWS_INSTALLERS.md).
- **Runner:** the previous tarball and deb stay published.
- **A bad tag** is never moved or deleted; the fix is the next PATCH.

## 9. What changes in the code (follow-up bead)

1. `cargo xtask release prepare <version>`: set the workspace version, check every place of section 2
   agrees, write the candidate folder and pass the installer manifest to the existing notes
   generator. Add a tag-range summary for the rest of the notes.
2. Commit in `--version`: the packaging commands set `DNACALC_BUILD_COMMIT`; `build.rs` reads it, so
   builds stay reproducible; without it the text is `+unknown`.
3. `build.json` and `package-manifest.json` gain `"stream": "dev" | "preview" | "stable"`.
4. `version.rc` is written from the workspace version by `build.rs` (ProductVersion), with the
   16.0.0.1 file version kept as section 3 says.
5. `deploy/linux/package.sh`: the `~dev` mapping for the deb.
6. `deploy/windows/build-msi.sh` (dc-fk66.1) drops the `-dev` part for the MSI version, which
   must be numeric; preview MSIs are made only from release commits.
