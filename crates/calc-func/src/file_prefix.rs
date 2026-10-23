//! The prefix a workbook file writes before a named function's name.
//!
//! A file stores a function Excel added after its 2007 file format with the prefix `_xlfn.`
//! (`_xlfn.MAXIFS(...)`), and Excel's load reads the same name written without it as an unknown
//! name (`#NAME?`), while a formula a user enters spells every function by its plain name. The
//! table below must give, for each named function the catalogue holds, the prefix Excel writes
//! before it: the acceptance case `dc_xrxb__prefix_metadata_matches_excel` compares every row with
//! the package Excel saves from the oracle workbook `tests/fixtures/dc_xrxb/oracle.xlsx` and fails
//! while a catalogued named function has no row.

use crate::catalogue::{FunctionCallTarget, Notation};

/// What a file writes before a named function's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilePrefix {
    /// Nothing: the file writes the plain name (`SUM(...)`).
    Bare,
    /// `_xlfn.`, before a function Excel added after its 2007 file format (`_xlfn.IFNA(...)`).
    Xlfn,
}

impl FilePrefix {
    /// The prefix as a file writes it before the function's name: empty for [`FilePrefix::Bare`].
    pub const fn text(self) -> &'static str {
        match self {
            Self::Bare => "",
            Self::Xlfn => "_xlfn.",
        }
    }
}

/// Each named function of the catalogue by its spelling, in byte order of the spelling (checked
/// below, since the lookup is a binary search), with the prefix a file writes before it. No
/// catalogued function is stored with `_xlfn._xlws.`, the prefix Excel writes before FILTER and
/// SORT, so there is no variant for it.
const FILE_PREFIXES: [(&str, FilePrefix); 128] = [
    ("ABS", FilePrefix::Bare),
    ("ACOS", FilePrefix::Bare),
    ("ACOSH", FilePrefix::Bare),
    ("ACOT", FilePrefix::Xlfn),
    ("ACOTH", FilePrefix::Xlfn),
    ("AND", FilePrefix::Bare),
    ("ARABIC", FilePrefix::Xlfn),
    ("AREAS", FilePrefix::Bare),
    ("ASIN", FilePrefix::Bare),
    ("ASINH", FilePrefix::Bare),
    ("ATAN", FilePrefix::Bare),
    ("ATAN2", FilePrefix::Bare),
    ("ATANH", FilePrefix::Bare),
    ("AVERAGE", FilePrefix::Bare),
    ("AVERAGEA", FilePrefix::Bare),
    ("AVERAGEIF", FilePrefix::Bare),
    ("AVERAGEIFS", FilePrefix::Bare),
    ("BASE", FilePrefix::Xlfn),
    ("CEILING", FilePrefix::Bare),
    ("CHOOSE", FilePrefix::Bare),
    ("COLUMN", FilePrefix::Bare),
    ("COLUMNS", FilePrefix::Bare),
    ("COMBIN", FilePrefix::Bare),
    ("COMBINA", FilePrefix::Xlfn),
    ("COS", FilePrefix::Bare),
    ("COSH", FilePrefix::Bare),
    ("COT", FilePrefix::Xlfn),
    ("COTH", FilePrefix::Xlfn),
    ("COUNT", FilePrefix::Bare),
    ("COUNTA", FilePrefix::Bare),
    ("COUNTBLANK", FilePrefix::Bare),
    ("COUNTIF", FilePrefix::Bare),
    ("COUNTIFS", FilePrefix::Bare),
    ("CSC", FilePrefix::Xlfn),
    ("CSCH", FilePrefix::Xlfn),
    ("DECIMAL", FilePrefix::Xlfn),
    ("DEGREES", FilePrefix::Bare),
    ("ERROR.TYPE", FilePrefix::Bare),
    ("EVEN", FilePrefix::Bare),
    ("EXP", FilePrefix::Bare),
    ("FACT", FilePrefix::Bare),
    ("FACTDOUBLE", FilePrefix::Bare),
    ("FALSE", FilePrefix::Bare),
    ("FLOOR", FilePrefix::Bare),
    ("GCD", FilePrefix::Bare),
    ("HLOOKUP", FilePrefix::Bare),
    ("IF", FilePrefix::Bare),
    ("IFERROR", FilePrefix::Bare),
    ("IFNA", FilePrefix::Xlfn),
    ("INDEX", FilePrefix::Bare),
    ("INDIRECT", FilePrefix::Bare),
    ("INT", FilePrefix::Bare),
    ("ISBLANK", FilePrefix::Bare),
    ("ISERR", FilePrefix::Bare),
    ("ISERROR", FilePrefix::Bare),
    ("ISEVEN", FilePrefix::Bare),
    ("ISFORMULA", FilePrefix::Xlfn),
    ("ISLOGICAL", FilePrefix::Bare),
    ("ISNA", FilePrefix::Bare),
    ("ISNONTEXT", FilePrefix::Bare),
    ("ISNUMBER", FilePrefix::Bare),
    ("ISODD", FilePrefix::Bare),
    ("ISOMITTED", FilePrefix::Xlfn),
    ("ISREF", FilePrefix::Bare),
    ("ISTEXT", FilePrefix::Bare),
    ("LCM", FilePrefix::Bare),
    ("LN", FilePrefix::Bare),
    ("LOG", FilePrefix::Bare),
    ("LOG10", FilePrefix::Bare),
    ("LOOKUP", FilePrefix::Bare),
    ("MATCH", FilePrefix::Bare),
    ("MAX", FilePrefix::Bare),
    ("MAXA", FilePrefix::Bare),
    ("MAXIFS", FilePrefix::Xlfn),
    ("MDETERM", FilePrefix::Bare),
    ("MIN", FilePrefix::Bare),
    ("MINA", FilePrefix::Bare),
    ("MINIFS", FilePrefix::Xlfn),
    ("MINVERSE", FilePrefix::Bare),
    ("MMULT", FilePrefix::Bare),
    ("MOD", FilePrefix::Bare),
    ("MROUND", FilePrefix::Bare),
    ("MULTINOMIAL", FilePrefix::Bare),
    ("MUNIT", FilePrefix::Xlfn),
    ("N", FilePrefix::Bare),
    ("NA", FilePrefix::Bare),
    ("NOT", FilePrefix::Bare),
    ("ODD", FilePrefix::Bare),
    ("OFFSET", FilePrefix::Bare),
    ("OR", FilePrefix::Bare),
    ("PERMUT", FilePrefix::Bare),
    ("PERMUTATIONA", FilePrefix::Xlfn),
    ("PI", FilePrefix::Bare),
    ("POWER", FilePrefix::Bare),
    ("PRODUCT", FilePrefix::Bare),
    ("QUOTIENT", FilePrefix::Bare),
    ("RADIANS", FilePrefix::Bare),
    ("ROMAN", FilePrefix::Bare),
    ("ROUND", FilePrefix::Bare),
    ("ROUNDDOWN", FilePrefix::Bare),
    ("ROUNDUP", FilePrefix::Bare),
    ("ROW", FilePrefix::Bare),
    ("ROWS", FilePrefix::Bare),
    ("SEC", FilePrefix::Xlfn),
    ("SECH", FilePrefix::Xlfn),
    ("SERIESSUM", FilePrefix::Bare),
    ("SIGN", FilePrefix::Bare),
    ("SIN", FilePrefix::Bare),
    ("SINH", FilePrefix::Bare),
    ("SQRT", FilePrefix::Bare),
    ("SQRTPI", FilePrefix::Bare),
    ("SUM", FilePrefix::Bare),
    ("SUMIF", FilePrefix::Bare),
    ("SUMIFS", FilePrefix::Bare),
    ("SUMPRODUCT", FilePrefix::Bare),
    ("SUMSQ", FilePrefix::Bare),
    ("SUMX2MY2", FilePrefix::Bare),
    ("SUMX2PY2", FilePrefix::Bare),
    ("SUMXMY2", FilePrefix::Bare),
    ("TAN", FilePrefix::Bare),
    ("TANH", FilePrefix::Bare),
    ("TRUE", FilePrefix::Bare),
    ("TRUNC", FilePrefix::Bare),
    ("TYPE", FilePrefix::Bare),
    ("VLOOKUP", FilePrefix::Bare),
    ("XLOOKUP", FilePrefix::Xlfn),
    ("XMATCH", FilePrefix::Xlfn),
    ("XOR", FilePrefix::Xlfn),
];

const _: () = {
    let mut i = 1;
    while i < FILE_PREFIXES.len() {
        let (left, right) = (
            FILE_PREFIXES[i - 1].0.as_bytes(),
            FILE_PREFIXES[i].0.as_bytes(),
        );
        let mut b = 0;
        while b < left.len() && b < right.len() && left[b] == right[b] {
            b += 1;
        }
        let ordered = if b < left.len() && b < right.len() {
            left[b] < right[b]
        } else {
            left.len() < right.len()
        };
        if !ordered {
            panic!("the file prefix table is not in strictly increasing byte order of spelling");
        }
        i += 1;
    }
};

impl FunctionCallTarget<'_> {
    /// The prefix a file writes before this named function's name, or `None` for an operator and
    /// for a named function whose prefix has not been measured (one the catalogue gained after the
    /// table above was measured).
    pub fn file_prefix(self) -> Option<FilePrefix> {
        if self.notation() != Notation::Call {
            return None;
        }
        let spelling = self.spelling();
        FILE_PREFIXES
            .binary_search_by(|(name, _)| name.as_bytes().cmp(spelling.as_bytes()))
            .ok()
            .map(|at| FILE_PREFIXES[at].1)
    }
}
