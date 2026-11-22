//! Static HTML shared by the blog and download pages.

use pulldown_cmark::{Event, Options, Parser, Tag, html};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(super) const TABLE_ALIGNMENT: &str = "\n.markdown-align-left{text-align:left}
.markdown-align-center{text-align:center}.markdown-align-right{text-align:right}\n";

pub(super) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(super) fn markdown(text: &str) -> String {
    render_markdown(text, |_| None)
}

/// Rewrite the destinations of mapped links, leaving Markdown formatting intact.
pub(super) fn resolve_markdown(
    text: &str,
    site_link: impl Fn(&str) -> Option<String>,
) -> Result<String, String> {
    let mut edits = BTreeMap::new();
    let mut events = Parser::new_ext(text, Options::ENABLE_TABLES).into_offset_iter();
    while let Some((event, mut span)) = events.next() {
        let Event::Start(Tag::Link { dest_url, id, .. }) = event else {
            continue;
        };
        let Some(resolved) = site_link(&dest_url) else {
            continue;
        };
        let marker = if let Some(definition) = events.reference_definitions().get(&id) {
            span = definition.span.clone();
            "]:"
        } else {
            "]("
        };
        let raw = &text[span.clone()];
        let target = raw
            .match_indices(marker)
            .find_map(|(offset, _)| {
                let tail = &raw[offset + 2..];
                let leading = tail.len() - tail.trim_start().len();
                let tail = tail.trim_start();
                let start = offset + 2 + leading + usize::from(tail.starts_with('<'));
                let end = start + dest_url.len();
                (raw.get(start..end) == Some(dest_url.as_ref())
                    && (end == raw.len()
                        || raw[end..]
                            .starts_with(|c: char| c == ')' || c == '>' || c.is_whitespace())))
                .then_some(start..end)
            })
            .ok_or_else(|| format!("could not locate Markdown destination {dest_url}"))?;
        edits.insert(
            span.start + target.start,
            (span.start + target.end, resolved),
        );
    }
    let mut output = String::new();
    let mut copied = 0;
    for (start, (end, resolved)) in edits {
        output.push_str(&text[copied..start]);
        output.push_str(&resolved);
        copied = end;
    }
    output.push_str(&text[copied..]);
    Ok(output)
}

pub(super) fn render_markdown(text: &str, site_link: impl Fn(&str) -> Option<String>) -> String {
    let mut rendered = String::new();
    let events = Parser::new_ext(text, Options::ENABLE_TABLES).map(|event| match event {
        Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let dest_url = site_link(&dest_url).map_or(dest_url, Into::into);
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            })
        }
        other => other,
    });
    html::push_html(&mut rendered, events);
    // The renderer emits inline alignment for Markdown tables. Use the site's stylesheet.
    for alignment in ["left", "center", "right"] {
        for tag in ["th", "td"] {
            rendered = rendered.replace(
                &format!(r#"<{tag} style="text-align: {alignment}">"#),
                &format!(r#"<{tag} class="markdown-align-{alignment}">"#),
            );
        }
    }
    rendered
}

pub(super) fn page(title: &str, feed: &str, content: &str) -> String {
    let title = escape(title);
    format!(
        "<!doctype html>\n<html lang=\"en\"><meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n\
         <title>{title} | DNA Calc</title><link rel=\"stylesheet\" href=\"content.css\">{feed}\n\
         <main>{content}</main></html>\n"
    )
}

pub(super) fn write_page(
    directory: &Path,
    filename: &str,
    title: &str,
    feed: &str,
    content: &str,
) -> Result<(), String> {
    super::write(
        &directory.join(filename),
        page(title, feed, content).as_bytes(),
    )
}

pub(super) fn create_directory(directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory)
        .map_err(|e| format!("cannot create {}: {e}", directory.display()))?;
    if fs::read_dir(directory)
        .map_err(|e| e.to_string())?
        .next()
        .is_some()
    {
        return Err(format!(
            "use an empty output directory for {}",
            directory.display()
        ));
    }
    let css = format!(
        ":root{{color-scheme:light dark}}body{{margin:0;font:1rem/1.65 system-ui,sans-serif}}
main{{max-width:66ch;margin:3rem auto;padding:0 1.25rem}}h1,h2{{line-height:1.2}}
nav,small,time{{font-size:.9rem}}a{{text-underline-offset:.15em}}
code{{overflow-wrap:anywhere}}dt{{font-weight:600}}dd{{margin:0 0 1.5rem}}
{TABLE_ALIGNMENT}"
    );
    super::write(&directory.join("content.css"), css.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_links_keep_inline_and_reference_markdown() {
        let source =
            "[One][same] [Two](<chapter.md> \"Title\") [Three][same]\n\n[same]: chapter.md";
        let output = resolve_markdown(source, |link| {
            (link == "chapter.md").then(|| "/chapters/".to_owned())
        })
        .unwrap();
        assert_eq!(output, source.replace("chapter.md", "/chapters/"));
        assert_eq!(markdown(&output).matches("href=\"/chapters/\"").count(), 3);
    }
}
