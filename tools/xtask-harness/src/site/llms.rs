//! The product site's discovery index and checks against the served build inventory.

use super::downloads::PREVIEW_NOTES;
use super::{CONTENT_HOST, write, write_nested};
use pulldown_cmark::{Event, Options, Parser, Tag};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;
use tungstenite::http::Uri;

pub(super) const FILE: &str = "llms.txt";
const TITLE: &str = "# DNA Calc";
const SECTIONS: [&str; 4] = [
    "Products",
    "Developer guides",
    "Handbook",
    "Evidence and articles",
];

#[derive(Clone, Copy)]
pub(super) enum Section {
    Products,
    Developers,
    Handbook,
    Evidence,
}

#[derive(Default)]
pub(super) struct Index {
    sections: [Vec<String>; SECTIONS.len()],
}

impl Index {
    pub(super) fn add(&mut self, section: Section, label: &str, url: &str, description: &str) {
        let label = label
            .replace('\\', "\\\\")
            .replace('[', "\\[")
            .replace(']', "\\]");
        let url = if url.starts_with('/') {
            format!("https://{CONTENT_HOST}{url}")
        } else {
            url.to_owned()
        };
        self.sections[section as usize].push(format!("- [{label}]({url}): {description}"));
    }

    pub(super) fn write(&self, dir: &Path) -> Result<(), String> {
        let mut index = format!(
            "{TITLE}\n\n> DNA Calc opens and calculates Excel workbooks without Excel, in the browser, on Linux and on Windows, and publishes how its results compare with Excel. It has a Formula Explorer for individual formulas and developer tools for workbooks, VBA and add-ins.\n"
        );
        for (name, entries) in SECTIONS.into_iter().zip(&self.sections) {
            let _ = writeln!(index, "\n## {name}\n");
            for entry in entries {
                let _ = writeln!(index, "{entry}");
            }
        }
        write(&dir.join(FILE), index.as_bytes())
    }
}

pub(super) fn write_edition(dir: &Path, path: &str, text: &str) -> Result<String, String> {
    write_nested(&dir.join(path.trim_start_matches('/')), text.as_bytes())?;
    Ok(format!(
        r#"<link rel="alternate" type="text/markdown" href="{path}">"#
    ))
}

pub(super) const DESCRIBED_BY: &str = r#"<link rel="describedby" href="/llms.txt">"#;

/// Check the index's intentionally small subset of the llms.txt format.
fn index_links(index: &str) -> Result<Vec<String>, String> {
    let mut blocks = index.trim_end().split("\n\n");
    if blocks.next() != Some(TITLE)
        || !blocks.next().is_some_and(|summary| {
            summary.starts_with("> ") && summary.len() > 2 && !summary.contains('\n')
        })
    {
        return Err("expected the DNA Calc title and a one-line blockquote".to_owned());
    }
    let mut links = Vec::new();
    for section in SECTIONS {
        if blocks.next() != Some(&format!("## {section}")) {
            return Err(format!("expected section {section}"));
        }
        for line in blocks
            .next()
            .ok_or_else(|| format!("empty section {section}"))?
            .lines()
        {
            let invalid = || format!("expected a described HTTPS link in {section}: {line}");
            let item = line
                .strip_prefix("- ")
                .filter(|item| item.starts_with('[') && !item.starts_with("[]"))
                .ok_or_else(invalid)?;
            let (url, span) = Parser::new(item)
                .into_offset_iter()
                .find_map(|(event, span)| match event {
                    Event::Start(Tag::Link { dest_url, .. }) => Some((dest_url, span)),
                    _ => None,
                })
                .ok_or_else(invalid)?;
            let description = item[span.end..].strip_prefix(": ").ok_or_else(invalid)?;
            if span.start != 0 || description.trim().is_empty() || !valid_https(&url) {
                return Err(invalid());
            }
            links.push(url.into_string());
        }
    }
    if blocks.next().is_some() {
        return Err("unexpected content after Evidence and articles".to_owned());
    }
    Ok(links)
}

fn valid_https(link: &str) -> bool {
    let target = link.split('#').next().unwrap_or_default();
    target.parse::<Uri>().is_ok_and(|uri| {
        uri.scheme_str() == Some("https")
            && uri
                .authority()
                .is_some_and(|authority| !authority.as_str().contains('@'))
    })
}

#[derive(Default)]
pub(super) struct Served {
    paths: BTreeSet<String>,
    text: BTreeMap<String, String>,
}

impl Served {
    pub(super) fn remember(
        &mut self,
        path: &str,
        name: &str,
        body: Vec<u8>,
        failures: &mut Vec<String>,
    ) {
        self.paths.insert(path.to_owned());
        if name == FILE || name.ends_with(".html") || name.ends_with(".md") {
            match String::from_utf8(body) {
                Ok(text) => {
                    self.text.insert(path.to_owned(), text);
                }
                Err(_) => failures.push(format!("{path}: expected UTF-8")),
            }
        }
    }

    /// Resolve links with the canonical origin mapped to this release's served routes.
    fn check_link(&self, page: &str, link: &str, failures: &mut Vec<String>) {
        let (target, fragment) = link.split_once('#').unwrap_or((link, ""));
        let target = target.split('?').next().unwrap_or_default();
        let uri;
        let target = if target.contains("://") || target.starts_with("//") {
            let absolute = if target.starts_with("//") {
                format!("https:{target}")
            } else {
                target.to_owned()
            };
            let Ok(parsed) = absolute.parse::<Uri>() else {
                failures.push(format!("{page}: invalid URL {link:?}"));
                return;
            };
            uri = parsed;
            if uri.scheme_str() != Some("https")
                || !uri
                    .host()
                    .is_some_and(|host| host.eq_ignore_ascii_case(CONTENT_HOST))
                || uri.port_u16().is_some_and(|port| port != 443)
            {
                return;
            }
            uri.path()
        } else if target.starts_with("mailto:") {
            return;
        } else {
            target
        };
        let path = if target.is_empty() {
            page.to_owned()
        } else {
            let base = if target.starts_with('/') {
                ""
            } else {
                page.rsplit_once('/').map_or("", |(dir, _)| dir)
            };
            let joined = format!("{base}/{target}");
            let mut pieces = Vec::new();
            for segment in joined.split('/') {
                match segment {
                    "" | "." => {}
                    ".." => {
                        pieces.pop();
                    }
                    _ => pieces.push(segment),
                }
            }
            let path = format!("/{}", pieces.join("/"));
            if path != "/" && (target.ends_with('/') || !self.paths.contains(&path)) {
                format!("{path}/")
            } else {
                path
            }
        };
        if !self.paths.contains(&path) {
            failures.push(format!("{page}: link {link:?} has no served target"));
        } else if !fragment.is_empty()
            && !self.text.get(&path).is_some_and(|html| {
                tags(html).any(|tag| {
                    attribute(tag, "id") == Some(fragment)
                        || (tag.starts_with("a ") && attribute(tag, "name") == Some(fragment))
                })
            })
        {
            failures.push(format!("{page}: link {link:?} has no served anchor"));
        }
    }

    pub(super) fn check(&self, failures: &mut Vec<String>) {
        let index_path = format!("/{FILE}");
        let index_count = match self.text.get(&index_path).map(|index| index_links(index)) {
            Some(Ok(links)) => {
                for link in &links {
                    self.check_link(&index_path, link, failures);
                }
                links.len()
            }
            Some(Err(error)) => {
                failures.push(format!("{index_path}: {error}"));
                0
            }
            None => 0, // Missing or unreadable responses are reported by the manifest checks.
        };

        let mut editions = 0;
        let mut pages = 0;
        for (path, text) in &self.text {
            if let Some(stem) = path.strip_suffix(".md") {
                editions += 1;
                let html = if path == PREVIEW_NOTES.1 {
                    PREVIEW_NOTES.0.to_owned()
                } else {
                    format!("{stem}.html")
                };
                if !self.paths.contains(&html) {
                    failures.push(format!("{path}: no served HTML edition {html}"));
                }
                for event in Parser::new_ext(text, Options::ENABLE_TABLES) {
                    if let Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) =
                        event
                    {
                        self.check_link(path, &dest_url, failures);
                    }
                }
            } else if let Some(stem) = path.strip_suffix(".html") {
                pages += 1;
                let edition = if path == PREVIEW_NOTES.0 {
                    PREVIEW_NOTES.1.to_owned()
                } else {
                    format!("{stem}.md")
                };
                let mut alternates = Vec::new();
                let mut described_by = Vec::new();
                for tag in tags(text).filter(|tag| tag.starts_with("link ")) {
                    match attribute(tag, "rel") {
                        Some("alternate") if attribute(tag, "type") == Some("text/markdown") => {
                            alternates.push(attribute(tag, "href"));
                        }
                        Some("describedby") => described_by.push(attribute(tag, "href")),
                        _ => {}
                    }
                }
                let expected = self.paths.contains(&edition);
                if (expected && alternates.as_slice() != [Some(edition.as_str())])
                    || (!expected && !alternates.is_empty())
                {
                    failures.push(format!("{path}: invalid Markdown alternate {alternates:?}"));
                }
                if described_by.as_slice() != [Some(index_path.as_str())] {
                    failures.push(format!("{path}: expected describedby link to {index_path}"));
                }
            }
        }
        if failures.is_empty() {
            println!(
                "SITE discovery: {index_count} index links, {editions} Markdown editions with alternates, {pages} HTML pages describe the index"
            );
        }
    }
}

// The generated site's tags use quoted attributes. Only link metadata and anchor names are read.
fn tags(html: &str) -> impl Iterator<Item = &str> {
    html.split('<').skip(1).filter_map(|part| {
        let tag = part.split_once('>')?.0;
        tag.starts_with(|c: char| c.is_ascii_alphabetic())
            .then_some(tag)
    })
}

fn attribute<'a>(tag: &'a str, key: &str) -> Option<&'a str> {
    for quote in ['"', '\''] {
        let pattern = format!(" {key}={quote}");
        if let Some((_, tail)) = tag.split_once(&pattern) {
            return tail.split_once(quote).map(|(value, _)| value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = "# DNA Calc

> A workbook and formula tool.

## Products

- [App](https://dnacalc.com/onecalc/#embedding): A preview.

## Developer guides

- [Engine](https://dnacalc.com/onecalc/): How to embed it.

## Handbook

- [ROUND](https://handbook.dnacalc.com/functions/ROUND.html): Examples and evidence.

## Evidence and articles

- [Guide](https://dnacalc.com/guide/index.md): A guide.
";

    #[test]
    fn discovery_checks_layout_links_anchors_and_alternates() {
        assert_eq!(index_links(INDEX).unwrap().len(), 4);
        for malformed in [
            INDEX.replace("## Handbook", "## Helping"),
            INDEX.replace("https://dnacalc.com/onecalc/", "/onecalc/"),
            INDEX.replace("https://dnacalc.com/onecalc/#embedding", "https://"),
            INDEX.replace(": A preview.", ": "),
            INDEX.replace("): A preview.", ") A preview."),
        ] {
            assert!(index_links(&malformed).is_err(), "{malformed}");
        }
        let guide = format!(
            r#"<body class="site"><link rel="alternate" type="text/markdown" href="/guide/index.md">{DESCRIBED_BY}"#
        );
        let fixture = [
            ("/llms.txt", INDEX),
            ("/onecalc/", r#"<h2 id="embedding">Embed</h2>"#),
            ("/guide/index.html", &guide),
            ("/guide/index.md", "[Embed](../onecalc/#embedding)"),
        ];
        let verify = |bad_path, replacement| {
            let mut served = Served::default();
            let mut failures = Vec::new();
            for (path, contents) in fixture {
                let contents = if path == bad_path {
                    replacement
                } else {
                    contents.as_bytes()
                };
                let name = if path == "/onecalc/" {
                    "onecalc/index.html"
                } else {
                    path.trim_start_matches('/')
                };
                served.remember(path, name, contents.to_vec(), &mut failures);
            }
            served.check(&mut failures);
            failures
        };
        assert!(verify("", b"").is_empty());
        assert!(
            verify("/llms.txt", b"\xff")
                .iter()
                .any(|failure| failure.contains("UTF-8"))
        );
        let missing = INDEX.replace("/onecalc/#embedding", "/onecalc/#missing");
        assert!(
            verify("/llms.txt", missing.as_bytes())
                .iter()
                .any(|failure| failure.contains("no served anchor"))
        );
        assert!(
            verify("/guide/index.md", b"[Missing](/missing/)")
                .iter()
                .any(|failure| failure.contains("no served target"))
        );
        let wrong = guide.replace("/guide/index.md", "/wrong.md");
        assert!(
            verify("/guide/index.html", wrong.as_bytes())
                .iter()
                .any(|failure| failure.contains("invalid Markdown alternate"))
        );
    }
}
