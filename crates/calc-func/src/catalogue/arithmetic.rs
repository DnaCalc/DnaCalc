//! The arithmetic family fragment: the arithmetic operators. Metadata only; no kernel yet.

use super::{CatalogueEntry, Family, FunctionId, Notation, Parameter};

const CODE: u16 = 1;

const NUMBERS: &[Parameter] = &[Parameter::value("number1"), Parameter::value("number2")];
const NUMBER: &[Parameter] = &[Parameter::value("number")];

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[
        CatalogueEntry::operator(FunctionId::new(CODE, 1), "+", Notation::Infix, NUMBERS),
        CatalogueEntry::operator(FunctionId::new(CODE, 2), "-", Notation::Infix, NUMBERS),
        CatalogueEntry::operator(FunctionId::new(CODE, 3), "*", Notation::Infix, NUMBERS),
        CatalogueEntry::operator(FunctionId::new(CODE, 4), "/", Notation::Infix, NUMBERS),
        CatalogueEntry::operator(FunctionId::new(CODE, 5), "^", Notation::Infix, NUMBERS),
        CatalogueEntry::operator(FunctionId::new(CODE, 6), "-", Notation::Prefix, NUMBER),
        CatalogueEntry::operator(FunctionId::new(CODE, 7), "+", Notation::Prefix, NUMBER),
        CatalogueEntry::operator(FunctionId::new(CODE, 8), "%", Notation::Postfix, NUMBER),
    ],
};
