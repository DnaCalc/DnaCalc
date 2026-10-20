//! Binding the grid references of a parsed formula against a catalogue and an anchor.
//!
//! A reference here is an identifier or number spelled as a cell, row or column of the formula's
//! notation (see the `notation` module), unqualified or qualified by one sheet, and a range of two
//! such endpoints of one form on one sheet (`B3:D7`, `2:4`, `B:D`, `Sheet2!B3:C4`,
//! `Sheet2!B3:Sheet2!C4`). A lone row or column is a reference in R1C1 notation (`R3`, `C`) and
//! not in A1 notation, where `3` is a number and `B` a name. Everything else is left to the
//! formula's own operators and to later binding: a `:` between other operands, or between
//! endpoints of different forms or sheets, is the range operator over separately bound
//! references; names, tables and calls are not references here.
//!
//! A qualifier belongs to its whole operand. A qualified operand is bound with its qualifier or
//! refused; the walk never enters it, so nothing inside it can bind as an unqualified reference.
//! The same holds for syntax the parser refused.

use super::notation::{self, Spelled};
use super::{
    AxisRef, CellRef, FormulaAnchor, ReferenceNotation, ReferenceOperand, ReferenceShape,
    SheetTarget,
};
use crate::syntax::{SyntaxChild, SyntaxKind, SyntaxNode, SyntaxTree, TextSpan, Token, TokenKind};
use calc_func::value::SheetId;

/// The workbook identities a formula's references are bound against. Binding asks it only for
/// sheets, by the name a qualifier spells, never for reference text.
pub trait ReferenceCatalog {
    /// The sheet of the workbook named `name`, or `None` when there is none. `name` is the
    /// qualifier's sheet name as written, without quotes or `!` (`It's` for `'It''s'!A1`); the
    /// catalogue matches it as Excel matches sheet names, regardless of case.
    fn sheet_named(&self, name: &str) -> Option<SheetId>;
}

/// One reference of a formula, or one stretch of syntax the parser refused, and what it bound to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceSite {
    /// The site's source, in UTF-16 code units of the formula text.
    pub span: TextSpan,
    pub binding: Result<ReferenceOperand, BindError>,
}

/// Why a site was refused at binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindError {
    /// The reference names a cell, row or column outside the grid wherever it is placed:
    /// `$XFE$1`, `1:1048577`, `R0C1`, `R[1048576]C`.
    OutsideGrid,
    /// The catalogue has no sheet of this name.
    UnknownSheet { name: String },
    /// A qualifier naming another workbook (`[Book2.xlsx]Sheet1!A1`); its binding belongs to
    /// bead cfml-m3.5.
    ExternalWorkbook,
    /// A run of sheets (`Sheet1:Sheet3!A1`, `'Sheet 1:Sheet 3'!A1`); its binding belongs to bead
    /// cfml-m3.2.
    SheetSpan,
    /// A qualifier (one sheet, a run of sheets, another workbook or `#REF!`) before an operand
    /// that is neither a cell, row or column of the formula's notation nor a name:
    /// `Sheet2!(A1)`, `Sheet2!Sheet3!A1`, `#REF!SUM(A1)`, `Sheet2!3`, and
    /// `Sheet2!#REF!`, which Excel writes for a deleted cell and which this binding does not
    /// model. The site spans the qualifier and its operand, and the catalogue is not asked.
    QualifiedNonReference,
    /// Syntax the parser refused with an unsupported-syntax diagnostic, such as a trim reference
    /// (`Sheet1!A1.:B10`) or a `.` member selector (`(A1).Value`). The site spans the refused
    /// syntax, and no reference inside it is bound.
    UnsupportedSyntax,
}

/// Binds every grid reference of `tree`, read in `notation`, for a formula anchored at `anchor`;
/// the sites come in source order. A range of two endpoints of one form is one site.
///
/// A qualified operand is one site, bound to the sheet its qualifier names or refused: a
/// qualifier before anything but a reference endpoint is [`BindError::QualifiedNonReference`],
/// except a name (`Sheet2!TaxRate`), which gives no site and is left to name binding. Syntax the
/// parser refused is refused here as one [`BindError::UnsupportedSyntax`] site, whatever it
/// holds: the references inside it (`Sheet1!A1` and `B10` in `=SUM(Sheet1!A1.:B10)`) give no
/// sites of their own.
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
    Span,
}

struct Binder<'c> {
    notation: ReferenceNotation,
    anchor: FormulaAnchor,
    catalog: &'c dyn ReferenceCatalog,
    sites: Vec<ReferenceSite>,
}

impl Binder<'_> {
    fn walk(&mut self, node: &SyntaxNode) {
        let binding = match node.kind() {
            SyntaxKind::UnsupportedExpr => Some(Err(BindError::UnsupportedSyntax)),
            // Bound with the qualifier or refused, never entered: what lies inside a qualified
            // operand is not an unqualified reference.
            SyntaxKind::QualifiedReferenceExpr | SyntaxKind::SheetSpan3DReferenceExpr => {
                self.qualified(node)
            }
            SyntaxKind::IdentifierExpr | SyntaxKind::NumberLiteralExpr => {
                only_token(node).and_then(|token| self.lone(&Qualifier::Unqualified, token))
            }
            SyntaxKind::RangeExpr => match self.range(node) {
                Some(binding) => Some(binding),
                None => {
                    self.walk_children(node);
                    return;
                }
            },
            _ => {
                self.walk_children(node);
                return;
            }
        };
        if let Some(binding) = binding {
            self.sites.push(ReferenceSite {
                span: node.span(),
                binding,
            });
        }
    }

    fn walk_children(&mut self, node: &SyntaxNode) {
        for child in node.child_nodes() {
            self.walk(child);
        }
    }

    /// The binding of a qualified operand standing alone: the operand bound on the qualifier's
    /// sheet when it is a reference endpoint, `None` when it is a name, which name binding
    /// resolves, and [`BindError::QualifiedNonReference`] for anything else.
    fn qualified(&self, node: &SyntaxNode) -> Option<Result<ReferenceOperand, BindError>> {
        let Some((qualifier, token)) = endpoint(node) else {
            return Some(Err(BindError::QualifiedNonReference));
        };
        match self.lone(&qualifier, token) {
            Some(binding) => Some(binding),
            None if token.kind() == TokenKind::Identifier => None,
            None => Some(Err(BindError::QualifiedNonReference)),
        }
    }

    /// The binding of one endpoint standing alone; `None` when its text is no reference alone in
    /// the formula's notation.
    fn lone(
        &self,
        qualifier: &Qualifier,
        token: &Token,
    ) -> Option<Result<ReferenceOperand, BindError>> {
        let shape = match self.spell(token)? {
            Spelled::Cell(cell) => ReferenceShape::Cell(cell),
            // Alone, a row or a column is a reference only in R1C1 notation (`R3`, `C`).
            Spelled::Row(row) if self.notation == ReferenceNotation::R1C1 => ReferenceShape::Rows {
                first: row,
                last: row,
            },
            Spelled::Column(column) if self.notation == ReferenceNotation::R1C1 => {
                ReferenceShape::Columns {
                    first: column,
                    last: column,
                }
            }
            Spelled::OutsideGrid(form)
                if self.notation == ReferenceNotation::R1C1 || form == notation::Form::Cell =>
            {
                return Some(Err(BindError::OutsideGrid));
            }
            Spelled::Row(_) | Spelled::Column(_) | Spelled::OutsideGrid(_) => return None,
        };
        Some(
            self.sheet_target(qualifier)
                .map(|sheet| ReferenceOperand { sheet, shape }),
        )
    }

    /// `left:right` as one reference when both are endpoints of one form, and the right one is
    /// unqualified or names the left one's sheet; otherwise `None`, and each side binds alone.
    fn range(&self, node: &SyntaxNode) -> Option<Result<ReferenceOperand, BindError>> {
        let mut operands = node.child_nodes();
        let (left, right) = (operands.next()?, operands.next()?);
        let (left_qualifier, left_token) = endpoint(left)?;
        let (right_qualifier, right_token) = endpoint(right)?;
        let left = self.spell(left_token)?;
        let right = self.spell(right_token)?;
        if left.form() != right.form() {
            return None;
        }
        let one_sheet = matches!(
            (&left_qualifier, &right_qualifier),
            (_, Qualifier::Unqualified)
                | (Qualifier::External | Qualifier::Span, _)
                | (Qualifier::Deleted, Qualifier::Deleted)
                | (Qualifier::Sheet(_), Qualifier::Sheet(_))
        );
        if !one_sheet {
            return None;
        }
        let shape = match pair(left, right) {
            Ok(shape) => shape,
            Err(refused) => return Some(Err(refused)),
        };
        let sheet = match self.sheet_target(&left_qualifier) {
            Ok(sheet) => sheet,
            Err(refused) => return Some(Err(refused)),
        };
        if let Qualifier::Sheet(_) = right_qualifier {
            match self.sheet_target(&right_qualifier) {
                Err(refused) => return Some(Err(refused)),
                // Two sheets: the range operator between two references, not one area.
                Ok(other) if other != sheet => return None,
                Ok(_) => {}
            }
        }
        Some(Ok(ReferenceOperand { sheet, shape }))
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

    fn sheet_target(&self, qualifier: &Qualifier) -> Result<SheetTarget, BindError> {
        match qualifier {
            Qualifier::Unqualified => Ok(SheetTarget::Anchor),
            Qualifier::Deleted => Ok(SheetTarget::Deleted),
            Qualifier::External => Err(BindError::ExternalWorkbook),
            Qualifier::Span => Err(BindError::SheetSpan),
            Qualifier::Sheet(name) => self
                .catalog
                .sheet_named(name)
                .map(SheetTarget::Sheet)
                .ok_or_else(|| BindError::UnknownSheet { name: name.clone() }),
        }
    }
}

/// The shape between two endpoints of one form, each axis's pair in the order of [`ordered`].
fn pair(left: Spelled, right: Spelled) -> Result<ReferenceShape, BindError> {
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

// A node that can be a reference endpoint: an identifier or number, alone, after one sheet's
/// qualifier, or after a run of sheets. Gives its qualifier and the token to spell.
fn endpoint(node: &SyntaxNode) -> Option<(Qualifier, &Token)> {
    match node.kind() {
        SyntaxKind::IdentifierExpr | SyntaxKind::NumberLiteralExpr => {
            Some((Qualifier::Unqualified, only_token(node)?))
        }
        SyntaxKind::QualifiedReferenceExpr => {
            let target = only_token(node.child_nodes().next()?)?;
            let SyntaxChild::Token(qualifier) = node.children().first()? else {
                return None;
            };
            Some((qualifier_of(qualifier), target))
        }
        SyntaxKind::SheetSpan3DReferenceExpr => {
            Some((Qualifier::Span, only_token(node.child_nodes().next()?)?))
        }
        _ => None,
    }
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
    } else if name.contains(':') {
        Qualifier::Span
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
        // The parser reads an unquoted `B3:Sheet2!` as a run of sheets from B3 to Sheet2.
        assert_eq!(
            bound("=B3:Sheet2!D7", A1),
            [("B3:Sheet2!D7".to_owned(), Err(BindError::SheetSpan))]
        );
        assert_eq!(texts("=B3:D7:E9", A1), ["B3:D7", "E9"]);
        assert_eq!(texts("=B3:4", A1), ["B3"]);
        assert_eq!(texts("=(B3):D7", A1), ["B3", "D7"]);
        assert_eq!(texts("=SUM(B:B 2:2)", A1), ["B:B", "2:2"]);
        assert_eq!(texts("=@B3:B5+-C1%+D2#", A1), ["B3:B5", "C1", "D2"]);
        // `#` binds tighter than `:`, so B5# is the operand of the range operator here.
        assert_eq!(texts("=B3:B5#", A1), ["B3", "B5"]);
        // A lone row or column is a reference in R1C1 notation only.
        assert_eq!(texts("=B+3+$B+$3", A1), Vec::<String>::new());
        assert_eq!(
            texts("=R3+C+R[1]:R[2]+R1C1:R2C2", R1C1),
            ["R3", "C", "R[1]:R[2]", "R1C1:R2C2"]
        );
        assert_eq!(texts("=R1:C1", R1C1), ["R1", "C1"]);
        // Each notation reads only its own spellings.
        assert_eq!(texts("=R1C1+B3", A1), ["B3"]);
        assert_eq!(texts("=R1C1+B3", R1C1), ["R1C1"]);
        // Names, sheet-qualified names, tables and calls are not grid references.
        assert_eq!(
            texts(
                "=TaxRate+Sheet2!TaxRate+Sales2023+XFE1+XFE:XFE+Table1[Col]+TRUE+SUM(B3)",
                A1
            ),
            ["B3"]
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
            ("'S 1:S 2'!B3", Err(BindError::SheetSpan)),
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
