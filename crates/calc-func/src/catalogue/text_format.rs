//! The text functions that read or write number formats, dates and other text forms, ported from
//! OxFunc (functions/ at 6fca3518): value_fn.rs, text_fn.rs, fixed_fn.rs, dollar_fn.rs,
//! number_regex_translate_family.rs, valuetotext_fn.rs, array_text_split_family.rs,
//! text_delim_family.rs, text_compat_locale_family.rs (ASC and DBCS; Excel on Windows has no
//! JIS), web_text_xml_family.rs and hyperlink_fn.rs. A call whose result is not modelled is refused as not modelled.

use crate::kernel::{Site, Step};
use crate::value::CalcValue;

const CODE: u16 = 202;

/// The refusal of a call this family does not model.
fn refused(site: &Site<'_>) -> Step<CalcValue> {
    Err(site.not_modelled(None))
}

ported_family! {
    code: CODE;
    1 value_fn "VALUE" (1, 1) [V "text"] |s, a, sv| { refused(s) }
    2 text_fn "TEXT" (2, 2) [V "value", V "format_text"] |s, a, sv| { refused(s) }
    3 fixed_fn "FIXED" (1, 3) [V "number", V "decimals", V "no_commas"] |s, a, sv| { refused(s) }
    4 dollar_fn "DOLLAR" (1, 2) [V "number", V "decimals"] |s, a, sv| { refused(s) }
    5 numbervalue_fn "NUMBERVALUE" (1, 3) [V "text", V "decimal_separator", V "group_separator"] |s, a, sv| { refused(s) }
    6 valuetotext_fn "VALUETOTEXT" (1, 2) [V "value", V "format"] |s, a, sv| { refused(s) }
    7 arraytotext_fn "ARRAYTOTEXT" (1, 2) [R "array", V "format"] |s, a, sv| { refused(s) }
    8 bahttext_fn "BAHTTEXT" (1, 1) [V "number"] |s, a, sv| { refused(s) }
    9 asc_fn "ASC" (1, 1) [V "text"] |s, a, sv| { refused(s) }
    10 dbcs_fn "DBCS" (1, 1) [V "text"] |s, a, sv| { refused(s) }
    12 phonetic_fn "PHONETIC" (1, 1) [F "reference"] |s, a, sv| { refused(s) }
    13 textbefore_fn "TEXTBEFORE" (2, 6) [V "text", V "delimiter", V "instance_num", V "match_mode", V "match_end", V "if_not_found"] |s, a, sv| { refused(s) }
    14 textafter_fn "TEXTAFTER" (2, 6) [V "text", V "delimiter", V "instance_num", V "match_mode", V "match_end", V "if_not_found"] |s, a, sv| { refused(s) }
    15 textsplit_fn "TEXTSPLIT" (2, 6) [V "text", V "col_delimiter", V "row_delimiter", V "ignore_empty", V "match_mode", V "pad_with"] |s, a, sv| { refused(s) }
    16 regextest_fn "REGEXTEST" (2, 3) [V "text", V "pattern", V "case_sensitivity"] |s, a, sv| { refused(s) }
    17 regexextract_fn "REGEXEXTRACT" (2, 4) [V "text", V "pattern", V "return_mode", V "case_sensitivity"] |s, a, sv| { refused(s) }
    18 regexreplace_fn "REGEXREPLACE" (3, 5) [V "text", V "pattern", V "replacement", V "occurrence", V "case_sensitivity"] |s, a, sv| { refused(s) }
    19 encodeurl_fn "ENCODEURL" (1, 1) [V "text"] |s, a, sv| { refused(s) }
    20 filterxml_fn "FILTERXML" (2, 2) [V "xml", V "xpath"] |s, a, sv| { refused(s) }
    21 hyperlink_fn "HYPERLINK" (1, 2) [V "link_location", V "friendly_name"] |s, a, sv| { refused(s) }
}
