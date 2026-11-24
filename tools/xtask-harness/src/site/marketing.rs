//! The public site pages, built from the checked-in copy, evidence and app bundles.

use super::content::{TABLE_ALIGNMENT, escape, markdown, render_markdown, resolve_markdown};
use super::llms::{self, Index, Section};
use super::{CONTENT_HOST, app_page, blog, downloads, files_under, read, write_nested};
use crate::app::{bind_web_entry, web_bindgen};
use crate::cargo::{self, Workspace};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::Path;

const FUN: &str = "docs/fun";
const NUMBER_TEXT_ROUTE: &str = "/articles/last-bit-number-text/";
const ARTICLES: &[(&str, &str, Section, &str)] = &[
    (
        "last-bit-normsinv.md",
        "/articles/last-bit-normsinv/",
        Section::Evidence,
        "How measured tail values reproduce Excel's inverse normal distribution.",
    ),
    (
        "last-bit-gamma.md",
        "/articles/last-bit-gamma/",
        Section::Evidence,
        "Excel measurements and arithmetic behind the gamma family.",
    ),
    (
        "last-bit-beta.md",
        "/articles/last-bit-beta/",
        Section::Evidence,
        "How the 29 incomplete-beta callers moved across AS 109, Ext80 bgrat and Modified-Lentz bfrac.",
    ),
    (
        "last-bit-pmt.md",
        "/articles/last-bit-pmt/",
        Section::Evidence,
        "How the five PMT and amortization functions reached 100% exact on 308,710 witness rows.",
    ),
    (
        "last-bit-bessel.md",
        "/articles/last-bit-bessel/",
        Section::Evidence,
        "The rounding of large-argument Bessel approximations and backward recurrence.",
    ),
    (
        "last-bit-linest.md",
        "/articles/last-bit-linest/",
        Section::Evidence,
        "The rounding and summation order behind Excel's regression and prediction results.",
    ),
    (
        "last-bit-number-text.md",
        NUMBER_TEXT_ROUTE,
        Section::Evidence,
        "Excel's 15-digit text conversion and the evidence behind it.",
    ),
    (
        "last-bit-status.md",
        "/articles/last-bit-status/",
        Section::Evidence,
        "Witness results for the numerical families covered in the last-bit articles.",
    ),
    (
        "calcvba-variant.md",
        "/articles/calcvba-variant/",
        Section::Developers,
        "How CalcVBA represents Automation values and preserves COM identity.",
    ),
];
const VIDEO: &[(&str, &str)] = &[
    ("README.md", "/blog/video/"),
    ("SERIES_PLAN.md", "/blog/video/series-plan/"),
    (
        "01-dna-calc-on-windows.md",
        "/blog/video/01-dna-calc-on-windows/",
    ),
    (
        "02-formula-explorer-addin-bench.md",
        "/blog/video/02-formula-explorer-addin-bench/",
    ),
    (
        "03-how-we-match-excel.md",
        "/blog/video/03-how-we-match-excel/",
    ),
];
const PAGES: &str = "docs/site/pages";
const LAYOUT: &str = "docs/site/layout.json";
const WITNESS: &str = "crates/calc-func/witness-status.tsv";
const FUZZ_STATUS: &str = crate::fuzz::coverage::FUZZ_STATUS_PATH;
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

type Page = (
    &'static str,
    &'static str,
    &'static str,
    Option<(Section, &'static str)>,
);

const PAGES_BY_ROUTE: &[Page] = &[
    ("", "home", "Excel workbooks on Windows", None),
    (
        "desktop/",
        "desktop",
        "DNA Calc for Windows",
        Some((
            Section::Products,
            "Open Excel workbooks and use the Windows hosts for native add-ins and VBA.",
        )),
    ),
    (
        "onecalc/",
        "onecalc",
        "DNA Calc - Formula Explorer",
        Some((
            Section::Products,
            "Explore single formulas in the browser and test native add-ins in the Windows preview.",
        )),
    ),
    (
        "web/",
        "web",
        "In the browser",
        Some((
            Section::Products,
            "Open and calculate workbooks in the browser. Native XLL and COM add-ins use the Windows desktop hosts.",
        )),
    ),
    (
        "developers/",
        "developers",
        "Developers",
        Some((
            Section::Developers,
            "Choose a workbook, formula, VBA or add-in integration.",
        )),
    ),
    (
        "developers/addins/",
        "addins",
        "Add-ins",
        Some((
            Section::Developers,
            "Run the pinned Windows x64 XLL bench example in Formula Explorer.",
        )),
    ),
    (
        "developers/vba/",
        "vba",
        "VBA",
        Some((
            Section::Developers,
            "Use the experimental VBA host and read its measured coverage by project and route.",
        )),
    ),
    (
        "developers/engine/",
        "engine",
        "Engine and runner",
        Some((
            Section::Developers,
            "Round-trip a workbook with the Linux runner or the Rust workbook doctest.",
        )),
    ),
    (
        "handbook/",
        "handbook",
        "Handbook of Excel Functions",
        Some((
            Section::Handbook,
            "Search the Handbook from the product site.",
        )),
    ),
    ("play/", "play", "Playground", None),
    (
        "download/",
        "download",
        "Download",
        Some((
            Section::Products,
            "See preview package names for Windows and the Linux runner.",
        )),
    ),
    ("about/", "about", "About", None),
    ("licence/", "licence", "Licence", None),
    (
        "status/",
        "status",
        "Status",
        Some((
            Section::Evidence,
            "Read dated workbook and Function Bank results with their denominators and build references.",
        )),
    ),
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
        let discovery = llms::DESCRIBED_BY;
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
{explorer}<link rel="stylesheet" href="{prefix}/site.css">{discovery}{extra}
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
    css.push_str(TABLE_ALIGNMENT);
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
    let mut index = Index::default();
    let vba = vba_table(root)?;
    let fuzz = fuzz_section(root)?;
    let builds = builds_table(root, version, commit, built)?;
    let runner = runner_commands(root)?;
    let live = live_box(layout.explorer)?;
    let packages = downloads::preview(root, output, &layout, &settings.planned_release, commit)?;

    for &(route, filename, title, discovery) in PAGES_BY_ROUTE {
        let mut contents = source(root, &format!("{PAGES}/{filename}.html"))?;
        for (key, value) in [
            ("live", live.as_str()),
            ("metrics", &metrics),
            ("bank", &bank),
            ("cards", &cards),
            ("vba", &vba),
            ("fuzz", &fuzz),
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
        let route_url = format!("/{route}");
        if let Some((section, description)) = discovery {
            index.add(section, title, &route_url, description);
        }
        let page = layout.page(&route_url, title, &extra, &contents);
        write_nested(&output.join(format!("{route}index.html")), page.as_bytes())?;
    }
    write_nested(
        &output.join("app/index.html"),
        with_site_metadata(
            app_page(calc.to_owned(), &format!("{prefix}/calc"))?,
            prefix,
        )
        .as_bytes(),
    )?;
    write_nested(&output.join("sample.xlsx"), &read(&root.join(SAMPLE))?)?;
    for (section, label, url, description) in [
        (
            Section::Developers,
            "Embed Formula Explorer",
            "/onecalc/#embedding",
            "Mount the experimental Rust/Wasm component from a pinned build.",
        ),
        (
            Section::Handbook,
            "Handbook llms.txt",
            "https://handbook.dnacalc.com/llms.txt",
            "Discover the Handbook's reference and structured records.",
        ),
        (
            Section::Handbook,
            "Handbook catalogue",
            "https://handbook.dnacalc.com/catalogue/index.html",
            "Browse Excel functions, operators and aliases.",
        ),
        (
            Section::Handbook,
            "ROUND example",
            "https://handbook.dnacalc.com/functions/ROUND.html",
            "Read behaviour, quirks, examples and available evidence for ROUND.",
        ),
        (
            Section::Handbook,
            "ROUND JSON",
            "https://handbook.dnacalc.com/api/functions/FUNC.ROUND.json",
            "Get the matching structured record and its sources.",
        ),
        (
            Section::Handbook,
            "Handbook function notes",
            "https://handbook.dnacalc.com/notes/index.html",
            "Read recorded Excel quirks, their scope and reproductions.",
        ),
        (
            Section::Handbook,
            "Handbook notes JSON",
            "https://handbook.dnacalc.com/api/notes.json",
            "Get the related machine-readable note records.",
        ),
        (
            Section::Handbook,
            "Handbook ID index",
            "https://handbook.dnacalc.com/api/index.json",
            "Resolve function, operator and alias IDs to canonical records.",
        ),
        (
            Section::Evidence,
            "Build record",
            "/build.json",
            "Inspect this site's source revision, build time and file hashes.",
        ),
        (
            Section::Evidence,
            "Function Bank preview",
            "/fun/function-bank.html",
            "Explore measured Excel answers and bit-level comparisons.",
        ),
    ] {
        index.add(section, label, url, description);
    }
    build_guides(root, output, &layout, include_drafts, &mut index)?;
    let posts = blog::build(
        root,
        output,
        CONTENT_HOST,
        include_drafts,
        Some((&layout, &mut index)),
    )?;
    index.write(output)?;
    println!(
        "SITE CONTENT {} pages, {posts} blog posts",
        PAGES_BY_ROUTE.len()
    );
    Ok(())
}

fn source(root: &Path, path: &str) -> Result<String, String> {
    fs::read_to_string(root.join(path)).map_err(|e| format!("{path}: {e}"))
}

fn build_guides(
    root: &Path,
    output: &Path,
    layout: &Layout<'_>,
    include_drafts: bool,
    index: &mut Index,
) -> Result<(), String> {
    let write_doc = |route: &str, title: &str, content: &str, edition: Option<(&str, &str)>| {
        let body = format!(r#"<article class="site-doc">{content}</article>"#);
        let extra = if let Some((path, text)) = edition {
            llms::write_edition(output, path, text)?
        } else {
            String::new()
        };
        let page = layout.page(route, title, &extra, &body);
        write_nested(
            &output
                .join(route.trim_start_matches('/'))
                .join("index.html"),
            page.as_bytes(),
        )
    };
    index.add(
        Section::Evidence,
        "Technical articles",
        "/articles/",
        "Read investigations into Excel’s numerical behaviour and CalcVBA’s Automation layout.",
    );
    let mut articles = String::from("<h1>Technical articles</h1><ul>");
    for &(filename, route, section, description) in ARTICLES {
        let path = format!("docs/articles/{filename}");
        let (title, content) = markdown_doc(root, &path)?;
        let mut content = resolve_markdown(&content, |link| {
            ARTICLES
                .iter()
                .find(|&&(name, _, _, _)| name == link)
                .map(|&(_, route, _, _)| route.to_owned())
        })
        .map_err(|e| format!("{path}: {e}"))?;
        if route == NUMBER_TEXT_ROUTE {
            content.push_str("\n\n[Try the number-to-text explorer](/fun/number-text.html)\n");
        }
        let html = markdown(&content);
        let _ = write!(
            articles,
            r#"<li><a href="{route}">{}</a></li>"#,
            escape(&title)
        );
        let body = format!(r#"<p><a href="/articles/">All technical articles</a></p>{html}"#);
        let edition = format!("{route}index.md");
        write_doc(route, &title, &body, Some((&edition, &content)))?;
        index.add(section, &title, &edition, description);
    }
    articles.push_str("</ul>");
    write_doc("/articles/", "Technical articles", &articles, None)?;

    let (title, guide) = markdown_doc(root, "docs/user/classic-grid.md")?;
    let edition = "/help/classic-grid/index.md";
    write_doc(
        "/help/classic-grid/",
        &title,
        &markdown(&guide),
        Some((edition, &guide)),
    )?;
    index.add(
        Section::Products,
        &title,
        edition,
        "Look up grid controls, keyboard shortcuts, Paste Special and Flash Fill.",
    );

    if include_drafts {
        for &(filename, route) in VIDEO {
            let (title, content) = markdown_doc(root, &format!("docs/site/video/{filename}"))?;
            let html = render_markdown(&content, |link| {
                if let Some(post) = link.strip_prefix("../blog/") {
                    return Some(match post.strip_suffix(".md") {
                        Some(stem) => format!("/blog/{stem}.html"),
                        None => "/blog/".to_owned(),
                    });
                }
                VIDEO
                    .iter()
                    .find(|&&(name, _)| name == link)
                    .map(|&(_, route)| route.to_owned())
            });
            let body = if filename == "README.md" {
                html
            } else {
                format!(r#"<p><a href="/blog/video/">Video drafts</a></p>{html}"#)
            };
            write_doc(route, &title, &body, None)?;
        }
    }
    Ok(())
}

fn markdown_doc(root: &Path, path: &str) -> Result<(String, String), String> {
    let content = source(root, path)?;
    let title = content
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("# "))
        .ok_or_else(|| format!("{path}: expected a document title"))?;
    Ok((title.replace('`', ""), content))
}

fn with_site_metadata(page: String, prefix: &str) -> String {
    let discovery = llms::DESCRIBED_BY;
    page.replacen(
        "<title>",
        &format!(r#"<link rel="icon" href="{prefix}/favicon.ico">{discovery}<title>"#),
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
            with_site_metadata(page, prefix).as_bytes(),
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
        "Oracle workbook formula cells · Linux debug · {} · {} · v{}",
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

fn hash_name(function: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in function.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    out
}

fn fuzz_section(root: &Path) -> Result<String, String> {
    let table = source(root, FUZZ_STATUS)?;
    let rows: Vec<_> = crate::fuzz::coverage::parse_tsv(&table)?
        .into_iter()
        .filter(|r| r.cases_swept > 0)
        .collect();
    let mut html = String::from("<h2>Per-function fuzz coverage</h2>");
    if rows.is_empty() {
        html.push_str(
            r#"<p class="site-note">0 functions swept · 0 cases swept · 0 frontiers mapped · 0 open clusters. Generated from live Excel sweep records into <code>crates/calc-func/fuzz-status.tsv</code> via <code>cargo xtask fuzz coverage</code> and mirrored in the <a href="/fun/function-bank.html">Function Bank</a>.</p>"#,
        );
        return Ok(html);
    }
    let swept: u64 = rows.iter().map(|r| r.cases_swept).sum();
    let matched: u64 = rows.iter().map(|r| r.matched).sum();
    let frontiers: u64 = rows.iter().map(|r| r.frontiers_mapped).sum();
    let open_clusters: u64 = rows.iter().map(|r| r.open_clusters).sum();
    let latest_date = rows
        .iter()
        .map(|r| r.last_sweep_date.as_str())
        .max()
        .unwrap_or("-");
    let evidence = format!(
        "{} functions · {} frontiers · {} open clusters · last sweep {} · fuzz-status.tsv",
        number(rows.len() as u64),
        number(frontiers),
        number(open_clusters),
        escape(latest_date),
    );
    let summary = metric("Fuzz sweeps", matched, swept, &evidence)?;
    let _ = write!(html, r#"<div class="site-metrics">{summary}</div>"#);
    html.push_str(
        r#"<div class="site-table"><table><thead><tr><th>Function</th><th>Cases swept</th><th>Matched</th><th>Failed</th><th>Harness faults</th><th>Regions covered</th><th>Frontiers mapped</th><th>Open clusters</th><th>Last sweep</th></tr></thead><tbody>"#,
    );
    for row in &rows {
        let _ = write!(
            html,
            r#"<tr><th scope="row"><a href="/fun/function-bank.html#{}">{}</a></th><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{} / {}</td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
            escape(&hash_name(&row.function)),
            escape(&row.function),
            number(row.cases_swept),
            number(row.matched),
            number(row.failed),
            number(row.harness_faults),
            number(row.regions_covered),
            number(row.regions_total),
            number(row.frontiers_mapped),
            number(row.open_clusters),
            escape(&row.last_sweep_date),
        );
    }
    html.push_str(
        r#"</tbody></table></div><p class="site-note">Generated from live Excel sweep records into <code>crates/calc-func/fuzz-status.tsv</code>; open clusters exclude clusters whose tracker bead is closed. Browse full function cards in the <a href="/fun/function-bank.html">Function Bank</a>.</p>"#,
    );
    Ok(html)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: builds site content from the checkout into a temporary directory.
    #[test]
    fn preview_renders_the_articles_grid_guide_and_video_drafts() {
        let ws = Workspace {
            root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
            members: Vec::new(),
        };
        let output = std::env::temp_dir().join(format!("site-content-{}", std::process::id()));
        let result = (|| {
            for include_drafts in [false, true] {
                let dir = output.join(if include_drafts { "preview" } else { "public" });
                let explorer = r#"<div id="onecalc"></div>"#;
                build(
                    &ws,
                    &dir,
                    "/assets/test",
                    r#"<title>App</title><link href="app.css"><script src="boot.js"></script>"#,
                    explorer,
                    "0123456789012345678901234567890123456789",
                    "0.2.0",
                    "2026-11-22",
                    include_drafts,
                )?;
                for route in [
                    "articles/index.html",
                    "articles/last-bit-normsinv/index.html",
                    "articles/last-bit-gamma/index.html",
                    "articles/last-bit-number-text/index.html",
                    "help/classic-grid/index.html",
                    "articles/last-bit-status/index.html",
                    "articles/calcvba-variant/index.html",
                ] {
                    source(&dir, route)?;
                }
                let status = source(&dir, "articles/last-bit-status/index.html")?;
                assert!(status.contains(r#"href="/articles/last-bit-gamma/""#));
                let status_page = source(&dir, "status/index.html")?;
                assert!(status_page.contains("<h2>Per-function fuzz coverage</h2>"));
                assert!(status_page.contains("crates/calc-func/fuzz-status.tsv"));
                let vba = source(&dir, "developers/vba/index.html")?;
                assert!(vba.contains(r#"href="/articles/calcvba-variant/""#));
                let gamma = source(&dir, "articles/last-bit-gamma/index.html")?;
                assert!(gamma.contains(r#"href="/articles/last-bit-normsinv/""#));
                assert!(gamma.contains(r#"<td class="markdown-align-right">"#));
                assert!(!gamma.contains(r#" style="text-align:"#));
                let guide = source(&dir, "help/classic-grid/index.html")?;
                assert!(guide.contains("<table>"));
                let index = source(&dir, "llms.txt")?;
                assert!(index.starts_with("# DNA Calc\n\n> "));
                assert!(index.contains("https://dnacalc.com/onecalc/#embedding"));
                assert!(index.contains("https://dnacalc.com/articles/calcvba-variant/index.md"));
                assert!(index.contains("https://dnacalc.com/articles/last-bit-status/index.md"));
                for handbook in [
                    "llms.txt",
                    "catalogue/index.html",
                    "functions/ROUND.html",
                    "api/functions/FUNC.ROUND.json",
                    "api/index.json",
                    "notes/index.html",
                    "api/notes.json",
                ] {
                    assert!(index.contains(&format!("https://handbook.dnacalc.com/{handbook})")));
                }
                assert!(!index.contains("https://app.dnacalc.com/"));
                assert!(!index.contains("/blog/video/"));
                assert!(!index.contains("/download/release-notes"));
                assert!(guide.contains(r#"rel="describedby" href="/llms.txt""#));
                assert!(guide.contains(
                    r#"rel="alternate" type="text/markdown" href="/help/classic-grid/index.md""#
                ));
                let gamma_md = source(&dir, "articles/last-bit-gamma/index.md")?;
                assert!(gamma_md.contains("](/articles/last-bit-normsinv/)"));
                assert!(!dir.join("developers/engine/index.md").exists());
                assert!(!dir.join("blog/video/index.md").exists());
                let blog = source(&dir, "blog/index.html")?;
                assert_eq!(blog.contains(r#"href="/blog/video/""#), include_drafts);
                assert_eq!(dir.join("blog/video/index.html").is_file(), include_drafts);
                if include_drafts {
                    let episode = source(&dir, "blog/video/03-how-we-match-excel/index.html")?;
                    assert!(
                        episode.contains(r#"href="/blog/2026-11-21-checking-excel-answers.html""#)
                    );
                }
            }
            assert_eq!(
                source(&output.join("public"), "llms.txt")?,
                source(&output.join("preview"), "llms.txt")?
            );

            let blog_root = output.join("blog-source");
            for (name, status) in [("ready", "published"), ("review", "draft")] {
                write_nested(
                    &blog_root.join(format!("docs/site/blog/2026-11-22-{name}.md")),
                    format!("# A {name} post\nTags: grid\nStatus: {status}\n\nUse **cells**.\n")
                        .as_bytes(),
                )?;
            }
            let layout = Layout {
                prefix: "/assets/test",
                explorer: "",
            };
            for include_drafts in [false, true] {
                let dir = output.join(if include_drafts {
                    "blog-preview"
                } else {
                    "blog-public"
                });
                let mut index = Index::default();
                let count = blog::build(
                    &blog_root,
                    &dir,
                    CONTENT_HOST,
                    include_drafts,
                    Some((&layout, &mut index)),
                )?;
                assert_eq!(count, 1 + usize::from(include_drafts));
                index.write(&dir)?;
                let discovery = source(&dir, "llms.txt")?;
                assert!(discovery.contains("/blog/2026-11-22-ready.md"));
                assert!(!discovery.contains("/blog/2026-11-22-review"));
                let edition = source(&dir, "blog/2026-11-22-ready.md")?;
                assert!(edition.ends_with("Use **cells**.\n"));
                let html = source(&dir, "blog/2026-11-22-ready.html")?;
                assert!(html.contains(r#"type="text/markdown" href="/blog/2026-11-22-ready.md""#));
                assert!(!dir.join("blog/2026-11-22-review.md").exists());
                if include_drafts {
                    let html = source(&dir, "blog/2026-11-22-review.html")?;
                    assert!(!html.contains(r#"type="text/markdown""#));
                }
            }
            Ok::<_, String>(())
        })();
        let _ = fs::remove_dir_all(output);
        result.unwrap();
    }

    /// Native-only: writes a temporary fuzz-status.tsv and checks /status fuzz rendering.
    #[test]
    fn status_fuzz_section_renders_per_function_coverage_from_fuzz_status_tsv() {
        let base = std::env::temp_dir().join(format!(
            "dnacalc-site-fuzz-status-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let tsv_path = base.join(FUZZ_STATUS);
        fs::create_dir_all(tsv_path.parent().unwrap()).unwrap();

        // Empty table renders honest zero-state summary.
        fs::write(&tsv_path, format!("{}\n", crate::fuzz::coverage::FUZZ_STATUS_HEADER)).unwrap();
        let empty_html = fuzz_section(&base).unwrap();
        assert!(empty_html.contains("0 functions swept"));
        assert!(empty_html.contains("0 cases swept"));

        // Populated table renders metric card + per-function rows with every coverage column.
        let rows = vec![
            crate::fuzz::coverage::FuzzCoverageRow {
                function: "IRR".to_owned(),
                sweeps: 2,
                cases_swept: 1200,
                matched: 1150,
                failed: 50,
                harness_faults: 5,
                regions_covered: 7,
                regions_total: 8,
                frontiers_mapped: 12,
                open_clusters: 3,
                last_sweep_date: "2026-11-24".to_owned(),
                last_sweep_commit: "abcdef1234".to_owned(),
            },
            crate::fuzz::coverage::FuzzCoverageRow {
                function: "NORM.S.INV".to_owned(),
                sweeps: 1,
                cases_swept: 500,
                matched: 500,
                failed: 0,
                harness_faults: 0,
                regions_covered: 6,
                regions_total: 6,
                frontiers_mapped: 4,
                open_clusters: 0,
                last_sweep_date: "2026-11-23".to_owned(),
                last_sweep_commit: "1234567890".to_owned(),
            },
        ];
        fs::write(&tsv_path, crate::fuzz::coverage::format_tsv(&rows)).unwrap();
        let html = fuzz_section(&base).unwrap();
        assert!(html.contains("Fuzz sweeps"));
        assert!(html.contains("1,650 of 1,700 matched · 50 failed"));
        assert!(html.contains("2 functions · 16 frontiers · 3 open clusters · last sweep 2026-11-24"));
        assert!(html.contains(r#"href="/fun/function-bank.html#IRR">IRR</a>"#));
        assert!(html.contains(r#"href="/fun/function-bank.html#NORM-S-INV">NORM.S.INV</a>"#));
        assert!(html.contains("<td>1,200</td><td>1,150</td><td>50</td><td>5</td><td>7 / 8</td><td>12</td><td>3</td><td>2026-11-24</td>"));
        assert!(html.contains("<td>500</td><td>500</td><td>0</td><td>0</td><td>6 / 6</td><td>4</td><td>0</td><td>2026-11-23</td>"));

        // A 0-case row (e.g. statically skipped function) must NOT inflate the fuzzed function count.
        let mut with_zero = rows;
        with_zero.push(crate::fuzz::coverage::FuzzCoverageRow {
            function: "RAND".to_owned(),
            sweeps: 1,
            cases_swept: 0,
            matched: 0,
            failed: 0,
            harness_faults: 0,
            regions_covered: 0,
            regions_total: 0,
            frontiers_mapped: 0,
            open_clusters: 0,
            last_sweep_date: "2026-11-24".to_owned(),
            last_sweep_commit: "abcdef1234".to_owned(),
        });
        fs::write(&tsv_path, crate::fuzz::coverage::format_tsv(&with_zero)).unwrap();
        let html_zero = fuzz_section(&base).unwrap();
        assert!(
            html_zero.contains("2 functions · 16 frontiers"),
            "0-case skipped row must not inflate fuzzed function count: {html_zero}"
        );
        assert!(
            !html_zero.contains("#RAND"),
            "0-case skipped row must not render in /status swept table: {html_zero}"
        );

        let _ = fs::remove_dir_all(&base);
    }
}
