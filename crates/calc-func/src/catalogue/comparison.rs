//! The comparison family fragment: the comparison operators. Metadata only; no kernel yet.

use super::{CatalogueEntry, Family, FunctionId, Notation, Parameter};

const CODE: u16 = 2;

const VALUES: &[Parameter] = &[Parameter::value("value1"), Parameter::value("value2")];

pub(super) const FAMILY: Family = Family {
    code: CODE,
    entries: &[
        CatalogueEntry::operator(FunctionId::new(CODE, 1), "=", Notation::Infix, VALUES),
        CatalogueEntry::operator(FunctionId::new(CODE, 2), "<>", Notation::Infix, VALUES),
        CatalogueEntry::operator(FunctionId::new(CODE, 3), "<", Notation::Infix, VALUES),
        CatalogueEntry::operator(FunctionId::new(CODE, 4), "<=", Notation::Infix, VALUES),
        CatalogueEntry::operator(FunctionId::new(CODE, 5), ">", Notation::Infix, VALUES),
        CatalogueEntry::operator(FunctionId::new(CODE, 6), ">=", Notation::Infix, VALUES),
    ],
};
