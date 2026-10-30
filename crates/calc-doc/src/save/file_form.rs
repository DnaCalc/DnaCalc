//! The file form of a formula a user authored: the text an `<f>` element holds, and whether Excel
//! stores the formula as a dynamic array. Both rules are measured on Excel 16.0 build 20430
//! (`tests/fixtures/d1/`: `measure-prefixes.ps1`, `measure-array-positions.ps1` and
//! `measure-file-form.ps1`, whose outputs the d1_file_form tests compare with this module).
//!
//! Text:
//! - a function Excel added after its 2007 file format is written with `_xlfn.` before its name,
//!   and the worksheet-only dynamic-array functions FILTER and SORT with `_xlfn._xlws.`;
//! - a LET or LAMBDA parameter is written with `_xlpm.` (at its declaration, at each use in its
//!   scope, and before a call of a LET-bound function); an optional LAMBDA parameter `[y]` as
//!   `_xlop.y`;
//! - the spilled-range operator `A1#` is written `_xlfn.ANCHORARRAY(A1)`, and the implicit
//!   intersection operator `@x` is written `_xlfn.SINGLE(x)`, except at the top of the formula,
//!   where it is dropped and the formula is stored as an ordinary one (Excel's legacy formulas
//!   intersect implicitly);
//! - a function named where a LAMBDA value is taken (`GROUPBY(A1:A3,B1:B3,SUM)`, `=SUM` alone) is
//!   written `_xleta.` before its name and no other prefix (`_xleta.STDEV.S`, `_xleta.CONCAT`),
//!   and is no array (dc-h9u, Excel 16.0.20527 through COM on 2026-10-30, 11 formulas entered
//!   through `Range.Formula2` and saved);
//! - a name that already carries a file prefix (`_xlfn.`, `_xlpm.`) is left as written;
//! - a reference to another workbook named by its file name, with or without its folder
//!   (`[Book2.xlsx]Sheet1!A1`, `'C:\Data\[Book2.xlsx]Sheet1'!A1`), is written by the number of
//!   the workbook's link to that file (`[1]Sheet1!A1`), as Excel 16.0.20527 stores a formula typed
//!   so (dc-ylr, 2026-10-30), when such a link exists ([`file_form_with_links`]); otherwise it is
//!   left as written.
//!
//! Dynamic array: Excel stores an entered formula as a dynamic-array formula when the formula
//! could give more than one value: when its value is a multi-cell reference or an array (a range,
//! a union or intersection, an array constant, a spilled range, a name that is not a constant), or
//! when such an operand meets an operator, a function argument that takes a single value (the
//! measured positions), or a function that returns an array whatever its arguments (SEQUENCE,
//! FILTER, INDIRECT, OFFSET with a height or width, ...; the measured base calls). INDEX with a
//! row or column number of 0 and a call of a LAMBDA are dynamic too. A function Excel does not
//! know is stored as a dynamic array (measured on unknown names).

#[path = "file_form_table.rs"]
mod table;

use calc_doc_model::{DefinedName, FormulaText};
use calc_fml::syntax::{SyntaxChild, SyntaxKind, SyntaxNode, Token, TokenKind, parse_formula};
use std::collections::BTreeMap;
use table::FUNCTIONS;

/// The prefix Excel writes before a function's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Prefix {
    Bare,
    Xlfn,
    XlfnXlws,
}

impl Prefix {
    fn text(self) -> &'static str {
        match self {
            Prefix::Bare => "",
            Prefix::Xlfn => "_xlfn.",
            Prefix::XlfnXlws => "_xlfn._xlws.",
        }
    }
}

/// One function's measured file form.
pub(super) struct FunctionFileForm {
    pub(super) name: &'static str,
    pub(super) prefix: Prefix,
    /// The argument count of the first base call measured.
    pub(super) first_count: usize,
    /// Whether the call with `first_count + i` single-cell arguments is stored as a dynamic
    /// array.
    pub(super) bases: &'static [bool],
    /// Whether a range at argument position `i` makes the call a dynamic array; a position past
    /// the end takes the last one's.
    pub(super) positions: &'static [bool],
}

fn function(name: &str) -> Option<&'static FunctionFileForm> {
    let upper = name.to_ascii_uppercase();
    FUNCTIONS
        .binary_search_by(|entry| entry.name.as_bytes().cmp(upper.as_bytes()))
        .ok()
        .map(|at| &FUNCTIONS[at])
}

/// The name a call of the function `name` is written with in a formula's file form: with the
/// prefix Excel writes before it (`_xlfn.XLOOKUP`, `_xlfn._xlws.FILTER`, `ISO.CEILING`), or `None`
/// for a name that is not one of the functions measured.
pub(crate) fn function_file_name(name: &str) -> Option<String> {
    function(name).map(|entry| format!("{}{}", entry.prefix.text(), entry.name))
}

/// A formula in file form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileForm {
    /// The text an `<f>` element holds.
    pub text: Vec<u16>,
    /// Whether Excel stores the formula as a dynamic-array formula.
    pub dynamic: bool,
}

/// Why a formula has no file form here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileFormError {
    /// The text is not valid UTF-16 or the parser reads it with a diagnostic.
    Unreadable,
    /// The formula nests deeper than the converter walks ([`MAX_DEPTH`]).
    TooDeep,
}

/// The deepest nesting of the syntax tree the converter walks.
pub const MAX_DEPTH: usize = 512;

/// What the walk learnt of an expression.
#[derive(Clone, Copy, Default)]
struct Shape {
    /// The expression may hold more than one value.
    multi: bool,
}

struct Walk<'n> {
    names: &'n [DefinedName],
    /// The workbook's links to other workbooks: each link's number and its target's file name in
    /// upper case (dc-ylr).
    links: &'n [(u32, String)],
    /// Edits of the `=`-prefixed source, as (start, end, replacement) in UTF-16 code units.
    edits: Vec<(usize, usize, String)>,
    dynamic: bool,
    /// The LET and LAMBDA parameters in scope, innermost last: upper-case name and whether its
    /// value may hold more than one value.
    scopes: Vec<BTreeMap<String, bool>>,
}

/// Whether `text` is a single-cell A1 reference (`B3`, `$B$3`).
fn is_cell(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = 0;
    if bytes.get(at) == Some(&b'$') {
        at += 1;
    }
    let letters = bytes[at..]
        .iter()
        .take_while(|b| b.is_ascii_alphabetic())
        .count();
    if !(1..=3).contains(&letters) {
        return false;
    }
    at += letters;
    if bytes.get(at) == Some(&b'$') {
        at += 1;
    }
    let digits = &bytes[at..];
    !digits.is_empty() && digits.iter().all(u8::is_ascii_digit) && digits[0] != b'0'
}

/// Whether a defined name's formula is a constant (a number, a text, a logical or an error).
fn is_constant(formula: &str) -> bool {
    let text = formula.trim();
    let text = text.strip_prefix('=').unwrap_or(text);
    text.parse::<f64>().is_ok()
        || (text.starts_with('"') && text.ends_with('"') && text.len() >= 2)
        || text.eq_ignore_ascii_case("TRUE")
        || text.eq_ignore_ascii_case("FALSE")
        || text.starts_with('#')
}

fn has_prefix(name: &str) -> bool {
    let lower = name.get(..5).map(str::to_ascii_lowercase);
    matches!(
        lower.as_deref(),
        Some("_xlfn" | "_xlpm" | "_xlop" | "_xlws" | "_xlud" | "_xlet")
    )
}

fn first_token(node: &SyntaxNode) -> Option<&Token> {
    node.children().iter().find_map(|child| match child {
        SyntaxChild::Token(token) => Some(token),
        SyntaxChild::Node(_) => None,
    })
}

/// The argument nodes of an argument list.
fn arguments(list: &SyntaxNode) -> Vec<&SyntaxNode> {
    list.child_nodes().collect()
}

impl Walk<'_> {
    fn parameter(&self, name: &str) -> Option<bool> {
        let upper = name.to_uppercase();
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(&upper).copied())
    }

    fn insert(&mut self, at: usize, text: &str) {
        self.edits.push((at, at, text.to_owned()));
    }

    /// dc-ylr: the qualifier of `node` written with the number of the workbook's link to the
    /// book it names by file name (`[Book2.xlsx]Sheet1` as `[1]Sheet1`, a folder before it
    /// dropped); quoted when the sheet's name needs it, as Excel writes `'[1]My Sheet'!A1`.
    fn number_external_book(&mut self, node: &SyntaxNode) {
        let Some(SyntaxChild::Token(qualifier)) = node.children().first() else {
            return;
        };
        let (quoted, text) = match qualifier.kind() {
            TokenKind::BracketedQualifier => (false, qualifier.text().to_owned()),
            TokenKind::QuotedIdentifier => {
                let inner = qualifier.text();
                let inner = inner
                    .strip_prefix('\'')
                    .and_then(|t| t.strip_suffix('\''))
                    .unwrap_or(inner);
                (true, inner.to_owned())
            }
            _ => return,
        };
        let Some((folder, rest)) = text.split_once('[') else {
            return;
        };
        let Some((book, sheet)) = rest.split_once(']') else {
            return;
        };
        if sheet.is_empty() || (folder.is_empty() && book.parse::<u32>().is_ok()) {
            return;
        }
        let wanted = book.to_uppercase();
        let Some(&(number, _)) = self.links.iter().find(|(_, name)| *name == wanted) else {
            return;
        };
        let plain = sheet
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && sheet.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.');
        let written = if plain {
            // A quoted name doubles its quotes; a plain one holds none.
            format!("[{number}]{sheet}")
        } else if quoted {
            format!("'[{number}]{sheet}'")
        } else {
            format!("'[{number}]{}'", sheet.replace('\'', "''"))
        };
        let span = qualifier.span();
        self.edits.push((span.start, span.end(), written));
    }

    fn name_is_multi(&self, text: &str) -> bool {
        if text.eq_ignore_ascii_case("TRUE") || text.eq_ignore_ascii_case("FALSE") {
            return false;
        }
        if is_cell(text) {
            return false;
        }
        match self
            .names
            .iter()
            .find(|name| name.name.eq_ignore_ascii_case(text))
        {
            Some(name) => !is_constant(&name.formula),
            // A name the workbook does not define, a whole column (`A`), or a structured
            // reference: possibly more than one value.
            None => true,
        }
    }

    fn walk(&mut self, node: &SyntaxNode, depth: usize) -> Result<Shape, FileFormError> {
        if depth > MAX_DEPTH {
            return Err(FileFormError::TooDeep);
        }
        let multi = |multi| Ok(Shape { multi });
        match node.kind() {
            SyntaxKind::NumberLiteralExpr
            | SyntaxKind::StringLiteralExpr
            | SyntaxKind::ErrorLiteralExpr
            | SyntaxKind::OmittedArgExpr
            | SyntaxKind::MissingExpr => multi(false),
            SyntaxKind::ArrayLiteralExpr => {
                for child in node.child_nodes() {
                    self.walk(child, depth + 1)?;
                }
                multi(true)
            }
            SyntaxKind::IdentifierExpr | SyntaxKind::QuotedIdentifierExpr => {
                let tokens = node.tokens();
                if tokens
                    .iter()
                    .any(|token| token.kind() == TokenKind::BracketedQualifier)
                {
                    return multi(true);
                }
                let Some(token) = tokens.first() else {
                    return multi(false);
                };
                if token.kind() == TokenKind::Identifier
                    && let Some(parameter) = self.parameter(token.text())
                {
                    self.insert(token.span().start, "_xlpm.");
                    return multi(parameter);
                }
                if token.kind() == TokenKind::Identifier
                    && tokens.len() == 1
                    && !is_cell(token.text())
                    && !token.text().eq_ignore_ascii_case("TRUE")
                    && !token.text().eq_ignore_ascii_case("FALSE")
                    && function(token.text()).is_some()
                    && !self
                        .names
                        .iter()
                        .any(|name| name.name.eq_ignore_ascii_case(token.text()))
                {
                    self.insert(token.span().start, "_xleta.");
                    return multi(false);
                }
                multi(self.name_is_multi(token.text()))
            }
            SyntaxKind::QualifiedReferenceExpr | SyntaxKind::SheetSpan3DReferenceExpr => {
                if node.kind() == SyntaxKind::QualifiedReferenceExpr {
                    self.number_external_book(node);
                }
                let mut shape = Shape::default();
                for child in node.child_nodes() {
                    shape = self.walk(child, depth + 1)?;
                }
                // A qualified name or cell keeps its own shape.
                multi(shape.multi)
            }
            SyntaxKind::RangeExpr | SyntaxKind::UnionExpr | SyntaxKind::IntersectionExpr => {
                for child in node.child_nodes() {
                    self.walk(child, depth + 1)?;
                }
                multi(true)
            }
            SyntaxKind::GroupingExpr => {
                let mut shape = Shape::default();
                for child in node.child_nodes() {
                    shape = self.walk(child, depth + 1)?;
                }
                Ok(shape)
            }
            SyntaxKind::BinaryExpr => {
                // A chain of left-associative operators (`1+1+...+1`) is walked along its left
                // spine, not by recursion, so a long chain is not a deep walk.
                let mut rights = Vec::new();
                let mut current = node;
                while current.kind() == SyntaxKind::BinaryExpr {
                    let mut operands = current.child_nodes();
                    let (left, right) = (operands.next(), operands.next());
                    rights.extend(right);
                    match left {
                        Some(left) => current = left,
                        None => break,
                    }
                }
                let mut any = current.kind() != SyntaxKind::BinaryExpr
                    && self.walk(current, depth + 1)?.multi;
                for right in rights.iter().rev() {
                    any |= self.walk(right, depth + 1)?.multi;
                }
                if any {
                    self.dynamic = true;
                }
                multi(any)
            }
            SyntaxKind::PrefixExpr => {
                let operator = first_token(node).map(|token| (token.kind(), token.span()));
                let operand = node.child_nodes().next();
                match (operator, operand) {
                    (Some((TokenKind::At, span)), Some(operand)) => {
                        let shape = self.walk(operand, depth + 1)?;
                        let _ = shape;
                        self.edits
                            .push((span.start, span.end(), "_xlfn.SINGLE(".to_owned()));
                        self.insert(operand.span().end(), ")");
                        multi(false)
                    }
                    (_, Some(operand)) => {
                        let shape = self.walk(operand, depth + 1)?;
                        if shape.multi {
                            self.dynamic = true;
                        }
                        Ok(shape)
                    }
                    (_, None) => multi(false),
                }
            }
            SyntaxKind::PostfixExpr => {
                let operand = node.child_nodes().next();
                let operator = node.children().iter().rev().find_map(|child| match child {
                    SyntaxChild::Token(token) => Some((token.kind(), token.span())),
                    SyntaxChild::Node(_) => None,
                });
                let Some(operand) = operand else {
                    return multi(false);
                };
                if let Some((TokenKind::Hash, span)) = operator {
                    // Inserted before the operand's own edits at the same place.
                    self.insert(operand.span().start, "_xlfn.ANCHORARRAY(");
                    self.walk(operand, depth + 1)?;
                    self.edits.push((span.start, span.end(), ")".to_owned()));
                    return multi(true);
                }
                let shape = self.walk(operand, depth + 1)?;
                match operator {
                    _ => {
                        if shape.multi {
                            self.dynamic = true;
                        }
                        Ok(shape)
                    }
                }
            }
            SyntaxKind::InvokeExpr => {
                for child in node.child_nodes() {
                    match child.kind() {
                        SyntaxKind::ArgumentList => {
                            for argument in arguments(child) {
                                self.walk(argument, depth + 1)?;
                            }
                        }
                        _ => {
                            self.walk(child, depth + 1)?;
                        }
                    }
                }
                self.dynamic = true;
                multi(true)
            }
            SyntaxKind::CallExpr => self.call(node, depth),
            // A trim reference (dc-h9u) as Excel 16.0.20527 saves it: `C1:.C20` as
            // `_xlfn._TRO_TRAILING(C1:C20)`, `C1.:C20` as `_xlfn._TRO_LEADING(C1:C20)` and
            // `C1.:.C20` as `_xlfn._TRO_ALL(C1:C20)`.
            SyntaxKind::TrimRangeExpr => {
                let tokens: Vec<&calc_fml::syntax::Token> = node
                    .children()
                    .iter()
                    .filter_map(|child| match child {
                        SyntaxChild::Token(token) => Some(token),
                        SyntaxChild::Node(_) => None,
                    })
                    .collect();
                let operator: String = tokens.iter().map(|token| token.text()).collect();
                let function = match operator.as_str() {
                    ".:" => "_xlfn._TRO_LEADING(",
                    ":." => "_xlfn._TRO_TRAILING(",
                    ".:." => "_xlfn._TRO_ALL(",
                    _ => return Err(FileFormError::Unreadable),
                };
                let (Some(first), Some(last)) = (tokens.first(), tokens.last()) else {
                    return Err(FileFormError::Unreadable);
                };
                self.insert(node.span().start, function);
                for child in node.child_nodes() {
                    self.walk(child, depth + 1)?;
                }
                self.edits
                    .push((first.span().start, last.span().end(), ":".to_owned()));
                self.insert(node.span().end(), ")");
                multi(true)
            }
            SyntaxKind::ArgumentList => {
                for argument in arguments(node) {
                    self.walk(argument, depth + 1)?;
                }
                multi(false)
            }
            SyntaxKind::FormulaRoot | SyntaxKind::UnsupportedExpr => {
                let mut shape = Shape::default();
                for child in node.child_nodes() {
                    shape = self.walk(child, depth + 1)?;
                }
                Ok(shape)
            }
        }
    }

    fn call(&mut self, node: &SyntaxNode, depth: usize) -> Result<Shape, FileFormError> {
        let Some(name) = first_token(node) else {
            return Ok(Shape { multi: true });
        };
        let list = node
            .child_nodes()
            .next()
            .expect("a call holds its argument list");
        let arguments = arguments(list);
        let text = name.text();
        // A call of a LET-bound function.
        if self.parameter(text).is_some() {
            self.insert(name.span().start, "_xlpm.");
            for argument in &arguments {
                self.walk(argument, depth + 1)?;
            }
            self.dynamic = true;
            return Ok(Shape { multi: true });
        }
        let prefixed = has_prefix(text);
        let plain = if prefixed {
            text.rsplit('.').next().unwrap_or(text)
        } else {
            text
        };
        let upper = plain.to_ascii_uppercase();
        if upper == "LET" || upper == "LAMBDA" {
            if !prefixed {
                self.insert(name.span().start, "_xlfn.");
            }
            return self.binding(&upper, &arguments, depth);
        }
        let entry = function(plain);
        if !prefixed && let Some(entry) = entry {
            let prefix = entry.prefix.text();
            if !prefix.is_empty() {
                self.insert(name.span().start, prefix);
            }
        }
        let mut shapes = Vec::with_capacity(arguments.len());
        for argument in &arguments {
            shapes.push(self.walk(argument, depth + 1)?);
        }
        let Some(entry) = entry else {
            // A function Excel does not know may return an array (measured: an unknown
            // function's call alone is stored as a dynamic array).
            return Ok(Shape { multi: true });
        };
        let count = arguments.len();
        let base = if entry.bases.is_empty() {
            false
        } else {
            let at = count.saturating_sub(entry.first_count);
            entry.bases[at.min(entry.bases.len() - 1)]
        };
        let mut multi = base;
        // INDEX of a range at row and column numbers that are non-zero number literals gives one
        // cell, so its range does not make it a dynamic array (measured: INDEX(A1:A3,1) is
        // ordinary, INDEX(A1:A3,0) and INDEX(A1:A3,Z1) are dynamic).
        let literal_index = upper == "INDEX"
            && arguments.len() >= 2
            && arguments.iter().skip(1).take(2).all(|argument| {
                argument.kind() == SyntaxKind::NumberLiteralExpr
                    && argument
                        .text()
                        .trim()
                        .parse::<f64>()
                        .is_ok_and(|n| n != 0.0)
            });
        // XLOOKUP returns one cell of its return array when its lookup array is a range of the
        // same orientation; the measured position flag (a range there beside a single-cell lookup
        // array) is the case where it returns a whole row or column (measured:
        // XLOOKUP(11,A1:A3,B1:B3) is ordinary).
        let lookup_range = upper == "XLOOKUP" && shapes.get(1).is_some_and(|shape| shape.multi);
        for (position, shape) in shapes.iter().enumerate() {
            if !shape.multi
                || entry.positions.is_empty()
                || (literal_index && position == 0)
                || (lookup_range && position == 2)
            {
                continue;
            }
            if entry.positions[position.min(entry.positions.len() - 1)] {
                // A range where the function takes one value: evaluated elementwise, which a
                // legacy formula would intersect instead.
                self.dynamic = true;
                multi = true;
            }
        }
        // INDEX with a row or column number of 0 gives a whole column or row.
        if upper == "INDEX"
            && arguments.iter().skip(1).take(2).any(|argument| {
                argument.kind() == SyntaxKind::NumberLiteralExpr && argument.text().trim() == "0"
            })
        {
            multi = true;
        }
        Ok(Shape { multi })
    }

    /// LET (`name, value, ..., body`) or LAMBDA (`parameter, ..., body`): each parameter gets
    /// `_xlpm.` (an optional LAMBDA parameter `[y]` becomes `_xlop.y`) and is in scope after its
    /// declaration.
    fn binding(
        &mut self,
        function: &str,
        arguments: &[&SyntaxNode],
        depth: usize,
    ) -> Result<Shape, FileFormError> {
        self.scopes.push(BTreeMap::new());
        let last = arguments.len().saturating_sub(1);
        let mut shape = Shape::default();
        let mut index = 0;
        while index < arguments.len() {
            let argument = arguments[index];
            let declares = index < last && (function == "LAMBDA" || index % 2 == 0);
            if declares {
                let token = argument.tokens().into_iter().next().cloned();
                match token {
                    Some(token) if token.kind() == TokenKind::BracketedQualifier => {
                        let inner = token.text().trim_start_matches('[').trim_end_matches(']');
                        let span = token.span();
                        self.edits
                            .push((span.start, span.end(), format!("_xlop.{inner}")));
                        self.scopes
                            .last_mut()
                            .expect("a scope")
                            .insert(inner.to_uppercase(), false);
                    }
                    Some(token)
                        if token.kind() == TokenKind::Identifier && !has_prefix(token.text()) =>
                    {
                        self.insert(token.span().start, "_xlpm.");
                        let value = if function == "LET" {
                            match arguments.get(index + 1) {
                                Some(value) => {
                                    let shape = self.walk(value, depth + 1)?;
                                    index += 1;
                                    shape.multi
                                }
                                None => false,
                            }
                        } else {
                            false
                        };
                        self.scopes
                            .last_mut()
                            .expect("a scope")
                            .insert(token.text().to_uppercase(), value);
                    }
                    Some(token) if token.kind() == TokenKind::Identifier => {
                        let name = token
                            .text()
                            .split_once('.')
                            .map_or(token.text(), |(_, name)| name);
                        let value = if function == "LET" {
                            match arguments.get(index + 1) {
                                Some(value) => {
                                    let shape = self.walk(value, depth + 1)?;
                                    index += 1;
                                    shape.multi
                                }
                                None => false,
                            }
                        } else {
                            false
                        };
                        self.scopes
                            .last_mut()
                            .expect("a scope")
                            .insert(name.to_uppercase(), value);
                    }
                    _ => {
                        self.walk(argument, depth + 1)?;
                    }
                }
            } else {
                shape = self.walk(argument, depth + 1)?;
            }
            index += 1;
        }
        self.scopes.pop();
        if function == "LAMBDA" {
            // A LAMBDA's value is a function, which only a call (an InvokeExpr) applies.
            return Ok(Shape { multi: false });
        }
        Ok(shape)
    }
}

/// The file form of `formula`, which a user authored: its text with the prefixes Excel writes,
/// and whether Excel stores it as a dynamic array. `names` are the workbook's defined names,
/// which tell a constant name from one that may hold several values.
pub fn file_form(formula: &FormulaText, names: &[DefinedName]) -> Result<FileForm, FileFormError> {
    file_form_with_links(formula, names, &[])
}

/// [`file_form`] in a workbook with links to other workbooks, `links` giving each link's number
/// and the file name of the book it targets (dc-ylr): a reference naming such a book by file name
/// is written by the link's number.
pub fn file_form_with_links(
    formula: &FormulaText,
    names: &[DefinedName],
    links: &[(u32, String)],
) -> Result<FileForm, FileFormError> {
    let links: Vec<(u32, String)> = links
        .iter()
        .map(|(number, name)| (*number, name.to_uppercase()))
        .collect();
    let units = formula.text().utf16_code_units();
    let text = String::from_utf16(units).map_err(|_| FileFormError::Unreadable)?;
    let source = format!("={text}");
    let tree = parse_formula(&source);
    if !tree.diagnostics().is_empty() {
        return Err(FileFormError::Unreadable);
    }
    let mut walk = Walk {
        names,
        links: &links,
        edits: Vec::new(),
        dynamic: false,
        scopes: Vec::new(),
    };
    let Some(expression) = tree.expression() else {
        return Err(FileFormError::Unreadable);
    };
    // A top-level @ is dropped: the formula is an ordinary (implicitly intersecting) one.
    let (expression, dropped) = match (expression.kind(), first_token(expression)) {
        (SyntaxKind::PrefixExpr, Some(token)) if token.kind() == TokenKind::At => {
            let span = token.span();
            match expression.child_nodes().next() {
                Some(operand) => (operand, Some(span)),
                None => (expression, None),
            }
        }
        _ => (expression, None),
    };
    let shape = walk.walk(expression, 0)?;
    if dropped.is_none() && shape.multi {
        walk.dynamic = true;
    }
    if dropped.is_some() {
        walk.dynamic = false;
    }
    if let Some(span) = dropped {
        walk.edits.push((span.start, span.end(), String::new()));
    }
    // Apply the edits to the source's UTF-16 code units, then drop the leading `=`.
    let source_units: Vec<u16> = source.encode_utf16().collect();
    let mut edits = walk.edits;
    edits.sort_by_key(|(start, end, _)| (*start, *end));
    let mut out = Vec::with_capacity(source_units.len() + 16);
    let mut at = 0;
    for (start, end, replacement) in edits {
        if start < at {
            continue;
        }
        out.extend_from_slice(&source_units[at..start]);
        out.extend(replacement.encode_utf16());
        at = end;
    }
    out.extend_from_slice(&source_units[at..]);
    out.remove(0);
    Ok(FileForm {
        text: out,
        dynamic: walk.dynamic,
    })
}
