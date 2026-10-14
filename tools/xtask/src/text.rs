//! Splitting text into lower-case word pieces and finding word sequences among them. The
//! tree-reference token check and the word scan both match through these functions.

/// One maximal run of ASCII letters and digits, or one camel-case part of such a run, with its
/// byte range in the text it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub lower: String,
    pub start: usize,
    pub end: usize,
}

/// The pieces of `text`: maximal runs of ASCII letters and digits, each split further at
/// camel-case boundaries when `camel` is set (`TreeRef` gives `tree` and `ref`, `XMLTree` gives
/// `xml` and `tree`, `IDs` stays one piece).
pub fn pieces(text: &str, camel: bool) -> Vec<Piece> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_alphanumeric() {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
            i += 1;
        }
        if camel {
            split_camel(&text[start..i], start, &mut out);
        } else {
            out.push(Piece {
                lower: text[start..i].to_ascii_lowercase(),
                start,
                end: i,
            });
        }
    }
    out
}

fn split_camel(run: &str, offset: usize, out: &mut Vec<Piece>) {
    let b = run.as_bytes();
    let mut start = 0;
    for i in 1..b.len() {
        let (prev, cur, next) = (b[i - 1], b[i], b.get(i + 1).copied());
        let lower_to_upper =
            (prev.is_ascii_lowercase() || prev.is_ascii_digit()) && cur.is_ascii_uppercase();
        // The last capital of a capital run starts a new word when a lower-case letter follows,
        // except for a plural acronym such as `IDs` at the end of the run.
        let acronym_end = prev.is_ascii_uppercase()
            && cur.is_ascii_uppercase()
            && next.is_some_and(|n| n.is_ascii_lowercase())
            && !(next == Some(b's') && i + 2 == b.len());
        if lower_to_upper || acronym_end {
            out.push(Piece {
                lower: run[start..i].to_ascii_lowercase(),
                start: offset + start,
                end: offset + i,
            });
            start = i;
        }
    }
    out.push(Piece {
        lower: run[start..].to_ascii_lowercase(),
        start: offset + start,
        end: offset + b.len(),
    });
}

/// Whether the text between two pieces joins them into one phrase: nothing at all (a camel-case
/// boundary) or only spaces, tabs, line breaks, hyphens and underscores.
fn joins(separator: &str) -> bool {
    separator
        .chars()
        .all(|c| c.is_ascii_whitespace() || c == '-' || c == '_')
}

/// The byte ranges at which `words` occur in `text` as consecutive pieces joined only by
/// whitespace, hyphens, underscores or a camel-case boundary. With `plural`, the last word may
/// also carry a trailing `s`.
pub fn find_words(
    text: &str,
    pieces: &[Piece],
    words: &[String],
    plural: bool,
) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    if words.is_empty() || pieces.len() < words.len() {
        return found;
    }
    for first in 0..=pieces.len() - words.len() {
        let window = &pieces[first..first + words.len()];
        let matches = window
            .iter()
            .zip(words)
            .enumerate()
            .all(|(k, (piece, word))| {
                piece.lower == *word
                    || (plural
                        && k + 1 == words.len()
                        && piece.lower.strip_suffix('s') == Some(word.as_str()))
            });
        let joined = window
            .windows(2)
            .all(|pair| joins(&text[pair[0].end..pair[1].start]));
        if matches && joined {
            found.push((window[0].start, window[words.len() - 1].end));
        }
    }
    found
}

/// The lower-case words of a phrase as the camel-aware splitter sees them (`TreeRef`, `tree_ref`
/// and `tree ref` all give `tree`, `ref`).
pub fn phrase_words(phrase: &str) -> Vec<String> {
    pieces(phrase, true)
        .into_iter()
        .map(|piece| piece.lower)
        .collect()
}

/// `text` with every occurrence of each identifier replaced by spaces, where an occurrence must
/// not be preceded or followed by an ASCII letter, digit, `_` or `-`, so `a_b` does not hide
/// `xa_b` or `a_bc`; with the number of occurrences masked for each identifier, in order.
pub fn mask_identifiers(text: &str, identifiers: &[String]) -> (String, Vec<usize>) {
    let mut bytes = text.as_bytes().to_vec();
    let mut masked = vec![0; identifiers.len()];
    let is_identifier_byte = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'-';
    for (index, identifier) in identifiers.iter().enumerate() {
        if identifier.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(found) = text[from..].find(identifier.as_str()) {
            let start = from + found;
            let end = start + identifier.len();
            let before_ok = start == 0 || !is_identifier_byte(text.as_bytes()[start - 1]);
            let after_ok = end == text.len() || !is_identifier_byte(text.as_bytes()[end]);
            if before_ok && after_ok {
                masked[index] += 1;
                for byte in &mut bytes[start..end] {
                    *byte = b' ';
                }
            }
            from = start + 1;
        }
    }
    // Only ASCII bytes inside whole identifiers were replaced, so the text stays UTF-8.
    let text = String::from_utf8(bytes).expect("masking replaces whole ASCII identifiers");
    (text, masked)
}

/// The 1-based line on which byte `offset` of `text` lies.
pub fn line_at(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lowers(text: &str, camel: bool) -> Vec<String> {
        pieces(text, camel).into_iter().map(|p| p.lower).collect()
    }

    #[test]
    fn camel_pieces() {
        assert_eq!(lowers("TreeRef", true), ["tree", "ref"]);
        assert_eq!(lowers("XMLTreeRef", true), ["xml", "tree", "ref"]);
        assert_eq!(lowers("profile IDs", true), ["profile", "ids"]);
        assert_eq!(
            lowers("TREE_REF tree-ref", true),
            ["tree", "ref", "tree", "ref"]
        );
        assert_eq!(lowers("TreeRef", false), ["treeref"]);
        assert_eq!(
            lowers("ReferenceSystemProvider", true),
            ["reference", "system", "provider"]
        );
    }

    #[test]
    fn word_sequences_need_joining_separators() {
        let words = phrase_words("tree reference");
        for text in [
            "tree reference",
            "Tree-Reference",
            "TREE_REFERENCE",
            "treeReference",
        ] {
            let p = pieces(text, true);
            assert_eq!(find_words(text, &p, &words, false).len(), 1, "{text}");
        }
        for text in [
            "tree.reference",
            "street reference",
            "tree referenced",
            "tree/reference",
        ] {
            let p = pieces(text, true);
            assert!(find_words(text, &p, &words, false).is_empty(), "{text}");
        }
        let text = "host references";
        let p = pieces(text, true);
        assert!(find_words(text, &p, &phrase_words("host reference"), false).is_empty());
        assert_eq!(
            find_words(text, &p, &phrase_words("host reference"), true),
            [(0, 15)]
        );
    }

    #[test]
    fn identifiers_are_masked_only_as_whole_identifiers() {
        let ids = ["a_b".to_owned(), "c-d".to_owned()];
        assert_eq!(
            mask_identifiers("x a_b y", &ids),
            ("x     y".to_owned(), vec![1, 0])
        );
        assert_eq!(
            mask_identifiers("xa_b a_bc a_b. c-d c-de", &ids),
            ("xa_b a_bc    .     c-de".to_owned(), vec![1, 1])
        );
    }
}
