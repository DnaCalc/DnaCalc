//! Editor assistance over formula text: the role of each token for syntax highlighting, function
//! name completion from calc-func's catalogue, and the call the caret is in for signature help.
//!
//! Ported from OxFml's language service (`crates/oxfml_core/src/language_service/mod.rs` at OxFml
//! 15daac5: `completion_prefix`, `signature_help_context_at_cursor`, `smallest_call_like_node`,
//! `closed_call_close_paren_end` and `active_argument_index`) and DnaOneCalc's syntax runs
//! (`src/dnaonecalc-host/src/ui/editor/render_projection.rs` at DnaOneCalc 61e3979), onto this
//! crate's lexer, parser and spans. Every offset is in UTF-16 code units, the unit a browser's
//! text field counts its caret in.
//!
//! Nothing here evaluates or binds: completion offers the catalogue's named functions, and the
//! compiler alone decides whether a formula's names resolve.

use crate::syntax::{SyntaxChild, SyntaxKind, SyntaxNode, SyntaxTree, TextSpan, TokenKind, lex};
use calc_func::{FunctionCallTarget, Notation, catalogue};

/// What a run of formula text is, for highlighting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenRole {
    /// A name followed by `(`: a function call's name.
    Function,
    /// Any other unquoted or quoted name: a reference, a defined name, `TRUE`, a sheet name.
    Name,
    Number,
    /// A string literal, quotes included.
    Text,
    /// A worksheet error literal such as `#N/A`.
    Error,
    /// An operator: `=` `+` `-` `*` `/` `^` `&` `%` `:` `!` `@` `#` and the comparisons.
    Operator,
    /// `(` `)` `,` `;` `{` `}`.
    Delimiter,
    Whitespace,
    /// A character the lexer does not recognise.
    Unknown,
}

/// One token of the formula text and its role. The runs of [`highlight`] cover the text exactly,
/// in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightRun {
    pub span: TextSpan,
    pub text: String,
    pub role: TokenRole,
}

/// The formula text split into runs, one per token of calc-fml's lexer, each with its role. The
/// runs' texts, joined, are `text` exactly.
pub fn highlight(text: &str) -> Vec<HighlightRun> {
    let tokens = lex(text);
    let mut runs = Vec::with_capacity(tokens.len());
    for (index, token) in tokens.iter().enumerate() {
        if token.kind() == TokenKind::Eof {
            continue;
        }
        let role = match token.kind() {
            TokenKind::Identifier => {
                let next = tokens[index + 1..]
                    .iter()
                    .find(|next| next.kind() != TokenKind::Whitespace);
                if next.is_some_and(|next| next.kind() == TokenKind::LParen) {
                    TokenRole::Function
                } else {
                    TokenRole::Name
                }
            }
            TokenKind::QuotedIdentifier | TokenKind::BracketedQualifier => TokenRole::Name,
            TokenKind::Number => TokenRole::Number,
            TokenKind::StringLiteral => TokenRole::Text,
            TokenKind::ErrorLiteral => TokenRole::Error,
            TokenKind::LParen
            | TokenKind::RParen
            | TokenKind::Comma
            | TokenKind::Semicolon
            | TokenKind::LBrace
            | TokenKind::RBrace => TokenRole::Delimiter,
            TokenKind::Whitespace => TokenRole::Whitespace,
            TokenKind::Unknown => TokenRole::Unknown,
            TokenKind::Equals
            | TokenKind::NotEqual
            | TokenKind::Less
            | TokenKind::LessEqual
            | TokenKind::Greater
            | TokenKind::GreaterEqual
            | TokenKind::Colon
            | TokenKind::Ampersand
            | TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Percent
            | TokenKind::Caret
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Dot
            | TokenKind::At
            | TokenKind::Hash
            | TokenKind::Bang => TokenRole::Operator,
            TokenKind::Eof => unreachable!("skipped above"),
        };
        runs.push(HighlightRun {
            span: token.span(),
            text: token.text().to_owned(),
            role,
        });
    }
    runs
}

/// The catalogue's named function that a call written `spelling` calls: the spelling matched in
/// any ASCII case after the prefixes a file stores before a function's name (`_xlfn.`, and
/// `_xlws.` after it), as the compiler resolves it.
pub fn function_named(spelling: &str) -> Option<FunctionCallTarget<'static>> {
    catalogue()
        .resolve_surface(Notation::Call, crate::eval::file_unprefixed(spelling))
        .ok()
}

/// One completion: a catalogue function whose name starts with the typed prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub target: FunctionCallTarget<'static>,
}

impl Proposal {
    /// The function's name, as the list shows it.
    pub fn label(self) -> &'static str {
        self.target.spelling()
    }

    /// The text that replaces the prefix: the name and its opening parenthesis.
    pub fn insert_text(self) -> String {
        format!("{}(", self.target.spelling())
    }
}

/// The completions at a caret: the span of the name being typed and the catalogue's named
/// functions that start with it, in name order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// The typed prefix's span, which an accepted proposal replaces.
    pub replace: TextSpan,
    pub prefix: String,
    pub proposals: Vec<Proposal>,
}

impl Completion {
    /// `text` with the prefix replaced by `proposal`'s insert text, and the caret just after it.
    pub fn apply(&self, text: &str, proposal: Proposal) -> (String, usize) {
        let units: Vec<u16> = text.encode_utf16().collect();
        let start = self.replace.start.min(units.len());
        let end = self.replace.end().min(units.len());
        let insert = proposal.insert_text();
        let mut out: Vec<u16> = units[..start].to_vec();
        out.extend(insert.encode_utf16());
        let caret = out.len();
        out.extend_from_slice(&units[end..]);
        (String::from_utf16_lossy(&out), caret)
    }
}

/// The function names that complete the name the caret ends, or `None` when the caret is not at
/// the end of a name being typed: the caret must lie inside or just after an unquoted name token
/// of calc-fml's lexer that begins with a letter, `_` or `\`, holds no `$`, and starts at the
/// formula's `=` or after an operator, delimiter or whitespace. The prefix is the token's text up
/// to the caret; a function matches when its name starts with the prefix in any ASCII case.
pub fn complete(text: &str, caret: usize) -> Option<Completion> {
    if !text.trim_start().starts_with('=') {
        return None;
    }
    let tokens = lex(text);
    let at = tokens.iter().position(|token| {
        let span = token.span();
        token.kind() == TokenKind::Identifier && span.start < caret && caret <= span.end()
    })?;
    let token = &tokens[at];
    let prefix_units: Vec<u16> = token
        .text()
        .encode_utf16()
        .take(caret - token.span().start)
        .collect();
    let prefix = String::from_utf16(&prefix_units).ok()?;
    let first = prefix.chars().next()?;
    if !(first.is_alphabetic() || first == '_' || first == '\\') || prefix.contains('$') {
        return None;
    }
    // A name right after a `!` is a sheet-qualified reference; one after `[` a structured
    // reference.
    if at > 0
        && matches!(
            tokens[at - 1].kind(),
            TokenKind::Bang | TokenKind::BracketedQualifier
        )
    {
        return None;
    }
    let wanted = crate::eval::file_unprefixed(&prefix).to_ascii_uppercase();
    let proposals: Vec<Proposal> = catalogue()
        .surfaces()
        .filter(|target| target.notation() == Notation::Call)
        .filter(|target| target.spelling().starts_with(wanted.as_str()))
        .map(|target| Proposal { target })
        .collect();
    Some(Completion {
        replace: TextSpan::new(token.span().start, prefix_units.len()),
        prefix,
        proposals,
    })
}

/// The named call the caret is inside, for signature help.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite {
    /// The call's name as written.
    pub callee: String,
    pub callee_span: TextSpan,
    /// The whole call, name to closing parenthesis (or to where the text ends, for a call not
    /// closed yet).
    pub call_span: TextSpan,
    /// The argument the caret is in, from 0: the number of the argument list's commas before it.
    pub active_argument: usize,
    /// The catalogue entry the name calls, if the catalogue has one.
    pub target: Option<FunctionCallTarget<'static>>,
}

/// The innermost named call whose argument list the caret is in: after its `(` and, for a closed
/// call, before its `)`. A caret just past an unclosed call's last token still counts as inside,
/// so help stays up while the arguments are typed.
pub fn call_at(tree: &SyntaxTree, caret: usize) -> Option<CallSite> {
    let mut best: Option<&SyntaxNode> = None;
    let mut pending: Vec<&SyntaxNode> = vec![tree.root()];
    while let Some(node) = pending.pop() {
        let span = node.span();
        let inside = caret >= span.start && caret <= span.end();
        let one_past = caret == span.end() + 1;
        if !inside && !one_past {
            continue;
        }
        if node.kind() == SyntaxKind::CallExpr
            && let Some(arguments) = argument_list(node)
        {
            let open_end = arguments.children().first().and_then(|child| match child {
                SyntaxChild::Token(token) if token.kind() == TokenKind::LParen => {
                    Some(token.span().end())
                }
                _ => None,
            });
            let close = closing_paren(arguments);
            let after_open = open_end.is_some_and(|end| caret >= end);
            let before_close = match close {
                Some(close) => caret <= close.start,
                None => true,
            };
            let reaches = inside || close.is_none();
            if after_open
                && before_close
                && reaches
                && best.is_none_or(|current| span.len <= current.span().len)
            {
                best = Some(node);
            }
        }
        pending.extend(node.child_nodes());
    }
    let call = best?;
    let name = match call.children().first()? {
        SyntaxChild::Token(token) => token,
        SyntaxChild::Node(_) => return None,
    };
    let arguments = argument_list(call)?;
    let active_argument = arguments
        .children()
        .iter()
        .filter(|child| {
            matches!(child, SyntaxChild::Token(token)
                if token.kind() == TokenKind::Comma && token.span().start < caret)
        })
        .count();
    Some(CallSite {
        callee: name.text().to_owned(),
        callee_span: name.span(),
        call_span: call.span(),
        active_argument,
        target: function_named(name.text()),
    })
}

fn argument_list(call: &SyntaxNode) -> Option<&SyntaxNode> {
    call.child_nodes()
        .find(|node| node.kind() == SyntaxKind::ArgumentList)
}

/// The span of an argument list's closing `)` when it has one the text holds (not one the parser
/// supplied as missing).
fn closing_paren(arguments: &SyntaxNode) -> Option<TextSpan> {
    match arguments.children().last()? {
        SyntaxChild::Token(token) if token.kind() == TokenKind::RParen && !token.is_missing() => {
            Some(token.span())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_formula;

    fn roles(text: &str) -> Vec<(String, TokenRole)> {
        highlight(text)
            .into_iter()
            .map(|run| (run.text, run.role))
            .collect()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn highlight_runs_cover_the_text_with_roles() {
        let text = "= SUM(A1:B2, \"x\", #N/A, 1.5) & Sheet2!C3";
        let runs = highlight(text);
        let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(joined, text);
        let shown = roles(text);
        assert_eq!(shown[0], ("=".to_owned(), TokenRole::Operator));
        assert_eq!(shown[1], (" ".to_owned(), TokenRole::Whitespace));
        assert_eq!(shown[2], ("SUM".to_owned(), TokenRole::Function));
        assert_eq!(shown[3], ("(".to_owned(), TokenRole::Delimiter));
        assert!(shown.contains(&("\"x\"".to_owned(), TokenRole::Text)));
        assert!(shown.contains(&("#N/A".to_owned(), TokenRole::Error)));
        assert!(shown.contains(&("1.5".to_owned(), TokenRole::Number)));
        assert!(shown.contains(&("Sheet2".to_owned(), TokenRole::Name)));
        assert!(shown.contains(&("&".to_owned(), TokenRole::Operator)));
        // Spans are UTF-16: an astral character in a string takes two units.
        let runs = highlight("=\"\u{1F600}\"&A1");
        assert_eq!(runs[1].span, TextSpan::new(1, 4));
        assert_eq!(runs[2].span.start, 5);
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn function_names_complete_from_the_catalogue() {
        let completion = complete("=SU", 3).expect("a name being typed");
        assert_eq!(completion.prefix, "SU");
        assert_eq!(completion.replace, TextSpan::new(1, 2));
        let labels: Vec<&str> = completion.proposals.iter().map(|p| p.label()).collect();
        assert!(labels.contains(&"SUM"), "{labels:?}");
        assert!(labels.iter().all(|label| label.starts_with("SU")));
        let mut sorted = labels.clone();
        sorted.sort_unstable();
        assert_eq!(labels, sorted, "name order");
        let sum = completion
            .proposals
            .iter()
            .copied()
            .find(|p| p.label() == "SUM")
            .expect("SUM");
        assert_eq!(completion.apply("=SU", sum), ("=SUM(".to_owned(), 5));

        // Case does not matter, and the rest of the text stays.
        let completion = complete("=1+su*2", 5).expect("a name");
        assert_eq!(completion.prefix, "su");
        let sum = completion
            .proposals
            .iter()
            .copied()
            .find(|p| p.label() == "SUM")
            .expect("SUM");
        assert_eq!(
            completion.apply("=1+su*2", sum),
            ("=1+SUM(*2".to_owned(), 7)
        );

        // The caret mid-name completes the part before it.
        let completion = complete("=SUMX", 3).expect("a name");
        assert_eq!(completion.prefix, "SU");
        assert_eq!(completion.replace, TextSpan::new(1, 2));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn completion_is_not_offered_outside_a_name() {
        assert_eq!(complete("SU", 2), None, "not a formula");
        assert_eq!(complete("=\"SU", 4), None, "inside a string");
        assert_eq!(complete("=1.5", 4), None, "a number");
        assert_eq!(complete("=$A", 3), None, "an absolute reference");
        assert_eq!(complete("=Sheet2!SU", 10), None, "a qualified reference");
        assert_eq!(complete("=SU", 1), None, "the caret before the name");
        let none = complete("=ZZZQ", 5).expect("a name being typed");
        assert!(none.proposals.is_empty());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn the_call_at_the_caret_gives_its_active_argument() {
        let text = "=SUM(1,IF(A1,2,3),4)";
        let tree = parse_formula(text);
        let at = |caret| call_at(&tree, caret).map(|site| (site.callee, site.active_argument));
        assert_eq!(at(4), None, "on the name, before the parenthesis");
        assert_eq!(at(5), Some(("SUM".to_owned(), 0)));
        assert_eq!(at(7), Some(("SUM".to_owned(), 1)));
        assert_eq!(at(10), Some(("IF".to_owned(), 0)));
        assert_eq!(at(13), Some(("IF".to_owned(), 1)));
        assert_eq!(at(15), Some(("IF".to_owned(), 2)));
        assert_eq!(at(18), Some(("SUM".to_owned(), 2)));
        assert_eq!(
            at(19),
            Some(("SUM".to_owned(), 2)),
            "before the closing parenthesis"
        );
        assert_eq!(at(20), None, "after the closing parenthesis");

        // An unclosed call keeps its help while arguments are typed.
        let text = "=ROUND(A1, ";
        let tree = parse_formula(text);
        let site = call_at(&tree, 11).expect("inside ROUND");
        assert_eq!(site.callee, "ROUND");
        assert_eq!(site.active_argument, 1);
        let target = site.target.expect("ROUND is in the catalogue");
        assert_eq!(target.spelling(), "ROUND");

        // A file's prefix resolves to the function; an unknown name has no target.
        let tree = parse_formula("=_xlfn.SUM(1)");
        assert_eq!(
            call_at(&tree, 11)
                .and_then(|site| site.target)
                .map(|t| t.spelling()),
            Some("SUM")
        );
        let tree = parse_formula("=NOSUCH(1)");
        let site = call_at(&tree, 8).expect("a call");
        assert_eq!(site.callee, "NOSUCH");
        assert_eq!(site.target, None);
    }
}
