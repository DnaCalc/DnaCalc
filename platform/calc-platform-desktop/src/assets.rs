//! A page's files in one block of bytes, to embed in an executable.
//!
//! `cargo xtask package onecalc` packs the built page (HTML, script, style, wasm) with [`pack`];
//! the executable carries the block and [`Assets::parse`]s it at start, and the WebView2 host
//! answers the page's requests from it ([`crate::WebView::serve_assets`]). The format is the
//! simplest that holds: a magic, a count, then for each file its name, its size and its bytes.
//! There is no compression and no path rule beyond the name: the names are the page's, flat.
//!
//! Plain `std`, so the packer (a native tool) and the reader (the executable) share it.

use std::collections::BTreeMap;

const MAGIC: &[u8; 8] = b"DNAPAGE1";

/// The block holding `files` (name, bytes), in the order given.
pub fn pack(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        MAGIC.len() + 4 + files.iter().map(|(n, b)| n.len() + b.len() + 10).sum::<usize>(),
    );
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(files.len() as u32).to_le_bytes());
    for (name, bytes) in files {
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out
}

/// The files of a packed block, by name.
#[derive(Debug, Default)]
pub struct Assets {
    files: BTreeMap<String, (usize, usize)>,
    block: Vec<u8>,
}

impl Assets {
    /// Reads the block [`pack`] wrote. A block of any other shape is an error.
    pub fn parse(block: &[u8]) -> Result<Self, String> {
        let bad = || "the embedded page is damaged".to_owned();
        let rest = block.strip_prefix(MAGIC.as_slice()).ok_or_else(bad)?;
        let count = u32::from_le_bytes(rest.get(..4).ok_or_else(bad)?.try_into().unwrap());
        let mut at = MAGIC.len() + 4;
        let mut files = BTreeMap::new();
        for _ in 0..count {
            let name_len =
                u16::from_le_bytes(block.get(at..at + 2).ok_or_else(bad)?.try_into().unwrap());
            at += 2;
            let name = std::str::from_utf8(block.get(at..at + name_len as usize).ok_or_else(bad)?)
                .map_err(|_| bad())?
                .to_owned();
            at += name_len as usize;
            let size =
                u64::from_le_bytes(block.get(at..at + 8).ok_or_else(bad)?.try_into().unwrap())
                    as usize;
            at += 8;
            block.get(at..at.checked_add(size).ok_or_else(bad)?).ok_or_else(bad)?;
            files.insert(name, (at, size));
            at += size;
        }
        if at != block.len() {
            return Err(bad());
        }
        Ok(Self {
            files,
            block: block.to_vec(),
        })
    }

    /// The bytes of the file called `name`.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files
            .get(name)
            .map(|&(at, size)| &self.block[at..at + size])
    }

    /// The names of the files, in name order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
}

/// The media type a page file is served with.
pub fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// The file name a request path asks for: no query, no fragment, `/` is the index page.
/// `None` for a name that climbs out or holds a separator (the block is flat).
pub fn requested_name(path: &str) -> Option<String> {
    let path = path.split(['?', '#']).next().unwrap_or("");
    let name = path.trim_start_matches('/');
    let name = if name.is_empty() { "index.html" } else { name };
    if name.contains(['/', '\\']) || name == ".." || name.contains("..") {
        return None;
    }
    Some(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native-only: the block round-trips every file byte for byte, and a damaged block is refused.
    #[test]
    fn a_packed_page_reads_back_and_a_damaged_one_is_refused() {
        let files = vec![
            ("index.html".to_owned(), b"<html></html>".to_vec()),
            ("app.wasm".to_owned(), (0u32..70_000).map(|i| (i % 251) as u8).collect()),
            ("empty.txt".to_owned(), Vec::new()),
        ];
        let block = pack(&files);
        let assets = Assets::parse(&block).unwrap();
        for (name, bytes) in &files {
            assert_eq!(assets.get(name), Some(bytes.as_slice()), "{name}");
        }
        assert_eq!(assets.get("missing"), None);
        assert_eq!(assets.names().count(), 3);
        assert!(Assets::parse(&block[..block.len() - 1]).is_err());
        assert!(Assets::parse(b"nonsense").is_err());
        let mut extra = block.clone();
        extra.push(0);
        assert!(Assets::parse(&extra).is_err());
        assert!(Assets::parse(&[]).is_err());
    }

    /// Native-only: request paths map to flat names, and nothing climbs out of the block.
    #[test]
    fn request_paths_become_flat_names() {
        assert_eq!(requested_name("/").as_deref(), Some("index.html"));
        assert_eq!(requested_name("/index.html?x=1").as_deref(), Some("index.html"));
        assert_eq!(requested_name("/dna-onecalc-web_bg.wasm#a").as_deref(), Some("dna-onecalc-web_bg.wasm"));
        assert_eq!(requested_name("/a/b.js"), None);
        assert_eq!(requested_name("/../x"), None);
        assert_eq!(requested_name("/a\\b"), None);
        assert_eq!(content_type("x.wasm"), "application/wasm");
        assert_eq!(content_type("boot.js"), "text/javascript; charset=utf-8");
        assert_eq!(content_type("noext"), "application/octet-stream");
    }
}
