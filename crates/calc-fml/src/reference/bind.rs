//! Binding the grid references of a parsed formula against a catalogue and an anchor.
//!
//! A reference here is an identifier or number spelled as a cell, row or column of the formula's
//! notation (see the `notation` module), unqualified or qualified by one sheet, and a range of two
//! such endpoints of one form on one sheet (`B3:D7`, `2:4`, `B:D`, `Sheet2!B3:C4`,
//! `Sheet2!B3:Sheet2!C4`). A lone row or column is a reference in R1C1 notation (`R3`, `C`) and
//! not in A1 notation, where `3` is a number and `B` a name, and `$3` and `$B`, which can be
//! neither, are refused. Everything else is left to the formula's own operators and to later
//! binding: a `:` between other operands, or between endpoints of different forms or sheets, is
//! the range operator over separately bound references; names, tables and calls are not
//! references here.
//!
//! A qualifier belongs to its whole operand. A qualified operand is bound with its qualifier or
//! refused; the walk never enters it, so nothing inside it can bind as an unqualified reference.
//! The same holds for syntax the parser refused. A reference inside an array constant is refused:
//! an array constant holds constants only.

use super::notation::{self, Spelled};
use super::structured::{TableLookup, bind_structured};
use super::{
    AxisRef, CellRef, FormulaAnchor, ReferenceNotation, ReferenceOperand, ReferenceShape,
    SheetTarget,
};
use crate::syntax::{SyntaxChild, SyntaxKind, SyntaxNode, SyntaxTree, TextSpan, Token, TokenKind};
use calc_func::value::{CellKey, SheetId, WorksheetErrorCode};

/// The workbook identities a formula's references are bound against. Binding asks it only for
/// sheets, by the name a qualifier spells, never for reference text.
pub trait ReferenceCatalog {
    /// The sheet of the workbook named `name`, or `None` when there is none. `name` is the
    /// qualifier's sheet name as written, without quotes or `!` (`It's` for `'It''s'!A1`); the
    /// catalogue matches it as Excel matches sheet names, regardless of case.
    fn sheet_named(&self, name: &str) -> Option<SheetId>;

    /// The defined name `name` (as written, matched regardless of case as Excel matches names)
    /// seen from `scope`. The default is [`NameDefinition::NotModelled`]: a catalogue that does
    /// not carry the workbook's defined names says so, and a formula that uses a name is then
    /// refused rather than given #NAME?.
    fn defined_name(&self, name: &str, scope: NameScope) -> NameDefinition {
        let _ = (name, scope);
        NameDefinition::NotModelled
    }

    /// The table (ListObject) whose name is `name`, matched regardless of case (L3, dc-m3u). The
    /// default is [`TableLookup::NotModelled`]: a structured reference is then refused.
    fn table(&self, name: &str) -> TableLookup {
        let _ = name;
        TableLookup::NotModelled
    }

    /// The table whose area holds `cell`, which an unqualified structured reference (`[@Qty]`)
    /// in a formula there names (L3, dc-m3u). The default is [`TableLookup::NotModelled`].
    fn table_containing(&self, cell: CellKey) -> TableLookup {
        let _ = cell;
        TableLookup::NotModelled
    }

    /// The sheets a 3-D reference from `first` to `last` names, in the workbook's sheet order,
    /// both included, whichever comes first (L3, dc-m3u). `None` when the catalogue does not
    /// know the sheets' order: a 3-D reference is then refused ([`BindError::SheetSpan`]).
    fn sheet_span(&self, first: SheetId, last: SheetId) -> Option<Vec<SheetId>> {
        let _ = (first, last);
        None
    }

    /// Whether a call of a name that neither a function nor a defined name holds is the error
    /// #NAME?: true when the workbook can hold no function of its own (no VBA project, no macro
    /// sheet). The default, `false`, refuses such a call: a VBA or add-in function might hold
    /// the name. Excel 16.0 (2026-10-23) gives `=FOO(1)` #NAME? in a workbook without macros and
    /// stores it as `FOO(1)`.
    fn unknown_calls_are_names(&self) -> bool {
        false
    }

    /// The file name of the formula's workbook (`Book1.xlsx`), which reference text may name
    /// (`[Book1.xlsx]Sheet1!A1`, dc-10y). The default, `None`, refuses text that names a workbook:
    /// it might name this one.
    fn workbook_name(&self) -> Option<&str> {
        None
    }

    /// The sheet `sheet` (the name as written, matched as Excel matches sheet names) of the book
    /// `book` names, as the workbook's link to that book caches it (dc-ylr): the sheet ID by which
    /// a reference to its cells names them, which the reader of the workbook's cells reads from
    /// the link's cache. The default, `None`, refuses every external reference as not modelled.
    fn external_sheet(&self, book: &ExternalBookRef, sheet: &str) -> Option<SheetId> {
        let _ = (book, sheet);
        None
    }
}

/// How a formula names another workbook (dc-ylr).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExternalBookRef {
    /// By the number a file's formula writes (`[1]Sheet1!A1` names the workbook's first link).
    Number(u32),
    /// By its file name, as a typed formula writes it (`[Book2.xlsx]Sheet1!A1`, or with the
    /// folder before it, `'C:\Data\[Book2.xlsx]Sheet1'!A1`): the link whose target has that
    /// file name.
    File(String),
}

/// A reference to another workbook (`[1]Sheet1!A1`, `'[1]My Sheet'!A1:B2`,
/// `[Book2.xlsx]Sheet1!A1`): the book, the sheet's name and the cells' shape, and whether it
/// belongs to a defined name's formula, whose relative references wrap at the grid's edge (dc-ylr).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExternalOperand {
    pub book: ExternalBookRef,
    pub sheet: String,
    pub shape: ReferenceShape,
    pub wrapped: bool,
}

/// The external reference `node` writes, read in `notation` for a formula anchored at `anchor`: a
/// qualifier `[N]Sheet` (quoted or not) before a cell, or before the first end of a range whose
/// second end is unqualified or has the same qualifier. `None` for anything else (a book named by
/// its file name or path, which a file's formula does not store, a run of sheets, a name).
pub(crate) fn external_operand(
    node: &SyntaxNode,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
) -> Option<ExternalOperand> {
    let spell = |token: &Token| match notation {
        ReferenceNotation::A1 => notation::a1(token.text(), anchor.cell().row(), anchor.cell().column()),
        ReferenceNotation::R1C1 => notation::r1c1(token.text()),
    };
    let qualified = |node: &SyntaxNode| -> Option<(ExternalBookRef, String, Spelled)> {
        if node.kind() != SyntaxKind::QualifiedReferenceExpr {
            return None;
        }
        let SyntaxChild::Token(qualifier) = node.children().first()? else {
            return None;
        };
        let text = match qualifier.kind() {
            TokenKind::BracketedQualifier => qualifier.text().to_owned(),
            TokenKind::QuotedIdentifier => unquote(qualifier.text()),
            _ => return None,
        };
        let (folder, rest) = text.split_once('[')?;
        let (book, sheet) = rest.split_once(']')?;
        if sheet.is_empty() || sheet.contains(':') || book.is_empty() {
            return None;
        }
        let book = match book.parse::<u32>() {
            Ok(0) => return None,
            Ok(number) if folder.is_empty() => ExternalBookRef::Number(number),
            _ => ExternalBookRef::File(book.to_owned()),
        };
        let spelled = spell(only_token(node.child_nodes().next()?)?)?;
        Some((book, sheet.to_owned(), spelled))
    };
    let (book, sheet, shape) = match node.kind() {
        SyntaxKind::QualifiedReferenceExpr => {
            let (book, sheet, spelled) = qualified(node)?;
            let Spelled::Cell(cell) = spelled else {
                return None;
            };
            (book, sheet, ReferenceShape::Cell(cell))
        }
        SyntaxKind::RangeExpr => {
            let mut operands = node.child_nodes();
            let (left, right) = (operands.next()?, operands.next()?);
            let (book, sheet, first) = qualified(left)?;
            let last = match right.kind() {
                SyntaxKind::QualifiedReferenceExpr => {
                    let (other_book, other_sheet, last) = qualified(right)?;
                    if other_book != book || other_sheet != sheet {
                        return None;
                    }
                    last
                }
                _ => spell(only_token(right)?)?,
            };
            if first.form() != last.form() {
                return None;
            }
            (book, sheet, pair(first, last).ok()?)
        }
        _ => return None,
    };
    Some(ExternalOperand {
        book,
        sheet,
        shape,
        wrapped: false,
    })
}

/// Where a defined name is looked up from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NameScope {
    /// An unqualified name in a formula on this sheet: the sheet's own name of that spelling,
    /// else the workbook's.
    Formula(SheetId),
    /// A name qualified by this sheet (`Sheet2!TaxRate`): that sheet's own name.
    Sheet(SheetId),
}

/// What a defined name is, as a [`ReferenceCatalog`] gives it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NameDefinition {
    /// The name's formula in A1 notation, with or without a leading `=` (`Sheet1!$B$2`,
    /// `0.05`, `LAMBDA(x,x+1)`), as a file's `definedName` element holds it. Its references are
    /// bound as a file stores them, relative to cell A1; one that is relative is refused.
    Formula(String),
    /// The workbook has no such name: Excel's #NAME?.
    Unknown,
    /// The catalogue does not carry defined names.
    NotModelled,
}

/// One reference of a formula, or one stretch of syntax the parser refused, and what it bound to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceSite {
    /// The site's source, in UTF-16 code units of the formula text.
    pub span: TextSpan,
    pub binding: Result<ReferenceOperand, BindError>,
    /// The reference takes the formula's row from its area by implicit intersection: a
    /// structured reference's `#This Row` (`Sales[@Qty]`), bound to the column's data rows.
    pub this_row: bool,
}

/// Why a site was refused at binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindError {
    /// The reference names a cell, row or column outside the grid wherever it is placed:
    /// `$XFE$1`, `1:1048577`, `R0C1`, `R[1048576]C`.
    OutsideGrid,
    /// The catalogue has no sheet of this name.
    UnknownSheet { name: String },
    /// A qualifier naming another workbook (`[Book2.xlsx]Sheet1!A1`, or `[1]Sheet1!A1` as a file
    /// stores it): external references are not modelled (L3, dc-m3u), and a formula holding one
    /// is refused.
    ExternalWorkbook,
    /// A run of sheets (`Sheet1:Sheet3!A1`, `'Sheet 1:Sheet 3'!A1`) bound against a catalogue
    /// that does not know the workbook's sheet order ([`ReferenceCatalog::sheet_span`]).
    SheetSpan,
    /// A structured reference names a table the workbook does not hold (Excel refuses to enter
    /// such a formula).
    UnknownTable { name: String },
    /// A structured reference names a column its table does not hold (Excel refuses to enter
    /// such a formula).
    UnknownColumn { name: String },
    /// A structured reference bound against a catalogue that does not carry tables.
    TablesNotModelled,
    /// A structured reference whose combination of sections this package does not bind
    /// (`#This Row` with another section, `#Headers` with `#Totals`).
    StructuredReference,
    /// A qualifier (one sheet, a run of sheets, another workbook or `#REF!`) before an operand
    /// that is neither a cell, row or column of the formula's notation nor a name nor `#REF!`:
    /// `Sheet2!(A1)`, `Sheet2!Sheet3!A1`, `#REF!SUM(A1)`, `Sheet2!3`, and `#REF!(A1)`, which the
    /// parser reads as a call of the `#REF!` error value. The site spans the qualifier and its
    /// operand, and the catalogue is not asked.
    QualifiedNonReference,
    /// In A1 notation, a row or column written alone with a `$` (`$B`, `$3`, `Sheet2!$B`). A1
    /// notation has no lone row or column reference (the whole column is `B:B`, the whole row
    /// `3:3`), and a `$` form is neither a name nor a number. Without a `$`, `B` is left to name
    /// binding and `3` is a number, and neither gives a site. The catalogue is not asked.
    LoneRowOrColumn,
    /// A reference inside an array constant (`{A1}`, `{1,Sheet2!B3}`): an array constant holds
    /// constants only, so a reference there is no live operand. The catalogue is not asked. A
    /// site inside an array constant that is refused for another reason keeps that reason.
    InArrayConstant,
    /// Syntax the parser refused with an unsupported-syntax diagnostic, such as a trim reference
    /// (`Sheet1!A1.:B10`) or a `.` member selector (`(A1).Value`). The site spans the refused
    /// syntax, and no reference inside it is bound.
    UnsupportedSyntax,
}

/// Binds every grid reference of `tree`, read in `notation`, for a formula anchored at `anchor`;
/// the sites come in source order. A range of two endpoints of one form on one sheet is one site.
///
/// A qualified operand is one site, bound to the sheet its qualifier names or refused: a
/// qualifier before anything but a reference endpoint or `#REF!` is
/// [`BindError::QualifiedNonReference`], except a name (`Sheet2!TaxRate`), which gives no site
/// and is left to name binding. `Sheet2!#REF!`, the form Excel writes for a reference whose cells
/// were deleted, binds to a [`ReferenceShape::Deleted`] operand on that sheet. Syntax the parser
/// refused is refused here as one [`BindError::UnsupportedSyntax`] site, whatever it holds: the
/// references inside it (`Sheet1!A1` and `B10` in `=SUM(Sheet1!A1.:B10)`) give no sites of their
/// own. Each reference inside an array constant is refused with
/// [`BindError::InArrayConstant`].
pub fn bind_references(
    tree: &SyntaxTree,
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &dyn ReferenceCatalog,
) -> Vec<ReferenceSite> {
    let mut binder = Binder {
        notation,
        anchor,
        catalog,
        in_array_constant: false,
        sites: Vec::new(),
    };
    binder.walk(tree.root());
    binder.sites
}

/// Where a reference endpoint's sheet comes from, as its syntax spells it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Qualifier {
    Unqualified,
    Sheet(String),
    Deleted,
    External,
    /// A run of sheets: the first and last sheet names as written.
    Span(String, String),
}

struct Binder<'c> {
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &'c dyn ReferenceCatalog,
    /// The walk is inside an array constant, where nothing binds as a live reference.
    in_array_constant: bool,
    sites: Vec<ReferenceSite>,
}

/// One step of the binder's walk.
enum Visit<'t> {
    /// A node to bind or to enter.
    Node(&'t SyntaxNode),
    /// The end of an array constant: whether the walk was inside another one.
    LeaveArrayConstant { outer: bool },
}

impl Binder<'_> {
    /// Visits the nodes under `root` in source order, each before the nodes inside it. The walk
    /// keeps the nodes still to visit on a stack of its own, the next one on top, so a formula
    /// nested however deep is walked without recursion.
    fn walk(&mut self, root: &SyntaxNode) {
        let mut pending = vec![Visit::Node(root)];
        while let Some(visit) = pending.pop() {
            let node = match visit {
                Visit::Node(node) => node,
                Visit::LeaveArrayConstant { outer } => {
                    self.in_array_constant = outer;
                    continue;
                }
            };
            let binding = match node.kind() {
                SyntaxKind::UnsupportedExpr => Some(Err(BindError::UnsupportedSyntax)),
                SyntaxKind::QualifiedReferenceExpr if self.qualified_structured(node) => continue,
                // Bound with the qualifier or refused, never entered: what lies inside a
                // qualified operand is not an unqualified reference.
                SyntaxKind::QualifiedReferenceExpr | SyntaxKind::SheetSpan3DReferenceExpr => {
                    self.qualified(node)
                }
                // `#REF!(A1)`: the `#REF!` qualifier before a parenthesised operand, which the
                // parser reads as a call of the error value. Refused whole, as `Sheet2!(A1)` is.
                SyntaxKind::InvokeExpr if node.child_nodes().next().is_some_and(is_ref_error) => {
                    Some(Err(BindError::QualifiedNonReference))
                }
                SyntaxKind::IdentifierExpr if self.structured(node) => continue,
                SyntaxKind::IdentifierExpr | SyntaxKind::NumberLiteralExpr => {
                    only_token(node).and_then(|token| self.alone(&Qualifier::Unqualified, token))
                }
                SyntaxKind::RangeExpr | SyntaxKind::TrimRangeExpr => {
                    if !self.range(node) {
                        visit_children(node, &mut pending);
                    }
                    continue;
                }
                SyntaxKind::ArrayLiteralExpr => {
                    let outer = std::mem::replace(&mut self.in_array_constant, true);
                    pending.push(Visit::LeaveArrayConstant { outer });
                    visit_children(node, &mut pending);
                    continue;
                }
                _ => {
                    visit_children(node, &mut pending);
                    continue;
                }
            };
            if let Some(binding) = binding {
                self.push(node.span(), binding);
            }
        }
    }

    fn push(&mut self, span: TextSpan, binding: Result<ReferenceOperand, BindError>) {
        self.sites.push(ReferenceSite {
            span,
            binding,
            this_row: false,
        });
    }

    /// Binds an identifier holding a bracket group as a structured reference (`Sales[Qty]`,
    /// `[@Qty]`) and returns whether it was one: `false` for an identifier without brackets, and
    /// for a bracket group that is no structured reference (`[y]`, a LAMBDA's optional
    /// parameter, outside every table).
    fn structured(&mut self, node: &SyntaxNode) -> bool {
        let Some(token) = only_token(node) else {
            return false;
        };
        if !token.text().contains('[') {
            return false;
        }
        // In R1C1 notation a bracket marks a relative offset (`R[-1]C`), not a table.
        if self.notation == ReferenceNotation::R1C1 && notation::r1c1(token.text()).is_some() {
            return false;
        }
        if self.in_array_constant {
            self.push(node.span(), Err(BindError::InArrayConstant));
            return true;
        }
        match bind_structured(token.text(), self.anchor, self.catalog) {
            None => false,
            Some(Err(error)) => {
                self.push(node.span(), Err(error));
                true
            }
            Some(Ok(bound)) => {
                self.sites.push(ReferenceSite {
                    span: node.span(),
                    binding: Ok(bound.operand),
                    this_row: bound.this_row,
                });
                true
            }
        }
    }

    /// Binds a structured reference after a sheet qualifier (`Tab!Sales[Qty]`) as the structured
    /// reference alone and returns whether it was one (dc-10y). Excel drops the qualifier, whatever
    /// sheet of the workbook it names: Excel 16.0.20430 (2026-10-25) stored `=SUM(Tab!Sales[Qty])`,
    /// `=SUM('Tab'!Sales[Qty])` and `=Other!Sales[Qty]` (Sales being a table on sheet Tab) as
    /// `=SUM(Sales[Qty])` and `=Sales[@Qty]`, entered through `Range.Formula` and read from a
    /// file alike. A qualifier naming no sheet of the workbook, a run of sheets or another workbook
    /// is refused, as not measured.
    fn qualified_structured(&mut self, node: &SyntaxNode) -> bool {
        if self.notation != ReferenceNotation::A1 {
            return false;
        }
        let Some((qualifier, target)) = qualified_parts(node) else {
            return false;
        };
        let Some(token) = only_token(target) else {
            return false;
        };
        if !token.text().contains('[') {
            return false;
        }
        match self.sheet_target(&qualifier) {
            Ok(SheetTarget::Sheet(_)) => {}
            Ok(_) | Err(_) => {
                self.push(node.span(), Err(BindError::StructuredReference));
                return true;
            }
        }
        match bind_structured(token.text(), self.anchor, self.catalog) {
            None => false,
            Some(Err(error)) => {
                self.push(node.span(), Err(error));
                true
            }
            Some(Ok(bound)) => {
                self.sites.push(ReferenceSite {
                    span: node.span(),
                    binding: Ok(bound.operand),
                    this_row: bound.this_row,
                });
                true
            }
        }
    }

    /// The binding of a qualified operand standing alone: the operand bound on the qualifier's
    /// sheet when it is a reference endpoint or `#REF!`, `None` when it is a name, which name
    /// binding resolves, and [`BindError::QualifiedNonReference`] for anything else.
    fn qualified(&self, node: &SyntaxNode) -> Option<Result<ReferenceOperand, BindError>> {
        let Some((qualifier, target)) = qualified_parts(node) else {
            return Some(Err(BindError::QualifiedNonReference));
        };
        if is_ref_error(target) {
            // `Sheet2!#REF!`: a reference on that sheet whose cells were deleted.
            return Some(self.sheet_target(&qualifier).map(|sheet| ReferenceOperand {
                sheet,
                shape: ReferenceShape::Deleted,
            }));
        }
        let Some(token) = only_token(target) else {
            return Some(Err(BindError::QualifiedNonReference));
        };
        qualified_site(token, self.alone(&qualifier, token))
    }

    /// The binding of one endpoint standing alone with `qualifier`, whose sheet is resolved only
    /// once the endpoint's text gives a shape; `None` when its text is no reference alone in the
    /// formula's notation.
    fn alone(
        &self,
        qualifier: &Qualifier,
        token: &Token,
    ) -> Option<Result<ReferenceOperand, BindError>> {
        Some(self.lone(token)?.and_then(|shape| {
            Ok(ReferenceOperand {
                sheet: self.sheet_target(qualifier)?,
                shape,
            })
        }))
    }

    /// The shape one endpoint's text names standing alone, or why it is refused; `None` when it
    /// is no reference alone in the formula's notation: in A1 notation a name (`B`, `XFE1`) or a
    /// number (`3`, `0`, `1048577`).
    fn lone(&self, token: &Token) -> Option<Result<ReferenceShape, BindError>> {
        let r1c1 = self.notation == ReferenceNotation::R1C1;
        // In A1 notation a row or column form holds a `$` only at its start, and a text with one
        // is neither a name nor a number.
        let dollar = token.text().starts_with('$');
        Some(match self.spell(token)? {
            Spelled::Cell(cell) => Ok(ReferenceShape::Cell(cell)),
            // Alone, a row or a column is a reference only in R1C1 notation (`R3`, `C`).
            Spelled::Row(row) if r1c1 => Ok(ReferenceShape::Rows {
                first: row,
                last: row,
            }),
            Spelled::Column(column) if r1c1 => Ok(ReferenceShape::Columns {
                first: column,
                last: column,
            }),
            Spelled::OutsideGrid(form) if r1c1 || form == notation::Form::Cell || dollar => {
                Err(BindError::OutsideGrid)
            }
            Spelled::Row(_) | Spelled::Column(_) if dollar => Err(BindError::LoneRowOrColumn),
            Spelled::Row(_) | Spelled::Column(_) | Spelled::OutsideGrid(_) => return None,
        })
    }

    /// Binds `left:right` as one site when both are endpoints of one form on one sheet, and
    /// returns whether it bound anything; when it returns `false`, each side is walked alone.
    /// The ends lie on one sheet when the right one is unqualified or its qualifier names the
    /// left one's sheet. Each of two sheet qualifiers is asked of the catalogue once; when they
    /// name two sheets, or the catalogue does not know one of them, each end is a site of its
    /// own, bound or refused alone (the range operator between two references).
    fn range(&mut self, node: &SyntaxNode) -> bool {
        let mut operands = node.child_nodes();
        let (Some(left_node), Some(right_node)) = (operands.next(), operands.next()) else {
            return false;
        };
        let (Some((left_qualifier, left_token)), Some((right_qualifier, right_token))) =
            (endpoint(left_node), endpoint(right_node))
        else {
            return false;
        };
        let (Some(left), Some(right)) = (self.spell(left_token), self.spell(right_token)) else {
            return false;
        };
        if left.form() != right.form() {
            return false;
        }
        let binding = match (&left_qualifier, &right_qualifier) {
            (_, Qualifier::Unqualified)
            | (Qualifier::External | Qualifier::Span(..), _)
            | (Qualifier::Deleted, Qualifier::Deleted) => pair(left, right).and_then(|shape| {
                Ok(ReferenceOperand {
                    sheet: self.sheet_target(&left_qualifier)?,
                    shape,
                })
            }),
            (Qualifier::Sheet(_), Qualifier::Sheet(_)) => {
                match (
                    self.sheet_target(&left_qualifier),
                    self.sheet_target(&right_qualifier),
                ) {
                    (Ok(sheet), Ok(other)) if sheet == other => {
                        pair(left, right).map(|shape| ReferenceOperand { sheet, shape })
                    }
                    (left_sheet, right_sheet) => {
                        self.end_alone(left_node, left_token, left_sheet);
                        self.end_alone(right_node, right_token, right_sheet);
                        return true;
                    }
                }
            }
            _ => return false,
        };
        self.push(node.span(), binding);
        true
    }

    /// One end of a range whose ends are not on one sheet, bound alone on `sheet`, its
    /// qualifier's resolution, as [`Binder::qualified`] binds it.
    fn end_alone(
        &mut self,
        node: &SyntaxNode,
        token: &Token,
        sheet: Result<SheetTarget, BindError>,
    ) {
        let binding = self.lone(token).map(|shape| {
            shape.and_then(|shape| {
                Ok(ReferenceOperand {
                    sheet: sheet?,
                    shape,
                })
            })
        });
        if let Some(binding) = qualified_site(token, binding) {
            self.push(node.span(), binding);
        }
    }

    fn spell(&self, token: &Token) -> Option<Spelled> {
        match self.notation {
            ReferenceNotation::A1 => {
                let at = self.anchor.cell();
                notation::a1(token.text(), at.row(), at.column())
            }
            ReferenceNotation::R1C1 => notation::r1c1(token.text()),
        }
    }

    /// The sheet a qualifier names. Inside an array constant nothing is a live reference: every
    /// endpoint there, qualified or not, is refused without asking the catalogue.
    fn sheet_target(&self, qualifier: &Qualifier) -> Result<SheetTarget, BindError> {
        if self.in_array_constant {
            return Err(BindError::InArrayConstant);
        }
        match qualifier {
            Qualifier::Unqualified => Ok(SheetTarget::Anchor),
            Qualifier::Deleted => Ok(SheetTarget::Deleted),
            Qualifier::External => Err(BindError::ExternalWorkbook),
            Qualifier::Span(first, last) => {
                let sheet = |name: &String| {
                    self.catalog
                        .sheet_named(name)
                        .ok_or_else(|| BindError::UnknownSheet { name: name.clone() })
                };
                let (first, last) = (sheet(first)?, sheet(last)?);
                match self.catalog.sheet_span(first, last) {
                    Some(_) => Ok(SheetTarget::Span { first, last }),
                    None => Err(BindError::SheetSpan),
                }
            }
            Qualifier::Sheet(name) => self
                .catalog
                .sheet_named(name)
                .map(SheetTarget::Sheet)
                .ok_or_else(|| BindError::UnknownSheet { name: name.clone() }),
        }
    }
}

/// Puts the child nodes of `node` on the walk's stack, the first on top.
fn visit_children<'t>(node: &'t SyntaxNode, pending: &mut Vec<Visit<'t>>) {
    let first = pending.len();
    pending.extend(node.child_nodes().map(Visit::Node));
    pending[first..].reverse();
}

/// A qualified endpoint's site given its binding alone: that binding, or, when its text is no
/// reference alone, no site for a name (`Sheet2!TaxRate`, left to name binding) and
/// [`BindError::QualifiedNonReference`] for anything else (`Sheet2!3`).
fn qualified_site(
    token: &Token,
    binding: Option<Result<ReferenceOperand, BindError>>,
) -> Option<Result<ReferenceOperand, BindError>> {
    match binding {
        Some(binding) => Some(binding),
        None if token.kind() == TokenKind::Identifier => None,
        None => Some(Err(BindError::QualifiedNonReference)),
    }
}

/// The shape between two endpoints of one form, each axis's pair in the order of [`ordered`].
pub(super) fn pair(left: Spelled, right: Spelled) -> Result<ReferenceShape, BindError> {
    match (left, right) {
        (Spelled::Cell(a), Spelled::Cell(b)) => {
            let (top, bottom) = ordered(a.row, b.row);
            let (left, right) = ordered(a.column, b.column);
            Ok(ReferenceShape::Area {
                first: CellRef {
                    row: top,
                    column: left,
                },
                last: CellRef {
                    row: bottom,
                    column: right,
                },
            })
        }
        (Spelled::Row(a), Spelled::Row(b)) => {
            let (first, last) = ordered(a, b);
            Ok(ReferenceShape::Rows { first, last })
        }
        (Spelled::Column(a), Spelled::Column(b)) => {
            let (first, last) = ordered(a, b);
            Ok(ReferenceShape::Columns { first, last })
        }
        // The forms agree, so at least one endpoint lies outside the grid.
        _ => Err(BindError::OutsideGrid),
    }
}

/// The two ends of an axis in an order that no anchor changes, so that an operand is the same
/// value wherever it was bound: two absolute or two relative coordinates by position, which is
/// the same at every anchor; an absolute and a relative one, which cross as the anchor moves,
/// absolute first. Placing the operand finds which end comes first on the grid.
fn ordered(a: AxisRef, b: AxisRef) -> (AxisRef, AxisRef) {
    let swap = match (a, b) {
        (AxisRef::Absolute(a), AxisRef::Absolute(b)) => a > b,
        (AxisRef::Relative(a), AxisRef::Relative(b)) => a > b,
        (AxisRef::Relative(_), AxisRef::Absolute(_)) => true,
        (AxisRef::Absolute(_), AxisRef::Relative(_)) => false,
    };
    if swap { (b, a) } else { (a, b) }
}

/// A node that can be a reference endpoint: an identifier or number, alone, after one sheet's
/// qualifier, or after a run of sheets. Gives its qualifier and the token to spell.
fn endpoint(node: &SyntaxNode) -> Option<(Qualifier, &Token)> {
    match node.kind() {
        SyntaxKind::IdentifierExpr | SyntaxKind::NumberLiteralExpr => {
            Some((Qualifier::Unqualified, only_token(node)?))
        }
        _ => {
            let (qualifier, target) = qualified_parts(node)?;
            Some((qualifier, only_token(target)?))
        }
    }
}

/// The sheet name and the name token of a name qualified by one sheet (`Sheet2!TaxRate`,
/// `'Q2 Data'!Rate`), when `node` is one.
pub(crate) fn qualified_name(node: &SyntaxNode) -> Option<(String, &Token)> {
    let (qualifier, target) = qualified_parts(node)?;
    let Qualifier::Sheet(sheet) = qualifier else {
        return None;
    };
    let token = match target.children() {
        [SyntaxChild::Token(token)] if target.kind() == SyntaxKind::IdentifierExpr => token,
        _ => return None,
    };
    Some((sheet, token))
}

/// The qualifier of a qualified operand and the operand it qualifies.
fn qualified_parts(node: &SyntaxNode) -> Option<(Qualifier, &SyntaxNode)> {
    match node.kind() {
        SyntaxKind::QualifiedReferenceExpr => {
            let SyntaxChild::Token(qualifier) = node.children().first()? else {
                return None;
            };
            Some((qualifier_of(qualifier), node.child_nodes().next()?))
        }
        SyntaxKind::SheetSpan3DReferenceExpr => {
            let names: Vec<&Token> = node
                .children()
                .iter()
                .filter_map(|child| match child {
                    SyntaxChild::Token(token)
                        if matches!(
                            token.kind(),
                            TokenKind::Identifier | TokenKind::QuotedIdentifier
                        ) =>
                    {
                        Some(token)
                    }
                    _ => None,
                })
                .collect();
            let [first, last] = names[..] else {
                return None;
            };
            let name = |token: &Token| match token.kind() {
                TokenKind::QuotedIdentifier => unquote(token.text()),
                _ => token.text().to_owned(),
            };
            let (first, last) = (name(first), name(last));
            let qualifier = if [&first, &last]
                .iter()
                .any(|name| name.contains(['[', ']', '\\', '/']))
            {
                Qualifier::External
            } else {
                Qualifier::Span(first, last)
            };
            Some((qualifier, node.child_nodes().next()?))
        }
        _ => None,
    }
}

/// Whether `node` is the error literal `#REF!`.
fn is_ref_error(node: &SyntaxNode) -> bool {
    node.kind() == SyntaxKind::ErrorLiteralExpr
        && matches!(
            node.children(),
            [SyntaxChild::Token(token)] if token.error_code() == Some(WorksheetErrorCode::Ref)
        )
}

/// The token of an identifier or number node.
fn only_token(node: &SyntaxNode) -> Option<&Token> {
    if !matches!(
        node.kind(),
        SyntaxKind::IdentifierExpr | SyntaxKind::NumberLiteralExpr
    ) {
        return None;
    }
    match node.children() {
        [SyntaxChild::Token(token)] => Some(token),
        _ => None,
    }
}

/// What a qualifier token names. A sheet name never holds `[`, `]`, `\`, `/` or `:`, so a
/// qualifier holding one of the first four names another workbook and one holding `:` a run of
/// sheets.
fn qualifier_of(token: &Token) -> Qualifier {
    let name = match token.kind() {
        TokenKind::ErrorLiteral => return Qualifier::Deleted,
        TokenKind::BracketedQualifier => return Qualifier::External,
        TokenKind::QuotedIdentifier => unquote(token.text()),
        _ => token.text().to_owned(),
    };
    if name.contains(['[', ']', '\\', '/']) {
        Qualifier::External
    } else if let Some((first, last)) = name.split_once(':') {
        Qualifier::Span(first.to_owned(), last.to_owned())
    } else {
        Qualifier::Sheet(name)
    }
}

/// The name inside a quoted qualifier: without its quotes, each doubled quote one quote.
fn unquote(text: &str) -> String {
    let inner = text.strip_prefix('\'').unwrap_or(text);
    let inner = inner.strip_suffix('\'').unwrap_or(inner);
    inner.replace("''", "'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_formula;
    use calc_func::value::{CellKey, IdAllocator};

    struct Sheets(Vec<(&'static str, SheetId)>);

    impl ReferenceCatalog for Sheets {
        fn sheet_named(&self, name: &str) -> Option<SheetId> {
            self.0
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(name))
                .map(|(_, sheet)| *sheet)
        }
    }

    fn fixture() -> (Sheets, SheetId, SheetId, FormulaAnchor) {
        let mut ids: IdAllocator<SheetId> = IdAllocator::new();
        let one = ids.issue().unwrap();
        let two = ids.issue().unwrap();
        let anchor = FormulaAnchor::new(CellKey::new(one, 5, 3).unwrap());
        (
            Sheets(vec![("Sheet1", one), ("Sheet2", two)]),
            one,
            two,
            anchor,
        )
    }

    /// Each site as the source text it spans and its binding.
    fn bound(
        formula: &str,
        notation: ReferenceNotation,
    ) -> Vec<(String, Result<ReferenceOperand, BindError>)> {
        let (catalog, _, _, anchor) = fixture();
        let units: Vec<u16> = formula.encode_utf16().collect();
        bind_references(&parse_formula(formula), notation, anchor, &catalog)
            .into_iter()
            .map(|site| {
                let text = String::from_utf16(&units[site.span.start..site.span.end()]).unwrap();
                (text, site.binding)
            })
            .collect()
    }

    fn texts(formula: &str, notation: ReferenceNotation) -> Vec<String> {
        bound(formula, notation)
            .into_iter()
            .map(|(text, _)| text)
            .collect()
    }

    /// Which `:` makes one reference and which is the range operator between references.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn ranges_bind_as_one_reference_only_between_endpoints_of_one_form_and_sheet() {
        use ReferenceNotation::{A1, R1C1};
        assert_eq!(texts("=B3:D7", A1), ["B3:D7"]);
        assert_eq!(texts("=Sheet2!B3:D7", A1), ["Sheet2!B3:D7"]);
        assert_eq!(texts("=Sheet2!B3:sheet2!D7", A1), ["Sheet2!B3:sheet2!D7"]);
        assert_eq!(
            texts("=Sheet1!B3:Sheet2!D7", A1),
            ["Sheet1!B3", "Sheet2!D7"]
        );
        assert_eq!(texts("=B3:'Sheet2'!D7", A1), ["B3", "'Sheet2'!D7"]);
        // The parser reads an unquoted `B3:Sheet2!` as a run of sheets from B3 to Sheet2, and the
        // workbook holds no sheet B3.
        assert_eq!(
            bound("=B3:Sheet2!D7", A1),
            [(
                "B3:Sheet2!D7".to_owned(),
                Err(BindError::UnknownSheet {
                    name: "B3".to_owned()
                })
            )]
        );
        assert_eq!(texts("=B3:D7:E9", A1), ["B3:D7", "E9"]);
        assert_eq!(texts("=B3:4", A1), ["B3"]);
        assert_eq!(texts("=(B3):D7", A1), ["B3", "D7"]);
        assert_eq!(texts("=SUM(B:B 2:2)", A1), ["B:B", "2:2"]);
        assert_eq!(texts("=@B3:B5+-C1%+D2#", A1), ["B3:B5", "C1", "D2"]);
        // `#` binds tighter than `:`, so B5# is the operand of the range operator here.
        assert_eq!(texts("=B3:B5#", A1), ["B3", "B5"]);
        // A lone row or column is a reference in R1C1 notation only. In A1 notation `B` is a name
        // and `3` a number; `$B` and `$3` are neither, and are refused.
        assert_eq!(
            bound("=B+3+$B+$3", A1),
            [
                ("$B".to_owned(), Err(BindError::LoneRowOrColumn)),
                ("$3".to_owned(), Err(BindError::LoneRowOrColumn)),
            ]
        );
        assert_eq!(
            texts("=R3+C+R[1]:R[2]+R1C1:R2C2", R1C1),
            ["R3", "C", "R[1]:R[2]", "R1C1:R2C2"]
        );
        assert_eq!(texts("=R1:C1", R1C1), ["R1", "C1"]);
        // Each notation reads only its own spellings.
        assert_eq!(texts("=R1C1+B3", A1), ["B3"]);
        assert_eq!(texts("=R1C1+B3", R1C1), ["R1C1"]);
        // Names, sheet-qualified names and calls are not grid references. A structured reference
        // is a site of its own, refused here by a catalogue that carries no tables (L3).
        assert_eq!(
            texts(
                "=TaxRate+Sheet2!TaxRate+Sales2023+XFE1+XFE:XFE+Table1[Col]+TRUE+SUM(B3)",
                A1
            ),
            ["Table1[Col]", "B3"]
        );
        assert_eq!(
            bound("=Table1[Col]", A1),
            [("Table1[Col]".to_owned(), Err(BindError::TablesNotModelled))]
        );
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn qualifiers_and_refusals() {
        let (_, _, two, _) = fixture();
        let cell = ReferenceShape::Cell(CellRef {
            row: AxisRef::Relative(-2),
            column: AxisRef::Relative(-1),
        });
        let sites = bound(
            "=Sheet2!B3+'Sheet2'!B3+#REF!B3+Nope!B3+[1]Sheet2!B3+'C:\\[B.xlsx]S'!B3+Sheet1:Sheet2!B3+'S 1:S 2'!B3+$XFE$1+0:3+#REF!B3:C4",
            ReferenceNotation::A1,
        );
        let expected = [
            (
                "Sheet2!B3",
                Ok(ReferenceOperand {
                    sheet: SheetTarget::Sheet(two),
                    shape: cell,
                }),
            ),
            (
                "'Sheet2'!B3",
                Ok(ReferenceOperand {
                    sheet: SheetTarget::Sheet(two),
                    shape: cell,
                }),
            ),
            (
                "#REF!B3",
                Ok(ReferenceOperand {
                    sheet: SheetTarget::Deleted,
                    shape: cell,
                }),
            ),
            (
                "Nope!B3",
                Err(BindError::UnknownSheet {
                    name: "Nope".to_owned(),
                }),
            ),
            ("[1]Sheet2!B3", Err(BindError::ExternalWorkbook)),
            ("'C:\\[B.xlsx]S'!B3", Err(BindError::ExternalWorkbook)),
            ("Sheet1:Sheet2!B3", Err(BindError::SheetSpan)),
            (
                "'S 1:S 2'!B3",
                Err(BindError::UnknownSheet {
                    name: "S 1".to_owned(),
                }),
            ),
            ("$XFE$1", Err(BindError::OutsideGrid)),
            ("0:3", Err(BindError::OutsideGrid)),
            (
                "#REF!B3:C4",
                Ok(ReferenceOperand {
                    sheet: SheetTarget::Deleted,
                    shape: ReferenceShape::Area {
                        first: CellRef {
                            row: AxisRef::Relative(-2),
                            column: AxisRef::Relative(-1),
                        },
                        last: CellRef {
                            row: AxisRef::Relative(-1),
                            column: AxisRef::Relative(0),
                        },
                    },
                }),
            ),
        ];
        assert_eq!(sites.len(), expected.len());
        for ((text, binding), (expected_text, expected_binding)) in sites.iter().zip(expected) {
            assert_eq!(text, expected_text);
            assert_eq!(binding, &expected_binding, "{text}");
        }
    }

    /// The corners of an area and the ends of a row or column run are ordered per axis without
    /// the anchor (C5): two of a kind by position, an absolute and a relative one absolute first,
    /// even where the relative one lies first at C5.
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    fn endpoints_are_ordered_per_axis_without_the_anchor() {
        let shape_of = |formula: &str| match &bound(formula, ReferenceNotation::A1)[..] {
            [(_, Ok(operand))] => operand.shape,
            other => panic!("{formula}: {other:?}"),
        };
        assert_eq!(shape_of("=D7:B3"), shape_of("=B3:D7"));
        assert_eq!(shape_of("=B7:D3"), shape_of("=B3:D7"));
        assert_eq!(shape_of("=$D$7:B3"), shape_of("=B3:$D$7"));
        assert_eq!(
            shape_of("=B3:$D$7"),
            ReferenceShape::Area {
                first: CellRef {
                    row: AxisRef::Absolute(7),
                    column: AxisRef::Absolute(4)
                },
                last: CellRef {
                    row: AxisRef::Relative(-2),
                    column: AxisRef::Relative(-1)
                },
            }
        );
        assert_eq!(
            shape_of("=$B5:A$1"),
            ReferenceShape::Area {
                first: CellRef {
                    row: AxisRef::Absolute(1),
                    column: AxisRef::Absolute(2)
                },
                last: CellRef {
                    row: AxisRef::Relative(0),
                    column: AxisRef::Relative(-2)
                },
            }
        );
        assert_eq!(
            shape_of("=C:A"),
            ReferenceShape::Columns {
                first: AxisRef::Relative(-2),
                last: AxisRef::Relative(0)
            }
        );
        assert_eq!(
            shape_of("=9:$2"),
            ReferenceShape::Rows {
                first: AxisRef::Absolute(2),
                last: AxisRef::Relative(4)
            }
        );
    }
}
