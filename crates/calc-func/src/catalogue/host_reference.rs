//! The information and reference functions that read host facts or build references, ported
//! from OxFunc (functions/ at 6fca3518): cell.rs, info_fn.rs, reference_metadata_family.rs
//! (ADDRESS, FORMULATEXT, SHEET, SHEETS) and address_name_classes.rs; and GETPIVOTDATA and
//! TRANSPOSE. A fact the host does not supply ([`crate::HostFacts`]) refuses the call as not
//! modelled.

use crate::kernel::{Site, Step};
use crate::value::CalcValue;

const CODE: u16 = 204;

fn refused(site: &Site<'_>) -> Step<CalcValue> {
    Err(site.not_modelled(None))
}

ported_family! {
    code: CODE;
    1 cell_fn "CELL" (1, 2) [V "info_type", F "reference"] |s, a, sv| { refused(s) }
    2 info_fn "INFO" (1, 1) [V "type_text"] |s, a, sv| { refused(s) }
    3 sheet_fn "SHEET" (0, 1) [R "value"] |s, a, sv| { refused(s) }
    4 sheets_fn "SHEETS" (0, 1) [F "reference"] |s, a, sv| { refused(s) }
    5 formulatext_fn "FORMULATEXT" (1, 1) [F "reference"] |s, a, sv| { refused(s) }
    6 address_fn "ADDRESS" (2, 5) [V "row_num", V "column_num", V "abs_num", V "a1", V "sheet_text"] |s, a, sv| { refused(s) }
    7 getpivotdata_fn "GETPIVOTDATA" (2, 254) [V "data_field", F "pivot_table", V "field1", V "item1"] |s, a, sv| { refused(s) }
    8 transpose_fn "TRANSPOSE" (1, 1) [R "array"] |s, a, sv| { refused(s) }
}
