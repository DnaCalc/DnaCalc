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
const FILE_PREFIXES: [(&str, FilePrefix); 74] = [
    ("ABS", FilePrefix::Bare),
    ("AND", FilePrefix::Bare),
    ("AREAS", FilePrefix::Bare),
    ("AVERAGE", FilePrefix::Bare),
    ("AVERAGEA", FilePrefix::Bare),
    ("AVERAGEIF", FilePrefix::Bare),
    ("AVERAGEIFS", FilePrefix::Bare),
    ("CEILING", FilePrefix::Bare),
    ("CHOOSE", FilePrefix::Bare),
    ("COLUMN", FilePrefix::Bare),
    ("COLUMNS", FilePrefix::Bare),
    ("COUNT", FilePrefix::Bare),
    ("COUNTA", FilePrefix::Bare),
    ("COUNTBLANK", FilePrefix::Bare),
    ("COUNTIF", FilePrefix::Bare),
    ("COUNTIFS", FilePrefix::Bare),
    ("ERROR.TYPE", FilePrefix::Bare),
    ("EVEN", FilePrefix::Bare),
    ("FALSE", FilePrefix::Bare),
    ("FLOOR", FilePrefix::Bare),
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
    ("ISREF", FilePrefix::Bare),
    ("ISTEXT", FilePrefix::Bare),
    ("LOOKUP", FilePrefix::Bare),
    ("MATCH", FilePrefix::Bare),
    ("MAX", FilePrefix::Bare),
    ("MAXA", FilePrefix::Bare),
    ("MAXIFS", FilePrefix::Xlfn),
    ("MIN", FilePrefix::Bare),
    ("MINA", FilePrefix::Bare),
    ("MINIFS", FilePrefix::Xlfn),
    ("MOD", FilePrefix::Bare),
    ("MROUND", FilePrefix::Bare),
    ("N", FilePrefix::Bare),
    ("NA", FilePrefix::Bare),
    ("NOT", FilePrefix::Bare),
    ("ODD", FilePrefix::Bare),
    ("OR", FilePrefix::Bare),
    ("PRODUCT", FilePrefix::Bare),
    ("QUOTIENT", FilePrefix::Bare),
    ("ROUND", FilePrefix::Bare),
    ("ROUNDDOWN", FilePrefix::Bare),
    ("ROUNDUP", FilePrefix::Bare),
    ("ROW", FilePrefix::Bare),
    ("ROWS", FilePrefix::Bare),
    ("SIGN", FilePrefix::Bare),
    ("SUM", FilePrefix::Bare),
    ("SUMIF", FilePrefix::Bare),
    ("SUMIFS", FilePrefix::Bare),
    ("SUMPRODUCT", FilePrefix::Bare),
    ("SUMSQ", FilePrefix::Bare),
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
