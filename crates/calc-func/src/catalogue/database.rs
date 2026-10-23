//! The database functions family fragment ported from OxFunc (functions/database_family.rs at
//! 6fca3518): DAVERAGE, DCOUNT, DCOUNTA, DGET, DMAX, DMIN, DPRODUCT, DSTDEV, DSTDEVP, DSUM, DVAR
//! and DVARP over a database range and a criteria range.

use crate::kernel::{Site, Step};
use crate::value::CalcValue;

const CODE: u16 = 205;

fn refused(site: &Site<'_>) -> Step<CalcValue> {
    Err(site.not_modelled(None))
}

ported_family! {
    code: CODE;
    1 daverage "DAVERAGE" (3, 3) [R "database", V "field", R "criteria"] |s, a, sv| { refused(s) }
    2 dcount "DCOUNT" (3, 3) [R "database", V "field", R "criteria"] |s, a, sv| { refused(s) }
    3 dcounta "DCOUNTA" (3, 3) [R "database", V "field", R "criteria"] |s, a, sv| { refused(s) }
    4 dget "DGET" (3, 3) [R "database", V "field", R "criteria"] |s, a, sv| { refused(s) }
    5 dmax "DMAX" (3, 3) [R "database", V "field", R "criteria"] |s, a, sv| { refused(s) }
    6 dmin "DMIN" (3, 3) [R "database", V "field", R "criteria"] |s, a, sv| { refused(s) }
    7 dproduct "DPRODUCT" (3, 3) [R "database", V "field", R "criteria"] |s, a, sv| { refused(s) }
    8 dstdev "DSTDEV" (3, 3) [R "database", V "field", R "criteria"] |s, a, sv| { refused(s) }
    9 dstdevp "DSTDEVP" (3, 3) [R "database", V "field", R "criteria"] |s, a, sv| { refused(s) }
    10 dsum "DSUM" (3, 3) [R "database", V "field", R "criteria"] |s, a, sv| { refused(s) }
    11 dvar "DVAR" (3, 3) [R "database", V "field", R "criteria"] |s, a, sv| { refused(s) }
    12 dvarp "DVARP" (3, 3) [R "database", V "field", R "criteria"] |s, a, sv| { refused(s) }
}
