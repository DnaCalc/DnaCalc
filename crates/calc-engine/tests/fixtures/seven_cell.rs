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

/// The P03 contract control inputs: reading the Range can be provisional; merely passing
/// it does not request its values. Shared by Host and the actual worker smoke route.
pub const P03_VBA_SOURCE: &str = "Private held As Range\nPrivate calls As Long\nPrivate emptyReads As Long\nPublic Function LateRange(ByVal r As Range) As Double\n calls=calls+1\n Dim v As Variant\n v=r.Value2\n If IsEmpty(v) Then emptyReads=emptyReads+1\n LateRange=v\nEnd Function\nPublic Function UnusedRange(ByVal r As Range) As Long\n calls=calls+1\n UnusedRange=1\nEnd Function\nPublic Function LateDouble(ByVal v As Double) As Double\n calls=calls+1\n LateDouble=v\nEnd Function\nPublic Function EntryStats() As Long\n EntryStats=calls*10+emptyReads\nEnd Function\nPublic Function HoldRange(ByVal r As Range) As Long\n Set held=r\n HoldRange=1\nEnd Function\nPublic Function ReadHeld() As Double\n calls=calls+1\n Dim v As Variant\n v=held.Value2\n If IsEmpty(v) Then emptyReads=emptyReads+1\n ReadHeld=v\nEnd Function\n";
pub const P03_RANGE_CASES: &[(&str, &str, Option<&str>, f64, f64, usize, Option<&str>)] = &[
    (
        "=LateRange(INDIRECT(\"D1\"))",
        "10+1",
        None,
        11.0,
        21.0,
        1,
        None,
    ),
    (
        "=UnusedRange(INDIRECT(\"D1\"))",
        "10+1",
        None,
        1.0,
        10.0,
        0,
        None,
    ),
    ("=LateDouble(D1+0)", "10+1", None, 11.0, 10.0, 0, None),
    (
        "=LateRange(INDIRECT(\"D2\"))",
        "SEQUENCE(2,1,F1)",
        Some("SEQUENCE(2,1,F1)"),
        21.0,
        20.0,
        1,
        None,
    ),
    ("=ReadHeld()", "10+1", None, 11.0, 21.0, 1, Some("D1")),
    ("=ReadHeld()", "10+1", None, 0.0, 11.0, 0, Some("B1")),
    ("=ReadHeld()", "C1+1", None, 0.0, 11.0, 0, Some("D1")),
];
