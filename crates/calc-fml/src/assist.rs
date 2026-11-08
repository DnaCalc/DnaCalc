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

/// Friendly spelling of recognized function calls, without changing stored formula text.
/// Tokens keep their syntax roles: strings, sheet/table names and defined names are retained.
/// Incomplete or unsupported syntax stays exactly as authored.
pub fn display_formula(text: &str) -> String {
    let bytes = text.as_bytes();
    let has = |prefix: &[u8]| bytes.windows(prefix.len()).any(|v| v.eq_ignore_ascii_case(prefix));
    if !text.trim_start().starts_with('=') || !(has(b"_xlfn.") || has(b"_xlpm.") || has(b"_xlws.")) {
        return text.to_owned();
    }
    let tree = crate::parse_formula(text);
    if !tree.diagnostics().is_empty() {
        return text.to_owned();
    }
    let mut changes = std::collections::BTreeMap::new();
    let mut pending = vec![tree.root()];
    while let Some(node) = pending.pop() {
        if node.kind() == SyntaxKind::CallExpr
            && let Some(SyntaxChild::Token(token)) = node.children().first()
        {
            let spelling = token.text();
            // `_xlws.` can also stand alone before a worksheet function the file stores with it.
            let named = crate::eval::file_unprefixed(spelling);
            let friendly = crate::eval::without_prefix(named, "_xlws.").unwrap_or(named);
            if friendly != spelling
                && (function_named(friendly).is_some()
                    || crate::eval::binding_call(friendly).is_some())
            {
                changes.insert(token.span().start, friendly.to_owned());
            }
        }
        pending.extend(node.child_nodes());
    }
    // A LET name or LAMBDA parameter a file stores as `_xlpm.name` is shown as `name`.
    for token in tree.tokens() {
        if token.kind() == TokenKind::Identifier
            && let Some(name) = crate::eval::without_prefix(token.text(), "_xlpm.")
            && !name.is_empty()
            && !changes.contains_key(&token.span().start)
        {
            changes.insert(token.span().start, name.to_owned());
        }
    }
    let mut shown = String::with_capacity(text.len());
    for token in tree.tokens() {
        for trivia in token.leading_trivia() {
            shown.push_str(trivia.text());
        }
        shown.push_str(
            changes
                .get(&token.span().start)
                .map_or(token.text(), String::as_str),
        );
    }
    shown
}

#[cfg(test)]
mod formula_display_tests {
    use super::display_formula;

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn display_changes_only_known_call_tokens_and_preserves_lossless_source() {
        for (source, expected) in [
            ("=_xlfn.SEQUENCE(3,2)", "=SEQUENCE(3,2)"),
            (
                "= _XLFN._XLWS.FILTER( A1:A3 , A1:A3>0 ) ",
                "= FILTER( A1:A3 , A1:A3>0 ) ",
            ),
            (
                "=\"😀 _xlfn.SEQUENCE(\"&_xlfn.SUM(A1)+_xlfn.foo+'_xlfn.SUM'!A2",
                "=\"😀 _xlfn.SEQUENCE(\"&SUM(A1)+_xlfn.foo+'_xlfn.SUM'!A2",
            ),
            (
                "=_xlfn.LET(_xlpm.x,2,_xlpm.x+_xlfn.SEQUENCE(2))",
                "=LET(x,2,x+SEQUENCE(2))",
            ),
            (
                "=_xlfn.LAMBDA(_xlpm.x,_xlpm.x+1)(2)",
                "=LAMBDA(x,x+1)(2)",
            ),
            ("=LET(_xlpm.x,2,_XLPM.X+1)", "=LET(x,2,X+1)"),
            ("=_xlws.SORT(A1:A3)+'_xlpm.x'!A1", "=SORT(A1:A3)+'_xlpm.x'!A1"),
            ("=_xlws.NOSUCH(1)", "=_xlws.NOSUCH(1)"),
            ("=_xlfn.SUM+_xlfn.SEQUENCE", "=_xlfn.SUM+_xlfn.SEQUENCE"),
            ("=SUM(_xlfn.Table[_xlfn.SEQUENCE])", "=SUM(_xlfn.Table[_xlfn.SEQUENCE])"),
            ("=_xlfn.UNKNOWN(1)", "=_xlfn.UNKNOWN(1)"),
            ("=_xlfn.SEQUENCE(", "=_xlfn.SEQUENCE("),
            ("_xlfn.SEQUENCE(3)", "_xlfn.SEQUENCE(3)"),
            (
                "='_xlfn.SEQUENCE'!A1+_xlfn._xlws.FILTER(A1:A3,A1:A3)",
                "='_xlfn.SEQUENCE'!A1+FILTER(A1:A3,A1:A3)",
            ),
        ] {
            let stored = source.to_owned();
            let shown = display_formula(&stored);
            assert_eq!(shown, expected, "{source}");
            assert_eq!(stored, source);
            assert_eq!(display_formula(&shown), shown);
        }
    }
}

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

/// A call's name without the prefixes a file stores before it (`_xlfn.`, then `_xlws.`).
pub fn unprefixed(spelling: &str) -> &str {
    crate::eval::file_unprefixed(spelling)
}

/// What a proposal is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalKind {
    /// A function of calc-func's catalogue.
    Function,
    /// A function the caller registered (an add-in's or a VBA module's), in [`Scope::functions`].
    Registered,
    /// A defined name, in [`Scope::names`].
    Name,
    /// A LET name or LAMBDA parameter in scope at the caret.
    Local,
}

/// What the caller owns that completion offers besides the catalogue: defined names and registered
/// functions. LET names and LAMBDA parameters come from the formula text itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scope {
    pub names: Vec<String>,
    pub functions: Vec<String>,
}

/// One completion: a function, a name or a local whose spelling starts with the typed prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub kind: ProposalKind,
    /// The catalogue entry, for [`ProposalKind::Function`].
    pub target: Option<FunctionCallTarget<'static>>,
    label: String,
}

impl Proposal {
    fn function(target: FunctionCallTarget<'static>) -> Proposal {
        Proposal {
            kind: ProposalKind::Function,
            label: target.spelling().to_owned(),
            target: Some(target),
        }
    }

    /// The spelling the list shows.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Whether accepting it writes a call (the name and its opening parenthesis).
    pub fn is_call(&self) -> bool {
        matches!(self.kind, ProposalKind::Function | ProposalKind::Registered)
    }

    /// The text that replaces the prefix: a call's name and its opening parenthesis, or a name.
    pub fn insert_text(&self) -> String {
        if self.is_call() {
            format!("{}(", self.label)
        } else {
            self.label.clone()
        }
    }

    /// What the list says the proposal is, besides its label.
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            ProposalKind::Function => "function",
            ProposalKind::Registered => "registered function",
            ProposalKind::Name => "defined name",
            ProposalKind::Local => "local name",
        }
    }
}

/// The completions at a caret: the span of the name being typed and the functions, names and
/// locals that start with it, in name order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// The typed prefix's span, which an accepted proposal replaces.
    pub replace: TextSpan,
    pub prefix: String,
    pub proposals: Vec<Proposal>,
}

impl Completion {
    /// Replace the name being edited, retaining an existing opening parenthesis and arguments.
    /// `replace` records the typed prefix; acceptance also consumes the token's remaining suffix.
    pub fn apply(&self, text: &str, proposal: &Proposal) -> (String, usize) {
        let units: Vec<u16> = text.encode_utf16().collect();
        let start = self.replace.start.min(units.len());
        let end = lex(text)
            .into_iter()
            .find(|token| {
                token.kind() == TokenKind::Identifier
                    && token.span().start == start
                    && token.span().end() >= self.replace.end()
            })
            .map_or(self.replace.end(), |token| token.span().end())
            .min(units.len());
        let spaces = units[end..].iter().take_while(|u| matches!(**u, 9 | 10 | 12 | 13 | 32)).count();
        let has_open = units.get(end + spaces) == Some(&(b'(' as u16));
        let reuse = has_open && proposal.is_call();
        let insert = if reuse || !proposal.is_call() { proposal.label().to_owned() } else { proposal.insert_text() };
        let mut out: Vec<u16> = units[..start].to_vec();
        out.extend(insert.encode_utf16());
        let caret = out.len() + if reuse { spaces + 1 } else { 0 };
        out.extend_from_slice(&units[end..]);
        (String::from_utf16_lossy(&out), caret)
    }
}

/// The function names that complete the name the caret ends, or `None` when the caret is not at
/// the end of a name being typed: the caret must lie inside or just after an unquoted name token
/// of calc-fml's lexer that begins with a letter, `_` or `\`, holds no `$`, and starts at the
/// formula's `=` or after an operator, delimiter or whitespace. The prefix is the token's text up
/// to the caret; a function matches when its name starts with the prefix in any ASCII case. The
/// LET names and LAMBDA parameters in scope at the caret are offered too ([`complete_in`] adds the
/// caller's defined names and registered functions).
pub fn complete(text: &str, caret: usize) -> Option<Completion> {
    complete_in(text, caret, &Scope::default())
}

/// [`complete`] with the caller's defined names and registered functions.
pub fn complete_in(text: &str, caret: usize, scope: &Scope) -> Option<Completion> {
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
    let starts = |name: &str| name.to_ascii_uppercase().starts_with(&wanted);
    let mut proposals: Vec<Proposal> = catalogue()
        .entries()
        .filter(|target| target.notation() == Notation::Call)
        .filter(|target| function_named(target.spelling()).is_some())
        .filter(|target| target.spelling().starts_with(wanted.as_str()))
        .map(Proposal::function)
        .collect();
    let mut extra = |kind: ProposalKind, name: &str| {
        let duplicate = proposals
            .iter()
            .any(|have| have.kind == kind && have.label.eq_ignore_ascii_case(name));
        if !duplicate && !name.is_empty() && starts(name) {
            proposals.push(Proposal {
                kind,
                target: None,
                label: name.to_owned(),
            });
        }
    };
    let tree = crate::parse_formula(text);
    for local in locals_at(&tree, text, caret) {
        // The name being declared is not its own completion.
        if !(local.eq_ignore_ascii_case(&prefix) && prefix_units.len() == token.span().len) {
            extra(ProposalKind::Local, &local);
        }
    }
    for name in &scope.names {
        extra(ProposalKind::Name, name);
    }
    for name in &scope.functions {
        extra(ProposalKind::Registered, name);
    }
    proposals.sort_by_cached_key(|p| (p.label.to_ascii_uppercase(), p.kind as u8));
    Some(Completion {
        replace: TextSpan::new(token.span().start, prefix_units.len()),
        prefix,
        proposals,
    })
}

/// A name in scope at a caret: a LET name with the source text of its value, or a LAMBDA parameter
/// (no value until the LAMBDA is called).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalBinding {
    pub name: String,
    /// The source text of the LET name's value; `None` for a LAMBDA parameter.
    pub value: Option<String>,
}

/// The LET names and LAMBDA parameters visible at the caret, innermost first: the names of a LET
/// whose value the caret is past, and the parameters of a LAMBDA the caret is in the body of.
pub fn locals_at(tree: &SyntaxTree, text: &str, caret: usize) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for binding in local_bindings_at(tree, text, caret).into_iter().rev() {
        if !found.iter().any(|have| have.eq_ignore_ascii_case(&binding.name)) {
            found.push(binding.name);
        }
    }
    found
}

/// The bindings in scope at the caret, outermost enclosing call first and in declaration order
/// within it, so a shadowed name comes before the binding that shadows it.
pub fn local_bindings_at(tree: &SyntaxTree, text: &str, caret: usize) -> Vec<LocalBinding> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut calls: Vec<&SyntaxNode> = Vec::new();
    let mut pending: Vec<&SyntaxNode> = vec![tree.root()];
    while let Some(node) = pending.pop() {
        let span = node.span();
        if !(caret >= span.start && caret <= span.end() + 1) {
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
            if open_end.is_some_and(|end| caret >= end)
                && close.is_none_or(|close| caret <= close.start)
            {
                calls.push(node);
            }
        }
        pending.extend(node.child_nodes());
    }
    // Outermost first: the widest call has the largest span.
    calls.sort_by_key(|node| std::cmp::Reverse(node.span().len));
    let mut found: Vec<LocalBinding> = Vec::new();
    for call in calls {
        let Some(SyntaxChild::Token(callee)) = call.children().first() else {
            continue;
        };
        let unprefixed = crate::eval::file_unprefixed(callee.text());
        let is_let = unprefixed.eq_ignore_ascii_case("LET");
        if !is_let && !unprefixed.eq_ignore_ascii_case("LAMBDA") {
            continue;
        }
        let Some(arguments) = argument_list(call) else {
            continue;
        };
        // The argument slices: from the opening parenthesis or a comma to the next comma or the
        // closing parenthesis.
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        for child in arguments.children() {
            if let SyntaxChild::Token(token) = child {
                match token.kind() {
                    TokenKind::LParen if starts.is_empty() => starts.push(token.span().end()),
                    TokenKind::Comma => {
                        ends.push(token.span().start);
                        starts.push(token.span().end());
                    }
                    _ => {}
                }
            }
        }
        ends.push(closing_paren(arguments).map_or(units.len(), |close| close.start));
        let count = starts.len().min(ends.len());
        let active = arguments
            .children()
            .iter()
            .filter(|child| {
                matches!(child, SyntaxChild::Token(token)
                    if token.kind() == TokenKind::Comma && token.span().start < caret)
            })
            .count();
        let slice = |index: usize| -> Option<String> {
            let range = units.get(starts[index].min(units.len())..ends[index].min(units.len()))?;
            String::from_utf16(range).ok()
        };
        let name_of = |index: usize| -> Option<String> {
            let raw = slice(index)?;
            let raw = raw.trim().trim_start_matches('[').trim_end_matches(']').trim();
            let raw = crate::eval::without_prefix(raw, "_xlpm.")
                .or_else(|| crate::eval::without_prefix(raw, "_xlop."))
                .unwrap_or(raw);
            let valid = raw.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
                && raw.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.');
            valid.then(|| raw.to_owned())
        };
        if is_let {
            for name in (0..count).step_by(2).filter(|name| name + 1 < active && name + 1 < count) {
                if let (Some(n), Some(value)) = (name_of(name), slice(name + 1)) {
                    found.push(LocalBinding { name: n, value: Some(value.trim().to_owned()) });
                }
            }
        } else if active + 1 == count && active >= 1 {
            for index in 0..active {
                if let Some(n) = name_of(index) {
                    found.push(LocalBinding { name: n, value: None });
                }
            }
        }
    }
    found
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
            .cloned()
            .find(|p| p.label() == "SUM")
            .expect("SUM");
        assert_eq!(completion.apply("=SU", &sum), ("=SUM(".to_owned(), 5));

        // Case does not matter, and the rest of the text stays.
        let completion = complete("=1+su*2", 5).expect("a name");
        assert_eq!(completion.prefix, "su");
        let sum = completion
            .proposals
            .iter()
            .cloned()
            .find(|p| p.label() == "SUM")
            .expect("SUM");
        assert_eq!(
            completion.apply("=1+su*2", &sum),
            ("=1+SUM(*2".to_owned(), 7)
        );

        // The caret mid-name completes the part before it.
        let completion = complete("=SUMX", 3).expect("a name");
        assert_eq!(completion.prefix, "SU");
        assert_eq!(completion.replace, TextSpan::new(1, 2));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn completion_includes_every_resolvable_catalogue_call() {
        for target in catalogue().entries().filter(|t| t.notation() == Notation::Call) {
            if function_named(target.spelling()).is_none() { continue; }
            let text = format!("={}", target.spelling());
            let found = complete(&text, text.encode_utf16().count()).unwrap();
            assert!(found.proposals.iter().any(|p| p.target == Some(target)), "{}", target.spelling());
        }
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
    fn accepting_completion_preserves_arguments_and_replaces_the_whole_name() {
        for (text, caret, expected, after) in [
            ("=SUMX(1,2)", 4, "=SUM(1,2)", 5),
            ("=SUM(1,2)", 4, "=SUM(1,2)", 5),
            ("=SUMX + 2", 4, "=SUM( + 2", 5),
            ("=SUMX  (1,2)", 4, "=SUM  (1,2)", 7),
            ("=SUMX\u{c}(1,2)", 4, "=SUM\u{c}(1,2)", 6),
            ("=IF(TRUE,\"😀\",SUMX(1,2))", 17, "=IF(TRUE,\"😀\",SUM(1,2))", 18),
        ] {
            let completion = complete(text, caret).expect(text);
            let sum = completion.proposals.iter().cloned().find(|p| p.label() == "SUM").unwrap();
            assert_eq!(completion.apply(text, &sum), (expected.to_owned(), after), "{text}");
        }
    }

    fn labels_of(completion: &Completion) -> Vec<(String, ProposalKind)> {
        completion
            .proposals
            .iter()
            .map(|p| (p.label().to_owned(), p.kind))
            .collect()
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn let_names_and_lambda_parameters_complete_where_they_are_in_scope() {
        let local = |text: &str, caret: usize| {
            complete(text, caret)
                .map(|c| labels_of(&c))
                .unwrap_or_default()
                .into_iter()
                .filter(|(_, kind)| *kind == ProposalKind::Local)
                .map(|(label, _)| label)
                .collect::<Vec<_>>()
        };
        // In the body, and in a later value, a LET name is offered; in its own value it is not.
        assert_eq!(local("=LET(alpha,1,al", 15), ["alpha"]);
        assert_eq!(local("=LET(alpha,1,beta,al", 20), ["alpha"]);
        assert_eq!(local("=LET(alpha,al", 13), Vec::<String>::new());
        assert_eq!(local("=LET(alpha,1,beta,2,be", 22), ["beta"]);
        // The file's prefix is not part of the name.
        assert_eq!(local("=_xlfn.LET(_xlpm.alpha,1,al", 27), ["alpha"]);
        // A LAMBDA parameter is offered in the body only; an optional one without its brackets.
        assert_eq!(local("=LAMBDA(x,[yy],y", 16), ["yy"]);
        assert_eq!(local("=LAMBDA(xx,xx+1)(x", 18), Vec::<String>::new());
        assert_eq!(local("=LAMBDA(xx,x", 12), ["xx"]);
        // Nested: the inner binding first, the outer ones still visible, and after the call none.
        assert_eq!(
            local("=LET(aa,1,MAP({1},LAMBDA(ab,a)))", 29),
            ["aa", "ab"],
            "the list is in name order"
        );
        let text = "=LET(aa,1,MAP({1},LAMBDA(ab,a)))";
        assert_eq!(locals_at(&parse_formula(text), text, 29), ["ab", "aa"], "innermost first");
        assert_eq!(local("=LET(aa,1,aa)+a", 15), Vec::<String>::new());
        // A local accepted keeps no parenthesis.
        let completion = complete("=LET(alpha,1,al", 15).unwrap();
        let alpha = completion.proposals.iter().find(|p| p.label() == "alpha").unwrap();
        assert_eq!(alpha.insert_text(), "alpha");
        assert_eq!(completion.apply("=LET(alpha,1,al", alpha), ("=LET(alpha,1,alpha".to_owned(), 18));
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn defined_names_and_registered_functions_complete_beside_the_catalogue() {
        let scope = Scope {
            names: vec!["Sales".into(), "SumRange".into()],
            functions: vec!["SUMTHING".into(), "Module1.Add2".into()],
        };
        let completion = complete_in("=SUM+Sal", 8, &scope).unwrap();
        assert_eq!(labels_of(&completion), [("Sales".to_owned(), ProposalKind::Name)]);
        let completion = complete_in("=SUM", 4, &scope).unwrap();
        let found = labels_of(&completion);
        assert!(found.contains(&("SUM".to_owned(), ProposalKind::Function)));
        assert!(found.contains(&("SumRange".to_owned(), ProposalKind::Name)));
        assert!(found.contains(&("SUMTHING".to_owned(), ProposalKind::Registered)));
        let mut sorted: Vec<String> = found.iter().map(|(label, _)| label.to_ascii_uppercase()).collect();
        let before = sorted.clone();
        sorted.sort();
        assert_eq!(before, sorted, "name order across kinds");
        let thing = completion.proposals.iter().find(|p| p.label() == "SUMTHING").unwrap();
        assert_eq!(completion.apply("=SUM", thing), ("=SUMTHING(".to_owned(), 10));
        let name = completion.proposals.iter().find(|p| p.label() == "SumRange").unwrap();
        assert_eq!(completion.apply("=SUM", name), ("=SumRange".to_owned(), 9));
        let completion = complete_in("=Mod", 4, &scope).unwrap();
        assert!(labels_of(&completion).contains(&("Module1.Add2".to_owned(), ProposalKind::Registered)));
        let completion = complete_in("=Module1.A", 10, &scope);
        assert!(completion.is_none_or(|c| c.proposals.iter().all(|p| p.label().starts_with("Module1.A"))));
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
