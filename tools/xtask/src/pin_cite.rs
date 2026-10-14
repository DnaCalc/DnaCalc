//! Reading one bead's Port from section: the legacy paths it cites, the symbols it names for each,
//! and what the text says may be done with each path.
//!
//! The rules are mechanical and are restated in the manifest's `rules` (`pin.rs`):
//!
//! - A path token is a run of ASCII letters, digits, `_`, `.`, `-` and `/` that ends in `/` (a
//!   directory) or whose last component has one of [`PATH_EXTENSIONS`]; trailing `.` and `,` are
//!   punctuation. A token whose first component is a legacy repository name is anchored there; a
//!   token naming only a repository (`OxFunc/`) is not a path. Any other token is relative and is
//!   resolved later against the previous anchored token of the same text (`pin.rs`).
//! - The clause of a token runs from its end to the next token or the first sentence end (`.`
//!   followed by white space and an upper-case letter, or the end of the text), whichever comes
//!   first. The trailing text is everything after the first sentence end that follows the last
//!   token; it applies to every token of the text.
//! - Disposition: `quarantined` when the clause holds `QUARANTINED`; `reference` when the token's
//!   own sentence names it a [`DISCOVERY_MARKER`] (a source-discovery path, read to find the files
//!   a later bead selects, never ported whole) or when the clause or the trailing text holds one of
//!   [`REFERENCE_MARKERS`] (case-insensitive); otherwise `port`. A token whose own sentence says
//!   `not port` before it is excluded, not an input. The generator (`pin.rs`) adds the dispositions
//!   that depend on the pinned content: `quarantined` for paths under the quarantined numeric
//!   directory and `quarantine_derived` for research-x87 code.
//! - Symbols: code-like words of the clause and the trailing text (holding `::` or `_`, or
//!   camel-case), and capitalised single words listed directly after `<path>:`; repository and
//!   library names are not symbols.

use std::collections::BTreeSet;

/// The legacy repositories under the legacy root, in the order the manifest lists them. DnaOneCalc
/// is the archived one; OxVba is cited by the Port from text of three beads.
pub const REPOSITORIES: [&str; 12] = [
    "OxFunc",
    "OxFml",
    "OxCalc",
    "OxDoc",
    "DnaTreeCalc",
    "DnaOneCalc",
    "OxReplay",
    "OxXlPlay",
    "ExcelFunctionsHandbook",
    "DnaVisiCalc",
    "Foundation",
    "OxVba",
];

/// The extensions that make a token without a trailing `/` a file path.
pub const PATH_EXTENSIONS: [&str; 22] = [
    "rs", "md", "py", "toml", "json", "jsonl", "tsv", "csv", "txt", "ps1", "psm1", "cs", "csproj",
    "sln", "xml", "yaml", "yml", "sh", "js", "ts", "html", "xlsx",
];

/// Text that says a path is read for behaviour only: a behavioural reference, ideas, a precedent,
/// lessons, a discovery location, or a design candidate that is not an authority.
pub const REFERENCE_MARKERS: [&str; 10] = [
    "behavioural reference",
    "reference only",
    "references only",
    "owner-brief reference",
    "precedent",
    "ideas",
    "lessons",
    "discovery location",
    "design candidate",
    "not authority",
];

/// What a token's own sentence says when the path is a source-discovery path: the place where a
/// later bead finds and selects files (the capture lineage), read for behaviour only. The
/// generator puts its files in the oracle-runner category and marks the ones that drive Excel as
/// runner candidates.
pub const DISCOVERY_MARKER: &str = "source-discovery path";

/// The clause word that quarantines a path (written in capitals in the tracker).
pub const QUARANTINE_MARKER: &str = "QUARANTINED";

/// What a token's own sentence says before it when the path must not be ported.
pub const EXCLUDE_MARKER: &str = "not port";

/// Camel-case names that are product, library or application names rather than symbols.
const NOT_SYMBOLS: [&str; 7] = [
    "CalcFunc",
    "CalcFml",
    "CalcEngine",
    "CalcDoc",
    "CalcHost",
    "OneCalc",
    "DnaCalc",
];

/// What may be done with an input, from least to most restrictive: a bead that reaches one input
/// twice keeps the more restrictive reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Disposition {
    /// May be copied after review.
    Port,
    /// Read for behaviour only.
    Reference,
    /// Code built on the quarantined x87 research surface (`pin.rs` assigns it); not a port
    /// source until infra-m0.5 gives it a disposition.
    QuarantineDerived,
    /// Not a port source pending infra-m0.5.
    Quarantined,
}

impl Disposition {
    pub fn as_str(self) -> &'static str {
        match self {
            Disposition::Port => "port",
            Disposition::Reference => "reference",
            Disposition::QuarantineDerived => "quarantine_derived",
            Disposition::Quarantined => "quarantined",
        }
    }
}

/// One path token of a Port from text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cited {
    /// The token as written.
    pub token: String,
    /// The repository the token names, or `None` for a relative token.
    pub repository: Option<String>,
    /// The token without its repository component; for a directory it ends in `/`.
    pub path: String,
    pub dir: bool,
    /// The token's own sentence says not to port it.
    pub excluded: bool,
    /// The token's own sentence names it a source-discovery path ([`DISCOVERY_MARKER`]).
    pub discovery: bool,
    pub disposition: Disposition,
    /// The symbols named for this token: its clause's and the trailing text's.
    pub symbols: BTreeSet<String>,
    /// The clause holds `catalogue entries`: the bead resolves catalogue entries of this file.
    pub catalogue: bool,
}

/// What one Port from text cites.
#[derive(Debug, Default)]
pub struct PortFrom {
    pub cited: Vec<Cited>,
    /// The text asks for symbols to be resolved (it holds the word `symbols`).
    pub wants_symbols: bool,
    /// The text asks for test files to be resolved (it holds `test files`).
    pub wants_tests: bool,
}

fn is_path_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-' | b'/')
}

/// The byte ranges of the path tokens of `text`, with whether each is a directory.
fn path_tokens(text: &str) -> Vec<(usize, usize, bool)> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if !is_path_byte(bytes[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && is_path_byte(bytes[i]) {
            i += 1;
        }
        let mut end = i;
        while end > start && matches!(bytes[end - 1], b'.' | b',') {
            end -= 1;
        }
        let run = &text[start..end];
        if run.len() < 2 || run.starts_with(['.', '-', '/']) {
            continue;
        }
        if run.ends_with('/') {
            if run.bytes().any(|b| b.is_ascii_alphabetic()) && !run.contains("//") {
                tokens.push((start, end, true));
            }
            continue;
        }
        let last = run.rsplit('/').next().unwrap_or(run);
        if let Some((stem, extension)) = last.rsplit_once('.')
            && !stem.is_empty()
            && PATH_EXTENSIONS.contains(&extension)
        {
            tokens.push((start, end, false));
        }
    }
    tokens
}

/// The byte offset just past the first sentence end in `text[from..]`, or `None`.
fn sentence_end(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'.' {
            let mut j = i + 1;
            if j == bytes.len() {
                return Some(j);
            }
            if bytes[j].is_ascii_whitespace() {
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j == bytes.len() || bytes[j].is_ascii_uppercase() {
                    return Some(i + 1);
                }
            }
        }
        i += 1;
    }
    None
}

/// The start of the sentence that contains offset `at`.
fn sentence_start(text: &str, at: usize) -> usize {
    let mut start = 0;
    let mut from = 0;
    while let Some(end) = sentence_end(text, from) {
        if end > at {
            break;
        }
        start = end;
        from = end;
    }
    start
}

fn is_camel(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_uppercase)
        && bytes.iter().any(u8::is_ascii_lowercase)
        && bytes.windows(2).any(|pair| {
            (pair[0].is_ascii_lowercase() || pair[0].is_ascii_digit())
                && pair[1].is_ascii_uppercase()
        })
}

fn is_identifier(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes
        .first()
        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

fn is_named_thing(word: &str) -> bool {
    REPOSITORIES.contains(&word) || NOT_SYMBOLS.contains(&word)
}

/// The code-like words of `text`: runs of identifiers joined by `::` that hold `::` or `_` (and a
/// letter), or are camel-case.
fn code_words(text: &str, out: &mut BTreeSet<String>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let starts = (b.is_ascii_alphabetic() || b == b'_')
            && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_'));
        if !starts {
            i += 1;
            continue;
        }
        let start = i;
        loop {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let joined = bytes.get(i) == Some(&b':')
                && bytes.get(i + 1) == Some(&b':')
                && bytes
                    .get(i + 2)
                    .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_');
            if joined {
                i += 2;
            } else {
                break;
            }
        }
        let word = &text[start..i];
        let followed_by_path = matches!(bytes.get(i), Some(b'/') | Some(b'.'))
            && bytes.get(i + 1).is_some_and(u8::is_ascii_alphanumeric);
        if followed_by_path || is_named_thing(word) {
            continue;
        }
        let code_like = word.contains("::")
            || (word.contains('_') && word.bytes().any(|c| c.is_ascii_alphabetic()))
            || is_camel(word);
        if code_like {
            out.insert(word.to_owned());
        }
    }
}

/// Capitalised single identifiers listed directly after `<path>:`, up to the first `;` or sentence
/// end of the clause (`: CalcValue, CoreValue, Shared and WorksheetErrorCode`).
fn listed_words(clause: &str, out: &mut BTreeSet<String>) {
    let Some(list) = clause.trim_start().strip_prefix(':') else {
        return;
    };
    let list = list.split(';').next().unwrap_or_default();
    let list = match sentence_end(list, 0) {
        Some(end) => &list[..end - 1],
        None => list,
    };
    for item in list.split(',').flat_map(|part| part.split(" and ")) {
        let item = item.trim();
        if is_identifier(item)
            && item.starts_with(|c: char| c.is_ascii_uppercase())
            && !is_named_thing(item)
        {
            out.insert(item.to_owned());
        }
    }
}

/// `text` with the byte ranges in `cut` blanked out, so path tokens are not read as symbols.
fn without(text: &str, cut: &[(usize, usize)], offset: usize) -> String {
    let mut bytes = text.as_bytes().to_vec();
    for &(start, end) in cut {
        let (start, end) = (start.saturating_sub(offset), end.saturating_sub(offset));
        for b in bytes.iter_mut().take(end.min(text.len())).skip(start) {
            *b = b' ';
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Reads one Port from text.
pub fn parse(text: &str) -> PortFrom {
    let tokens: Vec<(usize, usize, bool)> = path_tokens(text)
        .into_iter()
        .filter(|&(start, end, _)| {
            let token = &text[start..end];
            !REPOSITORIES
                .iter()
                .any(|repository| token.strip_prefix(repository) == Some("/"))
        })
        .collect();
    let ranges: Vec<(usize, usize)> = tokens.iter().map(|&(s, e, _)| (s, e)).collect();
    let lower = text.to_ascii_lowercase();
    let mut port_from = PortFrom {
        wants_symbols: lower.contains("symbols"),
        wants_tests: lower.contains("test files"),
        ..PortFrom::default()
    };
    let trailing = match tokens.last() {
        Some(&(_, last_end, _)) => match sentence_end(text, last_end) {
            Some(end) => &text[end..],
            None => "",
        },
        None => "",
    };
    let trailing_offset = text.len() - trailing.len();
    let mut trailing_symbols = BTreeSet::new();
    code_words(
        &without(trailing, &ranges, trailing_offset),
        &mut trailing_symbols,
    );
    let trailing_lower = trailing.to_ascii_lowercase();
    for (index, &(start, end, dir)) in tokens.iter().enumerate() {
        let token = &text[start..end];
        let next = tokens.get(index + 1).map_or(text.len(), |t| t.0);
        let clause_end = sentence_end(text, end).map_or(next, |e| e.min(next));
        let clause = &text[end..clause_end.max(end)];
        let clause_lower = clause.to_ascii_lowercase();
        let own_start = sentence_start(text, start);
        let before = &lower[own_start..start];
        let own_end = sentence_end(text, end).unwrap_or(text.len());
        let discovery = lower[own_start..own_end].contains(DISCOVERY_MARKER);
        let disposition = if clause.contains(QUARANTINE_MARKER) {
            Disposition::Quarantined
        } else if discovery
            || REFERENCE_MARKERS
                .iter()
                .any(|marker| clause_lower.contains(marker) || trailing_lower.contains(marker))
        {
            Disposition::Reference
        } else {
            Disposition::Port
        };
        let mut symbols = trailing_symbols.clone();
        code_words(&without(clause, &ranges, end), &mut symbols);
        listed_words(clause, &mut symbols);
        let (repository, path) = match token.split_once('/') {
            Some((first, rest)) if REPOSITORIES.contains(&first) => {
                (Some(first.to_owned()), rest.to_owned())
            }
            _ => (None, token.to_owned()),
        };
        port_from.cited.push(Cited {
            token: token.to_owned(),
            repository,
            path,
            dir,
            excluded: before.contains(EXCLUDE_MARKER),
            discovery,
            disposition,
            symbols,
            catalogue: clause_lower.contains("catalogue entries"),
        });
    }
    port_from
}

/// The text of the `## <name>` section of a bead description, without its heading.
pub fn section<'a>(description: &'a str, name: &str) -> Option<&'a str> {
    let heading = format!("## {name}\n");
    let start = if description.starts_with(&heading) {
        heading.len()
    } else {
        description.find(&format!("\n{heading}"))? + heading.len() + 1
    };
    let rest = &description[start..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    Some(rest[..end].trim())
}

/// The items of the list a family bead's Do names after `Port the named family only:`, up to the
/// first sentence end, split at commas and ` and `.
pub fn family_items(do_text: &str) -> Option<Vec<String>> {
    const LEAD: &str = "Port the named family only:";
    let start = do_text.find(LEAD)? + LEAD.len();
    let rest = &do_text[start..];
    let end = sentence_end(rest, 0).map_or(rest.len(), |e| e - 1);
    let list = rest[..end].split(';').next().unwrap_or_default();
    Some(
        list.split(',')
            .flat_map(|part| part.split(" and "))
            .map(|item| item.trim().to_owned())
            .filter(|item| !item.is_empty())
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cited(text: &str) -> Vec<Cited> {
        parse(text).cited
    }

    #[test]
    fn anchored_relative_and_excluded_paths_are_read() {
        let text = "OxCalc/src/oxcalc-core/src/grid/machine/differential.rs: GridSeededLaneOutcome; \
            OxCalc/src/oxcalc-core/src/grid/authored.rs: GridInputSnapshotId. Do NOT port consumer.rs \
            or grid/machine/r1c1_plan.rs.";
        let found = cited(text);
        let shown: Vec<(&str, Option<&str>, bool)> = found
            .iter()
            .map(|c| (c.path.as_str(), c.repository.as_deref(), c.excluded))
            .collect();
        assert_eq!(
            shown,
            [
                (
                    "src/oxcalc-core/src/grid/machine/differential.rs",
                    Some("OxCalc"),
                    false
                ),
                (
                    "src/oxcalc-core/src/grid/authored.rs",
                    Some("OxCalc"),
                    false
                ),
                ("consumer.rs", None, true),
                ("grid/machine/r1c1_plan.rs", None, true),
            ]
        );
        assert_eq!(
            found[0].symbols.iter().collect::<Vec<_>>(),
            ["GridSeededLaneOutcome"]
        );
        assert_eq!(
            found[1].symbols.iter().collect::<Vec<_>>(),
            ["GridInputSnapshotId"]
        );
        assert!(found.iter().all(|c| c.disposition == Disposition::Port));
    }

    #[test]
    fn repository_roots_and_non_paths_are_not_tokens() {
        let text = "OxFunc/crates/oxfunc_core/src/function.rs: FunctionMeta; \
            crates/oxfunc_core/src/registry.rs. Paths after the first are relative to OxFunc/. \
            Not paths: Rc/RefCell, IFS/CHOOSE, F.2/F.4, fml-kt8.1, public-ISA/black-box.";
        let found = cited(text);
        let tokens: Vec<&str> = found.iter().map(|c| c.token.as_str()).collect();
        assert_eq!(
            tokens,
            [
                "OxFunc/crates/oxfunc_core/src/function.rs",
                "crates/oxfunc_core/src/registry.rs"
            ]
        );
        // Code-like words of the trailing text apply to every token.
        assert!(found[0].symbols.contains("RefCell"));
        assert!(found[0].symbols.contains("FunctionMeta"));
        assert!(!found[1].symbols.contains("FunctionMeta"));
    }

    #[test]
    fn dispositions_follow_the_clause_and_the_trailing_text() {
        let text = "OxFunc/a/function.rs: FunctionMeta laziness; OxFml/b/eval/mod.rs: the lazy \
            scheduling as the behavioural reference; OxFml/b/semantics/ for argument classification. \
            Port the metadata-driven scheduling.";
        let found = cited(text);
        let dispositions: Vec<Disposition> = found.iter().map(|c| c.disposition).collect();
        assert_eq!(
            dispositions,
            [Disposition::Port, Disposition::Reference, Disposition::Port]
        );
        assert!(found[2].dir);
        let all = "DnaTreeCalc/x/workbook.rs: WorkbookSession; DnaTreeCalc/x/command.rs: HostCommand. \
            Use these as behavioural references only; replace engine-owned authoring.";
        assert!(
            cited(all)
                .iter()
                .all(|c| c.disposition == Disposition::Reference)
        );
        let quarantined = "OxFunc/n/x87.rs: exp, pow_direct, atan and the fyl2x_fn expansion are \
            QUARANTINED, not port sources until provenance clearance. OxFunc/n/ is a discovery \
            location only.";
        let found = cited(quarantined);
        assert_eq!(found[0].disposition, Disposition::Quarantined);
        assert_eq!(found[1].disposition, Disposition::Reference);
        assert!(!found[1].excluded);
        assert!(found[0].symbols.contains("pow_direct") && found[0].symbols.contains("fyl2x_fn"));
    }

    #[test]
    fn listed_capitalised_words_and_qualified_names_are_symbols() {
        let text = "OxFunc/crates/oxfunc_value_types/src/lib.rs: CalcValue, CoreValue, Shared, \
            ReferenceLike and WorksheetErrorCode. Remove the Rc feature arm.";
        let symbols = &cited(text)[0].symbols;
        assert_eq!(
            symbols.iter().collect::<Vec<_>>(),
            [
                "CalcValue",
                "CoreValue",
                "ReferenceLike",
                "Shared",
                "WorksheetErrorCode"
            ]
        );
        let text = "OxDoc/crates/x/src/lib.rs (191 engine-named OxCalc* symbols such as OxCalcCell and \
            OXCALC_INGEST_INTERFACE_V1 that must not enter CalcDoc); OxDoc/crates/y/src/lib.rs \
            (WriteMode::FreshExport Save As); OxDoc/docs/SPEC.md, LIFECYCLE.md and SCOPE.md. The \
            text asks for exact symbols.";
        let port_from = parse(text);
        assert!(port_from.wants_symbols);
        let first = &port_from.cited[0].symbols;
        assert!(first.contains("OxCalcCell") && first.contains("OXCALC_INGEST_INTERFACE_V1"));
        assert!(!first.contains("OxCalc") && !first.contains("CalcDoc"));
        assert!(
            port_from.cited[1]
                .symbols
                .contains("WriteMode::FreshExport")
        );
        let tokens: Vec<&str> = port_from.cited.iter().map(|c| c.token.as_str()).collect();
        assert_eq!(
            tokens,
            [
                "OxDoc/crates/x/src/lib.rs",
                "OxDoc/crates/y/src/lib.rs",
                "OxDoc/docs/SPEC.md",
                "LIFECYCLE.md",
                "SCOPE.md"
            ]
        );
    }

    #[test]
    fn sections_and_family_lists_are_found() {
        let description = "## Outcome\nx\n\n## Do\nPort the named family only: ABS, SIGN, \
            NETWORKDAYS.INTL and their A-suffixed variants. Preserve tests.\n\n## Port from\nOxFunc/a.rs\n\n## Acceptance\ny\n";
        assert_eq!(section(description, "Port from"), Some("OxFunc/a.rs"));
        assert_eq!(section(description, "Outcome"), Some("x"));
        let items = family_items(section(description, "Do").unwrap()).unwrap();
        assert_eq!(
            items,
            [
                "ABS",
                "SIGN",
                "NETWORKDAYS.INTL",
                "their A-suffixed variants"
            ]
        );
        assert!(family_items("Port the metadata.").is_none());
    }

    #[test]
    fn catalogue_and_test_requests_are_noticed() {
        let text = "OxFunc/crates/oxfunc_core/src/functions/surface_dispatch.rs: the catalogue \
            entries for the function IDs named in Do; OxFunc/crates/oxfunc_core/src/function_call.rs: \
            FunctionCallTarget.";
        let found = cited(text);
        assert!(found[0].catalogue && !found[1].catalogue);
        let tests = parse("OxFml/a/syntax/ are discovery paths. Resolve exact test files.");
        assert!(tests.wants_tests && !tests.wants_symbols);
    }

    // The disposition classifier, one rule per test, on fixture texts shaped like the tracker's
    // Port from sections. Each test fails when its rule is removed: the default port reading, the
    // reference markers, the source-discovery marker, the quarantine marker and the exclusion.

    /// A path with no marker in its clause, its sentence or the trailing text may be ported.
    #[test]
    fn classifier_port() {
        let found = cited(
            "OxFunc/crates/oxfunc_core/src/coercion.rs: CoercionError and the decimal parser. \
             Preserve the tests.",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].disposition, Disposition::Port);
        assert!(!found[0].excluded && !found[0].discovery);
    }

    /// A reference marker in the clause makes the path a behavioural reference.
    #[test]
    fn classifier_reference() {
        let found = cited(
            "DnaTreeCalc/src/dnacalc-host-core/src/workbook.rs: WorkbookSession as a \
             behavioural reference; OxCalc/src/oxcalc-core/src/grid/authored.rs: \
             GridInputSnapshotId.",
        );
        assert_eq!(found[0].disposition, Disposition::Reference);
        assert!(!found[0].discovery);
        assert_eq!(found[1].disposition, Disposition::Port);
    }

    /// Paths that a sentence names source-discovery paths are references (never port), even when
    /// the marker sits after the next path, as in `A and B are source-discovery paths`. The
    /// parser text, which names discovery paths whose exact files must be resolved before
    /// porting, stays port: it asks for the files to be selected for porting, not read.
    #[test]
    fn classifier_discovery_reference() {
        let text = "OxFunc/smart-fuzzer/ and OxFunc/tools/xll-addin/ are source-discovery paths \
            for capture. DnaTreeCalc/src/dnacalc-conform/ is the provenance-first verdict \
            precedent. Resolve and record exact files in infra-m0.4; retain only one new COM \
            capture implementation and one verdict implementation.";
        let found = cited(text);
        let shown: Vec<(&str, Disposition, bool)> = found
            .iter()
            .map(|c| (c.token.as_str(), c.disposition, c.discovery))
            .collect();
        assert_eq!(
            shown,
            [
                ("OxFunc/smart-fuzzer/", Disposition::Reference, true),
                ("OxFunc/tools/xll-addin/", Disposition::Reference, true),
                (
                    "DnaTreeCalc/src/dnacalc-conform/",
                    Disposition::Reference,
                    false
                ),
            ]
        );
        let parser = cited(
            "OxFml/crates/oxfml_core/src/syntax/ and OxFml/crates/oxfml_core/src/language_service/ \
             are discovery paths. The source-manifest bead must resolve exact \
             lexer/parser/service/test files and exported symbols before porting.",
        );
        assert!(
            parser
                .iter()
                .all(|c| c.disposition == Disposition::Port && !c.discovery)
        );
    }

    /// The quarantine marker in the clause quarantines the path whatever else the text says.
    #[test]
    fn classifier_quarantined() {
        let found = cited(
            "OxFunc/crates/oxfunc_core/src/excel_numeric/x87.rs: exp and pow_direct are \
             QUARANTINED, not port sources until provenance clearance. \
             OxFunc/crates/oxfunc_core/src/excel_numeric/ is a discovery location only.",
        );
        assert_eq!(found[0].disposition, Disposition::Quarantined);
        assert_eq!(found[1].disposition, Disposition::Reference);
    }

    /// A path whose own sentence says not to port it is excluded; the paths of other sentences
    /// are not.
    #[test]
    fn classifier_excluded() {
        let found = cited(
            "OxCalc/src/oxcalc-core/src/grid/machine/invalidation.rs: GridAxisValueDependency. \
             Do NOT port consumer.rs or grid/machine/r1c1_plan.rs.",
        );
        let shown: Vec<(&str, bool)> = found
            .iter()
            .map(|c| (c.path.as_str(), c.excluded))
            .collect();
        assert_eq!(
            shown,
            [
                ("src/oxcalc-core/src/grid/machine/invalidation.rs", false),
                ("consumer.rs", true),
                ("grid/machine/r1c1_plan.rs", true),
            ]
        );
    }
}
