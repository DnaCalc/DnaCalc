//! The Excel formula parser: a recursive-descent parser over the lossless tokens that always
//! returns a tree. Malformed or incomplete input yields diagnostics, missing tokens and
//! [`SyntaxKind::MissingExpr`] nodes, never a lost character.
//!
//! Precedence, loosest first: comparison (`=`, `<>`, `<`, `<=`, `>`, `>=`), `&`, `+` `-`, `*` `/`,
//! `^`, postfix `%`, prefix `+` `-`, then the reference operators union (`,`), intersection
//! (whitespace) and range (`:`), prefix `@`, and postfix `#` and invocation.

use super::lexer::{lex, quoted_text_is_closed};
use super::token::{SyntaxDiagnostic, SyntaxDiagnosticKind, TextSpan, Token, TokenKind, Trivia};
use super::tree::{SyntaxChild, SyntaxKind, SyntaxNode, SyntaxTree};
use calc_func::value::WorksheetErrorCode;

/// Parses formula text, with or without its leading `=`, into a lossless syntax tree.
pub fn parse_formula(source: &str) -> SyntaxTree {
    let mut parser = Parser::new(lex(source));
    let root = parser.parse_formula_root();
    SyntaxTree::new(root, parser.diagnostics)
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    diagnostics: Vec<SyntaxDiagnostic>,
    /// Whitespace skipped since the last token was taken; it becomes the leading trivia of the
    /// next token taken.
    pending_trivia: Vec<Trivia>,
    /// The whitespace a spaced-range-colon recovery has dropped; every suggested correction is
    /// the source without it, so an outer suggestion carries the inner corrections too.
    recovered_whitespace_spans: Vec<TextSpan>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            index: 0,
            diagnostics: Vec::new(),
            pending_trivia: Vec::new(),
            recovered_whitespace_spans: Vec::new(),
        }
    }

    fn parse_formula_root(&mut self) -> SyntaxNode {
        let mut children = Vec::new();
        self.skip_whitespace();
        if self.at(TokenKind::Equals) {
            children.push(SyntaxChild::Token(self.bump()));
        }
        self.skip_whitespace();
        children.push(SyntaxChild::Node(self.parse_expression(true)));
        self.skip_whitespace();
        while !self.at(TokenKind::Eof) {
            let token = self.bump();
            self.diagnostics.push(SyntaxDiagnostic::message(
                format!("unexpected trailing token {:?}", token.kind()),
                token.span(),
            ));
            children.push(SyntaxChild::Token(token));
            self.skip_whitespace();
        }
        children.push(SyntaxChild::Token(self.bump()));
        SyntaxNode::new(SyntaxKind::FormulaRoot, children)
    }

    fn parse_expression(&mut self, allow_union_comma: bool) -> SyntaxNode {
        self.parse_comparison(allow_union_comma)
    }

    fn parse_comparison(&mut self, allow_union_comma: bool) -> SyntaxNode {
        self.parse_binary(
            allow_union_comma,
            &[
                TokenKind::Equals,
                TokenKind::NotEqual,
                TokenKind::Less,
                TokenKind::LessEqual,
                TokenKind::Greater,
                TokenKind::GreaterEqual,
            ],
            Self::parse_concat,
        )
    }

    fn parse_concat(&mut self, allow_union_comma: bool) -> SyntaxNode {
        self.parse_binary(
            allow_union_comma,
            &[TokenKind::Ampersand],
            Self::parse_additive,
        )
    }

    fn parse_additive(&mut self, allow_union_comma: bool) -> SyntaxNode {
        self.parse_binary(
            allow_union_comma,
            &[TokenKind::Plus, TokenKind::Minus],
            Self::parse_multiplicative,
        )
    }

    fn parse_multiplicative(&mut self, allow_union_comma: bool) -> SyntaxNode {
        self.parse_binary(
            allow_union_comma,
            &[TokenKind::Star, TokenKind::Slash],
            Self::parse_power,
        )
    }

    fn parse_power(&mut self, allow_union_comma: bool) -> SyntaxNode {
        self.parse_binary(allow_union_comma, &[TokenKind::Caret], Self::parse_percent)
    }

    /// One left-associative level of binary operators over the next tighter level.
    fn parse_binary(
        &mut self,
        allow_union_comma: bool,
        operators: &[TokenKind],
        operand: fn(&mut Self, bool) -> SyntaxNode,
    ) -> SyntaxNode {
        let mut left = operand(self, allow_union_comma);
        while operators.contains(&self.current().kind()) {
            let op = self.bump();
            let right = operand(self, allow_union_comma);
            left = SyntaxNode::new(
                SyntaxKind::BinaryExpr,
                vec![
                    SyntaxChild::Node(left),
                    SyntaxChild::Token(op),
                    SyntaxChild::Node(right),
                ],
            );
        }
        left
    }

    fn parse_percent(&mut self, allow_union_comma: bool) -> SyntaxNode {
        let mut node = self.parse_prefix(allow_union_comma);
        while self.at(TokenKind::Percent) {
            let percent = self.bump();
            node = SyntaxNode::new(
                SyntaxKind::PostfixExpr,
                vec![SyntaxChild::Node(node), SyntaxChild::Token(percent)],
            );
        }
        node
    }

    fn parse_prefix(&mut self, allow_union_comma: bool) -> SyntaxNode {
        self.skip_whitespace();
        if self.at(TokenKind::Plus) || self.at(TokenKind::Minus) {
            let op = self.bump();
            let operand = self.parse_prefix(allow_union_comma);
            SyntaxNode::new(
                SyntaxKind::PrefixExpr,
                vec![SyntaxChild::Token(op), SyntaxChild::Node(operand)],
            )
        } else {
            self.parse_union(allow_union_comma)
        }
    }

    /// The union operator `,`, which only exists where a comma separates nothing else: at the top
    /// level and inside parentheses, not in an argument list or an array literal.
    fn parse_union(&mut self, allow_union_comma: bool) -> SyntaxNode {
        let mut left = self.parse_intersection();
        loop {
            self.skip_whitespace();
            if !(allow_union_comma && self.at(TokenKind::Comma)) {
                return left;
            }
            let comma = self.bump();
            self.skip_whitespace();
            let right = self.parse_intersection();
            left = SyntaxNode::new(
                SyntaxKind::UnionExpr,
                vec![
                    SyntaxChild::Node(left),
                    SyntaxChild::Token(comma),
                    SyntaxChild::Node(right),
                ],
            );
        }
    }

    /// The intersection operator: whitespace between two references. Whitespace that is not
    /// followed by the start of a reference is left for the caller as trivia.
    fn parse_intersection(&mut self) -> SyntaxNode {
        let mut left = self.parse_range();
        loop {
            let before_whitespace = self.index;
            let spaces = self.take_whitespace_tokens();
            if spaces.is_empty() {
                return left;
            }
            if self.at(TokenKind::Colon) {
                left = self.recover_spaced_range_colon(left, spaces);
                continue;
            }
            if !self.starts_reference_expr() {
                self.index = before_whitespace;
                return left;
            }
            let right = self.parse_range();
            let mut children = vec![SyntaxChild::Node(left)];
            children.extend(spaces.into_iter().map(SyntaxChild::Token));
            children.push(SyntaxChild::Node(right));
            left = SyntaxNode::new(SyntaxKind::IntersectionExpr, children);
        }
    }

    fn parse_range(&mut self) -> SyntaxNode {
        self.skip_whitespace();
        if self.at(TokenKind::At) {
            let at = self.bump();
            let operand = self.parse_range();
            return SyntaxNode::new(
                SyntaxKind::PrefixExpr,
                vec![SyntaxChild::Token(at), SyntaxChild::Node(operand)],
            );
        }
        if self.at(TokenKind::Caret) {
            return self.parse_unsupported_caret_selector();
        }
        if self.at(TokenKind::Dot) {
            return self.parse_unsupported_leading_selector();
        }
        let left = self.parse_postfix();
        self.parse_range_tail(left)
    }

    /// The `:` chain of a range, continued from its left operand. Each colon must immediately
    /// follow the previous operand; whitespace after a colon is trivia.
    fn parse_range_tail(&mut self, mut left: SyntaxNode) -> SyntaxNode {
        loop {
            if self.at_trim_range_operator() {
                left = self.parse_unsupported_trim_range(left);
                continue;
            }
            if !self.at(TokenKind::Colon) {
                return left;
            }
            let colon = self.bump();
            self.skip_whitespace();
            let right = self.parse_postfix();
            left = SyntaxNode::new(
                SyntaxKind::RangeExpr,
                vec![
                    SyntaxChild::Node(left),
                    SyntaxChild::Token(colon),
                    SyntaxChild::Node(right),
                ],
            );
        }
    }

    /// Whether the parser stands on a trim-range operator: `.:`, `:.` or `.:.` with no
    /// whitespace inside it.
    fn at_trim_range_operator(&self) -> bool {
        let next = self.tokens.get(self.index + 1).map(Token::kind);
        (self.at(TokenKind::Dot) && next == Some(TokenKind::Colon))
            || (self.at(TokenKind::Colon) && next == Some(TokenKind::Dot))
    }

    /// `left.:right`, `left:.right` or `left.:.right`: an Excel trim reference, which this
    /// grammar does not parse. Its operands stay structured; the operator tokens go unstructured
    /// into the [`SyntaxKind::UnsupportedExpr`], whose diagnostic spans exactly them.
    fn parse_unsupported_trim_range(&mut self, left: SyntaxNode) -> SyntaxNode {
        let mut tokens = Vec::new();
        if self.at(TokenKind::Dot) {
            tokens.push(self.bump());
        }
        tokens.push(self.bump());
        if self.at(TokenKind::Dot) {
            tokens.push(self.bump());
        }
        self.skip_whitespace();
        let right = self.parse_postfix();
        self.unsupported(
            Some(left),
            tokens,
            Some(right),
            "unsupported syntax: trim references (the `.:`, `:.` and `.:.` range operators) are not in the grammar this parser accepts",
        )
    }

    /// The parser stands on a `:` that follows `left` after the whitespace `spaces_before`.
    /// Builds the range the author meant, with the whitespace on either side of the colon as
    /// trivia, and records one [`SyntaxDiagnosticKind::SpacedRangeColon`] whose suggestion is the
    /// range's source without that whitespace.
    fn recover_spaced_range_colon(
        &mut self,
        left: SyntaxNode,
        spaces_before: Vec<Token>,
    ) -> SyntaxNode {
        self.adopt_recovered_whitespace(&spaces_before);
        let colon = self.bump();
        let spaces_after = self.take_whitespace_tokens();
        self.adopt_recovered_whitespace(&spaces_after);
        let right = self.parse_postfix();
        let range = SyntaxNode::new(
            SyntaxKind::RangeExpr,
            vec![
                SyntaxChild::Node(left),
                SyntaxChild::Token(colon),
                SyntaxChild::Node(right),
            ],
        );
        let range = self.parse_range_tail(range);
        let suggested = self.source_text_without_recovered_whitespace(range.span());
        self.diagnostics.push(SyntaxDiagnostic {
            message: format!("whitespace before ':' in range; suggested correction: {suggested}"),
            span: range.span(),
            kind: SyntaxDiagnosticKind::SpacedRangeColon { suggested },
        });
        range
    }

    /// Makes taken whitespace tokens the leading trivia of the next token and records them as
    /// dropped by a recovery.
    fn adopt_recovered_whitespace(&mut self, spaces: &[Token]) {
        for space in spaces {
            self.recovered_whitespace_spans.push(space.span());
            self.pending_trivia
                .push(Trivia::new(space.text().to_string(), space.span()));
        }
    }

    fn parse_postfix(&mut self) -> SyntaxNode {
        let node = self.parse_primary(false);
        self.parse_postfix_from(node)
    }

    fn parse_postfix_from(&mut self, mut node: SyntaxNode) -> SyntaxNode {
        loop {
            if self.at(TokenKind::Hash) {
                let hash = self.bump();
                node = SyntaxNode::new(
                    SyntaxKind::PostfixExpr,
                    vec![SyntaxChild::Node(node), SyntaxChild::Token(hash)],
                );
            } else if self.at(TokenKind::LParen) {
                let args = self.parse_argument_list();
                node = SyntaxNode::new(
                    SyntaxKind::InvokeExpr,
                    vec![SyntaxChild::Node(node), SyntaxChild::Node(args)],
                );
            } else if self.at(TokenKind::Dot) && !self.at_trim_range_operator() {
                node = self.parse_unsupported_member_selector(node);
            } else {
                return node;
            }
        }
    }

    /// `^`, `^^` or `^.name` where an operand starts: never Excel syntax. Its tokens go into one
    /// [`SyntaxKind::UnsupportedExpr`].
    fn parse_unsupported_caret_selector(&mut self) -> SyntaxNode {
        let mut tokens = Vec::new();
        while self.at(TokenKind::Caret) {
            tokens.push(self.bump());
        }
        if self.at(TokenKind::Dot) {
            tokens.push(self.bump());
            self.skip_whitespace();
            if self.at(TokenKind::Identifier) || self.at(TokenKind::Star) {
                tokens.push(self.bump());
            }
        }
        self.unsupported(
            None,
            tokens,
            None,
            "unsupported syntax: a `^` selector cannot start an operand of an Excel formula",
        )
    }

    /// `.name` or `.@name` where an operand starts.
    fn parse_unsupported_leading_selector(&mut self) -> SyntaxNode {
        let tokens = self.take_selector_tokens(false);
        self.unsupported(
            None,
            tokens,
            None,
            "unsupported syntax: a `.` selector at the start of an operand is outside the Excel grammar this parser accepts",
        )
    }

    /// `.name`, `.@name`, `.*` or `.[x]` after an operand.
    fn parse_unsupported_member_selector(&mut self, operand: SyntaxNode) -> SyntaxNode {
        let tokens = self.take_selector_tokens(true);
        self.unsupported(
            Some(operand),
            tokens,
            None,
            "unsupported syntax: a `.` member selector after an operand is outside the Excel grammar this parser accepts",
        )
    }

    /// The `.`, an optional `@` and the selector name, each after optional whitespace.
    fn take_selector_tokens(&mut self, allow_bracketed: bool) -> Vec<Token> {
        let mut tokens = vec![self.bump()];
        self.skip_whitespace();
        if self.at(TokenKind::At) {
            tokens.push(self.bump());
            self.skip_whitespace();
        }
        if self.at(TokenKind::Identifier)
            || self.at(TokenKind::Star)
            || (allow_bracketed && self.at(TokenKind::BracketedQualifier))
        {
            tokens.push(self.bump());
        }
        tokens
    }

    /// An [`SyntaxKind::UnsupportedExpr`] over the operand before `tokens` (if any), `tokens`
    /// and the operand after them (if any), with one
    /// [`SyntaxDiagnosticKind::UnsupportedSyntax`] spanning exactly `tokens`.
    fn unsupported(
        &mut self,
        before: Option<SyntaxNode>,
        tokens: Vec<Token>,
        after: Option<SyntaxNode>,
        message: &str,
    ) -> SyntaxNode {
        let span = match (tokens.first(), tokens.last()) {
            (Some(first), Some(last)) => TextSpan::covering(first.span(), last.span()),
            _ => TextSpan::new(self.insertion_point(), 0),
        };
        self.diagnostics
            .push(SyntaxDiagnostic::unsupported(message, span));
        let mut children: Vec<SyntaxChild> = before.into_iter().map(SyntaxChild::Node).collect();
        children.extend(tokens.into_iter().map(SyntaxChild::Token));
        children.extend(after.into_iter().map(SyntaxChild::Node));
        SyntaxNode::new(SyntaxKind::UnsupportedExpr, children)
    }

    /// One operand. `qualifier_target` is true for the operand after a qualifier's `!`, which
    /// never starts a sheet span: in `Sheet1!A1:Sheet1!B2` the `A1` is a cell, and the `:` is
    /// the range operator between two qualified references.
    fn parse_primary(&mut self, qualifier_target: bool) -> SyntaxNode {
        self.skip_whitespace();
        match self.current().kind() {
            TokenKind::Number => SyntaxNode::new(
                SyntaxKind::NumberLiteralExpr,
                vec![SyntaxChild::Token(self.bump())],
            ),
            TokenKind::StringLiteral => SyntaxNode::new(
                SyntaxKind::StringLiteralExpr,
                vec![SyntaxChild::Token(self.bump())],
            ),
            TokenKind::ErrorLiteral => {
                let error = self.bump();
                if error.error_code() == Some(WorksheetErrorCode::Ref)
                    && matches!(
                        self.current().kind(),
                        TokenKind::Identifier | TokenKind::Number
                    )
                {
                    // `#REF!A1`: the qualifier Excel writes in place of a deleted sheet. The
                    // error literal holds the `!`, and the reference follows it directly.
                    let target = self.parse_primary(true);
                    SyntaxNode::new(
                        SyntaxKind::QualifiedReferenceExpr,
                        vec![SyntaxChild::Token(error), SyntaxChild::Node(target)],
                    )
                } else {
                    SyntaxNode::new(
                        SyntaxKind::ErrorLiteralExpr,
                        vec![SyntaxChild::Token(error)],
                    )
                }
            }
            TokenKind::Identifier | TokenKind::QuotedIdentifier | TokenKind::BracketedQualifier => {
                let name = self.bump();
                if !qualifier_target
                    && matches!(
                        name.kind(),
                        TokenKind::Identifier | TokenKind::QuotedIdentifier
                    )
                    && self.at_sheet_span_tail()
                {
                    self.parse_sheet_span(name)
                } else if self.at(TokenKind::Bang) {
                    let bang = self.bump();
                    self.skip_whitespace();
                    let target = self.parse_primary(true);
                    SyntaxNode::new(
                        SyntaxKind::QualifiedReferenceExpr,
                        vec![
                            SyntaxChild::Token(name),
                            SyntaxChild::Token(bang),
                            SyntaxChild::Node(target),
                        ],
                    )
                } else if name.kind() == TokenKind::Identifier && self.at(TokenKind::LParen) {
                    let args = self.parse_argument_list();
                    SyntaxNode::new(
                        SyntaxKind::CallExpr,
                        vec![SyntaxChild::Token(name), SyntaxChild::Node(args)],
                    )
                } else if name.kind() == TokenKind::QuotedIdentifier {
                    SyntaxNode::new(
                        SyntaxKind::QuotedIdentifierExpr,
                        vec![SyntaxChild::Token(name)],
                    )
                } else {
                    SyntaxNode::new(SyntaxKind::IdentifierExpr, vec![SyntaxChild::Token(name)])
                }
            }
            TokenKind::LParen => {
                let open = self.bump();
                let expr = self.parse_expression(true);
                let close = self.expect(TokenKind::RParen, "expected ')'");
                SyntaxNode::new(
                    SyntaxKind::GroupingExpr,
                    vec![
                        SyntaxChild::Token(open),
                        SyntaxChild::Node(expr),
                        SyntaxChild::Token(close),
                    ],
                )
            }
            TokenKind::LBrace => self.parse_array_literal(),
            TokenKind::Eof => {
                let at = self.insertion_point();
                self.diagnostics.push(SyntaxDiagnostic::message(
                    "expected an expression",
                    TextSpan::new(at, 0),
                ));
                SyntaxNode::empty(SyntaxKind::MissingExpr, at)
            }
            kind => {
                let token = self.bump();
                self.diagnostics.push(SyntaxDiagnostic::message(
                    format!("unexpected token {kind:?}"),
                    token.span(),
                ));
                SyntaxNode::new(SyntaxKind::MissingExpr, vec![SyntaxChild::Token(token)])
            }
        }
    }

    /// `first:last!target` after `first` was taken; the lookahead has confirmed `: name !`. The
    /// span claims the `:` before the range loop can split on it. The target is one primary, so
    /// `Sheet1:Sheet3!A1:B2` becomes a range whose left operand is the span.
    fn parse_sheet_span(&mut self, first: Token) -> SyntaxNode {
        self.skip_whitespace();
        let colon = self.bump();
        self.skip_whitespace();
        let last = self.bump();
        self.skip_whitespace();
        let bang = self.bump();
        self.skip_whitespace();
        let target = self.parse_primary(true);
        SyntaxNode::new(
            SyntaxKind::SheetSpan3DReferenceExpr,
            vec![
                SyntaxChild::Token(first),
                SyntaxChild::Token(colon),
                SyntaxChild::Token(last),
                SyntaxChild::Token(bang),
                SyntaxChild::Node(target),
            ],
        )
    }

    fn parse_argument_list(&mut self) -> SyntaxNode {
        let open = self.expect(TokenKind::LParen, "expected '('");
        let mut children = vec![SyntaxChild::Token(open)];
        self.skip_whitespace();
        while !self.at(TokenKind::RParen) && !self.at(TokenKind::Eof) {
            if self.at(TokenKind::Comma) {
                children.push(SyntaxChild::Node(self.omitted_argument()));
            } else {
                children.push(SyntaxChild::Node(self.parse_expression(false)));
            }
            self.skip_whitespace();
            if !self.at(TokenKind::Comma) {
                break;
            }
            children.push(SyntaxChild::Token(self.bump()));
            self.skip_whitespace();
            if self.at(TokenKind::RParen) {
                children.push(SyntaxChild::Node(self.omitted_argument()));
                break;
            }
        }
        let close = self.expect(TokenKind::RParen, "expected ')'");
        children.push(SyntaxChild::Token(close));
        SyntaxNode::new(SyntaxKind::ArgumentList, children)
    }

    fn parse_array_literal(&mut self) -> SyntaxNode {
        let open = self.expect(TokenKind::LBrace, "expected '{'");
        let mut children = vec![SyntaxChild::Token(open)];
        self.skip_whitespace();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            // Inside an array literal `,` and `;` separate elements and rows, never a union.
            children.push(SyntaxChild::Node(self.parse_expression(false)));
            self.skip_whitespace();
            if self.at(TokenKind::Comma) || self.at(TokenKind::Semicolon) {
                children.push(SyntaxChild::Token(self.bump()));
                self.skip_whitespace();
            } else {
                break;
            }
        }
        let close = self.expect(TokenKind::RBrace, "expected '}'");
        children.push(SyntaxChild::Token(close));
        SyntaxNode::new(SyntaxKind::ArrayLiteralExpr, children)
    }

    fn omitted_argument(&self) -> SyntaxNode {
        SyntaxNode::empty(SyntaxKind::OmittedArgExpr, self.insertion_point())
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }

    /// Where a missing token or an empty node goes: right after the last token taken, before
    /// any whitespace skipped since, so the tree's positions never decrease in tree order.
    fn insertion_point(&self) -> usize {
        self.pending_trivia
            .first()
            .map(|trivia| trivia.span().start)
            .unwrap_or_else(|| self.current().span().start)
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.current().kind() == kind
    }

    /// The kinds of the next `count` tokens that are not whitespace, stopping at the end of
    /// input.
    fn peek_significant_kinds(&self, count: usize) -> Vec<TokenKind> {
        self.tokens[self.index..]
            .iter()
            .map(Token::kind)
            .filter(|kind| *kind != TokenKind::Whitespace)
            .take_while(|kind| *kind != TokenKind::Eof)
            .take(count)
            .collect()
    }

    /// Whether `: name !` follows the name just taken, which makes it the first sheet of a sheet
    /// span. Only an unquoted last sheet is taken: `Sheet1:'Q2 Data'!A1` is not a span here.
    fn at_sheet_span_tail(&self) -> bool {
        matches!(
            self.peek_significant_kinds(3).as_slice(),
            [TokenKind::Colon, TokenKind::Identifier, TokenKind::Bang]
        )
    }

    fn starts_reference_expr(&self) -> bool {
        matches!(
            self.current().kind(),
            TokenKind::Identifier
                | TokenKind::QuotedIdentifier
                | TokenKind::BracketedQualifier
                | TokenKind::Number
                | TokenKind::At
                | TokenKind::Dot
                | TokenKind::LParen
                | TokenKind::LBrace
        )
    }

    /// Takes the current token with the pending whitespace as its leading trivia, reporting
    /// unterminated quoted text. The end of input is taken once, by the root.
    fn bump(&mut self) -> Token {
        let mut token = self.tokens[self.index].clone();
        if self.index + 1 < self.tokens.len() {
            self.index += 1;
        }
        token.set_leading_trivia(std::mem::take(&mut self.pending_trivia));
        let unterminated =
            match token.kind() {
                TokenKind::StringLiteral => (!quoted_text_is_closed(token.text(), '"'))
                    .then_some("unterminated string literal"),
                TokenKind::QuotedIdentifier => (!quoted_text_is_closed(token.text(), '\''))
                    .then_some("unterminated quoted name"),
                _ => None,
            };
        if let Some(message) = unterminated {
            self.diagnostics
                .push(SyntaxDiagnostic::message(message, token.span()));
        }
        token
    }

    /// Takes the current token when it is of `kind`; otherwise reports `message` and returns a
    /// missing token of `kind`, empty at the current token's start.
    fn expect(&mut self, kind: TokenKind, message: &str) -> Token {
        if self.at(kind) {
            return self.bump();
        }
        let at = self.insertion_point();
        self.diagnostics
            .push(SyntaxDiagnostic::message(message, TextSpan::new(at, 0)));
        Token::missing(kind, at)
    }

    fn skip_whitespace(&mut self) {
        while self.at(TokenKind::Whitespace) {
            let token = &self.tokens[self.index];
            self.pending_trivia
                .push(Trivia::new(token.text().to_string(), token.span()));
            self.index += 1;
        }
    }

    /// Takes the whitespace tokens at the current position as tokens (the intersection
    /// operator), not as trivia.
    fn take_whitespace_tokens(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        while self.at(TokenKind::Whitespace) {
            tokens.push(self.tokens[self.index].clone());
            self.index += 1;
        }
        tokens
    }

    /// The source covered by `span` without the whitespace the recoveries dropped.
    fn source_text_without_recovered_whitespace(&self, span: TextSpan) -> String {
        self.tokens
            .iter()
            .filter(|token| token.kind() != TokenKind::Eof)
            .filter(|token| token.span().start >= span.start && token.span().end() <= span.end())
            .filter(|token| !self.recovered_whitespace_spans.contains(&token.span()))
            .map(Token::text)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expression_kind(formula: &str) -> (SyntaxKind, usize) {
        let tree = parse_formula(formula);
        assert_eq!(
            tree.source_text(),
            formula,
            "{formula} must reprint exactly"
        );
        (
            tree.expression().expect("a root has an expression").kind(),
            tree.diagnostics().len(),
        )
    }

    /// The root expression kind and parse-diagnostic count of each formula; each also reprints
    /// exactly.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn root_kinds_and_diagnostic_counts() {
        use SyntaxKind as K;
        let cases: &[(&str, SyntaxKind, usize)] = &[
            ("=1+2", K::BinaryExpr, 0),
            ("=A1", K::IdentifierExpr, 0),
            ("=A1:B2", K::RangeExpr, 0),
            ("=@A1#", K::PrefixExpr, 0),
            ("=SUM(A1,2)", K::CallExpr, 0),
            ("=A1,B2", K::UnionExpr, 0),
            ("=A1 B2", K::IntersectionExpr, 0),
            ("=LAMBDA(x,x+1)(2)", K::InvokeExpr, 0),
            ("=Sheet2!InputValue", K::QualifiedReferenceExpr, 0),
            ("=1:3", K::RangeExpr, 0),
            ("=A:C", K::RangeExpr, 0),
            ("='Annual Data'!1:'Annual Data'!3", K::RangeExpr, 0),
            ("='Annual Data'!A:'Annual Data'!C", K::RangeExpr, 0),
            ("=[Book.xlsx]Sheet2!A1", K::QualifiedReferenceExpr, 0),
            ("=IFS(A1>0,B2,TRUE,C3)", K::CallExpr, 0),
            ("=CHOOSE(1,A1,B2)", K::CallExpr, 0),
            ("=SWITCH(A1,1,B2,C3)", K::CallExpr, 0),
            ("=IFNA(A1,B2)", K::CallExpr, 0),
            ("=Größe*2", K::BinaryExpr, 0),
            ("=Année!A1", K::QualifiedReferenceExpr, 0),
            ("=Umsatz[Betrag]", K::IdentifierExpr, 0),
            ("=Ä1", K::IdentifierExpr, 0),
            ("=Μέγεθος+Данные", K::BinaryExpr, 0),
            ("=売上*2", K::BinaryExpr, 0),
            ("=e\u{0301}tat", K::IdentifierExpr, 0),
            ("=1größe", K::NumberLiteralExpr, 1),
            ("=\u{1F600}*2", K::BinaryExpr, 1),
            ("=A1 : A3", K::RangeExpr, 1),
            ("=A1: A3", K::RangeExpr, 0),
            ("=A1:B2 C1:C3", K::IntersectionExpr, 0),
            ("=A1 (B1)", K::IntersectionExpr, 0),
            ("=myname (A2)", K::IntersectionExpr, 0),
            (
                "=_xlfn.XLOOKUP(2,{1,2,3},{\"a\",\"b\",\"c\"})",
                K::CallExpr,
                0,
            ),
            ("=_xlfn._xlws.FILTER(A1:A4,A1:A4<>\"x\")", K::CallExpr, 0),
            ("=_xlfn.LET(_xlpm.x,1,_xlpm.x+2)", K::CallExpr, 0),
            ("=_xlfn.LAMBDA(_xlpm.x,_xlpm.x+1)(2)", K::InvokeExpr, 0),
            (
                "=_xlfn.LAMBDA(_xlpm.x,_xlop.y,_xlpm.x+IF(_xlfn.ISOMITTED(_xlpm.y),0,_xlpm.y))(1)",
                K::InvokeExpr,
                0,
            ),
            (
                "=_xlfn.LET(_xlpm.f,_xlfn.LAMBDA(_xlpm.x,_xlpm.x+1),_xlpm.f(2))",
                K::CallExpr,
                0,
            ),
            (
                "=_xlfn.BYROW(A5:B6,_xlfn.LAMBDA(_xlpm.r,SUM(_xlpm.r)))",
                K::CallExpr,
                0,
            ),
            ("=_xlfn.NOTAFUNCTION(1)", K::CallExpr, 0),
            ("=_xlws.FILTER({1,2,3},{1,0,1})", K::CallExpr, 0),
            ("=_xlfn.SINGLE(A1)", K::CallExpr, 0),
            ("=TOTAL*2", K::BinaryExpr, 0),
            ("=GRÖSSE*2", K::BinaryExpr, 0),
            ("=İsik*2", K::BinaryExpr, 0),
            ("=LET(Größe,21,GRÖßE*2)", K::CallExpr, 0),
            (
                "=\u{039C}\u{0388}\u{0393}\u{0395}\u{0398}\u{039F}\u{03A3}+\u{0414}\u{0410}\u{041D}\u{041D}\u{042B}\u{0415}",
                K::BinaryExpr,
                0,
            ),
            (
                "=\u{039C}\u{0395}\u{0393}\u{0395}\u{0398}\u{039F}\u{03A3}*2",
                K::BinaryExpr,
                0,
            ),
            ("=LET(Größe,21,GRÖSSE*2)", K::CallExpr, 0),
            ("=Größe*2", K::BinaryExpr, 0),
            ("=A1:A3 (A2:B3)", K::IntersectionExpr, 0),
            ("=(A1:B3) (B2:C4)", K::IntersectionExpr, 0),
            ("=SUM(A1)", K::CallExpr, 0),
            ("=SUM( A1 )", K::CallExpr, 0),
            ("=A1:A3", K::RangeExpr, 0),
            ("=A1:B2:C3", K::RangeExpr, 0),
            ("=@A1:A3", K::PrefixExpr, 0),
            ("=Sheet1:Sheet3!A1:B2", K::RangeExpr, 0),
        ];
        for (formula, kind, diagnostics) in cases {
            assert_eq!(expression_kind(formula), (*kind, *diagnostics), "{formula}");
        }
    }

    /// `SUM (A1)` is an intersection of the name `SUM` and `(A1)` here: refusing it needs the
    /// function catalogue, which this parser does not consult.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn space_before_call_is_an_intersection_without_the_catalogue() {
        assert_eq!(
            expression_kind("=SUM (A1)"),
            (SyntaxKind::IntersectionExpr, 0)
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn spaced_range_colon_suggests_the_range_without_the_spaces() {
        for (formula, span_text, suggested) in [
            ("=A1 : A3", "A1 : A3", "A1:A3"),
            ("=A1 :A3", "A1 :A3", "A1:A3"),
            ("=A1  :  A3", "A1  :  A3", "A1:A3"),
            ("=Sheet1!A1 : A3", "Sheet1!A1 : A3", "Sheet1!A1:A3"),
            ("=A1:B2 : C3", "A1:B2 : C3", "A1:B2:C3"),
            ("=SUM(A1 : A3)", "A1 : A3", "A1:A3"),
            ("=1 : 3", "1 : 3", "1:3"),
        ] {
            let tree = parse_formula(formula);
            assert_eq!(tree.source_text(), formula);
            assert_eq!(tree.diagnostics().len(), 1, "{formula}");
            let diagnostic = &tree.diagnostics()[0];
            let units: Vec<u16> = formula.encode_utf16().collect();
            let covered =
                String::from_utf16(&units[diagnostic.span.start..diagnostic.span.end()]).unwrap();
            assert_eq!(covered, span_text, "{formula}");
            assert_eq!(
                diagnostic.suggested_correction(),
                Some(suggested),
                "{formula}"
            );
            let corrected = format!(
                "{}{}{}",
                String::from_utf16(&units[..diagnostic.span.start]).unwrap(),
                suggested,
                String::from_utf16(&units[diagnostic.span.end()..]).unwrap()
            );
            assert!(
                parse_formula(&corrected).diagnostics().is_empty(),
                "{corrected}"
            );
        }
        let tree = parse_formula("=A1 : B2 : C3");
        assert_eq!(tree.diagnostics().len(), 2);
        assert_eq!(
            tree.diagnostics()[1].suggested_correction(),
            Some("A1:B2:C3")
        );
    }

    /// Chained intersections inside an argument list parse as one left-nested intersection.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn intersections_chain_inside_an_argument_list() {
        let tree = parse_formula("=SUM(A1:C3 B1:B5 A2:C2)");
        assert!(tree.diagnostics().is_empty(), "{:?}", tree.diagnostics());
        let call = tree.expression().unwrap();
        let args = call.child_nodes().next().unwrap();
        let argument = args.child_nodes().next().unwrap();
        assert_eq!(argument.kind(), SyntaxKind::IntersectionExpr);
        assert_eq!(
            argument.child_nodes().next().unwrap().kind(),
            SyntaxKind::IntersectionExpr
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn non_ascii_names_lex_as_one_identifier_each() {
        for name in [
            "Größe",
            "Année",
            "Μέγεθος",
            "Данные",
            "売上",
            "表1",
            "매출",
            "e\u{0301}tat",
            "\u{0915}\u{094D}\u{0937}\u{0947}\u{0924}\u{094D}\u{0930}",
        ] {
            let formula = format!("={name}*2");
            let kinds: Vec<(TokenKind, String)> = lex(&formula)
                .into_iter()
                .filter(|token| token.kind() != TokenKind::Eof)
                .map(|token| (token.kind(), token.text().to_string()))
                .collect();
            assert_eq!(
                kinds,
                vec![
                    (TokenKind::Equals, "=".to_string()),
                    (TokenKind::Identifier, name.to_string()),
                    (TokenKind::Star, "*".to_string()),
                    (TokenKind::Number, "2".to_string()),
                ],
                "{name}"
            );
        }
        // `\` may start a name but not continue one.
        let kinds: Vec<TokenKind> = lex("=a\\b").iter().map(Token::kind).collect();
        assert_eq!(
            kinds,
            [
                TokenKind::Equals,
                TokenKind::Identifier,
                TokenKind::Identifier,
                TokenKind::Eof
            ]
        );
    }

    fn contains_kind(node: &SyntaxNode, kind: SyntaxKind) -> bool {
        node.kind() == kind || node.child_nodes().any(|inner| contains_kind(inner, kind))
    }

    fn significant(formula: &str) -> Vec<(TokenKind, String)> {
        lex(formula)
            .into_iter()
            .filter(|token| !matches!(token.kind(), TokenKind::Eof | TokenKind::Whitespace))
            .map(|token| (token.kind(), token.text().to_string()))
            .collect()
    }

    /// A sheet span claims its `:` only with a trailing `!` and an unquoted last sheet.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn sheet_spans_claim_their_colon_only_before_a_bang() {
        let tree = parse_formula("=Sheet1:Sheet3!A1");
        assert!(tree.diagnostics().is_empty());
        let span = tree.expression().unwrap();
        assert_eq!(span.kind(), SyntaxKind::SheetSpan3DReferenceExpr);
        let tokens: Vec<(TokenKind, &str)> = span
            .children()
            .iter()
            .filter_map(|child| match child {
                SyntaxChild::Token(token) => Some((token.kind(), token.text())),
                SyntaxChild::Node(_) => None,
            })
            .collect();
        assert_eq!(
            tokens,
            [
                (TokenKind::Identifier, "Sheet1"),
                (TokenKind::Colon, ":"),
                (TokenKind::Identifier, "Sheet3"),
                (TokenKind::Bang, "!"),
            ]
        );

        let tree = parse_formula("=Sheet1:Sheet3!A1:B2");
        assert!(tree.diagnostics().is_empty());
        let range = tree.expression().unwrap();
        assert_eq!(range.kind(), SyntaxKind::RangeExpr);
        assert_eq!(
            range.child_nodes().next().unwrap().kind(),
            SyntaxKind::SheetSpan3DReferenceExpr
        );

        for formula in [
            "=A1:A3",
            "=name:name",
            "=Sheet1:Sheet3",
            "=Sheet1!A1:B2",
            "='Annual Data'!A:'Annual Data'!C",
        ] {
            let tree = parse_formula(formula);
            assert!(tree.diagnostics().is_empty(), "{formula}");
            let expr = tree.expression().unwrap();
            assert_eq!(expr.kind(), SyntaxKind::RangeExpr, "{formula}");
            assert!(
                !contains_kind(expr, SyntaxKind::SheetSpan3DReferenceExpr),
                "{formula}"
            );
        }
        assert!(contains_kind(
            parse_formula("=Sheet1!A1:B2").expression().unwrap(),
            SyntaxKind::QualifiedReferenceExpr
        ));

        let tree = parse_formula("='My Sheet':Sheet3!A1");
        assert!(tree.diagnostics().is_empty());
        assert_eq!(
            tree.expression().unwrap().kind(),
            SyntaxKind::SheetSpan3DReferenceExpr
        );
        assert_ne!(
            parse_formula("=Sheet1:'Q2 Data'!A1")
                .expression()
                .unwrap()
                .kind(),
            SyntaxKind::SheetSpan3DReferenceExpr
        );
    }

    /// An unbalanced deep nesting gets one missing-')' diagnostic; quoted names, strings and
    /// bracketed qualifiers keep their token kinds; file prefixes and dotted names lex as one
    /// identifier.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn token_kinds_and_unbalanced_nesting() {
        // An incomplete operand gets untyped diagnostics with no suggested correction.
        let tree = parse_formula("=1+");
        assert!(!tree.diagnostics().is_empty());
        for diagnostic in tree.diagnostics() {
            assert_eq!(diagnostic.kind, SyntaxDiagnosticKind::Message);
            assert_eq!(diagnostic.suggested_correction(), None);
        }

        for name in [
            "_xlfn.XLOOKUP",
            "_XLFN.xlookup",
            "_xlfn._xlws.FILTER",
            "_xlfn.STDEV.P",
            "_xlfn._FV",
            "_xlpm.x",
            "_xlop.y",
            "_xlws.FILTER",
            "_xlfn.NOTAFUNCTION",
        ] {
            assert_eq!(
                significant(&format!("={name}(1)")),
                [
                    (TokenKind::Equals, "=".to_string()),
                    (TokenKind::Identifier, name.to_string()),
                    (TokenKind::LParen, "(".to_string()),
                    (TokenKind::Number, "1".to_string()),
                    (TokenKind::RParen, ")".to_string()),
                ],
                "{name}"
            );
        }

        let source = "=((((((((((((((((1+1)+1)+1)+1)+1)+1)+1)+1)+1)+1)+1)+1)+1)+1)+1)";
        let tree = parse_formula(source);
        assert_eq!(tree.source_text(), source);
        assert_eq!(tree.diagnostics().len(), 1);
        assert_eq!(tree.diagnostics()[0].message, "expected ')'");

        let s = |kind: TokenKind, text: &str| (kind, text.to_string());
        assert_eq!(
            significant("='Année'!A1"),
            [
                s(TokenKind::Equals, "="),
                s(TokenKind::QuotedIdentifier, "'Année'"),
                s(TokenKind::Bang, "!"),
                s(TokenKind::Identifier, "A1"),
            ]
        );
        assert_eq!(
            significant("=\"Größe\""),
            [
                s(TokenKind::Equals, "="),
                s(TokenKind::StringLiteral, "\"Größe\""),
            ]
        );
        assert_eq!(
            significant("=[@金額]"),
            [
                s(TokenKind::Equals, "="),
                s(TokenKind::BracketedQualifier, "[@金額]"),
            ]
        );
        assert_eq!(
            significant("=[Buch.xlsx]Übersicht!A1"),
            [
                s(TokenKind::Equals, "="),
                s(TokenKind::BracketedQualifier, "[Buch.xlsx]Übersicht"),
                s(TokenKind::Bang, "!"),
                s(TokenKind::Identifier, "A1"),
            ]
        );
        assert_eq!(
            significant("=Größe_2.Wert"),
            [
                s(TokenKind::Equals, "="),
                s(TokenKind::Identifier, "Größe_2.Wert"),
            ]
        );
        assert_eq!(
            significant("=_xlfn._xlws.FILTER(_xlpm.x)"),
            [
                s(TokenKind::Equals, "="),
                s(TokenKind::Identifier, "_xlfn._xlws.FILTER"),
                s(TokenKind::LParen, "("),
                s(TokenKind::Identifier, "_xlpm.x"),
                s(TokenKind::RParen, ")"),
            ]
        );
    }

    /// `Kind(child child ...)` with tokens as their text, trivia left out.
    fn shape(node: &SyntaxNode) -> String {
        let parts: Vec<String> = node
            .children()
            .iter()
            .map(|child| match child {
                SyntaxChild::Node(inner) => shape(inner),
                SyntaxChild::Token(token) => token.text().to_string(),
            })
            .collect();
        format!("{:?}({})", node.kind(), parts.join(" "))
    }

    /// The UTF-16 code units of `source` that `span` covers.
    fn covered(source: &str, span: TextSpan) -> String {
        let units: Vec<u16> = source.encode_utf16().collect();
        String::from_utf16(&units[span.start..span.end()]).unwrap()
    }

    /// The first argument of the call that is the formula's expression.
    fn first_argument(tree: &SyntaxTree) -> &SyntaxNode {
        let call = tree.expression().unwrap();
        assert_eq!(call.kind(), SyntaxKind::CallExpr);
        call.child_nodes()
            .next()
            .unwrap()
            .child_nodes()
            .next()
            .unwrap()
    }

    /// Each trim-reference form gets exactly one unsupported-syntax diagnostic naming trim
    /// references, spanning the trim operator; its operands stay references, `A1.` is never
    /// lexed as a name, no range node is built over the trim operator, and the text reprints.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn trim_references_are_refused_as_unsupported_syntax() {
        for (formula, operator, argument) in [
            (
                "=SUM(A1.:B10)",
                ".:",
                "UnsupportedExpr(IdentifierExpr(A1) . : IdentifierExpr(B10))",
            ),
            (
                "=SUM(A1:.B10)",
                ":.",
                "UnsupportedExpr(IdentifierExpr(A1) : . IdentifierExpr(B10))",
            ),
            (
                "=SUM(A1.:.B10)",
                ".:.",
                "UnsupportedExpr(IdentifierExpr(A1) . : . IdentifierExpr(B10))",
            ),
            (
                "=SUM(A:.A)",
                ":.",
                "UnsupportedExpr(IdentifierExpr(A) : . IdentifierExpr(A))",
            ),
            (
                "=SUM(1:.3)",
                ":.",
                "UnsupportedExpr(NumberLiteralExpr(1) : . NumberLiteralExpr(3))",
            ),
            (
                "=SUM(1.:3)",
                ".:",
                "UnsupportedExpr(NumberLiteralExpr(1) . : NumberLiteralExpr(3))",
            ),
            (
                "=SUM(Sheet1!A1.:B10)",
                ".:",
                "UnsupportedExpr(QualifiedReferenceExpr(Sheet1 ! IdentifierExpr(A1)) . : IdentifierExpr(B10))",
            ),
        ] {
            let tree = parse_formula(formula);
            assert_eq!(tree.source_text(), formula);
            assert_eq!(
                tree.diagnostics().len(),
                1,
                "{formula}: {:?}",
                tree.diagnostics()
            );
            let diagnostic = &tree.diagnostics()[0];
            assert_eq!(
                diagnostic.kind,
                SyntaxDiagnosticKind::UnsupportedSyntax,
                "{formula}"
            );
            assert!(
                diagnostic.message.contains("trim references"),
                "{formula}: {}",
                diagnostic.message
            );
            assert_eq!(covered(formula, diagnostic.span), operator, "{formula}");
            assert_eq!(shape(first_argument(&tree)), argument, "{formula}");
            assert!(
                !contains_kind(tree.root(), SyntaxKind::RangeExpr),
                "{formula}"
            );
            assert!(
                lex(formula)
                    .iter()
                    .all(|token| token.kind() == TokenKind::Dot || !token.text().contains('.')),
                "{formula}: a '.' of the trim operator was lexed into another token"
            );
        }
        // Names with an inner '.' and decimals are untouched.
        for formula in ["=SUM(A1.B:B10)", "=SUM(1.5,A1:B10)", "=Base.Value"] {
            let tree = parse_formula(formula);
            assert!(
                tree.diagnostics().is_empty(),
                "{formula}: {:?}",
                tree.diagnostics()
            );
        }
        // The recorded limit: after whitespace, the trim operator is refused with the
        // spaced-colon and untyped diagnostics, not named as a trim reference.
        let formula = "=SUM(A1 :.B10)";
        let tree = parse_formula(formula);
        assert_eq!(tree.source_text(), formula);
        let kinds: Vec<&SyntaxDiagnosticKind> =
            tree.diagnostics().iter().map(|d| &d.kind).collect();
        assert!(
            kinds
                .iter()
                .any(|kind| matches!(kind, SyntaxDiagnosticKind::SpacedRangeColon { .. })),
            "{kinds:?}"
        );
        assert!(
            tree.diagnostics()
                .iter()
                .any(|d| d.message == "unexpected token Dot"),
            "{:?}",
            tree.diagnostics()
        );
        assert!(
            !kinds.contains(&&SyntaxDiagnosticKind::UnsupportedSyntax),
            "{kinds:?}"
        );
    }

    /// The operand after a qualifier's `!` never starts a sheet span, so a range whose ends are
    /// both qualified is a range of two qualified references.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn qualified_range_ends_are_two_qualified_references() {
        for (formula, argument) in [
            (
                "=SUM(Sheet1!A1:Sheet1!B2)",
                "RangeExpr(QualifiedReferenceExpr(Sheet1 ! IdentifierExpr(A1)) : QualifiedReferenceExpr(Sheet1 ! IdentifierExpr(B2)))",
            ),
            (
                "=SUM(Sheet1!A:Sheet1!C)",
                "RangeExpr(QualifiedReferenceExpr(Sheet1 ! IdentifierExpr(A)) : QualifiedReferenceExpr(Sheet1 ! IdentifierExpr(C)))",
            ),
            (
                "=SUM('Annual Data'!A:'Annual Data'!C)",
                "RangeExpr(QualifiedReferenceExpr('Annual Data' ! IdentifierExpr(A)) : QualifiedReferenceExpr('Annual Data' ! IdentifierExpr(C)))",
            ),
            (
                "=SUM(Sheet1:Sheet3!A1:Sheet3!B2)",
                "RangeExpr(SheetSpan3DReferenceExpr(Sheet1 : Sheet3 ! IdentifierExpr(A1)) : QualifiedReferenceExpr(Sheet3 ! IdentifierExpr(B2)))",
            ),
        ] {
            let tree = parse_formula(formula);
            assert_eq!(tree.source_text(), formula);
            assert!(
                tree.diagnostics().is_empty(),
                "{formula}: {:?}",
                tree.diagnostics()
            );
            assert_eq!(shape(first_argument(&tree)), argument, "{formula}");
        }
    }

    /// `#REF!` directly followed by a reference is the qualifier of a deleted sheet; any other
    /// error literal, and `#REF!` on its own, stays an error literal.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn deleted_sheet_ref_qualifies_a_reference() {
        for (formula, expression) in [
            (
                "=#REF!A1",
                "QualifiedReferenceExpr(#REF! IdentifierExpr(A1))",
            ),
            (
                "=#ref!$B$2",
                "QualifiedReferenceExpr(#ref! IdentifierExpr($B$2))",
            ),
            (
                "=SUM(#REF!A1:B2)",
                "CallExpr(SUM ArgumentList(( RangeExpr(QualifiedReferenceExpr(#REF! IdentifierExpr(A1)) : IdentifierExpr(B2)) )))",
            ),
            (
                "=SUM(#REF!1:3)",
                "CallExpr(SUM ArgumentList(( RangeExpr(QualifiedReferenceExpr(#REF! NumberLiteralExpr(1)) : NumberLiteralExpr(3)) )))",
            ),
            ("=#REF!", "ErrorLiteralExpr(#REF!)"),
            (
                "=Sheet2!#REF!",
                "QualifiedReferenceExpr(Sheet2 ! ErrorLiteralExpr(#REF!))",
            ),
            (
                "=SUM(#REF!,A1)",
                "CallExpr(SUM ArgumentList(( ErrorLiteralExpr(#REF!) , IdentifierExpr(A1) )))",
            ),
        ] {
            let tree = parse_formula(formula);
            assert_eq!(tree.source_text(), formula);
            assert!(
                tree.diagnostics().is_empty(),
                "{formula}: {:?}",
                tree.diagnostics()
            );
            assert_eq!(shape(tree.expression().unwrap()), expression, "{formula}");
        }
        let tree = parse_formula("=#REF!A1");
        let qualifier = tree.tokens()[1];
        assert_eq!(qualifier.kind(), TokenKind::ErrorLiteral);
        assert_eq!(qualifier.error_code(), Some(WorksheetErrorCode::Ref));

        let tree = parse_formula("=#DIV/0!A1");
        assert_eq!(tree.source_text(), "=#DIV/0!A1");
        assert_eq!(
            shape(tree.expression().unwrap()),
            "ErrorLiteralExpr(#DIV/0!)"
        );
        assert_eq!(tree.diagnostics().len(), 1, "{:?}", tree.diagnostics());
    }

    /// The texts of the tree's tokens, end of input excluded.
    fn token_texts(tree: &SyntaxTree) -> Vec<String> {
        tree.tokens()
            .into_iter()
            .filter(|token| token.kind() != TokenKind::Eof)
            .map(|token| token.text().to_string())
            .collect()
    }

    fn leading_text(token: &Token) -> String {
        token
            .leading_trivia()
            .iter()
            .map(|trivia| trivia.text())
            .collect()
    }

    /// Whitespace belongs to the token after it, once: in `=SUM( A1 ) ` the space before `A1`
    /// is `A1`'s leading trivia, the space after it is `)`'s, and the final space is the end of
    /// input's.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn whitespace_is_the_leading_trivia_of_the_next_token() {
        let tree = parse_formula("=SUM( A1 ) ");
        let tokens = tree.tokens();
        let texts: Vec<(&str, String)> = tokens
            .iter()
            .map(|token| (token.text(), leading_text(token)))
            .collect();
        assert_eq!(
            texts,
            [
                ("=", String::new()),
                ("SUM", String::new()),
                ("(", String::new()),
                ("A1", " ".to_string()),
                (")", " ".to_string()),
                ("", " ".to_string()),
            ]
        );
        assert_eq!(tokens.last().unwrap().kind(), TokenKind::Eof);
    }

    /// Editor inputs, complete or not, reprint exactly; leading whitespace is the first token's
    /// trivia; unexpected trailing tokens stay in the tree with a diagnostic; whitespace between
    /// references is the intersection operator's token, so only the other tokens are compared.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn editor_inputs_reprint_and_keep_their_tokens() {
        for text in [
            "",
            " ",
            "\t",
            "\n",
            "\n=aaa",
            "= a a",
            "=A A",
            "=SUM( A1 ) ",
            "=A1   B1",
            "=a\tb",
            "=1)",
            "=SUM(1,2))",
            "=",
            "=(",
            "=A1+",
            "=@A1#",
            "=\"a b\"",
            "=#VALUE!",
            "=A1&\" text \"",
            "=1 @ 2",
        ] {
            assert_eq!(parse_formula(text).source_text(), text, "{text:?}");
        }

        for (text, leading, expected) in [
            ("\n=aaa", "\n", &["=", "aaa"][..]),
            ("\n\n=aaa", "\n\n", &["=", "aaa"][..]),
            (" =aaa", " ", &["=", "aaa"][..]),
            ("\t=aaa", "\t", &["=", "aaa"][..]),
            (
                "\n=SUM(1,2)",
                "\n",
                &["=", "SUM", "(", "1", ",", "2", ")"][..],
            ),
        ] {
            let tree = parse_formula(text);
            assert_eq!(token_texts(&tree), expected, "{text:?}");
            let first = tree.tokens()[0];
            assert_eq!(first.kind(), TokenKind::Equals, "{text:?}");
            assert_eq!(first.span(), TextSpan::new(leading.len(), 1), "{text:?}");
            assert_eq!(leading_text(first), leading, "{text:?}");
        }

        for (text, expected) in [
            ("=1)", &["=", "1", ")"][..]),
            (
                "=SUM(1,2))",
                &["=", "SUM", "(", "1", ",", "2", ")", ")"][..],
            ),
            ("=1 + 2)", &["=", "1", "+", "2", ")"][..]),
        ] {
            let tree = parse_formula(text);
            assert_eq!(token_texts(&tree), expected, "{text:?}");
            assert!(
                tree.diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.message.contains("unexpected trailing token")),
                "{text:?}: {:?}",
                tree.diagnostics()
            );
        }

        for (text, expected) in [
            ("= a a", &["=", "a", "a"][..]),
            ("=A A", &["=", "A", "A"][..]),
            ("= a b c d", &["=", "a", "b", "c", "d"][..]),
            ("=A1 B1", &["=", "A1", "B1"][..]),
            ("= ABC DEF", &["=", "ABC", "DEF"][..]),
            ("=A1   B1", &["=", "A1", "B1"][..]),
            ("=a\tb", &["=", "a", "b"][..]),
        ] {
            let tree = parse_formula(text);
            assert_eq!(tree.source_text(), text, "{text:?}");
            let significant: Vec<String> = tree
                .tokens()
                .into_iter()
                .filter(|token| !matches!(token.kind(), TokenKind::Eof | TokenKind::Whitespace))
                .map(|token| token.text().to_string())
                .collect();
            assert_eq!(significant, expected, "{text:?}");
        }
    }
}
