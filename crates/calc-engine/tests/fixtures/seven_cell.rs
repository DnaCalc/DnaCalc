//! The seven-cell SliceV1 workbook, shared by engine and application-route tests.
//! These are hand-authored inputs, not Excel observations. D1 requires the P01/P03 callback.

/// Row, column, entered text, and the formula spelling stored in an Excel package.
pub const CELLS: &[(u32, u32, &str, Option<&str>)] = &[
    (1, 1, "1", None),
    (1, 2, "=A1+1", Some("A1+1")),
    (1, 3, "=B1*2", Some("B1*2")),
    (1, 4, "=PeekB1()", Some("PeekB1()")),
    (1, 5, "=SEQUENCE(F1)", Some("_xlfn.SEQUENCE(F1)")),
    (1, 6, "1", None),
    (10, 7, "=E10+1", Some("E10+1")),
];
pub const PURE_SCOPE: &[(u32, u32)] = &[(1, 2), (1, 3), (1, 5), (10, 7)];
