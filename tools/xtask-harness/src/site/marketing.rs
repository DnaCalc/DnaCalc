//! The public site pages, built from the checked-in copy, evidence and app bundles.

use super::content::{escape, markdown};
use super::{app_page, blog, downloads, files_under, read, write_nested};
use crate::app::{bind_web_entry, web_bindgen};
use crate::cargo::{self, Workspace};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::Path;

const FUN: &str = "docs/fun";
const NUMBER_TEXT_ARTICLE: &str = "docs/articles/last-bit-number-text.md";
const NUMBER_TEXT_ROUTE: &str = "/articles/last-bit-number-text/";
const PAGES: &str = "docs/site/pages";
const LAYOUT: &str = "docs/site/layout.json";
const WITNESS: &str = "crates/calc-func/witness-status.tsv";
const RELEASES: &str = "docs/plan/RELEASES.md";
const VBA: &str = "crates/calc-vba/tests/corpus/baseline.json";
const SAMPLE: &str = "crates/calc-doc/tests/fixtures/cdoc_m1_2/d1.xlsx";

const NAV: &[(&str, &str)] = &[
    ("/", "Home"),
    ("/desktop/", "DNA Calc"),
    ("/onecalc/", "Formula Explorer"),
    ("/web/", "In the browser"),
    ("/developers/", "Developers"),
    ("/handbook/", "Handbook"),
    ("/play/", "Playground"),
    ("/blog/", "Blog"),
    ("/download/", "Download"),
];

const PAGES_BY_ROUTE: &[(&str, &str, &str)] = &[
    ("", "home", "Excel workbooks on Windows"),
    ("desktop/", "desktop", "DNA Calc for Windows"),
    ("onecalc/", "onecalc", "DNA Calc - Formula Explorer"),
    ("web/", "web", "In the browser"),
    ("developers/", "developers", "Developers"),
    ("developers/addins/", "addins", "Add-ins"),
    ("developers/vba/", "vba", "VBA"),
    ("developers/engine/", "engine", "Engine and runner"),
    ("handbook/", "handbook", "Handbook of Excel Functions"),
    ("play/", "play", "Playground"),
    ("download/", "download", "Download"),
    ("about/", "about", "About"),
    ("licence/", "licence", "Licence"),
    ("status/", "status", "Status"),
];

#[derive(Deserialize)]
struct Settings {
    browser_app_url: String,
    planned_release: String,
}

pub(super) struct Layout<'a> {
    prefix: &'a str,
    explorer: &'a str,
}

impl Layout<'_> {
    pub(super) fn page(&self, route: &str, title: &str, extra: &str, contents: &str) -> String {
        let mut links = String::new();
        for &(href, label) in NAV {
            let active = href == route || (href != "/" && route.starts_with(href));
            let current = if active {
                r#" aria-current="page""#
            } else {
                ""
            };
            let _ = write!(links, r#"<a href="{href}"{current}>{label}</a>"#);
        }
        let title = escape(title);
        let prefix = self.prefix;
        let explorer = if contents.contains(r#"id="onecalc""#) {
            format!(
                r#"<link rel="stylesheet" href="{prefix}/explorer/app.css"><script type="module" src="{prefix}/explorer/boot.js"></script>"#
            )
        } else {
            String::new()
        };
        format!(
            r##"<!doctype html>
<html lang="en" data-dna-theme="cockpit-dark">
<head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} | DNA Calc</title>
<link rel="icon" href="{prefix}/favicon.ico">
{explorer}<link rel="stylesheet" href="{prefix}/site.css">{extra}
</head>
<body class="site">
<a class="site-skip" href="#main">Skip to content</a>
<header class="site-header"><div class="site-bar">
<a class="site-brand" href="/"><span class="site-mark" aria-hidden="true"><i></i><i></i><i></i><i></i></span>DNA Calc</a>
<nav class="site-nav" aria-label="Main navigation">{links}</nav>
</div></header>
<main class="site-main" id="main">{contents}</main>
<footer class="site-footer"><div class="site-footer-inner"><span>DNA Calc · Excel workbook calculation</span>
<a href="/about/">About</a><a href="/licence/">Licence</a><a href="/status/">Status</a><a href="/build.json">Build record</a>
</div></footer>
</body>
</html>
"##
        )
    }
}

pub(super) fn stage_assets(ws: &Workspace, staging: &Path) -> Result<(), String> {
    let root = &ws.root;
    write_nested(
        &staging.join("favicon.ico"),
        &read(&root.join("apps/dna-calc-desktop/icon/calc.ico"))?,
    )?;
    let mut css = dna_strand::stylesheet();
    css.push_str(&source(root, "docs/site/site.css")?);
    write_nested(&staging.join("site.css"), css.as_bytes())?;
    write_nested(
        &staging.join("handbook.js"),
        &read(&root.join("docs/site/handbook.js"))?,
    )?;
    for name in fun_files(root)? {
        if name.ends_with(".css") || name.ends_with(".js") {
            write_nested(
                &staging.join("fun").join(&name),
                &read(&root.join(FUN).join(name))?,
            )?;
        }
    }
    println!("SITE CONTENT building CalcFunc's number-to-text explorer");
    bind_web_entry(
        ws,
        &web_bindgen()?,
        "calc-func",
        ["--example", "number-text-wasm"],
        None,
        &staging.join("fun"),
    )
}

pub(super) fn build(
    ws: &Workspace,
    output: &Path,
    prefix: &str,
    calc: &str,
    explorer: &str,
    commit: &str,
    version: &str,
    built: &str,
    include_drafts: bool,
) -> Result<(), String> {
    let root = &ws.root;
    let settings: Settings =
        serde_json::from_str(&source(root, LAYOUT)?).map_err(|e| format!("{LAYOUT}: {e}"))?;
    if !matches!(
        settings.browser_app_url.as_str(),
        "/app/" | "https://app.dnacalc.com/" | "https://dnacalc.com/"
    ) {
        return Err(format!(
            "{LAYOUT}: browser_app_url must be /app/, https://app.dnacalc.com/ or https://dnacalc.com/"
        ));
    }
    let url = escape(&settings.browser_app_url);
    let app_address = if include_drafts {
        format!(r#"<p class="site-note">Configured public app address <code>{url}</code></p>"#)
    } else {
        format!(r#"<p><a href="{url}">Open the public browser app</a></p>"#)
    };
    let layout = Layout { prefix, explorer };
    let workbook = corpus_metric(root)?;
    let bank = bank_metric(root)?;
    let metrics = format!(r#"<div class="site-metrics">{workbook}{bank}</div>"#);
    let cards = build_fun(root, output, prefix)?;
    let article = layout.page(
        NUMBER_TEXT_ROUTE,
        "The last bit: Excel’s number-to-text conversion",
        "",
        &markdown(&source(root, NUMBER_TEXT_ARTICLE)?),
    );
    write_nested(
        &output
            .join(NUMBER_TEXT_ROUTE.trim_start_matches('/'))
            .join("index.html"),
        article.as_bytes(),
    )?;
    let vba = vba_table(root)?;
    let builds = builds_table(root, version, commit, built)?;
    let runner = runner_commands(root)?;
    let live = live_box(layout.explorer)?;
    let packages = downloads::preview(root, output, &layout, &settings.planned_release, commit)?;

    for &(route, filename, title) in PAGES_BY_ROUTE {
        let mut contents = source(root, &format!("{PAGES}/{filename}.html"))?;
        for (key, value) in [
            ("live", live.as_str()),
            ("metrics", &metrics),
            ("bank", &bank),
            ("cards", &cards),
            ("vba", &vba),
            ("builds", &builds),
            ("runner", &runner),
            ("packages", &packages),
            ("app_address", &app_address),
            ("release_version", settings.planned_release.as_str()),
        ] {
            contents = contents.replace(&format!("{{{{{key}}}}}"), value);
        }
        if contents.contains("{{") {
            return Err(format!("{filename}: unresolved site placeholder"));
        }
        let extra = if filename == "handbook" {
            format!(r#"<script src="{prefix}/handbook.js" defer></script>"#)
        } else {
            String::new()
        };
        let page = layout.page(&format!("/{route}"), title, &extra, &contents);
        write_nested(&output.join(format!("{route}index.html")), page.as_bytes())?;
    }
    write_nested(
        &output.join("app/index.html"),
        with_icon(
            app_page(calc.to_owned(), &format!("{prefix}/calc"))?,
            prefix,
        )
        .as_bytes(),
    )?;
    write_nested(&output.join("sample.xlsx"), &read(&root.join(SAMPLE))?)?;
    let posts = blog::build(root, output, "dnacalc.com", include_drafts, Some(&layout))?;
    println!(
        "SITE CONTENT {} pages, {posts} blog posts",
        PAGES_BY_ROUTE.len()
    );
    Ok(())
}

fn source(root: &Path, path: &str) -> Result<String, String> {
    fs::read_to_string(root.join(path)).map_err(|e| format!("{path}: {e}"))
}

fn with_icon(page: String, prefix: &str) -> String {
    page.replacen(
        "<title>",
        &format!(r#"<link rel="icon" href="{prefix}/favicon.ico"><title>"#),
        1,
    )
}

fn live_box(explorer: &str) -> Result<String, String> {
    let start = explorer
        .find(r#"<div id="onecalc">"#)
        .ok_or("Formula Explorer mount is missing")?;
    let after = explorer[start..]
        .find("</div>")
        .ok_or("Formula Explorer mount has no end")?
        + start
        + "</div>".len();
    Ok(format!(
        r#"<div class="site-live">{}</div>"#,
        &explorer[start..after]
    ))
}

/// The exhibit HTML stays usable at its source location. The pack only rewrites its local assets.
fn fun_files(root: &Path) -> Result<Vec<String>, String> {
    // Keep tools and their outputs out of the site, including untracked local Python caches.
    let entries = fs::read_dir(root.join(FUN)).map_err(|e| format!("{FUN}: {e}"))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "exhibit filename is not UTF-8")?;
        if entry.path().is_file()
            && ["html", "css", "js"]
                .iter()
                .any(|ext| name.ends_with(&format!(".{ext}")))
        {
            names.push(name);
        }
    }
    names.extend(
        files_under(&root.join(FUN).join("vendor"))?
            .into_iter()
            .map(|name| format!("vendor/{name}")),
    );
    names.sort();
    Ok(names)
}

fn build_fun(root: &Path, output: &Path, prefix: &str) -> Result<String, String> {
    let files = fun_files(root)?;
    let mut count = 0;
    for name in files.iter().filter(|name| name.ends_with(".html")) {
        let mut page = source(root, &format!("{FUN}/{name}"))?;
        for asset in files.iter().filter(|name| !name.ends_with(".html")) {
            page = page.replace(
                &format!(r#"="{asset}""#),
                &format!(r#"="{prefix}/fun/{asset}""#),
            );
        }
        if name == "index.html" {
            page = page.replace(
                "<body>",
                r#"<body><a class="fun-return" href="/play/">← DNA Calc Playground</a>"#,
            );
        } else {
            count += 1;
        }
        if name == "number-text.html" {
            page = page.replace("../articles/last-bit-number-text.md", NUMBER_TEXT_ROUTE);
        }
        write_nested(
            &output.join("fun").join(name),
            with_icon(page, prefix).as_bytes(),
        )?;
    }
    let index = source(root, &format!("{FUN}/index.html"))?;
    let (_, cards) = index
        .split_once("  <h2>")
        .ok_or("docs/fun/index.html has no exhibit headings")?;
    let (cards, _) = cards
        .split_once("  <footer>")
        .ok_or("docs/fun/index.html has no exhibit footer")?;
    let cards = format!("<h2>{cards}").replace(r#"class="grid""#, r#"class="site-grid""#);
    let cards = cards.replace(r#" href=""#, r#" href="/fun/"#);
    println!("SITE CONTENT {count} Playground exhibits");
    Ok(cards)
}

fn number(n: u64) -> String {
    let digits = n.to_string();
    let mut rendered = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            rendered.push(',');
        }
        rendered.push(c);
    }
    rendered
}

fn metric(title: &str, matched: u64, total: u64, evidence: &str) -> Result<String, String> {
    if total == 0 || matched > total {
        return Err(format!("invalid totals for {title}"));
    }
    let rate = 100.0 * matched as f64 / total as f64;
    Ok(format!(
        r#"<section class="site-metric"><p class="site-kicker">Preview · {title}</p><strong>{rate:.2}%</strong><p>{} of {} matched · {} failed</p><p class="site-note">{evidence}</p></section>"#,
        number(matched),
        number(total),
        number(total - matched)
    ))
}

fn corpus_metric(root: &Path) -> Result<String, String> {
    let plan = source(root, RELEASES)?;
    let baseline = plan
        .lines()
        .find(|line| line.contains("formula cells (commit "))
        .ok_or_else(|| format!("{RELEASES}: missing the oracle corpus baseline"))?;
    let (counts, rest) = baseline
        .trim()
        .split_once(" formula cells (commit ")
        .ok_or_else(|| format!("{RELEASES}: invalid oracle corpus baseline"))?;
    let (matched, total) = counts.split_once(" of ").ok_or("missing corpus counts")?;
    let (commit, _) = rest.split_once(')').ok_or("missing corpus commit")?;
    let date = cargo::git(root, &["show", "-s", "--format=%cs", commit])?;
    let manifest = cargo::git(root, &["show", &format!("{commit}:Cargo.toml")])?;
    let version = super::version_from_manifest(&manifest)?;
    let evidence = format!(
        "Oracle workbook formula cells · {} · {} · v{}",
        date.trim(),
        escape(commit),
        escape(&version)
    );
    let numeric = |n: &str| {
        n.replace(',', "")
            .parse::<u64>()
            .map_err(|e| format!("oracle corpus count: {e}"))
    };
    metric(
        "Workbook corpus",
        numeric(matched)?,
        numeric(total)?,
        &evidence,
    )
}

fn bank_metric(root: &Path) -> Result<String, String> {
    let table = source(root, WITNESS)?;
    let mut rows = table
        .lines()
        .map(|line| line.split('\t').collect::<Vec<_>>());
    let headers = rows.next().ok_or("missing witness-status header")?;
    let col = |name: &str| {
        headers
            .iter()
            .position(|label| *label == name)
            .ok_or_else(|| format!("{WITNESS}: missing {name} column"))
    };
    let [bank, rows_col, matched_col] = [col("bank")?, col("rows")?, col("matched")?];
    let mut functions = 0;
    let mut total = 0;
    let mut matched = 0;
    for row in rows.filter(|row| row.get(bank).copied() == Some("full")) {
        let count = |i| {
            row.get(i)
                .and_then(|v: &&str| v.parse::<u64>().ok())
                .ok_or_else(|| format!("{WITNESS}: invalid full-bank row {row:?}"))
        };
        functions += 1;
        total += count(rows_col)?;
        matched += count(matched_col)?;
    }
    let origin = cargo::git(root, &["log", "-1", "--format=%cs · %h", "--", WITNESS])?;
    let evidence = format!(
        "{functions} functions · {} · witness-status.tsv",
        origin.trim()
    );
    metric("Function bank", matched, total, &evidence)
}

fn builds_table(root: &Path, version: &str, commit: &str, built: &str) -> Result<String, String> {
    let path = super::PUBLISHED_FILE;
    let data: Value =
        serde_json::from_str(&source(root, path)?).map_err(|e| format!("{path}: {e}"))?;
    let mut html = String::from(
        r#"<h2>Web builds</h2><div class="site-table"><table><thead><tr><th>Build</th><th>Version</th><th>Recorded UTC</th><th>Source commit</th></tr></thead><tbody>"#,
    );
    let _ = write!(
        html,
        "<tr><td>Local site preview, including both apps</td><td>{version}</td><td>{built}</td><td><code>{commit}</code></td></tr>"
    );
    for (key, name) in [
        ("calc", "DNA Calc (public web app)"),
        ("onecalc", "DNA Calc - Formula Explorer (public web app)"),
    ] {
        let release = &data["sites"][key];
        let published = release["published_utc"]
            .as_str()
            .ok_or_else(|| format!("{path}: missing {key} publish time"))?;
        let revision = release["commit"]
            .as_str()
            .ok_or_else(|| format!("{path}: missing {key} commit"))?;
        let live_manifest = cargo::git(root, &["show", &format!("{revision}:Cargo.toml")])?;
        let live_version = super::version_from_manifest(&live_manifest)?;
        let _ = write!(
            html,
            "<tr><td>{name}</td><td>{}</td><td>{}</td><td><code>{}</code></td></tr>",
            escape(&live_version),
            escape(published),
            escape(revision)
        );
    }
    html.push_str("</tbody></table></div><p class=\"site-note\">The committed public web build record supplies the build identities and file hashes. Formula measurements below come from their own evidence snapshots.</p>");
    Ok(html)
}

#[derive(Deserialize)]
struct VbaCounts {
    matched: u64,
    mismatched: u64,
    refused: u64,
    missing: u64,
}

fn vba_table(root: &Path) -> Result<String, String> {
    let baseline: BTreeMap<String, BTreeMap<String, VbaCounts>> =
        serde_json::from_str(&source(root, VBA)?).map_err(|e| format!("{VBA}: {e}"))?;
    let mut html = String::from(
        r#"<div class="site-table"><table><thead><tr><th>Project / route</th><th>Matched</th><th>Mismatched</th><th>Refused</th><th>Missing</th></tr></thead><tbody>"#,
    );
    for (platform, routes) in baseline {
        for (name, cases) in routes {
            let _ = write!(
                html,
                "<tr><th scope=\"row\">{} ({})</th><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape(&name),
                escape(&platform),
                cases.matched,
                cases.mismatched,
                cases.refused,
                cases.missing
            );
        }
    }
    html.push_str("</tbody></table></div>");
    Ok(html)
}

fn runner_commands(root: &Path) -> Result<String, String> {
    let runner = source(root, "deploy/linux/README.txt")?;
    let lines = runner
        .lines()
        .filter_map(|line| line.strip_prefix("  dnacalc "))
        .map(|command| format!("dnacalc {command}"))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return Err("Linux runner README has no commands".to_owned());
    }
    Ok(markdown(&format!("```text\n{}\n```", lines.join("\n"))))
}
