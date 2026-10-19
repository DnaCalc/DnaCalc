//! The text family fragment: the concatenation operator. Metadata only; no kernel yet.

use super::{CatalogueEntry, Family, FunctionId, Notation, Parameter};

const CODE: u16 = 3;

const TEXTS: &[Parameter] = &[Parameter::value("text1"), Parameter::value("text2")];

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[CatalogueEntry::operator(
        FunctionId::new(CODE, 1),
        "&",
        Notation::Infix,
        TEXTS,
    )],
};
