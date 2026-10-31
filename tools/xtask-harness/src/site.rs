//! `cargo xtask site ...`: the published form of DNA OneCalc (`onecalc`, for onecalc.dnacalc.com)
//! and DNA Calc (`calc`, for the apex dnacalc.com), and the owner's tools to publish it
//! (dc-ysl; docs/plan/PUBLISHING_PLAN.md). No command here is run against the host by an agent:
//! agents pack, serve, smoke, verify locally and dry-run; the owner publishes.
//!
//! - `site pack <site> [--allow-dirty]` builds the app's release bundle (`app::bundle`) into
//!   `target/site/<site>/<release>/`, `<release>` being `<UTC yyyymmddThhmmssZ>-<commit12>`:
//!   `index.html` pointing at `assets/<id>/boot.js`; `assets/<id>/` holding `boot.js`, the
//!   wasm-bindgen glue and the wasm, `<id>` the first 16 hex digits of a sha256 over the three; the
//!   public `build.json` (format, site, release, commit, version, built UTC, rustc, wasm-bindgen,
//!   the asset folder, and every other file's size and sha256); the workspace `LICENSE`; and
//!   `THIRD-PARTY-NOTICES.txt` (the ported-code notices of `deploy/notices/ported.txt` and the
//!   licence texts of every crate compiled into the wasm, from cargo metadata). Beside it, the
//!   uncompressed tar the host receives (`<release>.tar`: sorted entries, fixed owner, modes and
//!   the commit's time, so two packs of one commit differ only where their bytes do) and the
//!   private `<release>.pack.json` (the tar's size and sha256, build.json's sha256, the tool
//!   versions, whether the tree was dirty, and the smoke result once `site smoke` has run). It
//!   refuses a tree with changes, or a HEAD that origin/master (after a fetch) does not contain,
//!   unless `--allow-dirty`, which marks the release `-dirty`; a dirty release cannot be published.
//! - `site serve <site> [--release <dir>] [--port <n>]` serves a release (the latest packed when
//!   none is named) on 127.0.0.1 with the headers of `deploy/caddy/headers.caddy`, the snippet the
//!   host's Caddy imports, until stopped.
//! - `site smoke <site> [--release <dir>] [--browser chromium|firefox]` serves the release the
//!   same way, verifies it (below), runs the app's smoke suite against it in headless Chrome and
//!   Firefox (`app::smoke` by URL; the page's fault list includes every Content-Security-Policy
//!   violation), and records the result in the release's pack.json.
//! - `site verify <site> --url <base> [--release <dir>]` fetches the served `/build.json` and every
//!   file it lists (with curl, so a live https site is checked the same way), and checks each
//!   file's sha256, the content type of the page, scripts and wasm, and every header the snippet
//!   sets for its path; with `--release`, the served build.json must be the release's own.
//! - `site publish <site> --release <dir> --dry-run [--live <url|build.json>]` shows what a
//!   publish would send (the tar, its size and sha256, the command) and how the release differs
//!   from the live build.json. Without `--dry-run` it needs `--to <ssh target>`, a terminal on
//!   standard input and output, a clean release whose smoke passed, and then pipes the tar to
//!   `ssh <target> bin/dnacalc-site-receive publish <site>` (`deploy/host/dnacalc-site-receive`),
//!   recording the release in `deploy/site/published.json`.
//! - `site status <site> --to <ssh target>` and `site rollback <site> --to <ssh target> --release
//!   <release>` ask the receiver; rollback passes the current release it read (`--if-current`) and
//!   needs a terminal too.
//!
//! Exit code: 0 on success, 1 on a failure or refusal, 2 for a usage error.

use crate::accept::utc_now;
use crate::app::{self, APPS, App, SmokeTarget};
use crate::cargo::{self, Workspace};
use calc_verdict::sha256_hex;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::Arc;

pub const USAGE: &str = "usage: cargo xtask site pack onecalc|calc [--allow-dirty]\n       cargo xtask site serve onecalc|calc [--release <dir>] [--port <n>]\n       cargo xtask site smoke onecalc|calc [--release <dir>] [--browser chromium|firefox]\n       cargo xtask site verify onecalc|calc --url <base> [--release <dir>]\n       cargo xtask site publish onecalc|calc --release <dir> (--dry-run [--live <url|build.json>] | --to <ssh target>)\n       cargo xtask site status onecalc|calc --to <ssh target>\n       cargo xtask site rollback onecalc|calc --to <ssh target> --release <release>";

/// The site snippet every host imports and the local serve reads.
pub const HEADERS_FILE: &str = "deploy/caddy/headers.caddy";
/// The notices of code ported from third parties (docs/plan/THIRD_PARTY_REVIEW.md).
pub const PORTED_NOTICES: &str = "deploy/notices/ported.txt";
/// The record of what the owner published, per site.
pub const PUBLISHED_FILE: &str = "deploy/site/published.json";

pub const BUILD_FORMAT: &str = "dnacalc-site-build/1";
pub const PACK_FORMAT: &str = "dnacalc-site-pack/1";

/// The receiver on the host, relative to the ssh user's home (`deploy/host/dnacalc-site-receive`,
/// installed there by the owner's setup).
const RECEIVER: &str = "bin/dnacalc-site-receive";

/// The MIT licence's terms, after its copyright line.
const MIT_TERMS: &str =
    "Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the \"Software\"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.";

/// The wasm-bindgen release the bundle is built with (app.rs checks the CLI against it).
const WASM_BINDGEN: &str = "0.2.117";

/// One site: its name, its app and its host.
struct Site {
    key: &'static str,
    app: &'static App,
    host: &'static str,
}

fn site(key: &str) -> Option<Site> {
    match key {
        "onecalc" => Some(Site {
            key: "onecalc",
            app: &APPS[0],
            host: "onecalc.dnacalc.com",
        }),
        "calc" => Some(Site {
            key: "calc",
            app: &APPS[1],
            host: "dnacalc.com",
        }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// The headers.

/// The headers of `deploy/caddy/headers.caddy`, as the local serve sends them: the `header` block's
/// for every path, then each `header @matcher` line's for the paths its matcher names.
#[derive(Debug, Default, PartialEq)]
pub struct SiteHeaders {
    all: Vec<(String, String)>,
    rules: Vec<(Vec<String>, String, String)>,
}

/// A header line's name and its quoted value.
fn header_line(line: &str) -> Result<(String, String), String> {
    let (name, value) = line
        .split_once(char::is_whitespace)
        .ok_or_else(|| format!("a header line without a value: {line:?}"))?;
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .ok_or_else(|| format!("a header value that is not quoted: {line:?}"))?;
    Ok((name.to_owned(), value.to_owned()))
}

impl SiteHeaders {
    /// Reads the snippet's text. A line it does not understand inside a `header` block, or a
    /// `header` line of another form, is refused, so the local serve never sends less than the
    /// host would.
    pub fn parse(text: &str) -> Result<SiteHeaders, String> {
        let mut headers = SiteHeaders::default();
        let mut matchers: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut in_block = false;
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if in_block {
                if line == "}" {
                    in_block = false;
                } else if line.starts_with('-') {
                    // A header the host removes (`-Server`); the local serve sends none.
                } else {
                    headers.all.push(header_line(line)?);
                }
                continue;
            }
            if line == "header {" {
                in_block = true;
            } else if let Some(rest) = line.strip_prefix('@') {
                let mut words = rest.split_whitespace();
                let name = words.next().unwrap_or_default().to_owned();
                if words.next() != Some("path") {
                    return Err(format!("a matcher other than path: {line:?}"));
                }
                matchers.insert(name, words.map(str::to_owned).collect());
            } else if let Some(rest) = line.strip_prefix("header @") {
                let (name, header) = rest
                    .split_once(char::is_whitespace)
                    .ok_or_else(|| format!("a matched header line without a header: {line:?}"))?;
                let paths = matchers
                    .get(name)
                    .ok_or_else(|| {
                        format!("a header line naming no matcher defined above: {line:?}")
                    })?
                    .clone();
                let (header, value) = header_line(header.trim())?;
                headers.rules.push((paths, header, value));
            } else if line.starts_with("header") {
                return Err(format!(
                    "a header line of a form the local serve does not read: {line:?}"
                ));
            }
        }
        if in_block {
            return Err("the header block is not closed".to_owned());
        }
        if headers.all.is_empty() {
            return Err("the snippet sets no headers".to_owned());
        }
        Ok(headers)
    }

    pub fn load(ws: &Workspace) -> Result<SiteHeaders, String> {
        let path = ws.root.join(HEADERS_FILE);
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        SiteHeaders::parse(&text).map_err(|e| format!("{HEADERS_FILE}: {e}"))
    }

    /// The headers a response for `path` carries.
    pub fn for_path(&self, path: &str) -> Vec<(String, String)> {
        let mut out = self.all.clone();
        for (paths, header, value) in &self.rules {
            let matched = paths.iter().any(|pattern| match pattern.strip_suffix('*') {
                Some(prefix) => path.starts_with(prefix),
                None => path == pattern,
            });
            if matched {
                out.push((header.clone(), value.clone()));
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// The command line.

pub fn run(raw: &[String]) -> ExitCode {
    let ws = match std::env::current_dir()
        .map_err(|e| format!("no current directory: {e}"))
        .and_then(|cwd| Workspace::load(&cwd))
    {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("xtask site: cannot read the workspace: {e}");
            return ExitCode::from(1);
        }
    };
    let Some((command, key, rest)) = (match raw {
        [command, key, rest @ ..] => site(key).map(|site| (command.as_str(), site, rest)),
        _ => None,
    }) else {
        return usage(raw);
    };
    // The options: flags without a value, the rest with one.
    let mut flags = BTreeSet::new();
    let mut options = BTreeMap::new();
    let mut args = rest.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--allow-dirty" | "--dry-run" => {
                flags.insert(arg.clone());
            }
            "--release" | "--port" | "--browser" | "--url" | "--live" | "--to" => {
                match args.next() {
                    Some(value) => {
                        options.insert(arg.clone(), value.clone());
                    }
                    None => return usage(raw),
                }
            }
            _ => return usage(raw),
        }
    }
    let option = |name: &str| options.get(name).cloned();
    let release = |ws: &Workspace| match option("--release") {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => latest_release(ws, &key),
    };
    let result = match command {
        "pack" => pack(&ws, &key, flags.contains("--allow-dirty")).map(|_| ()),
        "serve" => {
            let port = match option("--port").map(|p| p.parse::<u16>()) {
                Some(Ok(port)) => port,
                Some(Err(_)) => return usage(raw),
                None => 8080,
            };
            release(&ws).and_then(|dir| serve_forever(&ws, &dir, port))
        }
        "smoke" => {
            return match release(&ws) {
                Ok(dir) => site_smoke(&ws, &key, &dir, option("--browser").as_deref()),
                Err(e) => {
                    println!("SITE RESULT failed: {e}");
                    ExitCode::from(1)
                }
            };
        }
        "verify" => match option("--url") {
            Some(url) => {
                let release = option("--release").map(PathBuf::from);
                verify(&ws, &url, release.as_deref()).and_then(|failures| {
                    if failures.is_empty() {
                        Ok(())
                    } else {
                        Err(failures.join("\n"))
                    }
                })
            }
            None => return usage(raw),
        },
        "publish" => {
            let dir = match option("--release") {
                Some(dir) => PathBuf::from(dir),
                None => return usage(raw),
            };
            if flags.contains("--dry-run") {
                dry_run(&ws, &key, &dir, option("--live").as_deref())
            } else {
                match option("--to") {
                    Some(to) => publish(&ws, &key, &dir, &to),
                    None => return usage(raw),
                }
            }
        }
        "status" => match option("--to") {
            Some(to) => remote(&to, &["status", key.key], false).map(|out| print!("{out}")),
            None => return usage(raw),
        },
        "rollback" => match (option("--to"), option("--release")) {
            (Some(to), Some(release)) => rollback(&key, &to, &release),
            _ => return usage(raw),
        },
        _ => return usage(raw),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            println!("SITE RESULT failed: {e}");
            ExitCode::from(1)
        }
    }
}

fn usage(raw: &[String]) -> ExitCode {
    eprintln!("xtask site: unknown arguments {raw:?}\n{USAGE}");
    ExitCode::from(2)
}

/// The folder a site's releases are packed into.
fn site_dir(ws: &Workspace, site: &Site) -> PathBuf {
    ws.root.join("target").join("site").join(site.key)
}

/// The latest release packed for a site: the greatest release name (they start with the time).
fn latest_release(ws: &Workspace, site: &Site) -> Result<PathBuf, String> {
    let dir = site_dir(ws, site);
    let mut releases: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| format!("no release is packed in {}: {e}", dir.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join("build.json").is_file())
        .collect();
    releases.sort();
    releases
        .pop()
        .ok_or_else(|| format!("no release is packed in {}", dir.display()))
}

// ---------------------------------------------------------------------------------------------
// Pack.

/// `2026-10-29T10:15:00Z` as `20261029T101500Z`.
fn compact_utc(utc: &str) -> String {
    utc.chars().filter(|c| *c != '-' && *c != ':').collect()
}

/// A record as pretty JSON with a final newline.
fn pretty(value: &Value) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// Every file under `dir`, as paths relative to it with `/`, sorted.
fn files_under(dir: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in
            fs::read_dir(&next).map_err(|e| format!("cannot list {}: {e}", next.display()))?
        {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path
                    .strip_prefix(dir)
                    .map_err(|e| e.to_string())?
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push(relative);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Packs `site` and returns its release folder.
fn pack(ws: &Workspace, site: &Site, allow_dirty: bool) -> Result<PathBuf, String> {
    let status = cargo::git(&ws.root, &["status", "--porcelain"])?;
    let dirty = !status.trim().is_empty();
    if !allow_dirty {
        if dirty {
            return Err("the tree has changes; commit them, or pack with --allow-dirty for a release that cannot be published".to_owned());
        }
        cargo::git(&ws.root, &["fetch", "-q", "origin"])?;
        cargo::git(
            &ws.root,
            &["merge-base", "--is-ancestor", "HEAD", "origin/master"],
        )
        .map_err(|_| "HEAD is not on origin/master; push it first".to_owned())?;
    }
    let commit = cargo::git(&ws.root, &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let commit_time: u64 = cargo::git(&ws.root, &["show", "-s", "--format=%ct", "HEAD"])?
        .trim()
        .parse()
        .map_err(|e| format!("the commit's time: {e}"))?;
    let built = utc_now();
    let release = format!(
        "{}-{}{}",
        compact_utc(&built),
        &commit[..12],
        if dirty { "-dirty" } else { "" }
    );
    let base = site_dir(ws, site);
    let staging = base.join(".staging");
    let _ = fs::remove_dir_all(&staging);
    app::bundle(ws, site.app, &staging)?;

    // The assets and their folder.
    // Every file of the bundle but the page: the stylesheet, the start module, the glue, the wasm.
    let assets: Vec<String> = files_under(&staging)?
        .into_iter()
        .filter(|name| name != "index.html")
        .collect();
    let mut listing = String::new();
    for name in &assets {
        let bytes = read(&staging.join(name))?;
        listing.push_str(&format!("{name}\0{}\n", sha256_hex(&bytes)));
    }
    let id = sha256_hex(listing.as_bytes())[..16].to_owned();
    let dir = base.join(&release);
    let _ = fs::remove_dir_all(&dir);
    let asset_dir = dir.join("assets").join(&id);
    fs::create_dir_all(&asset_dir)
        .map_err(|e| format!("cannot create {}: {e}", asset_dir.display()))?;
    for name in &assets {
        write(&asset_dir.join(name), &read(&staging.join(name))?)?;
    }
    let page = String::from_utf8(read(&staging.join("index.html"))?)
        .map_err(|_| "the page is not UTF-8".to_owned())?;
    let mut page = page;
    for (attribute, name) in [("src", "boot.js"), ("href", "app.css")] {
        let from = format!("{attribute}=\"{name}\"");
        if page.matches(&from).count() != 1 {
            return Err(format!("the page does not load {name} once ({from})"));
        }
        page = page.replace(&from, &format!("{attribute}=\"assets/{id}/{name}\""));
    }
    write(&dir.join("index.html"), page.as_bytes())?;
    write(&dir.join("LICENSE"), &read(&ws.root.join("LICENSE"))?)?;
    let notices = notices(ws, site.app)?;
    write(&dir.join("THIRD-PARTY-NOTICES.txt"), notices.as_bytes())?;
    let _ = fs::remove_dir_all(&staging);

    // build.json: every other file's size and hash.
    let mut files = Map::new();
    for name in files_under(&dir)? {
        let bytes = read(&dir.join(&name))?;
        files.insert(
            name,
            json!({ "bytes": bytes.len(), "sha256": sha256_hex(&bytes) }),
        );
    }
    let (_, rustc) = cargo::rustc_version()?;
    let rustc = rustc
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_owned();
    let build = json!({
        "format": BUILD_FORMAT,
        "site": site.key,
        "release": release,
        "commit": commit,
        "version": workspace_version(ws)?,
        "built_utc": built,
        "rustc": rustc,
        "wasm_bindgen": WASM_BINDGEN,
        "assets": id,
        "files": files,
    });
    let build_bytes = pretty(&build)?;
    write(&dir.join("build.json"), &build_bytes)?;

    // The tar the host receives.
    let mut entries: Vec<(String, Option<Vec<u8>>)> = Vec::new();
    let mut folders = BTreeSet::new();
    for name in files_under(&dir)? {
        let mut parent = String::new();
        for segment in name
            .split('/')
            .rev()
            .skip(1)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            parent.push_str(segment);
            parent.push('/');
            folders.insert(parent.clone());
        }
        entries.push((format!("{release}/{name}"), Some(read(&dir.join(&name))?)));
    }
    entries.push((format!("{release}/"), None));
    for folder in folders {
        entries.push((format!("{release}/{folder}"), None));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let tar = tar(&entries, commit_time)?;
    let tar_path = base.join(format!("{release}.tar"));
    write(&tar_path, &tar)?;
    let pack = json!({
        "format": PACK_FORMAT,
        "site": site.key,
        "host": site.host,
        "release": release,
        "commit": commit,
        "dirty": dirty,
        "built_utc": built,
        "tar": { "file": format!("{release}.tar"), "bytes": tar.len(), "sha256": sha256_hex(&tar) },
        "build_json_sha256": sha256_hex(&build_bytes),
        "tools": { "rustc": rustc, "wasm_bindgen": WASM_BINDGEN },
        "smoke": Value::Null,
    });
    write(&base.join(format!("{release}.pack.json")), &pretty(&pack)?)?;
    for (name, value) in build["files"].as_object().into_iter().flatten() {
        println!("SITE file {name}: {} bytes", value["bytes"]);
    }
    println!(
        "SITE packed {} release {release}: {} ({} bytes, sha256 {}){}",
        site.key,
        tar_path.display(),
        tar.len(),
        sha256_hex(&tar),
        if dirty {
            "; DIRTY, cannot be published"
        } else {
            ""
        }
    );
    Ok(dir)
}

/// The workspace's version (`[workspace.package] version`).
fn workspace_version(ws: &Workspace) -> Result<String, String> {
    let manifest = fs::read_to_string(ws.root.join("Cargo.toml")).map_err(|e| e.to_string())?;
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[workspace.package]";
        } else if in_package
            && let Some(value) = line.strip_prefix("version")
            && let Some(value) = value.trim().strip_prefix('=')
        {
            return Ok(value.trim().trim_matches('"').to_owned());
        }
    }
    Err("Cargo.toml has no [workspace.package] version".to_owned())
}

/// An uncompressed ustar archive of `entries` (a folder's name ends with `/` and has no bytes),
/// every entry owned by 0:0 with mode 0644 (0755 for a folder) and the time `mtime`.
pub fn tar(entries: &[(String, Option<Vec<u8>>)], mtime: u64) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for (name, bytes) in entries {
        let mut header = [0u8; 512];
        let (prefix, short) = if name.len() <= 100 {
            ("", name.as_str())
        } else {
            let split = name[..name.len().min(156)]
                .rfind('/')
                .filter(|at| name.len() - at - 1 <= 100 && *at <= 155)
                .ok_or_else(|| format!("a path too long for a tar header: {name}"))?;
            (&name[..split], &name[split + 1..])
        };
        let field = |header: &mut [u8; 512], at: usize, text: &str| {
            header[at..at + text.len()].copy_from_slice(text.as_bytes());
        };
        field(&mut header, 0, short);
        let size = bytes.as_ref().map_or(0, Vec::len);
        field(
            &mut header,
            100,
            if bytes.is_some() {
                "0000644"
            } else {
                "0000755"
            },
        );
        field(&mut header, 108, "0000000");
        field(&mut header, 116, "0000000");
        field(&mut header, 124, &format!("{size:011o}"));
        field(&mut header, 136, &format!("{mtime:011o}"));
        field(&mut header, 148, "        ");
        header[156] = if bytes.is_some() { b'0' } else { b'5' };
        field(&mut header, 257, "ustar");
        field(&mut header, 263, "00");
        field(&mut header, 345, prefix);
        let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
        field(&mut header, 148, &format!("{sum:06o}"));
        header[154] = 0;
        header[155] = b' ';
        out.extend_from_slice(&header);
        if let Some(bytes) = bytes {
            out.extend_from_slice(bytes);
            out.resize(out.len().div_ceil(512) * 512, 0);
        }
    }
    out.resize(out.len() + 1024, 0);
    Ok(out)
}

/// THIRD-PARTY-NOTICES.txt for `app`: the ported-code notices, then the licence texts of every
/// crate compiled into its wasm (cargo metadata for wasm32: the app's normal dependencies, not
/// build scripts or procedural macros, which run at build time), each text once with the crates
/// that carry it.
fn notices(ws: &Workspace, app: &App) -> Result<String, String> {
    let mut command = Command::new(cargo::cargo_program());
    command
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--filter-platform",
            cargo::WASM_TARGET,
        ])
        .current_dir(&ws.root);
    let out = cargo::output(&mut command)?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            cargo::tail(&out.stderr, 20)
        ));
    }
    let metadata: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("cargo metadata printed no JSON: {e}"))?;
    let packages: BTreeMap<String, &Value> = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p["id"].as_str()?.to_owned(), p)))
        .collect();
    let nodes: BTreeMap<String, &Value> = metadata["resolve"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| Some((n["id"].as_str()?.to_owned(), n)))
        .collect();
    let roots = std::iter::once(app.package)
        .chain(app.worker.map(|(package, _)| package))
        .map(|name| {
            packages
                .iter()
                .find(|(_, p)| p["name"] == name && p["source"].is_null())
                .map(|(id, _)| id.clone())
                .ok_or_else(|| format!("cargo metadata has no package {name}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let is_macro = |id: &str| {
        packages.get(id).is_some_and(|p| {
            p["targets"].as_array().into_iter().flatten().any(|t| {
                t["kind"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|k| k == "proc-macro")
            })
        })
    };
    let mut seen = roots.iter().cloned().collect::<BTreeSet<_>>();
    let mut pending = roots;
    while let Some(id) = pending.pop() {
        for dep in nodes
            .get(&id)
            .and_then(|n| n["deps"].as_array())
            .into_iter()
            .flatten()
        {
            let normal = dep["dep_kinds"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|k| k["kind"].is_null());
            let Some(to) = dep["pkg"].as_str() else {
                continue;
            };
            if normal && !is_macro(to) && seen.insert(to.to_owned()) {
                pending.push(to.to_owned());
            }
        }
    }
    // Each licence text, by hash, with the crates that carry it.
    let mut texts: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    let mut crates = Vec::new();
    for id in &seen {
        let Some(package) = packages.get(id) else {
            continue;
        };
        if package["source"].is_null() {
            continue; // the workspace's own crates: LICENSE
        }
        let name = format!(
            "{} {} ({})",
            package["name"].as_str().unwrap_or_default(),
            package["version"].as_str().unwrap_or_default(),
            package["license"]
                .as_str()
                .unwrap_or("no licence expression")
        );
        let folder = Path::new(package["manifest_path"].as_str().unwrap_or_default())
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let mut found = 0;
        let mut entries: Vec<PathBuf> = fs::read_dir(&folder)
            .map_err(|e| format!("cannot list {}: {e}", folder.display()))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        entries.sort();
        for path in entries {
            let file = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_uppercase();
            if ["LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE"]
                .iter()
                .any(|prefix| file.starts_with(prefix))
            {
                let text = String::from_utf8_lossy(&read(&path)?).trim().to_owned();
                let hash = sha256_hex(text.as_bytes());
                texts
                    .entry(hash)
                    .or_insert_with(|| (text, BTreeSet::new()))
                    .1
                    .insert(name.clone());
                found += 1;
            }
        }
        if found == 0 {
            // A crate that ships no licence file: under MIT (alone or as a choice), the MIT text
            // with the authors its manifest names as the copyright holders.
            let expression = package["license"].as_str().unwrap_or_default();
            if !expression
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                .any(|id| id == "MIT")
            {
                return Err(format!(
                    "{name} carries no licence file in {} and is not under MIT; its notice must be added by hand",
                    folder.display()
                ));
            }
            let authors: Vec<String> = package["authors"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|author| {
                    author
                        .split('<')
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .to_owned()
                })
                .collect();
            let holders = if authors.is_empty() {
                format!(
                    "the authors of {}",
                    package["name"].as_str().unwrap_or_default()
                )
            } else {
                authors.join(", ")
            };
            let text = format!(
                "MIT License (the crate ships no licence file; its manifest names MIT and these\nauthors)\n\nCopyright (c) {holders}\n\n{MIT_TERMS}"
            );
            texts
                .entry(sha256_hex(text.as_bytes()))
                .or_insert_with(|| (text, BTreeSet::new()))
                .1
                .insert(name.clone());
        }
        crates.push(name);
    }
    let ported = String::from_utf8(read(&ws.root.join(PORTED_NOTICES))?)
        .map_err(|_| format!("{PORTED_NOTICES} is not UTF-8"))?;
    let rule = "=".repeat(78);
    let mut out = format!(
        "Third-party notices for {} (dnacalc.com).\n\nDNA Calc's own code is under the MIT licence in LICENSE. The release also carries\ncode ported from third parties into DNA Calc's crates, and the crates below, compiled\ninto its WebAssembly modules. Their notices follow.\n\n{rule}\nPart 1. Code ported into DNA Calc's crates\n{rule}\n\n{}\n\n{rule}\nPart 2. Crates compiled into the modules ({} crates)\n{rule}\n\n",
        app.package,
        ported.trim_end(),
        crates.len()
    );
    for name in &crates {
        out.push_str(&format!("- {name}\n"));
    }
    for (text, names) in texts.values() {
        out.push_str(&format!(
            "\n{rule}\nUsed by: {}\n{rule}\n\n{text}\n",
            names.iter().cloned().collect::<Vec<_>>().join("; ")
        ));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Serve and smoke.

fn serve_forever(ws: &Workspace, dir: &Path, port: u16) -> Result<(), String> {
    let headers = Arc::new(SiteHeaders::load(ws)?);
    let port = app::serve_on(dir.to_path_buf(), Some(headers), port)?;
    println!(
        "SITE serving {} at http://127.0.0.1:{port}/ with the headers of {HEADERS_FILE}; stop with Ctrl+C",
        dir.display()
    );
    loop {
        std::thread::park();
    }
}

fn site_smoke(ws: &Workspace, site: &Site, dir: &Path, browser: Option<&str>) -> ExitCode {
    let headers = match SiteHeaders::load(ws) {
        Ok(headers) => Arc::new(headers),
        Err(e) => {
            println!("SITE RESULT failed: {e}");
            return ExitCode::from(1);
        }
    };
    let port = match app::serve(dir.to_path_buf(), Some(headers)) {
        Ok(port) => port,
        Err(e) => {
            println!("SITE RESULT failed: {e}");
            return ExitCode::from(1);
        }
    };
    let base = format!("http://127.0.0.1:{port}/");
    println!("SITE serving {} at {base}", dir.display());
    let verified = match verify(ws, &base, Some(dir)) {
        Ok(failures) => failures,
        Err(e) => vec![e],
    };
    for failure in &verified {
        println!("SITE verify failed: {failure}");
    }
    let smoked = app::smoke(ws, site.app, SmokeTarget::Url(base), browser, None);
    let passed = verified.is_empty() && smoked == ExitCode::SUCCESS;
    // The result goes into the release's pack.json.
    let pack_path = dir.with_extension("pack.json");
    if let Ok(bytes) = fs::read(&pack_path)
        && let Ok(mut pack) = serde_json::from_slice::<Value>(&bytes)
    {
        pack["smoke"] = json!({
            "passed": passed,
            "verified": verified.is_empty(),
            "browsers": browser.map_or_else(|| json!(["chromium", "firefox"]), |b| json!([b])),
            "utc": utc_now(),
        });
        if let Ok(text) = pretty(&pack) {
            let _ = fs::write(&pack_path, text);
        }
    }
    println!(
        "SITE RESULT {}: {} release {} served with the site headers; verify {}, smoke {}",
        if passed { "passed" } else { "failed" },
        site.key,
        dir.file_name().unwrap_or_default().to_string_lossy(),
        if verified.is_empty() {
            "passed"
        } else {
            "failed"
        },
        if smoked == ExitCode::SUCCESS {
            "passed"
        } else {
            "failed"
        }
    );
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

// ---------------------------------------------------------------------------------------------
// Verify.

/// One response: its status, its headers (names lower-cased) and its body.
struct Fetched {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Fetched {
    fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// GETs `url` with curl (no compression asked for, so the bytes are the file's).
fn fetch(url: &str) -> Result<Fetched, String> {
    let out = cargo::output(
        Command::new("curl")
            .args(["-sS", "-i", "--http1.1", "--max-time", "60", url])
            .stdin(Stdio::null()),
    )?;
    if !out.status.success() {
        return Err(format!("curl {url}: {}", cargo::text(&out.stderr).trim()));
    }
    let split = out
        .stdout
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| format!("{url}: an answer without a header"))?;
    let head = String::from_utf8_lossy(&out.stdout[..split]).into_owned();
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("{url}: no status line"))?;
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    Ok(Fetched {
        status,
        headers,
        body: out.stdout[split + 4..].to_vec(),
    })
}

/// Checks the site served at `base` against its own `/build.json` (and, with `release`, that this
/// is the release's): every file's bytes, the content types and the snippet's headers. The
/// failures, empty when it passed.
fn verify(ws: &Workspace, base: &str, release: Option<&Path>) -> Result<Vec<String>, String> {
    let headers = SiteHeaders::load(ws)?;
    let base = base.trim_end_matches('/');
    let mut failures = Vec::new();
    let build = fetch(&format!("{base}/build.json"))?;
    if build.status != 200 {
        return Err(format!("{base}/build.json answered {}", build.status));
    }
    if let Some(release) = release {
        let local = read(&release.join("build.json"))?;
        if local != build.body {
            failures.push("the served build.json is not the release's".to_owned());
        }
    }
    let record: Value = serde_json::from_slice(&build.body)
        .map_err(|e| format!("the served build.json is not JSON: {e}"))?;
    if record["format"] != BUILD_FORMAT {
        failures.push(format!("build.json's format is {}", record["format"]));
    }
    let mut checked = 0;
    let files = record["files"].as_object().cloned().unwrap_or_default();
    let mut paths: Vec<(String, String)> = files
        .keys()
        .map(|name| (format!("/{name}"), name.clone()))
        .collect();
    paths.push(("/".to_owned(), "index.html".to_owned()));
    paths.push(("/build.json".to_owned(), "build.json".to_owned()));
    for (path, name) in paths {
        let got = match fetch(&format!("{base}{path}")) {
            Ok(got) => got,
            Err(e) => {
                failures.push(e);
                continue;
            }
        };
        if got.status != 200 {
            failures.push(format!("{path} answered {}", got.status));
            continue;
        }
        if let Some(expected) = files.get(&name).and_then(|f| f["sha256"].as_str())
            && sha256_hex(&got.body) != expected
        {
            failures.push(format!("{path}: the served bytes are not build.json's"));
        }
        let want = app::content_type(&name);
        let served = got.header("content-type").unwrap_or_default();
        let essence = |t: &str| {
            t.split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
        };
        if essence(served) != essence(want) {
            failures.push(format!("{path}: content type {served:?}, not {want:?}"));
        }
        for (header, value) in headers.for_path(&path) {
            match got.header(&header) {
                Some(served) if served == value => {}
                Some(served) => {
                    failures.push(format!("{path}: {header} is {served:?}, not {value:?}"))
                }
                None => failures.push(format!("{path}: no {header} header")),
            }
        }
        checked += 1;
    }
    if failures.is_empty() {
        println!(
            "SITE verify passed: {base}: release {}, {checked} paths, bytes, types and headers as {HEADERS_FILE} sets them",
            record["release"].as_str().unwrap_or("?")
        );
    }
    Ok(failures)
}

// ---------------------------------------------------------------------------------------------
// Publish, status, rollback.

fn pack_record(dir: &Path) -> Result<Value, String> {
    let path = dir.with_extension("pack.json");
    serde_json::from_slice(&read(&path)?).map_err(|e| format!("{}: {e}", path.display()))
}

/// What a publish of `dir` would send, and how it differs from the live build.json.
fn dry_run(ws: &Workspace, site: &Site, dir: &Path, live: Option<&str>) -> Result<(), String> {
    let pack = pack_record(dir)?;
    let build: Value = serde_json::from_slice(&read(&dir.join("build.json"))?)
        .map_err(|e| format!("build.json: {e}"))?;
    let release = build["release"].as_str().unwrap_or_default();
    println!("SITE publish {} (dry run): release {release}", site.key);
    println!(
        "SITE would send {} ({} bytes, sha256 {}) with: ssh <target> bin/dnacalc-site-receive publish {} < {}",
        pack["tar"]["file"].as_str().unwrap_or_default(),
        pack["tar"]["bytes"],
        pack["tar"]["sha256"].as_str().unwrap_or_default(),
        site.key,
        pack["tar"]["file"].as_str().unwrap_or_default()
    );
    for refusal in refusals(&pack) {
        println!("SITE a publish would refuse: {refusal}");
    }
    let Some(live) = live else {
        println!("SITE no live build.json given (--live): every file is new to the host");
        return Ok(());
    };
    let live: Value = if live.starts_with("http://") || live.starts_with("https://") {
        let got = fetch(&format!("{}/build.json", live.trim_end_matches('/')))?;
        if got.status != 200 {
            println!(
                "SITE the live site serves no build.json (status {}): it is not a packed release, and every file is new",
                got.status
            );
            return Ok(());
        }
        serde_json::from_slice(&got.body).map_err(|e| format!("the live build.json: {e}"))?
    } else {
        serde_json::from_slice(&read(Path::new(live))?)
            .map_err(|e| format!("the live build.json: {e}"))?
    };
    println!(
        "SITE live release {} (commit {}), to be replaced by {release} (commit {})",
        live["release"].as_str().unwrap_or("?"),
        live["commit"].as_str().unwrap_or("?"),
        build["commit"].as_str().unwrap_or("?")
    );
    let empty = Map::new();
    let old = live["files"].as_object().unwrap_or(&empty);
    let new = build["files"].as_object().unwrap_or(&empty);
    for (name, value) in new {
        match old.get(name) {
            None => println!("SITE   new      {name} ({} bytes)", value["bytes"]),
            Some(was) if was["sha256"] != value["sha256"] => {
                println!(
                    "SITE   changed  {name} ({} to {} bytes)",
                    was["bytes"], value["bytes"]
                )
            }
            Some(_) => println!("SITE   same     {name}"),
        }
    }
    for name in old.keys().filter(|name| !new.contains_key(*name)) {
        println!("SITE   gone     {name} (its asset folder stays on the host)");
    }
    let _ = ws;
    Ok(())
}

/// Why a release cannot be published.
fn refusals(pack: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if pack["dirty"] != json!(false) {
        out.push("the release was packed from a tree with changes".to_owned());
    }
    if pack["smoke"]["passed"] != json!(true) {
        out.push("its smoke has not passed (cargo xtask site smoke)".to_owned());
    }
    out
}

/// Runs a receiver command on the host over ssh; standard input from `input` when given.
fn remote_with(to: &str, args: &[&str], input: Option<&Path>) -> Result<String, String> {
    let mut command = Command::new("ssh");
    command.arg(to).arg(RECEIVER).args(args);
    match input {
        Some(path) => {
            command.stdin(Stdio::from(
                fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?,
            ));
        }
        None => {
            command.stdin(Stdio::null());
        }
    }
    let out = cargo::output(&mut command)?;
    if !out.status.success() {
        return Err(format!(
            "ssh {to} {}: {}",
            args.join(" "),
            cargo::text(&out.stderr).trim()
        ));
    }
    Ok(cargo::text(&out.stdout))
}

fn remote(to: &str, args: &[&str], needs_terminal: bool) -> Result<String, String> {
    if needs_terminal {
        attended()?;
    }
    remote_with(to, args, None)
}

/// A publish and a rollback run only with a person at a terminal: never from an agent's or a
/// script's pipe.
fn attended() -> Result<(), String> {
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        Ok(())
    } else {
        Err(
            "this command runs only at a terminal (the owner's publish); none is attached"
                .to_owned(),
        )
    }
}

fn publish(ws: &Workspace, site: &Site, dir: &Path, to: &str) -> Result<(), String> {
    attended()?;
    let pack = pack_record(dir)?;
    let refusals = refusals(&pack);
    if !refusals.is_empty() {
        return Err(refusals.join("; "));
    }
    let tar = dir
        .parent()
        .unwrap_or(Path::new("."))
        .join(pack["tar"]["file"].as_str().unwrap_or_default());
    if sha256_hex(&read(&tar)?) != pack["tar"]["sha256"].as_str().unwrap_or_default() {
        return Err(format!(
            "{} is not the tar pack.json records",
            tar.display()
        ));
    }
    let out = remote_with(to, &["publish", site.key], Some(&tar))?;
    print!("{out}");
    // The record of what is live.
    let path = ws.root.join(PUBLISHED_FILE);
    let mut published: Value = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| json!({ "format": "dnacalc-site-published/1", "sites": {} }));
    published["sites"][site.key] = json!({
        "host": site.host,
        "release": pack["release"],
        "commit": pack["commit"],
        "build_json_sha256": pack["build_json_sha256"],
        "tar_sha256": pack["tar"]["sha256"],
        "published_utc": utc_now(),
    });
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write(&path, &pretty(&published)?)?;
    println!(
        "SITE published {} release {}; recorded in {PUBLISHED_FILE} (commit it); now: cargo xtask site verify {} --url https://{}/ --release {}",
        site.key,
        pack["release"].as_str().unwrap_or_default(),
        site.key,
        site.host,
        dir.display()
    );
    Ok(())
}

fn rollback(site: &Site, to: &str, release: &str) -> Result<(), String> {
    attended()?;
    let status: Value = serde_json::from_str(&remote_with(to, &["status", site.key], None)?)
        .map_err(|e| format!("the receiver's status is not JSON: {e}"))?;
    let current = status["current"]
        .as_str()
        .ok_or("the host reports no current release")?
        .to_owned();
    let out = remote_with(
        to,
        &["rollback", site.key, release, "--if-current", &current],
        None,
    )?;
    print!("{out}");
    println!("SITE {} switched from {current} to {release}", site.key);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: reads the repository's own snippet from the file system.
    #[test]
    fn the_snippet_gives_each_path_its_headers() {
        let text = include_str!("../../../deploy/caddy/headers.caddy");
        let headers = SiteHeaders::parse(text).unwrap();
        let names = |path: &str| -> Vec<String> {
            headers
                .for_path(path)
                .into_iter()
                .map(|(n, v)| format!("{n}: {v}"))
                .collect()
        };
        let page = names("/");
        assert!(page.iter().any(|h| h.starts_with(
            "Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval';"
        )));
        assert!(page.contains(&"Cache-Control: no-cache".to_owned()));
        let asset = names("/assets/0123456789abcdef/boot.js");
        assert!(asset.contains(&"Cache-Control: public, max-age=31536000, immutable".to_owned()));
        assert!(asset.contains(&"X-Content-Type-Options: nosniff".to_owned()));
        assert!(
            !names("/other")
                .iter()
                .any(|h| h.starts_with("Cache-Control"))
        );
        assert!(SiteHeaders::parse("header {\n  X-A \"1\"\n").is_err());
        assert!(SiteHeaders::parse("header X-A \"1\"").is_err());
        assert!(SiteHeaders::parse("header {\n X-A 1\n}").is_err());
    }

    /// Native-only: the archive is checked byte by byte, no browser involved.
    #[test]
    fn tar_entries_are_ustar_with_fixed_owner_and_time() {
        let tar = tar(
            &[
                ("r/".to_owned(), None),
                ("r/a.txt".to_owned(), Some(b"hello".to_vec())),
            ],
            1_700_000_000,
        )
        .unwrap();
        assert_eq!(tar.len(), 512 + 512 + 512 + 1024);
        assert_eq!(&tar[0..2], b"r/");
        assert_eq!(tar[156], b'5');
        assert_eq!(&tar[512 + 124..512 + 135], b"00000000005");
        assert_eq!(&tar[512 + 257..512 + 262], b"ustar");
        assert_eq!(&tar[1024..1029], b"hello");
        let sum: u32 = tar[512..1024]
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    u32::from(*b)
                }
            })
            .sum();
        assert_eq!(&tar[512 + 148..512 + 154], format!("{sum:06o}").as_bytes());
        assert_eq!(compact_utc("2026-10-29T10:15:00Z"), "20261029T101500Z");
    }
}
