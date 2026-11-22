//! The work package that owns a worksheet function, for the triage of a refusal that names a
//! function but no owner (bead `dc-vcs`).
//!
//! CalcFml's `CompileError::Unsupported` names the bead that owns the construct it refuses, so the
//! triage takes that owner as it is. A kernel's `FunctionFault::NotImplemented`, a function the
//! catalogue does not hold (`CompileError::UnknownFunction`) and the other refusals of a catalogued
//! function name no owner. For those this table gives the work package whose scope holds the
//! function, by Excel's function category. The bulk-port packages F1, F2, F3, L1 and L2
//! (`docs/plan/BULK_PORT_PLAN.md` section 4) are closed, and their gaps passed to these owners:
//!
//! - F1's categories, exactness `dc-j9k`: math and trigonometry, statistics (with the
//!   compatibility functions), engineering, and the arithmetic operators.
//! - F2's categories, exactness `dc-uar`: text, date and time, information, logical, lookup and
//!   reference, database, web and cube functions, and the concatenation and comparison operators.
//! - F3's categories, exactness `dc-j9k`: financial functions, the dynamic-array functions and the
//!   LAMBDA helpers.
//! - L1's, the language remainder `dc-10y` (after L3 `dc-m3u`): LET and LAMBDA, which the plan
//!   puts in the formula language.
//! - L2's, the language remainder `dc-10y`: INDIRECT and OFFSET, whose dynamic dependencies the
//!   plan puts in the engine.
//!
//! The table is this repository's reading of the plan, not a statement the refusal makes, and a
//! name in no list (a user-defined or add-in function, or a name Excel does not know) is owned by
//! none: [`UNASSIGNED`].

/// The owner of a function in none of the lists.
pub(crate) const UNASSIGNED: &str = "no package (a name in no function category)";

const F1: &str = "exactness dc-j9k";
const F2: &str = "exactness dc-uar";
const F3: &str = "exactness dc-j9k";
const REGEX: &str = "exactness dc-zmk.12";
const L1: &str = "language dc-10y";
const L2: &str = "language dc-10y";

use calc_catalogue::{FunctionId, operators, ported, unheld};

fn owner_of(id: FunctionId) -> &'static str {
    match id {
        calc_catalogue::offset::OFFSET | calc_catalogue::reference_text::INDIRECT => return L2,
        unheld::RTD_ID | unheld::DETECTLANGUAGE_ID | unheld::TRANSLATE_ID => return F2,
        _ => {}
    }
    match id.family() {
        operators::arithmetic::CODE
        | calc_catalogue::aggregate::CODE
        | calc_catalogue::rounding::CODE
        | calc_catalogue::conditional::CODE
        | ported::math::CODE
        | ported::rounding_more::CODE
        | ported::subtotal::CODE
        | ported::statistics::CODE
        | ported::distributions::CODE
        | ported::engineering::CODE
        | ported::random::CODE => F1,
        operators::comparison::CODE
        | operators::text::CODE
        | calc_catalogue::logical::CODE
        | calc_catalogue::logical_more::CODE
        | calc_catalogue::error_fallback::CODE
        | calc_catalogue::lookup::CODE
        | calc_catalogue::information::CODE
        | ported::text_functions::CODE
        | ported::text_format::CODE
        | ported::dates::CODE
        | ported::host_facts::CODE
        | ported::database::CODE
        | ported::cube_functions::CODE
        | ported::fetching::CODE
        | ported::importing::CODE => F2,
        calc_catalogue::offset::CODE
        | calc_catalogue::group_pivot::CODE
        | ported::financial::CODE
        | ported::bonds::CODE
        | ported::arrays::CODE
        | ported::lambda_helpers::CODE
        | ported::grouping::CODE => F3,
        _ => UNASSIGNED,
    }
}

/// The prefixes a file stores before a function's name (`_xlfn._xlws.SORT`); a user-defined
/// function's `_xludf.` is not one of them, so such a name stays unassigned.
const FILE_PREFIXES: [&str; 2] = [
    calc_catalogue::FUTURE_FUNCTION_PREFIX,
    calc_catalogue::WORKSHEET_FUNCTION_PREFIX,
];

/// The work package that owns the function or operator spelled `spelling` (any case, with or
/// without the file prefixes), or [`UNASSIGNED`].
pub(crate) fn function_owner(spelling: &str) -> &'static str {
    let mut name = spelling.trim().to_ascii_uppercase();
    while let Some(rest) = FILE_PREFIXES
        .iter()
        .find_map(|prefix| calc_catalogue::strip_ascii_prefix(&name, prefix))
    {
        name = rest.to_owned();
    }
    if calc_catalogue::binding_form(&name).is_some() {
        return L1;
    }
    if matches!(name.as_str(), "REGEXTEST" | "REGEXEXTRACT" | "REGEXREPLACE") {
        return REGEX;
    }
    let catalogue = calc_func::catalogue();
    [
        calc_func::Notation::Call,
        calc_func::Notation::Infix,
        calc_func::Notation::Prefix,
        calc_func::Notation::Postfix,
    ]
    .into_iter()
    .find_map(|notation| catalogue.resolve_surface(notation, &name).ok())
    .map_or(UNASSIGNED, |entry| owner_of(entry.id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_map_to_the_package_of_their_category() {
        for (spelling, owner) in [
            ("SUM", F1),
            ("norm.s.dist", F1),
            ("IMAGINARY", F1),
            ("+", F1),
            ("NOT", F2),
            ("VLOOKUP", F2),
            ("_xlfn.XLOOKUP", F2),
            ("_xlfn.REGEXEXTRACT", REGEX),
            ("&", F2),
            ("PMT", F3),
            ("_xlfn._xlws.SORT", F3),
            ("LAMBDA", L1),
            ("INDIRECT", L2),
            ("OFFSET", L2),
            ("_xludf.MYFUNC", UNASSIGNED),
            ("NOSUCHFUNCTION", UNASSIGNED),
        ] {
            assert_eq!(function_owner(spelling), owner, "{spelling}");
        }
    }

    /// Every function the catalogue holds has an owner.
    #[test]
    fn every_catalogued_function_has_an_owner() {
        let unowned: Vec<String> = calc_func::catalogue()
            .targets()
            .map(|target| target.spelling().to_owned())
            .filter(|spelling| function_owner(spelling) == UNASSIGNED)
            .collect();
        assert!(unowned.is_empty(), "{unowned:?}");
    }
}
