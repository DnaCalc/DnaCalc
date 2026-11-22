use super::content::{create_directory, escape, markdown, write_page};
use super::llms::{self, Index, Section};
use super::marketing::Layout;
use std::fmt::Write;
use std::fs;
use std::path::Path;

const FEED: &str =
    r#"<link rel="alternate" type="application/atom+xml" href="feed.atom" title="DNA Calc Blog">"#;

struct Post<'a> {
    date: &'a str,
    slug: &'a str,
    title: &'a str,
    tags: Vec<&'a str>,
    draft: bool,
    body: String,
    markdown_body: &'a str,
}

impl<'a> Post<'a> {
    fn parse(stem: &'a str, source: &'a str) -> Result<Self, String> {
        let invalid = || format!("{stem}.md needs a date, slug, title, tags and status");
        let date = stem
            .get(..10)
            .filter(|s| valid_date(s))
            .ok_or_else(invalid)?;
        let slug = stem
            .get(10..)
            .and_then(|s| s.strip_prefix('-'))
            .filter(|s| {
                !s.is_empty()
                    && s.bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            })
            .ok_or_else(invalid)?;
        let mut lines = source.lines();
        let title = lines
            .next()
            .and_then(|s| s.strip_prefix("# "))
            .filter(|s| !s.is_empty())
            .ok_or_else(invalid)?;
        let tags: Vec<_> = lines
            .next()
            .and_then(|s| s.strip_prefix("Tags: "))
            .ok_or_else(invalid)?
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let draft = match lines.next() {
            Some("Status: draft") => true,
            Some("Status: published") => false,
            _ => return Err(invalid()),
        };
        if tags.is_empty() || lines.next() != Some("") {
            return Err(invalid());
        }
        let markdown_body = source.splitn(5, '\n').nth(4).unwrap_or_default();
        let body = markdown(markdown_body);
        Ok(Self {
            date,
            slug,
            title,
            tags,
            draft,
            body,
            markdown_body,
        })
    }

    fn filename(&self) -> String {
        format!("{}-{}.html", self.date, self.slug)
    }

    fn description(&self) -> String {
        let tags = self
            .tags
            .iter()
            .map(|tag| escape(tag))
            .collect::<Vec<_>>()
            .join(", ");
        let draft = if self.draft { " · Draft" } else { "" };
        format!(
            "<time datetime=\"{date}\">{date}</time> · {tags}{draft}",
            date = self.date
        )
    }
}

fn valid_date(date: &str) -> bool {
    let parts: Vec<_> = date.split('-').collect();
    let [year, month, day] = parts.as_slice() else {
        return false;
    };
    if (year.len(), month.len(), day.len()) != (4, 2, 2)
        || !parts
            .iter()
            .all(|part| part.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) =
        (year.parse::<u16>(), month.parse::<u8>(), day.parse::<u8>())
    else {
        return false;
    };
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return false,
    };
    year > 0 && (1..=days).contains(&day)
}

fn feed(posts: &[Post<'_>], host: &str) -> String {
    let base = format!("https://{host}/blog");
    let updated = posts
        .first()
        .map(|post| format!("{}T00:00:00Z", post.date))
        .unwrap_or_else(crate::accept::utc_now);
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<feed xmlns=\"http://www.w3.org/2005/Atom\">\n\
         <title>DNA Calc Blog</title><id>{base}/</id><author><name>DNA Calc</name></author>\n\
         <link rel=\"self\" href=\"feed.atom\"/><link href=\"index.html\"/>\n\
         <updated>{updated}</updated>\n"
    );
    for post in posts {
        let filename = post.filename();
        let url = format!("{base}/{filename}");
        let title = escape(post.title);
        let date = post.date;
        let content = escape(&post.body);
        let _ = writeln!(
            out,
            "<entry><title>{title}</title><id>{url}</id><link href=\"{filename}\"/>\n\
             <published>{date}T00:00:00Z</published><updated>{date}T00:00:00Z</updated>"
        );
        for tag in &post.tags {
            let _ = writeln!(out, "<category term=\"{}\"/>", escape(tag));
        }
        let _ = writeln!(out, "<content type=\"html\">{content}</content></entry>");
    }
    out.push_str("</feed>\n");
    out
}

pub(super) fn build(
    root: &Path,
    output: &Path,
    host: &str,
    include_drafts: bool,
    mut content: Option<(&Layout<'_>, &mut Index)>,
) -> Result<usize, String> {
    let layout = content.as_ref().map(|(layout, _)| *layout);
    let sources = root.join("docs/site/blog");
    let mut files = Vec::new();
    for entry in fs::read_dir(&sources).map_err(|e| format!("{}: {e}", sources.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|extension| extension == "md") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("blog filename is not UTF-8")?;
            let source =
                fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            files.push((stem.to_owned(), source));
        }
    }
    let mut posts = Vec::new();
    for (stem, source) in &files {
        let post = Post::parse(stem, source)?;
        if include_drafts || !post.draft {
            posts.push(post);
        }
    }
    if posts.is_empty() && layout.is_none() {
        return Ok(0);
    }
    posts.sort_unstable_by_key(|post| std::cmp::Reverse((post.date, post.slug)));
    let directory = output.join("blog");
    let page = |filename: &str, title: &str, contents: &str, alternate: &str| {
        if let Some(layout) = layout {
            super::write_nested(
                &directory.join(filename),
                layout
                    .page("/blog/", title, &format!("{FEED}{alternate}"), contents)
                    .as_bytes(),
            )
        } else {
            write_page(&directory, filename, title, FEED, contents)
        }
    };
    if layout.is_none() {
        create_directory(&directory)?;
    }
    let mut index =
        String::from("<article><h1>DNA Calc Blog</h1><p><a href=\"feed.atom\">Atom feed</a></p>");
    if layout.is_some() {
        index.push_str(r#"<p><a href="/articles/">Last-bit articles</a></p>"#);
        if include_drafts {
            index.push_str(r#"<p><a href="/blog/video/">Video drafts</a></p>"#);
        }
    }
    if posts.is_empty() {
        index.push_str("<p>Posts appear here after editorial review.</p>");
    }
    index.push_str("<ul>");
    for post in &posts {
        let title = escape(post.title);
        let description = post.description();
        let filename = post.filename();
        let _ = writeln!(
            index,
            "<li><a href=\"{filename}\">{title}</a><br><small>{description}</small></li>"
        );
        let body = &post.body;
        let contents = format!(
            "<nav><a href=\"index.html\">Blog</a></nav><article><h1>{title}</h1><p>{description}</p>{body}</article>"
        );
        let mut alternate = String::new();
        if !post.draft
            && let Some((_, discovery)) = &mut content
        {
            let name = format!("{}-{}.md", post.date, post.slug);
            let path = format!("/blog/{name}");
            let tags = post.tags.join(", ");
            let edition = format!(
                "# {}\n\n{} · {tags}\n\n{}",
                post.title, post.date, post.markdown_body
            );
            alternate = llms::write_edition(output, &path, &edition)?;
            let description = format!("Published {}. Topics: {tags}.", post.date);
            discovery.add(Section::Evidence, post.title, &path, &description);
        }
        page(&filename, post.title, &contents, &alternate)?;
    }
    index.push_str("</ul></article>");
    page("index.html", "Blog", &index, "")?;
    super::write(&directory.join("feed.atom"), feed(&posts, host).as_bytes())?;
    Ok(posts.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_metadata_and_atom_preserve_special_characters() {
        let source = "# A & B\nTags: <test>, grid\nStatus: draft\n\nWork with **cells**.";
        let post = Post::parse("2028-02-29-test", source).unwrap();
        assert!(Post::parse("2026-02-29-test", source).is_err());
        assert!(post.draft);
        assert_eq!(post.filename(), "2028-02-29-test.html");
        let atom = feed(&[post], "dnacalc.com");
        assert!(atom.contains("<title>A &amp; B</title>"));
        assert!(atom.contains("<category term=\"&lt;test&gt;\"/>"));
        assert!(atom.contains("&lt;strong&gt;cells&lt;/strong&gt;"));
    }
}
