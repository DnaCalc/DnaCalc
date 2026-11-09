//! IMPORTCSV and IMPORTTEXT as the catalogue holds them: what Excel gives when the file cannot be
//! read (#VALUE!), for a host with no [`crate::ImportRoute`]. A host with a route binds these
//! names to [`crate::ImportFunction`] instead; the rules and their measurements are in
//! [`crate::import`].

use super::kit::fail;
use crate::value::WorksheetErrorCode;

use calc_catalogue::ported::importing::{CODE, ENTRIES as DECLARATIONS};

ported_shared_family! {
    code: CODE;
    declarations: DECLARATIONS;
    1 importcsv_fn |s, a, sv| { fail(WorksheetErrorCode::Value) }
    2 importtext_fn |s, a, sv| { fail(WorksheetErrorCode::Value) }
}
